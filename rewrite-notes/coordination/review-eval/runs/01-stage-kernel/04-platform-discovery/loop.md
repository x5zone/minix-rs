# D04 循环轨迹 —— 04-platform-discovery

## §0 基线

- 工作流版本：V2（`rewrite-notes/coordination/review-eval/WORKFLOW-VERSIONS.md`）
- schema: V3.4 from R3（V3.5 制品链起约束的首轮；重开依据 D-070。R1/R2 保持各自历史标记，不追溯）
- 基线提交：`fc1fe0c71`
- 文档：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`
- 文档基线 sha256：`0cfaa66fdb9eb7b2916129b8d7c97abf6f4e82b912ebb2c9d9c823c25a02af67`
- 基线行数：2682
- 工作树：除 `prompt/resources/` 未跟踪外干净
- 语料条目：D04（`CORPUS.md` 第二节）；盲区重点 B-01、B-02、B-03、B-04、B-08、B-13
- 设计快照：`04-outline.v4.md`、`04-outline-review.v4.md`、`04-design.v2.md`；`design-coverage-check.sh 01-stage-kernel` 全 PASS
- 关联代码：51 个文件、39,253 行，`tools/doc-code-map.sh --check` 51/51 通过；清单见 `related-code.txt`
- 本篇形态：平台发现 / DTB-ACPI / 时钟-SMP 描述符 / 三架构；高风险，按 `review-panel.md` 第七节启用全角色
- 范围声明：本篇范围 = 文档主题（内核如何把固件描述变成可用硬件描述）+ `related-code.txt` 的 51 个文件 + 本轮修复触碰的文件。不在范围 = 相邻模块、未列入清单的文件、下一批次的结构性重构。

## §1 面板

- P-FACT：文档全文、51 个关联 Rust 文件、25 个 C 源、trait 读数、冻结决策编号；未读历史 scan 与其他面板输出。输出 `panel/fact.md`，17 条发现。
- P-DESIGN D1：C 语义、文档 Ch1/Ch2、项目约束；任务书禁止现有 Rust、Ch3/Ch4、`.design/`、`reader-profile.md`。**读取披露（DEGRADED）**：报告自述用 grep 定位章节时看到了 Ch3 的小节标题，并读到文档前言列出的 Rust 文件路径清单；未读 Ch3/Ch4 正文或 Rust 实现。按 B-16 记 `DEGRADED`，D1 的 21 条 DECISION 只作参考。
- P-DESIGN D2：D1 报告、文档 Ch3/Ch4、51 个 Rust 文件、trait 读数；未读作者解释、历史评审、其他面板输出。输出 `panel/design-d2.md`，21 条发现；本轮 D2 在 D1 落盘后启动，做了 D1→D2 对照。
- P-READER A/B/C：只读文档全文与 `reader-profile.md`。输出 `panel/reader-a.md`（16）、`reader-b.md`（12）、`reader-c.md`（22）。
- P-REFUTE：文档、C 源、关联 Rust、设计快照；未读其他面板输出。输出 `panel/refute.md`，6 条主张：3 条 P1（§1.4 例外清单、51→53、4→6）、4 条 P2、其余反证失败。
- 无发现面板：`design-d1.md`（独立设计提案，21 条 DECISION，只作参考）；`refute.md`（6 条 CLAIM，反证结果见 `defect-ledger.md` 的 `## P-REFUTE 附加` 表）。
- 缩编：无。工作树完整性：面板前后 `git status --porcelain` 只有 `prompt/resources/` 与新增 `runs/`。

## §2 发现

- 面板原始输出：`tmp/review-eval/01-stage-kernel/04-platform-discovery/panel/`。
- 总计 88 条 `## FINDING`：FACT 17、DESIGN-D2 21、READER-A 16、READER-B 12、READER-C 22；P-REFUTE 另有 6 条主张。
- 最高严重度：
  - P1：FACT-01（`platform_sources` 51→53、分布漏两文件）、FACT-02（`uefi_helpers` 4→6）、FACT-03（`getinfo_machine` 逐字引用失实）、FACT-04（Phase A.5/B.5 归到 `arch_boot_impl`）、FACT-07（ACPI 越界反事实）、FACT-08（§6.1 归组句）、FACT-10（“只搬地址”漏 `/memory` 例外）、DESIGN-01（`Frozen::freeze` 二次冻结 UB）、DESIGN-02（`CpuTopology.nr_cpus` 无不变式）、DESIGN-03（`CpuInfo` 架构专用字段双真源）。
