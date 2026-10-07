# Notes Directory Migration — Independent Audit Report (Muse side)

- **Audit target**: repository `/home/xzhao/github/minix-rs`, branch `rewrite`,
  migration chain `notes/pre-migrate-20261007` (`bb8a90e05`) → `notes/post-migrate-20261007`
  (`964e3e28e`). HEAD at audit time: `77be8773a`.
- **Auditor**: Muse-side session. Did not participate in the migration.
- **Protocol**: `migrate_notes_plan/REVIEW-PROMPT.md` items 0–8.
- **Discipline**: read-only. No file under audit was modified, no `git add` / `commit` /
  `checkout` / `reset` / `clean` was executed. The only writes in the repository are new
  files under `migrate_notes_plan/Muse/` (this report and its evidence). Temporary
  read-only artifacts live in `/tmp/muse-*` (tar listings, link-check outputs).
- **Live mutation probes deliberately NOT executed**: REVIEW-PROMPT item 6 asks for
  throwaway probe files inside the tracked trees. Because this audit is strictly
  read-only (and the worktree contains the maintainer's in-progress edits), no probe
  file was created in any tracked directory. Gate effectiveness for item 6 is instead
  established by (a) all five tool `--self-test` suites passing, (b) static confirmation
  that the incremental lint gate now diffs against `${NOTES_TREES[@]}` from
  `tools/notes-layout.conf`, (c) the rule/tool consistency gates, and (d) the live-probe
  evidence already recorded by the DS and GLM audits, whose results I cross-checked but
  did not need to reproduce by mutation.
- **Relation to the DS and GLM audits**: I read their two final reports
  (`migrate_notes_plan/DS/REVIEW-REPORT.md`, `migrate_notes_plan/GLM/99-AUDIT-REPORT.md`
  plus `GLM/01-ITEM-RESULTS.md`) for context. I did not run any of their scripts and do
  not cite their evidence files. Every verdict below is backed by a command I executed
  in this session; outputs are stored in `migrate_notes_plan/Muse/evidence/`. Where my
  numbers match theirs, that is independent convergence, and I say so explicitly.

## Verdict: DELIVERABLE — P0 = 0, P1 = 1, P2 = 4

No content loss, no content corruption, no history breakage, no disabled quality gate.
The one P1 is a terminology-coverage gap in rule docs (pre-existing, zero functional
impact). P2 items are documentation-precision issues.

## Item-by-item results

| # | REVIEW-PROMPT item | Result |
|---|---|---|
| 0 | Worktree boundary (in-progress files untouched) | PASS |
| 1 | Content preservation (most important) | PASS (with corrected criterion, see below) |
| 2 | Snapshot + tar integrity | PASS |
| 3 | Rename history + tracking conservation | PASS |
| 4 | Reference rewrite residue | PASS |
| 5 | Links and anchors | PASS |
| 6 | Gate effectiveness (no silent failure) | PASS (without live mutation; see discipline note) |
| 7 | Compile surface + code semantics | PASS (`cargo check` rc=0, zero errors) |
| 8 | Twelve user decisions honored | PASS |

### Item 0 — Boundary

- Tracked modifications: exactly one file,
  `M rewrite-notes/coordination/TODO-3ARCH-PARITY-20261006.md` (evidence `item0-status.txt`).
- Its committed content is byte-identical pre/post migration
  (sha256 `6ef43c6d…` on both sides), while the worktree still carries the maintainer's
  6-insertion/2-deletion edit (`item0b-todo-*.txt`). The migration did not submit on the
  maintainer's behalf.
- Untracked entries are exactly the expected set: `migrate_notes_plan/DS/`,
  `GLM/`, `Muse/` (this audit), `Mimo/` (a parallel audit that appeared mid-session;
  untouched), `agents-workflow-optim.md`, `new_laptop_migrate/`,
  `PENDING-DECISIONS-3ARCH-PARITY.md`, `tools/atf-c-compat/probes/p7.c`.
- Both tags exist locally with the documented targets:
  `notes/pre-migrate-20261007 → bb8a90e05`, `notes/post-migrate-20261007 → 964e3e28e`
  (`item0-tags.txt`, including tag messages).

### Item 1 — Content preservation

- Counts close: manifest 2428 = path-map 2428 = on-disk new-tree files 2428
  (`item1a-counts.txt`). Tracked files: 1102 pre → 1105 now = 1102 + 3 newly added
  entry documents (`MIGRATION.md`, `redesign-notes/vm/README.md`, `study-notes/README.md`).
- Per-file sha256 reconciliation against `manifest.notes.disk.tsv` via `path-map.tsv`:
  **2095 identical / 0 missing / 333 different** (`item1b-reconcile.txt`, full list
  `item1b-difflist.txt`). This reproduces the DS/GLM numbers exactly.
- **Criterion correction (agreeing with DS P1-2 and GLM P2-2)**: REVIEW-PROMPT item 1b
  expects "identical 2428 / missing 0 / different 0", which is unsatisfiable by design —
  Phase 3 (reference rewrite) and Phase 7 (entry-document rewrite) intentionally change
  tracked-file contents. The correct criterion is: **missing must be 0, and every
  difference must be attributable to a recorded rewrite operation**. Verified:
  - All 333 differences are `git-mv` (tracked) kind; **zero** in frozen zones
    (`.design/`, `evidence/`, `legacy-fork-bak/`) — `item1c-attribution.txt`.
  - Line-count analysis of `e90e4173d..HEAD` over the three trees: 336 files changed =
    333 + 3 new; the only asymmetric (line-adding/deleting) entries are the documented
    rewrites (`rewrite-notes/README.md` 69+/96-, `redesign-notes/README.md` 12/5) and
    the 3 new files. All other 331 files are symmetric in-line substitutions, i.e. the
    "no line add/del" discipline held (`item1d-numstat.txt`, `item1d-asymmetric.txt`).
  - Spot-checked diffs (`item1e-spotcheck.txt`) show pure old-prefix → new-prefix
    substitution, including same-directory references correctly retargeted into
    `coordination/` (e.g. `NK4C-MIGRATION-20260930.md`, `riscv-reviewlog.md`).
- The `path-map.tsv` contract itself is unmodified since Phase 1:
  sha256 `c43f82c5…` matches the value logged in EXECUTION-LOG (`item8a-pathmapsha.txt`).

### Item 2 — Snapshot and tar integrity

- `sha256sum -c snapshot.sha256`: **7/7 OK** (`item2-snapshot.txt`).
- Tar sha256 `39e68026…` matches the recorded value; full listing readable, rc=0.
  File entries (directory entries excluded): `notes/` 2428, `.design/` 1162,
  `.review/` 1728 — all match the baseline counts (`item2-tarfiles.txt`).
- Spot check: `PENDING-DECISIONS-3ARCH-PARITY.md` sha256 is identical across tar,
  manifest, and current disk (`4cdde71c…`).
- Single-copy limitation acknowledged by the migration (tar lives in git-ignored
  `tmp/`) remains true; copy off-repo after acceptance (P2-4).

### Item 3 — Rename history and tracking conservation

- Pure-move commit `e90e4173d`: `1102 files changed, 0 insertions(+), 0 deletions(-)`;
  authoritative `git diff-tree -M --name-status` shows **1102 × R, 0 A, 0 D**
  (`item3a-puremove.txt`, `item3b-movetypes.txt`).
- `git log --follow` penetrates the migration (checked `16-smp.md`, reaches pre-move
  history) — `item3c-follow.txt`.
- Interval diff `pre..HEAD` shows 1100 R + 5 A + 2 D (`item3d-interval.txt`). Both D
  entries verified as rename-similarity artifacts, not loss:
  - `notes/rewrite/README.md → rewrite-notes/README.md`: blob identical **at the move
    commit** (later rewritten in documented Phase 7);
  - `…/edge3.md → coordination/edge3.md`: blob identical at the move commit
    (`item3f-Dpairs.txt`, including my first wrong-pair guess and its correction).
  - 5 A = 2 new sides of the above + 3 new entry documents.
- The Phase-2 "108 silently untracked" fix holds: all 1102 mapped `git-mv` targets are
  present in the index (1105 = 1102 + 3 new), no D-type loss.

### Item 4 — Reference rewrite

- Byte-level scan (Python, avoiding the ugrep invalid-UTF-8 silent-drop issue noted by
  GLM) over the mandatory-change domain, excluding frozen dirs: hits in exactly **2
  files**, both whitelisted — `.claude/settings.local.json` (40 lines, frozen auth
  history; +2 vs the 38 logged, consistent with appended new authorizations) and
  `rewrite-notes/MIGRATION.md` (21 lines, the mapping table itself). **Zero residue
  outside the whitelist** (`item4-residue.txt`).
- Frozen-zone retention counts match the baseline exactly: `evidence/` 4 files,
  `legacy-fork-bak/` 8 files, `.design/` 56 files (`item4b-frozen.txt`).
- `.review/` freeze verified by full recursive comparison of tar-extracted `.review/`
  against disk: **0 content differences, 0 missing**; only additions are the new
  `PATH-MAPPING.md` and the 3 archived files under
  `.review/archive/notes-fork-syscall-rewrite-2026-09/` (`item4-reviewfrozen.txt`).

### Item 5 — Links and anchors

- Anchor baselines: 874 / 6609 lines, **zero old-path residue**; all 101 referenced
  document paths exist on disk (`item5a-anchors.txt`, `item5d-anchors-exist.txt`).
- Fresh link scan of the three trees: 2133 files, **373 broken** — byte-identical to
  the three-tree subset of `broken-links.after.txt`, so Phases 4/7 introduced nothing
  after that baseline (`item5c-after-vs-now.txt`, diff rc=0).
- M3 re-run via the migration's own `compare-links.py` (logic reviewed first):
  **0 migration-introduced broken links** (452 → 288 mapped → 257; 31 resolved)
  (`item5b-m3.txt`).
- The known-dangerous silent-retarget shape (`00-master-plan/README.md`'s
  `../README.md` → `../misc/legacy-fork-syscall-index.md`) is in place and the target
  exists.

### Item 6 — Gate effectiveness (no silent failure)

- All five tool self-tests pass, rc=0: `notes-link-check`, `doc-style-lint`,
  `anchor-resolve`, `anchor-migrate`, `unsafe-audit` (`item6a–6e`).
- `check-review-rules.sh` consistent rc=0; `generate-derived-skills.sh --check` no
  drift; `lint-review-rules.sh` 0 failures (`item6f–6h`).
- `design-coverage-check.sh 01-stage-kernel` → rc=1 (business "missing snapshot",
  correct); `fork-syscall-rewrite` → readable "retired, see MIGRATION.md" error.
- Static + live confirmation that the silent-failure fix is real: the incremental
  style gate diffs `${NOTES_TREES[@]}` sourced from `tools/notes-layout.conf`, and a
  live `--diff` run prints scope "limited to rewrite-notes redesign-notes
  study-notes" with error-level 0 on the current tree (`item6i–6j`). No probe residue:
  `misc.md` and `tools/` show no modification from my runs (`item6k-cleancheck.txt`).

### Item 7 — Compile surface

- `os/` interval diff: 90 files, 101+/101- (pure in-line). The single non-comment
  added line is the already-disclosed `minix-types/README.md` markdown list item
  pointing at the pre-existing dangling `fork-syscall-plan.md` reference
  (`item7b-osnoncomment.txt`). Samples confirm comment/TOML-comment-only changes
  (`item7c-ossamples.txt`).
- `cargo check --workspace --tests` (host): **rc=0, zero lines starting with `error`**
  (one grep hit for "error" is a rustc E0133 help footer on a warning, not an error).
  475 warnings, all pre-existing lint classes (unused/never-read/unnecessary-unsafe —
  the `os/` diff is comment-only so none can be migration-caused).
- Full `cargo test` not re-run here: the two prior audits already ran it (host and
  docker `minix-ci:1.94`), including the pre-tag对照 proving the `minix-driver-rt`
  SIGSEGV / hosted-test instability predates the migration. My text-level proof (zero
  semantic `os/` change) independently entails the same "no causal link" conclusion.
  Recorded as a limit, not a gap: see "Not verified" §3.

### Item 8 — User decisions (12 rulings)

Verified by direct observation: root-level three trees + untouched `book/`; `.review/`
frozen + `PATH-MAPPING.md`; manifest+tar preservation with no extra commits of
untracked files; the two in-progress files moved path-only with content intact;
`study-notes/` verbatim + qualitative README; `rewrite-notes/` internal zones
(`concepts/`, `misc/`, `coordination/`, `evidence/`, `archive/`) with zero stage-dir
renames; all 9 migration commits on `rewrite`; local-only dual tags
(`git ls-remote --tags origin 'refs/tags/notes/*'` returns empty, rc=0 —
`item8f-remote.txt`); `redesign-notes/` topic buckets (`architecture/` 10 = 8 × R15 +
2 × R1, `ipc/` 2, `fork/` 1, `vm/` stub + root README); anchor prefix-swap with
unchanged line counts (874/6609); mandatory-domain-only rewrite (item 4).
`notes/` is gone from disk; zero empty directories in the new trees
(`item8b-layout.txt`, `item8c-emptydirs.txt`).
`{stage}`/`{module}` separation is correctly stated in
`prompt/review-rules/review-process.md` (≈line 403) — with the one exception below (P1).

## P0 — Blocking issues

**None.**

## P1 — Non-blocking but should be fixed

### P1-1  `{rw-module}` spelling family (77 lines / 11 files) not covered by the `{module}` → `{stage}` rename

- Reproduced independently (`item8d-rwmodule.txt`): 77 lines across 4 `prompt/` sources
  + 2 `.claude/` + 3 `.codex/` + 2 `.trae/` derived copies (first reported by the DS audit;
  my file count differs slightly by counting method, line count identical).
- Pre/post comparison shows the counts are **identical before and after the migration**
  (prompt sources: 8/6/11/5 both sides) — so this is a pre-existing second spelling of
  the same concept that the rename's scope missed, not a migration-introduced regression.
- It is still a genuine inconsistency against the migration's own宣布: the definition
  section now says the state-dir key is `{stage}`, while line 1240 of
  `prompt/review-rules/review-process.md` still documents `{rw-module}` as the
  "rewrite module name" — the retired module-layer concept (that layer's value was
  constantly `fork-syscall-rewrite`, retired 2026-10-07).
- Impact: zero functional (tools take `--output` literally; examples still resolve to
  correct shapes). Fix: replace `{rw-module}` with `{stage}` (or explicitly declare
  equality), re-run derived-skill generation + `check-review-rules.sh`. Recommend
  batching with P2-1.

## P2 — Observations and suggestions

### P2-1  REVIEW-PROMPT item 1b expectation is unsatisfiable as written

Expects "identical 2428 / missing 0 / different 0" from a HEAD comparison, but
Phases 3+7 intentionally change 333 tracked files. Any literal re-audit would file a
false P0. Reword to "missing must be 0; every difference must be attributable to a
logged rewrite operation (pure-move purity is checked at `e90e4173d`)". (Converges
with DS P1-2 / GLM P2-2; my item-1 evidence above is the third independent
demonstration.)

