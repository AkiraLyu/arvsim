//! 默认机器的地址布局。
//!
//! 这些常量只描述 `Dram::new`、默认 CLI 和测试平台使用的布局；可配置入口可在运行时覆盖它们。

/// 默认 DRAM 容量：128 MiB。
pub const DRAM_SIZE: usize = 1024 * 1024 * 128;
/// 默认 DRAM 物理基址。
pub const DRAM_BASE: u64 = 0x80000000;
/// 默认 DRAM 半开区间的末端地址。
pub const DRAM_END: u64 = DRAM_BASE + DRAM_SIZE as u64;
/// CPU 默认复位向量。
pub const CPU_START_ADDR: u64 = DRAM_BASE;
/// 默认 UART MMIO 基址。
pub const UART_BASE: u64 = 0x10000000;
/// 默认 UART MMIO 窗口大小（16550 寄存器窗口）。
pub const UART_SIZE: u64 = 0x100;
