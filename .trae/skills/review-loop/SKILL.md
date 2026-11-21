---
name: review-loop
description: 文档+关联代码的固定 review 循环与工作流评测：L0 基线、L1 独立面板、L2 归因、L3 修复、L4 复验、L5 轨迹、L6 meta-review、L7 升版。用于逐篇试点、判断 review 流程是否进步与收敛。
---

# Review Loop —— 文档 + 代码的统一 review 循环

## 何时使用

- 用户要求“按固定循环 review 一篇文档和它相关的代码，并记录轨迹”；
- 用户要求“判断当前 review 工作流是否进步 / 是否收敛”；
- 用户要求逐篇处理 `rewrite-notes/01-stage-kernel/` 的 01–10 篇，或后续 `02-stage-vm/`、`03-stage-rs/` 的 01–10 篇。

不用于：

- 普通的一次性 review（走 `full-review` 等六个命令）；
- 只改文风（走 `style-fix`）；
- 只做代码卓越性（走 `code-excellence`）；
- 只修一个 TODO（走 `todo-fix`）。

本技能是外层编排，不新增第七个任务命令。它调用现有命令与检查清单，只新增“独立面板、归因、评测、升版”四层。

## 开工前必读

1. `prompt/review-rules/review-panel.md`：角色、材料隔离、7 字段、只读纪律。
2. `prompt/review-rules/review-evals.md`：语料、轨迹格式、指标、meta 问卷、收敛标准。
3. `rewrite-notes/coordination/review-eval/WORKFLOW-VERSIONS.md`：当前版本。
4. `rewrite-notes/coordination/review-eval/CORPUS.md`：本篇条目与期望。
5. `rewrite-notes/coordination/review-eval/DECISIONS.md`：与本篇相关的冻结决策。
6. `prompt/review-rules/review-process.md`、`review-doc-checklist.md` 或 `review-code-checklist.md`：按本轮范围加载。

主 agent 必须通过 Skill 工具显式调用 `review-process-skill`，并按范围调用 `review-doc-skill` 或 `review-code-skill`；面板子 agent 使用 `review-panel.md` 第四节的 brief，不加载完整技能，以保持材料隔离。这个例外必须写进 `loop.md §1`。

## L0 基线、范围、语料装载

1. 确认工作树状态；记录 `git rev-parse HEAD`、`git status --porcelain`。
2. 记录文档 sha256 与行数：`sha256sum <doc>`、`wc -l <doc>`。
3. 跑 Step 0 硬预检：
   ```bash
   ls rewrite-notes/<stage>/.design/<NN>-outline.v*.md
   ls rewrite-notes/<stage>/.design/<NN>-outline-review.v*.md
   ls rewrite-notes/<stage>/.design/<NN>-design.v*.md
   ls rewrite-notes/<stage>/.design/<NN>-design-final.v*.md
   tools/design-coverage-check.sh <stage>
   ```
   缺失时按 `review-process.md` Step 0.3 嵌入生成，不中断循环。
4. 关联代码清单：`tools/doc-code-map.sh <doc> --check`；有缺失先登记，不进入 L1。
5. 初始化 review 状态：`tools/review-init.sh zcode <doc>`。
6. 写 `runs/<stage>/<doc-stem>/loop.md §0`：commit、sha256、行数、范围、关联代码、语料条目 ID。
7. 02/03 阶段先读 `review-eval/doc-set/` 的对账简报；没有简报就先做对账，不进入 L1。
8. **读 follow-ups（V3 只核对已批准项）**：V3 的上一轮必须已关闭，理论上没有“未修”项。读 `runs/<stage>/<doc-stem>/follow-ups.md` 中用户批准的 ②（带日期）与上一轮 `suggestions.md`，确认目的地是否已落地；不得把未批准 ② 或 ⑤ 带入新轮。V2.5 过渡轮仍按旧口径分“顺手修/核销清单”，但同样不得把未批准项带进下一轮。首轮没有前轮 follow-ups 时跳过此步。**上一 V1/V2 轮未关闭、遗留大量未批准旧 ② 时**（05 篇的 72 条旧 ② 在 R1 台账与 follow-ups 两处）：在 §0 列出存量清单并请用户裁决去留（补批准日期 / 转 ⑤ 取证 / 明确丢弃）；不得静默带入新轮，也不得静默丢弃。补日期仅限确有当时批准记录的项，不得事后追认。
9. **定轮型与 edition**：`loop.md §0` 必须写 `轮型: full|structural|confirmation` 与
   `edition: V3.x`。`full` = 完整 survey 交易；`structural` = survey 输入取上一轮
   已落盘报告的结构收口轮（不重开面板）；`confirmation` = 确认轮（§6A 四态模型，
   全量 survey、修复权限仅限本轮新准入项、不实施 ⑥、每个 CANDIDATE 最多 2 次、不占 §7.5
   修改预算）。修改轮（full/structural）没有收敛声明权——最多 `ROUND-CLOSED` /
   `CONVERGENCE-CANDIDATE`；R25 是 V2.5 过渡轮，仍写 `轮型: discovery`，其收口按
   certify 阶段执行。
