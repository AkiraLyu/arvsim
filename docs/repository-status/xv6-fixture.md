# `tests/xv6_fixture.rs`：xv6 验收测试

## 功能与实现

测试根据 UART 输出判断 xv6 运行进度，依次检查测试文件、内核启动、shell、基础用户程序、快速 usertests 和完整 usertests。可用 `ARVSIM_XV6_*_STEPS` 环境变量调整各阶段步数上限，并检查输出中是否出现 `panic`、`FAILED` 等失败标记。

## 实现状态

- 测试文件检查默认执行；如果文件不存在，会直接向终端输出 `SKIPPED`。设置 `ARVSIM_REQUIRE_XV6_FIXTURE=1` 后缺失文件会使测试失败，构建测试文件的脚本模式会自动启用该要求。当前完整性测试会验证 kernel ELF 和两个主要镜像非空，但对 `_usertests` 只检查路径存在，尚未验证其 ELF 头或必需符号。
- 启动、基础命令、快速 usertests 和完整 usertests 共 4 个验收测试，都标有 `#[ignore]`，需要手动运行。
- 显式设置的步数预算必须是合法 `usize` 十进制数；解析失败会立即指出变量名和值，不再静默使用默认预算。
- 当前工作区已有正式设备路径启动到 shell、基础用户程序、快速 usertests 和完整 usertests 的通过记录；本次文档复审未重跑这些耗时合同。UART 发送完成与中断路径不再使用 `tx_busy` 测试旁路，virtio 也不修改 guest 驱动私有状态。

## 配置方式

本文件不提供库接口。`ARVSIM_REQUIRE_XV6_FIXTURE` 控制测试文件是否必须存在；其他环境变量可分别控制启动信息、init、shell、命令和 usertests 的最大步数，非法值会使对应测试失败。运行方法见 [`scripts.md`](./scripts.md)。

## 依赖关系

测试依赖正式 `VirtPlatform`、`tests/support` 中的 fixture 包装、外部 xv6 内核和文件系统镜像，以及固定的启动和用户程序输出。完整测试默认最多允许执行 20 亿步，并依赖 CPU 中的 xv6 加速。当前验证结果见 [`verification.md`](./verification.md)。

## 已知问题与改进建议

- 默认的持续集成流程不运行 xv6 行为测试，CPU 加速或设备回归可能长期无法发现。
- 构建脚本会记录实际 xv6 提交版本，但测试不读取 `fixture.env`，也不校验提交版本和测试文件哈希；固定结构偏移没有与 xv6 版本绑定。
- “well formed” 默认测试没有读取 `_usertests` 内容；空文件、错误架构或缺少 `exec` 符号会到 ignored 行为测试构造机器时才失败。
- 以自由文本匹配判断状态易受上游输出变化影响。
- 建议在日常持续集成中加入短启动测试，把长测试放到定时任务；固定 xv6 提交版本并记录测试文件哈希；明确区分跳过和通过；为关键阶段增加便于程序识别的结束标记。
