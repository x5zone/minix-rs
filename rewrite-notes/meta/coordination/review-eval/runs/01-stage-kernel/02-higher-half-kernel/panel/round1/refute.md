# 反证报告：02-higher-half-kernel.md（P-REFUTE，round 1）

范围：只读核查。文档全文一条，关联 Rust 源 27 个（`tools/doc-code-map.sh ... --check` 全部存在），以及文档引用的 Minix3 C 源。
方法：对每条主张先写"它会在什么情况下不成立"，再去源码里找那条路径；能找到就给位置，找不到才写"主张成立"。
已执行的复核命令（全部在仓库根执行，只读）：
- `SKIP_BUILD=1 bash os/kernel-image/check-layout.sh all` → 三架构 L0–L9 全 PASS
- `readelf -hW/-lW` 与 `nm` 读 `os/target/*/release/kernel` 三份工件
- 文档自带的复算命令逐条重跑（下文注明哪条与文档不符）

---

## CLAIM 01

- 主张：C 侧 i386 的链接布局是"同一个镜像两套地址"——`.unpaged_*` 三段不带 `AT()`（VMA = LMA，落在低侧），五个带 `AT()` 的段虚拟地址减装载地址等于同一常量 `_kern_offset`；因为 `mpx.o` 不在 `objcopy` 改名名单里，`mpx.S` 里定义的 `k_initial_stktop` 只能落进高半 `.bss`，所以 `head.S` 换到的是一个高半栈（文档 §2.1、§2.2、§2.4）。

- 反证尝试：
  1. 找"`mpx.o` 被另一条规则改名"的路径。`minix3/minix/kernel/arch/i386/Makefile.inc` 里唯一产生 `unpaged_*.o` 的是第 50–68 行的 `.for unpaged_obj in ...` 块加 `objcopy --prefix-symbols`；`mpx.S` 只出现在第 75 行的普通 `SRCS+=` 清单里。第 108、125 行的 `klib.o mpx.o head.o: procoffsets.h` 是依赖行，不改名。此路不通。
  2. 找"`mpx.o` 的 `.bss` 被 `.unpaged_bss` 收走"的路径。链接脚本的输入通配是 `unpaged_*.o(.bss COMMON)`（`i386/kernel.lds:19`），匹配的是输入文件名；`objcopy` 只改符号表，不改文件名或段名。`mpx.o` 的名字不匹配。此路不通。
  3. 找"`.bss` 输出段排在 `. += _kern_offset` 之前"的路径。脚本次序是 `.unpaged_text/.unpaged_data/.unpaged_bss` → `__k_unpaged__kern_unpaged_end` → `. += _kern_offset` → `.usermapped*` → `.text` → `.data` → `.bss`（`i386/kernel.lds:16–33`）。唯一能收 `mpx.o` 的 `.bss` 在偏移之后。此路不通。
  4. 找"`.unpaged_*` 三段里有一段为空、以致 `.text` 的虚拟地址正好等于 0xF0400000"的路径。`.unpaged_text` 收 `head.o` 的 `.text`，而 `head.o` 在改名名单第一位；`head.o` 的 `.text` 非空。因此偏移后位置计数器严格大于 `0xF0400000`。此路不通。
  5. 找"`k_initial_stktop` 在别处另有一个不带前缀的定义"的路径。全树只有两处定义：`i386/mpx.S:634` 的 `LABEL(__k_unpaged_k_initial_stktop)` 与 `earm/mpx.S:272` 的同一行；引用只有 `i386/head.S:84` 与 `earm/head.S:42`。

  另一条我顺手核过、但不构成反证的旁支：`pre_init.c:112` 先把 `user_sp` 赋成 `&_kern_vir_base`，第 156 行再覆盖为 `USR_STACKTOP`。文档只摘了后两行，最终取值正确，不改变结论。

- 结果：反证失败（主张成立）。

