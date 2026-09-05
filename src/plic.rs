//! RISC-V 平台级中断控制器（PLIC）。
//!
//! 实现优先级、待处理位、目标使能、阈值和 claim/complete 流程。
//! [`PlicLayout`] 默认采用 PLIC 1.0 的 MMIO 布局，也允许平台显式配置其他布局。

use crate::bus::MemDevice;
use crate::interrupt::InterruptLine;
use crate::trap::{Exception, InterruptCause, InterruptSet};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// PLIC 规范允许的最大中断源编号；编号 0 永远表示“没有中断”。
pub const MAX_INTERRUPT_SOURCES: u32 = 1023;

/// PLIC 寄存器区的地址布局。
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PlicLayout {
    pub size: u64,
    pub priority_base: u64,
    pub pending_base: u64,
    pub enable_base: u64,
    pub enable_context_stride: u64,
    pub context_base: u64,
    pub context_stride: u64,
}

impl PlicLayout {
    /// SiFive PLIC 和 QEMU `virt` 平台采用的寄存器布局。
    pub const SIFIVE: Self = Self {
        size: 0x0400_0000,
        priority_base: 0,
        pending_base: 0x1000,
        enable_base: 0x2000,
        enable_context_stride: 0x80,
        context_base: 0x20_0000,
        context_stride: 0x1000,
    };
}

impl Default for PlicLayout {
    fn default() -> Self {
        Self::SIFIVE
    }
}

/// PLIC 参数无法由所选寄存器布局表示。
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PlicError {
    InvalidSourceCount,
    InvalidMaximumPriority,
    InvalidContextInterrupt,
    DuplicateContextInterrupt,
    NoContexts,
    MisalignedLayout,
    LayoutTooSmall,
    AddressOverflow,
    UnknownSource(u32),
}

impl Display for PlicError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSourceCount => f.write_str("PLIC source count must be in 1..=1023"),
            Self::InvalidMaximumPriority => {
                f.write_str("PLIC maximum priority must be a non-zero all-ones WARL mask")
            }
            Self::InvalidContextInterrupt => {
                f.write_str("PLIC contexts must target a standard external interrupt")
            }
            Self::DuplicateContextInterrupt => {
                f.write_str("PLIC contexts must target distinct hart interrupt causes")
            }
            Self::NoContexts => f.write_str("PLIC must expose at least one target context"),
            Self::MisalignedLayout => {
                f.write_str("PLIC base, register offsets, strides, and size must be 32-bit aligned")
            }
            Self::LayoutTooSmall => f.write_str(
                "PLIC register layout cannot represent the configured sources or contexts",
            ),
            Self::AddressOverflow => f.write_str("PLIC MMIO range overflows u64"),
            Self::UnknownSource(source) => write!(f, "PLIC source {source} is not configured"),
        }
    }
}

impl Error for PlicError {}

/// 一个可配置源数和目标上下文的 PLIC。
pub struct Plic {
    base: u64,
    layout: PlicLayout,
    source_count: u32,
    maximum_priority: u32,
    context_interrupts: Vec<InterruptCause>,
    priorities: Vec<u32>,
    pending: Vec<bool>,
    enabled: Vec<Vec<bool>>,
    thresholds: Vec<u32>,
    source_lines: Vec<InterruptLine>,
    active_sources: Vec<usize>,
    pending_sources: BTreeSet<usize>,
    gateway_busy: Vec<bool>,
}

