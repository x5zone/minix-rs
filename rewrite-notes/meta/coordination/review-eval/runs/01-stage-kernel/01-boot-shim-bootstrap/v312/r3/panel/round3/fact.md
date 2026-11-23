DOC 01-boot-shim-bootstrap.md

# 事实审查报告（R3 独立事实面板）

- 目标：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- 校验和：sha256 `98b7cdaf4864d67d3da615bcab4fc9c18f9e7bb2f41f72d634cd47cd1a2333ea`（与轮次声明一致）
- 行数：`wc -l` = 3049
- 只读：未修改任何仓库文件；本报告是唯一写入。

## 机械门读数（命令与输出摘要）

| 门 | 命令 | 读数 | 退出码 |
|---|---|---|---|
| 锚点 | `tools/anchor-resolve.sh --check <doc>` | `anchors=86 resolved=86 zero-def=0 multi-def=0` | 0 |
| 关联代码 | `tools/doc-code-map.sh <doc> --check` | 49 个文件，存在性 49/49 通过 | 0 |
| 结构 | `python3 tools/doc-structure-check.py <doc>` | 表结构与内部 .md 链接均可解析 | 0 |
| 章节引用 | `tools/doc-ref-check.sh <doc>` | 内部章节引用均可解析 | 0 |
| 文风 | `tools/doc-style-lint.sh <doc>` | `error级=0` | 0 |
| 引文 | `python3 tools/doc-citation-check.py <doc>` | `blocks=55 checked=19 failed=0 skipped=32 warned=4` | 0 |

补充：`doc-citation-check` 只识别块首行/围栏前一行的单行标注；本文多数 C 侧引文块与部分 Rust 块把标注写成多行 `/* … */`，因此被计入 32 个 SKIP（`--verbose` 显示大量 `[SKIP] block ?`）。这 32 个 SKIP 块由本报告逐块手工对账（见下“引文块”），不以机械门读数代替。

## 检查覆盖与阴性声明