- 完整清单与逐条处置见 `defect-ledger.md`；未修项见 `follow-ups.md`。

## §3 归因

- 处置分布：① 15 条、② 73 条、③ 0 条、④ 0 条、⑤ 0 条。
- 冻结决策检查：0 条 `re-litigation`；D-001 四章骨架没有被重新提出。
- 范围门：0 条 adjacent 被当阻塞；所有 ② 项只进 `follow-ups.md`。
- 误报为 0 不等于精度完美；reader A/B/C 的转出项可能含少量措辞偏好。

## §4 修复

### 文档

- FACT-01：§5.3/§6.2 的 `51`→`53`；分布表补 `vm_handoff.rs` 与 `ebs_pair.rs` 两行；§6.2 #4 输出改为 `head -3` + “其余见 §5.3”。
- FACT-02：`uefi_helpers.rs` 的 4 条→6 条（1 条 `KernelInfo` + 5 条 bootstrap memmap）。
- FACT-03：§4.9 `getinfo_machine` 块改为“节选（构造已抽到 `machine_record`）”。
- FACT-04：§6.1/§7 的 Phase A.5/B.5 归属改为 `kmain`（`arch_boot_impl` 只负责 Step 0–4）。
- FACT-05：§1.6 改为“C 缺失 XSDT（Rust 已补）；扩展校验和两边都不校验”。
- FACT-06：§4.5 Safety 引文的 `27-28` 恢复为 `fn arch_boot_impl`。
- FACT-07：§2.2 反事实改为“越界从 `i=35` 开始；`acpi_phys_copy` 在 `!vm_running` 时返回 0，ERROR 不可达”。
- FACT-08：§6.1 归组句改为行 2 靠调用栈、行 3 靠 panic 文本与构建档位。
- FACT-09：§3.4 改为“`init_timer` 等 `&mut self` 方法只操作寄存器，不是唯一可变入口”。
- FACT-10：§1.3 加“就平台发现而言只搬地址；`/memory` 解析是 §1.4 例外”。
- FACT-12：`memory.c:815`→`819`。
- FACT-13：§2.4 改为 `acpi_get_ioapic_next` 无循环、取一条记录即返回。
- FACT-14：§4.8 改为“Step 2b 的一处编译期断言（Step 1 用运行期 `assert_eq!`）”。
- FACT-15：§4.10 改为“八个方法（六个必需 + 两个可选能力）”。
- FACT-16：§7 术语注改为“kind 常量叫 `DTB`；`FDT` 只作格式名”。
- REFUTE P1：§1.4 例外清单补 aarch64 UEFI 腿的 `dtb_copy::preflight`/`authorize_and_copy`/`cross_validate_config_dtb`（EBS 前交叉校验，5 条 panic 停机路径）。

### 代码

- 本轮未改代码；21 条设计发现全部转出（见 `follow-ups.md`），D04 标 design-debt。

### 未修

- FACT-11/17、DESIGN-01..21、READER-A/B/C 的 73 条转出项：见 `follow-ups.md`。

## §5 复验

- `tools/anchor-resolve.sh --check`：anchors=73 resolved=73 zero-def=0 multi-def=0，exit 0。
- `tools/doc-code-map.sh --check`：53/53 存在，exit 0。
- `tools/doc-ref-check.sh --check`：内部章节引用均可解析，exit 0（FACT-07 的“第 36 次迭代”曾误触章节引用门，已改为 `i=35`）。
- `tools/doc-code-density-check.sh --check`：4 个可判章均有码/命令，阻断项通过，exit 0。
- `tools/doc-style-lint.sh --diff`：1 个文件 / 52 个新增行，error=0，exit 0。
- `cargo check -p minix-kernel --features mock`：待跑。
- `tools/eval-trace-check.py`：88 条发现全部有处置行、无孤儿行、metrics 自洽，exit 0。
- QEMU 真机回归：未跑；DESIGN-01/02/03 与 §1.4 的 aarch64 DTB 交叉校验都需要 QEMU 或专门载体。

## §6 指标

- 见 `metrics.md`：发现 88、① 15、② 73、③ 0、④ 0、⑤ 0、`re-litigation` 0、`adjacent 阻塞` 0、重构提出 21/接受 0/实施 0/登记 21。
- 成本：7 个面板 agent，面板约 90 分钟；修复约 1 小时；token 未采集。

