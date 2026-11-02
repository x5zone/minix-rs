# 03 · kmain 与 cstart：三张表与内核第一次行使特权

> 路径与标注约定（`[ARCH: …]` 等记号的解码处）全局只说一次：见 [README.md](../README.md) 的「全局阅读约定」。

> **目标读者**：熟悉操作系统基本概念（特权级、分页、中断与异常、描述符表大意）与 C 语言的开发者；Rust 一侧读到 trait、`unsafe` 契约与内联汇编大意即可。
> **前置知识**：分页已开启、内核正在高半地址上执行、`kmain` 刚被进入（`02-higher-half-kernel.md` §4.4 的状态表是起点）；x86 的段选择子、门、TSS 三个词听过即可。**前置**：[01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md)、[02-higher-half-kernel.md](02-higher-half-kernel.md)（本系列前两篇；跨篇分工见 §1.4）。
> **教学目标**：①三张表各回答什么问题，x86 与 aarch64/riscv64 的承载物对照（§1.1、§1.2）；②`kmain` 与 `cstart` 的调用序列与不可交换的次序（§2.1、§2.2）；③GDT/IDT/TSS 的装填与 `prot_load_selectors` 的指令序列（§2.3–§2.5）；④重建页表的理由，以及 64 位下为什么不再重建（§2.6、§3.5）；⑤64 位下三张表各自还剩多少语义、Rust 如何承载（§3.1–§3.7、§4.2–§4.6）；⑥真机 `test-protection` 三架构的判据与诊断章的使用（§5.2、§6）。
> **后续**：`04-platform-discovery.md`（平台参数发现）；`05-clock-interrupt-init.md`（时钟与中断控制器初始化——`cstart` 的后半段）；更远的分工表见 §1.4 与 §7。

上一篇结束时，CPU 已经在自己的高半地址上、在自己的栈上执行 `kmain`。这一篇从 `kmain` 的第一条指令开始，讲它做的第一件实质工作：把"当前特权级、异常入口、陷入内核用哪根栈"这三份答案从引导方手里接过来。三份答案在 x86 上落在三张表里，接管动作是一串只有 ring0 能执行的指令。接完之后，内核才第一次拥有"出事能被自己接住"的能力。

---

## 1. 概述

### 1.1 02 篇交出的状态：三份答案仍由引导方提供

跳转完成时，CPU 的 GDTR/IDTR 仍指向引导方留下的表；`02-higher-half-kernel.md` §4.4 用一张状态表把这一刻记成“保护结构未建立”，接管它是 `kmain` 的 **Phase B 第一件事**（依据是该篇引用的 `lib.rs` 的 [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection) 文档注释）。这一行更精确的读法是：危险不在此刻，而在这些表的归属。

内核在 `kmain` 之前一直借住在别人的规则里。x86 的 UEFI 引导路径上，boot-shim 运行在固件的 GDT/IDT 下，`ExitBootServices` 之后这些表所在的内存不再属于任何保留区间；GRUB/Multiboot 路径上，引导方的描述符表同样是"启动期用一下"的对象。内核此刻能跑，是因为它还没触发任何需要查表的动作——一旦异常到达而门里指向的是固件代码，或者 ring3 进入 ring0 时 CPU 从引导方的 TSS 读栈指针，那一刻的落点没有任何保证。

x86 上 CPU 在"出事"时需要预先知道的三件事，以及它们的存放处：

```text
CPU 在异常/特权级切换时需要预先知道的三件事，以及它们的存放处
  ① 现在是什么特权级、目标段能不能访问 ── GDT 的段描述符（DPL 位 + 类型 + 基址/限长）
  ② 出事跳到哪个地址 ─────────────────── IDT 的门（偏移 + 选择子 + DPL）
  ③ 从 ring3 进来时用哪根栈 ──────────── TSS 的 sp0（长模式另有给 NMI/#DF 的 IST 数组）
          ▲                                  ▲                       ▲
        lgdt                               lidt                     ltr
     （装载 GDT）                      （装载 IDT）              （装载 TSS 描述符）
```

三张表的内容是纯数据，写好之前不改变 CPU 行为；真正的换手发生在三条装载指令执行的那一刻。C 侧分析从 §2.1 的入口开始，按表与职责分组展开。

引导方留下的表不能继续用的另一条理由在 C 代码里能看到：`prot_init` 不打算读它们、也不打算复用它们里的任何一条，而是从 `memset` 开始重建（`i386/protect.c` 的 [`prot_init`](minix3/minix/kernel/arch/i386/protect.c:prot_init)）。这不是"旧表一定坏"，而是新表的所有权与生命周期要由内核自己保证。

### 1.2 三张表与三个问题

三份答案落在三张表上，每张表的装填方式由它回答的问题决定：

| CPU 需要知道的事 | C i386 的承载物 | Rust x86-64 的承载物 | aarch64 的承载物 | riscv64 的承载物 |
|---|---|---|---|---|
| 当前特权级与段可访问性 | GDT 段描述符（DPL）；段基址/限长 | GDT 仍保留 DPL，长模式段基址恒 0；访问控制主要靠页表 | 由 CPU 的异常级别决定（EL0/EL1），页表项权限位承载访问控制 | 由 CPU 的模式决定（U/S），页表项权限位承载访问控制 |
| 异常/中断/syscall 的入口 | IDT 的 8 字节门（每向量一条） | IDT 的 16 字节门（每向量一条，含 IST 字段） | `VBAR_EL1` 指向的定长向量表 | `stvec` 指向的向量（Direct 模式；实现里在两条入口路径之间切换，见 §4.6） |
| 陷入内核用哪根栈 | TSS 的 `sp0`（32 位 TSS） | TSS 的 `sp0`（64 位 TSS，另加 IST 数组） | EL1 的当前栈指针（EL1h）；栈基址记在 `TPIDR_EL1` | `sscratch` 交换（硬件不换栈，由入口第一条指令交换） |

C 的 earm-32 与 Rust 的 aarch64 是同一问题的两代载体：§2.7 讲 earm 的 `write_vbar`，§4.6 讲 aarch64 的 `VBAR_EL1`/`TPIDR_EL1`；两者回答同一问题，不要求寄存器名相同。

三张表的一个差别是：②是逐条目不同的（x86 上 256 个门，每类事件一条），①和③各只有一套（DPL 存在少数几个描述符里、`sp0` 每 CPU 一个字段）。后文 §2.4、§2.5 的装填方式不同正是这个差别带来的——IDT 要按向量逐条建，GDT 与 TSS 只需建少数几条描述符与一个结构体。

③ 还带一个"内核态里又出事用哪根栈"的子问题，三架构的答法同样不同：x86 交给 IST（NMI 与 #DF 各有一条专用栈，64 位新增见 §3.1、门里的 IST 见 §4.4）；aarch64 不换栈，继续用 `SP_EL1`；riscv64 也不换栈，用当前 `sp`。陷阱入口如何判断"这次进来的是用户还是内核"（riscv64 用两条入口路径按 `stvec` 切换，进自用户态的那条无条件交换 `sscratch`）属异常/中断与 trap 入口的主题（`14-exception-interrupt.md`，异常与中断篇）。

x86 上还有一张表与本节的三张并列出现：LDT。`GDT` 里为它留了一个槽，装载路径上也有一条 `lldt`（`i386/protect.c` 的 [`prot_load_selectors`](minix3/minix/kernel/arch/i386/protect.c:prot_load_selectors)）。它不回答上面三问中的任何一问，本篇 §2.3 按源码如实处理。

本篇的"保护结构"一词指上面三张表与它们回答的三个问题。它不含页表：页表已经由 01/02 篇建好并在运行，本篇的 `prot_init` 里确实还有一段重建页表的代码（§2.6），RFC 式的说法是"保护结构初始化顺手把引导页表换成正式版"，C 的写法把两件事放在同一个函数里，Rust 把它们分开了（§3.5）。

### 1.3 "第一次行使特权"指什么

`kmain` 之前，内核已经执行过特权指令：`pre_init` 里写 CR3、`vm_enable_paging` 里写 CR0/CR4（`i386/pg_utils.c` 的 [`vm_enable_paging`](minix3/minix/kernel/arch/i386/pg_utils.c:vm_enable_paging)），这些同样是 ring0 专属。本篇说的"第一次"不在指令的特权级，而在控制权：内存里的表从"引导方的"换成"内核自己维护的"，并且由内核决定什么时候把它交给 CPU。

| 指令 | 特权要求 | 本条之前，该寄存器/表是谁的值 | 本条之后是谁的值 |
|---|---|---|---|
| `lgdt` | ring0 | 引导方 GDT 的基址与限长 | [`prot_init`](minix3/minix/kernel/arch/i386/protect.c:prot_init) 里 `gdt_desc` 指向的 `gdt[]` |
| `lidt` | ring0 | 引导方 IDT | 同文件 `idt_desc` 指向的 `idt[]` |
| `lldt` | ring0 | 引导方 LDTR（或复位值） | `LDT_SELECTOR`（[`include/archconst.h`](minix3/minix/kernel/arch/i386/include/archconst.h) 里的槽号 5） |
| `ltr` | ring0 | 引导方 TR（或复位值） | `TSS_SELECTOR(0)`，即上面那张 TSS 描述符 |
| 重载 CS/DS/ES/FS/GS/SS | 非特权，但只有新表里存在对应描述符时才成功 | 引导方选择子（Multiboot 留下的那组） | 内核 GDT 里的内核段选择子 |
| `wrmsr`（`SYSENTER_CS`/`_ESP`/`_EIP`、`STAR`、`EFER`） | ring0 | 引导方或复位值 | 系统调用入口地址与段选择子（见 §2.4） |

表里只有最后一列由内核写入，倒数第二列才是"接手"的证据。`lgdt` 等四条指令在这里是使内存里的数据成为 CPU 行为依据的动作。

### 1.4 本篇范围与可以安全跳过的内容

| 可以跳过的内容 | 归属 |
|---|---|
| 引导器契约、启动信息的收集与内存记账、页表的建立与开启 | `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` |
| 高半地址、按装载地址摆放、成对跳转 | `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` |
| 平台的参数发现（ACPI/设备树分支、平台描述符） | `04-platform-discovery.md`（本篇不引节号） |
| 时钟与中断控制器的初始化、中断使能时序 | `05-clock-interrupt-init.md`（本篇不引节号） |
| 页表的多级结构、页表接口与 Direct Map 的完整语义 | `rewrite-notes/02-stage-vm/07-pagetable-struct.md`；01 篇 §2.6 已用到的部分不重复 |
| 异常处理体、trap frame 布局、用户态返回路径与"用户→内核→用户"的完整往返（含 riscv64 的两条入口路径与 `sscratch` 交换） | 异常/中断与 trap 入口主题（`14-exception-interrupt.md`，异常与中断篇）与启动收尾/返回主题（`10-switch-to-user.md`，切用户篇）；本篇只到"门里有真入口"为止 |
| 系统调用的分派与各调用语义 | syscall 分派主题（`13-syscall-dispatch.md`，系统调用分派篇），本篇只到"入口地址与段选择子写进 MSR"为止 |
| 进程表、boot 进程装载、特权结构 | 进程初始化与启动收尾主题（`06-proc-init-boot-proc.md`，进程表与 boot 进程篇；`kmain` 的后续 Phase） |
| SDM/APM 的描述符逐位格式全表、ARM 与 RISC-V 的特权规范 | 外部资料；本篇只解释代码实际使用的那几位 |

可以安全跳过的一条：描述符的位级格式（`segdesc_s` 的 `base_low`/`base_middle`/`base_high` 与 granularity 字节）不必背下来。本篇只解释三处用到它的代码（S 位的含义、DPL 占哪两位、限长的"0 表示 4G"约定），其余位在代码里各有宏名，读到再看。

本篇沿用 §1.2 的定义：保护结构 = 三张表与三问，不含页表；页表由 01/02 篇建好并在运行，§3.5 只解释为什么 Rust 不再重建它。

---

## 2. C 源码分析

### 2.1 kmain 入口的五步

`kmain` 在调用 `cstart` 之前只做准备工作（`kernel/main.c` 的 [`kmain`](minix3/minix/kernel/main.c:kmain)）：

```c
/* 代码引用，minix3/minix/kernel/main.c:kmain（函数头到 cstart() 调用）。 */
void kmain(kinfo_t *local_cbi)
{
/* Start the ball rolling. */
  struct boot_image *ip;	/* boot image pointer */
  register struct proc *rp;	/* process pointer */
  register int i, j;
  static int bss_test;

  /* bss sanity check */
  assert(bss_test == 0);
  bss_test = 1;

  /* save a global copy of the boot parameters */
  memcpy(&kinfo, local_cbi, sizeof(kinfo));
  memcpy(&kmess, kinfo.kmess, sizeof(kmess));

   /* We have done this exercise in pre_init so we expect this code
      to simply work! */
   machine.board_id = get_board_id_by_name(env_get(BOARDVARNAME));
#ifdef __arm__
  /* We want to initialize serial before we do any output */
  arch_ser_init();
#endif
  /* We can talk now */
  DEBUGBASIC(("MINIX booting\n"));

  /* Kernel may use bits of main memory before VM is started */
  kernel_may_alloc = 1;

  assert(sizeof(kinfo.boot_procs) == sizeof(image));
  memcpy(kinfo.boot_procs, image, sizeof(kinfo.boot_procs));

  cstart();
```

五步依次是：BSS 自检、把引导信息拷进全局、识别板子、放开"内核可直接分配物理内存"的闸门、把 boot image 表拷进启动信息。逐条看它们改了什么、谁读。

BSS 自检用的是函数内的静态变量 `bss_test`。静态存储期的变量放在 BSS 段，链接脚本把 BSS 声明为一段需要清零的内存，但清零动作由装载方执行（装载器行为：实现分析在 `02-higher-half-kernel.md` §4.2，测试登记在 `01-boot-shim-bootstrap.md` §5.2.1 的 `test_bss_region_is_zeroed_after_load`）。`assert(bss_test == 0)` 在赋值之前读它一次：如果装载方没清，这里读到的是垃圾值，断言当场炸掉，比让它带着垃圾值跑进后续初始化要好。断言通过之后 `bss_test = 1`，这一句的作用是给下一次启动留证据（同一段内存被第二次使用时，值不再是 0）。

第二个动作的两行 `memcpy` 源与目的是同一个对象：`kinfo` 是 [`i386/pre_init.c`](minix3/minix/kernel/arch/i386/pre_init.c) 里的文件级全局变量（`kinfo_t kinfo;`），`pre_init` 的收尾是 `return &kinfo;`，`head.S` 把这个指针压栈后传给 `kmain`，所以 `local_cbi == &kinfo`，第一行是一次全局自拷贝。第二行同理：`glo.h` 的 `#define kmess kmessages` 与 `pre_init` 里的 `cbi->kmess = &kmess;` 说明 `kinfo.kmess` 就是全局缓冲区 `kmessages` 自己的地址。两行代码体现的事实是"启动信息就落在内核自己的静态存储里"——引导侧（`pre_init` 本身就是内核代码）已经把 `kinfo` 与 `kmessages` 放在 `.bss` 上，`kmain` 不是把信息从别处搬进来，而是把这份约定再确认一遍。Rust 侧的情况不同——启动信息的交接形态与随之而来的故障记录见 §4.1。

```c
/* 教学简化（只保留与自拷贝论证有关的四行；函数体省略），三处证据的原文。 */
return &kinfo;                 // pre_init.c 收尾
push %eax                      // head.S：把 pre_init 的返回值压给 kmain
#define kmess kmessages        // glo.h
cbi->kmess = &kmess;           // pre_init.c
```

第三步识别板子：`machine.board_id = get_board_id_by_name(env_get(BOARDVARNAME))`。它读的是启动参数里的环境变量，注释写着"We have done this exercise in pre_init so we expect this code to simply work!"——同一件事在 `pre_init` 里已经做过一次，这里只是把结果存进 `machine`。紧随其后的 `#ifdef __arm__` 串口初始化说明为什么 ARM 侧必须早于任何输出：串口没配好之前，调试输出无处可去。

第四步 `kernel_may_alloc = 1` 需要对着定义点读。这个变量在 C 里是 `glo.h` 的 `EXTERN int`（声明），两处**定义**（带初值的那个副本）在 i386 与 earm 各自的 `pre_init.c` 里：

```
$ grep -rn "int kernel_may_alloc" minix3/minix/kernel/
minix3/minix/kernel/glo.h:76:EXTERN int kernel_may_alloc;
minix3/minix/kernel/arch/earm/pre_init.c:38:int kernel_may_alloc = 1;
minix3/minix/kernel/arch/i386/pre_init.c:33:int kernel_may_alloc = 1;
```

也就是说，进入 `kmain` 时它的值已经是 1；`kmain` 这一行是把同一句话再说一遍。真正会翻转它的地方在 `kernel/main.c` 的 [`bsp_finish_booting`](minix3/minix/kernel/main.c:bsp_finish_booting)（置 0，注释"Kernel may no longer use bits of memory as VM will be running soon"）。这个标志的读者是物理内存分配函数：

