# `src/virtio.rs`：Virtio MMIO 块设备

规范基准：[Virtual I/O Device (VIRTIO) Version 1.2](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html)。

## 功能与实现

实现 Virtio 1.2 的现代 MMIO transport 和块设备子集：magic/version/device/vendor、64 位特性选择、设备状态与复位、单个 split virtqueue、32 位对齐的队列配置、available/used ring、描述符链、IN/OUT 块请求、请求状态、通知抑制、InterruptStatus/ACK，以及配置区的 64 位容量。

设备始终提供 `VIRTIO_F_VERSION_1`，只接受已提供的 driver feature；只在 `DRIVER_OK` 且 QueueReady 时读取队列。描述符编号、保留标志、环增量、链循环、方向、地址运算、完整 32 位通知值、队列上限和广告介质范围均受检查。介质范围预检在 DMA 开始前完成，避免越界介质请求部分改写 guest 内存或介质；全部 guest DMA 范围尚未统一预检。guest 可控传输使用固定 64 KiB 暂存块分段执行，不会按描述符长度直接分配宿主内存。请求完成只更新标准状态字节和 used ring，不引用 xv6 的 `struct buf` 或其他驱动私有布局。

## 抽象边界

- `GuestMemory` 提供 DMA 读写；正式实现可直接使用 `Shared<Dram>`。
- `BlockBackend` 提供稳定容量、随机读写和只读属性。
- `MemoryBlockBackend` 是可共享的内存介质，只允许等容量替换内容。
- `InterruptLine` 在 `InterruptStatus != 0` 时保持高电平，由 PLIC 处理 IRQ 编号和 claim/complete。

## 公共接口

- `VirtioBlock::{new, base, size, device_status, interrupt_line}` 与 `MemDevice` 实现。
- `VirtioBlockConfig`、`VirtioBlockError`。
- `GuestMemory`、`DmaError`。
- `BlockBackend`、`MemoryBlockBackend`、`BlockError`。
- MMIO、块设备和扇区常量。

## 已知限制

- 只实现块设备和一个 split queue；未提供 packed ring、间接描述符、EVENT_IDX、多队列、discard、write-zeroes 或可变配置通知。
- 队列通知同步处理所有当前 available 请求，没有异步时延或并行 I/O。
- `MemoryBlockBackend` 适用于测试与嵌入；尚无正式文件后端和持久化错误恢复。
- IN 请求的多个数据描述符中，若前段 DMA 写入成功而后段 guest 地址失败，设备会写入 `IOERR`，但 used length 固定为 1，没有包含已经写入的数据字节。
- guest 违反描述符协议且无法定位标准状态字节时，设备设置 `DEVICE_NEEDS_RESET` 和配置变化中断；尚未覆盖所有恶意链组合的模糊测试。
