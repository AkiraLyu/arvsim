# `src/cfg.rs`：机器常量

## 功能

集中定义正式机器的固定 DRAM 和 UART 布局。

## 当前状态与实现思路

已实现单一静态配置：DRAM 为 128 MiB，范围 `0x8000_0000..0x8800_0000`，复位 PC 为 DRAM 起点，UART 基址为 `0x1000_0000`。

## 对外接口

- `DRAM_SIZE: usize`
- `DRAM_BASE: u64`
- `DRAM_END: u64`
- `CPU_START_ADDR: u64`
- `UART_BASE: u64`

## 耦合方式

被 `Cpu` 初始化和快速路径、`Dram`、CLI、指令加速以及测试总线共同引用。`Bus::attach_ram` 也隐式假设所有 RAM 都有 `DRAM_SIZE`。

## 不完善之处和优化方向

- 无运行时配置、多平台或多内存区域支持。
- PLIC、virtio 等地址只存在测试模块，机器布局没有唯一来源。
- 建议引入不可变 `MachineConfig`，由平台组装层注入 CPU、总线和设备；为常用 virt 平台提供预设，并让测试复用同一配置。
