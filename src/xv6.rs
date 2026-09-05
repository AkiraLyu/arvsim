//! xv6 镜像验证与机器组装，供命令行示例和集成测试共用。
//!
//! 设备和 CPU 均来自正式库。可选加速仅接受固定提交的干净源码及构建脚本记录的镜像。

use crate::cpu::{XV6_PROC_TABLE_SIZE, Xv6Accelerator};
use crate::loader::{self, ImageFormat};
use crate::uart::UartBackend;
use crate::virt_platform::{VirtMachine, VirtPlatform, VirtPlatformConfig};
use crate::virtio::MemoryBlockBackend;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 与固定结构布局匹配的 xv6 提交；构建脚本读取同一文件。
pub const SUPPORTED_REVISION: &str = include_str!("../fixtures/xv6-revision");
const KERNEL_ELF: &str = "kernel/kernel";
const DISK_IMAGE: &str = "fs.img";

/// 构建脚本生成的 xv6 镜像目录。
pub struct Xv6Fixture {
    directory: PathBuf,
}

impl Xv6Fixture {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    /// 使用 `XV6_DIR`，未设置时读取仓库的默认测试镜像目录。
    pub fn from_env() -> Self {
        Self::new(
            std::env::var_os("XV6_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("target/testbench/xv6-riscv")
                }),
        )
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// 装载内核与磁盘；关闭加速时不依赖源码仓库、构建记录或 ELF 符号。
    pub fn build(
        &self,
        uart: Box<dyn UartBackend>,
        accelerate: bool,
    ) -> Result<VirtMachine, Box<dyn Error>> {
        let accelerator = if accelerate {
            Some(self.accelerator()?)
        } else {
            None
        };
        let disk = fs::read(self.directory.join(DISK_IMAGE))?;
        if disk.is_empty() {
            return Err("xv6 filesystem image is empty".into());
        }
        let platform = VirtPlatform::new(
            VirtPlatformConfig::default(),
            uart,
            Box::new(MemoryBlockBackend::new(disk)),
        )?;
        let kernel = loader::load_image(
            &mut platform.dram_mut(),
            self.directory.join(KERNEL_ELF),
            ImageFormat::Elf,
        )?;
        let mut machine = platform.build(kernel.entry)?;
        if let Some(accelerator) = accelerator {
            machine.cpu.set_xv6_accelerator(accelerator)?;
        }
        Ok(machine)
    }

    fn accelerator(&self) -> Result<Xv6Accelerator, Box<dyn Error>> {
        let metadata = fs::read_to_string(self.directory.join("fixture.env"))?;
        let commit = command_output(
            Command::new("git")
                .arg("-C")
                .arg(&self.directory)
                .args(["rev-parse", "HEAD"]),
        )?;
        if commit.trim() != SUPPORTED_REVISION.trim() {
            return Err("xv6 acceleration requires the pinned revision; disable acceleration for other versions".into());
        }
        if !metadata
            .lines()
            .any(|line| line.strip_prefix("XV6_COMMIT=") == Some(commit.trim()))
        {
            return Err("xv6 fixture commit does not match the checkout".into());
        }
        command_output(
            Command::new("git")
                .arg("-C")
                .arg(&self.directory)
                .args(["diff", "--quiet", "HEAD", "--"]),
        )?;

        // 只校验运行所需的固定文件，不读取清单中任意路径，也不依赖清单顺序。
        let expected = fs::read_to_string(self.directory.join("fixture.sha256"))?;
        let actual = command_output(
            Command::new("sha256sum")
                .current_dir(&self.directory)
                .args(["--", KERNEL_ELF, DISK_IMAGE]),
        )?;
        if !actual
            .lines()
            .all(|line| expected.lines().any(|expected| expected == line))
        {
            return Err("xv6 artifact checksums do not match fixture.sha256".into());
        }
        accelerator_from_symbols(&symbols_from_elf(&self.directory.join(KERNEL_ELF))?)
    }
}

fn command_output(command: &mut Command) -> Result<String, Box<dyn Error>> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "{command:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn accelerator_from_symbols(
    symbols: &BTreeMap<String, u64>,
) -> Result<Xv6Accelerator, Box<dyn Error>> {
    let proc_start = required_xv6_symbol(symbols, "proc")?;
    let config = VirtPlatformConfig::default();
    let dram_end = config
        .dram_base
        .checked_add(u64::try_from(config.dram_size)?)
        .ok_or("DRAM address range overflows")?;
    let accelerator = Xv6Accelerator {
        mycpu: required_xv6_symbol(symbols, "mycpu")?,
        holding: required_xv6_symbol(symbols, "holding")?,
        push_off: required_xv6_symbol(symbols, "push_off")?,
        acquire: required_xv6_symbol(symbols, "acquire")?,
        pop_off: required_xv6_symbol(symbols, "pop_off")?,
        release: required_xv6_symbol(symbols, "release")?,
        memcmp: required_xv6_symbol(symbols, "memcmp")?,
        memmove: required_xv6_symbol(symbols, "memmove")?,
        strncmp: required_xv6_symbol(symbols, "strncmp")?,
        strncpy: required_xv6_symbol(symbols, "strncpy")?,
        strlen: required_xv6_symbol(symbols, "strlen")?,
        uvmunmap: required_xv6_symbol(symbols, "uvmunmap")?,
        freewalk: required_xv6_symbol(symbols, "freewalk")?,
        uvmcopy: required_xv6_symbol(symbols, "uvmcopy")?,
        myproc: required_xv6_symbol(symbols, "myproc")?,
        wakeup: required_xv6_symbol(symbols, "wakeup")?,
        cpus: required_xv6_symbol(symbols, "cpus")?,
        kmem: required_xv6_symbol(symbols, "kmem")?,
        kernel_end: required_xv6_symbol(symbols, "end")?,
        phys_top: dram_end,
        dram_base: config.dram_base,
        dram_end,
        proc_start,
        proc_end: proc_start
            .checked_add(XV6_PROC_TABLE_SIZE)
            .ok_or("xv6 proc table address overflows")?,
    };
    Ok(accelerator)
}

fn symbols_from_elf(path: &Path) -> Result<BTreeMap<String, u64>, Box<dyn Error>> {
    let nm = format!(
        "{}nm",
        std::env::var("TOOLPREFIX").unwrap_or_else(|_| "riscv64-elf-".into())
    );
    let stdout = command_output(Command::new(&nm).arg(path))?;
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
        .ok_or_else(|| format!("xv6 ELF is missing required symbol: {name}").into())
}
