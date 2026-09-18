#!/bin/bash
# test-paging-faultloop.sh — E5(d) page-fault complete loop smoke
# (edge1 K17, C-4; assertion items per edge_todo.md E5(d) 2026-09-08).
#
# Boots test-paging-faultloop.efi (OVMF, same staging as test-user-trap.sh).
# The guest kernel adopts the boot root as the self page table, runs the
# map/query/unmap legs, maps CPL3 code/data/stack pages (FAULT_VA left
# unmapped), and hands the CPU to the payload. The payload's first read
# faults (#PF, user, not-present, err=4) into the carrier arm:
#   RTS_PAGEFAULT set → VM_PAGEFAULT message built → stand-in resolve
#   (patterned frame) → hardware PTE write + invlpg → RTS_PAGEFAULT
#   cleared → iretq → the read re-executes successfully.
#
# PASS determination: the script reads two mailboxes through the QEMU
# gdbstub (the payload spins at CPL3 with the live CR3):
#   user mailbox (VA 0x1_0001_0000):
#     [+0x00] == 0x5a5a5a5a5a5a5a5a  loop closed, PTE maps the RIGHT frame
#     [+0x08] == 0x000000000000beef  payload completion marker
#   kernel box (identity VA 0x400_5000):
#     [+0x00] == 1                   fault count — the anti-livelock
#                                    assertion (a re-fault would loop
#                                    forever without the PTE write)
#     [+0x08] == 0x000000000000feed  adopt + map/query/unmap legs OK
# The guest never exits, so the script kills QEMU after the reads.
#
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EFI="${2:-$ROOT/target/x86_64-unknown-uefi/release/test-paging-faultloop.efi}"
TIMEOUT_BOOT="${TIMEOUT_BOOT:-90}"

if ! command -v qemu-system-x86_64 &>/dev/null; then
    echo "SKIP: qemu-system-x86_64 not found"; exit 2
fi

FW=""; FW_SRC_VARS=""
for c in /usr/share/OVMF/OVMF_CODE_4M.fd /usr/share/OVMF/OVMF_CODE.fd; do
    [ -f "$c" ] && FW="$c" && break
done
for c in /usr/share/OVMF/OVMF_VARS_4M.fd /usr/share/OVMF/OVMF_VARS.fd; do
    [ -f "$c" ] && FW_SRC_VARS="$c" && break
done
if [ -z "$FW" ] || [ -z "$FW_SRC_VARS" ]; then
    echo "SKIP: OVMF firmware not found"; exit 2
fi

# Stage the EFI on a FAT disk (test-user-trap.sh pattern).
STAGING="$(mktemp -d /tmp/faultloop_staging.XXXXXX)"
mkdir -p "$STAGING/EFI/BOOT"
DISK_IMG="$(mktemp /tmp/faultloop_disk.XXXXXX.img)"
if command -v mkfs.vfat &>/dev/null && command -v mmd &>/dev/null && command -v mcopy &>/dev/null; then
    dd if=/dev/zero of="$DISK_IMG" bs=1M count=64 status=none 2>/dev/null
    mkfs.vfat "$DISK_IMG" >/dev/null 2>&1
    mmd -i "$DISK_IMG" ::EFI ::EFI/BOOT >/dev/null 2>&1
    mcopy -v -i "$DISK_IMG" "$EFI" "::EFI/BOOT/BOOTX64.EFI" || echo "FAIL: mcopy BOOTX64"
    printf 'echo -off\r\nFS0:\r\ncd EFI\\BOOT\r\nBOOTX64.EFI\r\n' > "$STAGING/startup.nsh"
    mcopy -i "$DISK_IMG" "$STAGING/startup.nsh" "::startup.nsh" || echo "FAIL: mcopy startup.nsh"
    DISK_DRIVE="file=$DISK_IMG,format=raw,media=disk"
else
    DISK_DRIVE="file=fat:rw:$STAGING,format=raw,media=disk"
