# Complexity & Engineering Quality Check (§11)

Verify that the codebase maintains engineering quality without unnecessary complexity.

## §11.1: Unnecessary Abstraction (P1)

| Check Item | Assessment |
|-----------|-----------|
| Deep trait hierarchies (>2 levels)? Simple logic behind many layers? | ✅/❌ |
| Generics used where a single concrete type would suffice? | ✅/❌ |
| Macro-generated code obscuring logic that should be explicit? | ✅/❌ |
| Abstractions with only one implementation? (YAGNI violation) | ✅/❌ |

```rust
// ❌ Wrong: 3-level trait hierarchy for simple page allocation
trait Allocator { fn alloc(&self) -> ...; }
trait PageAllocator: Allocator { fn alloc_pages(&self) -> ...; }
trait BuddyAllocator: PageAllocator { fn buddy_alloc(&self) -> ...; }
struct Buddy { ... }
// → 3 traits for one concrete type: remove 2 levels

// ✅ Correct: single trait with clear purpose
trait FrameAlloc {
    fn alloc(&mut self, order: Order) -> Result<PhysAddr, AllocError>;
    fn free(&mut self, addr: PhysAddr, order: Order);
}
```

## §11.2: Platform Isolation (P1)

| Check Item | Assessment |
|-----------|-----------|
| arch-specific code in `arch/` or `arch64/`, not mixed with OS logic? | ✅/❌ |
| OS logic depends on trait, not on arch-specific types? | ✅/❌ |
| `#[cfg(target_arch = "...")]` used minimally, only in arch module? | ✅/❌ |
| Platform constants (page size, address width) defined in arch module? | ✅/❌ |

## §11.3: Macro Hygiene (P1)

| Check Item | Assessment |
|-----------|-----------|
| Macros only used for boilerplate reduction (not logic hiding)? | ✅/❌ |
| Macro-generated items have clear naming and documentation? | ✅/❌ |
| `macro_rules!` scoped properly (`#[macro_export]` vs `pub(crate)`)? | ✅/❌ |
| Procedural macros isolated in separate crate? | ✅/❌ |

```rust
// ❌ Wrong: macro hides control flow
macro_rules! handle_msg {
    ($msg:expr) => {
        match $msg.m_type {
            DO_FORK => do_fork($msg),
            DO_EXEC => do_exec($msg),
            // implicit return, error handling invisible
        }
    };
}

// ✅ Correct: explicit, readable dispatch
fn handle_message(msg: &Message) -> Result<(), VmError> {
    match msg.m_type {
        DO_FORK => do_fork(msg),
        DO_EXEC => do_exec(msg),
        _ => Err(VmError::Ebadcall),
    }
}
```

## §11.4: Feature Flags (P2)

| Check Item | Assessment |
|-----------|-----------|
| `#[cfg(feature = "...")]` used where feature-gating is needed? | ✅/❌ |
| Feature flags correspond to compile-time decisions (not runtime)? | ✅/❌ |
| All feature combinations compilable and tested? | ✅/❌ |

## §11.5: Dependency Cost (P2)

| Check Item | Assessment |
|-----------|-----------|
| Each dependency justified? (used in ≥1 code path) | ✅/❌ |
| No dependency brings entire `std`? | ✅/❌ |
| Dependency tree depth reasonable? (no deep transitive chains) | ✅/❌ |
| `Cargo.lock` checked in? | ✅/❌ |

## §11.6: Code Duplication (P1)

| Check Item | Assessment |
|-----------|-----------|
| Similar logic in multiple places that could be unified? | ✅/❌ |
| Copy-pasted error handling that should be centralized? | ✅/❌ |
| Duplicated IPC message parsing code? | ✅/❌ |

## Summary

```markdown
| Sub-dimension | Result | Issues | Priority |
|--------------|--------|--------|----------|
| Unnecessary abstraction | Pass/Warn/Fail | {count} | P1 |
| Platform isolation | Pass/Warn/Fail | {count} | P1 |
| Macro hygiene | Pass/Warn/Fail | {count} | P1 |
| Feature flags | Pass/Warn/Fail | {count} | P2 |
| Dependency cost | Pass/Warn/Fail | {count} | P2 |
| Code duplication | Pass/Warn/Fail | {count} | P1 |
```