# code: 代码正确性检查（合并 16 个 code check）

> 本文件合并原 code/01-16 共 16 个检查项，解决 attention decay 和过度拆解问题。
> **强制规则**：每个检查必须先执行 grep/read，再下结论。每个判定标注 evidence [DIRECT/MEDIUM/INFERRED]。

---

## 通用规则（适用于所有 code check）

1. **先读后判**：每个判定必须先执行 grep 验证，再下结论。禁止凭印象判断。
2. **Evidence 分级**：
   - [DIRECT] grep 直接验证（有明确输出）
   - [MEDIUM] 名称匹配但语义需进一步确认
   - [INFERRED] 基于上下文推断（需标注"待确认"）
3. **零输出禁令**：如果检查发现 0 个问题，必须写"Checked N items, found 0 issues"。禁止留空。
4. **证据强制**：每个检查必须包含 grep count、file count 或 line number range 作为证据。
5. **Attention Decay 警告**：code check 数量多，后半段易被跳过。每完成 5 个 check 后暂停，重读规则 #2 和 #3。

---

## Check 01: Rewrite Quality（Rewrite 质量）

**Execute**:
阅读目标 `.rs` 文件。检查这些"Translate"气味（每个 = P1）：
1. 裸整数应为 newtype/enum：`rg "fn \w+\(.*u32.*\)" FILE.rs` / `rg "fn \w+\(.*i32.*\)" FILE.rs`
2. C 风格哨兵值：`rg "\s0\b.*no|none|empty" FILE.rs -i` / `rg "\-1" FILE.rs`
3. 未命名魔术数字：`rg " [0-9]{4,} " FILE.rs`
4. C 风格标志组合：`rg "\| " FILE.rs`（检查是否未用 bitflags）
5. C 风格错误码传递：`rg "-> i32" FILE.rs` 和 `rg "-> Result" FILE.rs`
6. C 宏字面翻译为 Rust 宏：`rg "macro_rules!" FILE.rs`

**Output**:
| Location | Code | Smell Type | P? | Suggested Fix |
|----------|------|-----------|----|---------------|

**Pass condition**: 零 Translate 气味。错误码严格匹配 Minix3 errno。
**⛔ 自造错误码 → P0。**

---

## Check 02: Hardware Abstraction（硬件抽象）

**Execute**:
1. `rg "CR3|cr3|TSS|MSR|PTE|PDE" FILE.rs -i` → 如果找到且不在 `//!` 文档注释内 → P0
2. `rg "#\[cfg\(target_arch" FILE.rs` → P0（应用 trait 静态分派）
3. `rg "struct.*PageTable|struct.*Paging" FILE.rs` → 检查：暴露硬件字段？→ P0

**Output**:
| Location | Code | Violation | P? | Suggested Fix |
|----------|------|----------|----|---------------|

**Pass condition**: OS 层无硬件寄存器名。无 `#[cfg(target_arch)]` 选择行为。所有硬件交互通过 trait。
**⛔ OS 代码中硬件寄存器 → P0。#[cfg(target_arch)] 选择行为 → P0。**

---

## Check 03: Trait Design Quality（trait 设计质量）

**Execute**:
1. `rg "^trait \w+" FILE.rs` — 列出所有定义的 trait
2. 对每个 trait 检查 3 个问题：
   - **多态性**：是否有 ≥2 个**行为不同**的实现？`rg "impl TRAIT_NAME for" FILE.rs`
   - **Trait bound**：是否曾被用作泛型约束？`rg "TRAIT_NAME" FILE.rs -n` 找 `<T: TRAIT_NAME>`
   - **单方法？**：如果只有 1 个方法，能否用自由函数替代？
   - **机制 vs 策略**：描述机制（映射页）还是策略（绑定进程）？

**Output**:
| Trait | File | ≥2 Impls? | Used as Bound? | Methods | Mechanism/Policy? | Verdict |
|-------|------|----------|---------------|---------|-------------------|--------|

**Pass condition**: 每个 trait 有 ≥2 行为不同的实现 AND 被用作 bound。
**⛔ 不必要 trait → P1。策略 trait → P1。单方法 trait → P1。**
**⛔ SELF-CHECK**: 列出 `rg "^trait"` 找到的每个 trait，每个填一行。0 个则写 "rg '^trait': 0 matches across N files"。

---

## Check 04: Type Safety（类型安全）

