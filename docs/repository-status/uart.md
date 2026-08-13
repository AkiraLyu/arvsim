# `src/uart.rs`：16550 兼容 UART

## 功能与实现

实现当前平台需要的 16550 字节寄存器语义：RBR/THR、IER/IIR/FCR、LCR、MCR、LSR、MSR、SCR 和 DLAB 下的除数锁存器。接收队列非空时设置 LSR.DR；发送完成后设置 LSR.THRE/TEMT。RX available 与 THRE 中断按 IER 和 IIR 优先级驱动共享 `InterruptLine`，读取 IIR 或写 THR 会撤销当前 THRE 请求。

发送完成延迟以平台周期配置，避免在 guest 紧邻 THR 写入之后、驱动提交自身忙状态之前过早触发中断。模型不假定 CPU 步长，`VirtPlatformConfig::uart_transmit_delay_cycles` 明确传入该参数。

## 实现状态

部分实现。初始化、原始字节收发、FIFO 清理、DLAB、多路寄存器、RX/THRE 中断和复位足以支持 xv6；构造器要求 MMIO 窗口至少容纳 8 个字节寄存器，IER 只保留已实现的 RX/THRE 使能位。波特率除数、字符格式和 modem 控制会保存但不会改变实际宿主串行时序，FIFO 只建模启用与清空而不限制深度，也没有线路错误或 modem 状态变化。

宿主 I/O 与寄存器状态已解耦：`StdoutUartBackend` 按原始字节写入并刷新标准输出；`BufferedUartBackend` 提供可克隆的输入队列和输出视图，正式平台测试与交互程序直接复用它，不再实现测试 UART。

## 公共接口

- `Uart::{new, with_backend, with_backend_and_timing, base, window_size, interrupt_line}` 与 `MemDevice` 实现。
- `UartBackend`。
- `StdoutUartBackend`、`BufferedUartBackend`。
- `UartError`。

## 依赖关系

UART 只依赖 `MemDevice`、`InterruptLine` 和注入的 `UartBackend`，不知道 PLIC 源编号或 xv6 符号。`Platform::attach_uart` 使用标准输出后端；`VirtPlatform` 从配置中取得 IRQ 线路、窗口和发送时延。

## 已知限制

- 未实现真实波特率、完整 16 字节 FIFO、超时中断、线路错误和 modem 信号。
- 标准输出后端同步写入并刷新，可能成为大量控制台输出的宿主性能瓶颈。
- `BufferedUartBackend` 基于 `Rc<RefCell<_>>`，只适用于当前单线程运行模型；输出会持续追加到 `Vec`，长期调用方需要主动清空。当前交互 runner 为保持增量索引没有清空，累计输出会线性占用宿主内存。
