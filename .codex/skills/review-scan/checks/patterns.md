# patterns: 错误模式检查（合并 doc/cross/code patterns）

> 本文件合并原 patterns/ 下 3 个文件：doc-patterns.md、cross-patterns.md、code-patterns.md。
> **强制规则**：每个模式检查必须先执行 grep，再下结论。每个判定标注 evidence [DIRECT/MEDIUM/INFERRED]。
> **⛔ Step 0 硬阻断前置（NEW 2026-07-16）**：进入本文件任何模式检查前，必须已通过 [SKILL.md Phase 1 §Step 0 硬阻断预检](../SKILL.md) + [process.md §Step 0 硬阻断规则](process.md)。**新增模式 69/70/71**（PSMD/CTOS/DOG）由本文件统一引用。

---

## 通用规则

1. **先读后判**：每个模式匹配必须先执行 grep 验证。
2. **Evidence 分级**：[DIRECT] grep 直接验证 / [MEDIUM] 需语义确认 / [INFERRED] 推断。
3. **零输出禁令**：0 个匹配也必须写"Checked pattern N, 0 matches"。

---

## §0 P0 必检清单（Gate D，每项必须显式回答 ✅/❌ + grep 证据）

| # | 检查项 | grep 命令模板 | 判定标准 | 未通过 |
|---|-------|--------------|---------|--------|
| **1** | 文档 §5 测试是否真实存在？ | `rg "fn {test_name}" {rust_dir} --type rust -n` | 文档 §5 列出的每个测试函数必须存在；缺失/未找到→P0 | P0 |
| **2** | 文档声明的 trait 是否有 ≥2 行为不同的 impl？ | `rg "impl.*{TraitName}" {rust_dir} --type rust -n` | 0 impl → P0（死代码/虚构 trait）；1 impl → P1（可能死代码，trait 抽象需 ≥2 行为不同的实现）；≥2 impl → ✅ | P0/P1 |
| **3** | 文档声明的函数是否在声明的文件中？ | `rg "fn {name}" {file}` | 文档说"在 file.rs 中定义 fn foo"，但 grep 无结果→P0；找到但 signature 完全不符也按未通过处理 | P0 |
| **4** | 核心算法是否是 stub？ | `rg "spin_loop\|todo!\|unimplemented!\|unreachable!\|panic!" {rust_dir} --type rust -n` | 文档描述的算法在代码中体现为 `spin_loop!`/`todo!`/`unimplemented!`/`unreachable!` → P0；非 test 代码中的 `panic!` 若表示功能未实现或本不应触发却可能触发 → 按 stub / 未处理路径处理，需在注释中论证其不可达性或可接受性 | P0/P1 |
| **5** | 文档 §4 签名是否与实际一致？ | 逐函数对比 `rg "fn {name}" {file}` 输出 vs 文档 §4 | 参数/返回值/可见性/泛型约束不一致→P0；有一项不符即整项 ❌ | P0 |

> **0 impl 优先于测试覆盖（2026-08-15 修复 C-P1-3）**：当 trait 0 impl 时，即便有测试覆盖，仍判 P0（trait 无任何实现 = 死代码 / 虚构）。判定流程：(a) 先检查 trait 是否有 ≥1 impl？否 → P0 死代码；(b) 有 impl → 检查测试覆盖度，0 测试覆盖 → P0-test-missing，< 3 测试 → P1。

**严格通过标准**：
- 5 项每一项必须为 ✅。
- **出现 PARTIAL / ⚠️ / 部分通过 / "基本通过" 中的任何一种，该项按 ❌ 处理，Gate D 整体未通过。**
- 任何一项 ❌ → scan.md 标记 DRAFT，禁止写入 STATE.md。
- 若某项确实不适用（如文档无 §5），需明确说明原因并单独列为一行 "N/A + 原因"，不能直接跳过。

**输出格式**（写入 scan.md）：
```markdown
### Gate D: P0 必检清单

| # | 检查项 | grep 命令 | grep 结果 | 判定 |
|---|--------|----------|----------|------|
| 1 | §5 测试存在 | `rg "fn test_foo" os/` | 0 matches | ❌ P0 |
| 2 | trait FooImpl 有 impl | `rg "impl.*FooImpl" os/` | 0 matches | ❌ P0 |
| 3 | fn bar 在 baz.rs | `rg "fn bar" os/baz.rs` | baz.rs:42 | ✅ |
| 4 | 核心算法非 stub（含 panic! 检查） | `rg "todo!\|unimplemented!\|panic!" os/` | 0 matches | ✅ |
| 5 | §4 签名一致 | 逐函数对比 | 一致 | ✅ |

**统计**：5 项中 N 项未通过 → N 个 P0
```

