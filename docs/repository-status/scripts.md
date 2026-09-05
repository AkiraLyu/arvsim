# 构建、测试与交互运行

## 构建 xv6 镜像

`scripts/build_xv6_fixture.sh` 从 `fixtures/xv6-revision` 读取默认提交号，重新构建内核 ELF、原始二进制、文件系统镜像和 usertests，并生成 `fixture.env` 与 `fixture.sha256`。构建记录保存实际提交号，镜像使用相对路径，移动整个目录后仍可校验。

可通过 `XV6_DIR`、`XV6_REPO`、`XV6_REF`、`TOOLPREFIX` 修改目录、源码仓库、版本和工具链前缀。获取源码失败时，脚本立即退出。设置 `XV6_OFFLINE=1` 后，只能使用仓库地址、指定版本和当前 HEAD 均符合要求的本地源码。脚本不会覆盖 Git 已跟踪文件的未提交修改。

构建需要 Bash、Git、Make、用于编译主机工具的 GCC、Perl、SHA-256 工具和 RISC-V GCC/binutils。在线获取源码还需要网络。

## 运行测试

`scripts/run_testbench.sh` 提供以下运行方式：

| 参数 | 执行内容 |
| --- | --- |
| 不加参数 | 运行 `cargo test --all-targets`，包含示例参数测试 |
| `--with-xv6-fixture` | 构建 xv6 镜像，再运行全部默认测试 |
| `--future-contracts` | 运行默认测试和两个 xv6 耗时测试，使用已有镜像 |
| `--xv6-contracts` | 构建镜像，检查镜像能否创建机器，再运行两个 xv6 耗时测试 |

会生成镜像的运行方式设置 `ARVSIM_REQUIRE_XV6_FIXTURE=1`，避免缺少镜像时跳过检查。两个耗时测试使用 release 构建，并依次运行基础用户程序和完整 usertests。`--future-contracts` 是保留的旧参数名，不会构建镜像。

## 交互运行

`scripts/run_xv6_cli.sh [--build-fixture] [--boot-only]` 调用 Cargo 示例 `examples/xv6.rs`。`--build-fixture` 强制重新构建镜像；`--boot-only` 在看到第一个 shell 提示符后退出。不加 `--boot-only` 时进入交互模式，终端输入输出通过 UART 转发，按 `Ctrl-]` 退出，脚本随后恢复终端设置。

也可以直接运行示例：

```sh
cargo run --release --example xv6 -- --boot-only
```

`ARVSIM_XV6_CLI_STEP_CHUNK` 设置每批执行步数，`ARVSIM_XV6_CLI_BOOT_STEPS` 设置等待启动的步数上限。两者都必须是正整数，零、负数、无效文本和超出整数范围的值都会报错。

在 `--boot-only` 模式下，每批步数还会受剩余步数限制，达到上限仍未出现提示符就报错退出。交互模式达到上限时只提示一次，并继续运行。程序最多每一万步刷新一次 UART 输出，并清空已经显示的缓冲；提示符即使分成两批输出，也能被识别。

默认开启固定版本的 xv6 内核加速。直接运行示例时可传入 `--no-acceleration`；通过脚本运行时可设置 `ARVSIM_XV6_NO_ACCELERATION=1`。关闭加速后只需内核 ELF 和磁盘镜像，通常要提高启动和测试的最大步数。

主命令行 `arvsim` 仍只提供 DRAM/UART 配置，包含 PLIC 和块设备的完整平台通过库或 xv6 示例运行。