## §7 状态

**NOT CONVERGED（design-debt + teaching-debt + test-debt pending）**

- 设计债：DESIGN-01..21 共 21 条转出；其中 DESIGN-01/02/03 是 P1，需优先排队。
- 教学债：reader A/B/C 的 50 条转出项。
- 测试债：`cargo test -p minix-arch` 测试模块编译失败（D04-F16，沿用 D01-F01）。
- 已修 15 条；未达到 `CONVERGED` 的原因不是“还有小问题”，而是上述三类债都有明确去处且尚未关闭。
- meta-review 总体 `FLAT`，运行质量 `NOT PASS`（抽样 33/88 = 37.5%，严格一致性 69.7%）；15 条 ① 只有 10 条干净，5 条部分成立已按复核补修；B-23（P-REFUTE 映射错位）与 B-24（V2 规则未落地）入账。

## §8 给 meta-review 的不确定点

1. D1 的 `DEGRADED`（看到 Ch3 小节标题与 Rust 文件路径清单）是否足以作废其 21 条 DECISION；D2 已做 D1→D2 对照，但只覆盖部分决策。
2. DESIGN-01/02/03 是 P1 代码安全问题，却被转出；本轮按“代码改动不属于文档轮”处理，请判是否应改为本轮修。
3. 73 条转出项中，reader A/B/C 的 50 条有多少是真正的教学债、多少是文风偏好；本轮未逐条判 ③。
4. §1.4 的 aarch64 UEFI DTB 交叉校验补写是否足够；它依赖 `dtb_copy` 的三阶段两快照纪律，未跑 QEMU。
5. 文档 §5.1 的 26→18 测试计数与 `cargo test --list` 的关系；`minix-arch` 测试模块当前不编译，静态计数不等于能跑。

## R1 完整 survey（V3.3）

### R1 §0 基线

