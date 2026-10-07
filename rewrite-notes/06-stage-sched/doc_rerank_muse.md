# 06-stage-sched Document Rebuild Blueprint (muse)

## 0. Metadata

- Executor: `muse`
- Date: 2026-09-20 (UTC)
- Target directory: `notes/rewrite/fork-syscall-rewrite/06-stage-sched/`
- Repository root: `/home/xzhao/github/minix-rs`
- Current commit: `7fe0df839` (measured with `git rev-parse --short HEAD` on 2026-09-20)
- Task: Phase R (rebuild blueprint). Output is `target_dir/doc_rerank_muse.md`. No body text is modified.
- Output language: English (per muse special requirement).
- Constraints honored: no reference to `.design/` or `tmp_design_and_todo/`; the only file written is `doc_rerank_muse.md`; no other agent's `doc_rerank_*` product was read (the four existing `doc_rerank_*.md` files in this directory were listed but never opened).

### 0.1 Scope

In scope (numbered docs + global doc):

| # | File | Lines | Header-declared not-covering |
|---|------|-------|------------------------------|
| 00 | `00-sched-overview.md` | 80 | all mechanism detail |
| 01 | `01-sched-init-main.md` | 209 | dispatch rules (02), handler bodies (06-08), balancer definition (11), machine-info uses (10) |
| 02 | `02-sched-message-surface.md` | 279 | handler bodies (06-08), field semantics (09/12/13), balancer execution (11), startup (01) |
| 03 | `03-schedproc-struct.md` | 208 | table gates (04), value semantics (05), writers (06-08) |
| 04 | `04-schedproc-table.md` | 191 | field detail (03), per-handler uses (06-08), error full table (99) |
| 05 | `05-priority-timeslice-model.md` | 218 | struct/table impl (03/04), per-handler uses (06/08/09/10/11), kernel-side checks (09), PM nice user surface (04-stage-pm/16) |
| 06 | `06-start-scheduling.md` | 249 | dispatch (02), client packing (13/14), fanout internals (09), cpu choice internals (10) |
| 07 | `07-stop-scheduling.md` | 213 | client short-circuit (13/14), fanout/cpu (09/10), takeover flow (06) |
| 08 | `08-noquantum-nice.md` | 208 | fanout (09), accounting fields (12), PM caller surface (13), recovery (11) |
| 09 | `09-schedule-process.md` | 220 | scheduler registration (12), cpu policy (10), per-handler call sites (06-08) |
| 10 | `10-pick-cpu-smp.md` | 197 | kernel migration (01-stage-kernel/16), fanout (09), EBADCPU retry (06) |
| 11 | `11-balance-queues.md` | 204 | clock kernel side (01-stage-kernel/15), setalarm impl, fanout (09), demotion flow (08) |
| 12 | `12-kernel-interface.md` | 214 | kernel primitive internals (01-stage-kernel/11), sched_proc full flow, mini_send detail, PM/RS caller surface (13/14) |
| 13 | `13-pm-interaction.md` | 212 | PM nice user surface (04-stage-pm/16), RS surface (14), server checks (06), _taskcall transport |
| 14 | `14-rs-interaction.md` | 217 | RS slot config (03-stage-rs/08), live update (03-stage-rs/16), server checks (06), PM surface (13) |
| 99 | `99-global-concepts.md` | 50 | kernel queue impl, nice syscall iface, nice numeric table (05/08) |

Reference material (not rebuilt, read as evidence): `plan.md` (383 lines), `todo.md` (60 lines), `draft/` (6 stub files: `00-sched-overview.md`, `01-sched-struct.md`, `02-sched-inherit.md`, `03-sched-start.md`, `04-sched-quantum.md`, `99-global-concepts.md`), `archive/` (two review archives).

Out of scope: everything outside this directory except as cited boundary contracts (Sections 0.3, 3.4).

### 0.2 Read list

1. All 16 docs above: full header declarations read (table 0.1); body sampled section-by-section for the knowledge pool (Chap. 1 headers, §1.x concept claims, boundary lists, and every C anchor cited).
2. C ground truth, full read: `minix3/minix/servers/sched/main.c` (137), `schedule.c` (369), `utility.c` (74), `schedproc.h` (39), `sched.h` (18), `proto.h` (21); total 658 lines. Counterpart files: `minix3/minix/include/minix/com.h:801-807` (SCHEDULING_*), `com.h:449` (SCHEDCTL_FLAG_KERNEL), `include/minix/config.h:66-77` (queue constants), `include/minix/ipc.h:1428-1445,1819-1828,1904-1913` (wire structs), `lib/libsys/sched_start.c` (76), `lib/libsys/sched_stop.c` (29), `servers/pm/schedule.c` (94), `servers/rs/utility.c:364-384` (sched_init_proc), `kernel/system/do_schedctl.c` (49), `kernel/system/do_schedule.c` (31), `kernel/system.c:sched_proc`, `kernel/proc.c:1860-1910` (notify_scheduler/proc_no_time).
3. Rust implementation entries (read, not authoritative for order): `os/servers/sched/src/` — 19 files, 4,707 lines total (measured `wc -l` over `src/*.rs` + `src/**/*.rs` on 2026-09-20): `main.rs`, `server.rs` (1,187), `sef.rs`, `dispatch.rs`, `schedproc.rs`, `table.rs`, `valid.rs`, `priority.rs`, `scheduling/{start,stop,noquantum,nice}.rs`, `kernel_api/{schedule,schedctl,transport}.rs`, `cpu.rs`, `balancer.rs`, `client.rs`, `lib.rs`.
4. Boundary materials: `notes/rewrite/fork-syscall-rewrite/00-master-plan/README.md` (stage table row 06, boot two-layer semantics), `edge_todo.md` (E-SCHEDNICED, E-PREEMPTFLAG, E-SCHEDSMP, E-MINTYPES-SYS, E8, E5(e)), this stage `plan.md` + `todo.md`.
5. Neighbor stage overview: `05-stage-vfs/00-vfs-overview.md` (header + startup-chain shape, confirms no concept overlap: VFS boot chain vs SCHED policy server are disjoint) and `04-stage-pm/16-scheduling.md` exists as the declared owner of the PM nice user surface.
6. Style example: `01-stage-kernel/06-todo.md` exists in `01-stage-kernel/` listing; only its contract-writing pattern (what / not-what / deferred-to / acceptance) is borrowed, none of its conclusions.

### 0.3 Commands run (evidence excerpts)

```bash
git rev-parse --short HEAD                                   # 7fe0df839
wc -l notes/rewrite/fork-syscall-rewrite/06-stage-sched/*.md # §0.1 line counts
ls minix3/minix/servers/sched/                               # Makefile main.c proto.h sched.h schedproc.h schedule.c utility.c
ls -R os/servers/sched/src/ ; wc -l os/servers/sched/src/*.rs os/servers/sched/src/**/*.rs  # 19 files, 4707
grep -n 'SCHEDULING_BASE|...' include/minix/com.h            # com.h:801 base 0xF00, :803-807 five requests
grep -n 'NR_SCHED_QUEUES|...' include/minix/config.h         # config.h:66-77
grep -n 'mess_lsys_sched|mess_pm_sched|mess_sched_lsys' include/minix/ipc.h  # 4 wire structs
grep -rn 'schedule_process_migrate|schedule_process_local' servers/sched/    # migrate: defined, zero callers
grep -rn 'pick_cpu|cpu_proc' servers/sched/schedule.c        # 8 hits incl. :302 re-pick inside fanout
grep -rn '06-stage-sched' notes/ os/ --include='*.md' --include='*.rs' | wc -l  # 159 cross-refs
grep -rn '\.md' os/servers/sched/src/ | wc -l                # 32 doc mentions across all 19 rs files
```

---

## 1. C True Order (runtime order, rebuilt from source, not paraphrased)

Stage type verdict: **service event-loop type** (per §9 of the prompt: birth + init, then message interface, core data structures, per-scenario request handling, queries/misc, neighbor protocols). SCHED is not a boot chain (RS owns its birth; SCHED only registers callbacks) and not a call collection (five requests share one table and one fanout path). The true-order table therefore has two segments: **S1 birth/init (T-01..T-06)** and **S2 loop (T-07..T-22)**.

### 1.1 S1 — Birth and init

| Step | Action | Anchor | Note |
|------|--------|--------|------|
| T-01 | RS loads the SCHED image; `main()` entered | `main.c:22` | Birth itself is out of stage (RS owns it). SCHED's first owned act is T-02. |
| T-02 | `sef_local_startup()` registers `sef_cb_init_fresh` + `SEF_CB_INIT_RESTART_STATEFUL`, calls `sef_startup()` | `main.c:111-121` | No signal callbacks. Restart path is stateful (table survives). |
| T-03 | `sef_cb_init_fresh`: `sys_getmachine(&machine)` fills `processors_count`/`bsp_id`, panics on failure | `main.c:126-131` | Topology is read once, never refreshed. |
| T-04 | `sef_cb_init_fresh`: `init_scheduling()` computes `balance_timeout = BALANCE_TIMEOUT * sys_hz()` and arms `sys_setalarm`; panics on failure | `schedule.c:334-342`, `schedule.c:18` (`BALANCE_TIMEOUT 5`) | Failure here is fatal (panic), not a reply error. |
| T-05 | Static zero-initialization of `schedproc[NR_PROCS]`, `cpu_proc[]`, `machine`, `balance_timeout` | `schedproc.h:36`, `schedule.c:16,46`, `main.c:17` | All-free / all-zero / all-alive is the implicit ledger start; no explicit clear loop exists. |
| T-06 | Enter `while (TRUE)` loop | `main.c:35` | Handoff from S1 to S2. |

### 1.2 S2 — Loop (receive, classify, dispatch, reply)

