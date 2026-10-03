/* NK4-C 目标③：C 测试的 syscall 桥（跨架构单一实现）——把 picolibc 的 sysstub
 * (_write/_exit/_sbrk/_read/_close/_lseek/_fstat/_isatty/_getpid) 映射到我方
 * kernel_call ABI，对位 os/libs/minix-sys/src/arch_trap.rs（单一真源）：
 *   riscv64 ：`ecall` a7=KERNEL_CALL_TRAP_NR(0), a0=&Message, 回码在 a0
 *   aarch64 ：`svc #0` x8=KERNEL_CALL_TRAP_NR(0), x0=&Message, 回码在 x0
 * 仅 ③ 构建/链接用；不参与 minix-rs 生产镜像。
 * Message 布局（minix-types ipc/message.rs）：m_source i32@0, m_type i32@4, m_u@8。
 * 常量（ipc/kernel_call.rs）：SYS_DIAGCTL=0x62c, SYS_EXIT=0x635, DIAGCTL_CODE_DIAG=1。
 * IPC 腿常量（ipc/message.rs + minix-sys pm.rs）：SENDREC=3, PM 端点=0,
 *   PM_CALL_EXIT=1（C callnr.h:14；m_lc_pm_exit.status 在 m_u@0 即字节 8）。 */
#include <sys/types.h>
#include <sys/stat.h>
#include <errno.h>

#define SYS_DIAGCTL 0x62c
#define SYS_EXIT    0x635
#define DIAGCTL_CODE_DIAG 1
#define KERNEL_CALL_TRAP_NR 0
#define IPC_SENDREC_NR 3          /* arch_trap.rs SENDREC_NR（C ipcconst.h SENDREC=3） */
#define PM_ENDPOINT   0           /* minix-sys pm.rs PM_ENDPOINT_NUMBER（我方重写端点号） */
#define PM_CALL_EXIT  1           /* minix-sys pm.rs PM_CALL_EXIT（C callnr.h:14 PM_EXIT） */

/* picolibc _write 第三参为 size_t；手工 typedef 避免与已有 sys stub 头冲突。 */
typedef unsigned long size_t_local;

/* Message 手写镜像（单点定义在 kmsg.h，与 posix-stubs.c 共用）：前 8 字节
 * m_source/m_type，其后 diagctl 载荷 code@8,buf@16,len@24（载荷共 56B）。 */
#include "kmsg.h"

/* 一次 kernel_call：寄存器对位 arch_trap.rs 的 kernel_call_trap（两架构同形：
 * 消息指针=第一入参寄存器，调用号=syscall-number 寄存器，回码=第一返回寄存器）。 */
#if defined(__riscv)
static long kcall(struct kmsg *m) {
    register long a0 __asm__("a0") = (long)m;
    register long a7 __asm__("a7") = KERNEL_CALL_TRAP_NR;
    __asm__ __volatile__("ecall" : "+r"(a0) : "r"(a7) : "memory", "a1","a2","a3","a4","a5");
    return a0;
}
#elif defined(__aarch64__)
static long kcall(struct kmsg *m) {
    register long x0 __asm__("x0") = (long)m;
    register long x8 __asm__("x8") = KERNEL_CALL_TRAP_NR;
    __asm__ __volatile__("svc #0" : "+r"(x0) : "r"(x8) : "memory", "x1","x2","x3","x4","x5");
    return x0;
}
#else
#error "sys-bridge.c: 未支持的架构（需与 os/libs/minix-sys/src/arch_trap.rs 的 kernel_call_trap 同形）"
#endif

/* 一次 IPC SendRec（同步呼叫等回执）：腿对位 arch_trap.rs ipc_trap
 * （riscv: a0=端点/a1=消息/a7=呼叫号；aarch64: x0/x1/x8 同形），回码在
 * 第一返回寄存器（errno，0=OK）。非 static：posix-stubs.c 的 VFS 族
 * （access 等）经本腿复用，避免第二份 trap 内联汇编漂移。
 * 返回：0=往返成功（回执已写回 m 的 m_u，调用方按 C 协议读回复字段），
 * 非 0=errno（正数）。 */
