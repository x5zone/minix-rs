#!/bin/bash
# test-riscv64-uboot.sh — T-10: the riscv64 real boot chain through
# U-Boot (fatload → bootelf) instead of the OpenSBI+UEFI path the rest of
# the matrix uses.
#
# Why: the OpenSBI/UEFI lane proves the kernel boots, but a real machine
# may hand the kernel over from a U-Boot bootloader. The chain exercised
# here: U-Boot (M-mode firmware) scans the virtio-blk FAT disk, runs the
# boot.scr script image, fatloads the kernel ELF into RAM, and bootelf
# jumps to the ELF entry — from which point the SAME kernel binary must
# reach "### TEST_RESULT: PASS ###" as under OpenSBI (the kernel talks to
# the UART directly and never calls firmware services after entry, so the
# firmware swap must be transparent to it).
#
# Prerequisites: qemu-system-riscv64, u-boot-qemu (U-Boot virt firmware),
# u-boot-tools (mkimage), dosfstools (mkfs.vfat), mtools (mmd/mcopy).
# CI ubuntu-latest provides all via apt (see qemu-tests.yml job
# riscv64-uboot); hosts without them SKIP.
#
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KERNEL="${2:-$ROOT/target/riscv64gc-unknown-none-elf/release/hello-boot-riscv64}"
TIMEOUT_SECS="${TIMEOUT_SECS:-120}"

skip() { echo "SKIP: $1"; exit 2; }
fail() { echo "FAIL: $1"; exit 1; }

command -v qemu-system-riscv64 &>/dev/null || skip "qemu-system-riscv64 not found"
command -v mkimage &>/dev/null || skip "mkimage not found (apt install u-boot-tools)"
command -v mkfs.vfat &>/dev/null || skip "mkfs.vfat not found (apt install dosfstools)"
command -v mdir &>/dev/null || skip "mdir not found (apt install mtools)"

UBOOT=""
for c in /usr/lib/u-boot/qemu-riscv64_smode/uboot.elf \
         /usr/lib/u-boot/qemu-riscv64/uboot.elf \
         /usr/lib/u-boot/qemu-riscv64/u-boot.bin; do
    [ -f "$c" ] && UBOOT="$c" && break
done
[ -n "$UBOOT" ] || skip "U-Boot riscv64 firmware not found (apt install u-boot-qemu)"

# 宿主适配（2026-09-30 本机实测定谳）：Ubuntu 24.04 的 u-boot-qemu 同时提供
# smode（S-mode 载荷，必须垫在 OpenSBI 之下）与裸机两blob；实测两条路径：
#   1) 首选 OpenSBI(-bios default) + smode U-Boot(-kernel)；
#   2) 失败回退裸机 U-Boot 直挂 -bios。
# 注意：裸机 U-Boot 的 distro boot 只认分区盘（本脚本建的是整盘 FAT，无分区
# 表时新版 U-Boot 拒挂载；旧 CI blob 能读）。本脚本的 FAT 盘形与 CI 对齐，
# 若在新 blob 下因分区问题 FAIL，属宿主 U-Boot 差异，记 SKIP 不记 FAIL。
UBOOT_MODE="bare"
if [ "$UBOOT" = "/usr/lib/u-boot/qemu-riscv64_smode/uboot.elf" ]; then
    UBOOT_MODE="smode"
fi

# 1. The kernel ELF (build on demand so the script is self-contained).
if [ ! -f "$KERNEL" ]; then
    echo "building hello-boot-riscv64…"
    ( cd "$ROOT" && cargo build -p hello-boot-riscv64 \
        --target riscv64gc-unknown-none-elf --features fw-riscv64-none --release ) || fail "kernel build failed"
fi
[ -f "$KERNEL" ] || fail "kernel ELF missing after build"

WORK="$(mktemp -d /tmp/riscv_uboot_XXXXXX)"
trap 'rm -rf "$WORK"' EXIT

