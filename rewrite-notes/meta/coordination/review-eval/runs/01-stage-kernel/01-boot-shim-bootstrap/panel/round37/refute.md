# 反证报告：01-boot-shim-bootstrap

> 角色：反证者。只读仓库源文件；本文件是面板产物，不是对仓库的修改。
> 读取范围：目标文档 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`、`rewrite-notes/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/related-code.txt` 列出的源码、设计合同 `.design/01-design.v4.md` 与 `.design/01-outline.v5.md`。C 源码只作为真值基线引用。没有读取作者结论性解释、历史评审、其他面板角色输出。
> 结论分级：**阻断 P0/P1**（主张与源码相反，或使核心不变量无法核验）、**非阻断 P2**（过宽、过期、遗漏、跨节不一致，但不推翻主结论）、**反证失败**（尝试构造反例后主张成立）。
> 方法说明：每条主张先引原文，再列尝试过的相反执行路径、失败路径、架构分支与时序条件；证据以 `文件:行号` 与原文短句给出。没有运行真机或 QEMU，所有结论都是源码层结论。

## CLAIM 01

### 主张

文档 §2.4.1.2–§2.4.1.3 主张：`alloc_lowest()` 把**长度** `len` 当作 `cut_memmap()` 的**结束地址**传入（原文：“`cut_memmap` 的第三个参数是结束地址……而这里传的是 `len`”）；在四个前提同时成立时，真实调用序落在“第三行”`len <= lowest`，切除一次都不发生，跳板占用的那一段仍留在空闲清单上；文档同时声明这不是结构保证。

### 反证尝试

1. **相反执行路径：是否存在第二个调用点，或调用发生在 `arch_init` 之前。**
   - `rg -n "alloc_lowest" minix3/` 只命中定义（`minix3/minix/kernel/arch/i386/pg_utils.c:65`）、原型（`minix3/minix/kernel/arch/i386/include/arch_proto.h:204`、`minix3/minix/kernel/arch/earm/include/arch_proto.h:37`）与唯一调用（`minix3/minix/kernel/arch/i386/arch_smp.c:76`）。ARM 侧只有原型，没有调用。
   - `arch_smp.c` 与 `trampoline.S` 只在 `CONFIG_SMP` 下进入构建（`minix3/minix/kernel/arch/i386/Makefile.inc:85` 起）。非 SMP 构建里 `alloc_lowest` 不会运行。
   - SMP 路径的时序：`kmain` 先置 `kernel_may_alloc = 1`（`minix3/minix/kernel/main.c:142`），再调 `cstart()`（`:147`）；`cstart()` 内调 `arch_init()`（`:474`），`arch_init` 末尾才是两条 `cut_memmap`（`minix3/minix/kernel/arch/i386/arch_system.c:277-278`）；`smp_init()` 在 `:311`，晚于 `cstart`。`copy_trampoline` 由 `smp_init` 之后的 AP 启动路径调用。没有更早的调用点。

2. **失败路径：`lowest == 0` 是否可达。**
   - `add_memmap` 对基址做 `roundup(addr, I386_PAGE_SIZE)`（`minix3/minix/kernel/arch/i386/pg_utils.c:99`），`0` 向上取整仍是 `0`；所以“仅靠 `add_memmap` 保证基址非零”不成立。
   - 但 `arch_init` 先切 `[BIOS_MEM_BEGIN, BIOS_MEM_END] = [0, 0x4FF]`（`arch_system.c:277`）；`cut_memmap` 把结束地址向上取整到 `0x1000`（`pg_utils.c:37-40`），回填的右余量从 `0x1000` 起。即使走粗档 `add_memmap(cbi, 0, mem_lower*1024)`（`pre_init.c:186`），这段也会被同一刀切掉。到 `alloc_lowest` 运行时，最低条目基址至少是 `0x1000`。`lowest == 0` 不可达。

3. **架构/配置分支：`roundup(tramp_size)` 是否可能大于 `lowest`。**
   - `tramp_size = (unsigned)&__trampoline_end - tramp_start`（`arch_smp.c:75`）。`trampoline.S` 的布局是 16 位代码 + `__ap_id`/`__ap_pt`/`__ap_gdt`/`__ap_idt` 共 24 字节 + `GDT_SIZE*DESC_SIZE` + `IDT_SIZE*DESC_SIZE`（`minix3/minix/kernel/arch/i386/trampoline.S:5-58`）。
   - `IDT_SIZE = 256`（`archconst.h:11`），`GDT_SIZE = TSS_INDEX(CONFIG_MAX_CPUS) = 6 + CONFIG_MAX_CPUS`（`archconst.h:21`）。默认 `CONFIG_MAX_CPUS = 1`（`minix3/minix/kernel/kernel.h:8`），`protect.c:212` 另有 `assert(CONFIG_MAX_CPUS <= 8)`；即使按 8 个 CPU 重算，`2048 + 14*8 + 代码` 仍远小于 4096。`roundup` 后是 `0x1000`，等于 `lowest`，仍落在第三行。

4. **反向构造：把 `cut_memmap(cbi, lowest, len)` 改成 `cut_memmap(cbi, lowest, lowest + len)` 才能使第一行/第二行成立。**
   - 源码明确是 `cut_memmap(cbi, lowest, len)`（`pg_utils.c:81`）。没有第二个版本。

5. **对文档从属主张的反例：非退化请求下取整是否真的不改变净账本。**
   - 取条目 `[0x1000, 0x3000)`，切 `[0x1001, 0x1FFF)`。带取整时切 `[0x1000, 0x2000)`，剩 `[0x2000, 0x3000)`。不带取整时左余量长度 `0x1` 被 `add_memmap` 的 `rounddown` 变成 0，右余量基址向上取整到 `0x2000`，净结果同样是 `[0x2000, 0x3000)`。第一页仍然出局。文档“只有退化请求才不同”的细分成立。
   - 退化请求 `[0x1001, 0x1001)`：带取整扩成 `[0x1000, 0x2000)`；不带取整时夹取后 `substart >= subend`，循环直接 `continue`，什么都不切。文档结论成立。

### 结果

**反证失败。** 主张与 `pg_utils.c`、`arch_smp.c`、`arch_system.c`、`main.c` 的实际代码一致；四个前提在当前源码下都成立，第三行 `len <= lowest` 在 `CONFIG_MAX_CPUS <= 8` 的范围内稳定成立。文档“不是结构保证”的限定也是必要的：它依赖 `arch_init` 的两刀与 `CONFIG_MAX_CPUS` 的上限。

可以补强的一点：文档只写“若改 `CONFIG_MAX_CPUS`，用同一式子重算”，没有引用 `protect.c:212` 的 `assert(CONFIG_MAX_CPUS <= 8)`。这条断言把“改配置后第三行是否会翻到第二行”的范围钉死了，引用它比“重算”更有力。

### 证据

- `minix3/minix/kernel/arch/i386/pg_utils.c:65-84`：`alloc_lowest` 全文，`:81` 是 `cut_memmap(cbi, lowest, len)`。
- `minix3/minix/kernel/arch/i386/pg_utils.c:32-61`：`cut_memmap`，`:37-40` 是起终点取整方向。
- `minix3/minix/kernel/arch/i386/arch_smp.c:68-95`：`copy_trampoline`，`:76` 是唯一调用。
- `minix3/minix/kernel/arch/i386/arch_system.c:246-279`：`arch_init` 末尾两条 `cut_memmap`。
- `minix3/minix/kernel/main.c:142,147,311,474`：`kernel_may_alloc`、`cstart`、`smp_init`、`arch_init` 的时序。
- `minix3/minix/kernel/arch/i386/trampoline.S:5-58`、`archconst.h:11,21`、`kernel.h:8`、`protect.c:212`：跳板尺寸与 CPU 上限。
- 文档原句：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:601-618`。

