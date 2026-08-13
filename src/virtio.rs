//! Virtio 1.2 MMIO 块设备。
//!
//! 设备实现现代 MMIO 寄存器、特性协商、split virtqueue、块读写完成和中断确认。
//! guest 内存与块后端分别通过 [`GuestMemory`] 和 [`BlockBackend`] 注入，设备不依赖 xv6
//! 数据结构，也不会直接修改驱动私有状态。

use crate::bus::{MemDevice, Shared};
use crate::dram::Dram;
use crate::interrupt::InterruptLine;
use crate::trap::Exception;
use std::cell::{Ref, RefCell};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::rc::Rc;

pub const MMIO_MAGIC_VALUE: u32 = 0x7472_6976;
pub const MMIO_VERSION: u32 = 2;
pub const BLOCK_DEVICE_ID: u32 = 2;
pub const BLOCK_SECTOR_SIZE: u64 = 512;
pub const MIN_MMIO_SIZE: u64 = 0x108;
pub const MAX_QUEUE_SIZE: u16 = 32768;

const REG_MAGIC_VALUE: u64 = 0x000;
const REG_VERSION: u64 = 0x004;
const REG_DEVICE_ID: u64 = 0x008;
const REG_VENDOR_ID: u64 = 0x00c;
const REG_DEVICE_FEATURES: u64 = 0x010;
const REG_DEVICE_FEATURES_SEL: u64 = 0x014;
const REG_DRIVER_FEATURES: u64 = 0x020;
const REG_DRIVER_FEATURES_SEL: u64 = 0x024;
const REG_QUEUE_SEL: u64 = 0x030;
const REG_QUEUE_NUM_MAX: u64 = 0x034;
const REG_QUEUE_NUM: u64 = 0x038;
const REG_QUEUE_READY: u64 = 0x044;
const REG_QUEUE_NOTIFY: u64 = 0x050;
const REG_INTERRUPT_STATUS: u64 = 0x060;
const REG_INTERRUPT_ACK: u64 = 0x064;
const REG_STATUS: u64 = 0x070;
const REG_QUEUE_DESC_LOW: u64 = 0x080;
const REG_QUEUE_DESC_HIGH: u64 = 0x084;
const REG_QUEUE_DRIVER_LOW: u64 = 0x090;
const REG_QUEUE_DRIVER_HIGH: u64 = 0x094;
const REG_QUEUE_DEVICE_LOW: u64 = 0x0a0;
const REG_QUEUE_DEVICE_HIGH: u64 = 0x0a4;
const REG_CONFIG_GENERATION: u64 = 0x0fc;
const REG_BLOCK_CAPACITY_LOW: u64 = 0x100;
const REG_BLOCK_CAPACITY_HIGH: u64 = 0x104;

const STATUS_ACKNOWLEDGE: u8 = 1;
const STATUS_DRIVER: u8 = 2;
const STATUS_DRIVER_OK: u8 = 4;
const STATUS_FEATURES_OK: u8 = 8;
const STATUS_DEVICE_NEEDS_RESET: u8 = 64;
const STATUS_FAILED: u8 = 128;
const DRIVER_STATUS_MASK: u8 =
    STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_DRIVER_OK | STATUS_FEATURES_OK | STATUS_FAILED;

const INTERRUPT_USED_BUFFER: u32 = 1;
const INTERRUPT_CONFIGURATION_CHANGE: u32 = 2;
const VIRTIO_F_VERSION_1: u64 = 1 << 32;
const VIRTIO_BLK_F_RO: u64 = 1 << 5;

const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;
const VIRTQ_DESC_F_INDIRECT: u16 = 4;
const VIRTQ_DESC_SUPPORTED_FLAGS: u16 =
    VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE | VIRTQ_DESC_F_INDIRECT;
const VIRTQ_AVAIL_F_NO_INTERRUPT: u16 = 1;

const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_T_OUT: u32 = 1;
const VIRTIO_BLK_S_OK: u8 = 0;
const VIRTIO_BLK_S_IOERR: u8 = 1;
const VIRTIO_BLK_S_UNSUPP: u8 = 2;
const TRANSFER_CHUNK_SIZE: usize = 64 * 1024;

/// Virtio DMA 访问 guest 物理内存时的错误。
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DmaError {
    Read(u64),
    Write(u64),
    AddressOverflow(u64),
}

impl Display for DmaError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(addr) => write!(f, "guest memory read failed at {addr:#x}"),
            Self::Write(addr) => write!(f, "guest memory write failed at {addr:#x}"),
            Self::AddressOverflow(addr) => {
                write!(f, "guest memory address overflows from {addr:#x}")
            }
        }
    }
}

impl Error for DmaError {}

