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

---

# Round 7

## §0 基线

- HEAD：`6a9afd065`；文档 3163 行；配置同 R4-R6（fact+合并读者+d2）；面板期间不改文件。

## §1 面板

- d2（收窄）：R6 源码批逐条回读 + `os/boot-shim/src`、`os/libs/minix-boot/src` 窄面扫描；13 条，8 项回读中 4 项失败。
- reader（A+B+C 合并）：全文冷读 + 自我重读；初版 31 条（07:26）→ 中间版 11 条（07:42）→ 终版 2 条（07:45）。P1 两条（初版 A-01 判据表不可执行、C-01 省略段称 EBS 后有 GetMemoryMap）均已随修订解决。
- fact：重派实例；首版 16 条 + 2 溢出 → 终版 11 条（07:47），其余随修订解决。

## §2-§3 发现与归因

- d2 13 条全部 ①；其中 D2-01（aarch64 startup 记录借用 riscv bump 池）是前六轮均未发现的真实代码缺陷，D2-02（内核重叠防线只在无调用方函数、事后检查、未页取整）成立。
- fact 终版 11 条全部 ①；关键三条：M16"宿主无法构造"不成立（补 5 条纯函数测试）、`reserved_regions` 源码注释把 UEFI 腿误写 `&[]`（SELF-INFLICTED，改回非空）、§4.8 把 U-Boot 一族整体算进 v1 路径。
- reader 前两版 29+9 条随修订解决；终版 2 条（§4.4 表缺页大小行、未定义 `entry_start_pa`）全部 ①。
- 计：终版面板 26 条（d2 13 + fact 11 + reader 2），无 ②/③/④/⑤（A-04 的"定义/引用互斥"子项在初版记 ③，终版已并入改写）。
- D2-03 的三处不成立断言已按"注册表不追踪真实入口"改写；保留/删除/接线属结构取舍，待用户裁决，不阻塞本轮。

## §4 修复

- 批一 `c85a93837`：d2 13 + reader 初/中版（含 §4.2 判据令牌表与五例手算、§6.1 判别力纠正、§2.4/§2.6 结算移位、§5.4 状态口径）。
- 批二（本轮收尾提交）：fact 11 + reader 终版 2 + reader 中间版 11 的残留；新增 5 条内核重叠防线宿主测试；`reserved_regions` 字段注释改回 UEFI 非空；§4.4 门链表补齐并去 `entry_start_pa`。
- 新增防线：`record_kernel_span` + `alloc_module_pages` 拷贝前拒绝内核重叠页；两条 UEFI 入口页取整重查；root/bump 装载后复核；文档 §4.2 步骤 2 与 §5.1 M16 同步。

## §5 复验

- 文档五门：锚点 102/102、style error 0、doc-ref 全解、doc-code-map 45/45、承载门通过。
- 测试：kernel 857（mock）/ minix-boot 176 / boot-shim 129+5。
- trace：`--panel-dir round7` PASS（26 条发现全有处置、零孤儿；台账 414 条处置）。
- 版本漂移：reader/fact 都在文档并发修订中自我重读，终版报告保留收敛记录；面板原始输出覆盖旧版属预期（tmp 不追历史）。

## §6 指标

- 终版发现 26（d2 13 / fact 11 / reader 2）；处置 ① 26 / ② 0 / ③ 0 / ④ 0 / ⑤ 0。
- 初版累计 44 条（d2 13 + reader 初版 31），其中 29 条随修订解决、2 条并入终版、13 条保留；fact 首版 16 → 终版 11，其余随修订解决。
- 修两批：`c85a93837` + 收尾提交；新增宿主测试 5 条。

## §7 状态

- **NOT CONVERGED → R8**：终版零 P0；P1 两条（D2-01 真实代码缺陷、FACT-01 证据面误判）均已修。新增 26 条仍非零，未达 G8 连续两轮零新增。
- 收敛观察：R5→R7 新增性质已从"方向错误"降到"归属/口径/重复句"；本轮新出现 3 条 SELF-INFLICTED（FACT-03、FACT-11、reader 中间版残留），修复批自身仍是主要新增源。
- 待用户裁决：BootForm 结构取舍（R7-FU-01）、内核 LMA 预留（R7-FU-03）、STUCK 计数口径。


# Round 8

## §0 基线

- HEAD：`1ceaf539d`；文档 3207 行 / 351986 B；配置同 R4-R7（fact + 合并读者 + 收窄 d2）；面板期间不改文件。

## §1 面板

- fact：8 条（P1 1 / P2 4 / P3 3）；首版 5 条，追加 FACT-R8-06/07/08。
- reader：25 条（P2 7 / P3 18），快照 3207 行全程未漂移。
- d2：R7 两批 20 个源码 hunk 逐条回读（14 通过 / 1 部分 / 5 失败）+ 窄面扫描 4 条，共 9 条（P0 1 / P1 1 / P2 3 / P3 3）。

## §2-§3 发现与归因

- 42 条全部 ①；两条需用户裁决的按"可取证说法"先收敛：a2 改"备用通道、仓内无生产者"（DESIGN-R8-01）、BootForm 结构取舍随 R7-FU-01（DESIGN-R8-08）。
- DESIGN-R8-02（pool 分配面仍可覆写内核）做最小修复（ELF 段拷贝 src/dest 不重叠断言 + M16 收窄），全局 allocator 守卫登记 follow-up。
- 三条 SELF-INFLICTED：FACT-R8-01（新测试未回填计数）、FACT-R8-04（store 幂等理由）、DESIGN-R8-03（页取整测试无判别力）。

## §4 修复

- 收尾提交：R8 全部 42 条（文档 25 条 reader + fact/d2 的文档面；源码注释与断言 17 处）。
- 源码要点：loader 段拷贝加 src/dest 断言；BootFileEntry 路径长度断言；内核重叠测试换判别案；VmBootHandoff v8 历史补齐；kerninfo/main/vm_server/handoff 注释按 D72VM 现状改写。
- 文档要点：§5.2.1 计数 129/11/118；Step 0-4 阶段表；§2.8 R/M 符号拆分；接收链委托壳口径；aarch64 直核门归因。

## §5 复验

- 五门：锚点 102/102、style error 0、doc-ref 全解、承载门通过、doc-code-map 46/46。
- 测试：kernel 857（mock）/ minix-boot 176 / boot-shim 129+5。
- trace：`--panel-dir round8` PASS（42 条发现全有处置、零孤儿）。

## §6 指标

- 42 条；处置 ① 42 / ② 0 / ③ 0 / ④ 0 / ⑤ 0。
- 新增性质：归属/指代/计数 30 条，源码注释时态 10 条，测试判别力 1 条，门覆盖口径 1 条。P0 1 条（a2 三说互斥）。

## §7 状态

