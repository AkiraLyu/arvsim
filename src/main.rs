mod cpu;
mod bus;
mod dram;
mod csr;

fn main() {
    let mut cpu = cpu::Cpu::new();
    cpu.init_dram();
}