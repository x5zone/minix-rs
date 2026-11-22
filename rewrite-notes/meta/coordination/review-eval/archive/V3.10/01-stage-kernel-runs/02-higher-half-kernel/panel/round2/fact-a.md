# D02 R2 确认轮 · P-FACT-A（C 侧断言独立事实审查）

- 目标文档：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 基线核对：`wc -l` = 1280；`sha256sum` = `0888ffb0bbb27b4f67560058c91e99e0749addcc72868e41130d44ceb3a557b2`。与任务给的 1280 行 / 前缀 `0888ffb0bbb27b4f` 一致，未换稿。
- 读取范围：目标文档、`minix3/` 下被文档引用的源文件、冻结决策清单 `round1-decisions-snippet.md`、只读工具。未读历史评审、本 run 顶部 V1 段、R1 轨迹、其他面板输出、expected/。
- 证据分级（本报告内）：E1 = 源码直证（文件:行）；E2 = 工具/等价复现输出；E3 = 推理。
- 射程：只审 C 侧断言（Minix3 C 源码 / 汇编 / 链接脚本 / 构建文件）。Rust 侧断言不在本报告射程内。

## 0. 机械门读数（任务 0）

| 门 | 命令 | 退出码 | 读数 |
|---|---|---|---|
| 引文对账 | `python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | 0 | `blocks=32 checked=1 failed=0 skipped=31 warned=0`；唯一被检查的块是 597–610（Rust `plan_kernel_mapping`） |
| 锚点解析 | `tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | 0 | `anchors=56 resolved=56 zero-def=0 multi-def=0` |
| 结构门 | `python3 tools/doc-structure-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | 0 | `✅ doc-structure-check: 1 个文件表结构与内部 .md 链接均可解析` |
| 引用门 | `tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | 0 | `✅ doc-ref-check: … 内部章节引用均可解析` |

引文门的覆盖面必须写清：`doc-citation-check.py` 的 `PATH_RE` 只匹配 `os/**/*.rs`，本篇 9 个 C 引文块（`minix3/**`）全部落在 `skipped=31` 里，机械门对 C 侧引文的覆盖是 0。下文的 C 引文逐字结论全部来自手工对照 `minix3/` 源文件，不是该门给的。

### 锚点/行号审计（任务 1）

- 14 个 C 函数/标签锚点全部解析到预期定义（`anchor-resolve` 退出 0）：`head.S:multiboot_init`（L24/L211→68）、`pre_init.c:get_parameters`（L48→94）、`pre_init.c:pre_init`（L207→217）、`pg_utils.c:pg_mapkernel`（L48/L192/L207/L593→186）、`stacktrace.c:util_stacktrace`（L229→17）、`exception.c:proc_stacktrace_execute`（L229→289）、`earm/head.S:multiboot_init`（L242→31）、`pagetable.c:pt_init`（L265→1088）、`mpx.S:k_initial_stack`（L305→632）、`main.c:kmain`（L310→115）。
- 文件级引用（`kernel.lds`、`Makefile.inc`、`const.h`、`sys.mk`、`asm.h`、`earm/kernel.lds`、`earm/Makefile.inc` 等）在文中是纯代码路径，不带行号；这些文件的断言我在下文的"已核实"表逐条给了实际行号。没有发现"路径存在但符号不在"的锚点。
- 结论：C 侧断言都能定位到源文件；函数锚点全部可解析，文件级断言靠手工行号补齐，不影响可核查性。

## 1. FINDING

### FINDING RD2-FACT-A-01（P1）