10. **写 schema 标记**：`loop.md §0` 必须写当前生效版本的 schema。**新开篇目
   （05 起）写 `schema: V3.4 from R1`**（V3.4 最小内核的门只对 `schema: V3.4` 起的
   文档生效；写 V3.3 = 新门静默失效）；02–04 已 FREEZE 的轮次保持各自历史标记
   （01 现行 `V3.3 from R31`，02/03/04 `V3.3 from R1`），不追溯、不改写。已在旧
   schema 下立基线又需切新版的（如 05 的 R1 §0 已写 V3.3）：面板启动前更正 §0 的
   schema 行并在 loop 记一行 VARIANCE——这是基线修正，不是轮中途改规则（L6.9.8
   管辖的是轮内改 edition，不覆盖此动作）。
   `eval-trace-check.py --round` 缺这个标记直接 FAIL——这是防止新文档落回旧 schema、
   重新出现无限全量轮的硬门。

范围声明模板：

```text
本篇范围 = <文档主题> + 以下关联代码：
<doc-code-map 清单>
不在范围 = 相邻模块、未列入清单的文件、下一批次的结构性重构。
```

## L0.9 出版门前置（V3.7，开轮前强制）

开轮前必须对**当前篇**与**其关联代码**跑出版门，零命中才允许进入 L1：

```bash
python3 tools/doc-publication-check.py <当前篇文档>
python3 tools/code-publication-check.py $(cat runs/<stage>/<doc>/related-code.txt | tr '\n' ' ')
```

- 有命中 → **本轮的 L0 阶段先清出版门**，清到零再派面板；不得「带着历史叙事开轮」，
  否则面板会收敛在脏文档上（D01/D02 的历史教训）。
- 出版门不清零，本轮最多 `ROUND-CLOSED`，不得 `CONVERGED`。

## L1 独立审查面板

按 `review-panel.md` 第三节选择角色，按第七节决定是否缩编。**档位唯一源是
`review-panel.md` §七（V3.4-r7：此前 L1 与 §七各写一套阈值——<500 行合并形状相反、
D1 触发三口径——执行者会用到先读到的那份）**。不可让与的原则：读者 A/B/C 的独立性
本身是检出能力（≥500 行不得以成本合并）；高风险篇目（启动链/IPC/内存管理/时钟中断）
加 P-REFUTE 且读者强制独立；P-REFUTE 触发与档位判据一律按 §七表执行。派 D1 前查
`DECISIONS.md` D-024（D1 停跑）——档位表判应派而 D-024 命中时，在 loop §1 记
VARIANCE 并引用 D-024，未裁决前默认不派。

V3 没有独立的验证轮。每轮的 survey 阶段按上面的默认配置派全量角色；fix 之后的 certify 阶段只派定点回读：P-FACT 只查本轮修复 hunk 涉及的符号/常量/引文，READER 只读改动段及邻句（不读全文），RB 审最终 HEAD（含本轮全部修复提交）。certify 不重开全量 READER A/B/C，不派 DESIGN 全量；发现的新问题进**本轮**台账，在本轮内修完，不得用下一轮承接。

派发方式：

