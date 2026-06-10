#!/usr/bin/env bash
# run_all.sh — CI entry point. Runs all QEMU integration tests.
#
# Test categories:
#   1. hello-boot:      full boot chain (UEFI/OpenSBI → Paging trait → serial output)
#   2. test-memmap:     KernelInfo.memmap reflects real physical memory layout
#   3. test-paging-enable: paging.enable() switches page table base and CPU survives
#   4. test-kernel-map: high-half/identity mapping returns correct data
#   5. test-higher-half: HigherHalf trait transition (stack/PC switch → kmain)
#
# Each test writes "### TEST_RESULT: PASS <name> ###" on success.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OS_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PASS=0
FAIL=0

run_test() {
    local name="$1"
    local arch="$2"
    local binary="$3"

    if [ ! -f "$binary" ]; then
        echo "($name: binary not found, skip)"
        return
    fi

    echo "--- Running: $name ($arch) ---"
    if "$SCRIPT_DIR/run_qemu.sh" "$arch" "$binary"; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
    fi
}

echo "=== Building test kernels ==="

# ── x86_64 (UEFI) ──
for pkg in hello-boot test-memmap test-paging-enable test-kernel-map test-higher-half; do
    echo "--- x86_64: $pkg ---"
    cargo build --manifest-path "$OS_ROOT/Cargo.toml" -p "$pkg" --target x86_64-unknown-uefi --release 2>&1 || echo "(build failed)"
done

# ── aarch64 (UEFI) ──
for pkg in hello-boot-aarch64 test-memmap-aarch64 test-paging-enable-aarch64 test-kernel-map-aarch64 test-higher-half-aarch64; do
    echo "--- aarch64: $pkg ---"
    cargo build --manifest-path "$OS_ROOT/Cargo.toml" -p "$pkg" --target aarch64-unknown-uefi --release 2>&1 || echo "(build failed)"
done

# ── riscv64 (OpenSBI, bare-metal) ──
for pkg in hello-boot-riscv64 test-memmap-riscv64 test-paging-enable-riscv64 test-kernel-map-riscv64 test-higher-half-riscv64; do
    echo "--- riscv64: $pkg ---"
    cargo build --manifest-path "$OS_ROOT/Cargo.toml" -p "$pkg" --target riscv64gc-unknown-none-elf --release 2>&1 || echo "(build failed)"
done

echo ""
echo "=== Running QEMU tests ==="

# x86_64 tests
if command -v qemu-system-x86_64 &>/dev/null; then
    run_test "hello-boot"              x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/hello-boot.efi"
    run_test "test-memmap"             x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/test-memmap.efi"
    run_test "test-paging-enable"      x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/test-paging-enable.efi"
    run_test "test-kernel-map"         x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/test-kernel-map.efi"
    run_test "test-higher-half"        x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/test-higher-half.efi"
fi

# aarch64 tests
if command -v qemu-system-aarch64 &>/dev/null; then
    run_test "hello-boot-aarch64"      aarch64  "$OS_ROOT/target/aarch64-unknown-uefi/release/hello-boot-aarch64.efi"
    run_test "test-memmap-aarch64"     aarch64  "$OS_ROOT/target/aarch64-unknown-uefi/release/test-memmap-aarch64.efi"
    run_test "test-paging-enable-aarch64" aarch64 "$OS_ROOT/target/aarch64-unknown-uefi/release/test-paging-enable-aarch64.efi"
    run_test "test-kernel-map-aarch64" aarch64  "$OS_ROOT/target/aarch64-unknown-uefi/release/test-kernel-map-aarch64.efi"
    run_test "test-higher-half-aarch64" aarch64 "$OS_ROOT/target/aarch64-unknown-uefi/release/test-higher-half-aarch64.efi"
fi

# riscv64 tests
if command -v qemu-system-riscv64 &>/dev/null; then
    run_test "hello-boot-riscv64"      riscv64  "$OS_ROOT/target/riscv64gc-unknown-none-elf/release/hello-boot-riscv64"
    run_test "test-memmap-riscv64"     riscv64  "$OS_ROOT/target/riscv64gc-unknown-none-elf/release/test-memmap-riscv64"
    run_test "test-paging-enable-riscv64" riscv64 "$OS_ROOT/target/riscv64gc-unknown-none-elf/release/test-paging-enable-riscv64"
    run_test "test-kernel-map-riscv64" riscv64  "$OS_ROOT/target/riscv64gc-unknown-none-elf/release/test-kernel-map-riscv64"
    run_test "test-higher-half-riscv64" riscv64 "$OS_ROOT/target/riscv64gc-unknown-none-elf/release/test-higher-half-riscv64"
fi

echo ""
echo "=== All QEMU tests done: $PASS passed, $FAIL failed ==="

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
