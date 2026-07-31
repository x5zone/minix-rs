# Minix-RS Review 代码卓越性检查

> 本文件定义代码卓越性检查标准，追求"redox 级"代码质量。
> **前置条件**：正确性 gate（[review-code-checklist.md](review-code-checklist.md)）已通过。
> **强制规则**：每个判定标注 evidence [DIRECT/MEDIUM/INFERRED]。

---

## 一、卓越性 vs 正确性

| 维度 | 正确性（gate） | 卓越性（grade） |
|------|--------------|---------------|
| 目标 | 代码不错 | 代码好读、好维护、好扩展 |
| 判定 | P0/P1/P2 | 卓越性 P1/P2（不降级为正确性 P1） |
| 时机 | 先执行 | 正确性通过后执行 |
| 修复 | 必须修复 | 建议修复 |

**⛔ 卓越性问题不降级为正确性问题。卓越性 P1 是"可以更好"，正确性 P1 是"必须修复"。**

---

## 二、§15 API 设计卓越性

### 2.1 检查项

| 检查项 | 说明 | Evidence 级别 |
|--------|------|-------------|
| 难以误用 | make wrong state unrepresentable | [MEDIUM] |
| Rust 惯用法 | builder pattern、where clause 等 | [DIRECT] |
| 错误类型精确 | 不滥用 `Box<dyn Error>` | [DIRECT] |
| 层次清晰 | low/mid/high level API 分层 | [MEDIUM] |
| 最小接口 | 只暴露必要的 API | [DIRECT] |

### 2.2 Execute

1. 检查 API 是否难以误用：
   - 是否有"无效状态可表达"的问题？（如 `struct { is_init: bool, data: Option<T> }` 可合并为 `enum { Init(T), Uninit }`）
   - 是否有"调用顺序依赖"未用 typestate 表达？

2. 检查错误类型：
   ```bash
   rg "Box<dyn Error|Box<dyn std::error::Error>" FILE.rs -n
   ```
   → 滥用 `Box<dyn Error>` = 卓越性 P1

3. 检查 API 层次：
   - low-level：直接操作硬件/内存
   - mid-level：OS 语义（alloc/map）
   - high-level：业务逻辑

4. 检查 pub 接口是否最小化。

### 2.3 Output

| 位置 | API 设计问题 | 卓越性 P? | Suggested Fix |
|------|------------|----------|---------------|

### §15.5 design-first API 设计原则

> **核心立场**：API 设计应优先考虑 design 而非习惯。如果 API 与 design 偏离但与 Rust 习惯一致，需要 design Refactor（修 design）而非 code Refactor（迁就习惯）。

| 维度 | 习惯驱动 ❌ | design-first 驱动 ✅ |
|------|---------|------------------|
| 命名 | 与 Rust 标准库一致 | 与 design 中的概念一致（如 `BootProcArch::load_vm_elf` 直接对应 design §3.4）|
| 参数 | 传 `Option<T>` 表示可选 | 用 typestate 表达状态机（如 `Uninit → Init` 阶段不暴露 `Option`）|
| 错误 | 用 `Box<dyn Error>` | 用 design 定义的 `VmError`（具体错误类型，含语义）|
| 可见性 | `pub` 暴露方便 | 仅暴露 design 中的 public trait（如 `ProtectionArch` 对外，`X8664Protection` 私有）|
| Unsafe 边界 | 必要时裸写 `unsafe` | 用 safe 包装（newtype 隐藏 unsafe，如 `PhysAddr(VirtAddr)` 强制边界检查）|
| Trait 实现 | 倾向 `dyn` + async-trait | 倾向 `impl Trait` + 泛型（编译期单态，更具体约束）|

**判定**：
- ❌ API 与 design 偏离但与 Rust 习惯一致 → **P1-design-deviation**（强制 design 优先）
- ⚠️ API 与 design 偏离但未找到 design 依据 → **P0-design-missing**（先补 design 再评 API）
- ✅ API 完全反映 design 决策 → A 级 API 卓越

**示例（正反对比）**：
```rust
// ❌ 习惯驱动：用 enum 暴露类型（违反 design.md §4.1 要求）
pub enum PlatformDescriptorPtr { /* 暴露具体类型 */ }

// ✅ design-first：用 trait object（符合 design §4.1）
pub trait PlatformDesc { /* design 规定的接口 */ }
pub fn get_platform() -> &'static dyn PlatformDesc { /* ... */ }
```

