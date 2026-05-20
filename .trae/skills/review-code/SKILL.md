---
name: review-code
description: >
  Review Minix-RS Rust code for rewrite quality, type safety, hardware abstraction,
  no_std compliance, and C-Rust semantic alignment. Invoke when user asks to review
  .rs files, check code quality, or validate trait designs.
---

# Review Code

Review Minix-RS project Rust code for rewrite quality, type safety, hardware abstraction, no_std compliance, and C-Rust semantic alignment.

## When to Use

- User asks to "review xxx.rs" or "check code quality"
- User asks to validate trait designs or hardware abstraction
- User asks to check C-Rust semantic alignment
- User asks to verify no_std compliance

Do NOT use when:
- User only wants to review documentation (use `review-doc` skill)
- User wants cross-validation between documentation AND code (use `review-cross` skill)

## Core Principles

### Rewrite, Not Translate

- **Translate** (1:1 syntax conversion): ❌ Forbidden
- **Rewrite** (preserve observable behavior, re-express with Rust type system): ✅ Goal
- **Redesign** (change architecture/mechanisms/protocols): ❌ Currently forbidden

Turn "implicit encoding" into "explicit protocol". External behavior unchanged, internal expression rethought.

### Ground Truth Priority

```
Minix3 source behavior  >  Documentation description  >  Rust implementation  >  AI analysis
```

### Execution Model Assumptions

Current VM/PM/VFS servers are **single-threaded event loop**, **no shared memory concurrent modification**, **no SMP parallel access**. Therefore: `Rc` over `Arc`, `RefCell` over `Mutex`, `!Send`/`!Sync` are reasonable.

### no_std Constraint

**Available**: `core`, `alloc` (with global allocator), custom crates.
**Forbidden**: `std`, `std::sync`, `std::collections`, `std::io`/`std::fs`, `std::thread`.
**Exception**: `#[cfg(test)]` and mock code may use `std`.

## Execution Steps

### Step Dependency Graph

Steps have **logical dependencies** — later steps rely on earlier steps' outputs.

```
Step 0 (Scope) → Step 1 (Rewrite Quality) → Step 2 (Hardware Abstraction)
    → Step 3 (Type Safety)

Step 4 (C-Rust Semantic Alignment)  ◄── depends on Step 1 (translate smell) + Step 2 (hw leak)
Step 5 (no_std Compliance)          ── independent P0 gate
Step 6-9 (Module/Naming/Comment/64bit) ── sequential, no cross-dependency
Step 10 (Architecture Evolution)    ◄── depends on Step 4 (alignment context)
Step 11-14 (ExecModel/Memory/Test/Complexity) ── sequential, no cross-dependency
Step 15 (Action Item Generation)
Step 16 (Self-Check Confirmation)
Step 17 (Final Output)
```

**Key dependencies**:
- **Step 4 ← Step 1 + Step 2**: C-Rust semantic alignment requires knowing which code is "translate smell" (Step 1) and where hardware leaks exist (Step 2) — misaligned code that's also translate smell is P0, not P1
- **Step 10 ← Step 4**: Architecture evolution compliance requires C-Rust alignment context — can't judge "allowed evolution" if you don't know whether the function is semantically aligned
- **Step 5 is an independent P0 gate**: no_std violations must be caught regardless of other steps' results

### Step 0: Scope Declaration

Identify target files and review scope. Determine if focused review (specific dimensions) or complete review.

### Step 1: Rewrite Quality Check (P0/P1)

Scan for "Translate smell" — C-style code that hasn't been re-expressed:

- **Bare integers for semantics** → Use newtype/enum
- **C-style null/sentinel values** (raw `0`, `-1`) → `Option<T>`
- **Unnamed magic numbers** → Named constants
- **C-style flag combinations** → `bitflags`/`enum`/`typestate`
- **C-style error code passing** → `Result` + `?`
- **C macros directly translated** → `trait`/generics
- **C-style data structures** → Rust ownership restructured
- **Memory ownership unclear** → C alloc/free points must have Rust `Owner`

