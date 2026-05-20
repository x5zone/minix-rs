---
name: review-cross
description: >
  Cross-validate Minix-RS documentation against Rust code and vice versa. Invoke when
  user asks to review a .md document AND its associated .rs files, verify doc-code
  consistency, or check implementation matches design and chapter linkages.
---

# Complete Review: Documentation + Code

This is the **complete review** skill — it validates documentation against Minix3 C source, checks Rust code quality, and cross-validates documentation↔code consistency. Use this when reviewing a document together with its associated Rust code.

## When to Use

- User asks to **"review xxx.md 和它关联的 rust 代码"** ← most common scenario
- User asks to "review xxx.md and its associated .rs files"
- User asks for complete review covering both documentation and code
- User asks to verify doc-code consistency or check implementation matches design
- User asks to validate chapter linkages against actual code

Do NOT use when:
- User only wants to review documentation without checking code (use `review-doc` skill)
- User only wants to review code without checking documentation (use `review-code` skill)

## Core Principles

### Bidirectional Consistency

1. **Doc → Code**: Every description in documentation must have corresponding code implementation
2. **Code → Doc**: Every key implementation in code must be documented somewhere

Both directions must be verified. Missing in either direction is a problem.

### Ground Truth Priority

```
Minix3 source behavior  >  Documentation description  >  Rust implementation  >  AI analysis
```

When documentation and code conflict:
1. Verify against Minix3 C source — which one matches C behavior?
2. The one matching C behavior is correct, the other needs fixing
3. If neither matches C behavior → both need fixing

### Linkage Chain

```
Ch1+Ch2 ──derive──▶ Ch3(Design) ──implement──▶ Ch4(Description) ──match──▶ Rust Code
```

Cross-validation focuses on the last two links: Ch4↔Code and Ch3↔Code.

## Execution: Three-Phase Review

### Phase Dependency Graph

Phases have **strict sequential dependencies** — each phase relies on the previous phase's outputs.

```
Phase A (Doc vs C Source) → Phase B (Code Quality) → Phase C (Cross-Validation)
                              ◄── B depends on A        ◄── C depends on A+B
```

**Key dependencies**:
- **Phase B ← Phase A**: Code review needs verified concepts from doc review — can't judge "C-Rust semantic alignment" if you haven't verified what C actually does (Phase A Step 3)
- **Phase C ← Phase A + Phase B**: Cross-validation needs both "what doc claims" (Phase A) and "what code does" (Phase B) — without both, you're comparing an unverified claim against unverified code
- **Within Phase C**: Step C5 (Full Chain C↔Doc↔Code) depends on C1-C4 — can't verify the full chain until mapping and individual consistencies are checked

**Violation consequence**: Executing Phase B before Phase A may produce false P1s (code looks wrong but doc is actually wrong). Executing Phase C before A+B produces unreliable cross-validation (both sides unverified).

### Phase A: Documentation vs C Source

> Execute `review-doc` Steps 3-7 against the document.

Key checks: concept accuracy (P0), C code references (P0), C source coverage (P0), design decision quality (P0/P1), chapter linkage (P1).