- 证据：
  - `minix3/minix/kernel/arch/i386/kernel.lds:6–8`（两个基址与 `_kern_offset`）、`:16–19`（三个不带 `AT()` 的 unpaged 段）、`:25–32`（五处 `AT(ADDR(...) - _kern_offset)`，`grep -n 'AT('` 恰好 5 条）、`:32–34`（通用 `.bss` 在偏移之后）。
  - `minix3/minix/kernel/arch/i386/Makefile.inc:50–56`（27 项名单；三项库清单复算 3/13/2 全部与文档一致）、`:66–68`（`OBJCOPY --prefix-symbols=__k_unpaged_`）、`:75`（`mpx.S` 只在普通源码清单）。
  - `minix3/minix/kernel/arch/i386/mpx.S:632–634`（`.bss` 里的 `k_initial_stack` 与 `LABEL(__k_unpaged_k_initial_stktop)`）。
  - `minix3/minix/kernel/arch/i386/head.S:84`（`mov $k_initial_stktop, %esp`）。
  - `minix3/minix/kernel/proto.h:49–50`（`#ifndef UNPAGED` 下把 `kmain` 改名）与 `pre_init.c:2`（`#define UNPAGED 1`）合起来解释"定义与引用两侧的命名如何闭合"，文档 §2.2 的描述与源码一致。

- 若我错了：最可能错在我把"链接脚本输入通配匹配文件名"当成了唯一机制。如果 binutils 的 `unpaged_*.o` 在某种调用形态下匹配的是归档成员名或节名，第 2 步就会失效；但 `objcopy` 输出的是普通 `.o` 而非归档，且段名与节名里没有 `unpaged_` 前缀，所以我看不到这条路径。

- 附：文档 §2.2 的"反方向错法"推演不成立（非阻断 P2）
  - 文档说：把 `kmain` 这个"本该在高半执行的函数"放进改名名单，它的段体就落进低地址一侧，`head.S` 会调到低侧那一份，坏点推迟出现（第 207 行）。
  - 按同一节自己描述的机制，这一步走不通：`objcopy --prefix-symbols` 给**定义**也加前缀（文档第 154 行原话"定义与引用都加"）。`main.c` 里的定义已经由 `proto.h:49–50` 的宏改名成 `__k_unpaged_kmain`，再过一遍 `objcopy` 会变成 `__k_unpaged___k_unpaged_kmain`；而 `head.o` 的引用只被加一次前缀，仍是 `__k_unpaged_kmain`。两者对不上。
  - 更关键的是 `main.c` 仍在普通源码清单里（`minix3/minix/kernel/Makefile:15`），`OBJS.kernel` 同时收普通对象与 `UNPAGED_OBJS`（`arch/i386/Makefile.inc:83`）。所以 `main.o` 的高半定义还在，`head.o` 的引用解析到它；低侧多出来的 `unpaged_main.o` 是一份没人引用的副本（连链接错误都不会有）。结果是"kmain 仍在高半、低侧副本是死代码"，不是文档说的"低侧副本被调用、坏点推迟"。
  - 要让这个推演真的成立，需要同时把 `main.o` 从普通清单里去掉（或者把源文件按 `-DUNPAGED` 重编一份），文档没有写这一步。

---

## CLAIM 02

- 主张：Rust 侧链接脚本按"谁按装载地址摆放"分三种形态——生产镜像必须给低地址 LMA（`AT()`）交外部装载器；`os/kernel/src/arch/*/link.ld` 只记录预期布局、没有生产链接读者；载体用单地址脚本。判据是"有读者读 `p_paddr` 才需要两套地址"（文档 §3.1、§4.1、§6.2）。

- 反证尝试：
  1. 找"生产脚本没有 `AT()`"或"arch 脚本有 `AT()`"的反例。`grep -c ': AT('` 对 `os/kernel-image/{x86_64,aarch64,riscv64}.ld` 各 4，对 `os/kernel/src/arch/*/link.ld` 各 0（`aarch64`/`riscv64` 放宽成 `AT(` 会因头注释各多 1，`x86_64` 多 2，与文档 §6.2 的口径说明一致）。
  2. 找"arch 脚本被某个构建脚本当链接输入"的路径。文档给的两条命令我原样重跑：`grep -rn 'src/arch/.*link.ld' os/qemu-tests/*.sh os/xtask/src/*.rs` 无命中；扩大到 `os` 下全部 `*.rs/*.toml/*.sh/Makefile` 后，命中的只有 `os/kernel/tests/link_ld_lockstep.rs`（读文件做常量比对）与注释。`os/kernel-image/build.rs:1–20` 只按 `--target` 选 `os/kernel-image/*.ld`。此路不通。
  3. 找"载体脚本有 `AT()`"的反例。`find os/qemu-tests -name link.ld` 逐个 `grep -c ': AT('` 全为 0；`test-higher-half-riscv64/link.ld` 的正文与文档 §3.1 引用的 `. = 0x80200000`、`__stack_top = . + 0x10000` 完全一致。
  4. 找"常量锁步测试能证明脚本被链接"的反证。`os/kernel/tests/link_ld_lockstep.rs:56–72` 只读两份脚本、解出 `KERN_VIRT_BASE/KERN_PHYS_BASE` 做 `assert_eq!`，不产生链接参数。文档说"只锁常量，不证明被链接"是准确的。
  5. 找"第四种形态"（别的内核镜像脚本带两套地址）。全仓 `*.ld` 清单里，除三份生产脚本外只有 `os/boot-shim/riscv64-uboot.ld` 属于另一个程序（boot-shim 自身），不属于"内核镜像"分类。