/// 可被设备 DMA 的 guest 物理内存。
pub trait GuestMemory {
    fn read(&mut self, addr: u64, output: &mut [u8]) -> Result<(), DmaError>;
    fn write(&mut self, addr: u64, input: &[u8]) -> Result<(), DmaError>;
}

impl GuestMemory for Shared<Dram> {
    fn read(&mut self, addr: u64, output: &mut [u8]) -> Result<(), DmaError> {
        self.borrow()
            .read_bytes(addr, output)
            .map_err(|_| DmaError::Read(addr))
    }

    fn write(&mut self, addr: u64, input: &[u8]) -> Result<(), DmaError> {
        self.borrow_mut()
            .write_bytes(addr, input)
            .map_err(|_| DmaError::Write(addr))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BlockError {
    OutOfRange,
    ReadOnly,
    CapacityMismatch,
    BackendFailure,
}

impl Display for BlockError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfRange => f.write_str("block access exceeds backend capacity"),
            Self::ReadOnly => f.write_str("block backend is read-only"),
            Self::CapacityMismatch => {
                f.write_str("replacement media must preserve the advertised capacity")
            }
            Self::BackendFailure => f.write_str("block backend operation failed"),
        }
    }
}

impl Error for BlockError {}

/// Virtio 块设备使用的随机访问后端。
///
/// 后端装入设备后必须保持容量和只读属性不变；它们是 Virtio 特性与配置空间的
/// 一部分。介质内容可在容量不变的前提下替换。
pub trait BlockBackend {
    fn capacity_bytes(&self) -> u64;
    fn read_at(&mut self, offset: u64, output: &mut [u8]) -> Result<(), BlockError>;
    fn write_at(&mut self, offset: u64, input: &[u8]) -> Result<(), BlockError>;
    fn is_read_only(&self) -> bool {
        false
    }
}

#[derive(Debug)]
struct MemoryBlockState {
    bytes: Vec<u8>,
    read_only: bool,
}

/// 以共享字节向量保存数据的正式块后端。
#[derive(Debug, Clone)]
pub struct MemoryBlockBackend(Rc<RefCell<MemoryBlockState>>);

impl MemoryBlockBackend {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self::with_read_only(bytes, false)
    }

    pub fn with_read_only(bytes: Vec<u8>, read_only: bool) -> Self {
        Self(Rc::new(RefCell::new(MemoryBlockState { bytes, read_only })))
    }

    pub fn replace(&self, bytes: Vec<u8>) -> Result<(), BlockError> {
        if bytes.len() != self.0.borrow().bytes.len() {
            return Err(BlockError::CapacityMismatch);
        }
        self.0.borrow_mut().bytes = bytes;
        Ok(())
    }

    pub fn bytes(&self) -> Ref<'_, [u8]> {
        Ref::map(self.0.borrow(), |state| state.bytes.as_slice())
    }

    fn range(&self, offset: u64, len: usize) -> Result<std::ops::Range<usize>, BlockError> {
        let start = usize::try_from(offset).map_err(|_| BlockError::OutOfRange)?;
        let end = start.checked_add(len).ok_or(BlockError::OutOfRange)?;
        (end <= self.0.borrow().bytes.len())
            .then_some(start..end)
            .ok_or(BlockError::OutOfRange)
    }
}

impl BlockBackend for MemoryBlockBackend {
    fn capacity_bytes(&self) -> u64 {
        u64::try_from(self.0.borrow().bytes.len()).unwrap_or(u64::MAX)
    }

    fn read_at(&mut self, offset: u64, output: &mut [u8]) -> Result<(), BlockError> {
        let range = self.range(offset, output.len())?;
        output.copy_from_slice(&self.0.borrow().bytes[range]);
        Ok(())
    }

    fn write_at(&mut self, offset: u64, input: &[u8]) -> Result<(), BlockError> {
        if self.is_read_only() {
            return Err(BlockError::ReadOnly);
        }
        let range = self.range(offset, input.len())?;
        self.0.borrow_mut().bytes[range].copy_from_slice(input);
        Ok(())
    }

    fn is_read_only(&self) -> bool {
        self.0.borrow().read_only
    }
}

/// Virtio MMIO 块设备的静态参数。
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct VirtioBlockConfig {
    pub base: u64,
    pub size: u64,
    pub queue_size: u16,
    pub vendor_id: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VirtioBlockError {
    MmioWindowTooSmall,
    MisalignedMmioWindow,
    AddressOverflow,
    InvalidQueueSize,
}

impl Display for VirtioBlockError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MmioWindowTooSmall => write!(
                f,
                "virtio MMIO window must contain at least {MIN_MMIO_SIZE:#x} bytes"
            ),
            Self::MisalignedMmioWindow => {
                f.write_str("virtio MMIO base and window size must be 32-bit aligned")
            }
            Self::AddressOverflow => f.write_str("virtio MMIO range overflows u64"),
            Self::InvalidQueueSize => {
                write!(
                    f,
                    "virtio split queue size must be a power of two in 1..={MAX_QUEUE_SIZE}"
                )
            }
        }
    }
}

