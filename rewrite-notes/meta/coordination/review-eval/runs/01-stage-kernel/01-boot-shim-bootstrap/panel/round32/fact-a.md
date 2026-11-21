# P-FACT-A 报告 — 01-boot-shim-bootstrap.md（R32 面板，事实对账角色）

- **审查对象**：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（3565 行）
- **基线核对**：sha256 = `7fd75e78c9e1b43588c59fb193ad3845742bc37fc991e0d6b308e2dc09065329`，与 R32 基线前缀 `7fd75e78c9e1b435` 一致；行数 3565 一致。
- **审查范围**：第 2 章（§2.1–§2.8，L162–L1390）全部 C 引用断言 + §1 中引用 C 的断言（L19–L161：阶段表、D1–D5、五个栈顶名字表、mpx.S/objcopy 命名陷阱、boot2 边界声明）。
- **对照源**：`minix3/`（只读，未修改）。 Ground truth 优先序按 `prompt/review-rules/review.md`：Minix3 C 源码行为 > 一切文档。
- **证据强度**：本报告全部结论为 L1/L2 证据（Read/grep 实际命令输出，命令与行号见各条）；无 L3 推断入结论。

## 结论摘要

对第 2 章与 §1 的 C 引用断言做了逐条对账，覆盖约 190 条可核对断言（含 15 处整段代码引用的逐字比对、30+ 个常量/行号/文件定位、以及 mb_set_param 删除扫描、OVERLAP 相切推演、add/cut_memmap 手算、第三行常态四前提、vm_enable_paging 位序、pg_mapkernel 覆盖推演等分析性断言的重放）。**未发现 P0/P1 级事实错误**。发现 1 条 P3 级示例前提缺失，另有 2 条改进建议。

- 发现数：FINDING 1（P3 × 1，P0/P1/P2 × 0）；SUGGESTION 2。

---

## FINDING RD32-FACT-A-01（P3）

**位置**：§2.3，"删掉'已存在先删'那段循环会怎样"第二层（L464）：

> "例如 `param_buf` 里先有 `xboard=2`，内核再写 `board=` 时，中段 `board=2` 会被删掉，后一条 `arch=i386` 的前半截被补进原位，留下伪记录 `xarch=i386`。"

**源码证据**：`minix3/minix/kernel/arch/i386/pre_init.c:48-59`（`mb_set_param` 的删除扫描）：

```c
	while (*p) {
		if (strncmp(p, name, namelen) == 0 && p[namelen] == '=') {
			...
			break;
		}
		while (*p++)
			;
		p++;
	}
```

**为什么算缺陷**：该示例缺一条前提——`xboard=2` 不能是 `param_buf` 的第一条记录。扫描总是从 `p = bigbuf`（记录 0 的首字符）开始；若 `xboard=2` 是首条记录，`strncmp("xboar…", "board", 5)` 首字符即失配，随后 `while (*p++) ; p++` 直接跳过整条记录（落在下一条 `arch=i386` 的第二个字符 `r` 上，与 `"board"` 同样失配），**不会发生任何删除**，伪记录 `xarch=i386` 不会出现。只有当 `xboard=2` 之前还有至少一条更早的记录（例如命令行里写成 `console=… xboard=2`，或缓冲区为 `K=v\0xboard=2\0arch=i386\0\0`）时，`p` 才会被推到 `xboard` 记录的第二个字符 `b` 上、前缀匹配从记录中部命中，得到文档所述的 `xarch=i386` 结果。机制的陈述（"删除扫描只按 name= 前缀匹配，不要求命中点位于记录起点"）本身正确，缺陷仅在示例未给出"存在更早记录"这一前提；按最自然的极小读法（`xboard=2\0arch=i386\0\0`）不可复现。同节第三层（L465）对"首条提升分支"给了严格前提，此层的示例却缺前提，属同一议题上的精度不一致。

**严重度**：P3（示例前提缺失，机制结论不受影响；不误导"删循环会怎样"的主结论）。

