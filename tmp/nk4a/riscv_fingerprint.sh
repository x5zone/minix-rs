#!/usr/bin/env bash
# NK4-C (A) 同-build 现场捕获：正常 boot 无 -S、零探针，从内核 trap 帧的同一串口行
# 拿到 sepc 与 stval（同源，配对正确），随后对【同一个】 minix-vm ELF 反汇 sepc 那条指令。
# 目的：终结 §续-266 的"跨 build 归属"疑点——sepc 在产生它的那个 build 里到底是不是
# 一次 query 槽读、其地址算式能否解释 stval 的低 12 位。
set -u
TAG="${1:-gh114}"
PORT="${2:-1244}"
ROOT=/home/xzhao/github/minix-rs/os
REL="$ROOT/target/riscv64gc-unknown-none-elf/release"
IMG="$ROOT/target/image/riscv64"
LOG=/home/xzhao/github/minix-rs/tmp/nk4a

LA=$(ROOT="$ROOT" IMG="$IMG" REL="$REL" python3 -c '
import os
rel=os.environ["REL"];img=os.environ["IMG"];base=0x86000000
mods=[("ds","minix-ds"),("rs","minix-rs"),("pm","minix-pm"),("sched","minix-sched"),("vfs","minix-vfs"),("memory","minix-driver-memory"),("tty","minix-driver-tty"),("mib","minix-mib"),("vm","minix-vm"),("pfs","minix-fs-pfs"),("mfs","minix-fs-mfs"),("init","minix-init")]
pa=base;o=[]
for n,b in mods:
    d=open(os.path.join(rel,b),"rb").read(); mf=os.path.join(img,"mod_%s.bin"%n); open(mf,"wb").write(d)
    o+=["-device","loader,addr=0x%x,file=%s,force-raw=on"%(pa,mf)]; pa+=(len(d)+0xfff)&~0xfff
print(" ".join(o))')

cleanup(){ pkill -f "[q]emu-system-riscv64.*$PORT" 2>/dev/null || true; sleep 1; }
if ss -ltn 2>/dev/null | grep -q ":$PORT "; then echo "WARN port busy"; fi
cleanup
# 裸启动（无 -plugin、无 -S）；-gdb 端口留着但主指纹来自 panic-halt 前的串口行
setsid qemu-system-riscv64 -machine virt -smp 1 -m 512M -bios default \
  -kernel "$REL/kernel" -dtb "$IMG/qemu.dtb" \
  -device "loader,addr=0x85000000,file=$IMG/table.bin,force-raw=on" $LA \
  -serial file:$LOG/$TAG.serial -display none -no-reboot -gdb tcp::$PORT </dev/null >/dev/null 2>&1 &
for i in $(seq 1 150); do
  grep -aq "pagefault in VM" "$LOG/$TAG.serial" 2>/dev/null && { echo "PANIC ~$((i*2))s"; break; }
  sleep 2
done
sleep 2
pkill -f "[q]emu-system-riscv64.*$PORT" 2>/dev/null; sleep 1

echo "===同-build 主指纹（sepc/stval/pfvm 同源串口行）==="
grep -aE 'pagefault for VM sepc|pfvm: pa=|pfvm: satp=' "$LOG/$TAG.serial" | head
echo "===CAPTURE-DONE tag=$TAG serial=$LOG/$TAG.serial==="