- 轮型: full
- edition: V3.3
- schema: V3.3 from R1
- 基线提交：`ce6288154`（D03 FREEZE 之后）
- 文档：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`
- 文档 sha256：`a8cb91b11e94a2c42daffe1b6cad558850025fcba9c0286e3ee51ab1f8e5e5b5`
- 行数：2688
- 设计快照：04 组齐备；`design-coverage-check.sh 01-stage-kernel` ALL DOCS COMPLETE
- 关联代码：`tools/doc-code-map.sh --check` 52/52 通过；清单见 `related-code.txt`
- L0 预修：删除指向不存在文件 `os/libs/minix-boot/src/boot_form.rs` 的表格行，并把 §5.1 的“53 处”改为 52；该修改记入 R1 台账。
- 范围声明：本篇范围 = 文档主题（平台发现：ACPI/DTB/多架构） + 52 个关联文件 + 本轮修复触碰的文件。不在范围 = 相邻模块、未列入清单的文件。
- 历史说明：本文件顶部的旧 §0–§8 是 V1 记录，保留为历史段、不追溯改账。

### R1 §1 面板

- 3 角色：fact（C+Rust）、design-d2、reader-abc（A/B/C 合并，VARIANCE 已记）。
- fact 8 条（P0 3 / P1 2 / P2 3）；design-d2 12 条（P1 2 / P2 6 / P3 4，ARCH 提案 1）；reader-abc 33 条（24 FINDING + 9 SUGG）。
- 材料隔离：读者未读源码；设计未读作者解释；事实未读历史评审。
- 无发现面板：`reader-fix-log.md`（修复代理的编辑日志，不是面板报告）

### R1 §2 发现

- 53 条：P0 = 3（`find_platform_sources` 不存在、`PlatformDescSource` 两字段/Box::leak 旧形态、uefi 测试计数 6 vs 12）；P1 = 4；P2 = 33；P3 = 13。
- 关键：平台描述符已改为 `kind + len + phys_addr` 的 BootStorage 槽位；`find_platform_sources_into` 写调用者槽位并返回条数；`&[]` 计数 54。

### R1 §3 归因

- 处置：① 34 / ② 6 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 13 = 53；唯一根因 53。
- ② 六条：design P2 代码/架构项，登记 `D04-design-batch`（D-054）。
- 冻结决策：0 条 re-litigation；P3 13 条进 suggestions。

### R1 §4 修复

- 文档：`find_platform_sources_into` 全量改名；§3.1/§3.6/§4.2 平台描述符三字段与槽位；§4.2 节选改 `with_optional_len`/`out[..].copy_from_slice`；riscv 改 `push_platform_source`；uefi 测试 12；54 处计数；C-01 反事实与 C-02 计数；读者 24 条按 `reader-fix-log.md` 逐条修。
- 代码：无行为改动；design P2 六项登记设计批。

### R1 §5 复验

- 文档门：anchor 72/72；doc-code-map 52/52；structure/ref/style 通过；citation 86 块全跳过（MANUAL_FALLBACK，见 §8）。
- 旧串归零：`find_platform_sources`（无 `_into`）、`Box::leak`、`上述 52 处`、`切片内容`、`ncpus` 停在已计入 均为 0（按替换点核）。
- 测试读数：uefi_helpers 12 条、minix-platform 26 条与文档一致；QEMU 未跑。

### R1 §6 指标

- 见 `metrics.md` R1 块：发现 53、唯一根因 53、① 34 / ② 6 / SUGG 13、未闭环 0、ROUND-CLOSED、代裁决 1、ARCH 提案 1。

### R1 §7 状态

**ROUND-CLOSED**（修改轮出口；进入 CONVERGENCE-CANDIDATE）。设计代码项按 §4.0f 登记，不阻断；不宣告 CONVERGED。

### R1 §8 给 meta-review 的不确定点

1. citation 门对本文 86 个 `/* */` 块全部跳过；按 FACT-05 记 `MANUAL_FALLBACK`，引文由 fact 人工逐块核对。
2. 读者 33 条中 9 条是低优先 SUGG；24 条 FINDING 由修复代理按最小修复落地，主 agent 只复核 diff 与门。
3. design 6 条 P2 需改代码，延期到 D04-design-batch；是否削弱 ROUND-CLOSED 请 meta 判断。
4. QEMU 未跑。

### L6

- 同 agent 收口：53 条全部处置；P0 3 条全修；ROUND-CLOSED。
- BLIND-SPOT-DELTA：平台描述符形态（两字段→三字段+槽位）是本轮最大漂移；无新结构性盲区。

### L6.5

- 三问：① 本轮不变量是“平台来源必须有长度、写入所有者槽位、无 Box::leak”；② 前提是旧 V1 run 只作历史；③ 证据是 53 行台账与门读数。
- 资源重读：本轮盲区是“文档停在旧 boot-shim 形态”，检测模式是 `doc-code-map` + fact 的符号存在性核对；无需新增规则。

## R2 确认轮（V3.3 四态模型）

### R2 §0 基线

- 轮型: confirmation
- edition: V3.3
- schema: V3.3 from R1
- 基线提交：`58c8bdcdc`
- 文档 sha256：`773c10f1bf69de6f3604a690bc97850282b7077705ac760d8b493087d56675e3`
- 行数：2708
- 轮次语义：确认轮；只修新准入 P0/P1/P2 事实/同步；不实施 ⑥；certify 0 才 CONVERGED。

### R2 §1 面板

- 3 角色：fact、design-d2、reader-abc（合并，VARIANCE 已记）。
- fact 10 条（P2 3 / P3 7）；design-d2 16 条（P2 6 / P3 10，ARCH 1）；reader-abc 14 条（P2/P3）。
- 材料隔离：读者未读源码；设计未读作者解释；事实未读历史评审。

### R2 §2 发现

- 40 条：P0 = 0；P1 = 0；P2 = 18；P3 = 22。
- 关键：§4.5 “低 4GB 恒等映射”前提过宽、§3.4 “实例字段构造后不变”被 riscv `set_plic_va_base` 推翻、§3.1 KernelInfo 注释节选改写；design 6 条代码项与 reader 9 条 P2。

### R2 §3 归因

- 处置：① 15 / ② 6 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 19 = 40；唯一根因 40。
- ② 六条 design 代码/ARCH 项登记 `D04-design-batch`（D-054）。
- 0 条 re-litigation；P3 19 条进 suggestions。

### R2 §4 修复

- 文档：§4.5 恒等窗口改最小窗口 + owned copy；§3.4 freeze 前例外；§3.1 KernelInfo 注释恢复 `len=0` 回退 2 MiB；reader 18 条按 `reader-fix-log.md` 修（A-01/A-02/A-04/A-07/B-01..B-04 等）。
- 代码：无行为改动；design 6 项登记设计批。

### R2 §5 复验

- 文档门：anchor 77/77；doc-code-map 52/52；structure/ref/style 通过；citation 全部跳过（MANUAL_FALLBACK）。
- 旧串归零：`低 4GB 铺了 VA == PA`、`构造后不再被`、`len = 0 means unknown/legacy` 均为 0（按替换点核）。
- certify 已跑（R2 收口时完成 3 条修复，同 agent grep 复核，见 FREEZE 的收敛证据节）；QEMU 未跑。

### R2 §6 指标

- 见 `metrics.md` R2 块：发现 40、唯一根因 40、① 15 / ② 6 / SUGG 19、未闭环 0、CONVERGED、代裁决 1、ARCH 提案 1。

### R2 §7 状态

**CONVERGED**（确认轮出口）。G1–G9：in-scope P0/P1/P2 = 0；⑤ = 0；② 六条有 D-054 批准；机械门全绿；台账 40 行全部有处置与复验；`certify 新准入 = 0`。设计代码项进 `D04-design-batch`，不阻断。

### R2 §8 给 meta-review 的不确定点

1. citation 门 86 块全跳过（MANUAL_FALLBACK）；引文由 fact 人工核对。
2. design 6 条 P2 需改代码，延期到 D04-design-batch。
3. reader 14 条 FINDING 由修复代理按最小修复落地，主 agent 复核门与 diff。
4. QEMU 未跑。

### L6

- 同 agent 收口：40 条全部处置；P0/P1 = 0；CONVERGED。
- BLIND-SPOT-DELTA：R1 后仍有“恒等窗口前提/实例不可变”两处语义漂移；无新结构性盲区。

### L6.5

- 三问：① 不变量是“平台 blob 必须在已注册恒等窗口内、实例字段 freeze 后不可变”；② 前提是 R1 ROUND-CLOSED；③ 证据是 40 行台账与门读数。
- 资源重读：本轮盲区属“旧启动前提残留”，检测模式是确认轮的跨节前提核对；无需新增规则。

## R3 确认轮（V3.4 四态模型；D-070 授权的重开）

### R3 §0 基线

- 轮型: confirmation
- edition: V3.4
- schema: V3.4 from R3
- 基线提交：`a16191022`（D03 收尾提交；本轮修复未提交时的工作树）
- 文档：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`
- 文档基线 sha256：`ff8aad7ea337acd52faedbfe9fda268814d2e632e794b11b4faea44f040b22f5`（R2 FREEZE 值，与面板启动时一致）
- 行数：2735（修复后 2737）
- 设计快照：04 组齐备；`design-coverage-check.sh 01-stage-kernel` ALL DOCS COMPLETE
- 关联代码：`doc-code-map --check` 52/52（修复后 54/54——两处裸锚点补全路径后计数入册）
- 语料条目：D04（`CORPUS.md` 第二节）
- 范围声明：平台发现（DTB/ACPI 双源、描述符分类、GIC、CPU 拓扑）+ 52 个关联文件 + 本轮修复触碰的文件；不在范围 = 相邻模块与下一批次结构性重构
- 重开依据：D-070

