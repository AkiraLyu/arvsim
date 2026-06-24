# 集成测试

## 设计

- 集成测试分为小型 RV64 冒烟测试和 xv6 验收测试。
- 长耗时 xv6 测试使用 `#[ignore]`，由脚本或手工命令显式运行。

## 实现

- `tests/rv64i_smoke.rs`
  - 编译一段小型汇编，验证 `addi` 单步执行。
  - 验证测试 UART 模型能捕获发送字节。
  - 保留一个默认忽略的 RV64 指令行为测试，覆盖 `x0`、加载/存储、分支和跳转。
- `tests/xv6_fixture.rs`
  - 如果 xv6 构件存在，检查内核 ELF、裸二进制和 `fs.img` 形态。
  - 默认忽略的验收测试覆盖启动到 shell、执行基础用户程序、运行快速 `usertests`、运行完整 `usertests`。
  - 通过 UART 输出检查 `panic:`、`FAILED`、`SOME TESTS FAILED` 等失败标记。
  - 步数预算可通过 `ARVSIM_XV6_*_STEPS` 环境变量调整。

## 接口

```sh
cargo test
cargo test --test rv64i_smoke -- --ignored
cargo test --test xv6_fixture -- --ignored
cargo test --release --test xv6_fixture xv6_runs_full_usertests_suite -- --ignored --nocapture
```

## 限制

- `#[ignore]` 的说明字符串里仍保留了 “future contract” 这样的旧说法；这只是测试标签文本，不影响当前已验证状态。
- 完整 xv6 `usertests` 在 debug 模式下耗时很长。
