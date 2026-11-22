# Review 执行流程

> 本文档定义 AI 执行 Review 的强制步骤和工具命令。
> 输出格式见 [review.md §AI Review 输出模板](review.md#ai-review-输出模板)，口诀见 [review.md §快速判断口诀](review.md#快速判断口诀)。

## 快速导航（Quick Nav）

> **AI 长 session 注意力衰减时，优先用此表定位所需章节，避免线性扫描全文（当前 ~2700 行）。**
>
> **⛔ 新增规则元规则（2026-09-05，反膨胀）**：本文件经 20+ 轮迭代已积累 49 个带编号 Step（其中 2/3 为补丁式追加）。
> **今后任何新检查规则，一律以「§检查项注册表」中的一行加入（id / 名称 / 触发时机 / 命令 / 证据要求），禁止新开 Step 编号章节。**
> 历史补丁步骤的正文保留原地不动（导航见注册表）；注册表行与正文的关系是"索引 ↔ 详情"。

| 需要找... | 跳转到 | 行数 |
|----------|--------|------|
| Review 元原则（禁止行为） | [§Review 元原则](#review-元原则) | ~24 行 |
| 执行模式选择（构造/快速/深度/设计优先） | [§〇 执行模式选择](#〇执行模式选择构造--快速--深度--设计优先) | ~252 行 |
| **Step 0-7 强制步骤** | [§一 AI 执行 Review 的强制步骤](#一ai-执行-review-的强制步骤) | ~2271 行 |
| structure.md 12 节骨架模板 | [§Step 0.5 模板](#step-05-生成-structuremd-并评审骨架文档-review-强制) | ~118 行 |
| 6 维反查矩阵 | [§6 维反查矩阵](#step-056-6-维反查矩阵标准化反查方法新增2026-07-16) | ~61 行 |
| Issue 清单（历史问题反查来源） | [§Issue 清单](#step-058-issue-反查来源标注新增2026-07-16) | ~21 行 |
| IN_DESIGN 健康度审计 | [§IN_DESIGN 审计](#五附录-din_design-时间上限--月度审计) | ~59 行 |
| Resume Point（跨 session 续审） | [§Resume Point](#一附录-a2跨-session-续审协议resume-point-标准化) | ~58 行 |
| Review 输出格式 | [§二 Review 输出格式](#二review-输出格式) | ~14 行 |
| Review 工具命令 | [§三 Review 工具命令](#三review-工具命令) | ~42 行 |
| 快速判断口诀 | [§四 快速判断口诀](#四review-快速判断口诀) | ~11 行 |
| 修复阶段工作流 | [§五 修复阶段工作流](#五修复阶段工作流fix-phase) | ~33 行 |
| Blocker Gates 状态表 | [§Step 5.5 收敛判断](#step-55-状态写入与收敛判断) | 搜索 "Blocker Gates" |
| Gate H design 门控 | [§Gate H](#gate-h-design-门控) | 搜索 "Gate H" |

### Step 总览（主干流程）

| 主干 Step | 名称 | Gate 关联 |
|---|---|---|
| Step 0 | 范围声明 + 时间预算 + 状态恢复 | — |
| Step 0.5 | structure.md 生成与骨架评审（0.5.1/0.5.2） | D-6 |
| Step 1 | Ground Truth Lookup（源码定位） | — |
| Step 1.5 | Coverage Enumeration 覆盖率穷举 | A |
| Step 1.6 | 设计对齐检查 | H |
| Step 2 | Diff Extraction 差异提取（Top5 行为契约表） | B |
| Step 2.5 | Link Validation 链路验证 | — |
| Step 3 | Sanity Check 一致性检查 | — |
| Step 3.5 | Precision Check 细节精确检查（3.5a 纵向链路 / 3.5b 因果链抽样） | C |
| Step 3.6 | 关联代码维度检查（文档 review 强制） | code（gate-evidence-code） |
| Step 4 | Cross-Document Check 跨文档联动 | — |
| Step 4.5 | Test Verification 测试章节验证 | E |
| Step 5 | Final Review Output 最终输出 | — |
| Step 5.5 | 状态写入与收敛判断（Blocker Gates 状态表） | 全体 |
| Step 5.6 | Review Verification Protocol 独立验证 | G |
| Step 5.7 | Rule Discovery 规则发现 | — |
| Step 6 | Action Item Generation 修改项生成 | — |
| Step 7 | 自检清单确认 | — |
| Step 7.1 | 收敛成本警告（4 条停止规则） | — |

### 检查项注册表（历史补丁步骤的索引层；正文保留原地，新规则以行加入本表）

| id | 名称 | 触发时机 | 命令 / 证据 | 引入 |
|---|---|---|---|---|
| -0.5 | 工具辅助检查（review 前置） | review 启动前 | review-init.sh / lint-review-rules.sh | 2026-08-15 |
| 0.5.3 | doc ↔ 写作大纲对齐（可选输入；不再是 Gate H 条件） | 文档 review，作者提供了 outline 时 | 偏离矩阵（无 P0 偏离） | 2026-07-16 |
| 0.5.4 | Step 0.5 通过门槛（原 0.5.3 改号让位） | structure/outline 对齐评审后 | 门槛判定（有 P0 不阻塞但标注） | 2026-07-16 |
| 0.5.5 | 跨章节重复内容一致性检查 | 同文档多章节描述同一事 | 跨章节一致性矩阵 | 2026-07-16 |
| 0.5.6 | 6 维反查矩阵（标准化反查方法） | 文档 review | 6/6 反查覆盖率 100% | 2026-07-16 |
| 0.5.7 | 章节意图分析 | 反查中标"多余"的章节 | 强调/解释/对齐/判定 4 项 | 2026-07-16 |
| 0.5.8 | issue 反查来源标注 | scan.md 每条 issue | 来源维度标注（无标注降级） | 2026-07-16 |
| 0.7.1 | Path Existence Validation（RCPD） | 输入含 TODO/路径引用 | `ls`/`test -f` 逐条 | 2026-07-16 |
| 0.7.2 | AI Claim Grep Verification（CFNOC） | AI claim 含概念对象 | `rg` claim 关键词 | 2026-07-16 |
| 0.7.3 | Doc Chapter Context Awareness（DSC） | AI 报告涉 Ch2/Ch4 归属 | 章节上下文核对 | 2026-07-16 |
| 0.7.4 | TODO Staleness Check（CTOS） | 外部 TODO 清单 >5 条且含时间敏感词 | `todo-ledger.py check`（origin_file FAIL / origin_line WARN） | 2026-07-16；2026-11-21 换账本门 |
| 1.0 | 锚点解析（符号锚点 + 卫生项分层；D5 改写收编原 1.0a~1.0g 七步行号族） | doc review，引用代码位置 | `tools/anchor-resolve.sh --check {doc}`；0 定义 → P0-fact；多定义 → 补 impl 限定；行号只允许工具派生 `（Lnnn，工具生成）` | 2026-09-18 |
| 4.5a | 测试数量偏差检查 | doc 声称测试数 | `cargo test` 对账（>50% 偏差 → P1） | 2026-07-30 |
| 3.6 | 关联代码维度检查（doc-code-map + 代码维度 + gate-evidence-code） | full-review 入口、编号文档 | 维度表 + gate-evidence-code 块 | 2026-09-18 |
| 1.1 | 锚点句意复核（按侧对称：`os/` + `minix3/`；区分「符号消失」与「符号选错」） | Step 1.0 | `tools/anchor-suspect-scan.py <doc|dir> --side c|rust [--baseline …]` 出清单（含已知假阳性形态：表格行/块级锚点，由人工判）；基线 `tools/anchor-suspect-baseline-c.txt`（C 侧，2026-10-08 实测 3501 处）与 `tools/anchor-suspect-baseline.txt`（Rust 侧，历史）；错符号按 P0-fact | 2026-10-08 |
| 1.2 | 正文禁引 `.design/` 与 `meta/`（中间产物目录；AGENTS.md 判 P0-process-violation） | doc review 全程 | `rg "(\.design/|meta/)" {doc}` → 0 命中 | 2026-10-08 |
| 1.5a | 覆盖率工具数据缺口判定（映射表缺失 / `--rust-dir` 缺省 / 短名子串） | Step 1.5 | `ls tools/coverage-extract/{module}-semantic-map.json`；缺表即新建 | 2026-10-08 |
| 5.6a | Gate G 两趟（骨架 → 回填 gate-evidence-G → 复跑自检） | Step 5.6 | 首跑 FAIL 属正常，回填后 `VERIFY-SELF PASS` | 2026-10-08 |
| 5.6b | 多 agent 并行 review 的隔离纪律（Round 4–8 取证 R5-D1） | 并行调度时 | 只读仓库 + 各自 `.review/` 子目录 + 禁改 `tools/`/`os/`/`prompt/`；否则证据无版本锚、增量门吃他人改动 | 2026-10-08 |
| 1.5b | `--c-dir`/`MODULE_PATHS` 与真实 C 目录核对（fs 不在 `servers/` 下；模板曾静默回落 kernel） | Step 1.5 | `python3 tools/coverage-extract/coverage-extract.py <module> --print-c-dir`（单一真相源，`MODULE_PATHS` 已补 fs/sched/ipc/input/is/mib/devman）；`review-init.sh` 不再自造映射、解析失败即告警 | 2026-10-08 |
| 1.0a | 「文件 + 第 N 行」式坐标（`` `sem.c` 第 697 行 ``）不在三个锚点工具射程内 | Step 1.0 | `tools/anchor-suspect-scan.py` 已能列出该形态（标 `[卫生项]`）；修法 = 迁移为符号锚点，不阻塞 review | 2026-10-08 |
| 0.9 | **修复的形态覆盖自检**（Round 9–13 串行循环取证：20 条流程缺陷里 12 条是"改一处漏一处"） | 每次改工具/规范后 | 改完用**真实语料全仓 grep 该形态**确认覆盖面（不是只跑手头那一个案例）；`--self-test` 夹具必须含真实形态 | 2026-10-08 |
| 0.10 | 串行 vs 并行的调度纪律 | 多轮 dry-run 调度时 | **广度用并行**（快速铺 stage），**修复收敛必须串行**（每轮带"上一轮修复验证"作业）；并行结构上发现不了"修复未修透" | 2026-10-08 |
| 0.11 | **教学构件承载门 #9**（机制文档分析/实现章无码、诊断章无命令 = P1-docStyle；承载性属正确性非卓越性 grade） | doc review Step 0/交付前；存量只报告、增量阻断 | `tools/doc-code-density-check.sh --check <doc>`（语言感知：ASCII 图/表不算码；承载章按内容锚点识别；降级键=该章自身行数） | 2026-10-08 |
| 0.12 | 文档内部章节引用对账（`§N.M`/`第 N 章` 指向不存在节） | doc review Step 3.5 | `tools/doc-ref-check.sh --check <doc>`（外部 `.md §` 与 CJK 编号自动跳过） | 2026-10-08 |
| 1.5c | **覆盖率语义列填充硬门**（残留 `_待AI补充_` = Gate A 未闭环；修正旧 `Gaps(semantic)` 恒 0 循环假绿） | Gate A 后、AI 逐行补完 | `python3 tools/coverage-extract/coverage-extract.py --check-filled <SYMBOLS.md>` | 2026-10-08 |
| 5.6c | R2 零发现反证探针 | style-fix/code-excellence 判"无需改动"时 | 从 draft/旧版随机抽 3 个内容形态（一个代码块/一段辨析/一张图），逐项问"新文档对应物在哪"；命中缺项 → R2 不得判通过 | 2026-10-08 |
| 5.6d | Gate G 负抽样 | 文档 review Step 5.6 | VERIFY-CHECK 抽样池必须含"未被任何 issue 命中的章节"≥2 节（seed 固定、人工标注池构成），命中新缺陷 → 该轮 DRAFT | 2026-10-08 |
| 0.7.5 | draft 资产收割清单（改写旧文档时防教学形态"无声蒸发"） | 新文档迁移/改写第 1 步 | 四类逐项裁决"继承/不继承+理由"：源码节选块、辨析段、图表、测试与诊断命令；写入 scan.md，"不继承"必须写可复核理由 | 2026-10-08 |
| 0.13 | 受审正文与交付物绑定（防"过了门的版本≠入库的版本"） | doc 交付提交前 | `STATE.md` 记 `受审正文 sha256`；提交前 `sha256sum <doc>` 与记录不一致 → 重跑承载门/文风/anchor 后再提交（人工步骤，工具化待后续） | 2026-10-08 |

> **新增规则写入方式**：在本表追加一行（id 用表内下一序号，不再新开 `#### Step` 章节），详情正文放对应主干 Step 之下或独立小节并用本表行索引。

### Gate 权威注册表（唯一定位表）

| Gate | 语义 | 权威定义位置 | 证据分级 / 通过标准 |
|---|---|---|---|
| **Gate 0** | scan.md 制品完整性（9 个 grep 锚段；预检段必含关联代码清单，B4.2） | prompt/skill/review-agent-ide.md §Gate 0；锚段清单 prompt/skill/review-process-skill.md §Step 0 产物 | L1；缺任一锚段 → FAIL |
| **Gate A** | 覆盖率穷举强制运行 | review-coverage-skill.md §0 + 本文件 Step 1.5 | L1（工具输出必须附 gate-evidence-A）；PARTIAL → FAIL；AI 语义列须填满（`tools/coverage-extract/coverage-extract.py --check-filled <SYMBOLS.md>` exit0，残留 `_待AI补充_` = FAIL） |
| **Gate B** | Top5 行为契约表（8 字段 × 5 函数） | review-core-semantics.md §Gate B + 本文件 Step 2 | L2；3 语义偏移 + 2 覆盖缺口 |
| **Gate C** | 细节精确性（Precision Check） | review-code-checklist.md §15 + 本文件 Step 3.5 | L2；5 meta-rules 表 |
| **Gate D** | P0 必检清单 5 项 | review-patterns.md §0 + 本文件 | L1（每项 grep 证据）；PARTIAL/⚠️ = ❌ |
| **Gate D-6** | structure.md 生成 + 12 节评审 | 本文件 Step 0.5 | L2；失败进 Issue List |
| **Gate D-Impl** | 设计→代码实施验证 | review-implementation-skill.md §Gate D-Impl | L2；Fix Phase / 设计一致性专项 |
| **Gate E** | §5 测试章节 grep 验证 | 本文件 Step 4.5 | L1；doc 无 §5 时 N/A + 原因 |
| **Gate G** | VERIFY-CHECK 独立验证 | 本文件 Step 5.6 | L2；consistency ≥ 90% PASS；CONCERN/FAIL 不得 CONVERGED |
| **Gate H** | 设计语义门控（H.2–H.5；快照存在性不再是门） | 本文件 §Gate H | L2；无 design 可对照时 N/A + 理由；语义不一致按 P0/P1 处理 |

**证据分级与最低级别（V3.10）**：每个 `gate-evidence-{X}` 块正文首行必须写
`level: L1` / `level: L2` / `level: L3`。L1 = 工具输出（命令 + stdout/exit code），
L2 = 手工 grep（命令 + 命中行），L3 = 推断。注册表的最低级别：
A / D / E / G10 / G11 / G12 / G13 必须 L1；B / C / D-6 / G / H 默认 L2。
L3 只允许 B / C / D-6 / H，且必须写 `MANUAL_FALLBACK: <为什么只能推断>`；
其余门写 L3 直接 FAIL。没有 `level:` 行 = FAIL——空口“✅ 通过”不再算证据。


#### scan.md 的机械契约（由 `tools/verify-check.py` 强制，此前只写在脚本里，规范源未提）

`tools/verify-check.py` 是 Gate G 的机械化验证器，它会**因为格式不合而拒绝判定**，因此下列形式要求属于
硬性规范，不是风格偏好：

1. **9 个必备锚段**（标题文字必须逐字一致，`## ` 级）：
   `## Skill Invocation Log`、`## Blocker Gates Status`、`## Step 0: 预检结果`、
   `## Step 1: C Source Ground Truth Lookup`、`## Step 1.5: Coverage Enumeration`、`## Step 2: Diff Extraction`、
   `## Step 3.5: Precision Check`、`## Issue List`、`## Artifact Inventory`
2. **Gate 证据块**必须是围栏块，围栏首行写 ` ```gate-evidence-{X} `（X = A/B/C/D/D-6/E/G/H/G10/G11/G12/G13），
   块内首行写 `level: L1|L2|L3`（最低级别见 Gate 注册表），且含该门的必备关键字：
   A 需 `coverage-extract.py` 与 `Coverage Summary`；B 需 `行为契约`；
   D 需 ≥5 条 `rg`/`grep`；E 需 `fn test_*` 或 `N/A`+理由；G 需 `consistency NN%` 且 ≥90；
   G10/G11 需出版工具的 0 命中/exit 0；G12 需 `open=0`/OK；G13 需 0 缺项。
   （写成 `### gate-evidence-A` 标题不算，验证器只认围栏块。D-Impl 的证据在 Fix Phase /
   设计一致性专项报告，不强制 scan 块。）
3. **Issue List 表格首列必须是 `P[012]-标识` 形式的编号**（如 `P1-docHdrField`），第二列写 `P0/P1/P2`；
   用 `F1`、`#3` 之类编号会让验证器提取到 0 个 issue，反向验证随即空转（抽样表一片空白却报告"已抽样"）。
4. **Gate G 是两趟**：首跑 `verify-check.py` 会因 scan.md 缺 `gate-evidence-G` 块判 `VERIFY-SELF FAIL`——这是设计使然（证据块的内容来自它自己生成的 VERIFY-CHECK）。先出骨架、回填 `gate-evidence-G`、再复跑自检即 PASS。
5. **Unit Ledger 段（doc 模式）**：scan.md 必须含 `## Unit Ledger`，内容为
   `tools/review-units.py stats <doc>` 的输出与本轮 T1/T2 单元列表；它不是 Gate G 的 9 个锚段，
   但 G14 以状态文件为准。
6. **Gate G 产物由该脚本生成后再填人工段**，不要手写整份 VERIFY-CHECK：
   `python3 tools/verify-check.py --scan {scan.md} --state {STATE.md} --symbols {SYMBOLS.md} --units {units.yaml} --sample-ratio 0.2 --seed N --output {VERIFY-CHECK.md}`
   （整篇 survey 仍用 0.5；单元模式用 0.2 + `--units`）
   （`--seed` 必填以让抽样可复现；报告头会如实打印抽样比例与种子。）


---

## Review 元原则

> **先穷举，再组织；先本质，再实现；先大纲，再正文；先自检，再 review。**

这五句口号是 Minix-RS 项目的核心方法论，review 流程必须遵循：

| 口号 | review 阶段对应 | 强制要求 |
|------|----------------|---------|
| 先穷举 | Step 1.5 覆盖率穷举 | 必须用 `tools/coverage-extract/coverage-extract.py` |
| 再组织 | Step 1.6 design 对齐 | design.md 必查（Step 0 预检）|
| 先本质 | Step 0.5 structure 评审 | Ch1 必须有灵魂本质 |
| 再实现 | Step 2 差异提取 | design 优先于 code |
| 先大纲 | Step 0.5 structure 评审 | outline 必出 |
| 再正文 | Step 4 review 修复 | 文档/代码必改 |
| 先自检 | Step 5.6 VERIFY-CHECK | Blocker Gates 全过 |
| 再 review | 下一轮 review | 必须 CONVERGED |

**违反执行口号的判定**：
- ❌ 跳过 Step 1.5 直接 review → P0 流程违规
- ❌ 未生成 structure.md 直接 review → P0 流程违规
- ❌ VERIFY-CHECK 未通过就标 CONVERGED → P0 流程违规

---

## 〇、执行模式选择（构造 / 快速 / 深度 / 设计优先）

> **目的**：根据任务规模和精度要求，选择合适的执行模式。不同模式裁剪不同的 Step，平衡覆盖度与效率。

| 模式 | 适用场景 | 执行 Step | 预计耗时 | 对应 Profile |
|------|---------|----------|---------|-------------|
| **构造模式（Constructive）** | 文档/代码初稿阶段，需要引导作者补全 | Step 0, 1, 1.5, 2, 5, 6 | 15~30 分钟 | A / B / G |
| **快速模式（Quick）** | 日常 PR 审阅、时间有限的扫描 | Step 0, 1, 2, 5 | 10~20 分钟 | D |
| **深度模式（Deep）** | 里程碑验收、关键模块完整 Review | Step 0-7（全量） | 40~120 分钟 | C / H→I→J→K / O / P |

> **2026-08-15 修复 A-P1-4（明确 Profile 关系）**：
> - **Profile C** = 单次深度（**单 session**）：500-1500 行文档分 2 rounds 内部裁剪（R1 正确性 + R2 卓越性），session 内完成
> - **Profile H/I/J/K** = 分阶段深度（**多 session**）：> 1500 行文档分 4 rounds 跨多 session 续审，每 session 仅做 1-2 步
> - **Profile O / P** = 深度变体：O = 正确性已过追求卓越性；P = 覆盖率验收
> - **互斥关系**：同一文档同一时刻只能选 1 个 Profile；C 与 H/I/J/K 互斥，O 与 P 互斥
| **设计优先模式（Design-First）** | design 错误（design-wrong，存在但抓错本质） | Step 0 + 修 design + Step 1.6 + 2 | 30~60 分钟 | R |

**深度模式 rounds 动态化**（新增，2026-07-16）：根据文档行数决定分阶段轮次，避免小文档过度分阶段、大文档一轮过载。

| 文档行数 | rounds 数 | 每 round 范围 | 理由 |
|---------|----------|--------------|------|
| < 500 行 | 1 round（全量） | Step 0-7 一轮完成 | 小文档单轮可完成，无需分阶段 |
| 500-1500 行 | 2 rounds | R1: 正确性（Step 0-4 + Gate 0/A/B/C/D/D-6/E/H）<br>R2: 卓越性（Step 5 + Gate G + patterns/excellence） | 中等文档分两轮：先保正确性，再求卓越 |
| > 1500 行 | 4 rounds | R1: 正确性（Step 0-4）<br>R2: 卓越性（Step 5 + excellence）<br>R3: patterns 对照<br>R4: 跨文档 + Gate G 收敛 | 大文档需 4 轮，避免单轮 context 过载 |

**判定规则**：默认按行数查表；用户明确要求"深度全面 full-review"时按 2 rounds 起步（不强制 4 rounds），避免过度分阶段。

**工具协助**（NEW 2026-07-16）：用 `tools/review-init.sh {tool} {doc-path} {agent} --size-adaptive` 自动统计文档行数 + 输出推荐 rounds。每个 session 启动时建议默认加 `--size-adaptive`。产物齐全性用
`python3 tools/verify-check.py --preflight --tool {tool} --stage {stage} --doc-stem {doc-stem} --agent {agent}`
一次检查（7 组产物 + B4.1 关联代码清单/gate-evidence-code + D4 artifact 存在性）。

> **2026-08-15 修复 B-P1-4（超大规模文档策略）**：> 3000 行的超大规模文档按以下策略处理：
> - **优先分章节拆解**：单章 ≤ 1500 行时按章节独立 review，最后合并
> - **多 reviewer 分块**：用户显式批准后，2-3 个 reviewer 并行 review 不同章节，最后由 1 人统一 CONVERGED 判定
> - **滚动 review**：按"Ch1&2 → Ch3 → Ch4 → Ch5" 顺序滚动，每步必须 CONVERGED 后才进入下一步
> - **不推荐**：单 reviewer 单 session 完整 review > 3000 行文档（上下文超载 + 注意力衰减）
> - **判定信号**：scan.md 中断时"完成度 < 50%" → 提示用户分章节或加 reviewer

### 设计优先模式（Design-First）

> **定位**：只处理“design 存在但抓错本质”（design-wrong），或 review 中需要先把设计决策修正再改代码的场景。没有 design 文档不是触发条件——H.2–H.5 按“无 design 可对照”记录 N/A + 理由，不补生成。

**触发条件**：
- review 中发现 P0-design-wrong（design 抓错本质）→ 触发 Design-First + design Refactor
- Step 1.6.2 一致性 < 80% 且判断为 design 侧错误 → 触发 Design-First + design Refactor
- 用户明确指定“先看 design”

**执行 Step**：
1. **Step 0**：声明 design 状态（存在且错误/可改进；不存在则记 N/A）。
2. **修 design**：以 C 源码与当前 Rust 代码为准，直接修正错误的设计决策；需要留档时写入 `meta/design/{stage}/{NN}-design.v{N+1}.md`，不要求从脚手架重新生成。
3. **Step 1.6**：design ↔ code 一致性检查。
4. **Step 2**：design vs Minix3 本质对比。

**跳过**：Step 0.5（structure.md 评审，design 修复阶段不评审文档骨架）/ Step 1 / 1.5 / 2.5 / 3 / 3.5 / 4 / 4.5 / 6 / 7
**输出重点**：design 缺陷清单 + Refactor 触发条件
**配套 Gate**：Gate H 的 H.2–H.5（语义一致性；不看快照是否存在）

**与日常 review 的关系**：
- design 修复后，恢复日常 review 模式
- 不替代日常 review，而是日常 review 的前置阶段
- **见** [review.md §Design First 原则](review.md#design-first-原则rust-重写场景)

**重要术语映射**：

| 历史用语 | 当前规范用语 | 含义 |
|---------|-------------|------|
| 工作流 Step 2.1 章 "Redesign" | **design Refactor** | design 自身不抓本质/漏概念，先修 design 再修 code |
| 历史评论 "redesign"（修复偏离）| **code Refactor** | design 已规定但实现偏离，修复 code 回到 design |
| §通用禁令（"旧版/最初/后来"）| **禁止叙事** | 仍适用于 design Refactor 的产出（outline 不应使用迭代史） |

**review 修复 = 基于 outline 的部分重写**：
- review 发现文档问题后，修复不是打补丁，而是回到 outline 阶段重新组织
- 重大修复（Ch1/Ch3 重写）必须先更新写作 outline，再重写正文
- 轻微修复（错别字/行号）可直接打补丁，无需走 outline

**自动触发条件**：
- Step 1.6.2 一致性 < 80% 且判断为 design 侧错误 → 触发 Design-First + design Refactor
- Step 1.6.3 出现 P0-design-wrong → 触发 Design-First + design Refactor

**手动触发条件**（AI 主动判断）：
- AI 在 review 中识别到“开发文档味”严重 → 提示用户是否切 Design-First
- AI 在 Step 2 发现 design 未覆盖的 Minix3 概念 → 提示用户
- AI 在 Step 3.5 发现纵向链路断 → 提示用户

---

### §〇.临时文档规则（含 bak / tmp_design_and_todo / /tmp/）

> 来源：2026-07-16 工作流 bug 修复
> **扩展理由**：原规则只禁 `*.bak`，未覆盖 `tmp_design_and_todo/`（该目录已删除，下同）和 `/tmp/` 实施稿，导致 AI 违规用临时讨论稿当 design 依据。
> **状态（2026-09-05 清淤）**：`tmp_design_and_todo/` 目录**已物理删除**。本节规则保留两个目的：a) **历史案例记录**（下文 Session 案例中的路径均指向已删除目录，仅作教学引用）；b) **通用守卫**——任何未来的临时目录/未定稿产物（`tmp_*`、`draft/` 中未定稿文件、`/tmp/` 实施稿）同等禁止作为 design 或权威依据。

**临时文档处置策略**：

| 临时文档类型 | 处置策略 | review 中是否可读 | 作为 design 依据 |
|-------------|---------|-----------------|----------------|
| `*.bak` 旧版文档 | 仅作历史参考，**不进入 review 流程** | ❌ 禁止 | ❌ 禁止 |
| `*.bak` 旧版 design | 已被新 design 替代，对比意义 | ⚠️ 仅在 §1.6 设计演进说明中引用 | ❌ 禁止 |
| `*.bak` 旧版代码 | 不应保留在主分支 | ❌ 应删除或归档到 history/ | — |
| `tmp_design_and_todo/` 下任何文件 | 临时讨论池，**非定稿** | ⚠️ 仅作背景理解，不作权威源 | ❌ **禁止**（P0-process-violation）|
| `/tmp/` 下任何文件（含 `improve_*` 实施稿）| 实施稿，未走 design 流程 | ⚠️ 仅作背景理解，不作权威源 | ❌ **禁止**（P0-process-violation）|
| 现有文档 §3 内嵌设计章节 | 被审对象的一部分，**非独立 design** | ✅ 可读（作为被审对象） | ❌ **禁止**作为 design.md 来源（循环论证：用被审者的自述作为审他的依据）|
| 无 `{NN}-` 前缀的 `design.md` / `kboot-design.md` | 命名不规范 | ⚠️ 仅作背景理解 | ❌ **禁止**（必须有 `{NN}-` 前缀）|

**强制要求**：
- ❌ Review 中不得引用 `*.bak` / `tmp_design_and_todo/` / `/tmp/` 文件作为权威来源
- ❌ 新生成的 design 不得基于 `*.bak` design 演化
- ❌ design.md 不得引用 `tmp_design_and_todo/` 或 `/tmp/` 文件
- ⚠️ 如需对比新/旧 design，使用 `git diff` 而非读 `.bak`
- ✅ `/tmp/` 下的实施稿（如 `improve-design-m3.md`）可作为 design Refactor 的**输入参考**（用户显式提供时），但不能直接作为 design.md

**判定信号**：
- review 输出含 `*.bak` 路径引用 → P1-process-violation
- review 输出含 `tmp_design_and_todo/` 路径引用作为 design 依据 → **P0-process-violation**
- review 输出含 `/tmp/` 路径引用作为 design 依据 → **P0-process-violation**
- design.md 内容与 `*.bak` 内容高度相似 → P0-design-no-evolution（未演进）

**grep 自动检测**：
```bash
# 检测 review 引用临时文档（{DOC_DIR} 为被审文档所在目录）
grep -rnE "\.bak|tmp_design_and_todo|/tmp/" {DOC_DIR}/scan.md {DOC_DIR}/design.md {DOC_DIR}/design-final.md
# 期望：无输出（或仅作为背景引用，非权威源）
```

### 构造模式（Constructive）

> **定位**：初稿阶段的"脚手架 Review"。不追求发现所有问题，而是帮作者建立完整骨架。
> **特点**：以覆盖率穷举和差异提取为核心，输出"缺什么"而非"哪里错"。

**执行 Step**：
1. **Step 0**：范围声明 + 时间预算
2. **Step 1**：源码定位（验证引用文件存在）
3. **Step 1.5**：覆盖率穷举（生成 SYMBOLS.md，标记缺口）
4. **Step 2**：差异提取（Top 3 语义偏差）
5. **Step 5**：输出（聚焦缺口清单 + 补全建议）
6. **Step 6**：修改项（P0 缺口必须生成补全项）

**跳过**：Step 2.5（链路验证，初稿可能链路未建）、Step 3（逐行精确验证）、Step 3.5（细节精确）、Step 4（跨文档，初稿可能未引用）

**输出重点**：
- 覆盖率缺口清单（哪些 C 函数/结构体/宏未覆盖）
- Top 3 语义偏差（方向性错误）
- 补全建议（下一步应该写什么）

### 快速模式（Quick）

> **定位**：日常 PR 审阅的"烟雾测试"。快速发现明显 P0，不做穷举。
> **特点**：用口诀扫描 + 差异提取，仅输出 P0。

**执行 Step**：
1. **Step 0**：范围声明 + 时间预算
2. **Step 1**：源码定位（仅验证关键引用）
3. **Step 2**：差异提取（Top 3）
4. **Step 5**：输出（仅 P0 问题）

**跳过**：Step 1.5（覆盖率穷举）、Step 2.5（链路验证）、Step 3（逐行验证）、Step 3.5（细节精确）、Step 4（跨文档）、Step 6（修改项，P0 在输出中直接说明）

**输出重点**：
- Top 3 语义偏差
- P0 问题清单（仅 P0，P1/P2 不输出）
- "建议升级到深度模式"的提示（如发现 P0 数量 > 3）

### 深度模式（Deep）

> **定位**：里程碑验收的"全量 Review"。执行所有 Step，穷举所有维度。
> **特点**：每个 Step 产生完整中间产物，覆盖正确性 + 卓越性 + 覆盖率。

**执行 Step**：Step 0-7 全量执行（见下方"一、AI 执行 Review 的强制步骤"）

**输出重点**：
- 全部中间产物（10 个维度的表格）
- P0/P1/P2 完整问题清单
- 修改项（P0 必须有代码修改项）
- 自检清单确认

**模式选择决策树**：
```
任务来了
  ├─ 是初稿？ → 构造模式
  ├─ 是日常 PR / 时间紧？ → 快速模式
  │    └─ 发现 P0 > 3？ → 建议升级深度模式
  └─ 是里程碑 / 关键模块？ → 深度模式
       ├─ 文档 < 500 行 → Profile C（单次深度）
       ├─ 文档 500-1500 行 → Profile C（单次深度，2 rounds 内部裁剪）
       ├─ 文档 > 1500 行 → Profile H→I→J→K（分阶段深度）
       ├─ 正确性已过，追求卓越 → Profile O（深度+卓越性）
       └─ 覆盖率验收 → Profile P（深度+覆盖率穷举）
```

> **阈值对齐说明（2026-08-15 统一）**：决策树档位与 rounds 表（<500 / 500-1500 / >1500）保持一致；500-1500 行文档虽然分 2 rounds（R1 正确性 + R2 卓越性），但**单 session 完成**（属 Profile C），不需要跨 session。仅当文档 > 1500 行时才需要 Profile H/I/J/K 分阶段（多 session）。

---

## 一、AI 执行 Review 的强制步骤

> 为防止 AI 在 Review 过程中"浮躁"或遗漏关键验证，必须按以下步骤执行。
> **关键原则**：每个 Step 必须产生**可见的中间产物**（表格、列表、grep 输出）。不允许"在脑子里过一遍"然后跳到最终输出。

### Step -0.5: 工具辅助检查（review 前置，2026-08-15 修复 B-P1-6）

> **目的**：复用 `cargo` 生态工具的检查结果，避免 review 与 lint 结果矛盾。
> **执行步骤**（仅当 review 涉及 Rust 代码时执行）：
> 1. `cargo check` — 编译检查，确保无新 error（warning 不阻断 review）
> 2. `cargo clippy -- -W clippy::all` — lint 检查，记录 clippy 警告列表（**作为 review Step 4 的输入**，但不替代 review）
> 3. `cargo fmt --check` — 格式检查（仅当 review 关注代码风格时）
> 4. `cargo test` — 运行测试，记录失败的测试（**作为 review Step 4.2 测试覆盖度的输入**）
> **判定**：
> - `cargo check` 失败 → 阻断 review（先修编译错误）
> - `cargo clippy` 警告作为 P2 候选（review 决定是否升级）
> - **不允许**用 `cargo clippy` 替代 review 的人工判断

### Step 0: 范围声明 + 时间预算 + 状态恢复 + **design 预检**

- 按 [§〇 执行模式选择](#〇执行模式选择构造--快速--深度--设计优先) 确定执行模式（构造/快速/深度/设计优先）
- 按 [review.md §Review 启动：范围声明](review.md) 声明 Review 模式和范围
- **单元范围（doc 模式）**：`tools/review-units.py check <doc>` 通过后，用 `next` 声明本轮
  T1/T2 单元；首 edition 的 full survey 仍读整篇（`review-units.md` §五）
- 声明时间预算（可选；按 [review.md §时间预算参考](review.md#时间预算参考)）
  - **分阶段时间预算**（新增，2026-07-16）：若执行 Step 0.7 TODO 验证，时间预算分两阶段声明：
    - **TODO 验证阶段**：按 TODO 数 × 5 分钟预估（含 grep 验证 + 误报否定 + 真实修复）
    - **review 阶段**：按文档行数查表（<500 行→15-30 min | 500-1000→30-60 | 1000-1500→60-90 | >1500→90-120）
    - 实际耗时与预估对比写入 scan.md，偏差 >50% 需说明原因（避免偷懒）
- **读取状态（按运行时隔离，互不共享中间结果）**：状态根目录是 `.review/{tool}/{stage}/`，
  `{tool}` 只能取下表登记的运行时名。**只读写自己那一行**，不要替别的运行时写产物。
  本表在派生进各运行时副本时**不做运行时身份替换**——每个会话按自己是什么工具去对号，
  这样规范源与三端副本描述的是同一个事实（此前源里只列 Trae 与 Claude 两家，而 `.review/` 下
  实际已有五家运行时目录，Codex 只存在于派生副本的措辞替换里）。


| 运行时 | `{tool}` | 状态根目录 | 产物布局 |
|---|---|---|---|
| Trae IDE | `trae` | `.review/trae/` | 多 agent 袋装：`{stage}/scans/{doc-stem}-{agent}-scan.md` |
| Claude Code | `claude` | `.review/claude/` | 单 session：`{stage}/{doc-stem}/scan.md` |
| Codex CLI | `codex` | `.review/codex/` | 同 Claude（无 agent 后缀） |
| ZCode | `zcode` | `.review/zcode/` | 同上 |
| Qoder | `qoder` | `.review/qoder/` | 同上 |

**接入新运行时的做法** = 往这张表加一行 + 沿用同一产物布局，不要另造一套状态结构。
`.review/` 下另有按旧模块名命名的目录（如 `03-stage-kernel/`、`pm/`、`vm/`），那是目录迁移前的
历史冻结区，不属于任何运行时，本表的读写规则不适用。
  - 各运行时各自维护独立 STATE.md，**绝不共享任何中间结果**（STATE/scan/SYMBOLS/structure/VERIFY-CHECK）。Bagging 聚合只发生在 Trae 内（多 AI 的 scan 聚合）。
  - 若同一工具下两份 STATE.md 同时存在且内容矛盾，**不要自动合并**，在 scan.md 中记录分歧并询问用户哪个为准。
  - **`{stage}` 的确定**：取目标文档所在路径里笔记树根（`rewrite-notes/`、`redesign-notes/`、`study-notes/`）下的**第一级目录名**。例如 `rewrite-notes/01-stage-kernel/03-kmain-cstart.md` → `{stage}=01-stage-kernel`。它同时是评审状态目录的分组键（`.review/{tool}/{stage}/`）。旧布局在树根与阶段之间还有一个模块层目录（当时取值恒为 `fork-syscall-rewrite`），该层已于 2026-10-07 目录迁移退役，旧→新对照见 `rewrite-notes/MIGRATION.md`。这与覆盖率脚本 `--module kernel`（Minix3 模块名）是**两个不同概念**，不得混用。
  - **STATE 预检**：Step 0 启动时运行 `tools/review-state-validate.py --state {state_path}` 校验 STATE 引用的文件是否存在、Open 列表条目能否在 scan.md 中找到对应条目。预检失败 → 在 scan.md 标注并先修复再继续。
  - 推荐用 `tools/review-init.sh {tool} {doc-path}` 自动计算 `{tree}`/`{stage}`/`{doc-stem}` 并 mkdir 标准目录。
- **设计快照是可选的写作输入，不是 review 门**（V3.10）：`meta/design/{stage}/{NN}-design*.v*.md` 与 `{NN}-outline*.v*.md` 只在大纲/初稿写作阶段作为参考；review 启动不再要求 `ls` 四条快照、不再做覆盖检查，也不因缺失生成快照。Gate H 只检查 H.2–H.5 的语义一致性（design ↔ code / Minix3 / Refactor 区分），不检查快照存在性。没有 design 文档时，H.2–H.5 按“无 design 可对照”记录 N/A + 理由，而不是 FAIL 或补生成。
- **旧快照边界**：历史快照可以读，但不得当 ground truth；以 C 源码与当前 Rust 代码为准。旧快照里的过时结论按普通发现处理，不因“快照如此”豁免。

### Step 0.5: 生成 structure.md 并评审骨架（文档 Review 强制）

> **单元模式复用（V3.9）**：`review-units.py fingerprint` 报 `structure: same` 时，复用上轮
> structure.md 并在 scan.md 记 `structure_reused: <commit>`；只对脏章重新评审。`code: same` 时
> Gate A 复用上轮 SYMBOLS.md，记 `coverage_reused`。两者都必须有 fingerprint 输出作证据。

> **核心原则**：reviewer 必须先提取文档骨架并评审，再执行正确性检查。
> 正确性检查验证"文档说了什么"，structure.md 验证"读者读到了什么"。两者正交。
> 来源：03-kmain-cstart

**Step 0.5.1 生成 structure.md**（按以下模板，概念文档全量 12 节，实现文档简化为 6 节）：

> **模板不是正文排布令（V3.12）**：本模板的「主题思想 / 不变量 / 编号系统」是作者组织内容与
> 评审者检查完整性的全局信息，**不代表它们必须集中呈现在正式章节开头**。正式正文按读者理解
> 机制的顺序逐步引入：先说明问题，再在需要时引入概念、不变量和编号。作者掌握的全局信息，
> 不等于读者应该一次性接收的全部信息。

```markdown
# structure.md — {文档名} 结构分析

## 1. 主题思想（一句话）
> 例：prot_init() 给 CPU 配置回答"特权级/异常入口/内核栈"三问的数据结构
> 说不出 → P1（读者无法建立概念模型）

## 2. 目标读者
> 初学者/中级/高级 + 前置知识
> 未声明 → P1（无法验证教学性是否匹配读者）

## 3. 叙事主语
> prot_init() / CPU / 读者 / OS —— 选一个，附证据（Ch1 开篇第一句主语）
> 主语是函数名 → P1（实现驱动）

## 4. 驱动方向
> 概念驱动 / 实现驱动 / 混合 —— 附判定证据
> 实现驱动 → P1（模式 51）

## 5. 文档大纲（每章核心命题）
| 章节 | 核心命题（一句话） | 回答的问题 | 叙事弧角色 |
|------|------------------|-----------|-----------|
| Ch1 | 保护结构是 CPU 三问的答案 | WHAT/WHY | 起(问题) |
| Ch2 | Minix3 C 如何回答三问 | HOW(C) | 承(证据) |
| Ch3 | Rust trait 如何抽象三问 | HOW(抽象) | 转(设计) |
| Ch4 | 具体实现如何落地 | HOW(代码) | 合(落地) |
| Ch5 | 测试如何验证 | VERIFY | 合(验证) |
> 叙事弧缺环节 → P1；章节核心命题说不出 → P1

## 6. 核心概念清单（按引入顺序）
| 顺序 | 概念 | 来源层级(L0/L1/L2) | 依赖的概念 | 依赖已引入? | 文字行数 | 占比 |
|------|------|-------------------|-----------|-----------|---------|------|
| 1 | 保护 | L1 | 无 | — | 15 | 10% |
| 2 | 特权级 | L1 | 保护 | ✅ | 20 | 13% |
| 3 | GDT | L0 | 特权级 | ✅ | 40 | 27% |
> 依赖倒置 → P1；L2 术语作概念定义 → P1；
> 历史包袱占比 > 核心概念 → P2
> 本表是**引入顺序的组织图**，不是“章首集中定义所有概念”的要求；概念应在读者需要它
> 理解当前问题时引入（`review-tutorial-rubric.md` TR-0/TR-2）。

## 7. 跨架构统一抽象（多架构文档）
| CPU 问题 | x86 | aarch64 | riscv64 | 文档位置 |
|---------|-----|--------|--------|---------|
| 当前特权级 | ring | EL | mode | §1.2 |
| 异常入口 | IDT | VBAR | stvec | §1.4 |
| 内核栈 | TSS.sp0 | SP_EL1 | sscratch | §1.5 |
> 无统一抽象层 → P1（模式 53）

## 8. 双向闭环完整性
| 机制 | 进入 | 返回 | 完整? |
|------|------|------|-------|
| 特权级切换 | user→kernel ✅ | kernel→user ✅/❌ | |
> 只单向 → P1（模式 52）

## 9. 起承转合（叙事弧）
> 起(问题) → 承(概念) → 转(证据/抽象) → 合(落地/验证)
> 缺失环节标注 ❌ → P1

## 10. 元注释位置标记
| 位置 | 类型(六类之一) | 判定 |
|------|--------------|------|
| §1.2 L39 | 过渡类 | 删除 |
| §1.4 L106 | 深度策略类 | 删除 |
> >5 处 → P1（模式 49）

## 11. 读者复述测试（必答产物，2026-09-18 H1 提级）
> 审阅者**自己写**一段"不看代码、不用函数名，用机制语言复述 Ch1"的结论（一页以内），
> 直接写进本节（不是把 Ch1 抄一遍）。判断标准 = 能否说出
> "这个机制解决什么矛盾、状态怎么转、边界在哪"。允许标注"复述在 X 处卡住"（卡点即教学缺陷线索）。
> 复述不通过 → **教学类 P1**：新增/触碰章节必须修复或写明延期理由；存量章节登记 TODO 账本（kind: doc）；
> 复述结论与卡点在 scan.md §structure.md 评审与 Issue List 显著位置单列。

## 12. 纵向链路映射
| Ch1 概念 | Ch2 C 函数 | Ch3 Rust trait | Ch4 实现 | Ch5 测试 |
|---------|-----------|---------------|---------|---------|
| 特权级 | tss_init() | ProtectionArch | X8664Protection | test_privilege |
| 异常入口 | idt_init() | TrapEntryArch | X8664TrapEntry | test_trap_entry |
> 缺映射 → P2（要求补映射表）
```

**Step 0.5.2 评审 structure.md**（逐项判定，失败项写入 scan.md §structure.md 评审）：
- 1-12 节每节判定 ✅/P0/P1/P2 + 证据
- 失败项汇总为 Issue List 的 P0/P1/P2 条目
- **读者复述测试（§11）是必答产物**（2026-09-18 H1 提级）：复述不通过 → 教学类 P1，在 scan.md §structure.md 评审与 Issue List 显著位置**单列**（新增/触碰章节必须修复或写明延期理由，存量登记）
- **元注释章节 review**（NEW 2026-07-31）：同时主动验证文档中的元注释章节（H2/H3 标题含"已知"/"修订"/"元注释"/"自审"/"修复记录"等关键词），验证章节声称的文件/函数/行号引用 → 错误按 Pattern #66/#73/#75 记录到 Issue List（首次发现：04-platform-discovery review 2026-07-31）

**Step 0.5.3 doc ↔ outline 对齐检查**（方案 D 新增，仅当 outline.md 存在时执行）：

> **目的**：对照持久化的 outline.md（文档结构契约），检查文档正文是否遵循大纲。发现三种偏离：遗漏 / 多余 / 顺序错位。
> **前提**：作者提供 outline 时才执行；没有 outline 时本步记 N/A，不要求补生成，也不影响 Gate H。

**检查方法**：
1. 读取 outline.md 每小节的"讲什么 + 知识点 + 教学要点"
2. 读取文档正文对应章节
3. 逐小节对照，输出偏离矩阵

**偏离矩阵格式**（必须输出）：

| outline 小节 | outline 规定的知识点 | 文档实际覆盖? | 偏离类型 | 严重度 | 评估 |
|-------------|---------------------|-------------|---------|--------|------|
| §1.1 CPU 上电后状态 | UEFI/OpenSBI 退出启动服务、三架构统一 | ✅ 完整 | — | — | — |
| §1.2 boot-shim 职责 | 6 步序列、GRUB 职责转移 | ⚠️ 缺"GRUB 职责转移" | 遗漏 | P1 | 坏偏离：核心概念缺失 |
| §3.5 bootstrap 淘汰 | 选项 B 决策、64 位无 unpaged 段 | ⚠️ 文档写了"保留字段" | 多余 | P2 | 好偏离：实现偏离已加注 |
| §2.3 页表机制 | 4MB 大页、4KB 小页、PG_ALLOCATEME | ✅ 完整 | — | — | — |

**偏离类型定义**：
- **遗漏**：outline 规定的知识点，文档没写 → 默认坏偏离（覆盖缺口），除非该知识点已过时
- **多余**：outline 没规定，文档写了 → 需评估（合理补充 = 好偏离；画蛇添足 = 坏偏离）
- **顺序错位**：文档顺序与 outline 不一致 → 需评估（合理调整 = 好偏离；破坏叙事弧 = 坏偏离）

**严重度判定**：
- P0：outline 规定的核心概念（Ch1/Ch3 灵魂本质）遗漏 → P0-coverage-gap
- P1：outline 规定的非核心知识点遗漏，或坏偏离的多余内容
- P2：好偏离的多余内容（合理补充），或轻微顺序错位

### ⛔ 反查原则：不必然导向统一结果，可包含 Open Questions（2026-07-16）

> **核心原则**：**review 的最终产出不一定非得是一个确定的结果，也可以包含待讨论项（Open Question）**。
> 适用所有反查维度（outline ↔ 文档、design ↔ code、doc ↔ code、跨快照 diff 等）。

**三类判定（替代"二值 P0/P1/P2 判定"）**：

| 判定类型 | 含义 | 严重度标注 | 处理方式 |
|---------|------|----------|---------|
| **🔴 直接判定** | AI 能直接判定哪个更好（依据 C 源码语义 / OS 理论 / 类型系统 / 架构原则） | P0/P1/P2 | AI 给出推荐 + 理由，写入 scan.md |
| **🟡 Open Question** | AI 无法直接判定哪个更好（涉及设计权衡、命名美学、风格选择） | ❓ OQ-N（编号） | 写入 scan.md `§Open Questions` 段，**上交用户决定**，不阻塞 review 流程 |
| **🟢 共识一致** | 反查双方无差异 | — | 写入 scan.md `§反查一致项` 段，无需处理 |

**Open Question 格式**：
```markdown
| OQ ID | 反查维度 | 两侧方案 | 各自优劣 | AI 倾向（可选） |
|-------|---------|---------|---------|----------------|
| OQ-1 | design ↔ code | design 用 `trait Foo`，code 用 `struct Foo` | trait 更可测 / struct 更简单 | 倾向 trait（trait 可 mock，便于测试）|
| OQ-2 | outline ↔ doc | doc §3.5 写 `mov sp, x0`，code 写 `mov sp, {stktop}` | doc 简化 / code 占位 | —（需用户判定文档意图） |
```

**Open Question 收敛机制**：
- 用户回复决定后，OQ 转为 issue（P0/P1/P2）进入修复流程
- 累计 ≥3 轮未决 OQ → 在 STATE.md 标记"决策阻塞"，提示集中处理
- 同一 OQ 跨多轮 review 仍开放 → 升级为"设计争议"，建议开会讨论或写 RFC

**反"无脑一致"原则**：
- ❌ 禁止"design ↔ Rust 代码不一致 → 直接判 P0 改 code"——必须先判断哪个更好
- ❌ 禁止"doc 和 outline-review.md 不一致 → 直接判 P0 改 doc"——必须先判断哪个更好
- ✅ 若 AI 能判定哪个更好 → 给出推荐 + 理由 + 严重度
- ✅ 若 AI 无法判定 → 标 OQ，上交用户
- ✅ review 报告同时含 issue 清单（确定项）和 OQ 清单（待决项），两者独立

**示例**（02 文档 §3.5 寄存器名）：
- ❌ 错误做法：发现 §3.5 与 code 不一致 → 直接判 P0
- ✅ 正确做法：先判断 §3.5 `mov sp, x0` 和 code `mov sp, {stktop}` 谁更准确 → 基于 inline asm 真实语法判定 code 对，doc 错 → P1 改 doc
- ✅ 正确做法：若两侧各有合理性（如 doc 想表达"先加载到寄存器"、code 表达"直接占位"）→ 标 OQ，交用户决定

**通过门槛**：
- 无 P0 偏离 → 通过
- 有 P0 偏离 → 标注"outline 对齐 P0"，建议先修文档或更新 outline 再继续
- outline 与文档严重偏离（>30% 小节偏离）→ 建议**同步更新 outline.md**（文档演进后 outline 过时），更新后重新走 outline-review 批准

**Step 0.5.4 通过门槛**（原 Step 0.5.3）：
- structure.md 评审通过（无 P0）+ outline 对齐通过（无 P0）后才进入 Step 1（覆盖率穷举）
- 若 structure.md 有 P0 → 在 scan.md 标注"骨架层 P0，建议先修骨架再继续"，但**不阻塞**后续 Step（用户可能希望一次性看到所有问题）
- 若 outline 对齐有 P0 → 在 scan.md 标注"outline 对齐 P0，建议修文档或更新 outline"，**不阻塞**后续 Step

**Step 0.5.5 跨章节重复内容一致性检查**（新增，2026-07-16）：

> **目的**：同一文档内多个章节描述同一事时（如 §3.5 和 §4.3 都列三架构差异表），必须保证内容一致。这是 Step 0.5 12 节评审的补充——12 节评审看骨架，0.5.5 看骨架间的交叉一致性。
> **触发场景**：02-higher-half-kernel.md review 发现 §3.5 和 §4.3 都列三架构差异表，但寄存器名矛盾（§3.5 写 `mov sp, x0`，§4.3 正确写 `mov sp, {stktop}`）。
> **扩展（2026-07-17，模式 72 CSSCM 配套）**：步骤数对齐——§2 C 分析列 N 步、§4 Rust 实现 M 步时，若 M ≠ N 必须在 §4 末尾添加"与 C N 步的差异说明"表。每条差异必须归类为：架构演进 / 设计决策 / 已知缺口 / C 源码 bug，并附 C 源码行号。检查命令：`rg "与 C .* 步的差异说明|步骤数差异|未实现步骤" {doc}`。详见 [review-patterns.md 模式 72](review-patterns.md)。

**检查方法**：
1. 扫描文档找出"描述同一主题的多个章节"（grep 关键词：表格、对照、差异、对比）
2. 对每对重复内容，逐字段对照一致性
3. 输出一致性矩阵

**一致性矩阵格式**（必须输出）：

| 主题 | 章节 A | 章节 B | 一致? | 不一致字段 | 严重度 |
|------|--------|--------|-------|-----------|--------|
| 三架构差异表 | §3.5 行 276 | §4.3 行 670 | ❌ | aarch64 栈切换：§3.5 `mov sp, x0` vs §4.3 `mov sp, {stktop}` | P1 |
| 三架构差异表 | §3.5 行 277 | §4.3 行 673 | ❌ | aarch64 跳转：§3.5 `br x2` vs §4.3 `br x1` | P1 |

**严重度判定**：
- P0：核心契约矛盾（如函数签名、错误码、时序）
- P1：技术细节矛盾（如寄存器名、指令名、地址值）
- P2：表述风格差异（如同义不同写）

**通过门槛**：无 P0 矛盾 → 通过；有 P0/P1 矛盾 → 必须修复后才能标 CONVERGED

#### Step 0.5.6 6 维反查矩阵（标准化反查方法，新增，2026-07-16）

> **目的**：把"反查"作为 Step 0.5 的标准化方法，**超越原 Step 0.5.3 的 outline ↔ 文档单维检查**。每次 review 必跑，输出统一的「6 维反查矩阵」。这是对之前 6 维建议（A-F）的全面落地。

**6 维反查维度**：

| # | 维度 | 实际工具 | 反查对象 | 偏离类型 |
|---|------|---------|---------|---------|
| **1** | **outline ↔ 文档正文** | grep + Read | `outline.v{N}.md` vs `{doc}.md` | 遗漏/多余/顺序错位/强调失配/意图偏离 |
| **2** | **design ↔ Rust 代码** | rg + Read | `design.v{N}.md` vs `os/**/*.rs` | trait 未实现/签名偏移/ARCH 未标注/不变量未体现/错误码偏移 |
| **3** | **design ↔ Minix3 C 源码** | rg + Read | `design.v{N}.md` vs `minix3/**/*.c` | C 行为遗漏/Rust 行为偏移/错误码不一致/副作用遗漏 |
| **4** | **outline ↔ design**（结构契约与代码契约一致性）| Read + diff | `outline.v{N}.md` vs `design.v{N}.md` | 概念命名不一致/API 未声明/重点未实现 |
| **5** | **文档 ↔ 代码**（横向反查）| rg + Read | `{doc}.md` vs `os/**/*.rs` | 注释与文档矛盾/测试未验证/行号错误 |
| **6** | **元层反查** | diff + Read | `v{N}` vs `v{N-1}` 快照、跨文档 outline | 概念增删/命名变更/跨文档契约违反/修了又出 |

**反查矩阵统一格式**（必须输出）：

```markdown
## 6 维反查矩阵（Step 0.5.6）

### 维度 1：outline ↔ 文档正文
| 主题 | outline 章节 | doc 章节 | 偏离类型 | 严重度 | 判定依据 |
|------|------------|---------|---------|-------|---------|

### 维度 2：design ↔ Rust 代码
| 主题 | design 章节 | code 位置 | 偏离类型 | 严重度 | 判定依据 |
|------|------------|----------|---------|-------|---------|

### 维度 3：design ↔ Minix3 C 源码
| 主题 | design 章节 | C 源码位置 | 偏离类型 | 严重度 | 判定依据 |
|------|------------|----------|---------|-------|---------|

### 维度 4：outline ↔ design
| 主题 | outline 章节 | design 章节 | 偏离类型 | 严重度 | 判定依据 |
|------|------------|----------|---------|-------|---------|

### 维度 5：文档 ↔ 代码
| 主题 | doc 章节 | code 位置 | 偏离类型 | 严重度 | 判定依据 |
|------|---------|----------|---------|-------|---------|

### 维度 6：元层反查
| 主题 | v{N} 位置 | v{N-1} 位置 / 跨文档位置 | 偏离类型 | 严重度 | 判定依据 |
|------|----------|------------------------|---------|-------|---------|
```

**每条偏离必须标注三类判定**（依据"反查原则"段）：

| 判定类型 | 标注 | 含义 |
|---------|------|------|
| 🔴 直接判定 | P0/P1/P2 | AI 能判定哪个更好 |
| 🟡 Open Question | ❓ OQ-N | AI 无法判定，上交用户 |
| 🟢 共识一致 | ✅ | 无差异 |

**反查覆盖率**：6/6 维度必须全部输出，缺任一 → Step 0.5.6 FAIL。

**通过门槛**：
- 反查覆盖率 = 100%（6/6）
- 🔴 直接判定中 P0 偏离 = 0
- 🟡 Open Question 不阻塞 review（按反查原则）
- 🟢 共识一致项 ≥ 50%（健康反查应多数一致）

#### Step 0.5.7 章节意图分析（新增，2026-07-16）

> **目的**：对"反查矩阵"中标为"多余"的章节（维度 1、4），分析其教学意图，判断"试图向读者强调什么 / 试图解释什么"，决定保留/删除/转 outline。

**意图分析格式**（对每个"多余章节"必填）：

```markdown
### 章节意图分析（Step 0.5.7）

#### 多余章节：[doc §X.Y L###]
- **试图强调什么**？（教学意图）
- **试图解释什么**？（内容意图）
- **是否与 outline 教学目标对齐**？（✅ 对齐 / ⚠️ 偏离 / ❌ 无关）
- **判定**：
  - ✅ 合理 → 建议更新 outline 增列该章节（避免下次 review 误判）
  - ⚠️ 偏离 → 改写章节以符合 outline 目标
  - ❌ 无关 → 删除该章节
```

**示例**（02 文档 §6「过渡：从 HigherHalf 切换到 kmain」）：
- 试图强调：从 boot-shim 到 kmain 的状态过渡（教学意图）
- 试图解释：CPU 已在高地址执行 kmain 的系统状态（内容意图）
- 是否对齐：✅ 对齐（跨文档衔接）
- 判定：✅ 合理 → 更新 outline 增列"过渡章节"

#### Step 0.5.8 issue 反查来源标注（新增，2026-07-16）

> **目的**：scan.md 中每条 issue 必须标注"反查来源"，便于追溯 issue 是哪类反查发现的，改进反查工具时知道哪类维度 issue 最多，用户审 scan.md 时知道每条意见的可信度。

**强制格式**：

```markdown
## Issue 清单（带反查来源）

| Issue ID | 严重度 | 反查维度 | 反查维度号 | 位置 | 判定类型 |
|---------|-------|---------|----------|------|---------|
| P1-1 | P1 | outline ↔ doc | 维度 1 | doc §3.5 L276 + aarch64/higher_half.rs:41 | 🔴 直接判定 |
| OQ-1 | ❓ | design ↔ code | 维度 2 | design §3.4 vs os/kernel/src/lib.rs:71-104 | 🟡 Open Question |
```

**反查维度号**必须从 [维度 1, 维度 2, 维度 3, 维度 4, 维度 5, 维度 6] 中选取一个。

**禁止**：无反查来源标注的 issue → 视为"凭空产生"，严重度降级（P0 → P1，P1 → P2）。

**Step 0.5 产物**：structure.md（保存到与 scan.md 同目录） + 评审结果表 + **outline 偏离矩阵**（若 outline.md 存在）+ **跨章节一致性矩阵**（Step 0.5.5 产物）+ **6 维反查矩阵**（Step 0.5.6 产物）+ **章节意图分析**（Step 0.5.7 产物，多余章节时必填）+ **issue 反查来源标注**（Step 0.5.8 产物，所有 issue 必填）

### Step 0.7: TODO 验证（若输入含 TODO 清单，新增，2026-07-16）

> **目的**：当 review 输入包含外部 TODO 清单（历史案例如 `tmp_design_and_todo/0108-todo-final.md` 多 AI bagging 产物——该目录已删除；当前形态为 `meta/coordination/todo-ledger/<stage>.yaml`）时，TODO 验证是 review 的前置步骤，不是独立任务。TODO 验证结果直接喂入 Step 2 差异提取，避免二次 grep。
> **触发条件**：用户输入包含 TODO 清单路径，或 review 目标文档含 `TODO`/`todo!`/`unimplemented!` 标记。

**执行步骤**：
1. **逐个验证 TODO 真实性**：对每个 TODO，用 grep/glob/read 交叉验证：
   - TODO 声称的文件/函数/行号是否真实存在
   - TODO 描述的问题是否真的存在（可能是误报）
   - TODO 优先级是否合理（P0/P1/P2）
2. **分类处理**：
   - ❌ **误报**：TODO 描述的问题不存在（如事实错误、已修复）→ 标记"误报否定"，不修复
   - ⚠️ **真实（代码任务）**：TODO 指向代码缺失（如 build.rs 缺失）→ 在文档中标注"实现状态"，不强行写代码
   - ✅ **真实（文档任务）**：TODO 指向文档问题 → 直接修复文档
3. **修复真实 TODO**：仅修复真实文档 TODO，代码 TODO 标注状态
4. **喂入 Step 2**：TODO 验证过程中发现的 grep 证据（如行号、寄存器名）直接用于 Step 2 差异提取，不重复 grep

**TODO 验证结果表**（必须输出到 scan.md）：

| TODO | 原优先级 | 验证结论 | 证据 | 处理 |
|------|---------|---------|------|------|
| TODO-XX-1 | P0 | ❌ 否定 | grep 证据 | 无需修复 |
| TODO-XX-2 | P1 | ✅ 真实 | grep 证据 | ✅ 已修复 |

**时间预算**：TODO 验证计入 review 时间预算，不单独计费。预计每个 TODO 5 分钟（含 grep 验证）。

**TODO 验证 vs Review 发现问题区分**（NEW 2026-07-16）：

| 维度 | TODO 验证（Step 0.7） | Review 发现（Step 2+） |
|------|---------------------|---------------------|
| 来源 | 外部 TODO 清单（如 `0108-todo-final.md`）| Reviewer 阅读文档/代码后独立发现 |
| 时间点 | Step 0.7 验证阶段（review 开始前） | Step 2-5 review 阶段 |
| 输出位置 | scan.md `§Step 0.7 TODO 验证表` | scan.md `§Issue List` |
| 标识 | `已修复：TODO-XX-N`（带原 TODO ID）| 新 issue ID（如 `P1-1` 等）|
| 严重度约束 | 沿用 TODO 原优先级（可能过期）| 按 review 重新判定 |
| 性质 | **修复型**（greppable + 验证存在）+ 有时间戳 | **发现型**（concept check / design check）+ 无时间戳 |
| 证据强度 | L1 grep 验证（含修改前后行号） | L1/L2/L3（按 §3.5 evidence 分级） |
| 必填字段 | TODO ID + 验证结论 + 证据 + 处理 | 反查来源（Step 0.5.8 强制格式）+ Location + 证据 |

**强制规则**：TODO 验证的"已修复"项与 Step 2+ 的 "新发现 issue" **必须分别列在 scan.md 不同段**：
- `§Step 0.7 产物：TODO 验证表`（已修复项）
- `§Issue List`（Review 新发现）

不可合并（TODO 修复可能跨多次 review 会重复被修复，issue 编号不应复用）。

#### Step 0.7.1: Path Existence Validation（NEW 2026-07-16，模式 66 RCPD 配套）

> **目的**：TODO 描述中引用的 `file:line` 必须真实存在，否则会导致虚假 P0 共识（参考 [review-patterns.md 模式 66 RCPD](../review-rules/review-patterns.md)）。
> **触发条件**：每次执行 Step 0.7 TODO 验证时，**强制**对每条 TODO 描述中的 `file:line` 引用执行存在性验证。

**执行步骤**：
1. **提取所有 file:line 引用**：从 TODO 描述中 grep 形如 `path/to/file.rs:NNN` 或 `path/to/file.rs:NNN-MMM` 的引用
2. **逐项验证存在性**：
   ```bash
   # 文件存在性
   ls -la <file_path>
   # 函数/符号存在性（可选）
   rg "fn <symbol_name>" <file_path> -n
   # 行号范围精确验证（推荐）
   sed -n '<start_line>,<end_line>p' <file_path>
   ```
3. **分类处理**：
   - ✅ **存在** + 行号范围与 TODO 描述匹配 → 标 "valid"，进入正常验证流程
   - ⚠️ **存在** + 行号范围偏差 >5 行 → 标 "stale line range"，需重新核对（**P2 偏差不阻塞，但需在 doc 修正**）
   - ❌ **不存在**（文件已删除/重构/重命名）→ 标 "**path drift**"，触发 doc 重写而非 code 修复；TODO 严重度自动降级（**P0 → P1 至少**）
4. **多 AI 共识过滤**（NEW）：
   - 任何"多 AI 共识"必须满足"全共识 × 全部 path 存在验证通过"才采纳为真实 bug
   - 任一共识 AI 基于 path drift → 整条 TODO 标"共识待验证"

**强制格式**（scan.md §Step 0.7 必须含此表）：

| TODO | 引用 file:line | ls 验证 | 偏差类型 | 处理 |
|------|---------------|--------|---------|------|
| TODO-04-2 | `os/arch/src/{x86_64,riscv64,arm64}/proc_arch.rs:350/252/279` | ❌ 3 个文件全部 No such file | **path drift** | 严重度 P0 → P1（doc 重写，不修 code）|
| TODO-XX-N | `<file>:<line>` | ⚠️ 文件存在，行号偏差 ±6 | **stale line range** | 标 P2 偏差，下次 review 修正 doc |

**严重度降级规则**：
- P0 + path drift → P1（强制）
- P1 + path drift → P2（强制）
- P2 + path drift → 标"误报"（✅ 删除）
- 任何严重度降级必须附 L1 证据（`ls`/`rg` 命中或失效证明）

**反例**（必须避免）：
- ❌ "TODO 描述引用 `proc_arch.rs:350`，未做 ls 验证直接采纳为 P0 真实 bug" → 模式 66 触发
- ❌ "2 AI 共识 TODO 严重度，未验证共识基础" → 模式 66 触发

**正例**（04-platform-discovery §11 Session #8 案例）：
- ✅ `ls os/arch/src/arch/{x86_64,riscv64,arm64}/proc_arch.rs` → 3 个文件全部 No such file
- ✅ 读 `os/arch/src/arch/mod.rs:20` 注释 "proc_arch was removed in 06-design-final.md"
- ✅ 结论：T2 严重度 P0 → P1 降级，触发 doc §11 重写而非 code 修复

**自动化建议**（未来工具）：
- `python3 tools/todo-ledger.py check` 对 YAML 账本的 `origin_file` 做存在性 FAIL、`origin_line` 做超界 WARN；
  `--strict-refs` 进一步解析 `refs` 的符号/文档单元/测试/判决行。旧 markdown `todo.md` 在迁移前不做 file:line 自动对账，人工判定。
- 可与 `coverage-extract.py` 配合使用，输出"已删除引用 + 已变更引用"两个清单

#### Step 0.7.2: AI Claim Grep Verification（NEW 2026-07-16，模式 67 CFNOC 配套）

> **目的**：AI 报告中引用的"概念对象"（如 "CLOCK task"、"system server"、"kernel subsystem"）必须真实存在，否则会导致虚假 P0/P1 共识（参考 [review-patterns.md 模式 67 CFNOC](../review-rules/review-patterns.md)）。
> **触发条件**：每次执行 Step 0.7 TODO 验证时，**强制**对每条 TODO 描述中的"概念对象关键词"执行 grep 存在性验证。

**执行步骤**：
1. **提取所有概念对象引用**：从 TODO 描述中 grep 形如 "X task" / "X subsystem" / "X server" / "X daemon" 的引用
2. **逐项验证存在性**：
   ```bash
   # 概念对象存在性
   rg -ni "X task|X subsystem|X server|X daemon" <target_doc.md>
   rg -ni "X" minix3/minix/include/ --type c
   rg -ni "X" minix3/minix/kernel/ --type c
   # 函数存在性（用于区分"操作名" vs "对象名"）
   rg "fn init_X|fn X_init" minix3/minix/ --type c -n
   ```
3. **分类处理**：
   - ✅ **概念对象真实存在**（在 C 源码或 doc 中有 `proc_ptr` / `NR_TASKS+N` 等表示）→ 标 "valid concept"
   - ⚠️ **仅有 C 函数名 `init_X()` / `X_init()`** → 标 "**operation not object**"，TODO 严重度自动降级（P0 → P1 / P1 → 误报）
   - ❌ **完全无匹配** → 标 "**phantom concept**"，触发 doc 重写

**C 函数名 vs OS 概念对象 区分规则**：
| C 代码形式 | 含义 | 类别 |
|-----------|------|------|
| `void init_clock(void)` | "对 clock 的初始化动作" | **操作名**（非对象） |
| `int clock_task(void)` | "clock 任务进程函数体" | **对象**（任务进程） |
| `proc[NR_TASKS+N]` | "任务表中的某进程" | **对象**（进程实体） |
| `#define CLOCK_TASK ...` | "clock 任务的索引常量" | **对象**（常量标识） |
| `SYSTEM` / `TASK_CLOCK` | "任务类型枚举值" | **对象**（类型标识） |

**强制格式**（scan.md §Step 0.7.2 必须含此表）：
| TODO | 引用概念 | rg 验证 | 分类 | 处理 |
|------|---------|---------|------|------|
| TODO-05-1 | "CLOCK task" | ❌ 0 命中 | **operation not object**（`init_clock()` 是操作名）| 误报（前提错误） |
| TODO-XX-N | "X task" | ✅ `proc_ptr` / `NR_TASKS` 命中 | valid concept | 正常验证流程 |

**反例**（必须避免）：
- ❌ "AI 报告 'CLOCK task'，未做 rg 验证直接采纳为 P1 真实问题" → 模式 67 触发
- ❌ "2 AI 共识 TODO 严重度，未验证共识基础" → 模式 67 触发

**正例**（05-clock-interrupt-init Session #10 案例）：
- ✅ `rg "CLOCK task\|clock task\|System Task\|Kernel Subsystem" 05-clock-interrupt-init.md` → 0 命中
- ✅ `rg "init_clock" minix3/minix/kernel/clock.c -n` → clock.c:48 是 `void init_clock(void)` 函数定义（非 "CLOCK task"）
- ✅ 结论：F1 误报（前提错误：把 `init_clock()` 操作名错认为 "CLOCK task" 对象名）

#### Step 0.7.3: Doc Chapter Context Awareness（NEW 2026-07-16，模式 68 DSC 配套）

> **目的**：AI 报告中引用的 doc 章节上下文必须明确（Ch2 C 展示 vs Ch4 Rust 实现），否则会导致虚假 P0/P2 误判（参考 [review-patterns.md 模式 68 DSC](../review-rules/review-patterns.md)）。
> **触发条件**：每次执行 Step 0.7 TODO 验证时，**强制**对每条 TODO 描述的章节上下文做归属判定。

**执行步骤**：
1. **识别章节类型**：从 TODO 描述中识别涉及的章节（Ch1/Ch2/Ch3/Ch4/Ch5/Ch6）
2. **判定章节性质**：
   - **Ch1** = 概念驱动（主语 CPU/OS/机制）
   - **Ch2** = C 源码分析（**展示** C 代码，**应保留** `#ifdef`）
   - **Ch3** = Rust 设计决策（解释 trait/类型系统选择）
   - **Ch4** = Rust 实现详解（**实际代码**，应避免 `#ifdef` 散落）
   - **Ch5** = 测试要点
3. **交叉验证 Rust 实现**：
   ```bash
   # 验证 Rust 端是否有散落的条件编译
   rg "#\[cfg\(|\bifndef\b|\bifdef\b" os/ --type rust
   rg "static mut" os/ --type rust
   rg "fn.*#\[cfg" os/ --type rust
   ```
4. **分类处理**：
   - **Ch2 引用 + Ch2 报告** → ✅ 合法（C 展示章节必须保留 C 预处理指令）
   - **Ch2 引用 + Ch4 报告** → ⚠️ **章节错位**（DSC 模式触发）：AI 把 Ch2 C 展示错认为 Ch4 Rust 实现问题
   - **Ch4 引用 + Rust 端 grep 0 命中** → ⚠️ **章节错位**：AI 报告"Ch4 有 #ifdef"，但 Ch4 实际无 `#[cfg]`
   - **Ch4 引用 + Rust 端 grep 命中** → ✅ 真实问题

**Doc 章节类型判定表**（自动分类）：
| doc 章节特征 | 类型 | 应包含 |
|------------|------|--------|
| 标题含 "概念" / "本章聚焦" | Ch1 | 主语 = CPU/OS/机制 |
| 标题含 "C 源码分析" / "Minix3 C 行为" | **Ch2** | C 代码块（含 `#ifdef`/`#include`）|
| 标题含 "Rust 设计" / "决策" / "选型" | Ch3 | trait 选择 + 类型系统论证 |
| 标题含 "实现" / "代码" / "落地" | **Ch4** | Rust 代码块（应避免 `#ifdef` 散落）|
| 标题含 "测试" / "验证" | Ch5 | 测试列表 + QEMU 集成 |

**强制格式**（scan.md §Step 0.7.3 必须含此表）：
| TODO | 引用章节 | 章节类型 | Rust 实现 grep | 判定 | 处理 |
|------|---------|---------|--------------|------|------|
| TODO-05-2 | "§2.4/§2.5 arch_init" | **Ch2**（C 展示）| `rg "ifndef CONFIG_SMP" os/` 0 命中（仅 3 注释）| **章节错位**（DSC）| 误报（前提错误：Ch2 应保留 C 预处理指令） |
| TODO-XX-N | "Ch4 code has #ifdef" | Ch4（Rust 实现）| `rg "#\[cfg\(smp" os/` 命中 | 真实问题 | 正常修复流程 |

**反例**（必须避免）：
- ❌ "AI 报告 'Ch2 §2.4 有 30+ 行 ifndef 散落'，未做章节类型判定直接采纳为 P2" → 模式 68 触发
- ❌ "看到 #ifdef 即报告 Rust 代码散落，未区分 Ch2 展示 vs Ch4 实现" → 模式 68 触发

**正例**（05-clock-interrupt-init Session #10 案例）：
- ✅ `rg "ifndef CONFIG_SMP\|ifdef CONFIG_SMP" 05-clock-interrupt-init.md §2.4` → 命中（C 展示章节）
- ✅ `rg "ifndef CONFIG_SMP\|ifdef CONFIG_SMP\|#\[cfg\(smp\|CONFIG_SMP" os/` → 仅 3 注释提及（os/kernel/src/{smp.rs, sched.rs}），无 `#[cfg]` 分支

#### Step 0.7.4: TODO Staleness Check（NEW 2026-07-16，模式 70 CTOS 配套）

> **目的**：防止外部 TODO 清单因跨多轮 review 累积而包含"前提失效"误报。Session #12 实测（历史输入 `tmp_design_and_todo/0108-todo-final.md`，该目录已删除）：8 个 TODO-06 中 3 个 (37.5%) 是误报，主要原因为 TODO 列表引用了已修复/已接通的旧状态。
> **触发条件**（2026-08-15 修复 A-P1-1 明确语义）：输入 TODO 清单中 TODO 数 > 5 **且（AND）** 任一 TODO 描述含"基于..."/"依赖..."/"待..."/"阻塞"等时间敏感词 → **必须跑** Step 0.7.4。两个条件必须同时满足才触发；仅 TODO 数 > 5 但无时间敏感词不触发，仅含时间敏感词但 TODO 数 ≤ 5 不触发。

**执行步骤**：
1. **识别"基于状态"前提**：从 TODO 描述中 grep 形如下模式：
   - "基于 TODO-XX-N 未修复"
   - "依赖 Y 已实现/未实现"
   - "待 X 完成"
   - "因 Z 阻塞"
2. **对每个前提，用 rg 验证当前状态**：
   ```bash
   # 例：TODO-06-4 假设 "TODO-01-3 阻塞"，验证 TODO-01-3 当前状态
   #（历史案例：当时目录名为 03-stage-kernel、文档编号 01-kmain-cstart；现目录 01-stage-kernel、该文档编号 03）
   rg "TODO-01-3" rewrite-notes/01-stage-kernel/03-kmain-cstart.md -n
   # 如果对应文档没有 TODO-01-3 标记 → TODO-06-4 的前提失效
   ```
3. **分类处理**：
   - ✅ **前提成立** + 描述准确 → 正常 TODO 验证流程
   - ⚠️ **前提偏差**（部分已修复/部分阻塞）→ 修正前提，TODO 重新分类
   - ❌ **前提失效**（已修复/已接通/不存在）→ 标"前提失效，TODO 不适用" + 严重度自动降级（P0→P1，P1→P2，P2→误报）
4. **批量模式**：当 TODO 数 ≥ 10 时，先跑 `python3 tools/todo-ledger.py check`（v2 账本）或对未迁移 markdown 人工核对前提依赖，输出 staleness 报告。

**强制格式**（scan.md §Step 0.7.4 必须含此表）：

| TODO | 引用前提 | rg 验证当前状态 | 判定 | 处理 |
|------|---------|--------------|------|------|
| TODO-06-2 | "1 个 trait 而非 3 个" | `rg "pub trait " os/kernel/src/proc.rs` 0 命中（当前无 pub trait） | ❌ 前提错误（误报）| 不修复，标"前提错误" |
| TODO-06-4 | "TODO-01-3 阻塞" | `rg "TODO-01-3" 01-kmain-cstart.md` 0 命中（已接通）| ❌ 前提失效 | 不修复，标"前提失效" |
| TODO-06-7 | "需 3 trait" | `rg "pub trait " os/kernel/src/proc.rs` 已正确实现 | ✅ 前提成立 + 已修复 | 标 ✅ 已修复（无需修改）|

**严重度降级规则**（与模式 66 RCPD 一致）：
- P0 + 前提失效 → P1
- P1 + 前提失效 → P2
- P2 + 前提失效 → 标"误报"（✅ 删除）
- 任何降级必须附 L1 证据（`rg` 命中或失效证明）

**反例**（必须避免）：
- ❌ "未做 staleness check 直接采纳 8 个 TODO 全部为真 → 误报率 37.5%（3/8）浪费 review 时间" → 模式 70 触发
- ❌ "看到 TODO 标题就复制进 §Step 0.7 TODO 验证表，未验证'基于状态'前提" → 模式 70 触发

**正例**（Session #12 06-proc-init-boot-proc 案例）：
- ✅ 8 个 TODO-06 → staleness check 后 → 3 误报 + 5 真实 → 实际修复 4 项（1 项已修复）
- ✅ 节省约 30 分钟（避免修复误报 + 二次 grep）

**Step 0.7.1/0.7.2/0.7.3 关系**（AI claim verification 三元组）：
- **0.7.1 Path Existence Validation** (RCPD)：验证 TODO 引用的代码路径是否真实存在
- **0.7.2 AI Claim Grep Verification** (CFNOC)：验证 AI 引用的概念对象是否真实存在
- **0.7.3 Doc Chapter Context Awareness** (DSC)：验证 AI 引用的章节上下文是否正确归属
- 三者共同构成"AI claim verification 三元组"，覆盖 AI 报告中 3 类常见误报来源

---

- **中间产物**：执行模式 + 范围声明 + 时间预算声明（或省略说明） + 状态恢复摘要 + **Step 0 预检结果（STATE/scan/structure/SYMBOLS/VERIFY-CHECK + 关联代码清单）** + **TODO 验证结果表（若执行 Step 0.7）** + **Path Existence Validation 表（若执行 Step 0.7.1）**

### Step 1: Ground Truth Lookup（源码定位）

- 识别文档中提到的所有 Minix3 源文件
- 使用 `rg` 命令验证这些文件是否存在于 `minix3/` 目录
- 记录每个引用的**文件路径**和**行号范围**

> **效率提示**：如果文档较长（>500 行），优先用 grep 提取所有 `.c` / `.h` 文件引用（`rg "\.c" doc.md`），然后抽样验证引用行号的准确性，而非逐行全量验证。Step 3 再补做行号精确验证。

**中间产物**（必须输出）：
```markdown
### Step 1 产物：源码文件清单

| 文件路径 | 文档引用位置 | 文件存在? | 引用行号范围 |
|---------|------------|----------|------------|
| minix3/minix/servers/vm/pb.c | Ch2§2.3 | ✅ | 33-168 |
| minix3/minix/servers/vm/region.h | Ch2§2.2 | ✅ | 23-35 |
| ... | ... | ... | ... |
```

### Step 1.0 锚点解析（符号锚点校验，anchor-resolve；2026-09-18 D5 改写收编原 1.0a~1.0g）

> **背景**：行号是 AI 的校验脚手架，不是目标；漂移用符号锚点结构性消灭，机械项降级为卫生项。本节只保留当前的一步检查。
> **职责划分**：符号锚点解析与存在性 → 本步（`tools/anchor-resolve.sh`）；路径约定（`os/` 前缀）→ review-doc-checklist §2.4c（模式 74）；文档 Rust 代码块 idiom（`static mut` 等）→ review-doc-checklist §2.4b（模式 73）+ 文档 Rust 代码块逐块审查（`tools/doc-snippet-extract.sh`）；代码注释 doc 归属 → 模式 76（按卫生项处理）；forward reference 透明声明核对保留在 §2.2 引用验证中执行。

**锚点约定**（写入正式文档与中间产物）：

| 对象 | 锚点形态 | 例子 |
|---|---|---|
| Rust 函数/类型/trait/常量 | `path:fn name`、`path:struct Name`、`path:enum Name`、`path:trait Name`、`path:const NAME` | `os/kernel/src/clock.rs:fn clock_init` |
| 同名方法歧义 | 加 impl 限定 | `os/kernel/src/proc.rs:impl Proc::new` |
| C 函数/结构体/宏 | `path:func`、`path:struct name`、`path:NAME` | `minix3/minix/kernel/proc.c:proc_init` |
| 函数内部的具体片段 | 函数符号 + 引文片段 | `os/kernel/src/proc.rs:fn do_fork（match 分支）` |
| 行号（可选） | **只进锚点底账**：`.review/{tool}/{stage}/{doc-stem}.anchors.md`（`tools/anchor-resolve.sh --extract {doc}` 生成）。**正式文档正文禁止出现 `（Lnnn，工具生成）`**——坐标是审查看的、讲解是读者看的（style-bible 硬裁决），SL-10 按 error 级计；移出与正文修改同一笔提交，**底账缺失的移出 = P0** | — |
> ⚠️ **迁移产物语义告警（S1，2026-09-18）**：带 `（Lnnn，工具生成）` 后缀的锚点是迁移启发式（向上就近定义）的产物——L 是**原文档行号**（非现行号），且旧行号本身可能已漂移，解析出的符号**未必是句子讨论的符号**（错符号比错行号更隐蔽：符号看似权威）。规则：
> 1. 迁移产物在句意复核前**不得当作权威引用**；复核清单见 `tools/anchor-suspect-baseline.txt`（符号名未出现在句中的 L 后缀锚点，正式文档 868 处）。
> 2. 范围锚点（原 `path:N-M`）按首行启发式解析，同属需复核。
> 3. review 中发现句子讨论的符号与锚点符号不一致 → 按 P0-fact 修正锚点（这是修正句意，不是修格式）。


**执行**：
```bash
tools/anchor-resolve.sh --check {doc}.md        # 校验文档内全部符号锚点（fenced 代码块不参与）
tools/anchor-migrate.sh [--write] {doc|dir}     # 旧行号锚点一次性迁移（默认 dry-run；工具不猜，无法解析列清单）
```

**判定（先句意复核，再机械判定）**：
- **第 0 步（强制）：句意复核**——对每个锚点问"这个符号名出现在句子里了吗？句子讨论的真是它吗？"。
  工具只做存在性/唯一性，**不做语义**；"符号可解析"不等于"锚点指对了对象"。
  - **按侧对称**：复核必须同时覆盖 `os/`（Rust）与 `minix3/`（C）两侧。历史基线 `tools/anchor-suspect-baseline.txt`
    只收 `os/` 锚点（868 条），C 侧零覆盖——**C 锚点错符号没有任何工具会预警**（2026-10-08 三轮取证：
    6 处/39 处/10 处成片存在）。
- 符号锚点解析 **0 定义** → 先判"是符号消失/改名，还是**符号选错**"：前者 **P0-fact**；后者改锚点为真符号（同为 P0-fact，但修法不同——不是修句子，是修锚点）。
- **多定义** → 先判"真多定义（同名多处定义，需补 `impl Type::method` 限定）"还是"**符号选错**（如锚点写成结构体类型名
  `rproc`，句子讲的却是 `publish_service`）"。后者改真名后 `multi-def` 自消；工具不加特例。
- 残留的手工行号锚点、全角/半角、拼写、格式 → **卫生项**：scan.md 单列"卫生项"分区，只记录 + 批量修，**不进 P1/P2 计数、不进 `weighted_new`、不参与 Step 7.1 收敛判定**（唯一例外：符号锚点 0 定义/符号选错按 P0-fact 处理——属正确性，不属卫生）

**一次性迁移基线（2026-09-18 已完成）**：全仓正式文档中 4,701 处可机械恢复的行号锚点已转为符号锚点（316 个文件，行数零变化）；无法机械恢复的（目标文件消失/改名、行号漂移超出识别、裸文件名重名）保留原样并登记清单（`.review/anchor-migration-unresolved.txt`），按卫生项批量清理，不阻塞 review、不计入收敛。

### Step 1.5: Coverage Enumeration（覆盖率穷举，机器+AI）

> **目的**：机器生成穷举清单，AI 只负责语义判断。解决 AI "凭印象扫描"导致覆盖率不足的问题。
> **基础**：`tools/coverage-extract/coverage-extract.py` 确定性脚本生成 SYMBOLS.md 骨架。
> **详见**：[review-coverage-skill.md](../skill/review-coverage-skill.md)

**执行步骤**：

1. **机器生成 SYMBOLS.md 骨架**（**强制运行**，见 Gate A 强制运行规则；不允许"语义范围手动验证"代替）。
   单元模式下 `fingerprint` 报 `code: same` 时可复用上轮 SYMBOLS，但必须在 scan.md 记
   `coverage_reused` 与 fingerprint 输出；代码 hash 变化则必须重跑。
   ```bash
   # 模块级 — 服务器模块（vm / pm / vfs / rs / ds / inet ...）
   #   注：脚本第一参数 {minix3-module} 是 Minix3 模块名；--output 路径里的 {stage} 是评审状态目录键（笔记树根下第一级目录名）
   python3 tools/coverage-extract/coverage-extract.py {minix3-module} {doc_dir} \
     --rust-dir os --c-dir minix3/minix/servers/{minix3-module} \
     --output .review/{tool}/{stage}/scans/SYMBOLS.md

   # 模块级 — 内核
   python3 tools/coverage-extract/coverage-extract.py kernel {doc_dir} \
     --rust-dir os --c-dir minix3/minix/kernel \
     --output .review/{tool}/{stage}/scans/SYMBOLS.md

   # 单文档级（推荐 doc-specific review）— 服务器模块
   python3 tools/coverage-extract/coverage-extract.py {minix3-module} {doc_dir} \
     --rust-dir os --c-dir minix3/minix/servers/{minix3-module} \
     --doc-file {target-doc}.md \
     --semantic-map tools/coverage-extract/{minix3-module}-semantic-map.json \
     --output .review/{tool}/{stage}/scans/{doc-stem}-{agent}-SYMBOLS.md

   # 单文档级 — 内核
   python3 tools/coverage-extract/coverage-extract.py kernel {doc_dir} \
     --rust-dir os --c-dir minix3/minix/kernel \
     --doc-file {target-doc}.md \
     --semantic-map tools/coverage-extract/kernel-semantic-map.json \
     --output .review/{tool}/{stage}/scans/{doc-stem}-{agent}-SYMBOLS.md
   ```
   > **目录创建**：脚本已修复为使用 `--output` 时自动创建父目录；若使用旧版本脚本，请先 `mkdir -p $(dirname .review/.../SYMBOLS.md)`。
   > **`--doc-file` 只写文件名**（如 `10-pm-wait.md`，**不带目录**）。脚本已归一化（传路径会取 basename），
   > 且未命中任何文档时**报错退出**（历史坑：静默输出"文档覆盖 0.0% + 空符号表"的假报告）。
   > **`--rust-dir` 必须显式传**：脚本对非 vm/kernel/pm 的模块不会推断 Rust 目录，缺省会静默按"无 Rust"跑（`Found 0 Rust symbols`）。
   > **模块无语义映射表**（`ls tools/coverage-extract/{module}-semantic-map.json` 不存在）→ 这**不是实现缺口**，
   > 而是工具数据缺口：Rust 覆盖会恒 0%。处置 = **新建该表**（照 `rs-semantic-map.json` 的格式，C 符号 → Rust 落点），
   > 而不是在 scan.md 里写"Rust 未实现"。2026-10-08 已补 `sched-semantic-map.json`。
   > **短名假阳性**：文档覆盖按子串匹配，`US`、`main` 这类短符号会命中任意含该子串的文本；符号抽样里遇到短名必须人工判真伪。
   脚本自动提取 C 函数/结构体/宏/枚举 + Rust pub 项 + 文档覆盖检查 + 名称匹配。
   - `--rust-dir os`：扫描整个 `os/` 目录，避免 `kmain`/`ProtectionArch` 等跨 crate 符号遗漏。
   - `--c-dir`：服务器模块用 `minix3/minix/servers/{minix3-module}`，内核用 `minix3/minix/kernel`。
   - `--semantic-map`：C→Rust 改写必须提供语义映射表，否则 Rust 覆盖率会显示为 0%。
   - `--doc-file`：限定到单篇文档，避免两篇 doc 的 coverage 数字完全相同。
   - **Gate A 强制运行规则**：运行后必须将命令 + stdout 写入 scan.md 的 `gate-evidence-A` 块（见 review-coverage-skill.md §0）。若脚本物理不可用 → 显式记录 PARTIAL 状态（≠ PASS），不允许进 Final Review。
   - **2026-08-15 修复 E-P0-1（Gate A 补救路径）**：脚本不可用（环境缺 Python、缺 coverage-extract.py 等）时，**禁止直接标 PARTIAL 通过**，必须按以下补救路径之一：
     - **路径 A（推荐）**：定位 root cause（`which python3`、`ls tools/coverage-extract/coverage-extract.py`）→ 修复工具链 → 重跑脚本
     - **路径 B（脚本永久不可用）**：在 STATE.md `§Gate A PARTIAL 处置` 段记录：(a) 不可用原因（环境/脚本 bug）；(b) 人工覆盖范围（用 `rg` + `Read` 手工穷举的 C 符号）；(c) 人工覆盖结果与脚本预期覆盖率差异；(d) 用户确认记录。**仅当用户显式确认后可继续 review，但 scan.md 必须显式标 "Gate A: PARTIAL (人工覆盖)"**
     - **路径 C（放弃 Gate A）**：若人工覆盖也失败 → 整个 review 暂停，待工具恢复后重跑——不允许跳过 Gate A 后标 CONVERGED
   - **判定**：Gate A PARTIAL 必须有用户/STATE.md 双重记录，否则不允许进入收敛判定。
   - 若 Rust 覆盖率为 0%，必须先检查 `--rust-dir`/`--semantic-map` 是否正确，或确实缺失实现。

2. **AI 补充语义判断**（5 项，每项标注 evidence [DIRECT/MEDIUM/INFERRED]）：
   - Rust 对应关系确认（名称匹配 ≠ 语义对应）
   - 架构演进标记（ARCH: 不需要 + 理由）
   - 语义归属判定（以功能语义为准）
   - 行为契约表（核心函数：输入/输出/副作用/错误码/时序）
   - 测试覆盖补充（L1对偶/L2契约/L3 doctest）

3. **更新 STATE.md Coverage Status 段**

**中间产物**（必须输出）：
```markdown
### Step 1.5 产物：覆盖率穷举

**SYMBOLS.md**: .review/{tool}/{stage}/scans/SYMBOLS.md（模块级）或 .review/{tool}/{stage}/scans/{doc-stem}-{agent}-SYMBOLS.md（文档级）

| 指标 | 数值 |
|------|------|
| C 符号总数 | N |
| 文档覆盖 | M (M%) |
| Rust 覆盖 | K (K%) |
| 完全缺口 | G |
| 架构演进 | A |

**P0 缺口**（在语义范围内但无文档无Rust）:
| 符号 | C 源码 | 判定 | evidence |
|------|--------|------|----------|
| `func_name` | file.c:N | P0 缺口 | DIRECT: rg 无结果 |

**ARCH 标记**:
| 符号 | 理由 | evidence |
|------|------|----------|
| `map_service` | IPC 协议演进 | INFERRED |
```

> **反幻觉**：每个覆盖判定必须先执行 grep 验证，再下结论。禁止凭印象判断"已覆盖"。

### Step 1.6: 设计对齐检查（Design Alignment）

> **前提**：所有 review 模式都执行此步骤；快速/构造模式只可裁剪内容检查，不能跳过 Gate H。
> **目的**：验证当前 Rust 实现的 trait/类型/架构是否与 design 文档一致；没有 design 文档时，直接对照 C 源码与代码的语义一致性。
> **触发**：design 错误（存在但有错，design-wrong）→ 切换到 Profile R（Design-First 模式）；design 不存在 → H.2–H.5 记 N/A + 理由，不补生成。

**Step 1.6.1**：design 文档输入确认（可选）
```bash
ls {tree}/meta/design/{stage}/{NN}-design.v*.md         # 非 bagging（可选输入）
ls {tree}/meta/design/{stage}/{NN}-design-final.v*.md   # bagging（可选输入）
```
> 命中则作为对照输入；未命中不阻断，Gate H 只检查语义项。

**Step 1.6.2**：design ↔ code 一致性矩阵

```markdown
| design 决策 | design 中的实现路径 | code 中的实际路径 | 一致性 |
|------------|-------------------|-----------------|--------|
| PlatformDescriptorPtr 用 trait object | `&'static dyn PlatformDesc` | `pub enum PlatformDescriptorPtr` | ❌ 偏离 |
| DirectMapArch 替代 MemoryInitArch | 仅保留 DirectMapArch | 两者并存 | ⚠️ 部分偏离 |
```

> **2026-08-15 修复 E-P1-3（一致性百分比计算方法）**：
> - **计算公式**：`consistency_pct = (✅_count + ⚠️_count * 0.5) / total_decisions * 100`
>   - ✅ 完全一致：1.0
>   - ⚠️ 部分偏离：0.5（设计正确但实现部分偏离；code Refactor）
>   - ❌ 完全偏离：0（设计错 / 缺失 / 严重偏离）
> - **PASS 阈值**：≥ 80%
> - **失败处理**：< 80% → Step 1.6.4 状态判定为 `DESIGN_DIVERGED`（≥30% 偏离）或 `DESIGN_DIVERGED_WRONG`（设计错且严重偏离）
> - **示例**：10 项决策，6 ✅ + 2 ⚠️ + 2 ❌ → (6 + 2*0.5) / 10 = 70% → FAIL（< 80%）

**Step 1.6.3**：design ↔ Minix3 对齐检查

```markdown
| Minix3 概念 | design 中是否有对应 | code 中是否有对应 | 缺失位置 |
|------------|-------------------|-----------------|---------|
| 进程表 | ✅ §3.2 | ✅ | - |
| 特权表 | ✅ §3.4 | ✅ | - |
| 三类运行态实体区分 | ❌ | ❌ | design + code 都缺 |
```

**Step 1.6.4**：design 状态判定

| 状态 | 触发条件 | 后续动作 |
|------|---------|---------|
| **PASS** | 一致性 ≥ 80%，无 design-wrong | 进入 Step 2 |
| **DESIGN_MISSING** | design 不存在（可选输入缺失）| H.2–H.5 记 N/A + 理由；继续 review，不补生成 |
| **DESIGN_WRONG** | design 错（如漏核心概念） | 阻断 review，design Refactor 必须 |
| **DESIGN_DIVERGED** | design ↔ code 严重偏离（≥30%）且 design 正确 | code Refactor（修 code） |
| **DESIGN_DIVERGED_WRONG** | design ↔ code 严重偏离（≥30%）且 design 错 | design Refactor 必须 + code Refactor |

### Step 2: Diff Extraction（差异提取）

- 列出 **3 个**你认为文档/代码中最背离 Minix3 原始语义的地方
- 对每个差异，说明：
  - Minix3 源码的实际行为
  - 文档/代码中的描述/实现
  - 差异的性质（概念错误、语义偏移、过度简化等）

**差异类型区分**（新增，2026-07-16）：Gate B 的 8 字段表区分两种差异类型，避免"C 行为"字段对架构差异表强制 N/A：

| 差异类型 | 适用场景 | 8 字段表适配 |
|---------|---------|------------|
| **C→Rust 行为差异** | C 函数行为 vs Rust 实现行为（如语义偏移、错误码改变） | 8 字段全适用：函数名/C 行为/Rust 行为/差异类型/严重度/C 证据/Rust 证据/备注 |
| **doc↔code 描述差异** | 文档声称 vs 代码实际（如寄存器名错误、占位 URL） | 8 字段中"C 行为"字段改为"doc 声称"，C 证据字段为 N/A（无 C 源码对应） |

**判定规则**：若差异源于"文档描述与代码不符"（如 §3.5 表格寄存器名错误），归为 doc↔code 描述差异；若差异源于"Rust 实现偏离 C 行为"（如语义拆分），归为 C→Rust 行为差异。

**中间产物**（必须输出）：
```markdown
### Step 2 产物：Top 3 差异

| # | 差异点 | Minix3 行为 | 文档/代码描述 | 差异性质 |
|---|--------|------------|-------------|---------|
| 1 | xxx | pagetable.c:333 实际行为 | 文档 L245 描述 | 概念错误 |
| 2 | ... | ... | ... | ... |
| 3 | ... | ... | ... | ... |
```

**Gate B 行为契约表**（Top 5：3 语义偏移 + 2 覆盖缺口，8 字段 × 5 函数）：

| # | 函数/位置 | C 行为（或 doc 声称） | Rust 行为 | 差异类型 | 严重度 | C 证据 | Rust 证据 | Reviewer 备注 |
|---|----------|---------------------|----------|---------|--------|--------|----------|-------------|
| 1 | ... | ... | ... | C→Rust 行为差异 | P1 | file:line | file:line | ... |
| 2 | ... | doc 声称 X | 代码实际 Y | doc↔code 描述差异 | P1 | N/A | file:line | ... |

### Step 2.5: Link Validation（链路验证）

> **目的**：验证文档各章节之间的推导链路是否完整。链路断裂 = 设计或实现有问题。
> 详细的链路验证规则见 [review-doc-checklist.md §2.10](review-doc-checklist.md#210-章节链路验证强制不可跳过)。

**链路验证**：
1. Ch3→Ch1&2：每个设计决策是否有依据？
2. Ch4→Ch3：每个实现是否对应设计？
3. 测试→Ch3+Ch4：测试是否覆盖设计决策和实现细节？
4. 代码→Ch4：代码是否与文档描述一致？

**中间产物**：按 [review-doc-checklist.md §2.10](review-doc-checklist.md#210-章节链路验证强制不可跳过) 要求的 4 个表格输出。

> **注意**：局部 Review（仅 Ch1&2）时跳过此步骤。完整 Review 时在执行 Step 4 后，结合跨文档信息做最终链路验证。

### Step 3: Sanity Check（一致性检查）

- 验证文档第 2 章引用的**所有行号**是否真实存在于本地文件
- 验证文档中的**所有数值常量**是否与 Minix3 源码一致
- 验证文档中的**所有函数签名**是否与 Minix3 源码一致

**中间产物**：按 [review-doc-checklist.md §2.1](review-doc-checklist.md#21-概念准确性检查强制不可跳过) 和 [§2.2](review-doc-checklist.md#22-c-代码引用验证强制不可跳过) 要求的表格输出。

### Step 3.5: Precision Check（细节精确检查）

> **目的**：在 Sanity Check（大方向正确）之后，对代码和注释的微观精确性进行扫描。本步骤的 5 个元规则跨阶段复用，适用于 Boot/VM/PM/VFS 等所有模块。
> **策略**：Agent 负责"标记可疑点"，不要求 100% 自动判定正确性，输出待人工确认的标记列表。

**3.5.1 外部知识标记**
- 扫描代码注释中引用的外部知识（寄存器名称、标志位、协议条款、规范版本）
- 输出标记列表："以下注释引用了外部知识，建议人工验证准确性"
- **检查口令**："注释中提到的这个硬件行为/协议要求，来源是什么？在当前上下文中仍然成立吗？"

**3.5.2 通用接口纯度扫描**
- 扫描共享结构体、trait、公共 API 的字段/方法
- 对每个元素提问："此字段/方法对所有消费者上下文都有语义意义吗？"
- 标记仅在特定上下文（架构、模块、进程类型）中有意义的元素
- **检查口令**："如果消费者是 aarch64/riscv64/其他模块，这个字段还有意义吗？"

**3.5.3 返回值完整性扫描**
- 扫描所有外部调用（固件、系统调用、库函数、硬件抽象 trait 方法）
- 检查返回值是否被：① 使用 ② 传递 ③ 显式注释说明可安全丢弃
- 标记被忽略但未说明理由的返回值
- **检查口令**："这个外部调用的返回值被忽略了吗？忽略是安全的吗？"

**3.5.4 资源生命周期闭环扫描**
- 扫描资源获取点（分配、映射、打开、租借）
- 检查是否有对应的释放点，或显式声明"不释放"的理由
- **检查口令**："这个资源谁负责释放？什么时候释放？如果'不释放'，理由是什么？"

**3.5.5 理由可质疑性扫描**
- 扫描所有包含"为什么"解释的注释（如"因为/由于/避免/为了/需要"）
- 输出"以下理由需人工质疑"标记列表
- **检查口令**："这个理由在当前上下文中成立吗？有没有更诚实的说法？"

**中间产物**：
```markdown
### Step 3.5 产物：细节精确性可疑点标记

| 元规则 | 位置 | 可疑内容 | 人工确认建议 |
|--------|------|---------|------------|
| 外部知识 | L942 | "CR4.PSE 支持 2MB 大页" | 验证 x86-64 长模式下 PSE 与 2MB 页的关系 |
| 通用接口 | L876 | `syscall_entry: VirBytes` | 确认是否对所有架构有意义 |
| 返回值 | Lxxx | `exit_boot_services()` 返回 `_mmap` | 确认丢弃最终内存映射是否安全 |
| 资源闭环 | Lxxx | `Box::leak(memmap)` | 确认泄漏后的回收路径 |
| 理由质疑 | L872 | "u64 避免 32 位截断" | 微内核是否真的需要担心 4GB 截断 |
```

### Step 3.5a: 纵向链路检查（Vertical Link Check，文档 Review 强制）

> **目的**：Step 2.5 的 Link Validation 检查"横向链路"（Ch3→Ch4→Ch5 之间），本步骤检查"纵向链路"——Ch1 概念 → Ch3 设计决策 → Ch4 实现 → Ch5 测试 的端到端可追溯性。
> 来源：03-kmain-cstart

**检查项**：
1. Ch1 引入的每个核心概念 → Ch3 是否有对应设计决策？无 → P1（概念无落地）
2. Ch3 每个设计决策 → Ch4 是否有对应实现？无 → P1（决策无实现）
3. Ch4 每个核心类型/函数 → Ch5 测试是否覆盖？无 → P1（实现无测试）
4. Ch5 每个测试 → 是否能追溯到 Ch3 设计决策？无 → P2（测试无设计依据）

**输出格式**：
```markdown
### Step 3.5a 产物：纵向链路检查

| Ch1 概念 | Ch3 决策 | Ch4 实现 | Ch5 测试 | 链路完整? |
|---------|---------|---------|---------|----------|
| 保护结构 | §3.2 ProtectionArch trait | §4.1 ProtectionArchImpl | §5.2 test_protection | ✅ |
| CPU 三问 | §3.1 三问框架 | §4.2 prot_init | §5.1 test_three_questions | ✅ |
| ... | ... | ... | ... | ... |
```

**判定**：链路断裂 → P1；测试无设计依据 → P2。

**Step 3.5b: 因果链抽样验证（文档 Review 强制）**

> **目的**：从 Ch2 抽样"为什么这样设计"的解释，验证其因果链每一步是否成立。
> **与 §2.0.3 的关系**：§2.0.3 是全量因果链验证（所有带"因为/所以"的 claim）；本步骤是聚焦 Ch2 设计解释的抽样验证。两者互补，不重复。

**执行步骤**：
1. 从 Ch2 抽取 5-10 个"为什么这样设计"的解释
2. 对每个解释，识别其因果链（A→B→C→结论）
3. 验证因果链每一步是否成立（用 C 语义/ISA 规范）
4. 失败 → P0（模式 48 因果链编造）

**输出格式**：
```markdown
### Step 3.5b 产物：因果链抽样验证

| Ch2 位置 | 设计解释 | 因果链 | 每步成立? | 判定 |
|---------|---------|--------|----------|------|
| §2.3 L45 | "memcpy 必要因为栈帧被覆盖" | 栈帧被覆盖→需复制 | ❌ (C 语义错) | P0 |
| §2.4 L78 | "refcount=0 因为初始化" | 初始化→refcount=0 | ✅ | ✅ |
```

### Step 3.6: 关联代码维度检查（文档 review 强制，2026-09-18 B3.3 新增）

> **目的**：full-review 的语义是"文档 + 关联 Rust 代码"一体审查（B3.1）。代码维度做没做、做到什么程度，必须有产物证据，不由执行者临场裁量。
> **输入**：Step 0 产出的关联代码清单（`tools/doc-code-map.sh`）。
> **检查维度**（按 [review-code-checklist.md](review-code-checklist.md) 执行，至少覆盖）：§1 Rewrite 质量、§2 硬件抽象、§4 执行模型/SMP、§8 命名、§13 设计-代码一致性、§14 C-Rust 语义对齐。
> **附带证据行**（gate-evidence-code 块内的固定行）：
> - "代码可读性增量"：`cargo clippy -p {crate} --lib --no-deps -- -D warnings` 与 `tools/unsafe-audit.sh --diff` 的结果（`--no-deps` 必需：workspace 依赖的既有告警会让 `-D warnings` 恒 FAIL，2026-10-08 R8-D3）
>   （G1 卡已于 2026-10-07 判定作废并合并：`pub` 项与模块缺文档注释属 rustc 的 `missing_docs` 射程（落点在 G2），
>   `unsafe` 无 SAFETY 由 unsafe-audit 覆盖；新增代码可读性违规 = 正确性 P1。不得引用 `tools/code-style-lint.sh`——该脚本不存在）
> - "非法态封堵"：清单内核心状态机的封堵结论（H2；code-excellence §16.6）
> - "unsafe 审计"：`tools/unsafe-audit.sh --baseline tools/unsafe-baseline.txt --diff`（D2：新增裸 unsafe = 0 才通过；命令 / 裸 unsafe 总数 / 与基线差值 / 新增违规清单）
> - "文档代码块审查"：文档 Rust 块四分类表（H3；doc-checklist §2.4k）

**产物**：维度 × 文件 × 结论 × 证据 表 + `gate-evidence-code` 块（命令 / exit / stdout_key / artifact 路径 / date，格式见 `prompt/WORKFLOW-OPEN-ITEMS.md` F-A）；无发现也要写明"已检查、无发现"。发现的问题沿用既有 P0/P1/P2 判定（不新增 P0 分类）。

### Step 4: Cross-Document Check（跨文档联动）

> **范围限定**：优先检查本文档所在目录下的其他文档，其次检查"参见"章节引用的外部文档。

- 检查本文档涉及的概念/类型/函数是否在同目录其他文档中已有定义
- 如果同目录其他文档已有完整设计和实现，检查本文档是否：
  - 正确引用了那些文档？
  - 与那些文档的描述一致？
  - 没有重复定义或矛盾？
- 检查本文档"参见"章节引用的外部文档：
  - 引用的文档是否存在？
  - 引用的内容是否与外部文档一致？
- 特别关注：
  - 共享的数据结构（如 `vmproc`、`vir_region`）
  - 共享的常量/配置（如 `CLICK_SIZE`、`VM_RQ_BASE`）
  - 跨模块调用（如 PM → VM 的 IPC 接口）
- **重复处理检查**：如果同目录其他文档已经完整处理了某个概念（分析+设计+实现），本文档是否又重复处理了一遍？如果是，应精简为引用而非重述

#### 4.1 语义归属判定（grep 辅助）

> **目的**：当同一个 C 函数/结构体/宏可能属于多个文档的语义范围时，通过 grep 搜索 + 语义分析确定其归属文档，避免遗漏或重复。

**执行步骤**：

1. **提取关键符号**：从当前文档的 Ch1&2 中提取所有涉及的 C 符号（函数名、结构体名、宏名）
2. **grep 跨文档搜索**：在同目录下所有 `.md` 文件中搜索这些符号
   ```bash
   rg "SYMBOL_NAME" {tree}/ --type md -n
   ```
3. **语义归属判定**：对每个符号，根据其**语义本质**判断应归属哪个文档：
   - 如果符号的语义与当前文档主题直接相关 → 当前文档应完整覆盖（分析 + 设计 + 实现）
   - 如果符号的语义属于同目录其他文档的主题 → 应在那个文档中完整覆盖，当前文档仅保留引用
   - 如果符号在任何文档中都未被覆盖 → 判定归属后，在对应文档中补充

**判定原则**：
- 以**功能语义**为准，而非以"哪个文件定义了它"为准
- 例如：`vm_mappages` 定义在 `mmap.c`，但语义上属于"页表操作"，应归入 `07-pagetable-ops.md`
- 例如：`pt_t` 定义在 `proto.h`，但语义上属于"页表结构"，应归入 `06-pagetable-struct.md`

**输出格式**：
```
| 符号 | 类型 | 语义归属 | 当前覆盖状态 | 处理建议 |
|------|------|---------|-------------|---------|
| vm_mappages | 函数 | 07-pagetable-ops.md | 未覆盖 | 在 07 中补充分析 |
| pt_t | 结构体 | 06-pagetable-struct.md | 已覆盖 | 无需处理 |
| ARCH_VM_PTE_PRESENT | 宏 | 06-pagetable-struct.md | 07 中重复 | 07 精简为引用 |
```

#### 4.2 Design Quality Check（设计质量检查）

> **目的**：在跨文档联动确认后，对 Ch3 设计决策做质量评估。这步依赖 Step 2.5 的链路验证结果。
> 详细规则见 [review-doc-checklist.md §2.9](review-doc-checklist.md#29-设计决策质量检查ch3-专项强制不可跳过)。

1. 列出 Ch3 的所有设计决策
2. 对每个设计决策，检查：
   - 是否有 Ch1&2 的依据？（可追溯性）
   - 是否考虑了替代方案？（合理性）
   - 是否覆盖了 Ch2 的所有场景？（完整性）
   - 是否在 `no_std` 下可行？（可实现性）
3. 如果发现更好的替代方案，要求在 Ch3 添加 TODO 段落

> **注意**：局部 Review（仅 Ch1&2）时跳过此步骤。

#### 4.2.1 Rust 代码设计质量检查

> **目的**：在文档设计质量检查之外，对 Rust 代码中的 trait/类型设计做独立评估。
> 详细规则见 [review-code-checklist.md §2.5](review-code-checklist.md#25-trait-设计质量评估)。

1. 列出代码中所有自定义 trait
2. 对每个 trait，检查：
   - 是否有 ≥2 个行为不同的实现？（多态必要性）
   - 是否被用作 trait bound？（多态使用）
   - 方法在所有架构上的实现是否真的不同？（跨架构差异）
   - 是否混淆了机制和策略？（职责分离）
3. 如果发现不必要的 trait，标记为 P1，建议简化

> **注意**：此检查在完整 Review（模式 C）时执行，局部 Review 时跳过。

### Step 4.5: Test Verification（测试章节验证）— **Gate E**

> **适用条件**（2026-08-15 修复 A-P1-2 明确触发语义）：文档含 §5 测试章节（或类似测试要点章节）时执行。触发条件为文档标题含 `^## §?5(\.|\s)|^# 5(\.|\s)|测试章节|验证章节` 任一模式；纯设计文档 / 局部 Review（仅 Ch1&2）跳过。

**执行步骤**：
1. 提取文档 §5 列出的所有测试函数名
2. 对每个测试函数名执行 grep：
   ```bash
   rg "fn {test_name}" {rust_dir} --type rust -n
   ```
3. 判定：
   - 存在 → ✅
   - 不存在 → ❌ **P0（测试缺失）**

**中间产物**（Gate E）：
```markdown
### Step 4.5 产物：测试章节验证（Gate E）

| 文档 §5 测试名 | grep 命令 | grep 结果 | 判定 |
|---------------|----------|----------|------|
| test_vmctl_clear_pagefault | `rg "fn test_vmctl_clear_pagefault" os/` | 0 matches | ❌ P0 缺失 |
| test_vmctl_param_from_u32 | `rg "fn test_vmctl_param_from_u32" os/` | vm.rs:1333 | ✅ 存在 |

**统计**：文档承诺 N 个测试，实际存在 M 个，缺失 N-M 个 → P0
```

> **若文档无 §5**：输出 "文档无 §5 测试章节，Gate E 不适用"，不阻断。

### Step 5: Final Review Output（最终输出）

- 按 [review.md §AI Review 输出模板](review.md#ai-review-输出模板) 整理所有发现
- 每个问题必须标注优先级（P0/P1/P2）
- 每个问题必须提供明确的修改方向
- **必须包含**：维度覆盖自检表格、最弱项自检、时间预算评估

### Step 5.5: 状态写入与收敛判断

> **G14（V3.9）**：doc 模式收敛前必须 `tools/review-units.py check --doc <当前篇>` 退出 0，
> 全部单元 frozen 且 T3 通过；旧 FREEZE 篇目 grandfather。

> **目的**：将当前 phase 的验证结果持久化写入工具对应的路径，并判断是否收敛。

**执行步骤**：
1. 创建或更新本运行时的 `STATE.md`（路径按 §Step 0 的运行时表取自己那一行，各运行时互不共享）：
   `.review/{tool}/{stage}/STATE.md`
2. 创建或更新 `SYMBOLS.md`（Step 1.5 产物）到对应路径
3. **所有维度结果写入 scan.md 单文件**（NOT 10 个维度检查文件）。若用户显式指定输出位置（如交互式修复），双写到用户指定路径（被 review 文档同目录下 `{doc-stem}-trae-review.md` / `{doc-stem}-claude-report.md`）+ 工具默认路径（`scans/` 内）。双写校验见 Gate 0 Artifact Inventory。
4. 将 scan.md 中**新发现 P0/P1/P2** 同步到 STATE.md 的 Open P0/P1/P2 列表；已修复问题移入 Closed Issues 段落。
5. 更新 STATE.md 的 Phase Completion Log 和 Convergence Checklist
6. 输出收敛状态评估
7. **Severity Reconciliation 表**：若有跨轮次严重性分歧，记录降级/升级理由 + C 源/设计文档 `file:line` 证据

**收敛终止条件**（全部满足才算审查完成）：
1. scan.md 中所有维度章节标记 COMPLETE
2. 最近一次完整 Pass 中，P0 新增数量 = 0
3. 最近一次完整 Pass 中，P1 新增数量 = 0（V3.7 出版标准 §三.1；不得以 ≤1 判收敛）
4. **Gate G 独立验证（VERIFY-CHECK.md）结果为 PASS**（**必须完成，不能跳过**）
5. scan.md / STATE.md 中所有 P0 已被修复并验证通过
6. SYMBOLS.md 覆盖率穷举完成
7. **Blocker Gates 0/A/B/C/D/D-6/D-Impl/E/G/H 全部通过且有 gate-evidence 附件**（2026-08-15 修复 C-P0-2：明确 Gate H 属于 Blocker Gates 之一，所有 review 模式必检）
8. **出版与零欠账门**：`review-publication.md` §五 的 G10–G14 与零欠账全部通过（V3.7；任何命令不得裁剪）

### Gate H: design 门控

**触发条件**：所有 review 模式。Profile D/A/G 可以裁剪内容检查，但不能跳过 Step 0 预检或 Gate H。

> **快照不是门（V3.10）**：`meta/design/<stage>/{NN}-design*.v*.md` 与 `{NN}-outline*.v*.md` 是写作输入，不是 review 的前置条件。没有快照不 FAIL、不补生成；有计划/大纲时可拿来对照，但以 C 源码与当前代码为准。

**检查项**（4 项，只查语义）：
- [ ] **H.2**：design ↔ code 一致性 ≥ 80%（Step 1.6.2 矩阵）；无 design 文档时改为“代码 ↔ C 语义一致性”矩阵。
- [ ] **H.3**：design ↔ Minix3 对齐无 P0 缺失（Step 1.6.3 矩阵）；无 design 时查代码是否有 Minix3 概念缺失。
- [ ] **H.4**：design 无 P0-design-wrong（design 错而非实现错）。
- [ ] **H.5**：design-deviation 与 design-wrong 区分清楚（避免误判 Refactor 类型）。

**允许 N/A + 理由**：纯算法文档、历史/概念文档无 Rust 实现可对照时，H.2–H.5 可标“不适用 + 理由”，记录在 STATE.md；不适用 ≠ 跳过，理由必须具体到缺哪类对照物。无 design 文档但不是纯概念文档时，H.2/H.3 改用 C 源码矩阵，不标 N/A。

**状态机**：
```
[开始] → 有 design 文档 → 对照 design → PASS / FAIL
       → 无 design 文档 → 对照 C 源码与代码 → PASS / FAIL（记 N/A 理由）
```

**PASS 条件**：H.2 + H.3 + H.4 + H.5 全部 ✅，或按上文 N/A + 具体理由。

**FAIL 后果**：
- design 错误（design-wrong，H.4）→ 阻断进入 Step 2，触发 design Refactor。
- 代码偏离 design（H.2）→ code Refactor；两者都错 → design Refactor + code Refactor。
- design-wrong → 记录到 scan.md 的 IN_DESIGN 状态 + 触发 Profile R（设计优先模式）。

**与 Gate D-6 的关系**：
- Gate D-6 检查 structure.md（单文档骨架，review 内部产物）。
- Gate H 检查 design 语义一致性（有 design 文档时）或代码 ↔ C 语义一致性（无 design 时）。
- 两者独立，但都必须在 review 进入 Step 2 前通过。

**最小证据要求**：
- **H.2**：一致性矩阵输出 ≥ 5 行 + 一致性百分比 ≥ 80%（Step 1.6.2）。
- **H.3**：Minix3 对齐矩阵输出 ≥ 3 行 + 无 P0 缺失（Step 1.6.3）。
- **H.4**：`rg "design-wrong" scan.md` 无命中（或有命中但已附 design 修复计划）。
- **H.5**：`rg "code Refactor|design Refactor" scan.md` 输出明确区分两类。

**禁止**：
- ❌ 用 `find . -name "*design*.md"` 跨目录找快照充当 design 依据。
- ❌ 仅“非空”作为通过条件（量化阈值缺失）。
- ❌ H.2–H.5 任一未提供矩阵/输出即标 PASS。

### IN_DESIGN 状态机

```
PENDING → IN_PROGRESS → CONVERGED
                ↓
            IN_DESIGN（review 中断去 design）
                ↓
            RESUMED → IN_PROGRESS → CONVERGED
```

**IN_DESIGN 状态判定**：
- scan.md 中含 `## Review Status: IN_DESIGN`
- STATE.md 中 `Open P0` 包含 design-missing/wrong
- IN_DESIGN.md 存在（`.review/{tool}/{stage}/IN_DESIGN.md`）

**收敛条件**（增加 IN_DESIGN 路径）：
- 标准路径：所有 P0 修复 → CONVERGED
- IN_DESIGN 路径：design 完成 + Gate H PASS + P0-fact 修复 → CONVERGED

### §五.附录 D：IN_DESIGN 时间上限 + 月度审计

> **目的**：避免 IN_DESIGN 状态变成长期 DEFERRED 逃避。

**时间上限机制**：

| IN_DESIGN 时长 | 状态判定 | 后续动作 |
|---------------|---------|---------|
| 0-3 天 | 正常 | 继续 IN_DESIGN |
| 4-7 天 | 警告 | 强制 P1（每周 review 必须检视 IN_DESIGN 列表）|
| > 7 天 | 强制 P0 | 必须升级为 design-wrong，由用户决定：放弃该 review / 合并到其他 review / 重新生成 design |

**健康度审计**：

**审计频率**：
- 月度审计：每月第一个工作日扫描所有 STATE.md 中的 IN_DESIGN 状态
- 周度抽查：每周随机抽 20% IN_DESIGN 模块做快速检视
- 季度复盘：每季度评估 IN_DESIGN 状态机是否需要调整

**审计命令清单**：
```bash
# 1. 列出所有 IN_DESIGN 状态
find .review -name "IN_DESIGN.md" -exec head -5 {} \;

# 2. 统计 IN_DESIGN 时长
grep -rE "IN_DESIGN.*(\d+ 天)" .review/

# 3. 检查 IN_DESIGN 占比
find .review -name "scan.md" | wc -l  # 总 review 数
find .review -name "IN_DESIGN.md" | wc -l  # IN_DESIGN 数
# IN_DESIGN 占比应 ≤ 20%

# 4. 标记超期 IN_DESIGN
grep -lE "IN_DESIGN.*30 天" .review/*/IN_DESIGN.md
```

**审计处理流程**：
1. 发现 IN_DESIGN > 7 天 → 自动标记 P0-design-evading
2. 发现 IN_DESIGN > 30 天 → 强制清理（除非用户书面延期）
3. IN_DESIGN 占比 > 20% → 暂停新 review 启动，全员清理
   - **v6 豁免规则**：项目初期（第一个月）豁免此规则，允许 design 普遍缺失时仍启动 review
   - **v6 豁免规则**：按模块分别计算，不跨模块合并
   - **v6 豁免规则**：用户可在 STATE.md 顶部标注 `## Design 集中补齐期` 期间豁免

**审计记录表**（每月填写）：
```markdown
## IN_DESIGN 健康度月度审计（YYYY-MM）

| 模块 | IN_DESIGN 起始 | 持续天数 | 状态判定 | 处置 |
|------|---------------|---------|---------|------|
| module-A | 2026-06-01 | 30+ | 强制清理 | 已合并到 module-B review |
| module-C | 2026-07-10 | 5 | 警告 | 每周检视 |
| 总计 | — | — | 2 个 IN_DESIGN / 10 总 review（20%）| OK |
```

**判定信号**：
- ❌ 同一模块的 IN_DESIGN > 7 天 → P0-design-evading（逃避 design）
- ⚠️ 同一 review 周期内 IN_DESIGN > 3 次 → P1-process-violation（流程违反）

### 收敛判定（统一 G1–G14）

Step 5.5 的出口判据只有一套：P0 六类全部修完 +
Blocker Gates 0/A/B/C/D/D-6/D-Impl/E/G/H 全 PASS；卓越性缺口按 G2 的 P1/P2
入 Issue List 并当轮处置。G1–G14 的定义与四态模型只在 `review-evals.md` §6A
维护，本文件不复制门列表。

### Step 5.6: Review Verification Protocol（独立验证，强制）

> **目的**：解决"自己审自己"的盲区。在所有维度 COMPLETE 后，必须执行独立验证才能标记 CONVERGED。

**触发条件**：所有维度标记 COMPLETE + P0/P1 收敛后，**必须**执行本步骤并生成 VERIFY-CHECK.md。未执行 VERIFY-CHECK 时，状态必须为 NOT_CONVERGED。

**执行方式**（独立会话中执行）：
1. Agent 读取 STATE.md + scan.md + 原始文档/代码
2. **随机抽样**：从 scan.md Issue List 中随机选取 20% 的已报告问题
3. **反向验证**：对每个抽样问题，独立重新验证——source evidence 是否充分？判定等级是否合理？
   - **2026-08-15 修复 C-P1-5（明确抽样方法）**：AI 无内置"随机"，使用**分层抽样**替代——从 Issue List 中按 P0/P1/P2 比例各取头 20% + 尾 20% + 中间 20%。例如 Issue List 共 20 条（P0=3, P1=12, P2=5）→ P0 取第 1 条 + 最后 1 条 = 2 条；P1 取第 1/6/12 条 = 3 条；P2 取第 1 条 = 1 条，合计 6 条（30%）。**禁止**仅抽 P0（应全层级覆盖）
4. **遗漏检查**：抽样 20% 的源码符号（函数/结构体/宏），验证是否都在文档/检查中覆盖了
5. **收敛验证**：检查 STATE.md 的 Convergence Checklist 是否有"标记 COMPLETE 但实际未完成"的维度
6. **Blocker Gates 复验**：检查 scan.md 中 Gate 0/A/B/C/D/D-6/D-Impl/E/G/H 是否都附带真实证据（gate-evidence 块）
7. **跨 agent 验证推荐**（新增，2026-07-16）：若条件允许，优先由**不同 agent** 执行 VERIFY-CHECK（如 trae 内 glm 的 scan 由 kimi/ds 验证）。同 agent 验证时必须基于 grep 命令重放（非语义回忆），降低同 agent 系统性盲区风险。跨 agent 验证结果记入 VERIFY-CHECK.md 的"验证局限说明"段。
8. **输出判定**：
   - **PASS**：抽样验证一致性 ≥ 90%，无遗漏 key symbols，收敛状态可信，Gates 真实通过
   - **CONCERN**：抽样验证一致性 70-90% → 特定维度需重新审查
   - **FAIL**：抽样验证一致性 < 70% 或发现关键遗漏 → 整体重新审查

**输出**：写入工具对应的 VERIFY-CHECK.md 路径（Trae: `.review/trae/{stage}/VERIFY-CHECK.md`; Claude: `.review/claude/{stage}/VERIFY-CHECK.md`）。VERIFY-CHECK.md 必须包含"验证局限说明"段，标注验证者（同 agent / 跨 agent）及验证方法（grep 重放 / 语义回忆）。填完 §2.1 表后，跑
`python3 tools/verify-check.py --finalize {VERIFY-CHECK.md}`：仍有（待填）或一致性 <90% 返回非 0，不得标记 CONVERGED。

**Multi-Agent 强制规则（NEW 2026-07-16）**：

| 报告 P0 数 | 推荐验证 agent | 阻断? |
|-----------|---------------|-------|
| 0 | 同 agent 可（带 grep 重放） | ❌ 不阻断 |
| ≥ 1 | **优先跨 agent**；**环境只有一个 agent 时，同 agent + grep 命令重放可接受**（须在 VERIFY-CHECK.md「验证局限说明」段显式标注，AGENTS.md 同此口径） | 仅当「有第二 agent 却用了同 agent 且未标注」时阻断 |

> **口径统一（2026-10-08 干跑 R4-D6 裁决）**：本节表格旧文写作「≥1 必须跨 agent」，与同节第 7 条及
> AGENTS.md 的「VERIFY-CHECK 的同 agent 局限」段自相矛盾（单会话/单运行时根本不存在第二 agent，
> 照字面执行只能造假或停工）。**以本行为准**：跨 agent 是首选而非绝对前提，同 agent 验证的硬要求是
> ① 重放 grep 命令（非语义回忆）② 在 VERIFY-CHECK.md 标注局限 ③ consistency < 90% 写 NOT PASS。

**触发场景**：
- 若 review 报告 P0 ≥ 1 → Gate G VERIFY-CHECK 必须由不同 agent 二次验证
- Trae 内允许：glm → kimi / glm → ds / kimi → seed 等轮换
- Claude 内允许：m3 review → glm-flash verify（独立 session）
- 同 agent 验证时必须基于 grep 命令重放（非语义回忆），并在 VERIFY-CHECK.md 标注"同 agent 验证，已重放 grep 命令"

**工具层触发**：
- `tools/review-init.sh --require-multi-agent` 启用（默认 false）
- 若启用，review-init 输出"⛔ 已设置 --require-multi-agent：若本 review 报告 P0 ≥ 1，Gate G 必须由不同 agent 执行 VERIFY-CHECK"

**判定**：报告 P0 ≥ 1 但 VERIFY-CHECK 由同 agent 写 → Gate G 判 FAIL → STATE.md 不得标 CONVERGED。

### Step 5.7: Rule Discovery（规则发现，强制填写）

> **目的**：将 review 中发现的新模式反馈到规则集，实现规则演化。
> **详见**：[review.md §规则演化机制](review.md#规则演化机制rule-evolution)。

> **2026-08-15 修复 B-P1-2（误判 P0 回退流程）**：若 review 报告的 P0 是 AI 误判或事后被证明虚假，应按以下流程回退：
> 1. **触发场景**：
>    - 用户反馈"P0 是误判"（如 grep 证据有误、概念引用错误）
>    - 跨 session 验证（VERIFY-CHECK.md）发现 P0 不成立
>    - AI 在修复时发现 P0 描述与实际不符（如 file:line 引用错位）
> 2. **回退流程**：
>    - **立即移除**：scan.md §Issue List 中删除该 P0
>    - **同步 STATE.md**：从 Open P0 列表移除，移入 Closed Issues 并标 "WITHDRAWN" + 原因
>    - **修正规则**：若是 grep 命令误用导致误判 → 写入 Rule Discovery 段
> 3. **预防机制**：
>    - P0 必须有 L1 grep 证据（路径存在 + 行号匹配），缺证据 → 自动降级为 P1
>    - AI claim verification 三元组（Step 0.7.1/0.7.2/0.7.3）必须全跑，未跑 → 阻断 P0 报告
>    - VERIFY-CHECK.md 必须抽样 20% 反向验证（Step 5.6）

**执行步骤**：
1. 回顾本轮 review 发现的所有问题
2. 判断是否有 ≥2 次同类新模式（现有规则未覆盖的）
3. 若有 → 生成新模式提案（含案例、判定、归类、规则草案）
4. 写入 scan.md §Rule Discovery 段落
5. 用户确认后，落地到对应 rules 文件

**输出格式**：
```markdown
### Rule Discovery
- 本次 Review 是否发现新模式？[✅/❌]
- 若 ✅：
  - 新模式名: [名称]
  - 案例: [file:line + 描述]
  - 判定: [P0/P1/P2]
  - 归类: [文档/代码/跨阶段/卓越性/叙事概念]
  - 规则草案: [一句话描述]
  - 建议落地文件: [review-patterns.md / review-doc-checklist.md / ...]
```

### Step 6: Action Item Generation（修改项生成）

> **目的**：将 review 发现转化为可执行的修改项，确保代码会被实际修改。这是解决"review 完了代码不改"问题的关键步骤。

对每个 P0/P1 问题，必须生成：
1. **问题描述**：什么问题
2. **修改方向**：应该怎么改
3. **影响范围**：涉及哪些文件（文档 + 代码）
4. **验证方法**：修改后如何验证

**格式**：
```
### TODO #N: [简述]
- **优先级**: P0/P1
- **类型**: 设计缺陷 / 代码-设计不一致 / no_std 违规 / 语义偏移 / ...
- **文件**: `path/to/file.rs`
- **问题**: [详细描述]
- **修改方案**: [具体方案]
- **验证**: [如何验证修改正确]
```

**关键**：P0 问题必须生成代码修改项，不能只停留在"建议修改"。
P1 问题如果涉及设计改进，必须在文档 Ch3 添加 TODO 段落描述替代方案。

### Step 7: 自检清单确认（强制）

> **目的**：在输出最终结果前，AI 必须逐条确认以下清单。这是防止"输出看起来完整但实际漏了关键维度"的最后一道防线。
> **模式适配**：构造/快速模式跳过的 Step 标注"跳过（模式）"，深度模式必须全部确认。

```markdown
### Step 7 产物：自检清单

- [ ] Step 0 执行模式已声明 + 范围声明和时间预算已输出（或已说明省略）
- [ ] Step 0 已读取本运行时的 STATE.md（按 §Step 0 运行时表取 `{tool}`）
- [ ] Step 1 源码文件清单已输出
- [ ] Step 1.5 覆盖率穷举已输出（SYMBOLS.md + 缺口/ARCH 判定）〔构造/深度必做，快速跳过〕
- [ ] **Gate 0**: 制品完整性（scan.md 含 9 个 grep 可验锚段 + 标准路径文件齐全）
- [ ] **Gate A**: coverage-extract.py 已运行且 scan.md 附 SYMBOLS.md 路径 + gate-evidence-A 块
- [ ] Step 2 Top 3 差异 + Top 2 覆盖缺口已输出（**8 字段 × 5 函数**行为契约表）
- [ ] **Gate B**: Top 5 行为契约表已产出（**8 字段 × 5 函数**：函数名/C行为/Rust行为/差异类型/严重度/C证据/Rust证据/Reviewer备注）
- [ ] Step 2.5 链路验证表格已输出（如适用）〔深度必做，构造/快速跳过〕
- [ ] Step 3 概念准确性表格已输出〔深度必做，构造/快速跳过〕
- [ ] Step 3 C 代码引用验证表格已输出〔深度必做，构造/快速跳过〕
- [ ] Step 3 数据结构覆盖表格已输出〔深度必做，构造/快速跳过〕
- [ ] Step 3 C 源码覆盖完整性表格已输出（含覆盖率）〔深度必做，构造/快速跳过〕
- [ ] Step 3 文档风格验证表格已输出（§2.11）〔深度必做，构造/快速跳过〕
- [ ] **Gate C**: Step 3.5 Precision Check 5 元规则检查表已产出
- [ ] Step 4 跨文档检查已输出〔深度必做，构造/快速跳过〕
- [ ] Step 4.1 设计决策质量表格已输出（如适用）〔深度必做，构造/快速跳过〕
- **Gate D**: P0 必检清单 5 项已回答 ✅/❌ + grep 证据（PARTIAL/⚠️ 一律视为 ❌，与 [review-patterns.md §0 L19-22](review-patterns.md) 一致）
- [ ] **Gate D-6**: structure.md 已生成并评审（文档 Review 强制，Step 0.5 产物；含 Step 0.5.1 12 节模板 + Step 0.5.2 评审 + Step 0.5.3 outline 对齐 + Step 0.5.4 通过门槛 + Step 0.5.5 跨章节一致性 + Step 0.5.6 6 维反查 + Step 0.5.7 章节意图 + Step 0.5.8 issue 反查来源）
- [ ] **Gate E**: §5 测试函数名已 grep 验证（§5 = "测试/验证"章节标题。2026-08-15 修复 A-P1-2：触发条件为文档含 `^## §?5(\.|\s)|^# 5(\.|\s)|测试章节|验证章节` 任一标题模式；纯设计文档 / 局部 Review（仅 Ch1&2）跳过）
- [ ] **Gate H**: H.2–H.5 语义项已给出矩阵/证据（无 design 文档时用 C 源码矩阵或 N/A + 理由）
- [ ] Step 5 维度覆盖自检表格已输出
- [ ] Step 5 最弱项自检 **5** 个问题已确认
- [ ] **收敛判定已按 G1–G14 输出**
- [ ] Step 5 时间预算评估已输出（或已说明省略）
- [ ] Skill Invocation Log 已输出（真实 tool 调用记录）
- [ ] Step 5.5 scan.md 单文件已写入 + STATE.md 已更新 + Open P0/P1/P2 已同步
- [ ] **Artifact Inventory 已输出（Gate 0 校验项，含双写校验）**
- [ ] **Severity Reconciliation 已输出（若有跨轮次严重性调和）**
- [ ] Step 5.6 VERIFY-CHECK.md 已生成（Gate G PASS，收敛终止必要条件）
- [ ] Step 5.7 Rule Discovery 已填写（是否发现新模式 ✅/❌ + 草案）
- [ ] Step 6 修改项已生成（P0 必须有代码修改项）〔构造/深度必做，快速跳过〕
- [ ] 所有 grep 命令的输出已作为证据附在对应表格后
```

> **如果以上任何一项未完成（且非模式跳过），AI 必须回到对应 Step 重新执行，不得跳过。**

### Step 7.05: 零输出检查复跑（强制）

报告前统计每个检查的证据行数。零输出不是“无问题”，而是“可能没执行”：
- Checklist 中的每个检查至少 1 行证据；零输出 → 立即重跑该项。
- 重跑后仍无法执行 → 写 `NOT RE-RUN: <reason>`，按 P1-process 登记，禁止无声跳过。
- 复跑记录写入 scan.md 的 `## Zero-Output Recheck` 段（检查名 / 是否重跑 / 结果）。

### Step 7.1: 收敛成本警告（强制）

> **目的**：防止"过度收敛"——为了把 P1 降到 0 而反复 review，成本超过收益。
> 来源：用户反馈

**判定规则**（任一触发即应停止并交付）：
1. **轮次阈值**：同一篇修改轮达到 3 轮（V3.8 硬上限，跨 edition 累计）→ 停止修改、整节重写；不得以 backlog 交付（`review-publication.md` §三）
2. **P1 边际递减**：连续 2 轮新发现 P1 ≤ 1 → 停止继续寻找新问题并交付；状态仍按 §五 判定（P1 = 0 才可 CONVERGED），不得转 backlog
3. **成本/收益比**（2026-08-15 修复 A-P1-3 加权定义）：当前轮 review 耗时 > 上一轮 80% 但加权新发现问题 < 上一轮 20% → 停止
   - **加权公式**：`weighted_new = P0_count * 10 + P1_count * 3 + P2_count * 1`
   - **首轮不适用**：第一轮 review 没有"上一轮"基准，自动跳过此条
   - **判定**：本轮 `weighted_new < 上一轮 weighted_new * 0.2` 且 本轮耗时 > 上一轮耗时 * 0.8 → 停止
4. **首次零发现**：首次 review 0 P0/P1/P2 → 触发**漏检自检**：随机抽 3 个检查项重跑（推荐：Gate D 第 1/3/5 项 + Step 2 因果链抽样）；若仍 0 发现 → 交付；若发现遗漏 → 之前的 review 标记 DRAFT，补完后再交付
5. **卫生项排除（2026-09-18 D5）**：卫生项（锚点提示过期、全角/半角、拼写、格式等机械项）只记录 + 批量修，**不进 P1/P2 计数、不进 `weighted_new`、不参与本步收敛判定**；scan.md Issue List 单列"卫生项"分区。唯一例外：符号锚点 0 定义（符号消失/改名）按 P0-fact 处理——属正确性，不属卫生（见 Step 1.0）

**输出**：在 scan.md 末尾标注"收敛成本评估"：
```markdown
### 收敛成本评估
- 当前轮次: N
- 本轮新发现: P0=X, P1=Y, P2=Z
- 触发停止规则: [1/2/3/无]
- 决定: 继续收敛 / 停止寻找新问题并交付（状态按 §五 判定，不转 backlog）
```

---

### §一.附录 A：Review 中断协议

> **目的**：当 review 遇到 design 错误（design-wrong，design 存在但抓错本质）时，不是"放弃"也不是"DEFERRED 逃避"，而是"主动中断，等待 design"。
> **注意（V3.10）**：design 不存在不触发本协议，也不补生成；H.2–H.5 记 N/A + 理由。本协议只用于 design **存在但有错误**（design-wrong）场景。

**触发条件**（任一）：
- Step 1.6 发现 design 严重 outdated 或 design-wrong（design 不存在时不在此列）
- Step 2 发现 design 未覆盖的 Minix3 核心概念
- Step 3.5 发现 design 抓错了本质

**中断流程**（4 步）：

1. **标注 scan.md 状态**
   ```markdown
   ## Review Status: IN_DESIGN

   **Review 中断原因**: design.md 存在但 design-wrong（抓错本质）
   **中断时间**: 2026-07-15
   **设计优先级**: P0（核心阻塞）
   **恢复条件**: design.md 生成并通过 Gate H
   ```

2. **生成 IN_DESIGN.md**（在 `.review/{tool}/{stage}/` 下）
   ```markdown
   # IN_DESIGN.md — Review 中断：等待 design

   ## 待 design 覆盖的概念清单
   - [ ] 三类运行态实体区分
   - [ ] 进程表核心抽象
   - [ ] ...

   ## 当前 review 已发现问题（design 修复后可继续 review）
   - P0-fact: 03.md §1.8 表 D 行 outdated
   - P0-code-bug: opensbi_helpers.rs:229 PhysBytes(0)

   ## Design 完成标准
   - [ ] design.md 存在（非 bagging）/ design-final.md 存在（bagging）
   - [ ] Gate H PASS
   - [ ] 与 Minix3 概念对齐（Step 1.6.3）
   ```

3. **不动任何文档/代码**（避免半成品状态）

4. **design 完成后恢复**
   ```markdown
   ## Review Status: RESUMED (after design OK)
   ```

**与 DEFERRED 的根本区别**：

| 维度 | DEFERRED | IN_DESIGN |
|------|---------|-----------|
| 含义 | 逃避问题 | 主动承认需要先 design |
| 状态 | 不解决 | 解决中 |
| 恢复 | 不需要恢复 | design 完成自动恢复 |
| 风险 | 累计技术债务 | 暂停 review 等待 design |

### §一.附录 A.2：跨 session 续审协议（Resume Point 标准化）

> **背景**：大文档（>1000 行）+ 多 Rust 文件 + design 生成流程，单 session 上下文不够完成全量 review。需要标准化跨 session 续审协议，避免临时补丁导致信息丢失。
> **来源**：01-boot-shim-bootstrap Session #4→#5 续审实践验证。

**触发条件**（任一）：
- 上下文容量接近耗尽（>80%）
- 单 session 未能完成所有 mandatory Step（Step 0-5.6）
- 用户主动中断 review

**中断时强制动作**（当前 session 必须在 scan.md 末尾写入 Resume Point 段）：

````markdown
## Resume Point（跨 session 续审入口，下一 session 必读）

### 当前断点
- **Session**: #{N}（{日期}，{中断原因}）
- **完成度**: 见 scan.md `## Blocker Gates Status`
- **未完成**: 见 scan.md `## Issue List` 的 open 行 + STATE.md Convergence Checklist

### 下一 session 启动协议
1. 必读（按顺序）：scan.md（Issue List + Gate Status）、STATE.md、design.md（若 Gate H PASS）、
   被审文档与关联代码（按需读取，避免一次性耗尽上下文）。
2. 跳过：已完成的 Step；Gate H PASS 后的 design 脚手架不重读。
3. 从 `{下一 Step}` 开始：{下一 Step 描述}。

### 指针
- Issue 清单：`scan.md` `## Issue List` 第 {start}–{end} 行
- Gate 状态：`scan.md` `## Blocker Gates Status` 第 {start}–{end} 行
- 待执行 Step：`STATE.md` `Convergence Checklist` 未勾选项

### 必跑命令（缺一不可）
```bash
python3 tools/verify-check.py --preflight --tool {tool} --stage {stage} --doc-stem {doc-stem} --agent {agent}
python3 tools/verify-check.py --scan {scan} --state {state} --symbols {symbols} --units {units} --sample-ratio 0.2 --seed {seed}
python3 tools/eval-trace-check.py {run_dir} --round R{N} --units {units}
python3 tools/todo-ledger.py check --doc {doc} --fail-on-open
```
````

**Resume Point 必含字段**（缺一不可，缺失 = P1 流程违规）：
1. **当前断点**：Session 编号 + 中断原因
2. **下一 session 启动协议**：必读文件清单 + 跳过项 + 起始 Step
3. **指针段**：Issue List / Gate Status 的行号范围 + STATE.md 未勾选 Step
4. **必跑命令段**：preflight、verify-check、eval-trace、todo-ledger 四条命令

**下一 session 启动检查**（必须执行）：
1. 读取 scan.md 末尾 Resume Point 段
2. 确认所有"必读文件"可访问
3. 按清单从下一 Step 开始执行
4. 不重做已完成 Step（除非用户要求或发现已完成 Step 有错误）

**STATE.md 同步更新**：
- 中断时：STATE.md Session Status 标记为 `⏸ SUSPENDED（{原因}）`
- 续审完成时：STATE.md Session Status 标记为 `✅ CONVERGED`

### §一.附录 B：Scan 文件生命周期协议

> **背景**：review 流程产生脚手架产物（scan/structure/SYMBOLS/VERIFY-CHECK/IN_DESIGN），每次重新生成。`meta/design/` 下的 outline / design 文档是作者的可选写作输入，不再由 review 生成或刷新。

**产物位置与生命周期**：

| 类别 | 产物 | 位置 | 生命周期 |
|------|------|------|---------|
| 脚手架 | scan.md（草稿）| `.review/{tool}/{stage}/scans/{doc-stem}-{agent}-scan.md` | 每次重新生成，CONVERGED 后可清理 |
| 脚手架 | structure.md（review 骨架）| `.review/{tool}/{stage}/scans/{doc-stem}-{agent}-structure.md` | 每次重新生成，CONVERGED 后可清理 |
| 脚手架 | SYMBOLS.md | `.review/{tool}/{stage}/scans/{doc-stem}-{agent}-SYMBOLS.md` | 每次重新生成，CONVERGED 后可清理 |
| 脚手架 | VERIFY-CHECK.md | `.review/{tool}/{stage}/VERIFY-CHECK.md` | 每次重新生成，CONVERGED 后可清理 |
| 脚手架 | IN_DESIGN.md | `.review/{tool}/{stage}/IN_DESIGN.md` | design 解决后删除 |
| 可选输入 | outline / outline-review / design 文档 | 作者指定路径（常见 `{tree}/meta/design/{stage}/`）| review 只读；缺失不补、不改写、不作为 Gate H 条件 |

> **快照语义**：`meta/design/` 下的历史文档是“前人理解”，可以读，但不是 ground truth；review 结论以 C 源码与当前 Rust 代码为准。review 不要求刷新快照，也不检查快照是否存在。

**编号防泄漏规则**（核心）：
- scan.md 中的 `P0-XX-1` 编号**不应**出现在文档/代码中
- 若出现，review 必须标注 `TODO: 编号来自 scan.md，CONVERGED 后需清除`
- CONVERGED 后，scan.md 编号必须删除或迁入 design.md（非 bagging）/ design-final.md（bagging）作为正式 issue 编号

**强制要求**：
- ✅ 所有 P0/P1 编号仅在 scan.md 出现
- ❌ 文档中不得出现 `P0-XX-1` 格式编号（除非交叉引用 scan.md）
- ❌ 代码注释中不得出现 `P0-XX-1` 格式编号

**判定**：
- 文档含 `P0-XX-1` 编号 → P1-fact（必须清除或迁移）
- 代码注释含 `P0-XX-1` 编号 → P1-code-style（必须清除）

**grep 自动检测**：
```bash
# 文档/代码中不应出现 scan 编号
grep -rnE "P0-[0-9]+(-[0-9]+)?" {DOC_FILE} {CODE_FILE}
# 期望：无输出
```

### §一.附录 C：design 文档（可选输入）与修复协议

> **定位（V3.10）**：design 文档不是 review 的必交产物，也不是 Gate H 的存在性条件。作者已有 design 时，它作为对照输入；没有时 H.2–H.5 用 C 源码矩阵代替。下面是可选的 design 文档格式与 review 修复协议。

#### C.1 术语对齐（review-rules ↔ 文档写作）

| review-rules 用语 | 文档写作工作流用语 | 含义 |
|------------------|------------------|------|
| review Step 0/0.5/1/.../5.6 | — | review 流程的步骤 |
| — | 写作 Step 1 | 穷举知识点 → structure |
| — | 写作 Step 2 | 生成 outline（结构契约）|
| — | 写作 Step 3 | outline review |
| — | 写作 Step 4 | 基于 outline 写正文 |
| review 修复 | = 写作 Step 4（部分）| review 发现问题后基于 outline 的部分重写 |

#### C.2 review 修复协议（基于 outline 的部分重写）

| 问题严重度 | 修复方式 | 是否走 outline | 示例 |
|-----------|---------|--------------|------|
| **P0 概念错误** | Ch1/Ch3 重写 | ✅ 必须先更新 outline | Ch1 主语是函数名 → 重写 Ch1 |
| **P0 设计错误** | Ch3 重写 + 代码 Refactor | ✅ 必须先更新 outline | trait 设计错 → 重写 Ch3 + 修代码 |
| **P0 覆盖缺口** | 补充章节 | ⚠️ 需更新 outline 对应小节 | 漏 C 符号 → 补 Ch2 小节 |
| **P1 链路断裂** | 局部重写 | ⚠️ 视情况更新 outline | Ch3 决策无 Ch1 依据 → 补 Ch1 |
| **P1 细节不精确** | 局部修改 | ❌ 直接打补丁 | 错误行号/常量值 |
| **P2 可读性** | 局部修改 | ❌ 直接打补丁 | 命名/格式/交叉引用 |

P0/P1 重大修复：先更新 outline 对应小节 → 基于 outline 重写正文 → 跑质量检查 → 更新 scan.md。

#### C.3 design 文档格式（作者选择留档时）

位置：`{tree}/meta/design/{stage}/{NN}-design.v{N}.md`，版本化保留；也可放在作者自己的工作目录，review 只按路径读取。

```markdown
# {NN}-design.v{N}.md — {stage} 设计

> **状态**: DESIGN（非 bagging）/ FINAL（bagging）
> **版本**: v{N}
> **生成日期**: YYYY-MM-DD
> **来源**: C 源码 + 当前 Rust 代码 + OS 理论

## Ch1. 设计决策（核心抽象）
## Ch2. 与 Minix3 的语义对齐
## Ch3. Rust 类型系统表达
## Ch4. 已知限制与未来演进
```

#### C.4 与 IN_DESIGN 协议的衔接

| IN_DESIGN 阶段 | design 阶段 | 检查项 |
|---------------|------------|--------|
| IN_DESIGN.md 待 design 覆盖概念清单 | 对应 Ch1 设计决策 | 每条概念必须在 Ch1 找到对应决策 |
| IN_DESIGN.md 当前 review 已发现问题 | 对应 Ch2 语义对齐 | 每个 P0 在 Ch2 标注修复章节 |
| Gate H H.2（design ↔ code 一致性 ≥ 80%）| 通过 | Step 1.6.2 矩阵 |
| Gate H H.3（design ↔ Minix3 对齐）| 通过 | Step 1.6.3 矩阵 |

#### C.5 判定信号

- ❌ IN_DESIGN.md 中提到的概念在 design 文档找不到对应 → P0-design-missing。
- ❌ design 文档与 outline 矛盾 → P0-design-deviation。

### §一.附录 D：分阶段 Review（H–K）与 Profile R 别名

> 本附录保留仍需执行的分阶段检查序列，以及 Profile R 的别名映射。A–G/O/P/AG 的旧别名对账在 `review-cmds.md` §八。

**Profile H（阶段 1：Ch1&2 准确性）**：核心 + 文档检查清单（§2.1–§2.3、§2.5、§2.7、§2.8）+ 错误模式 1–9。检查概念准确性、C 引用、数据结构覆盖、架构演进说明、图表、C 源码覆盖。输出 P0 概念/引用/覆盖错误 + P1 架构差异。

**Profile I（阶段 2：Ch3&4 设计质量）**：核心 + 文档检查清单（§2.4、§2.9、§2.10）+ 错误模式 10–13 + 阶段 1 输出。检查 design ↔ code 一致性、设计决策可追溯性、章节链路。输出 P0 场景遗漏 + P1 链路断裂。

**Profile J（阶段 3：Rust 代码质量）**：核心 + 代码检查清单（§1–15）+ 错误模式 16–25 + 阶段 2 输出。检查 Rewrite 质量、硬件抽象、类型安全、执行模型、no_std、设计-代码一致性。输出 P0 UB/语义偏移 + P1 类型安全问题。

**Profile K（阶段 4：跨文档与可读性）**：核心 + 文档检查清单（§2.6、§3.1–§3.4）+ 跨文档错误模式 + 阶段 1–3 输出。检查重复定义、矛盾、遗漏引用、文本流畅性与组织。输出 P1 跨文档矛盾 + P2 可读性。

**使用顺序**：H → I → J → K；每个阶段独立 session，上一阶段结论作为下一阶段输入。文档 < 500 行不必拆阶段；> 1500 行或关键模块才拆。

**Profile R（设计优先）**：别名指向本文件 §设计优先模式（Design-First）。只用于 design 存在且错误（design-wrong）的场景；design 不存在不触发，也不补生成。

**Profile O / P**：分别指 `full-review` 的卓越性专项与覆盖率专项；具体命令与技能路由见 `review-cmds.md` §一b 与 §八。

## 二、Review 输出格式

> 输出格式已统一至 [review.md §AI Review 输出模板](review.md#ai-review-输出模板)。
> 所有 Review 任务必须按该模板输出，此处不再重复定义。

**关键约束**：
- 每个问题必须标注优先级（P0/P1/P2）
- 每个问题必须标注位置（`文档:章节` 或 `代码:行号`）
- 每个问题必须提供依据（源码行号或规则引用）
- 每个问题必须给出明确修复建议
- P0 问题必须生成可执行的修改项（按 Step 6 格式）

---

## 三、Review 工具命令

```bash
# ===== 以下 {module} 指 Minix3 模块名（vm/pm/vfs/kernel 等），不是评审状态目录键 {stage} =====

# 1. 验证常量定义
rg "^#define CONSTANT" minix3/minix/servers/{module}/ -n
rg "^#define CONSTANT" minix3/minix/kernel/ -n

# 2. 验证函数定义
rg "^return_type function_name\(" minix3/minix/servers/{module}/ -n
rg "^return_type function_name\(" minix3/minix/kernel/ -n

# 3. 验证结构体定义
rg "^struct struct_name " minix3/minix/servers/{module}/ -n
rg "^typedef struct" minix3/minix/servers/{module}/ -A 5

# 4. 验证宏使用
rg "MACRO_NAME" minix3/minix/servers/{module}/ --type c -n

# 5. 验证枚举值（头文件通常在 include/ 目录）
rg "ENUM_VALUE" minix3/minix/include/ --type h -n

# 6. 全局搜索（不确定模块时使用）
rg "SYMBOL_NAME" minix3/minix/ --type c --type h -n

# 7. 对比文档与代码
# 打开文档引用的代码位置，逐行对比
```

**模块路径速查**：
| 模块 | 源码路径 |
|------|----------|
| VM | `minix3/minix/servers/vm/` |
| PM | `minix3/minix/servers/pm/` |
| VFS | `minix3/minix/servers/vfs/` |
| Kernel | `minix3/minix/kernel/` |
| Drivers | `minix3/minix/drivers/` |
| 公共头文件 | `minix3/minix/include/` |

---

## 四、Review 快速判断口诀

> 口诀已统一至 [review.md §快速判断口诀](review.md#快速判断口诀)。
> 所有快速扫描任务使用 review.md 中的口诀，此处不再重复定义。

**使用时机**：
- 快速 Review（Profile D）时，用口诀快速扫描
- 详细 Review 时，先过一遍口诀找明显问题，再进入 Step 0-7

---

## 五、修复阶段工作流（Fix Phase）

> Review 结束后进入修复阶段时，AI 必须按本流程执行，确保修复不违反 review 规则。

### 1. 修复前准备
1. 重读 STATE.md 中的 Open Issues 列表与对应的 scan.md Issue List。
2. 按问题类型显式加载 Skill：
   - 代码修复（Rust） → `review-code-skill` + `review-patterns-skill`
   - 文档修复（Markdown） → `review-doc-skill` + `review-patterns-skill`
   - 涉及核心语义（IPC/生命周期/错误/权限/地址空间） → `review-core-semantics-skill`
   - 涉及覆盖率/状态追踪 → `review-process-skill` + `review-coverage-skill`
   - 涉及 design → 实施验证（实施 design doc 后） → `review-implementation-skill`（2026-06-22 新增，详见 `../skill/review-implementation-skill.md` §Gate D-Impl）
3. 对每个修复项确认：修改范围、验证方法、是否引入新的 P0/P1。

### 2. 修复执行原则
- **先 P0 后 P1/P2**：P0 全部修复并验证前，不标记收敛。
- **文档与代码同步修**：改代码若影响 Ch4 描述，必须同步改文档；改文档若已要求代码实现，必须同步改代码。
- **禁止引入新的违反**：修复过程中仍需满足 no_std、硬件抽象 trait、SMP/BKL、Claims-Evidence 等约束。
- **保留证据**：每个修复项在 scan.md / STATE.md 中记录：修复日期、修改文件、验证命令输出。

- **文风增量门（2026-09-18 A3.2）**：文档类修复完成后，跑 `tools/doc-style-lint.sh --diff`；有 error 级命中则修复未完成——先把过程信息移到 scan.md / fix-status，再把正文改写为假设性推理或直接正确表述。
### 3. 修复后验证
1. **单元测试**：`cargo test -p <crate>` 必须全部通过。
2. **编译检查**：`cargo check` 无新增 error；新增 warning 需说明理由。
3. **重新跑相关 Gate**：
   - 修了代码语义 → 重新跑 Gate B（Top 5 差异表）抽样验证。
   - 修了代码/测试 → 重新跑 Gate D（P0 必检）和 Gate E（§5 测试存在性）。
   - 修了文档 claim → 重新跑 Gate A/C 相关部分。
4. **更新 STATE.md**：将已修复问题从 Open 列表移入 Closed Issues，注明修复 scan/日期，更新 Convergence Checklist。

### 4. 修复结束标准
- 本次计划修复的所有 P0 已修复并验证。
- 未修复的 P0 必须标记为 `WONTFIX` 并给出不可辩驳的理由（如架构演进明确替代）。
- 最新一次完整 Pass：新增 P0 = 0，新增 P1 = 0。
- 完成 VERIFY-CHECK.md 后才可标记 **CONVERGED**。
