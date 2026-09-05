# Review 任务组合配置


> **⚠️ 现役入口变更（2026-09-05）**：任务一级入口已迁移至 [review-cmds.md](review-cmds.md) 的 6 个 cmd（full-review / style-fix / code-excellence / test-audit / todo-fix / style-bible）。本文件的 Profile 定义保留为：(a) 历史 STATE.md/scan.md 引用的别名；(b) cmd 加载规则的模块矩阵细节来源。新任务请从 cmd 入口进入，对账表见 review-cmds.md §八。

> 本文档定义不同 Review 场景下应加载的模块组合。
> 目的：根据任务类型按需加载，避免上下文过载。

---

## 配置说明

| 配置项 | 说明 |
|--------|------|
| **核心** | 始终加载：[review.md](review.md)（核心原则 + 路由表 + 优先级矩阵） |
| **文档检查清单** | [review-doc-checklist.md](review-doc-checklist.md) |
| **代码检查清单** | [review-code-checklist.md](review-code-checklist.md) |
| **错误模式** | [review-patterns.md](review-patterns.md) |
| **执行流程** | [review-process.md](review-process.md)（强制步骤 + 工具命令） |
| **核心语义** | [review-core-semantics.md](review-core-semantics.md)（行为契约表 + IPC/生命周期契约） |
| **文档卓越性** | [review-doc-excellence.md](review-doc-excellence.md)（叙事结构 + 读者体验 + 教学深度） |
| **代码卓越性** | [review-code-excellence.md](review-code-excellence.md)（API 设计 + 表达力 + 性能 + 测试质量） |

> **2026-08-15 修复 D-P1-8（加载模块 ≠ 工作量）**：Profile 描述"加载模块"指**规则文件是否加载到 AI 上下文**，与实际 review 工作量**非线性相关**。例如 Profile C 加载全部 8 个模块，但若目标文档仅 100 行，实际工作量仅 5-10 分钟。AI 看到 Profile C 不能假设"工作量 = 全模块加载"；实际工作量取决于：(a) 目标文档大小；(b) 模式（C/H/I/J/K）；(c) 已发现的 issue 数量（参考 Step 7.1 收敛成本评估）。

---

## 任务组合

### Profile A：文档 Review（专注文档质量）

**加载模块**：核心 + 文档检查清单 + 错误模式

**适用场景**：
- 新文档审阅
- 文档更新后的复查
- 专注检查文档与 Minix3 源码的一致性

