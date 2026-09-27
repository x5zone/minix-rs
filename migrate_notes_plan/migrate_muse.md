# Notes Migration Plan (muse)

## 0. Status and Scope

- **Author:** muse (English document per author requirement).
- **Goal:** Eliminate the deep, semantically-deprecated `notes/rewrite/fork-syscall-rewrite/...`
  hierarchy and re-home every notes subtree to a role-based top-level layout, without
  breaking the `tools/` chain or silently invalidating the `.review/` history.
- **Non-goal:** No content rewrite during the move. Content cleanup (study-notes
  pruning, misc triage follow-ups) happens after the move converges, not inside it.
- **Scale (measured 2026-09-22):**
  - `notes/rewrite/fork-syscall-rewrite/`: 21 stage dirs (`00-master-plan` … `20-redesign`),
    heaviest stages are `01-stage-kernel` (51 entries), `05-stage-vfs` (42),
    `02-stage-vm` (38), `17-stage-net` (35), `16-stage-drivers` (34).
  - ~20 loose files at the `fork-syscall-rewrite/` root (`HANDOFF-*`, `NK4A-*`,
    `NK4B-*`, `edge*`, `new_edge*`, `new_todo_*`, `claim-prompt.md`, `evidence/`).
  - 14 top-level `notes/rewrite/*.md` misc docs.
  - `notes/study/`: 187 files.
  - `notes/redesign/`: 12 files (including 2 `.backup` files).
  - `.review/`: 1725 files across `trae/`, `claude/`, `codex/` plus legacy top-level dirs.

## 1. Current-State Inventory (What Exactly Moves)

### 1.1 Source tree roles

| # | Path | Role | Disposition |
|---|------|------|-------------|
| S1 | `notes/rewrite/fork-syscall-rewrite/{01..19}-stage-*` | Active rewrite work area, boot-order stages | Move to new rewrite home, one level shallower (§3) |
| S2 | `notes/rewrite/fork-syscall-rewrite/00-master-plan/` (16 files) | Project overview / roadmap / decision records | Move as `00-master-plan/` or fold into new `00-meta/`; decision records stay verbatim |
| S3 | `notes/rewrite/fork-syscall-rewrite/20-redesign/` (2 files) | Redesign content hiding inside rewrite | Move to redesign home, not rewrite home |
| S4 | Loose root files (`HANDOFF-*`, `NK4A-*`, `NK4B-*`, `edge*`, `new_todo_*`, `evidence/`) | Session handoffs, worklogs, edge-case scratch | Move as a unit to `99-handoff-archive/` inside rewrite home; read-only after move |
| S5 | `notes/rewrite/*.md` (14 files: `README`, `RECONSTRUCTION-PRINCIPLES`, `arch_mapping`, `elf-loader`, `invariant`, `ipc-sendrec`, `minimal-skeleton`, `misc`, `modern-hardware-and-rust`, `project-plan`, `project-structure`, `rewrite-strategy`, `rewrite`, `vertical-slice-strategy`) | Misc / strategy / principle docs | Triage per §5 into rewrite-home `00-meta/` vs redesign-home; none deleted in this migration |
| S6 | `notes/rewrite/concepts/` (6 files) | Cross-cutting concepts (capability, endpoint, fail-stop, typestate, …) | Move to rewrite-home `00-meta/concepts/` (they are rewrite ground rules, not stage content) |
| S7 | `notes/rewrite/archive_bak/` (untracked, ~40 `fork-rewr*` backups, `.bak*`) | Obsolete backups | Do NOT migrate. Keep out of git (already untracked). Delete only by explicit separate decision |
| S8 | `notes/study/` (187 files, AI-generated Minix3 learning notes) | Early learning scratch | Quarantine first (§6); prune/summarize only after migration converges |
| S9 | `notes/redesign/` (12 files) | OS-direction exploration | Move as-is to redesign home; normalize filenames then, not now |
| S10 | `book/` | Future ebook output dir, currently build artifacts present | **Out of scope. Do not touch.** Migration must not alter `book/` or `book.toml` wiring |
| S11 | `.review/` (1725 files) | Historical review products, per-tool isolated (`trae/`, `claude/`, `codex/`) | **Freeze, do not rewrite** (§7) |

### 1.2 Why `fork-syscall-rewrite` is semantically dead (record the rationale)

