DOC: 01-boot-shim-bootstrap.md
CHAPTER: 2. C 源码分析
VERDICT: PASS

# Ch2 聚焦事实复核（快照 207a12ef）

## 0. 快照门

- 输入切片：`/home/xzhao/github/minix-rs/tmp/review-loop/01-stage/chapters/01-boot-shim-bootstrap/ch2-final-207a12ef.md`
- `sha256sum` 得 `207a12effbb4712b60a91da79da9305149dde96fb68cf2d31769d7d0fa8eb8b3`，与任务给定一致；`wc -l` 为 953 行。快照门 PASS。
- C 基线：`/home/xzhao/github/minix-rs/minix3/`（只读）。
- 本轮只核对 207a12ef 相对上一版的六个改动点，不重跑全文；判定一律重新对 C 源核对，不沿用其他轮次的结论。

## 一、逐项复核

### 1. `alloc_lowest` / `copy_trampoline` 的符号与跳板尺寸 — PASS

**切片（565 行与 568–581 行代码引文，601 行推论）**

- 565 行：`alloc_lowest` 的唯一调用点在 `copy_trampoline`，调用为 `trampoline_base = alloc_lowest(&kinfo, tramp_size);`。
- 572–573 行代码引文：

```
	tramp_size = (unsigned) &__trampoline_end - tramp_start;
	trampoline_base = alloc_lowest(&kinfo, tramp_size);
```

- 601 行写明：`arch_smp.c` 的 `tramp_start = (unsigned)&trampoline`，`tramp_size = &__trampoline_end - tramp_start`；当前约 2212 B（16 位代码 + 24 B 的 `__ap_id`/`__ap_pt`/`__ap_gdt`/`__ap_idt` 槽 + 固定 IDT 2048 B + GDT 56 B）；`GDT_SIZE = TSS_INDEX(CONFIG_MAX_CPUS) = 6 + CONFIG_MAX_CPUS`，当前 `CONFIG_MAX_CPUS = 1`，表为 7×8 = 56 B。

**C 源锚点**

```
$ nl -ba minix3/minix/kernel/arch/i386/arch_smp.c | sed -n '68,81p'
    68	void copy_trampoline(void)
    69	{
    70		unsigned tramp_size, tramp_start = (unsigned)&trampoline;;
    71	
    72		/* The trampoline code/data is made to be page-aligned. */
    73		assert(!(tramp_start % I386_PAGE_SIZE));
    74	
    75		tramp_size = (unsigned) &__trampoline_end - tramp_start;
    76		trampoline_base = alloc_lowest(&kinfo, tramp_size);
    77	
    78		/* The memory allocator finds the lowest available memory.. 
    79		 * Verify it's low enough
    80		 */
    81		assert(trampoline_base + tramp_size < (1 << 20));
```

```
$ nl -ba minix3/minix/kernel/arch/i386/trampoline.S | sed -n '45,58p'
    45	.balign 4
    46	LABEL(__ap_id)
    47	.space 4
    48	LABEL(__ap_pt)
    49	.space 4
    50	LABEL(__ap_gdt)
    51	.space 8
    52	LABEL(__ap_idt)
    53	.space 8
    54	LABEL(__ap_gdt_tab)
    55	.space GDT_SIZE*DESC_SIZE
    56	LABEL(__ap_idt_tab)
    57	.space IDT_SIZE*DESC_SIZE
    58	LABEL(__trampoline_end)
```

```
$ nl -ba minix3/minix/kernel/arch/i386/include/archconst.h | sed -n '11p;19,21p;31p'
    11	#define IDT_SIZE 256	/* the table is set to it's maximal size */
    19	#define TSS_INDEX_FIRST      6
    20	#define TSS_INDEX(cpu)       (TSS_INDEX_FIRST + (cpu)) /* per cpu kernel tss */
    21	#define GDT_SIZE             (TSS_INDEX(CONFIG_MAX_CPUS))	/* LDT descriptor */
    31	#define DESC_SIZE	8

$ nl -ba minix3/minix/kernel/kernel.h | sed -n '7,8p'
     7	#ifndef CONFIG_MAX_CPUS
     8	#define CONFIG_MAX_CPUS	1
```

