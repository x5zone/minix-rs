#!/bin/bash
# riscv_halt_dump.sh — NK4-C 续-281 零扰动取证：崩后 QEMU monitor pmemsave 全 RAM 保全
# + python3 软件走 VM 页表比对 text 页 + 全 RAM text 页别名普查。
#
# 与 test-riscv64-boot-full.sh 的关系：复用其 Stage 4 产物（table.bin + mod_*.bin +
# qemu.dtb，SKIP_BUILD 门跑一次即齐），只把 Stage 5 的 QEMU 启动加上
# -monitor unix socket；崩溃 halt 后经 monitor pmemsave 512MB 到盘，QEMU 退出。
# guest 已 halt ⇒ dump 是冻结现场，零观测者效应。
#
# 用法: riscv_halt_dump.sh <tag> [wait_seconds]
set -u
ROOT="/home/xzhao/github/minix-rs/os"
TRIPLE="riscv64gc-unknown-none-elf"
REL="$ROOT/target/$TRIPLE/release"
IMG="$ROOT/target/image/riscv64"
TAG="${1:-hd1}"
WAIT="${2:-90}"
WORK="/home/xzhao/github/minix-rs/tmp/nk4a/a280"
SERIAL="$WORK/serial.$TAG.log"
RAMF="$WORK/ram_$TAG.bin"
MON="$WORK/mon_$TAG.sock"
TABLE_PA=0x85000000
# 续-289 变体矩阵：QEMU_BIN 覆盖（如 docker 内的 9.x 二进制），默认系统 8.2.2。
QEMU_BIN="${QEMU_BIN:-qemu-system-riscv64}"
# 续-295 smp 度对照矩阵：SMP 覆盖（默认 1 = 门同构）。
SMP_N="${SMP_N:-1}"
MODULE_BASE=0x86000000

for f in "$REL/kernel" "$IMG/qemu.dtb" "$IMG/table.bin"; do
    [ -f "$f" ] || { echo "SKIP: 缺 $f（先跑一次 test-riscv64-boot-full.sh）"; exit 2; }
done

# 续-312 修复：mod_*.bin 从 REL 逐个重写（旧字节陷阱——REL 重建后 mod
# bins 不跟手，QEMU 装载旧字节而取证比对用新 ELF=全案错位根源），随后与
# 门同源重算 12 模块 loader 参数。
for _n in ds rs pm sched vfs memory tty mib vm pfs mfs init; do
    case "$_n" in
        memory) _f=minix-driver-memory ;;
        pfs)    _f=minix-fs-pfs ;;
        mfs)    _f=minix-fs-mfs ;;
        *)      _f=minix-$_n ;;
    esac
    cp "$REL/$_f" "$IMG/mod_$_n.bin"
done
# §续-317 同源重生成：从（已刷新的）mod_*.bin 重算 base 序列并重写
# table.bin（MNXBOOT1=MAGIC u64+count u32+pad u32+16×(path[64]+pa u64+len
# u64)）——消除「QEMU loader 用新布局、表用旧布局」的错位（§续-316：
# pfs/mfs/init 镜像非 ELF 的自错位根源）。
LOADER_ARGS=$(ROOT="$ROOT" IMG="$IMG" MOD_BASE="$MODULE_BASE" TABLE_PA="$TABLE_PA" TABLE_BIN="$IMG/table.bin" python3 - <<'PYEOF'
import os, struct
rel = os.path.join(os.environ["ROOT"], "target", "riscv64gc-unknown-none-elf", "release")
img = os.environ["IMG"]; base = int(os.environ["MOD_BASE"], 0); tpa = int(os.environ["TABLE_PA"], 0)
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
        d = f.read()
    entries.append((path_field(name), pa, len(d)))
    loaders.append("-device"); loaders.append("loader,addr=0x%x,file=%s,force-raw=on" % (pa, os.path.join(img, "mod_%s.bin" % name)))
    pa += (len(d) + 0xfff) & ~0xfff
blob = struct.pack("<QII", MAGIC, len(entries), 0)
for path, e_pa, e_len in entries:
    blob += path + struct.pack("<QQ", e_pa, e_len)
blob += bytes((16 - len(entries)) * (64 + 16))
with open(os.environ["TABLE_BIN"], "wb") as g:
    g.write(blob)
print(" ".join(loaders))
PYEOF
)
[ -n "$LOADER_ARGS" ] || { echo "FAIL: loader 参数为空"; exit 1; }

rm -f "$SERIAL" "$RAMF" "$MON"
# shellcheck disable=SC2086
setsid "$QEMU_BIN" \
    -machine virt -smp "$SMP_N" -m 512M -bios default \
    -kernel "$REL/kernel" \
    -dtb "$IMG/qemu.dtb" \
    -device "loader,addr=$TABLE_PA,file=$IMG/table.bin,force-raw=on" \
    $LOADER_ARGS \
    -serial "file:$SERIAL" -display none -no-reboot \
    -monitor "unix:$MON,server,nowait" &
QPID=$!
echo "qemu pid=$QPID tag=$TAG"

hit=0
for _ in $(seq 1 "$WAIT"); do
    if [ -f "$SERIAL" ] && grep -qa "pagefault in VM" "$SERIAL" 2>/dev/null; then hit=1; break; fi
    if [ -f "$SERIAL" ] && grep -qa "minimal boot script marker" "$SERIAL" 2>/dev/null; then hit=2; break; fi
    kill -0 "$QPID" 2>/dev/null || break
    sleep 1