> **强制要求**：scan.md 必须含此表格，否则 Gate D 未通过，scan.md 标记 DRAFT。

---

## 一、文档错误模式（15 个）

| # | Pattern | grep Command | Anti-Pattern | Correct |
|---|---------|-------------|-------------|---------|
| 1 | 概念混淆 | `rg "KEY_TERM" FILE` | 错误含义 | grep 验证真实含义 |
| 2 | 条件编译遗漏 | `rg "SPAREPAGES" FILE` | 单一值 | 因平台不同 |
| 3 | 过度简化行为 | 读关键函数描述 | 缺细节 | 完整算法 |
| 4 | 缺架构差异 | `rg "64\|32" FILE` | 未提及 | 必须文档化 |
| 5 | 文档-代码不匹配 | 对比 §4 与 .rs | API 不同 | 必须匹配 |
| 6 | 不必要 ASCII art | 审查图表 | 文字即可 | 用文字 |
| 7 | 图表对齐差 | 目测图表 | 锯齿边缘 | 重新对齐 |
| 8 | 绝对文件路径 | `rg "file://" FILE` | 绝对路径 | 相对路径 `minix3/...` |
| 9 | C 覆盖不完整 | 来自 doc-08 结果 | 缺符号 | 完整覆盖 |
| 10 | 设计无依据 | 来自 doc-09 结果 | 无 Ch1&2 引用 | 追溯到 C |
| 11 | 设计-实现差距 | 来自 doc-04 结果 | 不匹配 | 对齐 |
| 12 | 测试未覆盖 | 来自 doc-10 测试 | 缺设计测试 | 覆盖设计 |
| 13 | no_std 违规 | `rg "std::" FILE` | std import | alloc + no_std |
| 14 | 硬件非 trait | 来自 code-02 结果 | 裸寄存器 | trait 抽象 |
| 15 | 开发记录风格 | `rg "[✅❌🚧]" FILE` | 状态 emoji | 中性文字 |

**Output**:
| Pattern# | Location | Anti-Pattern Found | P? | Suggested Fix |
|----------|---------|-------------------|----|---------------|

**Pass condition**: 零模式匹配（除非文档已说明）。

---

## 二、跨文档联动错误模式（3 个）

**Execute**:
1. 找同目录文档：`ls $(dirname TARGET)/*.md | grep -v review`
2. 检查**重复定义**：
   `rg "CONSTANT\s*=" $(dirname TARGET)/ --type md -n`
   → 同一常量在多个文档定义 → Pattern A (P2)
3. 检查**矛盾**：
   `rg "struct STRUCT_NAME" $(dirname TARGET)/ --type md -n`
   → 不同字段数/描述 → Pattern B (P1)
4. 检查**缺失引用**：
   阅读目标文档。如果提到某概念在同目录有专属文档，验证是否引用。
   → 未引用 → Pattern C (P2)

| Pattern | Detail | Doc A | Doc B | P? | Suggested Fix |
|---------|--------|-------|-------|----|---------------|
| A: 重复 | CLICK_SIZE=4096 | 04-doc.md:L30 | 05-doc.md:L30 | P2 | 从 05 移除，引用 04 |
| B: 矛盾 | vmproc 8 字段 vs 7 | doc1:L50 | doc2:L50 | P1 | 验证源码，统一 |
| C: 缺失引用 | 用 VmForkIn 但无 fork 文档引用 | target:L200 | — | P2 | 加参见 |

**Verification Commands**:
```bash
DIR=$(dirname TARGET)
rg "CONSTANT\s*=" "$DIR" --type md -n | sort -t: -k3
rg "struct struct_name" "$DIR" --type md -n
rg "\[.*\]\(.*\.md\)" TARGET -n
```

**Pass condition**: 同目录文档无重复、矛盾、缺失引用。

---

## 三、代码错误模式（19 个：基础 10 + Kernel SMP 4 + 跨阶段通用 5）

### 基础代码模式（10 个，16-25）