### 若我错了

如果存在一种构建，`arch_init` 的两条 `cut_memmap` 被跳过，或者 `CONFIG_MAX_CPUS` 被改到足以让跳板超过 4096 字节（按当前布局与约 90 字节的 16 位代码估算，约需 236 个以上 CPU，且要先去掉 `protect.c` 的 `<= 8` 断言），第三行就会翻到第二行；文档的“第三行常态”必须按新调用序重推。另一个可能：如果未来 `alloc_lowest` 增加第二个调用点，且它发生在 `arch_init` 之前，前提一失效。这两种情况在当前源码里都不存在。

## CLAIM 02

### 主张

文档 §2.3 主张：`mb_set_param()` 的“已存在先删”扫描只在待删键是缓冲区**第一条记录**时可靠；`while (*p++) ;` 越过记录末尾的 NUL 后已经让 `p` 落在下一条记录的首字节，紧接着独立的 `p++` 又多走一步，从第二条记录起匹配从记录的第二个字符开始，同名键再也匹配不上。因此 C 的“同名键最多一份”不变量只对首条成立。

### 反证尝试

1. **逐字节推演。**
   - 缓冲区 `A=1\0B=2\0\0`。第一轮从下标 0 开始，`"A"` 不匹配 `"B"`；`while (*p++) ;` 越过下标 3 的 NUL 后 `p = 4`，独立 `p++` 后 `p = 5`，正好是 `B=2` 的 `=`。第二轮 `strncmp(p, "B", 1)` 从 `=` 开始，失败。再写 `B=3` 时找不到旧的 `B`，追加成两条 `B`。文档推演正确。
   - 末尾双 NUL 不改变结论：扫描在最后一条记录后越过 NUL，再 `p++`，循环条件读到 0 结束；不会回头匹配。

2. **第一条记录的正常删除。**
   - 缓冲区只有 `B=2\0\0`，再写 `B=3`：第一轮 `p = 0` 就匹配 `B=`，删除后追加，只剩一条。文档说“待删键恰好是第一条记录时删除正常生效”，正确。

3. **架构分支：ARM 侧是否不同。**
   - `minix3/minix/kernel/arch/earm/pre_init.c:125-140` 是同一套 `while (*p++); p++;` 结构，缺陷同构。文档以 i386 为例，但结论可推广到 ARM，不构成反例。

