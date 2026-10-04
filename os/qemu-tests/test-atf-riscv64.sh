#!/usr/bin/env bash
# test-atf-riscv64.sh — NK4-C 目标③ riscv64 acceptance: the minix3 ATF C suite
# reaches terminal results on the riscv64 boot console.
#
# The aarch64 sibling `test-atf-aarch64.sh` is the same contract on the first
# arch; this is the riscv leg, unblocked by §续-338 (the exec stack top had to
# land inside the Sv39 user range before any exec'd test binary could run).
#
# It drives, end to end:
#   1. Cross-build the ATF suite ELFs (tools/build-atf-test.sh per case; the
#      riscv leg links the custom birth stub since §续-338) + p1/p2/p3 sentinels.
#   2. Assemble the ATF face via `xtask atf-face --arch riscv64` (single source:
#      rc variant with the suite exec lines + atf-plan.txt), seed /tests into the
#      riscv imgrd, rebuild mfs + kernel-image, then the OpenSBI-direct-load
#      BootFileTable face (same recipe as test-riscv64-boot-full.sh).
#   3. Boot under QEMU virt, serial to a log.
#   4. Count ATF result lines against atf-plan.txt: every expected case reaches a
#      terminal result (passed/skipped) → PASS; any failed/broken or panic (FAIL);
#      stalled progress (FAIL).
#
# Honest posture: this is the first ATF batch on riscv (string/memory libtests,
# one process per case). §续-277 定谳 that the riscv suite binaries previously
# could not boot at all (picolibc default crt0); a green run here proves the
# birth leg + stdio bridge + PM exit + VFS lstat round-trip work on this arch.
#
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).
#
# Env knobs: RS_ATF_SKIP_BUILD=1 reuse suite ELFs + image artifacts;
#            RS_ATF_WORK=<dir> keep the work dir (serial log inspection);
#            TIMEOUT_BOOT window.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs/os
TRIPLE="riscv64gc-unknown-none-elf"
REL="$ROOT/target/$TRIPLE/release"
IMG="$ROOT/target/image/riscv64"
PLAN="$IMG/atf-plan.txt"
# riscv 上机节奏实测（§续-340，探针重度在场的诊断构建）：约 1.5-2 分钟/例
# （每例 exec 都触发 VM 页表分配的 `nk4a:` 探针洪流，串口写是瓶颈）；36 例全
# 套件在 1800s 窗内推进到 23/36（21 passed+1 failed+1 skipped）。窗口按
# 60 分钟给足；Phase E 滚除探针后应显著加快（届时可回收此值）。
TIMEOUT_BOOT="${TIMEOUT_BOOT:-3600}"
WORK="${RS_ATF_WORK:-$(mktemp -d /tmp/atf_rs64.XXXXXX)}"
mkdir -p "$WORK" || { echo "FAIL: 工作目录不可建：$WORK"; exit 1; }
TABLE_PA=0x85000000
MODULE_BASE=0x86000000
IMGRD_BLOCKS=4096

cleanup() { [ -n "${RS_ATF_WORK:-}" ] || rm -rf "$WORK"; }
trap cleanup EXIT

skip() { echo "SKIP: $1"; exit 2; }
fail() { echo "FAIL: $1"; exit 1; }

command -v qemu-system-riscv64 &>/dev/null || skip "qemu-system-riscv64 not found"
command -v fdtput &>/dev/null || skip "fdtput not found (device-tree-compiler)"
command -v python3 &>/dev/null || skip "python3 not found"
command -v strings &>/dev/null || skip "'strings' (binutils) not found"

ATF_TESTS="t_bm t_memchr t_memcpy t_memmem t_memset t_popcount t_strcat t_strchr \
           t_strcmp t_strcpy t_strcspn t_strerror t_stresep t_strlen t_strpbrk \
           t_strrchr t_strspn t_swab"