impl Error for VirtioBlockError {}

#[derive(Debug, Copy, Clone, Default)]
struct VirtQueue {
    num: u16,
    ready: bool,
    descriptor_addr: u64,
    driver_addr: u64,
    device_addr: u64,
    last_available_idx: u16,
    next_used_idx: u16,
}

#[derive(Debug, Copy, Clone)]
struct Descriptor {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[derive(Debug, Copy, Clone)]
struct RequestCompletion {
    status_addr: u64,
    status: u8,
    written_len: u32,
}

/// 单队列、现代 MMIO transport 的 Virtio 块设备。
pub struct VirtioBlock {
    config: VirtioBlockConfig,
    memory: Box<dyn GuestMemory>,
    backend: Box<dyn BlockBackend>,
    interrupt_line: InterruptLine,
    capacity_sectors: u64,
    device_features: u64,
    driver_features: u64,
    device_features_sel: u32,
    driver_features_sel: u32,
    queue_sel: u32,
    queue: VirtQueue,
    interrupt_status: u32,
    status: u8,
    config_generation: u32,
}

impl VirtioBlock {
    pub fn new(
        config: VirtioBlockConfig,
        memory: Box<dyn GuestMemory>,
        backend: Box<dyn BlockBackend>,
        interrupt_line: InterruptLine,
    ) -> Result<Self, VirtioBlockError> {
        if config.size < MIN_MMIO_SIZE {
            return Err(VirtioBlockError::MmioWindowTooSmall);
        }
        if config.base & 3 != 0 || config.size & 3 != 0 {
            return Err(VirtioBlockError::MisalignedMmioWindow);
        }
        config
            .base
            .checked_add(config.size)
            .ok_or(VirtioBlockError::AddressOverflow)?;
        if config.queue_size == 0
            || config.queue_size > MAX_QUEUE_SIZE
            || !config.queue_size.is_power_of_two()
        {
            return Err(VirtioBlockError::InvalidQueueSize);
        }
        let capacity_sectors = backend.capacity_bytes() / BLOCK_SECTOR_SIZE;
        let device_features = VIRTIO_F_VERSION_1
            | if backend.is_read_only() {
                VIRTIO_BLK_F_RO
            } else {
                0
            };
        Ok(Self {
            config,
            memory,
            backend,
            interrupt_line,
            capacity_sectors,
            device_features,
            driver_features: 0,
            device_features_sel: 0,
            driver_features_sel: 0,
            queue_sel: 0,
            queue: VirtQueue::default(),
            interrupt_status: 0,
            status: 0,
            config_generation: 0,
        })
    }

    pub const fn base(&self) -> u64 {
        self.config.base
    }

    pub const fn size(&self) -> u64 {
        self.config.size
    }

    pub const fn device_status(&self) -> u8 {
        self.status
    }

    pub fn interrupt_line(&self) -> InterruptLine {
        self.interrupt_line.clone()
    }

    fn offset(&self, addr: u64) -> Option<u64> {
        let offset = addr.checked_sub(self.config.base)?;
        offset
            .checked_add(4)
            .filter(|end| *end <= self.config.size)
            .map(|_| offset)
    }

    fn selected_feature_word(features: u64, selector: u32) -> u32 {
        match selector {
            0 => features as u32,
            1 => (features >> 32) as u32,
            _ => 0,
        }
    }

    fn update_driver_features(&mut self, value: u32) {
        if self.status & STATUS_FEATURES_OK != 0 {
            return;
        }
        match self.driver_features_sel {
            0 => self.driver_features = (self.driver_features & !0xffff_ffff) | u64::from(value),
            1 => {
                self.driver_features =
                    (self.driver_features & 0xffff_ffff) | (u64::from(value) << 32)
            }
            _ => {}
        }
    }

    fn write_status(&mut self, value: u8) {
        if value == 0 {
            self.reset_transport();
            return;
        }
        let requested = value & DRIVER_STATUS_MASK;
        let mut next = self.status | requested;
        if requested & STATUS_FEATURES_OK != 0 && self.driver_features & !self.device_features != 0
        {
            next &= !STATUS_FEATURES_OK;
        }
        if requested & STATUS_DRIVER_OK != 0 && next & STATUS_FEATURES_OK == 0 {
            next &= !STATUS_DRIVER_OK;
        }
        self.status = next;
    }

