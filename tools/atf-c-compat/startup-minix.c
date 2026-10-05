/* NK4-C 目标③：C 测试的出生链（自定义 _start，链接侧用 -nostartfiles 替换
 * picolibc 默认 crt0）。仅 aarch64 腿当前上机；riscv 分支同形备好（上机受
 * (A) 门控，解锁后启用）。
 *
 * 为什么不能用 picolibc 默认 crt0（实测反汇裁定）：
 *   1. 其 _start 把 sp 切到 ELF 内 .stack 段（嵌入式假设），丢弃内核建立的
 *      进程栈；
 *   2. 其 _cstart 以 main(0, NULL) 调入口——atf tp_main 第一句就
 *      strrchr(argv[0], '/') 解引用 NULL（minix3/.../detail/tp_main.c
 *      atf_tp_main），argv 必须由出生链提供。
 * 我方 exec 出生 ABI（权威对位 os/libs/minix-rt/src/crt0.rs 与内核
 * build_cpu_context）：入口 sp=初始栈、x0/a0=ps_strings 指针。
 * ps_strings 布局（C: sys/exec.h；Rust: PsStringsRaw，LP64）：
 *   argv_str(char **) @0, n_argv(i32) @8, env_str(char **) @16, n_env(i32) @24
 *
 * .data/.bss 不做 memcpy/memset：我方 ELF 加载器按 PT_LOAD 放置文件字节并对
 * memsz>filesz 零填充（os/libs/minix-elf 契约，Rust 命令同链已上机验证），
 * crt0 的复制在 LMA==VMA 下是自拷——跳过并注释（code-excellence：删嵌入式
 * 冗余而非照抄）。TLS 与 init_array 保留（aarch64 errno/tinystdio 走
 * tpidr_el0；构造器面 __libc_init_array）。
 *
 * 仅 ③ 构建/上机用；不参与生产镜像。 */
#include <stddef.h>

/* 本 TU 不需 kernel_call 常量（_exit/_write 均由 sys-bridge.c 提供）。 */
extern void _exit(int code);
extern void exit(int code);            /* picolibc stdlib：fflush 全部流 → _exit */
extern int  _write(int fd, const void *buf, size_t len);
extern void __libc_init_array(void);   /* picolibc：preinit/init 构造器 */
extern char **environ;                 /* libc.a 定义（D environ，默认 NULL） */

/* main：atf 测试程序的入口（tp_main.c 定义）。 */
extern int main(int argc, char **argv);

/* ps_strings 描述符（字段顺序/宽度与 Rust PsStringsRaw 逐一对位）。 */
struct startup_ps_strings {
    char **argv_str;
    int    n_argv;
    char **env_str;
    int    n_env;
};

/* argc<1 时给 tp_main 一个可活着的 argv[0]（否则 strrchr(NULL) 直接死）；
 * 空名字保持与 C crt0-common "empty_string" 同语义。存为非 const：
 * environ/argv 的标准类型就是 `char **`，const 化会触发赋值处
 * -Wdiscarded-qualifiers；安全性由"本文件写后不修"保证（注释约束）。 */
static char *empty_argv[] = { (char *)"", NULL };
/* n_env≤0/env_str 为野值时的 environ 兑底：picolibc getenv/_findenv 扫
 * NULL 或野指针会直接炸（真机 cr2=0xbfffbfffee9bee73 野值取证，§续-277），
 * 空表（首个元素 NULL）是两者都安全的标准形态。 */
static char *empty_env[] = { NULL };

void startup_c_main(unsigned long ps_addr);

/* env 向量可用性谓词（读一次 volatile 字段，避免 TOCTOU 式双读不一致）。 */
static int argc_ok_env(const volatile struct startup_ps_strings *ps) {
    return ps->n_env > 0 && ps->env_str != NULL;
}

/* 出生体（两架构的入口 stub 都跳这里）。
 * 注：不用 assert/printf——stdio 尚未证明可用时才走 _write 裸道。 */
void startup_c_main(unsigned long ps_addr) {
    int argc;
    char **argv;

    if (ps_addr != 0) {
        const volatile struct startup_ps_strings *ps =
            (const volatile struct startup_ps_strings *)ps_addr;
        /* 内核写好的向量本身可写（const volatile 只是本侧读视图），脱质
         * 限定靠中间 void* 显式化，不靠隐式丢 const。 */
        argc = ps->n_argv;
        argv = (char **)(void *)(unsigned long)ps->argv_str;
        /* env 腿：计数正且指针非零才挂；否则空表兑底（禁扫野指针）。 */
        environ = (argc_ok_env(ps)) ? (char **)(void *)(unsigned long)ps->env_str
                                    : empty_env;
    } else {
        argc = 0;
        argv = NULL;
        environ = empty_env;
    }

    if (argc < 1 || argv == NULL) {  /* 内核没给描述符/空向量：合成 argv[0] */
        argc = 0;
        argv = empty_argv;
        (void)_write(2, "startup: no argv from kernel\n", 29);
    }

    /* 临时诊断已滚除（§续-277 完成使命）：SUT 指针行定谳了 ps/argv/env 全合法；
     * PREMAIN 行定谳了 main 进点可达；后续 NULL-jump 取证改用内核 pfa 腿。
     * 取证链结论入 WORKLOG §续-277。 */
    /* 真机二分取证（§续-277 gh123/124）：本调用崩过——但定谳后确认毒不在
     * init_array 循环本体，而在 Debian 默认加固塞进 .init_array 的两个
     * libgcc/crt 成员（paciasp on ARMv8.0 + __stack_chk_init 野写）；
     * 编译面关加固（build-atf-test.sh aarch64 MC）后 .init_array 整段被
     * --gc-sections 裁空（objdump 实测无该 section），循环 0 条目安全，
     * 调用已恢复。 */
    __libc_init_array();
    /* 收尾用 stdlib exit() 而非裸 _exit：tinystdio 是块缓冲，main 返回后
     * 必须 fflush 才能把 atf 报告（Pass/warn）刷到 _write/SYS_DIAGCTL 串口——
     * 真机取证：上一轮裸 _exit 让全部输出静默丢弃（§续-277）。C 对位：
     * crt0 的 exit(main()) 语义（glibc __libc_start_main_ret / C 各 crt）。
     * exit 意外返回（不应发生）时落回裸 _exit 兑底。 */
    exit(main(argc, argv));
    _exit(127);
}