4. **空键反例（找到一条文档未覆盖的失败路径）。**
   - 命令行扫描允许空键：如果某个词以 `=` 开头，`var` 的收集循环不前进，`var[0] = 0`，随后 `if (*p++ != '=')` 看到 `=` 为真，继续收集 `value`，最后调用 `mb_set_param(cbi->param_buf, "", value, cbi)`（`minix3/minix/kernel/arch/i386/pre_init.c:131-144`）。
   - 此时 `namelen = 0`。删除循环第一轮在下标 0 检查 `strncmp(p, "", 0) == 0` 为真，但 `p[0]` 是记录首字符，不是 `=`；`while (*p++); p++;` 把 `p` 推到第二条记录的第二个字符。若第二条记录是 `B=2`，`p` 落在 `=` 上，第二轮 `strncmp(p, "", 0) == 0 && p[0] == '='` 成立，删除从该 `=` 开始的内容。
   - 对 `A=1\0B=2\0\0` 调用 `mb_set_param(..., "", "v")`，结果是 `B=2` 被削成 `B`（值丢失），随后空键以 `=v` 追加。C 的账本被改写，而文档 §3.2 的十条可观察分歧之其八只记了“C 存成一条 `=value`”，没有记这个副作用。

### 结果

**主域反证失败；空键反例成立，属非阻断 P2。** 对非空键，文档的“只对首条成立”是准确的；对空键，`strncmp` 的长度为 0，匹配退化成“任意 `=`”，删除可以命中非首条记录的 `=`，因此“同名键再也匹配不上”这句话在空键域为假。这是文档没有覆盖的失败路径，不是主结论被推翻。

### 证据

- `minix3/minix/kernel/arch/i386/pre_init.c:35-58`：`mb_set_param` 的删除循环全文。
- `minix3/minix/kernel/arch/i386/pre_init.c:131-144`：命令行扫描允许 `=value` 形式的空键。
- `minix3/minix/kernel/arch/i386/pre_init.c:148-150`：解析后无条件写 `arch=`/`board=`，是重复键的另一条来源。
- `minix3/minix/kernel/arch/earm/pre_init.c:125-140`：ARM 同构循环。
- 文档原句：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:303-311`；空键分歧（其八）见 `:1119`。

### 若我错了

如果 C 的命令行扫描在进入 `mb_set_param` 之前会拒绝空键（源码没有这条过滤；`if (*p++ != '=')` 在空键时恰好通过），或者 `strncmp(p, "", 0)` 在目标 libc 里不被当作匹配（标准语义是长度 0 恒等），空键反例才不成立。我没有在真机上跑这条命令行，只按源码路径推演。

## CLAIM 03

### 主张

文档 §2.7 主张：ARM32 的 `overlaps` 是闭区间判定——`INRANGE` 的上界写成 `<= thismod->mod_end`，`OVERLAP` 第二项直接取 `mod2->mod_end`，没有 x86 版的 `- 1`；但 `MB_MODS_ALIGN = 0x00800000` 且每个合成模块的 `mod_end` 减了一页，所以模块—模块不会相切；内核项是例外，`mod_end = kernbase + kernsize`，它与 0 号模块的 30 MiB 间隔是构建事实（`0x82000000 - 0x80200000 = 0x1E00000`），不是结构保证；内核长到恰好 30 MiB 时会被误报。

### 反证尝试

1. **源码逐字核对。**
   - `minix3/minix/kernel/arch/earm/pre_init.c:167-169` 的宏与文档引文一致：上界 `<= thismod->mod_end`，第二项 `mod2->mod_end`，没有 `- 1`。`mod` 是宏形参、`thismod` 是外层循环变量，宏在展开点捕获；文档对这个读法的解释正确。
   - `setup_mbi` 在 `:199-203` 写 `mod_start = MB_MODS_BASE + i * MB_MODS_ALIGN`、`mod_end = mod_start + MB_MODS_ALIGN - ARM_PAGE_SIZE`；`MB_MODS_BASE = 0x82000000`、`MB_MODS_ALIGN = 0x00800000`（`:181-183`）。相邻模块之间恰好留一页，不会相切。
   - 内核项在 `:322-323` 追加，`mod_end = kernbase + kernsize`，没有减一页；`:327-341` 的循环对 `mods_with_kernel` 条记录逐条调用 `overlaps` 并 `cut_memmap`，内核项确实参与检查。

2. **架构分支：内核基址是否真的取 `_kern_phys_base`。**
   - `earm/pre_init.c:224-227` 声明并取址 `_kern_phys_base`、`_kern_size`；`earm/kernel.lds:4` 写 `_kern_phys_base = 0x80200000`。`0x82000000 - 0x80200000 = 0x1E00000` = 30 MiB。算式成立。

3. **边界反例：内核恰好 30 MiB。**
   - 若 `kernsize = 0x1E00000`，内核 `mod_end = 0x82000000`。检查模块 0 作为 `cmp`、内核作为 `thismod` 时，`INRANGE(kernel, mod0->mod_start = 0x82000000)` 的上界是 `kernel.mod_end = 0x82000000`，条件 `0x82000000 >= 0x82000000 && 0x82000000 <= 0x82000000` 为真，误报。文档的“若内核恰好长到 0x1E00000 字节会被误报”正确。
   - 反方向（`thismod = mod0`、`cmp = kernel`）也会在 `INRANGE(mod0, kernel.mod_end)` 命中。两个方向都说明这是相切误报，不是真重叠。

4. **结构保证反例：`earm/kernel.lds` 是否有尺寸断言。**
   - 在 `minix3/minix/kernel/arch/earm/kernel.lds` 中没有找到 `ASSERT`/尺寸上限；文档说“没有尺寸断言”成立。当前镜像远小于 30 MiB 是构建观察，不是链接期约束。

### 结果

**反证失败。** 宏、常量、模块间隔、内核项例外与边界行为都与源码一致；文档对“闭区间判定在当前尺寸下无害、但不是结构保证”的定性准确。

### 证据

- `minix3/minix/kernel/arch/earm/pre_init.c:162-171`：`overlaps` 与宏。
- `minix3/minix/kernel/arch/earm/pre_init.c:181-203`：`MB_MODS_*` 与 `setup_mbi`。
- `minix3/minix/kernel/arch/earm/pre_init.c:322-341`：内核项追加与逐条检查/切除。
- `minix3/minix/kernel/arch/earm/kernel.lds:4`：`_kern_phys_base = 0x80200000`。
- 文档原句：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:844-860`。

