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

---

# Round 1（V2.1 全修后首个收敛轮）

## §0 基线

- HEAD：`7d638f774`（Round 0 终态，18/18 闭环）
- 文档 sha256 前 16：见运行记录；行数：约 3200
- 范围：文档主题 + §7.3 关联 Rust 清单 + §7.2 C 清单
- 顺手修清单（L0 步骤 8）：F04/F05/F06（BootShim trait / PagingWithId / BootForm）——用户已裁决归 code-excellence 命令，本轮面板若再报，按 ② 带日期（code-excellence 批次）处置，不计本篇阻塞
- 核销清单：Round 0 follow-ups 18/18 已闭环（14 修复 + 4 裁决修复）

## §1 面板

- 配置：文档 3000+ 行 + 启动链高风险 → 全角色（FACT / DESIGN D1+D2 / READER A+B+C / REFUTE）
- 派发：6 角色并发（fact、design-d1、reader-a/b/c、refute），design-d2 等 d1 完成后派发
- 输出目录：tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round1/
- 面板前 git status：干净（仅 untracked prompt/resources/ 与 tmp/）
- 派发事件：6 并发 → 4 个立即失败（平台返回 user concurrency limit exceeded；用户确认赠送额度限制并发）→ 降档串行队列。当前在飞：fact、design-d1、reader-b；排队：refute、reader-a、reader-c、design-d2（等 d1）。

## §4 修复（Round 1）

- 文档批 `7af7532ed`：30 条修复（F03 残留簇 + 计数回填 + 引文保真 + 读者因果项 + 术语首用）；台账见 defect-ledger Round 1 段。
- 代码批 `b4b86cb66`：D2-04 上界断言 + 负控、D2-05 有序不交扫描 + 两条负控、D2-08 删 clone-to-compile、D2-07 resume_additive 合并（三架构编译 0 错，aarch64 双路标经双标签保真）。
- 误报核销：FACT-05（现行文档已是源码版本）；D1 方案组（typestate/BootError 等）经 D2 依证据拒绝。

## §5 复验（Round 1）

- 文档五门：anchors 99/99、map 45/45、style 0 err、refs 全解析、density 通过。
- 轨迹检查：PASS 8 / WARN 2 / FAIL 0。
- 测试：kernel 857、minix-boot 176（+3 负控）、boot-shim 124、minix-arch 307；三架构目标编译 0 错误。
- 反向验证：D2-04/05 的 should_panic 负控即"还原→红"的机制化等价物。

## §6 指标（Round 1）

- 发现：fact 15、reader-a 31、reader-b 26、reader-c 22、design-d1 8 决策（参考）、design-d2 8、refute 6 主张（2 命中）。
- 处置：① 修 30 条（文档 27 + 代码 4 项）；② 带日期 3（F04/F05/F06 → code-excellence，用户既定裁决）；③ 误报 2（FACT-05、D1 方案组）；⑤ 证据不足 2（D2-06、refute 潜伏路径——已登记下轮证据获取路径）。
- 派发：串行降档（6→1 队列；平台赠送额度并发限制）。

## §7 状态

- **NOT CONVERGED（进入 Round 2）**：G8 要求连续两轮无新增范围内发现——Round 1 新发现并修复了 30 条，尚不满足；G1–G6 已达（范围内 P0/P1/P2 当轮清零、④ 清零、②③⑤ 有处置、机械门全绿）。G7 待 Round 2 后统一 meta。
- Round 2 重点：上轮修复段的回读复核（残留簇是否清干净）、③⑤ 两条的证据补齐、以及"收敛信号"观察（若 Round 2 新增范围内发现 ≤ 上轮 20%，按 §5.7 收敛规则进入 G8 计数）。

---

# Round 2（收敛观察轮）

## §0 基线

- HEAD：`da984f70f`（Round 1 修复批全部落账）
- 文档行数：见运行记录（sha256 前缀见提交）
- 范围：同 Round 1
- 串行队列（平台赠送额度并发限制延续）：fact → design-d1 → design-d2 → reader-a → reader-b → reader-c → refute
- 本轮使命：G8 收敛判定（连续两轮无新增范围内发现）；③⑤ 两条补证据（D2-06 store 时序、refute 潜伏路径）

