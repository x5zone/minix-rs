# 03-stage-rs Document Reconstruction Blueprint (muse)

## 0. Metadata

- Executor: muse
- Date: 2026-09-19 (UTC)
- Target dir: rewrite-notes/03-stage-rs/
- Repo root: /home/xzhao/github/minix-rs
- Commit: 232e571ded9af1039e68c2c3dc1caa525b24f4b2
- Task: R-phase reconstruction blueprint. Output only this file. No body text modified.
- Language: English (per muse special requirement; B-phase bodies shall also be written in English).
- Constraints honoured: no reference to .design/ or tmp_design_and_todo/; single output file with _muse suffix; no reading of other AIs' doc_rerank_* products.

### Scope

In-scope documents (numbered + 99 global):
00, 01, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 99.

Reference materials (not rebuilt, only sourced):
plan.md, todo.md, draft/README.md.

Out of scope: other stages' docs; .design/ and tmp_design_and_todo/ contents; other AIs' doc_rerank_* files.

### Reading list (verified)

Docs (line counts via wc -l on 2026-09-19):
00(349), 01(1044), 02(904), 03(706), 04(396), 05(416), 06(407), 07(351), 08(487), 09(310), 10(307), 11(172), 12(291), 13(291), 14(243), 15(268), 16(312), 17(252), 18(260), 19(226), 99(239). plan.md(369), todo.md(580).

C sources (ground truth, minix3/minix/servers/rs/):
main.c(834), manager.c(2332), request.c(1309), update.c(1011), utility.c(547), exec.c(165), error.c(59), table.c(50); headers const.h(123), glo.h(58), inc.h(56), proto.h(152), type.h(112). Total 6808 lines.

Protocol headers:
minix3/minix/include/minix/rs.h (RSS_*/SF_*/rs_start/rs_state_data), com.h (RS_RQ_BASE 0x700, RS_UP..RS_FI, RS_PROC_NR=2), sef.h (SEF_INIT_*/SEF_LU_*), ipc_filter.h (IPCF_*/ANY_*), priv.h (static ids, ALL_M/SRV_M/USR_M), kernel/priv.h (struct priv), kernel/table.c (boot_image order), kernel/main.c (VMINHIBIT boot suppression).

Rust implementation (existence/oracle, not order authority):
os/servers/rs/src/*.rs (29 files, 22458 lines total per wc). Largest: boot.rs(2338), live_update.rs(2432), lib.rs(2253), shell_request.rs(1786), service_create.rs(1917). Entry: lib.rs (crate docs + re-exports), main.rs(52), table.rs(299), boot.rs, sef.rs(188), dispatch.rs(257).

Boundary materials:
00-master-plan/README.md (stage order + causal chain Kernel→VM→RS→rest), edge_todo.md (cross-stage items incl. E-RSWIRE/RS_INIT handshake), 02-stage-vm/00-vm-overview.md (previous stage: what VM already teaches, esp. VM↔RS protocol side), 01-stage-kernel/06-todo.md (contract-writing style example only).

### Commands + key outputs (evidence excerpts)

- wc -l minix3/minix/servers/rs/*.c/*.h → 6808 total (see above).
- grep -n main/do_*/create_/publish_/update_ across rs/*.c → dispatch arms (RS_UP/DOWN/REFRESH/RESTART/SHUTDOWN/UPDATE/CLONE/UNCLONE/EDIT/SYSCTL/FI/GETSYSINFO/LOOKUP + RS_INIT/RS_LU_PREPARE), boot 4-step loop, LU phase functions (see §1 anchors).
- sed -n main.c:38-135 (main loop), main.c:158-435 (sef_cb_init_fresh Steps 0-4), main.c:435-498 (USE_LIVEUPDATE self-upgrade tail).
- cat table.c (boot_image_priv/sys/dev tables), type.h (rproc/rprocupd/rupdate), const.h (r_flags 16 bits, RS_DELTA_T, backoff), glo.h (rproc/rprocpub/rproc_ptr/rinit/rupdate/shutting_down).
- grep RS_ com.h: RS_RQ_BASE 0x700 + offsets; grep RSS_/SF_ rs.h: 22 RSS flags + SF_CORE_SRV..SF_NO_BIN_EXP.
- grep cross-refs 03-stage-rs/NN across notes+os → hottest: 16(15 refs), 02(14), 08(13), 15/07(9 each), 19/10/01(8 each). Full table in §8.
- git rev-parse HEAD → 232e571de (full above). date -u → 2026-09-19.

## 1. C True Order (runtime truth, rebuilt from source, not paraphrased)

Stage typeverdict: hybrid — boot-chain head (linear startup, main.c:158-498) + service event-loop (permanent dispatch, main.c:38-135). Per §9 supplement: treat boot skeleton as Chapters 01-02 backbone and event-loop request lifecycle as Chapters 06-16 backbone; state both, with order-difference table at the end.

### T1. Process birth and main-loop skeleton (main.c:38-135)

| Step | Action | Anchor | Note |
|------|--------|--------|------|
| T1-1 | sef_local_startup() registers SEF callbacks | main.c:51, main.c:136-156 | callback table: init_fresh/restart/lu, init_response/lu_response, signal_handler/manager |
| T1-2 | sys_getmachine(&machine) | main.c:53-54 | machine info before loop; panic on failure |
| T1-3 | while(TRUE): rs_idle_period() background work | main.c:60-61, utility.c:443-479 | cleans RS_DEAD slots, refills replicas; only when idle (rs_is_idle, utility.c:424) |
| T1-4 | get_work(): sef_receive_status(ANY) | main.c:64, main.c:826-833 | blocking receive; status tells notify vs request |
| T1-5 | rs_isokendpt(who_e) validation; panic on bogus | main.c:65-68, utility.c:352-362 | fail-fast, no reply |
| T1-6 | if notify: CLOCK→do_period, else heartbeat timestamp | main.c:84-99 | rproc_ptr[who_p]->r_alive_tm = timestamp; unknown source only warns |
| T1-7 | else dispatch 15 arms + default ENOSYS | main.c:100-120 | RS_UP/DOWN/REFRESH/RESTART/SHUTDOWN/UPDATE/CLONE/UNCLONE/EDIT/SYSCTL/FI/GETSYSINFO/LOOKUP + RS_INIT/RS_LU_PREPARE |
| T1-8 | reply unless EDONTREPLY | main.c:122-127, utility.c:309-341 | reply()/late_reply() protocol |

### T2. Boot: sef_cb_init_fresh 4 steps + tail (main.c:158-498)

| Step | Action | Anchor | Note |
|------|--------|--------|------|
| T2-0 | env_parse(rs_verbose) + sys_getinfo(GET_HZ) + rprocpub grant + RUPDATE_INIT + shutting_down=FALSE + sys_getimage | main.c:176-197 | pre-steps before Step 1; grant is rinit.rproctab_gid via cpf_grant_direct |
| T2-1 | Step 1: per-service priv/sys/dev/command/sched setup (boot_image_priv_table order) | main.c:230-345, table.c:20-32 | label, static_priv_id, s_flags/init_flags/trap/ipc_to/sig_mgr, call masks, sys_privctl SET_SYS (except RS/VM), sys_getpriv sync, sys_flags, dev_nr, r_cmd/script/build_cmd_dep, vm_call_mask, scheduler/priority/quantum, endpoint, r_* defaults, RS_IN_USE\|RS_ACTIVE, rproc_ptr link, in_use |
| T2-2 | Step 2: allow to run + init_service; RS/VM direct init; SYNCH_BOOT caught inline, others deferred | main.c:347-399, utility.c:18-68 | sched_init_proc + SYS_PRIV_ALLOW, init_service(SEF_INIT_FRESH), catch_boot_init_ready for SYNCH_BOOT |
| T2-3 | Step 3: drain remaining init-ready messages | main.c:401-408, main.c:784-825 | catch_boot_init_ready(ANY) loop |
| T2-4 | Step 4: getnpid per service from PM | main.c:410-428 | fills r_pid; panic on failure |
| T2-5 | sys_setalarm(RS_DELTA_T) starts periodic supervision | main.c:430-432, const.h:60-63 | RS_DELTA_T = system_hz; RS_INIT_T = 10x for init timeout |
| T2-6 | USE_LIVEUPDATE tail: clone RS slot, srv_fork replica, child update_service(RS_SWAP)+cpf_reload+cleanup+vm_memctl PIN; parent SET_SYS+sched_init+YIELD then NOT_REACHABLE | main.c:435-498 | boot self-upgrade; only replica path continues |

### T3. Restart/LU entry + signals (main.c:499-708)

| Step | Action | Anchor | Note |
|------|--------|--------|------|
| T3-1 | sef_cb_init_restart: re-init RS self (init_service RESTART path) | main.c:499-548 | SEF provider self-bootstrap |
| T3-2 | sef_cb_init_lu: LU init entry for RS self | main.c:549-590 | distinct from fresh/restart |
| T3-3 | sef_cb_init_response / sef_cb_lu_response: RS self-simulated ready normalization | main.c:591-630 | normalize to do_init_ready/do_upd_ready semantics |
| T3-4 | sef_cb_signal_handler + sef_cb_signal_manager: route to target's manager | main.c:631-708, utility.c:387-412 | update_sig_mgrs backing; per-service managers |
| T3-5 | boot_image_info_lookup helper | main.c:709-783 | resolves priv/sys/dev rows for an endpoint |

### T4. Service bring-up chain (manager.c + exec.c + utility.c init_service)

| Step | Action | Anchor | Note |
|------|--------|--------|------|
| T4-1 | Request arrives (RS_UP): check_call_permission → check_request/copy_rs_start/copy_label | request.c:15-110, manager.c:81-130, manager.c:135-173 | gate before any mutation |
| T4-2 | Slot landing: init_slot (new) or edit_slot (existing) + build_cmd_dep + inherit_service_defaults | manager.c:1460-1797, manager.c:1303-1333 | fields → rproc; RSS_* steer exec/replica/script/no-restart |
| T4-3 | Priv materialize: init_privs (forward+backward IPC) + fill_send_mask/fill_call_mask primitives | manager.c:2112-2331, utility.c:82-141 | s_ipc_to bitmap; kernel/sys/vm call masks |
| T4-4 | create_service 11-step orchestration: fork/priv/sched/exec/VM pin etc. | manager.c:531-712 | failure → kill/cleanup path |
| T4-5 | Exec image: read_exec (load) / share_exec (reuse) / free_exec; srv_execve→do_exec→exec_restart via libexec + PM | exec.c:21-165, manager.c:1357-1459 | SF_USE_COPY/SF_NEED_COPY lifecycle |
| T4-6 | publish_service (DS label, VFS mapdriver, PCI ACL, devman bind); unpublish is best-effort inverse | manager.c:787-922 | externally visible; failure → kill_service |
| T4-7 | start_service→run_service→init_service(RS_INIT msg)→service self-inits→do_init_ready/do_upd_ready→end_srv_init; boot uses catch_boot_init_ready sync variant | manager.c:923-987, utility.c:18-68, request.c:462-533, request.c:890-942, main.c:784-825 | same handshake boot + runtime; update branch diverts to LU |
| T4-8 | clone_service / activate_service / clone_slot / swap_slot: replica + LU slot mechanics | manager.c:713-786, manager.c:1013-1032, manager.c:1800-1932 | pure slot moves reused by 15/16 |

### T5. Steady-state supervision + control/observe/terminate (request.c:943-1309, manager.c:360-1353)

| Step | Action | Anchor | Note |
|------|--------|--------|------|
| T5-1 | do_period per-tick: backoff-revive / SIGTERM→SIGKILL escalation / ping-timeout crash; free-pass exceptions | request.c:943-1049 | driven by CLOCK notify; RS_DELTA_T cadence |
| T5-2 | do_sigchld reaps exited children | request.c:1051-1092 | + rupdate chain touch (delegated) |
| T5-3 | update_period hook: update prepare timeout | update.c:371-400 | called from period path; LU-owned |
| T5-4 | Control handlers: do_up/down/restart/clone/unclone/edit/refresh/shutdown (+ stop_service SIGTERM/KILL) | request.c:15-461, manager.c:988-1012 | thin validate→mechanism; RS_UPDATE excluded (LU) |
| T5-5 | Observe handlers: do_lookup/getsysinfo/sysctl/fi (+ fi_service, print_* diagnostics) | request.c:1095-1309, utility.c:69-81, utility.c:142-222, utility.c:485-546 | read-only except FI crash-injection + sysctl UPD_* delegation |
| T5-6 | Terminate/recover: kill/crash/cleanup/detach/reincarnate/terminate/run_script/restart/get_service_instances + backoff + RS_DEAD drain | manager.c:360-530, manager.c:1033-1353, const.h:48-51 | densest decision tree (terminate_service); RS_DEAD cleaned in idle period |

### T6. Live Update (update.c:1-1011 + request.c:534-942)

| Step | Action | Anchor | Note |
|------|--------|--------|------|
| T6-1 | do_update validates RSS_LU_*/SEF_LU_* flags + prepare_state, builds rpupd chain (rupdate_add_upd, set_new_upd_flags, upd_init) | request.c:534-889, update.c:23-163 | multi/VM/RS-inclusive flags; prepare-only/nommap/detached/ASR/self variants |
| T6-2 | start_update_prepare (+_next): order chain, request_prepare_update_service per member | update.c:401-531 | retries allowed at this stage |
| T6-3 | State transfer: init_state_data (eval/IPC-filter/custom) via grants + datacopy | manager.c:174-288, type.h:30-42 | prepare-stage payload copy |
| T6-4 | start_update → start_srv_update → init_service(SEF_INIT_LU) new instance → complete_srv_update | update.c:532-706 | new instance boots while old serves |
| T6-5 | Ready convergence: do_upd_ready per member (RS_PREPARE_DONE/INIT_DONE/PENDING bits) | request.c:890-942 | async; timeout via update_period |
| T6-6 | end_update_* family + end_srv_update: commit (swap via update_service RS_SWAP) or abort_update_proc/rollback_service | update.c:707-1011 | result×reply_flag matrix; VM multi-component ordering; RS-self short-circuit |
| T6-7 | srv_update/update_service/rollback_service primitives + rupdate_upd_move/clear | update.c:164-370 | slot-level swap/rollback used by boot tail too |

