#!/usr/bin/env bash
# run_all.sh — CI entry point. Runs all QEMU integration tests.
#
# Test categories:
#   1. hello-boot:      full boot chain (UEFI/OpenSBI → Paging trait → serial output)
#   2. test-memmap:     KernelInfo.memmap reflects real physical memory layout
#   3. test-paging-enable: paging.enable() switches page table base and CPU survives
#   4. test-kernel-map: high-half/identity mapping returns correct data
#   5. test-higher-half: HigherHalf trait transition (stack/PC switch → kmain)
#   6. test-protection: protection structure init (GDT/IDT/TSS, VBAR/SP_EL1, stvec/sscratch)
#   7. test-proc-init: process table init + VM ELF loading + ptproc/freepdes (Phase C/D)
#   8. test-user-trap / test-rt-birth / test-sysboot: the CPL3 user-mode
#      chain and the multi-process boot carrier. These run through their
#      own scripts — their PASS protocol is gdbstub mailbox reads / serial
#      markers, not a TEST_RESULT line (edge1 K12).
#
# Each test writes "### TEST_RESULT: PASS <name> ###" on success.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OS_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PASS=0
FAIL=0
SKIP=0
# SD-28 / P-ALL-09：构建失败必须真报红。历史行为＝`|| echo "(build failed)"`
# 吞掉退出码，下游 "binary not found, skip" 静默 return（连 SKIP 都不计数），
# 构建坏了整表照绿。现在：构建失败记入 FAILED_PKGS，缺二进制的运行按
# FAIL 计（不再冒充 SKIP），入口最终退出码非零。
BUILD_FAIL=0
FAILED_PKGS=""

run_test() {
    local name="$1"
    local arch="$2"
    local binary="$3"

    if [ ! -f "$binary" ]; then
        # 二进制缺失分两类：构建失败所致＝FAIL（P-ALL-09）；包本身不在
        # 本仓构建清单＝SKIP（维持旧语义）。
        if pkg_build_failed "$(basename "$binary" | sed 's/\.efi$//')"; then
            echo "($name: build failed, counted as FAILURE)"
            FAIL=$((FAIL + 1))
        else
            echo "($name: binary not found, skip)"
            SKIP=$((SKIP + 1))
        fi
        return
    fi

    echo "--- Running: $name ($arch) ---"
    local out rc=0
    out=$("$SCRIPT_DIR/run_qemu.sh" "$arch" "$binary" 2>&1) || rc=$?
    echo "$out"
    if echo "$out" | grep -q "TEST SKIPPED"; then
        SKIP=$((SKIP + 1))
    elif [ "$rc" -eq 0 ]; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
    fi
}

echo "=== Building test kernels ==="

# 构建帮助函数：失败即记 BUILD_FAIL 并把包名挂进 FAILED_PKGS（供运行段
# 把「缺二进制」归类为失败而非跳过）。X-2/NK5：--target 与 --features
# 必须成对（required-features 门），缺 --features 会被静默跳过 → 下游
# "binary not found" 全表 skip——本函数不改这条契约，只改计数诚实度。
build_pkg() {
    local target="$1" feat="$2" pkg="$3"
    echo "--- $pkg ---"
    if ! cargo build --manifest-path "$OS_ROOT/Cargo.toml" -p "$pkg" \
            --target "$target" --features "$feat" --release 2>&1; then
        echo "(build failed: $pkg)"
        BUILD_FAIL=$((BUILD_FAIL + 1))
        FAILED_PKGS="$FAILED_PKGS $pkg"
    fi
}

# ── x86_64 (UEFI) ──
for pkg in hello-boot test-memmap test-paging-enable test-kernel-map test-higher-half test-protection test-proc-init test-smp-topo test-smp-ap-alive test-timer-irq test-smp-aps test-smp-ipi test-smp-shutdown test-user-trap test-paging-faultloop; do
    build_pkg x86_64-unknown-uefi fw-x86-uefi "$pkg"
done

# ── aarch64 (UEFI) ──
for pkg in hello-boot-aarch64 test-memmap-aarch64 test-paging-enable-aarch64 test-kernel-map-aarch64 test-higher-half-aarch64 test-protection-aarch64 test-smp-topo-aarch64 test-rt-birth-aarch64 test-shutdown-aarch64 test-timer-irq-aarch64; do
    build_pkg aarch64-unknown-uefi fw-aarch64-uefi "$pkg"
