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
- `tools/todo-reference-validate.sh`（该脚本从未存在）→ 改指**已实现**的 `tools/todo-staleness-check.sh`（职责完全一致）。
- `tools/ci-doc-test-count.sh`（从未实现，且只在运行时副本里要求——又一处反向漂移）→ 改为 `cargo test` + `rg -c` 人工对比，注明 Step 4.5a。
- G2 为何不能顺手加：`missing_docs = "warn"` 会让每个缺文档的公开项产生警告，而 CI 那条是 `-D warnings` →
  存量警告会当场把 CI 打红。必须先统计量级再按 OQ18 逐 crate 升级，配方已写进 todo_plan 的 G2 条。

### 5.4 把这条教训固化成门：`lint-review-rules.sh` 新增 L10

不变量：规则文本里出现的 `tools/*.sh|py`，若文件不存在，**同一行必须带状态标记**
（未实现 / 未来实施 / 从未 / 不存在 / 计划中 / 已作废 / 拟 / 建议 / 曾写作 / 未落地），否则 FAIL 并列出位置。
首跑抓到 10 处未标注引用（含我自己在 todo_plan 里刚写的作废结论——同一行判据把它们都揪了出来），
逐行补标注后归零。双向验证：植入一处指向不存在脚本的引用（临时探针名 `tools/fake-ghost-check.sh`，该脚本不存在、仅作埋点）→ `FAIL: L10 发现 1 处`、退出码 1；
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

---

## 六、外部成熟做法的借鉴（R4：借原则不搬结构）

对照表全文写在 `prompt/agents-workflow-optim.md` 末节（含「明确拒绝的」一栏）。这里记落地与实测：

| 借到的原则 | 实测缺口 | 落地 |
|---|---|---|
| 技能描述写**触发条件**，否则 agent 看名字以为已知、跳过完整读取 | 9 个领域技能里 **8 个是主题式**描述（`grep -m1 '^description:'` 逐一看过，只有 socratic 带「当…时」） | 8 个 description 追加「用在…之时/之前」子句；重派生后 Codex 副本 824 字符（硬限 1024）、Trae 副本同样带触发语，`--check` 无漂移 |
| 新会话第一眼应是**任务→命令→技能决策树** | 6 个命令入口有清单但没有树；`review-cmds.md` 里只有表格 | 新增 §一b 决策树（文档/代码/测试/待办/只写不审五分支 + 判不了走苏格拉底 + 该在哪步查哪些模式两条出口），AGENTS.md 改为先指树 |
| 引用可解析性（外部框架靠 CLI 硬绑，本仓靠门） | 已在本轮第 5 节暴露 | 除 L10 外新增 **L11 幽灵模式号**（引用的「模式 N」必须在库里有定义，61/62 空号白名单）与 **L12 技能可达性**（每个技能目录必须被入口/命令/规则引用） |
| 未使用技能检测 | 实测 18 个技能目录全部被引用（最少 3 处）→ 当前**无死技能** | 不新建工具，改由 L12 常驻守护 |
| 阶段产物分离 / 持久状态机 / HANDOFF | 本仓已有同构物（outline→design→structure→SYMBOLS→VERIFY-CHECK；STATE.md + Resume Point） | **不搬**：改名或加 JSON 状态文件会制造第二真源，收益为零 |

L11/L12 首跑即全绿（存量干净），说明它们不会变成噪音门。L10 反而当场逮到我自己在
本日志里写的两处不存在脚本名（探针名与 todo-reference-validate）——**门对作者本人有效**，
这是比"门存在"更强的证据。

范围声明：本轮未动 `os/` 任何代码、未动三棵树任何正式文档正文（除埋点后已完全还原的那一行）。

---

## 七、模式库三副本漂移：我自己犯下的缺陷，以及把它变成门

### 7.1 发现过程（不是靠读文档，是靠交叉核对）

提交 TODO-2 之后我做了一次「三端引用一致性」抽查，命令里有一项是「模式 85 在规则源与派生副本各出现几次」，
结果是 `规则 2 / 派生 0` —— 顺着查下去发现模式库在本仓其实有**三份用途不同的副本**：

