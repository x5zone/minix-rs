# D01 循环轨迹 —— 01-boot-shim-bootstrap

## §0 基线

- 工作流版本：V1（`rewrite-notes/coordination/review-eval/WORKFLOW-VERSIONS.md`）
- 基线提交：`31f3c1829`（V1 脚手架提交；其上 `e364d5f92` 为 WIP 快照）
- 文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- 文档基线 sha256：`9f6a2f3922f8c004a9eecd396fad2961dca1eaab502a81c4ce033b7397f655ac`
- 基线行数：3086
- 工作树：除 `prompt/resources/` 未跟踪外干净
- 语料条目：D01（`CORPUS.md` 第二节）；盲区重点 B-04、B-06、B-08、B-12，另查 B-01、B-02、B-03、B-05、B-07、B-13
- 设计快照：`01-outline.v4.md`、`01-outline-review.v4.md`、`01-design.v3.md`；`design-coverage-check.sh 01-stage-kernel` 全 PASS
- 关联代码：44 个文件，`tools/doc-code-map.sh --check` 44/44 通过；清单见 `related-code.txt`
- 本篇形态：启动链 / boot shim / 三架构；高风险，按 `review-panel.md` 第七节启用全角色，包含 P-REFUTE
- 范围声明：本篇范围 = 文档主题（boot-shim 引导准备与控制权交接）+ `related-code.txt` 的 44 个文件 + 本轮修复触碰的文件。不在范围 = 相邻模块、未列入清单的文件、下一批次的结构性重构。

## §1 面板

- 起止：2026-11-07T03:49:56+08:00 至 2026-11-07T04:21:48+08:00。
- 角色与材料：
  - P-FACT：文档全文、44 个关联 Rust 文件、18 个 C 源、trait 读数、冻结决策编号；未读历史 scan 与其他面板输出。输出 `panel/fact.md`，17 条发现。
  - P-DESIGN D1：C 语义、文档 Ch1/Ch2、项目约束；任务书禁止现有 Rust、Ch3/Ch4、设计快照。**违规记录（meta-review 判定：需降级）**：报告开头自述读了文档标题大纲（含 Ch3/Ch4 小节标题，正文未读）、`kernel/const.h`、`servers/vm/utility.c`、`servers/vm/main.c` 的调用点，用于下游容量约束；报告已标注影响。按 `review-panel.md` 第六节，D1 输出应作废。meta-review 判 `INVALIDATED`：D1 的 14 条 DECISION 只作参考，不作为本轮设计发现来源；D2 的 15 条 FINDING 与主 agent 采纳的 5 条设计修复均来自 D2 的独立复核，不依赖泄漏材料。这是本轮流程缺陷，已登记 B-16。
  - P-DESIGN D2：D1 报告、现有 Rust、文档 Ch3/Ch4、trait 读数；未读其他面板输出。输出 `panel/design-d2.md`，15 条发现。
  - P-READER A/B/C：只读文档全文与 `reader-profile.md`；未读源码、其他文档、diff、工作流规则、其他面板输出。输出 `panel/reader-a.md`（16 条）、`reader-b.md`（20 条）、`reader-c.md`（17 条）。
  - P-REFUTE：文档、C 源、关联 Rust、设计快照（当作待反证主张）；未读其他面板输出。输出 `panel/refute.md`，6 条主张，其中 CLAIM 04/05/06 反证成立。
- 缩编：无。高风险篇目，全角色。
- 工作树完整性：面板前后 `git status --porcelain` 均只有 `prompt/resources/` 与新增 `runs/`，无面板写入源码或文档。
- 无发现面板：`design-d1.md`（独立设计提案，14 条 DECISION，只作参考）；`refute.md`（6 条 CLAIM，反证结果见 `defect-ledger.md` 的 `## P-REFUTE 附加` 表）。

## §2 发现

