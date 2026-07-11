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

- 所有 CSR 均可读写，不检查当前特权级、只读编码、WARL/WPRI 或未实现 CSR。
- `load/store` 是接受任意 `usize` 的公开 API，地址超过 4095 会直接越界 panic；指令路径虽只产生 12 位地址，下游直接调用没有保护。
- `SSTATUS` 掩码和 SIE/SIP 别名只是子集；`MIDELEG/MIP/MIE` 也缺少规范写掩码。硬件 pending 位与软件存储没有区分，设备或 guest 可互相覆盖状态。
- supervisor timer pending 由 CPU 临时比较而不是写入 `MIP/SIP.STIP`，读 CSR 观察不到 CPU 即将交付的 timer interrupt；counter/Sstc 权限也未实现。
- 建议由 CSR 层接收当前 privilege 和访问类型，返回非法指令错误；为每个实现 CSR 定义读写掩码、硬件 pending 输入和副作用，并增加非法地址、只读位和 timer 可见性测试。