- 位置：L58（§1.2）、L319（§2.5 表）、L332（§3.1）、L404（§3.2）、L1084 与 L1092（§4.5）；依赖同一前提的反事实在 L126–L128。
- 严重度：P1（C 侧装载模型错误；不影响 Rust 设计结论，但使 §2.1 两条反事实与 §6.1 一条诊断行落在错误的装载者模型上）。
- 问题：文档把 C 侧的装载者写成 GRUB、并断言"段体摆放是装载器按 ELF 的 `p_paddr` 字段做的"（L58）、"C 侧读它们的是 GRUB"（L404）、"装载者从 GRUB 换成 boot-shim…表达式与读法不变"（L1084）。Minix3 树内实际的内核装载器（boot monitor）不读 `p_paddr`：它用 `LOADADDR(p_vaddr) = (p_vaddr & 0x07ffffff) + offset` 把 VMA 的低 27 位当作物理地址。`p_paddr` 在整条装载路径里只出现在 `internalize_phdr` 的字节序转换中（被复制，不被消费）。GRUB 是可选的外部 multiboot 装载器，不是"C 侧"的默认装载者；Minix 自己的 `multiboot` 命令同样走 `common_load_kernel` → `loadfile`，也不消费 `p_paddr`。
- 影响：对默认 C 启动路径，段体落点由 VMA 高位模式（`0xF04xxxxx` 掩成 `0x004xxxxx`）决定，不是 `AT()`/`p_paddr`。因此 L126（全常量 `AT`）与 L128（删掉 `AT`）两条反事实描述的不是默认 C 路径的失败方式；§6.1 L1181 的诊断行"多个 LOAD 的 `p_paddr` 相同 ⇒ 段体互相覆盖"在默认路径上也不成立。读者按本篇模型排查真实 Minix 启动会找不到对应机制。Rust 侧的 `AT()` 讨论本身不受影响（boot-shim 确实读 `p_paddr`）。
- 证据或推理：
  - E1：`minix3/sys/arch/x86/include/loadfile_machdep.h:40` `#define LOADADDR(a) ((((u_long)(a)) & 0x07ffffff) + offset)`；`:42` `READ(f, b, c) → pread(f, (void *)LOADADDR(b), c)`；`:44` `BZERO` 同样走 `LOADADDR`。
  - E1：`minix3/sys/lib/libsa/loadfile_elf32.c:336` `READ(fd, phdr[i].p_vaddr, phdr[i].p_filesz)`；`:363-364` `BZERO(phdr[i].p_vaddr + p_filesz, …)`；`p_paddr` 只在 `:164`/`:173` 的 `internalize_phdr` 里做 `I32/I64` 转换。
  - E1：`minix3/sys/arch/i386/stand/lib/exec.c:301` 用 `loadfile(file, marks, LOAD_KERNEL …)` 装载内核；`:369` `exec_netbsd`；`minix3/sys/arch/i386/stand/boot/boot2.c:260` `exec_netbsd(filename, 0, …)`（`offset` 因此为 0）；`:588-593` `command_multiboot` → `exec_multiboot`（`exec.c:713-787`）仍走 `common_load_kernel`。
  - E1（掩码复算）：`0xF0400000 & 0x07ffffff = 0x00400000`，与该段 `VMA − _kern_offset` 数值相同，所以默认路径"碰巧"与 `AT()` 结果一致，但推导来源不同。
  - E1（阴性）：`grep -rn "p_paddr" minix3/minix/kernel/` 计数 0——"内核自己一行解析代码都没有"这半句是对的；错的是"读它们的是 GRUB"这半句。
- 最小修复：在 §1.2 或 §2.1 补一句射程声明与事实：树内 boot monitor 走 `LOADADDR(p_vaddr)` 的低 27 位（引 `loadfile_machdep.h:40`），`AT()`/`p_paddr` 只对外部 multiboot 装载器（GRUB）成立；把 §3.2、§4.5 的"GRUB"改成"外部 multiboot 装载器（如 GRUB）"，并在 §2.1 两条反事实前加"以下按 GRUB 的 `p_paddr` 读法"。
- 置信度：0.9。
- 是否需源码核实：已核实（E1）。残余：未在 QEMU 里跑 Minix3 原生引导链复核，但掩码与调用链是源码直读。
- 若我错了：若本篇 C 侧明确只以 GRUB 装载为射程（例如上游另有约定），则不是事实错误，而是缺一句射程声明；但文中没有任何这样的限定，且 L58/L1084 的措辞是无条件的。
- 对应判据：真值优先序（Minix3 C 源码行为 > 设计/技术文档）；P0-fact 分类。

### FINDING RD2-FACT-A-02（P1）