- 结果：反证失败（主张成立）。

- 证据：
  - `os/kernel-image/{x86_64,aarch64,riscv64}.ld` 的 `AT` 表达式与文档 §3.1 的两种写法逐字一致（x86_64 开头 `KERNEL_HIGH_BASE = 0xFFFF800000000000`、`KERNEL_PHYS_BASE = 0x200000`；另两份是 `KERNEL_VIRT_BASE + KERNEL_PHYS_BASE` 形状）。
  - `os/kernel/src/arch/x86_64/link.ld:8`（`No AT() directives — boot-shim handles LMA placement`）、`:16`（`KERN_VIRT_BASE = 0xFFFF800000000000`）。
  - `os/kernel-image/build.rs`（按 `--target` 三选一，宿主目标走 `None`）。
  - `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half-riscv64/link.ld`（单地址、无 `AT()`、`__stack_top = . + 0x10000`）。
  - `os/kernel/tests/link_ld_lockstep.rs:36–72`（只做常量相等断言）。

- 若我错了：如果某个 CI 作业或仓库外的打包流程用 `-T os/kernel/src/arch/<arch>/link.ld` 链接内核，文档的"无读者"就不成立。我在仓库内找不到这种调用；仓库外的脚本不在可核查范围。

---

## CLAIM 03

- 主张：`kmain` 用的栈顶不再是链接符号，而是装载方按 `kern_stack_top = kern_virt_base + kern_size` 算出的镜像尾虚拟地址，按值经 `KernelInfo` 交接；它成立有两个前提（末段覆盖镜像尾、镜像尾 16 字节对齐）；链接脚本 `.bss` 末尾的 `. = ALIGN(0x200000)` 同时决定 `kern_size`、`kern_stack_top` 与映射循环上界，使页尺寸阶梯永远不落进 panic 分支（文档 §3.3、§3.5）。

- 反证尝试：
  1. 找"`kern_size` 不是 `kern_virt_base + kern_size` 那个算式"的实现。`os/boot-shim/src/lib.rs:125` 原文就是 `kern_stack_top: minix_types::VirBytes(kern_virt_base.0 + kern_size)`；`os/libs/minix-boot/src/kernel_info.rs:464–470` 还在生产门（strict）里断言 `kern_stack_top == kern_virt_base + kern_size`。没有第二份实现。
  2. 找"镜像尾不等于 `kern_virt_base + kern_size`"的工件。用三份 release 工件实测：
     - x86_64：`kern_virt_base = 0xffff800000200000`，`kern_size = 0x600000 - 0x200000 = 0x400000`，和为 `0xffff800000600000`；末段 LOAD 是 `0xffff800000339000 + memsz 0x2c7000 = 0xffff800000600000`。相等。
     - aarch64：`0xffff800000000000 + 0x400000 = 0xffff800000400000`，末段 `0xffff80000011e000 + 0x2e2000` 同值。
     - riscv64：`0xffffffc000000000 + 0x400000 = 0xffffffc000400000`，末段 `0xffffffc000108000 + 0x2f8000` 同值。
     前提一在现工件上成立。
  3. 找"收口没进末段 memsz"的反例。三份脚本的 `. = ALIGN(0x200000)` 都在 `.bss` 的花括号内（x86_64.ld 的 `.bss` 段、aarch64.ld 同形、riscv64.ld 同形），`.bss` 的 `p_filesz = 0`、`p_memsz` 覆盖到收口端点。`readelf -lW` 的末段 memsz 与上面算出的差值一致。此路不通。
  4. 找"跨距不是 2MiB 整数倍、页尺寸阶梯会 panic"的工件。`SKIP_BUILD=1 bash os/kernel-image/check-layout.sh all` 三架构 L5 全 PASS（"镜像跨距 4 MiB 且 2MiB 对齐（余 0 字节）"），riscv64 L9 还对上 `bootface.rs` 的 `KERN_SIZE = 0x400000`。此路不通。
  5. 找"`plan_kernel_mapping` 会选 4KiB"的路径。`os/kernel/src/boot_policy.rs:98–150` 只有 preferred/fallback 两档，`layout_satisfies` 要求基数与跨距三者都能被该粒度整除；4KiB 不在阶梯里，所以"不收口就用 4KiB 细粒度"是否决方案而非实现。
  6. 找"跳过收口仍能正常映射"的路径：不能——`NoKernelPageSize` 会在任何页表写入之前 panic（`os/kernel/src/lib.rs:947` 的 `panic!("arch_boot: page-size policy: {e}")`，Display 文本在 `boot_policy.rs:51`）。文档 §3.5 的描述准确。

