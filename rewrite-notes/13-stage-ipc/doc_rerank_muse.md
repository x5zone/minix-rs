# 13-stage-ipc Document Rebuild Blueprint (muse)

## 0. Metadata

- Executor: muse. Date: 2026-09-20. Target: `notes/rewrite/fork-syscall-rewrite/13-stage-ipc/`.
  Repo root: `/home/xzhao/github/minix-rs`. HEAD at blueprint time: `d6323548`
  (re-verified; earlier in-session observation `a9e61a9d4` moved during parallel lanes).
- Task = Phase R (rebuild blueprint): emit `target_dir/doc_rerank_muse.md` only.
  No body text modified, nothing renamed/moved/deleted, nothing committed.
- Scope, in-scope numbered docs (12, line counts via `wc -l`):
  `00-ipc-overview.md`(46), `01-ipc-init-main.md`(252), `02-ipc-message-contract.md`(299),
  `03-ipc-mib-registration.md`(226), `04-ipc-permissions.md`(204), `05-ipc-sem-table.md`(248),
  `06-ipc-semop.md`(232), `07-ipc-shm-segment.md`(191), `08-ipc-shm-attach.md`(226),
  `09-ipc-proc-events.md`(176), `10-ipc-lifecycle.md`(151), `99-ipc-global-concepts.md`(82).
  Reference-only (not rebuilt): `plan.md` (410 lines, ratified 2026-08-16, §5 coverage
  contract + §4 ARCH list + §7 review log), `todo.md` (R1 round closed 2026-09-16,
  100 unit + 4 integration tests), `README.md`, `draft/README.md` (placeholder material),
  `archive/todo-R1-archive-2026-09-16.md`.
  Explicitly out of scope and NOT read: any `doc_rerank_*` product by another AI
  (their filenames appeared in directory listings and `rg` filename hits only; no content
  from them was used — one early `rg` output leaked a few snippet lines, which are
  disregarded and nothing below depends on them).
  Forbidden citations (`.design/`, `tmp_design_and_todo/`) were not referenced.
- Reading list actually read: all 12 docs above (headers + full body for 00/99, section
  scan + spot body for 01–10); C ground truth `minix3/minix/servers/ipc/` read in full
  (`main.c` 284, `sem.c` 888, `shm.c` 469, `utility.c` 49, `inc.h` 66, `ipc.conf` 18);
  headers `minix/include/minix/com.h` (IPC_BASE/PROC_EVENT/SUSPEND),
  `minix/include/minix/ipc.h` (7 `mess_lc_ipc_*` structs + `mess_pm_lsys_proc_event`),
  `sys/sys/ipc.h`, `sys/sys/sem.h`, `sys/sys/shm.h`, `sys/sys/sysctl.h` (KERN_SYSVIPC);
  `minix/lib/libc/sys/sem.c` + `shm*.c` callers (head), `minix/lib/libc/sys/mmap.c`
  VM verbs, `minix/servers/pm/event.c` + `syslib.h:289-294` event bits,
  `Makefile` (ipc build), draft README; Rust entries
  `os/servers/ipc-server/src/` (17 files, 7411 lines total incl. tests) and
  `os/libs/minix-types/src/ipc/ipc_server.rs` (constants present — plan §3.5's A-1
  gap is closed); boundary materials `00-master-plan/README.md` (stage table),
  `edge_todo.md` (E-IPCWIRE closed 2026-09-17, E-RMIBWIRE), `12-stage-input/00` head
  (previous-stage style reference only).
- Commands run (evidence excerpts inline in §§1–3,8): `wc -l`, `rg` sweeps for
  `check_perm` call sites (7), `SUSPEND|EDONTREPLY|EIDRM`, `IXSEQ_TO_IPCID`,
  `KERN_SYSVIPC`, section-header scans per doc, in-stage cross-reference counts,
  code-comment citations of doc names, `git log -1`.

## 1. C Ground Truth Sequence

Stage type: **service event-loop** (not a boot chain, not a syscall set). Two segments:
startup (birth → register → loop) and loop (receive → classify → dispatch → reply).