- 位置：L126（§2.1 反事实），连带 L1181（§6.1 诊断表同一前提）。
- 严重度：P1（反事实的链接期结论错误；教学结论"逐段 `AT()`"仍成立，但失败形态完全不同）。
- 问题：文档断言"链接器不会为此报错——`AT()` 是合法表达式，段在虚拟地址空间里也不重叠"。实测 GNU ld 2.42 对 LMA 重叠直接报错并退出 1、不产出 ELF；gold 也以内部错误失败、不产出 ELF。其次，"`.bss` 是 `p_filesz = 0` 的段，不写字节，但清零范围同样从 0x00400000 起"与常规 ELF 布局不符：真实脚本里 `.data` 与 `.bss` 同属一个 RW `PT_LOAD`（`.bss` 是其中的 NOBITS 尾段），`.bss` 不是独立的 `p_filesz = 0` 段，清零起点也不是段首 0x00400000。
- 影响：读者按本篇预期去构造"全常量 `AT`"镜像，得到的是链接错误而不是"静默覆盖 + `kmain` 首指令变成别人的数据"；§6.1 的"内核镜像里某段的内容像另一段 ⇒ `AT()` 用了常量表达式"这条诊断在现行工具链上观察不到。反事实的结论方向（不要用常量 `AT`）仍对，但理由应从"静默覆盖"改成"链接期 LMA 重叠检查直接拒绝"。
- 证据或推理：
  - E2：等价脚本 `/tmp/ldsprobe/t3.ld`（`.unpaged_text` 在 0x400000，`. += 0xF0000000`，`.usermapped_glo`/`.usermapped`/`.text`/`.data`/`.bss` 全部 `AT(_kern_phys_base)`），`ld -T t3.ld -o t3b.elf a.o b.o c.o` 输出 `ld: section .text LMA [0000000000400000,0000000000400000] overlaps section .unpaged_text LMA [0000000000400000,0000000000400000]`（另一轮同脚本还报 `.usermapped_glo`/`.usermapped`/`.data` 的同类重叠），退出码 1，无输出文件；`gold -T t3.ld …` 退出码 1，`gold: internal error in segment_precedes, at ../../gold/layout.cc:3872`。
  - E2：正常 `AT` 形态的等价脚本 `/tmp/ldsprobe/t.ld`（`.data` 与 `.bss` 各带 `AT(ADDR − offset)`、中间有 4KiB 对齐）产出单个 RW `LOAD`：`FileSiz 0x000004  MemSiz 0x001010`，Section-to-Segment 映射为 `.data .bss`——`.bss` 不是独立 `p_filesz=0` 段。
  - E1：`minix3/minix/kernel/arch/i386/kernel.lds:14-20` 的 `.unpaged_text/.unpaged_data/.unpaged_bss` 占据 `0x00400000` 起的 LMA；任何把高侧五段 `AT` 设为 `_kern_phys_base` 的写法都必然与这些低侧段 LMA 重叠。
- 最小修复：把 L126 改成"现行 ld 在 LMA 重叠时报错（给命令与输出），因此该错法在链接期就被挡住；旧工具链若不检查 LMA 重叠，才会出现后写段覆盖前写段的镜像"；删掉或改写 `.bss` 的 `p_filesz=0` 括注（按合并段事实写成"`.bss` 是 .data 所在 RW 段的 NOBITS 尾段"）；同步修 §6.1 L1181 的诊断行。
- 置信度：0.85（依赖工具链版本；GNU ld 与 gold 在本环境均拒绝）。
- 是否需源码核实：部分。源码（lds 布局）已核实；链接器行为用等价脚本复现，未用 Minix 实际构建工具链跑真实 kernel.lds。
- 若我错了：若 Minix 固定的旧 `ld` 不检查 LMA 重叠且能产出镜像，覆盖推演在旧工具链上可能成立；但文档没有限定工具链版本，而现代 GNU ld/gold 都不沉默，且 `.bss` 合并段的结论与工具链版本无关。
- 对应判据：P0-fact 分类；反事实前提必须成立（真值优先序）。

### FINDING RD2-FACT-A-03（P2）

