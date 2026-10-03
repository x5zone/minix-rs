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
#include <stddef.h>
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

/* VFS IPC 腿（sys-bridge.c ipc_sendrec，对位 minix-sys perform_syscall）：
 * 返回 0=往返成功（回执写入 *m），非 0=errno。端点号对位 minix-sys
 * VFS_ENDPOINT_NUMBER=1；调用号见各使用处注。struct kmsg 单点定义在
 * kmsg.h（与 sys-bridge.c 同源，杜绝跨 TU 手抄漂移）。 */
#include "kmsg.h"
extern long ipc_sendrec(int endpoint, struct kmsg *m);
#define VFS_ENDPOINT    1

/* lstat：真实现 + **布局翻译层**（CodeReview 续-277 P0 定谳：我方 FS/VFS 写的是
 * minix-types Stat 152B 形（os/libs/minix-types/src/types/stat.rs 布局测试钉死），
 * 而 picolibc 各腿 struct stat 形各不相同（评审实测 aarch64=120B，本会话探针实测
 * riscv=104B 且 ino/mode 窄字段）——直接把调用者栈对象指针交给 FS 写 = aarch64
 * 腿越界 32B 砸掉 saved x29/x30（真机 §续-277b 前“lstat 回执后公共链跳 0”的定谳
 * 根因）+ 字段全读垃圾）。
 * 做法：FS 写进本函数栈上的 raw[152]，再逐字段 C 赋值进调用者对象——目标侧偏移
 * 与字段宽度全由编译器推导，两架构腿零手抄；FS 侧偏移用下方 FS_ST_* 常量，对位
 * stat.rs 布局测试（两侧任一漂移，读侧 ld32/ld64 赋值即显式截断，非静默错位）。
 * C 对位：minix3 里 FS 写的本就是调用者同编译环境的 C 形 stat，无翻译需求——
 * 异 libc 环的跨尺寸边翻译是本重写专属的必要层。 */
#define FS_ST_DEV     0   /* u64 */
#define FS_ST_MODE    8   /* u32 */
#define FS_ST_INO     16  /* u64 */
#define FS_ST_NLINK   24  /* u32 */
#define FS_ST_UID     28  /* u32 */
#define FS_ST_GID     32  /* u32 */
#define FS_ST_RDEV    40  /* u64 */
#define FS_ST_ATIM    48  /* {sec,nsec} 2×i64 */
#define FS_ST_MTIM    64
#define FS_ST_CTIM    80
#define FS_ST_SIZE    112 /* i64 */
#define FS_ST_BLOCKS  120 /* i64 */
#define FS_ST_BLKSIZE 128 /* i32 */
#define FS_STAT_SIZE  152

/* 读 raw 字节缓冲里一个 LE u64/i64 / u32（对齐无关）。 */
static unsigned long long ld64(const unsigned char *p) {
    unsigned long long v = 0;
    for (int i = 7; i >= 0; i--) v = (v << 8) | p[i];
    return v;
}
static unsigned int ld32(const unsigned char *p) {
    return (unsigned int)p[0] | ((unsigned int)p[1] << 8)
         | ((unsigned int)p[2] << 16) | ((unsigned int)p[3] << 24);
}

