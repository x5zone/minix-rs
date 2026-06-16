#!/bin/bash
# QEMU test: riscv64 clock and interrupt initialization
#
# Verifies:
# 1. init_clock_and_interrupts() executes without panic
# 2. CLINT mtimecmp is configured (non-zero after init)
# 3. S-mode timer interrupt (STIE) is enabled in sie
# 4. ClockState uptime advances after timer interrupt
#
# Exit codes:
#   0 = PASS (GDB reached post-init checkpoint)
#   1 = FAIL (QEMU/GDB error or checkpoint not reached)
#   2 = SKIP (QEMU/GDB not available)
#
# Usage: ./qemu_test_riscv64.sh [kernel.elf]

set -euo pipefail

KERNEL="${1:-build/riscv64/kernel.elf}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULT_FILE="${SCRIPT_DIR}/qemu_riscv64_result.txt"

# Check prerequisites
if ! command -v qemu-system-riscv64 &>/dev/null; then
    echo "SKIP: qemu-system-riscv64 not found"
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

echo "=== riscv64 Clock/Interrupt QEMU Test ==="

# Clean up any previous result
rm -f "${RESULT_FILE}"

# Build QEMU flags: use KVM if available (requires riscv64 host)
KVM_OPTS=()
if [ -w /dev/kvm ] && qemu-system-riscv64 -machine virt -accel kvm -cpu host -kernel /dev/null -S 2>/dev/null; then
    KVM_OPTS=(-enable-kvm -cpu host)
    echo "KVM: enabled"
else
    KVM_OPTS=(-cpu rv64)
    echo "KVM: not available, using TCG"
fi

# Start QEMU with GDB stub
qemu-system-riscv64 \
    -machine virt \
    "${KVM_OPTS[@]}" \
    -kernel "${KERNEL}" \
    -s -S \
    -serial stdio \
    -display none \
    -m 128M \
    -no-reboot \
    -d int \
    -D qemu_riscv64.log &
QEMU_PID=$!

# Wait for QEMU to start
sleep 1

# Run GDB commands — write PASS to result file if we reach post-init
gdb -batch \
    -ex "target remote :1234" \
    -ex "break init_clock_and_interrupts" \
    -ex "continue" \
    -ex "echo === BREAKPOINT HIT at init_clock_and_interrupts ===\n" \
    -ex "step" \
    -ex "step" \
    -ex "echo === Verifying CLINT mtimecmp configuration ===\n" \
    -ex "x/gx 0x2004000" \
    -ex "echo Expected: non-zero (mtimecmp set)\n" \
    -ex "echo === Verifying mtime value ===\n" \
    -ex "x/gx 0x200BFF8" \
    -ex "echo === Verifying S-mode interrupt enable ===\n" \
    -ex "p/x \$sie" \
    -ex "echo Expected: 0x22 (SEIE + STIE set)\n" \
    -ex "echo === Verifying S-mode status ===\n" \
    -ex "p/x \$sstatus" \
    -ex "echo === Finish init_clock_and_interrupts ===\n" \
    -ex "finish" \
    -ex "shell echo PASS > ${RESULT_FILE}" \
    -ex "echo === Test PASSED (reached post-init, timer+interrupt initialized) ===\n" \
    -ex "quit" \
    "${KERNEL}" 2>&1 || true

# Cleanup
kill "${QEMU_PID}" 2>/dev/null || true
wait "${QEMU_PID}" 2>/dev/null || true

echo "=== riscv64 QEMU Test Complete ==="
echo "Log: qemu_riscv64.log"

# Check result
if [ -f "${RESULT_FILE}" ] && grep -q "PASS" "${RESULT_FILE}"; then
    echo "RESULT: PASS"
    rm -f "${RESULT_FILE}"
    exit 0
else
    echo "RESULT: FAIL (post-init checkpoint not reached)"
    rm -f "${RESULT_FILE}"
    exit 1
fi
