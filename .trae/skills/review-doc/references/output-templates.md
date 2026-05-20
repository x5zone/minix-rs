# Output Templates

## Step 1: Source File Table

```markdown
| Source File | Document Ref | Line Range | Exists? | Covers? |
|------------|-------------|-----------|---------|---------|
| vm/main.c | §2.1 | L50-200 | ✅ | ✅ |
| vm/region.c | §2.3 | L30-80 | ✅ | Partial |
| vm/pagetable.c | Not ref'd | — | ✅ | ❌ |
```

## Step 2: Top 3 Differences

```markdown
| # | C Source Actual Behavior | Document Description | Difference |
|---|-------------------------|---------------------|-------------|
| 1 | buddy_alloc returns phys addr | doc says returns virt addr | Wrong return type |
| 2 | NR_REGIONS = 5 | doc says up to 256 | Wrong constant |
| 3 | pt_bind does single-level mapping | doc says recursive mapping | Semantic error |
```

## Step 3: Concept Accuracy

### Concept Verification Table

```markdown
| Concept | Doc Claim | Source Grep | Source Behavior | Match? | Priority |
|---------|----------|-------------|----------------|--------|----------|
| vm_slot | "represents per-process VM state" | `rg "struct vm_slot"` | linked list node with phys addr + length | ✅ | — |
| pt_ptalloc | "allocates page directory" | `rg "pt_ptalloc"` | initializes multi-level page table entries | Semantic mismatch | P0 |
```

### Constant Verification Table

```markdown
| Constant | Doc Value | Source Value | Source Location | Match? | Priority |
|----------|----------|--------------|----------------|--------|----------|
| NR_REGIONS | 256 | 5 | vm/const.h:L15 | ❌ | P0 |
| ARCH_VM_DIR_ENTRIES | 1024 | 1024 | arch/include/archtypes.h:L42 | ✅ | — |
```

### Algorithm Verification Table

```markdown
| Algorithm | Doc Description | Source Function | Read Lines | Match? | Difference |
|-----------|----------------|----------------|------------|--------|------------|
| Page allocation | buddy alloc + free list merge | buddy_alloc() | vm/buddy.c:L100-180 | ✅ | — |
| First-fit | scan hole list, return first match | allocmem() | pm/alloc.c:L50-120 | Partial | Doc missed adjacent-hole merge case |
```

## Step 4: Code Reference Verification

```markdown
| Ref # | File:Line | File Exists? | Line Match? | Snippet Match? | Explanation Consistent? | Priority |
|-------|----------|-------------|-------------|---------------|------------------------|----------|
| 1 | vm/main.c:L50 | ✅ | ✅ | ✅ | ✅ | — |
| 2 | vm/region.c:L30 | ✅ | ❌ (L42) | ✅ | ✅ | P2 |
| 3 | vm/pgtable.c:L80 | ❌ (no such file) | — | — | — | P0 |
```

Path format check:
- ❌ `file:///home/user/github/minix-rs/minix3/minix/servers/vm/main.c`
- ✅ `minix3/minix/servers/vm/main.c`

## Step 5: C Source Coverage

```markdown
| Symbol | Type | C Source | In Scope? | Covered in Doc? | Location in Doc | Priority |
|--------|------|---------|-----------|----------------|----------------|----------|
| do_fork | function | vm/fork.c:L120 | ✅ | ✅ | §2.4.1 | — |
| do_brk | function | vm/brk.c:L60 | ✅ | ❌ | — | P0 |
| NR_REGIONS | macro | vm/const.h:L15 | ✅ | ❌ | — | P0 |
| struct vm_slot | struct | vm/region.h:L25 | ✅ | Partial | §2.3 | P1 |
| vm_set_priv | function | vm/utility.c:L200 | ❌ (test util) | ❌ | — | — |
```

Coverage statistics:
```markdown
| Metric | Count | Rate |
|--------|-------|------|
| Total symbols in scope | 45 | — |
| Covered | 38 | 84.4% |
| Uncovered | 7 | 15.6% |
```