    fn reset_transport(&mut self) {
        self.driver_features = 0;
        self.device_features_sel = 0;
        self.driver_features_sel = 0;
        self.queue_sel = 0;
        self.queue = VirtQueue::default();
        self.interrupt_status = 0;
        self.status = 0;
        self.interrupt_line.deassert();
    }

    fn set_low(value: &mut u64, low: u32) {
        *value = (*value & 0xffff_ffff_0000_0000) | u64::from(low);
    }

    fn set_high(value: &mut u64, high: u32) {
        *value = (*value & 0x0000_0000_ffff_ffff) | (u64::from(high) << 32);
    }

    fn update_interrupt_line(&self) {
        self.interrupt_line.set(self.interrupt_status != 0);
    }

    fn fail_device(&mut self) {
        self.status |= STATUS_DEVICE_NEEDS_RESET;
        self.interrupt_status |= INTERRUPT_CONFIGURATION_CHANGE;
        self.update_interrupt_line();
    }

    fn read_memory<const N: usize>(&mut self, addr: u64) -> Result<[u8; N], DmaError> {
        let mut bytes = [0; N];
        self.memory.read(addr, &mut bytes)?;
        Ok(bytes)
    }

    fn read_u16(&mut self, addr: u64) -> Result<u16, DmaError> {
        Ok(u16::from_le_bytes(self.read_memory(addr)?))
    }

    fn read_u32(&mut self, addr: u64) -> Result<u32, DmaError> {
        Ok(u32::from_le_bytes(self.read_memory(addr)?))
    }

    fn read_u64(&mut self, addr: u64) -> Result<u64, DmaError> {
        Ok(u64::from_le_bytes(self.read_memory(addr)?))
    }

    fn write_u16(&mut self, addr: u64, value: u16) -> Result<(), DmaError> {
        self.memory.write(addr, &value.to_le_bytes())
    }

    fn write_u32(&mut self, addr: u64, value: u32) -> Result<(), DmaError> {
        self.memory.write(addr, &value.to_le_bytes())
    }

    fn checked_addr(base: u64, offset: u64) -> Result<u64, DmaError> {
        base.checked_add(offset)
            .ok_or(DmaError::AddressOverflow(base))
    }

    fn descriptor(&mut self, index: u16) -> Result<Descriptor, DmaError> {
        let offset = u64::from(index)
            .checked_mul(16)
            .ok_or(DmaError::AddressOverflow(self.queue.descriptor_addr))?;
        let addr = Self::checked_addr(self.queue.descriptor_addr, offset)?;
        Ok(Descriptor {
            addr: self.read_u64(addr)?,
            len: self.read_u32(Self::checked_addr(addr, 8)?)?,
            flags: self.read_u16(Self::checked_addr(addr, 12)?)?,
            next: self.read_u16(Self::checked_addr(addr, 14)?)?,
        })
    }

    fn descriptor_chain(&mut self, head: u16) -> Result<Vec<Descriptor>, DmaError> {
        let queue_num = usize::from(self.queue.num);
        if usize::from(head) >= queue_num {
            return Err(DmaError::Read(self.queue.descriptor_addr));
        }
        let mut seen = vec![false; queue_num];
        let mut chain = Vec::new();
        let mut index = head;
        loop {
            let slot = usize::from(index);
            if slot >= queue_num || seen[slot] || chain.len() == queue_num {
                return Err(DmaError::Read(self.queue.descriptor_addr));
            }
            seen[slot] = true;
            let descriptor = self.descriptor(index)?;
            if descriptor.flags & !VIRTQ_DESC_SUPPORTED_FLAGS != 0
                || descriptor.flags & VIRTQ_DESC_F_INDIRECT != 0
            {
                return Err(DmaError::Read(descriptor.addr));
            }
            chain.push(descriptor);
            if descriptor.flags & VIRTQ_DESC_F_NEXT == 0 {
                return Ok(chain);
            }
            index = descriptor.next;
        }
    }

