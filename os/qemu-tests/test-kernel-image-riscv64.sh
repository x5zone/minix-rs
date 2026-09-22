#!/bin/bash
# test-kernel-image-riscv64.sh — NK4-B P4 M4.3 的装载链载体（OpenSBI 腿）。
#
# 断言三件事，全部围绕「固件把生产内核镜像装载到位并交出去」：
#   A1 固件跳入地址 = 镜像物理基址：OpenSBI 打印的 `Domain0 Next Address`
#      必须等于 `os/kernel-image/riscv64.ld` 里的 `KERNEL_PHYS_BASE`。镜像
#      入口是高半 VA（0xFFFFFFC000000000），固件不可能直接跳它。反例实验
#      没护住的那一面也得写清：本条只反映 QEMU 约定的 next-address 与 .ld
#      声明一致，它不看工件内容（喂一份 x86_64 ELF 进来 A1 仍 PASS），所以
#      「段确实被装到 paddr」靠 A2（横幅文字来自 .rodata，装错就读不出来）
#      与静态侧 check-layout.sh 的 L3c/L4 保证，不靠 A1。
#   A2 入口确实被执行：镜像横幅（`os/kernel-image/src/main.rs` 的
#      `rust_image_main` 第一行输出）出现在串口。本条是唯一对工件敏感的断言。
#   A3 顺序：横幅必须排在 `Domain0 Next Address` 之后。没有这条，A2 可能被
#      「日志里别处的 echo」污染，不构成「负载被跳入后自己打印」的证据。
#
# 装载方式对照 `test-timer-irq-riscv64.sh`（同一条 `-bios default -kernel`）
# 与 `test-riscv64-uboot.sh` 的 `fatload → bootelf`：这里走前者，因为后者
# 需要 `mkimage` 与 U-Boot 固件 blob（本机宿主与 minix-ci 容器都没有，见
# NK4B-WORKLOG「P4 M4.1/M4.3」的上交裁决）。两条腿共同的入口约定是「跳
# 镜像物理基址的首字节」，A1 断言的就是这一条；段拷贝语义两边都由 ELF 的
# PT_LOAD paddr 驱动（静态侧由 check-layout.sh L3c/L4 钉住）。
#
# 为什么高半镜像能在分页关闭（satp=0）的情况下从物理基址跑起来：riscv64
# 默认 medany 代码模型，`la`/符号引用被链接器松弛成 PC 相对的 `auipc`+`addi`，
# 整个镜像内的引用都按实际 PC 解析（A1/A2/A3 三合一本机三轮实测成立，反向
# 实验则如预期卡在 A2/A3）。
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
MARK="### minix-rs kernel image: entry reached"
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

# 横幅出现即收；镜像停在 `_start` 的 `wfi` 循环里不会自己退出，所以必须
# 主动 kill（与 test-timer-irq-riscv64.sh 同一处理方式）。
found=0
for _ in $(seq 1 "$TIMEOUT_BOOT"); do
    if [ -f "$SERIAL_LOG" ] && grep -qaF "$MARK" "$SERIAL_LOG"; then
        found=1
        break
    fi
    kill -0 "$QEMU_PID" 2>/dev/null || break
    sleep 1
done
kill "$QEMU_PID" 2>/dev/null || true
wait "$QEMU_PID" 2>/dev/null || true
trap - EXIT

# ── Stage 4: 三条断言 ─────────────────────────────────────────────────
[ -f "$SERIAL_LOG" ] || fail "串口日志缺失：$SERIAL_LOG（QEMU 没起来？）"

A1=0; A2=0; A3=0
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

if [ "$A1$A2$A3" = "111" ]; then
    echo "### TEST_RESULT: PASS test-kernel-image-riscv64 ###"
    grep -a "$NEXT_ADDR_KEY" "$SERIAL_LOG" | head -1
    grep -aF "$MARK" "$SERIAL_LOG" | head -1
    exit 0
fi

echo "### TEST_RESULT: FAIL test-kernel-image-riscv64 ###"
echo "--- 串口尾部 20 行："
tail -20 "$SERIAL_LOG"
exit 1
