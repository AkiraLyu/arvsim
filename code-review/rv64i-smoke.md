# `tests/rv64i_smoke.rs`：RV64 与正式平台冒烟测试审查记录

> 本文属于代码审查报告（基线：提交 `17ad107` 的当前工作区，2026-08-12），只记录本轮审查结论；未改动实现代码。总览见 [README](./README.md)。对应的现状文档：[rv64i-smoke.md](/home/akira/codespace/arvsim/docs/repository-status/rv64i-smoke.md)。

## 审查范围与总体判断

覆盖外部工具链构建、单步 `addi`、UART、virtio 队列参数、机器复位、总线边界及带明确签名的 RV64I 小程序。测试已经复用正式 `VirtPlatform` 并使用 Rust 常量生成 guest 地址，本轮未发现新增问题。

共 0 条发现：高危 0、中危 0、低危 0。

逐条 ISA 覆盖仍不足，且默认执行依赖外部 RISC-V 工具链；这些属于工程与覆盖缺口，继续由验证文档跟踪。
