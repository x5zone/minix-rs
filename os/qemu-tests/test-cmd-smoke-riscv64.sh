#!/bin/bash
# test-cmd-smoke-riscv64.sh — riscv64 command-surface acceptance (goal②, 3rd arch).
#
# The x86 sibling `test-cmd-smoke.sh` and the aarch64 sibling
# `test-cmd-smoke-aarch64.sh` prove the 18-stage command face boots and prints the
# `/etc/rc` marker. This is the riscv64 leg of the SAME contract, made repeatable so
# riscv64 command-surface regressions get caught by CI, not just by one-off serial-log
# archaeology.
#
# It drives, end to end:
#   1. Build the 12 riscv64 server modules + commands + kernel-image and assemble
#      the OpenSBI-direct-load boot face (12 modules + BootFileTable, same recipe as
#      `test-riscv64-boot-full.sh`).
#   2. Boot under QEMU virt (OpenSBI `-bios default`), serial to a log.
#   3. Assert the marker line the /etc/rc `echo` prints.
#   4. Assert `ls /bin` and `cat /etc/rc` ran on the real VFS IPC path: the /bin
#      directory listing plus the echoed rc body must appear after the marker, so the
#      command face is exercised, not just the marker printed in isolation.
#
# Honest posture: this test is green only when riscv64 actually runs /etc/rc's
# echo/ls/cat through the VFS; if the boot chain regresses it FAILS with the serial
# tail — which is exactly its job.
#
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).
#
# Env knobs: RS64_SMOKE_SKIP_BUILD=1 reuse existing artifacts (local iteration);
#            RS64_T4_MARKER override the marker; TIMEOUT_BOOT/TIMEOUT_T4 windows.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs/os
TRIPLE="riscv64gc-unknown-none-elf"
REL="$ROOT/target/$TRIPLE/release"
IMG="$ROOT/target/image/riscv64"
TIMEOUT_BOOT="${TIMEOUT_BOOT:-420}"
TIMEOUT_T4="${TIMEOUT_T4:-240}"
T4_MARKER="${RS64_T4_MARKER:-rc: minimal boot script marker}"
WORK="${RS64_SMOKE_WORK:-$(mktemp -d /tmp/cmd_smoke_rs64.XXXXXX)}"
cleanup() { [ -n "${RS64_SMOKE_WORK:-}" ] || rm -rf "$WORK"; }
trap cleanup EXIT

skip() { echo "SKIP: $1"; exit 2; }
fail() { echo "FAIL: $1"; exit 1; }

mkdir -p "$WORK" || fail "工作目录不可建：$WORK"
TABLE_PA=0x85000000
MODULE_BASE=0x86000000

command -v qemu-system-riscv64 &>/dev/null || skip "qemu-system-riscv64 not found"
command -v fdtput &>/dev/null || skip "fdtput not found (device-tree-compiler)"
command -v python3 &>/dev/null || skip "python3 not found"
# strings 属 binutils；缺失时 strings 静默空输出→grep 永不命中会误报 FAIL(1)
# 而非 SKIP(2)（与兄弟脚本对工具件的检查对齐）。
command -v strings &>/dev/null || skip "'strings' (binutils) not found"

