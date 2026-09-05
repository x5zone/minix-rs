# prompt/

本目录包含 Minix-RS 项目的 Review 规则集及**三套** IDE/Runtime 适配产物，均由 `prompt/` 单一规则源派生：**Trae IDE**（手工复制粘贴 + `.trae/skills/`）、**Claude Code Runtime**（项目根 `CLAUDE.md` + `.claude/` 自动加载）与 **Codex CLI**（项目根 `AGENTS.md` + `.codex/` 自动加载）。三者内容同源，仅适配各自工具的加载机制与字段限制（见下方各章节）。

## 目录结构

```
prompt/
├── README.md                — 本文件
├── review-rules/            — 原始 Review 规则集（唯一真相源）
│   ├── review-cmds.md       —   任务命令规范（6 个独立 cmd：full-review/style-fix/code-excellence/test-audit/todo-fix/style-bible；Profile 对账 + scope 参数，2026-09-05）
│   ├── review.md            —   Review 核心框架（原则、约束、优先级、输出模板、执行模型分层）
│   ├── review-doc-checklist.md  —  文档检查清单（§1~§3，含 §2.0 Claims-Evidence）
│   ├── review-code-checklist.md —  代码检查清单（§1~§15，含 Kernel SMP/BKL 并发）
│   ├── review-patterns.md       —  常见错误模式（84 个枚举模式，含文档/代码/测试/卓越性/流程/架构抽象与锚点纪律）
│   ├── review-process.md        —  执行流程（§〇三模式 + Step 0~7 + 状态追踪 + 收敛判断）
│   ├── review-profiles.md       —  任务组合配置（Profile A~P + R + AG，含分阶段 H~K + 卓越性 O + 覆盖率 P）
│   ├── review-core-semantics.md —  核心语义对齐（行为契约表 + IPC/生命周期契约模板）
│   ├── review-doc-excellence.md —  文档卓越性（§4.1叙事结构 + §4.2读者体验 + §4.3教学深度 + §4.4可维护性）
│   └── review-code-excellence.md—  代码卓越性（§16 API设计 + §17表达力 + §18性能 + §19代码即文档 + §20可测试性 + §21测试质量）
├── skill/                   — Skill 适配层源文件（9 个领域 Skill；同步至 Trae/Codex，review-scan 编排器见 .claude/.codex）
│   ├── review-agent-ide.md      —  Trae 智能体精简版（⚠️ 9,739 字符，余量 261，见下方说明）
│   ├── review-agent-trigger.md  —  触发器描述（何时调用 Agent，12 个示例覆盖 8 域 + 工作流评估/修复/快照补齐阶段）
│   ├── review-doc-skill.md      —  文档 Review 技能（含 §2.0 Claims-Evidence）
│   ├── review-code-skill.md     —  代码 Review 技能（含 §4.2 Kernel SMP/BKL 并发）
│   ├── review-patterns-skill.md —  错误模式对照技能（含测试6+卓越性7模式）
│   ├── review-process-skill.md  —  执行流程技能（含 §〇三模式 + STATE.md 格式 + 状态追踪 + 独立验证）
│   ├── review-core-semantics-skill.md — 核心语义技能（行为契约表模板）
│   ├── review-excellence-skill.md    — 卓越性技能（文档+代码卓越性检查流程）
│   ├── review-coverage-skill.md      — 覆盖率技能（机器穷举 + AI 语义判断）
│   ├── review-implementation-skill.md — 实施验证技能（design ↔ code 一致性 + §X self-review issues 追踪）
│   └── review-socratic-skill.md      — 苏格拉底追问技能（13 场景追问话术模板）
└── README.md                — 本文件
```

`.review/` 目录为 Review 执行时自动生成的**状态持久化输出**，不在本目录中（见下方「状态管理与收敛」）。

---

## 🆕 Design-First 升级摘要（2026-07-15）

> **核心机制**：引入 Design First 原则，使 design 本身成为 review 的核心 deliverable。修复"DEFERRED 逃避"问题。

**主要变更**：
1. **三层术语统一**：Rewrite / Refactor (code + design) / Architectural Evolution，废弃"Redesign"模糊用法
2. **P0 六分类**：在原 5 类（P0-fact/code-bug/...）基础上新增 P0-test-missing + P0-design-deviation/missing/wrong
3. **优先级链**：Minix3 源码行为 > design doc > Rust 代码 > 设计/技术文档
4. **Gate H**：design 门控（**所有 review 模式必检，2026-07-16 扩**），含 6 项 design 对齐检查 + design 缺口清单 + IN_DESIGN 状态管理
5. **Profile R**：设计优先模式 Review — `review xxx-design.md`（非 bagging）/ `review xxx-design-final.md`（bagging）时加载，或 Step 0 design 预检发现 design 缺失时自动触发；验证 design 完整性 + 可实现性 + design ↔ code 一致性
6. **Review 中断协议**：当 design 缺关键决策时，4 步中断 + IN_DESIGN 状态机（替代 DEFERRED 逃避）
7. **IN_DESIGN 时间上限**：7 天警告、30 天清理、月度审计
8. **Pattern 63/64/65**：Design-Missing / 开发文档味 / Translate 倾向
9. **架构演进 5 类型**（§2.0）：FPU / 中断 / 分页 / 地址空间 / 启动链 — 显式标注 ARCH
10. **Scan 文件生命周期协议**：防止 P0-XX-1 编号泄漏到 doc/code
11. **🆕 2026-07-16 workflow 修复**：Session #12 复盘发现 Step 0 design 预检被错误跳过
    - **Gate H 扩为所有 review 模式必检**（原"仅完整/深度/设计优先"）
    - **模式 69 PSMD** (Per-doc Snapshot Missing)：per-doc design/outline 快照缺失检测
    - **模式 70 CTOS** (Cross-Turn Outdated Staleness)：TODO 列表跨轮状态陈旧检测
    - **模式 71 DOG** (Decision Over-Generalization)：用户决策泛化误用检测
    - **硬阻断规则**：Step 0 启动时必须跑 4 条 `ls .design/{NN}-*.md`，缺失即 FAIL
    - **工具**：`tools/design-coverage-check.sh {module}` 自动扫描所有 stage 缺失报告
    - **STATE.md Resume Point 模板**：续 session 必含 `ls .design/` 预检命令
    - **scan.md Gate 0 锚段扩为 9 个**：新增 `§Step 0: 预检结果` 锚段

