//! 最小化 UART 输出设备。
//!
//! 该模型只提供当前命令行运行所需的发送路径和固定状态：接收缓冲始终为空，
//! 发送保持就绪，写 THR 会立即输出到宿主标准输出。测试平台使用另一套可注入输入的 UART 模型。

use crate::{
    bus::{MemDevice, valid_access_size},
    trap::Exception,
};

/// 以 `base` 为 MMIO 起点的简化 UART。
pub struct Uart {
    pub base: u64,
}

impl Uart {
    /// 创建 UART；调用方必须将同一基址用于总线映射。
    pub fn new(base: u64) -> Self {
        Uart { base }
    }
}

impl MemDevice for Uart {
    fn read(&mut self, addr: u64, size: usize) -> Result<u64, Exception> {
        if !valid_access_size(size) || size != 1 {
            return Err(Exception::LoadAccessFault(addr));
        }
        let offset = addr
            .checked_sub(self.base)
            .ok_or(Exception::LoadAccessFault(addr))?;
        match offset {
            0x00 => {
                // RBR (Receiver Buffer Register)，简化版始终返回 0
                Ok(0)
            }
            0x05 => {
                // LSR (Line Status Register)，bit5=1 表示 THR 空
                Ok(0x20)
            }
            _ => Err(Exception::IllegalInstruction(addr)),
        }
    }

    fn write(&mut self, addr: u64, value: u64, size: usize) -> Result<(), Exception> {
        if !valid_access_size(size) || size != 1 {
            return Err(Exception::StoreAMOAccessFault(addr));
        }
        let offset = addr
            .checked_sub(self.base)
            .ok_or(Exception::StoreAMOAccessFault(addr))?;
        match offset {
            0x00 => {
                // THR (Transmit Holding Register)，把字节输出
                let ch = (value & 0xff) as u8;
                print!("{}", ch as char);
                use std::io::Write;
                std::io::stdout()
                    .flush()
                    .map_err(|_| Exception::StoreAMOAccessFault(addr))?;
                Ok(())
            }
            _ => Err(Exception::IllegalInstruction(addr)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UART_BASE_ADDR: u64 = 0x10000000;

    #[test]
    fn test_read_lsr_is_always_ready_to_transmit() {
        let mut uart = Uart::new(UART_BASE_ADDR);
        // 读取 Line Status Register (LSR)
        let lsr_addr = UART_BASE_ADDR + 0x05;
        // 0x20 (bit 5) 表示 Transmitter Holding Register is empty
        match uart.read(lsr_addr, 1) {
            Ok(value) => assert_eq!(value, 0x20),
            Err(_) => panic!("Reading LSR should not fail"),
        }
    }

    #[test]
    fn test_read_rbr_is_always_empty() {
        let mut uart = Uart::new(UART_BASE_ADDR);
        // 读取 Receiver Buffer Register (RBR)
        let rbr_addr = UART_BASE_ADDR;
        match uart.read(rbr_addr, 1) {
            Ok(value) => assert_eq!(value, 0),
            Err(_) => panic!("Reading RBR should not fail"),
        }
    }

    #[test]
    fn test_write_to_thr_succeeds() {
        let mut uart = Uart::new(UART_BASE_ADDR);
        // 写入 Transmitter Holding Register (THR)
        let thr_addr = UART_BASE_ADDR;
        // 写入字符 'A' (ASCII 65)
        let result = uart.write(thr_addr, 65, 1);
        assert!(result.is_ok());
    }

    #[test]
    fn invalid_widths_and_addresses_are_rejected() {
        let mut uart = Uart::new(UART_BASE_ADDR);

        assert_eq!(
            uart.read(UART_BASE_ADDR, 0),
            Err(Exception::LoadAccessFault(UART_BASE_ADDR))
        );
        assert_eq!(
            uart.read(UART_BASE_ADDR, 4),
            Err(Exception::LoadAccessFault(UART_BASE_ADDR))
        );
        assert_eq!(
            uart.write(UART_BASE_ADDR, 65, 8),
            Err(Exception::StoreAMOAccessFault(UART_BASE_ADDR))
        );
        assert_eq!(
            uart.write(UART_BASE_ADDR - 1, 65, 1),
            Err(Exception::StoreAMOAccessFault(UART_BASE_ADDR - 1))
        );
    }
}
