#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${XV6_DIR:-$ROOT/target/testbench/xv6-riscv}"
XV6_REPO="${XV6_REPO:-https://github.com/mit-pdos/xv6-riscv.git}"
XV6_REF="${XV6_REF:-$(cat "$ROOT/fixtures/xv6-revision")}"
XV6_OFFLINE="${XV6_OFFLINE:-0}"
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
need sha256sum
need "${TOOLPREFIX}gcc"
need "${TOOLPREFIX}objcopy"
need "${TOOLPREFIX}readelf"
need "${TOOLPREFIX}nm"

if ((${#missing[@]} > 0)); then
  printf 'Missing tools:\n' >&2
  printf '  %s\n' "${missing[@]}" >&2
  cat >&2 <<'EOF'

On Arch Linux, install the missing pieces manually if needed:
  run0 pacman -S --needed base-devel git perl riscv64-elf-gcc riscv64-elf-binutils
EOF
  exit 127
fi

mkdir -p "$(dirname "$DEST")"

if [[ "$XV6_OFFLINE" != 0 && "$XV6_OFFLINE" != 1 ]]; then
  printf 'error: XV6_OFFLINE must be 0 or 1\n' >&2
  exit 2
fi

if [[ ! -e "$DEST/.git" ]]; then
  if [[ "$XV6_OFFLINE" == 1 ]]; then
    printf 'error: offline build requires an existing xv6 checkout\n' >&2
    exit 1
  fi
  git init -q "$DEST"
  git -C "$DEST" remote add origin "$XV6_REPO"
fi
if git -C "$DEST" rev-parse --verify HEAD >/dev/null 2>&1 &&
   ! git -C "$DEST" diff --quiet HEAD --; then
  printf 'error: xv6 checkout has tracked changes; refusing to replace them\n' >&2
  exit 1
fi

if [[ "$XV6_OFFLINE" == 1 ]]; then
  requested_commit="$(git -C "$DEST" rev-parse --verify --end-of-options "$XV6_REF^{commit}")"
  if [[ "$(git -C "$DEST" rev-parse HEAD)" != "$requested_commit" ||
        "$(git -C "$DEST" remote get-url origin)" != "$XV6_REPO" ]]; then
    printf 'error: offline checkout does not match XV6_REPO and XV6_REF\n' >&2
    exit 1
  fi
else
  git -C "$DEST" remote set-url origin "$XV6_REPO"
  git -C "$DEST" fetch --depth 1 -- origin "$XV6_REF"
  git -C "$DEST" checkout --detach FETCH_HEAD
fi

# 重新生成所有产物，防止旧目标文件掩盖源码或工具链的变化。
rm -f "$DEST/fixture.env" "$DEST/fixture.sha256"
make -C "$DEST" TOOLPREFIX="$TOOLPREFIX" clean
make -C "$DEST" TOOLPREFIX="$TOOLPREFIX" kernel/kernel fs.img
"${TOOLPREFIX}objcopy" -O binary "$DEST/kernel/kernel" "$DEST/kernel/kernel.bin"

commit="$(git -C "$DEST" rev-parse HEAD)"
entry="$(LC_ALL=C "${TOOLPREFIX}readelf" -h "$DEST/kernel/kernel" | awk '/Entry point address/ {print $4}')"
if [[ -z "$entry" ]]; then
  printf 'error: could not determine entry point from %s\n' "$DEST/kernel/kernel" >&2
  exit 1
fi

cat >"$DEST/fixture.env" <<EOF
XV6_REPO=$XV6_REPO
XV6_REF=$XV6_REF
XV6_COMMIT=$commit
TOOLPREFIX=$TOOLPREFIX
KERNEL_ELF=kernel/kernel
KERNEL_BIN=kernel/kernel.bin
FS_IMG=fs.img
USERTESTS_ELF=user/_usertests
ENTRY=$entry
EOF

(
  cd "$DEST"
  sha256sum -- kernel/kernel kernel/kernel.bin fs.img user/_usertests >fixture.sha256
)

printf 'xv6 fixture ready:\n'
printf '  commit: %s\n' "$commit"
printf '  entry:  %s\n' "$entry"
printf '  kernel: %s\n' "$DEST/kernel/kernel"
printf '  binary: %s\n' "$DEST/kernel/kernel.bin"
printf '  fs.img: %s\n' "$DEST/fs.img"
