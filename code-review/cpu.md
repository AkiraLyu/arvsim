# `src/cpu.rs`：CPU 与 xv6 加速路径审查记录

> 本文属于代码审查报告（基线：提交 `17ad107` 的当前工作区，2026-08-12），只记录本轮仍成立的审查发现与修改建议，未改动实现代码。总览见 [README](./README.md)。对应的现状文档：[cpu.md](/home/akira/codespace/arvsim/docs/repository-status/cpu.md)。

## 审查范围与总体判断

覆盖 CPU 单步、异常与中断、PMP、Sv39、访存辅助和可选 xv6 快速路径。本轮重点复核上一轮整改后的地址运算、跨页访问、页表预检查和宿主分配。标准执行路径未发现新的高危问题；可选加速器仍允许一次机器单步执行由 guest 控制的无上限工作。

共 1 条发现：高危 0、中危 1、低危 0。

## 发现的问题

### 1. 快速路径没有单步工作预算，`memmove` 仍可增长到 guest 给定长度

- 位置：[`src/cpu.rs:833`](/home/akira/codespace/arvsim/src/cpu.rs#L833)、[`src/cpu.rs:851`](/home/akira/codespace/arvsim/src/cpu.rs#L851)、[`src/cpu.rs:869`](/home/akira/codespace/arvsim/src/cpu.rs#L869)、[`src/cpu.rs:920`](/home/akira/codespace/arvsim/src/cpu.rs#L920)、[`src/cpu.rs:943`](/home/akira/codespace/arvsim/src/cpu.rs#L943)、[`src/cpu.rs:993`](/home/akira/codespace/arvsim/src/cpu.rs#L993)
- 分级：中危 · 健壮性
- 备注：状态文档已概括为“不适合运行不可信输入”，本条补充具体触发方式和统一修复边界

`memcmp/memmove/strncmp/strncpy` 的长度来自 guest 寄存器，页表区间路径也按 guest 给定页数或字节数循环；`strlen` 更没有显式终点。这些循环都在一次 `Machine::step()` 内完成，因此 `RunOptions::max_steps` 无法中断它们。

`fast_xv6_memmove` 虽已从 `Vec::with_capacity(len)` 改为按需 `push`，最终仍会保存全部 `len` 字节。`len` 经 `u32` 截断后最大仍为 4 GiB；监督模式 guest 可以用 Sv39 别名让很大的虚拟范围持续可读，从而让宿主长时间停在一次单步内并耗尽内存。稀疏页表预检查同样可能在畸形但可遍历的巨大范围上耗费无界时间。

**修改建议：**

```rust
pub struct Xv6Accelerator {
    // 现有符号和布局字段……
    pub max_fast_path_bytes: u64,
    pub max_fast_path_pages: u64,
}

fn fast_len_allowed(&self, len: u64) -> bool {
    self.xv6_accelerator
        .is_some_and(|config| len <= config.max_fast_path_bytes)
}

// 必须在任何写入、分配或页表修改之前决定是否回退。
if !self.fast_len_allowed(len as u64) {
    return Ok(false);
}
```

为无长度的 `strlen` 设置扫描上限，达到上限即回退到真实 guest 指令；为页表路径单独限制页数。`memmove` 在预算内也可用固定大小临时块分段处理：根据区间重叠关系选择从前往后或从后往前复制，且每块仍须先读完再写回，避免保存完整向量。配置入口应拒绝零预算、反向 DRAM/进程区间和明显不一致的范围。

回归测试至少应覆盖：超过预算时无写入并返回 `Ok(false)`；上限内重叠复制保持 `memmove` 语义；无 NUL 字符的别名映射不会让一次 `Machine::step` 无限运行。
