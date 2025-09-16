use crate::bus::Bus;
pub const CPU_START_ADDR: u64 = 0x80000000;
pub struct Cpu {
    regs: [u64; 32],      // 32 个通用寄存器 x0-x31
    pc: u64,              // 程序计数器 (Program Counter)
    // csr: Csr,             // CSR 寄存器组
    pub bus: Bus,             // 连接到总线
    // mode: PrivilegeMode,  // 当前特权级 (User, Supervisor, Machine)
}

pub enum ExitReason {
    Ecall,
    Fault,
}

impl Cpu {
    pub fn new() -> Self {
        Cpu {
            regs: [0; 32],
            pc: CPU_START_ADDR,
            bus: Bus::new(),
        }
    }
    
    pub fn reset(&mut self) {
        self.regs = [0; 32];
        self.pc = CPU_START_ADDR;
        // self.csr.reset();
        // self.mode = PrivilegeMode::Machine;
    }

    pub fn init_dram(&mut self,filepath: &str) {
        let result = self.bus.dram.load_binary_file(filepath, CPU_START_ADDR);
        if let Err(_) = result {
            panic!("Failed to load binary file into DRAM");
        }
    }
    pub fn run(&mut self) {
        loop {
            match self.fetch() {
                Ok(instruction) => {
                    self.regs[0] = 0;
                    println!("Fetched instruction: 0x{:08x} at PC: 0x{:016x}", instruction, self.pc);
                    self.pc += 4; // 假设每条指令长度为4字节
                    self.decode(instruction);
                }
                Err(_) => {
                    println!("Failed to fetch instruction at PC: 0x{:016x}", self.pc);
                    break;
                }
            }
        }
    }

    fn fetch(&self) -> Result<u32, ()> {
        self.bus.read_u32(self.pc)
    }

    fn decode(&self, instruction: u32) {
        println!("Decoding instruction: 0x{:08x}", instruction);
        crate::instruction::decode(instruction);
    }

    fn execute(&mut self, instruction: crate::instruction::Instruction) {
        todo!();
    }
}