    fn transfer(
        &mut self,
        descriptors: &[Descriptor],
        disk_offset: u64,
        device_writes: bool,
    ) -> Result<u32, BlockError> {
        let mut transfer_len = 0u64;
        for descriptor in descriptors {
            let writable = descriptor.flags & VIRTQ_DESC_F_WRITE != 0;
            if writable != device_writes {
                return Err(BlockError::BackendFailure);
            }
            transfer_len = transfer_len
                .checked_add(u64::from(descriptor.len))
                .ok_or(BlockError::OutOfRange)?;
        }
        let medium_size = self
            .capacity_sectors
            .checked_mul(BLOCK_SECTOR_SIZE)
            .ok_or(BlockError::OutOfRange)?;
        let transfer_end = disk_offset
            .checked_add(transfer_len)
            .ok_or(BlockError::OutOfRange)?;
        if transfer_end > medium_size {
            return Err(BlockError::OutOfRange);
        }
        let used_len = if device_writes {
            u32::try_from(transfer_len)
                .ok()
                .and_then(|len| len.checked_add(1))
                .ok_or(BlockError::OutOfRange)?
        } else {
            1
        };

        let mut transferred = 0u64;
        let mut buffer = vec![0; TRANSFER_CHUNK_SIZE];
        for descriptor in descriptors {
            let mut remaining = u64::from(descriptor.len);
            let mut guest_addr = descriptor.addr;
            while remaining != 0 {
                let chunk = usize::try_from(remaining.min(TRANSFER_CHUNK_SIZE as u64))
                    .map_err(|_| BlockError::OutOfRange)?;
                let backend_addr = disk_offset
                    .checked_add(transferred)
                    .ok_or(BlockError::OutOfRange)?;
                if device_writes {
                    self.backend.read_at(backend_addr, &mut buffer[..chunk])?;
                    self.memory
                        .write(guest_addr, &buffer[..chunk])
                        .map_err(|_| BlockError::BackendFailure)?;
                } else {
                    self.memory
                        .read(guest_addr, &mut buffer[..chunk])
                        .map_err(|_| BlockError::BackendFailure)?;
                    self.backend.write_at(backend_addr, &buffer[..chunk])?;
                }
                guest_addr = guest_addr
                    .checked_add(chunk as u64)
                    .ok_or(BlockError::OutOfRange)?;
                transferred = transferred
                    .checked_add(chunk as u64)
                    .ok_or(BlockError::OutOfRange)?;
                remaining -= chunk as u64;
            }
        }
        Ok(used_len)
    }

    fn process_request(&mut self, head: u16) -> Result<RequestCompletion, DmaError> {
        let chain = self.descriptor_chain(head)?;
        if chain.len() < 2 {
            return Err(DmaError::Read(self.queue.descriptor_addr));
        }
        let header = chain[0];
        let status_descriptor = *chain.last().unwrap();
        if header.flags & VIRTQ_DESC_F_WRITE != 0
            || header.len < 16
            || status_descriptor.flags & VIRTQ_DESC_F_WRITE == 0
            || status_descriptor.len < 1
        {
            return Err(DmaError::Read(header.addr));
        }
        let header_bytes = self.read_memory::<16>(header.addr)?;
        let request_type = u32::from_le_bytes(header_bytes[0..4].try_into().unwrap());
        let sector = u64::from_le_bytes(header_bytes[8..16].try_into().unwrap());
        let disk_offset = sector
            .checked_mul(BLOCK_SECTOR_SIZE)
            .ok_or(DmaError::AddressOverflow(header.addr))?;
        let data = &chain[1..chain.len() - 1];

        let (status, written_len) = match request_type {
            VIRTIO_BLK_T_IN => match self.transfer(data, disk_offset, true) {
                Ok(written) => (VIRTIO_BLK_S_OK, written),
                Err(_) => (VIRTIO_BLK_S_IOERR, 1),
            },
            VIRTIO_BLK_T_OUT => match self.transfer(data, disk_offset, false) {
                Ok(written) => (VIRTIO_BLK_S_OK, written),
                Err(_) => (VIRTIO_BLK_S_IOERR, 1),
            },
            _ => (VIRTIO_BLK_S_UNSUPP, 1),
        };
        Ok(RequestCompletion {
            status_addr: status_descriptor.addr,
            status,
            written_len,
        })
    }

    fn complete_request(
        &mut self,
        head: u16,
        completion: RequestCompletion,
    ) -> Result<(), DmaError> {
        self.memory
            .write(completion.status_addr, &[completion.status])?;
        let slot = self.queue.next_used_idx % self.queue.num;
        let element_offset = u64::from(slot)
            .checked_mul(8)
            .and_then(|offset| offset.checked_add(4))
            .ok_or(DmaError::AddressOverflow(self.queue.device_addr))?;
        let element = Self::checked_addr(self.queue.device_addr, element_offset)?;
        self.write_u32(element, u32::from(head))?;
        self.write_u32(Self::checked_addr(element, 4)?, completion.written_len)?;
        self.queue.next_used_idx = self.queue.next_used_idx.wrapping_add(1);
        self.write_u16(
            Self::checked_addr(self.queue.device_addr, 2)?,
            self.queue.next_used_idx,
        )
    }

