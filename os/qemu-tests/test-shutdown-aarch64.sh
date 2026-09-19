#!/bin/bash
# test-shutdown-aarch64.sh — aarch64 semantic shutdown through the QEMU
# test backend (edge1 K11: semihosting SYS_EXIT; S-11 §3.8 QEMU layer).
#
# PASS requires BOTH: the serial marker and QEMU terminating ON ITS OWN
# with exit code 0 under `-semihosting-config enable=on,target=native`.
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EFI="${2:-$ROOT/target/aarch64-unknown-uefi/release/test-shutdown-aarch64.efi}"
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

STAGING="$(mktemp -d /tmp/shutdown_a64_staging.XXXXXX)"
mkdir -p "$STAGING/EFI/BOOT"
DISK_IMG="$(mktemp /tmp/shutdown_a64_disk.XXXXXX.img)"
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
FW_VARS="$(mktemp /tmp/shutdown_a64_vars.XXXXXX.fd)"
cp "$FW_SRC_VARS" "$FW_VARS"
SERIAL_LOG="$(mktemp /tmp/shutdown_a64_serial.XXXXXX.log)"

pkill -9 qemu-system-aar 2>/dev/null || true
sleep 1

timeout "$TIMEOUT_RUN" qemu-system-aarch64 \
    -machine virt \
    -cpu cortex-a57 \
    -smp 1 \
    -m 256M \
    -nographic \
    -semihosting-config enable=on,target=native \
    -drive "if=pflash,format=raw,unit=0,file=$FW,readonly=on" \
    -drive "if=pflash,format=raw,unit=1,file=$FW_VARS" \
    -drive "$DISK_DRIVE" \
    -serial "file:$SERIAL_LOG" \
    -display none \
    -no-reboot
RC=$?

pass=1
grep -qa "TEST_RESULT: PASS test-shutdown-aarch64" "$SERIAL_LOG" || { pass=0; echo "FAIL: serial marker missing"; }
if [ "$RC" -ne 0 ]; then
    pass=0
    echo "FAIL: QEMU exit code $RC (want 0 from semihosting SYS_EXIT; 124 = timeout = guest never shut down)"
fi
rm -rf "$STAGING" "$DISK_IMG" "$FW_VARS" "$SERIAL_LOG"

if [ "$pass" -eq 1 ]; then
    echo "### TEST_RESULT: PASS test-shutdown-aarch64 ###"
    exit 0
fi
echo "### TEST_RESULT: FAIL test-shutdown-aarch64 ###"
exit 1
