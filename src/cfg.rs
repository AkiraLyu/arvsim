pub const DRAM_SIZE: usize = 1024 * 1024 * 128;
pub const DRAM_BASE: u64 = 0x80000000;
pub const DRAM_END: u64 = DRAM_BASE + DRAM_SIZE as u64;
pub const CPU_START_ADDR: u64 = DRAM_BASE;
pub const UART_BASE: u64 = 0x10000000;
