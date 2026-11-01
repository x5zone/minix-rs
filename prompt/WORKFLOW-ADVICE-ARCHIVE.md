# WORKFLOW-ADVICE-ARCHIVE — 2026-10-08~20 工作流优化批次删档承接（增量收口＋出处存档）

> **性质**：过程存档文档（与 `WORKFLOW-OPEN-ITEMS.md` 同类的台账文档，非规则源；「规则的权威内容在
> `prompt/review-rules/`」不因本文存在而改变）。
> **裁决（2026-10-28，用户拍板）**：`prompt/` 下 25 份历史评审/建议/取证文件整体删档。其中 **12 份曾入库**
> （`git log` 可回捞全文）：`GPT_advice.md`、8 份 `REVIEW-TRACE-*`、`doc-optim-qwen.md`、
> `doc-commit-review-v2-{ds,longcat}.md`；**13 份从未入库**（全文不可恢复）：`GPT_doc_advice.md`、
> `doc-optim-{ds,glm,longcat,mimo,muse}.md`、`doc-commit-review-{ds,glm,longcat,muse}.md`、
> `doc-commit-review-v2-{glm,muse}.md`、`subagent-optim-advice.md`。
> 本文承接：① 未被当前工作流吸收的增量（§2~§4）；② 承接关系图谱（§5，防考古迷路）。
> 本文同时闭环三项悬置拍板：原 `REVIEW-TRACE-docgen-01-05-5rounds-v2` §0.6-①「13 份 bagging 原件要不要
> 归档」与其收尾 §3-③「12 份未跟踪 doc-*.md 是否入库」→ **裁决：不归档、不入库，增量收口于本文**；
> 原 `REVIEW-TRACE-docgen-01-05-v3` §7.7「维持不入库」→ **改判：删档收口**。

## 一、被删文件清单与吸收判定

