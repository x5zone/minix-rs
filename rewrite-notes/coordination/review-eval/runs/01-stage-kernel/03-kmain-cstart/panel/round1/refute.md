# D03 R1 反证报告（P-REFUTE）

基线：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md`，1382 行，sha256 前缀 `469727d74e1ec63c`。
只读范围：该文档、`tools/doc-code-map.sh --check` 列出的 21 个 Rust 文件、文档引用的 `minix3/` C 源、设计合同 `rewrite-notes/01-stage-kernel/.design/03-design.v2.md`（只作历史对照，冲突以文档与源码为准）。
方法：每条主张先复述，再沿三条路找反例——(a) 源码里的相反路径或另一份实现，(b) 构建/链接规则决定的真实符号，(c) 失败路径（资源、错误码、回滚、并发）。只列能改变结论的反例；反证不成立时如实写“反证失败”。

---

## CLAIM 01

- **主张**（§2.1，文档第 140 行与第 178 行）：`kmain` 开头的两行 `memcpy` 是“全局自拷贝”。原文：“`pre_init` 的收尾是 `return &kinfo;`，`head.S` 把这个指针压栈后传给 `kmain`，所以 `local_cbi == &kinfo`，第一行是一次全局自拷贝。第二行同理：……说明 `kinfo.kmess` 就是全局缓冲区 `kmessages` 自己的地址。”随后第 178 行据此断言：“把第二步的两行自拷贝 `memcpy` 删掉会怎样：源与目的同一个对象，删掉前后 `kinfo`/`kmessages` 的内容没有区别，这一刀在 C 侧切不出后果；反事实要放到 Rust 侧才成立。”

- **反证尝试**：按链接期的实际符号走一遍，而不是按函数体内的同名标识符走。

  1. `pre_init.c` 与 `head.S` 都不在 `SRCS` 里。它们只以 `unpaged_*.o` 的形式进入链接（`minix3/minix/kernel/arch/i386/Makefile.inc:51-71`）：`objcopy --prefix-symbols=__k_unpaged_` 把对象文件里的**全部**符号加前缀。我在宿主上用最小对象复现过这条规则：定义符号 `defined_var` 与未定义引用 `undef_var` 同时被改写成 `__k_unpaged_defined_var` 与 `U __k_unpaged_undef_var`（`objcopy --prefix-symbols=__k_unpaged_ a.o b.o; nm b.o`）。因此 `pre_init.o` 里的 `kinfo` 与 `kmessages`（定义在 `minix3/minix/kernel/arch/i386/pre_init.c:20-21`）在镜像中叫 `__k_unpaged_kinfo` 与 `__k_unpaged_kmessages`，落在 `.unpaged_bss`（`minix3/minix/kernel/arch/i386/kernel.lds:15-20`）。
  2. `kmain` 所在的 `main.c` 是分页段代码，不经过改名。它经 `glo.h:22` 的 `extern struct kinfo kinfo;` 使用的 `kinfo`，由 `minix3/minix/kernel/usermapped_data.c:7` 的 `struct kinfo kinfo __section(".usermapped");` 定义；`kmessages` 同理在 `usermapped_data.c:9`。这两个对象位于高半 `.usermapped` 段，与 `.unpaged_bss` 不是同一块内存。这里有一个容易误读的细节：`proto.h:49-51` 在 `#ifndef UNPAGED` 下把 `kmain` 宏映射成 `__k_unpaged_kmain`，所以分页段 `main.c` 定义的符号名字带前缀；**段放置看的是对象文件名是否匹配 `unpaged_*.o`，不是符号名**（`kernel.lds:17-19`），`main.o` 不匹配，仍进 `.text`。符号名带前缀不代表代码在未分页段。
  3. `head.S:77-87` 传给 `kmain` 的参数，是 `pre_init` 返回的 `&__k_unpaged_kinfo`；`main.c:128` 的 `&kinfo` 是 `.usermapped` 副本。所以第一行是**跨区拷贝**，不是自拷贝。第二行 `memcpy(&kmess, kinfo.kmess, sizeof(kmess))` 的源是 `kinfo.kmess = &kmess`（在 `pre_init.c` 里等于 `&__k_unpaged_kmessages`，见 `pre_init.c:117`），目的是 `.usermapped` 的 `kmessages`，同样是跨区拷贝。
  4. 后果推演：删掉这两行，`.usermapped` 的 `kinfo`/`kmessages` 不会自动获得 `pre_init` 采集的数据（它们不是同一对象；后续代码只会零散改写个别字段，采集到的内存映射、模块表、参数缓冲、诊断消息没有第二个写入者）。随后分页段代码读到的 `kinfo`（`cstart` 的 `kinfo.user_sp`、`pg_identity(&kinfo)` 的 `mem_high_phys`、boot image 循环的 `kinfo.module_list` 等，`main.c:181`、`protect.c:361`）都会是空值；`memory.c:878-881` 还把 `&kinfo` 与 `&kmessages` 的地址发布进 `minix_kerninfo`（`ASSIGN(kinfo)`、`ASSIGN(kmessages)`），用户态 VM/IS 读到的也会是空副本。也就是说，删掉不是“切不出后果”，而是启动信息根本没有搬到读者手里。
  5. 文档自己在 §2.6 第 661 行引用了“改名名单”这个机制（“`pre_init.o` 与 `pg_utils.o` 都在改名名单里”），但在 §2.1 没有把它用到 `kinfo`/`kmessages` 上。两节对同一构建机制给出两种互斥的读法，这是跨节复述冲突（见文末“跨节复述冲突”第 1 条）。