**尺寸实测**

按 `trampoline.S` 的指令与数据布局独立写一份等价汇编（`CONFIG_MAX_CPUS = 1`，脚本在 `tmp/factcheck-ch2/tramp_measure.S`，只读 C 源，不改仓库），再测符号差：

```
$ as --32 -o tramp_measure.o tramp_measure.S && nm -n tramp_measure.o | grep -E "trampoline|__ap_|__trampoline_end"
00000000 T trampoline
00000054 t __ap_id
00000058 t __ap_pt
0000005c t __ap_gdt
00000064 t __ap_idt
0000006c t __ap_gdt_tab
000000a4 t __ap_idt_tab
000008a4 t __trampoline_end

$ size -A tramp_measure.o | head -6
.text   2212      0
```

- `__ap_id` 到 `__ap_gdt_tab` 的差为 `0x6c - 0x54 = 0x18 = 24` B：`4 + 4 + 8 + 8`。
- GDT 表：`0xa4 - 0x6c = 0x38 = 56` B，即 `GDT_SIZE(7) × DESC_SIZE(8)`；`TSS_INDEX(CONFIG_MAX_CPUS) = 6 + 1 = 7`。
- IDT 表：`0x8a4 - 0xa4 = 0x800 = 2048` B，即 `IDT_SIZE(256) × DESC_SIZE(8)`。
- 总量：`0x8a4 = 2212` B；其中 16 位代码与对齐补白为 `84` B，`84 + 24 + 56 + 2048 = 2212`。

**结论**：符号写法与 `arch_smp.c:70-75` 一致（源码第 70 行多一个分号，切片未复制该笔误，不影响语义）；24 B 内嵌数据、56 B GDT、2048 B IDT 与 2212 B 总量全部成立。**判定 PASS。**

### 2. 移交 VM 时的 16 条截断 — PASS

**切片（613 行标题，615 行正文）**

> VM 只读前 `MIN(MAXMEMMAP, NR_MEMS) = 16` 条（`NR_MEMS = 16` 定义在 `minix/const.h`），下标 16–39 的 24 条被静默忽略：`mmap_size` 可达 40，VM 只读前 16 条——`vm/utility.c` 的注释原文是 "Any memory chunk in excess of NR_MEMS is silently ignored"。

**C 源锚点**

```
$ nl -ba minix3/minix/include/minix/const.h | sed -n '98p'
    98	#define NR_MEMS            16

$ nl -ba minix3/minix/include/minix/param.h | sed -n '13p;18p'
    13	#define MAXMEMMAP 40
    18	        multiboot_memory_map_t  memmap[MAXMEMMAP]; /* free mem list */

$ nl -ba minix3/minix/servers/vm/utility.c | sed -n '55,62p'
    55	  memset(mem_chunks, 0, NR_MEMS*sizeof(*mem_chunks));
    56	
    57	  /* Obtain and parse memory from kernel environment. */
    58	  /* XXX Any memory chunk in excess of NR_MEMS is silently ignored. */
    59	  for(i = 0; i < MIN(MAXMEMMAP, NR_MEMS); i++) {
    60	  	mem_chunks[i].base = kernel_boot_info.memmap[i].mm_base_addr;
    61	  	mem_chunks[i].size = kernel_boot_info.memmap[i].mm_length;
    62	  }
```

