# prompt/ — 规则与技能的唯一真相源

> 项目入口是根目录 `AGENTS.md`；本目录只存规范源与派生源。运行时副本
> （`.claude/`、`.codex/`、`.trae/`、`.agents/`）不得手写新规则。
>
> 改动顺序固定：**改这里 → 生成派生件 → 跑四道门 → 提交**。

## 一、目录

```
prompt/
├── README.md                    — 本文件：目录、派生关系与同步门
├── review-rules/                — 规则规范源（唯一权威）
├── skill/                       — Skill 适配层源
│   ├── cmds/                    — 6 个任务命令薄壳
│   ├── review-scan/             — review-scan 编排器（SKILL.md + checks/）
│   ├── review-agent-ide.md      — Trae Agent 提示词（路由器）
│   ├── review-agent-trigger.md  — Trae Agent 触发描述
│   └── review-*-skill.md        — 9 个领域 Skill + review-loop
├── WORKFLOW-OPEN-ITEMS.md       — 裁决与开放项存档（非规则）
├── WORKFLOW-ADVICE-ARCHIVE.md   — 历史删档承接与出处（非规则）
├── DOC-ITER-WORKFLOW.md         — 文档迭代统一规格（review-panel P-READER 引用）
├── EXECUTE-D01-D06-CONVERGENCE.md — 执行入口（非规则）
├── V3.6-USABILITY-REPORT.md     — 历史可用性报告（非规则）
└── TODO-MIGRATION-2026-11-21.md — 01/02 旧 TODO 迁移记录（非规则）
```

`tmp/` 是会话产物与取证目录，整域不入库；`rewrite-notes/` 是文档与协作账本树。

## 二、review-rules/（15 个文件）

| 文件 | 职责 |
|---|---|
| `review.md` | 核心不变量：执行模型、真值优先序、三档术语、P0 六分类 |
| `review-process.md` | Step 0–7 全流程、Blocker Gates、Gate 证据分级、状态写入与收敛 |
| `review-cmds.md` | 6 个任务命令入口、决策树、不可裁剪的通用强制门 |
| `fix-guard.md` | 修复守则：动手前 5 件事、一次一条、修后留痕 |
| `review-patterns.md` | 错误模式库（编号与条数以文件头索引行为唯一口径） |
| `review-doc-checklist.md` | 文档检查清单与 Claims-Evidence、Doc Code Sync 七项 |
| `review-code-checklist.md` | 代码检查清单（含内核 SMP/BKL 并发） |
| `review-core-semantics.md` | 核心语义对齐与行为契约表模板 |
| `review-doc-excellence.md` | 文档卓越性：叙事、读者体验、教学深度 |
| `review-code-excellence.md` | 代码卓越性：API、表达力、性能、可测试性 |
| `review-publication.md` | 出版标准 V3.7：DP-1..DP-6、CP-1..CP-6、零欠账、顺序迭代 |
| `review-tutorial-rubric.md` | 教程 rubric V3.8：TR-1..TR-6 与 G13 |
| `review-units.md` | 单元级 review V3.9：`###` 节冻结、T1/T2/T3 三轨、G14 |
| `review-panel.md` | review-loop 独立面板：角色、材料隔离、归因 |
| `review-evals.md` | review-loop 评测：G1–G14、§6A 四态收敛模型 |
| `review-profiles.md` | 旧 Profile A–P/R/AG 的组合配置（保留为别名对照） |

## 三、skill/（适配层）

- **领域 Skill 源（10 个单文件）**：`review-code-skill`、`review-doc-skill`、
  `review-patterns-skill`、`review-process-skill`、`review-core-semantics-skill`、
  `review-coverage-skill`、`review-excellence-skill`、`review-implementation-skill`、
  `review-socratic-skill`、`review-loop`。
- **编排器**：`review-scan/SKILL.md` + `checks/{doc,code,patterns,process,excellence}.md`。
- **任务命令薄壳**：`cmds/{full-review,style-fix,code-excellence,test-audit,todo-fix,style-bible}/SKILL.md`。
- **Trae Agent**：`review-agent-ide.md`（路由器，字符预算由 lint 的 L15 把守）与
  `review-agent-trigger.md`（触发描述）。

规则源到 Skill 的对应关系：

