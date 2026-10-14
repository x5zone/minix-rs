/* bsd-strerr.c — BSD strerror 族的自给语义面（目标③ 构建/链接面，不进生产镜像）。
 *
 * 为什么单独成件（台账 P-X86-01 的 t_strerror 缺口）：`t_strerror.c` 的四支用例全押在
 * NetBSD/BSD 语义上——以 `sys_nerr` 为「首个未知码」的边界、表外码必须产
 * `"Unknown error: N"` 并置 `errno=EINVAL`、`strerror_r` 取 XSI 整数码（0/EINVAL/ERANGE）。
 * **glibc 与 picolibc 都不给这套语义**：glibc 的措辞是 `Unknown error %d`（无冒号）、
 * 越界不置 EINVAL，且 `_GNU_SOURCE` 下 `strerror_r` 是 GNU 形返回 `char *`；
 * picolibc（newlib 血缘）越界码实测给空串（探针 gh160 定谳）。原先这套实现住在
 * posix-stubs.c 尾部，而 posix-stubs.c 是 picolibc 专用的整层接管面，于是 x86_64
 * （glibc）腿永远拿不到它——`t_strerror` 编译期就 `sys_nerr undeclared`。
 * 现在拆成本件，三条腿同一张表、同一套语义，宿主腿还能把 t_strerror 真跑出终态。
 *
 * 消息源不变（逐条对位，见下面的原注释）：
 *   1..79   = minix3/lib/libc/compat/gen/compat_errlist.c 逐行移植（本仓 C 真源）；
 *   80..134 = picolibc sys/errno.h 各码英文注释（表洞码诚实落 "error"）；
 *   越界    = NetBSD strerror.c 同形 "Unknown error: N" + errno=EINVAL
 *             （对位 minix3/lib/libc/string/strerror.c:111 的越界腿）。
 * 表内容与 libc 种类无关，故三腿读数一致；下表与搬动前逐字节相同（135 项未增未删）。 */

#include <errno.h>
#include <locale.h>   /* locale_t（strerror_l 签名） */
#include <stdint.h>   /* intptr_t（XSI 返回码的指针衣装） */
#include <stdio.h>    /* snprintf */
#include <string.h>   /* strlen/strcpy */

#include "bsd-strerr.h"

/* BSD sys_nerr（picolibc 与 glibc 都不给这个名字：glibc 2.32 起把 sys_nerr/sys_errlist
 * 的 BSD 名撤掉，picolibc 的 <errno.h> 里根本没有）。定义端与声明端同 TU 对账，
 * 防「声明/定义跨 TU 漂移」（旧记录见 errno-compat.h 头注，该头已把本族移交此处）。
 * 边界差一定谳：旧值 134 + `e <= sys_nerr` 判据令 e=134 被当已知，而测试以 sys_nerr 为
 * 「首个未知码」——表补 0 项后表项数 135，e∈[1,134] 已知、e>=135 未知。 */
const int sys_nerr = 135;

/* --- strerror 族真语义（§续-279m 探针 gh160 定谳后落地，自给表实现）——
 * picolibc(newlib 血缘)对越界码**永不产 BSD 形 "Unknown error: N"**：
 * gh160 实测 strerror(135/200/INT_MAX/INT_MIN)=空串、strerror(133)=空串
 * （表洞）、strerror_r(GNU 声明+XSI 语义混流)把码写进 buf[0]——t_strerror
 * 四案的断言全押在 NetBSD 语义上，故 compat 全接管两函数（抢占机制同
 * sbrk/sysconf：命令行目标文件先于归档解析）。消息源为**自给表**：
 *   1..79   = minix3/lib/libc/compat/gen/compat_errlist.c 逐行移植（本仓 C 真源）；
 *   80..134 = picolibc sys/errno.h 各码英文注释（picolibc 自己的措辞，表洞码
 *             诚实落 "error"）；越界 = NetBSD strerror.c 同形 "Unknown error: N"
 *             + errno=EINVAL（C lib/libc/string/strerror.c:111）。
 * 不向内问路 picolibc：_strerror_r 的真实签名是 (int,int,int*)
 * （string.h:145），按假原型调用是 UB；自给表零外呼。
 * 链接形定稿（三轮试错坐实，证据链在 WORKLOG §续-279n）：测试调用是
 * **裸名 strerror_r**（objdump 实锤 R_AARCH64_CALL26 strerror_r），只挂 asm
 * 别名不满足裸引用——链接器经 picolibc strerror_r 成员的 U _strerror_r 拽入
 * libc_string_strerror.c.o（定义 strerror），与自家强定义 multiple definition
 * 硬错。定稿=errno-compat.h 在 string.h 声明**之前**注入 `#define strerror_r
 * __xpg_strerror_r`（其 GNU 形声明被一并改名，与定义同形无冲突），所有 TU
 * 裸引用统一到本 XSI 实现；本 TU 在 string.h 后补同名宏保持域内一致。
 * 诚实登记：测试比较式处 `int` 值对 char* 声明的告警保留（§续-278d 基线），
 * 0/EINVAL/ERANGE 的零/非零性在两值域同形，断言面（== 0 / == EINVAL /
 * == ERANGE / != NULL）语义成立，真机读数定案。 */
