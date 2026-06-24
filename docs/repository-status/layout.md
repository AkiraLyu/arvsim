# 目录和 crate 边界

## 设计

- `src/` 是正式库和命令行入口。
- `tests/` 是集成测试和 xv6 测试环境，不是正式平台模型。
- `scripts/` 负责构建 xv6 测试环境、运行测试套件和启动交互式 xv6。
- `docs/` 记录当前状态、测试说明和变更分析。

## 实现

- `Cargo.toml` 没有第三方依赖，当前实现只用 Rust 标准库。
- `src/lib.rs` 公开所有顶层模块：`bus`、`cfg`、`clint`、`cpu`、`csr`、`dram`、`instruction`、`plic`、`trap`、`uart`。
- `src/main.rs` 是最小命令行程序：读取一个裸二进制文件，加载到 DRAM，挂载 UART，创建 CPU，然后进入 `Cpu::run()`。

## 接口

- 库接口通过 `arvsim::*` 模块暴露。
- 命令行接口：

```sh
cargo run -- <binary_file>
```

## 限制

- 正式命令行入口只挂载 DRAM 和简化 UART，不挂载 PLIC、CLINT 或 virtio。
- `Cpu::run()` 目前会持续打印调试信息，因为 `src/cpu.rs` 中 `DEBUG` 是常量 `true`。
