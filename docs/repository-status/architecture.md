# 仓库结构与代码组成

## 组成与职责

- `src/` 提供库和主命令行程序，包括指令解释器、内存、设备、平台组装和镜像装载。
- `src/paging.rs` 集中定义页大小、Sv39 页表位和编码函数；它是私有模块。
- `src/cpu/xv6_accelerator.rs` 隔离可选的 xv6 内核加速，普通 CPU 默认关闭它。
- `src/xv6.rs` 装载内核和磁盘，直接组装正式平台；仅加速模式依赖版本记录与内核符号。
- `examples/xv6.rs` 提供交互式 xv6 运行入口，由启动脚本调用。
- `tests/` 只构造测试输入、调用正式接口并断言结果，不实现设备、指令、ELF 解析或系统调用。
- `scripts/` 获取和构建外部源码、编排测试及管理终端；`fixtures/xv6-revision` 保存默认 xv6 提交。

包使用 Rust 2024 版，不依赖第三方 Rust 库。外部 xv6 镜像仍需 Git、交叉编译工具链及构建工具。

## 运行与依赖

`Platform` 检查 DRAM、MMIO、入口和初始栈，再创建 `Bus` 与 `Machine`。`VirtPlatform` 在这一层上连接共享 DRAM、UART、PLIC 和 virtio-blk。`Machine::step` 先推进设备，再执行 CPU；`run` 复用同一入口。

CPU 通过 `MemDevice` 访问物理内存、查询中断与 LR/SC 保留版本。UART 和 Virtio 通过 `InterruptLine` 连接 PLIC；Virtio 通过 `GuestMemory` 访问共享 DRAM，通过 `BlockBackend` 访问块介质。DMA 和 CPU 写入使用同一内存写入接口，因此均能使保留失效。

普通镜像使用主命令行或 `Platform`。xv6 示例与测试共同调用 `Xv6Fixture::build`，无需包含测试源码或维护另一套设备。构建脚本与库共同读取固定提交文件；镜像目录保存实际提交和 SHA-256 清单。

## 当前限制

平台只有一个硬件线程，每步固定推进 10 个周期。设备同步执行，没有事件队列或空闲跳时；CLINT/ACLINT 尚未实现。xv6 内核加速依赖固定版本的结构布局，不能替代原始指令的符合性测试。完整限制见 [后续计划](./gaps-and-roadmap.md)。
