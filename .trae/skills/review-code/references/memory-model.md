# Memory Model & State Expression Check (§5)

Verify that Rust code's memory layout, state expression, and ownership model correctly implement Minix3 semantics.

## §5.1: Memory Layout Semantics (P1)

| Check Item | Assessment |
|-----------|-----------|
| Physical vs virtual address separation clear? | ✅/❌ |
| Address types (`PhysAddr`, `VirtAddr`) are newtypes, not bare `u64`? | ✅/❌ |
| Page-alignment constraints enforced at type level or assertion? | ✅/❌ |
| Memory region boundaries match C's page table structure? | ✅/❌ |

```rust
// ❌ Wrong: no phys/virt distinction
fn alloc(addr: u64, size: u64) { ... }

// ✅ Correct: type-level distinction
fn alloc(addr: PhysAddr, size: PageCount) -> Result<PhysAddr, AllocError> { ... }
```

## §5.2: State Machine Expression (P1)

| Check Item | Assessment |
|-----------|-----------|
| All states explicitly enumerated (enum, not int)? | ✅/❌ |
| State transitions explicit and exhaustive? | ✅/❌ |
| Invalid states unrepresentable at compile time? | ✅/❌ |
| Typestate transitions type-safe? No bypass paths? | ✅/❌ |

```rust
// ❌ Wrong: C-style int states, invalid states representable
const S_INIT: u8 = 0;
const S_RUNNING: u8 = 1;
struct Proc { state: u8 } // could be 42

// ✅ Correct: exhaustive enum
enum ProcState { Init, Running, Blocked, Dying }
struct Proc { state: ProcState }
```

### Typestate Pattern (when complexity justified)

```rust
// ✅ Typestate: transition methods consume self, return next state
struct InitProc { pid: Pid }
impl InitProc {
    fn start(self, entry: VirtAddr) -> RunningProc { ... }
}
struct RunningProc { pid: Pid, pt: PageTable }
// InitProc::start() consumes InitProc → no double-start possible
```

## §5.3: Ownership Model (P1)

| Check Item | Assessment |
|-----------|-----------|
| Each allocation has a clear Owner type? | ✅/❌ |
| Owner type's Drop cleans up the resource? | ✅/❌ |
| Borrow/Clone semantics explicit, no accidental sharing? | ✅/❌ |
| C's `free()` points all have corresponding Rust `Drop` implementations? | ✅/❌ |

```rust
// ❌ Wrong: unclear who owns page_table, who frees it
fn do_fork(proc: &VmProc) {
    let pt = proc.page_table.clone();
    // Who frees pt? When?
}

// ✅ Correct: ownership transferred to child
fn do_fork(proc: &VmProc) -> Result<VmProc, VmError> {
    let child_pt = proc.page_table.copy()?;
    Ok(VmProc { page_table: child_pt, .. })
} // child_pt freed when VmProc dropped
```

### Ownership Mapping: C → Rust

| C Pattern | Rust Pattern |
|-----------|-------------|
| `malloc()` → `free()` | Constructor → `Drop` |
| Caller-allocated buffer | `&mut [u8]` parameter |
| Callee-allocated buffer | Return `Box<[u8]>` or `Vec<u8>` |
| Reference counting (C `ref_t`) | `Rc<T>` |
| Global singleton | `static` or `OnceLock` |

## §5.4: Drop Semantic Correctness (P1)

| Check Item | Assessment |
|-----------|-----------|
| Drop order matches C's cleanup order? | ✅/❌ |
| Drop is idempotent? (no double-free via manual Drop + field Drop) | ✅/❌ |
| Panic in Drop handled? (usually `std::mem::forget` or `ManuallyDrop` in no_std) | ✅/❌ |
| Resource release in Drop matches C's error-path cleanup? | ✅/❌ |

## Summary

```markdown
| Sub-dimension | Result | Issues | Priority |
|--------------|--------|--------|----------|
| Memory layout | Pass/Warn/Fail | {count} | P1 |
| State expression | Pass/Warn/Fail | {count} | P1 |
| Ownership | Pass/Warn/Fail | {count} | P1 |
| Drop semantics | Pass/Warn/Fail | {count} | P1 |
```