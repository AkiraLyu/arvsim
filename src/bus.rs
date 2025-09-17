use std::collections::BTreeMap;
use crate::exception::Exception;

pub trait MemDevice {
    fn read(&mut self, addr: u64, size: usize) -> Result<u64, Exception>;
    fn write(&mut self, addr: u64, value: u32, size: usize) -> Result<(), Exception>;
}

pub struct Bus {
    ram_map: BTreeMap<u64, Box<dyn MemDevice>>,
    uart_map: BTreeMap<u64, Box<dyn MemDevice>>,
}

impl Bus {
    pub fn new() -> Self {
        Bus {
            ram_map: BTreeMap::new(),
            uart_map: BTreeMap::new(),
        }
    }

    pub fn attach_ram(&mut self, base: u64, dev: Box<dyn MemDevice>) {
        self.ram_map.insert(base, dev);
    }

    pub fn attach_uart(&mut self, base: u64, dev: Box<dyn MemDevice>) {
        self.uart_map.insert(base, dev);
    }

    fn find_dev(
        map: &mut BTreeMap<u64, Box<dyn MemDevice>>,
        addr: u64,
    ) -> Option<&mut Box<dyn MemDevice>> {
        let mut key = None;
        for k in map.keys() {
            if *k <= addr {
                key = Some(*k);
            }
        }
        key.and_then(move |k| map.get_mut(&k))
    }
}

impl MemDevice for Bus {
    fn read(&mut self, addr: u64, size: usize) -> Result<u64, Exception> {
        if let Some(dev) = Self::find_dev(&mut self.ram_map, addr) {
            return dev.read(addr, size);
        }
        if let Some(dev) = Self::find_dev(&mut self.uart_map, addr) {
            return dev.read(addr, size);
        }
        Err(Exception::LoadAccessFault(addr))
    }

    fn write(&mut self, addr: u64, value: u32, size: usize) -> Result<(), Exception> {
        if let Some(dev) = Self::find_dev(&mut self.ram_map, addr) {
            return dev.write(addr, value, size);
        }
        if let Some(dev) = Self::find_dev(&mut self.uart_map, addr) {
            return dev.write(addr, value, size);
        }
        Err(Exception::StoreAccessFault(addr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dram::Dram;

    #[test]
    fn test_bus_read_write() {
        let mut dram = Dram::new();
        let base = dram.base;
        let size = 16;
        for i in 0..size {
            dram.dram[i] = i as u8;
        }

        let mut bus = Bus::new();
        bus.attach_ram(base, Box::new(dram));

        // read
        for i in 0..size {
            let val = bus.read(base + i as u64, 1).unwrap();
            assert_eq!(val, i as u64);
        }

        // write
        for i in 0..size {
            bus.write(base + i as u64, (i + 1) as u32, 1).unwrap();
        }
        for i in 0..size {
            let val = bus.read(base + i as u64, 1).unwrap();
            assert_eq!(val, (i + 1) as u64);
        }
    }
} 