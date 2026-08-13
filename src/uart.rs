//! 16550 兼容 UART。
//!
//! 寄存器模型实现接收、发送、除数锁存、FIFO 清理和 RX/THRE 中断。宿主 I/O 通过
//! [`UartBackend`] 注入，因此命令行可以直写标准输出，测试和嵌入方也可以使用内存缓冲，
//! 无需另写一套 MMIO 设备。

use crate::bus::MemDevice;
use crate::interrupt::InterruptLine;
use crate::trap::Exception;
use std::cell::{Ref, RefCell};
use std::collections::VecDeque;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io::{self, Write};
use std::rc::Rc;

const REG_DATA: u64 = 0;
const REG_INTERRUPT_ENABLE: u64 = 1;
const REG_INTERRUPT_IDENTIFICATION_FIFO_CONTROL: u64 = 2;
const REG_LINE_CONTROL: u64 = 3;
const REG_MODEM_CONTROL: u64 = 4;
const REG_LINE_STATUS: u64 = 5;
const REG_MODEM_STATUS: u64 = 6;
const REG_SCRATCH: u64 = 7;
const REGISTER_WINDOW_SIZE: u64 = REG_SCRATCH + 1;

const LCR_DLAB: u8 = 1 << 7;
const IER_RX_AVAILABLE: u8 = 1 << 0;
const IER_THR_EMPTY: u8 = 1 << 1;
const IIR_NO_INTERRUPT: u8 = 1;
const IIR_THR_EMPTY: u8 = 0x02;
const IIR_RX_AVAILABLE: u8 = 0x04;
const IIR_FIFO_ENABLED: u8 = 0xc0;
const FCR_FIFO_ENABLE: u8 = 1;
const FCR_CLEAR_RX: u8 = 1 << 1;
const FCR_CLEAR_TX: u8 = 1 << 2;
const LSR_DATA_READY: u8 = 1;
const LSR_THR_EMPTY: u8 = 1 << 5;
const LSR_TRANSMITTER_EMPTY: u8 = 1 << 6;

/// UART 宿主侧的字节输入输出接口。
pub trait UartBackend {
    fn has_input(&self) -> bool;
    fn read_byte(&mut self) -> Option<u8>;
    fn write_byte(&mut self, byte: u8) -> io::Result<()>;

    fn clear_input(&mut self) {}
    fn reset(&mut self) {}
}

/// 把 guest 输出按原始字节写入宿主标准输出的后端。
#[derive(Debug, Default)]
pub struct StdoutUartBackend;

impl UartBackend for StdoutUartBackend {
    fn has_input(&self) -> bool {
        false
    }

    fn read_byte(&mut self) -> Option<u8> {
        None
    }

    fn write_byte(&mut self, byte: u8) -> io::Result<()> {
        let mut stdout = io::stdout().lock();
        stdout.write_all(&[byte])?;
        stdout.flush()
    }
}

#[derive(Debug, Default)]
struct BufferedUartState {
    input: VecDeque<u8>,
    output: Vec<u8>,
}

/// 可克隆的内存 UART 后端，供嵌入方注入输入并观察原始输出。
#[derive(Debug, Clone, Default)]
pub struct BufferedUartBackend(Rc<RefCell<BufferedUartState>>);

impl BufferedUartBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn queue_input(&self, bytes: &[u8]) {
        self.0.borrow_mut().input.extend(bytes.iter().copied());
    }

    pub fn output(&self) -> Ref<'_, [u8]> {
        Ref::map(self.0.borrow(), |state| state.output.as_slice())
    }

    pub fn output_string(&self) -> String {
        String::from_utf8_lossy(&self.0.borrow().output).into_owned()
    }

    pub fn clear_output(&self) {
        self.0.borrow_mut().output.clear();
    }
}

impl UartBackend for BufferedUartBackend {
    fn has_input(&self) -> bool {
        !self.0.borrow().input.is_empty()
    }

    fn read_byte(&mut self) -> Option<u8> {
        self.0.borrow_mut().input.pop_front()
    }

    fn write_byte(&mut self, byte: u8) -> io::Result<()> {
        self.0.borrow_mut().output.push(byte);
        Ok(())
    }

    fn clear_input(&mut self) {
        self.0.borrow_mut().input.clear();
    }

