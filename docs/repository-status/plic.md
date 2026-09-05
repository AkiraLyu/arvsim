# `src/plic.rs`：平台级中断控制器

PLIC 管理中断源的优先级、待处理状态，以及各中断目标的启用配置和通知阈值。优先级相同时，先选择编号较小的中断源。

软件通过读取 claim 寄存器获取待处理的中断号，处理完后将中断号写回 complete 寄存器。每个电平中断源最多保留一个未完成请求；报告完成时电平仍为高，就会再次产生请求。

## 请求选择与完成

claim 从当前目标已启用、优先级非零的待处理中断中选择最高优先级的请求，不受通知阈值限制。阈值只决定是否向 CPU 发送中断通知，因此即使通知被屏蔽，软件仍可主动读取 claim。

收到 complete 写入时，PLIC 检查该中断源是否仍被写入目标启用。只要满足条件，就接受完成通知，不要求写入目标与最初读取 claim 的目标相同。这两项规则分别依据 [PLIC claim](https://docs.riscv.org/reference/plic/v1.0.0/plic-claims.html) 和 [completion](https://docs.riscv.org/reference/plic/plic-completion.html) 规范。

## 寄存器、接口与限制

`PlicLayout::SIFIVE` 使用 PLIC 1.0 的寄存器偏移，也可为其他平台指定布局。寄存器只接受按 4 字节对齐的 32 位访问。源 0 保留；待处理位由 PLIC 的中断输入逻辑维护，保留寄存器读取返回 0、写入不改变状态。

公开接口包括 `Plic::{new, base, size, source_count, source_line}`、`PlicLayout`、`PlicError` 和 `MAX_INTERRUPT_SOURCES`。设备通过 `InterruptLine` 提供电平信号，PLIC 通过 `InterruptSet` 报告机器模式或监督模式外部中断，不依赖 UART、Virtio 或 xv6。

测试覆盖通知被屏蔽时读取 claim、禁用或零优先级中断源、不同目标之间的完成通知、电平持续为高时重新产生请求，以及优先级选择、地址布局和访问宽度。当前只支持单硬件线程的机器模式和监督模式中断目标，以及电平触发的输入；未支持边沿触发、消息中断（MSI）或多硬件线程并发。
