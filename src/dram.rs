// define the DRAM size as 128 MB
pub const DRAM_SIZE: usize = 1024 * 1024 * 128;
pub const DRAM_BASE: u64 = 0x80000000;

pub struct Dram {
    pub memory: Vec<u8>,
}

impl Dram {
    pub fn new() -> Self {
        Dram {
            memory: vec![0; DRAM_SIZE],
        }
    }

    pub fn read_u8(&self, addr: u64) -> Result<u8, &'static str> {
        if addr < DRAM_BASE {
            return Err("Address below DRAM base");
        }
        let index = (addr - DRAM_BASE) as usize;
        if index >= DRAM_SIZE {
            return Err("DRAM address out of bounds");
        }
        Ok(self.memory[index])
    }

    pub fn write_u8(&mut self, addr: u64, value: u8) -> Result<(), &'static str> {
        if addr < DRAM_BASE {
            return Err("Address below DRAM base");
        }
        let index = (addr - DRAM_BASE) as usize;
        if index>= DRAM_SIZE {
            return Err("DRAM address out of bounds");
        }
        self.memory[index] = value;
        Ok(())
    }

    pub fn read_u32(&self, addr: u64) -> Result<u32, &'static str> {
        if addr < DRAM_BASE {
            return Err("Address below DRAM base");
        }
        let index = (addr - DRAM_BASE) as usize;
        if index + 3 >= DRAM_SIZE {
            return Err("DRAM address out of bounds for u32 read");
        }
        let data = &self.memory[index..index + 4];
        Ok(u32::from_le_bytes(data.try_into().unwrap()))
    }

    pub fn write_u32(&mut self, addr: u64, value: u32) -> Result<(), &'static str> {
        if addr < DRAM_BASE {
            return Err("Address below DRAM base");
        }
        let index = (addr - DRAM_BASE) as usize;
        if index + 4 > DRAM_SIZE {
            return Err("DRAM address out of bounds for u32 write");
        }
        let bytes = value.to_le_bytes();
        self.memory[index..index + 4].copy_from_slice(&bytes);
        Ok(())
    }
    
    pub fn load_binary_file(&mut self, filename: &str, start_addr: u64) -> Result<(), Box<dyn std::error::Error>> {
        use std::fs::File;
        use std::io::Read;
    
        let mut file = File::open(filename)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
    
        if buffer.len() > DRAM_SIZE {
            return Err("Binary file too large to fit in DRAM".into());
        }

        for (i, &byte) in buffer.iter().enumerate() {
            self.write_u8(start_addr + i as u64, byte)?;
        }
    
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dram_read_write() {
        let mut dram = Dram::new();
        dram.write_u8(DRAM_BASE, 42).unwrap();
        let value = dram.read_u8(DRAM_BASE).unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn test_dram_read_write_out_of_bounds() {
        let mut dram = Dram::new();
        let result = dram.read_u8(DRAM_BASE + DRAM_SIZE as u64);
        assert!(result.is_err());
    }

    #[test]
    fn test_dram_load_binary_file() {
        let mut dram = Dram::new();
        let result = dram.load_binary_file("/home/akira/test.bin", DRAM_BASE);
        assert!(result.is_ok());
        assert_eq!(dram.read_u8(DRAM_BASE).unwrap(), 0x7F);
        assert_eq!(dram.read_u8(DRAM_BASE + 1).unwrap(), 0x45);
        assert_eq!(dram.read_u8(DRAM_BASE + 2).unwrap(), 0x4C);
        assert_eq!(dram.read_u8(DRAM_BASE + 3).unwrap(), 0x46);
    }
}