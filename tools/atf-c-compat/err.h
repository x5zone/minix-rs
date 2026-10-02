/* NK4-C 目标③：picolibc(riscv64-unknown-elf) 缺 BSD <err.h>。
 * atf-c/utils.c 等用 err/warn(3)。此处提供原型；实现（err/warn/errx/warnx
 * + v* 变体）在链接期由 runtime shim 提供（构建 libatf-c.a 只需声明）。
 * 仅构建期用；不参与 minix-rs 生产镜像。 */
#ifndef ATF_COMPAT_ERR_H
#define ATF_COMPAT_ERR_H
#include <stdarg.h>
void err(int eval, const char *fmt, ...);
void errx(int eval, const char *fmt, ...);
void warn(const char *fmt, ...);
void warnx(const char *fmt, ...);
void verr(int eval, const char *fmt, va_list ap);
void verrx(int eval, const char *fmt, va_list ap);
void vwarn(const char *fmt, va_list ap);
void vwarnx(const char *fmt, va_list ap);
#endif
