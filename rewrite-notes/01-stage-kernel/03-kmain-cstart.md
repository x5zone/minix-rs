# 03 · kmain 与 cstart：三张表与内核第一次行使特权

> 路径与标注约定（`[ARCH: …]` 等记号的解码处）全局只说一次：见 [README.md](../README.md) 的「全局阅读约定」。

> **目标读者**：熟悉操作系统基本概念（特权级、分页、中断与异常、描述符表大意）与 C 语言的开发者；Rust 一侧读到 trait、`unsafe` 契约与内联汇编大意即可。
> **前置知识**：分页已开启、内核正在高半地址上执行、`kmain` 刚被进入（`02-higher-half-kernel.md` §4.4 的状态表是起点）；x86 的段选择子、门、TSS 三个词听过即可。**前置**：[01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md)、[02-higher-half-kernel.md](02-higher-half-kernel.md)（本系列前两篇；跨篇分工见 §1.4）。
> **教学目标**：①三张表各回答什么问题，x86 与 aarch64/riscv64 的承载物对照（§1.1、§1.2）；②`kmain` 与 `cstart` 的调用序列与不可交换的次序（§2.1、§2.2）；③GDT/IDT/TSS 的装填与 `prot_load_selectors` 的指令序列（§2.3–§2.5）；④重建页表的理由，以及 64 位下为什么不再重建（§2.6、§3.5）；⑤64 位下三张表各自还剩多少语义、Rust 如何承载（§3.1–§3.7、§4.2–§4.6）；⑥真机 `test-protection` 三架构的判据与诊断章的使用（§5.2、§6）。
> **后续**：`04-platform-discovery.md`（平台参数发现）；`05-clock-interrupt-init.md`（时钟与中断控制器初始化——`cstart` 的后半段）；更远的分工表见 §1.4 与 §7。

上一篇结束时，CPU 已经在自己的高半地址上、在自己的栈上执行 `kmain`。这一篇从 `kmain` 的第一条指令开始，讲它做的第一件实质工作：把"当前特权级、异常入口、陷入内核用哪根栈"这三份答案从引导方手里接过来。三份答案在 x86 上落在三张表里，接管动作是一串只有 ring0 能执行的指令。接完之后，内核才第一次拥有"出事能被自己接住"的能力。

---

## 1. 概述

### 1.0 本章导读

#### 学习目标

读完本章能：

- 说出 02 篇交出的状态：GDTR/IDTR 仍指向引导方留下的表，TSS 还没有内核自己的版本。
- 分清三张表（GDT、IDT、TSS）与四类入口，知道 `kmain` 阶段 B 的第一件事为什么是 `init_protection`。
- 解释“第一次行使特权”指哪一次动作（把三张表交给 CPU 的装载指令），以及它为什么是控制权转移而不是普通执行。

#### 前置概念

- 02 篇的四个时刻与 J1–J4；知道内核已经运行在高半、分页已开启。
- x86 的保护模式直觉：段描述符、门描述符、TSS 各自回答什么问题；不要求会手写描述符位。

#### 最小示例

```bash
# 内核侧的保护结构入口；阶段 B 的第一件事（§1.2）
$ grep -n 'fn init_protection' os/kernel/src/lib.rs
# kmain 的 C 入口；五步顺序的起点（§2.1）
$ grep -n 'void kmain' minix3/minix/kernel/main.c
```

第一条给出 Rust 侧接管点，第二条给出 C 侧顺序原型；三张表的“谁提供、谁接管”都围绕这两处读。

#### 失败对照

如果在 `init_protection` 之前触发异常或 NMI，CPU 会按引导方留下的 IDTR 递送：门里指向的是固件代码，或者门根本不在内核可访问的地址上，第一次异常就变成三重故障。危险不在“表不存在”，而在“表还属于别人”。

#### 小结

- 02 篇交出的是“能跑但规则仍借自引导方”的状态；三张表必须由内核重新建立并装载。
- `kmain` 阶段 B 的第一件事是 `init_protection`，之后才允许时钟与中断控制器动作。
- “第一次行使特权”是三张表从引导方交给 CPU 的那串装载动作（`lgdt`/`lidt`/`lldt`/`ltr`）；此后 CPU 的异常/中断/CPL3 入口才按内核的表解释。

#### 术语

- **GDT / IDT / TSS**：全局描述符表、中断描述符表、任务状态段；分别承载段、门与内核栈。
- **DPL**：描述符特权级；用户段与内核段的 DPL 不同。
- **cstart**：C `kmain` 中建立保护结构并继续初始化的序列。
- **第一次行使特权**：`lgdt`/`lidt`/`lldt`/`ltr` 把 GDT/IDT/TSS 的控制权交给 CPU 的那串动作；不是第一次异常本身。

### 1.1 02 篇交出的状态：三份答案仍由引导方提供

本文的“引导方”指进入内核前提供表的固件/GRUB/boot-shim；“装载方”在 §2.1 谈 BSS 清零时指同一方。跳转完成时，CPU 的 GDTR/IDTR 仍指向引导方留下的表；`02-higher-half-kernel.md` §4.4 用一张状态表把这一刻记成“保护结构未建立”，接管它是 `kmain` 的 **阶段 B 第一件事**（依据是该篇引用的 `lib.rs` 的 [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection) 文档注释）。这一行更精确的读法是：危险不在此刻，而在这些表的归属。

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

### 1.2 三张表与四类入口

四类入口落在三张表与系统调用路径上，每张表的装填方式由它回答的问题决定：

| CPU 需要知道的事 | C i386 的承载物 | Rust x86-64 的承载物 | aarch64 的承载物 | riscv64 的承载物 |
|---|---|---|---|---|
| 当前特权级与段可访问性 | GDT 段描述符（DPL）；段基址/限长 | GDT 仍保留 DPL，长模式段基址恒 0；访问控制主要靠页表 | 由 CPU 的异常级别决定（EL0/EL1），页表项权限位承载访问控制 | 由 CPU 的模式决定（U/S），页表项权限位承载访问控制 |
| 异常/中断的入口 | IDT 的 8 字节门（每向量一条） | IDT 的 16 字节门（每向量一条，含 IST 字段） | `VBAR_EL1` 指向的定长向量表 | `stvec` 指向的向量（Direct 模式；见 §4.6） |
| 系统调用入口 | SYSENTER/SYSCALL MSR 与 `int $0x20/$0x21` 软中断门（十进制 32/33；libc 主路径，后者是 IDT 门） | SYSCALL/SYSRET + `LSTAR`，另装 32–35 软中断门 | `svc` 经 `VBAR_EL1` | `ecall` 经 `stvec` |
| 陷入内核用哪根栈 | TSS 的 `sp0`（32 位 TSS） | TSS 的 `sp0`（64 位 TSS，另加 IST 数组） | EL1 的当前栈指针（EL1h）；栈基址记在 `TPIDR_EL1` | `sscratch` 交换（硬件不换栈，由入口第一条指令交换） |

C 的 earm-32 与 Rust 的 aarch64 是同一问题的两代载体：§2.7 讲 earm 的 `write_vbar`，§4.6 讲 aarch64 的 `VBAR_EL1`/`TPIDR_EL1`；两者回答同一问题，不要求寄存器名相同。

三张表的一个差别是：§1.1 的 ② 是逐条目不同的（x86 上 256 个表项，当前填 39 条门，每类事件一条），§1.1 的 ① 和 ③ 各只有一套（DPL 存在少数几个描述符里、`sp0` 每 CPU 一个字段）。后文 §2.4、§2.5 的装填方式不同正是这个差别带来的——IDT 要按向量逐条建，GDT 与 TSS 只需建少数几条描述符与一个结构体。

§1.1 的 ③ 还带一个"内核态里又出事用哪根栈"的子问题，三架构的答法同样不同：x86 交给 IST（NMI 与 #DF 各有一条专用栈，64 位新增见 §3.1、门里的 IST 见 §4.4）；aarch64 不换栈，继续用 `SP_EL1`；riscv64 也不换栈，用当前 `sp`。陷阱入口如何判断"这次进来的是用户还是内核"（riscv64 用两条入口路径按 `stvec` 切换，进自用户态的那条无条件交换 `sscratch`）属异常/中断与 trap 入口的主题（`14-exception-interrupt.md`，异常与中断篇）。

x86 上还有一张表与本节的三张并列出现：LDT。`GDT` 里为它留了一个槽，装载路径上也有一条 `lldt`（`i386/protect.c` 的 [`prot_load_selectors`](minix3/minix/kernel/arch/i386/protect.c:prot_load_selectors)）。它不回答上面四类入口中的任何一类，本篇 §2.3 按源码如实处理。

本篇的"保护结构"一词指上面三张表/系统调用路径与它们回答的四类入口。它不含页表：页表已经由 01/02 篇建好并在运行，本篇的 `prot_init` 里确实还有一段重建页表的代码（§2.6），RFC 式的说法是"保护结构初始化顺手把引导页表换成正式版"，C 的写法把这两件事放在同一个函数里；Rust 拆到两个阶段（§3.5）：页表在 `arch_boot_impl` 建一次，`init_protection` 不再重建。

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
| 平台的参数发现（ACPI/设备树分支、平台描述符） | `04-platform-discovery.md` |
| 时钟与中断控制器的初始化、中断使能时序 | `05-clock-interrupt-init.md` |
| 页表的多级结构、页表接口与 Direct Map 的完整语义 | `rewrite-notes/02-stage-vm/07-pagetable-struct.md`；01 篇 §2.6 已用到的部分不重复 |
| 异常处理体、trap frame 布局与"用户→内核→用户"的完整寄存器保存/恢复 | 异常/中断与 trap 入口主题（`14-exception-interrupt.md`）与切用户篇（`10-switch-to-user.md`）；本篇只到入口就位与 `sscratch` 的栈交换契约（§4.6 讲契约，不讲完整往返） |
| 系统调用的分派与各调用语义 | syscall 分派主题（`13-syscall-dispatch.md`，系统调用分派篇），本篇只到"入口地址与段选择子写进 MSR"为止 |
| 进程表、boot 进程装载、特权结构 | 进程初始化与启动收尾主题（`06-proc-init-boot-proc.md`，进程表与 boot 进程篇；`kmain` 的后续 Phase） |
| SDM/APM 的描述符逐位格式全表、ARM 与 RISC-V 的特权规范 | 外部资料；本篇只解释代码实际使用的那几位 |

可以安全跳过的一条：描述符的位级格式（`segdesc_s` 的 `base_low`/`base_middle`/`base_high` 与 granularity 字节）不必背下来。本篇只解释三处用到它的代码（S 位的含义、DPL 占哪两位、限长的"0 表示 4G"约定），其余位在代码里各有宏名，读到再看。

保护结构不含页表；页表由 01/02 篇建好并在运行，§3.5 只解释为什么 Rust 不再重建它。

---

## 2. C 源码分析

### 2.0 本章导读

#### 学习目标

读完本章能：

- 复述 `kmain` 入口的五步，以及 `cstart` 的调用序列与顺序不变量。
- 说出 `prot_init` 的五类动作、`tss_init` 的一条描述符与一个结构体、`prot_load_selectors` 的生效指令。
- 解释 C 为什么在 `cstart` 里重建页表，以及 ARM32 的 `protect.c` 与 x86 版的差异。

#### 前置概念

- 第 1 章的三张表与“第一次行使特权”。
- x86 保护模式的段选择子、门描述符与 `ltr`/`lgdt`/`lidt` 的基本作用。

#### 最小示例

```bash
# C 的 kmain 入口与 cstart 调用点（§2.1）
$ grep -n 'void kmain\|cstart()' minix3/minix/kernel/main.c
# prot_init 的 C 实现；五类动作的起点（§2.3）
$ grep -n 'void prot_init' minix3/minix/kernel/arch/i386/protect.c
```

第一条给出 `kmain` 到 `cstart` 的边界，第二条给出保护结构建立的 C 侧原型；顺序不变量都围绕这两个函数读。

#### 失败对照

如果 `cstart` 把 `prot_load_selectors` 排到门装载之前，CPU 会用尚未生效的 GDT 选择子继续执行：`lgdt` 之后取指仍使用缓存的旧 CS 描述符，所以取指轴暂时安全；真正的 #GP 出现在远跳重载 CS 之后（新描述符位错），或 `ltr` 对空槽当场报 #NP/#GP。若 `tss_init` 排在 `ltr` 之后，`ltr` 读到的描述符通常是空槽，失败点是 `ltr` 本身；只有该槽残留一条可用 TSS 描述符时，才会潜伏到第一次 CPL3→CPL0 在 `sp0=0` 上取栈。顺序不是风格，是 CPU 状态机的前置条件。

#### 小结

- `kmain` 先做准备，`cstart` 依次完成保护结构、时钟、中断与架构初始化。
- `prot_init` 建立 GDT/IDT/TSS，`prot_load_selectors` 让表生效，`tss_init` 的产物必须在使用前装好。
- C 在 `cstart` 里重建页表有历史原因；Rust 侧把这一步淘汰，但保留了它服务的读者。

#### 术语

- **`cstart`**：`kmain` 中从保护结构到架构初始化的调用序列。
- **`prot_init`**：C 的保护结构建立函数；填 GDT/IDT/TSS 并装载。
- **选择子**：GDT 槽位的索引加 RPL；`lgdt` 之后才能按新表解释。
- **`sp0`**：TSS 里的 ring0 栈指针；第一次 CPL3→CPL0 用它取栈。

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

