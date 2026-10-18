/* BSD `fgetln(3)` 的宿主补位实现 —— 只服务参照件构建，不参与任何目标构建。
 *
 * `minix3/usr.bin/uniq/uniq.c` 的行读入口是 `fgetln(ifp, &length)`（两处调用），
 * glibc 从来没有这个名字（它只有 `getline`/`getdelim`），链接期直接
 * `undefined reference to 'fgetln'`。BSD 语义与 `getline` 有两处不一样，补位必须照住：
 * ① 返回的串**不**以 NUL 结尾，长度由 `*lenp` 交回，且**含**行尾换行；
 * ② 缓冲区归这条流所有，调用者不许 free，下一次对同一条流的操作使其失效。
 * uniq 拿到指针就立刻比较/写出，不缓存跨调用，所以「下一次操作失效」这一条
 * 用一枚复用的静态缓冲就等价——它比 `getline` 每行分配更省事，也不会被误 free。
 */

#if defined(__GLIBC__)

#include <stdio.h>
#include <stdlib.h>

#define HOST_REF_FGETLN_START 1024

char *
fgetln(FILE *stream, size_t *lenp)
{
	static char *buf;
	static size_t cap;
	size_t used = 0;
	int ch;

	if (cap == 0) {
		cap = HOST_REF_FGETLN_START;
		buf = malloc(cap);
		if (buf == NULL)
			return (NULL);
	}

	while ((ch = getc(stream)) != EOF) {
		if (used + 1 >= cap) {
			char *grown;

			cap *= 2;
			grown = realloc(buf, cap);
			if (grown == NULL)
				return (NULL);
			buf = grown;
		}
		buf[used++] = (char)ch;
		if (ch == '\n')
			break;
	}

	if (used == 0)
		return (NULL);
	if (lenp != NULL)
		*lenp = used;
	return (buf);
}

#endif /* __GLIBC__ */