- 位置：L207（§2.2「把 `pg_utils.o` 从改名名单里去掉会怎样」及其"比如把 `pre_init.o` 去掉"括注）。
- 严重度：P2（§2.2 的主反事实不可实现；正面的改名机制与对象清单本身正确）。
- 问题：把 `pg_utils.o` 从改名名单里去掉，不会得到"这些函数落进高一侧 `.text`、分页开启前 CPU 跳过去取指"的故障。`pre_init.o` 本身在名单里，`unpaged_pre_init.o` 由 `objcopy --prefix-symbols` 产出，它的**未定义引用** `pg_clear/pg_identity/pg_mapkernel/pg_load/vm_enable_paging` 也被加前缀成 `__k_unpaged_pg_*`（E2 实测 objcopy 会改未定义符号）；而普通 `pg_utils.o`（`SRCS` 里有 `pg_utils.c`）只定义不带前缀的名字。两者不匹配 → 链接期未定义符号，链接失败。文档自己在两句话之后就为"反方向"写对了这条机制（"引用与定义前缀不匹配时链接期报未定义符号"），但没有把它应用到正方向。括注"比如把 `pre_init.o` 去掉"更不可实现：`pre_init.c` 不在任何 `SRCS` 里（只在注释里出现），把它从 `.for` 名单去掉就同时去掉了唯一生成 `pre_init.o` 的规则，`__k_unpaged_pre_init` 也变成未定义。
- 影响：§2.2 的"C 侧负担"主例教了一个到不了的故障形态；读者照做实验得到的是链接错误，不是三重故障。该段的正面结论（低侧对象必须进改名名单）不受影响，反而被"链接期 fail-fast"加强。
- 证据或推理：
  - E1：`minix3/minix/kernel/arch/i386/Makefile.inc:51-54` 名单含 `pre_init.o`、`pg_utils.o`；`:65-68` 的 `.else` 分支 `unpaged_${unpaged_obj}: ${unpaged_obj}` + `objcopy --prefix-symbols`；`:75-76` `SRCS+= … pg_utils.c`（普通副本存在），而 `pre_init.c` 不在 `SRCS`。
  - E1（阴性）：`grep -rn "SRCS.*pre_init\|pre_init.*SRCS" minix3/minix/kernel/ --include="Makefile*" | wc -l` = 0；`pre_init.c` 在 Makefile.inc 链里只出现在注释 `# the following is required by pre_init.c`。
  - E2：等价最小复现 `printf '.text\n.globl foo\nfoo: nop\n call bar\n'` 经 `objcopy --prefix-symbols=__k_unpaged_` 后 `nm` 显示 `U __k_unpaged_bar`（未定义引用被改名）。
  - E1：`minix3/minix/kernel/arch/i386/pre_init.c:230-234` 的五个调用就是 `unpaged_pre_init.o` 里被改名的引用；`proto.h:49-50` 的 `#define kmain __k_unpaged_kmain` 也说明这套"定义/引用两侧同时改名"是本节的成立前提。
- 最小修复：把反事实改成真实可发生的形态——"从名单里去掉 `pg_utils.o` 会在链接期得到 `__k_unpaged_pg_*` 未定义符号（`pre_init.o` 一侧的引用已加前缀，普通 `pg_utils.o` 只有不带前缀的定义）"；若要保留"函数落高半、分页前执行"的故障，必须同时把调用方也移出名单（那会把整条 pre-paging 路径一起移高，是另一个更粗的错法），并删掉 `pre_init.o` 括注。
- 置信度：0.9。
- 是否需源码核实：已核实（E1+E2）。残余：未用 Minix 的实际 BSD make 跑移除名单后的链接，但依赖规则与符号前缀链是直读。
- 若我错了：若构建系统在别处把 `pre_init.c` 加进 `SRCS` 并让普通 `pre_init.o` 入链，括注里的 `pre_init.o` 路径可能不同；但 `pg_utils.o` 这一主例的未定义引用结论与 `pre_init.c` 是否入 `SRCS` 无关，仍然成立。
- 对应判据：P0-fact 分类；文档内部自洽（§2.2 正/反方向同一机制应同型）。

