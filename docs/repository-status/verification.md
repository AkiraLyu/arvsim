# 验证结果和后续工作

## 当前验证结果

已验证命令：

```sh
scripts/run_testbench.sh
cargo test --test xv6_fixture xv6_runs_quick_usertests -- --ignored --nocapture
cargo test --release --test xv6_fixture xv6_runs_full_usertests_suite -- --ignored --nocapture
cargo test --release --test xv6_fixture xv6_shell_runs_basic_user_programs -- --ignored --nocapture
git diff --check
```

验证结果：

- 稳定测试套件通过。
- xv6 快速 `usertests` 在 debug 模式的测试框架下通过。
- xv6 完整 `usertests` 在 release 模式的测试框架下通过，并输出 `ALL TESTS PASSED`。
- shell 基础用户程序在 release 模式的测试框架下通过。
- diff 空白检查通过。

## 主要边界和后续工作

- 把 `src/clint.rs`、`src/plic.rs` 从占位文件扩展为正式设备模型。
- 将测试支撑中的 PLIC、virtio、UART 输入能力迁移或抽象到正式机器模型。
- 将 xv6 专用加速路径从硬编码地址改为符号驱动、配置开关或可替换策略。
- 建模完整特权级状态机、CSR 权限、委托位过滤和多 hart 原子语义。
- 引入官方 riscv-tests 或更系统的 ISA 规范一致性测试。
- 清理测试和脚本中的旧 “future contract” 标签文本，使命名和当前状态一致。
