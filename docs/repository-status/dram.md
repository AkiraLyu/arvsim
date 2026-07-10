# `src/dram.rs`：主存

## 功能与实现思路

用清零的 `Vec<u8>` 表示从 `cfg::DRAM_BASE` 开始的连续小端内存；支持把 flat binary 整体复制到内存起点，并通过 `MemDevice` 接入总线。

## 当前状态

基本读写和镜像大小检查已实现，支持默认布局或运行时 `base/size` 布局，并增加指定地址字节加载和范围清零。单元测试覆盖 1/2/4 字节读写、越界和文件加载。地址低于基址或访问末端越界会返回对应 access fault。

## 对外接口

- `Dram { pub dram: Vec<u8>, pub base: u64 }`
- `Dram::{new, with_layout, end, load, load_bytes, zero_range}`
- `Default` 和 `MemDevice` 实现

## 耦合方式

容量和基址来自 `cfg`；错误使用 `trap::Exception`；由 CLI 和 Bus 直接构造/挂载。测试支撑没有复用此类型，而是维护自己的 RAM 向量。

## 不完善之处和优化方向

- `read/write` 接受任意 `size`；大于 8 的读移位和大于 4 的 `u32` 写移位可能溢出或 panic。
- 公开底层 `Vec` 和 `base`，无法保持不变量。
- 镜像解析已移到 `loader`，DRAM 本身只负责范围内字节装载和清零。
- 默认实例仍分配 128 MiB；可配置布局仍是连续 `Vec`，没有稀疏内存、快照或共享页机制。
- 建议限制访问宽度、使用安全切片转换并封装公开字段。
