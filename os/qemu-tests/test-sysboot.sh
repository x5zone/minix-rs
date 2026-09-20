#!/bin/bash
# test-sysboot.sh — S42 ④ multi-process boot carrier (C-27): the kernel
# boots TWO user images through the production boot path and the two
# complete the first user↔user IPC exchange on real machine.
#
#   - VM boot slot (endpoint 8) runs sysboot-rx: parks in receive(ANY),
#     answers the exchange, parks again.
#   - RS slot (endpoint 2) runs sysboot-tx: sendrec to endpoint 8.
#
# Builds both user ELFs (x86_64-unknown-none), embeds them into the
# test-sysboot kernel (env paths → include_bytes!), boots it under OVMF
# (test-rt-birth.sh pattern), and greps the serial log for the exchange
# markers:
#   SYSBOOT RX UP / RX GOT / RX DONE — the receive half answered
#   SYSBOOT TX UP / TX GOT / TX DONE — the send half got the reply
#
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs/os
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

# 1. Build both user images, then the kernel with them embedded.
echo "building sysboot-rx (x86_64-unknown-none)…"
( cd "$ROOT" && cargo build -p sysboot-rx --target x86_64-unknown-none --features fw-x86-none --release ) || {
    echo "FAIL: sysboot-rx build failed"; exit 1
}
echo "building sysboot-tx (x86_64-unknown-none)…"
( cd "$ROOT" && cargo build -p sysboot-tx --target x86_64-unknown-none --features fw-x86-none --release ) || {
    echo "FAIL: sysboot-tx build failed"; exit 1
}
RX_ELF="$ROOT/target/x86_64-unknown-none/release/sysboot-rx"
TX_ELF="$ROOT/target/x86_64-unknown-none/release/sysboot-tx"

echo "building test-sysboot kernel (embeds both)…"
( cd "$ROOT" && SYSBOOT_RX_ELF_PATH="$RX_ELF" SYSBOOT_TX_ELF_PATH="$TX_ELF" \
    cargo build -p test-sysboot --target x86_64-unknown-uefi --features fw-x86-uefi --release ) || {
    echo "FAIL: test-sysboot build failed"; exit 1
}
EFI="$ROOT/target/x86_64-unknown-uefi/release/test-sysboot.efi"

# 2. Stage the EFI on a FAT disk (test-rt-birth.sh pattern).
STAGING="$(mktemp -d /tmp/sysboot_staging.XXXXXX)"
mkdir -p "$STAGING/EFI/BOOT"
DISK_IMG="$(mktemp /tmp/sysboot_disk.XXXXXX.img)"
if command -v mkfs.vfat &>/dev/null && command -v mmd &>/dev/null && command -v mcopy &>/dev/null; then
    dd if=/dev/zero of="$DISK_IMG" bs=1M count=64 status=none 2>/dev/null
    mkfs.vfat "$DISK_IMG" >/dev/null 2>&1
    mmd -i "$DISK_IMG" ::EFI ::EFI/BOOT >/dev/null 2>&1
    mcopy -v -i "$DISK_IMG" "$EFI" "::EFI/BOOT/BOOTX64.EFI" || echo "FAIL: mcopy BOOTX64"
    printf 'echo -off\r\nFS0:\r\ncd EFI\\BOOT\r\nBOOTX64.EFI\r\n' > "$STAGING/startup.nsh"
    mcopy -i "$DISK_IMG" "$STAGING/startup.nsh" "::startup.nsh" || echo "FAIL: mcopy startup.nsh"
    DISK_DRIVE="file=$DISK_IMG,format=raw,media=disk"
else
    DISK_DRIVE="file=fat:rw:$STAGING,format=raw,media=disk"
fi

FW_VARS="$(mktemp /tmp/sysboot_vars.XXXXXX.fd)"
cp "$FW_SRC_VARS" "$FW_VARS"
SERIAL_LOG="$(mktemp /tmp/sysboot_serial.XXXXXX.log)"
MON_SOCK="$(mktemp /tmp/sysboot_mon.XXXXXX.sock)"
rm -f "$MON_SOCK"

qemu-system-x86_64 \
    -machine q35 \
    -net none \
    -smp 1 \
    -m 256M \
    -drive "if=pflash,format=raw,unit=0,file=$FW,readonly=on" \
    -drive "if=pflash,format=raw,unit=1,file=$FW_VARS" \
    -drive "$DISK_DRIVE" \
    -serial "file:$SERIAL_LOG" \
    -monitor "unix:$MON_SOCK,server,nowait" \
    -display none \
    -no-reboot &
QEMU_PID=$!

cleanup() {
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
    rm -rf "$STAGING" "$DISK_IMG" "$FW_VARS" "$SERIAL_LOG" "$MON_SOCK"
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
echo "serial: scheduler hand-off reached — both processes should run"

# Give the exchange time to run (traps + IPC + console writes).
sleep 3

pass=1
grep -q "SYSBOOT RX UP" "$SERIAL_LOG" || { pass=0; echo "FAIL: RX never reached main"; }
grep -q "SYSBOOT TX UP" "$SERIAL_LOG" || { pass=0; echo "FAIL: TX never reached main"; }
grep -q "SYSBOOT RX GOT" "$SERIAL_LOG" || { pass=0; echo "FAIL: RX never received the message (user receive half broken)"; }
grep -q "SYSBOOT RX DONE" "$SERIAL_LOG" || { pass=0; echo "FAIL: RX reply never left (user sendnb half broken)"; }
grep -q "SYSBOOT TX GOT" "$SERIAL_LOG" || { pass=0; echo "FAIL: TX never got the reply (sendrec round trip broken)"; }
grep -q "SYSBOOT TX DONE" "$SERIAL_LOG" || { pass=0; echo "FAIL: exchange incomplete"; }

if [ "$pass" -eq 1 ]; then
    echo "RESULT: PASS (multi-process boot + first user↔user IPC on real machine — S42 ④/C-27)"
    exit 0
fi
echo "--- serial tail ---"
tail -15 "$SERIAL_LOG" 2>/dev/null || true
echo "RESULT: FAIL"
exit 1
