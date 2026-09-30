#!/bin/bash
# test-kernel-image-riscv64.sh — NK4-B P4 M4.3 的装载链载体（OpenSBI 腿）。
#
# 断言三件事，全部围绕「固件把生产内核镜像装载到位并交出去」：
#   A1 固件跳入地址 = 镜像物理基址：OpenSBI 打印的 `Domain0 Next Address`
#      必须等于 `os/kernel-image/riscv64.ld` 里的 `KERNEL_PHYS_BASE`。镜像
#      入口是高半 VA（0xFFFFFFC000000000），固件不可能直接跳它。
#      本条的机制已用实验钉住（不是推测）：把成品镜像的四条 PT_LOAD
#      `p_paddr` 整体 +2MiB 后重跑，固件就改打印 `0x80400000` 且横幅照打
#      ——即 next-address 取自**工件自己的最低装载物理址**。但它对「被拒装
#      的工件」会假 PASS：喂 x86_64 ELF 时 QEMU 装不进回退到平台约定址
#      `0x80200000`，恰与 `.ld` 同值。所以 A1 只护「产物最低装载址 与
#      `.ld` 的 KERNEL_PHYS_BASE 不互相漂」（VIRT 侧由静态层 L0/L3b/L4 护），
#      不能单拿它当「工件正确」的证据；那一格靠 A2。
#   A2 入口确实被执行：镜像横幅（`os/kernel-image/src/main.rs` 的
#      `rust_image_main` 第一行输出）出现在串口。工件被拒装时它是唯一会
#      当场红的断言（横幅文字取自 `.rodata`，段没装对就读不出来）。
#   A3 顺序：横幅必须排在 `Domain0 Next Address` 之后。没有这条，A2 可能被
#      「日志里别处的 echo」污染，不构成「负载被跳入后自己打印」的证据。
#
# 装载方式对照 `test-timer-irq-riscv64.sh`（同一条 `-bios default -kernel`）。
# 本脚本只证 OpenSBI 腿。另一条对照腿 `test-riscv64-uboot.sh` 本轮**未跑**
# （缺 `mkimage` 与 U-Boot blob，见 NK4B-WORKLOG「P4 M4.1/M4.3」的上交裁决），
# 而且它的语义与本腿不同：`bootelf` 按 ELF 的 **e_entry** 跳，而本镜像的
# e_entry 是高半 VA（satp=0 下直跳即陷）。也就是说 U-Boot 腿真接的时候可能
# 需要 entry 侧适配，那是装载链的待裁决项，本脚本不替它下结论。
# 为什么高半镜像能在分页关闭（satp=0）的情况下从物理基址跑起来：riscv64
# 默认 medany 代码模型，`la`/符号引用被链接器松弛成 PC 相对的 `auipc`+`addi`，
# 整个镜像内的引用都按实际 PC 解析（A1/A2/A3 三合一本机四轮实测成立，且
# paddr 整体搬移实验下横幅仍照打——进一步佐证执行视图不依赖绝对地址；
# 喂 x86_64 工件的反例则如预期卡在 A2/A3）。
#
# 本脚本**不注册进 run_all.sh**（按 NK4B-TODO §6，接线归 P6）。
# 临时文件落在 target 树内，不用 `mktemp`（它会硬写 /tmp，受限沙箱下报假
# 失败——NK4-B P3 M3.2 记过，P4 M4.3 复现于本目录）。
#
# 用法：bash os/qemu-tests/test-kernel-image-riscv64.sh [ELF]
#       RUN=<轮次名> 指定串口日志名；SKIP_BUILD=1 复用已有工件；
#       TIMEOUT_BOOT=<秒> 指定超时（默认 60）。反向判别用 KERNEL/位置参数
#       喂一份非 riscv64 工件，预期 A2/A3 FAIL。
# Exit codes: 0 = PASS（三条都在）, 1 = FAIL, 2 = SKIP（前置条件缺失）。

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs/os
TRIPLE="riscv64gc-unknown-none-elf"
FEATURE="fw-riscv64-none"
LD="$ROOT/kernel-image/riscv64.ld"
KERNEL="${1:-$ROOT/target/$TRIPLE/release/kernel}"
RUN="${RUN:-m43a}"
TIMEOUT_BOOT="${TIMEOUT_BOOT:-60}"
MARK="### minix-rs kernel image: riscv64 self-boot (NK4-C jia-an)"
# NK4-C 续-88 甲案 A4：无 BootFileTable（QEMU 默认 DTB 不带 chosen 属性，
# a0=hartid 过不了 magic 闸）时 bootface 必须走到诚实停机臂——这条在
# 接线前不存在（旧入口只有横幅），接线后若 bootface 在 memmap/闸门之前
# 崩溃也会红，所以对「甲案入口腿完整跑通」有判别力。
MARK_HALT="module channel not wired, halting"
NEXT_ADDR_KEY="Domain0 Next Address"