| # | 文件 | 入库 | 吸收判定（一句话） | 增量去向 |
|---|---|---|---|---|
| 1 | GPT_advice.md | 是 | 教学性诊断（docgen v1/v2 迭代真源）：三架构统一模型等已入 review-rules；「读者认知转移」方法论未入规格 | §2.1 |
| 2 | GPT_doc_advice.md | 否 | 规则级已入 DOC-ITER-WORKFLOW（E7/E8/C1/E5＋反模式 1/2/5）；判据级 6 组未入 | §2.2 |
| 3 | REVIEW-TRACE-vfs-08-dryrun.md | 是 | 第一次实证：14 条流程观察（D1–D13）已修/由后续轮吸收；4 条登记未做 | §4.1 |
| 4 | REVIEW-TRACE-pm-09-fullrun.md | 是 | 第二次实证：流程空档 F-1~F-8 已于 2026-10-28 逐条复核收口（台账 §四 行末注） | 台账 §四 |
| 5 | REVIEW-TRACE-pm-rs-sched-3rounds.md | 是 | 第三次实证：D1–D13 全部落地（含新增 anchor-suspect-scan.py）；2 项用户裁决悬置 | §4.3 |
| 6 | REVIEW-TRACE-rounds4-8.md | 是 | 五轮并行：8 修 8 登记；登记未修 ~13 条为工具轮欠账 | §4.1 |
| 7 | REVIEW-TRACE-rounds9-13.md | 是 | 五轮串行：串行收敛方法论已入检查项注册表；登记未修 ~8 条欠账 | §4.1 |
| 8 | REVIEW-TRACE-docgen-01-05-5rounds.md | 是 | docgen 第一棒（用户否决收场）：SL-11、#11 四层认知序、4.6 学生走查已进 lint 与 doc-gen-prompt；遗留 3 条由 v2 挂账转述 | §5 |
| 9 | REVIEW-TRACE-docgen-01-05-5rounds-v2.md | 是 | docgen 第二棒：M1–M8 标尺原址（git 留存）；T/S 系改动并入 AI-chats 两个 prompt；4 条拍板项由本文头部裁决闭环 | 头部裁决 |
| 10 | REVIEW-TRACE-docgen-01-05-v3.md | 是 | docgen 第三棒（总判「达到了」）：W1–W22 分流落地（doc-gen-prompt / review-doc-excellence / review-cmds / DOC-ITER V2 盲测）；入口 A 装置由 DOC-ITER-WORKFLOW 承接 | §4.4 |
| 11 | doc-optim-ds.md | 否 | P-1~P-10 / T-1~T-7 经 qwen 收口与 v2 复验分流（P-2→doc-ref-check、P-3→注册表 5.6d、T-2→density 读数、T-6→W-6 候选）；「待用户裁决四项」余 2 无裁决记录 | §3.2 |
| 12 | doc-optim-glm.md | 否 | 提案 A–G 分流落地（A/B 中档→通用门 #9、C→5.6c＋W-6、D→style-bible 第 10 条、E→0.7.5、F→5.6c、G.2 辨析三已修）；C.2 新手问题预演仍开放 | §3.2 |
| 13 | doc-optim-longcat.md | 否 | 10 检查项的 P0 判级被「中等档」裁决有意降档；3.3 落成 tools/doc-ref-check.sh | §3.3 |
| 14 | doc-optim-mimo.md | 否 | G-1（coverage 恒真）/G-4（引用对账）/G-7（脚手架，R8-D7）已落地；cut_memmap 悬置已由 01 篇取证闭环；G-2/G-5/G-8/G-10 开放 | §3.2 |
| 15 | doc-optim-muse.md | 否 | devices 9.1–9.12 多数并入 §3.2 三角账；§10 七项内容级跟进随 11 篇冻结（2026-10-18）失效 | §3.2 |
| 16 | doc-optim-qwen.md | 是 | bagging 收口提案：§7/§7.1 落地清单是本批权威对账记录（git 留存）；⏸ deferred 5 项 | §3.2 |
| 17 | doc-commit-review-ds.md | 否 | v1 审计：13 类载重发现在 v2-ds 重放闭合 12/13；O-1~O-11 分流 | §3.1/§3.2 |
| 18 | doc-commit-review-glm.md | 否 | v1 审计：C1–C5 复验 10/13 修复；§四「9 项共识」余 3 项 | §3.2 |
| 19 | doc-commit-review-longcat.md | 否 | v1 审计：2.1–2.5 由二修与 v2 收口；2.4 未采纳表并入三角账 | §3.2 |
| 20 | doc-commit-review-muse.md | 否 | v1 审计：§3 覆盖矩阵是 6 份提案的汇总账（未动 ~12 项→三角账）；F-7 已修、F-9→doc-ref-check | §3.2 |
| 21 | doc-commit-review-v2-ds.md | 是 | v2 审计：重放闭合 12/13；O-1~O-5 / E-1~E-3 现状 | §3.1/§3.2 |
| 22 | doc-commit-review-v2-glm.md | 否 | v2 审计：N1/N2 已修；N4/N5 与 §四 3 项 | §3.2 |
| 23 | doc-commit-review-v2-longcat.md | 是 | v2 审计：A/D/F/G 已修；B/C/E | §3.1/§3.2 |
| 24 | doc-commit-review-v2-muse.md | 否 | v2 审计：V2-2/V2-6 已修，V2-5 部分；§3 Still-open 7 行 | §3.1/§3.2 |
| 25 | subagent-optim-advice.md | 否 | 对抗性冷审协议：整体未吸收（DOC-ITER v0.9 早于该文件两天，之后规格未升版） | §2.3 |

## 二、未吸收增量 · 判据与方法论

### 2.1 GPT_advice 的「读者认知转移」方法论（已入库删档，git 历史可查全文；此处存蒸馏清单）

该文是 GPT 读 01/05 新旧两版后的教学性诊断，曾为 docgen v1/v2 迭代的「真源」。其方法论只有一部分进了
现行装置（三架构统一模型在 `review-rules/review.md`；认知四段 Input/Question/Evidence/Output 在
`AI-chats/doc-gen-prompt.md`）。以下条目**未进入任何规范源**——若做 DOC-ITER-WORKFLOW §十 未决-4
（入口 A 完整重做），应先回看这份清单与 git 原文：

1. **读者认知路径链（六字段版）**：每小节明确 `Reader Before → Question → Evidence → Observation →
   Insight → Next Dependency`，是生成约束不是打印项。现行 S2.1 读者路径表只有 3 字段（本节回答什么/
   依赖前文什么/为后文提供什么），缺「读者此刻的问题→证据→观察→跃迁」一档。
