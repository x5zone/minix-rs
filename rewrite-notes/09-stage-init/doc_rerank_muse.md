# 09-stage-init Document Rebuild Blueprint (muse)

## Execution header (report section 1, verbatim)

```text
your_name(AI agent name) = muse
target_dir(concerned working directory) = rewrite-notes/09-stage-init
repo_root(repository root) = /home/xzhao/github/minix-rs

Task = Phase R: rebuild blueprint. Output target_dir/doc_rerank_muse.md. Modify no body text.
       Reading C code alone is not enough; also read: header declarations of all docs in
       this directory, the corresponding entry points under os/, the navigation tables of
       00-*-overview.md.
Constraint = Must not cite .design/ or tmp_design_and_todo/; every landed artifact must carry
       the _muse suffix; must not read or copy other AIs' doc_rerank_* products.
```

All factual claims below carry anchors. Judgments without anchors are explicitly marked
"speculation" or "to be verified". This report is written in English (per muse convention).
It is self-contained: it does not assume the reader has seen any prior conversation.

Scope note (user addendum, accepted): a single doc may carry a single concept even if long
(soft row limit, up to ~3000 rows for extremely complex concepts); readability controls
length in the normal case. Old doc count is not a constraint: docs may be added or removed.
This task exists to fix defects in existing docs, rebuilding where needed; old docs may
contain errors or unreviewed content, so the ground truth is the minix3 + Rust code, not
the old prose.

---

## 0. Metadata

- Executor: muse. Date (UTC): 2026-09-20. Target: `rewrite-notes/09-stage-init`.
- Commit: `ca49a1a7914fdf1f7c29a77479e5c1312b40d970` (from `git rev-parse HEAD`).
- Stage type判定: **startup-chain type** (see §1, last paragraph). The whole stage is one linear
  boot state machine plus its supporting data structures; there is no event-loop service,
  no syscall collection, no driver set. Teaching order = runtime order, with exactly two
  recorded order-differences (§4, order-difference table).
- What counts as a doc: the 16 numbered docs `00` + `01`–`14` + `99`.
  Reference material (not docs): `plan.md`, `todo.md`, `README.md`, `draft/README.md`,
  `archive/todo-V1-archive-2026-09-18.md`.
  Out of scope: `.design/`, `draft/` internals, `archive/` internals, other AIs'
  `doc_rerank_*` files (not read, per constraint).

### 0.1 Document inventory (line counts measured 2026-09-20)

| Doc | Title | Lines | Header declares: about / not-about / prereqs |
|-----|-------|-------|-----------------------------------------------|
| 00-init-overview.md | INIT overview | 25 | about: what init is, boot position, state-machine map, navigation; prereq: none (+ kernel 09/10 pre-reads); not-about: all mechanisms |
| 01-init-main-entry.md | Entry and process identity | 251 | about: main() full flow; prereq: 01-stage-kernel 06/09; not-about: 02 state machine, 03 logging/disaster, 12 securelevel mechanism, 02/03/14 handler semantics |
| 02-init-state-machine.md | State-machine skeleton and signal conversion | 196 | about: transition/requested_transition/handlers/7 state chars; prereq: 01; not-about: state bodies (04–11), 03, 13 runlevel hook, 14 hooks |
| 03-init-logging-failure.md | Logging trio and fatal signals | 150 | about: stall/warning/emergency/disaster; prereq: 02; not-about: 02 transitions, 14 hooks |
| 04-init-single-user.md | Single-user repair state | 128 | about: state 's' full flow; prereq: 02/03; not-about: 12 security mechanism, 09 setctty/collect_child, 05 rc |
| 05-init-runcom.md | Running the startup script | 103 | about: state 'r', runcom/runetcrc; prereq: 04/02/03; not-about: 12 chroot mechanism, 09 setctty/collect_child, 13 utmp |
| 06-init-read-ttys.md | Reading the terminal table | 88 | about: state 't', read_ttys/do_setttyent; prereq: 02/05; not-about: 07 struct details, 08 DB, 13 boot records, 12 chroot path |
| 07-init-session-model.md | Session structure and lifetime | 88 | about: session_t, SE_* flags, new/free/setupargv/construct_argv; prereq: 06; not-about: 08 DB ops, 09 start/reap, 09 thrash comparison |
| 08-init-session-db.md | Session database | 75 | about: start/add/del/find, ARCH A-1; prereq: 07; not-about: 13 utmp hook |
| 09-init-multi-user.md | Multi-user steady state | 79 | about: state 'm', start_getty/window/setctty/collect_child; prereq: 06/07/08; not-about: 13 records, 12 chroot |
| 10-init-clean-ttys.md | Re-reading the terminal table | 74 | about: state 'T', PRESENT-diff, n-squared; prereq: 07/09; not-about: 07 argv mechanism |
| 11-init-shutdown.md | Catatonia and death | 72 | about: states 'c'/'d', DEATH_WATCH; prereq: 02/09; not-about: 13 shutdown records |
| 12-init-sysctl-interaction.md | Security level and new root | 72 | about: has/get/setsecuritylevel, createsysctlnode/shouldchroot, ARCH A-4/A-5; prereq: 01/04/05/09 call sites; not-about: kernel sysctl internals |
| 13-init-utmp.md | Session log ledger | 77 | about: session_utmpx/make_utmpx/get_runlevel/utmpx_set_runlevel/clear_session_logs, ARCH A-2; prereq: 02/07; not-about: libc file-format internals |
| 14-init-external-contracts.md | External contracts | 96 | about: minixreboot/minixpowerdown + 13 cross-service contracts; prereq: 01/02/09/11; not-about: peer-service internals |
| 99-init-global-concepts.md | Global concept tables | 40 | about: constants/paths/globals shared tables; prereq: none; not-about: all mechanisms |

Reference material: `plan.md` 389 lines (reorg plan, final 2026-08-16, coverage contract §5, ARCH list §4
A-1–A-11); `todo.md` 100 lines (Rust arch-review TODO, 2026-09-18 round closed P0×10/P1×7/P2×6,
4 open items on shared infra); `README.md` 39 lines (16-doc catalog + key files + boot position).

Finding F-0 (size health): all docs are short (25–251 lines) and each carries one semantic
unit. There is no 3000-line monster and no forced split pressure. The rebuild problem of
this stage is **not length or ordering** — it is **anchor forgery, homeless Rust topics,
thin 00/99, and stale test claims** (see §3). Consequence: the new catalog keeps the same
16 numbers and titles (§4); the rebuild is content-rewrite-in-place, not reshuffle.

### 0.2 C source inventory (all files in `minix3/sbin/init/`, verified by listing)

| File | Lines | Owner doc |
|------|-------|-----------|
| `init.c` | 1902 | 01–14 (function-level map §5.2 of plan.md, re-verified in §1 below) |
| `pathnames.h` | 40 | 99 (`_PATH_SLOGGER` dead constant, `_PATH_RUNCOM`) |
| `init.8` | 390 | 00/02 (7-state behavior contract, NetBSD wording) + 05/11 (rc/chroot/shutdown wording) |
| `Makefile` | 20 | 99 + plan ARCH A-7 (build-variant macros) |
| `NOTES` | 119 | 02 reference only (POSIX duties: orphan reaping, job control, controlling terminal) |

Header dependencies of init.c (non-init files that carry init-visible semantics):
`minix3/include/paths.h` (`_PATH_BSHELL`, `_PATH_CONSOLE`, `_PATH_CONSTTY`, `_PATH_DEV`,
`_PATH_TTYS`) → 99/01/04/06; `minix3/include/ttyent.h` (`TTY_ON`, `TTY_SECURE`) → 06/07;
`minix3/include/utmp.h` + `utmpx.h` (paths `utmp.h:42-43`, `utmpx.h:39-40`, `RUNLVL_MSG`
`utmpx.h:74`, `BOOT_MSG`/`DOWN_MSG`) → 13/99; `minix3/include/db.h` (`db.h:72,213`) → 08;
`minix3/minix/servers/pm/const.h:9` (`INIT_PID`) → 99.

### 0.3 Non-C artifact inventory (§4 item 3 of the prompt, each answered)

| Artifact class | Actual files found | Verdict |
|----------------|-------------------|---------|
| Link scripts | none apply (init is a normal dynamically-unlinked user binary, not a boot image) | explicitly excluded with reason (§7, N-1) |
| Assembly entry / trap entry | none (init enters via exec + kernel scheduling, not via its own asm) | excluded with reason (§7, N-2) |
| Boot chain / boot protocol | `minix3/minix/kernel/table.c:64` (boot_image last slot), `minix3/minix/servers/rs/table.c:28` (USR_F), `minix3/minix/servers/vm/main.c:346-347` (fixed argv), `main.c:498-514` (exec_bootproc), `minix3/minix/servers/pm/main.c:188-204` (INIT parent/self, scheduler) | owned by 00/01/14 (§7, N-3) |
| Image layout | no init-owned image layout (single ELF) | excluded with reason (§7, N-4) |
| User startup / runtime loading | `/etc/rc` family (`minix3/etc/rc`, `rc.minix`, `rc.shutdown`, `rc.d/`), `/etc/ttys` (`minix3/etc/ttys` sample exists), `/etc/MAKEDEV` fallback, `/sbin/shutdown` target | owned by 05/06/01/14 (§7, N-5) |
| Cross-module interfaces / wire formats | utmp/wtmpx record layout (libc), Berkeley DB `DB_HASH` memory-table ABI, sysctl `init.root` node ABI, `TTY_*` line format, shutdown argv protocol (`-r`/`-p now CTRL-ALT_DEL`) | owned by 13/08/12/06/14 (§7, N-6) |
| Build scripts / toolchain | `minix3/sbin/init/Makefile` (20 lines: `MFS_DEV_IF_NO_CONSOLE`, `SUPPORT_UTMP`, `SUPPORT_UTMPX`, default `ALTSHELL+SECURE+CHROOT`, `SMALLPROG→LETS_GET_SMALL`, `INIT_CHROOT→CHROOT`) | owned by 99 + ARCH A-7 (§7, N-7) |
| Test infrastructure / simulator scripts | `os/commands/sbin/init` crate tests + `ScriptHost` harness; no QEMU init-boot test exists (edge E5 open) | owned by per-doc §5 + edge pointer (§7, N-8) |

### 0.4 Boundary material

