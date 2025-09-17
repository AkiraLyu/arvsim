use std::env::args;

mod bus;
mod cfg;
mod cpu;
mod dram;
mod uart;

fn main() {
    let args = args();
    if args.len() < 2 {
        eprintln!("Usage: cargo run <binary_file>");
    }

    let mut dram = dram::Dram::new();
    let uart = uart::Uart::new(cfg::UART_BASE);
    dram.load(&args.into_iter().nth(1).unwrap()).unwrap();
    let mut bus = bus::Bus::new();
    bus.attach_ram(dram.base, Box::new(dram));
    bus.attach_uart(0x1000_0000, Box::new(uart));

    let mut cpu = cpu::Cpu::new(Box::new(bus));
    cpu.reset();
    cpu.run();
}