    fn reset(&mut self) {
        let mut state = self.0.borrow_mut();
        state.input.clear();
        state.output.clear();
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum UartError {
    WindowTooSmall,
    AddressOverflow,
    ZeroTransmitDelay,
}

impl Display for UartError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WindowTooSmall => write!(
                f,
                "UART MMIO window must contain all {REGISTER_WINDOW_SIZE} registers"
            ),
            Self::AddressOverflow => f.write_str("UART MMIO range overflows u64"),
            Self::ZeroTransmitDelay => {
                f.write_str("UART transmit completion delay must be non-zero")
            }
        }
    }
}

impl Error for UartError {}

/// 以字节宽度访问的 16550 兼容 UART。
pub struct Uart {
    base: u64,
    window_size: u64,
    backend: Box<dyn UartBackend>,
    interrupt_line: InterruptLine,
    interrupt_enable: u8,
    line_control: u8,
    modem_control: u8,
    scratch: u8,
    divisor_low: u8,
    divisor_high: u8,
    fifo_enabled: bool,
    transmit_delay_cycles: u64,
    transmit_cycles_remaining: u64,
    transmitter_empty: bool,
    transmitter_interrupt_pending: bool,
}

impl Uart {
    /// 创建使用标准输出、默认 UART 窗口且尚未连接中断控制器的 UART。
    pub fn new(base: u64) -> Self {
        Self::with_backend(
            base,
            crate::cfg::UART_SIZE,
            Box::<StdoutUartBackend>::default(),
            InterruptLine::new(),
        )
        .expect("the default UART window is valid")
    }

    pub fn with_backend(
        base: u64,
        window_size: u64,
        backend: Box<dyn UartBackend>,
        interrupt_line: InterruptLine,
    ) -> Result<Self, UartError> {
        Self::with_backend_and_timing(
            base,
            window_size,
            backend,
            interrupt_line,
            crate::cfg::UART_TRANSMIT_DELAY_CYCLES,
        )
    }

    /// 创建使用显式发送时延的 UART；时延单位与 [`MemDevice::tick`] 的平台周期一致。
    pub fn with_backend_and_timing(
        base: u64,
        window_size: u64,
        backend: Box<dyn UartBackend>,
        interrupt_line: InterruptLine,
        transmit_delay_cycles: u64,
    ) -> Result<Self, UartError> {
        if window_size < REGISTER_WINDOW_SIZE {
            return Err(UartError::WindowTooSmall);
        }
        if transmit_delay_cycles == 0 {
            return Err(UartError::ZeroTransmitDelay);
        }
        base.checked_add(window_size)
            .ok_or(UartError::AddressOverflow)?;
        Ok(Self {
            base,
            window_size,
            backend,
            interrupt_line,
            interrupt_enable: 0,
            line_control: 0,
            modem_control: 0,
            scratch: 0,
            divisor_low: 0,
            divisor_high: 0,
            fifo_enabled: false,
            transmit_delay_cycles,
            transmit_cycles_remaining: 0,
            transmitter_empty: true,
            transmitter_interrupt_pending: false,
        })
    }

    pub const fn base(&self) -> u64 {
        self.base
    }

    pub const fn window_size(&self) -> u64 {
        self.window_size
    }

    pub fn interrupt_line(&self) -> InterruptLine {
        self.interrupt_line.clone()
    }

    fn offset(&self, addr: u64) -> Option<u64> {
        let offset = addr.checked_sub(self.base)?;
        (offset < self.window_size).then_some(offset)
    }

    fn dlab(&self) -> bool {
        self.line_control & LCR_DLAB != 0
    }

    fn interrupt_identification(&self) -> u8 {
        let fifo = if self.fifo_enabled {
            IIR_FIFO_ENABLED
        } else {
            0
        };
        if self.interrupt_enable & IER_RX_AVAILABLE != 0 && self.backend.has_input() {
            fifo | IIR_RX_AVAILABLE
        } else if self.interrupt_enable & IER_THR_EMPTY != 0 && self.transmitter_interrupt_pending {
            fifo | IIR_THR_EMPTY
        } else {
            fifo | IIR_NO_INTERRUPT
        }
    }

    fn refresh_interrupt_line(&self) {
        self.interrupt_line
            .set(self.interrupt_identification() & IIR_NO_INTERRUPT == 0);
    }

    fn line_status(&self) -> u8 {
        let mut status = 0;
        if self.backend.has_input() {
            status |= LSR_DATA_READY;
        }
        if self.transmitter_empty {
            status |= LSR_THR_EMPTY | LSR_TRANSMITTER_EMPTY;
        }
        status
    }

