# Test Quality Check (§8)

Verify that test coverage and quality meet minix-rs project standards.

## §8.1: Unit Test Coverage (P1)

| Check Item | Assessment |
|-----------|-----------|
| Every public function has at least one test? | ✅/❌ |
| Error paths tested (not just happy path)? | ✅/❌ |
| Boundary conditions tested (empty, max, zero, null-equivalent)? | ✅/❌ |
| State machine: all transitions tested? | ✅/❌ |
| Error codes match Minix3 definitions? | ✅/❌ |

```rust
// ❌ Wrong: only happy path tested
#[test]
fn test_alloc() {
    let result = alloc(Order::new(0));
    assert!(result.is_ok());
}

// ✅ Correct: happy + error + boundary
#[test]
fn test_alloc_success() { ... }
#[test]
fn test_alloc_oom() { ... }
#[test]
fn test_alloc_order_zero() { ... }
#[test]
fn test_alloc_order_max() { ... }
```

## §8.2: Integration Test Coverage (P1)

| Check Item | Assessment |
|-----------|-----------|
| IPC scenarios: each message type tested end-to-end? | ✅/❌ |
| Multi-server interaction: PM→VM, VFS→PM, etc. tested? | ✅/❌ |
| Error recovery: server handles invalid IPC messages gracefully? | ✅/❌ |
| Concurrency: interleaving scenarios tested? | ✅/❌ |

## §8.3: Test Isolation (P1)

| Check Item | Assessment |
|-----------|-----------|
| Tests in `#[cfg(test)]` modules or separate test crates? | ✅/❌ |
| Test code does NOT use `std::` outside `#[cfg(test)]`? | ✅/❌ |
| Each test independent? (no test-order dependency) | ✅/❌ |
| Mocks/doubles for external dependencies? | ✅/❌ |

## §8.4: Test Quality

| Check Item | Assessment |
|-----------|-----------|
| Assertions test meaningful properties (not trivially true)? | ✅/❌ |
| Test names describe what's being tested? | ✅/❌ |
| Complex setups extracted to helper functions? | ✅/❌ |
| Property-based tests for complex state machines? | ✅/❌ |

```rust
// ❌ Wrong: meaningless assertion
#[test]
fn test_init() {
    let proc = VmProc::new();
    assert!(true); // always passes, tests nothing
}

// ✅ Correct: meaningful assertion
#[test]
fn test_init_creates_empty_page_table() {
    let proc = VmProc::new(Pid::from(1));
    assert!(proc.page_table.is_empty());
    assert_eq!(proc.state(), ProcState::Init);
}
```

## §8.5: Property-Based & Fuzz Testing (P2)

| Check Item | Assessment |
|-----------|-----------|
| State machine transitions: proptest for arbitrary sequences? | ✅/❌ |
| Input parsing: fuzz targets for message deserialization? | ✅/❌ |
| Allocation stress: random alloc/free sequences? | ✅/❌ |

## §8.6: Test for C-Rust Alignment

| Check Item | Assessment |
|-----------|-----------|
| Leaf functions: test cases mirror C's behavior? | ✅/❌ |
| Error codes: test asserts correct Minix3 errno values? | ✅/❌ |
| Side effects: test verifies same observable state as C? | ✅/❌ |

## Summary

```markdown
| Sub-dimension | Result | Issues | Priority |
|--------------|--------|--------|----------|
| Unit test coverage | Pass/Warn/Fail | {count} | P1 |
| Integration coverage | Pass/Warn/Fail | {count} | P1 |
| Test isolation | Pass/Warn/Fail | {count} | P1 |
| Test quality | Pass/Warn/Fail | {count} | P1 |
| Property/Fuzz | Pass/Warn/Fail | {count} | P2 |
| C-Rust alignment tests | Pass/Warn/Fail | {count} | P1 |
```