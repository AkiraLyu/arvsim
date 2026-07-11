# `src/clint.rs`：CLINT 占位模块

## 功能与当前状态

实现为空，仅有模块级说明并通过 `src/lib.rs` 暴露模块名；没有类型、常量、寄存器或测试，状态为占位。

## 对外接口

只有 `arvsim::clint` 模块路径，没有可调用项。

## 耦合方式

当前无代码耦合。时间推进和 supervisor timer compare 被直接实现在 `Cpu::tick/timer_is_pending` 中，没有 MMIO CLINT。该路径比较 `TIME/STIMECMP` 并直接交付 supervisor timer，实际更接近未完整建模权限的 Sstc 快捷路径，不会产生 CLINT 应有的 `MSIP/MTIP` 电平，也不会把 pending 反映到 `MIP/SIP`。

## 优化方向

明确目标平台采用 legacy CLINT 兼容布局还是 ACLINT；实现 `msip/mtime/mtimecmp` 或对应拆分设备，通过 `Machine` 的统一时钟推进并提交每 hart 的类型化本地中断。CPU 只消费中断线并负责 M-mode trap 路由；现有 `STIMECMP/STIP` 兼容逻辑应作为独立 Sstc 路径保留或修正，不能冒充 CLINT。
