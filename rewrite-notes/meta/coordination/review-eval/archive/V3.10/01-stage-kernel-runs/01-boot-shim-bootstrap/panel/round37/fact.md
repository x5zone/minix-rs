# 事实审查报告 — 01-boot-shim-bootstrap

审查对象：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（2651 行）。
只读范围：目标文档；`related-code.txt` 所列 Rust 源码；`DECISIONS.md` 的 D-099..D-107；C 侧事实按项目真值优先序核对 `minix3/` 原始源码。未读取任何历史 scan/review 报告、其他面板角色输出或 expected/。

## 机械门读数（先跑后引）

- `python3 tools/doc-citation-check.py <doc> --root .` → `blocks=48 checked=21 failed=0 skipped=23 warned=4`。四个 WARN 为 1256-1298、1302-1323、1749-1846、2041-2069，原因均为 `structure-only(declared-rewrite,source-symbol-ok)`：只核对了 source-symbol，**代码内容未逐行对账**。下面 FACT-05、FACT-06 正落在这一豁免里。
- `bash tools/anchor-resolve.sh --check <doc>` → `anchors=89 resolved=89 zero-def=0 multi-def=0`。
- `python3 tools/doc-structure-check.py <doc>` → 1 个文件表结构与内部 `.md` 链接均可解析。
- `bash tools/doc-ref-check.sh <doc>` → 内部章节引用均可解析（该门只验被引节存在，不验内容指向，见 FACT-07）。
- `bash tools/doc-style-lint.sh <doc>` → error 级 0。
- `bash tools/doc-code-density-check.sh --check <doc>` → 4 个可判章均有码/命令，承载门阻断项通过。
- `bash tools/doc-code-map.sh <doc>` → 48 个文件，存在性 48/48 通过。
- 测试计数复算（`cargo test --no-run` 后列测试二进制）：`minix-boot --lib` 187、`boot-shim --lib` 136、`minix-elf --lib` 27、`minix-kernel --features mock --lib` 874 总数 / `--list --ignored` 8 → 866 通过，与 §5.2 的 187/136/866/27 一致。
- `rustc --print target-list | grep -i uefi` → `aarch64-unknown-uefi`、`i686-unknown-uefi`、`x86_64-unknown-uefi`，无 riscv64，与 §3.1.1 一致。

## FINDING FACT-01（P1）