if [ "${RS_ATF_SKIP_BUILD:-0}" != "1" ]; then
    echo "== Stage 1: 交叉构建 ATF 套件（riscv64，自定义出生腿）=="
    fail_cases=0
    for t in $ATF_TESTS; do
        ( cd "$ROOT/.." && bash tools/build-atf-test.sh \
            "minix3/tests/lib/libc/string/$t.c" riscv64 ) >>"$WORK/build.log" 2>&1 || {
            echo "FAIL: build-atf-test.sh ($t) failed"; fail_cases=$((fail_cases+1)); }
    done
    [ "$fail_cases" -gt 0 ] && { echo "FAIL: $fail_cases suite case(s) failed to build"; tail -8 "$WORK/build.log"; exit 1; }
    for p in p1 p2 p3; do
        src="$ROOT/../tools/atf-c-compat/probes/$p.c"
        if [ ! -f "$src" ] || ! ( cd "$ROOT/.." && bash tools/build-atf-test.sh "$src" riscv64 ) \
                >>"$WORK/build.log" 2>&1; then
            echo "WARN: 桥面哨兵 $p 未能构建/播种——rc 将报 not found（不阻断本门主判据）"
        fi
    done
fi

echo "== Stage 2: ATF 面（xtask atf-face：rc 变体 + atf-plan.txt，单源）=="
if [ "${RS_ATF_SKIP_BUILD:-0}" != "1" ] || [ ! -s "$PLAN" ]; then
    ( cd "$ROOT" && cargo run -q -p xtask -- atf-face --arch riscv64 ) >"$WORK/atf_face.log" 2>&1 || {
        echo "FAIL: xtask atf-face 失败"; tail -6 "$WORK/atf_face.log"; exit 1; }
fi
[ -s "$PLAN" ] || fail "$PLAN missing/empty — ATF 面未产出（先跑 tools/build-atf-test.sh + xtask atf-face）"
[ -s "$IMG/rc.imgrd" ] || fail "$IMG/rc.imgrd missing — rc 变体未产出"
EXPECTED=$(grep -c ' ' "$PLAN")
[ "$EXPECTED" -gt 0 ] || fail "$PLAN 无有效条目"

# 套件 prog 清单（从 atf-face 的 tests.<prog>=<rel> 行解析；非 xtask 重跑路径
# 回退为对 plan 去重取首字段）。
if [ -s "$WORK/atf_face.log" ]; then
    mapfile -t TEST_PROGS < <(sed -n 's/^tests\.\([^=]*\)=.*/\1/p' "$WORK/atf_face.log")
    mapfile -t TEST_RELS  < <(sed -n 's/^tests\.[^=]*=//p' "$WORK/atf_face.log")
else
    mapfile -t TEST_PROGS < <(awk '{print $1}' "$PLAN" | sort -u)
    TEST_RELS=()
    for p in "${TEST_PROGS[@]}"; do TEST_RELS+=("target/atf/riscv64/tests/$p"); done
fi
[ "${#TEST_PROGS[@]}" -gt 0 ] || fail "套件 prog 清单为空"

if [ "${RS_ATF_SKIP_BUILD:-0}" != "1" ]; then
    echo "== Stage 3: 逐包构建 12 模块 + 命令 + imgrd（含 /tests + rc 变体）=="
    for p in minix-ds minix-rs minix-pm minix-sched minix-vfs \
             minix-driver-memory minix-driver-tty minix-mib minix-vm \
             minix-fs-pfs minix-fs-mfs minix-init minix-shell minix-fileops; do
        ( cd "$ROOT" && ulimit -v 4194304; \
          cargo build -q --release --target "$TRIPLE" -p "$p" ) || fail "$p 构建失败"
    done
    ( cd "$ROOT" && cargo build -q --release -p minix-diskfmt ) || fail "mkfs_mfs 构建失败"
    MKFS="$ROOT/target/release/mkfs_mfs"
    PROTO="$IMG/imgrd.proto"
    {
        printf 'minix-rs imgrd\n%d 0\nd--755 0 0\n' "$IMGRD_BLOCKS"
        printf 'etc d--755 0 0\nrc ---755 0 0 %s\nttys ---644 0 0 etc/ttys\n$\n' "$IMG/rc.imgrd"
        printf 'dev d--755 0 0\nconsole c--600 0 0 4 0\n$\n'
        printf 'tests d--755 0 0\n'
        for i in "${!TEST_PROGS[@]}"; do
            printf '%s ---755 0 0 %s\n' "${TEST_PROGS[$i]}" "${TEST_RELS[$i]}"
        done
        printf '$\n'
        printf 'bin d--755 0 0\n'
        for n in sh echo ls cat; do printf '%s ---755 0 0 target/%s/release/%s\n' "$n" "$TRIPLE" "$n"; done
        printf '$\n$\n'
    } > "$PROTO"
    ( cd "$ROOT" && "$MKFS" "$IMG/imgrd.img" "$IMGRD_BLOCKS" 0 4096 -p "$PROTO" ) || fail "mkfs_mfs 播种失败"
    touch "$ROOT/fs/mfs/src/main.rs"
    ( cd "$ROOT" && ulimit -v 4194304; cargo build -q --release --target "$TRIPLE" -p minix-fs-mfs ) || fail "mfs 重建失败"
    ( cd "$ROOT" && ulimit -v 4194304; cargo build -q --release --target "$TRIPLE" --features fw-riscv64-none -p kernel-image ) || fail "kernel-image 构建失败"
