#!/usr/bin/env bash
# test-atf-x86_64.sh — x86_64 ATF guest-leg acceptance: the upstream C test
# suite boots under OVMF and every case reaches an ATF terminal result.
#
# Why the birth leg changed: the first x86_64 ATF attempt linked host cc +
# glibc. glibc's startup calls arch_prctl(ARCH_SET_FS) and writes errno
# through %fs; this kernel has no per-process FS base, so the test faulted
# before main. The leg now links the same non-TLS picolibc as aarch64/riscv
# (tools/vendor-picolibc-x86_64.sh) and takes its `_start` from
# tools/atf-c-compat/startup-minix.c, which reads the kernel ps_strings ABI
# from RBX. The host-execution gate for the glibc flavor lives in
# tools/atf-host-suite.sh under the separate `x86_64-host` token; it is not
# this gate.
#
# It drives, end to end:
#   1. Build the 18 string-suite ELFs and the p1/p2/p3 bridge probes with
#      tools/build-atf-test.sh x86_64.
#   2. Assemble the bootable image (xtask image --arch x86_64 --release). The
#      ATF_BOOT_LEG_READY whitelist now includes x86_64, so the assembler
#      seeds /tests, injects the rc exec lines and writes atf-plan.txt.
#   3. Boot under OVMF + QEMU q35 with serial to a log.
#   4. Count ATF result lines against atf-plan.txt (the single source of the
#      expected count). Every expected case terminal (passed/skipped), no
#      failed/broken, no panic, no stalled progress -> PASS.
#
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).
#
# Env knobs: ATF_X86_SKIP_BUILD=1 reuse image + prebuilt ELFs;
#            ATF_X86_SMP (default 1, the same shape as the other full-image
#            x86 gates); TIMEOUT_BOOT (default 360).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs/os
TIMEOUT_BOOT="${TIMEOUT_BOOT:-360}"
SMP="${ATF_X86_SMP:-1}"
WORK="$(mktemp -d /tmp/atf_x86_64.XXXXXX)"

EVIDENCE_BASE="/tmp/atf_x86_64_evidence"
archive_evidence() {
    [ -d "$WORK" ] || return 0
    local have=0 f
    for f in serial.log build.log xtask.log; do
        [ -f "$WORK/$f" ] && have=1 && break
    done
    [ "$have" -eq 1 ] || return 0
    local dest="$EVIDENCE_BASE/$(date +%Y%m%d-%H%M%S)-$$"
    mkdir -p "$dest" 2>/dev/null || return 0
    for f in serial.log build.log xtask.log atf-evidence.txt; do
        [ -f "$WORK/$f" ] && cp -p "$WORK/$f" "$dest/$f" 2>/dev/null || true
    done
    echo "evidence archived: $dest" >&2
}
cleanup() { archive_evidence; rm -rf "$WORK"; }
trap cleanup EXIT

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
    echo "SKIP: OVMF firmware not found (install ovmf)"; exit 2
fi

# ── Stage 1: build the ATF suite ELFs (non-TLS picolibc guest leg) ──────
if [ "${ATF_X86_SKIP_BUILD:-0}" != "1" ]; then
    fail_cases=0
    for t in t_bm t_memchr t_memcpy t_memmem t_memset t_popcount t_strcat \
             t_strchr t_strcmp t_strcpy t_strcspn t_strerror t_stresep \
             t_strlen t_strpbrk t_strrchr t_strspn t_swab; do
        ( cd "$ROOT/.." && bash tools/build-atf-test.sh \
            "minix3/tests/lib/libc/string/$t.c" x86_64 ) >>"$WORK/build.log" 2>&1 || {
            echo "FAIL: build-atf-test.sh ($t) failed"; fail_cases=$((fail_cases+1)); }
    done
    [ "$fail_cases" -gt 0 ] && { echo "FAIL: $fail_cases suite case(s) failed to build"; tail -8 "$WORK/build.log"; exit 1; }
    for p in p1 p2 p3; do
        src="$ROOT/../tools/atf-c-compat/probes/$p.c"
        if [ ! -f "$src" ] || ! ( cd "$ROOT/.." && bash tools/build-atf-test.sh "$src" x86_64 ) \
                >>"$WORK/build.log" 2>&1; then
            echo "WARN: bridge probe $p did not build/seed (rc will report not found; not part of the main verdict)"
        fi
    done
    echo "assembling image (xtask image --arch x86_64 --release)…"
    ( cd "$ROOT" && cargo run -q -p xtask -- image --arch x86_64 --release ) \
        >"$WORK/xtask.log" 2>&1 || {
        echo "FAIL: xtask image assembly failed"; tail -8 "$WORK/xtask.log"; exit 1
    }
fi
IMG="$ROOT/target/image/x86_64/minix.img"
PLAN="$ROOT/target/image/x86_64/atf-plan.txt"
[ -s "$PLAN" ] || { echo "SKIP: $PLAN missing (run tools/build-atf-test.sh x86_64 + xtask image first)"; exit 2; }
[ -f "$IMG" ] || { echo "FAIL: $IMG missing"; exit 1; }
newest_elf=$(ls -t "$ROOT"/target/atf/x86_64/tests/t_* 2>/dev/null | head -1)
if [ -n "$newest_elf" ] && [ "$newest_elf" -nt "$IMG" ]; then
    echo "FAIL: $newest_elf is newer than $IMG — image assembled before the suite was built"; exit 1