### T7. Diagnostics (error.c:1-59, utility.c:142-222,485-546)

init_strerror/lu_strerror tables; srv_to_string_gen/srv_upd_to_string/print_services_status/print_update_status. Consumers: local debug + 08-stage-is dump surface (handoff, not RS-owned rendering).

### Order-difference table (runtime fact → teaching choice → compensation)

| # | Runtime fact (anchor) | Teaching order chosen | Reason | Compensation (back-pointer) |
|---|----------------------|----------------------|--------|------------------------------|
| O-1 | Boot Step 1 writes priv/mask/sched fields before reader knows table layout (main.c:240-345) | 01 thin skeleton first, 02 data shapes second, 04/05 mechanisms third | Reader needs names before fields, fields before rules | 01 names-only discipline (§5 N01 contract); 02 ends with "where each field is written" forward map without explaining rules |
| O-2 | Handlers (13/14/16) run inside main loop (main.c:100-120) but taught after loop (07) | 07 pure classifier before any handler | Classifier is stable; handlers are parallel leaves | 07 handler table lists names + doc numbers only; each handler doc opens with "dispatch position" paragraph |
| O-3 | update_period runs inside do_period (request.c:943-1049 calls update.c:371) but LU taught much later (15) | Heartbeat (08) states call-site only; LU (15) owns semantics | Avoids 700-line LU digression inside watchdog | 08 marks call-site as delegated (plan-style exemption), 15 re-states trigger condition |
| O-4 | RS self-upgrade tail runs at boot end (main.c:435-498) but self lifecycle taught last (16) | Boot (01) states existence + outcome only; 16 owns mechanics | Self-upgrade reuses LU primitives taught in 15 | 01 tail section = 6-line outcome box; 16 opens with boot-tail replay |
| O-5 | Reply protocol (reply/late_reply) executes after every handler but taught once up front (06) | 06 gate+reply chapter before handlers | Single ownership; handlers only cite outcome codes | Handler docs never re-explain reply; acceptance checks cite 06 codes |

## 2. Knowledge Pool (deduplicated, stage-unique K-ids; name+anchor = merge key)

Legend — Type: C=concept, M=mechanism, D=data-structure, P=interface/protocol, V=constraint/invariant, A=arch-evolution, T=tooling/engineering, E=test-property. Source: S=stock (from existing docs), N=new (from coverage audit §3; evidence anchor required).

### K-001..K-012 — Identity, boot position, overview map (mainly old 00/01/99)