- 面板原始输出：`tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/`。
- 总计 85 条 `## FINDING`：P-FACT 17、P-DESIGN D2 15、P-READER A 16、B 20、C 17。P-REFUTE 另有 3 条反证成立项（CLAIM 04/05/06），内容已并入 FACT-15/03/05/07/09，不单独计为发现。
- 最高严重度：
  - P0：FACT-01..08（`FREE_MEMMAP`/Phase A.2、`classify_map`、arena 四页、`KernelInfo` 14 字段、`BootHandoffV1` 形状、`bootstrap_alloc_limit` 反向、`pt_alloc` 引文、注册点）、READER-A-09、READER-C-02/C-03、REFUTE-05/06。
  - P1：FACT-09..15、DESIGN-05/06/10/11/12、READER-A-05..10/12..14、READER-B-01/03/06/09/10/16/17、READER-C-01/C-04/C-07。
- 完整清单与逐条处置见 `defect-ledger.md`；未修项见 `follow-ups.md`。
- meta-review 新增三项否决（已在 §4 补修）：
  1. N1 测试计数按文档文件表口径漏了 `dtb_copy.rs`、`resv_claims.rs`、`fdt_layout.rs`、`lib.rs`；crate 全量是 125/175，不是 97/144。
  2. N2 DESIGN-10 的 `validate()` 要求 `boot_modules` 非空，三个 QEMU 测试内核构造 `boot_modules: &[]`，会在 QEMU 里 panic。
  3. N3 `kernel_info.rs` 的 `reserved_regions` 注释仍写“内核链进 boot-shim PE、恒等重放”，与 `vm_handoff` 的退役事实冲突。

## §3 归因

- 处置分布：① 37 条、② 48 条、③ 0 条、④ 0 条、⑤ 0 条。
- 冻结决策检查：0 条 `re-litigation`；D-001（四章骨架）与 D-011（主张表）没有被任何角色重新提出。
- 范围门：0 条 adjacent 被当阻塞；所有 ② 项只进 `follow-ups.md`。
- 误报为 0 不等于精度完美：reader B/C 的转出项可能含少量措辞偏好，留待下一轮闭环复核；本轮不把它们记为 ③，是因为它们都有可核的文档内依据，只是不在本轮阻塞范围。
- 证据强度：FACT 以 E1 为主，FACT-09/10 为 E2（按文档自述的 `#[test]` 口径复算）；DESIGN 以 E1 加代码事实为主；READER 为 E3（只引用文档内句子）。

## §4 修复

### 代码

- `os/kernel/src/lib.rs`：
  - `arch_boot_finish` 在 `store_kernel_info` 前调用 `kernel_info.validate()` 与 `check_boot_postconditions()`（DESIGN-10）。
  - `store_kernel_info` 补 `SOURCE_RECORD_STORE` 与 `STARTUP_RECORD_STORE` 两块深拷与重指，从 5/7 到 7/7（DESIGN-06）。
  - 新增两条溢出负控：`test_store_kernel_info_rejects_source_overflow`、`test_store_kernel_info_rejects_startup_overflow`。
- `os/arch/src/arch/pt_alloc.rs`：`uninit_alloc` 改为 panic；`alloc_pt_page` 前置 `is_registered()` 断言（DESIGN-12）。
- `os/libs/minix-boot/src/handoff_v1_read.rs`：`wire_module_to_boot_module` 返回 `Result`，`ValueOutOfRange` 不再折成零长；测试改 `.unwrap()`（DESIGN-12）。
- `os/kernel/src/boot_alloc.rs`：并发注释改为“单线程、原子类型不提供并发正确性；SMP 需 CAS 或 per-CPU”（DESIGN-14）。
- 注释数字同步：`handoff_v1.rs` 的 `WireSourceRecord` 48→72；`lib.rs` 与 `uefi_helpers.rs` 的 seven-page→nine-page。

### 文档

