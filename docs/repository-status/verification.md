# 验证记录

## 本轮执行结果

执行日期：2026-07-11。分析基线：提交 `008ae8b` 加当前未提交工作树；验证命令针对实际工作树执行。

### 默认与静态检查

`cargo test --all-targets` 通过：

- 库单元测试：20 passed。
- `src/main.rs`：3 passed。
- `tests/cli.rs`：3 passed。
- `tests/rv64i_smoke.rs`：3 passed。
- `tests/xv6_fixture.rs`：1 passed，4 ignored。
- 合计 30 passed、4 ignored、0 failed。

其他检查：

- `cargo test --doc`：通过，当前 0 个 doc test。
- `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`：通过。
- `cargo fmt --all -- --check`：通过。
- `cargo clippy --lib --bin arvsim -- -D warnings`：通过。
- `cargo clippy --all-targets -- -D warnings`：通过。
- `bash -n scripts/build_xv6_fixture.sh scripts/run_testbench.sh scripts/run_xv6_cli.sh`：通过。
- `git diff --check`：通过。
- `docs/**/*.md` 相对链接与空文件检查：通过；原有两个空白占位文档已移除，索引不再指向空页面。

默认回归覆盖 flat/ELF 装载、BSS 清零、参数解析、平台区域冲突、真实 CLI 退出码、CPU 运行限制/reset、CSR 别名、基础指令合同和测试 UART。它没有覆盖完整特权状态机、关键 trap/MMU/interrupt 语义、正式 UART 输入、CLINT/PLIC 或 xv6 行为合同。

### 已有 xv6 回归记录（本轮未复核）

2026-07-10 曾针对 fixture commit `1982fd12595f52a0e5ef8db466257a01fb1fbfef` 验证动态解析 `Xv6Accelerator` 符号后的启动与基础 shell 合同：`run_xv6_cli.sh --boot-only` 到达 shell，启动测试与 `echo/ls/cat README` 合同通过。这些结果说明当时的 fixture 可用，但不作为 2026-07-11 当前工作树的重新执行证据。

### 未执行项

未运行 `cargo test --test xv6_fixture -- --ignored`：四个测试需要外部 fixture，最长预算为 20 亿步，不适合作为本轮状态扫描的即时命令。默认执行的 fixture 完整性测试在构件缺失时会返回成功，因此其他环境中的绿色默认测试不能证明 xv6 fixture 存在。

## 覆盖缺口

- CLI 已有参数单元测试和 3 个真实进程测试，但 ELF、调试输出、UART 平台与 guest exception 进程路径尚未覆盖。
- CPU 有运行循环和 reset 单元测试；零偏移控制流、关键特权、trap、MMU 和中断语义没有精确回归。
- Bus 只测基本读写与区域末端；重叠/溢出通过 `Platform` 间接覆盖，零宽/非法宽度、中断顺序和直接 `Bus::attach_device` panic 合同未覆盖。
- 指令测试仅有解码和立即数小测试，大部分执行语义没有精确回归。
- `Machine` 测试证明转发和组装可用，但不覆盖设备 tick/reset；测试 helper 仍直接调用 CPU。
- 测试 PLIC/virtio 没有独立设备测试，畸形访问或 descriptor 可导致 panic/错误类型混用。
- 格式和 clippy 当前手工通过，但仓库没有 CI gate，也没有 Miri、fuzz、riscv-arch-test 或覆盖率门槛。
