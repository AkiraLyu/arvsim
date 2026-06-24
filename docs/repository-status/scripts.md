# 脚本

## 设计

- 脚本把常用测试和 xv6 构件构建流程固定下来，避免手工重复输入长命令。

## 实现和接口

- `scripts/build_xv6_fixture.sh`
  - 检查 `git`、`make`、`gcc`、`perl`、`riscv64-elf-gcc`、`riscv64-elf-objcopy`、`riscv64-elf-readelf`。
  - 默认从 `https://github.com/mit-pdos/xv6-riscv.git` 的 `riscv` 分支构建。
  - 生成 `kernel/kernel`、`kernel/kernel.bin`、`fs.img` 和 `fixture.env`。
  - 可用 `XV6_DIR`、`XV6_REPO`、`XV6_REF`、`TOOLPREFIX` 覆盖默认值。
- `scripts/run_testbench.sh`
  - 无参数运行 `cargo test`。
  - `--with-xv6-fixture` 先构建 xv6 构件，再运行稳定测试。
  - `--future-contracts` 运行默认测试、默认忽略的 RV64 测试和默认忽略的 xv6 测试。
  - `--xv6-contracts` 构建 xv6 构件并运行 xv6 忽略测试。
- `scripts/run_xv6_cli.sh`
  - 自动确保 xv6 构件存在。
  - 构建 release 版库，再生成一个临时 Rust 运行器。
  - 复用 `tests/support/mod.rs` 的 xv6 测试机器。
  - 默认进入交互模式，把主机终端输入转发到 xv6 UART；按 `Ctrl-]` 退出。
  - `--boot-only` 启动到第一个 shell 提示符后退出。
  - `--build-fixture` 强制重建 xv6 构件。

## 限制

- `run_testbench.sh` 的帮助文本仍使用旧的 “future-contract” 英文说明；脚本行为本身可用。
- `run_xv6_cli.sh` 通过生成临时 Rust 文件复用测试支撑代码，是最短可用路径，不是长期 CLI 架构。
