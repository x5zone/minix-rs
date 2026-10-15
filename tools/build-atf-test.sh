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
NETBSD_ID_MACROS=()     # 只有 glibc 腿需要补 NetBSD 标识串宏（见下面 x86_64 分支）
LIBC_KIND=picolibc      # 两支交叉腿共用 picolibc + 本仓 BSD 接管面
COMPAT_TAKEOVER=1       # 是否挂 posix-stubs / stdio-minix / sys-bridge 那层 picolibc 专用接管（BSD 补位件不受这个开关管）
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
    x86_64) # 宿主 cc + glibc，与 `tools/build-libatf-c.sh:50-51` 同一条腿：发行版没有
            # x86_64 的 `-unknown-elf` 裸机包，宿主就是它的裸机腿。glibc 原生有
            # <err.h>/<sys/uio.h>/sysconf/strerror_r(GNU 形)/random，再注入本仓那层
            # picolibc 专用接管面会与 glibc 声明冲突（旧判据：`errno-compat.h` 的
            # `extern const int sys_nerr;` 对 glibc 的 `int _sys_nerr` 类型不符；该名字现已
            # 拆给 bsd-strerr.h，三腿共用），故整层接管面一律不挂，与库腿 `COMPAT_INC=()`
            # 同一个判断。`md5.h`/`bm.h` glibc 没有，仍走 -I"$COMPAT"。
            CC="cc"; MC=(); SYS=(); SEMIHOST=(); LIBC_KIND=glibc; COMPAT_TAKEOVER=0
            # 上游测试源文件在文件作用域写 NetBSD 的标识串宏（`t_bm.c:32:__RCSID(...)`），
            # picolibc 的 <sys/cdefs.h> 给得出，glibc 给不出——报
            # 「expected declaration specifiers or '...' before string constant」。这里把它
            # 定义成**空展开**（`= ` 结尾，不能省：`-D__RCSID(x)` 会展开成 1 而留下 `1("..")`），
            # 与批次二百五十四那个三行 shim 同形。只挂 glibc 腿：picolibc 腿用真宏，
            # 覆盖它会改掉交叉产物的标识串段。
            NETBSD_ID_MACROS=(-D'__RCSID(x)=' -D'__COPYRIGHT(x)=') ;;
    *) echo "本脚本支持 riscv64 | aarch64 | x86_64（真跑另需 fork/exec/VFS 桥）" >&2; exit 2 ;;
esac
command -v "$CC" >/dev/null || { echo "缺编译器：$CC（目标③ 前置，请先装交叉工具链）" >&2; exit 2; }
if [ "$COMPAT_TAKEOVER" = 1 ]; then
    [ -f "${SEMIHOST[0]}" ] || { echo "缺 ${SEMIHOST[0]}（picolibc 安装前缀随发行版不同）" >&2; exit 2; }
fi
# P1-2 防混用：库必须是同 CC 同 libc 腿的产物（build-libatf-c.sh 写的 flavor 戳），
# 否则旧 libc 腿的 libatf-c.a 会被静默复用（FILE 布局/stdio 符号语义错）。
EXPECT_FLAVOR="$CC|$LIBC_KIND"
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
      '-D__arraycount(__x)=(sizeof(__x)/sizeof((__x)[0]))' "${NETBSD_ID_MACROS[@]}")

# picolibc 腿要靠 `-include errno-compat.h` 补它缺的 sysconf 声明；glibc 腿不挂那层
# （它自带 sysconf，注入反而与 glibc 声明冲突，理由见上面 x86_64 分支）。
# bsd-strfun.h 与 bsd-strerr.h 则**三腿都挂**：glibc 与 picolibc 的头面都没有 BSD 的
# stresep/popcount 家族，也给不出 strerror 族的 BSD 语义（`sys_nerr` 这个名字 glibc 2.32 起
# 已撤、措辞也不是 NetBSD 形），不挂就是隐式声明按 int 返回——aarch64 上 `sxtw w0` 把返回
# 指针截成野地址（CodeReview 续-279f P0 实测坐实），位计数族也会把 `unsigned int` 返回值截成 int。
COMPAT_INCLUDE=(-include "$COMPAT/bsd-strfun.h" -include "$COMPAT/bsd-strerr.h")
if [ "$COMPAT_TAKEOVER" = 1 ]; then
    COMPAT_INCLUDE+=(-include "$COMPAT/errno-compat.h")
fi

mkdir -p "$BUILD"
# 渲染 atf-c/defs.h（测试经 <atf-c/defs.h> 包含，与 build-libatf-c.sh 同源）。
mkdir -p "$BUILD/atf-c"
sed -e 's/@ATTRIBUTE_FORMAT_PRINTF@/__attribute__((format(printf, a, b)))/' \
    -e 's/@ATTRIBUTE_NORETURN@/__attribute__((__noreturn__))/' \
    -e 's/@ATTRIBUTE_UNUSED@/__attribute__((__unused__))/' \
    "$SRC/atf-c/defs.h.in" > "$BUILD/atf-c/defs.h"