## 2. 已核实为真的 C 断言（覆盖清单，非穷举）

以下逐条对照过源文件，未发现问题：

| 断言 | 位置 | 证据 |
|---|---|---|
| `_kern_phys_base=0x00400000`、`_kern_vir_base=0xF0400000`、`_kern_offset=0xF0000000`、三行别名、`ENTRY(__k_unpaged_MINIX)` | L22、L181-190、L203、L1207 | `kernel.lds:2,4-10`；`printf` 复算 `0xF0400000-0x00400000=0xf0000000` |
| 恰好五处 `AT()`：`.usermapped_glo/.usermapped/.text/.data/.bss` | L124、L126 | `grep -c ': AT(' kernel.lds` = 5（行 25/27/29/30/32） |
| `.unpaged_text` 无 `AT()` ⇒ VMA=LMA；`.unpaged_bss` 只收 `unpaged_*.o(.bss COMMON)`；`. += _kern_offset` 在 `.unpaged_bss` 之后 | L112、L306 | `kernel.lds:14-22,32` |
| `.usermapped_glo` 由 `usermapped_glo_ipc.S` 供料；`.usermapped` 由 3 个 `minix_ipcvecs_*` + 8 个全局对象供料 | L124 | `usermapped_glo_ipc.S:10`；`usermapped_data_arch.c:4,14,24`；`usermapped_data.c:4-15`（8 个符号，无 `#ifdef`） |
| 改名名单 9 显式 + 3 + 13 + 2 = 27；复算命令输出 13/3/2 | L155 | `Makefile.inc:8,14,19-23,51-54`；实跑三条复算命令得 13/3/2 |
| `objcopy --prefix-symbols` 同时改定义与引用 | L154 | E2 实测 `foo`/`bar` → `__k_unpaged_foo`/`U __k_unpaged_bar` |
| `mpx.S` 只出现在 `SRCS+=` 一行，不在名单；`mpx.o` 名无 `unpaged_` 前缀 | L159、L304 | `grep -n 'mpx\.S' Makefile.inc` 只有行 75（`mpx.o` 的依赖行是另一符号） |
| `mpx.S` 末尾 `.data/.bss` 切换、`k_initial_stack`/`k_boot_stack` 各 `.space K_STACK_SIZE`、`LABEL(__k_unpaged_k_initial_stktop)`；`K_STACK_SIZE=I386_PAGE_SIZE=4096` | L276-296 | `mpx.S:628-641`；`arch_proto.h:7`；`vm.h:7` |
| `LABEL(y) → _LABEL(_C_LABEL(y))`；`_C_LABEL` 在 `__ELF__` 下恒等；`_LABEL(x) → .globl x; x:`；`#define` 与宏名之间是制表符 | L298 | `asm.h:64-65,98-99,177`（`cat -A` 确认 `^I`） |
| ARM32：`_kern_phys_base=0x80200000`、`.startup` 入 `.unpaged_text`、`__k_unpaged__kern_unpaged_edata`/`_end`、名单差异（无 `io_inb/io_outb`、`get_bp.o`、compiler_rt、`${BSP_OBJS_UNPAGED}`） | L205、L242-259 | `earm/kernel.lds:4-6,17-22`；`earm/head.S:21,31-46`；`earm/Makefile.inc:8,21-23,56-63` |
| `USR_DATATOP` 默认 `0xF0000000`、`_MINIX_MAGIC` 下 `0xE0000000`、`USR_STACKTOP=USR_DATATOP`；`MKMAGIC=yes`/`MKASR=yes` 才定义 `_MINIX_MAGIC`，默认 no | L48 | `const.h:32-43`；`sys.mk:25-26` |
| `pg_mapkernel` 不置 `I386_VM_USER`；`pg_identity` 置了；两个 4MB 对齐断言；`0xF0400000/0x400000=961`；循环向上取整 4MB | L48、L203、L593 | `pg_utils.c:162-184,186-202`；实算 961 |
| `pre_init` 依次调用 `pg_clear/pg_identity/pg_mapkernel/pg_load/vm_enable_paging`，五者同在 `pg_utils.o`；返回 `kinfo_t *`；`prot_init` 在 `cstart`（main.c:403-411） | L207、L230、L593 | `pre_init.c:217-238`；`pg_utils.c:204,247,254`；`main.c:403-411` |
| `add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len)` 在 `system_init()` 之后，注释原文一致 | L310 | `main.c:294-301` |
| `pt_init` 复制循环跳过 `ARCH_VM_BIGPAGE` | L265 | `pagetable.c:1278-1286` |
| C 侧回溯器从帧指针链走、`hbp <= bp` 防环；`util_stacktrace`/`proc_stacktrace_execute` 同形 | L229 | `stacktrace.c:17-24`；`exception.c:289-320` |
| `head.S` 四行交接（设栈/终止槽/压参/`call`）、`mov $0,%ebp` 在换栈前、`hang: jmp hang`、`load_stack`/`load_stack_start` 4096 字节 | L213-223、L228-230、L265-269 | `head.S:68-96` |
| `pre_init` 的 `push %eax` 是 `kmain` 的第一个参数（i386 栈约定） | L230 | `pre_init.c:217,237`；`main.c:115` |

