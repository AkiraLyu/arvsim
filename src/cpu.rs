use crate::bus::MemDevice;
use crate::csr;
use crate::exception::Exception;
use crate::instruction;

pub struct Cpu {
    pub registers: [u64; 32],
    pub pc: u64,
    pub bus: Box<dyn MemDevice>,
    pub csr: csr::Csr,
}

impl Cpu {
    pub fn new(bus: Box<dyn MemDevice>) -> Self {
        let mut cpu = Cpu {
            registers: [0; 32],
            pc: crate::cfg::CPU_START_ADDR,
            bus,
            csr: csr::Csr::new(),
        };
        cpu.registers[2] = crate::cfg::DRAM_END;
        cpu
    }

    pub fn reset(&mut self) {
        self.registers = [0; 32];
        self.registers[2] = crate::cfg::DRAM_END;
        self.pc = crate::cfg::CPU_START_ADDR;
    }

    pub fn run(&mut self) {
        loop {
            let instruction = match self.fetch() {
                Ok(instruction) => {
                    println!("pc: {:#x}, instruction: {:#x}", self.pc, instruction);
                    instruction
                }
                Err(_) => {
                    eprintln!("Failed to fetch instruction at pc: {:#x}", self.pc);
                    break;
                }
            };
            match self.execute(instruction) {
                Ok(new_pc) => self.pc = new_pc,
                Err(e) => {
                    println!("Failed to execute at {:?}",e);
                }
            };
        }
    }

    // read a 32 bits instruction from memory and increment the pc
    fn fetch(&mut self) -> Result<u64, Exception> {
        match self.bus.read(self.pc, 4) {
            Ok(instruction) => Ok(instruction),
            Err(e) => {
                match &e {
                    Exception::IllegalInstruction(addr)
                    | Exception::LoadAccessFault(addr)
                    | Exception::StoreAMOAccessFault(addr) => {
                        eprintln!("Memory access error at address: 0x{:016x}", addr);
                    }
                    _ => { eprintln!("Exception occurred: {:?}", e);}
                }
                Err(e)
            }
        }
    }
    // execute the instruction and return the new pc address
    fn execute(&mut self, instruction: u64) -> Result<u64, Exception> {
        let inst = instruction as u32;
        let decoded = instruction::decode(inst);
        // instruction::execute(self, decoded)?;
        // Ok(self.pc + 4)
        Ok(self.pc)
        
    }
}