name="$(basename "$TEST" .c)"
"$CC" -c -Os "${MC[@]}" "${SYS[@]}" \
    -I"$SRC" -I"$BUILD" -I"$COMPAT" "${COMPAT_INCLUDE[@]}" "${EXTRA_INC[@]}" "${CFGS[@]}" \
    "$TEST" -o "$BUILD/$name.o"
# sys-bridge 也在接管面之内：它是「与我方内核 trap 同形」的架构内联汇编（文件末尾对
# 未知架构直接 `#error`）。宿主 glibc 腿走真系统调用，不需要它——把它挂上只会把
# x86_64 卡死在编译期。x86_64 的出生/系统调用腿属台账 P-X86-01 的缺口④⑤，不在本批。
#
# ⚠ 接管面对象一律「先初始化一次、之后只追加」。批次二百六十三把这几段包进
# `if [ "$COMPAT_TAKEOVER" = 1 ]` 时，在 if 块之后又写了一条无条件 `STUB_OBJS=()`，
# 把刚放进来的 sys-bridge.o 当场抹掉——链接行里从此没有它。后果不是「少一个可有可无
# 的对象」：`_write`/`_read`/`ipc_sendrec`/`geteuid`/`getgroups` 全定义在 sys-bridge.c，
# riscv64 腿实测 13 处 undefined、一个 ATF 二进制也产不出（aarch64 同构同病）。而
# 批次二百六十三/二百六十四的验收证据只跑了 x86_64 腿，这条回退在账面上隐身了两天，
# 直到下一轮想往 STUB_OBJS 里追加新 compat 件时才被同一颗雷绊住（判据见台账 P-X86-01）。
# 因此这里只留一条追加通道，任何新增 compat 件都必须走 append、不许再赋值。
STUB_OBJS=()
if [ "$COMPAT_TAKEOVER" = 1 ]; then
    "$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "$COMPAT/sys-bridge.c" -o "$BUILD/sys-bridge.o"
    STUB_OBJS+=("$BUILD/sys-bridge.o")
fi
# posix-stubs / stdio-minix / sys-bridge 三件是「抢 picolibc 缺位」或「抢 libc
# 成员」的强定义接管面，只在 picolibc 腿挂。glibc 本就有 sysconf/strerror_r(GNU 形)/random，
# 再挂一份会在静态链上撞重复定义，还会改掉测试看到的 libc 语义。其余三族——stresep/位计数
# （bsd-strfun）、strerror/sys_nerr（bsd-strerr）、BSD 随机序（random.c）——都是「glibc 与
# picolibc 都不给同一个答案」的补位件，走下面三腿共用的通道。这条分工记在台账 P-X86-01 行。
if [ "$COMPAT_TAKEOVER" = 1 ]; then
    "$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "${CFGS[@]}" "$COMPAT/posix-stubs.c" -o "$BUILD/posix-stubs.o"
    STUB_OBJS+=("$BUILD/posix-stubs.o")
fi
# 把 picolibc semihost stdio 钩子改接我方 _write/SYS_DIAGCTL（两腿共享；抢定义
# sys_semihost_putc/getc，须排在 libsemihost.a 之前才能挡住 semihost 实现成员）。
if [ "$COMPAT_TAKEOVER" = 1 ]; then
    "$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "$COMPAT/stdio-minix.c" -o "$BUILD/stdio-minix.o"
    STUB_OBJS+=("$BUILD/stdio-minix.o")
