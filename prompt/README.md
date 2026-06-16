# prompt/

本目录包含 Minix-RS 项目的 Review 规则集及两套 IDE/Runtime 适配产物：**Trae IDE**（手工复制粘贴）与 **Claude Code Runtime**（项目根 + `.claude/` 自动加载）。

## 目录结构

```
prompt/
├── README.md                — 本文件
├── review-rules/            — 原始 Review 规则集（唯一真相源）
│   ├── review.md            —   Review 核心框架（原则、约束、优先级、输出模板）
│   ├── review-doc-checklist.md  —  文档检查清单（§1~§3，含 §2.0 Claims-Evidence）
│   ├── review-code-checklist.md —  代码检查清单（§1~§14，含 Kernel SMP/BKL 并发）
│   ├── review-patterns.md       —  常见错误模式（文档 15 个、代码 14 个、跨文档 3 个）
│   ├── review-process.md        —  执行流程（Step 0~7 + 状态追踪 + 收敛判断）
│   └── review-profiles.md       —  任务组合配置（Profile A~K，含分阶段策略）
├── skill/                   — Trae IDE 适配层（手工复制粘贴至 IDE）
│   ├── review-agent.md          —  智能体主定义（路由 + 决策 + 输出控制 + 状态管理）
│   ├── review-agent-ide.md      —  智能体精简版（适配 Trae 10k 字硬限制）
│   ├── review-agent-trigger.md  —  触发器描述（何时调用 Agent）
│   ├── review-doc-skill.md      —  文档 Review 技能（含 §2.0 Claims-Evidence）
│   ├── review-code-skill.md     —  代码 Review 技能（含 §4.2 Kernel SMP/BKL 并发）
│   ├── review-patterns-skill.md —  错误模式对照技能（含 Kernel SMP 并发模式 25-28）
│   └── review-process-skill.md  —  执行流程技能（含状态追踪 + 独立验证）
├── improve.md               — 规则集多轮改进计划（演进记录）
└── advice-glm.md            — GLM 独立改进建议（独立 AI 视角）
```

`.review/` 目录为 Review 执行时自动生成的**状态持久化输出**，不在本目录中（见下方「状态管理与收敛」）。

---

## review-rules/（唯一真相源）

Review 规则集是项目在多轮迭代中积累的规则文档，定义了针对 Minix-RS 项目的所有 Review 要求。其逻辑顺序为：

1. **review.md** — 核心框架，定义 Rewrite/Translate/Redesign 三态（原则层）
   - **v2 新增**：执行模型按模块分层——用户态服务器（单线程）vs 内核（SMP + BKL）
2. **review-profiles.md** — 任务组合配置，决定不同场景加载哪些规则模块（策略层）
3. **review-process.md** — 强制执行步骤，要求每个步骤必须产生可见中间产物（流程层）
   - **v2 新增**：Step 5.5 状态写入与收敛判断、Step 5.6 Review Verification Protocol（独立验证）
4. **review-doc-checklist.md** — 文档维度的检查清单（文档维度层）
   - **v2 新增**：§2.0 Claims-Evidence Tracing（论文级文档质量方法论）
5. **review-code-checklist.md** — 代码维度的检查清单（代码维度层）
   - **v2 新增**：§4.2 内核 SMP/BKL 并发检查项（BKL 持有验证、CPU-local 数据、共享状态保护、Rust 类型系统与 SMP）
6. **review-patterns.md** — 常见错误模式汇总（错误模式层）
   - **v2 新增**：模式 25-28（Kernel SMP 并发违规：BKL 未持有、Rc/RefCell 跨 CPU、spinlock 内睡眠、per-CPU 跨 CPU 访问）

---

## skill/（Trae IDE 适配层）

`skill/` 目录下的文件是 Trae IDE 的 Agent 与 Skill 定义，由 `review-rules/` 规则集转化而来。**手工复制粘贴**至 Trae IDE 的对应配置入口（不是项目级自动加载）。

### Trae IDE 字符/字段限制（关键）

> 以下限制来自 Trae 官方文档与社区实测，**直接决定 skill 文件能否保存与生效**。新加内容前先核对字数。