第二个动作的两行 `memcpy` 不是自拷贝：`pre_init.o` 经 `objcopy --prefix-symbols=__k_unpaged_` 改名后进入链接，它里的 `kinfo`/`kmessages` 落在 `.unpaged_bss`，是引导期暂存副本；`main.c` 里的 `kinfo`/`kmessages` 是 `usermapped_data.c` 定义的 `.usermapped` 最终全局，`kmain` 的 `local_cbi` 指向 `pre_init` 返回的 `.unpaged_bss` 暂存副本，`memcpy` 把它搬进最终全局。两行是“暂存副本 → 最终全局”的搬运：把 `pre_init` 填好的启动信息搬进 `.usermapped` 副本，之后 VM 等读者才看得到。Rust 侧的情况不同——启动信息由 boot-shim 引用交接，`kmain` 落 `.bss` 副本的故障记录见 §4.1。

```c
/* 教学简化（只保留与副本身份关系有关的四行；函数体省略），四行证据的原文与归属见下表。 */
return &kinfo;                 // pre_init.c 收尾
push %eax                      // head.S：把 pre_init 的返回值压给 kmain
#define kmess kmessages        // glo.h
cbi->kmess = &kmess;           // pre_init.c
```

| 证据行 | 证明什么 |
|---|---|
| `return &kinfo;` | `pre_init` 返回的是它自己 `.unpaged_bss` 里的 `kinfo` 地址 |
| `push %eax` | `kmain` 的形参 `local_cbi` 就是这个地址 |
| `#define kmess kmessages` | `kinfo.kmess` 指向 `pre_init` 的 `kmessages` 缓冲区 |
| `cbi->kmess = &kmess;` | 暂存副本里记录的是暂存缓冲区的地址 |

第三步识别板子：`machine.board_id = get_board_id_by_name(env_get(BOARDVARNAME))`。它读的是启动参数里的环境变量，注释写着"We have done this exercise in pre_init so we expect this code to simply work!"——同一件事在 `pre_init` 里已经做过一次，这里只是把结果存进 `machine`。紧随其后的 `#ifdef __arm__` 串口初始化说明为什么 ARM 侧必须早于任何输出：串口没配好之前，调试输出无处可去。

第四步 `kernel_may_alloc = 1` 需要对着两份对象读。`glo.h` 的 `EXTERN` 宏在定义 `_TABLE` 的那个编译单元（`table.c`）里展开成真正的定义，其余单元展开成 `extern`；所以普通副本由 `table.c:30` 的宏展开产生，不出现在源码字面里（下面的 grep 因此只列声明与 `pre_init.c` 的显式定义），而 `pre_init.c` 的 `int kernel_may_alloc = 1;` 经改名只服务未分页副本（`pg_utils.o` 的 unpaged 一侧）：

```
$ grep -rn "int kernel_may_alloc" minix3/minix/kernel/
minix3/minix/kernel/glo.h:76:EXTERN int kernel_may_alloc;
minix3/minix/kernel/arch/earm/pre_init.c:38:int kernel_may_alloc = 1;
minix3/minix/kernel/arch/i386/pre_init.c:33:int kernel_may_alloc = 1;
```

也就是说，普通副本（`kmain` 与 `bsp_finish_booting` 使用的那一份）进入 `kmain` 时是 0，`kmain` 这一行是把它置 1 的唯一动作，是承重赋值而不是复述；未分页副本在 `pre_init` 阶段已经是 1，只服务 `pre_init` 自己的 `pg_utils` 路径。真正会翻转普通副本的地方在 `kernel/main.c` 的 [`bsp_finish_booting`](minix3/minix/kernel/main.c:bsp_finish_booting)（置 0，注释"Kernel may no longer use bits of memory as VM will be running soon"）。这个标志的读者是物理内存分配函数：

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

第五步 `assert(sizeof(kinfo.boot_procs) == sizeof(image)); memcpy(kinfo.boot_procs, image, ...)` 把链接期就存在的 boot image 表 `image[]` 拷进启动信息。`image[]` 是 [`kernel/table.c`](minix3/minix/kernel/table.c) 里的纯静态表（进程号与名字）；模块的 `start_addr`/`len` 稍后由 `kmain` 在 `cstart` 之后的启动循环按 `kinfo.module_list` 回填（`ip = &image[i]`，[`kernel/main.c`](minix3/minix/kernel/main.c) 的 `ip->start_addr = mb_mod->mod_start;`），循环读写的是 `image[]` 本身。第一次拷贝在循环前给 `kinfo.boot_procs` 留一份静态快照；循环与第二次拷贝都不读它；启动循环（`main.c:171`）读 `image[]`，`proc_init`（`main.c:157`）不读它，所以真正被消费的是第二次拷贝。循环结束后，同文件的第二次 `memcpy(kinfo.boot_procs, image, ...)` 把填好 `start_addr`/`len` 的 `image[]` 同步给 `kinfo.boot_procs`；这份副本的读者是用户态 VM（`servers/vm/main.c` 的 `kernel_boot_info.boot_procs`），不是 `proc_init`。

**次序承重**：步骤 2 必须早于 `cstart`（`prot_init` 读 `kinfo`）；步骤 3 依赖步骤 2（`env_get` 读 `kinfo.param_buf`）；步骤 5 在启动循环读 `image[]` 之前完成即可，`proc_init` 不读 `boot_procs`。

**把第二步的两行 `memcpy` 删掉会怎样**：最终 `.usermapped` 全局保持全零，`module_list` 与 `kmessages` 丢失，`pg_identity` 的 `assert(cbi->mem_high_phys)`（`i386/pg_utils.c:170`）会在 `cstart`→`prot_init` 链上先炸；不是“切不出后果”。**「先炸」的排序没有取证**：`main.c:133` 的 `env_get(BOARDVARNAME)` 早于 `main.c:147` 的 `cstart()`，也是 `kinfo` 的消费者，它在 `.usermapped` 全零时的行为（返回空串、还是读到坏指针）本篇没有核对，所以“第一个失败点是 `pg_identity` 的断言”目前只是推断，不是实测顺序。Rust 侧对应的是另一条故障：启动信息由 boot-shim 引用交接，`kmain` 若不把它落进全局副本、继续用入口参数引用，参数引用被编译器 spill 到内核栈后读回会变成 `.rdata` 的 ASCII 垃圾（§4.1）。

### 2.2 cstart 的调用序列与顺序不变量

`kmain` 的一个早期动作是 `cstart()`；它返回后 `kmain` 继续执行 BKL 加锁、`proc_init`、boot image 循环、`arch_post_init`、`memory_init`、`system_init`、`smp_init`、`bsp_finish_booting`。`cstart` 把保护结构、时钟软件状态、中断控制器、架构初始化串成一条固定序列，中间穿插启动环境变量的读取（`kernel/main.c` 的 [`cstart`](minix3/minix/kernel/main.c:cstart)）：

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

从这一步开始，本篇只展开 `prot_init()`（§2.3–§2.6），另外三次调用各归他篇。它们的顺序属于本篇，但结论不是"每个调用都依赖 `prot_init`"——下表逐条说明依赖强度，只有 `arch_init` 有硬依赖：

| 调用 | 依赖前一步的哪样产物 | 把它提前/推后，最先坏的对象 |
|---|---|---|
| `prot_init()` | 无（函数自己完成全部前提） | 它是后面所有步骤的前提，无法提前 |
| `init_clock()` | 无硬依赖；它只初始化 `kclockinfo`/`kloadinfo` 并读 `hz` 环境变量（[`init_clock`](minix3/minix/kernel/clock.c:init_clock)） | 与 `prot_init` 可交换；它的硬件编程不在这里（见下） |
| `intr_init(0)` | 不依赖 IDT；它只编程 8259 并屏蔽源。放在 `prot_init` 之后：先让异常入口就位，再动控制器 | 提前到 `prot_init` 之前不会新造可屏蔽中断窗口（IF=0）；变的是"控制器已编程、异常入口仍是引导方的"这一段，与 `intr_init` 本身无关 |
| `arch_init()` | 需要保护结构、控制器都已就绪 | 提前：它里面还会再动一次 TSS 与栈（见 §2.4 末尾） |

`arch_init` 的函数体（`i386/arch_system.c`）做 TSS 二次调用、串口/ACPI/APIC 初始化；在 `USE_APIC && !CONFIG_SMP` 分支它还会调 `apic_single_cpu_init()`，后者编程 LAPIC 并 `apic_idt_init` + `idt_reload` 改写 IDT。所以在 APIC 构建里它会初始化控制器本身；只有 PIC 构建里“控制器已就绪”的前提才完全来自 `cstart` 的调用次序。

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

第三段填 GDT 的四个段描述符。两个内核段 DPL=0、两个用户段 DPL=3（`INTR_PRIVILEGE` 与 `USER_PRIVILEGE`，[`include/archconst.h`](minix3/minix/kernel/arch/i386/include/archconst.h) 里分别是 0 与 3；访问字节的位序在这里用到两处——S 位是第 4 位，区分系统描述符与代码/数据段描述符；DPL 占第 5–6 位，这两个值就填在那一对位上）；段基址全 0、限长取满——32 位下段式隔离已经不用，段描述符在这里的作用是承载 DPL 与"这是代码段还是数据段"。这四个描述符的装填函数 `init_codeseg`/`init_dataseg` 以及它们的公共底座 `sdesc` 是 `prot_init` 的直接被调者，锚点见 `i386/protect.c` 的 [`init_codeseg`](minix3/minix/kernel/arch/i386/protect.c:init_codeseg)、`:init_dataseg`、`:sdesc`。段式隔离退出之后这张表仍然必须在场，理由不在段而在 TSS：`ltr` 装的是选择子，CPU 与后续转场都要经 GDT 里的描述符才能找到 TSS 结构体（§2.4），所以段的语义可以被削到只剩 DPL，GDT 本身不能退役。

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
| `cs` | `KERN_CS_SELECTOR`（仅硬件任务切换时由 CPU 装载） | 无读者；门转场用门里的选择子，不用这个字段 |
| `ds`/`es`/`fs`/`gs` | `KERN_DS_SELECTOR` | 无读者：Minix3 不用硬件任务切换，转场只由 CPU 读 `ss0`/`sp0` |
| `iobase` | `sizeof(struct tss_s)` | 高于 TSS 限长（由 `init_param_dataseg` 按 size 折出的限长是 `sizeof - 1`，即恰好 +1 字节）即"没有 I/O 位图"；默认拒绝 CPL > IOPL 的 `in`/`out`，但 `enable_iop`/`SYS_IOPENABLE` 可授予 IOPL=3（`protect.c:51` 写 `psw |= 0x3000`），所以不是绝对全部被拒 |
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

这一段是"每次切回用户态之前，把当前进程指针写到 `sp0` 指向的字"。第二个字由 `tss_init` 自己写：`*((reg_t *)(t->sp0 + 1 * sizeof(reg_t))) = cpu`，注释说明用途——"so we know on which cpu is this stack in use when we trap to kernel"。这个字的读者是 `cpuid` 宏：它把当前帧地址向上取整到 `K_STACK_SIZE` 边界、再读边界下的一个字（[`include/arch_smp.h`](minix3/minix/kernel/arch/i386/include/arch_smp.h) 的 `#define cpuid`），对跑在 `k_stacks` 数组上的 CPU，那个字正是 `sp0 + sizeof(reg_t)`；[`i386/arch_smp.c`](minix3/minix/kernel/arch/i386/arch_smp.c) 的注释写着它的适用前提（BSP 早期 still run on the boot stack and we cannot use cpuid）。SYSCALL 入口读的是同一个栈顶区的第一个字——`sp0` 处的进程指针，不是这个 CPU 号：

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

`tss_init` 的调用跨度可以用末尾的 grep 核：i386 在 `prot_init` 里用 `&k_boot_stktop` 调它，而 `arch_init` 里还有第二次调用（非 SMP 构建，参数是 `get_k_stack_top(0)`），SMP 构建则在 `smp_init` 的路径上对所有 CPU 各调一次（`i386/arch_system.c` 的 [`arch_init`](minix3/minix/kernel/arch/i386/arch_system.c:arch_init)、`i386/arch_smp.c` 的 [`tss_init_all`](minix3/minix/kernel/arch/i386/arch_smp.c:tss_init_all)）。两次调用的差别在参数：第一次指向 `.bss` 里那块固定的 `k_boot_stack`，第二次指向 `k_stacks` 数组里按 CPU 编号切分的两页式栈（[`include/arch_proto.h`](minix3/minix/kernel/arch/i386/include/arch_proto.h) 的 `get_k_stack_top` 宏）。`tss_init` 的返回值（`SEG_SELECTOR(index)`）在 C 树的五处命中全部被丢弃（四处调用 + 一处宏内复用；复算：`grep -rn "tss_init(" minix3/minix/kernel/ | grep -v "int tss_init"` 命中五处——`arch/earm/arch_system.c:113`、`arch/i386/protect.c:338`、`arch/i386/include/arch_smp.h:18`（宏内）、`arch/i386/arch_smp.c:272`、`arch/i386/arch_system.c:256`）；TR 装的选择子在 §2.5 由 `TSS_SELECTOR(booting_cpu)` 现算，不经过这个返回值。这条跨度说明"TSS 初始化"不是一个一次性动作：`prot_init` 先给它一个能用的临时栈，运行期栈的登记在后续步骤完成。本篇只把这条线索指出来，`arch_init` 与 `smp_init` 的内部不在这里展开。

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

`ltr` 还有一条容易被忽略的写回：装载成功时，CPU 会把该 TSS 描述符的 type 从 1001（可用）置为 1011（busy），改的是内存里的 GDT 表项，不是 TR 里的私有副本。因此同一个实例第二次调 `load()` 会在 `ltr` 处撞上 busy 描述符而 #GP；换一个实例装载则会静默换掉 GDTR/TR，留下两套保护状态各自以为自己生效。Rust 侧的 [`fn load`](os/arch/src/arch/protection.rs:trait ProtectionArch) 把「每个 CPU 的常驻实例只装载一次」写进 trait 契约；[`fn load_with_tss`](os/arch/src/x86_64/protection.rs:fn load_with_tss) 在 `lgdt` 之前读目标槽的 access 字节，命中 busy 就带名停机，把无上下文的 #GP 换成 "load called twice: TSS descriptor busy"。

