# `src/machine.rs`：平台创建与机器运行

## 创建平台

`Platform` 保存共享 DRAM 和待添加的设备。创建 CPU 前，它会检查地址区域是否为空、地址计算是否溢出，以及 MMIO 区域是否与 DRAM 或其他设备重叠。

`build` 要求 CPU 复位地址位于 DRAM 内，并按 2 字节对齐。初始栈指针取 DRAM 结束地址，再向下调整到 16 字节对齐；如果结果小于 DRAM 起始地址，则返回配置错误。检查通过后才创建 `Bus` 和 `Machine`。总线添加设备时发现的错误也会返回给调用方。

`dram_handle` 提供同一块 DRAM 的共享引用，让 Virtio 等 DMA 设备可以在平台构建前接入内存。

## 运行与复位

`Machine` 提供统一的运行入口：

- `reset` 先复位设备，再复位 CPU。
- `step` 先让设备按经过的 10 个周期更新状态，再由 CPU 更新时间、检查中断，并执行指令或进入异常、中断处理程序。
- `run` 重复调用 `step`，直到达到步数上限，或收到执行错误。

当前所有 RISC-V 异常都交给被模拟程序的异常处理入口，不会作为 `step` 的错误返回。`RunOutcome::Exception` 为将来无法继续运行的错误预留。

命令行通过 `Platform` 创建 DRAM/UART 平台；xv6 测试和示例通过 `VirtPlatform` 创建包含 PLIC 和 Virtio 块设备的平台。它们都通过 `Machine::step` 运行。`Cpu::reset` 和 `Cpu::step` 仅在库内可见，外部调用方应使用 `Machine`，以保证设备状态也按时更新。

UART 通过 `tick` 模拟配置的发送延迟，PLIC 在时钟更新和中断查询时读取中断线电平。Virtio 当前在收到队列通知后同步处理请求。机器复位会保留 RAM 和磁盘内容，同时将 UART、PLIC 和 Virtio 的寄存器、队列配置及中断状态恢复到初始值。

## 公开接口

- `Platform::{new, dram, dram_mut, dram_handle, attach_device, attach_uart, build}`。`dram` 和 `dram_mut` 返回带运行时借用检查的内存访问对象。
- `PlatformError`：表示空区域、地址溢出、区域重叠、复位地址越界或未对齐，以及初始栈指针低于 DRAM 起点。
- `Machine::from_address_space`：使用调用方提供的完整地址空间创建机器。
- `STACK_ALIGNMENT`：RISC-V psABI 要求的初始栈对齐值。
- `Machine::{reset, step, run}` 和公开的 `cpu` 状态。
- `DebugLevel`、`RunOptions`、`RunOutcome`。`DebugLevel` 定义在 `cpu` 中；旧的 `cpu::{RunOptions, RunOutcome}` 导入路径仍可使用。

## 依赖关系与限制

`Platform` 使用 `Dram`、`Uart`、`Bus` 和 `MemDevice`。`Machine` 通过 CPU 持有的地址空间调用设备方法，总线负责合并各设备的中断位。

当前只支持单个硬件线程和固定周期步长，没有独立的事件调度，也不会在 CPU 空闲时跳过等待时间。`MemDevice::reset/tick` 默认不做任何操作；自定义设备若需要复位或随时间变化，必须实现对应方法。

`Machine::from_address_space` 不执行 `Platform` 的地址区域、复位地址和栈对齐检查，调用方需自行保证这些条件。`cpu` 字段仍公开，可直接修改寄存器、CSR 或总线状态。

设备使用 `Box<dyn MemDevice>` 保存。`Shared<T>` 只适用于单线程，借用冲突会触发 Rust 的 `panic`。DRAM 在平台创建时一次性分配，主机内存分配失败后无法恢复。