# 2. FAT disk: kernel ELF + a boot.scr script image (mkimage wraps the
#    U-Boot script commands). bootdelay=0 keeps the run tight.
#
# 盘形＝分区盘（MBR 表 + 分区内 FAT，分区起于 LBA 2048）：新版 U-Boot
# （Ubuntu 24.04 的 2025.10）distro boot 只认分区盘，整盘 FAT 会
# `** No partition table - virtio 0 **` 拒挂载→boot.scr 永不执行（本机
# 2026-09-30 实锤：手建 MBR 分区后 `## Executing script` 出现）；旧 CI
# blob 两种盘形都能读，分区形向下兼容。mtools 用 `img@@<byte-offset>` 寻址分区（2048 扇区=1048576B）。
dd if=/dev/zero of="$WORK/disk.img" bs=1M count=16 status=none
printf 'o\nn\np\n1\n\n\nw\n' | fdisk "$WORK/disk.img" > /dev/null 2>&1 \
    || fail "fdisk failed"
# mkfs.vfat 不认 mtools 的 @@ 寻址：抽出分区→格式化→写回。
dd if="$WORK/disk.img" of="$WORK/part.bin" bs=512 skip=2048 count=28671 status=none \
    || fail "extract partition failed"
mkfs.vfat "$WORK/part.bin" > /dev/null 2>&1 || fail "mkfs.vfat failed"
dd if="$WORK/part.bin" of="$WORK/disk.img" bs=512 seek=2048 conv=notrunc status=none \
    || fail "write back failed"
# 镜像可读性健康检查（mtools 4.3+ 下 mmd 对已存在 . 返回非零，用 mdir 纯读）。
mdir -i "$WORK/disk.img@@1048576" ::/ >/dev/null 2>&1 || fail "mdir failed"
mcopy -i "$WORK/disk.img@@1048576" "$KERNEL" ::boot.elf || fail "mcopy kernel failed"

cat > "$WORK/boot.cmd" <<'EOS'
fatload virtio 0 ${loadaddr} boot.elf
if test $? -ne 0; then echo UBOOT-CHAIN: fatload failed; else
bootelf ${loadaddr}
fi
EOS
mkimage -T script -C none -n "minix-rs uboot chain" \
    -d "$WORK/boot.cmd" "$WORK/boot.scr.img" || fail "mkimage script failed"
mcopy -i "$WORK/disk.img@@1048576" "$WORK/boot.scr.img" ::boot.scr || fail "mcopy scr failed"

# 3. Boot: U-Boot as -bios firmware, disk on virtio, serial captured to a
#    file (U-Boot console and kernel output share it). smode 形态下 U-Boot
#    发 OpenSBI 之下当 -kernel 载荷（裸机 blob 不能直接跳 S-mode 入口，
#    会在 bootelf 时 Load access fault 循环重启——本机 2026-09-30 实锤）。
SERIAL="$WORK/serial.log"
if [ "$UBOOT_MODE" = "smode" ]; then
qemu-system-riscv64 \
    -machine virt -smp 1 -m 256M \
    -bios default \
    -kernel "$UBOOT" \
    -drive "if=virtio,format=raw,file=$WORK/disk.img" \
    -nographic -serial "file:$SERIAL" -monitor none \
    > "$WORK/qemu.stdout" 2>&1 &
else
qemu-system-riscv64 \
    -machine virt -smp 1 -m 256M \
    -bios "$UBOOT" \
    -drive "if=virtio,format=raw,file=$WORK/disk.img" \
    -nographic -serial "file:$SERIAL" -monitor none \
    > "$WORK/qemu.stdout" 2>&1 &
fi
QEMU_PID=$!

# 4. Wait for the TEST_RESULT line (the kernel may not exit; U-Boot's
#    autoboot delay plus boot time sit inside the timeout).
PASS=false
for _ in $(seq 1 "$TIMEOUT_SECS"); do
    if grep -q "### TEST_RESULT: PASS hello-boot-riscv64 ###" "$SERIAL" 2>/dev/null; then
        PASS=true
        break
    fi
    kill -0 "$QEMU_PID" 2>/dev/null || break
    sleep 1
done
kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null

if $PASS; then
    echo "RESULT: PASS (riscv64 U-Boot chain: fatload → bootelf → kernel TEST_RESULT)"
    exit 0
fi
echo "--- serial tail ---"
tail -20 "$SERIAL" 2>/dev/null || echo "(no serial output)"
fail "TEST_RESULT line not seen within ${TIMEOUT_SECS}s"