- `MIN(MAXMEMMAP, NR_MEMS) = MIN(40, 16) = 16`；数组下标 0–15 被读取，16–39 共 24 条不进入 VM 的 `mem_chunks`。
- `mmap_size` 的上界为 40：`add_memmap` 在 `mmap_size < MAXMEMMAP` 的断言之后，以 `m + 1` 抬高边界，`m` 最大 39（`pg_utils.c:97,104,110-111`），故边界最大 40。
- `vm/utility.c:58` 的注释原文包含 "Any memory chunk in excess of NR_MEMS is silently ignored"；切片的引文是该句的逐字子串（略去注释前缀 `XXX`）。
- 补充说明：VM 的读取循环不查询 `mmap_size`，而是固定读前 `MIN(MAXMEMMAP, NR_MEMS)` 个槽；`memmap[]` 在 `get_parameters` 里已由 `memset` 清零（`pre_init.c:339`），未越界的空槽是零值。切片“`mmap_size` 可达 40，VM 只读前 16 条”的表述与此不矛盾。

**结论**：16 条截断、24 条静默忽略、`NR_MEMS = 16`、`MAXMEMMAP = 40` 三项全部成立。**判定 PASS。**

### 3. 满槽停机点（2.4.2.1 表） — PASS

**切片（554 行，§2.4.2.1 槽位表第一行）**

> m 之后第一个空槽；若其后没有空槽（此时 `mmap_size` 必为 40），切除后的第一次 `add_memmap` 在入口 `assert(cbi->mmap_size < MAXMEMMAP)`（`pg_utils.c:97`）处停住；`panic("no available memmap slot")`（`pg_utils.c:120`）只在 assert 被 `-DNDEBUG` 编掉（`MKSMALL=yes`，`sys.mk:20-21`）时可达

**C 源锚点**

```
$ nl -ba minix3/minix/kernel/arch/i386/pg_utils.c | sed -n '97p;104,111p;120p'
    97	        assert(cbi->mmap_size < MAXMEMMAP);
   104	        for(m = 0; m < MAXMEMMAP; m++) {
   105			phys_bytes highmark;
   106	                if(cbi->memmap[m].mm_length) continue;
   107	                cbi->memmap[m].mm_base_addr = addr;
   108	                cbi->memmap[m].mm_length = len;
   109	                cbi->memmap[m].mm_type = MULTIBOOT_MEMORY_AVAILABLE;
   110	                if(m >= cbi->mmap_size)
   111	                        cbi->mmap_size = m+1;
   120	        panic("no available memmap slot");
```

```
$ nl -ba minix3/share/mk/sys.mk | sed -n '20,23p'
    20	.if ${MKSMALL:U} == "yes"
    21	CPPFLAGS+= -DNDEBUG
    22	DBG=	-Os
    23	.endif

$ nl -ba minix3/include/assert.h | sed -n '50,56p'
    50	#ifdef NDEBUG
    51	# ifndef __lint__
    52	#  define assert(e)	(__static_cast(void,0))
    53	# else /* !__lint__ */
    54	#  define assert(e)
    55	# endif /* __lint__ */
    56	#else /* !NDEBUG */
```

- 不变量：所有非空槽的下标都小于 `mmap_size`（`cut_memmap` 只遍历 `< mmap_size` 的槽，`add_memmap` 只在需要时把边界抬到 `m + 1`）。因此 `mmap_size < 40` 时，下标 `mmap_size` 的槽必为空，`add_memmap` 的 `for (m = 0; m < MAXMEMMAP; m++)` 循环必能找到一个空槽，`panic` 到不了。
- 当 0–39 全部被占用时，`mmap_size` 必为 40：切除后的补回若没有空槽，入口 `assert(cbi->mmap_size < MAXMEMMAP)` 在 `pg_utils.c:97` 停住，到不了 104 行的循环，更到不了 120 行的 `panic`。
- `assert` 在 `NDEBUG` 下被编成空语句（`include/assert.h:50-55`）；树内定义 `NDEBUG` 的唯一常规入口是 `MKSMALL=yes`（`share/mk/sys.mk:20-21`），而 `MKSMALL` 在 `bsd.own.mk:1306-1307,1313-1315` 的默认变量表里默认 `no`。故默认构建停在 97 行断言，`panic` 只在 `-DNDEBUG`（树内即 `MKSMALL=yes`）下可达。