- 位置：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` §5.3/§5.4（2384-2390 行）；引用点 §3.1.1（981-982）、§3.6（1693）、§3.2.4（1170）、§4.5（2288）、§6.4（2534/2543/2544）。
- 严重度：P1
- 问题：文档至少八处把“§5.4 的表格”当作五条真机门链成员与判据、以及“三层证据判别”的唯一权威，但 §5.4 正文只有一段使用规则，没有任何表格；§5.3 也没有它提到的“本表”。
- 影响：文档自称的“唯一权威”不存在；五条门链的成员、判据、覆盖腿与“不证明什么”无法按图核对；§5.5 的“没有未闭环测试缺口”缺少它宣称的登记处。§3.2.4 的“候选②读数的三层判别”同样无处可查。
- 证据或推理：`sed -n '2384,2394p' <doc>` 输出三段连续文字（§5.3 一段、§5.4 一段、§5.5 一段），无任何表格行；`grep -n '§5\.4' <doc>` 命中 8 处，措辞分别为“见 §5.4 的表格”“成员以 §5.4 的表格为唯一权威”“按 §5.4 表对应行判定”“三层判别见 §5.4”。E1（门读数允许它通过，因为 doc-ref-check 只验节存在）+ E2（grep/sed）。
- 最小修复：在 §5.4 补出五条门链表（脚本、判据、适用腿、不证明什么）与三层证据判别表；或把四处“见 §5.4 的表格”改为真实存在的外部节并写明篇名。
- 置信度：高
- 是否需源码核实：否（文档内结构事实）
- 若我错了：若 §5.4 的表格在渲染产物或另一分支存在而工作树文件缺失，本条降为 P3；当前 2651 行的工作树文件 2384-2394 只有三段文字。

## FINDING FACT-02（P1）

- 位置：文档 §3.7（1702 行）；源码 `os/libs/minix-boot/src/kernel_info.rs:567-590`（`check_boot_postconditions`）。
- 严重度：P1
- 问题：§3.7 说 `check_boot_postconditions` 检查“派生值（栈顶、用户栈顶、系统调用入口）与镜像/模块区间的半开不重叠”。实际函数只有一条断言 `kern_virt_base > u32::MAX`；函数自己的注释写明 v10（D-062）已删除那三条派生值断言；镜像/模块的半开不重叠在 `validate` 的共享校验体里，不在这个函数。
- 影响：I4“校验先于消费”的证据段落把一条不存在的检查写成第二道门，读者会以为 v10 之后仍有栈顶/用户栈顶/syscall 入口校验；与同文档 §4.4（2156 行“v10 之后只剩一条可被剩余字段证伪的关系”）直接矛盾。
- 证据或推理：`sed -n '/pub fn check_boot_postconditions/,/^    }/p' os/libs/minix-boot/src/kernel_info.rs` 输出仅 `assert!(self.kern_virt_base.0 > u64::from(u32::MAX), ...)`；`rg -n "kern_stack_top|user_sp|syscall_entry" kernel_info.rs` 在 `check_boot_postconditions` 体内零命中；§4.4 同页已给正确描述。E2 + E3。
- 最小修复：改写 §3.7 该句为“`check_boot_postconditions` 只保留 `kern_virt_base > u32::MAX`；三条派生值断言随 v10 删除；镜像/模块不重叠在共享校验体”。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若该函数在别的 cfg 档有第二实现，本条部分成立；`rg -n "fn check_boot_postconditions" os/` 只有 `kernel_info.rs` 一处。

## FINDING FACT-03（P2）

- 位置：文档 §6.1 里程碑表（2435 行）；同文档 §4.4（2121 行“`arch_boot` 的调试打印已先读过部分字段”）；源码 `os/kernel/src/lib.rs:239-264`。
- 严重度：P2
- 问题：表中列 `kernel: arch_boot arg=…` / `kernel: arch_boot reslen=… r0=…`，声明为 x86_64 `arch_boot` 的取证行，并称 `reslen=0` 与 `reslen=127` 是两个判别落点。当前代码的 x86_64 `arch_boot` 只打印 `kernel: arch_boot entered\n`；全仓内核源码对 `reslen` / `arch_boot arg` 零命中。
- 影响：诊断章把不存在的日志行当作定位依据；按该行 grep 会得到空结果，“reslen=127”这一证据锚点无法复现。§4.4 的同句也随之失真。
- 证据或推理：`rg -n '"kernel: arch_boot' os/` → 唯一命中 `Console::write_str("kernel: arch_boot entered\n")`；`rg -n "reslen|arch_boot arg" os/ --glob '*.rs'` → 内核侧零命中（仅 MIB 的 `call_reslen`，与 boot 无关）。E1（机械门不覆盖里程碑字符串）+ E2。
- 最小修复：删除该表行；或改写成实际存在的 `kernel: arch_boot entered` 并说明它不是 reslen 探针，同时修 §4.4 的同句。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若该探针存在于未入库的测试补丁或旧分支，当前树仍不提供该输出；以当前树为准，本条成立。

## FINDING FACT-04（P2）

- 位置：文档 §1.2 I4 行（27 行）；对照 §4.4“腿间实际执行序”（2159 行）；源码 `os/kernel-image/src/bootface_common.rs:558-561`、`os/kernel/src/lib.rs:214-230`、`299-350`、`653-731`。
- 严重度：P2
- 问题：I4 的“可观测判据”写成 `validate_before_tables` → `store_kernel_info` → `validate_requiring_sources`。实际直核面是 `validate_requiring_sources`（在 `arch_boot` 之前显式调用）→ `validate_before_tables` → `check_boot_postconditions` → `store_kernel_info`；v1 接收面是 `validate_requiring_sources` → `check_boot_postconditions` → `resume_additive` 内的 `store_kernel_info`。文档自己的 §4.4 也给出这个次序。
- 影响：四条核心不变量的汇总表与详述节互相矛盾；读者会把 strict 门当成 store 之后的检查，I4 的“可观测判据”失去可核性。
- 证据或推理：`sed -n '558,561p' os/kernel-image/src/bootface_common.rs` 显示 `validate_requiring_sources(); ... arch_boot(...)`；`arch_boot_finish` 体为 `validate_before_tables; check_boot_postconditions; store_kernel_info; arch_boot_impl;`；`arch_boot_resume_v1` 体为 `validate_requiring_sources(); check_boot_postconditions(); resume_boot_pt_alloc(...); resume_after_v1_decode(...)`。E2。
- 最小修复：把 I4 行改为真实次序，并注明“strict 门在 arch_boot 之前、store 之前”。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若箭头只表示“三个检查都存在”而非时序，本条降为措辞歧义；但同表 I1-I3 的箭头都读作流程，且 §4.4 明说“腿间实际执行序”。

## FINDING FACT-05（P2）

- 位置：文档 §4.2 代码块（1749-1846 行，关键 1760-1767）；源码 `os/boot-shim/src/uefi_helpers.rs:1026-1062`。
- 严重度：P2
- 问题：块头声明“保留顺序与关键约束；注释与排版有改写/删节”，但块内改了三处代码：(1) `loader::load_segments_into_phys_memory(elf_data.as_slice())`，源码是 `(&layout)`（该函数签名收 `&ValidatedKernelImage<'_>`，传 `&[u8]` 不能编译）；(2) 源码在 `compute_kernel_layout(...)` 之后有 `.validate()`（正是 §4.3 说的 A/B 两道 ELF 防线），块里只剩 `.expect(...)`；(3) `let kern = layout;`，源码是 `let kern = layout.layout();`。
- 影响：按该块理解生产入口会漏掉 ELF 段源/目标重叠校验（A/B 门），并得到类型不一致、不可编译的“代码引用”；与 doc-citation-check 的“节选允许省略但不允许改写”纪律冲突。
- 证据或推理：逐行 token 对账（脚本跳过注释/中文/省略号，按 `...` 切段检索源码）报 1766 `load_segments_into_phys_memory(elf_data.as_slice())` 与 1767 `let kern = layout;` 不在源码；`sed -n '1026,1062p' os/boot-shim/src/uefi_helpers.rs` 显示 `.validate()`（1033 行）、`(&layout)`（1059 行）、`layout.layout()`（1061 行）。citation-check 对该块只给 WARN（`mode=structure-only(declared-rewrite,source-symbol-ok)`），所以门没有报。E1 + E2。
- 最小修复：恢复 `(&layout)`、`.validate()` 链与 `layout.layout()`；或在块头明确标“示意/改写”并写明“validate 与真实参数已省略”，同时在正文补一句 A/B 门在 `loader::compute_kernel_layout(...).validate()`。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若仓库另有同名函数收 `&[u8]` 且文档引用的是旧版本，本条部分成立；当前 `load_segments_into_phys_memory` 唯一签名是 `&ValidatedKernelImage`。