2. **「outline 加结构字段，不加检查维度」**：诊断——outline 本质是 Knowledge Graph + Chapter
   Structure，缺 Reader Transition Graph；修法是给 outline contract 加结构字段（本节之前读者已知道
   什么/本节要让读者发现什么问题/通过什么证据/结束后能用什么新语言解释旧现象/下一节依赖什么认知），
   而不是往 review-rules 塞教学质量检查（原文元规则：「缺的是生成阶段一个非常明确的『源码导读叙事
   模式』」）。
3. **C 函数深读七要件**：真实 file:line、真实节选、关键语句解释、调用关系、依赖什么、**没做什么**、
   后续哪个函数才完成剩余动作；判据「名字看起来像做 X，实际上只做 X 的前半部分」最有教学价值。
4. **贴码四优先**：揭示语义的最小代码 / C→Rust 转换代表 / 可自行追踪入口 / 一条完整短执行路径；
   存在理由判据「看了它以后，读者多理解了一个东西」「不能回答『读者为什么现在必须看它』就不要放」。
5. **「读者理解时间」位置纪律**：代码/图表/跨章引用须有理解时间的位置；禁「完整实现见 §4.3」式
   跳过——重要就先讲最小必要部分。
6. **范围控制四要素**：该对象在机制中的角色 / 为何停在这里 / 继续学去哪 / 停下时已获得什么模型。
7. **反压缩删除判据**：删前问「删掉以后，读者是不是失去了一次自己发现某件事情的机会」；六类不可删
   （小段 C 源码 / 函数间连接 /「看起来应该这样但实际不是」/ 寄存器字段为何存在 / 须回头才能理解的
   关系 / 从观察归纳机制的过程）与六类可删（重复结论 / 无关字段罗列 / 纯实现目录 / 无教学作用整码 /
   开发审查历史叙事 / 为显得完整的架构细节）。
8. **写后五测试的后两项**：D 删除测试（随机删一段码，问读者是否少一次理解）、E 压缩反向测试（是否把
   「看源码→观察→推理」压成一句结论）。现行 V2 盲测/V5 冷读覆盖 A/B/C 的近似物，D/E 无对应装置。
9. **「删内容 ≠ 压过程」两层论**：anti-bloat 防「删教学内容」，防不了「压教学过程」——多 AI review
   会共同把「8 次理解跃迁压成 3 句结论」且在 reviewer 视角被奖励为「改进」，这是共同失败机制。

M1–M8 学生标尺只定义于原 v2 trace §0.4（git 历史），约束「8 条封顶、不得反向变成新的机器门」。

### 2.2 GPT_doc_advice 的判据级增量（未入库——全文不可恢复，以下为尽量完整的摘录）

规则级内容已吸收进 `DOC-ITER-WORKFLOW.md`（E7 结构改动三级 / E8 可引用≠不必解释 / C1 承载物保护 /
E5 篇幅由内容决定＋反模式 1/2/5）。以下**判据级**内容未入：

1. **三条件删除判据**（与 C1 保护清单互补的「允许删」侧）：只有当内容真正 ① 错误；② 与本篇主题无关；
   ③ 在其他文档已有完整且更合适的教学位置——才允许删除。
2. **8 条禁删理由**（agent 常用但站不住的删减借口，逐条禁）：「旧版重复」「可以交叉引用」「已经在
   前文解释过」「属于实现细节」「放到下一篇」「读者不需要知道这么多」「更紧凑」「提高信息密度」。
3. **单节复现判据**：「一个第一次读的人，仅凭这一节，能否自己回到源码中复现这个结论」——比 V2 盲测
   便宜的单节级检查。
4. **非直觉行为六步结构**：源码 → 表面行为 → 具体数字例子 → 为什么直觉写法会错 → 实际写法 →
   一般性规则；配套判据「『为什么』必须由源码逼出来，而不是由结论倒推源码」（先提出真实问题再回
   源码找证据；反过来写，源码只是答案的插图）。
5. **教学对象十项清单**（比 C1 清单多出的部分加粗）：真实 C 片段、**源码位置**、**调用关系**、
   **状态变化**、具体例子、反例、**设计动机**、**实现落点**、**测试**、**失败模式**。
6. **教学完整性五问串**：这里发生了什么 / 为什么这样做 / 源码在哪里 / 如果改错会怎样 / Rust 为什么
   不一样；配套判据「只有最终结论而没有足够证据或推导 → 不是『信息已存在』，而是教学内容仍然缺失」。

### 2.3 subagent-optim-advice 的对抗性冷审协议（未入库——整体未吸收，全文不可恢复）