done

# ── riscv64 (OpenSBI, bare-metal) ──
for pkg in hello-boot-riscv64 test-memmap-riscv64 test-paging-enable-riscv64 test-kernel-map-riscv64 test-higher-half-riscv64 test-protection-riscv64 test-smp-topo-riscv64 test-smp-ipi-riscv64 test-rt-birth-riscv64 test-shutdown-riscv64 test-timer-irq-riscv64; do
    build_pkg riscv64gc-unknown-none-elf fw-riscv64-none "$pkg"
done

pkg_build_failed() {
    case " $FAILED_PKGS " in
        *" $1 "*) return 0 ;;
        *) return 1 ;;
    esac
}

echo ""
echo "=== Running QEMU tests ==="

# x86_64 tests
if command -v qemu-system-x86_64 &>/dev/null; then
    run_test "hello-boot"              x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/hello-boot.efi"
    run_test "test-memmap"             x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/test-memmap.efi"
    run_test "test-paging-enable"      x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/test-paging-enable.efi"
    run_test "test-kernel-map"         x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/test-kernel-map.efi"
    run_test "test-higher-half"        x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/test-higher-half.efi"
    run_test "test-protection"         x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/test-protection.efi"
    run_test "test-proc-init"          x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/test-proc-init.efi"
    run_test "test-smp-topo"           x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/test-smp-topo.efi"
    run_test "test-timer-irq"          x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/test-timer-irq.efi"
    run_test "test-smp-aps"            x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/test-smp-aps.efi"
    run_test "test-smp-ipi"            x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/test-smp-ipi.efi"
    run_test "test-smp-shutdown"       x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/test-smp-shutdown.efi"
    # S-3d landed 2026-09-14 (both the OVMF variant and the multiboot
    # variant pass); the env-var skip remains as a CI escape hatch.
    if [ "${QEMU_TESTS_SKIP_AP_ALIVE:-0}" != "1" ]; then
        run_test "test-smp-ap-alive"   x86_64   "$OS_ROOT/target/x86_64-unknown-uefi/release/test-smp-ap-alive.efi"
    fi

    # ── Special-protocol scripts (edge1 K12): the user-mode chain tests
    # own their PASS determination — test-user-trap reads a CPL3 mailbox
    # through the gdbstub, test-rt-birth builds its own kernel with the
    # user ELF embedded (include_bytes!(env!)) and greps serial markers.
    # Both exit 0 = PASS, 1 = FAIL, 2 = SKIP (prerequisites missing).

    echo "--- Running: test-user-trap (special: gdbstub mailbox) ---"
    if [ -f "$OS_ROOT/target/x86_64-unknown-uefi/release/test-user-trap.efi" ]; then
        rc=0
        bash "$SCRIPT_DIR/test-user-trap.sh" || rc=$?
        if [ "$rc" -eq 0 ]; then PASS=$((PASS + 1));
        elif [ "$rc" -eq 2 ]; then SKIP=$((SKIP + 1)); echo "(test-user-trap: skipped)"
        else FAIL=$((FAIL + 1)); fi
    else
        echo "(test-user-trap: binary not found, skip)"
        SKIP=$((SKIP + 1))
    fi

    echo "--- Running: test-paging-faultloop (special: gdbstub mailbox) ---"
    if [ -f "$OS_ROOT/target/x86_64-unknown-uefi/release/test-paging-faultloop.efi" ]; then
        rc=0
        bash "$SCRIPT_DIR/test-paging-faultloop.sh" || rc=$?
        if [ "$rc" -eq 0 ]; then PASS=$((PASS + 1));
        elif [ "$rc" -eq 2 ]; then SKIP=$((SKIP + 1)); echo "(test-paging-faultloop: skipped)"
        else FAIL=$((FAIL + 1)); fi
    else
        echo "(test-paging-faultloop: binary not found, skip)"
        SKIP=$((SKIP + 1))
    fi

    echo "--- Running: test-rt-birth (special: embedded-ELF payload) ---"
    rc=0
    bash "$SCRIPT_DIR/test-rt-birth.sh" || rc=$?
    if [ "$rc" -eq 0 ]; then PASS=$((PASS + 1));
    elif [ "$rc" -eq 2 ]; then SKIP=$((SKIP + 1)); echo "(test-rt-birth: skipped)"
    else FAIL=$((FAIL + 1)); fi

    # ── Special-protocol script (S42 ④ / C-27 / NK1): the multi-process
    # boot carrier builds both user ELFs + its own kernel and judges PASS
    # from the six serial exchange markers (RX UP → TX DONE).
    echo "--- Running: test-sysboot (special: two-image IPC serial markers) ---"
    rc=0
    bash "$SCRIPT_DIR/test-sysboot.sh" || rc=$?
    if [ "$rc" -eq 0 ]; then PASS=$((PASS + 1));
    elif [ "$rc" -eq 2 ]; then SKIP=$((SKIP + 1)); echo "(test-sysboot: skipped)"
    else FAIL=$((FAIL + 1)); fi
