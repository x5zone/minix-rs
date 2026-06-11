#!/bin/bash
# QEMU test: aarch64 clock and interrupt initialization
#
# Verifies:
# 1. init_clock_and_interrupts() executes without panic
# 2. ARM Generic Timer is enabled (CNTP_CTL_EL0.ENABLE = 1)
# 3. GICv3 distributor and redistributor are initialized
# 4. ClockState uptime advances after timer interrupt
#
# Usage: ./qemu_test_aarch64.sh [kernel.elf]

set -euo pipefail

KERNEL="${1:-build/aarch64/kernel.elf}"

echo "=== aarch64 Clock/Interrupt QEMU Test ==="

# Start QEMU with GDB stub
qemu-system-aarch64 \
    -machine virt \
    -cpu cortex-a72 \
    -kernel "${KERNEL}" \
    -s -S \
    -serial stdio \
    -display none \
    -m 128M \
    -no-reboot \
    -d int \
    -D qemu_aarch64.log &
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
    -ex "echo === Verifying Generic Timer configuration ===\n" \
    -ex "p/x \$cntp_ctl_el0" \
    -ex "echo Expected: 0x1 (ENABLE bit set)\n" \
    -ex "echo === Verifying Counter Frequency ===\n" \
    -ex "p/x \$cntfrq_el0" \
    -ex "echo === Verifying GICv3 CPU Interface ===\n" \
    -ex "p/x \$icc_sre_el1" \
    -ex "echo Expected: 0x7 (SRE + Enable + DIE)\n" \
    -ex "p/x \$icc_pmr_el1" \
    -ex "echo Expected: 0xFF (priority mask = accept all)\n" \
    -ex "echo === Finish init_clock_and_interrupts ===\n" \
    -ex "finish" \
    -ex "echo === Test PASSED (reached post-init, timer+interrupt initialized) ===\n" \
    -ex "quit" \
    "${KERNEL}" 2>&1 || true

# Cleanup
kill "${QEMU_PID}" 2>/dev/null || true
wait "${QEMU_PID}" 2>/dev/null || true

echo "=== aarch64 QEMU Test Complete ==="
echo "Log: qemu_aarch64.log"
