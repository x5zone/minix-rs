# prompt/todo.md

`prompt/` 目录的规则源 todo 列表。每条记录**待纳入**哪个规则文件、**待同步**到哪些派生产物，落地后删除本条目（保留完整历史请移至 git commit message 或 PR description）。

---

## 元原则 (架构抽象设计原则，2026-08-15 提出)

**来源**：`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/05-clock-interrupt-init.md §4.7.1` 重构 `ArchBoot::register_timer_handler` 时归纳的四条设计原则。已确认这些原则**不属于单一 trait 章节**——它们是跨 trait / 跨模块的普适设计指导，不应散落在各章节末尾重复。

### 内容（待落地为 review 规则）

1. **trait 成员由 architecture variance 决定，不由调用时序决定**——架构抽象应按子系统能力（`ClockArch` / `TimerIrqGate` / …）分，而非按 boot 调用时机。
2. **不为 mock 写抽象，不预建假想差异的抽象**——在真硬件 binding 出现前删除占位 trait；YAGNI。
3. **生产实现不得触碰测试状态（`MOCK_*` 全局）**——测试 mock 的 static 只存在于 `#[cfg(test)]` 或 mock 实现自身；真实现写 mock state 属 P0-class 缺陷。
4. **cfg 边界**：target-specific `cfg` 仅存在于 arch crate 的 `Current*` 选择 alias，绝不泄漏到 capability 使用方。

### 候选落地位置（待决策）

- **(A) `prompt/review-rules/review-patterns.md`**：作为 79 个模式之后的新增模式（编号 80-83），归入"代码 / 设计"类。每条模式遵循既有格式（名称 + 描述 + 反例 + 验证命令）。优点：复用现有规则文档结构，三端同步路径清晰。
- **(B) `prompt/review-rules/review-core.md`** "Rewrite 定义"段：作为"允许/禁止"的扩展。该段目前讲 "Allowed: data structure reorganization / Forbidden: changing external behavior"，元原则属于同类（描述架构抽象边界）。优点：与"Rewrite 定义"语义连贯。
- **(C) 新建 `prompt/review-rules/review-design-principles.md`**：独立的"架构抽象原则"小文档。优点：聚焦；缺点：过度工程化（仅 4 条规则不必单文件）。

### 待同步派生产物（落地后必须同步）

- `CLAUDE.md`（项目根）
- `.claude/rules/review-core.md` + `.claude/rules/review-process.md`（如新增内容）
- `.claude/skills/review-code-skill/SKILL.md`（如涉及 §X.X）
- `.trae/skills/review-code-skill/SKILL.md`
- `.codex/skills/review-code-skill/SKILL.md`
- `.trae/documents/review-rules-meta-review-plan.md`（如影响规则集结构）

### 反例来源（落地时引用，**不**写入 review 规则本身）

§4.7.1 重构时四个具体反例已在源文档保留，落地 review 规则时引用即可，无需在 review 规则内重述：

- 第 1 条反例：`ArchBoot` 把"boot 时序入口"当成"架构能力边界"
- 第 2 条反例：`ArchBoot::register_timer_handler` 三架构实现均为 mock 占位（"未实现的一致"）
- 第 3 条反例：旧 `X86_64TimerIrqGate::enable_timer_irq` 真路径写 `MOCK_IRQ_ENABLED`，LAPIC 未映射时 fall back 写 mock
- 第 4 条反例：target-specific `#[cfg(target_arch)]` 泄漏到 capability 使用方

### 状态

- [x] 决策落地位置（A / B / C）——**选 A**（2026-09-05：4 条均可判 violate、带验证命令，适合模式格式；B 的 review-core"允许/禁止"段过于原则化不可 grep；C 单文件过度工程化）
- [x] 落地到选定文件——review-patterns.md 新增 §九 模式 79-82（2026-09-05）
- [x] 同步三端派生产物——prompt/skill/review-patterns-skill.md §X.7 压缩版 + generate-derived-skills.sh 重生成
- [ ] 删除本条目（保留完整历史请 git commit message）——**回归检查通过后可删除本文件**