**结论**：题目问的两项均成立：表仍写入入口 `assert(cbi->mmap_size < MAXMEMMAP)`；`panic("no available memmap slot")` 只在 assert 被 `-DNDEBUG` 编掉（`MKSMALL=yes`，`sys.mk:20-21`）时可达。**判定 PASS。**

**非阻断观察（P3 措辞事实，不影响本题两项判据）**：同一单元格把停住点写成“切除后的第一次 `add_memmap`”。按 `cut_memmap` 的实际调用序，命中条目清零后先补左余量（`pg_utils.c:57-59`），再补右余量（`60-61`）；该表格行的前提是左余量落到刚清空的槽 `m`，所以左余量那次 `add_memmap` 会成功，真正撞上入口断言的是右余量那次（切除后的第二次调用；只有无左余量时它才是第一次）。最小措辞修复：把“切除后的第一次 `add_memmap`”改为“右余量的那次 `add_memmap`（切除后的第二次调用；无左余量时才是第一次）”。

### 4. 模块计数 N1：`NR_BOOT_MODULES = 12`、`kmain` 硬检查、`image[NR_BOOT_PROCS]` 17 项 — PASS

**切片（736 行）**

> C 侧的计数锚点是 `NR_BOOT_MODULES = INIT_PROC_NR + 1 = 12`（`minix3/minix/include/minix/com.h`）与 `kmain` 的 `if(NR_BOOT_MODULES != kinfo.mbi.mi_mods_count) panic(...)`（`minix3/minix/kernel/main.c`）；`image[NR_BOOT_PROCS]` 共 17 项，其中 5 项是内核任务、12 项对应用户态引导进程，不是 12 项模块表

**C 源锚点**

```
$ nl -ba minix3/minix/include/minix/com.h | sed -n '56p;70,74p'
    56	#define NR_TASKS	  5 
    70	#define LAST_SPECIAL_PROC_NR	11	/* An untyped version for
    71	                                           computation in macros.*/
    72	#define INIT_PROC_NR ((endpoint_t) LAST_SPECIAL_PROC_NR)  /* init
    73	                                                        -- goes multiuser */
    74	#define NR_BOOT_MODULES (INIT_PROC_NR+1)

$ nl -ba minix3/minix/kernel/main.c | sed -n '160,162p'
   160	   if(NR_BOOT_MODULES != kinfo.mbi.mi_mods_count)
   161	   	panic("expecting %d boot processes/modules, found %d",
   162			NR_BOOT_MODULES, kinfo.mbi.mi_mods_count);

$ nl -ba minix3/minix/include/minix/param.h | sed -n '9p'
     9	#define NR_BOOT_PROCS   (NR_TASKS + LAST_SPECIAL_PROC_NR + 1)

$ awk 'NR>=44 && NR<=64 && /^\{/' minix3/minix/kernel/table.c | wc -l
17
```

- `INIT_PROC_NR = LAST_SPECIAL_PROC_NR = 11`，故 `NR_BOOT_MODULES = 11 + 1 = 12`；`kmain` 在 `main.c:160-162` 以 `panic` 硬检查模块数必须等于 12。
- `NR_BOOT_PROCS = NR_TASKS + LAST_SPECIAL_PROC_NR + 1 = 5 + 11 + 1 = 17`。
- `table.c:44-64` 的 `image[NR_BOOT_PROCS]` 共 17 个初始化项：`ASYNCM`、`IDLE`、`CLOCK`、`SYSTEM`、`HARDWARE` 5 个内核任务，加 `ds`、`rs`、`pm`、`sched`、`vfs`、`memory`、`tty`、`mib`、`vm`、`pfs`、`mfs`、`init` 12 个用户态引导进程。
- “不是 12 项模块表”一句成立：`image[]` 是 17 项的启动镜像表，12 只是其中的用户态进程项数，与 `NR_BOOT_MODULES` 的 12 个引导模块不是同一张表。

