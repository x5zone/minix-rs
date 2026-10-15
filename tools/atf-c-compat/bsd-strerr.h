/* bsd-strerr.h — BSD strerror 族的契约面（sys_nerr + strerror/strerror_r/strerror_l）。
 *
 * 与 bsd-strfun.h 同族：都是「glibc 与 picolibc 都不给」的 BSD 件，因此**三条腿同一注入
 * 通道**（build-atf-test.sh 的 COMPAT_INCLUDE），不再属于 errno-compat.h 那张 picolibc
 * 专用清单。定义见同目录 bsd-strerr.c（含 135 项自给表与越界腿的真源对位）。
 *
 * 为什么声明必须与定义同签名、且先于系统头注入：
 *   - `sys_nerr`：picolibc 的 <errno.h> 与 glibc（2.32 起撤掉 BSD 名）都没有这个名字，
 *     缺声明则 t_strerror 编译期 undeclared；类型取 `const int`，与定义端同 TU 对账
 *     （防 CodeReview P2-1 声明/定义漂移，旧通道 errno-compat.h 已把本族移交此处）。
 *   - `strerror_r`：`-D_GNU_SOURCE` 下 glibc/picolibc 声明的是 **GNU 形**（返回 `char *`），
 *     而 t_strerror 的四支断言全按 **XSI 整数码**（== 0 / == EINVAL / == ERANGE）比较。
 *     故本头在系统 <string.h> 之前注入 `#define strerror_r __xpg_strerror_r`：那条 GNU 形
 *     声明被一并改名，所有 TU 的裸引用都落到 bsd-strerr.c 的 XSI 实现。只挂 asm 别名不够
 *     ——objdump 实锤测试调用是裸名，链接器会经 libc 的 strerror_r 成员把定义 `strerror`
 *     的同文件拽进来撞强-强硬错（三轮试错坐实，证据链在 WORKLOG 的 compat 真语义层一节）。
 *   - **负向契约**：GNU 风格调用法（`char *s = strerror_r(...)` 后解引用返回值）在本通道是
 *     UB——返回值是 XSI 码的整数衣装而非指针。现役测试集全为 XSI 断言式，无 GNU 受害者。 */
#ifndef MINIX_RS_BSD_STRERR_H
#define MINIX_RS_BSD_STRERR_H

#include <stddef.h>   /* size_t：本头经 -include 注入在测试 TU 所有 #include 之前，须自给 */
#include <locale.h>   /* locale_t：strerror_l 的签名（与本头同批注入，不依赖调用方先包） */
/* **必须先吃下 <string.h>，再定义文件末尾那条转发宏**：宏若先生效，系统头里那句
 * `char *strerror_r(...)`（`-D_GNU_SOURCE` 下的 GNU 形声明）会被一起改名成
 * `char *__xpg_strerror_r(...)`，与本头下面那条 `int` 声明当场冲突（gcc 14 实测
 * `error: conflicting types for '__xpg_strerror_r'`）。先包含就让系统声明以原名落地，
 * 之后的宏只影响调用点。glibc 自己也以 `int` 声明 `__xpg_strerror_r`，同形即无冲突。 */
#include <string.h>

/* 已知错误码上界（BSD 语义：[1, sys_nerr) 已知，sys_nerr 本身即「首个未知码」）。 */
extern const int sys_nerr;

/* BSD 措辞的 strerror：表内取本仓自给表，表外产 "Unknown error: N" 并置 errno=EINVAL。 */
char *strerror(int);

/* XSI strerror_r 的实现名；裸名经文件末尾的宏统一转发到此。**返回型必须是 `int`**：
 * 那既是 XSI 的契约（本仓 `strerror_r(3)` 成败值是 0/EINVAL/ERANGE），也是测试的用法
 * （`t_strerror.c:110` 把返回值赋给 `int`、`:94` 拿它与 `EINVAL` 比）。旧写法为了与被
 * 改名的 GNU 形声明「同形」而取 `char *`，代价是每次赋值都要把整数伪装成指针——
 * gcc 13 只给 `-Wint-conversion` 警告，gcc 14（CI 镜像 `minix-ci:1.94`）直接判错。 */
int __xpg_strerror_r(int e, char *buf, size_t buflen);

/* 本环不存在 locale 切换，l 版即同表查询（接管它同时断掉 libc 成员被拽入的链）。 */
char *strerror_l(int e, locale_t loc);

#ifndef strerror_r
#define strerror_r __xpg_strerror_r
#endif

#endif /* MINIX_RS_BSD_STRERR_H */
