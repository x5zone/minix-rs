# WORKFLOW-OPEN-ITEMS — 工作流开放项与裁决承接台账

> **性质**：本文件是 `agents-workflow-optim.md`、`todo_plan.md`、`EXECUTION-LOG-workflow-optim.md` 三份已归档规划/取证文件的**开放项承接台账**（2026-10-08 建立；三文件完成使命后删除，全文可从 git 历史回捞）。
> 承接原则：只搬**仍未闭环的事项、被活规则引用的格式定义、以及不可重演的用户裁决**；已落地能力的规范源本来就在 `prompt/review-rules/` 与 `tools/`，不重复。

## 一、开放项（需要行动或用户裁决）

### W-1 存量正文锚点降噪回炉批次（原 TODO-1，状态 READY）

- 能力已具备：`tools/doc-style-lint.sh` 的 SL-9（新增行阻断）与 SL-10（告警），判定口径与四类形态在 `prompt/skill/cmds/style-bible/SKILL.md`「硬裁决：读者体验优先于审查便利」；锚点底账格式 `.review/{tool}/{stage}/{doc-stem}.anchors.md`（移出与正文修改同一笔提交；**底账缺失的移出 = P0**，`review-doc-checklist.md` §3.95）。
- 未做：存量 774 篇逐篇回炉。当时基线（2026-10-06 实测，按纪律以现跑为准）：SL-9 命中 441 处、SL-10 命中 3085 处；热区 `02-stage-vm` 之外最密为 `02-rs-process-table.md` 178 处、`01-rs-boot-init.md` 109 处。
- 步骤 0（跨文档调研清单）从未产出，批次启动时先做。历史自包含反例（抄录的原文行）见 git 历史 `prompt/agents-workflow-optim.md` §2。

### W-2 `missing_docs` 配置与逐 crate 升级（原 todo_plan G2）

- 落点：`os/` workspace 各 crate 顶层文档注释完备化。启动前**必须先统计存量警告数量级**（`cargo build` 开 `#![warn(missing_docs)]` 试跑），属代码批次，未排期。
- 关联：原 G1（`tools/code-style-lint.sh`）已作废（2026-10-07 判定，能力并入本项与既有工具）；`gate-evidence-code` 的"代码可读性增量"行现挂钩 `cargo clippy -- -D warnings` + `tools/unsafe-audit.sh --diff`（已写入 review-process.md Step 3.6）。

### W-3 卓越样章双达标（原 todo_plan H4）

- 验收物 = 用户认可一篇"教科书文档 × redox 级代码"样章；候选与判据见 `review-doc-excellence.md` / `review-code-excellence.md`。未排期。

### W-4 Open Questions 台账（原 todo_plan 十二，逐条判定现状）

| OQ | 议题 | 现状 |
|---|---|---|
| OQ1 | 头部字段统一名 `Rust 实现` | **已闭环**（`doc-code-map.sh` 强制执行，缺失新文档判 P1） |
| OQ2 | 关联代码维度用 Step 3.6 而非新增 Gate I | **已闭环**（review-process.md Step 3.6 在位） |
| OQ3 | 文风 lint 存量零容忍？ | **已闭环**（增量阻断 `--diff` + 存量只报告） |
| OQ4 | 演练文档用 `02-stage-vm/15-ipc-dispatch.md` | **作废**（流程已由 vfs/08 干跑 + pm/09 全量实跑两轮替代） |
| OQ5 | pre-commit / CI 钩子 | **裁决点到**（原条件"跑两轮 review 后再评估"已满足）。默认推荐**继续不做**：门都在 review 流程内触发，钩子会把卫生项批量修逼进提交路径。待用户表态 |
| OQ6 | 编辑战役（存量清洗）启动时机 | **已闭环**（用户表态延后；批次计划承接于 W-1） |
| OQ7 | `[ARCH:]`/`TODO`/`DEFERRED` 保留正文 | **已闭环**（保留，项目硬要求/既有流程管理） |
| OQ8 | QEMU 门触发面 `os/kernel|arch|boot-shim/**` | **未落地**（规范源 grep 无挂钩），属代码批次，待实施 |
| OQ9 | unsafe 基线冻结 vs 逐月递减 | **已闭环**（`unsafe-baseline.txt` + `--diff` 只拦新增；递减留待治理批次） |