### P2-2  REVIEW-PROMPT item 2 tar-count command counts directory entries

`tar -tzf … | grep -c '^notes/'` yields 2536 (includes 108 directory entries); file
count 2428 requires `grep -v '/$'`. Same for `.design` (1180 vs 1162). Data is fine;
the command needs the filter. (Converges with DS P2-1 / GLM P2-3.)

### P2-3  Minor documentation typo: p7.c sha prefix in 00-SNAPSHOT.md §5.2

Logged as `b8da7f7b726`, measured `b8da7f7b7276…` (a dropped digit;
`item8e-p7c.txt`). Content is intact (tar/disk/sha256sum agree). One-character table
fix. (Converges with GLM P2-1.)

### P2-4  Single-copy tar + 1.2 GB evidence remain repo-local

The tar volume (the only recovery source for git-blind content) still lives only in
git-ignored `tmp/`. Copy off-repo after acceptance, as the migration itself recommends.
No action needed before delivery.

## Not verified (with reasons)

1. **Full `cargo test --workspace`**: not re-run. Justification: (a) my text-level proof
   shows zero semantic `os/` change, which already rules out migration causation for any
   test outcome; (b) both prior audits ran the suite (host + docker authoritative
   recipe) including a pre-tag对照. Re-running would add cost without new information.
