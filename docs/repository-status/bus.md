# `src/bus.rs`：内存设备抽象与总线

## 功能

`MemDevice` 统一表示物理内存和 MMIO 设备的读写、复位、时钟推进和中断查询接口。`Bus` 用 `BTreeMap` 保存互不重叠的半开地址区间，并把一次完整访问转发给对应设备。总线会合并全部设备同时有效的 `InterruptSet`，并把复位和周期推进广播给全部设备。`Shared<T>` 为单线程平台提供可克隆句柄，使总线、DMA 设备和宿主观察接口可以引用同一正式设备对象。

## 实现状态

地址分发和生命周期广播已经可用。总线只接受 1、2、4、8 字节访问，并检查地址加长度是否溢出，以及整个访问是否落在同一设备区域。挂载设备时会检查区域非空、地址溢出和区域重叠，并以 `BusError` 返回可恢复错误；找不到设备时分别返回读或写访问错误。`write` 接收 `u64`，因此 8 字节写只需一次设备调用。设备接口约定先验证完整访问，失败时不得留下部分写入或 MMIO 副作用。`reset` 和 `tick` 有默认空实现，现有 RAM 和同步设备无需维护空方法。

## 公共接口

- `MemDevice` 接口（Rust `trait`）
  - `read(&mut self, addr, size) -> Result<u64, Exception>`
  - `write(&mut self, addr, value: u64, size) -> Result<(), Exception>`
  - `pending_interrupts(&mut self) -> InterruptSet`，默认没有中断
  - `reset(&mut self)`，默认不改变状态
  - `tick(&mut self, cycles)`，默认不推进状态
- `Bus::{new, attach_device, attach_uart}`；两个挂载入口都返回 `Result<(), BusError>`，RAM 必须通过 `attach_device` 明确给出实际大小。
- `BusError::{EmptyRegion, AddressOverflow, RegionOverlap}`。
- `Shared<T>`：基于 `Rc<RefCell<T>>` 的单线程共享句柄；当 `T: MemDevice` 时自动转发设备接口。
- `DeviceRegion { base, size, dev }`
- `Bus: Default + MemDevice`

## 依赖关系

- 依赖 `trap::Exception`；`attach_uart` 依赖 `cfg::UART_SIZE`。
- `Cpu` 持有 `Box<dyn MemDevice>`；`Machine` 通过这份地址空间统一复位和推进设备。
- DRAM、UART、PLIC 和 virtio-blk 都实现 `MemDevice`。PLIC 返回按 `mip` 位表示的中断集合，避免把 `mcause` 最高位编码泄漏进设备接口。

## 已知问题与改进建议

- `reset` 和 `tick` 不能返回错误，也没有事件期限或休眠语义；复杂异步设备仍只能按固定机器步长轮询。
- `size` 仍是 `usize`，调用方可以传入非法值，只是会收到访问错误。后续可改用 `AccessSize`，让非法宽度无法构造。
- `Shared<T>` 使用运行时借用检查且不是线程安全句柄；调用方运行机器时不能同时持有同一对象的借用。
