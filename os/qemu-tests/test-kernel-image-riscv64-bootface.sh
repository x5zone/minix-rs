#!/bin/bash
# test-kernel-image-riscv64-bootface.sh — NK4-C 续-88 甲案入口腿的真机载体。
#
# 证一件事：kernel-image 的 riscv64 自引导面（`src/bootface.rs`）的两条
# 输入通道都能在固件递入下工作——
#   B1 DTB `/chosen` 的 `opensbi,boot-file-table` 属性被 bootface 解析，
#      并驱动 BootFileTable 的取值（QEMU 默认 DTB 没有该属性；本脚本
#      dumpdtb → fdtput 注入 → `-dtb` 回喂，通道与 U-Boot 腿的 a2 约定
#      等价，magic 闸同款）。
#   B2 表被消费：entry_count=0 的合法表过 magic 闸后，12 模块装载腿在
#      位置契约检查处点名失败（`table holds 0x0 entries, kernel contract
#      needs 0xc`——loader.rs 的 fail-fast 语义在甲案腿的落点），然后走
#      halt 臂。不假装模块存在。
#   B3 顺序：表行号排在 chosen 注入后的 memmap 行之后（防日志他处污染）。
#
# 反向判别（写进断言语境的两个坏面）：
#   - bootface 没读到 chosen 属性 → B2 的表行不出现，B1 FAIL；
#   - bootface 在 DTB 解析前崩 → B1/B2/B3 全红，串口尾部只有横幅行。
# 接线前（旧入口「横幅+halt」）三条断言都不存在——对本改动有判别力。
#
# 前置：qemu-system-riscv64、dtc（dumpdtb）、fdtput（device-tree-compiler）。
# 缺件 SKIP（exit 2），不 FAIL。
#
# 用法：bash os/qemu-tests/test-kernel-image-riscv64-bootface.sh [ELF]
#       RUN=<轮次名> 串口日志名；SKIP_BUILD=1 复用已有工件。
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP。

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs/os
TRIPLE="riscv64gc-unknown-none-elf"
KERNEL="${1:-$ROOT/target/$TRIPLE/release/kernel}"
RUN="${RUN:-jiaf1}"
TIMEOUT_BOOT="${TIMEOUT_BOOT:-60}"
TABLE_PA="0x8c000000"   # chosen 注入的表址；`-device loader` 同址放 blob
# （CodeReview SF2 伴生：表尾+40B 探测余量必须整体落在 -m 256M 的
#  DRAM 内——0x8f000000 的尾 40B 越 RAM 顶，改 0x8c000000）

skip() { echo "SKIP: $1"; exit 2; }
fail() { echo "FAIL: $1"; exit 1; }

command -v qemu-system-riscv64 &>/dev/null || skip "qemu-system-riscv64 not found"
command -v dtc &>/dev/null || skip "dtc not found (apt install device-tree-compiler)"
command -v fdtput &>/dev/null || skip "fdtput not found (apt install device-tree-compiler)"

# ── Stage 1: 镜像 ──
if [ "${SKIP_BUILD:-0}" != "1" ]; then
    echo "building kernel-image ($TRIPLE)…"
    ( cd "$ROOT" && (ulimit -v 3145728; \
        cargo build -q -p kernel-image --target "$TRIPLE" \
            --features fw-riscv64-none --release) ) || fail "kernel-image build failed"
fi
[ -f "$KERNEL" ] || skip "kernel ELF not found: $KERNEL"

WORK="$ROOT/target/$TRIPLE"
mkdir -p "$WORK" || fail "cannot create $WORK"

# ── Stage 2: DTB（QEMU 默认机型的树）+ chosen 注入 ──
BASE_DTB="$WORK/bootface-base.dtb"
QEMU_DTB="$WORK/bootface.dtb"
rm -f "$BASE_DTB" "$QEMU_DTB"
timeout 10 qemu-system-riscv64 -machine virt -m 256M -display none \
    -bios default -machine dumpdtb="$BASE_DTB" >/dev/null 2>&1 || true
[ -f "$BASE_DTB" ] || fail "dumpdtb 未产出 $BASE_DTB"
# 已有 /chosen 的树直接加属性；无 /chosen 节点则先造（fdtput -p 建路径；
# 类型词是 `-t x`，dtc 1.7 没有 `-i` 选项——本机实测钉住）。写成 4 字节大端
# cell，bootface 的 `be_cell` 认 4/8 字节两形。
fdtput -p -t x "$BASE_DTB" /chosen opensbi,boot-file-table "$TABLE_PA" \
    > /dev/null 2>&1 || fail "fdtput 注入 chosen 属性失败"
cp "$BASE_DTB" "$QEMU_DTB"

