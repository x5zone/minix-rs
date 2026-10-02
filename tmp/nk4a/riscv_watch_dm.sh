#!/usr/bin/env bash
# NK4-C (A) 抓写者 · 【控制位判别实验】单后台长跑（plan 步骤4b-1 精化）
#
# 触发原因（§gh92 自动交替首轮负结果）：
#   form2（DM 虚 0x19DC377F8）：clean 复现 (A)@0x3abec/0x10bd28cb2c 但 watch 不命中。
#   form1（裸物理 + Qqemu.PhyMemMode:1）：gdb 15.1 报 "Invalid argument syntax"
#     拒绝该 maintenance packet → 地址多半仍按虚译，且把 (A) 漂到 0x3ac1a → 非干净负例。
#   且串口 in-tree filot 探针实测本 run 子根 ptroot=0x9dc37000（靶身份正确）。
#   ⇒ 无法区分三种因：① QEMU riscv TCG 根本不强制 hw 写 watchpoint
#                    ② awatch 地址被按虚而非物解释（0x19DC377F8 超 512MB 物理→永不匹配）
#                    ③ 目标槽 0x9DC377F8（i2=255）在 boot 期【从不被写】=帧复用未清零的
#                       陈旧数据字（§续-233/234 PT-frame reuse/stale 方向）。
#
# 判别设计（一次 boot，全非 PhyMemMode——沿用 clean 复现形态）：
#   控制位 awatch 0x9DC37FF8 = 子根 i2=511 槽（stack VA 0x7fffffffe000>>30=511，
#     exec 建子表必写它）⇒ 若它命中=watchpoint 能力OK且按物理解释，则靶不命中即③坐实。
#     若它也不命中=①/②（能力或语义问题）。
#   目标   awatch 0x9DC377F8 = 子根 i2=255 坏槽（§续-239 唯一靶）。
#   软断点 break *0x3abec     = walk_read 崩点（证 gdb 能停 + 读 fault 现场）。
# 用法: riscv_watch_ctrl.sh [tag]（默认 gh93）；端口 1241 锚定精确杀。
set -u
TAG="${1:-gh93}"
PORT=1241
ROOT=/home/xzhao/github/minix-rs/os
REL="$ROOT/target/riscv64gc-unknown-none-elf/release"
IMG="$ROOT/target/image/riscv64"
LOG=/home/xzhao/github/minix-rs/tmp/nk4a
CTRL=0x19DC37FF8                 # VM-DM 虚：子根 i2=511 槽（exec 经 DM 必写，能力控制位）
TGT=0x19DC377F8                  # VM-DM 虚：子根 i2=255 坏槽（§续-239 靶的 DM 形）
CRASH=0x3abec                   # walk_read 崩点 U-mode PC
BOOT_TIMEOUT="${BOOT_TIMEOUT:-420}"

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
  -serial file:$LOG/$TAG.serial -display none -no-reboot \
  -S -gdb tcp::$PORT </dev/null >/dev/null 2>&1 &
QPID=$!
sleep 3

# 设 3 点 → 反复 continue 抓前若干个 stop（控制位命中多半在 exec 建表期早于崩点）。
timeout "$BOOT_TIMEOUT" gdb-multiarch -batch -nx \
  -ex "set pagination off" -ex "set confirm off" \
  -ex "set can-use-hw-watchpoints 1" \
  -ex "file $REL/minix-vm" \
  -ex "target extended-remote localhost:$PORT" \
  -ex "awatch *(unsigned long*)$CTRL" \
  -ex "awatch *(unsigned long*)$TGT" \
  -ex "break *$CRASH" \
  -ex "info breakpoints" \
  -ex "continue" \
  -ex "printf \"\\n===STOP1===\\n\"" \
  -ex "x/1i \$pc" -ex "info registers a0 a1 a2 a3 a5 a6 a7 sp ra pc" \
  -ex "printf \"CTRL(0x9DC37FF8)=0x%016lx  TGT(0x9DC377F8)=0x%016lx\\n\", *(unsigned long*)$CTRL, *(unsigned long*)$TGT" \
  -ex "bt" \
  -ex "continue" \
  -ex "printf \"\\n===STOP2===\\n\"" \
  -ex "x/1i \$pc" -ex "info registers a0 a1 a2 a3 a5 a6 a7 sp ra pc" \
  -ex "printf \"CTRL=0x%016lx  TGT=0x%016lx\\n\", *(unsigned long*)$CTRL, *(unsigned long*)$TGT" \
  -ex "bt" \
  -ex "continue" \
  -ex "printf \"\\n===STOP3===\\n\"" \
  -ex "x/1i \$pc" -ex "info registers a0 a1 a2 a3 a5 a6 a7 sp ra pc" \
  -ex "printf \"CTRL=0x%016lx  TGT=0x%016lx\\n\", *(unsigned long*)$CTRL, *(unsigned long*)$TGT" \
  -ex "bt" \
  2>&1 | tee "$LOG/$TAG-gdb.txt"
RC=${PIPESTATUS[0]}
kill "$QPID" 2>/dev/null; pkill -P "$QPID" 2>/dev/null || true
cleanup
echo "===CTRL-DONE tag=$TAG rc=$RC log=$LOG/$TAG-gdb.txt==="
