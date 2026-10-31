#!/usr/bin/env bash
# test-riscv64-uboot-shim.sh — D-73: the riscv64 U-Boot → boot-shim →
# kernel-uboot full chain gate.
#
# Chain under test (D-73 frozen decision: "U-Boot is the loader, OpenSBI is
# the S-mode runtime, the RISC-V U-Boot BootShim converts U-Boot's resources
# into Minix-RS unified boot semantics"):
#
#   U-Boot (S-mode under OpenSBI, `-bios default -kernel uboot.elf`)
#     └─ boot.scr: fatload DTB / kernel-uboot / 12 modules / shim
#     └─ bootelf -p <shim staging> <dtb addr>   (argv channel,
#        probe-verified 2026-10-31: extra args reach the entry as
#        (extra_argc, extra_argv); autostart=yes required)
#         └─ riscv64-uboot-shim (physically linked, VMA=LMA=0x84500000)
#              └─ MNXBOOT1 table built in .bss → dtb/kernel/12 modules
#              └─ BootHandoffV1 arena → fence.i; jr → receiver _start
#                   └─ kernel-uboot (handoff v1 receiver, no bootface face)
#                        └─ entering scheduler
#
# Memory layout (all regions inside the bdinfo/lmb free band
# 0x80060000-0x8dea9fff for 256 MiB QEMU virt; the gate uses 512 MiB so
# the band is even wider). The staging addresses mirror
# boot-shim/src/uboot_main.rs `uboot_layout` — the single source; a drift
# fails the chain loudly (the shim reads the wrong file):
#   0x80200000-0x80600000  kernel-uboot PT_LOAD target (riscv64.ld LMA)
#   0x82000000-0x84000000  boot-shim bump/module region (existing constants)
#   0x84500000-0x8463d450  shim image (linked)
#   0x86000000             kernel-uboot ELF file staging
#   0x87000000+i*1MiB      module file staging (12 files)
#   0x8c000000             DTB staging (also the bootelf argv[0] value)
#
# Prerequisites: qemu-system-riscv64, u-boot-qemu (smode uboot.elf),
# u-boot-tools (mkimage), dosfstools, mtools, dtc-free DTB dump
# (QEMU -machine dumpdtb).
#
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs/os
TRIPLE="riscv64gc-unknown-none-elf"
REL="$ROOT/target/$TRIPLE/release"
TIMEOUT_SECS="${TIMEOUT_SECS:-240}"
KERNEL_MARKER="### minix-rs kernel-image: resuming high-half boot (handoff v1 arena, riscv64)"
SCHED_MARKER="entering scheduler"

skip() { echo "SKIP: $1"; exit 2; }
fail() { echo "FAIL: $1"; exit 1; }

command -v qemu-system-riscv64 &>/dev/null || skip "qemu-system-riscv64 not found"
command -v mkimage &>/dev/null || skip "mkimage not found (apt install u-boot-tools)"
command -v mkfs.vfat &>/dev/null || skip "mkfs.vfat not found (dosfstools)"
command -v mdir &>/dev/null || skip "mdir/mcopy not found (mtools)"

UBOOT=""
for c in /usr/lib/u-boot/qemu-riscv64_smode/uboot.elf \
         /usr/lib/u-boot/qemu-riscv64/uboot.elf; do
    [ -f "$c" ] && UBOOT="$c" && break
done
[ -n "$UBOOT" ] || skip "U-Boot riscv64 smode firmware not found (apt install u-boot-qemu)"

MODULES=(ds rs pm sched vfs memory tty mib vm pfs mfs init)
[ "${#MODULES[@]}" -eq 12 ] || fail "module list must be 12 (MODULE_NAMES contract)"

WORK="$(mktemp -d /tmp/riscv_uboot_shim_XXXXXX)"
archive() {
    local d="/tmp/d73-gate/$(date +%Y%m%d-%H%M%S)-$$"
    mkdir -p "$d" 2>/dev/null || return 0
    for f in serial.log build.log; do
        [ -f "$WORK/$f" ] && cp -p "$WORK/$f" "$d/" 2>/dev/null
    done
    echo "evidence archived: $d" >&2
}
trap 'archive; rm -rf "$WORK"' EXIT

