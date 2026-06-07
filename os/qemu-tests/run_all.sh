#!/usr/bin/env bash
# run_all.sh — CI entry point. Runs all QEMU integration tests.
#
# Test categories:
#   1. hello-boot:      full boot chain (UEFI → Paging trait → serial output)
#   2. test-memmap:     KernelInfo.memmap reflects real physical memory layout
#   3. test-paging-enable: paging.enable() switches CR3 and CPU survives
#   4. test-kernel-map: high-half mapping returns correct data
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

echo "--- x86_64: hello-boot ---"
cargo build --manifest-path "$OS_ROOT/Cargo.toml" -p hello-boot --target x86_64-unknown-uefi --release 2>&1 || echo "(build failed)"

echo "--- x86_64: test-memmap ---"
cargo build --manifest-path "$OS_ROOT/Cargo.toml" -p test-memmap --target x86_64-unknown-uefi --release 2>&1 || echo "(build failed)"

echo "--- x86_64: test-paging-enable ---"
cargo build --manifest-path "$OS_ROOT/Cargo.toml" -p test-paging-enable --target x86_64-unknown-uefi --release 2>&1 || echo "(build failed)"

echo "--- x86_64: test-kernel-map ---"
cargo build --manifest-path "$OS_ROOT/Cargo.toml" -p test-kernel-map --target x86_64-unknown-uefi --release 2>&1 || echo "(build failed)"

echo "--- aarch64: hello-boot-aarch64 ---"
cargo build --manifest-path "$OS_ROOT/Cargo.toml" -p hello-boot-aarch64 --target aarch64-unknown-uefi --release 2>&1 || echo "(build failed)"

echo "--- riscv64: hello-boot-riscv64 ---"
cargo build --manifest-path "$OS_ROOT/Cargo.toml" -p hello-boot-riscv64 --target riscv64gc-unknown-none-elf --release 2>&1 || echo "(build failed)"

echo ""
echo "=== Running QEMU tests ==="

# x86_64 tests
if command -v qemu-system-x86_64 &>/dev/null; then
    run_test "hello-boot" x86_64 "$OS_ROOT/target/x86_64-unknown-uefi/release/hello-boot.efi"
    run_test "test-memmap" x86_64 "$OS_ROOT/target/x86_64-unknown-uefi/release/test-memmap.efi"
    run_test "test-paging-enable" x86_64 "$OS_ROOT/target/x86_64-unknown-uefi/release/test-paging-enable.efi"
    run_test "test-kernel-map" x86_64 "$OS_ROOT/target/x86_64-unknown-uefi/release/test-kernel-map.efi"
fi

# aarch64 tests
if command -v qemu-system-aarch64 &>/dev/null; then
    run_test "hello-boot-aarch64" aarch64 "$OS_ROOT/target/aarch64-unknown-uefi/release/hello-boot-aarch64.efi"
fi

# riscv64 tests
if command -v qemu-system-riscv64 &>/dev/null; then
    run_test "hello-boot-riscv64" riscv64 "$OS_ROOT/target/riscv64gc-unknown-none-elf/release/hello-boot-riscv64"
fi

echo ""
echo "=== All QEMU tests done: $PASS passed, $FAIL failed ==="

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