    fn process_available(&mut self) -> Result<bool, DmaError> {
        let available_idx = self.read_u16(Self::checked_addr(self.queue.driver_addr, 2)?)?;
        let count = available_idx.wrapping_sub(self.queue.last_available_idx);
        if count > self.queue.num {
            return Err(DmaError::Read(self.queue.driver_addr));
        }
        let mut processed = false;
        while self.queue.last_available_idx != available_idx {
            let slot = self.queue.last_available_idx % self.queue.num;
            let head_offset = u64::from(slot)
                .checked_mul(2)
                .and_then(|offset| offset.checked_add(4))
                .ok_or(DmaError::AddressOverflow(self.queue.driver_addr))?;
            let head = self.read_u16(Self::checked_addr(self.queue.driver_addr, head_offset)?)?;
            let completion = self.process_request(head)?;
            self.complete_request(head, completion)?;
            self.queue.last_available_idx = self.queue.last_available_idx.wrapping_add(1);
            processed = true;
        }
        Ok(processed)
    }

    fn notify_queue(&mut self, queue: u32) {
        if queue != 0
            || self.status & STATUS_DRIVER_OK == 0
            || self.status & STATUS_FAILED != 0
            || !self.queue.ready
        {
            return;
        }
        match self.process_available() {
            Ok(true) => {
                let flags = self.read_u16(self.queue.driver_addr).unwrap_or(0);
                if flags & VIRTQ_AVAIL_F_NO_INTERRUPT == 0 {
                    self.interrupt_status |= INTERRUPT_USED_BUFFER;
                }
                self.update_interrupt_line();
            }
            Ok(false) => {}
            Err(_) => self.fail_device(),
        }
    }

    fn queue_configuration_writable(&self) -> bool {
        self.queue_sel == 0 && !self.queue.ready
    }
}

impl MemDevice for VirtioBlock {
    fn read(&mut self, addr: u64, size: usize) -> Result<u64, Exception> {
        if size != 4 || addr & 3 != 0 {
            return Err(Exception::LoadAccessFault(addr));
        }
        let offset = self.offset(addr).ok_or(Exception::LoadAccessFault(addr))?;
        let value = match offset {
            REG_MAGIC_VALUE => MMIO_MAGIC_VALUE,
            REG_VERSION => MMIO_VERSION,
            REG_DEVICE_ID => BLOCK_DEVICE_ID,
            REG_VENDOR_ID => self.config.vendor_id,
            REG_DEVICE_FEATURES => {
                Self::selected_feature_word(self.device_features, self.device_features_sel)
            }
            REG_QUEUE_NUM_MAX => {
                if self.queue_sel == 0 {
                    u32::from(self.config.queue_size)
                } else {
                    0
                }
            }
            REG_QUEUE_READY => (self.queue_sel == 0 && self.queue.ready) as u32,
            REG_INTERRUPT_STATUS => self.interrupt_status,
            REG_STATUS => u32::from(self.status),
            REG_CONFIG_GENERATION => self.config_generation,
            REG_BLOCK_CAPACITY_LOW => self.capacity_sectors as u32,
            REG_BLOCK_CAPACITY_HIGH => (self.capacity_sectors >> 32) as u32,
            _ => 0,
        };
        Ok(value.into())
    }

    fn write(&mut self, addr: u64, value: u64, size: usize) -> Result<(), Exception> {
        if size != 4 || addr & 3 != 0 {
            return Err(Exception::StoreAMOAccessFault(addr));
        }
        let offset = self
            .offset(addr)
            .ok_or(Exception::StoreAMOAccessFault(addr))?;
        let value = value as u32;
        match offset {
            REG_DEVICE_FEATURES_SEL => self.device_features_sel = value,
            REG_DRIVER_FEATURES => self.update_driver_features(value),
            REG_DRIVER_FEATURES_SEL => self.driver_features_sel = value,
            REG_QUEUE_SEL => self.queue_sel = value,
            REG_QUEUE_NUM if self.queue_configuration_writable() => {
                if value == 0
                    || value > u32::from(self.config.queue_size)
                    || !value.is_power_of_two()
                {
                    return Err(Exception::StoreAMOAccessFault(addr));
                }
                self.queue.num = value as u16;
            }
            REG_QUEUE_READY if self.queue_sel == 0 => match value {
                0 => self.queue.ready = false,
                1 if self.queue.num != 0
                    && self.queue.descriptor_addr & 0xf == 0
                    && self.queue.driver_addr & 1 == 0
                    && self.queue.device_addr & 3 == 0 =>
                {
                    self.queue.ready = true
                }
                _ => return Err(Exception::StoreAMOAccessFault(addr)),
            },
            REG_QUEUE_NOTIFY => self.notify_queue(value),
            REG_INTERRUPT_ACK => {
                self.interrupt_status &= !(value & 0x3);
                self.update_interrupt_line();
            }
            REG_STATUS => self.write_status(value as u8),
            REG_QUEUE_DESC_LOW if self.queue_configuration_writable() => {
                Self::set_low(&mut self.queue.descriptor_addr, value)
            }
            REG_QUEUE_DESC_HIGH if self.queue_configuration_writable() => {
                Self::set_high(&mut self.queue.descriptor_addr, value)
            }
            REG_QUEUE_DRIVER_LOW if self.queue_configuration_writable() => {
                Self::set_low(&mut self.queue.driver_addr, value)
            }
            REG_QUEUE_DRIVER_HIGH if self.queue_configuration_writable() => {
                Self::set_high(&mut self.queue.driver_addr, value)
            }
            REG_QUEUE_DEVICE_LOW if self.queue_configuration_writable() => {
                Self::set_low(&mut self.queue.device_addr, value)
            }
            REG_QUEUE_DEVICE_HIGH if self.queue_configuration_writable() => {
                Self::set_high(&mut self.queue.device_addr, value)
            }
            _ => {}
        }
        Ok(())
    }

