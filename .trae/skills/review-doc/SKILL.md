---
name: review-doc
description: >
  Review Minix-RS documentation against Minix3 C source code. Invoke when user asks to
  review a .md document, check documentation quality, verify C source references, or
  validate design decisions and chapter linkages.
---

# Review Documentation

Review Minix-RS project documentation against Minix3 C source code. Verify that every concept, C code reference, design decision, and implementation description is accurate and traceable.

## When to Use

- User asks to "review xxx.md" or "check documentation quality"
- User asks to verify C source code references in documentation
- User asks to validate design decisions or chapter linkages
- User asks for pedagogical quality review of documentation

Do NOT use when:
- User only wants to review Rust code (use `review-code` skill)
- User wants cross-validation between documentation AND code (use `review-cross` skill)

## Core Principles

### Rewrite, Not Translate

- **Translate** (1:1 syntax conversion): ❌ Forbidden
- **Rewrite** (preserve observable behavior, re-express with Rust type system): ✅ Goal
- **Redesign** (change architecture/mechanisms/protocols): ❌ Currently forbidden

**Behavior semantics first**: External observable behavior must remain unchanged. Internal expression can change. Turn "implicit encoding" into "explicit protocol".

### Ground Truth Priority

```
Minix3 source behavior  >  Documentation description  >  Rust implementation  >  AI analysis
```

Always use Minix3 source code as the ultimate source of truth.

### Documentation Linkage Model

```
Ch1(Concepts) + Ch2(Source Analysis) ──derive──▶ Ch3(Design Decisions) ──implement──▶ Ch4(Implementation) ──generate──▶ Rust code
```

Linkage rules:
1. Ch3 must be based on Ch1&2 — every design decision traceable to concepts or source analysis
2. Ch4 must follow Ch3 — every implementation detail corresponds to a design decision
3. Tests must cover Ch3+Ch4 — every design decision and key implementation has test points
4. Code must match Ch4 — Rust code consistent with implementation description
5. Ch1&2 must completely cover C source — all functions, structs, macros within semantic scope
6. Ch3&4 must completely implement Ch1&2 semantics — C source analyzed but not implemented = semantic loss

### Pedagogical Quality

Documents are teaching materials, not internal development notes. The most common degradation is becoming a "manual" — pasting code, listing functions, stating facts.

| Dimension | Manual (❌) | Teaching Document (✅) |
|-----------|------------|----------------------|
| Section intro | None, starts listing | Has guiding text explaining core idea |
| Code blocks | Bare code, no comments | Key lines annotated, explaining "what" and "why" |
| Design linkage | Not mentioned | References corresponding design decision |
| Semantic mapping | None | Behavior comparison with Minix3 C source |
| Concept introduction | Used without explanation | Definition, motivation, and context on first appearance |

## Document Structure Convention

Every document should follow this flexible structure:

```
# XX-title: Title

> **Category**: Module Library / Module Private / Global Infra
> **Source**: `minix3/minix/servers/module/xxx.c`
> **Description**: One-line function description
> **Status**: Optional, Rust implementation status

---

## 1. Overview
### 1.1 Concept Definition / Purpose
### 1.2 Correspondence with Minix3
### 1.3 Key State / Mechanism Description
### 1.4 Behavioral Rules

## 2. C Source Analysis
### 2.1 Related Definitions (constants, call numbers, configs)
### 2.2 Core Data Structures
### 2.3 Key Function Analysis
### 2.4 Call Relationships / Call Sites
### 2.5 Design Points / Special Handling

## 3. Rust Design Decisions
> High-level design choices, explaining "why designed this way"

## 4. Implementation Details
> Specific Rust implementation, explaining "how implemented"

## 5. Special Topics (if applicable)

## N. Test Points  (second-to-last chapter)
> What needs testing, not listing test code

## N+1. See Also  (last chapter)
- [Related doc](xxx.md)

## Appendix (optional)
```

