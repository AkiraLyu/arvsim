# `src/clint.rs`：CLINT 占位模块

## 实现状态

目前只有模块说明和公开模块名，没有类型、常量、寄存器实现或测试。

## 公共接口

只有 `arvsim::clint` 模块路径，没有可调用项。

## 依赖关系

当前没有代码依赖。计时和监督模式定时器比较都直接写在 `Cpu::tick/timer_is_pending` 中，没有 MMIO CLINT。启用 `menvcfg.STCE` 后，CPU 比较 `TIME` 与 `STIMECMP`，并把结果作为硬件 `STIP` 映射到 `MIP/SIP`；关闭 STCE 后会清除该硬件待处理位。这属于 Sstc，不会产生 CLINT 应有的 `MSIP/MTIP` 信号。

## 实现建议

先确定目标平台使用传统 CLINT 地址布局还是 ACLINT。实现 `msip`、`mtime`、`mtimecmp` 或相应的拆分设备，并由 `Machine` 统一推进时钟。CLINT 应为每个硬件线程提供明确的本地中断信号，CPU 只负责接收信号并进入机器模式中断入口。现有 `STIMECMP/STIP` 逻辑应作为 Sstc 单独保留，不能代替 CLINT。