| 位置 | 内含模式条数 | 定位 |
|---|---|---|
| `prompt/review-rules/review-patterns.md` | 69 个 `### 模式 N` 标题（含 83 个编号） | 全量权威源 |
| `prompt/skill/review-patterns-skill.md` | 35 条，最大编号停在 **84** | review 时被调用的技能源（**我漏改了它**） |
| `.claude/skills/review-scan/checks/patterns.md` | 8 条（73–79 一族） | Claude 编排器按领域分片的高频子集 |

我的错误是**只改了规则源**，于是新总括模式 85 在执行 review 时根本取不到。而 `generate-derived-skills.sh --check`
一路报绿——它只比对「派生副本 vs 技能源」的差异，**看不出「源改了、上游没跟」这类内容滞后**。
这是本轮最严重的一次自我发现：门绿不等于事对。

### 7.2 修复

- 技能副本补入模式 85 整节（从规则源整段搬，不重写以免措辞漂移），并去掉技能描述里「86 个枚举模式」这种
  **双写计数**（数字必然与三份副本各自漂移），改为「全量条数与编号以规则源为准，本副本不重复计数」。
- 第三份（Claude 分片）只收 73–79 却不声明自己是子集，也不指路全量——加载它的 agent 会以为模式库只有 8 条。
  分片本身是有意设计（控上下文占用），所以不硬灌全量，改为在头部写死射程声明 + 「需要其它族必须调用
  `review-patterns-skill`，不能因为这里没有就当作该模式不存在」。
- 新增门 **L13**：技能副本必须收录规则源编号最新的 5 条模式，落后即 FAIL 并提示「新增模式必须同步
  `prompt/skill/review-patterns-skill.md` 再重派生」。

### 7.3 L13 的双向验证（附一次我自己的测试缺陷）

第一次反向验证假通过：我用 `re.sub(r'^### 模式 85', ...)` 造埋点却**忘了加 `re.M`**，`^` 只匹配字符串开头，
文件实际未被改动，所以门「没反应」——那是测试的缺陷，不是门的缺陷。重做时先断言改动数 = 1，再跑门：

| 场景 | 结果 |
|---|---|
| 未埋点 | `OK: L13 技能副本已收录规则源编号最新的 5 条模式`，退出码 0 |
| 埋点（技能副本模式 85 标题被改掉，改动数断言为 1） | `FAIL: L13 技能副本落后于规则源：缺模式 85`，退出码 1 |
| 还原 | `OK` + `完成：0 个失败`，且与备份 `cmp` 逐字节一致 |

教训补一条：**做埋点时必须断言「埋点真的写进去了」**（改动数 / 事后 grep），否则「门没响」会被误读成「门无效」或「改动无效」。

### 7.4 顺带发现但未处置（需你裁决）

`.claude/skills/review-scan/` 整个编排器**没有规范源**（`prompt/` 下不存在同名目录），README 第 175 行也承认
「`.claude/` 独立维护」。这与「`prompt/` 是唯一真相源」的通则冲突：它是第四处手写规则的地方。
两条路：① 把 `review-scan/checks/*` 收编进 `prompt/skill/review-scan/` 作为源，再派生到 `.claude/`；
② 承认它是 Claude 专用的运行时编排层，在 `prompt/README.md` 与 `AGENTS.md` 里把它**明确列为例外**并规定
「例外文件必须自带指路声明」（本轮已给它补上声明）。我倾向 ②（收编会把分片设计的收益抹掉），等你定。

---

## 八、review-scan 编排器收编进规范源（用户第 1 项的答复与实施）

### 8.1 先回答"是不是 Claude 特有功能"

查证的结论：**只有一个字段是 Claude 专有的** —— `.claude/skills/review-scan/SKILL.md` frontmatter 里的
`allowed-tools: Read, Grep, Glob, Bash, Write`（限定该技能可使用的工具集）。其余内容（编排规则、5 个
checks 分片、Gate 序列、证据格式）完全可移植，`.codex/skills/review-scan/` 就是同一套内容的另一份拷贝。
所以按用户方向执行：吸收进规范源，三端从源派生。

更要紧的事实是收编前的状态：`generate-derived-skills.sh` 第 26 行原文写着
「9 个领域 Skill（review-scan 编排器由 `.claude/` 独立维护，不在此生成）」——即 **两份手抄件、零规范源**；
`prompt/README.md` 的派生表里那一行还自我承认「AGENTS.md 声明派生自 prompt/，实际 review-scan 从 prompt/skill 演进」，
名实不符是文档里公开承认的。这正是用户担心的可维护性问题：运行时端当了源。