**结论**：三项事实全部与 C 源一致，且仍在 2.5.1（736 行）。**判定 PASS。**

### 5. 2.5.1 移到 2.5 之后：三行改账表、`arch_boot_proc`、VM `exec_bootproc`/`free_mem` — PASS

**结构与搬动检查**

- 节序：`### 2.5` 在 617 行，`#### 2.5.1` 在 700 行，`### 2.6` 在 750 行；2.5.1 位于 2.5 正文之后、2.6 之前，标题层级为 2.5 的子节。
- 切片内对 §2.5.1 的指向只有 215 行一处：“细节见 §2.5.1 的“还引导段”行”。该行指向的表格行在 708 行，存在且标题为“还引导段”。
- 未发现 `2.4.2.5`、`2.4.3`、`2.5.2` 等失效编号：`grep -n "2\.4\.2\.5\|2\.4\.3\|2\.5\.2" <slice>` 无输出。
- 2.5.1 内部的自指均指向本小节紧随其后的表（“下表”“本节上表”“本节末”），搬动后仍然成立。

**三行改账表（704–708 行）**

| 行 | 表项 | C 源锚点 | 核对结果 |
|---|---|---|---|
| 706 | 切占用：内核与全部模块的字节；`pre_init` → `get_parameters`；`cut_memmap` | `pre_init.c:194-214`：`module_list[k]` 写入内核区间，`mods_with_kernel = mi_mods_count + 1`，循环内先 `overlaps` 再 `cut_memmap(mod_start, mod_end)`；`pre_init.c:224` 由 `pre_init` 调 `get_parameters` | PASS：对象为内核加全部模块，动作与区间语义一致 |
| 707 | 还 VM 镜像：只有 VM 模块的字节；`arch_boot_proc`；`add_memmap` + 清空描述符 | `protect.c:402` 的 `if(rp->p_nr == VM_PROC_NR)` 包住整段；`protect.c:449-451` 的 `add_memmap(&kinfo, mod->mod_start, mod->mod_end-mod->mod_start);` 与 `mod->mod_end = mod->mod_start = 0;`；`protect.c:454` 记 `vm_allocated_bytes` | PASS：只还 VM 的 blob，加回后置零 |
| 708 | 还引导段：内核 unpaged 段；`kmain` 后段（`cstart`、`system_init` 之后）；`add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len)`；此时 `kernel_may_alloc` 仍为 1 | `main.c:147` 调 `cstart()`，`main.c:295` 调 `system_init()`，`main.c:301` 调 `add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len)`；`main.c:142` 置 1，`main.c:104-107` 在 `bsp_finish_booting` 末尾、`switch_to_user()` 前两行（中间隔一空行）置 0；`pre_init.c:114-116` 把 `bootstrap_start`/`bootstrap_len` 取自 `_kern_unpaged_start`/`_kern_unpaged_end`；`usermapped_data.c:7` 的 `kinfo` 确实在 `.usermapped` 段 | PASS：时刻、表达式、`kernel_may_alloc` 状态均一致 |

**`arch_boot_proc` 代码引文（714–728 行）与 C 源逐行对照**

```
$ nl -ba minix3/minix/kernel/arch/i386/protect.c | sed -n '445,454p'
   445			arch_proc_init(rp, execi.pc, (vir_bytes)sp,
   446				execi.stack_high - sizeof(struct ps_strings),
   447				ip->proc_name);
   448	
   449			/* Free VM blob that was just copied into existence. */
   450			add_memmap(&kinfo, mod->mod_start, mod->mod_end-mod->mod_start);
   451			mod->mod_end = mod->mod_start = 0;
   452	
   453			/* Remember them */
   454			kinfo.vm_allocated_bytes = alloc_for_vm;
```

