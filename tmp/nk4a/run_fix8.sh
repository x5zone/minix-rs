#!/usr/bin/env bash
# NK4-A smoke runner: boot full minix.img, capture serial, kill after NUBASE wait.
# Usage: run_fix8.sh <logname> [seconds]
set -u
cd "$(dirname "$0")"
LOG="${1:-serial_fix8.log}"
SECS="${2:-30}"
INTLOG="${3:-}"     # optional: -d int,cpu_reset exception trace
QEMU_BIN="qemu-system-x86_64"   # script file itself avoids cmdline self-match
if pgrep -f '[q]emu-system-x86' >/dev/null; then
  echo "BUSY: another qemu is running"; exit 2
fi
rm -f "$LOG"
EXTRA=()
if [ -n "$INTLOG" ]; then
  rm -f "$INTLOG"
  EXTRA=(-d int,cpu_reset -D "$INTLOG")
fi
setsid "$QEMU_BIN" -m 512M \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=vars.fd \
  -drive file=/home/xzhao/github/minix-rs/os/target/image/x86_64/minix.img,format=raw,media=disk \
  -serial "file:$LOG" -display none -no-reboot "${EXTRA[@]}" < /dev/null > /dev/null 2>&1 &
QPID=$!
sleep "$SECS"
pkill -P "$QPID" 2>/dev/null
kill "$QPID" 2>/dev/null
sleep 1
pgrep -f '[q]emu-system-x86' >/dev/null && echo "WARN: qemu still alive" || true
tail -50 "$LOG"
