/* errno-compat.c — NK4-C 目标③构建/链接面兼容件（仅构建/测试用）。
 *
 * picolibc 不提供 BSD/NetBSD 的 `sys_nerr`（<errno.h> 里根本没有这个名字，
 * 之前 gcc 提示的 "_sys_nerr" 只是模糊拼写建议、并非真实符号），而
 * minix3/tests/.../t_strerror.c 以 `sys_nerr` 作为 strerror(3) 已知错误码
 * 的上界。缺 `sys_nerr` 会让 t_strerror 编译期报 "undeclared" → 无法链成
 * ELF → 卡在"构建/链接面"。
 *
 * 本文件只为让 t_strerror 通过【编译+链接+过 minix-elf ehdr 硬校验】。
 * **§续-279m 校准已落地**：真机探针 gh160 定谳 picolibc 越界码永不产 BSD 形
 * "Unknown error: N"（空串/表洞 133），故单调数值不够——posix-stubs.c 已全
 * 接管 strerror/strerror_r（已知 1..sys_nerr 用 picolibc 真串、表洞占位、
 * 越界产 NetBSD 同形+EINVAL）；已知区间 = 表项 [0, sys_nerr)（与 C
 * compat_errlist.c:153 的 `sys_nerr = 表项数` 同形，§续-279n 边界差一定谳：
 * 旧值 134 + `e <= sys_nerr` 判据令 e=134 被当已知，而测试以 sys_nerr 为
 * 「首个未知码」——表补 0 项后表项数 135，e∈[1,134] 已知、e≥135 未知）。
 */

#include "errno-compat.h"   /* 自身声明纳入本 TU，让 const 类型/声明与定义在编译期对账（防 CodeReview P2-1 声明/定义漂移）*/

const int sys_nerr = 135;
