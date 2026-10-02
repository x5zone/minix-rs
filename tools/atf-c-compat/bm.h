#ifndef _ATF_C_COMPAT_BM_H
#define _ATF_C_COMPAT_BM_H
/*
 * tools/atf-c-compat/bm.h — 目标③ 构建面：BSD Boyer-Moore（lib/libc/string/bm.c）
 * 的自包含头，满足 minix3 t_bm 的 `#include <bm.h>`（bm_comp/bm_exec/bm_free）。
 * picolibc 不提供 <bm.h>。struct/原型对齐 `minix3/include/bm.h`。仅构建/测试用。
 */
#include <stddef.h>
#include <sys/types.h>

/* picolibc 的 sys/types.h 用 __u_char_defined 卫 u_char；同名守卫避免重定义。*/
#ifndef __u_char_defined
#define __u_char_defined
typedef unsigned char u_char;
#endif

typedef struct {
	u_char	*pat;			/* pattern */
	size_t	 patlen;		/* pattern length */
	size_t	*delta;			/* skip delta */
	int	 rarec;			/* rare character */
	size_t	 rareoff;		/* rare offset */
	size_t	 md2;			/* mini delta */
} bm_pat;

bm_pat *bm_comp(const u_char *, size_t, const u_char *);
u_char *bm_exec(bm_pat *, u_char *, size_t);
void    bm_free(bm_pat *);

#endif /* _ATF_C_COMPAT_BM_H */