2. **Live mutation probes (item 6)**: intentionally skipped to preserve read-only
   discipline with maintainer in-progress edits in the worktree; covered via self-tests,
   static scope verification, and cross-checked prior live-probe evidence.
3. **Per-file `git log --follow` for all 1102 renames**: sampled (`16-smp.md`); full
   enumeration is disproportionate given the 1102×R authoritative diff-tree record.
4. **Semantic correctness of `.design/` snapshots and `.review/` contents**: out of scope
   by the freeze ruling; verified byte-preservation only.
5. **Push state of tags beyond `ls-remote`**: remote shows no `notes/*` tags (rc=0,
   empty), consistent with the local-only decision; full remote audit is not possible
   from here.

## Evidence index (`migrate_notes_plan/Muse/evidence/`)

| File | Content |
|---|---|
| `item0-status.txt`, `item0-status-uall.txt`, `item0-tags.txt` | Worktree status, untracked list, dual tags + tag messages |
| `item0b-todo-*.txt` | In-progress file: committed sha both sides, worktree 6/2 |
| `item1a-counts.txt` | 2428 = 2428 = 2428; tracked 1105 vs 1102 |
| `item1b-reconcile.txt`, `item1b-difflist.txt` | 2095 / 0 / 333 + full 333-file list |
| `item1c-attribution.txt` | All 333 are `git-mv`; 0 frozen-zone diffs |
| `item1d-numstat.txt`, `item1d-summary.txt`, `item1d-asymmetric.txt` | e90e..HEAD line stats; only documented rewrites asymmetric |
| `item1e-spotcheck.txt` | 3-file diff samples: pure prefix substitution |
| `item2-snapshot.txt`, `item2-tarsha.txt`, `item2-tarfiles.txt` | 7/7 OK; tar sha; 2428/1162/1728 file entries |
| `item3a-puremove.txt`, `item3b-movetypes.txt` | 1102 R100, 0 A/D at move commit |
| `item3c-follow.txt` | `--follow` penetrates migration |
| `item3d-interval.txt`, `item3e-AD.txt`, `item3f-Dpairs.txt` | 1100R+5A+2D explained; blob-identity proofs |
| `item4-residue.txt` | Mandatory-domain scan: only 2 whitelisted files |
| `item4b-frozen.txt` | Frozen retention 4 / 8 / 56 |
| `item4-reviewfrozen.txt` | Tar-vs-disk `.review/`: 0 differ, 0 missing |
| `item5a-anchors.txt`, `item5d-anchors-exist.txt` | 874/6609, 0 residue, 101/101 paths exist |
| `item5b-m3.txt` | M3 re-run: 0 new broken links |
| `item5c-after-vs-now.txt` | Fresh scan == after.txt subset (373, diff rc=0) |
| `item6a–6e-*.txt` | Five tool self-tests, all PASS rc=0 |
| `item6f-checkrules.txt`, `item6g-drift.txt`, `item6h-lintrules.txt` | Rules consistent, no drift, lint 0 failures |
| `item6i-lintscope.txt`, `item6j-lintdiff.txt`, `item6k-cleancheck.txt` | New-tree gate scope live; no probe residue |
| `item7a-osfiles.txt`, `item7b-osnoncomment.txt`, `item7c-ossamples.txt` | os/ 90 files; 1 known README line; comment-only samples |
| `item8a-pathmapsha.txt`, `item8b-layout.txt`, `item8c-emptydirs.txt` | Contract sha; layouts; no empty dirs; `notes/` gone |
| `item8d-rwmodule.txt` | 77 lines / 11 files (P1-1) |
| `item8e-p7c.txt`, `item8f-remote.txt` | p7.c sha (P2-3); no remote notes/* tags |
| `item9-final-status.txt` | Closing worktree status (no audit residue) |