**详见**：[review.md §Design First 原则](review.md) + [review-patterns.md §模式 23 pub 滥用](review-patterns.md) + [review-patterns.md §模式 24 类型安全过度](review-patterns.md) + [review-patterns.md §模式 65 Translate 倾向](review-patterns.md)。

---

## 三、§16 表达力卓越性

### 3.1 检查项

| 检查项 | 说明 | Evidence 级别 |
|--------|------|-------------|
| 类型系统利用 | newtype、phantom、typestate | [DIRECT] |
| 编译时保证 | 用编译时替代运行时检查 | [MEDIUM] |
| 命名表达意图 | 命名表达"做什么"而非"怎么做" | [MEDIUM] |
| 迭代器/组合子 | 用迭代器替代显式循环 | [DIRECT] |
| 模式匹配 | 用 match 替代 if-else 链 | [DIRECT] |

### 3.2 Execute

1. 检查 newtype 使用：
   ```bash
   rg "struct \w+\(\w+\);" FILE.rs -n
   ```
   → 裸整数类型（如 `u64` 用于地址）应有 newtype

2. 检查运行时检查可否改为编译时：
   ```bash
   rg "if.*== 0|if.*is_none|panic!" FILE.rs -n
   ```
   → 可用 typestate 替代的 = 卓越性 P1

3. 检查显式循环：
   ```bash
   rg "for .* in |while " FILE.rs -n
   ```
   → 可用迭代器/组合子替代的 = 卓越性 P2

4. 检查 if-else 链：
   ```bash
   rg "else if" FILE.rs -n
   ```
   → 可用 match 替代的 = 卓越性 P2

### 3.3 Output

| 位置 | 表达力问题 | 卓越性 P? | Suggested Fix |
|------|----------|----------|---------------|

---

## 四、§17 性能卓越性

### 4.1 检查项

| 检查项 | 说明 | Evidence 级别 |
|--------|------|-------------|
| 无不必要分配 | 避免热路径分配 | [MEDIUM] |
| 零成本抽象 | trait/泛型无运行时开销 | [INFERRED] |
| 热路径无抽象 | 热路径避免过度抽象 | [INFERRED] |
| 缓存友好 | 数据布局考虑缓存行 | [INFERRED] |
| 批量操作 | 支持批量而非逐个操作 | [MEDIUM] |

### 4.2 Execute

1. 检查热路径分配：
   ```bash
   rg "Vec::new|Box::new|String::from" FILE.rs -n
   ```
   → 在热路径（如 page fault handler）中 = 卓越性 P1

2. 检查动态分派：
   ```bash
   rg "dyn " FILE.rs -n
   ```
   → 在热路径中用 `dyn` = 卓越性 P2（建议泛型）

3. 检查逐个操作：
   ```bash
   rg "for .* in .*\.iter\(\)" FILE.rs -n
   ```
   → 可批量化的 = 卓越性 P2

### 4.3 Output

| 位置 | 性能问题 | 卓越性 P? | Suggested Fix |
|------|---------|----------|---------------|

**⚠️ 性能优化必须论证，不能泄漏硬件细节（参见 [review.md §硬件抽象原则](review.md)）。**

---

## 五、§18 代码即文档卓越性

### 5.1 检查项

| 检查项 | 说明 | Evidence 级别 |
|--------|------|-------------|
| 自解释 | 类型名、函数名表达意图 | [MEDIUM] |
| 注释解释为什么 | 注释解释"为什么"而非"是什么" | [DIRECT] |
| 无冗余注释 | 不重复代码已表达的信息 | [DIRECT] |
| 模块文档 | 有模块级 `//!` 文档 | [DIRECT] |
| 使用示例 | 有 doctest 示例 | [DIRECT] |

### 5.2 Execute

1. 检查模块文档：
   ```bash
   rg "^//!" FILE.rs -n
   ```
   → 无模块文档 = 卓越性 P1

2. 检查"是什么"注释（冗余）：
   ```bash
   rg "// .* = |// .* is |// .* the " FILE.rs -n
   ```
   → 重复代码已表达的 = 卓越性 P2