fi
EXPECTED=$(grep -c ' ' "$PLAN")

# ── Stage 2: freeze build identity (commit, toolchain, per-prog ELF hashes) ──
EVID="$WORK/atf-evidence.txt"
echo "== Stage 2b: ATF build evidence (tools/atf-evidence.sh) =="
bash "$ROOT/../tools/atf-evidence.sh" write x86_64 "$PLAN" "$EVID" \
    || { echo "FAIL: evidence generation failed (plan/progs disagree)"; exit 1; }
echo "ATF-EVIDENCE $(grep -E '^(commit|worktree|toolchain|progs|suite_sha256)=' "$EVID" | tr '\n' ' ')"
bash "$ROOT/../tools/atf-evidence.sh" check x86_64 "$PLAN" "$EVID" \
    || { echo "FAIL: just-written evidence does not re-check"; exit 1; }

# ── Stage 3: boot under OVMF; serial to a log ───────────────────────────
FW_VARS="$WORK/ovmf_vars.fd"
cp "$FW_SRC_VARS" "$FW_VARS"
SERIAL_LOG="$WORK/serial.log"
export TMPDIR=/tmp
timeout "$TIMEOUT_BOOT" qemu-system-x86_64 \
    -machine q35 \
    -smp "$SMP" \
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

# ── Stage 4: count ATF result lines against the manifest ────────────────
# Same verdict discipline as the aarch64/riscv siblings: EXPECTED is the
# plan's `prog tc` line count; a case is terminal at passed/failed/broken/
# skipped; failed/broken outvote everything; 120 one-second polls without a
# new result line is STALLED. Direct grep -ac/-qa on the binary log (no
# `strings | grep -q` under pipefail: SIGPIPE would turn a hit into 141).
verdict=""
stall_count=0
n_total_last=-1
for _ in $(seq 1 "$TIMEOUT_BOOT"); do
    if [ -f "$SERIAL_LOG" ]; then
        if grep -qa 'kernel panic\|PANIC' "$SERIAL_LOG" 2>/dev/null; then
            verdict="PANIC"; break
        fi
        n_fail=$(grep -acE '^failed|^broken' "$SERIAL_LOG" 2>/dev/null || true)
        n_pass=$(grep -ac '^passed' "$SERIAL_LOG" 2>/dev/null || true)
        n_skip=$(grep -acE '^skipped' "$SERIAL_LOG" 2>/dev/null || true)
        n_pass=${n_pass:-0}; n_fail=${n_fail:-0}; n_skip=${n_skip:-0}
        total=$((n_pass + n_fail + n_skip))
        if [ "$total" -ge "$EXPECTED" ]; then
            if [ "$n_fail" -gt 0 ]; then verdict="FAILED"; else verdict="PASSED"; fi
            break
        fi
        if [ "$total" -gt 0 ]; then
            if [ "$n_total_last" -eq "$total" ]; then
                stall_count=$((stall_count + 1))
                if [ "$stall_count" -ge 120 ]; then verdict="STALLED"; break; fi
            else
                stall_count=0
            fi
            n_total_last=$total
        fi
    fi
    sleep 1
done
kill_qemu

if [ "$verdict" = "PASSED" ]; then
    bash "$ROOT/../tools/atf-evidence.sh" check x86_64 "$PLAN" "$EVID" \
        || { echo "RESULT: FAIL — evidence changed during the run, reading not attributable"; exit 1; }
    echo "RESULT: PASS ($EXPECTED/$EXPECTED ATF cases reached a terminal result: $n_pass passed + $n_skip skipped, 0 failed/broken — x86_64 guest leg, -smp $SMP)"
    exit 0
elif [ "$verdict" = "PANIC" ]; then
    echo "FAIL: kernel panic during ATF boot leg"
    grep -a -m4 -A3 "PANIC" "$SERIAL_LOG" | cat -v || true
    exit 1
elif [ "$verdict" = "FAILED" ]; then
    echo "FAIL: ATF reported 'failed'/'broken' on at least one case"
    grep -a -m8 -B2 -E '^failed|^broken' "$SERIAL_LOG" | cat -v || true
    echo "      (context: last case names before the failure)"
    strings "$SERIAL_LOG" 2>/dev/null | grep -aE "^(_?[a-z]+: WARNING|passed|failed)" | tail -12 || true
    exit 1
elif [ "$verdict" = "STALLED" ]; then
    echo "FAIL: progress stalled at $total/$EXPECTED terminal ($n_pass passed + $n_skip skipped; a case hung or died silently)"
    strings "$SERIAL_LOG" 2>/dev/null | grep -aE "WARNING|^passed|^failed|^skipped" | tail -6 || true
    exit 1
else
    echo "FAIL: no ATF result lines within ${TIMEOUT_BOOT}s (boot chain regression? expected $EXPECTED cases)"
    strings "$SERIAL_LOG" 2>/dev/null | grep -av "^nk4a\|^vr:\|^pm:" | tail -10 || true
    exit 1
fi
