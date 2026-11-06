# D02 循环轨迹 —— 02-higher-half-kernel

## §0 基线

- 工作流版本：V1（`rewrite-notes/coordination/review-eval/WORKFLOW-VERSIONS.md`）
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