impl Plic {
    /// 创建 PLIC。`context_interrupts` 的顺序同时决定使能区和上下文区的索引。
    pub fn new(
        base: u64,
        layout: PlicLayout,
        source_count: u32,
        maximum_priority: u32,
        context_interrupts: Vec<InterruptCause>,
    ) -> Result<Self, PlicError> {
        if !(1..=MAX_INTERRUPT_SOURCES).contains(&source_count) {
            return Err(PlicError::InvalidSourceCount);
        }
        if maximum_priority == 0 || !maximum_priority.wrapping_add(1).is_power_of_two() {
            return Err(PlicError::InvalidMaximumPriority);
        }
        if context_interrupts.is_empty() {
            return Err(PlicError::NoContexts);
        }
        if context_interrupts.iter().any(|interrupt| {
            !matches!(
                interrupt,
                InterruptCause::SupervisorExternal | InterruptCause::MachineExternal
            )
        }) {
            return Err(PlicError::InvalidContextInterrupt);
        }
        if context_interrupts
            .iter()
            .enumerate()
            .any(|(index, interrupt)| context_interrupts[..index].contains(interrupt))
        {
            return Err(PlicError::DuplicateContextInterrupt);
        }
        if [
            base,
            layout.size,
            layout.priority_base,
            layout.pending_base,
            layout.enable_base,
            layout.enable_context_stride,
            layout.context_base,
            layout.context_stride,
        ]
        .into_iter()
        .any(|value| value & 3 != 0)
        {
            return Err(PlicError::MisalignedLayout);
        }
        base.checked_add(layout.size)
            .ok_or(PlicError::AddressOverflow)?;

        let source_slots = u64::from(source_count) + 1;
        let pending_words = source_slots.div_ceil(32);
        let context_count =
            u64::try_from(context_interrupts.len()).map_err(|_| PlicError::LayoutTooSmall)?;
        let priorities_end = layout
            .priority_base
            .checked_add(
                source_slots
                    .checked_mul(4)
                    .ok_or(PlicError::LayoutTooSmall)?,
            )
            .ok_or(PlicError::LayoutTooSmall)?;
        let pending_end = layout
            .pending_base
            .checked_add(
                pending_words
                    .checked_mul(4)
                    .ok_or(PlicError::LayoutTooSmall)?,
            )
            .ok_or(PlicError::LayoutTooSmall)?;
        let enable_words_size = pending_words
            .checked_mul(4)
            .ok_or(PlicError::LayoutTooSmall)?;
        let enables_end = layout
            .enable_base
            .checked_add(
                context_count
                    .checked_mul(layout.enable_context_stride)
                    .ok_or(PlicError::LayoutTooSmall)?,
            )
            .ok_or(PlicError::LayoutTooSmall)?;
        let contexts_end = layout
            .context_base
            .checked_add(
                context_count
                    .checked_mul(layout.context_stride)
                    .ok_or(PlicError::LayoutTooSmall)?,
            )
            .ok_or(PlicError::LayoutTooSmall)?;
        if priorities_end > layout.pending_base
            || pending_end > layout.enable_base
            || enable_words_size > layout.enable_context_stride
            || enables_end > layout.context_base
            || layout.context_stride < 8
            || contexts_end > layout.size
        {
            return Err(PlicError::LayoutTooSmall);
        }

        let slots = usize::try_from(source_slots).map_err(|_| PlicError::LayoutTooSmall)?;
        let contexts = context_interrupts.len();
        Ok(Self {
            base,
            layout,
            source_count,
            maximum_priority,
            context_interrupts,
            priorities: vec![0; slots],
            pending: vec![false; slots],
            enabled: vec![vec![false; slots]; contexts],
            thresholds: vec![0; contexts],
            source_lines: (0..slots).map(|_| InterruptLine::new()).collect(),
            active_sources: Vec::new(),
            pending_sources: BTreeSet::new(),
            gateway_busy: vec![false; slots],
        })
    }

    pub const fn base(&self) -> u64 {
        self.base
    }

    pub const fn size(&self) -> u64 {
        self.layout.size
    }

    pub const fn source_count(&self) -> u32 {
        self.source_count
    }

    /// 返回指定中断源的电平线。设备应保留克隆并在自身状态变化时更新它。
    pub fn source_line(&mut self, source: u32) -> Result<InterruptLine, PlicError> {
        if source == 0 || source > self.source_count {
            return Err(PlicError::UnknownSource(source));
        }
        let source = source as usize;
        if !self.active_sources.contains(&source) {
            self.active_sources.push(source);
        }
        Ok(self.source_lines[source].clone())
    }

    fn register_words(&self) -> usize {
        self.pending.len().div_ceil(32)
    }