> See [error-patterns.md](references/error-patterns.md) for concrete ❌/✅ examples of Patterns 15-24.
> See [dont-oversimulate.md](references/dont-oversimulate.md) for rules on NOT over-simulating C.

### Step 2: Hardware Abstraction Check (P0)

**All hardware must be abstracted into traits.**

- ❌ Upper-layer code directly operates hardware registers or PTE bit encoding
- ❌ Data structures contain architecture-specific hardware fields
- ❌ `#[cfg(target_arch)]` conditional compilation for hardware behavior
- ✅ Upper layer depends only on trait interface
- ✅ Each architecture implements trait, bound via generics or associated types
- ✅ OS semantic types (e.g., `PageFlags`) separated from hardware encoding

**Trait design quality**:
- Trait must have ≥2 **behaviorally different** implementations
- Trait must be used as a generic constraint (`where T: Trait` or `impl Trait`)
- Single-method traits: evaluate necessity
- Mechanism vs policy separation: mechanism in trait, policy in upper layer

### Step 3: Type Safety Check (P1)

- typestate truly constrains state? No bypass paths?
- typestate and flags strictly consistent? Can "logically illegal but type-legal" states be constructed?
- `unsafe` minimized? Each `unsafe` block has clear safety contract?
- `MaybeUninit`/`UnsafeCell`/raw pointer usage correct and necessary?
- Manual `unsafe Send`/`unsafe Sync` — safety argued?
- Drop semantics clear? No implicit drop risks?

### Step 4: C-Rust Semantic Alignment (P1, Mandatory)

#### 4.1 Alignment Levels

| Level | Alignment Requirement | Mismatch Handling |
|-------|----------------------|-------------------|
| **Architecture** | Mismatch allowed (evolution) | Document evolution rationale |
| **Leaf function** | **Must align** | Code comment explaining reason (P1) |
| **Corrective** (C has bug) | Mismatch allowed | Comment explaining C bug and Rust fix (P1) |

#### 4.2 Leaf Function Semantic Alignment

Each leaf function's Rust return value, error codes, side effects must match C. If not aligned: comment must explain why.

#### 4.3 C Has Implementation but Rust Missing

When C source has a callback/function but Rust trait impl doesn't override it:
- Omission reasonable (default sufficient)?
- Default behavior matches C callback?
  - Inconsistent → P1 (must override)
  - Consistent → No override needed, comment recommended
- Omission unreasonable → Add override

#### 4.4 Rust Comment C Source Reference Verification

Cited C function names, described behavior, source file+line — all must match C source.

> See [alignment-detail.md](references/alignment-detail.md) for §14.5 architecture evolution function disappearance rules, §14.7 review execution strategy, and comment template for corrective misalignment.

### Step 5: no_std Compliance (P0)

- Any `use std::` in non-test code → P0
- `Cargo.toml` sets `#![no_std]`?
- Third-party crates requiring `std`?
- `alloc` usage provides global allocator?
- Mock/test code isolated with `#[cfg(test)]`?

### Step 6: Module Design Check (P1)

- `pub` restrained? Minimum privilege?
- Modules high cohesion? Clear responsibilities?
- Logic that should be split out? Private APIs incorrectly exposed?
- **Module visibility layering**: Internal relaxed, external strict through API
- **Minimum visibility**: Each type's visibility = "minimum range that external needs"

### Step 7: Naming and Traceability (P1)

- Follow Rust conventions (snake_case, CamelCase)?
- Consistent with Minix3 names (prioritize same names for cross-referencing)?
- Bidirectional searchability (grep Minix3 function name finds corresponding Rust code)?
- Parameter names consistent with Minix3 (`clicks` not `count`, `base` not `addr`)?

### Step 8: Comment Quality (P1)

