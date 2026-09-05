# `src/virtio.rs`：Virtio MMIO 块设备

实现 Virtio 1.2 现代 MMIO、64 位特性协商、单个分离式队列（split virtqueue）、块读写、完成状态、中断确认和复位。依据 [Virtio 1.2](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html)。

## 请求处理

- 检查队列参数、描述符索引、标志、循环、读写方向和扇区范围。
- 请求字段与描述符边界独立：请求头可以分散在多个描述符中，数据可以与请求头或状态共用描述符。
- 写入前检查所有数据区、状态字节及完成环记录。传输缓冲固定为 64 KiB，避免按客体长度分配大缓冲。
- `used.len` 只报告已初始化的连续前缀。部分读盘失败时，末尾状态字节与有效数据之间仍有空洞，不能把状态字节计入前缀；规范允许少报写入量。
- 完成操作只更新标准状态与完成环，不访问 xv6 驱动私有字段。

设备始终提供 `VIRTIO_F_VERSION_1` 并拒绝未提供的特性。规范允许设备在驱动未接受 VERSION_1 时继续运行；当前保留这一行为，以兼容固定版本的 xv6 驱动。

## 接口与限制

`GuestMemory` 提供无副作用的 `validate_read/validate_write` 和 DMA 读写，单次失败不得留下部分写入；正式实现为 `Shared<Dram>`。`BlockBackend` 提供容量、只读属性及随机读写，`MemoryBlockBackend` 只允许等容量替换内容。

测试覆盖普通完成、字段跨描述符、共享描述符、DMA 与完成元数据预检查、部分后端失败的前缀长度，以及中断和复位。故障由不可读扇区触发，报告前缀与实际 DMA 写入范围核对，不固定后端调用次数或传输分块大小。

当前不支持 packed ring、间接描述符、EVENT_IDX、多队列、discard、write-zeroes、文件介质或异步 I/O。队列通知同步处理请求；无法定位有效完成信息时设置 `DEVICE_NEEDS_RESET`。
