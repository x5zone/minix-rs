# P-FACT-A · 事实审查（§1.x 引导器/固件断言 + §3.x 交接协议与 Rust 设计陈述）

**范围**：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 的 §1.1–§1.4、§3.1–§3.10；§2.x 与 §4–§6 由另一位事实审查者承担（本报告只对它们的机械门读数作背景引用，不对其内容下结论）。
**只读纪律**：除本文件外未修改仓库任何文件；未读取任何历史 scan.md / review 报告 / expected 目录 / 同目录其它面板输出。**披露一处越界读取**：为核对 §3.4.1 引用的缺口编号 `F-R34-01`，只读了 `../OPEN-DEBT.md` 的第 21 行（确认该 ID 与"L9 仅覆盖 riscv64"描述存在），未读取其中任何评审结论用于本报告的判断。

## 机械门读数（Step 0 强制，全部实跑）

| 门 | 命令 | 读数 | 判定 |
|---|---|---|---|
| 锚点解析 | `tools/anchor-resolve.sh --check <doc>` | `anchors=105 resolved=105 zero-def=0 multi-def=0`，EXIT=0 | PASS |
| 关联代码清单 | `tools/doc-code-map.sh <doc> --check` | `共 54 个文件；存在性 54/54 通过`，EXIT=0 | PASS |
| 表结构与内部链接 | `python3 tools/doc-structure-check.py <doc>` | `✅ 1 个文件表结构与内部 .md 链接均可解析`，EXIT=0 | PASS |
| 内部章节引用 | `tools/doc-ref-check.sh --check <doc>` | `1 处内部引用疑指向不存在章节：第 14 章/节`，**EXIT=1** | **FAIL（见 FACT-A-01）** |
| 文风 | `tools/doc-style-lint.sh <doc>` | `error级=0`，EXIT=0 | PASS |
| 引文对账 | `python3 tools/doc-citation-check.py <doc>` | `blocks=69 checked=28 failed=0 skipped=36 warned=5`，EXIT=0 | PASS，但覆盖率见 FACT-A-02 |

测试计数实跑复核（`cargo test ... -- --list`）：`boot-shim --features test-all --lib` = 136；`minix-boot --lib` = 196；`minix-elf --lib` = 23；`minix-kernel --features mock --lib` = 874（`cargo test` 实测 `866 passed; 0 failed; 8 ignored`）。§5.2.1 的四个总数与全部 21 行逐文件计数（`grep -c '#[test]'`）与实测**逐个一致**。

---

