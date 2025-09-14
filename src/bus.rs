use crate::dram::Dram;
pub struct Bus {
   pub dram: Dram,
}

impl Bus {
    pub fn new(dram_size: usize) -> Self {
        Bus {
            dram: Dram {
                memory: vec![0; dram_size],
            },
        }
    }

    pub fn read_u8(&self, addr: u64) -> u8 {
        self.dram.memory[addr as usize]
    }

    pub fn write_u8(&mut self, addr: u64, value: u8) {
        self.dram.memory[addr as usize] = value;
    }

    pub fn read_u32(&self, addr: u64) -> u32 {
        let b0 = self.read_u8(addr) as u32;
        let b1 = self.read_u8(addr + 1) as u32;
        let b2 = self.read_u8(addr + 2) as u32;
        let b3 = self.read_u8(addr + 3) as u32;
        (b3 << 24) | (b2 << 16) | (b1 << 8) | b0
    }

    pub fn write_u32(&mut self, addr: u64, value: u32) {
        self.write_u8(addr, (value & 0xFF) as u8);
        self.write_u8(addr + 1, ((value >> 8) & 0xFF) as u8);
        self.write_u8(addr + 2, ((value >> 16) & 0xFF) as u8);
        self.write_u8(addr + 3, ((value >> 24) & 0xFF) as u8);
    }
}