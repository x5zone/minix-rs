#!/usr/bin/env bash
# NK4-A forensic probe 4: stop at boot (-S), break on HeapArena::grow entry
# for every call, print (pages, page_alloc ptr), finish through the call and
# dump the Result registers / sret buffer, then kill qemu.
set -u
cd "$(dirname "$0")"
VMELF=/home/xzhao/github/minix-rs/os/target/image/x86_64/staging/EFI/minix/modules/vm
GROW=0x21f0c0   # _ZN8minix_vm10heap_arena9HeapArena4grow
QEMU_BIN="qemu-system-x86_64"
if pgrep -f '[q]emu-system-x86' >/dev/null; then
  echo "BUSY: another qemu is running"; exit 2
fi
rm -f probe4_serial.log
setsid "$QEMU_BIN" -m 512M \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=vars.fd \
  -drive file=/home/xzhao/github/minix-rs/os/target/image/x86_64/minix.img,format=raw,media=disk \
  -serial file:probe4_serial.log -display none -no-reboot \
  -S -gdb tcp::1234 < /dev/null > /dev/null 2>&1 &
QPID=$!
sleep 3
CMDS=()
for i in 1 2 3 4 5 6; do
  CMDS+=(-ex "printf \"== grow hit $i ==\n\"")
  CMDS+=(-ex "info registers rip rdi rsi rdx")
  CMDS+=(-ex "finish")
  CMDS+=(-ex "info registers rax rdx")
  CMDS+=(-ex "printf \"-- rdi buffer --\n\"")
  CMDS+=(-ex "x/6gx \$rdi")
  CMDS+=(-ex "continue")
done
gdb -batch -nx \
  -ex "set pagination off" \
  -ex "set confirm off" \
  -ex "file $VMELF" \
  -ex "target remote localhost:1234" \
  -ex "hbreak *$GROW" \
  "${CMDS[@]}" \
  2>&1 | tee gdb_probe4.txt | grep -A3 -E "grow hit|-- rdi|registers" | head -100
kill "$QPID" 2>/dev/null
pkill -P "$QPID" 2>/dev/null
sleep 1
pgrep -f '[q]emu-system-x86' >/dev/null && { pkill -f '[q]emu-system-x86'; echo "WARN: qemu killed post-probe"; } || true
