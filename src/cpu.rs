use crate::bus::MemDevice;
use crate::csr;
use crate::trap::Exception;
use crate::instruction;

pub struct Cpu {
    pub registers: [u64; 32],
    pub pc: u64,
    pub bus: Box<dyn MemDevice>,
    pub csr: csr::Csr,
}

pub const DEBUG: bool = true;

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

    pub fn step(&mut self) -> Result<(), Exception> {
        let instruction = self.fetch()?;
        let new_pc = self.execute(instruction)?;
        self.pc = new_pc;
        Ok(())
    }

    pub fn run(&mut self) {
        loop {
            if DEBUG {
                self.dump_pc();
                self.dump_registers();
                self.csr.dump_csr();
            }
            let instruction = match self.fetch() {
                Ok(instruction) => instruction,
                Err(_) => {
                    eprintln!("Failed to fetch instruction at pc: {:#x}", self.pc);
                    break;
                }
            };
            match self.execute(instruction) {
                Ok(new_pc) => self.pc = new_pc,
                Err(e) => {
                    println!("Failed to execute because of {:?}", e);
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
                    _ => {
                        eprintln!("Exception occurred: {:?}", e);
                    }
                }
                Err(e)
            }
        }
    }
    // execute the instruction and return the new pc address
    fn execute(&mut self, instruction: u64) -> Result<u64, Exception> {
        let old_pc = self.pc;
        let inst = instruction as u32;
        let decoded = instruction::decode(inst);
        match instruction::execute(self, decoded) {
            Ok(_) => {
                if self.pc == old_pc {
                    Ok(self.pc.wrapping_add(4))
                } else {
                    Ok(self.pc)
                }
            }
            Err(e) => {
                match &e {
                    Exception::IllegalInstruction(addr) => {
                        eprintln!("Illegal instruction at address: 0x{:016x}", addr);
                    }
                    Exception::LoadAccessFault(addr)
                    | Exception::StoreAMOAccessFault(addr)
                    | Exception::InstructionAccessFault(addr) => {
                        eprintln!("Memory access error at address: 0x{:016x}", addr);
                    }
                    Exception::InstructionAddrMisaligned(addr)
                    | Exception::LoadAccessMisaligned(addr)
                    | Exception::StoreAMOAddrMisaligned(addr) => {
                        eprintln!("Misaligned memory access at address: 0x{:016x}", addr);
                    }
                    _ => {
                        eprintln!("Exception occurred: {:?}", e);
                    }
                }
                self.pc += 4;
                Err(e)
            }
        }
    }

    pub fn dump_pc(&mut self) {
        println!("pc: {:#x}", self.pc);
    }

    pub fn dump_registers(&mut self) {
        for (i, &value) in self.registers.iter().enumerate() {
            println!("x{:02}: {:#018x}", i, value);
        }
    }
}