fi

# aarch64 tests
if command -v qemu-system-aarch64 &>/dev/null; then
    run_test "hello-boot-aarch64"      aarch64  "$OS_ROOT/target/aarch64-unknown-uefi/release/hello-boot-aarch64.efi"
    run_test "test-memmap-aarch64"     aarch64  "$OS_ROOT/target/aarch64-unknown-uefi/release/test-memmap-aarch64.efi"
    run_test "test-paging-enable-aarch64" aarch64 "$OS_ROOT/target/aarch64-unknown-uefi/release/test-paging-enable-aarch64.efi"
    run_test "test-kernel-map-aarch64" aarch64  "$OS_ROOT/target/aarch64-unknown-uefi/release/test-kernel-map-aarch64.efi"
    run_test "test-higher-half-aarch64" aarch64  "$OS_ROOT/target/aarch64-unknown-uefi/release/test-higher-half-aarch64.efi"
    run_test "test-protection-aarch64" aarch64  "$OS_ROOT/target/aarch64-unknown-uefi/release/test-protection-aarch64.efi"
    run_test "test-smp-topo-aarch64"   aarch64  "$OS_ROOT/target/aarch64-unknown-uefi/release/test-smp-topo-aarch64.efi"

    # ── Special-protocol script (edge1 K12b): the aarch64 birth-chain
    # carrier builds the rt-birth user image itself and judges PASS from
    # the serial markers.
    echo "--- Running: test-rt-birth-aarch64 (special: birth-chain serial markers) ---"
    rc=0
    bash "$SCRIPT_DIR/test-rt-birth-aarch64.sh" || rc=$?
    if [ "$rc" -eq 0 ]; then PASS=$((PASS + 1));
    elif [ "$rc" -eq 2 ]; then SKIP=$((SKIP + 1)); echo "(test-rt-birth-aarch64: skipped)"
    else FAIL=$((FAIL + 1)); fi

    # ── Special-protocol script (edge1 K11): aarch64 shutdown verification
    # — the script asserts the serial marker AND QEMU's own exit code.
    echo "--- Running: test-shutdown-aarch64 (special: semihosting exit code) ---"
    rc=0
    bash "$SCRIPT_DIR/test-shutdown-aarch64.sh" || rc=$?
    if [ "$rc" -eq 0 ]; then PASS=$((PASS + 1));
    elif [ "$rc" -eq 2 ]; then SKIP=$((SKIP + 1)); echo "(test-shutdown-aarch64: skipped)"
    else FAIL=$((FAIL + 1)); fi

    # ── Special-protocol script (E-3ARCHTRAP / NK3): the aarch64 timer
    # carrier judges PASS from the serial verdict line (CNTP → GIC →
    # production VBAR legs → uptime).
    echo "--- Running: test-timer-irq-aarch64 (special: AAVMF serial verdict) ---"
    rc=0
    bash "$SCRIPT_DIR/test-timer-irq-aarch64.sh" || rc=$?
    if [ "$rc" -eq 0 ]; then PASS=$((PASS + 1));
    elif [ "$rc" -eq 2 ]; then SKIP=$((SKIP + 1)); echo "(test-timer-irq-aarch64: skipped)"
    else FAIL=$((FAIL + 1)); fi
fi