- **结果**：**反证成功**。分级：阻断，P0-fact。错误不在细节，而在 §2.1 的结论句与它支撑的反事实：C 侧这两行是必需的跨区搬运，不是可删的自拷贝；文档把它当作“C 无后果、Rust 才出问题”的对照，对照的另一半不成立。

- **证据**：`minix3/minix/kernel/arch/i386/Makefile.inc:51-71`（改名循环）、`:76`（`SRCS` 不含 `pre_init.c`）；`minix3/minix/kernel/arch/i386/kernel.lds:15-20`（`.unpaged_bss`）；`minix3/minix/kernel/arch/i386/pre_init.c:20-21,117,237`；`minix3/minix/kernel/usermapped_data.c:7,9`；`minix3/minix/kernel/main.c:128-129,181`；`minix3/minix/kernel/arch/i386/memory.c:878-881`；宿主 `objcopy` 复现（`nm` 显示未定义引用也被加前缀）。

- **若我错了**：只有两种情况能救回原文。第一，链接时还存在一份未改名的 `pre_init.o`，让 `main.c` 的 `kinfo` 与 `pre_init` 的定义重合——但 `pre_init.c` 从未进入 `SRCS`，`kernel.lds:17-19` 只收 `unpaged_*.o`，`ORIG_UNPAGED_OBJS` 只进 `CLEANFILES`。第二，`.usermapped` 的 `kinfo` 只是对 `__k_unpaged_kinfo` 的别名——但 `memory.c` 的 `FIXEDPTR` 是对同一对象做的用户态虚拟地址重映射，不是第二个定义点，且 `usermapped_data.c` 是独立定义。两条都不成立，所以我保留这个反证。

---

## CLAIM 02

- **主张**（§2.6 第 661 行并延伸到第 668 行；§3.5 第 892 行复述）：页表重建的解释是——`pg_utils.o` 在改名名单里，“它的调用解析到 `pre_init.c` 的恒等版”；“`pagedir` 本身落在 `.unpaged_bss`”；因此判据是“看这个符号/数据落在哪一段”。第 668 行进一步把重建描述成“原地重填”：`pg_clear()` 清空“同一个静态页目录数组”，清零与重填之间不写 CR3，“这几步自身的取指与写表靠 TLB 里的旧条目继续走”。§3.5 第 892 行原样复述：“C 的‘原地重填’成立靠两点——清空与重填之间不写 CR3（取指靠 TLB 旧条目，§2.6），以及 `pagedir` 是一个可以安全重填的静态数组。”

- **反证尝试**：检查 `pg_utils.c` 到底被链接成几份对象。

  1. `minix3/minix/kernel/arch/i386/Makefile.inc:76` 把 `pg_utils.c` 放进 `SRCS`，因此生成一份普通（分页）`pg_utils.o`，符号不加前缀；同一文件 `:51-54` 又把 `pg_utils.o` 放进改名循环，生成 `unpaged_pg_utils.o`；`:83` 的 `OBJS.kernel += ${UNPAGED_OBJS}` 只是在 `SRCS` 派生的对象之外追加，`bsd.prog.mk` 不会删掉普通那份（`minix3/share/mk/bsd.prog.mk:420,523,629`）。所以镜像里有**两份** `pg_utils` 代码、两个 `pagedir` 数组。
  2. 普通那份的 `pagedir`（`pg_utils.c:19` 的 `static u32_t pagedir[1024]`）落在 `.bss`，VMA 是高半地址；改名那份的 `pagedir` 落在 `.unpaged_bss`，VMA 等于物理地址（`kernel.lds:15-19` 的 `unpaged_*.o(.bss COMMON)` 与 `*(.bss* COMMON)`）。
  3. 普通 `pg_utils.o` 里 `pg_load` 对 `vir2phys` 的引用是**不带前缀**的未定义符号，链接时只能解析到 `protect.c:33-39` 那份减平移量的实现（整个镜像里唯一的不带前缀 `vir2phys`；`pre_init.c:24` 那份已被改名为 `__k_unpaged_vir2phys`）。也就是说，`prot_init` 实际调用的 `pg_load`——§2.6 代码块引用的那一个——用的不是恒等版，而是减平移版；它的 `pagedir` 也不在 `.unpaged_bss`，而在 `.bss`。
  4. `reductio`：如果原文对 `prot_init` 路径成立（`pg_load` 用恒等版），写入 CR3 的将是 `.bss` 的高半虚拟地址（例如 `0xF04xxxxx`）。32 位机器把这样的值当页目录物理地址用，下一次页表遍历就会失败，Minix3 根本起不来；而文档 §2.2、§2.3 都以“cstart 能跑完”为前提。现实与原文模型只能有一个成立，成立的是“分页副本 + 减平移版”。
  5. “原地重填”的机制也随之不成立：`prot_init` 运行时 CR3 指向的是 `pre_init` 用改名版 `pg_load` 写进去的**改名版 pagedir**；`pg_clear()` 清空的是普通版 `pagedir`（一块尚未激活的内存）。清空、重填期间，取指一直走改名版那张仍然有效的表，直到 `pg_load()` 把 CR3 切到普通版。也就是说，C 侧不是“在活动表上原地重填并靠 TLB 旧条目撑过窗口”，而是“先建好另一张静态表，最后一步切换”。这解释了源码注释里的 “a new post-relocate bootstrap pagetable”。
  6. 需要的澄清：§2.6 的两条重建理由（给 VM 装载用、恒等窗口给 LAPIC/显存用）仍然成立；§3.5 的决策（Rust 不在 `init_protection` 里照搬重建）也不因本条被推翻，因为 Rust 的问题是“根表由 `arch_boot_impl` 建立且 Direct Map 叶子登记挂在上面”，与 C 是两张静态表这一点无关。被推翻的是“为什么 C 能这么做”的机制叙述与“只有一个 pagedir”的模型。