### W-5 安全提示：`.claude/settings.local.json` 中段通配授权（已复验关闭）

EXECUTION-LOG §2.5 曾发现形如 `Bash(rg "...*" notes/rewrite/...)` 的中段通配授权（≈ 允许在该位置插入任意参数而不再询问）。
2026-10-08 复验：`python3 -c` 枚举 permissions.allow 全部 71 条，含 `*` 的 12 条均为尾部 `Bash(cmd *)` 标准形式，**中段通配已不存在**（随文件后续演进消失）。本项关闭；今后若再现中段通配，按原提示清理即可。

### W-6 读者视角的执行装置缺口：卡点预判与可读性机器判据（状态 OPEN，2026-10-12 登记）

- 缘起：人审 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 第 1 章时连出 8 处修正（过渡句方向反、引言塞细节、残句、压缩简写、恒等与高半映射讲不清、跨篇重复取证、带圈数字渲染不清、引用标签含糊），**其中没有一处是被工具挡下的**，全部靠人读出来；而形式类噪声（工单号、日期、坐标噪声）早就有 SL-1～SL-11 挡着。结论：规则文本不缺，缺的是把"读者视角"变成可执行装置。
- 规则已有、无机器判据（本轮实测）：`prompt/skill/cmds/style-bible/SKILL.md` 第 57 行明令禁"压缩简写"，而"清单又越过它直接供给建表那一步"这类句子 `doc-style-lint.sh` 零次报警（判据：该句改成"页表本身也住在内存里，它要的页只能从清单的可用区间里拿"才读得通）。
- 规则真空（需先写规则再谈门）：① 引号风格与本文件主导风格不一致——本轮 agent 三次引入弯引号，门全绿（`grep -o '“' 该文档 | wc -l` 修前 15、修后 0，`git show HEAD:` 同文件为 0，即全属新引入）；② 含糊尾巴"及邻近字段/相关部分"；③ 跨篇共享取证链的判据归属（本轮 01 篇 §1.4 与 02 篇 §2.4 曾互标"这条依据链与本处同一条"，导致一头把四步证据压成一句嵌套长句）；④ "预判读者会卡在哪"——即 `prompt/doc-commit-review-v2-glm.md` §四 第一轮共识第 1、2 条（复述测试产物形态强制、新手问题预演），至今未落地。
- 候选三条（按成本递增，均需走 `prompt/README.md` 三端同步与四道派生门）：一、`doc-style-lint.sh` 增两条机械判据（引号主导风格一致性、含糊尾巴禁用），self-test 补正反例，当天可上；二、把 structure.md §11 的"整段复述 + 卡点清单"从产物要求升级为 Blocker Gate（缺则 FAIL），治的是"同一个 agent 自认为读得通"这一根本盲区，与 AGENTS.md 已记载的"VERIFY-CHECK 同 agent 局限"同源；三、定向吸收外部成熟条目进模式库（Diátaxis 象限纯度治"引言塞细节"、Google/Microsoft 开发者文档风格指南的句长与示例可复制条目、Strunk & White"一句一义"治压缩长句），必须写明与本项目"教学性必须展开"的适用边界。
- 本轮裁决承接：先只登记本条，不动规则源与工具；存量文档暂不扫，只防新增（见第三节 R5）。本轮已落地的两件与此相关：01 篇新增 §1.3「把第一章收成五句话」（各篇第一章末节固定为范围表，故骨架小结插在其前）；02 篇 §2.4 与 01 篇 §1.4 的互指改为单向（判据只在 owning 篇给全，他篇只引结论）。