skip() { echo "SKIP: $1"; exit 2; }
fail() { echo "FAIL: $1"; exit 1; }

command -v qemu-system-riscv64 &>/dev/null || skip "qemu-system-riscv64 not found"

# ── Stage 1: 产出镜像（与 check-layout.sh riscv64 同一条构建命令）──────
WORK="$ROOT/target/$TRIPLE"
mkdir -p "$WORK" || fail "cannot create $WORK"
if [ "${SKIP_BUILD:-0}" != "1" ]; then
    echo "building kernel-image ($TRIPLE + $FEATURE)…"
    BLOG="$WORK/kimage-riscv64-build.$RUN.log"
    ( cd "$ROOT" && (ulimit -v 3145728; \
        cargo build -q -p kernel-image --target "$TRIPLE" \
            --features "$FEATURE" --release) ) > "$BLOG" 2>&1 \
        || { echo "FAIL: kernel-image build failed (log: $BLOG)"; tail -30 "$BLOG"; exit 1; }
    rm -f "$BLOG"
fi
[ -f "$KERNEL" ] || skip "kernel ELF not found: $KERNEL（先跑 check-layout.sh riscv64 或去掉 SKIP_BUILD）"

# ── Stage 2: 从链接脚本取预期物理基址（不硬写第二份，防漂移）──────────
[ -f "$LD" ] || fail "linker script missing: $LD"
PHYS_HEX="$(sed -n 's/^[[:space:]]*KERNEL_PHYS_BASE[[:space:]]*=[[:space:]]*0x\([0-9a-fA-F]*\);.*/\1/p' "$LD" | tr 'A-F' 'a-f')"
[ -n "$PHYS_HEX" ] || fail "KERNEL_PHYS_BASE 在 $LD 里不可辨（sed 未命中）"
PHYS_WANT="$(printf '0x%016x' "0x$PHYS_HEX")"
echo "expected next-address (from riscv64.ld): $PHYS_WANT"

# ── Stage 3: OpenSBI 腿点火，等横幅 ────────────────────────────────────
SERIAL_LOG="$WORK/kimage-riscv64-serial.$RUN.log"
rm -f "$SERIAL_LOG"

pkill -f '[q]emu-system' 2>/dev/null || true
sleep 1

timeout "$TIMEOUT_BOOT" qemu-system-riscv64 \
    -machine virt \
    -smp 1 \
    -m 256M \
    -bios default \
    -kernel "$KERNEL" \
    -serial "file:$SERIAL_LOG" \
    -display none \
    -no-reboot &
QEMU_PID=$!

cleanup() {
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
}
trap cleanup EXIT

