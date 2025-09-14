use crate::dram::Dram;
pub struct Bus {
   pub dram: Dram,
}

impl Bus {
    pub fn new() -> Self {
        Bus {
            dram: Dram::new(),
        }
    }

    pub fn read_u8(&self, addr: u64) -> Result<u8, ()> {
        if addr > crate::dram::DRAM_SIZE as u64 {
            return self.dram.read_u8(addr).map_err(|_| ());
        }
        Err(())
    }

    pub fn write_u8(&mut self, addr: u64, value: u8) -> Result<(), ()> {
        if addr >= crate::dram::DRAM_SIZE as u64 {
            return self.dram.write_u8(addr, value).map_err(|_| ());
        }
        Err(())
    }

    pub fn read_u32(&self, addr: u64) -> Result<u32, ()> {
        if addr >= crate::dram::DRAM_SIZE as u64 {
            return self.dram.read_u32(addr).map_err(|_| ());
        }
        Err(())
    }

    pub fn write_u32(&mut self, addr: u64, value: u32) -> Result<(), ()> {
        if addr >= crate::dram::DRAM_SIZE as u64 {
            return self.dram.write_u32(addr, value).map_err(|_| ());
        }
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bus_read_write() {
        let mut bus = Bus::new();
        let _ = bus.write_u8(0x80000000, 42);
        let value = bus.read_u8(0x80000000);
        assert_eq!(value, Ok(42));
    }

    #[test]
    fn test_bus_read_write_out_of_bounds() {
        let bus = Bus::new();
        let _ = bus.read_u8(0x80000000 + crate::dram::DRAM_SIZE as u64);
    }
}