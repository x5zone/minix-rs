# Error Patterns (Documentation)

## Pattern 1: Concept Confusion

Confusing similar-sounding but semantically different concepts.

```markdown
❌ Wrong: Slab max object size is 200 bytes
✅ Correct: Slab supports 200 sizes (SLABSIZES=200),
        max object size is 207 bytes (MAXSIZE = SLABSIZES-1+MINSIZE = 207)
```

```markdown
❌ Wrong: The BUDDY allocator manages all physical pages
✅ Correct: BUDDY manages physical MEMORY, returning pages; pages may cross component boundaries
```

## Pattern 2: Fabricated Facts

Concepts in document that don't exist in C source at all.

```markdown
❌ Wrong: VM has a "Region Manager" component
✅ Correct: VM uses region data structure. There is no "Region Manager" as an independent component
```

Check: grep concept name in C source — if zero hits, it's fabricated.

## Pattern 3: Translation Smell

Describing C implementation in C style without re-expressing in document's own model.

```markdown
❌ Wrong (not re-expressed):
PM maintains a hole list, each hole has base address and size,
first-fit algorithm finds first hole large enough.

✅ Correct (re-expressed):
PM models free memory as a first-fit HoleList, where each Hole =
[PhysAddr, size]. Alloc iterates the list, returning the first
hole where hole.size >= request.
```

## Pattern 4: Scene Loss

C source contains multiple scenes (calls, parents, state transitions), document only covers one.

```markdown
❌ Wrong: table_get only handles the case where table pointer is already valid.
✅ Correct: table_get has 3 scenes:
  1. table pointer invalid → initialize (GPT: l.23-45)
  2. table pointer valid → return directly (GPT: l.47-48)
  3. request capacity > table capacity → re-initialize (GPT: l.50-70)
```

## Pattern 5: Missing Data Structures

Struct defined in C source but completely unanalyzed in document.

```markdown
❌ Wrong: The document describes the VM page table mechanism but never mentions
  the struct vm_page structure that actually represents page table entries.

✅ Correct: Must analyze at minimum: struct vm_page, struct pt_range, struct region
```

## Pattern 6: Wrong Numeric Constants

Numeric value in document doesn't match C source.

```markdown
❌ Wrong: VM supports up to 256 regions
✅ Correct: NR_REGIONS = 5 in const.h, supports up to 5 region types
```

Check: grep each constant against C source, verify value and semantics.

## Pattern 7: Wrong Architecture Description

Algorithm or mechanism description differs from C source behavior.

```markdown
❌ Wrong: pagefault handler allocates a physical page, then calls pt_bind
✅ Correct: pagefault handler allocates a physical page (buddy_alloc),
  maps it via pt_ptalloc (which initializes page table entries using pt_init)
```

Check: read the actual C function from entry to return.

## Pattern 8: Over-interpretation

From a simple variable or comparison in C code, inferring a complex design goal.

```markdown
❌ Wrong: The SLAB allocator uses a "quota system" to limit memory usage
  per user process...
✅ Correct: C source has a variable named 'quota', but it's a simple limit
  on slab pages per slab type, not a per-process quota system.
```

Judgment: re-read C source. If desc has >2 sentences beyond code behavior → suspicious.

## Pattern 9: Code Block Semantic Misannotation

Code block comments describe different behavior from what the code does.

```markdown
❌ Wrong:
/* Dynamically allocate page tables as needed */
vm_allocpage(pagetype, pages, flags)
// Actually: vm_allocpage requests pages from PM, not dynamically allocates PTs

✅ Correct:
/* Request physical pages from PM via ALLOCMEM */
vm_allocpage(pagetype, pages, flags)
```

## Pattern 10: Missing Error Path

C source has error handling that document doesn't describe.

```markdown
❌ Wrong: Only describes successful path

✅ Correct: Must also describe error paths:
  EFAULT_SRC / EFAULT_DST: invalid address (vm_acl_ok)
  ENOMEM: insufficient memory (slot allocation failed)
  SIGSEGV: process terminated (fatal pagefault)
```

## Pattern 11: Missing Null/Invalid State

C code handles NULL pointer or invalid state, document doesn't mention boundary conditions.

```markdown
❌ Wrong: Describes only normal flow

✅ Correct: Must document:
  - vm_slot == NULL: allocate new slot
  - vm_slot != NULL && vm_slot->next == NULL: single slot satisfies
  - vm_slot != NULL && vm_slot->next != NULL: multi-slot traversal
  - out of memory during allocation: return ENOMEM
```

## Pattern 12: Incomplete Macro/Constant Table

Only listing a few macros, failing to capture all macros in C header that define operational parameters.

```markdown
❌ Wrong: Table only lists ALLOCMEM, FREE, ADDMAP
✅ Correct: Must include all operation codes: ALLOCMEM, ALLOCBYTES, FREEMEM,
  ADDMAP, GETNEXT, FREEUNUSED, GETNEDSEG, GETNEXT, etc.
```

## Pattern 13: Lossy Semantic Mapping

When mapping C→Rust, losing semantic information.

```markdown
❌ Wrong:
C: u32_t flags → Rust: u32
// Lost: the semantic meaning of flags

✅ Correct:
C: u32_t flags → Rust: PageFlags (bitflags with ALLOC_ZERO, ALLOC_CONTIG, ...)
// Preserved: each bit's meaning mapped to named flag variants
```

## Pattern 14: Stale Cross-References

Document references another document that has been updated or reorganized.

```markdown
❌ Wrong: See vm/overview.md for VM architecture
  // overview.md no longer exists, renamed to architecture.md

✅ Correct: Always grep for referenced file:
  rg "REFERENCED_FILE_NAME" notes/rewrite/ --files-with-matches
```

## Cross-Document Error Patterns

### Pattern A: Duplicate Definition

Same concept defined in two documents, possibly with inconsistencies.

```markdown
❌ Wrong:
vm/process.md: "vm_slot represents a per-process VM state in VM module"
pm/process.md: "vm_slot represents a per-process memory mapping in PM module"
// Two definitions, one needs to be designated as canonical

✅ Correct:
vm/process.md: Defines struct vm_slot (canonical definition with C source reference)
pm/process.md: References vm/process.md §2.2 for vm_slot definition,
  focuses on PM's usage (fork/exec scenarios)
```

Judgment: same definition appearing in ≥2 documents → designate one canonical, others reference.

Check (grep commands for cross-document duplicate detection):
```bash
# Find duplicate concept definitions across documents
rg -l 'CONCEPT_NAME' notes/rewrite/
# Compare definitions side by side
rg 'vm_slot' notes/rewrite/ -A 3
# Find all struct/constant definitions to spot duplicates
rg 'struct vm_slot|NR_PT|MAXSIZE' notes/rewrite/ -n
```

### Pattern B: Contradictory Definition

Two documents define the same value or semantic differently.

```markdown
❌ Wrong:
vm/memory.md: "VM manages up to 1024 page tables"
pm/memory.md: "VM reports 512 page table slots"
// Contradiction: 1024 vs 512

✅ Correct: Verify C source: NR_PT = 1024. Mark pm/memory.md as erroneous,
  suggest using NR_PT constant directly.
```

### Pattern C: Missing Cross-Reference

Document describes shared responsibility but doesn't reference sibling documents.

```markdown
❌ Wrong: vm/fork.md describes fork process without mentioning PM's fork logic.

✅ Correct: Add reference: "The fork process involves cooperation between PM and VM.
  PM handles process table (PM fork.c:100-250), VM handles memory mappings
  (VM fork.c:260-400)."
```