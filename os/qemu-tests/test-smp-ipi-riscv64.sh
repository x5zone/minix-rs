#!/bin/bash
# test-smp-ipi-riscv64.sh — riscv64 SSIE software-interrupt IPI round-trip
# (edge1 K10 round 4: ACLINT SSWI direct-write delivery).
#
# Boots the bare-metal riscv64 carrier under QEMU virt with `aclint=on`
# (OpenSBI -bios default, ELF via -kernel). The carrier:
#   1. parses the DTB and installs the platform global (the production
#      send_sched_ipi reads the ACLINT SSWI SETIP base from it),
#   2. starts the non-boot hart through SBI HSM hart_start,
#   3. the target hart enables sie.SSIE + sstatus.SIE (bit 1!), points
#      stvec at the IPI handler, and parks in wfi,
#   4. the BSP sends the schedule IPI through the production
#      `SmpArch::send_sched_ipi` — SSWI direct write (the SBI send_ipi
#      path cannot reach an S-mode hart on aclint-mswi firmware),
#   5. the target traps (scause must be 0x8000_0000_0000_0001 —
#      interrupt bit + Supervisor Software Interrupt), clears sip.SSIP,
#      and signals the BSP.
#
# PASS: serial line `### TEST_RESULT: PASS test-smp-ipi-riscv64 ###`.
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KERNEL="${2:-$ROOT/target/riscv64gc-unknown-none-elf/release/test-smp-ipi-riscv64}"
TIMEOUT_RUN="${TIMEOUT_RUN:-30}"

if ! command -v qemu-system-riscv64 &>/dev/null; then
    echo "SKIP: qemu-system-riscv64 not found"; exit 2
fi
if [ ! -f "$KERNEL" ]; then
    echo "SKIP: carrier ELF not found: $KERNEL"; exit 2
fi

SERIAL_LOG="$(mktemp /tmp/smp_ipi_rv_serial.XXXXXX.log)"

qemu-system-riscv64 \
    -machine virt,aclint=on \
    -smp ${SMP:-2} \
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
    if grep -qa "TEST_RESULT: PASS" "$SERIAL_LOG" 2>/dev/null; then
        grep -a "scause = " "$SERIAL_LOG"
        echo "### TEST_RESULT: PASS test-smp-ipi-riscv64 ###"
        exit 0
    fi
    if grep -qa "TEST_RESULT: FAIL\|### FAIL" "$SERIAL_LOG" 2>/dev/null; then
        break
    fi
    sleep 1
done

echo "### TEST_RESULT: FAIL test-smp-ipi-riscv64 ###"
grep -a "FAIL\|diag:" "$SERIAL_LOG" | tail -3
exit 1
