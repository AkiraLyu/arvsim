//! 内存映射设备接口、设备生命周期和物理总线。
//!
//! [`Bus`] 维护互不重叠的半开地址区间，并把 CPU 的物理读写转发给覆盖完整访问范围的设备。
//! 地址未命中时在总线边界统一产生访问错误，设备仍负责解释自己的寄存器偏移和访问宽度。

use crate::trap::{Exception, InterruptSet};
use std::cell::{Ref, RefCell, RefMut};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::rc::Rc;

/// 可克隆的单线程共享对象句柄。
///
/// 总线持有句柄的克隆，平台组装器和设备后端可以保留另一份，用于 DMA、设备连线或状态观察。
pub struct Shared<T>(Rc<RefCell<T>>);

impl<T> Shared<T> {
    pub fn new(value: T) -> Self {
        Self(Rc::new(RefCell::new(value)))
    }

    pub fn borrow(&self) -> Ref<'_, T> {
        self.0.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        self.0.borrow_mut()
    }
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

/// CPU 和机器运行层与 RAM、MMIO 设备之间的统一协议。
///
/// 地址是总线物理地址而不是设备内偏移。读写宽度只能是 1、2、4 或 8 字节。
/// 设备必须先验证完整访问，再产生写入或 MMIO 副作用；返回错误时不得留下部分写入。
pub trait MemDevice {
    /// 从 `addr` 开始读取 `size` 个字节，并以小端整数返回。
    fn read(&mut self, addr: u64, size: usize) -> Result<u64, Exception>;
    /// 向 `addr` 开始的 `size` 个字节写入 `value` 的低位部分。
    fn write(&mut self, addr: u64, value: u64, size: usize) -> Result<(), Exception>;
    /// 返回当前同时有效的 RISC-V 中断位；默认设备不产生中断。
    fn pending_interrupts(&mut self) -> InterruptSet {
        InterruptSet::EMPTY
    }
    /// 恢复设备的上电状态；默认用于没有易失状态的 RAM 或同步设备。
    fn reset(&mut self) {}
    /// 推进 `cycles` 个平台周期；默认用于不依赖时间推进的设备。
    fn tick(&mut self, _cycles: u64) {}
}

impl<T: MemDevice> MemDevice for Shared<T> {
    fn read(&mut self, addr: u64, size: usize) -> Result<u64, Exception> {
        self.borrow_mut().read(addr, size)
    }

    fn write(&mut self, addr: u64, value: u64, size: usize) -> Result<(), Exception> {
        self.borrow_mut().write(addr, value, size)
    }

    fn pending_interrupts(&mut self) -> InterruptSet {
        self.borrow_mut().pending_interrupts()
    }

    fn reset(&mut self) {
        self.borrow_mut().reset();
    }

    fn tick(&mut self, cycles: u64) {
        self.borrow_mut().tick(cycles);
    }
}

pub(crate) const fn valid_access_size(size: usize) -> bool {
    matches!(size, 1 | 2 | 4 | 8)
}

/// 设备区域无法加入总线的原因。
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BusError {
    /// 设备区域必须至少包含一个字节。
    EmptyRegion,
    /// 区域末端超出 `u64` 可表示范围。
    AddressOverflow,
    /// 新区域与已有设备区域重叠。
    RegionOverlap { base: u64, end: u64 },
}

impl Display for BusError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyRegion => f.write_str("device region size must be non-zero"),
            Self::AddressOverflow => f.write_str("device region end overflows u64"),
            Self::RegionOverlap { base, end } => {
                write!(
                    f,
                    "device range {base:#x}..{end:#x} overlaps another region"
                )
            }
        }
    }
}