    fn reset_registers(&mut self) {
        self.interrupt_enable = 0;
        self.line_control = 0;
        self.modem_control = 0;
        self.scratch = 0;
        self.divisor_low = 0;
        self.divisor_high = 0;
        self.fifo_enabled = false;
        self.transmit_cycles_remaining = 0;
        self.transmitter_empty = true;
        self.transmitter_interrupt_pending = false;
        self.interrupt_line.deassert();
    }
}

impl MemDevice for Uart {
    fn read(&mut self, addr: u64, size: usize) -> Result<u64, Exception> {
        if size != 1 {
            return Err(Exception::LoadAccessFault(addr));
        }
        let offset = self.offset(addr).ok_or(Exception::LoadAccessFault(addr))?;
        let value = match offset {
            REG_DATA if self.dlab() => self.divisor_low,
            REG_DATA => self.backend.read_byte().unwrap_or(0),
            REG_INTERRUPT_ENABLE if self.dlab() => self.divisor_high,
            REG_INTERRUPT_ENABLE => self.interrupt_enable,
            REG_INTERRUPT_IDENTIFICATION_FIFO_CONTROL => {
                let value = self.interrupt_identification();
                if value & 0x0f == IIR_THR_EMPTY {
                    // 16550 的 THRE 中断在读取 IIR 或写入 THR 后撤销。
                    self.transmitter_interrupt_pending = false;
                }
                value
            }
            REG_LINE_CONTROL => self.line_control,
            REG_MODEM_CONTROL => self.modem_control,
            REG_LINE_STATUS => self.line_status(),
            REG_MODEM_STATUS => 0,
            REG_SCRATCH => self.scratch,
            _ => 0,
        };
        self.refresh_interrupt_line();
        Ok(value.into())
    }

    fn write(&mut self, addr: u64, value: u64, size: usize) -> Result<(), Exception> {
        if size != 1 {
            return Err(Exception::StoreAMOAccessFault(addr));
        }
        let offset = self
            .offset(addr)
            .ok_or(Exception::StoreAMOAccessFault(addr))?;
        let value = value as u8;
        match offset {
            REG_DATA if self.dlab() => self.divisor_low = value,
            REG_DATA => {
                self.backend
                    .write_byte(value)
                    .map_err(|_| Exception::StoreAMOAccessFault(addr))?;
                self.transmitter_empty = false;
                self.transmit_cycles_remaining = self.transmit_delay_cycles;
                self.transmitter_interrupt_pending = false;
            }
            REG_INTERRUPT_ENABLE if self.dlab() => self.divisor_high = value,
            REG_INTERRUPT_ENABLE => {
                let was_tx_enabled = self.interrupt_enable & IER_THR_EMPTY != 0;
                self.interrupt_enable = value & (IER_RX_AVAILABLE | IER_THR_EMPTY);
                if !was_tx_enabled
                    && self.interrupt_enable & IER_THR_EMPTY != 0
                    && self.transmitter_empty
                {
                    self.transmitter_interrupt_pending = true;
                }
            }
            REG_INTERRUPT_IDENTIFICATION_FIFO_CONTROL => {
                self.fifo_enabled = value & FCR_FIFO_ENABLE != 0;
                if value & FCR_CLEAR_RX != 0 {
                    self.backend.clear_input();
                }
                if value & FCR_CLEAR_TX != 0 {
                    self.transmit_cycles_remaining = 0;
                    self.transmitter_empty = true;
                    self.transmitter_interrupt_pending = self.interrupt_enable & IER_THR_EMPTY != 0;
                }
            }
            REG_LINE_CONTROL => self.line_control = value,
            REG_MODEM_CONTROL => self.modem_control = value,
            REG_SCRATCH => self.scratch = value,
            // LSR/MSR 只读；窗口中其余保留寄存器写入无副作用。
            _ => {}
        }
        self.refresh_interrupt_line();
        Ok(())
    }

    fn reset(&mut self) {
        self.backend.reset();
        self.reset_registers();
    }

