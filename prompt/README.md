# prompt/

本目录包含 Minix-RS 项目的 Review 规则集及两套 IDE/Runtime 适配产物：**Trae IDE**（手工复制粘贴）与 **Claude Code Runtime**（项目根 + `.claude/` 自动加载）。

## 目录结构

```
prompt/
├── README.md                — 本文件
├── review-rules/            — 原始 Review 规则集（唯一真相源）
│   ├── review.md            —   Review 核心框架（原则、约束、优先级、输出模板、执行模型分层）
│   ├── review-doc-checklist.md  —  文档检查清单（§1~§3，含 §2.0 Claims-Evidence）
│   ├── review-code-checklist.md —  代码检查清单（§1~§15，含 Kernel SMP/BKL 并发）
│   ├── review-patterns.md       —  常见错误模式（文档15+跨文档3+代码14+测试6+卓越性7=45个）
│   ├── review-process.md        —  执行流程（§〇三模式 + Step 0~7 + 状态追踪 + 收敛判断）
│   ├── review-profiles.md       —  任务组合配置（Profile A~P，含分阶段 H~K + 卓越性 O + 覆盖率 P）
│   ├── review-core-semantics.md —  核心语义对齐（行为契约表 + IPC/生命周期契约模板）
│   ├── review-doc-excellence.md —  文档卓越性（§4.1叙事结构 + §4.2读者体验 + §4.3教学深度 + §4.4可维护性）
│   └── review-code-excellence.md—  代码卓越性（§15 API设计 + §16表达力 + §17性能 + §18代码即文档 + §19可测试性 + §20测试质量）
├── skill/                   — Trae IDE 适配层（手工复制粘贴至 IDE；通过下方同步命令同步至 .trae/skills/）
│   ├── review-agent-ide.md      —  智能体精简版（✅ 8,903 字符达标，见下方说明）
│   ├── review-agent-trigger.md  —  触发器描述（何时调用 Agent，16 个示例覆盖 8 域 + 工作流评估/修复阶段）
│   ├── review-doc-skill.md      —  文档 Review 技能（含 §2.0 Claims-Evidence）
│   ├── review-code-skill.md     —  代码 Review 技能（含 §4.2 Kernel SMP/BKL 并发）
│   ├── review-patterns-skill.md —  错误模式对照技能（含测试6+卓越性7模式）
│   ├── review-process-skill.md  —  执行流程技能（含 §〇三模式 + STATE.md 格式 + 状态追踪 + 独立验证）
│   ├── review-core-semantics-skill.md — 核心语义技能（行为契约表模板）
│   ├── review-excellence-skill.md    — 卓越性技能（文档+代码卓越性检查流程）
│   ├── review-coverage-skill.md      — 覆盖率技能（机器穷举 + AI 语义判断）
│   ├── review-implementation-skill.md — 实施验证技能（design ↔ code 一致性 + §X self-review issues 追踪）
│   └── review-socratic-skill.md      — 苏格拉底追问技能（8 场景追问话术模板）
└── README.md                — 本文件
```

`.review/` 目录为 Review 执行时自动生成的**状态持久化输出**，不在本目录中（见下方「状态管理与收敛」）。

---

## review-rules/（唯一真相源）

Review 规则集是项目在多轮迭代中积累的规则文档，定义了针对 Minix-RS 项目的所有 Review 要求。其逻辑顺序为：

