# `src/bus.rs`：内存设备抽象与总线

## 功能

`MemDevice` 统一表示物理内存和 MMIO 设备的读写接口。`Bus` 用 `BTreeMap` 保存互不重叠的半开地址区间，并把一次完整访问转发给对应设备。总线还会依次查询设备是否有待处理中断。

## 实现状态

地址分发已经可用。挂载设备时会检查区域非空、地址溢出和区域重叠；找不到设备时分别返回读或写访问错误。所有设备都保存在同一张区域表中。直接调用 `Bus::attach_device` 时，非法布局会使进程异常退出；命令行程序通过 `Platform` 将这些情况转换为 `PlatformError`。

## 公共接口

- `MemDevice` 接口（Rust `trait`）
  - `read(&mut self, addr, size) -> Result<u64, Exception>`
  - `write(&mut self, addr, value: u32, size) -> Result<(), Exception>`
  - `pending_interrupt(&mut self) -> Option<u64>`，默认没有中断
- `Bus::{new, attach_device, attach_ram, attach_uart}`
- `DeviceRegion { base, size, dev }`
- `Bus: Default + MemDevice`

## 依赖关系

- 依赖 `trap::Exception`；`attach_ram` 依赖 `cfg::DRAM_SIZE`。
- `Cpu` 只持有 `Box<dyn MemDevice>`，因此既可接库中的 `Bus`，也可接测试使用的 `TestBus`。
- DRAM 和 UART 都实现 `MemDevice`；设备用未经封装的 `u64` 返回中断原因。

## 已知问题与改进建议

- `write` 只能接收 `u32`。64 位写会拆成两次，设备无法把它视为一次完整操作。
- `size` 没有限制为 1、2、4 或 8，长度为 0 的访问也可能命中设备。
- `attach_device` 是公共接口，却会因空区域、溢出或重叠而使进程异常退出；绕过 `Platform` 的调用方无法正常处理这些错误。
- `attach_ram` 固定声明 128 MiB，无法表达实际设备大小；`attach_uart` 固定 256 字节。
- `pending_interrupt` 按设备地址顺序返回第一个中断，没有优先级判断。
- `MemDevice` 没有时钟推进和复位接口，难以支持 CLINT 或异步设备。
- 建议将写入值改为 `u64`，用 `AccessSize` 限制访问宽度，让设备声明自己的地址区域，并为中断、时钟推进和复位分别定义接口。挂载失败也应返回错误，而不是直接终止进程。
