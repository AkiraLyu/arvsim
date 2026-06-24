# `src/uart.rs`：正式 UART 模型

## 设计

- 提供最小 16550a 风格 UART，使命令行运行的小程序可以输出字符。
- 该模块是正式入口使用的 UART，不是 xv6 测试中的完整交互 UART。

## 实现

- 读 `base + 0x00` 返回 0，表示接收缓冲区为空。
- 读 `base + 0x05` 返回 `0x20`，表示发送保持寄存器为空。
- 写 `base + 0x00` 会把低 8 位作为字符打印到主机 stdout 并 flush。
- 其他寄存器访问返回 `IllegalInstruction`。

## 接口

- `struct Uart { base: u64 }`
- `Uart::new(base: u64)`
- `impl MemDevice for Uart`

## 限制

- 不支持输入队列、中断、FIFO、波特率或完整 16550a 寄存器。
- xv6 运行依赖的 UART 输入和中断由 `tests/support/mod.rs` 中的测试总线实现。
