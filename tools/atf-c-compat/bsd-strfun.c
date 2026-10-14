/* bsd-strfun.c — BSD 字符串族 + 位计数族的实现（目标③ 构建/链接面，不进生产镜像）。
 *
 * 真源逐件对位：
 *   minix3/common/lib/libc/string/popcount32.c   popcount32（NetBSD 混合位计数）
 *   minix3/common/lib/libc/string/popcount64.c   popcount64（同族 64 位形）
 *   minix3/lib/libc/string/stresep.c             stresep（BSD strsep 基 + esc 处理）
 * 原型契约见 bsd-strfun.h（返回 unsigned int，不是 int）。
 *
 * 与真源的两处**形**之别（语义零改动，逐条说明在此）：
 * 1. 别名机制：Minix3 用 NetBSD 的 `__strong_alias(popcount, popcount32)`（在链接期
 *    造第二个名字）。宿主 glibc 与 picolibc 都不提供 `__strong_alias`，这里改写成
 *    同门的转发函数——两段机器码在 `-Os` 下都只是一次跳转，可观察行为（返回值、
 *    符号名可解析）完全一致；映射条件仍照抄真源的 `#if UINT_MAX/ULONG_MAX/
 *    ULLONG_MAX` 门，所以在 32 位目标上 `popcountl` 落到 popcount32、在 64 位目标上
 *    落到 popcount64，与 Minix3 同形。
 * 2. 真源 popcount64.c 末尾的 `#if defined(__minix) __strong_alias(popcountti2,
 *    popcount64)` 不带过来：`popcountti2` 是 Minix3 给 libkern 与 compiler-rt 对齐
 *    用的内部名，不在 ATF 测试套件的符号需求内（`t_popcount.c` 只引用
 *    `popcount`/`popcountll`）。注意本 TU 编译时带有 `-D__minix`（取自 CFGS），
 *    照抄那条 #if 会拽出一个无人引用的符号，属纯粹的表面积扩张，故显式舍弃。
 *
 * `#ifndef popcount32/popcount64` 两道门照抄真源（原注释：might be a builtin）——
 * 万一将来某个 libc 把名字定义成宏内建，本实现自动让位，与 Minix3 同规矩。 */

#include <limits.h>
#include <stddef.h>   /* NULL：stresep 的空指针判据用 */
#include <stdint.h>
/* 不带 <string.h>：stresep 的重叠前移已改成自给循环（理由见那里），
 * 本 TU 不再调用任何 string.h 成员。 */

#include "bsd-strfun.h"

#ifndef popcount32    /* might be a builtin（真源同门） */

/*
 * 混合位计数：并行计数与乘法之间取中——先把每个字节内的位数加总，最后用
 * 一次乘法完成累加（以下逐字对位 popcount32.c 的注释与算法）。
 */
unsigned int
popcount32(uint32_t v)
{
	unsigned int c;

	v = v - ((v >> 1) & 0x55555555U);
	v = (v & 0x33333333U) + ((v >> 2) & 0x33333333U);
	v = (v + (v >> 4)) & 0x0f0f0f0fU;
	c = (v * 0x01010101U) >> 24;
	/*
	 * v = (v >> 16) + v;
	 * v = (v >> 8) + v;
	 * c = v & 255;
	 */

	return c;
}

#if UINT_MAX == 0xffffffffU
unsigned int
popcount(unsigned int v)
{
	return popcount32(v);
}
#endif

#if ULONG_MAX == 0xffffffffU
unsigned int
popcountl(unsigned long v)
{
	return popcount32((uint32_t)v);
}
#endif

#endif	/* !popcount32 */

#ifndef popcount64    /* might be defined to use a __builtin（真源同门） */

/*
 * 若 uint64_t 比 size_t 宽，则拆成两个 32 位半相加更快；原生 64 位形与
 * popcount32(3) 同思路（注释对位 popcount64.c）。
 */