1. 用 Agent 工具启动子 agent，类型 `general-purpose`；每个角色一个独立子 agent。**默认全量并发**；仅在收到派发失败时降档（6 → 3 → 1），成功后探测回升（连续 3 个成功升并发 2 探测，逐级回升，失败回档位 1 并重新计数）。每次降档/升档事件记入 `loop.md §1`。
2. 每个角色先写报告骨架，每完成 2–3 条发现就追加写盘，不要等全部查完才写。
3. 角色在 10 分钟无输出时重试 1 次；仍失败标 `MISSING`，在 `loop.md §1` 写明缺哪个角色与可能漏什么。失败的 agent 先检查产出文件完整性（有 `## FINDING` 或显式"无发现"声明），不完整即重派一次。
4. 合并角色（如读者 A+B）必须在 `loop.md §1` 标 `VARIANCE`，说明合并理由与独立性损失。
5. 禁止用 `grep '^#'` 读章节标题来定位；需要标题时用 `rg '^## '` 只取二级标题，且不得把设计章标题交给需要盲读的角色。
6. 把 `review-panel.md` 第四节对应 brief 逐字作为任务，替换尖括号占位符；不要补充作者的解释、结论、历史评审。
7. 子 agent 只写 `tmp/review-eval/<stage>/<doc-stem>/panel/<role>.md`；禁止修改仓库文件。
8. 面板运行前后各跑一次 `git status --porcelain`；出现新改动即判违规，丢弃该角色输出并重跑。
9. 面板原始输出逐字保留，不合并、不润色。**V3.5 制品链**：面板输出入库
   `runs/<stage>/<doc>/panel/round<N>/` 并用 `python3 tools/panel-index.py` 生成
   `INDEX.md`（哈希索引）；tmp 只作工作副本。certify 独立回执入库
   `runs/<stage>/<doc>/certify/round<N>-*.md`。

`loop.md §1` 记录：角色、拿到什么、没拿到什么、模型、起止时间、原始输出路径、缩编理由与风险、派发降档/升档事件。

## L2 归因与决策

1. 收齐面板输出，按 `review-panel.md` 第五节的编号规则统一编号。
2. **去重与准入（V3）**：按 `review-evals.md` §4.0c 为每条发现分配 `根因 ID`（`RD-NNN`）与 `origin`；同根发现合并到同一行，在 `发现 ID` 列列出。四条准入判据（位置/判据/证据/反证）不全的，写 `suggestions.md`；以 SUGG 行入台账登记并计入 metrics 的 `SUGG` 键（V3.3 起为合法处置类；不进阻断集、不参与 G1–G9）。
3. 每条准入发现按 `review-evals.md` §4.1 处置：① 修（P0/P1/P2 一律）、② 用户批准的转出（带批准日期）、④ 读者验收清单内项（本轮修）、⑤ 证据不足（本轮不得关闭）、⑥ 设计重构（本轮实施）、FP 有反证的误报。**③ 已删除。** 范围内确认为真缺陷的一律本轮修，不允许无档期转出。**读者/设计的 P3 不按 ①/④ 准入**——标 `SUGG` 行登记 suggestions.md（§4.0c 严重度门槛）。
3b. **设计类决策（DESIGN 角色的 ⑥）必须标分类**（§4.1b，D-047）：`[MODERN]`（C 有此语义，换现代表达）与 `[DEL]`（C 没有此物，删翻译残留）= 代裁，当轮实施；`[ARCH]`（改变 boot 是什么）**不得实施**——P0/P1/P2 入台账 ⑤ + `轮结果: BLOCKED`（解除只认判决账行），P3 进 suggestions。未分类 = 按 ARCH 阻断。
3c. **减法倡导者触发（V3.6，C-37；r2 补触发点——原规则只在 panel §4.3b 定义，主流程从未启动它）**：归因完成后，若本轮存在 `[ARCH]` 提案、拟入判决账的 D-0xx 行、或任何「新增类型 / 新装置」级提案，**在交用户裁决之前**派一次 P-SUBTRACT（brief 见 `review-panel.md` §4.3b），产出 `panel/subtract.md` 并在 `loop.md §1` 记「P-SUBTRACT 已启动：<提案 ID 列表>」。无提案则记「未启动：无提案」。**主 agent 收到后按 `review-code-checklist.md` §3.5 与 §2.5 的机械判据裁决；判不了的进判决账；禁止按品味判。**
4. 查 `DECISIONS.md`；命中且无新证据的，记 `re-litigation`，不计文档缺陷。
5. 标范围：`in-scope` 或 `adjacent`；`adjacent` 标 ② 带日期（哪一轮修）。
6. 写 `defect-ledger.md`，V3 表头必须含 `根因 ID`、`发现 ID`、`origin`、`来源角色`、`严重度`、`类别`、`处置类`、`处置`、`证据`、`复验`。V2.5 过渡轮保留旧表头。
7. 写 `loop.md §2` 与 `§3`。
8. 跑轨迹检查（轮次关账只检查本轮）：
   ```bash
   python3 tools/eval-trace-check.py rewrite-notes/coordination/review-eval/runs/<stage>/<doc-stem> --round R<N>
   ```
   退出码非 0 时，回到第 1 步补处置；不允许进入 L3。**L2 时点的豁免（V3.4-r7，
   R6-e 判例）**：`--round` 在 L2 只裁决「面板发现 ↔ 台账处置」的覆盖与处置类合法
   性——round-closure（L6/L6.5 收口小节）、**loop-sections（§4–§8 空节）**与 metrics
   数字类（发现总数/轮型/轮结果等）是 L3–L6 的产物，L2 时点必然 FAIL，**不计入本步
   的回炉条件**；同一条命令的
   exit-0 全量通过是 L6.9 关账条件。**不带 `--round` 是全历史归档审计**，历史轮次的旧格式/stale 面板/缺行不阻断当前轮；归档审计的 FAIL 单独记账，不得删除历史证据来“变绿”。