| # | Pattern | grep Command | Anti-Pattern | Correct |
|---|---------|-------------|-------------|---------|
| 16 | 裸整数 | `rg "fn \w+\(.*u32.*u32.*\)" FILE` | `(u32, u32)` 参数 | 用 newtype/enum |
| 17 | C 哨兵 | `rg "== 0.*end\|== 0.*null\|== -1" FILE -i` | `ptr == 0` | 用 `Option` |
| 18 | unsafe 滥用 | `rg "unsafe \{" FILE` | 无安全注释 | 加 `// SAFETY:` |
| 19 | 错误 errno | `rg "Error::" FILE` | 自造 | 匹配 Minix3 E* |
| 20 | 裸 as 截断 | `rg "as u32\|as u16" FILE` | 无注释 | 加安全说明 |
| 21 | 硬件泄漏 | `rg "CR3\|cr3\|PTE\|pde" FILE` | OS 层裸寄存器 | 移到 arch crate |
| 22 | std 违规 | `rg "std::" FILE` (非 test) | std import | `alloc` + no_std |
| 23 | pub 滥用 | `rg "^pub " FILE` | 过度 pub | `pub(crate)` |
| 24 | 类型安全过度 | 太多 typestate | 类型爆炸 | 考虑 enum |
| 25 | 不必要 trait | `rg "^trait" FILE` | 1 实现，从未 bound | 自由函数 |

### 内核 SMP 模式（仅内核模块）

| # | Pattern | grep Command | Anti-Pattern | Correct |
|---|---------|-------------|-------------|---------|
| 26 | BKL 未持有 | `rg "static mut" FILE -n` | 全局访问无 BKL | `/// SAFETY: Caller holds BKL` |
| 27 | Rc/RefCell 跨 CPU | `rg "Rc\|RefCell" FILE -n` | 内核 Rc/RefCell 共享 | 用 `Arc`+`Mutex`/`Atomic`，或 `// per-CPU` |
| 28 | spinlock 内睡眠 | `rg "ipc_sendrec\|schedule\|sleep" FILE -n` | BKL_LOCK 内调用 | BKL_UNLOCK 后阻塞 |
| 29 | 跨 CPU 本地读 | `rg "PER_CPU\|cpu_var" FILE -n` | 从 CPU A 读 CPU B | Atomic 或 per-CPU guard |

### 跨阶段通用模式（5 个，适用于所有模块）

| # | Pattern | grep Command | Anti-Pattern | Correct |
|---|---------|-------------|-------------|---------|
| 30 | 外部知识误导 | `rg "CR4\|PSE\|MSR\|PAE\|granule\|Sv39" FILE -i -n` | 注释中错误硬件声明 | 注释匹配真实规范 |
| 31 | 通用 struct 上下文泄漏 | `rg "^pub struct" FILE` + 字段审计 | 共享 struct 中架构特定字段未标记 | 用 `Option<T>` 或注释 "x86-64 only" |
| 32 | 返回值静默丢弃 | `rg "let _\w+ = \|let _ = " FILE -n` | 丢弃返回值无注释 | 加注释说明为何安全忽略 |
| 33 | 无理由资源泄漏 | `rg "Box::leak\|mem::forget" FILE -n` | 泄漏无恢复说明 | 文档化恢复路径或"不释放因为..." |
| 34 | 可疑注释理由 | `rg "// .*因为\|// .*由于\|// .*避免\|// .*为了" FILE -n` | 理由在上下文中不成立 | 给出诚实、上下文合适的理由 |

**Output**:
| Pattern# | Location | Anti-Pattern Found | P? | Suggested Fix |
|----------|---------|-------------------|----|---------------|

**Pass condition**: P0/P1 模式零匹配。
**⛔ 模式 26-29 仅适用于内核模块。用户态服务器跳过。**

---

### 模式 73: Doc Code Example Rust 2024 Edition Drift（NEW 2026-07-30）

> **定义**：文档代码示例使用 Rust 2024 已 deprecated 的 `static mut`，实际代码已迁移至 `Atomic*` / `UnsafeCell`；或文档路径与 `find` 结果不一致（目录重组）。

**检查命令**：
```bash
# Doc-side
rg "static mut" {doc}.md  # 应仅在解释注释中出现
rg "arch/src/(pt_alloc|paging\.rs|paging_ext)" {doc}.md  # 应 0 hits（实际在 arch/src/arch/）

# Rust-side
rg "static mut" os/ -t rust  # 应 0 hits（实际用 Atomic/AtomicU64/AtomicBool）

# 路径一致性
find os/arch/src -name "pt_alloc.rs" -o -name "paging.rs" -o -name "paging_ext.rs"
```

**判定**：
- 文档示例含 `static mut` 而实际代码无 → **P1**（模式 73a）
- 文档路径 vs 实际路径不一致（目录重组）→ **P1**（模式 73b）

