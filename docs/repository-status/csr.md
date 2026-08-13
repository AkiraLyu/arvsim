# `src/csr.rs`：控制与状态寄存器

## 功能与实现

`Csr` 用 4096 项 `u64` 数组覆盖 12 位 CSR 地址空间，并为 `SSTATUS`、`SIE` 和 `SIP` 提供机器级寄存器的监督模式视图。CPU 负责检查指令访问权限，本模块负责存储、别名、写掩码、WARL 约束和硬件待处理中断。

## 实现状态

- 实现当前 CPU 使用的 M/S 级 CSR、`TIME`、`STIMECMP`、机器标识 CSR 和 16 个 PMP 表项。
- `SSTATUS` 只暴露监督模式可见字段，包括只读为零的 VS/FS/XS 视图；`SIE/SIP` 只暴露 `MIDELEG` 委托的中断位。
- `mstatus`、`tvec`、委托寄存器、中断寄存器、计数器开关、`menvcfg`、`xepc`、`satp` 和 PMP 均有写掩码或 WARL 处理。
- UXL/SXL 固定为 RV64；`misa` 和未分配的机器标识固定返回 0；`stimecmp` 复位为 `u64::MAX`。
- 硬件产生的待处理中断与软件写入的 `MIP` 状态分开保存。CSR 读改写不会把外部硬件信号意外写回软件状态；`menvcfg.STCE=0` 时机器模式可写 `mip.STIP`，启用 Sstc 时会清除旧的软件 STIP 并改由 `stimecmp` 驱动。
- CPU 会检查 CSR 是否实现、地址编码要求的特权级、只读属性，以及 TVM、计数器和 Sstc 访问条件；非法访问产生非法指令异常。
- PMP 支持 `pmpcfg0/2`、`pmpaddr0..15`、逐项锁定和 TOR 后继项对前一地址寄存器的锁定。

## 公共接口

- 公开已实现 CSR 的地址、状态位和中断位常量。
- `Csr::{new, load, store, is_implemented, is_read_only, update_pending, dump_csr}`。
- `Default` 实现。

## 依赖关系

CPU 持有 `Csr`，负责特权级检查、异常与中断状态切换，以及硬件中断采样。指令模块先调用 CPU 的权限检查，再通过 `load/store` 完成 CSR 指令。PMP 匹配由 CPU 完成，配置、地址和配置位常量由 `Csr` 模块统一提供。

## 已知问题与改进建议

- `load/store` 是公开的底层接口，不接收当前特权级；直接调用可以绕过指令权限检查。传入超过 4095 的地址会数组越界并触发宿主 panic，安全公共接口尚未封装地址范围。
- 只实现当前执行核心需要的 CSR 子集；没有 `cycle/instret`、完整性能计数器、调试、浮点、向量或虚拟化状态。
- `misa` 固定为 0，调用方无法从 CSR 得知实际支持的 I/M/A/C 子集。
- PMP 只实现基础 16 项模型，没有粒度配置和 Smepmp 等扩展。
- 后续可用受检的 CSR 访问接口封装地址、特权级和读写类型，同时保留仅供 CPU 内部更新的硬件接口。