### 8.2 合并方法（不手抄，避免再引入一次漂移）

写了一次性脚本做**自动合并**（用完即删）：把 `.claude` 与 `.codex` 两份先做路径归一
（`.claude/`↔`.codex/`→`@@RT@@/`，`.review/claude`↔`.review/codex`→`@@REVDIR@@`），再用 difflib 逐行对齐：
相同块原样进源，不同块写成 `<!-- @if:claude -->` / `<!-- @if:codex -->` 条件对。产出
`prompt/skill/review-scan/`（6 个文件、2566 行、55 处条件块；其中 doc/code/excellence 三份两份手抄件完全一致，
0 处条件）。差异分布：SKILL.md 10 处、checks/process.md 12 处、checks/patterns.md 5 处。

派生侧新增 `render_scan()` + `generate_scan()`：按目标裁剪条件块、展开 token，并把结果写进
`.claude/skills/review-scan/` 与 `.codex/skills/review-scan/`；`--check` 现在覆盖这 12 个派生文件。
Trae 无编排器（走 `review-agent-ide` / `review-agent-trigger` 定义），保持不生成并在注释里写明。

### 8.3 无损性验证（这一步不能省）

| 验证 | 结果 |
|---|---|
| 重新派生后与收编前的两份手抄件比对 | **差异 0 行**（合并没丢内容，也没改语义） |
| `generate-derived-skills.sh --check` | 绿（含 12 个 review-scan 派生文件） |
| 反向埋点：手工往 `.codex` 派生件加内容 | `--check` 退出码 1（漂移被抓）；还原后复绿 |
| `check-review-rules.sh` | 由「运行时层手工维护」改为**断言规范源 6 个文件齐备** |

### 8.4 过程中被自家门抓到的两处（都是我的错，且门是对的）

1. 条件块里保留 `@@REVDIR@@` token → L2（旧名活引用检查）报
   `prompt/skill/review-scan/checks/patterns.md:450` 命中。根因是**条件块内容本就是目标专属的**，
   那里放 token 毫无意义：`.review/claude/03-stage-kernel/…` 是一条真实的历史出处（该冻结目录至今存在），
   被 token 化后两个运行时都会读到一条不存在的路径。修法：条件块内一律写实际路径，只有两边共享的行留 token
   （共 32 行去 token）。修完 L2 归零、派生仍与手抄件零差异。
2. 第一次建 frontmatter 时把行内 `<!-- @if -->` 少写闭合，且我用 `lstrip` 多加了一个空行 → 派生件多一行。
   改为统一「标记独占一行」的单约定，并按真实副本重建头部；空行归一后差异回到 0。

### 8.5 文档一致性同步

`AGENTS.md` 一致性约定、`prompt/README.md` 的目录树行 / 派生关系表 / Codex 适配表三处均已改写：
现在除 `.claude/rules/*` 三份运行时副本外，**全部**从 `prompt/` 派生，第四处手写规则的位置没有了。

### 8.6 顺带更正我上一条回复里的两处范围错判

- 「OS 语义层依赖射程需要可执行守卫，且要先裁 PD-33 豁免」——**这是代码线的事**。工作流侧的能力已经齐了
  （§1.6 射程定义 + 三问 + 模式 85 的 grep 判据 + §16.7 强制栏）。本轮不造扫 `os/` 的守卫。
- 「G2 落地要不要开批跑 cargo check」——**同样越界**（动的是配置与代码），已从待办建议里撤回，
  它作为 todo_plan 的未落地项登记在案即可。

---

## 九、授权规则清理（用户批准的第 4 项）

### 9.1 触发点与实测

Claude Code 启动时对本仓 `.claude/settings.local.json` 打印告警：形如
`Bash(rg "\^\s*#{2,3}\s+.*(...)" notes/rewrite/...)` 的授权规则里，通配符出现在命令中段，
**等于允许在该位置插入任意参数而不再询问**。分类实测（脚本按「通配符是否在末尾」判定）：

| 类别 | 条数 | 处置 |
|---|---|---|
| A 死授权（参数路径已退役：`notes/rewrite/…`、`fork-syscall-rewrite`） | 41 | 删除（永不命中，且其中 17 条整条命令仍可跑，只是路径参数过期） |
| B 中段通配 | 13 | 删除——逐条看过，**全部是某次会话的一次性 grep/rg**，没有 cargo/git 类可复用项，改写成前缀授权没有价值 |
| C 精确命令、无通配 | 59 | 保留 |
| D 末尾通配前缀授权（`cargo check *`、`git status *` 等） | 12 | 保留 |

