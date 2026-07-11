#![allow(dead_code)]

//! 集成测试共用的机器、设备和 fixture 工具。
//!
//! [`TestBus`] 把 RAM、UART、简化 PLIC 和 virtio-blk 放进同一地址空间；[`TestMachine`]
//! 则通过正式 [`Machine`] 运行该地址空间，并保留可观察的共享状态。这里的设备模型只服务测试，不代表正式 CLI 的平台组成。

use arvsim::bus::MemDevice;
use arvsim::cfg;
use arvsim::cpu::Xv6Accelerator;
use arvsim::machine::Machine;
use arvsim::trap::Exception;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::rc::Rc;

const UART_LSR_TX_IDLE: u64 = 1 << 5;
const UART_LSR_RX_READY: u64 = 1;
const DEFAULT_SMOKE_RAM_SIZE: usize = 1024 * 1024;
const PLIC_BASE: u64 = 0x0c00_0000;
const PLIC_SIZE: u64 = 0x0400_0000;
const VIRTIO_BASE: u64 = 0x1000_1000;
const VIRTIO_SIZE: u64 = 0x1000;
const VIRTIO_QUEUE_SIZE: u16 = 8;
const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;
const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_T_OUT: u32 = 1;
const XV6_BUF_DATA_OFFSET: u64 = 88;
const UART0_IRQ: u32 = 10;
const SUPERVISOR_EXTERNAL_INTERRUPT: u64 = (1 << 63) | 9;

/// MMIO 日志中的访问方向。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MmioAccessKind {
    Read,
    Write,
}

/// 一次未被 RAM 路径吸收的 MMIO 访问记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MmioAccess {
    pub kind: MmioAccessKind,
    pub addr: u64,
    pub value: u64,
    pub size: usize,
}

/// 测试设备共享状态。
///
/// CPU 通过 `Rc<RefCell<_>>` 与测试断言共享这份状态，从而可在执行过程中注入 UART 输入并读取输出。
#[derive(Debug)]
pub struct TestBusState {
    ram: Vec<u8>,
    uart_output: Vec<u8>,
    uart_input: Vec<u8>,
    uart_input_pos: usize,
    plic_words: BTreeMap<usize, u32>,
    plic_pending: u32,
    uart_tx_busy_addr: Option<u64>,
    disk: Vec<u8>,
    virtio: VirtioState,
    mmio_log: Vec<MmioAccess>,
}

#[derive(Debug, Default)]
struct VirtioState {
    status: u32,
    driver_features: u32,
    queue_sel: u32,
    queue_num: u32,
    queue_ready: u32,
    desc_addr: u64,
    avail_addr: u64,
    used_addr: u64,
    interrupt_status: u32,
    last_avail_idx: u16,
}

#[derive(Debug, Clone, Copy)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

impl TestBusState {
    fn new(ram_size: usize) -> Self {
        Self {
            ram: vec![0; ram_size],
            uart_output: Vec::new(),
            uart_input: Vec::new(),
            uart_input_pos: 0,
            plic_words: BTreeMap::new(),
            plic_pending: 0,
            uart_tx_busy_addr: None,
            disk: Vec::new(),
            virtio: VirtioState::default(),
            mmio_log: Vec::new(),
        }
    }

    /// 将 flat binary 放到测试 RAM 起点；fixture 超过 RAM 时立即失败。
    pub fn load_at_dram_base(&mut self, bytes: &[u8]) {
        assert!(
            bytes.len() <= self.ram.len(),
            "fixture is {} bytes but test RAM is only {} bytes",
            bytes.len(),
            self.ram.len()
        );
        self.ram[..bytes.len()].copy_from_slice(bytes);
    }

    /// 替换 virtio-blk 后端使用的磁盘镜像。
    pub fn load_disk_image(&mut self, bytes: Vec<u8>) {
        self.disk = bytes;
    }