**详细规则**：见 `prompt/skill/review-patterns-skill.md §模式 73`。

**首次发现**：2026-07-30 01-boot-shim-bootstrap review（3 处 P1 doc-code 漂移）。
**⛔ 模式 30-34 适用于所有模块（用户态 + 内核）。不得跳过。**

---

## 四、测试错误模式（6 个）

> 对应源 [review-patterns.md §六](../../../../prompt/review-rules/review-patterns.md) 模式 35-40。**适用所有模块**（用户态 + 内核）。
> 详细执行方法见 [excellence.md §21 测试质量卓越性](excellence.md) + [process.md Step 4.5 测试验证（Gate E）](process.md)。

| # | Pattern | grep Command | Anti-Pattern | Correct |
|---|---------|-------------|-------------|---------|
| 35 | L1 对偶缺失 | 文档 §5 vs `rg "fn {test_name}" RUST_DIR` | 仅测 Rust 内部逻辑 | 验证与 Minix3 行为一致 |
| 36 | L2 trait 契约缺失 | `rg "fn test.*<.*:.*Trait" RUST_DIR` | 仅测具体实现 | 验证所有实现满足 trait 契约 |
| 37 | L3 doctest 缺失 | `rg "^///" RUST_DIR \| grep -v "/// "` | pub 函数无 doctest | 文档+可运行测试合一 |
| 38 | 测试命名不表达意图 | `rg "^fn test_" RUST_DIR` + 语义审计 | `test_1` / `test_foo` | `test_alloc_returns_null_when_pool_exhausted` |
| 39 | 测试仅覆盖正常路径 | 测试列表 vs 错误码列表（Minix3 E*） | 缺错误码路径 | happy path + 每个 errno 至少一个测试 |
| 40 | 测试依赖全局状态 | `rg "static \|Lazy\|OnceCell" RUST_DIR` | 测试间共享 mutable state | 隔离或 mock |

**Output**:
| Pattern# | Location | Anti-Pattern Found | P? | Suggested Fix |
|----------|---------|-------------------|----|---------------|

**Pass condition**: L1/L2/L3 三重标准至少 1 项覆盖每个核心函数。
**⛔ 模式 35 (L1) 是 P0 必检项 — 缺失即 P0。**

---

## 五、卓越性错误模式（7 个）

> 对应源 [review-patterns.md §七](../../../../prompt/review-rules/review-patterns.md) 模式 41-47。在正确性 gate 通过后执行。
> 详细执行方法见 [excellence.md §4.1-4.5 文档卓越性 + §16-21 代码卓越性](excellence.md)。

| # | Pattern | 检查项 | Anti-Pattern | Correct |
|---|---------|--------|-------------|---------|
| 41 | 文档叙事弧断裂 | 章节顺序 + 动机说明 | 章节堆砌、无过渡 | 问题→分析→决策→实现 4 段弧 |
| 42 | 文档术语未定义 | 首次出现位置 | 术语首次出现无定义 | 首次出现给定义 + 类型/范围 |
| 43 | 代码 API 易误用 | 状态机/类型设计 | 允许无效状态 | make wrong state unrepresentable |
| 44 | 代码错误类型不精确 | `Error::` 类型审计 | `Box<dyn Error>` 泛滥 | 精确 enum + thiserror 派生 |
| 45 | 代码冗余注释 | 注释 vs 代码语义 | 注释重复代码已表达信息 | 注释只解释"为什么"非"是什么" |
| 46 | 代码副作用隐藏 | 函数签名审计 | 看似纯函数实际有副作用 | 副作用显式化（返回新值 + 单独 effect） |
| 47 | 代码全局依赖未注入 | 依赖图审计 | 直接访问全局 static | DI 注入 / trait abstract / `with_*` builder |

**Output**:
| Pattern# | Location | Anti-Pattern Found | P? | Suggested Fix |
|----------|---------|-------------------|----|---------------|

**Pass condition**: 不降级。卓越性问题不降级为正确性问题。卓越性 P1 ≠ 正确性 P1。
**⛔ 模式 41-47 在正确性 gate 通过后执行，**禁止**在 P0/P1 修复阶段套用。**

---

## 六、叙事与概念错误模式（13 个，48-60）

> 对应源 [review-patterns.md §八](../../../../prompt/review-rules/review-patterns.md) 模式 48-60。
> **适用所有文档**（用户态 + 内核）。在 Phase 3 (Doc Checks) 和 Phase 5 (Pattern Checks) 中执行。