- **NOT CONVERGED → R9**：零未修 P0（a2 已按可取证说法收敛，补证据待用户）；新增 42 条仍非零，未达 G8。
- 收敛观察：R7→R8 新增未降反升（44→42 持平），主因是面板自我重读与修复批引入（3 条 SELF-INFLICTED）；R9 起把"修复批回读"固定为 d2 的第一项任务。
- **R8 修补记**：fact 面板在 R8 提交后自我纠错并追加 `FACT-R8-09`——其初版 FACT-R8-04（"kmain 不再 store"）来自截断 grep 的误判；R8 修复采纳了该误判，R9 已按源码（三调用点、resume/kmain 同址）改回。教训：证据命令不得 `| head` 截断后当全量结论。

# Round 9

## §0 基线

- HEAD：`6321ce600`；文档 3212 行；配置同 R4-R8；面板期间不改文件。

## §1 面板

- fact：10 条（P1 1 / P2 5 / P3 4），含对 R8 采纳误判的自我纠错。
- reader：17 条（P2 5 / P3 12），快照全程未漂移。
- d2：R8 修复批 11 条（P0-fact 6 / P1 4 / P2 1）；R8 代码行为本身无误，问题集中在注释/文档改写。

## §2-§3 发现与归因

- 38 条全部 ①；核心事件是 FACT-R9-01/DESIGN-R9-02——R8 采纳了 fact 初版基于截断 grep 的误判，把 `kmain` 同址再存改写成"不存在"；R9 按源码三调用点恢复。
- 其余高价值：loader 跨段源破坏（R8 断言只查同段）、boot-shim 测试并行互污染、VmBootHandoff v8 历史混入内核侧 exclusions、BootForm(e) 永久 wfi 表述、SHIM_HEAP 8/16 MiB。
- SELF-INFLICTED：R9 新增 loader 跨段测试使 boot-shim 130/119，计数需当轮回填（已在同一批修正）。

## §4 修复

- 收尾提交：38 条全修。源码：loader 全源×全目标谓词 + 跨段测试、测试串行锁、v8 历史、BootForm(e)、kerninfo 静态文档、SHIM_HEAP、a2 主语、DRAM 范围闸。
- 文档：store 三调用点与同址语义、落地区/深拷定论、§4.2 省略块拆分、Step 阶段表校正、UEFI 术语、§6.4 脚本口径、计数 130/119。

## §5 复验

- 五门：锚点 102/102、style 0、ref 全解、承载门、doc-code-map 46/46。
- 测试：boot-shim 131+5（默认单线程；`.cargo/config.toml` 固定 RUST_TEST_THREADS=1，显式并行不承诺）/ minix-boot 176 / kernel 857；kernel-image 默认与 riscv64 双 feature 构建 0 错。
- trace：`--panel-dir round9` PASS（38 条、零孤儿）。

## §6 指标

- 38 条；处置 ① 38 / ② 0 / ③ 0 / ④ 0 / ⑤ 0。
- 源头：fact 10、reader 17、d2 11；其中 4 条直接来自 R8 修复批（SELF-INFLICTED）。

## §7 状态

- **NOT CONVERGED → R10**：零未修 P0/P1；新增 38 条仍非零，未达 G8。
- 趋势：R8 42 → R9 38，降幅小；但 P0 级从 1 升到 6（全部来自 R8 注释改写），说明"修复批回读"仍是主矛盾。R10 d2 第一任务继续是回读 R9 批。

# Round 10

## §0 基线

- HEAD：`a327032d6`；文档 3212 行；配置同 R4-R9。

## §1 面板

- fact：8 条（P0 1 / P2 2 / P3 5）。
- reader：17 条（P2 8 / P3 9）。
- d2：R9 修复批 10 条（P2 4 / P3 6）。

## §2-§3 发现与归因

- 35 条全部 ①。P0 是 riscv64 `kernel-image` 编译失败（`bootface.rs` 对 u64 取 `.0`，由 D01-F07 引入、CI 无 kernel-image build 作业兜底）；已修并双 feature 构建验证。
- 第二组矛盾：R9 的 test_serial 锁只覆盖 preflight 全局态，分配计数测试在显式并线下仍红；结论改为“本仓默认单线程，并行不承诺”，撤销台账/loop 的"8 线程全绿"假证据。
- U-Boot shim 在跨镜像叙述里的系统性错位（F01/F04/F05/F06/F09）按同一张腿表统一改写。

## §4 修复

- 收尾提交：35 条全修。源码：bootface u64、in_memmap 死码、零长谓词与负控、config 单线程声明、resv_claims 锁、MAX_IDENT/ptr::copy/rustdoc/注释。
- 文档：接线权威表补 aarch64 直核、§4.1 导航表腿归属、arena 两腿来源、六入口定义、PML4 边界、调用嵌套、§7.3 清单、计数 131/120。
- R9 迟到件 FACT-R9-11（§3.8 引文漏段）同批补省略标记。

## §5 复验

- 五门：锚点 102/102、style 0、ref 全解、承载门、doc-code-map 48/48。
- 测试：boot-shim 131+5（默认单线程）/ minix-boot 176 / kernel 857；kernel-image 默认与 riscv64 两个 feature 构建 0 错。
- trace：`--panel-dir round10` PASS（35 条、零孤儿）。

## §6 指标

- 35 条；处置 ① 35 / ② 0 / ③ 0 / ④ 0 / ⑤ 0。
- P0 1（编译失败）、P2 14、P3 20；SELF-INFLICTED 约 10 条（R9/R8 修复批的注释与计数残留）。

## §7 状态

- **NOT CONVERGED → R11**：零未修 P0/P1；新增 35 条仍非零。
- 趋势：42 → 38 → 35，缓慢下降；P0 由编译门兜住（本轮首次）。R11 d2 继续回读 R10 批，fact 增加 riscv64 构建复核。

# Round 11

## §0 基线

- HEAD：`2b3f10c10`；文档 3216 行；配置同 R4-R10。

## §1 面板

- fact：10 条（P0 2 / P1 1 / P2 3 / P3 4）。
- reader：18 条（P2 5 / P3 13）。
- d2：R10 修复批 10 条（P1 1 / P2 5 / P3 4）。

## §2-§3 发现与归因

- 38 条全部 ①。两条 P0 都是 R10 文本层引入：§4.6 引文仍印已删的 one-shot expect、§3.7 "六个组合全部 DTB 注入"与事实相反。
- P1 回归：R10 为回应 reader F07 把 §3.4 memmap 行改反（内核其实恒在可用表）；R11 fact/d2 双源确认并改回源码契约。
- 测试并行：分配计数改 thread_local 后 8 线程 131 全绿；preflight 锁保留。
- 其余：BootFileTable 门从 8 字节扩到整结构体 1296 字节、零长负控判别力、§4.1 腿表、§7.1/§7.2 清单、多处合稿残留。

## §4 修复

