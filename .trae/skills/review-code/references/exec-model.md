# Execution Model & Concurrency Check (§4)

Verify that Rust code correctly implements Minix3's event-driven, single-threaded execution model.

## §4.1: Event Loop Structure (P0)

Minix3 servers use a signal-driven main loop: `get_work()` → handle message → `reply()` → loop. Verify:

| Check Item | Assessment |
|-----------|-----------|
| Main loop matches C's `get_work()` / `reply()` pattern? | ✅/❌ |
| IPC receive is blocking/polling, matching C's `receive()`? | ✅/❌ |
| Signal handling matches C's `sigaction` / `setjmp` semantics? | ✅/❌ |
| Timer/alarm handling matches C's `alarm()` / `SIGALRM`? | ✅/❌ |

```rust
// ✅ Correct: Minix3-style event loop
loop {
    let msg = sef_receive()?;
    let result = handle_message(msg);
    sef_reply(result)?;
}
```

## §4.2: Single-Threaded Consistency (P1)

| Check Item | Assessment |
|-----------|-----------|
| No `thread::spawn` or `std::thread` in production code? | ✅/❌ |
| `Rc`/`RefCell` preferred over `Arc`/`Mutex`? | ✅/❌ |
| No `Atomic*` types in production code? | ✅/❌ |
| `!Send`/`!Sync` on types not designed for cross-thread use? | ✅/❌ |

```rust
// ❌ Wrong: Mutex in single-threaded server (unnecessary overhead)
let data = Arc::new(Mutex::new(ServerState::new()));

// ✅ Correct: Rc + RefCell for single-threaded
let data = Rc::new(RefCell::new(ServerState::new()));
```

## §4.3: IPC-Based Concurrency (P1)

Minix3 servers communicate via synchronous IPC, not shared memory.

| Check Item | Assessment |
|-----------|-----------|
| IPC message passing, no shared mutable state between servers? | ✅/❌ |
| Message types match C's `message` union layout? | ✅/❌ |
| Message dispatch matches C's handler table? | ✅/❌ |
| Reply format matches C's reply message layout? | ✅/❌ |

## §4.4: Blocking & Async Patterns (P1)

| Check Item | Assessment |
|-----------|-----------|
| Blocking I/O patterns consistent with C's blocking `send()`/`receive()`? | ✅/❌ |
| No async runtime (tokio, async-std) in production code? | ✅/❌ |
| Interruptible operations match C's signal-safety? | ✅/❌ |

## §4.5: Process Lifecycle (P1)

| Check Item | Assessment |
|-----------|-----------|
| Fork/exec semantics match C's do_fork()/do_exec()? | ✅/❌ |
| Signal delivery matches C's check_sig()/sig_proc()? | ✅/❌ |
| Exit/cleanup matches C's pm_exit()? | ✅/❌ |
| Zombie reaping matches C's waitpid()? | ✅/❌ |

## §4.6: Concurrency Bug Patterns

| Pattern | Symptom | Priority |
|---------|---------|----------|
| Unsynchronized access to shared state | Data race (even in single-thread, if via interrupt) | P0 |
| Re-entrant handler without guard | Signal handler re-enters non-reentrant code | P0 |
| Deadlock via circular IPC | Server A waits for B, B waits for A | P0 |
| Missing `!Send` on RefCell-held types | Accidental cross-thread send compiles but panics at runtime | P1 |

## §4.7: Server Bootstrap Sequence

| Check Item | Assessment |
|-----------|-----------|
| Initialization order matches C's main() sequence? | ✅/❌ |
| SEF initialization matches C's sef_startup()? | ✅/❌ |
| State machine initialized to correct starting state? | ✅/❌ |

## Summary

```markdown
| Sub-dimension | Result | Issues | Priority |
|--------------|--------|--------|----------|
| Event loop | Pass/Warn/Fail | {count} | P0/P1 |
| Single-threaded | Pass/Warn/Fail | {count} | P1 |
| IPC concurrency | Pass/Warn/Fail | {count} | P1 |
| Blocking patterns | Pass/Warn/Fail | {count} | P1 |
| Process lifecycle | Pass/Warn/Fail | {count} | P1 |
| Concurrency bugs | Pass/Warn/Fail | {count} | P0/P1 |
| Bootstrap | Pass/Warn/Fail | {count} | P1 |
```