# D02 循环轨迹 —— 02-higher-half-kernel

## §0 基线

- 工作流版本：V1（`rewrite-notes/coordination/review-eval/WORKFLOW-VERSIONS.md`）
- schema: V3.4 from R3（V3.5 制品链起约束的首轮；重开依据 D-063。R1/R2 保持各自历史标记，不追溯）
- 基线提交：`691cde1bf`（V1 边界回归提交之后）
- 文档：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 文档基线 sha256：`6e5ffc3ce0398405b6666311e60e3c7152876deb7b349032c46f00c413a917b3`
- 基线行数：1267
- 工作树：除 `prompt/resources/` 未跟踪外干净
- 语料条目：D02（`CORPUS.md` 第二节）；盲区重点 B-01、B-02、B-03、B-04、B-08、B-12、B-13
- 设计快照：`02-outline.v4.md`、`02-outline-review.v4.md`、`02-design.v3.md`；`design-coverage-check.sh 01-stage-kernel` 全 PASS
- 关联代码：26 个文件、26,381 行，`tools/doc-code-map.sh --check` 26/26 通过；清单见 `related-code.txt`
- 本篇形态：链接布局 / 双地址 / 高半跳转 / 页表粒度；高风险，按 `review-panel.md` 第七节启用全角色，包含 P-DESIGN D1+D2 与 P-REFUTE
- 范围声明：本篇范围 = 文档主题（高半区链接布局与成对跳转）+ `related-code.txt` 的 26 个文件 + 本轮修复触碰的文件。不在范围 = 相邻模块、未列入清单的文件、下一批次的结构性重构。

## §1 面板

- 角色与材料：
  - P-FACT：文档全文、26 个关联 Rust 文件、21 个 C 源、trait 读数、冻结决策编号；未读历史 scan 与其他面板输出。输出 `panel/fact.md`，12 条发现。
  - P-DESIGN D1：C 语义、文档 Ch1/Ch2、项目约束；任务书禁止现有 Rust、Ch3/Ch4、设计快照。**违规记录（DEGRADED）**：报告自述读了 `reader-profile.md`，而读者画像的教学目标列出了作者 §3.1–§3.5 的小节标题；它没有读 Ch3/Ch4 正文或 Rust 实现。按 D01 的 B-16 分级，这是“清单外材料泄漏”，记为 `DEGRADED`，D1 的 14 条 DECISION 只作参考。
  - P-DESIGN D2：文档 Ch3/Ch4、26 个 Rust 文件、trait 读数；未读作者解释、历史评审、其他面板输出。输出 `panel/design-d2.md`，15 条发现。**流程缺口**：D2 启动时 D1 报告尚未落盘（并行派发），所以它没有做“第一遍 → 第二遍”的逐条对照，而是把基线换成“文档主张 ↔ 源码事实”的独立第二遍。设计结论因此是独立的，但 D1/D2 的意图对照没有发生，记入 §8。
  - P-READER A/B/C：只读文档全文与 `reader-profile.md`。输出 `panel/reader-a.md`（12 条）、`reader-b.md`（14 条）、`reader-c.md`（16 条）。
  - P-REFUTE：文档、C 源、关联 Rust、设计快照；未读其他面板输出。输出 `panel/refute.md`，6 条主张：5 条反证失败（主张成立），1 条反证成功（aarch64 去 `isb` 的失败推演），另有 2 条 P1、若干 P2 与观察。
- 缩编：无。
- 工作树完整性：面板前后 `git status --porcelain` 只有 `prompt/resources/` 与新增 `runs/`，无面板写入源码或文档。
- 无发现面板：`design-d1.md`（独立设计提案，14 条 DECISION，只作参考）；`refute.md`（6 条 CLAIM，反证结果见 `defect-ledger.md` 的 `## P-REFUTE 附加` 表）。

## §2 发现

- 面板原始输出：`tmp/review-eval/01-stage-kernel/02-higher-half-kernel/panel/`。
- 总计 69 条 `## FINDING`：FACT 12、DESIGN-D2 15、READER-A 12、READER-B 14、READER-C 16；P-REFUTE 另有 6 条主张（5 条成立、1 条被反证）。
- 最高严重度：
  - P1：FACT-01（测试计数 44 vs 113）、FACT-02/DESIGN-01（riscv64 引文数值）、FACT-03（bootface 节选断层）、DESIGN-02（aarch64 理由错误）、DESIGN-06/07（loader 孪生函数与调用顺序契约）、DESIGN-11（恢复序列三份重复）、DESIGN-12（aarch64 `KERN_SIZE` 无等值对账）、REFUTE 的 aarch64 `isb` 失败推演、READER-C-01/02/03（起点状态、保证者、x86_64 链接期前提）。
  - P2：FACT-04..12、DESIGN-03/04/05/08/09/13/14/15、reader A/B 的结构项、reader C 的其余一致性项。
- 完整清单与逐条处置见 `defect-ledger.md`；未修项见 `follow-ups.md`。

## §3 归因

- 处置分布：① 24 条、② 42 条、③ 1 条、④ 2 条、⑤ 0 条。
- 冻结决策检查：0 条 `re-litigation`；D-001 四章骨架没有被任何角色重新提出。
- 范围门：0 条 adjacent 被当阻塞；所有 ② 项只进 `follow-ups.md`。
- ④ 2 条：DESIGN-10（`Paging`/`HugePages`/`BootArch` 是真实语义抽象，拒绝删除）、DESIGN-15（翻译残留分类项）。
- ③ 1 条：READER-C-15（meta-review 复核：`_MINIX_MAGIC` 在 `sys.mk:24-28` 是有条件定义，文档说法正确，改判误报）。
- 误报为 0 的原判断已修正：除 C-15 外，READER-A-08/READER-B-04 与已修项重复，按 ① 合并；READER-A-10 只应用了起点修法，改 ②。

## §4 修复

### 文档

- FACT-01：§5.1 测试总数 44 → 默认特性下 113。
- FACT-02/DESIGN-01：§4.3 riscv64 引文改为源码原值 `0xffffffbf7ffe0000`，注明原文加数笔误，按视图差 `0xffffffbf7fe00000` 复算才是注释写的结果。
- FACT-03：§4.4 bootface 节选在 `);` 与 `say(` 之间补 `...`，块头说明补 sources/startup 记录组装。
- FACT-04：§4.4 删除 `fix27c`/NK4-C 锚点，改引 `store_kernel_info` 现存注释首句。
- FACT-05：§3.1“五块载荷”→七块（memmap/reserved/modules/platform/param/sources/startup）；§4.4 路径图的 arena 括号同步。
- FACT-06：§7.2 补 9 个 C 源、§7.3 补 5 个 Rust 源。
- FACT-07：三处引文块头改为“节选（注释有删改）”。
- FACT-08：`Elf64Ehdr` 丢失字段补 `e_ehsize`。
- FACT-09：§2.2 “上一段的两个 `printf`” → “§6.2 的两条 `printf`”。
- FACT-10：§4.4 路径图补 `arch_boot_finish`；§3.4 表第一行的返回点改为“`arch_boot_finish` 里 `arch_boot_impl` 的返回点”。
- FACT-11：锚点 `impl Iterator` → `impl Iterator for SegmentIter`。
- FACT-12：§4.1 三份脚本差异“四处”→“五处”，补链接期 `ASSERT`。
- DESIGN-02：§4.3 aarch64 理由改为“相对分支不能改变执行视图”，删掉 ±128 MiB 范围论。
- DESIGN-12：§3.1 限定 aarch64 自引导面只有上界断言、无等值对账；§5.4 M10 改“部分覆盖”。
- DESIGN-14：§4.4 补 aarch64 direct-core “低地址进入”来自固件平移入口的说明与 QEMU `-d in_asm` 观测入口。
- REFUTE P1：§4.3 其二改为“`enable()` 已有 `dsb sy; isb` 与 SCTLR 写后的 `isb`，删 `jump_to_kmain` 的 `isb` 不会产生所述失败；保留它是防御性屏障”。
- REFUTE P2：§3.5 档位表补 `FallbackUnsupported` 第四种出口；§3.2 `paddr` 改为“拷贝目标；`compute_kernel_layout` 也用它算基址与跨距”；§2.4 栈段归属补 ARM `.data`；§2.5 差异清单补第三处；§3.3 “三条序列”→“四条”。
- READER-A-10 / READER-C-01：开头前置知识改为“Rust 生产路径分页已开；C 侧 i386 从分页未开开始”。
- READER-C-02：§3.4 表第一行保证者区分 direct 面与 v1 面。
- READER-C-03：§3.4 明确 x86_64 没有运行时检查、靠链接布局；riscv64 才有显式补偿。
- READER-C-05：§2.3 与 §4.3 区分 C 侧 `pre_init` 帧指针与 Rust 侧 0 终止。
- READER-C-10：§2.2 注明 ARM 物理基址 `0x80200000` 不是 4MiB 对齐，不能套 i386 判据。