**Execute**:
1. `rg "unsafe" FILE.rs -n` — 每个：有 SAFETY 注释？有测试？
2. `rg "MaybeUninit|UnsafeCell" FILE.rs -n` — 用法正确？（MaybeUninit 用于后初始化，UnsafeCell 用于内部可变性）
3. 检查 `unsafe impl Send` / `unsafe impl Sync`：安全论据是否文档化？
4. `rg "Drop" FILE.rs -n` — Drop 语义清晰？隐式 drop 风险？
5. 检查 typestate：如果用了 typestate，非法状态转换是否不可能？

**Output**:
| Location | Unsafe Usage | Safety Comment? | Test? | Issue |
|----------|-------------|----------------|-------|-------|

**Pass condition**: 每个 `unsafe` 块有 SAFETY 注释。无 aliasing UB。Drop 显式。
**⛔ 无安全文档的 unsafe → P0。aliasing UB 风险 → P0。**

---

## Check 05: Execution Model & Concurrency（执行模型与并发）

**先确定模块类型**：是**用户态服务器**（VM/PM/VFS/RS/DS/INET）还是**内核**代码？

### A: 用户态服务器检查
1. 模块是否假设单线程执行？
2. `rg "impl.*(Send|Sync)|unsafe impl.*(Send|Sync)" FILE.rs -n` — 有正当理由？
3. `rg "lazy_static|OnceCell|AssumeSyncCell" FILE.rs -n` — 初始化顺序正确？
4. `rg "UnsafeCell" FILE.rs -n` — 单线程假设下安全？
5. `rg "RefCell|Rc" FILE.rs -n` — 单线程事件循环正确？

### B: 内核 SMP/BKL 检查
1. `rg "BKL_LOCK|BKL_UNLOCK|big_kernel_lock" FILE.rs -n` — 所有内核入口路径持有 BKL？
2. `rg "ipc_sendrec|schedule|sleep|block" FILE.rs -n` — 这些在 BKL_LOCK/BKL_UNLOCK 内调用？→ P0（spinlock 不能阻塞）
3. `rg "RefCell|Rc" FILE.rs -n` — 如果找到，每个是否标注 `// per-CPU` 或 `/// SAFETY: Caller holds BKL`？→ 跨 CPU 无保护 → P0
4. `rg "static mut|static" FILE.rs -n` — 每个全局变量有明确并发策略（BKL / `Atomic*` / per-CPU）？
5. `rg "get_cpu_var|put_cpu_var" FILE.rs -n` — 配对正确？无跨 CPU 读本地数据？
6. `rg "UnsafeCell" FILE.rs -n` — 安全论据必须是 BKL 保护或 per-CPU 隔离，**不是**"单线程"

**Output**:
| Location | Type | Assumption | Safe? | Issue |
|----------|------|-----------|-------|-------|

**Pass condition**:
- 用户态：单线程假设显式。所有 Send/Sync 实现有理由。
- 内核：所有共享数据有 BKL/Atomic/per-CPU 保护。spinlock 内无阻塞。
**⛔ 内核：Rc/RefCell 跨 CPU 无保护 → P0。内核路径不持 BKL → P0。BKL 内 sleep/schedule/IPC → P0。**
**⛔ SELF-CHECK**: 报告每子节的 grep count。0 命中也写 grep 命令 + "0 matches"。始终在输出中标识模块类型。

---

## Check 06: Memory Model（内存模型）

**Execute**:
1. `rg "MaybeUninit" FILE.rs -n` — 检查：仅用于未初始化槽（正确），还是"逻辑无效"状态（错误）？
2. `rg "manually_drop|ManuallyDrop" FILE.rs -n` — 正确？
3. Drop 用于资源释放（正确）还是状态管理（错误）？
4. 检查 alloc/free 配对：找每个 `alloc_` 调用，验证对应 `free_` 调用。

**Output**:
| Location | Pattern | Correct? | Issue |
|----------|---------|---------|-------|

**Pass condition**: 无未初始化内存读取 UB。所有 alloc/free 配对。Drop 仅用于资源释放。
**⛔ 读取 MaybeUninit → P0。alloc 无对应 free → P0。**

---

## Check 07: Module Design & pub Hygiene（模块设计与 pub 卫生）

**Execute**:
1. `rg "^pub " FILE.rs -n | head -30` — 列出所有 pub 项
2. 对每个 pub 项问："这个 pub 是因为外部代码需要，还是因为我懒？"
   - 检查跨模块引用：`rg "FILE::ITEM" os/servers/{module}/src/ -n`
   - 如果仅 crate 内使用：`pub(crate)` 足够。
3. `rg "^pub struct" FILE.rs` — 内部字段 `pub`？→ P1
4. 检查模块内聚：这个文件做一件事，还是很多事？

**Output**:
| pub Item | Location | Used Where? | Should be? | Issue |
|----------|---------|------------|-----------|-------|