### 若我错了

如果 ARM 的内核物理基址在某个构建配置下不是 `_kern_phys_base`（例如重定位或高半装载改变 LMA），30 MiB 的算式会变；如果 `mod_end` 的语义不是“最后一个字节之后的地址”（x86 侧从 `exec.c:753` 的 `base + len` 读出，ARM 侧从 `start + ALIGN - PAGE` 读出），闭区间判定是否“无害”也要重算。当前源码下这两个前提都成立。

## CLAIM 04

### 主张

文档 §1.2 的 I4 行把可观测判据写成 `validate_before_tables` → `store_kernel_info` → `validate_requiring_sources`；§3.7 进一步写：“`check_boot_postconditions` 检查派生值（栈顶、用户栈顶、系统调用入口）与镜像/模块区间的半开不重叠”。这两句合起来主张：校验按“建表前 → 落存储 → 严格来源”的次序执行，且后置条件检查包含派生值与区间重叠。

### 反证尝试

1. **直接读 `check_boot_postconditions` 的函数体。**
   - `os/libs/minix-boot/src/kernel_info.rs:567-588`：函数体只有一条 `assert!(self.kern_virt_base.0 > u64::from(u32::MAX), ...)`。源码注释明确说 v10 删掉了 `user_sp`/`syscall_entry`/`kern_stack_top` 三条派生关系，“用派生值重算同一条等式只是重抄，不构成检查”。`check_boot_postconditions` 里没有“栈顶、用户栈顶、系统调用入口”，也没有“镜像/模块区间的半开不重叠”。

2. **区间重叠检查在哪里。**
   - `kernel_info.rs:351-405` 的 `validate_impl_inner` 里有“Kernel image vs module spans, and module-vs-module spans”一段，用半开区间判定并 `checked_add` 防回绕。它属于共享校验体，`BeforeTables`/`Standard`/`Strict` 三档都会跑，不是 `check_boot_postconditions` 的内容。

3. **实际腿序是否支持“落存储后再严格校验”。**
   - 直核面：`os/kernel-image/src/bootface_common.rs:558-561` 先 `build_kernel_info`，再 `kernel_info.validate_requiring_sources()`，最后 `minix_kernel::arch_boot`。`arch_boot` 进入 `arch_boot_finish`，后者才做 `validate_before_tables()` → `check_boot_postconditions()` → `store_kernel_info()`（`os/kernel/src/lib.rs:223-225`）。严格校验在落存储**之前**。
   - v1 接收面：`os/kernel/src/lib.rs:737-739` 先 `kinfo.validate_requiring_sources()` → `kinfo.check_boot_postconditions()` → `resume_boot_pt_alloc`；`store_kernel_info` 在后面的 `resume_additive` 里（`:320`）。严格校验同样在落存储之前。
   - `kmain`：`os/kernel/src/lib.rs:1856` 调 `kernel_info.validate()`，`:1865` 调 `check_boot_postconditions()`，不重新 `store`。所以“`validate_before_tables` → `store_kernel_info` → `validate_requiring_sources`”不是任何一条生产腿的执行序。

4. **尝试为文档辩护：把箭头读成“逻辑分层”而不是执行序。**
   - 即使接受这种读法，§3.7 对 `check_boot_postconditions` 的内容描述仍然与源码相反，而且与同篇 §4.4 矛盾：§4.4 的 `:2156` 明确写“v10 之后只剩一条……`kern_virt_base` 必须大于 `u32::MAX`”，`:2159` 写直核面是“strict → pre-table → postconditions → boot_validate → kmain validate”。同篇两节不能同时为真。