    fn sync_gateways(&mut self) {
        for &source in &self.active_sources {
            if self.source_lines[source].is_asserted() && !self.gateway_busy[source] {
                self.pending[source] = true;
                self.pending_sources.insert(source);
                self.gateway_busy[source] = true;
            }
        }
    }

    fn eligible_source(&self, context: usize, threshold: u32) -> Option<usize> {
        self.pending_sources
            .iter()
            .copied()
            .filter(|source| self.enabled[context][*source] && self.priorities[*source] > threshold)
            .max_by(|left, right| {
                self.priorities[*left]
                    .cmp(&self.priorities[*right])
                    // 同优先级时编号较小者优先，因此反转编号比较。
                    .then_with(|| right.cmp(left))
            })
    }

    fn claim(&mut self, context: usize) -> u32 {
        self.sync_gateways();
        // 阈值只屏蔽通知；软件仍可通过 claim 轮询非零优先级的请求。
        let Some(source) = self.eligible_source(context, 0) else {
            return 0;
        };
        self.pending[source] = false;
        self.pending_sources.remove(&source);
        source as u32
    }

    fn complete(&mut self, context: usize, source: u32) {
        let source = source as usize;
        // PLIC 不跟踪 claim 的所有者，completion 只检查目标当前的使能位。
        if source == 0 || source >= self.pending.len() || !self.enabled[context][source] {
            return;
        }
        self.gateway_busy[source] = false;
        // 电平源在完成时仍为高电平，应立即形成下一次待处理请求。
        self.sync_gateways();
    }

    fn offset(&self, addr: u64) -> Option<u64> {
        let offset = addr.checked_sub(self.base)?;
        (offset < self.layout.size).then_some(offset)
    }

    fn pending_word(&self, word: usize) -> u32 {
        let first = word * 32;
        let mut value = 0u32;
        for bit in 0..32 {
            let source = first + bit;
            if source < self.pending.len() && self.pending[source] {
                value |= 1 << bit;
            }
        }
        value
    }

    fn enable_word(&self, context: usize, word: usize) -> u32 {
        let first = word * 32;
        let mut value = 0u32;
        for bit in 0..32 {
            let source = first + bit;
            if source < self.pending.len() && self.enabled[context][source] {
                value |= 1 << bit;
            }
        }
        value
    }

    fn write_enable_word(&mut self, context: usize, word: usize, value: u32) {
        let first = word * 32;
        for bit in 0..32 {
            let source = first + bit;
            if source > 0 && source < self.pending.len() {
                self.enabled[context][source] = value & (1 << bit) != 0;
            }
        }
    }

    fn decode_enable(&self, offset: u64) -> Option<(usize, usize)> {
        if !(self.layout.enable_base..self.layout.context_base).contains(&offset) {
            return None;
        }
        let relative = offset - self.layout.enable_base;
        let context = usize::try_from(relative / self.layout.enable_context_stride).ok()?;
        let within = relative % self.layout.enable_context_stride;
        let word = usize::try_from(within / 4).ok()?;
        (context < self.enabled.len() && word < self.register_words()).then_some((context, word))
    }

    fn decode_context(&self, offset: u64) -> Option<(usize, u64)> {
        let relative = offset.checked_sub(self.layout.context_base)?;
        let context = usize::try_from(relative / self.layout.context_stride).ok()?;
        let register = relative % self.layout.context_stride;
        (context < self.context_interrupts.len()).then_some((context, register))
    }
}

impl MemDevice for Plic {
    fn read(&mut self, addr: u64, size: usize) -> Result<u64, Exception> {
        if size != 4 || addr & 3 != 0 {
            return Err(Exception::LoadAccessFault(addr));
        }
        let offset = self.offset(addr).ok_or(Exception::LoadAccessFault(addr))?;
        self.sync_gateways();

        if offset >= self.layout.priority_base && offset < self.layout.pending_base {
            let source = usize::try_from((offset - self.layout.priority_base) / 4)
                .map_err(|_| Exception::LoadAccessFault(addr))?;
            return Ok(self.priorities.get(source).copied().unwrap_or(0).into());
        }
        let pending_bytes = u64::try_from(self.register_words()).unwrap_or(u64::MAX) * 4;
        if (self.layout.pending_base..self.layout.pending_base + pending_bytes).contains(&offset) {
            let word = ((offset - self.layout.pending_base) / 4) as usize;
            return Ok(self.pending_word(word).into());
        }
        if let Some((context, word)) = self.decode_enable(offset) {
            return Ok(self.enable_word(context, word).into());
        }
        if let Some((context, register)) = self.decode_context(offset) {
            return match register {
                0 => Ok(self.thresholds[context].into()),
                4 => Ok(self.claim(context).into()),
                _ => Ok(0),
            };
        }
        Ok(0)
    }

