#!/usr/bin/env bash
# NK4-A fix20 probe: breakpoint dispatch_exec outcome arms in kernel.elf
# (EINVAL@284 / EFAULT@334 / VmSuspend@335 / OK@380), print target endpt.
# Usage: run_gdb_exec.sh [wait_secs]
set -u
cd "$(dirname "$0")"
SECS="${1:-45}"
KELF=/home/xzhao/github/minix-rs/os/target/image/x86_64/staging/EFI/minix/kernel.elf
QEMU_BIN="qemu-system-x86_64"
if pgrep -f '[q]emu-system-x86' >/dev/null; then
  echo "BUSY: another qemu is running"; exit 2
fi
rm -f exec_serial.log
setsid "$QEMU_BIN" -m 512M \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=vars.fd \
  -drive file=/home/xzhao/github/minix-rs/os/target/image/x86_64/minix.img,format=raw,media=disk \
  -serial file:exec_serial.log -display none -no-reboot \
  -gdb tcp::1234 < /dev/null > /dev/null 2>&1 &
QPID=$!
sleep 12
cat > gdb_exec.txt <<'EOF'
set pagination off
set confirm off
EOF
cat >> gdb_exec.txt <<EOF
file $KELF
target remote localhost:1234
break syscall_process.rs:284
commands
printf "HIT-EINVAL-endpt=%d\\n", endpt
continue
end
break syscall_process.rs:334
commands
printf "HIT-EFAULT-nameptr=%lu\\n", name_ptr
continue
end
break syscall_process.rs:335
commands
printf "HIT-SUSPEND\\n"
continue
end
break syscall_process.rs:380
commands
printf "HIT-OK\\n"
continue
end
continue
EOF
timeout $((SECS)) gdb -batch -nx -x gdb_exec.txt 2>&1 | tee gdb_exec_out.txt
kill "$QPID" 2>/dev/null
pkill -P "$QPID" 2>/dev/null
sleep 1
pgrep -f '[q]emu-system-x86' >/dev/null && { pkill -f '[q]emu-system-x86'; echo "WARN: qemu killed post-probe"; } || true
echo "=== serial tail ==="
tail -8 exec_serial.log