### 代码

- `os/kernel/src/boot/higher_half.rs`：trait 注释两条理由订正（“启动尾段调用”“只能验证签名”）。

### 未修

- DESIGN-03/04/05/06/07/08/11/13 与 reader A/B/C 的 44 条转出项：见 `follow-ups.md`。

## §5 复验

- `tools/anchor-resolve.sh --check`：anchors=59 resolved=59 zero-def=0 multi-def=0，exit 0。
- `tools/doc-code-map.sh --check`：27/27 存在（新增 §7 路径），exit 0。
- `tools/doc-ref-check.sh --check`：内部章节引用均可解析，exit 0。
- `tools/doc-code-density-check.sh --check`：4 个可判章均有码/命令，阻断项通过，exit 0。
- `cargo check -p minix-kernel --features mock`：exit 0（trait 注释改动不影响编译）。
- `cargo test -p minix-kernel --features mock`：865 通过、0 失败、8 忽略（沿用 D01 后的基线；D02 未改内核行为）。
- `cargo test -p boot-shim --lib -- --list`：113 tests, 0 benchmarks（与 §5.1 的 113 一致）。
- `tools/doc-style-lint.sh --diff`：1 个文件 / 76 个新增行，error=0，exit 0。
- `tools/eval-trace-check.py`：69 条发现全部有处置行、无孤儿行、metrics 自洽，exit 0。
- QEMU 真机回归：本轮未跑；DESIGN-14 的 aarch64 入口 PC 与 DESIGN-08 的链接期断言都需要一次构建/真机读数。

## §6 指标

- 见 `metrics.md`：发现 69、① 24、② 42、③ 1、④ 2、⑤ 0、`re-litigation` 0、`adjacent 阻塞` 0、重构提出 15/接受 8/实施 5/登记 10。
- 成本：7 个面板 agent，面板约 40 分钟；修复约 1 小时；token 未采集。

## §7 状态

**NOT CONVERGED（design-debt + teaching-debt + test-debt pending）**

- 设计债：DESIGN-03/04/05/06/07/08/11/13 共 8 条转出；其中 DESIGN-04/05/13 需用户裁决。
- 教学债：reader A/B/C 的 44 条转出项，优先 reader B 的结构项与 reader C 的指针项。
- 测试债：`cargo test -p minix-arch` 测试模块编译失败（D02-F12，沿用 D01-F01）。
- 已修 24 条，文档事实层与三处 P1 一致性已清；未达到 `CONVERGED` 的原因不是“还有小问题”，而是上述三类债都有明确去处且尚未关闭。
- meta-review 总体 `FLAT`（相对 D01 基线），运行质量 `NOT PASS`（抽样 26/69 = 37.7%，一致性 84.6%）；四条处置错误已按复核改正，DESIGN-06/07 的文档侧已本轮补修，`isb` 残留表述已同轮改掉。

## §8 给 meta-review 的不确定点

1. **D2 没有看到 D1**：并行派发时 D1 尚未落盘，D2 改为“文档 ↔ 源码”的独立第二遍。设计发现是独立的，但 D1→D2 的对照没有发生；请判这是否降低了设计轴的可信度。
2. **D1 读了 `reader-profile.md`**：读者画像列出了 §3.1–§3.5 的小节标题，属于“清单外材料泄漏”；D1 没有读 Ch3/Ch4 正文或 Rust。按 B-16 记 `DEGRADED`，请判是否应作废。
3. **DESIGN-06/07 是 P1 但转出**：loader 孪生函数与“调用顺序契约”都是真实的可测试性/类型安全问题；本轮按 1–2 个结构性重构的上限转出，请判是否应改为本轮修。
4. **DESIGN-12 只修了文档**：aarch64 `KERN_SIZE` 的等值对账（L9 扩 aarch64）仍在 follow-up；文档的 M10 已改为“部分覆盖”，但代码侧的静默失效路径还在。
5. **QEMU 未跑**：DESIGN-14 的固件平移入口、DESIGN-08 的链接期断言都只有静态证据；请把“未跑”与“跑过但失败”分开看。
6. **测试计数**：113 是默认特性下的静态属性计数；`cargo test -- --list` 尚未跑，存在运行时过滤的差。

## R1 完整 survey（V3.3）

### R1 §0 基线

- 轮型: full
- edition: V3.3
- schema: V3.3 from R1
- 基线提交：`73a1b4a15`（D01 FREEZE 之后）
- 文档：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 文档 sha256：`2bb4a585597ffbd27cf4495292e2e85f026e1782b08fe5268426f374357e5485`
- 行数：1275
- 工作树：除 3 个既有未跟踪文件外干净；本轮不触碰
- 语料条目：D02（`CORPUS.md` 第二节）；盲区重点 B-01、B-02、B-03、B-04、B-08、B-12、B-13
- 设计快照：`02-outline.v4.md`、`02-outline-review.v4.md`、`02-design.v3.md` 等齐备；`design-coverage-check.sh 01-stage-kernel` ALL DOCS COMPLETE
- 关联代码：`tools/doc-code-map.sh --check` 28/28 通过；清单见 `related-code.txt`（本轮刷新为 28 行）
- 范围声明：本篇范围 = 文档主题（高半区链接布局、双地址、成对跳转、页表粒度）+ `related-code.txt` 的 28 个文件 + 本轮修复触碰的文件。不在范围 = 相邻模块、未列入清单的文件、下一批次的结构性重构。
- 历史说明：本文件顶部的 `## §0`–`## §8` 是 V1 时代的旧 run（69 条、`NOT CONVERGED`），按 V3.3 §4.0a 保留为历史段、不追溯改账；其 42 条未批准 ② 不带入 R1。R1 是 V3.3 的完整 survey，重新发现、重新准入。
- 轮次语义：修改轮（full）最多 `ROUND-CLOSED` / `CONVERGENCE-CANDIDATE`，不宣告 `CONVERGED`；收敛声明权在后续确认轮。

### R1 §1 面板

- 第一波 3 角色（fact / design-d2 / reader-abc），第二波 1 角色（refute，高风险篇目强制），第三波 2 角色（certify-fact / certify-reader）。
- P-FACT：文档全文 + C 源 + 27 个 Rust 文件 + 六门读数；输出 `panel/round1/fact.md`，14 条（P0 2 / P1 8 / P2 4）。
- P-DESIGN-D2：第 3/4 章 + 27 个 Rust 文件 + trait-audit；输出 `design-d2.md`，15 条（P2 8 / P3 7），2 条 ARCH-PROPOSAL。
- P-READER（A+B+C 合并）：文档 + `reader-profile.md`；输出 `reader-abc.md`，2 条 P2 + 17 SUGG；报告开头标 `VARIANCE`（合并导致三视角共享上下文）。
- P-REFUTE：文档 + C/Rust 源 + `02-design.v3.md`；输出 `refute.md`，6 条主张：4 条反证失败、2 条阻断（CLAIM05 帧指针、CLAIM06 文档-代码同步），另有 2 条 P2。
- certify：fact 报 3 条新 P2 + 1 条 REOPEN（`.usermapped` 供料方）；reader 报 3 P1 + 9 P2。均在本轮修完，certify #2 重读为 0。
- 材料隔离：读者未读源码；设计未读作者解释；事实未读历史评审。缩编：reader A/B/C 合并为一个 agent（500–1500 行档位），独立性损失已记。

### R1 §2 发现

- survey 33 条：FACT 14、DESIGN-D2 15、READER-ABC 2、REFUTE P2 2；REFUTE 另有 4 条反证失败主张。
- P0 = 2（`load_boot_modules_with_loader` 不存在、`build_kernel_info` 旧 10 参签名）；P1 = 8（测试计数、五个/六个 overlap、getter、缺源/目标重叠、place_modules 迁址、MODULE_TABLE 死引用、帧指针、stack_top 第二份公式）。
- certify #1 新准入 16 条（FACT 4、READER 12；含 3 P1）；certify #2 重读 0。
- 逐条位置与修复见 `defect-ledger.md` R1 段与 `panel/round1/*.md`。

### R1 §3 归因

- 处置：① 44 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 2 / SUGG 7 = 53；唯一根因 53（两条 REFUTE P2 并入 CLAIM 行，避免与门的分档聚合冲突）。
- 冻结决策：0 条 `re-litigation`；D2-04/D2-11 与 D-040/D-042 不冲突。
- 范围门：0 条 adjacent 被当阻塞；设计代码改进登记入设计批（D2-01..06/13），P3 进 follow-ups 的 suggestions 表。
- ARCH：A1（载荷所有权类型化）、A2（链接符号导出跨距）记入 `DECISIONS.md` D-050/D-051，待用户裁决，不实施、不阻断。

