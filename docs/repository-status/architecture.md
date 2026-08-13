# 仓库结构与代码组成

## 组成

- `src/` 同时构建 `arvsim` 库和命令行程序，主要由镜像装载器、`Platform`、`Machine` 和 CPU 解释器组成。
- `tests/` 包含测试用例、镜像与符号工具，不再实现运行时设备。
- `scripts/` 用于获取 xv6 源码、生成测试文件、运行测试和创建临时交互程序。
- `Cargo.toml` 使用 Rust 2024 版，包版本为 `0.1.0`，不依赖第三方 Rust 库。

## 实现状态

命令行程序通过 `Platform` 检查 DRAM、MMIO 和复位向量，再创建 `Bus` 与 `Machine`。`VirtPlatform` 在该通用组装层上连接共享 DRAM、16550 UART、PLIC 和 virtio-blk。`Machine` 统一管理设备和 CPU 的复位、单步与连续运行：每个机器步骤先推进设备，再执行 CPU；连续运行、测试辅助代码和 xv6 临时运行程序都经过同一入口。CPU 的复位和单步接口只在 crate 内可见。地址空间、DMA、中断连线和运行生命周期均由正式库实现，测试不再复制设备逻辑。

## 公共接口

- Rust 库：`arvsim::{bus,cfg,clint,cpu,csr,dram,instruction,interrupt,loader,machine,plic,trap,uart,virt_platform,virtio}`。
- 可执行程序：`cargo run -- [OPTIONS] <IMAGE>`，支持裸二进制/ELF64 镜像、运行限制、调试和 DRAM/UART 配置，见 [`main.md`](./main.md)。
- 测试入口：`cargo test` 和 `scripts/run_testbench.sh`。

## 依赖关系

- 所有内存和 MMIO 错误统一依赖 `trap::Exception`。
- `Dram::new`、命令行程序、部分 xv6 测试配置和 `VirtPlatformConfig::default` 会读取 `cfg` 中的默认地址；CPU 的 crate 内构造器由平台显式传入复位向量和栈指针。
- 命令行程序的基础模式通过 `Platform::build` 创建 `Machine`；xv6 测试通过 `VirtPlatform::build` 使用相同的 `Platform`、`Bus`、`Dram`、`Uart` 和 `Machine`，并增加正式 PLIC 与 virtio-blk。
- UART 和 virtio 通过 `InterruptLine` 连接 PLIC；virtio 通过 `GuestMemory` 访问共享 DRAM，通过 `BlockBackend` 访问介质。

## 已知问题

- `Machine` 已按固定的每步 10 周期推进设备，但没有独立事件队列、空闲时钟推进或高效跳时；UART 虽支持可配置发送延迟，其他设备仍同步处理。
- CLINT/ACLINT 仍未接入；`virtio` 当前只提供块设备、单个 split queue 和内存后端。
- `Platform` 和正式设备支持自定义 DRAM 地址与容量；xv6 加速器会接收实际 DRAM 范围，但仍与特定 xv6 数据结构和页大小耦合。
- `target/testbench` 同时保存克隆的仓库、xv6 测试文件和生成代码。结果是否可复现仍取决于网络、上游分支和本机工具链。
