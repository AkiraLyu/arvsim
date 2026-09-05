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
if [[ ! -f "$xv6_dir/kernel/kernel" || ! -f "$xv6_dir/fs.img" ]]; then
  build_fixture=1
fi
if [[ -z "${ARVSIM_XV6_NO_ACCELERATION+x}" &&
      ( ! -f "$xv6_dir/fixture.env" || ! -f "$xv6_dir/fixture.sha256" ) ]]; then
  build_fixture=1
fi

if ((build_fixture)); then
  scripts/build_xv6_fixture.sh
fi

args=()
if ((boot_only)); then
  args+=(--boot-only)
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
fi
cargo run --release --example xv6 -- "${args[@]}"