## L3 修复

1. **按根因分批（V3.1）**：每批 ≤20 个根因；同一根因的多条发现同批处理。批内不混无关改动，批与批之间 commit。**B 类/复发根因按段成批**：同一段/小节被报 ≥2 个根因，或同一根因第二次出现，该批必须重写整段/整小节（`rewrite`），不是逐句打补丁；A 类事实项可线级修。
2. 按 `fix-guard`：读目标 ±5 行、grep 确认现状、一次只修一条、修后 grep 验证、写 fix-status。
3. **每条代码修复必须**：带回归测试（覆盖修复前的失败场景）→ 反向验证（还原→红，恢复→绿）→ fix-scope（发现点名的位置逐处改到，缺一处降 ② 带日期）。
4. 文档修复与代码修复同批；代码改动必须同步 Ch3、Ch4、§5 与测试。
5. **代码改动触及 pub 签名/结构体字段/常量时**，必须跑 `tools/doc-symbol-drift.sh <doc> --symbol <旧符号>...`（或 `--from-diff <base> --strict`），旧串命中必须为 0；文档引文块改动必须跑 `tools/doc-citation-check.py <doc>`。两者是 commit 门，不留给下一轮面板。
6. **”顺手修”清单**与 ① 同批处理：L0 步骤 8 产出的”顺手修”项（前轮 follow-ups 中与本篇关联代码重叠的项），在本轮修复中一并处理。
7. 代码门：`cargo fmt --check`、`cargo clippy`、`cargo test`；触碰 arch 或 boot 时按 `AGENTS.md` 跑 QEMU。
8. **受影响 crate 全量绿**：提交前 `cargo test -p <crate>` 必须通过；含 minix-arch。
9. **100% 复验**：本批每个根因都要有可重放证据（旧串 grep 归零 / 测试名 + 输出 / 逐字对照 / 冷读回执），不抽样；R24 的 114 抽 29、R25 的 98 抽 46 是 V3 禁止的做法。修复引入新缺陷 → 该批回退，按上限重做。
10. 可选：本地跑 `tools/install-review-hooks.sh` 安装 pre-commit；暂存文档的引文对账与门自测会在提交时自动跑（共享门仍在 CI）。

## L3.5 DESIGN 实施

1. P-DESIGN 发现标 ⑥ 的项在本步实施（不是”下一轮”或”单独立项”——那就是永远不做）。
2. 实施纪律：
   - 先写设计取舍（改哪些文件、为什么这样改、替代方案是什么、拒绝理由）；
   - 再写代码，每个改动跟 `fix-guard`（读 ±5 行、grep 确认、一次一条、修后验证）；
   - 受影响 crate 全量绿（`cargo test -p <crate>` + clippy）；
   - 代码改完后**同步更新文档 §3/§4/§5 中对应的描述**——这是”文档与代码同步”门，不是可选步骤。
3. 确需跨 crate 协调（比如 `BootReport<'a>` 触 6 个 crate）的项，在 `OPEN-DEBT.md` 立独立项目（负责人 + 截止轮次），并在本篇的 `STUCK-REPORT.md` 或 `NOT CONVERGED` 标注中写明”本篇收敛受此项目阻塞”。
4. 本步完成后，L4 的文档五门与 trace 门用**更新后的文档**跑。
5. 若本步实施了跨 crate 的设计变更（`wire` 版本、所有权限定、ABI 布局等），旧轮次的收敛结论只对旧基线成立：实施后必须重新固定基线（提交号 + 文档行数/sha256 + 测试计数），重跑受影响门与 QEMU，再判断 G8/G9；不得用旧轮次的“零新增”宣告当前基线收敛。

## L4 收口认证（certify）

V3 的 certify 是**同一轮的关闭阶段**，不是新轮。认证输入是闭集：本轮全部修复 hunk、
本轮未闭环项、受影响的章节与代码。不得用“随机 20% 未改章节”重新发现全文。

