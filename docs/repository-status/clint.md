# `src/clint.rs`：尚未实现的 CLINT 模块

目前只有模块说明和公开路径 `arvsim::clint`，没有类型、常量、寄存器实现或测试，也没有其他代码依赖它。

## 与现有定时器的区别

当前计时和监督模式定时器比较由 `Cpu::tick/timer_is_pending` 完成。启用 `menvcfg.STCE` 后，CPU 比较 `TIME` 与 `STIMECMP`，把结果反映到 `MIP/SIP` 的硬件 `STIP` 位；关闭 STCE 后清除该硬件位。

这部分实现的是 Sstc 扩展。它没有 CLINT 的 MMIO 寄存器，也不会产生机器模式的软件中断 `MSIP` 或定时器中断 `MTIP`。

## 后续实现

先确定目标平台使用传统 CLINT 还是 ACLINT 布局，再实现 `msip`、`mtime`、`mtimecmp` 或对应的独立设备。设备通过 `MemDevice::tick/reset` 参与机器的时钟更新和复位，并为每个硬件线程提供本地中断信号。CPU 负责接收信号并进入相应的中断处理程序。

现有 `STIMECMP/STIP` 逻辑作为 Sstc 功能保留，不能代替 CLINT。
