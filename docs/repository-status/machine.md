# `src/machine.rs`：机器与平台组装层

## 功能与实现

`Platform` 在创建 CPU 前保存 DRAM 和待挂载设备，并检查区域是否为空、地址是否溢出、MMIO 是否与 DRAM 或其他设备重叠，以及复位向量是否位于 DRAM。`build` 随后创建 `Bus` 和 `Machine`。`Machine` 提供 `reset`、`step` 和 `run`，但目前只是把调用转发给 CPU，还不管理独立的平台时钟或设备状态。

## 实现状态

地址空间组装和冲突检查已经可用。命令行程序通过 `Platform` 创建 DRAM 和 UART，测试代码通过 `Machine::from_address_space` 传入自己的 `TestBus`。运行过程尚未统一：`Machine::run` 直接调用 `Cpu::run`，测试辅助代码和 xv6 临时运行程序也会直接调用公开的 `cpu.step()`。库中设备与测试设备仍是两套实现。

## 公共接口

- `Platform::{new, dram, dram_mut, attach_device, attach_uart, build}`。
- `PlatformError`：空区域、地址溢出、区域重叠和非法复位向量。
- `Machine::from_address_space`：接入已经实现完整地址分发的测试或自定义平台。
- `Machine::{reset, step, run}` 以及公开的 `cpu` 状态。

## 依赖关系

- `Platform` 依赖库中的 `Dram`、`Uart`、`Bus` 和 `MemDevice`。
- `Machine` 依赖 `Cpu`；时钟仍由 `Cpu::step` 推进，中断仍由 CPU 查询地址空间的 `pending_interrupt`。
- 测试平台只复用 `Machine` 容器，没有复用库中的 `Platform`、`Bus` 和 `Uart`。

## 已知限制

- `Machine` 当前只支持一个硬件线程。
- `reset()` 只复位 CPU；总线、UART 输入、PLIC、virtio 和未来的 CLINT 都会保留原状态。
- `run()` 不会反复调用 `Machine::step()`，公开的 `cpu` 字段也允许调用方跳过将来的设备推进和中断同步。
- `Platform::build` 只检查复位向量是否位于 DRAM，并直接把 DRAM 末端设为 SP。它不检查指令地址对齐，也不保证自定义 DRAM 末端满足 ABI 要求的 16 字节栈对齐。
- 平台设备仍以 `Box<dyn MemDevice>` 挂载，尚无能够明确区分中断类型的控制器接口。
- DRAM 在平台创建时一次性分配，宿主机内存分配失败后无法恢复。
