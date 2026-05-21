# Profile K: Stage 4 — Cross-Document + Readability

**Objective**: Verify cross-document consistency and overall readability quality.
**Rules**: ~25 items
**Prerequisite**: Stages H, I, J outputs (Ch1&2 accurate, Ch3&4 designed, code verified)
**Input**: All sibling .md files in the module directory + Stages H-J outputs
**Output**: P2 readability issues + P1 cross-document contradictions/duplicates

## Preparation

```bash
# List all sibling documents in the module
ls notes/rewrite/{module}/*.md

# Search for shared symbols across documents
rg "SYMBOL_NAME" notes/rewrite/{module}/ --type md -n
```

## K1: Cross-Document Consistency (P1)

> For detailed patterns, see [review-doc error patterns](../../review-doc/references/error-patterns.md#cross-document-error-patterns).

### K1a: Duplicate Definition Check (Pattern A)

| Symbol | Doc A Definition | Doc B Definition | Consistent? | Canonical? | Priority |
|--------|-----------------|------------------|------------|-----------|----------|

Process:
1. For each shared concept/type/function: grep across all sibling .md files
2. Same definition in ≥2 documents → designate one canonical, others reference
3. Check that cross-references use consistent language

### K1b: Contradictory Definition Check (Pattern B)

| Symbol | Doc A Claim | Doc B Claim | Verify Against C Source | Verdict | Priority |
|--------|-----------|------------|------------------------|---------|----------|

Process:
1. For each shared constant/value/semantic: compare definitions across documents
2. If contradiction → verify against C source (ground truth)
3. Mark incorrect document + fix suggestion

### K1c: Missing Cross-Reference Check (Pattern C)

| Shared Responsibility | Doc A Covers | Doc B References A? | Priority |
|----------------------|-------------|-------------------|----------|

Process:
1. Identify shared responsibilities (e.g., PM+VM fork, PM+FS exec)
2. Check if each document references its sibling document
3. Missing cross-reference → P1

### K1d: Shared Data Structure Consistency

```bash
# Find all struct definitions across documents
rg "struct \w+" notes/rewrite/{module}/ --type md -n
```

| Struct | Defined In | Used In | All Docs Consistently Describe? | Priority |
|--------|-----------|---------|-------------------------------|----------|

### K1e: IPC Interface Consistency

| IPC Message | Doc A Describes | Doc B Describes | Consistent Format? | Consistent Semantics? | Priority |
|------------|----------------|----------------|-------------------|--------------------|----------|

## K2: Readability & Organization (P2)

> **P2 执行策略** (review-doc-checklist §3.6): P2 问题按以下优先级处理：
> 1. 影响理解的表述歧义 → 必须修复
> 2. 冗余内容精简 → 建议修复
> 3. 纯风格改善 → 可选修复
> 不要求所有 P2 都修复，但必须全部列出。

### K2a: Section Flow

| Check Item | Assessment |
|-----------|-----------|
| Natural reading order? (build up from simple→complex) | ✅/❌ |
| Each section has a clear single purpose? | ✅/❌ |
| Section ordering logical for learning progression? | ✅/❌ |
| Transitions between sections smooth (not abrupt topic changes)? | ✅/❌ |

### K2b: Redundancy Control

| Check Item | Assessment |
|-----------|-----------|
| Same concept explained multiple times within one document? | ✅/❌ |
| Long background sections that should be a cross-reference? | ✅/❌ |
| Duplicate code blocks across sections? | ✅/❌ |
| Repetitive explanation patterns? | ✅/❌ |

### K2c: Language & Fluency

| Check Item | Assessment |
|-----------|-----------|
| No awkward Chinese phrasing or transliterated English? | ✅/❌ |
| Technical terms consistently translated? | ✅/❌ |
| Sentence length manageable (no >80 char sentences)? | ✅/❌ |
| Code block comments in Chinese (document convention)? | ✅/❌ |

### K2d: Reader Experience

| Check Item | Assessment |
|-----------|-----------|
| First-time reader can understand without external knowledge? | ✅/❌ |
| All acronyms defined on first use? | ✅/❌ |
| Concepts introduced before they are used? | ✅/❌ |
| Key takeaways clear at end of each major section? | ✅/❌ |

### K2e: References & Navigation

| Check Item | Assessment |
|-----------|-----------|
| All internal links work (section references, document references)? | ✅/❌ |
| External links to C source use relative paths (no `file:///`)? | ✅/❌ |
| Sibling document references up-to-date (no stale filenames)? | ✅/❌ |

## K3: Documentation Completeness from Code (P1)

Given Stage J confirmed code is correct, check document covers all key code:

| Rust Item | Visibility | Documented in Doc? | Doc Section | Priority |
|----------|-----------|-------------------|------------|----------|
| fn do_fork | pub | ✅ | §4.2.1 | — |
| struct VmSlot | pub(crate) | ❌ | — | P1 |

Process:
1. Extract all `pub`/`pub(crate)` items from Rust code (use Stage J output)
2. Cross-reference with document
3. Key functionality not documented → P1

## K4: Stale Content Check

| Check Item | Assessment |
|-----------|-----------|
| Any content referencing removed/reorganized C source files? | ✅/❌ |
| Any "TODO" or "FIXME" without tracking issue? | ✅/❌ |
| Any outdated figures/diagrams not matching current code? | ✅/❌ |
| Any version-specific notes that should be removed? | ✅/❌ |

### K4a: Stale Design Content Check (P1)

> **历史教训**: 15-pagefault.md 中的 AddressResolution/PageFaultMessage/RegionAvlTree 在 Ch4 中有完整定义但代码中不存在，属于陈旧设计内容。

| Ch3/Ch4 设计项 | Rust 代码中存在? | 标注为"未来设计"? | 优先级 |
|---------------|----------------|-----------------|--------|

**Process**:
1. 提取 Ch3&4 中所有 Rust struct/enum/trait/函数定义
2. 在实际 Rust 代码中搜索对应项
3. 代码中不存在的 → 必须标注为"未来设计"或"设计草图"
4. 未标注的陈旧设计内容 → P1（误导读者以为已实现）

## K5: Output Format

```markdown
# Profile K: Cross-Document + Readability — {module}

## Summary
- Sibling documents: {N}
- Cross-document issues: P1={M}, P2={K}
- Readability: 🟢 Good / 🟡 Needs polish / 🔴 Hard to read

## Cross-Document Issues
| # | Type | Priority | Location | Description | Fix |
|---|------|---------|----------|-------------|-----|

### Duplicate Definitions (Pattern A)
| Symbol | Docs | Issue | Recommendation |

### Contradictory Definitions (Pattern B)
| Symbol | Docs | C Source Truth | Fix |

### Missing Cross-References (Pattern C)
| Responsibility | Docs | Gap | Fix |

## Readability Issues
| # | Dimension | Location | Description | Suggestion |
|---|----------|----------|-------------|------------|

## Documentation Completeness from Code
| Code Item | Doc Coverage | Priority |

## Verdict
- Cross-document consistency: 🟢 Consistent / 🟡 Minor gaps / 🔴 Contradictions
- Readability: 🟢 Good / 🟡 Needs polish / 🔴 Needs rewrite
- Complete review finished: ✅ All 4 stages complete
```

## Complete Review Summary (After All 4 Stages)

```markdown
# Complete Review Summary: {module}

| Stage | Profile | Result | P0 | P1 | P2 |
|-------|---------|--------|----|----|-----|
| 1 | H (Ch1&2 accuracy) | 🟢/🟡/🔴 | {N} | {N} | {N} |
| 2 | I (Ch3&4 design) | 🟢/🟡/🔴 | {N} | {N} | {N} |
| 3 | J (Code quality) | 🟢/🟡/🔴 | {N} | {N} | {N} |
| 4 | K (Cross-doc + readability) | 🟢/🟡/🔴 | {N} | {N} | {N} |
| **Total** | | | **{N}** | **{N}** | **{N}** |

## Top Issues to Fix
| # | Stage | Priority | Description |
|---|-------|---------|-------------|

## Overall Verdict
{1-2 paragraph summary of module quality and readiness}
```