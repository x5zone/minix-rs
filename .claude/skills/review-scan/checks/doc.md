# doc: 文档正确性检查（合并 12 个 doc check）

> 本文件合并原 doc/00-12 共 12 个检查项，解决 attention decay 和过度拆解问题。
> **强制规则**：每个检查必须先执行 grep/read，再下结论。每个判定标注 evidence [DIRECT/MEDIUM/INFERRED]。
> **⛔ Step 0 硬阻断前置（NEW 2026-07-16；2026-07-17 更新）**：进入本文件任何检查前，必须已通过 [SKILL.md Phase 1 §Step 0 硬阻断预检](../SKILL.md) + [process.md §Step 0 硬阻断规则](process.md)。`{NN}-design.v*.md` / `{NN}-outline.v*.md` / `{NN}-outline-review.v*.md` 缺失 → **Gate H.1/H.6 FAIL → Step 0.3 嵌入生成**（不中断 review，2026-07-17 变更：原"阻断 Phase 2"改为"Step 0.3 嵌入生成"）（模式 69 PSMD 触发）。

---

## 通用规则（适用于所有 doc check）

1. **先读后判**：每个判定必须先执行 grep 验证，再下结论。禁止凭印象判断。
2. **Evidence 分级**：
   - [DIRECT] grep 直接验证（有明确输出）
   - [MEDIUM] 名称匹配但语义需进一步确认
   - [INFERRED] 基于上下文推断（需标注"待确认"）
3. **零输出禁令**：如果检查发现 0 个问题，必须写"Checked N items, found 0 issues"。禁止留空。
4. **证据强制**：每个检查必须包含 grep count、file count 或 line number range 作为证据。

---

## Check 00: Claims-Evidence Tracing（论文级质量）

> 每个关于 C 源码行为的事实声明必须有可验证证据（file+line）。

**Execute**:
1. 提取文档中所有**事实声明**（行为描述、数值断言、结构体字段声明、调用关系）。跳过过渡句和设计理由（"为什么"解释）。
2. 对每个声明：
   - `rg "PATTERN" minix3/minix/servers/{module}/ --type c -n` — grep 证据
   - **证据强度**: Direct grep (strong) / Need logic inference (medium) / No source basis (weak)
   - **可复现性**: 另一个人能否仅凭 doc + source 验证此声明？

**Output**:
| Claim | Type | Doc Location | Evidence Source | Strength | Reproducible? | Verdict |
|-------|------|-------------|----------------|----------|--------------|---------|
| alloc_pages scans high→low | behavior | §2.3 L45 | alloc.c:123-156 | strong | ✅ | ✅ |

**Extra Quality (paper-grade docs)**:
- [ ] **Prior assumptions stated**: 文档是否说明"读者已知道 X"？
- [ ] **Fact vs judgment separated**: 读者能否区分源码事实和作者观点？
- [ ] **Edge cases covered**: 文档是否讨论行为不成立的条件？
- [ ] **Uncertainty honest**: 无法确认的声明标注"待确认"？

**Pass condition**: 无 "weak" + unreproducible 声明 → P0。Medium 声明 → P1。
**⛔ SELF-CHECK**: "N claims extracted, M strong, K medium, L weak."

---

## Check 01: Concept Accuracy（概念准确性）

**Execute**:
1. 提取文档 Ch1 和 Ch2 中所有**粗体术语**、类型名、函数/结构体/宏名。
2. 对每个术语：`rg "TERM" minix3/minix/servers/{module}/ --type c -n`
3. 也搜索：`rg "TERM" minix3/minix/kernel/ --type c -n` 和 `rg "TERM" minix3/minix/include/ -n`
4. 数值常量：`rg "#define CONSTANT" minix3/minix/servers/{module}/ -n`

**Output**:
| Term | Doc Location | grep Result | Source Line | Match? | Issue |
|------|-------------|-------------|-------------|--------|-------|

**Numeric constants**:
| Constant | Doc Value | Source Value | Source Location | Match? |
|----------|----------|-------------|-----------------|--------|

**Pass condition**: 每个术语在 C 源码中找到。每个常量值匹配。
**⛔ SELF-CHECK**: 至少检查 50% 文档，最少 5 个。"Checked X of Y docs."

---

## Check 02: C Code Reference Verification（C 代码引用验证）

