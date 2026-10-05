#!/bin/bash
# test-smp-aps-riscv64.sh — P-A64RV-01 riscv 半接线门（§续-398/399/400）：
# -smp 2 + dumpdtb -smp 2（拓扑真值=DTB）；判据 = smp_init 汇聚点到达标记
# （nk4c: ap-arrived）。配方与 boot-full 同源（diff = 核数 + 判据双证）。
# 原始头注：# test-riscv64-boot-full.sh — NK4-C 续-89：riscv64 全系统 boot 装配 + 首跑。
#
# 甲案（kernel-image 自引导）入口腿（续-88）已过 arch_boot 前的模块装载闸，
# 但那只证「bootface 机制通」——本脚本把它推成真系统：装配 12 个 riscv 模块
# + imgrd（含 riscv /bin/{sh,echo,ls,cat}），拼一张 entry_count=12 的真
# BootFileTable，注入 DTB /chosen，用 OpenSBI 直载 + `-device loader` 把
# 表和模块放进物理 RAM，让镜像自引导 → arch_boot → kmain → 12 server birth
# → init → rc marker。
#
# 为什么 OpenSBI 直载而非 U-Boot：bootface 读 a1(DTB)+/chosen(表址)，OpenSBI
# `-kernel` 已实证递 a1=DTB（续-88 M4.3 对账），表经 /chosen 指到 `-device
# loader` 放的 blob 即可——不必等 M4.3「U-Boot bootelf 高半 entry 必陷」那条
# 待裁决腿。产物面与 U-Boot 腿同构（同一张 BootFileTable、同 12 模块布局），
# 差别只在「谁把表址交给镜像」。
#
# 判据（marker = 目标① riscv 的终灯）：串口出现
#   `minix-rs rc: minimal boot script marker`
# 未达 marker 也如实报告 boot 推进到的相位（arch_boot validate / kmain step /
# server birth / VM handoff），作为下一轮取证入口——riscv kmain 全链是首次
# 真跑（rt-birth 只手搬相位），预期会暴露新的架构侧缺口，属正常前沿推进。
#
# 前置：qemu-system-riscv64、dtc/fdtput、rust 工具链（riscv64gc 目标）。缺件 SKIP。
# Exit codes: 0 = marker 命中, 1 = 未达 marker（仍出证据）, 2 = SKIP。

set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs/os
TRIPLE="riscv64gc-unknown-none-elf"
REL="$ROOT/target/$TRIPLE/release"
IMG="$ROOT/target/image/riscv64"
RUN="${RUN:-jfull1}"
TIMEOUT_BOOT="${TIMEOUT_BOOT:-160}"
SERIAL="$ROOT/target/$TRIPLE/boot-full-serial.$RUN.log"
MARKER="minix-rs rc: minimal boot script marker"
# P-A64RV-01 riscv 半（§续-398/399/400）：次级核真醒判据＝smp_init 汇聚点
# 到达标记。rc marker 为辅证。
AP_MARKER="nk4c: ap-arrived"

skip() { echo "SKIP: $1"; exit 2; }
fail() { echo "FAIL: $1"; exit 1; }

command -v qemu-system-riscv64 &>/dev/null || skip "qemu-system-riscv64 not found"
command -v fdtput &>/dev/null || skip "fdtput not found (device-tree-compiler)"
command -v python3 &>/dev/null || skip "python3 not found"

# 物理内存布局（RAM 512M：0x80000000..0xA0000000）。
#   kernel-image   0x80200000 + 4MiB      （镜像 LMA，OpenSBI 装载）
#   bump pool      0x82000000 + 32MiB     （bootface 取页：root/pt/名字池/模块落地）
#   BootFileTable  0x85000000              （chosen 指向；bootface 按界闸读）
#   模块源         0x86000000 起顺序页对齐  （12 个 ELF，bootface 拷进 bump 池）
TABLE_PA=0x85000000
MODULE_BASE=0x86000000

if [ "${SKIP_BUILD:-0}" != "1" ]; then
    echo "== Stage 1: 逐包构建 12 模块 + 命令 + mkfs_mfs（riscv64）=="
    # 逐包独立 cargo 调用（对位 xtask image 的 cargo_release 每包一次）：
    # 一次建多包会跨包合并 feature / 同 crate 撞 panic_impl（duplicate lang
    # item），历史同源于 x86/aarch64 腿也逐包构。
    for p in minix-ds minix-rs minix-pm minix-sched minix-vfs \
             minix-driver-memory minix-driver-tty minix-mib minix-vm \
             minix-fs-pfs minix-fs-mfs minix-init minix-shell minix-fileops; do
        ( cd "$ROOT" && ulimit -v 4194304; \
          cargo build -q --release --target "$TRIPLE" -p "$p" ) || fail "$p 构建失败"
    done
    # 宿主工具 mkfs_mfs（默认三元组）
    ( cd "$ROOT" && cargo build -q --release -p minix-diskfmt ) || fail "mkfs_mfs 构建失败"
    MKFS="$ROOT/target/release/mkfs_mfs"
    [ -x "$MKFS" ] || MKFS="$ROOT/target/release/mkfs_mfs"

    echo "== Stage 2: 生成 riscv imgrd（proto = /etc + riscv /bin/{sh,echo,ls,cat}）=="
    mkdir -p "$IMG" || fail "mkdir $IMG"
    PROTO="$IMG/imgrd.proto"
    # proto 逐字对位 xtask::generate_etc_proto（块数/inode/bsize 同 x86 腿）。
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
    # build.rs 需见到 target/image/riscv64/imgrd.img → 强制重编 mfs
    touch "$ROOT/fs/mfs/src/main.rs"
    ( cd "$ROOT" && ulimit -v 4194304; cargo build -q --release --target "$TRIPLE" -p minix-fs-mfs ) || fail "mfs 重建失败"
    # 校验 imgrd 真嵌进去了（BOOT_IMGRD_DATA 非空 → mfs 体积明显大于无 imgrd）
    ( cd "$ROOT" && ulimit -v 4194304; cargo build -q --release --target "$TRIPLE" --features fw-riscv64-none -p kernel-image ) || fail "kernel-image 构建失败"