fi

FW_VARS="$(mktemp /tmp/faultloop_vars.XXXXXX.fd)"
cp "$FW_SRC_VARS" "$FW_VARS"
SERIAL_LOG="$(mktemp /tmp/faultloop_serial.XXXXXX.log)"

qemu-system-x86_64 \
    -machine q35 \
    -net none \
    -smp ${SMP:-1} \
    -m 256M \
    -gdb tcp::1234 \
    -drive "if=pflash,format=raw,unit=0,file=$FW,readonly=on" \
    -drive "if=pflash,format=raw,unit=1,file=$FW_VARS" \
    -drive "$DISK_DRIVE" \
    -serial "file:$SERIAL_LOG" \
    -display none \
    -no-reboot \
    -d int,cpu_reset -D /tmp/faultloop_int.log &
QEMU_PID=$!

cleanup() {
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
    rm -rf "$STAGING" "$DISK_IMG" "$FW_VARS" "$SERIAL_LOG" /tmp/faultloop_int.log
}
trap cleanup EXIT

# Wait for the guest to reach the scheduler hand-off (bounded).
reached=0
for _ in $(seq 1 "$TIMEOUT_BOOT"); do
    if [ -f "$SERIAL_LOG" ] && grep -q "entering scheduler" "$SERIAL_LOG" 2>/dev/null; then
        reached=1
        break
    fi
    if [ -f "$SERIAL_LOG" ] && grep -q "FAIL\|PANIC" "$SERIAL_LOG" 2>/dev/null; then
        echo "FAIL: guest reported failure before the hand-off:"
        cat "$SERIAL_LOG"
        exit 1
    fi
    sleep 1
done
if [ "$reached" -ne 1 ]; then
    echo "FAIL: guest never reached the scheduler hand-off"
    tail -20 "$SERIAL_LOG" 2>/dev/null || true
    exit 1
fi
echo "serial: scheduler hand-off reached — fault loop should close in-handler"

# Give the payload time to fault, get mapped, and store the markers.
sleep 3

# Read both mailboxes via GDB (the payload spins at CPL3 with the live
# CR3: user VA and kernel identity VA are both translatable).
GDB_OUT="$(timeout 20 gdb -batch \
    -ex 'target remote :1234' \
    -ex 'x/2gx 0x100010000' \
    -ex 'x/2gx 0x4005000' \
    "$EFI" 2>/dev/null || true)"
echo "$GDB_OUT" | tail -6

pass=1
echo "$GDB_OUT" | grep -q "0x5a5a5a5a5a5a5a5a" || { pass=0; echo "FAIL: fault-loop read-back != frame pattern (loop did not close or wrong frame)"; }
echo "$GDB_OUT" | grep -q "0x000000000000beef" || { pass=0; echo "FAIL: completion marker != 0xBEEF"; }
echo "$GDB_OUT" | grep -q "0x4005000:.*0x0000000000000001" || { pass=0; echo "FAIL: fault count != 1 (anti-livelock assertion)"; }
echo "$GDB_OUT" | grep -q "0x4005000:.*0x000000000000feed" || { pass=0; echo "FAIL: adopt/map/query/unmap legs not OK"; }

# The serial log must show the full arm sequence, exactly once each.
for marker in "  #PF: va=" "  resolve: frame patterned" "  ack: RTS_PAGEFAULT cleared"; do
    n=$(grep -c "$marker" "$SERIAL_LOG" 2>/dev/null || echo 0)
    if [ "$n" -ne 1 ]; then
        pass=0
        echo "FAIL: serial marker '$marker' seen $n times (want 1)"
    fi
done

if [ "$pass" -eq 1 ]; then
    echo "### TEST_RESULT: PASS test-paging-faultloop ###"
    exit 0
fi
echo "### TEST_RESULT: FAIL test-paging-faultloop ###"
exit 1
