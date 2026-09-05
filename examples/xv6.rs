//! 通过正式平台运行 xv6，并流式转发终端输入输出。

use arvsim::uart::BufferedUartBackend;
use arvsim::xv6::Xv6Fixture;
use std::env;
use std::error::Error;
use std::io::{self, Read, Write};
use std::sync::mpsc;
use std::thread;

fn positive_budget(name: &str, value: &str) -> Result<u64, Box<dyn Error>> {
    value
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{name} must be a positive integer, got {value:?}").into())
}

fn env_budget(name: &str, default: u64) -> Result<u64, Box<dyn Error>> {
    match env::var(name) {
        Ok(value) => positive_budget(name, &value),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(error.into()),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut boot_only = false;
    let mut accelerate = env::var_os("ARVSIM_XV6_NO_ACCELERATION").is_none();
    for arg in env::args().skip(1) {
        match arg.as_str() {
            "--boot-only" => boot_only = true,
            "--no-acceleration" => accelerate = false,
            "--help" | "-h" => {
                println!(
                    "Usage: cargo run --release --example xv6 -- [--boot-only] [--no-acceleration]"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown option: {arg}").into()),
        }
    }
    let step_chunk = env_budget("ARVSIM_XV6_CLI_STEP_CHUNK", 10_000)?;
    let max_boot_steps = env_budget("ARVSIM_XV6_CLI_BOOT_STEPS", 50_000_000)?;

    let uart = BufferedUartBackend::new();
    let mut machine = Xv6Fixture::from_env().build(Box::new(uart.clone()), accelerate)?;
    let (tx, rx) = mpsc::sync_channel::<u8>(4096);
    if !boot_only {
        thread::spawn(move || {
            let mut stdin = io::stdin().lock();
            let mut byte = [0u8; 1];
            while matches!(stdin.read(&mut byte), Ok(1)) {
                if tx.send(byte[0]).is_err() {
                    break;
                }
            }
        });
    }

    let mut steps = 0u64;
    let mut saw_prompt = false;
    let mut previous_byte = None;
    let mut warned_boot_budget = false;
    loop {
        for byte in rx.try_iter() {
            if byte == 0x1d {
                return Ok(());
            }
            uart.queue_input(&[byte]);
        }
        // 即使用户配置很大步长，也定期排出 UART 缓冲，保持交互响应和有界输出历史。
        let mut chunk = step_chunk.min(10_000);
        if boot_only && !saw_prompt {
            chunk = chunk.min(max_boot_steps - steps);
        }
        for _ in 0..chunk {
            machine
                .step()
                .map_err(|error| format!("CPU error after {steps} steps: {error:?}"))?;
            steps = steps.saturating_add(1);
        }
        {
            let output = uart.output();
            if !output.is_empty() {
                let mut stdout = io::stdout().lock();
                stdout.write_all(&output)?;
                stdout.flush()?;
                for &byte in output.iter() {
                    if !saw_prompt && previous_byte == Some(b'$') && byte == b' ' {
                        saw_prompt = true;
                        eprintln!("\n[arvsim] xv6 shell prompt reached after {steps} steps");
                    }
                    previous_byte = Some(byte);
                }
            }
        }
        uart.clear_output();
        if saw_prompt && boot_only {
            return Ok(());
        }
        if !saw_prompt && steps >= max_boot_steps {
            if boot_only {
                return Err(format!(
                    "timed out after {steps} steps while waiting for xv6 shell prompt"
                )
                .into());
            }
            if !warned_boot_budget {
                eprintln!(
                    "[arvsim] still waiting for shell prompt after {steps} steps; continuing"
                );
                warned_boot_budget = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_budgets_are_errors() {
        for value in ["0", "-1", "", "invalid", "18446744073709551616"] {
            assert!(positive_budget("steps", value).is_err());
        }
        assert_eq!(positive_budget("steps", "1").unwrap(), 1);
    }
}
