/* 参照件构建统一 force-include 的原型补面（由 `tools/build-bsd-ref.sh` 以 `-include` 挂上）。
 *
 * 只放「BSD 有、宿主 libc 到某个版本才有」的函数原型，不放任何宏——宏一律走 `-D`，
 * 这样源码里自带的 `#ifndef` 守卫（`fortune.c:186` 的 `#ifndef NAMLEN` 即是一例）仍由
 * 源码自己决定是否接受补值，补面不越过源码的门槛去改写它的定义。
 */

#ifndef _HOST_REF_BSD_COMPAT_PROTO_H
#define _HOST_REF_BSD_COMPAT_PROTO_H

#include <sys/types.h>
#include <features.h>
#include <stdio.h>

/* BSD 的 `fgetln(3)`：glibc 从未提供（它只有 `getline`/`getdelim`），而
 * `minix3/usr.bin/uniq/uniq.c` 的行读入口就是它。实现见同目录 `bsd-fgetln.c`，
 * 那件带同样的 glibc 判据。 */
#if defined(__GLIBC__)
char *fgetln(FILE *stream, size_t *len);
#endif

/* 有些 BSD 源直接调 `fstat`/`stat` 而不含 `<sys/stat.h>`（NetBSD 的包含链把这层替它做掉，
 * `minix3/usr.bin/split/split.c:293` 正是这种：它只含到 `<sys/param.h>` 与 `<unistd.h>`）。
 * 宿主上这会退化成隐式声明，`struct stat` 也拿不全，于是把这一枚头统一挂在补面上——
 * 补面里的 `sys/stat.h` 先接住 glibc 的原头，再补 BSD 的 `DEFFILEMODE`。 */
#include <sys/stat.h>

#if defined(__GLIBC__) && defined(__GLIBC_PREREQ)
#if !__GLIBC_PREREQ(2, 40)
const char *getprogname(void);
void setprogname(const char *);
#endif
#else
const char *getprogname(void);
void setprogname(const char *);
#endif

#endif /* _HOST_REF_BSD_COMPAT_PROTO_H */