| # | Pattern | 检查方法 | Anti-Pattern | Correct | P? |
|---|---------|---------|-------------|---------|-----|
| 48 | 因果链编造 | 提取"因为...所以..."句式，验证 X→Y 技术正确性 | claim 正确但解释的因果链技术上错误 | 解释的 BECAUSE 部分必须是真实机制 | P0 |
| 49 | 元注释泄漏 | `rg "本节将\|接下来\|首先.*然后\|这里我们" FILE -n` 统计 | 作者在正文中叙述写作策略 | 写作策略移到设计笔记，正文只讲技术 | P1 (>5 实例) |
| 50 | 架构范围未标注 | `rg "TSS\|GDT\|IDT\|CR3" FILE -n` 检查是否标注 x86-specific | x86 特有机制当作通用机制讲述 | 明确标注 "[x86-64]" 或 "架构特有" | P1 |
| 51 | 实现驱动概念章 | 读 Ch1 第一段，识别主语 | Ch1 主语是函数名/结构体名 | Ch1 主语是 CPU/OS/系统 | P1 |
| 52 | 单向心智模型 | 检查机制描述是否覆盖双向（entry + return） | 只描述 entry，不描述 return | 双向闭环：entry + return | P1 |
| 53 | 跨架构共性未提取 | 多架构文档检查是否有统一抽象 | 多架构文档无统一抽象，直接讲 arch-specific | 先统一抽象，再 arch-specific | P1 |
| 54 | 视角漂移 | 检查同一章节内主语是否一致 | 同一章内主语从 CPU 切换到函数 | 同一章保持一致视角 | P2 |
| 55 | 架构特有机制喧宾夺主 | 检查架构特有机制占比 | arch-specific legacy 占据核心篇幅 | arch-specific 作为补充，核心是通用机制 | P2 |
| 56 | 决策日志体 Ch3 | `rg "^###\|^####" FILE \| grep "3\."` 检查每个决策是否有 rationale | Ch3 列出决策但无 rationale | 每个决策有 Problem + Rationale | P1 |
| 57 | 例子前置知识泄漏 | 审查例子是否引入无关细节 | 例子引入未讲过的概念 | 例子只用已讲过的概念 | P2 |
| 58 | 跨文档阶段状态表漂移 | 检查含 §N 实施状态表的文档是否与代码同步 | 状态表写"待实施"但代码已实现 | 阶段完成时原子更新 §N 状态表（file.rs:N-M + 实现要点） | P1 |
| 59 | 文档字段计数漂移 | 比对 doc §X 字段计数与 struct 公有字段数 | doc 写"9 字段"实际 12；或代码示例遗漏字段 | 字段增/删/恢复时 doc 计数 + 示例同 commit 更新 | P1 |
| 60 | 诚实显式 TODO | 检查 TODO 注释是否含 file:line/严重度/解释/doc 引用 | `// TODO` + `// fix later` 等无四要素 | 四要素齐：file:line + 严重度 + 解释 + doc §X | P1 |

**Output**:
| Pattern# | Location | Anti-Pattern Found | P? | Suggested Fix |
|----------|---------|-------------------|----|---------------|

**Pass condition**:
- 模式 48 (因果链编造) 零匹配 → P0
- 模式 49-53, 56, 58-60 零匹配 → P1
- 模式 54-55, 57 零匹配 → P2
**⛔ 模式 48 是 P0 必检项 — 因果链编造即 P0。**
**⛔ 模式 51 (实现驱动概念章) 与 Check 13 (Ch1 Mandatory Skeleton) 配合使用。**

---

## 七、Design-First 错误模式

> 对应源 [review-patterns.md §X.5](../../../../prompt/review-rules/review-patterns.md) 模式 63-65。
> **适用 Profile R / Profile C / Profile I / Profile H-K review**（设计优先或完整 review）。
> **失败模式**：Design doc / 代码与 design 对齐 / Review 文档自身，三类失败。

| # | Pattern | 检查方法 | Anti-Pattern | Correct | P? |
|---|---------|---------|-------------|---------|-----|
| 63 | **Design-Missing**（设计缺口） | 遍历 doc/code 引用的 trait/方法/不变量，grep design doc（`06-design.md` 非 bagging / `06-design-final.md` bagging 等）是否定义 | doc/code 引用了 design 没说过的 trait 方法或不变量；scan.md 中 `P0-XX-1` 编号泄漏到 doc/code | design doc 显式定义所有引用的契约 | P0 |
| 64 | **开发文档味**（Development-Doc Flavor） | `rg "本节将\|接下来我们\|首先.*然后.*最后\|步骤 1\|进度\|待完成\|已完成\|🚧\|✅" FILE -n` 统计 | doc 读起来像开发日志 / 进度跟踪 / Todo List（>5 实例） | doc 是面向读者的解释，不是面向作者的过程记录 | P1 |
| 65 | **Translate 倾向**（Translation Tendency） | 读 doc/code 是否只翻译 Minix3 C 而无 Rust 类型系统表达 | "对应 C 中的 foo()，我们写一个 foo()"；改命名但不重表达 | 用 Rust 类型系统重新表达（newtype/enum/bitflags/trait） | P1 |