**Key constraints**:
- **Ch1 (Overview) and Ch2 (C Source Analysis) must NOT contain Rust-specific content**
- **Ch3 (Design Decisions)** focuses on "why", each decision traceable to Ch1&2
- **Ch4 (Implementation)** focuses on "how", each implementation corresponds to Ch3 design
- **Second-to-last chapter (Test Points)** must derive from Ch3 design decisions and Ch4 implementation
- **Last chapter is "See Also"**
- **C source analysis must be complete**: all core data structures, all key functions, call relationships
- **Ch3 should record better alternatives as TODO paragraphs**

## Execution Steps

### Step Dependency Graph

Steps have **logical dependencies** — later steps rely on earlier steps' outputs. Violating this order produces unreliable results.

```
Step 0 (Scope) → Step 1 (Ground Truth) → Step 2 (Diff Extraction)
    → Step 3 (Concept Accuracy) → Step 4 (C Ref Verification)
    → Step 5 (C Source Coverage) → Step 5.5 (Data Struct Coverage)
    → Step 5.6 (Architecture Evolution)

Step 6 (Design Decision Quality) ◄── depends on Step 3 + Step 5
Step 7 (Chapter Linkage)         ◄── depends on Step 6
Step 8 (Doc-Code Consistency)    ◄── depends on Step 7 + Step 1
Step 9 (Diagram Quality)         ── independent, P2
Step 10 (Pedagogical Quality)    ◄── depends on Step 7
Step 11 (Readability Check)      ── independent, P2
Step 12 (Cross-Document Check)   ◄── depends on Step 8
Step 13 (Action Item Generation)
Step 14 (Self-Check Confirmation)
Step 15 (Final Output)
```

**Key dependencies**:
- **Step 6 ← Step 3 + Step 5**: Design decision quality requires confirmed Ch1&2 accuracy and C source coverage — can't judge "design lacks basis" if concepts aren't verified
- **Step 7 ← Step 6**: Chapter linkage requires evaluated design decisions — can't verify "Ch3→Ch1&2" if Ch3 decisions aren't assessed
- **Step 8 ← Step 7 + Step 1**: Document-code consistency requires confirmed linkage and ground truth source locations
- **Step 12 ← Step 8**: Cross-document check requires single-document verification completed first

**Violation consequence**: Executing a step before its dependencies produces unreliable conclusions (e.g., judging design quality with unverified concepts → may miss P0 concept errors that invalidate the design).

### Step 0: Scope Declaration + Time Budget

Declare review mode and scope. Estimate time budget:

| Document Size | Estimated Time |
|--------------|---------------|
| < 200 lines | 10~20 min |
| 200~500 lines | 20~40 min |
| 500~1000 lines | 40~80 min |
| > 1000 lines | 80~120 min |

### Step 1: Ground Truth Lookup

Identify all Minix3 source files referenced in the document. Verify existence with `ls`. Record file paths and line number ranges.

