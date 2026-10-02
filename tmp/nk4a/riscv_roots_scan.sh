#!/usr/bin/env bash
# NK4-C (A) 多根 KDM 扫描（零扰动、单次 boot、不改码）：把 sas-send 列出的全部 12 个
# 进程页表根，在内核 panic-halt 时用内核 Direct Map 各 dump 512 槽，扫有没有【持久】
# corrupt 中间项（解出 paddr≥RAM 顶 0xA0000000 且 V=1）。
#   命中 ⇒ 坐实某根帧有耐久坏槽（无需插件即可定位 culprit）；
#   全净 ⇒ 坐实 (A) 是 A1 真瞬态（halt 看不到），只剩 QEMU 插件能抓瞬态写者。
# 正常 boot 无 -S（(A)@0x3abec clean 复现，~8s 到 panic），端口 1241 锚定精确杀。
set -u
TAG="${1:-gh103}"
PORT=1241
ROOT=/home/xzhao/github/minix-rs/os
REL="$ROOT/target/riscv64gc-unknown-none-elf/release"
IMG="$ROOT/target/image/riscv64"
LOG=/home/xzhao/github/minix-rs/tmp/nk4a
KDM=0xFFFFFFC040000000
# sas-send/sas-fork 全部 ep 根（gh92f2 实测）
ROOTS="0x84332000 0x82132000 0x82198000 0x8220d000 0x82239000 0x822ac000 0x822dd000 0x8230f000 0x8234c000 0x8241d000 0x82c7f000 0x9dc37000"

LA=$(ROOT="$ROOT" IMG="$IMG" REL="$REL" python3 -c '
import os
rel=os.environ["REL"];img=os.environ["IMG"];base=0x86000000
mods=[("ds","minix-ds"),("rs","minix-rs"),("pm","minix-pm"),("sched","minix-sched"),("vfs","minix-vfs"),("memory","minix-driver-memory"),("tty","minix-driver-tty"),("mib","minix-mib"),("vm","minix-vm"),("pfs","minix-fs-pfs"),("mfs","minix-fs-mfs"),("init","minix-init")]
pa=base;o=[]
for n,b in mods:
    d=open(os.path.join(rel,b),"rb").read(); mf=os.path.join(img,"mod_%s.bin"%n); open(mf,"wb").write(d)
    o+=["-device","loader,addr=0x%x,file=%s,force-raw=on"%(pa,mf)]; pa+=(len(d)+0xfff)&~0xfff
print(" ".join(o))')

cleanup() { pkill -f "[q]emu-system-riscv64.*$PORT" 2>/dev/null || true; sleep 1; }
if ss -ltn 2>/dev/null | grep -q ":$PORT "; then echo "WARN port busy"; cleanup; fi
cleanup
setsid qemu-system-riscv64 -machine virt -smp 1 -m 512M -bios default \
  -kernel "$REL/kernel" -dtb "$IMG/qemu.dtb" \
  -device "loader,addr=0x85000000,file=$IMG/table.bin,force-raw=on" $LA \
  -serial file:$LOG/$TAG.serial -display none -no-reboot -gdb tcp::$PORT </dev/null >/dev/null 2>&1 &
QPID=$!
for i in $(seq 1 150); do grep -aq "pagefault in VM" "$LOG/$TAG.serial" 2>/dev/null && { echo "PANIC ~$((i*2))s"; break; }; sleep 2; done
sleep 3

# 构造 gdb：对每个根的内核 DM 虚地址 x/512xg 整帧 dump
EX=(); for r in $ROOTS; do
  va=$(python3 -c "print(hex($KDM + $r))")
  EX+=( -ex "echo \"\\n===ROOT $r (KDM VA $va)===\\n\"" )
  EX+=( -ex "x/512xg $va" )
done
timeout 200 gdb-multiarch -batch -nx \
  -ex "set pagination off" -ex "file $REL/kernel" -ex "target extended-remote localhost:$PORT" \
  "${EX[@]}" 2>&1 | tee "$LOG/$TAG-gdb.txt"
kill "$QPID" 2>/dev/null; pkill -f "[q]emu-system-riscv64.*$PORT" 2>/dev/null; sleep 1

# 扫 corrupt：VALUE 列非零且解出 paddr≥0xA0000000（越 RAM 顶）
echo "===corrupt-slot scan (nonzero slot whose decoded paddr>=0xA0000000)==="
grep -aoE $'\t0x[0-9a-f]{16}' "$LOG/$TAG-gdb.txt" 2>/dev/null | tr -d '\t' | sort -u \
  | awk '{ v=strtonum($1); if ($1!="0x0000000000000000") { paddr=v; printf "%s\n",$1 } }' \
  | python3 -c '
import sys
n=0
for line in sys.stdin:
    line=line.strip()
    if not line: continue
    val=int(line,16)
    paddr=(val>>10)<<12
    # 合法叶子/表指针解出的 paddr 应 < 0xA0000000(RAM 顶)；越界=corrupt
    if (val & 1) and paddr >= 0xA0000000:
        print(f"CORRUPT slot={line} decoded_paddr={hex(paddr)} flags={hex(val & 0x3ff)}")
        n+=1
print(f"TOTAL corrupt-looking slots across all roots: {n}")
'
echo "===ROOTS-SCAN-DONE tag=$TAG log=$LOG/$TAG-gdb.txt==="
