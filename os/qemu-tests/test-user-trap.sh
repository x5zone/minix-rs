#!/bin/bash
# test-user-trap.sh — E1 slice 5: user-mode int-33 trap bridge bring-up.
#
# Boots test-user-trap.efi (OVMF, same staging as run_qemu.sh). The guest
# kernel maps CPL3 code/data/stack pages for the VM boot process, enters
# the scheduling loop, and the payload traps twice through vector 33:
#   round 1: undefined call 99  → EBADCALL(209) in RAX
#   round 2: MINIX_KERNINFO (6) → EBADCALL(209), page unpublished
# The payload writes both errnos plus a 0xDEAD completion marker into a
# user mailbox page (physical 0x400_1000).
#
# PASS determination: this script polls the QEMU monitor (`xp` = physical
# memory dump) for the three mailbox quadwords. The guest never exits
# (the payload spins), so the script kills QEMU after reading the values.
#
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EFI="${2:-$ROOT/target/x86_64-unknown-uefi/release/test-user-trap.efi}"
TIMEOUT_BOOT="${TIMEOUT_BOOT:-90}"

if ! command -v qemu-system-x86_64 &>/dev/null; then
    echo "SKIP: qemu-system-x86_64 not found"; exit 2
fi

FW=""; FW_SRC_VARS=""
for c in /usr/share/OVMF/OVMF_CODE_4M.fd /usr/share/OVMF/OVMF_CODE.fd; do
    [ -f "$c" ] && FW="$c" && break
done
for c in /usr/share/OVMF/OVMF_VARS_4M.fd /usr/share/OVMF/OVMF_VARS.fd; do
    [ -f "$c" ] && FW_SRC_VARS="$c" && break
done
if [ -z "$FW" ] || [ -z "$FW_SRC_VARS" ]; then
    echo "SKIP: OVMF firmware not found"; exit 2
fi

# Stage the EFI on a FAT disk (run_qemu.sh pattern).
STAGING="$(mktemp -d /tmp/user_trap_staging.XXXXXX)"
mkdir -p "$STAGING/EFI/BOOT"
DISK_IMG="$(mktemp /tmp/user_trap_disk.XXXXXX.img)"
if command -v mkfs.vfat &>/dev/null && command -v mmd &>/dev/null && command -v mcopy &>/dev/null; then
    dd if=/dev/zero of="$DISK_IMG" bs=1M count=64 status=none 2>/dev/null
    mkfs.vfat "$DISK_IMG" >/dev/null 2>&1
    mmd -i "$DISK_IMG" ::EFI ::EFI/BOOT >/dev/null 2>&1
    mcopy -v -i "$DISK_IMG" "$EFI" "::EFI/BOOT/BOOTX64.EFI" || echo "FAIL: mcopy BOOTX64"
    printf 'echo -off\r\nFS0:\r\ncd EFI\\BOOT\r\nBOOTX64.EFI\r\n' > "$STAGING/startup.nsh"
    mcopy -i "$DISK_IMG" "$STAGING/startup.nsh" "::startup.nsh" || echo "FAIL: mcopy startup.nsh"
    mdir -i "$DISK_IMG" ::EFI/BOOT || true
    DISK_DRIVE="file=$DISK_IMG,format=raw,media=disk"
else
    DISK_DRIVE="file=fat:rw:$STAGING,format=raw,media=disk"
fi

FW_VARS="$(mktemp /tmp/user_trap_vars.XXXXXX.fd)"
cp "$FW_SRC_VARS" "$FW_VARS"
SERIAL_LOG="$(mktemp /tmp/user_trap_serial.XXXXXX.log)"
MON_SOCK="$(mktemp /tmp/user_trap_mon.XXXXXX.sock)"
rm -f "$MON_SOCK"

qemu-system-x86_64 \
    -machine q35 \
    -net none \
    -smp 4 \
    -m 256M \
    -drive "if=pflash,format=raw,unit=0,file=$FW,readonly=on" \
    -drive "if=pflash,format=raw,unit=1,file=$FW_VARS" \
    -drive "$DISK_DRIVE" \
    -serial "file:$SERIAL_LOG" \
    -monitor "unix:$MON_SOCK,server,nowait" \
    -display none \
    -no-reboot \
    -d int,cpu_reset -D /tmp/user_trap_int.log &
QEMU_PID=$!

cleanup() {
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
    rm -rf "$STAGING" "$DISK_IMG" "$FW_VARS" "$SERIAL_LOG" "$MON_SOCK" /tmp/user_trap_int.log
}
trap cleanup EXIT

# Wait for the guest to reach the scheduler hand-off (bounded).
reached=0
for _ in $(seq 1 "$TIMEOUT_BOOT"); do
    if [ -f "$SERIAL_LOG" ] && grep -q "entering scheduler" "$SERIAL_LOG" 2>/dev/null; then
        reached=1
        break
    fi
    sleep 1
done
if [ "$reached" -ne 1 ]; then
    echo "FAIL: guest never reached the scheduler hand-off"
    tail -5 "$SERIAL_LOG" 2>/dev/null || true
    exit 1
fi
echo "serial: scheduler hand-off reached — user payload should be spinning"

# Give the payload time to trap, return, and write the mailbox.
sleep 3

# Read the mailbox through the QEMU monitor (xp = guest physical memory).
exec 3<>/dev/tcp/127.0.0.1/4444 || { echo "FAIL: monitor connect"; exit 1; }
# Drain the banner.
timeout 5 cat <&3 > /dev/null 2>&1 || true
printf 'xp /3gx 0x4001000\n' >&3
MON_OUT=""
deadline=$((SECONDS + 10))
while [ $SECONDS -lt $deadline ]; do
    line=""
    read -t 2 line <&3 || true
    [ -z "$line" ] && continue
    MON_OUT="$MON_OUT $line"
    case "$line" in *"(qemu)"*) break ;; esac
done
exec 3<&- 3>&-

echo "monitor: $MON_OUT"

pass=1
case "$MON_OUT" in
    *0x00000000000000d1*) ;; # round-1 EBADCALL at +0x00
    *) pass=0; echo "FAIL: mailbox[0] (round-1 errno) != 209" ;;
esac
case "$MON_OUT" in
    *0x000000000000dead*) ;; # completion marker at +0x08
    *) pass=0; echo "FAIL: mailbox[1] (completion marker) != 0xDEAD" ;;
esac
case "$MON_OUT" in
    *0x00000000000000d1*) ;;
    *) pass=0; echo "FAIL: mailbox[2] (round-2 errno) != 209" ;;
esac

if [ "$pass" -eq 1 ]; then
    echo "RESULT: PASS (E1 slice 5 — user int-33 trap round trip verified)"
    exit 0
fi
echo "RESULT: FAIL"
exit 1
