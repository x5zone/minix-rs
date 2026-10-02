/* NK4-C 目标③：picolibc(riscv64-unknown-elf) 缺 <sys/uio.h>。
 * atf-c/tc.c 需 struct iovec + writev。readv 由 picolibc 既有声明提供
 * （避免重复定义冲突），故此头仅补 struct iovec 与 writev 原型。
 * writev 实现在链接期由 runtime shim 提供。仅构建期用。 */
#ifndef ATF_COMPAT_SYS_UIO_H
#define ATF_COMPAT_SYS_UIO_H
#include <sys/types.h>
#ifndef _UIO_VEC_
#define _UIO_VEC_
struct iovec { void *iov_base; size_t iov_len; };
#endif
ssize_t writev(int fd, const struct iovec *iov, int iovcnt);
#endif
