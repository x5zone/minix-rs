/* errno-compat.h — 为构建/链接面补 picolibc 缺的 BSD 声明（经
 * build-atf-test.sh 以 -include 注入到测试编译）。
 *
 * 定义见同目录 errno-compat.c（sys_nerr）与 posix-stubs.c（stresep/sysconf）。
 * 声明必须与定义同签名：picolibc 头面对 stresep 零声明、对 sysconf 也缺
 * （unistd.h 无），调用侧隐式声明按 int 返回——aarch64 目标码用 `sxtw w0`
 * 截返回指针成野地址（CodeReview 续-279f P0 实测 t_stresep 二进制坐实）；
 * 同族 sysconf 的 int 截断对 4096 数值并态无害仍属 UB，一并声明。
 * 诚实边界见 errno-compat.c / posix-stubs.c 头注释。*/
#ifndef MINIX_RS_ERRNO_COMPAT_H
#define MINIX_RS_ERRNO_COMPAT_H

extern const int sys_nerr;

/* BSD 字符串族（定义在 posix-stubs.c，对位 minix3/lib/libc/string/stresep.c
 * 真源；参数 int esc 同定义签名，char 实参提升为 int 与 C 原型兼容）。 */
char *stresep(char **stringp, const char *delim, int esc);

/* POSIX 配置查询（定义在 posix-stubs.c，抢 libsemihost 强定义）。 */
long sysconf(int name);

#endif /* MINIX_RS_ERRNO_COMPAT_H */