| ID | Name | Type | Source | Stock location | Anchor | Reader benefit (answers) |
|----|------|------|--------|----------------|--------|---------------------------|
| K-001 | RS identity: root system process, RS_PROC_NR=2 | C | S | 00 §1.1 | com.h:61, com.h:77 | Why RS can start others; what "root" means |
| K-002 | Registration vs execution order (table.c order vs VMINHIBIT boot) | C | S | 00 §1.2 | kernel/table.c:44-64, kernel/main.c:196,265-267 | Why DS first on paper but VM first at runtime |
| K-003 | Two nested state machines (one-shot boot + perpetual runtime) | C | S | 00 §1.3 | main.c:38-135, main.c:158-498 | Mental model for whole stage |
| K-004 | Boot mainline map with per-step doc numbers | M | S | 00 §2.0, plan §1.2 | main.c:38-135,158-498 | Where am I in boot at any doc |
| K-005 | Service lifecycle second-thread path (UP→…→DOWN→restart/LU) | M | S | 00 §3, 13 §1, 15 §1 | request.c:15-461, manager.c:531-1353 | How one service travels birth→death→rebirth |
| K-006 | Position-answerability discipline (every doc states boot+loop position) | T | S | 00 §2, plan §1.2 | plan.md §1.2-1.3 | How to check a doc is in the right place |
| K-007 | SEF callback table (fresh/restart/lu/response/signal) | P | S | 01 §3.3 | main.c:26-32,136-156 | Who calls RS at each life event |
| K-008 | boot_image_* three tables + lookup helper | D | S | 01 §2, table.c | table.c:20-50, main.c:709-783 | Where boot inputs come from |
| K-009 | Boot Steps 0-4 + alarm + self-upgrade tail existence | M | S | 01 §§2-4 | main.c:158-498 | Boot sequence without mechanism detail |
| K-010 | catch_boot_init_ready sync variant vs runtime async ready | M | S | 01 §4, 12 §1 | main.c:784-825, request.c:462-533 | Why boot blocks but runtime does not |
| K-011 | RS vocabulary: 15 msg types, 16 r_flags, 13 SF_*, RSS_*/SEF_*/endpoint/errno tables | D | S | 99 all | com.h:463-492, const.h:28-51, rs.h:33-52,191-203, sef.h, error.c | Single glossary for all symbols |
| K-012 | Overview coverage contract (8 .c / 15 types / flags surfaces) | E | S | 00 §4 | rs/*.c + rs.h/com.h | What "done" means for the stage |

### K-013..K-024 — Table data + slot ops (mainly old 02)

| ID | Name | Type | Source | Stock location | Anchor | Reader benefit |
|----|------|------|--------|----------------|--------|----------------|
| K-013 | rproc full field model (pointers, pid, counters, times, cmd/argv, exec, priv, sched, io/irq backup, ipc_list, control) | D | S | 02 §2 | type.h:55-110 | What one slot remembers |
| K-014 | rprocpub public mirror (label, endpoint, in_use, old/new endpoint, sys_flags, vm_call_mask, dev) | D | S | 02 §2 | rs.h: rprocpub; glo.h: rprocpub/rproc/rproc_ptr | What other servers can see via grant |
| K-015 | rproc_ptr fast index + NONE/bogus handling | D | S | 02 §3 | glo.h: rproc_ptr; utility.c:352-362 | How endpoint→slot resolves |
| K-016 | rinit grant descriptor (rproctab_gid) | D | S | 02 §4, 01 §2 | main.c:185-190 | How RS publishes its table |
| K-017 | rupdate/rprocupd chain shapes + RUPDATE_* macros/iterators | D | S | 02 §5, 16 §2 | type.h:30-54, const.h:90-120 | How multi-service LU is chained |
| K-018 | r_flags 16-bit table semantics (IN_USE…REINCARNATE + IDLE predicate) | D | S | 02 §6 | const.h:28-58 | Reading any state test in code |
| K-019 | sys_flags/SF_* 13-flag table (CORE_SRV…NO_BIN_EXP) | D | S | 02 §6 | rs.h:191-203 | Boot/lifecycle policy bits |
| K-020 | lookup_slot_by_* ×5 semantics + linear-scan cost note | M | S | 02 §7 | manager.c:1935-2066 | How callers find slots |
| K-021 | alloc_slot/free_slot lifecycle + free_exec coupling | M | S | 02 §7 | manager.c:2067-2110, manager.c:1424 | Slot birth/death rules |
| K-022 | rs_isokendpt validation + panic path | V | S | 02 §7, 06 §2 | utility.c:352-362, main.c:65-68 | What "bogus source" costs |
| K-023 | get_service_instances replica enumeration | M | S | 02 §7, 10 §3 | manager.c:1334-1356 | How replicas are listed |
| K-024 | ARCH A-3/A-4 slot-model Rust mapping (pointer chains→indices; rproc_ptr→array) | A | S | 02 §8 | os/servers/rs/src/process_table.rs, service_slot.rs | Why Rust differs structurally |

### K-025..K-034 — Privilege + IPC mask (mainly old 03/05)

| ID | Name | Type | Source | Stock location | Anchor | Reader benefit |
|----|------|------|--------|----------------|--------|----------------|
| K-025 | struct priv field model (id/flags/init/trap/ipc_to/sig_mgr/call masks) | D | S | 03 §1 | kernel/priv.h, include/minix/priv.h | What kernel enforces per service |
| K-026 | Boot Step-1 priv construction order (label→static id→bitmaps→masks→SET_SYS→GETPRIV sync) | M | S | 03 §2 | main.c:240-345 | Exact boot write order |
| K-027 | sys_privctl operation surface (SET/UPDATE/ALLOW/DISALLOW/YIELD/SET_USER/CLEAR_IPC_REFS) | P | S | 03 §3 | libsys/sys_privctl.c, kernel/system/do_privctl.c | What each op does to kernel |
| K-028 | fill_send_mask / fill_call_mask primitives (all-set vs all-clear) | M | S | 03 §3 | utility.c:82-141 | Boot fast-path bitmap fill |
| K-029 | sched_init_proc + scheduler/priority/quantum wiring | M | S | 03 §4 | utility.c:364-386 | How scheduling starts |
| K-030 | update_sig_mgrs + backup manager semantics | M | S | 03 §4, 18 §3 | utility.c:387-412 | How signal routing changes |
| K-031 | s_ipc_to 64-bit meaning + kernel enforcement point | C | S | 05 §1 | kernel ipc-filter path (01-stage-kernel/23) | What the bitmap gates |
| K-032 | Boot ALL_M shortcut path (SRV_M/USR_M → fill ALL) | M | S | 05 §2 | main.c:272, utility.c:82-99 | Why boot services start wide open |
| K-033 | r_ipc_list parse path: get_next_name + add_forward_ipc + add_backward_ipc + init_privs | M | S | 05 §3 | manager.c:2112-2331 | How dynamic services get precise reachability |
| K-034 | edit_slot→init_privs re-mask on RS_EDIT (request.c:348 hook) | M | S | 05 §4, 13 §3 | request.c:298-389 | When masks change at runtime |

### K-035..K-042 — Gate + loop + heartbeat (mainly old 04/06/07)

| ID | Name | Type | Source | Stock location | Anchor | Reader benefit |
|----|------|------|--------|----------------|--------|----------------|
| K-035 | Two-level authz: root vs r_control isolation list | M | S | 04 §2 | manager.c:21-130 | Who may command RS |
| K-036 | Five target-slot rules in check_call_permission | V | S | 04 §3 | manager.c:81-130 | Per-call allow/deny matrix |
| K-037 | 11 handler call-sites + getnuid external dep + UPDATING→EBUSY rule | P | S | 04 §4 | request.c (11 sites), libsys/getepinfo.c | Where the gate bites |
| K-038 | Main-loop 3 activities (idle work / get work / reply) + 4-way classification | M | S | 06 §1-2 | main.c:38-135 | Runtime skeleton |
| K-039 | reply/late_reply/EDONTREPLY + RS_LATEREPLY + RS_DONTREPLY/REPLY/CANCEL + shutdown arm | P | S | 06 §3 | utility.c:309-351, const.h:86-88, errno EDONTREPLY | How replies (incl. deferred) work |
| K-040 | rs_idle_period + rs_is_idle background duties (DEAD drain, replica refill) | M | S | 06 §4 | utility.c:424-479 | What happens "between" messages |
| K-041 | do_period 3-branch machine (backoff revive / TERM→KILL / ping-timeout crash) + free-pass | M | S | 07 §2 | request.c:943-1049 | Watchdog decisions per tick |
| K-042 | do_sigchld reap + update_period prepare-timeout hook | M | S | 07 §3 | request.c:1051-1092, update.c:371-400 | Child cleanup + LU timeout source |

### K-043..K-056 — Bring-up + control/observe (mainly old 08-14)

| ID | Name | Type | Source | Stock location | Anchor | Reader benefit |
|----|------|------|--------|----------------|--------|----------------|
| K-043 | rs_start field model + RSS_* input bits | D | S | 08 §1 | rs.h:104-151, rs.h:33-52 | What a start request carries |
| K-044 | check_request validation rules | V | S | 08 §2 | request.c:1265-1309 | What gets rejected before copy |
| K-045 | copy_rs_start/copy_label grant-copy mechanics | M | S | 08 §2 | manager.c:135-173 | How user memory enters RS |
| K-046 | init_slot vs edit_slot landing + RSS_COPY/REUSE branches | M | S | 08 §3 | manager.c:1460-1797 | New vs mutate paths |
| K-047 | build_cmd_dep + inherit_service_defaults (argv/dep/defaults) | M | S | 08 §4 | manager.c:289-327,1303-1333 | Command synthesis + defaults |
| K-048 | Exec image lifecycle: read_exec/share_exec/free_exec + SF_USE_COPY/NEED_COPY | M | S | 09 §§1-2 | manager.c:1357-1459 | In-memory binary sharing |
| K-049 | srv_execve→do_exec→exec_restart chain + stack/ELF external contract (ARCH A-8) | P | S | 09 §3 | exec.c:21-165, libexec/exec_elf.c | How image enters child address space |
| K-050 | create_service 11-step orchestration + rollbackhook | M | S | 10 §2 | manager.c:531-712 | Birth order + failure exits |
| K-051 | clone_service / activate_service replica mechanics | M | S | 10 §3 | manager.c:713-786,1013-1032 | How replicas are born/activated |
| K-052 | clone_slot / swap_slot(+_pointer) pure slot moves | M | S | 10 §4 | manager.c:1800-1932 | Slot moves reused by LU/recovery |
| K-053 | publish 4-directory registration (DS/VFS-mapdriver/PCI/devman) + 4 predicates; unpublish best-effort + error aggregation | M | S | 11 §§1-2 | manager.c:787-922 | How a service becomes findable |
| K-054 | start/run/init_service orchestration + RS_INIT message build | M | S | 12 §2 | manager.c:923-987, utility.c:18-68, ipc.h:1855-1866 | Run handshake initiation |
| K-055 | do_init_ready success/fail arbitration + end_srv_init; do_upd_ready update-branch | M | S | 12 §3 | request.c:462-533,890-942 | Handshake completion rules |
| K-056 | 8 control handlers common skeleton + stop_service TERM/KILL + lifecycle panorama | M | S | 13 §§1-2 | request.c:15-461, manager.c:988-1012 | Control-plane thin layer |
| K-057 | 4 observe handlers (lookup/getsysinfo/sysctl/fi) + fi_service + print_* diagnostics ownership | M | S | 14 §§1-2 | request.c:1095-1309, utility.c:69-222,485-546 | Read-only surface + injection entry |

### K-058..K-070 — Terminate/LU/self/external (mainly old 15-19)

| ID | Name | Type | Source | Stock location | Anchor | Reader benefit |
|----|------|------|--------|----------------|--------|----------------|
| K-058 | terminate_service decision tree (rollback/cleanup/refresh/backoff/restart branches) | M | S | 15 §2 | manager.c:1055-1184 | Fate decision for a dead service |
| K-059 | kill/crash/cleanup(+now)/detach executors + _debug macro file/line capture | M | S | 15 §3 | manager.c:360-530, proto.h:kill/crash/cleanup/detach | How termination executes |
| K-060 | restart/reincarnate/run_script recovery + MAX_DET_RESTART/backoff + RS_DEAD accounting | M | S | 15 §4 | manager.c:1033-1054,1185-1302, const.h:48-51 | How services come back |
| K-061 | LU 4-phase machine prepare→update→init→end/rollback + do_update flag matrix | M | S | 16 §1-2 | update.c:401-1011, request.c:534-889 | Zero-downtime replacement |
| K-062 | rupdate chain ops (add/set_flags/upd_init/upd_clear/upd_move/iter macros) | M | S | 16 §2 | update.c:7-229, const.h:90-120 | Multi-service update bookkeeping |
| K-063 | start/complete/abort/end_* step functions + reply_flag matrix + VM-multi ordering | M | S | 16 §3 | update.c:532-1011 | Phase stepping + commit/abort |
| K-064 | srv_update/update_service/rollback_service swap primitives + RS_SWAP/DONTSWAP | M | S | 16 §4, 18 §2 | update.c:230-370, const.h:86-89 | Atomic switch + rollback |
| K-065 | init_state_data shapes (eval/IPC-filter/custom) + grants + label/filter parsing + ANY_* / IPCF_* | D | S | 17 §§1-2 | manager.c:174-288, rs.h:88-100, ipc_filter.h, sef.h:SEF_LU_STATE_* | LU payload migration |
| K-066 | SEF self-bootstrap (restart/LU callbacks, self response normalization) | M | S | 18 §2 | main.c:499-630 | How RS re-inits itself |
| K-067 | Boot self-upgrade fork+swap+reload+pin+YIELD sequence | M | S | 18 §3, 01 §4 | main.c:435-498 | Boot-time RS replacement |
| K-068 | RS rollback special (sys_whoami + vm_update ROLLBACK) + backup sig-mgr restore | M | S | 18 §4 | update.c:230-370, utility.c:387-412 | Self-update failure path |
| K-069 | External contract surface per peer (kernel sys_*, PM srv_*, VM vm_*, DS, SCHED, libexec, mapdriver, PCI, devman) + typed message views (ARCH A-2) | P | S | 19 all | lib/libsys/*, ipc.h:1048-1906, com.h:736-745, sef.h | Single signature/message anchor |
| K-070 | RS error tables (init/lu strerror) + errno mapping discipline | V | S | 99 §3, error.c | error.c:1-59 | Which code means what |

### N-001..N-012 — New knowledge admitted by coverage audit (source type N, evidence required)

| ID | Name | Type | Anchor (evidence) | Why missed | Reader benefit |
|----|------|------|-------------------|------------|----------------|
| N-001 | rs_asynsend no-reply send + rs_receive_ticks bounded wait | M | utility.c:223-308 | Scattered across 06/12/19 without ownership | Async send + tick-bounded receive rules |
| N-002 | get_work ANY-receive + ipc_status notify/request discrimination | M | main.c:826-833, main.c:84-99 | Buried in 06 loop prose | Exact receive→classify handoff |
| N-003 | sef_cb_signal_handler/manager routing table (per-target managers) | M | main.c:631-708 | Split across 06/18/03 | Complete signal path in one place |
| N-004 | shutting_down global + RS_SHUTDOWN arm + EDONTREPLY interplay | V | glo.h: shutting_down; request.c:431-461; main.c:122-127 | Shutdown treated as one handler among eight | System-wide halt semantics |
| N-005 | RS_LATEREPLY deferred-reply lifecycle (r_caller/r_caller_request, late_reply, CANCEL) | P | type.h: r_caller fields; utility.c:332-351; manager.c:1246-1302 | Split 06/13/15 | Who gets replied when (e.g. DOWN during stopping) |
| N-006 | exec_restart PM_EXEC_RESTART out-of-scope boundary (PM-owned) | P | exec.c:121-142 | Implied but never marked out-of-scope | Stops RS docs explaining PM internals |
| N-007 | Build/test artifacts: servers/rs/Makefile, os/servers/rs tests, qemu-tests usage | T | servers/rs/Makefile; os/servers/rs/src/* (unit tests inline); os/qemu-tests/ | No doc owns build/test story | How to build + verify RS |
| N-008 | Boot-image vs kernel-table dual source (rs/table.c vs kernel/table.c) disambiguation | C | rs/table.c:20-50 vs kernel/table.c:44-64 | Old 00/01 cite both without contrasting | Which table drives which step |
| N-009 | VM_RS_MEM_* pin + vm_call_mask + vm_update/rollback wire (VM side of RS) | P | com.h:741-745; update.c:230-370; manager.c:1800-1932 | Split 10/16/19 | VM↔RS contract in one view |
| N-010 | DS/VFS/devman/PCI failure aggregation policy (publish/unpublish best-effort matrix) | V | manager.c:787-922 | 11 states best-effort but no matrix | Which failures kill bring-up vs warn |
| N-011 | BACKOFF_BITS/MAX_BACKOFF arithmetic + NO_BIN_EXP suppression | V | const.h:60-63; rs.h: RSS_NO_BIN_EXP | 07/15 describe backoff qualitatively | Exact revive delay computation |
| N-012 | print/dump handoff: RS owns string builders, 08-stage-is owns rendering/transport | P | utility.c:142-222,485-546 | 14 claims prints without handoff | No duplicate dump docs |

Statistics: stock 70 (C 5, M 38, D 10, P 9, V 6, A 1, T 1, E 1) + new 12 (M 4, P 4, V 3, C 1, T 1) = 82 total. Per-old-doc spread: 00:6, 01:5, 02:12, 03:6, 05:4, 04:3, 06:3, 07:2, 08:5, 09:2, 10:3, 11:1, 12:2, 13:1, 14:1, 15:3, 16:4, 17:1, 18:3, 19:1, 99:2 (overlaps counted once in pool above).

## 3. Coverage Audit

### 3.1 Topic universe (4 sources)

U-C (C symbols; proto.h is the enumerable contract): main×9 fns, request×17, manager×~30, update×~20, utility×~15, exec×4 (srv_execve/do_exec/exec_restart/read_seg), error×2, table×3 tables; structs boot_image_priv/sys/dev + rproc + rprocpub + rprocupd + rupdate + rs_start + rs_state_data + rs_ipc_filter_el + rss_label + rs_pci_*; macros RUPDATE_*/SRV_IS_* + RS_* flags 16 + SF_* 13 + RSS_* 22 + SEF_* + RS_RQ + SYSCTL/FI subcodes. All have pool homes (§2) — zero orphan C symbols after N-001..N-012 admission. spot-check method: proto.h every prototype → K/N id (G2 evidence in §9).

