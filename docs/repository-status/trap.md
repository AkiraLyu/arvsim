# `src/trap.rs`：异常与中断原因

## 功能与实现

`Exception` 用枚举表示取指、访存、非法指令、断点、U/S/M 环境调用和页错误，并携带相关地址、PC 或原始指令。`InterruptCause` 单独表示六种当前支持的标准软件、定时器和外部中断；`InterruptSet` 表示一个或多个同时有效的 `mip` 位。`INTERRUPT_FLAG` 只用于最终的 `mcause/scause` 编码。异常与中断路由及状态切换由 CPU 完成。

## 实现状态

同步异常枚举覆盖当前执行核心会产生的原因。加载地址未对齐使用与规范一致的 `LoadAddrMisaligned` 名称；`IllegalInstruction` 同时表示不支持的编码和当前特权级或陷阱设置不允许执行的指令。环境调用会按当前特权级构造对应成员；CPU 再根据 `MEDELEG` 选择机器或监督模式入口。设备和总线只传递 `InterruptSet`，不会把最终 trap 编码或最高位标志混入设备协议。

## 公共接口

- `Exception`，派生 `Debug`、`Copy`、`Clone`、`PartialEq` 和 `Eq`。
- `Exception::cause() -> u64`。
- `Exception::value() -> u64`。
- `InterruptCause::{code, mask, encoded}`。
- `InterruptSet::{EMPTY, from_cause, bits, insert, merge, contains, is_empty}`。
- `INTERRUPT_FLAG`。

## 依赖关系

`MemDevice` 以 `Exception` 作为读写错误类型，并通过 `InterruptSet` 报告中断。CPU 使用异常 `cause/value` 或中断 `encoded` 写入机器或监督模式 trap CSR；PLIC 使用 `InterruptCause` 配置上下文输出。

## 已知问题与改进建议

- 没有实现 `Display` 和 `std::error::Error`，宿主调用方只能使用调试格式。
- 异常与中断尚未统一为一个 `Trap` 类型；宿主级致命错误也仍复用 `Exception` 通道。