#if defined(__riscv)
long ipc_sendrec(int endpoint, struct kmsg *m) {
    register long a0 __asm__("a0") = (long)endpoint;
    register long a1 __asm__("a1") = (long)m;
    register long a7 __asm__("a7") = IPC_SENDREC_NR;
    __asm__ __volatile__("ecall" : "+r"(a0), "+r"(a1) : "r"(a7) : "memory", "a2","a3","a4","a5");
    return a0;
}
#elif defined(__aarch64__)
long ipc_sendrec(int endpoint, struct kmsg *m) {
    register long x0 __asm__("x0") = (long)endpoint;
    register long x1 __asm__("x1") = (long)m;
    register long x8 __asm__("x8") = IPC_SENDREC_NR;
    __asm__ __volatile__("svc #0" : "+r"(x0), "+r"(x1) : "r"(x8) : "memory", "x2","x3","x4","x5");
    return x0;
}
#endif

/* DIAGBUFSIZE=128：内核 dispatch_diagctl(code=1) 硬拒 len>128，故分块循环。 */
int _write(int fd, const void *buf, size_t_local len) {
    (void)fd;
    const unsigned char *p = (const unsigned char *)buf;
    int done = 0;
    while (done < (int)len) {
        int chunk = (int)len - done > 128 ? 128 : (int)len - done;
        struct kmsg m = {0};
        m.m_type = SYS_DIAGCTL;
        m.slots[0] = DIAGCTL_CODE_DIAG;
        m.slots[1] = (long long)(long)(p + done);
        m.slots[2] = chunk;
        if (kcall(&m) != 0) { if (done == 0) { errno = EIO; return -1; } break; }
        done += chunk;
    }
    return done;
}

/* 退出腿，对位 C minix3/minix/lib/libc/sys/_exit.c:12-30：
 * 主腿 = _syscall(PM, PM_EXIT, &m)（IPC SendRec 给 PM，PM 走回收/通知父进程
 * waitpid 的簿记链；本进程正常不返回）。兑底 = C 同注释场景（PM 死锁时
 * suicide）：我方用 kernel_call SYS_EXIT 触发内核 do_exit→cause_sig(SIGABRT)
 * （对位 PM 代发的低层腿；内核 NoReply 腿续-277 已按 C EDONTREPLY 契约补齐）；
 * 仍不济则 __builtin_trap() 可定位卡死。旧版直发 SYS_EXIT 跨过了 PM 簿记，
 * 父 waitpid 永远刷不到子退出——真机取证见 WORKLOG §续-277。 */
void _exit(int code) {
    struct kmsg m = {0};
    m.m_type = PM_CALL_EXIT;
    m.slots[0] = code;              /* m_lc_pm_exit.status @m_u+0（ipc.h 同形） */
    (void)ipc_sendrec(PM_ENDPOINT, &m);

    struct kmsg k = {0};            /* 兑底：内核 do_exit（C suicide 腿的我方同义形） */
    k.m_type = SYS_EXIT;
    k.slots[0] = code;
    (void)kcall(&k);

    /* 两腿都失效：用可定位的非法指令陷阱卡住现场（aarch64 编成 brk、
     * riscv 编成未定义指令→进各自 trap 诊断腿），而非静默忙等。 */
    __builtin_trap();
}

/* 堆增长腿的位置说明（§续-279m 定谳）：picolibc 配置下 malloc→sbrk（标准
 * POSIX 名，非 newlib 内部 _sbrk 路），故真腿定义在 posix-stubs.c（抢
 * libsemihost 假 sbrk）；本 TU 不再提供 _sbrk（旧 bump 堆与其后继定义均
 * 不在链路上，nm 实测零引用后删除—20 基线的 bump 堆能跑是因它把 .bss
 * 堆预映射在内，假 sbrk 同机制的 semihost 版）。旧注释存档： */

/* ③ 首片：文件类 syscall 先给最小实现，够链；上机跑 open/read 由后续 VFS 桥补。 */
int _close(int fd) { (void)fd; return -1; }
int _fstat(int fd, struct stat *st) { (void)fd; if(st){/*zeroed*/} return -1; }
int _isatty(int fd) { if (fd == 1 || fd == 2) return 1; errno = ENOTTY; return 0; }
int _lseek(int fd, int off, int whence) { (void)fd;(void)off;(void)whence; return 0; }
int _read(int fd, void *buf, int len) { (void)fd;(void)buf;(void)len; return -1; }
int _getpid(void) { return 1; }
int _kill(int p, int s) { (void)p;(void)s; return -1; }

/* --- POSIX 层（libatf-c 自身引用；③ 首片最小实现，够链；上机跑 open/read
 *    的 VFS 桥在后续补）--- */
#include <stdarg.h>
int open(const char *path, int flags, ...) { (void)path; (void)flags; return -1; }
int close(int fd) { (void)fd; return -1; }
int geteuid(void) { return 0; }
int getgroups(int size, void *list) { (void)size; (void)list; return 0; }
