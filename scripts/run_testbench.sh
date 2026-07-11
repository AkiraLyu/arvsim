#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

case "${1:-}" in
  "")
    cargo test
    ;;
  --with-xv6-fixture)
    scripts/build_xv6_fixture.sh
    cargo test
    ;;
  --future-contracts)
    cargo test
    cargo test --test xv6_fixture -- --ignored
    ;;
  --xv6-contracts)
    scripts/build_xv6_fixture.sh
    cargo test --test xv6_fixture -- --ignored
    ;;
  *)
    cat >&2 <<'EOF'
Usage:
  scripts/run_testbench.sh
  scripts/run_testbench.sh --with-xv6-fixture
  scripts/run_testbench.sh --future-contracts
  scripts/run_testbench.sh --xv6-contracts

The opt-in xv6 contracts require an external fixture and long execution budgets.
They cover booting to a shell, running user programs, quick usertests, and the
full usertests suite; a failure is a regression or an unmet behavior contract.
EOF
    exit 2
    ;;
esac