1. **锚点与路径** — 扫了文档全部 86 个符号锚点（工具读数）与 49 条关联代码路径；手工补查了工具不认的 4 个手写行号锚点（`earm/head.S:22-24`、`ebs_pair.rs:219-221`、`exec.c:736`、`trap_api.rs:223-230`）：3 个正确，1 个错误（FACT-02）。N=0 的部分（符号锚点、路径存在性）不覆盖“行号锚点是否指向正确语句”，也不覆盖工具 `--extract` 未识别的锚点形态（`path:数字` 即其一）。
2. **文档-代码同步七项** — 静态可变：文档 0 处提到 `static mut`；路径：49/49 存在；常量：抽查并核对 `MULTIBOOT_MAX_MODS=20`、`MULTIBOOT_PARAM_BUF_SIZE=1024`、`MAXMEMMAP=40`、`NR_MEMS=16`、`MULTIBOOT_*` 标志位、`BOOT_HANDOFF_V1_VERSION=10`、`ARENA_CAP=32768`、`WIRE_*` 偏移与容量、`RESERVED_REGION_STORE_LEN=256`、`MEMMAP_REGION_STORE_LEN=128`、`PLATFORM_SOURCE_STORE_LEN=4`、`PARAM_BUF_STORE_LEN=1024`、`SOURCE_RECORD_STORE_LEN=296`、`BOOT_MODULE_NAME_LEN=16`、`VM_BOOT_HANDOFF_MAX_REGIONS=64`、`VM_BOOT_HANDOFF_MAX_DEDUCTED=47`、`MAX_BOOT_REGIONS=32`、`KERN_PHYS_BASE(riscv64)=0x80200000`、`KERN_PHYS_BASE(x86_64)=0x200000`，全部一致；内部 TODO：文档 0 处 TODO/FIXME；符号锚点：86/86；工作区前缀：文档引用均为 `os/…`；测试计数：见第 3 项。N=0 的部分不覆盖历史版本读数（7→8、9→10 的字节数、事故记录），这些无归档日志，只能核对其不冒充现役代码事实。
3. **测试名对账** — 扫了文档点名的 8 个测试函数：`generates_arch_and_board_first`、`rejects_internal_key_override`、`drops_tail_records_whole_at_capacity`、`splits_nul_and_whitespace_records`、`rejects_malformed_and_non_ascii`（`boot_params.rs`）、`decoded_map_survives_arena_poison_and_reissues_the_arena_frames`（`vm_handoff.rs`）、`boot_windows_reject_reversed_ranges`、`boot_windows_reject_empty_ranges`（`lib.rs`），8/8 存在。测试数量实跑：`cargo test -p minix-boot --lib` = 189 passed/0 failed；`-p boot-shim --lib` = 136/0；`-p minix-kernel --features mock --lib` = 866/0/8 ignored；`-p minix-elf --lib` = 27/0，与 §5.2 的 189/0、136/0、866/0/8、27/0 逐项一致。N=0 的部分不覆盖非 `--lib` 形态（集成测试、目标机测试）与门脚本实际运行。
4. **常量、签名、行为合同** — 扫了 `KernelInfo`（10 字段）/`BootReport`（9 字段）/`KernelInfoView`（9 方法）、`BootHandoffV1`（18 字段、128 B、12 处 `offset_of!` 断言）、arena 六个分区偏移、`BootStorage` 六个 `fill_*`、`prepare_boot` 与 `prepare_and_enter_kernel_image` 与 `enter_kernel_image_v1` 与 `boot_validate_and_prepare` 的签名、`check_boot_postconditions` 单断言、`validate_view` 三档、`adjudicate` 五步、`RETURN` 类型（`!`）、C 侧 `get_parameters`/`mb_set_param`/`overlaps`/`add_memmap`/`cut_memmap`/`alloc_lowest`/`pg_alloc_page`/`setup_mbi` 的签名与字段。除 FACT-01/02/03 外未发现不一致。
5. **引文块** — 扫了全部 55 个围栏块：工具判 19 PASS、4 WARN、32 SKIP。对 32 个 SKIP 中有源码路径声明的块逐块做了“去掉行内注释后按序子序列比对”，除两个明确声明“代码行按语义重排、非逐字”的块（`boot_alloc.rs` 的 `alloc`、`uefi_helpers.rs` 的 `prepare_and_enter_kernel_image`）外，所有保留下来的代码行都能在声称的源文件里按序找到；4 个 WARN 块中两个是上面这两块，另两个（`pt_alloc.rs`、`opensbi_helpers.rs`）只声明“注释译述/节略”，其代码行同样按序命中。N=0 的部分不覆盖：声明为非逐字的块（内容正确性未逐行对账）、无路径标注的示意块、以及文本块（`boot.cfg` 那一行已单独核对，与 `minix3/etc/boot.cfg.default:4` 逐字一致）。
6. **覆盖率缺口** — 扫了 §2 出现过的 C 函数名与 §4.7 对照表：`multiboot_init`、`pre_init`、`get_parameters`、`mb_set_param`、`add_memmap`、`cut_memmap`、`alloc_lowest`、`overlaps`、`pg_clear`、`pg_identity`、`pg_mapkernel`、`pg_load`、`vm_enable_paging`、`pg_alloc_page`、`alloc_pagetable`、`pg_map` 全部有落点行；未发现“正文讲了、对照表漏了”的函数。N=0 的部分不覆盖 §2.7 的 ARM 侧辅助函数（`find_value` 等未被声称覆盖）。
7. **反事实前提** — 扫了文档二十余处“若…会怎样”的反事实（push 对调、删删除循环、粗档降级、去 `-1`、去 `overlaps`、半深拷、`bootstrap_len` 误设、静态池搬 64 位、借 PML4 槽、按已用扣除、内联执行、重建活动根、内核直依 boot-shim、注册挪后、模块名互换、去 `bootstrap_len==0` 断言、去 `is_registered` 守卫、降级 `assert_bootstrap_outside_memmap` 等）：前提引用的命令、函数、字段、常量均真实存在（`load_mods`/`multiboot`、两条 `push`、`mb_set_param` 删除循环、`OVERLAP` 宏、`bootstrap_len` 历史字段、`PT_ALLOC`、`arch_boot_impl` 的注册点等）。N=0 的部分不覆盖反事实的后果推演本身（那属于语义论证，不在事实面板射程）。
8. **阴性声明** — 逐条复核了文档的“无命中/只有一处/输出 N”类声明：`grep -rn rootdevname minix3/minix/kernel/` 无输出（rc=1）、`rg alloc_lowest minix3/minix/kernel/arch/i386/` 命中定义/原型/唯一调用、`rg PG_ALLOCATEME …/i386/` 只有 `protect.c:381` 作为实参、`grep -n 'assert(magic == MULTIBOOT_INFO_MAGIC)'` 命中 `pre_init.c:219` 一行、`lib.rs` 7 处 `cfg(feature)`（6 uefi + 1 opensbi）、`uefi_helpers.rs` 10 处 `qemu-test-kernels`、`BOARD/board` 消费者审计（Rust 侧只命中 `boot_params.rs` 的规范化函数与测试字面量；`misc.rs` 的 `board_id` 是 C 布局位、不是参数表读者）。唯一为假的复算输出是 FACT-01；N=0 的部分不覆盖“未来代码漂移”。