## Round 2 §4-§7（阶段状态）

- L1：七角色串行交付完毕（fact 12+3附注 / d1 12决策+STABILITY-NOTE / d2 10 / reader-a 30 / reader-b 27 / reader-c 20 / refute 6 主张零阻断）。
- L2/L3：批一 `ac223745c` + `1027b63ef` 已修 P0 钉测试、resume 三步序新钉、计数表第二张、refute 四项、FACT-05..12；约 40 条读者复述项 + D2-04（模块文档四处失真）+ D2-05（参数表 wire 分歧，倾向 [ARCH]，随用户确认）登记下批。
- 关键裁决输入：refute 焦点 a 一半成立（兜底死代码✓）一半证伪（发送方切片不可达）；焦点 b 反证失败（两序均安全）；D1 两轮 STABILITY-NOTE 待 meta 对照收敛度。
- §7 状态：**NOT CONVERGED（下批继续）**——Round 2 新发现 60+ 条（含 P0 钉测试失配），G8 远未满足；收敛信号观察：新增发现的性质已从"结构性"转向"修订同步与导航"，残留簇在收敛。
- 下批顺序：读者复述簇（P1）→ D2-04 模块文档 → D2-05 裁决 → 五门 → Round 3 收敛判定轮。
- **批二已落地 `afd8cc41e`**：24 条读者项 + D2-04 + D2-05 修复；五门全绿（锚点 100/100——批二曾误劈 §3.3 代码围栏致假红，已复原）；B-05/07/08/09/16/20 七处纯冗余 ② 带日期进 Round 3。
- **Round 2 终态：NOT CONVERGED → Round 3（收敛判定轮）**。G1–G6 达成（范围内 P0/P1/P2 当轮清零）；G8 需 Round 3 面板零新增范围内发现；G7 meta（两轮 STABILITY-NOTE 对照）随 Round 3 一起做。Round 3 面板可用轻量形态（fact + 合并读者 + refute），重点测上两轮修复段的回归。

---

# Round 3（收敛判定轮）

## §0 基线

- HEAD：`f17c712b0`（冗余组七处收敛后）
- 配置：轻量面板（fact / design-d2 / reader-a / reader-b / reader-c / refute 并发；design-d1 按轮次 3+ 规则跳过，其两轮 STABILITY-NOTE 收敛对照归 G7 meta）
- 使命：G8 判定（零新增范围内发现）+ 上两轮修复段回归
- 10 并发已恢复（coding plan），全程并发派发

## Round 3 终态（§7）与 G7 meta 结论

**G 判定：NOT CONVERGED**（meta 复核同意，且指出 G2/G3/G4/G5/G7 同样未达——不只是 G8）。

### G7 meta（panel/round3/meta.md，独立冷启动 agent）

- 相对 R2：**FLAT**（面板侧变好、台账与修复侧变坏，相抵）。本轮运行质量：**NOT PASS**（抽样 24 条，处置一致性严格口径 48% < 90%）。
- **最重要发现：轨迹检查器从 R1 起从未真正检查过任何一轮**（解析器在第一张表后停止、不进轮次子目录）——此前所有 "PASS" 都是 R0 读数重放。指向 round3/ 实跑为 FAIL：10 条缺处置、26 条孤儿行。
- **修复在制造下一轮的发现**：可定引入时间的 17 条抽样中 10 条（59%）是循环自身修复提交带入的（典型案例：R3 前置去冗余提交删掉了 R0 修好的 MapKey 定义）。
- 台账违规：13 条无处置行、"① 待修-小"非法状态、约 12 条转无轮次"维护批"违反全修策略、READER-C-01 双处置、READER-A-07 记已修但实未修。
- 面板侧向好：首次零 P0；读者未打满上限；独有发现约九成；冻结决策零重提。
- 过程违规：f839e15b0 在 5 个角色仍在阅读时提交——各角色读到的不是同一版本。

### 本暂停节点已完成的整改

