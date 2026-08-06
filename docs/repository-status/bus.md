# `src/bus.rs`：内存设备抽象与总线

## 功能

`MemDevice` 统一表示物理内存和 MMIO 设备的读写、复位、时钟推进和中断查询接口。`Bus` 用 `BTreeMap` 保存互不重叠的半开地址区间，并把一次完整访问转发给对应设备。总线还会按地址顺序查询中断，并把复位和周期推进广播给全部设备。

## 实现状态

地址分发和生命周期广播已经可用。总线只接受 1、2、4、8 字节访问，并检查地址加长度是否溢出，以及整个访问是否落在同一设备区域。挂载设备时会检查区域非空、地址溢出和区域重叠；找不到设备时分别返回读或写访问错误。`write` 接收 `u64`，因此 8 字节写只需一次设备调用。设备接口约定先验证完整访问，失败时不得留下部分写入或 MMIO 副作用。`reset` 和 `tick` 有默认空实现，现有 RAM 和同步设备无需维护空方法。

## 公共接口

- `MemDevice` 接口（Rust `trait`）
  - `read(&mut self, addr, size) -> Result<u64, Exception>`
  - `write(&mut self, addr, value: u64, size) -> Result<(), Exception>`
  - `pending_interrupt(&mut self) -> Option<u64>`，默认没有中断
  - `reset(&mut self)`，默认不改变状态
  - `tick(&mut self, cycles)`，默认不推进状态
- `Bus::{new, attach_device, attach_ram, attach_uart}`
- `DeviceRegion { base, size, dev }`
- `Bus: Default + MemDevice`

## 依赖关系

- 依赖 `trap::Exception`；`attach_ram` 依赖 `cfg::DRAM_SIZE`，`attach_uart` 依赖 `cfg::UART_SIZE`。
- `Cpu` 持有 `Box<dyn MemDevice>`，因此既可接库中的 `Bus`，也可接测试使用的 `TestBus`；`Machine` 通过这份地址空间统一复位和推进设备。
- DRAM 和 UART 都实现 `MemDevice`；设备用未经封装的 `u64` 返回中断原因。

## 已知问题与改进建议

- `attach_device` 是公共接口，却会因空区域、溢出或重叠而使进程异常退出；绕过 `Platform` 的调用方无法正常处理这些错误。
- `attach_ram` 固定声明 128 MiB，无法表达实际设备大小。
- `pending_interrupt` 按设备地址顺序返回第一个中断，没有优先级判断。
- `reset` 和 `tick` 不能返回错误，也没有事件期限或休眠语义；复杂异步设备仍只能按固定机器步长轮询。
- `size` 仍是 `usize`，调用方可以传入非法值，只是会收到访问错误。后续可改用 `AccessSize`，让非法宽度无法构造。
- 建议让设备声明自己的地址区域，并为中断、时钟推进和复位分别定义接口。挂载失败也应返回错误，而不是直接终止进程。
