//! xv6 fixture 的分层验收测试。
//!
//! 测试以 UART 文本验证 shell、基础用户程序和完整 usertests；完整测试已包含快速用例。
//! 耗时较长的测试保持 `ignored`，由脚本或显式测试命令运行。

use arvsim::{uart::BufferedUartBackend, virt_platform::VirtMachine, xv6::Xv6Fixture};
use std::error::Error;
use std::io::Write;

const XV6_FAILURE_MARKERS: &[&str] = &[
    "panic:",
    "kernel panic",
    "FAILED",
    "SOME TESTS FAILED",
    "init: exec sh failed",
    "init: fork failed",
    "init: wait returned an error",
];
// 快速阶段包含大量真实 exec 调用；移除用户态捷径后需约 4 亿步。
const QUICK_TEST_STEPS: usize = 600_000_000;

fn budget(name: &str, default: usize) -> usize {
    // 每个阶段单独配置预算，超时信息才能准确指出卡住的启动或用户态阶段。
    let Ok(value) = std::env::var(name) else {
        return default;
    };
    value
        .parse()
        .unwrap_or_else(|error| panic!("{name}={value:?} is not a valid step budget: {error}"))
}

fn boot_to_shell() -> Result<Session, Box<dyn Error>> {
    let uart = BufferedUartBackend::new();
    let machine = Xv6Fixture::from_env().build(
        Box::new(uart.clone()),
        std::env::var_os("ARVSIM_XV6_NO_ACCELERATION").is_none(),
    )?;
    let mut machine = Session { machine, uart };
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
    Ok(machine)
}

#[test]
fn xv6_images_build_a_machine_when_present() -> Result<(), Box<dyn Error>> {
    let fixture = Xv6Fixture::from_env();
    if !fixture.directory().exists() && std::env::var_os("ARVSIM_REQUIRE_XV6_FIXTURE").is_none() {
        writeln!(
            std::io::stderr().lock(),
            "SKIPPED: run scripts/build_xv6_fixture.sh to build xv6 images"
        )?;
        return Ok(());
    }
    fixture.build(Box::new(BufferedUartBackend::new()), true)?;
    Ok(())
}

#[test]
#[ignore = "opt-in xv6 contract: requires an external fixture and a long execution budget"]
fn xv6_shell_runs_basic_user_programs() -> Result<(), Box<dyn Error>> {
    let mut machine = boot_to_shell()?;

    machine.uart.queue_input(b"echo ARVSIM_XV6_ECHO_OK\n");
    machine.require_uart_contains(
        "echo user program output",
        "\nARVSIM_XV6_ECHO_OK\n",
        budget("ARVSIM_XV6_COMMAND_STEPS", 20_000_000),
    )?;

    machine.uart.queue_input(b"ls\n");
    machine.require_uart_contains(
        "filesystem directory listing",
        "README",
        budget("ARVSIM_XV6_COMMAND_STEPS", 20_000_000),
    )?;

    machine.uart.queue_input(b"cat README\n");
    machine.require_uart_contains(
        "filesystem file read",
        "xv6 is a re-implementation",
        budget("ARVSIM_XV6_COMMAND_STEPS", 40_000_000),
    )?;

    Ok(())
}

#[test]
#[ignore = "opt-in full xv6 contract: requires an external fixture and a long execution budget"]
fn xv6_runs_full_usertests_suite() -> Result<(), Box<dyn Error>> {
    let mut machine = boot_to_shell()?;
    machine.uart.queue_input(b"usertests\n");
    machine.require_uart_contains(
        "full usertests start",
        "usertests starting",
        budget("ARVSIM_XV6_USERTESTS_START_STEPS", 40_000_000),
    )?;
    machine.require_uart_contains(
        "full usertests slow section",
        "usertests slow tests starting",
        budget("ARVSIM_XV6_SLOW_USERTESTS_START_STEPS", QUICK_TEST_STEPS),
    )?;
    machine.require_uart_contains(
        "full usertests completion",
        "ALL TESTS PASSED",
        budget("ARVSIM_XV6_FULL_USERTESTS_STEPS", 2_000_000_000),
    )?;
    Ok(())
}

struct Session {
    machine: VirtMachine,
    uart: BufferedUartBackend,
}

impl Session {
    fn require_uart_contains(
        &mut self,
        label: &str,
        expected: &str,
        max_steps: usize,
    ) -> Result<(), Box<dyn Error>> {
        let overlap = XV6_FAILURE_MARKERS
            .iter()
            .copied()
            .chain([expected])
            .map(str::len)
            .max()
            .unwrap()
            - 1;
        let mut checked = 0usize;
        for _ in 0..max_steps {
            self.machine
                .step()
                .map_err(|error| format!("CPU error during {label}: {error:?}"))?;
            let output = self.uart.output();
            if output.len() == checked {
                continue;
            }
            let new_text = String::from_utf8_lossy(&output[checked.saturating_sub(overlap)..]);
            if XV6_FAILURE_MARKERS
                .iter()
                .any(|marker| new_text.contains(marker))
            {
                return Err(
                    format!("xv6 failed during {label}: {}", self.uart.output_string()).into(),
                );
            }
            // 匹配可能横跨两次 UART 输出，重叠区同时保留成功与失败标记。
            if new_text.contains(expected) {
                return Ok(());
            }
            checked = output.len();
        }
        Err(format!(
            "timed out after {max_steps} steps waiting for {label}\n{}",
            self.uart.output_string()
        )
        .into())
    }
}
