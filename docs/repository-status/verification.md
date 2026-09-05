# 验证记录

执行日期为 2026-09-05，基线提交为 `849ace2`，包含本轮修复和测试精简。结果只说明所列路径得到验证，不表示完整 RISC-V 或设备符合性已经完成。

## 默认测试与静态检查

`cargo test --all-targets --quiet` 通过，合计 **93 项通过、2 项可选 xv6 测试未执行**：

| 范围 | 通过 | 未执行 |
| --- | ---: | ---: |
| 正式库 | 82 | 0 |
| 主命令行单元测试 | 4 | 0 |
| 命令行进程测试 | 3 | 0 |
| RV64 与平台冒烟测试 | 2 | 0 |
| xv6 镜像构建机器 | 1 | 2 |
| xv6 示例参数 | 1 | 0 |

相比精简前减少 12 项默认测试、2 项可选测试。删除重复设备检查、独立启动和快速 usertests 验收；指令、加速器和 Virtio 测试改为检查实际执行结果及外部约定，不依赖私有辅助函数、固定优化阈值或后端调用次数。

PLIC、DMA 与 LR/SC、CSR 无效地址、ELF 入口转换与失败原子性、Virtio 描述符和完成信息、特权级、PMP、Sv39、Sstc、访存对齐与复位等回归仍通过。其他检查均通过：

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --doc
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps
bash -n scripts/build_xv6_fixture.sh scripts/run_testbench.sh scripts/run_xv6_cli.sh
git diff --check
```

当前没有文档测试用例。

## xv6 运行所需文件

xv6 固定提交为 `1982fd12595f52a0e5ef8db466257a01fb1fbfef`，由 `fixtures/xv6-revision` 统一记录。构建仍记录产物校验值；运行只使用内核 ELF 和磁盘镜像。加速模式另核验版本、源码状态、所需内核符号和实际使用镜像的校验值。

在独立目录验证了以下行为，修改后的输入均已恢复：

- 删除主机上的内核裸二进制与 usertests ELF、仅保留构建记录中的提交字段，并调整校验清单顺序后，加速模式仍能启动到 shell。
- 修改磁盘镜像而未更新校验值时，加速模式拒绝启动。
- 关闭加速后，仅凭 `kernel/kernel` 和 `fs.img` 即可启动到 shell，无需 Git 仓库、构建记录或符号工具。

前两项结果保存在 `target/review/simplified-runtime-checks.json`。仅使用两个镜像的启动命令为：

```sh
XV6_DIR="$PWD/target/review/runtime-images-only" \
ARVSIM_XV6_CLI_BOOT_STEPS=1000000000 \
target/review/release/examples/xv6 --boot-only --no-acceleration
```

该命令观察到 shell 提示符，共执行 **436,110,000 步**；日志为 `target/review/xv6-minimal-runtime.log`。关闭加速的完整 usertests 尚未执行。

## xv6 执行验收

两个行为测试默认保持 `ignored`，使用正式 UART、PLIC、Virtio 和共享 DRAM；用户态始终逐条执行。启动由两项验收共同覆盖，完整 usertests 包含快速阶段。

```sh
cargo test --release --target-dir target/review --test xv6_fixture -- \
  --ignored --nocapture --test-threads=1
```

两项均在默认步数预算内通过，总耗时 **470.82 秒**：

| 验收 | 结果 |
| --- | --- |
| shell 执行 `echo`、`ls`、`cat README` | 通过 |
| 完整 usertests | `ALL TESTS PASSED` |

完整 usertests 的快速阶段预算为 6 亿步，慢速阶段为 20 亿步。步数只用于超时，不断言精确周期或优化次数。稀疏内存分配、复制和释放通过真实 xv6 程序验收。

日志为 `target/review/xv6-simplified.log`。本地构建产物和诊断日志不纳入版本控制。

## 覆盖限制

尚未运行独立 RISC-V 架构符合性套件；完整 RVC、CSR 组合、多硬件线程、异步访存和部分设备能力仍未覆盖。xv6 内核加速合并多条指令，不能验证精确指令时序或中断发生位置。UART 文本验收依赖固定版本的输出格式，不能替代逐指令差分测试。
