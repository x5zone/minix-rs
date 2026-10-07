# 04-stage-pm Document Rebuild Blueprint (muse)

## 0. Metadata

```text
your_name(AI agent name) = muse
target_dir(关注的工作目录) = rewrite-notes/04-stage-pm
repo_root(仓库根目录) = /home/xzhao/github/minix-rs
commit = 2c45b606c (git rev-parse --short HEAD, 2026-09-19)
date = 2026-09-19 (UTC)

任务 = R 相·重建蓝图：output target_dir/doc_rerank_muse.md, touch no body text.
约束 = no citation of .design/ or tmp_design_and_todo/; only one landed file
        (this one); no other AI's doc_rerank_* product was read.
语言 = English output (per muse special requirement).
```

### 0.1 Scope

In-scope documents (22, line counts via `wc -l` on 2026-09-19):

| Doc | Lines | Doc | Lines |
|---|---|---|---|
| 00-pm-overview.md | 96 | 11-signal-core.md | 356 |
| 01-pm-init-main.md | 609 | 12-signal-handlers.md | 633 |
| 02-mproc-struct.md | 723 | 13-signal-flow.md | 472 |
| 03-mproc-table.md | 704 | 14-itimer.md | 547 |
| 04-ipc-dispatch.md | 645 | 15-credentials.md | 503 |
| 05-vfs-interaction.md | 349 | 16-scheduling.md | 412 |
| 06-event-subscription.md | 688 | 17-exec.md | 426 |
| 07-pm-fork.md | 409 | 18-trace.md | 467 |
| 08-pm-srv-fork.md | 332 | 19-time.md | 468 |
| 09-pm-exit.md | 342 | 20-misc-queries.md | 522 |
| 10-pm-wait.md | 352 | 99-global-concepts.md | 93 |

Reference material (read, not rebuilt): `plan.md` (446 lines, reorg plan + ARCH A-1..A-13 +
kernel-ability 4-column table + coverage contract), `todo.md` (architecture review ledger,
Fix #1..#60, wiring batch table A–H), `draft/` (9 files: README + do-fork/exit/wait/
srv-fork/mproc-design/pid-generator/pm-call-vfs-fork/integration-test — old fork-mainline
material, declared unmaintained), `edge_todo.md` (cross-stage items E1/E5/E6/E7/E9).

Out of scope: `.design/` and `tmp_design_and_todo/` (never cited, per project rule);
other AIs' `doc_rerank_*` (never read); `01-stage-kernel/`, `02-stage-vm/`, `05-stage-vfs/`,
`03-stage-rs/` bodies (only their contracts are cited as boundary).

### 0.2 Read list and key evidence

- All 22 docs above: header declarations (scope / non-scope / prerequisites) fully read;
  bodies sampled for knowledge-pool extraction (finding: headers already carry explicit
  "prerequisites / responsibility / handoff" tables — plan.md §3.4 — and are largely
  forward-reference-free in numeric order).
- C ground truth: `minix3/minix/servers/pm/` — 15 × .c (~4,747 lines) + 6 local .h;
  `minix3/minix/include/minix/callnr.h` (PM_BASE + 47 call numbers);
  `minix3/minix/include/minix/com.h:513-583` (VFS_PM_* RQ 12 + RS 11);
  `minix3/minix/servers/pm/table.c:14` (call_vec, NR_PM_CALLS=48 slots);
  `minix3/minix/servers/pm/main.c:49-110` (main loop), `:115-126` (sef_local_startup),
  `:131-244` (sef_cb_init_fresh), `:252-424` (reply + handle_vfs_reply).
- Non-C artifacts: `boot_image` struct (`minix3/minix/include/minix/type.h:148-154`,
  populated `minix3/minix/kernel/main.c:64,165`, table `kernel/table.c:44`);
  `pm/Makefile` (PROG=pm, `-lsys -ltimers`, via minix.service.mk);
  SEF startup (`minix3/minix/lib/libsys/sef.c:68`, `sef_init.c`);
  RS restart gate sites (`misc.c:154`, `exec.c:70,136`, `forkexit.c:159`, `signal.c:212`).
- Rust truth: `os/servers/pm/src/` — 37 files, ~19,864 lines; dispatch center
  `ipc/calls.rs:212 dispatch_pm_call` (explicit arms for all 47 registered calls,
  `_: Reply(ENOSYS)` catch-all only, calls.rs:991); `ipc/dispatcher.rs:89
  dispatch_message` (PROC_EVENT_REPLY / PM-call / ENOSYS three-way);
  `init.rs` (PmServer skeleton: init/run_once/reply/fill_boot_procs).
- Boundary material: `00-master-plan/README.md` (PM is loaded by RS, fork demoted to
  sub-mainline); `edge_todo.md` (E1 trap, E5 PM↔VM/SCHED, E6 SYS_* wrappers, E7 wire
  types); previous stage `03-stage-rs/00-rs-overview.md` (RS supervision semantics —
  PM documents only its own side); `01-stage-kernel/06-proc-init-boot-proc.md`,
  `19-syscall-signal.md` (kernel peers of PM init and PM signals).
- Commands run (read-only): `wc -l`, `git rev-parse`, `rg -n` over
  `os/servers/pm/src/ipc/calls.rs` (dispatch arms + ENOSYS sites),
  `ipc/dispatcher.rs`, `minix3/minix/servers/pm/utility.c`, cross-reference greps for
  `04-stage-pm` in `notes/` + `os/` (see §8.3). Three parallel inventory subagents
  collected doc headers, C symbols, and Rust/reference tables (their outputs are
  folded into §§0–3; no doc_rerank_* was touched by anyone).

### 0.3 Method premise (rebuild, not relocate)

The current catalog is a 22-node network whose numbering already encodes the intended
teaching order (startup → model → loop → protocols → lifecycle → signals → services),
and every doc already declares prerequisites that point strictly backward
(verified in §9, check 1). Relocating sections between old docs would force
re-balancing every boundary at once. The cheaper operation is: freeze the proven
numbering, fix each doc against the code-truth deltas found in §3, split exactly one
over-broad doc (20), add exactly one control-path doc (21), and expand the three
thinnest load-bearing docs (00/05/99). Total: 23 docs. Old docs are archived, not
deleted; every old section gets a destination row in §8.

---

## 1. C True Order

Stage type: **service event-loop** (per prompt §9). Two segments: birth+init, then the
message loop. Evidence anchors below are all in `minix3/minix/servers/pm/`.

### 1.1 Birth segment — T-init (from `main.c:49-56,115-244`)

| Step | Action | Anchor | Note |
|---|---|---|---|
| T-0 | Kernel boots PM from boot image | `kernel/main.c:64,165`, `kernel/table.c:44`, `type.h:148-154` | PM slot/endpoint assigned by kernel, not by PM |
| T-1 | `main()` → `sef_local_startup()` | `main.c:49,56,115-126` | Registers init_fresh (:118), STATEFUL restart (:119), signal-manager `process_ksig` (:122), then `sef_startup()` (:125) |
| T-2 | `sef_cb_init_fresh`: timers + `MP_MAGIC` + sigact + eventsub tables | `main.c:131-~150` | Data-structure ground zero → docs 02/03 |
| T-3 | Build core_sset / ign_sset / noign_sset | `main.c:~150-165` | Signal vocabulary → doc 11 (semantics), 99 (index) |
| T-4 | `sys_getmonparams()` | `main.c:~165-178` | monitor_params KVP → docs 01 (source), 20 (find_param consumer) |
| T-5 | `sys_getimage(boot_image[NR_BOOT_PROCS])` | `main.c:~178` | Boot-image array → doc 01 |
| T-6 | Fill mproc loop: name/sigsets/parent/pid/flags/scheduler/nice/endpoint; INIT self-father + IN_USE; RS/INIT parentage; PRIV_PROC/NONE scheduling class | `main.c:178-243` | Per-process identity → docs 02/03; nice value via `get_nice_value` (`main.c:275-289`, semantics owned by 16) |
| T-7 | `VFS_PM_INIT` per-proc send + final `ENDPT=NONE` sendrec barrier sync | `main.c:~230-240` | Table-mirror exchange with VFS → doc 01 (handshake), 05 (protocol vocabulary) |
| T-8 | `system_hz = sys_hz()`; `sched_init()` | `main.c:~241-244`, `schedule.c:20-50` | Clock frequency → docs 14/19; scheduler init → doc 16 (call site owned by 01) |

### 1.2 Loop segment — T-loop (from `main.c:59-110`)

| Step | Action | Anchor |
|---|---|---|
| L-0 | `sef_receive_status(ANY)` | `main.c:61` |
| L-1 | notify? CLOCK → `expire_timers()` | `main.c:65-70` → `alarm.c:check_vtimer` family |
| L-2 | who_e / source; `pm_isokendpt` caller check; `mp = mproc[who_p]`; `call_nr = m_type` | `main.c:73-79`, `utility.c:108` |
| L-3 | EXITING process: drop late calls | `main.c:82` → docs 04/09 |
| L-4 | Route 1: `IS_VFS_PM_RS` → `handle_vfs_reply()` (11 reply arms + tail), returns SUSPEND | `main.c:84-88,294-424`, `utility.c:123 tell_vfs` |
| L-5 | Route 2: `PROC_EVENT_REPLY` → `do_proc_event_reply()` | `main.c:89`, `event.c:218-309` |
| L-6 | Route 3: `IS_PM_CALL` → `call_vec[call_nr-PM_BASE]()` (47 handlers); else ENOSYS | `main.c:90-101`, `table.c:14`, `callnr.h:9-62` |
| L-7 | `result != SUSPEND` → `reply()` | `main.c:104-108` (SUSPEND = "answer later": wait4 completion, VFS-reply path, or never for do_exit) |

Dispatch detail (route 3 by family): lifecycle EXIT/FORK/WAIT4/SRV_FORK
(`forkexit.c:45/146/246/475`); signals KILL/SRV_KILL (`signal.c:197/207`), sigaction
family (`signal.c:40-196,776-855`), delay/resume machinery (`signal.c:226-293,652-776`);
timers ITIMER (`alarm.c:93`); credentials get/set 13 calls (`getset.c:19/95`); scheduling
get/setpriority (`misc.c:239-286` + `schedule.c`); exec triple
(`exec.c:38/62/130,156`); ptrace (`trace.c:42,256`); time quintet (`time.c`);
misc queries (`misc.c` + `profile.c:22` + `mcontext.c:13/23`); events mask
(`event.c:171`).

### 1.3 Order-difference table (runtime order vs teaching order)

Teaching order = numeric order 01→21 + 99 glossary. It follows T-init then T-loop,
so no structural inversion exists. Residual differences (all deliberate, each with
forward-pointer compensation):

