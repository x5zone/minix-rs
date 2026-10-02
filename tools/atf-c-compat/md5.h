#ifndef _ATF_C_COMPAT_MD5_H
#define _ATF_C_COMPAT_MD5_H
/*
 * tools/atf-c-compat/md5.h — 目标③ 构建面：minix3 的 string 测试 t_memcpy 用
 * `#include <md5.h>` 走 MD5Init/Update/End 校验 memcpy 结果摘要。picolibc 不提供
 * BSD <md5.h>，故在 compat 层补一份与 `minix3/sys/sys/md5.h` 同签名（struct 同
 * BSD MD5Context）的最小实现，让 t_memcpy 能链成可加载 ELF。仅构建/测试用，
 * 不进任何生产镜像/OS 码。
 */
#include <stdint.h>

#define MD5_DIGEST_LENGTH 16

typedef struct MD5Context {
	uint32_t state[4];      /* state (ABCD) */
	uint32_t count[2];      /* number of bits, modulo 2^64 (lsb first) */
	unsigned char buffer[64];  /* input buffer */
} MD5_CTX;

void MD5Init(MD5_CTX *);
void MD5Update(MD5_CTX *, const unsigned char *, unsigned int);
void MD5Final(unsigned char[MD5_DIGEST_LENGTH], MD5_CTX *);
char *MD5End(MD5_CTX *, char *);
char *MD5Data(const unsigned char *, unsigned int, char *);

#endif /* _ATF_C_COMPAT_MD5_H */
