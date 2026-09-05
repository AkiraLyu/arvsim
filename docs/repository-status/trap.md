# `src/trap.rs`：异常与中断原因

## 功能

`Exception` 表示取指、内存访问、非法指令、断点、U/S/M 环境调用，以及页表转换或页面权限检查失败等异常，并保存相关地址、PC 或原始指令。`InterruptCause` 表示当前支持的六种软件、定时器和外部中断；`InterruptSet` 则保存同时有效的中断位，位的位置与 `mip` 一致。

CPU 根据这些信息选择处理入口，并更新状态寄存器。`INTERRUPT_FLAG` 只用于设置最终写入 `mcause/scause` 的中断标志位。

## 处理规则

`LoadAddrMisaligned` 表示加载地址未对齐。`IllegalInstruction` 既表示不支持的指令编码，也表示当前特权级或控制位不允许执行的指令。环境调用根据当前特权级生成对应异常，再由 CPU 根据 `MEDELEG` 选择机器模式或监督模式处理入口。

设备和总线只传递 `InterruptSet`，不会把 `mcause/scause` 的最高位标志混入中断集合。

## 公开接口

- `Exception`，支持 `Debug`、`Copy`、`Clone`、`PartialEq` 和 `Eq`。
- `Exception::cause() -> u64`：异常原因码。
- `Exception::value() -> u64`：写入 `mtval/stval` 的异常附加信息。
- `InterruptCause::{code, mask, encoded}`。
- `InterruptSet::{EMPTY, from_cause, bits, insert, merge, contains, is_empty}`。
- `INTERRUPT_FLAG`。

## 使用方式与限制

`MemDevice` 使用 `Exception` 返回读写错误，通过 `InterruptSet` 报告中断。CPU 将原因码和附加信息写入机器模式或监督模式的异常寄存器。PLIC 使用 `InterruptCause` 配置中断输出。

`Exception` 尚未实现 `Display` 和 `std::error::Error`，调用方只能用调试格式输出。异常和中断也没有合并为统一的 `Trap` 类型；运行接口中为模拟器执行失败预留的返回通道仍使用 `Exception`。
