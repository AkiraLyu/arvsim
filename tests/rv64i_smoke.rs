mod support;

use arvsim::bus::MemDevice;
use arvsim::cfg;
use std::error::Error;

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

    let mut machine = support::TestMachine::with_flat_binary(bin, 1024 * 1024)?;
    machine.run_steps(1).unwrap();

    assert_eq!(machine.cpu.pc, cfg::DRAM_BASE + 4);
    assert_eq!(machine.cpu.registers[31], 42);
    Ok(())
}

#[test]
fn testbench_uart_model_captures_16550_transmit_bytes() {
    let bus = support::TestBus::rv64_smoke();
    let state = bus.state();
    let mut device = bus;

    device.write(cfg::UART_BASE, b'O' as u32, 1).unwrap();
    device.write(cfg::UART_BASE, b'K' as u32, 1).unwrap();

    assert_eq!(state.borrow().uart_output_string(), "OK");
}

#[test]
#[ignore = "future ISA contract: requires correct load/store immediates, branches, jumps, and x0 hard-wiring"]
fn rv64i_memory_branch_and_x0_contract() -> Result<(), Box<dyn Error>> {
    let bin = support::build_flat_asm(
        "rv64i-contract",
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
pass:
        addi x31, x0, 42
"#,
    )?;

    let mut machine = support::TestMachine::with_flat_binary(bin, cfg::DRAM_SIZE)?;
    machine.run_steps(9).unwrap();

    assert_eq!(machine.cpu.registers[0], 0);
    assert_eq!(machine.cpu.registers[31], 42);
    Ok(())
}
