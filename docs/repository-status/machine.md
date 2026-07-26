# `src/machine.rs`：机器与平台组装层

## 功能与实现

`Platform` 在创建 CPU 前保存 DRAM 和待挂载设备，并检查区域是否为空、地址是否溢出、MMIO 是否与 DRAM 或其他设备重叠。`build` 还要求复位向量位于 DRAM 且按 2 字节对齐，并将 DRAM 末端向下对齐到 16 字节后作为初始栈指针，随后创建 `Bus` 和 `Machine`。

`Machine` 是公开的运行入口。`reset` 先复位地址空间中的设备，再恢复 CPU；`step` 先让设备推进一个固定的 10 周期步长，再让 CPU 更新时间、查询中断并执行；`run` 反复调用 `Machine::step`，直到达到步数上限或出现未处理异常。`MemDevice` 的 `reset` 和 `tick` 默认不执行操作，因此 DRAM、纯同步设备和已有自定义地址空间可以保持原行为，需要易失状态或异步推进的设备则应覆盖对应方法。

## 实现状态

地址空间组装、入口检查、栈对齐、设备生命周期和运行循环已经统一。命令行程序通过 `Platform` 创建 DRAM 和 UART，测试代码通过 `Machine::from_address_space` 传入自己的 `TestBus`；测试辅助代码、RV64I 冒烟测试和 xv6 临时运行程序均通过 `Machine::step` 推进。`Cpu::reset` 和 `Cpu::step` 只在 crate 内可见，CPU 自身不再提供连续运行循环，避免外部调用方绕过机器级设备推进。

库中设备与测试设备仍是两套实现；当前正式设备也都不依赖时钟。测试总线实现了设备复位，保留已装载的 RAM、磁盘镜像和符号配置，同时清除 UART、PLIC、virtio 和 MMIO 日志的易失状态。

## 公共接口

- `Platform::{new, dram, dram_mut, attach_device, attach_uart, build}`。
- `PlatformError`：空区域、地址溢出、区域重叠、复位向量越界和复位向量未对齐。
- `Machine::from_address_space`：接入已经实现完整地址分发和生命周期协议的测试或自定义平台。
- `Machine::{reset, step, run}` 以及公开的 `cpu` 状态。
- `DebugLevel`、`RunOptions`、`RunOutcome`；`DebugLevel` 的原始定义仍在 `cpu`，旧的 `cpu::{RunOptions, RunOutcome}` 路径保留为兼容重导出。

## 依赖关系

- `Platform` 依赖库中的 `Dram`、`Uart`、`Bus` 和 `MemDevice`。
- `Machine` 依赖 `Cpu`，并通过 CPU 持有的地址空间调用 `MemDevice::{reset,tick}`。CPU 内部周期和设备周期目前都按每步 10 个周期推进。
- 中断仍由 CPU 在每步设备推进后调用地址空间的 `pending_interrupt` 查询。
- 测试平台复用 `Machine` 和设备生命周期接口，但没有复用库中的 `Platform`、`Bus` 和 `Uart`。

## 已知限制

- `Machine` 当前只支持一个硬件线程，每步固定推进 10 个周期；还没有独立于指令执行的事件调度或空闲时钟推进。
- `MemDevice` 的生命周期方法默认为空。自定义设备若不实现 `reset`，其状态仍会跨机器复位保留；若不实现 `tick`，也不会随平台时钟推进。
- `Machine::from_address_space` 是低层入口，不执行 `Platform` 的区域、复位向量和栈对齐检查，调用方必须自行保证这些约束。
- `cpu` 字段仍公开，调用方可以直接修改寄存器、CSR 或总线状态；但外部代码不能直接调用 CPU 的复位、单步或连续运行接口。
- 平台设备仍以 `Box<dyn MemDevice>` 挂载，中断原因仍是未经类型约束的 `u64`，也没有能够明确区分本地中断和外部中断的控制器接口。
- 库中只有简化 UART，PLIC 和 virtio 仍只存在于测试平台。DRAM 在平台创建时一次性分配，宿主机内存分配失败后无法恢复。