- FACT-01：`FREE_MEMMAP`/Phase A.2 陈旧链改为“内核不保留可变 memmap；`vm_handoff::classify` 做扣除”。
- FACT-02：`classify_map`/`ram_top` 两遍扫描改为 `classify`/`adjudicate` 五步判定与现行类型映射。
- FACT-03：六处“四页 arena”改为九页；`ARENA_CAP = 9 * 4096`，分区上界 `WIRE_STARTUP_END = 36480`。
- FACT-04：`KernelInfo` 引文补 `memmap_sources`、`startup_objects`；字段数 14→16；新增行补两项。
- FACT-05：`BootHandoffV1` 的 `reserved0`→`ram_authority`，补 `sources_byte_len`/`startup_byte_len`；“五块载荷/落地区”改为七个分区/落地区。
- FACT-06：`bootstrap_alloc_limit()` 无参、`checked_sub(1)`；`MaxAddress` 约束末字节，不再“减请求大小”。
- FACT-07/08：`pt_alloc::register` 引文补 `already`/`if already`；换手表第一行改为 shim 只 `init_boot_pt_alloc`，`register` 在 `boot_validate_and_prepare`。
- FACT-09/10：测试计数 53→97、65→144；逐文件表与总数、`vm_handoff` 7→10、`assert_bootstrap_outside_memmap` 3→5、cfg 五处→七处。
- FACT-11/12：删除不存在的 `reserved-big`；辅助形态改为“无 DTB 一次快照，有 DTB 两快照（O-11）”。
- FACT-13：`arch_boot` 引文改为 `arch_boot_finish`，并说明 x86_64 `arch_boot` 只委托；`enter_kernel_image_v1` 引文改 `BootArchImpl`、单次 `entry`、`cli` 前导。
- FACT-14/15：补 `DUMPDTB_PATH`；`uboot_main.rs:333`→395。
- FACT-16/17：四条→五条；Step 2c 补 ACLINT SSWI 运行期窗口。
- READER-A-05/06/07/08/12/13/14/16：分配接口、`test-kernel-map`、`[ARCH]`、`read_required`、§4.1 入口三段、`MapKey`、`NR_BOOT_PROCS`、§7.5 节号。
- READER-C-03/04/07：§3.1 传递参数改为 arena 基址；§3.8 加自引导路径限定；§4.6 LAPIC/IO-APIC 归属改为 Step 2c。

### 未修

- DESIGN-05（static mut 视图）与其余 10 条设计转出项、reader B/C 的结构/教学项、`minix-arch` 测试模块债：见 `follow-ups.md`。

## §5 复验

- `cargo test -p minix-kernel --features mock`：865 通过、0 失败、8 忽略；新增 2 条测试。
- `cargo test -p minix-boot`：175 通过、0 失败。
- `cargo check -p minix-arch -p minix-boot`：exit 0。
- `cargo clippy -p minix-kernel --features mock`：exit 0，76 条既有警告（基线 80，未新增阻断）。
- `cargo clippy -p minix-boot -p minix-arch`：exit 0，minix-boot 9 条、minix-arch 35 条既有警告。
- `cargo test -p minix-arch`：FAIL（45 个编译错误，基线遗留的测试模块债，非本轮引入；登记 D01-F01）。
- 测试计数复算：按 crate 全量 `src/*.rs` 的属性计数，`boot-shim` 125（含 `dtb_copy.rs` 15、`resv_claims.rs` 13）、`minix-boot` 175（含 `fdt_layout.rs` 29、`lib.rs` 2）；文档 §5.2.1 已改为全量口径。
- QEMU 测试内核编译检查：`cargo check -p test-higher-half -p test-higher-half-aarch64 -p test-higher-half-riscv64 -p test-kernel-map-aarch64 -p hello-boot -p hello-boot-riscv64 -p test-paging-enable-riscv64`，exit 0。未跑 QEMU 真机回归；DESIGN-10 的运行时 panic 风险以 `validate_before_tables` 解除，仍需下一轮 QEMU 门确认。
- meta-review：`rewrite-notes/coordination/review-eval/meta/01-boot-shim-bootstrap-review.md`；抽样 24/85（28.2%），处置一致性 87.5%，判 `NOT PASS`；总体 `IMPROVED`（基线已建立）。三条否决已补修。
- 文档五门：
  - `tools/anchor-resolve.sh --check`：anchors=98 resolved=98 zero-def=0 multi-def=0，exit 0。
  - `tools/doc-code-map.sh --check`：44/44 存在，exit 0。
  - `tools/doc-style-lint.sh --diff`：1 个文件 / 270 个新增行，error=0，exit 0。
  - `tools/doc-ref-check.sh --check`：内部章节引用均可解析，exit 0。
  - `tools/doc-code-density-check.sh --check`：4 个可判章均有码/命令，阻断项通过，exit 0。