1. **100% 修复回读（硬门）**：本批每个根因都由**独立冷启动实例**只读修改段及邻句，
   标签写“修复后回读”；不抽样，不以“整体一致率”代替逐条回执。
   **`grep` 旧串归零不构成回读**：它只证明字面消失，不证明语义修对（D03 判例：
   `RD3-R2-25` 台账写“已修”、旧串也确已删，但同段仍残留 7 处过程词，回归 review 才抓到）。
   回读回执必须来自本次新派发的子 agent，不是主 agent 自己重跑命令。
   缺独立回执 = `certify 新准入` 不计为 0，轮次不得关闭。
2. **台账重放**：每条“已修”行给可重放 `grep` 命令与命中输出，或测试名与输出，或
   commit hunk 回指；模板句不算证据。
3. **文档五门 + 机械门**：
   ```bash
   tools/anchor-resolve.sh --check <doc>
   tools/doc-code-map.sh <doc> --check
   git add <doc>
   tools/doc-style-lint.sh --diff
   tools/doc-ref-check.sh --check <doc>
   tools/doc-code-density-check.sh --check <doc>
   python3 tools/doc-citation-check.py <doc>
   ```
4. **符号漂移与行为级陈旧行（V3.2 硬步骤）**：本轮改过 pub 签名/结构体字段/常量时，跑
   `tools/doc-symbol-drift.sh <doc> --symbol <旧符号>...`（或
   `--from-diff <base> --strict`）；**本轮代码批改了行为/签名/常量（无论是否删符号）时，
   必须再跑 `tools/doc-symbol-drift.sh <doc> --stale-lines <批前 rev> --strict`**——
   `let _ =`→`match` 这类行为改动不删任何符号，标识符粒度扫不到（R30 FACT-B-01/02
   判例）。两项旧串/陈旧行命中必须为 0；命中的是本轮同步债，修完才许关轮。
5. **代码块分类**：`tools/doc-snippet-extract.sh <doc>`；引文块逐块对账。
6. **回归扫描**：邻句、自引入过程词、端口义、引号、模糊尾巴、跨节复述。
7. **commit 级回归 review**：每个逻辑 commit 提交后，派一个小型 review（只看该 commit
   的改动范围，不是全量面板），按 `review-evals.md` §4e 的定义：
   - 文档 commit → 锚点 + doc-code-map + 改动段 fact 抽查 + 改动节 reader 抽查；
   - 代码 commit → 双轴（规范轴 + 设计意图轴）+ 契约/并发/失败路径；
   - 混合 commit → 两个子集都跑。
   发现的问题**当场修复**（2026-11-09 用户裁决 §4e-1）；修复后复验通过才闭合。
8. **写 certify 结果**：`loop.md §5` 必须写 `- certify 新准入: <整数>`；只有 `0` 且
   `未闭环=0` 才允许关轮。新准入项进本轮台账并在本轮修完，不得写成下一轮。
9. **所有权/接口顺序复核（V3.1；V3.2 升为硬步骤）**：本轮改动触及 `&mut`/`&'static`/
   生命周期/跨 crate 签名/`unsafe` 时，certify 必须重放“写后读”顺序（或 Miri/Stacked
   Borrows 等价检查），不能只读 diff；R27 D2-13 的 `BootStorage` 别名 UB 是判例。
   **复核必须在本轮 `loop.md §5` 留下逐条推演记录**（改了什么顺序假设、为什么新顺序
   成立）；缺记录 = G7 FAIL，轮次不关闭。
10. **改写段的源码再推演（V3.3 硬步骤）**：本轮**改写**过的机制段（整段/整小节
   rewrite，或触碰机制断言的段落），certify 必须对该段的每条机制断言做一次
   "vs C 源逐条再推演"并在 `loop.md §5` 留记录——引文/符号门只抓逐字漂移，抓不住
   机制语义错误（R31 §2.6.4 判例：两份页目录的机制错误在 R29 重写、R30 触碰后
   仍存活到 R31 survey 的 P0）。缺记录 = G7 FAIL。
11. **最终 HEAD 机械再对账**：`doc-citation-check.py`、`anchor-resolve.sh`、
   `doc-symbol-drift.sh` 全跑；上一批修复引入的引文/符号漂移必须在 certify 前清零。
12. 写 `loop.md §4` 与 `§5`。

## L5 轨迹与指标