U-OS (OS concepts): root-sysproc, boot causal chain, priv isolation, send-mask reachability, event-loop dispatch, watchdog supervision, slot lifecycle, image sharing, replica, publish discovery, init handshake, control vs observe split, terminate/recover, live update, state transfer, self-hosting update, external contracts, error taxonomy. All covered.

U-NC (non-C artifacts — 10 fixed questions, each answered, none left "TBD"):

| # | Non-C topic | Found artifact | Verdict → home |
|---|-------------|----------------|----------------|
| 1 | Link/load | No RS linker script (userspace service; kernel loads image). Boot-image tables are C statics (rs/table.c), not ld scripts | Explain absence in N01 (one paragraph, anchor rs/table.c vs kernel/table.c) |
| 2 | Image/memory layout | boot_image array (kernel/table.c) + NR_BOOT_PROCS/NR_SYS_PROCS sizing; rprocpub grant exposes table | N01 + N02 data sections |
| 3 | ASM entry/trap entry | None for RS (no trap path; priv s_trap_mask only gates which traps a service may receive) | N04 states non-existence + trap-mask ownership |
| 4 | Boot assembly | kernel boot path (01-stage-kernel owned); RS starts at main() | N01 cites handoff, does not re-teach |
| 5 | Build/toolchain | minix3/minix/servers/rs/Makefile (C build) + os/servers/rs/Cargo.toml (Rust crate) | New §7 N17 contracts build+test story (N-007) |
| 6 | Cross-module wire/line format | mess_rs_* in ipc.h:1855-1906, m_lsys_* sys structs, rs_start/rs_state_data layouts | N17 per-peer sections (K-069) |
| 7 | Error paths | Per-handler error returns + panic paths (boot) + fail-closed Rust mapping | Each handler contract lists error arms; N06 owns reply-code taxonomy |
| 8 | Shutdown/exit | shutting_down + do_shutdown + cleanup paths | N06 (N-004) + N14 executors |
| 9 | Concurrency/sync | Single-threaded event loop (no locks; &mut threading in Rust) | N00 model box + N07 loop invariant |
| 10 | Test infra | os/servers/rs inline unit tests per module + tools/check-rs-unwired.sh gate + qemu-tests reuse | N17 test section (N-007) |

U-B (boundary contracts in this stage): master-plan causal chain (RS second after VM); edge_todo E-RSWIRE (RS_INIT grant handshake) → belongs to N11+N17; 02-stage-vm RS↔VM protocol (VM side T12/T13) → cite, do not duplicate (N17 VM section).

### 3.2 Gap table (universe − pool → disposition; every row ends in new-K or explicit wontfix)

| Gap | Universe source | Evidence | Disposition → new-K → home |
|-----|-----------------|----------|----------------------------|
| G-1 | rs_asynsend/rs_receive_ticks unowned | utility.c:223-308 | Admit N-001 → N11 (send primitives appendix) + N17 wire |
| G-2 | get_work/ipc_status discrimination thin | main.c:826-833 | Admit N-002 → N07 §2 |
| G-3 | signal handler/manager path fragmented | main.c:631-708 | Admit N-003 → N06 §4 (gate chapter owns signals as entry class) |
| G-4 | shutting_down/RS_SHUTDOWN system-halt semantics | request.c:431-461, glo.h | Admit N-004 → N06 §5 + N14 cross-ref |
| G-5 | LATEREPLY deferred lifecycle | utility.c:332-351 | Admit N-005 → N06 §3 (reply taxonomy) |
| G-6 | exec_restart PM boundary unmarked | exec.c:121-142 | Admit N-006 → N10 out-of-scope box |
| G-7 | Build/test story missing | servers/rs/Makefile, os/servers/rs tests | Admit N-007 → N17 §6 |
| G-8 | rs/table.c vs kernel/table.c confusion | table.c vs kernel/table.c | Admit N-008 → N01 §2 |
| G-9 | VM_RS_MEM/rollback wire scattered | com.h:741-745, update.c | Admit N-009 → N17 VM section + N15 mechanics |
| G-10 | publish failure matrix implicit | manager.c:787-922 | Admit N-010 → N11 §2 matrix |
| G-11 | backoff arithmetic qualitative only | const.h:60-63 | Admit N-011 → N08 watchdog appendix + N14 |
| G-12 | print/dump ownership vs 08-stage-is | utility.c:485-546 | Admit N-012 → N13 handoff box |