done
echo "halt-detect=$hit (1=crash-halt 2=marker 0=timeout)"

if [ "$hit" = "0" ]; then
    echo "TIMEOUT 形（挂起/不崩）：仍保全场后再杀——挂起现场同具取证价值"
fi

sleep 1
python3 - "$MON" "$RAMF" <<'PYEOF'
import socket, sys, time, os
mon, ramf = sys.argv[1], sys.argv[2]
for _ in range(50):
    if os.path.exists(mon): break
    time.sleep(0.1)
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(mon)
time.sleep(0.3); s.recv(65536)
def cmd(c, wait=0.5):
    s.sendall((c + "\n").encode())
    time.sleep(wait)
    out = b""
    s.settimeout(2.0)
    try:
        while True:
            chunk = s.recv(1 << 20)
            if not chunk: break
            out += chunk
            if len(out) > (1 << 22): break
    except socket.timeout:
        pass
    s.settimeout(None)
    return out.decode(errors="replace")
r1 = cmd("stop", 0.5)
print("stop resp:", r1.replace("\x1b", "ESC")[:120])
r2 = cmd('pmemsave 0x80000000 0x20000000 "%s"' % ramf, 12.0)
print("pmemsave resp:", r2.replace("\x1b", "ESC")[:400])
print("dump size now:", os.path.getsize(ramf) if os.path.exists(ramf) else "absent")
cmd("quit", 0.3)
s.close()
PYEOF
wait "$QPID" 2>/dev/null
[ -f "$RAMF" ] || { echo "FAIL: dump 未落盘"; exit 1; }
echo "ram dump: $(ls -la "$RAMF" | awk '{print $5}') bytes"
[ "$hit" = "0" ] && exit 0

# 分析：走 VM 页表比对 text 页 + 全 RAM 别名普查
python3 - "$SERIAL" "$RAMF" <<'PYEOF'
import re, sys, struct
serial, ramf = sys.argv[1], sys.argv[2]
data = open(ramf, "rb").read()
RAM_BASE = 0x80000000
def rd(pa, n):
    return data[pa - RAM_BASE : pa - RAM_BASE + n] if RAM_BASE <= pa and pa + n <= RAM_BASE + len(data) else b""

m = re.search(rb"satp=0x([0-9a-f]+)", open(serial, "rb").read())
# 注意: 内核 pfvm 打印的 root= 值有移位 bug（trap_dispatch.rs:1984 用
# satp&(MASK<<12) 得 PPN 本值 0x82000，缺 <<12）。此处从 satp 行重算真根。
satp = int(m.group(1), 16) if m else 0x8000000000082000
root = (satp & 0xFFFFFFFFFFF) << 12
print("VM root paddr = 0x%x (satp=0x%x, pfvm 行的 root= 印的是 PPN 非 PA)" % (root, satp))

def walk(va):
    p = root
    for shift in (30, 21, 12):
        idx = (va >> shift) & 511
        pte = struct.unpack("<Q", rd(p + idx * 8, 8))[0]
        if not (pte & 1):
            return None, pte, shift
        p = ((pte >> 10) & 0xFFFFFFFFFFF) << 12
    return p, pte, 0

elf = open("/home/xzhao/github/minix-rs/os/target/riscv64gc-unknown-none-elf/release/minix-vm", "rb").read()
def elf_page(va):
    off = va - 0x11000          # R E 段: vaddr 0x185a4 @ file 0x75a4 → off = va - 0x11000
    pg = off & ~0xFFF
    return elf[pg : pg + 0x1000], va & ~0xFFF

SITES = [0x3ae10, 0x3aae8, 0x3ae3e, 0x3ad7c]
for va in SITES:
    page, pgva = elf_page(va)
    leaf, last_pte, lvl = walk(pgva)
    if leaf is None:
        print("VA 0x%x: walk NotPresent at lvl%s pte=0x%x" % (va, lvl, last_pte))
        continue
    mem = rd(leaf, 0x1000)
    diff = sum(1 for a, b in zip(page, mem) if a != b)
    verdict = "EXACT" if diff == 0 else "DIFF"
    print("VA page 0x%lx -> PA 0x%lx: %s (%d/4096 bytes differ, sepc-site byte=%s)" %
          (pgva, leaf, verdict, diff,
           "ok" if mem[va & 0xFFF : (va & 0xFFF) + 4] == page[va & 0xFFF : (va & 0xFFF) + 4] else "BAD"))
    if diff:
        first = next(i for i in range(0x1000) if page[i] != mem[i])
        print("   first diff @ page+0x%x (va 0x%lx): elf=%s mem=%s" %
              (first, pgva + first, page[first:first+8].hex(), mem[first:first+8].hex()))

# 别名普查：text 页 0x3ae000 的 ELF 内容 16B 签名，数 RAM 里多少页含它
sig_off = (0x3ae000 - 0x11000) + 0x200
sig = elf[sig_off : sig_off + 16]
print("census sig(0x3ae200)=%s" % sig.hex())
hits = []
start = 0
while True:
    i = data.find(sig, start)
    if i < 0: break
    if i % 4096 == (sig_off & 0xFFF):
        hits.append(RAM_BASE + (i & ~0xFFF))
    start = i + 1
print("text-page 0x3ae000 RAM 拷贝数 = %d -> %s" % (len(hits), [hex(h) for h in hits]))
PYEOF
echo "=== serial tail ==="
tail -3 "$SERIAL" | cut -c1-120
