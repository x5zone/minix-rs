#!/usr/bin/env bash
# NK4-C (A) 现场活体读【内核 DM 形】——硬件写 watchpoint 死路(§续-242)后的决定性地证。
#
# 思路：崩点在 VM U 态访问不到物理帧(Cannot access, §续-241)，但 panic 后内核停在
#   halt 回路时 satp=内核、内核 Direct Map 生效 ⇒ 可经 `KERNEL_DIRECT_MAP_BASE(
#   0xFFFF_FFC0_4000_0000)+phys` 活体读子根 0x9dc37000 整帧 512 槽。
#   ⇒ 直接看：坏槽 i2=255(0x…7F8) 到底装什么值、i2=511(0…FF8) 是否合法 PTE、整帧有多少
#   非零槽——判别「alloc 清零后仅 255 槽被旧数据写回(UAF 别名)」vs「更广破坏」。
#
# 配方（§续-228 晚 attach，正常 boot 无 -S 保 §续-238 clean 时序；(A)@0x3abec 稳复现）：
#   qemu -gdb tcp::1241(不加 -S，直接跑) → 轮询串口出 "pagefault in VM — halting" → gdb
#   attach 瞬停 halt 回路 → file $REL/kernel(内核符号) → 读内核 DM 各槽。
# 端口锚定精确杀。用法: riscv_live_read.sh [tag]（默认 gh95）。
set -u
TAG="${1:-gh95}"
PORT=1241
ROOT=/home/xzhao/github/minix-rs/os
REL="$ROOT/target/riscv64gc-unknown-none-elf/release"
IMG="$ROOT/target/image/riscv64"
LOG=/home/xzhao/github/minix-rs/tmp/nk4a
KDM=0xFFFFFFC040000000           # riscv64 KERNEL_DIRECT_MAP_BASE = 0xFFFF_FFC0_4000_0000
CHILD_ROOT="${CHILD_ROOT:-0x9dc37000}"  # 子进程页表根物理(可 env 覆盖：RS根=0x82132000)

cleanup() { pkill -f "[q]emu-system-riscv64.*$PORT" 2>/dev/null || true; sleep 1; }
if ss -ltn 2>/dev/null | grep -q ":$PORT "; then echo "WARN port busy"; cleanup; fi
cleanup

LA=$(ROOT="$ROOT" IMG="$IMG" REL="$REL" python3 -c '
import os
rel=os.environ["REL"];img=os.environ["IMG"];base=0x86000000
mods=[("ds","minix-ds"),("rs","minix-rs"),("pm","minix-pm"),("sched","minix-sched"),("vfs","minix-vfs"),("memory","minix-driver-memory"),("tty","minix-driver-tty"),("mib","minix-mib"),("vm","minix-vm"),("pfs","minix-fs-pfs"),("mfs","minix-fs-mfs"),("init","minix-init")]
pa=base;o=[]
for n,b in mods:
    d=open(os.path.join(rel,b),"rb").read(); mf=os.path.join(img,"mod_%s.bin"%n); open(mf,"wb").write(d)
    o+=["-device","loader,addr=0x%x,file=%s,force-raw=on"%(pa,mf)]; pa+=(len(d)+0xfff)&~0xfff
print(" ".join(o))')

setsid qemu-system-riscv64 -machine virt -smp 1 -m 512M -bios default \
  -kernel "$REL/kernel" -dtb "$IMG/qemu.dtb" \
  -device "loader,addr=0x85000000,file=$IMG/table.bin,force-raw=on" $LA \
  -serial file:$LOG/$TAG.serial -display none -no-reboot \
  -gdb tcp::$PORT </dev/null >/dev/null 2>&1 &
QPID=$!
echo "===LIVE-BEGIN tag=$TAG — booting, waiting for (A) panic/halt==="

# 轮询串口直至 (A) 打 "pagefault in VM — halting"（或 300s 上限）
for i in $(seq 1 150); do
  if grep -aq "pagefault in VM" "$LOG/$TAG.serial" 2>/dev/null; then echo "PANIC seen after ~$((i*2))s"; break; fi
  sleep 2
done
# halt 回路再给一点时间稳定，然后 attach
sleep 3

ROOTVA=$(python3 -c "print(hex($KDM + $CHILD_ROOT))")          # 子根内核DM虚
SLOT255=$(python3 -c "print(hex($KDM + $CHILD_ROOT + 255*8))") # i2=255 坏槽
SLOT511=$(python3 -c "print(hex($KDM + $CHILD_ROOT + 511*8))") # i2=511 控制槽
echo "===LIVE addrs: root=$ROOTVA slot255=$SLOT255 slot511=$SLOT511==="

timeout 120 gdb-multiarch -batch -nx \
  -ex "set pagination off" \
  -ex "file $REL/kernel" \
  -ex "target extended-remote localhost:$PORT" \
  -ex "printf \"===KDM slot255(i2=255 corrupt)===\\n\"" \
  -ex "x/1xg $SLOT255" \
  -ex "printf \"===KDM slot511(i2=511 control)===\\n\"" \
  -ex "x/1xg $SLOT511" \
  -ex "printf \"===decoded slot255: ppa=(v>>10)<<12 ; flags=v&0x3ff ; 8-align? off=v&0xfff===\\n\"" \
  -ex "p/x ((*(unsigned long*)$SLOT255) >> 10) << 12" \
  -ex "p/x (*(unsigned long*)$SLOT255) & 0x3ff" \
  -ex "printf \"===whole child root 512 slots (count nonzero via info)===\\n\"" \
  -ex "x/512xg $ROOTVA" \
  2>&1 | tee "$LOG/$TAG-gdb.txt"
RC=$?
kill "$QPID" 2>/dev/null; pkill -P "$QPID" 2>/dev/null || true
cleanup
# 非零槽统计（P1 修：CodeReview 97a55b8——旧口径跨全文匹配把独立 x/1xg 探针(slot511)
# 与 512-dump 各计一次致虚高 1。现限定到 512-dump 区间，与整帧语义对齐。）
echo "===LIVE nonzero-slot tally in child root (仅 512-dump 区间 VALUE 列非全零)==="
awk '/whole child root 512 slots/{f=1;next} f' "$LOG/$TAG-gdb.txt" 2>/dev/null \
  | grep -aoE $'\t0x[0-9a-f]{16}' | tr -d '\t' \
  | awk '$1!="0x0000000000000000"{c++} END{print c+0" nonzero slots"}'
echo "===distinct nonzero values in 512-dump (up to 24)==="
awk '/whole child root 512 slots/{f=1;next} f' "$LOG/$TAG-gdb.txt" 2>/dev/null \
  | grep -aoE $'\t0x[0-9a-f]{16}' | tr -d '\t' \
  | grep -v "^0x0000000000000000$" | sort | uniq -c | sort -rn | head -24
echo "===LIVE-DONE tag=$TAG rc=$RC log=$LOG/$TAG-gdb.txt==="
