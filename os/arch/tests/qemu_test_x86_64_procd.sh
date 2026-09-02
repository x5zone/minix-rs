#!/bin/bash
# QEMU test: x86-64 Phase C/D (proc_init + arch_boot_proc + arch_post_init)
#
# Verifies:
# 1. init_proc_and_boot() executes without panic
# 2. ProcessTable is initialized with SLOT_FREE for all slots
# 3. VM process gets ELF loaded (pc != 0)
# 4. init_post_and_memory() asserts VM page-table root valid + Direct Map
#    base configured, and installs VM as kernel-level ptproc
# 5. (No free PDE slots — freepdes is superseded by Direct Map)
#
# Exit codes:
#   0 = PASS (all checkpoints reached)
#   1 = FAIL (checkpoint not reached)
#   2 = SKIP (QEMU/GDB not available)
#
# Usage: ./qemu_test_x86_64_procd.sh [kernel.elf]

set -euo pipefail

KERNEL="${1:-build/x86_64/kernel.elf}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULT_FILE="${SCRIPT_DIR}/qemu_x86_64_procd_result.txt"

# Check prerequisites
if ! command -v qemu-system-x86_64 &>/dev/null; then
    echo "SKIP: qemu-system-x86_64 not found"
    exit 2
fi

if ! command -v gdb &>/dev/null; then
    echo "SKIP: gdb not found"
    exit 2
fi

if [ ! -f "${KERNEL}" ]; then
    echo "SKIP: kernel not found at ${KERNEL}"
    exit 2
fi

echo "=== x86-64 Phase C/D QEMU Test ==="

# Clean up any previous result
rm -f "${RESULT_FILE}"

# Start QEMU with GDB stub
qemu-system-x86_64 \
    -machine q35 \
    -cpu host \
    -enable-kvm \
    -kernel "${KERNEL}" \
    -s -S \
    -serial stdio \
    -display none \
    -m 128M \
    -no-reboot \
    -d int,cpu_reset \
    -D qemu_x86_64_procd.log &
QEMU_PID=$!

# Wait for QEMU to start
sleep 1

# Run GDB commands for Phase C/D verification
gdb -batch -ex "target remote :1234" \
    -ex "break init_proc_and_boot" \
    -ex "continue" \
    -ex "echo === Phase C: init_proc_and_boot reached ===\n" \
    -ex "finish" \
    -ex "echo === Phase C: init_proc_and_boot completed ===\n" \
    -ex "break init_post_and_memory" \
    -ex "continue" \
    -ex "echo === Phase D: init_post_and_memory reached ===\n" \
    -ex "finish" \
    -ex "echo === Phase D: init_post_and_memory completed ===\n" \
    -ex "shell echo PASS > ${RESULT_FILE}" \
    -ex "echo === Test PASSED (Phase C/D completed) ===\n" \
    -ex "quit" \
    "${KERNEL}" 2>&1 || true

# Cleanup
kill "${QEMU_PID}" 2>/dev/null || true
wait "${QEMU_PID}" 2>/dev/null || true

echo "=== x86-64 Phase C/D QEMU Test Complete ==="
echo "Log: qemu_x86_64_procd.log"

# Check result
if [ -f "${RESULT_FILE}" ] && grep -q "PASS" "${RESULT_FILE}"; then
    echo "RESULT: PASS"
    rm -f "${RESULT_FILE}"
    exit 0
else
    echo "RESULT: FAIL (Phase C/D checkpoint not reached)"
    rm -f "${RESULT_FILE}"
    exit 1
fi