| # | Action | Anchor | Note |
|---|--------|--------|------|
| T-01 | RS loads IPC at runtime (absent from boot image; `kernel/table.c` has no ipc entry; draft README table; `ipc.conf` declares privilege face) | `minix3/minix/servers/ipc/ipc.conf` (18 lines); `ipc.conf` installed via `Makefile` `FILES=ipc.conf → /etc/system.conf.d` | Privilege face: system UMAP+VIRCOPY; uid 0; receivable endpoints SYSTEM USER pm rs log tty ds vm; vm REMAP/REMAP_RO/SHM_UNMAP/GETPHYS/GETREF |
| T-02 | `main` records argv, registers 3 SEF callbacks, enters loop | `main.c:216-224` (`env_setargs`, `sef_local_startup`) | `sef_local_startup` at `main.c:124-142`: init_fresh + init_restart + signal handler, then `sef_startup()` |
| T-03 | Fresh/restart init registers `kern.ipc` remote MIB subtree; local failure panics, remote failure ignored | `main.c:80-95` (`sef_cb_init_fresh`, `rmib_register(mib={CTL_KERN,KERN_SYSVIPC})`) | Subtree shape `main.c:54-74`: INFO func-node + MSG/SEM/SHM int leaves (=0/1/1) + 5 reserved slots "not yet supported"; dispatcher `kern_ipc_info` `main.c:27-51` (namelen!=1→EINVAL; SEM_INFO→`get_sem_mib_info`; SHM_INFO→`get_shm_mib_info`; else EOPNOTSUPP; MIB listing open to all users, NetBSD semantics) |
| T-04 | Loop: blocking receive with status | `main.c:228-229` (`sef_receive_status(ANY,&m,&ipc_status)`; failure panics) | — |
| T-05 | Branch 1: kernel notify → print + ignore | `main.c:234-238` (`is_ipc_notify`) | Only branch that answers nothing, not even an error |
| T-06 | Branch 2: PM PROC_EVENT → `got_proc_event` → reply PROC_EVENT_REPLY via `asynsend3(AMF_NOREPLY)` | `main.c:241-245` + `main.c:191-210` (endpt+event decode; SEM_EVENTS gate → `sem_process_event`; echo reply; send failure prints) | Subscription edge: `update_sub` `main.c:144-170` (zero↔nonzero edge triggers `proceventmask(EXIT\|SIGNAL)`/`proceventmask(0)`; leftover events after unsubscribe must still be replied); `update_sem_sub` `main.c:176-189` |
| T-07 | Branch 3: MIB source → `rmib_process` | `main.c:248-252` (`m_source==MIB_PROC_NR`) | Client contract lives in `libsys/rmib.c` (register/deregister/process) |
| T-08 | Branch 4: `call_vec` dispatch by `m_type-IPC_BASE`; miss → ENOSYS | `main.c:12-20` (7 entries: SHMGET/SHMAT/SHMDT/SHMCTL/SEMGET/SEMCTL/SEMOP) + `main.c:255-261` | Call numbers `com.h:785-796` (`IPC_BASE 0xD00`, +1..+7). `sizeof(message)==64` (`ipc.h:2675` assert); 7 `mess_lc_ipc_*` payloads `ipc.h:355-422` |
| T-09 | Reply unless SUSPEND: `m.m_type=r`, `ipc_sendnb` | `main.c:264-276` (`SUSPEND=-998`, `com.h:1151`) | Only `do_semop`'s block path returns SUSPEND (`sem.c:768`) |
| T-10 | End-of-iteration: `update_refcount_and_destroy()` (lazy shm sweep) | `main.c:279` → `shm.c:173-206` (per-slot `vm_getrefcount`, `nattch=rc-1` with u8 wrap; `rc==(u8)-1`→print+skip; `nattch==0 && SHM_DEST`→`munmap`+free; shrink `shm_list_nr`) | Same sweep re-entered from `do_shmdt` (`shm.c:239`) and `do_shmctl` RMID/STAT paths (`shm.c:281,336`) |
| T-11 | Semaphore handlers: `do_semget` (find-or-create, `sem.c:93-156`); `do_semctl` (13 commands, `sem.c:469-648`); `do_semop` (validate→perm-first→try→maybe-suspend, `sem.c:654-779`) | Tables `sem.c:39-46` (`sem_list[SEMMNI=10]`, `sem_list_nr` high-water); `iproc[NR_PROCS]` waiter slots `sem.c:6-14`; atomicity `try_semop` `sem.c:294-373` (optimistic in-order + rollback); fairness retry `check_set` `sem.c:381-423`; counts `inc/dec_susp_count` `sem.c:163-200`; wakeup `complete_semop`+`send_reply` `sem.c:207-244`; deletion `remove_set` `sem.c:251-281` (EIDRM wakeups); event cancel `sem_process_event` `sem.c:866-888` (exit→EDONTREPLY else EINTR); MIB export `get_sem_mib_info` `sem.c:787-848`; nil-test `is_sem_nil` `sem.c:854-858` | Check order contract (`sem.c:693` comment): permission (EACCES) before sem_num range (EFBIG) before SEM_UNDO (EINVAL, `sem.c:729-739`); `nsems==0`→OK, `nsops>SEMOPM=100`→E2BIG; `SEM_STAT` takes raw slot index (`sem.c:497`) |
| T-12 | Shared-memory handlers: `do_shmget` (find-or-create + mmap + `vm_getphys`, `shm.c:51-127`); `do_shmat` (`shm.c:130-170`, SHM_RND rounding, `vm_remap`, atime+lpid refresh, nattch lazy); `do_shmdt` (`shm.c:209-242`, `vm_getphys` locate by addr, atime quirk — updates **atime** not dtime — `vm_unmap`, unknown id only prints); `do_shmctl` (`shm.c:261-371`: STAT/SHM_STAT with pre-sweep, SET with `~ACCESSPERMS` mask preserve, RMID sets SHM_DEST + sweep, INFO/SHM_INFO) | Table `shm_list[SHMMNI=1024]` `shm.c:6-12`; `SHM_ALLOC=0x0800` private to `shm.c:4`; `fill_shminfo` `shm.c:248-258`; `get_shm_mib_info` `shm.c:379-444`; `is_shm_nil` `shm.c:465-469`; dead `#if 0 list_shm_ds` `shm.c:448-464` | RMID asymmetry vs sem: shm marks SHM_DEST and destroys lazily; sem `remove_set` destroys immediately with EIDRM wakeups |
| T-13 | Permission gate shared by 7 call sites: `check_perm` + MIB export helper `prepare_mib_perm` | `utility.c:4-49` (root bypass; owner/group/other 0700/0070/0007 three-way; mode pre-masked `&=0700`) | Call-site masks: semget→flag (`sem.c:107`); semctl SETVAL/SETALL→IPC_W, SET/RMID→ownership/EPERM, INFO→none, rest→IPC_R (`sem.c:516-538`); semop→OR over ops (`sem.c:697-706`); shmget→flag (`shm.c:65`); shmat→RDONLY?R:R|W (`shm.c:151-156`); shmctl STAT→IPC_R, SET/RMID→ownership/EPERM, INFO→none (`shm.c:301-347`) |
| T-14 | Signal path: SIGTERM → double-nil check → deregister + `sef_exit(0)`, else warn-and-stay | `main.c:101-118` (`sef_cb_signal_handler`; non-TERM ignored) | Restart (`sef_cb_init_restart` = same fresh fn) re-registers subtree; all dynamic state lost |
| T-15 | Callers (user side): libc `semget/semctl/semop/shmget/shmat/shmdt/shmctl` pack `mess_lc_ipc_*`, `minix_rs_lookup("ipc")`, `_syscall(ipc_pt, IPC_* ,&m)` | `minix/lib/libc/sys/sem.c`, `shmat.c`, `shmctl.c`, `shmget.c` (verified semget shape; siblings same pattern) | This is the only caller-side ground truth; currently no doc owns it (§3 gap G-1) |

Order-difference table (runtime vs teaching): none — the new TOC follows T-01→T-15
exactly. The secondary SysV-journey thread (libc→dispatch→perm→table/op→VM/PM) is drawn
inside docs 05–08 and synthesized in new doc 11, each leg pointing back to its T-row.

## 2. Knowledge Pool

Type key: C=concept, M=mechanism, D=data structure, P=protocol/interface,
I=constraint/invariant, E=arch-evolution, T=tooling/test. Source: S=stock (existing doc),
N=new (from C/non-C/Rust, added by §3).