# ── Stage 1: build the four products ─────────────────────────────────
echo "== Stage 1: build products =="
{
    # set -e makes the block's exit status the first failing command's — the
    # trailing `set +x` must not mask it (`|| fail` below).
    set -e -x
    ( cd "$ROOT" && cargo build -q --release --target "$TRIPLE" -p boot-shim \
        --bin riscv64-uboot-shim --no-default-features --features fw-riscv-uboot )
    # The receiver product: RUSTFLAGS disable F/D (SD-23, the same injection
    # xtask applies to every riscv64 kernel-image build).
    ( cd "$ROOT" && RUSTFLAGS="-C target-feature=-f,-d" \
        cargo build -q --release --target "$TRIPLE" -p kernel-image \
        --features fw-riscv64-uboot )
    # The 12 boot modules (guest user-land target).
    for p in minix-ds minix-rs minix-pm minix-sched minix-vfs \
             minix-driver-memory minix-driver-tty minix-mib minix-vm \
             minix-fs-pfs minix-fs-mfs minix-init; do
        ( cd "$ROOT" && cargo build -q --release --target "$TRIPLE" -p "$p" )
    done
    set +x
} > "$WORK/build.log" 2>&1 || fail "build failed (see $WORK/build.log)"

SHIM_ELF="$REL/riscv64-uboot-shim"
KERNEL_ELF="$REL/kernel"
[ -f "$SHIM_ELF" ] || fail "shim ELF missing: $SHIM_ELF"
[ -f "$KERNEL_ELF" ] || fail "kernel-uboot ELF missing: $KERNEL_ELF"
for m in "${MODULES[@]}"; do
    # artifact name: minix-<name>, except the drivers (minix-driver-*) and
    # the two FS servers (minix-fs-*)
    case "$m" in
        memory|tty) art="minix-driver-$m" ;;
        pfs|mfs) art="minix-fs-$m" ;;
        *) art="minix-$m" ;;
    esac
    [ -f "$REL/$art" ] || fail "module missing: $REL/$art"
done

# ── Stage 2: DTB dump (QEMU virt, 512 MiB — matches the gate's -m) ───
echo "== Stage 2: DTB dump =="
( cd "$WORK" && timeout 10 qemu-system-riscv64 -machine virt -m 512M \
    -bios none -machine dumpdtb="$WORK/qemu.dtb" ) >/dev/null 2>&1
[ -s "$WORK/qemu.dtb" ] || fail "DTB dump failed"

# ── Stage 3: staging layout (mirror of uboot_layout — see header) ─────
# The MNXBOOT1 table is NOT staged: the shim builds it in .bss from these
# same constants (build_shim_table), so only the files are fatloaded.
echo "== Stage 3: staging layout (uboot_layout mirror) =="
DTB_ADDR=8c000000              # bootelf argv[0]: plain hex, no 0x prefix

# ── Stage 4: FAT disk (MBR partition, T-10 shape) ─────────────────────
echo "== Stage 4: FAT disk =="
# 32 MiB: the 12 modules must fit alongside the mfs image, which embeds the
# imgrd (built by the direct leg's mkfs stage — ~1 MiB). 16 MiB filled up.
DISK_MB=32
PART_SECTORS=$(( DISK_MB * 1024 * 1024 / 512 - 2048 ))   # sectors after the MBR gap
dd if=/dev/zero of="$WORK/disk.img" bs=1M count="$DISK_MB" status=none
printf 'o\nn\np\n1\n\n\nw\n' | fdisk "$WORK/disk.img" >/dev/null 2>&1 || fail "fdisk failed"
dd if="$WORK/disk.img" of="$WORK/part.bin" bs=512 skip=2048 count="$PART_SECTORS" status=none \
    || fail "extract partition failed"
mkfs.vfat "$WORK/part.bin" >/dev/null 2>&1 || fail "mkfs.vfat failed"
dd if="$WORK/part.bin" of="$WORK/disk.img" bs=512 seek=2048 conv=notrunc status=none \
    || fail "write back failed"
mdir -i "$WORK/disk.img@@1048576" ::/ >/dev/null 2>&1 || fail "mdir failed"

P="$WORK/disk.img@@1048576"
mcopy -i "$P" "$KERNEL_ELF" ::kernel-uboot.elf || fail "mcopy kernel failed"
i=0
for m in "${MODULES[@]}"; do
    case "$m" in
        memory|tty) art="minix-driver-$m" ;;
        pfs|mfs) art="minix-fs-$m" ;;
        *) art="minix-$m" ;;
    esac
    mcopy -i "$P" "$REL/$art" ::m_"$m" || fail "mcopy module $m failed"
    i=$((i + 1))
done
mcopy -i "$P" "$WORK/qemu.dtb" ::qemu.dtb || fail "mcopy dtb failed"
mcopy -i "$P" "$SHIM_ELF" ::shim.elf || fail "mcopy shim failed"

echo "  disk ready: kernel + 12 modules + dtb + shim staged"