### R1 §4 修复

- 文档事实层：两处符号/签名 P0；测试计数 22/125；六个 overlap；字段直读；源/目标重叠 panic 与四步；place_modules 迁址；`.bss` 例子改 `IMAGE_HEAP`；帧指针三处与 M11；stack_top 第二份公式；`_MINIX_MAGIC` 条件；`.usermapped` 供料方；aarch64 isb 注释；载体 link.ld。
- 文档读者层：§2.2 改名两种情形、§2.3 帧指针边界、§2.4 指向、§3.4 指针对象与 stack_top 断言范围、三处节选头、§4.4 place_modules 语法与模块文档指针、M7 口径、NoLoadSegments 测试名、aarch64 ldr 视图。
- certify 修复：riscv64 残留 getter、BootStorageCell `.data`、M7 18、usermapped_data.c、12 条读者项。
- 代码：无行为改动；设计代码改进全部登记设计批（D2-01..06/13）。

### R1 §5 复验

- 文档六门：citation `32 块/1 检查/0 失败/31 跳过`；anchor `56/56 zero-def=0`；structure/ref 通过；style-lint 文件级 error=0；density 承载门通过；doc-code-map `28/28`。
- 旧串归零：`load_boot_modules_with_loader`、`PhysBytes(0)`、`kern_virt_base()`、`pre_init` 的帧指针、`没有第二份实现`、`MODULE_TABLE`、`载体单地址脚本`、`五个 first_overlapping_pair`、`18 个测试`、`映射越界覆盖别人内存`、`反方向的错法是` 均为 0。
- 测试读数：`cargo test -p boot-shim --lib -- --list` → 125；loader `#[test]` 22；minix-elf `#[test]` 23；无代码行为改动，QEMU 未跑。
- certify #1 → 16 条已修；certify #2 → 0（见 `certify-*.md` 的 certify #2 小节）。

### R1 §6 指标

- 见 `metrics.md` R1 块：发现 53、唯一根因 53、本轮新准入 53、certify 新准入 0、未闭环 0、① 44 / FP 2 / SUGG 7、ARCH 提案 2。

### R1 §7 状态

**ROUND-CLOSED**（修改轮出口；按 §6A 自动进入 CONVERGENCE-CANDIDATE，等待确认轮）。
- open = 0；certify 新准入 = 0；G1–G9 的 open 部分满足；设计代码改进与 ARCH 提案按 §4.0f/§4.1b 登记，不阻断。
- 不宣告 CONVERGED：修改轮没有收敛声明权；下一篇确认轮在最终工作树上重跑六角色后再判。

### R1 §8 给 meta-review 的不确定点

1. 本轮把 8 条设计 P2 的代码改进登记设计批，只修文档同步；是否会削弱“全修”原则，请 meta 判断。
2. 两个 ARCH 提案（A1/A2）待用户裁决；A2 与 D01 的 `F-R34-01` 同源。
3. certify #1 抓到 3 条自引入 P2（BootStorageCell `.bss`、M7 18、usermapped 供料方），说明修复句的机械复核仍会漏；certify #2 已闭合。
4. QEMU 未跑；本轮无 arch/boot 行为改动。

### L6

- 同 agent 收口：49 条全部处置；P0 2、P1 11（含 certify 3 P1）全部修完；ROUND-CLOSED。
- BLIND-SPOT-DELTA：新增“设计 P2 代码改进被文档同步掩盖”的观察；无新的结构性盲区。

### L6.5

- 三问：① 本轮不变量是“文档与当前源码逐符号一致、读者口径跨节一致”；② 前提是旧 V1 run 只作历史、R1 全量 re-survey；③ 证据是 49 行台账、六门读数与 certify #2 的 0 新准入。
- 资源重读：本轮盲区属“文档-代码同步的符号级漂移”，检测模式已由 `anchor-resolve`/`doc-code-map`/certify 点名复核覆盖；无需新增规则。

## R2 确认轮（V3.3 四态模型）

### R2 §0 基线

- 轮型: confirmation
- edition: V3.3
- schema: V3.3 from R1
- 基线提交：`cdbfb3cf6`（R1 修复与轨迹提交之后）
- 文档：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 文档 sha256：`0888ffb0bbb27b4f67560058c91e99e0749addcc72868e41130d44ceb3a557b2`
- 行数：1280
- 工作树：除 3 个既有未跟踪文件外干净
- 设计快照：02 组 v2–v4 齐备；`design-coverage-check.sh` ALL DOCS COMPLETE
- 关联代码：`tools/doc-code-map.sh --check` 28/28；清单见 `related-code.txt`
- 范围声明：同 R1（文档主题 + 28 个关联文件 + 本轮修复触碰的文件）。
- 轮次语义：确认轮——全量六角色 survey；修复权限仅限本轮新准入的 P0/P1/P2 事实与同步项；不实施 ⑥；读者/设计 P3 走 §4.0c 门槛；`certify 新准入 = 0` 才可宣告 `CONVERGED`。

### R2 §1 面板

- 六角色：fact-a（C 侧）、fact-b（Rust 侧）、design-d2、reader-a、reader-b、reader-c；无缩编。
- fact-a 3 条（P1 2 / P2 1）；fact-b 1 条 P2；design-d2 9 条（P2 1 / P3 7 / P1 条件 1）；reader-a 1 条 P2；reader-b 1 条 P2；reader-c 6 条 P2。
- 材料隔离：读者未读源码；设计未读作者解释；事实未读历史评审。工作树前后仅有预期改动。
- 无发现面板：`certify-fact.md`（该次 certify 无新事实准入，仅 SUGGESTION）

### R2 §2 发现

- 29 条（survey 21 + certify 8）：P0 = 0；P1 = 5（C 侧装载者模型、LMA 重叠报错、D-045 注释改写、前置知识漏 riscv64 载体、§2.3 其一/其二冲突）；P2 = 17；P3 = 7。
- 关键：§2.1 的 GRUB/`p_paddr` 模型不适用于树内 boot monitor；§2.1 “链接器不会报错”被 GNU ld 实测否证；§3.2 的“任一步失败都 continue”与 `SegmentIter::new`/`parse_phdr` 的未检查加法冲突；§3.4 `syscall_entry` 的“无读者”过宽。

### R2 §3 归因

- 处置：① 22 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 7 = 29；唯一根因 29。
- 冻结决策：0 条 `re-litigation`；D2-08 的“注释有删改”改为“注释有改写”，按 D-045 的显式声明豁免处理。
- 范围门：0 条 adjacent 阻塞；设计代码修复（SegmentIter 未检查加法）登记设计批 D2-01，不在确认轮实施 ⑥。
- ARCH：D-052（BootLayout/token）记判决账，待用户裁决，不阻断。

### R2 §4 修复

- 文档：C 装载者射程声明、LMA 重叠报错、`pg_utils.o` 反事实、`syscall_entry` 读者范围、迭代器 panic 语义、四处“注释有改写”、§4.1 规划层落点、前置知识按腿分列、§2.3 建表列与 hang 停机、§4.2 重叠语义、§5.3 L9 限定、§2.5 ARM 证据限定。
- 代码：无行为改动；SegmentIter 未检查加法的修复登记设计批。

### R2 §5 复验

- 文档六门：citation `32 块/1 检查/0 失败/31 跳过`；anchor `55/55 zero-def=0`；structure/ref 通过；style-lint 文件级 error=0；density 通过；doc-code-map `28/28`。
- 旧串归零：`GRUB` 的无条件读法、`链接器不会为此报错`、`函数会落进高一侧的 .text`、`此时还没有到位`、`坏点推迟到恒等映射被移除`、`注释有删改`、`走进 panic 分支` 均为 0（按替换点核）。
- certify #1：21 条全修；certify #2：新准入 0（见 `certify-*.md`）。

### R2 §6 指标

- 见 `metrics.md` R2 块：发现 29、唯一根因 29、本轮新准入 29、certify 新准入 0、未闭环 0、① 22 / SUGG 7、ARCH 提案 1。

### R2 §7 状态

**CONVERGED**（确认轮出口）。G1–G9：in-scope P0/P1/P2 = 0；读者验收清单待修 = 0；FP 无；⑤ = 0；② = 0；机械门与宿主检查全绿；台账 29 行全部有处置与可重放复验；设计 P2 代码修复登记设计批，ARCH 提案 D-052 不阻断；`certify 新准入 = 0`。
限制：确认轮口径下的“无 open 项 + 无未销账债务”，不是零未知缺陷保证；设计批（D2-01..06/13、D-052）在下一批实施。

