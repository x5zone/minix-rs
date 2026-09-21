#!/usr/bin/env bash
# NK4-A probe 6: boot with gdbstub (no -S), after the VM spins, dump a wide
# slice of its stack and decode printable ASCII runs.
set -u
cd "$(dirname "$0")"
SECS="${1:-22}"
WORDS="${2:-400}"
MINRUN="${3:-16}"
VMELF=/home/xzhao/github/minix-rs/os/target/image/x86_64/staging/EFI/minix/modules/vm
if pgrep -f '[q]emu-system-x86' >/dev/null; then echo "BUSY"; exit 2; fi
rm -f p6_serial.log
setsid qemu-system-x86_64 -m 512M \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=vars.fd \
  -drive file=/home/xzhao/github/minix-rs/os/target/image/x86_64/minix.img,format=raw,media=disk \
  -serial file:p6_serial.log -display none -no-reboot -gdb tcp::1234 < /dev/null > /dev/null 2>&1 &
QPID=$!
sleep "$SECS"
gdb -batch -nx \
  -ex "set pagination off" \
  -ex "file $VMELF" \
  -ex "target remote localhost:1234" \
  -ex "x/${WORDS}gx \$rsp-0x200" \
  > gdb_p6.txt 2>&1
kill "$QPID" 2>/dev/null; pkill -P "$QPID" 2>/dev/null; sleep 1
pgrep -f '[q]emu-system-x86' >/dev/null && { pkill -f '[q]emu-system-x86'; echo "WARN killed"; } || true
MINRUN="$MINRUN" python3 - <<'EOF'
import re, struct, os
mn = int(os.environ.get('MINRUN', '16'))
txt = open('gdb_p6.txt').read()
pairs = re.findall(r':\s+(0x[0-9a-f]{16})\s+(0x[0-9a-f]{16})', txt)
ws = [int(a, 16) for a, b in pairs] + [int(b, 16) for a, b in pairs]
raw = b''.join(struct.pack('<Q', w) for w in ws)
runs = re.findall(rb'[\x20-\x7e]{%d,}' % mn, raw)
for r in sorted(set(runs), key=lambda x: -len(x))[:25]:
    print(r.decode())
EOF