- 结果：反证失败（主张成立），但带一条非阻断的跨节不一致（见下）。

- 证据：
  - `os/boot-shim/src/lib.rs:101–146`（`build_kernel_info` 的派生字段）、`:125`（栈顶算式）。
  - `os/boot-shim/src/loader.rs:175–215`（`compute_kernel_layout`：`kern_phys_base = min(paddr)`、`kern_end_phys = max(paddr + memsz)`、`kern_size = 差`）。
  - `os/kernel-image/{x86_64,aarch64,riscv64}.ld` 的 `.bss` 末尾 `ALIGN(0x200000)`。
  - `os/kernel-image/check-layout.sh:205–211`（L5 用末段 `VMA + memsz` 减首段 VMA）与 `:248–260`（L9 只在 riscv64 分支）。
  - 工件实测数值见上（`readelf -lW`/`nm`/`check-layout.sh` 输出）。

- 若我错了：如果将来某个工件的最后一个 `PT_LOAD` 不是 `.bss`（例如出现落在收口之后的孤儿节），前提一就破，`kern_stack_top` 会算到收口端点之外而映射上界仍按 `kern_size`。文档 §4.1 自己给过这个风险的另一面（`.lrodata.*` 孤儿"落在收口之前"才使 L5 通过），说明这条前提是工件相关的而不是脚本结构性保证的；这一点文档没有明说，但也不构成它当前结论的反例。

- 附：跨节不一致（非阻断 P2）
  - §3.3 末尾说"收口缺失的完整后果链（粒度选择落空、**映射越界覆盖别人内存**）……留在 §3.5"（文档第 545 行）。
  - §3.5 的实际说法是"panic 发生在任何页表写入之前——现场是'分页还没开就停'"（第 643 行），表格同档（第 616 行）。
  - 两处不能同时为真：粒度不满足时策略层先返回 `NoKernelPageSize`，映射循环根本不会开始，"越界覆盖别人内存"在 Rust 侧没有执行路径。这句话要么指向 C 侧 `pg_mapkernel` 的向上取整行为（§3.5 第一段），要么应删。它不影响本章结论，但按"文档事实"应改。

---

## CLAIM 04

- 主张：换栈与跳转成对、次序固定，Rust 把它收进 `HigherHalf` trait（`-> !` 加四条 `unsafe` 前置条件）；三架构指令不同但语义同构——x86_64 用相对 `call`（要求调用点执行地址等于链接地址）、aarch64 用字面量池取绝对值的 `ldr` + `br`、riscv64 用 PC 相对的 `la` 加 `auipc/bltu` 条件补 `delta`（文档 §3.4、§4.3）。

