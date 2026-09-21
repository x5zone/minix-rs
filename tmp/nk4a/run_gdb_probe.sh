#!/usr/bin/env bash
# NK4-A forensic probe: boot with gdbstub, let VM reach the spin, attach and
# dump the VM's user stack backtrace, then kill qemu.
# Usage: run_gdb_probe.sh [wait_secs]
set -u
cd "$(dirname "$0")"
SECS="${1:-25}"
VMELF=/home/xzhao/github/minix-rs/os/target/image/x86_64/staging/EFI/minix/modules/vm
QEMU_BIN="qemu-system-x86_64"
if pgrep -f '[q]emu-system-x86' >/dev/null; then
  echo "BUSY: another qemu is running"; exit 2
fi
rm -f probe_serial.log
setsid "$QEMU_BIN" -m 512M \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=vars.fd \
  -drive file=/home/xzhao/github/minix-rs/os/target/image/x86_64/minix.img,format=raw,media=disk \
  -serial file:probe_serial.log -display none -no-reboot \
  -gdb tcp::1234 < /dev/null > /dev/null 2>&1 &
QPID=$!
sleep "$SECS"
gdb -batch -nx \
  -ex "set pagination off" \
  -ex "file $VMELF" \
  -ex "target remote localhost:1234" \
  -ex "info registers rip rsp rbp rbx rdi rsi rdx rax" \
  -ex "x/8i \$rip" \
  -ex "x/80gx \$rsp" \
  2>&1 | tee gdb_probe.txt
# resume then kill the qemu session
kill "$QPID" 2>/dev/null
pkill -P "$QPID" 2>/dev/null
sleep 1
pgrep -f '[q]emu-system-x86' >/dev/null && { pkill -f '[q]emu-system-x86'; echo "WARN: qemu killed post-probe"; } || true
tail -5 probe_serial.log