| 项 | 限制 | 来源 | 当前文件 | 状态 |
|---|------|------|---------|------|
| **Agent Prompt（提示词）** | **硬上限 10,000 字符**（自动截断） | [Trae 官方 FAQ](https://forum.trae.cn/t/topic/7571) | `review-agent-ide.md` | **9,784 字符（≈ 97.8%）⚠️** |
| Rule（规则） | 硬上限 20,000 byte；建议 ≤ 10,000 字符；token 视角约 3,000 token | [Trae 官方 FAQ](https://forum.trae.cn/t/topic/52) | n/a（本目录无 Rule 文件） | — |
| **Skill `name`** | ≤ **64 字符**，仅小写字母/数字/连字符（`-`），与父目录同名 | [Trae Skill 规范](https://docs.trae.ai/ide/best-practice-for-how-to-write-a-good-skill) | n/a（Trae Skill 命名规范） | — |
| **Skill `description`** | ≤ **1024 字符**（硬限制），建议 ≤ 200 字符 | 同上 | n/a | — |
| Agent `description`（触发器描述） | 无明确公开硬上限；社区实测 ≤ ~1,024 字符稳定，< 2,000 字符安全 | 社区实测 | `review-agent-trigger.md` | **1,868 字符 ✅**（含 7 个触发示例） |
| MCP 工具总数 | ≤ **40 个** | [TRAE MCP 指南](https://blog.csdn.net/2601_96144997) | n/a | — |
| MCP 描述总字符数 | ≤ **8,000 字符**（超出丢弃） | 同上 | n/a | — |
| 上下文窗口（最强模型） | 240,000 tokens 输入 / 32,000 tokens 输出 | [Trae 模型文档](https://docs.trae.ai/ide/models) | 全局 | — |

### `review-agent-ide.md` 的 10,000 字限制说明

- **当前 9,784 字符**，已逼近 10,000 字符硬上限，**不能再追加内容**。新需求必须通过删除/重组实现。
- `review-agent.md`（14,761 字节 ≈ **完整版**）是当前 Trae 限制下放不下的完整规则集。
- **`review-agent-ide.md` 是手工精简版**：在保留执行模型（单线程 vs SMP+BKL）、路由表、加载规则等核心前提下，省略了次要细节（详细 P0/P1 示例、扩展 §16 等），尽量压在 10k 以内。
- **如必须再加内容**：先精简 §1/§2 的"可省略"段落，再重新计算字符数；超出 10k 字符会被自动截断导致 IDE 保存失败或行为异常。
- **复测命令**（bash）：`wc -m prompt/skill/review-agent-ide.md`

### `review-agent-trigger.md` 的限制说明

- 该文件对应 Trae Agent 配置中的 **"何时调用 / trigger description"** 字段，描述 Agent 在什么场景被自动调用（伴随 7 个 `<example>` 块）。
- 官方未公布该字段的精确硬上限，社区实测在 ≤ 2,000 字符内稳定。当前 1,868 字符有充足余量。
- **不要盲目加触发示例**：每加 1 个 `<example>` 块（≈ 200 字符）就消耗 1% 余量；触发示例应聚焦于**最常见的 5-7 个用户意图**，不追求穷举。

### 架构说明

```
review-agent-ide（智能体 / 路由器 + 核心规则）
    │
    ├── 加载 ▶ review-doc-skill（文档检查能力：§2.0 Claims + §2.1-§2.11 + §3）
    ├── 加载 ▶ review-code-skill（代码检查能力：§1-§14 + Kernel SMP/BKL §4.2）
    ├── 加载 ▶ review-patterns-skill（错误模式对照能力：文档15+跨文档3+代码14）
    └── 加载 ▶ review-process-skill（执行流程能力：Step 0-7 + 状态追踪 + 独立验证）
```

- **Agent**（review-agent-ide.md）：负责路由与决策。根据用户意图判断应加载哪些 Skill，控制输出格式，强制执行约束。Agent 之间不互相调用。
- **Skill**（review-*-skill.md）：各自负责独立的领域能力，互不引用。Agent 按需加载，Skill 本身不决策，仅提供规则和步骤。

### 使用方式（手工复制粘贴）

1. 在 Trae IDE 打开「智能体」配置面板（右上角 → 智能体 → 创建智能体）
2. 将 `review-agent-ide.md` 的内容**完整复制粘贴**至"提示词（Prompt）"输入框
3. 将 `review-agent-trigger.md` 的内容**完整复制粘贴**至"何时调用"输入框
4. 启用所需 MCP 工具（建议启用：文件系统、终端、联网搜索）
5. 在「规则与技能」面板，将 4 个 `review-*-skill.md` 各自作为 Skill 导入（注意 Trae 的 Skill 有 `name`/`description` 字段约束，见上表）

### 规则集与 Skill 的对应关系

| 原始规则 | 转化产物 | 角色 | 当前字符 |
|---------|---------|------|---------|
| review.md | review-agent-ide.md | Agent（精简原则 + 路由 + 输出模板 + 执行模型） | 9,784 |
| review.md | review-agent-trigger.md | Agent（触发器描述 + 7 个示例） | 1,868 |
| review-doc-checklist.md | review-doc-skill.md | Skill（§2.0 Claims-Evidence + §2.1-§2.11 + §3） | 8,608 |
| review-code-checklist.md | review-code-skill.md | Skill（§1-§14 + Kernel SMP/BKL §4.2） | 6,512 |
| review-patterns.md | review-patterns-skill.md | Skill（32 个错误模式：文档15+跨文档3+代码14） | 7,723 |
| review-process.md | review-process-skill.md | Skill（Step 0-7 + 状态追踪 + 独立验证） | 6,768 |
| review-profiles.md | review-agent-ide.md（路由指令部分） | 并入 Agent | — |

---

## Claude Code Runtime 适配（项目根 + .claude/）

Claude Code Runtime 的配置**自动加载**，与 Trae 完全不同：

- 加载位置：**项目根** + **`.claude/`** 目录
- 加载机制：Claude Code 启动时自动读取
- 不需要手工复制粘贴

### 项目根配置

- **`CLAUDE.md`**（项目根，1,880 字符）— Claude Code 启动时自动注入的**项目级 system prompt**。包含构建/测试命令、目录布局、执行模型分用户态/内核、编码约束（`no_std`、错误码映射 Minix3 errno、硬件抽象为 trait）、Ground Truth 优先级、文档结构（Ch1→Ch2→Ch3→Ch4+测试章节）。
- **`AGENT.md`** — Trae/Claude 兼容字段，备选（与 CLAUDE.md 同义）。

### `.claude/` 目录结构

```
.claude/
├── settings.local.json          — 本地权限配置（allow/deny 工具白名单，360 字节）
├── rules/                       — 始终加载（always-on），占用 context 持续存在
│   ├── review-core.md           —   执行模型 + ⛔ 禁止行为 + Ground Truth + 优先级（2,476 字符）
│   ├── review-process.md        —   Step 0-7 强制流程（2,465 字符）
│   └── fix-guard.md             —   安全修复规则（1,097 字符）
└── skills/                      — 按需加载（on-demand），不占用 context
    └── review-scan/             —   主 Skill
        ├── SKILL.md             —     Orchestrator（YAML frontmatter + Phase 1-4，4,921 字符）
        └── checks/              —     检查维度
            ├── doc/             —       12 个 doc 检查（00-claims-evidence → 12-skip-check）
            ├── code/            —       16 个 code 检查（01-rewrite-quality → 16-precision-check）
            └── patterns/        —       3 个模式检查（doc/code/cross）
```

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

- **`review-core.md`**（2,476 字符）— 始终加载。定义执行模型分用户态/内核、⛔ 禁止行为（从记忆回答/跳过检查/猜测/无输出标记 ✅/后期 check decay）、Ground Truth、Rewrite/Translate/Redesign 三态、P0/P1/P2 优先级。
- **`review-process.md`**（2,465 字符）— 始终加载。定义 Step 0-7 流程（Scope 声明 → C 源验证 → Diff 抽取 → Sanity Check → Precision Check → 状态写入 → VERIFY 独立验证）。
- **`fix-guard.md`**（1,097 字符）— 始终加载。定义安全修复的 4 条强制要求（读 ±5 行上下文 / grep 确认当前状态 / 一次性应用一个 fix / 写 fix-status）。

### `.claude/skills/review-scan/` 的 on-demand 加载

- **`SKILL.md`**（4,921 字符）— Orchestrator。YAML frontmatter 定义 `name`、`description`、`allowed-tools`。Phase 1-4 控制执行顺序：**Scope → Gap Scan → Doc Checks (12) → Code Checks (16) → Patterns (3) → Report**。
- **`checks/doc/*.md`**（12 个，629~2,352 字符）— 文档检查维度。覆盖 §2.0 Claims-Evidence、概念准确性、C 代码引用、数据结构、doc-code 一致性、架构演进、跨引用、图示、C 源码覆盖、设计质量、链接验证、文档风格、skip/假装检查。
- **`checks/code/*.md`**（16 个，744~2,694 字符）— 代码检查维度。覆盖 rewrite 质量、硬件抽象、trait 设计、类型安全、执行模型（含 SMP/BKL §4.2）、内存模型、模块设计、命名、测试、注释、64-bit、复杂度、no_std、设计-代码一致性、C-Rust 对齐、精度检查。
- **`checks/patterns/*.md`**（3 个，1,328~4,081 字符）— 错误模式库。文档 15 个 + 代码 14 个 + 跨文档 3 个。

### Claude 配置的设计意图

- **`CLAUDE.md` 是入口**：Claude Code 启动时自动注入项目背景（"这是 Minix-RS，是 Minix3 的 Rust Rewrite，不是翻译，是改写，no_std，硬件抽象为 trait"）。
- **`.claude/rules/` 是底线**：执行模型、禁止行为、流程——每次都加载，因为它们定义"必须遵守的约束"。
- **`.claude/skills/` 是按需加载**：避免一次注入所有检查导致 context 占用过高；Claude 根据用户意图决定加载哪个 check。

---

## 状态管理与收敛

> v2 新增：解决"反复 Review 仍发现新错误"的核心方案。

每次 Review 在目标文档/代码的上级目录自动创建 `.review/{module}/` 输出目录：

```
.review/{module}/
├── STATE.md              — 审查进度状态（跨会话持久，下轮从这里开始）
├── FINDINGS.md           — 汇总的 P0/P1/P2 问题清单
├── CONCEPT-CHECK.md      — §2.1 概念准确性验证结果
├── REF-CHECK.md          — §2.2 C代码引用验证结果
├── STRUCT-CHECK.md       — §2.3 数据结构覆盖结果
├── COVERAGE-CHECK.md     — §2.8 C源码覆盖完整性结果
├── DESIGN-CHECK.md       — §2.9 设计决策质量结果
├── LINK-CHECK.md         — §2.10 章节链路验证结果
├── CODE-CHECK.md         — Code §1-14 各维度结果（含 SMP/BKL 检查）
├── CROSS-DOC-CHECK.md    — 跨文档联动检查结果
├── CLAIMS-CHECK.md       — §2.0 Claims-Evidence 逐 claim 验证结果
└── VERIFY-CHECK.md       — 独立验证结果（Review-of-Review）
```

**收敛终止条件**（全部满足才算审查完成）：
1. 所有 10 个维度检查文件标记 COMPLETE
2. P0 新增数量 = 0（最近一次完整 Pass）
3. P1 新增数量 ≤ 1
4. 独立验证（VERIFY-CHECK.md）结果为 PASS
5. FINDINGS.md 中所有 P0 已被修复并验证通过

## 执行模型分层（v2）

> **关键变更**：Review 不再假设"所有模块单线程"。内核有 SMP + BKL，用户态服务器仍是单线程事件循环。

| 模块类型 | 执行模型 | 并发保护 | Rc/RefCell | UnsafeCell 安全论据 |
|---------|---------|---------|-----------|-------------------|
| 用户态服务器（VM/PM/VFS/RS/DS/INET） | 单线程事件循环 | IPC 隔离 | ✅ 合理 | 单线程前提 |
| 内核（Kernel） | SMP + BKL spinlock | BKL_LOCK()/UNLOCK() | ❌ 多核共享需 Arc | BKL 保护 / per-CPU / Atomic |

详见 [review.md §执行模型](review-rules/review.md) 和 [review-agent.md §执行模型](skill/review-agent.md)。