## Step 5.5: Data Structure Coverage

```markdown
| Struct | C Source | Fields | Analyzed in Doc? | Missing Fields | Priority |
|--------|---------|--------|-----------------|---------------|----------|
| struct vm_page | vm/pagetable.h:L20 | 4 fields | ✅ §2.2.1 | — | — |
| struct pt_range | vm/pagetable.h:L50 | 6 fields | Partial §2.3 | pt_flags, pt_type | P1 |
| struct vm_region | vm/region.h:L30 | 8 fields | ❌ | All | P0 |
```

## Step 5.6: Architecture Evolution

```markdown
| Aspect | Minix3 (32-bit) | minix-rs (64-bit) | Doc Annotates? | Priority |
|--------|----------------|-------------------|---------------|----------|
| Page table levels | 2 (PD+PT) | 4 (PML4+PDPT+PD+PT) | ❌ | P1 |
| Page table entry size | 32-bit (u32) | 64-bit (u64) | ❌ | P1 |
| Address split | 10+10+12 | 9+9+9+9+12 | ❌ | P1 |
| Page table pointer array | pt_pt[1024] | Dynamic allocation | ❌ | P1 |
| Address width | 32-bit linear | 48-bit virtual (4-level) | ❌ | P1 |
```

## Step 6: Design Decision Quality

### Design Decision Verification

```markdown
| Decision # | Content | Ch1&2 Basis | Scenario Coverage | no_std Compatible | Alternative Recorded | Priority |
|-----------|---------|------------|-------------------|------------------|--------------------|----------|
| DD-1 | Use buddy allocator for phys pages | §2.1 (buddy source analysis) | Full (alloc/free/merge) | ✅ | ❌ (slab also viable) | P1 |
| DD-2 | vm_page as u64 newtype | §2.2.1 (C struct vm_page) | Partial (arch-dependent bits missing) | ✅ | ✅ | P1 |
```

### Error Path Coverage Table

```markdown
| Error Path | C Source Scene | Doc Addresses? | Priority |
|-----------|---------------|----------------|----------|
| Out of memory | buddy_alloc returns NULL | ❌ | P0 |
| Invalid address | vm_acl_ok returns 0 | ❌ | P0 |
| Slot exhaustion | vm_slot allocation fail | Partial (§3.4 mentions) | P1 |
```

## Step 7: Chapter Linkage

### Table 1: Ch3 → Ch1&2

```markdown
| Design Decision | Ch1 Basis | Ch2 Basis | Status | Priority |
|----------------|-----------|-----------|--------|----------|
| DD-1 (Buddy allocator) | ✅ "Physical Memory" | ✅ §2.1 buddy source | Linked | — |
| DD-2 (vm_page newtype) | ✅ "Page Tables" | ✅ §2.2.1 vm_page struct | Linked | — |
| DD-3 (Region optimization) | ❌ (no mention in Ch1) | Partial (§2.3 no performance data) | Weak | P1 |
```

### Table 2: Ch4 → Ch3

```markdown
| Implementation | Design Decision Base | Match? | Priority |
|---------------|---------------------|--------|----------|
| BuddyAlloc::alloc() | CH3-DD1 (buddy allocator) | ✅ | — |
| PageTable::map() | CH3-DD2 (vm_page newtype) | ✅ | — |
| None (orphan) | — | ❌ No DD for mmap() | P1 |
```

### Table 3: Tests → Ch3+Ch4

```markdown
| Test | Covers Design | Covers Implementation | Match? | Priority |
|------|--------------|----------------------|--------|----------|
| test_buddy_alloc_basic | ✅ DD-1 (buddy) | ✅ BuddyAlloc::alloc() | ✅ | — |
| test_buddy_free_merge | ✅ DD-1 (buddy) | ✅ BuddyAlloc::free() | ✅ | — |
| DD-4 (Error recovery) | ❌ No test | ❌ No test | ❌ | P1 |
```

### Table 4: Code → Ch4

