# `src/dram.rs`：主存

`Dram` 用连续的字节数组模拟小端内存，默认容量为 128 MiB。总线每次可读写 1、2、4、8 字节；镜像加载和 DMA 可读写任意长度。所有访问使用同一套范围检查，拒绝地址计算溢出和越界访问，失败时不修改内存。

底层数组和基址只在库内可见。公开接口包括 `new/with_layout`、`base/end/bytes`、`contains_range`、`load/load_bytes`、`zero_range`、`read_bytes/write_bytes`，并实现 `MemDevice`。

每个物理页都有写入版本号。CPU 存储、DMA、镜像加载和清零都会更新该版本号，让 CPU 判断 LR/SC 保留区域是否被修改。调用方应使用带范围检查的写入方法；只读访问使用 `bytes()`。

`Platform` 和 Virtio 通过 `Shared<Dram>` 使用同一块内存。DRAM 统一检查范围和记录写入，CPU 无需了解具体的 DMA 设备。测试覆盖各种合法访问宽度、非法宽度、越界、地址溢出、镜像加载，以及 DMA 写入后 LR/SC 保留失效的情况。

当前没有稀疏内存、内存快照或共享页功能。
