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

## 选择建议

| 场景 | 推荐 Profile | 原因 |
|------|-------------|------|
| 新文档初稿审阅 | A | 专注文档质量，不需要代码检查 |
| 代码 PR 审阅 | B | 专注代码质量，不需要文档检查 |
| 模块完成验收 | C | 需要全面检查文档和代码 |
| 日常快速扫描 | D | 时间有限，用口诀快速发现问题 |
| 文档集一致性检查 | E | 专注跨文档联动问题 |
| 设计-代码链路验证 | F | 专注章节间推导完整性和代码-设计一致性 |
| 仅验证 Ch1&2 准确性和覆盖 | G | 轻量级，不做设计/链路层面检查 |
| 用户只说「review xxx.md」 | A（默认） | 默认文档 Review，发现 .rs 文件时提示升级到 C |