5. **检查设计合同是否支持文档。**
   - `.design/01-design.v4.md` 的 Ch2 I4 行也写了同一串箭头。设计合同是规范意图，不是源码事实；如果设计意图确实是“先落存储再严格校验”，那么实现没有遵守设计，按真值优先序（C 源码 > 设计契约 > Rust 实现 > 文档）应由设计/实现裁决，而不是让文档把源码事实写成相反；如果箭头只是逻辑分层，文档应写明“不表示执行序”，否则与 §4.4 的实际腿序冲突。

### 结果

**阻断 P1。** §3.7 的 `check_boot_postconditions` 描述是过期内容（v10 之前的派生字段检查），与 `kernel_info.rs` 的现行函数体和 §4.4 的现行描述都冲突；§1.2 的箭头顺序与所有生产腿的实际调用序相反。I4 是四条核心不变量之一，这个错误会让读者把“后置条件门”与“共享校验体”的职责分错，也会把 strict 门的时序说反。

### 证据

- `os/libs/minix-boot/src/kernel_info.rs:236-254`：`validate_before_tables`/`validate_requiring_sources` 入口。
- `os/libs/minix-boot/src/kernel_info.rs:256-535`：共享校验体，`:351-405` 是内核/模块区间重叠检查。
- `os/libs/minix-boot/src/kernel_info.rs:567-588`：`check_boot_postconditions`，`:583` 是唯一的 `kern_virt_base > u32::MAX` 断言。
- `os/kernel/src/lib.rs:223-225`：`arch_boot_finish` 的顺序。
- `os/kernel/src/lib.rs:320`、`:737-739`、`:1856`、`:1865`：v1 接收面与 `kmain` 的顺序。
- `os/kernel-image/src/bootface_common.rs:558-561`：直核面 strict 在 `arch_boot` 之前。
- 文档原句：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:27`、`:1702`、`:2155-2159`。

### 若我错了

如果把 §1.2 的箭头解释为“三个门按依赖存在，不表示时间序”，且把 §3.7 的句子标注为“v10 之前的历史描述”，那么只有措辞问题，不是事实错误。但文档没有时态标注，§3.7 与 §4.4 同篇并存，读者无法知道哪一句是现行行为；按“文档与代码同步”的要求，这仍然需要改。

## CLAIM 05

### 主张

文档 §3.7 主张：“`BootStorage` 是六类载荷的唯一所有者；`fill_modules`/`fill_platform`/`fill_param`/`fill_map_tables`/`fill_source_records` 各自在闭包返回时提交计数，半提交状态不可表示”；并在四类错误落点里写“半提交 → `fill_*` 的第二次调用 panic”。

### 反证尝试

1. **“六类载荷的唯一所有者”在 UEFI 腿上是否成立。**
   - `os/libs/minix-boot/src/boot_storage.rs:20-30` 的模块文档明确写：“Source records are the exception on the UEFI legs: the classifier writes them straight to the wire (its 296-record source budget does not fit this storage's 40 slots), so `BootReport` is not a complete view there.”
   - `os/boot-shim/src/uefi_helpers.rs:1154-1215` 的生产路径只调用 `storage.fill_map_tables(...)`，source 记录由 `ebs_pair::pair_and_classify` 直接写进 arena 的 wire 分区（同一段闭包内），没有 `fill_source_records`。因此对“source 记录”这一类，在两条 UEFI 腿上 `BootStorage` 不是所有者，wire/序列化器才是。
   - 文档自己在 §3.2 的 UEFI 路径说明里写了“UEFI 腿的 source 记录由分类器直写 wire；storage 的 source 切片为空”（`:1827`），与 §3.7 的无条件“唯一所有者”冲突。

2. **`fill_map_tables` 第二次调用是否 panic。**
   - `os/libs/minix-boot/src/boot_storage.rs:140-158` 的 `fill_map_tables` 没有 `map_committed` 之类的布尔守卫（结构体只有 `modules_committed`/`platform_committed`/`param_committed`/`sources_committed` 四个，`:101-104`），也没有“already committed”断言；它只检查两个计数不超过容量，然后直接写 `memmap_len`/`reserved_len`。第二次调用会静默覆盖计数（并可能留下第一轮写过的槽位）。
   - 对比：`fill_modules`（`:160-172`）、`fill_platform`（`:175-187`）、`fill_param`（`:193-205`）、`fill_source_records`（`:208-220`）都有 `*_committed` 守卫，第二次调用会 panic。测试里只有 `fill_modules_twice_is_refused`（`:397`），没有 `fill_map_tables_twice_is_refused`。
   - 因此“`fill_*` 的第二次调用 panic”作为对五个 `fill_*` 的普遍陈述为假；`fill_map_tables` 是例外。

3. **“半提交状态不可表示”是否仍成立。**
   - 对 `fill_map_tables` 的单次调用，`write` 闭包返回 `MapCounts` 后一次性写两个长度，确实没有“写了一张表、没提交长度”的中间态。这条子主张成立。
   - 但“第二次调用 panic”是另一条更强的机制主张，不能由“单次调用成对提交”推出；文档把两者写在同一段，读者会把 `fill_map_tables` 也当成 one-shot。

### 结果

**非阻断 P2。** 两处过宽/错误陈述：其一，“六类载荷的唯一所有者”在 UEFI 腿的 source 记录上不成立（文档别处已承认这个例外）；其二，“`fill_*` 第二次调用 panic”对 `fill_map_tables` 不成立。I3 的“半提交不可表示”主结论仍然成立，但证据句需要改写成“四个 `fill_*` 有 one-shot 守卫；两张内存表由单次调用内成对提交，没有单独的长度提交口”。

### 证据

- `os/libs/minix-boot/src/boot_storage.rs:20-30`：source 记录在 UEFI 腿的例外。
- `os/libs/minix-boot/src/boot_storage.rs:101-104`：结构体只有四个 `*_committed` 标志，没有 map 表的 committed 标志。
- `os/libs/minix-boot/src/boot_storage.rs:140-158`：`fill_map_tables` 无 one-shot 守卫。
- `os/libs/minix-boot/src/boot_storage.rs:160-220`：其余四个 `fill_*` 的 `*_committed` 守卫与断言。
- `os/libs/minix-boot/src/boot_storage.rs:397`：只有 `fill_modules_twice_is_refused` 这一条重复调用负测。
- `os/boot-shim/src/uefi_helpers.rs:1154-1215`：UEFI 腿只 `fill_map_tables`，source 走 `ebs_pair` 直写 wire。
- 文档原句：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:1700`、`:1704`、`:1827`。