**Pass condition**: 最小 pub 表面。无内部字段暴露。模块有清晰单一职责。
**⛔ 不必要 `pub` → P1。内部字段 `pub` → P1。**

---

## Check 08: Naming & Traceability（命名与可追溯性）

**Execute**:
1. `rg "fn [a-z_]+" FILE.rs -n` — 检查：snake_case？
2. `rg "struct [A-Z]" FILE.rs -n` — 检查：CamelCase？
3. 对每个函数名，验证：能否 grep 到 C 等价物？`rg "FUNC_NAME" minix3/minix/servers/{module}/ --type c -n`
4. 参数名：`clicks` 是否保持为 `clicks`（不是 `count` 或 `pages`）？

**Output**:
| Location | Name | Convention? | C Traceable? | Issue |
|----------|------|-----------|-------------|-------|

**Pass condition**: 所有公开名 snake_case/CamelCase。核心函数可在 C 源码 grep。
**⛔ 非标准命名 → P2。无法在 C 源码 grep → P2。**

---

## Check 09: Testing（测试）

**Execute**:
1. 找测试模块：`rg "#\[cfg\(test\)\]" FILE.rs -n`
2. 对每个 `unsafe` 函数/块，检查：是否有测试验证安全契约？
3. 检查测试覆盖：正常路径、边界条件、无效输入、状态转换。
4. 检查：无仅测试标准库行为的测试。
5. `rg "fn test_" FILE.rs` 统计测试数。

**Output**:
| Test | Doc Location | Covers What? | Type | Issue |
|------|-------------|-------------|------|-------|

**Pass condition**: 每个 `unsafe` 函数至少 1 个测试。核心功能有 normal+boundary+error 测试。
**⛔ unsafe 缺安全测试 → P1。零测试 → P1。**

---

## Check 10: Comments & Documentation（注释与文档）

**Execute**:
1. `rg "^pub fn" FILE.rs -A 1` — 每个有 `///` 文档注释？→ 否则 P1
2. `rg "^pub\(" FILE.rs -A 1` — pub(crate) 非显然函数有注释？
3. 检查模块级：文件是否以 `//!` 模块文档开头？
4. `rg "unsafe fn" FILE.rs -A 2` — 每个有 `/// # Safety` 段？
5. `rg "//.*Minix|//.*C:|//.*main\.c" FILE.rs` — 验证这些引用正确。

**Output**:
| Location | Missing What? | P? | Suggested Fix |
|----------|-------------|----|---------------|

**Pass condition**: 每个 `pub fn` 有 `///`。每个 `unsafe fn` 有 `# Safety`。模块有 `//!`。
**⛔ 缺 pub 文档 → P1。缺安全文档 → P1。注释中错误 C 引用 → P1。**
**⛔ SELF-CHECK**: 报告 pub fn count, pub(crate) fn count, pub struct count, pub type count。每类：X 有文档，Y 缺失。

---

## Check 11: 64-bit Assumptions（64 位假设）

**Execute**:
1. `rg "\bu32\b" FILE.rs -n` — 是否有 `u32` 用于地址/大小而应用 `u64`？
2. `rg "\bas u32\b" FILE.rs -n` — 任何 u64 截断？检查是否有安全注释。
3. `rg "\bas u16\b" FILE.rs -n` — 同样检查。
4. 代码是否利用 64 位优势？（更大地址空间、direct map）

**Output**:
| Location | Truncation | Safe? | Comment? | Issue |
|----------|-----------|-------|---------|-------|

**Pass condition**: 无无安全注释的 u64 截断。
**⛔ 无注释的不安全 `as` 截断 → P0。地址用 u32 → P1。**
**⛔ SELF-CHECK**: 报告 `as u32` 数量、`as u16` 数量、无注释数量。

---

## Check 12: Complexity & Engineering Judgment（复杂度与工程判断）

**Execute**:
1. `rg "struct.*\{.*\{.*\{|impl.*for.*where.*where" FILE.rs` → 深层嵌套泛型 → P1
2. 统计每函数行数：任何函数 >80 行 → 检查可否重构 → P2
3. `rg "macro_rules!" FILE.rs` → 任何可以是函数或 trait 的宏？→ P1
4. 设计是否比 C 原版更复杂？读 C 源码对比验证。

**Output**:
| Location | Issue | Complexity Type | P? | Suggested Fix |
|----------|-------|----------------|----|---------------|

**Pass condition**: 无不必要抽象。无函数显著比 C 等价物更复杂。
**⛔ 无收益的过度工程 → P1。**
**⛔ SELF-CHECK**: 报告函数数、>80 行函数数、可以是函数的宏数、>3 类型参数的 trait 数。