- `rewrite-notes/00-master-plan/README.md` (95 lines): stage table row
  `09 → 09-stage-init → INIT → user-space init, starts login/user processes (boot_image last
  item, table.c:64)`; boot two-layer semantics note (registration order vs execution order);
  directory numbering follows execution + reading order.
- `edge_todo.md`: E-INITSYS (§E-INITSYS, 2026-09-17 registration) is the only init-blocking
  edge item — three minix-sys client pieces (signal family, process-control family + open path,
  WaitStatus decode + WUNTRACED); 09-side association: P0-1–P0-7 live halves, P1-1, P2-6 residue.
  Related: E-CMDSYSFACE (no_std decision), E-ISBOOT (`/etc/rc` equivalent missing), E5 (end-to-end).
- Target `plan.md` + `todo.md`: read in full (see §0.1). Items declared not-to-do: LETS_GET_SMALL
  variant not implemented in Rust (plan §5.4); utmp/syslog/securelevel/CHROOT live halves
  deferred to shared infra (ARCH A-2–A-5, todo §1 open items); libcrypt backends deferred
  (todo P0-8 residue).
- Previous stage `08-stage-is/00-is-overview.md`: IS is a conditional debug service with no
  boot_image slot; its concepts (SEF loop, hooks table, five data channels) do not overlap
  init. Consequence: no de-duplication needed against stage 08; the only shared pre-reads are
  kernel boot docs (01-stage-kernel 06/09/10) and PM docs, as already declared in 01's boundary.
- Rust entry: `os/commands/sbin/init/` — 20 files, 5556 lines total (19 modules + main.rs;
  largest: `host.rs` 770, `single_user.rs` 577, `driver.rs` 539, `multi_user.rs` 468,
  `runcom.rs` 511). Crate `minix-init`. Baseline per todo.md: `cargo test -p minix-init`
  136 passed / 0 failed (2026-09-20 re-measured; older 143/138 figures are ledger drift).
- Style example `01-stage-kernel/06-todo.md`: only its contract-writing form is borrowed
  (what / not-what / handed-to-whom / acceptance), none of its conclusions.

### 0.5 Reference-relationship inventory (retrieval evidence)

- Doc→doc inside 09: all cross-references already use the new-number form
  (`02-init-state-machine.md`, `09-init-multi-user.md`, …). No old-number references remain.