### W-7 术语「腿」是仓库自造比喻，需登记为受控比喻（状态 OPEN，2026-10-12 登记）

- 缘起：人审 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` §2.2 时问了一句"腿这个术语在内核开发、尤其中文内核开发里用得很多吗"。查证结论：**不是通用术语**，中文通行的是路径/通路/通道/侧/半（fast path = 快路径；sendrec 的 send 半与 receive 半）；本仓的"腿"是英文 "leg"（the read leg / the write leg）的直译。
- 实测分布（可重跑：`grep -ro 腿 os/ rewrite-notes/ prompt/ --include=*.rs --include=*.md | wc -l` 得 3416 次，分布在 178 个文件；含该字的行合计 2243，其中 1371 行（六成的量）在 `rewrite-notes/coordination/` 的案卷与台账里，主要是 `NK4C-WORKLOG.md`）——真正需要治理的是代码与正式文档里已经固化的用法，集中在两处：`os/kernel/src/ipc.rs` 32 行（写腿、读腿、发送腿、接收腿、drain 腿、陷入腿、完成腿、自返腿）与 `rewrite-notes/08-stage-is/04-is-data-acquisition.md` 4 行（"VFS 两腿"）；文件名层面也有：`os/arch/tests/riscv64_trap_leg_shape.rs`、`os/arch/tests/riscv64_return_leg_pin.rs`。
- 为何不建议一刀切废除：它在这些位置承担了真实语义——"一个复合操作里可独立成败的一段路径"，比"半"精确（sendrec 本来两个半）、比"分支"少歧义（分支已被 fork/branch 占用）。需要治的是**无约束扩散**，不是这个词本身。
- 要立的两条规则（都要走 `prompt/README.md` 三端同步与四道派生门）：① 「腿」列为受控比喻，**不得在概述章/入门章首次引入**，并在首次出现处要给定义；② 同一段落不得把"腿"与"通路/通道/出口"混用——讲的是"输出有两个去处"这类分支选择时用"出口"，不套用"腿"。可执行形态：写进 `prompt/skill/cmds/style-bible/SKILL.md` 的词汇约束，或给 `tools/doc-style-lint.sh` 增一条**报告级**判据（同段混用两类词时出清单，不阻断，沿用 W-1 的"增量阻断 + 存量报告"口径）。
- 本轮已做的单点修正：`01-boot-shim-bootstrap.md` §2.2 的"串口这一腿"（该文件**唯一**一处「腿」，由本会话引入）已改为"串口这个出口"，并把该节两段合并简化（复述 §2.1 那条 assert→panic→刷屏链的部分删掉，按"简述即可"口径）；同句里的箭头速记也已改散文化。

## 二、被活规则引用、必须留存的格式/纪律定义

### F-A `gate-evidence-*` 统一格式（原 todo_plan 0.5；`review-process.md` Step 3.6 指向本定义）

scan.md 用围栏块，名字 `gate-evidence-{type}`：

```gate-evidence-{type}
command: {实际执行的完整命令}
exit: {退出码}
stdout_key: {关键输出 3-5 行，或计数摘要}
artifact: {磁盘产物路径；无产物写 none}
date: {YYYY-MM-DD}
note: {可选：环境限制 / N/A 理由 / 工具缺口}
```

约定：type ∈ {0, A, B, C, D, D-6, E, G, H, code, unsafe, qemu, test, dep, ...}；同一 review 内同一 type 只写一个块；`command` 必须可重放原文。
另注意 Gate 0 的 9 个锚段精确段名硬编码在 `tools/verify-check.py:GATE0_ANCHORS`（`## Skill Invocation Log`、`## Blocker Gates Status`、`## Step 0: 预检结果`、`## Step 1: C Source Ground Truth Lookup`、`## Step 1.5: Coverage Enumeration`、`## Step 2: Diff Extraction`、`## Step 3.5: Precision Check`、`## Issue List`、`## Artifact Inventory`），scan.md 命名须与其一致（见 REVIEW-TRACE-pm-09-fullrun.md F-2）。

