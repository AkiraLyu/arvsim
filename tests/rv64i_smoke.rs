//! 编译实际 RV64 程序，并验证正式平台上的执行与复位行为。

use arvsim::uart::BufferedUartBackend;
use arvsim::virt_platform::{VirtPlatform, VirtPlatformConfig};
use arvsim::virtio::MemoryBlockBackend;
use arvsim::{
    cfg, csr,
    loader::{self, ImageFormat},
};
use std::{error::Error, fs, path::PathBuf, process::Command};

const RV64I_SIGNATURE_ADDR: u64 = cfg::DRAM_BASE + 0x1000;
const RV64I_TRAP_VECTOR: u64 = cfg::DRAM_BASE + 0x2000;
const RV64I_RAM_SIZE: usize = 1024 * 1024;

fn platform(uart: &BufferedUartBackend) -> Result<VirtPlatform, Box<dyn Error>> {
    Ok(VirtPlatform::new(
        VirtPlatformConfig {
            dram_size: RV64I_RAM_SIZE,
            ..VirtPlatformConfig::default()
        },
        Box::new(uart.clone()),
        Box::new(MemoryBlockBackend::new(Vec::new())),
    )?)
}

#[test]
fn machine_reset_clears_devices_but_preserves_ram() -> Result<(), Box<dyn Error>> {
    let uart = BufferedUartBackend::new();
    let mut machine = platform(&uart)?.build(cfg::DRAM_BASE)?;

    machine.cpu.bus.write(cfg::DRAM_BASE, 0x2a, 1).unwrap();
    machine
        .cpu
        .bus
        .write(cfg::UART_BASE, u64::from(b'A'), 1)
        .unwrap();
    uart.queue_input(b"x");
    assert_eq!(uart.output_string(), "A");

    machine.reset();

    assert_eq!(machine.cpu.bus.read(cfg::DRAM_BASE, 1), Ok(0x2a));
    assert_eq!(uart.output_string(), "");
    assert_eq!(machine.cpu.bus.read(cfg::UART_BASE + 5, 1).unwrap() & 1, 0);
    Ok(())
}

#[test]
fn rv64i_memory_branch_and_x0_contract() -> Result<(), Box<dyn Error>> {
    let asm = format!(
        r#"
        .section .text
        .globl _start
_start:
        addi x0, x0, 7
        addi sp, sp, -16
        addi t0, x0, 123
        sw   t0, 0(sp)
        lw   t1, 0(sp)
        bne  t0, t1, fail
        jal  x0, pass
fail:
        addi x31, x0, 1
        jal  x0, done
pass:
        addi x31, x0, 42
done:
        li   t2, {RV64I_SIGNATURE_ADDR:#x}
        sd   x31, 0(t2)
        ebreak
"#
    );
    let image = build_flat_asm(&asm)?;
    let uart = BufferedUartBackend::new();
    let platform = platform(&uart)?;
    let loaded = loader::load_image(&mut platform.dram_mut(), image, ImageFormat::Flat)?;
    let mut machine = platform.build(loaded.entry)?;
    assert_eq!(
        machine.cpu.registers[2],
        cfg::DRAM_BASE + RV64I_RAM_SIZE as u64
    );
    machine.cpu.csr.store(csr::MTVEC, RV64I_TRAP_VECTOR);
    let mut halted = false;
    for _ in 0..32 {
        machine
            .step()
            .map_err(|error| format!("unexpected fatal CPU error: {error:?}"))?;
        if machine.cpu.pc == RV64I_TRAP_VECTOR && machine.cpu.csr.load(csr::MCAUSE) == 3 {
            halted = true;
            break;
        }
    }

    assert!(halted, "guest did not reach its explicit ebreak halt");
    let signature = machine
        .cpu
        .bus
        .read(RV64I_SIGNATURE_ADDR, 8)
        .map_err(|error| format!("failed to read guest signature: {error:?}"))?;
    assert_eq!(machine.cpu.registers[0], 0);
    assert_eq!(signature, 42);
    Ok(())
}

fn build_flat_asm(asm: &str) -> Result<PathBuf, Box<dyn Error>> {
    let prefix = std::env::var("TOOLPREFIX").unwrap_or_else(|_| "riscv64-elf-".into());
    let gcc = format!("{prefix}gcc");
    let objcopy = format!("{prefix}objcopy");
    let out_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(&out_dir)?;

    let stem = format!("rv64i-{}", std::process::id());
    let asm_path = out_dir.join(format!("{stem}.S"));
    let linker_path = out_dir.join(format!("{stem}.ld"));
    let elf_path = out_dir.join(format!("{stem}.elf"));
    let bin_path = out_dir.join(format!("{stem}.bin"));

    fs::write(&asm_path, asm)?;
    fs::write(
        &linker_path,
        format!(
            "OUTPUT_ARCH(riscv)\nENTRY(_start)\nSECTIONS {{\n. = {:#x};\n.text : {{ *(.text .text.*) }}\n.rodata : {{ *(.rodata .rodata.*) }}\n.data : {{ *(.data .data.*) }}\n.bss : {{ *(.bss .bss.* COMMON) }}\n}}\n",
            cfg::DRAM_BASE,
        ),
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

fn run(command: &mut Command) -> Result<(), Box<dyn Error>> {
    let output = command
        .output()
        .map_err(|error| format!("{command:?}: {error}"))?;
    if !output.status.success() {
        return Err(format!("{command:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(())
}
