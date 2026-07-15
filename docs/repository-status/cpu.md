# `src/cpu.rs`：CPU、地址保护、异常处理与 xv6 加速

## 功能与实现

`Cpu` 保存 32 个通用寄存器、PC、U/S/M 特权级、总线、CSR、周期计数、复位向量和初始栈指针。`step()` 依次推进时间、同步硬件中断状态、处理中断、取指、尝试可选的 xv6 快速路径，再译码并执行指令。架构异常会进入目标程序的异常入口；`run()` 按配置重复执行，直到达到步数上限或 `step()` 向宿主返回错误。当前标准执行路径会接管所有架构异常，宿主错误分支主要为接口扩展保留。

CPU 通过 `Box<dyn MemDevice>` 访问物理内存、MMIO 和设备中断，不依赖具体设备类型。`Machine` 仍只是转发 CPU 方法，因此时间推进和中断同步都在 CPU 内完成。

## 实现状态

- 复位后进入机器模式；当前特权级由 `PrivilegeMode` 明确记录，不再根据 PC 猜测。
- 指令通过 `pc_written` 标记是否写入下一条 PC，零偏移分支、跳转和返回不会被错误地顺序递增。
- U/S/M 同步异常按 `MEDELEG` 路由；机器模式异常不会向下委托。
- 异常或中断入口维护 `xIE/xPIE/xPP`、`xEPC/xCAUSE/xTVAL`，`sret/mret` 恢复特权级和中断状态。向量模式只影响中断。
- 支持 MEI、MSI、MTI、SEI、SSI、STI 的使能、委托、全局开关和优先级。
- Sstc 在 `menvcfg.STCE` 打开时根据 `TIME >= STIMECMP` 驱动 `STIP`；硬件待处理位与软件写入的 `MIP` 状态分开保存。
- 支持 16 个 PMP 表项及 TOR、NA4、NAPOT。最低编号的重叠项优先，部分覆盖会失败；未锁定表项不限制机器模式，锁定表项也约束机器模式。
- 支持 Bare 和 Sv39。Sv39 会检查规范地址、保留位、U/S 权限、SUM、MXR、超级页对齐和 A/D 位；页表遍历与最终物理访问都受 PMP 约束。
- `MPRV` 只影响机器模式的数据访问，不影响取指。翻译后的总线或 PMP 错误会报告原虚拟地址。

## 公共接口

- `Cpu` 公开 `registers`、`pc`、`bus`、`csr`、`privilege` 和 `cycles`。
- `PrivilegeMode::{User, Supervisor, Machine}`。
- `MemoryAccess::{Fetch, Load, Store}`。
- `DebugLevel::{Off, Pc, Full}`、`RunOptions` 和 `RunOutcome`。
- `Xv6Accelerator` 保存从 xv6 ELF 解析出的函数和全局对象地址。
- `Cpu::{new, with_reset_vector, reset, step, run, set_xv6_accelerator, clear_xv6_accelerator, translate, enter_supervisor_trap, enter_machine_trap, supervisor_return, machine_return, dump_pc, dump_registers}`。

## xv6 专用加速

`Xv6Accelerator` 可以直接模拟 `mycpu/myproc`、锁和中断嵌套、部分字符串与内存函数、页表操作、页分配与释放、`wakeup`，以及一个用户态 `exec` 特例。普通 CPU 默认关闭加速。快速路径只在匹配的 U/S 特权级和 ELF 符号入口触发；访问仍经过地址翻译与 PMP 检查。批量内存填充还要求监督模式、恒等映射和可写页面。

## 依赖关系

- 调用 `instruction::{decode, execute}`，后者又直接修改 `Cpu`。
- 依赖 `csr`、`cfg`、`trap::Exception` 和 `bus::MemDevice`。
- 测试代码负责从当前 xv6 ELF 读取符号并调用 `set_xv6_accelerator`。

## 已知问题与改进建议

- 仍是单硬件线程模型；时间每步固定增加 10，平台设备没有统一的推进和复位机制。
- 取指总是读取 4 字节，再判断是否为压缩指令。只有末尾 2 字节可读时会错误地产生取指访问异常。
- 取指会拒绝奇数地址，但尚未实现加载、存储和 AMO 的对齐检查；TLB、ASID 和实际的 `sfence.vma` 刷新也未实现。PMP 尚未覆盖扩展安全模型。
- 目标程序没有正常停机协议。`ebreak` 会按架构进入异常入口，不能作为通用的宿主退出信号。
- xv6 快速路径仍依赖固定结构偏移、数组步长、默认 DRAM 边界和用户程序地址，也会绕过真实指令与内存顺序，不适合运行不可信输入。
- 后续应优先补齐取指边界和原子/对齐语义，再为平台时钟、TLB 与加速器布局建立独立接口。
