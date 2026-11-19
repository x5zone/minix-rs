# RD34 事实审查报告（P-FACT-A，只审 C 侧）

## 基线核对

- 目标文档：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- `wc -l` = **3565**（与要求一致）
- `sha256sum` = `d909c236a373636678e66b13c0ad5ebde769ead738f09d81a846803647fcb98c`（前缀 `d909c236a3736366` 与要求一致）
- 审查范围：只核对文档中引用 Minix3 C 源码 / 汇编 / 链接脚本 / 构建文件的事实断言。Rust 侧断言、文风与设计判断不在本报告范围。
- 结论先行：C 侧引文整体忠实；发现 1 条 P1、2 条 P2、3 条 P3。机械门除 `doc-style-lint.sh --check` 的调用方式不在脚本用法内（见下）外全部通过。

## 机械门读数（仓库根执行）

| 门 | 命令 | 读数 | 判定 |
|---|---|---|---|
| 引文对账 | `python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `blocks=69 checked=29 failed=0 skipped=39 warned=1` | 通过（1 条 WARN 为第 1315–1327 行 ARM `overlaps` 节选，工具不判节选；本报告手工逐字核对通过） |
| 锚点解析 | `tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `anchors=105 resolved=105 zero-def=0 multi-def=0`，exit 0 | 通过 |
| 结构检查 | `python3 tools/doc-structure-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `✅ doc-structure-check: 1 个文件表结构与内部 .md 链接均可解析` | 通过 |
| 引用检查 | `tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `✅ doc-ref-check: ... 内部章节引用均可解析` | 通过 |
| 文风门 | `tools/doc-style-lint.sh --check <file>` | 脚本不认 `--check`（用法：`[--strict] FILE... \| --diff [RANGE] \| --dir DIR \| --self-test`）；改用 `tools/doc-style-lint.sh <file>` | `命中汇总：\| error级=0`，通过 |

## FINDING RD34-FACT-A-01（P1）

- 位置：文档 §2.3 第 465 行（另见第 365、457 行）；源码 `minix3/minix/drivers/storage/ramdisk/rc:9`、`:46`，`minix3/minix/commands/sysenv/sysenv.c:59`，`minix3/minix/servers/pm/main.c:169`，`minix3/minix/servers/pm/misc.c:374`，`minix3/minix/servers/pm/utility.c:57`
- 严重度：P1
- 问题：文档断言“`arch=` 仓内没有它的读者”“`arch=` 无读者”，但 RAMDS 的启动脚本 `ramdisk/rc` 在两处用 `sysenv arch` 读内核写入的 `arch=i386`/`arch=earm`，并据此决定启动哪些平台服务。
- 影响：读者会据此认为重复或外来 `arch=` 记录无害，且会把 `arch` 归入“只写不读”的参数类；实际上当命令行里的 `arch=` 不是 `param_buf` 第一条时，它会遮蔽内核写入的 `arch=`，`sysenv arch` 返回外部值，`ramdisk/rc:9`/`:46` 的 `Xi386`/`Xearm` 判断随之改变。§2.3 的“参数读者地图”也因此不完整——同一节已经为 `rootdevname` 画出了 `sysenv → PMGETPARAM → monitor_params` 链，`arch` 走的是同一条链。
- 证据或推理：`ramdisk/rc:9` 的文本是 ``if [ X`sysenv arch` = Xi386 ]``、`:46` 是 ``if [ X`sysenv arch` = Xearm ]``；`sysenv.c:59` `svrctl(PMGETPARAM, &sysgetenv)`；PM 在 `main.c:169` 用 `sys_getmonparams` 缓存 `kinfo.param_buf`，`PMGETPARAM` 分支在 `misc.c:374` 调 `find_param`（`utility.c:57-70` 扫描 `monitor_params`）。内核侧 `pre_init.c:149` 无条件写 `ARCHVARNAME`（`minix/const.h:138` 为 `"arch"`）。证据强度 E1（源码行 + 调用链）。
- 最小修复：把第 465 行“但仓内没有它的读者”改为“内核侧没有它的读者；用户态 RAMDS 的 `rc` 用 `sysenv arch` 读它（`ramdisk/rc:9`、`:46`）”，并在 §2.3 的落链图里为 `arch` 补一条同形读者；重跑 `tools/anchor-resolve.sh --check` 与 `doc-citation-check.py`。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：最可能的反例是“无读者”只限定在内核 `get_value`/`env_get` 的语境；但第 465 行写的是“仓内”，且 §2.3 的读者地图是以整个仓库为范围，故按字面仍为错。
- 对应判据：真值优先序“Minix3 C 源码行为 > 文档”；AGENTS.md 的 Doc Code Sync 七项（符号/行为核对）；本报告任务 2“常量、符号锚点、工作区前缀、测试计数”中的符号/行为项。

