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
#   1. Cross-build the ATF suite ELFs (tools/build-atf-test.sh per case;
#      requires the aarch64 picolibc vendor leg) + the p1/p2/p3 sentinels.
#   2. Assemble the bootable image (xtask image — seeds /tests, extracts the
#      case manifest from the minix3 C sources, writes atf-plan.txt and the rc
#      variant; the xtask arch gate whitelists aarch64, other arches untouched).
#   3. Boot under AAVMF + QEMU virt, serial to a log.
#   4. Count ATF result lines against atf-plan.txt: every expected `passed`
#      (PASS), any failed/broken or panic (FAIL), stalled progress (FAIL).
#
# Honest posture: this is the first ATF batch (string/memory libtests, one
# process per case) of the 586-test goal. The suite grows through §③'s
# remaining legs (fork/exec/VFS bridges for the syscall-flavoured tests); a
# green batch here proves the *pipeline per case*, not the whole goal.
#
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).
#
# Env knobs: ATF_A64_SKIP_BUILD=1 reuse image + prebuilt ELFs; TIMEOUT_BOOT.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs/os
TIMEOUT_BOOT="${TIMEOUT_BOOT:-260}"
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

# ── Stage 1: cross-build the ATF suite ELFs (picolibc vendor leg) ───────
if [ "${ATF_A64_SKIP_BUILD:-0}" != "1" ]; then
    fail_cases=0
    for t in t_bm t_memchr t_memcpy t_memmem t_memset t_popcount t_strcat \
             t_strchr t_strcmp t_strcpy t_strcspn t_strerror t_stresep \
             t_strlen t_strpbrk t_strrchr t_strspn t_swab; do
        ( cd "$ROOT/.." && bash tools/build-atf-test.sh \
            "minix3/tests/lib/libc/string/$t.c" aarch64 ) >>"$WORK/build.log" 2>&1 || {
            echo "FAIL: build-atf-test.sh ($t) failed"; fail_cases=$((fail_cases+1)); }
    done
    [ "$fail_cases" -gt 0 ] && { echo "FAIL: $fail_cases suite case(s) failed to build"; tail -8 "$WORK/build.log"; exit 1; }
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
PLAN_CHK="$ROOT/target/image/aarch64/atf-plan.txt"
[ -s "$PLAN_CHK" ] || { echo "SKIP: $PLAN_CHK missing (run tools/build-atf-test.sh + xtask image first)"; exit 2; }
[ -f "$IMG" ] || { echo "FAIL: $IMG missing"; exit 1; }
# Guard: the image must not be older than the newest suite ELF (xtask seeds
# /tests only when the host files exist at assembly time; a stale image would
# boot without /tests and every rc line would no-op to "not found" — never PASS).
newest_elf=$(ls -t "$ROOT"/target/atf/aarch64/tests/t_* 2>/dev/null | head -1)
if [ -n "$newest_elf" ] && [ "$newest_elf" -nt "$IMG" ]; then
    echo "FAIL: $newest_elf is newer than $IMG — image assembled before the suite was built"; exit 1
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

# ── Stage 3: count ATF result lines against the manifest ────────────────
# 判据纪律（NK4C 续-278）：套件多案下不"首个 passed 即定案"——以
# target/image/aarch64/atf-plan.txt（xtask 装配产物，prog tc 逐行）为期望数 N：
#   收齐 N 条结果行（passed+failed+broken）或停滞/超时后按终集定案：
#   failed/broken 存在 → FAILED（全部列出，首案作例程，诊断不止于第一例）；
#   passed 计数 >= N → PASSED；
#   首条结果行后连续 120 轮无新结果行 → STALLED（某案挂死/丢进程）。
# 直读文件 grep -qa/-ac（binary-as-text）：`strings | grep -q` 在 pipefail 下
# 命中也判 141（§续-277b 定谳的 SIGPIPE 假失败模式）。
PLAN="$ROOT/target/image/aarch64/atf-plan.txt"
if [ ! -s "$PLAN" ]; then
    echo "FAIL: $PLAN missing/empty — image assembled without ATF suite (stale build?)"; exit 1
fi
EXPECTED=$(grep -c ' ' "$PLAN")
verdict=""
stall_count=0
n_pass_last=-1
for _ in $(seq 1 "$TIMEOUT_BOOT"); do
    if [ -f "$SERIAL_LOG" ]; then
        if grep -qa 'kernel panic\|PANIC' "$SERIAL_LOG" 2>/dev/null; then
            verdict="PANIC"; break
        fi
        n_fail=$(grep -acE '^failed|^broken' "$SERIAL_LOG" 2>/dev/null || true)
        n_pass=$(grep -ac '^passed' "$SERIAL_LOG" 2>/dev/null || true)
        n_pass=${n_pass:-0}; n_fail=${n_fail:-0}
        total=$((n_pass + n_fail))
        if [ "$total" -ge "$EXPECTED" ]; then
            if [ "$n_fail" -gt 0 ]; then verdict="FAILED"; else verdict="PASSED"; fi
            break
        fi
        if [ "$n_pass" -gt 0 ] || [ "$n_fail" -gt 0 ]; then
            if [ "$n_pass_last" -eq "$n_pass" ]; then
                stall_count=$((stall_count + 1))
                if [ "$stall_count" -ge 120 ]; then verdict="STALLED"; break; fi
            else
                stall_count=0
            fi
            n_pass_last=$n_pass
        fi
    fi
    sleep 1
done
kill_qemu

if [ "$verdict" = "PASSED" ]; then
    echo "RESULT: PASS ($EXPECTED/$EXPECTED ATF cases passed on the aarch64 boot console — SYS_DIAGCTL stdio bridge, per atf-plan.txt)"
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
    echo "FAIL: progress stalled at $n_pass/$EXPECTED passed (a case hung or died silently)"
    strings "$SERIAL_LOG" 2>/dev/null | grep -aE "WARNING|^passed|^failed" | tail -6 || true
    exit 1
else
    echo "FAIL: no ATF result lines within ${TIMEOUT_BOOT}s (boot chain regression? expected $EXPECTED cases)"
    strings "$SERIAL_LOG" 2>/dev/null | grep -av "^nk4a\|^vr:\|^pm:" | tail -10 || true
    exit 1
fi
