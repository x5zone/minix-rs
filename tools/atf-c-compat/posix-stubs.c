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
#include <stdint.h>   /* uintptr_t/intptr_t（sbrk 定义用，见文件尾） */
#include <locale.h> /* locale_t（strerror_l 接管签名，见文件尾） */
#include <unistd.h>   /* sysconf/_SC_PAGESIZE（sysconf 定义在本文件尾部） */
/* TU 内裸名 strerror_r 调用（如 compat 自表路他员）统一转发到本仓 XSI 实现，
 * 与 errno-compat.h 对测试 TU 的注入同源；置于 string.h 之后避免改写其声明。 */
#ifndef strerror_r
#define strerror_r __xpg_strerror_r
#endif
#include "errno-compat.h"   /* 定义端也看 stresep/sysconf 原型：防跨 TU 签名
                              * 漂移无编译器检查（CodeReview 续-279h P2；现在
                              * flags 未开 -Wmissing-prototypes，原型可见性是
                              * 为将来加强告警面预留） */

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
ssize_t read(int fd, void *buf, size_t len) {
    /* CodeReview 续-277b P2-2：_read 占位不置 errno，POSIX 读失败契约要求
     * errno 有效——陈旧值会指向日志里无关的早期失败，故为 0 时兜底 ENOSYS。 */
    int r = _read(fd, buf, len > 0x7fffffff ? 0x7fffffff : (int)len);
    if (r < 0 && errno == 0) errno = ENOSYS;
    return r;
}
/* lseek 占位不能返 0（0 是合法偏移=假成功，SEEK_END 取长类用例会被静默骗过）：
 * 诚实报 ENOSYS（CodeReview 续-277b P2-2）；真 VFS lseek 桥属③后续大件。 */
off_t lseek(int fd, off_t off, int whence) { (void)fd; (void)off; (void)whence; errno = ENOSYS; return (off_t)-1; }
int unlink(const char *path) { (void)path; errno = ENOSYS; return -1; }
int remove(const char *path) { (void)path; errno = ENOSYS; return -1; }

/* --- 目标③：minix3 string 测试用到的 picolibc/编译器未提供的函数（够链，
 *    部分为上机后续真实现：dl* 真需 dlopen→我方不支持返回 NULL）。 --- */
#include <dlfcn.h>
void *dlopen(const char *f, int t){ (void)f;(void)t; return (void*)0; }
void *dlsym(void *h, const char *n){ (void)h;(void)n; return (void*)0; }
int dlclose(void *h){ (void)h; return -1; }
char *dlerror(void){ return (char *)"dlopen unsupported"; }

/* BSD stresep（picolibc 无）——对位本仓 C 真源逐行移植：
 * minix3/lib/libc/string/stresep.c（BSD strsep 基 + esc 处理：命中 esc 则
 * strcpy 就地删 esc 并字面化下一字符；未转义分隔符写 NUL 断句；串尽返
 * NULL 并置 *stringp=NULL；省 _DIAGASSERT/__weak_alias，余逐行同）。
 * **调用侧声明配套**：picolibc 头面零声明→隐式 int 返回在 aarch64 被
 * sxtw 截成野地址（CodeReview 续-279f P0 实测旧二进制坐实），声明已补入
 * errno-compat.h（-include 注入通道，与 sys_nerr 同络）；修后须核
 * `objdump -d t_stresep | grep -A1 'bl.*<stresep>'` 无 sxtw 才算闭合。
 * 旧占位返 NULL 的 t_stresep 假红预告（§续-271 P2-3）至此定义+声明配套就位。 */
char *stresep(char **stringp, const char *delim, int esc) {
    char *s;
    const char *spanp;
    int c, sc;
    char *tok;

    if ((s = *stringp) == NULL)
        return NULL;
    for (tok = s;;) {
        c = *s++;
        while (esc != '\0' && c == esc) {
            (void)strcpy(s - 1, s);
            c = *s++;
        }
        spanp = delim;
        do {
            if ((sc = *spanp++) == c) {
                if (c == 0)
                    s = NULL;
                else
                    s[-1] = 0;
                *stringp = s;
                return tok;
            }
        } while (sc != 0);
    }
}