    /// 设置 xv6 UART `tx_busy` 变量的物理地址，以兼容其发送状态轮询。
    pub fn set_uart_tx_busy_addr(&mut self, addr: Option<u64>) {
        self.uart_tx_busy_addr = addr;
    }

    /// 追加 guest 可读取的 UART 输入，并置位对应 PLIC pending 位。
    pub fn queue_uart_input(&mut self, bytes: &[u8]) {
        self.uart_input.extend_from_slice(bytes);
        if !bytes.is_empty() {
            self.plic_pending |= 1 << UART0_IRQ;
        }
    }

    /// 以有损 UTF-8 返回累计 UART 输出，便于按文本等待启动阶段。
    pub fn uart_output_string(&self) -> String {
        String::from_utf8_lossy(&self.uart_output).into_owned()
    }

    /// 返回从测试开始累计的 MMIO 访问日志。
    pub fn mmio_log(&self) -> &[MmioAccess] {
        &self.mmio_log
    }

    fn ram_offset(&self, addr: u64, size: usize) -> Result<usize, Exception> {
        let offset = addr
            .checked_sub(cfg::DRAM_BASE)
            .ok_or(Exception::LoadAccessFault(addr))? as usize;
        let end = offset
            .checked_add(size)
            .ok_or(Exception::LoadAccessFault(addr))?;
        if end <= self.ram.len() {
            Ok(offset)
        } else {
            Err(Exception::LoadAccessFault(addr))
        }
    }

    fn read_ram(&self, addr: u64, size: usize) -> Result<u64, Exception> {
        let offset = self.ram_offset(addr, size)?;
        let mut value = 0u64;
        for i in 0..size {
            value |= (self.ram[offset + i] as u64) << (i * 8);
        }
        Ok(value)
    }

    fn write_ram(&mut self, addr: u64, value: u64, size: usize) -> Result<(), Exception> {
        let offset = self.ram_offset(addr, size)?;
        for i in 0..size {
            self.ram[offset + i] = ((value >> (i * 8)) & 0xff) as u8;
        }
        Ok(())
    }

    fn copy_from_disk(
        &mut self,
        disk_offset: usize,
        addr: u64,
        len: usize,
    ) -> Result<(), Exception> {
        if disk_offset + len > self.disk.len() {
            return Err(Exception::LoadAccessFault(addr));
        }
        let ram_offset = self.ram_offset(addr, len)?;
        self.ram[ram_offset..ram_offset + len]
            .copy_from_slice(&self.disk[disk_offset..disk_offset + len]);
        Ok(())
    }

    fn copy_to_disk(&mut self, addr: u64, disk_offset: usize, len: usize) -> Result<(), Exception> {
        let ram_offset = self.ram_offset(addr, len)?;
        if disk_offset + len > self.disk.len() {
            self.disk.resize(disk_offset + len, 0);
        }
        self.disk[disk_offset..disk_offset + len]
            .copy_from_slice(&self.ram[ram_offset..ram_offset + len]);
        Ok(())
    }
}

/// 实现 [`MemDevice`] 的测试平台总线。
///
/// clone 只复制共享状态句柄，不复制 RAM 或设备状态。
#[derive(Clone)]
pub struct TestBus {
    state: Rc<RefCell<TestBusState>>,
}

impl TestBus {
    /// 创建指定 RAM 容量的空测试平台。
    pub fn new(ram_size: usize) -> Self {
        Self {
            state: Rc::new(RefCell::new(TestBusState::new(ram_size))),
        }
    }

    /// 创建用于短 RV64 冒烟测试的 1 MiB RAM 平台。
    pub fn rv64_smoke() -> Self {
        Self::new(DEFAULT_SMOKE_RAM_SIZE)
    }

    /// 创建与默认 xv6 物理内存容量一致的平台。
    pub fn xv6_sized() -> Self {
        Self::new(cfg::DRAM_SIZE)
    }

    /// 获取共享状态句柄，供测试在 CPU 运行期间观察或注入设备状态。
    pub fn state(&self) -> Rc<RefCell<TestBusState>> {
        Rc::clone(&self.state)
    }

