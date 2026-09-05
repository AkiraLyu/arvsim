# `src/interrupt.rs`：设备中断线

`InterruptLine` 是一个可共享的电平信号。设备根据寄存器状态拉高或拉低中断线，PLIC 读取电平并记录待处理中断。中断线本身不保存中断号、寄存器地址或被模拟程序的内部状态。

公开接口为 `InterruptLine::{new, set, assert, deassert, is_asserted}`，并实现 `Clone` 和 `Default`。

内部使用 `Rc<Cell<bool>>`，适用于当前单线程、单硬件线程模型，不支持跨线程同步。识别中断请求、领取请求（claim）和报告处理完成（complete）由 PLIC 负责。
