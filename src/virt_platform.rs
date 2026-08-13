//! 单 hart QEMU `virt` 风格平台的正式组装器。
//!
//! 地址和 IRQ 均由 [`VirtPlatformConfig`] 提供；组装器只负责把共享 DRAM、16550 UART、
//! PLIC 和 virtio-blk 按中断线连接起来，并复用 [`crate::machine::Platform`] 的区域校验。

use crate::bus::Shared;
use crate::dram::Dram;
use crate::machine::{Machine, Platform, PlatformError};
use crate::plic::{Plic, PlicError, PlicLayout};
use crate::trap::InterruptCause;
use crate::uart::{Uart, UartBackend, UartError};
use crate::virtio::{BlockBackend, VirtioBlock, VirtioBlockConfig, VirtioBlockError};
use std::cell::RefMut;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::ops::{Deref, DerefMut};

/// 一个单 hart `virt` 平台的完整可配置布局。
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct VirtPlatformConfig {
    pub dram_base: u64,
    pub dram_size: usize,
    pub plic_base: u64,
    pub plic_layout: PlicLayout,
    pub plic_source_count: u32,
    pub plic_max_priority: u32,
    pub uart_base: u64,
    pub uart_size: u64,
    pub uart_irq: u32,
    pub uart_transmit_delay_cycles: u64,
    pub block_base: u64,
    pub block_size: u64,
    pub block_irq: u32,
    pub block_queue_size: u16,
    pub virtio_vendor_id: u32,
}

impl Default for VirtPlatformConfig {
    fn default() -> Self {
        Self {
            dram_base: crate::cfg::DRAM_BASE,
            dram_size: crate::cfg::DRAM_SIZE,
            plic_base: crate::cfg::PLIC_BASE,
            plic_layout: PlicLayout::SIFIVE,
            plic_source_count: crate::cfg::PLIC_SOURCE_COUNT,
            plic_max_priority: crate::cfg::PLIC_MAX_PRIORITY,
            uart_base: crate::cfg::UART_BASE,
            uart_size: crate::cfg::UART_SIZE,
            uart_irq: crate::cfg::UART_IRQ,
            uart_transmit_delay_cycles: crate::cfg::UART_TRANSMIT_DELAY_CYCLES,
            block_base: crate::cfg::VIRTIO_BLOCK_BASE,
            block_size: crate::cfg::VIRTIO_BLOCK_SIZE,
            block_irq: crate::cfg::VIRTIO_BLOCK_IRQ,
            block_queue_size: crate::cfg::VIRTIO_QUEUE_SIZE,
            virtio_vendor_id: crate::cfg::VIRTIO_VENDOR_ID,
        }
    }
}

#[derive(Debug)]
pub enum VirtPlatformError {
    Platform(PlatformError),
    Plic(PlicError),
    Uart(UartError),
    Virtio(VirtioBlockError),
    InterruptSourceConflict(u32),
}

impl Display for VirtPlatformError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Platform(error) => Display::fmt(error, f),
            Self::Plic(error) => Display::fmt(error, f),
            Self::Uart(error) => Display::fmt(error, f),
            Self::Virtio(error) => Display::fmt(error, f),
            Self::InterruptSourceConflict(source) => {
                write!(f, "multiple devices use PLIC source {source}")
            }
        }
    }
}

impl Error for VirtPlatformError {}

impl From<PlatformError> for VirtPlatformError {
    fn from(value: PlatformError) -> Self {
        Self::Platform(value)
    }
}

impl From<PlicError> for VirtPlatformError {
    fn from(value: PlicError) -> Self {
        Self::Plic(value)
    }
}

impl From<UartError> for VirtPlatformError {
    fn from(value: UartError) -> Self {
        Self::Uart(value)
    }
}

impl From<VirtioBlockError> for VirtPlatformError {
    fn from(value: VirtioBlockError) -> Self {
        Self::Virtio(value)
    }
}

/// 已挂载设备的共享句柄；用于平台宿主注入输入或读取设备状态。
pub struct VirtPlatformDevices {
    dram: Shared<Dram>,
    plic: Shared<Plic>,
    uart: Shared<Uart>,
    block: Shared<VirtioBlock>,
}

impl Clone for VirtPlatformDevices {
    fn clone(&self) -> Self {
        Self {
            dram: self.dram.clone(),
            plic: self.plic.clone(),
            uart: self.uart.clone(),
            block: self.block.clone(),
        }
    }
}

