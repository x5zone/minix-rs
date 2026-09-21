#!/usr/bin/env bash
# NK4-A forensic probe 2: boot stopped (-S), break inside VM's Paging::map at the
# invlpg site, dump args + return chain, then kill qemu.
# Usage: run_gdb_bp.sh <break_addr> [extra gdb cmds]
set -u
cd "$(dirname "$0")"
BP="${1:-*0x22d262}"
VMELF=/home/xzhao/github/minix-rs/os/target/image/x86_64/staging/EFI/minix/modules/vm
QEMU_BIN="qemu-system-x86_64"
if pgrep -f '[q]emu-system-x86' >/dev/null; then
  echo "BUSY: another qemu is running"; exit 2
fi
rm -f probe2_serial.log
setsid "$QEMU_BIN" -m 512M \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=vars.fd \
  -drive file=/home/xzhao/github/minix-rs/os/target/image/x86_64/minix.img,format=raw,media=disk \
  -serial file:probe2_serial.log -display none -no-reboot \
  -S -gdb tcp::1234 < /dev/null > /dev/null 2>&1 &
QPID=$!
sleep 3
gdb -batch -nx \
  -ex "set pagination off" \
  -ex "file $VMELF" \
  -ex "target remote localhost:1234" \
  -ex "set confirm off" \
  -ex "hbreak $BP" \
  -ex "continue" \
  -ex "info registers rip rsp rbp rax rbx rcx rdx rsi rdi r8 r12" \
  -ex "x/6i \$rip-10" \
  -ex "x/60gx \$rsp" \
  2>&1 | tee gdb_probe2.txt
kill "$QPID" 2>/dev/null
pkill -P "$QPID" 2>/dev/null
sleep 1
pgrep -f '[q]emu-system-x86' >/dev/null && { pkill -f '[q]emu-system-x86'; echo "WARN: qemu killed post-probe"; } || true
