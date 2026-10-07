# 工作流优化执行日志（TODO-2 抽象语义规则强化 + 地基审计）

> **创建**: 本日志记录 workflow 优化批次的每一个动作，供**更换工具、更换模型的独立审计方**复核。
> 每条都写「做了什么 / 命令 / 实测结果 / 为什么」。审计方不需要相信叙述，按命令复跑即可。
> 上游任务文档：`prompt/agents-workflow-optim.md`（TODO-1～TODO-4）、`prompt/todo_plan.md`（40 张任务卡）。

## 用户裁决（本轮问答确定）

| # | 议题 | 裁决 |
|---|---|---|
| R1 | 本轮范围 | **TODO-2「抽象语义而非描述机制」为主 + 地基审计**（路径核对、缺工具补齐、日志）；TODO-1 只做步骤 0 调研，正文降噪开独立批次 |
| R2 | CLAUDE.md | **实测后真删**：先验证 Claude Code 是否原生加载 AGENTS.md，可加载则把独有内容并回规范源、改写生效引用、单独 commit 删除 |
| R3 | todo_plan.md 的 21 项未闭环验收 | **补跑并留证据**（命令 + 输出片段贴进验收小节）；跑不动或已过时的标注为「已过时/已合并」 |
| R4 | 外部参考（get-shit-done、高星 Rust skill） | **借原则，不搬结构**：先调研，写对照表，只取能补本仓缺口的机制 |

## 一、开工前盘点（证据，不是印象）

### 1.1 todo_plan.md 落地状态

按 0.2 第 3 条「只写已完成不算完成」复核：产物存在性 + 同义词复测 + 真跑验收命令。

| 任务卡 | 产物 | 实测 |
|---|---|---|
| A2/A3 文风门 | `tools/doc-style-lint.sh` + 4 处接入 | 存在，`--self-test` PASS |
| B2 关联代码清单 | `tools/doc-code-map.sh` | 存在，但**不支持 `--help`**（rc=2） |
| D2 unsafe 审计 | `tools/unsafe-audit.sh` + `unsafe-baseline.txt` | 存在，`--self-test` PASS |
| D5 符号锚点 | `tools/anchor-migrate.sh`、`anchor-resolve.sh` + 两份基线 | 存在，两脚本 `--self-test` PASS |
| G1 代码可读性 | 规则在（6 个文件命中「可读性」）| **工具 `tools/code-style-lint.sh` 未落地** |
| G2 `missing_docs` | 仅 1 处命中 | **Cargo 层配置未落地** |
| G3 依赖方向 | `check-rs-unwired.sh`、`check-command-boundary.sh` | 存在 |
| H1/H3 | 读者复述（5 处）、`doc-snippet-extract.sh` | 存在 |
| D1/E1/E3/F1/F2 | 同义词复测均命中（1/2/1/11/3 文件） | 规则已落地 |
| A 批后续 | `tools/todo-staleness-check.sh` | 存在，但**不支持 `--help`**（rc=2） |

**未闭环项**：A5/B6/G5/H5 四段的验收汇总共 21 个 `- [ ]` 从未贴证据 → 按 R3 补跑。

### 1.2 CLAUDE.md 退役的实测依据（R2 要求先测）

交互式真跑失败（本机 claude CLI 的模型端点订阅过期，报 400），改用**发行物字符串取证**（可复跑）：

```bash
P=~/.local/share/claude/versions/2.1.292
grep -aoE ".{90}AGENTS\.md.{90}" "$P"
```

命中两条决定性证据：
1. 项目指令加载清单 `vbt=[["CLAUDE.md"],["CLAUDE.local.md"],["AGENTS.md"],[".claude","CLAUDE.md"],[".claude","AGENTS.md"],[".claude","rules"]]`
   —— `AGENTS.md` 与 `.claude/rules` 都在自动加载之列。
2. 默认模式文档 `claude-md-or-agents-md (default): a project with no CLAUDE.md of its own gets its
   AGENTS.md files instead, loaded exactly where and how CLAUDE.md would be.`
   —— **项目没有 CLAUDE.md 时，用 AGENTS.md，加载位置与方式完全等同**。

