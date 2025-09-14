mod cpu;
mod bus;
mod dram;
mod csr;

use crate::dram::DRAM_SIZE;
use crate::dram::DRAM_BASE;

fn main() {

    let mut cpu = cpu::Cpu::new(DRAM_SIZE, DRAM_BASE);
    let mut bus = bus::Bus::new(DRAM_SIZE);
    let mut dram = dram::Dram::new(DRAM_SIZE);
    dram.load_binary_file("test.bin", DRAM_BASE);
}