The directory name encodes an abandoned organizing principle: fork-syscall as the main
narrative thread. Practice showed boot order must be the primary thread (too many
undefined concepts otherwise) with fork as at most a secondary thread. Keeping the name
means every new contributor must learn a history lesson to find `16-smp.md`. The rename
is therefore a semantic correction, not cosmetics, and the old name must not survive as
a symlink farm (symlinks would preserve the depth problem and double all path grep hits).

## 2. Pre-Migration Tag: Yes, Mandatory

**Decision: create an annotated tag before any move. This is not optional.**

- Tag name: `notes-layout-v1-freeze` (annotated, with date and HEAD hash in message).
- Why:
  1. Gives every parallel agent a common, unambiguous base commit to diff against.
  2. Makes rollback a one-command operation (`git mv` reversal or `git reset`-free
     re-application from tag) instead of forensic reconstruction.
  3. Preserves a citable boundary for the 1725 `.review/` files whose embedded paths
     will otherwise look "wrong" post-move.
- Procedure:
  1. `git status` clean on tracked files (stash or commit unrelated work first;
     note: repo currently has modified `AI-chats/daily.todo.md`, `tmp/nk4a/vars.fd`
     and untracked `notes/study/`, `notes/rewrite/archive_bak/` — decide before tagging).
  2. `git tag -a notes-layout-v1-freeze -m "Pre notes-migration freeze <date> <HEAD>"`.
  3. `git push origin notes-layout-v1-freeze` (so parallel agents on other machines
     share the boundary).
  4. Record tag + HEAD in the migration tracking issue / `migrate_notes_plan/` index.
- No migration PR merges without the tag existing. The tag check is Gate 0 (§8).

## 3. Target Layout Decision

### 3.1 Proposed target (from request) and assessment

Proposed:

```text
rewrite-notes/01-stage-kernel/16-smp.md
redesign-notes/vm_in_kernel/xx.md
study-notes/minix3_concept/xx.md
```

Assessment: the direction is right — **role at top level, one nesting level removed**.
`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/16-smp.md` (4 levels) becomes
`rewrite-notes/01-stage-kernel/16-smp.md` (2 levels). That directly fixes pain point #1.

### 3.2 Recommended refinement

Keep the proposal with three adjustments:

```text
rewrite-notes/
  00-meta/            # ex 00-master-plan + S5 misc triaged here + concepts/
    concepts/
  01-stage-kernel/
  02-stage-vm/
  ...
  19-stage-integration/
  99-handoff-archive/ # ex S4 loose files, read-only
  misc/               # landing zone for S5 items whose home is genuinely unclear
redesign-notes/
  from-notes-redesign/  # verbatim move of notes/redesign/, filenames normalized later
  from-rewrite-20/      # ex 20-redesign/, keeps provenance visible
  vm_in_kernel/         # new exploration branches land here (empty scaffold + README)
study-notes/
  minix3_concept/     # verbatim quarantine of notes/study/, prune later
```

Adjustments and reasons:

1. **`00-meta/` instead of reusing `00-master-plan/` verbatim.** The master-plan dir
   plus 14 misc docs plus `concepts/` are all "about the rewrite" rather than a boot
   stage. One `00-meta/` roof with `concepts/` underneath stops the next misc sprawl.
2. **Provenance prefixes under `redesign-notes/`** (`from-notes-redesign/`,
   `from-rewrite-20/`). Two sources merge into one home; without prefixes a later
   reader cannot tell which `ipc-improve.md`-style doc came from where. Remove prefixes
   in a later normalization pass once cross-links are stable.
3. **`99-handoff-archive/` + `misc/` as explicit, quarantined zones**, each with a
   `README.md` stating "no new content here; new content goes to stages / exploration
   branches". Without that README both become the next junk drawer.

### 3.3 Alternative considered and rejected

- **Stay under `notes/` (`notes/rewrite/01-stage-kernel/...`).** Shallower by one level
  too, and fewer tool edits. Rejected as the primary option because `notes/` then keeps
  three different roles under one roof and the next contributor re-invents the same
  confusion; also `notes/study` + `notes/redesign` names would still collide
  conceptually with the new top-level intent. If the team strongly prefers minimal
  diff, this is the fallback — but then `notes/rewrite/` must still drop the
  `fork-syscall-rewrite` level and gain the `00-meta/` / `99-handoff-archive/` zones.
- **Per-service split now (`kernel-notes/`, `vm-notes/`).** Rejected: the current
  organization is boot-order stages, and a service-axis rename is a content
  reorganization, not a move. Do it after convergence, never inside the move.

## 4. Risk Register

### 4.1 Toolchain (`tools/`) — the highest risk