- 收尾提交：R10 迟到 6 条（09–14）+ R11 38 条。源码：thread_local 计数、BootFileTable 全宽门、零长测试几何、模块/池 `>` 判据、注释与引用扫尾。
- 文档：memmap 契约、arena 归属、六脚本枚举、权威表块引用、证据类型组合说明、§2.8/§2.4.1 叙述修正。

## §5 复验

- 五门：锚点 102/102、style 0、ref 全解、承载门、doc-code-map 48/48。
- 测试：boot-shim 131+5（8 线程全绿）/ minix-boot 176 / kernel 857；kernel-image riscv64 双 feature 0 错。
- trace：`--panel-dir round11` PASS（38 条、零孤儿）；R10 trace 补 14 条后 PASS。

## §6 指标

- R11 38 条；处置 ① 38。R10 终版 14 条（首版 8 + 追加 6）全部处置。
- P0 2（均为 R10 文本层）、P1 2（memmap 回归、零长测试判别力）、其余 P2/P3。

## §7 状态

- **NOT CONVERGED → R12**：零未修 P0/P1；新增 38 条仍非零。
- 趋势：42 → 38 → 35 → 38，平台期；修复批自身仍是主要新增源（本轮 P0/P1 全部来自 R10 批）。R12 为 12 轮上限的最后一轮：若仍非零，按用户指令写 STUCK-REPORT 并暂停待人工。

# Round 12（12 轮上限最后一轮）

## §0 基线

- HEAD：`a29ddfdb9`；文档 3216 行；配置同 R4-R11。

## §1 面板

- fact：7 条（P2 1 / P3 6）；明确无未修 P0/P1。
- reader：13 条（P2 4 / P3 9）。
- d2：12 条（P0 1 / P2 5 / P3 6）。

## §2-§3 发现与归因

- 32 条全部 ①（4 条残留 P3 已在台账标注并列入 STUCK-REPORT）。
- P0：R11 把 U-Boot 入口锚点改成不存在的 `fn main`；锚点门 104/102 → 改 `fn uboot_main` 后 104/104。
- P2：§2.8 三条/两条计数、§2.4.1 四前提归属、共享 v1 入口 arena 类别、BootFileTable 8 字节门、per-thread 计数正控、§5.2.3 通道声明。
- 残留：三处长段、EBS/wire 定义位置、§4.9 overlaps 口径、三处手写行号。

## §4 修复

- 本报告同批提交：32 条中 28 条全修、4 条残留（P3）。
- 源码：v1 arena 按腿注释、store 符号锚、per-thread 计数正控（132 条）、零长夹具合法化、SUM/magic/RISC-V 注释。
- 文档：锚点、§2.8 计数、四前提归属、术语表补"产线"、§2.4 结算移位、§5.2.3 通道口径、计数 132/121。

## §5 复验

- 五门：锚点 104/104、style 0、ref 全解、承载门、doc-code-map 48/48。
- 测试：boot-shim 132+5（8 线程）/ minix-boot 176 / kernel 857；kernel-image 默认 + riscv64 双 feature 0 错。
- trace：`--panel-dir round12` PASS（32 条）。

## §6 指标

- 32 条；处置 ① 32（4 条残留）。P0 1、P2 10、P3 21。

## §7 状态

- **STUCK（12 轮上限到达）**：无未修 P0/P1；新增 32 条非零，未达 G8。已写 `STUCK-REPORT.md`，暂停待人工裁决。

# Round 13（冻结基线；用户裁决 A+C 后首轮）

## §0 基线

- ReviewedDocumentRevision：提交 `62d443ce3`，sha256 `9a782e0cb041d6b702fa0e6d10b0705b5d5279a0f0ceddc0cdd7ec2963e2dccf`，3235 行。
- ReviewedRepositoryCommit：`62d443ce3`。冻结记录：`panel/round13/BASELINE.txt`。
- 面板期间不改文档、不改 `os/`（合同第一条）。

## §1 面板

- d2：4 条先落盘（2 P2 / 2 P3），另有一项"六个入口委托壳"自查后确认文档错、已改。
- reader：5 条先落盘（2 P2 / 3 P3），全部与"拆段动作"相关。
- fact：3 小时内未落盘，两次催写无效 → 记为**未交付**；事实面由 RB13 的 fact 角色补齐（合同第二条）。

## §2-§3 发现与归因

- 9 条全部 ①。性质集中在两类：上一轮"清 P3 残留"动作本身的实现质量（拆段只插软换行、双空行、列表条目被吞并），以及两处转述过期（SUM 注释、arena 分配点）。
- 无 P0/P1。

## §4 修复

- 批一 `030c1ae96`：9 条全修。文档：真实段落拆分（pandoc 复验）、Step1 列表改六条、指针改写、手写行号符号化。源码：bootface 的 satp 时点。

## §5 复验

- 五门：锚点 104/104、style 0、ref 全解、承载门、doc-code-map 48/48。
- 渲染复验：三处拆段 pandoc `<p>` 计数分别为 5/4；Step1 `<li>` 计数 6（与"六条"声明一致）。
- 测试：boot-shim 132+5（8 线程）。
- trace：`--panel-dir round13`（仅 d2 + reader，9 条）。

## §6 指标

- 9 条（d2 4 / reader 5）；处置 ① 9。fact 未交付，不计入分母。

## §7 状态

- **NOT CONVERGED**（零新增计数为 0 轮）。进入 RB13 回读轮，之后 R14。

## R13 统计更新（面板全部交付后）

- 面板交付：d2 4 + reader 30（F01–F30）+ fact 6 = **40 条**（P0 1 / P1 2 / P2 10 / P3 27）。
- 处置：① 40（全部已修或批一已覆盖）；0 误报。
- 三批提交：`030c1ae96`（9）、`f9280568e`（22）、本批（9）。
- 零新增计数：**0 轮**（R13 非零）。

### L6

- campaign meta：R0–R12 从未跑 meta，合同第六条要求补一次全量；本轮已派发 campaign meta（冷启动，产物 `meta/01-boot-shim-bootstrap-campaign.md`），**回读轮派发时尚未返回**，R13 的 meta 结论先按"未交付"记录，R14 前补齐。
- 本轮 meta 观察（主 agent 自记，非 L6 结论）：40 条里 30 条来自 reader、4 来自 d2、6 来自 fact；其中 **11 条是上一轮修复批自身引入或未生效**（F01/F02/F03 拆段未生效、FACT-01/DESIGN-02 转述过期、FACT-02 符号锚错、FACT-05 for 列表、FACT-06 M4 计数、F13/F22 计数口径）。SELF-INFLICTED 率约 27%，与 R6–R12 区间一致。

### L6.5