**反证路径**：
1. 读 `pre_init.c:35-75`，确认删除扫描起点恒为 `p = bigbuf`。
2. 按引文逐字节重放极小场景：缓冲区 `xboard=2\0arch=i386\0\0`（下标 0–8 为首条记录、9–18 为 `arch=i386`），执行 `mb_set_param(buf, "board", …)`：p=0 处 `'x'≠'b'` 失配；内层 `while(*p++)` 越过下标 8 的 NUL 后 p=9（`'a'`），独立 `p++` 后 p=10（`'r'`）；`strncmp("rch=i386","board",5)` 失配；继续跳到缓冲区末尾，无删除、无伪记录。
3. 再重放补全前提后的场景：缓冲区 `a=1\0xboard=2\0arch=i386\0\0`，写 `board=`：p=0 失配，越过 `a=1\0`（NUL 在下标 3）后 p=4（`'x'`），`p++` 后 p=5（`'b'`）——此处 `board=2` 命中被删，右移把 `arch=i386` 的前半截补进原位，得到 `a=1\0xarch=i386\0\0`，与文档所述一致。前提补上后结论成立，说明缺陷只是前提缺失而非机制写错。

---

## SUGGESTION

### SUGGESTION S-01：§2.4.1.3 退化调用分析可补一句 `mem_high_phys` 的副作用

**位置**：§2.4.1.3（L712）与 §2.4（L552）："0 < len < 4096 的退化调用会在 rounddown 后变成 0 并被写成一个零长度条目（它会把 mmap_size 推高一格……）"；"`mem_high_phys` 只增不减：它是所有条目的最高地址上界（addr + len）"。

**依据**：`minix3/minix/kernel/arch/i386/pg_utils.c:86-121`（`add_memmap`）。`len == 0` 的提前返回在 `rounddown` **之前**（L98 在 L100 前），所以 `0 < len < 4096` 的调用会走到写条目分支，且随后 `highmark = addr + len`（L112）在 len 已为 0 时等于 `addr` 本身——零长度条目仍会把 `mem_high_phys` 抬到自己的（上取整后）基址，进而影响 `pg_identity` 的 `PWT|PCD` 分界比较（`pg_utils.c:178-179`）。

**建议**：§2.4.1.3 讲退化调用后果时补一句"零长度条目还会经 `highmark = addr + len` 把 `mem_high_phys` 抬到该基址"（后果温和：只影响非缓存分界方向，偏保守），使该处的穷举与 §2.4 "mem_high_phys 只增不减"一条完全闭合。属完整性增强，非错误。

### SUGGESTION S-02：§2.4.1.1 表格"或数组末尾"措辞可注明 full-array 例外

**位置**：§2.4.1.1 表（L681）："右余量落点：m 之后第一个空槽；若 m+1 已被占用则落到更后空槽或数组末尾"。

**依据**：`pg_utils.c:104-121`——若数组满（`mmap_size == MAXMEMMAP` 且 40 槽全非零），切除留下的第二个余量在 `add_memmap` 的 `assert(cbi->mmap_size < MAXMEMMAP)`（L97）处当场 panic，不存在"写到数组末尾"的路径（`cut_memmap` 清槽不改 `mmap_size`）。

**建议**："数组末尾"若意指"越过当前 `mmap_size` 高水位线"，可改为"高水位线之后的首个空槽（把 `mmap_size` 推高一格）"；并注明数组满时该余量触发 `add_memmap` 的断言停机。属措辞精度，非错误。

---

## 已验证无误的断言（抽样对账表，证明覆盖密度）

以下逐条与源码核对通过（抽样列出；全部核对项约 190 条，无一例 P0/P1）：

