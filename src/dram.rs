use crate::{bus::MemDevice, trap::Exception};

pub struct Dram {
    pub dram: Vec<u8>,
    pub base: u64,
}

impl Dram {
    pub fn new() -> Self {
        Self::with_layout(crate::cfg::DRAM_BASE, crate::cfg::DRAM_SIZE)
    }

    pub fn with_layout(base: u64, size: usize) -> Self {
        Dram {
            dram: vec![0; size],
            base,
        }
    }

    pub fn end(&self) -> Option<u64> {
        self.base.checked_add(self.dram.len() as u64)
    }

    pub fn load_bytes(&mut self, addr: u64, bytes: &[u8]) -> Result<(), std::io::Error> {
        let offset = addr
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| std::io::Error::other("image address is below DRAM base"))?;
        let end = offset
            .checked_add(bytes.len())
            .filter(|end| *end <= self.dram.len())
            .ok_or_else(|| std::io::Error::other("image segment exceeds DRAM size"))?;
        self.dram[offset..end].copy_from_slice(bytes);
        Ok(())
    }

    pub fn zero_range(&mut self, addr: u64, len: usize) -> Result<(), std::io::Error> {
        let offset = addr
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| std::io::Error::other("image address is below DRAM base"))?;
        let end = offset
            .checked_add(len)
            .filter(|end| *end <= self.dram.len())
            .ok_or_else(|| std::io::Error::other("image segment exceeds DRAM size"))?;
        self.dram[offset..end].fill(0);
        Ok(())
    }

    pub fn load(&mut self, filename: &str) -> Result<(), std::io::Error> {
        use std::fs::File;
        use std::io::Read;
        let mut file = File::open(filename)?;
        let mut buffer = Vec::new();

        file.read_to_end(&mut buffer)?;

        self.load_bytes(self.base, &buffer)
    }
}

impl MemDevice for Dram {
    fn read(&mut self, addr: u64, size: usize) -> Result<u64, Exception> {
        if addr < self.base {
            return Err(Exception::LoadAccessFault(addr));
        }
        let offset = (addr - self.base) as usize;
        if offset + size > self.dram.len() {
            return Err(Exception::LoadAccessFault(addr));
        }

        let mut val = 0u64;
        for i in 0..size {
            val |= (self.dram[offset + i] as u64) << (i * 8);
        }
        Ok(val)
    }

    fn write(&mut self, addr: u64, value: u32, size: usize) -> Result<(), Exception> {
        if addr < self.base {
            return Err(Exception::StoreAMOAccessFault(addr));
        }
        let offset = (addr - self.base) as usize;
        if offset + size > self.dram.len() {
            return Err(Exception::StoreAMOAccessFault(addr));
        }

        for i in 0..size {
            self.dram[offset + i] = ((value >> (i * 8)) & 0xff) as u8;
        }
        Ok(())
    }
}

impl Default for Dram {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dram_read_write() {
        let mut dram = Dram::new();

        // Write 4 bytes
        assert!(dram.write(dram.base, 0x12345678, 4).is_ok());
        // Read back the 4 bytes
        let val = dram.read(dram.base, 4).unwrap();
        assert_eq!(val, 0x12345678);

        // Write 2 bytes
        assert!(dram.write(dram.base + 4, 0x9abc, 2).is_ok());
        // Read back the 2 bytes
        let val = dram.read(dram.base + 4, 2).unwrap();
        assert_eq!(val, 0x9abc);

        // Write 1 byte
        assert!(dram.write(dram.base + 6, 0xde, 1).is_ok());
        // Read back the 1 byte
        let val = dram.read(dram.base + 6, 1).unwrap();
        assert_eq!(val, 0xde);

        // Test out of bounds read
        assert!(
            dram.read(dram.base + crate::cfg::DRAM_SIZE as u64, 4)
                .is_err()
        );
        // Test out of bounds write
        assert!(
            dram.write(dram.base + crate::cfg::DRAM_SIZE as u64, 0x1234, 2)
                .is_err()
        );
    }

    #[test]
    fn test_dram_load() {
        let path =
            std::env::temp_dir().join(format!("arvsim-dram-load-{}.bin", std::process::id()));
        std::fs::write(&path, [0x93, 0x0f, 0xa0, 0x02]).unwrap();

        let mut dram = Dram::new();
        let result = dram.load(path.to_str().unwrap());
        assert!(result.is_ok());
        let val = dram.read(dram.base, 4).unwrap();
        assert_eq!(val, 0x02a00f93);

        std::fs::remove_file(path).unwrap();
    }
}