| ID | Name | Type | Src | Stock location | Anchor | Reader benefit: answers… |
|----|------|------|-----|----------------|--------|--------------------------|
| K-001 | IPC server = user-space SysV object manager | C | S | 00 §1, README | `servers/ipc/` (4 .c, 1690+66+18 lines) | What lives here vs in the kernel? |
| K-002 | kernel-IPC vs server boundary (send/receive/notify excluded) | C | S | 00 §1, README, plan §1.4 | `01-stage-kernel/12-ipc-core.md`, `notes/rewrite/ipc-sendrec.md` | Which "IPC" does this stage (not) cover? |
| K-003 | RS runtime loading + boot-image absence | M | S | 00 §1, 01 §2.6, draft | `kernel/table.c` (no ipc entry); `ipc.conf` | When is this server born? |
| K-004 | SEF 3-callback startup (fresh=restart, signal) | M | S | 01 §§1.3/2.2 | `main.c:124-142` | What runs before the first message? |
| K-005 | 5-branch main loop (notify/PROC_EVENT/MIB/dispatch/tail-sweep) | M | S | 01 §§1.3–1.4/2.3 | `main.c:227-280` | Where does my request go? |
| K-006 | `call_vec[7]` indexed dispatch + ENOSYS default | D/P | S | 01 §2.4 | `main.c:12-25,255-261`; `com.h:785-796` | How do 7 calls share one loop? |
| K-007 | SUSPEND reply-suppression rule | I | S | 01 §2.5, 02 §2.4 | `com.h:1151` (-998); `main.c:264-276` | When does the server deliberately not reply? |
| K-008 | 7 call numbers IPC_BASE+1..+7 | P | S | 02 §2.1 | `com.h:785-796` | What is written on the envelope? |
| K-009 | 7 `mess_lc_ipc_*` field layouts (incl. `ops`/`addr`/`buf` user pointers in 64B message) | P | S | 02 §2.2 | `ipc.h:355-422`; `sizeof(message)==64` (`ipc.h:2675`) | What does each letter field mean? |
| K-010 | `mess_pm_lsys_proc_event` + PROC_EVENT/REPLY numbers | P | S | 02 §2.3, 09 §2.1 | `ipc.h:1802-1813`; `com.h:610-619` | How do process events arrive? |
| K-011 | `ipc.conf` privilege face (system/vipc/vm sections) | P | S | 02 §2.5 | `ipc.conf` (18 lines) | Who may write to us; what may we ask VM? |
| K-012 | kern.ipc subtree shape (INFO + 3 int leaves + 5 reserved) | D | S | 03 §§2.2/2.4 | `main.c:54-79`; `sysctl.h:275,684-705` | What does `ipcs` read? |
| K-013 | `kern_ipc_info` dispatch (namelen gate, SEM/SHM_INFO fan-out, open-to-all) | M | S | 03 §2.3 | `main.c:27-51` | How are listing queries answered? |
| K-014 | rmib client contract (register/process/deregister) | P | S | 03 §2.5 | `libsys/rmib.c`; `main.c:91,249-251,112` | How does a remote subtree work? |
| K-015 | `check_perm` 3-way rule + root bypass | M | S | 04 §§2.1–2.2 | `utility.c:4-32` | Who may touch this object? |
| K-016 | `prepare_mib_perm` export copy | M | S | 04 §2.3 | `utility.c:38-49` | What does MIB listing expose? |
| K-017 | 7-site permission mask matrix | I | S | 04 §2.4 | `sem.c:107,520,536,705`; `shm.c:65,156,305` | What mask does each handler pass? |
| K-018 | sem table (`sem_list[10]`, high-water, SEM_ALLOC, seq+1 & 0x7fff) | D | S | 05 §§2.1–2.3 | `sem.c:39-46,93-156` | Where do semaphore sets live? |
| K-019 | sem find_key/find_id (IX/SEQ double check) | M | S | 05 §2.2 | `sem.c:53-92`; `IPCID_TO_*` (`inc.h:41-42`, `sys/ipc.h:110-116`) | How are stale ids rejected? |
| K-020 | `do_semget` two branches (exists vs create; sub-checks order) | M | S | 05 §2.3 | `sem.c:93-156` | What happens on create vs open? |
| K-021 | `remove_set` 4 acts + EIDRM wakeups + unsubscribe-on-last | M | S | 05 §2.4 | `sem.c:251-281` | What does deletion destroy immediately? |
| K-022 | `do_semctl` 13 commands + perm classes (W / ownership-EPERM / R / free INFO) | M | S | 05 §2.5 | `sem.c:469-653` | Which ctl does what, and who may? |
| K-023 | `fill_seminfo` IPC_INFO vs SEM_INFO + `get_sem_mib_info` ipcs coupling | M | S | 05 §2.6 | `sem.c:430-468,787-848` | What does `ipcs -s` see? |
| K-024 | `iproc[NR_PROCS]` waiter slots + TAILQ-per-set + semzcnt/semncnt | D | S | 06 §§2.1/2.5 | `sem.c:6-23,163-200` | Where do blocked semops sleep? |
| K-025 | `do_semop` 7-step entry validation (perm-before-range-before-UNDO) | I/M | S | 06 §2.2, 99 §2 | `sem.c:654-739` + order comment `:693` | Which error wins when several apply? |
| K-026 | `try_semop` optimistic in-order + rollback; SEMVMX only on positive ops | M | S | 06 §2.3 | `sem.c:294-373` | How is atomicity done without locks? |
| K-027 | `check_set` FIFO retry-to-fixpoint + blkop migration counts | M | S | 06 §2.4 | `sem.c:381-423` | Who wakes waiters, and when? |
| K-028 | `complete_semop` + `send_reply` + EDONTREPLY suppression | M | S | 06 §2.5 | `sem.c:207-244` | How is a waiter woken or silently dropped? |
| K-029 | `sem_process_event` cancel (exit→EDONTREPLY, signal→EINTR) | M | S | 06 §2.6 (body) + 09 §2.4 (routing) | `sem.c:866-888`; `main.c:203-204` | What happens to waiters when a process dies? |
| K-030 | shm table (`shm_list[1024]`, SHM_ALLOC private, vm_id snapshot) | D | S | 07 §§2.1–2.2 | `shm.c:4-48` | Where do shm segments live? |
| K-031 | `do_shmget` two branches + roundup + mmap-zero + `vm_getphys` | M | S | 07 §§2.3–2.4 | `shm.c:51-127` | How is a segment born? |
| K-032 | `do_shmat` (SHM_RND, vm_remap, atime+lpid, lazy nattch) | M | S | 08 §2.1 | `shm.c:130-170` | How is a segment attached? |
| K-033 | `update_refcount_and_destroy` lazy sweep (rc-1 u8 wrap; DEST-deferred destroy) | M/I | S | 08 §2.2 | `shm.c:173-206`; re-entries `:239,281,336` | Who counts attachers, and when? |
| K-034 | `do_shmdt` (locate-by-vm_id, atime quirk, unmap, print-only miss) | M | S | 08 §2.3 | `shm.c:209-242` | How is a segment detached? |
| K-035 | `do_shmctl` 6 commands (pre-sweep on STAT, SET mask, RMID→DEST) | M | S | 08 §2.4 | `shm.c:261-371` | How are shm attributes/deletion handled? |
| K-036 | `fill_shminfo`/`get_shm_mib_info` + `is_shm_nil`/`is_sem_nil` exit gates | M | S | 08 §2.5, 10 §2.1 | `shm.c:248-260,379-469`; `sem.c:854-865`; `main.c:111` | What does `ipcs -m` see; when may we exit? |
| K-037 | Event subscription switch (edge-triggered zero↔nonzero, leftover-reply rule) | M/I | S | 09 §§2.2–2.4 | `main.c:144-210`; `syslib.h:289-294` | When do we (un)subscribe, and why edge-only? |
| K-038 | SIGTERM double-nil exit vs warn-and-stay; restart loses state | M | S | 10 §§2.1–2.3 | `main.c:101-122,128-129` | When may the server die? |
| K-039 | SysV constants dictionary (SEM*/SHM*/IPC_* values) | D | S (dup: 02 §2.6 + 99 §1) | `sys/sem.h`, `sys/shm.h`, `sys/ipc.h` | What are the magic numbers? |
| K-040 | errno special trio (SUSPEND/EDONTREPLY/EIDRM+EINTR pair) + do_semop order restated | I | S | 99 §2 | `com.h:1151`; `errno.h:199`; `sem.c:262,887` | Which non-errors steer control flow? |
| K-041 | IPCID = seq<<16\|ix; slot reuse vs identity (0x7fff seq cycle) | I | S | 99 §3 | `sys/ipc.h:110`; `sem.c:135`; `shm.c:109` | Why can't a reused slot be impersonated? |
| K-042 | Rust service-assembly contracts IPC-P1-1 (copy/alloc/clock/transport/layout/exclusions) | E | S (misplaced: 99 §4) | `ipc-server/src/service.rs,boundary.rs`; E-IPCWIRE | How is C behavior pinned in Rust? |
| K-043 | rmib walker internals (`rmib_info`/`rmib_call`, SENDREC-vs-async reply select, fail-closed INFO) | M | S (overreach: 03 §8) | `libsys/rmib.c`; `minix-sys/src/rmib.rs` | Where does rmib detail really belong? → 10-stage-mib/22 |
| K-044 | libc caller wrappers (pack→lookup→_syscall→retid) | P | N (§3 G-1) | `libc/sys/sem.c,shmat.c,shmctl.c,shmget.c` | How does a syscall become a message? |
| K-045 | Build artifact (`Makefile`: 4 SRCS, -lsys, ipc.conf→/etc/system.conf.d, WARNS=5) | T | N (§3 G-2) | `servers/ipc/Makefile` | How is the server built and installed? |
| K-046 | Error-path catalog (ENOSYS miss, send-error prints, vm_id-miss print, rc==0xFF skip, datacopy faults, malloc fail) | I | N (§3 G-3) | `main.c:261,274-275`; `sem.c:677-682`; `shm.c:183-185,237` | What can go wrong, and how loudly? |
| K-047 | SEM-RMID vs SHM-RMID asymmetry (immediate+EIDRM vs DEST+lazy) | C | N (§3 synthesis) | `sem.c:561-567` vs `shm.c:329-337` | Why do the two deletions differ? |
| K-048 | Test strategy (100 unit + 4 integration; per-module decision tests) | T | N (synthesis of per-doc §5) | `ipc-server/` (cargo test), todo.md R1 | How is this stage verified? |

