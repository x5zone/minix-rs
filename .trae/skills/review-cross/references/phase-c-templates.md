# Phase C: Cross-Validation Templates

## C1: Document-Code Mapping

```markdown
| Doc Section | Type/Function | Rust File | Line Range | Found? |
|------------|--------------|-----------|-----------|--------|
| §4.2 BuddyAlloc | struct BuddyAlloc | src/vm/buddy.rs | L20-80 | ✅ |
| §4.2 alloc() | fn alloc(order: Order) -> Result<PhysAddr> | src/vm/buddy.rs | L45-60 | ✅ |
| §4.3 PageTable | trait Paging | src/vm/paging.rs | L10-30 | ✅ |
| §4.3 map() | fn map() | src/vm/paging.rs | L25-28 | ✅ (trait method) |
```

## C2: Doc→Code Consistency Verification

```markdown
| Doc Claim | Doc Location | Actual Code | Code Location | Match? | Priority |
|----------|-------------|------------|--------------|--------|----------|
| "alloc() returns Result<PhysAddr>" | §4.2 L30 | `fn alloc() -> Result<PhysAddr, AllocError>` | buddy.rs:L45 | ❌ (missing error type) | P1 |
| "PageTable stores cr3 value" | §4.3 L15 | `struct PageTable { inner: ArchPageTable }` | paging.rs:L20 | ❌ (abstracted, no cr3) | P1 |
| "map() takes VirtAddr, PhysAddr" | §4.3 L40 | `fn map(vaddr: VirtAddr, paddr: PhysAddr, flags: PageFlags)` | paging.rs:L25 | ❌ (missing flags) | P1 |
```

### Verification Checklist

For each Ch4 section describing code:
- [ ] Function signature: parameter count, types, return type match?
- [ ] Struct definition: field count, field names, field types match?
- [ ] Trait definition: method signatures match?
- [ ] Enum definition: variant count, variant names match?
- [ ] Behavior description: all claims verified against code logic?
- [ ] Error handling: described error cases handled in code?

## C3: Design Decision → Code Implementation

```markdown
| Design Decision | Ch3 Location | Expected in Code | Actual Code | Match? | Priority |
|----------------|------------|-----------------|------------|--------|----------|
| DD-1: Buddy allocator | §3.1 | struct BuddyAlloc + trait FrameAlloc | ✅ buddy.rs:L20 | ✅ | — |
| DD-2: Typestate for Proc | §3.2 | InitProc, RunningProc, etc. | enum ProcState in proc.rs | ❌ (enum, not typestate) | P1 |
| DD-3: Hardware abstraction | §3.3 | trait Paging with x86-64 + aarch64 impls | trait Paging in paging.rs, only x86-64 impl | Partial (missing aarch64) | P1 |
| DD-4: RCU-style slot mgmt | §3.5 | Atomic/RCU types | NOT FOUND | ❌ | P0 |
```

## C4: Code Beyond Documentation

```markdown
| Rust Item | Visibility | Rust Location | Documented in Ch4? | Priority |
|----------|-----------|-------------|-------------------|----------|
| fn prealloc_pages() | pub(crate) | buddy.rs:L200 | ❌ | P1 |
| struct AllocStats | pub(crate) | buddy.rs:L250 | ❌ | P1 |
| impl Debug for BuddyAlloc | pub | buddy.rs:L300 | ❌ (trivial, no priority) | — |
| fn __internal_debug() | pub(crate) | paging.rs:L500 | ❌ (debug-only, no priority) | — |
```

### Judgment for Code Beyond Documentation

| Category | Priority |
|----------|----------|
| Key functionality (alloc, map, fork, exec) | P1 |
| New public type used by other modules | P1 |
| New trait implementation | P1 |
| Utility/helper not externally used | P2 |
| Debug/Display impl | — (no priority) |
| Test-only helper | — (no priority) |

## C5: Full Chain C↔Doc↔Code

```markdown
| Function | C Behavior | Doc Description | Code Behavior | C=Doc? | Doc=Code? | C=Code? | Priority |
|----------|-----------|----------------|--------------|--------|----------|---------|----------|
| buddy_alloc() | Returns phys addr or NULL | Returns PhysAddr | Returns Result<PhysAddr, AllocError> | ✅ | Partial (error type diff) | ✅ | P2 |
| pt_bind() | Maps single-level PTE | Maps multi-level PTE | Maps 4-level PTE (x86-64) | ❌ | ✅ | ❌ | P1 |
| do_fork() | Copies proc struct + PT | Copies proc struct + PT | Copies proc struct + PT | ✅ | ✅ | ✅ | — |
```

