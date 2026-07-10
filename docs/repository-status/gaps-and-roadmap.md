# 不完善之处和优化路线

## P0：先恢复可信的主路径

1. 修正 `store(SIP)` 使用错误源寄存器的问题。
2. 修复 RV64 ignored 测试的 9 步问题，引入明确 guest halt/signature，避免用魔法步数判断完成。

已完成：CLI 参数与非零错误退出、flat/ELF 装载、步数/调试/基础平台配置、`Cpu::run()` 复用 `step()`、结构化 `RunOutcome`，以及复位时清空 CSR。

## P1：收敛架构边界

1. 建立 `Machine`/`Platform` 层，集中组装 CPU、地址空间、时钟和中断控制器。
2. 将测试 UART、PLIC、virtio-blk 提升为正式模块；测试与 CLI 使用同一设备实现。
3. 把 `MemDevice::write` 扩为 `u64` 并引入受限访问宽度；定义类型化 interrupt source/cause。
4. 把 xv6 固定地址优化移出通用 CPU，做成默认关闭、带 fixture 版本/符号校验的 accelerator。
5. 缩小公共字段和模块可见性，给库提供稳定的构建器、loader 和运行控制接口。

## P2：提升 RISC-V 语义完整性

1. 显式建模 U/S/M privilege、delegation、trap 向量模式和返回状态。
2. 完善 CSR 权限、只读/WARL、pending/enable 关系与 counter/timer。
3. 完善 Sv39 canonical address、A/D、SUM/MXR、superpage 对齐、页表访问错误和可选 TLB。
4. 实现对齐异常、LR/SC reservation、aq/rl、多 hart 内存序；明确支持的 ISA 字符串。
5. 将指令模块按 I/M/A/C/system 拆分并接入 riscv-arch-test。

## P3：可复现性、性能与工程质量

1. 固定 xv6 commit 和 fixture 校验值；把短 boot smoke 放入 CI，把 usertests 放入 nightly。
2. 把 `run_xv6_cli.sh` 的临时 runner 替换为正式 Cargo binary/example。
3. 将现有 `off/pc/full` 调试级别扩展为可注入 trace sink 和交互式 debugger，避免核心直接打印 stdout。
4. 评估 basic-block cache、译码缓存或 JIT；先用 benchmark 量化热点，再减少对 guest 函数语义的硬编码。
5. 加入 `cargo fmt --check`、clippy、覆盖率、fuzz/property tests，并为设备非法输入补充健壮性测试。

## 建议验收顺序

每个阶段都应先通过小型 ISA/设备测试，再运行 xv6 boot smoke，最后运行 quick/full usertests。只有正式 CLI 与测试使用同一平台实现后，xv6 测试结果才能代表对外运行能力。