| Step | Action | Anchor | Note |
|------|--------|--------|------|
| T-07 | `sef_receive_status(ANY, &m_in, &ipc_status)`; panic on transport error | `main.c:39-40` | Single-threaded: one message at a time, no worker threads. |
| T-08 | Extract `who_e = m_source`, `call_nr = m_type` | `main.c:41-42` | |
| T-09 | Notification check first: `is_ipc_notify(ipc_status)` | `main.c:45` | Notifications outrank calls, always. |
| T-10 | CLOCK notification → `balance_queues()`, then `continue` (no reply) | `main.c:47-49,54` | The only notification SCHED acts on. |
| T-11 | Any other notification → `continue` (no reply, silent) | `main.c:50-54` | |
| T-12 | `SCHEDULING_INHERIT` / `SCHEDULING_START` → `do_start_scheduling()` | `main.c:58-61` | Two letters, one door. |
| T-13 | `SCHEDULING_STOP` → `do_stop_scheduling()` | `main.c:62-64` | |
| T-14 | `SCHEDULING_SET_NICE` → `do_nice()` | `main.c:65-67` | |
| T-15 | `SCHEDULING_NO_QUANTUM`: if `IPC_FLG_MSG_FROM_KERNEL` → `do_noquantum()`, warn-and-`continue` on failure (no reply ever); else log fakery + `result = EPERM` (replied) | `main.c:68-84` | The only request with a source-gate at the dispatch layer instead of inside the handler. |
| T-16 | default → `no_sys()` returns `ENOSYS` | `main.c:85-86`, `utility.c:18-23` | |
| T-17 | Reply unless `result == SUSPEND`: `m_in.m_type = result; reply(who_e, &m_in)` (`ipc_send`) | `main.c:90-93`, `main.c:101-106` | SUSPEND is the sole no-reply call path; T-10/T-11/T-15-kernel are no-reply by `continue`. |
| T-18 | `do_start_scheduling` full chain: assert two-letter → `accept_message` → `sched_isemtyendpt(child)` → fill slot → `max_priority >= NR_SCHED_QUEUES → EINVAL` → init self-parent special case → START-vs-INHERIT fill → `sys_schedctl` takeover → `flags = IN_USE` → `pick_cpu` → `schedule_process(ALL)` + EBADCPU retry (`cpu = CPU_DEAD`, re-pick) → reply `scheduler = SCHED_PROC_NR` | `schedule.c:140-249` | Longest chain in the stage; sub-order is fixed. |
| T-19 | `do_stop_scheduling`: `accept_message` → `sched_isokendpt` → (SMP) `cpu_proc[cpu]--` → `flags = 0` | `schedule.c:112-135` | No fanout, no reply-field write. |
| T-20 | `do_noquantum`: `sched_isokendpt(m_source)` → `priority < MIN_USER_Q → priority += 1` → `schedule_process_local` | `schedule.c:87-107` | No `accept_message` (trust comes from T-15). Floor: never demotes past `MIN_USER_Q`. |
| T-21 | `do_nice`: `accept_message` → `sched_isokendpt` → `new_q >= NR_SCHED_QUEUES → EINVAL` → snapshot old → `max_priority = priority = new_q` → `schedule_process_local`, rollback on failure | `schedule.c:254-292` | The only handler with transactional rollback. |
| T-22 | `balance_queues` (from T-10): scan all `NR_PROCS`, `IN_USE && priority > max_priority → priority -= 1` + local fanout; re-arm alarm, panic on failure | `schedule.c:353-369` | One level per 5-second round; re-arm failure is fatal. |

Fanout sub-chain (used by T-18/T-20/T-21/T-22): `schedule_process` **re-runs `pick_cpu` on every call** (`schedule.c:302`), aggregates `new_prio/new_quantum/new_cpu` with `-1`-keep semantics (`schedule.c:304-317`), derives `niced = (max_priority > USER_Q)` (`schedule.c:319`), and issues `sys_schedule` (`schedule.c:321-322`). Kernel side: `do_schedule` checks `caller == p_scheduler → else EPERM` (`do_schedule.c:20`), `sched_proc` applies with EINVAL/EBADCPU checks, RTS_NO_QUANTUM requeue, MF_NICED (`system.c:642-723`).

Three facts the current docs underplay (all verified above, each becomes a knowledge-pool entry with a contract owner in §5):

- **F1. `schedule_process` re-picks the CPU every call** (`schedule.c:302`). Every demotion, nice change, and balance round re-executes `pick_cpu`, which on multi-CPU non-system processes increments `cpu_proc[cpu]` again. The ledger is therefore bumped far more often than `do_stop` decrements it. Owner: new fanout doc (§4, N-10).
- **F2. `schedule_process_migrate` is dead** (defined `schedule.c:34-35`, zero callers per grep). Owner: new fanout doc, explicit "deleted with reason" item (§6, C-14).
- **F3. `do_start_scheduling` double-picks**: `pick_cpu` at `schedule.c:226` plus the re-pick inside `schedule_process` at `:302` before the EBADCPU retry loop at `:227-231`. Owner: takeover doc (§4, N-06) with a cross-pointer to the fanout doc.

### 1.3 Order-difference table (runtime order vs teaching order)

| # | Runtime fact (anchor) | Teaching choice | Reason + back-pointer compensation |
|---|----------------------|-----------------|-------------------------------------|
| D-01 | Table gates (`sched_isokendpt/isemtyendpt`, `utility.c:29-56`) execute *inside* handlers (T-18..T-21), i.e. after dispatch | Taught before any handler (N-04 precedes N-06..N-09) | Gates are preconditions of every chain; teaching them first removes four repetitions. Each handler doc back-points to N-04 for the gate table. |
| D-02 | Priority/timeslice constants (`config.h`, `schedule.c:41-44`) are used site-by-site across T-18..T-22 | Taught once as a hub (N-05 precedes N-06..N-12) | Same reason: one definition, six uses. Handler docs cite N-05 values without redefining them. |
| D-03 | `pick_cpu` executes inside `schedule_process` (T-18 fanout, `schedule.c:302`) and inside takeover (T-18, `:226`) | Taught after fanout (N-11 after N-10), not inline | Choice policy + ledger deserve one home; N-06/N-10 cite N-11 and never re-explain the three rules. |
| D-04 | `balance_queues` executes on CLOCK notify (T-10), i.e. interleaved with calls | Taught after all call handlers (N-12 last of Part E) | Periodic recovery only makes sense once demotion (N-08) is understood; N-02 names the door, N-12 owns the room. |
| D-05 | Kernel registration (`sys_schedctl`, inside T-18 at `schedule.c:218`) executes before any scheduling exists | Taught after fanout (N-13 after N-10) | Registration is a two-direction contract (up-call + down-notify); teaching it after both directions' messages exist avoids forward refs to NO_QUANTUM accounting. |

---

## 2. Knowledge Pool (deduplicated, stage-wide)

Legend: Type = C(oncept) / M(echanism) / D(ata-structure) / I(nterface+protocol) / K(constraint+invariant) / A(rch-evolution) / T(ool+engineering) / E(test-character). Source = S(tock: from existing docs) / N(ew: from C / non-C / OS theory, added by §3 audit).

### 2.1 Stock entries (from existing docs, deduplicated)