1. `loop.md` 补齐 §0–§8 九节。
2. 写 `metrics.md`，固定键：
   ```markdown
   - 发现总数: <整数>
   - 唯一根因数: <整数>
   - 本轮新准入: <整数>
   - certify 新准入: <整数>
   - 重复合并: <整数>
   - 自引入: <整数>
   - 处置: ① <整数> / ② <整数> / ④ <整数> / ⑤ <整数> / ⑥ <整数> / FP <整数> / SUGG <整数>
   - 未闭环: <整数>
   - 轮型: full | structural | confirmation
   - 轮结果: ROUND-CLOSED | CONVERGED | NOT CONVERGED | NOT CONVERGING | FREEZE-WITH-BACKLOG | BLOCKED
   - 代裁决: <整数> / ARCH 提案: <整数> / 用户否决: <整数>（V3.3 治理仪表）
   - edition: V3.x
   - re-litigation: <整数>
   - adjacent 阻塞: <整数>
   - 重构: 提出 <整数> / 接受 <整数> / 实施 <整数> / 登记 <整数>
   - 减法: 提案 <整数> / 采纳 <整数>（V3.6，C-34；提案=D2 SUBTRACTION-TABLE 行数）
   - 成本: agent <整数> / token <整数> / 耗时 <文本>
   ```
   R25 过渡轮仍用 V2.5 的旧键（`轮型: discovery|verification`、`新 in-scope 真缺陷`），
   但必须补 `- certify 新准入: 0`。
3. `发现总数` = 本轮台账原始发现 ID 数；`唯一根因数` = 本轮出现的不同 `根因 ID` 数；
   `本轮新准入` = 本轮 `origin` 首次出现的根因数；`certify 新准入` = certify 阶段首次
   出现的准入根因数。三个数分开报，禁止用一个数冒充另一个。
4. 再用 `--round R<N>` 跑一次 `eval-trace-check.py`，确认退出码 0；`未闭环` 必须等于
   台账逐行实际未闭环数（不是手填）。
5. 把本轮新增的 follow-up 写进 `follow-ups.md`，每条给目的地；清单外偏好写
   `suggestions.md`，不进台账。

## L6 独立 meta-review

1. **meta 修约（V3.4 最小内核，§5.0）**：轮次级独立 meta 不再逐轮要求——**收敛时
   （FREEZE 前）必须一次独立冷启动全量收口 meta**；其余轮次由主 agent 在 `### L6`
   写同 agent 收口并如实标注；每 2 篇做一次跨篇 meta。
2. 按 `review-panel.md` 4.6 的 brief 启动冷启动子 agent；输入按 `review-evals.md`
   第五节。**（V3.4-r7 修约限定：本步只适用于两个时点——收敛前全量收口 meta 与
   每 2 篇跨篇 meta；其余轮次主 agent 自收口并如实标注，不派子 agent。）**
3. **审计对象是台账与证据，不是重审文档**：P0/P1/未闭环项 100% 复判；P2/P3 随机 20% 校准；逐条检查 `根因 ID`/`origin`/复验证据能否重放。读者项按 §4.0e 的三问复核（不变量/前提/证据），三问答不出才算缺陷；“更喜欢另一种写法”不算。
4. 输出到 `meta/<doc-stem>-review.md`（仅上一步限定的两个时点），必须给 `唯一根因数`、`自引入数`、`重复合并数`、`edition 是否稳定` 四个数。
5. 结论为 `REGRESSED` 时，记录为**流程缺陷**（进 `BLIND-SPOTS.md` 与 edition 边界），不阻止文档关闭；文档是否收敛只看 §6A 的 G1–G9。
6. 新增盲区写入 `BLIND-SPOTS.md`，下一轮回归必须复现。
7. **抽查 `suggestions.md`**：若发现满足四条准入判据却被降级为 suggestion 的，记流程缺陷；建议桶不得成为新的 `③` 延期桶。

## L6.5 Resource re-consultation

1. 根据本轮 `BLIND-SPOT-DELTA` 的主导类别，从 `prompt/resources/` 选读一份最相关的文件：
   - 代码设计类盲区 → `adversarial-rust` 的对应规则文件
   - 并发/资源类盲区 → `graham-code-review` 或 `asterinas`
   - 教学类盲区 → `explain-in-simple-language` 或 `systems-programming-course-builder`
   - 流程类盲区 → `eai-agent-toolkit` 或 `fresh-eyes-review`
2. 读完后回答一个问题：本轮的盲区，这份资源有没有我们没用到的检测模式？
3. 有 → 记入 `BLIND-SPOTS.md` 的期望探测器列，下一轮实现。
4. 没有 → 记录"已查，无新模式"，防止同一资源被反复空读。

## L6.9 轮关闭闸（V3：本轮不闭合，不准开下一轮）

进入下一轮之前，逐项核对（缺一即停在“本轮未关闭”状态，不得派下一轮面板）：