结论：删除 CLAUDE.md 后 Claude Code 会加载 AGENTS.md + `.claude/rules/`，符合 R2 的解锁条件。

### 1.3 CLAUDE.md 323 行的内容归属复核（防止删掉真东西）

逐段 grep 规范源（`.claude/rules`、`prompt/review-rules`、`prompt/skill`）：Explicit Skill Invocation(3)、
Gate 证据强度(6)、Step 7.1 收敛(9)、Step 5.7 Rule Discovery(5)、Gate D 清单(1)、structure.md 12 节(6)、
Review 累积改进表(2，源在 `prompt/review-rules/review-doc-checklist.md` + `.claude/rules/review-core.md`)、
VERIFY 同 agent 局限(4)、任务命令表(12) —— **唯一在规范源里零命中的是「Doc Code Sync 七项表」**，
已于本批次并入 `prompt/review-rules/review-doc-checklist.md`（见 2.1）。

### 1.4 生效引用清单（删前必须改写）

`grep -rlE "CLAUDE\.md"` 排除 tmp/minix3/.review 后共 13 个入口/规则/工具文件 + 约 30 处历史记录
（`.review/*` 的 scan、coordination 案卷、todo 等按「历史记录保留」不动，与迁移期同一纪律）。

---

## 二、CLAUDE.md 退役（R2 的落地，按「实测→并源→改引用→单独删」四步走）

### 2.1 先补规范源的空洞（删之前确认不会丢东西）

| 动作 | 证据 |
|---|---|
| 「Doc Code Sync 七项」从 CLAUDE.md 迁入 `prompt/review-rules/review-doc-checklist.md` §4 | 迁入前 `grep -c 'Doc-Sync-1' prompt/review-rules/review-doc-checklist.md` = **0**，说明规范源确实没有；迁入后该行号定位 1155 |
| 同文件 §2.4g 有一处**外指 CLAUDE.md 的链接**（`(../../CLAUDE.md)`） | 改为指向 `review-core.md` |
| `.claude/rules/fix-guard.md` 无规范源对应文件 | 新建 `prompt/review-rules/fix-guard.md`，正文与运行时副本逐行一致，只多一段归属说明；顺带把「运行时端比源端多内容」这条漂移写进文件头 |
| 其余各段（显式调用技能 / Gate 证据强度 / Step 7.1 / Step 5.7 / structure 12 节 / 累积改进表 / VERIFY 同 agent / 任务命令） | grep 规范源命中数分别为 3/6/9/5/6/2/4/12，均已存在 |
| AGENTS.md 精简时被砍的两个段落复查 | Fix Phase 五步：fix-guard.md 已含前 5 条，`review-process.md:2433/2439` 含 `cargo test -p` 与 STATE 同步；技能表与 `name`=目录名约定：`prompt/README.md:328` 已载 |

### 2.2 AGENTS.md 从「Codex 入口」升为「三端唯一入口」

99 → 90 行（净瘦身，不是搬家）：新增开工前 6 行路由表（指明权威在 `prompt/review-rules/`）、
Key Constraints 补入 CLAUDE.md 的不可约条目（显式调用技能、Gate 证据 L1/L2/L3、Doc Code Sync 指向、
VERIFY 同 agent 局限）、目录布局补 `tools/` 真源与 `tmp/` 归属；同时**修正一处自相矛盾**——
原文写「内容以 `.claude/rules/` 为准」，与同段「规则权威在 prompt/」冲突，现统一为源端权威。

### 2.3 生效引用改写（13 个活引用文件，历史记录按纪律保留）

| 文件 | 改法 |
|---|---|
| `AGENTS.md` | 前言与必读清单重指；`CLAUDE.md` 退役原因与证据写进顶部说明 |
| `prompt/README.md` | 三端架构叙述、项目根配置节、加载方式对照表、Claude/Codex 设计意图共 7 处改为 `AGENTS.md`；历史 changelog 里那 1 处（2026-07-16 那轮的「变更覆盖范围」）**保留** |
| `prompt/review-rules/review-patterns.md`、`prompt/skill/review-patterns-skill.md` | 4 处与 3 处「与 CLAUDE.md 目录约定一致」类引用改指 `AGENTS.md` / 规范源，改后各为 0 |
| `prompt/todo_plan.md` | 6 处操作指引类（启动清单第 1 行、规则改动口径、复述内容清单等）改指 AGENTS.md；其余 13 处属当时点记录，保留 |
| `tools/lint-review-rules.sh` | 白名单注释里的 Hidden Folder Convention 出处改指 `AGENTS.md` |
| `.claude/rules/review-core.md:192` | 「Doc 10 review 验证 CLAUDE.md…」属历史事实，保留 |
| `.claude/settings.local.json` | 历史授权命令，按迁移期同一裁决冻结不动 |

