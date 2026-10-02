/*
 * tools/atf-c-compat/md5.c — 目标③ 构建面：MD5 摘要，满足 t_memcpy 的 <md5.h>。
 *
 * 基于 RSA Data Security, Inc. 的 MD5 Message-Digest Algorithm（RFC 1321 参考
 * 实现，公有领域）。签名对齐 BSD `minix3/sys/sys/md5.h`（struct MD5Context /
 * MD5Init/Update/Final/End/Data）。仅在链接 minix3 string 测试时使用，不进生产镜像。
 * 小端字节序编码（riscv64/aarch64/x86_64 目标均小端；byteDecode/Encode 逐字节
 * 组装，跨字节序安全）。
 */
#include <string.h>
#include <stdio.h>
#include "md5.h"

/* MD5 basic transformation. Transforms state based on block. */
#define F1(x, y, z) (z ^ (x & (y ^ z)))
#define F2(x, y, z) (y ^ (z & (x ^ y)))
#define F3(x, y, z) (x ^ y ^ z)
#define F4(x, y, z) (y ^ (x | ~z))

/* rotate left */
static uint32_t rot_l(uint32_t x, int n) { return (x << n) | (x >> (32 - n)); }

#define STEP(f, a, b, c, d, x, t, s) do { \
	a += f(b, c, d) + (x) + (t); \
	a = rot_l(a, (s)); \
	a += b; } while (0)

static void md5_encode(uint32_t *dst, const unsigned char *src, unsigned int len)
{
	unsigned int i, j;
	for (i = 0, j = 0; j < len; i++, j += 4)
		dst[i] = ((uint32_t)src[j]) |
			 (((uint32_t)src[j + 1]) << 8) |
			 (((uint32_t)src[j + 2]) << 16) |
			 (((uint32_t)src[j + 3]) << 24);
}

static void md5_transform(uint32_t state[4], const unsigned char block[64])
{
	uint32_t a = state[0], b = state[1], c = state[2], d = state[3];
	uint32_t x[16];

	md5_encode(x, block, 64);

	/* Round 1 */
	STEP(F1, a, b, c, d, x[0],  0xd76aa478, 7);
	STEP(F1, d, a, b, c, x[1],  0xe8c7b756, 12);
	STEP(F1, c, d, a, b, x[2],  0x242070db, 17);
	STEP(F1, b, c, d, a, x[3],  0xc1bdceee, 22);
	STEP(F1, a, b, c, d, x[4],  0xf57c0faf, 7);
	STEP(F1, d, a, b, c, x[5],  0x4787c62a, 12);
	STEP(F1, c, d, a, b, x[6],  0xa8304613, 17);
	STEP(F1, b, c, d, a, x[7],  0xfd469501, 22);
	STEP(F1, a, b, c, d, x[8],  0x698098d8, 7);
	STEP(F1, d, a, b, c, x[9],  0x8b44f7af, 12);
	STEP(F1, c, d, a, b, x[10], 0xffff5bb1, 17);
	STEP(F1, b, c, d, a, x[11], 0x895cd7be, 22);
	STEP(F1, a, b, c, d, x[12], 0x6b901122, 7);
	STEP(F1, d, a, b, c, x[13], 0xfd987193, 12);
	STEP(F1, c, d, a, b, x[14], 0xa679438e, 17);
	STEP(F1, b, c, d, a, x[15], 0x49b40821, 22);
	/* Round 2 */
	STEP(F2, a, b, c, d, x[1],  0xf61e2562, 5);
	STEP(F2, d, a, b, c, x[6],  0xc040b340, 9);
	STEP(F2, c, d, a, b, x[11], 0x265e5a51, 14);
	STEP(F2, b, c, d, a, x[0],  0xe9b6c7aa, 20);
	STEP(F2, a, b, c, d, x[5],  0xd62f105d, 5);
	STEP(F2, d, a, b, c, x[10], 0x02441453, 9);
	STEP(F2, c, d, a, b, x[15], 0xd8a1e681, 14);
	STEP(F2, b, c, d, a, x[4],  0xe7d3fbc8, 20);
	STEP(F2, a, b, c, d, x[9],  0x21e1cde6, 5);
	STEP(F2, d, a, b, c, x[14], 0xc33707d6, 9);
	STEP(F2, c, d, a, b, x[3],  0xf4d50d87, 14);
	STEP(F2, b, c, d, a, x[8],  0x455a14ed, 20);
	STEP(F2, a, b, c, d, x[13], 0xa9e3e905, 5);
	STEP(F2, d, a, b, c, x[2],  0xfcefa3f8, 9);
	STEP(F2, c, d, a, b, x[7],  0x676f02d9, 14);
	STEP(F2, b, c, d, a, x[12], 0x8d2a4c8a, 20);
	/* Round 3 */
	STEP(F3, a, b, c, d, x[5],  0xfffa3942, 4);
	STEP(F3, d, a, b, c, x[8],  0x8771f681, 11);
	STEP(F3, c, d, a, b, x[11], 0x6d9d6122, 16);
	STEP(F3, b, c, d, a, x[14], 0xfde5380c, 23);
	STEP(F3, a, b, c, d, x[1],  0xa4beea44, 4);
	STEP(F3, d, a, b, c, x[4],  0x4bdecfa9, 11);
	STEP(F3, c, d, a, b, x[7],  0xf6bb4b60, 16);
	STEP(F3, b, c, d, a, x[10], 0xbebfbc70, 23);
	STEP(F3, a, b, c, d, x[13], 0x289b7ec6, 4);
	STEP(F3, d, a, b, c, x[0],  0xeaa127fa, 11);
	STEP(F3, c, d, a, b, x[3],  0xd4ef3085, 16);
	STEP(F3, b, c, d, a, x[6],  0x04881d05, 23);
	STEP(F3, a, b, c, d, x[9],  0xd9d4d039, 4);
	STEP(F3, d, a, b, c, x[12], 0xe6db99e5, 11);
	STEP(F3, c, d, a, b, x[15], 0x1fa27cf8, 16);
	STEP(F3, b, c, d, a, x[2],  0xc4ac5665, 23);
	/* Round 4 */
	STEP(F4, a, b, c, d, x[0],  0xf4292244, 6);
	STEP(F4, d, a, b, c, x[7],  0x432aff97, 10);
	STEP(F4, c, d, a, b, x[14], 0xab9423a7, 15);
	STEP(F4, b, c, d, a, x[5],  0xfc93a039, 21);
	STEP(F4, a, b, c, d, x[12], 0x655b59c3, 6);
	STEP(F4, d, a, b, c, x[3],  0x8f0ccc92, 10);
	STEP(F4, c, d, a, b, x[10], 0xffeff47d, 15);
	STEP(F4, b, c, d, a, x[1],  0x85845dd1, 21);
	STEP(F4, a, b, c, d, x[8],  0x6fa87e4f, 6);
	STEP(F4, d, a, b, c, x[15], 0xfe2ce6e0, 10);
	STEP(F4, c, d, a, b, x[6],  0xa3014314, 15);
	STEP(F4, b, c, d, a, x[13], 0x4e0811a1, 21);
	STEP(F4, a, b, c, d, x[4],  0xf7537e82, 6);
	STEP(F4, d, a, b, c, x[11], 0xbd3af235, 10);
	STEP(F4, c, d, a, b, x[2],  0x2ad7d2bb, 15);
	STEP(F4, b, c, d, a, x[9],  0xeb86d391, 21);

	state[0] += a;
	state[1] += b;
	state[2] += c;
	state[3] += d;

	memset(x, 0, sizeof(x));
}