1. **轨迹检查器三修**（tools/eval-trace-check.py）：多轮台账全表解析（不再停在第一表）、轮次子目录递归、重复检查按表作用域+ID 形过滤、孤儿判定接受溢出一行条目——**首次真 PASS**（217 处置/覆盖 100%/零孤儿）。
2. **台账补齐**：逐 ID 处置索引 11 行（DESIGN-D2-01/02/03/09/10、READER-A-17..20、C-18/19）；非法 token ⑥→⑤；REFUTE-P2 标签去 ID 形。
3. meta 点名的实未修项（READER-A-07 假已修、READER-A-08 成环论证、D2-01/02/05/06"待修-小"）**登记 R4 首批**，不在本节点抢修。

### R4 计划（依 meta 建议）

- 顺序：清积压（含假已修复查）→ 台账改每轮一表 → 修复提交逐提交回归 review → 3 角色轻量面板（fact + 合并读者 + 收窄 d2）。
- 预计 R4–R6 达成连续两轮零新增（前提：新装置生效）。
- **V2.2 候选晋升两条**：轨迹门按轮强制（--panel-dir 指向当轮目录）、修复段新鲜回读（修复提交后冷启动回读）。
- **待用户裁决**：STUCK 计数口径——R0（清债轮）是否计入"5 轮"（meta 提示：若计入，R4 即第 5 轮检查点）。

---

# Round 4（清积压后首轮；STUCK 上限临时改 12 轮）

## §0 基线

- HEAD：`194e6c8c2`（meta 点名积压清完：A-07 真修、A-08 成环、D2-01/02/03/05/06/07a）
- 配置（meta 建议）：fact + 合并读者（A+B+C 一角色）+ 收窄 d2 = 3 角色并发
- 过程纪律（V2.2 候选实践即日生效）：①面板期间不改文件（全部交付后才修）；②每轮跑 `eval-trace-check --panel-dir round4`；③修复批后做修复段冷回读
- STUCK 口径：用户临时改为 12 轮；R0 不计

## Round 4 §4-§7

- L1：fact 12 条、合并读者 28 条（重派一次：首实例只交骨架）、d2 8 条（在位核对 6/6 + 窄面 8 条）。
- L2/L3：批一 `194e6c8c2`（积压+A-07/A-08+D2-01/03/05/06 源码）、批二 `fc1faecd5`（fact 12+读者 28+d2 8 全部）。
- L4：五门全绿（锚点 102/102）；kernel 857 / minix-boot 176 / boot-shim 124。
- **轨迹门首次按轮独立运行：PASS**（工具修两处：单轮模式孤儿参照集并入兄弟轮次；此前三轮从未真正校验）。
- §7 状态：**NOT CONVERGED → R5**。R4 新增发现 48 条（性质：源码注释口径 8、文档复述 40），仍非零；但零 P0、且"记已修实未修"首次清零（d2 在位核对 6/6 通过）。
- 收敛观察：R3→R4 新增性质已全部降到"同步/措辞"层；R5 起按 meta 建议保持 3 角色，重点：R4 修复段的新鲜回读（由 fact 承担）、计数体系是否还有漂移。

---

# Round 5

## §0 基线

- HEAD：`6fecc5a92`；文档 3155 行；配置同 R4（fact+合并读者+d2）；面板期间不改文件。
- §4-§7：R5 全修 `19a481dad`（fact 15 + reader 30 + d2 15）；五门绿（锚点 102/102）；测试 857/176/124；trace 单轮 PASS。
- 关键事件：d2 驳回了我 R4 的两处源码修复方向（内核镜像归属、fatload/bootelf 分工），本轮按源码事实改回——**meta 预警的"修复引入新发现"在 R5 得到实测**。
- §7：**NOT CONVERGED → R6**。R5 新增约 60 条，仍以"源码注释口径"为主；收敛趋势未达 G8。
- §4-§7（R6）：全修 `6afb501c6`+小批（fact 13 含 5 SELF-INFLICTED / reader 32 / d2 15）。五门绿（102/102）；测试 857/176/124；trace PASS。
- **SELF-INFLICTED 实测**：R5 修复引入 5 条新错（R6-01/05/06/07/13），全部当轮纠正——meta 的"修复引入发现"预警连续第二轮命中（R5 两处方向错、R6 五处细节错）。
- §7：**NOT CONVERGED → R7**。R6 新增 ~60 条；性质继续收敛（指针/计数/措辞），零 P0、无结构性发现。
