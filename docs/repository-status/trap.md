# `src/trap.rs`：同步异常

## 功能与实现

`Exception` 用枚举表示取指、访存、非法指令、断点、U/S/M 环境调用和页错误，并携带相关地址、PC 或原始指令。`cause()` 统一生成 `mcause/scause` 的异常原因码，`value()` 统一生成 `mtval/stval` 的附加值。异常路由和状态切换由 CPU 完成。

## 实现状态

枚举覆盖当前执行核心会产生的同步异常。环境调用会按当前特权级构造对应成员；CPU 再根据 `MEDELEG` 选择机器或监督模式入口。中断仍由总线返回带中断标志的 `u64` 原因值，不使用本枚举。

## 公共接口

- `Exception`，派生 `Debug`、`Copy`、`Clone`、`PartialEq` 和 `Eq`。
- `Exception::cause() -> u64`。
- `Exception::value() -> u64`。

## 依赖关系

`MemDevice` 以 `Exception` 作为读写错误类型；DRAM、UART、总线、CPU、指令执行和测试设备都会构造相应成员。CPU 使用 `cause/value` 写入机器或监督模式异常 CSR。

## 已知问题与改进建议

- 没有实现 `Display` 和 `std::error::Error`，宿主调用方只能使用调试格式。
- UART 仍用 `IllegalInstruction` 表示未知寄存器，错误含义不准确。
- 异常与中断没有统一为 `Trap` 类型；设备中断接口仍直接传递未经封装的 `u64`。
- 后续可增加独立的 `Interrupt` 和 `Trap`，并把设备寄存器错误改为访问错误或专用设备错误。
