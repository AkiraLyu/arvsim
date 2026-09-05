# PLIC 审查与修复

2026-09-05 已修复上轮确认的两项规范偏差。

1. 原实现让 claim 与中断通知共用阈值过滤。现在通知仍检查 `priority > threshold`，claim 只筛选待处理、已使能且优先级非零的源。即使阈值屏蔽通知，软件仍可通过 claim 轮询。
2. 原实现只允许最初认领请求的上下文完成请求。现在 completion 检查写入上下文当前的使能位，删除 `claimed_by`。未使能的源写入被忽略，其他已使能上下文可以完成请求。

回归测试覆盖阈值屏蔽、零优先级、禁用源、认领后禁用、跨上下文完成及电平重新触发。仲裁、通知和完成沿用同一源状态，避免重复维护所有权。

依据：[PLIC Interrupt Claims](https://docs.riscv.org/reference/plic/v1.0.0/plic-claims.html)、[Interrupt Completion](https://docs.riscv.org/reference/plic/plic-completion.html)。实现状态见 [PLIC 文档](../docs/repository-status/plic.md)。
