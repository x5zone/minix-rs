#!/bin/bash
# test-timer-irq-riscv64.sh — riscv64 S-mode timer tick through the
# production stvec kernel leg (E-3ARCHTRAP / NK3; x86 test-timer-irq
# sibling).
#
# Boots the bare-metal riscv64 carrier under QEMU virt (OpenSBI -bios
# default, ELF via -kernel). The carrier arms the timer through the SBI
# TIME ecall, opens sie.STIE + sstatus.SIE, and every tick lands in the
# production riscv64_kernel_trap_vector (full frame save, kernel dispatch
# body, SBI re-arm, IrqManager hook chain → uptime).
#
# PASS: serial line `### TEST_RESULT: PASS test-timer-irq-riscv64 ###`
# with at least one per-tick uptime line.
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KERNEL="${2:-$ROOT/target/riscv64gc-unknown-none-elf/release/test-timer-irq-riscv64}"
TIMEOUT_RUN="${TIMEOUT_RUN:-30}"

if ! command -v qemu-system-riscv64 &>/dev/null; then
    echo "SKIP: qemu-system-riscv64 not found"; exit 2
fi
if [ ! -f "$KERNEL" ]; then
    echo "SKIP: carrier ELF not found: $KERNEL"; exit 2
fi

SERIAL_LOG="$(mktemp /tmp/timer_irq_rv_serial.XXXXXX.log)"

qemu-system-riscv64 \
    -machine virt \
    -smp 1 \
    -m 256M \
    -nographic \
    -bios default \
    -kernel "$KERNEL" \
    -serial file:"$SERIAL_LOG" \
    -display none \
    -no-reboot &
QEMU_PID=$!

cleanup() {
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
    rm -f "$SERIAL_LOG"
}
trap cleanup EXIT

# The carrier prints its own verdict line; bounded wait for it.
for _ in $(seq 1 "$TIMEOUT_RUN"); do
    if grep -qa "TEST_RESULT: PASS test-timer-irq-riscv64" "$SERIAL_LOG" 2>/dev/null; then
        grep -a "tick: uptime" "$SERIAL_LOG" | head -5
        echo "### TEST_RESULT: PASS test-timer-irq-riscv64 ###"
        exit 0
    fi
    if grep -qa "TEST_RESULT: FAIL\|### PANIC" "$SERIAL_LOG" 2>/dev/null; then
        break
    fi
    sleep 1
done

echo "### TEST_RESULT: FAIL test-timer-irq-riscv64 ###"
grep -a "PANIC\|scause\|tick:" "$SERIAL_LOG" | tail -5
exit 1