### 2.4 删除与门禁

`git rm CLAUDE.md` 单独成笔。删除后四道门：`check-review-rules.sh` consistent（该脚本本无 CLAUDE.md 断言，
grep 确认）、`generate-derived-skills.sh --check` 无漂移、`lint-review-rules.sh` 0 失败、
`diff-trae-skills.sh --only-diff` 9/9 同步。

### 2.5 顺带发现的一处安全提示（未处置，需你决定）

Claude Code 启动时对本仓 `.claude/settings.local.json` 打印了告警：形如
`Bash(rg "...*" notes/rewrite/...)` 的授权规则里通配符出现在命令中段，
**等于允许在该位置插入任意参数而不再询问**。这类条目在该文件里有若干条。
本次按「settings.local.json 冻结」的既有裁决未动它 —— 是否要单独清理一轮授权规则？

---

## 三、TODO-2「抽象语义而非描述机制」固化为可判定规则（本轮重点）

### 3.0 先证实前提（不采信 optim 文档的自述）

`grep -rc 公理 prompt/review-rules/*.md prompt/skill/*.md` → **0 命中**。四条公理只活在案卷
`PENDING-DECISIONS-3ARCH-PARITY.md` PD-33 段与评审会话里。optim 文档所称「强调过但没写成可判定规则」成立。

### 3.1 四处规范源改动（源在 prompt/，派生重生成）

| 位置 | 加了什么 | 可判定性从哪来 |
|---|---|---|
| `review-core-semantics.md` §1.6（新） | 四条公理 + 配套三问 + 三条边界（arch 层可谈硬件名词、启动契约不在射程、禁止为漂亮提前改契约）+ **依赖射程唯一定义** + 待裁决豁免面实测登记 | 三问是必答项；射程定义给出「允许依赖 `*Arch` trait，禁止依赖具体实现模块/类型/CSR/汇编/三架构符号」 |
| `review-patterns.md` 模式 85（新，§九 总括） | 「机制伪装成语义抽象 / 为消除差异制造伪统一」：4 条判据、grep 起手命令、与 79/80/82 的根因关系、**反向误伤边界**、一正一负回归样本 | 正例=双根被抽成单根 trait；负例=只有单一使用者且注明等第二个实现再抽 → **不得**判违规 |
| `review-code-excellence.md` §16.7（新） | 跨架构接口比选强制栏：三问逐答 + 对比表增设「统一的是语义还是形状」列 + 射程自检 + 反方向检查；给出可贴的输出格式 | 填「形状」→ P1-design-deviation；无法回答 → 视为比选未完成 |
| `review-doc-checklist.md` §3.9（新） | 文档面：先语义后承载形态、禁「为兼容某架构的特殊处理」、差异表不得隐含 x86 为默认；**明文禁止把本原则念成「硬件名词 grep 清零」** | 附正例句式（PD-33 定稿措辞） |
| `review-code-checklist.md` 第 59 项 | 措辞收窄：原「是否使用 `#[cfg(target_arch)]` 选择硬件行为」易被读成架构层也不许分支 → 补边界澄清与射程指引 | 防误杀四层模型的合法依赖 |

模式编号：新增 85 后，`lint-review-rules.sh` 的 L5 门立刻抓到「宣称总数与实际编号数不一致」，
按 todo_plan「计数宣称同步（易漏）」把各处 85 → 86 同步（`review-patterns.md` 头部、
`review-patterns-skill.md` 描述、`review-agent-ide.md`、`prompt/README.md` 三处），并重生成派生。

### 3.2 顺带修掉一个「会静默死掉的门」（真缺陷）

