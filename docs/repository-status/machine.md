# `src/machine.rs`：机器与平台组装层

## 功能与实现

`Platform` 在创建 CPU 前保存共享 DRAM 和待挂载设备，并检查区域是否为空、地址是否溢出、MMIO 是否与 DRAM 或其他设备重叠。`build` 还要求复位向量位于 DRAM 且按 2 字节对齐，并将 DRAM 末端向下对齐到 16 字节后作为初始栈指针；若对齐结果落到 DRAM 基址之前则返回配置错误，随后才创建 `Bus` 和 `Machine`。`dram_handle` 允许 virtio 等正式 DMA 设备在平台构建前取得同一 DRAM 的共享句柄。

`Machine` 是公开的运行入口。`reset` 先复位地址空间中的设备，再恢复 CPU；`step` 先让设备推进一个固定的 10 周期步长，再让 CPU 更新时间、查询中断并执行；`run` 反复调用 `Machine::step`，直到达到步数上限或 CPU 将来上报宿主级致命错误。当前所有架构异常都由 CPU 路由到 guest trap 入口，因此 `Machine::step` 的错误通道和 `RunOutcome::Exception` 只作为未来扩展点，不表示 guest 普通异常。`MemDevice` 的 `reset` 和 `tick` 默认不执行操作，因此 DRAM、纯同步设备和已有自定义地址空间可以保持原行为，需要易失状态或异步推进的设备则应覆盖对应方法。

## 实现状态

地址空间组装、入口检查、栈对齐及其下界检查、共享 DRAM、设备生命周期和运行循环已经统一。命令行程序通过 `Platform` 创建 DRAM 和 UART；测试和 xv6 临时运行程序通过正式 `VirtPlatform` 创建 `Platform`、PLIC、UART 和 virtio-blk。所有路径均通过 `Machine::step` 推进。`Cpu::reset` 和 `Cpu::step` 只在 crate 内可见，CPU 自身不再提供连续运行循环，避免外部调用方绕过机器级设备推进。

`Platform::build` 会传播 `Bus` 挂载阶段返回的布局错误；公开挂载接口不再依靠进程异常退出表达失败。

正式 UART 使用 `tick` 完成带参数的发送延迟；PLIC 在推进和查询时采样已连接电平线；virtio 队列通知当前同步完成。机器复位保留 DRAM 和块介质，同时清除 UART、PLIC 和 virtio transport 的易失状态。

## 公共接口

- `Platform::{new, dram, dram_mut, dram_handle, attach_device, attach_uart, build}`；`dram` 与 `dram_mut` 返回共享借用守卫。
- `PlatformError`：空区域、地址溢出、区域重叠、复位向量越界、复位向量未对齐和初始栈对齐后越过 DRAM 下界。
- `Machine::from_address_space`：接入已经实现完整地址分发和生命周期协议的测试或自定义平台；`STACK_ALIGNMENT` 提供调用方计算初始栈所需的 psABI 对齐值。
- `Machine::{reset, step, run}` 以及公开的 `cpu` 状态。
- `DebugLevel`、`RunOptions`、`RunOutcome`；`DebugLevel` 的原始定义仍在 `cpu`，旧的 `cpu::{RunOptions, RunOutcome}` 路径保留为兼容重导出。

## 依赖关系

- `Platform` 依赖库中的 `Dram`、`Uart`、`Bus` 和 `MemDevice`。
- `Machine` 依赖 `Cpu`，并通过 CPU 持有的地址空间调用 `MemDevice::{reset,tick}`。CPU 内部周期和设备周期目前都按每步 10 个周期推进。
- 中断由 CPU 在每步设备推进后调用地址空间的 `pending_interrupts` 查询；总线合并所有设备同时有效的标准 `mip` 位。
- `VirtPlatform` 和测试复用 `Platform`、`Bus`、`Dram`、`Uart`、PLIC 与 virtio-blk。

## 已知限制

- `Machine` 当前只支持一个硬件线程，每步固定推进 10 个周期；还没有独立于指令执行的事件调度或空闲时钟推进。
- `MemDevice` 的生命周期方法默认为空。自定义设备若不实现 `reset`，其状态仍会跨机器复位保留；若不实现 `tick`，也不会随平台时钟推进。
- `Machine::from_address_space` 是低层入口，不执行 `Platform` 的区域、复位向量和栈对齐检查，调用方必须自行保证这些约束。
- `cpu` 字段仍公开，调用方可以直接修改寄存器、CSR 或总线状态；但外部代码不能直接调用 CPU 的复位、单步或连续运行接口。
- 平台设备仍以 `Box<dyn MemDevice>` 挂载；`Shared<T>` 适合当前单线程模型，但运行时借用冲突会 panic，也不能直接用于多线程 hart。
- DRAM 在平台创建时一次性分配，宿主机内存分配失败后无法恢复。