125 条 → 71 条。方向上只做收紧：**只删不加**（删除只会让 Claude 多问一次，是安全收益；
新增或放宽授权才是风险，本轮一律不做）。

### 9.2 验证

- 落笔后自检：JSON 可解析、条数 71、残留危险条目 0（同一判据复跑）。
- 旧路径字面量归零：`grep -c "notes/rewrite\|fork-syscall-rewrite" .claude/settings.local.json` = 0。
- 用本机 Claude Code 复跑同一条命令：改前会打印该告警，改后不再打印（配置校验发生在模型调用之前，
  本机模型订阅过期不影响这条判据）。
- **不可回退性说明**：该文件**不被 git 跟踪**，所以这次改动无法用 git 回滚。改前 125 条的完整副本
  已按落盘规则存进 `tmp/evidence/20261007-settings-cleanup/settings.local.json.before`
  （sha256 前缀 `c25c59ca7a4a6c6c`；改后文件 `9c02bdde08c6dd6b`）。要恢复直接覆盖回去即可。

### 9.3 顺带消解一处迁移白名单

`rewrite-notes/MIGRATION.md` 六b 的门 M2 白名单原本有三处，其中
`.claude/settings.local.json`（38 行旧路径）这一条随本次清理自然消失，白名单减为两处，
文档已就地更正并注明改前副本位置。

---

## 十、三方独立审计的回应（DS 4P1/10P2、GLM 4P1/12P2、Muse 4P1/6P2，P0 均为 0）

三方独立指向同一批 P1，逐条自己复算后处置。**先记一条我自己的不实陈述**：提交 `7c13acfc6` 的说明把
P1-2 列进「已核实并修复」，但正文只做了 P1-3 与 P1-4 —— P1-2 当时未动。本节的提交单独补做并更正，
不追溯改写那笔提交（历史里留着，读到这里的人应知道真实顺序）。

### 10.1 四条 P1 的核实与处置

| 项 | 复算结果 | 处置 |
|---|---|---|
| P1-1 门扫描域污染 | 证实：`lint-review-rules` 红 33 处，**全部**落在 `prompt/{DS,glm,mimo}/evidence/`，规则本体零命中；且证据文件互抄对方行号，清单从 3 涨到 25 再涨到 33 | L1/L2/L3/L10/L11 的扫描域改为 `git ls-files` 受跟踪清单（语义更正确：门守入库内容），未跟踪审阅区天然除外；修后 33 → 1 处，而那 1 处是我上批把 AGENTS.md 节标题从 "Hidden Folder Convention" 改中文、冲掉 L1 守卫语境豁免词所致，已补词；新增 L16 提示未跟踪审阅产物的存在 |
| P1-2 规范源无 Codex、仍是两运行时模型 | 证实：`review-process.md` 三处「双路径」只列 Trae/Claude，全文 Codex 提及 0 次；而 `.review/` 下实际有 `trae claude codex zcode qoder` 五个运行时目录 | 状态模型改成 `.review/{tool}/{stage}/` 通用式 + **已登记运行时表**（五行，含各自产物布局、新运行时接入方式、`.review/` 旧模块名目录属冻结区的说明）；派生**不再做运行时身份替换**（谁读谁对号）；同步 `review-process-skill.md`、`review-coverage-skill.md`、`prompt/README.md` 6 处描述、Trae agent 提示词的章节标题与规则行 |
| P1-3 退役遗漏 | 证实：根 README 两处仍称 CLAUDE.md 是入口并在阅读路线里让读者去读它；正式文档 5 文件 6 处规范性引用；`os/plat/src/lib.rs` 文档注释 | 全部改指 AGENTS.md（历史划线条目与"解释退役"的叙述保留）；GLM 把该文件归到 `09-stage-init/` 是位置标错，实为 `11-stage-devman/`，问题为真 |
| P1-4 计数散点漂移 | 证实：同步表写 `review-process-skill` 65,677 而实测 56,756（−14%）；索引表区间停在 79-84；Agent Prompt 预算此前无任何机器检查 | 抄表字符数全部撤除改为可复跑命令；对应关系表删掉「当前字符」列；索引表 79-85；新增 L14（区间内每个编号必须真实存在）、L15（预算 + 禁止再抄字符数，抄了就红） |

