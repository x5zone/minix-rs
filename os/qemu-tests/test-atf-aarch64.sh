#!/usr/bin/env bash
# test-atf-aarch64.sh — NK4-C 目标③ acceptance: the first minix3 ATF C test
# (t_memchr) runs to a `passed` result line on the aarch64 boot console.
#
# Why this is the goal-③ gate (vs the x86-style command smoke): the ATF
# harness legs — custom birth (_start reading the kernel ps_strings ABI),
# tinystdio over the SYS_DIAGCTL bridge, PM SendRec exit, VFS_LSTAT status
# round-trip — were wired in §NK4C 续-276/277 and proven live; this script
# freezes that evidence into a repeatable CI gate so any regression in the
# C-runtime bridge, the imgrd /tests seeding, or the kernel NoReply leg
# fails loudly instead of silently killing the test at boot.
#
# It drives, end to end:
#   1. Cross-build the ATF test + link it into a static ET_EXEC ELF
#      (tools/build-atf-test.sh; requires the aarch64 picolibc vendor leg).
#   2. Assemble the bootable image (xtask image — seeds /tests when present;
#      the xtask arch gate whitelists aarch64, so other arches are untouched).
#   3. Boot under AAVMF + QEMU virt, serial to a log.
#   4. Assert the ATF result line for t_memchr (`passed`) appears, and that
#      no kernel panic happened on the way (PANIC line = hard fail).
#
# Honest posture: this is one test case (memchr_basic) of the 586-test goal.
# The suite grows through §③'s remaining legs (fork/exec/VFS bridges); a
# `passed` here proves the *pipeline*, not the suite.
#
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).
#
# Env knobs: ATF_A64_SKIP_BUILD=1 reuse image + prebuilt ELF; TIMEOUT_BOOT.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs/os
TIMEOUT_BOOT="${TIMEOUT_BOOT:-260}"
RESULT_MARKER="passed"
WORK="$(mktemp -d /tmp/atf_a64.XXXXXX)"

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

# ── Stage 1: build the ATF C test ELF (picolibc vendor leg) ──────────────
if [ "${ATF_A64_SKIP_BUILD:-0}" != "1" ]; then
    echo "cross-building t_memchr (aarch64 picolibc leg)…"
    ( cd "$ROOT/.." && bash tools/build-atf-test.sh \
        minix3/tests/lib/libc/string/t_memchr.c aarch64 ) >"$WORK/build.log" 2>&1 || {
        echo "FAIL: build-atf-test.sh (t_memchr) failed — see below"; tail -6 "$WORK/build.log"; exit 1
    }
    for p in p1 p2 p3; do
        # 探针源与门同源（CodeReview 续-277b P2-4：不依赖 tmp/ scratch）；
        # 失败不再静默——哨兵缺件必须看得见（rc 无条件 exec /tests/p*）。
        src="$ROOT/../tools/atf-c-compat/probes/$p.c"
        if [ ! -f "$src" ] || ! ( cd "$ROOT/.." && bash tools/build-atf-test.sh "$src" aarch64 ) \
                >>"$WORK/build.log" 2>&1; then
            echo "WARN: 桥面哨兵 $p 未能构建/播种——rc 将报 not found（不阻断本门主判据）"
        fi
    done
    echo "assembling image (xtask image --arch aarch64 --release)…"
    ( cd "$ROOT" && cargo run -q -p xtask -- image --arch aarch64 --release ) \
        >"$WORK/xtask.log" 2>&1 || {
        echo "FAIL: xtask image assembly failed"; tail -6 "$WORK/xtask.log"; exit 1
    }
fi
IMG="$ROOT/target/image/aarch64/minix.img"
ELF="$ROOT/target/atf/aarch64/tests/t_memchr"
[ -f "$ELF" ] || { echo "SKIP: $ELF missing (run tools/build-atf-test.sh first)"; exit 2; }
[ -f "$IMG" ] || { echo "FAIL: $IMG missing"; exit 1; }
# Guard: the image must have been assembled with the ELF present (xtask seeds
# /tests only when the host file exists at assembly time). A stale image would
# boot without /tests and the rc line would no-op to "not found" — never PASS.
if [ "$IMG" -nt "$ELF" ]; then :; else
    echo "FAIL: $ELF is newer than $IMG — image assembled before the test was built"; exit 1
fi

# ── Stage 2: boot under AAVMF; serial to a log ───────────────────────────
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
    -net none \
    -display none \
    -no-reboot &
QEMU_PID=$!
kill_qemu() { kill "$QEMU_PID" 2>/dev/null || true; wait "$QEMU_PID" 2>/dev/null || true; }
trap 'kill_qemu; cleanup' EXIT

# ── Stage 3: wait for the ATF result line; panic is a hard fail ──────────
verdict=""
# 判据纪律（CodeReview 续-277b P2-3）：多 test case 下“首个 passed 即定案”
# 会掩盖后续 failed/broken——首次结果行命中后再等 15s 收尾窗口，按全部
# 结果行的最终集合定案（ATF 每案各写一行 passed/failed；扩套件后天然覆盖）。
RESULT_LINE_RE='^(passed|failed|broken|skipped)'
first_hit_waited=0
for _ in $(seq 1 "$TIMEOUT_BOOT"); do
    if [ -f "$SERIAL_LOG" ]; then
        # 直读文件（grep -qa，binary-as-text）：`strings | grep -q` 在
        # pipefail 下命中也判 141（§续-277b 定谳的 SIGPIPE 假失败模式）。
        if grep -qa 'kernel panic\|PANIC' "$SERIAL_LOG" 2>/dev/null; then
            verdict="PANIC"; break
        fi
        if grep -qaE "$RESULT_LINE_RE" "$SERIAL_LOG" 2>/dev/null; then
            if [ "$first_hit_waited" -ge 15 ]; then
                if grep -qaE '^failed|^broken' "$SERIAL_LOG" 2>/dev/null; then
                    verdict="FAILED"
                elif grep -qa '^passed' "$SERIAL_LOG" 2>/dev/null; then
                    verdict="PASSED"
                else
                    verdict="SKIPPED-ONLY"
                fi
                break
            fi
            first_hit_waited=$((first_hit_waited + 1))
        fi
    fi
    sleep 1
done
kill_qemu

if [ "$verdict" = "PASSED" ]; then
    echo "RESULT: PASS (t_memchr memchr_basic ran to completion on the aarch64 boot console — ATF 'passed' via the SYS_DIAGCTL stdio bridge)"
    exit 0
elif [ "$verdict" = "PANIC" ]; then
    echo "FAIL: kernel panic during ATF boot leg"
    grep -a -m4 -A3 "PANIC" "$SERIAL_LOG" | cat -v || true
    exit 1
elif [ "$verdict" = "FAILED" ]; then
    echo "FAIL: ATF reported 'failed'/'broken' for memchr_basic"
    grep -a -m8 -B2 -E '^failed|^broken' "$SERIAL_LOG" | cat -v || true
    exit 1
elif [ "$verdict" = "SKIPPED-ONLY" ]; then
    echo "FAIL: only 'skipped' result lines on console (no passed/failed) — harness mismatch?"
    grep -a -m8 -E '^skipped' "$SERIAL_LOG" | cat -v || true
    exit 1
else
    echo "FAIL: no ATF verdict within ${TIMEOUT_BOOT}s (marker '$RESULT_MARKER' unseen)"
    strings "$SERIAL_LOG" 2>/dev/null | grep -av "^nk4a\|^vr:\|^pm:" | tail -10 || true
    exit 1
fi
