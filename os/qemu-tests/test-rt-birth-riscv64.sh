#!/bin/bash
# test-rt-birth-riscv64.sh — first minix-rt user binary on riscv64
# (edge1 K12b: the riscv64 leg of the three-arch CPL3 birth chain).
#
# Builds the rt-birth user image for riscv64gc-unknown-none-elf, embeds it
# into the carrier kernel (RT_BIRTH_ELF_PATH), and boots under QEMU virt
# with OpenSBI (-bios default, -kernel). The carrier drives the production
# boot phases (arch_boot_impl Sv39 → init_protection → init_kerninfo →
# init_proc_and_boot's load_vm_elf → switch_to_user) and supplies the
# U-ecall trap leg (MINIX_KERNINFO query + SYS_DIAGCTL console).
#
# PASS: the serial log carries the birth chain's markers, every one of
# them user-CPL3 output that crossed the ecall boundary:
#   "rt-birth argv="               descriptor parse (crt0 birth chain)
#   "rt-birth kerninfo=ready"      real MINIX_KERNINFO trap query
#   "rt-birth user_sp="            kuserinfo dereference on the shared page
#   "RT-BIRTH MAIN OK"             whole chain alive
#   "rt-birth panic render check"  user panic hook render via SYS_DIAGCTL
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OS_ROOT="$ROOT"
TIMEOUT_RUN="${TIMEOUT_RUN:-60}"

if ! command -v qemu-system-riscv64 &>/dev/null; then
    echo "SKIP: qemu-system-riscv64 not found"; exit 2
fi

echo "building rt-birth (riscv64gc-unknown-none-elf)…"
( cd "$OS_ROOT" && cargo build -p rt-birth --target riscv64gc-unknown-none-elf --release ) || {
    echo "FAIL: rt-birth build failed"; exit 1
}
RT_BIRTH_ELF_PATH="$OS_ROOT/target/riscv64gc-unknown-none-elf/release/rt-birth"

echo "building test-rt-birth-riscv64 carrier (embeds $RT_BIRTH_ELF_PATH)…"
( cd "$OS_ROOT" && RT_BIRTH_ELF_PATH="$RT_BIRTH_ELF_PATH" \
    cargo build -p test-rt-birth-riscv64 --target riscv64gc-unknown-none-elf --release ) || {
    echo "FAIL: test-rt-birth-riscv64 build failed"; exit 1
}
KERNEL="$OS_ROOT/target/riscv64gc-unknown-none-elf/release/test-rt-birth-riscv64"

SERIAL_LOG="$(mktemp /tmp/rt_birth_rv_serial.XXXXXX.log)"

qemu-system-riscv64 \
    -machine virt \
    -smp 1 \
    -m 256M \
    -nographic \
    -bios default \
    -kernel "$KERNEL" \
    -serial file:"$SERIAL_LOG" \
    -display none \
    -no-reboot &
QEMU_PID=$!

cleanup() {
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
    rm -f "$SERIAL_LOG"
}
trap cleanup EXIT

# The birth chain spins after its forced panic (the handler's contract);
# wait for the last marker, then kill QEMU and judge the log.
seen=0
for _ in $(seq 1 "$TIMEOUT_RUN"); do
    if grep -qa "rt-birth panic render check" "$SERIAL_LOG" 2>/dev/null; then
        seen=1
        break
    fi
    sleep 1
done
if [ "$seen" -ne 1 ]; then
    echo "### TEST_RESULT: FAIL test-rt-birth-riscv64 ###"
    tail -8 "$SERIAL_LOG" 2>/dev/null
    exit 1
fi

pass=1
grep -qa "rt-birth argv="              "$SERIAL_LOG" || { pass=0; echo "FAIL: descriptor parse marker missing"; }
grep -qa "rt-birth kerninfo=ready"     "$SERIAL_LOG" || { pass=0; echo "FAIL: kerninfo trap query did not report ready"; }
grep -qa "rt-birth user_sp="           "$SERIAL_LOG" || { pass=0; echo "FAIL: kuserinfo dereference marker missing"; }
grep -qa "RT-BIRTH MAIN OK"            "$SERIAL_LOG" || { pass=0; echo "FAIL: birth chain did not reach main"; }
grep -qa "rt-birth panic render check" "$SERIAL_LOG" || { pass=0; echo "FAIL: panic render marker missing"; }

if [ "$pass" -eq 1 ]; then
    echo "### TEST_RESULT: PASS test-rt-birth-riscv64 ###"
    exit 0
fi
echo "### TEST_RESULT: FAIL test-rt-birth-riscv64 ###"
exit 1
