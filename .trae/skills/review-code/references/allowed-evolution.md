# Allowed Architecture Evolution

These changes are **architecture evolution** (allowed), not Redesign. Each evolution must pass all four criteria.

## Evolution Table

| Evolution | Minix3 (C/32-bit) | minix-rs (Rust/64-bit) | Reason |
|-----------|-------------------|----------------------|--------|
| Address width | x86-32 | x86-64 | 64-bit address space |
| Page table levels | 2 (PD+PT) | 4 (PML4+PDPT+PD+PT) | 64-bit requires more levels |
| Page table entry size | 32-bit (u32) | 64-bit (u64) | Larger physical addresses |
| State expression | `int state` + macros | `enum` / `typestate` | Invalid states unrepresentable |
| Error handling | return OK/errno | `Result<T, Error>` | Error codes must strictly correspond |
| Resource management | manual `free()` | RAII / `Drop` | Automatic release, prevent leaks |
| Permission control | `bitchunk_t` array | `bitflags` / `enum` | Type safety |
| Memory allocation | VM private slab | `alloc` + custom global allocator | `no_std` uses `alloc` crate |

## Judgment Criteria — All Four Must Hold

A change is Rewrite (allowed) if ALL of:

1. **External observable behavior unchanged** — IPC interface, return values, side effects
2. **IPC protocol unchanged** — message format, call numbers, permission checks
3. **Lifecycle semantics unchanged** — create, use, release order
4. **Scheduling/permission/address space semantics unchanged**

If ANY criterion fails → Redesign (currently forbidden).

## Judgment Examples

| Change | Criteria 1 | Criteria 2 | Criteria 3 | Criteria 4 | Verdict |
|--------|-----------|-----------|-----------|-----------|---------|
| enum instead of int state | ✅ | ✅ | ✅ | ✅ | Rewrite |
| Page table: 2-level→4-level | ✅ | ✅ | ✅ | ✅ | Rewrite |
| alloc instead of VM slab | ✅ | ✅ | ✅ | ✅ | Rewrite |
| New IPC message type | ❌ | ❌ | ✅ | ✅ | Redesign |
| Changed process lifecycle | ✅ | ✅ | ❌ | ✅ | Redesign |

> **Note**: Architecture evolution — even when allowed — must be annotated with a comment explaining both the C behavior and the Rust approach, so reviewers can distinguish "intentional evolution" from "accidental drift". See [alignment-detail.md](alignment-detail.md).