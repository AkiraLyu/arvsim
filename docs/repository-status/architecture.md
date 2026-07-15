# 仓库结构与代码组成

## 组成

- `src/` 同时构建 `arvsim` 库和命令行程序，主要由镜像装载器、`Platform`、`Machine` 和 CPU 解释器组成。
- `tests/` 除了测试用例，还包含一套比库中平台更完整的 xv6 测试平台。
- `scripts/` 用于获取 xv6 源码、生成测试文件、运行测试和创建临时交互程序。
- `Cargo.toml` 使用 Rust 2024 版，包版本为 `0.1.0`，不依赖第三方 Rust 库。

## 实现状态

命令行程序通过 `Platform` 检查 DRAM、MMIO 和复位向量，再创建 `Bus` 与 `Machine`。`Machine` 目前只是 CPU 的简单封装，`step`、`run` 和 `reset` 都直接转发给 CPU。设备没有单独的时钟推进和复位接口，测试辅助代码与 xv6 临时运行程序仍会直接调用 `cpu.step()`。因此地址空间的组装已经统一，运行和复位尚未统一。指令模块仍会直接修改 CPU，库中设备与测试设备也各自维护一套实现。

## 公共接口

- Rust 库：`arvsim::{bus,cfg,clint,cpu,csr,dram,instruction,loader,machine,plic,trap,uart}`。
- 可执行程序：`cargo run -- [OPTIONS] <IMAGE>`，支持裸二进制/ELF64 镜像、运行限制、调试和 DRAM/UART 配置，见 [`main.md`](./main.md)。
- 测试入口：`cargo test` 和 `scripts/run_testbench.sh`。

## 依赖关系

- 所有内存和 MMIO 错误统一依赖 `trap::Exception`。
- CPU 默认构造、DRAM、命令行程序、部分 xv6 加速和测试平台会读取 `cfg` 中的默认地址。
- 命令行程序通过 `Platform::build` 创建 `Machine`；测试代码通过 `Machine::from_address_space` 注入 `TestBus`。两者使用同一种机器容器，但设备实现不同，部分测试也没有经过 `Machine::step`。

## 已知问题

- UART、PLIC 和 virtio 的较完整实现只在测试代码中，库本身无法直接使用，测试范围也容易被误解。
- `Machine` 不管理平台时钟，也没有设备推进和复位接口。以后接入 CLINT 或异步 virtio 时，直接调用 CPU 会跳过设备更新。
- `Platform` 接受自定义 DRAM 地址和容量，但部分 xv6 加速仍读取 `cfg` 默认值，自定义配置没有传到所有模块。
- `target/testbench` 同时保存克隆的仓库、xv6 测试文件和生成代码。结果是否可复现仍取决于网络、上游分支和本机工具链。
