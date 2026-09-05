# `src/plic.rs`：平台级中断控制器

PLIC 支持源优先级、待处理位、目标使能、阈值、同优先级最低编号优先，以及 claim/complete。电平网关只保留一个未完成请求；完成后若电平仍高，可再次提交请求。

claim 忽略阈值，只选择已使能且优先级非零的请求。阈值只影响中断通知。completion 按写入目标当前是否使能该源判断，不记录或要求原 claim 上下文。这两项分别依据 [PLIC claim](https://docs.riscv.org/reference/plic/v1.0.0/plic-claims.html) 和 [completion](https://docs.riscv.org/reference/plic/plic-completion.html) 规范。

`PlicLayout::SIFIVE` 对应 PLIC 1.0 的寄存器偏移，平台可以配置其他布局。寄存器仅接受自然对齐的 32 位访问；源 0 保留，待处理位由网关维护，保留寄存器读零、写忽略。

公共接口包括 `Plic::{new, base, size, source_count, source_line}`、`PlicLayout`、`PlicError` 和 `MAX_INTERRUPT_SOURCES`。设备通过 `InterruptLine` 输入电平，PLIC 通过 `InterruptSet` 输出 M/S 外部中断，不依赖 UART、virtio 或 xv6。

测试覆盖阈值屏蔽时的 claim、禁用与零优先级源、跨上下文 completion、电平重入、仲裁、布局和访问宽度。当前只支持单硬件线程的 M/S 上下文和电平网关；未提供边沿网关、MSI 或多硬件线程并发。