## FINDING RD34-FACT-A-02（P2）

- 位置：文档 §2.3 第 464 行；源码 `minix3/minix/kernel/arch/i386/pre_init.c:35-75`（`mb_set_param` 的删除扫描在 `:47-59`）
- 严重度：P2
- 问题：`xboard=2 → 再写 board= → 留下伪记录 xarch=i386` 这个例子缺少前提“`xboard=2` 不是首条记录”。按文档字面顺序（`xboard=2` 先有、`arch=i386` 后一条），删除扫描第一次落在 `xboard` 首字符、第二次落在 `arch` 的第二个字符，永远不会检查 `xboard` 中段的 `board=2`，因此不会删除、也不会产生 `xarch=i386`。
- 影响：读者按例子复现会得到相反结果，进而怀疑“off-by-one 的第二种表现”不成立；这正好削弱本节想论证的“删除扫描只按前缀匹配、不要求命中点在记录起点”这一真实行为。
- 证据或推理：把 `mb_set_param` 原样编成宿主程序（`/tmp/factcheck/mb.c`，只去掉 `kinfo_t*` 形参与两个专用键分支）：
  - 情形一（`xboard=2` 为首条，再写 `arch=i386`、`board=intel`）：记录为 `<xboard=2><arch=i386><board=intel>`，没有 `xarch`；
  - 情形二（先 `a=1`，再 `xboard=2`、`arch=i386`，最后 `board=intel`）：记录为 `<a=1><xarch=i386><board=intel>`。
  证据强度 E1（可复跑的精确代码复刻）。
- 最小修复：在第 464 行补上前提——“当 `xboard=2` 不是首条记录时（例如前面还有 `a=1`）”——或把例子改成 `a=1\0xboard=2\0arch=i386` 再写 `board=`；可同时补 `pre_init.c:47-59` 的锚点。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若文档的“先有 `xboard=2`”隐含“它不是首条”，则例子成立；但字面没有这层限定，且“后一条 `arch=i386`”反而把它读成第二条。
- 对应判据：真值优先序；Doc Code Sync 的行为断言核对；本报告任务 6“反事实前提是否真实存在”。

## FINDING RD34-FACT-A-03（P2）

