use crate::bus::Bus;
pub struct Cpu {
    regs: [u64; 32],      // 32 个通用寄存器 x0-x31
    pc: u64,              // 程序计数器 (Program Counter)
    // csr: Csr,             // CSR 寄存器组
    pub bus: Bus,             // 连接到总线
    // mode: PrivilegeMode,  // 当前特权级 (User, Supervisor, Machine)
}

impl Cpu {
    pub fn new(dram_size: usize, start_addr: u64) -> Self {
        Cpu {
            regs: [0; 32],
            pc: start_addr,
            bus: Bus::new(dram_size),
        }
    }
}

pub fn load_program_from_file(bus: &mut Bus, program: &[u8], start_addr: u64) {
    for (i, &byte) in program.iter().enumerate() {
        bus.write_u8(start_addr + i as u64, byte);
    }
}