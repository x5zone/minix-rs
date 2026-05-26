# prompt/

本目录包含 Minix-RS 项目的 Review 规则集及 Trae IDE 适配产物。

## 目录结构

```
prompt/
├── README.md              — 本文件
├── review-rules/          — 原始 Review 规则集
│   ├── review.md          —   Review 核心框架（原则、约束、优先级、输出模板）
│   ├── review-doc-checklist.md  —  文档检查清单（§1~§3，含 §2.0 Claims-Evidence）
│   ├── review-code-checklist.md —  代码检查清单（§1~§14，含 Kernel SMP/BKL 并发）
│   ├── review-patterns.md       —  常见错误模式（文档 15 个、代码 14 个、跨文档 3 个）
│   ├── review-process.md        —  执行流程（Step 0~7 + 状态追踪 + 收敛判断）
│   └── review-profiles.md       —  任务组合配置（Profile A~K，含分阶段策略）
└── skill/                 — Trae IDE 可用的 Agent 与 Skill 定义
    ├── review-agent.md         —  智能体定义（路由、决策、输出控制、状态管理）
    ├── review-doc-skill.md     —  文档 Review 技能（含 §2.0 Claims-Evidence）
    ├── review-code-skill.md    —  代码 Review 技能（含 §4.2 Kernel SMP/BKL 并发）
    ├── review-patterns-skill.md —  错误模式对照技能（含 Kernel SMP 并发模式 25-28）
    └── review-process-skill.md —  执行流程技能（含状态追踪 + 独立验证）
```

`.review/` 目录为 Review 执行时自动生成的**状态持久化输出**，不在本目录中（见下方「状态管理与收敛」）。

## review-rules/

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

## skill/

`skill/` 目录下的文件是 Trae IDE 的 Agent 与 Skill 定义，由 `review-rules/` 规则集转化而来，供 IDE 加载使用。

### 架构说明

```
review-agent（智能体 / 路由器）
    │
    ├── 加载 ▶ review-doc-skill（文档检查能力：§2.0 Claims + §2.1-§2.11 + §3）
    ├── 加载 ▶ review-code-skill（代码检查能力：§1-§14 + Kernel SMP/BKL §4.2）
    ├── 加载 ▶ review-patterns-skill（错误模式对照能力：文档15+跨文档3+代码14）
    └── 加载 ▶ review-process-skill（执行流程能力：Step 0-7 + 状态追踪 + 独立验证）
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
| review.md | review-agent.md | Agent（原则 + 路由 + 输出模板 + 执行模型 + 状态管理） |
| review-doc-checklist.md | review-doc-skill.md | Skill（§2.0 Claims-Evidence + §2.1-§2.11 + §3） |
| review-code-checklist.md | review-code-skill.md | Skill（§1-§14 + Kernel SMP/BKL §4.2） |
| review-patterns.md | review-patterns-skill.md | Skill（32 个错误模式：文档15+跨文档3+代码14） |
| review-process.md | review-process-skill.md | Skill（Step 0-7 + 状态追踪 + 独立验证） |
| review-profiles.md | review-agent.md（路由指令部分） | 并入 Agent |

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