- External references into 09: exactly two — `18-stage-commands/00-commands-overview.md:61`
  (prereq on 09-stage-init 01's rc semantics) and `edge2.md:24` (P1-2 panic-handler pointer).
  Both use directory-level, not doc-level, anchors: renumber-insensitive.
- Code→doc references: `os/commands/sbin/init/src/*.rs` contains ~30 informal references of
  the forms `(see 02)`, `(doc 12)`, `(doc 04 gate)`, `(see module 07)`, plus 18
  `.design/NN-design.v1.md` contract pointers in module headers (host.rs, driver.rs,
  entry.rs, state_machine.rs, signal_state.rs, log.rs, ttys.rs, session.rs, session_db.rs,
  single_user.rs, runcom.rs, multi_user.rs, clean_ttys.rs, shutdown.rs, sysctl.rs, utmp.rs,
  contracts.rs, password.rs). The `.design` pointers violate the project hidden-folder rule
  and must all be migrated to stage-doc anchors (§8). The informal `(doc NN)` forms must be
  normalized to `NN-name.md §X` (§8).

Commands used (read-only): `ls`, `wc -l`, `rg -n`, `sed -n`, `git rev-parse HEAD`,
`git log --oneline`. No file was modified during evidence collection.

---

## 1. C True Order (runtime true sequence, rebuilt from source, not paraphrased)

Stage-type判定: **startup chain**. Evidence: `main()` (init.c:229) runs a linear prologue
(S0–S8 below) exactly once, then calls `transition(requested_transition)` (init.c:360-363)
which never returns (`for(;;) s = (*s)()` at init.c:624-644); every subsequent behavior is a
state-function return value. There is no message-receive loop, no SEF, no CALLMAP (plan §1.2,
confirmed by full-file read: no `sef_`, no `receive`, no callmap symbol in init.c).

### 1.1 True-order table

| # | Action | C anchor | Notes |
|---|--------|----------|-------|
| S0 | Boot handoff (before main): kernel boot_image last slot; RS USR_F registration; PM INIT self-parent + scheduler; VM exec_bootproc loads ELF with fixed argv `{"init",NULL}` | `minix3/minix/kernel/table.c:64`; `minix3/minix/servers/rs/table.c:28`; `minix3/minix/servers/pm/main.c:188-204`; `minix3/minix/servers/vm/main.c:346-347`, `main.c:498-514` | Not init.c; owned by docs 00/01/14. Record boot_time via gettimeofday at init.c:237-239 (SUPPORT_UTMPX). |
| S1 | Identity gates: `getuid()!=0 → EPERM exit`; `getpid()!=1 → "already running" exit` | `minix3/sbin/init/init.c:242-249` | LETS_GET_SMALL build skips both (init.c:230-251 `#ifndef` guard). |
| S2 | `setsid()` initial session (failure: warn + continue) | `init.c:255-256` | NetBSD `setlogin("root")` is `#if !defined(__minix)` (init.c:263-266): excluded, non-Minix branch. |
| S3 | `mfs_dev()`: if `/dev/console` missing, fork MAKEDEV (`./MAKEDEV` or `/etc/MAKEDEV`, args `-MM init`); failure → `requested_transition = single_user` | `init.c:269-271`, `init.c:1703-1790` (live part 1746-1788; 1716-1744 is `#if 0` dead test code) | MFS_DEV_IF_NO_CONSOLE build. |
| S4 | `openlog("init", LOG_CONS, LOG_AUTH)` | `init.c:280` | Skipped under LETS_GET_SMALL. |
| S5 | getopt `-s` → `requested_transition = single_user`; `-f` → `runcom_mode = FASTBOOT`; unknown → warning; excess args → warning | `init.c:289-311` | Defaults: `requested_transition = runcom` (init.c:195), `runcom_mode = AUTOBOOT` (init.c:151). LETS_GET_SMALL forces single_user (init.c:312-314). |
| S6 | Signal registration: minixreboot←SIGABRT, minixpowerdown←SIGUSR1, disaster←{FPE,ILL,SEGV,BUS}, transition_handler←{HUP,TERM,TSTP}, alrm_handler←ALRM; delset unblocks those 10; SIGTTIN/SIGTTOU ignored | `init.c:321-346` | Non-Minix branch additionally handles badsys←SIGSYS (init.c:318, 490-502): excluded. |
| S7 | `close(0/1/2)` paranoia | `init.c:351-353` | — |
| S8 | `createsysctlnode()` (CHROOT build) + `securelevel_present = has_securelevel()` | `init.c:356-366`, `init.c:1811-1857`, `init.c:544-566` | — |
| S9 | `transition(requested_transition)` — the infinite loop; per-iteration `utmpx_set_runlevel(old,new)` + `current_state = s` (SUPPORT_UTMPX) | `init.c:360-363`, `init.c:624-644`, `init.c:1429-1458` | The seven lines that are the whole "engine". |
| S10 | State 's': `single_user()` — downgrade securelevel; ignore TSTP/HUP in parent; fork child: constty-or-console setctty → SECURE password gate → ALTSHELL prompt → exec shell (fallback INIT_BSHELL); parent watch loop (stop→CONT, signal-death paths, exit→FASTBOOT→runcom) | `init.c:694-877` | Full body read; 5 outcomes (see K-031). |
| S11 | State 'r': `runcom()` → `runetcrc(0)` [/etc/rc autoboot|fastboot]; on success + shouldchroot → `runetcrc(1)` + `did_multiuser_chroot=1`; reset `runcom_mode=AUTOBOOT`; write reboot wtmp records; → read_ttys | `init.c:974-1019`, `init.c:879-972`, `init.c:1859-1902` | SIGTERM-during-rc + requested==catatonia → eternal sigsuspend (reboot path, init.c:944-952). |
| S12 | State 't': `read_ttys()` — BOOT/DOWN wtmpx records on first entry; destroy old list; start_session_db (fail → death if chrooted else single_user); do_setttyent; getttyent loop → new_session; → multi_user | `init.c:1222-1288` | — |
| S13 | Session support (called from S12/S14/S15, not states): construct_argv, new_session, free_session, setupargv, start/add/del/find_session_db | `init.c:1021-1099`, `init.c:1101-1220`, `init.c:1792-1809` | DB key = pid bytes; NULL-db tolerant add/find. |
| S14 | State 'm' (steady): `multi_user()` — securelevel→1 if 0; start all idle gettys; `waitpid(-1)` loop → collect_child (restart / SHUTDOWN-unlink / serious-trouble→clean_ttys) | `init.c:1528-1567`, `init.c:1321-1370`, `init.c:1290-1319`, `init.c:669-692`, `init.c:1460-1500` | start_getty: chroot, GETTY_SPACING/SLEEP debounce, window+WINDOW_WAIT, exec, `_exit(8)` on failure. |
| S15 | State 'T' (SIGHUP): `clean_ttys()` — clear PRESENT; per-line match (index-change warning, off→SHUTDOWN+SIGHUP, argv refresh); vanished lines → SHUTDOWN+SIGHUP; → multi_user | `init.c:1569-1632` | Self-declared n-squared (comment at init.c:1566-1567). |
| S16 | State 'c' (SIGTSTP): `catatonia()` — all SHUTDOWN; → multi_user | `init.c:1634-1647` | — |
| S17 | State 'd' (SIGTERM): `death()` — all SHUTDOWN; shutdown wtmp records; 3 rounds HUP/TERM/KILL with alarm(DEATH_WATCH)+clang wait + collect_child; ESRCH/ECHILD → single_user early; survivors → warning | `init.c:1661-1701`, `init.c:1649-1659` | `death_sigs[3]` at init.c:1667; DEATH_WATCH=10 at init.c:96. |
| S18 | Async intrusions (any steady point): minixreboot/minixpowerdown fork `/sbin/shutdown -r|-p now CTRL-ALT_DEL`; disaster logs + sleeps + `_exit(sig)`; transition_handler/alrm_handler only write request words | `init.c:517-541`, `init.c:504-515`, `init.c:1502-1526`, `init.c:1649-1659` | C forks inside signal handlers (init.c:519,532) — the one place C violates its own bridge model; Rust deliberately moves spawn to the loop boundary (declared deviation, §5 doc 14). |

Function census re-verified 2026-09-20: 46 declaration hits → 44 live definitions + `print_console`
(init.c:411, `#if 0` dead) + `badsys` (init.c:490, `#if !defined(__minix)` non-Minix). Zero
functions missing from plan §5.2; all line numbers in plan §5.2 confirmed against the
`rg ^(static )?(void|int|…)` output (S1–S18 table above doubles as the check).

---

## 2. Knowledge Pool (deduplicated, stage-wide)

Source types: **S** = stock (from existing docs), **N** = new (from C/non-C/Rust, absent in old
docs). Merge rule: one row per knowledge item; every existing location recorded; the primary
telling point is marked with `*`. Reader benefit is phrased as the question the item answers.

| ID | Name | Type | Src | Existing location(s) | Anchor | Reader answers |
|----|------|------|-----|----------------------|--------|----------------|
| K-001 | main identity gates (uid 0 / pid 1) | mechanism | S | 01 §2.2* | `init.c:242-249` | Why does init refuse to run as non-root or second instance? |
| K-002 | Initial setsid + warn-continue | mechanism | S | 01 §2.1* | `init.c:255-256` | Why does PID 1 detach from any terminal? |
| K-003 | setlogin non-Minix branch | constraint | N | nowhere (gap G-08) | `init.c:263-266` | What is deliberately NOT ported and why? |
| K-004 | mfs_dev MAKEDEV fallback | mechanism | S | 01 §2.4* | `init.c:1703-1790` (dead 1716-1744) | What happens when /dev/console does not exist? |
| K-005 | getopt -s/-f, runcom_mode AUTOBOOT/FASTBOOT | interface | S | 01 §2.3*, 04, 05 | `init.c:151`, `init.c:289-311` | How do boot flags steer the first state? |
| K-006 | Signal registration block (10 signals, 5 handlers) | mechanism | S | 01 §2.5 (call site), 02 §2.3* | `init.c:321-346` | Which signal goes to which handler? |
| K-007 | SIGTTIN/SIGTTOU ignore + full-mask discipline | mechanism | N | 01 passing mention (gap G-09) | `init.c:336-346` | Why is init immune to job-control stops? |
| K-008 | close(0/1/2) daemon hygiene | concept | S | 01 §1* | `init.c:351-353` | Why does init close stdio before the state machine? |
| K-009 | createsysctlnode call site + shouldchroot call sites | mechanism | S | 01 §2.6 (site), 12 §2* | `init.c:356-357`, `init.c:1811-1902` | Where is init.root created and consulted? |
| K-010 | securelevel_present probe | mechanism | S | 01 §2.6 (site), 12 §2* | `init.c:366`, `init.c:544-566` | How does init cope with kernels lacking securelevel? |
| K-011 | transition() entry call + never-returns | mechanism | S | 01 §2.1*, 02 §2.4 | `init.c:360-366` | Where does linear birth end and cyclic life begin? |
| K-012 | openlog(LOG_CONS, LOG_AUTH) setup | mechanism | N | 03 passing mention (gap G-10) | `init.c:280` | Which syslog channel do stall/warning/emergency write to? |
| K-013 | state_func_t / state_t recursive typedef | data-structure | S | 02 §2.1* | `init.c:125-131` | How does C type "a function returning a state"? |
| K-014 | Seven state characters d/s/r/t/m/T/c | data-structure | S | 02 §2.2*, 99 | `init.c:133-139` | What is the complete state alphabet? |
| K-015 | requested_transition dual default (runcom vs single_user under LETS_GET_SMALL) | constraint | S | 02, 99* | `init.c:195`, `init.c:217` | What is the first state in each build variant? |
| K-016 | transition loop + runlevel ledger hook | mechanism | S | 02 §2.4* | `init.c:624-644` | What exactly repeats forever? |
| K-017 | handle/delset variadic registration + "XXX SA_RESTART?" | mechanism | S | 02 §2.3* | `init.c:369-409` | How are handlers installed without SA_RESTART? |
| K-018 | transition_handler HUP/TERM/TSTP mapping + default-clear | mechanism | S | 02 §2.5* | `init.c:1502-1526` | Why do handlers only write a request word? |
| K-019 | alrm_handler + clang flag | mechanism | S | 02 §2.5*, 11 | `init.c:1649-1659`, `init.c:173` | How does the death countdown tick? |
| K-020 | current_state ledger variable | data-structure | N | 02 passing mention (gap G-11) | `init.c:201` | Where is "previous state" kept for runlevel records? |
| K-021 | LETS_GET_SMALL whole-program variant | constraint | N | 99 one line (gap G-12) | `init.c:216-218`, Makefile `SMALLPROG` | What disappears in the small build? |
| K-022 | init.8 seven-state table vs C letters | interface | N | 00/02 cite without mapping (gap G-13) | `init.8:60-160` | How does the man page's 1–7 numbering map to d/s/r/t/m/T/c? |
| K-023 | stall (log + 30s read time) | mechanism | S | 03 §2* | `init.c:440-455`, `init.c:95` | Why does init sleep after logging? |
| K-024 | warning (log, no sleep) | mechanism | S | 03 §2* | `init.c:457-470` | Which faults deserve a note but no pause? |
| K-025 | emergency (LOG_EMERG) | mechanism | S | 03 §2* | `init.c:472-488` | Which faults are emergencies? |
| K-026 | disaster (log + sleep + _exit(sig)) | mechanism | S | 03 §2* | `init.c:504-515` | What happens on a fatal signal? |
| K-027 | print_console #if-0 dead code | constraint | S | 03 (excluded)*, plan §5.4 | `init.c:411-437` | What code exists but must never be ported? |
| K-028 | badsys non-Minix branch | constraint | S | 03 (excluded)*, plan §5.4 | `init.c:490-502` | What belongs to NetBSD only? |
| K-029 | Session-logger NB notes (unimplemented wish) | constraint | S | 03 §1* | `init.c:453-455`, `init.c:648-651`, `init.c:1009` comments | Which improvements did upstream want but never build? |
| K-030 | single_user parent flow + TSTP/HUP ignore window | mechanism | S | 04 §2.1/§2.4* | `init.c:694-877` | How does the repair shell get guarded? |
| K-031 | Securelevel downgrade on entry | mechanism | S | 04 §2.2* | `init.c:724-728` | Why does single-user force level 0? |
| K-032 | SECURE password gate (console secure? root pw? ^D = go multi) | mechanism | S | 04 §2.3* | `init.c:741-772` | When is a password demanded before the shell? |
| K-033 | ALTSHELL prompt + INIT_BSHELL fallback exec | mechanism | S | 04 §2.3* | `init.c:774-811` | How can the admin pick a different shell? |
| K-034 | Watch-loop five outcomes (stop→CONT, KILL→sigsuspend, crash→restart, request→yield, exit→FASTBOOT) | mechanism | S | 04 §2.4* | `init.c:823-877` | What are all the ways the shell can end? |
| K-035 | runetcrc child six steps (ignore TSTP/HUP, setctty console, argv, unblock, chroot?, exec) | mechanism | S | 05 §2.2* | `init.c:884-916` | How is /etc/rc launched? |
| K-036 | rc argv autoboot/fastboot | interface | S | 05 §2.1* | `init.c:897-900` | What single argument does rc receive? |
| K-037 | rc parent wait + SIGTERM-during-rc reboot trap | mechanism | S | 05 §2.3* | `init.c:932-955` | How does /sbin/reboot executed from rc halt the wait? |
| K-038 | rc exit-code protocol (nonzero/signal → single_user) | interface | S | 05 §1/§2.3* | `init.c:957-971` | What is the only communication channel with rc? |
| K-039 | runcom double-run + did_multiuser_chroot latch | mechanism | S | 05 §2.4* | `init.c:974-1019`, `init.c:210` | Why might rc run twice? |
| K-040 | reboot/shutdown wtmp records | mechanism | S | 05 §2.4 (site), 13* | `init.c:1012-1017` | Where are boot/shutdown recorded? |
| K-041 | read_ttys destroy-and-rebuild | mechanism | S | 06 §2* | `init.c:1252-1260` | Why is the session list never diffed here? |
| K-042 | BOOT/DOWN wtmpx records on first entry | mechanism | S | 06 §2 (site), 13* | `init.c:1229-1250` | When are BOOT_TIME/DOWN_TIME written? |
| K-043 | start_session_db failover (death if chrooted else single_user) | mechanism | S | 06 §2 (site), 08* | `init.c:1262-1271` | What if the session DB cannot open? |
| K-044 | do_setttyent chroot-aware ttys path | mechanism | S | 06 §1/§2* | `init.c:1792-1809` | Which /etc/ttys is read after chroot? |
| K-045 | getttyent loop → new_session tail-append | mechanism | S | 06 §2* | `init.c:1279-1282` | How does the text table become a linked list? |
| K-046 | session_t all fields | data-structure | S | 07 §2.1* | `init.c:156-170` | What does one login line look like in memory? |
| K-047 | SE_SHUTDOWN / SE_PRESENT flags | data-structure | S | 07 §1*, 10, 11 | `init.c:161-162` | How are "in table" and "do not restart" encoded? |
| K-048 | new_session TTY_ON/getty filter + device path join | mechanism | S | 07 §2* | `init.c:1142-1183` | Which ttys lines are skipped? |
| K-049 | free_session five-release discipline | mechanism | S | 07 §2* | `init.c:1123-1140` | What must be freed per session? |
| K-050 | setupargv getty+window vector build | mechanism | S | 07 §2* | `init.c:1185-1220` | How is "getty + tty" pre-parsed? |
| K-051 | construct_argv space/tab tokenizer | mechanism | S | 07 §2* | `init.c:1101-1121` | Why is the parser deliberately minimal? |
| K-052 | se_started anti-thrash field (defined 07, compared 09) | data-structure | S | 07 (def)*, 09 (use) | `init.c:159` vs `init.c:1350-1355` | Where is getty flapping detected? |
| K-053 | start_session_db (close-old + dbopen NULL = memory HASH) | mechanism | S | 08 §2* | `init.c:1021-1036` | Why does the DB vanish on process exit? |
| K-054 | add/del/find_session pid-keyed ops + NULL tolerance | mechanism | S | 08 §2* | `init.c:1038-1099` | How does waitpid's bare pid become a session? |
| K-055 | ARCH A-1: Berkeley DB → ordered map (BTreeMap under no_std) | arch-evolution | S | 08 §1/§3* | `db.h:72,213`; `os/commands/sbin/init/src/session_db.rs` | What changed and what behavior promise holds? |
| K-056 | multi_user start-all + waitpid loop | mechanism | S | 09 §2* | `init.c:1528-1567` | What does "steady state" actually do? |
| K-057 | start_getty (chroot, spacing debounce, window, exec→_exit(8)) | mechanism | S | 09 §2* | `init.c:1321-1370` | How is one getty born? |
| K-058 | start_window_system (setsid + exec→_exit(6)) | mechanism | S | 09 §2* | `init.c:1290-1319` | How is the window system born once per line? |
| K-059 | setctty (setsid/window/DTR-wait/open/login_tty; revoke on non-Minix) | mechanism | S | 09 §2* | `init.c:669-692` | How does a child acquire a controlling terminal? |
| K-060 | collect_child (unknown→ignore, SHUTDOWN→unlink, restart→clean_ttys on failure) | mechanism | S | 09 §2* | `init.c:1460-1500` | How are dead children reaped vs restarted? |
| K-061 | GETTY_SPACING/SLEEP/WINDOW_WAIT + dtrtime constants | data-structure | S | 09, 99* | `init.c:92-94`, `init.c:98` | What are the four sleeps and why those values? |
| K-062 | clean_ttys PRESENT-diff + n-squared + index-change warning | mechanism | S | 10 §2* | `init.c:1569-1632` | How are added/removed/changed lines reconciled? |
| K-063 | catatonia (all-SHUTDOWN → multi_user) | mechanism | S | 11 §2* | `init.c:1634-1647` | What does "boring mode" do? |
| K-064 | death three rounds + alarm/clang + ESRCH/ECHILD exits | mechanism | S | 11 §2* | `init.c:1661-1701` | How does shutdown escalate HUP→TERM→KILL? |
| K-065 | has/get/setsecuritylevel trio | mechanism | S | 12 §2* | `init.c:544-621` | How are the two securelevel knobs turned? |
| K-066 | createsysctlnode init.root fabrication | mechanism | S | 12 §2* | `init.c:1811-1857` | How is the init.root node born with default "/"? |
| K-067 | shouldchroot read/recreate/validate/compare | mechanism | S | 12 §2* | `init.c:1859-1902` | When does init enter the new root? |
| K-068 | ARCH A-4/A-5 defer (no securelevel/sysctl service in minix-rs) | arch-evolution | S | 12 §1/§3* | plan §4 A-4/A-5; todo §1 open | What is honestly missing and what is the contract? |
| K-069 | session_utmpx / make_utmpx / get_runlevel / utmpx_set_runlevel / clear_session_logs | mechanism | S | 13 §2* | `init.c:1372-1458`, `init.c:647-666` | Who writes the ledger that `who` reads? |
| K-070 | ARCH A-2 defer (dual UTMP+UTMPX build, ledger-first) | arch-evolution | S | 13 §1/§3* | Makefile `SUPPORT_UTMP + SUPPORT_UTMPX`; plan §4 A-2 | Why two log channels and what is deferred? |
| K-071 | minixreboot / minixpowerdown fork-shutdown protocol | mechanism | S | 14 §2.1* | `init.c:517-541` | How do SIGABRT/SIGUSR1 become reboot/powerdown? |
| K-072 | USR_F identity + boot_image-last + fixed-argv + orphan-adoption + procfs-false + reboot-path + CAD-SIGABRT + power-SIGUSR1 + exec/file deps (13 external contracts) | interface | S | 14 §2.2*, 00, 01 | plan §5.3's 13 anchors (kernel/table.c:64, rs/table.c:28, vm/main.c:346-347+498-514, pm/main.c:188-204, pm/forkexit.c:336+396, pm/misc.c:224, pm/schedule.c:34+73, procfs/service.c:195-207, keyboard.c:300, tps65217.c:226, init.c:797-808+1312+1365, pathnames.h+paths.h+ttyent.h) | What does the rest of the system promise init (and vice versa)? |
| K-073 | Global tables: constants/paths/globals + ARCH A-7 build macros | data-structure | S | 99* | `pathnames.h:39-40`, `paths.h`, `pm/const.h:9`, Makefile | Where is every magic number listed? |
| K-074 | InitHost single seam (ARCH A-11): all machine effects behind one trait; live halves honestly ENOSYS; ScriptHost for tests | arch-evolution | N | plan §4 A-11; `host.rs` (770 lines); mentioned in 00/01/02/03/12 but **no owning contract** (gap G-01) | `os/commands/sbin/init/src/host.rs` | Where do fork/exec/waitpid/kill/signal get decided vs executed? |
| K-075 | DriverState single ownership + ChildCollector/Ledger views; real-graph step() tests | arch-evolution | N | `driver.rs` (539 lines); mentioned in 00 only (gap G-02) | `os/commands/sbin/init/src/driver.rs` | How did ten C globals become one owned struct? |
| K-076 | SignalState atomic-bit split (handler writes bits, loop drains + spawns) + deliberate fork-out-of-handler deviation | arch-evolution | N | `signal_state.rs`; 02/14 touch but neither owns (gap G-03) | `os/commands/sbin/init/src/signal_state.rs`; `init.c:517-538` (C side) | Why must handlers never fork, and where does spawn happen? |
| K-077 | Password pure decision (parse/dispatch/gate) + libcrypt backend defer | arch-evolution | N | `password.rs`; 04 §3.1 touches, 12 referenced ambiguously (gap G-04) | `os/commands/sbin/init/src/password.rs`; `init.c:741-772` | Which password checks are pure logic vs missing crypto? |
| K-078 | WaitStatus decode (wait.h bit-field anchored) + WUNTRACED | arch-evolution | N | `wait.rs` (8 lines); no doc owns (gap G-05) | `os/commands/sbin/init/src/wait.rs` | How are raw waitpid integers classified? |
| K-079 | no_std container substitution (HashMap→BTreeMap, Arc/atomic via alloc/core) | arch-evolution | N | 08 mentions outcome only (gap G-06) | todo P1-2; `session_db.rs`, `main.rs`, `driver.rs` | Why BTreeMap, and why is key order unobservable? |
| K-080 | PID 1 / orphan adoption / session / controlling-terminal / daemon-hygiene / runlevel / getty-login-cycle OS theory | concept | S | scattered in 01/07/09/13 ch1 (no single home; 00 candidate, gap G-07) | NOTES file; `init.8` | What OS theory must a reader know before ch1 of 01? |

Statistics: 80 items; by type: concept 3 (K-008, K-080 + K-012-adjacent counted as mechanism),
mechanism 47, data-structure 10, interface 7, constraint 6, arch-evolution 7.
By source: stock 66, new 14 (K-003, K-007, K-012, K-020, K-021, K-022, K-074–K-080 minus overlaps).
Duplicates merged: securelevel mechanism (01/04/05/09 call sites → 12 primary); utmp hooks
(02/05/06/09/11 call sites → 13 primary); chroot consults (05/06/09 → 12 primary);
se_started def/use (07 def primary, 09 use reference); BOOT/DOWN records (06 site, 13 primary);
reboot/shutdown records (05 site, 13 primary); A-11 seam mentions (00/01/02/03/12 → new §5
contract homes, §4 decision D-3).

---

## 3. Coverage Audit

### 3.1 Theme universe vs pool vs docs

Four theme sources checked: (1) C symbols — 44 live functions + 10 globals + 12 macro groups,
all in pool (§2) and all mapped (§1 table + plan §5.2 re-verified); (2) OS generic concepts —
K-080, covered but homeless (G-07); (3) non-C artifacts — §0.3 table, each with an owner;
(4) stage-boundary contracts — K-072's 13 items, all owned by 14 (00/01 reference only).

### 3.2 Coverage-gap table (every item becomes pool N-items above or an explicit exclusion)

| Gap | Theme | Evidence | Disposition |
|-----|-------|----------|-------------|
| G-01 | InitHost seam has no owning doc | `host.rs` 770 lines; only scattered mentions | Adopt into 02 (registration half) + 14 (deviation half); seam catalog in 00 (decision D-3) |
| G-02 | DriverState ownership has no owning doc | `driver.rs` 539 lines; only 00 mentions | Adopt into 02 (transition-driver half, decision D-4) |
| G-03 | SignalState split owned by nobody | `signal_state.rs`; 02/14 partial | Adopt into 02 as §4 decision (decision D-5) |
| G-04 | Password module ownership ambiguous (04 §3.1 vs "doc 12") | `password.rs` header: "Design contract 04 §1.1, doc 12" | Adopt into 04 as §3 decision; 12 keeps only securelevel half (decision D-6) |
| G-05 | WaitStatus owned by nobody | `wait.rs` 8 lines | Adopt into 09 as §3 micro-decision (decision D-7) |
| G-06 | no_std container story only in 08 outcome line | todo P1-2 | Adopt into 08 (DB half) + 99 (global policy half) (decision D-8) |
| G-07 | OS theory homeless (PID1/session/ctty/daemon/runlevel/getty) | scattered ch1 paragraphs | Adopt into 00 as §1 "prerequisite theory" (decision D-9) |
| G-08 | setlogin non-Minix branch unlisted | `init.c:263-266` | Explicit exclusion in 01 (with K-003) |
| G-09 | SIGTTIN/TTOU + mask discipline unowned | `init.c:336-346` | Adopt into 02 §2 (with K-007) |
| G-10 | openlog channel unowned | `init.c:280` | Adopt into 03 §2 (with K-012) |
| G-11 | current_state ledger var unowned | `init.c:201` | Adopt into 02 §2 (with K-020) |
| G-12 | LETS_GET_SMALL variant home unclear | `init.c:216-218` | Adopt into 99 as variant table (with K-021) |
| G-13 | init.8 numbering map missing | `init.8:60-160` | Adopt into 02 (map table) + 00 (nav row) (with K-022) |
| G-14 | Forged header anchors in ALL 16 docs (systematic wrong-symbol `(Lxxx, tool-generated)` text) | §8 table | Rewrite every header anchor to verified `init.c:NNN` (P0, decision D-1) |
| G-15 | Stale test statistics ("as of 2026-09-18" mixed 143/138/89–161 counts) | per-doc §5.1 vs todo baseline 136 | Re-audit every §5.1 against `cargo test -p minix-init` real output (P0, decision D-2) |
| G-16 | 18 `.design/` pointers in code headers + informal `(doc NN)` refs | §0.5 inventory | Migrate all to `NN-name.md §X` (decision D-10, §8 table) |

No gap requires a brand-new numbered doc: every N-item has an adopting home (§4 D-3–D-9).
§6 records this per non-C checklist item ("merge into X, not new doc, because…").

### 3.3 Duplicate-theme table

| Theme | Repeats in | New home: primary | Others become |
|-------|-----------|-------------------|---------------|
| securelevel mechanism | 01 §2.6, 04 §2.2, 05, 09 call sites vs 12 | 12 (mechanism + ARCH defer) | one-line "mechanism: see 12 §2" + call-site behavior stays |
| utmp hooks | 02 (ledger), 05/06/09/11 (call sites) vs 13 | 13 (records + ARCH defer) | one-line "record: see 13 §2" |
| chroot consults | 05 §2.4, 06 §1, 09 vs 12 | 12 (node + decision) | one-line "decision: see 12 §2" |
| password gate | 04 §2.3 vs 12 (ambiguous ref) | 04 (gate + pure decision) | 12 drops password, keeps securelevel |
| session-DB mention | 06 §2 vs 08 | 08 | 06 keeps failover edge only |
| state-char alphabet | 02 §2.2 vs 99 table | 02 (first complete telling) | 99 keeps value-only table row |

### 3.4 Out-of-scope theme table

None of the 16 docs teaches out-of-scope topics. Boundary reaffirmations for the rewrite:
kernel sysctl internals → `../01-stage-kernel/` (12 references, never expands); PM/VFS/RS
internals → respective stages (14 references); libc getttyent/utmp file-format internals →
deferred behind ARCH contracts (06/13 state the boundary, never implement); QEMU/boot-shim
details → kernel docs (00/01 reference only).

### 3.5 Non-C checklist answers (fixed list, no "to be determined")

Linking/loading: N-1 excluded (ordinary user ELF; no init-owned linker script — speculation:
none found in `minix3/sbin/init/`; verified by directory listing). Image/memory layout: N-4
excluded (single ELF, no layout owned). Assembly/trap entry: N-2 excluded (exec-loaded;
no asm owned). Boot assembly: N-3 owned by 00 (chain map) + 01 (handoff steps) + 14
(contract table). Build/toolchain: N-7 owned by 99 (macro table) + plan ARCH A-7. Cross-module
interfaces/wire: N-6 owned by 06/08/12/13/14 as listed. Error paths: every state doc owns its
`→single_user / →clean_ttys / →death` edges (§5 contracts list them). Shutdown/exit: 11 owns
both states; `_exit` codes table moves to 99 (codes 0,1,2,3,4,5,6,7,8,9,10,11,12 across
init.c:739-1786 — currently unlisted anywhere; new 99 row). Concurrency/sync: the bridge model
(handler-writes-word) owned by 02; the fork-in-handler deviation owned by 14; no threads exist.
Test infrastructure: per-doc §5 re-audited (D-2) + ScriptHost owned by 02/00; E5 QEMU gap
pointed to, not filled.

---

## 4. New Catalog

Decision: **keep all 16 numbers and titles; rewrite content in place; add zero docs;
merge zero docs; archive zero docs** (draft/ already archived; no doc is absorbed away).

| # | Title (unchanged) | One-line定位 | Group |
|---|-------------------|--------------|-------|
| 00 | init-overview | Where init sits in the boot chain and how to read docs 01–14/99 | A overview |
| 01 | init-main-entry | Linear birth S1–S8: identity, session, devices, flags, signals (sites), stdio, sysctl node, probe → transition | B entry |
| 02 | init-state-machine | The bridge model: alphabet, registration, loop, request words, ledger, SignalState, driver | B skeleton |
| 03 | init-logging-failure | stall/warning/emergency/disaster + syslog channel + dead-code exclusions | B infrastructure |
| 04 | init-single-user | State 's': guarded shell with password gate and five endings | C launch sequence |
| 05 | init-runcom | State 'r': /etc/rc black box with exit-code protocol, double-run | C launch sequence |
| 06 | init-read-ttys | State 't': table → linked list translation | C launch sequence |
| 07 | init-session-model | session_t + flags + argv vectors (the node) | D session |
| 08 | init-session-db | pid→session memory index + A-1/no_std story | D session |
| 09 | init-multi-user | State 'm': start-all + waitpid reap loop + WaitStatus | E steady |
| 10 | init-clean-ttys | State 'T': PRESENT-diff reconciliation | E steady |
| 11 | init-shutdown | States 'c'/'d': boring vs three-round kill | F shutdown |
| 12 | init-sysctl-interaction | securelevel trio + init.root node/decision (A-4/A-5 defer) | G system interaction |
| 13 | init-utmp | Login ledger + runlevel records (A-2 defer) | G system interaction |
| 14 | init-external-contracts | USR_F truth + 13 contracts + fork-out-of-handler deviation | G system interaction |
| 99 | init-global-concepts | Constants/paths/globals/_exit codes/variants table | H tables |

Why no renumber/merge/split (with evidence):

- D-0 Order is already runtime order. S1–S18 (§1) maps onto 01→02→03→04→05→06→07→08→
  09→10→11→12→14 with zero inversions except the two recorded differences below. A reshuffle
  would pay breakage cost (§8: ~40 doc→doc refs + ~30 code→doc refs + 18-stage prereq) for no
  readability gain. Old-doc count is not a constraint, but change needs a reason; here the
  defects are content defects (G-14/G-15/G-01–G-13), not ordering defects.
- D-1 (P0) Every header anchor is rewritten to a verified `minix3/sbin/init/init.c:NNN`
  symbol anchor (§8 migration table). The `(Lxxx, tool-generated)` wrong-symbol strings are
  deleted, not repaired in place, because the symbol names themselves are wrong (e.g. 01
  "make_utmpx (L229)" for `main`; 02 "setsecuritylevel (L624)" for `transition`; 09
  "transition_handler (L1528)" for `multi_user`).
- D-2 (P0) Every §5.1 test-statistics block is re-audited against a fresh
  `cargo test -p minix-init` run at rewrite time; counts + test names must match
  `cargo test` output exactly (Gate E). Stale 2026-09-18 figures are deleted.
- D-3 G-01: no new "host seam" doc. The seam's registration half belongs to 02 (it installs
  the handlers 02 specifies) and its deviation half to 14; 00 gains a seam catalog row.
  Reason: the seam has no independent runtime order position — it is the execution substrate
  of S6/S10/S11/S14/S17. A separate doc would forward-reference every state.
