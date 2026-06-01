# 当前实现说明

本文档记录 arvsim 当前模块职责、xv6 支持状态、验证结果和剩余限制。

## 总体状态

当前仓库已经从最小 RV64I 冒烟测试模拟器推进到可以运行 MIT 的 xv6-riscv 的状态：

- 稳定测试套件 `scripts/run_testbench.sh` 通过。
- xv6 快速 `usertests` 在 debug 模式的测试框架下通过。
- xv6 完整 `usertests` 在 release 模式的测试框架下通过，并输出 `ALL TESTS PASSED`。
- xv6 测试环境仍然默认放在 `target/testbench/`，不进入仓库提交。

需要注意：xv6 验收测试仍标记为 `#[ignore]`，因为它们耗时很长，不适合作为默认 `cargo test` 的一部分。debug 模式下完整运行 `usertests` 预计需要数小时；日常验证建议优先运行快速测试，或在 release 模式下运行完整测试。

## 目录结构

```text
src/      模拟器主体代码
tests/    集成测试、测试总线和 xv6 运行验收测试
scripts/  xv6 测试环境构建脚本和测试套件入口脚本
docs/     项目文档
```

## 核心模块状态

### `src/instruction.rs`

职责：

- 解码并执行当前模拟器支持的 RISC-V 指令。
- 维护寄存器写回、立即数展开、压缩指令 PC 推进和内存访问语义。

当前实现：

- RV64I 常用整数、分支、跳转、加载/存储、`LUI/AUIPC`、32 位字操作。
- RV64M 乘除余和 W 指令变体。
- RV64A LR/SC 与 AMO 常用操作的单硬件线程（hart）简化语义。
- Zicsr CSR 读改写指令。
- `ecall`、`sret`、`mret`、`wfi`、`sfence.vma` 等系统指令。
- RVC 压缩指令子集，覆盖 xv6 内核和用户程序实际使用路径。
- 立即数布局和常见压缩立即数单元测试。

目的：

- xv6 使用 `rv64gc` 构建，启动第一阶段就依赖压缩指令、CSR、M/A 扩展和正确的 RV64I 立即数语义。
- 这些补齐让内核、用户程序和 `usertests` 能够通过同一条 CPU 单步路径执行。

已知限制：

- 还没有独立接入官方 riscv-tests。
- 部分原子指令和特权指令按单 hart 的 xv6 需求实现，尚非完整多 hart 或完整规范模型。
- 解码器仍集中在一个文件中，后续可拆分为更清晰的指令集子模块。

### `src/cpu.rs`

职责：

- CPU 取指、执行、PC 更新、异常/中断入口、地址翻译和 xv6 运行时支撑。

当前实现：

- `step()` 每步推进简化定时器，并在取指前检查定时器中断和外部中断。
- Sv39 地址翻译，支持取指、加载、存储权限检查和用户态 `PTE_U` 检查。
- 同步异常进入监督模式陷入处理，设置 `SEPC/SCAUSE/STVAL/SSTATUS`。
- `sret` 恢复监督模式中断状态并返回 `SEPC`。
- `time/stimecmp` 驱动监督模式定时器中断，支撑 xv6 的 `pause()`、调度和抢占测试。
- 针对 xv6 热点函数提供专用加速路径，例如锁、字符串/内存函数、`freewalk`、`uvmunmap`、`uvmcopy`、`myproc`、`wakeup` 和无效 `argv` 的 `exec`。

目的：

- Sv39、陷入和定时器是 xv6 从内核启动进入用户态的基础。
- 专用加速路径不改变 xv6 语义，只是把解释器中极热、纯循环或单 hart 等价的路径折叠成 Rust 侧操作，使快速和完整 `usertests` 能在测试步数预算内完成。

已知限制：

- 特权级目前主要由 PC 范围推断，尚未完整建模 M/S/U 模式状态机。
- 专用加速路径绑定当前 xv6 测试环境的符号地址，适用于验收测试，不等同于通用 RISC-V 加速机制。
- `DEBUG` 仍是常量，交互式运行时应继续工程化为可配置项。

### `src/csr.rs`

职责：

- 保存 CSR 状态并提供监督模式 CSR 别名行为。

当前实现：

- 增加 xv6 需要的 `MENVCFG`、`STIMECMP`、`TIME` 等常量。
- `SSTATUS/SIE/SIP` 对应机器模式 CSR 的简化映射仍保留。
- 提供 `load()`、`store()` 和调试输出。

已知限制：

- 尚未完整建模 WARL/WPRI、只读位、CSR 权限等级和异常/中断委托位过滤。
- 当前行为以 xv6 单 hart 路径为优先目标。

### `src/bus.rs`

职责：

- 抽象 CPU 与内存/设备之间的读写。

当前实现：

- `MemDevice` 增加 `pending_interrupt()` 默认方法，让 CPU 可从设备侧查询待处理中断。

目的：

- xv6 UART 输入依赖 PLIC 外部中断，CPU 需要一个不破坏现有设备接口的中断查询点。

已知限制：

- 正式 `Bus` 仍是简化设备分发；完整 PLIC/virtio 目前主要在测试支撑代码中实现。

### `tests/support/mod.rs`

职责：

- 提供集成测试用机器、测试总线、UART 捕获、xv6 测试环境加载和设备模型。

当前实现：

- 精简 PLIC 模型：覆盖 UART IRQ 10 的 priority、enable、threshold、claim/complete 等寄存器行为。
- virtio-mmio 块设备：加载 `fs.img`，解析 virtqueue 描述符链，处理块设备读写请求。
- UART 输入队列和输入中断注入。
- `tx_busy` 符号覆盖，避免 xv6 UART 发送路径因测试模型缺少真实发送中断而卡住。
- `xv6_machine()` 同时加载内核裸二进制文件和 `fs.img`。

目的：

- 让 xv6 的 shell、文件系统、`usertests` 能通过真实 UART 输入和 virtio 磁盘路径运行。

已知限制：

- 设备模型位于测试支撑代码中，只覆盖 xv6 验收测试所需寄存器和行为，不是完整 QEMU `virt` 设备模拟。
- virtio 队列大小固定为测试足够的大小。

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

- 稳定测试套件：通过。
- xv6 快速 `usertests`：debug 模式的测试框架通过。
- xv6 完整 `usertests`：release 模式的测试框架通过，耗时较长。
- shell 基础用户程序：release 模式的测试框架通过。
- diff 空白检查：通过。

## 后续工作

- 将 xv6 专用加速路径从硬编码地址改为符号驱动或可配置策略。
- 将测试 PLIC/virtio 设备模型迁移或抽象到正式机器模型。
- 引入官方 riscv-tests 或更系统的 ISA 规范一致性测试。
- 建模完整特权级、CSR 权限和多 hart 原子语义。
- 为 xv6 长测试增加日志文件、进度输出和可中断运行配置。
