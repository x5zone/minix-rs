/* NK4-C 目标③：C 测试的 syscall 桥（跨架构单一实现）——把 picolibc 的 sysstub
 * (_write/_exit/_sbrk/_read/_close/_lseek/_fstat/_isatty/_getpid) 映射到我方
 * kernel_call ABI，对位 os/libs/minix-sys/src/arch_trap.rs（单一真源）：
 *   riscv64 ：`ecall` a7=KERNEL_CALL_TRAP_NR(0), a0=&Message, 回码在 a0
 *   aarch64 ：`svc #0` x8=KERNEL_CALL_TRAP_NR(0), x0=&Message, 回码在 x0
 * 仅 ③ 构建/链接用；不参与 minix-rs 生产镜像。
 * Message 布局（minix-types ipc/message.rs）：m_source i32@0, m_type i32@4, m_u@8。
 * 常量（ipc/kernel_call.rs）：SYS_DIAGCTL=0x62c, SYS_EXIT=0x635, DIAGCTL_CODE_DIAG=1。 */
#include <sys/types.h>
#include <sys/stat.h>
#include <errno.h>

#define SYS_DIAGCTL 0x62c
#define SYS_EXIT    0x635
#define DIAGCTL_CODE_DIAG 1
#define KERNEL_CALL_TRAP_NR 0

/* picolibc _write 第三参为 size_t；手工 typedef 避免与已有 sys stub 头冲突。 */
typedef unsigned long size_t_local;

/* 复用 C 侧 Message：前 8 字节 m_source/m_type，其后按 diagctl 载荷 code@8,buf@16,len@24。
 * Message 载荷共 56B（7 个 8 字节槽）。 */
struct kmsg {
    int m_source;
    int m_type;
    long long slots[7]; /* m_u：[0]=code(i32@8),[1]=buf@16,[2]=len@24 */
};

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

void _exit(int code) {
    struct kmsg m = {0};
    m.m_type = SYS_EXIT;
    m.slots[0] = code;
    kcall(&m);
    /* SYS_EXIT 正常不返回；若内核拒收，用可定位的非法指令陷阱卡住现场
     * （aarch64 编成 brk、riscv 编成未定义指令→进各自 trap 诊断腿），
     * 而非静默无限忙等（整板不可解释地卡住）。 */
    __builtin_trap();
}

/* 静态 bump 堆（供 malloc；上机时由我方 VM 供页）。 */
static char heap[1 << 20];
static size_t heap_off;
void *_sbrk(intptr_t inc) {
    if (inc < 0) {
        if ((intptr_t)heap_off < -inc) { errno = ENOMEM; return (void *)-1; }
        heap_off += (size_t)inc;
        return heap + heap_off;
    }
    size_t aligned = (heap_off + 15u) & ~(size_t)15u;  /* max_align_t=16 */
    if (aligned + (size_t)inc > sizeof(heap)) { errno = ENOMEM; return (void *)-1; }
    void *p = heap + aligned; heap_off = aligned + (size_t)inc; return p;
}

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
