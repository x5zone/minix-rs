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