- **Comments must use English** (principle, not suggestion)
- Every `pub` item has doc comment (`///`)?
- Every `pub(crate)` with non-self-explanatory semantics has comment?
- Every module has module-level doc comment (`//!`)?
- Complex algorithms have inline comments explaining "why"?
- `unsafe` code has safety comments?

### Step 9: 64-bit Assumptions (P1)

- Code written for 64-bit? Unnecessary 32-bit remnants (`u32` for address/size)?
- Lossy truncation via `as` (`u64 as u32`) without safety comment?

### Step 10: Architecture Evolution Compliance (P1)

Verify that architecture changes follow the [Allowed Evolution](references/allowed-evolution.md) criteria. All four must hold:
1. External observable behavior unchanged
2. IPC protocol unchanged
3. Lifecycle semantics unchanged
4. Scheduling/permission/address space semantics unchanged

> See [allowed-evolution.md](references/allowed-evolution.md) for the full evolution table (8 dimensions).

### Step 11: Execution Model & Concurrency (§4) (P0/P1)

From [review-code-checklist §4](references/exec-model.md):

- Event loop structure correct? Poll/wakeup matching C's signal-driven main loop?
- Each server single-threaded? No unintended multi-threaded patterns?
- `Rc`/`RefCell` usage consistent with single-thread assumption?
- No `Mutex`/`Arc`/`Atomic` in production code (unnecessary overhead)?
- IPC-based concurrency: message-passing, no shared mutable state?
- Blocking patterns: async/blocking consistent with Minix3 event model?
- `!Send`/`!Sync` correctly applied where needed?

### Step 12: Memory Model & State Expression (§5) (P1)

From [review-code-checklist §5](references/memory-model.md):

- Memory layout matches Minix3 semantics? (e.g., phys/virt separation)
- State machines explicit? All states visible, transitions unambiguous?
- Invalid state unrepresentable? Compile-time prevention of illegal states?
- Ownership clear? Each allocation has defined Owner, each Drop path explicit?

### Step 13: Tests (§8) (P1)

From [review-code-checklist §8](references/test-checklist.md):

- Unit tests cover leaf functions? Error paths tested?
- Integration tests cover IPC scenarios? End-to-end flows?
- Test-only mock/test code in `#[cfg(test)]` modules or separate crates?
- Test coverage of edge cases: empty input, max values, null equivalents?
- Property-based tests where state transitions are complex?
- Fuzzing targets for input-parsing code?

### Step 14: Complexity & Engineering (§11) (P1)

From [review-code-checklist §11](references/complexity.md):

- Unnecessary complexity? Abstracting away simple logic with deep trait hierarchies?
- Platform code (arch/arch64/) properly isolated from OS logic?
- Macro hygiene? No macro-generated code obscuring logic?
- Feature flags? `#[cfg(feature = "...")]` used where feature-gating is needed?
- Dependency cost? Each dependency justified?

### Step 15: Action Item Generation

For each P0/P1 issue, generate executable modification items:

```
### TODO #N: [Brief description]
- **Priority**: P0/P1
- **Dimension**: J1-J14
- **File**: `path/to/file.rs:line`
- **Problem**: [Detailed description]
- **Fix**: [Specific fix plan]
- **Verification**: [How to verify fix is correct]
```

**Key**: P0 issues MUST generate code modification items, not just "suggest fixing".

### Step 16: Self-Check Confirmation (Mandatory)

Before outputting final results, confirm:

- [ ] Step 0 scope declaration outputted
- [ ] Step 1 rewrite quality checked (no translate smell)
- [ ] Step 2 hardware abstraction checked (no hardware leak)
- [ ] Step 3 type safety checked
- [ ] Step 4 C-Rust semantic alignment checked (leaf functions, missing callbacks, comment references)
- [ ] Step 5 no_std compliance checked (no std:: in production)
- [ ] Step 6 module design checked
- [ ] Step 7 naming and traceability checked
- [ ] Step 8 comment quality checked
- [ ] Step 9 64-bit assumptions checked
- [ ] Step 10 architecture evolution compliance checked
- [ ] Step 11 execution model checked
- [ ] Step 12 memory model checked
- [ ] Step 13 tests checked
- [ ] Step 14 complexity checked
- [ ] Step 15 action items generated (P0 must have code modification items)
- [ ] All grep commands' output attached as evidence

