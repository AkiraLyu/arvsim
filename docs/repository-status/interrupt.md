# `src/interrupt.rs`：设备中断线

## 功能与实现

`InterruptLine` 是可克隆的共享电平信号。设备负责根据自身寄存器状态拉高或拉低线路，PLIC 负责采样和锁存，二者不共享 IRQ 编号、寄存器地址或 guest 私有状态。

## 公共接口

- `InterruptLine::{new, set, assert, deassert, is_asserted}`。
- `Clone + Default`。

## 依赖关系与限制

内部使用 `Rc<Cell<bool>>`，适合当前单线程、单 hart 模型，不提供跨线程同步。边沿锁存和 claim/complete 属于 PLIC 网关职责，不在该类型内实现。

