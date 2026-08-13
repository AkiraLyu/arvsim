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
/// 默认 UART 发送完成延迟，以平台周期计。
pub const UART_TRANSMIT_DELAY_CYCLES: u64 = 20;
/// QEMU `virt` 平台的 PLIC 基址。
pub const PLIC_BASE: u64 = 0x0c00_0000;
/// 默认 PLIC 可表示的最大中断源编号。
pub const PLIC_SOURCE_COUNT: u32 = 1023;
/// 默认 PLIC 的三位优先级字段。
pub const PLIC_MAX_PRIORITY: u32 = 7;
/// QEMU `virt` UART0 的 PLIC 源编号。
pub const UART_IRQ: u32 = 10;
/// QEMU `virt` 首个 virtio-mmio 设备的基址。
pub const VIRTIO_BLOCK_BASE: u64 = 0x1000_1000;
/// 默认 virtio-mmio 窗口大小。
pub const VIRTIO_BLOCK_SIZE: u64 = 0x1000;
/// QEMU `virt` 首个 virtio-mmio 设备的 PLIC 源编号。
pub const VIRTIO_BLOCK_IRQ: u32 = 1;
/// 默认 virtio split queue 可提供的最大描述符数。
pub const VIRTIO_QUEUE_SIZE: u16 = 256;
/// QEMU 的 virtio MMIO vendor 标识。
pub const VIRTIO_VENDOR_ID: u32 = 0x554d_4551;