| # | Runtime fact (anchor) | Teaching choice | Reason + compensation |
|---|---|---|---|
| D-1 | Signal sets are built at T-3 (`main.c:~150-165`) before any signal use | Full signal semantics deferred to 11/12/13 | T-3 is construction, not semantics; 01 names the three sets with one line each and points to 11; 99 indexes the constants |
| D-2 | `get_nice_value` is defined in `main.c:275-289` and called at T-6 | Its semantics live in 16 | Call-site timing (01) vs conversion math (16); 01 cites the call site, 16 owns both directions (`nice_to_priority`, `utility.c:91-103`) |
| D-3 | `do_getsetpriority` is defined in `misc.c:239-286` | Taught in 16, not 20 | Scheduling semantics outranks file co-location; 20's contract lists it as handed off |
| D-4 | Parallel families (13 credential calls, 6 sigaction calls, 10 misc queries) have no runtime sequence | Framework-first + grouped (15; 11→12→13; 20/21) | §4.3 grouping rule: one representative member taught deeply, rest in difference tables |
| D-5 | `sched_init` runs at T-8 | Semantics in 16 | Same split as D-2; 01 records the call point `sef_cb_init_fresh:241` and stops |

---

## 2. Knowledge Pool

Types: C=concept, M=mechanism, D=data structure, I=interface/protocol, V=constraint/
invariant, A=architectural evolution, T=tooling/engineering, S=test character.
Source: S=stock (exists in current docs) / N=new (gap found in §3; evidence-anchored,
not invented). "Home" = new-catalog doc that teaches it (§5 contract references K-IDs).

### 2.1 Startup and boot (T-init)

| ID | Name | T | Src | Current home(s) | Anchor | Reader benefit (answers…) |
|---|---|---|---|---|---|---|
| K-001 | SEF registration: init_fresh + STATEFUL restart + signal-manager callback | M | S | 01 | `main.c:115-126` | Why does PM survive RS restart, and where do kernel-origin signals enter? |
| K-002 | `sef_cb_init_fresh` 8-step sequence | M | S | 01 | `main.c:131-244` | What exact order does PM birth follow? |
| K-003 | boot_image fill loop (names, parents, classes) | M | S | 01 | `main.c:178-243`, `type.h:148-154` | Where does every initial process come from? |
| K-004 | VFS_PM_INIT handshake + ENDPT=NONE barrier | I | S | 01 (shake), 05 (vocab) | `main.c:~230-240` | How do PM and VFS agree on the initial table? |
| K-005 | monitor_params / sys_getmonparams + sys_getimage | I | S | 01 | `main.c:~165-178` | Where do boot parameters live? |
| K-006 | RS/INIT parentage + PRIV_PROC vs NONE classes | C | S | 01 | `main.c:178-243`, `forkexit.c:159` gate | Which processes are privileged from birth? |
| K-007 | system_hz + sched_init call points | M | S | 01 (sites), 14/16 (semantics) | `main.c:~241-244` | What clock and scheduler does PM start with? |
| K-008 | BootParams placeholder dependence (D-02) | V | S | 01 | `os/servers/pm/src/main.rs:15`, edge E6 | What part of boot still waits on kernel wrappers? |

### 2.2 Process model (mproc + table)

| ID | Name | T | Src | Current home(s) | Anchor | Reader benefit |
|---|---|---|---|---|---|---|
| K-010 | struct mproc field universe + 4-layer Rust model | D | S | 02 | `mproc.h:24-83`, `mproc/mproc.rs:334` | Where does each C field live in Rust? |
| K-011 | 19 flag bits → Lifecycle/Block/Wait enums + combinators (A-2) | A | S | 02 | `mproc.h:86-104`, `mproc/lifecycle.rs` | Why can't flags combine arbitrarily anymore? |
| K-012 | mpsigact independent table | D | S | 02 | `mproc.h` | Why is a child's handler table re-pointed, not copied? |
| K-013 | MP_MAGIC + generation anti-ABA | V | S | 02/03 | `mproc.h`, `utility.c:34-56` | How do stale endpoint references die? |
| K-014 | mproc[NR_PROCS] + procs_in_use accounting | D | S | 03 | `glo.h:16-23`, `main.c` fill loop | What bounds the process count? |
| K-015 | pm_isokendpt / find_proc validation | M | S | 03 | `utility.c:76-108` | Which callers are even allowed to speak? |
| K-016 | get_free_pid rotation + phase (increment-then-return) | M | S | 03 | `utility.c:34-56` | Why do PIDs rotate through 30000? |
| K-017 | endpoint/generation encoding; kernel-owned generation | C | S | 03/99 | `mproc.h`, kernel do_fork | Who owns which half of identity? |
| K-018 | NR_PROCS/NR_PIDS/INIT_PID/NO_EVENTSUB constants | C | S | 99 (index), 03 (use) | `const.h:20`, `pm.h:27` | What are the system's hard sizes? |
| K-019 | PmContext explicit params replacing file globals (A-3) | A | S | 03/04 | `glo.h:16-23`, `mproc/context.rs` | How does the borrow checker replace the BKL-era globals? |

### 2.3 Dispatch core

| ID | Name | T | Src | Current home(s) | Anchor | Reader benefit |
|---|---|---|---|---|---|---|
| K-020 | Main-loop decision order (notify→validate→discard→3 routes→conditional reply) | M | S | 04 | `main.c:59-110` | What does one PM millisecond look like? |
| K-021 | call_vec table → exhaustive match dispatch (A-5) | A | S | 04 | `table.c:14`, `ipc/calls.rs:212` | Where did the function-pointer table go? |
| K-022 | 47 call numbers PM_BASE+1..47 | I | S | 04 (table), 99 (index) | `callnr.h:9-62` | Which number means which request? |
| K-023 | ReplyIntent/SUSPEND 3 subclasses (wait4-later / VFS-later / never) | M | S | 04 (owns), 05/09/10 (cite) | `main.c:104-108`, `ipc/dispatcher.rs:49-57`, plan.md §7.3 | When is "no reply yet" correct? |
| K-024 | EXITING-process late-call discard | V | S | 04/09 | `main.c:82` | Why do dead processes go silent? |
| K-025 | CLOCK notify → expire_timers | M | S | 04 (route), 14 (work) | `main.c:65-70` | How do timers fire inside a message loop? |
| K-026 | SYSTEM notify → kernel-signal pull loop (batch H) | M | S | 04 (route), 11 (work) | `main.c:121`, `sef_signal` path, `signal.rs::process_sigmgr_signals` | How do kernel-accumulated signals reach PM? |
| K-027 | reply() persistent-payload reuse (mp_reply同型) | M | S | 04 | `main.c:249-270`, `init.rs:451` | Why is the reply buffer not per-request? |
| K-028 | ENOSYS guards (unregistered number, non-PM call, wrong replier) | V | S | 04 | `main.c:90-103`, `dispatcher.rs:89-114`, `calls.rs:991` | What does PM refuse, and how loudly? |
| K-029 | Typed IPC / wire codecs (A-4; Wait4/Ptrace landed, rest via E7) | A | S | 04 | `minix-types ipc/pm.rs`, `ipc/decode.rs:451` | Where is the union-to-types migration standing? |
| K-030 | **Wiring-status refresh: all 47 calls dispatched (S4), only unregistered numbers hit ENOSYS** | S | N | 04 | `ipc/calls.rs:594-990` arms + `:986-991` comment | Which handlers are truly reachable today? (replaces stale "7 wired" accounts) |

### 2.4 VFS protocol

| ID | Name | T | Src | Current home(s) | Anchor | Reader benefit |
|---|---|---|---|---|---|---|
| K-031 | tell_vfs 3-phase send (VFS_CALL set, async send, state continuation) | M | S | 05 | `utility.c:123-143` | How does PM ask VFS without deadlocking? |
| K-032 | handle_vfs_reply 11-arm state machine + tail restart_sigs | M | S | 05 | `main.c:294-424` | What happens when VFS answers? |
| K-033 | NEW_PARENT / UNPAUSED / VFS_CALL bits | V | S | 05 (owns), 02 (models) | `mproc.h`, `com.h:513-583` field map | How do forked/stopped processes cross the VFS round-trip? |
| K-034 | VFS_PM_* RQ 12 / RS 11 numbers + field layout | I | S | 05 | `com.h:513-583` | What is on the wire between PM and VFS? |
| K-035 | **REBOOT special path (sys_abort, return-to-loop HARD_STOP)** | M | N | 21 | `main.c:304-312`, `misc.c:199-238` | Why does reboot not reply like other calls? |
| K-036 | Core-dump path + pathname-pointer deferral (D-16) | V | S | 09/21 | `exit.rs:261/265`, `com.h` DUMPCORE | What is honestly still missing on coredump? |

### 2.5 Event subscription

| ID | Name | T | Src | Current home(s) | Anchor | Reader benefit |
|---|---|---|---|---|---|---|
| K-037 | subs[NR_SUBS=4] + NO_EVENTSUB=None encoding | D | S | 06 | `event.c:75-170` | How many watchers can exist, and what does "none" mean? |
| K-038 | do_proceventmask (mask/unmask + mut/non-mut cursor rule) | M | S | 06 | `event.c:171-217` | How does a subscriber enroll? |
| K-039 | publish_event 2 call sites (exit + signal paths) + VFS mutual exclusion | M | S | 06 | `event.c:316-353` | Who announces EXIT/SIGNAL, and when must it stay silent? |
| K-040 | resume_event → exit_restart / restart_sigs dispatch | M | S | 06 | `event.c:74-130` | How does a subscriber's answer resume the subject? |
| K-041 | PROC_EVENT_REPLY route + system-service-only replier gate | I | S | 06 (owns), 04 (routes) | `main.c:89`, `event.c:218-309` | Why can't ordinary processes answer events? |
| K-042 | Bounded serial-sync protocol (256 < 512 slots rationale) | C | S | 06 | `event.c`, `event.rs:1067` | Why is the bus serial instead of queued? |

### 2.6 Lifecycle (fork / srv_fork / exit / wait)

