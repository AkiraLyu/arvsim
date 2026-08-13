#![allow(dead_code)]

//! 集成测试共用的机器与 fixture 工具。
//!
//! UART、PLIC、virtio-blk、DMA 和地址分发全部来自正式库；本模块只负责装载测试镜像、
//! 配置 xv6 加速入口并按 UART 输出组织断言。

use arvsim::cpu::{XV6_PROC_TABLE_SIZE, Xv6Accelerator};
use arvsim::machine::Machine;
use arvsim::trap::Exception;
use arvsim::uart::BufferedUartBackend;
use arvsim::virt_platform::{VirtMachine, VirtPlatform, VirtPlatformConfig};
use arvsim::virtio::MemoryBlockBackend;
use std::cell::Ref;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const DEFAULT_SMOKE_RAM_SIZE: usize = 1024 * 1024;

/// 正式 `virt` 平台与测试可观察的宿主后端。
pub struct TestMachine {
    pub machine: VirtMachine,
    pub uart: BufferedUartBackend,
    pub disk: MemoryBlockBackend,
}

impl Deref for TestMachine {
    type Target = Machine;

    fn deref(&self) -> &Self::Target {
        &self.machine.machine
    }
}

impl DerefMut for TestMachine {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.machine.machine
    }
}

impl TestMachine {
    /// 创建只装载空 DRAM 和空磁盘的正式平台。
    pub fn empty(ram_size: usize) -> Result<Self, Box<dyn Error>> {
        Self::with_images(ram_size, &[], Vec::new())
    }

    /// 创建用于短 RV64 冒烟测试的平台。
    pub fn rv64_smoke() -> Result<Self, Box<dyn Error>> {
        Self::empty(DEFAULT_SMOKE_RAM_SIZE)
    }

    /// 装载 flat binary 并创建指定 RAM 容量的机器。
    pub fn with_flat_binary<P: AsRef<Path>>(
        path: P,
        ram_size: usize,
    ) -> Result<Self, Box<dyn Error>> {
        let image = fs::read(path)?;
        Self::with_images(ram_size, &image, Vec::new())
    }

    fn with_images(
        ram_size: usize,
        image: &[u8],
        disk_image: Vec<u8>,
    ) -> Result<Self, Box<dyn Error>> {
        let config = VirtPlatformConfig {
            dram_size: ram_size,
            ..VirtPlatformConfig::default()
        };
        let uart = BufferedUartBackend::new();
        let disk = MemoryBlockBackend::new(disk_image);
        let platform = VirtPlatform::new(config, Box::new(uart.clone()), Box::new(disk.clone()))?;
        platform.dram_mut().load_bytes(config.dram_base, image)?;
        Ok(Self {
            machine: platform.build(config.dram_base)?,
            uart,
            disk,
        })
    }

    /// 精确执行指定步数；若机器将来上报致命错误则立即结束运行。
    pub fn run_steps(&mut self, max_steps: usize) -> Result<(), Exception> {
        for _ in 0..max_steps {
            self.step()?;
        }
        Ok(())
    }

    /// 将字符串作为原始字节追加到 UART 输入队列。
    pub fn queue_uart_input(&self, input: &str) {
        self.queue_uart_bytes(input.as_bytes());
    }

    pub fn queue_uart_bytes(&self, input: &[u8]) {
        self.uart.queue_input(input);
    }

