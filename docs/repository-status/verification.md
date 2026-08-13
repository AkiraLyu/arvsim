# 验证记录

## 本轮执行结果

执行日期：2026-08-12。基线为 `main` 分支提交 `17ad107`，同时包含尚未提交的实现与文档变更；以下结果对应文档描述的当前工作区代码。2026-07-26 审查的 65 条问题已整改，但 2026-08-12 复审另有 11 条活动发现，测试通过不表示这些边界问题已经修复。

### 默认与静态检查

`cargo test --all-targets` 通过：

- 库单元测试：71 个通过。
- `src/main.rs`：4 个通过。
- `tests/cli.rs`：3 个通过。
- `tests/rv64i_smoke.rs`：6 个通过。
- `tests/xv6_fixture.rs`：1 个通过，4 个未执行。
- 合计：85 个通过，4 个未执行，没有失败。

其他检查：

- `cargo test --doc`：通过；当前没有文档测试。
- `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`：通过。
- `cargo fmt --all -- --check`：通过。
- `cargo clippy --all-targets -- -D warnings`：通过。
- `git diff --check`：通过。
- `bash -n scripts/build_xv6_fixture.sh scripts/run_testbench.sh scripts/run_xv6_cli.sh`：通过。
- `ARVSIM_REQUIRE_XV6_FIXTURE=1 cargo test --test xv6_fixture xv6_fixture_artifacts_are_well_formed_when_present`：通过。

默认测试除原有的镜像装载、参数解析、平台组装、基础指令和 UART 外，还覆盖：

- U/S/M 复位状态、异常委托、机器模式不可向下委托，以及 `sret/mret` 状态恢复。
- 中断委托、全局使能、优先级和 `tvec` 向量模式。
- CSR 特权级、只读和未实现地址检查，以及 TVM、TSR、计数器和 Sstc 权限。
- 16 个 PMP 表项、锁定、TOR/NAPOT、空 TOR、最低编号优先、部分覆盖和 MPRV。
- Sv39 的 U/S/SUM/MXR 权限、完整 8 字节 A/D 位写回、规范地址、PMP 与总线错误地址。
- Sstc 的 `STIP` 可见性、STCE 关闭时的软件写入、启用时的软件状态清除，以及定时比较驱动。
- 实际执行 `mret` 进入用户模式，再由用户态 `ecall` 进入委托的监督模式异常入口。
- 总线、DRAM、UART、PLIC 和 virtio-blk 对非法宽度、溢出地址及部分越界访问的拒绝。
- 加载、存储、压缩访存和 AMO 的对齐异常。
- LR/SC 保留的建立、成功、失败、消费和存储失效。
- 8 字节存储与 AMO 的单次设备写入，以及压缩指令在两字节映射末端的取指。
- `Platform` 对空区域、地址溢出、区域重叠、复位向量越界和未对齐的拒绝，以及初始栈指针的 16 字节对齐与 DRAM 下界检查。
- `Machine::run` 复用机器单步，在中断查询前把每步 10 个周期推进传递到总线设备，并统一复位 CPU 与设备；正式 `VirtPlatform` 复位会清除 UART、PLIC 和 virtio 易失状态而保留 RAM 与块介质。
- C.LWSP 的离散立即数字段、非法 W 型 M 扩展与 MISC-MEM 编码、C.LUI 保留编码、C.MV HINT、memset 指针别名和循环页表 `freewalk` 回退。
- `Bus::attach_device` 对空区域、地址溢出和重叠返回可恢复错误，并合并多设备同时上报的中断；UART 初始化寄存器、DLAB、收发中断、发送延迟和非 ASCII 原始字节输出。
- PLIC 优先级、使能、threshold 对通知的屏蔽、同优先级最低编号仲裁、非破坏性中断查询、同一上下文的 claim/complete 与电平重入。尚未覆盖 threshold 屏蔽通知时 claim 仍应返回请求，以及 completion 按目标 enable 位校验的规范行为。
- Virtio 1.2 MMIO 标识、`VIRTIO_F_VERSION_1`、队列参数、完整 32 位 QueueNotify、split descriptor 完成、DMA 前介质边界检查、used ring 和中断确认。
- xv6 稀疏页表区间预检查、非对齐快速访存的非连续跨页映射，以及 PTE 更新事务宽度。
- ELF 混合物理地址的镜像级回退决策、镜像布局错误分类、命令行冲突参数与选项名错误，以及正式 virtio 队列长度校验。
- 小容量测试 RAM 的实际栈顶，以及 xv6 测试文件的强制存在模式。