动作的次序有依赖，不是任意排列：

1. `x86_lgdt(&gdt_desc)`——GDTR 指向新表。此后 CS 仍持有引导方的旧选择子，CPU 继续按**已缓存的旧代码段描述符**取指（Rust 侧同一事实的记录见 `os/arch/src/x86_64/protection.rs` 的 "continues using the old CS descriptor"）。这个中间态在**取指轴**上安全，因为缓存描述符在重载前一直有效；但**异常递送轴**在这个窗口内不设防：IDTR 仍指向引导方的 IDT，而 GDTR 已换——若此窗口内来一次异常，CPU 按旧 IDT 递送，门选择子却要经新 GDTR 查表，正是 §3.2 反事实的错配方向。所以这一步的「安全」只对取指成立，窗口终点也不是 `prot_init` 入口而是其内部的 `idt_reload`（第 3 步）。直到第 6 步远跳，CS 才换成新表里的内核代码段选择子。

   §3.2 的反事实在**取指轴**上不可类比：那里**门表已换、段表未换**，门里的选择子是按新表写的，CPU 却仍用旧 GDTR 去查它；本处取指轴是"CS 的缓存描述符 vs GDTR"，被缓存描述符兜住。但**异常递送轴**与 §3.2 同型：旧 IDT 的门选择子经新 GDTR 解析，窗口终点是 `idt_reload`，这一段仍要防。

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

两张表合计 39 条门——这两张表就是 `idt_init` 的全部填充源（`idt_copy_vectors_pic` 转发 `gate_table_pic`，`idt_copy_vectors` 收 `gate_table_exceptions`），39 即 PIC 路径、`prot_init` 完成时 IDT 上被置为 present 的全部门（APIC 路径还会加 IOAPIC 门，见 §3.7；其余表项保持非存在）。`gate_table_pic[]` 的 16 条由 `VECTOR(irq)` 宏算出向量号（[`include/interrupt.h`](minix3/minix/include/arch/i386/include/interrupt.h)：`((irq < 8 ? IRQ0_VECTOR : IRQ8_VECTOR) + (irq & 0x07))`，`IRQ0_VECTOR`=0x50、`IRQ8_VECTOR`=0x70），全部 DPL=0。`gate_table_exceptions[]` 的 23 条里：19 条 CPU 异常（向量 0–14 与 16–19，跳过保留的 15），4 条软中断（32/33/34/35，即 0x20–0x23）。其中 DPL=3 的只有四条软中断与两条异常——`breakpoint_exception`（向量 3）与 `overflow`（向量 4）；其余异常 DPL=0。这张表是"用户态能不能用 `int $n` 主动触发某个向量"的依据，读者可以对着 `gate_table_exceptions[]` 的每一行核一遍 `privilege` 列（复算：`awk '/gate_table_exceptions\[\]/,/};/' minix3/minix/kernel/arch/i386/protect.c | grep -c 'USER_PRIVILEGE'` 输出 6——四条软中断与 breakpoint/overflow 两条异常）。

**把 `x86_ltr(TSS_SELECTOR(booting_cpu))` 提到 `x86_lgdt(&gdt_desc)` 之前会怎样**：TR 会被装上"用旧 GDT 解释的槽号 6"——引导方表里槽 6 是另一条描述符（或非存在）。`ltr` 本身就要求选择子指向一个 TSS 类型的系统描述符，这一条很可能当场 #GP；即使侥幸通过，随后的第一次 ring3→ring0 转场会按 TR 指向的描述符去取 TSS 基址，读到的是旧表里那个描述符的基址/限长，`sp0` 从那里读出来。后果落在"CPU 转场后压栈的目标地址"上。依据：`prot_load_selectors` 的调用次序、`:tss_init` 只往 `gdt[index]` 写而不写 TR、`prot_init` 的注释说明旧选择子仍在用。

### 2.6 重建页表：四个调用、一条理由与一条约束、两份 vir2phys

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

`pg_clear()` 清空的是当前对象里的 `pagedir`。`pg_utils.c` 被编译两份：`unpaged_pg_utils.o`（改名、`.unpaged_bss` 的 `pagedir`，`pre_init` 使用）与普通 `pg_utils.o`（`.bss` 的另一个 `pagedir`，`prot_init` 使用）。`prot_init` 的清空/建表/装载都作用在普通副本上；`pg_load()` 最后把这张新表的物理地址写进 CR3，从旧表切到新表——不是同一数组的原地重填。`pg_identity()` 把 1024 个页目录项全部填成"恒等映射"，每项 4MB：

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

两处细节可以自己核：恒等映射的权限位里带了 `I386_VM_USER`（对用户态可访问）与 `I386_VM_WRITE`——带 USER 位的原因 C 树没有注释，此处不替它编动机；可核的效果是这些条目都是 BIGPAGE，VM 接手根表时 `pt_init` 的复制循环跳过 BIGPAGE 条目（`vm/pagetable.c` 的 [`pt_init`](minix3/minix/servers/vm/pagetable.c:pt_init)），它们不会进入用户进程的页表；从 `mem_high_phys` 所在的 4MB 对齐块起，后面的目录项被标记为 `PWT|PCD`（直写、禁缓存）：比较用的是 4MB 掩码：`mem_high_phys` 未 4MB 对齐时，它所在的那最后一块真实内存也会被一起标记；对齐时被标记的是内存顶端之上的第一块。那些完全在内存顶端之上的块不是真内存；PWT/PCD 只改变缓存属性，不改变可访问性。`pg_mapkernel()` 把内核所在的虚拟区段映射到它的物理区段，每项带 `PRESENT|BIGPAGE|WRITE` 而不带 `USER`（`i386/pg_utils.c` 的 [`pg_mapkernel`](minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel)）。`pg_load()` 把页目录的物理地址写进 CR3：

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

两份实现的差别是"要不要减平移量"，选择哪一份由对象副本决定：未分页副本处于重定位前布局，内核符号等于物理地址，原样返回即可；普通副本处于高半，传入的内核符号要减掉 `_kern_vir_base - _kern_phys_base`。两份实现绑定到两份 `pg_utils`：未分页副本的引用经改名绑定 `pre_init.c` 的恒等版；普通副本绑定 `protect.c` 的减偏移版（`prot_init` 里的 `pg_load` 调的是后者）。`pagedir` 在两个副本里也各有一份：未分页副本在 `.unpaged_bss`（VMA = 物理地址），普通副本在 `.bss`（高半）。判据同时看对象副本与符号所在段，不是“函数在哪个时刻执行”。

现在可以回答"为什么重建"。源码给的理由在上面的注释里，一共两条，各自有明确的读者：

- "we can map in VM, and we no longer rely on pre-relocated data"——重建后的表是 VM 进程将要载入并运行的那张表（`i386/protect.c` 的 [`arch_boot_proc`](minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc) 把 VM 的 ELF 段直接映射进这张页表）。理由落在**映射拓扑**上：`pre_init` 那张表建立于重定位前的布局，VM 的段与正式 VA 布局都不在其中——注释里的 "pre-relocated data" 指的就是这批映射；重建后的表才是 VM 装载所需的那张。
- "Still need 1:1 for lapic and video mem and such"——恒等段此时仍未退位。LAPIC 的 MMIO、显存这类"地址就是物理地址"的对象要靠它访问，所以重建时必须立刻把它装回去。

**把 `pg_identity(&kinfo)` 从重建段去掉、或把重建整段删掉只留 `pg_load()` 会怎样**：前者，`pg_clear()` 把普通副本的新表清空，`pg_identity` 不装回恒等窗口，`pg_load()` 切 CR3 后第一个按物理地址访问的 LAPIC/显存缺页（故障地址是物理地址本身，如 `0xFEE00000` 一段）。后者更严重：普通副本的 `pagedir` 在 `.bss` 冷启动为零，`pg_load()` 切 CR3 后立即缺页/三重故障，不会“照常跑”；重建段的作用是给 VM 装载准备一张按正式 VA 布局建好的表，不是防重启。依据：`pg_clear` 的清零范围、`pg_identity` 的循环、`prot_init` 的两句注释与两份 `pg_utils` 对象。

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
| `tss_init` 的调用者 | `prot_init`（第二参 `&k_boot_stktop`，第一参是 CPU 序号 0；`arch_init` 与 AP 路径的第二次调用见 §2.4） | [`arch_init`](minix3/minix/kernel/arch/earm/arch_system.c:arch_init)（参数 `get_k_stack_top(0)`） |
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

三类的划分轴是"动作改变什么"，不是"可否回退"：第三类的 `pg_clear` 也改内存（与第一类重叠，按目的归此），而切 CR3 一旦生效就不可回退——**第二类与第三类都含不可逆动作**；而"生效"不是函数的最后一步——`prot_load_selectors()` 之后还有页表重建与标志位。这个次序本身就是一条不变量：读 `prot_init_done` 的路径（AP 启动、后续初始化）必须在表已装载之后才看到它。`prot_init_done` 的读者在 `i386/arch_smp.c` 的 AP 启动路径里（`assert(prot_init_done)`）：

```c
/* 代码引用，minix3/minix/kernel/arch/i386/arch_smp.c（AP 路径对 prot_init_done 的读）。 */
assert(prot_init_done);
```

**把 `prot_init_done = 1` 提到 `prot_load_selectors()` 之前会怎样**：AP 启动路径（`mpx.S` 的 AP 入口也要跑同一个 `prot_load_selectors`）与这个标志的读者之间不再有序。C 侧的 AP 启动序列中（Rust 侧对应 `init_ap` 见 §4.3、`smp_ap_tail` 见 §4.7 的 smp_init 调用链）：若 BSP 在 `prot_load_selectors` 之前置位并放行 AP，AP 的 `ltr` 会按尚未装填的 TSS 槽取描述符，首次转场读 `sp0` 时落到未初始化字节。但该反事实的前提不成立：`prot_init_done` 的唯一读者 `copy_trampoline` 由 `smp_start_aps` 在 `smp_init` 内调用，而 `smp_init` 在 `cstart` 之后；AP 不会在 `prot_load_selectors` 之前被放行。

---

## 3. Rust 设计决策

C 章的各节留下了在 Rust 里必须重新回答的问题，§3.1–§3.7 逐一回答：三张表在 64 位下各自还有多少语义（§3.1）、两个职责为什么拆成两个 trait（§3.2）、表装好之后由谁保证它还在原地址（§3.3）、系统调用入口在长模式里怎么选（§3.4）、页表重建为什么不做了（§3.5）、`cstart` 一条序列怎么变成启动阶段（§3.6）、门什么时候可以装载（§3.7）。

### 3.0 本章导读

#### 学习目标

读完本章能：

- 说出 64 位下 GDT、TSS、LDT、IDT 与页表重建各自的去留。
- 解释为什么顺序依赖被拆成 `ProtectionArch` 与 `TrapEntryArch` 两个 trait。
- 说明“表装进 static 再 load”的地址稳定契约，以及系统调用入口为什么换成一条路径加内核标签。

#### 前置概念

- 第 2 章的 C 侧五类动作与顺序不变量。
- Rust 的 trait、`static` 与 `&mut` 基本语义；不要求读懂所有位操作。

#### 最小示例

```bash
# 两个 trait 的声明；顺序依赖被拆开的边界（§3.2）
$ grep -n 'pub trait ProtectionArch' os/arch/src/arch/protection.rs
$ grep -n 'pub trait TrapEntryArch' os/arch/src/arch/trap_entry.rs
# 表装载入口；地址稳定契约的落点（§3.3）
$ grep -n 'fn load_with_tss' os/arch/src/x86_64/protection.rs
```

前两条给出职责边界，第三条给出“装进 static 再 load”的实现位置；三者合起来回答“为什么不是把 C 的函数原样搬过来”。

#### 失败对照

如果表先 `load`、随后又被搬进另一个 `static`，GDT/IDT 里的基址仍指向旧地址：CPU 查表时读到的是搬走后的内存，第一次异常或 `ltr` 就会失败。地址稳定不是优化，是“表已经生效”之后的硬约束；把 `init` 改写为就地写全局槽能消掉重建，但会把三架构启动路径的签名翻新，代价与收益不成比例。

#### 小结

- GDT、TSS、IDT 保留但按长模式重排；LDT 与页表重建段淘汰。
- 顺序依赖拆成 `ProtectionArch`（建立与装载）与 `TrapEntryArch`（门与入口）两个 trait。
- 表装进 static 后再 load，地址稳定成为接口外的契约；系统调用入口用内核自己的标签，不探测用户能力。

#### 术语

- **`ProtectionArch`**：建立并装载 GDT/IDT/TSS 的 arch trait。
- **`TrapEntryArch`**：装门、配置系统调用入口与 per-CPU 数据的 arch trait。
- **地址稳定**：表装载后其内存地址不能再变；搬移会让描述符里的基址失效。
- **系统调用标签**：内核自己维护的入口选择子/锚点，不依赖用户段的能力探测。

### 3.1 64 位下三张表的去留（→§2.3、§2.4、§2.6）

| 项 | 选择 | 否掉什么 | 代价 |
|---|---|---|---|
| GDT 与段描述符 | 保留，槽位按长模式重排 | 直接搬用 C 的槽号 | 用户 CS/DS 位置变化，STAR 的取值随之变（§4.3） |
| TSS | 保留，换成 64 位布局（16 字节描述符 + `ist[7]`） | 32 位 TSS 布局 | 描述符占两个 GDT 槽；IST 数组要有可用的栈地址（§4.3）。IST 落专用栈是 [ARCH: IST 取代 i386 同栈嵌套]：C 的 32 位 TSS 无 IST 字段，NMI/#DF 在被中断栈上嵌套；长模式用 IST1/IST2 切专用栈，消费方从帧内保存的 RSP 定位原栈 |
| LDT | 淘汰 | `lldt` 与第 5 槽占位 | 长模式不使用 LDT（x86 GDT 无 LDT 描述符，`load_with_tss` 不执行 `lldt`）；C 的 LDT 占位没有对应物 |
| IDT | 保留，门换成 16 字节并增加 IST 字段 | 8 字节门 | 两条走 IST 的向量（NMI/#DF，其余向量 IST=0）的栈选择成为显式配置 |
| 页表重建段 | 淘汰 | 在 `init_protection` 里重建 | 重建所服务的读者改由 01/02 篇的建表满足（§3.5） |