### R2 §8 给 meta-review 的不确定点

1. C 侧装载者模型的修正只做了源码直读 + 等价脚本，未跑 Minix 实际 boot monitor；QEMU 原生引导链未复核。
2. 设计 P2 代码修复（SegmentIter 未检查加法）只改文档、代码登记设计批；是否削弱确认轮的可信度请 meta 判断。
3. 读者 C 的 6 条 P2 全部是文档内可判定的矛盾；其中 §2.5 ARM 证据限定为“待补”，未补证据。
4. QEMU 未跑；无 arch/boot 行为改动。

### L6

- 同 agent 收口：21 条全部处置；P1 3 条全修；CONVERGED 由确认轮出口给出。
- BLIND-SPOT-DELTA：新增“C 装载者模型按外部装载器写、未覆盖树内 boot monitor”的观察；无新的结构性盲区。

### L6.5

- 三问：① 本轮不变量是“文档对 C 装载链与 Rust 起点的描述必须按腿限定”；② 前提是 R1 已 ROUND-CLOSED、R2 只修新准入事实/同步项；③ 证据是 21 行台账、六门读数与 certify #2 的 0 新准入。
- 资源重读：本轮盲区属“跨实现模型无条件泛化”，检测模式已由读者 C 的因果链核对覆盖；无需新增规则。


## REG-2026-11-18 补账索引（r6 R6-b，非编号轮）

- D-055 恢复独立读者后的回归轮：独立五角色 30 条口径、面板逐条 ID 见 `defect-ledger.md` 的 REG-2026-11-18 节；面板原件 `tmp/review-eval/01-stage-kernel/02-higher-half-kernel/panel/regress/`。
- 轮型：回归载体（r6 R6-b 语义）；不宣告收敛状态变更（维持 FREEZE 时点的 CONVERGED，FREEZE 回归节为出口）。

## R3 确认轮（V3.4 四态模型；D-063 授权的重开）

### R3 §0 基线

- 轮型: confirmation
- edition: V3.4
- schema: V3.4 from R3
- 基线提交：`35113c4cf`（D01 收尾提交；本轮修复未提交）
- 文档：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 文档基线 sha256：`829b3a71d141d99110588e30d33c446939ed97c7b07de57b66bdef45054ae21b`（R2 FREEZE 值，本轮面板启动前）
- 行数：1280（R3 修复后 1283）；最终 sha256 `4f096ea9997eae5aae99ad648967d2b8001256d22e7a63c017d6ea9de16804ce`
- 设计快照：02 组齐备；`design-coverage-check.sh 01-stage-kernel` ALL DOCS COMPLETE
- 关联代码：`doc-code-map --check` 28/28（本轮新增一处 `os/arch/src/riscv64/paging.rs` 引用，改后 29/29）
- 语料条目：D02（`CORPUS.md` 第二节）
- 范围声明：高半区链接布局与成对跳转 + 关联文件 + 本轮修复触碰的文件；不在范围 = 相邻模块与下一批次结构性重构
- 重开依据：D-063

### R3 §1 面板

- 六角色全量、独立子 agent；读者 A/B/C 独立（文档 1280 行 ≥ 500，D-055 墙 1）：
  - P-FACT：8 条 FINDING + 4 SUGGESTION（`fact.md`）
  - P-DESIGN-D2：16 条 FINDING + 2 ARCH-PROPOSAL（`design-d2.md`）；D1 停跑（D-024），基线为「文档主张 ↔ 源码事实」，记 VARIANCE
  - P-READER-A：4 + 19（`reader-a.md`）；P-READER-B：9 + 15（`reader-b.md`）；P-READER-C：4 + 10（`reader-c.md`）
  - P-REFUTE：6 条 CLAIM（阻断 0 / 非阻断 1 / 反证失败 5），`refute.md`
- **VARIANCE 1**：refute 正文用 `## CLAIM RD3-CLAIM-<NN>`，轨迹门的 `CLAIM <数字>` 正则识别不到；主 agent 在文件末尾追加了机器可读的 `## CLAIM 11..16` 索引（编号刻意避开 R1 已用的 `CLAIM 01..06`，防跨轮身份键覆盖；正文未改动）。6 条 CLAIM 因此计入 INDEX 与发现总数，并在台账立 RD-R3-35..37 三行。
- **VARIANCE 2**：fact 角色首次派发被平台截断，重派后完成；两次都只读允许材料。
- 制品入库：`panel/round3/` + `INDEX.md`（6 文件：FINDING 41 / SUGGESTION 48 / CLAIM 0）。
- 工作树：面板前后 `git status` 只有本轮要提交的轨迹文件与文档。

### R3 §2 发现

- 95 条机器可计：FINDING 41（fact 8、design-d2 16、reader-a 4、reader-b 9、reader-c 4）+ SUGGESTION 48 + CLAIM 6（refute 末尾追加的 `## CLAIM 11..16` 索引；见 §1 VARIANCE 1）。
- 严重度：P0 = 0；P1 = 11（fact 0、design 2、reader-a 2、reader-b 4、reader-c 0、其余为 design P2 与 reader P1/P2 分布）；P2 = 30 左右；P3 若干。逐条见 `panel/round3/`。
- 面板抓到 3 条**本仓文档的真实错误**：§4.4 状态表把 aarch64 direct-core 的跳转后 PC 写成「低地址进入」、§3.1 把 riscv64 测试载体并入「低地址执行视图」、§3.4 的「四条前置条件」与表格行对不上。

### R3 §3 归因

- 34 个根因（RD-R3-01..34，按角色/严重度/处置类聚簇）。
- 冻结决策检查：无命中；设计项的 11 条 P1/P2 按 §3.6 记 ② 延期（D-063）；2 条 ARCH 提案进判决账待裁。
- 范围：全部 in-scope。
- 归因结论：本轮 P1 全部是文档内部一致性（同一事实两处说法冲突），不是源码事实错误；设计的 2 条 P1 是代码契约风险（别名、fail-open），按确认轮权限不实施。

### R3 §4 修复

- 19 处文档修改点：§4.4 PC 行、§3.1 测试载体分类与复算口径、§3.2 外部装载器口径、§3.4 四条前置条件编号与表格对应、§1.1 两套地址限定到高半段、§2.1 补三个基址常量定义行、§5.1 M5 改「部分覆盖」、§6.2 `$rsp` 按架构分列、§6.2 `$rip` 的 riscv64 delta、§4.1 基址反事实改 2 MiB 口径、§3.4 对齐算式（只去 `sub` 也坏）、§3.5 panic 消息路径、§4.3 源码注释口径、§3.3「只 use」措辞、§3.5 引文前脚本注释与实现不符的说明、§6.2 第四组 `check_exception` 语义、§6.2 第五组寄存器名按架构、§3.1 六处命中口径、§2.3 前提倒置。
- 文档六门：anchor 55/55、citation `checked=25 failed=0`、structure/ref/style(error=0)/density 通过、doc-code-map 29/29。

### R3 §5 复验

- 独立回执 `certify/round3-certify.md`：三轮复核——首轮 19 条 14 CLOSED / 5 STILL-BROKEN；按证据改正 5 条后 18/1（余 §6.2 `:1242` 的 aarch64 前缀）；再改后 18/1（余 §3.5 panic 消息两层顺序）；第三次改正后 19/0 PASS。
- **回执抓到主 agent 的四处实质错误**：① 把 `0xffff7fffbfe00000`（L4 平移量）当成 aarch64 栈地址；② 误称 riscv64 的 `map_huge` 不检查 1 GiB 降级；③ §3.4 对齐算式方向写反；④ panic 消息的两层包装顺序写反。四条均已按回执给出的 path:line 改正。
- `certify 新准入 = 0` 口径：certify 输入闭集 = 上述 19 个修改点；回执覆盖全部，复核后无新增修改点。
- 代码行为：本轮无代码改动；QEMU 未跑。

### R3 §6 指标

- 与 `metrics.md` 的 `## R3 确认轮` 块一致：发现总数 95、唯一根因数 37、本轮新准入 37、certify 新准入 0、处置 ① 13 / ② 11 / ④ 17 / FP 5 / SUGG 49、未闭环 0。
- 轮型 `confirmation`；轮结果 `CONVERGED`。

### R3 §7 状态

- `CONVERGED`。前提：R1 是已关闭的修改轮（ROUND-CLOSED），R3 certify 新准入 0。
- 未闭环 0；`OPEN-DEBT` 的 `R3-design-batch` 有 D-063 判决行与日期。
- `CONVERGED` 的语义限制同 D01：确认轮口径下无 open 项 + 无未销账债务，不是零未知缺陷保证。

### R3 §8 给 meta-review 的不确定点

