//! CPU 与平台地址空间的组装层。
//!
//! [`Platform`] 在 CPU 创建前持有 DRAM 和待挂载设备，统一检查物理区域并完成总线组装。
//! [`Machine`] 则作为运行入口管理 CPU 的复位、单步和连续执行；CPU 内部时钟及总线上的中断源
//! 因而都随同一机器生命周期推进。

use crate::bus::{Bus, MemDevice};
use crate::cpu::{Cpu, RunOptions, RunOutcome};
use crate::dram::Dram;
use crate::trap::Exception;
use crate::uart::Uart;
use std::error::Error;
use std::fmt::{Display, Formatter};

struct MappedDevice {
    base: u64,
    size: u64,
    dev: Box<dyn MemDevice>,
}

/// 平台布局或设备映射不满足组装约束。
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PlatformError {
    /// DRAM 容量必须大于零。
    EmptyDram,
    /// MMIO 设备区域必须大于零。
    EmptyDeviceRegion,
    /// 地址区间的末端超出 `u64`。
    AddressOverflow,
    /// 新设备与 DRAM 或已有设备区域重叠。
    RegionOverlap { base: u64, end: u64 },
    /// CPU 复位向量不位于 DRAM 中。
    ResetVectorOutsideDram { reset_vector: u64 },
}

impl Display for PlatformError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyDram => f.write_str("DRAM size must be non-zero"),
            Self::EmptyDeviceRegion => f.write_str("device region size must be non-zero"),
            Self::AddressOverflow => f.write_str("platform address range overflows u64"),
            Self::RegionOverlap { base, end } => {
                write!(
                    f,
                    "device range {base:#x}..{end:#x} overlaps another region"
                )
            }
            Self::ResetVectorOutsideDram { reset_vector } => {
                write!(f, "reset vector {reset_vector:#x} is outside DRAM")
            }
        }
    }
}

impl Error for PlatformError {}

/// 尚未创建 CPU 的平台地址空间。
///
/// 镜像应通过 [`Platform::dram_mut`] 装载；设备必须在 [`Platform::build`] 前完成挂载。
pub struct Platform {
    dram: Dram,
    dram_end: u64,
    devices: Vec<MappedDevice>,
}

impl Platform {
    /// 创建只含 DRAM 的平台。
    pub fn new(dram_base: u64, dram_size: usize) -> Result<Self, PlatformError> {
        let size = u64::try_from(dram_size).map_err(|_| PlatformError::AddressOverflow)?;
        if size == 0 {
            return Err(PlatformError::EmptyDram);
        }
        let dram_end = dram_base
            .checked_add(size)
            .ok_or(PlatformError::AddressOverflow)?;
        Ok(Self {
            dram: Dram::with_layout(dram_base, dram_size),
            dram_end,
            devices: Vec::new(),
        })
    }

    /// 返回平台主存，供镜像装载器读取布局。
    pub fn dram(&self) -> &Dram {
        &self.dram
    }

    /// 返回平台主存的可变引用；CPU 创建后应改由总线访问内存。
    pub fn dram_mut(&mut self) -> &mut Dram {
        &mut self.dram
    }

    /// 挂载一个通用 MMIO 设备。
    ///
    /// 实现了 [`MemDevice::pending_interrupt`] 的设备会在构建后自动参与 CPU 的中断轮询。
    pub fn attach_device(
        &mut self,
        base: u64,
        size: u64,
        dev: Box<dyn MemDevice>,
    ) -> Result<(), PlatformError> {
        if size == 0 {
            return Err(PlatformError::EmptyDeviceRegion);
        }
        let end = base
            .checked_add(size)
            .ok_or(PlatformError::AddressOverflow)?;
        let overlaps_dram = ranges_overlap(base, end, self.dram.base, self.dram_end);
        let overlaps_device = self
            .devices
            .iter()
            .any(|region| ranges_overlap(base, end, region.base, region.base + region.size));
        if overlaps_dram || overlaps_device {
            return Err(PlatformError::RegionOverlap { base, end });
        }
        self.devices.push(MappedDevice { base, size, dev });
        Ok(())
    }

    /// 挂载正式库中的简化 UART。
    pub fn attach_uart(&mut self, base: u64) -> Result<(), PlatformError> {
        self.attach_device(base, 0x100, Box::new(Uart::new(base)))
    }