### R3 §1 面板

- 六角色全量、独立子 agent；读者 A/B/C 独立（文档 2735 行 ≥ 500）：
  - P-FACT：6 条 FINDING + 3 SUGGESTION（`fact.md`）——实测 26 条测试计数与逐文件分布精确吻合、13 个具名测试全存在、C 侧 20 余函数逐字核对通过
  - P-DESIGN-D2：9 条 FINDING + 1 ARCH-PROPOSAL（`design-d2.md`）；D1 停跑（D-024），基线「文档主张 ↔ 源码事实」，记 VARIANCE
  - P-READER-A：1 + 10（`reader-a.md`）；P-READER-B：7 + 12（`reader-b.md`）；P-READER-C：4 + 12（`reader-c.md`）
  - P-REFUTE：6 条 CLAIM（阻断 0 / 非阻断 1 / 反证失败 5），`refute.md`
- **VARIANCE（派发降档）**：平台并发上限 3，六角色按 3+3 两波派发（第一波 fact/reader-a/design-d2，第二波 reader-b/reader-c/refute）；按 L1.1 降档协议记档。
- 制品入库：`panel/round3/` + `INDEX.md`（6 文件：FINDING 27 / SUGGESTION 37 / CLAIM 6）。
- certify 回执：`certify/round3-certify.md`（12/12 CLOSED，首轮即 PASS）。

### R3 §2 发现