#define COMPAT_STRERROR_UNK_MAX 128   /* NetBSD strerror.c:101 同值上界 */
/* 布局与 C sys_errlist 同构：index == errno（0 项 = "Undefined error: 0"，
 * compat_errlist.c:52 真源原串），sys_nerr（errno-compat.c）= 表项数 = 135，
 * 与 compat_errlist.c:153 的 sizeof 公式同形。§续-279n 定谳的边界差一：
 * 旧实现判 `e <= sys_nerr` 且 sys_nerr=134，e=134 被当已知，而测试以
 * sys_nerr 为「首个未知码」——i=134 处 :54/:94 首断言即败（p3 探针只测了
 * 越界侧 135/140，恰好漏测边界，矛盾归一）。 */
static const char *const compat_errstr[] = {
	"Undefined error: 0", /* 0 */
	"Operation not permitted", /* 1 */
	"No such file or directory", /* 2 */
	"No such process", /* 3 */
	"Interrupted system call", /* 4 */
	"Input/output error", /* 5 */
	"Device not configured", /* 6 */
	"Argument list too long", /* 7 */
	"Exec format error", /* 8 */
	"Bad file descriptor", /* 9 */
	"No child processes", /* 10 */
	"Resource deadlock avoided", /* 11 */
	"Cannot allocate memory", /* 12 */
	"Permission denied", /* 13 */
	"Bad address", /* 14 */
	"Block device required", /* 15 */
	"Device busy", /* 16 */
	"File exists", /* 17 */
	"Cross-device link", /* 18 */
	"Operation not supported by device", /* 19 */
	"Not a directory", /* 20 */
	"Is a directory", /* 21 */
	"Invalid argument", /* 22 */
	"Too many open files in system", /* 23 */
	"Too many open files", /* 24 */
	"Inappropriate ioctl for device", /* 25 */
	"Text file busy", /* 26 */
	"File too large", /* 27 */
	"No space left on device", /* 28 */
	"Illegal seek", /* 29 */
	"Read-only file system", /* 30 */
	"Too many links", /* 31 */
	"Broken pipe", /* 32 */
	"Numerical argument out of domain", /* 33 */
	"Result too large or too small", /* 34 */
	"Resource temporarily unavailable", /* 35 */
	"Operation now in progress", /* 36 */
	"Operation already in progress", /* 37 */
	"Socket operation on non-socket", /* 38 */
	"Destination address required", /* 39 */
	"Message too long", /* 40 */
	"Protocol wrong type for socket", /* 41 */
	"Protocol option not available", /* 42 */
	"Protocol not supported", /* 43 */
	"Socket type not supported", /* 44 */
	"Operation not supported", /* 45 */
	"Protocol family not supported", /* 46 */
	"Address family not supported by protocol family", /* 47 */
	"Address already in use", /* 48 */
	"Can't assign requested address", /* 49 */
	"Network is down", /* 50 */
	"Network is unreachable", /* 51 */
	"Network dropped connection on reset", /* 52 */
	"Software caused connection abort", /* 53 */
	"Connection reset by peer", /* 54 */
	"No buffer space available", /* 55 */
	"Socket is already connected", /* 56 */
	"Socket is not connected", /* 57 */
	"Can't send after socket shutdown", /* 58 */
	"Too many references: can't splice", /* 59 */
	"Operation timed out", /* 60 */
	"Connection refused", /* 61 */
	"Too many levels of symbolic links", /* 62 */
	"File name too long", /* 63 */
	"Host is down", /* 64 */
	"No route to host", /* 65 */
	"Directory not empty", /* 66 */
	"Too many processes", /* 67 */
	"Too many users", /* 68 */
	"Disc quota exceeded", /* 69 */
	"Stale NFS file handle", /* 70 */
	"Too many levels of remote in path", /* 71 */
	"RPC struct is bad", /* 72 */
	"RPC version wrong", /* 73 */
	"RPC prog. not avail", /* 74 */
	"Program version wrong", /* 75 */
	"Bad procedure for program", /* 76 */
	"No locks available", /* 77 */
	"Function not implemented", /* 78 */
	"Inappropriate file type or format", /* 79 */
	"Given log. name not unique", /* 80 */
	"File descriptor in bad state", /* 81 */
	"Remote address changed", /* 82 */
	"Can't access a needed shared lib", /* 83 */
	"Accessing a corrupted shared lib", /* 84 */
	".lib section in a.out corrupted", /* 85 */
	"Attempting to link in too many libs", /* 86 */
	"Attempting to exec a shared library", /* 87 */
	"Function not implemented", /* 88 */
	"No more files", /* 89 */
	"Directory not empty", /* 90 */
	"File or path name too long", /* 91 */
	"Too many symbolic links", /* 92 */
	"error", /* 93 */
	"error", /* 94 */
	"Operation not supported on socket", /* 95 */
	"Protocol family not supported", /* 96 */
	"error", /* 97 */
	"error", /* 98 */
	"error", /* 99 */
	"error", /* 100 */
	"error", /* 101 */
	"error", /* 102 */
	"error", /* 103 */
	"Connection reset by peer", /* 104 */
	"No buffer space available", /* 105 */
	"Address family not supported by protocol family", /* 106 */
	"Protocol wrong type for socket", /* 107 */
	"Socket operation on non-socket", /* 108 */
	"Protocol not available", /* 109 */
	"Can't send after socket shutdown", /* 110 */
	"Connection refused", /* 111 */
	"Address already in use", /* 112 */
	"Software caused connection abort", /* 113 */
	"Network is unreachable", /* 114 */
	"Network interface is not configured", /* 115 */
	"Connection timed out", /* 116 */
	"Host is down", /* 117 */
	"Host is unreachable", /* 118 */
	"Connection already in progress", /* 119 */
	"Socket already connected", /* 120 */
	"Destination address required", /* 121 */
	"Message too long", /* 122 */
	"Unknown protocol", /* 123 = EPROTONOSUPPORT（两腿 picolibc 头皆 #define 123，非表洞；CodeReview 续-279n-b P2-1 保真） */
	"Socket type not supported", /* 124 */
	"Address not available", /* 125 */
	"Connection aborted by network", /* 126 */
	"Socket is already connected", /* 127 */
	"Socket is not connected", /* 128 */
	"Too many references: cannot splice", /* 129 */
	"Too many processes", /* 130 */
	"Too many users", /* 131 */
	"Reserved", /* 132 */
	"Reserved", /* 133 */
	"Not supported", /* 134 */
};

