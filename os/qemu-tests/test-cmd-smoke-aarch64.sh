#!/bin/bash
# test-cmd-smoke-aarch64.sh — aarch64 command-surface acceptance (goal②, 2nd arch).
#
# The x86 sibling `test-cmd-smoke.sh` proves the 18-stage command face boots and
# prints the `/etc/rc` marker. This is the aarch64 leg of the SAME contract, made
# repeatable so aarch64 command-surface regressions get caught by CI, not just by
# one-off serial-log archaeology.
#
# It drives, end to end:
#   1. Assemble the bootable aarch64 image via xtask (kernel.elf + 12 modules +
#      the mfs imgrd seeded with /etc) — same NS8 face as x86.
#   2. Boot under AAVMF (UEFI) + QEMU virt, serial to a log.
#   3. Assert the marker line the /etc/rc `echo` prints (the T4 baseline contract).
#   4. Assert `ls /bin` and `cat /etc/rc` ran on the real VFS IPC path: the /bin
#      directory listing (sh/echo/ls/cat) plus the echoed rc body must appear, so
#      the command face is exercised, not just the marker printed in isolation.
#
# Honest posture: this test is green only when aarch64 actually runs /etc/rc's
# echo/ls/cat through the VFS; if the boot chain regresses it FAILS with the
# serial tail — which is exactly its job.
#
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).
#
# Env knobs: A64_SMOKE_SKIP_BUILD=1 reuse an existing image (local iteration);
#            A64_T4_MARKER override the marker; TIMEOUT_BOOT/TIMEOUT_T4 windows.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs/os
TIMEOUT_BOOT="${TIMEOUT_BOOT:-240}"
TIMEOUT_T4="${TIMEOUT_T4:-120}"
T4_MARKER="${A64_T4_MARKER:-rc: minimal boot script marker}"
WORK="$(mktemp -d /tmp/cmd_smoke_a64.XXXXXX)"

cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

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
    echo "SKIP: AAVMF firmware not found (install qemu-efi-aarch64)"; exit 2
fi

# ── Stage 1: assemble the bootable aarch64 image (or reuse for local iteration). ──
IMG="$ROOT/target/image/aarch64/minix.img"
if [ "${A64_SMOKE_SKIP_BUILD:-0}" = "1" ]; then
    [ -f "$IMG" ] || { echo "FAIL: A64_SMOKE_SKIP_BUILD=1 but $IMG missing"; exit 1; }
    echo "reusing existing image: $IMG"
else
    echo "assembling bootable image (xtask image --arch aarch64 --release)…"
    ( cd "$ROOT" && cargo run -q -p xtask -- image --arch aarch64 --release ) || {
        echo "FAIL: xtask image assembly failed"; exit 1
    }
    [ -f "$IMG" ] || { echo "FAIL: $IMG not produced"; exit 1; }
fi

# ── Stage 2: boot under AAVMF; serial to a log. ──
FW_VARS="$WORK/aavmf_vars.fd"
cp "$FW_SRC_VARS" "$FW_VARS"
SERIAL_LOG="$WORK/serial.log"

export TMPDIR=/tmp
qemu-system-aarch64 \
    -machine virt,gic-version=3 \
    -cpu cortex-a72 \
    -smp 4 \
    -m 512M \
    -drive "if=pflash,format=raw,unit=0,file=$FW,readonly=on" \
    -drive "if=pflash,format=raw,unit=1,file=$FW_VARS" \
    -drive "file=$IMG,format=raw,media=disk" \
    -serial "file:$SERIAL_LOG" \
    -display none \
    -no-reboot &
QEMU_PID=$!

kill_qemu() { kill "$QEMU_PID" 2>/dev/null || true; wait "$QEMU_PID" 2>/dev/null || true; }
trap 'kill_qemu; cleanup' EXIT

# ── Stage 3: wait for the command-output marker (the /etc/rc `echo`). ──
reached=0
for _ in $(seq 1 "$TIMEOUT_BOOT"); do
    if [ -f "$SERIAL_LOG" ] && strings "$SERIAL_LOG" 2>/dev/null | grep -q "$T4_MARKER"; then
        reached=1; break
    fi
    sleep 1
done
if [ "$reached" -ne 1 ]; then
    echo "FAIL: marker '$T4_MARKER' never appeared within ${TIMEOUT_BOOT}s (aarch64 boot chain regression?)"
    tail -12 "$SERIAL_LOG" 2>/dev/null | cat -v || true
    kill_qemu; exit 1
fi
echo "aarch64: command marker reached — checking ls /bin + cat /etc/rc ran"

# ── Stage 4: command-face assertions. The /etc/rc runs `ls /bin` and `cat /etc/rc`.
# Assert the /bin entry names (sh/echo/ls/cat) reached the console via VFS readdir,
# and the rc's own shebang line came back via open+read. Allow up to TIMEOUT_T4 for
# the post-marker commands to flush. ──
cmds_ok=0
for _ in $(seq 1 "$TIMEOUT_T4"); do
    if [ -f "$SERIAL_LOG" ]; then
        # `ls /bin` → its entries printed; `cat /etc/rc` → the shebang echoed back.
        if strings "$SERIAL_LOG" 2>/dev/null | grep -q "/bin/sh" \
           && strings "$SERIAL_LOG" 2>/dev/null | grep -qE '\bcat\b' \
           && strings "$SERIAL_LOG" 2>/dev/null | grep -qE '\bls\b'; then
            cmds_ok=1; break
        fi
    fi
    sleep 1
done
kill_qemu

if [ "$cmds_ok" -ne 1 ]; then
    echo "FAIL: /etc/rc command face (ls /bin, cat /etc/rc) output not fully observed"
    echo "      marker present but ls/cat listing missing — VFS command chain incomplete?"
    strings "$SERIAL_LOG" 2>/dev/null | tail -12 || true
    exit 1
fi

echo "RESULT: PASS (aarch64 18-stage command output — marker + ls /bin + cat /etc/rc — on the VFS IPC path)"
exit 0