    pub fn load_flat_binary<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn Error>> {
        let bytes = fs::read(path)?;
        self.state.borrow_mut().load_at_dram_base(&bytes);
        Ok(())
    }

    pub fn load_disk_image<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn Error>> {
        let bytes = fs::read(path)?;
        self.state.borrow_mut().load_disk_image(bytes);
        Ok(())
    }

    fn dram_offset(state: &TestBusState, addr: u64, size: usize) -> Option<usize> {
        let offset = addr.checked_sub(cfg::DRAM_BASE)? as usize;
        let end = offset.checked_add(size)?;
        (end <= state.ram.len()).then_some(offset)
    }

    fn read_uart(state: &mut TestBusState, addr: u64, size: usize) -> Result<u64, Exception> {
        let offset = addr - cfg::UART_BASE;
        state.mmio_log.push(MmioAccess {
            kind: MmioAccessKind::Read,
            addr,
            value: 0,
            size,
        });

        match offset {
            0x00 => {
                if state.uart_input_pos < state.uart_input.len() {
                    let byte = state.uart_input[state.uart_input_pos];
                    state.uart_input_pos += 1;
                    Ok(byte as u64)
                } else {
                    Ok(0)
                }
            }
            0x02 => Ok(0),
            0x05 => {
                // LSR 始终报告发送空闲；仅在输入队列还有字节时报告接收就绪。
                let rx = (state.uart_input_pos < state.uart_input.len()) as u64;
                Ok(UART_LSR_TX_IDLE | (rx * UART_LSR_RX_READY))
            }
            _ => Ok(0),
        }
    }

    fn write_uart(
        state: &mut TestBusState,
        addr: u64,
        value: u32,
        size: usize,
    ) -> Result<(), Exception> {
        let offset = addr - cfg::UART_BASE;
        state.mmio_log.push(MmioAccess {
            kind: MmioAccessKind::Write,
            addr,
            value: value as u64,
            size,
        });

        match offset {
            0x00 => state.uart_output.push((value & 0xff) as u8),
            0x01..=0x03 => {}
            _ => {}
        }
        Ok(())
    }

    fn plic_offset(addr: u64, size: usize) -> Option<usize> {
        let offset = addr.checked_sub(PLIC_BASE)? as usize;
        let end = offset.checked_add(size)?;
        (end <= PLIC_SIZE as usize).then_some(offset)
    }

    fn read_plic(state: &mut TestBusState, addr: u64, size: usize) -> Result<u64, Exception> {
        state.mmio_log.push(MmioAccess {
            kind: MmioAccessKind::Read,
            addr,
            value: 0,
            size,
        });
        let offset = Self::plic_offset(addr, size).ok_or(Exception::LoadAccessFault(addr))?;
        let word = match offset {
            0x1000 => state.plic_pending,
            0x201004 => Self::claim_plic(state, true),
            _ => *state.plic_words.get(&(offset / 4)).unwrap_or(&0),
        };
        Ok((word >> ((offset & 0x3) * 8)) as u64 & ((1u64 << (size * 8)) - 1))
    }

    fn write_plic(
        state: &mut TestBusState,
        addr: u64,
        value: u32,
        size: usize,
    ) -> Result<(), Exception> {
        state.mmio_log.push(MmioAccess {
            kind: MmioAccessKind::Write,
            addr,
            value: value as u64,
            size,
        });
        let offset = Self::plic_offset(addr, size).ok_or(Exception::StoreAMOAccessFault(addr))?;
        if offset == 0x201004 {
            return Ok(());
        }
        let shift = (offset & 0x3) * 8;
        let mask = if size == 4 {
            u32::MAX
        } else {
            ((1u32 << (size * 8)) - 1) << shift
        };
        let word = state.plic_words.entry(offset / 4).or_default();
        *word = (*word & !mask) | ((value << shift) & mask);
        Ok(())
    }