> If Phase A alone is sufficient for analysis quality, keep concise. If the document is large or complex, load [review-doc's reference files](../review-doc/references/) for detailed templates.

### Phase B: Code Quality & C-Rust Alignment

> Execute `review-code` Steps 1-14 against the Rust code.

Key checks: rewrite quality (P0/P1), hardware abstraction (P0), C-Rust semantic alignment (P1), no_std (P0), type safety (P1), module design (P1), execution model (P0/P1), memory model (P1), tests (P1), complexity (P1).

> If Phase B alone is sufficient for analysis quality, keep concise. If the codebase is large or complex, load [review-code's reference files](../review-code/references/) for detailed checklists.

### Phase C: Cross-Validation

> Unique to this skill — verify documentation↔code bidirectional consistency.

#### Step C1: Document-Code Mapping

Extract all type names, function signatures, module references from Ch4. Locate corresponding Rust source files. Build mapping table: Ch4 section ↔ Rust file + line range.

**Output**: [Mapping table](references/phase-c-templates.md#c1-document-code-mapping)

#### Step C2: Doc→Code Consistency (P1)

Verify Ch4 descriptions match actual code:
- Function signatures: parameter types, return types, visibility?
- Type definitions: field names, field types?
- Behavior semantics: code behavior matches Ch4 description?
- Default implementations: Ch4 describes default, code matches?
- Error handling: Ch4 describes error cases, code handles them?

#### Step C3: Design→Code Consistency (P1)

Verify Ch3 design decisions are implemented in code:
- Ch3 says typestate → Does code use typestate?
- Ch3 says enum → Does code use enum or bare integers?
- Ch3 says trait with N implementations → Does code have N implementations?
- Not implemented → **P0**; Implemented differently → **P1**

#### Step C4: Code Beyond Documentation (P1)

Extract all `pub`/`pub(crate)` items from Rust code, cross-reference with Ch4:
- Functions/types in Rust not mentioned in Ch4 → P1
- Trait implementations not described in Ch4 → P1

#### Step C5: Full Chain C↔Doc↔Code (P1)

For each key function/callback: **C behavior = Doc description = Code behavior?** Focus on leaf functions, NULL callbacks, error codes. This is the most critical step — it validates the entire linkage chain.

#### Step C6: Cross-Document Consistency (P1)

Shared data structures, constants, IPC interfaces described consistently across documents? No duplicate definitions or contradictions?

> For cross-document error patterns (Patterns A/B/C), see [review-doc error patterns](../review-doc/references/error-patterns.md#cross-document-error-patterns).
> For detailed templates, see [phase-c-templates.md](references/phase-c-templates.md).

## Priority Matrix

| Priority | Cross-Validation Issues |
|----------|------------------------|
| P0 (Must fix) | Design decision not implemented in code; C behavior contradicted by both doc and code; `std::` violation; event loop structure error |
| P1 (Should fix) | Ch4 description doesn't match code; code implements undocumented features; leaf function semantic misalignment without comment; NULL callback semantics differ between C and Rust; cross-document contradiction; design-code deviation |
| P2 (Optional) | Minor description inaccuracies; test coverage gaps; undocumented but trivial code |

## Quick Judgment Cheat Sheet

1. **Design unimplemented?** — Ch3 says typestate but code uses bare integers → P0
2. **Description wrong?** — Ch4 says function returns X but code returns Y → P1
3. **Code beyond design?** — Code has features not in Ch3/Ch4 → P1
4. **NULL callback mismatch?** — C NULL returns EINVAL but Rust default returns Ok(()) → P1
5. **Error code wrong?** — C returns EINVAL but Rust returns different error → P1
6. **Cross-doc contradiction?** — Same concept described differently in two docs → P1

## Conflict Resolution

When documentation and code conflict:
1. Verify against Minix3 C source — which one matches C behavior?
2. If doc matches C, code doesn't → Fix code
3. If code matches C, doc doesn't → Fix doc
4. If neither matches C → Fix both

### Rule Conflict Resolution

| Conflict | Resolution |
|----------|-----------|
| **Accuracy vs Readability** | Accuracy wins |
| **Minix3 Naming vs Rust Convention** | Minix3 naming wins for cross-referencing; add Rust alias if needed |
| **C Alignment vs Type Safety** | Type safety wins for leaf functions; comment the C behavior difference |
| **Completeness vs Brevity** | Completeness wins — verbose better than missing critical info |
| **Pedagogical Quality vs Conciseness** | Pedagogical quality wins — teaching docs must explain |

## Anti-Laziness Mechanism

- **All three phases required**: Phase A, B, C must all be executed — no skipping phases
- **Phase B must read actual code**: Not infer from documentation descriptions
- **Phase C must cross-check both directions**: Doc→Code AND Code→Doc
- **Dimension coverage table**: Must list all phases and sub-dimensions with execution status
- **Time budget**: Complete cross-review is 2-4x single-skill review time

## Self-Check Confirmation (Mandatory)

Before outputting final results, confirm:

- [ ] Phase A: All doc review steps executed (concept accuracy, C references, coverage, design decisions, linkage)
- [ ] Phase B: All code review steps executed (rewrite quality, hardware abstraction, alignment, no_std, type safety)
- [ ] Phase C: All cross-validation steps executed (mapping, doc→code, design→code, code beyond doc, full chain)
- [ ] All grep commands' output attached as evidence
- [ ] Action items generated for all P0/P1 issues
- [ ] Time budget evaluation completed

> **If any item above is incomplete, go back to that phase and re-execute. No skipping.**

## Relationship to Other Skills

| Request | Skill | Scope |
|---------|-------|-------|
| "review xxx.md" | `review-doc` | Doc vs C source |
| "review xxx.rs" | `review-code` | Code quality + C-Rust alignment |
| **"review xxx.md + .rs"** | **`review-cross`** | **Doc + Code + Cross-validation** |

This skill integrates Phase A (= `review-doc` core), Phase B (= `review-code` core), and Phase C (unique cross-validation). For single-dimension reviews, use the specialized skill.

## Reference Files

| File | When to Load |
|------|-------------|
| [phase-c-templates.md](references/phase-c-templates.md) | Phase C: detailed mapping tables and verification templates |
| [review-doc references](../review-doc/references/) | Phase A: load doc error patterns, output templates, diagram checklist as needed |
| [review-code references](../review-code/references/) | Phase B: load code error patterns, exec model, memory model, test/alignment checklists as needed |