    fn reset(&mut self) {
        self.reset_transport();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u64 = 0x1000_1000;
    const RAM_BASE: u64 = 0x8000_0000;
    const DESC: u64 = RAM_BASE + 0x1000;
    const AVAIL: u64 = RAM_BASE + 0x2000;
    const USED: u64 = RAM_BASE + 0x3000;
    const HEADER: u64 = RAM_BASE + 0x4000;
    const DATA: u64 = RAM_BASE + 0x5000;
    const REQUEST_STATUS: u64 = RAM_BASE + 0x6000;

    fn write_desc(dram: &Shared<Dram>, index: u64, descriptor: Descriptor) {
        let addr = DESC + index * 16;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&descriptor.addr.to_le_bytes());
        bytes.extend_from_slice(&descriptor.len.to_le_bytes());
        bytes.extend_from_slice(&descriptor.flags.to_le_bytes());
        bytes.extend_from_slice(&descriptor.next.to_le_bytes());
        dram.borrow_mut().write_bytes(addr, &bytes).unwrap();
    }

    fn device() -> (VirtioBlock, Shared<Dram>, MemoryBlockBackend, InterruptLine) {
        let dram = Shared::new(Dram::with_layout(RAM_BASE, 0x10_000));
        let disk = MemoryBlockBackend::new(vec![0x5a; 2 * BLOCK_SECTOR_SIZE as usize]);
        let line = InterruptLine::new();
        let device = VirtioBlock::new(
            VirtioBlockConfig {
                base: BASE,
                size: 0x1000,
                queue_size: 8,
                vendor_id: 0x554d_4551,
            },
            Box::new(dram.clone()),
            Box::new(disk.clone()),
            line.clone(),
        )
        .unwrap();
        (device, dram, disk, line)
    }

    fn initialize(device: &mut VirtioBlock) {
        device
            .write(BASE + REG_STATUS, STATUS_ACKNOWLEDGE.into(), 4)
            .unwrap();
        device
            .write(
                BASE + REG_STATUS,
                u64::from(STATUS_ACKNOWLEDGE | STATUS_DRIVER),
                4,
            )
            .unwrap();
        device.write(BASE + REG_DEVICE_FEATURES_SEL, 1, 4).unwrap();
        assert_eq!(device.read(BASE + REG_DEVICE_FEATURES, 4), Ok(1));
        device.write(BASE + REG_DRIVER_FEATURES_SEL, 1, 4).unwrap();
        device.write(BASE + REG_DRIVER_FEATURES, 1, 4).unwrap();
        device
            .write(
                BASE + REG_STATUS,
                u64::from(STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK),
                4,
            )
            .unwrap();
        device.write(BASE + REG_QUEUE_NUM, 8, 4).unwrap();
        device.write(BASE + REG_QUEUE_DESC_LOW, DESC, 4).unwrap();
        device.write(BASE + REG_QUEUE_DRIVER_LOW, AVAIL, 4).unwrap();
        device.write(BASE + REG_QUEUE_DEVICE_LOW, USED, 4).unwrap();
        device.write(BASE + REG_QUEUE_READY, 1, 4).unwrap();
        device
            .write(
                BASE + REG_STATUS,
                u64::from(
                    STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK,
                ),
                4,
            )
            .unwrap();
    }

    fn queue_read_request(dram: &Shared<Dram>, sector: u64, len: u32) {
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&VIRTIO_BLK_T_IN.to_le_bytes());
        header[8..16].copy_from_slice(&sector.to_le_bytes());
        dram.borrow_mut().write_bytes(HEADER, &header).unwrap();
        write_desc(
            dram,
            0,
            Descriptor {
                addr: HEADER,
                len: 16,
                flags: VIRTQ_DESC_F_NEXT,
                next: 1,
            },
        );
        write_desc(
            dram,
            1,
            Descriptor {
                addr: DATA,
                len,
                flags: VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE,
                next: 2,
            },
        );
        write_desc(
            dram,
            2,
            Descriptor {
                addr: REQUEST_STATUS,
                len: 1,
                flags: VIRTQ_DESC_F_WRITE,
                next: 0,
            },
        );
        dram.borrow_mut()
            .write_bytes(AVAIL + 2, &1u16.to_le_bytes())
            .unwrap();
        dram.borrow_mut()
            .write_bytes(AVAIL + 4, &0u16.to_le_bytes())
            .unwrap();
    }

