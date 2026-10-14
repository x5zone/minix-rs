/* bsd-strfun.h — BSD 字符串族 + 位计数族的契约面（stresep(3) 与 popcount(3) 家族）。
 *
 * 为什么单独成件（台账 P-X86-01 缺口① 的余两例）：这两个家族的符号 **glibc 和
 * picolibc 都没有**——`stresep` 是 BSD 的「带转义符的 strsep」，`popcount/popcountl/
 * popcountll/popcount32/popcount64` 是 BSD 的位计数家族（glibc 只有编译器内建
 * `__builtin_popcount*`，没有可链接的同名库函数；`-static` 组内补 `-lgcc` 也补不上，
 * 批次二百六十六实测排除过这个猜测）。原先它们埋在 posix-stubs.c 里，而 posix-stubs.c
 * 是 picolibc 专用的整层接管面（同一 TU 还强定义 sysconf/strerror/strerror_r），
 * x86_64 腿要拿到这三个名字就得把整层挂回去、连带改掉宿主 libc 的语义。台账要求的
 * 「按符号拆开的 compat 面」就是本件。
 *
 * 定义见同目录 bsd-strfun.c；原型逐条对位本仓 C 真源：
 *   minix3/include/string.h:103   char *stresep(char **, const char *, int);
 *   minix3/include/strings.h:57-61 popcount/popcountl/popcountll/popcount32/popcount64
 * 全部返回 `unsigned int`（**不是 int**）。本头经 build-atf-test.sh 以 `-include` 注入
 * 到测试 TU 与所有系统头之前：picolibc/glibc 头面对这些名字零声明，缺声明则调用侧按
 * 隐式 int 处理——aarch64 上 `sxtw w0` 把返回指针截成野地址（CodeReview 续-279f P0
 * 实测 t_stresep 旧二进制坐实），位计数家族的 `unsigned int` 返回值同样会被截形。
 * 声明与定义必须同签名，跨 TU 漂移过一次就不再是「链接问题」而是「读到脏高位」。 */
#ifndef MINIX_RS_BSD_STRFUN_H
#define MINIX_RS_BSD_STRFUN_H

#include <stdint.h>   /* uint32_t/uint64_t：本头注入在所有 #include 之前，须自给 */

/* BSD stresep：取 *stringp 的下一段，分隔符为 delim，esc 非 NUL 时其后一字符
 * 被字面化（不计入分隔）。定义对位 minix3/lib/libc/string/stresep.c。 */
char *stresep(char **stringp, const char *delim, int esc);

/* BSD popcount 家族（定义对位 minix3/common/lib/libc/string/popcount32.c、
 * popcount64.c）：宽度特化的两个是「实现」，按整数类型取名的三个是它们
 * 的别名（映射规则由 limits.h 的 UINT_MAX/ULONG_MAX/ULLONG_MAX 决定，
 * 与 Minix3 的 __strong_alias 门一致）。 */
unsigned int popcount(unsigned int v);
unsigned int popcountl(unsigned long v);
unsigned int popcountll(unsigned long long v);
unsigned int popcount32(uint32_t v);
unsigned int popcount64(uint64_t v);

#endif /* MINIX_RS_BSD_STRFUN_H */