| ID | Name | T | Src | Current home(s) | Anchor | Reader benefit |
|---|---|---|---|---|---|---|
| K-043 | do_fork 9-step chain + 2-phase window (vm_fork = point of no return) | M | S | 07 | `forkexit.c:45-145` | Where can fork still fail, and where can it not? |
| K-044 | vm_fork fail-closed sendrec | V | S | 07 | `fork.rs:22/112`, `forkexit.c` VM call site | What if VM refuses the address space? |
| K-045 | mproc copy + sigact re-point + IN_USE/DELAY_CALL/TAINTED inherit | M | S | 07 (uses), 02 (models) | `forkexit.c:45-145` | What exactly does the child inherit? |
| K-046 | srv_fork 5 differences (RS-only, PRIV_PROC kept, 6-field credential injection, sync double reply) | M | S | 08 | `forkexit.c:146-245` | How are system services born differently? |
| K-047 | do_exit 9 steps with VFS_PM_EXIT barrier | M | S | 09 | `forkexit.c:246-417` | What must happen before a process may die? |
| K-048 | exit_restart (subscriber/VFS-deferred completion) | M | S | 09 | `forkexit.c:418-474` | How do deferred exits finish? |
| K-049 | zombify 2-level (ZOMBIE / TRACE_ZOMBIE) + TOLD_PARENT | D | S | 09 (produce), 10 (consume) | `forkexit.c:594-669` | What does "dead but not reaped" mean, twice? |
| K-050 | check_parent SIGCHLD delivery | M | S | 09/10 | `forkexit.c:670-730` | How does the parent learn? |
| K-051 | disinherit → INIT adoption + SIGHUP session broadcast | M | S | 09 | `forkexit.c:731-806` | Who raises orphans? |
| K-052 | tracer_died / NEW_PARENT guardianship transfer | M | S | 09/10/18 | `forkexit.c:760-806`, `mproc/guardianship.rs` | What happens when the tracer dies first? |
| K-053 | TRACE_EXIT bit (single C bit, dual-bool Rust history) | V | S | 09/18 | `mproc.h:100` | Which exit does the tracer want to see? |
| K-054 | Privileged-process direct destroy (deadlock guard) | V | S | 09 | `forkexit.c:246-266` | Why do system processes skip the zombie state? |
| K-055 | wait4: 4 pidarg selector states × 3 rings (ZOMBIE/TRACE_ZOMBIE/TRACE_STOPPED) | M | S | 10 | `forkexit.c:475-593` | Whom can wait4 reap? |
| K-056 | tell_parent / tell_tracer + wait4 reply payload contract (tag + typed body) | I | S | 10 | `forkexit.c:670-730`, Fix D-26 | Where does the exit code travel? |
| K-057 | rusage accumulation + vircopy delivery to parent | M | S | 10 | `utility.c:144-156`, `wait.rs:404` | How is a child's CPU time billed? |
| K-058 | cleanup: procs_in_use-- + slot release (pairs fork's ++) | M | S | 10 (owns), 07 (cites) | `forkexit.c:760-806` | When is the slot truly free? |
| K-059 | exec-signals interplay: catch reset, TAINTED double test | M | S | 17 (owns), 11/12 (cite) | `exec.c:38-200`, `signal.c:179-182` | What does exec do to signal state? |

### 2.7 Signals (core / handlers / flow)

| ID | Name | T | Src | Current home(s) | Anchor | Reader benefit |
|---|---|---|---|---|---|---|
| K-060 | do_kill 4 pid states + permission gate + broadcast | M | S | 11 | `signal.c:197-266` | Who may signal whom? |
| K-061 | srv_kill RS variant | M | S | 11 | `signal.c:207-225` | How do system services signal? |
| K-062 | check_sig gate (perm + lethality + state) | M | S | 11 | `signal.c:568-651` | Is this signal even deliverable? |
| K-063 | sig_proc 9-decision chain (TRACE-first, VFS/EVENT-pend+stop, caught→unpause→send, terminate) | M | S | 11 | `signal.c:384-545` | What is the single priority order for every signal? |
| K-064 | process_ksig pull loop + SIGSNDELAY tail (DELAY_CALL resume) | M | S | 11 | `signal.c:294-383` | How are kernel-origin signals drained? |
| K-065 | sig_proc_exit (lethal path into exit_proc) | M | S | 11 (owns), 09 (receives) | `signal.c:546-567` | How does a signal become an exit? |
| K-066 | sigaction / sigprocmask / sigpending / sigsuspend / sigreturn | I | S | 12 | `signal.c:40-196` | What is the user-visible handler API? |
| K-067 | SA_* flags + mask/mask2 (suspend-save) pairing | D | S | 12 | `signal.c:40-196,776-855` | How do temporary masks nest? |
| K-068 | IGN/DFL/CATCH asymmetry + KILL/STOP unkillable stripping (5 sites) | V | S | 12 | `signal.c` stripping sites | Which dispositions can never be caught? |
| K-069 | sig_send 4-step (sigmsg build + sys_sigsend + EINTR break + fallback terminate) | M | S | 12 | `signal.c:776-855` | How does a caught signal reach user stack? |
| K-070 | core/ign/noign set construction | D | S | 11 (build use), 99 (index) | `main.c:~150-165` | Which signals die / drop / deliver by default? |
| K-071 | stop_proc / try_resume_proc (MustStop vs MayDefer) | M | S | 13 | `signal.c:226-293` | How is a process frozen and thawed? |
| K-072 | unpause 3-path (UNPAUSED-ready / DELAY-busy / stop-then-ask-VFS) | M | S | 13 | `signal.c:720-775` | How does a VFS-blocked process become signal-ready? |
| K-073 | check_pending / restart_sigs + PROC_STOPPED dual use (stop flag + recheck hint) | M | S | 13 | `signal.c:652-719` | Who re-examines pending signals, and when? |
| K-074 | DELAY_CALL / SIGSNDELAY kernel deferred-call contract (A-10, deferred) | V | S | 13 | `signal.c`, kernel `system.c:463` | What can't PM do until the kernel helps? |
| K-075 | EBUSY→DELAY_CALL→SIGSNDELAY causal chain | C | S | 13 | `signal.c:226-293` | Why do three names describe one "busy" idea? |

### 2.8 Timers, credentials, scheduling

| ID | Name | T | Src | Current home(s) | Anchor | Reader benefit |
|---|---|---|---|---|---|---|
| K-076 | itimer 3 families (REAL/VIRTUAL/PROF) | C | S | 14 | `alarm.c:93-160` | What three clocks can a process set? |
| K-077 | timeval↔ticks round-up + clamp + sanity | M | S | 14 | `alarm.c:33-92` | How is wall time converted to ticks without overflow? |
| K-078 | set_alarm / check_vtimer / cause_sigalrm chain | M | S | 14 | `alarm.c:222-344` | What path does an expired timer take to SIGALRM? |
| K-079 | ALARM_ON (REAL only) vs sys_vtimer backend (VIRTUAL/PROF) | V | S | 14 | `alarm.c:160-221` | Why do virtual timers not use the PM queue? |
| K-080 | 13 credential calls: full-set vs single-set + SUPER_USER gate | I | S | 15 | `getset.c:19-223` | Which identity changes need root? |
| K-081 | Credentials triple (real/eff/saved × user/group) + supplemental groups ≤16 | D | S | 15 | `getset.c`, `mproc/credentials.rs:201` | What is the full identity state? |
| K-082 | TAINTED single true source | V | S | 15 (owns), 07/17 (cite) | `getset.c`, `exec.c` setuid sites | When is a process permanently untrustworthy? |
| K-083 | VFS dual copy (SETUID/SETGID/SETSID/SETGROUPS forward + SUSPEND close-loop) | I | S | 15 (owns), 05 (carries) | `getset.c:95-223`, `com.h` SET* RQ/RS | Why does VFS keep its own copy of uid/gid? |
| K-084 | setsid/setpgrp/getsid + session-leader = pid rule | M | S | 15 | `getset.c`, `forkexit.c` exit_proc use | How do sessions and groups work? |
| K-085 | sched_init / sched_start_user / sched_nice + KERNEL/NONE guard | M | S | 16 | `schedule.c:20-112` | How does PM hand a process to the scheduler? |
| K-086 | nice -20..20 ↔ queue 0..15 dual conversion (16:41 linear + 1→2 quantization note) | M | S | 16 | `utility.c:91-103`, `main.c:275-289` | Why do two functions convert in opposite directions? |
| K-087 | do_getsetpriority (file-misc, semantic-sched) | M | S | 16 | `misc.c:239-286` | Where do get/setpriority really belong? |
| K-088 | SCHED client protocol (start/inherit/stop/nice via _taskcall; A-8) | I | S | 16 | `minix/sched.h`, `schedule.c` | What does PM ask the scheduler service? |

### 2.9 Exec, ptrace, time, misc/control

| ID | Name | T | Src | Current home(s) | Anchor | Reader benefit |
|---|---|---|---|---|---|---|
| K-089 | do_exec / do_newexec / do_execrestart + exec_restart 3-stage | M | S | 17 | `exec.c:38-200` | What are the three doors into a new program? |
| K-090 | PARTIAL_EXEC flag + VFS/RS caller gates | V | S | 17 | `exec.c:38-130` | How is a half-finished exec represented? |
| K-091 | Credential check split: VFS-side permission, PM-side identity | C | S | 17 | `exec.c:38-62` | Who checks what during exec? |
| K-092 | Tracer-first: SIGTRAP/STOP before address-space switch | M | S | 17/18 | `exec.c`, `trace.c` | How does the tracer stay in control across exec? |
| K-093 | frame save (stack_high - len) | D | S | 17 | `exec.c:62-130` | Where does the new stack image wait? |
| K-094 | 18 T_* constants ABI (ptrace.h truth; corrects old doc/code drift) | I | N | 18 | `sys/sys/ptrace.h:226-250` | What does each ptrace request number really mean? |
| K-095 | ATTACH (TO_NOEXEC + real SIGSTOP) / DETACH (replay→signal→check_pending→kernel passthrough) | M | S | 18 | `trace.c:56-215` | How are tracing relationships made and unmade? |
| K-096 | RANGE read/write (datacopy range checks, TS_INS/TS_DATA) | M | S | 18 | `trace.c:167-188` | How is bulk peer-memory access bounded? |
| K-097 | INS/DATA peek/poke root gate position (before generic guard) | V | S | 18 | `trace.c:101-143` | Why is the root check ordered first? |
| K-098 | trace_stop + wait4 status-payload contract (W_STOPCODE 0x7f) | I | S | 18 (produces), 10 (consumes) | `trace.c:256-276` | How does a traced stop reach the tracer's wait4? |
| K-099 | sys_trace passthrough + error fidelity (no EINVAL folding) | V | S | 18 | `trace.c:220-249` | Which errors must pass through untouched? |
| K-100 | 5 time calls + REALTIME vs MONOTONIC dispatch | I | S | 19 | `time.c:22-131` | Which clock can be set, which can only be read? |
| K-101 | boottime + hz arithmetic without overflow | M | S | 19 | `time.c`, `main.c` system_hz | How is uptime converted safely? |
| K-102 | uname/uts + compatibility-block note | D | S | 20 | `misc.c:33-100` | What does PM report about itself? |
| K-103 | getsysinfo ProcTab C-ABI wire image (D-29 closed) | I | N | 20 | `misc.rs` do_getsysinfo arm, `mproc/wire.rs:201` | How does the whole process table leave PM? |
| K-104 | getprocnr / getepinfo (ngroups + groups copy-out) | I | S | 20 | `misc.c:149-198` | How are slot/pid/endpoint translated for callers? |
| K-105 | getrusage copy-out (consumer of K-057) | I | S | 20 | `misc.c:401-446` | How does resource usage reach the asker? |
| K-106 | svrctl 4 branches + check order | M | S | 21 | `misc.c:291-400` | What control operations exist, in what precedence? |
| K-107 | sprofile + SPROFILE-off ENOSYS default | V | S | 21 | `profile.c:22-45` | When is profiling honestly unavailable? |
| K-108 | mcontext get/set (machine context pair) | I | S | 20 | `mcontext.c:13-27` | Where is register state saved? |
| K-109 | calls_stats / ENABLE_SYSCALL_STATS cfg feature | T | S | 20 | `misc.c:64,131-132`, `main.c:35,96` | How is call counting optionally compiled in? |
| K-110 | find_param monitor KVP linear scan | T | S | 20 | `utility.c:57-75` | How are boot key=value pairs looked up? |
| K-111 | **ESCRIPT dead-code exclusion (C #define, zero uses)** | V | N | 20 | `exec.c:31` + zero-use grep | What must Rust NOT implement? |

### 2.10 Cross-cutting (evolution, errors, process)

| ID | Name | T | Src | Current home(s) | Anchor | Reader benefit |
|---|---|---|---|---|---|---|
| K-112 | ARCH A-1..A-13 index with per-doc landing | A | S | 00 (index), each doc (its rows) | plan.md §4 | Where did each Minix3→Rust evolution land? |
| K-113 | KernelGateway central + 4-column ability table (ability↔trait↔wrapper↔kernel peer) | A | S | 04 (route), 00 (index), plan.md §4.1 | `exit.rs` gateway, `minix-sys syscall.rs` | Where does every kernel capability enter PM? |
| K-114 | Error-fidelity rule: passthrough errors stay Kernel(i32); only PM's own judgments get semantic variants | V | S | 04 (rule), applied in 16/18/20 | plan.md §4.2 | Which errnos are observable contracts? |
| K-115 | Single-threaded event loop: &mut ProcTable as compile-time lock; no SMP code in PM | C | S | 04 | `mproc/context.rs`, `init.rs` run_once | What concurrency model does PM assume? |
| K-116 | Shutdown/exit barrier ordering (VFS first, VM announced, scheduler stopped, table freed last) | C | N | 21 (owns), 09 (exit side) | `forkexit.c:246-417`, `misc.c:199-238`, `main.c:304-312` | What order does teardown follow? |
| K-117 | Test character: production-path tests vs seam tests; the mock-leak lesson (V2-P0) | S | S | 04 (rule), each doc §5 | todo.md §11.4-L6 | Which tests prove behavior, which prove seams? |
| K-118 | 64-bit type mapping (Pid/Uid/Gid/Clock/Endpoint/UserSlot, A-11) | A | S | 02/03/99 | `minix-types`, `mproc/constants.rs:53` | Where did platform-sized C types go? |

Pool statistics: 101 entries (S=94 stock, N=7 new: K-030, K-035, K-094, K-103, K-111,
K-116, plus K-008's E6 remainder). By type: C=10, M=45, D=11, I=18, V=14, A=5, T=4,
S=1 (K-117; per-doc test matrices live in §5 contracts, not the pool).
By current doc: 01:8, 02/03:10, 04:11, 05:6, 06:6, 07–10:17, 11–13:16, 14:4, 15:5,
16:4, 17:5, 18:6, 19:2, 20/21:10, 99/00:6 (shared constants + ARCH index).
Every one of the 15 .c files and 6 local .h files owns at least one entry (G2 input).

---

## 3. Coverage Audit

### 3.1 Theme universe (4 provenances)

1. C symbols: 109 enumerated by `tools/coverage-extract/coverage-extract.py pm`
   (doc-covered 109/109 = 100%; Rust name-match 102/109 = 93.6%, 7 non-matches all
   adjudicated non-gaps: NO_EVENTSUB semantically expressed, SEND_PRIORITY/
   SEND_TIME_SLICE pending on E7, ESCRIPT dead code correctly absent,
   EXTERN/_SYSTEM/_TABLE compile macros need no counterpart — todo.md §11.1.2/§12.0).
2. OS-theory concepts: process identity/lifecycle, signal disposition model, session/
   group semantics, timer families, credential model, scheduler-class separation,
   ptrace attach model, monotonic vs realtime clocks, table-mirror protocols.
3. Non-C artifacts (§3.4 fixed list).
4. Boundary contracts: 00-master-plan README (PM loaded by RS; fork = sub-mainline),
   edge_todo E1/E5/E6/E7/E9, plan.md §5.4 exclusions, todo.md batch table A–H.

### 3.2 Coverage-gap table (each row becomes N-pool entries + a §5 contract home)

| # | Gap | Evidence anchor | Disposition |
|---|---|---|---|
| G-1 | Wiring-status staleness: plan.md §6.1-era "7 wired / 40 ENOSYS" and 00 §3.1 "8 wired + batches A–G" no longer describe the dispatcher, which now carries explicit arms for all 47 registered calls (`calls.rs:594-990`, catch-all only at `:991`) | `ipc/calls.rs:212-991` vs `00-pm-overview.md:65`, `todo.md §11.1.1` | Refresh in place: 04 owns the live wiring table (K-030); 00 cites it; batch table retained as history of how wiring was reached, explicitly marked historical |
| G-2 | Doc 18's TO_NOEXEC value contradicts C (`0x1` vs `ptrace.h:211 0x4`); T_* ABI drift risk in prose | `sys/sys/ptrace.h:211,226-250` vs `18-trace.md §2.14` | Fix in place in 18 (K-094); add full-ABI assertion test to acceptance |
| K-3 (G-3) | 05 too thin (349 lines, thinnest mechanism doc) for an 11-arm reply state machine + 12/11-number protocol + 3-phase send | `wc -l` + `main.c:294-424` + `com.h:513-583` | Expand 05 to ~600–700 lines: one subsection per reply arm + field-layout table (K-031..K-034) |
| G-4 | 99 too thin (93 lines) for its "authoritative definition of every magic number" promise; endpoint encoding, three signal sets, 7 globals, 47-number index all under-defined | `99-global-concepts.md` vs its own header claim | Expand 99 to ~300–400 lines: constant radius table + encoding diagram + set semantics + global-seven + call index (K-017/K-018/K-070) |
| G-5 | 00 too thin (96 lines) as entry + status board: wiring summary stale (G-1), no ARCH landing map, no reading-path skip marks | `00-pm-overview.md` | Revise 00 to ~250–350 lines: role boundary, layer map, live status (pointer to 04), ARCH landing index, 23-doc nav with skip marks |
| G-6 | 20 over-broad: 10+ unrelated syscalls (uname/sysinfo/procnr/epinfo/reboot/svrctl/rusage/sprofile/mcontext/stats/params) in 522 lines; reboot/svrctl/sprofile are control-plane, the rest are info-plane | `misc.c` + `profile.c` + `mcontext.c` vs single-doc rule §5.1(4) | Split 20 → 20 (info queries) + 21 (new: control & shutdown) |
| G-7 | Shutdown/teardown ordering has no home: REBOOT special path (`main.c:304-312`), reboot sequence (`misc.c:199-238`), coredump deferral (D-16), exit barrier (09) are scattered | anchors in K-116 row | New doc 21 owns K-035/K-106/K-107/K-116; 09 cites it for the exit side |
| G-8 | Code comments cite `.design/` snapshots (e.g. `event.rs:10`, `init.rs` design refs, minix-types `vfs.rs:313` "05-design.v1.md D2") — intermediate artifacts referenced from production code | `rg` §8.3 (3+ sites) | Code hygiene, not doc scope: 21/§8 lists the sites; B-phase files a code-cleanup note (replace with `NN-name.md §X` formal refs). Formal docs never cite `.design/` (this blueprint doesn't) |
| G-9 | A-10 DELAY_CALL/SIGSNDELAY has code-deferral but the doc contract (fail-closed semantics) is thin in 13 | `signal.c` + kernel `system.c:463` | Harden 13's contract: explicit deferred-call semantics section (K-074/K-075) |

### 3.3 Duplicate-theme table (one home, rest cite)

| Theme | Occurrences | Home | Others become pointers |
|---|---|---|---|
| get_free_pid | 01 (birth use), 03 (define), 07 (fork use) | 03 | 01/07 cite K-016, no re-derivation |
| Signal-set construction (core/ign/noign) | 01 (build step T-3), 11 (semantics), 99 (index) | 11 owns semantics; 99 owns numbers | 01 keeps 3 one-liners + pointer |
| VFS_PM_INIT | 01 (handshake event), 05 (protocol vocab) | 01 owns the event; 05 owns the message format | Cross-cite, no double state machine |
| ReplyIntent/SUSPEND | 04 (owns), cited by 05/07/09/10/11/12/15 | 04 | All citers use K-023 by reference + their own subclass only |
| rusage accumulate vs getrusage copy-out | 10 (produce K-057), 20 (consume K-105) | Split at the vircopy boundary | 20 cites 10's accounting, teaches only the query path |
| TAINTED source | 15 (owns K-082), used by 07/17 | 15 | 07/17 cite, never re-argue |
| sched_init call site vs semantics | 01 (T-8 site), 16 (owns) | 16 owns; 01 records site + stops | Already clean; contract locks it |
| mproc lifecycle enums | 02 (owns), used by 09/10/11/13 | 02 | Users cite K-011 + their own transitions only |
| Endpoint/generation | 03 (owns K-017), 99 (indexes) | 03 owns mechanics; 99 owns numbers | One diagram lives in 03; 99 reproduces the table, not the diagram |

### 3.4 Out-of-scope theme table (correct home each)

| Theme taught elsewhere | Correct home | PM's obligation (one pointer section) |
|---|---|---|
| sys_* kernel implementations (kill/getksig/endksig/sigsend/sigreturn/times/clear/trace/vircopy/runctl) | 01-stage-kernel (19-syscall-signal et al.) | 04/11/12 cite peer + teach only the PM-side call shape (K-113 table) |
| vm_fork/vm_willexit/vm_exit/vm_getrusage implementations | 02-stage-vm (18-vm-fork et al.) | 07/09/10 teach the PM-side protocol half only |
| VFS peer reply-side implementations | 05-stage-vfs | 05 teaches PM-side state machine only (K-032) |
| Service supervision / reincarnation / live-update | 03-stage-rs | 01/21 cite RS_UP/DOWN as PM observes them |
| Trap transport + SYS_* wrappers not yet landed | edge E1/E6 | 01 (D-02) and K-113 mark the seam; no PM doc re-explains trap mechanics |
| Wire-type systematization remainder | edge E7 | 04 (K-029) marks landed vs pending; per-doc contracts consume wire as prerequisite, never redefine it |
| ESCRIPT `#!` self-read relic | nowhere (C dead code, zero uses) | 20 records the exclusion with the zero-use grep (K-111) |

### 3.5 Non-C themes: fixed list of 10, each answered

| Non-C theme | Found artifact | Home or principled exclusion |
|---|---|---|
| Link & load | `pm/Makefile` (PROG=pm, objects, `-lsys -ltimers`) | 01 §"build identity": what is linked into the PM image; no new doc (5-line treatment) |
| Image & memory layout | `boot_image` (`type.h:148-154`, `kernel/table.c:44`, `kernel/main.c:64,165`) | 01 (T-0/T-5): struct + who populates; 99 indexes NR_BOOT_PROCS |
| ASM entry & trap entry | None PM-specific (PM is a user-space server; traps belong to kernel) | Excluded with reason; 01 states it in one line + points to 01-stage-kernel |
| Boot assembly | None PM-specific (same reason) | Excluded with reason; 01 one-liner |
| Build & toolchain | `pm/Makefile` + minix.service.mk | 01 §"build identity" (same as link&load — they are one 15-line section) |
| Cross-module iface & wire format | `com.h` VFS_PM_* + `callnr.h` PM_* + `minix-types` codecs | 05 (VFS wire, K-034) + 04 (PM-call wire, K-022/K-029); the two wire tables live exactly here |
| Error paths | ReplyIntent taxonomy + per-call errno tables + fidelity rule | 04 owns the rule (K-023/K-114); every mechanism doc carries its errno table; 21 owns shutdown errors |
| Shutdown & exit | REBOOT path + reboot sequence + coredump deferral + exit barrier | New doc 21 (K-116); 09 keeps the per-process exit, cites 21 for system teardown |
| Concurrency & sync | Single-threaded loop; `&mut ProcTable` compile-time lock; no SMP in PM | 04 (K-115), ~20 lines; explicitly contrasts with kernel SMP/BKL so readers stop looking for locks |
| Test infrastructure | `cargo test -p minix-pm` (lib + integration run_once), seam vs production-path discipline | 04 owns the discipline (K-117); each doc §5 carries its module matrix; 00 carries the live baseline pointer |

---

## 4. New Catalog

23 docs. Rule: **numbers are stable** (history lesson: renumbering breakage cost >
benefit). Exactly one split (20→20+21), zero merges, zero renames, three expansions
(00/05/99), one content refresh with a structural addition (04 gains the live wiring
table + concurrency/error sections).

### 4.1 New-doc table (number, title, one-line placement, group)

| New # | Title | Placement | Group |
|---|---|---|---|
| 00 | pm-overview: role boundary, layers, status, reading map | Entry; revised per G-5 | A entry |
| 01 | pm-init-main: birth chain T-0..T-8 | Unchanged number; +build-identity §, +exclusion one-liners | B birth |
| 02 | mproc-struct: the process record | Unchanged; lock K-010..K-013 wording | C model |
| 03 | mproc-table: slots, endpoints, PIDs | Unchanged; owns K-014..K-017 | C model |
| 04 | ipc-dispatch: loop, routes, reply intent, wire, errors, concurrency | Unchanged number; +live wiring table (G-1), +concurrency § (K-115), +error-fidelity § (K-114) | D loop |
| 05 | vfs-interaction: the VFS protocol, fully spelled | Expanded ~350→650 lines (G-3) | E shared protocols |
| 06 | event-subscription: the watcher bus | Unchanged | E shared protocols |
| 07 | pm-fork: birth of one process | Unchanged; cite-only K-016/K-082 | F lifecycle |
| 08 | pm-srv-fork: birth of a system service (skippable) | Unchanged diff-doc | F lifecycle |
| 09 | pm-exit: death with a barrier | Unchanged; cites 21 for teardown | F lifecycle |
| 10 | pm-wait: reaping + billing | Unchanged | F lifecycle |
| 11 | signal-core: generation and delivery | Unchanged | G signals |
| 12 | signal-handlers: the user API | Unchanged | G signals |
| 13 | signal-flow: stopping, pending, resuming (+deferred-call contract) | Unchanged number; +A-10 section (G-9) | G signals |
| 14 | itimer: three clocks over one queue | Unchanged | H services |
| 15 | credentials: identity in triplicate | Unchanged; owns TAINTED | H services |
| 16 | scheduling: classes, queues, nice | Unchanged; owns getsetpriority | H services |
| 17 | exec: three doors into a new program | Unchanged | H services |
| 18 | trace: ptrace without illusions (ABI-corrected) | Unchanged number; fix TO_NOEXEC + T_* table (G-2) | H services (skippable) |
| 19 | time: two clocks, five calls | Unchanged | H services |
| 20 | info-queries: uname, sysinfo, procnr, epinfo, rusage, mcontext, stats, params (narrowed) | Split remainder; control calls move to 21 | I info (skippable details) |
| 21 | control-and-shutdown: reboot, svrctl, sprofile, teardown order (NEW) | New; owns K-035/K-106/K-107/K-116 | J control (skippable) |
| 99 | global-concepts: the vocabulary (expanded index) | Expanded ~93→350 lines (G-4) | K glossary (read first or as needed) |

### 4.2 Reading paths

- Mainline (must): 00 → 99 (skim) → 02 → 03 → 01 → 04 → 05 → 06 → 07 → 09 → 10 →
  11 → 15 → 17 → 20 (skim).
- Branch signals-deep: 11 → 12 → 13 → 14.
- Branch services: 16 → 18 → 19 → 21.
- Skippable on first pass: 08 (RS-only fork), 12 (handler API details), 13 (delay
  machinery), 14 (timers), 18 (ptrace), 21 (control plane), 20 subsections
  (mcontext/stats/params).
- Reference only: 99 (look up numbers, never read end-to-end twice).

### 4.3 Parallel-family organization (§5.2 rule applied)

- Credentials (13 calls, `getset.c`): 15 opens with the triple+gate framework (K-080/
  K-081), then a difference table: full-set (SETUID/SETGID/SETEUID/SETEGID/SETGROUPS)
  vs single-set vs pure-query (GET*); sessions (SETSID/GETSID/GETPGRP) as one group;
  representative member taught deeply: `do_set` for the VFS round-trip, `do_get` for
  the query shape.
- Sigaction family (6 calls): 11 first (delivery framework K-063), 12 groups by
  install (sigaction) / mask (procmask/pending) / suspend-resume (suspend/return);
  representative: sigaction; rest by delta.
- Time quintet: 19 opens with the two-clock framework (K-100), then per-call deltas
  in one table.
- Info queries (20): one subsection per call, each ≤60 lines, identical skeleton
  (gate → work → copy-out → errors); representative: getsysinfo (K-103, the only one
  with a wire image).
- VFS replies (11 arms): 05 gives each arm its own subsection — this is the G-3
  expansion; representative: FORK_REPLY (touches NEW_PARENT + sched + reply), rest by
  delta against it.

---

## 5. Per-Document Contracts (all 23; seven elements each — no doc may ship without them)

### 00-pm-overview

- Placement: the only doc with no prerequisites; every other doc points here for
  "why PM exists".
- Covers: K-112 (ARCH landing index), K-113 (gateway pointer), K-117 (baseline
  pointer), role boundary (4 authorities: table owner / signal manager / lifecycle
  orchestrator / bookkeeper), layer map (state → logic → dispatch → transport →
  skeleton), 23-doc nav with skip marks, live status board (test baseline + wiring
  pointer to 04, refreshed per release — never a frozen count).
- Non-covers: every mechanism (hand to its doc); kernel/VM/VFS peer internals
  (hand to their stages + edge list); trap mechanics (E1).
- Prerequisites: none. Postrequisites: 99, 02, 03 reference its boundary.
- Ground truth: `servers/pm/` file→doc map table; `os/servers/pm/src/` module mirror;
  `callnr.h` count (47); test baseline command `cargo test -p minix-pm`.
- Knowledge rows: K-112 (A, plan.md §4, why here: single ARCH landing map) [S: plan
  §4]; K-113 (A, gateway file, why: index not plumbing) [S: 00 current §4.1];
  K-117 (S, todo.md §11.4-L6, why: baseline discipline stated once) [S: 00 §3.3].
- Acceptance: a new reader can answer "what is PM, what is it not, what do I read
  next for X" for X in {fork, signal, timer, credential, exec, ptrace}; zero
  mechanism paragraphs; all counts either live pointers or dated snapshots.

### 01-pm-init-main

- Placement: first mechanism doc; answers "how is PM born".
- Covers: K-001..K-008 (T-0..T-8, SEF cbs, fill loop, handshake event, hz/sched sites,
  boot placeholders) + build-identity section (Makefile link set, service image) +
  three exclusion one-liners (asm entry / boot asm / trap → kernel stage).
- Non-covers: mproc field semantics (→02), slot/PID mechanics (→03), loop routes
  (→04), reply state machine (→05), signal-set semantics (→11, K-070), nice math
  (→16), sched_init semantics (→16).
- Prerequisites: 00; `01-stage-kernel/06-proc-init-boot-proc.md` (boot-image source).
  Postrequisites: 02/03 (model built here, defined there), 04 (loop entry), 16 (T-8).
- Ground truth: `main.c:49-56,115-244`; `type.h:148-154`; `kernel/table.c:44`;
  `kernel/main.c:64,165`; `pm/Makefile`; `libsys/sef.c:68`.
- Knowledge rows: K-001..K-008 as above (all M/I/V, stock from current 01).
- Acceptance: reader can replay T-0..T-8 with file:line for each; can state which
  two init steps are deferred (D-02 boot params, D-12 sched client) and their edge
  owners; build-identity section ≤25 lines.

### 02-mproc-struct

- Placement: the record everything else indexes.
- Covers: K-010..K-013, K-118 (64-bit mapping of its fields), K-011 transition
  discipline (which enum moves where — the moves themselves belong to users).
- Non-covers: slot allocation (→03), any lifecycle transition logic (→07/09/10),
  signal delivery (→11–13), credential checks (→15).
- Prerequisites: 01 (birth context). Postrequisites: 03, 07, 09, 10, 11, 15 cite its
  layers; none restate them.
- Ground truth: `mproc.h:24-104`; `mproc/mproc.rs` (+§4 mapping table);
  `mproc/{lifecycle,block,wait,guardianship,trace,signal,credentials}.rs`.
- Knowledge rows: K-010 (D), K-011 (A), K-012 (D), K-013 (V), K-118 (A).
- Acceptance: field→layer table complete vs `mproc.h`; every flag bit maps to
  exactly one enum/combinator; MP_MAGIC + generation story in one place.

### 03-mproc-table

- Placement: the index view over the record (slot/endpoint/PID have different
  lifetimes — this doc's thesis).
- Covers: K-014..K-017, K-019 (explicit context params at table call sites).
- Non-covers: slot consumers (→07/09/10), endpoint generation by kernel (cited, not
  taught), field semantics (→02).
- Prerequisites: 01, 02. Postrequisites: 04 (validation call site), 07/09/10 (slot
  discipline), 18 (ESRCH paths).
- Ground truth: `glo.h:16-23`; `utility.c:34-56,76-108`; `mproc/table.rs`,
  `pid_gen.rs`, `context.rs`; `const.h` sizing.
- Knowledge rows: K-014 (D), K-015 (M), K-016 (M), K-017 (C), K-019 (A).
- Acceptance: reader can explain why slot, endpoint, and PID are three clocks;
  PID rotation phase stated with the increment-then-return anchor; generation
  ownership (kernel) explicit.

### 04-ipc-dispatch

- Placement: the heart; every service doc's "how do I get called" answer.
- Covers: K-020..K-030 plus K-113 (gateway table, route view), K-114 (fidelity rule),
  K-115 (concurrency model), K-117 (test discipline), K-029 (codec status).
- Non-covers: VFS reply work (→05), event reply work (→06), all 40+ handler bodies
  (→07–21), kernel peer internals (cited).
- Prerequisites: 01, 02, 03. Postrequisites: all of 05–21 cite its routes/intent.
- Ground truth: `main.c:59-110,249-270`; `table.c:14`; `callnr.h:9-62`;
  `ipc/calls.rs:212,594-991`; `ipc/dispatcher.rs:89-114`; `init.rs` run_once/reply;
  plan.md §7.3 (SUSPEND subclasses), §4.2 (fidelity).
- Knowledge rows: K-020..K-030 (M/A/I/V/S mix), K-113/K-114/K-115/K-117.
- Acceptance: live wiring table (47 rows: call → C handler → Rust arm → wire/wrapper
  prerequisites) with generation date; three SUSPEND subclasses exemplified
  (wait4-later, VFS-later, never); ENOSYS cases enumerated (3); concurrency section
  states the single-threaded assumption + what would break if violated.

### 05-vfs-interaction (expanded)

- Placement: the first shared protocol; every suspending service depends on it.
- Covers: K-031..K-034 (3-phase send, 11-arm machine with per-arm subsections, bit
  vocabulary, wire numbers + field map) + the INIT-format subsection (format only;
  event owned by 01) + the two publish_event pre-return mutual-exclusion notes.
- Non-covers: INIT event semantics (→01), any service's business logic (→07–21),
  VFS peer internals (cited), event bus (→06).
- Prerequisites: 01, 02, 03, 04. Postrequisites: 07/08/09/15/17 cite its arms.
- Ground truth: `main.c:294-424`; `utility.c:123-143`; `com.h:513-583`;
  `ipc/vfs.rs:190-282,302-304`.
- Knowledge rows: K-031 (M), K-032 (M), K-033 (V), K-034 (I).
- Acceptance: 11 reply arms each get a subsection (trigger → state change → reply/
  SUSPEND → tail effect); field-layout table covers every m7_* slot used; the
  FORK_REPLY representative treatment lets other arms be deltas; target 600–700
  lines;ff. current thinnest-doc gap closed.

### 06-event-subscription

- Placement: the second shared protocol (watcher bus).
- Covers: K-037..K-042 (table, mask incl. mut-cursor rule, publish sites +
  VFS-exclusion, resume dispatch, reply route + replier gate, bounded-serial
  rationale).
- Non-covers: exit/signal business logic that publishes (→09/11), DS/kernel queues
  (cited as non-goals), VFS machine (→05).
- Prerequisites: 04, 05, 02, 03. Postrequisites: 09/10/13 cite resume paths.
- Ground truth: `event.c:74-353`; `event.rs:164-557`; `syslib.h:292-293` (2 event
  types); `dispatcher.rs` PROC_EVENT_REPLY arm.
- Knowledge rows: K-037..K-042.
- Acceptance: reader can enroll, publish, answer, and resume one EXIT and one
  SIGNAL event end-to-end; serial-sync bound argued with the slot arithmetic;
  non-mut cursor rule stated as invariant, not advice.

### 07-pm-fork

- Placement: lifecycle opens; the most cross-server dance in PM.
- Covers: K-043..K-045, K-058-cite (cleanup pairing), two-phase window diagram,
  VFS_SUSPEND child-hang + NEW_PARENT protection, tracer-SIGSTOP hook (cite 11/18).
- Non-covers: VM-side copy (→02-stage-vm/18), VFS-side fd work (peer stage),
  srv_fork deltas (→08), sched start details (→16), PID mechanics (cite 03),
  TAINTED source (cite 15).
- Prerequisites: 03, 04, 05, 02, 06. Postrequisites: 08 (diff base), 09 (slot
  pairing), 10 (wait side).
- Ground truth: `forkexit.c:45-145`; `utility.c:34-56` (use); `fork.rs:22/112`;
  `mproc/fork.rs`; `com.h` FORK RQ/RS.
- Knowledge rows: K-043 (M), K-044 (V), K-045 (M).
- Acceptance: 9-step chain with the no-return point marked; failure-vs-rollback
  table for pre/post-vm_fork; reply matrix (parent gets pid, child gets OK,
  suspended-child case ReplyLater).

### 08-pm-srv-fork (skippable)

- Placement: the RS-only delta doc; reads only after 07.
- Covers: K-046 (5 differences), credential 6-field injection table, sync double
  reply vs SUSPEND contrast.
- Non-covers: ordinary fork (→07, by reference), VM/VFS halves (peers), scheduling
  (→16), signals (→11).
- Prerequisites: 07, 03, 04, 05. Postrequisites: none (leaf).
- Ground truth: `forkexit.c:146-245`; `fork.rs` srv arm; `com.h` SRV_FORK RQ/RS.
- Knowledge rows: K-046.
- Acceptance: 5-row diff table vs 07; reader can state why no SUSPEND is needed.

### 09-pm-exit

- Placement: death with a barrier; producer of zombies.
- Covers: K-047/K-048/K-049 (produce side), K-050/K-051/K-052/K-054, exit_proc 9 +
  exit_restart 5 step lists, system-proc fast path, TRACE_EXIT flagging (consume in
  10/18), coredump pointer deferral pointer (D-16 → 21).
- Non-covers: reaping/consumption (→10), VFS/VM peer work (cited), wait4 payload
  assembly (→10).
- Prerequisites: 03, 05, 06, 02, 07. Postrequisites: 10 (consume), 11 (lethal entry),
  21 (teardown cite).
- Ground truth: `forkexit.c:246-474,594-669,731-806`; `exit.rs:1138`;
  `mproc/lifecycle.rs`, `guardianship.rs`.
- Knowledge rows: K-047, K-048, K-049, K-050, K-051, K-052, K-053, K-054.
- Acceptance: barrier diagram (what precedes VFS_PM_EXIT); zombie two-level diagram;
  adoption + SIGHUP broadcast rule with session anchor; fast-path condition exact.

### 10-pm-wait

- Placement: reaping + billing; consumer of 09's zombies.
- Covers: K-055..K-058, K-098-consume (W_STOPCODE assembly), 4×3 selection matrix,
  tracer pseudo-parent path, vircopy rusage delivery + hz conversion, try_cleanup
  VFS|EVENT guard.
- Non-covers: zombie production (→09), tracer relationship setup (→18), signal
  semantics of collected stops (→11).
- Prerequisites: 03, 09, 02, 06. Postrequisites: 18 cites its stop-code contract.
- Ground truth: `forkexit.c:475-593,670-806`; `utility.c:144-156`; `wait.rs:404`;
  `mproc/wait.rs`.
- Knowledge rows: K-055, K-056, K-057, K-058.
- Acceptance: pidarg×ring matrix with an example each; payload contract (tag + typed
  body) stated once and reused by 18; billing path with failure semantics (no
  cleanup on copy failure).

### 11-signal-core

- Placement: signal framework; the 9-chain is this stage's most-cited object.
- Covers: K-060..K-065, K-070 (set construction use), pid 4-state table, perm gate,
  PRIV_PROC triple, lethal predicates, ksig-vs-user dual bitmaps.
- Non-covers: handler API (→12), stop/resume machinery (→13), kernel frame work
  (peer stage), mask API (→12).
- Prerequisites: 03, 04, 02, 09. Postrequisites: 12, 13, 14, 17, 18 cite the chain.
- Ground truth: `signal.c:197-225,294-567`; `signal.rs:1295`; `mproc/signal.rs`.
- Knowledge rows: K-060..K-065, K-070.
- Acceptance: 9-decision chain as a numbered list with per-decision anchor; perm
  gate precedences exact; reader can classify any kill/srv_kill/ksig input.

### 12-signal-handlers

- Placement: user-visible signal API (install/mask/suspend/return/send).
- Covers: K-066..K-069 (5 syscalls + flags + asymmetry + 4-step send with EINTR/EFAULT
 档位), KILL/STOP stripping 5-site list.
- Non-covers: delivery priority (→11), stop/resume mechanics (→13), kernel sigframe
  (peer stage).
- Prerequisites: 11, 02, 04. Postrequisites: 13 (send/stop call sites), 17 (catch
  reset consumer).
- Ground truth: `signal.c:40-196,776-855`; `signal_handlers.rs:505`.
- Knowledge rows: K-066, K-067, K-068, K-069.
- Acceptance: IGN/DFL/CATCH asymmetry table; mask2 pairing diagram; send path with
  all three outcomes (delivered / EINTR-broken / fallback-terminate).

### 13-signal-flow

- Placement: the machinery room (stop/pend/resume) + the honest deferral contract.
- Covers: K-071..K-075 (stop/resume, unpause 3-path, pending/restart + PROC_STOPPED
  dual use, A-10 deferred-call section per G-9, EBUSY causal chain).
- Non-covers: delivery semantics (→11/12), VFS/event business (→05/06), kernel
  EBUSY producer (cited).
- Prerequisites: 11, 12, 05, 06, 02. Postrequisites: 11 cites stop points; 14 cites
  alarm paths.
- Ground truth: `signal.c:226-293,652-776`; `signal_flow.rs:615`.
- Knowledge rows: K-071..K-075.
- Acceptance: unpause 3-path decision tree; PROC_STOPPED dual-use stated without
  hand-waving; A-10 section states exactly what is deferred, what fails closed, and
  the edge owner.

### 14-itimer

- Placement: time-triggered signals over the CLOCK route.
- Covers: K-076..K-079 (3 families, conversions, alarm chain, backend split).
- Non-covers: kernel timer queue (peer), clock source (→19 for hz, peer for driver),
  signal delivery (cite 11).
- Prerequisites: 04, 11, 02, 12. Postrequisites: none (leaf service).
- Ground truth: `alarm.c:33-344`; `timer.rs:813`.
- Knowledge rows: K-076..K-079.
- Acceptance: conversion formulas with round-up + clamp + invalid-input table;
  REAL vs VIRTUAL backend diagram; CLOCK-notify wiring from 04's route to
  cause_sigalrm end-to-end.

### 15-credentials

- Placement: identity authority + VFS dual-copy protocol.
- Covers: K-080..K-084 (13-call framework + difference table, triple + groups,
  TAINTED source section, dual-copy SUSPEND loop, session/group semantics).
- Non-covers: exec's setuid use (→17 cites), fork inherit (→07 cites), scheduling
  (→16), signals (→11).
- Prerequisites: 02, 05, 04. Postrequisites: 07/17 cite TAINTED.
- Ground truth: `getset.c:19-223`; `credentials.rs:487`; `mproc/credentials.rs`;
  `com.h` SET* RQ/RS.
- Knowledge rows: K-080..K-084.
- Acceptance: full-vs-single-set difference table (13 rows); dual-copy sequence
  with the SUSPEND close-loop; TAINTED source proof (all writers listed).

### 16-scheduling

- Placement: scheduler-client protocol + the two conversion functions.
- Covers: K-085..K-088 (init/start/nice + guard, dual conversion + quantization
  note, getsetpriority ownership, SCHED wire protocol + A-8 status).
- Non-covers: kernel runqueues (peer stage), SCHED service internals (06-stage),
  VFS machine (cited).
- Prerequisites: 01, 04, 02. Postrequisites: 07 cites start; 01 cites T-8.
- Ground truth: `schedule.c:20-112`; `utility.c:91-103`; `main.c:275-289`;
  `misc.c:239-286`; `sched.rs:515`; `minix/sched.h`.
- Knowledge rows: K-085..K-088.
- Acceptance: both conversion directions with the 1→2 quantization example;
  KERNEL/NONE guard stated with call-site anchor; getsetpriority file-vs-semantics
  note (D-7 locked).

### 17-exec

- Placement: identity + address-space change in one handshake.
- Covers: K-089..K-093, K-059-cite (signal reset consumer view), PARTIAL_EXEC +
  caller gates, VFS/PM check split, tracer-first ordering, frame save.
- Non-covers: VFS executable loading (peer), VM mapping (peer), signal reset
  production (→12), clock/hz (→19), kernel execve (peer).
- Prerequisites: 05, 15, 11, 12, 02. Postrequisites: 18 cites tracer-first.
- Ground truth: `exec.c:38-200`; `exec.rs:441`.
- Knowledge rows: K-089..K-093 (+K-059 consumer view).
- Acceptance: 3-door diagram (exec/newexec/restart) with RS-restart path;
  check-split table (VFS column vs PM column); reset list exact per
  `signal.c:179-182`.

### 18-trace (ABI-corrected, skippable)

- Placement: ptrace as a guarded debug protocol, not a backdoor.
- Covers: K-094..K-099 (full T_* table vs ptrace.h, ATTACH/DETACH chains, RANGE
  bounds, root-gate order, stop payload, passthrough fidelity).
- Non-covers: kernel sys_trace internals (peer), wait4 consumption (→10),
  exit production (→09).
- Prerequisites: 03, 10, 11. Postrequisites: none (leaf).
- Ground truth: `trace.c:42-276`; `sys/sys/ptrace.h:37-55,211,226-250`;
  `trace.rs:892`.
- Knowledge rows: K-094..K-099.
- Acceptance: 18-row T_* table with C values (G-2 fix verified by the full-ABI
  assertion test); DETACH chain (replay→signal→check_pending→passthrough) stepwise;
  error-fidelity list (which errors pass through, which are PM judgments).

### 19-time

- Placement: clocks as read-mostly queries with two settable exceptions.
- Covers: K-100/K-101 (5 calls, clock dispatch, boottime arithmetic).
- Non-covers: kernel clock driver/settime (peer), hz source (cite 01), VFS time
  (peer).
- Prerequisites: 01, 04, 02. Postrequisites: none (leaf).
- Ground truth: `time.c:22-131`; `time.rs:536`.
- Knowledge rows: K-100, K-101.
- Acceptance: per-call table (clock → permission → MONOTONIC-set refusal);
  overflow-safety argument for the hz arithmetic in one paragraph.

### 20-info-queries (narrowed; skippable details)

- Placement: the query plane — read-only windows into PM state.
- Covers: K-102/K-103/K-104/K-105/K-108/K-109/K-110/K-111 (uname, sysinfo wire image,
  procnr, epinfo, getrusage copy-out, mcontext pair, stats cfg, find_param) —
  identical per-call skeleton, getsysinfo as representative.
- Non-covers: everything control-plane (→21: reboot/svrctl/sprofile);
  getsetpriority (→16); rusage accounting production (→10, cite K-057).
- Prerequisites: 03, 04, 01. Postrequisites: none (leaf).
- Ground truth: `misc.c:33-198,401-446`; `mcontext.c:13-27`; `profile.c` (cite for
  the flag only); `utility.c:57-75`; `misc.rs:1177` (query arms); `mproc/wire.rs`.
- Knowledge rows as above.
- Acceptance: per-call subsections ≤60 lines each; ESCRIPT exclusion with zero-use
  grep (K-111); stats-cfg tri-state (on/off/absent) exact.

### 21-control-and-shutdown (NEW, skippable)

- Placement: the control plane + system teardown — the G-6/G-7 answer.
- Covers: K-035 (REBOOT special path), K-106 (svrctl branches + order), K-107
  (sprofile + honest ENOSYS), K-116 (teardown ordering), K-036-cite (coredump
  deferral status), RS gating (misc.c:154-style gates as PM observes them).
- Non-covers: per-process exit (→09), info queries (→20), RS supervision internals
  (→03-stage-rs), kernel halt mechanics (peer).
- Prerequisites: 04 (dispatch + ENOSYS/SUSPEND), 09 (exit barrier), 01 (RS
  parentage). Postrequisites: none (leaf).
- Ground truth: `main.c:304-312`; `misc.c:199-238,291-400`; `profile.c:22-45`;
  `forkexit.c` exit barrier (cite); RS gate sites (`misc.c:154`, `signal.c:212`).
- Knowledge rows: K-035, K-106, K-107, K-116 (+K-036 pointer).
- Acceptance: reboot sequence stepwise with the never-reply point marked; svrctl
  branch precedence table; teardown order diagram (VFS→VM→sched→table); deferred
  items each carry edge owner + fail-closed behavior.

### 99-global-concepts (expanded index)

- Placement: vocabulary — read first lightly, then as reference.
- Covers: K-017/K-018 (identity + sizes), K-070 (set numbers), 7-global table
  (from `glo.h` + A-3 note "now explicit params — see 03/04"), 47-number index
  (number → family → doc), endpoint encoding diagram (cite 03's, reproduce the
  table), constant-radius notes (where each constant may be assumed).
- Non-covers: all mechanics (every entry points to its doc).
- Prerequisites: 00. Postrequisites: all docs cite it for numbers.
- Ground truth: `pm.h:27`; `const.h:20`; `type.h:5`; `glo.h:31`; `proto.h:96`;
  `callnr.h:9-62`; `mproc/constants.rs:53`.
- Knowledge rows: K-017, K-018, K-070 (index views).
- Acceptance: every magic number appearing in 01–21 resolves here with a C anchor;
  47-index complete; target 300–400 lines; zero mechanism prose (test: deleting any
  paragraph must not remove behavioral knowledge).

---

## 6. Change Table

| Op # | Type | Old location | New location | Reason | Knowledge IDs | Destination |
|---|---|---|---|---|---|---|
| C-01 | refresh | 00 (96-line nav, stale wiring §3.1) | 00 (~300 lines: boundary+layers+live status+ARCH index+nav) | G-5; stale counts mislead | K-112,K-113,K-117 | Rewrite; old §3.1 → pointer to 04 wiring table |
| C-02 | refresh+add | 01 (+build/exclusion gaps) | 01 (+build-identity §, +3 exclusion one-liners) | §3.5 link/load/build/asm rows homeless | K-001..K-008 | Add §§; rest rewrite in place |
| C-03 | lock | 02/03 (723/704 lines, clean) | 02/03 (wording locked, cite discipline) | Headers already satisfy contracts; only stale anchors refreshed | K-010..K-019 | In-place truth refresh, no moves |
| C-04 | refresh+add | 04 (645 lines, pre-S4 wiring story) | 04 (+live wiring table K-030, +concurrency K-115, +fidelity K-114, +test discipline K-117) | G-1 (stale "7 wired"), §3.5 concurrency/test rows | K-020..K-030,K-113..K-115,K-117 | Rewrite dispatch-status §§; batch table → dated history box |
| C-05 | expand | 05 (349 lines) | 05 (~650 lines, 11 per-arm subsections + field table) | G-3 thinnest mechanism doc | K-031..K-034 | Expand in place; no content moves out |
| C-06 | lock | 06/07/08/09/10 (clean, well-bounded) | same numbers, truth refresh only | Audit finds no structural defect; 08 stays the justified diff-doc | their K rows | In-place; 09 adds teardown pointer →21 |
| C-07 | lock | 11/12 (clean framework+API split) | same numbers | 855-line C file justifies the split; headers clean | K-060..K-069 | In-place truth refresh |
| C-08 | refresh+add | 13 (+A-10 gap) | 13 (+deferred-call semantics §) | G-9 | K-071..K-075 | Add §; rest in place |
| C-09 | lock | 14/15/16/17 (clean, single-semantic) | same numbers | No defect found; TAINTED/getsetpriority ownerships locked | their K rows | In-place; 16 keeps D-7 note |
| C-10 | fix | 18 §2.14 TO_NOEXEC=0x1 | 18 (0x4 + full T_* table + ABI test) | G-2 factual error | K-094..K-099 | Correct + add acceptance test |
| C-11 | lock | 19 (clean leaf) | 19 | No defect | K-100/K-101 | In-place |
| C-12 | split | 20 (522 lines, 10+ calls, info+control mixed) | 20 (info plane, ~450 lines) + 21 (NEW control plane, ~350 lines) | G-6 single-semantic violation; G-7 teardown homeless | K-102..K-105,K-108..K-111 →20; K-035,K-106,K-107,K-116 →21 | §-level move table in §8.2 |
| C-13 | expand | 99 (93 lines) | 99 (~350 lines full vocabulary) | G-4 promise vs content | K-017/K-018/K-070 index views | Expand in place |
| C-14 | archive | `draft/` 9 files | archived (unchanged, still unmaintained) | Superseded; plan/todo already declare defer | — | No move; formal docs never cite draft beyond "superseded" |
| C-15 | note (code, not docs) | code comments citing `.design/` snapshots | code-cleanup note for B-phase | G-8; formal docs unaffected | — | File as code-hygiene ticket (§8.3 list) |

No renames, no renumbers, no merges. Total ops: 15 (5 refresh/add, 1 expand-major,
1 split+new, 1 factual fix, 6 lock/refresh-light, 1 archive-confirm).

---

## 7. Missing-New-Docs (the §3 gaps, each closed — no "TBD" allowed)

- Non-C fixed list: all 10 answered in §3.5 (homes: 01 for link/load/image/build +
  exclusion one-liners; 04/05 for wire; 04 for errors+concurrency+test discipline;
  21 for shutdown; per-doc errno tables for error paths). Nothing left TBD.
- Gap closures: G-1→04 live wiring table (C-04); G-2→18 ABI fix (C-10); G-3→05
  expansion (C-05); G-4→99 expansion (C-13); G-5→00 revision (C-01); G-6/G-7→21 new
  doc (C-12); G-8→code-hygiene ticket (C-15); G-9→13 A-10 section (C-08).
- New-doc raw material: for 21 — old 20's reboot/svrctl/sprofile subsections (move +
  rewrite), `main.c:304-312` REBOOT path (new from C), teardown ordering (new
  synthesis from forkexit.c + misc.c + main.c, flagged as synthesis with anchors per
  step), RS-gate observations (cite 03-stage-rs, no new claims).

---

## 8. Anchor Migration and Breakage Cost

### 8.1 Section-migration table (changed docs per-section; locked docs per-doc)

| Old location (doc §) | Old content (one line) | New location | Type | Breakage risk |
|---|---|---|---|---|
| 00 §3.1 wiring counts ("8 wired + batches") | stale wiring summary | 04 wiring table (live); 00 keeps dated pointer | rewrite | Low: 00 has no code refs; doc refs updated in §8.3 |
| 00 §2 nav (22 docs) | 22-doc map | 00 §2 (23-doc map + skip marks) | rewrite | Low |
| 04 batch table as current status | "40 ENOSYS" era table | 04 history box (dated) + live 47-row table | rewrite | Medium: todo.md §11.1.1 still cites batches — keep todo untouched, mark doc-side as history |
| 05 (whole, 349 lines) | single-pass 11-reply summary | 05 (11 per-arm subsections) | expand | Low: nobody cites 05 subsections by number (verified: refs are doc-level) |
| 13 (whole) | no A-10 section | 13 +deferred-call § | add | None |
| 18 §2.14 TO_NOEXEC=0x1 | wrong bit value | 18 corrected + T_* table | fix | Low: code already uses 0x4 (`guardianship.rs:62-63`); doc was the outlier |
| 20 reboot/svrctl/sprofile §§ | control calls inside info doc | 21 §§ (rewritten, +teardown + REBOOT path) | split/move | Medium: doc cross-refs to "20" for svrctl/reboot must retarget →21 (§8.3 list) |
| 20 remainder §§ | info queries | 20 (same numbers, tightened skeleton) | rewrite | Low |
| 99 (whole, 93 lines) | thin glossary | 99 (full vocabulary) | expand | Low: refs to 99 are doc-level |
| 01/02/03/06/07/08/09/10/11/12/14/15/16/17/19 | bodies (headers verified clean) | same docs, anchor/line refresh | refresh-light | None expected; verification grep in B-phase per doc |
| `draft/`/* | old fork mainline | archived, unchanged | archive | None (already declared unmaintained) |

### 8.2 Old-20 → new-20/21 section moves (the only split)

| Old 20 subsection | New home | Treatment |
|---|---|---|
| uname/uts | 20 §uname | tighten to skeleton |
| getsysinfo (perm+size+dреев wire) | 20 §sysinfo (representative, full) | keep + wire-image emphasis (K-103) |
| getprocnr / getepinfo | 20 §§ | tighten |
| getrusage | 20 § (consumer view; cite 10 for accounting) | add cite, keep query path |
| mcontext pair | 20 § | tighten |
| calls_stats / uts-compat / find_param / ESCRIPT note | 20 appendix § | keep, already well-formed |
| reboot sequence | 21 §reboot (+`main.c:304-312` path) | move + expand (K-035/K-116) |
| svrctl branches | 21 §svrctl | move + precedence table (K-106) |
| sprofile/SPROFILE | 21 §sprofile | move + honest-ENOSYS (K-107) |
| teardown ordering (new synthesis) | 21 §shutdown | new (K-116) |

### 8.3 Reference-migration table + breakage-cost summary

Doc→doc references (verified 2026-09-19, `rg` for doc basenames inside 04-stage-pm):

| Referrer (hits) | Target pattern | Action |
|---|---|---|
| plan.md (16), 01 (14), 04 (11), 03 (8), 13/05 (7 each), 02/06 (6 each) | old numbers/titles | No number changes → zero retargets, except: any "20 covers svrctl/reboot" claim → retarget to 21 (B-phase grep `20-misc-queries.*(svrctl\|reboot\|sprofile)` then hand-fix; estimated ≤6 sites) |
| todo.md, edge_todo.md (27 hits for "04-stage-pm") | stage-level pointers | Untouched (reference material, not rebuilt) |
| Other stages (06-stage-sched/plan 13, 05-stage-vfs/plan 9, 08-stage-is, 18-stage-commands) | "04-stage-pm/NN-*" links | Zero retargets (numbers stable); notify-only |

Code-comment references (`rg` in `os/`):

| Site | Cites | Action |
|---|---|---|
| `pm/src/init.rs:6,8-9,184,454,739,937`, `main.rs:4`, `ipc/calls.rs:3,988`, `ipc/dispatcher.rs:3,9`, `mproc/table.rs:5`, `mproc/mproc.rs:143,291` | `NN-name.md` formal paths | Zero changes (all formal, all numbers stable) |
| `minix-types/src/ipc/event.rs:7`, `vfs.rs:106,313`, `pm.rs:50` | `04-stage-pm/06…/05…` formal paths + `todo.md §11.1.1` | Zero changes (formal); the one `05-design.v1.md D2` cite (`vfs.rs:313`) + `event.rs:10`'s `.design/06-design` cite go to the C-15 code-hygiene ticket |
| `pm/src/event.rs:10`, (grep full list in B-phase) | `.design/*` snapshots | C-15 ticket: replace with formal `NN-name.md §X` refs; estimated 3–5 sites; no doc-side action |

Breakage-cost summary: affected references ≈ 40 doc-level cites (all number-stable →
mechanical zero-change except ≤6 hand retargets for the 20→21 split) + ≈20 code
comments (zero-change except 3–5 `.design/` cites → hygiene ticket). Hotspots:
plan.md (16), 01 (14), 04 (11). Batch method: `rg -n "20-misc-queries" notes/ os/`
→ hand-retarget control-call cites to `21-control-and-shutdown.md`; `rg -n
"\.design/" os/servers/pm os/libs/minix-types` → hygiene ticket; re-run both greps
to verify zero remainder. Total: no renumber, no rename — the I-14 lesson holds.

---

## 9. Verification, Self-Gates, Verdict

### 9.1 Four mechanical checks (§5.4)

1. Forward-reference scan (new order 00→99→02→03→01→04→05→06→07→…→21; 99 is a
   lookup, not a read-gate): every contract's "Prerequisites" points to an earlier
   number or a peer stage. Result: PASS (one intentional lateral: 17→12 catch-reset
   consumer view cites earlier-numbered 12; 21→09 cites earlier 09 — both backward).
2. Dependency graph: 23 nodes, edges from §5 prerequisites. Result: ACYCLIC (spot-
   checked chains 01→03→07→09→10, 04→05→07, 11→12→13, 20→10; no back edge; 04 is the
   hub, 00/99 are sources). Full machine check is B-phase work with the same edge
   list.
3. Coverage: all 101 pool entries have a §5 home (§2 "Home" column); 7 N-entries
   carry evidence anchors; deletions: exactly one (ESCRIPT, K-111, with zero-use
   grep rationale). Result: 100% placed, 1 principled deletion.
4. Breakage cost: §8.3 totals (≈40 doc cites, ≈20 code comments, ≤6 hand retargets,
   3–5 hygiene sites). Result: COUNTED with batch method + re-grep verification.

### 9.2 Self-gates G1–G9

| Gate | Check | Result + evidence |
|---|---|---|
| G1 | C true order spot-checkable (10 random anchors) | PASS — verified live 2026-09-19: `utility.c:34 get_free_pid` exists; `calls.rs:212 dispatch_pm_call`, `:991` catch-all, `:986-990` "no ENOSYS arm" comment; `dispatcher.rs:89/100-114` three-way; `draft/` 9 files listed; `doc_rerank_muse.md` absent (clean landing); `init.rs`/`calls.rs`/`dispatcher.rs`/`table.rs`/`mproc.rs` formal doc cites confirmed |
| G2 | Every C file + non-C artifact has a home or principled exclusion | PASS — 15 .c + 6 .h all own pool rows (§2.1–2.9 headers name each file); 10 non-C rows in §3.5 with 2 principled exclusions (asm entry, boot asm — PM is user-space) |
| G3 | Zero forward references in new catalog | PASS — §5 prerequisites audited (see check 1) |
| G4 | Dependency graph acyclic | PASS (spot-checked; machine check deferred to B-phase with edge list frozen here) |
| G5 | 100% pool placement; N-entries anchored; deletions listed | PASS — 101/101 placed; K-030/K-035/K-094/K-103/K-111/K-116/K-008 anchored; 1 deletion (ESCRIPT) |
| G6 | Splits/moves show stock destinations; new items show source anchors (10-spot) | PASS — C-12's 10 section moves in §8.2 each name old→new; 10-spot: K-030 (calls.rs arms), K-035 (main.c:304-312), K-094 (ptrace.h), K-103 (wire.rs), K-111 (exec.c:31+grep), K-116 (3 anchors), K-008 (main.rs:15), K-074 (system.c:463 cited), K-113 (exit.rs+syscall.rs), K-115 (context.rs) |
| G7 | Every contract has 7 elements | PASS — all 23 contracts carry placement/covers/non-covers/prereqs+postreqs/ground-truth/knowledge-rows/acceptance |
| G8 | Migration covers all changed docs per-section + code-comment refs | PASS — §8.1 per-section for 00/04/05/13/18/20/21/99, per-doc for locked docs; §8.3 covers doc cites + code comments incl. `.design/` hygiene sites |
| G9 | Factual claims anchored (10-spot); speculation labeled | PASS — anchors verified in G1/G6 spots; explicit speculation flags: "estimated ≤6 sites", "3–5 sites", "≈40/≈20" (estimates labeled as such), "target 600–700/300–400 lines" (targets, not claims) |

### 9.3 Verdict and questions for the user

Verdict: **BLUEPRINT COMPLETE — ready for B-phase** (all 9 gates pass; no gate
reports "incomplete"). Cost of the rebuild is bounded (§8.3); the only structural
operation is the 20→20+21 split; the highest-risk content operation is the 05
expansion (11 arms must each be verified against `main.c:294-424` during writing).

Questions requiring user adjudication (B-phase blockers, nothing else):

1. Confirm the single-split scope: is `21-control-and-shutdown.md` the right home
   for svrctl/reboot/sprofile, or should control calls stay in 20 with only the
   teardown section added? (Blueprint default: split, per the single-semantic rule.)
2. Confirm the wiring-table freshness protocol for 04: live table with generation
   date, refreshed per release — or a frozen snapshot with "as of" stamp? Frozen
   counts caused G-1; I recommend live-pointer discipline (default).
3. Confirm the C-15 code-hygiene ticket (3–5 `.design/` cites in production
   comments) is filed as code work, not doc work, and whether B-phase may touch
   `os/` comments at all.

Scope notes for B-phase: write docs strictly from pool + anchors (§2 + §5 ground
truth); never from other doc_rerank_*; never cite `.design/`; keep single-doc
lengths readable (05/99 expansions are the only ones allowed past 700 lines, and
only to their stated targets); archive, don't delete, old docs.
