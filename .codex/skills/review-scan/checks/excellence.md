# excellence: 卓越性检查（文档 + 代码）

> 本文件定义卓越性检查，在正确性 gate 通过后执行。追求"更好"而非"正确"。
> **详见**：[review-doc-excellence.md](../../../../prompt/review-rules/review-doc-excellence.md) + [review-code-excellence.md](../../../../prompt/review-rules/review-code-excellence.md)
> **强制规则**：每个判定标注 evidence [DIRECT/MEDIUM/INFERRED]。
> **⛔ Step 0 硬阻断前置（NEW 2026-07-16）**：进入本文件任何卓越性检查前，必须已通过 [SKILL.md Phase 1 §Step 0 硬阻断预检](../SKILL.md) + [process.md §Step 0 硬阻断规则](process.md)。

---

## 通用规则

1. **正确性优先**：卓越性检查仅在正确性 gate（doc.md + code.md + coverage.md + patterns.md）通过后执行。
2. **不降级**：卓越性问题不降级为正确性问题。卓越性 P1 ≠ 正确性 P1。
3. **Evidence 分级**：[DIRECT] / [MEDIUM] / [INFERRED]。
4. **零输出禁令**：0 个问题也必须写"Checked N items, found 0 issues"。

---

## 一、文档卓越性（§4.1-4.3）

> 详见 [review-doc-excellence.md](../../../../prompt/review-rules/review-doc-excellence.md)

### §4.1 叙事结构卓越性

**Execute**:
1. 检查文档是否有清晰的叙事弧：问题→分析→决策→实现
2. 检查每章是否有"为什么"的动机说明，而非仅"是什么"
3. 检查章节间是否有逻辑过渡，而非简单堆砌

**Output**:
| 章节 | 叙事弧完整? | 动机说明? | 逻辑过渡? | Issue |
|------|------------|----------|----------|-------|

### §4.2 读者体验卓越性

**Execute**:
1. 检查目标读者是否明确（内核开发者？Rust 学习者？）
2. 检查术语首次出现是否有定义
3. 检查复杂概念是否有类比或图解
4. 检查前置知识假设是否显式声明

**Output**:
| 位置 | 读者体验问题 | P? | Suggested Fix |
|------|------------|----|---------------|

### §4.3 教学深度卓越性

**Execute**:
1. 检查是否解释了设计取舍（为什么选 A 不选 B）
2. 检查是否讨论了边界情况和失败模式
3. 检查是否连接到更大的系统图景
4. 检查是否提供了可运行的示例

**Output**:
| 位置 | 教学深度问题 | P? | Suggested Fix |
|------|------------|----|---------------|

### §4.4 可维护性卓越性

> **Purpose**: 文档应便于长期维护、变更追踪与外部引用。
> **详见**：[review-doc-excellence.md §五 §4.4](../../../../prompt/review-rules/review-doc-excellence.md)

**Execute**:
1. 检查是否有变更日志段落
2. 检查 TODO 标记：`rg "TODO|FIXME|XXX" FILE.md -n`
3. 检查外部引用是否有链接
4. 检查术语和命名的一致性

**Output**:
| 位置 | 可维护性问题 | 卓越性 P? | Suggested Fix |
|------|------------|----------|---------------|

### §4.5 概念教学卓越性（Ch1 专项）

> **Purpose**: 概念章节（Ch1）的骨架必须在文档中可见，让读者能快速建立心智模型；同时从 CPU/系统视角而非 OS 代码角度组织。
> **详见**：[review-doc-excellence.md §六 §4.5](../../../../prompt/review-rules/review-doc-excellence.md) — Ch1 教学卓越性专项。
> 来源：Qwen 提案

**Execute**（6 项）：
1. **CPU/系统视角**：抽取 Ch1 开篇第一句，判定主语是 CPU/系统 还是 OS 代码/函数名。后者 → 降级（模式 51）。
2. **统一抽象先行**：多架构文档是否先给统一框架（如"CPU 三问"）再分架构展开。无 → 降级（模式 53）。
3. **心智模型完整**：进入类机制（trap/syscall/IPC）是否双向闭环（entry + return）。只单向 → 降级（模式 52）。
4. **认知负荷**：示例是否最小化（不引入与核心机制无关的细节）。引入额外问题 → 降级（模式 55/57）。
5. **预期管理**：Ch1 是否有"本章不讲什么"声明。无 → 降级。
6. **概念收束**：Ch1 章末是否用核心框架收束并预告后续章节。无 → 降级。

**Output**:
| # | 骨架元素 | 存在? | 位置 | Issue | 关联模式 |
|---|---------|-------|------|-------|---------|
| 1 | 主题思想（CPU 视角） | ✅/❌ | Ch1 §X | | 51 |
| 2 | 目标读者声明 | ✅/❌ | Ch1 §X | | - |
| 3 | 概念层次大纲 | ✅/❌ | Ch1 §X | | 51 |
| 4 | 核心概念清单 | ✅/❌ | Ch1 §X | | - |
| 5 | 统一抽象（多架构） | ✅/❌/N/A | Ch1 §X | | 53 |
| 6 | 双向闭环 | ✅/❌ | Ch1 §X | | 52 |
| 7 | 本章不讲什么（预期管理） | ✅/❌ | Ch1 §X | | - |
| 8 | 示例最小化（认知负荷） | ✅/❌ | Ch1 §X | | 55/57 |
| 9 | 框架收束（章末） | ✅/❌ | Ch1 §X | | - |

