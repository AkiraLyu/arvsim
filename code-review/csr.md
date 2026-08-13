# `src/csr.rs`：CSR 存储与别名审查记录

> 本文属于代码审查报告（基线：提交 `17ad107` 的当前工作区，2026-08-12），只记录本轮仍成立的审查发现与修改建议，未改动实现代码。总览见 [README](./README.md)。对应的现状文档：[csr.md](/home/akira/codespace/arvsim/docs/repository-status/csr.md)。

## 审查范围与总体判断

覆盖 CSR 地址空间、监督模式别名、写掩码、WARL 字段、硬件 pending 位、Sstc 和 PMP 配置。CPU 指令路径会先验证 12 位地址、实现状态和权限，当前 guest 无法通过 CSR 指令触发本条问题；问题位于对外公开的安全 Rust 接口。

共 1 条发现：高危 0、中危 0、低危 1。

## 发现的问题

### 1. 公开 `load/store` 接受任意 `usize`，超出 12 位地址会数组越界 panic

- 位置：[`src/csr.rs:194`](/home/akira/codespace/arvsim/src/csr.rs#L194)、[`src/csr.rs:208`](/home/akira/codespace/arvsim/src/csr.rs#L208)
- 分级：低危 · 健壮性
- 备注：对应状态文档已列为已知问题

`Csr` 内部数组固定为 4096 项，但 `load/store` 是公开安全函数且没有声明或检查 `addr < 4096`。多数 match 分支最终执行 `self.csrs[addr]`，库调用方传入 4096 或更大值即可触发宿主 panic。CPU 的 CSR 指令字段天然只有 12 位，因此默认 guest 路径不受影响。

**修改建议：**

```rust
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct CsrAddress(u16);

impl TryFrom<usize> for CsrAddress {
    type Error = CsrAddressError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        if value >= 4096 {
            return Err(CsrAddressError(value));
        }
        Ok(Self(value as u16))
    }
}
```

让公开访问接收 `CsrAddress` 或返回 `Result/Option`，把现有未检查数组入口降为 crate 私有。若为了兼容保留 `usize` 签名，也应先返回明确错误，而不是依赖索引 panic；同时补充 4095、4096 和 `usize::MAX` 的边界测试。