- **结果**：**反证成功**。分级：阻断，P0-fact。修复不应改动 §3.5 的决策，但必须把 §2.6 第 661 行与第 668 行改写成“两份对象、两张表、最后一步切 CR3”，并删掉 §3.5 里对“原地重填 + TLB 旧条目”的复述。

- **证据**：`minix3/minix/kernel/arch/i386/Makefile.inc:51-54,76,83`；`minix3/share/mk/bsd.prog.mk:420,523,629`；`minix3/minix/kernel/arch/i386/kernel.lds:15-19`；`minix3/minix/kernel/arch/i386/pg_utils.c:19,247-252`；`minix3/minix/kernel/arch/i386/protect.c:33-39,360-363`；`minix3/minix/kernel/arch/i386/pre_init.c:24,230-233`。

- **若我错了**：如果构建立有一条规则把普通 `pg_utils.o` 从链接输入里移除（例如 `OBJS.kernel` 被整体覆盖而不是追加），或者头文件里存在把所有 `pg_*` 名字映射到 `__k_unpaged_*` 的宏，那么“单份改名对象”就成立。实际检查结果：`OBJS.kernel` 在 `SRCS` 派生之后追加 `UNPAGED_OBJS`；`arch_proto.h:209-213` 是无宏的普通声明，全树没有 `#define pg_clear/pg_load/...`。这条反证依赖构建规则，最硬的复核方式是构建 i386 内核后 `nm kernel | grep -E ' (pg_load|__k_unpaged_pg_load)$'` 应看到两个符号；本环境没有 Minix3 交叉构建产物，但 Makefile 与链接脚本的证据是直接的。

---

## CLAIM 03

- **主张**（§4.1 第 964 行，§4.7 第 1187 行的表项）：Rust 侧保留了 `kernel_may_alloc` 闸门。§4.1 原句：“随后三步各有 C 侧对应：`KERNEL_MAY_ALLOC.store(true, Ordering::Release)`（闸门；Rust 侧的 `fn kernel_may_alloc` getter 当前没有生产调用者——`grep -rn 'kernel_may_alloc()' os --include=*.rs` 零命中；……）”。§4.7 原句：“`kernel_may_alloc` 闸门 | `static KERNEL_MAY_ALLOC`（Phase A 置 true） | 保留（改型为原子量） | 同一语义：VM 启动前允许内核直接分配”。

- **反证尝试**：在现源码里找这两个符号与那次 `store`。

  1. 全树检索（排除构建产物）：`grep -rn "KERNEL_MAY_ALLOC\|kernel_may_alloc" os --include=*.rs | grep -v "/target/"` 只有三处命中，全部是注释——`os/kernel/src/lib.rs:3701`（`/// 5. Set kernel_may_alloc = false`）、`:3706`（`/// Design decision D8 ... uses AtomicBool`）、`:3916`（Step 8 注释）。没有静态量，没有 getter，没有 `store` 调用。
  2. `os/kernel/src/globals.rs` 的静态量清单里没有 `KERNEL_MAY_ALLOC`（该文件的 `AtomicBool` 只有 `VM_RUNNING` 与 `KRANDOM_INIT`）。
  3. `kmain` 的 Phase A 函数体（`lib.rs:1673-1900`）从全局槽取 `KernelInfo`、`validate`、`check_boot_postconditions`、控制台初始化、`store_kernel_info`、平台初始化，然后直接进入 Phase B；没有 `KERNEL_MAY_ALLOC.store(true, ...)`。
  4. 代码反而明确记录了该闸门已被淘汰：`lib.rs:3916-3919` 的 Step 8 注释写“C 的 `kernel_may_alloc = 0`（main.c:105）在 Rust 侧没有对应物——该开关唯一的读者是已删除的 `add_memmap` 断言；账本换成 `FreeRangeSet` 后……不需要运行期布尔。”这与 §4.7 的“保留（改型为原子量）”正好相反，实际处置是“淘汰”。
  5. 二次核对：文档所称的锚点 `os/kernel/src/lib.rs:fn kernel_may_alloc` 在源码中不存在；按 §4.1 的命令原样执行会命中一个构建产物 `os/target/fmtchk/kernel_src_lib.rs:1850` 里的旧副本，所以“零命中”这条可复现读数在带 `os/target` 的工作树里也不成立（应加 `--exclude-dir=target` 才能表达文档的本意）。