**Pattern 64 扩展禁用词清单**（任一 >5 实例 → P1）：
- 迭代叙事：`本节将 / 接下来我们 / 首先 / 然后 / 最后 / 总结一下`
- 开发主体：`我们 / 笔者 / 开发者 / 实现者`
- 过程动词：`编写 / 实现 / 修改 / 调整 / 优化`
- 步骤标记：`步骤 1 / Step 1 / 第一步 / 下一步`
- 状态标记：`TODO / FIXME / XXX / 待完成 / 已完成 / 进行中 / 进度`

**Pattern 63 修复协议**：
- 发现 design 缺关键决策 → 不能 silently 用代码填充
- 必须：(a) 补 design 章节，或 (b) 显式标 IN_DESIGN 进入 Review 中断协议
- 绝不允许 scan.md 中 P0-design-missing 编号（`P0-XX-1`）泄漏到 doc/code 中

**Output**:
| Pattern# | Location | Anti-Pattern Found | P? | Suggested Fix |
|----------|---------|-------------------|----|---------------|

**Pass condition**:
- 模式 63 (Design-Missing) 零匹配 → P0（P0-design-missing 必修复或标 IN_DESIGN）
- 模式 64-65 零匹配 → P1
- IN_DESIGN 项数 ≤ 当前轮允许阈值（详见 [review-process.md §IN_DESIGN 状态机](../../../../prompt/review-rules/review-process.md)）

**⛔ 模式 63 是 Design-First Review 的 P0 必检项 — design 缺失必须执行 Step 0.3 嵌入生成或显式 IN_DESIGN。**
**⛔ 模式 58-60 是 doc-code 一致性 + TODO 规范专项；与 51/53/56 不同维度。**

### 模式 74: Doc Path Convention Drift（NEW 2026-07-30）

> **定义**：doc 内 Rust crate 路径漏 `os/` workspace 根前缀（典型：`kernel/src/...` 应为 `os/kernel/src/...`）。

**检查命令**：
```bash
# Doc-side 裸路径扫描（应仅命中 minix3/... 上下文）
rg "kernel/src/|boot-shim/src/|arch/src/" {doc}.md | grep -v "minix3"

# 双重前缀（sed 副作用）
rg "os/os/" {doc}.md  # 必须 0 hits

# 跨文档一致性
rg "os/kernel/src/" notes/rewrite/{module}/{stage}/0*-*.md | wc -l
```

**判定**：
- 裸 `kernel/src/` 等（缺 `os/`） → **P1**（模式 74 默认）
- `os/os/` 双重前缀 → **P1**（sed 副作用）

**修复**：
```bash
sed -i 's|kernel/src/|os/kernel/src/|g' {doc}.md
sed -i 's|os/os/|os/|g' {doc}.md
```

**详细规则**：见 `prompt/skill/review-patterns-skill.md §模式 74`。

**首次发现**：2026-07-30 02-higher-half-kernel review（18 处路径缺 `os/` 前缀）。

### 模式 75: Doc See-Also Range Drift（NEW 2026-07-31）

> **定义**：doc 中"参见 X.rs:Y-Z"形式的范围引用出现两类漂移——起止行号 +1 偏移（如 :55-76 → 实际 :56-78）+ 上界范围过短（如 :55-399 → 实际 :56-523，doc 写作时文件较小未随代码演化更新）。

**检查命令**（Step 1.0d 强制）：
```bash
# 1. 抽取"参见"型范围引用
rg -o "参见 \`[^\`]+\.rs:[0-9]+-[0-9]+\`" {doc}.md | sort -u

# 2. 验证起止行号
for ref in $(rg -o "参见 \`[^\`]+\.rs:[0-9]+-[0-9]+\`" {doc}.md | sort -u); do
    path=$(echo "$ref" | rg -o "[^\`]+\.rs")
    start=$(echo "$ref" | rg -o ":[0-9]+-" | rg -o "[0-9]+")
    sed -n "${start}p" "$path"  # 验证首行内容
done

# 3. 验证上界（impl 结束位置）
rg -n "^impl PlatformDesc for X|^impl fmt::Display" {path}
wc -l {path}  # 当前实际行数
```

