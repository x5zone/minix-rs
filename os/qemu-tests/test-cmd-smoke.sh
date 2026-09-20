#!/bin/bash
# test-cmd-smoke.sh — the 18-stage commands smoke acceptance (NS12; T4).
#
# Defines, in one place, what "the boot runs 18-stage commands with visible
# output" means as serial-log assertions, and drives every stage of the way:
#
#   1. Assemble the bootable image via xtask (kernel.elf + the 12 boot
#      modules + the mfs imgrd seeded with /etc — the NS8 face).
#   2. Verify the ESP contents directly with mtools (read-only; no guest
#      needed): kernel.elf present, 12 module files matching the
#      boot-shim MODULE_NAMES order source, imgrd non-empty.
#   3. Boot under OVMF and wait for the scheduler hand-off marker
#      ("entering scheduler" — the T1 contract).
#   4. T4 window: wait for the command-output marker (default
#      "CMD-SMOKE-OK", what /etc/rc echoes once the OQ-3 content lands;
#      override with SMOKE_MARKERS="pat1|pat2").
#
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).
#
# Honest state today: stages 1-2 are fully exercised by this script; stage
# 3 is the T1-established marker; stage 4 waits on the NK1 boot-chain
# completion (the hand-off is still mid-flight there) and the OQ-3 /etc
# finalization — until both land, the script FAILS at 3/4 with the serial
# tail printed, which is exactly its debugging job. SMOKE_SKIP_BOOT=1
# stops after stage 2 (the mode the script was first validated in).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs/os
TIMEOUT_BOOT="${TIMEOUT_BOOT:-120}"
TIMEOUT_T4="${TIMEOUT_T4:-60}"
T4_MARKER="${T4_MARKER:-CMD-SMOKE-OK}"

if ! command -v qemu-system-x86_64 &>/dev/null; then
    echo "SKIP: qemu-system-x86_64 not found"; exit 2
fi
if ! command -v mdir &>/dev/null || ! command -v mcopy &>/dev/null; then
    echo "SKIP: mtools (mdir/mcopy) not found"; exit 2
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

# ── Stage 1: assemble the bootable image (kernel + 12 modules + imgrd). ──
echo "assembling bootable image (xtask image --arch x86_64 --release)…"
( cd "$ROOT" && cargo run -q -p xtask -- image --arch x86_64 --release ) || {
    echo "FAIL: xtask image assembly failed"; exit 1
}
IMG="$ROOT/target/image/x86_64/minix.img"
[ -f "$IMG" ] || { echo "FAIL: $IMG not produced"; exit 1; }
echo "image assembled: $IMG"

# ── Stage 2: verify the ESP contents read-only (no guest needed). ──
expect_module() {  # name on the ESP
    mdir -i "$IMG" "::/EFI/minix/modules" 2>/dev/null | grep -qi "$1" || {
        echo "FAIL: module '$1' missing from ESP"; exit 1
    }
}
# mdir widens long names across columns ("kernel   elf") — collapse the
# spaces before matching.
mdir -i "$IMG" "::/EFI/minix" 2>/dev/null | tr -s ' ' | grep -qiE 'kernel[[:space:]]+elf' || {
    echo "FAIL: kernel.elf missing from ESP"; exit 1
}
mdir -i "$IMG" "::/EFI/minix" 2>/dev/null | grep -qi "imgrd" || {
    echo "FAIL: imgrd missing from ESP"; exit 1
}
MODULE_LIST="$(mdir -i "$IMG" "::/EFI/minix/modules" 2>/dev/null)"
for m in ds rs pm sched vfs memory tty mib vm pfs mfs init; do
    # word-ish match so "ds" cannot pass on some other name's substring
    echo "$MODULE_LIST" | grep -qiE "(^|[[:space:].])$m([[:space:].]|$)" || {
        echo "FAIL: module '$m' missing from ESP"; exit 1
    }
done
echo "ESP verified: kernel.elf + imgrd + all 12 module names present"

# ── Stage 2.5: caller may stop here (validation mode). ──
if [ "${SMOKE_SKIP_BOOT:-0}" = "1" ]; then
    echo "RESULT: PASS (stages 1-2; SMOKE_SKIP_BOOT=1 — boot stages not run)"
    exit 0
fi

# ── Stage 3: boot and wait for the scheduler hand-off (T1 contract). ──
FW_VARS="$(mktemp /tmp/cmd_smoke_vars.XXXXXX.fd)"
cp "$FW_SRC_VARS" "$FW_VARS"
SERIAL_LOG="$(mktemp /tmp/cmd_smoke_serial.XXXXXX.log)"
MON_SOCK="$(mktemp /tmp/cmd_smoke_mon.XXXXXX.sock)"
rm -f "$MON_SOCK"

qemu-system-x86_64 \
    -machine q35 \
    -net none \
    -smp 1 \
    -m 512M \
    -drive "if=pflash,format=raw,unit=0,file=$FW,readonly=on" \
    -drive "if=pflash,format=raw,unit=1,file=$FW_VARS" \
    -drive "file=$IMG,format=raw,media=disk" \
    -serial "file:$SERIAL_LOG" \
    -monitor "unix:$MON_SOCK,server,nowait" \
    -display none \
    -no-reboot &
QEMU_PID=$!

cleanup() {
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
    rm -rf "$FW_VARS" "$SERIAL_LOG" "$MON_SOCK"
}
trap cleanup EXIT

reached=0
for _ in $(seq 1 "$TIMEOUT_BOOT"); do
    if [ -f "$SERIAL_LOG" ] && grep -q "entering scheduler" "$SERIAL_LOG" 2>/dev/null; then
        reached=1
        break
    fi
    sleep 1
done
if [ "$reached" -ne 1 ]; then
    echo "FAIL: guest never reached the scheduler hand-off (NK1 boot chain in flight?)"
    tail -8 "$SERIAL_LOG" 2>/dev/null || true
    exit 1
fi
echo "serial: scheduler hand-off reached — waiting for the T4 command marker"

# ── Stage 4: the T4 command-output window. ──
t4=0
for _ in $(seq 1 "$TIMEOUT_T4"); do
    if [ -f "$SERIAL_LOG" ] && grep -q "$T4_MARKER" "$SERIAL_LOG" 2>/dev/null; then
        t4=1
        break
    fi
    sleep 1
done
if [ "$t4" -ne 1 ]; then
    echo "FAIL: T4 marker '$T4_MARKER' never appeared within ${TIMEOUT_T4}s"
    echo "      (init/multi-user chain or the /etc rc content — see the tail)"
    tail -12 "$SERIAL_LOG" 2>/dev/null || true
    exit 1
fi

echo "RESULT: PASS (18-stage command output visible on the boot console — T4)"
exit 0