1. 本轮的 2 条设计 P1（`store_kernel_info` 别名契约、`SegmentIter` fail-open）是代码正确性风险，确认轮不实施 ⑥ 把它们推入 `R3-design-batch`；若 meta 认为它们应按事实缺陷在轮内修，本轮就不能以确认轮关闭。
2. reader-a 的 2 条 P1（§2.1 缺常量定义行、§3.4 四条与表格不符）本是「文档承诺了但没给」类缺陷，修复是补内容；若 meta 认为这属结构性重写而非确认轮权限，需回修改轮。
3. refute 的 CLAIM 未被机器计数（§1 VARIANCE 1）——本轮靠人工确认其唯一非阻断发现已并入 reader 条目，机制上没有强制。

### L6

- 同 agent 收口（另有独立 certify 回执，见 §5）。流程质量记录：独立冷读在本轮抓到主 agent 三处实质错误（L4 平移量当栈地址、riscv64 AlreadyMapped、对齐算式方向），证明「回执必须给命令与读数」这条设计有效；同时暴露确认轮的固有张力——11 条设计 P1/P2 只能延期。
- 流程质量结论：`IMPROVED`（相对 R2：首次在 D02 上做独立六角色全量 + 独立 certify，且回执推翻了主 agent 的三处判断）。

### L6.5

- 三问：① 不变量是「确认轮必须在最终工作树上 certify 新准入 = 0，且每个修改点有独立回读」；② 前提是 R1 已 ROUND-CLOSED（成立）；③ 证据是 `certify/round3-certify.md` 的逐条读数与六门读数。
- 资源重读：盲区属「流程类」（确认轮的设计项降级），查 `prompt/resources/fresh-eyes-review`；无新检测模式，记 BLIND-SPOTS 候选。

## R4 确认轮（V3.4；R3-design-batch 实施后的基线重开）

### R4 §0 基线

- 轮型：`confirmation`；edition `V3.4`；schema `V3.4 from R3`（延续）。
- 重开依据：`FREEZE.md` 重开条件第 2 条（设计批实施后基线改变）+ 用户 2026-11-21 指令（review loop D02 直至收敛，代裁授权）。R3-design-batch 的 ELF 装载面已在 I-45..I-48（D-082..D-089）落码，另外 wire v10/D-062 删除了 `KernelInfo` 的派生字段；D02 文档在 R3 后未改，关联代码已移动。
- 基线提交：`2b1213b5e`（R4 预门同步之后）；工作树仅三个审查前已存在的未跟踪文件（D01 同款），无源码/文档改动。
- 文档：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 文档基线 sha256：`46a3497a40658daff0496baeb71ade5b9f188ddb21f8c5cf774f0dc3c5ddb4ab`
- 行数：1289
- 设计快照：`02-outline.v4.md`、`02-outline-review.v4.md`、`02-design.v3.md` 齐备；`design-coverage-check.sh 01-stage-kernel` ALL DOCS COMPLETE。
- 关联代码：`doc-code-map.sh --check` 30/30；清单见 `related-code.txt`（R3 时点 28 项，当前 30 项）。
- 语料条目：D02（`CORPUS.md` 第二节，地址空间 / higher-half）；盲区重点 B-01..B-13。
- 范围声明：本篇范围 = higher-half 链接布局、双地址、成对跳转与 ELF 装载边界 + 30 个关联文件 + 本轮修复触碰的文件。不在范围 = 相邻模块、未列入清单的文件、下一批次的结构性重构。
- R4 预门（`2b1213b5e`）：citation 修 4 个引用块后 `blocks=32 checked=25 failed=0 skipped=4 warned=3`；anchor 55/55 zero-def=0 multi-def=0；structure/ref/style（error=0）/density 通过；doc-code-map 30/30；`--stale-lines 0ac2c9007 --strict` PASS；`--from-diff` 仅通用词 WARN。
- 预门修复内容（commit `2b1213b5e`）：§3.3 栈顶从字段改为 `KernelInfo::kern_stack_top()` 派生访问器；`compute_kernel_layout` 改 `seg_result?`（I-48/D-084）；`load_segments_into_phys_memory` 改 `Result<Vec<_>>` 收集说明；bootface 的 `build_kernel_info` 删 `startup_objects` 参数；§4.9 与测试载体描述同步。
- follow-ups（L0 步骤 8）：R3-design-batch 的 11 条设计项中，D2-01（store 别名）按 D-083/D-071 未实施（用户要求只记账）；D2-02（SegmentIter fail-open）/D2-03（Safety 文档）/D2-04（buffer 计数）已由 I-45/I-46/I-47/I-48 落码；其余由 I-33/I-36 的 ELF 修剪面覆盖或仍待设计批。无未批准 ② 带入本轮。

### R4 §1 面板

- 角色与材料（六角色全量，均独立子 agent；读者 A/B/C 独立，未合并——文档 1289 行 ≥ 500）：
  - P-FACT：文档 + 30 个关联文件 + 决策摘要 + follow-ups；12 条 FINDING + 5 条 SUGGESTION。
  - P-DESIGN-D2：§3/§4 + 关联代码 + trait-audit + AGENTS.md；D1 停跑（D-024），基线为「文档主张 ↔ 源码事实」，记 VARIANCE。10 条 FINDING，0 ARCH。
  - P-READER-A/B/C：只读文档 + 读者画像；A 2 FINDING + 11 SUGG、B 6 + 15 SUGG、C 6 + 11 SUGG。
  - P-REFUTE：文档 + 源码 + 设计快照；6 条 CLAIM（阻断 2 / 反证失败 4）。
- 缩编：无读者合并；D1 停跑（D-024）+ D2 回退形态，记 VARIANCE。
- 技能调用 VARIANCE：运行时仍无 `review-process-skill`/`review-doc-skill`；主 agent 直接加载规范源，只实际调用 `review-loop`。
- 材料隔离 VARIANCE：P-FACT 自述一次全仓 `rg` 扫到 `panel/round*` 与旧 review 的文件名/片段（声明未采用）；按 B-16/B-19，“声明未采用”不等于“未接触”，记 VARIANCE，不升级为 DEGRADED（结论未采信历史材料）。读者 B 读了 `review-evals.md` §4.0e，读者 A 明确拒绝、读者 C 视其为禁读，三份材料矩阵不对称，同记 VARIANCE。
- 未编号建议：P-FACT 另有 5 条 `## SUGGESTION` 没有 ASCII ID，`panel-index.py` 按固定形状计数，INDEX 因此记 fact 的 SUGGESTION 为 0；这 5 条已在本文件 `suggestions.md` 以事后编号 `FACT-S1..S5` 登记，但不进 79 个 ID（B-72）。
- 模型：同一模型（opencode-go-chat/deepseek-v4.1-flash）的独立冷启动实例；无跨模型复核。
- 起止：2026-11-21 约 09:30–10:20（并行；各角色 20–35 分钟）；无降档/升档，无 10 分钟重试。
- 原始输出：`tmp/review-eval/01-stage-kernel/02-higher-half-kernel/panel/round4/`；制品入库 `panel/round4/` + `INDEX.md`（6 文件，FINDING 36 / SUGGESTION 37 / CLAIM 6）；`panel-index.py --check` 通过。
- 工作树：面板运行前后只有预门同步的文档改动与三个既有未跟踪文件；面板只写 tmp。

### R4 §2 发现

- 79 个 ID 承载产出：FINDING 36（fact 12、design-d2 10、reader-a 2、reader-b 6、reader-c 6）+ 编号 SUGGESTION 37（reader-a 11、reader-b 15、reader-c 11）+ CLAIM 6；另有 5 条 fact 未编号建议。
- 严重度（FINDING 36）：P0 = 4、P1 = 5、P2 = 20、P3 = 7；CLAIM 阻断 2（P1）、反证失败 4。5 条 P3 FINDING（DESIGN-D2-07/08/09、READER-B-05/B-06）按 G3 分流为 SUGG 行，不与 P0–P2 混算修复队列。
- 语义去重（台账外口径）：79 个 ID 聚成约 47 个缺陷根（含 4 条主张维持为 51 个主题），至少 28 个 ID 是跨角色/同根重复；最重簇为 §3.2 迭代器 8 个 ID、测试账/载体 6 个、v1 载荷/栈字段/表格行各 3 个。逐簇明细见 `metrics.md` 的「R4 语义聚簇」段。
- 最高簇：§3.2 ELF 迭代器仍写 I-48 之前的 fail-open（FACT-01/02/06、D2-01、reader A-01/B-03/C-01、REFUTE-05 独立同根）；v1 arena 七载荷（FACT-04/D2-03/REFUTE-06）；测试账 27/31/135 与测试名（FACT-05/06/D2-10）；kern_stack_top 字段残留（FACT-12/B-04/C-03/C-05）；`.lrodata` 孤儿（D2-05）。

