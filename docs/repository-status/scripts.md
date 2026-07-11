# `scripts/`：构建、测试与交互运行

## `build_xv6_fixture.sh`

检查宿主和 RISC-V 工具（包括 `nm`），克隆或更新 `mit-pdos/xv6-riscv` 的默认 `riscv` 分支，构建 kernel/fs.img，将 ELF 转为 flat binary，并生成 `fixture.env`。支持 `XV6_DIR`、`XV6_REPO`、`XV6_REF`、`TOOLPREFIX`。

状态：可用。CPU 加速地址会从构建出的 kernel ELF 动态解析，因此链接地址变化不会再误触发旧快速路径；但默认仍跟踪可变分支而非固定 commit，结构布局变化仍可能不兼容。已有 checkout 刷新失败时会静默复用旧版本。

## `run_testbench.sh`

提供默认测试、先构建 fixture 后运行默认测试、运行 opt-in xv6 合同，以及“先构建再只运行 xv6 合同”四种模式。`--future-contracts` 是保留的历史参数名，当前先跑默认测试再运行 xv6 ignored 合同，但本身不构建 fixture；缺少构件会导致该模式失败。帮助文字把失败定义为回归或未满足合同。

## `run_xv6_cli.sh`

确保 fixture 存在、构建 release 库，在 `target/testbench/generated` 写入临时 Rust runner，源码包含 `tests/support/mod.rs`，再用 `rustc` 链接 rlib。交互模式将 stdin 字节送入测试 UART，输出 guest UART；`Ctrl-]` 退出，`--boot-only` 到 shell prompt 后退出。

状态：能复用测试平台快速形成交互入口，但不是正式 CLI。它通过 shell here-doc 生成 Rust 源码、从 deps 中取第一个匹配 rlib，并依赖测试模块的非稳定接口；执行循环直接调用 `machine.cpu.step()`，未来 `Machine` 增加平台 tick 时会被绕过。

## 对外接口与耦合

- `scripts/run_testbench.sh [--with-xv6-fixture|--future-contracts|--xv6-contracts]`
- `scripts/run_xv6_cli.sh [--build-fixture] [--boot-only]`
- 强依赖 Bash、Git、Make、GCC、Perl、RISC-V binutils/GCC、网络和上游 xv6。
- 交互脚本还依赖 `cargo/rustc/find/stty` 和测试支撑源码布局。

## 优化方向

固定并校验 xv6 commit/构件哈希；将 runner 做成 Cargo binary/example 并只调用 `Machine` 运行接口；由正式平台构建器供 CLI 与测试共用；自动发现 tool prefix；重命名历史 `--future-contracts` 并明确 skip/fail 语义；在 CI 中缓存 fixture 并分离 smoke/nightly 测试档位。
