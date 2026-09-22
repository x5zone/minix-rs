#!/usr/bin/env bash
# NK4-B P3 M3.4 取证：aarch64 生产链的 platform panic 到底卡在哪一道门。
#
# 手法 = 单变量对照：同一个测试内核（test-smp-topo-aarch64，走的正是生产
# 那条 find_platform_sources → parse_by_kind → AcpiDesc::parse 链），只在
# `-machine virt` 的 gic-version 这一个轴上变化。run_qemu_gic3.sh 是本脚本
# 用 sed 从 os/qemu-tests/run_qemu.sh 派生的一次性副本，唯一差异是 aarch64
# 分支那一行 `-machine virt` → `-machine virt,gic-version=3`（脚本会先把
# diff 打出来自证）。
#
# 判据：两腿的 diag 行必须落在不同的 AcpiParseError 变体上，且两腿都必须
# FAIL——这才证明「ACPI 发现链在 QEMU virt + AAVMF 上没有任何一种 GIC 配置
# 能拿到描述符」，而不是某个配置写错。
set -uo pipefail

REPO=/home/xzhao/github/minix-rs
OS="$REPO/os"
W="$REPO/tmp/nk4a"
PKG=test-smp-topo-aarch64
EFI="$OS/target/aarch64-unknown-uefi/release/$PKG.efi"

echo "### 0) 派生单变量副本（diff 只应有一行）"
sed 's/^            -machine virt$/            -machine virt,gic-version=3/' \
    "$OS/qemu-tests/run_qemu.sh" > "$W/run_qemu_gic3.sh"
chmod +x "$W/run_qemu_gic3.sh"
diff "$OS/qemu-tests/run_qemu.sh" "$W/run_qemu_gic3.sh"

echo "### 1) 构建测试内核（aarch64-unknown-uefi / fw-aarch64-uefi）"
( cd "$OS" && ulimit -v 3145728 && cargo build -p "$PKG" \
    --target aarch64-unknown-uefi --features fw-aarch64-uefi --release ) >"$W/gic-build.log" 2>&1
echo "  BUILD-EXIT=$?"
[ -f "$EFI" ] || { echo "  FAIL: efi not produced"; exit 1; }

run_leg() {  # $1 = 腿名, $2 = runner 脚本
    pkill -f '[q]emu-system' 2>/dev/null || true
    sleep 1
    local out rc
    out=$(timeout 150 bash "$2" aarch64 "$EFI" 2>&1); rc=$?
    echo "### $1 (runner=$2, EXIT=$rc)"
    echo "$out" | grep -aE "platform sources:|diag: AcpiParseError|### FAIL|nr_cpus|TEST_RESULT" | cat -v | sed 's/^/    /'
}

echo "### 2) 腿 A：-machine virt（QEMU 默认 gic-version=2）"
run_leg "LEG-A gic-version=default" "$OS/qemu-tests/run_qemu.sh"
echo "### 3) 腿 B：-machine virt,gic-version=3（生产载体用的那个）"
run_leg "LEG-B gic-version=3" "$W/run_qemu_gic3.sh"
echo "### 4) 腿 A 复跑（同机时两次独立取证，排除偶发）"
run_leg "LEG-A rerun" "$OS/qemu-tests/run_qemu.sh"
pkill -f '[q]emu-system' 2>/dev/null || true
