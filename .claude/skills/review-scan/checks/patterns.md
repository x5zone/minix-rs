# patterns: 错误模式检查（合并 doc/cross/code patterns）

> 本文件合并原 patterns/ 下 3 个文件：doc-patterns.md、cross-patterns.md、code-patterns.md。
> **强制规则**：每个模式检查必须先执行 grep，再下结论。每个判定标注 evidence [DIRECT/MEDIUM/INFERRED]。

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
| **2** | 文档声明的 trait 是否有 ≥1 impl？ | `rg "impl.*{TraitName}" {rust_dir} --type rust -n` | 0 impl → P0（死代码/虚构 trait）；只有默认 impl 没有真实 arch impl 也算未通过 | P0 |
| **3** | 文档声明的函数是否在声明的文件中？ | `rg "fn {name}" {file}` | 文档说"在 file.rs 中定义 fn foo"，但 grep 无结果→P0；找到但 signature 完全不符也按未通过处理 | P0 |
| **4** | 核心算法是否是 stub？ | `rg "todo!\|unimplemented!\|unreachable!\|panic!" {rust_dir} --type rust -n` | 文档描述的算法在代码中体现为 `todo!`/`unimplemented!`/`unreachable!` → P0；非 test 代码中的 `panic!` 若表示功能未实现或本不应触发却可能触发 → 按 stub / 未处理路径处理，需在注释中论证其不可达性或可接受性 | P0/P1 |
| **5** | 文档 §4 签名是否与实际一致？ | 逐函数对比 `rg "fn {name}" {file}` 输出 vs 文档 §4 | 参数/返回值/可见性/泛型约束不一致→P0；有一项不符即整项 ❌ | P0 |

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

## 三、代码错误模式（14 个：基础 10 + Kernel SMP 4）

### 基础代码模式（14 个）

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
**⛔ 模式 30-34 适用于所有模块（用户态 + 内核）。不得跳过。**

---

## 四、测试错误模式（6 个）

> 对应源 [review-patterns.md §六](../../../../prompt/review-rules/review-patterns.md) 模式 35-40。**适用所有模块**（用户态 + 内核）。
> 详细执行方法见 [excellence.md §20 测试质量卓越性](excellence.md) + [process.md Step 4.5 测试验证（Gate E）](process.md)。

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
> 详细执行方法见 [excellence.md §4.1-4.4 文档卓越性 + §15-20 代码卓越性](excellence.md)。

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

## 六、叙事与概念错误模式（10 个，48-57）

> 对应源 [review-patterns.md §八](../../../../prompt/review-rules/review-patterns.md) 模式 48-57。
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
**⛔ 模式 58-60 是 doc-code 一致性 + TODO 规范专项；与 51/53/56 不同维度。**