/* sysconf：既有定义在 **libsemihost.a 的 sysconf.c.o，强符号**（非 weak；
 * libc.a 内无 sysconf，nm 实测）——它只认 _SC_CLK_TCK，其余（含
 * _SC_PAGESIZE）errno=EINVAL 返 -1，真机 gh142 定谳 strlen_huge 的
 * `page >= 0` 断言即在此 fail。抢占机制的实相：本 TU 是**命令行目标
 * 文件**，先于归档解析参与符号定义，libsemihost 的 sysconf.c.o 己无未定义
 * 符号可拉它——不是“weak 可覆盖”（CodeReview 续-279f P1-2 纠正旧误记）。
 * 若日后该成员因他符号被拉入将现强-强 multiple definition硬错（当前其仅
 * 定义 sysconf 一符号故不炸）；届时改法：同法把 semihost 剩成员也隔掉。
 * 代价登记：libsemihost 的 _SC_CLK_TCK 支持被遮（现 18 案无人读，进 sys/
 * gen 目录案时复查）。_SC_PAGESIZE=4096 对位三架构 PAGE_BYTES 真源。 */
long sysconf(int name) {
    if (name == _SC_PAGESIZE)
        return 4096;
    errno = EINVAL;
    return -1;
}

/* 堆增长腿——抢 libsemihost.a 的 **sbrk**（真机 gh153/155 定谳：malloc→
 * __malloc_sbrk_aligned→sbrk 命中的是 semihost 假实现：纯用户态推进自己
 * .data 里的 brk 变量、永不下陷，堆“成功”长进未映射页 → wro SIGSEGV，
 * memset_nonzero 即在此丢命；我方 sys-bridge.c 的 _sbrk 不在 malloc 链路上）。
 * 语义逐行对位 minix3 libc sys/sbrk.c：incr>0 先发 VM_BRK(new_addr)（C 为
 * _syscall(VM_PROC_NR, VM_BRK, &m)，m_lc_vm_brk.addr 在载荷 0 号槽），成功后
 * 才采纳新断点；incr<0 只回退 break——C VM 本无缩减腿（break.c 只有
 * extend；region.c:1016 低地址 no-op OK），页保留。断点起点取链接符号
 * _end（picolibc.ld `end = __bss_end`，与 C arch brksize.S 的
 * `_brksize: .long _end` 同源同义）。抢占机制同 sysconf（命令行目标文件
 * 先于归档解析）；风险同款登记：semihost sbrk.c.o 因他符号被拉入则强-强
 * 相冲（当前其成员只定义 sbrk/brk 族，不炸）。 */
extern long ipc_sendrec(int endpoint, struct kmsg *m);
#define POSIX_STUBS_VM_ENDPOINT 8      /* minix-sys vm.rs VM_ENDPOINT_NUMBER（C com.h:67） */
#define POSIX_STUBS_VM_CALL_BREAK 0xC02 /* minix-sys vm.rs VM_CALL_BREAK（C com.h:636 VM_BRK） */
extern char _end;                       /* 链接器提供的镜像末（picolibc.ld） */
static uintptr_t posix_stub_break = (uintptr_t)&_end;
void *sbrk(intptr_t incr) {
    uintptr_t old = posix_stub_break;
    uintptr_t next = (uintptr_t)((intptr_t)old + incr);
    if (incr > 0) {
        struct kmsg m;
        long r;
        /* CodeReview 续-279m P2-2：对位 C sbrk.c:20 的回绕保护——正向增长
         * 却得到更小地址即溢出（本空问不可能触达，封非法态防后继复用）。 */
        if (next < old) { errno = ENOMEM; return (void *)-1; }
        memset(&m, 0, sizeof m);
        m.m_type = POSIX_STUBS_VM_CALL_BREAK;
        m.slots[0] = (long long)next;   /* m_lc_vm_brk.addr @payload+0 */
        r = ipc_sendrec(POSIX_STUBS_VM_ENDPOINT, &m);
        if (r != 0) { errno = (int)r; return (void *)-1; }
        /* CodeReview 续-279m P2-1：VM 错误回执经 reply_to_errno（encode.rs:94）
         * 是**正 errno**（m_type=code，ipc_sendrec 已把往返错与回执码分离），
         * 与 C _syscall/lstat 路的**负 errno** 约定相反——这里直取正值，
         * 取负会得到非法负 errno。两种回执形各自钉死在此注释。 */
        if (m.m_type != 0) { errno = (int)m.m_type; return (void *)-1; }
    }
    posix_stub_break = next;
    return (void *)old;
}

int popcountll(long long x){ return __builtin_popcountll((unsigned long long)x); }
int popcount(unsigned x){ return __builtin_popcount(x); }

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
/* XSI 实现：C 名 __xpg_strerror_r；errno-compat.h（本 TU 亦 -include 之外
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