- 主导类别：**流程（修复质量）**——本轮最大一类是"修复批自身形式未生效/引用过期"。
- 资源选读（未读优先，本次首读）：`superpowers/skills/verification-before-completion`（证据先于断言）、`superpowers/skills/systematic-debugging`（先根因后修复）、`mattpocock-skills/engineering/retro`（按类别提改进）、`mattpocock-skills/productivity/writing-for-agents`（指针与信息层级）。
- **Q1 检测模式**：`verification-before-completion` 的 Gate Function（"claim 前必须在**本条消息内**跑完整命令并读输出"）正好接住本轮 B-29（拆段只报"行数变多"）与 B-30（转述未复验）。→ 已记入 `BLIND-SPOTS.md` B-29/B-30 的期望探测器列。
- **Q2 流程改进**：`retro` 的"机械违规先建确定性检查，规则只留给判断项"支持把 C-08（渲染层读数）做成 fix-guard 的机械项而非新规则；`writing-for-agents` 的"指针措辞决定触发率"支持把 `review-panel.md` 的冻结基线条款写成命令级指针（`ReviewedDocumentRevision` 字段）而不是叙述。→ 记入 `WORKFLOW-VERSIONS.md` 候选表 C-08/C-09/C-10。
- **Q3 立即可用**：下一轮修复批照单执行三条——①结构性修复附 pandoc/解析层读数；②"文档指控源码注释"句子当场重跑 grep；③每条 hunk 必须有台账行（这是 `retro` 的"guardrail 优先"与 B-28 的直接组合）。→ 已写入本节，R14 修复批执行。

## R14

### §0 基线

- 冻结：HEAD `d58206530`；文档 sha256 `f6be48e30b49f096e743551e1efe8876d2ec102ac6e29751bb25533ab53ca076`，3254 行；记录在 `panel/round14/BASELINE.txt`。
- **冻结漂移**：首次冻结写的是 `a752e3790`（sha256 `c56318ac…`）。该次派发的三个面板子 agent 在交付前中止，`round14/` 下无任何报告；随后 R13 fact 终版 9 条落库为 `d58206530`。按用户裁决重启面板，并在当前 HEAD 重新冻结，旧冻结作废（同 B-33 类：基线未随修复推进，已按"重新冻结 + 漂移记录"处理，不把旧基线结论混入）。
- 本轮角色：P-FACT、P-DESIGN D2、P-READER A/B/C 分三角色（不再合并 A+B，针对 campaign meta 的 B-35）。
- 材料隔离：三角色拿同一份冻结文档；reader 只拿正文与读者画像；fact 拿 C 源与 46 个关联代码文件；d2 只读 §3/§4 与现有实现。面板期间不改文档、不改 `os/`。
- 重启记录：首轮派发中 reader A、reader B 因 provider 拒绝失败（未产出报告），已按用户"可自行重启任务"授权原提示词重派。

### R14 §1 面板

- P-FACT 分三段（Ch1–2 / Ch3–4 / Ch5–7）：7 + 9 + 8 = 24 条（P0 0 / P1 3 / P2 11 / P3 10）。
- P-DESIGN D2：15 条（P0 1 / P1 2 / P2 6 / P3 6）。
- P-READER A：20 条（0/3/13/4）；B：32 条（20 条八字段 + 12 条压缩，自报 2/12/10/8）；C：11 条（1/3/2/5）。
- RB13 独立回读（子 agent 最终交付）：7 条，结论 NOT PASS（2 条 P1 打穿批三）。
- 原始输出：`tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round14/{fact-part1..3,design-d2,reader-a,reader-b,reader-c}.md`；切片在 `slices/`（±only 供 fact 分段，不参与发现计数）。
- 并发事故：首轮五个角色同时派发时，加上长期存活的旧 subagent 与主线程共 11 个并发，超过供应商上限 10，成批 `Provider rejected the model request`。按用户裁决"同供应商同款模型 + 可自行重启"，在槽位释放后按分段重派；未改串行。此事故记入 B-37（并发上限未纳入 L1 预算）。

### R14 §2 发现与 §3 归因

- 109 条发现（面板 102 = fact 24（7+9+8）+ d2 15 + reader 63（A20+B32+C11）；RB13 回读 7）。台账 109 行：① 71 + ①部分 4 = 75 条已修/部分修；② 29 条带档期转出（F-R14-01..26，R15/R16/R17）；③ 5 条误报（附反证）。无 ④——按 `review-evals.md` §4.1，读者体验项一律本轮修。
- 本轮最重的四条：DESIGN-01（DTB collector 一次性守卫 ⇒ aarch64 UEFI 同一次引导三次 collect 必 halt，P0 代码）；RB13-05（重复 `#[test]` + 丢属性，M16 正例从未执行，上一轮把 ③ 误报当结论）；READER-C-01（同一恒等映射机制两处相反故障语义，排障方向会反）；FACT-P2-01（辅助形态八步顺序与源码相反，正是 A+D 要消除的次序）。
- 误报纠正：RB13-03 旧台账记"③ 误报"，本轮独立回读证伪并改判 ①，见 `defect-ledger.md` 的同 ID 行。

### R14 §4 修复

- 批一 `4a0d45d38`：`collect_owned`（P0）+ 两处 post-EBS 消费者切换 + 新增 1 条 minix-boot 测试（176→177）；删重复 `#[test]`、恢复正例属性；文档 20+ 处事实/口径。
- 批二 `3416c4381`：§1.2 五条编号、§4.1 立 Step 编号、§3.4 两维度分段、arena/bump/LMA 定义、§6.4 GDB/-d、§7.1 描述、历史注释自包含、双空行。
- 代码面同时清理：`globals.rs` 的 `MAX_CANDIDATES` 陈旧注释、`os/kernel/src/lib.rs` Phase F 的 `bootstrap_len` 陈旧注释、`uefi_helpers.rs` 两处 "before any other allocation"。

### R14 §5 复验

- 文档五门：锚点 107/107、style error 0、ref 全解析、承载门 4/4、code-map 46/46。
- 测试：boot-shim test-all 134 / 默认 123（唯一名 134）；minix-boot 177；kernel mock 857；aarch64/x86_64 UEFI 交叉编译绿。
- trace：`--panel-dir round14` 解析 90 条，coverage 全绿（PASS 7 / WARN 6：WARN 均为切片文件不是报告）。

### R14 §6 指标

- 发现数 109（面板 102 / RB13 7）——R0 以来最高。面板口径：fact 24、d2 15、reader 63（含 B 的 12 条压缩项）。
- 处置：① 71 + ①部分 4 = 75（68.8%）；② 29（26.6%，全部带档期）；③ 5（4.6%）；④ 0。
- SELF-INFLICTED：RB13-01/02/05 与 FACT-P2-01/02 共 5 条源于上一轮 A+D 修复批（约 4.6%），低于 R13 的 27%。
- 成本：8 份报告（含切片 6 份）对 109 条；较 R13（81 条 / 4 份报告）产出密度下降，但检出的是 1 个 P0 代码缺陷 + 3 个新类别。
- 零新增计数：0 轮（R14 非零）。

### R14 §7 状态