    pub fn uart_output(&self) -> Ref<'_, [u8]> {
        self.uart.output()
    }

    pub fn uart_output_string(&self) -> String {
        self.uart.output_string()
    }

    /// 运行到 UART 输出包含目标文本或耗尽步数预算。
    pub fn run_until_uart_contains(
        &mut self,
        needle: &str,
        max_steps: usize,
    ) -> Result<bool, Exception> {
        let needle = needle.as_bytes();
        if needle.is_empty() {
            return Ok(true);
        }

        let mut searched = 0usize;
        for _ in 0..max_steps {
            self.step()?;
            let output = self.uart.output();
            if output.len() > searched {
                // 新内容可能只补全旧缓冲末尾的半个匹配，因此保留 needle-1 字节重叠区。
                let start = searched.saturating_sub(needle.len() - 1);
                if output[start..]
                    .windows(needle.len())
                    .any(|window| window == needle)
                {
                    return Ok(true);
                }
                searched = output.len();
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
                self.uart_output_string()
            )
            .into()),
            Err(error) => Err(format!(
                "fatal CPU error while waiting for {label}: {error:?}\nUART output:\n{}",
                self.uart_output_string()
            )
            .into()),
        }
    }

    /// 确认 UART 输出没有任何已知失败标记。
    pub fn require_uart_lacks(&self, forbidden: &[&str]) -> Result<(), Box<dyn Error>> {
        let output = self.uart_output_string();
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

/// xv6 源码与测试文件目录；与构建脚本共用 `XV6_DIR` 覆盖约定。
pub fn xv6_dir() -> PathBuf {
    std::env::var_os("XV6_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| testbench_target_dir().join("xv6-riscv"))
}

/// 按构建脚本的 `TOOLPREFIX` 约定生成 RISC-V 工具名。
pub fn toolchain(tool: &str) -> String {
    let prefix = std::env::var("TOOLPREFIX").unwrap_or_else(|_| "riscv64-elf-".to_string());
    format!("{prefix}{tool}")
}

/// 用外部 RISC-V 工具链构建一个从默认 DRAM 基址开始的 flat binary。
pub fn build_flat_asm(name: &str, asm: &str) -> Result<PathBuf, Box<dyn Error>> {
    let gcc = toolchain("gcc");
    let objcopy = toolchain("objcopy");
    require_tool(&gcc)?;
    require_tool(&objcopy)?;

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

    run(Command::new(&gcc).args([
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

    run(Command::new(&objcopy).args([
        "-O",
        "binary",
        elf_path.to_str().unwrap(),
        bin_path.to_str().unwrap(),
    ]))?;

    Ok(bin_path)
}

/// xv6 flat kernel 镜像路径。
pub fn xv6_kernel_bin() -> PathBuf {
    xv6_dir().join("kernel/kernel.bin")
}

/// 带符号表的 xv6 kernel ELF 路径。
pub fn xv6_kernel_elf() -> PathBuf {
    xv6_dir().join("kernel/kernel")
}

/// xv6 文件系统镜像路径。
pub fn xv6_fs_img() -> PathBuf {
    xv6_dir().join("fs.img")
}

/// xv6 `usertests` 用户程序 ELF 路径，用于解析用户态加速入口。
pub fn xv6_usertests_elf() -> PathBuf {
    xv6_dir().join("user/_usertests")
}

/// 确认运行 xv6 所需的 fixture 文件都已存在。
pub fn require_xv6_fixture() -> Result<(), Box<dyn Error>> {
    let required = [
        xv6_kernel_elf(),
        xv6_kernel_bin(),
        xv6_fs_img(),
        xv6_usertests_elf(),
    ];
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

/// 装载当前 xv6 fixture，并从实际 ELF 符号表配置兼容加速器。
pub fn xv6_machine() -> Result<TestMachine, Box<dyn Error>> {
    require_xv6_fixture()?;
    let symbols = symbols_from_elf(&xv6_kernel_elf())?;
    let user_symbols = symbols_from_elf(&xv6_usertests_elf())?;
    let proc_start = required_xv6_symbol(&symbols, "proc")?;
    let kernel = fs::read(xv6_kernel_bin())?;
    let disk = fs::read(xv6_fs_img())?;
    let config = VirtPlatformConfig::default();
    let dram_end = config
        .dram_base
        .checked_add(u64::try_from(config.dram_size)?)
        .ok_or("test DRAM address range overflows")?;
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
        phys_top: dram_end,
        dram_base: config.dram_base,
        dram_end,
        proc_start,
        proc_end: proc_start
            .checked_add(XV6_PROC_TABLE_SIZE)
            .ok_or("xv6 proc table address overflows")?,
        // 用户程序独立链接，必须从实际的 usertests ELF 解析入口。
        user_exec: Some(required_xv6_symbol(&user_symbols, "exec")?),
    };
    let mut machine = TestMachine::with_images(config.dram_size, &kernel, disk)?;
    machine.cpu.set_xv6_accelerator(accelerator);
    Ok(machine)
}

fn symbols_from_elf(path: &Path) -> Result<BTreeMap<String, u64>, Box<dyn Error>> {
    let nm = toolchain("nm");
    require_tool(&nm)?;
    let output = Command::new(&nm).arg(path).output()?;
    if !output.status.success() {
        return Err(format!(
            "{nm} failed for {} with status {}: {}",
            path.display(),
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