Hardcoded `notes/rewrite` references found (2026-09-22 grep):

| Tool | Coupling | Migration action |
|------|----------|------------------|
| `tools/design-coverage-check.sh` | `MODULE_DIR="notes/rewrite/${MODULE}"`; usage examples cite `fork-syscall-rewrite` | Parameterize base dir (`NOTES_BASE` env var, default new home); update usage text; add regression test invoking both old-tag path (expect clean error) and new path |
| `tools/review-init.sh` | Derives `{module}/{stage}/{doc-stem}` from doc-path; per-tool `.review/` layouts documented in header | Update path-derivation + header docs; keep old-path input producing an explicit "moved, see redirect index" error rather than silently creating stale dirs |
| `tools/design-index-update.sh` | Example path cites `notes/rewrite/fork-syscall-rewrite/03-stage-kernel` | Update example + any embedded default |
| `tools/doc-style-lint.sh`, `tools/review-gate-check.sh`, `tools/check-review-rules.sh` | Grep hits for `notes/rewrite` | Audit each hit; most are path prefixes needing the same `NOTES_BASE` treatment |
| `tools/coverage-extract/coverage-extract.py` | Hit via compiled cache; source must be checked | Same treatment; re-run to confirm output paths |
| `tools/anchor-*-baseline.txt` | Hundreds of lines embedding `notes/rewrite/fork-syscall-rewrite/...` with line numbers | **Regenerate, do not hand-edit.** Baselines are tool output; post-move run `anchor-migrate.sh`/`anchor-resolve.sh` flow and commit fresh baselines with a note linking the freeze tag |
| `tools/anchor-migrate.sh`, `tools/anchor-resolve.sh` | May assume old layout | Test against new layout before bulk anchor updates |

General rule: **centralize the base path in one variable** (`NOTES_BASE` or equivalent)
so the next rename touches one line. Every script that today writes
`notes/rewrite/...` literally is a repeat incident waiting to happen.

### 4.2 Rule / skill docs referencing old paths

`prompt/`, `.claude/`, `.codex/`, `.trae/`, `.agents/` all contain `notes/rewrite` and
`fork-syscall-rewrite` references (review-process, review-patterns, review-doc,
review-coverage skills; `prompt/README.md`, `prompt/todo_plan.md`; `.claude/settings.local.json`).
Per `prompt/README.md` these trees have a source→derived sync flow — so:

1. Edit the **source** (`prompt/...`) first, regenerate derived files via
   `tools/generate-derived-skills.sh`, then run `tools/check-review-rules.sh`
   (and `lint-review-rules.sh`) as the gate.
2. Count check: post-migration `grep -rl "fork-syscall-rewrite" prompt/ .claude/
   .codex/ .trae/ .agents/ tools/` must return only intentional historical mentions
   (changelog-style), zero functional references. Record the residual list in the
   migration report.

### 4.3 Historical review products (`.review/`, 1725 files)

- **Do not move or rewrite `.review/` content.** Internal scan reports quote old paths
  as evidence; rewriting them falsifies history.
- Instead: freeze (no new scans into old module dirs after cutover), publish a
  `REDIRECT.md` mapping `notes/rewrite/fork-syscall-rewrite/<stage>/<doc>` →
  `rewrite-notes/<stage>/<doc>`, and point `review-init.sh`'s error message at it.
- `STATE.md` files per tool (`trae`/`claude`/`codex` isolated — do not merge across
  tools) get an appended "layout migration" entry, not an edit of past entries.
- Legacy top-level `.review/` dirs (`01-multiboot-bootstrap`, `03-kmain-cstart`,
  `kernel`, `vm`, …) are already archival; leave them.

### 4.4 Link rot inside markdown

Cross-links between stage docs, `00-master-plan` decision records, and `.review/`
evidence blocks use relative and absolute `notes/...` paths. Bulk `sed` is forbidden
as the primary method (it corrupts code fences and anchor baselines). Instead:

1. Generate a full old→new path map (Phase 0 manifest, machine-readable).
2. Apply link updates with `tools/anchor-migrate.sh`-style tooling or a reviewed
   script limited to markdown link targets, then run `check_references.sh` (repo root)
   and the anchor baselines regeneration.
3. Residual `fork-syscall-rewrite` string hits after the pass must be triaged one by
   one (history quote vs live link).

### 4.5 Untracked / dirty state