fi

[ -x "$REL/kernel" ] || fail "kernel-image 缺失：$REL/kernel"
# 诊断检查（CodeReview 续-339b P2-4，与 boot-full 同款）：12 模块缺件要在
# 装配前显式失败——否则 QEMU 装载阶段才以错位形态暴露，难归因。
MOD_COUNT=$(ls "$REL"/minix-{ds,rs,pm,sched,vfs,driver-memory,driver-tty,mib,vm,fs-pfs,fs-mfs,init} 2>/dev/null | wc -l)
[ "$MOD_COUNT" = "12" ] || fail "12 模块不齐（实得 $MOD_COUNT）"

echo "== Stage 4: 拼 BootFileTable + 生成 DTB(chosen) + QEMU loader 参数 =="
BASE_DTB="$IMG/base.dtb"; QEMU_DTB="$IMG/qemu.dtb"; TABLE_BIN="$IMG/table.bin"
rm -f "$BASE_DTB" "$QEMU_DTB" "$TABLE_BIN"
timeout 10 qemu-system-riscv64 -machine virt -m 512M -display none -bios default \
    -machine dumpdtb="$BASE_DTB" >/dev/null 2>&1 || true
[ -f "$BASE_DTB" ] || fail "dumpdtb 未产出"
fdtput -p -t x "$BASE_DTB" /chosen opensbi,boot-file-table "$TABLE_PA" || fail "fdtput 注入失败"
cp "$BASE_DTB" "$QEMU_DTB"
LOADER_ARGS=$(ROOT="$ROOT" MOD_BASE="$MODULE_BASE" TABLE_PA="$TABLE_PA" REL="$REL" IMG="$IMG" python3 - <<'PYEOF'
import os, struct
rel = os.environ["REL"]; img = os.environ["IMG"]
base = int(os.environ["MOD_BASE"], 0); tpa = int(os.environ["TABLE_PA"], 0)
mods = [("ds","minix-ds"),("rs","minix-rs"),("pm","minix-pm"),("sched","minix-sched"),
        ("vfs","minix-vfs"),("memory","minix-driver-memory"),("tty","minix-driver-tty"),
        ("mib","minix-mib"),("vm","minix-vm"),("pfs","minix-fs-pfs"),("mfs","minix-fs-mfs"),
        ("init","minix-init")]
MAGIC = 0x31544f4f42584e4d
path_field = lambda name: (("/EFI/minix/modules/"+name).encode()+b"\0"*64)[:64]
pa = base
entries = []
loaders = []
for name, binname in mods:
    with open(os.path.join(rel, binname), "rb") as f:
        data = f.read()
    entries.append((path_field(name), pa, len(data)))
    mf = os.path.join(img, "mod_%s.bin" % name)
    with open(mf, "wb") as g:
        g.write(data)
    loaders.append("-device"); loaders.append("loader,addr=0x%x,file=%s,force-raw=on" % (pa, mf))
    pa += (len(data) + 0xfff) & ~0xfff
blob = struct.pack("<QII", MAGIC, len(entries), 0)
for path, e_pa, e_len in entries:
    blob += path + struct.pack("<QQ", e_pa, e_len)
blob += bytes((16 - len(entries)) * (64 + 16))
with open(os.path.join(img, "table.bin"), "wb") as f:
    f.write(blob)
print(" ".join(loaders))
PYEOF
) || fail "表装配失败"
[ -n "$LOADER_ARGS" ] || fail "loader 参数为空"