**变更覆盖范围**（同 commit 同步）：
- `prompt/review-rules/` — 9 个文件（review.md / review-process.md / review-patterns.md / review-doc-checklist.md / review-code-checklist.md / review-core-semantics.md / review-profiles.md / review-doc-excellence.md / review-code-excellence.md）
- `prompt/skill/` — 11 个文件（review-process-skill / review-patterns-skill / review-doc-skill / review-code-skill / review-core-semantics-skill / review-coverage-skill / review-excellence-skill / review-implementation-skill / review-socratic-skill / review-agent-ide / review-agent-trigger）
- `CLAUDE.md` — 项目根（新增 Design First 章节 + Gate H）
- `.claude/rules/` — review-core.md / review-process.md（新增术语 + Gate H）
- `.claude/skills/review-scan/` — SKILL.md + 5 checks/*.md（新增 Phase 2.6 + Pattern 63-65）
- `.trae/skills/` — 9 个 Skill subdir + 2 个 Agent subdir（Skill 自动同步，frontmatter 差异 + markdown 链接路径适配）

---

## review-rules/（唯一真相源）

Review 规则集是项目在多轮迭代中积累的规则文档，定义了针对 Minix-RS 项目的所有 Review 要求。其逻辑顺序为：

0. **review-cmds.md** — 任务命令规范（调度层）。6 个独立 cmd（full-review/style-fix/code-excellence/test-audit/todo-fix/style-bible）是任务一级入口：单一目标 + scope 参数（section/chapter/doc/range/dir）+ 强制门 + "不做"边界；含旧 Profile A-P/R/AG 的对账表（2026-09-05）。
1. **review.md** — 核心框架，定义 **Rewrite / Refactor (code Refactor + design Refactor) / Architectural Evolution** 三层术语（原则层）。含执行模型按模块分层（用户态服务器单线程 vs 内核 SMP+BKL）、核心语义验证（Ground Truth 具体化，引用 review-core-semantics.md）、**Design First 原则 + §2.0 架构演进作为独立知识点维度 + P0 六分类（含 P0-design-deviation/missing/wrong/test-missing）**。
2. **review-profiles.md** — 任务组合配置，决定不同场景加载哪些规则模块（策略层）。含 Profile O（卓越性专项）、Profile P（覆盖率专项）。
3. **review-process.md** — 强制执行步骤，要求每个步骤必须产生可见中间产物（流程层）。含状态写入与收敛判断、Review Verification Protocol、§〇 执行模式选择（构造/快速/深度三模式）、Step 1.5 覆盖率穷举。
4. **review-doc-checklist.md** — 文档维度的检查清单（文档维度层）。含 §2.0 Claims-Evidence Tracing（论文级文档质量方法论）。
5. **review-code-checklist.md** — 代码维度的检查清单（代码维度层）。含 §4.2 内核 SMP/BKL 并发检查项。
6. **review-patterns.md** — 常见错误模式汇总（错误模式层）。含 Kernel SMP 并发、测试、卓越性、叙事、Design-First 和流程漂移模式，共 84 个（81 个编号 1-60、63-83 + A/B/C 字母；61/62 已合并至 60 保留空号；79-83 为 2026-09-05 新增的架构抽象与锚点纪律模式）。
7. **review-core-semantics.md** — 核心语义对齐。定义核心语义不变性原则，提供函数/IPC/生命周期行为契约表模板。
8. **review-doc-excellence.md** — 文档卓越性。§4.1 叙事结构、§4.2 读者体验、§4.3 教学深度、§4.4 可维护性。
9. **review-code-excellence.md** — 代码卓越性。§16 API 设计、§17 表达力、§18 性能、§19 代码即文档、§20 可测试性、§21 测试质量。

---

## skill/（Skill 适配层源文件）

`skill/` 目录下的文件是 Trae IDE 的 Agent 与 Skill 定义，由 `review-rules/` 规则集转化而来，是 **`.trae/skills/` 与 `.codex/skills/` 的同步源**。Trae 使用方式为**手工复制粘贴**至 IDE 配置入口（不是项目级自动加载）；`.codex/skills/` 的同步规则见下方「三端同步说明」。

### Trae IDE 字符/字段限制（关键）

> 以下限制来自 Trae 官方文档与社区实测，**直接决定 skill 文件能否保存与生效**。新加内容前先核对字数。

| 项 | 限制 | 来源 | 当前文件 | 状态 |
|---|------|------|---------|------|
| **Agent Prompt（提示词）** | **硬上限 10,000 字符**（自动截断） | [Trae 官方 FAQ](https://forum.trae.cn/t/topic/7571) | `review-agent-ide.md` | **9,751 字符（≈ 97.5%）✅ 达标，余量 249 字符**（2026-09-05 实测） |
| Rule（规则） | 硬上限 20,000 byte；建议 ≤ 10,000 字符；token 视角约 3,000 token | [Trae 官方 FAQ](https://forum.trae.cn/t/topic/52) | n/a（本目录无 Rule 文件） | — |
| **Skill `name`** | ≤ **64 字符**，仅小写字母/数字/连字符（`-`），与父目录同名 | [Trae Skill 规范](https://docs.trae.ai/ide/best-practice-for-how-to-write-a-good-skill) | n/a（Trae Skill 命名规范） | — |
| **Skill `description`** | ≤ **1024 字符**（硬限制），建议 ≤ 200 字符 | 同上 | n/a | — |
| Agent `description`（触发器描述） | **硬上限 5,000 字符**（IDE 编辑窗口提示） | Trae IDE 实测 | `review-agent-trigger.md` | **4,001 字符 ✅**（含 12 个触发示例，覆盖全部 8 域 + 工作流评估/修复/快照补齐阶段） |
| MCP 工具总数 | ≤ **40 个** | [TRAE MCP 指南](https://blog.csdn.net/2601_96144997) | n/a | — |
| MCP 描述总字符数 | ≤ **8,000 字符**（超出丢弃） | 同上 | n/a | — |
| 上下文窗口（最强模型） | 240,000 tokens 输入 / 32,000 tokens 输出 | [Trae 模型文档](https://docs.trae.ai/ide/models) | 全局 | — |

### `review-agent-ide.md` 的 10,000 字限制说明

- **当前 9,751 字符，已达标但余量 249 字符**（硬上限 10,000 字符的 97.5%，2026-09-05 实测）。**新增任何约束前必须先核对余量；超 10,000 字符必须触发规则精简**（候选：精简重复条目 / 下沉更多详情到 Skill）。
- **结构**：Agent 作为**路由器**，详细知识下沉到 8 个 Skill：
  - Core Principles 保留最核心原则；
  - Output Template、Review Process、Phased Review 详情引用 `review-process-skill.md`；
  - Quick Cheat-Sheet 精简为条目式；
  - 苏格拉底追问话术拆分到 `review-socratic-skill.md`。
- **新增强制约束**：
  - **Explicit Skill Invocation**：必须通过 `Skill` tool 显式调用 Skill，禁止依赖隐式加载。
  - **STATE.md 双路径**：Trae IDE 与 Claude Code Runtime 分别使用不同的 STATE.md 路径，互不覆盖。
  - **Gate 证据规则**：每个 Blocker Gate 必须附带命令 + 输出片段证据。
  - **Gate D 严格通过**：PARTIAL/⚠️/部分通过 均视为 FAIL。
  - **VERIFY-CHECK.md 强制**：未完成独立验证不得标记 CONVERGED。
- **历史版本清理**：原 `review-agent.md`（8,847 字符完整版，未做 IDE 适配）已删除，STATE.md 格式定义已迁移至 `review-process-skill.md`。当前 IDE 适配层只保留 `review-agent-ide.md` + `review-agent-trigger.md`。
- **复测命令**（字符数）：`python3 -c "print(len(open('prompt/skill/review-agent-ide.md',encoding='utf-8').read()))"`（`wc -m` 在非 UTF-8 locale 下数字节，会高估含中文的文件，不可靠）

### `review-agent-trigger.md` 的限制说明

- 该文件对应 Trae Agent 配置中的 **"何时调用 / trigger description"** 字段，描述 Agent 在什么场景被自动调用。
- **硬上限 5,000 字符**（IDE 编辑窗口明确提示）。当前 4,001 字符，余量 999 字符。
- 含 12 个 `<example>` 块，覆盖全部 8 个域的触发场景，以及工作流评估、修复阶段、design 快照补齐三个新增场景：文档 review、代码 review、全模块 review、Ch1&2 部分 review、覆盖率检查、核心语义验证、快速扫描、Design-First review、design↔code 对齐、修复 review 发现、工作流评估、design 快照补齐。
- description 部分明列 8 个域的能力 + 8 个 Skill 路由 + 12 种触发意图，确保 agent 被正确触发。
- **不要盲目加 example**：每个 `<example>` 块约 200 字符，当前余量约够再加 5 个；新增前先确认是否覆盖了真正常见的新意图。

### 架构说明

```
review-agent-ide（智能体 / 路由器 + 核心规则）
    │
    ├── 加载 ▶ review-doc-skill（文档检查：§2.0 Claims + §2.1-§2.11 + §3）
    ├── 加载 ▶ review-code-skill（代码检查：§1-§15 + Kernel SMP/BKL §4.2）
    ├── 加载 ▶ review-patterns-skill（错误模式：文档15+跨文档3+代码14+测试6+卓越性7）
    ├── 加载 ▶ review-process-skill（执行流程：§〇三模式 + Step 0-7 + 状态追踪 + 独立验证）
    ├── 加载 ▶ review-core-semantics-skill（核心语义：行为契约表模板）
    ├── 加载 ▶ review-excellence-skill（卓越性：文档§4.1-4.5 + 代码§16-21）
    ├── 加载 ▶ review-coverage-skill（覆盖率：机器穷举 + AI 语义判断）
    └── 加载 ▶ review-socratic-skill（苏格拉底追问：13 场景追问话术，按需触发）
```

> **注**：`review-implementation-skill`（实施验证）**不在** agent-ide 的 8 个路由内——它用于 Fix Phase / 设计-代码一致性专项验证，按需由用户或 review 流程直接调用（`.trae/skills/` 与 `.codex/skills/` 均含此 skill）。

- **Agent**（review-agent-ide.md）：负责路由与决策。根据用户意图判断应加载哪些 Skill，控制输出格式，强制执行约束。Agent 之间不互相调用。
- **Skill**（review-*-skill.md）：各自负责独立的领域能力，互不引用。Agent 按需加载，Skill 本身不决策，仅提供规则和步骤。

### 使用方式（手工复制粘贴）

1. 在 Trae IDE 打开「智能体」配置面板（右上角 → 智能体 → 创建智能体）
2. 将 `review-agent-ide.md` 的内容**完整复制粘贴**至"提示词（Prompt）"输入框
   - ✅ **已达标**：9,751 字符 < 10,000 硬上限，可直接粘贴（余量 249，2026-09-05 实测）。
3. 将 `review-agent-trigger.md` 的内容**完整复制粘贴**至"何时调用"输入框
4. 启用所需 MCP 工具（建议启用：文件系统、终端、联网搜索）
5. 在「规则与技能」面板，将 9 个领域 `review-*-skill.md` 各自作为 Skill 导入（注意 Trae 的 Skill 有 `name`/`description` 字段约束，见上表）

### 三端同步说明（.trae / .claude / .codex，NEW 2026-08-14）

> **派生关系**：`prompt/skill/`（9 个领域 Skill 源）→ `.trae/skills/`（9 个 Skill + 2 个 Trae Agent 定义）+ `.codex/skills/`（9 个 Skill）；`review-scan` 编排器由 `.claude/skills/` 派生到 `.codex/skills/`，Claude 保留 `review-implementation-skill` 专项。三端内容同源，但运行时路径和 frontmatter 由适配层明确转换。**修改源文件后必须运行同步/校验工具**。

**同步规则**：

| 目标 | 文件名映射 | frontmatter 差异 | 来源 |
|------|-----------|-----------------|------|
| `.trae/skills/` | `review-{name}-skill.md` → `review-{name}-skill/SKILL.md` | `name`/`description` **不带引号**（Trae 标准格式）；**markdown 链接路径适配**（源 `prompt/skill/` 深度 2 → 派生深度 3，`../review-rules/` → `../../../prompt/review-rules/`、同目录 skill 链接 → `../{name}/SKILL.md`） | `prompt/skill/` |
| `.codex/skills/` | `review-{name}-skill.md` → `review-{name}-skill/SKILL.md` | `name` 不带引号 + `description` **双引号包裹**（Codex 硬限制 ≤1024 字符，超长触发启动校验错误）；路径/状态段 + **markdown 链接路径适配**（同 .trae） | `prompt/skill/` + `.claude/skills/review-scan/` |
| `.claude/skills/` | `review-scan/`（编排器 + 5 个 checks/） | 原样 | `.claude/` 独立维护（CLAUDE.md 声明派生自 prompt/，实际 review-scan 从 prompt/skill 演进） |

- **自动生成命令**（推荐，从 `prompt/skill/` 一键生成 `.trae/` + `.codex/`）：
  ```bash
  # 生成两套派生集（Trae: sed 去引号；Codex: sed 路径/命名/工具名适配）
  tools/generate-derived-skills.sh

  # 或仅生成某一端
  tools/generate-derived-skills.sh trae
  tools/generate-derived-skills.sh codex

  # 不写文件，仅检查漂移（CI/回归用）
  tools/generate-derived-skills.sh --check
  ```
- **校验命令**（生成后必跑）：
  ```bash
  tools/check-review-rules.sh
  ```
- **⚠️ review-rules ↔ skill 一致性**（AI 回归检查）：
  `prompt/skill/` 是 `prompt/review-rules/` 的 skill 适配层（加 frontmatter + 触发条件 + 输出模板）。两者内容应一致但**不会自动同步**。修改 `review-rules/` 后，需 AI 检查 `skill/` 是否需要同步更新。`tools/check-review-rules.sh` 的 H3 计数检查能捕获结构性漂移（如整段缺失），但无法检测段落内文字漂移。
- **验证命令**（复制后必跑）：
  ```bash
  for f in .codex/skills/*/SKILL.md; do
    dir=$(basename "$(dirname "$f")")
    name=$(awk '/^name:/{sub(/^name: *"?/,""); sub(/"?$/,""); print; exit}' "$f")
    desc=$(awk '/^description:/{sub(/^description: /,""); gsub(/^"|"$/,""); print; exit}' "$f")
    [ "$name" = "$dir" ] || echo "❌ name≠dirname: $f"
    # Codex description 限制 1024 字符；用 python3 数字符（locale 无关），${#desc} 在非 UTF-8 locale 下数字节会假阳性
    dlen=$(python3 -c 'import sys;print(len(sys.argv[1]))' "$desc")
    [ "$dlen" -le 1024 ] || echo "❌ desc>1024: $f ($dlen)"
    grep -q '^description: "' "$f" || echo "❌ desc 未加引号: $f"
  done
  ```

**当前已同步的 9 个 Skill**（prompt → `.trae/skills/` + `.codex/skills/`，2026-08-15 复验，`generate-derived-skills.sh` 自动生成）：
| Skill | prompt/skill/ 字符 | .trae/skills/ 字符 | .codex/skills/ 字符 | .trae diff | .codex diff\* |
|-------|-------------------|-------------------|--------------------|-----------|--------------|
| review-code-skill | 7,335 | 7,370 | 7,372 | +35 | +2 |
| review-doc-skill | 24,768 | 24,992 | 24,994 | +224 | +2 |
| review-patterns-skill | 39,338 | 39,425 | 39,427 | +87 | +2 |
| review-process-skill | 65,677 | 65,925 | 65,654 | +248 | -271 |
| review-core-semantics-skill | 8,464 | 8,486 | 8,488 | +22 | +2 |
| review-coverage-skill | 11,082 | 11,170 | 11,142 | +88 | -28 |
| review-excellence-skill | 9,220 | 9,320 | 9,322 | +100 | +2 |
| review-implementation-skill | 5,713 | 5,735 | 5,737 | +22 | +2 |
| review-socratic-skill | 5,696 | 5,705 | 5,707 | +9 | +2 |

\* `.codex` diff = codex 字符 − trae 字符；除 frontmatter 外还包含 Codex 的 `.review/codex`、无 agent、单 session 和引用路径适配（sed 规则化）。`.trae` diff = trae 字符 − prompt 字符，其中 frontmatter 去引号（-4）+ **markdown 链接路径适配**（同源链接在派生深度 3 下修正，差值随链接数变化）。共享规则内容仍由源文件约束，并由 `tools/check-review-rules.sh`（对源/派生统一归一化链接后再 diff）+ `tools/generate-derived-skills.sh --check` 校验。

> **说明**：Trae 对单 Skill 文件无硬字符上限，仅 Agent Prompt ≤ 10,000。字符数随累积改进持续增长——review-process-skill 自 2026-07-16 方案 D 后 36,087 → **65,581**（Step 0.5.3 doc↔outline 对齐 + Gate H.6 + 后续 Step 1.0a-g 等）；review-doc-skill **24,461**、review-patterns-skill **37,942**。**字符数以 python3 `len(open(f,encoding='utf-8').read())` 实测为准（Unicode 字符数）；`wc -m` 在非 UTF-8 locale 下数字节，会高估含中文的文件，不可靠。表格值需随同步更新。**
>
> 2026-06-22 新增 **review-implementation-skill**（由 06-design-final.md 实施过程沉淀），覆盖 design ↔ code 一致性 + §X self-review issues 追踪。详见 skill 文件 §Skill 输出模板 + §Gate D-Impl。

### 规则集与 Skill 的对应关系

| 原始规则 | 转化产物 | 角色 | 当前字符 |
|---------|---------|------|---------|
| review.md | review-agent-ide.md | Agent（精简原则 + 路由 + 强制约束；详细知识下沉到 Skill） | 9,751 ✅（余量 249，2026-09-05 实测） |
| review.md | review-agent-trigger.md | Agent（触发器描述 + 12 个示例，覆盖 8 域 + 工作流评估/修复/快照补齐阶段） | 4,001 ✅ |
| review-doc-checklist.md | review-doc-skill.md | Skill（§2.0 Claims-Evidence + §2.1-§2.11 + §3；强制逐行验证） | 24,461 |
| review-code-checklist.md | review-code-skill.md | Skill（§1-§15 + Kernel SMP/BKL §4.2） | 7,335 |
| review-patterns.md | review-patterns-skill.md | Skill（84 个错误模式；Gate D 严格通过标准） | 见同步表实测 |
| review-process.md | review-process-skill.md | Skill（§〇三模式 + Step 0-7 + 修复阶段 + STATE.md 三工具隔离 + Gate 证据 + Gate G/H 强制 + Gate 0 制品完整性 + L1/L2/L3 证据分级 + **方案 D outline 升格 + Step 0.5.3 doc↔outline 对齐 + Gate H.6 + Step 1.0a-g 等**） | 65,581 |
| review-core-semantics.md | review-core-semantics-skill.md | Skill（行为契约表模板 + 8 字段 × 5 函数） | 8,464 |
| review-doc-excellence.md + review-code-excellence.md | review-excellence-skill.md | Skill（文档§4.1-4.5 + 代码§16-21 卓越性） | 9,220 |
| review-process.md §Step 1.5 | review-coverage-skill.md | Skill（机器穷举 + AI 语义判断 + doc-specific 覆盖率 + semantic-map + Gate A 强制运行规则 + gate-evidence-A 块模板） | 11,082 |
| review.md（苏格拉底追问话术） | review-socratic-skill.md | Skill（13 场景追问话术模板） | 5,696 |
| review-process.md §实施验证（2026-06-22 新增） | review-implementation-skill.md | Skill（design ↔ code 一致性 + §X self-review 追踪 + 后向兼容重构 + 测试边界） | 5,713 |
| review-profiles.md | review-agent-ide.md（路由指令部分） | 并入 Agent | — |
| ~~review-agent.md~~ | ~~已删除~~ | 原 8,847 字符完整版，STATE.md 格式已迁移至 review-process-skill.md | — |

---

## Claude Code Runtime 适配（项目根 + .claude/）

Claude Code Runtime 的配置**自动加载**，与 Trae 完全不同：

- 加载位置：**项目根** + **`.claude/`** 目录
- 加载机制：Claude Code 启动时自动读取
- 不需要手工复制粘贴

### 项目根配置

- **`CLAUDE.md`**（项目根）— Claude Code 启动时自动注入的**项目级 system prompt**。包含构建/测试命令、目录布局、执行模型分用户态/内核、编码约束（`no_std`、错误码映射 Minix3 errno、硬件抽象为 trait）、Ground Truth 优先级、文档结构（Ch1→Ch2→Ch3→Ch4+测试章节），以及 Review 系统的强制约束（Explicit Skill Invocation、Blocker Gates 证据、STATE.md 双路径、VERIFY-CHECK 强制）。
- **`AGENTS.md`**（项目根，NEW 2026-08-14）— **Codex CLI 的项目指令入口**（Codex 不读 CLAUDE.md）。内容为 CLAUDE.md 的 Codex 适配子集 + review 工作流入口（指向 `.claude/rules/` 规范源）+ 10 个 skill 清单（带 file 路径）。详见下方「Codex CLI 适配」章节。

### `.claude/` 目录结构

```
.claude/
├── settings.local.json          — 本地权限配置（allow/deny 工具白名单）
├── rules/                       — 始终加载（always-on），占用 context 持续存在
│   ├── review-core.md           —   执行模型 + ⛔ 禁止行为 + Ground Truth + 优先级 + 强制 Skill 调用 + 严格 Gate D
│   ├── review-process.md        —   Step 0-7 强制流程 + STATE 双路径 + Gate 证据 + VERIFY-CHECK 强制
│   └── fix-guard.md             —   安全修复规则
└── skills/                      — 按需加载（on-demand），不占用 context
    ├── review-scan/             —   主 Skill（编排器，Phase 1-9）
    │   ├── SKILL.md             —     Orchestrator（YAML frontmatter + Phase 1-9 + Explicit Skill Invocation）
    │   └── checks/              —     检查维度（5 个领域文件）
    │       ├── doc.md           —       文档检查（含禁止"未逐行验证"）
    │       ├── code.md          —       代码检查
    │       ├── patterns.md      —       错误模式（含 Gate D 严格通过标准）
    │       ├── process.md       —       执行流程（含 STATE 双路径、VERIFY-CHECK 强制）
    │       └── excellence.md    —       卓越性检查
    └── review-implementation-skill/ — 实施验证 skill（design ↔ code 一致性 + §X self-review 追踪，2026-06-22 加入）