if [ "${RS64_SMOKE_SKIP_BUILD:-0}" != "1" ]; then
    echo "== Stage 1: 逐包构建 12 模块 + 命令 + mkfs_mfs（riscv64）=="
    # 与 test-riscv64-boot-full.sh Stage 1-3 同源（逐包独立 cargo 调用：一次建
    # 多包会跨包合并 feature / 撞 panic_impl）。
    for p in minix-ds minix-rs minix-pm minix-sched minix-vfs \
             minix-driver-memory minix-driver-tty minix-mib minix-vm \
             minix-fs-pfs minix-fs-mfs minix-init minix-shell minix-fileops; do
        ( cd "$ROOT" && ulimit -v 4194304; \
          cargo build -q --release --target "$TRIPLE" -p "$p" ) || fail "$p 构建失败"
    done
    ( cd "$ROOT" && cargo build -q --release -p minix-diskfmt ) || fail "mkfs_mfs 构建失败"
    MKFS="$ROOT/target/release/mkfs_mfs"

    echo "== Stage 2: 生成 riscv imgrd（/etc + /bin/{sh,echo,ls,cat}）=="
    mkdir -p "$IMG" || fail "mkdir $IMG"
    PROTO="$IMG/imgrd.proto"
    {
        printf 'minix-rs imgrd\n2048 0\nd--755 0 0\n'
        printf 'etc d--755 0 0\nrc ---755 0 0 etc/rc\nttys ---644 0 0 etc/ttys\n$\n'
        printf 'dev d--755 0 0\nconsole c--600 0 0 4 0\n$\n'
        printf 'bin d--755 0 0\n'
        for n in sh echo ls cat; do printf '%s ---755 0 0 target/%s/release/%s\n' "$n" "$TRIPLE" "$n"; done
        printf '$\n$\n'
    } > "$PROTO"
    ( cd "$ROOT" && "$MKFS" "$IMG/imgrd.img" 2048 0 4096 -p "$PROTO" ) || fail "mkfs_mfs 播种失败"

    echo "== Stage 3: 重建 mfs（嵌 imgrd）+ kernel-image =="
    touch "$ROOT/fs/mfs/src/main.rs"
    ( cd "$ROOT" && ulimit -v 4194304; cargo build -q --release --target "$TRIPLE" -p minix-fs-mfs ) || fail "mfs 重建失败"
    ( cd "$ROOT" && ulimit -v 4194304; cargo build -q --release --target "$TRIPLE" --features fw-riscv64-none -p kernel-image ) || fail "kernel-image 构建失败"
fi

[ -x "$REL/kernel" ] || fail "kernel-image 缺失：$REL/kernel（先跑一次构建或去掉 RS64_SMOKE_SKIP_BUILD）"
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

# python：读 12 模块 → 页对齐 PA → 写 mod_*.bin + table.bin + loader 参数
# （每次从 REL 新鲜重写，钉死「串口内容 == REL 构建产物」）。
LOADER_ARGS=$(ROOT="$ROOT" MOD_BASE="$MODULE_BASE" TABLE_PA="$TABLE_PA" REL="$REL" IMG="$IMG" python3 - <<'PYEOF'
import os, struct
rel = os.environ["REL"]; img = os.environ["IMG"]
base = int(os.environ["MOD_BASE"], 0); tpa = int(os.environ["TABLE_PA"], 0)
mods = [("ds","minix-ds"),("rs","minix-rs"),("pm","minix-pm"),("sched","minix-sched"),
        ("vfs","minix-vfs"),("memory","minix-driver-memory"),("tty","minix-driver-tty"),
        ("mib","minix-mib"),("vm","minix-vm"),("pfs","minix-fs-pfs"),("mfs","minix-fs-mfs"),
        ("init","minix-init")]
MAGIC = 0x31544f4f42584e4d  # MNXBOOT1
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
with open(os.environ.get("TABLE_BIN") or os.path.join(img, "table.bin"), "wb") as f:
    f.write(blob)
print(" ".join(loaders))
PYEOF
) || fail "表装配失败"
[ -n "$LOADER_ARGS" ] || fail "loader 参数为空"
TABLE_BIN="$IMG/table.bin"

SERIAL_LOG="$WORK/serial.log"
echo "== Stage 5: OpenSBI 直载点火（-kernel=镜像, -dtb=chosen, loader=表+12模块）=="
# 代码审阅 P1（续-339b）：先删旧日志再点火——`RS64_SMOKE_WORK` 复用目录时，
# 上一轮的 serial.log 会让本轮在 0.1s 内凭旧证据假绿（QEMU 未启动/未截断日志
# 的窗口）；test-riscv64-boot-full.sh:136 已有同款 `rm -f` 先例。
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