## FINDING FACT-A-01（P2）
- 位置：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` §3.4.1（第 1614 行）："`authority` 是第 14 个字段（连同它一起从 13 增加到 14）"
- 严重度：P2
- 问题：`tools/doc-ref-check.sh --check` 把序数短语「第 14 个字段」当作章引用解析，报「第 14 章/节」并以 EXIT=1 退出，机械门红。
- 影响：该门是文档 review 的 Blocker Gates 之一；红门使本篇不能进 CONVERGED 状态（无论事实是否正确）。本节其余内容（14 个字段的计数）经 git 实证正确：`f80ae358b` 删除 3 字段后为 13，`826980573` 加入 `authority` 后为 14 — 所以这是门误报，不是事实错误，但读数必须处置而不是改写门。
- 证据或推理：E1。`tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` → `⛔ ... 有 1 处内部引用疑指向不存在章节 ... · 第 14 章/节`，EXIT=1；工具正则 `/第 [0-9]+(\.[0-9]+)?[  ]*[章节]?/`（`doc-ref-check.sh:55`）在「第 14 个字段」上命中。全文 `grep -c '第 *[0-9]'` 的命中只有这一处。
- 最小修复：改写该句避开「第 + 阿拉伯数字」形态，例如「`authority` 是最后加入的字段（加入后共 14 个）」；改完重跑 `tools/doc-ref-check.sh --check`，要求 EXIT=0。
- 置信度：高
- 是否需源码核实：否
- 若我错了：最可能的反例是门在别处还有一处真引用错误而这条只是它的附带噪声——已核：该门输出只列 1 条，且全文无其他「第 N」形态命中。

## FINDING FACT-A-02（P2）
- 位置：§3.4.3 的 `struct BootHandoffV1` 引文块（第 1716–1755 行，注释写 `// 代码引用（略去字段文档注释；...源文件为英文原文`）
- 严重度：P2
- 问题：该块的引文标注是**跨两行**的 `//` 注释，且首行不含 `，路径` 分隔，`doc-citation-check.py` 的 `ANNOTATION_RE` 无法解析 → 该块被计为 SKIP「无引文标注」，全文最关键的一份 wire 协议结构因此完全在机械对账射程之外。
- 影响：门读数 `checked=28 / blocks=69` 看似绿，但 `BootHandoffV1` 的字段名、字段序、`#[repr(C)]`、`ARENA_CAP` 常量没有被工具核过一次；下一次信封字段漂移（历史上 v7→v8→v9 已连改两版）只靠人眼。这正是该工具 V3.4 自注所指的「D03/D04 的引文块全部静默跳过，failed=0 实为 checked=0 的假绿」同类形态。
- 证据或推理：E1。`python3 tools/doc-citation-check.py <doc> --json` 的 69 条结果里，位置 `1716-1755` 不出现（既不在 PASS 也不在 WARN/SKIP 带位置的集合里，SKIP 项全部 `location=None` 即「无引文标注」）。E1 人工逐字段对账结论：文档块与 `os/libs/minix-boot/src/handoff_v1.rs:299-330` 的 22 个字段名与顺序、`#[repr(C)]`/`derive(Clone, Copy)`、`pub const ARENA_CAP: usize = 8 * 4096;`（源 418 行）**全部一致**；`ram_authority` 行的〔编注〕已在块头声明（同一段的注加在字段注释里），`ARENA_CAP` 相关的三行中文注释已在块头声明为改述。即：**当前无内容错误，错的是覆盖**。
- 最小修复：把该块首行注释改成工具可解析的单行形态（`// 代码引用（略去字段文档注释），os/libs/minix-boot/src/handoff_v1.rs:struct BootHandoffV1。`），把〔编注〕与改述说明移到块后的正文或不换行的同一行；改完重跑 `doc-citation-check.py`，要求该块出现在 PASS 列表（`checked` 由 28 升到 29 以上）。
- 置信度：高
- 是否需源码核实：是（已完成，见上）
- 若我错了：最可能的反例是工具其实能解析跨行注释而我误读了 `--json` 的输出结构——已核：`--json` 输出里 SKIP 项确实不带 `location` 字段，而带位置的条目中不存在 1716 段。

## FINDING FACT-A-03（P3）
- 位置：§3.8 的 `impl FileLoader for UbootFileLoader` 块内中文译述（第 2591–2623 行），对照 `os/boot-shim/src/opensbi_helpers.rs:206-210` 的英文源注释
- 严重度：P3
- 问题：文档译述说「表里的长度已在生产者 `build_shim_table` 里按最后一个 PT_LOAD 修剪过（超槽容量时…截断到槽边界并打印 truncating）；这里对已在槽内的输入是 no-op」；而源码同处的英文注释说的是相反的事实——"The table stores the slot capacity, not the file length (the shim does not know the size before `fatload`)"。两者不可能同时成立。
- 影响：读者按文档走到源码对照时会看到自己刚读的结论被源码注释否定，无法判断该信谁。按真值优先序（代码 > 注释），**文档是对的**（`uboot_main.rs:363-395` 的 `staged_elf_used_len` 读暂存区 ELF 头的 PT_LOAD 求 `max(off+filesz)`，再 `used.min(slot)` 写入 len 字段），源码注释描述的是更早一版实现。但文档没有指出这一冲突，读者无从得知。
- 证据或推理：E1。`grep -n 'The table stores the slot capacity' os/boot-shim/src/opensbi_helpers.rs` → 206 行；`sed -n '363,395p' os/boot-shim/src/uboot_main.rs` → `let used = staged_elf_used_len(addr, slot).unwrap_or(slot); if used > slot { raw_serial_line("...truncating"); } let len_bytes = used.min(slot).to_le_bytes();`。
- 最小修复：在该块后已有一句「U-Boot 侧的生产者在 `build_shim_table` 里…」的正文（§3.2 亦有同述），在那句后补一个括注：源码同处英文注释仍写着旧行为（"stores the slot capacity"），以代码为准。或另开一笔修 `opensbi_helpers.rs:206-210` 的注释（属源码工作流，不在本文档修复范围）。
- 置信度：高
- 是否需源码核实：是（已完成）
- 若我错了：最可能的反例是 `staged_elf_used_len` 在非 ELF 或读失败时返回 `None` → `unwrap_or(slot)`，那一路确实写满槽容量，因此源码注释对"非 ELF 回退"这一支仍然成立——但文档译述已经明确把这一支排除在 no-op 之外（"对非 ELF 回退整槽的输入才生效"），所以冲突只在于"主路径到底是槽容量还是修剪长度"这一句上，判断不变。

