# `src/dram.rs`：主存

DRAM 用连续字节向量保存小端内存，默认容量为 128 MiB。总线访问只接受 1、2、4、8 字节；镜像装载和 DMA 可访问任意长度。所有访问共用字节范围检查，拒绝物理地址溢出和越界，失败时不修改内存。

底层向量和基址只在 crate 内可见。公共接口包括 `new/with_layout`、`base/end/bytes`、`contains_range`、`load/load_bytes`、`zero_range`、`read_bytes/write_bytes`，并实现 `MemDevice`。

每个物理页维护写入版本。CPU 存储、DMA、镜像装载和清零均更新对应页版本，使同页 LR/SC 保留失效；调用方应通过受检写入接口修改内存。只读观察使用 `bytes()`。

`Platform` 和 virtio 通过 `Shared<Dram>` 共享同一内存对象。范围检查与页版本由 DRAM 维护，CPU 无需知道具体 DMA 设备。测试覆盖合法宽度、非法宽度、跨界、物理地址溢出、装载及 DMA 保留失效。

当前没有稀疏内存、快照或共享页实现。
