# `src/uart.rs`：简化 UART

## 功能与实现思路

模拟 16550 UART 的最小输出路径：写 THR（偏移 `0x00`）将低字节打印并刷新主机 stdout；读 RBR 返回 0；读 LSR（偏移 `0x05`）固定返回发送空闲位 `0x20`。

## 当前状态

部分实现，足以输出字符但无法输入、配置波特率或产生中断。单元测试覆盖固定 RBR/LSR 值和 THR 写成功。

## 对外接口

- `Uart { pub base: u64 }`
- `Uart::new(base)`
- `impl MemDevice for Uart`

## 耦合方式

实现 `bus::MemDevice`，使用 `trap::Exception`；CLI 将其挂载到 UART MMIO 区域。xv6 测试不复用该实现，而在 `TestBus` 内维护输入队列、输出缓冲和 PLIC pending 位。

## 不完善之处和优化方向

- 忽略访问宽度，未知寄存器错误地返回 `IllegalInstruction`。
- 无 RX、FIFO、IER/IIR、寄存器状态、IRQ 或可替换的 I/O 后端。
- `addr - base` 在直接传入低地址时可能下溢；当前依赖 Bus 保证调用范围。
- 同步打印/flush 使设备模型带主机副作用且难以测试输出。
- 建议把 UART 寄存器状态与主机终端后端分离，复用测试 UART 行为，并通过 PLIC/中断控制器提交 IRQ。