1. 本轮全部可修项（①④⑥，含 RBn 与 commit 级回归的再生项）**已修，且每条附可重放证据**；FP 必须有反证；② 必须有用户批准日期；⑤ 与未批准 ② 视为未闭环。
2. **100% 复验**：每个根因都有复验，不是抽样；`复验` 列不能是模板句。
3. RBn 结论已入账（写进 `defect-ledger.md` 并计入未闭环，不能只写在 loop/RB 报告里）。
4. ② 转出/拖欠已获**用户明确批准**并落账（未经批准的拖欠按未修计，阻断收敛）。
5. `eval-trace-check.py --round R<N>` 退出码 0（含 ①/④/⑥ 证据、FP 反证、② 批准、未闭环=0、本轮 L6/L6.5 收口小节；`未闭环` 与台账实际数一致）。**文档级 todo 台账另跑** `--todo-file rewrite-notes/<stage>/<NN>-todo.md`（该文档没有 todo 文件时跳过并在 §7 注明"无 todo 台账"）；`允许开放` 不是合法处置类，必须映射为 ②（用户批准 + 日期）或 ⑤（取证路径 / 阻塞），未映射的按未闭环计。
6. **轮关闭条件**：`未闭环=0` 且 `certify 新准入=0`。修改轮（full/structural）关闭
   写 `ROUND-CLOSED` / 进入 `CONVERGENCE-CANDIDATE`——**不得宣告 CONVERGED**
   （收敛声明权只在确认轮，§6A 四态模型；R26/R30 两次假阳性判例）。
   `OPEN-DEBT.md` 是未闭环的一部分（§4.0f）：有未销账行 = 本轮不关闭，只有两个
   出口（实施附证据 / 判决账行），逾期行逐条点名。
7. **下一轮**：只能在上一轮关闭后开；修改轮之后是确认轮或下一个修改事务
   （§7.5 预算内），不得用缩减轮承接。上一轮未闭环 >0（含 OPEN-DEBT 未销账）时
   禁止出现下一轮小节。确认轮的权限与退出条件见 §6A：只修本轮新准入项、不实施 ⑥、
   每个 CANDIDATE 最多 2 次（edition 更替或批准重开后重新计数）；`certify 新准入=0` 才可宣告 `CONVERGED`。
8. **edition 不变**：本轮内不得改规则；确需变更按 `review-evals.md` §7.6 把本轮标 `INVALIDATED` 后在新 edition 重开。
9. **3 轮试验窗口**：同一 edition 在同一篇上第 3 轮仍 `open>0`，判 `NOT CONVERGING`，写 `EDITION-RETRO.md`，不得开第 4 轮。
10. **轮内 certify 上限**：同一轮第 3 次 certify 仍有新准入根因 → 本轮标 `BLOCKED`，写 `STUCK-REPORT.md`；解除按 `review-evals.md` §4.0b 的两级分流（第一级独立冷读仲裁，第二级用户病因讨论且结论落判决账）；不得第 4 次 certify，也不得开下一轮。

## L7 收敛检查与工作流升版

### 7a. 文档收敛检查

按 `review-evals.md` §6A 的四态模型执行（V3.3）：修改轮（full/structural）只能产出
`ROUND-CLOSED` / `CONVERGENCE-CANDIDATE`；**`CONVERGED` 只能由确认轮宣告**——
确认轮全量 survey、修复权限仅限本轮新准入的 P0/P1/P2 事实与同步项、不实施 ⑥，
`certify 新准入=0` 且 OPEN-DEBT 无未销账行 → 写 `FREEZE.md`（HEAD、文档 sha、
`未闭环=0`、`certify 新准入=0`、100% 复验证据、确认轮轮次号），进下一篇。
确认轮发现结构性新根因、或第 2 次确认仍 `新准入>0` → 该 edition `NOT CONVERGING`
回边界裁决；不得开第 3 次确认，也不得重开全量修改轮。

**同一根因连续两次修复仍复发** → 生成 `STUCK-REPORT.md` → 标记该根因 `STUCK` → 交用户裁决；不得用第 3 次措辞修复打补丁，也不得把复发计成“新发现”。

`meta PASS` 不是文档收敛；`CONVERGED` 只看 §6A。meta 的 `NOT PASS` 记流程缺陷，不阻止文档关闭。

**FREEZE 后回归审计（V3.4 最小内核，§4h；载体 `REGRESSION.md` + FREEZE 追加回归节）**：进下一篇 L0 前必须完成一次独立回归
审查（D-055 两墙口径）；回归轮的全部发现必须入 `defect-ledger.md` 行级账并在
`loop.md` 留索引节；FREEZE append-only（只允许追加回归节与更新 sha 行）。

