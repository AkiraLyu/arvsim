//! arvsim 的库入口。
//!
//! crate 将 CPU 核心、指令执行、CSR、异常以及内存映射设备拆成独立模块。
//! 常规调用方可通过 [`machine::Platform`] 组装 DRAM 和 MMIO 后构建 [`machine::Machine`]；
//! 自定义测试平台也可以把完整 [`bus::MemDevice`] 地址空间直接注入机器。

pub mod bus;
pub mod cfg;
pub mod clint;
pub mod cpu;
pub mod csr;
pub mod dram;
pub mod instruction;
pub mod loader;
pub mod machine;
pub mod plic;
pub mod trap;
pub mod uart;