`notes/study/` and `notes/rewrite/archive_bak/` are currently **untracked**; tag does
not protect untracked files. Before tagging, either `git add -N` + commit them or
explicitly record their checksums in the manifest. `archive_bak/` should stay
untracked and out of the migration (S7), but that decision must be written down, not
assumed.

## 5. Misc-Docs Triage (the 14 `notes/rewrite/*.md`)

Triage owner fills the last column before Phase 3 starts. Default proposal:

| Doc | Proposed home | Reason |
|-----|---------------|--------|
| `README.md` | `rewrite-notes/README.md` (rewritten to describe new layout) | Entry point follows the stages |
| `RECONSTRUCTION-PRINCIPLES.md` | `rewrite-notes/00-meta/` | Ground rule of the rewrite |
| `arch_mapping.md` | `rewrite-notes/00-meta/` | Architecture map, cross-stage |
| `project-plan.md`, `project-structure.md`, `rewrite-strategy.md`, `rewrite.md`, `vertical-slice-strategy.md`, `minimal-skeleton.md` | `rewrite-notes/00-meta/` (plan/strategy cluster) | Meta, not stage content |
| `invariant.md` | `rewrite-notes/00-meta/` | Cross-stage invariant |
| `ipc-sendrec.md`, `elf-loader.md`, `modern-hardware-and-rust.md` | `rewrite-notes/misc/` first, then owner assigns to a stage or keeps as cross-cutting | Technical topics with unclear stage ownership; do not force-fit during the move |
| `misc.md` | Dissolve: distribute its sections per above, then delete (only misc item allowed to disappear, with section→destination table in commit message) | It is an index of homeless content; the move must not replicate homelessness |

Rule: **no misc doc is deleted except `misc.md` itself**, and that only with a
section-level forwarding table.

## 6. Study-Notes Handling (187 files)