fi
# BSD <md5.h> 摘要实现（picolibc 不提供），供 t_memcpy 等测试链入（仅构建/测试用）。
"$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "$COMPAT/md5.c" -o "$BUILD/md5.o"
"$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "${CFGS[@]}" "$COMPAT/bsd-strerr.c" -o "$BUILD/bsd-strerr.o"
BSD_STRERR_O=("$BUILD/bsd-strerr.o")
# BSD 字符串/位计数族（`stresep(3)` + `popcount(3)` 家族）：**glibc 与 picolibc 都没有**
# 这几个 BSD 函数，所以它不属于「按 libc 种类分腿」的接管面，而是两腿通吃的补位件。
# 过去它们埋在 posix-stubs.c 里，而 posix-stubs.c 是 picolibc 专用的整层接管面（同一
# TU 里还强定义 `sysconf`/`strerror`/`strerror_r`）——x86_64 腿要拿 popcount/stresep 就
# 必须把整层挂回去，连带改掉宿主 libc 的语义。台账 P-X86-01 要的正是「按符号拆开的
# compat 面」，本件即该拆分的第一个落点（真源：minix3/common/lib/libc/string/
# popcount32.c、popcount64.c、minix3/lib/libc/string/stresep.c）。
"$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "${CFGS[@]}" "$COMPAT/bsd-strfun.c" -o "$BUILD/bsd-strfun.o"
BSD_STRFUN_O=("$BUILD/bsd-strfun.o")
# BSD Boyer-Moore <bm.h>（bm_comp/exec/free，picolibc 不提供），供 t_bm 链入。
"$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "$COMPAT/bm.c" -o "$BUILD/bm.o"
# BSD sys_nerr 与 strerror 族已拆给上面的 bsd-strerr（三腿通吃）；旧的 errno-compat.c
# 只剩这一条定义，随拆分删除。picolibc 不提供这些 BSD 件，真语义对位见 bsd-strerr.c 头注。
# BSD random/srandom 真源（NK4-C §续-283 D1）：minix3/common/lib/libc/stdlib/
# random.c 逐字 vendor（fidelity=零改动，全部兼容在 random-shim/）。命令行对象
# 强定义 random+srandom（+initstate/setstate）→ libc 归档里的同名单文件成员不再被拉入
# （picolibc 的 LCG _rand_next 形、glibc 的 __random 形都永不上台），测试看到的
# 随机序由此在三条腿上都是 BSD 形——`t_memcpy` 的 goodResult 正押在这个序上，
# glibc 腿不挂它时该用例在宿主必 fail（批次二百六十九实测现形，本批改入三腿共用面）。
# 抢 glibc 自带符号不撞重复定义这件事，已由同批的 strerror 族实测定谳：
# 命令行目标文件先于归档解析，同名单文件成员因此不被拽入。
"$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" -I"$COMPAT/random-shim" "$COMPAT/random.c" -o "$BUILD/random.o"
BSD_RNG_O=("$BUILD/random.o")
# sbrk/sysconf/strerror 同形先例（但前两者仍只在 picolibc 腿抢，理由见上面分工段）。

# 上机出生链（启动腿）：picolibc 默认 crt0 把 sp 切进 ELF 内 .stack 且
# main(0,NULL)（atf tp_main 解引用 argv[0] 必死），需 -nostartfiles 换
# startup-minix.c 的自定义 _start（从内核 ps_strings 取 argc/argv，对位
# minix-rt 出生 ABI）。aarch64 腿于 §续-277 启用；riscv 腿在 (A) 栈顶缺陷
# 修复（§续-338）后同形启用——两架构入口 stub 均在 startup-minix.c 内
# （riscv 无 TLS 寄存器步骤，见该文件 riscv 分支注释）。
STARTUP_O=()
LINK_EXTRA=()
case "$ARCH" in
    aarch64|riscv64)
        "$CC" -c -Os "${MC[@]}" "${SYS[@]}" -I"$COMPAT" "$COMPAT/startup-minix.c" -o "$BUILD/startup-minix.o"
        STARTUP_O=("$BUILD/startup-minix.o")
        LINK_EXTRA=(-nostartfiles) ;;
esac

"$CC" -static "${MC[@]}" "${SYS[@]}" "${LINK_EXTRA[@]}" "${STARTUP_O[@]}" "$BUILD/$name.o" \
    -Wl,--start-group "$LIB" "${STUB_OBJS[@]}" "${BSD_STRFUN_O[@]}" "${BSD_STRERR_O[@]}" "${BSD_RNG_O[@]}" "$BUILD/md5.o" "$BUILD/bm.o" "${SEMIHOST[@]}" -lc -Wl,--end-group \
    -o "$BUILD/$name"

# 校验我方加载器 parse_ehdr 的硬项（ELF64 + LSB + ET_EXEC）。
# 宿主腿的 `$CC` 是 `cc`，剥掉 `-gcc` 拼出来的 `cc-readelf` 不存在；按存在性挑一个
# （交叉腿仍是 `<triplet>-readelf`，与宿主腿各用各自的 reader，避免用宿主 readelf 读
# 交叉产物时字段解释不一致）。
READER="${CC%-gcc}-readelf"
command -v "$READER" >/dev/null || READER=readelf
RE="$("$READER" -h "$BUILD/$name")"
echo "$RE" | grep -q 'Class:.*ELF64'   || { echo "非 ELF64" >&2; exit 1; }
echo "$RE" | grep -qi 'little endian'   || { echo "非 LSB" >&2; exit 1; }
echo "$RE" | grep -q 'Type:.*EXEC'     || { echo "非 ET_EXEC" >&2; exit 1; }
echo "✅ $BUILD/$name：静态 ET_EXEC $ARCH ELF，过 minix-elf ehdr 硬校验（$(stat -c%s "$BUILD/$name")B）"