**Execute**:
1. 提取文档中所有 C 代码引用（文件路径 + 行号）。
2. **对每个引用做 4 项检查**：
   - 文件是否存在？`ls minix3/minix/servers/{module}/FILE.c`
   - 行号是否准确？`sed -n 'START,ENDp' minix3/minix/servers/{module}/FILE.c` 或 `rg -n`
   - 代码片段是否完整（未截断）？
   - 文档解释是否匹配源码行为？

> **严格禁止**：输出中出现 **"未逐行验证" / "未全部验证" / "抽样验证" / "行号未核对"** 等模糊表述。如果引用数量过大无法一次性验证完，必须明确列出已验证范围、未验证范围，并将未验证部分标记为 **"P1 待验证"** 或 **"to confirm"**，给出下一轮验证计划。

**Output**:
| Ref# | Doc Location | File Path | Line Range | Exists? | Lines OK? | Fragment OK? | Explanation OK? | Issues |
|------|-------------|-----------|-----------|---------|----------|-------------|----------------|--------|

**Pass condition**: 所有文件存在。行号 ±5 内。片段未截断。解释匹配。
**⛔ SELF-CHECK**: 至少检查 50% 含 C 代码引用的文档。"Checked X of Y docs."
**⛔ 出现"未逐行验证"等模糊结论 → P1（流程违规）**

---

## Check 03: Data Structure Coverage（数据结构覆盖）

**Execute**:
1. 从文档 Ch2 提取所有分析的结构体名。
2. 在 C 源码中找所有结构体：`rg "^struct \w+" minix3/minix/servers/{module}/ --type c -n`
3. 对文档语义范围内的每个结构体，对比覆盖字段 vs 源码字段。
4. 也检查头文件：`rg "^struct \w+" minix3/minix/servers/{module}/*.h -n`

**Output**:
| Struct | Source Location | Total Fields | Covered Fields | Missing Fields | Priority |
|--------|----------------|-------------|---------------|---------------|----------|

**Pass condition**: 核心结构体 0 缺失字段。非核心结构体关键字段覆盖。
**⛔ 完全未分析的核心结构体 → P0。**

---

## Check 04: Document-Code Consistency（文档与代码一致性）

**Execute**:
1. 从文档 Ch3 和 Ch4 提取所有 Rust 类型签名和函数签名。
2. 对每个签名，找对应代码：`rg "fn NAME" os/servers/{module}/src/ --type rust -A 5`
3. 对比：返回类型、参数类型、字段名、方法名。
4. 检查示例代码块是否能编译。

**Output**:
| Doc Signature | Doc Location | Code Location | Return Match? | Params Match? | Fields Match? | Issue |
|--------------|-------------|--------------|--------------|--------------|--------------|-------|

**Pass condition**: 所有签名匹配实际代码。示例代码可编译。
**⛔ SELF-CHECK**: 如果此检查输出 < 3 行，说明跳过了。回去重做。

---

## Check 05: Architecture Evolution（架构演进说明）

**Execute**:
1. 阅读文档 Ch2 和任何对比章节。
2. 检查是否明确说明：32-bit vs 64-bit、页表层级（2→4）、pte 大小（u32→u64）
3. 检查文档是否解释 WHY 某 C 机制在 Rust 中不存在（如 `static_sparepages[BSS]` → Direct Map）

**Output**:
| Aspect | Minix3 (32-bit) | minix-rs (64-bit) | Doc Says? | Match Reality? | Issue |
|--------|----------------|-------------------|-----------|---------------|-------|

**Pass condition**: 32-bit C 和 64-bit Rust 之间的所有架构变化已记录且正确。
**⛔ 缺少架构说明 → P1。**

---

## Check 06: Cross References（交叉引用）

**Execute**:
1. 提取文档所有引用：`rg "\[.*\]\(.*\.md\)" TARGET -n`
2. 提取所有"参见"引用。
3. 对每个引用：链接文件是否存在？
4. 对每个自引用：章节号是否正确？

**Output**:
| Ref Text | Doc Location | Linked File | Exists? | Correct? | Issue |
|----------|-------------|------------|---------|----------|-------|

**Pass condition**: 所有交叉引用解析到现有文件和正确章节。
**⛔ 断链 → P2。错误章节 → P2。**

---

## Check 07: Diagram Quality（图表质量）

**Execute**:
1. 找文档中所有 ASCII 图。
2. 检查：每个图是否对齐（列边界匹配）？
3. 检查：每个图是否必要，还是文字能解释？
4. 检查：标签与周围文字一致。

