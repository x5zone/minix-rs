# prompt/

本目录包含 Minix-RS 项目的 Review 规则集及 Trae IDE 适配产物。

## 目录结构

```
prompt/
├── README.md              — 本文件
├── review-rules/          — 原始 Review 规则集
│   ├── review.md          —   Review 核心框架（原则、约束、优先级、输出模板）
│   ├── review-doc-checklist.md  —  文档检查清单（§1~§3，十个维度）
│   ├── review-code-checklist.md —  代码检查清单（§1~§14，十四个维度）
│   ├── review-patterns.md       —  常见错误模式（文档 14 个、代码 10 个、跨文档 3 个）
│   ├── review-process.md        —  执行流程（Step 0~7 + 工具命令）
│   └── review-profiles.md       —  任务组合配置（Profile A~K，含分阶段策略）
└── skill/                 — Trae IDE 可用的 Agent 与 Skill 定义
    ├── review-agent.md         —  智能体定义（路由、决策、输出控制）
    ├── review-doc-skill.md     —  文档 Review 技能
    ├── review-code-skill.md    —  代码 Review 技能
    ├── review-patterns-skill.md —  错误模式对照技能
    └── review-process-skill.md —  执行流程技能
```

## review-rules/

Review 规则集是项目在多轮迭代中积累的规则文档，定义了针对 Minix-RS 项目的所有 Review 要求。其逻辑顺序为：

1. **review.md** — 核心框架，定义 Rewrite/Translate/Redesign 三态（原则层）
2. **review-profiles.md** — 任务组合配置，决定不同场景加载哪些规则模块（策略层）
3. **review-process.md** — 强制执行步骤，要求每个步骤必须产生可见中间产物（流程层）
4. **review-doc-checklist.md** — 文档维度的检查清单，含概念准确性、C 源码覆盖完整性、章节链路验证等（文档维度层）
5. **review-code-checklist.md** — 代码维度的检查清单，含 Rewrite 质量、硬件抽象、no_std 约束、C-Rust 语义对齐等（代码维度层）
6. **review-patterns.md** — 常见错误模式汇总，供 Review 时对照识别（错误模式层）

## skill/

`skill/` 目录下的文件是 Trae IDE 的 Agent 与 Skill 定义，由 `review-rules/` 规则集转化而来，供 IDE 加载使用。

### 架构说明

```
review-agent（智能体 / 路由器）
    │
    ├── 加载 ▶ review-doc-skill（文档检查能力）
    ├── 加载 ▶ review-code-skill（代码检查能力）
    ├── 加载 ▶ review-patterns-skill（错误模式对照能力）
    └── 加载 ▶ review-process-skill（执行流程能力）
```

- **Agent**（review-agent.md）：负责路由与决策。根据用户意图判断应加载哪些 Skill，控制输出格式，强制执行约束。Agent 之间不互相调用。
- **Skill**（review-*-skill.md）：各自负责独立的领域能力，互不引用。Agent 按需加载，Skill 本身不决策，仅提供规则和步骤。

### 使用方式

1. 将 `review-agent.md` 的内容复制粘贴至 Trae IDE 的智能体定义中
2. 将四个 `review-*-skill.md` 文件各自的内容复制粘贴至 IDE 的技能定义中
3. 在对话中由 Agent 根据用户意图自动调度 Skill

## 规则集与 Skill 的对应关系

| 原始规则 | 转化产物 | 角色 |
|---------|---------|------|
| review.md | review-agent.md | Agent（原则 + 路由 + 输出模板） |
| review-doc-checklist.md | review-doc-skill.md | Skill |
| review-code-checklist.md | review-code-skill.md | Skill |
| review-patterns.md | review-patterns-skill.md | Skill |
| review-process.md | review-process-skill.md | Skill |
| review-profiles.md | review-agent.md（路由指令部分） | 并入 Agent |