| ID | Name | Type | Existing locations (main telling in bold) | Anchor | Reader gain (answers…) |
|----|------|------|-------------------------------------------|--------|------------------------|
| K-001 | Two-level scheduling model (policy vs execution face) | C | **00 §1.1**, 99 §3, 01 §1.1, 12 §1.1 | `main.c:1-3` comment + `do_schedule.c:8-31` | Why does a user-space server decide while the kernel enforces? |
| K-002 | SEF startup: fresh + stateful-restart registration | M | **01 §1.2** | `main.c:111-121` | What does SCHED register before it can serve? |
| K-003 | Machine topology read-once (`processors_count`/`bsp_id`) | M | **01**, 10 (use) | `main.c:126-131`, `type.h:123-124` | Where do CPU counts come from and why never refreshed? |
| K-004 | `init_scheduling` call site (alarm arming) | M | **01** (call site), 11 (definition) | `schedule.c:334-342` | When does the 5-second rhythm start? |
| K-005 | Main-loop skeleton (receive → classify → dispatch → reply) | M | **01** (skeleton), **02** (rules) | `main.c:35-96` | What is the loop's shape? |
| K-006 | Five-letter message table + `SCHEDULING_BASE 0xF00` namespace | I | **02 §1.2**, 00 §1.3, 99 §1.1 | `com.h:801-807` | What is the complete wire surface? |
| K-007 | Notify-before-call + CLOCK-only action, rest silent | M | **02**, 11 (door use) | `main.c:44-55` | Which arrivals never reach dispatch? |
| K-008 | FROM_KERNEL gate for NO_QUANTUM + fakery→EPERM | K | **02**, 08 (trust asymmetry) | `main.c:70-83` | Why is one letter source-checked at the door? |
| K-009 | SUSPEND = sole no-reply call path; `continue` = notify no-reply | K | **02**, plan §5.3 | `main.c:54,76,90-93` | When does the server legally stay silent? |
| K-010 | `reply()` via `ipc_send` + log-on-failure | M | **02** | `main.c:101-106` | How are answers delivered and what if delivery fails? |
| K-011 | `no_sys` → ENOSYS for wild call numbers | M | **02** | `utility.c:18-23` | What happens on an unknown letter? |
| K-012 | `schedproc` 7 live fields (endpoint/parent/flags/max_priority/priority/time_slice/cpu) | D | **03** | `schedproc.h:23-32` | What does "a process" look like to the scheduler? |
| K-013 | `cpu_mask` dead field (declared, never written; FIXME only) | A | **03**, plan §7.3 (S-3) | `schedproc.h:33-35`, `schedule.c:185` | Why does the Rust record drop a C field? |
| K-014 | `IN_USE 0x1` single-bit occupancy | D | **03**, 04 | `schedproc.h:39` | How is liveness represented? |
| K-015 | Slot derivation `_ENDPOINT_P` + negative→EBADEPT | M | **04** | `utility.c:29-35,46-52` | How does an endpoint become a slot index? |
| K-016 | `sched_isokendpt` 4-way verdict (OK/EBADEPT/EINVAL/EDEADEPT×2) | M | **04** | `utility.c:29-41` | What does "occupied and genuine" check? |
| K-017 | `sched_isemtyendpt` 3-way verdict (vacant check) | M | **04** | `utility.c:46-56` | What does "vacant and addressable" check? |
| K-018 | `accept_message` PM/RS whitelist → EPERM | K | **04**, 06/07/08 (uses) | `utility.c:61-74` | Who is even allowed to knock? |
| K-019 | 16 queues + TASK_Q/MAX_USER_Q/USER_Q/MIN_USER_Q ladder | C | **05**, 99 (table) | `config.h:66-71` | What are the queue numbers and their order? |
| K-020 | `max_priority` (ceiling) vs `priority` (position) split | C | **05**, 06/08/11 (uses) | `schedproc.h:29-30` | Which number caps, which number moves? |
| K-021 | `time_slice` unit = milliseconds (not ticks) | K | **05** (correction of draft) | `schedule.c:41` + `p_quantum_size_ms` (kernel) | What unit does a quantum carry? |
| K-022 | `DEFAULT_USER_TIME_SLICE 200` + `USER_QUANTUM 200` init values | D | **05**, 06 (init use) | `schedule.c:41`, `config.h:74` | What does a newborn process run with? |
| K-023 | `is_system_proc` = parent == RS_PROC_NR | M | **05**, 10 (use) | `schedule.c:44` | How is "system process" decided? |
| K-024 | nice→queue conversion concept (PM-side formula) | C | **05** (concept), 13 (caller) | `pm/utility.c:nice_to_priority` (counterpart) | How does -20..20 become a queue? |
| K-025 | Takeover chain (assert → whitelist → vacant → fill → EINVAL → special cases → branches → schedctl → IN_USE → pick → fanout+retry → scheduler=4) | M | **06** | `schedule.c:140-249` | What is the exact birth order of a scheduled process? |
| K-026 | START (explicit quantum/prio) vs INHERIT (copy parent) branch split | M | **06** | `schedule.c:189-214` | Where do initial values come from? |
| K-027 | init self-parent special case (endpoint==parent → USER_Q/200/BSP) | K | **06**, 13 (INIT exception) | `schedule.c:171-187` | How is the first process bootstrapped? |
| K-028 | EBADCPU retry loop (`cpu=CPU_DEAD`, re-pick) | M | **06**, 10 (marking) | `schedule.c:227-231` | What happens when the kernel refuses a CPU? |
| K-029 | Stop chain (whitelist → isok → ledger-- → flags=0), no fanout | M | **07** | `schedule.c:112-135` | What is released on death and what is not? |
| K-030 | Demotion (`priority<MIN_USER_Q → +=1`, floor) + local fanout | M | **08** | `schedule.c:87-107` | What does quantum exhaustion change? |
| K-031 | Nice transaction (snapshot → set both → fanout → rollback on failure) | M | **08** | `schedule.c:254-292` | How is a ceiling change made atomic? |
| K-032 | Trust asymmetry (NO_QUANTUM door-checked, no whitelist; nice fully checked) | K | **08**, 02 (door) | `main.c:70-83` vs `schedule.c:261-275` | Why does one path skip the whitelist? |
| K-033 | Fanout aggregation (SCHEDULE_CHANGE_* mask, -1 keep, niced derivation, sys_schedule) | M | **09** | `schedule.c:22-30,297-328` | How are decisions packed for the kernel? |
| K-034 | `schedule_process` vs `sched_proc` name collision (two layers) | C | **09**, 12 | `schedule.c:297` vs `system.c:642` | Which function lives on which side? |
| K-035 | `do_schedule` caller==p_scheduler → else EPERM; sched_proc applies (EINVAL/EBADCPU/requeue/MF_NICED) | I | **09**, 12 | `do_schedule.c:8-31`, `system.c:642-723` | What does the kernel check before applying? |
| K-036 | `pick_cpu` 3 rules (single→BSP; system→BSP; else least-loaded non-BSP, BSP fallback) | M | **10** | `schedule.c:48-81` | Where does a new process run? |
| K-037 | Ledger `cpu_proc[]` + `CPU_DEAD -1` sentinel (++/--/mark) | D | **10**, 06/07 (mutations) | `schedule.c:37-46,77,130,229` | How is load counted? |
| K-038 | `cpu_is_available` + CONFIG_SMP compile gate → runtime topo | A | **10**, 01 (topo source) | `schedule.c:39,50-80` | Which CPUs may be chosen? |
| K-039 | `balance_queues` scan (IN_USE + above-ceiling → +1 level + local fanout, re-arm) | M | **11** | `schedule.c:353-369` | How does recovery work and how often? |
| K-040 | Demote-fast/promote-slow asymmetry rationale (anti-oscillation) | C | **11**, 08 (demote half) | `schedule.c:348-352` comment | Why is recovery one level per 5 s? |
| K-041 | `do_schedctl` registration (flags check, isokendpt, KERNEL branch vs user branch, p_scheduler=caller/NULL) | I | **12** | `do_schedctl.c:7-49` | How does scheduling authority change hands? |
| K-042 | `notify_scheduler` (RTS_NO_QUANTUM dequeue, 7 accounting fields, reset, mini_send FROM_KERNEL) | I | **12** | `proc.c:1860-1891` | What does the kernel report on exhaustion? |
| K-043 | `proc_no_time` PREEMPTIBLE two-branch | M | **12** | `proc.c:1893-1910` | When is the scheduler woken vs skipped? |
| K-044 | libsys trio (`sched_inherit`/`sched_start` with NONE/KERNEL short-circuits, `sched_stop`) | I | **13** | `sched_start.c:11-76`, `sched_stop.c:9-29` | How are requests packed on the caller side? |
| K-045 | PM caller face (`sched_init`, `sched_start_user` + PRIV_PROC→INIT exception, `sched_nice`, exit `sched_stop`, `mp_scheduler`) | I | **13** | `pm/schedule.c:20-112`, `pm/forkexit.c:425` | When does PM knock and with what? |
| K-046 | RS caller face (`sched_init_proc` 6-arg direct, parent=RS, `r_scheduler/r_priority/r_quantum/r_cpu`, two stop paths with warn-vs-code split) | I | **14** | `rs/utility.c:364-384`, `rs/manager.c:461`, `rs/request.c:342` | How do system processes arrive and leave? |
| K-047 | Five-table coherence (endpoint/parent must match kernel+PM+VM+VFS views) | K | **99 §1.3**, 04/06 (guards) | `schedproc.h:24-25` | What invariant do the gate checks protect? |
| K-048 | Single-threaded event-loop execution model (vs kernel SMP concurrency) | C | **99 §3**, 00 §1.1 | `main.c:35-96` (loop shape) | What concurrency exists inside SCHED? (None.) |

### 2.2 New entries (from §3 coverage audit; all need evidence anchors, none may appear from thin air)

| ID | Name | Type | Evidence anchor | Why it belongs (reader gain) |
|----|------|------|-----------------|------------------------------|
| K-049 | Fanout re-picks CPU on every call (F1) | M | `schedule.c:302` (`pick_cpu(rmp)` first line of `schedule_process`) | Explains ledger churn; every fanout caller must know the side effect. |
| K-050 | `schedule_process_migrate` dead macro (F2) | T | `schedule.c:34-35` defined; `grep` shows zero callers | Stops readers hunting a migration path that does not exist. |
| K-051 | Takeover double-pick (F3) | M | `schedule.c:226` + `:302` | Explains why one birth bumps the ledger twice. |
| K-052 | `USER_DEFAULT_CPU -1` keep-semantics on the wire | I | `config.h:77` | The `-1` convention is not only prio/quantum; cpu has a named keep value. |
| K-053 | SCHEDULE_CHANGE_* flag algebra (PRIO/QUANTUM/CPU/ALL) | I | `schedule.c:22-30` | The mask is the fanout's vocabulary; currently inline in 09 body, deserves a named contract row. |
| K-054 | niced derivation `max_priority > USER_Q` + MF_NICED end-to-end | K | `schedule.c:319` + `system.c` MF_NICED arm (see edge E-SCHEDNICED) | The one-bit policy hint crossing into the kernel; behavior gap lives here. |
| K-055 | Accounting 7-field shape (acnt_queue/deqs/ipc_sync/ipc_async/preempt/cpu/cpu_load) mostly unread by SCHED | I | `proc.c:1860-1891` | Readers must know the notify carries data SCHED deliberately ignores (for now). |
| K-056 | Alarm/panic policy (arm-fail and re-arm-fail are fatal; handler-fail is a reply code) | K | `schedule.c:340-341,367-368` vs handler `return rv` sites | Two failure severities, never mixed: startup fatal vs runtime replied. |
| K-057 | Static zero-init as ledger genesis (no clear loop) | K | `schedproc.h:36`, `schedule.c:16,46`, `main.c:17` | The "all free / all alive" start is implicit; ports must reproduce it. |
| K-058 | Wire struct field inventory (4 start fields, stop endpoint, nice endpoint+maxprio, reply scheduler) | I | `ipc.h:1428-1445,1819-1828,1904-1913` | Exact on-wire shapes in one place; client docs cite, never redefine. |
| K-059 | Restart-stateful survival (table persists across SEF restart) | K | `main.c:115` (`SEF_CB_INIT_RESTART_STATEFUL`) | A restart does not empty the table; newcomers must not assume a fresh table. |
| K-060 | Test seam inventory (81-test baseline, mock transport, PM↔SCHED loopback gap E5(e)) | E | `todo.md` baseline 81 passed; `server.rs` mock tests; `edge_todo.md` E5(e) | Where confidence comes from and where the loopback hole is. |
| K-061 | Build artifact (`servers/sched/Makefile`: 3 SRCS + libsys) | T | `minix3/minix/servers/sched/Makefile` | The stage's build unit, for the non-C checklist. |

Statistics: 61 entries — 48 stock + 13 new. By type: C 9, M 24, D 4, I 12, K 9, A 2, T 2, E 1. By existing doc: 02 owns most interface entries; 06 owns the longest chain; 08 currently overloads two mechanisms (split driver, §4).

---

## 3. Coverage Audit

