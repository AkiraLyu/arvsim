# `src/trap.rs`：异常类型

## 功能与实现思路

`Exception` 用带地址/原始值的枚举统一传递取指、访存、非法指令、断点、环境调用和页错误。模块本身只定义数据，不负责 trap 路由。

## 当前状态

异常种类基本覆盖当前 CPU 使用的同步异常，但没有中断枚举、权限级状态或结构化 cause/stval 转换。所有 variant 都携带 `u64`，即使某些环境调用字段当前会被忽略。

## 对外接口

公开 `Exception`，派生 `Debug`、`Copy`、`Clone`。variant 包括 instruction/load/store 的 misaligned、access fault、page fault，以及 illegal instruction、breakpoint 和 U/S/M-mode ecall。

## 耦合方式

`MemDevice` 以它作为错误类型；DRAM、UART、总线、CPU、指令执行和测试设备均直接构造 variant。CPU 私有函数再把它映射为 `scause/stval`。

## 不完善之处和优化方向

- 没有实现 `Display`/`Error`，调用方只能使用调试格式。
- `IllegalInstruction` 同时被 UART 用来表示未知寄存器，语义污染。
- 中断以裸 `u64 scause` 从总线返回，未与异常建模统一。
- 建议拆分 `Exception`、`Interrupt`、`Trap`，集中维护 cause 编码和 trap value，并为设备访问错误使用 access fault 或独立设备错误。