- `tools/doc-snippet-extract.sh`：代码块分类产出正常，exit 0。
- `tools/eval-trace-check.py`：85 条发现全部有处置行，无孤儿行，metrics 自洽，exit 0。
- 修复段回读：本轮未单独派回读 agent；已修段的证据由 L4 五门与 cargo 测试覆盖，读者视角的修复段回读留待闭环轮（见 §8 第 5 条）。
- 台账重放：`FACT` 类修复的关键 `rg` 命令已在 fact 报告与本节门读数中重放；逐条 grep 重放留待闭环轮。

## §6 指标

- 见 `metrics.md`：发现 85、① 37、② 48、③ 0、④ 0、⑤ 0、`re-litigation` 0、`adjacent 阻塞` 0、重构提出 15/接受 5/实施 5/登记 10。
- 成本：7 个面板 agent，面板约 32 分钟；修复约 2 小时；token 未采集。

## §7 状态

**NOT CONVERGED（design-debt + teaching-debt + test-debt pending）**

- 代码 correctness 债：DESIGN-05（static mut 视图）转出。
- 结构性设计债：DESIGN-01/02/03/04/07/08/09/11/13 共 10 条转出；其中 DESIGN-11（GET_KINFO 载荷）需用户裁决。
- 教学债：READER-B-01/03/06/09/10/16/17 等结构/证据项转出。
- 测试债：`cargo test -p minix-arch` 的测试模块编译失败（D01-F01）。
- 已修 37 条，文档事实层的主要硬错误已清；未达到 `CONVERGED` 的原因不是“还有小问题”，而是上述三类债都有明确去处且尚未关闭。
- meta-review 总体 `IMPROVED`（V1 基线已建立），但运行质量 `NOT PASS`（抽样一致性 87.5%，低于 90%）；三条否决已在本轮补修，D01 仍不因补修改判 CONVERGED，需下一轮闭环复核。

## §8 给 meta-review 的不确定点

1. D1 已按 meta-review 判 `INVALIDATED`；D2 的 15 条 FINDING 与 5 条设计修复是否足以支撑“设计轴成立”，需要下一轮用不同形态的篇目再验证。
2. DESIGN-05 转出是否合理：当前调用图单次，但 `&'static` 承诺已被违反；本轮因测试多次 collect 未加守卫。请下一轮判“转出”是否应改为“本轮修”。
3. reader B/C 的 48 条转出项中，有多少是真正的教学债、多少是文风偏好：本轮未逐条判 ③，留待闭环轮。
4. `minix-arch` 测试模块债是否应阻断 D01：它不是发布门，但属于关联代码的测试面；本轮标 test-debt 而非 P0。
5. meta-review 判 `NOT PASS`（87.5%）；三条否决已在评审后补修，但补修本身没有再经过一次独立 meta-review。下一轮应先做一次闭环复核，再判 D01 是否可冻结。
6. QEMU 只做了编译检查，没有跑真机回归；`validate_before_tables` 的运行时行为需要 QEMU 门确认。
7. 测试计数已改为 crate 全量口径；下一轮建议用 `cargo test -- --list` 再对一次，避免属性计数与运行期用例的差。
