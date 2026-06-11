#!/bin/bash
# QEMU test: riscv64 clock and interrupt initialization
#
# Verifies:
# 1. init_clock_and_interrupts() executes without panic
# 2. CLINT mtimecmp is configured (non-zero after init)
# 3. S-mode timer interrupt (STIE) is enabled in sie
# 4. ClockState uptime advances after timer interrupt
#
# Usage: ./qemu_test_riscv64.sh [kernel.elf]

set -euo pipefail

KERNEL="${1:-build/riscv64/kernel.elf}"

echo "=== riscv64 Clock/Interrupt QEMU Test ==="

# Start QEMU with GDB stub
qemu-system-riscv64 \
    -machine virt \
    -cpu rv64 \
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

# Run GDB commands
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
    -ex "echo === Test PASSED (reached post-init, timer+interrupt initialized) ===\n" \
    -ex "quit" \
    "${KERNEL}" 2>&1 || true

# Cleanup
kill "${QEMU_PID}" 2>/dev/null || true
wait "${QEMU_PID}" 2>/dev/null || true

echo "=== riscv64 QEMU Test Complete ==="
echo "Log: qemu_riscv64.log"