- **结果**：**反证成功**。分级：阻断，P0-fact（文档与代码同步项）。修复方向不是补代码，而是把 §4.1 的 `store(true)` 与 §4.7 的“保留（改型为原子量）”改成“淘汰；理由见 bsp_finish_booting Step 8 注释”，并删掉不存在的 getter 锚点。

- **证据**：`os/kernel/src/lib.rs:1673-1900`（kmain Phase A 函数体）、`:3701,3706,3916-3919`；`os/kernel/src/globals.rs`（静态量清单无 `KERNEL_MAY_ALLOC`）；文档第 964、1187 行；设计合同 `rewrite-notes/01-stage-kernel/.design/03-design.v2.md:113` 也停在“`memmap::add_memmap` 的 production `debug_assert` 使用”，而 `add_memmap` 与 `memmap` 模块在当前 `os/kernel/src` 中已不存在（`grep -rn "add_memmap" os/kernel/src` 只余注释）。

- **若我错了**：如果 `KERNEL_MAY_ALLOC` 位于某个未被 `grep` 覆盖的 crate 或生成文件里，或者被条件编译藏在宏展开后不可见的文本里，我的结论会失效。实际是全仓（除 `os/target` 构建产物与 `minix3/`）检索零定义；宏展开不会把标识符从源码文本里抹掉（宏定义与调用都会出现）。因此保留这个反证。

---

## CLAIM 04

- **主张**（§2.4 第 455 行的 TSS 字段表）：`iobase = sizeof(struct tss_s)` 高于 TSS 限长，因此“没有 I/O 位图”；“用户态 `in`/`out` 因此全部被拒”。

- **反证尝试**：x86 的 I/O 权限判定不是单层位图，而是两层。

  1. 处理器先比较 CPL 与 EFLAGS.IOPL：当 CPL ≤ IOPL 时，`in`/`out` 直接执行，**不查** TSS 位图；只有 CPL > IOPL 时才落到位图检查。位图不存在（`iobase` 大于限长）只决定第二层的默认结果。
  2. Minix3 自己给用户进程提权：`minix3/minix/kernel/arch/i386/protect.c:44-52` 的 `enable_iop` 把 `pp->p_reg.psw |= 0x3000`（IOPL 两位）写进进程状态，注释原文说明这是给用户进程的 “I/O Protection Level bits”。该函数由内核调用 `SYS_IOPENABLE`（`minix3/minix/kernel/arch/i386/do_iopenable.c:20-31`）触达，用户态侧有 `sys_enable_iop`。
  3. 真实使用者：内存字符设备在第一次打开时调用它（`minix3/minix/drivers/storage/memory/memory.c:366-375`，`sys_enable_iop(user_endpt)`）。拿到 IOPL=3 的进程随后执行 `in`/`out` 不受空位图约束。
  4. 因此准确的说法是：空位图让**未获 IOPL=3 的**用户进程的 I/O 指令默认触发 #GP；“全部被拒”不成立。字段本身的装填（`sizeof(struct tss_s)` = 104，限长 103）与“位图为空”的判断没有错，错的只是从位图为空推到“用户态 I/O 全拒”这一步。

- **结果**：**反证成功**。分级：阻断，P1-fact。单行事实，但它把 I/O 权限模型讲成了“位图决定一切”，读代码的人会误以为 `enable_iop`/`SYS_IOPENABLE` 没有作用。

- **证据**：文档第 455 行；`minix3/minix/kernel/arch/i386/protect.c:44-52`；`minix3/minix/kernel/arch/i386/do_iopenable.c:20-31`；`minix3/minix/drivers/storage/memory/memory.c:366-375`；`minix3/minix/kernel/arch/i386/include/archconst.h:59-61`（`PRESENT`/`DPL_SHIFT` 与位图字段同一份常量表）。

- **若我错了**：如果 `enable_iop` 是死代码、`SYS_IOPENABLE` 从未被用户态调用，那么“用户态 I/O 全拒”在运行期就近似成立。但 `do_iopenable` 是活的系统调用入口，内存驱动会调它；`protect.c:51` 的 `psw |= 0x3000` 是用户进程侧授予 IOPL=3 的写入点（常量表里另有 `INIT_TASK_PSW 0x1200` 给内核任务预置 IOPL=1，不改变用户态结论）。另一种救法是把原句读成“位图层全部被拒”——字面写的不是这个意思，句子里没有“位图层”限定。因此保留这个反证。

---

## CLAIM 05

