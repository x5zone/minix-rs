/* BSD `<stdlib.h>` 里的 `getprogname()`/`setprogname()` —— 宿主参照件用的补位实现。
 *
 * 为什么需要：`minix3/games/fortune/fortune/fortune.c` 在 `usage()`（`:1346`）与多处
 * 报文里用 `getprogname()` 取程序名。glibc 到 2.39 都没有这个名字（2.40 才补进
 * `<stdlib.h>`），隐式声明会把返回的指针截成 `int`，于是报文里的程序名是碎的、甚至
 * 直接段错误——那是承载造成的假读数，不是 C 的行为。
 *
 * 为什么带版本判据而不是无条件实现：容器权威门的 glibc 是 2.41，它自己就有这两个符号；
 * 无条件定义会在链接期撞出 duplicate symbol。判据让补面只在缺的那一侧生效。
 */

#include <sys/param.h>
#include <features.h>

#if defined(__GLIBC__) && defined(__GLIBC_PREREQ)
#if !__GLIBC_PREREQ(2, 40)
#define HOST_REF_NEEDS_PROGNAME 1
#endif
#else
#define HOST_REF_NEEDS_PROGNAME 1
#endif

#ifdef HOST_REF_NEEDS_PROGNAME

static const char *host_ref_progname = "?";

/* 参照件都从 `main` 的正常路径起，`__attribute__((constructor))` 在 `main` 之前跑，
 * 因此 `argc`/`argv` 是可得的。BSD 的语义就是把 `argv[0]` 的 basename 存下来。 */
__attribute__((constructor)) static void host_ref_capture(int argc, char **argv)
{
	const char *last_slash;

	if (argc < 1 || argv == NULL || argv[0] == NULL)
		return;
	last_slash = NULL;
	for (char *p = argv[0]; *p != '\0'; p++)
		if (*p == '/')
			last_slash = p;
	host_ref_progname = last_slash != NULL ? last_slash + 1 : argv[0];
}

const char *
getprogname(void)
{

	return (host_ref_progname);
}

void
setprogname(const char *name)
{

	if (name != NULL)
		host_ref_progname = name;
}

#endif /* HOST_REF_NEEDS_PROGNAME */