impl VirtPlatformDevices {
    pub fn dram(&self) -> Shared<Dram> {
        self.dram.clone()
    }

    pub fn plic(&self) -> Shared<Plic> {
        self.plic.clone()
    }

    pub fn uart(&self) -> Shared<Uart> {
        self.uart.clone()
    }

    pub fn block(&self) -> Shared<VirtioBlock> {
        self.block.clone()
    }
}

/// 尚未构建 CPU、但设备与 DMA 已完成连线的平台。
pub struct VirtPlatform {
    platform: Platform,
    devices: VirtPlatformDevices,
}

impl VirtPlatform {
    pub fn new(
        config: VirtPlatformConfig,
        uart_backend: Box<dyn UartBackend>,
        block_backend: Box<dyn BlockBackend>,
    ) -> Result<Self, VirtPlatformError> {
        if config.uart_irq == config.block_irq {
            return Err(VirtPlatformError::InterruptSourceConflict(config.uart_irq));
        }
        let mut platform = Platform::new(config.dram_base, config.dram_size)?;
        let dram = platform.dram_handle();
        let mut plic_device = Plic::new(
            config.plic_base,
            config.plic_layout,
            config.plic_source_count,
            config.plic_max_priority,
            vec![
                InterruptCause::MachineExternal,
                InterruptCause::SupervisorExternal,
            ],
        )?;
        let uart_interrupt = plic_device.source_line(config.uart_irq)?;
        let block_interrupt = plic_device.source_line(config.block_irq)?;
        let plic = Shared::new(plic_device);
        let uart = Shared::new(Uart::with_backend_and_timing(
            config.uart_base,
            config.uart_size,
            uart_backend,
            uart_interrupt,
            config.uart_transmit_delay_cycles,
        )?);
        let block = Shared::new(VirtioBlock::new(
            VirtioBlockConfig {
                base: config.block_base,
                size: config.block_size,
                queue_size: config.block_queue_size,
                vendor_id: config.virtio_vendor_id,
            },
            Box::new(dram.clone()),
            block_backend,
            block_interrupt,
        )?);

        platform.attach_device(
            config.plic_base,
            config.plic_layout.size,
            Box::new(plic.clone()),
        )?;
        platform.attach_device(config.uart_base, config.uart_size, Box::new(uart.clone()))?;
        platform.attach_device(
            config.block_base,
            config.block_size,
            Box::new(block.clone()),
        )?;

        Ok(Self {
            platform,
            devices: VirtPlatformDevices {
                dram,
                plic,
                uart,
                block,
            },
        })
    }

    pub fn dram_mut(&self) -> RefMut<'_, Dram> {
        self.platform.dram_mut()
    }

    pub fn devices(&self) -> VirtPlatformDevices {
        self.devices.clone()
    }

    pub fn build(self, reset_vector: u64) -> Result<VirtMachine, VirtPlatformError> {
        Ok(VirtMachine {
            machine: self.platform.build(reset_vector)?,
            devices: self.devices,
        })
    }
}

/// 正式 `virt` 平台机器与其设备句柄。
pub struct VirtMachine {
    pub machine: Machine,
    pub devices: VirtPlatformDevices,
}

impl Deref for VirtMachine {
    type Target = Machine;

    fn deref(&self) -> &Self::Target {
        &self.machine
    }
}

impl DerefMut for VirtMachine {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.machine
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uart::BufferedUartBackend;
    use crate::virtio::{MMIO_MAGIC_VALUE, MemoryBlockBackend};

    #[test]
    fn default_platform_uses_shared_formal_devices() {
        let config = VirtPlatformConfig {
            dram_size: 0x10_000,
            ..VirtPlatformConfig::default()
        };
        let uart = BufferedUartBackend::new();
        let disk = MemoryBlockBackend::new(vec![0; 4096]);
        let platform = VirtPlatform::new(config, Box::new(uart), Box::new(disk)).unwrap();
        platform
            .dram_mut()
            .write_bytes(config.dram_base, &[0x13, 0, 0, 0])
            .unwrap();
        let mut machine = platform.build(config.dram_base).unwrap();

        assert_eq!(
            machine.cpu.bus.read(config.block_base, 4),
            Ok(u64::from(MMIO_MAGIC_VALUE))
        );
        assert_eq!(machine.cpu.bus.read(config.dram_base, 4), Ok(0x13));
    }
}