```

> 检查维度合并为 5 个领域文件，避免 attention decay 和过度拆解。
> **注意**：`.claude/` 中的规则已与本轮修复同步，包含与 Trae Skill 相同的强制约束（Explicit Skill Invocation、双路径 STATE、Gate 证据、VERIFY-CHECK 强制、严格 Gate D、禁止"未逐行验证"、doc-specific 覆盖率）。

### 与 Trae 的关键差异

| 维度 | Trae IDE | Claude Code Runtime |
|------|---------|-------------------|
| 加载方式 | **手工复制粘贴** | **自动加载**（启动时读 CLAUDE.md / `.claude/`） |
| 配置位置 | Trae IDE 内（不存项目） | **项目根 + `.claude/`**（随仓库提交） |
| Rules 限制 | ≤ 10,000 字符（建议）/ 20,000 byte（硬上限） | 无明确字符上限（受模型 context window 约束） |
| Agent Prompt 限制 | **≤ 10,000 字符硬上限** | 无明确上限（受 context 约束） |
| Skill 加载 | 手动导入 IDE | **SKILL.md 放在 `.claude/skills/` 即自动可用** |
| 多个 rules | 单文件 / 项目多文件 | `.claude/rules/*.md` 多文件分别 always-on |
| 典型运行模型 | MiniMax-M3 / GPT-5 等（强模型） | Claude 系列 / MiniMax-M3 等 |
| 角色定位 | **深度 + 交互式编辑**（强模型 + 追问） | **广度 + 后台扫描**（批量 grep / 覆盖枚举） |

### `.claude/rules/*.md` 的 always-on 加载

- **`review-core.md`** — 始终加载。定义执行模型分用户态/内核、⛔ 禁止行为（从记忆回答/跳过检查/猜测/无输出标记 ✅/后期 check decay）、Ground Truth、**Rewrite / Refactor (code + design) / Architectural Evolution** 三层术语、**P0 六分类（含 P0-design-deviation/missing/wrong/test-missing）**、**Design First 原则 + 优先级链 Minix3 > design > code > doc**、P0/P1/P2 优先级、**强制 Skill 显式调用**、**Gate D 严格通过标准**。
- **`review-process.md`** — 始终加载。定义 Step 0-7 流程（Scope 声明 → C 源验证 → Diff 抽取 → Sanity Check → Precision Check → 状态写入 → VERIFY 独立验证），以及 **STATE.md 双路径**、**Gate 证据规则**、**VERIFY-CHECK.md 强制**。
- **`fix-guard.md`** — 始终加载。定义安全修复的 4 条强制要求（读 ±5 行上下文 / grep 确认当前状态 / 一次性应用一个 fix / 写 fix-status）。

### `.claude/skills/review-scan/` 的 on-demand 加载

- **`SKILL.md`** — Orchestrator。YAML frontmatter 定义 `name`、`description`、`allowed-tools`。Phase 1-9 控制执行顺序：**Scope → Coverage Enumeration → Gap Scan → Doc Checks → Code Checks → Patterns → Excellence → Cross-doc → Report**。含 evidence 分级、"先读后判"强制规则、**Explicit Skill Invocation**、**STATE.md 双路径**、**scan.md 双写规则**。
- **`checks/doc.md`** — 文档检查。覆盖 §2.0 Claims-Evidence、概念准确性、C 代码引用（**禁止"未逐行验证"**）、数据结构、doc-code 一致性、架构演进、跨引用、图示、C 源码覆盖、设计质量、链接验证、文档风格、skip 检查。
- **`checks/code.md`** — 代码检查。覆盖 rewrite 质量、硬件抽象、trait 设计、类型安全、执行模型（含 SMP/BKL §4.2）、内存模型、模块设计、命名、测试、注释、64-bit、复杂度、no_std、设计-代码一致性、C-Rust 对齐、精度检查。
  - **`checks/patterns.md`** — 错误模式库。源规则共 84 个枚举模式，Claude 版按领域合并检查。含 **Gate D 严格通过标准**。
- **`checks/process.md`** — 执行流程。含 §〇 三模式选择（构造/快速/深度）、**STATE.md 双路径**、**Gate 证据规则**、**VERIFY-CHECK.md 强制**、**P0/P1/P2 同步规则**。
- **`checks/excellence.md`** — 卓越性检查。文档 §4.1-4.5 + 代码 §16-21。

### Claude 配置的设计意图

- **`CLAUDE.md` 是入口**：Claude Code 启动时自动注入项目背景（"这是 Minix-RS，是 Minix3 的 Rust Rewrite，不是翻译，是改写，no_std，硬件抽象为 trait"）。
- **`.claude/rules/` 是底线**：执行模型、禁止行为、流程——每次都加载，因为它们定义"必须遵守的约束"。
- **`.claude/skills/` 是按需加载**：避免一次注入所有检查导致 context 占用过高；Claude 根据用户意图决定加载哪个 check。

---

## Codex CLI 适配（项目根 AGENTS.md + .codex/，NEW 2026-08-14）

Codex CLI 的配置**自动加载**（读取项目根 `AGENTS.md`，不读 CLAUDE.md），与 Claude 类似但存在关键机制差异。

### 加载机制

- **项目指令**：项目根 `AGENTS.md` — Codex 启动时自动注入。内容为 CLAUDE.md 的 Codex 适配子集：项目简介、Build & Test 命令、目录布局、关键约束（no_std / errno 映射 / 硬件抽象 / SMP+BKL / Ground Truth 链）、**review 工作流入口（明确要求先读 `CLAUDE.md` + `.claude/rules/review-core.md` + `review-process.md` + `fix-guard.md` —— 规范源仍在 `.claude/rules/`，Codex 用 AGENTS.md 补充工具适配）**、10 个 skill 清单（带 file 路径 + when-to-use，确保自动选择）。
- **Skills**：`.codex/skills/{name}/SKILL.md` — Codex 自动发现。每个 skill 必须在自己命名的子文件夹，frontmatter `name` 必须等于目录名。

### Codex 字段限制（关键，来自 Codex 官方文档与社区实测）

| 项 | 限制 | 后果 | 本项目的适配 |
|----|------|------|-------------|
| Skill `name` | ≤ 64 字符，与父目录同名 | 不匹配则不识别 | ✅ 10/10 name = 目录名 |
| Skill `description` | **≤ 1024 字符**（硬限制，[Codex Skills 规范](https://developers.openai.com/codex/skills/)） | 超长触发启动时校验错误（非静默跳过） | ✅ 10/10 ≤ 197 字符（review-scan 最长），远低于上限；仍建议精简以节省启动期 context |
| description YAML 特殊字符 | 需双引号包裹 | 未加引号可能静默跳过 | ✅ 10/10 双引号包裹 |
| Agent / subagent | **无**（Codex 无 agent 概念） | — | `review-agent-ide` / `review-agent-trigger` **不复制**（agent 定义无 frontmatter） |
| Rules 机制 | **无**（无 .claude/rules/ 等价物） | — | AGENTS.md 指引 Codex 读取 `.claude/rules/` 规范源 |
| `.review/` 状态隔离 | 使用独立 `.review/codex/` | 与 Trae/Claude 不共享 STATE/scan/SYMBOLS/structure/VERIFY-CHECK | Codex 单 session 无 bagging；仍必须产出标准制品和 VERIFY-CHECK |

### `.codex/skills/` 构成（10 个）

| Skill | 来源 | 说明 |
|-------|------|------|
| review-scan | `.claude/skills/` 复制后适配（含 5 个 checks/） | 编排器（Phase 1-9），路径指向 Codex 制品和 `.codex/` Skill |
| review-*-skill × 9 | `prompt/skill/` 派生 | 正文同源；Codex 仅精简 description，并适配状态/覆盖率路径 |

**同步**：修改 `prompt/skill/*.md` 或 `.claude/skills/review-scan/` 后按上方适配命令更新派生文件，再运行 `tools/check-review-rules.sh`。**已落地验证**：2026-08-14 首次配置，10/10 frontmatter 合规（name=目录名 / desc ≤1024 / 双引号）。

---

## 任务命令（review-cmds，NEW 2026-09-05）

6 个独立 cmd 是任务的一级入口（单一目标 + 明确边界 + 章节级默认范围），定义于 [review-rules/review-cmds.md](review-rules/review-cmds.md)：
`full-review` / `style-fix` / `code-excellence` / `test-audit` / `todo-fix` / `style-bible`（文风宪法，muse/opencode 必加载）。

- **薄壳注册**：`prompt/skill/cmds/{name}/SKILL.md`（源）→ `.agents/skills/{name}`（**软链**，ZCode 原生扫描 `.agents/skills/`）→ opencode.json 亦已注册（muse 可直呼）。
- **为何薄壳**：规则细节全部在 review-cmds.md（单一真相源），SKILL.md 只承载触发描述 + 目标 + 强制门摘要，避免再造平行真相源。
- **为何 .trae/.codex/.claude 不注册 cmd**：.trae 需手工导入且 Windows git checkout 会把软链退化为文本文件（真实文件 + 派生脚本仍是正解）；这三端继续用 Skill 清单 + AGENTS.md 路由方式使用 cmd（读 review-cmds.md 照做）。

## 状态管理与收敛

每次 Review 在**项目根 `.review/`** 下创建工具特定的输出目录，统一布局，`trae/`、`claude/` 与 `codex/` 三者互不干扰。三套工具**绝不共享任何中间结果**（STATE/scan/SYMBOLS/structure/VERIFY-CHECK）；Bagging 聚合只发生在 Trae 内（多 AI 的 scan 聚合）。Codex 使用单 session 标准制品布局，不使用 Trae 的 bagging 状态。

### 路径变量

- `{module}` = rewrite 模块名 = `notes/rewrite/{module}/` 的目录名（如 `fork-syscall-rewrite`）。取目标文档所在路径中 `notes/rewrite/` 下的第一级目录名。
- `{stage}` = 模块下的阶段子目录（如 `01-stage-kernel`），仅作 `{module}` 内分组，不替代 `{module}`。
- `{doc-stem}` = 目标文档去扩展名（如 `03-kmain-cstart`）。
- `{agent}` = AI 模型标识（Trae 内：glm/kimi/ds/qwen/seed/...；Claude 内：m3/glm-flash）；Codex 单 session 不使用 agent 后缀。
- **`{module}` 与覆盖率脚本 `--module` 是两个不同概念**：本路径的 `{module}` 是 rewrite 模块名；覆盖率脚本的 `--module kernel` 是 Minix3 模块名。不得混用。

### 统一布局（项目根 `.review/` 下分 `trae/`、`claude/` 与 `codex/`）

```
.review/
├── trae/{module}/
│   ├── STATE.md                              # Trae 专属，与 claude 隔离
│   ├── VERIFY-CHECK.md                       # Trae 专属
│   ├── session-plan.md                       # 多 session 续审计划
│   └── scans/
│       ├── {doc-stem}-{agent}-scan.md        # 某 AI 的 scan（bagging 输入）
│       ├── {doc-stem}-{agent}-structure.md
│       ├── {doc-stem}-{agent}-SYMBOLS.md
│       ├── MANIFEST-{doc-stem}.md            # 该 doc 所有 agent scan 清单
│       └── AGGREGATED-{doc-stem}.md          # bagging 聚合后主 scan
├── claude/{module}/
    ├── STATE.md                              # Claude 专属，与 trae 隔离
    ├── VERIFY-CHECK.md                       # Claude 专属
    └── {doc-stem}/{scan,structure,SYMBOLS}.md
└── codex/{module}/
    ├── STATE.md                              # Codex 专属，与 trae/claude 隔离
    ├── VERIFY-CHECK.md                       # Codex 专属
    └── {doc-stem}/{scan,structure,SYMBOLS}.md
```

> 采用**扁平 scans/ 结构**，不启用 `{stage}/` 子目录（`{doc-stem}` 已含 stage 编号前缀，足够区分）。

### 双写模式（交互式修复场景）

需交互式修复 review todo 时，除标准中间产物外，**额外**在被 review 文档同目录下写可读修复文档：

- Trae 交互式修复文档：`notes/rewrite/{module}/{stage}/{doc-stem}-trae-review.md`
- Claude 最终报告（可选）：`notes/rewrite/{module}/{stage}/{doc-stem}-claude-report.md`
- Codex 交互式修复报告：`notes/rewrite/{module}/{stage}/{doc-stem}-codex-report.md`

修复文档/最终报告 = 标准 scan 的**可读子集 + todo 进度跟踪**（按 issue ID/位置匹配的子集关系，非 checksum 一致）。scan.md 的 Artifact Inventory 表列出标准路径和可选双写路径。

### 三工具隔离规则

1. Step 0 先判断当前运行环境（Trae / Claude / Codex），读取对应 `.review/{tool}/{module}/STATE.md`。
2. 如果同一工具下两份 STATE.md 同时存在且内容矛盾，**不要自动合并**，在 scan.md 中记录分歧并询问用户哪个为准。
3. 每个 STATE.md 独立维护自己的 Open P0/P1/P2 列表；新增问题必须同步进 Open 列表，已修复问题移入 Closed Issues。
4. 收敛条件必须满足：scan.md 所有维度 COMPLETE、最近 Pass 新增 P0=0/P1≤1、Gate G VERIFY-CHECK = PASS、Blocker Gates 0/A/B/C/D/D-6/E/G/H 全部通过且有 gate-evidence 附件。

### Step 0 预检硬阻断（NEW 2026-07-16）

> **背景**：Session #12 复盘发现 — 即使 review-process.md §Step 0 已写"design 预检强制"，AI 仍会因"已有 CONVERGED 状态"/"incremental review"等理由错误跳过预检。Session #11 模式 69 (PSMD) 发现 04/05 缺快照时已记录此为 P0-process-violation，但缺少硬阻断机制。

**强制规则**（所有 review 模式强制）：

1. **必须跑 4 条 `ls`**（Step 0 启动时）：
   ```bash
   ls notes/rewrite/{module}/{stage}/.design/{NN}-outline.v*.md
   ls notes/rewrite/{module}/{stage}/.design/{NN}-outline-review.v*.md
   ls notes/rewrite/{module}/{stage}/.design/{NN}-design.v*.md
   ls notes/rewrite/{module}/{stage}/.design/{NN}-design-final.v*.md  # bagging only
   ```
   ls 输出必须写入 scan.md `§Step 0: 预检结果` 段（Gate 0 锚段，9 个之一）。

2. **缺失判定 + 嵌入生成（2026-07-17 更新）**：
   - `outline.v*.md` 缺失 → **Gate H.6 FAIL** → **Step 0.3.2 嵌入生成**（不中断 review）
   - `outline-review.v*.md` 缺失 → **Gate H.6 FAIL** → **Step 0.3.3 嵌入生成**（AI 自审，不需用户确认）
   - `design.v*.md` 缺失 → **Gate H.1 FAIL** → **Step 0.3.4 嵌入生成**（不中断 review）
   - **核心变更**：原"缺失 → 阻断 Step 1 + 触发附录 C/Design-First（中断）"改为"缺失 → Step 0.3 嵌入生成 → 继续 review"
   - 不允许以"已有 CONVERGED 状态"/"incremental review"/"复用 03/04 design"为由跳过（**模式 69 PSMD 触发**）

3. **存在旧快照时**：仍必须执行 v2 评估（重新产出 `.v{N+1}.md`）；旧快照仅作"前人理解"参考，**不是 ground truth**。

4. **工具支持**：`tools/design-coverage-check.sh {module}` 自动扫描所有 stage 的 `.design/` 目录，输出缺失报告。Session 启动时跑此工具 → 报告写入 STATE.md `§启动预检` 段。

5. **决策记录豁免**：仅一次性用户明确豁免；豁免必须登记在 STATE.md `§豁免列表` 段；**不可泛化**（**模式 71 DOG 触发**）。

6. **STATE.md Resume Point 模板**：续 session 必含 `ls .design/` 预检命令（避免续 session 跳过）。

**当前快照覆盖情况**（`tools/design-coverage-check.sh fork-syscall-rewrite --stage 01-stage-kernel` 输出（历史快照数字，以工具当前输出为准））：
- Total docs: 29
- Complete (design + outline): **3**（仅 01/02/03）
- Missing outline (H.6 FAIL): 24
- Missing design (H.1 FAIL): **24**

**含义**：fork-syscall-rewrite 模块 01-stage-kernel 阶段 **22/29 文档缺 design 快照**（01-07 已审过有快照，08-25/99/00 按需生成）。按新规则，缺失时 review 自动执行 **Step 0.3 嵌入生成**（2026-07-17 变更：原"必须先附录 C 追溯生成"改为"Step 0.3 嵌入 review 流程内生成"）。

> **精简说明**：所有维度检查结果（概念/引用/结构/覆盖/设计/链路/代码/跨文档/Claims）全部并入 `scan.md` 对应章节，不再拆 10 个维度文件。

**收敛终止条件**（全部满足才算审查完成）：
1. scan.md 中所有维度章节标记 COMPLETE
2. P0 新增数量 = 0（最近一次完整 Pass）
3. P1 新增数量 ≤ 1
4. **Gate G** 独立验证（VERIFY-CHECK.md）结果为 PASS
5. scan.md 中所有 P0 已被修复并验证通过
6. SYMBOLS.md 覆盖率穷举完成（所有 C 符号有"已覆盖/ARCH 不需要/缺口"判定）
7. **Blocker Gates 0/A/B/C/D/D-6/E/G/H 全部通过且有 gate-evidence 附件**（见 review-process-skill.md）
8. STATE.md 三工具路径无未解决的冲突（若同一工具有两份 STATE，需用户确认权威版本）

## 覆盖率穷举工具

> 解决"覆盖率不足"和"跨轮次累积"问题的基础设施。

```bash
# 模块级（服务器模块：vm / pm / vfs / rs / ds / inet ...）
#   注：脚本第一参数 {minix3-module} 是 Minix3 模块名；--output 路径里的 {rw-module} 是 rewrite 模块名
python3 tools/coverage-extract/coverage-extract.py {minix3-module} {doc_dir} \
  --rust-dir os --c-dir minix3/minix/servers/{minix3-module} \
  --output .review/{tool}/{rw-module}/scans/SYMBOLS.md

# 模块级（内核：kernel）
python3 tools/coverage-extract/coverage-extract.py kernel {doc_dir} \
  --rust-dir os --c-dir minix3/minix/kernel \
  --output .review/{tool}/{rw-module}/scans/SYMBOLS.md

# 单文档级 — 服务器模块（推荐用于 doc-specific review，避免两篇 doc 覆盖率数字相同）
python3 tools/coverage-extract/coverage-extract.py {minix3-module} {doc_dir} \
  --rust-dir os --c-dir minix3/minix/servers/{minix3-module} \
  --doc-file {target-doc}.md \
  --semantic-map tools/coverage-extract/{minix3-module}-semantic-map.json \
  --output .review/{tool}/{rw-module}/scans/{doc-stem}-{agent}-SYMBOLS.md

# 单文档级 — 内核
python3 tools/coverage-extract/coverage-extract.py kernel {doc_dir} \
  --rust-dir os --c-dir minix3/minix/kernel \
  --doc-file {target-doc}.md \
  --semantic-map tools/coverage-extract/kernel-semantic-map.json \
  --output .review/{tool}/{rw-module}/scans/{doc-stem}-{agent}-SYMBOLS.md
#   {tool}=trae 时 agent=glm/kimi/...；{tool}=claude 时输出到 .review/claude/{rw-module}/{doc-stem}/SYMBOLS.md
```

> **注意**：`--c-dir` 对服务器模块是 `minix3/minix/servers/{minix3-module}`，对内核是 `minix3/minix/kernel`。命令模板中的 `{minix3-module}`（Minix3 模块名）与 `--output` 路径里的 `{rw-module}`（rewrite 模块名）是**两个不同概念**，不得混用。

- **机器部分**：提取 C 源码所有函数/结构体/宏，生成 SYMBOLS.md 骨架表格
- **AI 部分**：补充 5 项语义判断（Rust 对应、架构演进、语义归属、行为契约、测试覆盖）
- **输出**：`.review/{tool}/{rw-module}/scans/{doc-stem}-{agent}-SYMBOLS.md`（Trae 单文档）或 `.review/claude/{rw-module}/{doc-stem}/SYMBOLS.md`（Claude 单文档）
- **Rust 覆盖率 0% 必须解释**：检查 `--rust-dir` / `--semantic-map` 是否正确，或确实缺失实现

详见 [review-process.md §Step 1.5](review-rules/review-process.md) 和 [review-coverage-skill.md](skill/review-coverage-skill.md)。

## 执行模式分层

> Review 不再只有"全量"一种模式。根据任务规模选择构造/快速/深度三模式。

| 模式 | 适用场景 | 执行 Step | 预计耗时 | 对应 Profile |
|------|---------|----------|---------|-------------|
| **构造（Constructive）** | 初稿阶段，引导补全 | 0, 1, 1.5, 2, 5, 6 | 15~30 分钟 | A / B / G |
| **快速（Quick）** | 日常 PR、时间有限 | 0, 1, 2, 5 | 10~20 分钟 | D |
| **深度（Deep）** | 里程碑验收、关键模块 | 0-7（全量） | 40~120 分钟 | C / H→I→J→K / O / P |

详见 [review-process.md §〇 执行模式选择](review-rules/review-process.md)。

## 执行模型分层

> Review 不再假设"所有模块单线程"。内核有 SMP + BKL，用户态服务器仍是单线程事件循环。

| 模块类型 | 执行模型 | 并发保护 | Rc/RefCell | UnsafeCell 安全论据 |
|---------|---------|---------|-----------|-------------------|
| 用户态服务器（VM/PM/VFS/RS/DS/INET） | 单线程事件循环 | IPC 隔离 | ✅ 合理 | 单线程前提 |
| 内核（Kernel） | SMP + BKL spinlock | BKL_LOCK()/UNLOCK() | ❌ 多核共享需 Arc | BKL 保护 / per-CPU / Atomic |

详见 [review.md §执行模型](review-rules/review.md) 和 [review-agent-ide.md §Execution Models](skill/review-agent-ide.md)。