- 位置：文档 §2.7 第 1240–1268 行（引文块 1245–1267，止于 `bx r2`）与 §3.5 第 1779–1808 行（引文块 1783–1807，第 1785–1787 行之间，1786 为空行）；源码 `minix3/minix/kernel/arch/earm/head.S:45-50`、`minix3/minix/kernel/arch/i386/kernel.lds:8-10`
- 严重度：P2
- 问题：两个以“代码引用”开头的块都省略了源码行却未标注省略。ARM `head.S` 块在 `bx r2` 后直接收尾，丢掉了源码第 48–50 行的 `/* not reached */ hang: b hang`；`kernel.lds` 块从 `_kern_offset = (...)` 直接跳到 `SECTIONS`，丢掉了源码第 8–10 行的三个 `__k_unpaged__kern_{offset,vir_base,phys_base}` 赋值（同块其余省略处都用了 `...`）。
- 影响：读者无法区分“函数到此为止”与“引文到此截断”；`kernel.lds` 那处省略还隐去了同一条前缀规则的另外三个导出（`__k_unpaged__kern_offset`/`__k_unpaged__kern_vir_base`/`__k_unpaged__kern_phys_base`），使 §3.5 的“前缀导出”在引文里只剩 `__k_unpaged__kern_unpaged_start/end` 两个样本。对照 §2.2 的 i386 引文块明确写了“节选，止于 `call pre_init`；其后的换栈与 `call kmain` 见 §2.2 第 6 条”，这两处不符合文档自己的引文惯例。
- 证据或推理：用逐行序列比对脚本（`/tmp/factcheck/cmp_block.py`，仅把单独一行 `...` 视为通配）：
  - ARM 块：`non-wild=23 gaps=2 unmatched=1`，第二个 gap 即 `MINIX:` 与 `b multiboot_init` 之间省略的第 27 行注释；另在块尾之外还有 3 行未标注（脚本按块边界不报）；
  - `kernel.lds` 块：`non-wild=23 gaps=3`，第一个 gap 是 `doc:1787 -> source 7..10`（三个 `__k_unpaged__kern_*` 赋值加空行），该处没有 `...`。
  证据强度 E1（脚本输出 + 源码行）。
- 最小修复：ARM 块头改为“代码引用（节选，止于 `bx r2`；其后的 `hang` 段不引）”；`kernel.lds` 块在 `_kern_offset = (...)` 后补一行 `...`；重跑 `python3 tools/doc-citation-check.py` 与锚点门。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：这两块都标了“代码引用”而非“整段全文”；若项目惯例允许不标尾部省略，则本条降为风格问题。但同篇 i386 块明确标注“止于”，且 D-045 要求节选只许省略、不许改写，故按同篇惯例仍应标记。
- 对应判据：D-045“节选引文只许省略不许改写”；AGENTS.md 引文/锚点纪律；本报告任务 3“节选是否标注了省略”。

## FINDING RD34-FACT-A-04（P3）

- 位置：文档 §4.3 第 2518 行；源码 `minix3/minix/kernel/table.c:44`
- 严重度：P3
- 问题：文档把 C 的 boot image 表写成 `struct boot_image image[]`，源码实际声明是 `struct boot_image image[NR_BOOT_PROCS] = {`。
- 影响：符号级引用不精确；按文档写法在源码里检索不到该声明，削弱“顺序按 C 表”这条可复核性。
- 证据或推理：`grep -n "struct boot_image image" minix3/minix/kernel/table.c` → `44:struct boot_image image[NR_BOOT_PROCS] = {`。证据强度 E1。
- 最小修复：把 `image[]` 改为 `image[NR_BOOT_PROCS]`。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：`image[]` 可能被当作“数组”的口语写法；但同段其余符号都写全名，按引用纪律应精确。
- 对应判据：Doc Code Sync 符号锚点项。

## FINDING RD34-FACT-A-05（P3）

- 位置：文档 §2.6.2 第 1072 行；源码 `minix3/minix/kernel/arch/i386/protect.c:33-39`
- 严重度：P3
- 问题：文档把 `vir2phys` 的“减 `_kern_offset` 版本”锚到 `protect.c:33-35`，但 33–35 只含函数签名与 `extern` 声明，真正的偏移计算在 36–38 行，函数在 39 行收尾。
- 影响：行号锚点落在函数内部但不覆盖被引行为；按锚点核对“减偏移”会先看到 `extern` 而看不到减法。
- 证据或推理：`sed -n '33,39p' minix3/minix/kernel/arch/i386/protect.c`：33 `phys_bytes vir2phys(void *vir)`、34 `{`、35 `extern char _kern_vir_base, _kern_phys_base;`、36–38 计算 `offset` 并 `return (phys_bytes)vir - offset;`。证据强度 E1。
- 最小修复：锚点改为 `protect.c:33-39`（或精确到 `:36-38`）。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若文档的行号只求“指向函数起点”，33 是可接受起点；但同篇其他 `file:line` 都指向被引行为，本条精度低于惯例。
- 对应判据：锚点纪律门（文件:行号必须可解析且指向被引内容）。