- D-4 G-02: no new "driver" doc. DriverState is the ownership telling of 02's loop; the
  contract lives in 02 §4. Reason: same — substrate, not a stage in the chain.
- D-5 G-03: SignalState lives in 02 §4 (the bridge's Rust telling). 14 references the
  deviation consequence only.
- D-6 G-04: password lives in 04 §3 (the gate's Rust telling). 12 drops all password text.
- D-7 G-05: WaitStatus lives in 09 §3 as a micro-decision (8-line module; a doc would be
  90% air).
- D-8 G-06: no_std container story splits 08 (DB half: why BTreeMap is unobservable) / 99
  (policy half: alloc-without-hash rule).
- D-9 G-07: OS theory preface moves to 00 §1 (PID 1, sessions, ctty, daemon hygiene,
  runlevels, getty cycle). State docs delete their re-explanations and point to 00 §1.
- D-10 Code-comment hygiene: all 18 `.design/` pointers and all `(doc NN)` informal refs in
  `os/commands/sbin/init/src/*.rs` migrate to `NN-name.md §X` at rewrite time (§8 table).
  Code edits are one-line comment swaps; no logic change.

Reading paths: main line 00→01→02→03→04→05→06→07→08→09→10→11→12→13→14 (runtime order);
skippable branches: 12/13/14 independently (system-interaction group, each states prereqs only);
99 as lookup table anytime after 00. Parallel-group rule does not apply (no parallel body;
single chain + substrate).

Order-difference table (teaching order vs runtime truth S1–S18):

| # | Runtime fact (anchor) | Teaching choice | Reason + back-reference compensation |
|---|----------------------|-----------------|--------------------------------------|
| OD-1 | 12's mechanisms run at S8 (probe/node) and inside S10/S11/S14 (get/set/chroot) | 12 taught after 11 | Mechanisms need all call sites known first; each call-site doc carries "mechanism: see 12 §2" one-liner (§3.3). Zero forward reference: 12's prereq lists 01/04/05/09, all earlier. |
| OD-2 | 13's hooks run inside S9 (ledger) and S12/S14 (records) | 13 taught after 12 | Same reason; call sites carry "record: see 13 §2". 13's prereqs (02/07) are earlier. |

Forward-reference scan on the new catalog: every contract's "prereq" points to strictly
earlier numbers (verified in §9 G3). Dependency graph is a DAG with one linear spine
(verified in §9 G4).

---

## 5. Per-Doc Contracts (all 16; B-phase writes prose from exactly these)

### 00-init-overview

- One-line定位: the boot-chain map and reading guide; the only doc a newcomer reads first.
- About: K-072 (chain subset: table.c:64, rs/table.c:28, vm argv/exec, pm INIT rows — map form,
  not mechanism), K-014 alphabet, K-080 OS theory preface (D-9), K-074 seam catalog (names +
  one line each, mechanism elsewhere), K-022 init.8 nav row, reading paths (§4), 99 pointer.
- Not-about: every mechanism (→01–14); Rust module details (→per-doc §4); test lists (→per-doc §5).
- Prereq: none (pre-reads kernel 09/10 suggested, not required).
- Post: all of 01–14/99 reference this doc's map.
- Ground truth: `minix3/minix/kernel/table.c:64`; `minix3/minix/servers/rs/table.c:28`;
  `minix3/minix/servers/vm/main.c:346-347`, `main.c:498-514`;
  `minix3/minix/servers/pm/main.c:188-204`; `minix3/sbin/init/init.8:60-160`; `NOTES` (theory).
- Knowledge rows: K-072(chain) / K-014 / K-080 / K-074(catalog) / K-022(nav).
- Acceptance: reader can draw the boot chain with init's slot + name all 7 states + state
  where each doc lives, without opening any other doc; OS-theory questions all answered in §1.

### 01-init-main-entry

- One-line定位: linear birth S1–S8; the anchor point every later doc points back to.
- About: K-001, K-002, K-004, K-005, K-006 (sites only), K-008, K-009 (site), K-010 (site),
  K-011, K-003 (exclusion), K-007 (mask half: install sites; discipline theory →02).
- Not-about: state machine (→02), logging/disaster (→03), securelevel mechanism (→12),
  handler semantics (→02/14), boot-argv truth beyond site (→14 K-072 row).
- Prereq: 00 (+ kernel 06/09/10 suggested).
- Post: 02 (loop entry), 12 (probe/node sites), 14 (argv/identity rows).
- Ground truth: `init.c:229-367`; `init.c:1703-1790`; `pathnames.h`; `paths.h`
  (`_PATH_BSHELL`, `_PATH_CONSOLE`, `_PATH_CONSTTY`); `vm/main.c:346-347` (argv, site only).
- Knowledge rows: K-001 / K-002 / K-003 / K-004 / K-005 / K-006(site) / K-007(sites) /
  K-008 / K-009(site) / K-010(site) / K-011.
- Acceptance: eight steps S1–S8 each with anchor + failure action; every site has a
  "mechanism: see NN" line; no mechanism expanded here.

### 02-init-state-machine

- One-line定位: the bridge model — async signals in, one state at a time out.
- About: K-013, K-014, K-015, K-016, K-017, K-018, K-019, K-020, K-007 (discipline),
  K-009?? no — K-011 handoff, K-022 map table, K-075 driver telling, K-076 SignalState telling,
  K-074 registration half.
- Not-about: state bodies (→04–11), logging (→03), runlevel records (→13), reboot hooks (→14).
- Prereq: 01, 00 §1 (theory).
- Post: 03 (disaster site), 04–11 (alphabet users), 13 (ledger site), 14 (deviation site).
- Ground truth: `init.c:125-139`, `init.c:151`, `init.c:173`, `init.c:195`, `init.c:201`,
  `init.c:195-218`, `init.c:369-409`, `init.c:624-644`, `init.c:1502-1526`, `init.c:1649-1659`;
  `init.8:60-160`; `NOTES`; `os/commands/sbin/init/src/{state_machine,signal_state,driver}.rs`
  + `host.rs` registration half.
- Knowledge rows: K-013 / K-014 / K-015 / K-016 / K-017 / K-018 / K-019 / K-020 / K-007 /
  K-022 / K-075 / K-076 / K-074(reg).
- Acceptance: bridge diagram (signal→word→loop→state); 7 chars × handler table with anchors;
  init.8↔letters map; SignalState split + driver ownership stated with deviation pointers;
  no state body expanded.

### 03-init-logging-failure

- One-line定位: the three log levels plus the fatal path, and what was never built.
- About: K-023, K-024, K-025, K-026, K-027, K-028, K-029, K-012, K-074 log-sink half
  (LogSink→InitHost face, one paragraph + pointer).
- Not-about: transitions (→02), reboot hooks (→14).
- Prereq: 02 (disaster site).
- Post: 04/05/09 (stall callers).
- Ground truth: `init.c:440-515` (minus dead 411-437/490-502); `init.c:95`; `init.c:280`;
  `os/commands/sbin/init/src/log.rs` + `host.rs` sink face.
- Knowledge rows: K-023 / K-024 / K-025 / K-026 / K-027 / K-028 / K-029 / K-012.
- Acceptance: level table (when to stall/warn/emergency) + disaster sequence + both exclusions
  with reasons + syslog channel named; session-logger wishes quoted as wishes, not promises.

### 04-init-single-user

- One-line定位: state 's' — the guarded repair shell and its five endings.
- About: K-030, K-031 (site; mechanism →12), K-032, K-033, K-034, K-077 password telling,
  K-072 exec-dep row (site; table →14).
- Not-about: rc (→05), session/ctty internals (→07/09), securelevel mechanism (→12),
  collect_child mechanism (→09).
- Prereq: 02, 03, 00 §1.
- Post: 05 (FASTBOOT handoff).
- Ground truth: `init.c:694-877`; `init.c:724-728` (site); `init.c:741-811`;
  `os/commands/sbin/init/src/single_user.rs` + `password.rs`.
- Knowledge rows: K-030 / K-031(site) / K-032 / K-033 / K-034 / K-077.
- Acceptance: five endings each with anchor + next state; password-gate truth table
  (console-flag × from-level × root-pw → ask/skip); pure-vs-backend split for crypto named.

### 05-init-runcom

- One-line定位: state 'r' — the /etc/rc black box and its exit-code protocol.
- About: K-035, K-036, K-037, K-038, K-039, runcom.rs entity telling (decision/execution split).
- Not-about: chroot decision mechanism (→12), setctty/collect_child (→09), utmp records (→13).
- Prereq: 04 (FASTBOOT source), 02, 03.
- Post: 06 (read_ttys handoff).
- Ground truth: `init.c:879-1019`; `minix3/etc/rc*` (script side, black-box boundary);
  `os/commands/sbin/init/src/runcom.rs`.
- Knowledge rows: K-035 / K-036 / K-037 / K-038 / K-039 (+K-040 site line).
- Acceptance: child six steps + parent wait + five outcomes each with anchor; double-run
  flowchart with `did_multiuser_chroot` latch; "only exit codes cross the boundary" stated.

### 06-init-read-ttys

- One-line定位: state 't' — text table becomes linked list.
- About: K-041, K-042 (site), K-043 (edge; mechanism →08), K-044, K-045, K-052?? no —
  K-046?? no. (Session struct →07; boot_time →99.)
- Not-about: session fields (→07), DB ops (→08), boot records mechanism (→13), chroot (→12).
- Prereq: 02, 05.
- Post: 07 (node), 08 (DB edge), 09 (list consumer).
- Ground truth: `init.c:1222-1288`; `init.c:1792-1809`; `minix3/etc/ttys` (sample);
  `minix3/include/ttyent.h`; `os/commands/sbin/init/src/ttys.rs`.
- Knowledge rows: K-041 / K-042(site) / K-043(edge) / K-044 / K-045.
- Acceptance: destroy-rebuild sequence with anchors; failover branch (death vs single_user);
  chroot-aware path stated with "mechanism: see 12".

### 07-init-session-model

- One-line定位: the node — what one login line looks like in memory.
- About: K-046, K-047, K-048, K-049, K-050, K-051, K-052 (definition half).
- Not-about: DB ops (→08), start/reap (→09), thrash comparison (→09, field defined here).
- Prereq: 06.
- Post: 08 (node consumer), 09 (field consumer), 10 (flag consumer).
- Ground truth: `init.c:156-170`; `init.c:1101-1220`; `ttyent.h` (`TTY_ON`, `TTY_SECURE`);
  `os/commands/sbin/init/src/session.rs`.
- Knowledge rows: K-046 / K-047 / K-048 / K-049 / K-050 / K-051 / K-052(def).
- Acceptance: full field table with anchors; constructor filter truth table; tokenizer
  edge cases (empty command → NULL) with Rust Option mapping.

### 08-init-session-db

- One-line定位: the pid→session memory index and its no_std retelling.
- About: K-053, K-054, K-055, K-079 (DB half), K-042?? no — utmp hook site line (→13).
- Not-about: session fields (→07), utmp mechanism (→13).
- Prereq: 07.
- Post: 09 (lookup consumer), 06 (failover reference).
- Ground truth: `init.c:1021-1099`; `db.h:72,213`; `os/commands/sbin/init/src/session_db.rs`.
- Knowledge rows: K-053 / K-054 / K-055 / K-079(DB).
- Acceptance: NULL-tolerance table (add/find on closed DB); A-1 behavior promise
  (interface + behavior unchanged, libdb gone); BTreeMap-unobservable argument in full.

### 09-init-multi-user

- One-line定位: state 'm' — start everything, then sleep in waitpid.
- About: K-056, K-057, K-058, K-059, K-060, K-061, K-052 (use half), K-078 WaitStatus telling.
- Not-about: utmp records (→13), chroot decision (→12).
- Prereq: 06/07/08, 00 §1.
- Post: 10 (SIGHUP edge), 11 (SHUTDOWN semantics), 13 (record sites).
- Ground truth: `init.c:1528-1567`; `init.c:1290-1370`; `init.c:669-692`; `init.c:1460-1500`;
  `init.c:92-94,98`; `os/commands/sbin/init/src/multi_user.rs` + `wait.rs`.
- Knowledge rows: K-056 / K-057 / K-058 / K-059 / K-060 / K-061 / K-052(use) / K-078.
- Acceptance: start-all loop + waitpid loop with anchors; collect_child decision tree;
  debounce arithmetic (5s/30s/3s) with example; WaitStatus bit map with wait.h anchor
  (to be verified at rewrite time against `minix3/sys/sys/wait.h` — marked thus if unread).

### 10-init-clean-ttys

- One-line定位: state 'T' — reconcile the list with the edited table.
- About: K-062 (+K-047 use, +K-050 use via setupargv refresh).
- Not-about: argv construction mechanism (→07), reap (→09).
- Prereq: 07, 09.
- Post: 09 (return edge).
- Ground truth: `init.c:1569-1632`; `os/commands/sbin/init/src/clean_ttys.rs`.
- Knowledge rows: K-062.
- Acceptance: three phases (clear/match/sweep) with anchors; n-squared note with rarity
  justification; index-change warning semantics; SHUTDOWN-mark-vs-unlink split with 09.

### 11-init-shutdown

- One-line定位: states 'c'/'d' — boring vs escalating kill.
- About: K-063, K-064, K-019 use (clang), K-077?? no (K-068?? no). death_sigs + DEATH_WATCH (→99 values).
- Not-about: shutdown records mechanism (→13), reap mechanism (→09).
- Prereq: 02 (clang), 09 (reap/SHUTDOWN).
- Post: 04 (single_user return), 12/13 (site lines).
- Ground truth: `init.c:1634-1701`; `init.c:96`, `init.c:1667`;
  `os/commands/sbin/init/src/shutdown.rs`.
- Knowledge rows: K-063 / K-064.
- Acceptance: catatonia vs death table; three-round escalation with alarm/clang timing;
  ESRCH/ECHILD early exits; survivor warning; all exits land on single_user.

### 12-init-sysctl-interaction

- One-line定位: the two kernel knobs — write-protect level and second-root address.
- About: K-065, K-066, K-067, K-068, K-009 mechanism half, K-010 mechanism half, sysctl.rs
  contract telling. Password text explicitly removed (→04).
- Not-about: kernel sysctl implementation (→`../01-stage-kernel/` docs); call-site behavior
  (stays in 01/04/05/09).
- Prereq: 01 (sites), 04/05/09 (callers).
- Post: 05/06/09 (decision consumers).
- Ground truth: `init.c:544-621`; `init.c:1811-1902`; `os/commands/sbin/init/src/sysctl.rs`.
- Knowledge rows: K-065 / K-066 / K-067 / K-068.
- Acceptance: probe/get/set flowchart with ENOENT/-1 paths; node fabrication fields;
  shouldchroot decision tree (missing→recreate→0, non-string→0, "/"→0, else 1); A-4/A-5
  defer contracts with unlock conditions (E-INITSYS).

### 13-init-utmp

- One-line定位: the ledger that `who`/`last` read.
- About: K-069, K-070, K-040 mechanism half, K-042 mechanism half, utmp.rs ledger-first telling.
- Not-about: session lifecycle (→07–11), libc file-format implementation (deferred).
- Prereq: 02 (state chars), 07 (fields).
- Post: 14 (end of chain).
- Ground truth: `init.c:1372-1458`; `init.c:647-666`; `init.c:1012-1017`; `init.c:1229-1250`;
  `utmp.h:42-43`; `utmpx.h:39-40,74`; Makefile dual flags; `os/commands/sbin/init/src/utmp.rs`.
- Knowledge rows: K-069 / K-070.
- Acceptance: record-type table (LOGIN/DEAD/RUN_LVL/BOOT/SHUTDOWN/INIT) × write site;
  get_runlevel 7-branch map; sessions-NULL skip rule with /var rationale; dual-channel
  build fact with single-ledger Rust telling.

### 14-init-external-contracts

- One-line定位: init is a well-known user process, not a service — and everything that follows.
- About: K-071, K-072 (full 13-row table), K-076 deviation consequence, K-005 boot-argv truth
  (VM fixed argv vs init.8 NetBSD wording correction).
- Not-about: peer-service internals (→their stages); signal mechanics (→02).
- Prereq: 01, 02, 09/11.
- Post: none (chain terminus; 18-stage-commands consumes 01's rc row).
- Ground truth: `init.c:517-541`; all 13 plan-§5.3 anchors; `procfs/service.c:195-207`;
  `keyboard.c:300`; `tps65217.c:226`; `os/commands/sbin/init/src/contracts.rs` +
  `signal_state.rs` drain half + `host.rs` spawn face.
- Knowledge rows: K-071 / K-072.
- Acceptance: 13-row contract table each with anchor + consumer doc; reboot/powerdown argv
  protocol bytes; USR_F-not-a-service stated with procfs evidence; fork-in-handler deviation
  declared with behavior-equivalence argument; boot-argv NetBSD-vs-Minix correction stated.

### 99-init-global-concepts

- One-line定位: the lookup tables — numbers, paths, globals, exits, variants.
- About: K-073, K-014 (values only), K-015 (defaults), K-021 variant table, K-061 values,
  _exit-code table (new: 0,1,2,3,4,5,6,7,8,9,10,11,12 with sites), dead-constant rows
  (`_PATH_SLOGGER`, `INIT_MOUNT_MFS`), K-079 policy half, ARCH A-7 macro table.
- Not-about: all mechanisms.
- Prereq: 00 (or standalone lookup).
- Post: referenced by all docs' value mentions.
- Ground truth: `pathnames.h`; `paths.h`; `pm/const.h:9`; `ttyent.h`; `utmp.h`; `utmpx.h`;
  `db.h`; Makefile; `init.c:92-107,151-211` (macros + globals).
- Knowledge rows: K-073 (+value rows for K-014/K-015/K-021/K-061).
- Acceptance: every magic number in 01–14 resolves to a 99 row with anchor; Rust constant
 落点 table refreshed against `os/commands/sbin/init/src/` at rewrite time (Gate E);
  variant table (default vs SMALLPROG vs INIT_CHROOT) complete.

---

## 6. Change Table

Operation types: rewrite-in-place (R), adopt (A, homeless topic → home), delete-with-reason
(X), archive (V). No reorder, no split, no merge, no new number (justified §4 D-0–D-10).

| Op | Type | Old position | New position | Reason | Knowledge items | Direction |
|----|------|--------------|--------------|--------|-----------------|-----------|
| C-01 | R | 00 (25-line skeleton) | 00 expanded (~150 lines) | Add theory preface + seam catalog + nav; keep map role | K-080, K-074(cat), K-022(nav), K-072(map) | stock-rewrite; new K-080 sourced from NOTES + init.8 |
| C-02 | R | 01 (251 lines, forged anchors) | 01 rewritten, same sections | Fix all anchors (D-1); add K-003 exclusion + argv-truth pointer | K-001–K-012 | stock→01 §1/§2 (+K-003 new, evidence init.c:263-266) |
| C-03 | R | 02 (196 lines) | 02 rewritten + §4 new decisions | Adopt driver/SignalState/seam-reg/mask/current_state/init.8-map | K-013–K-022, K-075, K-076, K-074(reg) | stock→02; new K-020/K-022/K-075/K-076 sourced from C + Rust files |
| C-04 | R | 03 (150 lines) | 03 rewritten | Add openlog channel; keep exclusions | K-023–K-029, K-012 | stock→03; new K-012 (init.c:280) |
| C-05 | R | 04 (128 lines) | 04 rewritten + password decision | Adopt password module; disambiguate from 12 | K-030–K-034, K-077 | stock→04; new K-077 (password.rs + init.c:741-772) |
| C-06 | R | 05 (103 lines) | 05 rewritten | Keep black-box telling; add reboot-trap emphasis | K-035–K-039 (+K-040 site) | stock→05 |
| C-07 | R | 06 (88 lines) | 06 rewritten | Keep; trim DB mechanism to edge line | K-041–K-045 | stock→06 |
| C-08 | R | 07 (88 lines) | 07 rewritten | Keep; mark se_started def-vs-use split | K-046–K-052 | stock→07 |
| C-09 | R | 08 (75 lines) | 08 rewritten + no_std half | Adopt container-substitution argument | K-053–K-055, K-079(DB) | stock→08; new K-079 (todo P1-2 + session_db.rs) |
| C-10 | R | 09 (79 lines) | 09 rewritten + WaitStatus micro-decision | Adopt wait.rs | K-056–K-061, K-052(use), K-078 | stock→09; new K-078 (wait.rs + wait.h to be verified) |
| C-11 | R | 10 (74 lines) | 10 rewritten | Keep three-phase telling | K-062 | stock→10 |
| C-12 | R | 11 (72 lines) | 11 rewritten | Keep boring-vs-kill telling | K-063–K-064 | stock→11 |
| C-13 | R | 12 (72 lines) | 12 rewritten, password removed | Remove password text (→04); keep trio+node+defers | K-065–K-068 | stock→12 minus password |
| C-14 | R | 13 (77 lines) | 13 rewritten | Keep ledger telling; own BOOT/DOWN + reboot/shutdown records | K-069–K-070 (+K-040/K-042 mech) | stock→13 |
| C-15 | R | 14 (96 lines) | 14 rewritten + deviation declared | Own fork-out-of-handler deviation + argv correction | K-071–K-072 | stock→14 |
| C-16 | R | 99 (40-line skeleton) | 99 expanded (~120 lines) | Add exits/variants/dead-consts/Rust-落点 refresh | K-073, K-021, K-079(policy) | stock→99; new exit table (init.c sites), K-021 table |
| C-17 | A | homeless K-074–K-079 | 00/02/04/08/09/99 (§4 D-3–D-8) | Six adoptions, zero new docs | K-074–K-079 | new→homes with Rust-file evidence |
| C-18 | X | print_console, badsys, setlogin-branch, revoke-branch, `_PATH_SLOGGER`, `INIT_MOUNT_MFS`, `#if-0` MAKEDEV block | deleted with reason (01/03/99 exclusion rows) | Dead/non-Minix/unused, each with anchor | K-027, K-028, K-003 + revoke (`init.c:674-678` non-Minix arm) | explicit X rows in 01/03/99 |
| C-19 | V | `draft/README.md` | stays in draft/ (already archived) | Placeholder is素材 only; never promoted | — | no move |

Spot-check rule (§5 prompt G6): ten operations sampled — C-02 (K-003→01 exclusion),
C-03 (K-076→02 §4), C-05 (K-077→04 §3), C-09 (K-079→08), C-10 (K-078→09), C-13 (password
out of 12), C-16 (exit table→99), C-18 (four X rows), C-04 (K-012→03), C-01 (K-080→00 §1):
each has a stock-destination (new § + subsection) and a source anchor. PASS.

---

## 7. Missing→New (gap-closure list; no item left "to be determined")

Each §3.2 gap closed: G-01→C-03/C-15/C-01; G-02→C-03; G-03→C-03; G-04→C-05/C-13;
G-05→C-10; G-06→C-09/C-16; G-07→C-01; G-08→C-02/C-18; G-09→C-03; G-10→C-04;
G-11→C-03; G-12→C-16; G-13→C-03/C-01; G-14→C-02–C-16 + §8; G-15→D-2 + §9 G5;
G-16→§8 table.

Non-C checklist closure (why no new doc for any item): N-1/N-2/N-4 are exclusions with
reasons (no init-owned artifact exists — verified by `ls minix3/sbin/init/` showing only
Makefile/NOTES/init.8/init.c/pathnames.h); N-3 splits across 00/01/14 because each
consumer needs only its row (chain map / handoff steps / contract bytes) — a standalone
"boot protocol" doc would duplicate kernel docs; N-5 splits 05/06/01/14 for the same reason
(script content belongs to whoever consumes it); N-6 splits 06/08/12/13/14 by interface
(each wire has exactly one reader); N-7 lives in 99 (single macro table); N-8 lives in
per-doc §5 + edge pointer (test reality is per-module, not a chapter).

---

## 8. Anchor Migration and Breakage Cost

### 8.1 Header-anchor migration (all 16 docs; old strings deleted, new anchors verified §1)

| Doc | Old forged text (to delete) | Verified replacement |
|-----|----------------------------|----------------------|
| 00 | `kernel/table.c:64` + `rs/table.c:28` (bare, no symbol) | `minix3/minix/kernel/table.c:64` (boot_image) + `minix3/minix/servers/rs/table.c:28` (boot_image_priv) + add `vm/main.c:346-347,498-514` + `pm/main.c:188-204` |
| 01 | `make_utmpx (L229)` for main; `mfs_dev 1703-1788`; `paths.h:_PATH_CONSOLE,125` | `main init.c:229-367`; `mfs_dev init.c:1703-1790`; `paths.h` symbol rows (BHShell/CONSOLE/CONSTTY/DEV/TTYS) with per-symbol lines at rewrite time |
| 02 | `setsecuritylevel (L624)` for transition; `handle 369-389`; `delset 394-405`; `transition_handler 1502-1522`; `alrm_handler 1649-1655` | `transition init.c:624-644`; `handle init.c:369-392`; `delset init.c:394-409`; `transition_handler init.c:1502-1526`; `alrm_handler init.c:1649-1659` |
| 03 | `print_console (L440)` for stall; `warning 457-466`; `emergency 472-481`; `disaster 504-511` | `stall init.c:440-455`; `warning init.c:457-470`; `emergency init.c:472-488`; `disaster init.c:504-515`; exclusions `print_console init.c:411-437 (#if 0)`, `badsys init.c:490-502 (non-Minix)` |
| 04 | `setctty (L694)` for single_user | `single_user init.c:694-877` (setctty is init.c:669-692, owned by 09) |
| 05 | `runetcrc (L974)` for runcom; `runetcrc 879-969` | `runcom init.c:974-1019`; `runetcrc init.c:879-972` |
| 06 | `setupargv (L1222)` for read_ttys; `do_setttyent 1792-1806` | `read_ttys init.c:1222-1288`; `do_setttyent init.c:1792-1809` |
| 07 | `init_session` (no lines) for session_t; `SE_* 161-162`; `new_session 1142-1180`; `free_session 1123-1137`; `setupargv 1185-1217`; `construct_argv 1101-1118` | `session_t init.c:156-170`; `SE_* init.c:161-162`; `new_session init.c:1142-1183`; `free_session init.c:1123-1140`; `setupargv init.c:1185-1220`; `construct_argv init.c:1101-1121` |
| 08 | `runcom (L1021)` for start_session_db; `add 1038-1057`; `del 1062-1075`; `find 1080-1096` | `start_session_db init.c:1021-1036`; `add_session init.c:1038-1060`; `del_session init.c:1062-1078`; `find_session init.c:1080-1099` |
| 09 | `transition_handler (L1528)` for multi_user; `start_getty 1321-1370`; `start_window_system 1290-1316`; `setctty 669-689`; `collect_child 1460-1497` | `multi_user init.c:1528-1567`; `start_getty init.c:1321-1370`; `start_window_system init.c:1290-1319`; `setctty init.c:669-692`; `collect_child init.c:1460-1500` |
| 10 | `multi_user (L1569)` for clean_ttys | `clean_ttys init.c:1569-1632` |
| 11 | `clean_ttys (L1634)` for catatonia; `death 1661-1698` | `catatonia init.c:1634-1647`; `death init.c:1661-1701` |
| 12 | `minixpowerdown (L544)` for has_securelevel; `getsecuritylevel 568-589`; `setsecuritylevel 594-618`; `createsysctlnode 1811-1857`; `shouldchroot 1859-1900` | `has_securelevel init.c:544-566`; `getsecuritylevel init.c:568-592`; `setsecuritylevel init.c:594-621`; `createsysctlnode init.c:1811-1857`; `shouldchroot init.c:1859-1902` |
| 13 | `start_getty (L1372)` for session_utmpx; `make_utmpx 1383-1409`; `get_runlevel 1411-1427`; `utmpx_set_runlevel 1429-1451`; `clear_session_logs 647-662` | `session_utmpx init.c:1372-1381`; `make_utmpx init.c:1383-1409`; `get_runlevel init.c:1411-1427`; `utmpx_set_runlevel init.c:1429-1458`; `clear_session_logs init.c:647-666` |
| 14 | `disaster (L517)` for minixreboot; `minixpowerdown 530-538` | `minixreboot init.c:517-528`; `minixpowerdown init.c:530-541` |
| 99 | (no forged anchors; values only) | Add per-row anchors at rewrite time; globals `init.c:151-211`; macros `init.c:92-107`; paths `pathnames.h:39-40` + `paths.h` + `pm/const.h:9` |

Pattern note: the forgery is a systematic symbol-shift (each header names the *neighbor*
function with approximately the right line number). B-phase must not "fix numbers only" —
every header is rewritten from §1's table.

### 8.2 Code-comment reference migration (logic-untouched, comment-only)

| File:line | Old pointer | New pointer |
|-----------|-------------|-------------|
| 18 module headers (`*.rs:1-8`) | `.design/NN-design.v1.md §…` | corresponding `NN-init-*.md §X` per §5 contracts (content of `.design` not cited; only the pointer target changes) |
| `main.rs:5`, `main.rs:94`, `driver.rs:46,61,63`, `ttys.rs:35`, `host.rs:144`, `runcom.rs:54`, `password.rs:5,77`, `clean_ttys.rs:11` | `(see 02)`, `(doc 12)`, `(doc 04 gate)`, `(see module 07)` … | full `NN-name.md §X` form per §5 (exact section fixed at rewrite time) |
| `host.rs:174-175` (E-SYSCALL-SIGN lesson) | edge_todo pointer (keep — edge refs are legal) | keep, normalize path |

### 8.3 Breakage-cost summary

- Affected doc→doc references: ~40 (all already new-number form; numbers unchanged → zero
  breakage from this blueprint; only section numbers inside rewritten docs need re-pointing,
  enumerated per-contract at B-phase).
- Affected code→doc references: ~30 informal + 18 `.design` pointers (§8.2; comment-only edits).
- External consumers: 2 directory-level refs (18-stage-commands overview, edge2.md) —
  renumber-insensitive, unaffected.
- Total: small. Hotspots: 01 header (most-cited entry doc), 02 header (most-cited skeleton).
  Batch method: rewrite headers first (C-02/C-03), then `rg` for stale `(L\d+.*tool-generated)`
  must return zero, then re-point code comments in one pass.

---

## 9. Verification, Self-Check Gates, Conclusion

### 9.1 Four mechanical checks (§5.4 of the prompt)

1. Forward-reference scan: every §5 contract's prereq points to strictly earlier numbers
   (00<01<02<03<04<05<06<07<08<09<10<11<12<13<14; 99 standalone). Two teaching/runtime
   differences recorded with compensation (OD-1/OD-2). PASS.
2. Dependency graph: linear spine + substrate pointers (02↔14 deviation, call-site→mechanism
   one-liners). No cycles: mechanism docs never point back to call-site docs for content.
   PASS (graph: 16 nodes, edges prereq-only, acyclic by construction).
3. Coverage: all 80 pool items have a home (§2 last column) or an explicit X row (§6 C-18);
   all 14 N-items carry evidence anchors; 6 X-items carry reasons + anchors. 100%. PASS.
4. Breakage accounting: §8 totals (~40 + ~48 + 2) with hotspots + batch method. PASS.

### 9.2 Self-check gates

| Gate | Check | Result |
|------|-------|--------|
| G1 | C true order checkable (10 random anchors re-read) | PASS — S1 `init.c:242-249` (uid/pid gates), S6 `init.c:321-346` (10-signal block), S10 `init.c:741-772` (password gate), S11 `init.c:944-952` (reboot trap), S12 `init.c:1262-1271` (DB failover), S14 `init.c:1350-1355` (spacing), S15 `init.c:1583-1617` (match loop), S17 `init.c:1684-1689` (alarm wait), S3 `init.c:1764-1765` (MAKEDEV argv), S18 `init.c:519,532` (fork in handler) — all re-read from source during this task |
| G2 | Pool completeness: every C file + non-C artifact has a home or exclusion | PASS — 5 C files (§0.2) + 8 non-C classes (§0.3) + 6 headers all assigned; exclusions N-1/N-2/N-4 reasoned |
| G3 | Zero forward references | PASS (§9.1.1) |
| G4 | Acyclicity;拆解 if cyclic | PASS, acyclic (§9.1.2) |
| G5 | 100% coverage: every pool item homed or deleted; new items anchored; deletions listed | PASS — 80/80 homed (73 homes + 7 X-rows: print_console/badsys/setlogin/revoke/SLOGGER/MOUNT_MFS/MAKEDEV-#if0) |
| G6 | Every split/merge states stock-destination; every new states source (10 sampled) | PASS — no splits/merges by design (D-0); 10 adoptions sampled in §6 |
| G7 | Every contract has 7 elements | PASS — §5: all 16 contracts carry定位/about/not-about/prereq/post/ground-truth/knowledge/acceptance (8 fields; superset) |
| G8 | Migration tables cover all changed docs + code comments | PASS — §8.1 (16/16 headers) + §8.2 (all comment refs) |
| G9 | Factual claims anchored (10 sampled); speculation labeled | PASS — §9.2/G1 sample re-verified; two speculation marks in report: N-1 "no linker script" (listing-evidenced, kept as exclusion), K-078 wait.h anchor (marked to-be-verified at B-phase) |

### 9.3 Conclusion and questions for the user

Conclusion: the blueprint is **complete and executable**. B-phase needs no further triage
decisions: for each of the 16 docs it takes material (stock paragraphs by §6 destinations,
new items by evidence anchors), checks against the §5 ground-truth list, and writes prose in
the new teaching order (= old order, repaired content). Estimated prose volume is unchanged
(~1400 lines total across 16 docs; 00 grows 25→~150, 99 grows 40→~120, others ±20%).

Open questions (user裁决, none blocking):

1. K-078's `wait.h` anchor: I did not re-read `minix3/sys/sys/wait.h:53-70` in this pass
   (todo cites it; Rust `wait.rs` claims it). B-phase must confirm or mark "to be verified".
   Acceptable?
2. Test baseline for D-2: todo says 136 passed (2026-09-20); per-doc §5.1 blocks cite older
   mixed counts. B-phase re-runs `cargo test -p minix-init` once and stamps all 16 docs with
   the same run. Acceptable?
3. Code-comment migration (§8.2) touches 18 Rust module headers (comment-only). Should that
   ride with B-phase doc rewrites, or as one separate comment-only commit first?
4. The `(Lxxx, tool-generated)` strings: I judge them unrepairable (symbols wrong, not just
   numbers) and prescribe deletion + rewrite from §1. If any downstream tooling parses those
   strings, say so before B-phase deletes them.
