#!/usr/bin/env bash
set -u
cd "$(dirname "$0")"
VMELF=/home/xzhao/github/minix-rs/os/target/image/x86_64/staging/EFI/minix/modules/vm
QEMU_BIN="qemu-system-x86_64"
pgrep -f '[q]emu-system-x86' >/dev/null && { echo BUSY; exit 2; }
setsid "$QEMU_BIN" -m 512M \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=vars.fd \
  -drive file=/home/xzhao/github/minix-rs/os/target/image/x86_64/minix.img,format=raw,media=disk \
  -serial file:cond_serial.log -display none -no-reboot \
  -S -gdb tcp::1234 < /dev/null > /dev/null 2>&1 &
QPID=$!
sleep 3
gdb -batch -nx \
  -ex "set pagination off" \
  -ex "file $VMELF" \
  -ex "target remote localhost:1234" \
  -ex "set confirm off" \
  -ex "hbreak *0x22d262 if \$rsi > 0x00007fffffffffff" \
  -ex "continue" \
  -ex "info registers rip rsi rdx rcx rbp rbx rax rdi" \
  -ex "x/40gx \$rsp" \
  2>&1 | tee gdb_cond.txt
kill "$QPID" 2>/dev/null; pkill -P "$QPID" 2>/dev/null; sleep 1
pgrep -f '[q]emu-system-x86' >/dev/null && pkill -f '[q]emu-system-x86' || true