## FINDING RD34-FACT-A-06（P3）

- 位置：文档 §2.4 第 499 行（同一说法另见 §6.3 第 3428 行）；源码 `minix3/minix/kernel/arch/i386/pre_init.c:20`、`minix3/minix/kernel/arch/i386/kernel.lds:19`
- 严重度：P3
- 问题：文档说 `kinfo` “作为全局对象落在 `.bss`、初值为 0”。`pre_init.c` 属于 unpaged 对象，其 `kinfo` 的 `.bss` 输入节被链接脚本收进 `.unpaged_bss`，不是高半 `.bss`；同篇 §2.6.4/§3.5 已经区分 `.unpaged_bss` 与 `.bss`。
- 影响：读者可能把 `pre_init` 阶段读写的 `kinfo` 与 `kmain` 之后 `.usermapped` 的 paged 副本混为一谈；零初值结论仍成立，但段归属被写错。
- 证据或推理：`pre_init.c:20` `kinfo_t kinfo;`；`Makefile.inc:51-62` 把 `pre_init.o` 放进 unpaged 列表并 `objcopy --prefix-symbols`；`kernel.lds:19` `.unpaged_bss ALIGN(4096) : { unpaged_*.o(.bss COMMON) }`。证据强度 E1。
- 最小修复：改为“落在 `.unpaged_bss`（低段 bss 副本）”。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：对象文件内部的节名确实是 `.bss`；若作者按对象节名而非输出节名叙述，则字面不错。但上下文正在讨论链接后的段划分，按输出节名更准确。
- 对应判据：Doc Code Sync 的段/工作区前缀核对；§3.5 自身对 `unpaged` 段的定义。

## SUGGESTION

以下为措辞/精度偏好，不声称是缺陷，不计入 FINDING：

1. §2.1 第 239 行“`load_mods` …… 不读文件内容、不分配地址”：`module_add_common`（`exec.c:187-215`）确实用 `alloc()` 分配 `boot_module_t` 与路径字符串；这里的“不分配地址”若指“不为模块内容分配装载地址”则成立，建议写成“不为模块内容分配物理装载地址”，避免与 `alloc()` 混淆。
2. §2.1 第 239 行“`module_init`（它是 `exec_multiboot` 内部的一个步骤，不是另一个顶层入口）”：`module_init` 是 `exec.c:501` 的 `static` 函数，既被 `exec_multiboot`（`:743`）调用，也被 `exec_netbsd`（`:381`）调用。建议写成“由 `exec_multiboot` 调用的静态步骤”，以免“内部”被读成嵌套函数。
3. §2.4.2 第 750 行“`add_memmap(bootstrap_start, bootstrap_len)`”：实际签名是 `add_memmap(cbi, addr, len)`（`pg_utils.c:86`），调用实参是 `add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len)`（`main.c:301`）。建议补 `&kinfo` 或标“实参从略”。
4. §2.6.2 第 1090 行行内引用 `static u32_t *pt;`：源码是 `static u32_t *pt = NULL;`（`pg_utils.c:271`）。建议补 `= NULL`。
5. §2.4.1.4 第 740 行“`vm/utility.c` 的注释原文是 `Any memory chunk in excess of NR_MEMS is silently ignored`”：源码原注释是 `/* XXX Any memory chunk in excess of NR_MEMS is silently ignored. */`（`vm/utility.c:58`）。引用子串本身正确；若称“原文”，建议保留 `XXX ` 或改为“注释中的一句”。
6. §1.2 第 77 行表行“`add_memmap` / `cut_memmap`（`pre_init.c`、`pg_utils.c`）”：两个函数都定义在 `pg_utils.c`（`:86`、`:32`），`pre_init.c` 是调用点（`:182-187`、`:205-207`）。建议写成“定义在 `pg_utils.c`，调用点在 `pre_init.c`”，以免读成两个文件各定义一个函数。

