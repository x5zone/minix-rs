#!/usr/bin/env bash
set -u
cd "$(dirname "$0")"
QEMU_BIN="qemu-system-x86_64"
pgrep -f '[q]emu-system-x86' >/dev/null && { echo BUSY; exit 2; }
setsid "$QEMU_BIN" -m 512M \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=vars.fd \
  -drive file=/home/xzhao/github/minix-rs/os/target/image/x86_64/minix.img,format=raw,media=disk \
  -serial file:cr4_serial.log -display none -no-reboot \
  -gdb tcp::1234 < /dev/null > /dev/null 2>&1 &
QPID=$!
sleep 12
gdb -batch -nx -ex "target remote localhost:1234" -ex "info registers cr0 cr4" 2>&1
kill "$QPID" 2>/dev/null; pkill -P "$QPID" 2>/dev/null; sleep 1
pgrep -f '[q]emu-system-x86' >/dev/null && pkill -f '[q]emu-system-x86' || true
