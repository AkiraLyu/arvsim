# `tests/xv6_fixture.rs`：xv6 验收测试

## 功能与实现思路

以 UART 文本为黑盒观察点，分层验证 fixture、内核启动、shell、基础用户程序、快速 usertests 和完整 usertests。步数预算可由 `ARVSIM_XV6_*_STEPS` 环境变量覆盖，运行过程中检查 panic/FAILED 等失败标记。

## 当前状态

- fixture 完整性检查默认执行；如果构件不存在会直接打印提示并返回成功，因此默认通过不证明 fixture 可构建或 xv6 可运行。
- 启动、基础命令、quick usertests、full usertests 共 4 个合同全部 `#[ignore]`。
- ignore 原因明确标注它们是依赖外部 fixture 和长预算的 opt-in 合同。本轮没有重新执行长测试，因此当前结论是“存在合同，未由本轮复核行为结果”。

## 对外接口

测试本身无产品接口。相关环境变量控制 banner、init、shell、命令和 usertests 各阶段最大步数；执行入口见 [`scripts.md`](./scripts.md)。

## 耦合方式

强依赖 `tests/support` 的平台模型、外部 xv6 kernel/fs fixture、特定启动输出和用户程序文本。完整测试最大预算达 20 亿步，性能依赖 CPU 内的 xv6 快速路径。

## 不完善之处和优化方向

- fixture 缺失时测试“绿色跳过”而不是 Cargo ignored/明确 skip，可能造成错误安全感。
- 默认 CI 不执行任何 xv6 行为合同，快速路径或设备回归可能长期未发现。
- 构建脚本会记录实际 commit，但测试不读取 `fixture.env`、不校验 commit/构件哈希，也没有把固定结构偏移与 fixture 版本绑定。
- 以自由文本匹配判断状态易受上游输出变化影响。
- 建议增加短时 boot smoke 到定期 CI，长测放 nightly；固定 xv6 commit 并记录 fixture 哈希；区分 skip/pass；为关键阶段提供结构化退出/signature。
