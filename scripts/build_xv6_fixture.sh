#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${XV6_DIR:-$ROOT/target/testbench/xv6-riscv}"
XV6_REPO="${XV6_REPO:-https://github.com/mit-pdos/xv6-riscv.git}"
XV6_REF="${XV6_REF:-riscv}"
TOOLPREFIX="${TOOLPREFIX:-riscv64-elf-}"

missing=()
need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    missing+=("$1")
  fi
}

need git
need make
need gcc
need perl
need "${TOOLPREFIX}gcc"
need "${TOOLPREFIX}objcopy"
need "${TOOLPREFIX}readelf"

if ((${#missing[@]} > 0)); then
  printf 'Missing tools:\n' >&2
  printf '  %s\n' "${missing[@]}" >&2
  cat >&2 <<'EOF'

On Arch Linux, install the missing pieces manually if needed:
  sudo pacman -S --needed base-devel git perl riscv64-elf-gcc riscv64-elf-binutils
EOF
  exit 127
fi

mkdir -p "$(dirname "$DEST")"

if [[ ! -d "$DEST/.git" ]]; then
  git clone --depth 1 --branch "$XV6_REF" "$XV6_REPO" "$DEST"
else
  if git -C "$DEST" fetch --depth 1 origin "$XV6_REF"; then
    git -C "$DEST" checkout --detach FETCH_HEAD
  else
    printf 'warning: could not refresh xv6; reusing existing checkout at %s\n' "$DEST" >&2
  fi
fi

make -C "$DEST" TOOLPREFIX="$TOOLPREFIX" kernel/kernel fs.img
"${TOOLPREFIX}objcopy" -O binary "$DEST/kernel/kernel" "$DEST/kernel/kernel.bin"

commit="$(git -C "$DEST" rev-parse HEAD)"
entry="$("${TOOLPREFIX}readelf" -h "$DEST/kernel/kernel" | awk '/Entry point address/ {print $4}')"

cat >"$DEST/fixture.env" <<EOF
XV6_REPO=$XV6_REPO
XV6_REF=$XV6_REF
XV6_COMMIT=$commit
TOOLPREFIX=$TOOLPREFIX
KERNEL_ELF=$DEST/kernel/kernel
KERNEL_BIN=$DEST/kernel/kernel.bin
FS_IMG=$DEST/fs.img
ENTRY=$entry
EOF

printf 'xv6 fixture ready:\n'
printf '  commit: %s\n' "$commit"
printf '  entry:  %s\n' "$entry"
printf '  kernel: %s\n' "$DEST/kernel/kernel"
printf '  binary: %s\n' "$DEST/kernel/kernel.bin"
printf '  fs.img: %s\n' "$DEST/fs.img"