TSS 的字段级去留（对 `include/arch_proto.h` 的 `tss_s`（`minix3/minix/kernel/arch/i386/include/arch_proto.h`） 的 27 个字段逐个处置）：

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

| 槽 | C（i386，`CONFIG_MAX_CPUS`=1） | Rust（x86-64，`MAX_CPUS`=32） |
|---|---|---|
| 0 | 空 | 空 |
| 1 | 内核 CS | 内核 CS |
| 2 | 内核 DS | 内核 DS |
| 3 | 用户 CS | SYSRET anchor（用户 CS 的副本，[`const GDT_USER_ANCHOR_INDEX`](os/arch/src/x86_64/protection.rs:const GDT_USER_ANCHOR_INDEX)） |
| 4 | 用户 DS | 用户 DS |
| 5 | LDT 占位 | 用户 CS |
| 6 起 | 每 CPU 一个 TSS 槽 | 每 CPU 两个槽（16 字节 TSS 描述符） |
| 表长 | `TSS_INDEX(CONFIG_MAX_CPUS)`（这里是 7） | `GDT_TSS_FIRST_INDEX + MAX_CPUS * GDT_SLOTS_PER_TSS`（6 + 32 × 2 = 70） |

第 3 槽的变化不止是槽号平移：64 位的用户段在这里多插了一个无人选择的副本。它存在的原因是 `sysret64` 取选择子的规则——返回时 CS 取 STAR 高 16 位加 16、SS 取加 8，因此 STAR 的高位要指向 anchor 槽本身——用户 CS 在它之后两格（`CS = STAR+16`、`SS = STAR+8`；源码注释把 anchor、用户 DS、用户 CS 这组连续三格写作 "THREE slots"）。§4.3 会把这条规则与一次真实的失败记录对上。

**只把 TSS 描述符的低 8 字节放进 GDT 映像（高半部分放进另一个不暴露的数组）会怎样**：长模式的 TSS 描述符是 16 字节，高 8 字节里存的是基址的 63:32 位——`ltr` 读到的基址高半为 0。内核在 0xFFFF_8000_... 一带的 TSS 结构体被解释成 0x0000_0000_xxxxxxxx 的物理地址，第一次 CPL3→CPL0 转场时 CPU 按这个基址读 `sp0`，读到的是低地址上的任意内存内容。Rust 侧对此留了记录：这个写法在早期版本里确实存在，因为那时"没有人跨过 ring"，缺陷是潜伏的（`protection.rs` 的 [`fn setup_tss_for_cpu`](os/arch/src/x86_64/protection.rs:fn setup_tss_for_cpu) 的注释）。

### 3.2 一条真实的顺序依赖拆出两个 trait（→§2.4、§2.5）

C 的 `prot_init` 内部有两个独立职责，切分轴是**入口机制 vs 访问控制**（`trap_entry.rs:7-21` 的文件头自述覆盖入口机制半边——IDT 与 SYSCALL MSR；访问控制半边的对应自述在 `protection.rs` 的 trait 注释里），不是状态载体：`ProtectionArch` 管"当前特权级与段可访问性 + 用哪根栈"，`TrapEntryArch` 管"出事跳到哪、系统调用从哪进"。两者都同时跨内存表（GDT/IDT）与寄存器（MSR/`TR`），所以按载体切分会自相矛盾。两者之间有一条真实的依赖：每个门的 `selector` 字段都写 `KERN_CS_SELECTOR`（`i386/protect.c` 的 [`int_gate`](minix3/minix/kernel/arch/i386/protect.c:int_gate)），所以门的可用性以 GDT 已被装载为前提。Rust 把这两个职责拆成两个 trait，并把顺序依赖写成接口契约（`protection.rs` 的 [`trait ProtectionArch`](os/arch/src/arch/protection.rs:trait ProtectionArch)、`trap_entry.rs` 的 [`trait TrapEntryArch`](os/arch/src/arch/trap_entry.rs:trait TrapEntryArch)）。

| 维度 | 单 trait 内 `#[cfg]` 分支 | 两个 trait（现码） |
|---|---|---|
| 加载顺序的表达 | 只能写在注释或运行期检查里 | 写在两个 trait 的文档契约里，调用点显式依次调用 |
| 实现体量 | 每个实现里混进另一职责的分支 | 每个实现只回答自己那组问题 |
| 单元测试 | 测"保护"要连带构造向量表 | [`protection.rs`](os/arch/src/x86_64/protection.rs) 与 `:trap_entry.rs` 各自可测 |
| 跨架构共性的表达 | 共性被分支淹没 | `ProtectionArch` 的接口只保留"栈 + 装载"的形状；特权级表示（`Privilege`/`PrivilegeLevel`）在当前实现中不存在——当前没有运行期消费者，等异常/中断篇 14 出现真实消费者时按差异设计 |

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

**把两个 `load` 合成一个函数、并在它内部先 `lidt` 后 `lgdt` 会怎样**：`lidt` 会让 IDTR 指向一张门里写满 `KERN_CS_SELECTOR` 的表，而此刻 GDT 还是引导方的那张——同一个槽号在两张表里的含义不同。在 `lidt` 与 `lgdt` 之间到达的异常，CPU 会用新 IDT 找到门、用门里的选择子去当前（旧）GDT 里查代码段，查到的要么非代码段要么非存在，异常递送的第一步就 #GP。依据：`TrapEntryArch` 的文档契约与 `int_gate` 里固定的内核 CS 选择子（`KERN_CS_SELECTOR`）。

### 3.3 表装进 static 再 load：地址稳定成为接口外的契约（→§2.4）

以下讨论只适用 x86-64（GDT/TSS/IDT 是内存表）；aarch64/riscv64 的保护状态是寄存器，不进入全局。`lgdt`/`lidt`/`ltr` 把表的地址写进 CPU 的寄存器。写进去的地址在此后一直生效，直到下一次装载；因此"表对象在装载期间不能移动、不能消失"是一条必须满足的前提。C 侧它自动成立：`gdt`/`idt`/`tss` 是文件级静态数组，地址从链接期起就是常量。Rust 侧它不自动成立：表被收进结构体，结构体沿值语义传递，移动与析构都会改变地址或让字节失效。现码的选择是把实例先搬进全局、再装载：

```text
/* 代码引用，os/kernel/src/lib.rs:fn init_protection（E1 注释原文）。 */
// E1: the instance's address changed when it moved into the global —
// the TSS descriptors embedded in the GDT still encode the pre-move
// stack-local bases. Rebuild them from the final addresses before
// ltr; otherwise the first CPL3→CPL0 transition (user-mode entry)
// reads a dead TSS (sp0 = 0 → page fault at VA -8 on the frame push).
```

四步完整代码见 §4.2；E1 的原文已在上一段引出，这里给的是四个候选的取舍。

四个候选被比较过：

| 候选 | 淘汰/选中 | 理由（落到具体后果） |
|---|---|---|
| 栈上构造，`load()` 后继续用局部变量 | 淘汰 | 函数返回即析构；GDTR/TR 仍指向已复用的栈内存，TSS 结构体的 `sp0` 变成栈上的旧字节 |
| 堆分配 + 泄漏（`Box::leak`） | 淘汰 | 引导期还没有通用堆；泄漏的分配在裸机环境下没有释放者 |
| 就地初始化：`init` 直接写全局槽（A-1） | 淘汰 | 要把 `init` 改写为 `&mut` 并删 `refresh_descriptors`，三架构 boot 关键路径签名翻新 + 约 20 个调用点（10 个单测 + 5 个载体 + lib.rs）；收益约 30 行，且与现形状一样靠纪律维持不变量，不是类型强制 |
| 全局 `SyncUnsafeCell<Option<...>>`，装载前搬入 | 选中 | 地址从搬入起稳定；accessor 在未初始化时 panic，给出确定的失败点 |

选中"全局搬入 + 重建"不是硬件必然，而是 Rust 值语义的代价：C 的 `gdt`/`idt`/`tss` 是文件级静态数组，天生就地（`protect.c:27`）；Rust 把手写表收进结构体后，结构体按值传递，搬移后表项里内嵌的地址就过期。IDT/TSS 的对称也说明这条边界：IDT 门项只存 handler 地址、不含表自身的地址，所以 `TrapEntry` 搬进全局后不需要 `refresh`；TSS 描述符内嵌 TSS 结构体地址，才需要重建。就地初始化（A-1）能消掉重建，但要把 `init` 改写为 `&mut` 并翻新三架构的 boot 关键路径，收益与成本不成比例；当前形状的搬移-重建窗口在 CLI 关闭、IDT 未装载的直线代码里，暴露面为零。

结构体骨架视图（表就在实例内部，这是"搬移会改变表地址"的根源）：

