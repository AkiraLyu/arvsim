# `src/bus.rs`：内存设备抽象与总线

## 功能与实现思路

`MemDevice` 把 CPU 的物理地址读写抽象成统一 trait；`Bus` 用以基址为键的 `BTreeMap` 保存不重叠的半开区间，并把访问转发给包含完整访问宽度的设备。总线也轮询设备的待处理中断。

## 当前状态

核心分发已实现。挂载时检查非零尺寸、地址溢出以及与前后区域重叠；访问未命中分别返回 load/store access fault。与旧设计不同，当前所有设备已统一保存在一张区域表中。

## 对外接口

- `trait MemDevice`
  - `read(&mut self, addr, size) -> Result<u64, Exception>`
  - `write(&mut self, addr, value: u32, size) -> Result<(), Exception>`
  - `pending_interrupt(&mut self) -> Option<u64>`，默认无中断
- `Bus::{new, attach_device, attach_ram, attach_uart}`
- `DeviceRegion { base, size, dev }`
- `Bus: Default + MemDevice`

## 耦合方式

- 依赖 `trap::Exception`；`attach_ram` 依赖 `cfg::DRAM_SIZE`。
- `Cpu` 只持有 `Box<dyn MemDevice>`，因此既可接正式 `Bus`，也可接测试 `TestBus`。
- DRAM 和 UART 实现该 trait；中断 cause 以裸 `u64` 由设备上传。

## 不完善之处和优化方向

- `write` 只有 `u32` 值，64 位写被上层拆成两次，破坏设备看到的访问原子性。
- `size` 未限制为 1/2/4/8，零宽访问也可能命中。
- `attach_ram` 固定声明 128 MiB，无法表达实际设备大小；`attach_uart` 固定 256 字节。
- `pending_interrupt` 按地址顺序取第一个，不具备优先级/仲裁语义。
- 建议改用 `u64` 写值和显式 `AccessSize`，让设备报告区域描述，定义类型化中断，并加入设备移除、只读查询和更完整的边界测试。