### 7b. 陷阱套件

每 2 轮复跑一次 `traps/T01–T08`。有陷阱翻转 → 过拟合警报（`review-evals.md` §十）。

### 7c. 边界批次

本篇收敛后、下一篇 L0 前，检查上篇与下篇的 follow-ups 中是否有 P0/P1 设计债，且其宿主文件在下篇的关联代码清单里。有 → 在两篇之间插一个"边界批次"（只修设计债，不开面板），修完才进下篇 L0。

### 7d. 工作流升版

版本边界**不依赖文档收敛**，在每个轮次关闭后、每篇 FREEZE 后、或同一 edition 连续 3 轮未收敛时执行。**同一篇文档最多 3 个 edition（K=3，V3.3 软上限）**：超出后继续迭代必须在 `DECISIONS.md` 判决账落一行（理由 + 范围 + 日期），把"无限换规则重试"变成显式决定：

1. 跑版本边界回归：`CORPUS.md` 的 R01–R12 与 T01–T08。
2. 汇总本轮与前几轮的指标，写 `meta/boundary-V<n>.md`；必须给 `唯一根因数`、`自引入数`、`重复合并数`、机械门命中、规则命中率。
2b. 历史模板审计：`python3 tools/eval-trace-check.py <run-dir> --audit-templates`；报告每轮 `处置`/`复验` 的模板串重复，不改历史、不阻断当前轮。
3. 晋升规则：每次最多 2 条；每条带案例 ID、检测器、目标文件、期望效果、回归风险、替代/合并的旧装置。优先晋升降低 `r`/`M` 的项（机械门、去重、身份键、读者清单收窄）。
4. 跑同步门：
   ```bash
   tools/generate-derived-skills.sh
   tools/check-review-rules.sh
   tools/lint-review-rules.sh
   tools/diff-trae-skills.sh --only-diff
   python3 tools/eval-trace-check.py --self-test
   python3 tools/trait-audit.py --self-test
   python3 tools/doc-citation-check.py --self-test
   bash tools/doc-symbol-drift.sh --self-test
   ```
5. 更新 `WORKFLOW-VERSIONS.md` 的版本表、减法账本、同步门读数。减法账本行数必须 ≥ 版本表行数。
6. 全绿后提交；未全绿不升版。
7. 连续两篇证明某条新规无效果或制造误报，回退并记录。
8. **3 轮试验**：同一 edition 在同一篇上第 3 轮仍 `open>0`，判 `NOT CONVERGING`，写 `EDITION-RETRO.md`；下一个边界处理最大阻塞项或回退，不得开第 4 轮。
9. **第 3 轮必须是结构轮**：按 `EDITION-RETRO.md` 重写被反复报的段/小节，并在最终 HEAD 跑 citation/anchor/symbol-drift 机械再对账；若第 3 轮仍是线级补丁或仍 `open>0`，同样判 `NOT CONVERGING`。
10. **设计快照是历史输入**：审查合同 = 文档 + 源码 + `DECISIONS.md`；`.design/*.md` 与文档/源码冲突时以文档/源码为准，不得把旧快照条款当作“文档偏离合同”。

## 每篇的提交

每篇两个逻辑单元：

1. 修复：文档、代码、测试、配置；
2. 轨迹：`runs/` 下的 `loop.md`、`metrics.md`、`defect-ledger.md`、`follow-ups.md`，以及 `meta/` 报告。

只用具名路径 `git add`；禁止 `git add -A`、`git commit -a`。

## 禁止事项

- 禁止把面板当投票；冲突按证据强度裁决。
- 禁止让读者角色读源码；禁止让设计角色先看现有实现再自称独立设计。
- 禁止没有处置行的发现进入修复。
- 禁止把相邻问题升级为当前阻塞。
- 禁止在轮次中途改规则；确需变更按 `review-evals.md` §7.6 把本轮标 `INVALIDATED` 后在新 edition 重开，旧结论不得静默继承。
- 禁止用“没有新问题”当收敛；收敛判据见 `review-evals.md` 第六节。
- 禁止用 reduced verification 轮承接本轮未闭环项；每轮都是完整 review。
- 禁止对 ①/④/⑥ 抽样复验；每个根因都要有可重放证据。
- 禁止用 `发现总数` 冒充 `唯一根因数` 或 `本轮新准入`。
- 禁止把 `prompt/resources/` 的外部文本直接抄进规则源。
