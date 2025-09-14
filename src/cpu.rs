use crate::bus::Bus;
pub const CPU_START_ADDR: u64 = 0x80000000;
pub struct Cpu {
    regs: [u64; 32],      // 32 个通用寄存器 x0-x31
    pc: u64,              // 程序计数器 (Program Counter)
    // csr: Csr,             // CSR 寄存器组
    pub bus: Bus,             // 连接到总线
    // mode: PrivilegeMode,  // 当前特权级 (User, Supervisor, Machine)
}

impl Cpu {
    pub fn new() -> Self {
        Cpu {
            regs: [0; 32],
            pc: CPU_START_ADDR,
            bus: Bus::new(),
        }
    }

    pub fn init_dram(&mut self) {
        let result = self.bus.dram.load_binary_file("test.bin", CPU_START_ADDR);
        if let Err(_) = result {
            panic!("Failed to load binary file into DRAM");
        }
    }
    fn fetch(&self) -> Result<u32, ()> {
        self.bus.read_u32(self.pc)
    }
}
