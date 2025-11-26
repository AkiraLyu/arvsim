use std::env::args;

mod bus;
mod cfg;
mod cpu;
mod csr;
mod dram;
mod instruction;
mod trap;
mod uart;

fn main() {
    let mut args = args();

    // 获取程序路径
    let _program = args.next();

    // 获取二进制文件
    let bin_path = match args.next() {
        Some(path) => path,
        None => {
            eprintln!("Usage: cargo run <binary_file>");
            return;
        }
    };

    // 初始化 DRAM
    let mut dram = dram::Dram::new();

    // 加载镜像文件
    if let Err(e) = dram.load(&bin_path) {
        eprintln!("Failed to load binary '{}': {}", bin_path, e);
        return;
    }

    // 初始化 UART
    let uart = uart::Uart::new(cfg::UART_BASE);

    // 构建 Bus
    let mut bus = bus::Bus::new();
    bus.attach_ram(dram.base, Box::new(dram));
    bus.attach_uart(0x1000_0000, Box::new(uart));

    // CPU
    let mut cpu = cpu::Cpu::new(Box::new(bus));

    cpu.reset();

    cpu.run();
}