```rust
/* 签名示意（字段与类型逐字取自源码；省略方法与 impl 块），os/arch/src/x86_64/protection.rs:struct X86_64Protection。 */
pub struct X86_64Protection {
    gdt: [u64; GDT_ENTRIES],
    tss: [Tss64; MAX_CPUS],
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

`program_gs` 把该区的地址写进 `IA32_GS_BASE` 与 `IA32_KERNEL_GS_BASE` 两个 MSR（用户态尚未存在时两者同值，`swapgs` 无事可做）。两个栈字段由 `program_gs` 在 BSP/AP 启动时写入；`user_rsp` 由 SYSCALL 入口写（`trap_stub.rs:376` 的 `mov gs:{gs_user_rsp}, rsp`），入口对该字段是读写而非只读。C 侧的对应物是 `k_percpu_stacks[cpu]` 加 per-CPU 入口标签；Rust 把它换成"入口读 `gs:0x0`"，因此入口不再需要按 CPU 分身，一个标签全 CPU 共用。

**AP 起来之后不重写这四个 MSR 会怎样**：这些寄存器是 per-CPU 的，BSP 写过的值不会跟着 AP 走；该 CPU 的 LSTAR 仍是复位值 0（或引导方遗留值），它上面第一次用户态 `syscall` 就跳到地址 0。Rust 侧把这条需要写成了独立函数与注释：`values the BSP wrote via configure_syscall do not propagate to APs, and an AP without them faults on its first syscall`（`trap_stub.rs` 的 [`fn write_syscall_msrs`](os/arch/src/x86_64/trap_stub.rs:fn write_syscall_msrs)），AP 尾路径在启动时调用它。依据：该函数的注释与 [`smp.rs`](os/kernel/src/smp.rs) 的 AP 尾路径。

### 3.5 页表重建的淘汰与恒等段的继续服役（→§2.6）

| 项 | C | Rust |
|---|---|---|
| 谁建表 | `pre_init` 建一次（引导用），`prot_init` 重建一次（正式用） | [`fn arch_boot_impl`](os/kernel/src/lib.rs:fn arch_boot_impl) 一次建好恒等窗口、内核映射与 Direct Map 覆盖 |
| 保护结构初始化里是否碰页表 | 碰（`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`） | 不碰 |
| "仍需要 1:1"的消费者 | LAPIC MMIO、显存等按物理地址访问的对象 | 由 `arch_boot_impl` 的恒等窗口与 Direct Map 覆盖承担（01/02 篇已讲） |
| `kinfo.freepde_start` | `pre_init` 存值、32 位跨地址空间机制消费 | 已由 `01-boot-shim-bootstrap.md` §3.4.3 处置（`KernelInfo` 字段删除，GET_KINFO 报告位固定填 0，消费者由 Direct Map + 软件页表走查取代） |

`init_protection` 的函数体里没有任何 CR3/页表调用。可复现读数：

```text
$ sed -n '/pub fn init_protection/,/^}/p' os/kernel/src/lib.rs | grep -nE 'cr3|paging|pg_|PageTable|map_'
（无输出）
```

`IDENTITY_MAP_END` 通过 `use ... as` 转出 `BOOT_IDENTITY_MAP_END`（定义在 `os/arch/src/lib.rs:221`）；Step 1（02 篇 §4.3 的 boot 阶段 Step 1）的现行注释记录的是它与 C 的差异（“C maps 4 GiB with `pg_identity`…the Rust cross-image leg only needs…”），不再是 1024×4MB 全量映射。

**把 C 的页表重建照搬进 Rust 的 `init_protection` 会怎样**：C 的 `prot_init` 不是在原表上重填——`pg_utils.c` 编译两份，`pre_init` 在未分页副本的 `pagedir` 上建第一张表，`prot_init` 在普通副本的 `.bss pagedir` 上建第二张表并在末尾切 CR3（§2.6）。Rust 侧没有这两份静态表：正在使用的根表由 `arch_boot_impl` 建立、Direct Map 的叶子登记挂在它上面，清空会把已登记的叶子抹掉；页表由分配器按页给出。依据：`lib.rs` 的 [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection) 的函数体（无建表调用）与 `:fn arch_boot_impl` 的 Step 划分，以及 02 篇 §4.4 的状态表。

### 3.6 把 cstart 拆成两个里程碑（→§2.2）

C 的 `cstart` 把四次调用放在一个函数里；Rust 按"哪一步之后系统多出一种能力"拆开，并把 `kmain` 的主要工作划成 A–F 六个 Phase；下表七行覆盖 A–F 全部 Phase（B 因职责拆成两行）；A 的函数落点即 `kmain` 自身（§4.1），E 没有函数落点，系统调用分派归 `13-syscall-dispatch.md`，不在本篇。现行注释块给的划分是（[`lib.rs`](os/kernel/src/lib.rs) 里 `kmain` 的文档注释；每个 Phase 的可锚定落点：B 的 `lib.rs` 的 [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection) 与 `:fn init_clock_and_interrupts`、C 的 `:fn init_proc_and_boot`、D 的 `:fn init_post_and_memory`、F 的 `:fn bsp_finish_booting`，E 没有函数落点）：

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

`kmain` 文档注释里 阶段 B 的括注 “software only”（见上方节选，紧跟 `intr` 之后）按位置读像是在说 `intr`；按实现它只能指 `init_clock()` 那一步——`intr.init()` 明确会编程控制器。这条括注本身有歧义，实现为准——同一 Phase 的 `intr.init()` 会编程 LAPIC/IOAPIC 的控制器侧。时钟门的放行在 阶段 F 的 `bsp_finish_booting`（`boot_init_timer` 的 program→register→gate 三步，见 [`timer_irq_gate.rs`](os/arch/src/x86_64/timer_irq_gate.rs)）。

本篇展开 阶段 A 与 阶段 B 中的保护结构部分：

| Phase | C 的对应 | 本篇展开 | 归属 |
|---|---|---|---|
| A | `kmain` 前半段 | §4.1 | 本篇 |
| B（保护结构） | `cstart` → `prot_init` | §4.2–§4.6 | 本篇 |
| B（时钟与中断控制器） | `cstart` → `init_clock`/`intr_init`/`arch_init` | 不展开 | 时钟与中断主题（`05-clock-interrupt-init.md`；`arch_init` 的 TSS 部分见 §2.4 末尾） |
| C | `proc_init` + boot image 循环 | 不展开 | 进程表与 boot 进程主题 |
| D | `arch_post_init` + `memory_init` | 不展开 | 跨地址空间主题（`07-cross-space-init.md`） |
| E | `system_init`（Rust 侧由分派枚举承担） | 不展开 | syscall 分派主题 |
| F | `bsp_finish_booting` + SMP + `switch_to_user` | 不展开 | 启动收尾与 SMP 主题 |

两个里程碑的次序不可交换，理由落在"中断源"而不是"中断处理"：`init_clock_and_interrupts` 里控制器初始化结束时全部中断线是屏蔽的，次序的意义在于保护结构必须先就绪，这样控制器编程期间的总线异常也有可用的栈与门。可复现读数：`rg -n "fn mask_all" os/plat/src` 命中三架构实现；`rg -n "intr.init\\(\\)" os/kernel/src/lib.rs` 命中 `init_clock_and_interrupts` 的调用点（其注释写明 `mask_all()` 在 `init` 内部）。

**把 `init_clock_and_interrupts` 提到 `init_protection` 之前会怎样**：控制器编程期间若发生异常或 NMI（可屏蔽中断被 IF=0 挡在门外），CPU 会按尚未装载的 IDTR 递送：IDTR 仍是引导方的值，门指向固件代码，而表体所在内存在 `ExitBootServices` 后不再受保护。这是一个场景，不是两个——`KERN_CS_SELECTOR` 与 GDT 不匹配是 §3.2 的反事实（门表已换、段表未换），不在本处射程。依据：`lib.rs` 的 [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection) 的"must be the very first thing called in kmain"注释（指 阶段 B 层级）与 `:fn init_clock_and_interrupts` 里 `intr.init()` 的全屏蔽语义；`init_protection` 的注释与函数体已同步，§5.3 记录的是同步后的状态。

### 3.7 门在第一次 load 之前就有真入口（→§2.5）

C 把真实处理函数地址（`mpx.S` 里的标签）在填 IDT 时一次写入；Rust 侧把写门收成一个写点：`X86_64TrapEntry::init` 只建 256 条**不存在**的占位项（handler/DPL/IST/type 全 0），`install_idt_handlers` 在 `load()` 之前经 `set_handler` 一次写齐 41 条门。`set_handler` 是唯一写者：DPL 取自 `user_accessible(vector)`、IST 取自 NMI/#DF 表（2→IST1、8→IST2）、类型恒为中断门。安装后立即对 41 条门断言 DPL/IST/type，并核对 gate presence 与 stub 覆盖相等；不再有 init 元数据与 installer 的第二来源。

| 项 | 选择 | 否掉什么 | 代价 |
|---|---|---|---|
| 门入口的写入时机 | `load()` 之前全部写入 | "先装载、后补入口" | 需要在装载前验证覆盖（`gate_present` 的存在性检查） |
| 覆盖范围 | 41 个向量一次写齐 | 按需逐条 `set_handler` | 尚未有生产者的向量也预留门（与 C 的整表一次填一致；41 项之外的表项保持非存在） |
| IST 恢复 | `set_handler` 写 IST=0 后由安装器恢复 NMI/#DF | 让 `set_handler` 自己记得 IST | 多一次簿记 |

门的清单与 C 的差异用现算的读数对齐：

```
$ grep -n "const IDT_ENTRIES" os/arch/src/x86_64/trap_entry.rs
114:const IDT_ENTRIES: usize = 256;
$ sed -n '/const STUB_VECTORS/,/^];/p' os/arch/src/x86_64/trap_stub.rs | grep -cE '^\s+\(0x|^\s+\([0-9]+,'
41
```

41 = 18（异常，0–8、10–14、16–19，跳过长模式保留的 9/15）+ 4（软中断 32–35）+ 16（PIC 的 0x50–0x57 与 0x70–0x77）+ 3（0xF0 调度 IPI、0xF1 LAPIC 本地定时器、0xFF 伪中断）。C 侧的 39 = 16 + 23，其中 23 条门含向量 9（`COPROC_SEG_VECTOR`，[`include/archconst.h`](minix3/minix/kernel/arch/i386/include/archconst.h)）、不含向量 15。Rust 比 C 少 1 条异常（向量 9）、多 3 条 LAPIC/调度相关向量：41 = 39 − 1 + 3。这条对照只对 `prot_init`/`idt_init` 完成时成立；APIC/SMP 路径用 `gate_table_ioapic` 的 64 条门（0x50–0x8F）替换 16 条 PIC 门，再加 spurious/错误向量：39 − 16 + 64 + 3/5 = 90/92（3 与 5 的差来自 `CONFIG_SMP` 的 0xF1/0xF2）。Rust 侧 IDT 仍是 41 门：IOAPIC 只路由 16 条 ISA IRQ，pins ≥16 屏蔽，没有 64 门表；C 的 0xF0=timer/0xF1=sched，Rust 相反（0xF0=sched/0xF1=timer），对照时按各自编号读。

**先 `load()` 再装门入口会怎样**：窗口内到达的异常或中断会走门里偏移 0 的入口——在那个地址上取指，取到的要么是空内存要么是别人；`lidt` 只保证 CPU 找得到门，不保证门有效。Rust 把这条顺序写成纪律与检查：`install_trap_stubs` 在 `load()` 之前一次写齐 41 个入口；`gate_present` 提供"某向量是否已有门"的读数（`trap_entry.rs` 的 [`fn gate_present`](os/arch/src/x86_64/trap_entry.rs:fn gate_present) 的注释："the stub installer cross-checks its coverage against this before the table may go live via `load()`"——执行这条交叉核对的是宿主测试 `test_stub_coverage_matches_idt_gates`，见 §5.1；`load()` 本身不检查；真机载体 `test-protection` 在 `load` 之前调用 `install_trap_stubs`（§5.2），门里不是占位 0，说明 `load()` 本身不检查门有效性；§5.3 的补法是**新增装载前守卫**（在 `load()` 之前显式断言 `gate_present` 覆盖全部向量），不是在 `load()` 内部加断言）。依据：`lib.rs` 的 [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection) 里 `install_trap_stubs` 在 `load` 之前、[`lib.rs`](os/arch/src/lib.rs) 里 `install_trap_stubs` 的 x86_64 分支转发到 `trap_stub.rs` 的 [`fn install_idt_handlers`](os/arch/src/x86_64/trap_stub.rs:fn install_idt_handlers) 的遍历，以及 `:fn test_stub_coverage_matches_idt_gates`。

---

## 4. 实现详解

### 4.0 本章导读

#### 学习目标

读完本章能：

- 从 `kmain` 阶段 A 的落地跟到 `init_protection` 的逐步实现。
- 说出 x86_64 的 GDT/TSS 装填、IDT 门覆盖与系统调用 MSR 的写入次序。
- 对照 aarch64 的 VBAR/SP_EL1 与 riscv64 的 stvec/sscratch，指出三架构差异留在哪里。

#### 前置概念

- 第 3 章的两个 trait 与地址稳定契约。
- 能读 Rust 的 `static`、`&mut` 与内联汇编的寄存器操作数；不要求会写描述符。

#### 最小示例

```bash
# 保护结构的 Rust 入口；阶段 A 之后的接管点（§4.2）
$ grep -n 'pub fn init_protection' os/kernel/src/lib.rs
# x86_64 的 TSS 装载；busy 检查与 lgdt 的次序（§4.3）
$ grep -n 'fn load_with_tss' os/arch/src/x86_64/protection.rs
```

第一条给出实现入口，第二条给出“先查目标槽再 lgdt”的关键次序；本节的失败对照都围绕这条次序。

#### 失败对照

如果 GDT 槽位按 32 位布局填写，长模式的 TSS 描述符会占错槽：`ltr` 读到的 type 位不是可用（1001），或者描述符所在槽位错；如果描述符高半丢失，`ltr` 本身能过，故障会潜伏到第一次 CPL3→CPL0 转场读 `sp0`。如果 IDT 门在 `load()` 之前没有装齐，装载后的第一个异常跳到 0。两类故障都发生在“表已生效但内容不对”的时刻，诊断要读 GDTR/IDTR/TR 的现场值。

#### 小结

- 阶段 A 先把启动信息落进 `.bss` 并发布全局 `KernelInfo`，阶段 B 才建立保护结构。
- x86_64 的 `load_with_tss` 在 `lgdt` 前检查目标槽，命中 busy 就带名停机。
- 三架构的差异留在各自实现里：x86 的 MSR、aarch64 的 VBAR/SP_EL1、riscv64 的 stvec/sscratch。

#### 术语

- **`load_with_tss`**：x86_64 装载 GDT 与 TSS 的入口；先检查 busy 描述符。
- **STAR / LSTAR / SFMASK / EFER**：x86_64 系统调用入口相关的 MSR。
- **VBAR / SP_EL1**：aarch64 的异常向量基址与 EL1 栈指针。
- **stvec / sscratch**：riscv64 的 trap 向量与 U→S 栈交换寄存器。

### 4.1 kmain 阶段 A 的落地（→3.6）

C 的 阶段 A 五步在 Rust 侧有一处结构性变化：全局副本不是“拷贝结构体”，而是把启动信息的七个 slice（代码字段名：`reserved_regions`、`memmap`、`boot_modules`、`platform_sources`、`param_buf`、`memmap_sources`、`startup_objects`）落进 `.bss` 里的固定存储，再发布一个全局 `KernelInfo`；函数末尾的 `let mut k = *kernel_info` 仍然拷贝结构体本身。这一步的动因写在 `kmain` 入口的注释里（同文件的可锚定落点：`lib.rs` 的 [`fn kernel_info`](os/kernel/src/lib.rs:fn kernel_info) 与 `:fn store_kernel_info`）：入口的参数引用被编译器 spill 到内核栈之后再读回，栈槽遭别名覆写，经参数读到的字段全是 `.rdata` ASCII 垃圾。**覆写者在当前仓库没有单独定位**；这条按“入口尽早深拷”的预防性不变量处理，证据强度与未排除候选见 01 篇 §3.2.4/§5.3，不声称已证机制。因此 `kmain` 的第一个动作是改从全局槽取引用：

```rust
/* 节选（保留入口取值；诊断输出块与说明注释省略），os/kernel/src/lib.rs:fn kmain。 */
    let kernel_info: &KernelInfo = crate::kernel_info()
        .expect("kmain: KERNEL_INFO must be stored by arch_boot before jump");
```

`store_kernel_info` 做的七块落地（保留区间、memmap、模块表+名字池、平台源列表、启动参数缓冲、`memmap_sources`、`startup_objects`）见 `lib.rs` 的 [`fn store_kernel_info`](os/kernel/src/lib.rs:fn store_kernel_info)。随后一步有 C 侧对应：`minix_platform::init_from_kinfo`（平台描述符，早于时钟与中断）；C 的 `kernel_may_alloc` 闸门在 Rust 无对应物（该闸门在当前实现中不存在；C 侧读者是 `alloc_lowest` 的 `assert(kernel_may_alloc)`，Rust 侧没有同名机制），引导期写权限由调用图与 FreeRangeSet 保证。**阶段 A.2（`kmain` 函数体内注释里 阶段 A 的 memmap 登记子步骤）在现行 Rust 里故意为空**：内核不保留可变 memmap 副本，`FREE_MEMMAP` 已不存在；内核镜像的保留由 `vm_handoff::classify` 在构造 VM 交接记录时用 `cut_memmap` 扣除并记录，`init_proc_and_boot` 与 阶段 F 都没有 `add_memmap` 调用。`bootstrap_start/len` 两字段已随 D01 的裁决从 `KernelInfo` 删除（`lib.rs:1858-1863`；01 篇（`01-boot-shim-bootstrap.md`）§3.4），不再存在 `(0,0)` 占位。

C 的“把内核镜像自己也切一刀”在 Rust 侧由 `vm_handoff::classify` 的扣除记录接上：UEFI 路径上内核镜像不在 conventional memmap 里，扣除对 free list 是空操作；OpenSBI 路径上它防止 GET_MEMINFO 把内核镜像当空闲内存报出去。

**不切全局副本、继续使用入口参数引用会怎样**：源码注释记录的现象是：参数经栈槽读回后字段变垃圾，`validate` 在第一处不变量上 panic；诊断块留下的三个读数（`rdi`、全局槽指针、`rsp`）就是为此加的。依据：`lib.rs` 的 [`fn kernel_info`](os/kernel/src/lib.rs:fn kernel_info) 与 `:fn store_kernel_info`（后者落七个 slice）。

### 4.2 init_protection 的逐步落地（→3.2、3.3、3.7）

`init_protection` 同时对三个决策负责：

```rust
/* 节选（保留 BSP 选举、保护结构、门装配与装载的次序；省略号处为 `program_entry_base` 调用、Step 2 说明注释与 `register_current_arch` 说明注释；段间说明注释按节选略去），os/kernel/src/lib.rs:fn init_protection。 */
    let topo = minix_platform::platform_desc().cpu_topology();
    let bsp_hw = elected_bsp_hw_id(&topo);
    let bsp_logical = (0..topo.nr_cpus as usize)
        .find(|&i| topo.cpus[i].hw_id == bsp_hw)
        .unwrap_or(0) as u32;
    let prot = CurrentProtection::init(bsp_logical, kernel_info.kern_stack_top());
    store_protection(prot);
    #[cfg(target_arch = "x86_64")]
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