## FINDING FACT-06（P2）

- 位置：文档 §3.4.2 代码块（1304-1310 行）；源码 `os/kernel/src/boot_alloc.rs:138-140`。
- 严重度：P2
- 问题：块内三次 load 写 `Ordering::Release`，源码是 `Ordering::Acquire`。块头只声明“块内中文注释为译述”，`Ordering` 是代码 token，不属于注释。
- 影响：把读侧的 Acquire 写成 Release，传达错误的发布/获取配对语义；这是内存序事实，不是排版差异。
- 证据或推理：`grep -n "self.base.load\|self.next.load\|self.end.load" os/kernel/src/boot_alloc.rs` → `138 self.base.load(Ordering::Acquire)`、`139 self.next.load(Ordering::Acquire)`、`140 self.end.load(Ordering::Acquire)`；文档 1307-1309 为 `Release`。citation-check 对该块 WARN（未逐行）。E1 + E2。
- 最小修复：三处 `Release` → `Acquire`。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若文档引用的是另一个 `alloc` 实现，请给出路径；当前 `BootAlloc::alloc` 只有这一份。

## FINDING FACT-07（P2）

- 位置：文档 1250（“交接形态（3.7）”）、2288（“机制与读数见 §3.7”）、2291（“在 3.8 已列出”）、2330（“见 3.8 引用的表”）、2435（“§3.7 的 `res=127 zeroed=127`”）；重复引用 673、1023、1033、1068（“§3.4 与 §3.4” / “§3.4/§3.4”）。
- 严重度：P2
- 问题：文档只有 §3.1-§3.7，没有 §3.8；深拷读数在 §3.2.4，交接形态在 §3.2，三处注册表与扣除表在 §3.4.2；“§3.4 与 §3.4”把两个不同目标折叠成同一节。
- 影响：交叉引用失效；`doc-ref-check` 只验“被引节存在”，不验内容指向，所以该门不会报出这类错误。
- 证据或推理：`grep -n "^### 3\." <doc>` → 3.1/3.2/3.3/3.4/3.5/3.6/3.7，无 3.8；`grep -n "res=127" <doc>` → 1170（§3.2.4 内）；`grep -n "3\.8" <doc>` → 2291/2330；`grep -n "§3.4 与 §3.4\|§3.4/§3.4" <doc>` → 673/1023/1033/1068。E1（doc-ref-check 通过读数）+ E2。
- 最小修复：1250→§3.2；2288/2435→§3.2.4；2291/2330→§3.4.2；673/1023/1033/1068 恢复为两个具体子节号（按内容应为 §3.4.1 与 §3.4.3）。
- 置信度：高
- 是否需源码核实：否（文档内结构事实）
- 若我错了：若“3.8”指外部篇的节号，需要写明“02 篇 §3.8”；当前文中其他跨篇引用都带“02 篇的”前缀，故按本篇失效处理。