**Output**: Source file table with existence verification. [Template](references/output-templates.md#step-1-source-file-table)

### Step 2: Diff Extraction

List the **top 3** places where the document most deviates from Minix3 original semantics. For each: Minix3 source actual behavior, document description, nature of difference.

**Output**: Top 3 differences table. [Template](references/output-templates.md#step-2-top-3-differences)

### Step 3: Concept Accuracy Verification (P0, Mandatory)

1. Extract all bold terms, type names, key nouns from Ch1; all struct/function/macro names from Ch2
2. Grep each concept against Minix3 source
3. Verify all numeric constants against source
4. Verify algorithm descriptions by reading corresponding C functions

**Output**: Concept verification, constant verification, algorithm verification tables. [Template](references/output-templates.md#step-3-concept-accuracy)

### Step 4: C Code Reference Verification (P0, Mandatory)

1. Extract all C code references (file path + line number) from the document
2. Verify each: file exists, line number accurate, code snippet complete, explanation consistent
3. Check path format: no absolute paths like `file:///home/user/...`
4. Verify explanation text matches actual source behavior (over-interpretation → P1)

**Output**: Reference verification table. [Template](references/output-templates.md#step-4-code-reference-verification)

### Step 5: C Source Coverage Completeness (P0, Mandatory)

1. Determine semantic scope from document title and Ch1
2. List all C source files in the module; extract all C symbols (functions, structs, macros)
3. For each symbol, determine if it's within semantic scope and if document covers it
4. Coverage rate < 80% → P0

**Output**: Coverage table with statistics. [Template](references/output-templates.md#step-5-c-source-coverage)

### Step 5.5: Data Structure Coverage (P1, Mandatory)

Extract all struct names analyzed in document Ch2. Extract all structs from C source using `rg "^struct \w+"` and `rg "^typedef struct"`. For each struct, read source definition, compare whether document analyzed all fields.

**Judgment**: Core struct completely unanalyzed → P0; core struct missing key fields → P1; non-core struct missing → P2.

**Output**: Data structure coverage table. [Template](references/output-templates.md#step-5.5-data-structure-coverage)

### Step 5.6: Architecture Evolution Annotation (P1, Mandatory)

If document describes 32-bit architecture (e.g., 2-level page tables), verify it annotates differences with 64-bit Rust implementation. **Must-annotate differences**: page table levels (2→4), page table entry size (32→64 bit), address split (10+10+12→9+9+9+9+12), page table pointer array (pt_pt[1024]→dynamic).

Unannotated architecture differences → P1.

**Output**: Architecture difference annotation table. [Template](references/output-templates.md#step-5.6-architecture-evolution)

### Step 6: Design Decision Quality (P0/P1, Mandatory for Ch3)

1. Extract all design decisions from Ch3
2. For each decision, verify:
   - Traceability to Ch1&2 (no basis → P1)
   - Scenario coverage completeness (missing error scenarios from Ch2 → P0)
   - no_std compatibility (depends on std → P0)
   - Alternative solutions recorded (better alternative unrecorded → P1)

**Output**: Design decision verification table + error path coverage table. [Template](references/output-templates.md#step-6-design-decision-quality)

### Step 7: Chapter Linkage Verification (P1, Mandatory)

1. Ch3→Ch1&2: Every design decision has basis in Ch1 or Ch2
2. Ch4→Ch3: Every implementation corresponds to a design decision
3. Tests→Ch3+Ch4: Every test point covers a design decision or implementation detail
4. Code→Ch4: Rust code consistent with implementation description

**Output**: Four linkage tables + linkage summary. [Template](references/output-templates.md#step-7-chapter-linkage)

### Step 8: Document-Code Consistency (P1, Mandatory)

From [review-doc-checklist §2.4](references/doc-code-consistency.md):
- Rust function signatures in document match actual code?
- Parameter types, return types match?
- Document-described behavior matches code implementation?
- Example code compiles?

### Step 9: Diagram and Visualization Quality (P2)

From [review-doc-checklist §2.7](references/diagram-checklist.md):
- **Necessity**: Does the diagram convey info text alone cannot?
- **Alignment precision**: ASCII borders, arrows, text strictly aligned?
- **Maintainability**: Diagram too complex for future edits?
- **Information density**: Does it carry enough information?
- **Alternative**: Can a simpler list/table/text replace it?

### Step 10: Pedagogical Quality Check (P1, Mandatory)

- Every section/subsection has guiding text explaining core idea
- Code blocks have comments on key lines (non-self-explanatory lines must)
- Code blocks reference corresponding design decisions
- New concepts have definition and motivation on first appearance
- No code blocks > 15 lines without comments or guiding text

### Step 11: Readability Check (P2)

From [review-doc-checklist §3.1-3.4](references/readability-checklist.md):

**Execution strategy** (from §3.6):
1. **Quick scan first**: Browse document, note first impression (~30s). If feels good, mark "no obvious readability issues" and skip detailed check
2. **Sample if needed**: If issues suspected, sample 3 spots (beginning, middle, end), check §3.1-3.4
3. **Focus on top 3 high-frequency problems**:
   - **Terminology inconsistency** (same thing called 3 different names)
   - **Information jumps** (reader must fill in gaps)
   - **Bare code without explanation** (>30 lines code with no text)
4. **Minimal P2 output**: Only output actual issues found, not per-item pass/fail
5. **Batch processing**: If doc >500 lines, sample 1-2 paragraphs per chapter

**Output format**:
```
### Readability Check (P2)
- Terminology: ✅ Consistent / ⚠️ N inconsistencies (list)
- Logic flow: ✅ Smooth / ⚠️ N jumps (list)
- Redundancy: ✅ None / ⚠️ N repetitions (list)
- Reader experience: ✅ Good / ⚠️ N issues (list)
```

### Step 12: Cross-Document Check

- Check if concepts/types/functions are already defined in sibling documents
- Verify cross-references are correct and consistent
- Check for duplicate definitions or contradictions
- Shared data structures, constants, IPC interfaces should be consistent

#### Semantic Ownership Determination (grep-assisted)

For symbols that could belong to multiple documents, use grep cross-document search to determine ownership:
```bash
rg "SYMBOL_NAME" notes/rewrite/{module}/ --type md -n
```

Output: ownership table (symbol, type, owner doc, current coverage status, recommendation).
[Template](references/output-templates.md#step-11-semantic-ownership)

> See [error-patterns.md](references/error-patterns.md#cross-document-error-patterns) for common cross-document errors.

### Step 13: Action Item Generation

> Purpose: Transform review findings into executable modification items. This solves the "review done but code unchanged" problem.

For each P0/P1 issue, generate:
1. **Problem description**: What's wrong
2. **Fix direction**: How to fix
3. **Affected scope**: Which files (document + code)
4. **Verification method**: How to verify the fix

**Format**:
```
### TODO #N: [Brief description]
- **Priority**: P0/P1
- **Type**: Design flaw / Code-design inconsistency / no_std violation / Semantic drift / ...
- **File**: `path/to/file.rs`
- **Problem**: [Detailed description]
- **Fix**: [Specific fix plan]
- **Verification**: [How to verify fix is correct]
```

**Key**: P0 issues MUST generate code modification items, not just "suggest fixing".
P1 issues involving design improvements MUST add TODO paragraph in document Ch3 describing alternatives.

### Step 14: Self-Check Confirmation (Mandatory)

Before outputting final results, confirm:

- [ ] Step 0 scope declaration and time budget outputted
- [ ] Step 1 source file table outputted
- [ ] Step 2 Top 3 differences outputted
- [ ] Step 3 concept accuracy tables outputted
- [ ] Step 4 code reference verification table outputted
- [ ] Step 5 C source coverage table outputted (with coverage rate)
- [ ] Step 5.5 data structure coverage table outputted
- [ ] Step 5.6 architecture evolution table outputted
- [ ] Step 6 design decision quality table outputted
- [ ] Step 7 chapter linkage tables outputted
- [ ] Step 10 pedagogical quality checked
- [ ] Step 11 readability checked
- [ ] Step 12 cross-document check outputted
- [ ] Step 13 action items generated (P0 must have code modification items)
- [ ] All grep commands' output attached as evidence in corresponding tables

> **If any item above is incomplete, AI must go back to that Step and re-execute. No skipping.**

### Step 15: Final Output

Generate structured review output. [Full template](references/output-templates.md#final-output):
1. **Summary**, 2. **Dimension Coverage Self-Check**, 3. **Detailed Results**,
4. **Issue List** (priority, location, description, evidence, fix), 5. **Cross-Document Findings**,
6. **Weakest-Item Self-Check**, 7. **Time Budget Evaluation**

## Priority Matrix

| Priority | Document Issues |
|----------|----------------|
| P0 (Must fix) | Concept errors, fabricated facts; C code reference errors; C source coverage incomplete (missing key functions/structs/macros) |
| P1 (Should fix) | Architecture differences unnoted; design decisions lack basis; chapter linkage broken; pedagogical quality missing; doc-code inconsistency |
| P2 (Optional) | Clarity; cross-reference completeness; ASCII diagram quality; alternative solutions unrecorded |

## Quick Judgment Cheat Sheet

1. **Fabricated concepts?** — Any concept not found in Minix3 source → P0
2. **Wrong references?** — Any C code reference pointing to wrong file/line → P0
3. **Missing coverage?** — Any key C function/struct not analyzed → P0
4. **Broken linkage?** — Ch3 design without Ch1&2 basis → P1
5. **Manual-style?** — Code blocks without comments or guiding text → P1
6. **Concepts undefined?** — New concept first appearing without definition → P1

## Anti-Laziness Mechanism

- **Never conclude before verification**: Must execute grep/read before giving judgment
- **Pause on contradictions**: If document contradicts source, re-read source, mark P0
- **Mark uncertainty**: If cannot confirm, mark "unverified" with reason
- **No reverse correction**: Never modify understanding of source to match document
- **Dimension coverage table**: Must list all dimensions with execution status
- **Tool invocation self-check**: For every claim requiring grep, execute grep. If no grep result → mark "unverified", not "passed"
- **Skip rationale self-check**: If any dimension is skipped, must explain why in output
- **Time budget check**: If actual time < 50% of estimate, likely cutting corners — re-check weakest items

## Key Rules

- **Document code block comments use Chinese** (Chinese document, aids readability)
- **Rust source code comments use English** (community convention)
- **No references to internal development guides** (like review.md) that readers cannot access
- **Documentation must not contain Rust-specific content in non-Rust chapters** (Ch1&2)
- **Ch3 design decisions must explain "why A not B"**, not just conclusions
- **Ch4 implementations must have guiding text** explaining core design thinking

## Tool Commands Reference

```bash
# ===== Replace {module} with vm/pm/vfs/kernel etc. =====

# Verify constant definitions
rg "^#define CONSTANT" minix3/minix/servers/{module}/ -n
rg "^#define CONSTANT" minix3/minix/include/ -n

# Verify function definitions
rg "^return_type function_name\(" minix3/minix/servers/{module}/ -n

# Verify struct definitions
rg "^struct struct_name " minix3/minix/servers/{module}/ -n
rg "^typedef struct" minix3/minix/servers/{module}/ -A 5

# Verify macro usage
rg "MACRO_NAME" minix3/minix/servers/{module}/ --type c -n

# Verify enum values (headers usually in include/)
rg "ENUM_VALUE" minix3/minix/include/ --type h -n

# Global search (when module uncertain)
rg "SYMBOL_NAME" minix3/minix/ --type c --type h -n

# Cross-document search
rg "SYMBOL_NAME" notes/rewrite/{module}/ --type md -n

# Check sibling document cross-references
rg "\[.*\]\(.*\.md\)" notes/rewrite/{module}/ --type md -n
```

**Module path reference**:

| Module | Source Path |
|--------|-----------|
| VM | `minix3/minix/servers/vm/` |
| PM | `minix3/minix/servers/pm/` |
| VFS | `minix3/minix/servers/vfs/` |
| Kernel | `minix3/minix/kernel/` |
| Drivers | `minix3/minix/drivers/` |
| Common headers | `minix3/minix/include/` |

## Review Mode Auto-Detection

When user says "review xxx.md" without specifying mode:
- Document directory has corresponding `.rs` files → Prompt user whether to do complete review
- User says "check concept accuracy only" → Local review (Ch1&2 only)
- Otherwise → Default document review (Mode B)

## Reference Files

Load these on demand when a step requires detailed guidance:

| File | When to Load |
|------|-------------|
| [error-patterns.md](references/error-patterns.md) | When recognizing anti-patterns during review (Pattern 1-14 + Cross-doc A/B/C) |
| [output-templates.md](references/output-templates.md) | When generating verification tables for Steps 1-15 and final output |
| [diagram-checklist.md](references/diagram-checklist.md) | Step 9: evaluating ASCII diagram quality (§2.7) |
| [doc-code-consistency.md](references/doc-code-consistency.md) | Step 8: verifying doc-code consistency (§2.4) |
| [readability-checklist.md](references/readability-checklist.md) | Step 11: P2 readability checks (§3.1-3.4 + §3.6 execution strategy) |