# arvsim 代码审查报告

本报告基于 2026-08-12 的当前工作区代码（`main` 分支基线提交 `17ad107`，包含尚未提交的实现与文档变更），审查范围为 `src/`、`tests/` 和 `scripts/`；`target/` 只作为测试输入和构建产物，不计入源码审查。审查目标包括功能缺陷、RISC-V/PLIC/Virtio/ELF 规范偏差、畸形输入与资源上限、测试可信度、脚本可复现性，以及文档与实现是否一致。

本轮只更新审查报告和状态文档，未修改实现代码。条目中的位置均指向当前工作区行号；修复后应重新定位并补充回归测试。

## 与上一轮报告的关系

2026-07-26 报告记录的 2 个高危、13 个中危和 50 个低危问题已在当前工作区完成整改。本文以当前实现重新审查，已解决条目不再作为活动问题重复保留；本轮新确认 11 条待处理问题。

## 审查方法

- 对照当前工作区逐模块复核公开接口、错误路径、算术与内存边界、设备状态机、测试辅助代码和脚本环境变量。
- 对 PLIC claim/complete、ELF 入口和 Virtio used length 分别对照官方规范，并用具体状态转换或输入布局验算触发路径。
- 交叉检查 `docs/` 的模块状态、已知限制、验证快照和路线图，修正把“上一轮全部整改”误写成“当前没有活动问题”的表述。
- 执行 `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --all-targets`、`cargo test --doc`、`RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`、脚本语法检查和强制 fixture 完整性测试。默认测试共 85 个通过，4 个 xv6 长测试按设计未执行；本次复审没有重跑耗时的 xv6 行为合同。

## 结果总览

| 严重度 | 数量 | 说明 |
| --- | --- | --- |
| 高危 | 0 | 未发现会在默认配置下直接造成宿主失陷或静默破坏的高危问题 |
| 中危 | 6 | PLIC 规范语义、ELF 入口转换、xv6 快速路径资源上限及脚本可复现性/停滞 |
| 低危 | 5 | 受检公共接口、Virtio 异常 DMA、交互缓冲、工具探测和 fixture 验证 |

| 类别 | 数量 |
| --- | --- |
| 健壮性 | 5 |
| 缺陷或规范偏差 | 4 |
| 可复现性 | 1 |
| 测试可信度 | 1 |

## 必须优先处理的问题

1. **PLIC claim 错误地受 threshold 影响**（中危，[plic.md](./plic.md) #1）：规范允许在通知被阈值屏蔽时轮询 claim；当前实现会返回 0。
2. **PLIC completion 按“最后 claim 的上下文”校验**（中危，[plic.md](./plic.md) #2）：规范要求按目标当前 enable 位决定是否接受 completion，当前实现会同时误拒绝和误接受。
3. **xv6 快速路径可在单个 `Machine::step` 内执行无上限工作并增长宿主内存**（中危，[cpu.md](./cpu.md) #1）：步数上限无法约束这类循环，畸形页表还可让 4 GiB `memmove` 长时间保持可读。
4. **ELF 使用 `p_paddr` 装载时仍直接把虚拟 `e_entry` 当复位物理地址**（中危，[loader.md](./loader.md) #1）：合法的非恒等装载布局会被拒绝，或从未装载的位置启动。
5. **fixture 更新失败后静默复用旧 checkout**（中危，[scripts.md](./scripts.md) #1）：显式 `XV6_REPO/XV6_REF` 可能没有生效，`fixture.env` 却记录请求值。
6. **`ARVSIM_XV6_CLI_STEP_CHUNK=0` 使交互 runner 永久不推进且不会超时**（中危，[scripts.md](./scripts.md) #2）。

## 模块索引

| 审查文档 | 审查对象 | 发现数（高/中/低） | 当前结论 |
| --- | --- | --- | --- |
| [plic.md](./plic.md) | `src/plic.rs` | 2（0/2/0） | claim 与 completion 各有一处规范偏差 |
| [cpu.md](./cpu.md) | `src/cpu.rs` | 1（0/1/0） | 可选 xv6 快速路径缺少单步资源预算 |
| [loader.md](./loader.md) | `src/loader.rs` | 1（0/1/0） | `e_entry` 未随虚拟/物理段布局转换 |
| [scripts.md](./scripts.md) | `scripts/` | 3（0/2/1） | checkout 复用、零步长停滞和无界输出历史 |
| [csr.md](./csr.md) | `src/csr.rs` | 1（0/0/1） | 安全公共接口可被越界地址触发 panic |
| [virtio.md](./virtio.md) | `src/virtio.rs` | 1（0/0/1） | 部分 IN DMA 失败时 used length 失真 |
| [test-support.md](./test-support.md) | `tests/support/mod.rs` | 1（0/0/1） | 工具名未经引用插入 shell 命令 |
| [xv6-fixture.md](./xv6-fixture.md) | `tests/xv6_fixture.rs` | 1（0/0/1） | “well formed” 未验证 usertests ELF |
| [bus.md](./bus.md) | `src/bus.rs` | 0（0/0/0） | 本轮未发现新增问题 |
| [instruction.md](./instruction.md) | `src/instruction.rs` | 0（0/0/0） | 本轮未发现新增问题；ISA 缺口已在状态文档列明 |
| [machine.md](./machine.md) | `src/machine.rs` | 0（0/0/0） | 本轮未发现新增问题 |
| [main.md](./main.md) | `src/main.rs` | 0（0/0/0） | 本轮未发现新增问题 |
| [uart.md](./uart.md) | `src/uart.rs` | 0（0/0/0） | 本轮未发现独立实现缺陷；缓冲后端的使用问题记入脚本报告 |
| [trap.md](./trap.md) | `src/trap.rs` | 0（0/0/0） | 本轮未发现新增问题 |
| [cli-tests.md](./cli-tests.md) | `tests/cli.rs` | 0（0/0/0） | 本轮未发现新增问题 |
| [rv64i-smoke.md](./rv64i-smoke.md) | `tests/rv64i_smoke.rs` | 0（0/0/0） | 本轮未发现新增问题 |

本轮未发现需要单独成文的问题：`src/cfg.rs`、`src/clint.rs`、`src/dram.rs`、`src/interrupt.rs`、`src/lib.rs` 和 `src/virt_platform.rs`。其中 CLINT 仍为空占位，DRAM 的连续分配、完整 ISA/设备支持等明确限制继续记录在 [`docs/repository-status/`](../docs/repository-status/)，不因“未发现新增缺陷”而视为已经完成。

## 阅读建议

优先处理 [plic.md](./plic.md)、[cpu.md](./cpu.md)、[loader.md](./loader.md) 和 [scripts.md](./scripts.md) 的 6 条中危问题。低危问题多数只在畸形输入、长期交互或自定义库调用下触发，但修复成本较低，适合与对应模块测试一起完成。