    fn write(&mut self, addr: u64, value: u64, size: usize) -> Result<(), Exception> {
        if size != 4 || addr & 3 != 0 {
            return Err(Exception::StoreAMOAccessFault(addr));
        }
        let offset = self
            .offset(addr)
            .ok_or(Exception::StoreAMOAccessFault(addr))?;
        let value = value as u32;

        if offset >= self.layout.priority_base && offset < self.layout.pending_base {
            let source = usize::try_from((offset - self.layout.priority_base) / 4)
                .map_err(|_| Exception::StoreAMOAccessFault(addr))?;
            if source > 0 && source < self.priorities.len() {
                self.priorities[source] = value & self.maximum_priority;
            }
            return Ok(());
        }
        let pending_bytes = u64::try_from(self.register_words()).unwrap_or(u64::MAX) * 4;
        if (self.layout.pending_base..self.layout.pending_base + pending_bytes).contains(&offset) {
            // IP 位由中断网关维护，软件写入没有效果。
            return Ok(());
        }
        if let Some((context, word)) = self.decode_enable(offset) {
            self.write_enable_word(context, word, value);
            return Ok(());
        }
        if let Some((context, register)) = self.decode_context(offset) {
            match register {
                0 => self.thresholds[context] = value & self.maximum_priority,
                4 => self.complete(context, value),
                _ => {}
            }
        }
        Ok(())
    }

    fn pending_interrupts(&mut self) -> InterruptSet {
        self.sync_gateways();
        let mut result = InterruptSet::EMPTY;
        for (context, interrupt) in self.context_interrupts.iter().copied().enumerate() {
            if self
                .eligible_source(context, self.thresholds[context])
                .is_some()
            {
                result.insert(interrupt);
            }
        }
        result
    }

    fn reset(&mut self) {
        self.priorities.fill(0);
        self.pending.fill(false);
        self.pending_sources.clear();
        for enabled in &mut self.enabled {
            enabled.fill(false);
        }
        self.thresholds.fill(0);
        self.gateway_busy.fill(false);
    }