### 10.2 采纳的 P2（四条）与新增的门

- **L17** 守护最后一处无守护手抄件：`.claude/skills/review-implementation-skill/SKILL.md` 与规范源
  归一化（只抹 frontmatter 与相对链接深度）后必须零差异；埋点验证过——往派生件塞两行就 FAIL，
  还原后 sha256 一致且门复绿。
- **`lint-review-rules.sh --self-test`** 把本轮反复手工做的双向验证固化：正向（未标注幽灵引用必须被抓、
  已标注与守卫语境行不得误伤）+ 反向（夹具全合规时门必须绿，排除"恒红"）。它同时能识破"恒绿"：
  若 L10 失效，正向断言就会失败。
- `tools/design-coverage-check.sh` 补 `--help`（显式请求帮助退 0，此前落到"参数错误"分支）。
- `.claude/rules/review-core.md` 模式清单加子集声明：它是 Claude 每会话必载文件，只列 48–78 却不声明
  射程，读者会以为模式库到 78 为止；现写明"79–85 不在此，需其它族必须调用 review-patterns-skill"。
- 模式 83（缺锚 = P0）与 `review-doc-checklist.md` §3.95（坐标霸占读者版面）互指一行，
  防止只读模式库的人反向操作——把坐标越补越多。
- 新增「审阅工作区与门的扫描域」约定写进 `prompt/README.md`：审计目录属第三方区；捕获门输出要脱敏
  （门是文本匹配的，抄一次等于把问题复制一份）；写入边界只限自己的目录；**埋点必须断言落盘**。

### 10.3 驳回的三条（都带复算证据）

1. GLM/Muse 称 README 有「≤197 字符（review-scan 最长）」宣称——**全仓不存在此句**，README 只有 ≤1024
   硬限；197 是他们自己实测 review-scan 描述长度，被误当成 README 的宣称。
2. GLM 称 opencode.json 只注册 6 个命令里的 2 个——实测 6 个命令名全部在册，不复现（Muse 亦判其不复现）。
3. SL-10 存量数 3028 vs 我的 3085——按我声明的同一扫描集（rewrite-notes 735 篇，排除 `.design` 与 archive）
   复测仍是 3085，属扫描集口径差异而非声明错误；热区前六名（178/109/88/76/75/70）双方精确一致。

### 10.4 我在这批修复里犯的三处错（不推给"环境"）

1. **同错互为印证**：L17 归一化正则里我把 `../` 写成了「三个转义点加斜杠」（多写一个点），而我用来做交叉验证的
   python 片段抄了同一条错正则 → 两边一致地不匹配，看起来"验证通过"。真正暴露它的是手工 diff。
   教训：交叉验证必须用**独立实现**（此处应改用 python 的字符串替换或逐行人工看），同源双跑等于没验证。
   正确式子 `(\.\./){1,3}`。
2. **三次补丁静默未写入**：用整串匹配定位代码块时假设了文件内容，断言失败即中止，其中一次让门继续红着
   我却以为改了。改为按行定位 + 断言命中数，并在改动后立刻读回确认。
3. **差点新造一类过度宣称**：把 Trae agent 提示词的章节标题写成"五个运行时"，而正文只有 Trae/Claude 两段
   ——改成"本文件只列两者的产物路径"+ 指向完整运行时表。这与我在修的缺陷同源：标题说的事，正文得做得到。

### 10.5 未闭环的两条 P2（有意不做，理由）

- **四道门进 CI**（GLM P2-1、Muse P2-1，均称性价比最高的单条自动化）：技术上现在就做得动，
  但它会把"审计窗口期"的红变成 CI 的红——P1-1 已修（扫描域只含入库文件），前提其实满足了。
  之所以仍留待你定：CI 一旦常驻，任何人改规则忘派生就会红全局检查，这是工作节奏的取舍，该由你决定
  而不是我顺手加。命令已经收口成一条：`bash tools/lint-review-rules.sh && bash tools/check-review-rules.sh && bash tools/generate-derived-skills.sh --check && bash tools/diff-trae-skills.sh --only-diff`。
- **体量拆分**（review-process.md 约 163KB 等）：审计方建议等 TODO-1 存量回炉后再评估，本轮只保证
  渐进加载机制可用（决策树 + 薄壳命令 + 常驻最小集），不动结构。
