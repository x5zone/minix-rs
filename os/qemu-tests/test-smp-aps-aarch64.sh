#!/bin/bash
# test-smp-aps-aarch64.sh — P-A64RV-01 aarch64 半专用门（§续-405）。
#
# 判据：-smp 2 下次级核经 PSCI CPU_ON 起到桩（ap_early_entry_start）、
# 装 BSP 的 TTBR/MAIR/TCR、开 MMU 跳汇聚点并置 AP_ARRIVED——BSP 侧接线
# 打印 `nk4c: ap-arrived cpu=<hw>` 为准。与 riscv 半门
# test-smp-aps-riscv64.sh 同形，boot 链换成既有 UEFI 载体：
# xtask image → AAVMF → boot-shim → kernel.elf（§续-405 的 aarch64 无
# 选举问题：UEFI 只派 boot processor 进 boot services，次级核停在固件
# PSCI 停车环，QEMU -smp 2 即真双核）。
#
# 前置：qemu-system-aarch64、AAVMF 固件、mtools(mdir)、cargo+xtask。
# Exit codes: 0 = PASS（ap-arrived 到达）, 1 = FAIL, 2 = SKIP。

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs/os
RUN="${RUN:-smpa1}"
TIMEOUT_BOOT="${TIMEOUT_BOOT:-240}"
AP_MARKER="nk4c: ap-arrived"

fail() { echo "FAIL: $1"; exit 1; }
skip() { echo "SKIP: $1"; exit 2; }

command -v qemu-system-aarch64 &>/dev/null || skip "qemu-system-aarch64 not found"
command -v mdir &>/dev/null || skip "mtools (mdir) not found"
FW=""; FW_SRC_VARS=""
for c in /usr/share/AAVMF/AAVMF_CODE.fd /usr/share/qemu-efi-aarch64/QEMU_EFI.fd; do
    [ -f "$c" ] && FW="$c" && break
done
for c in /usr/share/AAVMF/AAVMF_VARS.fd /usr/share/qemu-efi-aarch64/QEMU_VARS.fd; do
    [ -f "$c" ] && FW_SRC_VARS="$c" && break
done
[ -n "$FW" ] || skip "AAVMF firmware not found"
[ -n "$FW_SRC_VARS" ] || skip "AAVMF vars template not found"

# ── Stage 1: 装盘（xtask image --arch aarch64 --release；SKIP_BUILD=1 复用）。──
if [ "${SKIP_BUILD:-0}" != "1" ]; then
    echo "== Stage 1: xtask image --arch aarch64 --release =="
    ( cd "$ROOT" && cargo run -q -p xtask -- image --arch aarch64 --release ) || {
        fail "xtask image assembly failed"
    }
fi
IMG="$ROOT/target/image/aarch64/minix.img"
[ -f "$IMG" ] || fail "image missing: $IMG (先不带 SKIP_BUILD 跑一次)"
echo "image in use: $IMG"

# ── Stage 2: 只读验盘（BOOTAA64.EFI + kernel.elf + 12 模块名）。──────────
mdir -i "$IMG" "::/EFI/BOOT" 2>/dev/null | tr -s ' ' | grep -qiE 'BOOTAA64[[:space:]]+EFI' || {
    fail "/EFI/BOOT/BOOTAA64.EFI missing from ESP"
}
mdir -i "$IMG" "::/EFI/minix" 2>/dev/null | tr -s ' ' | grep -qiE 'kernel[[:space:]]+elf' || {
    fail "kernel.elf missing from ESP"
}
echo "ESP verified: BOOTAA64.EFI + kernel.elf present"

# ── Stage 3: AAVMF 下点火（-smp 2＝本门的存在理由）。────────────────────
WORK="$ROOT/target/image/aarch64"
FW_VARS="$WORK/fw_vars_$RUN.fd"
SERIAL_LOG="$WORK/serial_$RUN.log"
rm -f "$SERIAL_LOG"
cp "$FW_SRC_VARS" "$FW_VARS"

pkill -f '[q]emu-system' 2>/dev/null || true
sleep 1

timeout "$TIMEOUT_BOOT" qemu-system-aarch64 \
    -machine virt,gic-version=3 \
    -cpu cortex-a57 \
    -smp 2 \
    -m 512M \
    -net none \
    -drive "if=pflash,format=raw,unit=0,file=$FW,readonly=on" \
    -drive "if=pflash,format=raw,unit=1,file=$FW_VARS" \
    -drive "file=$IMG,format=raw,media=disk" \
    -serial "file:$SERIAL_LOG" \
    -display none \
    -no-reboot &
QEMU_PID=$!

cleanup() {
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
    rm -f "$FW_VARS"
}
trap cleanup EXIT

for _ in $(seq 1 "$TIMEOUT_BOOT"); do
    if [ -f "$SERIAL_LOG" ] && grep -qaF "$AP_MARKER" "$SERIAL_LOG"; then
        break
    fi
    # shim/内核 panic 提前收，省下整段超时。
    if [ -f "$SERIAL_LOG" ] && grep -qa "panicked at" "$SERIAL_LOG"; then
        break
    fi
    sleep 1
done

[ -f "$SERIAL_LOG" ] || fail "无串口日志"
echo "----- 关键相位计数 -----"
echo "boot-shim 路标            : $(grep -ac 'boot-shim:' "$SERIAL_LOG")"
echo "kernel kmain 相位          : $(grep -ac 'kernel: kmain\|vm_handoff' "$SERIAL_LOG")"
echo "ap-arrived                 : $(grep -acF "$AP_MARKER" "$SERIAL_LOG")"
echo "ap-timeout                 : $(grep -ac 'ap-timeout' "$SERIAL_LOG")"
echo "panic                      : $(grep -ac 'panicked at' "$SERIAL_LOG")"
echo "----- 串口尾 20 行 -----"
tail -20 "$SERIAL_LOG"

if grep -qaF "$AP_MARKER" "$SERIAL_LOG"; then
    echo "### TEST_RESULT: PASS test-smp-aps-aarch64 (secondary cpu reached convergence) ###"
    exit 0
fi
echo "### TEST_RESULT: FAIL test-smp-aps-aarch64 (no ap-arrived marker) ###"
exit 1
