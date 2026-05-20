# Error Patterns (Code)

Concrete anti-patterns to recognize during code review. Each has ❌ wrong and ✅ correct examples.

## Pattern 15: Bare Integers for Semantics (Translate Smell)

```rust
// ❌ Wrong: bare u32/i32 for semantic values
fn alloc_pages(count: u32, flags: u32) -> i32 { ... }

// ✅ Correct: newtype + Result
fn alloc_pages(count: PageCount, flags: PageFlags) -> Result<PhysAddr, AllocError> { ... }
```

## Pattern 16: C-Style Null/Sentinel Values

```rust
// ❌ Wrong: sentinel values for "no value"
const NO_PHYS: PhysAddr = PhysAddr(0);
let parent_id = -1 as i32;

// ✅ Correct: Option<T>
let parent_id: Option<ProcessId> = None;
let phys: Option<PhysAddr> = None;
```

## Pattern 17: unsafe Without Safety Contract

```rust
// ❌ Wrong: no safety comment
let ptr = addr as *mut u8;
unsafe { *ptr = value; }

// ✅ Correct: safety contract documented
/// SAFETY: `addr` must be a valid, aligned physical address within the
/// mapped page range. Caller guarantees the page is writable.
unsafe fn write_phys(addr: PhysAddr, value: u8) { ... }
```

## Pattern 18: Error Codes Not Aligned with Minix3

```rust
// ❌ Wrong: invented error code
return Err(Error::NotFound);  // should be EINVAL per Minix3

// ✅ Correct: errno strictly corresponds to Minix3
return Err(VmError::Einval);  // Minix3's EINVAL
```

## Pattern 19: Lossy `as` Cast Without Safety Comment

```rust
// ❌ Wrong: u64→u16 truncation without comment
let old_count = pages as u16;

// ✅ Correct: comment explains why truncation is safe
// Page count is bounded by MAX_PAGES (1024), fits in u16.
let old_count = pages as u16;
```

## Pattern 20: Hardware Semantics Leak to OS Layer

```rust
// ❌ Wrong: CR3 register in OS-layer struct
struct PageTable { cr3_value: u64, }

// ✅ Correct: abstracted through trait
trait Paging {
    fn load_table(&self, table: PhysAddr);
    fn enable(&self);
}
```

## Pattern 21: no_std Violation

```rust
// ❌ Wrong: std in production code
use std::collections::HashMap;

// ✅ Correct: alloc or no_std-compatible crate
use alloc::collections::BTreeMap;
```

## Pattern 22: pub Abuse

```rust
// ❌ Wrong: all fields pub
pub struct VmProc {
    pub id: u32,
    pub state: State,
    pub page_table: PageTable,
}

// ✅ Correct: minimum visibility
pub struct VmProc {
    pub(crate) id: u32,
    pub(crate) state: State,
    page_table: PageTable,  // private, access via methods
}
```

> Judgment: "Is this pub because external needs it, or because internal is too lazy to organize?"

## Pattern 23: Type Safety Over-Engineering

```rust
// ❌ Wrong: type explosion when runtime state transitions dominate
struct InitVmProc { ... }
struct RunningVmProc { ... }
struct BlockedVmProc { ... }
struct DyingVmProc { ... }

// ✅ Correct: simple enum + runtime check when complexity > benefit
enum VmState { Init, Running, Blocked, Dying }
struct VmProc { state: VmState, ... }
```

## Pattern 24: Unnecessary Trait Abstraction

```rust
// ❌ Wrong: all implementations identical, never used as trait bound
trait PhysAllocatorStats {
    fn memstats(&self) -> PhysMemStats;
}
// Never: fn foo<T: PhysAllocatorStats>(t: &T)
// Only: bitmap.memstats() / buddy.memstats()
// → Use inherent methods + enum dispatch instead

// ✅ Correct: ≥2 behaviorally different implementations + used as trait bound
trait Paging {
    fn map(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
        -> Result<(), PageTableError>;
}
// x86-64: 4-level page table; aarch64: different descriptor format
// fn paging_init<P: Paging>(p: &mut P) uses trait bound
```

> Judgment: ≥2 different implementations AND used as trait bound → valid trait. Otherwise → simplify.