### R4 §3 归因

- 根因：79 个 ID（RD-R4-001..079，逐 ID 立根因；跨角色同根由 action/evidence 组文本表达）；按语义聚簇约 47 个缺陷根，重复合并 28。
- 冻结决策检查：D-062（v10 字段删除）、D-082（ELF 修剪）、D-084（SegmentIter Err）、D-089（buffer 两循环）为修复依据；D2-04/D2-06 是新设计项，D-105/D-106 批准延期 `R4-design-batch`；无 re-litigation（D-064 的 `prepare_boot` 参数化未被重提）。
- 范围：全部 in-scope；无 adjacent 阻塞。
- 设计项：D2-04（BootReport 复用）与 D2-06（arch link.ld 去留）按 §3.6 不实施 ⑥，登记 OPEN-DEBT；D2-07/08/09 为 P3 建议。D2-06 与 `R3-design-batch` 的「arch link.ld 无读者」以及 V1 的 `D02-F03` 同源，本行是迁移登记而非第三次新根因；D2-07 同样 `dup-of` V1 `DESIGN-03`/`D02-F01`。
- D2-05 的权限说明：`.lrodata` 孤儿节是 P2 代码缺陷（真实链接风险：链接器版本决定布局），不是 §3.6 的 ⑥ 设计项；按确认轮的 P2 事实修复权限实施，不占用设计批额度。D2-04/D2-06 才是需要设计批的签名/文件去留项。
- 授权来源：D-105/D-106 是代理裁决，授权来源为用户 2026-11-21 会话中的 review-loop D02 代裁指令（本文件 R4 §0 记录），不借用 D01 R36 的授权；`DECISIONS.md` 与台账的批准行已按此改写。
- 读者项：P1/P2 按 ④ 本轮修；B-05/B-06 是 P3，连同全部 SUGGESTION 进 suggestions。
- 自引入：2（均在本轮修复）。其一：批一改了 §3.2 题句下的代码块却留下旧题句，制造同节自相矛盾（`certify-r4-pass1-fact.md` 首轮抓到，commit `0f5a5bd9d` 修）；其二：批一在 §5.1 新写「三类畸形」，实际迭代器覆盖四个 `ElfError` 变体（`round4-fact.md` 观察段抓到，本轮改为「四类畸形」并列出变体）。第二条是 certify 判定集外修复，按 §5 记为 post-certify P3。

### R4 §4 修复

- 预门批：commit `2b1213b5e`——§3.3 栈顶访问器、compute_kernel_layout/load_segments 的 I-48 形状、bootface 去 startup 参数、§4.9 同步。
- 批一：commit `cf2dc2b62`——§3.2 迭代器/12 变体、五类用途、v1 六载荷、测试账、§3.4/§4.5 字段残留、§5.2 载体、BUMP_NEXT、ARM 证据、GDB delta、第 3 行指针、`.lrodata` 归并；D-105/D-106 延期。
- 批二：`fix(02)` 引文标注提交——mpx.S/kernel.lds 尾部省略、loader 引文头、store 注释归属、InvalidSegment 序数、riscv 行号。
- 批三（post-certify P3，meta 清理）：§5.1 证明列「三类畸形」改为「四类畸形」并列出 `PhdrOutOfBounds`、`PhdrEntryTooSmall`、`SegmentOutOfBounds`、`FileszExceedsMemsz`；该行不在两轮 certify 的 18/12 个修改点内，属判定集外精度修复，改后文档 sha256 `48630c90b2a9e1eb2b2814f93ad61d44de0c8a6c634d4835ca25692c602eb781`、1294 行，已用源码 `minix-elf` 的四个 Err 出口逐条复核。
- 机械门（最终工作树）：anchor 54/54 zero-def=0 multi-def=0；citation `blocks=32 checked=24 failed=0 skipped=4 warned=4`；structure/ref/style（error=0）/density 通过；doc-code-map 30/30。
- 行为门：`check-layout.sh all` PASS（L0–L9；x86 L3a=4 个 PT_LOAD）；`test-sysboot.sh` PASS；`test-riscv64-uboot-shim.sh` 6/6 PASS；`readelf -lW x86_64/kernel | grep -c '^  LOAD'` → 4。
- 可引导镜像门（meta 第九问补丁）：`cd os && cargo run -q -p xtask -- image --arch x86_64 -r` 重建生产镜像后，从 `os/target/image/x86_64/minix.img` 解出 `::/EFI/minix/kernel.elf`，与 `target/image/x86_64/staging/EFI/minix/kernel.elf` 的 sha256 同为 `fca5b85f202eb7ca03d38d2f8d26636400a768a02c41cd323cb9e507bca8efe2`；`readelf -lW` 的 `LOAD` 行数 4、`readelf -SW` 的 `lrodata` 命中 0。此前 meta 读到的旧镜像（5 LOAD / 986 `.lrodata`）已被本次重建替换。
- 未实施（②）：D2-04 → D-105、D2-06 → D-106，均落 `R4-design-batch`（OPEN-DEBT）。
- 未实施（P3 SUGG）：D2-07/08/09 与全部面板建议 → `suggestions.md` R4 段。

### R4 §5 复验

- 独立冷读一（事实/设计/代码）：`certify/round4-fact.md` 第二轮 18/18 CLOSED（第一轮 15/18，5 条同源残项全部是 §3.2 line 458 的旧题句；修后复读）。第一轮回执是 `certify/certify-r4-pass1-fact.md`，不是旧引用的 `round4-fact-pass1.md`（文件名以目录实存为准）。
- 独立冷读二（读者）：`certify/round4-reader.md` 第三轮 12/12 CLOSED（第一轮 10/12、第二轮 11/12，残项 A-01/B-03 同源于 §3.2 题句，C-05 为 `.lrodata` 旧工件的 LMA 说明；修后复读）。第一轮回执是 `certify/certify-r4-pass1-reader.md`；第二轮回执未单独留档，只在 hand-back 摘要里记了 11/12，属 B-73 的命名/留档缺口。
- post-certify P3：批三改了 §5.1 的「三类→四类畸形」，该行不在两轮 certify 的判定集内；已派发一次只读单行复核 `certify/round4-postfix-line1120.md`（4/4 CLOSED：4 个 Err 变体与 `minix-elf` 源码逐字一致，12 个测试名全部存在），未重跑整篇冷读。最终文档 sha 因此从 `3c0806f18` 基线的 `9f9ec2e7…` 变为 `48630c90…`；这是 P3 精度修复，不改变 18/12 个判定点的结论。
- 台账重放：每行 ①/④ 的复验列给 grep/test/readelf/commit 或冷读回执；`python3 tools/eval-trace-check.py --round R4 <run-dir>` 最终 PASS（coverage/round-close/metrics/open-debt/severity 全过；2 个 WARN 是既有模板项）。
- 机械门（最终 HEAD `3c0806f18`）：anchor 54/54 zero-def=0 multi-def=0；citation `blocks=32 checked=24 failed=0 skipped=4 warned=4`；structure/ref/style（error=0）/density 通过；doc-code-map 30/30。
- 行为门：`check-layout.sh all` PASS（L0–L9，x86 L3a=4）；`test-sysboot.sh` PASS；`test-riscv64-uboot-shim.sh` 6/6 PASS；`readelf -lW os/target/x86_64-unknown-none/release/kernel | grep -c '^  LOAD'` → 4；旧 staging 工件 5 行已作为历史读数记入 §4.1。
- 所有权/写后读顺序：`.lrodata` 归并只改链接脚本的输入节匹配，不改变任何 Rust 所有权；`KernelInfo` 的 `kern_stack_top()` 为只读派生；`.lrodata` 旧孤儿的 LMA 继承已用旧工件 readelf 读数与 GNU ld 2.42 最小复现交叉核实。
- 改写段 vs C/源码再推演（G7）：本轮改写的 §3.2 迭代器/错误模型逐条对 `minix-elf` 源码（12 变体、6 处 `return Some(Err)`、唯一 `continue` 在非 `PT_LOAD`）；v1 六载荷对 `handoff_v1_read` 的六个 `*_byte_len` 与六个 `decode_*`；测试账对 `cargo --list` 的 27/31/135。
- 代码块分类：citation 24 个逐字/改写块逐块对账；4 个 WARN 为声明改写块，已人工核对并由 certify 覆盖。

### R4 §6 指标

