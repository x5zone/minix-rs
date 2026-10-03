#!/usr/bin/env bash
# build-atf-test.sh — NK4-C 目标③：把一个 minix3 atf-c 测试交叉编译+链接成
# **静态可加载 ELF**（riscv64 / aarch64）。③"上机跑通"的构建/链接前置；产出的
# ELF 满足我方 minix-elf 加载器的 ehdr 硬校验（ELF64 + LSB + ET_EXEC，machine
# 不校，见 06/os/libs/minix-elf parse_ehdr）。
#
# 两腿同构（均 picolibc + sys-bridge.c + posix-stubs.c 同源共享）：
#   riscv64 ：riscv64-unknown-elf-gcc + 系统 picolibc（--specs=picolibc.specs）
#   aarch64 ：aarch64-linux-gnu-gcc + picolibc-aarch64-linux-gnu（免 root，由
#             tools/vendor-atf-toolchain.sh 自动取包/本地化 specs）
#
# 依赖：tools/build-libatf-c.sh 已产出 os/target/atf/<arch>/libatf-c.a；
#       tools/atf-c-compat/{sys-bridge.c, posix-stubs.c, err.h, sys/*.h}。
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
    aarch64) CC="aarch64-linux-gnu-gcc"
             # 真机取证（§续-277 gh123/124）：Debian/Ubuntu aarch64 gcc 默认
             # 加固在本靶形双毒——① -mbranch-protection=standard 让 libgcc 的
             # init_have_lse_atomics 以 paciasp 开头：cortex-a72（ARMv8.0）无
             # PAC = 未定义指令；② 默认 stack protector 把 __stack_chk_init
             # 拖进 .init_array，其模板写走非常规路径（真机 cr2 野写 SIGSEGV）。
             # 目标机是嵌入式无加固信任面，关掉两项（非绕过检查：这些特性
             # 在 cortex-a72/picolibc 环境本就不成立）。
             MC=(-mbranch-protection=none -fno-stack-protector)
             # picolibc 来自 apt 包或 vendor（免 root）；vendor 脚本输出基目录+本地化 specs
             { read -r PICOA && read -r SPECS; } < <("$ROOT/tools/vendor-atf-toolchain.sh" aarch64) || true
             [ -n "${SPECS:-}" ] || { echo "aarch64 picolibc 未取得（见上方 vendor-atf-toolchain.sh 报错）" >&2; exit 2; }
             SYS=(--specs="$SPECS"); SEMIHOST=("$PICOA/lib/libsemihost.a") ;;
    *) echo "本脚本支持 riscv64 | aarch64（真跑另需 fork/exec/VFS 桥）" >&2; exit 2 ;;
esac
command -v "$CC" >/dev/null || { echo "缺编译器：$CC（目标③ 前置，请先装交叉工具链）" >&2; exit 2; }
[ -f "${SEMIHOST[0]}" ] || { echo "缺 ${SEMIHOST[0]}（picolibc 安装前缀随发行版不同）" >&2; exit 2; }
# P1-2 防混用：库必须是同 CC 同 libc 腿的产物（build-libatf-c.sh 写的 flavor 戳），
# 否则旧 libc 腿的 libatf-c.a 会被静默复用（FILE 布局/stdio 符号语义错）。
EXPECT_FLAVOR="$CC|picolibc"
FLAVOR="$(cat "$OUT/libatf-c.flavor" 2>/dev/null || echo MISSING)"
[ "$FLAVOR" = "$EXPECT_FLAVOR" ] || { echo "libatf-c.a flavor 不匹配：got '$FLAVOR' want '$EXPECT_FLAVOR' —— 先重跑 tools/build-libatf-c.sh $ARCH" >&2; exit 2; }