1. **review.md** — 核心框架，定义 Rewrite/Translate/Redesign 三态（原则层）。含执行模型按模块分层（用户态服务器单线程 vs 内核 SMP+BKL）、核心语义验证（Ground Truth 具体化，引用 review-core-semantics.md）。
2. **review-profiles.md** — 任务组合配置，决定不同场景加载哪些规则模块（策略层）。含 Profile O（卓越性专项）、Profile P（覆盖率专项）。
3. **review-process.md** — 强制执行步骤，要求每个步骤必须产生可见中间产物（流程层）。含状态写入与收敛判断、Review Verification Protocol、§〇 执行模式选择（构造/快速/深度三模式）、Step 1.5 覆盖率穷举。
4. **review-doc-checklist.md** — 文档维度的检查清单（文档维度层）。含 §2.0 Claims-Evidence Tracing（论文级文档质量方法论）。
5. **review-code-checklist.md** — 代码维度的检查清单（代码维度层）。含 §4.2 内核 SMP/BKL 并发检查项。
6. **review-patterns.md** — 常见错误模式汇总（错误模式层）。含 Kernel SMP 并发违规模式、测试错误模式、卓越性错误模式，共 45 个。
7. **review-core-semantics.md** — 核心语义对齐。定义核心语义不变性原则，提供函数/IPC/生命周期行为契约表模板。
8. **review-doc-excellence.md** — 文档卓越性。§4.1 叙事结构、§4.2 读者体验、§4.3 教学深度、§4.4 可维护性。
9. **review-code-excellence.md** — 代码卓越性。§15 API 设计、§16 表达力、§17 性能、§18 代码即文档、§19 可测试性、§20 测试质量。

---

## skill/（Trae IDE 适配层）

`skill/` 目录下的文件是 Trae IDE 的 Agent 与 Skill 定义，由 `review-rules/` 规则集转化而来。**手工复制粘贴**至 Trae IDE 的对应配置入口（不是项目级自动加载）。

### Trae IDE 字符/字段限制（关键）

> 以下限制来自 Trae 官方文档与社区实测，**直接决定 skill 文件能否保存与生效**。新加内容前先核对字数。