> **If any item above is incomplete, go back to that Step and re-execute. No skipping.**

### Step 17: Final Output

Generate structured review output:

1. **Summary**: Files reviewed, issue count (P0/P1/P2)
2. **Dimension Coverage Self-Check**: Table showing all 15 dimensions executed/skipped
3. **Detailed Findings**: Per-dimension results
4. **Issue List**: Priority, location, description, evidence, fix suggestion
5. **Action Items**: Per P0/P1: problem description, fix direction, affected files, verification method
6. **Weakest-Item Self-Check**: Most severe finding with root cause and fix

## Priority Matrix

| Priority | Code Issues |
|----------|------------|
| P0 (Must fix) | UB, memory safety; semantic drift; hardware semantics leak; error codes misaligned; `std::` violation (non-test/mock); event loop structure errors |
| P1 (Should fix) | typestate ineffective; `pub` abuse; unclear module responsibilities; ownership confusion; code-design inconsistency; hardware not abstracted to trait; leaf function semantic misalignment without comment; C callback missing Rust override; comment references wrong C source; no_std crate dependency; missing error path tests |
| P2 (Optional) | Naming conventions; comment coverage/quality; test coverage; unnecessary complexity |

## Quick Judgment Cheat Sheet

1. **Translate smell?** — Bare integers, sentinel values, C-style error codes → P0/P1
2. **Hardware leak?** — CR3/TSS/MSR in upper layer, `#[cfg(target_arch)]` → P0
3. **std violation?** — `use std::` in production code → P0
4. **Semantic drift?** — Leaf function behavior differs from C without comment → P1
5. **Unnecessary trait?** — All implementations identical, never used as bound → P1
6. **Missing override?** — C has specialized callback but Rust uses default → P1
7. **Concurrency model?** — `Arc`/`Mutex`/`Atomic` in single-threaded server → P1
8. **No test for error path?** — Error-returning function without error-path test → P1

## Anti-Laziness Mechanism

- **Never conclude before verification**: Must read source code before judgment
- **Discover one, check all**: If one leaf function misaligned, check all similar functions
- **Priority sampling for leaf functions**: Focus on error-returning, memory alloc/free, concurrency/lock
- **Dimension coverage table**: Must list all 15 dimensions with execution status
- **Skip rationale self-check**: If any dimension skipped, must explain why in output

## Key Rules

- **Comments in English** — Rust source code comments must use English
- **No blind C translation** — don't simulate C limitations; see [dont-oversimulate.md](references/dont-oversimulate.md)
- **Error codes strict** — must correspond to Minix3 errno definitions
- **Hardware in traits** — all hardware operations go through trait abstraction
- **Architecture evolution requires annotation** — when C behavior differs from Rust due to architecture change, must annotate

## Reference Files

Load on demand when a step requires detailed guidance:

| File | When to Load |
|------|-------------|
| [error-patterns.md](references/error-patterns.md) | Step 1: recognizing code anti-patterns (Pattern 15-24) |
| [dont-oversimulate.md](references/dont-oversimulate.md) | Step 1: distinguishing Rewrite from over-simulating C |
| [allowed-evolution.md](references/allowed-evolution.md) | Step 10: checking architecture evolution compliance (8 dimensions) |
| [alignment-detail.md](references/alignment-detail.md) | Step 4: §14.5/14.7 detailed alignment rules |
| [exec-model.md](references/exec-model.md) | Step 11: §4 execution model & concurrency checks |
| [memory-model.md](references/memory-model.md) | Step 12: §5 memory model & state expression checks |
| [test-checklist.md](references/test-checklist.md) | Step 13: §8 test quality checks |
| [complexity.md](references/complexity.md) | Step 14: §11 complexity & engineering checks |