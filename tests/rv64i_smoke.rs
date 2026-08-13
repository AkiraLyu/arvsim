//! RV64 指令执行和正式 `virt` 平台设备的短路径集成测试。
//!
//! 汇编片段由外部 RISC-V 工具链编译为 flat binary，再通过与 xv6 相同的测试机器接口执行。

mod support;

use arvsim::{cfg, csr};
use std::error::Error;

const RV64I_SIGNATURE_ADDR: u64 = cfg::DRAM_BASE + 0x1000;
const RV64I_TRAP_VECTOR: u64 = cfg::DRAM_BASE + 0x2000;
const RV64I_RAM_SIZE: usize = 1024 * 1024;

#[test]
fn compiled_addi_smoke_runs_one_step() -> Result<(), Box<dyn Error>> {
    let bin = support::build_flat_asm(
        "addi-smoke",
        r#"
        .section .text
        .globl _start
_start:
        addi x31, x0, 42
"#,
    )?;

    let mut machine = support::TestMachine::with_flat_binary(bin, RV64I_RAM_SIZE)?;
    machine.run_steps(1).unwrap();

    assert_eq!(machine.cpu.pc, cfg::DRAM_BASE + 4);
    assert_eq!(machine.cpu.registers[31], 42);
    Ok(())
}

#[test]
fn testbench_uart_model_captures_16550_transmit_bytes() {
    let mut machine = support::TestMachine::rv64_smoke().unwrap();
    let device = &mut machine.cpu.bus;

    device.write(cfg::UART_BASE + 3, 0x80, 1).unwrap();
    device.write(cfg::UART_BASE, 3, 1).unwrap();
    device.write(cfg::UART_BASE + 3, 0x03, 1).unwrap();
    device.write(cfg::UART_BASE, u64::from(b'O'), 1).unwrap();
    device.write(cfg::UART_BASE, u64::from(b'K'), 1).unwrap();

    assert_eq!(machine.uart_output_string(), "OK");
}

#[test]
fn testbench_virtio_rejects_invalid_queue_sizes() {
    const VIRTIO_QUEUE_NUM: u64 = cfg::VIRTIO_BLOCK_BASE + 0x38;
    let mut machine = support::TestMachine::rv64_smoke().unwrap();

    for value in [0, 3, u64::from(cfg::VIRTIO_QUEUE_SIZE) * 2] {
        assert_eq!(
            machine.cpu.bus.write(VIRTIO_QUEUE_NUM, value, 4),
            Err(arvsim::trap::Exception::StoreAMOAccessFault(
                VIRTIO_QUEUE_NUM
            ))
        );
    }
}

#[test]
fn testbench_machine_reset_clears_devices_but_preserves_ram() {
    let mut machine = support::TestMachine::rv64_smoke().unwrap();

    machine.cpu.bus.write(cfg::DRAM_BASE, 0x2a, 1).unwrap();
    machine
        .cpu
        .bus
        .write(cfg::UART_BASE, u64::from(b'A'), 1)
        .unwrap();
    machine.queue_uart_bytes(b"x");
    assert_eq!(machine.uart_output_string(), "A");

    machine.reset();

    assert_eq!(machine.cpu.bus.read(cfg::DRAM_BASE, 1), Ok(0x2a));
    assert_eq!(machine.uart_output_string(), "");
    assert_eq!(machine.cpu.bus.read(cfg::UART_BASE + 5, 1).unwrap() & 1, 0);
}

#[test]
fn testbench_bus_rejects_invalid_and_partial_accesses() {
    let mut machine = support::TestMachine::rv64_smoke().unwrap();
    let bus = &mut machine.cpu.bus;

    for size in [0, 3, 9, usize::MAX] {
        assert_eq!(
            bus.read(cfg::DRAM_BASE, size),
            Err(arvsim::trap::Exception::LoadAccessFault(cfg::DRAM_BASE))
        );
        assert_eq!(
            bus.write(cfg::DRAM_BASE, u64::MAX, size),
            Err(arvsim::trap::Exception::StoreAMOAccessFault(cfg::DRAM_BASE))
        );
    }

    let value = 0x0123_4567_89ab_cdef;
    bus.write(cfg::DRAM_BASE, value, 8).unwrap();
    assert_eq!(bus.read(cfg::DRAM_BASE, 8).unwrap(), value);
    assert_eq!(
        bus.write(cfg::UART_BASE, value, 8),
        Err(arvsim::trap::Exception::StoreAMOAccessFault(cfg::UART_BASE))
    );
    assert_eq!(
        bus.read(u64::MAX, 8),
        Err(arvsim::trap::Exception::LoadAccessFault(u64::MAX))
    );
    assert!(machine.uart_output().is_empty());
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
    let bin = support::build_flat_asm("rv64i-contract", &asm)?;

    let mut machine = support::TestMachine::with_flat_binary(bin, RV64I_RAM_SIZE)?;
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
