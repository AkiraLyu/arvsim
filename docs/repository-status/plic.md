# `src/plic.rs`：PLIC 占位模块

## 功能与当前状态

文件为空，仅保留公开模块路径，正式库没有 PLIC 类型或 MMIO 寄存器实现，状态为占位。

## 对外接口

只有 `arvsim::plic` 模块路径。

## 耦合方式

正式 `Bus` 可以轮询任意 `MemDevice::pending_interrupt`，但 CLI 未挂载 PLIC。唯一可工作的简化 PLIC 位于 `tests/support/mod.rs`，只仲裁 UART IRQ 10。

## 优化方向

把测试 PLIC 提升为正式设备并去除单 IRQ 假设；实现 source priority、pending、enable、context threshold、claim/complete，多 context/hart 和设备 raise/lower API；测试机器与 CLI 共用同一实现。