**把 `EFER.SCE` 的写入提前到 `LSTAR` 之前会怎样**：`EFER.SCE` 一旦置位，`SYSCALL` 指令立刻可用，而 LSTAR 可能仍是 0（复位值）；这条窗口里用户态的第一条 `SYSCALL` 跳到地址 0。两件事的正确顺序是"先把入口地址与掩码写全，再让机制生效"——`configure_syscall` 内部也是这个顺序（STAR → LSTAR → SFMASK → EFER）。依据：`trap_entry.rs` 的 [`fn configure_syscall`](os/arch/src/x86_64/trap_entry.rs:fn configure_syscall) 的写入次序与 `lib.rs` 的 [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection) 的调用点。该窗口内没有 CPL3 用户态（`configure_syscall` 在 阶段 B，IF=0），SYSCALL 不会到达；这条反事实若要成立，需在已有用户态的路径上讨论。

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

`load_with_tss` 是装载序列，其中第 4 步（`xor`+`mov fs/gs` 那一步）与 GS 区的次序构成一条被记录过的坑：

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

`X86_64TrapEntry::init` 只建占位表：256 条 `IdtEntry64::empty()`，所有字段 0，`gate_present` 全 false。真正的 41 条门由 `install_idt_handlers` 在 `load()` 之前写齐；`set_handler` 是唯一写者：DPL 由 `user_accessible(vector)` 决定（3/4/32–35 为 DPL=3），IST 由 NMI/#DF 表决定（2→IST1、8→IST2），类型恒为中断门。C 的 39 条门是 16 条 PIC + 23 条异常/软中断，**不含** 0xFF/0xF0/0xF1；Rust 在这 39 条基础上少一条向量 9、加 0xFF/0xF0/0xF1 三个 LAPIC/调度向量，共 41 条。C 侧异常区含 DPL=3 的 3/4，软中断区 32–35 全 DPL=3，PIC 区覆盖 0x50–0x57 与 0x70–0x77。

```rust
/* 代码引用，os/arch/src/x86_64/trap_entry.rs:fn init（整函数）。 */
    fn init() -> Self {
        // Placeholder table: every entry is absent (handler/dpl/ist/type all
        // zero). `install_idt_handlers` is the sole writer of the 41 real
        // gates, immediately before `load()`; `set_handler` writes all
        // metadata (DPL from `user_accessible`, IST from the NMI/#DF table,
        // type always an interrupt gate), so there is no second metadata
        // source to drift.
        Self {
            idt: [IdtEntry64::empty(); IDT_ENTRIES],
        }
    }
```

`IdtEntry64` 的七个字段与 C 的 `gatedesc_s` 对照：`offset_low`/`offset_mid`/`offset_high` 三段拼出 64 位入口地址（C 只有两段）、`selector`、`ist`（C 没有）、`p_dpl_type`（存在位+DPL+门类型）、`_reserved`。`fn new` 里 `ist & 0x7` 与 `dpl & 0x3` 两处掩码是硬件字段宽度的反映。

真正的入口由 `install_idt_handlers` 写入；它同时是安装后的表断言点：

```rust
/* 代码引用，os/arch/src/x86_64/trap_stub.rs:fn install_idt_handlers（整函数）。 */
pub fn install_idt_handlers(entry: &mut X86_64TrapEntry) {
    let mut i = 0;
    while i < STUB_VECTORS.len() {
        let (vector, _) = STUB_VECTORS[i];
        let handler = stub_va(vector).expect("STUB_VECTORS entry without a stub symbol");
        // `set_handler` is the sole writer of the gate metadata: DPL comes
        // from `user_accessible`, IST from the NMI/#DF table, type is always
        // an interrupt gate [ARCH: IST replaces i386 same-stack nesting].
        entry.set_handler(
            InterruptVector::new(vector),
            handler,
            user_accessible(vector),
        );
        let expected_dpl = if user_accessible(vector) { 3 } else { 0 };
        let expected_ist = match vector {
            2 => 1,
            8 => 2,
            _ => 0,
        };
        let (dpl, ist, kind) = entry.gate_metadata(vector);
        assert_eq!(dpl, expected_dpl, "vector {vector:#04x}: DPL != user_accessible");
        assert_eq!(ist, expected_ist, "vector {vector:#04x}: IST table mismatch");
        assert_eq!(kind, GATE_TYPE_INTERRUPT, "vector {vector:#04x}: not an interrupt gate");
        i += 1;
    }
    // Post-install coverage: exactly the stub vectors are present. A gate
    // outside the table (or a missing stub) would otherwise reach `load()`
    // and fault with no useful context.
    for vector in 0..=u8::MAX {
        assert_eq!(
            entry.gate_present(vector),
            has_stub(vector),
            "vector {vector:#04x}: gate presence != stub coverage"
        );
    }
}
```

`set_handler` 写门时把 IST 一起写：向量 2 写 1、向量 8 写 2、其余写 0。硬件按 `TSS.ist[ist-1]` 取值，所以实际用到的是 TSS 的 `ist[0]`/`ist[1]`（`setup_tss_for_cpu` 只填这两格）。`STUB_VECTORS` 的每项还带一个“是否由 CPU 压入错误码”的布尔值，但布尔值只被宿主测试 `test_errcode_classification_matches_sdm` 使用；汇编入口由 `TRAPSTUB vec err` 字面量实例化，不读这个布尔值。
`set_handler` 自己把 IST 一起写：向量 2 写 1、向量 8 写 2、其余写 0；仓内没有 `set_gate_ist`。硬件按 `TSS.ist[ist-1]` 取值，所以实际用到的是 TSS 的 `ist[0]`/`ist[1]`（`setup_tss_for_cpu` 只填这两格）。`STUB_VECTORS` 的每项还带一个“是否由 CPU 压入错误码”的布尔值，但布尔值只被宿主测试 `test_errcode_classification_matches_sdm` 使用；汇编入口由 `TRAPSTUB vec err` 字面量实例化，不读这个布尔值。

**让 `set_handler` 只写 handler/DPL、把 IST 留给安装后的第二次写会怎样**：门里 IST=0 的含义是"这类异常在当前栈上递送"；NMI 的用途之一是打断已经跑飞的代码、双故障的本意是"当前栈已经不可用时的最后一道门"。双故障门落回坏栈上递送，CPU 连最后的诊断机会都没有，直接三重故障。单写者把 IST 映射放进 `set_handler`，安装后立即断言 2/8 的 IST 与门类型；漏写会在安装点停机，而不是等到第一次 NMI。

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
/* 节选（保留前置条件与写入两行；省略号处为 EL1h 模型的长注释），os/arch/src/arm64/protection.rs:fn init。 */
    fn init(cpu_id: u32, kernel_stack_top: VirBytes) -> Self {
        ...
        assert_ne!(kernel_stack_top.0, 0, "init(0) would poison TPIDR_EL1");
        set_entry_sp(kernel_stack_top);
        Self
    }
