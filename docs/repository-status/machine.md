# `src/machine.rs`：机器与平台组装层

## 功能与实现思路

`Platform` 在 CPU 创建前持有 DRAM 和待映射设备，统一验证非零区域、地址溢出、MMIO/DRAM 重叠和复位向量。`build` 将这些组件组装成 `Bus` 和 `Machine`。`Machine` 暴露 reset、step 和 run facade，但当前实现只转发到 CPU，还没有独立的平台时钟或设备生命周期阶段。

## 当前状态

部分实现。地址空间组装和冲突检查已经落地：正式 CLI 使用 `Platform` 创建 DRAM/UART，测试支撑把自有 `TestBus` 通过 `Machine::from_address_space` 注入。运行边界尚未收口：`Machine::run` 调用 `Cpu::run`，测试 helper 与 xv6 runner 直接调用公开的 `cpu.step()`，设备实现也仍保持分离。

## 对外接口

- `Platform::{new, dram, dram_mut, attach_device, attach_uart, build}`。
- `PlatformError`：空区域、地址溢出、区域重叠和非法复位向量。
- `Machine::from_address_space`：接入已经实现完整地址分发的测试或自定义平台。
- `Machine::{reset, step, run}` 以及公开的 `cpu` 状态。

## 耦合方式

- `Platform` 依赖正式 `Dram`、`Uart`、`Bus` 和 `MemDevice`。
- `Machine` 依赖 `Cpu`；时钟仍由 `Cpu::step` 推进，中断仍由 CPU 查询地址空间的 `pending_interrupt`。
- 测试平台没有被提升为正式设备；它只复用 `Machine` 容器，不复用正式 `Platform/Bus/Uart`。

## 剩余边界

- `Machine` 当前只有单 hart。
- `reset()` 只复位 CPU；总线、UART 输入、PLIC、virtio 和未来 CLINT 等设备没有统一 reset hook，状态会保留。
- `run()` 不循环调用 `Machine::step()`，直接访问公开 `cpu` 也无法阻止调用方绕过未来的设备 tick/interrupt 同步。
- `Platform::build` 只要求复位向量位于 DRAM，并把 DRAM 末端直接作为 SP；未检查指令对齐，也未按 ABI 对自定义容量产生的 SP 做 16 字节对齐。
- 平台设备仍以 `Box<dyn MemDevice>` 挂载，尚无类型化中断控制器接口。
- DRAM 在平台创建时一次性分配，不处理可恢复的宿主内存分配失败。