## FINDING FACT-01（P2）

- 位置：§4.3（文档第 2277 行，`- 名字表有 12 项（复算：…）`）
- 严重度：P2（若按“文档-代码同步七项”的计数复算项计，可升 P1）
- 问题：文档给出的复算命令在当前源码上输出 `0`，不是声明的 `12`。
- 影响：读者按文档自带的复算命令核对，会得到与文字相反的读数；若据此判“表项数不对”，会把一条正确的计数（`NR_BOOT_MODULES = 12`）当成文档错误。该命令是本节唯一的可复算证据，失效后“12 项”只剩断言。
- 对应判据：文档-代码同步七项之“常量/测试计数复算”（AGENTS.md 的文档-代码同步门；复算命令的读数必须可复现）；事实断言须带可核对证据（本面板任务第 3、4 项）。
- 证据或推理：E1。逐字复跑文档命令：
  `sed -n '/pub const MODULE_NAMES/,/];/p' os/libs/minix-boot/src/kernel_info.rs | grep -o '"[a-z]*"' | wc -l` → `0`。
  原因：`MODULE_NAMES` 已改为 `BOOT_MODULE_IDENTITIES[i].name` 的派生视图（`kernel_info.rs:702-717`），小写名字字面量现在住在 `BOOT_MODULE_IDENTITIES`（`kernel_info.rs:685-700`）；同一命令改用 `BOOT_MODULE_IDENTITIES` 范围时输出 `12`（已实测）。
- 最小修复：把复算命令的 sed 范围改为 `/pub const BOOT_MODULE_IDENTITIES/,/^];/p`（实测输出 12），或在句子里改述为“`NR_BOOT_MODULES = 12` 由数组类型长度钉住”。
- 置信度：高
- 是否需源码核实：已核实（源码 + 命令复跑）
- 若我错了：唯一可能是文档以另一份工作树/另一提交为准；仓库内只有这一份 `kernel_info.rs`，且 `MODULE_NAMES` 的派生形态与文档同段的“`MODULE_NAMES` 与 `BOOT_MODULE_PROC_NRS` 都是该表的 const 派生视图”自相印证。

## FINDING FACT-02（P2）

- 位置：§2.1（文档第 251 行，`multiboot` 指令行），链接 `[exec.c](minix3/sys/arch/i386/stand/lib/exec.c:736)` 紧接文字“的 `snprintf(cmdline, len, "%s %s", file, args)`”。
- 严重度：P2
- 问题：行号锚点错一行：`exec.c:736` 是 `cmdline = alloc(len);`，被引的 `snprintf` 在 `737` 行。
- 影响：读者按锚点核对会先落到 `alloc` 行；更关键的是该锚点形态（`path:数字`）不被 `anchor-resolve.sh --extract` 识别，完全绕过机械门——同一写法若再出现，任何门都不会报。
- 对应判据：锚点纪律（`prompt/review-rules/review-process.md` §Step 1.0：行号锚点只允许工具派生后缀，禁止手工维护）；文档-代码同步七项之“符号锚点”。
- 证据或推理：E1。
  `grep -n 'snprintf(cmdline' minix3/sys/arch/i386/stand/lib/exec.c` → `737: snprintf(cmdline, len, "%s %s", file, args);`
  `sed -n '736,737p' …` → 736 为 `cmdline = alloc(len);`，737 为 `snprintf(...)`。
  `tools/anchor-resolve.sh --extract <doc> | grep exec.c` 只列出两条 `exec.c:exec_multiboot`（文档第 252、261 行），`:736` 未进入锚点清单，`anchors=86` 的读数不含它。