**判定**：
- 起止 ±1 偏移 → **P2 行号偏移**
- 上界 < 实际 impl 结束 → **P2 范围过短**
- 起止偏移 > 1 → **P1 行号漂移**

**修复**：
```bash
# 1. 修正 +1 偏移
sed -i 's|device_tree.rs:55-|device_tree.rs:56-|g' {doc}.md
sed -i 's|acpi.rs:110-|acpi.rs:111-|g' {doc}.md

# 2. 更新上界到 impl 结束
sed -i 's|device_tree.rs:55-399|device_tree.rs:56-423|g' {doc}.md
```

**与 Step 1.0a 区分**：Step 1.0a 行号主动抽样只检查单行引用（`// path:line`），漏检范围引用（`参见 path:line-line`）。本次 04 doc review 漏检 2 处 L831/L883 顺带修复。

**详细规则**：见 `prompt/skill/review-patterns-skill.md §模式 75` + `prompt/review-rules/review-process.md §Step 1.0d`。

**首次发现**：2026-07-31 04-platform-discovery review（2 处范围漂移 L831/L883 漏检）。

### 模式 76: Cross-Doc Attribution Drift（NEW 2026-07-31）

> **定义**：代码注释中"covered in NN" / "see XX-doc.md §Y" 等指向特定 doc 编号或文件名的引用，因 doc 编号重排或 doc 改名而系统性过时。

**检查命令**（Step 1.0e 强制）：
```bash
# 1. 扫描代码注释中的 doc 归属引用
rg "covered in 0[0-9]" os/ -t rust -n
rg "see 0[0-9]-.+\.md" os/ -t rust -n

# 2. 验证当前 doc 编号
ls notes/rewrite/{module}/{stage}/ | rg "^[0-9]+"

# 3. 验证目标 doc 存在
for ref in $(rg "see [0-9]+-.+\.md" os/ -t rust -o); do
    doc_file=$(echo "$ref" | rg -o "[0-9]+-.+\.md")
    [ ! -f "notes/.../$doc_file" ] && echo "❌ STALE: $ref"
done
```

**判定**：
- `(covered in NN)` 注释错位 → **P1 注释错位**
- `see XX-doc.md` 引用已删除 doc → **P1 注释失效**
- `see XX-doc.md` 引用已重命名 doc → **P1 注释失效**

**修复**（批量 sed）：
```bash
# 1. 修 (covered in NN) 注释
sed -i 's|(covered in 04)|(covered in 05)|g' os/kernel/src/lib.rs
sed -i 's|(covered in 05)|(covered in 06)|g' os/kernel/src/lib.rs
sed -i 's|(covered in 06)|(covered in 07)|g' os/kernel/src/lib.rs

# 2. 修 see XX-doc.md 注释（确认重命名映射后批量替换）
sed -i 's|04-clock-interrupt-init.md|05-clock-interrupt-init.md|g' os/arch/src/arch/{clock.rs,arch_init.rs}
```

**已知过时 doc 命名**（本次 05 review 发现）：
- `04-clock-interrupt-init.md` → `05-clock-interrupt-init.md`
- `05-exception-interrupt.md` → `14-exception-interrupt.md`（推测）
- `06-arch-post-init.md` → `08-system-init-boot-finish.md`（推测）
- `02-page-table-kernel.md` → `02-higher-half-kernel.md`（推测）

**与已有模式区分**：
- **Pattern #66** = Reference Code Path Drift（代码路径引用 `file:line` 不存在）
- **Pattern #76** = **Cross-Doc Attribution Drift**（doc 编号/文件名引用不一致）

### 模式 66 主动应用案例（NEW 2026-07-31, Doc 07 review）

> **亮点**：doc 07 §5.4 显式声明"不引用 syscall_copy.rs 具体行号避免漂移传播（Pattern #66 RCPD）"—— **首个 doc 主动标注已应用 review pattern**。
>
> 这表明 doc 作者已具备 review pattern 意识，主动避免引入 Pattern #66 风险。
>
> **建议**：其他 doc 在 §5 测试章节引用行号时，可借鉴 doc 07 的做法：
> - 显式声明"不引用 X.rs 行号，避免漂移传播"
> - 或：使用 grep 命令而非具体行号（如"通过 `rg fn X os/Y.rs` 找到实现"）
> - 或：行号引用加版本/时间戳（如"截至 YYYY-MM-DD, X.rs:Y"）
>
> **首次发现**：2026-07-31 07-cross-space-init review（历史参考：Doc 07 review）