    fn tick(&mut self, _cycles: u64) {
        self.sync_gateways();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u64 = 0x0c00_0000;

    fn plic() -> Plic {
        Plic::new(
            BASE,
            PlicLayout::SIFIVE,
            63,
            7,
            vec![
                InterruptCause::MachineExternal,
                InterruptCause::SupervisorExternal,
            ],
        )
        .unwrap()
    }

    #[test]
    fn priority_enable_threshold_and_claim_follow_plic_arbitration() {
        let mut plic = plic();
        let source_1 = plic.source_line(1).unwrap();
        let source_10 = plic.source_line(10).unwrap();

        plic.write(BASE + 4, 2, 4).unwrap();
        plic.write(BASE + 10 * 4, 2, 4).unwrap();
        plic.write(BASE + 0x2080, (1 << 1) | (1 << 10), 4).unwrap();
        source_10.assert();
        source_1.assert();

        let pending = plic.pending_interrupts();
        assert!(pending.contains(InterruptCause::SupervisorExternal));
        assert!(!pending.contains(InterruptCause::MachineExternal));
        // 同优先级由较小的源编号胜出，查询中断本身不能消费 claim。
        assert_eq!(plic.read(BASE + 0x201004, 4), Ok(1));
        source_1.deassert();
        plic.write(BASE + 0x201004, 1, 4).unwrap();
        assert_eq!(plic.read(BASE + 0x201004, 4), Ok(10));

        plic.write(BASE + 0x201004, 10, 4).unwrap();
        plic.write(BASE + 0x201000, 2, 4).unwrap();
        assert!(plic.pending_interrupts().is_empty());
    }

    #[test]
    fn a_level_source_can_reenter_only_after_completion() {
        let mut plic = plic();
        let source = plic.source_line(10).unwrap();
        plic.write(BASE + 10 * 4, 1, 4).unwrap();
        plic.write(BASE + 0x2080, 1 << 10, 4).unwrap();
        source.assert();

        assert_eq!(plic.read(BASE + 0x201004, 4), Ok(10));
        assert_eq!(plic.read(BASE + 0x201004, 4), Ok(0));
        plic.write(BASE + 0x201004, 10, 4).unwrap();
        assert_eq!(plic.read(BASE + 0x201004, 4), Ok(10));
        source.deassert();
        plic.write(BASE + 0x201004, 10, 4).unwrap();
        assert_eq!(plic.read(BASE + 0x201004, 4), Ok(0));
    }

    #[test]
    fn claim_ignores_threshold_but_excludes_disabled_and_zero_priority_sources() {
        let mut plic = plic();
        for source in 1..=3 {
            plic.source_line(source).unwrap().assert();
        }
        plic.write(BASE + 4, 2, 4).unwrap();
        plic.write(BASE + 8, 7, 4).unwrap();
        plic.write(BASE + 0x2080, (1 << 1) | (1 << 3), 4).unwrap();
        plic.write(BASE + 0x201000, 7, 4).unwrap();

        assert!(plic.pending_interrupts().is_empty());
        assert_eq!(plic.read(BASE + 0x201004, 4), Ok(1));
        assert_eq!(plic.read(BASE + 0x201004, 4), Ok(0));
        assert_eq!(plic.read(BASE + 0x1000, 4), Ok((1 << 2) | (1 << 3)));
    }

    #[test]
    fn completion_uses_current_target_enable_instead_of_claim_ownership() {
        let mut plic = plic();
        plic.source_line(1).unwrap().assert();
        plic.write(BASE + 4, 1, 4).unwrap();
        plic.write(BASE + 0x2000, 1 << 1, 4).unwrap();
        plic.write(BASE + 0x2080, 1 << 1, 4).unwrap();
        assert_eq!(plic.read(BASE + 0x200004, 4), Ok(1));

        // M 上下文 claim 后禁用此源，此时对 M 的 completion 必须被忽略。
        plic.write(BASE + 0x2000, 0, 4).unwrap();
        plic.write(BASE + 0x200004, 1, 4).unwrap();
        assert_eq!(plic.read(BASE + 0x201004, 4), Ok(0));

        // S 上下文仍启用此源，即使 claim 来自 M，也必须接受完成通知。
        plic.write(BASE + 0x201004, 1, 4).unwrap();
        assert_eq!(plic.read(BASE + 0x201004, 4), Ok(1));
        for invalid in [0, 64, u32::MAX] {
            plic.write(BASE + 0x201004, invalid.into(), 4).unwrap();
        }
        assert_eq!(plic.read(BASE + 0x201004, 4), Ok(0));
    }

    #[test]
    fn invalid_parameters_and_non_word_mmio_are_rejected() {
        assert!(matches!(
            Plic::new(
                BASE,
                PlicLayout::SIFIVE,
                0,
                7,
                vec![InterruptCause::MachineExternal]
            ),
            Err(PlicError::InvalidSourceCount)
        ));
        assert!(matches!(
            Plic::new(
                BASE,
                PlicLayout {
                    pending_base: PlicLayout::SIFIVE.pending_base + 1,
                    ..PlicLayout::SIFIVE
                },
                63,
                7,
                vec![InterruptCause::MachineExternal]
            ),
            Err(PlicError::MisalignedLayout)
        ));
        let mut plic = plic();
        assert_eq!(plic.read(BASE, 1), Err(Exception::LoadAccessFault(BASE)));
        assert_eq!(
            plic.write(BASE + 2, 1, 4),
            Err(Exception::StoreAMOAccessFault(BASE + 2))
        );
    }
}