- **主张**（§2.2 第 261-276 行，§2.8 第 722-729 行）：`cstart` 的四次调用存在一条不可任意交换的次序。具体是：`prot_init()` 是后面所有步骤的前提；`init_clock()` 只初始化 `kclockinfo`/`kloadinfo` 并读 `hz`，与 `prot_init` 可交换；`intr_init(0)` 只按 ICW 序列编程两片 8259 并在收尾屏蔽全部中断线，不填 IDT，把它提到 `prot_init` 之前不会新造中断窗口（可屏蔽中断被 IF=0 挡住）；`arch_init()` 需要保护结构与控制器都已就绪，在 `USE_APIC && !CONFIG_SMP` 分支它自己会编程 LAPIC 并改写 IDT。以及 §2.8 的 `prot_init_done` 次序论证：唯一读者是 `arch_smp.c` 的 `copy_trampoline`，它在 `cstart` 之后的 `smp_init` 里才被调用，所以“把 `prot_init_done = 1` 提到 `prot_load_selectors()` 之前”这个反事实的前提不成立。

- **反证尝试**：逐条找相反路径。

  1. `minix3/minix/kernel/arch/i386/i8259.c:33-59` 的 `intr_init` 全文只有 `outb`：ICW1 到 ICW4、master 掩码 `~(1 << CASCADE_IRQ)`、slave 掩码 `~0`，然后 `return OK`。没有 `idt_*` 调用，也没有别的硬件对象。
  2. `minix3/minix/kernel/clock.c:48-66` 的 `init_clock` 只做 `memset(&kclockinfo, ...)`、`env_get("hz")` 加 `[2, 50000]` 夹取与 `DEFAULT_HZ` 回退、`memset(&kloadinfo, ...)`。没有定时器编程。
  3. IF 状态：`minix3/minix/kernel/arch/i386/head.S:71-73` 用 `push $0` / `popf` 清标志位；全树最早的 `intr_enable()` 在 `arch_clock.c:109,113` 与 `apic.c:500,504`，都属于 `boot_cpu_init_timer` 走的 CPU 频率标定路径，晚于 `cstart`。所以“提前 `intr_init` 不制造可屏蔽中断”成立。
  4. `arch_system.c:246-280` 的 `arch_init`：`!CONFIG_SMP` 下再次 `tss_init(0, get_k_stack_top(0))`；串口与 ACPI 初始化；`USE_APIC && !CONFIG_SMP` 下调用 `apic_single_cpu_init()`；最后 `cut_memmap` 两段 BIOS 区。`apic.c:1125-1157` 的 `apic_single_cpu_init` 确实做 `lapic_enable`、`detect_ioapics`、`apic_idt_init(0)`、`idt_reload()`。文档的“APIC 构建里控制器已就绪的前提来自 `arch_init` 自己”成立。
  5. `prot_init_done` 的读者：全树只有 `arch_smp.c:87` 的 `assert(prot_init_done)`，所在函数 `copy_trampoline` 由 `smp_start_aps`（`arch_smp.c:120`）调用，而 `smp_start_aps` 在 `smp_init` 内、`smp_init` 在 `cstart` 之后。§2.8 的反事实前提确实不成立。
  6. 我尝试构造的反方向：若 `init_clock` 写的 `kclockinfo` 会被 `intr_init` 读到，则二者不可交换；但 `i8259.c` 全文不引用 `kclockinfo`/`system_hz`。若 `hw_intr` 层在 `intr_init` 里隐式填门，`intr_init` 的实现没有这条调用。没有找到相反路径。

- **结果**：**反证失败**。主张成立，文档这一节的 C 侧事实与次序论证经得起核对。分级：无（非阻断，不需要修）。

- **证据**：`minix3/minix/kernel/arch/i386/i8259.c:33-59`；`minix3/minix/kernel/clock.c:48-66`；`minix3/minix/kernel/arch/i386/head.S:71-73`；`minix3/minix/kernel/arch/i386/arch_clock.c:95-120`；`minix3/minix/kernel/arch/i386/apic.c:485-510,1125-1157`；`minix3/minix/kernel/arch/i386/arch_system.c:246-280`；`minix3/minix/kernel/arch/i386/arch_smp.c:68-125`。

- **若我错了**：如果 `intr_init` 在某个未读到的 `#ifdef` 分支里填 IDT，或 `init_clock` 有架构特化版本编程硬件，我的“成立”判断会错。源码里 `intr_init` 是单一实现、无内容条件编译；`init_clock` 在公共 `clock.c`，无 `#ifdef` 分支。因此保留“反证失败”的结论。

---

## CLAIM 06

- **主张**（§3.7 第 929-948 行，§4.4 第 1062-1111 行）：门的数量与装载顺序不变量。具体是：`STUB_VECTORS` 共 41 项，恰好覆盖 `X86_64TrapEntry::init()` 标记为 present 的门；`install_trap_stubs` 在 `load()` 之前把 41 个真实入口一次写齐；`set_handler` 写门时把 IST 置 0，随后由 `set_gate_ist` 恢复向量 2 与 8 的 IST1/IST2；`gate_present` 的交叉核对只由宿主测试执行，`load()` 自己不检查，x86_64 的 `test-protection` 载体不调 `install_trap_stubs`、门里保持占位 0 也能通过。算术：41 = 39 − 1 + 3，其中 C 的 39 = 16 + 23，23 条门含向量 9、不含向量 15。

