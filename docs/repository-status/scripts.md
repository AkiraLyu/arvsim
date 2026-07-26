# `scripts/`：构建、测试与交互运行

## `build_xv6_fixture.sh`

检查宿主工具和 RISC-V 工具链（包括 `nm`），克隆或更新 `mit-pdos/xv6-riscv` 的 `riscv` 分支，构建内核和 `fs.img`，将内核 ELF 转成裸二进制镜像，并生成 `fixture.env`。可通过 `XV6_DIR`、`XV6_REPO`、`XV6_REF` 和 `TOOLPREFIX` 覆盖默认值。

状态：可用。CPU 会从生成的内核 ELF 中读取加速地址，链接地址变化不再导致误触发。但脚本默认跟随可变分支，没有固定 xv6 提交版本；如果结构布局变化，仍可能不兼容。已有源码目录更新失败时，脚本只打印警告并继续使用旧版本。

## `run_testbench.sh`

支持四种模式：运行默认测试；先生成 xv6 测试文件再运行默认测试；运行可选的 xv6 验收测试；先生成测试文件再只运行 xv6 验收测试。`--future-contracts` 是历史参数名，它会先运行默认测试，再运行标有 `#[ignore]` 的 xv6 测试，但不会生成测试文件，因此缺少文件时会失败。

## `run_xv6_cli.sh`

确保 xv6 测试文件存在，并以 `release` 模式构建库，然后在 `target/testbench/generated` 生成临时 Rust 程序。该程序直接包含 `tests/support/mod.rs`，再由 `rustc` 链接编译后的 `arvsim` 库。交互模式把标准输入发送到测试 UART，并显示 xv6 的 UART 输出；按 `Ctrl-]` 退出，使用 `--boot-only` 时在出现 shell 提示符后退出。

状态：可以快速启动交互式 xv6，但不是正式命令行功能。脚本通过内嵌文本生成 Rust 源码，从 `deps` 目录取第一个匹配的编译库，并直接依赖测试模块的内部接口。生成程序的执行循环调用 `Machine::step()`，因此会经过正式的设备与 CPU 推进顺序。

## 命令与依赖

- `scripts/run_testbench.sh [--with-xv6-fixture|--future-contracts|--xv6-contracts]`
- `scripts/run_xv6_cli.sh [--build-fixture] [--boot-only]`
- 依赖 Bash、Git、Make、GCC、Perl、RISC-V binutils/GCC、网络和上游 xv6。
- 交互脚本还依赖 `cargo/rustc/find/stty`，并依赖测试辅助模块当前的源码布局。

## 改进建议

固定并校验 xv6 提交版本与测试文件哈希；把临时运行程序改成 Cargo 可执行目标或示例程序；让命令行程序和测试共用正式平台构建器；自动查找工具链前缀；重命名 `--future-contracts`，明确区分测试跳过和失败；在持续集成中缓存测试文件，并分开运行快速测试和定时长测。