| 项 | 限制 | 来源 | 当前文件 | 状态 |
|---|------|------|---------|------|
| **Agent Prompt（提示词）** | **硬上限 10,000 字符**（自动截断） | [Trae 官方 FAQ](https://forum.trae.cn/t/topic/7571) | `review-agent-ide.md` | **8,903 字符（≈ 89.0%）✅ 达标，约 1,100 字符余量** |
| Rule（规则） | 硬上限 20,000 byte；建议 ≤ 10,000 字符；token 视角约 3,000 token | [Trae 官方 FAQ](https://forum.trae.cn/t/topic/52) | n/a（本目录无 Rule 文件） | — |
| **Skill `name`** | ≤ **64 字符**，仅小写字母/数字/连字符（`-`），与父目录同名 | [Trae Skill 规范](https://docs.trae.ai/ide/best-practice-for-how-to-write-a-good-skill) | n/a（Trae Skill 命名规范） | — |
| **Skill `description`** | ≤ **1024 字符**（硬限制），建议 ≤ 200 字符 | 同上 | n/a | — |
| Agent `description`（触发器描述） | **硬上限 5,000 字符**（IDE 编辑窗口提示） | Trae IDE 实测 | `review-agent-trigger.md` | **4,913 字符 ✅**（含 16 个触发示例，覆盖全部 8 域 + 工作流评估/修复阶段） |
| MCP 工具总数 | ≤ **40 个** | [TRAE MCP 指南](https://blog.csdn.net/2601_96144997) | n/a | — |
| MCP 描述总字符数 | ≤ **8,000 字符**（超出丢弃） | 同上 | n/a | — |
| 上下文窗口（最强模型） | 240,000 tokens 输入 / 32,000 tokens 输出 | [Trae 模型文档](https://docs.trae.ai/ide/models) | 全局 | — |

### `review-agent-ide.md` 的 10,000 字限制说明

- **当前 8,903 字符，已达标**（硬上限 10,000 字符的 89.0%），保留约 1,100 字符余量以应对后续新增强制约束。本轮 P0 修复（improve-v2 §10）后增加约 2,168 字符，主因是新增 Gate 0、Gate G、双路径状态管理、VERIFY-SELF/VERIFY-CROSS、8 字段 × 5 函数行为契约表等强制约束。**新增任何约束前先核对余量；超 10,000 字符必须触发规则精简**。
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
- **复测命令**（bash）：`wc -m prompt/skill/review-agent-ide.md`

### `review-agent-trigger.md` 的限制说明

- 该文件对应 Trae Agent 配置中的 **"何时调用 / trigger description"** 字段，描述 Agent 在什么场景被自动调用。
- **硬上限 5,000 字符**（IDE 编辑窗口明确提示）。当前 4,913 字符，余量 87 字符。
- 含 16 个 `<example>` 块，覆盖全部 8 个域的触发场景，以及工作流评估、修复阶段两个新增场景：文档/代码 review、全模块 review、Ch1&2 部分 review、链接验证、跨文档检查、覆盖率检查、核心语义验证、卓越性专项、快速扫描、分阶段 review、苏格拉底追问、验证上次 review、中文口语化触发、工作流评估、修复 review 发现。
- description 部分明列 8 个域的能力 + 8 个 Skill 路由 + 15 种触发意图，确保 agent 被正确触发。
- **不要盲目加 example**：每个 `<example>` 块约 200 字符，当前余量仅够再加 3 个；新增前先确认是否覆盖了真正常见的新意图。

### 架构说明

```
review-agent-ide（智能体 / 路由器 + 核心规则）
    │
    ├── 加载 ▶ review-doc-skill（文档检查：§2.0 Claims + §2.1-§2.11 + §3）
    ├── 加载 ▶ review-code-skill（代码检查：§1-§15 + Kernel SMP/BKL §4.2）
    ├── 加载 ▶ review-patterns-skill（错误模式：文档15+跨文档3+代码14+测试6+卓越性7）
    ├── 加载 ▶ review-process-skill（执行流程：§〇三模式 + Step 0-7 + 状态追踪 + 独立验证）
    ├── 加载 ▶ review-core-semantics-skill（核心语义：行为契约表模板）
    ├── 加载 ▶ review-excellence-skill（卓越性：文档§4.1-4.4 + 代码§15-20）
    ├── 加载 ▶ review-coverage-skill（覆盖率：机器穷举 + AI 语义判断）
    ├── 加载 ▶ review-implementation-skill（实施验证：design ↔ code 一致性 + §X self-review 追踪，按需触发）
    └── 加载 ▶ review-socratic-skill（苏格拉底追问：8 场景追问话术，按需触发）
```

- **Agent**（review-agent-ide.md）：负责路由与决策。根据用户意图判断应加载哪些 Skill，控制输出格式，强制执行约束。Agent 之间不互相调用。
- **Skill**（review-*-skill.md）：各自负责独立的领域能力，互不引用。Agent 按需加载，Skill 本身不决策，仅提供规则和步骤。

### 使用方式（手工复制粘贴）

1. 在 Trae IDE 打开「智能体」配置面板（右上角 → 智能体 → 创建智能体）
2. 将 `review-agent-ide.md` 的内容**完整复制粘贴**至"提示词（Prompt）"输入框
   - ✅ **已达标**：8,903 字符 < 10,000 硬上限，可直接粘贴。
3. 将 `review-agent-trigger.md` 的内容**完整复制粘贴**至"何时调用"输入框
4. 启用所需 MCP 工具（建议启用：文件系统、终端、联网搜索）
5. 在「规则与技能」面板，将 8 个 `review-*-skill.md` 各自作为 Skill 导入（注意 Trae 的 Skill 有 `name`/`description` 字段约束，见上表）

### `.trae/skills/` 同步说明

项目根目录的 `.trae/skills/` 是 Trae IDE 识别 Skill 的标准位置。修改 `prompt/skill/*.md` 后，必须手动执行下方同步命令，将更新后的内容写入 `.trae/skills/{skill-name}/SKILL.md`。

**同步规则**：
- 文件名：`prompt/skill/review-{name}-skill.md` → `.trae/skills/review-{name}-skill/SKILL.md`
- frontmatter 格式差异：`prompt/skill/` 版本的 `name`/`description` 带双引号（便于 Markdown 渲染），`.trae/skills/` 版本不带引号（Trae 标准格式）
- 内容完全一致（仅 frontmatter 引号差异，字符数差 4）
- 同步命令（bash）：
  ```bash
  for s in review-code-skill review-doc-skill review-patterns-skill review-process-skill \
           review-core-semantics-skill review-coverage-skill review-excellence-skill \
           review-implementation-skill review-socratic-skill; do
    mkdir -p ".trae/skills/$s"
    sed -E '1,/^---$/ { s/^name: "([^"]+)"/name: \1/; s/^description: "([^"]+)"/description: \1/ }' \
      "prompt/skill/$s.md" > ".trae/skills/$s/SKILL.md"
  done
  ```

**当前已同步的 9 个 Skill**（2026-06-22 新增 review-implementation-skill）：
| Skill | prompt/skill/ 字符 | .trae/skills/ 字符 | diff | Trae 限制 |
|-------|-------------------|-------------------|------|----------|
| review-code-skill | 6,512 | 6,508 | 4 | — |
| review-doc-skill | 13,247 | 13,243 | 4 | — |
| review-patterns-skill | 16,569 | 16,565 | 4 | — |
| review-process-skill | 28,121 | 28,117 | 4 | — |
| review-core-semantics-skill | 7,293 | 7,289 | 4 | — |
| review-coverage-skill | 10,020 | 10,016 | 4 | — |
| review-excellence-skill | 7,026 | 7,022 | 4 | — |
| **review-implementation-skill** | **TBD** | **TBD** | 4 | — |
| review-socratic-skill | 5,179 | 5,175 | 4 | — |

> **说明**：本轮 P0 修复（improve-v2 §10）后，review-process-skill.md 从 16,088 字符增长到 28,121 字符（+74.7%），review-coverage-skill.md 从 8,032 字符增长到 10,020 字符（+24.7%）。增长主因是新增 Gate 0、Gate G、gate-evidence 块模板、L1/L2/L3 证据分级、Artifact Inventory、Severity Reconciliation、Per-Doc/Session Status、VERIFY-SELF/VERIFY-CROSS 等段。Trae 对单 Skill 文件无硬字符上限，仅 Agent Prompt ≤ 10,000。
>
> 2026-06-22 新增 **review-implementation-skill**（由 06-design-final.md 实施过程沉淀），覆盖 design ↔ code 一致性 + §X self-review issues 追踪。详见 skill 文件 §Skill 输出模板 + §Gate D-Impl。

### 规则集与 Skill 的对应关系

| 原始规则 | 转化产物 | 角色 | 当前字符 |
|---------|---------|------|---------|
| review.md | review-agent-ide.md | Agent（精简原则 + 路由 + 强制约束；详细知识下沉到 Skill） | 8,903 ✅ |
| review.md | review-agent-trigger.md | Agent（触发器描述 + 16 个示例，覆盖 8 域 + 工作流评估/修复阶段） | 4,913 ✅ |
| review-doc-checklist.md | review-doc-skill.md | Skill（§2.0 Claims-Evidence + §2.1-§2.11 + §3；强制逐行验证） | 13,247 |
| review-code-checklist.md | review-code-skill.md | Skill（§1-§15 + Kernel SMP/BKL §4.2） | 6,512 |
| review-patterns.md | review-patterns-skill.md | Skill（45 个错误模式；Gate D 严格通过标准） | 16,569 |
| review-process.md | review-process-skill.md | Skill（§〇三模式 + Step 0-7 + 修复阶段 + STATE.md 双路径 + Gate 证据 + Gate G VERIFY-CHECK 强制 + Gate 0 制品完整性 + L1/L2/L3 证据分级） | 28,121 |
| review-core-semantics.md | review-core-semantics-skill.md | Skill（行为契约表模板 + 8 字段 × 5 函数） | 7,293 |
| review-doc-excellence.md + review-code-excellence.md | review-excellence-skill.md | Skill（文档§4.1-4.4 + 代码§15-20 卓越性） | 7,026 |
| review-process.md §Step 1.5 | review-coverage-skill.md | Skill（机器穷举 + AI 语义判断 + doc-specific 覆盖率 + semantic-map + Gate A 强制运行规则 + gate-evidence-A 块模板） | 10,020 |
| review.md（苏格拉底追问话术） | review-socratic-skill.md | Skill（8 场景追问话术模板） | 5,179 |
| review-process.md §实施验证（2026-06-22 新增） | review-implementation-skill.md | Skill（design ↔ code 一致性 + §X self-review 追踪 + 后向兼容重构 + 测试边界） | TBD |
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
- **`AGENT.md`** — Trae/Claude 兼容字段，备选（与 CLAUDE.md 同义）。

### `.claude/` 目录结构

```
.claude/
├── settings.local.json          — 本地权限配置（allow/deny 工具白名单）
├── rules/                       — 始终加载（always-on），占用 context 持续存在
│   ├── review-core.md           —   执行模型 + ⛔ 禁止行为 + Ground Truth + 优先级 + 强制 Skill 调用 + 严格 Gate D
│   ├── review-process.md        —   Step 0-7 强制流程 + STATE 双路径 + Gate 证据 + VERIFY-CHECK 强制
│   └── fix-guard.md             —   安全修复规则
└── skills/                      — 按需加载（on-demand），不占用 context
    └── review-scan/             —   主 Skill
        ├── SKILL.md             —     Orchestrator（YAML frontmatter + Phase 1-9 + Explicit Skill Invocation）
        └── checks/              —     检查维度（5 个领域文件）
            ├── doc.md           —       文档检查（含禁止"未逐行验证"）
            ├── code.md          —       代码检查
            ├── patterns.md      —       错误模式（含 Gate D 严格通过标准）
            ├── process.md       —       执行流程（含 STATE 双路径、VERIFY-CHECK 强制）
            └── excellence.md    —       卓越性检查
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

- **`review-core.md`** — 始终加载。定义执行模型分用户态/内核、⛔ 禁止行为（从记忆回答/跳过检查/猜测/无输出标记 ✅/后期 check decay）、Ground Truth、Rewrite/Translate/Redesign 三态、P0/P1/P2 优先级、**强制 Skill 显式调用**、**Gate D 严格通过标准**。
- **`review-process.md`** — 始终加载。定义 Step 0-7 流程（Scope 声明 → C 源验证 → Diff 抽取 → Sanity Check → Precision Check → 状态写入 → VERIFY 独立验证），以及 **STATE.md 双路径**、**Gate 证据规则**、**VERIFY-CHECK.md 强制**。
- **`fix-guard.md`** — 始终加载。定义安全修复的 4 条强制要求（读 ±5 行上下文 / grep 确认当前状态 / 一次性应用一个 fix / 写 fix-status）。

### `.claude/skills/review-scan/` 的 on-demand 加载

- **`SKILL.md`** — Orchestrator。YAML frontmatter 定义 `name`、`description`、`allowed-tools`。Phase 1-9 控制执行顺序：**Scope → Coverage Enumeration → Gap Scan → Doc Checks → Code Checks → Patterns → Excellence → Cross-doc → Report**。含 evidence 分级、"先读后判"强制规则、**Explicit Skill Invocation**、**STATE.md 双路径**、**scan.md 双写规则**。
- **`checks/doc.md`** — 文档检查。覆盖 §2.0 Claims-Evidence、概念准确性、C 代码引用（**禁止"未逐行验证"**）、数据结构、doc-code 一致性、架构演进、跨引用、图示、C 源码覆盖、设计质量、链接验证、文档风格、skip 检查。
- **`checks/code.md`** — 代码检查。覆盖 rewrite 质量、硬件抽象、trait 设计、类型安全、执行模型（含 SMP/BKL §4.2）、内存模型、模块设计、命名、测试、注释、64-bit、复杂度、no_std、设计-代码一致性、C-Rust 对齐、精度检查。
- **`checks/patterns.md`** — 错误模式库。文档 15 + 代码 14 + 跨文档 3 + 测试 6 + 卓越性 7 = 45 个模式。含 **Gate D 严格通过标准**。
- **`checks/process.md`** — 执行流程。含 §〇 三模式选择（构造/快速/深度）、**STATE.md 双路径**、**Gate 证据规则**、**VERIFY-CHECK.md 强制**、**P0/P1/P2 同步规则**。
- **`checks/excellence.md`** — 卓越性检查。文档 §4.1-4.4 + 代码 §15-20。

### Claude 配置的设计意图

- **`CLAUDE.md` 是入口**：Claude Code 启动时自动注入项目背景（"这是 Minix-RS，是 Minix3 的 Rust Rewrite，不是翻译，是改写，no_std，硬件抽象为 trait"）。
- **`.claude/rules/` 是底线**：执行模型、禁止行为、流程——每次都加载，因为它们定义"必须遵守的约束"。
- **`.claude/skills/` 是按需加载**：避免一次注入所有检查导致 context 占用过高；Claude 根据用户意图决定加载哪个 check。

---

## 状态管理与收敛

每次 Review 在**项目根 `.review/`** 下创建工具特定的输出目录，统一布局，`trae/` 与 `claude/` 互不干扰。两套工具**绝不共享任何中间结果**（STATE/scan/SYMBOLS/structure/VERIFY-CHECK）；Bagging 聚合只发生在 Trae 内（多 AI 的 scan 聚合）。

### 路径变量

- `{module}` = rewrite 模块名 = `notes/rewrite/{module}/` 的目录名（如 `fork-syscall-rewrite`）。取目标文档所在路径中 `notes/rewrite/` 下的第一级目录名。
- `{stage}` = 模块下的阶段子目录（如 `03-stage-kernel`），仅作 `{module}` 内分组，不替代 `{module}`。
- `{doc-stem}` = 目标文档去扩展名（如 `03-kmain-cstart`）。
- `{agent}` = AI 模型标识（Trae 内：glm/kimi/ds/qwen/seed/...；Claude 内：m3/glm-flash）。
- **`{module}` 与覆盖率脚本 `--module` 是两个不同概念**：本路径的 `{module}` 是 rewrite 模块名；覆盖率脚本的 `--module kernel` 是 Minix3 模块名。不得混用。

### 统一布局（项目根 `.review/` 下分 `trae/` 与 `claude/`）

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
└── claude/{module}/
    ├── STATE.md                              # Claude 专属，与 trae 隔离
    ├── VERIFY-CHECK.md                       # Claude 专属
    └── {doc-stem}/{scan,structure,SYMBOLS}.md
```

> 采用**扁平 scans/ 结构**，不启用 `{stage}/` 子目录（`{doc-stem}` 已含 stage 编号前缀，足够区分；见 improve-v2 §7.2）。

### 双写模式（交互式修复场景）

需交互式修复 review todo 时，除标准中间产物外，**额外**在被 review 文档同目录下写可读修复文档：

- Trae 交互式修复文档：`notes/rewrite/{module}/{stage}/{doc-stem}-trae-review.md`
- Claude 最终报告（可选）：`notes/rewrite/{module}/{stage}/{doc-stem}-claude-report.md`

修复文档/最终报告 = 标准 scan 的**可读子集 + todo 进度跟踪**（按 issue ID/位置匹配的子集关系，非 checksum 一致）。scan.md 的 Artifact Inventory 表列出两份路径。

### 双路径规则

1. Step 0 先判断当前运行环境（Trae / Claude），读取对应 `.review/{tool}/{module}/STATE.md`。
2. 如果同一工具下两份 STATE.md 同时存在且内容矛盾，**不要自动合并**，在 scan.md 中记录分歧并询问用户哪个为准。
3. 每个 STATE.md 独立维护自己的 Open P0/P1/P2 列表；新增问题必须同步进 Open 列表，已修复问题移入 Closed Issues。
4. 收敛条件必须满足：scan.md 所有维度 COMPLETE、最近 Pass 新增 P0=0/P1≤1、Gate G VERIFY-CHECK = PASS、Blocker Gates 0/A-E+G 全部通过且有 gate-evidence 附件。

> **精简说明**：所有维度检查结果（概念/引用/结构/覆盖/设计/链路/代码/跨文档/Claims）全部并入 `scan.md` 对应章节，不再拆 10 个维度文件。

**收敛终止条件**（全部满足才算审查完成）：
1. scan.md 中所有维度章节标记 COMPLETE
2. P0 新增数量 = 0（最近一次完整 Pass）
3. P1 新增数量 ≤ 1
4. **Gate G** 独立验证（VERIFY-CHECK.md）结果为 PASS
5. scan.md 中所有 P0 已被修复并验证通过
6. SYMBOLS.md 覆盖率穷举完成（所有 C 符号有"已覆盖/ARCH 不需要/缺口"判定）
7. **Blocker Gates 0/A/B/C/D/D-6/E/G 全部通过且有 gate-evidence 附件**（见 review-process-skill.md）
8. STATE.md 双路径无未解决的冲突（若两个 STATE 均存在，需用户确认权威版本）

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