- **反证尝试**：

  1. 逐项数 `os/arch/src/x86_64/trap_stub.rs:168-215` 的 `STUB_VECTORS`：异常 18（0-8 共 9，10-14 共 5，16-19 共 4）、软中断 4（32-35）、PIC 16（0x50-0x57、0x70-0x77）、单独向量 3（0xF0、0xF1、0xFF），合计 41。`X86_64TrapEntry::init()`（`trap_entry.rs:198-261`）标记 present 的集合与它逐项相同，9 与 15 不 present。没有重复项，也没有“有 stub 无门”或“有门无 stub”。
  2. `install_idt_handlers`（`trap_stub.rs:613-637`）遍历 `STUB_VECTORS`，对每项 `stub_va` 取地址后 `set_handler`，随后 `match vector { 2 => set_gate_ist(2,1), 8 => set_gate_ist(8,2) }`。`set_handler`（`trap_entry.rs:333-341`）固定写 `ist=0` 且 `is_trap=false`。所以 IST 的“抹掉再恢复”链条与文档一致。
  3. 装载顺序：`os/kernel/src/lib.rs:2274-2282` 的 `init_protection` 先 `install_trap_stubs(&mut trap)`、`configure_syscall(...)`，再 `store_trap_entry(trap)`，最后 `with_trap_entry(|trap| trap.load())`。`load()`（`trap_entry.rs:302-327`）只执行 `lidt`，不检查门。宿主测试 `test_stub_coverage_matches_idt_gates`（`trap_stub.rs:770-786`）遍历 0..=255 断言 `gate_present == has_stub`。文档说“执行交叉核对的是宿主测试、`load()` 本身不检查”符合源码。
  4. `test-protection` 的 x86_64 载体（`os/qemu-tests/test-kernels/kernel/bootstrap/test-protection/src/main.rs:74-110`）只调 `CurrentProtection::init`/`load`、`CurrentTrapEntry::init`、`configure_syscall`、`load`，不调 `install_trap_stubs`；判定只读 GDTR/IDTR/TR/CS/DS 与四个 MSR。所以“门里保持占位 0 也能通过”与载体行为一致。
  5. C 侧算术：`protect.c:107-125` 的 `gate_table_pic` 有 16 条数据行；`:127-152` 的 `gate_table_exceptions` 有 23 条，其中 `COPROC_SEG_VECTOR`（9）在列、向量 15 不在列；`USER_PRIVILEGE` 出现在 4 条软中断与 breakpoint/overflow 共 6 行。`39 = 16 + 23` 成立。Rust 去掉 9、加上 0xF0/0xF1/0xFF 后 `41 = 39 − 1 + 3` 成立。C 的 APIC 路径另加 `gate_table_ioapic` 的 64 条（`apic.c:764-807`，`LAPIC_VECTOR` = 0x50+irq）、spurious 0xFF/error 0xFE、SMP 两条与 BSP timer 0xF0，总数 90/92 与文档一致。
  6. 没有找到相反路径：`STUB_VECTORS` 与 `init()` 的门集合由同一张表派生两次，测试兜底；`install_trap_stubs` 的唯一调用点在 `load` 之前；`set_gate_ist` 的两个向量与 `setup_tss_for_cpu` 填的 `ist[0]`/`ist[1]` 对应（`protection.rs:269-270`）。

- **结果**：**反证失败**。主张成立。分级：无（非阻断，不需要修）。补一条非阻断观察：`init()` 里向量 0/1/3/4 与 32-35 用 `is_trap=true`（陷阱门），但 `install_idt_handlers` 的 `set_handler` 会统一改写成中断门，最终加载的表确实是“全中断门”；文档只写了 PIC 行的 `false`，没有点明这一层覆盖关系，读者可能误以为 init() 里的 `is_trap` 会留到最终表。这属于表述完整性，不影响 §3.7 的结论。

- **证据**：`os/arch/src/x86_64/trap_stub.rs:168-215,613-637,770-786`；`os/arch/src/x86_64/trap_entry.rs:198-261,333-341,302-327`；`os/kernel/src/lib.rs:2274-2282`；`os/qemu-tests/test-kernels/kernel/bootstrap/test-protection/src/main.rs:74-110`；`minix3/minix/kernel/arch/i386/protect.c:107-152`；`minix3/minix/kernel/arch/i386/apic.c:764-807,855-915`；`minix3/minix/kernel/arch/i386/apic.h:90-94`。

- **若我错了**：如果 `STUB_VECTORS` 的某个 `stub_va` 落到未定义符号（那会在链接期失败，不会静默），或者 `install_trap_stubs` 还有第二个调用点在 `load` 之后（全树只有一个调用点），文档的“装齐再 load”就会被破坏。这两条都核实过，没有反例。因此保留“反证失败”的结论。