---

## Check 13: no_std Compliance（no_std 约束）

**Execute**:
1. `rg "use std::" FILE.rs -n` — 排除 `#[cfg(test)]` 块
2. `rg "#!\[no_std\]" Cargo.toml` — 验证 crate 级属性
3. `rg "std::collections::HashMap" FILE.rs -n` → 必须用 `alloc::collections::BTreeMap` 或 `hashbrown`
4. crate 是否需要 `alloc`？全局分配器是否配置？`rg "global_allocator" FILE.rs`

**Output**:
| Location | Code | Violation | P? | Suggested Fix |
|----------|------|----------|----|---------------|

**Pass condition**: `#[cfg(test)]` 外零 `use std::`。有 `#![no_std]`。全局分配器配置。
**⛔ 生产代码 std import → P0。**

---

## Check 14: Design-Code Consistency（设计-代码一致性）

**Execute**:
1. 找此 `.rs` 文件对应的文档（`notes/rewrite/` 中同模块）。
2. 从文档提取 Ch3 设计决策。
3. 对比每个决策与实际 Rust 代码：
   - 决策说"用 typestate"但代码用裸 int？→ P0
   - 决策说"用 enum"但代码用常量？→ P1
4. 提取 Ch4 实现描述。对比签名、类型、语义与实际代码。

**Output**:
| Design Decision | Doc (Ch3) | Code | Match? | P? |
|----------------|----------|------|--------|----|

**Pass condition**: 每个 Ch3 设计反映在代码中。每个 Ch4 描述匹配代码。
**⛔ 设计说 typestate 代码用 int → P0。设计说 enum 代码用 const → P1。**

---

## Check 15: C-Rust Semantic Alignment（C-Rust 语义对齐）

**Execute**:
对目标 `.rs` 中每个函数，找对应 C 函数：
1. 先检查**叶函数**（无内部依赖，直接操作数据）。
2. 对每个叶函数：行为与 C 相同？返回值、错误码、副作用？
3. 如果行为不同，检查：是否文档化了 WHY（架构演进 / C bug 修复）？
4. `rg "// Minix|// C:|main\.c:" FILE.rs` → 验证注释中 C 源码引用正确。

**Output**:
| Rust Function | C Function | C Source | Behavior Match? | Diff Documented? | Issue |
|--------------|-----------|---------|----------------|-----------------|-------|

**Pass condition**: 所有叶函数对齐。任何分歧有注释引用 C 源码 + 原因。
**⛔ 叶函数不对齐 → P1。未文档化分歧 → P1。注释中错误 C 引用 → P1。**

---

## Check 16: Precision Check（细节精确检查）

> 大方向检查之后的第二层。5 个元规则跨阶段复用。Agent 标记可疑点，人工确认。不自动判定正确性。

### 16.1 External Knowledge Verifiability
- `rg "CR4|PSE|MSR|LSTAR|PDPT|PAE|granule|Sv39|Sv48|UEFI|ACPI|SMBIOS" FILE -i -n`
- 对每个匹配：注释中的硬件/协议声明在当前上下文准确？
- 旧特性在新模式？（如 long mode 中的 PSE）

### 16.2 Universal Interface Purity
- `rg "^pub struct" FILE`
- 对每个 pub struct：检查每个字段对所有消费者（所有架构、所有模块类型）有意义？
- 架构特定/模块特定字段：是否标记？（`Option<T>`、注释 "x86-64 only"、或移到扩展 struct）

### 16.3 Return Value Completeness
- `rg "let _\w+ = |let _ = " FILE -n`
- 对每个被忽略的返回值：有注释说明为什么安全忽略？
- 检查外部调用（固件、syscall、trait 方法）——关键返回值（内存映射、状态码）被丢弃无说明？

### 16.4 Resource Lifecycle Closure
- `rg "Box::leak|mem::forget|ManuallyDrop|into_raw" FILE -n`
- 对每个资源泄漏：有释放路径或显式"不释放"理由？
- 谁负责释放？何时？如果"永不"，为什么？

### 16.5 Reason Questionability
- `rg "// .*因为|// .*由于|// .*避免|// .*为了|// .*需要|// .*防止" FILE -n`
- 对每个"为什么"注释：理由在上下文中成立？是唯一/诚实的理由？
- 性能/安全/大小声明：有量化支撑？

**Output**:
| Meta-Rule | Location | Suspicious Content | Human Check Needed |
|-----------|----------|-------------------|-------------------|

**Pass condition**: 5 个维度全部扫描。任何可疑点被标记，不自动判定。
**⛔ 未发现可疑点 = 偷懒检查。每个匹配必须列出。**