# riscv64 tests
if command -v qemu-system-riscv64 &>/dev/null; then
    run_test "hello-boot-riscv64"      riscv64  "$OS_ROOT/target/riscv64gc-unknown-none-elf/release/hello-boot-riscv64"
    run_test "test-smp-topo-riscv64"   riscv64  "$OS_ROOT/target/riscv64gc-unknown-none-elf/release/test-smp-topo-riscv64"
    run_test "test-memmap-riscv64"     riscv64  "$OS_ROOT/target/riscv64gc-unknown-none-elf/release/test-memmap-riscv64"
    run_test "test-paging-enable-riscv64" riscv64 "$OS_ROOT/target/riscv64gc-unknown-none-elf/release/test-paging-enable-riscv64"
    run_test "test-kernel-map-riscv64" riscv64  "$OS_ROOT/target/riscv64gc-unknown-none-elf/release/test-kernel-map-riscv64"
    run_test "test-higher-half-riscv64" riscv64  "$OS_ROOT/target/riscv64gc-unknown-none-elf/release/test-higher-half-riscv64"
    run_test "test-protection-riscv64" riscv64  "$OS_ROOT/target/riscv64gc-unknown-none-elf/release/test-protection-riscv64"

    # ── Special-protocol script (edge1 K10 round 4): the IPI round-trip
    # carrier determines its own PASS from the serial verdict line and
    # needs `aclint=on` (SSWI direct-write delivery).
    echo "--- Running: test-smp-ipi-riscv64 (special: aclint=on serial verdict) ---"
    if [ -f "$OS_ROOT/target/riscv64gc-unknown-none-elf/release/test-smp-ipi-riscv64" ]; then
        rc=0
        bash "$SCRIPT_DIR/test-smp-ipi-riscv64.sh" || rc=$?
        if [ "$rc" -eq 0 ]; then PASS=$((PASS + 1));
        elif [ "$rc" -eq 2 ]; then SKIP=$((SKIP + 1)); echo "(test-smp-ipi-riscv64: skipped)"
        else FAIL=$((FAIL + 1)); fi
    else
        echo "(test-smp-ipi-riscv64: binary not found, skip)"
        SKIP=$((SKIP + 1))
    fi

    # ── Special-protocol script (edge1 K12b): the riscv64 birth-chain
    # carrier builds the rt-birth user image itself and judges PASS from
    # the serial markers.
    echo "--- Running: test-rt-birth-riscv64 (special: birth-chain serial markers) ---"
    rc=0
    bash "$SCRIPT_DIR/test-rt-birth-riscv64.sh" || rc=$?
    if [ "$rc" -eq 0 ]; then PASS=$((PASS + 1));
    elif [ "$rc" -eq 2 ]; then SKIP=$((SKIP + 1)); echo "(test-rt-birth-riscv64: skipped)"
    else FAIL=$((FAIL + 1)); fi

    # ── Special-protocol script (edge1 K11): riscv64 shutdown verification
    # — the script asserts the serial marker AND QEMU's own exit code.
    echo "--- Running: test-shutdown-riscv64 (special: sifive_test exit code) ---"
    rc=0
    bash "$SCRIPT_DIR/test-shutdown-riscv64.sh" || rc=$?
    if [ "$rc" -eq 0 ]; then PASS=$((PASS + 1));
    elif [ "$rc" -eq 2 ]; then SKIP=$((SKIP + 1)); echo "(test-shutdown-riscv64: skipped)"
    else FAIL=$((FAIL + 1)); fi

    # ── Special-protocol script (E-3ARCHTRAP / NK3): the riscv64 timer
    # carrier judges PASS from the serial verdict line (SBI TIME →
    # production stvec kernel leg → uptime).
    echo "--- Running: test-timer-irq-riscv64 (special: OpenSBI serial verdict) ---"
    rc=0
    bash "$SCRIPT_DIR/test-timer-irq-riscv64.sh" || rc=$?
    if [ "$rc" -eq 0 ]; then PASS=$((PASS + 1));
    elif [ "$rc" -eq 2 ]; then SKIP=$((SKIP + 1)); echo "(test-timer-irq-riscv64: skipped)"
    else FAIL=$((FAIL + 1)); fi
fi

echo ""
echo "=== All QEMU tests done: $PASS passed, $FAIL failed, $SKIP skipped, $BUILD_FAIL build failures ==="
if [ "$BUILD_FAIL" -gt 0 ]; then
    echo "build-failed packages:$FAILED_PKGS"
fi

if [ "$FAIL" -gt 0 ] || [ "$BUILD_FAIL" -gt 0 ]; then
    exit 1
fi
