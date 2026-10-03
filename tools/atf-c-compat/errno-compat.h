/* errno-compat.h — 为构建/链接面补 picolibc 缺的 BSD 声明（经
 * build-atf-test.sh 以 -include 注入到测试编译）。
 *
 * 定义见同目录 errno-compat.c（sys_nerr）与 posix-stubs.c（stresep/sysconf）。
 * 声明必须与定义同签名：picolibc 头面对 stresep **零声明**（unistd.h/string.h
 * 均无，两 arch 实测），调用侧隐式声明按 int 返回——aarch64 目标码用
 * `sxtw w0` 截返回指针成野地址（CodeReview 续-279f P0 实测 t_stresep 二进制
 * 坐实）；sysconf 在 picolibc sys/unistd.h 已有同签名声明（long sysconf(int)），
 * 本处并列声明是同签名合法重声明，作测试 TU 的显式契约面统一。
 * 诚实边界见 errno-compat.c / posix-stubs.c 头注释。*/
#ifndef MINIX_RS_ERRNO_COMPAT_H
#define MINIX_RS_ERRNO_COMPAT_H

#include <stddef.h>   /* size_t：本头经 -include 注入在测试 TU 所有 #include 之前，须自给 */

extern const int sys_nerr;

/* BSD 字符串族（定义在 posix-stubs.c，对位 minix3/lib/libc/string/stresep.c
 * 真源；参数 int esc 同定义签名，char 实参提升为 int 与 C 原型兼容）。 */
char *stresep(char **stringp, const char *delim, int esc);

/* POSIX 配置查询（定义在 posix-stubs.c，抢 libsemihost 强定义；picolibc
 * sys/unistd.h 已有同签名声明，此处为契约面统一的合法重声明）。 */
long sysconf(int name);

/* strerror 族接管面（定义在 posix-stubs.c，§续-279m）：本仓 compat 全接管
 * 越界语义（BSD 形 "Unknown error: N"+EINVAL），测试 TU 经本通道看到与定义
 * 同签名的契约面；strerror 与 picolibc string.h:72 同签名=合法重声明。
 * strerror_r 用宏转发到本仓 XSI 定义（objdump 实锤测试调用是裸名
 * strerror_r，必须让**所有** TU 的裸名都落到本仓实现；若只挂 asm 别名，
 * 裸引用不满足，链接器会经 picolibc strerror_r 成员的 U _strerror_r 拽入
 * libc_string_strerror.c.o——其定义的 strerror 与自家强定义 multiple
 * definition 硬错，三轮试错坐实）。本宏在 string.h 声明之前注入：其 GNU 形
 * 声明 `char *strerror_r(...)` 被一并改名成 __xpg_strerror_r（与 posix-stubs.c
 * 定义同形，无类型冲突）；调用点保留 §续-278d 既有比较告警基线，XSI 返回码
 * 0/EINVAL/ERANGE 的零/非零性在指针/整数值域同形，真机读数定案。 */
char *strerror(int);
#ifndef strerror_r
#define strerror_r __xpg_strerror_r
#endif

#endif /* MINIX_RS_ERRNO_COMPAT_H */