该文（2026-10-20）解释「为什么网页版 GPT 审得比 IDE Agent 好」并给出 API 子 agent 复现方案。核心
论断：优势主因是**独立第三方效应**（审查上下文干净），不是模型代差；「任务框架，而非能力，是主要
驱动因素」。两条实证知识：**Context Rot**（约 11K 干净上下文判读 8/10 vs 约 299K 拥塞上下文 3/10）
与**「skill（能力层）与上下文污染（信息层）正交」**——要禁的是信息泄漏，不是能力赋予（应给专门
对抗性审查 skill，但零附带历史）。五条约束（与 DOC-ITER-WORKFLOW V5 冷读 / §八 用户裁决的差距就
在这五条）：

1. **零附加输入**：子 agent 输入＝产物本身＋固定 prompt；父 agent 不得附加任何介绍/自评/diff——
   「手滑」一句介绍即污染。
2. **异构模型**：审查用不同家族或至少不同代际的模型（同模型子 agent 盲点重合）。
3. **对抗性任务框架**：要求「找出矛盾、遗漏和未经证明的跳跃，而非确认它写得对」；输出按阻塞/
   非阻塞分级。
4. **裁决权在外部**：审查结果不回到父 agent 上下文里自证（父 agent 会为自己过去的决策辩护）。
5. **修改后重新冷启动**：每轮修改后全新会话再审，不复用旧审查上下文。

配套流程五步：文档定稿 → 父 agent 只输出文档本身 → 异构冷启动子 agent 按固定对抗框架审 →
分级输出 → 外部裁决。

## 三、未吸收增量 · 审计遗留（doc-optim / doc-commit-review 批次）

### 3.1 已核实修复（2026-10-28 核验，不再挂账）

| 项（出处） | 修复证据 |
|---|---|
| 测试/概念章误报（v2-glm N1 / v2-longcat A） | `tools/doc-code-density-check.sh` 按标题排除非承载章型（测试/参见/附录/概述/概念/目录/术语/导航/小结/导览/速查/总览/全景），self-test 含「测试章型→应放行」夹具 |
| 分母守卫三句（v2-ds E-3） | 同工具：输出「分母：承载/诊断章(可判) N / 全部章 M」；未识别到承载章→「需人工确认」不判 ✅；豁免过半→WARN 泛滥 |
| report 横幅中性化（v2-ds O-2 / v2-longcat D / v2-muse V2-9） | 同工具：「⚠ 发现 N 个阻断候选（report 模式，仅报告；--check 才阻断）」 |
| review-cmds §三 step4 旧尺度残留（v2-muse V2-2） | `prompt/review-rules/review-cmds.md` §三 step4 现为「阻断项（仅客观可判）＝无 C/Rust 节选码或无命令；读数项非自动阻断」 |
| doc-ref-check「第 N.M 名词形」假阴性（v2-longcat C 之半） | `tools/doc-ref-check.sh` 现匹配「第 N.M（节/章字可省）」 |
| 死代码 C_RUST_RE / check()（v2-glm N2 / v2-longcat F / v2-muse V2-9） | `grep -c C_RUST_RE tools/doc-code-density-check.sh` = 0 |
| Step 4.5b「截至」措辞（pm-09 F-3） | `grep -c 截至 prompt/review-rules/review-process.md` = 0 |
| mimo cut_memmap 实参语义悬置 | 已由 01 篇设计轮次闭环：`rewrite-notes/01-stage-kernel/todo.md` D-67 记「T4 cut_memmap 自动页化核实完毕」（取证 `01-forensics-round5.md`） |
| G-7 脚手架文档级布局（mimo） | rounds4-8 已修清单 R8-D7（git 历史） |
| 注册表行 0.11/0.12/0.13/1.5c/5.6c/5.6d/0.7.5 | 均在 `prompt/review-rules/review-process.md` 检查项注册表在册（0.13/5.6d 如实标注「人工步骤，工具化待后续」） |

### 3.2 仍开放（此前未在任何台账登记，在此登记；原文出处已删档）

1. **density 存量口径**（v2-ds O-1 / v2-longcat B / v2-muse §3 行6）：`--baseline/--diff` 增量模式
   未实现（qwen ⏸ deferred-2），~259/369 篇存量无政策出口（「成批返工 or 绕开 --check」）；配套的
   ~44 篇存量 backlog 清单（拟挂 `rewrite-notes/coordination/TODO-LEDGER-OPEN.md`）未建。