`tools/lint-review-rules.sh` 的 L5c 原本把 **82 写死在脚本里**：
`expect=$(grep -oE "82 个编号模式" ...)`。后果有两层——

1. 任何人新增一条模式都会让该门失败，且失败信息里的宣称值是空的；
2. 更糟：grep 无命中返回非零，在 `set -e` 下**整个脚本当场退出**，既不打印 FAIL 也不打印 OK，
   调用方只看到一个退出码。这与迁移期发现的 `doc-style-lint.sh` 增量门属同一类失效（门不报错却什么都不查）。

修法：宣称数字改为从索引行读出来（`grep -oE "[0-9]+ 个编号模式"`），并给两处 grep 加 `|| true`
与显式空值分支，让「宣称缺失」成为一种被报告的状态而非一次崩溃。双向验证：

| 场景 | 期望 | 实测 |
|---|---|---|
| 宣称与实测一致 | OK 且退出码 0 | `OK: L5c 编号模式总数 83 与索引表宣称（83）一致`，退出码 0 |
| 故意把宣称改成 71 | 响亮 FAIL、退出码 1、后续门仍跑完 | `FAIL: L5c 编号模式总数(83) 与索引表宣称(71)不一致…`，退出码 1 |
| 复原 | 全绿 | `完成：0 个失败` |

### 3.3 四道门

`check-review-rules.sh` consistent、`generate-derived-skills.sh --check` 无漂移、
`lint-review-rules.sh` 0 失败、`diff-trae-skills.sh --only-diff` 9/9 同步；
Trae agent 字符数 9289（硬上限 10000，未越线）。

### 3.4 明确没做的事（按范围裁决）

- 不改任何代码：豁免面（用户态 4 处、内核通用层约 65 处）只登记进 §1.6 文末，等 PD-33 三选一裁决。
- 不改任何正式文档正文：模式 85 与 §3.9 的「存量回炉」留给后续批次（本轮只让工作流具备判定能力）。

---

## 四、TODO-1 的「能力」落地（读者面向，不动存量正文）

按你的范围裁决：本轮只让工作流**具备**这个能力。存量正文降噪（774 篇回炉）是另一批次。

### 4.1 判定口径（你选「采用该口径」）与四类形态

写进 `prompt/skill/cmds/style-bible/SKILL.md` 新增节「硬裁决：读者体验优先于审查便利」，
核心是一条可执行的分辨规则：**同一个括号里，有讲解就留，只有坐标就走**。四类形态：
纯坐标（噪声，阻断）／坐标与讲解同框（半噪声，拆分）／工具派生行号提示（噪声，暂告警）／
符号锚点与块级区间题注（合法，保留，删它属误伤）。

**为什么不是"锚点越少越好"**：实测符号锚点 1734 处、`path:line` 型坐标 43015 处——后者大量是
§2 讲 C 源码时的正当路标。一刀切会把读者的路标也拔掉，且摧毁唯一不漂移的定位手段。

### 4.2 承接位置（你选甲：只放 `.review`）

规则定为 `.review/{tool}/{stage}/{doc-stem}.anchors.md`，格式与两条硬约束写进 style-bible：
- 移出与正文修改**必须同一笔提交** → git 历史永久留有原始坐标，底账丢失可从历史重建；
- **底账缺失的移出 = P0**（`review-doc-checklist.md` §3.95 第 3 项）。
optim 文档里悬着的决策点 D-anchor-persistence 就此关闭（原文已改写为裁决记录）。

### 4.3 机器门：SL-9 与 SL-10（`tools/doc-style-lint.sh`）

| 规则 | 级别 | 射程 | 为什么这么定 |
|---|---|---|---|
| SL-9 | error（`--diff` 阻断新增行） | 括号内只有数字与分隔符的坐标注释，含 `/* 455 — utility.c:44 */` | 纯坐标对读者零价值；且必须在**代码围栏内**也生效——噪声全长在复刻代码块里 |
| SL-10 | warning（`--strict`） | 坐标与讲解同框 `/* 16: 进程标志 */`、正文里的 `（Lnnn，工具生成）` | 后者是 `anchor-migrate.sh` 的**规范输出形态**，若设 error，工具自己产出违规；故告警级 + 明文写出升级为阻断的条件 |