Topic universe built from four roads: (a) C symbols (16 server functions + 14 counterpart functions + 7 wire structs + 5+6+4+3 constants), (b) OS-theory counterparts (scheduler state machine, address-space-free policy server, authority/delegation, load accounting, periodic rebalancing), (c) non-C artifacts (§3.3), (d) stage-boundary contracts (master-plan row 06, edge_todo SCHED items).

### 3.1 Gap table (in universe, told nowhere as its own contract)

| # | Topic | Evidence | Recommendation → pool |
|---|-------|----------|----------------------|
| G-01 | Fanout re-pick side effect (F1) | `schedule.c:302` | New K-049 → N-10 contract (fanout doc owns the warning box). |
| G-02 | Dead `schedule_process_migrate` macro (F2) | `schedule.c:34-35` + zero-caller grep | New K-050 → N-10 "deleted with reason" row. |
| G-03 | Takeover double-pick (F3) | `schedule.c:226` + `:302` | New K-051 → N-06 contract (with pointer to N-10). |
| G-04 | `USER_DEFAULT_CPU -1` wire keep value | `config.h:77` | New K-052 → N-10 (with N-05 hub pointer). |
| G-05 | Accounting fields' unread status made explicit | `proc.c:1860-1891` | New K-055 → N-13 (kernel contract owns the "carried but unread" table). |
| G-06 | Panic-vs-reply failure severity split | `schedule.c:340-341,367-368` vs handler returns | New K-056 → N-01 (startup fatal) + N-02 (runtime replied). |
| G-07 | Zero-init genesis (no clear loop) | `schedproc.h:36`, `schedule.c:16,46` | New K-057 → N-04 (table doc owns genesis). |
| G-08 | Wire field inventory in one place | `ipc.h` four structs | New K-058 → N-02 (surface doc owns the wire table; 13/14 cite it). |
| G-09 | Stateful-restart table survival | `main.c:115` | New K-059 → N-01. |
| G-10 | Explicit test-seam + loopback gap pointer | `todo.md`, `edge_todo.md` E5(e) | New K-060 → N-99 (global test paragraph) + N-14/N-15 pointers. |

No gap is judged "belongs to another stage" or "will not do": all ten are in-stage. G-04/G-08 could be mistaken for minix-types territory; they stay in-stage as *contract rows* (the constant authority remains `minix-types`; the docs only tabulate usage).

### 3.2 Duplication table (one home, rest become pointers)

| # | Topic repeated | Current repeat sites | New home (main telling) |
|---|---------------|---------------------|-------------------------|
| R-01 | Queue ladder / ceiling-vs-position / ms unit | 05 defines; 06/08/09/10/11 reuse inline | N-05 home; all others cite, max one sentence + pointer |
| R-02 | Gate verdicts (isok/isemty/whitelist) | 04 defines; 06/07/08 re-derive | N-04 home; handlers cite verdict names only |
| R-03 | NO_QUANTUM trust model | 02 (door) + 08 (asymmetry) overlap ~40% | N-02 owns the door, N-08 owns the handler; shared paragraph moved to N-02, N-08 cites |
| R-04 | CLOCK→balance door | 02 + 11 overlap | N-02 owns the door (one line), N-11 owns the room |
| R-05 | INIT self-parent exception | 06 + 13 overlap | N-06 owns the mechanism, N-14 owns the caller-side trigger (PRIV_PROC→INIT); each one paragraph, cross-cited |
| R-06 | pick_cpu three rules | 06 (call) + 09 (re-pick line) + 10 (definition) | N-11 home; N-06/N-10 cite rule names only |
| R-07 | `schedule_process` vs `sched_proc` naming | 09 + 12 both explain | N-10 owns the对照 table; N-13 cites it |
| R-08 | Five-letter table | 00 + 02 + 99 repeat the five names | N-02 home; 00/99 list names only, no semantics |

### 3.3 Out-of-scope table (told past its boundary today)

| # | Topic | Current site | Correct home |
|---|-------|-------------|--------------|
| O-01 | Kernel queue impl (enqueue/dequeue/pick_proc, RTS machine) | 12 skirts it | `01-stage-kernel/11-scheduling-primitives.md` (sovereign); N-13 keeps one paragraph + pointer |
| O-02 | PM nice user surface (do_getsetpriority, PRIO_MIN/MAX, permission checks) | 05/13 touch it | `04-stage-pm/16-scheduling.md` (sovereign); N-05/N-14 keep formula + pointer |
| O-03 | RS slot config (r_* origins, system.conf loading) | 14 touches it | `03-stage-rs/08-rs-slot-config.md` (sovereign); N-15 keeps entry point + pointer |
| O-04 | Live-update migration flow | 14 touches it | `03-stage-rs/16-rs-live-update.md` (sovereign); N-15 keeps the two cancel sites + pointer |
| O-05 | Kernel SMP migration (smp_schedule_migrate_proc) | 10 touches it | `01-stage-kernel/16-smp.md` (sovereign); N-11 keeps first-choice policy + pointer |

### 3.4 Non-C artifacts: ten fixed questions, each answered

| # | Checklist item | Finding (path or negative with evidence) | Where it is taught |
|---|---------------|------------------------------------------|--------------------|
| N-01 | Link + load | No SCHED linker script; standard `minix.service.mk` service link (`servers/sched/Makefile`: `PROG=sched`, `SRCS=main.c schedule.c utility.c`, `LDADD+=-lsys`) | N-01 (one paragraph: nothing special, standard service link) |
| N-02 | Image + memory layout | No SCHED-owned layout; birth image assembled by RS/boot image (master-plan boot two-layer semantics); static tables are BSS (`schedproc.h:36`, `schedule.c:16,46`) | N-01 (genesis paragraph, K-057) |
| N-03 | ASM entry + trap entry | None in stage: no `.s` files under `servers/sched/` (verified `ls`); user↔kernel traps belong to 01-stage-kernel | N-99 intentional-omissions row |
| N-04 | Boot assembly | None owned; RS loads SCHED (master-plan row 06); SCHED's first owned act is `sef_local_startup` | N-01 (boundary sentence) |
| N-05 | Build + toolchain | `servers/sched/Makefile` only (above); rest is tree-wide mk | N-01 (one paragraph, K-061) |
| N-06 | Cross-module iface + wire format | 4 wire structs `ipc.h:1428-1445,1819-1828,1904-1913` + `SCHEDULING_BASE` namespace + `sys_schedctl`/`sys_schedule` kernel calls + `_taskcall` client transport | N-02 owns the wire table (K-058); N-13/N-14/N-15 cite |
| N-07 | Error paths | Handler reply codes (EINVAL/EBADEPT/EDEADEPT/EPERM/ENOSYS/EBADCPU) + fakery-log path + warn-and-continue + panics (machine/alarm) | N-04 (gate verdicts) + N-02 (door errors) + N-99 (full error table) |
| N-08 | Shutdown + exit | No SCHED shutdown path exists in C (no exit handler; death = `do_stop_scheduling` per process + `flags=0`); service termination via RS stop sites | N-07 (per-process release) + N-15 (RS termination); N-99 states "no service-level shutdown" explicitly |
| N-09 | Concurrency + sync | None inside SCHED (single-threaded loop, `main.c:35-96`); SMP concurrency lives kernel-side; ledger races impossible by construction | N-99 execution-model paragraph (K-048) + N-11 (how multi-core facts enter single-thread policy) |
| N-10 | Test infra + simulator scripts | Rust: `server.rs` mock-transport tests (81 baseline per `todo.md`); no QEMU scenario covers PM↔SCHED loopback (edge E5(e) open) | N-99 test paragraph (K-060); no new test files in rebuild (docs only) |

---

## 4. New Catalog

Seventeen docs: `00` + `01–15` + `99`, in five parts. One insertion cascade (old 08 splits; old 09–14 shift by one) — the only renumber in the proposal, with full migration in §8. Old 00/99 keep their numbers.

| New # | Title | One-line charge | Part |
|-------|-------|----------------|------|
| 00 | sched-overview | Why a user-space policy server exists; boot lifeline; doc map | 0 map |
| 01 | sched-startup | Birth handoff: SEF registration, topology read-once, alarm arming, panic policy, restart survival | A birth |
| 02 | sched-loop-dispatch | The loop owns everything: receive, notify doors, five-letter dispatch, FROM_KERNEL gate, reply/SUSPEND/no_sys, wire table | A birth |
| 03 | schedproc-record | The 7-field record + IN_USE + dropped cpu_mask (S-3) | B state |
| 04 | schedproc-table-gates | Genesis (zero-init), slot math, isok/isemty verdicts, PM/RS whitelist, error verdict table | B state |
| 05 | priority-timeslice-model | The hub: 16-queue ladder, ceiling-vs-position, ms unit, init values, is_system_proc, nice formula (pointer) | C model |
| 06 | start-takeover | The birth chain: two letters one door, four checks, three fills, schedctl, IN_USE, pick + EBADCPU retry, scheduler=4 | D lifecycle |
| 07 | stop-release | The death chain: two checks, ledger--, flags=0, no fanout, start symmetry | D lifecycle |
| 08 | noquantum-demotion | Exhaustion: door-checked trust, floor, local fanout | D lifecycle |
| 09 | nice-regrade | Ceiling change: full checks, snapshot/set/fanout/rollback | D lifecycle |
| 10 | schedule-fanout | Packing: mask algebra, -1 keep, niced, sys_schedule; kernel apply (do_schedule/sched_proc); dead migrate macro deleted; re-pick warning | E mechanism |
| 11 | pick-cpu-ledger | Choice: three rules, ledger + CPU_DEAD, availability, USER_DEFAULT_CPU, compile-gate→runtime | E mechanism |
| 12 | balance-timers | Recovery: 5 s rhythm, scan, +1/round, re-arm, anti-oscillation rationale | E mechanism |
| 13 | kernel-contract | Bidirectional: schedctl registration + notify 7-field + proc_no_time + reset/mini_send + name-collision table | F contracts |
| 14 | pm-client | Caller face: libsys trio, PM init/fork/nice/exit, INIT exception, mp_scheduler | F contracts |
| 15 | rs-client | Caller face: sched_init_proc 6-arg, parent=RS, slot fields, two stop sites (warn vs code) | F contracts |
| 99 | global-concepts | Constants, five-table coherence, errors, execution model, omissions, test seam | G close |