fi

[ -x "$REL/kernel" ] || fail "kernel-image 缺失：$REL/kernel"
MOD_COUNT=$(ls "$REL"/minix-{ds,rs,pm,sched,vfs,driver-memory,driver-tty,mib,vm,fs-pfs,fs-mfs,init} 2>/dev/null | wc -l)
[ "$MOD_COUNT" = "12" ] || fail "12 模块不齐（实得 $MOD_COUNT）"

echo "== Stage 4: 拼 BootFileTable + 生成 DTB(chosen) + QEMU loader 参数 =="
BASE_DTB="$IMG/base.dtb"; QEMU_DTB="$IMG/qemu.dtb"; TABLE_BIN="$IMG/table.bin"
rm -f "$BASE_DTB" "$QEMU_DTB" "$TABLE_BIN"
# 拓扑真值=DTB：dump 必须 -smp 2（缺省 1 CPU ⇒ smp_init 多核分支惰性）
timeout 10 qemu-system-riscv64 -machine virt -smp 2 -m 512M -display none -bios default \
    -machine dumpdtb="$BASE_DTB" >/dev/null 2>&1 || true
[ -f "$BASE_DTB" ] || fail "dumpdtb 未产出"
fdtput -p -t x "$BASE_DTB" /chosen opensbi,boot-file-table "$TABLE_PA" || fail "fdtput 注入失败"
cp "$BASE_DTB" "$QEMU_DTB"

# python：读 12 模块 → 顺序页对齐 PA → 写 table.bin + 输出 -device loader 参数
LOADER_ARGS=$(ROOT="$ROOT" MOD_BASE="$MODULE_BASE" TABLE_PA="$TABLE_PA" REL="$REL" python3 - <<'PYEOF'
import os, struct, sys
rel = os.environ["REL"]; base = int(os.environ["MOD_BASE"], 0); tpa = int(os.environ["TABLE_PA"], 0)
# NAME_POOL 权威序 = C table.c：ds,rs,pm,sched,vfs,memory,tty,mib,vm,pfs,mfs,init
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
    # 写模块到独立文件，-device loader 放进 pa
    mf = os.path.join(os.environ["ROOT"], "target/image/riscv64/mod_%s.bin" % name)
    with open(mf, "wb") as g:
        g.write(data)
    loaders.append("-device"); loaders.append("loader,addr=0x%x,file=%s,force-raw=on" % (pa, mf))
    pa += (len(data) + 0xfff) & ~0xfff   # 页对齐游标
# table: magic(u64) + count(u32) + pad(u32) + 16 slots ×(path[64]+u64+u64)
blob = struct.pack("<QII", MAGIC, len(entries), 0)
for path, e_pa, e_len in entries:
    blob += path + struct.pack("<QQ", e_pa, e_len)
blob += bytes((16 - len(entries)) * (64 + 16))  # 补满 16 槽
with open(os.path.join(os.environ["ROOT"], "target/image/riscv64/table.bin"), "wb") as f:
    f.write(blob)
print(" ".join(loaders))
PYEOF
) || fail "表装配失败"
[ -n "$LOADER_ARGS" ] || fail "loader 参数为空"

echo "== Stage 5: OpenSBI 直载点火（-kernel=镜像, -dtb=chosen, loader=表+12模块）=="
pkill -f '[q]emu-system-riscv64' 2>/dev/null || true; sleep 1
rm -f "$SERIAL"
# shellcheck disable=SC2086
timeout "$TIMEOUT_BOOT" qemu-system-riscv64 \
    -machine virt -smp 2 -m 512M -bios default \
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
echo "arch_boot step0 validate ok : $(grep -ac 'step0 validate ok' "$SERIAL")"
echo "kmain 相位 (kernel: kmain)  : $(grep -ac 'kernel: kmain\|vm_handoff\|birth' "$SERIAL")"
echo "server rt-init / run enter  : $(grep -ac 'rt-init\|run enter' "$SERIAL")"
echo "panic 计数                  : $(grep -ac 'panic\|PANIC' "$SERIAL")"
echo "marker 计数                 : $(grep -acF "$MARKER" "$SERIAL")"
echo "----- 串口尾 30 行 -----"
tail -30 "$SERIAL"
AP_OK=0; RC_OK=0
grep -qaF "$AP_MARKER" "$SERIAL" && AP_OK=1
grep -qaF "$MARKER" "$SERIAL" && RC_OK=1
if [ "$AP_OK" = "1" ] && [ "$RC_OK" = "1" ]; then
    echo "### TEST_RESULT: PASS test-smp-aps-riscv64 (secondary hart arrived + BSP full chain) ###"; exit 0
elif [ "$AP_OK" = "1" ]; then
    echo "### TEST_RESULT: PASS test-smp-aps-riscv64 (ap-arrived; BSP chain incomplete) ###"; exit 0
fi
echo "### TEST_RESULT: FAIL test-smp-aps-riscv64 (no ap-arrived marker) ###"
exit 1