实现踩坑一次（值得记录，因为它就是本仓反复出现的「门写了却不查」类型）：我最初把三条坐标检查
插在 `clean = strip_inline_code(raw)` 之后，而围栏跳过逻辑在它**之前**执行——结果 SL-9 在真实语料上
命中 0，自测却因为夹具那几行不在围栏内而通过。自测通过 ≠ 门有效。修法是把坐标检查提到围栏判断
之前，并把围栏跳过改为 `if (diffmode != 1)` 的独立判断。

自测同步扩充（双向断言，缺一不许合入）：新增 `anchors.md` 噪声夹具与 `anchor-ok.md` 合法形态夹具，
断言 SL-9/SL-10 必须命中前者、**绝不得**命中后者；`for id in SL-1..SL-7` 加入 SL-9。

### 4.4 交叉验证：门测出的热区与当初人工统计吻合

774 篇正式文档全量摸底（`--strict`）：SL-9 = **441** 处、SL-10 = **3085** 处。
密度前六名：`03-stage-rs/02-rs-process-table.md` 178、`03-stage-rs/01-rs-boot-init.md` 109、
`01-stage-kernel/17-syscall-process.md` 88、`19/20-syscall-*` 70/76、`05-rs-ipc-sendmask.md` 75。
optim 文档 §2 当初人工统计的前两名是 158 与 107 —— 两条独立方法得出同一批热区，
说明这条门的判定确实指向当初批评的那个问题。合法形态反向验证：区间题注 + 符号锚点 + 纯讲解括号
→ 0 命中。

### 4.5 演进契约（防两个文件各自演化）

`tools/anchor-migrate.sh` 头部新增说明：该工具产出的 `（Lnnn，工具生成）` 后缀目前受 SL-10 告警管辖，
一旦「底账抽取」落地（工具改为写进 `.review/.../anchors.md`），SL-10 升为阻断，**两处必须同批改**。

### 4.6 门

`doc-style-lint.sh --self-test` PASS（含新夹具）、`anchor-migrate.sh --self-test` PASS、
`lint-review-rules.sh` 0 失败、`check-review-rules.sh` consistent、`doc-style-lint.sh --diff` error 级 0。

---

## 五、todo_plan 验收补跑（R3）与迁移后路径系统核查（用户第 1、3 项）

### 5.1 21 项验收汇总的补跑结论（证据已逐条写回 `prompt/todo_plan.md`）

| 段 | 结论 |
|---|---|
| A（5 项） | **5/5 达标**。A2 用埋点实测：往 `02-higher-half-kernel.md` 追加一行五类违规 → `--diff` 报出 SL-4/SL-5/SL-6/SL-7 各 1 处、error 级 4，只命中我埋的那行；`git checkout --` 后整文件 sha256 一致 |
| B（5 项） | **4/5 达标**；B5（端到端演练 5 点）无法复核——产物在 `.review/`，该区不入库，换环境即不可见（如实留未勾选） |
| G（5 项） | **G1 判定作废并合并**（见 5.3）、**G2 未落地且不能顺手加**（见 5.3）、G3 达标（脚本可用；`check-command-boundary.sh` 报 14 处存量违例，属登记不改码的范围裁决）、G4 因 G1 作废而半失效已改写 |
| H（6 项） | **H1/H2/H3 达标**（模板与脚本实测可用）；H4 的验收物是「用户认可」，按 0.2 第 2 条「任务先有触发」保持未勾选是正确状态 |

统计：勾选 12 项（带命令与输出片段），9 项显式标注未闭环/作废及理由，无一悬空。

### 5.2 迁移期路径检查的真实覆盖范围（回答「migrate 时是否已经检查过」）

**查过**：`notes/*` 旧路径归零、`{module}`→`{stage}` 键、锚点基线前缀、三端派生一致。
**没查过**：规则文本里对**脚本文件**与**目录**的引用是否存在。本轮补做（扫描 70 个规则/入口文件、
抽出 1123 处路径引用）→ 58 处指向不存在的路径，分三类：

