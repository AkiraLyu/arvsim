//! CPU、总线和设备之间传递的异常与中断原因。
//!
//! 同步异常携带地址、PC 或原始指令；中断原因则使用独立枚举，避免把 `mcause` 的最高位、
//! 中断编号和 `mip` 位图混为同一种值。

/// `mcause/scause` 中区分中断与同步异常的最高位。
pub const INTERRUPT_FLAG: u64 = 1 << 63;

/// 当前单 hart 执行核心支持的标准 RISC-V 中断原因。
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum InterruptCause {
    SupervisorSoftware = 1,
    MachineSoftware = 3,
    SupervisorTimer = 5,
    MachineTimer = 7,
    SupervisorExternal = 9,
    MachineExternal = 11,
}

impl InterruptCause {
    /// `mcause/scause` 最高位之外的中断原因编号。
    pub const fn code(self) -> u64 {
        self as u64
    }

    /// 该中断在 `mip/mie` 中对应的位。
    pub const fn mask(self) -> u64 {
        1 << self.code()
    }

    /// 写入 `mcause/scause` 的完整编码。
    pub const fn encoded(self) -> u64 {
        INTERRUPT_FLAG | self.code()
    }
}

/// 一个或多个同时有效的 `mip` 中断位。
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct InterruptSet(u64);

impl InterruptSet {
    pub const EMPTY: Self = Self(0);

    pub const fn from_cause(cause: InterruptCause) -> Self {
        Self(cause.mask())
    }

    pub const fn bits(self) -> u64 {
        self.0
    }

    pub fn insert(&mut self, cause: InterruptCause) {
        self.0 |= cause.mask();
    }

    pub fn merge(&mut self, other: Self) {
        self.0 |= other.0;
    }

    pub const fn contains(self, cause: InterruptCause) -> bool {
        self.0 & cause.mask() != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// 当前执行核心能够上报的异常集合。
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Exception {
    /// 取指地址未按要求对齐。
    InstructionAddrMisaligned(u64),
    /// 取指物理访问失败。
    InstructionAccessFault(u64),
    /// 指令编码不受支持，或当前特权级与陷阱设置不允许执行。
    IllegalInstruction(u64),
    /// 执行断点指令。
    Breakpoint(u64),
    /// 读取地址未按要求对齐。
    LoadAddrMisaligned(u64),
    /// 读取物理访问失败。
    LoadAccessFault(u64),
    /// 写入或原子访问地址未按要求对齐。
    StoreAMOAddrMisaligned(u64),
    /// 写入或原子物理访问失败。
    StoreAMOAccessFault(u64),
    /// 用户模式环境调用。
    EnvironmentCallFromUMode(u64),
    /// 监督模式环境调用。
    EnvironmentCallFromSMode(u64),
    /// 机器模式环境调用。
    EnvironmentCallFromMMode(u64),
    /// 取指地址翻译或页权限检查失败。
    InstructionPageFault(u64),
    /// 读取地址翻译或页权限检查失败。
    LoadPageFault(u64),
    /// 写入地址翻译或页权限检查失败。
    StoreAMOPageFault(u64),
}

impl Exception {
    /// RISC-V `mcause/scause` 中的同步异常原因码。
    pub const fn cause(self) -> u64 {
        match self {
            Self::InstructionAddrMisaligned(_) => 0,
            Self::InstructionAccessFault(_) => 1,
            Self::IllegalInstruction(_) => 2,
            Self::Breakpoint(_) => 3,
            Self::LoadAddrMisaligned(_) => 4,
            Self::LoadAccessFault(_) => 5,
            Self::StoreAMOAddrMisaligned(_) => 6,
            Self::StoreAMOAccessFault(_) => 7,
            Self::EnvironmentCallFromUMode(_) => 8,
            Self::EnvironmentCallFromSMode(_) => 9,
            Self::EnvironmentCallFromMMode(_) => 11,
            Self::InstructionPageFault(_) => 12,
            Self::LoadPageFault(_) => 13,
            Self::StoreAMOPageFault(_) => 15,
        }
    }

    /// RISC-V `mtval/stval` 中与异常相关的地址或指令值。
    pub const fn value(self) -> u64 {
        match self {
            Self::InstructionAddrMisaligned(addr)
            | Self::InstructionAccessFault(addr)
            | Self::IllegalInstruction(addr)
            | Self::Breakpoint(addr)
            | Self::LoadAddrMisaligned(addr)
            | Self::LoadAccessFault(addr)
            | Self::StoreAMOAddrMisaligned(addr)
            | Self::StoreAMOAccessFault(addr)
            | Self::InstructionPageFault(addr)
            | Self::LoadPageFault(addr)
            | Self::StoreAMOPageFault(addr) => addr,
            Self::EnvironmentCallFromUMode(_)
            | Self::EnvironmentCallFromSMode(_)
            | Self::EnvironmentCallFromMMode(_) => 0,
        }
    }
}
