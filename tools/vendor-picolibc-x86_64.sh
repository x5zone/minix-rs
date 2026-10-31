#!/usr/bin/env bash
# vendor-picolibc-x86_64.sh — ensure a non-TLS x86_64 picolibc is available and
# emit its library directory and a localized specs file.
#
# Why this exists: the x86_64 ATF guest leg used host glibc, whose startup
# requires per-process TLS (`arch_prctl(ARCH_SET_FS)`); the kernel has no FS
# base carrier, so glibc faults before the test runs. The aarch64/riscv ATF
# legs already use picolibc, so the non-TLS x86_64 libc keeps the C runtime
# consistent across the three legs and needs no kernel change.
#
# The tree is gitignored (`tools/vendor/`); this script regenerates it:
#   1. clone picolibc 1.8.6 (network) into tools/vendor/picolibc-src;
#   2. build it with the host x86_64 gcc in freestanding mode via meson/ninja;
#   3. install into tools/vendor/picolibc-x86_64 and write a localized specs
#      under os/target/atf/x86_64/.
# Output (two lines): the picolibc prefix, the specs path.
#
# The meson install's final step copies picolibc.specs into the compiler's
# built-in specs directory (`/usr/lib/gcc/...`), which needs root; the prefix
# (lib/include) is already populated before that step, so the failure is
# ignored after the artifacts are verified.
#
# Usage: tools/vendor-picolibc-x86_64.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR="$ROOT/tools/vendor"
# Allow a pre-fetched checkout (offline/flaky TLS); otherwise clone below.
SRC="${PICOLIBC_SRC:-$VENDOR/picolibc-src}"
PREFIX="$VENDOR/picolibc-x86_64"
OUT_DIR="$ROOT/os/target/atf/x86_64"
SPECS="$OUT_DIR/picolibc-x86_64.specs"

if [ ! -f "$PREFIX/lib/libc.a" ] || [ ! -f "$PREFIX/lib/libsemihost.a" ] \
        || [ ! -f "$PREFIX/include/picolibc.h" ] || [ ! -f "$PREFIX/lib/picolibc.ld" ]; then
    command -v git >/dev/null || { echo "缺 git（取 picolibc 源码）" >&2; exit 2; }
    command -v meson >/dev/null || { echo "缺 meson（构建 picolibc）" >&2; exit 2; }
    command -v ninja >/dev/null || { echo "缺 ninja（构建 picolibc）" >&2; exit 2; }
    mkdir -p "$VENDOR"
    if [ ! -f "$SRC/meson.build" ]; then
        echo "取 picolibc 1.8.6 源码…" >&2
        git clone --depth 1 --branch 1.8.6 \
            https://github.com/picolibc/picolibc.git "$SRC" >&2 || {
            echo "取源码失败（网络/TLS）。可先把 picolibc 1.8.6 检出到某目录，" >&2
            echo "再以 PICOLIBC_SRC=<dir> 重跑本脚本。" >&2
            exit 2
        }
    fi
    # Native x86_64 freestanding compiler configuration. No ccache (the
    # upstream cross file assumes it); `-nostdlib` makes meson's early
    # compiler probes use the picolibc world rather than the host libc.
    cat > "$SRC/cross-x86_64-host.txt" <<'EOF'
[binaries]
c = ['cc', '-march=core2', '-msse2', '-fno-pic', '-fno-PIE', '-static', '-nostdlib']
ar = 'ar'
as = 'as'
nm = 'nm'
strip = 'strip'
objcopy = 'objcopy'
[host_machine]
system = 'linux'
cpu_family = 'x86_64'
cpu = 'x86_64'
endian = 'little'
[properties]
skip_sanity_check = true
needs_exe_wrapper = true
link_spec = '--build-id=none'
default_flash_addr = '0x00100000'
default_flash_size = '0x00400000'
default_ram_addr = '0x00500000'
default_ram_size = '0x00200000'
EOF
    rm -rf "$SRC/build-x86-host"
    meson setup "$SRC/build-x86-host" "$SRC" \
        --cross-file="$SRC/cross-x86_64-host.txt" \
        -Dtests=false -Dmultilib=false -Dthread-local-storage=false \
        --prefix="$PREFIX" >&2
    ninja -C "$SRC/build-x86-host" >&2
    # The prefix install runs before the root-only compiler-specs copy; ignore
    # that last failure and verify the artifacts below. The generated
    # `picolibc.h` (included by `<newlib.h>`) can be skipped when install
    # stops early, so copy it explicitly.
    ninja -C "$SRC/build-x86-host" install >&2 || true
    cp "$SRC/build-x86-host/picolibc.h" "$PREFIX/include/picolibc.h"
    # The specs' `-Tpicolibc.ld` is resolved through the prefix lib directory;
    # the root-only gcc-specs install step can stop before it is copied.
    cp "$SRC/build-x86-host/picolibc.ld" "$PREFIX/lib/picolibc.ld"
    cp "$SRC/build-x86-host/picolibc.specs" "$PREFIX/lib/picolibc.specs"
fi

[ -f "$PREFIX/lib/libc.a" ] && [ -f "$PREFIX/lib/libsemihost.a" ] \
    || { echo "picolibc x86_64 构建不完整：缺 $PREFIX/lib/libc.a 或 libsemihost.a" >&2; exit 2; }

mkdir -p "$OUT_DIR"
# The installed specs already carries the absolute vendor prefix, so a reused
# prefix needs no meson build tree. Copy from the prefix, not from
# $SRC/build-x86-host: a previous run's source checkout may live elsewhere or
# be gone, while the prefix is the artifact this script guarantees.
cp "$PREFIX/lib/picolibc.specs" "$SPECS"
echo "$PREFIX"
echo "$SPECS"
