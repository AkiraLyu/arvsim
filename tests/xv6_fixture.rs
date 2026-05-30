mod support;

use std::error::Error;
use std::process::Command;

const XV6_FAILURE_MARKERS: &[&str] = &[
    "panic:",
    "kernel panic",
    "FAILED",
    "SOME TESTS FAILED",
    "init: exec sh failed",
    "init: fork failed",
    "init: wait returned an error",
];

fn budget(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn boot_to_shell() -> Result<support::TestMachine, Box<dyn Error>> {
    let mut machine = support::xv6_machine()?;
    machine.require_uart_contains(
        "xv6 kernel boot banner",
        "xv6 kernel is booting",
        budget("ARVSIM_XV6_BANNER_STEPS", 2_000_000),
    )?;
    machine.require_uart_contains(
        "init spawning shell",
        "init: starting sh",
        budget("ARVSIM_XV6_INIT_STEPS", 20_000_000),
    )?;
    machine.require_uart_contains(
        "xv6 shell prompt",
        "$ ",
        budget("ARVSIM_XV6_SHELL_STEPS", 20_000_000),
    )?;
    machine.require_uart_lacks(XV6_FAILURE_MARKERS)?;
    Ok(machine)
}

#[test]
fn xv6_fixture_artifacts_are_well_formed_when_present() -> Result<(), Box<dyn Error>> {
    let kernel = support::xv6_kernel_elf();
    let kernel_bin = support::xv6_kernel_bin();
    let fs_img = support::xv6_fs_img();

    if !kernel.exists() || !kernel_bin.exists() || !fs_img.exists() {
        eprintln!(
            "xv6 fixture not built; run scripts/build_xv6_fixture.sh to enable artifact checks"
        );
        return Ok(());
    }

    support::require_tool("riscv64-elf-readelf")?;
    let output =
        support::run(Command::new("riscv64-elf-readelf").args(["-h", kernel.to_str().unwrap()]))?;
    let header = String::from_utf8_lossy(&output.stdout);

    assert!(header.contains("Machine:                           RISC-V"));
    assert!(header.contains("Entry point address:               0x80000000"));
    assert!(std::fs::metadata(kernel_bin)?.len() > 0);
    assert!(std::fs::metadata(fs_img)?.len() > 0);
    Ok(())
}

#[test]
#[ignore = "future xv6 contract: requires RV64GC, CSRs, privilege transitions, traps, timer, PLIC, and virtio"]
fn xv6_kernel_reaches_first_shell() -> Result<(), Box<dyn Error>> {
    boot_to_shell()?;
    Ok(())
}

#[test]
#[ignore = "future xv6 contract: requires shell, console input, filesystem, user programs, and virtio disk"]
fn xv6_shell_runs_basic_user_programs() -> Result<(), Box<dyn Error>> {
    let mut machine = boot_to_shell()?;

    machine.queue_uart_input("echo ARVSIM_XV6_ECHO_OK\n");
    machine.require_uart_contains(
        "echo user program output",
        "\nARVSIM_XV6_ECHO_OK\n",
        budget("ARVSIM_XV6_COMMAND_STEPS", 20_000_000),
    )?;

    machine.queue_uart_input("ls\n");
    machine.require_uart_contains(
        "filesystem directory listing",
        "README",
        budget("ARVSIM_XV6_COMMAND_STEPS", 20_000_000),
    )?;

    machine.queue_uart_input("cat README\n");
    machine.require_uart_contains(
        "filesystem file read",
        "xv6 is a re-implementation",
        budget("ARVSIM_XV6_COMMAND_STEPS", 40_000_000),
    )?;

    machine.require_uart_lacks(XV6_FAILURE_MARKERS)?;
    Ok(())
}

#[test]
#[ignore = "future xv6 contract: requires user mode, syscalls, fork/exec/wait, pipes, filesystem, and timer interrupts"]
fn xv6_runs_quick_usertests() -> Result<(), Box<dyn Error>> {
    let mut machine = boot_to_shell()?;
    machine.queue_uart_input("usertests -q\n");
    machine.require_uart_contains(
        "quick usertests start",
        "usertests starting",
        budget("ARVSIM_XV6_USERTESTS_START_STEPS", 40_000_000),
    )?;
    machine.require_uart_contains(
        "quick usertests completion",
        "ALL TESTS PASSED",
        budget("ARVSIM_XV6_QUICK_USERTESTS_STEPS", 300_000_000),
    )?;
    machine.require_uart_lacks(XV6_FAILURE_MARKERS)?;
    Ok(())
}

#[test]
#[ignore = "full xv6 contract: boots to shell and completes the full xv6 usertests suite"]
fn xv6_runs_full_usertests_suite() -> Result<(), Box<dyn Error>> {
    let mut machine = boot_to_shell()?;
    machine.queue_uart_input("usertests\n");
    machine.require_uart_contains(
        "full usertests start",
        "usertests starting",
        budget("ARVSIM_XV6_USERTESTS_START_STEPS", 40_000_000),
    )?;
    machine.require_uart_contains(
        "full usertests slow section",
        "usertests slow tests starting",
        budget("ARVSIM_XV6_SLOW_USERTESTS_START_STEPS", 600_000_000),
    )?;
    machine.require_uart_contains(
        "full usertests completion",
        "ALL TESTS PASSED",
        budget("ARVSIM_XV6_FULL_USERTESTS_STEPS", 2_000_000_000),
    )?;
    machine.require_uart_lacks(XV6_FAILURE_MARKERS)?;
    Ok(())
}