- **NOT CONVERGED**（零新增计数 0 轮）。RB14 独立回读：19 项检查 18 PASS / 1 FAIL（§4.2 省略说明把 `cross_validate_config_dtb` 放进 41 行 startup 段，实际在 decode 之后、断言之前）；已修并重跑五门。L6 meta：FLAT / NOT PASS（58% 严格一致），收口动作已在本轮执行完毕（档期、压缩项、总数、④ 口径、metrics）。
- 下一轮：R15 全篇面板（新基线为本次 L6 收口提交）。按 Q3 三条执行：旧串 grep 归零、转出带档期、RB 先入账再冻结。

### L6

- 报告：`rewrite-notes/coordination/review-eval/meta/01-boot-shim-bootstrap-r14.md`（冷启动子 agent，366 行）。
- 结论：**FLAT**；运行质量 **NOT PASS**（严格规则一致 7/12 = 58%，实质可接受 9/12 = 75%，低于 90% 契约线）。
- 判定：Detection PASS（19 条高严重度里约 14 条落在既有盲区；新增三类：可复制命令从未执行、一次性静态守卫撞多次调用、测试身份≠测试总数）；Precision CONCERN（面板误报 0–1；reader B 结构项 35% 转出）；Independence CONCERN（RB13 冷启动抓出 2 条 P1，且推翻主 agent 自判的 ③；但 RB13 与 R14 面板并发，违反六步序）；Scope FAIL（25 条转出无档期、11 条 in-scope 被转出、8 条 ④ 写"不改"）；Trace FAIL（90/102/109 三个总数、metrics 停更、12 条压缩项无处置行）；Cost CONCERN（真实 81→109，+35%）。
- 本轮 L6 立即收口（不留给下一轮）：① 转出全部加档期 R15/R16/R17，补 `F-R14-26`（DESIGN-03 裁决截止 R16）；② 12 条压缩项补处置行（台账 109 行）；③ 总数统一 109，loop §6 与 metrics 同步；④ ④ 口径纠正为"读者体验本轮修"，8 条全部改判 ① 或 ③；⑤ `metrics.md` 写入 R14 行；⑥ RB14-01 修复（§4.2 省略说明与实际调用位置不符）。
- 残留：B-38/B-39/B-40 三条新盲区入账（OPEN）；B-17/B-18/B-20/B-22/B-31/B-36 复发，其中 B-17/B-31 由本次"加轮次前缀 `R14/` + 补压缩项"部分缓解，其余留给 R15 的装置改造。

### L6.5

- 主导类别：**账本与度量纪律**——本轮发现质量不差（1 个 P0 代码、3 个新类别），失效集中在"记了什么、怎么记"。
- 资源选读（未读优先，本次首读）：`fresh-eyes-review-dely0`（空上下文审查 + 每条带复现命令 + **反向验证：把 bug 装回去必须变红**）、`grillme-code-review`（双轴：Standards vs Spec，固定点先钉死再分派）。两条都支持同一件事：审查产出必须能被"装回去/钉固定点"证伪，而不是靠自述。
- **Q1 检测模式**：`fresh-eyes` 的反向验证直接接住 B-39——① 的验收不该是"五门绿"，而是"导致该发现的旧串全篇 grep = 0"。→ 记入 `BLIND-SPOTS.md` B-39 的期望探测器，候选规则 C-11。
- **Q2 流程改进**：`grillme` 的"先钉固定点、再分派"支持把 RB 与下一轮面板的时序写成硬门（RBn 产出必须先入账，R(n+1) 才冻结）；`fresh-eyes` 的"证据落盘、报告只写路径"支持 metrics 由台账自动生成（C-12），替代手写 metrics 与手写 loop §6。
- **Q3 立即可用**：R15 修复批照单执行三条——① 每条 ① 写"旧串 → 新串"，并在本条消息内跑全篇 grep 旧串（命中非 0 即未修）；② 转出项必须带 `R1x` 档期，否则 L2 不放行；③ RB 产出先入账再冻结下一轮基线。

## R15

### §0 基线

- 冻结：HEAD `9bd8c4c03`；文档 sha256 `f47fc6be4354cb042a9ddd2d57a7d76f27bf86b5745c8a1d31cec8a7f8f86ec6`，3275 行；`panel/round15/BASELINE.txt`。
- 角色：fact 分三段（Ch1–2 / Ch3–4 / Ch5–7）、d2、reader A/B/C 各自全文；7 个报告，主线程共 8 并发（≤10）。

### §1 面板

- fact：9 + 6 + 7 = 22（P0 0 / P1 1 / P2 8 / P3 13）；d2：18（0/3/12/3）；reader A：27（0/4/18/5）；B：20（0/6/9/5）；C：20（0/3/5/12）。合计 107。

### §2 发现

- 新类别集中：C 侧机制漏项（`prot_init` 重建、`VMCTL_CLEARMAPCACHE`）、跨表口径（M4"不读" vs TODO-01"不分配"）、mapping 表与防线表互斥（overlaps 拆分）、自伤残留（`MapError::OutOfPhysRange`、§4.6"Step 1–4"、§3.5 陈旧注释描述）。
- 自伤统计：FACT-P3-01/02/03、FACT-P2-01/03、B-04 共 6 条由 R14 修复批引入或未同步（5.6%）。

### §3 归因与 §4 修复

- ① 25 条：fact 19（P1-01..09、P2-01..05、P3-01..04/06/07）、A-04、B-04、B-06、C-01..03。
- ② 82 条：本篇 in-scope 结构与读者体验项 → R16（与 F-R14-01..12 合并）；跨 crate 重构与 `prepare_boot` 裁决 → R16（F-R14-13..26）。
- ③/④/⑤：0。

### §5 复验

- 五门：锚点 108/108、style 0、ref OK、承载 OK、code-map 46/46。
- 代码未改（本轮只有文档修正）。

### §6 指标

- 发现 112（面板 107 + RB15 5）；① 31（27.7%）/ ② 81（72.3%）。零新增计数 0 轮。
- 成本：7 份报告 / 面板约 35min；转出比例偏高，L6 需评估是否把 R16 的结构批提前。

### §7 状态

- **NOT CONVERGED**。进入 RB15 + L6；R16 先执行 R14/R15 转出的结构批，再跑新面板。

### R15 L6

- 报告：`rewrite-notes/coordination/review-eval/meta/01-boot-shim-bootstrap-r15.md`。
- 结论：**REGRESSED**；运行质量 NOT PASS（严格一致 8/16 = 50%，实质 75%）。
- 判定：Detection PASS（广度）；Precision CONCERN（误报 1–2、重复 4、纯措辞 ~14）；Independence CONCERN（冻结与并发守住；P-REFUTE 连续第 4 轮缺席；迟交 reader 读旧快照）；Scope FAIL（82 条 ② 中 55 条 in-scope；无 `F-R15-nn`；R16 欠账 ≥107）；Trace FAIL（sev 全 `—`、② 五模板、97/107 数字互斥）；Cost CONCERN（修复吞吐 −65%，欠账复利）。
- 立即收口：RB15 判 **NOT PASS**，RB15-01..05 已全修（§2.4.1 右余量、§2.6.4 四步、§4.6 Step 4 指针、M2 validate 口径、§5.2.3 时点）；② DESIGN-03 落地——删 `boot_form.rs` 与 6 条测试（裁决 A），minix-boot 177→171，文档计数同步；③ `DECISIONS-PENDING` 第 1 项标为已实施。
- 残留：B-41/B-42/B-43 入账（OPEN/DETECTED）；B-40/B-17/B-39 复发。R16 先建 open-debt 表，再跑面板。

