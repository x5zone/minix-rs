# Review 任务组合配置

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
3. 按 [review-process.md](review-process.md) Step 1-6 执行强制步骤
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
2. 逐一验证每个决策是否有 Ch1&2 的依据（见 [review-doc-checklist.md §2.9](review-doc-checklist.md#29-设计决策质量检查ch3-专项)）
3. 逐一验证 Ch4 是否实现了 Ch3 的设计（见 [review-doc-checklist.md §2.10](review-doc-checklist.md#210-章节链路验证)）
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

**加载模块**：核心 + 代码检查清单（§1~13）+ 错误模式（模式15~24）+ 阶段 2 的输出

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
| 用户只说「review xxx.md」 | A（默认） | 默认文档 Review，发现 .rs 文件时提示升级到 C 或分阶段 |
