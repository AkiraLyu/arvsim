# `src/uart.rs`：16550 兼容串口

## 寄存器与中断

模块实现当前平台使用的 16550 字节寄存器：RBR/THR、IER/IIR/FCR、LCR、MCR、LSR、MSR、SCR，以及由 DLAB 切换访问的波特率除数寄存器。

接收队列有数据时设置 LSR.DR；发送完成时设置 LSR.THRE/TEMT。接收数据就绪和发送寄存器空（THRE）中断根据 IER 配置和 IIR 优先级控制 `InterruptLine`。读取到 THRE 中断状态或写入 THR 后，会撤销当前的 THRE 请求。

发送延迟通过 `VirtPlatformConfig::uart_transmit_delay_cycles` 配置，以平台周期为单位。这样可以避免程序写入 THR 后、串口驱动尚未记录“正在发送”状态时，中断就提前到达。

## 已实现的功能

初始化、字节收发、FIFO 清空、DLAB 寄存器切换、RX/THRE 中断和复位已能支持 xv6。MMIO 地址范围至少需要容纳 8 个字节寄存器，IER 只保留已实现的 RX/THRE 中断使能位。

波特率除数、字符格式和调制解调器控制位会被保存，但不会改变主机上的实际传输时序。FIFO 只模拟启用和清空，没有容量上限，也没有线路错误或调制解调器状态变化。

`StdoutUartBackend` 将原始字节写入标准输出并立即刷新。`BufferedUartBackend` 提供可共享的输入队列和输出缓冲，测试与交互示例直接使用它。

## 公开接口与依赖

- `Uart::{new, with_backend, with_backend_and_timing, base, window_size, interrupt_line}`，并实现 `MemDevice`。
- `UartBackend`：输入输出接口。
- `StdoutUartBackend`、`BufferedUartBackend`：两种输入输出实现。
- `UartError`：串口配置错误。

UART 使用 `MemDevice`、`InterruptLine` 和调用方提供的 `UartBackend`，不需要知道 PLIC 中断号或 xv6 符号。`Platform::attach_uart` 默认使用标准输出；`VirtPlatform` 通过配置设置中断线、地址范围和发送延迟。

## 已知限制

真实波特率、完整 16 字节 FIFO、超时中断、线路错误和调制解调器信号尚未实现。大量串口输出时，同步写入并刷新标准输出可能影响模拟器性能。

`BufferedUartBackend` 使用 `Rc<RefCell<_>>`，只适用于单线程。输出会持续追加到缓冲区，长期运行的调用方需要主动清空。xv6 交互示例在显示输出后调用 `clear_output()`，不再持续保留已显示的内容。
