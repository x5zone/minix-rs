#!/usr/bin/env bash
# run_qemu.sh — single test runner
# Usage: ./run_qemu.sh <arch> <test_efi_path>

set -euo pipefail

ARCH="${1:-x86_64}"
TEST_EFI="${2:-test.efi}"
SERIAL_LOG="/tmp/qemu_serial_$$.log"

case "$ARCH" in
    x86_64)
        QEMU="qemu-system-x86_64"
        FW_CODE_CANDIDATES=(
            "/usr/share/OVMF/OVMF_CODE_4M.fd"
            "/usr/share/OVMF/OVMF_CODE.fd"
            "/usr/share/ovmf/OVMF.fd"
        )
        FW_VARS_CANDIDATES=(
            "/usr/share/OVMF/OVMF_VARS_4M.fd"
            "/usr/share/OVMF/OVMF_VARS.fd"
        )
        ;;
    aarch64)
        QEMU="qemu-system-aarch64"
        FW_CODE_CANDIDATES=(
            "/usr/share/AAVMF/AAVMF_CODE.fd"
            "/usr/share/qemu-efi-aarch64/QEMU_EFI.fd"
        )
        FW_VARS_CANDIDATES=(
            "/usr/share/AAVMF/AAVMF_VARS.fd"
        )
        ;;
    riscv64)
        QEMU="qemu-system-riscv64"
        FW_CODE_CANDIDATES=(
            "/usr/share/qemu-efi-riscv64/RISCV_VIRT_CODE.fd"
        )
        FW_VARS_CANDIDATES=(
            "/usr/share/qemu-efi-riscv64/RISCV_VIRT_VARS.fd"
        )
        ;;
    *)
        echo "Unknown arch: $ARCH"
        exit 1
        ;;
esac

# Find firmware
FW=""
for c in "${FW_CODE_CANDIDATES[@]}"; do
    [ -f "$c" ] && FW="$c" && break
done
FW_SRC_VARS=""
for c in "${FW_VARS_CANDIDATES[@]}"; do
    [ -f "$c" ] && FW_SRC_VARS="$c" && break
done

# Build staging directory with EFI/BOOT structure
STAGING="/tmp/qemu_staging_$$"
rm -rf "$STAGING"
mkdir -p "$STAGING/EFI/BOOT"
case "$ARCH" in
    x86_64)  BOOT_NAME="BOOTX64.EFI" ;;
    aarch64) BOOT_NAME="BOOTAA64.EFI" ;;
    riscv64) BOOT_NAME="BOOTRISCV64.EFI" ;;
esac
cp "$TEST_EFI" "$STAGING/EFI/BOOT/$BOOT_NAME"

# Build a FAT disk image from the staging directory.
# We prefer a real disk image over QEMU's fat:rw: driver because the latter
# needs /var/tmp which may be read-only in containers.
# Falls back to fat:rw: if disk image tools are not available.
DISK_IMG="/tmp/qemu_disk_$$.img"
USE_DISK_IMG=false
if command -v mkfs.vfat &>/dev/null && command -v mmd &>/dev/null && command -v mcopy &>/dev/null; then
    if dd if=/dev/zero of="$DISK_IMG" bs=1M count=64 status=none 2>/dev/null \
       && mkfs.vfat "$DISK_IMG" > /dev/null 2>&1 \
       && mmd -i "$DISK_IMG" ::EFI ::EFI/BOOT > /dev/null 2>&1 \
       && mcopy -i "$DISK_IMG" "$STAGING/EFI/BOOT/$BOOT_NAME" "::EFI/BOOT/$BOOT_NAME" > /dev/null 2>&1; then
        USE_DISK_IMG=true
    fi
fi

