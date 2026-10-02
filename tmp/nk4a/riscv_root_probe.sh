#!/usr/bin/env bash
# NK4-C (A) 定 root：一次性 gdb 条件断点（零生产码改动，不扰动布局）
#
# 据 HEAD 干净 minix-vm 反汇（riscv64-unknown-elf-objdump）：
#   0x3abd2  ld a1,0(a3)   ← walk_read L2 读：a3=root+i2*8, a1=l2e
#   0x3abd4  andi a4,a1,1  ← V 位检查（此处 a3/a1 均新鲜未覆写）
#   0x3abec  ld a1,0(a1)   ← L1 读，崩点（l1=pte_to_paddr(corrupt l2e) 越 RAM）
# 逻辑推断（§续-246）：崩在 :326(L1读) 需 l2e 于 :311 读为 V=1，但 gh96 干净读子根
#   0x9dc37000 i2=255 槽 halt=0 ⇒ 崩 walk 的 root 很可能≠0x9dc37000（A2 stale/错根）。
#   本 harness 在 0x3abd4（刚读 L2 后）设条件断点：仅当 (a1&V) && pte_to_paddr(a1)>=RAM顶
#   才停，拓 a3(=root+i2*8) + a1(=l2e raw) + bt ⇒ 定谳崩 walk 到底用哪个 root 的哪个槽。
# 零改码（纯 gdb），端口 1241 锚定精确杀。用法: riscv_root_probe.sh [tag]（默认 gh100）。
set -u
TAG="${1:-gh100}"
PORT=1241
ROOT=/home/xzhao/github/minix-rs/os
REL="$ROOT/target/riscv64gc-unknown-none-elf/release"
IMG="$ROOT/target/image/riscv64"
LOG=/home/xzhao/github/minix-rs/tmp/nk4a
BOOT_TIMEOUT="${BOOT_TIMEOUT:-440}"

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

# 条件断点：刚读完 L2(a3=root+i2*8) 且该 l2e 解码 paddr>=0xA0000000 且 V=1 → 停，定 root。
timeout "$BOOT_TIMEOUT" gdb-multiarch -batch -nx \
  -ex "set pagination off" -ex "set confirm off" \
  -ex "file $REL/minix-vm" \
  -ex "target extended-remote localhost:$PORT" \
  -ex "break *0x3abd4 if ( \$a1 & 1 ) && ( ((\$a1 >> 10) << 12) >= 0xa0000000 )" \
  -ex "info breakpoints" \
  -ex "continue" \
  -ex "printf \"\\n===HIT (L2 corrupt read at 0x3abd4): a3=slotDMVA(root+i2*8) a1=l2e===\\n\"" \
  -ex "x/1i \$pc" \
  -ex "printf \"a3(slotDMVA)=%#018lx  a1(l2e)=%#018lx\\n\", \$a3, \$a1" \
  -ex "printf \"slot_phys(=root+i2*8)= a3-0x1000000000 =\\n\"" \
  -ex "p/x \$a3 - 0x1000000000" \
  -ex "printf \"l2e decoded paddr=(a1>>10)<<12=\\n\"" \
  -ex "p/x ((\$a1 >> 10) << 12)" \
  -ex "printf \"l2e V/leaf bits=(a1&0x3ff)=\\n\"" \
  -ex "p/x (\$a1 & 0x3ff)" \
  -ex "bt" \
  -ex "printf \"\\n===resume (2nd corrupt-l2e hit if any)===\\n\"" \
  -ex "continue" \
  -ex "printf \"\\n===HIT2===\\n\"" \
  -ex "x/1i \$pc" \
  -ex "printf \"a3=%#018lx a1=%#018lx\\n\", \$a3, \$a1" \
  -ex "p/x \$a3 - 0x1000000000" \
  -ex "bt" \
  2>&1 | tee "$LOG/$TAG-gdb.txt"
RC=${PIPESTATUS[0]}
kill "$QPID" 2>/dev/null; pkill -P "$QPID" 2>/dev/null || true
cleanup
echo "===ROOT-PROBE-DONE tag=$TAG rc=$RC log=$LOG/$TAG-gdb.txt==="