static const char *compat_strerror_msg(int e) {
    /* C 契约（compat_errlist.c:153 sys_nerr=表项数）：已知区间是
     * [0, sys_nerr)，负值与 >=sys_nerr 都走调用方的 Unknown error: N 腿。 */
    if (e >= 0 && e < sys_nerr)
        return compat_errstr[e];
    return NULL;                      /* 越界：调用方产 Unknown error: N */
}
static int compat_strerror_unk(int e, char *out, size_t n) {
    return snprintf(out, n, "Unknown error: %d", e);
}
static char compat_strerror_static[COMPAT_STRERROR_UNK_MAX];
char *strerror(int e) {
    const char *s = compat_strerror_msg(e);
    if (s == NULL) {
        compat_strerror_unk(e, compat_strerror_static, sizeof compat_strerror_static);
        errno = EINVAL;               /* C strerror.c:111 越界同置 */
        s = compat_strerror_static;
    }
    return (char *)s;
}
/* XSI 实现：C 名 __xpg_strerror_r；bsd-strerr.h（本 TU 亦 -include 之外
 * 手动保持同形）以 `#define strerror_r __xpg_strerror_r` 把所有裸名转发到此。
 * 本 TU 的 string.h 在宏注入后才被包含的问题不存在：-include 先于一切 #include。
 * 定义取 char * 形=与测试 TU 被改名的 GNU 形声明同形（CodeReview 续-279n-b
 * P2-2，防 §续-279f stresep 类声明/定义跨 TU 漂移）：内部按 XSI 计码后以
 * (char *)(intptr_t) 回传。**负向契约**：GNU 风格调用法（`char *s =
 * strerror_r(...)` 后解引用）在本通道是 UB——返回值是 XSI 码的整数衣装而非
 * 指针；现役测试集（t_strerror.c）全为 XSI 断言式，无 GNU 受害者。 */
char *__xpg_strerror_r(int e, char *buf, size_t buflen) {
    const char *s = compat_strerror_msg(e);
    char unk[COMPAT_STRERROR_UNK_MAX];
    if (s == NULL) {
        compat_strerror_unk(e, unk, sizeof unk);
        s = unk;
    }
    if (buflen < strlen(s) + 1)
        return (char *)(intptr_t)ERANGE;  /* XSI strerror_r(3) 失败码 */
    strcpy(buf, s);
    if (compat_strerror_msg(e) == NULL) {
        errno = EINVAL;               /* 越界：与 strerror 同置（调用方可察） */
        return (char *)(intptr_t)EINVAL;
    }
    return (char *)(intptr_t)0;
}


/* picolibc 的 strerror_l 与 strerror 同居 libc_string_strerror.c.o：成员若被
 * 他员拽入即与自家 strerror 强-强硬错——compat 同形接管，断掉拉入链
 * （locale 在本环不存在，l 版即同表查询）。 */
char *strerror_l(int e, locale_t loc) { (void)loc; return strerror(e); }