    fn claim_plic(state: &mut TestBusState, clear: bool) -> u32 {
        // 当前测试控制器只建模 UART0；增加其他 source 时再引入优先级遍历。
        let irq = UART0_IRQ;
        let bit = 1 << irq;
        let priority = *state.plic_words.get(&(irq as usize)).unwrap_or(&0);
        let senable = *state.plic_words.get(&(0x2080 / 4)).unwrap_or(&0);
        let threshold = *state.plic_words.get(&(0x201000 / 4)).unwrap_or(&0);
        if state.plic_pending & bit != 0 && senable & bit != 0 && priority > threshold {
            // 查询 pending_interrupt 时不能提前清除；只有 guest 读取 claim 才消费中断。
            if clear {
                state.plic_pending &= !bit;
            }
            return irq;
        }
        0
    }

    fn virtio_offset(addr: u64, size: usize) -> Option<usize> {
        let offset = addr.checked_sub(VIRTIO_BASE)? as usize;
        let end = offset.checked_add(size)?;
        (end <= VIRTIO_SIZE as usize).then_some(offset)
    }

    fn read_virtio(state: &mut TestBusState, addr: u64, size: usize) -> Result<u64, Exception> {
        let offset = Self::virtio_offset(addr, size).ok_or(Exception::LoadAccessFault(addr))?;
        state.mmio_log.push(MmioAccess {
            kind: MmioAccessKind::Read,
            addr,
            value: 0,
            size,
        });
        let value = match offset {
            0x000 => 0x7472_6976,
            0x004 => 2,
            0x008 => 2,
            0x00c => 0x554d_4551,
            0x010 => 0,
            0x030 => state.virtio.queue_sel,
            0x034 => VIRTIO_QUEUE_SIZE as u32,
            0x038 => state.virtio.queue_num,
            0x044 => state.virtio.queue_ready,
            0x060 => state.virtio.interrupt_status,
            0x070 => state.virtio.status,
            _ => 0,
        };
        Ok(value as u64)
    }

    fn write_virtio(
        state: &mut TestBusState,
        addr: u64,
        value: u32,
        size: usize,
    ) -> Result<(), Exception> {
        let offset = Self::virtio_offset(addr, size).ok_or(Exception::StoreAMOAccessFault(addr))?;
        state.mmio_log.push(MmioAccess {
            kind: MmioAccessKind::Write,
            addr,
            value: value as u64,
            size,
        });
        match offset {
            0x020 => state.virtio.driver_features = value,
            0x030 => state.virtio.queue_sel = value,
            0x038 => state.virtio.queue_num = value,
            0x044 => state.virtio.queue_ready = value,
            0x050 => Self::process_virtio_queue(state)?,
            0x064 => state.virtio.interrupt_status &= !value,
            0x070 => {
                if value == 0 {
                    state.virtio = VirtioState::default();
                } else {
                    state.virtio.status = value;
                }
            }
            0x080 => {
                state.virtio.desc_addr = (state.virtio.desc_addr & !0xffff_ffff) | value as u64
            }
            0x084 => {
                state.virtio.desc_addr =
                    (state.virtio.desc_addr & 0xffff_ffff) | ((value as u64) << 32)
            }
            0x090 => {
                state.virtio.avail_addr = (state.virtio.avail_addr & !0xffff_ffff) | value as u64
            }
            0x094 => {
                state.virtio.avail_addr =
                    (state.virtio.avail_addr & 0xffff_ffff) | ((value as u64) << 32)
            }
            0x0a0 => {
                state.virtio.used_addr = (state.virtio.used_addr & !0xffff_ffff) | value as u64
            }
            0x0a4 => {
                state.virtio.used_addr =
                    (state.virtio.used_addr & 0xffff_ffff) | ((value as u64) << 32)
            }
            _ => {}
        }
        Ok(())
    }

