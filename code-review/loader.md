# `src/loader.rs`：镜像装载器审查记录

> 本文属于代码审查报告（基线：提交 `17ad107` 的当前工作区，2026-08-12），只记录本轮仍成立的审查发现与修改建议，未改动实现代码。总览见 [README](./README.md)。对应的现状文档：[loader.md](/home/akira/codespace/arvsim/docs/repository-status/loader.md)。

## 审查范围与总体判断

覆盖格式识别、ELF64 头与程序头边界、`PT_LOAD` 地址选择、BSS 清零、错误分类和入口返回。文件及 DRAM 范围检查采用受检算术；新发现集中在 `p_vaddr`、`p_paddr` 与 `e_entry` 的地址域不一致。

共 1 条发现：高危 0、中危 1、低危 0。

## 发现的问题

### 1. 段按 `p_paddr` 装载时，虚拟 `e_entry` 没有转换为复位物理地址

- 位置：[`src/loader.rs:131`](/home/akira/codespace/arvsim/src/loader.rs#L131)、[`src/loader.rs:166`](/home/akira/codespace/arvsim/src/loader.rs#L166)、[`src/loader.rs:178`](/home/akira/codespace/arvsim/src/loader.rs#L178)、[`src/loader.rs:211`](/home/akira/codespace/arvsim/src/loader.rs#L211)
- 分级：中危 · 缺陷
- 规范：[ELF Header 的 `e_entry`](https://gabi.xinuos.com/elf/02-eheader.html)、[Program Header 的 `p_vaddr/p_paddr`](https://gabi.xinuos.com/elf/07-pheader.html)

ELF `e_entry` 表示程序入口的虚拟地址；`p_vaddr` 和 `p_paddr` 分别表示段的虚拟地址与物理地址。当前装载器在任一 `PT_LOAD.p_paddr` 非零时把各段写到 `p_paddr`，但最后仍直接检查并返回原始 `e_entry`。CPU 复位时处于 Bare 机器模式，因此不会自动把该虚拟入口转换到已装载的物理位置。

例如段的 `p_vaddr=0x0040_0000`、`p_paddr=0x8000_0000`、`e_entry=0x0040_0100` 时，镜像内容会正确写到 DRAM，却因入口不在 DRAM 而被拒绝。若虚拟入口恰好也落在 DRAM，程序还可能从未装载或错误的字节启动。

**修改建议：**

```rust
let mut mapped_entry = None;

// 处理每个 PT_LOAD 时：
let virtual_end = virtual_address
    .checked_add(memory_size as u64)
    .ok_or_else(|| LoadError::InvalidImage("ELF virtual segment overflows".into()))?;
if (virtual_address..virtual_end).contains(&entry) {
    let candidate = address
        .checked_add(entry - virtual_address)
        .ok_or_else(|| LoadError::InvalidImage("ELF entry mapping overflows".into()))?;
    if mapped_entry.replace(candidate).is_some() {
        return invalid("ELF entry belongs to overlapping load segments");
    }
}

let entry = mapped_entry
    .ok_or_else(|| LoadError::InvalidImage("ELF entry is outside loadable segments".into()))?;
```

还应读取 `p_flags` 并要求入口段可执行。另一种更窄但安全的合同是显式拒绝入口所在段 `p_vaddr != 实际装载地址` 的镜像，并在接口文档中说明只支持恒等映射；不能继续把两个地址域静默混用。

增加至少两个回归测试：非恒等 `p_vaddr/p_paddr` 的入口正确映射；入口位于 DRAM 但不属于任何可装载段时拒绝。
