#!/bin/bash
# wrap-rt-birth-riscv64.sh — NK4-B P4 M4.4 取证包壳（非版管测试脚本）
#
# 为什么需要它：版管脚本 `os/qemu-tests/test-rt-birth-riscv64.sh` 把串口日志
# 写到 mktemp 路径，并在 `trap cleanup EXIT` 里 `rm -f`（第 61 行），跑完无档
# 可留。本包壳不复制任何逻辑——它按行号把那两个位置改掉后直接执行，因此
# 判据（第 82-86 行 5 个 marker）、构建命令、QEMU 机器/固件/内存参数与版管
# 脚本逐字节同源，不存在手抄漂移。
#
# 与版管脚本的全部差异（三处，都不触碰判据）：
#   1. 第 44 行：日志路径由 mktemp 改为调用方给定的 RTB_LOG；
#   2. 第 61 行：cleanup 里的 `rm -f "$SERIAL_LOG"` 改为 no-op（保留证据）；
#   3. `OS_ROOT="$ROOT"` 改为仓库 `os/` 的绝对路径——包壳文件落在 tmp/nk4a
#      下，版管脚本那句按 `BASH_SOURCE` 相对定位会指错目录（第一次试跑就
#      因此报 "can't find Cargo.toml in .../tmp"）。
# 若版管脚本增删行导致行号错位，本脚本会立即报错或把日志写进仓库根目录的
# 错误位置——用前先看一眼 `grep -n` 的两行输出是否还是那两行。
#
# 用法：RTB_LOG=<绝对路径> bash tmp/nk4a/wrap-rt-birth-riscv64.sh
set -uo pipefail
REPO=/home/xzhao/github/minix-rs
SRC="$REPO/os/qemu-tests/test-rt-birth-riscv64.sh"
: "${RTB_LOG:?usage: RTB_LOG=<abs path> $0}"

# 行号自检：错位就拒绝执行，而不是静默改到别的地方
l44=$(sed -n '44p' "$SRC")
l61=$(sed -n '61p' "$SRC")
case "$l44" in 'SERIAL_LOG="$(mktemp '*) ;; *) echo "FATAL: line 44 drifted: $l44"; exit 3;; esac
case "$l61" in *'rm -f "$SERIAL_LOG"'*) ;; *) echo "FATAL: line 61 drifted: $l61"; exit 3;; esac

WRAP="$(mktemp "$REPO/tmp/nk4a/rtb64-wrap.XXXXXX.sh")"
sed -e '44s#.*#SERIAL_LOG="'"$RTB_LOG"'"#' -e '61s#.*#    : #;s#^ *$#    : #' \
    -e 's#^OS_ROOT="\$ROOT"$#OS_ROOT="'"$REPO"'/os"#' "$SRC" > "$WRAP"
bash "$WRAP"; rc=$?
rm -f "$WRAP"
echo "run exit=$rc  serial=$RTB_LOG"
exit $rc