impl Error for BusError {}

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

    /// 按当前 UART 窗口大小挂载设备的便捷入口。
    pub fn attach_uart(&mut self, base: u64, dev: Box<dyn MemDevice>) -> Result<(), BusError> {
        self.attach_device(base, crate::cfg::UART_SIZE, dev)
    }

    /// 挂载一个设备，并拒绝零长度、地址溢出或区间重叠。
    pub fn attach_device(
        &mut self,
        base: u64,
        size: u64,
        dev: Box<dyn MemDevice>,
    ) -> Result<(), BusError> {
        if size == 0 {
            return Err(BusError::EmptyRegion);
        }
        let end = base.checked_add(size).ok_or(BusError::AddressOverflow)?;

        if let Some((_, prev)) = self.devices.range(..=base).next_back() {
            let prev_end = prev
                .base
                .checked_add(prev.size)
                .ok_or(BusError::AddressOverflow)?;
            if prev_end > base {
                return Err(BusError::RegionOverlap { base, end });
            }
        }
        if let Some((&next_base, _)) = self.devices.range(base..).next()
            && end > next_base
        {
            return Err(BusError::RegionOverlap { base, end });
        }

        self.devices.insert(base, DeviceRegion { base, size, dev });
        Ok(())
    }

    fn find_dev(&mut self, addr: u64, size: usize) -> Option<&mut Box<dyn MemDevice>> {
        if !valid_access_size(size) {
            return None;
        }
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

    fn write(&mut self, addr: u64, value: u64, size: usize) -> Result<(), Exception> {
        if let Some(dev) = self.find_dev(addr, size) {
            return dev.write(addr, value, size);
        }
        Err(Exception::StoreAMOAccessFault(addr))
    }

    fn pending_interrupts(&mut self) -> InterruptSet {
        let mut pending = InterruptSet::EMPTY;
        for region in self.devices.values_mut() {
            pending.merge(region.dev.pending_interrupts());
        }
        pending
    }

    fn reset(&mut self) {
        for region in self.devices.values_mut() {
            region.dev.reset();
        }
    }

    fn tick(&mut self, cycles: u64) {
        for region in self.devices.values_mut() {
            region.dev.tick(cycles);
        }
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
    use crate::trap::InterruptCause;

    struct FixedDevice;

    impl MemDevice for FixedDevice {
        fn read(&mut self, _addr: u64, _size: usize) -> Result<u64, Exception> {
            Ok(0xaa)
        }

        fn write(&mut self, _addr: u64, _value: u64, _size: usize) -> Result<(), Exception> {
            Ok(())
        }
    }

    struct InterruptSource(InterruptCause);

    impl MemDevice for InterruptSource {
        fn read(&mut self, addr: u64, _size: usize) -> Result<u64, Exception> {
            Err(Exception::LoadAccessFault(addr))
        }

        fn write(&mut self, addr: u64, _value: u64, _size: usize) -> Result<(), Exception> {
            Err(Exception::StoreAMOAccessFault(addr))
        }

        fn pending_interrupts(&mut self) -> InterruptSet {
            InterruptSet::from_cause(self.0)
        }
    }

    #[test]
    fn test_bus_read_write() {
        let mut dram = Dram::with_layout(crate::cfg::DRAM_BASE, 16);
        let base = dram.base;
        let size = dram.dram.len();
        for i in 0..size {
            dram.dram[i] = i as u8;
        }

        let mut bus = Bus::new();
        bus.attach_device(base, size as u64, Box::new(dram))
            .unwrap();

        for i in 0..size {
            let val = bus.read(base + i as u64, 1).unwrap();
            assert_eq!(val, i as u64);
        }

        for i in 0..size {
            bus.write(base + i as u64, (i + 1) as u64, 1).unwrap();
        }
        for i in 0..size {
            let val = bus.read(base + i as u64, 1).unwrap();
            assert_eq!(val, (i + 1) as u64);
        }
    }

    #[test]
    fn pending_interrupts_are_merged_across_devices() {
        let mut bus = Bus::new();
        bus.attach_device(
            0x1000,
            4,
            Box::new(InterruptSource(InterruptCause::MachineTimer)),
        )
        .unwrap();
        bus.attach_device(
            0x2000,
            4,
            Box::new(InterruptSource(InterruptCause::SupervisorExternal)),
        )
        .unwrap();

        let pending = bus.pending_interrupts();
        assert!(pending.contains(InterruptCause::MachineTimer));
        assert!(pending.contains(InterruptCause::SupervisorExternal));
    }

    #[test]
    fn test_bus_checks_device_region_size() {
        let mut bus = Bus::new();
        bus.attach_device(0x1000, 0x10, Box::new(FixedDevice))
            .unwrap();

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

    #[test]
    fn test_bus_rejects_invalid_widths_and_overflowing_ranges() {
        let mut bus = Bus::new();
        bus.attach_device(0x1000, 0x10, Box::new(FixedDevice))
            .unwrap();

        for size in [0, 3, 9, usize::MAX] {
            assert_eq!(
                bus.read(0x1000, size),
                Err(Exception::LoadAccessFault(0x1000))
            );
            assert_eq!(
                bus.write(0x1000, 0, size),
                Err(Exception::StoreAMOAccessFault(0x1000))
            );
        }

        let mut high_bus = Bus::new();
        let base = u64::MAX - 0x10;
        high_bus
            .attach_device(base, 0x10, Box::new(FixedDevice))
            .unwrap();
        assert_eq!(high_bus.read(base + 8, 8), Ok(0xaa));
        assert_eq!(
            high_bus.read(base + 9, 8),
            Err(Exception::LoadAccessFault(base + 9))
        );
        assert_eq!(
            high_bus.write(base + 9, 0, 8),
            Err(Exception::StoreAMOAccessFault(base + 9))
        );
    }

    #[test]
    fn attach_device_returns_recoverable_layout_errors() {
        let mut bus = Bus::new();
        assert_eq!(
            bus.attach_device(0x1000, 0, Box::new(FixedDevice)),
            Err(BusError::EmptyRegion)
        );
        assert_eq!(
            bus.attach_device(u64::MAX, 2, Box::new(FixedDevice)),
            Err(BusError::AddressOverflow)
        );

        bus.attach_device(0x1000, 0x100, Box::new(FixedDevice))
            .unwrap();
        assert_eq!(
            bus.attach_device(0x1080, 0x100, Box::new(FixedDevice)),
            Err(BusError::RegionOverlap {
                base: 0x1080,
                end: 0x1180,
            })
        );
    }
}