**Output**:
| Diagram | Doc Location | Aligned? | Necessary? | Labels OK? | Issue |
|---------|-------------|---------|-----------|-----------|-------|

**Pass condition**: 所有图对齐。只有必要的图。
**⛔ 未对齐图 → P2。不必要图 → P2（建议文字替代）。**

---

## Check 08: C Source Coverage（C 源码覆盖完整性）

> **注意**：此检查已被 [Check 00 Coverage Enumeration](#check-00-claims-evidence-tracing论文级质量) 的机器穷举增强。但仍需执行语义范围内的覆盖判定。

**Execute**:
1. 确定语义范围：文档关于什么模块？（从标题/Ch1）
2. 列出 C 源文件：`ls minix3/minix/servers/{module}/*.c`
3. 提取所有函数：`rg "^[a-z_].*\w+\(.*\)\s*$" minix3/minix/servers/{module}/FILE.c -n`
4. 提取所有结构体：`rg "^struct \w+" minix3/minix/servers/{module}/FILE.h -n`
5. 提取行为宏：`rg "^#define \w+" minix3/minix/servers/{module}/FILE.h -n`
6. 对每个符号：是否在文档语义范围内？如果是，是否被覆盖？

**Output**:
| Symbol | Type | C Source | In Scope? | Doc Covers? | Doc Location | Issue |
|--------|------|---------|----------|------------|-------------|-------|

**Coverage**: Total in-scope N / Covered M = M/N%

**Pass condition**: 覆盖率 ≥ 80%（所有核心函数/结构体覆盖）。
**⛔ 核心函数/结构体未覆盖 → P0。覆盖率 < 80% → 整体 P0。**

---

## Check 09: Design Decision Quality（设计决策质量，Ch3 专项）

**Execute**:
1. 从文档 Ch3 提取所有设计决策：`rg "^###|^####" TARGET | grep "3\."`
2. 对每个决策：能否追溯到 Ch1 或 Ch2？
3. 对 Ch2 中每个错误场景：Ch3 是否有对应设计？
4. 检查 no_std：是否有设计依赖 `std::`？

**Output**:
| Decision | Ch3 Loc | Ch1&2 Basis? | Covers Ch2 Errors? | no_std? | Alternative Noted? | Issue |
|----------|--------|-------------|-------------------|---------|-------------------|-------|

**Error path check**:
| Ch2 Error Scenario | Ch2 Loc | Has Ch3 Design? | Issue |
|-------------------|--------|---------------|-------|

**Pass condition**: 每个决策可追溯到 Ch1/Ch2。每个 Ch2 错误有 Ch3 设计。无 std:: 依赖。
**⛔ 无依据设计 → P1。缺失错误场景 → P0。std:: 依赖 → P0。**
**⛔ SELF-CHECK**: "N docs have Ch3, M design decisions checked."

---

## Check 10: Chapter Link Validation（章节链路验证）

**Execute**:
1. **Ch3→Ch1&2**：对每个 Ch3 设计决策，找其在 Ch1 或 Ch2 的支撑依据。
2. **Ch4→Ch3**：对每个 Ch4 实现，找其对应的 Ch3 设计决策。
3. **Tests→Ch3&Ch4**：对每个测试要点，验证它覆盖了 Ch3 决策或 Ch4 实现。
4. **Code→Ch4**：对 Ch4 中每个代码块，验证它存在于实际 Rust 源码。

**Output**:

**Ch3→Ch1&2**:
| Ch3 Decision | Ch3 Loc | Ch1&2 Basis | Link OK? |
|-------------|--------|-----------|---------|

**Ch4→Ch3**:
| Ch4 Implementation | Ch4 Loc | Ch3 Design Basis | Link OK? |
|-------------------|--------|----------------|---------|

**Tests→Ch3&Ch4**:
| Test Point | Target Loc | Covers Design | Covers Impl | Link OK? |
|-----------|-----------|-------------|-----------|---------|

**Code→Ch4**:
| Ch4 Code | Ch4 Loc | Actual Code | Match? |
|----------|--------|------------|--------|

**Pass condition**: 4 种链路类型完整——无孤立决策、无未文档化代码。
**⛔ Ch3 引入 Ch1&2 未有概念 → P1。Ch4 实现未设计 → P1。Ch3 设计未实现 → P1。**

---

## Check 11: Document Style（文档风格，开发记录检测）

**Execute**:
1. `rg "已实现|待实现|未实现|WIP" TARGET -n`
2. `rg "[✅❌🚧⏳🔧]" TARGET -n`
3. `rg "^#+\s*(实现清单|代码状态|现有代码|进度|开发记录|完成情况)" TARGET -n`

**Output**:
| Problem Text | Doc Location | Issue Type | Suggested Fix |
|-------------|-------------|-----------|---------------|

**Rules**:
- ❌ "已实现: X" / "待实现: Z" / "未实现: W" → "X 负责 Y" / "Z 功能处理 W 场景"
- ❌ ✅❌🚧 status emoji → 移除，用文字
- ❌ 进度标题（"实现清单"、"代码状态"）→ 重命名为中性
- ✅ TODO 标记允许（标记计划工作，非完成状态）

**Pass condition**: 零开发记录语言。>5 实例 → 系统性重写（P1）。
**⛔ Status emoji → P1。进度标题 → P1。"已实现/待实现" → P1。**

---

## Check 12: Skip/Fake Check Detection（跳过/虚假检查检测，Meta-Check）

> 在所有其他检查完成后、报告前执行。

**Execute**:
1. 回顾本次会话产生的所有检查输出。
2. 统计每个检查输出了多少行（排除检查头）。
3. 对任何零输出检查：很可能是被跳过/虚假执行。

**决策树**:
| Check | Zero Output? | Action |
|-------|-------------|--------|
| 00 Claims-Evidence | YES | **立即重跑**。提取所有事实声明，grep 每个找证据。 |
| 01 Concept accuracy | sampled < 100% | **扩展采样**。 |
| 04 Doc-code consistency | YES | **立即重跑**。对比 Ch4 签名与实际 .rs。 |
| 09 Design quality | YES | **立即重跑**。提取 Ch3 决策，验证 Ch1&2 依据。 |

**Output**:
| Check# | Name | Zero Output? | Action Taken |
|--------|------|-------------|---------------|

**Pass condition**: 所有检查至少 1 行证据。
**⛔ 无法重跑 → 写 "NOT RE-RUN: <reason>"。禁止无声跳过。**

---

## Check 13: Ch1 Mandatory Skeleton（Ch1 强制骨架检查）

> **Purpose**: Ch1 must be concept-driven (architecture perspective), not implementation-driven (code perspective). Ch1 subject = CPU/OS, not function name.

**Execute**:
1. Read Ch1 first paragraph. Identify the **subject** (主语) of the first sentence.
   - ✅ CPU / OS / 系统 / 进程 / 调度器 → architecture perspective
   - ❌ 函数名 / 结构体名 / trait 名 → implementation-driven → P1 (pattern 51)
2. Extract Ch1 H2 outline. Check if outline is organized by:
   - ✅ CPU 需要回答的问题 / 系统机制 / 概念层次
   - ❌ trait 名 / 函数名 / 结构体名 → P1 (pattern 51)
3. Check Ch1 driving direction:
   - ✅ WHY→WHAT→HOW (concept first)
   - ❌ HOW-only (jumps to implementation) → P1
4. For multi-arch docs: check if unified abstraction appears before arch-specific details.
   - ❌ Arch-specific first, no unified abstraction → P1 (pattern 53)

**Output**:
| Aspect | Finding | Verdict |
|--------|---------|---------|
| Ch1 subject | CPU / OS / 函数名 / 其他 | ✅/P1 |
| Ch1 outline organization | 架构视角 / 代码视角 | ✅/P1 |
| Driving direction | WHY→WHAT→HOW / HOW-only | ✅/P1 |
| Multi-arch unified abstraction | Yes / No / N/A | ✅/P1 |

**Pass condition**: Ch1 subject = CPU/OS, outline = architecture perspective, direction = WHY→WHAT→HOW.
**⛔ Ch1 subject = function name → P1 (pattern 51). Ch1 outline = function names → P1.**

---

## Check 14: Explanation Causal Chain Validation（解释因果链验证，P0）

> **Purpose**: claim 有 evidence 不够，解释 claim 的 BECAUSE 部分必须技术上正确。
> "因为 X 所以 Y" 中的 X 必须是真实机制，不是听起来合理的编造。

**Execute**:
1. Extract all **causal explanations** in doc (sentences with "因为...所以...", "由于...因此...", "X 是必要的，因为...", "This is required because...").
2. For each causal explanation:
   - Identify the **claim** (Y) and the **cause** (X).
   - Verify X is a **real mechanism** (grep C source or Rust source for evidence).
   - Check if X technically leads to Y (not just plausible-sounding).

**Output**:
| Causal Explanation | Doc Location | Claim (Y) | Cause (X) | X Verified? | X→Y Technically Correct? | Verdict |
|-------------------|-------------|-----------|-----------|-------------|-------------------------|---------|

**Example**:
```
❌ 错误：claim 正确但解释的因果链技术上错误
   "memcpy(&kinfo, local_cbi, ...) 是必要的" → 正确
   解释："调用链推进后栈帧被覆盖" → 错误（C 语义上 kmain 未返回时栈帧存在）
✅ 正确解释："local_cbi 作用域限于 kmain 调用链，非 kmain 链代码（如中断处理）需通过 kinfo 访问"
```

**Pass condition**: All causal explanations have verified causes and technically correct X→Y links.
**⛔ Causal chain fabrication (X is wrong or X→Y is technically wrong) → P0 (pattern 48).**

---

## Check 15: Author Intent Transparency（作者意图透明性）

> **Purpose**: Design decisions (Ch3) must explain WHY, not just list WHAT. Reader should understand the author's reasoning, not just the outcome.

**Execute**:
1. Extract all Ch3 design decisions (`rg "^###|^####" TARGET | grep "3\."`).
2. For each decision, check if it includes:
   - **Problem**: What problem does this decision solve?
   - **Alternatives**: What alternatives were considered?
   - **Rationale**: Why this choice over alternatives?
   - **Trade-off**: What are the costs/limitations?
3. Decisions that only state "我们选择 X" without rationale → P1 (pattern 56).

**Output**:
| Ch3 Decision | Doc Location | Problem? | Alternatives? | Rationale? | Trade-off? | Verdict |
|-------------|-------------|---------|--------------|-----------|-----------|---------|

**Pass condition**: Each Ch3 decision has Problem + Rationale at minimum. Alternatives + Trade-off recommended for excellence.
**⛔ Ch3 decision without rationale → P1 (pattern 56). Ch3 as decision log (list without WHY) → P1.**

---

## Design-First References

> Profile R / Profile C / Profile I / Profile H-K review 时，额外加载以下 Design-First 检查项。

### §1.5 Design 对齐维度（5 项）

| # | 检查项 | 通过条件 | 失败后果 |
|---|-------|---------|---------|
| 1 | doc 与 design 命名一致 | doc 命名前缀/术语与 design §X 一致 | P0-design-deviation |
| 2 | doc 引用的 trait 在 design 有完整定义 | doc 提到的 trait 方法签名在 design §X 可找到 | P0-design-missing（Pattern 63） |
| 3 | doc 描述的不变量在 design 有声明 | doc 中的不变量有 design §X 引用 | P0-design-missing |
| 4 | doc 描述的架构演进标注 ARCH | Minix3 行为偏离处有 `[ARCH: ...]` 标记 | P0-design-wrong |
| 5 | doc 引用 design 章节 | doc 关键决策有 `06-design.md §X`（非 bagging）/ `06-design-final.md §X`（bagging）引用 | P1（链路断裂） |

### §2.0.5 Design 引用规范（4 种格式）

| 格式 | 用途 | 示例 |
|------|------|------|
| `设计决策：[design §X.Y]` | 决策依据 | 设计决策：使用 trait 抽象（[design §3.2]） |
| `详细：[design §X.Y]` | 详细描述 | 详见 [design §3.2 trait ArchInterruptController] |
| `约束：[design §X.Y]` | 约束/不变量 | 约束：IRQ handler 不可睡眠（[design §4.1]） |
| `[ARCH: ...]` | 架构演进标注 | `[ARCH: APIC 替代 PIC，详见 design §5.3]` |

**判定规则**：
- doc 提到 trait/方法/不变量但无 design 引用 → P0-design-missing
- doc 引用了 design 但 design 章节不存在 → P0-design-deviation
- `[ARCH: ...]` 格式错误 → P0-design-wrong

详见 [review-doc-skill.md §2.0.5](../../../../prompt/skill/review-doc-skill.md) + [review-patterns-skill.md §X.5 Pattern 63 Design-Missing](../../../../prompt/skill/review-patterns-skill.md)。