Reading paths:

- Main line (runtime order): 00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15 → 99.
- Skeptical-of-policy line (mechanism first): 00 → 05 → 10 → 11 → 12 → 06 → others.
- Client-first line (integrators): 00 → 02 (wire table) → 14 → 15 → 13 → 06.
- Skippable on first pass: 12 (timers, if no SMP interest), 99 (reference), 03 §cpu_mask archaeology (S-3 box).

Parallel groups (no linear order faked): {06, 07, 08, 09} are four independent handlers sharing doors (02), gates (04), hub (05); {14, 15} are two independent callers sharing the wire table (02). Representative-member rule: 06 is the fully-worked member (longest chain); 07/08/09 are told as diffs against 06's chain shape (checks → mutate → fanout-or-not → reply-or-not).

---

## 5. Per-Doc Contracts (all seven elements mandatory; Phase B writes from these alone)

### 00-sched-overview

- Charge: why SCHED exists and where every mechanism lives; the reader leaves with the lifeline and the map, nothing else.
- Covers: K-001, K-006 (names only), K-048 (one paragraph), lifeline T-01..T-22 compressed to the S1/S2 diagram, parts A–G map (R-08: names only).
- Not covering: any handler internals (→ 06–09), any gate verdict (→ 04), any constant value (→ 05/99), any counterpart impl (→ 01-stage-kernel/11, 04-stage-pm/16, 03-stage-rs/08). Startup detail → 01; dispatch detail → 02.
- Requires: nothing (entry point).
- Required by: every doc (map only, not content).
- Ground truth: `servers/sched/` file map (main.c 137 / schedule.c 369 / utility.c 74 / headers 78 = 658); `com.h:801-807`; Rust `os/servers/sched/src/` 19 files.
- Knowledge rows: K-001 (concept) — `main.c:1-3` — hub — stock 00 §1.1; K-006-names (interface) — `com.h:801-807` — namespace shape only — stock 00 §1.3; K-048 (concept) — `main.c:35-96` — one paragraph — stock 99 §3.
- Acceptance: reader can draw the S1/S2 lifeline from memory with file:line for each station; zero forward references (only names of later docs, no mechanism); ≤ 100 lines.

### 01-sched-startup

- Charge: what SCHED owns in its first 30 lines of life, and what it inherits from RS without owning.
- Covers: K-002, K-003, K-004 (call site only), K-056 (fatal half), K-059, K-061, N-01/N-02/N-04/N-05 answers (link/load/image/boot/build), T-01..T-06.
- Not covering: loop body (→ 02), alarm definition/re-arm (→ 12), topology uses (→ 11), table genesis detail (→ 04; this doc states the handoff in one line).
- Requires: 00 (lifeline position).
- Required by: 02 (loop starts where this ends), 11 (topology source), 12 (alarm origin).
- Ground truth: `main.c:17,22-33,108-137`; `schedule.c:334-342` (call-site view only); `servers/sched/Makefile`; `type.h:122-125`.
- Knowledge rows: K-002 (M) — `main.c:111-121` — registration is this doc's core — stock 01 §1.2; K-003 (M) — `main.c:126-131` — read-once stated, uses deferred — stock 01; K-004-call-site (M) — `schedule.c:334-342` — arming stated, rhythm deferred — stock 01/11 split; K-056-fatal (K) — `schedule.c:340-341` — panic policy — new; K-059 (K) — `main.c:115` — survival — new; K-061 (T) — `Makefile` — build unit — new.
- Acceptance: lists every panic site with line; states the RS handoff boundary in one sentence; a port checklist (register → read topo → arm alarm → loop) each with an anchor.

### 02-sched-loop-dispatch

- Charge: the loop is the server: every arrival classified, every answer rule stated, the full wire table in one place.
- Covers: K-005, K-006 (full), K-007, K-008, K-009, K-010, K-011, K-056 (reply half), K-058, T-07..T-17.
- Not covering: handler bodies (→ 06–09, one line each naming the door only), balance execution (→ 12), field semantics of wire structs (→ 10/13/14; this doc tabulates shape only), startup (→ 01).
- Requires: 01 (loop entry).
- Required by: 06/07/08/09 (doors), 12 (CLOCK door), 14/15 (wire shapes).
- Ground truth: `main.c:35-106`; `utility.c:18-23`; `com.h:801-807`; `ipc.h:1428-1445,1819-1828,1904-1913` (shape table); `SUSPEND` (`com.h:1151` via minix-types `ipc_server.rs:76`).
- Knowledge rows: K-005 (M) — `main.c:35-96` — loop shape — stock 01/02 dedup R-03; K-006-full (I) — `com.h:801-807` — five rows with numbers — stock 02 §1.2; K-007 (M) — `main.c:44-55` — notify precedence — stock 02; K-008 (K) — `main.c:70-83` — gate — stock 02; K-009 (K) — `main.c:54,76,90-93` — silence rules — stock 02; K-010 (M) — `main.c:101-106` — reply — stock 02; K-011 (M) — `utility.c:18-23` — ENOSYS — stock 02; K-058 (I) — `ipc.h` structs — wire table — new; K-056-reply (K) — handler `return rv` sites — replied severity — new.
- Acceptance: a decision table (arrival × condition → action × reply?) with all 8 rows (CLOCK/other-notify/5 letters/wild); wire table with field names + widths; zero handler internals.

### 03-schedproc-record

- Charge: what one scheduler row is, field by field, and which eighth field was deliberately dropped.
- Covers: K-012, K-013, K-014.
- Not covering: slot math/verdicts (→ 04), value semantics (→ 05), writers per handler (→ 06–09; this doc lists writer names in a table, one row each, no flow).
- Requires: 00 (five-table context), 02 (who the messages name).
- Required by: 04 (operates on these fields), 05 (gives them meaning).
- Ground truth: `schedproc.h:23-39`; `BITMAP_CHUNKS(CONFIG_MAX_CPUS)`; `os/servers/sched/src/schedproc.rs` (S-2/S-4 newtypes as ARCH boxes).
- Knowledge rows: K-012 (D) — `schedproc.h:23-32` — 7 fields — stock 03; K-014 (D) — `schedproc.h:39` — occupancy — stock 03; K-013 (A) — `schedproc.h:33-35` + `schedule.c:185` + zero-write grep — S-3 elimination box — stock 03 + plan §7.3.
- Acceptance: field table (field × C type × Rust type × writer docs); S-3 box states declaration + zero-write evidence + elimination verdict; 64-bit width note (S-10).

### 04-schedproc-table-gates

- Charge: the gatehouse: how endpoints become slots and who may knock.
- Covers: K-015, K-016, K-017, K-018, K-047 (guard view), K-057, verdict→errno sub-table of N-07.
- Not covering: field detail (→ 03), per-handler flows (→ 06–09), full error table (→ 99; this doc owns verdict rows only).
- Requires: 03 (record shape), 02 (message names).
- Required by: 06/07/08/09 (every chain starts here).
- Ground truth: `schedproc.h:36` (`schedproc[NR_PROCS]`); `utility.c:29-74`; `_ENDPOINT_P` (`endpoint.h`); `errno` EBADEPT/EINVAL/EDEADEPT/EPERM.
- Knowledge rows: K-015 (M) — `utility.c:29-35` — slot math — stock 04; K-016 (M) — `utility.c:29-41` — 4-way — stock 04; K-017 (M) — `utility.c:46-56` — vacant — stock 04; K-018 (K) — `utility.c:61-74` — whitelist — stock 04; K-057 (K) — zero-init sites — genesis — new; K-047-guard (K) — `schedproc.h:24-25` — invariant guarded here — stock 99 §1.3.
- Acceptance: verdict decision trees for isok/isemty (all branches with errno); whitelist table (PM_PROC_NR/RS_PROC_NR allow, else deny); genesis paragraph (K-057) a port can reproduce.

### 05-priority-timeslice-model

- Charge: the hub: every number every handler uses, defined once.
- Covers: K-019, K-020, K-021, K-022, K-023, K-024 (concept + pointer), K-052 (pointer to N-11), K-053 names (pointer to N-10).
- Not covering: per-handler uses (→ 06/08/09/10/11/12; this doc never names a handler beyond a use-table), kernel-side validation (→ 10), PM user surface (→ 04-stage-pm/16; formula + pointer only).
- Requires: 03 (fields), 04 (slots exist).
- Required by: 06/08/09/10/11/12/13 (hub — the most-cited doc in the stage).
- Ground truth: `config.h:66-77`; `schedule.c:41,44`; `pm/utility.c:nice_to_priority` (counterpart, concept only).
- Knowledge rows: K-019 (C) — `config.h:66-71` — ladder — stock 05; K-020 (C) — `schedproc.h:29-30` — ceiling vs position — stock 05; K-021 (K) — `schedule.c:41` — ms unit + ticks-correction note — stock 05; K-022 (D) — `schedule.c:41`, `config.h:74` — init values — stock 05; K-023 (M) — `schedule.c:44` — system test — stock 05; K-024 (C) — counterpart pointer — stock 05.
- Acceptance: constant table (name × value × authority × user); ceiling/position diagram; the ms correction stated with both anchors; use-table (value × using docs) with no per-use explanation.

### 06-start-takeover

- Charge: the birth chain, end to end, including the two ugly truths (double-pick, EBADCPU retry).
- Covers: K-025, K-026, K-027, K-028, K-051, T-18.
- Not covering: dispatch detail (→ 02), client packing (→ 14/15), fanout internals (→ 10; one line + pointer), cpu policy (→ 11; one line + pointer), stop symmetry detail (→ 07; one paragraph).
- Requires: 02 (doors), 04 (gates), 05 (hub values).
- Required by: 07 (symmetric counterpart), 14/15 (server side of their calls).
- Ground truth: `schedule.c:140-249`; `ipc.h:1428-1445` (shape, cite N-02); `sys_schedctl` (behavior, cite N-13).
- Knowledge rows: K-025 (M) — `schedule.c:140-249` — 10-link chain — stock 06; K-026 (M) — `schedule.c:189-214` — branch split — stock 06; K-027 (K) — `schedule.c:171-187` — init exception — stock 06; K-028 (M) — `schedule.c:227-231` — retry — stock 06; K-051 (M) — `schedule.c:226+302` — double-pick warning box — new.
- Acceptance: numbered 10-link chain each with line anchor; branch diff table (START vs INHERIT: source of prio/quantum/parent-check); EBADCPU walk-through with ledger states; "fork sub-line" diagram (PM do_fork → sched_start_user → INHERIT → this chain → mp_scheduler) with per-hop owner doc.

