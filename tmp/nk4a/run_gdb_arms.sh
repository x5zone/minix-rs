#!/usr/bin/env bash
# NK4-A probe 5: break on HeapArena::grow's outcome arms (cond: pages==229)
# to identify which failure the oversize grow hits.
set -u
cd "$(dirname "$0")"
VMELF=/home/xzhao/github/minix-rs/os/target/image/x86_64/staging/EFI/minix/modules/vm
QEMU_BIN="qemu-system-x86_64"
if pgrep -f '[q]emu-system-x86' >/dev/null; then echo "BUSY"; exit 2; fi
rm -f probe5_serial.log
setsid "$QEMU_BIN" -m 512M \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=vars.fd \
  -drive file=/home/xzhao/github/minix-rs/os/target/image/x86_64/minix.img,format=raw,media=disk \
  -serial file:probe5_serial.log -display none -no-reboot -S -gdb tcp::1234 < /dev/null > /dev/null 2>&1 &
QPID=$!
sleep 3
gdb -batch -nx \
  -ex "set pagination off" -ex "set confirm off" \
  -ex "file $VMELF" -ex "target remote localhost:1234" \
  -ex "hbreak *0x21f2ea if \$r14 >= 200" \
  -ex "hbreak *0x21f31d if \$r14 >= 200" \
  -ex "hbreak *0x21f2d9 if \$r14 >= 200" \
  -ex "hbreak *0x21f35b if \$r14 >= 200" \
  -ex "hbreak *0x21f336 if \$r14 >= 200" \
  -ex "continue" \
  -ex "printf \"== arm rip ==\n\"" \
  -ex "info registers rip r14 rbx r12 rbp rax rdx" \
    -ex "x/32gx \$r12" -ex "dump binary memory /tmp/vmbitmap.bin 0x80001000 (0x80001000+14592)" \
  2>&1 | tee gdb_probe5.txt
kill "$QPID" 2>/dev/null; pkill -P "$QPID" 2>/dev/null; sleep 1
pgrep -f '[q]emu-system-x86' >/dev/null && pkill -f '[q]emu-system-x86' || true
