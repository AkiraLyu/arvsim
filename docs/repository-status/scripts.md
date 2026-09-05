# 构建、测试与交互运行

## 构建 xv6 镜像

`scripts/build_xv6_fixture.sh` 从 `fixtures/xv6-revision` 读取默认提交，完整重建内核、裸二进制、文件系统和 usertests，并生成 `fixture.env` 与 `fixture.sha256`。元数据记录实际提交，镜像路径使用相对路径，目录迁移后仍可验证。

可用 `XV6_DIR`、`XV6_REPO`、`XV6_REF`、`TOOLPREFIX` 覆盖配置。获取失败立即退出；`XV6_OFFLINE=1` 只允许复用仓库地址、请求提交与当前 HEAD 均匹配的本地源码。脚本拒绝覆盖已跟踪文件的修改。

依赖 Bash、Git、Make、宿主 GCC、Perl、SHA-256 工具和 RISC-V GCC/binutils。在线构建另需网络。

## 运行测试

`scripts/run_testbench.sh` 支持默认测试、`--with-xv6-fixture`、`--future-contracts`、`--xv6-contracts`。默认检查使用 `cargo test --all-targets`，包括示例参数测试；耗时 xv6 验收使用 release 构建，逐项执行基础程序和完整 usertests。生成镜像的模式会设置 `ARVSIM_REQUIRE_XV6_FIXTURE=1`。历史参数 `--future-contracts` 表示执行可选 xv6 验收测试，不会生成镜像。

## 交互运行

`scripts/run_xv6_cli.sh [--build-fixture] [--boot-only]` 调用正式 Cargo 示例 `examples/xv6.rs`，不再生成 Rust 程序或包含测试源码。交互模式转发原始 UART 字节，按 `Ctrl-]` 退出，并在退出时恢复终端设置。

也可以直接运行：

```sh
cargo run --release --example xv6 -- --boot-only
```

`ARVSIM_XV6_CLI_STEP_CHUNK` 和 `ARVSIM_XV6_CLI_BOOT_STEPS` 必须是正整数，零值、负数、非法文本和溢出均报错。启动预算不会因大步长而超出；UART 至少每一万步排出一次，已打印历史会清除，提示符可跨输出批次匹配。

默认启用固定 xv6 版本的内核加速。Cargo 示例接受 `--no-acceleration`；通过脚本运行时可设置 `ARVSIM_XV6_NO_ACCELERATION=1`。关闭加速后只需内核 ELF 与磁盘镜像，通常需要提高启动和测试预算。当前主命令行 `arvsim` 仍只提供 DRAM/UART 配置；完整平台通过库或此示例运行。
