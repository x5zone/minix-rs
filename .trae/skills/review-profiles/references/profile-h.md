# Profile H: Stage 1 — Ch1&2 Accuracy Verification

**Objective**: Verify that Ch1 (Concepts) and Ch2 (Source Analysis) accurately describe Minix3 C source code.
**Rules**: ~40 items
**Input**: Target document + Minix3 C source code
**Output**: P0 concept errors + P0 reference errors + P0 coverage gaps + P1 architecture annotation gaps

## Preparation

```bash
# Identify C source files
rg "minix3/minix/" {document} --type md -n

# Verify existence
ls minix3/minix/servers/{module}/*.{c,h}

# Extract all C symbols
rg "^static |^void |^int |^struct |^#define" minix3/minix/servers/{module}/ --type c -n
rg "^struct \w+" minix3/minix/servers/{module}/ --type c -n
rg "^typedef struct" minix3/minix/servers/{module}/ --type c -n
```

## H1: Concept Accuracy (P0, Mandatory)

From review-doc-checklist §2.1.

**Process**:
1. Extract all bold terms, type names, key nouns from Ch1
2. Extract all struct/function/macro names from Ch2
3. grep each concept against Minix3 source

| Check Item | Method | P0 if |
|-----------|--------|-------|
| Concept exists in C source | `rg "CONCEPT" minix3/.../module/ --type c` | Zero grep hits |
| Concept semantics match C source | Read surrounding context | Semantic contradiction |
| Capitalization correct (e.g., ALLOCMEM vs AllocMem) | Compare with grep output | Wrong case |

**Verification Table**:
```markdown
| Concept | Doc Claim | Source Hit | Source Semantics | Match? |
|---------|----------|-----------|-----------------|--------|
```

## H2: C Code Reference Verification (P0, Mandatory)

From review-doc-checklist §2.2.

**Process**:
1. Extract ALL C code references (file + line) from document
2. Verify file exists: `ls minix3/minix/servers/{module}/{file}`
3. Read the referenced lines: compare snippet, function name, behavior description

| Check Item | Method | P0 if |
|-----------|--------|-------|
| File exists | `ls` | File not found |
| Line range correct | Read file at referenced lines | Line off by >5, function at different line |
| Code snippet matches | Compare blocked code vs actual | Different code, different behavior |
| Explanation matches source | Read function body end-to-end | Explanation contradicts source |
| No absolute paths | grep `file:///` | Absolute path found |

**Verification Table**:
```markdown
| Ref # | File:Line | Exists? | Line OK? | Snippet OK? | Explanation OK? | Priority |
```

## H3: Numeric Constant Verification (P0)

From review-doc-checklist §2.3.

**Process**:
1. Extract all numeric constants from document
2. grep each in C source headers

```bash
rg "#define CONSTANT_NAME" minix3/minix/servers/{module}/ --type c -n
```

| Check Item | Method | P0 if |
|-----------|--------|-------|
| Constant value matches source | grep `#define NAME` | Wrong value |
| Constant semantics match | Read definition + usage context | Wrong meaning |

**Verification Table**:
```markdown
| Constant | Doc Value | Source Value | Source Location | Match? |
```

## H4: Algorithm Description Verification (P0)

From review-doc-checklist §2.3.

**Process**:
1. Extract all algorithm descriptions from Ch2
2. Read the corresponding C function end-to-end
3. Compare behavior claim against actual code path

| Check Item | Method | P0 if |
|-----------|--------|-------|
| Algorithm flow matches C function | Read function body | Wrong order, missing branch |
| All branches described | Trace all if/switch/case | Branch unmentioned |
| Error handling described | Trace error return paths | Error path missing |

**Verification Table**:
```markdown
| Algorithm | Doc Description | Source Function | Read Lines | Match? | Missing |
```

## H5: Data Structure Coverage (P1, Mandatory)

From review-doc-checklist §2.5.

**Process**:
1. Extract all struct names from document Ch2
2. List all structs in C source: `rg "^struct \w+"` and `rg "^typedef struct"`
3. For each struct in scope, read definition, compare with document analysis

| Check Item | Method | P0/P1 if |
|-----------|--------|----------|
| Core struct analyzed | Check doc has § for struct | Completely missing → P0 |
| All fields analyzed | Compare struct definition vs doc | Key fields missing → P1 |
| Field semantics correct | Read field usage in C code | Wrong interpretation → P1 |

**Coverage Table**:
```markdown
| Struct | C Source | Fields | Doc Section | Fields Covered | Missing Fields | Priority |
```

## H6: Architecture Evolution Annotation (P1)

From review-doc-checklist §2.5.5 + §2.6.

If document describes 32-bit architecture (e.g., 2-level page tables):

| Aspect | Minix3 (32-bit) | minix-rs (64-bit) | Doc Annotates? | P1 if |
|--------|----------------|-------------------|---------------|-------|
| Page table levels | 2 (PD+PT) | 4 (PML4+PDPT+PD+PT) | ✅/❌ | ❌ |
| Page table entry size | 32-bit | 64-bit | ✅/❌ | ❌ |
| Address split | 10+10+12 | 9+9+9+9+12 | ✅/❌ | ❌ |
| Page table pointer array | pt_pt[1024] | Dynamic allocation | ✅/❌ | ❌ |
| Address width | 32-bit linear | 48-bit virtual | ✅/❌ | ❌ |

## H7: Diagram Quality (P2)

From review-doc-checklist §2.7.

| Check Item | Criteria |
|-----------|----------|
| Necessity | Diagram conveys info text alone cannot? |
| Alignment | ASCII borders, arrows, text strictly aligned? |
| Maintainability | Can diagram be edited by adding one row without cascading reformat? |
| Information density | Each element carries unique information? |
| Simpler alternative | Can a table/list replace it? |

## H8: C Source Coverage Completeness (P0)

From review-doc-checklist §2.8.

**Process**:
1. Determine semantic scope from document title + Ch1
2. List all C source files in the module
3. Extract all C symbols: `functions`, `structs`, `macros`, `enums`
4. For each symbol within semantic scope, check if document covers it
5. Coverage rate < 80% → P0

**Coverage Table**:
```markdown
| Symbol | Type | C File | In Scope? | Documented? | Doc Location | Priority |
```

**Statistics**:
```markdown
| Metric | Count | Rate |
|--------|-------|------|
| Total symbols in scope | {N} | — |
| Covered | {M} | {M/N*100}% |
| Uncovered | {K} | {K/N*100}% |
```

## H9: Output Format

```markdown
# Profile H: Ch1&2 Accuracy — {document}

## Summary
- Concepts verified: {N}, errors: {M}
- C references verified: {N}, errors: {M}
- Constants verified: {N}, errors: {M}
- Algorithms verified: {N}, errors: {M}
- Structs analyzed: {N}, gaps: {M}
- Architecture annotation gaps: {N}
- Coverage rate: {X}%

## P0 Issues
| # | Type | Location | Description | Evidence | Fix |
|---|------|---------|-------------|---------|-----|

## P1 Issues
| # | Type | Location | Description | Evidence | Fix |

## Verdict
- Ch1&2 accuracy: 🟢 Accurate / 🟡 Minor issues / 🔴 Major issues
- Ready for Stage I (Profile I): ✅ Yes / ⚠️ Fix P0 first / ❌ Major rework needed
```