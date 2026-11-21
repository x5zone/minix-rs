# P-FACT 独立事实审查 · round38

目标：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（2941 行，sha256 见文末）。
只读审查；C 真值按 `minix3/` 源码，Rust 按 `os/` 现树。

## 机械门读数（全部实跑）

- `tools/anchor-resolve.sh --check <doc>`：`anchors=89 resolved=89 zero-def=0 multi-def=0`
- `tools/doc-code-map.sh <doc> --check`：`共 48 个文件；存在性 48/48 通过`
- `python3 tools/doc-structure-check.py <doc>`：`✅ … 1 个文件表结构与内部 .md 链接均可解析`
- `tools/doc-ref-check.sh --check <doc>`：`✅ … 内部章节引用均可解析`
- `tools/doc-style-lint.sh --diff`：`diff 范围：0 个文件 / 0 个新增行`，增量门通过
- `python3 tools/doc-citation-check.py <doc>`：`blocks=54 checked=21 failed=0 skipped=29 warned=4`
  （warn 块 1393-1435、1439-1460、1930-2030、2225-2253；reason 均为 `declared-rewrite`，不做逐行对账）

## FINDING FACT-01（P1）

- 位置：第 492 行，§2.4 `add_memmap` 代码引用块（块头只写「代码引用，minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap」，无「节选/改述」标注）。
- 严重度：P1。
- 问题：引文把源码注释 `Truncate available memory at 4GB` 写成 `Truncate available memory at 4 GiB`。
- 影响：按引文 grep 源码不命中；读者会以为源码用 IEC 单位，且该块被当作逐字引文引用。
- 证据或推理：E1 直接对读。`grep -n 'Truncate available memory' minix3/minix/kernel/arch/i386/pg_utils.c` → `90: … at 4GB …`；文档 492 行 → `… at 4 GiB …`。`doc-citation-check` 未检查此块（21 个 checked 中不含它），故 failed=0 不构成反证。
- 最小修复：把块内 `4 GiB` 改回 `4GB`。
- 置信度：高。
- 是否需源码核实：已核实。
- 若我错了：若项目允许把 C 注释里的 `4GB` 规范化为 `4 GiB` 且该块已声明非逐字，则降为 SUGGESTION；但块头无此声明。
- 对应判据：代码引用逐字/显式节选标注；文档-代码同步（引文与源码逐字对账）。

## FINDING FACT-02（P1）

- 位置：第 1408、2183、2243、2248 行，四个 Rust 代码引用块。
- 严重度：P1。
- 问题：引文在英文源码注释里插入现树不存在的过程编号：`(F-R23-05)`（pt_alloc 块）、`(R18 D-4)`（UbootFileLoader 块）；`D-082（I-36）` 也被插进 `load_boot_modules_into`/`UbootFileLoader` 的生产注释（源码只在 `loader.rs` 测试模块注释里出现 `I-36 / D-082`）。块头只声明「中文注释为译述/改述」「有一处截断」，未声明英文注释可增写。
- 影响：代码引用不再是逐字证据；过程编号出现在文档正文，违反出版标准。
- 证据或推理：E1。`grep -rn 'F-R23-05' os/` 只命中文档 1408 行，源码零命中；`grep -rn 'R18 D-4' os/` 零命中；`grep -n 'D-082' os/boot-shim/src/loader.rs` 只命中 1198/1204（测试模块），生产注释 614-620 无编号；`grep -n 'D-082' os/boot-shim/src/opensbi_helpers.rs` 零命中。
- 最小修复：删除引文内三处编号，或移到围栏外的 `〔编注〕`；若保留，块头显式声明「英文注释有增写」。
- 置信度：高（`F-R23-05`/`R18 D-4` 两处）；`D-082（I-36）` 处因块头已许「改述」置信度中，但仍是过程编号入正文。
- 是否需源码核实：已核实。
- 若我错了：若这些编号来自被引的旧 revision 且文档声明引用旧版，则不成立；但块头只给当前 `file:symbol` 锚点，无 revision。
- 对应判据：引文逐字/节选标注；出版标准（文档禁过程编号）。

## FINDING FACT-03（P2）

- 位置：第 1320 行（§3.2.4）与第 2639 行（§5.3）。
- 严重度：P2。
- 问题：两处声称「本篇用本仓探针初步排除了候选①（探针页在 EBS 前后与开分页后校验和一致）」，但全仓找不到该探针的测试、脚本或源码锚点；两节互相指认证据（§3.2.4 说「读数与三层判别见 §5.3」，§5.3 说「证据在 §3.2.4」），形成闭环。
- 影响：I1 深拷不变量的「已排除候选①」无法复核；读者会把一条未归档观察当成可复跑证据。
- 证据或推理：E2。`grep -rn '校验和\\|checksum\\|probe' os/kernel os/kernel-image os/boot-shim os/libs/minix-boot --include='*.rs'` 未命中任何 EBS 前后页校验和探针；§5.3 全文只有结论，无 `file:line`/测试名。
- 最小修复：写出探针的测试名/脚本与命令，或把措辞降为「人工探针，未归档日志」（与同段真机读数的诚实标注一致）。
- 置信度：中高。
- 是否需源码核实：已核实（零命中）。
- 若我错了：若探针是 QEMU 手工会话且无需入库，则应至少给出命令与日期；当前连日期都没有，仍不满足锚点纪律。
- 对应判据：事实断言须有可解析来源锚点；证据强度分级（L3 推断不得当证据）。