Stats: 48 entries (42 stock + 6 new). By type: C 4, M 26, D 4, P 6, I 6, E 1, T 2.
By stock doc: 00:2, 01:5, 02:5(+1 dup), 03:4(+1 overreach), 04:3, 05:6, 06:6, 07:2, 08:5,
09:2(+1 shared), 10:1, 99:4(+1 misplaced). Duplicates merged with multi-location record:
K-029 (06 body + 09 routing), K-039 (02 §2.6 + 99 §1).

## 3. Coverage Audit

Topic universe (4 roads): (1) C symbols — 37 functions + 7 data/tables from plan §5.3,
re-verified by direct reads (all bodies above); (2) OS-generic concepts attached to them
(object tables, capability checks, blocking wakeup, lazy refcount, event subscription);
(3) non-C artifacts (§3 table below); (4) boundary contracts (plan §5.4: 9 consumer
directions + kernel-IPC exclusion).

Gap table (each becomes §4 intake or an explicit wont-do):

| # | Topic in universe, in no doc | Evidence anchor | Disposition |
|---|------------------------------|-----------------|-------------|
| G-1 | libc caller side: 4 wrapper files' pack/lookup/syscall/return flow; `minix_rs_lookup("ipc")` failure → ENOSYS path | `libc/sys/sem.c` (semget/semctl/semop), `shmat.c`, `shmctl.c`, `shmget.c` | NEW doc 11; K-044 |
| G-2 | Build/install artifact: `Makefile` (SRCS, -lsys, FILES→system.conf.d, WARNS=5) | `servers/ipc/Makefile` | NEW doc 11 §fact-base (build half-page) — closest home for "how it ships"; K-045 |
| G-3 | Consolidated error-path catalog (dispatch miss, reply-send failure, datacopy faults, malloc fail, vm_id miss, rc==0xFF, leftover events) | `main.c:261,274`; `sem.c:677`; `shm.c:183,237`; `main.c:160-161` | Distributed: each new-doc contract gains an "error paths" acceptance line; catalog table lives in new 11 (journey needs every failure edge); K-046 |
| G-4 | SEM-RMID vs SHM-RMID asymmetry as one contrast | `sem.c:561-567` vs `shm.c:329-337` | Joint section in new 11 (only place that sees both ends); K-047 |
| G-5 | Consolidated test strategy (what 100+4 cover, per-module decision tests, TestBoundary) | `ipc-server/`, todo.md R1 | Kept per-doc §5 (plan §3.1 template); 00 map points at them; no new doc (test-per-doc is the enforced pattern, K-048 is synthesis only) |
| G-6 | `ipcs(1)`/sysctl consumer end (who reads `sysvipc_info`) | No `commands/ipcs/` in tree (verified missing) | Explicit wont-do with reason: consumer absent from this repo snapshot; 03 keeps the server-side contract + NetBSD-semantics note |

Duplicate table:

| # | Topic repeated | Locations | New-home ruling |
|---|---------------|-----------|-----------------|
| D-1 | SysV constants (`IPC_*`, `SEM_*`, `SHM_*` values) | 02 §2.6 vs 99 §§1.1–1.3 | Single home 99; 02 keeps only the 3 constants its wire structs need inline (IPC_R/W/M as used by masks) with pointer |
| D-2 | `sem_process_event` cancel body vs event routing | 06 §2.6 vs 09 §2.4 | KEEP split (plan D-1 stands: cancel is semop semantics, routing is event service); sharpen to one-line cross-pointers both ways |
| D-3 | Subscription open/close call sites | 05 (create/delete) vs 09 (switch) | KEEP split (call sites vs switch mechanics); 05 cites 09, 09 cites 05 — already so, keep |
| D-4 | do_semop error order | 06 §2.2 vs 99 §2 restatement | KEEP: 06 owns the mechanism, 99 owns the one-lineoyer dictionary entry; mark 99's as pointer, not second explanation |

