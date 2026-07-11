//! 内存映射设备接口和物理总线。
//!
//! [`Bus`] 维护互不重叠的半开地址区间，并把 CPU 的物理读写转发给覆盖完整访问范围的设备。
//! 地址未命中时在总线边界统一产生访问错误，设备仍负责解释自己的寄存器偏移和访问宽度。

use crate::trap::Exception;
use std::collections::BTreeMap;

/// CPU 与 RAM、MMIO 设备之间的最小访问协议。
///
/// 地址是总线物理地址而不是设备内偏移。`write` 的数据宽度受现有接口约束为 `u32`；
/// 需要写入 64 位值的上层必须拆成两次 32 位访问。
pub trait MemDevice {
    /// 从 `addr` 开始读取 `size` 个字节，并以小端整数返回。
    fn read(&mut self, addr: u64, size: usize) -> Result<u64, Exception>;
    /// 向 `addr` 开始的 `size` 个字节写入 `value` 的低位部分。
    fn write(&mut self, addr: u64, value: u32, size: usize) -> Result<(), Exception>;
    /// 返回一个待处理的中断原因；默认设备不产生中断。
    fn pending_interrupt(&mut self) -> Option<u64> {
        None
    }
}

/// 按物理地址分发访问的设备总线。
pub struct Bus {
    devices: BTreeMap<u64, DeviceRegion>,
}

/// 一个已挂载设备及其半开地址区间 `[base, base + size)`。
pub struct DeviceRegion {
    pub base: u64,
    pub size: u64,
    pub dev: Box<dyn MemDevice>,
}

impl Bus {
    /// 创建一条尚未挂载任何设备的总线。
    pub fn new() -> Self {
        Bus {
            devices: BTreeMap::new(),
        }
    }

    /// 按默认 DRAM 容量挂载 RAM 的便捷入口。
    pub fn attach_ram(&mut self, base: u64, dev: Box<dyn MemDevice>) {
        self.attach_device(base, crate::cfg::DRAM_SIZE as u64, dev);
    }

    /// 按当前 UART 窗口大小挂载设备的便捷入口。
    pub fn attach_uart(&mut self, base: u64, dev: Box<dyn MemDevice>) {
        self.attach_device(base, 0x100, dev);
    }

    /// 挂载一个设备，并拒绝零长度、地址溢出或区间重叠。
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
        // 先找不大于起始地址的最后一个区域，再验证“整个访问”都没有越过区域末端。
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

        for i in 0..size {
            let val = bus.read(base + i as u64, 1).unwrap();
            assert_eq!(val, i as u64);
        }

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