## FINDING FACT-04（P2）

- 位置：第 859 行（§2.6）与第 1603 行（§3.5）。
- 严重度：P2。
- 问题：§2.6 说两个直核腿用 `place_modules` 的「basename/长度/memmap/池相交四道门」；§3.5 同函数列出「条目数 ≥ 12、basename、长度非零、memmap 单区间、不与 bump 池相交」五道。源码另有第六条 `module_table.len() >= NR_BOOT_MODULES` 断言与第七条目标页 vs 内核跨度的 assert。两节计数互相矛盾，且都少算。
- 影响：读者按「四道门」做改动评估会漏掉条目数与目标页检查；两节对同一函数给出不同门数。
- 证据或推理：E1。`os/kernel-image/src/bootface_common.rs:210-275`：`assert!(module_table.len() >= NR_BOOT_MODULES)`、`entries.len() < NR_BOOT_MODULES`、basename、`len == 0`、`in_memmap_range`、bump 池相交、`dest_end <= pool.kern_start || dest >= pool.kern_end` 七处；文档 859 行只列四项。
- 最小修复：两节统一为同一清单（至少补「条目数 ≥ 12」与「目标页不落内核跨度」），或明确「四道输入门 + 三条断言」。
- 置信度：高。
- 是否需源码核实：已核实。
- 若我错了：若「四道门」只指输入的语义门、不含断言，则需在正文写明口径；当前未写。
- 对应判据：常量/签名/行为合同（检查项集合）；文档内部一致性。

## FINDING FACT-05（P2）

- 位置：第 859、2147、2550、2949 行。
- 严重度：P2。
- 问题：文档把防线 C 定义为「`alloc_module_pages` 拷贝前拒绝 + `assert_modules_clear_of_kernel` 装载后复查」，并称「三条 v1 发送腿」都走它。实际 `assert_modules_clear_of_kernel` 只在 `os/boot-shim/src/uefi_helpers.rs:107` 定义、`:1091` 调用，是 UEFI 腿专有；U-Boot shim 腿不调用它，其 C 保证来自编译期 `KERNEL_MAX_LMA_END <= MODULE_REGION_BASE`（`opensbi_helpers.rs:93-95`）与装载后 root/bump vs 内核跨度的另一组 assert（`:378-390`）。
- 影响：按文档核对 U-Boot 腿会找不到所述复查函数，误判「实现缺失」；防线归属被夸大。
- 证据或推理：E1。`grep -rn 'assert_modules_clear_of_kernel' os/boot-shim/src` → 仅 uefi_helpers 定义/调用/测试；`grep -n 'KERNEL_MAX_LMA_END' os/boot-shim/src/opensbi_helpers.rs` → 编译期断言 93-95。
- 最小修复：把 C 的两处命名复查限定为 UEFI 腿；U-Boot 腿改写为「编译期上界 + root/bump 复查」。
- 置信度：高。
- 是否需源码核实：已核实。
- 若我错了：若句子本意是「三条腿合起来用这套函数」，也应补上 U-Boot 的替代机制，否则读者仍会按统一机制理解。
- 对应判据：符号锚点；文档-代码同步（函数存在性与调用点）。

## FINDING FACT-06（P2）

- 位置：第 2067-2094 行（§4.2.2 分流规则表）。
- 严重度：P2。
- 问题：§4.2.2 以「`fn classify` 与单条判定 `fn adjudicate`」开题，随后把规则概括为「五步加两道内联闸」。实际 `classify` 在逐条 `adjudicate` 之后还有整图拒绝：同地址域描述符两两重叠 → `PolicyError::OverlapsDescriptor`；以及 `WIRE_MEMMAP_CAP`/`WIRE_RESERVED_CAP`/`WIRE_SOURCES_CAP`/`side_cap` 四类容量拒绝。这些 fail-closed 分支在本节与 §4.2.1 动作 6/7 均未列出。
- 影响：读者会以为分类只按单条五步判定，漏掉「重叠描述符整图作废」这条与 §2.5 模块重叠检查同级的规则。
- 证据或推理：E1。`os/boot-shim/src/ebs_map.rs` 的 `classify`（约 726-760 行）有 `OverlapsDescriptor(j)` 与四组容量检查；文档全文 `grep -n 'OverlapsDescriptor'` 零命中。
- 最小修复：在 §4.2.2 末补一小段「整图级拒绝」：重叠描述符 + 四个容量上限，并给出错误变体名。
- 置信度：中（若作者有意只描述 `adjudicate`，需在段首把范围写死，但现文点名了 `classify`）。
- 是否需源码核实：已核实。
- 若我错了：若「分流规则」仅指单条描述符的档位判定，则标题应改为「`adjudicate` 的五步与两道闸」，并另处交代 `classify` 的整图拒绝。
- 对应判据：覆盖率缺口（文档声称覆盖 `classify` 而漏 fail-closed 分支）。

