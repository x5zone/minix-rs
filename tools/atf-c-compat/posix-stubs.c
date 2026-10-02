/* NK4-C 目标③：picolibc 未提供的 BSD/POSIX 函数补全（跨架构共享，纯 C 无 trap
 * 指令），使真实 minix3 atf 测试 (libatf-c + 用例) 能**完整链接**成静态 ELF
 * （③ 的"可加载二进制"里程碑；riscv64/aarch64 两腿同源）。
 *
 * 诚实边界（WIP）：
 *   - writev / err / warnx：真实现（writev→_write 循环；err/warnx→vsnprintf+console）。
 *   - fork / waitpid / exec / access / lstat / dup2 / fchmod / umask / rmdir /
 *     mkdtemp / getcwd：先给最小占位够链。**真正上机跑**需把这些接到我方
 *     VFS(open/read/write/getdents) + 进程(SYS_FORK/SYS_WAITPID/exec) 桥——那是 ③
 *     后续大块（riscv 上机受 (A) 门控；aarch64 上机腿见 build-atf-test.sh）。
 * 此处只解"链接"，不谎称"可跑"。仅 ③ 构建期用；不参与生产镜像/三架构 marker。 */
#include <sys/types.h>
#include <sys/stat.h>
#include <errno.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>

#ifndef _UIO_VEC_
struct iovec { void *iov_base; size_t iov_len; };
#endif
extern int _write(int fd, const void *buf, size_t len);   /* 与 sys-bridge.c 定义同签名，避免跨 TU ODR 不一致 */

ssize_t writev(int fd, const struct iovec *iov, int cnt) {
    ssize_t tot = 0;
    for (int i = 0; i < cnt; i++) {
        ssize_t w = _write(fd, iov[i].iov_base, iov[i].iov_len);
        if (w < 0) return tot ? tot : -1;
        tot += w;
    }
    return tot;
}

static void diag_v(const char *fmt, va_list ap, int do_exit, int code) {
    char buf[128];
    int n = vsnprintf(buf, sizeof buf, fmt, ap);
    if (n < 0) n = 0;
    if (n >= (int)sizeof buf) n = (int)sizeof buf - 1;  /* vsnprintf 截断时 buf 实有 sizeof-1 字符，勿写进 NUL */
    _write(2, buf, n);
    _write(2, "\n", 1);
    if (do_exit) { extern void _exit(int); _exit(code ? code : 1); }
}
void warn(const char *fmt, ...) { va_list a; va_start(a,fmt); diag_v(fmt,a,0,0); va_end(a); }
void warnx(const char *fmt, ...) { va_list a; va_start(a,fmt); diag_v(fmt,a,0,0); va_end(a); }
void err(int code, const char *fmt, ...) { va_list a; va_start(a,fmt); diag_v(fmt,a,1,code); va_end(a); }
void errx(int code, const char *fmt, ...) { va_list a; va_start(a,fmt); diag_v(fmt,a,1,code); va_end(a); }
/* err.h 声明了 va_list 变体；libatf-c 若引用需定义，否则链接断。 */
void vwarn(const char *fmt, va_list a)  { diag_v(fmt,a,0,0); }
void vwarnx(const char *fmt, va_list a) { diag_v(fmt,a,0,0); }
void verr(int code, const char *fmt, va_list a)  { diag_v(fmt,a,1,code); }
void verrx(int code, const char *fmt, va_list a) { diag_v(fmt,a,1,code); }

/* --- 进程/文件系统占位（够链；真跑接我方 IPC/VFS 桥，见文件头 WIP 说明）--- */
pid_t fork(void) { errno = ENOSYS; return -1; }
pid_t waitpid(pid_t p, int *st, int fl) { (void)p;(void)st;(void)fl; errno = ENOSYS; return -1; }
int access(const char *p, int m) { (void)p;(void)m; errno = ENOSYS; return -1; }
int lstat(const char *p, struct stat *s) { (void)p;(void)s; errno = ENOSYS; return -1; }
int dup2(int a, int b) { (void)a;(void)b; errno = ENOSYS; return -1; }
int fchmod(int f, mode_t m) { (void)f;(void)m; return 0; }
mode_t umask(mode_t m) { (void)m; return 0; }
int rmdir(const char *p) { (void)p; errno = ENOSYS; return -1; }
char *getcwd(char *buf, size_t sz) {
    if (!buf) { errno = EINVAL; return NULL; }
    if (sz < 2) { errno = ERANGE; return NULL; }
    buf[0] = '/'; buf[1] = '\0';
    return buf;
}
char *mkdtemp(char *tmpl) { errno = ENOSYS; (void)tmpl; return NULL; }

/* --- 目标③：minix3 string 测试用到的 picolibc/编译器未提供的函数（够链，
 *    部分为上机后续真实现：dl* 真需 dlopen→我方不支持返回 NULL）。 --- */
#include <dlfcn.h>
void *dlopen(const char *f, int t){ (void)f;(void)t; return (void*)0; }
void *dlsym(void *h, const char *n){ (void)h;(void)n; return (void*)0; }
int dlclose(void *h){ (void)h; return -1; }
char *dlerror(void){ return (char *)"dlopen unsupported"; }

int popcountll(long long x){ return __builtin_popcountll((unsigned long long)x); }
int popcount(unsigned x){ return __builtin_popcount(x); }

/* BSD stresep（picolibc 无）。真原型 char *stresep(char**, const char*, int esc)。
 * 够链占位（返回 NULL）；真语义上机再补。签名必须对齐否则调用侧按 implicit-int
 * 截返回指针→高位丢失现野地址。__arraycount 不在此（它是 NetBSD 编译期宏，
 * 由 build-atf-test.sh 的 -D 提供，若做函数存根会致测试循环零覆盖假绿）。 */
char *stresep(char **stringp, const char *delim, int esc){
    (void)stringp; (void)delim; (void)esc; return (char *)0;
}