## 覆盖率核对（任务 5）

文档第 2 章有逐行引文或专门解码的 C 侧对象（均可在源码逐条对上）：`multiboot_init`（`head.S`）、`get_parameters`、`mb_set_param`、`overlaps`（i386）、`pre_init`、`add_memmap`、`cut_memmap`、`alloc_lowest`、`pg_alloc_page`、`pg_identity`、`pg_mapkernel`、`vm_enable_paging`、`alloc_pagetable`、`pg_map`（入口段）、`memory_init`、`createpde`、`arch_post_init`、`pt_init`、`arch_boot_proc`（VM 回收段）、`copy_trampoline`、`setup_mbi`、`overlaps`（earm）、`pre_init`（earm，仅表格式对照）。这些对象都有对应说明或引文，未发现“声称逐行覆盖但无说明”的情况。

文档提到但只作一行说明/调用点/读者锚点、没有逐行展开的 C 对象：`pg_clear`、`pg_load`、`exec_multiboot`、`module_init`、`module_add`、`arch_init`、`lin_lin_copy`、`virtual_copy_f`、`vm_memset`、`pci.c:complete_bars`、`dmp_kernel.c`（`nr_procs/nr_tasks/release/version` 四个读取）、`procfs/root.c`（`nr_procs/nr_tasks`）、`do_copy.c`/`do_safecopy.c`/`do_memset.c`/`do_safememset.c`（系统调用入口）、`vm/alloc.c`（first-fit 注释）、`vm/utility.c`（NR_MEMS 注释）、`vm/main.c`（`mem_add_total_pages`/`free_mem`）、`table.c`（boot image 表）、`biosmemx.S`（E820 查询）、`arch_reset.c`（panic 输出，§6.3 用到但未引文件锚点）。这些在 §1.4 的下游交代清单与 §7.2 的“正文逐段引用的小文件不重复列出”里已被显式声明为不展开，**不构成本篇承诺的覆盖缺口**；其中 `arch_reset.c` 是 §6.3 第一行失败模式的实际行为来源，若后续要把 §6.3 的 C 行做成可回查，建议补 `arch_reset.c:118-130` 锚点（不在本轮判为缺陷）。

## 阴性声明（任务 7：扫了什么、发现数、N=0 时不覆盖什么）

