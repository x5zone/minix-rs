/* BSD `<sys/param.h>` 补位面 —— 只服务宿主参照件构建，不参与任何目标构建。
 *
 * 与同目录 `cdefs.h` 同一规矩：先用 `#include_next` 把 glibc 那份接住，再补 BSD 特有
 * 而 glibc 没有的拼写。glibc 的 `<sys/param.h>` 有 `MIN`/`MAX`/`ROUNDUP`，但没有
 * BSD 的缓冲区尺寸类常量。
 */

#ifndef _HOST_REF_SYS_PARAM_H
#define _HOST_REF_SYS_PARAM_H

#include_next <sys/param.h>

/* `minix3/usr.bin/split/split.c:179` 与 `:249` 用它定读缓冲。真值出处是本仓 vendor 的
 * 兼容定义头 `minix3/tools/compat/compat_defs.h:1150-1151`（`#ifndef MAXBSIZE`
 * 再 `#define MAXBSIZE (64 * 1024)`）——与片甲/片乙落 Rust 侧时取的块大小同一个数。 */
#ifndef MAXBSIZE
#define MAXBSIZE (64 * 1024)
#endif

/* BSD 的「默认文件模式」在 `<sys/stat.h>`，`MAXBSIZE` 一族的尺寸常量在这边。
 * `sectrb`/`btodb` 这类块地址换算宏宿主参照件用不到，等真有用到再补，不预先摊开。 */

#endif /* _HOST_REF_SYS_PARAM_H */
