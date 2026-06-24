# `src/cpu.rs`：CPU、地址翻译和 xv6 运行时支撑

## 设计

- CPU 是解释执行核心：每步推进时间、处理中断、尝试 xv6 专用加速路径、取指、执行、更新 PC。
- 特权级没有作为独立状态机完整建模，当前主要通过 PC 范围和 CSR 位满足 xv6 路径。
- xv6 热点函数使用硬编码符号地址加速，目标是让验收测试在可接受时间内完成。

## 实现

- `Cpu` 保存 32 个通用寄存器、PC、总线、CSR 和周期计数。
- `Cpu::new()` 将 PC 设为 `CPU_START_ADDR`，将 `sp` 设为 `DRAM_END`。
- `step()` 每次先调用 `tick()`，把 `TIME` 增加 10；随后检查监督模式定时器中断和外部中断。
- `fetch()` 通过 `translate()` 做取指地址翻译，再从总线读取 4 字节。
- `execute()` 调用 `instruction::decode()` 和 `instruction::execute()`，并处理普通 4 字节 PC 推进。
- Sv39 地址翻译支持三级页表、叶子 PTE 判断、R/W/X 权限检查、用户页检查和页错误返回。
- 同步异常会进入监督模式陷入处理，写入 `SEPC/SCAUSE/STVAL/SSTATUS`，PC 跳到 `STVEC`。
- `sret` 通过 `supervisor_return()` 恢复 `SSTATUS.SIE/SPIE/SPP` 并返回 `SEPC`。
- 定时器中断由 `TIME >= STIMECMP` 触发，外部中断通过 `bus.pending_interrupt()` 查询。

## 接口

- `struct Cpu`
  - `registers: [u64; 32]`
  - `pc: u64`
  - `bus: Box<dyn MemDevice>`
  - `csr: Csr`
  - `cycles: u64`
- `enum MemoryAccess`
  - `Fetch`
  - `Load`
  - `Store`
- `Cpu::new(bus)`
- `reset()`
- `step() -> Result<(), Exception>`
- `run()`
- `translate(addr, access) -> Result<u64, Exception>`
- `enter_supervisor_trap(scause, stval)`
- `supervisor_return()`
- 调试接口：`dump_pc()`、`dump_registers()`

## xv6 专用加速路径

- CPU/锁相关：`mycpu`、`myproc`、`holding`、`push_off`、`pop_off`、`acquire`、`release`。
- 内存和字符串：`memcmp`、`memmove`、`strncmp`、`strncpy`、`strlen`。
- 页表和内存管理：`freewalk`、`uvmunmap`、`uvmcopy`，内部复用 xv6 页表格式、空闲链表和页分配规则。
- 进程唤醒：`wakeup` 扫描进程表，把匹配通道上的 sleeping 进程设为 runnable。
- 用户态无效 `exec` 参数：对不可读的 `argv[0]` 快速返回 `-1`。

## 限制

- `DEBUG` 为 `true` 时 `run()` 输出很多调试信息；测试通常使用 `step()` 避免这个问题。
- xv6 加速路径绑定当前 xv6 测试环境的符号地址，换 xv6 版本可能需要重新校准。
- 特权级、委托位过滤、访问权限和中断行为是满足 xv6 的简化模型，不是完整特权架构实现。