1. **引文块逐字对账**：扫了全部带 `minix3/` 路径的 22 处引文头/块（含 `boot.cfg.default` 配置行与 1 处正文提及；实际代码块覆盖 `head.S`、`pre_init.c`、`pg_utils.c`、`arch_smp.c`、`memory.c`、`protect.c`、`vm/pagetable.c`、`earm/head.S`、`earm/pre_init.c`、`kernel.lds`）。发现内容不一致 **0**；发现标注问题 2 处（RD34-FACT-A-03）。**N=0 不覆盖**：工具 skip 的 39 块（含 Rust 块与工具无法定位的块，本报告未逐块核对其中非 C 内容）、引文块内由 `...` 声明省略且本报告未逐行展开的内容（其省略位置已用脚本核对与声明一致）、以及外部规范文本（Multiboot 前 8KB、SysV 栈对齐）。
2. **锚点与路径**：扫了机械门解析的 105 个 `file:symbol` 锚点与文档中显式写出的 39 个具体 `minix3/` 文件路径（另有目录前缀与通配写法，未逐 token 计数）。解析失败 0、零定义 0、多定义 0、路径缺失 0。**N=0 不覆盖**：不是 `file:symbol` 形式的纯文字符号（如 `__k_unpaged_` 机制讨论）、Rust 侧锚点。
3. **常量与位掩码**：扫了本篇引用的 40+ 个 C 常量/位值（`MULTIBOOT_*`、`MAXMEMMAP`、`MULTIBOOT_MAX_MODS`、`MULTIBOOT_PARAM_BUF_SIZE`、`LIMIT`、`I386_VM_*`、`I386_CR0_*`、`I386_CR4_*`、`I386_PAGE_SIZE`、`I386_BIG_PAGE_SIZE`、`PG_ALLOCATEME`、`MB_MODS_ALIGN/BASE`、`MB_MMAP_*`、`USR_*`、`NR_MEMS`、三个链接期基址）。与源码不一致 **0**。**N=0 不覆盖**：只由外部规范定义、仓库内无定义的值（Multiboot 头扫描窗口 8KB、SysV ABI 16 字节对齐），以及依赖运行期观测的数值（E820 样本最低条目 0x1000、跳板 2212 B、探针读数）——这些文档已标为观测/近似，静态核对无法证伪。
4. **函数签名、参数个数与返回类型**：扫了 23 个在文档中有签名或逐行引文的 C 函数（`get_parameters`、`mb_set_param`、`overlaps`、`pre_init`（i386 与 earm）、`add_memmap`、`cut_memmap`、`alloc_lowest`、`pg_alloc_page`、`pg_identity`、`pg_mapkernel`、`pg_clear`、`pg_load`、`vm_enable_paging`、`alloc_pagetable`、`pg_map`、`memory_init`、`createpde`、`arch_post_init`、`setup_mbi`、`copy_trampoline`、`arch_boot_proc`、`lin_lin_copy`）。不一致 **0**。**N=0 不覆盖**：Rust 侧签名、宏的展开后参数类型，以及只以调用语句出现、文档未给签名的 `vm_memset`/`virtual_copy_f`。
5. **行号锚点**：扫了文档中 25 处 `file:line` / `file:NNN 行` 形式的行号断言（`head.S:86`、`head.S:93-96`、`Makefile.inc:51`、`Makefile.inc:83`、`kernel.lds:18`、`exec.c:584/595/617/759-760`、`arch_system.c:246/277-278`、`arch_smp.c:76`、`main.c:128-129/147/311`、`pg_utils.c:19/270-302`、`protect.c:33-35/360-363/381`、`memory.c:35`、`do_vmctl.c:164`、`pm/main.c:169`、`vm/main.c:482-520`、`earm/head.S:20-27`）。不精确 **1** 处（RD34-FACT-A-05，`protect.c:33-35`）。**N=0 不覆盖**：引文块内部的行号（以逐字比对代替）、只给符号不给行的锚点。
6. **行为合同与反事实前提**：扫了 9 组 C 侧行为/反事实断言（push 对调、删除扫描 off-by-one、粗档 E820 退化、`alloc_lowest` 传长度、去掉 `OVERLAP` 的 `-1`、`pg_identity`/`pg_mapkernel` 反排、`pg_mapkernel` 返回最后已用槽、ARM 闭区间相切、`board=` 删除后的遮蔽）。前提不成立/例子写错 **1** 处（RD34-FACT-A-02）。**N=0 不覆盖**：需要真机或反汇编才能确认的结论（`pre_init` 是否生成 SSE 栈对齐访问、`alloc_lowest` 误切是否“从未”暴露、跳板生命周期），文档已用“观测/推断/未证明”标注，本报告不把它们升级为事实；§3.5 的 `bootstrap_len` 误设反事实属 Rust 历史，不在 C 侧范围。
7. **C 侧覆盖率**：扫了文档第 2 章声称覆盖的 C 函数清单（见上“覆盖率核对”）。声称逐行覆盖但无说明 **0**；只作调用点/读者锚点、已在 §1.4/§7.2 声明不展开的对象 **22 个**（见上段清单），不计为缺口。**N=0 不覆盖**：第 2 章之外只作 C→Rust 对照行的函数（§4.9 的对照表不承诺逐函数展开）。
8. **C 侧测试计数**：本篇的测试计数全部是 Rust 侧（§5.2.1 的 `cargo test` 转录、§5.1 的测试条数），**没有** C 侧测试计数断言；本项 N/A，未扫。

## 未展开根因

无。本轮共 6 条 FINDING（1 P1 / 2 P2 / 3 P3），根因分别独立，未达到 40 条阈值，故不设“未展开根因”清单。