- 反证尝试：
  1. 找"某架构漏了 16 字节对齐或清帧指针"的实现。三份 `higher_half.rs` 都对 `sp` 做 `and -16`、都把帧指针寄存器清零（`xor rbp` / `mov x29,#0` / `li s0,0`），x86_64 额外 `push 0` + `sub rsp,8`。无遗漏。
  2. 找"riscv64 无条件加 delta 或无条件不加"的实现。源码确实是 `auipc t3, 0` + `bltu t3, t4, 3f` 的两条 `jalr`（`os/kernel/src/arch/riscv64/higher_half.rs:118–123` 附近），与文档的两条腿描述一致；`checked_sub` 的 `expect` 也在。
  3. 找"x86_64 的 `call` 不是相对调用"的实现。`os/kernel/src/arch/x86_64/higher_half.rs` 的 asm 是 `call {kmain}` + `kmain = sym kmain`，汇编器生成直接 `call rel32`；x86_64 生产入口 `_start` 与 `kmain` 在同一镜像、同一高半布局，位移是小量。文档"链接期前提"的说法成立。
  4. 找"aarch64 的 `isb` 是正确性前提"的反例。`os/arch/src/arm64/paging.rs:695–710` 在写 `ttbr0_el1/ttbr1_el1` 后有 `dsb sy; isb`，写 `sctlr_el1` 后有 `isb`；`jump_to_kmain` 里那条 `isb` 之前没有新的 TTBR/SCTLR 写入。文档"删掉不产生按旧映射取指的失败"成立。
  5. 找"参数寄存器会被 asm 破坏"的反例。三份 asm 分别用 `in("rdi")`/`in("x0")`/`in("a0")` 绑定 `kinfo`，riscv64 源码注释明确禁用 a0/a6/a7；a6/a7 未出现在 asm 里。
  6. 找"trait 的第四条前置条件被类型系统兜住"的反例。类型系统只兜住"不返回"（`-> !`），调用次数没有类型级约束；文档 §3.4 的表把"只调用一次"归给调用者，正确。

- 结果：反证失败（主张成立，指 trait 契约与三架构寻址机制这一层）。

- 证据：
  - `os/kernel/src/boot/higher_half.rs:24–49`（trait 与四条 Safety 条款）。
  - `os/kernel/src/arch/x86_64/higher_half.rs:31–55`、`os/kernel/src/arch/aarch64/higher_half.rs:30–53`、`os/kernel/src/arch/riscv64/higher_half.rs:96–123`（三份 asm）。
  - `os/arch/src/arm64/paging.rs:695–710`（enable 的屏障）。
  - `os/kernel/src/lib.rs:225–232`（`arch_boot_finish` 的调用点）、`:326–375`（`resume_additive` 的调用点）。

- 若我错了：如果 riscv64 的 `la` 在 `medany` 之外还有其他 code model 分支（例如某次构建换成 `medlow`），`la` 的展开会不同，文档"PC 相对 + 补 delta"的论述需要按 code model 重述。当前 `.cargo/config.toml` 没有给 riscv64 目标改 code model，默认 `medany`，我看不到这条分支。

- 附：一条无法在仓库内证实、也不算反证的观察
  - §3.1 与 §4.4 把"aarch64 direct-core 低地址进入"写成事实，同处又注明它"来自固件按装载地址平移入口地址的装载行为，不在本仓链接脚本里；可用 QEMU `-d in_asm` 核对"（第 1032 行）。这是全文唯一一条核心机制依赖仓库外行为的断言；它没有被任何宿主断言或真机日志在本篇内锚定。我按"观察"记，不判阻断。

---

## CLAIM 05

- 主张：C 与 Rust 在"帧指针链如何终止"上不同——C 侧 `head.S` 入口的 `mov $0, %ebp` 只保证换栈前 EBP 为 0，换栈后调用 `kmain` 时它保存的是 `pre_init` 的帧指针（不是 0），链要走到别处才停；Rust 侧把 `xor rbp, rbp` 放在换栈之后，`kmain` 的保存槽里是 0，链在 0 处终止（文档 §2.3 第 229 行、§4.3 第 897 行、§4.3 第三条推演第 994 行）。