#if SIZE_MAX < 0xffffffffffffffffULL
unsigned int
popcount64(uint64_t v)
{
	return popcount32((uint32_t)(v >> 32)) +
	    popcount32((uint32_t)(v & 0xffffffffULL));
}
#else
unsigned int
popcount64(uint64_t v)
{
	unsigned int c;

	v = v - ((v >> 1) & (uint64_t)0x5555555555555555ULL);
	v = (v & (uint64_t)0x3333333333333333ULL) +
	    ((v >> 2) & (uint64_t)0x3333333333333333ULL);
	v = ((v + (v >> 4)) & (uint64_t)0x0f0f0f0f0f0f0f0fULL) *
	    (uint64_t)0x0101010101010101ULL;
	c = (unsigned int)(v >> 56);

	return c;
}
#endif

#if ULONG_MAX == 0xffffffffffffffffULL
unsigned int
popcountl(unsigned long v)
{
	return popcount64((uint64_t)v);
}
#endif

#if ULLONG_MAX == 0xffffffffffffffffULL
unsigned int
popcountll(unsigned long long v)
{
	return popcount64((uint64_t)v);
}
#endif

#endif	/* !popcount64 */

/* BSD stresep——对位 minix3/lib/libc/string/stresep.c 移植（BSD strsep 基 +
 * esc 处理：命中 esc 则就地删 esc 并字面化下一字符；未转义分隔符写 NUL 断句；
 * 串尽返 NULL 并置 *stringp=NULL；省 _DIAGASSERT/__weak_alias，余逐行同）。
 * 本件从 posix-stubs.c 搬来（批次二百六十四实测 glibc **没有** stresep，把它记在
 * 「glibc 本就有」那份理由里是错的）；声明与定义同签名的教训见 bsd-strfun.h 头注
 * （CodeReview 续-279f P0）。
 *
 * **一处必要的等价改写**（不是风格问题，是可观察行为问题）：真源用
 * `strcpy(s - 1, s)` 做「位移一个字节」的重叠复制。本仓 C 真源里 strcpy 是**前向逐
 * 字节**循环（minix3/common/lib/libc/string/strcpy.c:58 的
 * `for (; (*to = *from) != '\0'; ++from, ++to);`），picolibc（newlib 血缘）同形，所以重叠区
 * 在两条交叉腿上是正确的。glibc **不保证**这一点（标准里重叠即 UB）：拿同一个
 * t_stresep.c 的输入在宿主上跑，`-O2` 得
 * `[foo  bar] [baz] [bar foobar  foo] [   baz] [bbaar\] [ ]`，`-O0` 又是**另一种**损坏
 * （`[bar foobar  fooo] [\] [  baz] [bar\] [ ]`）——两种读数不同正是 UB 的形，不是
 * 「另一个可以对齐的固定行为」。所以这里写成同形的显式前向逐字节前移：行为等于
 * 本仓 C 真源的 strcpy，且在标准内有定义，三腿读数一致。
 * 真值门：x86_64 腿把 t_stresep 跑在宿主上，六个 `expect()` 全过
 * （minix3/tests/lib/libc/string/t_stresep.c:58-63）。 */

/* 与 minix3 strcpy 的循环一字同形；指针按值传，调用侧的 s 不受影响。 */
static void
bsd_shift_left1(char *dst, const char *src)
{
	while ((*dst++ = *src++) != '\0')
		;
}

char *
stresep(char **stringp, const char *delim, int esc)
{
	char *s;
	const char *spanp;
	int c, sc;
	char *tok;

	if ((s = *stringp) == NULL)
		return NULL;
	for (tok = s;;) {
		c = *s++;
		while (esc != '\0' && c == esc) {
			bsd_shift_left1(s - 1, s);
			c = *s++;
		}
		spanp = delim;
		do {
			if ((sc = *spanp++) == c) {
				if (c == 0)
					s = NULL;
				else
					s[-1] = 0;
				*stringp = s;
				return tok;
			}
		} while (sc != 0);
	}
}