切片 718–728 行的引文与上述 445–454 行逐行一致，仅注释头说明为节选；没有把 `add_memmap` 的第三实参写成结束地址，与 §2.4.2.3 对 `cut_memmap` 长度/结束地址错位的分析形成正确对照。

**VM `exec_bootproc` / `free_mem` 事实（738 行）**

```
$ nl -ba minix3/minix/servers/vm/main.c | sed -n '485,490p;498,520p'
   485		for (mod = &kernel_boot_info.module_list[0];
   486			mod < &kernel_boot_info.module_list[kernel_boot_info.mods_with_kernel-1]; mod++) {
   487			phys_bytes len = mod->mod_end-mod->mod_start+1;
   488			len = roundup(len, VM_PAGE_SIZE);
   489			mem_add_total_pages(len/VM_PAGE_SIZE);
   490		}
   498		for (ip = &kernel_boot_info.boot_procs[0];
   499			ip < &kernel_boot_info.boot_procs[NR_BOOT_PROCS]; ip++) {
   500			struct vmproc *vmp;
   501	
   502			if(ip->proc_nr < 0) continue;
   503	
   504			assert(ip->start_addr);
   505	
   506			/* VM has already been set up by the kernel and pt_init().
   507			 * Any other boot process is already in memory and is set up
   508			 * here.
   509			 */
   510			if(ip->proc_nr == VM_PROC_NR) continue;
   511	
   512			vmp = init_proc(ip->proc_nr);
   513	
   514			exec_bootproc(vmp, ip);
   515	
   516			/* Free the file blob */
   517			assert(!(ip->start_addr % VM_PAGE_SIZE));
   518			ip->len = roundup(ip->len, VM_PAGE_SIZE);
   519			free_mem(ABS2CLICK(ip->start_addr), ABS2CLICK(ip->len));
   520		}
```

- “先对每个模块 `mem_add_total_pages(len/VM_PAGE_SIZE)`”对应 485–490 行的循环：范围为 `module_list[0 .. mods_with_kernel-1)`，即全部 12 个模块（内核为第 13 条伪模块，被 `-1` 排除）。
- “随后 `exec_bootproc` 对其余 11 个模块”对应 498–514 行：`ip->proc_nr < 0` 的 5 个内核任务在 502 行跳过，VM 在 510 行 `continue`，余下 11 个模块进入 `exec_bootproc`。
- “再用 `free_mem(ABS2CLICK(ip->start_addr), ABS2CLICK(ip->len))`”与 519 行逐字一致；518 行先对 `ip->len` 做页上取整。
- “循环对 VM 自己的模块 `continue`”与 510 行一致；“VM 的 blob 由内核侧 `arch_boot_proc` 的 `add_memmap` 归还”与 402、450 行一致。

**结论**：三行改账表的对象、时刻、动作与 C 源一致；`arch_boot_proc` 的 VM 专属回收与 `exec_bootproc`/`free_mem` 的 11 个模块回收均核对无误；搬动没有留下悬空节号或失效指向。**判定 PASS。**

**非阻断观察（P3，不影响本题判定）**：738 行把 VM 侧的入账写成 `mem_add_total_pages(len/VM_PAGE_SIZE)`。这是 489 行的调用形式，但该处 `len` 是 487–488 行的 `roundup(mod->mod_end - mod->mod_start + 1, VM_PAGE_SIZE)` 的结果，不是原始 `mod_end - mod_start`。若要让读者不回头读两行，可补一句“这里的 `len` 已按 `mod_end - mod_start + 1` 向上取整到 `VM_PAGE_SIZE`”。

### 6. 失败对照新增“模块表分配失败 → HAS_MODS 未置位” — PASS

**切片（86–88 行 §2.1，921 行 §2.8 失败对照）**

