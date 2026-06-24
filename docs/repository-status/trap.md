# `src/trap.rs`：异常类型

## 设计

- 用一个枚举表示 CPU、内存和指令执行可能产生的 RISC-V 异常。
- 该枚举是模块之间传递失败原因的共同类型。

## 实现

- 覆盖取指地址不对齐、访问错误、非法指令、断点、加载/存储异常、环境调用和页错误。
- 每个变体携带相关地址、PC 或原始指令值。

## 接口

- `Exception` 枚举。
- 主要调用方是 `MemDevice::read/write`、`Cpu::step`、`Cpu::translate` 和 `instruction::execute`。

## 限制

- 只有异常枚举，没有单独的中断类型；中断在 CPU 内部用 `scause` 数值进入监督模式陷入处理。
- 异常到 `scause/stval` 的映射在 `src/cpu.rs` 的 `exception_trap_info()` 中完成。
