# `tests/support/mod.rs`：测试机器与 xv6 平台

## 功能与实现思路

该模块把 CPU 集成测试需要的 RAM、UART、PLIC、virtio-blk、MMIO 日志、fixture 路径和外部命令包装成一套测试平台。`TestBus` 以 `Rc<RefCell<TestBusState>>` 共享内部状态，并作为完整地址空间注入正式 `Machine`；测试仍可注入 UART 输入和观察输出。`TestMachine` 虽持有 `Machine`，运行 helper 当前直接调用公开的 `cpu.step()`，没有经过 `Machine::step()`。

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

直接依赖正式 `Machine`、`Xv6Accelerator`、`cfg`、`MemDevice` 和 `Exception`，但没有复用正式 `Bus/Dram/Uart`。virtio 逻辑知道 xv6 `struct buf` 的 data 偏移 88，并清除特定字段；`xv6_machine` 通过一次 `riscv64-elf-nm` 调用解析所有必需的 kernel 函数、全局对象和 `tx_busy` 地址，再显式启用 CPU 快速路径。符号地址已不再硬编码，但结构布局仍与 xv6 版本耦合。

## 不完善之处和优化方向

- 一个约 900 行文件混合设备、机器、构建器和 shell 工具，职责过多。
- `RefCell` 的运行时借用检查只适合单线程；状态字段大多私有但脚本通过源码包含方式复用。
- PLIC 只支持 UART；virtio 未完整实现 feature negotiation、合法状态机、queue 校验和 IRQ 到 PLIC 的连接。
- 设备访问宽度未集中校验；PLIC 位掩码、RAM 移位和 virtio 的 guest `queue_num/sector/descriptor` 算术在畸形输入下可能 panic、除零或返回错误类型不匹配。当前测试只喂可信 xv6 fixture。
- `Machine::reset()` 不清测试总线状态，运行 helper 又绕过 `Machine::step()`；接入 clocked device 后容易出现测试与正式入口推进顺序不同。
- `run_until_uart_contains` 每执行一步都会重新复制并解码累计 UART 输出，失败标记只在阶段结束后检查；长测可能产生不必要的 O(steps × output) 开销并延迟报告 guest panic 文本。
- 测试 UART 与正式 UART 行为不一致，掩盖 CLI 平台缺口。
- 应拆为正式 `devices/`、`platform/virt` 和纯测试 helper；让 xv6 特例显式版本化，并增加设备级单元测试与非法描述符测试。
