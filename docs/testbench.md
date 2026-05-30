# Testbench 说明

本项目的最终目标是让模拟器能够成功运行 MIT xv6-riscv。因此 testbench
围绕 xv6 在 QEMU `virt` 机器上依赖的硬件契约来设计，同时把当前模拟器尚未实现的能力清楚地暴露出来。

## 分层设计

1. 主机单元测试层：测试 DRAM、Bus 分发、UART 状态、CSR 映射、指令解码等纯组件行为。
2. RV64 smoke 层：集成测试使用 `riscv64-elf-gcc` 编译很小的 RISC-V 程序，再转成 flat binary，加载到 `0x80000000`，并通过 CPU 单步接口做有限步执行。
3. 未来 ISA 契约层：用 ignored 测试描述 xv6 所需的基础 RV64 行为，例如 `x0` 恒为 0、立即数解码、load/store、branch、jump 等。随着模拟器实现推进，可以逐步打开这些测试。
4. xv6 fixture 层：`scripts/build_xv6_fixture.sh` 会获取官方 `mit-pdos/xv6-riscv`，构建 `kernel/kernel` 和 `fs.img`，并额外生成当前模拟器 flat-binary loader 可用的 `kernel/kernel.bin`。
5. xv6 启动契约层：ignored 测试加载 xv6 kernel binary，并按阶段检查从 kernel banner、`init`、shell prompt、基础用户程序，到 xv6 自带 `usertests` 的完整运行路径。

## xv6 机器契约

当前 xv6-riscv 以 `rv64gc` 构建，入口物理地址是 `0x80000000`。它假设 QEMU `virt` 风格的内存和外设布局：

- DRAM：`0x80000000..0x88000000`
- UART 16550a：`0x10000000`
- Virtio block device：`0x10001000`
- PLIC：`0x0c000000`
- CLINT/timer 相关平台行为，以及 `time`、`stimecmp` 等 CSR

xv6 的最初几条指令已经不仅需要 RV64I，还需要 compressed 指令、`mul`、CSR 读写、`mret`、从 machine mode 切到 supervisor mode、timer CSR 和 trap delegation。继续启动还会依赖页表、原子指令、PLIC、UART 中断和 virtio 磁盘行为。

## 运行命令

运行稳定 testbench：

```sh
scripts/run_testbench.sh
```

获取或刷新 xv6 fixture。第三方 xv6 源码会放在 `target/testbench/` 下，不进入仓库提交：

```sh
scripts/build_xv6_fixture.sh
```

构建 xv6 fixture 后运行稳定测试：

```sh
scripts/run_testbench.sh --with-xv6-fixture
```

主动检查未来契约测试：

```sh
scripts/run_testbench.sh --future-contracts
```

`--future-contracts` 中的 ignored 测试在当前模拟器上预期会失败。它们不是当前实现的回归测试，而是用于标记“距离 xv6 启动还缺什么”。

只检查 xv6 完整运行契约：

```sh
scripts/run_testbench.sh --xv6-contracts
```

## xv6 完整运行覆盖范围

xv6 契约测试位于 `tests/xv6_fixture.rs`，默认 ignored。它们覆盖以下阶段：

- `xv6_kernel_reaches_first_shell`：从 `0x80000000` 入口启动，看到 `xv6 kernel is booting`、`init: starting sh` 和 shell prompt `$ `。
- `xv6_shell_runs_basic_user_programs`：通过 UART 输入执行 `echo`、`ls`、`cat README`，验证控制台输入、shell、用户程序、文件系统和 virtio 磁盘读路径。
- `xv6_runs_quick_usertests`：执行 `usertests -q`，要求输出 `ALL TESTS PASSED`。
- `xv6_runs_full_usertests_suite`：执行完整 `usertests`，要求进入 slow tests 阶段并最终输出 `ALL TESTS PASSED`。这是“完整运行 xv6”的最终测试。

这些测试还会检查 UART 输出中不能出现 `panic:`、`FAILED`、`SOME TESTS FAILED`、`init: exec sh failed` 等失败标记。

步数预算可以通过环境变量调整，例如：

```sh
ARVSIM_XV6_FULL_USERTESTS_STEPS=4000000000 scripts/run_testbench.sh --xv6-contracts
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

脚本不会自动执行需要 root 权限的安装命令。如果缺工具，它只会打印需要你手动执行的 Arch Linux 安装命令。

## 当前状态

稳定测试通过；xv6 fixture 可以构建出：

- `target/testbench/xv6-riscv/kernel/kernel`
- `target/testbench/xv6-riscv/kernel/kernel.bin`
- `target/testbench/xv6-riscv/fs.img`

当前默认不运行 xv6 真启动测试，因为模拟器还没有补齐 xv6 必需的 RV64GC、CSR、特权态、trap/timer、PLIC 和 virtio 行为。
