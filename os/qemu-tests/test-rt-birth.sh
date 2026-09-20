#!/bin/bash
# test-rt-birth.sh — the first minix-rt freestanding user binary on real
# machine (edge E1 slice 5 acceptance; 14-stage-runtime V1-P1-1 step 2;
# E-KERNINFO end-to-end).
#
# Builds the rt-birth user ELF (x86_64-unknown-none), embeds it into the
# test-rt-birth kernel (RT_BIRTH_ELF_PATH env → include_bytes!), boots it
# under OVMF, and lets the production boot path (init_proc_and_boot's VM
# branch: load_vm_elf → segments + stack + ps_strings → build_cpu_context)
# hand the CPU over at the ELF entry with RBX = ps_strings.
#
# The birth chain's output lands on the serial log through SYS_DIAGCTL
# (kernel console channel):
#   rt-birth argv=0 progname=''      descriptor parse (boot images: empty)
#   rt-birth kerninfo=ready flags=2  MINIX_KERNINFO trap query validated
#                                    (flags=2 = MINIX_KIF_USERINFO)
#   rt-birth user_sp=0x7fffffff…     direct read of the page's kuserinfo
#   RT-BIRTH MAIN OK                 full chain alive
#   …panicked at …: rt-birth panic render check
#                                    user-space panic hook routed the
#                                    rendered report back to the console
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

# 1. Build the user image, then the kernel with the artifact embedded.
echo "building rt-birth (x86_64-unknown-none)…"
( cd "$ROOT" && cargo build -p rt-birth --target x86_64-unknown-none --features fw-user-none --release ) || {
    echo "FAIL: rt-birth build failed"; exit 1
}
RT_BIRTH_ELF_PATH="$ROOT/target/x86_64-unknown-none/release/rt-birth"
echo "building test-rt-birth kernel (embeds $RT_BIRTH_ELF_PATH)…"
( cd "$ROOT" && RT_BIRTH_ELF_PATH="$RT_BIRTH_ELF_PATH" \
    cargo build -p test-rt-birth --target x86_64-unknown-uefi --features fw-x86-uefi --release ) || {
    echo "FAIL: test-rt-birth build failed"; exit 1
}
EFI="$ROOT/target/x86_64-unknown-uefi/release/test-rt-birth.efi"

# 2. Stage the EFI on a FAT disk (test-user-trap.sh pattern).
STAGING="$(mktemp -d /tmp/rt_birth_staging.XXXXXX)"
mkdir -p "$STAGING/EFI/BOOT"
DISK_IMG="$(mktemp /tmp/rt_birth_disk.XXXXXX.img)"
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

FW_VARS="$(mktemp /tmp/rt_birth_vars.XXXXXX.fd)"
cp "$FW_SRC_VARS" "$FW_VARS"
SERIAL_LOG="$(mktemp /tmp/rt_birth_serial.XXXXXX.log)"
MON_SOCK="$(mktemp /tmp/rt_birth_mon.XXXXXX.sock)"
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
    -gdb tcp::1234 \
    -monitor "unix:$MON_SOCK,server,nowait" \
    -display none \
    -no-reboot \
    -d int,cpu_reset -D /tmp/rt_birth_int.log &
QEMU_PID=$!

cleanup() {
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
    rm -rf "$STAGING" "$DISK_IMG" "$FW_VARS" "$SERIAL_LOG" "$MON_SOCK" /tmp/rt_birth_int.log
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
echo "serial: scheduler hand-off reached — rt-birth should be running"

# Give the birth chain time to run (traps + console writes).
sleep 3

pass=1
grep -q "rt-birth argv=0 progname=''" "$SERIAL_LOG" || { pass=0; echo "FAIL: descriptor parse line missing"; }
grep -q "rt-birth kerninfo=ready flags=2" "$SERIAL_LOG" || { pass=0; echo "FAIL: kerninfo trap query did not validate (flags != MINIX_KIF_USERINFO)"; }
grep -q "rt-birth user_sp=0x00007fff" "$SERIAL_LOG" || { pass=0; echo "FAIL: kuserinfo direct read missing (user_sp)"; }
grep -q "RT-BIRTH MAIN OK" "$SERIAL_LOG" || { pass=0; echo "FAIL: main not reached"; }
grep -q "rt-birth panic render check" "$SERIAL_LOG" || { pass=0; echo "FAIL: panic render check missing on console"; }

if [ "$pass" -eq 1 ]; then
    echo "RESULT: PASS (minix-rt birth chain on real machine — E1 slice 5 + V1-P1-1 step 2)"
    exit 0
fi
echo "--- serial tail ---"
tail -15 "$SERIAL_LOG" 2>/dev/null || true
echo "RESULT: FAIL"
exit 1
