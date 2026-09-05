# `src/cpu.rs`：CPU、地址保护、异常处理与 xv6 加速

## 功能与实现

`Cpu` 保存 32 个通用寄存器、PC、U/S/M 特权级、总线、CSR、周期计数、复位向量和初始栈指针。crate 内部的 `step()` 依次推进 CPU 时间、同步硬件中断状态、处理中断、取指、尝试可选的 xv6 快速路径，再译码并执行指令。架构异常会进入目标程序的异常入口；公开的连续运行循环位于 `Machine::run`。当前标准执行路径会接管所有架构异常，宿主错误分支主要为接口扩展保留。

CPU 通过 `Box<dyn MemDevice>` 访问物理内存、MMIO 和设备中断，不依赖具体设备类型。`Machine::step` 先通过同一地址空间推进平台设备，再调用 CPU 的内部单步；CPU 自己维护 `TIME` 和周期计数，并在设备推进后查询中断。

## 实现状态

- 复位后进入机器模式；当前特权级由 `PrivilegeMode` 明确记录，不再根据 PC 猜测。
- 指令通过 `pc_written` 标记是否写入下一条 PC；未显式写 PC 时按编码长度推进 2 或 4 字节，零偏移分支、跳转和返回不会被错误地顺序递增。
- U/S/M 同步异常按 `MEDELEG` 路由；机器模式异常不会向下委托。
- 异常或中断入口维护 `xIE/xPIE/xPP`、`xEPC/xCAUSE/xTVAL`，`sret/mret` 恢复特权级和中断状态。向量模式只影响中断。
- 支持 MEI、MSI、MTI、SEI、SSI、STI 的使能、委托、全局开关和优先级。
- Sstc 在 `menvcfg.STCE` 打开时根据 `TIME >= STIMECMP` 驱动 `STIP`；硬件待处理位与软件写入的 `MIP` 状态分开保存。
- 支持 16 个 PMP 表项及 TOR、NA4、NAPOT。最低编号的重叠项优先，部分覆盖会失败，空或反向 TOR 区间不参与匹配；未锁定表项不限制机器模式，锁定表项也约束机器模式。
- 支持 Bare 和 Sv39。Sv39 会检查规范地址、保留位、U/S 权限、SUM、MXR、超级页对齐和 A/D 位；页表遍历与最终物理访问都受 PMP 约束，A/D 更新会写回完整 8 字节 PTE。
- `MPRV` 只影响机器模式的数据访问，不影响取指。翻译后的总线或 PMP 错误会报告原虚拟地址。
- 取指先读取低 16 位，仅在编码表明指令为 32 位时读取高 16 位。压缩指令位于映射末端时不再越界读取。
- 加载、存储和 AMO 会按访问宽度检查地址对齐。LR 在支持保留的内存上记录物理地址、宽度和内存页版本；SC 无论成功与否都会消费保留，普通存储、AMO、异常入口及同页 DMA 写入也会使保留失效。默认 MMIO 不支持 LR/SC。
- xv6 `freewalk` 快速路径按 Sv39 的三级结构限制递归深度；最低层仍出现非叶指针时会退出加速路径，不再因循环页表耗尽宿主栈。
- xv6 页表解除映射和复制会在修改前检查已存在的叶子与物理页范围，按当前 xv6 惰性分配语义跳过缺失的中间页表或 PTE；非叶或越界映射会在任何修改前退出加速。非对齐的多字节辅助访存会逐字节翻译，跨页映射不再误用物理连续地址。
- xv6 加速配置分别记录 guest `PHYSTOP` 与实际 DRAM 区间；页释放和批量填充不再直接读取编译期默认内存边界。guest 派生地址统一使用回绕或受检运算。内存移动不再一次性按 guest 长度预分配，但仍会在读取过程中把全部内容追加到宿主缓冲，最终内存占用仍可达到 guest 给定长度。

## 公共接口

- `Cpu` 公开 `registers`、`pc`、`bus`、`csr`、`privilege` 和 `cycles`。
- `PrivilegeMode::{User, Supervisor, Machine}`。
- `MemoryAccess::{Fetch, Load, Store}`。
- `DebugLevel::{Off, Pc, Full}`，并从 `machine` 重导出；`RunOptions` 和 `RunOutcome` 已移到 `machine`，旧的 `cpu` 路径保留兼容重导出。
- `Xv6Accelerator` 保存从 xv6 ELF 解析出的函数和全局对象地址，并显式携带 guest `phys_top` 与实际 `dram_base..dram_end`。
- `XV6_PROC_TABLE_SIZE` 集中保存当前兼容版本的 xv6 进程表布局总大小，供加速器配置代码复用。
- `Cpu::{set_xv6_accelerator, clear_xv6_accelerator, translate, enter_supervisor_trap, enter_machine_trap, supervisor_return, machine_return, dump_pc, dump_registers}`。构造、复位和单步只在 crate 内可见，外部调用方必须通过 `Platform::build` 或 `Machine::from_address_space` 创建并运行机器。

## xv6 专用加速

`Xv6Accelerator` 可以直接模拟 `mycpu/myproc`、锁和中断嵌套、部分字符串与内存函数、页表操作、页分配与释放、`wakeup`，以及一个用户态 `exec` 特例。普通 CPU 默认关闭加速。快速路径只在匹配的 U/S 特权级和 ELF 符号入口触发；访问仍经过地址翻译与 PMP 检查。批量内存填充还要求监督模式、实际 DRAM 范围、恒等映射、可写页面和循环期间不变的填充值。

## 依赖关系

- 调用 `instruction::{decode, execute}`，后者又直接修改 `Cpu`。
- 依赖 `csr`、`trap::Exception`、`bus::MemDevice` 和公共中断标志；默认内存布局由加速器构建方显式传入。
- 测试代码负责从当前 xv6 ELF 读取符号并调用 `set_xv6_accelerator`。

## 已知问题与改进建议

- 仍是单硬件线程模型；CPU 和设备每步固定增加 10 个周期，没有可变指令时延或独立事件调度。
- 当前同步、单硬件线程模型按程序顺序完成访存；尚未验证多硬件线程下的 aq/rl 和内存顺序。
- TLB、ASID 和实际的 `sfence.vma` 刷新尚未实现。PMP 也未覆盖扩展安全模型。
- 目标程序没有正常停机协议。`ebreak` 会按架构进入异常入口，不能作为通用的宿主退出信号。
- xv6 快速路径仍依赖固定结构偏移和数组步长，也会绕过真实指令与内存顺序；字符串、内存与页表循环没有统一的单步工作预算，`memmove` 的缓冲还可增长到 guest 给定长度。`Machine::run` 的步数上限无法中断单次快速路径，因此不适合运行不可信输入。函数入口由实际内核和 usertests ELF 解析，但尚未校验对应源码版本与结构布局。
- 后续仍应为独立事件调度、TLB、设备写入通知和加速器布局建立更明确的接口。