# Build QEMU arguments
QEMU_ARGS=()
case "$ARCH" in
    x86_64)
        if [ -z "$FW" ] || [ -z "$FW_SRC_VARS" ]; then
            echo "ERROR: OVMF firmware not found. Install: sudo apt install ovmf"
            exit 1
        fi
        FW_VARS="/tmp/ovmf_vars_$$.fd"
        cp "$FW_SRC_VARS" "$FW_VARS"
        if $USE_DISK_IMG; then
            DISK_DRIVE="file=$DISK_IMG,format=raw,media=disk"
        else
            DISK_DRIVE="file=fat:rw:$STAGING,format=raw,media=disk"
        fi
        QEMU_ARGS=(
            -drive "if=pflash,format=raw,unit=0,file=$FW,readonly=on"
            -drive "if=pflash,format=raw,unit=1,file=$FW_VARS"
            -drive "$DISK_DRIVE"
            -serial "file:$SERIAL_LOG"
            -display none
            -no-reboot
        )
        ;;
    aarch64)
        if [ -z "$FW" ] || [ -z "$FW_SRC_VARS" ]; then
            echo "ERROR: AAVMF firmware not found. Install: sudo apt install qemu-efi-aarch64"
            exit 1
        fi
        FW_VARS="/tmp/aavmf_vars_$$.fd"
        cp "$FW_SRC_VARS" "$FW_VARS"
        if $USE_DISK_IMG; then
            DISK_DRIVE="file=$DISK_IMG,format=raw,media=disk"
        else
            DISK_DRIVE="file=fat:rw:$STAGING,format=raw,media=disk"
        fi
        QEMU_ARGS=(
            -machine virt
            -cpu cortex-a72
            -drive "if=pflash,format=raw,unit=0,file=$FW,readonly=on"
            -drive "if=pflash,format=raw,unit=1,file=$FW_VARS"
            -drive "$DISK_DRIVE"
            -serial "file:$SERIAL_LOG"
            -display none
            -no-reboot
        )
        ;;
    riscv64)
        if [ -n "$FW" ] && [ -n "$FW_SRC_VARS" ] && [[ "$TEST_EFI" == *.efi ]]; then
            FW_VARS="/tmp/riscv_vars_$$.fd"
            cp "$FW_SRC_VARS" "$FW_VARS"
            if $USE_DISK_IMG; then
                DISK_DRIVE="file=$DISK_IMG,format=raw,media=disk"
            else
                DISK_DRIVE="file=fat:rw:$STAGING,format=raw,media=disk"
            fi
            QEMU_ARGS=(
                -machine virt,acpi=off
                -drive "if=pflash,format=raw,unit=0,file=$FW,readonly=on"
                -drive "if=pflash,format=raw,unit=1,file=$FW_VARS"
                -drive "$DISK_DRIVE"
                -serial "file:$SERIAL_LOG"
                -display none
                -no-reboot
            )
        else
            QEMU_ARGS=(
                -machine virt
                -bios default
                -kernel "$TEST_EFI"
                -serial "file:$SERIAL_LOG"
                -display none
                -no-reboot
            )
        fi
        ;;
esac

echo "=== QEMU Test: $ARCH — $TEST_EFI ==="

# QEMU's fat:rw: driver needs a writable TMPDIR for temporary files.
# In containers /var/tmp may be read-only, so force TMPDIR to /tmp.
export TMPDIR=/tmp
timeout 30 "$QEMU" "${QEMU_ARGS[@]}" 2>&1 || true

# Print serial output (sanitize binary chars for terminal display)
if [ -f "$SERIAL_LOG" ]; then
    cat -v "$SERIAL_LOG"
fi

# Clean up staging directory, firmware VARS, and disk image
rm -rf "$STAGING"
rm -f /tmp/ovmf_vars_$$.fd /tmp/aavmf_vars_$$.fd /tmp/riscv_vars_$$.fd
rm -f "$DISK_IMG"

# Check for PASS marker (use strings to handle binary serial logs)
PASSED=false
if [ -f "$SERIAL_LOG" ] && strings "$SERIAL_LOG" | grep -q "### TEST_RESULT: PASS"; then
    PASSED=true
fi

rm -f "$SERIAL_LOG"

if $PASSED; then
    echo "✅ TEST PASSED"
    exit 0
else
    echo "❌ TEST FAILED — no PASS marker found"
    exit 1
fi