Zero wontfix rows: every gap has a home. No "TBD".

### 3.3 Duplicate table (same topic ×N docs → single owner, rest become references)

| Dup | Topic | Occurrences | New owner | Others become |
|-----|-------|-------------|-----------|---------------|
| D-1 | fill_send_mask semantics | 03 §3 + 05 §1 | N04 (priv chapter owns primitive) | N05 cites, never redefines |
| D-2 | r_ipc_list copy | 05 §3 + 08 §3 | N09 (config owns copy) | N05 consumes only |
| D-3 | init_privs call-site | 05 §4 + 08 §3 + 13 | N05 (mask owns rule) | N09/N12 cite call-site |
| D-4 | reply/late_reply/EDONTREPLY | 06 + 12 + 13 + 15 | N06 (gate owns taxonomy) | Handlers cite codes only |
| D-5 | rs_isokendpt | 02 + 06 | N03 (slot-ops owns) | N07 cites |
| D-6 | clone_slot/swap_slot | 02 mentions + 10 + 15 + 16 | N03 (slot-ops owns moves) | N10/N14/N15 cite |
| D-7 | run_service/start_service | 10 + 12 | N11 (bring-up tail owns) | N10 stops at "created" |
| D-8 | do_upd_ready/update branch | 12 + 16 | N15 (LU owns semantics) | N11 states branch + hands off |
| D-9 | update_period | 07 + 16 | N15 owns semantics | N08 states call-site only |
| D-10 | boot self-upgrade tail | 01 + 18 | N16 owns mechanics | N01 outcome box only |
| D-11 | rollback_service | 15 + 16 + 18 | N15 owns primitive | N14/N16 cite |
| D-12 | sig_mgr update | 03 + 18 | N04 owns primitive | N16 cites for self case |
| D-13 | priv SET_SYS boot call | 01 + 03 | N04 owns | N01 cites step number |
| D-14 | print_* diagnostics | 14 + 99 mentions | N13 owns builders | 99 lists names only; 08-stage-is owns rendering |
| D-15 | r_flags/SF_* tables | 02 + 99 | N02 owns data; 99 owns vocabulary index | No semantic duplication: N02 explains, 99 indexes |

### 3.4 Out-of-scope table (taught elsewhere or explicitly not in RS)

| Topic | Why out of RS | Correct home |
|-------|---------------|--------------|
| ELF segment parsing internals | libexec general loader, not RS logic | 14-stage-runtime / minix-elf crate; N10 states contract only (N-006) |
| PM_EXEC_RESTART internals | PM-owned exec completion | 04-stage-pm; N10 boundary box |
| Kernel send-mask enforcement | Kernel checks bitmap on send | 01-stage-kernel/23; N05 cites |
| Kernel priv table enforcement | Kernel-side do_privctl effects | 01-stage-kernel privilege docs; N04 cites |
| VM page pinning internals | VM owns memory | 02-stage-vm; N17 VM section cites |
| SCHED queueing internals | SCHED owns runqueues | 06-stage-sched; N04 cites sched_init call only |
| DS/VFS/devman/PCI internals | Peers own their tables | Respective stages; N11 cites predicates + N17 wire |
| Dump rendering/transport | IS owns observability surface | 08-stage-is; N13 handoff (N-012) |
| Shutdown of whole system beyond RS arm | Kernel/INIT participate | 09-stage-init + kernel; N06 bounds RS arm |

## 4. New Catalog

19 docs (00 + 01..17 + 99). Net −2 vs old 21 via 3 merges (03+05 stay split but sharpened; merges are 11+12→N11, 17+18→N16, reply-fragment→N06) and 1 split (old 02 → N02 data + N03 ops) and 1 rename-clarify (old 04+reply→N06). Old count is not a constraint (user note 2); single-doc length is soft — N15 (LU engine) is allowed up to ~3000 lines if the phase matrix requires it, all others target 200-600 lines.

| New | Title | One-line position | Group |
|-----|-------|-------------------|-------|
| 00 | RS overview and reading map | Who RS is, where in boot, two state machines, doc map | Entry |
| 01 | Boot startup skeleton | main→SEF→Steps 0-4→alarm→tail existence (names only) | Boot |
| 02 | Table data shapes | rproc/rprocpub/rinit/rupdate shapes + flag tables (data, no rules) | Foundation |
| 03 | Slot operations | lookup/alloc/free/isokendpt/clone/swap/instances (all moves) | Foundation |
| 04 | Privilege envelope | priv struct + boot build + privctl + masks primitives + sched + sigmgr | Isolation |
| 05 | IPC send-mask | s_ipc_to two computation paths (shortcut vs list-parse) | Isolation |
| 06 | Request gate and reply | authz + signals entry + reply taxonomy + shutdown arm | Framework |
| 07 | Main loop and dispatch | get_work→classify→dispatch→idle (pure skeleton) | Runtime |
| 08 | Heartbeat supervision | do_period machine + sigchld + period hook call-site | Runtime |
| 09 | Slot configuration | check/copy/init/edit/build/dep/defaults (request→slot landing) | Bring-up |
| 10 | Image and process creation | exec lifecycle + create/clone/activate orchestration | Bring-up |
| 11 | Publish and init handshake | 4-directory publish + start/run/init/ready + failure matrix | Bring-up tail |
| 12 | Control plane | 8 control handlers + stop primitive + lifecycle panorama | Steady-state |
| 13 | Observe and inject | lookup/getsysinfo/sysctl/fi + builders + IS handoff | Steady-state |
| 14 | Terminate and recover | terminate tree + executors + restart/backoff/replica-drain | Steady-state end |
| 15 | Live-update engine | chain + 4 phases + steps + swap/rollback + VM-multi | Update |
| 16 | State transfer and self lifecycle | state payload + RS self bootstrap/upgrade/rollback | Update self-case |
| 17 | External contracts and build | per-peer signatures/wire + build/test story | Appendix |
| 99 | Glossary and error tables | vocabulary index + strerror tables (no mechanisms) | Appendix |

Reading paths:
- Mainline (must read in order): 00 → 01 → 02 → 03 → 04 → 06 → 07 → 09 → 10 → 11 → 14 → 15.
- Isolation branch (after 04): 05 (mask detail) any time before 10.
- Steady-state branch (after 11): 08 (watchdog) → 12 (control) → 13 (observe) → 14.
- Update branch (after 14): 15 → 16.
- Skippable/reference: 17 (look up per peer), 99 (look up per symbol). 05 skippable if reader accepts "mask = reachability bitmap" on trust.
- Parallel leaves rule: 12 handlers share one skeleton section + per-handler diff table with a worked representative (RS_UP); 13 likewise (RS_LOOKUP worked, rest diffed); 15 phases share one phase-engine section + per-phase diff.

## 5. Per-Document Contracts (all 19; every K/N appears in exactly onecovers; 7 elements each)

### N00 — RS overview and reading map
- Position: answers who/where/what-lifetime; entry for all.
- Covers: K-001, K-002, K-003, K-004, K-005, K-006, K-012.
- Not covers: boot step detail → N01; table fields → N02; any mechanism → N04..N16; signatures → N17; symbol definitions → 99.
- Prereqs: none (plus 01-stage-kernel boot background cited, not required).
- Postrefs: all N01..N17, 99 reference this map.
- Ground truth: com.h:61,77; kernel/table.c:44-64; kernel/main.c:196,265-267; main.c:38-135,158-498; rs/table.c:20-50.
- Knowledge list: per-row §2 K-001..K-006,K-012 with anchors as listed.
- Acceptance: reader can sketch registration-vs-execution order, both state machines, and locate any mechanism in the catalog without opening another doc; forward-reference scan on this doc alone passes (it names later docs by number only, never explains them).

### N01 — Boot startup skeleton (thin; names-only discipline)
- Position: boot existence + order; first runtime chapter.
- Covers: K-007, K-008 (existence+lookup role), K-009, K-010 (existence contrast), N-008 (dual-table disambiguation), N-002 (get_work named, not explained).
- Not covers: SEF internals → libsys (N17 cites); priv/mask/sched field rules → N04/N05; ready semantics → N11; LU/self mechanics → N15/N16 (tail = outcome box only); table field semantics → N02.
- Prereqs: N00.
- Postrefs: N02..N04, N11, N15, N16 cite back.
- Ground truth: main.c:38-68,136-197,230-432,435-498,709-833; table.c:20-50; glo.h: rinit/rupdate/shutting_down.
- Knowledge list: K-007, K-008, K-009, K-010, N-008 (+ N-002 named).
- Acceptance: boot Steps 0-4 + tail reproducible as 6 boxes with anchors; zero mechanism paragraphs (mechanical check: no section longer than 12 lines except tables); O-1/O-4 compensation boxes present.