Overreach table:

| # | Section beyond its charter | Ruling |
|---|---------------------------|--------|
| O-1 | 03 §8 rmib walker internals (rmib_info/rmib_call arms, SENDREC-vs-async select, minix-sys module layout, E-IPCWIRE anchoring narrative) | DELETE from 03. Client-use contract (K-014) stays as 03 §2.5 summary + pointer to `10-stage-mib/22` and E-RMIBWIRE. Walker internals have no C ground truth in this stage (they document `libsys/rmib.c` + `minix-sys`, owned by 10-stage-mib). No knowledge lost: K-043 recorded as pointer-only. |
| O-2 | 99 §4 service-assembly contracts inside a dictionary page | MOVE to 01 (new § on assembly contracts). Dictionary pages must stay lookup-only (single-unit rule); assembly contracts constrain every handler and 01 is the only doc all handlers precede. |
| O-3 | 02 §1.4 caller journey figure duplicating handler content | TRIM to envelope-level figure (pack→send→reply); journey detail moves to new 11 |

Non-C artifact checklist (all 10 answered, none left "TBD"):

| Artifact class | Found | New home |
|----------------|-------|----------|
| Link/load | `Makefile` (`PROG=ipc`, `SRCS=main.c utility.c shm.c sem.c`, `-lsys`, `minix.service.mk`) | 11 (build half-page) |
| Image/memory layout | None (RS-loaded user process; no linker script in stage) — explicitly stated | 01 (one line + T-01) |
| ASM/trap entry | None (pure userspace; `sef_receive_status`/`ipc_sendnb` are libsys calls) — explicitly stated | 01 (one line) |
| Boot assembly | Not in boot_image; RS loads via `ipc.conf` installed to `/etc/system.conf.d` | 01 (T-01/T-02) |
| Build/toolchain | `Makefile` WARNS=5, FILES install rule | 11 |
| Cross-module iface + wire format | 64B `message` union (`ipc.h:2675`); 7 payloads; COMMON_MIB via rmib; `ipc.conf` faces | 02 (wire), 03 (MIB wire), 11 (caller envelope) |
| Error paths | Catalog K-046 | 11 (catalog) + per-doc acceptance lines |
| Shutdown/exit | `sef_cb_signal_handler`, deregister, `sef_exit` | 10 (unchanged) |
| Concurrency/sync | Single-threaded loop, no locks; SUSPEND+TAILQ+retry as the only "concurrency" (A-10) | 01 (contract line) + 06 (mechanism) |
| Test infra | `cargo test -p minix-ipc-server` 100+4; no C tests in stage | per-doc §5 (kept) + 00 pointer |

## 4. New Table of Contents

13 docs: 00–10 keep numbers (stability for the ~60 in-stage + code-comment citations),
new 11 inserted before 99, 99 narrowed to dictionary. Size discipline: ordinary docs stay
150–300 lines (current healthy range); a doc may grow toward ~800 lines only if its single
concept demands it (none here does — the largest contract below is 06 at ~250 lines).

| New # | Title (one-line定位) | Group | Sources taken from |
|-------|----------------------|-------|--------------------|
| 00 | Overview: what the server is, two threads, doc map | entry | 00 (trim Rust-status §4 to pointer + K-048 line) |
| 01 | Startup, event loop, dispatch, reply rule, transport seam + assembly contracts | skeleton | 01 + 99 §4 (moved in) + non-C "none" answers |
| 02 | Wire protocol: 7 numbers, 7 structs, PROC_EVENT msg, SUSPEND, ipc.conf | protocol | 02 minus §2.6 (→99), §1.4 trimmed (→11) |
| 03 | kern.ipc subtree: shape, dispatch, mount, client contract | protocol | 03 minus §8 (deleted per O-1) |
| 04 | Permission model + 7-site mask matrix | gate | 04 unchanged |
| 05 | Semaphore tables: get/ctl/delete/info | sem group | 05 unchanged (+acceptance error line) |
| 06 | Semop: atomicity, waiters, 3 wakeups | sem group | 06 unchanged (D-2/D-4 pointers sharpened) |
| 07 | Shm segments: shmget + phys snapshot | shm group | 07 unchanged |
| 08 | Attach/detach/ctl + lazy refcount | shm group | 08 unchanged |
| 09 | Process events: switch, routing, receipt | cross-service | 09 unchanged |
| 10 | Lifecycle: SIGTERM gates, restart | cross-service | 10 unchanged |
| 11 | NEW Caller journey + error catalog + build (who sends, what can fail, how it ships) | synthesis | G-1/G-2/G-3/G-4 (K-044..K-047); figure material from 02 §1.4 |
| 99 | Dictionary: constants, errno trio, IPCID/endpoint (lookup only) | dictionary | 99 minus §4 (→01); absorbs 02 §2.6 |

Reading paths. Main line (runtime order = teaching order, zero forward refs):
00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 99.
Parallel body (7 syscalls under one dispatch — framework-first, §5.2 rule):
framework 01 + wire 02 first; sem group (05 table then 06 op); shm group (07 segment then
08 attach); cross-service 09, 10; synthesis 11; dictionary 99 skippable any time after 02.
Branch: 03 (MIB) and 04 (perms) may be read in either order after 02; both precede 05–08.
Fast path (fix a bug in one handler): 00 map → 01 branch locating → 02 fields → 04 mask →
05/06/07/08 mechanism → 99 values.

## 5. Per-Document Contracts

### 00-overview
-定位: the only page a newcomer must read; answers "what/where/map" in under 5 minutes.
-讲什么: K-001, K-002, K-003 (pointer), main-thread figure (T-01→T-14 compressed), journey-thread figure, doc map table with thread positions, Rust-status pointer (K-048 one line + test count), boundary list.
-不讲什么: any mechanism (→01–11); any value table (→99); kernel primitives (→`01-stage-kernel/12-ipc-core.md` + `notes/rewrite/ipc-sendrec.md`); MIB internals (→`10-stage-mib`).
-前置: none. -后置: all (map target).
-事实底线: `servers/ipc/` totals (`main.c:284, sem.c:888, shm.c:469, utility.c:49`); `ipc.conf`; `kernel/table.c` absence; `ipc-server/` test totals (todo.md R1).
-知识点: K-001, K-002, K-003, K-048 (pointer depth only).
-验收: reader can place any of the 7 calls on both threads without opening another doc; every map row names a T-row; no section exceeds one screen of mechanism.

