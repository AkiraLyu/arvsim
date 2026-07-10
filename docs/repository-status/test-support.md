# `tests/support/mod.rs`：测试机器与 xv6 平台

## 功能与实现思路

该模块把 CPU 集成测试需要的 RAM、UART、PLIC、virtio-blk、MMIO 日志、fixture 路径和外部命令包装成一套测试平台。`TestBus` 以 `Rc<RefCell<TestBusState>>` 共享内部状态，CPU 持有其 clone，测试仍可注入 UART 输入和观察输出。

## 当前状态

测试专用但功能较完整：

- RAM：1 MiB smoke 配置或 128 MiB xv6 配置，支持 flat binary。
- UART：输出缓冲、输入队列、LSR RX/TX 状态；有输入时置 PLIC UART pending。
- PLIC：实现 UART IRQ 10 的 priority、pending、S-mode enable、threshold、claim/complete。
- virtio-mmio block：实现关键识别/状态/queue 寄存器、大小 8 的单队列、descriptor chain、磁盘读写、used ring 和 interrupt status。
- 机器控制：固定步数运行、运行到 UART 包含目标文本、失败标记检查。
- fixture 工具：编译临时 RV64 汇编、定位 kernel/fs 镜像、调用 `nm/readelf` 和验证工具。

## 对外接口

- `MmioAccessKind`、`MmioAccess`。
- `TestBusState` 的镜像加载、UART 注入/输出、MMIO 日志方法。
- `TestBus::{new, rv64_smoke, xv6_sized, state, load_flat_binary, load_disk_image}` 和 `MemDevice` 实现。
- `TestMachine::{from_bus, with_flat_binary, run_steps, queue_uart_input, run_until_uart_contains, require_uart_contains, require_uart_lacks}`。
- `build_flat_asm`、fixture 路径/验证、`xv6_machine`、`require_tool`、`run`。

## 耦合方式

直接依赖正式 `Cpu`、`cfg`、`MemDevice` 和 `Exception`，但没有复用正式 `Bus/Dram/Uart`。virtio 逻辑知道 xv6 `struct buf` 的 data 偏移 88，并清除特定字段；`xv6_machine` 通过 ELF 符号或硬编码地址处理 `tx_busy`，因此与 xv6 版本强耦合。

## 不完善之处和优化方向

- 一个约 800 行文件混合设备、机器、构建器和 shell 工具，职责过多。
- `RefCell` 的运行时借用检查只适合单线程；状态字段大多私有但脚本通过源码包含方式复用。
- PLIC 只支持 UART；virtio 未完整实现 feature negotiation、合法状态机、queue 校验和 IRQ 到 PLIC 的连接。
- 测试 UART 与正式 UART 行为不一致，掩盖 CLI 平台缺口。
- 应拆为正式 `devices/`、`platform/virt` 和纯测试 helper；让 xv6 特例显式版本化，并增加设备级单元测试与非法描述符测试。