    fn process_virtio_queue(state: &mut TestBusState) -> Result<(), Exception> {
        let queue_num = if state.virtio.queue_num == 0 {
            VIRTIO_QUEUE_SIZE
        } else {
            state.virtio.queue_num as u16
        };
        let avail_idx = state.read_ram(state.virtio.avail_addr + 2, 2)? as u16;
        while state.virtio.last_avail_idx != avail_idx {
            let slot = state.virtio.last_avail_idx % queue_num;
            let head = state.read_ram(state.virtio.avail_addr + 4 + slot as u64 * 2, 2)? as u16;
            Self::process_virtio_request(state, head, queue_num)?;
            state.virtio.last_avail_idx = state.virtio.last_avail_idx.wrapping_add(1);
        }
        Ok(())
    }

    fn process_virtio_request(
        state: &mut TestBusState,
        head: u16,
        queue_num: u16,
    ) -> Result<(), Exception> {
        let req = Self::read_desc(state, head)?;
        if req.flags & VIRTQ_DESC_F_NEXT == 0 {
            return Err(Exception::LoadAccessFault(req.addr));
        }
        let data = Self::read_desc(state, req.next)?;
        if data.flags & VIRTQ_DESC_F_NEXT == 0 {
            return Err(Exception::LoadAccessFault(data.addr));
        }
        let status = Self::read_desc(state, data.next)?;
        let request_type = state.read_ram(req.addr, 4)? as u32;
        let sector = state.read_ram(req.addr + 8, 8)?;
        let disk_offset = (sector as usize) * 512;
        let data_len = data.len as usize;

        // 数据描述符方向由请求类型决定：IN 从磁盘复制到 guest，OUT 反向复制。
        match request_type {
            VIRTIO_BLK_T_IN => state.copy_from_disk(disk_offset, data.addr, data_len)?,
            VIRTIO_BLK_T_OUT => state.copy_to_disk(data.addr, disk_offset, data_len)?,
            _ => return Err(Exception::LoadAccessFault(req.addr)),
        }

        state.write_ram(status.addr, 0, 1)?;
        let used_idx = state.read_ram(state.virtio.used_addr + 2, 2)? as u16;
        let used_slot = used_idx % queue_num;
        let elem = state.virtio.used_addr + 4 + used_slot as u64 * 8;
        state.write_ram(elem, head as u64, 4)?;
        state.write_ram(elem + 4, data_len as u64, 4)?;
        state.write_ram(
            state.virtio.used_addr + 2,
            used_idx.wrapping_add(1) as u64,
            2,
        )?;
        state.write_ram(data.addr.wrapping_sub(XV6_BUF_DATA_OFFSET) + 4, 0, 4)?;
        state.virtio.interrupt_status |= 1;
        Ok(())
    }

    fn read_desc(state: &TestBusState, index: u16) -> Result<VirtqDesc, Exception> {
        let addr = state.virtio.desc_addr + index as u64 * 16;
        Ok(VirtqDesc {
            addr: state.read_ram(addr, 8)?,
            len: state.read_ram(addr + 8, 4)? as u32,
            flags: state.read_ram(addr + 12, 2)? as u16,
            next: state.read_ram(addr + 14, 2)? as u16,
        })
    }
}

impl MemDevice for TestBus {
    fn read(&mut self, addr: u64, size: usize) -> Result<u64, Exception> {
        let mut state = self.state.borrow_mut();
        if state.uart_tx_busy_addr == Some(addr) && size == 4 {
            return Ok(0);
        }
        if let Some(offset) = Self::dram_offset(&state, addr, size) {
            let mut value = 0u64;
            for i in 0..size {
                value |= (state.ram[offset + i] as u64) << (i * 8);
            }
            return Ok(value);
        }

        if (cfg::UART_BASE..cfg::UART_BASE + 0x100).contains(&addr) {
            return Self::read_uart(&mut state, addr, size);
        }
        if Self::virtio_offset(addr, size).is_some() {
            return Self::read_virtio(&mut state, addr, size);
        }
        if Self::plic_offset(addr, size).is_some() {
            return Self::read_plic(&mut state, addr, size);
        }

        state.mmio_log.push(MmioAccess {
            kind: MmioAccessKind::Read,
            addr,
            value: 0,
            size,
        });
        Err(Exception::LoadAccessFault(addr))
    }