## FINDING FACT-08（P3）

- 位置：文档 §3.2.1（1044 行）。
- 严重度：P3
- 问题：说 `FREE_MEMMAP`“已没有符号定义（仅 `bootface_a64.rs` 的一行注释提及该字符串）”。全仓排除 `rewrite-notes/` 与 `minix3/` 后 `FREE_MEMMAP` 零命中，`bootface_a64.rs` 也没有该词。
- 影响：阴性声明过强且指向一个不存在的注释；读者按此检索会找不到。
- 证据或推理：`rg -in "FREE_MEMMAP" --glob '!rewrite-notes/**' --glob '!minix3/**' .` → 空；`rg -in "free_memmap" kernel-image/src/*.rs` → 空。E2。
- 最小修复：删去括号里的“仅……提及”，只保留“已没有符号定义”。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若该字符串在二进制或生成物里，当前源码树仍无；按当前树为准。

## FINDING FACT-09（P3）

- 位置：文档 §4.3（1460 行）。
- 严重度：P3
- 问题：“kernel 槽 2 MiB、模块槽 1 MiB”对 `mfs` 不成立。`os/boot-shim/src/uboot_main.rs` 的 `module_slot_size(10)` 返回 `MFS_SLOT_SIZE = 16 MiB`（mfs 内嵌约 8.5 MiB 的 ramdisk），其余模块才是 `MODULE_SLOT_SIZE = 1 MiB`。
- 影响：读者会以为 mfs 的 staging 上限是 1 MiB；实际代码给了专用 16 MiB 槽，截断边界与“超过槽容量”的行为都不同。
- 证据或推理：`sed -n '180,220p' os/boot-shim/src/uboot_main.rs` → `MODULE_FILE_STAGE_MFS`、`MFS_SLOT_SIZE: 16 MiB`、`module_slot_size(idx)` 对 `idx == 10` 返回 `MFS_SLOT_SIZE`。E2。
- 最小修复：写成“kernel 槽 2 MiB、普通模块槽 1 MiB、mfs 专用 16 MiB”。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若文档的“模块槽”专指 `MODULE_FILE_STAGE_STRIDE` 而非容量，则本条为措辞歧义；同句“超过槽容量”表明它说的是容量。