### NULL Callback Semantic Check

```markdown
| Callback | C NULL Behavior | Doc Description | Rust Default | Match? | Priority |
|----------|----------------|----------------|-------------|--------|----------|
| page_fault_handler | NULL → ENOSYS | Not described | Ok(()) | ❌ (should be Err(ENOSYS)) | P1 |
| invalidate_cache | NULL → Ok (no-op) | Default skips | Ok(()) | ✅ | — |
| pre_fork_hook | NULL → return 0 (ok) | Default returns 0 | Ok(()) | ✅ | — |
```

## C6: Cross-Document Consistency

```markdown
| Symbol/Semantic | Doc A | Doc B | Consistent? | Priority |
|----------------|-------|-------|------------|----------|
| vm_slot | vm/process.md §2.2: "per-process VM state" | pm/process.md §3.1: "per-process memory mapping" | ⚠️ (different focus, same concept) | P2 |
| NR_PT | vm/pagetable.md: 1024 | Not in pm/pagetable.md | ❌ (missing in pm doc) | P1 |
| Page size | vm/memory.md: 4096 | pm/memory.md: 4096 | ✅ | — |
```

## Final Output Template

```markdown
# Complete Review: {document name} + {code paths}

## Summary
- **Document**: {file name} ({N} lines, {M} sections)
- **Code**: {N} files, {M} lines of Rust
- **Ground Truth**: {minix3 source files}
- **Overall Rating**: 🟢 Pass / 🟡 Needs Work / 🔴 Major Issues
- **Key Findings**: {1-2 sentence summary}

## Phase A Results: Documentation Quality
{Concept verification, reference check, coverage stats, linkage summary}

## Phase B Results: Code Quality
{Rewrite quality, hardware abstraction, alignment, no_std, type safety, etc.}

## Phase C Results: Cross-Validation
{Mapping table, consistency verification, full chain, cross-document}

## Consolidated Issue List
| # | Phase | Priority | Location | Description | Evidence | Fix |
|---|-------|---------|----------|-------------|---------|-----|

## Action Items
| # | Priority | Problem | Fix Direction | Affected Files | Verification |
|---|---------|---------|--------------|---------------|-------------|

## Dimension Coverage Self-Check
| Phase | Dimension | Executed? | Result | Time |
|-------|----------|-----------|--------|------|
| A | Concept accuracy | ✅/⏭️ | Pass/Warn/Fail | {min} |
| A | C code references | ✅/⏭️ | Pass/Warn/Fail | {min} |
| A | C source coverage | ✅/⏭️ | Pass/Warn/Fail | {min} |
| A | Design decisions | ✅/⏭️ | Pass/Warn/Fail | {min} |
| A | Chapter linkage | ✅/⏭️ | Pass/Warn/Fail | {min} |
| B | Rewrite quality | ✅/⏭️ | Pass/Warn/Fail | {min} |
| B | Hardware abstraction | ✅/⏭️ | Pass/Warn/Fail | {min} |
| B | C-Rust alignment | ✅/⏭️ | Pass/Warn/Fail | {min} |
| B | no_std compliance | ✅/⏭️ | Pass/Warn/Fail | {min} |
| B | Execution model | ✅/⏭️ | Pass/Warn/Fail | {min} |
| B | Memory model | ✅/⏭️ | Pass/Warn/Fail | {min} |
| B | Tests | ✅/⏭️ | Pass/Warn/Fail | {min} |
| B | Complexity | ✅/⏭️ | Pass/Warn/Fail | {min} |
| C | Doc-code mapping | ✅/⏭️ | Pass/Warn/Fail | {min} |
| C | Doc→Code consistency | ✅/⏭️ | Pass/Warn/Fail | {min} |
| C | Design→Code | ✅/⏭️ | Pass/Warn/Fail | {min} |
| C | Code beyond doc | ✅/⏭️ | Pass/Warn/Fail | {min} |
| C | Full chain C↔Doc↔Code | ✅/⏭️ | Pass/Warn/Fail | {min} |
| C | Cross-document | ✅/⏭️ | Pass/Warn/Fail | {min} |

## Time Budget Evaluation
| Planned | Actual | Difference |
|---------|--------|-----------|
```