**详细规则**：见 `prompt/skill/review-patterns-skill.md §模式 76` + `prompt/review-rules/review-process.md §Step 1.0e`。

**首次发现**：2026-07-31 05-clock-interrupt-init review（3 处 `(covered in NN)` + 9+ 处 `see XX-doc.md`）。

### 模式 77: Code Comment Line Drift（NEW 2026-07-31）

> **定义**：代码注释中引用 `file:line`（如 `// see proc_table.rs:129`）因代码增量而**漂移**（文件行号下移）。本次 08 doc review 发现 `os/kernel/src/lib.rs:1170/1181/1193` 3 处注释错位。

**检查命令**（Step 1.0f 强制）：
```bash
# 1. 扫描所有代码注释中的 file:line 引用
rg "see [a-z_/0-9]+\.rs:[0-9]+" os/ -t rust -n

# 2. 验证实际行号
for ref in $(rg "see [a-z_/0-9]+\.rs:[0-9]+" os/ -t rust -o); do
    file=$(echo "$ref" | rg -o "[a-z_/0-9]+\.rs")
    line=$(echo "$ref" | rg -o "[0-9]+")
    actual=$(sed -n "${line}p" "$file" 2>/dev/null)
    echo "$ref: $actual"
done
```

**判定**：
- 引用行号 ±1 偏移 → ✅
- 引用行号偏差 2-5 → **P2 代码注释轻微漂移**
- 引用行号偏差 > 5 → **P2 代码注释漂移**
- 引用行号偏差 > 50 → **P1 代码注释显著漂移**

**修复**（双修避免传递性 drift）：
```bash
# 1. 修代码注释（root cause）
sed -i 's|(see proc_table.rs:129)|(see proc_table.rs:276)|' os/kernel/src/lib.rs

# 2. 同步修所有复述的 doc（如果 doc 复述了错误注释）
sed -i 's|proc_table.rs:129|proc_table.rs:276|g' notes/.../{doc}.md

# 3. 验证全项目干净
rg "proc_table\.rs:129|smp\.rs:127-132|smp\.rs:80-145" os/ notes/
# (empty = ✅)
```

**已知偏差**（本次 08 review 发现）：
- `os/kernel/src/lib.rs:1170` `smp.rs:80-145` → 实际 `smp.rs:135`（已修）
- `os/kernel/src/lib.rs:1181` `smp.rs:127-132` → 实际 `smp.rs:200`（已修）
- `os/kernel/src/lib.rs:1193` `proc_table.rs:129` → 实际 `proc_table.rs:276`（已修）

**与已有模式区分**：
- **Pattern #66** = Reference Code Path Drift（代码路径引用 `file:line` 不存在）—— Pattern #77 是 Pattern #66 的子类型（行号漂移）
- **Pattern #77** = **Code Comment Line Drift**（**代码注释行号漂移**）—— 重点是代码注释而非 doc 引用

**详细规则**：见 `prompt/skill/review-patterns-skill.md §模式 77` + `prompt/review-rules/review-process.md §Step 1.0f`。

**首次发现**：2026-07-31 08-system-init-boot-finish review（**根因是 lib.rs 代码注释错误，doc §4.6 复述了错误注释**）。

### 模式 78: C Source Bug Unlabeled (CSBU)（NEW 2026-08-14）

**严重度**：P1

**问题**：Rust 修复了 Minix3 C 源码 bug，但代码注释未标注 `// MINIX3 BUG:`。维护者可能"修复"回 C 的 bug。

**判定标准**：
- Rust 行为与 C 不一致，原因是 C 源码有 bug（非设计差异）
- 代码注释无 `// MINIX3 BUG:` 标注
- 文档 §2 对应位置无 bug 说明

**验证命令**：
```bash
rg "// MINIX3 BUG:" os/ --type rust -n
```

**正确示例**：
```rust
// MINIX3 BUG: region.c:841-842 ignores ev_reference return value
// Rust fix: ev_copy returns Err(NotSupported)
```

**来源案例**：region.c:841 ev_reference 忽略、enter_queue 写错进程、anon_pagefault 内存泄漏。

**详细规则**：见 `prompt/skill/review-patterns-skill.md §模式 78`。

**首次发现**：2026-08-14 规则集优化（从 project_memory 沉淀的多个 C bug 修复案例抽象）。