```markdown
| Rust Module | Ch4 Section | Signature Match? | Behavior Match? | Priority |
|------------|------------|-----------------|----------------|----------|
| buddy.rs | §4.2 | ✅ | ✅ | — |
| pagetable.rs | §4.3 | ✅ | Partial (missing arch feature gate) | P1 |
```

### Linkage Summary

```markdown
| Linkage | Covered | Total | Rate | Health |
|---------|---------|-------|------|--------|
| Ch3→Ch1&2 | 18 | 22 | 81.8% | ⚠️ (4 weak) |
| Ch4→Ch3 | 15 | 16 | 93.8% | ✅ |
| Tests→Ch3+Ch4 | 12 | 16 | 75.0% | ❌ (4 untested) |
| Code→Ch4 | 16 | 16 | 100% | ✅ |
```

## Step 11: Semantic Ownership

```markdown
| Symbol | Type | Owner Doc | Current Coverage | Recommendation |
|--------|------|-----------|-----------------|---------------|
| vm_slot | struct | vm/process.md §2.2 | Covered | — (singleton in vm) |
| ENOMEM | constant | kernel/errno.md §1.3 | Can reference | Shared constant, keep single def |
| do_fork | function | pm/process.md §2.4 | Covered | — (belongs to PM) |
| NR_PT | macro | vm/pagetable.md §2.1 | Not covered | Add to pagetable.md |
```

## Final Output

```markdown
# Review Report: {file name}

## 1. Summary
- **Document**: {file name} ({N} lines, {M} sections)
- **Ground Truth**: {minix3 source files}
- **Overall Rating**: 🟢 Pass / 🟡 Needs Work / 🔴 Major Issues
- **Key Findings**: {1-2 sentence summary of most important issues}

## 2. Dimension Coverage Self-Check

| Dimension | Executed? | Result | Time |
|-----------|-----------|--------|------|
| Concept accuracy (P0) | ✅/❌/⏭️ | Pass/Fail/Warn | {min} |
| C code ref verification (P0) | ✅/❌/⏭️ | Pass/Fail/Warn | {min} |
| C source coverage (P0) | ✅/❌/⏭️ | Pass/Fail/Warn | {min} |
| Data structure coverage (P1) | ✅/❌/⏭️ | Pass/Fail/Warn | {min} |
| Architecture evolution (P1) | ✅/❌/⏭️ | Pass/Fail/Warn | {min} |
| Design decision quality (P0/P1) | ✅/❌/⏭️ | Pass/Fail/Warn | {min} |
| Chapter linkage (P1) | ✅/❌/⏭️ | Pass/Fail/Warn | {min} |
| Doc-code consistency (P1) | ✅/❌/⏭️ | Pass/Fail/Warn | {min} |
| Diagram quality (P2) | ✅/❌/⏭️ | Pass/Fail/Warn | {min} |
| Pedagogical quality (P1) | ✅/❌/⏭️ | Pass/Fail/Warn | {min} |
| Cross-document (P2) | ✅/❌/⏭️ | Pass/Fail/Warn | {min} |

## 3. Detailed Results

{Concept verification, code reference verification, coverage completeness, etc.}

## 4. Issue List

| # | Priority | Location | Description | Evidence (C Source) | Fix |
|---|----------|---------|-------------|-------------------|-----|
| 1 | P0 | §2.1 L45 | NR_REGIONS = 256, actual is 5 | vm/const.h:L15 `#define NR_REGIONS 5` | Correct to 5 |
| 2 | P1 | §3.2 | Design DD-5 lacks Ch1&2 basis | No corresponding analysis in §2 | Add basis or remove decision |

## 5. Cross-Document Findings

| Finding | Type | Affected Documents | Fix |
|---------|------|-------------------|-----|
| vm_slot defined in both vm/process.md and pm/process.md | Duplicate | vm/process.md, pm/process.md | Designate vm/process.md §2.2 as canonical |

## 6. Weakest-Item Self-Check

Most problematic finding with highest priority, including root cause and fix.

## 7. Time Budget Evaluation

| Planned | Actual | Difference |
|---------|--------|-----------|
| {N} min | {M} min | {Diff} min |
```