    fn write(&mut self, addr: u64, value: u32, size: usize) -> Result<(), Exception> {
        let mut state = self.state.borrow_mut();
        if state.uart_tx_busy_addr == Some(addr) && size == 4 {
            return Ok(());
        }
        if let Some(offset) = Self::dram_offset(&state, addr, size) {
            for i in 0..size {
                state.ram[offset + i] = ((value >> (i * 8)) & 0xff) as u8;
            }
            return Ok(());
        }

        if (cfg::UART_BASE..cfg::UART_BASE + 0x100).contains(&addr) {
            return Self::write_uart(&mut state, addr, value, size);
        }
        if Self::virtio_offset(addr, size).is_some() {
            return Self::write_virtio(&mut state, addr, value, size);
        }
        if Self::plic_offset(addr, size).is_some() {
            return Self::write_plic(&mut state, addr, value, size);
        }

        state.mmio_log.push(MmioAccess {
            kind: MmioAccessKind::Write,
            addr,
            value: value as u64,
            size,
        });
        Err(Exception::StoreAMOAccessFault(addr))
    }

    fn pending_interrupt(&mut self) -> Option<u64> {
        let mut state = self.state.borrow_mut();
        (Self::claim_plic(&mut state, false) != 0).then_some(SUPERVISOR_EXTERNAL_INTERRUPT)
    }
}

/// CPU 与可观察测试总线状态的组合。
pub struct TestMachine {
    pub machine: Machine,
    pub state: Rc<RefCell<TestBusState>>,
}

impl Deref for TestMachine {
    type Target = Machine;

    fn deref(&self) -> &Self::Target {
        &self.machine
    }
}

impl DerefMut for TestMachine {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.machine
    }
}

impl TestMachine {
    /// 用已有测试总线创建 CPU，并保留同一状态的观察句柄。
    pub fn from_bus(bus: TestBus) -> Self {
        let state = bus.state();
        Self {
            machine: Machine::from_address_space(Box::new(bus), cfg::CPU_START_ADDR, cfg::DRAM_END),
            state,
        }
    }

    /// 装载 flat binary 并创建指定 RAM 容量的机器。
    pub fn with_flat_binary<P: AsRef<Path>>(
        path: P,
        ram_size: usize,
    ) -> Result<Self, Box<dyn Error>> {
        let bus = TestBus::new(ram_size);
        bus.load_flat_binary(path)?;
        Ok(Self::from_bus(bus))
    }

    /// 精确执行指定步数，首个未处理异常会立即结束运行。
    pub fn run_steps(&mut self, max_steps: usize) -> Result<(), Exception> {
        for _ in 0..max_steps {
            self.cpu.step()?;
        }
        Ok(())
    }

    /// 将字符串作为原始字节追加到 UART 输入队列。
    pub fn queue_uart_input(&mut self, input: &str) {
        self.state.borrow_mut().queue_uart_input(input.as_bytes());
    }