3. 检查 doctest：
   ```bash
   rg "/// ```" FILE.rs -n
   ```
   → pub 函数无 doctest = 卓越性 P2

4. 检查命名是否表达意图（如 `process_data` vs `parse_header`）。

### 5.3 Output

| 位置 | 代码即文档问题 | 卓越性 P? | Suggested Fix |
|------|-------------|----------|---------------|

---

## 六、§19 可测试性卓越性

### 6.1 检查项

| 检查项 | 说明 | Evidence 级别 |
|--------|------|-------------|
| 依赖注入 | 依赖通过参数注入而非全局 | [DIRECT] |
| 纯函数 | 逻辑为纯函数，副作用隔离 | [MEDIUM] |
| 副作用显式 | 副作用不隐藏在纯函数中 | [MEDIUM] |
| 不变量测试 | 测试覆盖不变量 | [INFERRED] |
| Property-based | 有 property-based test 机会 | [INFERRED] |

### 6.2 Execute

1. 检查全局依赖：
   ```bash
   rg "static|lazy_static|OnceCell" FILE.rs -n
   ```
   → 可注入的依赖用全局 = 卓越性 P1

2. 检查副作用隐藏：
   ```bash
   rg "fn .*-> .* \{" FILE.rs -n
   ```
   → 返回值的函数不应有隐藏副作用 = 卓越性 P1

3. 检查测试是否覆盖不变量（而非仅行为）。

### 6.3 Output

| 位置 | 可测试性问题 | 卓越性 P? | Suggested Fix |
|------|------------|----------|---------------|

---

## 七、§20 测试质量卓越性

### 7.1 测试三重标准

| 级别 | 类型 | 说明 | Evidence 级别 |
|------|------|------|-------------|
| L1 | C-Rust 对偶测试 | 验证 Rust 实现与 C 行为一致 | [DIRECT] |
| L2 | Trait 契约测试 | 验证 trait 实现满足契约 | [DIRECT] |
| L3 | doctest | 文档中的代码示例可运行 | [DIRECT] |

### 7.2 检查项

| 检查项 | 说明 | Evidence 级别 |
|--------|------|-------------|
| 测试有意义 | 非"测试 1+1=2" | [MEDIUM] |
| 错误路径覆盖 | 测试覆盖错误和边界 | [DIRECT] |
| 测试快速确定 | 无 flaky test | [INFERRED] |
| 测试命名 | 测试名表达意图 | [DIRECT] |
| L1/L2/L3 覆盖 | 核心函数至少一项覆盖 | [DIRECT] |

### 7.3 Execute

1. 统计测试数：
   ```bash
   rg "#\[test\]" FILE.rs -n | wc -l
   ```

2. 检查 L1 对偶测试：
   ```bash
   rg "fn test_.*c_rust|fn test_.*parity|fn test_.*dual" FILE.rs -n
   ```

3. 检查 L2 契约测试：
   ```bash
   rg "fn test_.*contract|fn test_.*trait" FILE.rs -n
   ```

4. 检查 L3 doctest：
   ```bash
   rg "/// ```" FILE.rs -n
   ```

5. 检查测试命名：
   ```bash
   rg "fn test_\w+" FILE.rs -n
   ```
   → `test_it_works` = 卓越性 P2

### 7.4 Output

| 位置 | 测试质量问题 | 卓越性 P? | Suggested Fix |
|------|------------|----------|---------------|

**测试覆盖统计**:
| 核心函数 | L1 对偶 | L2 契约 | L3 doctest | 总覆盖 |
|---------|---------|---------|-----------|--------|

---

## 八、Pass condition

- §15 API 设计：至少 B 级
- §16 表达力：至少 B 级
- §17 性能：至少 B 级
- §18 代码即文档：至少 B 级
- §19 可测试性：至少 B 级
- §20 测试质量：核心函数至少 L1/L2/L3 一项覆盖

**⛔ 卓越性问题不阻塞 Review 收敛。但应在 FINDINGS.md 中记录，供后续改进。**

---

## 九、与现有文件的关系

| 文件 | 角色 | 与本文件关系 |
|------|------|-------------|
| [review.md](review.md) | 核心原则 | 本文件细化"硬件抽象"和"类型安全"的卓越性 |
| [review-code-checklist.md](review-code-checklist.md) | 正确性检查 | 本文件是其卓越性补充 |
| [review-patterns.md](review-patterns.md) | 错误模式 | 本文件定义卓越性模式 |
| [review-core-semantics.md](review-core-semantics.md) | 核心语义 | 本文件验证核心语义的卓越性表达 |

### 在 Review 流程中的位置

```
Step 3: Sanity Check (正确性)
  ↓
Step 4: Cross-Document Check (正确性)
  ↓
Step 4.5: Excellence Check ← 本文件
  ↓
Step 5: Final Review Output
```