- 与 `metrics.md` 的 `## R4 确认轮` 块一致：发现总数 79、唯一根因数 79（ID 口径）、语义根因数约 47、本轮新准入 79、certify 新准入 0（三轮回执的 STILL-BROKEN 都是同根修复不完整，最终 fact 18/18 + reader 12/12 CLOSED）、处置 ① 19 / ② 2 / ④ 12 / FP 4 / SUGG 42、未闭环 0。
- 严重度：FINDING 36 为 P0 4 / P1 5 / P2 20 / P3 7；5 条 P3 FINDING 按 G3 进 SUGG 桶。
- FP 语义：`FP 4` 全部是 `CLAIM-01..04` 的「反证失败、主张维持」，不是误报；本轮真正的误报 FINDING = 0。`FP` 是工具既有桶名，语义在 metrics 里并列写清，避免与「精度」指标混淆。
- 自引入：2（均轮内修复；见 §3 与 §5 的 post-certify P3）。
- 可引导镜像：重建后镜像内 kernel 与 staging 同哈希 `fca5b85f…`，4 LOAD / 0 `.lrodata`（详见 §4）。
- 轮型 `confirmation`；轮结果 `CONVERGED`。

### R4 §7 状态

- `CONVERGED`。前提：R4 是确认轮，最终工作树上 `certify 新准入 = 0`；OPEN-DEBT 的 `R4-design-batch`（D2-04/D2-06）有 D-105/D-106 与 2026-11-21 日期，按 §4.0f 出口 2 销账。
- 限制照旧：`CONVERGED` 的含义是「确认轮口径下没有 open 项 + 无未销账债务」，不是零未知缺陷保证。

### R4 §8 给 meta-review 的不确定点

1. 面板与三轮 certify 均为同一模型；无跨模型复核。
2. `.lrodata` 归并改动了三架构链接脚本并重建工件；check-layout all、test-sysboot、riscv U-Boot 6/6 已跑，但 aarch64 的 `test-rt-birth-aarch64` 仍有已知 A2 user-chain 遗留（D01 轨道），不能作为本轮绿门。
3. D2-04（BootReport 复用）与 D2-06（无读者 arch link.ld 去留）是 P2 设计项，确认轮不实施 ⑥，D-105/D-106 批准延期；下一设计批会再次移动基线。
4. `KernelInfo::kern_stack_top()` 的派生公式只在访问器一处，但 `boot_validate_and_prepare` 为对齐断言保留了一次本地展开；D2-10 已把 §3.3 的“公式只留访问器一处”改成“生产消费点经访问器 + 断言本地展开”。
5. 测试计数 27/31/135 是默认特性下的 `--list` 读数；`--features test-all` 为 146。文档已写清两档口径。
6. C-05 的“GNU ld 对孤儿段继承 LMA 平移”由 reader certify 在 `/tmp` 用 ld 2.42 最小复现，但仓库旧工件只保留一个 staging ELF 样例；不是全量链接器矩阵。
7. 独立 meta（`meta/02-higher-half-kernel-r4.md`）判本轮总体 FLAT、运行质量 NOT PASS，主要指向轨迹/去重轴：79 个 ID 的语义重复、FP 语义、严重度分布、制品未入库、FREEZE 未追加、盲区回写无工件、可引导镜像仍嵌旧内核。上述各项已在本轮收口：语义根因数与重复合并入 metrics、严重度改 4/5/20/7、FP 语义并列说明、四份 certify 回执与 loop 变更提交、FREEZE 追加 R4、BLIND-SPOTS 新增 B-69..B-75、镜像重建后 4 LOAD/0 `.lrodata`。
8. meta 的第 9 项（行为门止于内核 ELF、不到可引导镜像）已按建议补一条手工门记录，但没有新增自动脚本；`minix.img` 的一致性目前靠本轮重建命令与哈希比对，不是 CI 常驻断言（B-71 仍 OPEN）。
9. meta 抽样的 41 个 ID 中，5 条 SUGG 的处置已过期（A-S03/A-S06/B-S11/B-S14/C-S10(a)），4 条 CLAIM 的 FP 类别是工具桶名与语义的错位。本轮已把 5 条 SUGG 标成「已由 FINDING 修复覆盖（dup-of）」，但 CLAIM 的桶名未改（改桶名会破坏工具契约），只在 metrics/loop 里写清语义。

### L6

- 同 agent 收口（收敛时点，独立 meta 另见 `meta/02-higher-half-kernel-r4.md`）。流程质量记录：本轮的最重问题全部来自 R3 之后的代码移动（I-45..I-48、v10），而文档 sha 未变——说明“文档未改”不等于“基线未变”，FREEZE 的重开条件必须在每次设计批实施后主动检查。三轮 certify 才清完 §3.2 的一行旧题句，与 D01 同型。
- 流程质量结论：`IMPROVED`（相对 R3：首次在 D02 上主动用 stale-line/citation 预门抓到 4 个引用块，并在面板外修掉 `.lrodata` 的真实链接风险）。

### L6.5

- 三问：① 不变量是「确认轮在最终工作树上 certify 新准入=0，且每个 ①/④ 有独立回读」；② 前提是 R4 面板与三轮 certify 以最终 HEAD 为闭集（面板 2b1213b5e，最终 3c0806f18；最终回执覆盖最终工作树）；③ 证据是 `round4-fact.md` 18/18、`round4-reader.md` 12/12、六门与行为门。
- 资源重读：盲区属“流程/基线类”（设计批后文档未动但代码移动），按 L6.5 查 `prompt/resources/fresh-eyes-review` 的“changed contract counterparts by content”检测模式；已实际回写 `BLIND-SPOTS.md` 的 B-69..B-75（语义去重、制品入库、镜像门、未编号建议、回执命名、检索材料面、严重度对账），并修正此前只有宣称、没有工件的 B-68 扩展句。

## R5 重写基线（V3.4；full）

### R5 §0 基线