### R15 L6.5

- 主导类别：**消化能力**——检测仍有效（新类别 5 个），瓶颈从"找"转到"修与记"。
- 资源选读（未读优先，本次首读）：`grillme-code-review`（双轴：Standards vs Spec，固定点先钉死）；`fresh-eyes-review-dely0` 的反向验证（把 bug 装回去必须变红）。
- **Q1 检测模式**：机制生命周期四问（建立/重建/通知/退役）命中 `prot_init`、`VMCTL_CLEARMAPCACHE`、PM 缓存、二次调用四处 → 记入 B-41 期望探测器。
- **Q2 流程改进**：C-14 跨轮缺陷身份键（同一缺陷第二次出现时合并计数，替代手工去重与借旧行）；C-15 轨迹门三源恒等 + 桶检测 + severity 非空（替代手写 metrics 与只自校验的门）。
- **Q3 立即可用**：R16 开工顺序——① 先建 open-debt 表（R14/R15 全部 ②，带模块与轮次）；② 每条 ① 写"旧串 → 新串"并当场 grep；③ 面板派发前数并发并给报告头写 sha/HEAD。

## R16

### §0 基线

- 冻结：HEAD `5193faf4e`；文档 sha256 `31f21532149f26b2ab0a8867ca4a49510837fd4aa16e46b947b48fada1fd23db`，3314 行；`panel/round16/BASELINE.txt`。
- 先做了 open-debt 结构批一（编号索引、§3.4/§4.2 分小节、§3.1.1 权威表、M17、省略标记），再冻结跑面板。

### §1 面板

- fact 26（10+10+6：P0 1、P1 1、P2 5、P3 19）；d2 17（0/1/12/4）；reader A 26（0/3/17/6）；B 30（3/11/11/5）；C 20（0/5/10/5）。合计 119。

### §2 发现

- P0 一条：§4.8 把 riscv64 入口写成三参数（R15 删 a2 后未同步文档）——自伤。
- P1：§4.2.2 手算例把 `TYPE_INDEPENDENT` 当 Step 4 可达（实际只有 RUNTIME 可达）；§2.4.1 右余量句自相矛盾；x86 stub 实参序未讲清；M12 真机覆盖声明超过脚本断言；U-Boot arena 三说互斥。
- 结构类：§1.2 编号冲突（框图 1–4 vs 规则 1–5）、元叙述密度、§3.4 前向依赖未标注。

### §3 归因与 §4 修复

- ① 22 条（P0 1、P1 2、其余 P2/P3）：见台账 R16 段。
- ② 97 条：面板新增的结构/设计项 → R17；`DESIGN-01` 的 U-Boot 跨度守卫另立 `F-R16-01`（代码修复，非文档）。
- ③/④/⑤：0。

### §5 复验

- 五门：锚点 108/108、style 0、ref OK、承载 OK、code-map 46/46。
- 测试未改（本轮只有文档修正）。

### §6 指标

- 发现 119（fact 26 / d2 17 / reader 76）；① 22（18.5%）/ ② 97。零新增计数 0 轮。
- 自伤 1 条 P0（R15 a2 删除未同步 §4.8），与 R15 的 6 条同族。

### §7 状态

- **NOT CONVERGED**。进入 RB16 + L6；R17 先做 R16 转出与 open-debt，再新面板。

### R16 L6

- 报告：`rewrite-notes/coordination/review-eval/meta/01-boot-shim-bootstrap-r16.md`。
- 结论：**FLAT**；运行质量 **NOT PASS**（终稿：严格 5/16 = 31.3%，实质 9/16 = 56.3%）。
- 判定：Detection PASS（复检面扎实，但无运行期验证、新类别单薄）；Precision CONCERN；Independence CONCERN（冻结与并发守住，RB15 先入账）；Scope FAIL（97 条 ② 里约 90 条按 §4.0 应本轮修，含 1 P0 + 14 P1）；Trace CONCERN（① 全部落地、旧串归零、三源总数一致；但 5 列、severity 全 `—`、② 五模板、B-31 量化 117/119 跨轮同名）。
- 判决落账：RB16 判 **NOT PASS**，四条残留（RB16-03/04/05/08）已全修；
- 立即收口：① `loop.md` fact 计数改为 1/1/5/19；② 台账 123 行改为 `R16/` 轮次前缀 + 报告自报 severity（轨迹门已支持前缀归一）；③ RB16-03/04/05/08 全修；④ F-R16-01/02 落盘 follow-ups 与 OPEN-DEBT。
- 残留：B-44（判决不落账）/B-45（同节兄弟）/B-46（源结构与外链门）/B-47（欠账源）入账，另 B-48/B-49；B-42/B-31/B-17/B-18/B-39/B-40 复发。R17 第一优先是那 1 条 P0 + 14 条 P1，而不是新面板。

### R16 L6.5

- 主导类别：**范围/轨迹纪律**——检测与修复本身可靠，但 ② 成了万能出口，跨轮 ID 让覆盖门近乎失效。
- 资源选读（未读优先）：`adversarial-review-steveclarke`（对抗式复核：主动找"这条修复是否只是把矛盾改写"）、`graham-code-review`（小步、可验证的改动）。
- **Q1 检测模式**：Markdown 源结构破损（表格被空行劈开）五门零覆盖 → 记入 B-44 期望探测器：全文 `|` 表块连续性检查，表头后必须紧跟分隔行且中间无空行。
- **Q2 流程改进**：C-16 外链基址统一检查（所有 `.md` 链接按文档目录解析，失败即门红），C-17 跨轮 ID 强制 `R<nn>/` 前缀并禁止无前缀行（轨迹门已支持）。
- **Q3 立即可用**：R17 施工顺序——① 先清 1 P0 + 14 P1（逐条旧串 grep 归零）；② 再按 OPEN-DEBT 清结构项；③ 每条 ② 必须引用本轮新建的 `F-R17-nn`，无行不放行。

## R17（清债批，非新面板轮）

### §0 基线

- 起点：R16 收口 `3fdb6f0ad` 后的工作树；文档 3262 行量级。本轮不冻面板，先清 open-debt。

### §1 处置

