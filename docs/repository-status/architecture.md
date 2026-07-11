# 仓库与 crate 边界

## 功能与实现思路

- `src/` 同时构建库 `arvsim` 和可配置 CLI。核心采用“loader + `Platform` 地址空间 + `Machine` + CPU 解释器”的结构。
- `tests/` 不只是断言，还包含一套比正式库更完整的 xv6 测试机器。
- `scripts/` 负责获取外部 xv6 源码、构建 fixture、编排测试和生成临时交互运行器。
- `Cargo.toml` 使用 Rust 2024 edition，包版本为 `0.1.0`，没有第三方 crate 依赖。

## 当前状态

`Platform` 已成为正式 CLI 的地址空间组装边界：它验证 DRAM、MMIO 和复位向量，再构造 `Bus` 与 `Machine`。`Machine` 目前只是 CPU facade，`step/run/reset` 都转发给 CPU；设备没有独立 tick/reset 接口，测试 helper 和 xv6 runner 仍直接调用公开的 `cpu.step()`。因此组装边界已经建立，运行期生命周期边界尚未真正收口。指令执行与 CPU 状态仍强耦合，正式设备与测试设备也仍然分裂。

## 对外接口

- Rust 库：`arvsim::{bus,cfg,clint,cpu,csr,dram,instruction,loader,machine,plic,trap,uart}`。
- 可执行程序：`cargo run -- [OPTIONS] <IMAGE>`，支持 flat/ELF、运行限制、调试和 DRAM/UART 配置，见 [`main.md`](./main.md)。
- 测试入口：`cargo test` 和 `scripts/run_testbench.sh`。

## 耦合方式

- 所有内存和 MMIO 错误统一依赖 `trap::Exception`。
- `cfg` 被 CPU、DRAM、CLI、指令快速路径和测试机器共同引用，是全局平台常量源。
- CLI 通过 `Platform::build` 创建 `Machine`；测试支撑通过 `Machine::from_address_space` 注入自有 `TestBus`。两者共享容器类型但仍使用不同设备实现，而且部分测试路径绕过 `Machine::step`。

## 主要问题

- 测试代码承担产品级设备实现，导致能力归属和验证范围不清晰。
- `Machine` 不持有平台时钟、clocked devices 或 reset hooks；将来接入 CLINT/virtio 异步状态时，当前直接调用 CPU 的路径会绕过设备推进。
- 运行时 DRAM 基址/容量由 `Platform` 接收，但 CPU 的用户态启发式和指令快速路径仍读取 `cfg` 默认常量，自定义平台并未完全去全局化。
- `target/testbench` 同时保存克隆仓库、fixture 和生成代码，可复现性依赖网络、外部分支和本机工具链。