---

## trait 设计质量评估纳入 doc review 的 Ch3 维度（2026-09-05 提出）

**来源**：`AI-chats/daily.todo.md` 提问 —— "prompt/review.md里面有关于Rust设计的评估吗？像这次 VmPagingExt trait 就是在阅读中，通过讨论发现的，之前对 07 文档 review，都没发现这个设计问题？"

### 问题分析（规则缺口）

1. trait 设计质量评估规则**已存在**：`prompt/review-rules/review-code-checklist.md §2.5`（多态必要性 / trait bound 使用 / 单方法 trait / 机制 vs 策略分离 / 跨架构差异验证）+ `review-patterns.md 模式 25`（反例恰好是 VmPagingExt 案例，说明该案例事后已补录）。
2. 但 `review.md` §4 维度表（L596）明确 **§2.5 = 仅完整 Review 才检查**。doc-only review（Profile A / 局部 Ch1&2）不加载 §2.5。
3. 机制原因：trait 抽象问题主要藏在 design doc 的 Ch3（Rust 设计决策）中，而 doc review 的 §2.9 设计决策质量检查未要求交叉核对 trait 抽象合理性 → 文档级 review 系统性漏检此类设计问题。
4. 未来风险：仅靠"事后补录反例到模式 25"不够——触发条件（仅完整 Review）未变，同类问题在 doc review 中仍会漏检。

### 候选落地位置（待决策）

- **(A) `review-doc-checklist.md §2.9 设计决策质量`**：新增子检查项"Ch3 声明的 trait/类型抽象是否满足 code-checklist §2.5 判据（≥2 行为不同实现 + 作为泛型 bound 使用）"。doc review 直接可执行，无需改变 Profile 加载。优点：改动局部、doc review 立即生效。
- **(B) `review-profiles.md`**：将 §2.5 从"仅完整 Review"扩展到 Profile R（design review）/ doc review 含 Ch3 设计决策时加载。优点：不污染 doc checklist；缺点：需维护 Profile 加载矩阵。
- **(C) `review.md §4 维度表 + Layer 分层`**：新增 Layer 判据，使 doc review 遇到 Ch3 时自动触发 §2.5 交叉检查。优点：机制级修复；缺点：改动面最大。

### 待同步派生产物（落地后必须同步）

- `prompt/skill/review-doc-skill.md`（若选 A）
- `prompt/skill/review-code-skill.md` 或 `prompt/skill/review-profiles` 对应 skill（若选 B/C）
- `.claude/rules/review-process.md` / `.claude/skills/`（如新增内容）
- `.trae/skills/` + `.codex/skills/` 对应 skill（用 `tools/generate-derived-skills.sh` 生成）
- 全部用 `tools/check-review-rules.sh` 校验漂移

### 反例来源（落地时引用，不写入 review 规则本身）

- `VmPagingExt` trait：doc-only review 未触发 trait 设计质量评估，设计问题仅在阅读/讨论中发现 —— 已补录为 `review-patterns.md 模式 25` 反例，但触发条件缺口仍在。

### 状态

- [x] 决策落地位置（A / B / C）——**选 A**（2026-09-05：改动局部、doc review 立即生效，无需改 Profile 加载矩阵；B/C 需维护加载机制，收益不成比例）
- [x] 落地到选定文件——review-doc-checklist.md §2.9 新增 Step 2.9.5（trait/类型抽象质量交叉核对，判据回指 code-checklist §2.5 + 模式 79-82）
- [x] 同步三端派生产物——prompt/skill/review-doc-skill.md §2.9 镜像 + 派生重生成
- [ ] 删除本条目（保留完整历史请 git commit message）——**回归检查通过后可删除本文件**

---

## 维护约定

- 新增条目：日期 + 标题 + 内容 + 落地位置 + 同步清单 + 状态（`- [ ]`）
- 完成：勾选状态 `[x]` + 在 commit message 引用本文件条目
- 长期未动（>30 天）的条目移入 `.review/claude/` 状态文件归档