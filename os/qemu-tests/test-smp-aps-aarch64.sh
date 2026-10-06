#!/bin/bash
# test-smp-aps-aarch64.sh — P-A64RV-01 aarch64 半专用门（§续-405/406）。
#
# 判据：-smp 2 下次级核经停车邮箱交付（kernel-image `_start` 选举＋
# AP_GO 发布），起桩装 BSP 的 TTBR/MAIR/TCR、开 MMU 跳汇聚点置
# AP_ARRIVED——BSP 侧打印 `nk4c: ap-arrived cpu=<hw>` 为准。
#
# §续-406 起走 QEMU 直核 `-kernel` ELF 引导（§续-88 甲案 aarch64 形）：
# 非 Linux ELF 走 do_cpu_reset 的 !is_linux 臂＝全 CPU 进同一入口（与
# riscv fw_dynamic 同模型），无 AAVMF/TF-A——§续-405 定谳的 UEFI 链
# 次级核交付缺陷（EDK2 早年唤醒、EBS 后覆写、CPU_ON 交付不了已 ON 的
# 核）整段绕开。五段对位 test-smp-aps-riscv64.sh。
#
# 物理内存布局（RAM 512M：0x40000000..0x60000000）：
#   kernel-image   0x40200000 + 4MiB     （镜像 LMA，QEMU -kernel ELF 装载）
#   DTB            0x40000000            （arm_load_dtb 的 ELF 臂落点）
#   bump 池        0x44000000 + 32MiB    （bootface_a64 取页）
#   BootFileTable  0x47000000            （chosen 指向；bootface 按界闸读）
#   模块源         0x48000000 起顺序页对齐（12 个 ELF）
#
# 前置：qemu-system-aarch64、dtc/fdtput、rust 工具链（aarch64 目标）。
# Exit codes: 0 = PASS（ap-arrived 到达）, 1 = FAIL, 2 = SKIP。

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs/os
TRIPLE="aarch64-unknown-none"
REL="$ROOT/target/$TRIPLE/release"
IMG="$ROOT/target/image/aarch64"
RUN="${RUN:-smpd1}"
TIMEOUT_BOOT="${TIMEOUT_BOOT:-300}"
AP_MARKER="nk4c: ap-arrived"
TABLE_PA=0x47000000
MODULE_BASE=0x48000000

fail() { echo "FAIL: $1"; exit 1; }
skip() { echo "SKIP: $1"; exit 2; }

command -v qemu-system-aarch64 &>/dev/null || skip "qemu-system-aarch64 not found"
command -v fdtput &>/dev/null || skip "fdtput not found (device-tree-compiler)"
command -v python3 &>/dev/null || skip "python3 not found"

if [ "${SKIP_BUILD:-0}" != "1" ]; then
    echo "== Stage 1: 逐包构建 12 模块 + 命令 + mkfs_mfs（aarch64）=="
    for p in minix-ds minix-rs minix-pm minix-sched minix-vfs \
             minix-driver-memory minix-driver-tty minix-mib minix-vm \
             minix-fs-pfs minix-fs-mfs minix-init minix-shell minix-fileops; do
        ( cd "$ROOT" && ulimit -v 4194304; \
          cargo build -q --release --target "$TRIPLE" -p "$p" ) || fail "$p 构建失败"
    done
    ( cd "$ROOT" && cargo build -q --release -p minix-diskfmt ) || fail "mkfs_mfs 构建失败"
    MKFS="$ROOT/target/release/mkfs_mfs"

    echo "== Stage 2: 生成 aarch64 imgrd（proto = /etc + aarch64 /bin）=="
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

    echo "== Stage 3: 重建 mfs（嵌 imgrd）+ kernel-image（fw-aarch64-none，SD-23 RUSTFLAGS）=="
    touch "$ROOT/fs/mfs/src/main.rs"
    ( cd "$ROOT" && ulimit -v 4194304; cargo build -q --release --target "$TRIPLE" -p minix-fs-mfs ) || fail "mfs 重建失败"
    ( cd "$ROOT" && ulimit -v 4194304; \
      RUSTFLAGS="-C target-feature=-neon,-fp-armv8" \
      cargo build -q --release --target "$TRIPLE" --features fw-aarch64-none -p kernel-image ) || fail "kernel-image 构建失败"
fi

[ -x "$REL/kernel" ] || fail "kernel-image 缺失：$REL/kernel"
MOD_COUNT=$(ls "$REL"/minix-{ds,rs,pm,sched,vfs,driver-memory,driver-tty,mib,vm,fs-pfs,fs-mfs,init} 2>/dev/null | wc -l)
[ "$MOD_COUNT" = "12" ] || fail "12 模块不齐（实得 $MOD_COUNT）"