```

理由写在 `protection.rs` 的 [`fn init`](os/arch/src/arm64/protection.rs:fn init) 的注释里：EL1h 模型下 `SPSel=1`，`SP_EL1` 就是当前的活动栈指针；在返回中的 Rust 函数里 `msr SP_EL1, Xt` 会把当前帧挂空（函数尾声从新栈弹 `x29/x30`），这是启动期崩溃的真实原因，——源码注释曾写 UNDEFINED，已由 `fn init` 的注释修正；§4.6 引用的是修正后的口径。从 EL0 进 EL1 时硬件选中 banked `SP_EL1`；不存在的是 x86 TSS 那样的内存取栈。所以每个 CPU 的栈基址停在 `TPIDR_EL1`，由返回用户的路径在 `eret` 之前重装（这条栈基址约定与 `TPIDR_EL1` 的所有权写在 `set_entry_sp` 的文档注释里）。`set_kernel_stack` 因此是文档化的 no-op：在返回中的 Rust 函数里搬动 `SP_EL1` 会把调用者帧挂空（函数尾声要从新栈弹 `x29/x30`）。`load` 只发一条 `isb`。

`TrapEntryArch::load` 写 `VBAR_EL1` 再 `isb`（`trap_entry.rs` 的 [`fn load`](os/arch/src/arm64/trap_entry.rs:fn load)）。向量表本体由 `trap_stub` 的 `global_asm!` 定义：2 KiB（16 槽 × 128 字节）、`.align 11` 保证基址低 11 位为 0，槽位按"异常类别 × 来源"固定排布。

riscv64 侧写 `sscratch`：

```rust
/* 代码引用，os/arch/src/riscv64/protection.rs:fn set_entry_stack_base（整函数）。 */
pub(crate) fn set_entry_stack_base(base: VirBytes) {
    debug_assert_eq!(base.get() % 8, 0, "kernel stack top must be 8-byte aligned");
    // SAFETY: CSR write to sscratch in S-mode with a valid kernel VA; each
    // hart writes only its own CSR, so no cross-hart alias exists.
    unsafe {
        asm!("csrw sscratch, {}", in(reg) base.get(), options(nomem, preserves_flags));
    }
}
```

`sscratch` 是用户陷阱的交换寄存器（进入内核时第一条指令 `csrrw sp, sscratch, sp` 把用户 sp 存进去、内核栈换出来），用户陷阱进入内核后它保存的是用户 sp；`TrapReturnArch::prepare_user_return` 在每次用户返回前把调度方传入的 `stack_top` 重新写进本 hart 的 `sscratch`，下一次进入时再交换。状态只存在 per-hart CSR 里，没有共享单元——共享标量曾被 SMP bring-up 的 AP `init_ap` 覆盖，源码注释与 `os/arch/tests/riscv64_percpu_trap_stack_pin.rs` 都禁止它回归。`ProtectionArch::load` 在 riscv64 上没有需要执行的步骤（CSR 写入即生效）；异常入口由 `TrapEntryArch::load` 写 `stvec` 承载——Direct 模式（低两位为 0），实现里在两只入口腿（源码注释把内核与用户两套入口都称作 "leg"，这里沿用译为"腿"：**内核腿**/**用户腿**）之间切换 `stvec`，用户腿第一条指令无条件交换 `sscratch`（不按 `SPP` 判定；`SPP` 只在返回侧重锚 `stvec` 时被读），随后由软件按 `scause` 分派。

两条路径的**触发指令**在各体系结构里是固定的一条：aarch64 用 `svc`（[`trap_stub.rs`](os/arch/src/arm64/trap_stub.rs) 的文件注释写明 `svc` **会**把返回地址推进到下一条指令），riscv64 用 `ecall`（[`trap_stub.rs`](os/arch/src/riscv64/trap_stub.rs) 的文件注释写明 `ecall` **不**推进 `sepc`——这一点与 x86 的 `int`、arm64 的 `svc` 相反，返回地址的修正因此落在内核的 trap 入口体自己身上）。用户态执行到那条指令时才发生特权级切换与入口跳转；本篇只到"门与入口已经就位"，完整的"用户 → 内核 → 用户"往返（谁保存寄存器、谁切换栈、哪条指令把特权级降回去）归异常与中断篇（`14-exception-interrupt.md`）。

**aarch64 的 `init` 直接 `msr SP_EL1, kernel_stack_top` 会怎样**：`SP_EL1` 在 EL1 可访问（HCR_EL2.TSP=0）；真正的问题是它把当前活动栈指针换掉，函数尾声从新栈弹 `x29/x30`，启动期崩溃。真正的栈基址因此停在 `TPIDR_EL1`，由 `restore_to_user` 在 `eret` 前重装。依据：`protection.rs` 的 [`fn init`](os/arch/src/arm64/protection.rs:fn init) 与 [`fn set_entry_sp`](os/arch/src/arm64/protection.rs:fn set_entry_sp) 的注释，以及 `test-protection-aarch64` 的只读 `SP_EL1` 探针。

把三架构的 Rust 落点按 §1.2 的四类入口收一张表：

| 四类入口 | x86-64 | aarch64 | riscv64 |
|---|---|---|---|
| 当前特权级 | GDT DPL + CS.RPL | EL0/EL1 | U/S mode |
| 异常/中断入口 | IDT 41 门（16 字节门） | `VBAR_EL1` 的 16 槽向量表 | `stvec` 的内核腿/用户腿 |
| 系统调用入口 | SYSCALL/SYSRET + `LSTAR`（不是 IDT 门） | `svc` 经 `VBAR_EL1` | `ecall` 经 `stvec` |
| 陷入内核用哪根栈 | TSS.sp0 + IST + GS 区 | SP_EL1 + `TPIDR_EL1` | `sscratch` + `prepare_user_return` |

### 4.7 与 Minix3 C 的对照（→§2.1–§2.7）

| C 侧的行为 | Rust 侧落点 | 处置 | 差异原因 |
|---|---|---|---|
| `kmain` 拷 `kinfo` 到全局 | [`fn store_kernel_info`](os/kernel/src/lib.rs:fn store_kernel_info)（把七个 slice 落进 `.bss`、发布全局副本） | 改型 | 值语义与 `.bss` 落地；引用不能跨镜像存活（01 篇 §3.4） |
| `bss_test` 断言 | 无对应物 | 淘汰 | BSS 清零由 boot-shim 装载器保证（测试覆盖见 `01-boot-shim-bootstrap.md` §5.2.1） |
| `kernel_may_alloc` 闸门 | 无对应物（闸门在当前实现中不存在） | 淘汰 | 引导期写权限由调用图与 FreeRangeSet 保证（`lib.rs:3916-3919`） |
| `cstart` 单函数 | [`fn init_protection`](os/kernel/src/lib.rs:fn init_protection) + `:fn init_clock_and_interrupts` | 改型 | 按里程碑拆分（§3.6） |
| `prot_init` 的表清单 | `:fn init_protection` 的两段（保护结构 + trap 层） | 改型 | 两个 trait 的职责划分（§3.2） |
| `tss_init` 的 `sp0` 与栈顶保留字 | [`fn setup_tss_for_cpu`](os/arch/src/x86_64/protection.rs:fn setup_tss_for_cpu) + `:fn stamp_cpu_id` | 保留（宽度随 64 位） | 保留字从 2×4 字节变 2×8 字节 |
| `_cpufeature(SYSENTER/SYSCALL)` + 两套入口 | `:fn configure_syscall` 只装 SYSCALL | 淘汰 SYSENTER | 长模式不提供 SYSENTER |
| per-CPU 入口标签 + `k_percpu_stacks` | [`struct GsArea`](os/arch/src/x86_64/trap_stub.rs:struct GsArea) + `:fn program_gs` | 改型 | 入口从 GS 区取栈顶 |
| `prot_load_selectors` 的 `lldt` | [`fn load_with_tss`](os/arch/src/x86_64/protection.rs:fn load_with_tss) 注释说明不执行 `lldt` | 淘汰 | 长模式不使用 LDT（x86 GDT 无 LDT 描述符，`load_with_tss` 不执行 `lldt`） |
| 重建页表（`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`） | [`fn arch_boot_impl`](os/kernel/src/lib.rs:fn arch_boot_impl) 建一次 | 淘汰重建 | §3.5 |
| `prot_init_done` 标志 | x86-64：全局 `Option` + `with_protection`/`with_trap_entry` 的 panic 契约；aarch64/riscv64：无状态实现（accessor 造哑实例） | 改型（x86 限定） | 未初始化在 x86 是确定失败点；non-x86 不进入全局 |
| `gate_table_*` 39 条门 | [`const STUB_VECTORS`](os/arch/src/x86_64/trap_stub.rs:const STUB_VECTORS) 41 项 + `:fn install_idt_handlers` | 改型 | 长模式保留 9/15、增加 LAPIC/调度向量（PIC 对照；APIC 路径另加 IOAPIC 门） |

本篇做完之后，系统处在这样一个状态：三张表都已装载生效、门里有真入口（§3.7）；**中断控制器尚未初始化**（`intr.init()` 属于 `init_clock_and_interrupts`，不在本篇射程），**也没有任何中断源被放行**（IF=0 一直保持）；进程与用户态都还没有。下一件事是 阶段 B 的剩余部分——时钟与中断控制器初始化（`lib.rs` 的 [`fn init_clock_and_interrupts`](os/kernel/src/lib.rs:fn init_clock_and_interrupts)），它才调用 `intr.init()` 编程 LAPIC/IOAPIC；其次序与内部拆分是 `05-clock-interrupt-init.md` 的主题。

---

## 5. 测试要点

### 5.0 本章导读

#### 学习目标

读完本章能：

- 说出宿主测试能证明什么、不能证明什么，以及 `test-protection` 三架构真机门覆盖到哪一步。
- 把测试缺口表里的每一项对到具体机制断言，不把“部分覆盖”读成“已证明”。
- 复算宿主测试计数，不沿用历史读数。

#### 前置概念

- 第 4 章的 `init_protection` 实现与三架构差异。
- 会读 `cargo test` 的通过计数与 QEMU 测试的 `TEST_RESULT` 判定。

#### 最小示例

```bash
# 宿主侧保护/门表的单元测试（§5.1）
$ cd os && cargo test -p minix-arch
# 三架构 test-protection 载体的构建与运行入口（§5.2）
$ grep -n 'test-protection' os/qemu-tests/run_all.sh
```

第一条验证位与字段，第二条给出真机载体的注册位置；宿主与真机回答的是不同层面的问题。

#### 失败对照

如果宿主测试通过就宣称“CPU 装载之后的行为已证明”，会把位字段检查当成硬件行为证据：`load_with_tss` 的 `lgdt` 之后是否真的生效、第一次 CPL3 进入是否从 `sp0` 取栈，都只能在真机链上观察。缺口表里的“部分覆盖”必须保留，不能用一条绿测试掩盖。

#### 小结

- 宿主测试证明表里的位与字段；真机 `test-protection` 证明三架构能装载并继续启动。
- 缺口表把未覆盖项对到机制断言与跨篇登记，不把“部分覆盖”当成“已证明”。
- 测试计数按当前源码复算；真机门的 PASS 判定以 `TEST_RESULT` 行为准。

#### 术语

- **`test-protection`**：三架构的保护结构真机载体；判据是装载后继续启动。
- **缺口表**：§5.3 的未覆盖项清单；每项给补法与去向。
- **M 项**：§5.4 的机制断言编号；用于把缺口对到具体机制。
- **部分覆盖**：宿主或真机只证明了一部分前提，不能读成完整证明。

### 5.1 宿主：arch crate 的单元测试（→4.3、4.4）

宿主上能证明的是"表里的位与字段"，不是"CPU 装载之后的行为"。测试计数与分布：

```
$ for f in os/arch/src/x86_64/protection.rs os/arch/src/x86_64/trap_entry.rs os/arch/src/x86_64/trap_stub.rs os/arch/src/arm64/protection.rs os/arch/src/arm64/trap_entry.rs os/arch/src/arm64/trap_stub.rs os/arch/src/riscv64/protection.rs os/arch/src/riscv64/trap_entry.rs os/arch/src/riscv64/trap_stub.rs; do printf "%s: " "$f"; grep -c '#\[test\]' "$f"; done
os/arch/src/x86_64/protection.rs: 16
os/arch/src/x86_64/trap_entry.rs: 13
os/arch/src/x86_64/trap_stub.rs: 12
os/arch/src/arm64/protection.rs: 0
os/arch/src/arm64/trap_entry.rs: 1
os/arch/src/arm64/trap_stub.rs: 3
os/arch/src/riscv64/protection.rs: 0
os/arch/src/riscv64/trap_entry.rs: 1
os/arch/src/riscv64/trap_stub.rs: 7
```

这 9 个文件是与本篇直接相关的子集（按 `#[test]` 属性统计，不是按锚点去重）；arch crate 全量 `src/**/*.rs` 按 `#[test]` 属性口径是 376 个/52 文件，`cargo test -p minix-arch --lib -- --list` 实际列出 266 个可运行测试（默认形态 266/0），`cargo test -p minix-arch -- --list` 含 `os/arch/tests` 共 355 个；`os/arch/tests/*.rs` 静态属性 87 个/21 文件。静态属性数高于 `--list` 是因为部分测试被 `#[cfg]` 门住或属于门内 inventory，实际可运行数以 `--list` 为准。

x86_64 的 16 个保护测试里与本篇直接相关的几类：TSS 布局（`tss64_size_is_104_bytes`、`tss64_offsets_correct`）、描述符位组合（`gdt_descriptors_have_correct_dpl`、`gdt_descriptors_are_flat_mode`、`tss_descriptor_is_64bit`）、描述符与结构体地址的一致性（`tss_descriptor_high_half_carries_base_above_4gib`、`init_creates_tss_descriptor_in_gdt`）、`sp0` 算术（`set_kernel_stack_updates_sp0`、`init_sets_tss_sp0_below_reserved_area`）。13 个 trap 测试 = 9 条门表测试（1 条 `init` 全占位 + 1 条保留向量 + 7 条 `install_idt_handlers` 后的 DPL/IST/类型）+ 2 条 `set_handler` + 2 条结构布局；`install_idt_handlers` 在生产路径上对 41 条门逐门断言 DPL/IST/类型，并核对 gate presence 与 stub 覆盖相等；`user_accessible` 的谓词与 DPL 另有 `test_user_accessible_gate_set` 与 `set_handler_sets_dpl_correctly` 两条直接断言。IPC 门（向量 33）由 `install_trap_stubs` 装 stub，不再单列门测试。12 个 trap_stub 测试里的覆盖检查改为先调 `install_idt_handlers` 再对 `STUB_VECTORS` 与 gate presence 核验。

四条启动顺序源码钉在 `os/kernel/tests/init_protection_order_pin.rs`：`store_protection → refresh_descriptors → load → program_entry_base`、`install_trap_stubs → trap.load`、`smp_init` 的 `set_kernel_stack → boot_ap`，以及 BSP 逻辑号只来自 `elected_bsp_hw_id`（`bsp_identity_has_one_source`）。宿主不能执行 `lgdt`/`ltr`/`lidt`，源码序是这里诚实的仪器；顺序反排即红。

aarch64 与 riscv64 侧的数字对应的是浅测试：两份 `protection.rs` 在 `cpu_count` 自证测试删除后没有宿主测试（状态是 ZST，由静态 accessor 与真机载体覆盖）、两份 `trap_entry.rs` 各 1 条 trait 边界断言（`aarch64_trap_entry_arch_impl_satisfies_trait_bound` 与 `riscv64_trap_entry_arch_impl_satisfies_trait_bound`）。两份 `trap_stub.rs` 各 3 与 6 条（`grep` 的 7 含一条注释命中），覆盖向量表布局与入口腿的静态断言。它们证明的是枚举往返、结构体字段与表布局，不覆盖 §4.6 的寄存器语义——那部分只能在真机上读（§5.2）。

宿主覆盖不到的部分需要说清：`lgdt`/`lidt`/`ltr` 真正执行后的 CPU 状态、CPL3→CPL0 转场的实际栈切换、AP 上 MSR 的实际写入、`isb`/`csrw` 之后的可见性——这些只能在真机上读寄存器。

### 5.2 真机：test-protection 三架构（→4.2、4.6）

三份测试载体各自把"`init_protection` 之后 CPU 的寄存器读数"作为判据，对生产代码的复用程度不同：x86_64 与 riscv64 版复用生产 trait 方法（`CurrentProtection::init`/`load` 与 `CurrentTrapEntry::init`/`configure_syscall`/`load`；x86_64 版还调 `install_trap_stubs` 与 `program_entry_base`，门里是真入口、GS 基址已编程），aarch64 版是独立探针——不调 `init_protection`/`CurrentProtection::init`/`CurrentTrapEntry`，自己装 `VBAR_EL1` 与探针汇编。

x86_64 版（[`main.rs`](os/qemu-tests/test-kernels/kernel/bootstrap/test-protection/src/main.rs)）在 `arch_boot_impl` 之后按 `kmain` 阶段 B 的方针调用 `CurrentProtection::init`/`load`/`program_entry_base` 与 `CurrentTrapEntry::init`/`install_trap_stubs`/`configure_syscall(syscall_entry_va())`/`load`（生产入口，不是 `KernelInfo.syscall_entry` 元数据），然后读六处寄存器读数（GDTR/IDTR 的基址、TR 的选择子、GS_BASE、CS/DS 的选择子）与四个 SYSCALL MSR（STAR/LSTAR/SFMASK/EFER.SCE）。实例留在栈上（main 不返回），所以载体不覆盖 `store_protection` + `refresh_descriptors` 的搬迁收尾；那一步由 `test-user-trap` 的生产 `init_protection` 覆盖。

```rust
/* 代码引用（保留判定与输出两行），os/qemu-tests/test-kernels/kernel/bootstrap/test-protection/src/main.rs（x86_64 版）。 */
    if gdtr_base == 0 {
        early_console::write_str("  FAIL: GDT base is 0\n");
```

同一文件里 `sidt` 读 IDTR、`str` 读 TR、`mov cs/ds` 读选择子低两位（RPL=0 即 ring0）。aarch64 版读 `CurrentEL`（必须等于 EL1）、`VBAR_EL1`（非零）、`DAIF`（全屏蔽），并做 `SPSel` 切换与 `SP_EL1` 只读回环——回环只有一条 `mrs x0, SP_EL1`，文件注释标注 READ-ONLY（代码里没有对 `SP_EL1` 的写；`msr SP_EL1` 这一串只出现在解释此前一次崩溃的记录注释里）；回环放在 `SPSel=0` 窗口内做，文件注释说明原因：`SPSel=1` 时 `SP_EL1` 就是当前 SP，测试会把自己的栈换掉。riscv64 版读 `stvec`（非零且低两位为 0，即 Direct 模式）、`sscratch`（必须等于 `kern_stack_top`）、`sstatus`（非零）。

运行方式与判定标记：`os/qemu-tests/run_all.sh` 把三个载体分别列入三架构的测试清单，判定由 `os/qemu-tests/run_qemu.sh` 按串口日志里的 `### TEST_RESULT: PASS` 前缀匹配完成（载体实际打印 `### TEST_RESULT: PASS test-protection[-aarch64|-riscv64] ###`）。

把三个载体合起来看：它们验证的是三张表与 MSR 的装载面（位与字段层在 §5.1 的宿主测试里）；`test-protection` 的 x86_64 载体现在调 `install_trap_stubs` 装真入口，并在 `program_entry_base` 后读 GS_BASE 非零，因此**门有效性**与「GS 基址在 `load` 之后编程」也进入真机覆盖——此外还有 §5.1 的覆盖检查与 `test-user-trap`（生产 `init_protection` 装真 stub 后以 CPL3 `int $0x21`（十进制 33）进门）分别承担。