| # | 文档断言（节） | 源码证据 | 结果 |
|---|---|---|---|
| 1 | §2.1 引文：multiboot_magic 块（.balign 8 起至 multiboot_depth）逐字 | `minix3/minix/kernel/arch/i386/head.S:43-66` | 一致 |
| 2 | §2.1 `MULTIBOOT_HEADER_MAGIC=0x1BADB002` / `MULTIBOOT_INFO_MAGIC=0x2BADB002` / `MULTIBOOT_MAX_MODS=20` | `sys/arch/i386/include/multiboot.h:40,86,244` | 一致 |
| 3 | §2.1 HAS_VBE=0x00000004；五字段同注 HAS_ADDR；struct 注释 "Valid if mh_flags sets MULTIBOOT_HEADER_HAS_VBE" | multiboot.h:43,57,64 | 一致 |
| 4 | §2.1 HAS_MODS=0x00000008 / HAS_MMAP=0x00000040 / HAS_MEMORY=0x00000001 | multiboot.h:87,90,93 | 一致 |
| 5 | §2.1 mm_size 注释（"except itself…count mm_size + 4"）；mmo_end 无注释 | multiboot.h:208-210,224-229 | 一致 |
| 6 | §2.1 boot2.c 命令表含 menu/load_mods/multiboot；command_load_mods 转 module_add | `sys/arch/i386/stand/boot/boot2.c:146,151,153,453-458` | 一致 |
| 7 | §2.1 module_add 只登记路径不读文件 | `sys/arch/i386/stand/lib/exec.c:163-198`（bm_path/bm_next 链表） | 一致 |
| 8 | §2.1 exec_multiboot 先查 boot_modules_enabled（默认 true）再 module_init，758 置标志、759-760 填 count/addr；空链表仍建表、条数 0 | exec.c:125,742-760；module_init exec.c:565-573 无条件 alloc + num=0 | 一致（反序诊断断言分析成立） |
| 9 | §2.1 assert 打印形如 `assert "…" failed` 不打值；libsys 版走 panic，libc 版走 abort | `minix/lib/libsys/assert.c:15-24`；`lib/libc/gen/assert.c:50-60` | 一致 |
| 10 | §2.2 引文 multiboot_init 逐字（含 push 次序）；入口 esp=top−16≡0；head.S:86 push %eax；load_stack 在 93-96 行 .data 末尾 | `head.S:68-96` | 一致 |
| 11 | §2.2 构建链：Makefile.inc:51 head.o 首位、83 行 OBJS.kernel、objcopy --prefix-symbols；kernel.lds:18 .unpaged_data；load_stack_start=段起点+4096 | `Makefile.inc:51,64-69,83`；`kernel.lds:17-20` | 一致 |
| 12 | §2.2 i386 SysV 入口 %esp≡12 要求、当前布局给 ≡0、差一个 push | psABI 语义推演 + head.S 布局 | 一致 |
| 13 | §2.2 loadfile_elf32.c 按 LOAD_BSS 清 BSS；LOAD_KERNEL=LOAD_ALL&~LOAD_TEXTA 含 LOAD_BSS | `sys/lib/libsa/loadfile_elf32.c:360`；`sys/arch/x86/include/loadfile_machdep.h:35`；`sys/lib/libsa/loadfile.h:49,53` | 一致 |
| 14 | §2.3 get_parameters 引文逐字；user_sp 先占位后被 USR_STACKTOP 覆盖；kmess=kmessages 宏别名（glo.h:35）；kernel_may_alloc 定义/声明处 | `pre_init.c:94-163`；`main.c:156`；`kernel/glo.h:24,35,76` | 一致 |
| 15 | §2.3 mb_set_param 引文逐字；cttyline/cttybaud 唯二专用键；三处调用点忽略返回值；arch= 无读者 | `pre_init.c:35-75,144,149-150`；`const.h:137-140`；grep ARCHVARNAME 仅写点 | 一致 |
| 16 | §2.3 删除扫描 off-by-one（第二条记录起匹配从第二字符开始）字节级推演 | pre_init.c:56-58 重放 | 一致 |
| 17 | §2.3 param 落链：do_getinfo.c GET_MONPARAMS 整块拷 param_buf；pm/main.c:169 起点；sysenv.c:59 PMGETPARAM；mib/kern.c:96,103,108；kernel 无 rootdevname 读者；ramdisk rc 拼前缀/"rootdevname not set" | `kernel/system/do_getinfo.c:143-145`；`servers/pm/main.c:169`；`commands/sysenv/sysenv.c:59`；`servers/mib/kern.c:96-108,392`；`drivers/storage/ramdisk/rc:56-60` | 一致 |
| 18 | §2.3 get_value 从头扫返回首匹配；kmain 用 env_get(BOARDVARNAME) 取板卡；env_get 另服务 verbose/ac_layout/no_apic 等 | `kernel/main.c:481-507,133,414,421,443` | 一致 |
| 19 | §2.4 内存收集段引文逐字；E820 查询在 biosmemx.S（movl $0xe820,int $0x15）；AVAILABLE=1；步长 mm_size+4 | `pre_init.c:173-188`；`biosmemx.S:116-118`；`multiboot.h:243,208-210` | 一致 |
| 20 | §2.4 add_memmap/cut_memmap 引文逐字；LIMIT 0xFFFFF000；两条 assert+提前返回次序；roundup/rounddown 方向；0<len<4096 退化写零长度条目；mmap_size 语义与空洞；手算样例 [0x400000,0x402000) 全部数字 | `pg_utils.c:32-121` 重放 | 一致 |
| 21 | §2.4 kernel_may_alloc=1/0 的两处写点与注释、switch_to_user 前两行（隔一空行）；.bss 清零两条装载路径 | `pre_init.c:33`；`main.c:105-107,142`；`arch_system.c` 无写点 | 一致 |
| 22 | §2.4.1 alloc_lowest/pg_alloc_page 引文逐字；第三参数错误（len 当 end）三行表；第三行常态四前提（main.c:147/311、arch_init 在 cstart 体内 main.c:474、BIOS 区 arch_system.c:277-278 与 memory.h 常量、GDT_SIZE=6+CPUS（archconst.h:19-21）、IDT 2048B、tramp≈2212B、唯一调用 arch_smp.c:76） | `pg_utils.c:65-84,138-160`；`arch_smp.c:74-76`；`main.c:147,311,474`；`arch_system.c:246,277-278`；`memory.h:29,33,118-119`；`kernel.h:8` | 一致 |
| 23 | §2.4.1.4 VM 只读 MIN(MAXMEMMAP,NR_MEMS)=16 条；"silently ignored" 注释 | `servers/vm/utility.c:55-59`；`const.h:98`；`param.h:13` | 一致 |
| 24 | §2.4.2 arch_boot_proc 仅 VM 分支还镜像（add_memmap+置零+vm_allocated_bytes，protect.c:445-454）；还引导段在 system_init 之后（main.c:295-301）；两本账（vm/main.c:485-519 first-fit 由 alloc.c:244 佐证） | `protect.c:405-454`；`main.c:294-301`；`servers/vm/main.c:485-519` | 一致 |
| 25 | §2.5 overlaps 引文逐字；-1 相切误报推演（含方向与"只发现一次"）；overlaps 守内容不守账本；image_end 584/595/617 行 | `pre_init.c:77-92,190-214`；`exec.c:584,595,617,753` | 一致 |
| 26 | §2.6.1 pre_init 引文逐字（八条语句）；pg_identity/pg_mapkernel/vm_enable_paging 引文逐字；961=0xF0400000/4MB；_kern_offset=0xF0000000；覆盖推演；PWT|PCD 方向例（0x2001000）；SMEP/SMAP 不被 vm_enable_paging 触碰（CR4 只清 PGE|PSE，pg_utils.c:224）；位序（PG→PSE→PG→WP→PGE） | `pg_utils.c:162-245`；`kernel.lds:4-6` | 一致 |
| 27 | §2.6.2 alloc_pagetable 引文逐字（PG_PAGETABLES 6、panic、vir2phys 双副本）；pg_map 引文与 mapped_pde/pt 静态前提（pg_utils.c:270-302）；PG_ALLOCATEME=((phys_bytes)-1)（archconst.h:151）；rg 只有 protect.c:381 实参、pg_utils.c 三处消费；protect.c:33-39 减 offset 版 | `pg_utils.c:123-134,267-310`；`protect.c:33-39,381` | 一致 |
| 28 | §2.6.3 memory_init/createpde 引文逐字；freepdes[0]=961+k；lin_lin_copy 借 0/1 号槽+read_cr3 断言+reload_cr3；vm_memset 借 0 号槽；mem_clear_mapcache memory.c:35、do_vmctl.c:164；do_copy/do_safecopy/do_memset/do_safememset.c 存在 | `memory.c:69-117,149-192,526-559,707-717`；`system/do_vmctl.c:162-164` | 一致 |
| 29 | §2.6.4 双副本论（Makefile.inc pg_utils.o 双列+kernel.lds 分区）；prot_init 四步 protect.c:360-363、返回值丢弃；arch_post_init/pg_info 引文逐字；pt_init BIGPAGE 跳过引文逐字（pagetable.c:1282-1286） | `protect.c:33,355-363,381-390`；`vm/pagetable.c:1030-1031,1282-1286` | 一致 |
| 30 | §2.6.5 vm.h 常量表 17 项+三宏+CR0/CR4 五位逐值核对；WP 行注释错误（"Enable paging" 与 PG 行逐字同）属实 | `minix/include/arch/i386/include/vm.h:7-63`（47-48 行注释复现） | 一致 |
| 31 | §2.7 earm/head.S 引文逐字（.startup/MINIX/multiboot_init/load_stack .data 末尾）；setup_mbi 引文逐字（MODS/MEM_MAP/CMDLINE、MB_MODS_ALIGN=0x00800000、-ARM_PAGE_SIZE、单条内存表）；earm overlaps 闭区间引文逐字；kernbase=0x80200000 → 0x1E00000 相切例成立；earm 无尺寸断言；vm_enable_paging TTBCR/DACR(0x55555555=16 域 Client)/SCTLR.M+I/D cache+分支预测；dcache_clean 在 pre_init（L404）；1MB Section | `earm/head.S:21-55`；`earm/pre_init.c:180-216,162-178,374-413`；`earm/kernel.lds:4-6`；`earm/pg_utils.c:158-246` | 一致 |
| 32 | §1 mpx.S:634 手写 LABEL(__k_unpaged_k_initial_stktop)；head.S:84 引用无前缀；Makefile.inc objcopy 前缀改写 | `mpx.S:634`；`head.S:84`；`Makefile.inc:64-67` | 一致 |
| 33 | §1 D4：head.S:86 push %eax（同名不同处：引文内 push %eax 是 pre_init 传参） | `head.S:76,86` | 一致 |
| 34 | §1 五个栈顶名字表 C 侧三行（load_stack_start/k_initial_stktop 归属与用途） | `head.S:69,84,93-96`；`mpx.S:634` | 一致 |
| 35 | §2.1 rootdevname：update_bootcfg.sh:96 sed 原样、99 行注释"changed during setup"；boot.cfg.default:4 首条 menu 行逐字 | `minix/commands/update_bootcfg/update_bootcfg.sh:96,99,123`；`etc/boot.cfg.default:4` | 一致 |

