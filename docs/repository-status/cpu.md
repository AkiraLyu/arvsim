# `src/cpu.rs`：CPU、MMU、trap 与 xv6 快速路径

## 功能与实现思路

`Cpu` 保存 32 个通用寄存器、PC、总线、CSR、软件周期计数以及私有复位向量/初始栈指针。`step()` 的顺序为：周期推进 → 中断 → xv6 快速路径 → 取指/地址翻译 → 解码执行 → PC 更新。`run()` 根据运行选项重复调用 `step()`。CPU 通过 `Box<dyn MemDevice>` 与具体机器解耦。

## 当前状态

部分实现。单步解释器、监督模式 trap 返回、Sv39 三级页表遍历、计时器中断和外部中断接入已可用，但都是面向单 hart/xv6 的简化语义。`new/reset` 设置 PC 为 DRAM 起点、SP 为 DRAM 末端；每个 `step` 把 `cycles/TIME` 增加 10。

## 对外接口

- `Cpu` 的 `registers`、`pc`、`bus`、`csr`、`cycles` 公开，复位向量和初始 SP 私有。
- `MemoryAccess::{Fetch, Load, Store}`。
- `DebugLevel::{Off, Pc, Full}`、`RunOptions { max_steps, debug }` 和 `RunOutcome`。
- `Cpu::{new, with_reset_vector, reset, step, run, translate, enter_supervisor_trap, supervisor_return, dump_pc, dump_registers}`。

## 地址翻译与 trap

- `satp.mode=0` 直接使用物理地址，mode 8 走 Sv39，其他 mode 返回页错误。
- 遍历三级 PTE，检查 V、非法 W&&!R、R/W/X 和简单 U 位条件，支持 superpage 地址拼接。
- 同步异常在 `STVEC != 0` 时统一进入 supervisor trap，写 `SEPC/SCAUSE/STVAL/SSTATUS`。
- 中断只在 `SSTATUS.SIE` 打开时检查；定时器优先于外部中断。定时器条件为 `TIME >= STIMECMP`，外部中断由总线返回 supervisor external cause。

## xv6 专用快速路径

CPU 通过固定 PC 地址替代 xv6 函数执行，包括 `mycpu/myproc`、自旋锁和关中断嵌套、`memcmp/memmove/strncmp/strncpy/strlen`、`uvmunmap/freewalk/uvmcopy`、页分配/释放、`wakeup`，以及用户 `exec` 无效参数特例。相关常量还硬编码了 `cpus`、`proc`、`kmem`、内核末端、结构偏移和步长。完成后通常以 `ra` 作为返回 PC。

## 耦合方式

- 调用 `instruction::{decode,execute}`，后者又直接修改 CPU。
- 依赖 `csr`、`cfg`、`Exception` 和 `MemDevice`。
- 快速路径与某个具体 xv6 二进制的符号地址、结构布局和内存分配器强耦合；测试支撑也有一个 `tx_busy` 地址 fallback。

## 已知问题和优化方向

- 没有显式 privilege 字段，而用 `pc < DRAM_BASE` 推断用户态；trap delegation、SPP 和 ecall cause 因此不可靠。
- `reset()` 现会恢复配置的入口/SP并清空 CSR；`run()` 可报告步数上限或异常，但仍没有 guest 主动 halt/exit 协议。
- Sv39 未检查虚拟地址 canonical form、A/D 位、superpage PPN 对齐、SUM/MXR、ASID/TLB；页表访存错误也未统一转换为页错误。
- `STVEC` vectored mode、M-mode trap、delegation 和 pending 位更新不完整。
- 固定 xv6 地址导致换 commit、编译选项或链接布局即可失效，且快速路径绕开真实指令/锁/内存序语义。
- 建议下一步优先完善 privilege/trap 状态机和 MMU；将快速路径迁入可选、版本化 accelerator 层，默认通用核心不启用。
