/* NK4-C 目标③：把 picolibc tinystdio 的输出腿接我方 kernel_call 桥。
 *
 * 机制：picolibc 的 stdio 流对象（stdout/stderr/stdin）由 libsemihost.a 的
 * iob.c.o 定义，其每字符读写回调是 sys_semihost_putc/getc——semihost 版实现
 * 走宿主调试通道（aarch64 brk / riscv 自定义 trap 序列），我方内核 EL0 不可
 * 达。本 TU 抢定义这两个符号：链接顺序上本 .o 在 libsemihost.a 之前，archive
 * 成员只为**尚未定义**的符号拉入，故 iob.c.o 的引用解析到这里，而 semihost
 * 的 putc/getc 实现成员不进镜像（无死代码，链接器已裁）。
 * 输出经 sys-bridge.c _write → SYS_DIAGCTL → 内核串口，即 atf 测试报告上机
 * 的显示通道。
 *
 * 原型对位 picolibc 1.8.6 include/semihost.h（两架构同签名，实测）：
 *   int sys_semihost_getc(FILE *file);
 *   int sys_semihost_putc(char c, FILE *file);
 *
 * 诚实边界：逐字符一次 trap（每字符一次 SYS_DIAGCTL）对首跑足够（atf 单例
 * 报告数百字节）；批量合并（128B DIAGBUFSIZE 分块已在 _write 层做）待上机
 * 观测串口吞吐后再按需优化。仅 ③ 构建/上机用；不参与生产镜像。 */
#include <stdio.h>

/* sys-bridge.c 定义（riscv/aarch64 共用同一 C 签名，避免跨 TU ODR 不一致）。 */
extern int _write(int fd, const void *buf, size_t len);

/* stdin/stdout/stderr 由 <stdio.h> 声明（picolibc tinystdio：`FILE *const`，
 * iob.c.o 定义）；本 TU 只用其值比较流身份，不重声明（重声明与头冲突）。 */

int sys_semihost_putc(char c, FILE *file) {
    /* fd 语义对齐 POSIX：stdout=1、stderr=2；其余流按 stdout 处理。
     * 返回 0=成功（semihost 惯例；诊断串口无背压语义，写入失败也吞掉——
     * 测试报告丢字符的代价小于在 putc 里再造错误处理链）。 */
    int fd = (file == stderr) ? 2 : 1;
    char b = c;
    (void)_write(fd, &b, 1);
    return 0;
}

int sys_semihost_getc(FILE *file) {
    (void)file;
    /* EOF：首跑上机面没有 stdin 道（tty 交互桥属 ③ 后续）；atf 单测 body
     * 不读输入（读输入的属 atf_check -I 场景，未进首批 18 案）。 */
    return -1;
}