```c
/* 节选（保留函数签名、`lowest` 初值与两处断言；`EMPTY` 在 `alloc_lowest` 函数体内 `#define EMPTY 0xffffffff`；此处以下为区间对齐与 memmap 遍历），minix3/minix/kernel/arch/i386/pg_utils.c:alloc_lowest。 */
phys_bytes alloc_lowest(kinfo_t *cbi, phys_bytes len)
{
	phys_bytes lowest = EMPTY;
	assert(len > 0);
	len = roundup(len, I386_PAGE_SIZE);

	assert(kernel_may_alloc);
```

`assert(kernel_may_alloc)` 是这道闸门的执行点：`kernel_may_alloc` 为 0 时，任何一次内核直接分配都会触发断言。启动期内核确实需要直接分配——把 VM 的 ELF 段装进页表用的就是这条路（`i386/protect.c` 的 [`libexec_pg_alloc`](minix3/minix/kernel/arch/i386/protect.c:libexec_pg_alloc) 调 `pg_map(PG_ALLOCATEME, ...)`），所以闸门必须在 `proc_init` 之前是开的；而 VM 跑起来之后，内核不再需要自己分配物理页，闸门才关。

第五步 `assert(sizeof(kinfo.boot_procs) == sizeof(image)); memcpy(kinfo.boot_procs, image, ...)` 把链接期就存在的 boot image 表 `image[]` 拷进启动信息。`image[]` 是 [`kernel/table.c`](minix3/minix/kernel/table.c) 里的纯静态表（进程号与名字）；模块的 `start_addr`/`len` 稍后由 `kmain` 在 `cstart` 之后的启动循环按 `kinfo.module_list` 回填（`ip = &image[i]`，[`kernel/main.c`](minix3/minix/kernel/main.c) 的 `ip->start_addr = mb_mod->mod_start;`），循环读写的是 `image[]` 本身。循环结束后，同文件的第二次 `memcpy(kinfo.boot_procs, image, ...)` 把填好 `start_addr`/`len` 的 `image[]` 同步给 `kinfo.boot_procs`；这份副本的读者是用户态 VM（`servers/vm/main.c` 的 `kernel_boot_info.boot_procs`），不是 `proc_init`。

**把第二步的两行自拷贝 `memcpy` 删掉会怎样**：源与目的同一个对象，删掉前后 `kinfo`/`kmessages` 的内容没有区别，这一刀在 C 侧切不出后果；反事实要放到 Rust 侧才成立——Rust 的启动信息由 boot-shim 用引用交接，`kmain` 若不把它落进全局副本、继续用入口参数引用，就会重现 §4.1 记录的故障：参数引用被编译器 spill 到内核栈后读回，经参数读到的字段变成 `.rdata` 的 ASCII 垃圾，`validate` 在第一处不变量上 panic。依据：`i386/pre_init.c` 的 [`pre_init`](minix3/minix/kernel/arch/i386/pre_init.c:pre_init) 的定义与返回、[`lib.rs`](os/kernel/src/lib.rs) 里 `kmain` 的入口注释与 `:fn kernel_info`。

### 2.2 cstart 的调用序列与顺序不变量

`kmain` 的最后一个动作是 `cstart()`。它把保护结构、时钟软件状态、中断控制器、架构初始化串成一条固定序列，中间穿插启动环境变量的读取（`kernel/main.c` 的 [`cstart`](minix3/minix/kernel/main.c:cstart)）：

```c
/* 代码引用，minix3/minix/kernel/main.c:cstart（整函数）。 */
void cstart(void)
{
/* Perform system initializations prior to calling main(). Most settings are
 * determined with help of the environment strings passed by MINIX' loader.
 */
  register char *value;				/* value in key=value pair */

  /* low-level initialization */
  prot_init();

  /* determine verbosity */
  if ((value = env_get(VERBOSEBOOTVARNAME)))
	  verboseboot = atoi(value);

  /* Initialize clock variables. */
  init_clock();

  /* Get memory parameters. */
  value = env_get("ac_layout");
  if(value && atoi(value)) {
        kinfo.user_sp = (vir_bytes) USR_STACKTOP_COMPACT;
        kinfo.user_end = (vir_bytes) USR_DATATOP_COMPACT;
  }

  DEBUGEXTRA(("cstart\n"));

  /* Record miscellaneous information for user-space servers. */
  kinfo.nr_procs = NR_PROCS;
  kinfo.nr_tasks = NR_TASKS;
  strlcpy(kinfo.release, OS_RELEASE, sizeof(kinfo.release));
  strlcpy(kinfo.version, OS_VERSION, sizeof(kinfo.version));

  /* Initialize various user-mapped structures. */
  memset(&arm_frclock, 0, sizeof(arm_frclock));

  memset(&kuserinfo, 0, sizeof(kuserinfo));
  kuserinfo.kui_size = sizeof(kuserinfo);
  kuserinfo.kui_user_sp = kinfo.user_sp;

#ifdef USE_APIC
  value = env_get("no_apic");
  if(value)
	config_no_apic = atoi(value);
  else
	config_no_apic = 1;
  value = env_get("apic_timer_x");
  if(value)
	config_apic_timer_x = atoi(value);
  else
	config_apic_timer_x = 1;
#endif

#ifdef USE_WATCHDOG
  value = env_get("watchdog");
  if (value)
	  watchdog_enabled = atoi(value);
#endif

#ifdef CONFIG_SMP
  if (config_no_apic)
	  config_no_smp = 1;
  value = env_get("no_smp");
  if(value)
	config_no_smp = atoi(value);
  else
	config_no_smp = 0;
#endif
  DEBUGEXTRA(("intr_init(0)\n"));

  intr_init(0);

  arch_init();
}
```

从这一步开始，本篇只展开 `prot_init()`（§2.3–§2.6），另外三次调用各归他篇。但它们的**顺序**属于本篇，因为它由 `prot_init` 的产物决定：

| 调用 | 依赖前一步的哪样产物 | 把它提前/推后，最先坏的对象 |
|---|---|---|
| `prot_init()` | 无（函数自己完成全部前提） | 它是后面所有步骤的前提，无法提前 |
| `init_clock()` | 无硬依赖；它只初始化 `kclockinfo`/`kloadinfo` 并读 `hz` 环境变量（[`init_clock`](minix3/minix/kernel/clock.c:init_clock)） | 与 `prot_init` 可交换；它的硬件编程不在这里（见下） |
| `intr_init(0)` | 不依赖 IDT；它只编程 8259 并屏蔽源。保留在 `prot_init` 之后是防御性次序：先把异常入口装好，再动控制器 | 提前到 `prot_init` 之前不会新造中断窗口——可屏蔽中断被 IF=0 挡住；真正的风险是任何 `prot_init` 之前的异常/NMI 都无门可走，与 `intr_init` 无关 |
| `arch_init()` | 需要保护结构、控制器都已就绪 | 提前：它里面还会再动一次 TSS 与栈（见 §2.4 末尾） |

`arch_init` 的函数体（`i386/arch_system.c`）只做 TSS 二次调用、串口/ACPI/APIC 初始化；它不初始化控制器本身，所以“控制器已就绪”的前提来自 `cstart` 的调用次序。

`init_clock` 本身只做三件事——`memset(&kclockinfo, ...)`、读 `hz` 环境变量并夹在 `[2, 50000]` 区间内（越界用 `DEFAULT_HZ`）、`memset(&kloadinfo, ...)`。也就是说，C 的 `init_clock` 不编程硬件定时器；硬件定时器的编程在后续篇讲的那条"配置→注册→使能"路径上（[`kernel/clock.c`](minix3/minix/kernel/clock.c) 的 `boot_cpu_init_timer` 位置，属时钟与中断的主题）。

`intr_init(0)` 在 i8259 这份实现里只做一件事：按 ICW 序列编程两片 8259，最后把中断线屏蔽（`i386/i8259.c` 的 [`intr_init`](minix3/minix/kernel/arch/i386/i8259.c:intr_init)，收尾一行 `outb(INT2_CTLMASK, ~0)` 关掉 slave 的 8 条线；master 的掩码在序列中段写作 `~(1 << CASCADE_IRQ)`，只放开 cascade 线，其余屏蔽）。它不填 IDT——门是在 `prot_init` 里填的。把这条与"控制器已初始化"分开，是本篇与时钟/中断主题的分界：本篇结束时中断**有门可走**，但**没有源被放行**。

**把 `intr_init(0)` 提到 `prot_init()` 之前会怎样**：可屏蔽中断不会来（`head.S` 的 `push $0`+`popf` 把标志位清零，IF=0 保持到内核第一次 `intr_enable()`），所以 i8259 的屏蔽字改写本身不制造可见中断。会变的只是“控制器已编程、但异常入口仍是引导方的”这一状态；而任何 `prot_init` 之前的异常或 NMI 本来就没有内核门可走——这个窗口从 `kmain` 进入到 `prot_init` 一直存在，不是 `intr_init` 提前才有的。保留 `prot_init` 在前，是为了先把异常入口装好，再动控制器。依据：`i386/i8259.c` 的 [`intr_init`](minix3/minix/kernel/arch/i386/i8259.c:intr_init) 只 `outb` 编程两片 8259、不填 IDT；IF 状态见 [`i386/head.S`](minix3/minix/kernel/arch/i386/head.S) 的 `popf` 与 [`i386/arch_clock.c`](minix3/minix/kernel/arch/i386/arch_clock.c) 的 `intr_enable()` 调用点。

### 2.3 prot_init：五类动作与两张表的容量

`prot_init` 的函数体不长，动作可以分成五类：探测 CPU 能力、清零并设置表指针、建 TSS/填 GDT、一次装载让表生效、重建页表与置标志（第五类在 §2.6 单列）。`prot_init` 全文（`i386/protect.c` 的 [`prot_init`](minix3/minix/kernel/arch/i386/protect.c:prot_init)）：

```c
/* 代码引用，minix3/minix/kernel/arch/i386/protect.c:prot_init（整函数）。 */
void prot_init(void)
{
  extern char k_boot_stktop;

  if(_cpufeature(_CPUF_I386_SYSENTER))
	minix_feature_flags |= MKF_I386_INTEL_SYSENTER;
  if(_cpufeature(_CPUF_I386_SYSCALL))
	minix_feature_flags |= MKF_I386_AMD_SYSCALL;

  memset(gdt, 0, sizeof(gdt));
  memset(idt, 0, sizeof(idt));

  /* Build GDT, IDT, IDT descriptors. */
  gdt_desc.base = (u32_t) gdt;
  gdt_desc.limit = sizeof(gdt)-1;
  idt_desc.base = (u32_t) idt;
  idt_desc.limit = sizeof(idt)-1;
  tss_init(0, &k_boot_stktop);

  /* Build GDT */
  init_param_dataseg(&gdt[LDT_INDEX],
    (phys_bytes) 0, 0, INTR_PRIVILEGE); /* unusable LDT */
  gdt[LDT_INDEX].access = PRESENT | LDT;
  init_codeseg(KERN_CS_INDEX, INTR_PRIVILEGE);
  init_dataseg(KERN_DS_INDEX, INTR_PRIVILEGE);
  init_codeseg(USER_CS_INDEX, USER_PRIVILEGE);
  init_dataseg(USER_DS_INDEX, USER_PRIVILEGE);

  /* Currently the multiboot segments are loaded; which is fine, but
   * let's replace them with the ones from our own GDT so we test
   * right away whether they work as expected.
   */
  prot_load_selectors();

  /* Set up a new post-relocate bootstrap pagetable so that
   * we can map in VM, and we no longer rely on pre-relocated
   * data.
   */

  pg_clear();
  pg_identity(&kinfo); /* Still need 1:1 for lapic and video mem and such. */
  pg_mapkernel();
  pg_load();

  prot_init_done = 1;
}
```

第一段是能力探测，它不是"检查 CPU 型号"式的诊断，而是把结果写进一个全局能力字（`minix_feature_flags`），供后文 `tss_init` 决定装哪套系统调用入口。两个条件各对应一种硬件路径：Intel 的 `SYSENTER` 与 AMD 的 `SYSCALL`（`SYSCALL` 由 AMD 引入后被 Intel 采纳）。这两条 `if` 的意义在 §2.4 会具体起来。

第二段清零两张表，并写好描述符表指针（GDTR/IDTR 的镜像，真正的装载在后面；这一段里还有一行 `tss_init(0, &k_boot_stktop)`，TSS 描述符的装填在 §2.4）。两处 `limit = sizeof(table)-1` 是 x86 的约定：限长是"最后一个字节的下标"，不是字节数；写错成 `sizeof` 会让 CPU 允许访问表外一个字节。表的容量由头文件定义：

```
$ grep -n "define IDT_SIZE\|define DESC_SIZE\|define GDT_SIZE\|define TSS_INDEX" minix3/minix/kernel/arch/i386/include/archconst.h
11:#define IDT_SIZE 256	/* the table is set to it's maximal size */
19:#define TSS_INDEX_FIRST      6
20:#define TSS_INDEX(cpu)       (TSS_INDEX_FIRST + (cpu)) /* per cpu kernel tss */
21:#define GDT_SIZE             (TSS_INDEX(CONFIG_MAX_CPUS))	/* LDT descriptor */
31:#define DESC_SIZE	8
```

`GDT_SIZE` 的表达式把 TSS 的槽号算进了表的长度：GDT 的长度 = 固定槽数 6 + 每 CPU 一个 TSS 槽 × CPU 数。`CONFIG_MAX_CPUS` 在这份源码里是 1（[`kernel/kernel.h`](minix3/minix/kernel/kernel.h)），所以 `GDT_SIZE = TSS_INDEX(1) = 6 + 1 = 7`（复算：`printf '%d\n' $((6+1))` 输出 7）；IDT 则是固定 256 项、每项 `DESC_SIZE` = 8 字节。把每个槽的名字对上号，得到本篇第一张查表表：

| GDT 下标 | 内容 | 谁读它 |
|---|---|---|
| 0 | 空描述符（`memset` 保证全 0） | CPU 对选择子 0 的约定 |
| 1 | 内核代码段（`init_codeseg`，DPL=0） | IDT 每个门的 `selector`；`x86_load_kerncs` 重载 CS |
| 2 | 内核数据段（`init_dataseg`，DPL=0） | `prot_load_selectors` 重载 DS/ES/FS/GS/SS；TSS 里的 `ds`/`es`/`fs`/`gs`/`ss0` |
| 3 | 用户代码段（DPL=3） | 用户进程的 CS（返回用户态时） |
| 4 | 用户数据段（DPL=3） | 用户进程的 DS/SS |
| 5 | LDT 占位（见下） | `lldt` 用 |
| 6 | TSS 描述符（`tss_init(0, ...)` 建） | `ltr` 用；CPU 在 ring3→ring0 时按它找 TSS |

第 5 槽单独说明。源码把它写成两步：

```c
/* 代码引用，minix3/minix/kernel/arch/i386/protect.c:prot_init（LDT 两行）。 */
  init_param_dataseg(&gdt[LDT_INDEX],
    (phys_bytes) 0, 0, INTR_PRIVILEGE); /* unusable LDT */
  gdt[LDT_INDEX].access = PRESENT | LDT;
```

第一行用一个 size 参数为 0 的"数据段"构造打底，第二行把访问字节整个换成"存在 + LDT 类型"。构造函数对 size 的处理是 `--size` 之后再看要不要走页粒度（`i386/protect.c` 的 [`init_param_dataseg`](minix3/minix/kernel/arch/i386/protect.c:init_param_dataseg) 调用的辅助函数，其中一行注释是 `--size; /* convert to a limit, 0 size means 4G */`），所以这条描述符的限长并不是 0——传进去的 0 被解释成"最大区段"。真正让它"不可用"的是类型：LDT 描述符是系统描述符，任何按代码/数据段去加载它的选择子都会被拒绝。源码自己对它的两个称呼是硬证据：这一行的注释叫 `unusable LDT`，装载点那一行的注释叫 `Load bogus ldt`。它为什么必须存在：`lldt` 要求选择子指向一个 LDT 类型的描述符，槽里没有描述符就没法执行这条装载。除此之外没有源码证据表明还有别的用途，本段到此为止，不再替它编动机。

第三段填 GDT 的四个段描述符。两个内核段 DPL=0、两个用户段 DPL=3（`INTR_PRIVILEGE` 与 `USER_PRIVILEGE`，[`include/archconst.h`](minix3/minix/kernel/arch/i386/include/archconst.h) 里分别是 0 与 3；访问字节的位序在这里用到两处——S 位是第 4 位，区分系统描述符与代码/数据段描述符；DPL 占第 5–6 位，这两个值就填在那一对位上）；段基址全 0、限长取满——32 位下段式隔离已经不用，段描述符在这里的作用是承载 DPL 与"这是代码段还是数据段"。这四个描述符的装填函数 `init_codeseg`/`init_dataseg` 以及它们的公共底座 `sdesc` 是 `prot_init` 的直接被调者，锚点见 `i386/protect.c` 的 [`init_codeseg`](minix3/minix/kernel/arch/i386/protect.c:init_codeseg)、`:init_dataseg`、`:sdesc`。段式隔离废弃之后这张表仍然必须在场，理由不在段而在 TSS：`ltr` 装的是选择子，CPU 与后续转场都要经 GDT 里的描述符才能找到 TSS 结构体（§2.4），所以段的语义可以被削到只剩 DPL，GDT 本身不能退役。

第四段只有一行 `prot_load_selectors()`，§2.5 整节讲它。

**把两张表的清零 `memset`（`gdt`/`idt` 各一行）删掉会怎样**：`gdt`/`idt` 是 BSS 里的静态数组，冷启动的初值本来就是 0，第一次启动看不出差别；两行 `memset` 是这两张表唯一的清零点——`idt_init` 只写两张门表列出的 39 个门，GDT 的装填也只碰固定槽与 TSS 槽，其余表项要靠清零回到已知状态。同一份二进制里 `GDT_SIZE`/`IDT_SIZE` 是编译期常量，两次执行的槽数不变；会变的是表项内容指向的地址。在"这块内存此前已被用过"的前提下（重启路径：`prepare_shutdown`/`minix_shutdown` 之后由监控程序重新进入内核；该路径上的内存复用形态是推断，C 树只到这两个入口为止；若走的是重新装载镜像，`.bss` 会被装载器再清零、此反事实即不成立），未被这次写入覆盖的表项保留此前的字节，其中的入口地址/基址此时可能已指向别的用途。另注意 §2.1 的 `bss_test` 断言："整体未清零"的第二次进入会先在它那里停下——两行 `memset` 挡的是检测被绕过（如手工清零 canary）或局部复用的情形。依据：`i386/protect.c` 的 [`idt_copy_vectors`](minix3/minix/kernel/arch/i386/protect.c:idt_copy_vectors) 的循环只遍历给定表、`:prot_init` 的两行 `memset` 是唯一的清零点、[`include/archconst.h`](minix3/minix/kernel/arch/i386/include/archconst.h) 的 `GDT_SIZE`/`IDT_SIZE` 定义。

### 2.4 tss_init：一条描述符、一个结构体、两条系统调用路径

`tss_init` 被 `prot_init` 调用，它同时承担三件事：往 GDT 里放 TSS 描述符、填 TSS 结构体、按能力位装系统调用入口。`tss_init` 全文（`i386/protect.c` 的 [`tss_init`](minix3/minix/kernel/arch/i386/protect.c:tss_init)）：

```c
/* 代码引用，minix3/minix/kernel/arch/i386/protect.c:tss_init（整函数）。 */
int tss_init(unsigned cpu, void * kernel_stack)
{
	struct tss_s * t = &tss[cpu];
	int index = TSS_INDEX(cpu);
	struct segdesc_s *tssgdt;

	tssgdt = &gdt[index];
  
	init_param_dataseg(tssgdt, (phys_bytes) t,
			sizeof(struct tss_s), INTR_PRIVILEGE);
	tssgdt->access = PRESENT | (INTR_PRIVILEGE << DPL_SHIFT) | TSS_TYPE;

	/* Build TSS. */
	memset(t, 0, sizeof(*t));
	t->ds = t->es = t->fs = t->gs = t->ss0 = KERN_DS_SELECTOR;
	t->cs = KERN_CS_SELECTOR;
	t->iobase = sizeof(struct tss_s);	/* empty i/o permissions map */

	/* 
	 * make space for process pointer and cpu id and point to the first
	 * usable word
	 */
	k_percpu_stacks[cpu] = t->sp0 = ((unsigned) kernel_stack) - X86_STACK_TOP_RESERVED;
	/* 
	 * set the cpu id at the top of the stack so we know on which cpu is
	 * this stack in use when we trap to kernel
	 */
	*((reg_t *)(t->sp0 + 1 * sizeof(reg_t))) = cpu;

	/* Set up Intel SYSENTER support if available. */
	if(minix_feature_flags & MKF_I386_INTEL_SYSENTER) {
	  ia32_msr_write(INTEL_MSR_SYSENTER_CS, 0, KERN_CS_SELECTOR);
  	  ia32_msr_write(INTEL_MSR_SYSENTER_ESP, 0, t->sp0);
  	  ia32_msr_write(INTEL_MSR_SYSENTER_EIP, 0, (u32_t) ipc_entry_sysenter);
  	}

	/* Set up AMD SYSCALL support if available. */
	if(minix_feature_flags & MKF_I386_AMD_SYSCALL) {
		u32_t msr_lo, msr_hi;

		/* set SYSCALL ENABLE bit in EFER MSR */
		ia32_msr_read(AMD_MSR_EFER, &msr_hi, &msr_lo);
		msr_lo |= AMD_EFER_SCE;
		ia32_msr_write(AMD_MSR_EFER, msr_hi, msr_lo);

		/* set STAR register value */
#define set_star_cpu(forcpu) if(cpu == forcpu) {				\
		ia32_msr_write(AMD_MSR_STAR,					\
		  ((u32_t)USER_CS_SELECTOR << 16) | (u32_t)KERN_CS_SELECTOR,	\
		  (u32_t) ipc_entry_syscall_cpu ## forcpu); }
		set_star_cpu(0);
		set_star_cpu(1);
		set_star_cpu(2);
		set_star_cpu(3);
		set_star_cpu(4);
		set_star_cpu(5);
		set_star_cpu(6);
		set_star_cpu(7);
		assert(CONFIG_MAX_CPUS <= 8);
  	}

	return SEG_SELECTOR(index);
}
```

第一件事：把 TSS **描述符**放进 GDT 的第 `TSS_INDEX(cpu)` 槽。构造分两步——`init_param_dataseg` 先按普通数据段把基址/限长/粒度填好（基址就是 TSS 结构体 `t` 的地址），随后 `access` 被整个换成 `PRESENT | (INTR_PRIVILEGE << DPL_SHIFT) | TSS_TYPE`。`TSS_TYPE` 在文件顶部展开为 `(AVL_286_TSS | DESC_386_BIT)`，即"可用的 286 TSS"类型位加上 386 标志位；这一组合的最终含义是"32 位可用 TSS"，CPU 在 `ltr` 与转场时按这个类型去解 TSS。

第二件事：填 TSS 结构体本身。逐字段看，只有几个字段有值：

| 字段 | 值 | 谁读它 |
|---|---|---|
| `sp0` | `kernel_stack - X86_STACK_TOP_RESERVED` | CPU 在 ring3→ring0 转场时加载到 RSP |
| `ss0` | `KERN_DS_SELECTOR` | CPU 在同一转场时加载到 SS |
| `cs` | `KERN_CS_SELECTOR` | 转场后 CS 的期望值（由门/选择子决定，字段本身是备份；Minix3 不用硬件任务切换，没有读者） |
| `ds`/`es`/`fs`/`gs` | `KERN_DS_SELECTOR` | 同上（数据段备份；没有读者） |
| `iobase` | `sizeof(struct tss_s)` | 高于 TSS 限长（由 `init_param_dataseg` 按 size 折出的限长是 `sizeof - 1`，即恰好 +1 字节）即"没有 I/O 位图"；用户态 `in`/`out` 因此全部被拒 |
| `ist` | 本结构体没有该字段 | 32 位 TSS 没有 IST 数组；64 位才有（§3.1） |
| 其余（`backlink`/`sp1`/`sp2`/`cr3`/通用寄存器区/`ldt`） | `memset` 清零 | 32 位硬件任务切换才用；Minix3 不用硬件任务切换 |

`X86_STACK_TOP_RESERVED` 是 `2 * sizeof(reg_t)`（[`include/archconst.h`](minix3/minix/kernel/arch/i386/include/archconst.h)），32 位下是 8 字节、两个 32 位字。这两个字不是给 CPU 的，是内核自己借用的：

```c
/* 代码引用，minix3/minix/kernel/arch/i386/arch_system.c:arch_finish_switch_to_user（节选，保留取栈与进程指针写入，含 SMP 分支）。 */
#ifdef CONFIG_SMP
	stk = (char *)tss[cpuid].sp0;
#else
	stk = (char *)tss[0].sp0;
#endif
	/* set pointer to the process to run on the stack */
	p = get_cpulocal_var(proc_ptr);
	*((reg_t *)stk) = (reg_t) p;
```

这一段是"每次切回用户态之前，把当前进程指针写到 `sp0` 指向的字"。第二个字由 `tss_init` 自己写：`*((reg_t *)(t->sp0 + sizeof(reg_t))) = cpu`，注释说明用途——"so we know on which cpu is this stack in use when we trap to kernel"。这个字的读者是 `cpuid` 宏：它把当前帧地址向上取整到 `K_STACK_SIZE` 边界、再读边界下的一个字（[`include/arch_smp.h`](minix3/minix/kernel/arch/i386/include/arch_smp.h) 的 `#define cpuid`），对跑在 `k_stacks` 数组上的 CPU，那个字正是 `sp0 + sizeof(reg_t)`；[`i386/arch_smp.c`](minix3/minix/kernel/arch/i386/arch_smp.c) 的注释写着它的适用前提（BSP 早期 still run on the boot stack and we cannot use cpuid）。SYSCALL 入口读的是同一个栈顶区的第一个字——`sp0` 处的进程指针，不是这个 CPU 号：

```asm
/* 代码引用，minix3/minix/kernel/arch/i386/mpx.S:ipc_entry_syscall_percpu（宏展开后的入口头部）。 */
ENTRY(ipc_entry_syscall_cpu ## cpu)			;\
	xchg	%ecx, %edx				;\
	mov	k_percpu_stacks+4*cpu, %esi		;\
	mov	(%esi), %ebp 				;\
	movl	$KTS_SYSCALL, P_KERN_TRAP_STYLE(%ebp)	;\
	xchg	%esp, %esi				;\
	jmp	syscall_sysenter_common
```

`k_percpu_stacks[cpu]` 在 `tss_init` 里被赋成与 `sp0` 相同的值，入口用"CPU 号 → 表项 → 栈顶第一个字 → 进程指针"两步拿到当前进程。第三件事：系统调用入口的两种硬件路径。

`minix_feature_flags` 的两个位在 `prot_init` 开头按 `_cpufeature` 置好，这里按位选择。Intel SYSENTER 路径写三个 MSR：`SYSENTER_CS`（内核代码段选择子）、`SYSENTER_ESP`（**栈指针，取自刚算好的 `t->sp0`**）、`SYSENTER_EIP`（入口 `ipc_entry_sysenter`）。AMD SYSCALL 路径写 `EFER.SCE`（使能 SYSCALL/SYSRET）与 `STAR`。STAR 的位段按 legacy SYSCALL 的约定切分：`STAR[63:48]` 是返回时的用户 CS、`STAR[47:32]` 是按下的内核 CS、`STAR[31:0]` 是 SYSCALL 的目标 EIP——`set_star_cpu` 把入口地址写在第三个字段里。这一点可以从 `ia32_msr_write(reg, hi, lo)` 的参数去向核对：`hi` 进 `EDX`、`lo` 进 `EAX`，`wrmsr` 写的 64 位值是 `EDX:EAX`，所以传参的**上半**是选择子对、**下半**才是入口地址（[`i386/klib.S`](minix3/minix/kernel/arch/i386/klib.S) 的 `ia32_msr_write`）。两条路径的入口地址属于两类汇编标签：SYSENTER 侧一个 `ipc_entry_sysenter`，SYSCALL 侧按 CPU 分成八个（`ipc_entry_syscall_cpu0`…`_cpu7`）。宏 `set_star_cpu(forcpu)` 展开成"只在 `cpu == forcpu` 时写"，八次展开覆盖八颗 CPU，随后一行 `assert(CONFIG_MAX_CPUS <= 8)` 钉住这个上限。这套写法的含义是：宏参数里带 CPU 号是为了给每颗 CPU 写它自己的入口地址，因为 SYSCALL 不在门里、CPU 也不通过 TSS 找栈，入口必须自己找到"我是哪颗 CPU 的内核栈"。

**把 `sp0` 直接写成 `kernel_stack`、不减两个保留字会怎样**：错位的第一步在 `tss_init` 自己——CPU 号写在 `sp0 + sizeof(reg_t)`，不减时这个写点落到栈顶**之上**，越出这段栈、写进相邻对象（`mpx.S` 里 `k_boot_stack` 的栈顶之后是对齐到 `K_STACK_SIZE` 的 `k_stacks_start` 与每 CPU 两页的内核栈数组）；随后 `cpuid` 宏读 `K_STACK_SIZE` 边界下的一格时，那格已不是 CPU 号（真正的写点被推到了栈外），读回的是别的字节。硬件转场的压栈本身仍落在栈内——错位的是"栈顶两格归内核专用"这条约定，以及依赖它的读取。依据：`:tss_init` 的减法与写点、[`i386/mpx.S`](minix3/minix/kernel/arch/i386/mpx.S) 的 `.bss` 排布、`:arch_finish_switch_to_user` 写 `sp0` 处的约定、[`include/arch_smp.h`](minix3/minix/kernel/arch/i386/include/arch_smp.h) 的 `cpuid` 宏。

`tss_init` 的调用跨度可以用末尾的 grep 核：i386 在 `prot_init` 里用 `&k_boot_stktop` 调它，而 `arch_init` 里还有第二次调用（非 SMP 构建，参数是 `get_k_stack_top(0)`），SMP 构建则在 `smp_init` 的路径上对所有 CPU 各调一次（`i386/arch_system.c` 的 [`arch_init`](minix3/minix/kernel/arch/i386/arch_system.c:arch_init)、`i386/arch_smp.c` 的 [`tss_init_all`](minix3/minix/kernel/arch/i386/arch_smp.c:tss_init_all)）。两次调用的差别在参数：第一次指向 `.bss` 里那块固定的 `k_boot_stack`，第二次指向 `k_stacks` 数组里按 CPU 编号切分的两页式栈（[`include/arch_proto.h`](minix3/minix/kernel/arch/i386/include/arch_proto.h) 的 `get_k_stack_top` 宏）。`tss_init` 的返回值（`SEG_SELECTOR(index)`）在 C 树的五个调用点全部被丢弃（复算：`grep -rn "tss_init(" minix3/minix/kernel/ | grep -v "int tss_init"` 命中五处——`arch/earm/arch_system.c:113`、`arch/i386/protect.c:338`、`arch/i386/include/arch_smp.h:18`（宏内）、`arch/i386/arch_smp.c:272`、`arch/i386/arch_system.c:256`）；TR 装的选择子在 §2.5 由 `TSS_SELECTOR(booting_cpu)` 现算，不经过这个返回值。这条跨度说明"TSS 初始化"不是一个一次性动作：`prot_init` 先给它一个能用的临时栈，运行期栈的登记在后续步骤完成。本篇只把这条线索指出来，`arch_init` 与 `smp_init` 的内部不在这里展开。

### 2.5 prot_load_selectors：让表生效的一串指令

表建好了，还都在内存里。让它们生效的是一串装载指令，收在一个函数里（`i386/protect.c` 的 [`prot_load_selectors`](minix3/minix/kernel/arch/i386/protect.c:prot_load_selectors)）：

```c
/* 代码引用，minix3/minix/kernel/arch/i386/protect.c:prot_load_selectors（整函数）。 */
void prot_load_selectors(void)
{
  /* this function is called by both prot_init by the BSP and
   * the early AP booting code in mpx.S by secondary CPU's.
   * everything is set up the same except for the TSS that is per-CPU.
   */
  x86_lgdt(&gdt_desc);	/* Load gdt */ 
  idt_init();
  idt_reload();
  x86_lldt(LDT_SELECTOR); 	/* Load bogus ldt */
  x86_ltr(TSS_SELECTOR(booting_cpu));

  x86_load_kerncs();
  x86_load_ds(KERN_DS_SELECTOR);
  x86_load_es(KERN_DS_SELECTOR);
  x86_load_fs(KERN_DS_SELECTOR);
  x86_load_gs(KERN_DS_SELECTOR);
  x86_load_ss(KERN_DS_SELECTOR);
}
```

这个函数的源码注释给出两条事实。第一条是复用关系："called by both prot_init by the BSP and the early AP booting code in mpx.S by secondary CPU's"。AP 侧的调用点在 [`i386/mpx.S`](minix3/minix/kernel/arch/i386/mpx.S)（该文件里由 `ENTRY(startup_ap_32)` 宏展开的那个入口），它在保护模式里先用 `_C_LABEL(k_boot_stktop) - 4` 当临时栈，再调这个函数，之后才跳进 `smp_ap_boot`。第二条是差异范围："everything is set up the same except for the TSS that is per-CPU"——同一串指令两边都跑，差别只在 `ltr` 用哪个选择子（`booting_cpu` 这个静态变量决定——它定义在 [`i386/protect.c`](minix3/minix/kernel/arch/i386/protect.c)，AP 启动路径在 [`i386/arch_smp.c`](minix3/minix/kernel/arch/i386/arch_smp.c) 里把它置成本 CPU 号）。这两条合起来说明这个函数不能按"BSP 专用初始化"读，它是一段被两条路径共用的"把描述符表寄存器换成本内核的表"的动作。

动作的次序有依赖，不是任意排列：

1. `x86_lgdt(&gdt_desc)`——GDTR 指向新表。此后 CS 仍是旧选择子，CPU 还按旧表的解释执行，直到 CS 被重载。
2. `idt_init()`——**填**IDT。它会遍历两张门表逐条写门。
3. `idt_reload()`——`lidt`，把 IDTR 指向新表。
4. `x86_lldt(LDT_SELECTOR)`——LDTR 装第 5 槽那条 LDT 占位。
5. `x86_ltr(TSS_SELECTOR(booting_cpu))`——TR 装本 CPU 的 TSS 描述符。注意它写在 `lgdt` 之后：TR 的选择子要被新 GDT 解释，反过来先装 TR 的话，CPU 会拿旧表去查同一个槽号，查到的未必是 TSS 描述符。
6. 重载 CS 与五个数据段寄存器——把引导方留下的选择子换成新表里的内核段选择子。源码在 `prot_init` 里给这一步写了理由：`Currently the multiboot segments are loaded; which is fine, but let's replace them with the ones from our own GDT so we test right away whether they work as expected.`（`i386/protect.c` 的 [`prot_init`](minix3/minix/kernel/arch/i386/protect.c:prot_init)）——旧选择子能用，但换成自己的之后就顺带验证了新表。

`idt_init()` 的实现分两层。外层按固定顺序填两张表（`i386/protect.c` 的 [`idt_init`](minix3/minix/kernel/arch/i386/protect.c:idt_init)）：

```c
/* 代码引用，minix3/minix/kernel/arch/i386/protect.c:idt_init。 */
void idt_init(void)
{
	idt_copy_vectors_pic();
	idt_copy_vectors(gate_table_exceptions);
}
```

内层是一个通用循环与一个写门的函数（`i386/protect.c` 的 [`idt_copy_vectors`](minix3/minix/kernel/arch/i386/protect.c:idt_copy_vectors)、`:int_gate`）：

```c
/* 代码引用，minix3/minix/kernel/arch/i386/protect.c:idt_copy_vectors。 */
void idt_copy_vectors(struct gate_table_s * first)
{
	struct gate_table_s *gtp;
	for (gtp = first; gtp->gate; gtp++) {
		int_gate(idt, gtp->vec_nr, (vir_bytes) gtp->gate,
				PRESENT | INT_GATE_TYPE |
				(gtp->privilege << DPL_SHIFT));
	}
}

/* 代码引用，minix3/minix/kernel/arch/i386/protect.c:int_gate。 */
void int_gate(struct gatedesc_s *tab,
	unsigned vec_nr, vir_bytes offset, unsigned dpl_type)
{
/* Build descriptor for an interrupt gate. */
  register struct gatedesc_s *idp;

  idp = &tab[vec_nr];
  idp->offset_low = offset;
  idp->selector = KERN_CS_SELECTOR;
  idp->p_dpl_type = dpl_type;
  idp->offset_high = offset >> OFFSET_HIGH_SHIFT;
}
```

三处结构在源码里有定义，组成本篇的第二张查表表：

| 结构 | 字段 | 说明 |
|---|---|---|
| `struct gate_table_s`（[`include/arch_proto.h`](minix3/minix/kernel/arch/i386/include/arch_proto.h)） | `void (*gate)(void)`、`unsigned char vec_nr`、`unsigned char privilege` | 门的"元数据行"。这个 32 位版本**没有 IST 字段**——IST 是长模式的扩展，64 位重写时才引入（§3.1） |
| `struct gatedesc_s`（[`include/archtypes.h`](minix3/minix/include/arch/i386/include/archtypes.h)） | `offset_low`、`selector`、`pad`、`p_dpl_type`、`offset_high` | IDT 表项的物理布局；`int_gate` 写其中四个字段——`offset_low` 与 `offset_high`（偏移两半）、`selector`、`p_dpl_type`（存在位 + DPL + 门类型），`pad` 不动 |
| `gate_table_exceptions[]` 的终止条件 | 最后一行 `{ NULL, 0, 0 }` | `idt_copy_vectors` 的循环条件 `gtp->gate` 靠它停住；数组必须留这个零填充行 |

门的分布用现算的读数核对：

```
$ awk '/gate_table_pic\[\]/,/};/' minix3/minix/kernel/arch/i386/protect.c | grep -c 'hwint'
16
$ awk '/gate_table_exceptions\[\]/,/};/' minix3/minix/kernel/arch/i386/protect.c | grep -cE '_VECTOR'
23
```

两张表合计 39 条门——这两张表就是 `idt_init` 的全部填充源（`idt_copy_vectors_pic` 转发 `gate_table_pic`，`idt_copy_vectors` 收 `gate_table_exceptions`），39 即 IDT 上被置为 present 的全部门（其余表项保持非存在）。`gate_table_pic[]` 的 16 条由 `VECTOR(irq)` 宏算出向量号（[`include/interrupt.h`](minix3/minix/include/arch/i386/include/interrupt.h)：`((irq < 8 ? IRQ0_VECTOR : IRQ8_VECTOR) + (irq & 0x07))`，`IRQ0_VECTOR`=0x50、`IRQ8_VECTOR`=0x70），全部 DPL=0。`gate_table_exceptions[]` 的 23 条里：19 条 CPU 异常（向量 0–14 与 16–19，跳过保留的 15），4 条软中断（32/33/34/35）。其中 DPL=3 的只有四条软中断与两条异常——`breakpoint_exception`（向量 3）与 `overflow`（向量 4）；其余异常 DPL=0。这张表是"用户态能不能用 `int $n` 主动触发某个向量"的依据，读者可以对着 `gate_table_exceptions[]` 的每一行核一遍 `privilege` 列（复算：`awk '/gate_table_exceptions\[\]/,/};/' minix3/minix/kernel/arch/i386/protect.c | grep -c 'USER_PRIVILEGE'` 输出 6——四条软中断与 breakpoint/overflow 两条异常）。

**把 `x86_ltr(TSS_SELECTOR(booting_cpu))` 提到 `x86_lgdt(&gdt_desc)` 之前会怎样**：TR 会被装上"用旧 GDT 解释的槽号 6"——引导方表里槽 6 是另一条描述符（或非存在）。`ltr` 本身就要求选择子指向一个 TSS 类型的系统描述符，这一条很可能当场 #GP；即使侥幸通过，随后的第一次 ring3→ring0 转场会按 TR 指向的描述符去取 TSS 基址，读到的是旧表里那个描述符的基址/限长，`sp0` 从那里读出来。后果落在"CPU 转场后压栈的目标地址"上。依据：`prot_load_selectors` 的调用次序、`:tss_init` 只往 `gdt[index]` 写而不写 TR、`prot_init` 的注释说明旧选择子仍在用。

### 2.6 重建页表：四个调用、两条理由、两份 vir2phys

`prot_load_selectors` 之后还有四行，它们与前面四类动作不同类：清空页目录、重建两种映射、装载。这四行在 `pre_init` 里已经出现过一次——同样的 `pg_clear()`、`pg_identity()`、`pg_mapkernel()`、`pg_load()`，`pre_init` 的版本还把 `pg_mapkernel()` 的返回值存进了 `kinfo.freepde_start`（`i386/pre_init.c` 的 [`pre_init`](minix3/minix/kernel/arch/i386/pre_init.c:pre_init)），而 `prot_init` 的版本丢弃返回值。

先看这一段与它前后两句注释（`i386/protect.c` 的 [`prot_init`](minix3/minix/kernel/arch/i386/protect.c:prot_init)）：

```c
/* 代码引用，minix3/minix/kernel/arch/i386/protect.c:prot_init（重建页表段与相邻注释）。 */
  /* Set up a new post-relocate bootstrap pagetable so that
   * we can map in VM, and we no longer rely on pre-relocated
   * data.
   */

  pg_clear();
  pg_identity(&kinfo); /* Still need 1:1 for lapic and video mem and such. */
  pg_mapkernel();
  pg_load();

  prot_init_done = 1;
```

`pg_clear()` 清空的是同一个静态页目录数组（`i386/pg_utils.c` 的 [`pg_clear`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_clear)，`memset(pagedir, 0, sizeof(pagedir))`）。`pg_identity()` 把 1024 个页目录项全部填成"恒等映射"，每项 4MB：

```c
/* 节选（保留循环、权限位组合与 PWT/PCD 分支；本块为连续逐字段落，无省略），minix3/minix/kernel/arch/i386/pg_utils.c:pg_identity。 */
        for(i = 0; i < I386_VM_DIR_ENTRIES; i++) {
		u32_t flags = I386_VM_PRESENT | I386_VM_BIGPAGE
			| I386_VM_USER
			| I386_VM_WRITE;
                phys = i * I386_BIG_PAGE_SIZE;
		if((cbi->mem_high_phys & I386_VM_ADDR_MASK_4MB)
			<= (phys & I386_VM_ADDR_MASK_4MB)) {
			flags |= I386_VM_PWT | I386_VM_PCD;
		}
                pagedir[i] =  phys | flags;
        }
```

两处细节可以自己核：恒等映射的权限位里带了 `I386_VM_USER`（对用户态可访问）与 `I386_VM_WRITE`——带 USER 位的原因 C 树没有注释，此处不替它编动机；可核的效果是这些条目都是 BIGPAGE，VM 接手根表时 `pt_init` 的复制循环跳过 BIGPAGE 条目（`vm/pagetable.c` 的 [`pt_init`](minix3/minix/servers/vm/pagetable.c:pt_init)），它们不会进入用户进程的页表；从 `mem_high_phys` 所在的 4MB 对齐块起，后面的目录项被标记为 `PWT|PCD`（直写、禁缓存）：比较用的是 4MB 掩码，所以最后一个仍含真实内存的 4MB 块也会被一起标记；那些完全在内存顶端之上的块不是真内存、按普通内存访问会出事。`pg_mapkernel()` 把内核所在的虚拟区段映射到它的物理区段，每项带 `PRESENT|BIGPAGE|WRITE` 而不带 `USER`（`i386/pg_utils.c` 的 [`pg_mapkernel`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel)）。`pg_load()` 把页目录的物理地址写进 CR3：

```c
/* 代码引用，minix3/minix/kernel/arch/i386/pg_utils.c:pg_load。 */
phys_bytes pg_load(void)
{
	phys_bytes phpagedir = vir2phys(pagedir);
        write_cr3(phpagedir);
	return phpagedir;
}
```

`vir2phys` 在这里出现，而同一个名字在本篇涉及的两个文件里有两份不同实现：

```c
/* 代码引用，minix3/minix/kernel/arch/i386/pre_init.c（文件头部，函数体单行）。 */
phys_bytes vir2phys(void *addr) { return (phys_bytes) addr; } 
```

```c
/* 代码引用，minix3/minix/kernel/arch/i386/protect.c:vir2phys。 */
phys_bytes vir2phys(void *vir)
{
	extern char _kern_vir_base, _kern_phys_base;	/* in kernel.lds */
	u32_t offset = (vir_bytes) &_kern_vir_base -
		(vir_bytes) &_kern_phys_base;
	return (phys_bytes)vir - offset;
}
```

两份实现的差别是"要不要减平移量"，而这件事的前提是它们各自的运行时刻：`pre_init` 在重定位之前执行，那时内核符号还等于物理地址（02 篇 §2.2 的两段式布局），原样返回就是物理地址；`protect.c` 的版本在 `prot_init` 的高半执行期使用，传入的内核符号要减掉 `_kern_vir_base - _kern_phys_base` 才是物理地址。但这不是“任何内核符号都减”：`pg_utils.c` 里的 `pg_load` 也调 `vir2phys(pagedir)`，而 `pg_utils.o` 在改名名单里，它的调用解析到 `pre_init.c` 的恒等版；`pagedir` 本身落在 `.unpaged_bss`（链接在 `_kern_offset` 之前，VMA = 物理地址）。所以判据是“看这个符号/数据落在哪一段”，不是“函数在哪个时刻执行”。函数名相同，语义按符号所在段与调用点不同。C 侧对同一个问题只留了这一个函数名，读代码时按“谁在哪个前提下调它”判断。（两份不会在链接期相撞：`pre_init.o` 与 `pg_utils.o` 都在改名名单里（02 篇 §2.2），它们的调用解析到带前缀的 `__k_unpaged_vir2phys`；`protect.o` 未改名，用自己那份。）

现在可以回答"为什么重建"。源码给的理由在上面的注释里，一共两条，各自有明确的读者：

- "we can map in VM, and we no longer rely on pre-relocated data"——重建后的表是 VM 进程将要载入并运行的那张表（`i386/protect.c` 的 [`arch_boot_proc`](minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc) 把 VM 的 ELF 段直接映射进这张页表）。理由落在**映射拓扑**上：`pre_init` 那张表建立于重定位前的布局，VM 的段与正式 VA 布局都不在其中——注释里的 "pre-relocated data" 指的就是这批映射；重建后的表才是 VM 装载所需的那张。
- "Still need 1:1 for lapic and video mem and such"——恒等段此时仍未退位。LAPIC 的 MMIO、显存这类"地址就是物理地址"的对象要靠它访问，所以重建时必须立刻把它装回去。

**把 `pg_identity(&kinfo)` 从重建段去掉、或把重建整段删掉只留 `pg_load()` 会怎样**：前者的后果可以逐步推：`pg_clear()` 已经把 1024 个目录项清成 0，恒等窗口在表里消失；清零与重填之间没有写 CR3，这几步自身的取指与写表靠 TLB 里的旧条目继续走——这就是"原地重填"能成立的机制（§3.5 会用同一机制解释 Rust 侧为什么不能照搬）；`pg_load()` 写入 CR3 之后，执行转而依赖重填后的表内容（此时高半已回、恒等未回——本反例删掉了 `pg_identity`），第一个按物理地址访问的对象成为现场——启动期访问 LAPIC 的 MMIO 或写显存时，目标地址在页目录里没有对应项，缺页，而故障地址不是高半地址而是物理地址本身（如 `0xFEE00000` 一段）。后者（删掉清空与重建只留 `pg_load`）在物理地址层面反而看不出差别：`pagedir` 是同一个静态数组的同一块物理内存，`pg_load` 写进 CR3 的值不因此改变；这也解释了为什么这段代码删掉后**冷启动可能照常跑**，它挡的是"第二次进入这段代码"与"此后表内容的正确性"这一类问题——与 §2.3 的 `memset` 是同一族。这里的"第二次进入"指重启路径：上一轮内核运行结束之后由监控程序重新进入内核（`kernel/main.c` 的 [`prepare_shutdown`](minix3/minix/kernel/main.c:prepare_shutdown) 与同文件的 `minix_shutdown`，§2.3）。与第一次进入的差别在字节来源：第一次进入时 `.bss` 的初值由装载方的清零与段体摆放定义（§2.1），第二次进入时 `pagedir` 里是上一轮留下的表项，`pg_clear`/`pg_identity`/`pg_mapkernel` 把 1024 个表项逐个写成本轮需要的值（该前提与 §2.1 `bss_test` 断言的关系见 §2.3 的括注）。依据：`pg_clear` 的清零范围、`pg_identity` 的循环、`prot_init` 的两句注释。

### 2.7 earm 的 protect.c：另一份独立实现

`grep -c '#if' minix3/minix/kernel/arch/i386/protect.c` 输出 0。i386 与 earm 的 `protect.c` 是各自独立的文件，不是同一份代码的条件编译两半；读它们时不能按“另一个分支里应该有对应段落”去找。ARM32 一侧有自己的一份 `protect.c`，它的 `prot_init` 只有两个动作（`earm/protect.c` 的 [`prot_init`](minix3/minix/kernel/arch/earm/protect.c:prot_init)）：

```c
/* 代码引用，minix3/minix/kernel/arch/earm/protect.c:prot_init（整函数）。 */
void prot_init(void)
{
	/* tell the HW where we stored our vector table */
	write_vbar((reg_t)&exc_vector_table);

	/* Set up a new post-relocate bootstrap pagetable so that
	 * we can map in VM, and we no longer rely on pre-relocated
	 * data.
	 */

	pg_clear();
	pg_identity(&kinfo); /* Still need 1:1 for device memory . */
	pg_mapkernel();
	pg_load();

	prot_init_done = 1;
}
```

对照 i386 那份，差别不在"少了哪个条件编译分支"，而在两端的承载物不同：

| 对照项 | i386 | earm |
|---|---|---|
| 段描述符表 | GDT 七个槽（§2.3） | 没有 |
| TSS 描述符 | GDT 第 6 槽（§2.4） | 没有；TSS 结构体只存 `sp0` |
| `tss_init` 的调用者 | `prot_init`（第一个参数 `&k_boot_stktop`；`arch_init` 与 AP 路径的第二次调用见 §2.4） | [`arch_init`](minix3/minix/kernel/arch/earm/arch_system.c:arch_init)（参数 `get_k_stack_top(0)`） |
| 异常入口 | 填 IDT + `lidt`（在 `prot_load_selectors` 里） | `write_vbar(&exc_vector_table)`，一条写系统寄存器的动作 |
| 重建页表 | 有 | 有（恒等注释略不同："Still need 1:1 for device memory ."） |
| 置 `prot_init_done` | 有 | 有 |

两处可以直接核的读数：两份 `protect.c` 各自定义一次 `prot_init`（`grep -rn "^void prot_init" minix3/minix/kernel/arch/*/protect.c` 输出两行）；架构差异由**文件**承载，不由条件编译承载。earm 的 `tss_init` 也带着同样的两个约定：`sp0` 减 `ARM_STACK_TOP_RESERVED`、把 CPU 号写进 `sp0 + sizeof(reg_t)`（`earm/protect.c` 的 [`tss_init`](minix3/minix/kernel/arch/earm/protect.c:tss_init)）；`sp0` 的读者同 i386，是 [`arch_finish_switch_to_user`](minix3/minix/kernel/arch/earm/arch_system.c:arch_finish_switch_to_user) 里的 `tss[cpuid].sp0`。

C 世界里没有第三份：`ls minix3/minix/kernel/arch/` 只有 `i386` 与 `earm` 两个架构目录。RISC-V 没有 C 真值源，本篇不在 C 章为它虚构一段实现；它的落地在 §4.6 按 Rust 实现讲。这条边界对读者有一个实际后果：凡是要"回到 C 源码核一遍"的结论，RISC-V 侧都只能核到 Rust 代码与架构规范，不能核到 Minix3 源码。

**把 earm 的 `prot_init` 当成 i386 的另一个分支来读会怎样**：会得出"这里应该有一段与 `idt_init` 对应的填充代码"的预期，然后在 `prot_init` 里找一个并不存在的门表。按这个误读去改代码，最可能的动作是把门表的初始化从别处搬进 `prot_init`——而 ARM 侧的异常入口是 `VBAR_EL1`/`VBAR` 指向的定长向量表，填充它的动作与 `write_vbar` 不是同一件事。后果落在异常入口寄存器仍指向引导方的值，与 §2.5 的"IDT 未装载"是同一类故障在另一份载体上的表现。依据：上面两条现算读数与两份文件各自的 `prot_init` 定义。

### 2.8 本章归纳

C 侧的动作换一个轴——按"改内存还是改 CPU 状态"归成三类（§2.3 的五类按执行顺序切分）：

| 类 | 动作 | 出现在 | 出错的可见性 |
|---|---|---|---|
| 建表（改内存） | 能力探测写全局能力字、`memset` 清零、填段描述符、建 TSS 描述符与结构体、填门 | §2.3–§2.5、§2.6 的清零 | 写错还来得及改；但错误表内容会在装载后生效 |
| 生效（改 CPU 状态） | `lgdt`/`lidt`/`lldt`/`ltr`/重载段寄存器、写 MSR | §2.4、§2.5 | 执行完即改变 CPU 行为，不可回退 |
| 接管后的收尾（改内存与 CR3） | 重建页表、`prot_init_done = 1` | §2.6 | 表内容与"完成"标志的状态 |

三类里只有第二类不可逆；而"生效"不是函数的最后一步——`prot_load_selectors()` 之后还有页表重建与标志位。这个次序本身就是一条不变量：读 `prot_init_done` 的路径（AP 启动、后续初始化）必须在表已装载之后才看到它。`prot_init_done` 的读者在 `i386/arch_smp.c` 的 AP 启动路径里（`assert(prot_init_done)`）：

```c
/* 代码引用，minix3/minix/kernel/arch/i386/arch_smp.c（AP 路径对 prot_init_done 的读）。 */
assert(prot_init_done);
```

**把 `prot_init_done = 1` 提到 `prot_load_selectors()` 之前会怎样**：AP 启动路径（[`i386/mpx.S`](minix3/minix/kernel/arch/i386/mpx.S) 里的 AP 入口也要跑同一个 `prot_load_selectors`）与这个标志的读者之间不再有序。具体现场：`smp_ap_tail` 在 AP 上先 `init_ap`（`lgdt`/`ltr`）；若 BSP 在 `prot_load_selectors` 之前置位并放行 AP，AP 的 `ltr` 会按尚未装填的 TSS 槽取描述符，首次转场读 `sp0` 时落到未初始化字节。

---

## 3. Rust 设计决策

C 章的各节留下了在 Rust 里必须重新回答的问题，§3.1–§3.7 逐一回答：三张表在 64 位下各自还有多少语义（§3.1）、两个职责为什么拆成两个 trait（§3.2）、表装好之后由谁保证它还在原地址（§3.3）、系统调用入口在长模式里怎么选（§3.4）、页表重建为什么不做了（§3.5）、`cstart` 一条序列怎么变成启动阶段（§3.6）、门什么时候可以装载（§3.7）。

### 3.1 64 位下三张表的去留（→§2.3、§2.4、§2.6）

| 项 | 选择 | 否掉什么 | 代价 |
|---|---|---|---|
| GDT 与段描述符 | 保留，槽位按长模式重排 | 直接搬用 C 的槽号 | 用户 CS/DS 位置变化，STAR 的取值随之变（§4.3） |
| TSS | 保留，换成 64 位布局（16 字节描述符 + `ist[7]`） | 32 位 TSS 布局 | 描述符占两个 GDT 槽；IST 数组要有可用的栈地址（§4.3） |
| LDT | 淘汰 | `lldt` 与第 5 槽占位 | 长模式不使用 LDT；C 的 LDT 占位没有对应物 |
| IDT | 保留，门换成 16 字节并增加 IST 字段 | 8 字节门 | 两条走 IST 的向量（NMI/#DF，其余向量 IST=0）的栈选择成为显式配置 |
| 页表重建段 | 淘汰 | 在 `init_protection` 里重建 | 重建所服务的读者改由 01/02 篇的建表满足（§3.5） |

TSS 的字段级去留（对 `include/arch_proto.h` 的 [`tss_s`](minix3/minix/kernel/arch/i386/include/arch_proto.h:tss_s) 的 27 个字段逐个处置）：

| C 字段 | 处置 | Rust 对应 |
|---|---|---|
| `sp0` | 保留（64 位宽度） | [`struct Tss64`](os/arch/src/x86_64/protection.rs:struct Tss64) 的 `sp0` |
| `ss0` | 保留语义 | 长模式 TSS 没有 `ss0` 字段：门转场不读它（SS 由 64 位模式规则置为 NULL），SYSCALL 的内核 SS 由 STAR 推出（`STAR[47:32]+8`） |
| `cs`/`ss`/`ds`/`es`/`fs`/`gs` | 淘汰 | 长模式下转场不读这些备份字段 |
| `sp1`/`sp2` | 保留为 64 位格式槽（零值、无读者） | `Tss64` 的格式自带这两格；语义上不用 |
| `ss1`/`ss2`/`cr3`/`ip`/`flags`/`ax`…`di`/`ldt`/`trap` | 淘汰 | 32 位硬件任务切换遗留；Minix3 不用硬件任务切换 |
| `iobase` | 保留 | `iobase = 0x8000`（大于 TSS 限长即"无 I/O 位图"） |
| （无） | 新增 | `ist[7]`（NMI/#DF 的专用栈）、`_reserved*` 填充（对齐到 104 字节） |
| `backlink` | 淘汰 | 只有硬件任务切换与嵌套任务才用 |

GDT 的槽位对照（两张表的长度也一并现算）：

| 槽 | C（i386，`CONFIG_MAX_CPUS`=1） | Rust（x86-64，`MAX_CPUS`=8） |
|---|---|---|
| 0 | 空 | 空 |
| 1 | 内核 CS | 内核 CS |
| 2 | 内核 DS | 内核 DS |
| 3 | 用户 CS | SYSRET anchor（用户 CS 的副本，[`const GDT_USER_ANCHOR_INDEX`](os/arch/src/x86_64/protection.rs:const GDT_USER_ANCHOR_INDEX)） |
| 4 | 用户 DS | 用户 DS |
| 5 | LDT 占位 | 用户 CS |
| 6 起 | 每 CPU 一个 TSS 槽 | 每 CPU 两个槽（16 字节 TSS 描述符） |
| 表长 | `TSS_INDEX(CONFIG_MAX_CPUS)`（这里是 7） | `GDT_TSS_FIRST_INDEX + MAX_CPUS * GDT_SLOTS_PER_TSS`（6 + 8 × 2 = 22） |

第 3 槽的变化不止是槽号平移：64 位的用户段在这里多插了一个无人选择的副本。它存在的原因是 `sysret64` 取选择子的规则——返回时 CS 取 STAR 高 16 位加 16、SS 取加 8，因此 STAR 的高位要指向 anchor 槽本身——用户 CS 在它之后两格（`CS = STAR+16`、`SS = STAR+8`；源码注释把 anchor、用户 DS、用户 CS 这组连续三格写作 "THREE slots"）。§4.3 会把这条规则与一次真实的失败记录对上。

**只把 TSS 描述符的低 8 字节放进 GDT 映像（高半部分放进另一个不暴露的数组）会怎样**：长模式的 TSS 描述符是 16 字节，高 8 字节里存的是基址的 63:32 位——`ltr` 读到的基址高半为 0。内核在 0xFFFF_8000_... 一带的 TSS 结构体被解释成 0x0000_0000_xxxxxxxx 的物理地址，第一次 CPL3→CPL0 转场时 CPU 按这个基址读 `sp0`，读到的是低地址上的任意内存内容。Rust 侧对此留了记录：这个写法在早期版本里确实存在，因为那时"没有人跨过 ring"，缺陷是潜伏的（`protection.rs` 的 [`fn setup_tss_for_cpu`](os/arch/src/x86_64/protection.rs:fn setup_tss_for_cpu) 的注释）。

### 3.2 一条真实的顺序依赖拆出两个 trait（→§2.4、§2.5）

C 的 `prot_init` 内部有两个独立职责：§2.4 的"TSS + 段描述符 + 内核栈"与 §2.5 的"IDT + 门 + 系统调用入口"。两者之间有一条真实的依赖：每个门的 `selector` 字段都写 `KERN_CS_SELECTOR`（`i386/protect.c` 的 [`int_gate`](minix3/minix/kernel/arch/i386/protect.c:int_gate)），所以门的可用性以 GDT 已被装载为前提。Rust 把这两个职责拆成两个 trait，并把顺序依赖写成接口契约（`protection.rs` 的 [`trait ProtectionArch`](os/arch/src/arch/protection.rs:trait ProtectionArch)、`trap_entry.rs` 的 [`trait TrapEntryArch`](os/arch/src/arch/trap_entry.rs:trait TrapEntryArch)）。

| 维度 | 单 trait 内 `#[cfg]` 分支 | 两个 trait（现码） |
|---|---|---|
| 加载顺序的表达 | 只能写在注释或运行期检查里 | 写在两个 trait 的文档契约里，调用点显式依次调用 |
| 实现体量 | 每个实现里混进另一职责的分支 | 每个实现只回答自己那组问题 |
| 单元测试 | 测"保护"要连带构造向量表 | [`protection.rs`](os/arch/src/x86_64/protection.rs) 与 `:trap_entry.rs` 各自可测 |
| 跨架构共性的表达 | 共性被分支淹没 | `ProtectionArch` 的接口对三个架构都表达"特权级 + 栈" |

职责到方法的映射（把 §2.4/§2.5 的 C 函数对上）：

| C 职责 | Rust trait 方法 |
|---|---|
| `tss_init` 的 `sp0`/栈顶保留字 | `ProtectionArch::init`（首次）、`set_kernel_stack`（运行期重写） |
| GDT 段描述符装填 | `ProtectionArch::init`（实现内部） |
| `prot_load_selectors` 的 `lgdt`/`ltr`/段重载 | `ProtectionArch::load` |
| AP 路径（[`i386/mpx.S`](minix3/minix/kernel/arch/i386/mpx.S) 的 AP 入口） | `ProtectionArch::init_ap` |
| `idt_init`/`gate_table_*`/`int_gate` | `TrapEntryArch::init` + `set_handler` |
| 系统调用 MSR（`tss_init` 的两段 `if`） | `TrapEntryArch::configure_syscall` |
| `idt_reload` | `TrapEntryArch::load` / `load_ap` |
| TSS 描述符地址重建（实例搬进全局之后） | `ProtectionArch::refresh_descriptors`（x86 用；aarch64/riscv64 默认 no-op） |
| GS 区/入口基址编程（BSP） | `ProtectionArch::program_entry_base`（x86 用；aarch64/riscv64 默认 no-op；AP 的同一动作在 `init_ap` 内部） |

两个 trait 的文档里各自写了顺序要求。`ProtectionArch` 的注释是 `ProtectionArch::init() + load() must complete before TrapEntryArch::init() is called, because trap entry descriptors (e.g., IDT gate descriptors on x86-64) reference segment selectors that must be valid in the GDT.`；`TrapEntryArch` 的注释是同一句话的另一半。它对应 §2.5 里 `int_gate` 固定写 `KERN_CS_SELECTOR` 这一事实——门的可用性以 GDT 已装载为前提。

**把两个 `load` 合成一个函数、并在它内部先 `lidt` 后 `lgdt` 会怎样**：`lidt` 会让 IDTR 指向一张门里写满 `KERN_CS_SELECTOR` 的表，而此刻 GDT 还是引导方的那张——同一个槽号在两张表里的含义不同。装载后第一个异常到达时，CPU 用新 IDT 找到门、用门里的选择子去当前 GDT 里查代码段，查到的要么非代码段要么非存在，异常递送的第一步就 #GP。依据：`TrapEntryArch` 的文档契约与 `int_gate` 里固定的内核 CS 选择子（`KERN_CS_SELECTOR`）。

### 3.3 表装进 static 再 load：地址稳定成为接口外的契约（→§2.4）

`lgdt`/`lidt`/`ltr` 把表的地址写进 CPU 的寄存器。写进去的地址在此后一直生效，直到下一次装载；因此"表对象在装载期间不能移动、不能消失"是一条必须满足的前提。C 侧它自动成立：`gdt`/`idt`/`tss` 是文件级静态数组，地址从链接期起就是常量。Rust 侧它不自动成立：表被收进结构体，结构体沿值语义传递，移动与析构都会改变地址或让字节失效。现码的选择是把实例先搬进全局、再装载：

```text
/* 代码引用，os/kernel/src/lib.rs:fn init_protection（E1 注释原文）。 */
// E1: the instance's address changed when it moved into the global —
// the TSS descriptors embedded in the GDT still encode the pre-move
// stack-local bases. Rebuild them from the final addresses before
// ltr; otherwise the first CPL3→CPL0 transition (user-mode entry)
// reads a dead TSS (sp0 = 0 → page fault at VA -8 on the frame push).
```

四步完整代码见 §4.2；E1 这条设计论据的原文如下。

三个候选被比较过：

| 候选 | 淘汰/选中 | 理由（落到具体后果） |
|---|---|---|
| 栈上构造，`load()` 后继续用局部变量 | 淘汰 | 函数返回即析构；GDTR/TR 仍指向已复用的栈内存，TSS 结构体的 `sp0` 变成栈上的旧字节 |
| 堆分配 + 泄漏（`Box::leak`） | 淘汰 | 引导期还没有通用堆；泄漏的分配在裸机环境下没有释放者 |
| 全局 `SyncUnsafeCell<Option<...>>`，装载前搬入 | 选中 | 地址从搬入起稳定；accessor 在未初始化时 panic，给出确定的失败点 |

结构体骨架视图（表就在实例内部，这是"搬移会改变表地址"的根源）：

```rust
/* 签名示意（字段与类型逐字取自源码；省略方法与 impl 块），os/arch/src/x86_64/protection.rs:struct X86_64Protection。 */
pub struct X86_64Protection {
    gdt: [u64; GDT_ENTRIES],
    tss: [Tss64; MAX_CPUS],
    cpu_count: u32,
    boot_cpu_id: u32,
}
```

**按最直觉的写法在栈上构造实例、`load()` 之后让实例随作用域结束会怎样**：`ltr` 已经把 TSS 描述符装进 TR，这个描述符里嵌着 TSS 结构体的地址；实例析构后那一段栈内存被后续调用复用。第一次用户态进入（CPL3→CPL0）时 CPU 从该描述符取基址、从那个地址读 `sp0`——读到的是复用后的字节，压栈目标随之变成一个任意地址。代码注释把后果写成了具体地址：`sp0 = 0 → page fault at VA -8 on the frame push`（帧压栈时写地址 −8 引发缺页）。依据：`trap_entry.rs` 的 [`fn load`](os/arch/src/x86_64/trap_entry.rs:fn load) 的 LIFETIME CONTRACT 注释与 `lib.rs` 的 [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection) 的 lifetime 契约注释。

### 3.4 用户态入口：能力探测换成一条路径 + 内核自己的标签（→§2.4）

| 项 | C（i386） | Rust（x86-64） | 处置 |
|---|---|---|---|
| 入口指令 | 按能力位逐个安装（`SYSENTER` 与 `SYSCALL` 各有一个独立的 `if`，两类指令都支持的 CPU 上两条路径都装） | 只有 `SYSCALL`/`SYSRET` | 淘汰 SYSENTER：长模式不提供该指令 |
| 能力探测 | `minix_feature_flags` 两个位，`tss_init` 里按位装 MSR | 无探测；直接装 SYSCALL 的四个 MSR | 淘汰 |
| 入口地址来源 | 两类汇编标签：`ipc_entry_sysenter`（1 个）与 `ipc_entry_syscall_cpu0..7`（8 个） | [`fn syscall_entry_va`](os/arch/src/x86_64/trap_stub.rs:fn syscall_entry_va) 返回内核自己的汇编标签 | 改型 |
| "我在哪颗 CPU" | per-CPU 入口标签 + `k_percpu_stacks[cpu]` 表 | per-CPU GS 区（`gs:0x0` 取内核栈顶） | 改型 |
| `KernelInfo.syscall_entry` | —— | 只作元数据；`configure_syscall` 不读它 | 降级为参考信息 |

`configure_syscall` 要写的四个 MSR（`trap_entry.rs` 的 [`fn configure_syscall`](os/arch/src/x86_64/trap_entry.rs:fn configure_syscall)）：`STAR`（按下的 CS 与返回时的用户 CS 锚点）、`LSTAR`（入口地址）、`SFMASK`（`SYSCALL` 时清除的 RFLAGS 位，这里只清 IF）、`EFER.SCE`（使能位）。其中 `STAR`/`EFER` 与 C 的 AMD 路径对应（语义相同、位置不同：C 写在 `tss_init` 里、Rust 侧归 `TrapEntryArch`）；`LSTAR`/`SFMASK` 是长模式替换 SYSENTER 后的新增，C 侧没有这两个 MSR。

GS 区是"入口如何找到内核栈"的答案（`trap_stub.rs` 的 [`struct GsArea`](os/arch/src/x86_64/trap_stub.rs:struct GsArea)）：

```rust
/* 代码引用，os/arch/src/x86_64/trap_stub.rs:struct GsArea（含三个字段的注释）。 */
#[repr(C, align(16))]
pub struct GsArea {
    /// Kernel stack top the SYSCALL entry switches to (GS_SLOT_KERNEL_STACK).
    pub kernel_stack_top: u64,
    /// Scratch slot where the entry parks the user RSP (GS_SLOT_USER_RSP).
    pub user_rsp: u64,
    /// This CPU's logical id (AP self-identity readback).
    pub cpu_id: u64,
}
```

`program_gs` 把该区的地址写进 `IA32_GS_BASE` 与 `IA32_KERNEL_GS_BASE` 两个 MSR（用户态尚未存在时两者同值，`swapgs` 无事可做）。三个字段由 `program_gs` 在 BSP/AP 启动时写入；入口汇编只读。C 侧的对应物是 `k_percpu_stacks[cpu]` 加 per-CPU 入口标签；Rust 把它换成"入口读 `gs:0x0`"，因此入口不再需要按 CPU 分身，一个标签全 CPU 共用。

**AP 起来之后不重写这四个 MSR 会怎样**：这些寄存器是 per-CPU 的，BSP 写过的值不会跟着 AP 走；该 CPU 的 LSTAR 仍是复位值 0（或引导方遗留值），它上面第一次用户态 `syscall` 就跳到地址 0。Rust 侧把这条需要写成了独立函数与注释：`values the BSP wrote via configure_syscall do not propagate to APs, and an AP without them faults on its first syscall`（`trap_stub.rs` 的 [`fn write_syscall_msrs`](os/arch/src/x86_64/trap_stub.rs:fn write_syscall_msrs)），AP 尾路径在启动时调用它。依据：该函数的注释与 [`smp.rs`](os/kernel/src/smp.rs) 的 AP 尾路径。

### 3.5 页表重建的淘汰与恒等段的继续服役（→§2.6）

| 项 | C | Rust |
|---|---|---|
| 谁建表 | `pre_init` 建一次（引导用），`prot_init` 重建一次（正式用） | [`fn arch_boot_impl`](os/kernel/src/lib.rs:fn arch_boot_impl) 一次建好恒等窗口、内核映射与 Direct Map 覆盖 |
| 保护结构初始化里是否碰页表 | 碰（`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`） | 不碰 |
| "仍需要 1:1"的消费者 | LAPIC MMIO、显存等按物理地址访问的对象 | 由 `arch_boot_impl` 的恒等窗口与 Direct Map 覆盖承担（01/02 篇已讲） |
| `kinfo.freepde_start` | `pre_init` 存值、32 位跨地址空间机制消费 | 已由 `01-boot-shim-bootstrap.md` §3.9 处置（字段保留、消费者搬走） |

`init_protection` 的函数体里没有任何 CR3/页表调用。可复现读数：

```text
$ sed -n '/pub fn init_protection/,/^}/p' os/kernel/src/lib.rs | grep -nE 'cr3|paging|pg_|PageTable|map_'
（无输出）
```

`IDENTITY_MAP_END` 这个常量在 [`lib.rs`](os/kernel/src/lib.rs) 里保留；`build_bootstrap_root_and_enable` 的 Step 1 注释写着它与 C 的对应（`Maps 1024 × 4MB = 4GB (C) or 4 × 1GB (x86-64)…`），常量本身定义在 `os/arch/src/lib.rs`。

**把 C 的页表重建照搬进 Rust 的 `init_protection` 会怎样**：C 的"原地重填"成立靠两点——清空与重填之间不写 CR3（取指靠 TLB 旧条目，§2.6），以及 `pagedir` 是一个可以安全重填的静态数组。Rust 侧两点都不成立：正在使用的根表由 `arch_boot_impl` 建立、Direct Map 的叶子登记挂在它上面，清空会把已登记的叶子抹掉，而重填的来源（`arch_boot_impl` 的 Step 划分）不在这个函数里；页表由分配器按页给出，没有 `pagedir` 那样一个原地可重填的静态数组。依据：`lib.rs` 的 [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection) 的函数体（无建表调用）与 `:fn arch_boot_impl` 的 Step 划分，以及 02 篇 §4.4 的状态表。

### 3.6 把 cstart 拆成两个里程碑（→§2.2）

C 的 `cstart` 把四次调用放在一个函数里；Rust 按"哪一步之后系统多出一种能力"拆开，并把 kmain 的全部工作划成 Phase。现行注释块给的划分是（[`lib.rs`](os/kernel/src/lib.rs) 里 `kmain` 的文档注释；每个 Phase 的可锚定落点：B 的 `lib.rs` 的 [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection) 与 `:fn init_clock_and_interrupts`、C 的 `:fn init_proc_and_boot`、D 的 `:fn init_post_and_memory`、F 的 `:fn bsp_finish_booting`，E 没有函数落点）：

```rust
/* 代码引用，os/kernel/src/lib.rs:fn kmain（文档注释里的 Phase 清单）。 */
/// Phase A (this function): Entry — validate kinfo, allow kernel alloc
/// Phase B: cstart — prot_init + clock + intr (software only) + arch_init
/// Phase C: proc_init + arch_boot_proc
/// Phase D: arch_post_init + memory_init
/// Phase E: (no boot-time action — C `system_init` became the `enum Syscall`
///          dispatch; per-call work runs when syscalls arrive)
/// Phase F: bsp_finish_booting (timer program+register+gate) + switch_to_user
```

Phase 清单把时钟门的硬件编程与放行留到 Phase F：Phase B 只做软件态，硬件编程与门放行在 `bsp_finish_booting` 里完成（决定的注释在 [`timer_irq_gate.rs`](os/arch/src/x86_64/timer_irq_gate.rs)；时钟主题见 `05-clock-interrupt-init.md`）。

本篇展开 Phase A 与 Phase B 中的保护结构部分：

| Phase | C 的对应 | 本篇展开 | 归属 |
|---|---|---|---|
| A | `kmain` 前半段 | §4.1 | 本篇 |
| B（保护结构） | `cstart` → `prot_init` | §4.2–§4.6 | 本篇 |
| B（时钟与中断控制器） | `cstart` → `init_clock`/`intr_init`/`arch_init` | 不展开 | 时钟与中断主题（`05-clock-interrupt-init.md`；`arch_init` 的 TSS 部分见 §2.4 末尾） |
| C | `proc_init` + boot image 循环 | 不展开 | 进程表与 boot 进程主题 |
| D | `arch_post_init` + `memory_init` | 不展开 | 跨地址空间主题（`07-cross-space-init.md`） |
| E | `system_init`（Rust 侧由分派枚举替代） | 不展开 | syscall 分派主题 |
| F | `bsp_finish_booting` + SMP + `switch_to_user` | 不展开 | 启动收尾与 SMP 主题 |

两个里程碑的次序不可交换，理由落在"中断源"而不是"中断处理"：`init_clock_and_interrupts` 里控制器初始化结束时全部中断线是屏蔽的，次序的意义在于保护结构必须先就绪，这样控制器编程期间的总线异常也有可用的栈与门。可复现读数：`rg -n "fn mask_all" os/plat/src` 命中三架构实现；`rg -n "intr.init\\(\\)" os/kernel/src/lib.rs` 命中 `init_clock_and_interrupts` 的调用点（其注释写明 `mask_all()` 在 `init` 内部）。

**把 `init_clock_and_interrupts` 提到 `init_protection` 之前会怎样**：控制器编程期间若发生异常或 NMI（可屏蔽中断被 IF=0 挡在门外），CPU 会按尚未装载的 IDTR 递送（此时 IDTR 仍是引导方的值，而表体所在内存在 `ExitBootServices` 后不再受保护）；即使不谈内存回收，门里的选择子与内核新 GDT 也可能不匹配。依据：`lib.rs` 的 [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection) 的"must be the very first thing called in kmain"注释（指 Phase B 层级）与 `:fn init_clock_and_interrupts` 里 `intr.init()` 的全屏蔽语义；`init_protection` 的注释与函数体的出入在 §5.3 有披露。

### 3.7 门在第一次 load 之前就有真入口（→§2.5）

C 把真实处理函数地址（`mpx.S` 里的标签）在填 IDT 时一次写入；Rust 把这一步显式拆成两段：`TrapEntryArch::init()` 写元数据（DPL、IST、门类型、占位 0），`install_trap_stubs` 把真实入口写进每一个向量，然后才 `load()`。

| 项 | 选择 | 否掉什么 | 代价 |
|---|---|---|---|
| 门入口的写入时机 | `load()` 之前全部写入 | "先装载、后补入口" | 需要在装载前验证覆盖（`gate_present` 的存在性检查） |
| 覆盖范围 | 41 个向量一次写齐 | 按需逐条 `set_handler` | 尚未有生产者的向量也预留门（与 C 的整表一次填一致；41 项之外的表项保持非存在） |
| IST 恢复 | `set_handler` 写 IST=0 后由安装器恢复 NMI/#DF | 让 `set_handler` 自己记得 IST | 多一次簿记 |

门的清单与 C 的差异用现算的读数对齐：

```
$ grep -n "const IDT_ENTRIES" os/arch/src/x86_64/trap_entry.rs
113:const IDT_ENTRIES: usize = 256;
$ sed -n '/const STUB_VECTORS/,/^];/p' os/arch/src/x86_64/trap_stub.rs | grep -cE '^\s+\(0x|^\s+\([0-9]+,'
41
```

41 = 18（异常，0–8、10–14、16–19，跳过长模式保留的 9/15）+ 4（软中断 32–35）+ 16（PIC 的 0x50–0x57 与 0x70–0x77）+ 3（0xF0 调度 IPI、0xF1 LAPIC 本地定时器、0xFF 伪中断）。C 侧的 39 = 16 + 23，其中 23 条门含向量 9（`COPROC_SEG_VECTOR`，[`include/archconst.h`](minix3/minix/kernel/arch/i386/include/archconst.h)）、不含向量 15。Rust 比 C 少 1 条异常（向量 9）、多 3 条 LAPIC/调度相关向量：41 = 39 − 1 + 3。

**先 `load()` 再装门入口会怎样**：窗口内到达的异常或中断会走门里偏移 0 的入口——在那个地址上取指，取到的要么是空内存要么是别人；`lidt` 只保证 CPU 找得到门，不保证门有效。Rust 把这条顺序写成纪律与检查：`install_trap_stubs` 在 `load()` 之前一次写齐 41 个入口；`gate_present` 提供"某向量是否已有门"的读数（`trap_entry.rs` 的 [`fn gate_present`](os/arch/src/x86_64/trap_entry.rs:fn gate_present) 的注释："the stub installer cross-checks its coverage against this before the table may go live via `load()`"——执行这条交叉核对的是宿主测试 `test_stub_coverage_matches_idt_gates`，见 §5.1；`load()` 本身不检查，真机载体不调 `install_trap_stubs`（§5.2）、门里保持占位 0 也能通过，正说明这一点）。依据：`lib.rs` 的 [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection) 里 `install_trap_stubs` 在 `load` 之前、[`lib.rs`](os/arch/src/lib.rs) 里 `install_trap_stubs` 的 x86_64 分支转发到 `trap_stub.rs` 的 [`fn install_idt_handlers`](os/arch/src/x86_64/trap_stub.rs:fn install_idt_handlers) 的遍历，以及 `:fn test_stub_coverage_matches_idt_gates`。

---

## 4. 实现详解

### 4.1 kmain Phase A 的落地（→3.6）

C 的 Phase A 五步在 Rust 侧有一处结构性变化：全局副本不是"拷贝结构体"，而是把启动信息的三个变长载荷（保留区间、内存映射、模块表）落进 `.bss` 里的固定存储，再发布一个全局 `KernelInfo`。这一步的动因写在 `kmain` 入口的注释里（同文件的可锚定落点：`lib.rs` 的 [`fn kernel_info`](os/kernel/src/lib.rs:fn kernel_info) 与 `:fn store_kernel_info`）：入口的参数引用被编译器 spill 到内核栈之后再读回，栈槽遭别名覆写，经参数读到的字段全是 `.rdata` ASCII 垃圾。因此 `kmain` 的第一个动作是改从全局槽取引用：

```rust
/* 节选（保留入口取值；诊断输出块与说明注释省略），os/kernel/src/lib.rs:fn kmain。 */
    let kernel_info: &KernelInfo = crate::kernel_info()
        .expect("kmain: KERNEL_INFO must be stored by arch_boot before jump");
```

`store_kernel_info` 做的五块落地（保留区间、memmap、模块表+名字池、平台源列表、启动参数缓冲）见 `lib.rs` 的 [`fn store_kernel_info`](os/kernel/src/lib.rs:fn store_kernel_info)。随后三步各有 C 侧对应：`KERNEL_MAY_ALLOC.store(true, Ordering::Release)`（闸门；Rust 侧的 [`fn kernel_may_alloc`](os/kernel/src/lib.rs:fn kernel_may_alloc) getter 由 [`memmap.rs`](os/kernel/src/memmap.rs) 的 `add_memmap` production `debug_assert` 使用）、`FREE_MEMMAP` 的拷贝与逐模块 `cut_memmap`（模块物理内存的临时切除；归还发生在 Phase C——`lib.rs` 的 [`fn init_proc_and_boot`](os/kernel/src/lib.rs:fn init_proc_and_boot) 把 VM 的 ELF 段装进页表后调用 `memmap.rs` 的 [`fn add_memmap`](os/kernel/src/memmap.rs:fn add_memmap) 补回。Phase F 的 `add_memmap` 目标是 `bootstrap_start/len`，本移植下该区间长度为 0、恒为 no-op）、`minix_platform::init_from_kinfo`（平台描述符，早于时钟与中断）。

C 的"把内核镜像自己也切一刀"在 Rust 侧由内核侧的一处补充动作接上：UEFI 路径上内核镜像不在 conventional memmap 里，这一步是空操作（`cut_memmap` 返回 `Err`，代码显式丢弃并注明这是预期），OpenSBI 路径上它防止 GET_MEMINFO 把内核镜像当空闲内存报出去（两条行为在源注释里逐条写明：[`lib.rs`](os/kernel/src/lib.rs) 的 Phase A Step 3，C 对位引用 `i386/pre_init.c` 的 [`196-216`](minix3/minix/kernel/arch/i386/pre_init.c:196-216)）。

**不切全局副本、继续使用入口参数引用会怎样**：源码注释记录的现象是：参数经栈槽读回后字段变垃圾，`validate` 在第一处不变量上 panic；诊断块留下的四个读数（`rdi`、全局槽指针、`rsp`、`bootstrap_len`）就是为此加的。依据：`lib.rs` 的 [`fn kernel_info`](os/kernel/src/lib.rs:fn kernel_info) 与 `:fn store_kernel_info`（后者落五块载荷）。

### 4.2 init_protection 的逐步落地（→3.2、3.3、3.7）

`init_protection` 同时对三个决策负责：

```rust
/* 节选（保留保护结构、门装配与装载的次序；两处省略号分别为 `program_entry_base` 的 BSP 调用与 `register_current_arch` 的注册细节，段间说明注释按节选略去），os/kernel/src/lib.rs:fn init_protection。 */
    let prot = CurrentProtection::init(0, kernel_info.kern_stack_top);
    store_protection(prot);
    with_protection_mut(|prot| prot.refresh_descriptors());
    with_protection(|prot| prot.load());
    ...
    let mut trap = CurrentTrapEntry::init();
    use minix_arch::{install_trap_stubs, syscall_entry_va};
    install_trap_stubs(&mut trap);
    trap.configure_syscall(syscall_entry_va());
    ...
    trap_dispatch::register_current_arch();
    store_trap_entry(trap);
    with_trap_entry(|trap| trap.load());
```

次序与 §3.2/§3.3/§3.7 的对应：`store_protection` 在 `load` 之前（§3.3 的搬迁契约）；`refresh_descriptors` 紧跟搬迁（§3.3 的收尾）；`prot.load()` 在 `CurrentTrapEntry::init()` 之前（§3.2 的顺序依赖）；`install_trap_stubs` 在 `trap.load()` 之前（§3.7 的空门不变量）。函数的文档注释与函数体一致：本函数装真入口并 `load()`，没有“丢弃后重建”的后续阶段。

**把 `EFER.SCE` 的写入提前到 `LSTAR` 之前会怎样**：`EFER.SCE` 一旦置位，`SYSCALL` 指令立刻可用，而 LSTAR 可能仍是 0（复位值）；这条窗口里用户态的第一条 `SYSCALL` 跳到地址 0。两件事的正确顺序是"先把入口地址与掩码写全，再让机制生效"——`configure_syscall` 内部也是这个顺序（STAR → LSTAR → SFMASK → EFER）。依据：`trap_entry.rs` 的 [`fn configure_syscall`](os/arch/src/x86_64/trap_entry.rs:fn configure_syscall) 的写入次序与 `lib.rs` 的 [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection) 的调用点。

### 4.3 x86_64 的 GDT 与 TSS 装填（→3.1、3.3）

`fill_flat_segments` 填五个描述符。四个段描述符的访问字节是常量（内核 CS `0x9A`、内核 DS `0x92`、用户 CS `0xFA`、用户 DS `0xF2`，`protection.rs` 的 [`const KERN_CS_ACCESS`](os/arch/src/x86_64/protection.rs:const KERN_CS_ACCESS) 等），粒度字节区分代码段（`L=1` 表示 64 位代码）与数据段（`B=1`）；第 3 槽是 SYSRET anchor：

```rust
/* 代码引用，os/arch/src/x86_64/protection.rs:const GDT_USER_ANCHOR_INDEX（含注释）。 */
/// SYSRET anchor slot (AMD APM Vol 3 / SDM: sysret64 loads
/// `CS ← (STAR[63:48] + 16) | 3` and `SS ← (STAR[63:48] + 8) | 3` — the
/// STAR user base points THREE slots before the 64-bit user CS, with user
/// data one slot below it. Same convention as Linux's GDT
/// (`__USER32_CS`=anchor, `__USER_DS`=anchor+8, `__USER_CS`=anchor+16):
/// STAR[63:48] holds this slot's selector and SYSRET derives the real
/// CS/SS from it. The descriptor here is a copy of the user code segment;
/// nothing ever selects it — it exists positionally.
/// (The pre-fix layout put user CS at this slot and user DS at +8, so
/// sysretq loaded CS from the TSS descriptor at anchor+16 — live #GP(0x28)
/// on the first iret after a syscall in test-user-trap E8.)
const GDT_USER_ANCHOR_INDEX: usize = 3;
```

注释最后两行是一次真实失败的记录，可以逐字对上：修前布局把用户 CS 放在第 3 槽（anchor 位）、用户 DS 放在第 4 槽，于是 `sysretq` 按 anchor+16 取到第 5 槽——那套布局里第 5 槽是 TSS 描述符，选择子 `5*8 = 0x28`，CPU 试图把系统描述符当代码段装载，`#GP(0x28)`。`test-user-trap E8` 是这条故障的载体名，写在注释里。

TSS 的装填在 `setup_tss_for_cpu`：

```rust
/* 节选（保留 sp0 计算、IST 安装与描述符写入；省略号处为 cpu-id 戳记、iobase 与 IST、both-halves 三段说明注释），os/arch/src/x86_64/protection.rs:fn setup_tss_for_cpu。 */
        let usable_top = kernel_stack_top.get() - X86_64_STACK_TOP_RESERVED as u64;
        self.tss[idx].sp0 = usable_top;
        ...
        self.tss[idx].ist[0] = ist_stack_top(0, idx);
        self.tss[idx].ist[1] = ist_stack_top(1, idx);

        let tss_addr = &self.tss[idx] as *const Tss64 as u64;
        let desc = make_tss_desc64(tss_addr, TSS64_SIZE as u16 - 1, 0);
        ...
        let gdt_idx = GDT_TSS_FIRST_INDEX + idx * GDT_SLOTS_PER_TSS;
        self.gdt[gdt_idx] = desc[0];
        self.gdt[gdt_idx + 1] = desc[1];
```

`X86_64_STACK_TOP_RESERVED` 是 `2 * size_of::<u64>()` = 16 字节，与 C 的 `2 * sizeof(reg_t)` 同构；保留字里的 CPU 号由 `stamp_cpu_id` 写（写 `usable_top + 8`，即两个 64 位字里的第二个），`init` 给 BSP 写、`set_kernel_stack` 给 AP 写。`ist_stack_top` 从一个静态数组的两个槽位里取栈顶（`IST_STACKS[slot][cpu]`，每栈 8 KiB），NMI 与 #DF 的专用栈由此而来。

`load_with_tss` 是装载序列，其中第 4 步与 GS 区的次序构成一条被记录过的坑：

```rust
/* 节选（保留 lgdt、远跳重载 CS、数据段重载、FS/GS 清零、ltr 五步；省略号处为各条 asm 的操作数绑定、options 与说明注释），os/arch/src/x86_64/protection.rs:fn load_with_tss。 */
            core::arch::asm!("lgdt [{gdtr}]", ...);
            ...
            core::arch::asm!(
                "mov {cs}, {sel}",
                "push {cs}",
                "lea {addr}, [2f + rip]",
                "push {addr}",
                "retfq",
                "2:",
                ...);
            ...
            core::arch::asm!("mov ds, {0:x}", "mov es, {0:x}", "mov ss, {0:x}", ...);
            core::arch::asm!("xor {0:e}, {0:e}", "mov fs, {0:x}", "mov gs, {0:x}", ...);
            let tr_sel: u16 = Self::tss_selector(cpu_id);
            core::arch::asm!("ltr {0:x}", in(reg) tr_sel, ...);
```

远跳重载 CS 的写法（`mov`+两次 `push`+`retfq`）是 64 位下重载 CS 的常规手段：`lgdt` 之后 CS 仍是旧选择子，不重载的话后续指令仍按旧描述符解释。片段里 `{cs}`/`{sel}` 是 asm 操作数占位符——选择子先经一个通用寄存器暂存；64 位没有立即数形式的远跳，重载 CS 只能走这条 push 加 `retfq` 的路径。`init_ap` 的注释记录了第 4 步与 GS 区的先后：`mov gs, 0` 在 x86-64 上会把 GS BASE 清零，因此 `program_gs` 必须在 `load_with_tss` 之后调用（后果见 §4.5）。

**沿用去 LDT 后的旧 Rust 槽号布局（用户 CS 第 3 槽、用户 DS 第 4 槽、TSS 从第 5 槽起；C 的布局在第 5 槽还有 LDT 占位、TSS 从第 6 槽起）会怎样**：按 §3.1 的 anchor 规则，`sysret64` 取到第 5 槽——那是 TSS 描述符，选择子 0x28；返回用户态的第一步就是 `#GP(0x28)`。这条推论与代码注释里记录的现场完全一致，区别只是这次读的是注释而不是实测。依据：`const GDT_USER_ANCHOR_INDEX` 的注释、`fn configure_syscall` 里 STAR 的构造、`const GDT_TSS_FIRST_INDEX` 的取值。

### 4.4 IDT 的装填与门覆盖（→3.2、3.7）

`X86_64TrapEntry::init` 逐条写门。异常区每条的 DPL 按 C 的 `gate_table_exceptions[]` 对齐（两条异常带 DPL=3：向量 3/4）；IST 是 64 位新增——C 的 32 位表没有该字段（§2.5），引文注释里的 "parity" 指这两个向量与 C 的异常表条目集合对齐（C 也有 NMI/#DF），不是 DPL 口径。本实现里两条走 IST（2 走 IST1、8 走 IST2）。软中断区 32–35 全部 DPL=3；PIC 区用一个循环覆盖 0x50–0x57 与 0x70–0x77；最后三个单独向量是 0xFF（LAPIC 伪中断）、0xF0（调度 IPI）、0xF1（LAPIC 本地定时器）。节选里 `set_gate` 的五个参数依次是（签名见 `trap_entry.rs` 的 [`fn set_gate`](os/arch/src/x86_64/trap_entry.rs:fn set_gate)）：向量号、handler 地址、DPL、IST 索引、`is_trap`——PIC 行的 `(0x50 + irq, 0, 0, 0, false)` 即 handler 占位 0（真入口稍后由 `install_idt_handlers` 写入）、DPL=0、IST=0、中断门（`false` 表示非陷阱门）。

```rust
/* 节选（保留 PIC 循环与三个单个向量；异常与软中断的门清单在节选段之前，省略号处为说明注释），os/arch/src/x86_64/trap_entry.rs:fn init。 */
        for irq in 0..8 {
            entry.set_gate(0x50 + irq, 0, 0, 0, false); // master PIC
            entry.set_gate(0x70 + irq, 0, 0, 0, false); // slave PIC
        }
        ...
        entry.set_gate(0xFF, 0, 0, 0, false);
        ...
        entry.set_gate(0xF0, 0, 0, 0, false);
        ...
        entry.set_gate(0xF1, 0, 0, 0, false);
```

`IdtEntry64` 的七个字段与 C 的 `gatedesc_s` 对照：`offset_low`/`offset_mid`/`offset_high` 三段拼出 64 位入口地址（C 只有两段）、`selector`、`ist`（C 没有）、`p_dpl_type`（存在位+DPL+门类型）、`_reserved`。`fn new` 里 `ist & 0x7` 与 `dpl & 0x3` 两处掩码是硬件字段宽度的反映。

真正的入口由 `install_idt_handlers` 写入：

```rust
/* 代码引用，os/arch/src/x86_64/trap_stub.rs:fn install_idt_handlers（整函数）。 */
pub fn install_idt_handlers(entry: &mut X86_64TrapEntry) {
    let mut i = 0;
    while i < STUB_VECTORS.len() {
        let (vector, _) = STUB_VECTORS[i];
        let handler = stub_va(vector).expect("STUB_VECTORS entry without a stub symbol");
        entry.set_handler(
            InterruptVector::new(vector),
            handler,
            user_accessible(vector),
        );
        // C gate_table_exceptions parity: NMI and #DF run on dedicated IST
        // stacks (IST1/IST2, programmed into the per-CPU TSS) — a
        // gate with IST≠0 must not have it flattened by set_handler.
        match vector {
            2 => entry.set_gate_ist(2, 1),
            8 => entry.set_gate_ist(8, 2),
            _ => {}
        }
        i += 1;
    }
}
```

`set_handler` 写门时 IST 一律写 0（它没有 IST 参数），随后 2/8 两个向量由 `set_gate_ist` 恢复——这两条是 NMI 与双故障，门里写的 IST 号是 1 与 2，硬件按 `TSS.ist[ist-1]` 取值，所以实际用到的是 TSS 的 `ist[0]`/`ist[1]`（`setup_tss_for_cpu` 只填这两格）。`STUB_VECTORS` 的每项还带一个"是否由 CPU 压入错误码"的布尔值，供入口汇编决定栈帧形状（`trap_stub.rs` 的 [`fn has_errcode`](os/arch/src/x86_64/trap_stub.rs:fn has_errcode)）。

**让 `set_handler` 写完门就完事、不做 IST 恢复会怎样**：门里 IST 字段为 0 的含义是"这类异常在当前栈上递送"；NMI 的用途之一是打断已经跑飞的代码、双故障的本意是"当前栈已经不可用时的最后一道门"。双故障门落回坏栈上递送，CPU 连最后的诊断机会都没有，直接三重故障。依据：`:fn set_gate_ist` 的存在与 `:fn install_idt_handlers` 里 `match vector { 2 ..., 8 ... }` 两行。

### 4.5 系统调用入口与 per-CPU 数据的落地（→3.4）

`configure_syscall` 的四个 MSR 与 C 的 AMD 路径逐项对应：

```rust
/* 节选（保留四个 MSR 的构造与写入；`unsafe` 块头、SAFETY 注释与说明按节选略去），os/arch/src/x86_64/trap_entry.rs:fn configure_syscall。 */
            let star = (KERN_CS_SELECTOR as u64) << 32
                     | (USER_STAR_ANCHOR_SELECTOR as u64) << 48;
            wrmsr(MSR_STAR, star);
            wrmsr(MSR_LSTAR, entry_point.get());
            wrmsr(MSR_SFMASK, SFMASK_CLEAR_IF);
            let efer = rdmsr(MSR_EFER);
            wrmsr(MSR_EFER, efer | EFER_SCE);
```

STAR 的 63:48 存 anchor 选择子，47:32 存按下的内核 CS；LSTAR 存入口地址（`syscall_entry_va()`，即 `trap_stub.rs` 的 [`static x86_syscall_entry`](os/arch/src/x86_64/trap_stub.rs:static x86_syscall_entry) 的地址）；SFMASK 只置 `0x200`（IF），因为 `SYSCALL` 不自动关中断，内核要以明确的方式控制 IF；EFER 读改写只置 `SCE`。C 侧的 `set_star_cpu` 宏写的是同一个 STAR（`USER_CS_SELECTOR << 16 | KERN_CS_SELECTOR`，32 位模式下 STAR 的高 16 位直接是用户 CS 选择子），差别来自长模式 `SYSRET` 的取法（§4.3）。

`program_gs` 与 `write_syscall_msrs` 是 BSP 与 AP 两条路径的分工：前者写当前 CPU 的 GS 区地址到两个 GS_BASE MSR；后者重写四个 SYSCALL MSR。两者都按"只有当前 CPU 的 MSR 会被改"这一前提写成"在目标 CPU 上执行"的函数。

**把 GS 区编程放到 `load_with_tss` 之前会怎样**：装载序列第 4 步的 `mov gs, 0` 会把 GS BASE 清零，刚写进 MSR 的基址丢掉；此后入口汇编读 `gs:0x0` 取内核栈顶，读到的是基址 0 处的内存内容。代码注释记录过一次这样的现场：`gs:0x10` 读回 IVT 字节。依据：`fn init_ap` 的 `Load order matters` 注释与 `fn load_with_tss` 的第 4 步。

### 4.6 aarch64 与 riscv64 的落地（→3.1、3.3）

aarch64 侧的 `ProtectionArch::init` 写的是 `TPIDR_EL1`，不是 `SP_EL1`：

```rust
/* 节选（保留初始化与哨兵判定；省略号处为逐行说明注释），os/arch/src/arm64/protection.rs:fn init。 */
    fn init(cpu_id: u32, kernel_stack_top: VirBytes) -> Self {
        ...
        if kernel_stack_top.0 != 0 {
            set_entry_sp(kernel_stack_top);
        }
        Self { cpu_count: cpu_id + 1 }
    }
```

理由写在 `protection.rs` 的 [`fn init`](os/arch/src/arm64/protection.rs:fn init) 的注释里：EL1h 模型下 `SPSel=1`，`SP_EL1` 就是当前的活动栈指针，`msr SP_EL1, Xt` 在这个窗口里的编码是 UNDEFINED，会触发同步异常（启动期观察到的正是这条异常）。异常进入时不发生硬件换栈——这一点与 x86 的 TSS 相反——所以每个 CPU 的栈基址停在 `TPIDR_EL1`，由返回用户的路径在 `eret` 之前重装（这条栈基址约定与 `TPIDR_EL1` 的所有权写在 `set_entry_sp` 的文档注释里）。`set_kernel_stack` 因此是文档化的 no-op：在返回中的 Rust 函数里搬动 `SP_EL1` 会把调用者帧挂空（函数尾声要从新栈弹 `x29/x30`）。`load` 只发一条 `isb`。

`TrapEntryArch::load` 写 `VBAR_EL1` 再 `isb`（`trap_entry.rs` 的 [`fn load`](os/arch/src/arm64/trap_entry.rs:fn load)）。向量表本体由 `trap_stub` 的 `global_asm!` 定义：2 KiB（16 槽 × 128 字节）、`.align 11` 保证基址低 11 位为 0，槽位按"异常类别 × 来源"固定排布。

riscv64 侧写 `sscratch`：

```rust
/* 代码引用，os/arch/src/riscv64/protection.rs:fn set_entry_stack_base（整函数）。 */
fn set_entry_stack_base(base: VirBytes) {
    debug_assert_eq!(base.get() % 8, 0, "kernel stack top must be 8-byte aligned");
    // SAFETY: CSR write to sscratch in S-mode with a valid kernel VA; each
    // hart writes only its own CSR, so no cross-hart alias exists.
    unsafe {
        asm!("csrw sscratch, {}", in(reg) base.get(), options(nomem, preserves_flags));
    }
}
```

`sscratch` 是用户陷阱的交换寄存器（进入内核时第一条指令 `csrrw sp, sscratch, sp` 把用户 sp 存进去、内核栈换出来），所以返回之后它不能继续当作“内核栈基址”用：返回路径由 `TrapReturnArch::prepare_user_return` 在每次用户返回前把调度方传入的 `stack_top` 重新写进本 hart 的 `sscratch`。状态只存在 per-hart CSR 里，没有共享单元——共享标量曾被 SMP bring-up 的 AP `init_ap` 覆盖，源码注释与 `os/arch/tests/riscv64_percpu_trap_stack_pin.rs` 都禁止它回归。`ProtectionArch::load` 在 riscv64 上没有需要执行的步骤（CSR 写入即生效）；异常入口由 `TrapEntryArch::load` 写 `stvec` 承载——Direct 模式（低两位为 0），实现里在两只入口腿（源码注释把内核与用户两套入口都称作 "leg"，这里沿用译为"腿"：**内核腿**/**用户腿**）之间切换 `stvec`，用户腿第一条指令无条件交换 `sscratch`（不按 `SPP` 判定；`SPP` 只在返回侧重锚 `stvec` 时被读），随后由软件按 `scause` 分派。

两条路径的**触发指令**在各体系结构里是固定的一条：aarch64 用 `svc`（[`trap_stub.rs`](os/arch/src/arm64/trap_stub.rs) 的文件注释写明 `svc` **会**把返回地址推进到下一条指令），riscv64 用 `ecall`（[`trap_stub.rs`](os/arch/src/riscv64/trap_stub.rs) 的文件注释写明 `ecall` **不**推进 `sepc`——这一点与 x86 的 `int`、arm64 的 `svc` 相反，返回地址的修正因此落在内核的 trap 入口体自己身上）。用户态执行到那条指令时才发生特权级切换与入口跳转；本篇只到"门与入口已经就位"，完整的"用户 → 内核 → 用户"往返（谁保存寄存器、谁切换栈、哪条指令把特权级降回去）归异常与中断篇（`14-exception-interrupt.md`）。

**aarch64 的 `init` 直接 `msr SP_EL1, kernel_stack_top` 会怎样**：EL1h 下 `SPSel=1`，`SP_EL1` 就是当前活动栈指针，ARMv8 D1.5 把 `msr SP_EL1, Xt` 在这个窗口里的编码定义为 UNDEFINED；执行它会触发同步异常（启动期观察到的正是这条异常），而不是把栈换掉。真正的栈基址因此停在 `TPIDR_EL1`，由 `restore_to_user` 在 `eret` 前重装。依据：`protection.rs` 的 [`fn init`](os/arch/src/arm64/protection.rs:fn init) 与 [`fn set_entry_sp`](os/arch/src/arm64/protection.rs:fn set_entry_sp) 的注释，以及 `test-protection-aarch64` 的只读 `SP_EL1` 探针。

把三架构的 Rust 落点按 §1.2 的三问收一张表：

| 三问 | x86-64 | aarch64 | riscv64 |
|---|---|---|---|
| 当前特权级 | GDT DPL + CS.RPL | EL0/EL1 | U/S mode |
| 异常/中断/syscall 入口 | IDT 41 门（16 字节门） | `VBAR_EL1` 的 16 槽向量表 | `stvec` 的内核腿/用户腿 |
| 陷入内核用哪根栈 | TSS.sp0 + IST + GS 区 | SP_EL1 + `TPIDR_EL1` | `sscratch` + `prepare_user_return` |

### 4.7 与 Minix3 C 的对照（→§2.1–§2.7）

| C 侧的行为 | Rust 侧落点 | 处置 | 差异原因 |
|---|---|---|---|
| `kmain` 拷 `kinfo` 到全局 | [`fn store_kernel_info`](os/kernel/src/lib.rs:fn store_kernel_info)（把五个载荷落进 `.bss`、发布全局副本） | 改型 | 值语义与 `.bss` 落地；引用不能跨镜像存活（01 篇 §3.4） |
| `bss_test` 断言 | 无对应物 | 淘汰 | BSS 清零由 boot-shim 装载器保证（测试覆盖见 `01-boot-shim-bootstrap.md` §5.2.1） |
| `kernel_may_alloc` 闸门 | [`static KERNEL_MAY_ALLOC`](os/kernel/src/globals.rs:static KERNEL_MAY_ALLOC)（Phase A 置 true） | 保留（改型为原子量） | 同一语义：VM 启动前允许内核直接分配 |
| `cstart` 单函数 | [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection) + `:fn init_clock_and_interrupts` | 改型 | 按里程碑拆分（§3.6） |
| `prot_init` 的表清单 | `:fn init_protection` 的两段（保护结构 + trap 层） | 改型 | 两个 trait 的职责划分（§3.2） |
| `tss_init` 的 `sp0` 与栈顶保留字 | [`fn setup_tss_for_cpu`](os/arch/src/x86_64/protection.rs:fn setup_tss_for_cpu) + `:fn stamp_cpu_id` | 保留（宽度随 64 位） | 保留字从 2×4 字节变 2×8 字节 |
| `_cpufeature(SYSENTER/SYSCALL)` + 两套入口 | `:fn configure_syscall` 只装 SYSCALL | 淘汰 SYSENTER | 长模式不提供 SYSENTER |
| per-CPU 入口标签 + `k_percpu_stacks` | [`struct GsArea`](os/arch/src/x86_64/trap_stub.rs:struct GsArea) + `:fn program_gs` | 改型 | 入口从 GS 区取栈顶 |
| `prot_load_selectors` 的 `lldt` | [`fn load_with_tss`](os/arch/src/x86_64/protection.rs:fn load_with_tss) 注释说明不执行 `lldt` | 淘汰 | 长模式不使用 LDT |
| 重建页表（`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`） | [`fn arch_boot_impl`](os/kernel/src/lib.rs:fn arch_boot_impl) 建一次 | 淘汰重建 | §3.5 |
| `prot_init_done` 标志 | 全局 `Option` + `with_protection`/`with_trap_entry` 的 panic 契约 | 改型 | 未初始化成为一个确定的失败点 |
| `gate_table_*` 39 条门 | [`const STUB_VECTORS`](os/arch/src/x86_64/trap_stub.rs:const STUB_VECTORS) 41 项 + `:fn install_idt_handlers` | 改型 | 长模式保留 9/15、增加 LAPIC/调度向量 |

本篇做完之后，系统处在这样一个状态：三张表都已装载生效、门里有真入口（§3.7），而**尚未放行任何中断源**（IF=0 一直保持、控制器尚未编程；"有门可走，但没有源被放行"（§2.2）在当前即成立，控制器的编程与放行在下一篇）；进程与用户态都还没有。下一件必须做的事是 `kmain` Phase B 的剩余部分：时钟与中断控制器的初始化（`lib.rs` 的 [`fn init_clock_and_interrupts`](os/kernel/src/lib.rs:fn init_clock_and_interrupts)），它的次序与内部拆分是 `05-clock-interrupt-init.md` 的主题。

---

## 5. 测试要点

### 5.1 宿主：arch crate 的单元测试（→4.3、4.4）

宿主上能证明的是"表里的位与字段"，不是"CPU 装载之后的行为"。测试计数与分布：

```
$ for f in os/arch/src/x86_64/protection.rs os/arch/src/x86_64/trap_entry.rs os/arch/src/x86_64/trap_stub.rs os/arch/src/arm64/protection.rs os/arch/src/arm64/trap_entry.rs os/arch/src/arm64/trap_stub.rs os/arch/src/riscv64/protection.rs os/arch/src/riscv64/trap_entry.rs os/arch/src/riscv64/trap_stub.rs; do printf "%s: " "$f"; grep -c '#\[test\]' "$f"; done
os/arch/src/x86_64/protection.rs: 16
os/arch/src/x86_64/trap_entry.rs: 13
os/arch/src/x86_64/trap_stub.rs: 11
os/arch/src/arm64/protection.rs: 2
os/arch/src/arm64/trap_entry.rs: 1
os/arch/src/arm64/trap_stub.rs: 3
os/arch/src/riscv64/protection.rs: 2
os/arch/src/riscv64/trap_entry.rs: 1
os/arch/src/riscv64/trap_stub.rs: 7
```

x86_64 的 16 个保护测试里与本篇直接相关的几类：TSS 布局（`tss64_size_is_104_bytes`、`tss64_offsets_correct`）、描述符位组合（`gdt_descriptors_have_correct_dpl`、`gdt_descriptors_are_flat_mode`、`tss_descriptor_is_64bit`）、描述符与结构体地址的一致性（`tss_descriptor_high_half_carries_base_above_4gib`、`init_creates_tss_descriptor_in_gdt`）、`sp0` 算术（`set_kernel_stack_updates_sp0`、`init_sets_tss_sp0_below_reserved_area`）。13 个 trap 测试覆盖门表：异常区存在性、保留向量非存在、软中断 DPL=3、PIC 区 DPL=0、NMI/#DF 的 IST。IPC 门（向量 33）由 `install_trap_stubs` 装 stub，不再单列门测试。11 个 trap_stub 测试里有一条覆盖检查，把 `STUB_VECTORS` 与 `X86_64TrapEntry::init` 标记的门对齐核验。

aarch64 与 riscv64 侧的数字对应的是浅测试：两份 `protection.rs` 各 2 条（`privilege_level_roundtrip`、`protection_has_cpu_count`）、两份 `trap_entry.rs` 各 1 条 trait 边界断言（`aarch64_trap_entry_arch_impl_satisfies_trait_bound` 与 `riscv64_trap_entry_arch_impl_satisfies_trait_bound`）。两份 `trap_stub.rs` 各 3 与 7 条，覆盖向量表布局与入口腿的静态断言。它们证明的是枚举往返、结构体字段与表布局，不覆盖 §4.6 的寄存器语义——那部分只能在真机上读（§5.2）。

宿主覆盖不到的部分需要说清：`lgdt`/`lidt`/`ltr` 真正执行后的 CPU 状态、CPL3→CPL0 转场的实际栈切换、AP 上 MSR 的实际写入、`isb`/`csrw` 之后的可见性——这些只能在真机上读寄存器。

### 5.2 真机：test-protection 三架构（→4.2、4.6）

三份测试载体各自把"`init_protection` 之后 CPU 的寄存器读数"作为判据，对生产代码的复用程度不同：x86_64 与 riscv64 版复用生产 trait 方法（`CurrentProtection::init`/`load` 与 `CurrentTrapEntry::init`/`configure_syscall`/`load`；x86_64 版不调 `install_trap_stubs`，门里的入口保持占位 0），aarch64 版是独立探针——不调 `init_protection`/`CurrentProtection::init`/`CurrentTrapEntry`，自己装 `VBAR_EL1` 与探针汇编。

x86_64 版（[`main.rs`](os/qemu-tests/test-kernels/kernel/bootstrap/test-protection/src/main.rs)）在 `arch_boot_impl` 之后按 `kmain` Phase B 的方针调用 `CurrentProtection::init`/`load` 与 `CurrentTrapEntry::init`/`configure_syscall(syscall_entry_va())`/`load`（生产入口，不是 `KernelInfo.syscall_entry` 元数据），然后读五处表寄存器（GDTR、IDTR、TR、CS、DS）与四个 SYSCALL MSR（STAR/LSTAR/SFMASK/EFER.SCE）。实例留在栈上（main 不返回），所以载体不覆盖 `store_protection` + `refresh_descriptors` 的搬迁收尾；那一步由 `test-user-trap` 的生产 `init_protection` 覆盖。

```rust
/* 代码引用（保留判定与输出两行），os/qemu-tests/test-kernels/kernel/bootstrap/test-protection/src/main.rs（x86_64 版）。 */
    if gdtr_base == 0 {
        early_console::write_str("  FAIL: GDT base is 0\n");
```

同一文件里 `sidt` 读 IDTR、`str` 读 TR、`mov cs/ds` 读选择子低两位（RPL=0 即 ring0）。aarch64 版读 `CurrentEL`（必须等于 EL1）、`VBAR_EL1`（非零）、`DAIF`（全屏蔽），并做 `SPSel` 切换与 `SP_EL1` 只读回环——回环只有一条 `mrs x0, SP_EL1`，文件注释标注 READ-ONLY（代码里没有对 `SP_EL1` 的写；`msr SP_EL1` 这一串只出现在解释此前一次崩溃的记录注释里）；回环放在 `SPSel=0` 窗口内做，文件注释说明原因：`SPSel=1` 时 `SP_EL1` 就是当前 SP，测试会把自己的栈换掉。riscv64 版读 `stvec`（非零且低两位为 0，即 Direct 模式）、`sscratch`（必须等于 `kern_stack_top`）、`sstatus`（非零）。

运行方式与判定标记：`os/qemu-tests/run_all.sh` 把三个载体分别列入三架构的测试清单，判定由 `os/qemu-tests/run_qemu.sh` 按串口日志里的 `### TEST_RESULT: PASS` 前缀匹配完成（载体实际打印 `### TEST_RESULT: PASS test-protection[-aarch64|-riscv64] ###`）。

把三个载体合起来看：它们验证的是三张表与 MSR 的装载面（位与字段层在 §5.1 的宿主测试里）；`test-protection` 的 x86_64 载体不调 `install_trap_stubs`、门里入口保持占位 0，因此**门有效性**不在**这三个 test-protection 载体**的覆盖里——它由 §5.1 的覆盖检查、`test-user-trap`（生产 `init_protection` 装真 stub 后以 CPL3 `int 0x21` 进门）与 §5.3 的负向缺口分别承担。

**把 aarch64 的 `SP_EL1` 探针放回 `SPSel=1` 窗口内执行写入会怎样**：写的就是当前 SP，执行写入的那段代码会在读写之间把自己的栈换掉，崩溃会被误读成"EL2 陷获了 SP_EL1 访问"——测试文件里保留了这段定位记录。依据：`.../test-protection-aarch64/src/main.rs` 里 `Spsel` 窗口的注释段。

### 5.3 测试缺口

| 缺口 | 现状 | 补法 | TODO |
|---|---|---|---|
| `init/load` 顺序不变量的负向验证 | 无测试；顺序只写在调用点与注释里 | 构造"先 load 后装门"的测试载体，断言装载前的 `gate_present` 检查失败 | **R03-T01** |
| 门覆盖的负向验证 | 只有正向覆盖检查（`STUB_VECTORS` 与门表对齐）；漏掉一个向量时正向测试会红，但"漏掉后真机行为"没有载体 | 与上一条同族：在门表里人为留一个空向量，断言装载被拦 | **R03-T01** |
| `refresh_descriptors` 的后果 | 无宿主测试（需要真实地址）；`test-user-trap` 走生产 `init_protection` 后以 CPL3 进入，覆盖了首次用户态进入 | 由 `test-user-trap` 的首次用户态进入间接覆盖 | **R03-T02** |
| aarch64 `set_kernel_stack` 为 no-op 的行为 | API 层无可观察行为，只有文档说明 | 由返回用户态路径的真机载体覆盖（属异常/返回主题） | **R03-T04**（跨篇） |
| [`fn init`](os/arch/src/x86_64/trap_entry.rs:fn init) 的作者口径 | 函数体的门清单与 C 的 39 条对齐关系只在注释里 | 把 §3.7 的 41 项拆解写成断言（按区计数），任何一侧增删向量都会红 | **R03-T05** |

`init_protection` 的文档注释已与函数体同步：本函数装真入口并 `load()`，没有“丢弃后重建”的后续阶段；`install_trap_stubs` 在 `trap.load()` 之前完成，入口非零。

### 5.4 机制 → 需要证明什么（反转核对表）

| # | 机制（正文位置） | 需要证明什么（可观测断言） | 覆盖状态 | 现有证据 |
|---|---|---|---|---|
| M1 | 三张表的建表（§2.3–§2.5） | GDT 槽位/访问字节/DPL；TSS 描述符两半与字段；IDT 门字段与分布 | 已覆盖 | 宿主：16 个保护测试（`tss64_size`/`offsets`/`dpl`/`flat`/`descriptor_high_half`…）+ 13 个 trap 测试（门表五类） |
| M2 | 一次装载让表生效（§2.5） | 装载后 GDTR/IDTR/TR 指向内核自己的表 | 已覆盖（x86_64/riscv64 走生产 trait；aarch64 是探针） | 真机 `test-protection`：x86_64 读 GDTR/IDTR/TR/CS/DS；riscv64 读 `stvec`/`sscratch`；**aarch64 是独立探针**（自己写 `VBAR_EL1`/`DAIF`，不调 `TrapEntryArch::load`） |
| M3 | 门入口覆盖不变量（§3.7） | 41 个向量写齐；present 门 ↔ stub 一一对应；`load()` 在装齐之后 | 部分覆盖 | 宿主 `test_stub_coverage_matches_idt_gates`（正向）；`test-user-trap` 走生产 `init_protection` 以 CPL3 `int 0x21` 进门（真机正向）；**负向（先 load 后装门）无载体** → §5.3 缺口行；**R03-T01** |
| M4 | 系统调用入口 MSR（§4.5） | STAR/LSTAR/SFMASK/EFER 的值与写入次序；AP 上重写 | 部分覆盖 | 宿主不覆盖 MSR 面；x86 载体在 `configure_syscall` 后用 `rdmsr` 校验 STAR/LSTAR/SFMASK/EFER.SCE；**AP 侧重写仍是行为性、无自动断言** → **R03-T03**；本轮未跑 QEMU |
| M5 | 栈交接：`sp0`/保留字/GS 次序（§2.4、§4.3、§4.5） | 首次 CPL3→CPL0 用 `sp0`；GS 基址未被装载序列清掉 | 部分覆盖 | 宿主 `sp0` 算术 2 条；`init_ap` 的 production 断言校验 AP `sp0 == 自己的栈顶 - 16`；生产 `smp_init` 的 sp0 写入由 `test-smp-aps` 端到端覆盖（本轮未跑）；`test-smp-ap-alive-mb` 手工设置 sp0 后调 `init_ap`；第一次跨 ring 由 `test-user-trap` 覆盖；AP 的 MSR 重写仍无自动断言 → **R03-T03**；aarch64 no-op 归 **R03-T04** |
| M6 | 描述符地址稳定（§3.3） | 实例搬迁后 TSS 描述符指向最终地址 | 部分覆盖 | 宿主无法构造（需真实地址）；由首次用户态进入的真机链间接覆盖 → **R03-T02** |
| M7 | 跨架构入口与交换（§4.6） | `stvec` 非零且 Direct；`sscratch = kern_stack_top`；`CurrentEL`/`VBAR_EL1`/`DAIF`；`SP_EL1` 只读 | 已覆盖 | 真机 `test-protection` aarch64/riscv64 的寄存器判据 |
| M8 | `sscratch` 返回重锚（§4.6） | 返回路径在每次用户返回前把本 hart 的 `sscratch` 重写为调度方传入的 `stack_top`（per-hart，无共享单元） | 未覆盖（本篇） | 属"用户→内核→用户"往返，归异常/中断篇的载体 → **R03-T06**（跨篇） |
| M9 | 重建页表的原地重填（§2.6，C 侧） | 清零→重填窗口内取指可行（TLB 旧条目）；`pg_load` 后表内容齐全 | 说明性 | 架构行为与 C 侧逻辑，不在本仓测试面；Rust 侧以淘汰代替（§3.5）→ **R03-T07**（登记，无动作） |
| M10 | 本篇出口状态（§4.7） | 门就位、无源放行（控制器未编程、IF=0） | 部分覆盖 | 真机链的时序读数；完整形态（控制器编程与放行）归 `05-clock-interrupt-init.md` → **R03-T01 附属**（顺序负向同族） |

（§5.3 的缺口分别对到本表 M3、M5、M6 与 M1 的断言化；缺口 ID 与分流见 §5.3，本表只把它们对到机制断言。）

---

## 6. 诊断

### 6.1 症状 → 怀疑对象 → 确认命令

| 症状 | 怀疑对象 | 确认动作 |
|---|---|---|
| `init_protection` 之前任何异常即三重故障 | 引导方 IDTR/GDT 随 `ExitBootServices` 失效 | QEMU `-d int,cpu_reset` 的 `check_exception` 行；GDB 在 `kmain` 入口读 `idtr`/`gdtr` 的 base |
| 进入 `kmain` 后第一次压栈失败，故障地址是低地址 | 栈未切换（02 篇 §4.3）或 `kern_stack_top` 值错 | 读入口的 `rsp` 与 `kern_stack_top`，比对高半范围 |
| `lgdt` 之后取指 #GP | 新 GDT 的段描述符位组合错，或选择子指向的槽没填 | GDB `info registers gdtr` 看 base/limit；测试载体按 §5.2 读 CS/DS 的 RPL |
| `ltr` 报 #GP，或第一次用户态进入时缺页在 `sp0=0` | TSS 描述符不在 GDT、描述符高半丢失、或实例被移动后未重建描述符 | 读 TR（`str`）；核 `refresh_descriptors` 是否在 `load` 前执行 |
| 第一次 `syscall` 落 0 或 #GP(0x28) | LSTAR 未写 / STAR anchor 槽位错 | 核 `configure_syscall` 的次序；核 GDT 槽位与 `USER_STAR_ANCHOR_SELECTOR` 的取值（0x28 对应受限场景见 §4.3） |
| IDT 装载后异常跳 0 | 门里入口为 0（未装 stub）、或漏装某个向量 | 核 `install_trap_stubs` 是否在 `load` 之前；核 `STUB_VECTORS` 是否覆盖该向量 |
| AP 首次用户态 `syscall` 失败 | 该 CPU 的四个 MSR 未重写（per-CPU） | 在 AP 尾路径确认 `write_syscall_msrs` 被调用；核 `program_gs` 的调用次序 |
| LAPIC/显存访问缺页，故障地址是物理地址本身 | 恒等窗口消失（C 侧重启路径；Rust 侧建表遗漏） | C 侧核 `pg_identity` 是否在重建段里；Rust 侧核 `arch_boot_impl` 的建表 Step |
| 重启（监控程序第二次进入内核）后异常跳进旧地址、GDT/IDT 槽内容疑似上一轮残留 | 表项残留——§2.3 的 `memset` 清零或 §2.6 的 `pg_clear`/重建被跳过 | 读 `gdtr`/`idtr` 指向的表内容与门表行数；核 `prot_init` 的两行 `memset` 与页表重建段是否在场（§2.3、§2.6 的反例） |
| aarch64 启动即同步异常，PC 落在 `TPIDR_EL1`/`SP_EL1` 相关代码 | `SPSel=1` 下写了 `SP_EL1` | 核 [`fn init`](os/arch/src/arm64/protection.rs:fn init) 是否走了 `set_entry_sp` 分支 |
| riscv64 首次用户陷阱返回后取到错的内核栈，或 `sscratch` 读数与预期不符 | 返回路径未在 `prepare_user_return` 里重写本 hart 的 `sscratch`，或 `stvec` 未按运行模式切回内核腿 | 读 `sscratch`/`stvec` 与前一次读数比较；核 [`fn set_entry_stack_base`](os/arch/src/riscv64/protection.rs:fn set_entry_stack_base) 与 §4.6 的入口腿说明 |

### 6.2 可复制命令

```bash
# 第一组：C 侧事实与常量（口径写在各条之后）
grep -c '#if' minix3/minix/kernel/arch/i386/protect.c
# 关键行：0。口径：文件里所有以 #if 开头的行；0 表示架构差异由独立文件承载。
grep -rn "^void prot_init" minix3/minix/kernel/arch/*/protect.c
# 关键行：两行（i386 与 earm 各一处定义）。
grep -rn "int kernel_may_alloc" minix3/minix/kernel/
# 关键行：三行——`glo.h` 一处声明 + i386/earm 两处定义点（初值都为 1）。
awk '/gate_table_pic\[\]/,/};/' minix3/minix/kernel/arch/i386/protect.c | grep -c 'hwint'
awk '/gate_table_exceptions\[\]/,/};/' minix3/minix/kernel/arch/i386/protect.c | grep -cE '_VECTOR'
# 关键行：16 与 23；两表合计 39 条门（口径：表体里的数据行，不含结尾的 { NULL, 0, 0 } 行）。
printf '%d\n' $((6+1))
# 关键行：7 —— GDT_SIZE = TSS_INDEX(CONFIG_MAX_CPUS)，CONFIG_MAX_CPUS = 1。

# 第二组：Rust 侧的门与测试计数
grep -n "const IDT_ENTRIES" os/arch/src/x86_64/trap_entry.rs
# 关键行：IDT_ENTRIES: usize = 256;
sed -n '/const STUB_VECTORS/,/^];/p' os/arch/src/x86_64/trap_stub.rs | grep -cE '^\s+\(0x|^\s+\([0-9]+,'
# 关键行：41（口径：以向量号开头的表项行；= 18 异常 + 4 软中断 + 16 PIC + 3 单独向量）。
for f in os/arch/src/x86_64/{protection,trap_entry,trap_stub}.rs os/arch/src/arm64/{protection,trap_entry,trap_stub}.rs os/arch/src/riscv64/{protection,trap_entry,trap_stub}.rs; do printf "%s: " "$f"; grep -c '#\[test\]' "$f"; done
# 关键行：16 / 13 / 11 / 2 / 1 / 3 / 2 / 1 / 7。

# 第三组：真机链（会在目标目录里重新构建）
bash os/qemu-tests/run_all.sh
# 或单跑一个载体：
os/qemu-tests/run_qemu.sh x86_64 os/target/x86_64-unknown-uefi/release/test-protection.efi
# 关键行：串口日志里的 ### TEST_RESULT: PASS test-protection ###（各架构后缀不同）；
#         失败时同一位置是 ### TEST_RESULT: FAIL ... ###，其前的若干行以 "  FAIL:" 开头。

# 第四组：GDB 远程（在 init_protection 前后读三个表寄存器）
# 给上一组的 QEMU 调用加 -s -S 两个调试开关（裸 QEMU 起法与各架构固件路径见 `01-boot-shim-bootstrap.md` §6.4）。
gdb <被调试的工件>
(gdb) break init_protection
(gdb) continue
(gdb) info registers gdtr idtr tr      # 此刻 gdtr/idtr 仍是引导方的值
(gdb) finish                            # 到函数返回前——IDT 的装载在函数体后半段（§4.2 的次序）
(gdb) info registers gdtr idtr tr
# 预期输出：两条 info 打印 gdtr/idtr 的基址与限长（tr 是选择子不是基址）；后一条的 gdtr.base 落在工件的高半地址段、
#           idtr.base 变为 IDT 数组地址、tr 的选择子非 0。不要用单条 `next` 代替 `finish`——IDT 装载隔着一串调用，
#           且 release 构建下步进粒度受内联影响。
```

---

## 7. 参见

### 7.1 前置与后续篇

- [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) — 启动信息、内存记账、页表建立与开启、`KernelInfo` 字段来源
- [02-higher-half-kernel.md](02-higher-half-kernel.md) — 链接布局、按装载地址摆放、成对跳转
- [04-platform-discovery.md](04-platform-discovery.md) — 平台参数发现
- [05-clock-interrupt-init.md](05-clock-interrupt-init.md) — 时钟与中断控制器初始化
- [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) — 进程表与 boot 进程装载（`kmain` Phase C）
- `rewrite-notes/02-stage-vm/07-pagetable-struct.md` — 页表多级结构与页表接口
- 后续主题级指向：异常/中断与 trap 入口（`14-exception-interrupt.md`）、syscall 分发（`13-syscall-dispatch.md`）

### 7.2 关联 C 源码清单

- `minix3/minix/kernel/{main,clock,table}.c`
- `minix3/minix/kernel/arch/i386/{protect.c,pre_init.c,pg_utils.c,arch_system.c,i8259.c,arch_smp.c}`
- `minix3/minix/kernel/arch/i386/{mpx.S,head.S,klib.S}`
- `minix3/minix/kernel/arch/i386/kernel.lds`
- `minix3/minix/kernel/arch/i386/include/{arch_proto.h,archconst.h,arch_smp.h}`
- `minix3/minix/kernel/{kernel.h,glo.h}`
- `minix3/minix/include/arch/i386/include/{interrupt.h,archtypes.h}`
- `minix3/minix/kernel/arch/earm/{protect.c,arch_system.c}`

### 7.3 关联 Rust 源码清单

- `os/kernel/src/{lib,globals,memmap,smp}.rs`
- `os/arch/src/arch/{protection,trap_entry}.rs`
- `os/arch/src/{x86_64,arm64,riscv64}/{protection,trap_entry,trap_stub}.rs`
- `os/arch/src/lib.rs`
- `os/qemu-tests/test-kernels/kernel/bootstrap/{test-protection,test-protection-aarch64,test-protection-riscv64}/src/main.rs`

### 7.4 构建与回归入口

- `os/qemu-tests/run_qemu.sh`、`os/qemu-tests/run_all.sh`