## FINDING FACT-10（P3）

- 位置：文档 §6.4（2575 行）。
- 严重度：P3
- 问题：第三层 GDB 检查项写“检查 `KernelInfo` 的字段……`kern_stack_top` 是否等于镜像尾”。v10 之后 `kern_stack_top` 不是字段，是访问器 `KernelInfo::kern_stack_top()`（同文档 §3.2.1/§4.4 已写明）。
- 影响：在 GDB 里按字段名查会落空；与同句“`bootstrap_len` 已随字段删除”自相矛盾（`kern_stack_top` 同样已从结构体删除，只是保留了访问器）。
- 证据或推理：`struct KernelInfo` 的 10 个字段列表无 `kern_stack_top`；`kernel_info.rs` 的 `pub fn kern_stack_top(&self)`；§3.2.1 的 v10 删除段。E2。
- 最小修复：改为“检查 `kern_virt_base + kern_size`（或访问器）是否等于镜像尾”。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若调试信息里保留了旧字段名，当前源码仍无该字段。

## FINDING FACT-11（P3）

- 位置：文档 §4.1（1740 行）。
- 严重度：P3
- 问题：声称“覆盖范围按 `bootface.rs` 的常量注释是「Sv39 root + identity low half + high half + two DM windows」”。`bootface_common.rs:35` 的 `PT_BUMP_PAGES` 注释只写“1 MiB, 256 pages; both faces use the same order of magnitude and take an explicit halt arm on exhaustion”，`bootface.rs`/`bootface_a64.rs` 均无该句。
- 影响：该清单是“DM 窗口也算在这 256 页里”的唯一出处；注释不存在则这一覆盖结论没有可核来源（结论本身可能仍对，因为 DM 建表确实走同一个 `pt_alloc` 池）。
- 证据或推理：`rg -in "Sv39 root|identity low|two DM|DM windows" os/kernel-image/src` → 无匹配；`sed -n '25,40p' os/kernel-image/src/bootface_common.rs` 输出常量注释全文。E2。
- 最小修复：把该清单改为正文自述并给出可核的窗口构造点，或删去“按 `bootface.rs` 的常量注释”并标为分析。
- 置信度：中高（注释可能存在于未入库的历史版本）
- 是否需源码核实：是（确认是否曾存在于历史版本）
- 若我错了：若该注释在 `bootface.rs` 的未检索区段，请给出文件与行号；当前对 `kernel-image/src` 全部 `*.rs` 的大小写不敏感检索为零命中。

## FINDING FACT-12（P3）