## FINDING FACT-A-04（P3）
- 位置：§3.4.2 第 1673 行 `:const NR_TASKS`；§3.8 第 2097 行 `:fn alloc_pt_page`；§3.8 第 2178 行 `:fn classify`
- 严重度：P3
- 问题：三处无路径的冒号锚点（`：kind Name` 形态）不被 `anchor-resolve.sh` 抽取，因此完全无机械校验；全文 105 个被解析的锚点不含这三个符号。
- 影响：这三个符号若改名/消失，`anchor-resolve.sh --check` 仍报 105/105 全绿——与 FACT-A-02 同类的"读数虚高"。本次人工核实三符号均存在（`os/kernel/src/proc_table.rs:37 pub const NR_TASKS`、`os/arch/src/arch/pt_alloc.rs:125 pub fn alloc_pt_page`、`os/kernel/src/vm_handoff.rs:389 pub fn classify`），所以当前无事实错误。
- 证据或推理：E1。`tools/anchor-resolve.sh --extract <doc> | awk -F'\t' '$2==1673||$2==2097||$2==2178'` → 只返回同一行的**带路径**锚点（`proc_table.rs:const NR_PROCS>`、`pt_alloc.rs:fn register>`、`vm_handoff.rs:fn build_vm_handoff>`），三个简写不在结果里。Step 1.0 的锚点规约（`prompt/review-rules/review-process.md` §1.0 表）只登记 `path:fn NAME` 等形态，无路径简写不在射程内。
- 最小修复：三处改写为完整形态——`[`:const NR_TASKS`](<os/kernel/src/proc_table.rs:const NR_TASKS>)`、`[`:fn alloc_pt_page`](<os/arch/src/arch/pt_alloc.rs:fn alloc_pt_page>)`、`[`:fn classify`](<os/kernel/src/vm_handoff.rs:fn classify>)`；改完重跑 `anchor-resolve.sh --check`，`anchors` 总数应升到 108。
- 置信度：高
- 是否需源码核实：是（已完成）
- 若我错了：若 D-002「锚点采用短锚+链接」的裁决本意就允许"同路径续写简写"，那这属于受控写法而不是漏洞——但 D-002 说的是短锚**加链接**，而无链接的裸 `:fn X` 连可点击目标都没有，仍建议统一。

---

## SUGGESTION SUG-A-01（证据可升级）
- 位置：§1.1 第 9 行、§1.4 第 154 行的 boot2 `CR0.PG` 保留声明（"boot2 的 `CR0.PG` 实际值未逐例核实" / "boot2 交棒时的寄存器/栈契约不在本篇取证范围"）
- 现状与建议：本仓证据足以把这条从"未核实"升级为**代码事实**：`minix3/sys/arch/i386/stand/lib/realprot.S` 的 `real_to_prot` 只做 `or $CR0_PE, %ax; movl %eax, %cr0`（150–152 行），全 `minix3/sys/arch/i386/stand/` 目录 `grep -n 'CR0_PG'` 零命中，`CR0.PG` 的唯一写点在 `minix3/minix/kernel/arch/i386/pg_utils.c:223/235`（内核自己）。即"boot2 交棒时分页关闭"在本仓是可核的，不必只挂在规范上。建议补一句带锚点的陈述（保留"规范必要性只对 GRUB 类成立"的原有限定）。
- 对应判据：Step 1.0 锚点纪律（可核断言应带锚点）；review.md 真值优先序。
- 置信度：高；是否需源码核实：是（已完成）。

