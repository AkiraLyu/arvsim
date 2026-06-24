# `src/bus.rs`：内存设备抽象和总线

## 设计

- `MemDevice` 是所有内存映射设备的最小接口。
- `Bus` 负责按基址把 CPU 读写分发到 DRAM 或 UART 类设备。
- 中断查询用 `pending_interrupt()` 放在 `MemDevice` 默认方法里，避免为测试设备另建一套总线接口。

## 实现

- `Bus` 内部有两个 `BTreeMap<u64, Box<dyn MemDevice>>`：一个用于 RAM，一个用于 UART 类设备。
- 查找设备时选择不大于访问地址的最大基址，然后把原始物理地址交给设备处理。
- 未命中的读返回 `LoadAccessFault`，未命中的写返回 `StoreAMOAccessFault`。
- 当前 `Bus` 自身没有覆盖 `pending_interrupt()`，所以正式总线默认不会主动产生外部中断。

## 接口

- `trait MemDevice`
  - `read(&mut self, addr: u64, size: usize) -> Result<u64, Exception>`
  - `write(&mut self, addr: u64, value: u32, size: usize) -> Result<(), Exception>`
  - `pending_interrupt(&mut self) -> Option<u64>`
- `struct Bus`
  - `Bus::new()`
  - `attach_ram(base, dev)`
  - `attach_uart(base, dev)`

## 限制

- `write()` 的值参数是 `u32`。64 位写入由 CPU 或设备层拆成两次 32 位写。
- `Bus` 没有设备尺寸表，实际越界检查由设备自己完成。
- 设备分类目前只有 RAM 和 UART 两张表，正式平台设备还没有统一抽象。