### xv6 验证

当前工作区在移除测试设备实现、改用正式 PLIC/UART/virtio 后已有以下长测记录。本次文档复审只重新执行了默认与静态检查，没有重跑这些耗时合同：

- `cargo test --release --test xv6_fixture xv6_kernel_reaches_first_shell -- --ignored --nocapture`：通过，耗时 0.94 秒。
- `cargo test --release --test xv6_fixture xv6_shell_runs_basic_user_programs -- --ignored --nocapture`：通过，`echo`、`ls` 和 `cat README` 均完成，耗时 1.17 秒。
- `cargo test --release --test xv6_fixture xv6_runs_quick_usertests -- --ignored --nocapture`：默认 3 亿步预算内通过，观察到 `ALL TESTS PASSED`，耗时 114.91 秒。
- `cargo test --release --test xv6_fixture xv6_runs_full_usertests_suite -- --ignored --nocapture`：通过，观察到慢速测试阶段和 `ALL TESTS PASSED`，耗时 429.17 秒。

4 个 xv6 行为测试默认均为 `ignored`；本地缺少测试文件时完整性测试会明确显示 `SKIPPED`，自动化环境应设置 `ARVSIM_REQUIRE_XV6_FIXTURE=1` 将缺失视为失败。正式 UART 按延迟产生 THRE 中断，不再绕过 guest 驱动状态；稀疏页表快速路径按当前 fixture 的 xv6 语义跳过缺失 PTE，长测已验证该路径。

## 覆盖缺口

- 命令行已有参数单元测试和 3 个进程测试，但还没有覆盖 ELF、调试输出、UART 平台、地址冲突和为未来保留的致命 CPU 错误退出路径。
- 特权级、异常、PMP、Sv39、数据访问对齐和中断已有针对性单元测试，也覆盖了快速辅助函数的非连续跨页访问；尚未覆盖所有 CSR 组合、TLB/ASID、aq/rl 和多硬件线程保留失效。
- `Bus` 已覆盖基本读写、区域末端、零宽、非法宽度、访问地址溢出，以及直接挂载时的空区域、区域重叠、末端溢出和多设备中断集合并；尚未进行大量动态挂载或借用冲突的压力测试。
- 指令模块自身的测试仍主要覆盖解码、立即数和 memset 模式；很多执行规则通过 CPU 集成测试间接覆盖，缺少逐条 ISA 测试。
- `Machine` 已覆盖设备推进、复位、运行循环和平台对齐；UART 已使用可变发送时延，但仍没有事件跳时、空闲推进或多硬件线程测试。
- 正式 PLIC 和 virtio 已有独立单元测试及 xv6 连接验证；PLIC 缺少 threshold 独立 claim 和 completion enable 校验，virtio 缺少部分 DMA 失败时 used length 的测试；此外仍缺边沿网关、恶意描述符模糊测试、间接描述符、多队列和异步后端。
- ELF 测试覆盖恒等入口和混合 `p_paddr` 的装载决策，但没有非恒等 `p_vaddr/p_paddr` 的入口转换，以及入口必须属于可执行 `PT_LOAD` 的验证。
- 默认 fixture 完整性测试没有解析 `_usertests` ELF 或核对必需符号；脚本环境边界也没有自动化测试，包含零 step chunk 和更新失败后复用 checkout 的路径。
- 格式和 clippy 已手工通过，但仓库还没有持续集成必检项，也没有 Miri、模糊测试、riscv-arch-test 或覆盖率要求。