- 反证尝试：
  1. 追 C 侧 EBP 的实际值。`head.S:69–70` 先把栈立到 `load_stack_start` 并把 EBP 清零，`:77` 调用 `pre_init`；`pre_init.c:217` 的签名是 `kinfo_t *pre_init(u32_t magic, u32_t ebx)`，`:237` 以 `return &kinfo;` 正常返回。在带帧指针的编译下，`pre_init` 的序言把 EBP（值为 0）压进自己的帧，尾声 `pop %ebp`（或 `leave`）在 `ret` 之前把它恢复成 0。所以 `head.S:87` 的 `call kmain` 发生时 EBP 已经是 0，`kmain` 的序言压进保存槽的是 0，不是 `pre_init` 的帧指针。
  2. 把第 1 步从"带帧指针编译"这个可能有争议的前提上摘下来。EBP 是 callee-saved，与编译器是否把它当帧指针无关：只要 `pre_init` 是正常返回的 C 函数（`:237` 的 `return &kinfo;`），它在返回前必须把 EBP 恢复成入口值，也就是 `head.S:70` 写的 0。所以 `call kmain` 时 EBP 只能是 0。反过来，如果编译器在 i386 `-O2`（`minix3/share/mk/sys.mk:55–71` 对非 aarch64 架构取 `-O2`，`-fomit-frame-pointer` 只写在 aarch64 分支；`minix3/minix/kernel/Makefile:8–12` 的 `DBG=-O0` 只对 `earm` 且 `MKLLVM=yes` 生效）下把帧指针整个省掉，`kmain` 就连 EBP 保存槽都没有。两条分支都得不到文档说的"槽里是 `pre_init` 的帧指针"。我不能在这份报告里断定 C 侧工具链落在哪条分支（C 内核的构建产物不在仓库内），但这不影响反证：两条分支都否证该主张。
  3. 查 Rust 侧是否真有"`kmain` 的保存槽"。反汇编生产工件 `os/target/x86_64-unknown-none/release/kernel` 的 `kmain`（符号 `_ZN12minix_kernel5kmain17h94a869568c2e2d66E`，地址 `0xffff80000020be60`）：序言是 `push %r14; push %r12; push %rbx`，没有 `push %rbp` / `mov %rsp, %rbp`。也就是说当前 release 工件里 `kmain` 根本没有帧指针保存槽，"保存槽里是 0"描述的是一个不存在的槽。
  4. 查"Rust 内核强制帧指针"的构建开关。全仓 `grep -rn force-frame-pointers os --include=*.toml --include=*.rs --include=*.sh` 只命中 `os/arch/src/x86_64/boot.rs:420–422` 的一句注释：帧指针链回溯要求编译期打开 `-C force-frame-pointers`，否则 `rbp` 是通用寄存器、回溯立刻停止。`os/Cargo.toml:301–305` 的 profile 只设 `panic = "abort"`；`os/kernel-image/build.rs`、`os/.cargo/config.toml`、`check-layout.sh` 的构建命令都没有这个旗标。

- 结果：阻断（P1，事实错误；按项目"P0-fact = 事实错误"的口径可判 P0）。它不推翻 CLAIM 04 的跳转机制与 trait 契约，但它使 §2.3 的"两侧终止方式不同"、§4.3 的"`xor rbp` 让保存槽为 0、链在 0 处终止"以及 §6.1 的一整条诊断判据同时失去依据。

- 证据：
  - `minix3/minix/kernel/arch/i386/head.S:69–70`（`mov $0, %ebp`）、`:77`（`call pre_init`）、`:84–87`（设栈后 `call kmain`）。
  - `minix3/minix/kernel/arch/i386/pre_init.c:217`（返回 `kinfo_t *`）、`:237`（`return &kinfo;`）。
  - `minix3/share/mk/sys.mk:55–71`（分支表：`-fomit-frame-pointer` 只出现在 aarch64 分支；i386 落 `-O2`）与 `minix3/minix/kernel/Makefile:8–12`（`DBG=-O0` 只对 `earm` + LLVM 生效）——用来界定"帧指针可能被省"的分支，而不是用来断言它一定被保留。
  - `os/target/x86_64-unknown-none/release/kernel` 的 `objdump -d`（`kmain` 序言无 `%rbp`）。
  - `os/arch/src/x86_64/boot.rs:414–422`（"Frame pointer must be enabled at compile time (`-C force-frame-pointers`) … Without frame pointers, `rbp` is a general-purpose register and the walk stops immediately"）。
  - 文档 §6.1 第 1179 行把"栈回溯出现与调用关系无关的地址"归因到"清帧指针动作缺失"；按上面第 3、4 步，当前构建里更可能的成因是"帧指针根本没有被强制打开"，而不是 `higher_half.rs` 少了一条 `xor rbp`。

