# Profile I: Stage 2 — Ch3&4 Design Quality Verification

**Objective**: Verify that design decisions (Ch3) are well-reasoned and implementation descriptions (Ch4) are consistent with actual Rust code.
**Rules**: ~30 items
**Prerequisite**: Stage H output (Ch1&2 confirmed accurate)
**Input**: Target document + Stage H output + Rust code
**Output**: P0 scene gaps + P1 broken linkage + P1 design lacks basis

## Preparation

```bash
# Map Ch4 sections to Rust files
ls src/{module}/*.rs

# Extract all pub items from Rust code
rg "^pub " src/{module}/ --type rust -n
```

## I1: Document-Code Consistency (§2.4)

From review-doc-checklist §2.4.

**Process**:
1. Extract all function signatures, struct definitions, trait definitions from Ch4
2. Locate corresponding Rust code
3. Compare line by line

### I1a: Signature Match

| Function | Doc Signature | Actual Signature | Match? | Priority |
|----------|-------------|-----------------|--------|----------|

Check: return type, parameter count/names/types, mutability, visibility.

### I1b: Type Definition Match

| Type | Doc Fields | Actual Fields | Match? | Priority |
|------|-----------|--------------|--------|----------|

Check: field count, field names, field types.

### I1c: Behavior Description Match

| Claim | Doc Location | Actual Code Behavior | Match? | Priority |
|-------|-------------|---------------------|--------|----------|

Method: Read actual Rust function end-to-end, trace all branches, compare with every behavior claim.

### I1d: Error Handling Match

| Error Path | Doc Describes? | Code Handles? | Match? | Priority |
|-----------|---------------|--------------|--------|----------|

### I1e: Example Code Compilability

| Code Block | Doc Location | Compiles? | Issues | Priority |
|-----------|-------------|-----------|--------|----------|

## I2: Design Decision Quality (§2.9)

From review-doc-checklist §2.9.

**Process**:
1. Extract all design decisions from Ch3
2. For each decision, verify:

| Decision | Ch1&2 Basis | Scene Coverage | no_std Compatible | Alternatives | Priority |
|----------|------------|---------------|------------------|-------------|----------|

### I2a: Traceability (P1 if missing)

Every design decision must have a basis in Ch1 (concept) or Ch2 (source analysis).
- Design says "use leather for safety" → Ch1&2 must analyze safety requirements of leather material
- No basis → P1

### I2b: Scene Coverage Completeness (P0 if missing)

Design decisions must consider all scenes documented in Ch2:
- Ch2 describes 3 error scenarios → Ch3 design must cover error handling for all 3
- Missing → P0

### I2c: no_std Compatibility (P0 if violated)

Each design decision must be compatible with no_std:
- Design says "use std::HashMap" → P0
- Design says "use alloc::BTreeMap" → ✅

### I2d: Alternative Solutions (P1 if missing)

For each key design decision:
- Are ≥1 alternative solutions recorded?
- Is the choice rationale clear ("why A not B")?
- Better alternative unrecorded → P1

## I3: Chapter Linkage Verification (§2.10)

From review-doc-checklist §2.10.

### I3a: Ch3 → Ch1&2 (P1)

```markdown
| Design Decision | Ch1 Basis | Ch2 Basis | Status | Priority |
```

### I3b: Ch4 → Ch3 (P1)

```markdown
| Implementation | Design Decision Base | Match? | Priority |
```

### I3c: Tests → Ch3+Ch4 (P1)

```markdown
| Test | Covers Design | Covers Implementation | Match? | Priority |
```

### I3d: Code → Ch4 (P1)

```markdown
| Rust Module | Ch4 Section | Signature Match? | Behavior Match? | Priority |
```

### I3e: Linkage Summary

```markdown
| Linkage | Covered | Total | Rate | Health |
|---------|---------|-------|------|--------|
| Ch3→Ch1&2 | | | % | |
| Ch4→Ch3 | | | % | |
| Tests→Ch3+Ch4 | | | % | |
| Code→Ch4 | | | % | |
```

## I4: Pedagogical Quality (P1)

- Every Ch3 design decision entry: title + rationale + alternatives → readable as standalone
- Every Ch4 section: guiding text before code blocks
- No bare code blocks > 15 lines without comments
- New concepts introduced with definition on first use

## I5: Error Scene Coverage from Ch2 to Ch3/Ch4

**Critical check — most often missed**:

1. Re-read Ch2: extract all error scenes from C source (NULL returns, ENOMEM, EINVAL, etc.)
2. Check Ch3: does design cover error handling for ALL scenes in Ch2?
3. Check Ch4: does implementation description cover error handling?
4. Missing scene in Ch3 → P0; Missing in Ch4 → P1

```markdown
| Error Scene | C Source | Ch2 Covers? | Ch3 Addresses? | Ch4 Addresses? | Priority |
|------------|---------|------------|---------------|---------------|----------|
| Out of memory | buddy_alloc returns NULL | ✅ §2.3 L50 | ❌ | ❌ | P0 |
```

## I6: Output Format

```markdown
# Profile I: Ch3&4 Design Quality — {document}

## Summary
- Doc-code consistency: {N} checks, {M} mismatches
- Design decisions: {N} verified, {M} issues
- Chapter linkage: {rate}%, {M} broken links
- Pedagogical quality: Pass/Warn/Fail

## P0 Issues
| # | Type | Location | Description | Evidence | Fix |

## P1 Issues
| # | Type | Location | Description | Evidence | Fix |

## Verdict
- Ch3&4 design quality: 🟢 Sound / 🟡 Minor issues / 🔴 Major issues
- Ready for Stage J (Profile J): ✅ Yes / ⚠️ Fix P0 first
```