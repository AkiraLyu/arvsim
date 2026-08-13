#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

build_fixture=0
boot_only=0

usage() {
  cat <<'EOF'
Usage:
  scripts/run_xv6_cli.sh [--build-fixture] [--boot-only]

Options:
  --build-fixture  Rebuild the fixture directory (XV6_DIR or the default path).
  --boot-only      Boot until the first xv6 shell prompt, then exit.

Interactive mode forwards terminal input to xv6 UART. Press Ctrl-] to leave.
EOF
}

while (($# > 0)); do
  case "$1" in
    --build-fixture)
      build_fixture=1
      ;;
    --boot-only)
      boot_only=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
  shift
done

xv6_dir="${XV6_DIR:-$ROOT/target/testbench/xv6-riscv}"
if [[ ! -f "$xv6_dir/kernel/kernel.bin" ||
      ! -f "$xv6_dir/kernel/kernel" ||
      ! -f "$xv6_dir/fs.img" ||
      ! -f "$xv6_dir/user/_usertests" ]]; then
  build_fixture=1
fi

if ((build_fixture)); then
  scripts/build_xv6_fixture.sh
fi

cargo build --release >/dev/null

out_dir="$ROOT/target/testbench/generated"
runner="$out_dir/xv6_cli.rs"
binary="$out_dir/xv6_cli"
mkdir -p "$out_dir"

rlib="$ROOT/target/release/libarvsim.rlib"
if [[ ! -f "$rlib" ]]; then
  printf 'could not find release arvsim rlib after cargo build\n' >&2
  exit 1
fi

cat >"$runner" <<RS
#[path = "${ROOT}/tests/support/mod.rs"]
mod support;
RS

cat >>"$runner" <<'RS'
use std::env;
use std::error::Error;
use std::io::{self, Read, Write};
use std::sync::mpsc;
use std::thread;

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn main() -> Result<(), Box<dyn Error>> {
    let boot_only = env::var_os("ARVSIM_XV6_CLI_BOOT_ONLY").is_some();
    let step_chunk = env_usize("ARVSIM_XV6_CLI_STEP_CHUNK", 10_000);
    let max_boot_steps = env_usize("ARVSIM_XV6_CLI_BOOT_STEPS", 50_000_000);

    eprintln!("[arvsim] loading xv6 fixture");
    let mut machine = support::xv6_machine()?;
    let (tx, rx) = mpsc::channel::<u8>();

    if !boot_only {
        thread::spawn(move || {
            let mut stdin = io::stdin();
            let mut byte = [0u8; 1];
            loop {
                match stdin.read(&mut byte) {
                    Ok(0) => break,
                    Ok(_) => {
                        if tx.send(byte[0]).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    let mut printed = 0usize;
    let mut steps = 0usize;
    let mut saw_prompt = false;
    let mut warned_boot_budget = false;

    loop {
        for byte in rx.try_iter() {
            if byte == 0x1d {
                eprintln!("\n[arvsim] leaving xv6 cli");
                return Ok(());
            }
            machine.queue_uart_bytes(&[byte]);
        }

        for _ in 0..step_chunk {
            machine
                .step()
                .map_err(|e| format!("fatal CPU error after {steps} steps: {e:?}"))?;
            steps = steps.wrapping_add(1);
        }

        let output = machine.uart_output();
        if output.len() > printed {
            io::stdout().write_all(&output[printed..])?;
            io::stdout().flush()?;
            printed = output.len();

            if !saw_prompt && output.windows(2).any(|window| window == b"$ ") {
                saw_prompt = true;
                if boot_only {
                    eprintln!("\n[arvsim] xv6 shell prompt reached after {steps} steps");
                    return Ok(());
                }
                eprintln!("\n[arvsim] xv6 shell ready; press Ctrl-] to leave");
            }
        }

        if !saw_prompt && steps > max_boot_steps {
            if boot_only {
                return Err(format!(
                    "timed out after {steps} steps while waiting for xv6 shell prompt"
                )
                .into());
            }
            if !warned_boot_budget {
                eprintln!(
                    "\n[arvsim] still waiting for shell prompt after {steps} steps; continuing"
                );
                warned_boot_budget = true;
            }
        }
    }
}
RS

CARGO_MANIFEST_DIR="$ROOT" rustc \
  --edition=2024 \
  -C opt-level=3 \
  "$runner" \
  -L "dependency=$ROOT/target/release/deps" \
  --extern "arvsim=$rlib" \
  -o "$binary"

if ((boot_only)); then
  ARVSIM_XV6_CLI_BOOT_ONLY=1 "$binary"
else
  printf '[arvsim] starting xv6 cli; press Ctrl-] to leave\n' >&2
  if [[ -t 0 ]]; then
    old_stty="$(stty -g)"
    restore_tty() {
      stty "$old_stty"
    }
    trap restore_tty EXIT
    stty -echo -icanon min 1 time 0
  fi
  "$binary"
fi