- 位置：文档 1044（“阶段 已移除”）、1232（“随 阶段 退役”）、1247（“阶段 已不再要求……”与“阶段 的取证注释”）、2394（“R02-阶段/阶段/阶段”）。
- 严重度：P3
- 问题：“阶段”前带空格，没有指代对象，读不出指的是哪个阶段；§5.5 的“R02-阶段/阶段/阶段”更把三个不同的测试缺口编号压成同一个词。
- 影响：相关证据声明不可复核（不知道指哪一条 R02 缺口、哪一个阶段），也让 §5.5 的“无未闭环测试缺口”无法按名核对。
- 证据或推理：`grep -no "阶段 已[^，。；]*\|阶段 的[^，。；]*\|随 阶段[^，。；]*" <doc>` → 1044/1232/1247；`grep -n "R02-阶段" <doc>` → 2394。E2。
- 最小修复：恢复具体阶段名与三个 R02 编号；或删除无法指代的句子。
- 置信度：高（文本事实）
- 是否需源码核实：是（需要作者补回原名）
- 若我错了：若“阶段”是项目约定的通用代号，请在文档术语表里定义；当前 §1.4 术语表没有它。

## 附：已核对且未发现问题的关键面

- C 侧逐字引文：`head.S` 的 Multiboot 头与 `multiboot_init`、`pre_init.c` 的 `get_parameters`/`mb_set_param`/`overlaps`、`pg_utils.c` 的 `add_memmap`/`cut_memmap`/`alloc_lowest`/`pg_alloc_page`/`alloc_pagetable`、`protect.c` 的“还 VM 镜像”、`main.c` 的 `get_value`/`kmain`/`bsp_finish_booting`、`arch_system.c` 的两条 `cut_memmap`、`arch_smp.c` 的 `copy_trampoline`、`earm/pre_init.c` 的 `setup_mbi`/`overlaps`、`multiboot.h`/`param.h`/`const.h` 的常量：逐项与源码一致（含行号锚点 51/83/18、584/595/617、758-760、76/9-12/42 等）。
- Rust 侧常量与签名：`KernelInfo` 10 字段、`BootHandoffV1` 字段与 envelope 128 B、版本 10、`ARENA_CAP` 32768、`WIRE_END` 7872、`WIRE_SOURCES_END` 29184、余量 3584、`WIRE_SOURCES_CAP` 296、`MEMMAP_REGION_STORE_LEN` 128、`RESERVED_REGION_STORE_LEN` 256、`BOOT_MODULE_NAME_LEN` 16、`PLATFORM_SOURCE_STORE_LEN` 4、`PARAM_BUF_STORE_LEN` 1024、`VM_BOOT_HANDOFF_MAX_REGIONS` 64、`VM_BOOT_HANDOFF_MAX_DEDUCTED` 47、`BOOT_PT_BUMP_PAGES` 1024、`PT_BUMP_PAGES` 256、`KinfoStruct` 48 B 布局、`MODULE_NAMES` 12 项与顺序、`BOOT_MODULE_PROC_NRS`、`DTB_SOURCE_CAP` 40：均一致。
- 阴性声明：`impl BootShim` 零命中、`grep rootdevname minix3/minix/kernel/` 零命中、内核侧 `arch=` 只写不读、Rust 侧 `BOARD/board` 只在 `boot_params.rs` 出现、riscv64 无 UEFI target、`ram_top` 分类变量不存在、`reserved-big` 行不存在：均已复现为零命中或与声明一致。
- 测试名：`generates_arch_and_board_first`、`rejects_internal_key_override`、`drops_tail_records_whole_at_capacity`、`splits_nul_and_whitespace_records`、`rejects_malformed_and_non_ascii`、`boot_windows_reject_reversed_ranges`、`boot_windows_reject_empty_ranges`、`decoded_map_survives_arena_poison_and_reissues_the_arena_frames`、`free_set_page_equivalent_to_reference_subtraction`：均存在。
- 门链事实：`test-riscv64-uboot-shim.sh` 的 6 个 marker、`test-kernel-image-riscv64-bootface.sh` 的 DTB 表址/模块 MISSING 两条断言、六个脚本的 `fdtput` + `opensbi,boot-file-table` 注入、`check-layout.sh` L9 已扩到 x86_64/aarch64/riscv64（注释引 D-107 F-R34-01）：均与文档一致。
