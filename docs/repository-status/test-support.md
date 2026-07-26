# `tests/support/mod.rs`：测试机器与 xv6 平台

## 功能与实现

该模块为 CPU 集成测试提供 RAM、UART、PLIC、virtio-blk、MMIO 日志、xv6 测试文件路径和外部命令工具。`TestBus` 通过 `Rc<RefCell<TestBusState>>` 共享状态，并作为完整地址空间传给 `Machine`，因此测试可以在运行期间输入 UART 数据并读取输出。`TestMachine` 的固定步数和文本等待辅助函数都通过 `Machine::step()` 运行。

## 实现状态

测试专用但功能较完整：

- RAM：可选 1 MiB 冒烟测试配置或 128 MiB xv6 配置，支持裸二进制镜像。
- UART：维护输出缓冲、输入队列和 LSR 收发状态；收到输入后设置 PLIC 的 UART 待处理位。
- PLIC：为 UART 中断号 10 实现优先级、待处理位、监督模式使能、阈值和领取/完成（claim/complete）操作。
- virtio 块设备：实现关键的识别、状态和队列寄存器，以及一个长度为 8 的队列、描述符链、磁盘读写、已用描述符环和中断状态。
- 访问检查：RAM 支持 1、2、4、8 字节，UART 只支持 1 字节，virtio 只支持 4 字节，PLIC 支持不跨 32 位寄存器的 1、2、4 字节访问；非法宽度和溢出地址会返回访问错误。
- 机器控制：通过正式机器入口固定步数运行、运行到 UART 包含目标文本、失败标记检查。
- 生命周期：机器复位会保留已装载 RAM、磁盘镜像和 xv6 符号配置，并清除 UART、PLIC、virtio 与 MMIO 日志的易失状态；当前测试设备不需要周期推进。
- xv6 测试文件工具：编译临时 RV64 汇编、定位内核和文件系统镜像，并调用 `nm`、`readelf` 等外部工具。

## 公共接口

- `MmioAccessKind`、`MmioAccess`。
- `TestBusState` 的镜像加载、UART 注入/输出、MMIO 日志方法。
- `TestBus::{new, rv64_smoke, xv6_sized, state, load_flat_binary, load_disk_image}` 和 `MemDevice` 实现。
- `TestMachine::{from_bus, with_flat_binary, run_steps, queue_uart_input, run_until_uart_contains, require_uart_contains, require_uart_lacks}`。
- `build_flat_asm`、xv6 测试文件路径与检查、`xv6_machine`、`require_tool`、`run`。

## 依赖关系

模块依赖库中的 `Machine`、`Xv6Accelerator`、`cfg`、`MemDevice` 和 `Exception`，并实现 `MemDevice::reset`，但没有复用 `Bus`、`Dram` 和 `Uart`。virtio 实现知道 xv6 `struct buf` 中 `data` 位于偏移 88，并会直接清除特定字段。`xv6_machine` 只调用一次 `riscv64-elf-nm`，读取所需的内核函数、全局对象和 `tx_busy` 地址，再启用 CPU 加速。符号地址不再硬编码，但结构布局仍与特定 xv6 版本绑定。

## 已知问题与改进建议

- 一个约 900 行文件混合了设备、机器、构造代码和命令行工具，职责过多。
- `RefCell` 的运行时借用检查只适合单线程。大多数字段虽然私有，脚本仍会通过直接包含源码来复用它们。
- PLIC 只支持 UART；virtio 尚未完整实现特性协商、合法状态转换、队列校验和中断请求到 PLIC 的连接。
- virtio 的 `queue_num`、扇区和描述符计算仍缺少完整校验；恶意测试文件可能触发除零、整数溢出或不准确的错误类型。当前测试只使用可信的 xv6 测试文件。
- 测试设备目前都在 MMIO 访问时同步完成工作，没有覆盖 `tick` 的异步设备测试；以后增加此类设备时仍需验证推进顺序和中断到期边界。
- `run_until_uart_contains` 每执行一步都会复制并解码全部 UART 输出，且只在阶段结束后检查失败标记。长测试可能因此反复处理相同文本，并延迟报告 xv6 的 `panic` 信息。
- 测试 UART 与正式 UART 行为不一致，可能掩盖命令行平台的问题。
- 建议将可复用设备移到正式 `devices/` 和 `platform/virt` 模块，只在测试目录保留辅助代码；同时固定 xv6 版本，并增加设备单元测试和非法描述符测试。
