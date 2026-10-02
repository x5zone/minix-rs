#!/usr/bin/env bash
# build-libatf-c.sh — NK4-C 目标③：交叉构建 NetBSD ATF 的 C 库（libatf-c.a）。
#
# 目标③（minix3 的 586 个 C 测试上机）的公共依赖：所有 atf-c 测试程序链接
# libatf-c。本脚本把 `minix3/external/bsd/atf/dist/atf-c` 的全部库源（顶层 7 个
# + detail/ 子树 10 个）用交叉工具链 + picolibc 编成静态库，产出到 os/target/atf/<arch>/。
#
# 这是 boot-independent 的构建期产物（不参与 minix-rs 生产镜像）。库已**完整**：
# 链一个真实 atf 测试已无 atf_* 未定符号，仅剩 POSIX syscall/stdio（_exit/open/
# close/geteuid/getgroups/stdout/stderr）——即③的「C→我方 kernel_call/VFS syscall 桥」
# 与上机那一段（见 WORKLOG §续-224 组件清单）。
#
# 关键移植点（对照 picolibc 缺件，均在 tools/atf-c-compat/ 补齐）：
#   - defs.h 由 defs.h.in 渲染（三个 @ATTRIBUTE_*@ 宏 → GCC __attribute__）。
#   - atf-c/tc.c 需 <sys/uio.h>（struct iovec）—— picolibc 缺，用 compat 头（仅 riscv64 注入；
#     x86_64 走 glibc 原生，不注入以免覆盖）。
#   - atf-c 源是 Minix 补丁化的：`#if defined(__minix)` 块须 -D__minix 才闭合
#     （否则 check.c 的 `atf_error_t err` 声明被跳过、后续引用它报 undeclared）。
#   - config.c 需要 11 个 config-time 宏（ATF_BUILD_* 与 ATF_*DIR/SHELL/WORKDIR），
#     正常由 atf 的 configure 注入；这里以 -D 提供占位值（构建期不影响测试语义）。
#
# 用法：tools/build-libatf-c.sh [arch]   # arch=riscv64（默认）| aarch64 | x86_64
# 依赖：riscv64-unknown-elf-gcc + picolibc（riscv64）；aarch64 用 aarch64-linux-gnu-gcc
#       （glibc 自带 uio/err）；x86_64 用 host gcc（libc 自带 uio/err）。
# 注：目标③ 上机首选 aarch64（目标① aarch64 已 ✅、可 boot+exec；riscv 上机受 (A) 门控）。
set -euo pipefail

ARCH="${1:-riscv64}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # …/minix-rs
SRC="$ROOT/minix3/external/bsd/atf/dist"
COMPAT="$ROOT/tools/atf-c-compat"
OUT="$ROOT/os/target/atf/$ARCH"
BUILD="$OUT/build"

case "$ARCH" in
    riscv64) CC="riscv64-unknown-elf-gcc"; AR="riscv64-unknown-elf-ar"; \
             SYSROOT_FLAGS=(--specs=picolibc.specs); COMPAT_INC=(-I"$COMPAT"); \
             MCFLAGS=(-mcmodel=medany) ;;   # riscv medlow 默认 ±2GiB 假设→高地址链接重定位截断，需 medany
    aarch64) CC="aarch64-linux-gnu-gcc"; AR="aarch64-linux-gnu-ar"; \
             SYSROOT_FLAGS=(); COMPAT_INC=(); MCFLAGS=() ;;   # glibc 自带 <err.h>/<sys/uio.h>，不注入 compat
    x86_64)  CC="cc"; AR="ar"; SYSROOT_FLAGS=(); \
             COMPAT_INC=(); MCFLAGS=() ;;   # x86_64 用 glibc 原生 <err.h>/<sys/uio.h>，不注入 compat（否则覆盖原生头致冲突）
    *) echo "未知 arch：$ARCH（支持 riscv64 | aarch64 | x86_64）" >&2; exit 2 ;;
esac

command -v "$CC" >/dev/null || { echo "缺少编译器：$CC（目标③ 前置，请先装交叉工具链）" >&2; exit 2; }
command -v "$AR" >/dev/null || { echo "缺少归档器：$AR（目标③ 前置，请先装交叉工具链）" >&2; exit 2; }

rm -rf "$BUILD"; mkdir -p "$BUILD/atf-c" "$OUT"

# 渲染 defs.h：三个 @ATTRIBUTE_*@ 占位符 → GCC 属性。FORMAT_PRINTF 保留 (a,b) 参数位。
sed -e 's/@ATTRIBUTE_FORMAT_PRINTF@/__attribute__((format(printf, a, b)))/' \
    -e 's/@ATTRIBUTE_NORETURN@/__attribute__((__noreturn__))/' \
    -e 's/@ATTRIBUTE_UNUSED@/__attribute__((__unused__))/' \
    "$SRC/atf-c/defs.h.in" > "$BUILD/atf-c/defs.h"

# config-time 宏（占位值；构建期不影响测试语义）+ picolibc 能力探测宏。
# HAVE_SETENV/UNSETENV/PUTENV：newlib/picolibc 提供 setenv → 让 detail/env.c 闭合。
# PACKAGE_*：autoconf 包宏（detail/sanity.c 的断言消息用）。
CFGS=(
    -D__minix -DHAVE_SETENV -DHAVE_UNSETENV -DHAVE_PUTENV
    '-DPACKAGE_NAME="libatf-c"'
    '-DPACKAGE_TARNAME="atf"'
    '-DPACKAGE_VERSION="0.6"'
    '-DPACKAGE_STRING="libatf-c 0.6"'
    '-DPACKAGE_BUGREPORT=""'
    '-DATF_BUILD_CC="cc"'
    '-DATF_BUILD_CFLAGS=""'
    '-DATF_BUILD_CPP="cpp"'
    '-DATF_BUILD_CPPFLAGS=""'
    '-DATF_BUILD_CXX="c++"'
    '-DATF_BUILD_CXXFLAGS=""'
    '-DATF_INCLUDEDIR="/usr/include/atf"'
    '-DATF_LIBEXECDIR="/usr/libexec/atf"'
    '-DATF_PKGDATADIR="/usr/share/atf"'
    '-DATF_SHELL="/bin/sh"'
    '-DATF_WORKDIR="/tmp"'
)

# 库源：atf-c 顶层 7 个 + atf-c/detail/ 子树 10 个（排除 *_helpers.c/test_helpers.c，
# 那是库自测用）。漏 detail/ 会使 libatf-c.a 不完整（atf_list_* 等缺失）。
LIB_SRCS=(
    error build check config tc tp utils
    detail/dynstr detail/env detail/fs detail/list detail/map
    detail/process detail/sanity detail/text detail/tp_main detail/user
)

OBJS=()
for f in "${LIB_SRCS[@]}"; do
    echo "  CC  atf-c/$f.c"
    ob="$(echo "$f" | tr '/' '_')"
    "$CC" -c -Os "${MCFLAGS[@]}" "${SYSROOT_FLAGS[@]}" \
        -I"$SRC" -I"$BUILD" -I"$SRC/atf-c" "${COMPAT_INC[@]}" \
        "${CFGS[@]}" \
        "$SRC/atf-c/$f.c" -o "$BUILD/$ob.o"
    OBJS+=("$BUILD/$ob.o")
done

"$AR" rcs "$OUT/libatf-c.a" "${OBJS[@]}"
echo "✅ 产出 $OUT/libatf-c.a：$($AR t "$OUT/libatf-c.a" | wc -l) 个成员，$(stat -c%s "$OUT/libatf-c.a") 字节"