    /// 运行到 UART 输出包含目标文本或耗尽步数预算。
    pub fn run_until_uart_contains(
        &mut self,
        needle: &str,
        max_steps: usize,
    ) -> Result<bool, Exception> {
        for _ in 0..max_steps {
            self.cpu.step()?;
            if self.state.borrow().uart_output_string().contains(needle) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// 等待 UART 文本，失败时附带完整输出作为诊断上下文。
    pub fn require_uart_contains(
        &mut self,
        label: &str,
        needle: &str,
        max_steps: usize,
    ) -> Result<(), Box<dyn Error>> {
        match self.run_until_uart_contains(needle, max_steps) {
            Ok(true) => Ok(()),
            Ok(false) => Err(format!(
                "timed out after {max_steps} steps while waiting for {label}: {needle:?}\nUART output:\n{}",
                self.state.borrow().uart_output_string()
            )
            .into()),
            Err(e) => Err(format!(
                "CPU exception while waiting for {label}: {e:?}\nUART output:\n{}",
                self.state.borrow().uart_output_string()
            )
            .into()),
        }
    }

    /// 确认 UART 输出没有任何已知失败标记。
    pub fn require_uart_lacks(&self, forbidden: &[&str]) -> Result<(), Box<dyn Error>> {
        let output = self.state.borrow().uart_output_string();
        for needle in forbidden {
            if output.contains(needle) {
                return Err(
                    format!("unexpected UART output {needle:?}\nUART output:\n{output}").into(),
                );
            }
        }
        Ok(())
    }
}

/// 仓库根目录。
pub fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 测试生成物和 xv6 fixture 的统一输出目录。
pub fn testbench_target_dir() -> PathBuf {
    project_root().join("target/testbench")
}

/// 用外部 RISC-V 工具链构建一个从默认 DRAM 基址开始的 flat binary。
pub fn build_flat_asm(name: &str, asm: &str) -> Result<PathBuf, Box<dyn Error>> {
    require_tool("riscv64-elf-gcc")?;
    require_tool("riscv64-elf-objcopy")?;

    let out_dir = testbench_target_dir().join("generated");
    fs::create_dir_all(&out_dir)?;

    let stem = format!("{}-{}", name, std::process::id());
    let asm_path = out_dir.join(format!("{stem}.S"));
    let linker_path = out_dir.join(format!("{stem}.ld"));
    let elf_path = out_dir.join(format!("{stem}.elf"));
    let bin_path = out_dir.join(format!("{stem}.bin"));

    fs::write(&asm_path, asm)?;
    fs::write(
        &linker_path,
        "OUTPUT_ARCH(riscv)\n\
         ENTRY(_start)\n\
         SECTIONS\n\
         {\n\
           . = 0x80000000;\n\
           .text : { *(.text .text.*) }\n\
           .rodata : { *(.rodata .rodata.*) }\n\
           .data : { *(.data .data.*) }\n\
           .bss : { *(.bss .bss.* COMMON) }\n\
         }\n",
    )?;

    run(Command::new("riscv64-elf-gcc").args([
        "-nostdlib",
        "-nostartfiles",
        "-ffreestanding",
        "-march=rv64i_zicsr",
        "-mabi=lp64",
        "-Wl,--no-relax",
        "-T",
        linker_path.to_str().unwrap(),
        "-o",
        elf_path.to_str().unwrap(),
        asm_path.to_str().unwrap(),
    ]))?;

    run(Command::new("riscv64-elf-objcopy").args([
        "-O",
        "binary",
        elf_path.to_str().unwrap(),
        bin_path.to_str().unwrap(),
    ]))?;

    Ok(bin_path)
}

/// xv6 flat kernel 镜像路径。
pub fn xv6_kernel_bin() -> PathBuf {
    testbench_target_dir().join("xv6-riscv/kernel/kernel.bin")
}

/// 带符号表的 xv6 kernel ELF 路径。
pub fn xv6_kernel_elf() -> PathBuf {
    testbench_target_dir().join("xv6-riscv/kernel/kernel")
}

/// xv6 文件系统镜像路径。
pub fn xv6_fs_img() -> PathBuf {
    testbench_target_dir().join("xv6-riscv/fs.img")
}

/// 确认运行 xv6 所需的三个 fixture 文件都已存在。
pub fn require_xv6_fixture() -> Result<(), Box<dyn Error>> {
    let required = [xv6_kernel_elf(), xv6_kernel_bin(), xv6_fs_img()];
    let missing: Vec<_> = required
        .iter()
        .filter(|path| !path.exists())
        .map(|path| path.display().to_string())
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "missing xv6 fixture artifacts:\n  {}\nrun scripts/build_xv6_fixture.sh first",
            missing.join("\n  ")
        )
        .into())
    }
}