**把 aarch64 的 `SP_EL1` 探针放回 `SPSel=1` 窗口内执行写入会怎样**：写的就是当前 SP，执行写入的那段代码会在读写之间把自己的栈换掉，崩溃会被误读成"EL2 陷获了 SP_EL1 访问"——测试文件里保留了这段定位记录。依据：`.../test-protection-aarch64/src/main.rs` 里 `Spsel` 窗口的注释段。

### 5.3 测试缺口

| 缺口 | 现状 | 补法 | 去向 |
|---|---|---|---|
| `init/load` 顺序不变量的负向验证 | 无负向测试；正向顺序钉见 `os/kernel/tests/init_protection_order_pin.rs` | 构造"先 load 后装门"的测试载体；需要新增装载前守卫（在 `load()` 之前显式断言 `gate_present` 覆盖全部向量），守卫失败即为负向证据 | **缺口1** |
| 门覆盖的负向验证 | 只有正向覆盖检查（`STUB_VECTORS` 与门表对齐）；漏掉一个向量时正向测试会红，但"漏掉后真机行为"没有载体 | 与上一条同族：在门表里人为留一个空向量，断言新增的装载前守卫拒绝 | **缺口1** |
| `refresh_descriptors` 的后果 | 无宿主测试（需要真实地址）；`test-user-trap` 走生产 `init_protection` 后以 CPL3 进入，覆盖了首次用户态进入 | 由 `test-user-trap` 的首次用户态进入间接覆盖 | **缺口2** |
| aarch64 `set_kernel_stack` 为 no-op 的行为 | API 层无可观察行为，只有文档说明 | 由返回用户态路径的真机载体覆盖（属异常/返回主题） | **缺口4**（跨篇） |
| [`fn init`](os/arch/src/x86_64/trap_entry.rs:fn init) 的作者口径 | `fn init` 只建 256 条空占位项；41 条真门由 `install_trap_stubs` 装，C 的 39 条对齐关系不在 `fn init` 的注释里 | 把 §3.7 的 41 项拆解写成断言（按区计数），任何一侧增删向量都会红 | **缺口5** |

`init_protection` 的文档注释已与函数体同步；`install_trap_stubs` 在 `trap.load()` 之前完成，入口非零（次序细节见 §4.2，不重复）。

### 5.4 机制 → 需要证明什么（反转核对表）

| # | 机制（正文位置） | 需要证明什么（可观测断言） | 覆盖状态 | 现有证据 |
|---|---|---|---|---|
| M1 | 三张表的建表（§2.3–§2.5） | GDT 槽位/访问字节/DPL；TSS 描述符两半与字段；IDT 门字段与分布 | 部分覆盖 | 宿主：16 个保护测试 + 13 个门表测试（含 `install_idt_handlers` 的 41 门 DPL/IST/type 断言） |
| M2 | 一次装载让表生效（§2.5） | 装载后 GDTR/IDTR/TR 指向内核自己的表 | 已覆盖（x86_64/riscv64 走生产 trait；aarch64 是探针） | 真机 `test-protection`：x86_64 读 GDTR/IDTR/TR/CS/DS；riscv64 读 `stvec`/`sscratch`；**aarch64 是独立探针**（自己写 `VBAR_EL1`/`DAIF`，不调 `TrapEntryArch::load`） |
| M3 | 门入口覆盖不变量（§3.7） | 41 个向量写齐；present 门 ↔ stub 一一对应；`load()` 在装齐之后 | 部分覆盖 | 宿主 `test_stub_coverage_matches_idt_gates`（正向）；`test-user-trap` 走生产 `init_protection` 以 CPL3 `int $0x21`（十进制 33）进门（真机正向）；**负向（先 load 后装门）无载体** → §5.3 缺口行；**缺口1** |
| M4 | 系统调用入口 MSR（§4.5） | STAR/LSTAR/SFMASK/EFER 的值与写入次序；AP 上重写 | 部分覆盖 | 宿主不覆盖 MSR 面；x86 载体在 `configure_syscall` 后用 `rdmsr` 校验 STAR/LSTAR/SFMASK/EFER.SCE；**AP 侧重写仍是行为性、无自动断言** → **缺口3**；QEMU 未跑 |
| M5 | 栈交接：`sp0`/保留字/GS 次序（§2.4、§4.3、§4.5） | 首次 CPL3→CPL0 用 `sp0`；GS 基址未被装载序列清掉 | 部分覆盖 | 宿主 `sp0` 算术 2 条；`init_ap` 的 production 断言校验 AP `sp0 == 自己的栈顶 - 16`；生产 `smp_init` 的 sp0 写入由 `test-smp-aps` 端到端覆盖（QEMU 未跑）；`test-smp-ap-alive-mb` 手工设置 sp0 后调 `init_ap`；第一次跨 ring 由 `test-user-trap` 覆盖；AP 的 MSR 重写仍无自动断言 → **缺口3**；aarch64 no-op 归 **缺口4** |
| M6 | 描述符地址稳定（§3.3） | 实例搬迁后 TSS 描述符指向最终地址 | 部分覆盖 | 宿主无法构造（需真实地址）；由首次用户态进入的真机链间接覆盖 → **缺口2** |
| M7 | 跨架构入口与交换（§4.6） | `stvec` 非零且 Direct；`sscratch = kern_stack_top`；`CurrentEL`/`VBAR_EL1`/`DAIF`；`SP_EL1` 只读 | 已覆盖 | 真机 `test-protection` aarch64/riscv64 的寄存器判据 |
| M8 | `sscratch` 返回重锚（§4.6） | 返回路径在每次用户返回前把本 hart 的 `sscratch` 重写为调度方传入的 `stack_top`（per-hart，无共享单元） | 未覆盖（本篇） | 属"用户→内核→用户"往返，归异常/中断篇的载体 → **缺口6**（跨篇） |
| M9 | 重建页表：普通副本建第二张表并切 CR3（§2.6，C 侧） | 清零→建表→`pg_load` 切 CR3；两张表分别是未分页副本与普通副本 | 说明性 | 架构行为与 C 侧逻辑，不在本仓测试面；Rust 侧以淘汰代替（§3.5）→ **缺口7**（登记，无动作） |
| M10 | x86-64 出口状态（§4.7；non-x86 见 §4.6 末表） | 门就位、无源放行（**控制器未编程**、IF=0） | 部分覆盖 | 真机链的时序读数；完整形态（时钟门放行）归 `05-clock-interrupt-init.md` → **缺口1 附属**（顺序负向同族） |

（§5.3 的缺口分别对到本表 M3、M5、M6 与 M1 的断言化；缺口 3/缺口 6/缺口 7 在 §5.4 就地定义（跨篇/登记），其余缺口 ID 与分流见 §5.3；本表只把它们对到机制断言。）

---

## 6. 诊断

### 6.0 本章导读

#### 学习目标

读完本章能：

- 按“症状 → 怀疑对象 → 确认命令”定位 GDT、IDT、TSS 三类故障。
- 用 QEMU `-d int,cpu_reset` 与 GDB 读 GDTR/IDTR/TR，区分“表没装”与“表装错”。
- 复现 §6.2 的可复制命令，不把引导方留下的表当成内核自己的表。

#### 前置概念

- 第 1–5 章的三张表、装载次序与测试证据边界。
- 会读 QEMU 异常日志与 GDB 的 `info registers` 输出。

#### 最小示例

```bash
# 先确认保护结构入口与装载函数存在（§6.2）
$ grep -n 'pub fn init_protection' os/kernel/src/lib.rs
$ grep -n 'fn load_with_tss' os/arch/src/x86_64/protection.rs
# QEMU 异常追踪：故障向量与地址（§6.1）
$ qemu-system-x86_64 ... -d int,cpu_reset -D /tmp/qemu_int.log
```

前两条给出代码落点，第三条给出异常现场；先静态、后动态，避免把装载次序问题误判成描述符位问题。

#### 失败对照

如果把 `ltr` 的 #GP 直接归因于 IDT 门没装，会改错表：`ltr` 查的是 GDT 里的 TSS 描述符，与 IDT 无关。正确顺序是先读 TR/GDTR 的现场值，再检查目标槽的 type 位是否为 busy、描述符高半是否丢失；两类原因的症状都在 `ltr`，修法完全不同。

#### 小结

- 先读 GDTR/IDTR/TR 的现场值，再决定是“表没装”还是“表装错”。
- `ltr` 的 #GP 优先查 TSS 描述符的 type/槽位，不要先怀疑 IDT。
- 复现命令按 §6.2 的次序跑；临时打印与正式路标要分开读。

#### 术语

- **三重故障**：异常处理期间再次异常且无可用门；通常意味着 IDTR/GDT 不可用。
- **busy TSS**：`ltr` 成功后描述符 type 置为 1011；第二次装载同一槽会 #GP。
- **`sp0`**：TSS 的 ring0 栈指针；为 0 时第一次 CPL3→CPL0 会在 0 上取栈。
- **现场值**：GDB 读到的 GDTR/IDTR/TR 基址与限长，用于区分“指向旧表”与“表内容错”。

### 6.1 症状 → 怀疑对象 → 确认命令

| 症状 | 怀疑对象 | 确认动作 |
|---|---|---|
| `init_protection` 之前任何异常即三重故障 | 引导方 IDTR/GDT 随 `ExitBootServices` 失效 | QEMU `-d int,cpu_reset` 的 `check_exception` 行；QEMU monitor `info registers` 读 GDT/IDT 的 base/limit（GDB 没有 `gdtr`/`idtr` 寄存器名） |
| 进入 `kmain` 后第一次压栈失败，故障地址是低地址 | 栈未切换（02 篇 §4.3）或 `kern_stack_top` 值错 | 读入口的 `rsp` 与 `kern_stack_top`，比对高半范围 |
| 远跳/CS 重载后取指 #GP，或 `ltr` 报 #GP | 新 GDT 的段描述符位组合错，或选择子指向的槽没填 | GDB `monitor info registers` 看 GDT/IDT 的 base/limit；测试载体按 §5.2 读 CS/DS 的 RPL |
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
# 关键行：16 / 13 / 12 / 0 / 1 / 3 / 0 / 1 / 7。

# 第三组：真机链（会在目标目录里重新构建）
bash os/qemu-tests/run_all.sh
# 或单跑一个载体：
os/qemu-tests/run_qemu.sh x86_64 os/target/x86_64-unknown-uefi/release/test-protection.efi
# 关键行：串口日志里的 ### TEST_RESULT: PASS test-protection ###（各架构后缀不同）；
#         失败时同一位置是 ### TEST_RESULT: FAIL ... ###，其前的若干行以 "  FAIL:" 开头。

# 第四组：GDB 远程（在 init_protection 前后读 GDT/IDT 两个表寄存器）
# 调试对象必须是带符号的生产 ELF（os/target/x86_64-unknown-none/release/kernel）；
# 用硬件断点：-S 暂停在复位向量时高半尚未映射，软件断点无法向高半地址写 0xcc。
# UEFI 测试载体的 .efi 是 stripped PE，gdb/nm 都读不到 init_protection，不要用它。
# 生产镜像按 `01-boot-shim-bootstrap.md` §6.4 的固件/盘面参数启动，追加 -s（开 gdbstub）与 -S（首指令前暂停）。
$ gdb os/target/x86_64-unknown-none/release/kernel
(gdb) target remote :1234
(gdb) hbreak minix_kernel::init_protection
(gdb) continue
(gdb) monitor info registers    # QEMU monitor 打印 GDT/IDT 的基址与限长；GDB 没有 gdtr/idtr 寄存器名
(gdb) finish                     # 停在调用者 `kmain`（`init_protection` 在 `kmain` 内调用）——IDT 的装载在函数体后半段（§4.2 的次序）
(gdb) monitor info registers
# 预期输出：两条 monitor 输出里 GDT/IDT 的基址与限长；后一条的 GDT 基址落在工件的高半地址段、
#           IDT 基址变为 IDT 数组地址。不要用单条 `next` 代替 `finish`——IDT 装载隔着一串调用，
#           且 release 构建下步进粒度受内联影响。
```

---

## 7. 参见

### 7.1 前置与后续篇

- [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) — 启动信息、内存记账、页表建立与开启、`KernelInfo` 字段来源
- [02-higher-half-kernel.md](02-higher-half-kernel.md) — 链接布局、按装载地址摆放、成对跳转
- [04-platform-discovery.md](04-platform-discovery.md) — 平台参数发现
- [05-clock-interrupt-init.md](05-clock-interrupt-init.md) — 时钟与中断控制器初始化
- [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) — 进程表与 boot 进程装载（`kmain` 阶段 C）
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
- `minix3/minix/kernel/arch/earm/{protect.c,arch_system.c,pre_init.c}`
- `minix3/minix/kernel/arch/i386/arch_clock.c`
- `minix3/minix/servers/vm/main.c`
- `minix3/minix/servers/vm/pagetable.c`

### 7.3 关联 Rust 源码清单

- `os/kernel/src/{lib,globals,smp}.rs`
- `os/arch/src/arch/{protection,trap_entry}.rs`
- `os/arch/src/{x86_64,arm64,riscv64}/{protection,trap_entry,trap_stub}.rs`
- `os/arch/src/x86_64/timer_irq_gate.rs`、`os/kernel/src/trap_dispatch.rs`
- `os/arch/tests/riscv64_percpu_trap_stack_pin.rs`
- `os/arch/src/lib.rs`
- `os/qemu-tests/test-kernels/kernel/bootstrap/{test-protection,test-protection-aarch64,test-protection-riscv64}/src/main.rs`

### 7.4 构建与回归入口

- `os/qemu-tests/run_qemu.sh`、`os/qemu-tests/run_all.sh`
