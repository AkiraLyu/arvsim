# `src/cfg.rs`：机器常量

## 功能

集中定义默认 CPU/DRAM/UART 布局；命令行程序和 `Platform` 可以在运行时覆盖这些默认值。

## 实现状态

默认配置为 128 MiB DRAM，地址范围是 `0x8000_0000..0x8800_0000`；复位 PC 位于 DRAM 起点，UART 基址为 `0x1000_0000`。命令行参数可以覆盖 DRAM、UART 和入口地址，`Platform` 会使用这些运行时值创建机器，因此本模块只提供默认值。

## 公共接口

- `DRAM_SIZE: usize`
- `DRAM_BASE: u64`
- `DRAM_END: u64`
- `CPU_START_ADDR: u64`
- `UART_BASE: u64`

## 依赖关系

CPU 默认初始化、`Dram` 默认构造、命令行默认参数、部分 xv6 加速和测试总线都会读取这些常量。`Bus::attach_ram` 还默认所有 RAM 都是 `DRAM_SIZE` 大小。

## 已知问题与改进建议

- 自定义布局只传给了 `Platform`。CPU 已显式记录特权级，但批量内存填充和部分 xv6 加速仍读取默认地址，因此自定义 DRAM 基址没有传到所有模块。
- PLIC、virtio 等地址只存在测试模块，机器布局没有唯一来源。
- `DRAM_END` 只适用于默认布局，不能描述任意 `Platform`。
- 建议增加不可变的 `MachineConfig`，由平台创建代码把地址配置传给 CPU、加速代码和设备；常用 virt 平台和测试也应共用同一配置。
