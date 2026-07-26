# 验证记录

## 本轮执行结果

执行日期：2026-07-16。基线为提交 `0744969`，同时包含当前工作区中的 machine 模块补全；以下结果对应文档描述的当前代码。

### 默认与静态检查

`cargo test --all-targets` 通过：

- 库单元测试：49 个通过。
- `src/main.rs`：3 个通过。
- `tests/cli.rs`：3 个通过。
- `tests/rv64i_smoke.rs`：5 个通过。
- `tests/xv6_fixture.rs`：1 个通过，4 个未执行。
- 合计：61 个通过，4 个未执行，没有失败。

其他检查：

- `cargo test --doc`：通过；当前没有文档测试。
- `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`：通过。
- `cargo fmt --all -- --check`：通过。
- `cargo clippy --all-targets -- -D warnings`：通过。
- `git diff --check`：通过。

默认测试除原有的镜像装载、参数解析、平台组装、基础指令和 UART 外，还覆盖：

- U/S/M 复位状态、异常委托、机器模式不可向下委托，以及 `sret/mret` 状态恢复。
- 中断委托、全局使能、优先级和 `tvec` 向量模式。
- CSR 特权级、只读和未实现地址检查，以及 TVM、TSR、计数器和 Sstc 权限。
- 16 个 PMP 表项、锁定、TOR/NAPOT、最低编号优先、部分覆盖和 MPRV。
- Sv39 的 U/S/SUM/MXR 权限、A/D 位、规范地址、PMP 与总线错误地址。
- Sstc 的 `STIP` 可见性及关闭 STCE 后的清除行为。
- 实际执行 `mret` 进入用户模式，再由用户态 `ecall` 进入委托的监督模式异常入口。
- 总线、DRAM、UART 和 `TestBus` 对非法宽度、溢出地址及部分越界访问的拒绝。
- 加载、存储、压缩访存和 AMO 的对齐异常。
- LR/SC 保留的建立、成功、失败、消费和存储失效。
- 8 字节存储与 AMO 的单次设备写入，以及压缩指令在两字节映射末端的取指。
- `Platform` 对空区域、地址溢出、区域重叠、复位向量越界和未对齐的拒绝，以及初始栈指针的 16 字节对齐。
- `Machine::run` 复用机器单步，在中断查询前把每步 10 个周期推进传递到总线设备，并统一复位 CPU 与设备；测试总线复位会清除易失设备状态而保留 RAM。

### xv6 验证

当前工作区执行了以下验证：

- `./scripts/run_xv6_cli.sh --boot-only`：成功进入 shell，共执行 5,620,000 步。
- `cargo test --release --test xv6_fixture xv6_runs_quick_usertests -- --ignored --nocapture`：通过，测试观察到 `ALL TESTS PASSED`，耗时 223.41 秒。

没有运行完整 usertests。该测试最长允许执行 20 亿步，仍需显式执行。4 个 xv6 行为测试默认均为 `ignored`；默认的文件检查在缺少测试文件时也会返回成功，因此不能只凭默认测试判断 xv6 是否可运行。

## 覆盖缺口

- 命令行已有参数单元测试和 3 个进程测试，但还没有覆盖 ELF、调试输出、UART 平台、地址冲突和目标程序异常退出。
- 特权级、异常、PMP、Sv39、数据访问对齐和中断已有针对性单元测试，但尚未覆盖所有 CSR 组合、跨页访问、TLB/ASID、aq/rl 和多硬件线程保留失效。
- `Bus` 已覆盖基本读写、区域末端、零宽、非法宽度和访问地址溢出。区域重叠与挂载溢出主要由 `Platform` 间接测试；中断顺序和直接调用 `Bus::attach_device` 导致进程异常退出的情况尚未覆盖。
- 指令模块自身的测试仍主要覆盖解码和立即数；很多执行规则通过 CPU 集成测试间接覆盖，缺少逐条 ISA 测试。
- `Machine` 已覆盖设备推进、复位、运行循环和平台对齐，但尚无真实异步设备、可变时延、空闲时钟推进或多硬件线程测试。
- 测试用 PLIC 和 virtio 没有独立设备测试；异常访问或错误描述符可能使进程异常退出，错误类型也可能不准确。
- 格式和 clippy 已手工通过，但仓库还没有持续集成必检项，也没有 Miri、模糊测试、riscv-arch-test 或覆盖率要求。