# atf-c/config-time 宏（与 build-libatf-c.sh 一致）+ _GNU_SOURCE：picolibc
# 的 BSD 扩展声明（memrchr 等）在 __GNU_VISIBLE 门内；不显宏则 t_memchr 的
# memrchr 退化成 implicit-int 声明，64 位指针返回值被截成脏高位（真机
# SIGSEGV 野写取证见 WORKLOG §续-277）。不定义则同族声明面一律保守报错。
CFGS=(-D__minix -D_GNU_SOURCE -DHAVE_SETENV -DHAVE_UNSETENV -DHAVE_PUTENV
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
    -I"$SRC" -I"$BUILD" -I"$COMPAT" -include "$COMPAT/errno-compat.h" "${EXTRA_INC[@]}" "${CFGS[@]}" \
    "$TEST" -o "$BUILD/$name.o"
"$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "$COMPAT/sys-bridge.c" -o "$BUILD/sys-bridge.o"
"$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "${CFGS[@]}" "$COMPAT/posix-stubs.c" -o "$BUILD/posix-stubs.o"
# 把 picolibc semihost stdio 钩子改接我方 _write/SYS_DIAGCTL（两腿共享；抢定义
# sys_semihost_putc/getc，须排在 libsemihost.a 之前才能挡住 semihost 实现成员）。
"$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "$COMPAT/stdio-minix.c" -o "$BUILD/stdio-minix.o"
# BSD <md5.h> 摘要实现（picolibc 不提供），供 t_memcpy 等测试链入（仅构建/测试用）。
"$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "$COMPAT/md5.c" -o "$BUILD/md5.o"
# BSD Boyer-Moore <bm.h>（bm_comp/exec/free，picolibc 不提供），供 t_bm 链入。
"$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "$COMPAT/bm.c" -o "$BUILD/bm.o"
# BSD sys_nerr（picolibc 不提供），供 t_strerror 编译/链接（真语义上机前校准，见 errno-compat.c 头注释）。
"$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "$COMPAT/errno-compat.c" -o "$BUILD/errno-compat.o"
# BSD random/srandom 真源接管（NK4-C §续-283 D1）：minix3/common/lib/libc/stdlib/
# random.c 逐字 vendor（fidelity=零改动，全部兼容在 random-shim/）。命令行对象
# 强定义 random+srandom（+initstate/setstate）→ picolibc 单符号成员
# libc_stdlib_{random,srandom}.c.o 不再被拉入（LCG _rand_next 形永不上台），
# t_memcpy 的 BSD 期望序由此满足——host 离线对账 MD5=7b405d24… 已 MATCH（§续-283）。
# sbrk/sysconf/strerror 同形先例。
"$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" -I"$COMPAT/random-shim" "$COMPAT/random.c" -o "$BUILD/random.o"

# 上机出生链（启动腿）：picolibc 默认 crt0 把 sp 切进 ELF 内 .stack 且
# main(0,NULL)（atf tp_main 解引用 argv[0] 必死），需 -nostartfiles 换
# startup-minix.c 的自定义 _start（从内核 ps_strings 取 argc/argv，对位
# minix-rt 出生 ABI）。当前仅 aarch64 腿启用（上机目标）；riscv 腿沿用
# 默认 crt0（其出生链上机受 (A) 门控，解锁后一并切换）。
STARTUP_O=()
LINK_EXTRA=()
case "$ARCH" in
    aarch64)
        "$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "$COMPAT/startup-minix.c" -o "$BUILD/startup-minix.o"
        STARTUP_O=("$BUILD/startup-minix.o")
        LINK_EXTRA=(-nostartfiles) ;;
esac

"$CC" -static "${MC[@]}" "${SYS[@]}" "${LINK_EXTRA[@]}" "${STARTUP_O[@]}" "$BUILD/$name.o" \
    -Wl,--start-group "$LIB" "$BUILD/sys-bridge.o" "$BUILD/stdio-minix.o" "$BUILD/posix-stubs.o" "$BUILD/md5.o" "$BUILD/bm.o" "$BUILD/errno-compat.o" "$BUILD/random.o" "${SEMIHOST[@]}" -lc -Wl,--end-group \
    -o "$BUILD/$name"

# 校验我方加载器 parse_ehdr 的硬项（ELF64 + LSB + ET_EXEC）。
RE="$("${CC%-gcc}-readelf" -h "$BUILD/$name")"
echo "$RE" | grep -q 'Class:.*ELF64'   || { echo "非 ELF64" >&2; exit 1; }
echo "$RE" | grep -qi 'little endian'   || { echo "非 LSB" >&2; exit 1; }
echo "$RE" | grep -q 'Type:.*EXEC'     || { echo "非 ET_EXEC" >&2; exit 1; }
echo "✅ $BUILD/$name：静态 ET_EXEC $ARCH ELF，过 minix-elf ehdr 硬校验（$(stat -c%s "$BUILD/$name")B）"