2. **~15 项 teaching devices 三角账**（v2-muse §3 行4 + v1-glm §四 + v1-longcat 2.4）：orientation
   block、why-not-B 列、角色+后果句、历史包袱 vs 知识框、reader/appendix 契约、单一叙事轴、跨文档
   ownership 表、tension 一行式、新手问题预演、复述产物形态强制、Gate B 双 8 字段、证据分级换算表、
   脚手架纪律、读者任务覆盖、临时标记保留、doc/outline/design 编号映射——从未逐项
   accept/defer/reject（W-6 已吸收其中两项作候选）。
3. **mimo G-2/G-5/G-8/G-10**：rerank 蓝图 `§选题对账` 门（规范源 0 命中）；Gate C 因果断言回读
   （生成端已有近似物：doc-gen-prompt 的「被引注释行为断言对现码验」，评审层未落）；Gate B 两套
   8 字段并存与 [INFERRED]→L2 换算表；读者任务覆盖清单（DOC-ITER S2.1 读者路径表为部分对应物）。
4. **v1-glm §四 9 项共识经 v2 复验后仍余 3 项**：复述测试产物形态强制（DOC-ITER V2 盲测学生为部分
   对应物）、新手问题预演（同 W-6 候选）、excellence B 级收紧（「缺 DIRECT 项不得评 B」——未核验
   是否已有等价收紧）。
5. **doc-ref-check 接入与剩余局限**：未接入任何命令薄壳强制门清单（`grep -rn doc-ref-check
   prompt/skill/cmds/ AGENTS.md` = 0，仅在注册表 0.12 行在册）；已知局限待核——跨文档 `§N.M` 撞
   同号小节假通过、「第 N 章」误伤自然语言（docgen v3 §三）、`.md` 抑制的行作用域语义（v2-muse
   V2-5：探针「见foo.md 好，第 9 章坏。」应报而漏报；现实现写「同一行左侧邻近出现 .md」）。
6. **注册表行与 skill 对应表的对账机械化**（v2-glm N4 / v2-longcat E）：建议 `check-review-rules.sh`
   加「注册表行 id 集合 ⊆ skill 对应表」检查；同型缺口曾两次在不同文件复发。5.6c 反证探针的执行
   载体（style-fix 薄壳内 0 命中，规则在册但触发点不知道它存在——v2-glm N5）同属此类。
7. **qwen ⏸ deferred 5 项**：review-process.md Step 0.3 正文级全重写（现由 Gate H precedence 注 +
   §一b 桥接）、density `--baseline`（见 1）、负抽样工具化（5.6d 人工）、sha256 edit-binding 工具化
   （0.13 人工）、doc-teach-check 重型读数器（tools/doc-teach-check.sh，未实现）。
8. **v2-muse §3 行7 提交时证据习惯**：提交信息或 `.review` 附件约定四门退出码＋工具自测（DS 4.8.3）。
9. **ds「待用户裁决四项」余 2 项无裁决记录**：负抽样节数（≥2 节或仅对改动章节施加）、T-4 八类收割
   清单增删。

### 3.3 有意不做（用户已裁或被后序裁决降档，勿再提案）

1. 「每个核心函数必须贴码否则 P0」严格档（qwen §8：容错太低、易逼出为凑码而贴码）——采「中等档」：
   无 C/Rust 码或无命令才 P1，三架构/调用图/辨析为读数项。
2. 把文档写回 draft 体量（2019 行）；削弱任何真值校验；为漂亮提前改抽象契约（qwen §8 三条非目标）。
3. longcat 的 P0 判级口径（「为什么解释/目标读者/设计动机」为 P0）——被「中等档」决策整体降档。
4. v1-ds 四项裁决中两项已有结果：裁决 1（doc-ref-check 深度，先做两条确定性检查）已按建议落地；
   裁决 3（Ch1 机制图/对照表升 P1）已按「中等档」改判。

## 四、未吸收增量 · 流程取证（REVIEW-TRACE 批次；8 份均 git 历史可查，此处只登记欠账指针）

### 4.1 review 工具轮欠账（rounds4-8 / rounds9-13「登记未修」共约 21 条，两轮收尾均判「需要一次专门的工具轮次」）

