#!/bin/bash
# test-shim-bootmarks-x86.sh — x86-64 的 shim→独立镜像交接门。
#
# T2（NK1/C-29）把 x86-64 从「内核链进 boot-shim、进程内调 arch_boot」切到
# 「shim 建根开分页 + v1 arena + 跳独立 kernel.elf 高半入口」。本脚本只断言
# 这条路标链，不判命令面：
#
#   boot-shim: kernel loaded (entry staged)
#   boot-shim: 12 boot modules loaded
#   boot-shim: [raw] entering kernel image v1
#   ### minix-rs kernel-image: resuming high-half boot (handoff v1 arena, x86_64)
#   entering scheduler
#
# 前三条在 shim 侧（EBS 之前两条、跳转前一条裸串口）；第四条证明独立镜像的
# Rust 入口真的跑起来；第五条证明内核走到调度器（kmain 存活）。命令面另有
# test-cmd-smoke.sh（它的 du 目录读腿是既有 A 板问题，不作本门判据）。
#
# 与 aarch64 兄弟脚本的关系：同一套两段式（先只读验盘、再点火）与同一组
# 路标判据；差异只有固件（OVMF）、ESP 默认加载项（BOOTX64.EFI）与机器参数。
# 本脚本不注册进 run_all.sh（aarch64 兄弟同样未注册；run_all 跑的是测试
# 内核矩阵，shell 门手工跑）。
#
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP（前置条件缺失）。

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs/os
RUN="${RUN:-nk1x}"
TIMEOUT_BOOT="${TIMEOUT_BOOT:-180}"
MARK1="boot-shim: kernel loaded (entry staged)"
MARK2="boot-shim: 12 boot modules loaded"
MARK3="boot-shim: [raw] entering kernel image v1"
MARK4="### minix-rs kernel-image: resuming high-half boot (handoff v1 arena, x86_64)"
MARK5="entering scheduler"

if ! command -v qemu-system-x86_64 &>/dev/null; then
    echo "SKIP: qemu-system-x86_64 not found"; exit 2
fi
if ! command -v mdir &>/dev/null; then
    echo "SKIP: mtools (mdir) not found"; exit 2
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

# ── Stage 1: 装盘（xtask image --arch x86_64 --release）。──────────────
if [ -n "${IMG:-}" ]; then
    echo "using caller-provided image: $IMG (assembly skipped)"
else
    echo "assembling x86_64 image (xtask image --arch x86_64 --release)…"
    ( cd "$ROOT" && cargo run -q -p xtask -- image --arch x86_64 --release ) || {
        echo "FAIL: xtask image assembly failed"; exit 1
    }
    IMG="$ROOT/target/image/x86_64/minix.img"
fi
[ -f "$IMG" ] || { echo "FAIL: $IMG not produced"; exit 1; }
echo "image in use: $IMG"

# ── Stage 2: 只读验盘（不需要虚拟机）。─────────────────────────────────
mdir -i "$IMG" "::/EFI/BOOT" 2>/dev/null | tr -s ' ' | grep -qiE 'BOOTX64[[:space:]]+EFI' || {
    echo "FAIL: /EFI/BOOT/BOOTX64.EFI missing from ESP"; exit 1
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
echo "ESP verified: BOOTX64.EFI + kernel.elf + all 12 module names present"

if [ "${SMOKE_SKIP_BOOT:-0}" = "1" ]; then
    echo "RESULT: PASS (stages 1-2; SMOKE_SKIP_BOOT=1 — boot stage not run)"
    exit 0
fi

# ── Stage 3: OVMF 下点火，等整条路标链。───────────────────────────────
WORK="$ROOT/target/image/x86_64"
FW_VARS="$WORK/fw_vars_$RUN.fd"
SERIAL_LOG="$WORK/serial_$RUN.log"
rm -f "$SERIAL_LOG"
cp "$FW_SRC_VARS" "$FW_VARS"

pkill -f '[q]emu-system' 2>/dev/null || true
sleep 1

timeout "$TIMEOUT_BOOT" qemu-system-x86_64 \
    -machine q35 \
    -net none \
    -smp 1 \
    -m 512M \
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

all=0
for _ in $(seq 1 "$TIMEOUT_BOOT"); do
    if [ -f "$SERIAL_LOG" ] \
        && grep -qaF "$MARK1" "$SERIAL_LOG" \
        && grep -qaF "$MARK2" "$SERIAL_LOG" \
        && grep -qaF "$MARK3" "$SERIAL_LOG" \
        && grep -qaF "$MARK4" "$SERIAL_LOG" \
        && grep -qaF "$MARK5" "$SERIAL_LOG"; then
        all=1
        break
    fi
    if [ -f "$SERIAL_LOG" ] && grep -qa "panicked at" "$SERIAL_LOG"; then
        break
    fi
    sleep 1
done

if [ "$all" -eq 1 ]; then
    echo "### TEST_RESULT: PASS test-shim-bootmarks-x86 ###"
    grep -a "boot-shim:\|kernel-image:\|entering scheduler" "$SERIAL_LOG" | head -12
    exit 0
fi

echo "### TEST_RESULT: FAIL test-shim-bootmarks-x86 ###"
for mark in "$MARK1" "$MARK2" "$MARK3" "$MARK4" "$MARK5"; do
    grep -qaF "$mark" "$SERIAL_LOG" 2>/dev/null && echo "    出现: $mark" || echo "    未出现: $mark"
done
echo "--- 串口尾部 20 行（去掉固件版本噪声）："
grep -av "UEFI shell" "$SERIAL_LOG" 2>/dev/null | tail -20
exit 1
