use std::env::args;

mod cpu;
mod bus;
mod dram;
mod csr;
mod instruction;

fn main() {
    let mut args = args();
    if args.len() != 2 {
        eprintln!("Usage: cargo run <binary_file>");
        return;
    }
    let mut cpu = cpu::Cpu::new();
    cpu.reset();
    cpu.init_dram(args.nth(1).unwrap().as_str());
    cpu.run();
}