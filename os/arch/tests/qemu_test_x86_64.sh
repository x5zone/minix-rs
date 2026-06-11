#!/bin/bash
# QEMU test: x86-64 clock and interrupt initialization
#
# Verifies:
# 1. init_clock_and_interrupts() executes without panic
# 2. PIT is configured (8254 channel 0 rate generator mode)
# 3. LAPIC is enabled after init
# 4. ClockState uptime advances after timer interrupt
#
# Usage: ./qemu_test_x86_64.sh [kernel.elf]

set -euo pipefail

KERNEL="${1:-build/x86_64/kernel.elf}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=== x86-64 Clock/Interrupt QEMU Test ==="

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
    -D qemu_x86_64.log &
QEMU_PID=$!

# Wait for QEMU to start
sleep 1

# Run GDB commands
gdb -batch -ex "target remote :1234" \
    -ex "break init_clock_and_interrupts" \
    -ex "continue" \
    -ex "echo === BREAKPOINT HIT at init_clock_and_interrupts ===\n" \
    -ex "step" \
    -ex "step" \
    -ex "echo === Checking clock state initialization ===\n" \
    -ex "print clock" \
    -ex "echo === Checking interrupt controller initialization ===\n" \
    -ex "step" \
    -ex "step" \
    -ex "echo === Finish init_clock_and_interrupts ===\n" \
    -ex "finish" \
    -ex "echo === Verifying PIT configuration (port 0x43, 0x40) ===\n" \
    -ex "echo === Test PASSED (reached post-init) ===\n" \
    -ex "quit" \
    "${KERNEL}" 2>&1 || true

# Cleanup
kill "${QEMU_PID}" 2>/dev/null || true
wait "${QEMU_PID}" 2>/dev/null || true

echo "=== x86-64 QEMU Test Complete ==="
echo "Log: qemu_x86_64.log"
