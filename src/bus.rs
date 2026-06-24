use crate::trap::Exception;
use std::collections::BTreeMap;

pub trait MemDevice {
    fn read(&mut self, addr: u64, size: usize) -> Result<u64, Exception>;
    fn write(&mut self, addr: u64, value: u32, size: usize) -> Result<(), Exception>;
    fn pending_interrupt(&mut self) -> Option<u64> {
        None
    }
}

pub struct Bus {
    devices: BTreeMap<u64, DeviceRegion>,
}

pub struct DeviceRegion {
    pub base: u64,
    pub size: u64,
    pub dev: Box<dyn MemDevice>,
}

impl Bus {
    pub fn new() -> Self {
        Bus {
            devices: BTreeMap::new(),
        }
    }

    pub fn attach_ram(&mut self, base: u64, dev: Box<dyn MemDevice>) {
        self.attach_device(base, crate::cfg::DRAM_SIZE as u64, dev);
    }

    pub fn attach_uart(&mut self, base: u64, dev: Box<dyn MemDevice>) {
        self.attach_device(base, 0x100, dev);
    }

    pub fn attach_device(&mut self, base: u64, size: u64, dev: Box<dyn MemDevice>) {
        assert!(size > 0, "device region size must be non-zero");
        let end = base.checked_add(size).expect("device region end overflow");

        if let Some((_, prev)) = self.devices.range(..=base).next_back() {
            let prev_end = prev
                .base
                .checked_add(prev.size)
                .expect("device region end overflow");
            assert!(prev_end <= base, "device region overlaps");
        }
        if let Some((&next_base, _)) = self.devices.range(base..).next() {
            assert!(end <= next_base, "device region overlaps");
        }

        self.devices.insert(base, DeviceRegion { base, size, dev });
    }

    fn find_dev(&mut self, addr: u64, size: usize) -> Option<&mut Box<dyn MemDevice>> {
        let size = u64::try_from(size).ok()?;
        let (_, region) = self.devices.range_mut(..=addr).next_back()?;
        let end = addr.checked_add(size)?;
        let region_end = region.base.checked_add(region.size)?;
        (end <= region_end).then_some(&mut region.dev)
    }
}

impl MemDevice for Bus {
    fn read(&mut self, addr: u64, size: usize) -> Result<u64, Exception> {
        if let Some(dev) = self.find_dev(addr, size) {
            return dev.read(addr, size);
        }
        Err(Exception::LoadAccessFault(addr))
    }

    fn write(&mut self, addr: u64, value: u32, size: usize) -> Result<(), Exception> {
        if let Some(dev) = self.find_dev(addr, size) {
            return dev.write(addr, value, size);
        }
        Err(Exception::StoreAMOAccessFault(addr))
    }

    fn pending_interrupt(&mut self) -> Option<u64> {
        self.devices
            .values_mut()
            .find_map(|region| region.dev.pending_interrupt())
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dram::Dram;

    struct FixedDevice;

    impl MemDevice for FixedDevice {
        fn read(&mut self, _addr: u64, _size: usize) -> Result<u64, Exception> {
            Ok(0xaa)
        }

        fn write(&mut self, _addr: u64, _value: u32, _size: usize) -> Result<(), Exception> {
            Ok(())
        }
    }

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

    #[test]
    fn test_bus_checks_device_region_size() {
        let mut bus = Bus::new();
        bus.attach_device(0x1000, 0x10, Box::new(FixedDevice));

        assert_eq!(bus.read(0x100f, 1).unwrap(), 0xaa);
        assert!(matches!(
            bus.read(0x1010, 1),
            Err(Exception::LoadAccessFault(0x1010))
        ));
        assert!(matches!(
            bus.write(0x1010, 0, 1),
            Err(Exception::StoreAMOAccessFault(0x1010))
        ));
    }
}