#define VFS_CALL_LSTAT 0x117    /* vfs.rs VFS_CALL_LSTAT = VFS_BASE+23 */
int lstat(const char *path, struct stat *st) {
    struct kmsg m;
    unsigned char raw[FS_STAT_SIZE];
    size_t len;
    long r;
    if (path == NULL || st == NULL) { errno = EFAULT; return -1; }
    len = strlen(path) + 1;
    memset(raw, 0, sizeof raw);
    memset(&m, 0, sizeof m);
    m.m_type = VFS_CALL_LSTAT;
    m.slots[0] = (long long)len;                          /* length（含 NUL） */
    m.slots[1] = (long long)(unsigned long)(const void *)path; /* name */
    m.slots[2] = (long long)(unsigned long)(void *)raw;   /* buffer：本帧 152B 落点 */
    r = ipc_sendrec(VFS_ENDPOINT, &m);
    if (r != 0) { errno = (int)r; return -1; }
    if (m.m_type < 0) { errno = -m.m_type; return -1; }
    /* 152B FS 形 → 调用者 struct stat：逐字段 C 赋值（宽度截断由编译期
     * 显式化，两架构 picolibc 形自动适配；未赋值槽保持清零默认） */
    memset(st, 0, sizeof *st);
    st->st_dev     = (dev_t)ld64(raw + FS_ST_DEV);
    st->st_ino     = (ino_t)ld64(raw + FS_ST_INO);
    st->st_mode    = (mode_t)ld32(raw + FS_ST_MODE);
    st->st_nlink   = (nlink_t)ld32(raw + FS_ST_NLINK);
    st->st_uid     = (uid_t)ld32(raw + FS_ST_UID);
    st->st_gid     = (gid_t)ld32(raw + FS_ST_GID);
    st->st_rdev    = (dev_t)ld64(raw + FS_ST_RDEV);
    st->st_size    = (off_t)ld64(raw + FS_ST_SIZE);
    st->st_atim.tv_sec = (time_t)ld64(raw + FS_ST_ATIM);
    st->st_mtim.tv_sec = (time_t)ld64(raw + FS_ST_MTIM);
    st->st_ctim.tv_sec = (time_t)ld64(raw + FS_ST_CTIM);
    st->st_blksize = (blksize_t)ld32(raw + FS_ST_BLKSIZE);
    st->st_blocks  = (blkcnt_t)ld64(raw + FS_ST_BLOCKS);
    return 0;
}

/* access：委托 lstat + 按 st_mode 位判权限（P2-3：翻译层修复后 st_mode 可信，
 * 不再无条件返 0 造成 x 位假绿；本 OS 单用户 root 面对 r/w 宽松、对 x 看
 * 类型位，对位 C 的 root-旁路语义）。atf_fs_exists 只需 F_OK。 */
#ifndef F_OK
#define F_OK 0
#endif
#ifndef X_OK
#define X_OK 1
#endif
#ifndef W_OK
#define W_OK 2
#endif
#ifndef R_OK
#define R_OK 4
#endif
#define FS_MODE_XBITS 0111u
int access(const char *path, int mode) {
    struct stat st;
    if (lstat(path, &st) == -1)
        return -1;   /* errno 已由 lstat 置好（ENOENT → exists=false 的 C 语义） */
    if (mode == F_OK)
        return 0;
    /* X_OK 真判执行位；R_OK/W_OK 在本单用户 root 面宽松（C 对 root 同形：
     * x 仍需任一位，见 atf 同源的 BSD access 语义）。 */
    if ((mode & X_OK) && (st.st_mode & FS_MODE_XBITS) == 0) {
        errno = EACCES;
        return -1;
    }
    return 0;
}
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

/* --- POSIX 名转发（CodeReview 续-277b P2-4）：libsemihost.a 的 write/read/
 *    lseek/unlink 成员用半主机 brk 序列（EL0 不可达，一旦被抓就未知异常）；
 *    本 TU 排在 group 中 libsemihost.a 之前，抢定义这些 POSIX 名后 semihost
 *    成员整个不被拉入（archive 只为未定义符号拉成员）。open/close 真源在
 *    sys-bridge.c（单点定义，不重复）。语义：write→我方 _write 桥；其余按
 *    占位（真 VFS open/read 桥属③后续大件，诚实占位不谎称可跑）。 --- */
extern int _write(int fd, const void *buf, size_t len);
extern int _read(int fd, void *buf, int len);
ssize_t write(int fd, const void *buf, size_t len) { return _write(fd, buf, len); }
ssize_t read(int fd, void *buf, size_t len) { return _read(fd, buf, len > 0x7fffffff ? 0x7fffffff : (int)len); }
off_t lseek(int fd, off_t off, int whence) { (void)fd; (void)off; (void)whence; return 0; }
int unlink(const char *path) { (void)path; errno = ENOSYS; return -1; }
int remove(const char *path) { (void)path; errno = ENOSYS; return -1; }

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