/* ── 架构入口 stub（不建帧，保住内核放好的 x0/a0 与 sp） ──
 * aarch64 TLS 初始化内联在此：逐字节摹写 picolibc crt0.o 反汇序列
 * （adrp+ldr 经 GOT 解引用 __tls_base/__arm64_tls_tcb_offset 两个链接
 * 常量——它们是 ld PROVIDE 地址而非内存变量，C 侧 extern 引用会错解成
 * 内容加载，故不走 C）。.data/.bss 不拷不清：加载器已按 PT_LOAD 放置+
 * 零填充（见文件头）。 */
#if defined(__aarch64__)

__asm__(
    ".text\n"
    ".globl _start\n"
    ".type _start, %function\n"
    "_start:\n"
    /* tpidr_el0 = __tls_base - __arm64_tls_tcb_offset。
     * 真机取证（§续-277 gh127）：旧版经 :got: 两跳读两符号——但
     * __arm64_tls_tcb_offset 是 ld PROVIDE 的**绝对数值符号**（A 型，非内存
     * 对象），静态链接器不保证给它分配 GOT 槽（实测读到零洞/无关内容，
     * tpidr 成 -49 垃圾 → errno 的 TLS 写 `str w1,[tp,#0x10]` 打
     * cr2=0xffffffffffffffdf 野地址 SIGSEGV，lstat 占位臂现场）。
     * 改直取：__tls_base 用 adrp+lo12（D 型符号地址）；tcb_offset 是 A 型
     * 数值符号，gas 的 mov 不容符号立即数——经字面量池 .quad 链接期绝对
     * 重定位取回（text 可读，ET_EXEC 无运行时重定位需求）。 */
    "   adrp x10, __tls_base\n"
    "   add  x10, x10, :lo12:__tls_base\n"
    "   adrp x9, 9990f\n"
    "   ldr  x9, [x9, :lo12:9990f]\n"
    "   sub  x10, x10, x9\n"
    "   msr  tpidr_el0, x10\n"
    "   bl   startup_c_main\n"     /* x0=ps_strings 直接按 AArch64 ABI 传参 */
    "   b    .\n"                  /* 不可达（main 走 _exit）；卡住可定位 */
    /* 槽布局：label 必须在 .align 之后——gh129 真机取证：label 写在 align
     * 之前时 :lo12: 落在 nop 填充上（tp=两枚 nop 拼成的垃圾 → errno 写
     * 野地址 SIGSEGV），对齐后才轮到 .quad。 */
    "   .align 8\n"
    "9990:\n .quad __arm64_tls_tcb_offset\n"
    ".size _start, .-_start\n");

#elif defined(__riscv)

__asm__(
    ".text\n"
    ".globl _start\n"
    ".type _start, @function\n"
    "_start:\n"
    /* tp = __tls_base。§续-384 真机定谳（旧注释「riscv 版 crt0 无 TLS 寄存器
     * 步骤」被证伪——本工具链的 picolibc riscv 对 errno 走 TLS，t_strerror
     * 四案首笔 errno 写 `sw a4,0(tp)` 落地址 0 → SIGSEGV，12 个不碰 errno
     * 的用例全绿所以此前不可见）：RISC-V 链接器把 TCB 偏移折进 TPREL（访问
     * 形态 0(tp)，aarch64 则留 0x10 TCB 头偏移故需减 __arm64_tls_tcb_offset；
     * riscv 版 picolibc.ld 只定义 __arm32/__arm64 两个 tcb_offset、没有
     * riscv 版，与「无需减」互证）。__tls_base 是 PROVIDE 符号
     *（picolibc.ld:209 `__tls_base = ADDR(.tdata)`，本仓测试全无 .tdata、
     * 与 .tbss 同址=0x80200058），la 的 HI20/LO12 重定位引用即兑现。 */
    ".option push\n"
    ".option norelax\n"
    "   la   t0, __tls_base\n"
    "   mv   tp, t0\n"
    ".option pop\n"
    "   call startup_c_main\n"    /* a0=ps_strings 按 RISC-V ABI 传参 */
    "   j    .\n"
    ".size _start, .-_start\n");

#else
#error "startup-minix.c: 未支持的架构（出生 stub 需与内核 build_cpu_context 同形）"
#endif
