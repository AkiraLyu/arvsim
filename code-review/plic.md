# `src/plic.rs`：PLIC 审查记录

> 本文属于代码审查报告（基线：提交 `17ad107` 的当前工作区，2026-08-12），只记录本轮仍成立的审查发现与修改建议，未改动实现代码。总览见 [README](./README.md)。对应的现状文档：[plic.md](/home/akira/codespace/arvsim/docs/repository-status/plic.md)。

## 审查范围与总体判断

覆盖源优先级、pending/enable/threshold、claim/complete、多个目标上下文、电平网关、MMIO 布局和复位。地址边界、同优先级仲裁、非破坏性中断查询及电平重入实现清晰；claim 和 completion 各有一处与 PLIC 1.0.0 明确要求不符的行为。

共 2 条发现：高危 0、中危 2、低危 0。

## 发现的问题

### 1. claim 与中断通知共用 threshold 过滤，无法在阈值屏蔽时轮询请求

- 位置：[`src/plic.rs:265`](/home/akira/codespace/arvsim/src/plic.rs#L265)、[`src/plic.rs:281`](/home/akira/codespace/arvsim/src/plic.rs#L281)
- 分级：中危 · 缺陷
- 规范：[PLIC 1.0.0，Interrupt Claims](https://docs.riscv.org/reference/plic/v1.0.0/plic-claims.html)

`eligible_source` 同时检查 `priority > threshold`，`pending_interrupts` 和 `claim` 都调用它。PLIC 规范明确允许软件把 threshold 设为最大值以关闭外部中断通知，同时通过读取 claim 寄存器轮询 pending 且 enabled 的非零优先级源；claim 不受目标 threshold 影响。

当前只要 `priority <= threshold`，即使源已经 pending 且对上下文 enabled，claim 也返回 0。默认 xv6 把 threshold 保持为 0，因此现有启动测试无法暴露该偏差。

**修改建议：**

```rust
fn best_source(&self, context: usize, apply_threshold: bool) -> Option<usize> {
    self.pending_sources
        .iter()
        .copied()
        .filter(|source| {
            self.enabled[context][*source]
                && self.priorities[*source] != 0
                && (!apply_threshold
                    || self.priorities[*source] > self.thresholds[context])
        })
        .max_by(|left, right| {
            self.priorities[*left]
                .cmp(&self.priorities[*right])
                .then_with(|| right.cmp(left))
        })
}

// EIP/CPU 通知应用 threshold；claim 不应用。
let notification = self.best_source(context, true);
let claim = self.best_source(context, false);
```

增加回归测试：断言源优先级为 1、threshold 为 7 时 `pending_interrupts()` 为空，但读取 claim 仍返回该源编号。

### 2. completion 按记录的 claimant 校验，而规范要求按目标当前 enable 位校验

- 位置：[`src/plic.rs:288`](/home/akira/codespace/arvsim/src/plic.rs#L288)、[`src/plic.rs:292`](/home/akira/codespace/arvsim/src/plic.rs#L292)
- 分级：中危 · 缺陷
- 规范：[PLIC 1.0.0，Interrupt Completion](https://docs.riscv.org/reference/plic/plic-completion.html)

实现用 `claimed_by[source]` 记录 claim 上下文，并且只接受同一上下文随后写回相同源编号。PLIC 规范规定控制器不检查 completion ID 是否等于该目标最后一次 claim 的 ID；只要该源当前对写入 completion 的目标 enabled，就应把 completion 转发给网关，否则静默忽略。

因此当前实现有两个相反错误：另一个已 enable 的上下文写 completion 会被误拒绝；原 claimant 在 claim 后已经清除 enable 位时，completion 仍会被误接受。前者可让网关永久保持 busy，后者会过早重新开放网关并重新锁存仍为高电平的源。

**修改建议：**

```rust
fn complete(&mut self, context: usize, source: u32) {
    let source = source as usize;
    if source == 0
        || source >= self.gateway_busy.len()
        || !self.enabled[context][source]
    {
        return;
    }

    self.gateway_busy[source] = false;
    self.sync_gateways();
}
```

删除 `claimed_by`，并增加两组测试：已 enable 的不同上下文可以完成请求；claim 后清除该上下文 enable 位时 completion 被忽略。
