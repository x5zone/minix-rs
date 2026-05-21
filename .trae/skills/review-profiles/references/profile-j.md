# Profile J: Stage 3 — Rust Code Quality Verification

**Objective**: Verify Rust code quality across all 15 code review dimensions.
**Rules**: ~70 items
**Prerequisite**: Stage I output (Ch3 design confirmed correct, Ch4 confirmed accurate)
**Input**: Rust code + Stage I output
**Output**: P0 UB/semantic drift + P1 type safety issues + P1 trait design issues

## Preparation

```bash
# List all Rust source files
ls src/{module}/*.rs

# Extract all pub items
rg "^pub " src/{module}/ --type rust -n

# Extract all unsafe blocks
rg "unsafe" src/{module}/ --type rust -n

# Extract all C code references in comments
rg "minix3/" src/{module}/ --type rust -n

# Extract all std usage (check for violations)
rg "use std::" src/{module}/ --type rust -n
```

## Check Dimensions

For each dimension, load the corresponding reference file from `review-code` for detailed checklists when needed.

### J1: Rewrite Quality (P0/P1)
> Load [review-code error-patterns.md](../../review-code/references/error-patterns.md) and [dont-oversimulate.md](../../review-code/references/dont-oversimulate.md) for detailed patterns.

- Bare integers for semantics → P0 (Pattern 15)
- C-style null/sentinel values → P1 (Pattern 16)
- Unnamed magic numbers → P2
- C-style flag combinations → P1
- C-style error code passing → P1
- C macros directly translated → P1
- C-style data structures → P1
- Memory ownership unclear → P1
- Over-simulating C limitations → P1

### J2: Hardware Abstraction (P0)
- Hardware registers/PTE bits in upper-layer code → P0 (Pattern 20)
- `#[cfg(target_arch)]` for hardware behavior → P0
- Upper layer depends only on trait interface? ✅
- OS semantic types separated from hardware encoding? ✅
- Trait has ≥2 behaviorally different implementations? (Pattern 24)
- Trait used as generic constraint?
- Mechanism vs policy separation?

### J3: Type Safety (P1)
- Typestate truly constrains state (no bypass)?
- Can "logically illegal but type-legal" states be constructed?
- `unsafe` minimized with safety contracts? (Pattern 17)
- Manual `unsafe Send`/`unsafe Sync` — safety argued?
- Drop semantics clear, no implicit drop risks?
- No excessive typestate over-engineering? (Pattern 23)

### J4: C-Rust Semantic Alignment (P1)
> Load [review-code alignment-detail.md](../../review-code/references/alignment-detail.md) for §14.5/14.7 rules.

**J4a: 叶函数语义对齐 (§14.2)** — 逐函数分级检查

| 函数 | C 返回值 | Rust 返回值 | C 错误码 | Rust 错误类型 | C 副作用 | Rust 副作用 | 对齐? | 优先级 |
|------|---------|-----------|---------|-------------|---------|-----------|------|--------|

- Leaf function return value semantics match C?
- Leaf function error codes match C? (Pattern 18)
- Leaf function side effects match C?

**J4b: C 有但 Rust 缺失 (§14.3)** — C 回调覆盖检查

| C 回调 | C 文件 | Rust trait 方法 | Rust 实现? | 缺失影响 | 优先级 |
|--------|-------|---------------|----------|---------|--------|

- C callback exists but Rust trait doesn't override it → P1?
- C 全局实例中的回调字段在 Rust 中无对应 → P1

**J4c: 架构级不对齐 (§14.5)** — 需注释说明

| 不对齐项 | C 行为 | Rust 行为 | 注释说明? | 优先级 |
|---------|--------|---------|----------|--------|

- Architecture-level mismatch annotated with rationale?
- Architecture evolution function disappearance properly categorized? (§14.5)

**J4d: 修正性不对齐 (§14.6)** — C bug 修正需注释

| 修正项 | C 行为(bug) | Rust 行为(修正) | 注释说明? | 优先级 |
|--------|-----------|---------------|----------|--------|

- Corrective mismatch (C bug) annotated with explanation?
- 修正性不对齐未标注 → P1（读者无法区分"设计差异"和"bug 修正"）

### J5: no_std Compliance (P0)
- `use std::` in non-test code → P0 (Pattern 21)
- `Cargo.toml` has `#![no_std]`?
- Third-party crates requiring `std`?
- `alloc` usage has global allocator?
- Test/mock code isolated with `#[cfg(test)]`?

### J6: Module Design (P1)
- `pub` restrained, minimum privilege? (Pattern 22)
- Modules high cohesion, clear responsibilities?
- Private APIs incorrectly exposed?
- Module visibility layering: internal relaxed, external strict?

### J7: Naming & Traceability (P1)
- Rust conventions followed? (snake_case, CamelCase)
- Consistent with Minix3 names (prioritize same for cross-referencing)?
- Bidirectional searchability (grep C name finds Rust code)?
- Parameter names consistent with Minix3 (`clicks`, not `count`)?

### J8: Comment Quality (P1)
- **Comments in English** (principle)
- Every `pub` item has `///` doc comment?
- `pub(crate)` with non-self-explanatory semantics has comment?
- Every module has `//!` doc comment?
- Complex algorithms have inline "why" comments?
- `unsafe` blocks have safety comments?

