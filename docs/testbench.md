# 测试套件说明

本项目的最终目标是让模拟器能够成功运行 MIT 的 xv6-riscv。因此，测试套件围绕 xv6 在 QEMU `virt` 机器上依赖的硬件接口行为设计，同时明确暴露当前模拟器尚未实现的能力。

## 测试分层

1. 主机端单元测试：测试 DRAM、Bus 分发、UART 状态、CSR 映射、指令解码等纯组件行为。
2. RV64 冒烟测试：集成测试使用 `riscv64-elf-gcc` 编译小型 RISC-V 程序，再转换为裸二进制文件，加载到 `0x80000000`，并通过 CPU 单步接口做有限步执行。
3. RISC-V 指令行为测试：默认忽略的测试用例描述 xv6 所需的基础 RV64 行为，例如 `x0` 恒为 0、立即数解码、加载/存储、分支、跳转等。随着模拟器实现推进，可以逐步纳入常规测试。
4. xv6 测试环境：`scripts/build_xv6_fixture.sh` 会获取官方 `mit-pdos/xv6-riscv`，构建 `kernel/kernel` 和 `fs.img`，并额外生成当前裸二进制加载器可用的 `kernel/kernel.bin`。
5. xv6 启动与用户态验收测试：默认忽略的测试用例加载 xv6 内核镜像，并分阶段检查从内核启动信息、`init`、shell 提示符、基础用户程序，到 xv6 自带 `usertests` 的完整运行过程。

## xv6 机器接口

当前 xv6-riscv 以 `rv64gc` 构建，入口物理地址是 `0x80000000`。它使用 QEMU `virt` 风格的内存和外设布局：

- DRAM：`0x80000000..0x88000000`
- UART 16550a：`0x10000000`
- virtio-mmio 块设备：`0x10001000`
- PLIC：`0x0c000000`
- CLINT/定时器相关平台行为，以及 `time`、`stimecmp` 等 CSR

xv6 的最初几条指令不仅需要 RV64I，还需要压缩指令、`mul`、CSR 读写、`mret`、从机器模式切换到监督模式、定时器 CSR 和异常/中断委托。继续启动后，还会依赖页表、原子指令、PLIC、UART 中断和 virtio 磁盘行为。

## 运行命令

运行稳定测试套件：

```sh
scripts/run_testbench.sh
```

获取或刷新 xv6 测试环境。第三方 xv6 源码会放在 `target/testbench/` 下，不进入仓库提交：

```sh
scripts/build_xv6_fixture.sh
```

构建 xv6 测试环境后运行稳定测试：

```sh
scripts/run_testbench.sh --with-xv6-fixture
```

主动运行完整验收测试：

```sh
scripts/run_testbench.sh --future-contracts
```

`--future-contracts` 会运行默认测试、默认忽略的 RV64 指令行为测试和 xv6 验收测试。当前这些 xv6 验收测试已经可以通过，但完整 `usertests` 耗时很长，不建议放入日常短反馈流程。

只检查 xv6 验收测试：

```sh
scripts/run_testbench.sh --xv6-contracts
```

交互式启动 xv6 并进入 shell：

```sh
scripts/run_xv6_cli.sh
```

该脚本会复用测试套件中的 xv6 测试环境、PLIC、virtio 和 UART 模型，启动到 xv6 shell 后把终端输入转发到 xv6 的 UART 输入队列。按 `Ctrl-]` 退出。只想确认能启动到第一个 shell 提示符时可以运行：

```sh
scripts/run_xv6_cli.sh --boot-only
```

## xv6 完整运行覆盖范围

xv6 验收测试位于 `tests/xv6_fixture.rs`，默认标记为 `#[ignore]`。它们覆盖以下阶段：

- `xv6_kernel_reaches_first_shell`：从 `0x80000000` 入口启动，看到 `xv6 kernel is booting`、`init: starting sh` 和 shell 提示符 `$ `。
- `xv6_shell_runs_basic_user_programs`：通过 UART 输入执行 `echo`、`ls`、`cat README`，验证控制台输入、shell、用户程序、文件系统和 virtio 磁盘读取路径。
- `xv6_runs_quick_usertests`：执行 `usertests -q`，要求输出 `ALL TESTS PASSED`。
- `xv6_runs_full_usertests_suite`：执行完整 `usertests`，要求进入 slow tests 阶段并最终输出 `ALL TESTS PASSED`。这是“完整运行 xv6”的最终验收测试。

这些测试还会检查 UART 输出中不能出现 `panic:`、`FAILED`、`SOME TESTS FAILED`、`init: exec sh failed` 等失败标记。

步数预算可以通过环境变量调整，例如：

```sh
ARVSIM_XV6_FULL_USERTESTS_STEPS=4000000000 scripts/run_testbench.sh --xv6-contracts
```

耗时提示：

- `xv6_runs_quick_usertests` 已在 debug 模式的测试框架下通过，但需要较长时间。
- `xv6_runs_full_usertests_suite` 已在 release 模式的测试框架下通过；debug 模式下运行完整 `usertests` 预计需要数小时。
- 日常验证完整 xv6 行为时，推荐先运行：

```sh
cargo test --release --test xv6_fixture xv6_runs_full_usertests_suite -- --ignored --nocapture
```

## 工具依赖

当前环境已验证可用的关键工具包括：

- `cargo`
- `rustc`
- `riscv64-elf-gcc`
- `riscv64-elf-objcopy`
- `riscv64-elf-readelf`
- `make`
- `git`
- `perl`
- 系统 `gcc`

脚本不会自动执行需要 root 权限的安装命令。如果缺工具，它只会打印需要手动执行的 Arch Linux 安装命令。

## 当前状态

稳定测试套件通过；xv6 测试环境可以构建出：

- `target/testbench/xv6-riscv/kernel/kernel`
- `target/testbench/xv6-riscv/kernel/kernel.bin`
- `target/testbench/xv6-riscv/fs.img`

当前 xv6 验收测试默认保持 `#[ignore]`，原因只是避免默认测试套件过慢。已验证状态：

- `scripts/run_testbench.sh` 通过。
- `cargo test --test xv6_fixture xv6_runs_quick_usertests -- --ignored --nocapture` 通过。
- `cargo test --release --test xv6_fixture xv6_runs_full_usertests_suite -- --ignored --nocapture` 通过。
- `cargo test --release --test xv6_fixture xv6_shell_runs_basic_user_programs -- --ignored --nocapture` 通过。

完整变更目的和实现分析见 [xv6 支持变更分析](./xv6-change-analysis.md)。