- 70 条机器可计。严重度：P0 = 0；P1 = 1（READER-C-01：§1.4 的 Rust 失败点位与 §4.1/§6.1 矛盾）；P2 = 15；P3 = 54。
- 面板要点：fact 的对账质量很高（26 条测试计数逐文件精确吻合）；设计侧的画像仍是「统一抽象的隐性税」+ 一条真问题（`enable_sched_sgi_for_cpu` 的 BSP 身份分叉，一行可修）；读者侧抓到 1 条 P1（概述与实现给出互斥的失败点位）。

### R3 §3 归因

- 29 个根因（RD-R3-01..29）。
- 冻结决策检查：无命中；3 条设计 P2 记 ② 延期（D-070）；ARCH 提案 RD04-A1 进判决账待裁；CLAIM 11..15 反证失败记 FP、CLAIM-16 非阻断已修记 ①。
- 范围：全部 in-scope。

### R3 §4 修复

- 12 处文档修改点：§1.4 Rust 失败点位（init_from_kinfo/init_protection 先于时钟初始化）、C 侧约束放大口径、§1.3 DTB 单形态限定、§3.7 双措辞区分、§4.2 authority 过滤机制句、§2.9 板号两步失败停点、§5.4 M1 与 §5.3 口径、§5.2 riscv64 topo 行构造器、§4.2 cross_validate/push_dtb_sources 锚点组、三处引文标注（句序重排/中文转述/中文转述）、§3.2 enum 理由原话与推论区分、两个裸锚点补全路径、§5.2 表下补三变体构造器差异说明。
- 文档六门：anchor 77/77、citation `checked=68 failed=0 warned=7`、structure/ref/style(error=0)/density 通过、doc-code-map 54/54。

### R3 §5 复验

- 独立回执 `certify/round3-certify.md`：12 条全部 CLOSED，首轮即 PASS；每条附源码 grep/sed 读数（含 `cell.rs:246/250` 双措辞、`memmap_source.rs:354-366`、`main.rs:110`、`uefi_helpers.rs:855-895` 等）。
- `certify 新准入 = 0` 口径：certify 输入闭集 = 12 个修改点；回执覆盖全部。
- 代码行为：本轮无代码改动；QEMU 未跑。

### R3 §6 指标

- 与 `metrics.md` 的 `## R3 确认轮` 块一致：发现总数 70、唯一根因数 29、本轮新准入 29、certify 新准入 0、处置 ① 10 / ② 3 / ④ 7 / FP 5 / SUGG 45、未闭环 0。
- 轮型 `confirmation`；轮结果 `CONVERGED`。

### R3 §7 状态

- `CONVERGED`。前提：R1 是已关闭的修改轮，R3 certify 新准入 0。
- 未闭环 0；`OPEN-DEBT` 的 `R3-design-batch` 有 D-070 判决行与日期。
- `CONVERGED` 的语义限制同 D01–D03。

### R3 §8 给 meta-review 的不确定点

1. 3 条设计 P2 里 D2-03（BSP 身份分叉）读起来像真缺陷（SGI 门可能开错 redistributor），确认轮只能延期；若 meta 认为应按事实缺陷在轮内修，本轮不能以确认轮关闭。
2. 设计 P3 里 D2-08/09 被面板标注「需用户裁决」，但按 §4.0c 门槛它们是 P3 → SUGG；「需用户裁决」与「不阻断登记」的界线没有明文。
3. refute 的 CLAIM-16（非阻断 P2）的精度瑕疵（4 条停机路径、push_dtb_sources 归属）由主 agent 按源码改正，certify 未单列复核——依赖 certify 第 9 条的覆盖。

### L6

- 同 agent 收口（另有独立 certify 回执，见 §5）。流程质量记录：fact 对账密度高（26 条测试计数逐文件吻合）而读者侧只报 1 条 P1，说明 D04 在 R2 后的文档质量同样已经很高；设计侧继续给「隐性税」画像 + 一条一行可修的真分叉。
- 流程质量结论：`IMPROVED`（ certify 首轮 12/12，是四篇中唯一一次首轮全过；说明前两篇的修改模式已被吸收）。

### L6.5

- 三问：① 不变量是「确认轮在最终工作树上 certify 新准入 = 0」；② 前提是 R1 已 ROUND-CLOSED；③ 证据是 certify 回执 12/0 与六门读数。
- 资源重读：盲区属「流程类」，查 `fresh-eyes-review`；无新检测模式，记 BLIND-SPOTS 候选。