- 流程修复 `4da73a37e`：⑥ DESIGN 处置（本轮实施）、L3.5 实施步、fix-guard 重构纪律、G9 收敛门。
- 文档/代码清理：
  - 代码：DESIGN-06（DM 候选四遍历提取）、07（identity_windows 内联）、09（四条后置断言负测）、10（memmap∩reserved 交叉校验）、11（删 12 个转发 getter，106 调用点）、12（ValidationLevel 枚举）、13（ebs_failed 删除）、14（PagingWithId/HUGE_PAGE_SHIFT/PTE_HUGE_IDENTIFIER_BIT 删除）、15（Plan→ClassificationPlan）、03（删 UefiBootShim::prepare_boot，裁决 A）。
  - 文档：A-03（跨文档链接统一）、A-20（两道内联闸覆盖范围）、§1.1 拆段、§1.3 压缩、§2.4.1 四子节、§5 账本收口、§3.4.1 图例、§5.2.1 计数史压缩、§3.2/§3.10/§4.9 标题与口径。
- DESIGN-05 尝试：把 shim 与接收侧游标合并，QEMU U-Boot shim 门在 3/6 标记处 panic（`bump_end` 是 1024 页页表专用子区间，模块/arena 在其外），已回滚并按“需设计裁决”登记 R18。

### §2 复验

- 宿主：minix-boot 175、boot-shim 134+2、minix-kernel 857、minix-arch 287；三架构 kernel-image 与 riscv64 U-Boot shim 构建绿。
- QEMU：x86 shim bootmarks PASS、aarch64 shim bootmarks PASS、riscv64 U-Boot shim 6/6 PASS。
- 五门：锚点 108/108、style 0、ref/承载/code-map 46/46。

### §3 剩余欠账

- 13 行 = 4 个唯一 DESIGN 项（02 BootReport 跨 6 crate；04 arena 双写；05 bump 分区设计；08 FileLoader borrow）+ 1 条相邻（02 篇未生成）。文档类可修项已清零。

## R18 实施批（非面板；全部裁决项落码）

> 依据：`DECISIONS-PENDING-R18.md` §14.5 九步。R18 不启动面板（P-3：实施 → 固定基线 → 停止，R17-panel 待用户明确指示）。本节记录实施与回归，不产生新的面板发现。

### §0 基线

- 起点：`dd46841a2`（R17 收口后）；文档 3292 行、sha256 `c1b7c86c…`；工作树含用户扩展的 `DECISIONS-PENDING-R18.md`。
- 范围：D-1…D-8、P-1/P-2 的代码与流程装置；文档同步 `01-boot-shim-bootstrap.md`；不启动新一轮面板。

### §1 处置（按九步）

1. **D-6**：删除 `boot_validate_and_prepare` 的 memmap 兜底；`require_boot_region` 未注册即 panic；正负控 2 条。
2. **D-3**：`minix_boot::BumpRegion`（5 测试）；U-Boot 池原子游标 + 编译期分区断言；`prepare_boot` 运行期断言 root/PT 子区间次序；`BootAlloc` 复用 `is_consistent`。
3. **D-7/D-8**：`source_destination_overlap_stops_before_any_segment_write`；R7-FU-02→F-R16-01、R7-FU-03 归档。
4. **P-1/P-2**：`review-evals.md` §3.5 八字段判决账 + §3.6 结构/链接门；`tools/doc-structure-check.py`（8 项自测）+ CI 接线；`DECISIONS.md` 落 R18 十一行。
5. **D-1/D-2/D-4**：`BootStorage`/`BootReport<'a>`（5 测试）；UEFI 腿 storage 化；wire v9、语义分区删除、arena 8 页；`FileBytes<'a>` 两条固件实现。
6. **D-5**：`arch_boot_resume_v1` 生命周期合同；`decoded_map_survives_arena_poison_and_reissues_the_arena_frames`；`load_kernel_with_loader_gated` + `KERNEL_MAX_LMA_END` 编译期断言。
7. 全量宿主测试、三架构构建、三个 QEMU 门（见 §2）。
8. 固定提交 `10ecd8ef4`；文档版本/测试基线同步（本提交）。
9. 停止：不进入 R17-panel。

### §2 复验读数

- 宿主：`minix-boot` 184；`boot-shim --features test-all` 135 + `ebs_boundary_audit` 2；`minix-kernel --features mock` 860 + 8 ignored（合计 868）；`cargo build --workspace` 绿。
- 三架构：`kernel-image` riscv64 直核/ U-Boot 接收、aarch64 直核、`boot-shim` riscv64 U-Boot 构建绿。
- QEMU：`test-shim-bootmarks-x86.sh` PASS；`test-shim-bootmarks-aarch64.sh` PASS；`test-riscv64-uboot-shim.sh` 6/6 PASS。
- 文档门：anchor 108/108、style 0 error、ref 通过、承载门通过、code-map 46/46、结构/链接门 0 问题。
- 规则门：`check-review-rules` / `lint-review-rules` / `generate-derived-skills --check` / `diff-trae-skills --only-diff` 全绿。

### §3 剩余欠账

- `OPEN-DEBT.md` 0 行：F-R18-01 已实施；`R14/READER-A-16` 是 adjacent，归 `follow-ups.md`（目标 = 02 篇立项轮），按 §4.2 不阻塞本篇收敛。
- 本篇状态：**NOT CONVERGED（无新一轮面板）**；基线已封存，等待用户指示是否启动 R17-panel。

### R18 补记：F-R18-01（D-1 第二阶段）完成

- 范围：U-Boot shim 腿的 `BootStorage` 迁移。设计已由 D-1 裁决冻结（`BootStorage` 所有者 + `BootReport<'a>` 视图 + `FileBytes<'a>`），本轮只落码，不新增裁决。
- 变更：
  - `BootShim::prepare_boot(storage: &mut BootStorage, bump_pages) -> BootPrepareResult<'a>`；trait 结果携带借用 storage 的报告。
  - `BootStorage` 增 source/startup 记录槽；`DtbMapOwned` 携带 source 记录（`collect_owned` 不再是"只有两张表"）。
  - U-Boot 腿：内存图/保留区/来源/参数/模块/平台源全部写入 storage；arena 在 `prepare_boot` 内从同一 bump 池分配；五类 startup 记录返回前写齐；删除 `Box::leak`（模块表、平台源、参数）与 `STARTUP_OBJECTS` 静态量。
  - `BootReport::from_kernel_info` 删除（无生产调用者）；30 个 QEMU 测试内核改为直接持有 `KernelInfo` 与局部变量，不再构造 `BootPrepareResult`；`test-smp-aps`/`test-smp-topo-aarch64` 改用 `find_platform_sources_into`。