## 3. C 引文块逐字审计（任务 2）

9 个 C 引文块逐个对照，全部**逐字**（缩进、制表符、拼写错误 `returnd`/`Afther` 均保留），省略均以 `...` 或"节选"声明：

| 文档位置 | 源位置 | 结论 |
|---|---|---|
| L26-32 | `head.S:84-87` | 逐字 |
| L90-107 | `kernel.lds:14-29` | 逐字；`...` 替换 `.unpaged_data/.unpaged_bss` 两条完整定义（注解措辞见 S1） |
| L134-143 | `Makefile.inc:48-54` | 逐字 |
| L147-148 | `Makefile.inc:66-67` | 逐字（`.else` 分支） |
| L183-187 | `kernel.lds:8-10` | 逐字 |
| L194-201 | `pg_utils.c:193-201` | 逐字；循环体 `...` 已声明 |
| L213-223 | `head.S:79-87` | 逐字（含原注释） |
| L244-257 | `earm/head.S:36-46` | 逐字（含 `Afther` 原文） |
| L275-291 | `mpx.S:628-641` | 逐字；`LABEL(k_boot_stktop)` 行尾注释被删已由"注释有删节"声明（省略号问题见 S2） |

内联引文抽查：L48 `cbi->user_sp = USR_STACKTOP; cbi->user_end = USR_DATATOP;` 与注释、L190 `extern char _kern_vir_base, _kern_phys_base; /* in kernel.lds */`、L190 `static phys_bytes kern_vir_start = (phys_bytes) &_kern_vir_base;`、L593 两行断言、L310 `add_memmap` 注释、L298 `asm.h` 宏——均与源文件一致。

## 4. 阴性声明审计（任务 5）

| 阴性声明 | 位置 | 文档给的扫描依据 | 我的复核 | N=0 覆盖面 |
|---|---|---|---|---|
| `kernel.lds` 没有断言平移一致 | L124 | 无（只说"自己并没有断言它"） | `grep -c ASSERT kernel.lds` = 0 | 只覆盖 `ASSERT` 关键字；不覆盖其他形式的检查（本脚本确实没有） |
| `mpx.S` 不在改名名单、只进普通源码清单 | L159 | 给出 `.for` 名单与 `SRCS+=` 行 | `grep -n 'mpx\.S'` 仅行 75 | 覆盖文件名字符串；不覆盖 `mpx.o` 的依赖行（行 108/125，与改名无关） |
| ARM 名单没有 `io_inb.o/io_outb.o`、没有 `_cpufeature.o/_cpuid.o` | L205 | 给出 ARM 名单 | `grep -c "io_inb\|io_outb" earm/Makefile.inc` = 0；`MINLIB_OBJS_UNPAGED= get_bp.o` | 覆盖该 Makefile.inc；不覆盖 BSP 子 Makefile 是否另加 |
| C 内核没有 ELF 解析代码 | L404 | 无 | `grep -rn "ELFMAG\|Elf32_Ehdr\|p_paddr" minix3/minix/kernel/` = 0 | 只覆盖 ELF 解析标识符；不覆盖 multiboot 结构体解析（内核确实解析 multiboot info） |
| C 侧没有独立布局断言脚本 | L1088 | 无 | 未找到等价脚本，但未做全树穷举 | 未机器证明；"没有"应写明扫了什么 |
| `pg_mapkernel` 只有两个基址断言、没有镜像大小对齐断言 | L593 | 给出断言原文 | `pg_utils.c:191-192` 只有 `kern_vir_start`/`kern_phys` 两条 | 覆盖该函数；`kern_kernlen` 确无断言 |

