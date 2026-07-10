# `src/csr.rs`：控制与状态寄存器

## 功能与实现思路

用 4096 项 `u64` 数组保存完整 12 位 CSR 地址空间；对 `SIE`、`SIP`、`SSTATUS` 做机器寄存器的视图映射，其余地址直接读写。

## 当前状态

部分实现。已定义当前 xv6 路径所需的 M/S-mode CSR、`STIMECMP`、`TIME` 和常用状态/中断位；支持读、写和非零项调试输出。没有规范级权限或字段约束。

## 对外接口

- 大量公开 CSR 地址和位掩码常量。
- `Csr::{new, load, store, dump_csr}`。
- `Default` 实现。

## 耦合方式

CPU 持有并直接更新 `Csr`；指令模块执行 CSR 指令时直接调用 `load/store`；没有经过权限检查或 trait。S-mode 视图依赖 `MIDELEG`、`MIE/MIP` 和 `MSTATUS` 的内部关系。

## 已知问题与优化方向

- `store(SIP)` 保留未委托位时读取的是 `MIE` 而不是 `MIP`，会把中断使能状态混入 pending 状态，疑似实现错误。
- 所有 CSR 均可读写，不检查当前特权级、只读编码、WARL/WPRI 或未实现 CSR。
- `Cpu::reset()` 不重置 CSR，复位后可能保留旧的页表和 trap 状态。
- `SSTATUS` 掩码和 SIE/SIP 别名只是子集；计时器和 counter 语义也未规范化。
- 建议由 CSR 层接收当前 privilege 和访问类型，返回非法指令错误；为每个实现 CSR 定义读写掩码和副作用，并补齐复位测试。
