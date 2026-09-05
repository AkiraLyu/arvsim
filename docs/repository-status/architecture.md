# 目录结构与模块分工

## 目录和文件

- `src/` 提供库和主命令行程序，包括指令解释器、内存、设备、平台配置和镜像加载。
- `src/paging.rs` 统一定义页大小、Sv39 页表位和页号转换函数，仅供库内部使用。
- `src/cpu/xv6_accelerator.rs` 实现可选的 xv6 内核加速。直接创建 CPU 时默认关闭加速。
- `src/xv6.rs` 加载内核和磁盘镜像，创建运行 xv6 所需的平台。开启加速时才检查版本记录和内核符号。
- `examples/xv6.rs` 提供 xv6 交互示例，由启动脚本调用。
- `tests/` 准备测试输入、调用库接口并检查结果，不实现设备、指令、ELF 解析或系统调用。
- `scripts/` 获取和构建外部源码、运行测试、设置和恢复终端；`fixtures/xv6-revision` 保存默认使用的 xv6 提交号。

项目使用 Rust 2024 版，不依赖第三方 Rust 库。构建 xv6 镜像还需要 Git、RISC-V 交叉编译工具链和相关构建工具。

## 程序如何运行

`Platform` 检查内存、MMIO 地址范围、入口地址和初始栈，再创建 `Bus` 与 `Machine`。`VirtPlatform` 在此基础上连接共享 DRAM、UART、PLIC 和 Virtio 块设备。`Machine::step` 先更新设备状态，再执行 CPU 单步；`run` 重复调用同一个单步方法。

CPU 通过 `MemDevice` 访问物理内存、查询中断和 LR/SC 保留区域的写入版本号。UART 和 Virtio 通过 `InterruptLine` 向 PLIC 发送中断信号。Virtio 通过 `GuestMemory` 读写共享 DRAM，通过 `BlockBackend` 读写磁盘数据。CPU 和 DMA 写入都会更新 DRAM 的页写入版本号，因此 CPU 能发现保留区域被修改。

普通程序镜像可通过主命令行或 `Platform` 运行。xv6 示例和测试共同调用 `Xv6Fixture::build`，使用同一套设备实现。构建脚本与库读取同一个 xv6 提交号文件；镜像目录保存实际提交号和 SHA-256 校验清单。

## 当前限制

平台只模拟一个硬件线程，每步固定增加 10 个周期。设备同步处理请求，没有事件队列，也不会在空闲时直接跳到下一个事件；CLINT/ACLINT 尚未实现。xv6 内核加速依赖固定版本的数据结构布局，不能用来证明原始指令完全符合规范。完整说明见 [后续计划](./gaps-and-roadmap.md)。
