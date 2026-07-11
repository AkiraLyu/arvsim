# `src/cpu.rs`：CPU、MMU、trap 与 xv6 快速路径

## 功能与实现思路

`Cpu` 保存 32 个通用寄存器、PC、总线、CSR、软件周期计数以及私有复位向量/初始栈指针。`step()` 的顺序为：周期推进 → 中断 → xv6 快速路径 → 取指/地址翻译 → 解码执行 → PC 更新。`run()` 根据运行选项重复调用 `step()`。CPU 通过 `Box<dyn MemDevice>` 与具体机器解耦。

## 当前状态

部分实现。单步解释器、监督模式 trap 返回、Sv39 三级页表遍历、计时器中断和外部中断接入已可用，但都是面向单 hart/xv6 的简化语义。`new/reset` 恢复配置的 PC、SP、CSR 和周期；每个 `step` 把 `cycles/TIME` 增加 10。`Machine` 当前仍直接转发到这些 CPU 方法，所以时钟和中断轮询实际归 CPU 所有。

## 对外接口

- `Cpu` 的 `registers`、`pc`、`bus`、`csr`、`cycles` 公开，复位向量和初始 SP 私有。
- `MemoryAccess::{Fetch, Load, Store}`。
- `DebugLevel::{Off, Pc, Full}`、`RunOptions { max_steps, debug }` 和 `RunOutcome`。
- `Xv6Accelerator` 保存动态解析的 xv6 函数与全局对象地址。
- `Cpu::{new, with_reset_vector, reset, step, run, set_xv6_accelerator, clear_xv6_accelerator, translate, enter_supervisor_trap, supervisor_return, dump_pc, dump_registers}`。

## 地址翻译与 trap

- `satp.mode=0` 直接使用物理地址，mode 8 走 Sv39，其他 mode 返回页错误。
- 取指统一读取 4 字节后再判断是否为压缩指令；映射末端只有 2 字节可读时会错误地产生访问失败。
- 遍历三级 PTE，检查 V、非法 W&&!R、R/W/X 和简单 U 位条件，支持 superpage 地址拼接。
- 同步异常在 `STVEC != 0` 时统一进入 supervisor trap，写 `SEPC/SCAUSE/STVAL/SSTATUS`。
- 中断只在 `SSTATUS.SIE` 打开时检查；定时器优先于外部中断。定时器条件为 `TIME >= STIMECMP`，外部中断由总线返回 supervisor external cause。

## xv6 专用快速路径

CPU 可通过显式 `Xv6Accelerator` 配置替代 xv6 函数执行，包括 `mycpu/myproc`、自旋锁和关中断嵌套、`memcmp/memmove/strncmp/strncpy/strlen`、`uvmunmap/freewalk/uvmcopy`、页分配/释放、`wakeup`，以及用户 `exec` 无效参数特例。通用 CPU 默认不启用加速；测试支撑从当前 kernel ELF 符号表解析函数和全局对象地址后注入配置，避免链接地址变化在错误 PC 上触发加速。结构偏移、数组步长和用户态 `exec` 地址仍是兼容性常量。完成快速路径后通常以 `ra` 作为返回 PC。

## 耦合方式

- 调用 `instruction::{decode,execute}`，后者又直接修改 CPU。
- 依赖 `csr`、`cfg`、`Exception` 和 `MemDevice`。
- 快速路径通过 `Xv6Accelerator` 与具体符号地址解耦，测试支撑负责解析 ELF 并调用 `set_xv6_accelerator`；数据布局和内存分配器语义仍与 xv6 强耦合。

## 已知问题和优化方向

- 没有显式 privilege 字段，而用 `pc < DRAM_BASE` 推断用户态；trap delegation、SPP 和 ecall cause 因此不可靠。
- `run()` 可报告步数上限或异常，但仍没有 guest 主动 halt/exit 协议；`ebreak` 对 CLI 仍被当作失败异常。
- 指令提交用“执行后 PC 是否变化”判断是否顺序前进；合法的零偏移 branch/jump 会被错误地再加 4。
- Sv39 未检查虚拟地址 canonical form、A/D 位、superpage PPN 对齐、SUM/MXR、ASID/TLB；页表访存错误也未统一转换为页错误。
- `STVEC` vectored mode、M-mode trap、delegation 和 pending 位更新不完整；timer pending 由 `TIME/STIMECMP` 临时计算，不反映到 `SIP.STIP`，且把 `STIMECMP=0` 特判为禁用。
- kernel 函数和对象地址已动态解析，但结构偏移、`NPROC`、CPU/proc 步长及用户态 `exec` 地址仍可能随 xv6 版本变化；快速路径也会绕开真实指令、锁和内存序语义。部分字符串/内存快速路径使用 guest 提供的长度循环或分配宿主 `Vec`，启用 accelerator 时不应把 guest 视为不可信输入。
- 建议下一步优先完善 privilege/trap 状态机和 MMU，并为 accelerator 增加布局版本校验或调试信息驱动的结构描述。
