# Don't Over-Simulate C

Rewrite's goal is not to replicate Minix3's historical implementation details. If a design exists solely due to C limitations, prefer modern Rust.

## Design Constraints to Escape

| C Limitation | Modern Rust Alternative |
|-------------|----------------------|
| No generics | Use Rust generics + trait bounds |
| No pattern matching | Use `match` + destructuring |
| No RAII | Use `Drop` + RAII |
| No type system (raw pointers) | Use strong types, `Option<T>`, `Result<T, E>` |
| 32-bit fixed arrays | Use 64-bit dynamic collections |
| Implicit casts | Use explicit `From`/`Into` |
| Manual error propagation | Use `Result<T, E>` + `?` |
| Manual free() | Use `Drop` |

## Examples

### Intrusive Linked List
```rust
// ❌ Wrong: replicating C's intrusive linked list
struct ListNode { next: *mut ListNode, ... }

// ✅ Correct: use Rust safe abstractions
struct FreeList { head: Option<Box<Node>> }
```

### Error Code Passing
```rust
// ❌ Wrong: C-style error return
fn do_something() -> i32 {
    if fail { return -1; }
    0
}

// ✅ Correct: Rust Result
fn do_something() -> Result<(), VmError> {
    if fail { return Err(VmError::Enomem); }
    Ok(())
}
```

### Flag Combinations
```rust
// ❌ Wrong: C-style bit flags
const ALLOC_ZERO: u32 = 0x01;
const ALLOC_CONTIG: u32 = 0x02;
fn alloc(flags: u32) -> ... { ... }

// ✅ Correct: bitflags macro
bitflags! {
    pub struct AllocFlags: u32 {
        const ZERO = 0x01;
        const CONTIG = 0x02;
    }
}
fn alloc(flags: AllocFlags) -> ... { ... }
```

### State Machine
```rust
// ❌ Wrong: C-style int state + macros
const STATE_INIT: i32 = 0;
const STATE_RUNNING: i32 = 1;
struct Proc { state: i32, ... }

// ✅ Correct: Rust enum
enum ProcState { Init, Running, Blocked, Dying }
struct Proc { state: ProcState, ... }
```

## Judgment Boundary

| C Pattern | Should Keep? | Rationale |
|-----------|-------------|-----------|
| IPC message format | ✅ | Must match kernel ABI |
| errno values | ✅ | Must match Minix3 protocol |
| Process lifecycle order | ✅ | Must match kernel expectations |
| Internal data structure layout | ❌ | C limitation, Rust can do better |
| Manual memory management patterns | ❌ | Use RAII |
| Sentinel-based loops | ❌ | Use iterators |
| Function pointer dispatch tables | ❌ | Use traits |