### 若我错了

如果“六类载荷”指的是 wire 的六个分区而不是 `BootStorage` 的六个槽，那么 source 记录在 UEFI 腿上确实有“所有者”（序列化器/arena），但句子的主语是 `BootStorage`，按字面读不成立。如果 `fill_*` 在“第二次调用 panic”一句里被作者有意限定为“有 committed 标志的那四个”，句子应写明；同段刚刚列了 `fill_map_tables`，读者不会自动排除它。

## CLAIM 06

### 主张

文档 §3.5.2 主张：`minix-kernel` 不依赖 boot-shim，也不依赖 `uefi`；`minix-boot` 只依赖 `minix-types` 与 `fdt`；`kernel-image` 在 riscv64/aarch64 target 下反向依赖 boot-shim 的**库**（只开 `opensbi` feature，不开 `uefi`）；boot-shim 对 `minix-kernel` 的调用面只有 `enter_kernel_image_v1` 与 `boot_alloc::init_boot_pt_alloc`。

### 反证尝试

1. **四份 manifest 逐份核对。**
   - `os/kernel/Cargo.toml` 的 `[dependencies]`：`minix-arch`、`minix-plat`、`minix-platform`、`minix-types`、`minix-boot`、`bitflags`。没有 boot-shim，也没有 `uefi`。`[dev-dependencies]` 只有 `minix-arch`、`minix-plat`，也不引入 boot-shim。主张成立。
   - `os/libs/minix-boot/Cargo.toml` 的 `[dependencies]`：`minix-types`、`fdt`。主张成立。
   - `os/kernel-image/Cargo.toml` 的 `[target.'cfg(target_arch = "riscv64")'.dependencies]` 与 `[target.'cfg(target_arch = "aarch64")'.dependencies]` 都是 `boot-shim = { ..., default-features = false, features = ["opensbi"] }` 与 `fdt`；没有 `uefi`。主张成立。
   - `os/boot-shim/Cargo.toml` 有 `minix-kernel`（boot-shim → kernel，方向正确），`uefi`/`uefi-raw` 是可选依赖，由 `uefi` feature 打开。主张成立。

2. **调用面反例：boot-shim 是否调用了 `minix-kernel` 的其他符号。**
   - `rg -n "minix_kernel::" os/boot-shim/src` 只命中 `boot_alloc::init_boot_pt_alloc`（`uefi_helpers.rs:1056`、`uboot_main.rs:481`）、`enter_kernel_image_v1`（`uefi_helpers.rs:1265`、`uboot_main.rs:529`）以及一条文档注释（`opensbi_helpers.rs:537`）。没有第三种运行期调用。主张成立。

3. **反向边/成环反例。**
   - `minix-kernel → minix-boot` 是库依赖；`minix-boot → minix-kernel` 不存在。`kernel-image → boot-shim` 只在 riscv64/aarch64 target 生效，且 `kernel-image` 是独立产物；`boot-shim → minix-kernel` 是引导器依赖内核，不构成环。主张成立。
   - 尝试找 `minix-kernel` 通过 feature 间接拉入 boot-shim 的路径：`os/kernel/Cargo.toml` 的 feature 段没有 boot-shim 依赖；`minix-kernel` 也不依赖 `uefi`。没有反例。

### 结果

**反证失败。** 四份 manifest、调用面与 target 条件都与文档一致；单向边界在当前构建图里成立。

### 证据

