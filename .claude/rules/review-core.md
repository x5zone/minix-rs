# Review Core — always loaded

## Execution Model (by module type)
Review must check the correct concurrency model for the module being reviewed:

**User-space servers (VM/PM/VFS/RS/DS/INET)**: Single-threaded event loop. `Rc`/`RefCell`/`AssumeSyncCell`/`!Send`/`!Sync` reasonable. `UnsafeCell` safe under single-thread.

**Kernel**: SMP + BKL (Big Kernel Lock spinlock). Multi-CPU kernel execution with `CONFIG_SMP`. BKL is spinlock (busy-wait) — NO sleep/schedule/IPC inside critical section. `Rc`/`RefCell` NOT safe for cross-CPU sharing (need `Arc`+`Mutex`/`Atomic`). `UnsafeCell` safety must argue BKL-protection or per-CPU isolation, NOT "single-threaded".

## ⛔ PROHIBITED BEHAVIORS
1. **NEVER answer from memory.** Every claim about C code requires a grep/read result as evidence.
2. **NEVER skip a check.** If you cannot execute it, write "UNVERIFIED: <reason>".
3. **NEVER guess.** If uncertain, say so — do NOT fabricate.
4. **NEVER mark a check as ✅ without output.** ZERO findings = write "Checked N items, found 0 issues." Prove the check happened.
5. **NEVER let late checks decay.** Checks after #13 in a long session are at HIGH RISK of being ghosted. Pause before #14, re-read the rules.

## Ground Truth
```
Minix3 C source (minix3/) > Rust code (os/servers/) > docs (notes/) > AI analysis
```

## Review Terms
- **Translate**: 1:1 C→Rust. ❌ Forbidden.
- **Rewrite**: Same external behavior, Rust types internally. ✅ Goal.
- **Redesign**: Change architecture/protocol. ❌ Forbidden (without explicit approval).

## Priority
- **P0**: C source errors (concept/grep/coverage), UB/memory safety, `std::` violations, hardware semantics leak. Kernel: `Rc`/`RefCell` cross-CPU, BKL not held, sleep in spinlock.
- **P1**: Architecture diff unstated, design without basis, dev-journal style (✅❌🚧), doc-code mismatch, pub misuse, **detail imprecision** (wrong hardware claim in comment, context leak in universal struct, silently dropped returns, leaks without rationale, dubious comment reasons)
- **P2**: Readability, naming, cross-refs, diagram quality

## ⛔ MANDATORY: Explicit Skill Invocation

- **You MUST invoke Skill tools explicitly** via the available `Skill` function. NEVER rely on "rules already loaded" or "context already has it".
- For document review: invoke `review-doc-skill` + `review-patterns-skill`
- For code review: invoke `review-code-skill` + `review-patterns-skill`
- For full/deep review: invoke all relevant Skills in phases per `review-process-skill`
- The Skill Invocation Log in scan.md must reflect actual Skill tool calls, not planned/intended calls.

## ⛔ P0 Mandatory Checklist (Gate D, see patterns §0)
Every scan.md MUST include this table (5 items, each with grep evidence):
1. **§5 tests exist**: `rg "fn {test_name}" {rust_dir}` — missing → P0
2. **trait has ≥1 impl**: `rg "impl.*{TraitName}" {rust_dir}` — 0 impl → P0
3. **function in declared file**: `rg "fn {name}" {file}` — not found → P0
4. **core algorithm not stub**: `rg "todo!|unimplemented!|unreachable!|panic!" {rust_dir}` — stub → P0; `panic!` in non-test code that represents unimplemented functionality or a reachable unhandled path → treat as stub/unhandled path, must be justified in comment
5. **§4 signatures match**: compare doc §4 vs actual — mismatch → P0

**Strict pass rule**: Each item must be ✅. **PARTIAL / ⚠️ / "部分通过" / "基本通过" counts as ❌ FAIL.** Any ❌ item means Gate D failed → scan.md DRAFT.

## ⛔ Gate D-6: structure.md Skeleton Review (Doc Review Only)
Before correctness checks, generate `structure.md` (12-section skeleton analysis) to verify "what the reader reads" (orthogonal to correctness). Sections: 主题思想/目标读者/叙事主语/驱动方向/文档大纲/核心概念清单/跨架构统一抽象/双向闭环/叙事弧/元注释/裸概念复述/纵向链路映射. Template: `prompt/skill/review-process-skill.md` §Step 0.5. Failure → scan.md DRAFT.

## Concept Abstraction Principle (Ch1 Docs)
**Core principle**: Concept chapters (Ch1) must be organized from architecture perspective (CPU questions/system mechanisms), NOT from code perspective (function/struct/trait names).

| Dimension | ❌ Code perspective | ✅ Architecture perspective |
|-----------|---------------------|----------------------------|
| Organization | trait name / function name / struct name | CPU questions / system mechanisms |
| Ch1 subject | function name | CPU / OS / system |
| Driving direction | HOW-only (jumps to implementation) | WHY→WHAT→HOW (concept first) |
| Multi-arch | arch-specific first, no unified abstraction | unified abstraction first, then arch-specific |

Violation → P1 (pattern 51: 实现驱动概念章).

## Causal Chain Validation (P0)
Every causal explanation ("因为 X 所以 Y") must have:
- X verified as a **real mechanism** (grep C/Rust source for evidence)
- X→Y technically correct (not just plausible-sounding)

Causal chain fabrication (X is wrong or X→Y is technically wrong) → P0 (pattern 48).

## Review Workflow
1. Always start by declaring scope: target file, mode (doc/code/full), estimated time (optional), **STATE.md status** (see dual-path rule below)
2. **Read correct STATE.md path**: Trae IDE → `.review/trae/{module}/STATE.md`; Claude Code Runtime → `.review/claude/{module}/STATE.md`. These two paths are **isolated** — never share STATE/scan/SYMBOLS/structure/VERIFY-CHECK between tools. If the **same tool** has conflicting STATE.md copies, log divergence in scan.md and ask user which is authoritative.
3. Execute checks ONE AT A TIME — never batch them mentally
4. Output a progress checklist showing each check as done/undone
5. Collect all findings into a review report at the end
6. **After all checks**: write STATE.md and convergence assessment
7. **Verify Blocker Gates 0/A/B/C/D/D-6/E/G all passed WITH EVIDENCE** before marking scan.md as Final. Gate A/D/E evidence must be L1 (tool/grep output); Gate B/C may be L1 or L2; L3 inference counts as FAIL unless `MANUAL_FALLBACK` is justified.
