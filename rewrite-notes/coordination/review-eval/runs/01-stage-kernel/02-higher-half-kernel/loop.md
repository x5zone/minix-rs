# D02 循环轨迹 —— 02-higher-half-kernel

## §0 基线

- 工作流版本：V1（`rewrite-notes/coordination/review-eval/WORKFLOW-VERSIONS.md`）
- schema: V3.4 from R3（V3.5 制品链起约束的首轮；重开依据 D-061。R1/R2 保持各自历史标记，不追溯）
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
- 基线提交：`5ac8bfa8f`（D01 FREEZE 之后）
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
- 基线提交：`73e805b01`（R1 修复与轨迹提交之后）
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

## R3 确认轮（V3.4 四态模型；D-061 授权的重开）

### R3 §0 基线

- 轮型: confirmation
- edition: V3.4
- schema: V3.4 from R3
- 基线提交：`94a254231`（D01 收尾提交；本轮修复未提交）
- 文档：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 文档基线 sha256：`829b3a71d141d99110588e30d33c446939ed97c7b07de57b66bdef45054ae21b`（R2 FREEZE 值，本轮面板启动前）
- 行数：1280（R3 修复后 1283）；最终 sha256 `4f096ea9997eae5aae99ad648967d2b8001256d22e7a63c017d6ea9de16804ce`
- 设计快照：02 组齐备；`design-coverage-check.sh 01-stage-kernel` ALL DOCS COMPLETE
- 关联代码：`doc-code-map --check` 28/28（本轮新增一处 `os/arch/src/riscv64/paging.rs` 引用，改后 29/29）
- 语料条目：D02（`CORPUS.md` 第二节）
- 范围声明：高半区链接布局与成对跳转 + 关联文件 + 本轮修复触碰的文件；不在范围 = 相邻模块与下一批次结构性重构
- 重开依据：D-061

### R3 §1 面板

- 六角色全量、独立子 agent；读者 A/B/C 独立（文档 1280 行 ≥ 500，D-055 墙 1）：
  - P-FACT：8 条 FINDING + 4 SUGGESTION（`fact.md`）
  - P-DESIGN-D2：16 条 FINDING + 2 ARCH-PROPOSAL（`design-d2.md`）；D1 停跑（D-024），基线为「文档主张 ↔ 源码事实」，记 VARIANCE
  - P-READER-A：4 + 19（`reader-a.md`）；P-READER-B：9 + 15（`reader-b.md`）；P-READER-C：4 + 10（`reader-c.md`）
  - P-REFUTE：6 条 CLAIM（阻断 0 / 非阻断 1 / 反证失败 5），`refute.md`
- **VARIANCE 1**：refute 的标题用 `## CLAIM RD3-CLAIM-<NN>`，轨迹门的 `CLAIM <数字>` 正则识别不到，因此 6 条 CLAIM 不计入 INDEX/发现总数/覆盖率（D01 R35 用的是 `## CLAIM <数字>`，本次标注形式不同）。影响：refute 的唯一非阻断发现已并入 reader 侧同根条目（§4.4 PC 行），无处置遗漏。
- **VARIANCE 2**：fact 角色首次派发被平台截断，重派后完成；两次都只读允许材料。
- 制品入库：`panel/round3/` + `INDEX.md`（6 文件：FINDING 41 / SUGGESTION 48 / CLAIM 0）。
- 工作树：面板前后 `git status` 只有本轮要提交的轨迹文件与文档。

### R3 §2 发现

- 89 条机器可计：FINDING 41（fact 8、design-d2 16、reader-a 4、reader-b 9、reader-c 4）+ SUGGESTION 48；另有 refute 6 条 CLAIM（见 §1 VARIANCE 1）。
- 严重度：P0 = 0；P1 = 11（fact 0、design 2、reader-a 2、reader-b 4、reader-c 0、其余为 design P2 与 reader P1/P2 分布）；P2 = 30 左右；P3 若干。逐条见 `panel/round3/`。
- 面板抓到 3 条**本仓文档的真实错误**：§4.4 状态表把 aarch64 direct-core 的跳转后 PC 写成「低地址进入」、§3.1 把 riscv64 测试载体并入「低地址执行视图」、§3.4 的「四条前置条件」与表格行对不上。

### R3 §3 归因

- 34 个根因（RD-R3-01..34，按角色/严重度/处置类聚簇）。
- 冻结决策检查：无命中；设计项的 11 条 P1/P2 按 §3.6 记 ② 延期（D-061）；2 条 ARCH 提案进判决账待裁。
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

- 与 `metrics.md` 的 `## R3 确认轮` 块一致：发现总数 89、唯一根因数 34、本轮新准入 34、certify 新准入 0、处置 ① 12 / ② 11 / ④ 17 / SUGG 49、未闭环 0。
- 轮型 `confirmation`；轮结果 `CONVERGED`。

### R3 §7 状态

- `CONVERGED`。前提：R1 是已关闭的修改轮（ROUND-CLOSED），R3 certify 新准入 0。
- 未闭环 0；`OPEN-DEBT` 的 `R3-design-batch` 有 D-061 判决行与日期。
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