### 07-stop-release

- Charge: the death chain and why it is deliberately smaller than birth.
- Covers: K-029, start-symmetry diff, N-08 shutdown answer (per-process half).
- Not covering: client short-circuits (→ 14/15), fanout/cpu detail (→ 10/11; states "no fanout" with reason), takeover flow (→ 06; symmetry paragraph only).
- Requires: 02 (STOP door), 04 (gates), 06 (symmetric source).
- Required by: 14/15 (server side of their cancel calls).
- Ground truth: `schedule.c:112-135`.
- Knowledge rows: K-029 (M) — `schedule.c:112-135` — 4-link chain — stock 07.
- Acceptance: 4-link chain with anchors; symmetry table (start × stop: check/check, fill/clear, pick/unpick, fanout/none, scheduler-field/none); "why no fanout" reason (dead processes need no kernel state); ledger-before/after example.

### 08-noquantum-demotion

- Charge: what quantum exhaustion changes and why this path trusts the door, not the whitelist.
- Covers: K-030, K-032 (handler half), T-20.
- Not covering: fanout internals (→ 10), accounting field meanings (→ 13), PM caller surface (→ 14), recovery (→ 12; one line + pointer).
- Requires: 02 (door + gate), 04 (isok), 05 (ceiling/position/floor).
- Required by: 12 (demote half of the asymmetric pair).
- Ground truth: `schedule.c:87-107`.
- Knowledge rows: K-030 (M) — `schedule.c:87-107` — demote+fanout — stock 08-half; K-032-handler (K) — `schedule.c:87-96` vs `:261-275` — asymmetry — stock 08.
- Acceptance: trust diagram (kernel flag → m_source → isok, no whitelist) with T-15/N-02 pointer; floor proof walk (MIN_USER_Q stops); invalid-endpoint warning path with log line anchor.

### 09-nice-regrade

- Charge: how a ceiling change is made atomic, and what rolls back.
- Covers: K-031, T-21.
- Not covering: fanout internals (→ 10), PM packing/nice formula (→ 14 + 04-stage-pm/16), demotion (→ 08; diff paragraph only).
- Requires: 02 (door), 04 (gates), 05 (ceiling semantics).
- Required by: 14 (server side of sched_nice).
- Ground truth: `schedule.c:254-295`.
- Knowledge rows: K-031 (M) — `schedule.c:254-292` — transaction — stock 08-half (moved home).
- Acceptance: transaction table (snapshot → set → fanout → commit/rollback) with line per step; rollback test sketch (fail fanout → old values restored); diff-vs-demotion table (trigger/fields/checks/failure-mode).

### 10-schedule-fanout

- Charge: the single narrow waist every decision squeezes through — and its two traps.
- Covers: K-033, K-034, K-049, K-050, K-053, K-054, kernel-apply half of K-035.
- Not covering: registration semantics (→ 13; one line), cpu policy (→ 11; re-pick stated as a call, policy deferred), per-handler call sites (→ 06–09; use-table only).
- Requires: 05 (values + niced concept), 06 (full-fanout example).
- Required by: 06/08/09/12 (all fanout callers), 13 (apply side).
- Ground truth: `schedule.c:20-35,297-328`; `do_schedule.c:8-31`; `system.c:642-723` (apply arms); `syslib.h` sys_schedule decl.
- Knowledge rows: K-033 (M) — `schedule.c:297-328` — aggregation — stock 09; K-053 (I) — `schedule.c:22-30` — mask algebra — new; K-049 (M) — `schedule.c:302` — re-pick warning box — new; K-050 (T) — `schedule.c:34-35` — dead macro, deleted with reason — new; K-054 (K) — `schedule.c:319` — niced + E-SCHEDNICED pointer — new; K-034 (C) — `schedule.c:297` vs `system.c:642` — name table — stock 09; K-035-apply (I) — `do_schedule.c` + `system.c` — kernel checks — stock 09.
- Acceptance: mask truth table (flag × field × -1 behavior); message packing diagram (field ← source); kernel apply checklist (EPERM/EINVAL/EBADCPU/requeue/MF_NICED); warning box F1 quoted with line; deletion row F2 with zero-caller evidence.

### 11-pick-cpu-ledger

- Charge: where processes run and how the books are kept (and why the books drift).
- Covers: K-036, K-037, K-038, K-052, ledger-mutation map (06 pick, 07 unpick, 10 re-pick, 06 DEAD-mark), S-4/S-5 ARCH boxes.
- Not covering: kernel migration (→ 01-stage-kernel/16; one line), fanout packing (→ 10), retry loop flow (→ 06; marking action only).
- Requires: 01 (topology source), 05 (system test + keep value).
- Required by: 06 (choice call), 10 (re-pick call).
- Ground truth: `schedule.c:37-81,130,229`; `type.h:122-125`; `config.h:77`; `os/servers/sched/src/cpu.rs` (Option-ledger, S-4).
- Knowledge rows: K-036 (M) — `schedule.c:48-81` — 3 rules in C order — stock 10; K-037 (D) — `schedule.c:37-46` — ledger+sentinel — stock 10; K-038 (A) — `schedule.c:39,50-80` — availability + gate→runtime — stock 10; K-052 (I) — `config.h:77` — keep value — new.
- Acceptance: rule flowchart with tie-break (strict `>` keeps first minimum, `schedule.c:71`) and BSP-fallback worked example; ledger state machine (++/--/DEAD) with every mutation site; drift note (re-pick bumps without matching decrements — pointer to N-10 F1 box); S-4/S-5 boxes with code pointers.

### 12-balance-timers