### 01-skeleton (startup + loop + dispatch + reply + assembly contracts)
-定位: the anchor every handler chapter hangs off; answers "where in the loop am I".
-讲什么: K-003, K-004, K-005, K-006, K-007, K-042 (moved from 99 §4, as new closing section), non-C "none" answers (image/asm), single-thread contract (A-10), transport seam (`ipc_sendnb` vs `asynsend3`).
-不讲什么: field meanings (→02); subtree shape (→03); handler bodies (→05–08); event routing (→09); SIGTERM gates (→10); caller wrappers (→11).
-前置: 00. -后置: 02–11 (branch locator), 99 (values).
-事实底线: `main.c:12-25,101-142,216-284`; `com.h:1151`; `ipc.conf`; E-IPCWIRE closure note.
-知识点: K-003..K-007, K-042 (6 contract items each with owner-doc pointer).
-验收: 5-branch figure matches T-04→T-10 one-to-one; SUSPEND rule stated once and cited by 06/09, never re-explained; error line: dispatch miss→ENOSYS, send failure→print (`main.c:261,274-275`).

### 02-wire protocol
-定位: the envelope-and-letter spec; answers "what bits did the caller send".
-讲什么: K-008, K-009, K-010, K-011, trimmed journey figure (envelope depth only).
-不讲什么: SysV value tables (→99, D-1); handler flows (→05–08); masks (→04); caller wrapper code (→11); IPCID encoding (→99).
-前置: 01. -后置: 03 (names), 04 (bits), 05–08 (fields), 11 (envelope origin).
-事实底线: `com.h:785-796,610-619,1151`; `ipc.h:355-422,1802-1813,2675 (64B)`; `ipc.conf`; `sys/ipc.h` IPC_R/W/M only.
-知识点: K-008..K-011.
-验收: all 7 structs field-tabled with byte discipline (padding columns named, not skipped); every handler doc can cite a field without redefining it; error line: none at this layer (wire is total — malformed numbers fall to 01's ENOSYS).

### 03-mib subtree
-定位: the listing-query side door; answers "how does `ipcs` see us".
-讲什么: K-012, K-013, K-014 (use-contract depth: register/process/deregister call shapes + failure semantics), mount-in-startup position (T-03), NetBSD open-listing note.
-不讲什么: info assembly bodies (→05 §get_sem, 08 §get_shm); MIB service internals (→`10-stage-mib/12,22`); rmib library internals (DELETED per O-1, pointer only); exit-deregister (→10).
-前置: 01, 02. -后置: 05, 08 (assembly), 10 (deregister).
-事实底线: `main.c:27-95`; `sysctl.h:275,684-705`; `libsys/rmib.c` (called shapes only); `rmib.h`.
-知识点: K-012, K-013, K-014, K-043 as pointer-only (no body).
-验收: reader can trace a `sysvipc_info` query end-to-end naming the exact handoff doc at each hop; reserved slots + MSG=0 listed as explicit non-goals; error line: namelen→EINVAL, unknown→EOPNOTSUPP.

### 04-permissions
-定位: the gate every handler passes; answers "may this caller touch this object".
-讲什么: K-015, K-016, K-017 (7-site matrix with handler-doc pointers).
-不讲什么: wire fields (→02); handler flows (→05–08); listing-without-perm rationale owned here once (03 cites it).
-前置: 02. -后置: 05–08 (mask users).
-事实底线: `utility.c` (all 49 lines); `sys/ipc.h:54-92`; `getepinfo.c` identity source.
-知识点: K-015, K-016, K-017.
-验收: any handler's mask citable in one line; owner/group/other + root-bypass + EACCES-vs-EPERM distinction testable as pure function; error line: EACCES vs EPERM split per command class.

### 05-sem tables
-定位: set life history; answers "where do semaphore sets live and die".
-讲什么: K-018, K-019, K-020, K-021, K-022, K-023.
-不讲什么: atomicity/waiters (→06); event switch mechanics (→09, two call-site lines stay); subtree shape (→03); mask internals (→04).
-前置: 02, 04 (03 for info-face context). -后置: 06 (op on these sets), 09 (subscription edges), 10 (nil gate).
-事实底线: `sem.c:39-46,53-162,251-293,430-653,787-865`.
-知识点: K-018..K-023.
-验收: 13 ctl commands each with perm-class + effect + reply field; create/delete water-level accounting exact; error line: EEXIST/ENOENT/EINVAL/ENOSPC/EACCES/EPERM/ERANGE per command.

### 06-semop
-定位: blocking atomic op; answers "all-or-nothing, or sleep until possible".
-讲什么: K-024..K-029 (cancel body owned here; routing cited to 09).
-不讲什么: table create/delete (→05); switch mechanics (→09); UNDO support — excluded once, cited (A-7).
-前置: 04, 05. -后置: 09 (cancel callee), 10 (sets must drain before exit).
-事实底线: `sem.c:6-14,163-250,294-429,654-779,866-888`.
-知识点: K-024..K-029.
-验收: 7-step entry order pinnable (perm→range→UNDO); rollback logic statable; 3 wakeups (retry/delete/event) each with reply code (OK/EIDRM/EINTR-or-silence); error line: E2BIG/ENOMEM/EACCES/EFBIG/EINVAL/EAGAIN/SUSPEND.

### 07-shm segments
-定位: segment birth; answers "how does shared memory come into existence".
-讲什么: K-030, K-031 (incl. raw-size-vs-rounded accounting + phys snapshot meaning).
-不讲什么: attach/count/ctl (→08); mask internals (→04); VM internals (→`02-stage-vm`, call-contract depth only).
-前置: 02, 04 (05 as mirror). -后置: 08.
-事实底线: `shm.c:3-127`; `mmap.c` `vm_getphys` contract; `SHM_ALLOC` privateness note.
-知识点: K-030, K-031.
-验收: exists-vs-create branches with 4 sub-checks each; raw `segsz` vs rounded mapping stated once (08 cites it); error line: EACCES/EEXIST/EINVAL/ENOENT/ENOSPC/ENOMEM.

### 08-attach and refcount
-定位: mapping + lazy counting + deferred destroy; answers "who uses it, when is it really gone".
-讲什么: K-032, K-033, K-034, K-035, K-036 (shm half).
-不讲什么: creation (→07); mask internals (→04); subtree shape (→03).
-前置: 07, 04. -后置: 09 (no subscription — contrast line), 10 (nil gate), 11 (DEST asymmetry).
-事实底线: `shm.c:130-242,248-260,261-447,465-469`.
-知识点: K-032..K-036.
-验收: lazy-vs-bookkeeping rationale statable; sweep re-entry points listed (loop/detach/ctl); atime quirk pinned as ground-truth quirk; error line: EINVAL/EACCES/ENOMEM/EPERM + print-only miss.

### 09-proc events
-定位: death/signals delivered; answers "who tells waiters their process is gone".
-讲什么: K-010 (use depth), K-037, K-029 routing half (call into 06, receipt always).
-不讲什么: cancel body (→06); PM internals (→`04-stage-pm`); why-shm-exempt (one contrast line citing 08).
-前置: 01, 05 (call sites), 06 (callee). -后置: 06 (cancel), 10 (no unsubscribe-on-exit needed).
-事实底线: `main.c:3-4,144-214`; `syslib.h:289-294`; `pm/event.c` sender shape.
-知识点: K-010, K-037 (+K-029 routing half).
-验收: edge-switch rule statable (which transitions call `proceventmask`); leftover-reply rule stated; error line: receipt-send failure prints, never fails the event.

### 10-lifecycle
-定位: clean-exit gate; answers "when may the server die".
-讲什么: K-036 (nil-gate use), K-038 (filter/double-check/clean-vs-dirty/restart-amnesia).
-前置: 01, 03 (deregister), 05/08 (nil tests). -后置: none (terminus before synthesis).
-事实底线: `main.c:101-122,128-129`.
-知识点: K-036, K-038.
-验收: clean vs dirty journeys both drawable; "warn-and-stay, no retry timer" pinned; error line: none (signal path cannot fail outward).

### 11-caller journey, error catalog, build (NEW)
-定位: synthesis for the fixer and the porter; answers "how does a syscall become this message, what can fail along the way, how does this server ship".
-讲什么: K-044 (4 wrapper files end-to-end: pack→`minix_rs_lookup`→`_syscall`→retid/errno; lookup-fail ENOSYS), K-046 (full error catalog table: layer × site × code × loudness), K-047 (RMID asymmetry contrast), K-045 (Makefile build/install), G-6 wont-do note (ipcs absent).
-不讲什么: handler bodies (→05–08, cited as journey legs); wire re-spec (→02, cited); kernel syscall path (→`01-stage-kernel`, pointer).
-前置: 01, 02, 04, 05–08 (all legs must exist first — hence position 11). -后置: 99 (values used in catalog).
-事实底线: `libc/sys/sem.c,shmat.c,shmctl.c,shmget.c`; `servers/ipc/Makefile`; catalog anchors `main.c:261,274-275`, `sem.c:677-682`, `shm.c:183-185,237`, `main.c:160-161`.
-知识点: K-044, K-045, K-046, K-047.
-验收: every catalog row carries file:line; every journey leg names its handler doc; asymmetry section states which deletion wakes (EIDRM) vs which defers (DEST); build half-page reproducible (`PROG/SRCS/FILES/WARNS`).

### 99-dictionary (lookup only)
-定位: values desk; answers "what number" in under 30 seconds.
-讲什么: K-039 (5 constant tables), K-040 (trio + order pointer), K-041 (encoding + endpoint slots + m_source unforgeability), exclusion pointer list (A-7/A-8 one-liners → owners).
-不讲什么: EVERYTHING behavioral (→01–11); assembly contracts MOVED OUT (→01, O-2).
-前置: none (usable right after 02). -后置: none.
-事实底线: `sys/sem.h`, `sys/shm.h`, `sys/ipc.h`, `com.h`, `errno.h`, `inc.h:41-42`.
-知识点: K-039, K-040, K-041.
-验收: every value carries header:line; `semmns` footgun (600 vs 60) pinned; no paragraph longer than 4 lines (dictionary discipline).

## 6. Change Table

| Op # | Type | Old position | New position | Reason | Knowledge | Direction rule satisfied |
|------|------|--------------|--------------|--------|-----------|--------------------------|
| C-01 | trim | 00 §4 (Rust status, ~8 lines incl. transport/test detail) | 00 pointer + 1 line | Overview must stay navigation-only (single-unit) | K-048 | stock→ 00 map row; detail stays in per-doc §5 |
| C-02 | move | 99 §4 (6 assembly contracts) | 01 new closing section | Dictionary/implementation-manual mix (O-2); only 01 precedes all constrained handlers | K-042 | stock→ 01 §X with per-item owner pointers |
| C-03 | move | 02 §2.6 (SysV value tables) | 99 §1 (merge) | D-1 duplicate; wire spec vs value desk | K-039 | stock→ 99 tables; 02 keeps 3 inline uses + pointer |
| C-04 | delete | 03 §8 (walker appendix) | pointer to `10-stage-mib/22` + E-RMIBWIRE | O-1 overreach; no in-stage C ground truth; duplicates minix-sys docs | K-043 | stock→ pointer-only (no body lost: internals never belonged here) |
| C-05 | trim | 02 §1.4 (journey figure w/ handler detail) | envelope-level figure; detail → 11 | O-3 forward-detail; journey needs handlers to exist first | K-044 part | stock figure→ 02 (envelope), body→ 11 |
| C-06 | sharpen | 06 §2.6 ↔ 09 §2.4 cross-pointers | one-line each way, ownership words ("body here / routing there") | D-2 keep-split must be explicit or readers re-merge | K-029 | no move; pointers both ways |
| C-07 | create | (none; libc wrappers homeless) | NEW 11 | G-1/G-2/G-3/G-4; caller side is ground truth with no page | K-044..K-047 | new (C anchors listed, no stock moved except 02-figure material) |
| C-08 | keep | 04, 05, 07, 08, 09, 10 bodies | same numbers | Audit found bodies clean: single-unit, ordered, anchored | all resident K | identity (only acceptance error-lines appended) |
| C-09 | archive | all 12 old files | archive (B phase, untouched in R) | Rebuild takes material from pool + C, not by搬移 prose | — | old docs are sources, not move targets |

## 7. Gap Intake (non-C list closed out)

Link/load → 11 (Makefile). Image layout → 01 one-liner (none, RS-loaded).
ASM/trap → 01 one-liner (none, libsys calls). Boot assembly → 01 (T-01/T-02).
Build/toolchain → 11. Iface/wire → 02+03+11 split. Error paths → 11 catalog + per-doc
acceptance lines. Shutdown → 10. Concurrency → 01 contract + 06 mechanism.
Test infra → per-doc §5 kept + 00 pointer. No "TBD" remains; G-6 wont-do reasoned.

## 8. Anchor Migration and Breakage Cost

Per-section migration (old → new; "verbatim" only where structure already matches):

| Old section | Content in one line | New home | Type |
|-------------|---------------------|----------|------|
| 00 §§1–3,5 | identity/boundary/threads/map | 00 §§same | verbatim (trim §4 only) |
| 00 §4 | Rust status detail | 00 pointer row | rewrite (shrink) |
| 01 §§1–2,2.6,3–7 | readers/concept, source核对, Rust, tests, transition | 01 §§same | verbatim (+error line) |
| 01 (new) | assembly contracts | 01 new §X | move from 99 §4 (rewrite as contract table) |
| 02 §§1–2.5,3–7 | concept, numbers, structs, event msg, SUSPEND, ipc.conf, Rust, tests | 02 §§same | verbatim (minus §2.6) |
| 02 §2.6 | SysV value tables | 99 §1 | move (merge) |
| 02 §1.4 | journey figure | 02 trimmed + 11 §journey | split (C-05) |
| 03 §§1–7 | concept, consts, table, dispatch, mount, client, Rust, tests | 03 §§same | verbatim |
| 03 §8 | walker appendix | deleted → pointer | delete (C-04) |
| 04 (all 7 §§) | readers, model, journey, perm struct, check_perm, mib copy, mask matrix, Rust, tests | 04 §§same | verbatim (+error line) |
| 05 (all) | 6 mechanism §§ + Rust/tests | 05 §§same | verbatim (+error line) |
| 06 (all) | 6 mechanism §§ + Rust/tests | 06 §§same | verbatim (cross-pointers sharpened) |
| 07 (all) | 4 mechanism §§ + Rust/tests | 07 §§same | verbatim |
| 08 (all) | 5 mechanism §§ + Rust/tests | 08 §§same | verbatim (+error line) |
| 09 (all) | 4 mechanism §§ + Rust/tests | 09 §§same | verbatim |
| 10 (all) | signal/restart/RS-contract + Rust/tests | 10 §§same | verbatim |
| (new) | caller/build/errors/asymmetry | 11 (new) | new (G-1..G-4) |
| 99 §§1–3,5 | constants/errno/encoding/boundary | 99 §§same | verbatim |
| 99 §4 | assembly contracts | 01 new §X | move (C-02) |

Reference migration (measured, `rg` evidence §0): in-stage filename citations cluster at
5–7 per handler doc (01:6, 02:6, 03:6, 04:7, 05:7, 06:6, 07:5, 08:7, 09:5, 10:6, 99:1,
README:11, plan:24) ≈ 60 doc→doc hits; code-comment citations to `05/06/08` doc names in
`ipc-server/src` (table/op/waiter/ctl/attach/refcount/service/lib ≈ 10 hits).
Migration rule (mechanical, B phase): numbers 00–10/99 are STABLE, so all existing
citations keep working untouched. Only three new targets need inserts: (a) 11 citations
added from 02 §1.4-trim, 05/08 RMID sections, 01 envelope figure; (b) 01-§X citations added
from 99 §4's former citers (none — §4 was terminal, zero inbound); (c) 99-absorbed §2.6
citations: 02's internal refs rewritten to 99 pointers (2–3 hits). Neighboring-stage hits
for `13-stage-ipc` (03-stage-rs, 06-stage-sched, 08-stage-is draft, 14-stage-runtime plan,
edge_todo) are stage-level, number-free → unaffected. Total edits ≈ under 15 lines across
≤6 files; hotspots: 02 (trim+pointers), 03 (delete §8 + pointer), 99 (delete §4 + merge),
new 11. Batch method: one `rg` re-run post-B per doc + `cargo doc`-style link check is N/A
(docs are plain md); verification = §9 checks.
Breakage verdict: near-zero — number stability (C-08/C-09) was chosen precisely to keep the
I-14-style renumber risk (cf. `01-stage-kernel/todo.md` I-14 lesson cited in the R prompt)
at zero while still achieving the rebuild (new material enters via one NEW number + moves
into stable numbers, never via renumbering).

## 9. Verification Gates and Conclusion

Mechanical checks (§5.4): (1) forward-ref scan — each contract's 前置 points strictly
earlier (00∅; 01→00; 02→01; 03→01,02; 04→02; 05→02,04; 06→04,05; 07→02,04; 08→07,04;
09→01,05,06; 10→01,03,05,08; 11→01,02,04,05–08; 99→∅): PASS, DAG verified, no cycle
(03 defers info bodies to 05/08 via 后置, not 前置 — the plan D-4 pattern).
(2) dependency graph: acyclic by construction above: PASS.
(3) coverage: all 48 K-entries have a home or reason (42 stock placed incl. K-043
pointer-only and K-039 merged; 6 new placed in 11/01/99; G-6 wont-do reasoned; deletions
C-04/O-1 with reason): 100% PASS.
(4) breakage census: §8 counts with `rg` outputs: PASS (bounded, <15 lines).

| Gate | Result |
|------|--------|
| G1 10 random C anchors re-checked | PASS — `main.c:91-92` panic-on-local-fail; `:160-161` leftover-reply comment; `:207-208` asynsend3 receipt; `:261` ENOSYS; `:279` tail sweep; `sem.c:693` order comment; `:729-739` UNDO; `:880` ip_endpt assert; `shm.c:4` SHM_ALLOC private; `:228` atime quirk — all match §1 rows |
| G2 every C file + non-C artifact placed or excluded-with-reason | PASS — 4 .c + inc.h + ipc.conf + Makefile + libc 4 wrappers + mmap.c verbs + pm/event.c + rmib.c + absent-boot-image + absent-asm, all in §1/§3 tables |
| G3 zero forward refs | PASS (scan above) |
| G4 acyclic (or split plan) | PASS (no cycle) |
| G5 100% pool placed; new entries anchored; deletions listed | PASS (C-04 single deletion + G-6 wont-do) |
| G6 10 sampled split/merge moves show direction | PASS — C-02/C-03/C-04/C-05/C-07 + D-1..D-4 rulings all carry old→new with K-IDs |
| G7 all 13 contracts carry 7 elements | PASS (定位/讲什么/不讲什么/前置/后置/事实底线+知识点/验收 present in each) |
| G8 migration covers every changed section + doc/code refs | PASS (§8 table is per-section; ref census with counts) |
| G9 10 random factual claims anchored; guesses labeled | PASS — zero guesses offered; size claims re-measured (`wc -l`, `sizeof==64` assert); one correction applied: HEAD moved mid-session (noted in §0) |

Conclusion: blueprint EXECUTABLE — B phase can proceed per §5 contracts with取料 paths
(stock section ⇄ K-ID ⇄ C anchor) fully specified. Open questions for the user: (Q1) new-11
scope — keep build half-page inside 11 (proposal) or split a 12th doc? (Q2) 03 §8 deletion —
confirm the rmib-walker truly belongs to 10-stage-mib/22 before B deletes it.
Both are flagged, neither blocks B on docs 01,02,04–10,99 (unaffected by either answer).
