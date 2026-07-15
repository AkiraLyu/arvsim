//! CPU、总线和设备之间传递的同步异常。
//!
//! 枚举携带产生异常的地址、PC 或原始指令，并统一提供架构原因码与附加值。

/// 当前执行核心能够上报的异常集合。
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Exception {
    /// 取指地址未按要求对齐。
    InstructionAddrMisaligned(u64),
    /// 取指物理访问失败。
    InstructionAccessFault(u64),
    /// 指令编码不受当前执行器支持。
    IllegalInstruction(u64),
    /// 执行断点指令。
    Breakpoint(u64),
    /// 读取地址未按要求对齐。
    LoadAccessMisaligned(u64),
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
            Self::LoadAccessMisaligned(_) => 4,
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
            | Self::LoadAccessMisaligned(addr)
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
