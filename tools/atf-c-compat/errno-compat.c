/* errno-compat.c — NK4-C 目标③构建/链接面兼容件（仅构建/测试用）。
 *
 * picolibc 不提供 BSD/NetBSD 的 `sys_nerr`（<errno.h> 里根本没有这个名字，
 * 之前 gcc 提示的 "_sys_nerr" 只是模糊拼写建议、并非真实符号），而
 * minix3/tests/.../t_strerror.c 以 `sys_nerr` 作为 strerror(3) 已知错误码
 * 的上界。缺 `sys_nerr` 会让 t_strerror 编译期报 "undeclared" → 无法链成
 * ELF → 卡在"构建/链接面"。
 *
 * 本文件只为让 t_strerror 通过【编译+链接+过 minix-elf ehdr 硬校验】。诚实边界
 * （对齐同目录 posix-stubs 里 dl 相关函数、stresep 等 "链接占位、真语义上机补" 的口径）：
 *   - 本值决定 t_strerror 里 `for (i=1;i<sys_nerr;i++)` 与
 *     `strerror(i)` 是否含 "Unknown error:" 两段的分界。要让【上机跑通】语义
 *     正确，本数须与 picolibc `strerror()` 内部错误表覆盖的最大 errno 对齐。
 *   - riscv64 上机执行本身受缺陷 (A)（fork 路径竞态）门控（见 WORKLOG §续-266~270），
 *     故此处取 BSD 惯例值 134 作链接面占位，并在 WORKLOG 登记：上机前须按实际
 *     picolibc strerror 覆盖度校准 sys_nerr（或改判据为逐条对齐 sys_errlist）。
 */

const int sys_nerr = 134;
