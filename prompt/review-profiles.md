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
| **执行流程** | [review-process.md](review-process.md)（强制步骤 + 输出格式 + 工具命令 + 口诀） |

---

## 任务组合

### Profile A：文档 Review（专注文档质量）

**加载模块**：核心 + 文档检查清单 + 错误模式

**适用场景**：
- 新文档审阅
- 文档更新后的复查
- 专注检查文档与 Minix3 源码的一致性

**执行流程**：
1. 阅读 [review.md](review.md) 核心原则
2. 按 [review-doc-checklist.md](review-doc-checklist.md) 逐项检查
3. 对照 [review-patterns.md](review-patterns.md) 识别错误模式
4. 按 [review-process.md](review-process.md) §输出格式 输出结果

---

### Profile B：代码 Review（专注代码质量）

**加载模块**：核心 + 代码检查清单 + 错误模式

**适用场景**：
- Rust 代码 PR 审阅
- 重构后的代码复查
- 专注检查 Rewrite 质量和类型安全

**执行流程**：
1. 阅读 [review.md](review.md) 核心原则
2. 按 [review-code-checklist.md](review-code-checklist.md) 逐项检查
3. 对照 [review-patterns.md](review-patterns.md) 识别错误模式
4. 按 [review-process.md](review-process.md) §输出格式 输出结果

---

### Profile C：完整 Review（文档 + 代码）

**加载模块**：全部模块

**适用场景**：
- 里程碑评审
- 模块完成后的全面检查
- 需要同时验证文档和代码的一致性

**执行流程**：
1. 阅读 [review.md](review.md) 核心原则
2. 按 [review-process.md](review-process.md) Step 1-5 执行强制步骤
3. 文档部分：使用 [review-doc-checklist.md](review-doc-checklist.md) + [review-patterns.md](review-patterns.md)
4. 代码部分：使用 [review-code-checklist.md](review-code-checklist.md) + [review-patterns.md](review-patterns.md)
5. 按 [review-process.md](review-process.md) §输出格式 输出结果

---

### Profile D：快速 Review（口诀扫描）

**加载模块**：核心 + 执行流程（仅口诀部分）

**适用场景**：
- 日常快速检查
- 初步筛选明显问题
- 时间有限的场景

**执行流程**：
1. 阅读 [review.md](review.md) 核心原则
2. 使用 [review-process.md](review-process.md) §Review 快速判断口诀 逐项自问
3. 仅输出发现的 P0 级别问题

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

**加载模块**：核心 + 文档检查清单（§2.9 + §2.10）+ 代码检查清单（§13）

**适用场景**：
- 文档初稿完成后，验证章节间推导链路
- 代码实现完成后，验证代码与文档设计的一致性
- 发现代码与设计脱节时的专项检查
- review 完成后代码未修改时的根因排查

**执行流程**：
1. 列出 Ch3 的所有设计决策
2. 逐一验证每个决策是否有 Ch1&2 的依据
3. 逐一验证 Ch4 是否实现了 Ch3 的设计
4. 逐一验证测试章节是否覆盖了 Ch3+Ch4
5. 逐一验证代码是否与 Ch4 一致
6. 输出所有链路断裂点，并生成修改项（按 Step 6 格式）

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
