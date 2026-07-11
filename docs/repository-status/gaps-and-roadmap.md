# 不完善之处和优化路线

本文只保留当前未完成的工作；已经落地并有回归证据的历史事项不再占用路线图编号。

## P0：修复已确认的正确性和健壮性问题

1. 用结构化 next-PC/执行结果替代“执行后 PC 是否变化”的提交启发式；当前合法的零偏移 branch/jump 会被错误地额外前进 4 字节。
2. 在总线边界限制访问宽度并使用 checked range；当前 `size=0` 可命中设备，DRAM/TestBus 的大宽度读写可能因移位或下标运算在宿主 panic。
3. 为 system/atomic 边界补精确回归并修正语义：`ecall` 目前绕过统一异常入口且固定进入 S-mode，LR 使用 Store 权限翻译，SC 不维护 reservation。

## P1：收敛架构边界

1. 收口运行期 `Machine` 边界：`run()` 必须复用 `Machine::step()`，测试/runner 不再直接推进 CPU，并为平台设备增加 tick/reset 生命周期。
2. 将测试 UART、PLIC、virtio-blk 提升为正式模块；测试与 CLI 使用同一设备实现。
3. 把 `MemDevice::write` 扩为 `u64` 并引入受限访问宽度；定义类型化 interrupt source/cause，区分本地、外部和最终 trap cause。
4. 为默认关闭且使用 ELF 动态符号的 `Xv6Accelerator` 增加 fixture commit、结构布局、符号范围和用户二进制地址校验，并限制 guest 可控的宿主分配/循环规模。
5. 缩小公共字段和模块可见性，给库提供稳定的构建器、loader 和运行控制接口。

## P2：提升 RISC-V 语义完整性

1. 显式建模 U/S/M privilege、delegation、trap 向量模式和返回状态。
2. 完善 CSR 权限、只读/WARL、pending/enable 关系与 counter/timer；把当前 Sstc 式快捷计时器与未来 CLINT/ACLINT 平台设备分开。
3. 完善 Sv39 canonical address、A/D、SUM/MXR、superpage 对齐、页表访问错误和可选 TLB。
4. 实现取指/访存对齐异常、LR/SC reservation、aq/rl、多 hart 内存序；修复压缩指令位于映射末端时被 4 字节统一取指误伤的问题。
5. 将指令模块按 I/M/A/C/system 拆分并接入 riscv-arch-test，明确并验证对外宣称的 ISA 字符串。

## P3：可复现性、性能与工程质量

1. 固定 xv6 commit 和 fixture 校验值；让 fixture 缺失成为明确 skip/fail 而非绿色通过，把短 boot smoke 放入 CI、usertests 放入 nightly。
2. 把 `run_xv6_cli.sh` 的临时 runner 替换为正式 Cargo binary/example。
3. 将现有 `off/pc/full` 调试级别扩展为可注入 trace sink 和交互式 debugger，避免核心直接打印 stdout。
4. 评估 basic-block cache、译码缓存或 JIT；先用 benchmark 量化热点，再减少对 guest 函数语义的硬编码。
5. 把当前可通过的 `cargo fmt --check` 和 clippy 固化为 CI gate，并继续加入覆盖率、Miri、fuzz/property tests 与设备非法输入测试。

## 建议验收顺序

每个阶段都应先通过小型 ISA/设备测试，再运行 xv6 boot smoke，最后运行 quick/full usertests。只有正式 CLI 与测试使用同一平台实现后，xv6 测试结果才能代表对外运行能力。