- 若我错了：如果生产构建实际带了 `-C force-frame-pointers`（例如由未入库的打包脚本或 CI 环境变量注入），Rust 侧的"保存槽"就存在；但即便如此，C 侧 `pre_init` 恢复 EBP 为 0 这一步仍然成立，文档"C 保存的是 `pre_init` 帧指针"的说法仍然错。要让整条主张成立，需要同时否证这两步，而它们各自都有直接的源码/工件证据。
- 关于证据时效：我反汇编的 `os/target/x86_64-unknown-none/release/kernel` 构建于 2026-11-11 03:45，之后只有一个提交（67db5ef37，19:07）动过 `os/kernel/src/lib.rs`（`kmain` 改用 `store_kernel_info` 返回值）；`higher_half.rs`、`os/.cargo/config.toml`、`os/kernel-image/build.rs` 与 profile 设置在这段时间里没有改动。帧指针是否发射由目标规范与优化档决定，不随这次 `kmain` 体内改动翻转，所以结论不受影响；要彻底排除这层疑虑，用 `check-layout.sh` 同款命令重建一次再 `objdump -d` 即可（本报告按只读纪律没有重建）。

---

## CLAIM 06

- 主张：§4.2 那条链上的每个环节都有与当前源码一致的宿主测试与代码引用（§5.1 的 23/18 两个计数、§4.2 的两段"代码引用"、§4.3/§4.4 的代码节选、以及全部 `[fn …](path:fn …)` 锚点）。

- 反证尝试：
  1. 原样重跑文档给出的两条复算命令。`grep -c '    fn test_' os/libs/minix-elf/src/lib.rs` → `23`，与文档一致；`grep -c '    fn test_' os/boot-shim/src/loader.rs` → `21`，文档写 `18`（第 1106 行）。按 `#[test]` 数则是 `22`（多出的一个是 `source_destination_overlap_stops_before_any_segment_write`，函数名不以 `test_` 开头）。
  2. 逐个检查文档里带"定义锚点"的符号是否在所指文件里仍有定义。两条失败：
     - `os/boot-shim/src/loader.rs:fn load_boot_modules_with_loader`：函数已在提交 `73f91ba52`（2026-11-09）删除，现名 `load_boot_modules_into<L: FileLoader>`（`loader.rs:422`）。
     - `os/kernel-image/src/bootface.rs:fn place_modules`：函数已移到 `os/kernel-image/src/bootface_common.rs:177`，`bootface.rs:296` 只剩调用。
  3. 检查 §4.3 riscv64 节选是否还能对上当前源码。文档写 `kinfo.kern_virt_base().0` 与 `kinfo.kern_phys_base().0`（第 928–933 行）；提交 `ea6cb51c4`（2026-11-09）删掉了 12 个纯转发 getter，当前源码是字段直读 `kinfo.kern_virt_base.0` / `kinfo.kern_phys_base.0`（`os/kernel/src/arch/riscv64/higher_half.rs:107–110`）。文档节选照抄不能编译。
  4. 检查 §4.4 的 `build_kernel_info` 调用节选（第 1038–1049 行，10 个实参，含 `PhysBytes(0), 0`）。当前签名是 11 个参数，第 7、8 位是 `platform_sources`/`param_buf`，尾部多出 `memmap_sources`/`startup_objects`/`authority`（`os/boot-shim/src/lib.rs:101–115`）；`bootface.rs:427–442` 的实参里已无 `PhysBytes(0), 0`。文档节选来自更早的签名形态。
  5. 检查 §4.2 标着"代码引用（保留函数体）"的 `load_segments_into_phys_memory`（第 838–861 行）。当前函数体在 `first_overlapping_pair` 之后还有一道 `first_source_destination_overlap` 检查，命中即 `panic!`，且它先于任何一次段写入执行（`loader.rs:292–302`）；这道闸配有专门的测试 `source_destination_overlap_stops_before_any_segment_write`（`loader.rs:698`）。文档的"完整函数体"节选里没有它，§5.1 的测试清单也没有它。
  6. 追寻时间线，确认不是"我读错了版本"。文档最后一次修改是 `96cea283e`（2026-11-07 05:53）；上面 5 处变动分别来自 2026-11-08（`first_source_destination_overlap`）与 2026-11-09（删 getter、删 `load_boot_modules_with_loader`、`place_modules` 迁址、`build_kernel_info` 加 `authority`）。在文档自己的提交上，这些内容是对的；对当前工作树，它们已经过期。

- 结果：阻断（P1，文档-代码同步门与锚点纪律门）。核心机制（解析器语义、摆放与清零语义、测试覆盖的**类别**）仍然成立，但本文件已不能作为当前源码的可核对说明：一条函数名指向已删除的符号、一条锚点指向已迁址的函数、一段"完整函数体"缺一道真实防线、两段代码节选照抄不能成立、一个测试计数与它自己的复算命令不符。

