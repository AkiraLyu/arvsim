use std::collections::BTreeMap;

pub trait MemDevice {
    fn read(&mut self, addr: u64, size: usize) -> Result<u64, ()>;
    fn write(&mut self, addr: u64, value: u32, size: usize) -> Result<(), ()>;
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
    fn read(&mut self, addr: u64, size: usize) -> Result<u64, ()> {
        if let Some(dev) = Self::find_dev(&mut self.ram_map, addr) {
            return dev.read(addr, size);
        }
        if let Some(dev) = Self::find_dev(&mut self.uart_map, addr) {
            return dev.read(addr, size);
        }
        Err(())
    }

    fn write(&mut self, addr: u64, value: u32, size: usize) -> Result<(), ()> {
        if let Some(dev) = Self::find_dev(&mut self.ram_map, addr) {
            return dev.write(addr, value, size);
        }
        if let Some(dev) = Self::find_dev(&mut self.uart_map, addr) {
            return dev.write(addr, value, size);
        }
        Err(())
    }
}