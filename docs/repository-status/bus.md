# `src/bus.rs`：内存、设备接口与总线

## 功能

`MemDevice` 定义物理内存和内存映射 I/O（MMIO）设备共用的读写、复位、时钟更新和中断查询接口。`Bus` 根据地址把访问交给对应设备，并合并各设备报告的中断。机器复位或更新时钟时，总线会依次通知所有设备。

总线用 `BTreeMap` 保存互不重叠的地址区间，区间包含起始地址，不包含结束地址。`Shared<T>` 让总线、DMA 设备和主机上的调用方共享同一个设备对象，只适用于单线程。

## 访问规则

总线只接受 1、2、4、8 字节访问。一次访问必须完整落在同一设备区域内，地址加长度也不能溢出。添加设备时会检查区域是否为空、是否溢出、是否与已有区域重叠；失败时返回 `BusError`。访问未映射地址时，返回相应的读或写访问错误。

`write` 接收 `u64`，所以 8 字节写入只调用一次设备方法。设备应先检查完整访问范围；访问失败时，不得只写入部分数据或改变 MMIO 状态。`reset` 和 `tick` 默认不做任何操作，设备只需实现自己需要的行为。

## 公开接口

| `MemDevice` 方法 | 用途 |
| --- | --- |
| `read(&mut self, addr, size) -> Result<u64, Exception>` | 读取指定地址和长度的数据 |
| `write(&mut self, addr, value: u64, size) -> Result<(), Exception>` | 写入指定长度的数据 |
| `reservation_epoch(&mut self, addr, size) -> Option<u64>` | 查询 LR/SC 保留区域的写入版本号；默认不支持保留 |
| `pending_interrupts(&mut self) -> InterruptSet` | 查询当前中断；默认没有中断 |
| `reset(&mut self)` | 复位设备；默认不改变状态 |
| `tick(&mut self, cycles)` | 按经过的周期数更新设备；默认不改变状态 |

支持 LR/SC 的设备必须在 CPU 或 DMA 写入保留区域后更新版本号。

- `Bus::{new, attach_device, attach_uart}`：两个设备添加方法均返回 `Result<(), BusError>`。添加 RAM 时，必须通过 `attach_device` 指定实际大小。
- `BusError::{EmptyRegion, AddressOverflow, RegionOverlap}`：分别表示空区域、地址溢出和区域重叠。
- `Shared<T>`：基于 `Rc<RefCell<T>>`。当 `T: MemDevice` 时，会把接口调用转交给共享的设备。
- `DeviceRegion { base, size, dev }`：保存设备地址范围和设备对象。
- `Bus` 实现 `Default` 和 `MemDevice`。

## 依赖关系与限制

`Cpu` 通过 `Box<dyn MemDevice>` 访问地址空间，`Machine` 通过同一接口复位设备和更新时钟。DRAM、UART、PLIC 和 Virtio 块设备都实现了该接口。访问错误使用 `trap::Exception`；`attach_uart` 使用 `cfg::UART_SIZE`。

设备报告的是对应 `mip` 位的中断集合。`mcause` 最高位是中断标志，由 CPU 编码，不应混入设备报告的位集合。

`reset` 和 `tick` 不能返回错误，也不能安排下一次唤醒时间，异步设备仍需随机器单步接受检查。访问宽度使用 `usize`，非法值只能在运行时被拒绝。`Shared<T>` 不支持跨线程共享；运行机器时，调用方不能同时持有会与设备访问冲突的借用。
