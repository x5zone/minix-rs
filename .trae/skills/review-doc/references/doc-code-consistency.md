# Document-Code Consistency Check (§2.4)

Verify that Rust code descriptions in the document match the actual Rust source code.

## Check Dimensions

### 1. Signature Match

Extract function signatures from the document and compare against actual Rust code.

```markdown
| Function | Doc Signature | Actual Signature | Match? | Priority |
|----------|-------------|-----------------|--------|----------|
| buddy_alloc | `fn alloc(order: Order) -> Result<PhysAddr>` | `fn alloc(order: Order) -> Result<PhysAddr, AllocError>` | ❌ | P1 |
| pt_map | `fn map(&self, virt: VirtAddr, phys: PhysAddr)` | `fn map(&mut self, virt: VirtAddr, phys: PhysAddr, flags: PageFlags)` | ❌ | P1 |
```

Check: return type, parameter count, parameter types, mutability, error types.

### 2. Parameter/Return Type Match

Compare every type in documented signatures against actual code.

| Function | Parameter | Doc Type | Actual Type | Match? | Priority |
|----------|-----------|---------|-------------|--------|----------|
| alloc | count | `u32` | `PageCount` | ❌ (should be newtype) | P1 |
| alloc | flags | `u32` | `PageFlags` | ❌ (should be newtype) | P1 |

### 3. Behavior Description Match

Compare documented behavior paragraphs against actual implementation.

| Behavior Claim | Doc Location | Actual Code Behavior | Match? | Priority |
|---------------|-------------|---------------------|--------|----------|
| "buddy_alloc searches free list for exact-order block" | §4.2 L45 | Code splits larger blocks, returns smallest fitting | ❌ | P0 |
| "pt_bind returns Ok on success" | §4.3 L120 | Code returns Result with more detailed error variants | Partial | P1 |

Method: read the actual Rust function, trace all branches, compare with every claim in the document.

### 4. Example Code Compilability

Extract all ` ```rust ` code blocks from the document. For each:

| Code Block | Doc Location | Compiles? | Issues | Priority |
|-----------|-------------|-----------|--------|----------|
| ```rust ... ``` | §4.2 L30-45 | ❌ | Missing `use` for PageFlags | P1 |
| ```rust ... ``` | §4.3 L60-80 | ✅ | — | — |

### 5. Trait Implementation Consistency

If document describes trait implementations, verify they exist and match.

| Trait | Doc says impl'd for | Actually impl'd for | Match? | Priority |
|-------|--------------------|--------------------|--------|----------|
| PageTableOps | BuddyAlloc | PageTable | ❌ (wrong type) | P1 |
| Allocator | BuddyAlloc | ✅ | ✅ | — |

### 6. Module Path Match

Documented Rust module paths vs actual paths.

| Module | Doc Path | Actual Path | Exists? | Priority |
|--------|---------|-------------|---------|----------|
| buddy | `vm::buddy` | `src/vm/buddy.rs` | ✅ | — |
| pagetable | `vm::pagetable` | `src/vm/page_table.rs` | ❌ (name differs) | P2 |

## Verification Process

1. Extract all function signatures in ` ```rust ` blocks from document
2. Grep the actual codebase: `rg "fn FUNCTION_NAME" src/ -n`
3. Compare line by line
4. For behavior claims, read the actual function body
5. Compile example code blocks if possible
6. Check module paths with `ls`

## Judgment

| Finding | Priority |
|---------|----------|
| Signature mismatch | P1 |
| Behavior description contradicts code | P0 |
| Missing `use` in example | P1 |
| Path mismatch | P2 |
| Trait impl mismatch | P1 |
| Type mismatch (bare integer vs newtype) | P1 |