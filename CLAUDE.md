# Minix-RS

Minix3 kernel modules rewritten in Rust (x86-64, no_std). Not a translation — a semantic rewrite preserving external behavior while using Rust's type system internally.

## Build & Test
- Build: `cargo build`
- Test: `cargo test`
- Lint: `cargo clippy`

## Directory Layout
```
minix3/              — original Minix3 C source (ground truth, do NOT modify)
os/servers/vm/       — VM server Rust rewrite
os/libs/minix-types/ — shared IPC types, constants, codec traits
notes/rewrite/       — documentation (one .md per C source concept)
prompt/              — review rules, skill definitions (source of truth for .claude/)
.claude/             — Claude Code runtime: rules + skills (derived from prompt/)
```

## Execution Model (by module type)
- **User-space servers (VM/PM/VFS etc.)**: Single-threaded event loop — `!Send`/`!Sync`/`AssumeSyncCell`/`Rc`/`RefCell` correct
- **Kernel**: SMP + BKL (Big Kernel Lock spinlock) — multi-CPU concurrency possible. Shared data needs `Arc`+`Mutex`/`Atomic`, not `Rc`/`RefCell`. BKL is spinlock: no sleep/schedule/IPC inside critical section.

## Coding Constraints
- `#![no_std]` everywhere except `#[cfg(test)]`
- Error types must map to Minix3 errno values — no self-invented error codes
- Hardware is abstracted behind traits — never expose CR3/PTE bits to OS layer
- **Concept abstraction (Ch1 docs)**: Concept chapters organized from architecture perspective (CPU questions/system mechanisms), NOT from code perspective (function/struct/trait names). Ch1 subject = CPU/OS, not function name. Multi-arch docs give unified abstraction first.

## Ground Truth Priority
```
Minix3 C source > Rust implementation > documentation > AI analysis
```
When in doubt, grep `minix3/` and read the original C code.

## Documentation Structure
Each doc in `notes/rewrite/` follows:
- Ch1: Concepts & Minix3 context (concept-driven, WHY→WHAT→HOW; multi-arch unified abstraction first)
- Ch2: Full C source analysis (functions, structs, macros)
- Ch3: Rust design decisions (WHY, with alternatives & rationale)
- Ch4: Rust implementation details (HOW)
- ChN-1: Test points
- ChN: See-also references

**Ch1&2 must cover ALL C symbols in the semantic scope. Ch3&4 must implement ALL semantics described in Ch1&2.**

## Review System

The review system enforces structured review via 9 skills (in `prompt/skill/`, synced to `.claude/skills/` and `.trae/skills/`). Full process details: `prompt/skill/review-process-skill.md`. The 9th skill `review-implementation-skill` (added 2026-06-22 from the 06-design-final.md implementation) verifies design ↔ code consistency, tracks §X self-review issues, and enforces backward-compatible refactor + test coverage boundary.

### ⛔ Explicit Skill Invocation
You MUST invoke Skill tools explicitly via the available `Skill` function. NEVER rely on "rules already loaded" or "context already has it". The Skill Invocation Log in scan.md must reflect actual Skill tool calls, not planned/intended calls.

### Blocker Gates (must pass before Final Review)
- **Gate 0**: Artifact inventory — standard paths complete (STATE/scan/structure/SYMBOLS); scan.md contains 8 grep-verifiable anchor sections (Skill Invocation Log / Blocker Gates Status / Step 1 / 1.5 / 2 / 3.5 / Issue List / Artifact Inventory)
- **Gate A**: Coverage enumeration — `coverage-extract.py` executed + SYMBOLS.md path in scan.md + `gate-evidence-A` block with command + stdout
- **Gate B**: Diff extraction — Top 5 behavior contract table (3 语义偏移 + 2 覆盖缺口, **8 fields × 5 funcs**)
- **Gate C**: Precision check — 5 meta-rules check table output
- **Gate D**: P0 mandatory checklist — 5 items answered (✅/❌ + grep evidence); **PARTIAL/⚠️/"部分通过" = ❌ FAIL**
- **Gate D-6**: structure.md skeleton review (doc review only) — structure.md generated + 12-section review table + failures in Issue List
- **Gate E**: Test verification — §5 each test function grep-verified (if doc has §5)
- **Gate G**: VERIFY-CHECK independent validation — VERIFY-CHECK.md produced + verdict PASS (consistency ≥ 90%); CONCERN/FAIL may NOT mark CONVERGED

Any Gate failed → scan.md marked DRAFT, STATE.md not updated. **"✅ Gate passed" without attached command+output evidence is invalid.** Gate evidence strength: L1 (tool output, required for A/D/E), L2 (manual grep, acceptable for B/C), L3 (inference, treated as FAIL unless `MANUAL_FALLBACK` justified).

### structure.md (Doc Review Mandatory, Step 0.5)
Before correctness checks, generate `structure.md` (12-section skeleton analysis) to verify "what the reader reads" (orthogonal to correctness which verifies "what the doc says"). Sections: 主题思想/目标读者/叙事主语/驱动方向/文档大纲/核心概念清单/跨架构统一抽象/双向闭环/叙事弧/元注释/裸概念复述/纵向链路映射. Template: `prompt/skill/review-process-skill.md` §Step 0.5.