- `os/boot-shim/Cargo.toml`、`os/kernel/Cargo.toml`、`os/libs/minix-boot/Cargo.toml`、`os/kernel-image/Cargo.toml` 的 `[dependencies]`/`[dev-dependencies]`/`[target.'cfg(...)'.dependencies]` 段。
- `rg -n "minix_kernel::" os/boot-shim/src` 的输出（两种调用 + 一条注释）。
- 文档原句：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:1481-1517`。

### 若我错了

如果某个构建 feature 在 `os/kernel/Cargo.toml` 之外（例如 workspace 级 feature 或 `minix-platform` 的传递依赖）把 `uefi` 拉进内核依赖面，主张会失效。我检查的是四份 manifest 的直接依赖与 `minix_kernel::` 调用点，没有展开整个 workspace 的传递依赖树；如果存在传递依赖，需要再核对。

## 额外发现：跨节复述冲突与失败路径省略

以下问题不单列为“核心主张”，但属于任务要求的“跨节复述冲突、失败路径省略”。

### X1（阻断 P1）：§5.4 被多处指定为“唯一权威”，但承诺的表格与“三层判别”不存在

- §3.1.1 的接线状态表说 riscv64 直核门的错误路径“见 §5.4”、aarch64 直核的 SMP 判据“见 §5.4”（`:981-982`）。
- §3.2.4 说零化候选②“读数与三层判别见 §5.4”（`:1170`）。
- §3.6 的计数表说“真机门链五条……成员以 §5.4 的表格为唯一权威（本节不重复名单）”（`:1693`）。
- §4.5 说“三层证据判别见 §5.4”（`:2288`）。
- §6.4 说“真机门链五条，成员与判据见 §5.4 的表格”（`:2534`）。
- 实际读取 §5.3（`:2379-2381`）与 §5.4（`:2383-2386`）：§5.3 只有一段“不把宿主全绿当真机全绿”，§5.4 只有一段“宿主测试通过不能替代真机门通过”。没有五条门链的表格，也没有“三层判别”。§5.3 里还写着“本表把这三项列在‘不证明什么’里”，但本节没有表（表在 §5.1）。
- 这不是单处笔误，而是多处引用同一个不存在的内容。它使 §3.6 的“唯一权威”落空，也使 I4 的证据缺口（三层判别）无法核对。按 review 规则，这属于跨节引用完整性/证据强度问题，至少 P1。

### X2（P1，范围限于 x86_64 测试内核）：§6.1 的 `kernel: arch_boot arg=…` / `reslen=…` 里程碑在源码中不存在

- 文档 `:2435` 列出一行里程碑：`kernel: arch_boot arg=…` / `kernel: arch_boot reslen=… r0=…`，位置写 `os/kernel/src/lib.rs` 的 x86_64 `arch_boot`，并说 `reslen=0` 与 `reslen=127` 是两个判别落点。
- 源码 `os/kernel/src/lib.rs:239-258` 的 x86_64 `arch_boot` 只打印 `kernel: arch_boot entered`；`rg -n "kernel: arch_boot arg|arch_boot reslen" os/` 零命中。`reslen` 这个子串在 `os/` 里只出现在 mib/ipc 的 `call_reslen` 字段（`os/servers/mib/src/dispatch.rs`、`os/libs/minix-types/src/ipc/mib.rs`），与 `arch_boot` 无关。文档的字符串与位置都不成立。
- 影响：按 §6.1 的“里程碑清单是排查时的定位依据”去抓串口，会一直找不到这两行，进而误判“没走到 `arch_boot`”。这不是核心不变量错误，但会直接误导诊断，标 P1；如果测试内核有仓外载体，位置列也至少指错了。

### X3（P2）：字段台账的 21/22 计数不一致，且“§3.4 与 §3.4”重复引用

- §3.2 的“删除”行写“21 个字段”并列出 21 个（`:1033`）；§3.2.2 写“这 22 个是 C `kinfo_t` → Rust `KernelInfo` 的累计退场台账”（`:1068`），其表里多出 `user_sp` 一行（`:1083`）。`user_sp` 确实是 C `kinfo_t` 的字段、也确实在 v10 从 `KernelInfo` 删除，累计台账应为 22。两个数字不能同时成立。
- 同篇四处出现“§3.4 与 §3.4”/“§3.4/§3.4”（`:673`、`:1023`、`:1033`、`:1068`）。这像是两个不同子节号的引用在改名时被压成同一个，读者无法按引用查到具体小节。P2。

### X4（P2）：`FREE_MEMMAP` 的“仅 bootface_a64.rs 注释提及”不成立

- 文档 `:1044` 写“`FREE_MEMMAP` 已没有符号定义（仅 `bootface_a64.rs` 的一行注释提及该字符串）”。
- `rg -n "FREE_MEMMAP" os/` 在整个 `os/` 树零命中；`os/kernel-image/src/bootface_a64.rs` 的 `memmap` 相关注释（`:39`）也没有这个字符串。否定事实写错，P2。

### X5（P2）：`validate_before_tables` 的检查项枚举不完整

- 文档 `:2155` 写“pre-table gate：`kern_size != 0`、`memmap` 非空、`param_buf.len() <= 1024`——它不查 `boot_modules` 非空”。
- `os/libs/minix-boot/src/kernel_info.rs:256-535` 的 `BeforeTables` 档还会检查：`platform_sources` 的非空地址/非零 kind/跨度溢出、`memmap` 与 `reserved_regions` 的升序与两两不相交、两张表的互斥、内核与模块以及模块与模块的半开区间不重叠、每个区间的非零长度与页对齐、`source_records` 非空时的覆盖审计。表 `:2169-2171` 只列了三项。
- 其中“区间互斥/模块重叠”在表里的“共享校验体”两行有覆盖，但 `platform_sources` 校验、排序/页对齐/非零长度没有出现在任何行。这个枚举会被读者当成完整清单，P2。

### X6（P2）：`WRITE_BACK == 0 → Side` 会把内核跨度排除出两份表，进而触发 strict 门，但文档没有把这条失败路径接起来

- 文档 `:1044` 说 UEFI 腿的内核跨度“分类落 `ReservedMapped` 时进入 `reserved_regions`；无 `WRITE_BACK` 时落 `Side`，不进这份表”。§4.2.2 的 Step 5 也写 `WRITE_BACK == 0 → Side`（`:1893`）。
- 但 `reserve_kernel_span` 只把内核跨度标成 `LOADER_DATA`；如果固件在最终内存图里把这块报成“无 WRITE_BACK”，分类器会把它放进 `Side`，`memmap` 与 `reserved_regions` 都不含内核跨度。此时 strict 门的 `requires_startup_kernel_record` 要求“内核物理跨度落在某条 `memmap` 或 `reserved_regions` 区间内”（`kernel_info.rs:507-525`），接收镜像会在 `arch_boot_resume_v1` 的 `validate_requiring_sources()` 处 panic（直核面则在 bootface 的 strict 调用处）。
- 这条路径是 fail-closed，不是静默错误；但文档的 §6.3 失败模式表没有列它，§4.4 的门表也没有把“固件不设 WB 的 loader 页”列为触发条件。按“失败路径省略”记 P2；实际固件通常会给 `LOADER_DATA` 设 WB，所以不是常态。

## 汇总

| 编号 | 主题 | 结果 | 关键证据 |
|---|---|---|---|
| CLAIM 01 | C `alloc_lowest` 长度当结束地址、第三行常态 | 反证失败（主张成立；可补 `CONFIG_MAX_CPUS <= 8`） | `pg_utils.c:81`、`arch_smp.c:76`、`main.c:142/147/311`、`arch_system.c:277-278` |
| CLAIM 02 | C `mb_set_param` 去重只对首条生效 | 主域反证失败；空键反例成立（P2） | `pre_init.c:35-58`、`:131-144` |
| CLAIM 03 | ARM32 闭区间判定与 30 MiB 间隔 | 反证失败（主张成立） | `earm/pre_init.c:167-169`、`:181-203`、`:322-341`、`earm/kernel.lds:4` |
| CLAIM 04 | I4 的次序与 `check_boot_postconditions` 内容 | 阻断 P1 | `kernel_info.rs:567-588`、`lib.rs:223-225/737-739`、`bootface_common.rs:558-561`、文档 `:27/:1702/:2155-2159` |
| CLAIM 05 | `BootStorage` 唯一所有者与 `fill_*` 二次调用 | 非阻断 P2 | `boot_storage.rs:20-30/140-158/160-220`、`uefi_helpers.rs:1154-1215` |
| CLAIM 06 | crate 单向依赖边界 | 反证失败（主张成立） | 四份 `Cargo.toml`、`rg "minix_kernel::" os/boot-shim/src` |
| X1 | §5.4 “唯一权威”落空 | 阻断 P1 | 文档 `:981-982/:1170/:1693/:2288/:2534`；§5.3/§5.4 无表 |
| X2 | `arch_boot arg/reslen` 里程碑不存在 | P1（限 x86_64 测试内核） | `lib.rs:239-258`；`rg "kernel: arch_boot arg|arch_boot reslen" os/` 零命中 |
| X3 | 21/22 字段计数与重复 §3.4 引用 | P2 | 文档 `:1033/:1068/:1083/:673/:1023` |
| X4 | `FREE_MEMMAP` 注释不存在 | P2 | `rg "FREE_MEMMAP" os/` 零命中 |
| X5 | `validate_before_tables` 枚举不完整 | P2 | `kernel_info.rs:256-535`；文档 `:2155/:2169-2171` |
| X6 | 无 WRITE_BACK 的 loader 页会触发 strict 门 | P2（失败路径省略） | `ebs_map.rs:634-636`、`kernel_info.rs:507-525`、文档 `:1044/:1893` |

## 本次反证的局限

- 只做源码阅读，没有运行宿主测试、QEMU 或真机；CLAIM 01/02/03 的 C 结论是按源码逐行推演的，C 的运行期行为（尤其空键路径）没有实机取证。
- 没有展开 workspace 的完整传递依赖树；CLAIM 06 的“无环”结论只覆盖四份 manifest 的直接依赖与 boot-shim 的运行期调用点。
- 没有读取历史评审或其他面板输出；X1 的“§5.4 缺表”是对当前目标文档的独立核对，不依赖任何历史结论。
- 主张分级是反证视角：P1 表示“按当前源码，文档句子为假或证据链断裂”，不等于代码有缺陷；CLAIM 04/05 的错误都在文档侧，CLAIM 01/03/06 的代码行为与文档一致。
