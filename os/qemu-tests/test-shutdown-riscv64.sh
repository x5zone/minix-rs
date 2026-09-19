#!/bin/bash
# test-shutdown-riscv64.sh — riscv64 semantic shutdown through the QEMU
# test backend (edge1 K11: sifive_test FINISHER_PASS; S-11 §3.8 QEMU layer).
#
# PASS requires BOTH: the serial marker and QEMU terminating ON ITS OWN
# with exit code 0 (a guest that hangs cannot fake this — the timeout
# would fire and the run is a FAIL).
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KERNEL="${2:-$ROOT/target/riscv64gc-unknown-none-elf/release/test-shutdown-riscv64}"
TIMEOUT_RUN="${TIMEOUT_RUN:-30}"

if ! command -v qemu-system-riscv64 &>/dev/null; then
    echo "SKIP: qemu-system-riscv64 not found"; exit 2
fi
if [ ! -f "$KERNEL" ]; then
    echo "SKIP: carrier ELF not found: $KERNEL"; exit 2
fi

SERIAL_LOG="$(mktemp /tmp/shutdown_rv_serial.XXXXXX.log)"
pkill -9 qemu-system-ris 2>/dev/null || true
sleep 1

timeout "$TIMEOUT_RUN" qemu-system-riscv64 \
    -machine virt \
    -smp 1 \
    -m 128M \
    -nographic \
    -bios default \
    -kernel "$KERNEL" \
    -serial "file:$SERIAL_LOG" \
    -display none \
    -no-reboot
RC=$?

pass=1
grep -qa "TEST_RESULT: PASS test-shutdown-riscv64" "$SERIAL_LOG" || { pass=0; echo "FAIL: serial marker missing"; }
if [ "$RC" -ne 0 ]; then
    pass=0
    echo "FAIL: QEMU exit code $RC (want 0 from FINISHER_PASS; 124 = timeout = guest never shut down)"
fi
rm -f "$SERIAL_LOG"

if [ "$pass" -eq 1 ]; then
    echo "### TEST_RESULT: PASS test-shutdown-riscv64 ###"
    exit 0
fi
echo "### TEST_RESULT: FAIL test-shutdown-riscv64 ###"
exit 1
