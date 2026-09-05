# 验证记录

以下是 2026-09-05 的运行结果，验证对象为基线提交 `849ace2` 之后完成修复和测试精简的代码。结果只说明所列功能通过了测试，不能证明整个 RISC-V 指令集或所有设备都符合规范。

## 默认测试与代码检查

`cargo test --all-targets --quiet` 运行结果为 **93 项通过，2 项 xv6 耗时测试跳过**。这两个测试已用后文的命令单独运行。

| 测试范围 | 通过 | 该命令跳过 |
| --- | ---: | ---: |
| 库单元测试 | 82 | 0 |
| 主命令行单元测试 | 4 | 0 |
| 命令行进程测试 | 3 | 0 |
| RV64 程序与平台集成测试 | 2 | 0 |
| xv6 镜像创建机器与功能测试 | 1 | 2 |
| xv6 示例参数测试 | 1 | 0 |

精简后减少了 12 项默认测试和 2 项可选测试，主要删除重复的设备检查、独立启动测试和快速 usertests。指令、加速器与 Virtio 测试改为检查实际执行结果和接口约定，不依赖私有辅助函数、固定优化阈值或后端调用次数。

PLIC、DMA 与 LR/SC、无效 CSR 地址、ELF 入口转换与加载失败后内存不变、Virtio 描述符与完成信息、特权级、PMP、Sv39、Sstc、内存对齐和复位等测试均通过。以下检查也通过：

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --doc
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps
bash -n scripts/build_xv6_fixture.sh scripts/run_testbench.sh scripts/run_xv6_cli.sh
git diff --check
```

当前没有可运行的 Rust 文档示例测试，`cargo test --doc` 的用例数为零。

## xv6 镜像要求

固定 xv6 提交号为 `1982fd12595f52a0e5ef8db466257a01fb1fbfef`，由 `fixtures/xv6-revision` 统一记录。构建时记录生成文件的校验值；运行只使用内核 ELF 和磁盘镜像。开启加速时，还要检查版本、源码修改情况、所需内核符号和两个运行文件的校验值。

在独立目录中完成了以下检查，并在检查结束后恢复输入文件：

- 删除主机上的内核原始二进制和 usertests ELF，只保留构建记录中的提交字段，并调整校验清单的行顺序后，加速模式仍能启动到 shell。
- 修改磁盘镜像但不更新校验值时，加速模式拒绝启动。
- 关闭加速后，只保留 `kernel/kernel` 和 `fs.img`，仍能启动到 shell，不需要 Git 仓库、构建记录或符号工具。

前两项结果保存在 `target/review/simplified-runtime-checks.json`。只使用两个镜像的启动命令为：

```sh
XV6_DIR="$PWD/target/review/runtime-images-only" \
ARVSIM_XV6_CLI_BOOT_STEPS=1000000000 \
target/review/release/examples/xv6 --boot-only --no-acceleration
```

该命令在 **436,110,000 步**后显示 shell 提示符，日志保存在 `target/review/xv6-minimal-runtime.log`。尚未在关闭加速的情况下运行完整 usertests。

## xv6 功能测试

两个耗时测试使用库中的 UART、PLIC、Virtio 和共享 DRAM，用户程序始终逐条执行。两项都包含内核启动过程，完整 usertests 还包含快速阶段。运行命令为：

```sh
cargo test --release --target-dir target/review --test xv6_fixture -- \
  --ignored --nocapture --test-threads=1
```

两项都在默认最大步数内通过，总耗时 **470.82 秒**：

| 测试内容 | 结果 |
| --- | --- |
| shell 执行 `echo`、`ls`、`cat README` | 通过 |
| 完整 usertests | `ALL TESTS PASSED` |

完整 usertests 的快速阶段最多运行 6 亿步，慢速阶段为 20 亿步。这个上限只用于判断超时，不要求固定的周期数或优化次数。实际 xv6 程序也验证了按需分配内存后的复制和释放。

日志保存在 `target/review/xv6-simplified.log`。本地构建文件和诊断日志不加入 Git。

## 尚未验证的范围

尚未运行独立的 RISC-V 架构测试套件。完整 RVC、各种 CSR 组合、多硬件线程、异步内存访问和部分设备功能仍缺少测试。

xv6 内核加速会合并多条指令，因此不能验证精确的指令时序或中断发生位置。UART 输出检查依赖固定版本的文本格式，也不能代替将每条指令的执行结果与参考实现比较。