## 自检

- **范围自检**：第 2 章 §2.1–§2.8 全部读过（L162–L1390）；§1 的 C 引用断言（阶段表、D1–D5、栈顶名字表、边界声明、命名陷阱）逐条核对；§3–§7 的 Rust 侧断言不在本角色范围，未审。
- **最弱项自检**：(1) C 覆盖完整性——第 2 章点名的每个 C 文件（head.S、pre_init.c、pg_utils.c、memory.c、protect.c、arch_smp.c、arch_system.c、main.c、trampoline.S、mpx.S、earm 三件、multiboot.h、vm.h、const.h、param.h、glo.h、archconst.h、boot2.c、exec.c、assert.c×2、loadfile 三件、biosmemx.S、vm/utility.c、vm/pagetable.c、vm/main.c、vm/alloc.c、do_getinfo.c、do_vmctl.c、do_copy/do_safecopy/do_memset/do_safememset.c、update_bootcfg.sh、sysenv.c、mib/kern.c、ramdisk/rc、kernel.lds×2、Makefile.inc）均实际打开或 grep 过；(2) 15 处整段代码引用均做了逐字比对，未发现增删改（除标注的省略号处）；(3) 分析性推演（删除扫描字节级、OVERLAP 相切、add/cut 手算、第三行常态、覆盖推演）均独立重放；(4) 未确认项——无（本次未出现需要标"待确认"的点）。
- **矛盾处理**：核对中曾疑似"arch_system.c 切除在 278-279 行"，经 `grep -n` 复核为 277-278，文档正确（见自检记录，不构成发现）。
- **Rule Discovery**：未发现新模式（本文档 C 事实层的准确性显著高于一般水平：连行号级引用与注释级引用都能逐字对上）。
