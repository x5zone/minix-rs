/* Minix 的 `getopt(3)` 语义 —— 宿主参照件专用，不参与任何目标构建。
 *
 * 为什么要补：`minix3/usr.bin/uniq/uniq.c:77` 的 optstring 是 `"-cdf:s:u"`，并在
 * `:79-81` 写了 `case '-': --optind; goto done;`。glibc 的 getopt 见到开头的 `-`
 * 走的是「非选项按字符码 1 交回 + 把 argv 重新排列」那一套，于是 `case '-'` 永不命中、
 * `1` 又落进 `case '?'` → `usage()` ⇒ **宿主参照件带任何文件操作数都跑不动**
 * （现查：`uniq_ref 输入`、`uniq_ref 输入 输出`、`uniq_ref -c 输入` 一律 usage 退 1）。
 *
 * 真值不是猜的，取自本仓 vendor 的实现 `minix3/lib/libc/stdlib/getopt.c:63-143`：
 *   · **非选项就停**：`if (optind >= nargc || *place++ != '-') { place = EMSG; return -1; }`
 *     —— 不重排 argv、也不把非选项当成一枚选项交回，扫描停在第一个非选项上；
 *   · `"--"` 结束选项；**孤零零一枚 `-`** 只在 optstring 里含 `-` 时才当选项交回
 *     （`if (strchr(ostr, '-') == NULL) return -1; optopt = '-';`），uniq 的
 *     `case '-'` 正是为这一形写的，不是为「非选项交回」写的；
 *   · 认不出来的字母 → `BADCH`（`'?'`），optstring 以 `':'` 开头时静默；
 *   · 需要参数而没有 → 同样 `BADCH`，除非 optstring 以 `':'` 开头则回 `BADARG`；
 *   · `x:` 里第三个字符也是 `:` 时参数可缺省（GNU 扩展，本仓实现带着）。
 *
 * 复用 glibc 的 `optind`/`optarg`/`optopt`/`opterr` 四个全局，只换扫描算法；
 * 调用方经 `bsd-compat-proto.h` 的 `#define getopt bsd_getopt` 改名，所以不会与
 * 系统实现撞符号。`#if defined(__GLIBC__)` 让它在别的 libc 上整件退化为空翻译单元。
 */

#if defined(__GLIBC__)

#include <stdio.h>
#include <string.h>
#include <unistd.h>

#define HOST_REF_BADCH	(int)'?'
#define HOST_REF_BADARG	(int)':'
#define HOST_REF_EMSG	""

int
bsd_getopt(int nargc, char *const nargv[], const char *ostr)
{
	static const char *place = HOST_REF_EMSG;	/* 剩余待扫的选项字母串 */
	char *oli;					/* optstring 里的命中位置 */

	if (*place == 0) {
		place = nargv[optind];
		if (optind >= nargc || *place++ != '-') {
			/* 参数已尽，或这一枚不是选项：停在这里，把剩下的交回调用方 */
			place = HOST_REF_EMSG;
			return (-1);
		}
		optopt = (unsigned char)*place++;
		if (optopt == '-' && *place == 0) {
			/* "--" => 选项到此为止 */
			++optind;
			place = HOST_REF_EMSG;
			return (-1);
		}
		if (optopt == 0) {
			/* 孤零零一枚 "-"：只有 optstring 认它，才当选项交回 */
			place = HOST_REF_EMSG;
			if (strchr(ostr, '-') == NULL)
				return (-1);
			optopt = '-';
		}
	} else
		optopt = (unsigned char)*place++;

	if (optopt == ':' || (oli = strchr(ostr, optopt)) == NULL) {
		if (*place == 0)
			++optind;
		if (opterr && *ostr != ':')
			(void)fprintf(stderr, "%s: unknown option -- %c\n",
			    getprogname(), optopt);
		return (HOST_REF_BADCH);
	}

	if (oli[1] != ':') {
		optarg = NULL;
		if (*place == 0)
			++optind;
	} else {
		if (*place)
			optarg = (char *)place;
		else if (oli[2] == ':')
			optarg = NULL;
		else if (nargc > ++optind)
			optarg = nargv[optind];
		else {
			place = HOST_REF_EMSG;
			if (*ostr == ':')
				return (HOST_REF_BADARG);
			if (opterr)
				(void)fprintf(stderr,
				    "%s: option requires an argument -- %c\n",
				    getprogname(), optopt);
			return (HOST_REF_BADCH);
		}
		place = HOST_REF_EMSG;
		++optind;
	}
	return (optopt);
}

#endif /* __GLIBC__ */
