# 仓库与 crate 边界

## 功能与实现思路

- `src/` 同时构建库 `arvsim` 和可配置 CLI。核心采用“loader + CPU 解释器 + `MemDevice` 地址空间”的结构。
- `tests/` 不只是断言，还包含一套比正式库更完整的 xv6 测试机器。
- `scripts/` 负责获取外部 xv6 源码、构建 fixture、编排测试和生成临时交互运行器。
- `Cargo.toml` 使用 Rust 2024 edition，包版本为 `0.1.0`，没有第三方 crate 依赖。

## 当前状态

分层边界已经出现，但还不稳定：指令执行与 CPU 状态强耦合；平台设备在正式代码与测试支撑中分裂；xv6 优化逻辑进入通用 CPU 核心。CLI 与测试现已共用 `step()` 执行语义，但仍使用不同的平台设备组合。

## 对外接口

- Rust 库：`arvsim::{bus,cfg,clint,cpu,csr,dram,instruction,plic,trap,uart}`。
- 可执行程序：`cargo run -- [OPTIONS] <IMAGE>`，支持 flat/ELF、运行限制、调试和 DRAM/UART 配置，见 [`main.md`](./main.md)。
- 测试入口：`cargo test` 和 `scripts/run_testbench.sh`。

## 耦合方式

- 所有内存和 MMIO 错误统一依赖 `trap::Exception`。
- `cfg` 被 CPU、DRAM、CLI、指令快速路径和测试机器共同引用，是全局平台常量源。
- 测试支撑通过 `Box<dyn MemDevice>` 注入 CPU，但没有复用正式 `Bus`、`Uart` 或平台设备。

## 主要问题

- 缺少稳定的 `Machine`/`Platform` 组装层和运行控制 API。
- 测试代码承担产品级设备实现，导致能力归属和验证范围不清晰。
- `target/testbench` 同时保存克隆仓库、fixture 和生成代码，可复现性依赖网络、外部分支和本机工具链。
