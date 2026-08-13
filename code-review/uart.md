# `src/uart.rs`：16550 UART 审查记录

> 本文属于代码审查报告（基线：提交 `17ad107` 的当前工作区，2026-08-12），只记录本轮审查结论；未改动实现代码。总览见 [README](./README.md)。对应的现状文档：[uart.md](/home/akira/codespace/arvsim/docs/repository-status/uart.md)。

## 审查范围与总体判断

覆盖 8 字节寄存器窗口、DLAB、收发状态、RX/THRE 中断、可配置发送延迟、宿主后端和复位。上一轮的异常类型、初始化寄存器和原始字节输出问题已经修复，本轮未发现独立的 UART 状态机缺陷。

共 0 条发现：高危 0、中危 0、低危 0。

## 仍需保留的设计限制

模型只实现 xv6 所需的 16550 子集，标准输出后端逐字节刷新；`BufferedUartBackend` 为单线程设计。交互 runner 不释放已转发输出造成的累计内存问题记录在 [scripts.md](./scripts.md) #3。