分类摘要：**行号门 AMBIG 存量**（R6-D4 删门或降卫生项需裁决、R8-D2 解析文档声明源码目录）；
**字段级坐标进底账**（R8-D8 格式待扩）；**锚点形态覆盖**（R6-D1 `path:NNN` 与 `name:fn` 形态、
R6-D2 外部 OS 锚点误判仓内、R6-D3 中文 `产物:` 标签、R7-D5 底账两套格式规格统一）；**口径统一**
（R7-D2 无连字符过程编号形态集合、R7-D3 收敛条件「P1 新增≤1」口径、R9-D5 verify-check 与
state-validate 计数口径、R12-D3 余项 SYMBOLS 标签冲突、R13-D2 Per-Doc Status 按运行时布局重写）；
**文案/低危**（R5-D1 并发会话只读约定、R5-D3 失败文案、R5-D5 baseline 语义进注册表、R4-D7 计数
不自洽、R4-D8 补 impl 限定对 trait impl 不可执行、R10-D6 关联清单两路并集、R13-D3 `--diff`
退出码传导根因）。逐条原文见 git 历史 `prompt/REVIEW-TRACE-rounds4-8.md` / `-rounds9-13.md` §登记未修。

### 4.2 未执行动作

- Gaps(semantic) 旧口径 13 轮重跑（rounds4-8 / rounds9-13 附录建议：历史 scan.md 中以恒真指标支撑
  Gate A PASS 的段落用「有文档无Rust」新口径复跑一次）。恒真缺陷本体已修；scan.md 不入库，实际
  影响有限，登记即可。

### 4.3 待用户裁决（悬置至今）

- `draft/`、`doc_rerank_*`、`plan/todo` 是否算「文档」进覆盖分母（pm-rs-sched-3rounds §D4 余项）。
- 小文档（<300 行）每轮 v2 快照全量重推导的收益/成本复议（同文件，登记时明言「改规则需用户裁决」）。

### 4.4 docgen 线剩余（v3 收尾）

- **跨节一致性自查**（「两节互相打架」类残留）与**生成阶段自我门禁**：v3 判为「下一轮该动的地方」，
  后由 DOC-ITER-WORKFLOW V 系与五门部分承接，专项未做。
- 直引号/含糊尾巴升级 doc-style-lint 机械判据（W-6 候选一，仍待裁决）。
- Rust receive 向死锁预检未接线（建议进代码迭代待办）；sched.rs 模块注释引用旧版文档节号（随下次
  代码提交校正）。
- 源码侧三项待统一已由 v3 §7.7 登记 `rewrite-notes/01-stage-kernel/todo.md` §26，不在此重复。

## 五、承接关系图谱（谁取代了谁——防考古迷路）

1. **外层驱动协议**：`AI-chats/doc-gen-iterate-prompt.md`（v1）→ `AI-chats/doc-gen-iterate-prompt-v2.md`
   → 第三棒协议（原 `REVIEW-TRACE-docgen-01-05-v3.md` §协议）→ **`prompt/DOC-ITER-WORKFLOW.md`**
   （统一规格：入口 A 新生成沿用 v3 装置，入口 B 存量优化 S0~S5）。两个旧驱动文件仍在库，头部已加
   废弃注（2026-10-28）。
2. **bagging 链**：6 份 doc-optim 提案 → 落地提交（6ff56f440 / ff3524c07 / 6d91b7b16 / 9dbc3cca6）→
   4 份 v1 审计 → 二修 → 4 份 v2 审计 → 多数遗留由 2026-10-08 晚与 2026-10-15 规范/工具更新吸收
   （§3.1），余项登记于 §3.2。
3. **review 实证链**：vfs-08 → pm-09 → pm-rs-sched → rounds4-8（并行）→ rounds9-13（串行）；
   台账 `WORKFLOW-OPEN-ITEMS.md` §四 只登记前两轮为「实证检验索引」，其余轮次的欠账指针在本文 §4。
4. **GPT 出处辨析**：DOC-ITER-WORKFLOW 规则表 E7/E8/C1 的来源标注「GPT_doc_advice」指已删档的
   `prompt/GPT_doc_advice.md`（判据摘录 §2.2）；E1–E6 的「GPT 判据一~五」真源是
   `AI-chats/GPT-workflow-optim-begin.md`（**仍在库**，勿混淆）。
5. **跨轮状态**：doc-iter 轮次的活状态由 `rewrite-notes/coordination/DOC-ITER-LEDGER.md`（入库）与
   `tmp/doc-iter/`（不入库）承接，不依赖本文；本台账 W-15 指向本文。
