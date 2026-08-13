# `scripts/`：构建、测试与交互运行

## `build_xv6_fixture.sh`

检查宿主工具和 RISC-V 工具链（包括 `nm`），通过 `git init/fetch` 克隆或更新 `mit-pdos/xv6-riscv`，构建内核和 `fs.img`，将内核 ELF 转成裸二进制镜像，并生成 `fixture.env`。`XV6_REF` 可使用分支、标签或可获取的提交哈希；复用目录时会同步 `origin` 到当前 `XV6_REPO`。入口地址在固定英文 locale 下解析，无法取得时脚本直接失败。可通过 `XV6_DIR`、`XV6_REPO`、`XV6_REF` 和 `TOOLPREFIX` 覆盖默认值；测试辅助模块和交互脚本使用相同的 `XV6_DIR`、`TOOLPREFIX` 约定。

状态：可用，但来源可复现性不完整。CPU 会从生成的内核 ELF 中读取加速地址，链接地址变化不再导致误触发。脚本默认跟随可变分支，没有固定 xv6 提交版本；如果结构布局变化，仍可能不兼容。已有源码目录更新失败时，脚本只打印警告并继续使用旧 `HEAD`，即使调用方显式改变了 `XV6_REPO/XV6_REF`；随后 `fixture.env` 仍记录请求的仓库和 ref，而不是说明复用了旧 checkout。

## `run_testbench.sh`

支持四种模式：运行默认测试；先生成 xv6 测试文件再运行默认测试；运行可选的 xv6 验收测试；先生成测试文件再只运行 xv6 验收测试。脚本只接受零或一个模式参数，多余参数会显示用法并返回 2。生成测试文件的模式会设置 `ARVSIM_REQUIRE_XV6_FIXTURE=1` 执行完整性检查，缺失文件不能再被当作普通通过。`--future-contracts` 是历史参数名，它会先运行默认测试，再运行标有 `#[ignore]` 的 xv6 测试，但不会生成测试文件，因此缺少文件时会失败。

## `run_xv6_cli.sh`

按 `XV6_DIR` 确保 xv6 测试文件存在，并以 `release` 模式构建库，然后在 `target/testbench/generated` 生成临时 Rust 程序。该程序直接包含 `tests/support/mod.rs`，再以仓库相同的 Rust 2024 edition 链接 Cargo 生成的确定性顶层 `libarvsim.rlib`。固定路径前缀与带引号的 heredoc 分开生成，正文不会被 shell 展开。交互模式把标准输入发送到正式缓冲 UART 后端，并按原始字节转发输出，非 ASCII 数据不会经过字符串切片；按 `Ctrl-]` 退出，使用 `--boot-only` 时在出现 shell 提示符后退出。显式帮助写入标准输出，参数错误的用法写入标准错误。

状态：可以快速启动交互式 xv6，但不是正式命令行功能。脚本仍通过内嵌文本生成 Rust 源码并直接依赖测试辅助接口；生成程序的执行循环调用 `Machine::step()`，UART 输入输出及 PLIC/virtio 路径均来自正式 `VirtPlatform`。`ARVSIM_XV6_CLI_STEP_CHUNK` 当前未要求大于零：设为 0 时 CPU 不再推进，步数超时也永远不会触发；非法环境值则静默回退默认值。runner 还会保留全部已打印 UART 历史，长期会话的宿主内存随累计输出增长。

## 命令与依赖

- `scripts/run_testbench.sh [--with-xv6-fixture|--future-contracts|--xv6-contracts]`
- `scripts/run_xv6_cli.sh [--build-fixture] [--boot-only]`
- 依赖 Bash、Git、Make、GCC、Perl、RISC-V binutils/GCC、网络和上游 xv6。
- 交互脚本还依赖 `cargo/rustc/stty`，并依赖测试辅助模块当前的源码布局。

## 改进建议

固定并校验 xv6 提交版本与测试文件哈希；获取失败时默认终止，只在显式离线模式下校验并记录实际复用版本；严格解析 runner 的正数步长和预算；让交互输出可增量取走而不是保留完整历史；把临时运行程序改成 Cargo 可执行目标或示例程序；让主命令行暴露已有正式 `VirtPlatform` preset；自动查找工具链前缀；重命名 `--future-contracts`，明确区分测试跳过和失败；在持续集成中缓存测试文件，并分开运行快速测试和定时长测。
