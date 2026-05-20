# Alignment Detail (§14.5 & §14.7)

## §14.5: Architecture Evolution — Function Disappearance

When architecture evolution causes a C function to have no corresponding Rust function:

### Disappearance Categories

| Category | Example | Acceptable? | Requirement |
|----------|---------|------------|-------------|
| **Merge** | C's `alloc_pages()` + `map_pages()` merged into Rust's `alloc_and_map()` | ✅ | Comment on both C functions explaining merge |
| **Inline** | C's `set_flag(ptr, FLAG)` becomes `ptr.flag = true` | ✅ | Comment at access site |
| **Absorb by type system** | C's `is_valid_addr()` becomes Rust's `VirtAddr` (invalid states unconstructable) | ✅ | Document in design decision |
| **Drop (missing)** | C has callback, Rust trait has no equivalent method | ❌ P1 | Must add trait method |

### Verification Process

1. List all C functions in the module: `rg "^void\|^int\|^static" minix3/.../module/ --type c`
2. Map each to its Rust equivalent
3. For unmatched C functions, determine disappearance category
4. Verify annotation exists for merge/inline/absorb cases
5. Flag Drop cases as P1

## §14.7: Review Execution Strategy

### Priority Order for Code Review

1. **Safety-critical first**: `unsafe` blocks, raw pointers, memory alloc/free
2. **Interface correctness**: IPC message handling, kernel ABI compliance
3. **Semantic alignment**: Leaf functions, error codes, side effects
4. **Type safety**: typestate, newtypes, trait design
5. **Quality**: naming, comments, module organization, tests

### Leaf Function Priority Sampling

Leaf functions are high risk for semantic drift. Prioritize:
- **Error-returning functions** — most likely to have misaligned error codes
- **Memory alloc/free functions** — most likely to have ownership confusion
- **State transition functions** — most likely to have typestate bypass
- **IPC handler functions** — most likely to have protocol mismatch

### Corrective Misalignment Comment Template

```rust
// Minix3 bug: anon_contig_reference() returns ENOMEM but region.c ignores
// the return value (line 841). Rust fix: ev_copy returns NotSupported to
// correctly reject fork for contiguous memory regions.
fn ev_copy(&self) -> Result<(), VmError> {
    Err(VmError::NotSupported)
}
```

### Architecture Evolution Comment Template

```rust
// Architecture evolution (32→64 bit): In Minix3, pt_pt[] is a fixed array
// of 1024 entries (2-level page table, 32-bit). Rust uses 4-level paging
// (64-bit) with dynamic allocation of page table pages via the frame
// allocator. The pt_pt[] concept is replaced by PageTable::walk() which
// traverses the multi-level structure on-demand.
fn walk(&self, vaddr: VirtAddr) -> Result<&mut PageTableEntry, PageFault> {
    // ...
}
```