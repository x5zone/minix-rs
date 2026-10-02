/* errno-compat.h — 为构建/链接面补 BSD `sys_nerr` 声明（picolibc 不提供）。
 * 定义见同目录 errno-compat.c。经 build-atf-test.sh 以 -include 注入到测试编译，
 * 使引用 `sys_nerr` 的 t_strerror 等测试可编译。诚实边界见 errno-compat.c 头注释。*/
#ifndef MINIX_RS_ERRNO_COMPAT_H
#define MINIX_RS_ERRNO_COMPAT_H

extern const int sys_nerr;

#endif /* MINIX_RS_ERRNO_COMPAT_H */
