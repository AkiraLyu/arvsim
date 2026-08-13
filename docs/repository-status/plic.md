# `src/plic.rs`：RISC-V PLIC

规范基准：[RISC-V Platform-Level Interrupt Controller Specification 1.0.0](https://docs.riscv.org/reference/plic/index.html)。

## 功能与实现

实现 RISC-V PLIC 1.0.0 的主要数据路径：中断源 0 保留、源优先级、全局待处理请求、每目标上下文使能位、优先级阈值、最高优先级仲裁、同优先级最低编号优先，以及 claim/complete 寄存器。电平源由 `InterruptLine` 驱动；网关在请求被 claim 后保持忙状态，收到实现接受的 completion 后才能再次转发仍处于高电平的请求。当前 claim 错误地应用 threshold，completion 也使用了与规范不同的上下文校验，详见下方限制。

PLIC 规范不规定寄存器物理布局。`PlicLayout` 因此把区域大小、priority/pending/enable/context 基址和步长全部参数化，并提供 `SIFIVE` 常量覆盖 QEMU `virt` 使用的常见布局。源数、最大优先级和上下文对应的 RISC-V 外部中断原因也由构造参数提供。

## 实现状态

部分实现。当前默认 xv6 工作负载使用 threshold 0，并由同一上下文完成自己 claim 的请求，因此现有路径可用；实现也支持多个目标上下文和最多 1023 个源。寄存器只接受自然对齐的 32 位访问，构造时会拒绝未对齐布局和重复的 hart 中断目标；优先级和阈值按配置的 WARL 掩码收敛，IP 写入无效，保留寄存器读零、写忽略。查询 hart 中断不会消费 claim，只有 guest 读取 claim 寄存器才清除对应 pending 位。

单元测试覆盖优先级、阈值通知、使能、同优先级仲裁、同一上下文的 claim/complete、电平重入、无效参数和 MMIO 宽度；尚未覆盖 threshold 屏蔽通知时的轮询 claim，以及 completion 的 enable 校验。正式 `VirtPlatform` 把 UART IRQ 10 和 virtio IRQ 1 接入同一 PLIC；xv6 启动、输入和磁盘测试均经过该路径。

## 公共接口

- `Plic::{new, base, size, source_count, source_line}` 与 `MemDevice` 实现。
- `PlicLayout` 和 `PlicLayout::SIFIVE`。
- `PlicError` 与 `MAX_INTERRUPT_SOURCES`。

## 依赖关系

依赖 `interrupt::InterruptLine` 采样设备电平，使用 `trap::{InterruptCause, InterruptSet}` 向 CPU 输出 M/S 外部中断，通过 `MemDevice` 暴露 MMIO 和生命周期。具体地址、IRQ 分配和上下文顺序由平台组装器决定，PLIC 不引用 UART、virtio 或 xv6。

## 已知限制

- claim 与中断通知共用 `priority > threshold` 过滤；规范要求 claim 忽略 threshold，因此当前不能在用最大阈值关闭通知时轮询 claim。
- completion 只接受 `claimed_by` 记录的原上下文；规范要求按写入目标当前是否 enable 该源决定是否接受，不要求 ID 匹配最后一次 claim。
- 当前只建模电平触发网关；边沿触发、MSI 转换和平台自定义网关行为尚未提供配置。
- CPU 仍只有一个 hart；数据结构支持多个上下文，但尚无多 hart 并发访问与仲裁测试。
- `PlicLayout::SIFIVE` 是常见平台约定，不是 PLIC 规范强制的地址映射。