| 类别 | 数量 | 处置 |
|---|---|---|
| 占位符与多行号写法（`minix3/minix/servers/模块/xxx.c`、`os/kernel/src/lib.rs:1170/1181/1193`） | 约 20 | 合法，不动（我的扫描脚本对「一行写多个行号」解析不足，非文档缺陷） |
| **幽灵工具引用** | 6 个名字、10+ 处 | 全部处置，见 5.3 |
| 落盘目录不存在（`tmp/log`、`tmp/bin`） | 2 | 预建四个约定目录 + 规则注明「重定向不会自动创建父目录，写前先建」 |

### 5.3 幽灵工具引用：最危险的一类缺陷

`prompt/review-rules/review-process.md` 把**强制证据行**「代码可读性增量」定义为
`tools/code-style-lint.sh --diff` 的结果，而**该脚本从未存在**（三端副本同病）。
执行 review 的 agent 拿不到这条证据，最自然的结局就是编一份输出——这是能直接伪造门结论的缺陷。

处置（原则：合并优于新建）：
- G1 卡作废：其拟查四项里，「`pub` 缺 `///`」「模块缺 `//!`」正是 rustc 的 `missing_docs`（属 G2 射程）；
  「`unsafe` 无 SAFETY」已由 `tools/unsafe-audit.sh` 覆盖（`--self-test` 实测退出码 0）；注释语言项依 OQ19 本就是 warning。
  自建 `code-style-lint.sh` 会与编译器与既有门三重重复 → 不写。
- 强制证据行改指真实手段：`cargo clippy -p {crate} --lib -- -D warnings` + `tools/unsafe-audit.sh --diff`，
  源、`.claude/rules`、`.claude|.codex/skills/review-scan/checks/process.md` 四处同改。
- `tools/todo-reference-validate.sh` → 改指**已实现**的 `tools/todo-staleness-check.sh`（职责完全一致）。
- `tools/ci-doc-test-count.sh`（从未实现，且只在运行时副本里要求——又一处反向漂移）→ 改为 `cargo test` + `rg -c` 人工对比，注明 Step 4.5a。
- G2 为何不能顺手加：`missing_docs = "warn"` 会让每个缺文档的公开项产生警告，而 CI 那条是 `-D warnings` →
  存量警告会当场把 CI 打红。必须先统计量级再按 OQ18 逐 crate 升级，配方已写进 todo_plan 的 G2 条。

### 5.4 把这条教训固化成门：`lint-review-rules.sh` 新增 L10

不变量：规则文本里出现的 `tools/*.sh|py`，若文件不存在，**同一行必须带状态标记**
（未实现 / 未来实施 / 从未 / 不存在 / 计划中 / 已作废 / 拟 / 建议 / 曾写作 / 未落地），否则 FAIL 并列出位置。
首跑抓到 10 处未标注引用（含我自己在 todo_plan 里刚写的作废结论——同一行判据把它们都揪了出来），
逐行补标注后归零。双向验证：植入 `tools/fake-ghost-check.sh` 一处引用 → `FAIL: L10 发现 1 处`、退出码 1；
还原 → `OK`，且还原前后文件 sha256 同哈希。

### 5.5 我自己犯的两个错，记录以免被当成「审计方挑刺」

1. **凭退出码下结论**：看到 `tools/doc-code-map.sh --help` 返回 2 就写成「不支持 `--help`」。实际它早已支持，
   只是把「显式请求帮助」当错误处理（打 stderr、退 2）。真缺陷是退出码语义 + `todo-staleness-check.sh`
   缺 `--help`。两处都改了，并把 todo_plan 里的措辞更正为本来的样子。
2. **埋点脚本用了不存在的文件名**：`printf >> rewrite-notes/01-stage-kernel/02-multiboot-header.md`
   凭空造出一个未跟踪文件留在文档树里（`git checkout` 因该路径不在索引而失败，没能兜住）。
   已用文件工具删除并核对工作树；教训是**写文件前必须断言目标存在**，这与迁移期 108 件静默退跟踪同源。

### 5.6 门

`lint-review-rules.sh` 0 失败（含新 L10）、`check-review-rules.sh` consistent、
`generate-derived-skills.sh --check` 无漂移、`diff-trae-skills.sh --only-diff` 9/9、
`doc-style-lint.sh --self-test` 与 `--diff`（error 级 0）通过；`bash -n` 覆盖全部 tools/*.sh。
