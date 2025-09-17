use crate::bus::MemDevice;
use crate::exception::Exception;

pub struct Cpu {
    pub registers: [u64; 32],
    pub pc: u64,
    pub bus: Box<dyn MemDevice>,
}

impl Cpu {
    pub fn new(bus: Box<dyn MemDevice>) -> Self {
        Cpu {
            registers: [0; 32],
            pc: crate::cfg::CPU_START_ADDR,
            bus,
        }
    }

    pub fn reset(&mut self) {
        self.registers = [0; 32];
        self.pc = crate::cfg::CPU_START_ADDR;
    }

    pub fn run(&mut self) {
        loop {
            match self.fetch() {
                Ok(instruction) => {
                    println!("pc: {:#x}, instruction: {:#x}", self.pc, instruction);
                    print!("test uart output: ");
                    let _result = self.bus.write(crate::cfg::UART_BASE, 'A' as u32, 1);
                    println!("\n");
                    self.pc = self.execute().unwrap_or(self.pc);
                }
                Err(_) => {
                    eprintln!("Failed to fetch instruction at pc: {:#x}", self.pc);
                    break;
                }
            }
        }
    }

    // read a 32 bits instruction from memory and increment the pc
    fn fetch(&mut self) -> Result<u32, Exception> {
        match self.bus.read(self.pc, 4) {
            Ok(instruction) => Ok(instruction as u32),
            Err(e) => {
                match &e {
                    Exception::IllegalInstruction(addr)
                    | Exception::LoadAccessFault(addr)
                    | Exception::StoreAccessFault(addr) => {
                        eprintln!("Memory access error at address: 0x{:016x}", addr);
                    }
                }
                Err(e)
            }
        }
    }
    // execute the instruction and return the new pc address
    fn execute(&mut self) -> Result<u64, ()> {
        Ok(self.pc + 4)
    }
}

