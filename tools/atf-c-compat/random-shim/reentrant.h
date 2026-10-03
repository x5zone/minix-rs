/* random-shim/reentrant.h — vendored random.c 的目标编译壳（NK4-C §续-283）。
 * 语义对位真源 _KERNEL 分支的 no-op mutex 宏；_REENTRANT 不定义
 * （build-atf-test.sh CFGS 无此宏），random_mutex 声明被守卫消元、
 * 使用点被吞参宏消元。 */
#ifndef RANDOM_SHIM_REENTRANT_H
#define RANDOM_SHIM_REENTRANT_H
/* picolibc sys/cdefs.h 若定义 __weak_alias：真源头部 __weak_alias(random,_random)
 * 等会把 picolibc 归档的 _random 拉进符号决议，破坏本文件的抢占语义——削掉
 * （真源 :33 先含 <sys/cdefs.h>，本文件 :47 后含，undef 在其后生效）。 */
#undef __weak_alias
/* picolibc cdefs 不定义 _DIAGASSERT（真源 initstate/setstate 用）。 */
#ifndef _DIAGASSERT
#define _DIAGASSERT(e)	((void)0)
#endif
#define mutex_lock(a)	(void)0
#define mutex_unlock(a)	(void)0
#endif
