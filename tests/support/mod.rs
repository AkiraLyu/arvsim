#![allow(dead_code)]

use arvsim::bus::MemDevice;
use arvsim::cfg;
use arvsim::cpu::Cpu;
use arvsim::trap::Exception;
use std::cell::RefCell;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::rc::Rc;

const UART_LSR_TX_IDLE: u64 = 1 << 5;
const UART_LSR_RX_READY: u64 = 1;
const DEFAULT_SMOKE_RAM_SIZE: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MmioAccessKind {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MmioAccess {
    pub kind: MmioAccessKind,
    pub addr: u64,
    pub value: u64,
    pub size: usize,
}

#[derive(Debug)]
pub struct TestBusState {
    ram: Vec<u8>,
    uart_output: Vec<u8>,
    uart_input: Vec<u8>,
    uart_input_pos: usize,
    mmio_log: Vec<MmioAccess>,
}

impl TestBusState {
    fn new(ram_size: usize) -> Self {
        Self {
            ram: vec![0; ram_size],
            uart_output: Vec::new(),
            uart_input: Vec::new(),
            uart_input_pos: 0,
            mmio_log: Vec::new(),
        }
    }

    pub fn load_at_dram_base(&mut self, bytes: &[u8]) {
        assert!(
            bytes.len() <= self.ram.len(),
            "fixture is {} bytes but test RAM is only {} bytes",
            bytes.len(),
            self.ram.len()
        );
        self.ram[..bytes.len()].copy_from_slice(bytes);
    }

    pub fn queue_uart_input(&mut self, bytes: &[u8]) {
        self.uart_input.extend_from_slice(bytes);
    }

    pub fn uart_output_string(&self) -> String {
        String::from_utf8_lossy(&self.uart_output).into_owned()
    }

    pub fn mmio_log(&self) -> &[MmioAccess] {
        &self.mmio_log
    }
}

#[derive(Clone)]
pub struct TestBus {
    state: Rc<RefCell<TestBusState>>,
}

impl TestBus {
    pub fn new(ram_size: usize) -> Self {
        Self {
            state: Rc::new(RefCell::new(TestBusState::new(ram_size))),
        }
    }

    pub fn rv64_smoke() -> Self {
        Self::new(DEFAULT_SMOKE_RAM_SIZE)
    }

    pub fn xv6_sized() -> Self {
        Self::new(cfg::DRAM_SIZE)
    }

    pub fn state(&self) -> Rc<RefCell<TestBusState>> {
        Rc::clone(&self.state)
    }

    pub fn load_flat_binary<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn Error>> {
        let bytes = fs::read(path)?;
        self.state.borrow_mut().load_at_dram_base(&bytes);
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
            0x01 | 0x02 | 0x03 => {}
            _ => {}
        }
        Ok(())
    }
}

impl MemDevice for TestBus {
    fn read(&mut self, addr: u64, size: usize) -> Result<u64, Exception> {
        let mut state = self.state.borrow_mut();
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
        if let Some(offset) = Self::dram_offset(&state, addr, size) {
            for i in 0..size {
                state.ram[offset + i] = ((value >> (i * 8)) & 0xff) as u8;
            }
            return Ok(());
        }

        if (cfg::UART_BASE..cfg::UART_BASE + 0x100).contains(&addr) {
            return Self::write_uart(&mut state, addr, value, size);
        }

        state.mmio_log.push(MmioAccess {
            kind: MmioAccessKind::Write,
            addr,
            value: value as u64,
            size,
        });
        Err(Exception::StoreAMOAccessFault(addr))
    }
}

pub struct TestMachine {
    pub cpu: Cpu,
    pub state: Rc<RefCell<TestBusState>>,
}

impl TestMachine {
    pub fn from_bus(bus: TestBus) -> Self {
        let state = bus.state();
        Self {
            cpu: Cpu::new(Box::new(bus)),
            state,
        }
    }

    pub fn with_flat_binary<P: AsRef<Path>>(
        path: P,
        ram_size: usize,
    ) -> Result<Self, Box<dyn Error>> {
        let bus = TestBus::new(ram_size);
        bus.load_flat_binary(path)?;
        Ok(Self::from_bus(bus))
    }

    pub fn run_steps(&mut self, max_steps: usize) -> Result<(), Exception> {
        for _ in 0..max_steps {
            self.cpu.step()?;
        }
        Ok(())
    }

    pub fn queue_uart_input(&mut self, input: &str) {
        self.state
            .borrow_mut()
            .queue_uart_input(input.as_bytes());
    }

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

    pub fn require_uart_lacks(&self, forbidden: &[&str]) -> Result<(), Box<dyn Error>> {
        let output = self.state.borrow().uart_output_string();
        for needle in forbidden {
            if output.contains(needle) {
                return Err(format!("unexpected UART output {needle:?}\nUART output:\n{output}").into());
            }
        }
        Ok(())
    }
}

pub fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn testbench_target_dir() -> PathBuf {
    project_root().join("target/testbench")
}

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

pub fn xv6_kernel_bin() -> PathBuf {
    testbench_target_dir().join("xv6-riscv/kernel/kernel.bin")
}

pub fn xv6_kernel_elf() -> PathBuf {
    testbench_target_dir().join("xv6-riscv/kernel/kernel")
}

pub fn xv6_fs_img() -> PathBuf {
    testbench_target_dir().join("xv6-riscv/fs.img")
}

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

pub fn xv6_machine() -> Result<TestMachine, Box<dyn Error>> {
    require_xv6_fixture()?;
    TestMachine::with_flat_binary(xv6_kernel_bin(), cfg::DRAM_SIZE)
}

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