    fn tick(&mut self, cycles: u64) {
        if self.transmit_cycles_remaining != 0 {
            self.transmit_cycles_remaining = self.transmit_cycles_remaining.saturating_sub(cycles);
        }
        if !self.transmitter_empty && self.transmit_cycles_remaining == 0 {
            self.transmitter_empty = true;
            self.transmitter_interrupt_pending = self.interrupt_enable & IER_THR_EMPTY != 0;
        }
        self.refresh_interrupt_line();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u64 = 0x1000_0000;

    fn buffered_uart() -> (Uart, BufferedUartBackend, InterruptLine) {
        let backend = BufferedUartBackend::new();
        let line = InterruptLine::new();
        let uart = Uart::with_backend(
            BASE,
            crate::cfg::UART_SIZE,
            Box::new(backend.clone()),
            line.clone(),
        )
        .unwrap();
        (uart, backend, line)
    }

    #[test]
    fn divisor_latch_does_not_emit_and_transmit_preserves_bytes() {
        let (mut uart, backend, _) = buffered_uart();
        uart.write(BASE + REG_LINE_CONTROL, LCR_DLAB.into(), 1)
            .unwrap();
        uart.write(BASE + REG_DATA, 3, 1).unwrap();
        uart.write(BASE + REG_INTERRUPT_ENABLE, 0, 1).unwrap();
        uart.write(BASE + REG_LINE_CONTROL, 3, 1).unwrap();
        uart.write(BASE + REG_DATA, 0xe4, 1).unwrap();

        assert_eq!(&*backend.output(), &[0xe4]);
        assert_eq!(uart.read(BASE + REG_LINE_STATUS, 1), Ok(0));
        uart.tick(crate::cfg::UART_TRANSMIT_DELAY_CYCLES);
        assert_eq!(
            uart.read(BASE + REG_LINE_STATUS, 1).unwrap()
                & u64::from(LSR_THR_EMPTY | LSR_TRANSMITTER_EMPTY),
            u64::from(LSR_THR_EMPTY | LSR_TRANSMITTER_EMPTY)
        );
    }

    #[test]
    fn receive_and_transmit_interrupts_track_register_state() {
        let (mut uart, backend, line) = buffered_uart();
        uart.write(
            BASE + REG_INTERRUPT_ENABLE,
            u64::from(IER_RX_AVAILABLE | IER_THR_EMPTY),
            1,
        )
        .unwrap();
        assert!(line.is_asserted());
        assert_eq!(
            uart.read(BASE + REG_INTERRUPT_IDENTIFICATION_FIFO_CONTROL, 1),
            Ok(IIR_THR_EMPTY.into())
        );
        assert!(!line.is_asserted());

        uart.write(BASE + REG_DATA, u64::from(b'A'), 1).unwrap();
        uart.tick(crate::cfg::UART_TRANSMIT_DELAY_CYCLES - 1);
        assert!(!line.is_asserted());
        uart.tick(1);
        assert!(line.is_asserted());
        assert_eq!(
            uart.read(BASE + REG_INTERRUPT_IDENTIFICATION_FIFO_CONTROL, 1),
            Ok(IIR_THR_EMPTY.into())
        );
        assert!(!line.is_asserted());

        backend.queue_input(b"x");
        uart.tick(1);
        assert!(line.is_asserted());
        assert_eq!(uart.read(BASE + REG_LINE_STATUS, 1).unwrap() & 1, 1);
        assert_eq!(uart.read(BASE + REG_DATA, 1), Ok(u64::from(b'x')));
        assert!(!line.is_asserted());
    }

    #[test]
    fn reset_clears_registers_backend_and_interrupt() {
        let (mut uart, backend, line) = buffered_uart();
        uart.write(BASE + REG_INTERRUPT_ENABLE, IER_THR_EMPTY.into(), 1)
            .unwrap();
        uart.write(BASE + REG_DATA, b'A'.into(), 1).unwrap();
        backend.queue_input(b"x");
        uart.tick(crate::cfg::UART_TRANSMIT_DELAY_CYCLES);
        assert!(line.is_asserted());

        uart.reset();
        assert!(backend.output().is_empty());
        assert_eq!(uart.read(BASE + REG_INTERRUPT_ENABLE, 1), Ok(0));
        assert_eq!(uart.read(BASE + REG_LINE_STATUS, 1).unwrap() & 1, 0);
        assert!(!line.is_asserted());
    }

    #[test]
    fn invalid_widths_and_addresses_are_access_faults() {
        let (mut uart, _, _) = buffered_uart();
        assert_eq!(uart.read(BASE, 4), Err(Exception::LoadAccessFault(BASE)));
        assert_eq!(
            uart.write(BASE - 1, 0, 1),
            Err(Exception::StoreAMOAccessFault(BASE - 1))
        );
        assert_eq!(
            uart.read(BASE + crate::cfg::UART_SIZE, 1),
            Err(Exception::LoadAccessFault(BASE + crate::cfg::UART_SIZE))
        );
        assert!(matches!(
            Uart::with_backend(
                BASE,
                REGISTER_WINDOW_SIZE - 1,
                Box::new(BufferedUartBackend::new()),
                InterruptLine::new()
            ),
            Err(UartError::WindowTooSmall)
        ));
    }
}
