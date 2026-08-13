# `src/cfg.rs`：机器常量

## 功能

集中定义默认 CPU/DRAM 以及 QEMU `virt` 风格 UART、PLIC、virtio-blk 布局；命令行程序、`Platform` 和 `VirtPlatformConfig` 可以在运行时覆盖这些默认值。

## 实现状态

默认配置为 128 MiB DRAM，地址范围是 `0x8000_0000..0x8800_0000`；复位 PC 位于 DRAM 起点。UART 位于 `0x1000_0000`、IRQ 为 10，PLIC 位于 `0x0c00_0000`，virtio-blk 位于 `0x1000_1000`、IRQ 为 1。PLIC 源数、优先级宽度、virtqueue 上限、vendor id 和 UART 平台周期发送延迟也集中定义。命令行参数当前可覆盖 DRAM、UART 和入口地址；库调用方可复制并修改 `VirtPlatformConfig::default()` 覆盖完整布局。

## 公共接口

- `DRAM_SIZE: usize`
- `DRAM_BASE: u64`
- `DRAM_END: u64`
- `CPU_START_ADDR: u64`
- `UART_BASE: u64`
- `UART_SIZE: u64`
- `UART_TRANSMIT_DELAY_CYCLES: u64`
- `PLIC_{BASE,SOURCE_COUNT,MAX_PRIORITY}`
- `UART_IRQ`
- `VIRTIO_BLOCK_{BASE,SIZE,IRQ}`
- `VIRTIO_QUEUE_SIZE`
- `VIRTIO_VENDOR_ID`

## 依赖关系

`Dram` 默认构造、命令行默认参数和 `VirtPlatformConfig::default` 会读取这些常量。通用 RAM 通过 `Bus::attach_device` 显式传入实际大小；`Bus::attach_uart` 与 `Platform::attach_uart` 按 `UART_SIZE` 挂载 UART 窗口。xv6 加速器把 guest `PHYSTOP` 与实际 DRAM 区间作为独立配置传给 CPU。

## 已知问题与改进建议

- `DRAM_END` 只适用于默认布局，不能描述任意 `Platform`。
- 默认值描述 QEMU `virt` 风格单 hart 布局，不代表 PLIC 规范强制的地址映射；实际平台应显式传入配置。
