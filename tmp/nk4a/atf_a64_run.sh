#!/usr/bin/env bash
# atf_a64_run.sh — 目标③ aarch64 上机 harness（诊断载体，不入生产）：
# 装配镜像 → qemu boot → 等串口出现 ATF 判定行或超时 → 打印证据。
# 用法：bash tmp/nk4a/atf_a64_run.sh <tag> [wait_secs]
set -uo pipefail
TAG="${1:?tag}"; WAIT="${2:-150}"
ROOT=/home/xzhao/github/minix-rs
WORK="$ROOT/tmp/nk4a/atf-$TAG"
rm -rf "$WORK"; mkdir -p "$WORK"
cp /usr/share/AAVMF/AAVMF_VARS.fd "$WORK/vars.fd"
( cd "$ROOT/os" && cargo run -q -p xtask -- image --arch aarch64 --release ) >"$WORK/xtask.log" 2>&1 || { echo "xtask FAIL"; tail -5 "$WORK/xtask.log"; exit 1; }
setsid qemu-system-aarch64 \
  -machine virt,gic-version=3 -cpu cortex-a72 -smp 4 -m 512M \
  -drive "if=pflash,format=raw,unit=0,file=/usr/share/AAVMF/AAVMF_CODE.fd,readonly=on" \
  -drive "if=pflash,format=raw,unit=1,file=$WORK/vars.fd" \
  -drive "file=$ROOT/os/target/image/aarch64/minix.img,format=raw,media=disk" \
  -serial "file:$WORK/serial.log" -net none -display none -no-reboot \
  >"$WORK/qemu.log" 2>&1 &
QPID=$!
sleep "$WAIT"
# 定点杀（CodeReview 续-277b P2-5：禁裸 pkill，避免误伤并行任务的 qemu）
kill "$QPID" 2>/dev/null || true
wait "$QPID" 2>/dev/null || true
echo "=== ATF evidence ==="
strings "$WORK/serial.log" | grep -nE "atf|Pass|Fail|ERROR|memchr|warning|unsupported|PANIC" | grep -v "cat /etc/rc" | head -20
echo "=== tail (non-probe) ==="
strings "$WORK/serial.log" | grep -vE "^nk4a|^vr:|^pm:" | tail -6
