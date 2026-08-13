# `src/virtio.rs`：Virtio MMIO 块设备审查记录

> 本文属于代码审查报告（基线：提交 `17ad107` 的当前工作区，2026-08-12），只记录本轮仍成立的审查发现与修改建议，未改动实现代码。总览见 [README](./README.md)。对应的现状文档：[virtio.md](/home/akira/codespace/arvsim/docs/repository-status/virtio.md)。

## 审查范围与总体判断

覆盖现代 MMIO transport、特性协商、split queue、描述符链、块请求、DMA、used ring、中断和复位。队列大小、描述符循环、介质范围和固定大小传输缓冲已有明确边界；异常 IN DMA 的 used length 仍可能与实际写入量不符。

共 1 条发现：高危 0、中危 0、低危 1。

## 发现的问题

### 1. IN 请求部分 DMA 成功后失败，used length 固定写 1 而不是实际写入总量

- 位置：[`src/virtio.rs:551`](/home/akira/codespace/arvsim/src/virtio.rs#L551)、[`src/virtio.rs:607`](/home/akira/codespace/arvsim/src/virtio.rs#L607)、[`src/virtio.rs:625`](/home/akira/codespace/arvsim/src/virtio.rs#L625)
- 分级：低危 · 缺陷
- 规范：[Virtio 1.2，Split Virtqueues](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html#x1-540005)

IN 请求可能包含多个设备可写数据描述符。`transfer` 只在开始前验证总介质范围，没有预验证全部 guest DMA 范围；它会逐描述符写入。若前一个描述符有效、后一个描述符指向 DRAM 外，前面的字节已经写入 guest，随后返回 `BackendFailure`。`process_request` 把所有传输错误统一转换为 `(IOERR, 1)`，而 `complete_request` 又会成功写入 1 字节状态，因此 used ring 的 `len=1`，实际设备写入却是“已完成的数据字节 + 状态字节”。Virtio 要求 used `len` 表示设备写入缓冲区的总字节数。

默认 xv6 使用合法连续缓冲，不触发该路径；畸形或自定义驱动会得到失真的完成元数据。

**修改建议：**

优先在产生任何副作用前验证全部数据描述符、状态描述符和 used ring 写入范围；对 `Shared<Dram>` 可增加无副作用的范围验证接口。若要允许后端中途失败，则错误必须携带已写入字节数：

```rust
struct TransferFailure {
    error: BlockError,
    device_written: u32,
}

// transfer 只返回数据区写入量；状态字节由 process_request 统一计入。
let (status, data_written) = match self.transfer(data, disk_offset, true) {
    Ok(written) => (VIRTIO_BLK_S_OK, written),
    Err(failure) => (VIRTIO_BLK_S_IOERR, failure.device_written),
};
let written_len = data_written.saturating_add(1);
```

实际重构时应先统一 `transfer` 返回值是否包含状态字节，避免重复计数。增加一个两段 IN 请求测试：第一段位于 DRAM，第二段越界，断言要么请求在写入前失败且数据不变，要么 used length 精确反映已写数据与状态字节。