- 复验：`minix-boot` 183、`boot-shim --features test-all` 135 + 2、`minix-kernel --features mock` 868；32 个测试内核 `cargo check` 通过（`test-rt-birth-riscv64` 需构建期 `RT_BIRTH_ELF_PATH`，属既有构建注入，不在本批范围）；QEMU x86/aarch64 shim bootmarks PASS、riscv64 U-Boot shim 6/6 PASS。
- 欠账：`OPEN-DEBT.md` 为空；`R14/READER-A-16` 是 adjacent（02 篇未生成），按 §4.2 不阻塞本篇收敛。收敛仍未宣告：R18 的架构级改动使旧轮次结论失效，需在新基线上重跑面板满足 G8（G9 已满足）。

## R17 面板轮（2026-11-09，用户明确指示持续循环）

### §0 基线

- commit：`ec67e3d4f`（代码面同 `d82f481a6`，之后只有台账/规则提交）
- 文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`，3308 行，sha256 `544ad466…`
- 工作树：仅未跟踪 `0`、`prompt/resources/`（既有、非本轮产物）
- Step 0 预检：`ls .design/01-*` 五类齐全；`tools/design-coverage-check.sh 01-stage-kernel` → ALL DOCS COMPLETE
- 关联代码：`tools/doc-code-map.sh <doc> --check` → 46/46 存在
- 范围：文档主题 + doc-code-map 清单 + 本轮修复触碰文件；相邻模块与 02 篇不在范围
- 语料：`CORPUS.md` D01；冻结决策：`DECISIONS.md`（D-001–D-023）
- 技能调用：`review-loop`、`full-review` 已通过 Skill 工具调用；`review-process-skill`/`review-doc-skill` 不在运行时技能清单，按其规范源 `prompt/skill/review-process-skill.md`、`review-doc-skill.md` 加载（此例外记入 §1）

### R17 §1 面板

- 角色：P-FACT、P-DESIGN D1、P-DESIGN D2、P-READER A/B/C、P-REFUTE（启动链高风险，全角色）。
- 材料隔离：FACT 只拿正文/关联源码/工具；D1 只拿 C 源与 §1/§2；D2 拿 D1 输出、现有 Rust、§3/§4 与 trait-audit；A/B/C 只拿正文与读者画像；REFUTE 拿正文/源码/设计合同。均禁止历史评审、其他角色输出与 expected/。
- 技能：`review-loop`、`full-review` 经 Skill 工具调用；`review-process-skill`/`review-doc-skill` 不在运行时技能清单，按其规范源加载（例外记录）。
- 原始输出：`tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round17/`（fact 10 条、reader-a 16、reader-b 20、reader-c 15、design-d1 5 决策、design-d2 20、refute 6 主张；D1 首轮输出被截断，重派一次补完 5 条决策）。
- 违规检查：面板前后 `git status --porcelain` 仅有主 agent 的 loop.md/文档改动，无面板写仓库。

### R17 §2 发现

- 原始发现 87 条：FACT 10、READER-A 16、READER-B 20、READER-C 15、DESIGN-D2 20、REFUTE 6 条主张。
- 重复簇：FACT-05/06 = A-07 = B-03 = C-03 = CLAIM 03（测试计数）；A-01 = B-01（小节顺序）；B-06 = C-12（§1.4 指针）；FACT-01/02/03 = CLAIM 01/02/03 的对应主张。
- 设计第一遍：5 条决策；D2 否决其中 3 条（可变账、AllocAuthority、页目录 typestate），另指出 D1 垫片/Paging/恒等范围/测试门四类表述与实现相反。D2 接受 8 条低成本代码改进。

### R17 §3 归因

- ①（代码/事实修复）25 条：FACT-01..08、10；READER-C-01（跨节契约）；DESIGN-D2-04..13、17；CLAIM 05/06。
- ④（读者体验，本轮修）51 条：READER-A/B/C 全部条目。
- ③（误报/不改）10 条：FACT-09（行号正确）、CLAIM 04（反证失败、主张成立）、DESIGN-D2-01/02/03/14/15/16/19/20（D1 前提或表述，不改代码）。
- ②（转出）1 条：DESIGN-D2-18 → F-R17-04（`KernelInfo` authority 字段，契约变更待裁决）。
- re-litigation 0（未命中 DECISIONS 且无新证据的重复项）；adjacent 阻塞 0。

### R17 §4 修复

- 文档：§3.6 HugePages 引文/理由/否决表；§3.8 BootAlloc 引文与反事实；§3.7 enter 引文、arena 生命周期措辞；§4.4 builder 分组；§5.2.1 计数表与拆解；§2.4.2 切除机制；§3.4.4/3.4.5 顺序；§4.2.1 时间序列；§2.2 栈对齐裁决；§1.2 D1 与执行序；§2.6.1 SMEP 前提；§2.4.1.3 四前提锚点；§3.1 EBS helper 分工；§3.2 腿/bump 定义；§3.7 计数单位；§6.1 通道措辞；§4.6 五类错误；§7.5 栈符号；§1.4 快照/恒等窗口；§3.1.1 指针；§3.5/§3.9 wire 账；§2.8 证据分账；§3.9 前提回指；§2.4.1.4 开篇；§2.6.2 开篇；§4.6 pg_clear；§2.6.3 标题；§3.4.1 字段删除定义；§3.10 权威规则；§1.3 读法；§6.2/§1.4 计数承诺；2.4.1.1 索引导语；§4.3 UbootFileLoader 引文；arena v8 九页；四地址；M13/M16；模块扣除 UEFI 限定；§1.4 五步消费者；壳层级；两类形态；§4.1 反事实入口；§2.3 board/arch；overlaps 索引；§3.4.5 source/startup 容量；§2.4.1.4 与 §3.4 的 64 条上限；`kern_stack_top` 清零口径。
- 代码：`pt_alloc` 写者 CAS 抢写；`BootStorage::commit_map_tables`；`build_kernel_info` 收 source/startup 两参（两直核腿先算后传）；`globals.rs` 四容量编译期等式 + `FREE_SET_CAP` 复用；`for_each_live_bootstrap` 统一两处占用枚举；删除 `classified_descriptors`；`push_module` 收进 `#[cfg(test)]`；三条腿补打参数拒绝计数；`WIRE_MEMMAP_CAP` 加推导注释；`OwnedDtb` 生命周期注释；`BootShim` 文档去掉已删除的 UEFI 实现描述。
- 测试：`minix-boot` 183、`boot-shim --features test-all` 135+2、`minix-kernel --features mock` 868、`minix-arch` 287；riscv64 直核与 aarch64 直核 `cargo check` 通过。
- QEMU：x86 shim bootmarks PASS、aarch64 shim bootmarks PASS、riscv64 U-Boot shim 6/6 PASS（本轮代码改动后重跑）。

### R17 §5 复验

- 文档六门：anchor 109/109、style 0 error、ref 通过、承载门通过、code-map 46/46、结构/链接门 0 问题。
- trace 门：`eval-trace-check.py <run-dir> --panel-dir …/panel/round17` 待本轮台账收口后执行。
- 回读：修复段由主 agent 逐条复读（同 agent 局限已在 §8 标注；reader 条目以原文对读为准）。
