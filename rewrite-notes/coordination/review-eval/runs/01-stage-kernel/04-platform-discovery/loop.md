# D04 循环轨迹 —— 04-platform-discovery

## §0 基线

- 工作流版本：V2（`rewrite-notes/coordination/review-eval/WORKFLOW-VERSIONS.md`）
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