# 停机臂末条出现即收（轮询最后一条 marker `MARK_HALT`而非首条横幅，
# 避免 kill 抢在后续行落盘前造成截断竞态）；镜像停在 `_start` 的 `wfi` 循环里不会自己退出，所以必须
# 主动 kill（与 test-timer-irq-riscv64.sh 同一处理方式）。
# `qemu_died_early` 只用于失败时多给一句区分（「QEMU 提前退出」与
# 「跑到了超时但横幅没出现」是两种不同的坏），不参与任何断言判定。
qemu_died_early=0
for _ in $(seq 1 "$TIMEOUT_BOOT"); do
    if [ -f "$SERIAL_LOG" ] && grep -qaF "$MARK_HALT" "$SERIAL_LOG"; then
        break
    fi
    kill -0 "$QEMU_PID" 2>/dev/null || { qemu_died_early=1; break; }
    sleep 1
done
kill "$QEMU_PID" 2>/dev/null || true
wait "$QEMU_PID" 2>/dev/null || true
trap - EXIT

# ── Stage 4: 四条断言 ─────────────────────────────────────────────────
[ -f "$SERIAL_LOG" ] || fail "串口日志缺失：$SERIAL_LOG（QEMU 没起来？）"

A1=0; A2=0; A3=0; A4=0
# OpenSBI 的串口输出是 CRLF（写本脚本前的探索性跑用 cat -A 实测行尾 ^M），
# 取地址必须删 \r，否则字符串比较因尾随 \r 永远不等。
NEXT_ADDR_GOT="$(grep -a "$NEXT_ADDR_KEY" "$SERIAL_LOG" | head -1 | sed 's/.*: *//' | tr -d '\r')"
[ "$NEXT_ADDR_GOT" = "$PHYS_WANT" ] && A1=1
grep -qaF "$MARK" "$SERIAL_LOG" && A2=1
ln_addr="$(grep -an "$NEXT_ADDR_KEY" "$SERIAL_LOG" | head -1 | cut -d: -f1)"
ln_mark="$(grep -anF "$MARK" "$SERIAL_LOG" | head -1 | cut -d: -f1)"
if [ -n "$ln_addr" ] && [ -n "$ln_mark" ] && [ "$ln_mark" -gt "$ln_addr" ]; then
    A3=1
fi

echo "--- A1 固件跳入地址 = .ld 的 KERNEL_PHYS_BASE："
if [ "$A1" = 1 ]; then echo "    PASS（$NEXT_ADDR_GOT）"
else echo "    FAIL（期望 $PHYS_WANT，实得 '${NEXT_ADDR_GOT:-未出现}'）"; fi
echo "--- A2 镜像入口横幅出现："
[ "$A2" = 1 ] && echo "    PASS" || echo "    FAIL（未出现 '$MARK'）"
echo "--- A3 横幅排在 Next Address 之后（行号 ${ln_mark:-无} > ${ln_addr:-无}）："
[ "$A3" = 1 ] && echo "    PASS" || echo "    FAIL"
echo "--- A4 无表时 bootface 诚实停机臂出现（module channel not wired）："
grep -qaF "$MARK_HALT" "$SERIAL_LOG" && A4=1
[ "$A4" = 1 ] && echo "    PASS" || echo "    FAIL（未出现 '$MARK_HALT'）"

if [ "$A1$A2$A3$A4" = "1111" ]; then
    echo "### TEST_RESULT: PASS test-kernel-image-riscv64 ###"
    grep -a "$NEXT_ADDR_KEY" "$SERIAL_LOG" | head -1
    grep -aF "$MARK" "$SERIAL_LOG" | head -1
    exit 0
fi

echo "### TEST_RESULT: FAIL test-kernel-image-riscv64 ###"
# 提示分三档，不能只按 qemu_died_early 二分：反例实验里出现过「横幅已出现、
# 坏在 A1」的形态，那时若说「横幅未出现」就是假提示。
if [ "$A2" = 1 ]; then
    echo "--- 成因提示：入口横幅已出现，坏在地址/顺序对账（A1/A3）"
elif [ "$qemu_died_early" = 1 ]; then
    echo "--- 成因提示：QEMU 在横幅出现前就退出了（看下面尾部的报错）"
else
    echo "--- 成因提示：QEMU 跑到超时，横幅未出现"
fi
echo "--- 串口尾部 20 行："
tail -20 "$SERIAL_LOG"
exit 1