### F-B 模式库三副本同步纪律（原 EXECUTION-LOG §7；patterns.md 两处指向本定义）

| 位置 | 定位 |
|---|---|
| `prompt/review-rules/review-patterns.md` | 全量权威源（编号与条数以此为准，副本不双写计数） |
| `prompt/skill/review-patterns-skill.md` | review 时被调用的技能源（须随权威源同步补条） |
| `.claude/skills/review-scan/checks/patterns.md` | Claude 编排器按领域分片的高频子集 |

教训（2026-10-07，模式 85 漏同步事件）：**门绿不等于事对**——`generate-derived-skills.sh --check` 只比"派生 vs 技能源"，看不出"规则源改了、技能源没跟"。改权威源后必须手工同步技能源，再跑同步脚本。

### F-C 会话产物落盘纪律（原 agents-workflow-optim TODO-3，生效中；`rewrite-notes/README.md` 指向本定义）

1. 任何会话/脚本不得向仓库根写任何产物（log/bin/patch/临时 md 一律不行）；仓库根只允许配置与入口文档。
2. 落盘进 `tmp/` 功能子目录：运行日志 → `tmp/log/`；评审/工具取证 → `tmp/evidence/<日期-主题>/`；QEMU/DTB/内存转储 → `tmp/bin/`；NK4C 探针 → `tmp/nk4a/`；一次性脚本用完即删。父目录需先建。
3. `tmp/` 已整域 `.gitignore`（2026-10-07 裁决），写入物无入库风险；代价是不受版本库保护，重要取证须及时转入 `.review/` 或正式文档。

## 三、用户裁决存档（不可重演的历史决定）

| # | 议题 | 裁决（2026-10-07 批次） |
|---|---|---|
| R1 | workflow 优化轮范围 | TODO-2「抽象语义而非描述机制」为主 + 地基审计；TODO-1 只做能力落地，存量正文降噪开独立批次（→ W-1） |
| R2 | CLAUDE.md 去留 | 实测后真删：验证 Claude Code 原生加载 AGENTS.md → 独有内容并回规范源 → 改写生效引用 → 单独 commit 删除（已完成；入口以 `AGENTS.md` 为准） |
| R3 | todo_plan 21 项未闭环验收 | 逐条补跑并留证据（已完成：12 勾选、9 标明未闭环/作废理由；存活项承接于本台账） |
| R4 | 外部参考（get-shit-done、高星 Rust skill） | 借原则，不搬结构 |
| R5 | 读者体验与正确性装置的取舍（2026-10-12 批次，人审 01 篇第 1 章引出） | ① 代码块的保真度分类标签（逐字引用 / 教学简化 / 反面示例 / 签名示意）**不得因"像废话"删除**：它对读者是微小噪音，对文档正确性是可 diff 的钩子；可改进的是措辞（逐字引用块写"未删节"或起止符号，禁"及邻近字段"式含糊尾巴）。② 可读性缺口的处置：本轮**只登记开放项 W-6，不动规则源与工具**。③ 存量扫描：暂不回扫存量文档，只防新增（与 W-1"增量阻断 + 存量报告"的既有口径一致） |

## 四、review 流程实证检验索引（本台账的"过程正常完备且正确"证据链）

- 第一轮：`prompt/REVIEW-TRACE-vfs-08-dryrun.md`（vfs/08 干跑，流程侧修复 F5-F8 的来源）。
- 第二轮：`prompt/REVIEW-TRACE-pm-09-fullrun.md`（pm/09 全量实跑，含 Gate A 工具阻断→修复→重跑、Gate G 机检 FAIL→重组→跨 session 独立验证 7/7 的完整轨迹；流程空档 F-1~F-8 待据此修订规范源）。
- 两轮均收敛；F-2/F-3/F-4/F-5 四条建议改规范源时，按 `prompt/README.md` 三端同步流程执行。