1. **Quarantine, don't prune, during migration:** verbatim move
   `notes/study/` → `study-notes/minix3_concept/`, add `study-notes/README.md`
   ("AI-generated learning scratch, unverified against C source; do not cite as
   ground truth").
2. **After convergence**, a separate pass (different plan, different review) decides
   per-file delete vs compress-into-summary. Deletion criteria to pre-agree now so the
   later pass is mechanical: (a) superseded by a `rewrite-notes` stage doc, (b) no
   `minix3/` C-source anchor, (c) duplicate of another study note.
3. Never mix study content into `rewrite-notes/` stages without a full review cycle —
   study notes are explicitly unverified and would poison stage-doc evidence chains.

## 7. Redesign-Notes Handling

1. Verbatim move `notes/redesign/` → `redesign-notes/from-notes-redesign/`.
2. Move `20-redesign/` → `redesign-notes/from-rewrite-20/`.
3. Scaffold `redesign-notes/vm_in_kernel/README.md` stub describing the open question
   (VM in kernel address space vs IPC optimization alternatives) — stub only, no new
   technical content in this migration.
4. Keep the two `.backup` files out: move them to the archive decision in S7
   (they are byte-identical duplicates per size match 74982/74982 and 82726/82726 —
   verify by checksum, then exclude).

## 8. Execution Phases and Gates

Work is organized so **multiple AIs can execute in parallel without stepping on each
other** (§9). Phase order is strict; within Phase 4, stages parallelize.

| Phase | Work | Owner count | Gate |
|-------|------|-------------|------|
| P0 Freeze & manifest | Clean tree decision (§4.5); create `notes-layout-v1-freeze` tag; generate machine-readable old→new path manifest (every file, checksum); publish `REDIRECT.md` skeleton | 1 agent | **Gate 0:** tag exists on origin; manifest covers 100% of S1–S6+S8+S9 files; `git status` clean on tracked files |
| P1 Toolchain decoupling | Introduce `NOTES_BASE`; update §4.1 scripts; update usage strings; add old-path error + redirect hint; regenerate derived skills; run `check-review-rules.sh` + `lint-review-rules.sh` | 1 agent | **Gate 1:** all §4.1 tools pass against a scratch copy of the new layout; zero functional `fork-syscall-rewrite` refs outside history quotes |
| P2 Study + redesign pre-move | Verbatim moves S8, S9 + `20-redesign`; checksums match manifest; add READMEs | 1 agent, parallel with P1 | **Gate 2:** file counts match (187 study; 12−2 backups redesign); `git status` shows renames only |
| P3 Misc triage | Fill §5 table; execute; dissolve `misc.md` with forwarding table | 1 agent | **Gate 3:** zero `*.md` left at old top level; forwarding table committed |
| P4 Stage migration | Move stages `00`–`19` in batches (§9); `concepts/` → `00-meta/concepts/`; loose root files → `99-handoff-archive/` untouched | N agents, disjoint batches | **Gate 4 (per batch):** `git diff --stat` shows pure renames (similarity ~100%); per-batch link-check slice passes; batch manifest checksums verify |
| P5 Link & anchor repair | Path-map-driven link update; `check_references.sh`; regenerate anchor baselines; re-run `design-coverage-check.sh` per stage | 1 agent | **Gate 5:** `check_references.sh` clean; coverage check exit 0 on new layout; residual old-string hits triaged to zero-live-links |
| P6 Rule/skill/doc sync | Update `prompt/` sources → regenerate → `check-review-rules.sh`; update `AGENTS.md` / `CLAUDE.md` directory-layout sections; `.review/*/STATE.md` migration entries; `REDIRECT.md` finalized | 1 agent | **Gate 6:** `tools/check-review-rules.sh` passes; grep residual audit recorded |
| P7 Acceptance | Full `cargo build` + `cargo test` + `cargo clippy` (workspace root `os/`) to prove no code-side path dependency broke; final manifest-vs-tree reconciliation | 1 agent (or rotating verifier) | **Gate 7:** build/test/clippy green; manifest reconciliation 100%; migration report committed to `migrate_notes_plan/` |

Rollback: any gate failure → stop, fix forward if the batch is small, otherwise
`git mv` back using the P0 manifest (which is reversible by construction) or reset the
migration branch to the freeze tag. **Never mix new content commits into the migration
branch** — migration commits must be pure moves plus mechanical link updates, so
revert stays trivial.

## 9. Parallel-AI Execution Protocol (Anti-Omission, Anti-Collision)

The requester runs several AIs concurrently and explicitly wants independent,
cross-checkable results.

1. **Single manifest, disjoint partitions.** P0 publishes the manifest with one row
   per file: `old_path, new_path, sha256, batch_id`. Each agent owns whole batch(es);
   no two agents touch the same directory. Suggested batches: `{00-meta+concepts}`,
   `{01-kernel}`, `{02-vm}`, `{03-rs,04-pm}`, `{05-vfs}`, `{06-sched,07-ds,08-is}`,
   `{09-init,10-mib,11-devman}`, `{12-input,13-ipc,14-runtime}`, `{15-fs}`,
   `{16-drivers}`, `{17-net}`, `{18-commands,19-integration,20→redesign}`,
   `{misc+study+redesign}`.
2. **Movers don't verify their own batch.** Each batch is verified by a *different*
   agent: re-run checksums, confirm `git diff` shows renames not delete+add, spot-run
   the stage's `design-coverage-check.sh` slice.
3. **No cross-reading of other agents' plans** (per requester instruction) — but all
   agents read the same P0 manifest and gates, so divergence surfaces as gate
   failures, not silent skew.
4. **Communication contract:** agents report per-batch `DONE (batch, files, sha-ok,
   gate-evidence)` or `BLOCKED (batch, reason, path)` lines; the integrator merges
   only `DONE` batches with attached gate evidence.
5. **Branch discipline:** one `migrate/notes-layout` branch; each agent works on a
   `migrate/notes-layout/<batch>` fork and opens a PR into it; the integrator
   fast-forwards batch by batch. Direct pushes to the migration branch are forbidden.

## 10. Open Questions for the Requester

1. Confirm top-level (`rewrite-notes/` at repo root) vs `notes/`-internal fallback (§3.3)?
2. Confirm `notes-layout-v1-freeze` tag name and whether to push it to origin?
3. Confirm `archive_bak/` stays untracked and out of scope (S7)?
4. Confirm study-notes prune criteria (§6) so the later pass needs no new decisions?
5. Who owns the §5 misc table final column, and who is the P7 rotating verifier?

## 11. Acceptance Checklist

- [ ] Freeze tag on origin; manifest 100% coverage.
- [ ] New trees exist with `README.md` per top-level dir + `00-meta/` + `misc/` +
      `99-handoff-archive/` discipline notes.
- [ ] Old `notes/rewrite/fork-syscall-rewrite/` level gone; no symlinks left behind.
- [ ] All gates 0–7 evidenced in the migration report.
- [ ] `cargo build` / `cargo test` / `cargo clippy` green.
- [ ] `.review/` untouched except appended STATE entries + `REDIRECT.md`.
- [ ] `book/` untouched.
- [ ] Residual `fork-syscall-rewrite` grep audit committed (history quotes only).