/// 装载当前 xv6 fixture，并从 kernel ELF 符号表配置兼容加速器。
pub fn xv6_machine() -> Result<TestMachine, Box<dyn Error>> {
    require_xv6_fixture()?;
    let symbols = xv6_symbols()?;
    let accelerator = Xv6Accelerator {
        mycpu: required_xv6_symbol(&symbols, "mycpu")?,
        holding: required_xv6_symbol(&symbols, "holding")?,
        push_off: required_xv6_symbol(&symbols, "push_off")?,
        acquire: required_xv6_symbol(&symbols, "acquire")?,
        pop_off: required_xv6_symbol(&symbols, "pop_off")?,
        release: required_xv6_symbol(&symbols, "release")?,
        memcmp: required_xv6_symbol(&symbols, "memcmp")?,
        memmove: required_xv6_symbol(&symbols, "memmove")?,
        strncmp: required_xv6_symbol(&symbols, "strncmp")?,
        strncpy: required_xv6_symbol(&symbols, "strncpy")?,
        strlen: required_xv6_symbol(&symbols, "strlen")?,
        uvmunmap: required_xv6_symbol(&symbols, "uvmunmap")?,
        freewalk: required_xv6_symbol(&symbols, "freewalk")?,
        uvmcopy: required_xv6_symbol(&symbols, "uvmcopy")?,
        myproc: required_xv6_symbol(&symbols, "myproc")?,
        wakeup: required_xv6_symbol(&symbols, "wakeup")?,
        cpus: required_xv6_symbol(&symbols, "cpus")?,
        kmem: required_xv6_symbol(&symbols, "kmem")?,
        kernel_end: required_xv6_symbol(&symbols, "end")?,
        proc_start: required_xv6_symbol(&symbols, "proc")?,
        proc_end: required_xv6_symbol(&symbols, "proc")?
            .checked_add(64 * 360)
            .ok_or("xv6 proc table address overflows")?,
        // 用户程序独立于 kernel ELF 链接，因此这个兼容入口无法从内核符号表解析。
        user_exec: Some(0x505c),
    };
    let bus = TestBus::xv6_sized();
    bus.load_flat_binary(xv6_kernel_bin())?;
    bus.load_disk_image(xv6_fs_img())?;
    bus.state()
        .borrow_mut()
        .set_uart_tx_busy_addr(Some(required_xv6_symbol(&symbols, "tx_busy")?));
    let mut machine = TestMachine::from_bus(bus);
    machine.cpu.set_xv6_accelerator(accelerator);
    Ok(machine)
}

fn xv6_symbols() -> Result<BTreeMap<String, u64>, Box<dyn Error>> {
    let output = Command::new("riscv64-elf-nm")
        .arg(xv6_kernel_elf())
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "riscv64-elf-nm failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut symbols = BTreeMap::new();
    for line in stdout.lines() {
        let mut parts = line.split_whitespace();
        let Some(addr) = parts.next() else {
            continue;
        };
        let _kind = parts.next();
        let Some(name) = parts.next() else {
            continue;
        };
        let Ok(addr) = u64::from_str_radix(addr, 16) else {
            continue;
        };
        symbols.insert(name.to_string(), addr);
    }
    Ok(symbols)
}

fn required_xv6_symbol(symbols: &BTreeMap<String, u64>, name: &str) -> Result<u64, Box<dyn Error>> {
    symbols
        .get(name)
        .copied()
        .ok_or_else(|| format!("xv6 kernel is missing required symbol: {name}").into())
}

/// 确认外部命令可从 `PATH` 找到。
pub fn require_tool(tool: &str) -> Result<(), Box<dyn Error>> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {tool} >/dev/null 2>&1"))
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("required tool is missing from PATH: {tool}").into())
    }
}

/// 执行外部命令，并在失败时保留 stdout/stderr 作为错误上下文。
pub fn run(command: &mut Command) -> Result<Output, Box<dyn Error>> {
    let output = command.output()?;
    if output.status.success() {
        return Ok(output);
    }

    Err(format!(
        "command failed with status {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .into())
}
