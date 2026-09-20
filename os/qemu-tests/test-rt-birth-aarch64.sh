#!/bin/bash
# test-rt-birth-aarch64.sh — first minix-rt user binary on aarch64
# (edge1 K12b: the aarch64 leg of the three-arch CPL3 birth chain).
#
# Builds the rt-birth user image for aarch64-unknown-none, embeds it into
# the carrier kernel (RT_BIRTH_ELF_PATH), and boots under QEMU virt with
# AAVMF (UEFI). The carrier drives the production boot phases
# (arch_boot_impl → init_kerninfo → init_proc_and_boot's load_vm_elf →
# switch_to_user) and supplies the EL0-svc trap leg (MINIX_KERNINFO query
# + SYS_DIAGCTL console) through its own VBAR_EL1 vector table.
#
# PASS: the serial log carries the birth chain's markers, every one of
# them user-CPL3 output that crossed the svc boundary:
#   "rt-birth argv="               descriptor parse (crt0 birth chain)
#   "rt-birth kerninfo=ready"      real MINIX_KERNINFO trap query
#   "rt-birth user_sp="            kuserinfo dereference on the shared page
#   "RT-BIRTH MAIN OK"             whole chain alive
#   "rt-birth panic render check"  user panic hook render via SYS_DIAGCTL
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OS_ROOT="$ROOT"
TIMEOUT_RUN="${TIMEOUT_RUN:-90}"

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

echo "building rt-birth (aarch64-unknown-none)…"
( cd "$OS_ROOT" && cargo build -p rt-birth --target aarch64-unknown-none --features fw-user-none --release ) || {
    echo "FAIL: rt-birth build failed"; exit 1
}
RT_BIRTH_ELF_PATH="$OS_ROOT/target/aarch64-unknown-none/release/rt-birth"

echo "building test-rt-birth-aarch64 carrier (embeds $RT_BIRTH_ELF_PATH)…"
( cd "$OS_ROOT" && RT_BIRTH_ELF_PATH="$RT_BIRTH_ELF_PATH" \
    cargo build -p test-rt-birth-aarch64 --target aarch64-unknown-uefi --features fw-aarch64-uefi --release ) || {
    echo "FAIL: test-rt-birth-aarch64 build failed"; exit 1
}
EFI="$OS_ROOT/target/aarch64-unknown-uefi/release/test-rt-birth-aarch64.efi"

# Stage the EFI on a FAT disk (test-user-trap.sh pattern, BOOTAA64 name).
STAGING="$(mktemp -d /tmp/rt_birth_a64_staging.XXXXXX)"
mkdir -p "$STAGING/EFI/BOOT"
DISK_IMG="$(mktemp /tmp/rt_birth_a64_disk.XXXXXX.img)"
if command -v mkfs.vfat &>/dev/null && command -v mmd &>/dev/null && command -v mcopy &>/dev/null; then
    dd if=/dev/zero of="$DISK_IMG" bs=1M count=64 status=none 2>/dev/null
    mkfs.vfat "$DISK_IMG" >/dev/null 2>&1
    mmd -i "$DISK_IMG" ::EFI ::EFI/BOOT >/dev/null 2>&1
    mcopy -v -i "$DISK_IMG" "$EFI" "::EFI/BOOT/BOOTAA64.EFI" || echo "FAIL: mcopy BOOTAA64"
    printf 'echo -off\r\nFS0:\r\ncd EFI\\BOOT\r\nBOOTAA64.EFI\r\n' > "$STAGING/startup.nsh"
    mcopy -i "$DISK_IMG" "$STAGING/startup.nsh" "::startup.nsh" || echo "FAIL: mcopy startup.nsh"
    DISK_DRIVE="file=$DISK_IMG,format=raw,media=disk"
else
    DISK_DRIVE="file=fat:rw:$STAGING,format=raw,media=disk"
fi

FW_VARS="$(mktemp /tmp/rt_birth_a64_vars.XXXXXX.fd)"
cp "$FW_SRC_VARS" "$FW_VARS"
SERIAL_LOG="$(mktemp /tmp/rt_birth_a64_serial.XXXXXX.log)"

qemu-system-aarch64 \
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

seen=0
for _ in $(seq 1 "$TIMEOUT_RUN"); do
    if grep -qa "rt-birth panic render check" "$SERIAL_LOG" 2>/dev/null; then
        seen=1
        break
    fi
    sleep 1
done
if [ "$seen" -ne 1 ]; then
    echo "### TEST_RESULT: FAIL test-rt-birth-aarch64 ###"
    tail -8 "$SERIAL_LOG" 2>/dev/null
    exit 1
fi

pass=1
grep -qa "rt-birth argv="              "$SERIAL_LOG" || { pass=0; echo "FAIL: descriptor parse marker missing"; }
grep -qa "rt-birth kerninfo=ready"     "$SERIAL_LOG" || { pass=0; echo "FAIL: kerninfo trap query did not report ready"; }
grep -qa "rt-birth user_sp="           "$SERIAL_LOG" || { pass=0; echo "FAIL: kuserinfo dereference marker missing"; }
grep -qa "RT-BIRTH MAIN OK"            "$SERIAL_LOG" || { pass=0; echo "FAIL: birth chain did not reach main"; }
grep -qa "rt-birth panic render check" "$SERIAL_LOG" || { pass=0; echo "FAIL: panic render marker missing"; }

if [ "$pass" -eq 1 ]; then
    echo "### TEST_RESULT: PASS test-rt-birth-aarch64 ###"
    exit 0
fi
echo "### TEST_RESULT: FAIL test-rt-birth-aarch64 ###"
exit 1