## SUGGESTION SUG-A-02（外部规范引用缺版本锚）
- 位置：§1.1 第 32 行"Multiboot 规范在「机器状态」一节把除少数几个寄存器以外的处理器状态都列为未定义，并明写「操作系统必须在自己需要栈时立刻建一个」"
- 现状与建议：句子给了规范小节名（"机器状态"），但没有版本号或出处定位（Multiboot 0.6.96 / 1.6 的 "Machine state" 段落措辞不完全相同；本仓 `minix3/sys/arch/i386/include/multiboot.h` 只有结构定义、不含该段文字，无法据此反查）。建议补规范版本（以及可优先引用的公开 URL），使这句引文可被第三方独立复核。
- 对应判据：style-bible「事实断言必须带锚点」；review-doc-checklist 来源锚点项。
- 置信度：中（规范原文正确性我按通行版本判定为真，但本仓内无副本可核）；是否需源码核实：否（需外部规范原件）。

---

## 已核为真的关键断言（阴性命中记录，供下一步复用）

以下逐条实跑/读源后**未发现偏差**，不构成 findings；列出以说明覆盖范围与"零命中"的含义：

1. **§1.x**：`load_stack` 位于 `head.S` 末尾 `.data` 段、`.space 4096`、`load_stack_start` 为高端符号（`head.S:93-97`）；`multiboot_init` 首条为 `mov $load_stack_start, %esp`（`head.S:68-69`）；`k_initial_stktop` 由 `mpx.S:634 LABEL(__k_unpaged_k_initial_stktop)` 定义、`head.S:84` 只引用，`objcopy --prefix-symbols=__k_unpaged_`（`Makefile.inc:64/67`）负责相遇——§1.4 的"命名陷阱"整段成立。§1.2 的 D5（`pg_identity` 铺满 1024 项、`pg_mapkernel` 从 961 起覆盖）经 `pg_utils.c:162-201` 成立。
2. **§3.1–§3.3**：`exit_boot_services` 引文块逐字一致且仅 `qemu-test-kernels` 门控；`prepare_boot(&mut BootStorage, bump_pages) -> BootPrepareResult<'a>` 签名与结构体一致；`FileLoader` 块逐字一致；`lib.rs` 确有 7 处 feature 门控（6 `uefi` + 1 `opensbi`，`grep -o` 实测），`uefi_helpers.rs` 确有 8 处 `qemu-test-kernels`；`impl BootShim` 在 `5ed051a53` 恰 1 处、当前树 0 处，`dyn BootShim` 在该提交 0 处；四份 manifest 的依赖面经 `cargo metadata` 复核与 §3.3 四份清单**逐项一致**（含 `boot-shim → minix-kernel` 仅 `enter_kernel_image_v1` + `init_boot_pt_alloc`，`minix-arch` 仅 `boot_dm_admissible_end`）；`*-unknown-uefi` 目标列表实测为 `aarch64/i686/x86_64`，不含 riscv64。
3. **§3.4**：`KernelInfo` 14 字段（含 `authority`）与块逐字一致；`MEMMAP_REGION_STORE_LEN=128`、`VM_BOOT_HANDOFF_MAX_REGIONS=64`、`VM_BOOT_HANDOFF_MAX_DEDUCTED=47`、`USER_STACK_TOP`、`NR_PROCS=256` 全部实测相符；`KinfoStruct` 7 字段 + 4 B `_padding` = 48 B，五个 Rust userland 读者（RS 的 `user_sp`、IS 的 `nr_procs`/`nr_tasks`/`release`/`version`）逐个找到；`freepde_start` 恒填 0、`vir_kern_start` 仓内零读者成立。**§3.4.2 的十条参数分歧里可机验的两条（重复键"首条时后写胜出"、`board=` 遮蔽）我用 C 重写 `mb_set_param` 实跑复现**：case A 得 `b=2,a=3`（后写胜出、只剩一条），case B 得 `x=9,a=1,a=3` 且 `get_value` 返回 1（首条胜出），case C 外部 `board=EXTERNAL` 被内核生成值删改后胜出——与文档"其二/其一/其三"的描述一致。
4. **§3.4.3/§3.4.5**：`BOOT_HANDOFF_V1_VERSION=9`；`ENVELOPE_CAP=256`、`WIRE_END=7872`、`WIRE_SOURCES_END=29184`、`WIRE_STARTUP_END=30624`、`ARENA_CAP=32768`、余 2144；`7616/21312/1440` 三段算术与 6144 B 旧分区（128×16+256×16）全部对上；`WirePlatformDescSource{kind u32,len u32,addr u64}` 与旧字段名 `reserved` 复用经 git 历史确认；`BootStorage::new()` 非全零（`ZERO_SOURCE` 的 kind=DTB=1）、`size_of::<BootStorage>() <= 16 KiB` 断言在 `bootface_common.rs:16`、直核腿闲置约 9 KiB（2048+4096+40×72=9024）——数字全部成立。
5. **§3.6**：`Paging` 成员实测 = `PAGE_SIZE` + 21 个方法，与文档所列**逐名一致**；`HugePages` 块逐字一致；x86-64 常量 `[GIB_1, MIB_2]`/首选 `GIB_1`/回退 `MIB_2` 一致；`supports_huge_page` 内 `cpuid eax=0x80000001` + `edx & (1<<26)` 一致；`AlreadyMapped` 守卫与"去掉守卫后写坏 1 GiB 块头部"的推演与源码分支结构一致；`plan_kernel_mapping` 尺寸选择段逐字一致，`layout_satisfies` 三对齐成立。
6. **§3.7**：`arch_boot` 三个 cfg 分支（239/292/786）+ mock 的 `arch_boot_test`；`arch_boot_finish` 与 `resume_additive` 文档注释两段引文**逐字**一致；`new_from_page` 在 x86_64/arm64/riscv64 三处都是 `write_bytes(ptr, 0, 512)`；`inherit_supervisor_half` 三架构都是 256..512；`jump_kernel_image_v1` 的 `in("x0") arena_pa` 与 x86 的 `__minix_x86_enter_image`（rbx）+ riscv `a0` 三臂一致；riscv `jump_to_kmain` 的 `auipc` 探针与 medany 推演一致。
7. **§3.8/§3.9**：`PG_PAGETABLES=6` + `panic("no more pagetables")`；`pg_alloc_page` 从最高条目尾部取页；`pt_alloc` 的 `PT_ALLOC/PT_REGISTERED/PT_CLAIMED` 三静态与 `register`/`alloc_pt_page` 块内容一致；VM 侧 `register`/`register_free` 成对且 `pt_free → vm_freepages → WMF_FREE → free_mem` 链成立；`build_vm_handoff` 调 `classify`；`memory_init` 两次自增 `freepde_start` → `freepdes[]`；`getinfo_kinfo` 固定 0。
8. **§1.4 五条栈名对照表**、**§1.2 四样/五约束**的符号落点（`add_memmap`/`cut_memmap`/`pg_identity`/`pg_mapkernel`/`_kern_phys_base`/`_kern_size`）与 C 源码位置一致；`kernel.lds` 的 `_kern_size = . - _kern_vir_base` 与 `__k_unpaged__` 双前缀机制一致。

