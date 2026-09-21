#!/usr/bin/env bash
# NK4-A forensic probe 3: stop at boot (-S), break on the VM global-allocator
# OOM (handle_alloc_error) for the PageFrames-size request, dump registers,
# backtrace, raw stack, and the HeapArena / PAGE_ALLOC_PTR statics, then kill.
# Usage: run_gdb_oom.sh [size_dec]
set -u
cd "$(dirname "$0")"
SZ="${1:-933880}"
VMELF=/home/xzhao/github/minix-rs/os/target/image/x86_64/staging/EFI/minix/modules/vm
HEAP_ARENA=0x246da0     # _ZN8minix_vm6global10HEAP_ARENA  {base,limit,top}
PAGE_ALLOC_PTR=0x288040 # _ZN8minix_vm6global14PAGE_ALLOC_PTR
HAPE=0x237429           # _ZN5alloc5alloc18handle_alloc_error
QEMU_BIN="qemu-system-x86_64"
if pgrep -f '[q]emu-system-x86' >/dev/null; then
  echo "BUSY: another qemu is running"; exit 2
fi
rm -f probe3_serial.log
setsid "$QEMU_BIN" -m 512M \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=vars.fd \
  -drive file=/home/xzhao/github/minix-rs/os/target/image/x86_64/minix.img,format=raw,media=disk \
  -serial file:probe3_serial.log -display none -no-reboot \
  -S -gdb tcp::1234 < /dev/null > /dev/null 2>&1 &
QPID=$!
sleep 3
gdb -batch -nx \
  -ex "set pagination off" \
  -ex "set confirm off" \
  -ex "file $VMELF" \
  -ex "target remote localhost:1234" \
  -ex "hbreak *$HAPE if \$rdi == $SZ" \
  -ex "continue" \
  -ex "info registers rip rsp rdi rsi rax rbx" \
  -ex "bt 20" \
  -ex "x/48gx \$rsp" \
  -ex "x/4gx $HEAP_ARENA" \
  -ex "x/1gx $PAGE_ALLOC_PTR" \
  2>&1 | tee gdb_probe3.txt
kill "$QPID" 2>/dev/null
pkill -P "$QPID" 2>/dev/null
sleep 1
pgrep -f '[q]emu-system-x86' >/dev/null && { pkill -f '[q]emu-system-x86'; echo "WARN: qemu killed post-probe"; } || true