- 证据：
  - `grep -c '    fn test_' os/boot-shim/src/loader.rs` → `21`；`grep -c '#\[test\]' os/boot-shim/src/loader.rs` → `22`；文档第 1106 行写 `18`。
  - `git log --format='%h %ci %s' -1 73f91ba52` → `2026-11-09`，提交说明"删除 load_boot_modules_with_loader 遗留 leaks-slice 包装"。
  - `git log --format='%h %ci %s' -1 ea6cb51c4` → `2026-11-09`，提交说明"删 12 个纯转发 getter，字段直读"。
  - `git log -1 --format='%h %ci' -- rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` → `96cea283e 2026-11-07 05:53:25`。
  - `os/boot-shim/src/loader.rs:286–316`（当前函数体，含 `:292–302` 的源/目标重叠 panic 闸）、`:422`（`load_boot_modules_into`）、`:698`（对应测试）。
  - `os/kernel-image/src/bootface_common.rs:177`（`place_modules` 现址）。
  - `os/boot-shim/src/lib.rs:101–115`（11 参签名）。

- 若我错了：如果评审基线是文档自己的历史提交而不是当前工作树，第 1–5 步全部不成立，只剩计数口径之争（`fn test_` 与 `#[test]` 的差别）。但项目真值序把 Rust 实现放在文档之上，且 Doc Code Sync 是每次文档评审的强制门，所以我按当前工作树判。

---

## 汇总

| 主张 | 结果 | 一句话 |
|---|---|---|
| 01 C 侧双地址、`AT()` 平移一致、`k_initial_stktop` 落高半 | 反证失败（主张成立；附 P2 推演不成立） | 五条可能让它落低侧的路径逐条走死：改名名单、输入通配、脚本次序、段非空、无第二处定义；但 §2.2 的"把 kmain 放进名单"反方向推演站不住（`objcopy` 也改定义名，低侧副本成死代码） |
| 02 三种链接脚本形态与"谁读 `p_paddr`"判据 | 反证失败（主张成立） | `: AT(` 计数 4/4/4 与 0/0/0；全仓无把 arch `link.ld` 当链接输入的路径；载体脚本单地址 |
| 03 栈顶 = `kern_virt_base + kern_size` + 2MiB 收口 | 反证失败（主张成立；附 P2 跨节不一致） | 三份工件实测镜像尾等于算式值；L0–L9 全 PASS；§3.3 的"映射越界"与 §3.5 的"写入前 panic"互相矛盾 |
| 04 `HigherHalf` 契约与三架构寻址 | 反证失败（主张成立；附一条仓库外依赖观察） | 三份 asm 与文档逐条对上；只有"aarch64 direct-core 低地址进入"依赖仓库外行为，文档已自标 |
| 05 C 与 Rust 的帧指针链终止方式不同 | **阻断（P1，事实错误）** | C 侧 EBP 在 `call kmain` 前必为 0（callee-saved）或根本没有帧指针；Rust 生产 `kmain` 反汇编无 `%rbp` 序言，且全仓没有 `-C force-frame-pointers` |
| 06 测试与代码引用与当前源码一致 | **阻断（P1，文档-代码同步）** | 测试计数 18 vs 21/22；两个锚点指向已删除/已迁址符号；两段代码节选照抄不能成立；一段"完整函数体"缺一道 2026-11-08 才加入的重叠防线 |

说明：
- 我对"阻断"的用法是：这条主张被否证，且按项目的 P0/P1 口径会挡 CONVERGED。CLAIM 05 若按"P0-fact = 事实错误"严格归类，应记 P0；我记 P1 是因为它不推翻本篇的架构结论（trait、前置条件、三种寻址）。
- 六条里四条是"反证失败"。这不是为了凑数：CLAIM 01/02/03/04 的每条反证路径我都写进了上面的"反证尝试"，走死的原因也写在同处；如果评审要抽查，优先重放 CLAIM 03 的 `SKIP_BUILD=1 bash os/kernel-image/check-layout.sh all` 与 CLAIM 05 的 `objdump -d`。
- 本轮列出的文档-代码漂移全部发生在文档最后一次修改（2026-11-07）之后的两天里（2026-11-08、2026-11-09）。这不是"作者写错"，而是"后续重构没有回写文档"；修复方向应是同步文档而不是改代码。