- 最小修复：把链接目标改为 `…/exec.c:737`，或改用符号锚点（同段已用 `exec.c:exec_multiboot`，可写成“`exec_multiboot` 里的 `snprintf`”）；顺带全仓检查 `path:数字` 形态（本文共 4 处，其余 3 处已核对正确）。
- 置信度：高
- 是否需源码核实：已核实
- 若我错了：若该 `:736` 本意指向 `cmdline = alloc(len);` 这一行作为“拼串”的起点，则锚点不算错；但文字把它与 `snprintf` 直接绑定，读法只有一个。

## FINDING FACT-03（P3）

- 位置：§3.3（文档第 1396 行，`SourceRecord` 给每个投影字节记来源：…）
- 严重度：P3
- 问题：字段列举只给 5 个（`kind`/`origin`/`range`/`input_object`/`attribute`），实际 `SourceRecord` 有 11 个字段，且句子没有“等/主要字段”之类的范围声明。
- 影响：读者会把来源记录的结构收窄成五个字段，看不到 wire 上真实存在的 `id`、`local_source`、`snapshot_id`、`virtual_start`、`address_domain`、`raw_type`；后文“每个投影字节有来源记录”的审计判据无法与结构逐字段对上（例如 `snapshot_id` 决定记录属于哪一次 `GetMemoryMap` 快照）。
- 对应判据：本面板任务第 1 项（每个事实断言须与可解析的符号/来源一致）与第 4 项（结构体字段须与源码一致）；部分列举不得以冒号列表冒充全集（AGENTS.md 的“抽象语义，不描述机制”之外的文档精确性要求）。
- 证据或推理：E1。`sed -n '492,517p' os/libs/minix-boot/src/memmap_source.rs` 的 `pub struct SourceRecord` 字段为：`id`、`kind`、`origin`、`range`、`input_object`、`local_source`、`snapshot_id`、`attribute`、`virtual_start`、`address_domain`、`raw_type`，共 11 个；文档句只列 5 个，无省略标记。
- 最小修复：句末加“等”或“主要字段”，或按“身份（id/local_source/input_object/snapshot_id）—范围（range/address_domain）—分类（kind/origin/raw_type）—固件原值（attribute/virtual_start）”分组补全。
- 置信度：中
- 是否需源码核实：已核实
- 若我错了：作者可能有意只列 §3.3 审计判据用到的字段；但 `raw_type`、`snapshot_id` 也参与 UEFI 来源语义，且句中没有“本节只用这五个字段”的限定，故按事实缺口报。

## SUGGESTION

1. §4.4 的 `arch_boot_finish` 节选（文档 2389–2414 行）省掉了 `where A: BootArch, H: crate::boot::HigherHalf` 子句与 `register_panic_diagnostic();` 一行，块内没有省略号；同节 `land_and_store` 节选（2502–2539 行）把源码顺序 `store_kernel_info → store_boot_report → land_and_store` 改成 `land_and_store` 在前，并省掉了 `KernelInfo` 构造所在的 `unsafe { … }` 括号。建议在块内用 `……` 标出这些删行，或在块头补一句“按调用关系重排、省略 where 子句与 panic 钩子注册”。（不属于事实错误：块头已标“节选”，工具子序列检查也通过；列在这里是为了读者按块抄代码时不丢 `register_panic_diagnostic`。）
2. §2.4.2.5 表“还引导段”行的动作用简写 `add_memmap(bootstrap_start, bootstrap_len)`，实际调用是 `add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len)`（`minix3/minix/kernel/main.c:301`）。建议补全首参与 `kinfo.` 限定，避免与 §2.4 给出的签名 `add_memmap(kinfo_t *cbi, u64_t addr, u64_t len)` 混读。

## 未展开根因

无。以上 3 条 FINDING 分属 3 个根因（复算命令漂移、手写行号锚点漂移、字段清单无完备性标记），均已在正文展开。