## ZERO-FINDINGS（扫了什么、发现数、N=0 不覆盖什么）

1. `static mut`：文档零命中；唯一 `UnsafeCell` 引文（1397 行）与 `os/arch/src/arch/pt_alloc.rs:63` 一致。发现数 0。不覆盖：`static mut` 之外的可变性机制（文档未声称）。
2. 路径：doc-code-map 48/48；`§7.2/§7.3/§7.4` 全部列名逐项 `test -e` 通过；反引号路径 44 个候选中 15 个是 brace/glob/anchor 写法，展开成员逐一存在。发现数 0。不覆盖：非反引号散文路径与 02/03/04/05 篇目标。
3. 常量：`MULTIBOOT_HEADER_MAGIC=0x1BADB002`、`INFO_MAGIC=0x2BADB002`、`HAS_VBE=4`、`MAXMEMMAP=40`、`LIMIT=0xFFFFF000`、`PG_PAGETABLES=6`、`NR_MEMS=16`、`MULTIBOOT_MAX_MODS=20`、`MB_MODS_ALIGN=0x00800000`、`MB_MODS_BASE=0x82000000`、`GDT_SIZE=7`、`BOOT_PT_BUMP_PAGES=1024`、`PT_BUMP_PAGES=256`、`MEMMAP_REGION_STORE_LEN=128`、`VM_BOOT_HANDOFF_MAX_REGIONS=64`、`MAX_DEDUCTED=47`、`WIRE_END=7872`、`WIRE_SOURCES_END=29184`、`ARENA_CAP=32768`、`SOURCE_RECORD=296×72`、`KinfoStruct=48` 全部命中。发现数 0（FACT-01 除外）。不覆盖：trampoline 精确字节数（文档只给「约 2212 B」，论证只需 <4096）；历史 v8/v9 数字仅按 git 核对 v9=160/v10=128。
4. 签名/结构体：`prepare_boot(&mut BootStorage, usize) -> BootPrepareResult`、`build_kernel_info(&BootReport<'static>, RamAuthority)`、`arch_boot_finish<A,H>`、`enter_kernel_image_v1(PhysBytes,&BootReport,PhysBytes,&[(u64,u64)])`、`PreparedJump{entry,arena_pa}`、`FileLoader`、`KernelInfo` 10 字段、`BootHandoffV1` 18 字段、`cut_memmap(cbi,start,end)` 全部逐字段对读通过。发现数 0。
5. 测试名/数量：`cargo test -p minix-boot --lib -- --list`=187；`-p boot-shim --lib`=136；`-p minix-kernel --features mock --lib`=874（=866+8 ignored）；`-p minix-elf --lib`=27。文档 187/136/866+8/27 全部命中；`boot_windows_reject_*`、`decoded_map_survives_arena_poison_…`、`generates_arch_and_board_first` 等引用测试名均存在。发现数 0。不覆盖：测试实际执行（只 `--list`，未跑 pass/fail）；AGENTS.md 的 831/8 是旧读数，非本篇缺陷。
6. 覆盖率/反事实：§4.7 与 §7.5 列出的 C 函数全部存在；`alloc_lowest` 确实仅 `arch_smp.c` 一处调用；反事实前提（两条 push、粗档 else、`-1`、`a2` 通道、`bootstrap_len` 误设、`is_registered` 守卫）在现树或文档标注的历史中均成立。发现数 0。不覆盖：C 运行期行为（只按源码推理）、QEMU/真机结果、跨篇引用（02/03/04/05 未读）。
7. 沙箱边界：`doc-citation-check` 的 21 个 checked 块中 4 个 WARN 为声明改述，只核了 source-symbol 与过程编号，未逐行；其余 skipped 29 块中已人工对读 `head.S`、`get_parameters`、`mb_set_param`、`cut_memmap`、`overlaps`、`setup_mbi`、`kernel.lds`、`protect.c` 等，仅发现 FACT-01。

目标文件 sha256（实跑 `sha256sum rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`）：
`2e72aecc924c4833c22b24eabe333088f6c03ea7b847950bb6a1508658ef80d4`。