| 规则源 | Skill |
|---|---|
| `review-doc-checklist.md` | `review-doc-skill` |
| `review-code-checklist.md` | `review-code-skill` |
| `review-patterns.md` | `review-patterns-skill` |
| `review-process.md` | `review-process-skill` |
| `review-core-semantics.md` | `review-core-semantics-skill` |
| `review-doc-excellence.md` + `review-code-excellence.md` | `review-excellence-skill` |
| `review-process.md` §覆盖率穷举 | `review-coverage-skill` |
| `review-process.md` §实施验证 | `review-implementation-skill` |
| `review.md` 苏格拉底追问 | `review-socratic-skill` |
| `review-panel.md` + `review-evals.md` + `review-units.md` | `review-loop` |
| `review-cmds.md` | `cmds/*/SKILL.md` |
| `review.md` + `review-profiles.md` | `review-agent-ide.md` / `review-agent-trigger.md` |

Skill 是规则的适配层（frontmatter、触发条件、输出模板），**不会自动同步文本**：
`check-review-rules.sh` 的计数检查只能捕获结构性漂移，段落内文字漂移由 review 保证。

## 四、派生关系

| 目标 | 来源 | 方式 |
|---|---|---|
| `.trae/skills/<name>/SKILL.md` | `prompt/skill/<name>.md` | `generate-derived-skills.sh` 自动生成；frontmatter 去引号，链接按派生深度改写 |
| `.trae/skills/review-agent-ide`、`review-agent-trigger` | `prompt/skill/review-agent-*.md` | 同上 |
| `.codex/skills/<name>/SKILL.md` | `prompt/skill/<name>.md` | 同上；`description` 加双引号（≤1024 字符） |
| `.claude/skills/review-scan/`、`.codex/skills/review-scan/` | `prompt/skill/review-scan/` | 生成时按 `<!-- @if:claude -->` / `<!-- @if:codex -->` 裁剪，展开 `@@RT@@`、`@@REVDIR@@` |
| `.claude/skills/review-implementation-skill/` | `prompt/skill/review-implementation-skill.md` | 手工同步（运行时路径与源不同） |
| `.claude/rules/{review-core,review-process,fix-guard}.md` | `review-rules/` 对应源 | 手工同步的 always-on 副本 |
| `.agents/skills/{full-review,style-fix,code-excellence,test-audit,todo-fix,style-bible}` | `prompt/skill/cmds/*` | 软链；L8 校验可解析 |
| `.agents/skills/review-loop` | `prompt/skill/review-loop.md` | 软链 |

`.trae` 不使用 review-scan 编排器，走 review-agent 定义；`.codex` 与 `.claude`
使用同一份 review-scan 源。

## 五、改完必跑的四道门

```bash
tools/generate-derived-skills.sh          # 重新生成 .trae/ 与 .codex/
bash tools/check-review-rules.sh          # 源与派生件一致性、计数、引用
bash tools/lint-review-rules.sh           # 字符预算、frontmatter、软链、元规则锚点
bash tools/diff-trae-skills.sh --only-diff # Trae 派生件逐文件差异
```

只想检查漂移、不写文件：`tools/generate-derived-skills.sh --check`（退出码 0 = 无漂移）。

CI 的 `rules-consistency.yml` 在 `prompt/`、`.claude/`、`.codex/`、`.trae/`、`.agents/`、
`tools/`、`AGENTS.md` 变更时自动跑上述门，并跑各工具的 `--self-test` 与
`lint-review-rules.sh --self-test`，不依赖人工记得执行。

**硬限制（由 lint 把守，不在文档里抄当前读数）**：

- Trae Agent Prompt ≤ 10,000 字符；超限必须精简规则或下沉到 Skill。
- Codex Skill `description` ≤ 1024 字符，且必须双引号包裹；`name` 与父目录同名。
- Skill `name` 只允许小写字母、数字与连字符。

## 六、运行产物去哪

| 产物 | 位置 |
|---|---|
| Review 状态与 scan/structure/SYMBOLS/独立验证报告 | `.review/{tool}/{stage}/`（三工具隔离） |
| review-loop 轨迹、面板、冻结决策 | `rewrite-notes/coordination/review-eval/` |
| TODO 单一账本 | `rewrite-notes/coordination/todo-ledger/<stage>.yaml` |
| 会话日志、镜像、一次性取证 | `tmp/<功能子目录>/`（不入库） |

## 七、维护边界

1. **规则只写在 `review-rules/`**；Skill 只做适配，不新增约束。
2. **派生文件不手改**：改源后重新生成；手改会在下一次同步时被覆盖，并让门变红。
3. **新增规则要有真实案例**：目标文件、期望效果、回归风险、替代或合并了哪个旧装置；
   没有替代对象的纯新增必须说明旧装置为何接不住。
4. **过程报告不是规则**：`WORKFLOW-*.md`、`V3.6-USABILITY-REPORT.md`、迁移记录等
   只作存档与溯源，不参与门禁判定。
