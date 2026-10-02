#!/usr/bin/env bash
# vendor-atf-toolchain.sh — NK4-C 目标③：确保 aarch64 的 picolibc 可用并产出
# **本地化 specs**（免 root）。
#
# 背景：riscv 腿用系统安装的 picolibc（/usr/lib/picolibc/riscv64-unknown-elf，
# gcc 驱动原生搜索）。aarch64 腿的编译器是发行版的 aarch64-linux-gnu-gcc
# （vanilla 驱动，不认 picolibc 定制的 -picolibc-prefix= 选项），其 specs 若
# 来自 deb 则硬编码绝对路径 /usr/lib/picolibc/aarch64-linux-gnu。两种来源：
#   1. 系统已装 apt 包 picolibc-aarch64-linux-gnu → 直接用其绝对路径；
#   2. 未装（无 sudo 的无人值守环境）→ `apt-get download` + `dpkg -x` 解到
#      tools/vendor/（git 不跟踪），再 sed 把 specs 里的绝对路径改写为 vendor
#      路径。两条来源统一产出一份可直接传给 gcc 的本地化 specs。
#
# 幂等：重复执行只是重下载/重改写（产物覆盖），无副作用累积。
# 用法：tools/vendor-atf-toolchain.sh [arch=aarch64]   # 输出两行：picolibc 基目录、本地化 specs 路径
# 注：apt-get 只取包不解包不改系统状态；需网络与 apt 索引（新机器先 apt-get update）。
set -euo pipefail
ARCH="${1:-aarch64}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

case "$ARCH" in
    aarch64) TRIPLE="aarch64-linux-gnu" ;;
    *) echo "本脚本仅服务 aarch64 腿（riscv64 用系统 picolibc，见 build-atf-test.sh）" >&2; exit 2 ;;
esac

PKG="picolibc-$TRIPLE"
SYS_DIR="/usr/lib/picolibc/$TRIPLE"
VENDOR="$ROOT/tools/vendor/usr/lib/picolibc/$TRIPLE"
OUT_DIR="$ROOT/os/target/atf/$ARCH"
SPECS="$OUT_DIR/picolibc-$TRIPLE.specs"

# 选库来源：系统安装优先（路径即绝对，specs 无需改写内容等价），否则 vendor。
if [ -d "$SYS_DIR/lib" ]; then
    LIB_DIR="$SYS_DIR"
elif [ -d "$VENDOR/lib" ]; then
    LIB_DIR="$VENDOR"
else
    mkdir -p "$ROOT/tools/vendor"
    tmp="$(mktemp -d)"
    ( cd "$tmp" && apt-get download "$PKG" >/dev/null 2>&1 \
        && dpkg -x "$PKG"*.deb "$ROOT/tools/vendor/" ) || {
        rm -rf "$tmp"
        echo "取 $PKG 失败：装 apt 包（sudo apt-get install -y $PKG）或确保网络可用后重试" >&2
        exit 2
    }
    rm -rf "$tmp"
    LIB_DIR="$VENDOR"
fi
[ -d "$LIB_DIR/lib" ] || { echo "picolibc 布局不完整：缺 $LIB_DIR/lib" >&2; exit 2; }

# 本地化：specs 里所有 /usr/lib/picolibc/<triple> 引用改写成实际来源路径，
# 产出物不再依赖 gcc 驱动的搜索逻辑，可直接 --specs=<abs>。
mkdir -p "$OUT_DIR"
sed "s|/usr/lib/picolibc/$TRIPLE|$LIB_DIR|g" "$LIB_DIR/picolibc.specs" > "$SPECS"
echo "$LIB_DIR"
echo "$SPECS"