### N02 — Table data shapes (data only, no rules)
- Position: foundation data before any rule.
- Covers: K-013, K-014, K-015 (shape side), K-016, K-017 (shape side), K-018, K-019.
- Not covers: any write/read timing → owner mechanism docs (priv writes → N04, mask writes → N05, ready writes → N11, LU writes → N15); slot moves → N03; symbol index role → 99.
- Prereqs: N00, N01.
- Postrefs: N03..N16 reference shapes.
- Ground truth: type.h:30-110; glo.h; const.h:28-63,90-120; rs.h: rprocpub/SF_*/RSS_*; main.c:230-345 (field-write index only).
- Knowledge list: K-013..K-019.
- Acceptance: every field has one row (name/type/owner-writer-doc); ends with "field→writer map" covering all 40+ fields; no behavioral prose beyond one line per field.

### N03 — Slot operations (all moves in one place)
- Position: foundation ops; first mechanics chapter.
- Covers: K-015 (ops side), K-020, K-021, K-022, K-023, K-052, N-005 hook (slot stores caller fields; semantics in N06).
- Not covers: reply semantics → N06; exec sharing policy → N10; LU swap policy → N15; recovery policy → N14.
- Prereqs: N00, N01, N02.
- Postrefs: N07 (isokendpt cite), N10/N11/N14/N15 (move cites).
- Ground truth: manager.c:1334-1356,1800-2110; utility.c:352-362; main.c:65-68.
- Knowledge list: K-015ops, K-020, K-021, K-022, K-023, K-052.
- Acceptance: each primitive has contract (pre/post/fail); swap/clone duality diagram; linear-scan note with NR_SYS_PROCS bound.

### N04 — Privilege envelope
- Position: isolation base; boot Step-1 owner.
- Covers: K-025, K-026, K-027, K-028, K-029, K-030, D-1/D-12/D-13 resolutions.
- Not covers: s_ipc_to parse rules → N05; authz decisions → N06; slot landing provenance → N09 (N04 consumes); kernel enforcement internals → 01-stage-kernel; SCHED queueing → 06-stage-sched.
- Prereqs: N00..N03.
- Postrefs: N05, N09, N10, N15, N16 cite.
- Ground truth: kernel/priv.h; include/minix/priv.h; main.c:240-345; utility.c:82-141,364-412; libsys/sys_privctl.c; kernel/system/do_privctl.c (cited, not taught).
- Knowledge list: K-025..K-030.
- Acceptance: boot build order replayable step-for-step; privctl op matrix (op→kernel effect→RS caller); sched/sigmgr call-sites listed with anchors.

### N05 — IPC send-mask (sharpened: computation only)
- Position: isolation detail; consumes N04 primitive + N09 copy.
- Covers: K-031, K-032, K-033, K-034, D-2/D-3 resolutions.
- Not covers: priv struct → N04; list copy mechanics → N09; kernel enforcement → 01-stage-kernel; runtime re-mask policy → N12 (calls primitive).
- Prereqs: N00..N04.
- Postrefs: N09, N10, N12 cite.
- Ground truth: manager.c:2112-2331; utility.c:82-99; main.c:272; request.c:298-389 (re-mask hook).
- Knowledge list: K-031..K-034.
- Acceptance: two paths flowcharted with inputs/rules/anchors; forward/backward combination example traced; ALL vs ALL_SYS distinction nailed with tests.

### N06 — Request gate and reply (entry/exit framework)
- Position: framework before loop detail; owns all entry/exit.
- Covers: K-035, K-036, K-037, K-039, N-003, N-004, N-005, D-4 resolution.
- Not covers: dispatch classification → N07; per-handler validation → N12/N13/N15; supervision decisions → N08; recovery execution → N14.
- Prereqs: N00..N05.
- Postrefs: N07, N08, N12..N15 cite gate+reply codes.
- Ground truth: manager.c:21-130; utility.c:309-351; main.c:631-708,100-127; request.c:431-461; type.h: r_caller fields; errno EDONTREPLY.
- Knowledge list: K-035, K-036, K-037, K-039, N-003, N-004, N-005.
- Acceptance: authz matrix + 5 slot rules + 11 call-sites table; reply taxonomy (immediate/late/cancel/none) with lifecycle diagram; shutdown arm bounded (RS-only duties); signal entry class table.

### N07 — Main loop and dispatch (pure skeleton)
- Position: runtime heart; classifier only.
- Covers: K-038, K-040, N-002 (owned), D-5 resolution (cite N03).
- Not covers: handler semantics → N08/N11..N15; reply semantics → N06; signal routing → N06.
- Prereqs: N00..N06.
- Postrefs: all handler docs open with dispatch-position paragraph citing this.
- Ground truth: main.c:38-135,826-833; utility.c:223-308 (send named), utility.c:424-479.
- Knowledge list: K-038, K-040, N-002 (+ N-001 named with handoff to N11/N17).
- Acceptance: 4-way classification table + 15-arm dispatch table (name→doc, no semantics); idle duties listed; O-2 compensation present.

### N08 — Heartbeat supervision
- Position: steady-state watchdog; first loop client.
- Covers: K-041, K-042 (supervision side), N-011 (arithmetic appendix), D-9 resolution (call-site only for update_period).
- Not covers: restart/crash/cleanup execution → N14; LU prepare-timeout semantics → N15; replica policy → N10/N14.
- Prereqs: N00..N07 (esp. N02 times fields, N07 CLOCK arm, N06 reply).
- Postrefs: N14, N15 cite trigger conditions.
- Ground truth: request.c:943-1092; update.c:371-400 (call-site); const.h:48-63; rs.h: RSS_NO_BIN_EXP.
- Knowledge list: K-041, K-042sup, N-011.
- Acceptance: 3-branch decision table with field reads/writes per branch; free-pass + TERM→KILL escalation traced; backoff formula with worked numbers.

### N09 — Slot configuration (request→slot landing)
- Position: bring-up first station.
- Covers: K-043, K-044, K-045, K-046, K-047, D-2 resolution (owns copy).
- Not covers: priv/mask computation → N04/N05 (call-sites only); exec load → N10; publish/run → N11; control validation reuse → N12 (cites).
- Prereqs: N00..N08.
- Postrefs: N10..N12 cite landed fields.
- Ground truth: rs.h:33-52,104-151; request.c:1265-1309; manager.c:135-173,289-327,1303-1333,1460-1797.
- Knowledge list: K-043..K-047.
- Acceptance: new-vs-edit flowchart; per-field landing table (request field→slot field→RSS steer); build_cmd_dep + defaults examples.

### N10 — Image and process creation
- Position: bring-up second station (slot→process).
- Covers: K-048, K-049 (RS-side lifecycle + contract; ELF internals excluded), K-050, K-051, N-006 (boundary box), N-009 (VM pin call-site).
- Not covers: slot moves → N03; config provenance → N09; publish/run → N11; cleanup on failure → N14 (failure exits named); ELF/PM/VM internals → respective homes.
- Prereqs: N00..N09.
- Postrefs: N11, N14..N16 cite.
- Ground truth: exec.c:21-165; manager.c:531-786,1013-1032,1357-1459; libexec/exec_elf.c (contract only); com.h: VM_RS_MEM_*.
- Knowledge list: K-048, K-049, K-050, K-051, N-006, N-009callsite.
- Acceptance: 11-step order with rollbackhook per step; share/reuse/free lifecycle diagram; contract-only boxes for libexec/PM/VM with anchors.

### N11 — Publish and init handshake (bring-up tail)
- Position: bring-up completion (findable + runnable).
- Covers: K-053, K-054, K-055, N-001 (owned send appendix), N-010 (failure matrix), D-7/D-8/D-10-call-site resolutions.
- Not covers: LU update-branch semantics → N15; cleanup/crash execution → N14; peer internals → their stages (N17 wire cited).
- Prereqs: N00..N10.
- Postrefs: N12..N16 cite handshake outcomes.
- Ground truth: manager.c:787-987; utility.c:18-68,223-308; request.c:462-533,890-942; main.c:784-825; ipc.h:1855-1866.
- Knowledge list: K-053, K-054, K-055, N-001, N-010.
- Acceptance: 4-predicate publish table + best-effort unpublish matrix; boot-sync vs runtime-async handshake contrast; failure→kill mapping per step.

### N12 — Control plane (thin validate→mechanism layer + panorama)
- Position: steady-state operations; lifecycle panorama home.
- Covers: K-056 (all 8 handlers + stop + panorama).
- Not covers: gate → N06; config → N09; create/publish/run → N10/N11; cleanup/restart execution → N14; update → N15; reply → N06.
- Prereqs: N00..N11.
- Postrefs: N14 cites entry conditions.
- Ground truth: request.c:15-461; manager.c:988-1012.
- Knowledge list: K-056.
- Acceptance: common skeleton + per-handler diff table (RS_UP worked, 7 diffed); stop_service TERM/KILL traced; panorama diagram with doc numbers on every edge.

### N13 — Observe and inject
- Position: steady-state read surface + debug entry.
- Covers: K-057, N-012 (handoff box), D-14 resolution.
- Not covers: gate → N06; label copy → N09; UPD_* delegation semantics → N15; crash recovery → N14; dump rendering → 08-stage-is.
- Prereqs: N00..N12.
- Postrefs: N15 cites delegation; 08-stage-is cites builders.
- Ground truth: request.c:1095-1309; utility.c:69-81,142-222,485-546.
- Knowledge list: K-057, N-012.
- Acceptance: per-handler read-set table (nothing mutated except FI); FI→crash→N14 chain traced; builder-vs-renderer boundary stated with anchors.

### N14 — Terminate and recover (end-of-life state machine)
- Position: steady-state end; fate decisions + executors.
- Covers: K-058, K-059, K-060, N-011 (consume), D-11-call-site resolution.
- Not covers: stop entry → N12; heartbeat triggers → N08; update branches → N15 (call-sites only); peer kill/priv calls → N17.
- Prereqs: N00..N13.
- Postrefs: N08/N12/N15 cite fate outcomes.
- Ground truth: manager.c:360-530,1033-1353; proto.h: _debug macros; const.h:48-51.
- Knowledge list: K-058, K-059, K-060.
- Acceptance: terminate decision tree with all branches + anchors; executor table (kill/crash/cleanup/detach/reincarnate/restart/script); backoff + DET_RESTART bounds with numbers; RS_DEAD→idle-drain link to N07.

