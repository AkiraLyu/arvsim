//! xv6 fixture 的分层验收测试。
//!
//! 测试以 UART 文本作为黑盒进度信号，从镜像完整性逐步覆盖启动、shell、基础用户程序和 usertests。
//! 长时间合同保持 `ignored`，由脚本或显式测试命令运行。

mod support;

use arvsim::cfg;
use std::error::Error;
use std::io::Write;
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
    // 每个阶段单独配置预算，超时信息才能准确指出卡住的启动或用户态阶段。
    let Ok(value) = std::env::var(name) else {
        return default;
    };
    value
        .parse()
        .unwrap_or_else(|error| panic!("{name}={value:?} is not a valid step budget: {error}"))
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

    if let Err(error) = support::require_xv6_fixture() {
        if std::env::var_os("ARVSIM_REQUIRE_XV6_FIXTURE").is_some() {
            return Err(error);
        }
        writeln!(
            std::io::stderr().lock(),
            "SKIPPED: xv6 fixture not built; run scripts/build_xv6_fixture.sh to enable artifact checks"
        )?;
        return Ok(());
    }

    let readelf = support::toolchain("readelf");
    support::require_tool(&readelf)?;
    let output = support::run(
        Command::new(&readelf)
            .env("LC_ALL", "C")
            .args(["-h", kernel.to_str().unwrap()]),
    )?;
    let header = String::from_utf8_lossy(&output.stdout);
    let field = |key: &str, expected: &str| {
        header
            .lines()
            .any(|line| line.trim_start().starts_with(key) && line.contains(expected))
    };

    assert!(
        field("Machine:", "RISC-V"),
        "unexpected ELF header:\n{header}"
    );
    assert!(
        field("Entry point address:", &format!("{:#x}", cfg::DRAM_BASE)),
        "unexpected ELF entry point:\n{header}"
    );
    assert!(std::fs::metadata(kernel_bin)?.len() > 0);
    assert!(std::fs::metadata(fs_img)?.len() > 0);
    Ok(())
}

#[test]
#[ignore = "opt-in xv6 contract: requires an external fixture and a long execution budget"]
fn xv6_kernel_reaches_first_shell() -> Result<(), Box<dyn Error>> {
    boot_to_shell()?;
    Ok(())
}

#[test]
#[ignore = "opt-in xv6 contract: requires an external fixture and a long execution budget"]
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
#[ignore = "opt-in xv6 contract: requires an external fixture and a long execution budget"]
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
#[ignore = "opt-in full xv6 contract: requires an external fixture and up to two billion steps"]
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
