## What this is

A rewrite of the MINIX 3 kernel and its user-space servers in Rust for x86-64, ARM64 and
RISC-V. This is not a line-by-line translation: the goal is to keep MINIX 3's observable
behaviour while expressing the implementation in Rust's type system. The original C source
under `minix3/` is the reference for every claim.

## Project Structure

- `os/` contains the Rust workspace: kernel, user-space servers, drivers, libraries, and the
  `xtask`/QEMU harnesses that build and boot them.
- `minix3/` contains the unmodified source code from the official MINIX 3 project.
- `rewrite-notes/` contains the rewrite documentation, one concept per C source area, organised
  by service boot order (`rewrite-notes/01-stage-kernel/16-smp.md`). Start with its README.
- `redesign-notes/` contains architecture-exploration documents. This area activates once the
  rewrite stabilises; its current contents are unreviewed proposals, not implementation guidance.
- `study-notes/` contains early AI-generated MINIX 3 study notes. They were never verified
  against the C source, so read them as leads, not as facts.
- `book/` is the mdBook output area, generated from the note trees once content settles.
- `tools/` contains the build, review, coverage, and anchor-checking scripts.
- `prompt/` is the single source of truth for the review rule set; `.claude/`, `.trae/`, and
  `.codex/` are derived adapters, and `CLAUDE.md` / `AGENTS.md` are the per-tool entry documents.
- `tmp/` is the scratch area and is not tracked by Git: forensic logs and run evidence
  (`tmp/evidence/`), QEMU images and memory dumps (`tmp/bin/`), serial logs (`tmp/log/`),
  debug harnesses (`tmp/nk4a/`), plus the archived migration record in `tmp/migrate_notes_plan/`
  and the machine-migration notes in `tmp/new_laptop_migrate/`. Nothing outside `tmp/` should
  hold throwaway artefacts.

## Where to start

1. `rewrite-notes/README.md` — the stage index and the reading order.
2. `rewrite-notes/00-master-plan/README.md` — why the stages run in this order, and who loads
   whose executable image.
3. `rewrite-notes/RECONSTRUCTION-PRINCIPLES.md` — what may change and what must not.
4. `CLAUDE.md` / `AGENTS.md` — the working rules for the review workflow (ground-truth priority
   chain, directory conventions, quality gates).

## License

- Original code in `os/`, documentation in `rewrite-notes/`, `redesign-notes/`, `study-notes/`
  and `book/`, and notes are licensed under the **MIT License** (see [LICENSE](LICENSE)).
- The `minix3/` directory contains unmodified source code from the official MINIX 3 project,
  licensed under its original **BSD-like license**. See [minix3/LICENSE](minix3/LICENSE) for
  the full text.