- 88 行：“若模块表分配失败（另一条路）：`module_init` 会提前返回，`HAS_MODS` 未置位，内核停在查标志的断言，到不了条数断言。”
- 921 行：“模块表分配失败是另一条路：`module_init` 提前返回，`HAS_MODS` 未置位，内核停在查标志的断言，到不了条数断言（§2.1）。”

**C 源锚点**

```
$ nl -ba minix3/sys/arch/i386/stand/lib/exec.c | sed -n '563,569p;741,761p'
   563		/* Allocate the module list */
   564		btinfo_modulelist = alloc(len);
   565		if (btinfo_modulelist == NULL) {
   566			printf("WARNING: couldn't allocate module list\n");
   567			wait_sec(MODULE_WARNING_SEC);
   568			return;
   569		}
   741		/* pull in any modules if necessary */
   742		if (boot_modules_enabled) {
   743			module_init(file);
   744			if (btinfo_modulelist) {
   745				mbm = alloc(sizeof(struct multiboot_module) *
   746						   btinfo_modulelist->num);
   ...
   758				mbi->mi_flags |= MULTIBOOT_INFO_HAS_MODS;
   759				mbi->mi_mods_count = btinfo_modulelist->num;
   760				mbi->mi_mods_addr = vtophys(mbm);
   761			}
   762		}

$ nl -ba minix3/minix/kernel/arch/i386/pre_init.c | sed -n '167,169p'
   167		assert(mbi->mi_flags & MULTIBOOT_INFO_HAS_MODS);
   168		assert(mbi->mi_mods_count < MULTIBOOT_MAX_MODS);
   169		assert(mbi->mi_mods_count > 0);
```

- `module_init` 在 `btinfo_modulelist = alloc(len)` 返回 `NULL` 时打印警告、等待后直接 `return`（564–568 行），不会执行后续填充。
- `exec_multiboot` 在 `module_init(file)` 之后用 `if (btinfo_modulelist)` 包住 `HAS_MODS` 置位、`mi_mods_count` 与 `mi_mods_addr` 的填充（744–760 行）；分配失败时 `btinfo_modulelist` 保持静态初值 `NULL`（`exec.c:137`），整段被跳过，`HAS_MODS` 不置位。
- 内核 `get_parameters` 的断言顺序是 `HAS_MODS`（167 行）→ `mi_mods_count < MULTIBOOT_MAX_MODS`（168 行）→ `mi_mods_count > 0`（169 行）。标志未置位时停在 167 行，确实到不了 169 行的条数断言；168 行在条数为 0 时会通过，不影响“停在查标志断言”的结论。
- 与“链表为空但分配成功”的对照也成立：空链表时 `module_init` 分配成功、`num = 0`，`exec_multiboot` 仍会置 `HAS_MODS`、填 `mi_mods_count = 0`，内核通过 167 行后在 169 行停住。两条路径的现场确实不同。
- 本条的内核侧断言在 `pre_init.c` 的 `get_parameters`（167–169 行），不在 `kernel/main.c`；`main.c:160-162` 的 `kmain` 模块数硬检查在 `pre_init` 之后才执行，标志未置位时内核已在 167 行停住，走不到 `main.c:160`。题目把 `main.c` 与 `module_init` 并列时，对应的是这条先后关系，而不是把 167 行的断言归到 `main.c`。

**结论**：新增失败对照与 `module_init`、`exec_multiboot`、`pre_init.c` 的实际控制流一致。**判定 PASS。**

**非阻断观察（P3，不影响本题判定）**：句中的“模块表”指 `module_init` 里的 bootinfo 模块表 `btinfo_modulelist`。`exec_multiboot` 随后还有一次 `mbm = alloc(...)`（745 行）没有 NULL 检查；该分配失败不会取消已置位的 `HAS_MODS`，但不在这句话的射程内。若要消歧，可把“模块表分配失败”写成“bootinfo 模块表（`btinfo_modulelist`）分配失败”。

