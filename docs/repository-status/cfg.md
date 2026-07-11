# `src/cfg.rs`：机器常量

## 功能

集中定义默认 CPU/DRAM/UART 布局；运行时覆盖由 CLI 和 `Platform` 处理。

## 当前状态与实现思路

已实现默认配置：DRAM 为 128 MiB，范围 `0x8000_0000..0x8800_0000`，复位 PC 为 DRAM 起点，UART 基址为 `0x1000_0000`。CLI 可以覆盖 DRAM/UART/entry，`Platform` 会用运行时值组装机器；这些常量不再代表唯一可构造的布局。

## 对外接口

- `DRAM_SIZE: usize`
- `DRAM_BASE: u64`
- `DRAM_END: u64`
- `CPU_START_ADDR: u64`
- `UART_BASE: u64`

## 耦合方式

被 `Cpu` 默认初始化和用户态启发式、`Dram` 默认构造、CLI 默认参数、指令加速以及测试总线共同引用。`Bus::attach_ram` 也隐式假设所有 RAM 都有 `DRAM_SIZE`。

## 不完善之处和优化方向

- 运行时布局只在 `Platform` 局部生效；CPU 的 `pc < DRAM_BASE` privilege 启发式和 memset 快速路径仍使用默认常量，自定义 DRAM 基址并未贯穿所有层。
- PLIC、virtio 等地址只存在测试模块，机器布局没有唯一来源。
- `DRAM_END` 是默认常量，不能描述任意 `Platform`；新增代码若误用它会重新引入布局分裂。
- 建议引入不可变 `MachineConfig`，由平台组装层把布局注入 CPU、快速路径和设备；为常用 virt 平台提供预设，并让测试复用同一配置。