**J8a: C 源码引用注释 (§14.4)** — 验证注释中的 C 引用正确性

| Rust 函数 | 注释中的 C 引用 | C 文件:行号 | 引用正确? | 优先级 |
|----------|---------------|-----------|----------|--------|

- Rust comment C source references correct?
- 注释中引用的 C 函数名与源码一致?
- 注释中引用的行号与源码偏差 <5?
- 关键语义对齐处有 C 源码引用注释?

### J9: 64-bit Assumptions (P1)
- No unnecessary 32-bit remnants (`u32` for address/size)?
- Lossy `as` casts have safety comments? (Pattern 19)

### J10: Architecture Evolution Compliance (P1)
> Load [review-code allowed-evolution.md](../../review-code/references/allowed-evolution.md) for full table.

All four criteria must hold:
1. External observable behavior unchanged
2. IPC protocol unchanged
3. Lifecycle semantics unchanged
4. Scheduling/permission/address space semantics unchanged

### J11: Execution Model & Concurrency (P0/P1)
> Load [review-code exec-model.md](../../review-code/references/exec-model.md) for full checklist.

- Event loop matches C's get_work()/reply()? (P0)
- Single-threaded: no `Mutex`, `Arc`, `Atomic`? (P1)
- IPC-based concurrency, no shared mutable state? (P1)
- Blocking patterns consistent with Minix3 model? (P1)
- Process lifecycle matches C? (P1)
- No concurrency bugs (data race, re-entrant handler, deadlock)? (P0)

### J12: Memory Model & State Expression (P1)
> Load [review-code memory-model.md](../../review-code/references/memory-model.md) for full checklist.

- Phys/Virt address separation via newtype?
- State machines explicit, all transitions unambiguous?
- Invalid states unrepresentable at compile time?
- Ownership clear, each allocation has Owner with Drop?

### J13: Tests (P1)
> Load [review-code test-checklist.md](../../review-code/references/test-checklist.md) for full checklist.

- Unit tests cover leaf functions, error paths, boundary conditions?
- Integration tests cover IPC scenarios, end-to-end flows?
- Test isolation: `#[cfg(test)]`, no `std::` leaks?
- Property-based / fuzz tests for complex state machines? (P2)
- Tests mirror C behavior for C-Rust alignment?

### J14: Complexity & Engineering (P1)
> Load [review-code complexity.md](../../review-code/references/complexity.md) for full checklist.

- No unnecessary abstraction (deep traits, single-impl traits)?
- Platform code isolated in arch/arch64/?
- Macro hygiene: no logic-hiding macros?
- Feature flags used correctly?
- Dependencies justified, no `std`-requiring deps?
- No code duplication?

### J15: Design-Code Consistency (P1)

Given Stage I confirmed Ch3/Ch4 correctness:
- Ch3 says typestate → code uses typestate?
- Ch3 says enum → code uses enum (not bare ints)?
- Ch4 says function returns X → code returns X?
- Ch4 says error handling covers N cases → code handles N cases?

## Output Format

```markdown
# Profile J: Code Quality — {module}

## Summary
- Files reviewed: {N}, total lines: {M}
- Issues: P0={N}, P1={M}, P2={K}

## Dimension Coverage
| Dim | Name | Executed? | Result | Issues | Time |
|-----|------|-----------|--------|--------|------|
| J1 | Rewrite quality | ✅ | Pass/Warn/Fail | {N} | {min} |
| J2 | Hardware abstraction | ✅ | Pass/Warn/Fail | {N} | {min} |
| J3 | Type safety | ✅ | Pass/Warn/Fail | {N} | {min} |
| J4 | C-Rust alignment | ✅ | Pass/Warn/Fail | {N} | {min} |
| J5 | no_std compliance | ✅ | Pass/Warn/Fail | {N} | {min} |
| J6 | Module design | ✅ | Pass/Warn/Fail | {N} | {min} |
| J7 | Naming | ✅ | Pass/Warn/Fail | {N} | {min} |
| J8 | Comments | ✅ | Pass/Warn/Fail | {N} | {min} |
| J9 | 64-bit assumptions | ✅ | Pass/Warn/Fail | {N} | {min} |
| J10 | Architecture evolution | ✅ | Pass/Warn/Fail | {N} | {min} |
| J11 | Execution model | ✅ | Pass/Warn/Fail | {N} | {min} |
| J12 | Memory model | ✅ | Pass/Warn/Fail | {N} | {min} |
| J13 | Tests | ✅ | Pass/Warn/Fail | {N} | {min} |
| J14 | Complexity | ✅ | Pass/Warn/Fail | {N} | {min} |
| J15 | Design-code consistency | ✅ | Pass/Warn/Fail | {N} | {min} |

## P0 Issues (Must Fix)
| # | Dimension | Location | Description | Evidence | Fix |

## P1 Issues (Should Fix)
| # | Dimension | Location | Description | Evidence | Fix |

## P2 Issues (Optional)
| # | Dimension | Location | Description | Evidence | Fix |

## Verdict
- Code quality: 🟢 High / 🟡 Acceptable / 🔴 Needs work
- Ready for Stage K (Profile K): ✅ Yes / ⚠️ Fix P0 first
```