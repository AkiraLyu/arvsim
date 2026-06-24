# `src/cfg.rs`：机器常量

## 设计

- 集中保存当前模拟器的固定物理地址和内存尺寸。
- 这些常量同时被正式运行入口、CPU、DRAM、UART 和测试支撑代码使用。

## 实现

- DRAM 大小固定为 128 MiB。
- DRAM 物理地址范围是 `0x80000000..0x88000000`。
- CPU 起始 PC 是 `DRAM_BASE`。
- UART 基址是 `0x10000000`。

## 接口

- `DRAM_SIZE: usize`
- `DRAM_BASE: u64`
- `DRAM_END: u64`
- `CPU_START_ADDR: u64`
- `UART_BASE: u64`

## 限制

- 这些值是编译期常量，没有运行时配置。
- 目前只覆盖 xv6 测试所需的单一机器布局。