## 二、判定

**VERDICT: PASS。**

- 六项复核全部 PASS，无 P0/P1/P2 事实阻断项。
- 三处非阻断观察（第 1 条是序数事实错误，第 2、3 条是措辞精确度；都不改变任何结论）：
  1. 第 554 行“切除后的第一次 `add_memmap`”应为右余量那次（切除后的第二次调用；无左余量时才是第一次）。
  2. 第 738 行 `mem_add_total_pages(len/VM_PAGE_SIZE)` 的 `len` 已在 C 源中按 `roundup(mod_end - mod_start + 1, VM_PAGE_SIZE)` 处理；当前缩写可能让读者以为入账页数直接等于原始长度除以页大小。
  3. 第 88、921 行“模块表分配失败”的指代对象是 `module_init` 的 `btinfo_modulelist`；`exec_multiboot` 后续的 `mbm` 分配没有 NULL 检查，不在该句射程内。
- 未发现因 2.5.1 搬动产生的悬空节号：切片内唯一的 §2.5.1 指向（215 行）落到 708 行“还引导段”行；`2.4.2.5`、`2.4.3`、`2.5.2` 等旧编号无命中；节序为 2.5（617）→ 2.5.1（700）→ 2.6（750）。

## 三、命令汇总

- 快照：`sha256sum <slice>`；`wc -l <slice>`。
- 第 1 项：`nl -ba minix3/minix/kernel/arch/i386/arch_smp.c | sed -n '68,81p'`；`nl -ba minix3/minix/kernel/arch/i386/trampoline.S | sed -n '45,58p'`；`nl -ba minix3/minix/kernel/arch/i386/include/archconst.h | sed -n '11p;19,21p;31p'`；`nl -ba minix3/minix/kernel/kernel.h | sed -n '7,8p'`；本地汇编 `as --32` 后 `nm -n`、`size -A`。
- 第 2 项：`nl -ba minix3/minix/include/minix/const.h | sed -n '98p'`；`nl -ba minix3/minix/include/minix/param.h | sed -n '13p;18p'`；`nl -ba minix3/minix/servers/vm/utility.c | sed -n '55,62p'`。
- 第 3 项：`nl -ba minix3/minix/kernel/arch/i386/pg_utils.c | sed -n '97p;104,111p;120p'`；`nl -ba minix3/share/mk/sys.mk | sed -n '20,23p'`；`nl -ba minix3/include/assert.h | sed -n '50,56p'`；`nl -ba minix3/share/mk/bsd.own.mk | sed -n '1306,1307p;1313,1315p'`。
- 第 4 项：`nl -ba minix3/minix/include/minix/com.h | sed -n '56p;70,74p'`；`nl -ba minix3/minix/kernel/main.c | sed -n '160,162p'`；`nl -ba minix3/minix/include/minix/param.h | sed -n '9p'`；`awk 'NR>=44 && NR<=64 && /^\{/' minix3/minix/kernel/table.c | wc -l`。
- 第 5 项：`nl -ba minix3/minix/kernel/arch/i386/pre_init.c | sed -n '190,215p;224p'`；`nl -ba minix3/minix/kernel/arch/i386/protect.c | sed -n '402p;445,454p'`；`nl -ba minix3/minix/kernel/main.c | sed -n '104,107p;142p;147p;295p;301p'`；`nl -ba minix3/minix/kernel/usermapped_data.c | sed -n '7p'`；`nl -ba minix3/minix/servers/vm/main.c | sed -n '485,490p;498,520p'`；`grep -n "2\.4\.2\.5\|2\.4\.3\|2\.5\.2" <slice>`。
- 第 6 项：`nl -ba minix3/sys/arch/i386/stand/lib/exec.c | sed -n '563,569p;741,761p'`；`nl -ba minix3/minix/kernel/arch/i386/pre_init.c | sed -n '167,169p'`。