    #[test]
    fn modern_features_queue_completion_and_interrupt_follow_the_spec() {
        let (mut device, dram, _, line) = device();
        initialize(&mut device);
        queue_read_request(&dram, 0, BLOCK_SECTOR_SIZE as u32);

        device.write(BASE + REG_QUEUE_NOTIFY, 0, 4).unwrap();

        let mut data = vec![0; BLOCK_SECTOR_SIZE as usize];
        dram.borrow().read_bytes(DATA, &mut data).unwrap();
        assert!(data.iter().all(|byte| *byte == 0x5a));
        assert_eq!(dram.borrow_mut().read(REQUEST_STATUS, 1), Ok(0));
        assert_eq!(dram.borrow_mut().read(USED + 2, 2), Ok(1));
        assert!(line.is_asserted());
        device
            .write(BASE + REG_INTERRUPT_ACK, INTERRUPT_USED_BUFFER.into(), 4)
            .unwrap();
        assert!(!line.is_asserted());
    }

    #[test]
    fn queue_notification_uses_the_full_32_bit_queue_index() {
        let (mut device, dram, _, line) = device();
        initialize(&mut device);
        queue_read_request(&dram, 0, BLOCK_SECTOR_SIZE as u32);

        device.write(BASE + REG_QUEUE_NOTIFY, 0x1_0000, 4).unwrap();

        assert_eq!(dram.borrow_mut().read(USED + 2, 2), Ok(0));
        assert!(!line.is_asserted());
    }

    #[test]
    fn request_is_range_checked_before_dma_starts() {
        let (mut device, dram, _, _) = device();
        initialize(&mut device);
        queue_read_request(&dram, 1, BLOCK_SECTOR_SIZE as u32 + 1);

        device.write(BASE + REG_QUEUE_NOTIFY, 0, 4).unwrap();

        let mut data = vec![0; BLOCK_SECTOR_SIZE as usize + 1];
        dram.borrow().read_bytes(DATA, &mut data).unwrap();
        assert!(data.iter().all(|byte| *byte == 0));
        assert_eq!(dram.borrow_mut().read(REQUEST_STATUS, 1), Ok(1));
    }

    #[test]
    fn memory_backend_replacement_keeps_the_advertised_capacity_stable() {
        let disk = MemoryBlockBackend::new(vec![0; 512]);

        assert_eq!(disk.replace(vec![1; 512]), Ok(()));
        assert_eq!(
            disk.replace(vec![2; 513]),
            Err(BlockError::CapacityMismatch)
        );
        assert_eq!(&*disk.bytes(), &[1; 512]);
    }

    #[test]
    fn reset_preserves_media_but_clears_transport_state() {
        let (mut device, _, disk, line) = device();
        initialize(&mut device);
        device.interrupt_status = INTERRUPT_USED_BUFFER;
        device.update_interrupt_line();
        device.write(BASE + REG_STATUS, 0, 4).unwrap();

        assert_eq!(device.read(BASE + REG_STATUS, 4), Ok(0));
        assert_eq!(device.read(BASE + REG_QUEUE_READY, 4), Ok(0));
        assert!(!line.is_asserted());
        assert_eq!(disk.bytes().len(), 2 * BLOCK_SECTOR_SIZE as usize);
    }

    #[test]
    fn queue_size_and_mmio_accesses_are_validated() {
        let (mut device, _, _, _) = device();
        for value in [0, 3, 16] {
            assert_eq!(
                device.write(BASE + REG_QUEUE_NUM, value, 4),
                Err(Exception::StoreAMOAccessFault(BASE + REG_QUEUE_NUM))
            );
        }
        assert_eq!(
            device.read(BASE + 2, 4),
            Err(Exception::LoadAccessFault(BASE + 2))
        );

        let dram = Shared::new(Dram::with_layout(RAM_BASE, 0x1000));
        assert!(matches!(
            VirtioBlock::new(
                VirtioBlockConfig {
                    base: BASE + 1,
                    size: 0x1000,
                    queue_size: 8,
                    vendor_id: 0,
                },
                Box::new(dram),
                Box::new(MemoryBlockBackend::new(Vec::new())),
                InterruptLine::new(),
            ),
            Err(VirtioBlockError::MisalignedMmioWindow)
        ));
    }
}