    /// 验证复位向量并完成总线、CPU、时钟状态和中断源的组装。
    pub fn build(self, reset_vector: u64) -> Result<Machine, PlatformError> {
        if !(self.dram.base..self.dram_end).contains(&reset_vector) {
            return Err(PlatformError::ResetVectorOutsideDram { reset_vector });
        }

        let dram_base = self.dram.base;
        let dram_size = self.dram_end - dram_base;
        let mut bus = Bus::new();
        bus.attach_device(dram_base, dram_size, Box::new(self.dram));
        for region in self.devices {
            bus.attach_device(region.base, region.size, region.dev);
        }

        Ok(Machine::from_address_space(
            Box::new(bus),
            reset_vector,
            self.dram_end,
        ))
    }
}

/// 已组装地址空间、可以执行 guest 的机器容器。
pub struct Machine {
    /// CPU 状态；总线、时钟计数和中断查询均由该状态推进。
    pub cpu: Cpu,
}

impl Machine {
    /// 用一个已经实现完整物理地址空间的设备创建机器。
    ///
    /// 该入口用于测试平台等自行实现地址分发和中断控制的场景；常规平台应优先使用
    /// [`Platform::build`] 以获得区域冲突检查。
    pub fn from_address_space(
        address_space: Box<dyn MemDevice>,
        reset_vector: u64,
        initial_sp: u64,
    ) -> Self {
        Self {
            cpu: Cpu::with_reset_vector(address_space, reset_vector, initial_sp),
        }
    }

    /// 恢复 CPU 的复位状态；当前设备 trait 没有 reset 协议，因此总线设备状态会保留。
    pub fn reset(&mut self) {
        self.cpu.reset();
    }

    /// 推进一个机器步骤；当前只是转发到 CPU，尚无独立的平台设备时钟阶段。
    pub fn step(&mut self) -> Result<(), Exception> {
        self.cpu.step()
    }

    /// 运行到步数上限或未处理异常；当前连续运行同样由 CPU 内部循环完成。
    pub fn run(&mut self, options: RunOptions) -> RunOutcome {
        self.cpu.run(options)
    }
}

fn ranges_overlap(lhs_start: u64, lhs_end: u64, rhs_start: u64, rhs_end: u64) -> bool {
    lhs_start < rhs_end && rhs_start < lhs_end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::DebugLevel;

    struct InterruptSource;

    impl MemDevice for InterruptSource {
        fn read(&mut self, addr: u64, _size: usize) -> Result<u64, Exception> {
            Err(Exception::LoadAccessFault(addr))
        }

        fn write(&mut self, addr: u64, _value: u32, _size: usize) -> Result<(), Exception> {
            Err(Exception::StoreAMOAccessFault(addr))
        }

        fn pending_interrupt(&mut self) -> Option<u64> {
            Some((1 << 63) | 9)
        }
    }

    #[test]
    fn machine_run_advances_the_cpu_owned_clock() {
        let base = 0x8000_0000;
        let mut platform = Platform::new(base, 16).unwrap();
        platform
            .dram_mut()
            .load_bytes(base, &[0x93, 0x0f, 0xa0, 0x02])
            .unwrap();
        let mut machine = platform.build(base).unwrap();

        let outcome = machine.run(RunOptions {
            max_steps: Some(1),
            debug: DebugLevel::Off,
        });

        assert!(matches!(outcome, RunOutcome::StepLimitReached { steps: 1 }));
        assert_eq!(machine.cpu.registers[31], 42);
        assert_eq!(machine.cpu.cycles, 10);
    }

    #[test]
    fn platform_rejects_invalid_regions_and_reset_vectors() {
        let mut platform = Platform::new(0x8000_0000, 0x1000).unwrap();
        assert!(matches!(
            platform.attach_device(0x8000_0800, 0x100, Box::new(InterruptSource)),
            Err(PlatformError::RegionOverlap { .. })
        ));
        assert!(matches!(
            platform.build(0x9000_0000),
            Err(PlatformError::ResetVectorOutsideDram { .. })
        ));
    }

    #[test]
    fn mapped_interrupt_sources_reach_the_cpu_bus() {
        let mut platform = Platform::new(0x8000_0000, 0x1000).unwrap();
        platform
            .attach_device(0x1000_0000, 0x100, Box::new(InterruptSource))
            .unwrap();
        let mut machine = platform.build(0x8000_0000).unwrap();

        assert_eq!(machine.cpu.bus.pending_interrupt(), Some((1 << 63) | 9));
    }
}