前三条可接受（直接可复核的清单/文件读取）；后三条建议补一句扫描口径（见 S5）。

## 5. SUGGESTION

- S1（L91，引文注解精度）：块注解说"保留位置计数器起点、**三组 `.unpaged_*` 段**、两段 usermapped 与 `.text` 的头部；省略号处为 `.unpaged_data/.unpaged_bss` 的段体"，但块里只出现了 `.unpaged_text`，`...` 替换的是 `.unpaged_data/.unpaged_bss` 两条**完整输出段定义**（含段名与 `ALIGN`），不是"段体"。建议改成"保留 `.unpaged_text` 与两端符号；省略号处为 `.unpaged_data/.unpaged_bss` 两条输出段定义"。
- S2（L276，引文注解精度）：`mpx.S` 块注解写"省略号处为同文件此前的代码"，但块内没有可见的 `...`；块尾还省略了 `k_stacks_start/k_stacks_end`（同属栈定义）而未声明。建议改成"节选自文件末尾；块前代码与块后的 `k_stacks_*` 未摘录"，或补一个 `...` 行。
- S3（L310，行为描述精度）："`pg_mapkernel` 只映射 `[kern_vir_start, kern_vir_start + kern_kernlen)`"与 §3.5 L593 自己的"最后一块向上取整到 4MB"不一致；实际映射区间是 `[start, start + 4MiB*ceil(kernlen/4MiB))`，是所述区间的超集。建议与 L593 统一口径（"至少映射到 `+kern_kernlen`，末块按 4MB 取整"）。
- S4（L259，数值精度）：ARM 的"`kmain` 与调用点相距 `0xF0400000 − 0x80200000 = 0x70200000`"把基址差当成精确距离；实际距离还要加低侧长度与高侧段内偏移（x86 段 L231 已经写成"`0xF0000000` 量级"，这里可以同口径写成"约 `0x70200000`，量级远超 ±32MB"）。
- S5（L124/L404/L1088，阴性声明口径）：三处"没有/并没有"未给扫描依据。建议分别补 `grep -c ASSERT`、`grep -rn "ELFMAG\|Elf32_Ehdr\|p_paddr" minix3/minix/kernel/` 的输出，以及"C 侧断言脚本"的检索面（例如 `minix3/` 下 `check*layout*`/`assert*layout*` 的命中数），并写明 N=0 不覆盖什么。
- S6（L207，若保留该反事实）：即使按 FINDING-03 修复，也建议把"反方向"与"正方向"的链接期行为并列写成一张小表（引用加前缀/定义不加前缀 = 未定义符号；两侧都加 = 落低；两侧都不加 = 落高），这比单句"不能同型推演"更可核对。

## 6. 未覆盖 / 残余风险

- Rust 侧断言（`os/**` 的 loader、`higher_half`、`check-layout.sh`、`KernelInfo` 字段等）不在本报告射程；本篇引文门唯一检查的块（597–610）也只是"恰好被工具覆盖"，不代表 Rust 侧已全查。
- 未运行 Minix3 的 C 构建（本机无 Minix 工具链）：FINDING-02 的链接器行为用等价脚本复现，FINDING-01 的掩码链用源码直读；两者都建议在实际 `minix-ci` 或 QEMU 引导链上复核一次。
- 未审 01 篇 `01-boot-shim-bootstrap.md` 的 §1.4/§2.8/§5.2 转述（任务只允许目标文档与 `minix3/`）；本篇对 01 篇的交叉引用文字本身未核对。