---

## 跨节复述冲突

1. **§2.1 与 §2.6 对同一构建机制给出互斥读法（阻断，P0）**。§2.6 第 661 行把 `pre_init.o` 的改名当作 `vir2phys` 两份实现的解释基础；§2.1 第 140 行却把 `pre_init.c` 里的 `kinfo`/`kmessages` 当成 `main.c` 同名标识符的同一对象。同一份 `Makefile.inc` 不可能同时支持这两种读法。CLAIM 01 的反证就是这条内部矛盾的直接后果。

2. **§2.6 与 §3.5 把“单 pagedir、原地重填”复述两次（阻断，P0）**。§2.6 第 668 行写“`pagedir` 是同一个静态数组的同一块物理内存”“清零与重填之间没有写 CR3……靠 TLB 里的旧条目继续走”；§3.5 第 892 行把它提升为对照前提：“C 的‘原地重填’成立靠两点……”。两节互相引用，错误被放大到两处；修复必须同时改，且不能只删一句，要换成“两张静态表 + 最后 `pg_load` 切 CR3”的模型。§2.6 的“为什么重建”两条理由与 §3.5 的淘汰决策本身不需要改。

3. **§4.1 与 §4.7 把“`kernel_may_alloc` 保留”复述两次（阻断，P0）**。§4.1 第 964 行说 Phase A 执行 `KERNEL_MAY_ALLOC.store(true, ...)` 且 getter 无生产调用者；§4.7 第 1187 行说“保留（改型为原子量）”。源码第 3916-3919 行的 Step 8 注释说“没有对应物”。两处都要改成“淘汰”，并删掉指向不存在符号的锚点。

4. **§4.4 与最终门状态（非阻断，表述完整性）**。`TrapEntryArch::init` 里向量 0/1/3/4 与 32-35 用 `is_trap=true`，但 `install_idt_handlers` 的 `set_handler` 统一改成中断门，最终装载的表全是中断门。§4.4 只解释了 PIC 行的 `false`，没有点明“init 的陷阱门标记会在装 stub 时被覆盖”。读者若把 `init()` 当最终表，会错误理解 `is_trap` 的生命周期。建议加一句，不改变结论。

5. **§5.2 与 aarch64 测试载体自身的注释冲突（非阻断，证据卫生）**。`test-protection-aarch64/src/main.rs:9-11` 的文件头写“UEFI 固件置 HCR_EL2.TSP=1，会陷获所有 SP_EL1 访问”，同文件 `:368` 与 `:437` 又写“TSP=0 confirmed / SP_EL1 is accessible”。文档 §4.6 第 1171 行采用 TSP=0，和载体后两处一致；冲突在载体内部，不在文档。建议文档引用“confirmed”那两处，或同时指出文件头的旧注释已过时。

6. **§1.2/§4.6 与 §1.4 的范围表述重叠（非阻断，边界说明）**。§1.4 第 82 行把“riscv64 的两条入口路径与 `sscratch` 交换”整体划给 `14-exception-interrupt.md`，但 §1.2 第 52 行与 §4.6 第 1167 行都实际描述了入口腿与 `sscratch` 交换。§4.6 结尾已经声明“只到门与入口已经就位”，所以这是“提一句落点”与“完整往返归他篇”的分工，不是事实冲突；建议在 §1.4 的括注里区分“入口腿的落点描述”与“完整往返”。

---

## 失败路径（资源、错误码、回滚、并发）

- **资源**：本篇涉及的路径没有任何动态分配。C 侧是 `gdt[7]`、`idt[256]`、`tss[CONFIG_MAX_CPUS]`（`protect.c:24-27`）与静态栈数组；Rust 侧是 `[u64; 22]`、`[IdtEntry64; 256]`、`[Tss64; 8]`、`IST_STACKS[2][8]`（每栈 8 KiB，`protection.rs:326-337`）、`GS_AREAS[8]`。容量在编译期定死，没有 `ENOMEM` 分支；失败形态是 #GP/#DF/三重故障，不是 errno。文档没有声称这里有可恢复的资源错误，主张与源码一致。一个可选的观察：`setup_tss_for_cpu` 只填 `ist[0]`/`ist[1]`（`protection.rs:269-270`），`ist[2..7]` 保持 0；当前没有门引用 IST3-7，所以不是缺陷，但若将来新增 IST 门，需要同步填表，否则硬件会回落到当前栈。

- **错误码**：C 的 `tss_init` 返回 `SEG_SELECTOR(index)`（`protect.c:215`），五个调用点全部丢弃（`earm/arch_system.c:113`、`i386/protect.c:338`、`i386/include/arch_smp.h:18` 宏内、`i386/arch_smp.c:272`、`i386/arch_system.c:256`）；`intr_init` 返回 `OK`。Rust 的 `ProtectionArch::init`/`TrapEntryArch::init` 返回 `Self`，没有错误通道；未初始化时的失败点是 `with_protection`/`with_trap_entry` 的 `expect` panic（`lib.rs:2315-2343`）。文档 §4.7 说“未初始化成为一个确定的失败点”，与源码一致。这里没有“错误码缺失”的问题，因为接口本来就不返回 errno；真正需要留意的是 panic 在引导期等于停机，这与 C 的 `panic()` 语义同构。