**阴性声明的边界**（N=0 时不覆盖什么）：
- 锚点 105/105 只证明"符号可解析"，**不证明符号选对**（Step 1.0 明写工具不做语义）；我按 §1/§3 逐段做了句意复核，未发现错符号，但这是抽样强度的人工核对，不是穷举。
- `doc-citation-check` 的 `checked=28` **不覆盖** 36 个 SKIP 块（含 FACT-A-02 指出的 `BootHandoffV1`，以及 §2 全部 C 侧引文块——它们的注释是多行 `/* */`，工具只认单行形态）。§3 范围内我人工核了全部 15 个 PASS 之外的块，未见内容偏差。
- 测试计数对账只覆盖文档点名的 22 个测试函数名（全部存在，唯一例外 `test_build_kernel_info_bootstrap_zero_means_no_reclaim` 在文档里带删除线、标为"已移除"，实测 0 命中，属正确标注）与 21 行逐文件计数；未对未点名的测试做覆盖度判断。
- 真机读数（`res=127 zeroed=127`、`15 条 conventional`、`pa=0x5e9b000 / verdict=replayed`）只能核到"源码注释与探针代码里确有其事"，**不能核读数本身**——日志不在库内，这也正是文档自己声明的可靠性等级。
- 反事实推演（§3.5 过度回收表、§3.6 去掉 `AlreadyMapped`、§3.8 改按已用部分扣除、§3.7 两个替代做法）核的是**前提与代码机制是否真实存在**（均成立），不评价推演的完备性。

- 本次扫描：4 条 FINDING，2 条 SUGGESTION。
