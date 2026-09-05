# 总线与内存审查记录

2026-09-05 复核了区域挂载、访问宽度、地址溢出、完整范围检查、中断合并、复位和周期推进。

本轮为 `MemDevice` 增加可选的 `reservation_epoch`，`Bus` 与 `Shared<T>` 转发到底层 DRAM，使 CPU 能观察 DMA 写入导致的 LR/SC 保留失效。默认设备不提供保留能力。DRAM 的多种访问共用范围检查，并限制外部直接修改字节向量。

`Shared<T>` 仍采用 `Rc<RefCell<T>>`，只适用于单线程，重叠的可变借用会产生运行时错误。这一限制属于公共使用约束，见 [总线](../docs/repository-status/bus.md)和 [DRAM](../docs/repository-status/dram.md) 文档。
