#!/usr/bin/env bash
# (A) 插件取证：正常 boot 无 -S + QEMU mem 插件观察 PTE 帧读写（零暂停、不改 guest 码）。
set -u
TAG="${1:-gh105}"; PORT_UNUSED=1242
ROOT=/home/xzhao/github/minix-rs/os
REL="$ROOT/target/riscv64gc-unknown-none-elf/release"
IMG="$ROOT/target/image/riscv64"
LOG=/home/xzhao/github/minix-rs/tmp/nk4a
SO="$LOG/watch_store_plugin.so"
TARGETS="${TARGETS:-0x9DC37000,0x9d2ad000,0x9d2ac000}"
LA=$(ROOT="$ROOT" IMG="$IMG" REL="$REL" python3 -c '
import os
rel=os.environ["REL"];img=os.environ["IMG"];base=0x86000000
mods=[("ds","minix-ds"),("rs","minix-rs"),("pm","minix-pm"),("sched","minix-sched"),("vfs","minix-vfs"),("memory","minix-driver-memory"),("tty","minix-driver-tty"),("mib","minix-mib"),("vm","minix-vm"),("pfs","minix-fs-pfs"),("mfs","minix-fs-mfs"),("init","minix-init")]
pa=base;o=[]
for n,b in mods:
    d=open(os.path.join(rel,b),"rb").read(); mf=os.path.join(img,"mod_%s.bin"%n); open(mf,"wb").write(d)
    o+=["-device","loader,addr=0x%x,file=%s,force-raw=on"%(pa,mf)]; pa+=(len(d)+0xfff)&~0xfff
print(" ".join(o))')
cleanup(){ pkill -f "[q]emu-system-riscv64" 2>/dev/null; }
cleanup; sleep 1
setsid qemu-system-riscv64 -machine virt -smp 1 -m 512M -bios default \
  -kernel "$REL/kernel" -dtb "$IMG/qemu.dtb" \
  -device "loader,addr=0x85000000,file=$IMG/table.bin,force-raw=on" $LA \
  -plugin "file=$SO" \
  -serial file:$LOG/$TAG.serial -display none -no-reboot \
  >$LOG/$TAG-plugin.out 2>&1 &
QPID=$!
echo "plugin-boot pid=$QPID targets=$TARGETS"
for i in $(seq 1 90); do grep -aq "pagefault in VM" $LOG/$TAG.serial 2>/dev/null && { echo "PANIC ~$((i*2))s"; break; }; sleep 2; done
sleep 2; kill $QPID 2>/dev/null; cleanup
echo "===plugin armed line==="; grep -aE "watch_store_plugin: armed" $LOG/$TAG-plugin.out 2>/dev/null
echo "===PLG event count==="; grep -acE "^PLG " $LOG/$TAG-plugin.out 2>/dev/null
echo "===writes to targets (W)==="; grep -aE "^PLG .* W " $LOG/$TAG-plugin.out 2>/dev/null | head -25
echo "===events with paddr near bogus leaf or slot 255/511==="; grep -aE "paddr=0x[0-9a-f]*bd28c|slot=(255|511) " $LOG/$TAG-plugin.out 2>/dev/null | head -25
