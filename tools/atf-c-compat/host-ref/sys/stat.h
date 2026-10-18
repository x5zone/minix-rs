/* BSD `<sys/stat.h>` 补位面 —— 只服务宿主参照件构建。
 *
 * 结构体与 `fstat`/`stat` 一律交给 glibc（这边不做 `#include_next` 之外的任何改写），
 * 只补 BSD 特有的「新建文件默认模式」常量。
 */

#ifndef _HOST_REF_SYS_STAT_H
#define _HOST_REF_SYS_STAT_H

#include_next <sys/stat.h>

/* BSD 定义 `DEFFILEMODE` 为 0666（新建档在 umask 之前的默认模式）；
 * `minix3/usr.bin/split/split.c:332` 的 `open(fname, O_WRONLY|O_CREAT|O_TRUNC, DEFFILEMODE)`
 * 用的就是它。glibc 没有这个名字。 */
#ifndef DEFFILEMODE
#define DEFFILEMODE 0666
#endif

#endif /* _HOST_REF_SYS_STAT_H */
