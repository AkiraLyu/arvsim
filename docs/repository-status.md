# 当前实现说明

本文档是 arvsim 当前实现状态的索引页。模块级说明已经拆到 `docs/repository-status/` 下；每个模块都包含当前设计、实现、接口和限制。

## 总体状态

arvsim 现在是一个单 hart、解释执行式的 RV64 模拟器。主机端可以运行小型裸二进制程序；测试支撑代码可以构造接近 QEMU `virt` 布局的 xv6 运行环境，并已验证 xv6-riscv 可以启动 shell、运行基础用户程序、通过快速 `usertests` 和完整 `usertests`。

稳定测试入口：

```sh
scripts/run_testbench.sh
```

xv6 长测试默认仍标记为 `#[ignore]`，原因是耗时长，不适合作为默认 `cargo test` 的一部分。

## 模块文档

- [目录和 crate 边界](./repository-status/layout.md)
- [机器常量：`src/cfg.rs`](./repository-status/cfg.md)
- [异常类型：`src/trap.rs`](./repository-status/trap.md)
- [内存设备抽象和总线：`src/bus.rs`](./repository-status/bus.md)
- [主存：`src/dram.rs`](./repository-status/dram.md)
- [正式 UART 模型：`src/uart.rs`](./repository-status/uart.md)
- [CSR 状态：`src/csr.rs`](./repository-status/csr.md)
- [指令解码和执行：`src/instruction.rs`](./repository-status/instruction.md)
- [CPU、地址翻译和 xv6 运行时支撑：`src/cpu.rs`](./repository-status/cpu.md)
- [平台设备占位模块：`src/clint.rs`、`src/plic.rs`](./repository-status/platform-devices.md)
- [命令行运行入口：`src/main.rs`](./repository-status/main.md)
- [测试机器和 xv6 设备模型：`tests/support/mod.rs`](./repository-status/test-support.md)
- [集成测试](./repository-status/tests.md)
- [脚本](./repository-status/scripts.md)
- [验证结果和后续工作](./repository-status/verification.md)