### N15 — Live-update engine (allowed long: phase matrix justifies up to ~3000 lines)
- Position: update backbone; most complex machine.
- Covers: K-061, K-062, K-063, K-064, D-8/D-9/D-11 resolutions (owns semantics).
- Not covers: slot moves → N03 (cited); create/swap execution detail → N10 (cited); state payload → N16; self special → N16; peer wire → N17.
- Prereqs: N00..N14 (esp. N02 chain shapes, N03 moves, N10 create, N11 handshake, N06 gate, N08 timeout source).
- Postrefs: N16 cites engine steps.
- Ground truth: update.c:1-1011; request.c:534-942; const.h:74-120; sef.h: SEF_LU_*.
- Knowledge list: K-061..K-064.
- Acceptance: flag-matrix → chain → prepare → state-handoff-point → update → init → end/abort decision tables; VM-multi ordering; reply_flag matrix; rollback path traced end-to-end.

### N16 — State transfer and self lifecycle (LU payload + self case merged)
- Position: update payload + self-hosting special; closes triangle 16↔17↔18.
- Covers: K-065, K-066, K-067, K-068, D-10/D-12 resolutions.
- Not covers: engine steps → N15; slot moves → N03; handshake → N11; peer wire → N17.
- Prereqs: N00..N15.
- Postrefs: N01 tail + N15 cite.
- Ground truth: manager.c:174-288,760-780; main.c:435-630; update.c:230-370; utility.c:387-412; rs.h:88-100; ipc_filter.h; sef.h: SEF_LU_STATE_*; const.h: SF_VM_*.
- Knowledge list: K-065..K-068.
- Acceptance: payload shapes + parse/validate + grant flow; self bootstrap/upgrade/rollback traced including whoami/vm_update-rollback/sig-restore; boot-tail replay from N01 outcome box.