**执行流程**：
1. 按 [review.md §Review 启动：范围声明](review.md) 声明范围（模式 B）
2. 阅读 [review.md](review.md) 核心原则
3. 按 [review-doc-checklist.md](review-doc-checklist.md) 逐项检查
4. 对照 [review-patterns.md](review-patterns.md) 识别错误模式
5. 按 [review.md §AI Review 输出模板](review.md#ai-review-输出模板) 输出结果

---

### Profile B：代码 Review（专注代码质量）

**加载模块**：核心 + 代码检查清单 + 错误模式

**适用场景**：
- Rust 代码 PR 审阅
- 重构后的代码复查
- 专注检查 Rewrite 质量和类型安全

**执行流程**：
1. 按 [review.md §Review 启动：范围声明](review.md) 声明范围
2. 阅读 [review.md](review.md) 核心原则
3. 按 [review-code-checklist.md](review-code-checklist.md) 逐项检查（含 §2.5 trait 设计质量评估）
4. 对照 [review-patterns.md](review-patterns.md) 识别错误模式（含模式 24 不必要的 trait 抽象）
5. 按 [review.md §AI Review 输出模板](review.md#ai-review-输出模板) 输出结果

---

### Profile C：完整 Review（文档 + 代码）

**加载模块**：全部模块

**适用场景**：
- 里程碑评审
- 模块完成后的全面检查
- 需要同时验证文档和代码的一致性

**执行流程**：
1. 按 [review.md §Review 启动：范围声明](review.md) 声明范围（模式 C）
2. 阅读 [review.md](review.md) 核心原则
3. 按 [review-process.md](review-process.md) Step 0-7 执行强制步骤（Step 0 design 预检强制）
4. 文档部分：使用 [review-doc-checklist.md](review-doc-checklist.md) + [review-patterns.md](review-patterns.md)
5. 代码部分：使用 [review-code-checklist.md](review-code-checklist.md)（含 §2.5 trait 设计质量评估）
   + [review-patterns.md](review-patterns.md)（含模式 24 不必要的 trait 抽象）
6. 按 [review.md §AI Review 输出模板](review.md#ai-review-输出模板) 输出结果

---

### Profile D：快速 Review（口诀扫描）

**加载模块**：核心

**适用场景**：
- 日常快速检查
- 初步筛选明显问题
- 时间有限的场景

**执行流程**：
1. 按 [review.md §Review 启动：范围声明](review.md) 声明范围（模式 B 或 C）
2. 阅读 [review.md](review.md) 核心原则
3. 使用 [review.md §快速判断口诀](review.md#快速判断口诀) 逐项自问
4. 仅输出发现的 P0 级别问题

> Profile D 只裁剪内容检查，不裁剪流程安全门：Step 0 预检、Gate 0（制品完整性）、Gate H（design 门控）仍然必须执行；未通过时只能输出 DRAFT，不能标记 CONVERGED。
>
> **2026-08-15 修复 A-P0-3（明确 Profile D 必检的 Gate）**：Profile D 必检以下 Blocker Gates，其余 Gate 按需抽样：
> - ✅ **必须执行**：Step 0 预检（design/outline 状态 + 4 条 `ls` 命令）；Gate 0（scan.md 制品完整性）；Gate H（design 门控 H.1-H.6）
> - ⚠️ **必须执行**（即便快速模式）：Gate D §0 P0 必检清单 5 项（test 存在 / trait 实现 / 函数位置 / 算法非 stub / 签名一致）—— 这 5 项是基本正确性，不应被"快速"裁剪
> - ❌ **可裁剪**：Gate A（覆盖率穷举，Profile D 跳 SYMBOLS.md 全量）、Gate B（Top 5 行为契约表完整版）、Gate C（5 元规则 Precision Check 全量）、Gate E（§5 测试全量 grep）、Gate G（VERIFY-CHECK.md 跨 agent 验证）—— 这些是深度模式才要求的
> - **判定**：Profile D 未通过 Step 0 预检 / Gate 0 / Gate D / Gate H → 只能 DRAFT，禁止 CONVERGED

---

### Profile E：跨文档联动检查

**加载模块**：核心 + 错误模式（仅跨文档部分）

**适用场景**：
- 检查同一目录下多篇文档的一致性
- 验证文档间引用是否正确
- 检查共享概念是否有重复定义或矛盾

**执行流程**：
1. 阅读 [review.md](review.md) 核心原则
2. 使用 [review-patterns.md](review-patterns.md) §二、跨文档联动错误模式 逐项检查
3. 使用 [review-patterns.md](review-patterns.md) §三、检查范围与验证方法 中的命令验证
4. 输出发现的重复定义、矛盾、遗漏引用等问题

---

### Profile F：链路验证 Review（专注章节间推导完整性）

**加载模块**：核心 + 文档检查清单（§2.9 + §2.10）+ 代码检查清单（§1 最后 3 项）

**适用场景**：
- 文档初稿完成后，验证章节间推导链路
- 代码实现完成后，验证代码与文档设计的一致性
- 发现代码与设计脱节时的专项检查
- review 完成后代码未修改时的根因排查
- 对已经过一次 Review 的文档做第二轮专项检查

**执行流程**：
1. 列出 Ch3 的所有设计决策
2. 逐一验证每个决策是否有 Ch1&2 的依据（见 [review-doc-checklist.md §2.9](review-doc-checklist.md#29-设计决策质量检查ch3-专项强制不可跳过)）
3. 逐一验证 Ch4 是否实现了 Ch3 的设计（见 [review-doc-checklist.md §2.10](review-doc-checklist.md#210-章节链路验证强制不可跳过)）
4. 逐一验证测试章节是否覆盖了 Ch3+Ch4
5. 逐一验证代码是否与 Ch4 一致
6. 输出所有链路断裂点，并生成修改项（按 [review-process.md Step 6](review-process.md) 格式）

---

### Profile G：局部 Review（仅 Ch1&2）

**加载模块**：核心 + 文档检查清单（§2.1, §2.2, §2.3, §2.8）

**适用场景**：
- 用户指定「只 review 第一章和第二章」
- 概念准确性和源码覆盖的快速验证
- 文档写作阶段，先确保 Ch1&2 描写正确

**执行流程**：
1. 按 [review.md §Review 启动：范围声明](review.md) 声明范围（模式 A）
2. 阅读 [review.md](review.md) 核心原则
3. 按 [review-process.md](review-process.md) Step 1（源码定位）+ Step 3（一致性检查）执行
4. **不执行**：Step 2（差异提取）、Step 2.5（链路验证）、Step 4（跨文档）、Step 6（修改项）
5. 按 [review.md §AI Review 输出模板](review.md#ai-review-输出模板) 输出结果
   - 问题清单仅含概念/引用/覆盖问题
   - 链路验证结果标注"不适用（局部 Review）"
   - 跨文档检查标注"不适用（局部 Review）"

---

### Profile H-N：分阶段完整 Review（解决注意力过载问题）

> **背景**：一次完整 Review 需要检查 150+ 条规则。AI 在单次对话中容易"凭印象扫描"，漏掉不擅长的维度。
> 分阶段 Review 将一次完整 Review 拆成 3~4 次独立对话，每次只加载部分规则，确保每个维度都得到充分检查。

#### Profile H：阶段 1 — Ch1&2 准确性验证

**加载模块**：核心 + 文档检查清单（§2.1, §2.2, §2.3, §2.5, §2.7, §2.8）+ 错误模式（模式1~9）

**检查内容**（约 40 条规则）：
- 概念准确性（每个概念 grep 验证）
- C 代码引用验证（每个引用读源码对比）
- 数据结构覆盖（每个结构体的每个字段）
- 架构演进说明（32位 vs 64位差异标注）
- 图表质量（对齐、必要性）
- C 源码覆盖完整性（每个 .c 文件的每个函数/结构体/宏）

**输入**：目标文档 + C 源码
**输出**：P0 概念错误 + P0 引用错误 + P0 覆盖缺口 + P1 架构差异未标注

**预计耗时**：20~60 分钟（取决于文档规模）

#### Profile I：阶段 2 — Ch3&4 设计质量验证

**加载模块**：核心 + 文档检查清单（§2.4, §2.9, §2.10）+ 错误模式（模式10~13）+ 阶段 1 的输出

**检查内容**（约 30 条规则）：
- 文档与 Rust 代码一致性（函数签名、类型定义）
- 设计决策质量（可追溯性、场景覆盖、替代方案）
- 章节链路验证（Ch3→Ch1&2、Ch4→Ch3、测试→Ch3+Ch4）

**输入**：目标文档 + 阶段 1 输出（已知 Ch1&2 准确）+ Rust 代码
**输出**：P0 场景遗漏 + P1 链路断裂 + P1 设计缺乏依据

**预计耗时**：15~40 分钟

#### Profile J：阶段 3 — Rust 代码质量验证

**加载模块**：核心 + 代码检查清单（§1~15）+ 错误模式（模式16~25）+ 阶段 2 的输出

**检查内容**（约 70 条规则）：
- Rewrite 质量（newtype、enum、Result、所有权）
- 硬件抽象（trait 多态性、机制 vs 策略分离）
- 类型安全（typestate、unsafe、aliasing）
- 执行模型（单线程假设、Send/Sync）
- no_std 约束
- 设计-代码一致性

**输入**：Rust 代码 + 阶段 2 输出（已知 Ch3 设计正确）
**输出**：P0 UB/语义偏移 + P1 类型安全问题 + P1 trait 设计问题

**预计耗时**：20~50 分钟

#### Profile K：阶段 4 — 跨文档联动 + 可读性

**加载模块**：核心 + 文档检查清单（§2.6, §3.1~3.4）+ 错误模式（模式 A~C, 跨文档部分）+ 阶段 1~3 的输出

**检查内容**（约 25 条规则）：
- 跨文档重复定义、矛盾、遗漏引用
- 文本流畅性、逻辑组织、冗余控制、读者体验

**输入**：同目录所有 .md 文件 + 阶段 1~3 输出
**输出**：P2 可读性问题 + P1 跨文档矛盾/重复

**预计耗时**：10~30 分钟

#### 分阶段 Review 使用指南

**推荐流程**：
```
阶段 1 (Profile H) ──→ 阶段 2 (Profile I) ──→ 阶段 3 (Profile J) ──→ 阶段 4 (Profile K)
    Ch1&2 准确性           Ch3&4 设计质量          Rust 代码质量           跨文档+可读性
```

**每次对话的启动指令示例**：
```
阶段 1：「按 Profile H 对 xxx.md 做 Ch1&2 准确性验证」
阶段 2：「按 Profile I 对 xxx.md 做 Ch3&4 设计质量验证。阶段 1 的结论是：[粘贴上一轮输出摘要]」
阶段 3：「按 Profile J 对 xxx.rs 做代码质量验证。阶段 2 的结论是：[粘贴上一轮输出摘要]」
阶段 4：「按 Profile K 对 xxx.md 做跨文档联动和可读性检查。前几轮的结论是：[粘贴摘要]」
```

**vs 单次完整 Review (Profile C) 的对比**：

| 维度 | Profile C（单次） | 分阶段（H→I→J→K） |
|------|------------------|-------------------|
| 总耗时 | 40~120 分钟 | 65~180 分钟（总耗时更长） |
| 完备性 | 中（注意力分散，容易漏） | 高（每阶段专注 25~40 条规则） |
| 可验证性 | 低（你不知道哪些维度被跳过） | 高（每阶段输出独立可检查） |
| 人工介入 | 需要手动迭代 2~3 次 | 每阶段独立，不需要迭代 |
| 适用场景 | 简单文档（< 300 行） | 复杂文档（> 300 行）或关键模块 |

**选择建议**：
- 文档 < 300 行 + 非关键模块 → Profile C（单次完整 Review）
- 文档 > 300 行 或 关键模块 → 分阶段 Review（H→I→J→K）
- 如果单次 Review 后发现需要人工迭代 → 下次直接用分阶段 Review

---

### Profile O：卓越性专项 Review（追求教科书级质量）

**加载模块**：核心 + 文档卓越性 + 代码卓越性 + 错误模式（卓越性部分）

**适用场景**：
- 正确性 Review 通过后，追求文档教科书化、代码 redox 级质量
- 模块即将作为参考实现对外发布
- 关键模块需要高质量文档吸引贡献者
- 正确性已验证，需要提升表达力和可维护性

**前置条件**：建议先完成 Profile C 或分阶段 Review（H→I→J→K），确保正确性已通过。

**执行流程**：
1. 按 [review.md §Review 启动：范围声明](review.md) 声明范围（模式 B 或 C）
2. 阅读 [review.md](review.md) 核心原则
3. **文档卓越性**：按 [review-doc-excellence.md](review-doc-excellence.md) 逐项检查
   - §4.1 叙事结构卓越性（叙事弧、动机、过渡、层次、聚焦）
   - §4.2 读者体验卓越性（前置知识、可读性、示例、抽象层次）
   - §4.3 教学深度卓越性（设计取舍、边界、失败模式、历史背景）
   - §4.4 可维护性卓越性（一致性、可演进、文档债）
4. **代码卓越性**：按 [review-code-excellence.md](review-code-excellence.md) 逐项检查
   - §16 API 设计卓越性（难误用、惯用法、错误类型、层次、最小接口）
   - §17 表达力卓越性（类型系统、编译时保证、命名、迭代器、模式匹配）
   - §18 性能卓越性（零成本抽象、分配、缓存、批处理）
   - §19 代码即文档（自解释、注释价值、unsafe 可见性）
   - §20 可测试性卓越性（依赖注入、纯函数、副作用隔离）
   - §21 测试质量卓越性（L1 对偶、L2 契约、L3 doctest、属性测试）
5. 对照 [review-patterns.md](review-patterns.md) §七、卓越性错误模式（模式 41-47）
6. 按 [review.md §AI Review 输出模板](review.md#ai-review-输出模板) 输出结果
   - 问题优先级以 P1/P2 为主（正确性问题已在前期 Review 解决）
   - 每个问题提供"现状→目标→改进路径"三段式建议

**输出特点**：
- 不再重复正确性检查（假设已通过）
- 聚焦"如何从合格到优秀"
- 每个建议附"为什么这样更好"的理由
- 提供可执行的改进示例（不只是说"应该改"，而是给出改后的样子）

---

### Profile P：覆盖率专项 Review（穷举式覆盖验证）

**加载模块**：核心 + 核心语义 + 执行流程（Step 1.5）+ 错误模式

**适用场景**：
- 怀疑文档/代码覆盖不完整（漏函数、漏结构体、漏宏）
- 跨轮次 Review 后需要确认覆盖率无回退
- 模块完成后做最终覆盖率验收
- 新增 C 源码后验证 Rust 实现是否同步

**执行流程**：
1. 按 [review.md §Review 启动：范围声明](review.md) 声明范围
2. 阅读 [review.md](review.md) 核心原则
3. **机器生成 SYMBOLS.md 骨架**：
   ```bash
   python3 tools/coverage-extract/coverage-extract.py {module} {doc_dir} [--rust-dir {rust_dir}]
   ```
4. **AI 补充语义判断**（按 [review-process.md §Step 1.5](review-process.md) 执行）：
   - Rust 对应关系确认（名称匹配 ≠ 语义对应）
   - 架构演进标记（ARCH: 不需要 + 理由）
   - 语义归属判定（以功能语义为准）
   - 行为契约表（核心函数：输入/输出/副作用/错误码/时序）
   - 测试覆盖补充（L1对偶/L2契约/L3 doctest）
5. **核心语义对齐**：按 [review-core-semantics.md](review-core-semantics.md) 对核心函数生成行为契约表
6. 输出覆盖率报告：
   - C 符号总数 / 文档覆盖 / Rust 覆盖 / 完全缺口 / 架构演进
   - P0 缺口清单（在语义范围内但无文档无 Rust）
   - ARCH 标记清单（不需要 Rust 对应 + 理由）
   - 行为契约表（核心函数）

**输出特点**：
- 以表格为主，量化覆盖率
- 每个缺口标注 evidence [DIRECT/MEDIUM/INFERRED]
- ARCH 标记必须附理由（不能只标"不需要"）
- 行为契约表覆盖核心函数的输入/输出/副作用/错误码/时序

---

### Profile R：设计优先模式 Review

> **核心定位**：在 Minix-RS Rust 重写场景下，design 本身是核心交付物。design 缺失由所有 review 模式在 Step 0.3 内嵌生成；design 已存在但错误时，使用本 Profile 先修正 design，再 review 实现。

**适用场景**：
- Rust design 严重 outdated 或已存在但错误
- review 中发现 P0-design-missing/wrong
- 完整重写前的 design-first 阶段
- 用户明确指定"先看 design"

**Profile = Profile D（快速） + 强制执行以下检查**：

| 维度 | 强制项 | 说明 |
|------|--------|------|
| **Step 1.6 设计对齐检查** | 强制 | design ↔ code 一致性矩阵 + Minix3 对齐 |
| **Gate H design 门控** | 强制 | H.1-H.6 全部 grep 验证（含 outline ↔ doc 对齐） |
| **Design Feedback §8** | 强制 | scan.md §8 Design Feedback 必填（design-missing/wrong/improvable/divergence）|
| **Review 中断协议** | 强制 | 检测到 design-wrong 时进入 IN_DESIGN；design 缺失走 Step 0.3，不因缺失中断 |
| **Architecture Evolution 维度 §2.0** | 强制 | 检查演进史 + 现代硬件模型 + Rust 抽象方向 |

**加载 Skill**：
- `review-process-skill`（必须，含 §〇 设计优先模式 + Step 1.6 + Gate H + Review 中断协议）
- `review-patterns-skill`（必须，含模式 63 Design-Missing + 模式 64 开发文档味 + 模式 65 Translate 倾向）
- `review-doc-skill` 或 `review-code-skill`（按对象选其一）
- `review-core-semantics-skill`（如涉及核心语义）

**执行流程**（按 [review-process.md §〇 设计优先模式](review-process.md#设计优先模式design-first)）：
1. **Step 0**：声明 design 状态（错误/可改进；缺失由 Step 0.3 处理）
2. **Step 0.5**：design 评审（如错误）；缺失快照由 Step 0.3 先生成
3. **Step 1.5**：覆盖率穷举（验证 design 是否覆盖所有 C 概念）
4. **Step 1.6**：design ↔ code 一致性检查
5. **Step 2**：design vs Minix3 本质对比

**跳过**：Step 2.5 / 3 / 3.5 / 4 / 4.5 / 6 / 7

**输出**：
- scan.md（含 Design Feedback §8 + 行为决策矩阵）
- IN_DESIGN.md（如中断，参见 [review-process.md §一.附录 A](review-process.md)）
- design.md（非 bagging，如缺失或需修正，参见 [review-process.md §Step 0.3 缺失即生成](review-process.md)）
- design-final.md（bagging 场景，多 AI 评审合并后的定稿）
- 不输出代码修改项（design 修复后才能决定）

**与 Profile C（深度 review）的关系**：
- Profile R 是 Profile C 的**前置**阶段
- design 修复后可转入 Profile C
- 不替代日常 review，而是日常 review 的前置阶段

**与实施验证专项（review-implementation-skill）的关系**：
- 实施验证专项用于 design → code 实施过程验证
- Profile R 用于 design-wrong → 修正 design；缺失快照由所有模式的 Step 0.3 生成
- 两者串联：Profile R 先生成 design，实施验证专项验证实施

**触发条件**（自动 + 手动）：
- 自动触发：Step 1.6.2 一致性 < 80% / Step 1.6.3 出现 P0-design-wrong；快照缺失由 Step 0.3 内嵌处理，不切换 Profile
- 手动触发：用户明确指定"先看 design" 或 AI 识别到"开发文档味"严重

**详见**：
- [review-process.md §〇 设计优先模式](review-process.md#设计优先模式design-first)
- [review-process.md §Gate H design 门控](review-process.md#gate-h-design-门控)
- [review-process.md §一.附录 A Review 中断协议](review-process.md)
- [review-patterns.md §模式 63 Design-Missing](review-patterns.md)
- [review.md §Design First 原则](review.md)

---

### Profile AG：自动生成文档 Review（NEW 2026-08-15, 修复 B-P2-4）

> **核心定位**：处理 `cargo doc` 等工具自动生成的文档（target/doc/、comment-derived docs）。此类文档**不是人工设计**，**不应按 Profile A/C 流程 review**，但需做基础正确性检查。

**适用场景**：
- 用户要求 review `target/doc/` 下的 HTML 文档
- 自动生成的 API reference（如 `///` doc comment 生成的文档）
- 第三方依赖文档（如 `minix_types` 的 generated rustdoc）

**加载模块**：仅核心 + 错误模式（基础部分）

**检查清单**（裁剪后）：
1. **基本可读性**：HTML/Markdown 格式是否正确，链接是否失效
2. **代码示例可用性**：`cargo doc --document-private-items` 生成的代码示例是否仍可编译
3. **术语一致性**：跨多个 auto-gen 文档的术语是否一致（grep 验证）
4. **链接可达性**：文档内 `[[link]]` / `[Type]` 是否指向真实类型

**跳过**：
- ❌ Step 0.5 structure.md 评审（auto-gen 文档无叙事结构）
- ❌ Step 1.5 覆盖率穷举（auto-gen 文档覆盖率无意义）
- ❌ Step 1.6 design 对齐（auto-gen 文档无 design 来源）
- ❌ Step 2 差异提取（auto-gen 文档与实现无差异，差异即 bug）
- ❌ Gate A-D 全量检查（仅检查基本正确性）

**输出**：
- 仅输出 P0（链接失效 / 代码示例编译失败）
- P1 仅记录"建议人工撰写对应设计文档"

**触发条件**：
- 文件路径匹配 `target/doc/**/*.html` 或 `.rustdoc` 后缀
- 用户显式声明"review cargo doc 输出"

---

## 选择建议

| 场景 | 推荐 Profile | 原因 |
|------|-------------|------|
| 新文档初稿审阅 | A | 专注文档质量，不需要代码检查 |
| 代码 PR 审阅 | B | 专注代码质量，不需要文档检查 |
| 简单文档完整验收 (< 300行) | C | 一次对话可覆盖，注意力够用 |
| 复杂文档/关键模块完整验收 | **H→I→J→K（分阶段）** | 避免注意力过载，每阶段专注 25~40 条规则 |
| 日常快速扫描 | D | 时间有限，用口诀快速发现问题 |
| 文档集一致性检查 | E | 专注跨文档联动问题 |
| 设计-代码链路验证 | F | 专注章节间推导完整性和代码-设计一致性 |
| 仅验证 Ch1&2 准确性和覆盖 | G | 轻量级，不做设计/链路层面检查 |
| 正确性通过后追求卓越质量 | **O（卓越性专项）** | 聚焦文档教科书化 + 代码 redox 级，不重复正确性检查 |
| 覆盖率验收/缺口排查 | **P（覆盖率专项）** | 机器穷举 + AI 语义判断，量化覆盖率 |
| 用户只说「review xxx.md」 | A（默认） | 默认文档 Review，发现 .rs 文件时提示升级到 C 或分阶段 |
