# 当前仓库实现说明

本文档记录 arvsim 当前模块职责、xv6 支持状态、验证结果和剩余限制。

## 总体状态

当前仓库已经从最小 RV64I smoke 模拟器推进到可以运行 MIT xv6-riscv 的状态：

- 稳定 testbench `scripts/run_testbench.sh` 通过。
- xv6 quick usertests 在 debug test harness 下通过。
- xv6 full usertests 在 release test harness 下通过，并输出 `ALL TESTS PASSED`。
- xv6 fixture 仍然默认放在 `target/testbench/`，不进入仓库提交。

需要注意：xv6 契约测试仍是 `#[ignore]`，因为它们耗时很长，不适合作为默认 `cargo test` 的一部分。debug full usertests 预计需要数小时；日常验证建议优先跑 quick 或 release full。

## 目录结构

```text
src/      模拟器主体代码
tests/    集成测试、测试总线和 xv6 运行契约测试
scripts/  xv6 fixture 构建和 testbench 入口脚本
docs/     项目文档
```

## 核心模块状态

### `src/instruction.rs`

职责：

- 解码并执行当前模拟器支持的 RISC-V 指令。
- 维护寄存器写回、立即数展开、压缩指令 PC 推进和内存访问语义。

当前实现：

- RV64I 常用整数、分支、跳转、load/store、`LUI/AUIPC`、word 运算。
- RV64M 乘除余和 word 变体。
- RV64A LR/SC 与 AMO 常用操作的单 hart 简化语义。
- Zicsr CSR 读改写指令。
- `ecall`、`sret`、`mret`、`wfi`、`sfence.vma` 等系统指令。
- RVC 压缩指令子集，覆盖 xv6 kernel/user 程序实际使用路径。
- 立即数布局和常见压缩立即数单元测试。

目的：

- xv6 使用 `rv64gc` 构建，启动第一阶段就依赖 compressed、CSR、M/A 扩展和正确的 RV64I 立即数语义。
- 这些补齐让 kernel、user programs 和 usertests 能够通过同一条 CPU 单步路径执行。

已知限制：

- 还没有独立接入官方 riscv-tests。
- 部分原子和特权指令按单 hart xv6 需求实现，尚非完整多 hart/完整规范模型。
- decoder 仍集中在一个文件中，后续可拆分为更清晰的 ISA 子模块。

### `src/cpu.rs`

职责：

- CPU 取指、执行、PC 更新、异常/中断入口、地址翻译和 xv6 运行时支撑。

当前实现：

- `step()` 每步推进简化 timer，并在取指前检查 timer/external interrupt。
- Sv39 地址翻译，支持 fetch/load/store 权限检查和用户态 `PTE_U` 检查。
- 同步异常进入 supervisor trap，设置 `SEPC/SCAUSE/STVAL/SSTATUS`。
- `sret` 恢复 supervisor interrupt 状态并返回 `SEPC`。
- `time/stimecmp` 驱动 supervisor timer interrupt，支撑 xv6 `pause()`、调度和 preemption 测试。
- 针对 xv6 热点函数提供 fast path，例如锁、字符串/内存函数、`freewalk`、`uvmunmap`、`uvmcopy`、`myproc`、`wakeup` 和 invalid-argv `exec`。

目的：

- Sv39、trap、timer 是 xv6 从 kernel boot 到 user mode 的基础。
- fast path 不是改变 xv6 语义，而是把解释器中极热、纯循环或单 hart 等价的路径折叠成 Rust 侧操作，使 quick/full usertests 在测试步数预算内完成。

已知限制：

- privilege mode 目前主要由 PC 范围推断，尚未建模完整 M/S/U mode 状态机。
- fast path 绑定当前 xv6 fixture 的符号地址，适用于测试契约，不等同于通用 RISC-V 加速层。
- `DEBUG` 仍是常量，交互式运行时应继续工程化为可配置项。

### `src/csr.rs`

职责：

- 保存 CSR 状态并提供 supervisor alias 行为。

当前实现：

- 增加 xv6 需要的 `MENVCFG`、`STIMECMP`、`TIME` 等常量。
- `SSTATUS/SIE/SIP` 对应 machine CSR 的简化映射仍保留。
- 提供 `load()`、`store()` 和 debug dump。

已知限制：

- 尚未完整建模 WARL/WPRI、只读位、CSR 权限等级和 delegation 位过滤。
- 当前行为以 xv6 单 hart 路径为优先目标。

### `src/bus.rs`

职责：

- 抽象 CPU 与内存/设备之间的读写。

当前实现：

- `MemDevice` 增加 `pending_interrupt()` 默认方法，让 CPU 可从设备侧查询待处理中断。

目的：

- xv6 UART 输入依赖 PLIC 外部中断，CPU 需要一个不破坏现有设备接口的 pending 查询点。

已知限制：

- 正式 `Bus` 仍是简化设备分发；完整 PLIC/virtio 目前主要在测试支撑层实现。

### `tests/support/mod.rs`

职责：

- 提供集成测试用机器、测试总线、UART 捕获、xv6 fixture 加载和设备模型。

当前实现：

- sparse PLIC：覆盖 UART IRQ 10 的 priority、enable、threshold、claim/complete 路径。
- virtio-mmio block：加载 `fs.img`，解析 virtqueue descriptor，处理 block read/write 请求。
- UART 输入队列和输入中断注入。
- `tx_busy` 符号覆盖，避免 xv6 UART 发送路径因测试模型缺少真实 TX 中断而卡住。
- `xv6_machine()` 同时加载 kernel flat binary 和 `fs.img`。

目的：

- 让 xv6 的 shell、文件系统、`usertests` 能通过真实 UART 输入和 virtio 磁盘路径运行。

已知限制：

- 设备模型是 test fixture 级别，覆盖 xv6 contract 所需寄存器和行为，不是完整 QEMU virt 设备模拟。
- virtio queue size 固定为测试足够的大小。

## 测试状态

当前已验证：

```sh
scripts/run_testbench.sh
cargo test --test xv6_fixture xv6_runs_quick_usertests -- --ignored --nocapture
cargo test --release --test xv6_fixture xv6_runs_full_usertests_suite -- --ignored --nocapture
cargo test --release --test xv6_fixture xv6_shell_runs_basic_user_programs -- --ignored --nocapture
git diff --check
```

验证结果：

- 稳定 testbench：通过。
- xv6 quick usertests：debug harness 通过。
- xv6 full usertests：release harness 通过，耗时较长。
- shell 基础用户程序：release harness 通过。
- diff whitespace 检查：通过。

## 后续工作

- 将 xv6 fast path 从硬编码地址改为符号驱动或可配置策略。
- 将测试 PLIC/virtio 设备模型迁移或抽象到正式机器模型。
- 引入官方 riscv-tests 或更系统的 ISA conformance 测试。
- 建模完整 privilege mode、CSR 权限和多 hart 原子语义。
- 为 xv6 长测试增加日志文件、进度输出和可中断运行配置。
