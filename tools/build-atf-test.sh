#!/usr/bin/env bash
# build-atf-test.sh — NK4-C 目标③：把一个 minix3 atf-c 测试交叉编译+链接成
# **静态可加载 ELF**（riscv64）。③"上机跑通"的构建/链接前置；产出的 ELF 满足
# 我方 minix-elf 加载器的 ehdr 硬校验（ELF64 + LSB + ET_EXEC，machine 不校，
# 见 06/os/libs/minix-elf parse_ehdr）。
#
# 依赖：tools/build-libatf-c.sh 已产出 os/target/atf/<arch>/libatf-c.a；
#       tools/atf-c-compat/{sys-riscv.c, posix-stubs-riscv.c, err.h, sys/*.h}。
# 诚实边界（WIP，见 WORKLOG §续-230）：链成 ELF ≠ 上机跑通。真跑还需把
#   fork/waitpid/exec/open/read/write 接我方 SYS_*/VFS 桥 + 测试入 imgrd、rc exec。
#
# 用法：tools/build-atf-test.sh <test.c 路径> [arch=riscv64] [额外编译 flags（含 -I）]
set -euo pipefail
TEST="${1:?用法: build-atf-test.sh <test.c> [arch] [编译flags...]}"; shift
# arch 仅在 $1 是已知值时消费，否则默认 riscv64、把 $@ 全当额外 flags。
case "${1:-}" in
    riscv64|aarch64|x86_64) ARCH="$1"; shift ;;
    *) ARCH="riscv64" ;;
esac
EXTRA_INC=("$@")   # 剩余的当额外编译 flags（如 -I<dir>）

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$ROOT/minix3/external/bsd/atf/dist"
COMPAT="$ROOT/tools/atf-c-compat"
OUT="$ROOT/os/target/atf/$ARCH"
LIB="$OUT/libatf-c.a"
BUILD="$OUT/tests"
PICO=/usr/lib/picolibc/riscv64-unknown-elf

[ -f "$LIB" ] || { echo "缺 $LIB —— 先跑 tools/build-libatf-c.sh $ARCH" >&2; exit 2; }
case "$ARCH" in
    riscv64) CC="riscv64-unknown-elf-gcc"; MC=(-mcmodel=medany)
             SYS=(--specs=picolibc.specs); SEMIHOST=("$PICO/lib/libsemihost.a") ;;
    *) echo "本脚本首片仅支持 riscv64（真跑另需 fork/exec/VFS 桥）" >&2; exit 2 ;;
esac
command -v "$CC" >/dev/null || { echo "缺编译器：$CC（目标③ 前置，请先装交叉工具链）" >&2; exit 2; }
[ -f "${SEMIHOST[0]}" ] || { echo "缺 ${SEMIHOST[0]}（picolibc 安装前缀随发行版不同）" >&2; exit 2; }

# atf-c/config-time 宏（与 build-libatf-c.sh 一致）。
CFGS=(-D__minix -DHAVE_SETENV -DHAVE_UNSETENV -DHAVE_PUTENV
      '-DPACKAGE_NAME="libatf-c"' '-DPACKAGE_TARNAME="atf"' '-DPACKAGE_VERSION="0.6"'
      '-DPACKAGE_STRING="libatf-c 0.6"' '-DPACKAGE_BUGREPORT=""'
      '-DATF_BUILD_CC="cc"' '-DATF_BUILD_CFLAGS=""' '-DATF_BUILD_CPP="cpp"'
      '-DATF_BUILD_CPPFLAGS=""' '-DATF_BUILD_CXX="c++"' '-DATF_BUILD_CXXFLAGS=""'
      '-DATF_INCLUDEDIR="/usr/include/atf"' '-DATF_LIBEXECDIR="/usr/libexec/atf"'
      '-DATF_PKGDATADIR="/usr/share/atf"' '-DATF_SHELL="/bin/sh"' '-DATF_WORKDIR="/tmp"'
      '-D__arraycount(__x)=(sizeof(__x)/sizeof((__x)[0]))')

mkdir -p "$BUILD"
# 渲染 atf-c/defs.h（测试经 <atf-c/defs.h> 包含，与 build-libatf-c.sh 同源）。
mkdir -p "$BUILD/atf-c"
sed -e 's/@ATTRIBUTE_FORMAT_PRINTF@/__attribute__((format(printf, a, b)))/' \
    -e 's/@ATTRIBUTE_NORETURN@/__attribute__((__noreturn__))/' \
    -e 's/@ATTRIBUTE_UNUSED@/__attribute__((__unused__))/' \
    "$SRC/atf-c/defs.h.in" > "$BUILD/atf-c/defs.h"
name="$(basename "$TEST" .c)"
"$CC" -c -Os "${MC[@]}" "${SYS[@]}" \
    -I"$SRC" -I"$BUILD" -I"$COMPAT" "${EXTRA_INC[@]}" "${CFGS[@]}" \
    "$TEST" -o "$BUILD/$name.o"
"$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "$COMPAT/sys-riscv.c" -o "$BUILD/sys-riscv.o"
"$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "${CFGS[@]}" "$COMPAT/posix-stubs-riscv.c" -o "$BUILD/posix-stubs.o"
# BSD <md5.h> 摘要实现（picolibc 不提供），供 t_memcpy 等测试链入（仅构建/测试用）。
"$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "$COMPAT/md5.c" -o "$BUILD/md5.o"

"$CC" -static "${MC[@]}" "${SYS[@]}" "$BUILD/$name.o" \
    -Wl,--start-group "$LIB" "$BUILD/sys-riscv.o" "$BUILD/posix-stubs.o" "$BUILD/md5.o" "${SEMIHOST[@]}" -lc -Wl,--end-group \
    -o "$BUILD/$name"

# 校验我方加载器 parse_ehdr 的硬项（ELF64 + LSB + ET_EXEC）。
RE="$(riscv64-unknown-elf-readelf -h "$BUILD/$name")"
echo "$RE" | grep -q 'Class:.*ELF64'   || { echo "非 ELF64" >&2; exit 1; }
echo "$RE" | grep -qi 'little endian'   || { echo "非 LSB" >&2; exit 1; }
echo "$RE" | grep -q 'Type:.*EXEC'     || { echo "非 ET_EXEC" >&2; exit 1; }
echo "✅ $BUILD/$name：静态 ET_EXEC riscv64 ELF，过 minix-elf ehdr 硬校验（$(stat -c%s "$BUILD/$name")B）"