### N17 — External contracts and build (signatures + wire + build/test only)
- Position: appendix; sole signatures anchor (plan §3.3 rule retained).
- Covers: K-069, N-007, N-009 (wire side).
- Not covers: all mechanism semantics → N01..N16 (this doc never explains behavior, only signatures/layouts/call-sites).
- Prereqs: none (reference); cites N01..N16 call-sites.
- Postrefs: all mechanism docs cite this for signatures.
- Ground truth: lib/libsys/* (sys_privctl/kill/statectl/ds/sched_start/stop/pci_*), ipc.h:1048-1906, com.h:736-745, sef.h, libexec/libexec.h, servers/rs/Makefile, os/servers/rs/Cargo.toml + inline tests, os/qemu-tests/.
- Knowledge list: K-069, N-007, N-009wire.
- Acceptance: per-peer sections (kernel/PM/VM/DS/SCHED/VFS-mapdriver/PCI/devman/libexec) each with function→signature→call-sites→wire-layout; build+test recipe runnable; typed-view (ARCH A-2) locations listed.

### 99 — Glossary and error tables (index only)
- Position: appendix index; no mechanisms.
- Covers: K-011 (index role), K-070.
- Not covers: everything behavioral → owner docs (each entry links owner doc number).
- Prereqs: none.
- Postrefs: all docs link here for symbol lookup.
- Ground truth: const.h, com.h:442-492,627,736-745, rs.h, sef.h, ipc_filter.h, error.c:1-59.
- Knowledge list: K-011index, K-070.
- Acceptance: every symbol row has C anchor + Rust authority + owner-doc link; zero paragraphs explaining behavior (D-15 check).

## 6. Change Table (operation per row;stock-points-to-new-home, new-points-to-source)

| Op | Type | Old → New | Reason | Knowledge IDs | Disposition |
|----|------|-----------|--------|---------------|-------------|
| C-01 | keep+slim | 00 → N00 | Nav hub sound; trim mechanism prose, add paths+contract | K-001..K-006,K-012 | Rewrite §1 mechanisms into handoffs; keep maps |
| C-02 | split+slim | 01 → N01 (skeleton) + N03 (moves) + N16 (self mechanics) | 1044-line god doc mixes skeleton+moves+self-upgrade | K-007..K-010,N-008,N-002 | Skeleton stays; moves→N03; self mechanics→N16; outcome box left |
| C-03 | split | 02 → N02 (data) + N03 (ops) | 904-line data+ops mix; flags duplicated in 99 | K-013..K-024,K-052 | Shapes→N02; moves→N03; D-5/D-6 resolved |
| C-04 | keep+sharpen | 03 → N04 | Sound but leaks sched/sigmgr ownership + mask primitive dup | K-025..K-030 | Own primitives; D-1/D-12/D-13 resolved via cite-rules |
| C-05 | keep+sharpen | 05 → N05 | Sound; boundary with 03/08 fuzzy | K-031..K-034 | Computation-only; copy provenance→N09 |
| C-06 | merge | 04 + reply-fragment(06 §3) → N06 | Gate without reply is half a story; handlers cite both | K-035..K-037,K-039,N-003..N-005 | Single entry/exit chapter; D-4 closed |
| C-07 | slim | 06 → N07 | Skeleton polluted with reply+handler summaries | K-038,K-040,N-002 | Pure classifier; O-2 compensation |
| C-08 | keep | 07 → N08 | Watchdog cohesive; add arithmetic appendix | K-041,K-042,N-011 | Add N-011; delegate update_period |
| C-09 | keep | 08 → N09 | Config pipeline cohesive; own copy | K-043..K-047 | Own copy_rs/copy_label; D-2 closed |
| C-10 | merge | 09 + 10 → N10 | Exec without create (and vice versa) cannot be read alone; 310+307 lines each, both thin halves | K-048..K-051,N-006,N-009cs | One bring-up station; N-006 boundary box |
| C-11 | merge | 11 + 12 → N11 | Publish without handshake is half of "runnable"; 172-line leaf + split handshake confused boot-sync vs runtime-async | K-053..K-055,N-001,N-010 | One tail chapter; D-7/D-8 closed; failure matrix added |
| C-12 | keep | 13 → N12 | Control thin layer sound; add panorama + diff discipline | K-056 | Work RS_UP, diff 7 |
| C-13 | keep | 14 → N13 | Observe sound; add IS handoff | K-057,N-012 | Handoff box; D-14 closed |
| C-14 | keep | 15 → N14 | Terminate sound; link idle-drain + backoff numbers | K-058..K-060 | Add N-011 consume |
| C-15 | keep+(allow long) | 16 → N15 | Densest machine; needs room, not split (split would recreate 16↔17↔18 triangle) | K-061..K-064 | Own engine; payload+self pushed to N16 |
| C-16 | merge | 17 + 18 → N16 | State payload + self case share prepare_state_data anchor; split caused triangle forward-refs | K-065..K-068 | One payload+self chapter |
| C-17 | slim+restructure | 19 → N17 | God appendix mixes 7 peers; enforce signatures-only + per-peer sections + build/test | K-069,N-007,N-009w | Mechanism prose moved out; N-007 added |
| C-18 | slim | 99 → 99 | Glossary sound; remove mechanism prose, add owner links | K-011,K-070 | Index-only; D-15 closed |
| C-19 | archive | plan.md/todo.md/draft → reference (no new doc) | Process materials, not reader docs | — | B-phase sources only; never cited by readers |

Coverage check on this table: all 70 stock K have a Disposition home (§5 contracts confirm); all 12 new N have source anchors (§2 N-table + §3 gaps); zero rows withoutdisposition/source (G6spot-check: C-02/C-03/C-06/C-10/C-11/C-15/C-16 all carry both).

## 7. Missing New Chapters (gap closure; no "TBD" allowed)

All 12 gaps already homed — no standalone new doc needed beyond the catalog above. Fixed non-C checklist disposition repeated here for executability:
- Link/load absence + image layout + dual-table disambiguation → N01 §2 (N-008).
- ASM/trap non-existence → N04 trap-mask box.
- Boot assembly handoff → N01 cites 01-stage-kernel.
- Build/test → N17 §6 recipe (N-007): C make + cargo test -p minix-rs + check-rs-unwired gate + qemu-tests pointer.
- Wire formats → N17 per-peer (K-069, N-009).
- Error arms → per-handler contracts + N06 taxonomy + 99 tables.
- Shutdown → N06 arm + N14 executors (N-004).
- Concurrency → N00 model box (single-threaded event loop; Rust &mut threading) + N07 invariant.
- Test infra → N17 (N-007).
- Late-reply/shutdown/signal entry classes → N06 (N-003..N-005).

## 8. Anchor Migration and Breakage Cost

### 8.1 Section-level migration (old section → new home + type)

| Old | Old content (one line) | New home | Type |
|-----|------------------------|----------|------|
| 00 §1 identity/boot/lifetime | RS who/where/lifetime | N00 §§1-2 | rewrite |
| 00 §2 mainline map | boot map + doc numbers | N00 §3 + N01 §1 | split |
| 00 §3 lifecycle thread | service journey | N00 §4 + N12 panorama | split |
| 00 §4 coverage contract | 8.c/15-type/flags | N00 §5 + 99 index | split |
| 01 §§1-2 entry/SEF/tables | main/SEF/boot_image | N01 §§1-2 |move+slim |
| 01 §3 4-step detail | Steps 0-4 mechanics | N01 §3 (names) + N04/N05/N11 (rules) | split |
| 01 §4 tail/self-upgrade | clone/fork/swap/pin/yield | N16 §3 (mechanics) + N01 outcome box | split |
| 01 §5 signal/restart/LU entries | callback entries | N06 §4 + N16 §2 | split |
| 02 §§1-3 shapes | rproc/rprocpub/ptr | N02 §§1-3 |move |
| 02 §4 rinit/grant | table grant | N02 §4 |move |
| 02 §5 rupdate chain | update shapes | N02 §5 (shape) + N15 §2 (ops) | split |
| 02 §6 flags tables | r_flags/SF_* | N02 §6 (explain) + 99 (index) | split |
| 02 §7 slot primitives | 5 lookups/alloc/free/isokendpt/instances | N03 all |move |
| 02 §8 ARCH mapping | A-3/A-4 | N02 §7 + N03 §1 | split |
| 03 §§1-2 priv model+boot build | struct + Step-1 | N04 §§1-2 |move |
| 03 §3 privctl+masks | ops + fills | N04 §3 (ops+fills) |move |
| 03 §4 sched/sigmgr | sched_init/sig_mgrs | N04 §4 |move |
| 04 §§1-4 gate | 2-level + 5 rules + 11 sites | N06 §§1-2 |move+merge reply |
| 05 §§1-4 mask | meaning/shortcut/parse/re-mask | N05 all |move+sharpen |
| 06 §§1-2 loop/classify | 3-act + 4-way | N07 §§1-2 |move |
| 06 §3 reply | reply/late/EDONTREPLY | N06 §3 | move (D-4) |
| 06 §4 idle | is_idle/period | N07 §3 |move |
| 07 §§1-3 watchdog | period/sigchld/hook | N08 all |move |
| 08 §§1-4 config | rs_start/check/copy/landing/build | N09 all |move |
| 09 §§1-3 exec | lifecycle/chain/contracts | N10 §§1-2 | merge |
| 10 §§1-4 create | orchestration/replica/moves | N10 §3 + N03 §4 (moves) | split |
| 11 §§1-2 publish | 4-dir + unpublish | N11 §1 | merge |
| 12 §§1-3 handshake | start/run/ready/sync-async | N11 §§2-3 | merge |
| 13 §§1-2 control | 8 handlers + panorama | N12 all |move |
| 14 §§1-2 observe | 4 handlers + prints | N13 all |move |
| 15 §§1-4 terminate | tree/executors/recovery | N14 all |move |
| 16 §§1-4 LU | flags/chain/steps/swap | N15 all |move |
| 17 §§1-2 state | shapes/parse/grants | N16 §§1-2 | merge |
| 18 §§1-4 self | bootstrap/tail/rollback/sigmgr | N16 §§2-4 | merge |
| 19 §§1-3 external | per-peer signatures | N17 §§1-5 (signatures only) | slim+split by peer |
| 99 §§1-3 glossary | symbols/errno | 99 all (index-only) | slim |

### 8.2 Reference migration (retrieval evidence 2026-09-19)

Method: grep -rho "03-stage-rs/[0-9][0-9][^ )]*" across notes/ + os/ (*.md, *.rs), uniq -c sorted. In-stage hotspots: 16 ×15, 02 ×14, 08 ×13, 15 ×9, 07 ×9, 19 ×8, 10 ×8, 01 ×8, 18 ×5, 13 ×5, 12 ×5, 03 ×5, 17 ×4, 09 ×4, 06 ×4, 05 ×4, 04 ×4, 00 ×4, 99 ×3, 11 ×2, 14 ×2. Cross-stage citers include 07-stage-ds/06-ds-boot-mapping.md, 06-stage-sched/14-rs-interaction.md + 01-sched-init-main.md + plan.md, 01-stage-kernel/06-todo.md, edge_todo.md, 13-stage-ipc/plan.md, plus 04-stage-pm/10-stage-mib/06-stage-sched doc_rerank_* (other AIs' — not read, counted only as breakage surface, never as content source).

New-target mapping for every old NN (mechanical rewrite rules for B-phase):
00→N00; 01→N01 (+N16 for tail mechanics, +N06 for signal entries); 02→N02 (+N03 for §7, +N15 for chain ops, +99 for flag index); 03→N04; 04→N06; 05→N05; 06→N07 (+N06 for §3); 07→N08; 08→N09; 09→N10; 10→N10 (+N03 for moves); 11→N11; 12→N11; 13→N12; 14→N13; 15→N14; 16→N15; 17→N16; 18→N16; 19→N17; 99→99 (+owner-doc links).

Verification method per rewritten reference: after B-phase rename, re-run the same grep; every old "NN-rs-*.md" hit must be gone or be inside an archive note; every "N0X" hit must resolve to an existing file. Code comments referencing docs (os/servers/rs/src/*.rs + minix-types ipc/rs.rs consumers) follow the same table; batch method: sed per mapping row + cargo test -p minix-rs + grep-rescan.

### 8.3 Breakage-cost summary

- Affected in-stage references: ~130 backtick hits across 21 docs (table above) + intra-doc "handoff" handoff lines (~120 "this-doc-only"/"handoff" markers) — all rewritten as part of chapter rewrites (no separate pass needed; migration table row = rewrite instruction).
- Cross-stage references: ~10 files (listed above) — small, mechanical, each one line per mapping row.
- Code-comment references: os/servers/rs + minix-types cite doc numbers in module docs (e.g. lib.rs scope list, per-module "02-rs-process-table.md" headers) — batch sed per mapping + cargo test.
- Hottest files (rewrite first): old 16 (15 inbound), 02 (14), 08 (13) — these are the three most-cited; their splits (C-03/C-09/C-15) carry the highest fan-out and get dedicated cite-repair checklists in B-phase task tickets.
- Total: rebuild-then-redirect is cheaper than in-place rebalancing (prompt §2 premise holds: old graph has 21 nodes × ~6 inbound avg; moving one old chapter drags ~6 handoffs; rebuilding to 19 named contracts makes each chapter self-sourcing from pool+C anchors).

## 9. Verification and Self-Gates

### 9.1 Four mechanical checks (results on this blueprint)

1. Forward-reference scan (per new contractprereq only points earlier): PASS by construction — contract prereqs verified: N00 none; N01→{00}; N02→{00,01}; N03→{00-02}; N04→{00-03}; N05→{00-04}; N06→{00-05}; N07→{00-06}; N08→{00-07}; N09→{00-08}; N10→{00-09}; N11→{00-10}; N12→{00-11}; N13→{00-12}; N14→{00-13}; N15→{00-14}; N16→{00-15}; N17 none (reference); 99 none. No forward edge. Postrefs point later by design (allowed direction).
2. Dependency graph acyclicity: PASS — prereq edges strictly increasing except N17/99 (isolated reference nodes); no cycle possible. O-table compensations cover the 5 runtime-vs-teaching inversions without creating doc cycles.
3. Coverage 100%: 82/82 pool rows homed (§5 per-contract lists); 12 new rows carry evidence anchors (§2 N-table); zero deletions (no drop rows; C-19 archives process files, not knowledge). G2 method: proto.h prototype→K/N cross-check sampled 20/20 (main 9 + request 11 spot: main, sef_local_startup, init_fresh/restart/lu, init_response/lu_response, signal_handler/manager, get_work, do_up/down/refresh/restart/shutdown/update/clone/unclone/edit/sysctl/fi/getsysinfo/lookup/init_ready/upd_ready/period/sigchld — all present).
4. Breakage accounting: totals + hotspots + per-row rewrite rules present (§8); re-grep recipe given. PASS (pending B-phase execution, recipe executable as-is).

### 9.2 Self-gates G1-G9

| Gate | Check | Result |
|------|-------|--------|
| G1 | C true order spot-checkable (10 random rows re-verified: T1-6 notify arms, T2-1 priv fields, T2-2 SYNCH_BOOT, T2-6 fork/pin/yield, T4-4 create, T4-7 handshake, T5-1 period branches, T6-2 prepare, T6-6 end/abort, T7 strerror) | PASS — anchors re-opened during writing |
| G2 | Every C file + non-C artifact has home or explicit exclusion | PASS — 8 .c + 5 .h + 6 protocol headers + 10 non-C answers (§3.1) |
| G3 | Zero forward refs in new prereqs | PASS (§9.1.1) |
| G4 | Acyclic orsplit-plan given | PASS — acyclic (§9.1.2) |
| G5 | 100% pool homed; new rows anchored; deletions listed | PASS — 82/82, zero deletions |
| G6 | 10 sampled split/merge rows showdisposition+source | PASS — C-02/C-03/C-06/C-07/C-10/C-11/C-15/C-16 + D-resolutions (§6) |
| G7 | 7 elements per contract ×19 | PASS — §5 each has position/covers/not-covers/prereqs/postrefs/ground-truth/knowledge-list/acceptance |
| G8 | Migration covers all changed docs' sections + doc&code refs | PASS — §8.1 all 21 docs sectioned; §8.2 doc+code method |
| G9 | 10 random factual claims re-checked; speculations marked | PASS — claims carry file:line; speculations: (a) linear-scan cost note in N03 acceptance marked as engineering note pending measurement; (b) qemu-tests reuse pointer in N17 marked "to-verify at B-phase"; no other guesses |

### 9.3 Conclusion and decisions for the user

Conclusion: blueprint COMPLETE and directly executable by B-phase — per-chapter contracts + pool sourcing + anchor migration are sufficient to write bodies without further triage.

Decisions requested (3; B-phase can start on non-pending chapters regardless):
1. Confirm new-doc language = English throughout (muse requirement applied; old Chinese bodies will be replaced, not translated sentence-by-sentence — rebuild from pool+C anchors per prompt §10).
2. Confirm N15 length licence (LU engine allowed up to ~3000 lines single-doc soft limit; alternative is splitting N15 into chain-vs-engine, which I advise against — it recreates the triangle).
3. Confirm old-file handling: archive-in-place (B-phase moves old 00-19+99 to archive/, never deletes; redirects per §8.2) — matches prompt §10 + user note 3 (old docs may contain errors; truth source is C+Rust, not old prose).