echo "== Stage 4: 拼 BootFileTable + 生成 DTB(chosen) + QEMU loader 参数 =="
BASE_DTB="$IMG/smp_base.dtb"; QEMU_DTB="$IMG/smp_qemu.dtb"; TABLE_BIN="$IMG/smp_table.bin"
rm -f "$BASE_DTB" "$QEMU_DTB" "$TABLE_BIN"
# 拓扑真值=DTB：dump 必须 -smp 2
timeout 10 qemu-system-aarch64 -machine virt,gic-version=3 -smp 2 -m 512M -display none \
    -machine dumpdtb="$BASE_DTB" >/dev/null 2>&1 || true
[ -f "$BASE_DTB" ] || fail "dumpdtb 未产出"
fdtput -p -t x "$BASE_DTB" /chosen opensbi,boot-file-table "$TABLE_PA" || fail "fdtput 注入失败"
cp "$BASE_DTB" "$QEMU_DTB"

LOADER_ARGS=$(ROOT="$ROOT" MOD_BASE="$MODULE_BASE" TABLE_PA="$TABLE_PA" REL="$REL" python3 - <<'PYEOF'
import os, struct
rel = os.environ["REL"]; base = int(os.environ["MOD_BASE"], 0); tpa = int(os.environ["TABLE_PA"], 0)
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
    path = path_field(name)
    entries.append((path, pa, len(data)))
    mf = os.path.join(os.environ["ROOT"], "target/image/aarch64/mod_%s.bin" % name)
    with open(mf, "wb") as g:
        g.write(data)
    loaders.append("-device"); loaders.append("loader,addr=0x%x,file=%s,force-raw=on" % (pa, mf))
    pa += (len(data) + 0xfff) & ~0xfff
blob = struct.pack("<QII", MAGIC, len(entries), 0)
for path, e_pa, e_len in entries:
    blob += path + struct.pack("<QQ", e_pa, e_len)
blob += bytes((16 - len(entries)) * (64 + 16))
with open(os.path.join(os.environ["ROOT"], "target/image/aarch64/smp_table.bin"), "wb") as f:
    f.write(blob)
print(" ".join(loaders))
PYEOF
) || fail "表装配失败"
[ -n "$LOADER_ARGS" ] || fail "loader 参数为空"

echo "== Stage 5: QEMU 直核点火（-kernel=镜像 ELF, -dtb=chosen, loader=表+12模块）=="
pkill -f '[q]emu-system-aarch64' 2>/dev/null || true; sleep 1
SERIAL="$IMG/smp_serial_$RUN.log"
rm -f "$SERIAL"
# shellcheck disable=SC2086
timeout "$TIMEOUT_BOOT" qemu-system-aarch64 \
    -machine virt,gic-version=3 -cpu cortex-a57 -smp 2 -m 512M \
    -kernel "$REL/kernel" \
    -dtb "$QEMU_DTB" \
    -device "loader,addr=$TABLE_PA,file=$TABLE_BIN,force-raw=on" \
    $LOADER_ARGS \
    -serial "file:$SERIAL" -display none -no-reboot &
QPID=$!
trap 'kill $QPID 2>/dev/null || true; wait $QPID 2>/dev/null || true' EXIT
for _ in $(seq 1 "$TIMEOUT_BOOT"); do
    grep -qaF "$AP_MARKER" "$SERIAL" 2>/dev/null && break
    kill -0 "$QPID" 2>/dev/null || break
    sleep 1
done
kill "$QPID" 2>/dev/null || true; wait "$QPID" 2>/dev/null || true; trap - EXIT

[ -f "$SERIAL" ] || fail "无串口日志"
echo "----- boot 推进证据（关键相位计数）-----"
echo "self-boot 横幅              : $(grep -ac 'aarch64 self-boot' "$SERIAL")"
echo "arch_boot step0 validate ok : $(grep -ac 'step0 validate ok' "$SERIAL")"
echo "kmain 相位                  : $(grep -ac 'kernel: kmain\|vm_handoff' "$SERIAL")"
echo "ap-arrived                  : $(grep -acF "$AP_MARKER" "$SERIAL")"
echo "ap-timeout                  : $(grep -ac 'ap-timeout' "$SERIAL")"
echo "panic                       : $(grep -ac 'panicked' "$SERIAL")"
echo "----- 串口尾 30 行 -----"
tail -30 "$SERIAL"
if grep -qaF "$AP_MARKER" "$SERIAL"; then
    echo "### TEST_RESULT: PASS test-smp-aps-aarch64 (secondary cpu reached convergence) ###"
    exit 0
fi
echo "### TEST_RESULT: FAIL test-smp-aps-aarch64 (no ap-arrived marker) ###"
exit 1