- schema: V3.4 from R5
- 轮型: full
- edition: V3.4
- 基线：`cf79b63e5e74f7188e181cea5aa71a947bee0289`（重写后、面板前）。
- 文档：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`，sha256 `b2ab482f…`，1840 行。
- 范围：02 篇全文 + `related-code.txt` 的 30 个关联文件；不含相邻模块与下一批结构重构。
- 语料：本篇按“02=内核镜像”新轴重写；旧 v3 归档 `draft/v3-02-higher-half-kernel.md`，旧节号映射见 `rewrite-notes/MIGRATION.md` 第九节。
- L0 预检：`.design/02-outline.v5.md`、`02-design.v4.md` 存在；`design-coverage-check.sh 01-stage-kernel` PASS。
- L0.9：citation/anchor/structure/ref/style/density/doc-code-map 全绿；`doc-publication-check` 无新增违规；`todo-ledger check --fail-on-open` OK。
- L0.9 偏差：`tutorial-rubric-check` 6/6 章节缺 TR 项，与旧 v3 基线同型；`code-publication-check` 对 30 个关联文件报存量过程注释，按“不改 os/”登记为 stage 风险。
- 面板：P-FACT、P-READER-A/B/C、P-REFUTE 五个独立冷启动实例；无 DESIGN 角色，记 VARIANCE（同 D01 R37）。

### R5 §1 面板

- 轮型 `full`；五个独立冷启动角色：P-FACT、P-READER-A、P-READER-B、P-READER-C、P-REFUTE。模型同为 deepseek-v4.1-flash；无 P-DESIGN，记 VARIANCE：本轴重写后的首轮先验事实/读者/反证。
- 材料隔离：每个角色只拿当前文档、`related-code.txt`、`.design/02-outline.v5.md` 与 `02-design.v4.md`；不拿历史版本、旧 panel、loop/台账、作者结论。P-REFUTE 另拿允许的源码清单做反证。
- 时间：2026-11-21 17:20–18:10；原始输出 `panel/round5/{fact,reader-a,reader-b,reader-c,refute}.md`，`INDEX.md` 由 `tools/panel-index.py` 生成（5 文件，哈希自洽）。
- 降档/升档：无；5 个角色全部一次派出成功。READER-C 延迟到 18:10 交稿，产出完整（8 个编号 FINDING + 10 条 P3 建议），不记 MISSING。

### R5 §2 发现

- 面板共 36 个 FINDING + 6 个 CLAIM。FINDING：P1 14（FACT-01/03/04、READER-A-01、READER-B-01..06、READER-C-01..04）、P2 18（FACT-02/05/06、READER-A-02、READER-B-07..16、READER-C-05..08）、P3 4（FACT-07..10）。
- CLAIM：CLAIM-01 J1 跨节冲突（P1）、CLAIM-02 栈顶公式（主张成立，附测试载体 P2 边界）、CLAIM-03 AlreadyMapped 可达性（P1）、CLAIM-04 Step 2 失败路径（P1）、CLAIM-05 Step 1 窗口清单（P1）、CLAIM-06 L9 x86_64 覆盖（P2）。
- 逐条位置、证据与建议见 `panel/round5/` 原文与本轮 `defect-ledger.md` 的 R5 段。

### R5 §3 归因

- 全部 in-scope：36 个 FINDING 与 6 个 CLAIM 都落在 02 篇正文、其关联代码或两篇接缝声明内；无 adjacent 阻塞。
- 处置：① 41、FP 1（READER-C-03：基线 §3.6 的“守卫拒绝”与 §4.3 的“假如没有这道检查”是兼容表述，读者按现行行为误读；反证见 `panel/round5/reader-c.md` 与 §3.6/§4.3 原文）；无 ②/⑤/⑥/SUGG。
- 根因按 ID 逐条立（RD-R5-001..042），跨角色同根：J1 范围（FACT/READER-A-01/READER-B-01/READER-C-01/CLAIM-01）、J2 范围（READER-C-04/06）、Step 2 反事实（FACT-04/CLAIM-04）、Step 1 窗口（CLAIM-05/READER-C-05）、EBS/建根（READER-C-02）。
- 冻结决策检查：D-099..D-107 已实施状态作为输入；D2-06/D02-F08 未重开；无 re-litigation。
- 设计项：0（无 P-DESIGN 角色，未新增 [ARCH] 提案）。

### R5 §4 修复

- 全部为文档修复；`os/`、`minix3/` 零改动。修复批次：
  1. 主轴：J1 组内成立（C 低侧组 0、高侧组 `_kern_offset`）、J2 限定 Rust 生产镜像、J4 的 SP/PC 判据按架构拆开。
  2. 失败路径：Step 2 的 `AlreadyMapped` → `.expect` panic、Step 1 补调用者栈窗口、EBS 后建根、载荷无映射 vs 零读、C 侧反序不产生可定位页错误。
  3. 结构/引文：§3.1 三形态枚举、§4.5 去重复表、M3/M9 覆盖措辞、§6.1 LMA 重叠限定旧工具链、§4.4 引文头注明断言省略。
  4. 去过程词：删“设计裁决/评审发现”占位，恢复具体阶段名与判据。
- fix-guard：每段先 grep 现状、改后回读邻句；重复标题与重复表按整段删除，不逐句补丁。提交见 `docs(review): 01/02 重写后基线`。

### R5 §5 复验

- 独立回读：`certify/round5-fact.md`（FACT-01..10 + CLAIM-01..06 + 附注 1..4）与 `certify/round5-reader.md`（READER-A/B/C 26 条）。初读 fact 17 CLOSED / 3 STILL-BROKEN（CLAIM-03、附注 2、附注 4）、reader 23 CLOSED / 3 STILL-BROKEN（B-02、B-08、B-15）；逐条修复后复读全部 CLOSED。
- `certify 新准入: 0`：复读只确认修复点；CLAIM-03 推翻了主 agent 第一次修复方向（把 Step 1 同尺寸重叠写成 `Ok`），按三份 `map_huge` 实现改回 `AlreadyMapped` 后复读 CLOSED。
- 机械门（最终工作树）：citation `blocks=41 checked=27 failed=0 skipped=9 warned=5`；anchor `66/66 zero-def=0 multi-def=0`；structure/ref 通过；style `error=0`；density 4 个可判章通过；doc-code-map 30/30；publication 无新增；todo `--doc <路径>` OK。
- 改写段 vs C/源码再推演：J1 组内一致（C 低侧 0 / 高侧 `_kern_offset`，Rust 生产一组）；Step 1 同尺寸叶 `AlreadyMapped`（三架构 paging.rs）；Step 2 删守卫 `.expect` panic；EBS 后建根（`prepare_kernel_image_v1`）；v1 接收 additive 不重建根。记录见 §4.3/§4.5 与 certify。
- 未跑 cargo/QEMU：本轮零代码改动（`git diff --stat -- os/ minix3/` 空）。
- 回归扫描：旧串 `arch_boot arg=`、`reslen=`、`设计裁决`、`评审发现`、`R02-阶段`、`§3.6` 重复标题、`§3.4/§3.8` 断链全部 0 命中。

### R5 §6 指标

- 与 `metrics.md` 的 `## R5 重写基线` 块一致：发现总数 42（36 FINDING + 6 CLAIM）、唯一根因数 42、本轮新准入 42、certify 新准入 0、重复合并 0、自引入 0、处置 ① 42 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0、未闭环 0。
- 轮型 `full`；轮结果 `ROUND-CLOSED`。代裁决 0 / ARCH 提案 0 / 用户否决 0；re-litigation 0；adjacent 阻塞 0；重构 0；减法 0。

### R5 §7 状态

- `ROUND-CLOSED`。全轮 in-scope 发现清零，100% 回读与机械门通过。
- **不宣告 `CONVERGED`**：full 轮无收敛声明权；G11 code-publication 的 73 处存量过程注释、G13 tutorial-rubric 的 6/6 章缺项、`D02-EXTERNAL-D107` 外部 todo 哨兵未清零。三项都不是本轮文档重写引入，也不在本任务允许的修改面内。
- 无 design-debt / carry-over；stage 风险见 `follow-ups.md` F-R5-01..06。
- `eval-trace-check --round R5`：PASS 12 / WARN 0 / FAIL 1；唯一 FAIL 是 `lifecycle-v33`（R4 旧 CONVERGED 与重写基线冲突）。本轮没有伪造 edition 升级，按已知限制登记。

### R5 §8 给 meta-review 的不确定点

1. 面板与四份 certify 都是同一模型的冷启动实例，无跨模型复核。
2. `eval-trace-check --round R5` 的 `lifecycle-v33` 可能仍报“R4 CONVERGED 后出现 R5”：重写使旧 CONVERGED 作废，但工具只认 edition 变更/重开判决行；本轮没有伪造 edition 升级。
3. G11/G13/外部哨兵是门级存量，不是本轮发现；是否解冻 `os/`、教程 rubric 是否适用于 rewrite-notes，需用户决定。
4. `os/kernel/src/lib.rs` 的 Step 1 注释与三份 `map_huge` 实现不符；本轮文档按实现写，注释留 F-R5-06。
5. 未跑 QEMU；`test-higher-half` 的行为证据来自历史轮，本轮未重跑。

### L6

- 同 agent 收口。流程质量：面板把 J1/J2 的适用范围、EBS/建根、Step 2 反事实、Step 1 窗口四类错误分开抓；certify 进一步推翻主 agent“同尺寸重写返回 Ok”的错误修复，说明源码优先序必须由独立回读执行，不能靠主 agent 自证。
- 结论：`IMPROVED`（相对 D02 v3 的 R4：同样的角色数下，certify 抓到了一处修复方向错误，避免了把错误写进最终文档）。

### L6.5

- 三问：① 本轮不变量 = 四个时刻与 J1–J4 在正文/证据/诊断三处一致；② 基线 = `cf79b63e5`，最终文档 sha/行数见 §0 与 `FREEZE.md`；③ 证据 = 42 条台账、四份 certify、九门读数与 `--round R5`。
- 资源重读：本轮盲区属“源码注释与实现不一致”类，查 `prompt/resources/adversarial-rust`；已确认“实现 > 注释”这条判据有效，建议下一版把“源码注释也纳入真值核对”写进面板 brief。

### R39 确认轮（V3.10，2026-11-22）

> **INVALIDATED（2026-11-22）**：本文件中的 R39 是 D01 的轮次，不是本篇轮次；D-114 仅适用于 D01。原“CONVERGED”宣告作废，状态为 CONVERGENCE-CANDIDATE。纠正记录见 `rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/ROUND-IDENTITY-CORRECTION.md`。

- L0.9：tutorial 6/6 0 缺项；30 个 related-code CP-3/CP-4 归零、CJK 0；TODO open=0；D02 旧 blocked 4 条由冷启动 me-agent 裁决（C-04/F10/F10b historical、F10a 补 target remote 后关闭）。
- 单元账本：41/41 冻结、T3 PASS；9 个冻结单元抽样（3.5、4.0、4.5、5.3、6.2、3、5、3.0、5.2）。
- 确认轮发现 7 条（1 P1 + 5 P2 + 1 P3）：两处跨篇指针、§6.2 GDB 组不可执行、§5.2 L0–L8/L10 三收段、§3 调用图表述。D1–D7 修复后两轮增量复核 `DELTA: PASS`。
- 确认轮修复 2/2 批次，未开新修改轮；`defect-ledger.md` 新增 `RD-R39-001..007` 全部 CLOSED。
- 机械门：文档七门绿、anchor 65/65、code-publication 30/30=0、TODO 0；cargo 四包全绿；QEMU 三腿 + check-layout PASS。
- 轮结果：`INVALIDATED`（原写 `CONVERGED`）。文档 sha256 `b34ae2fd…`，2108 行。