void MD5Init(MD5_CTX *ctx)
{
	ctx->count[0] = ctx->count[1] = 0;
	ctx->state[0] = 0x67452301;
	ctx->state[1] = 0xefcdab89;
	ctx->state[2] = 0x98badcfe;
	ctx->state[3] = 0x10325476;
}

void MD5Update(MD5_CTX *ctx, const unsigned char *input, unsigned int input_len)
{
	unsigned int have, need;

	have = (unsigned int)((ctx->count[0] >> 3) & 0x3F);
	ctx->count[0] += ((uint32_t)input_len) << 3;
	if (((uint32_t)(input_len << 3)) < ((uint32_t)input_len)) /* carry into high */
		ctx->count[1]++;
	ctx->count[1] += ((uint32_t)input_len >> 29);

	need = 64 - have;
	if (input_len >= need) {
		memcpy(ctx->buffer + have, input, need);
		md5_transform(ctx->state, ctx->buffer);
		input += need;
		input_len -= need;
		have = 0;
		while (input_len >= 64) {
			md5_transform(ctx->state, input);
			input += 64;
			input_len -= 64;
		}
	}
	memcpy(ctx->buffer + have, input, input_len);
}

void MD5Final(unsigned char digest[MD5_DIGEST_LENGTH], MD5_CTX *ctx)
{
	static const unsigned char padding[64] = { 0x80 };
	unsigned char bits[8];
	unsigned int have, pad_len;
	unsigned int i;

	for (i = 0; i < 8; i++)
		bits[i] = (unsigned char)(ctx->count[i >> 2] >> ((i & 3) << 3));

	have = (unsigned int)((ctx->count[0] >> 3) & 0x3F);
	pad_len = (have < 56) ? (56 - have) : (120 - have);
	MD5Update(ctx, padding, pad_len);
	MD5Update(ctx, bits, 8);

	md5_encode((uint32_t *)digest, (const unsigned char *)ctx->state, 16);
	memset(ctx, 0, sizeof(*ctx));
}

char *MD5End(MD5_CTX *ctx, char *buf)
{
	unsigned char digest[MD5_DIGEST_LENGTH];
	int i;

	MD5Final(digest, ctx);
	for (i = 0; i < MD5_DIGEST_LENGTH; i++)
		sprintf(buf + i + i, "%02x", digest[i]);
	buf[MD5_DIGEST_LENGTH * 2] = '\0';
	return buf;
}

char *MD5Data(const unsigned char *data, unsigned int len, char *buf)
{
	MD5_CTX ctx;

	MD5Init(&ctx);
	MD5Update(&ctx, data, len);
	return MD5End(&ctx, buf);
}
