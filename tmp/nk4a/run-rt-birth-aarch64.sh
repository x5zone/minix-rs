#!/bin/bash
# run-rt-birth-aarch64.sh — NK4-B P3 M3.1 取证包壳（非版管测试脚本）
#
# 与 os/qemu-tests/test-rt-birth-aarch64.sh 的差别只有一处：串口日志写到
# tmp/nk4a/serial_a64_<TAG>.log 并保留（原版用 mktemp + trap 删除，跑完无档）。
# 其余构建/分区/启动参数逐字照抄原版，QEMU 机器参数可由 A64_MACHINE 覆盖
# （默认 "virt"，与原版一致；对照实验用 "virt,gic-version=3"，
#  见 NK4B-TODO §3「AAVMF 载体必须 gic-version=3」）。
#
# 用法：bash tmp/nk4a/run-rt-birth-aarch64.sh <TAG> [等待秒=75]
set -uo pipefail
TAG="${1:?usage: $0 <TAG> [wait_s]}"
WAIT="${2:-75}"
REPO=/home/xzhao/github/minix-rs
OS_ROOT="$REPO/os"
A64_MACHINE="${A64_MACHINE:-virt}"
LOG="$REPO/tmp/nk4a/serial_a64_${TAG}.log"

echo "building rt-birth (aarch64-unknown-none)…"
( cd "$OS_ROOT" && cargo build -p rt-birth --target aarch64-unknown-none --features fw-user-none --release ) || exit 1
RT_BIRTH_ELF_PATH="$OS_ROOT/target/aarch64-unknown-none/release/rt-birth"
echo "building carrier (machine=$A64_MACHINE)…"
( cd "$OS_ROOT" && RT_BIRTH_ELF_PATH="$RT_BIRTH_ELF_PATH" \
    cargo build -p test-rt-birth-aarch64 --target aarch64-unknown-uefi --features fw-aarch64-uefi --release ) || exit 1
EFI="$OS_ROOT/target/aarch64-unknown-uefi/release/test-rt-birth-aarch64.efi"

STAGING="$(mktemp -d /tmp/dbg_a64_staging.XXXXXX)"
mkdir -p "$STAGING/EFI/BOOT"
DISK_IMG="$(mktemp /tmp/dbg_a64_disk.XXXXXX.img)"
dd if=/dev/zero of="$DISK_IMG" bs=1M count=64 status=none
mkfs.vfat "$DISK_IMG" >/dev/null 2>&1
mmd -i "$DISK_IMG" ::EFI ::EFI/BOOT >/dev/null 2>&1
mcopy -i "$DISK_IMG" "$EFI" "::EFI/BOOT/BOOTAA64.EFI"
printf 'echo -off\r\nFS0:\r\ncd EFI\\BOOT\r\nBOOTAA64.EFI\r\n' > "$STAGING/startup.nsh"
mcopy -i "$DISK_IMG" "$STAGING/startup.nsh" "::startup.nsh"
DISK_DRIVE="file=$DISK_IMG,format=raw,media=disk"

FW=/usr/share/AAVMF/AAVMF_CODE.fd
FW_VARS="$(mktemp /tmp/dbg_a64_vars.XXXXXX.fd)"
cp /usr/share/AAVMF/AAVMF_VARS.fd "$FW_VARS"
rm -f "$LOG"

qemu-system-aarch64 \
    -machine "$A64_MACHINE" \
    -cpu cortex-a57 \
    -smp 1 \
    -m 256M \
    -nographic \
    -drive "if=pflash,format=raw,unit=0,file=$FW,readonly=on" \
    -drive "if=pflash,format=raw,unit=1,file=$FW_VARS" \
    -drive "$DISK_DRIVE" \
    -serial "file:$LOG" \
    -display none \
    -no-reboot &
QEMU_PID=$!
cleanup() {
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
    rm -rf "$STAGING" "$DISK_IMG" "$FW_VARS"
}
trap cleanup EXIT

for _ in $(seq 1 "$WAIT"); do
    grep -qa "rt-birth panic render check" "$LOG" 2>/dev/null && break
    sleep 1
done
pkill -f '[q]emu-system' 2>/dev/null || true
sleep 1
echo "SERIAL=$LOG lines=$(wc -l < "$LOG")"
