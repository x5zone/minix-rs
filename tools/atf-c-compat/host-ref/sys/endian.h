/* BSD <sys/endian.h> 补位面 —— 只服务宿主参照件构建。
 *
 * glibc 把同名转换函数放在 `<endian.h>`（不在 `<sys/endian.h>`），所以本文件多数工作是
 * 「把 BSD 的头名翻译成宿主的头名」，而不是自己实现字节序转换。BSD 独有而 glibc 没有的
 * 只有 `bswap16/32/64` 这一族「无条件翻转」，在此按 `<byteswap.h>` 补上。
 *
 * `minix3/games/fortune/fortune/fortune.c:52` 含了这枚头却一个符号都没用（现查：该文件
 * 里没有 `BYTE_ORDER`/`*_ENDIAN`/`htole*`/`*toh` 任何一处引用，见批次三百六十七的读数）。
 * 补它不是为 `fortune`，是为了下一支参照件——`cksum` 那族（A4）要读 NetBSD 的哈希状态
 * 结构，那类文件是真会用 `le32toh` 的。
 */

#ifndef _HOST_REF_SYS_ENDIAN_H
#define _HOST_REF_SYS_ENDIAN_H

#include <endian.h>
#include <byteswap.h>
#include <stdint.h>

#ifndef bswap16
#define bswap16(x) ((uint16_t)__bswap_16(x))
#endif

#ifndef bswap32
#define bswap32(x) ((uint32_t)__bswap_32(x))
#endif

#ifndef bswap64
#define bswap64(x) ((uint64_t)__bswap_64(x))
#endif

/* NetBSD 的旧拼写：`BE16TOH`/`LE32TOH` 一类大写宏在 1.6 时代源码里常见。 */
#ifndef BE16TOH
#define BE16TOH(x) be16toh(x)
#endif

#ifndef BE32TOH
#define BE32TOH(x) be32toh(x)
#endif

#ifndef BE64TOH
#define BE64TOH(x) be64toh(x)
#endif

#ifndef LE16TOH
#define LE16TOH(x) le16toh(x)
#endif

#ifndef LE32TOH
#define LE32TOH(x) le32toh(x)
#endif

#ifndef LE64TOH
#define LE64TOH(x) le64toh(x)
#endif

#ifndef HTOBE16
#define HTOBE16(x) htobe16(x)
#endif

#ifndef HTOBE32
#define HTOBE32(x) htobe32(x)
#endif

#ifndef HTOBE64
#define HTOBE64(x) htobe64(x)
#endif

#ifndef HTOLE16
#define HTOLE16(x) htole16(x)
#endif

#ifndef HTOLE32
#define HTOLE32(x) htole32(x)
#endif

#ifndef HTOLE64
#define HTOLE64(x) htole64(x)
#endif

#endif /* _HOST_REF_SYS_ENDIAN_H */