**卓越性标准**（与 [review-doc-excellence.md §六、§4.5 概念教学卓越性](../../../../prompt/review-rules/review-doc-excellence.md) 一致）：
- **A（优秀）**：CPU 视角 + 统一框架 + 心智完整 + 预期管理 + 框架收束
- **B（良好）**：概念驱动 + 统一抽象 + 心智基本完整
- **C（合格）**：概念驱动但缺统一抽象或心智不完整
- **D（不合格）**：实现驱动

---

## 二、代码卓越性（§15-20）

> 详见 [review-code-excellence.md](../../../../prompt/review-rules/review-code-excellence.md)
> **模式编号映射**：§15 = 模式 43（API 易误用），§16 表达力 ↔ 模式 43/47，§17 性能 ↔ 模式 47（全局依赖），§18 = 模式 45（冗余注释），§19 = 模式 46（副作用隐藏），§20 = 模式 44（错误类型）。详见 [patterns.md §五 卓越性模式（41-47）](patterns.md)。

### §15 API 设计卓越性

**Execute**:
1. 检查 API 是否难以误用（make wrong state unrepresentable）
2. 检查 API 是否符合 Rust 惯用法（builder pattern、where clause 等）
3. 检查错误类型是否精确（不滥用 `Box<dyn Error>`）
4. 检查 API 是否有清晰的层次（low-level / mid-level / high-level）

**Output**:
| 位置 | API 设计问题 | P? | Suggested Fix |
|------|------------|----|---------------|

### §16 表达力卓越性

**Execute**:
1. 检查是否充分利用类型系统（newtype、phantom、typestate）
2. 检查是否避免运行时检查（用编译时保证替代）
3. 检查命名是否表达意图（而非实现）
4. 检查是否利用迭代器/组合子替代显式循环

**Output**:
| 位置 | 表达力问题 | P? | Suggested Fix |
|------|----------|----|---------------|

### §17 性能卓越性

**Execute**:
1. 检查是否有不必要的分配/拷贝
2. 检查是否利用零成本抽象
3. 检查热路径是否有不必要的抽象层
4. 检查是否考虑了缓存友好性

**Output**:
| 位置 | 性能问题 | P? | Suggested Fix |
|------|---------|----|---------------|

### §18 代码即文档卓越性

**Execute**:
1. 检查代码是否自解释（类型名、函数名表达意图）
2. 检查注释是否解释"为什么"而非"是什么"
3. 检查是否有冗余注释（重复代码已表达的信息）
4. 检查模块文档是否提供使用示例

**Output**:
| 位置 | 代码即文档问题 | P? | Suggested Fix |
|------|-------------|----|---------------|

### §19 可测试性卓越性

**Execute**:
1. 检查代码是否易于测试（依赖注入、纯函数）
2. 检查是否有副作用隐藏在看似纯函数中
3. 检查测试是否覆盖了不变量（invariant）而非仅行为
4. 检查是否有 property-based test 的机会

**Output**:
| 位置 | 可测试性问题 | P? | Suggested Fix |
|------|------------|----|---------------|

### §20 测试质量卓越性

**Execute**:
1. 检查测试是否有意义（非"测试 1+1=2"）
2. 检查测试是否覆盖了错误路径和边界
3. 检查测试是否快速且确定（无 flaky test）
4. 检查测试命名是否表达意图

**测试三重标准**:
- L1 C-Rust 对偶测试：验证 Rust 实现与 C 行为一致
- L2 Trait 契约测试：验证 trait 实现满足契约
- L3 doctest：文档中的代码示例可运行

**Output**:
| 位置 | 测试质量问题 | P? | Suggested Fix |
|------|------------|----|---------------|

---

## Pass condition
- 文档卓越性：叙事弧完整、读者体验良好、教学深度充分
- 代码卓越性：API 难以误用、表达力强、性能合理、自解释、可测试、测试高质量
- 测试三重标准：L1/L2/L3 至少一项覆盖每个核心函数

**⛔ 卓越性问题不降级为正确性问题。卓越性 P1 是"可以更好"，正确性 P1 是"必须修复"。**

---

## Design-First 卓越性扩展

> 配合 [review-doc-excellence.md §4.3.5 + §4.4](../../../../prompt/review-rules/review-doc-excellence.md) + [review-code-excellence.md §15.5](../../../../prompt/review-rules/review-code-excellence.md) 使用。

### 文档卓越性扩展（4 项）

| 检查项 | 说明 |
|--------|------|
| §4.3.5 Design 视角教学深度 | 每个设计决策是否有 design 引用？是否先讲"为什么"再讲"怎么实现"？是否说明替代方案及拒绝理由？ |
| §4.4 文档组织合理性 | 13 项组织检查（核心概念完整性 / Ch1&2:Ch3&4 平衡 / 设计决策集中度 / 章节依赖单向 / 文档拆分判定 / 跨文档一致性 等） |

### 代码卓越性扩展（6 项）

§15.5 design-first API 设计原则：
1. 命名 — 与 design 一致
2. 参数 — 严格匹配 trait 方法签名
3. 错误 — Error 变体与 design 错误码策略一致
4. 可见性 — pub 接口与 design 公共 API 列表一致
5. unsafe — unsafe 边界与 design 安全论证一致
6. trait — trait 方法与 design trait 抽象一致

### 架构演进作为独立知识点维度（5 类）

1. FPU 上下文处理演进（如 `FpuState` newtype 替代裸 `u32`）
2. 中断/异常处理演进（如 APIC trait 抽象 PIC）
3. 分页模型演进（2→4 级页表）
4. 地址空间布局演进（32→64 位）
5. 启动链演进（多架构启动路径统一抽象）

详见 [review-excellence-skill.md](../../../../prompt/skill/review-excellence-skill.md)。