# ── Stage 5: boot.scr — the actual U-Boot hand-over sequence ──────────
cat > "$WORK/boot.cmd" <<EOS
setenv autostart yes
fatload virtio 0 0x86000000 kernel-uboot.elf
fatload virtio 0 0x87000000 m_ds
fatload virtio 0 0x87100000 m_rs
fatload virtio 0 0x87200000 m_pm
fatload virtio 0 0x87300000 m_sched
fatload virtio 0 0x87400000 m_vfs
fatload virtio 0 0x87500000 m_memory
fatload virtio 0 0x87600000 m_tty
fatload virtio 0 0x87700000 m_mib
fatload virtio 0 0x87800000 m_vm
fatload virtio 0 0x87900000 m_pfs
fatload virtio 0 0x87a00000 m_mfs
fatload virtio 0 0x87b00000 m_init
fatload virtio 0 0x8c000000 qemu.dtb
fatload virtio 0 \${loadaddr} shim.elf
if test \$? -ne 0; then echo UBOOT-SHIM: fatload failed; else
bootelf -p \${loadaddr} ${DTB_ADDR}
fi
EOS
mkimage -T script -C none -n "minix-rs uboot shim chain" \
    -d "$WORK/boot.cmd" "$WORK/boot.scr.img" || fail "mkimage script failed"
mcopy -o -i "$P" "$WORK/boot.scr.img" ::boot.scr || fail "mcopy scr failed"

# ── Stage 6: boot under U-Boot; serial to a log; wait for markers ─────
echo "== Stage 5: U-Boot chain (timeout ${TIMEOUT_SECS}s) =="
SERIAL="$WORK/serial.log"
timeout "$TIMEOUT_SECS" qemu-system-riscv64 \
    -machine virt -smp 1 -m 512M \
    -bios default \
    -kernel "$UBOOT" \
    -drive "if=virtio,format=raw,file=$WORK/disk.img" \
    -nographic -serial "file:$SERIAL" -monitor none \
    > "$WORK/qemu.stdout" 2>&1 &
QEMU_PID=$!
kill_qemu() { kill "$QEMU_PID" 2>/dev/null || true; wait "$QEMU_PID" 2>/dev/null || true; }
trap 'kill_qemu; archive; rm -rf "$WORK"' EXIT

verdict=""
stall=0
last_total=-1
for _ in $(seq 1 "$TIMEOUT_SECS"); do
    if [ -f "$SERIAL" ]; then
        if grep -qa 'kernel panic\|PANIC\|Unhandled exception' "$SERIAL"; then
            verdict="PANIC"; break
        fi
        n1=0; grep -qa "boot-shim: uboot leg enter (d73)" "$SERIAL" && n1=1
        n2=0; grep -qa "boot-shim: table built (MNXBOOT1" "$SERIAL" && n2=1
        n3=0; grep -qa "boot-shim: kernel + 12 modules loaded (d73)" "$SERIAL" && n3=1
        n4=0; grep -qa "boot-shim: \[raw\] entering kernel image v1" "$SERIAL" && n4=1
        n5=0; grep -qa "$KERNEL_MARKER" "$SERIAL" && n5=1
        n6=0; grep -qa "$SCHED_MARKER" "$SERIAL" && n6=1
        total=$((n1 + n2 + n3 + n4 + n5 + n6))
        if [ "$total" -eq 6 ]; then verdict="PASS"; break; fi
        if [ "$total" -gt 0 ]; then
            if [ "$total" -eq "$last_total" ]; then
                stall=$((stall + 1))
                [ "$stall" -ge 120 ] && { verdict="STALLED"; break; }
            else
                stall=0
            fi
            last_total=$total
        fi
    fi
    sleep 1
done
kill_qemu

case "$verdict" in
    PASS)
        echo "RESULT: PASS (D-73 U-Boot → boot-shim → kernel-uboot full chain: \
6/6 markers, entering scheduler)"
        exit 0
        ;;
    PANIC)
        echo "FAIL: guest panic during the U-Boot chain"
        grep -a -m4 -B2 -A3 "panic\|Unhandled exception" "$SERIAL" | cat -v || true
        exit 1
        ;;
    STALLED)
        echo "FAIL: progress stalled at $total/6 markers"
        grep -a "boot-shim\|kernel-image\|entering scheduler" "$SERIAL" | tail -6 | cat -v || true
        exit 1
        ;;
    *)
        echo "FAIL: markers never appeared within ${TIMEOUT_SECS}s"
        tail -12 "$SERIAL" 2>/dev/null | cat -v || true
        exit 1
        ;;
esac
