#!/bin/bash
# test-shim-bootmarks-aarch64.sh — NK4-B P3 M3.3 的专用载体：AAVMF 下装
# 一张 aarch64 ESP 盘，只断言 boot-shim 的两行路标。
#
#   boot-shim: kernel loaded (entry staged)
#   boot-shim: 12 boot modules loaded
#
# 两行都在 `os/boot-shim/src/uefi_helpers.rs:179,182`，位置在
# ExitBootServices 之前，所以本脚本判的是「固件加载了 BOOTAA64.EFI、
# shim 能从 ESP 读到 kernel.elf 与 12 个模块」这三件事，不判内核是否
# 活着（那是 M3.4）。
#
# 与同目录既有载体的关系：
# - qemu 参数抄 `test-timer-irq-aarch64.sh`：`-machine virt,gic-version=3`
#   是硬要求（AAVMF 的 ACPI GICR 表项恒 0，只有 gic-version=3 的
#   QemuVirtDesc 路径能绕开，见 NK4B-TODO §3 已知事实）。
# - 串口采集与「先验盘再点火」的两段式照抄 `test-cmd-smoke.sh`（x86_64
#   腿），差别是这里用 `xtask image --arch aarch64` 的成品盘，不再自己
#   mcopy 一个空壳。
# - 内存取 512M 与 x86_64 冒烟腿一致：本脚本要真装 kernel.elf + 12 模块
#   + bump 区，timer 载体那 256M 是给不装东西的测试内核用的。
# - 临时文件落在 target 树内，不用 `mktemp`（它会硬写 /tmp，受限沙箱下
#   报假失败——NK4-B P3 M3.2 已记过这条环境陷阱）。
#
# 本脚本**不注册进 run_all.sh**（按 NK4B-TODO §6，接线归 P6）。
#
# Exit codes: 0 = PASS（两行都在）, 1 = FAIL, 2 = SKIP（前置条件缺失）。

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs/os
RUN="${RUN:-m33a}"
TIMEOUT_BOOT="${TIMEOUT_BOOT:-90}"
MARK1="boot-shim: kernel loaded (entry staged)"
MARK2="boot-shim: 12 boot modules loaded"

if ! command -v qemu-system-aarch64 &>/dev/null; then
    echo "SKIP: qemu-system-aarch64 not found"; exit 2
fi
if ! command -v mdir &>/dev/null; then
    echo "SKIP: mtools (mdir) not found"; exit 2
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

# ── Stage 1: 装盘（xtask image --arch aarch64 --release）。──────────────
# 调用方给了 IMG 就用它（反向判别要喂一张故意缺件的盘），否则自己装。
if [ -n "${IMG:-}" ]; then
    echo "using caller-provided image: $IMG (assembly skipped)"
else
    echo "assembling aarch64 image (xtask image --arch aarch64 --release)…"
    ( cd "$ROOT" && cargo run -q -p xtask -- image --arch aarch64 --release ) || {
        echo "FAIL: xtask image assembly failed"; exit 1
    }
    IMG="$ROOT/target/image/aarch64/minix.img"
fi
[ -f "$IMG" ] || { echo "FAIL: $IMG not produced"; exit 1; }
echo "image in use: $IMG"

# ── Stage 2: 只读验盘（不需要虚拟机）。 BOOTAA64.EFI 必须在 ESP 上： ──
# 这个名字是 AAVMF 的唯一默认加载项，缺它 = 固件直接落在 UEFI Shell。
mdir -i "$IMG" "::/EFI/BOOT" 2>/dev/null | tr -s ' ' | grep -qiE 'BOOTAA64[[:space:]]+EFI' || {
    echo "FAIL: /EFI/BOOT/BOOTAA64.EFI missing from ESP"; exit 1
}
mdir -i "$IMG" "::/EFI/minix" 2>/dev/null | tr -s ' ' | grep -qiE 'kernel[[:space:]]+elf' || {
    echo "FAIL: kernel.elf missing from ESP"; exit 1
}
MODULE_LIST="$(mdir -i "$IMG" "::/EFI/minix/modules" 2>/dev/null)"
for m in ds rs pm sched vfs memory tty mib vm pfs mfs init; do
    echo "$MODULE_LIST" | grep -qiE "(^|[[:space:].])$m([[:space:].]|$)" || {
        echo "FAIL: module '$m' missing from ESP"; exit 1
    }
done
echo "ESP verified: BOOTAA64.EFI + kernel.elf + all 12 module names present"

if [ "${SMOKE_SKIP_BOOT:-0}" = "1" ]; then
    echo "RESULT: PASS (stages 1-2; SMOKE_SKIP_BOOT=1 — boot stage not run)"
    exit 0
fi

# ── Stage 3: AAVMF 下点火，等两行路标。 ─────────────────────────────────
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
    -smp 1 \
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

both=0
for _ in $(seq 1 "$TIMEOUT_BOOT"); do
    if [ -f "$SERIAL_LOG" ] \
        && grep -qaF "$MARK1" "$SERIAL_LOG" \
        && grep -qaF "$MARK2" "$SERIAL_LOG"; then
        both=1
        break
    fi
    # shim 的 panic 走 uefi 的 panic hook（ConOut 可见），内核侧死循环不在
    # 本脚本判据内——出现 panicked at 就提前收，省下整段超时。
    if [ -f "$SERIAL_LOG" ] && grep -qa "panicked at" "$SERIAL_LOG"; then
        break
    fi
    sleep 1
done

if [ "$both" -eq 1 ]; then
    echo "### TEST_RESULT: PASS test-shim-bootmarks-aarch64 ###"
    grep -a "boot-shim:" "$SERIAL_LOG" | head -12
    exit 0
fi

echo "### TEST_RESULT: FAIL test-shim-bootmarks-aarch64 ###"
echo "--- marker 1 [$MARK1]："
grep -qaF "$MARK1" "$SERIAL_LOG" 2>/dev/null && echo "    出现" || echo "    未出现"
echo "--- marker 2 [$MARK2]："
grep -qaF "$MARK2" "$SERIAL_LOG" 2>/dev/null && echo "    出现" || echo "    未出现"
echo "--- 串口尾部 20 行（去掉固件版本噪声）："
grep -av "UEFI shell" "$SERIAL_LOG" 2>/dev/null | tail -20
exit 1
