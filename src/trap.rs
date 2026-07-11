//! CPU、总线和设备之间传递的同步异常。
//!
//! 枚举携带产生异常的地址、PC 或原始指令；CPU 在进入监督模式 trap 时再把它转换为 `scause/stval`。

/// 当前执行核心能够上报的异常集合。
#[derive(Debug, Copy, Clone)]
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
