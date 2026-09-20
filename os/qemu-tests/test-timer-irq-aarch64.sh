#!/bin/bash
# test-timer-irq-aarch64.sh — aarch64 CNTP timer tick through the
# production VBAR_EL1 legs (E-3ARCHTRAP / NK3; x86 test-timer-irq
# sibling).
#
# Boots the UEFI aarch64 carrier under QEMU virt + AAVMF. The carrier
# arms CNTP through the Generic Timer, opens the GIC gates, and every
# tick lands in the production current-EL IRQ slot (full frame save,
# ICC_IAR1 claim → kernel dispatch body → hook chain → ICC_EOIR1 → CNTP
# re-arm → uptime).
#
# PASS: serial line `### TEST_RESULT: PASS test-timer-irq-aarch64 ###`
# with at least one per-tick uptime line.
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EFI="${2:-$ROOT/target/aarch64-unknown-uefi/release/test-timer-irq-aarch64.efi}"
TIMEOUT_RUN="${TIMEOUT_RUN:-60}"

if ! command -v qemu-system-aarch64 &>/dev/null; then
    echo "SKIP: qemu-system-aarch64 not found"; exit 2
fi
FW=""; FW_SRC_VARS=""
for c in /usr/share/AAVMF/AAVMF_CODE.fd /usr/share/qemu-efi-aarch64/QEMU_EFI.fd; do
    [ -f "$c" ] && FW="$c" && break
done
for c in /usr/share/AAVMF/AAVMF_VARS.fd /usr/share/qemu-efi-aarch64/QEMU_VARS.fd; do
    [ -f "$c" ] && FW_SRC_VARS="$c" && break
done
if [ -z "$FW" ] || [ -z "$FW_SRC_VARS" ]; then
    echo "SKIP: AAVMF firmware not found"; exit 2
fi
[ -f "$EFI" ] || { echo "SKIP: carrier EFI not found: $EFI"; exit 2; }

STAGING="$(mktemp -d /tmp/timer_irq_a64_staging.XXXXXX)"
mkdir -p "$STAGING/EFI/BOOT"
DISK_IMG="$(mktemp /tmp/timer_irq_a64_disk.XXXXXX.img)"
if command -v mkfs.vfat &>/dev/null && command -v mmd &>/dev/null && command -v mcopy &>/dev/null; then
    dd if=/dev/zero of="$DISK_IMG" bs=1M count=64 status=none 2>/dev/null
    mkfs.vfat "$DISK_IMG" >/dev/null 2>&1
    mmd -i "$DISK_IMG" ::EFI ::EFI/BOOT >/dev/null 2>&1
    mcopy -i "$DISK_IMG" "$EFI" "::EFI/BOOT/BOOTAA64.EFI" >/dev/null
    printf 'echo -off\r\nFS0:\r\ncd EFI\\BOOT\r\nBOOTAA64.EFI\r\n' > "$STAGING/startup.nsh"
    mcopy -i "$DISK_IMG" "$STAGING/startup.nsh" "::startup.nsh" >/dev/null
    DISK_DRIVE="file=$DISK_IMG,format=raw,media=disk"
else
    DISK_DRIVE="file=fat:rw:$STAGING,format=raw,media=disk"
fi
FW_VARS="$(mktemp /tmp/timer_irq_a64_vars.XXXXXX.fd)"
cp "$FW_SRC_VARS" "$FW_VARS"
SERIAL_LOG="$(mktemp /tmp/timer_irq_a64_serial.XXXXXX.log)"

pkill -9 qemu-system-aar 2>/dev/null || true
sleep 1

timeout "$TIMEOUT_RUN" qemu-system-aarch64 \
    -machine virt \
    -cpu cortex-a57 \
    -smp 1 \
    -m 256M \
    -nographic \
    -drive "if=pflash,format=raw,unit=0,file=$FW,readonly=on" \
    -drive "if=pflash,format=raw,unit=1,file=$FW_VARS" \
    -drive "$DISK_DRIVE" \
    -serial "file:$SERIAL_LOG" \
    -display none \
    -no-reboot &
QEMU_PID=$!

cleanup() {
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
    rm -rf "$STAGING" "$DISK_IMG" "$FW_VARS" "$SERIAL_LOG"
}
trap cleanup EXIT

for _ in $(seq 1 "$TIMEOUT_RUN"); do
    if grep -qa "TEST_RESULT: PASS test-timer-irq-aarch64" "$SERIAL_LOG" 2>/dev/null; then
        grep -a "tick: uptime" "$SERIAL_LOG" | head -5
        echo "### TEST_RESULT: PASS test-timer-irq-aarch64 ###"
        exit 0
    fi
    if grep -qa "TEST_RESULT: FAIL\|### PANIC" "$SERIAL_LOG" 2>/dev/null; then
        break
    fi
    sleep 1
done

echo "### TEST_RESULT: FAIL test-timer-irq-aarch64 ###"
grep -a "PANIC\|esr\|tick:" "$SERIAL_LOG" | tail -5
exit 1