- Charge: the slow half of scheduling: why recovery is periodic, partial, and panic-armed.
- Covers: K-039, K-040, alarm lifecycle (arm → fire → re-arm), K-004 definition half.
- Not covering: clock kernel side (→ 01-stage-kernel/15), setalarm impl, fanout detail (→ 10), demotion flow (→ 08; asymmetry paragraph only).
- Requires: 02 (CLOCK door), 05 (ceiling as stop condition).
- Required by: none (terminal mechanism; cited by 08's asymmetry paragraph).
- Ground truth: `schedule.c:18,334-369`; `main.c:66-72` (door, cite N-02); `sysutil.h` setalarm/Hz decls; S-9 policy-replaceability box.
- Knowledge rows: K-039 (M) — `schedule.c:353-369` — scan+fanout+rearm — stock 11; K-040 (C) — `schedule.c:348-352` — asymmetry rationale — stock 11; K-004-def (M) — `schedule.c:334-342` — arm — stock split.
- Acceptance: rhythm diagram (5 s × sys_hz → alarm → scan → re-arm); scan pseudo-walk over NR_PROCS with the two predicates; oscillation argument (fast-down/slow-up) in five lines; S-9 box (default policy replaceable, Rust trait pointer).

### 13-kernel-contract

- Charge: the two directions across the privilege line, with the unread-mail table.
- Covers: K-041, K-042, K-043, K-055, K-035-check half, K-034 pointer, O-01 boundary paragraph.
- Not covering: primitive internals (→ 01-stage-kernel/11; cite, never re-explain), mini_send mechanics, caller packing (→ 14/15).
- Requires: 10 (fanout messages exist), 05 (ranges).
- Required by: 06 (takeover's schedctl call), 08 (notify's arrival).
- Ground truth: `do_schedctl.c:7-49`; `proc.c:1860-1910`; `do_schedule.c:8-31` (check row); `system.c:642-723` (apply rows, cite); `com.h:449`.
- Knowledge rows: K-041 (I) — `do_schedctl.c` — registration branches — stock 12; K-042 (I) — `proc.c:1860-1891` — notify+7 fields — stock 12; K-055 (I) — same — unread table — new; K-043 (M) — `proc.c:1893-1910` — two-branch — stock 12 (+ E-PREEMPTFLAG pointer).
- Acceptance: direction diagram (up: schedctl with flag matrix; down: NO_QUANTUM with 7-field table marking read-vs-carried); branch table for do_schedctl (flag × endpoint-state → outcome); PREEMPTIBLE two-branch with edge pointer; O-01 pointer sentence.

### 14-pm-client

- Charge: every way PM knocks, with the three-road short-circuit map.
- Covers: K-044, K-045, K-027-trigger half (INIT exception), K-058 shapes (cite N-02), O-02 boundary paragraph.
- Not covering: PM user surface (→ 04-stage-pm/16), server-side checks (→ 06/07/09; shape-only here), _taskcall transport, RS surface (→ 15; diff paragraph only).
- Requires: 02 (wire shapes), 04 (slot concepts), 05 (USER_Q/USER_QUANTUM values).
- Required by: 06 (START/INHERIT arrivals), 07 (STOP arrival), 09 (nice arrival).
- Ground truth: `sched_start.c:11-76`; `sched_stop.c:9-29`; `pm/schedule.c:20-112`; `pm/forkexit.c:425`; `sched.h` libsys decls; `mp_scheduler` semantics.
- Knowledge rows: K-044 (I) — libsys trio — stock 13; K-045 (I) — PM face — stock 13.
- Acceptance: road map (NONE/KERNEL/USER per scheduler identity with code anchor each); message-content diff (START 4 fields vs INHERIT 3 fields); INIT exception walk; reply-trust rule (reply scheduler wins) with anchor.

### 15-rs-client

- Charge: every way RS knocks, and why system birth differs from user birth.
- Covers: K-046, parent=RS invariant, two stop sites (warn-vs-code), K-060 loopback pointer, O-03/O-04 boundary paragraphs, N-08 service-termination half.
- Not covering: slot-config origins (→ 03-stage-rs/08), live-update flow (→ 03-stage-rs/16), server checks (→ 06; shape-only), PM surface (→ 14; diff paragraph).
- Requires: 02 (wire shapes), 14 (PM对照).
- Required by: 06 (system START arrivals), 07 (RS STOP arrivals).
- Ground truth: `rs/utility.c:364-384`; `rs/manager.c:461`; `rs/request.c:342`; `type.h:92-95` (slot entry, cite); Rust `os/servers/rs/src/sched.rs` vs `os/servers/sched/src/client.rs` vs `valid.rs` three-layer note (from current 14 header — keep, it resolves false-gap audits).
- Knowledge rows: K-046 (I) — RS face — stock 14.
- Acceptance: birthplace diff table (RS-born vs PM-born: timing/asserts/parent); 6-arg call walk with assert anchors; cancel-site table (cleanup→warn-continue vs slot-swap→return-code) with both anchors.

### 99-global-concepts

- Charge: the reference appendix: numbers, errors, invariants, model, omissions, test seam — no mechanism.
- Covers: K-047 (full), K-019-names subset (pointer to N-05), error full table (EPERM/EBADEPT/EINVAL/EDEADEPT/ENOSYS/EBADCPU with verdict homes), K-048 (execution model), K-060, N-03/N-09/N-10 answers, omission rows O-01..O-05.
- Not covering: everything mechanism (each row points home; zero new explanation).
- Requires: none (terminal reference; readable standalone after 00).
- Required by: none (cited by all for constants/errors).
- Ground truth: `com.h:801-807`; `config.h:66-77`; `sys/errno.h:212` (EBADEPT); `todo.md` + `edge_todo.md` E5(e) (test seam).
- Knowledge rows: K-047 (K) — five-table coherence — stock 99 §1.3; K-060 (E) — test seam — new; error rows — per-handler verdict homes — stock 99 §2 + 04/06/08/10.
- Acceptance: constant table (name × value × authority × home doc); error table (code × verdict × home); omission table (topic × sovereign doc × one-line pointer); test paragraph (81 baseline + mock seam + E5(e) gap, no new claims).

---

## 6. Change Table (old → new)

Operation kinds: REORDER (move), SPLIT, MERGE, NEW, ARCHIVE, REWRITE (same slot, new contract).

| Op # | Kind | Old location | New location | Reason | Knowledge IDs | Going (stock → new §) |
|------|------|-------------|--------------|--------|---------------|----------------------|
| C-01 | REWRITE | 00-sched-overview.md | N-00 | Lifeline must show S1/S2 + F1–F3 pointers; current §1.2 station list omits re-pick/dead-macro/double-pick | K-001,K-006-names,K-048 | 1:1, plus F-pointers |
| C-02 | SPLIT | 01 (startup + loop skeleton) | N-01 (startup) + N-02 (loop) | Single-semantics: birth vs loop are different units; skeleton currently claimed by both 01 and 02 | K-002..K-005 | skeleton §§ → N-02; startup §§ → N-01 |
| C-03 | REWRITE | 02-sched-message-surface.md | N-02 | Absorb loop skeleton; add wire table K-058 + severity split K-056; shed handler detail to R-03 | K-005..K-011,K-058 | 1:1 + skeleton in, handler detail out |
| C-04 | REWRITE | 03-schedproc-struct.md | N-03 | Add writer table (names only) + S-10 width note; no flow moves | K-012..K-014 | 1:1 |
| C-05 | REWRITE | 04-schedproc-table.md | N-04 | Add genesis K-057 + verdict trees; shed per-handler re-derivation (R-02) | K-015..K-018,K-047-guard,K-057 | 1:1 + genesis in |
| C-06 | REWRITE | 05-priority-timeslice-model.md | N-05 | Add use-table; shed per-handler exposition (R-01); absorb K-052/K-053 names as pointers | K-019..K-024 | 1:1 |
| C-07 | REWRITE | 06-start-scheduling.md | N-06 | Add double-pick box K-051 + fork sub-line acceptance; shed fanout/cpu internals (R-06) | K-025..K-028,K-051 | 1:1 + F3 box in |
| C-08 | REWRITE | 07-stop-scheduling.md | N-07 | Add symmetry table + no-fanout reason; unchanged chain | K-029 | 1:1 |
| C-09 | SPLIT | 08-noquantum-nice.md | N-08 (demotion) + N-09 (regrade) | Single-semantics violation: kernel-driven demotion vs human-driven atomic regrade share nothing but a file; different triggers, checks, failure modes | K-030,K-032 → N-08; K-031 → N-09 | demotion §§ → N-08; regrade §§ → N-09; trust-asymmetry paragraph duplicated by pointer (door half stays N-02) |
| C-10 | REWRITE | 09-schedule-process.md | N-10 | Add mask algebra K-053, F1 warning K-049, dead-macro deletion K-050, niced end-to-end K-054, name table K-034; shed cpu policy (R-06) | K-033..K-035,K-049,K-050,K-053,K-054 | 1:1 + three boxes in |
| C-11 | REWRITE | 10-pick-cpu-smp.md | N-11 | Add ledger map + USER_DEFAULT_CPU K-052 + drift note; shed retry flow (→ N-06) and migration (O-05) | K-036..K-038,K-052 | 1:1 |
| C-12 | REWRITE | 11-balance-queues.md | N-12 | Add arm→fire→rearm lifecycle + S-9 box; shed door (→ N-02) and demotion (→ N-08) | K-039,K-040,K-004-def | 1:1 |
| C-13 | REWRITE | 12-kernel-interface.md | N-13 | Add unread-mail table K-055 + direction diagram; shed primitive internals (O-01) and name explanation (→ N-10 table) | K-041..K-043,K-055 | 1:1 |
| C-14 | REORDER | 09's kernel-apply rows + 12's name paragraph | N-10 table + N-13 pointer | One home for the collision (R-07) | K-034 | 12-name-§ → N-10; N-13 cites |
| C-15 | REWRITE | 13-pm-interaction.md | N-14 | Add road map + reply-trust rule; shed server checks (R-02) and user surface (O-02) | K-044,K-045 | 1:1 |
| C-16 | REWRITE | 14-rs-interaction.md | N-15 | Add birthplace diff + cancel-site table; keep three-layer Rust note; shed config origins (O-03/O-04) | K-046 | 1:1 |
| C-17 | REWRITE | 99-global-concepts.md | N-99 | Add error full table + omission table + test seam K-060; shed repeated letter/queue semantics (R-08/R-01) | K-047,K-060 | 1:1 + tables in, repeats out |
| C-18 | ARCHIVE | draft/*.md (6 stubs) | archive (no delete) | Already superseded; remain as material with "material" tags, never cited as authority | — | no movement (reference only) |
| C-19 | RENUMBER | old 09→N-10, 10→N-11, 11→N-12, 12→N-13, 13→N-14, 14→N-15 | §8 migration | Cascade from the 08 split; §8 prices it fully before any rebuild starts |

No MERGE is proposed: nothing in the current catalog is too small to stand alone (shortest body, 99 at 50 lines, grows into a real appendix under N-99).

---

## 7. Missing-New-Docs落实 (gap-by-gap closing; no "TBD" allowed)

Every §3.1 gap closes here with home + acceptance (all homes are §§5 contracts above):

- G-01 (F1 re-pick) → N-10 warning box; acceptance: quote `schedule.c:302`, list all four fanout callers affected, ledger-drift note in N-11.
- G-02 (dead migrate macro) → N-10 deletion row; acceptance: definition anchor + zero-caller grep transcript + "do not revive; add IPC face if affinity ever needed" (S-3 kin).
- G-03 (double-pick) → N-06 box; acceptance: both line anchors + ledger +2 walk + pointer to N-10.
- G-04 (USER_DEFAULT_CPU) → N-11; acceptance: value anchor + keep-semantics sentence + hub pointer.
- G-05 (unread accounting) → N-13 table; acceptance: 7-field rows each marked read/carried with the single read anchor (`m_source` only).
- G-06 (panic vs reply) → N-01 (fatal list) + N-02 (reply list); acceptance: every panic site and every reply-code site enumerated, no site in both lists.
- G-07 (zero-init genesis) → N-04; acceptance: four zero-init anchors + port-reproduction sentence.
- G-08 (wire inventory) → N-02; acceptance: 4 structs × fields table with `ipc.h` lines.
- G-09 (restart survival) → N-01; acceptance: `main.c:115` + consequence sentence (no re-init of table on restart).
- G-10 (test seam) → N-99 + N-14/N-15 pointers; acceptance: 81-baseline cited from `todo.md`, mock seam named, E5(e) gap named with `edge_todo.md` pointer, no new test claims.

---

## 8. Anchor Migration + Breakage Cost

### 8.1 Section migration (every changed doc, every section)

Format: old § → new § + type. (Section numbers below are the current docs' `##`-level sections as read; Phase B re-verifies numbering before moving text.)

| Old position | One-line content | New position | Type | Breakage note |
|--------------|------------------|--------------|------|---------------|
| 01 §1.1–1.2 (startup) | SEF registration, topology read | N-01 §§2–3 | move | intra-dir cites of "01 startup" stay valid (number kept) |
| 01 loop-skeleton §§ | receive/classify/reply skeleton | N-02 §2 | move | cites of "01 skeleton" retarget to N-02 §2 |
| 02 all §§ | five letters, doors, gate, reply, no_sys | N-02 §§2–5 | rewrite-in-place | number kept; handler-detail paragraphs move out (→ N-06..N-09 doors-only) |
| 03 all §§ | 7 fields + IN_USE + cpu_mask | N-03 §§2–4 | rewrite-in-place | number kept; add writer table |
| 04 all §§ | slot math, verdicts, whitelist | N-04 §§2–5 | rewrite-in-place | number kept; add genesis + trees |
| 05 all §§ | ladder, ceiling/position, ms, init values, system test, nice | N-05 §§2–4 | rewrite-in-place | number kept; per-handler expositions shrink to use-table |
| 06 all §§ | takeover chain + fork sub-line | N-06 §§2–5 | rewrite-in-place | number kept; add F3 box; fanout/cpu detail shrinks to pointers |
| 07 all §§ | stop chain + symmetry | N-07 §§2–3 | rewrite-in-place | number kept |
| 08 demotion §§ | noquantum path | N-08 §§2–3 | split | cites of "08 demotion" → N-08; cites of "08 nice" → N-09 |
| 08 regrade §§ | nice path + rollback | N-09 §§2–4 | split | see above; rollback walk becomes N-09 acceptance core |
| 08 trust-asymmetry § | door vs whitelist | N-02 §3 (door half) + N-08 §2 (handler half, cite) | split | shared paragraph lives N-02 |
| 09 fanout §§ | mask, -1, niced, sys_schedule | N-10 §§2–3 | move (renumber) | "09 fanout" cites → N-10 |
| 09 kernel-apply §§ | do_schedule/sched_proc | N-10 §4 + N-13 §3 (pointer) | move | R-07 home N-10 |
| 09 pick_cpu line + 10 §§ | choice + ledger | N-11 §§2–4 | move (renumber) | "10 pick_cpu" cites → N-11 |
| 11 §§ | arm/scan/rearm/rhythm | N-12 §§2–4 | move (renumber) | "11 balance" cites → N-12 |
| 12 §§ | schedctl/notify/proc_no_time | N-13 §§2–4 | move (renumber) | "12 kernel" cites → N-13 |
| 12 name-collision § | process vs proc | N-10 §5 (home) | move | N-13 cites N-10 |
| 13 §§ | libsys trio, PM face, INIT | N-14 §§2–4 | move (renumber) | "13 PM" cites → N-14 |
| 14 §§ | RS face, stop sites, 3-layer note | N-15 §§2–4 | move (renumber) | "14 RS" cites → N-15 |
| 00 §§ | lifeline + map | N-00 §§1–3 | rewrite-in-place | update station list (F-boxes) + doc map (17 entries) |
| 99 §§ | constants/errors/coherence/model | N-99 §§2–5 | rewrite-in-place | repeats out, tables in |

### 8.2 Reference migration (old number/filename → new target + verification)

Measured breakage surface (2026-09-20):

- **159 hits** for `06-stage-sched` across `notes/` + `os/` (`*.md` + `*.rs`). Hotspots: `04-stage-pm/16-scheduling.md` + its `doc_rerank_*` family, `07-stage-ds` docs, `01-stage-kernel` `doc_rerank_*` family, `00-master-plan/README.md`, `edge_todo.md`, `edge3.md`, `14-stage-runtime/plan.md`, this dir's `plan.md`/`archive/*`. Most hits are stage-level (`06-stage-sched/` path) and are ** unaffected by inner renumber** — only hits naming `09-`–`14-` filenames or `NN-<slug>` doc titles need retargeting.
- **32 `.md` mentions across all 19 Rust files** under `os/servers/sched/src/` (every file carries a `NN-<slug>.md` home pointer in its header comment). Files pointing at shifted numbers (09–14 slugs) need comment updates: `kernel_api/schedule.rs`, `kernel_api/schedctl.rs`, `kernel_api/transport.rs`, `cpu.rs`, `balancer.rs`, `client.rs`, `server.rs` (partial), `scheduling/*` (names stable, numbers shift only for fanout callers' pointers).
- Intra-dir cross-refs: every doc's "not covering → see NN" list and §7-style "see also" blocks that name shifted numbers (09–14) retarget per table 8.1. Unshifted numbers (00–08) keep working.

Batch method (proposed, Phase B executes): `rg -n '0[9]-(schedule-process|start)|10-pick-cpu|11-balance|12-kernel-interface|13-pm-interaction|14-rs-interaction' notes/ os/ --glob '*.md' --glob '*.rs'` → per-hit retarget per table 8.1 → re-run until zero → `cargo test -p minix-sched` unchanged (comment-only edits) → `tools/design-coverage-check.sh fork-syscall-rewrite --stage 06-stage-sched` still ALL COMPLETE.

### 8.3 Breakage-cost summary

- References needing retarget: estimated 40–70 of the 159 hits (only inner-number hits; stage-path hits unaffected). Verification: the rg command above gives the exact list; no guessing.
- Hotspot files (hand-check, not batch): `04-stage-pm/16-scheduling.md` (sovereign neighbor — notify before editing), `edge_todo.md` + `edge3.md` (cross-stage ledgers — append-only conventions apply), `00-master-plan/README.md` (stage table — inner numbers not named there, likely zero hits needing change).
- Old docs archived, not deleted: zero link-rot for external bookmarks; migration is pointer updates, not URL deaths.
- Rebuild is approved to proceed **iff** the consensus blueprint (bagging merge) keeps §8.1+§8.2 complete: any new proposal that adds a renumber must extend both tables before Phase B starts.

---

## 9. Verification + Self-Check Gates

### 9.1 Four mechanical checks (results on this blueprint)

1. **Forward-reference scan**: new order 00→01→…→15→99; every contract's "Requires" points strictly backward (05→03/04, 06→02/04/05, 10→05/06, 11→01/05, 13→10/05, 14→02/04/05, 15→02/14). **PASS** — no contract requires a later number. (N-00/N-99 exempt as map/appendix carrying names only.)
2. **Dependency DAG**: edges from §5 Require/Required-by form a DAG with hub N-05 (in-degree 6) and doors N-02/N-04 (in-degree 5/4). No cycles: handlers never require each other (07 cites 06's shape by name only, explicitly non-dependency). **PASS**.
3. **Coverage**: all 61 pool entries have a home (§2 rows each name one; §7 closes all 10 gaps); the two deletions (K-050 dead macro, K-013 cpu_mask already-deleted-in-Rust) are listed with reasons, not silently dropped. **PASS (100%)**.
4. **Breakage census**: 159 cross-refs counted, 32 Rust header pointers counted, batch command + hotspot list given. **PASS with method** (exact per-hit list is Phase-B runtime output of the stated rg, not pre-fabricated here).

### 9.2 Self-check gates G1–G9

| Gate | Check | Result |
|------|-------|--------|
| G1 | True order checkable (10 random anchors re-verified against source during writing: main.c:22/35/45/58/70/90/111/126, schedule.c:44/226/302/334/353, utility.c:29/46/61, com.h:801-807, config.h:66-77, ipc.h:1428-1445) | PASS |
| G2 | Every C file + non-C artifact has a home or explicit exclusion (6 server files + 8 counterpart files + 4 wire structs + Makefile + 10 non-C answers in §3.4) | PASS |
| G3 | Zero forward references (§9.1-1) | PASS |
| G4 | DAG acyclic (§9.1-2); only apparent loop (06↔10 via re-pick) resolved by call-vs-policy split with one-directional Require (N-10 requires N-06's example; N-06 cites N-11's policy by name) | PASS with documented split |
| G5 | 100% pool homing (§9.1-3); new entries all anchored; deletions listed (K-013/K-050) | PASS |
| G6 | Every split/move names stock going (C-02, C-09, C-14 sampled plus full table); every NEW (K-049..K-061) names an anchor | PASS |
| G7 | All 17 contracts carry 7 elements (charge/covers/not/requires/required-by/ground-truth/rows/acceptance) | PASS |
| G8 | Migration covers all changed docs per-section (§8.1, 21 rows) + reference layer (§8.2 with counts + command) | PASS |
| G9 | Factual claims anchored (spot-check 10: 658-line total, 19-file/4707-line Rust count, 159 refs, 32 rs pointers, BALANCE_TIMEOUT 5, USER_DEFAULT_CPU -1, CPU_DEAD -1, SCHED_PROC_NR 4 (schedule.c:246), IN_USE 0x1, MIN_USER_Q floor); judgments without anchors labeled (e.g. "estimated 40–70", "likely zero") | PASS |

### 9.3 Conclusion + questions for consensus

**Conclusion**: the blueprint is complete and executable: 17-doc catalog, 61-entry pool with 100% homing, 10 gaps closed, 8 duplications + 5 overflows resolved, 10 non-C questions answered, migration priced. The single structural bet is the **08 split + 09–14 cascade renumber** (C-09/C-19): it buys single-semantics for the two most-confused mechanisms (demotion vs regrade) at the price of 40–70 pointer updates.

Questions for the bagging merge (no unilateral calls):

1. **Split vs keep 08**: is the demotion/regrade confusion worth a cascade renumber, or should N-09 be appended as an unnumbered insert (e.g. `08b-nice-regrade.md`) with zero cascade? Cost/benefit hinges on how many of the 159 hits name `09–14` — Phase-B rg (§8.2) decides; if under ~20, cascade stands.
2. **N-13 granularity**: should the kernel contract split further (registration vs notification) into two docs? I kept one (both directions fit one privilege-line story); merge-round may overrule if N-13's contract exceeds ~400 lines in Phase B.
3. **F1 ledger drift**: the blueprint documents the re-pick churn (K-049) but does not judge it a C bug. Should N-10/N-11 call it a bug with an [ARCH] fix proposal, or stay descriptive? Evidence for "bug" (unbounded `cpu_proc` growth between stops) vs "harmless" (relative ordering preserved under uniform churn) needs a worked numeric example — flagged for Phase B, not decided here.
4. **N-00 length cap**: proposed ≤ 100 lines. If the consensus map needs the full 17-row table, the cap moves to the table-only exception — to be ratified at merge.

---

## Appendix: size and readability note (task-specific constraint)

Per the task note: single-doc length limits are soft; control length for readability; doc count is free; rebuild (not patch) is expected and old docs may carry unreviewed errors, so C + Rust ground truth outranks existing prose. This blueprint follows all four: the longest proposed docs (N-06, N-10, N-13) may each approach 300–400 lines because their mechanisms are genuinely dense (10-link chain, narrow-waist + traps, bidirectional contract); the count moves 16 → 17 only because one doc demonstrably carried two semantics; every contract's ground-truth section outranks carried-over prose, and §3 gaps derived from fresh C reads (F1–F3) override any conflicting old-doc claims.
