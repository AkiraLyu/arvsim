# 验证记录

## 本轮执行结果

执行日期：2026-07-10。分析基线：提交 `2f64e7a` 加当前工作树中的 CLI、loader、CPU/DRAM API、测试和文档变更。

### `cargo test`

结果：通过。

- 库单元测试：15 passed。
- `src/main.rs`：4 passed。
- `tests/cli.rs`：3 passed。
- `tests/rv64i_smoke.rs`：2 passed，1 ignored。
- `tests/xv6_fixture.rs`：1 passed，4 ignored。
- doc tests：0 tests。

完成 CLI 优化后默认回归一共实际执行 25 个测试：库 15 个、CLI 单元 4 个、CLI 进程集成 3 个、RV64 集成 2 个、fixture 检查 1 个；另有 5 个测试被忽略。新增覆盖包括 flat/ELF 装载、BSS 清零、参数解析、区域重叠、真实进程退出码、`Cpu::run()` 步数限制和可配置复位/CSR 清理。仍未覆盖 CPU trap/MMU/interrupt、正式 UART 输入、CLINT/PLIC 或 xv6 行为合同。

`cargo clippy --lib --bin arvsim -- -D warnings` 通过。`cargo clippy --all-targets -- -D warnings` 仍被既有 `tests/support/mod.rs` 的两个 lint 阻断（单元素循环和可改为范围的 OR pattern），与本次 CLI 改动无关。

### `cargo test --test rv64i_smoke -- --ignored`

结果：失败。

`rv64i_memory_branch_and_x0_contract` 在第 9 步以 `IllegalInstruction(0)` 失败。汇编的成功控制流只执行 8 条指令，故该结果首先暴露的是测试步数/终止协议问题，不能据此断言 load/store/branch/jump 语义失败。

### 未执行项

未运行 `cargo test --test xv6_fixture -- --ignored`：四个测试需要外部 fixture，最长预算为 20 亿步，不适合作为本轮文档核验的即时命令。仓库历史文档曾记录通过结果，但当前默认测试不能复核该结论。

## 覆盖缺口

- CLI 已有参数单元测试和 3 个真实进程测试，但 ELF、调试输出、UART 平台与 guest exception 进程路径尚未覆盖。
- CPU 新增运行循环和 reset 单元测试；关键特权、trap、MMU 和中断语义仍主要依赖默认关闭的 xv6 黑盒测试。
- Bus 只测基本区域末端，没有重叠、溢出、零尺寸和中断顺序测试。
- 指令测试仅有解码和立即数小测试，大部分执行语义没有精确回归。
- 测试 PLIC/virtio 没有独立设备测试。
- 没有自动化 clippy/格式、Miri、fuzz、riscv-arch-test 或覆盖率门槛的仓库配置。