- **回滚**：没有回滚。`lgdt`/`lidt`/`lldt`/`ltr` 一旦执行，CPU 状态就改变；如果后续某步失败（例如 `ltr` 因描述符类型错而 #GP），而 IDT 尚未装载有效门，异常递送会升级为 #DF 再升级为三重故障。文档 §2.8 的“只有‘生效’类不可逆”与 §2.5/§3.7 的两个反事实（先 `ltr` 后 `lgdt`、先 `load` 后装门）方向正确。补充一个失败面：`prot_load_selectors` 内部如果 `lgdt` 成功而 `idt_reload` 之前就发生异常，CPU 会用旧 IDTR（引导方的表）递送；这正是 §2.2 把 `prot_init` 放在 `intr_init` 之前、并把“装门”放在“动控制器”之前的原因，文档的次序论证覆盖了这一点。

- **并发**：C 的 AP 路径共用 `prot_load_selectors`，`booting_cpu` 是单一全局（`protect.c:296`），BSP 在 `arch_smp.c:140` 为每颗 AP 写它，并等到 `ap_cpu_ready == cpu` 才进入下一轮 SIPI（`:134-160`）。这条串行化是 `booting_cpu` 安全的前提；文档 §2.5 只说“它由 `booting_cpu` 决定”，没有写出这个前提。当前代码满足前提，所以主张不假；但如果将来把 SIPI 循环并行化，`booting_cpu` 会变成数据竞争。Rust 侧的对策是把 sp0 写进每 CPU 的 TSS 槽、AP 自己写 `sscratch`/`TPIDR_EL1`/GS 区与四个 MSR（`smp.rs:1617-1628`、`protection.rs:424-474`），并用 `init_ap` 的 `sp0` 与 CPU-id 戳记断言兜底；文档 §5.4 的 M5/R03-T03 已把“AP 的 MSR 重写无自动断言”列为缺口，和源码一致。

- **额外观察（非阻断，不属于上述六条主张）**：C 的 AP 蹦床复制有一处源码可疑点——`minix3/minix/kernel/arch/i386/arch_smp.c:89` 写 `memcpy(&__ap_idt_tab, gdt, sizeof(idt))`，源是 `gdt` 而不是 `idt`，且 `sizeof(idt)`（2048）大于 `gdt`（56），会越界读 `gdt` 之后的对象。文档 §2.8 只引用同一函数里的 `assert(prot_init_done)` 作为次序见证，没有提到这一行。它不影响 §2.8 的 `prot_init_done` 论证（`assert` 在 `memcpy` 之前），但属于“AP 路径的失败面”文档没有覆盖的部分；建议另开条目核对，不要混进本篇的修复范围。

---

## 分级汇总

| 编号 | 主张 | 反证结果 | 分级 | 修复动作 |
|---|---|---|---|---|
| CLAIM 01 | §2.1 两行 `memcpy` 是自拷贝、删掉无后果 | 反证成功 | 阻断 P0-fact | 改写为“未分页副本 → `.usermapped` 副本的必需搬运”，重做该反事实 |
| CLAIM 02 | §2.6/§3.5 单 pagedir、恒等 `vir2phys`、原地重填 | 反证成功 | 阻断 P0-fact | 改写为“两份 `pg_utils`、两张表、最后 `pg_load` 切 CR3”；保留 §3.5 决策 |
| CLAIM 03 | §4.1/§4.7 `KERNEL_MAY_ALLOC` 保留、Phase A 置 true | 反证成功 | 阻断 P0-fact | 改为“淘汰”，删掉不存在的静态量与 getter 锚点 |
| CLAIM 04 | §2.4 `iobase` 为空 ⇒ 用户态 `in`/`out` 全部被拒 | 反证成功 | 阻断 P1-fact | 补 IOPL 层：默认拒绝，`enable_iop`/`SYS_IOPENABLE` 授权后放行 |
| CLAIM 05 | §2.2/§2.8 `cstart` 次序与 `prot_init_done` 论证 | 反证失败 | 非阻断（主张成立） | 无 |
| CLAIM 06 | §3.7/§4.4 41 门、装齐再 `load`、IST 恢复 | 反证失败 | 非阻断（主张成立） | 仅补一句 `init()` 陷阱门被 `set_handler` 覆盖的说明 |

**总体判断**：本篇的骨架（三张表、装载次序、Rust 两个 trait 的拆分、41 门的覆盖不变量）经得起反证；问题集中在两类：一是对 Minix3 的 `objcopy` 双份对象模型只应用了一半（CLAIM 01、02），二是 Rust 侧若干处复述停在旧代码状态（CLAIM 03，以及 §3.5 引用的 `Maps 1024 × 4MB...` Step 1 注释在当前 `build_bootstrap_root_and_enable` 里已不存在，实际注释是“minimal boot identity windows”那一段）。这两类都属于 P0-fact，且都能在不动设计决策的前提下修好。

