/* errno-compat.h — 为构建/链接面补 picolibc 缺的声明（经 build-atf-test.sh
 * 以 -include 注入到测试编译），**只服务 picolibc 腿**。
 *
 * 本头现在只剩 sysconf 一条契约：定义在 posix-stubs.c（本头与那个文件都只在
 * picolibc 腿挂）。声明必须与定义同签名：picolibc 头面对本处名字是零声明或
 * 弱声明，调用侧隐式声明按 int 返回——aarch64 目标码用 `sxtw w0` 截返回指针成
 * 野地址（CodeReview 续-279f P0 实测坐实）；sysconf 在 picolibc sys/unistd.h 已有
 * 同签名声明（long sysconf(int)），本处并列声明是同签名合法重声明，作测试 TU
 * 的显式契约面统一。
 * 已搬出去的两族：BSD 字符串/位计数族→bsd-strfun.h，BSD strerror 族→bsd-strerr.h；
 * 它们都属于「glibc 与 picolibc 都没有」的补位件，走三腿共用的注入通道，
 * 因此不能留在本头这张 picolibc 专用清单里。诚实边界见 posix-stubs.c /
 * bsd-strerr.c / bsd-strfun.c 头注释。*/
#ifndef MINIX_RS_ERRNO_COMPAT_H
#define MINIX_RS_ERRNO_COMPAT_H

#include <stddef.h>   /* size_t：本头经 -include 注入在测试 TU 所有 #include 之前，须自给 */

/* BSD strerror 族（sys_nerr / strerror / strerror_r / strerror_l）与 stresep/位计数族同命：
 * glibc 与 picolibc 都不给那套 BSD 语义，所以走 bsd-strerr.h / bsd-strfun.h 那两条三腿
 * 共用的注入通道，不在本头这张 picolibc 专用清单里。 */

/* POSIX 配置查询（定义在 posix-stubs.c，抢 libsemihost 强定义；picolibc
 * sys/unistd.h 已有同签名声明，此处为契约面统一的合法重声明）。 */
long sysconf(int name);

#endif /* MINIX_RS_ERRNO_COMPAT_H */
