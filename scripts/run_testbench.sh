#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

usage() {
  cat <<'EOF'
Usage:
  scripts/run_testbench.sh
  scripts/run_testbench.sh --with-xv6-fixture
  scripts/run_testbench.sh --future-contracts
  scripts/run_testbench.sh --xv6-contracts

The two opt-in xv6 tests require an external fixture and long execution budgets.
They boot to a shell, then run basic user programs or the full usertests suite.
The full suite includes the quick tests.
EOF
}

if (($# > 1)); then
  printf 'error: expected at most one argument\n\n' >&2
  usage >&2
  exit 2
fi

case "${1:-}" in
  "")
    cargo test --all-targets
    ;;
  --with-xv6-fixture)
    scripts/build_xv6_fixture.sh
    ARVSIM_REQUIRE_XV6_FIXTURE=1 cargo test --all-targets
    ;;
  --future-contracts)
    cargo test --all-targets
    cargo test --release --test xv6_fixture -- --ignored --test-threads=1
    ;;
  --xv6-contracts)
    scripts/build_xv6_fixture.sh
    ARVSIM_REQUIRE_XV6_FIXTURE=1 \
      cargo test --test xv6_fixture xv6_images_build_a_machine_when_present
    cargo test --release --test xv6_fixture -- --ignored --test-threads=1
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