# ── Stage 3: BootFileTable blob（magic + entry_count=0，其余全零）──
# 布局对位 boot_shim::opensbi_helpers::BootFileTable（repr(C)）：
#   u64 magic("MNXBOOT1" = 0x31544f4f42584e4d) | u32 entry_count | u32 pad
#   | entries[16] × (path[64] | u64 phys | u64 len)
# 尺寸必须整表宽（16×80+16 = 1296B）；短了会让表读越未映射区——夹具保留
# 这条显式 assert（本轮曾在截断行上误推过短读假说，真凶另在probe定位）。
TABLE_BIN="$WORK/bootface_table.bin"
python3 - "$TABLE_BIN" <<'PYEOF' || fail "table blob 生成失败"
import struct, sys
magic = 0x31544f4f42584e4d  # "MNXBOOT1"
entry = 64 + 8 + 8          # BootFileEntry repr(C) 宽
blob = struct.pack("<QII", magic, 0, 0) + bytes(16 * entry)
assert len(blob) == 16 + 16 * entry, len(blob)
open(sys.argv[1], "wb").write(blob)
PYEOF

# ── Stage 4: 点火 ──
SERIAL_LOG="$WORK/kimage-bootface-serial.$RUN.log"
rm -f "$SERIAL_LOG"
pkill -f '[q]emu-system' 2>/dev/null || true
sleep 1

timeout "$TIMEOUT_BOOT" qemu-system-riscv64 \
    -machine virt -smp 1 -m 256M \
    -bios default \
    -kernel "$KERNEL" \
    -dtb "$QEMU_DTB" \
    -device "loader,addr=$TABLE_PA,file=$TABLE_BIN" \
    -serial "file:$SERIAL_LOG" \
    -display none -no-reboot &
QEMU_PID=$!
trap 'kill "$QEMU_PID" 2>/dev/null || true; wait "$QEMU_PID" 2>/dev/null || true' EXIT

for _ in $(seq 1 "$TIMEOUT_BOOT"); do
    if [ -f "$SERIAL_LOG" ] && grep -qaF "module placement failed" "$SERIAL_LOG"; then
        break
    fi
    kill -0 "$QEMU_PID" 2>/dev/null || break
    sleep 1
done
kill "$QEMU_PID" 2>/dev/null || true
wait "$QEMU_PID" 2>/dev/null || true
trap - EXIT

[ -f "$SERIAL_LOG" ] || fail "串口日志缺失：$SERIAL_LOG"

# ── Stage 5: 断言 ──
B1=0; B2=0; B3=0
ln_mem="$(grep -an "memmap regions" "$SERIAL_LOG" | head -1 | cut -d: -f1)"
ln_tab="$(grep -an "boot file table pa" "$SERIAL_LOG" | head -1 | cut -d: -f1)"
ln_miss="$(grep -an "table holds 0x0000000000000000 entries" "$SERIAL_LOG" | head -1 | cut -d: -f1)"
# B1：chosen 属性被解析出表址（行存在且地址含 TABLE_PA 低位形状）。
if [ -n "$ln_tab" ] && grep -qaF "boot file table pa 0x000000008c000000" "$SERIAL_LOG"; then
    B1=1
fi
# B2：表被消费并在第一名点名失败 + halt 臂到达。
[ -n "$ln_miss" ] && grep -qaF "module placement failed" "$SERIAL_LOG" && B2=1
# B3：顺序 memmap < table < MISSING。
if [ -n "$ln_mem" ] && [ -n "$ln_tab" ] && [ -n "$ln_miss" ] \
   && [ "$ln_tab" -gt "$ln_mem" ] && [ "$ln_miss" -gt "$ln_tab" ]; then
    B3=1
fi

echo "--- B1 DTB /chosen 表址解析（$TABLE_PA）："
[ "$B1" = 1 ] && echo "    PASS" || echo "    FAIL（未出现表址行）"
echo "--- B2 表消费 + MISSING 点名 + halt 臂："
[ "$B2" = 1 ] && echo "    PASS" || echo "    FAIL"
echo "--- B3 顺序 memmap(${ln_mem:-无}) < table(${ln_tab:-无}) < MISSING(${ln_miss:-无})："
[ "$B3" = 1 ] && echo "    PASS" || echo "    FAIL"

if [ "$B1$B2$B3" = "111" ]; then
    echo "### TEST_RESULT: PASS test-kernel-image-riscv64-bootface ###"
    exit 0
fi
echo "### TEST_RESULT: FAIL test-kernel-image-riscv64-bootface ###"
echo "--- 串口尾部 20 行："
tail -20 "$SERIAL_LOG"
exit 1