# ── Stage 6: wait for the command-output marker (the /etc/rc `echo`). ──
# 轮询用 `grep -qa` 直读串口文件（binary-as-text），不走 `strings | grep -q`：
# pipefail 下 grep -q 命中即退→上游 strings 收 SIGPIPE→管道退出码 141→命中被
# 误判为未命中（§续-277 定谳；与两兄弟脚本同款防线）。
reached=0
for _ in $(seq 1 "$TIMEOUT_BOOT"); do
    if [ -f "$SERIAL_LOG" ] && grep -qa "$T4_MARKER" "$SERIAL_LOG" 2>/dev/null; then
        reached=1; break
    fi
    kill -0 "$QEMU_PID" 2>/dev/null || break
    sleep 1
done
if [ "$reached" -ne 1 ]; then
    echo "FAIL: marker '$T4_MARKER' never appeared within ${TIMEOUT_BOOT}s (riscv64 boot chain regression?)"
    tail -12 "$SERIAL_LOG" 2>/dev/null | cat -v || true
    exit 1   # EXIT trap 已 kill_qemu，不重复调
fi
echo "riscv64: command marker reached — checking ls /bin + cat /etc/rc ran"

# ── Stage 7: command-face assertions. The /etc/rc runs `echo` (marker), `ls /bin`
# and `cat /etc/rc`; the console must show all three, in order, on the real VFS path.
# Judging notes (learned from the first two riscv runs, whose shapes differed):
#   * The echo output line can be mangled by the interleaved protocol lines, so a
#     marker match may land on the same string inside `cat /etc/rc`'s dump instead —
#     either occurrence still proves the script reached the console (cat runs after
#     echo/ls in the script).
#   * The `ls /bin` listing (bare `cat`/`echo`/`ls`/`sh` lines) and the `cat` dump
#     (`#!/bin/sh` + the rc body) are interleaved with protocol lines, so the listing
#     is matched with a tolerant in-order regex, not as a contiguous block.
#   * Deliberately NOT asserted: `/tests/*: not found` — that is the rc script's ATF
#     section and its shape changes when the riscv ATF suite lands (goal③).
cmds_ok=0
for _ in $(seq 1 "$TIMEOUT_T4"); do
    if [ -f "$SERIAL_LOG" ]; then
        if python3 - "$SERIAL_LOG" "$T4_MARKER" <<'PYEOF'
import re, sys
raw = open(sys.argv[1], "rb").read().decode("ascii", "replace")
marker = sys.argv[2]
ok_marker = marker in raw
ok_dump = "#!/bin/sh" in raw and "minix-rs /etc/rc" in raw
# In-order bare entry lines of `ls /bin` (arbitrary interleaved lines between them).
ok_listing = re.search(
    r"(?m)^cat\r?$\n(?:.*\n)*?^echo\r?$\n(?:.*\n)*?^ls\r?$\n(?:.*\n)*?^sh\r?$", raw
) is not None
print("  cmdface: marker=%s dump=%s listing=%s" % (ok_marker, ok_dump, ok_listing))
sys.exit(0 if (ok_marker and ok_dump and ok_listing) else 1)
PYEOF
        then
            cmds_ok=1; break
        fi
    fi
    sleep 1
done
kill "$QEMU_PID" 2>/dev/null || true   # 停住串口增长；wait 由 EXIT trap 兜底

if [ "$cmds_ok" -ne 1 ]; then
    echo "FAIL: /etc/rc command face (marker + ls /bin listing + cat /etc/rc dump) not fully observed"
    echo "      VFS command chain incomplete? serial tail:"
    strings "$SERIAL_LOG" 2>/dev/null | tail -12 || true
    exit 1
fi

echo "RESULT: PASS (riscv64 18-stage command output — marker + ls /bin listing + cat /etc/rc dump — on the VFS IPC path)"
exit 0