SERIAL_LOG="$WORK/serial.log"
echo "== Stage 5: OpenSBI 直载点火（-kernel=镜像, -dtb=chosen, loader=表+12模块）=="
# 代码审阅 P1（续-339b 同族）：先删旧日志再点火——`RS_ATF_WORK` 复用目录时
# 上一轮的 serial.log 会让本轮凭旧证据假绿；boot-full 脚本已有同款 `rm -f` 先例。
rm -f "$SERIAL_LOG"
# shellcheck disable=SC2086
qemu-system-riscv64 \
    -machine virt -smp 1 -m 512M -bios default \
    -kernel "$REL/kernel" \
    -dtb "$QEMU_DTB" \
    -device "loader,addr=$TABLE_PA,file=$TABLE_BIN,force-raw=on" \
    $LOADER_ARGS \
    -serial "file:$SERIAL_LOG" -display none -no-reboot &
QEMU_PID=$!
kill_qemu() { kill "$QEMU_PID" 2>/dev/null || true; wait "$QEMU_PID" 2>/dev/null || true; }
trap 'kill_qemu; cleanup' EXIT

# ── Stage 6: count ATF result lines against the manifest （判据结构与
# test-atf-aarch64.sh Stage 3 同源：终集={passed,failed,broken,skipped}，
# failed/broken 一票否决，停滞 120 轮判 STALLED；直读文件 grep -qa/-ac 防
# SIGPIPE-141 假失败）。──
verdict=""
stall_count=0
n_total_last=-1
n_pass=0; n_fail=0; n_skip=0; total=0
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
    kill -0 "$QEMU_PID" 2>/dev/null || break
    sleep 1
done
kill_qemu

if [ "$verdict" = "PASSED" ]; then
    echo "RESULT: PASS ($EXPECTED/$EXPECTED riscv64 ATF cases reached a terminal result: $n_pass passed + $n_skip skipped, 0 failed/broken — SYS_DIAGCTL stdio bridge, per atf-plan.txt)"
    exit 0
elif [ "$verdict" = "PANIC" ]; then
    echo "FAIL: kernel panic during ATF boot leg"
    grep -a -m4 -A3 "PANIC" "$SERIAL_LOG" | cat -v || true
    exit 1
elif [ "$verdict" = "FAILED" ]; then
    echo "FAIL: ATF reported 'failed'/'broken' on at least one case"
    grep -a -m8 -B2 -E '^failed|^broken' "$SERIAL_LOG" | cat -v || true
    strings "$SERIAL_LOG" 2>/dev/null | grep -aE "^(_?[a-z]+: WARNING|passed|failed)" | tail -12 || true
    exit 1
elif [ "$verdict" = "STALLED" ]; then
    echo "FAIL: progress stalled at $total/$EXPECTED terminal ($n_pass passed + $n_skip skipped; a case hung or died silently)"
    strings "$SERIAL_LOG" 2>/dev/null | grep -aE "WARNING|^passed|^failed|^skipped" | tail -6 || true
    exit 1
elif [ "$total" -gt 0 ]; then
    echo "FAIL: incomplete — only $total/$EXPECTED cases reached a terminal result within ${TIMEOUT_BOOT}s ($n_pass passed + $n_fail failed + $n_skip skipped)"
    strings "$SERIAL_LOG" 2>/dev/null | grep -aE "^passed|^failed|^skipped" | tail -8 || true
    exit 1
else
    echo "FAIL: no ATF result lines within ${TIMEOUT_BOOT}s (boot chain regression? expected $EXPECTED cases)"
    strings "$SERIAL_LOG" 2>/dev/null | grep -av "^nk4a\|^vr:\|^pm:" | tail -10 || true
    exit 1
fi
