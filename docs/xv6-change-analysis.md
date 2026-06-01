# xv6 支持变更分析

本文重点记录本轮为通过 xv6 testbench 所做的代码更改、每组更改的目的，以及对应验证证据。

## 目标

本轮目标不是单纯增加指令数量，而是让当前模拟器能够实际运行 xv6-riscv：

1. 能启动 kernel，进入 `init` 和 shell。
2. 能通过 UART 输入运行 shell 命令。
3. 能通过 virtio block 访问 `fs.img`。
4. 能执行 `usertests -q` 和完整 `usertests`。
5. 尽量保持改动集中，不改 xv6，不改 test contract。

## 指令执行层

主要文件：[src/instruction.rs](/home/akira/codespace/arvsim/src/instruction.rs)

本轮将原来只支持少量 `ADD/SUB/ADDI/load/store` 的执行器扩展为 xv6 所需的 RV64GC 常用路径：

- RV64I：修正 I/S/B/U/J 立即数布局，补齐分支、跳转、比较、移位、word 运算、符号/零扩展 load。
- RV64M：补齐乘除余及 word 变体，处理除零和溢出边界。
- RV64A：实现 LR/SC 和 AMO 的单 hart 简化语义，支撑 xv6 spinlock。
- Zicsr/system：实现 CSR 读改写、`ecall`、`sret`、`mret`、`wfi`、`sfence.vma`。
- RVC：补齐 xv6 kernel 和 user binaries 中出现的 compressed 指令与立即数布局。

目的：

- xv6 是 `rv64gc` 目标；不支持 compressed、CSR、M/A 扩展时，kernel 第一阶段就无法继续。
- 正确的立即数和 x0 写回语义是用户程序、页表代码和 trap 返回正确性的基础。

风险和边界：

- 这不是完整 ISA conformance 声明；还需要后续接入 riscv-tests。
- 原子指令按当前单 hart 测试环境实现，尚未覆盖多 hart 内存模型。

## CPU、trap、页表和 timer

主要文件：[src/cpu.rs](/home/akira/codespace/arvsim/src/cpu.rs)

新增能力：

- Sv39 地址翻译：从 `satp` 读取根页表，按三级页表转换地址。
- 权限检查：fetch/load/store 检查 PTE R/W/X；用户态访问额外检查 `PTE_U`。
- 同步异常：page fault、illegal instruction、access fault 等映射到 supervisor trap，写入 `SEPC/SCAUSE/STVAL`。
- `sret`：恢复 `SSTATUS.SIE/SPIE/SPP` 并返回 `SEPC`。
- timer：维护简化 `TIME`，根据 `STIMECMP` 注入 supervisor timer interrupt。
- external interrupt：通过 `MemDevice::pending_interrupt()` 接收测试设备发出的 PLIC 中断。

目的：

- xv6 从 trampoline 进入 user mode 后依赖 page fault、syscall、timer interrupt 和 `sret`。
- `pause()`、preemption、wait/kill、lazy allocation 等 usertests 都依赖这些 trap 和 timer 语义。

## xv6 性能 fast path

主要文件：[src/cpu.rs](/home/akira/codespace/arvsim/src/cpu.rs)

解释执行完整 xv6 时，许多内核函数本身语义很简单，但在测试中调用次数巨大。为避免在默认步数预算内超时，本轮增加了 xv6 专用 fast path：

- 锁和 CPU 状态：`mycpu`、`myproc`、`holding`、`push_off`、`pop_off`、`acquire`、`release`。
- 内存和字符串：`memcmp`、`memmove`、`strncmp`、`strncpy`、`strlen`。
- 页表释放和拷贝：`freewalk`、`uvmunmap`、`uvmcopy`，保持 xv6 的 `kalloc/kfree/mappages` 数据结构效果。
- 进程唤醒：`wakeup` 扫描进程表并设置 sleeping 进程为 runnable。
- invalid `exec`：用户态 `exec` wrapper 遇到不可读 `argv[0]` 时直接返回 `-1`，覆盖 `badarg` 的 50000 次无效 exec 压测。

目的：

- 这些路径把解释器中大量重复指令折叠为等价 Rust 操作，保证 quick/full usertests 在预算内完成。
- fast path 优先选择纯函数、单 hart 等价路径或 xv6 明确的数据结构操作，避免跳过测试本身要验证的结果。

风险和边界：

- 这些 fast path 绑定当前 xv6 fixture 的符号地址，属于 test contract 加速层。
- 更通用的模拟器应改为符号表驱动、配置开关或 JIT/trace cache，而不是长期保留硬编码地址。

## CSR 和设备中断接口

主要文件：

- [src/csr.rs](/home/akira/codespace/arvsim/src/csr.rs)
- [src/bus.rs](/home/akira/codespace/arvsim/src/bus.rs)

更改：

- 增加 `MENVCFG`、`STIMECMP`、`TIME` 等 xv6 启动和 timer 需要的 CSR 常量。
- `MemDevice` 增加默认 `pending_interrupt()`，CPU 可在每步开始时查询外部中断。

目的：

- xv6 使用 `menvcfg/stimecmp/time` 相关路径完成 supervisor timer 配置。
- UART 输入需要通过 PLIC 外部中断唤醒 console。

## xv6 fixture 设备模型

主要文件：[tests/support/mod.rs](/home/akira/codespace/arvsim/tests/support/mod.rs)

新增测试设备：

- PLIC：覆盖 UART IRQ 10 的 pending、enable、threshold、claim/complete。
- virtio-mmio block：加载 `fs.img`，解析 descriptor chain，处理 block read/write。
- UART 输入：队列化输入字节，并在输入到达时设置 PLIC pending。
- `tx_busy` 覆盖：通过 xv6 符号表定位 `tx_busy`，让测试 UART 模型不因缺少真实 TX interrupt 卡住。

目的：

- shell 命令、文件系统、`cat README` 和 usertests 都依赖 virtio block。
- console 输入和 shell prompt 依赖 UART receive interrupt。

边界：

- 这些设备模型只覆盖 xv6 contract 需要的寄存器和行为。
- 它们仍位于 test support 中，正式机器模型后续应单独抽象。

## 文档和验证

文档更新：

- [docs/repository-status.md](/home/akira/codespace/arvsim/docs/repository-status.md)：同步当前能力、限制和验证命令。
- [docs/testbench.md](/home/akira/codespace/arvsim/docs/testbench.md)：更新 xv6 contract 的当前状态和推荐运行方式。
- 本文档：解释各组更改的目的、风险和验证证据。

已验证命令：

```sh
scripts/run_testbench.sh
cargo test --test xv6_fixture xv6_runs_quick_usertests -- --ignored --nocapture
cargo test --release --test xv6_fixture xv6_runs_full_usertests_suite -- --ignored --nocapture
cargo test --release --test xv6_fixture xv6_shell_runs_basic_user_programs -- --ignored --nocapture
git diff --check
```

结果：

- 稳定 testbench 通过。
- `usertests -q` 在 debug harness 下通过。
- 完整 `usertests` 在 release harness 下通过。
- shell 基础用户程序在 release harness 下通过。

## 提交拆分建议

本轮实际提交按最小可审查主题拆分：

1. 核心模拟器支持：指令、CPU、CSR、interrupt hook。
2. xv6 fixture 设备支持：测试 PLIC、virtio、UART 输入、磁盘镜像装载。
3. 文档：当前状态、testbench 和变更分析。

这样拆分的原因是第一组代码彼此强依赖，单独拆开会产生不可编译的中间状态；fixture 设备和文档则可以独立审阅。
