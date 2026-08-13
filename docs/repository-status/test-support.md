# `tests/support/mod.rs`：测试机器与 xv6 fixture 工具

## 功能与实现

该模块只负责测试编排：创建正式 `VirtPlatform`、装载 flat kernel 和磁盘镜像、从 kernel 与 usertests ELF 解析加速符号、编译临时 RV64 汇编，以及按正式 `BufferedUartBackend` 的输出组织等待和失败诊断。UART、PLIC、virtio-blk、DMA、RAM 地址分发和中断连接均来自 `src/`，测试目录不再实现运行时设备。

`TestMachine` 包装 `VirtMachine` 并保留缓冲 UART 与内存块后端句柄。固定步数运行和文本等待全部调用 `Machine::step`；文本搜索只检查新增输出并保留跨边界重叠区。机器复位由正式设备执行：DRAM 和块介质保留，UART 输入输出、PLIC 和 virtio transport 易失状态清除。

## 实现状态

- 冒烟平台可选择实际 DRAM 容量，栈顶由正式 `Platform` 按 DRAM 末端计算并保持 16 字节对齐。
- xv6 平台使用 `VirtPlatformConfig::default` 的集中地址与 IRQ 配置。
- kernel 函数、全局对象和用户态 `exec` 均从实际 ELF 符号表解析；不再读取或绕过 xv6 的 `tx_busy`，virtio 也不再知道或修改 `struct buf` 的私有偏移。
- UART 输入经过正式 RX/IIR/PLIC/claim-complete 路径；磁盘完成经过正式 used ring、virtio 中断状态和 PLIC IRQ 1 路径。
- 不再维护测试专用 MMIO 日志；设备寄存器边界由各正式模块的单元测试覆盖。

## 测试辅助接口

- `TestMachine::{empty, rv64_smoke, with_flat_binary, run_steps, queue_uart_input, queue_uart_bytes, uart_output, uart_output_string, run_until_uart_contains, require_uart_contains, require_uart_lacks}`。
- `build_flat_asm`、`xv6_dir`、`toolchain`、xv6 fixture 路径与检查、`xv6_machine`、`require_tool`、`run`。

## 依赖关系

依赖库中的 `VirtPlatform`、`Machine`、`BufferedUartBackend`、`MemoryBlockBackend`、`Xv6Accelerator` 和 `Exception`。文件与工具链操作仍只属于测试层；正式设备模块不依赖 `tests/` 或 xv6。

## 已知问题

- fixture 仍依赖外部 xv6 仓库、RISC-V 工具链和本机命令，版本固定与哈希校验尚未完成。
- `Xv6Accelerator` 仍依赖特定 xv6 数据结构布局，虽然函数地址已不再硬编码。
- `require_tool` 把由 `TOOLPREFIX` 形成的工具名直接拼入 `sh -c` 源码；空格或 shell 元字符会改变探测命令，应改为参数传递或在 Rust 中遍历 `PATH`。
- UART 等待仍只在阶段结束后统一检查失败标记，xv6 的 panic 信息可能延迟到当前等待目标超时后才报告。
- `run_xv6_cli.sh` 仍通过临时 Rust 程序包含本模块；应改成正式可执行目标或 example。
