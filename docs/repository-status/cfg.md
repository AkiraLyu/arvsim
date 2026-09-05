# `src/cfg.rs`：默认地址与设备配置

本模块集中定义 CPU、DRAM 以及 QEMU `virt` 风格设备的默认配置。命令行程序、`Platform` 和 `VirtPlatformConfig` 可以在运行时修改其中相应的配置。

## 默认配置

| 项目 | 默认值 |
| --- | --- |
| DRAM 容量 | 128 MiB |
| DRAM 地址范围 | `0x8000_0000..0x8800_0000`，不包含结束地址 |
| CPU 复位地址 | DRAM 起始地址 |
| UART 基址与中断号 | `0x1000_0000`，IRQ 10 |
| PLIC 基址 | `0x0c00_0000` |
| Virtio 块设备基址与中断号 | `0x1000_1000`，IRQ 1 |

PLIC 中断源数量、最大优先级、Virtio 队列大小上限、厂商编号和 UART 发送延迟也在这里定义。UART 延迟以平台周期为单位。命令行当前可修改 DRAM、UART 和入口地址；库调用方可在 `VirtPlatformConfig::default()` 的基础上修改完整平台配置。

## 公开常量

- `DRAM_SIZE: usize`、`DRAM_BASE: u64`、`DRAM_END: u64`、`CPU_START_ADDR: u64`。
- `UART_BASE: u64`、`UART_SIZE: u64`、`UART_TRANSMIT_DELAY_CYCLES: u64`、`UART_IRQ`。
- `PLIC_{BASE,SOURCE_COUNT,MAX_PRIORITY}`。
- `VIRTIO_BLOCK_{BASE,SIZE,IRQ}`、`VIRTIO_QUEUE_SIZE`、`VIRTIO_VENDOR_ID`。

## 使用范围

`Dram` 的默认构造方法、命令行默认参数和 `VirtPlatformConfig::default` 读取这些常量。通过 `Bus::attach_device` 添加 RAM 时，需要另行指定实际大小；`Bus::attach_uart` 与 `Platform::attach_uart` 使用 `UART_SIZE` 确定 UART 的地址范围。xv6 加速器分别记录内核使用的内存上限 `PHYSTOP` 和模拟器实际提供的 DRAM 范围。

`DRAM_END` 只适用于默认布局，不能用来判断任意平台的内存边界。这些地址描述的是单硬件线程的 QEMU `virt` 风格平台；其他平台应使用自己的配置。
