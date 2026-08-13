# `src/instruction.rs`：指令解码与执行审查记录

> 本文属于代码审查报告（基线：提交 `17ad107` 的当前工作区，2026-08-12），只记录本轮审查结论；未改动实现代码。总览见 [README](./README.md)。对应的现状文档：[instruction.md](/home/akira/codespace/arvsim/docs/repository-status/instruction.md)。

## 审查范围与总体判断

覆盖 RV64I/M/A、已实现的 RVC 子集、系统指令、对齐、LR/SC、PC 提交和 memset 快速路径。上一轮的 C.LWSP、非法 W 型 M 扩展、MISC-MEM、C.LUI、C.MV HINT 和指针别名问题均已有针对性回归测试，本轮未发现新增问题。

共 0 条发现：高危 0、中危 0、低危 0。

## 仍需保留的设计限制

aq/rl、多 hart 内存顺序、完整 RVC、`wfi` 等待行为及 `sfence.vma` 的实际刷新语义尚未实现；这些属于已公开的能力缺口，继续由状态文档和路线图跟踪。
