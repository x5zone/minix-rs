/* BSD <sys/cdefs.h> 补位面 —— 只服务宿主参照件构建，不参与任何目标构建。
 *
 * 为什么单开一个目录而不是往 ../sys/ 里加：../sys/ 那几枚头是交叉编译 ATF 测试时
 * 喂给 picolibc 的，被目标端的 `-I` 选中；本文件由宿主的 glibc 端 `-I` 选中，两拨
 * 搜索路径互不相交，改哪一侧都不会让另一侧的门变红。
 *
 * 为什么不 `-I minix3/include`：那半套 vendored 头与宿主头是互相指错的——glibc 的
 * `<sys/param.h>` 一被拉进来就顺着 include 链撞上 `minix3/include/stddef.h:38` 的
 * `#include <sys/featuretest.h>`，而本仓并没有 `sys/featuretest.h`（判据见
 * `rewrite-notes/18-stage-commands/todo.md` 批次三百六十六的两条 `ls` 读数）。参照件
 * 要的只是 BSD 特有的那几个宏与函数名，标准 C 与 POSIX 的部分一律交给 glibc。
 *
 * 用 `#include_next` 而不是整份自写：glibc 的 `<sys/cdefs.h>` 定义了 `__THROW`、
 * `__nonnull`、`__attribute_malloc__` 等一整套内部拼写，自家头的每个声明都要用它们；
 * 自己另写一份等于替 glibc 决定这些拼写是什么，那会在任何一次 libc 升级后失真。
 */

#ifndef _HOST_REF_SYS_CDEFS_H
#define _HOST_REF_SYS_CDEFS_H

#include_next <sys/cdefs.h>

/* NetBSD 的版权与 RCS 标识：真源里写成 `__COPYRIGHT("...")` 这种「像函数调用、实为
 * 一条带初值的 static 声明」的形式，所以补面必须展开成声明而不是空替换——留等号是
 * 让 `__IDSTRING(n, s)` 展开成 `static const char n[] = s;` 后仍是一条完整声明。
 * 这套写法由批次二百六十四定形。 */
#ifndef __IDSTRING
#define __IDSTRING(naming, string) \
	static const char naming[] __unused = string
#endif

#ifndef __COPYRIGHT
#define __COPYRIGHT(string) __IDSTRING(__rcsid_copyright, string)
#endif

#ifndef __RCSID
#define __RCSID(string) __IDSTRING(__rcsid_ident, string)
#endif

#ifndef __BUGSRC
#define __BUGSRC(string) __IDSTRING(__rcsid_bugs, string)
#endif

#ifndef __LIBLEGACYID
#define __LIBLEGACYID(string) __IDSTRING(__rcsid_legacy, string)
#endif

/* 同一枚 `__IDSTRING` 被用在已声明的标识上时，BSD 侧只挂一次；这里两枚 naming 各自
 * 固定，若同一编译单元里出现两枚 `__COPYRIGHT` 会撞同名——那是参照件构建才会遇到的
 * 形状，撞了就靠下面这几枚「整族退化成空」的开关兜住。 */
#ifndef __RCSIDED
#define __RCSIDED(x)
#endif

/* BSD 侧的「不可到达」「未使用」「数组长度」三枚小拼写。glibc 的 cdefs 没有它们。 */
#ifndef __dead
#define __dead __attribute__((__noreturn__))
#endif

#ifndef __unused
#define __unused __attribute__((__unused__))
#endif

#ifndef __arraycount
#define __arraycount(a) (sizeof(a) / sizeof((a)[0]))
#endif

/* BSD 的 `__printflike(fmtarg, firstvararg)` 在 glibc 里叫 `__format__(arch_sprintf, ...)`；
 * 参照件不需要这层检查，展成空即可，硬翻成 glibc 的拼写反而容易在属性名上失真。 */
#ifndef __printflike
#define __printflike(a, b)
#endif

#ifndef __printf0like
#define __printf0like(a, b)
#endif

#ifndef __alloc_size
#define __alloc_size(a)
#endif

#endif /* _HOST_REF_SYS_CDEFS_H */