### P0 Mandatory Checklist (Gate D, see `prompt/skill/review-patterns-skill.md` §0)
1. §5 tests exist: `rg "fn {test_name}" {rust_dir}` — missing → P0
2. trait has ≥1 impl: `rg "impl.*{TraitName}" {rust_dir}` — 0 impl → P0
3. function in declared file: `rg "fn {name}" {file}` — not found → P0
4. core algorithm not stub: `rg "todo!|unimplemented!|unreachable!|panic!" {rust_dir}` — stub → P0; `panic!` in non-test code that represents unimplemented functionality or a reachable unhandled path → treat as stub/unhandled path, must be justified in comment
5. §4 signatures match: compare doc §4 vs actual — mismatch → P0

### Key Patterns (48-57, narrative & concept)
- **48 因果链编造 (P0)**: claim correct but causal explanation technically wrong
- **49 元注释泄漏 (P1)**: author narrates writing strategy in body text (>5 → P1)
- **50 架构范围未标注 (P1)**: x86-specific mechanism told as common
- **51 实现驱动概念章 (P1)**: Ch1 subject is function name, not CPU/OS
- **52 单向心智模型 (P1)**: entry mechanism only covers entry, not return
- **53 跨架构共性未提取 (P1)**: multi-arch doc has no unified abstraction
- **54 视角漂移 (P2)**: subject switches within same chapter
- **55 架构特有机制喧宾夺主 (P2)**: arch-specific legacy overshadows core
- **56 决策日志体 Ch3 (P1)**: Ch3 lists decisions without rationale
- **57 例子前置知识泄漏 (P2)**: example introduces unrelated details

### Rule Evolution (Rule Discovery, Step 5.7)
Every scan.md MUST include a `§Rule Discovery` section answering: "Did this review discover a new pattern? ✅/❌". If ✅, propose a new pattern (name/case/severity/category/draft rule/target file). This makes the rule set self-evolving — patterns discovered in one review feed back into the rules for the next review. See `prompt/skill/review-process-skill.md` §Step 5.7.

### Convergence Cost Warning (Step 7.1)
To prevent over-convergence (chasing P1→0 across many rounds), stop and deliver when ANY of these trigger:
1. Same doc reviewed ≥5 rounds → force deliver, remaining P1/P2 → backlog
2. Two consecutive rounds with new P1 ≤ 1 → converged, remaining P1 → backlog
3. Current round cost >80% of previous but new findings <20% → stop

See `prompt/skill/review-process-skill.md` §Step 7.1.

### Skill Invocation Log (mandatory in scan.md)
Every scan.md MUST include a Skill Invocation Log table (Skill name, 调用时机, 关键产出). Missing → scan.md DRAFT.

### State Management: Dual-Path (Trae vs Claude)
- **Trae IDE** → `.review/trae/{module}/STATE.md` (project root `.review/`, not inside `notes/`)
  - Per-doc/cross-AI scans: `.review/trae/{module}/scans/{doc-stem}-{agent}-scan.md`
  - Bagging aggregate: `.review/trae/{module}/scans/AGGREGATED-{doc-stem}.md`
- **Claude Code Runtime** → `.review/claude/{module}/STATE.md` (project root `.review/`)
  - Module-level scan: `.review/claude/{module}/{doc-stem}/scan.md`
  - Verification: `.review/claude/{module}/VERIFY-CHECK.md`
- **No shared intermediate results** between tools: STATE/scan/SYMBOLS/structure/VERIFY-CHECK are isolated. Bagging aggregation happens only inside Trae (multi-AI scan merge). Cross-tool divergence must NOT be auto-merged; log it in scan.md and ask the user.
- `{module}` = first directory under `notes/rewrite/` (e.g. `fork-syscall-rewrite`). `{doc-stem}` = target doc basename without extension (e.g. `03-kmain-cstart`). `{agent}` = model id (Trae: glm/kimi/...; Claude: m3/...).
- Recommended: `tools/review-init.sh claude {doc-path}` auto-computes `{module}`/`{doc-stem}` and creates standard directories.

### Intermediate Artifacts
- `STATE.md` — review progress (tool-specific path, see above)
- `SYMBOLS.md` — coverage enumeration (machine-generated + AI judgment)
  - Trae doc-level: `.review/trae/{module}/scans/{doc-stem}-{agent}-SYMBOLS.md`
  - Claude doc-level: `.review/claude/{module}/{doc-stem}/SYMBOLS.md`
- `structure.md` — skeleton analysis (doc review, 12 sections, saved alongside scan.md)
- `scan.md` — single-file aggregation of all dimension results (NOT 10 separate check files)
  - Trae: `.review/trae/{module}/scans/{doc-stem}-{agent}-scan.md`
  - Claude: `.review/claude/{module}/{doc-stem}/scan.md`
  - If user explicitly requests another output location, **dual-write** to user-specified path + tool default path (Trae interactive fix doc: `notes/rewrite/{module}/{stage}/{doc-stem}-trae-review.md`; Claude report: `{doc-stem}-claude-report.md`).
- `VERIFY-CHECK.md` — independent validation result (mandatory before CONVERGED)
  - Trae: `.review/trae/{module}/VERIFY-CHECK.md`
  - Claude: `.review/claude/{module}/VERIFY-CHECK.md`
