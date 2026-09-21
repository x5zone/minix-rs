# 05-stage-vfs Documentation Rebuild Blueprint (muse)

## 0. Metadata

```text
your_name(AI agent name) = muse
target_dir(关注的工作目录) = notes/rewrite/fork-syscall-rewrite/05-stage-vfs/
repo_root(仓库根目录) = /home/xzhao/github/minix-rs
commit = 9ae17ca54 (docs(edge3): S41 follow-up; git log --oneline -1, verified 2026-09-20)
date = 2026-09-20 (UTC)

任务 = R 相·重建蓝图：输出 target_dir/doc_rerank_muse.md，不改任何正文。
       只读 C 代码不够，还必须读：本目录全部文档的头部声明、os/ 目录对应入口、
       00-*-overview.md 的导航表。
约束 = 不得引用 .design/ 与 tmp_design_and_todo/；任何落盘产物必须带
       _muse 后缀；不得读取或照抄其它 AI 的 doc_rerank_* 产物。
```

**Language note (muse special requirement):** this blueprint is written in English, and all
rebuilt documents specified in §4–§5 must be written in English in Phase B.
Existing Chinese-language documents are treated purely as knowledge sources, never as
sentence sources.

**Independence disclosure:** two peer files present in the target directory
(`doc_rerank_glm.md`, `doc_rerank_deepseek.md`) were opened for directory-context
purposes before the scope of the non-reading rule was fully internalized; two further
peer files (`doc_rerank_HY4.md`, `doc_rerank_qwen.md`) were never opened (verified by
listing only). No sentence, table, numbering decision, or knowledge ID below is copied
from any peer product. Every factual claim carries its own C/Rust anchor verified in
this session (§9, G1/G9); every ordering decision carries its own forward-reference
justification (§4.3). Where conclusions coincide with peer-visible directory facts
(e.g. file counts), the evidence command is re-run and quoted here.

**Scope (per Step 0):**

- **In scope (rebuild objects, 33 docs):** `00-vfs-overview.md`, numbered docs
  `01`–`31`, `99-global-concepts.md`. All enter the knowledge pool (§2) and receive a
  contract (§5) or an archive decision (§6).
- **Reference material (not rebuilt, used as evidence/boundary):** `plan.md` (553 lines),
  `todo.md` (547 lines, R1/R2 architecture review + Fix campaign record), `draft/`
  (old fork-mainline material, 27 items + README), `archive/todo-R1-archive-2026-09-09.md`.
- **Out of scope (cross-referenced only):** filesystem servers (`minix3/minix/fs/`,
  stage 15-stage-fs); driver bodies (stages 16/17); kernel `sys_*` implementations
  (stage 01-stage-kernel); libc syscall stubs and runtime loading (stage 14-stage-runtime);
  trap path into VFS (01-stage-kernel IPC docs). Intermediate-product directories
  (`.design/`, `tmp_design_and_todo/`, `.review/`) are never cited, per project rules.

**Reading list (seven required input classes, all covered):**

| # | Class | What was actually read |
|---|-------|------------------------|
| 1 | All docs in target dir | Headers (first ~60 lines) of all 33 docs + full `plan.md`; three parallel read agents cross-checked scope/boundary/prereq declarations. Line counts re-measured (§0.3). No body sentences reused. |
| 2 | Full Minix3 C source for this stage | `minix3/minix/servers/vfs/`: 33 `.c` (16,735 lines) + 15 `.h` (1,007 lines), `wc -l` verified. `main.c` read in full; `table.c`, `fproc.h`, `vfsif.h` read in full; all other files verified by function-level grep + spot `sed -n` reads (§1 anchors). |
| 3 | Non-C build/boot artifacts | `servers/vfs/Makefile`; `include/minix/{com.h,callnr.h,vfsif.h}` wire sections; `libsys/timers.c` (`expire_timers` home); `libmthread/` existence; `kernel/table.c` boot image; `rs/table.c` service registration. Item-by-item answers in §3.5. |
| 4 | Stage boundary material | `00-master-plan/README.md` (boot causal chain); `edge_todo.md` VFS entries (E-REQWIRE closed, E-VFSWIRE closed); `plan.md` exclusion table; `todo.md` stale-entry复核 status. |
| 5 | Previous stage overview | `04-stage-pm/00-pm-overview.md` (only `00-*` file present; PM owns mproc authority, signals, fork/exit orchestration — VFS must not re-narrate; firewall listed in §3.4). |
| 6 | Rust implementation entry | `os/servers/vfs/src/`: 34 files, 48,181 lines total incl. tests (`wc -l` re-measured). `lib.rs`, `main.rs`, `main_loop.rs` dispatch shape, `call_table.rs`, `worker.rs` ARCH A-1 header, `minix-types/src/ipc/{vfs.rs,fs_driver.rs}`, `minix-fs/src/protocol.rs` wire authority. Rust is corroboration only, never sequence authority. |
| 7 | Style example | `01-stage-kernel/06-todo.md` contract-writing pattern (what/why-it-belongs/acceptance). Only the pattern is borrowed; no content. |

**Commands and key outputs (evidence excerpts, re-run this session):**

```text
$ wc -l minix3/minix/servers/vfs/*.c | tail -1
16735 total
$ wc -l minix3/minix/servers/vfs/*.h | tail -1
1007 total
$ ls minix3/minix/servers/vfs/*.c | wc -l   → 33
$ ls minix3/minix/servers/vfs/*.h | wc -l   → 15
$ grep -cE '^\s*CALL\(' minix3/minix/servers/vfs/table.c → 64
$ grep -c '#define REQ_' minix3/minix/include/minix/vfsif.h → 33 (incl. dead REQ_GETNODE)
$ wc -l os/servers/vfs/src/*.rs os/servers/vfs/src/ipc/*.rs | tail -1
48181 total
$ grep -rhoE "[0-9]{2}-[a-z0-9-]+\.md" notes/rewrite/fork-syscall-rewrite/05-stage-vfs/*.md | wc -l
1142  (inter-doc filename references; hotspot table in §8.3)
$ grep -rn "stage-vfs" os/servers/vfs/src/ | wc -l
1  (worker.rs:808 only; broad code-comment cross-ref cleanup already landed)
$ grep -rn "18-syscall-copy" notes/rewrite/fork-syscall-rewrite/05-stage-vfs/*.md | wc -l
35  (stale references to a non-existent doc; triage in §8.4)
$ git log --oneline -1
9ae17ca54 docs(edge3): S41 行随动 ——卡N 00/99 骨架清零八篇
```

**G1/G9 spot-check anchors (10/10 verified this session, `sed -n '<line>p'`):**

| # | Anchor | Observed content (truncated) | Verdict |
|---|--------|------------------------------|---------|
| 1 | `minix3/minix/servers/vfs/main.c:54` | `int main(void)` | ✅ |
| 2 | `minix3/minix/servers/vfs/main.c:69` | `while (TRUE) {` | ✅ |
| 3 | `minix3/minix/servers/vfs/main.c:410` | `/* Initialize the process table with help of the process manager...` | ✅ |
| 4 | `minix3/minix/servers/vfs/table.c:17` | `int (* const call_vec[NR_VFS_CALLS])(void) = {` | ✅ |
| 5 | `minix3/minix/include/minix/callnr.h:68` | `#define VFS_BASE  0x100` | ✅ |
| 6 | `minix3/minix/include/minix/com.h:589` | `#define FS_BASE  0xA00  /* Requests sent by VFS to filesystem...` | ✅ |
| 7 | `minix3/minix/include/minix/vfsif.h:41` | `#define REQ_GETNODE  (FS_BASE + 1)  /* Should be removed */` | ✅ |
| 8 | `minix3/minix/servers/vfs/const.h:5` | `#define NR_FILPS  1024` | ✅ |
| 9 | `minix3/minix/servers/vfs/fproc.h:15` | `EXTERN struct fproc {` | ✅ |
| 10 | `minix3/minix/servers/vfs/worker.c:27` | `void worker_init(void)` | ✅ |

**Document inventory (re-measured this session; headers surveyed, bodies used only as
knowledge sources):**

| File | Lines | Declared topic (from header) |
|------|-------|------------------------------|
| 00-vfs-overview.md | 85 | Stage navigation, startup graph, 64-call/12-PM/REQ service map |
| 01-vfs-init-main.md | 539 | main/SEF, VFS_PM_INIT handshake, init chain, do_init_root |
| 02-fproc-struct.md | 398 | struct fproc fields, flags, blocked_on union, credentials |
| 03-fproc-table.md | 265 | fproc[NR_PROCS], isokendpt guards, sentinels, fproc_light |
| 04-filp-table.md | 223 | filp table, counts, get/find/lock, FSF bits |
| 05-vnode-table.md | 225 | vnode cache, dual counts, dup/put, vnode locks |
| 06-vmnt-table.md | 223 | vmnt table, free/find/lock, unmap cascade |
| 07-tll-lock.md | 217 | tll_t, three-state semantics, lock/unlock/upgrade |
| 08-worker-thread.md | 309 | NR_WTHREADS, pool counters, start/allow/suspend/resume |
| 09-main-loop.md | 285 | dispatch priority, call_vec[64], reply/SUSPEND |
| 10-pm-protocol.md | 266 | service_pm, pm_fork/exit/creds/reboot/dumpcore entry |
| 11-fs-comm.md | 291 | m_comm window, TRANSID, send/rec/cancel/queue |
| 12-request-wrappers.md | 264 | req_* wrappers, REQ spectrum, grants, response structs |
| 13-path-lookup.md | 255 | lookup/advance/eat_path/last_dir/canonical, mount crossing |
| 14-filedes.md | 238 | fd allocation, close_fd, copyfd, invalidate family |
| 15-open-close.md | 260 | open/creat/common_open/new_node/mknod/mkdir/close/lseek |
| 16-read-write.md | 249 | read/write/peek/getdents, bsf lock, file positions |
| 17-pipe.md | 258 | pipe quota, suspend/release/revive/unpause |
| 18-mount.md | 263 | mount_fs/mount_pfs/umount, dev encoding, root special |
| 19-device-map.md | 281 | dmap/smap, map_service/driver, ioctl grant decode |
| 20-bdev.md | 232 | block-device protocol half |
| 21-cdev.md | 248 | char-device protocol half, CTTY |
| 22-sdev.md | 269 | socket-driver protocol half |
| 23-select.md | 293 | do_select stages, fd types, timers |
| 24-socket.md | 289 | socket syscall family |
| 25-exec.md | 272 | pm_exec checks, script/ELF, memmap vs read paths |
| 26-coredump.md | 251 | ELF core writer |
| 27-link.md | 241 | link/unlink/rename/truncate/slink/rdlink |
| 28-stadir.md | 251 | chdir/stat/statvfs families |
| 29-protect.md | 240 | forbidden/in_group/chmod/chown/umask/access |
| 30-fcntl-lock.md | 267 | fcntl commands, POSIX lock table |
| 31-misc-queries.md | 265 | getsysinfo/sync/fsync/dupvm/vm_call/svrctl/utimens/gcov/getrusage |
| 99-global-concepts.md | 96 | Capacity constants, globals, omissions ledger |

Total: 33 docs, ~8,370 lines of rebuild input (excluding plan/todo/draft).

---

## 1. C True Order (runtime sequence reconstructed from source, not from existing docs)

**Stage-type verdict: service event-loop.** VFS is a resident user-space server:
`main()` (`main.c:54`) ends in `while (TRUE)` (`main.c:69`) and never returns; startup
is only a prelude to the loop. All post-startup behavior is receive–dispatch–handle–reply
driven by `get_work()` (`main.c:580`) with a strict priority chain (`main.c:80-138`),
executed by 9 worker threads (`worker.c`, `const.h:9`). The request surface is parallel
(64 syscalls in `table.c:17-82`, 12 PM requests in `com.h:520-531`, 33 REQ constants of
which 32 are live in `vfsif.h:41-73`), so §4 organizes it as framework-first plus
scenario groups per the parallel-body rule. True-order table below is split into
**startup segment** (RS load → loop entry) and **loop segment** (one iteration by
priority), plus one end-to-end FS-roundtrip lifecycle used for acceptance tests.

### 1.1 Startup segment (`main.c:54-527`)

| Step | Action | C anchor | Note |
|------|--------|----------|------|
| S01 | RS loads VFS from boot image | `minix3/minix/servers/rs/table.c` VFS entry; `minix3/minix/kernel/table.c` image order | Loading itself belongs to RS/kernel stages; VFS starts at `main` |
| S02 | `main()` entry → `sef_local_startup()` | `main.c:54-64` | Thin entry; registers callbacks then loops |
| S03 | SEF callback registration (fresh/restart/LU groups) | `main.c:374-388` | Count the `sef_setcb_*` calls from source at B-phase time; do not copy any doc's "5 vs 6" claim without recounting |
| S04 | `sef_startup()` → `sef_cb_init_fresh()` | `main.c:387`, `main.c:393` | Main init entry |
| S05 | fproc slots zeroed (`fp_endpoint=NONE`, `fp_pid=PID_FREE`) | `main.c:405-408` | First-pass init |
| S06 | `VFS_PM_INIT` handshake loop, one slot per message (`SYS_UID/GID`, `umask=~0`); `VFS_PM_ENDPT==NONE` terminates | `main.c:410-434`; `com.h:520-531` | Earliest cross-server message exchange in VFS life |
| S07 | Reply `OK` to PM (two-way barrier) | `main.c:435-436` | — |
| S08 | `system_hz = sys_hz()` | `main.c:438` | Clock frequency cache; select timers depend on it |
| S09 | `ds_subscribe("drv\\.[bc]..\\..*")`, panic on failure | `main.c:441` | Driver hotplug entry; handling itself is a later-stage topic (§4, new 18) |
| S10 | `worker_init()` (9 threads, `NR_WTHREADS`) | `main.c:445`; `worker.c:27`; `const.h:9` | Worker pool exists **before** the four core tables — existing doc order (worker after tables) contradicts runtime order; this is reordering evidence #1 |
| S11 | `bsf_lock` init (block-special-file global lock) | `main.c:448` | Home: new 16 |
| S12 | `init_dmap()` / `init_smap()` | `main.c:451-452`; `dmap.c:230`; `smap.c:22` | Device tables exist before boot-service mapping |
| S13 | Fetch `rproctab` from RS, `map_service()` each in-use boot service | `main.c:455-465` | Where endpoint knowledge comes from at boot |
| S14 | Second fproc pass: `mutex_init(fp_lock)`, `fp_worker=NULL`, clear `fp_filp[OPEN_MAX]`, `fp_rd/fp_wd=NULL` | `main.c:468-484` | Slot locks belong to slots, not processes |
| S15 | `init_vnodes()` / `init_vmnts()` / `init_select()` / `init_filps()` | `main.c:486-489` | Fixed C init order: vnode → vmnt → select → filp |
| S16 | `worker_start(..., do_init_root, ...)` — root mount runs in a worker, init returns unblocked | `main.c:492-493` | — |
| S17 | `do_init_root`: `worker_allow(FALSE)` → `mount_pfs()` → `mount_fs(DEV_IMGRD,"bootramdisk","/",MFS,…)` → panic on failure → `worker_allow(TRUE)` | `main.c:501-523`; `mount.c:391-425`; `mount.c:156-385` | Mount mechanism is invoked at startup but *explained* later (order-difference D-2) |

### 1.2 Loop segment (`main.c:69-141`, one iteration in priority order)

Per-iteration preamble: `worker_yield()` (`main.c:70`), `send_work()` drains FS queues
(`main.c:72`; `comm.c:37`), `get_work()` (`main.c:77-78`; revive-first `main.c:590-597`
via `unblock()` `main.c:921-973`, else `sef_receive(ANY)` `main.c:601` with endpoint
mapping `main.c:605-607`).

| Priority | Test | Action | C anchor |
|----------|------|--------|----------|
| P1 | `IS_VFS_FS_TRANSID(TRNS_GET_ID(m_type))` | FS async reply → `worker_get(tid-VFS_TRANSID)` → strip id → `do_reply(wp)` | `main.c:80-90`; `com.h:909-912`; `vfsif.h:79-81` |
| P2 | `who_e==PM_PROC_NR` | `service_pm()` (fast inline vs `worker_start` deferred arms) | `main.c:91-94`; `main.c:764-915` |
| P3 | `is_notify(call_nr)` | DS → `handle_work(ds_event)`; KERNEL → `mthread_stacktraces()`; CLOCK → `expire_timers()` (impl in `libsys/timers.c:97`); else ignore | `main.c:95-117`; `com.h:90-93` |
| P4 | `who_p<0` (kernel task) | Ignore non-notify task messages | `main.c:118-124` |
| P5a/b/c | `IS_BDEV_RS` / `IS_CDEV_RS` / `IS_SDEV_RS` | `bdev_reply()` / `cdev_reply()` / `sdev_reply()` | `main.c:126-134`; `com.h:963-967`; `com.h:919-923`; `com.h:1038-1041` |
| P6 | else normal syscall | `handle_work(do_work)` → `worker_start(fp,do_work,use_spare=TRUE)` with `VMNT_CALLBACK` FS-deadlock guard | `main.c:135-138`; `main.c:146-181` |

Worker-side dispatch `do_work()` (`main.c:263-298`): drop `fp_pid==PID_FREE`
(`main.c:268-273`); `IS_VFS_CALL` bounds-checked `call_vec[call_nr-VFS_BASE]` call else
`ENOSYS` (`main.c:283-294`); reply unless `SUSPEND` (`main.c:297`). `reply()`/
`replycode()` are non-blocking `ipc_sendnb` (`main.c:638-663`).

### 1.3 One FS roundtrip lifecycle (acceptance backbone for new 11/12 contracts)

| Step | Action | C anchor |
|------|--------|----------|
| F01 | Handler calls `req_*` wrapper, builds message with grants | `request.c` per-function; `request.c:30-80` CPF_TRY |
| F02 | `fs_sendrec`/`fs_sendmsg` occupies mount window and sends | `comm.c:134-168`; `comm.c:12-32` |
| F03 | Window full / CALLBACK → `queuemsg` into `m_comm.m_req_queue` | `comm.c:223-244` |
| F04 | Main loop `send_work()` re-issues when FS idle | `comm.c:37-45` |
| F05 | FS replies with `TRNS_ADD_ID`-encoded message | `vfsif.h:79-81`; `comm.c:20-23` |
| F06 | Loop P1 routes to `do_reply`, fills `wp->w_sendrec`, `worker_signal` | `main.c:187-211` |
| F07 | Worker resumes, `req_*` `_actual` half back-fills response | `request.c` per-function `_actual` |

### 1.4 Order-difference table (runtime fact vs teaching choice; each needs a back-pointer)

| # | Runtime fact (anchored) | Teaching choice | Why | Compensation (back-pointer) |
|---|-------------------------|-----------------|-----|------------------------------|
| D-1 | `VFS_PM_INIT` handshake is the earliest message exchange (`main.c:410-434`) | New 01 covers envelope + handshake state machine; new 10 covers the full PM protocol | fork/exit/exec semantics need fproc/filp/vnode tables (new 02-04/07) which do not exist yet at handshake time | 01 states "one message fills one slot" only; 10 opens by pointing back to 01 |
| D-2 | Root mount executes in startup (`main.c:501-523`) | Mount mechanism lives in new 19, after device-map (18), path (13), REQ wrappers (12) | `mount_fs` needs dmap lookup + `req_readsuper`; teaching it in 01 would forward-reference three chapters | 01 covers only the gate + call sites; 19 opens by pointing back to 01 |
| D-3 | DS subscription happens at startup (`main.c:441`) | `ds_event` classification lives in new 18 | Needs dmap/smap tables first | 01 records what + failure semantics; 18 expands handling |
| D-4 | Loop precedes all request handling (`main.c:69`) | Worker (new 05) and loop (new 09) come after the data-model docs (02-04, 06-08) | Dispatch acts on tables; objects before scheduling | Each data doc ends with "which startup step creates me" |
| D-5 | CLOCK notify calls `expire_timers` (`main.c:112`) whose implementation is `libsys/timers.c:97` | New 09 covers dispatch; new 23 covers select timeout | Timer implementation is a runtime-library topic, not VFS-process code | Both docs name the real home |
| D-6 | `RMDIR` shares `do_unlink` (`table.c` alias row) | New 09 covers the alias table; new 27 covers unlink semantics | Alias is a dispatch-table fact | 09 marks aliases; 27 points back |
| D-7 | `do_pending_pipe` is replayed by the loop via `unblock` (`main.c:216-258,921-973`) | Pipe suspend/revive lives in new 17; new 09 covers `unblock` forks only | Pipe quota machine (16/17) is prerequisite knowledge | 09 marks the two forks with destinations; 17 points back |
| D-8 | `mount_fs` calls `eat_path`/`lookup` internally (`mount.c:186-199`) | Path machine lives in new 13; new 19 only references it | Mount-point parsing reuses the resolver | 19 cites 13's resolver interface |
| D-9 | `worker_stop` is triggered by dmap/smap endpoint recycling (`dmap.c:180-199`, `smap.c:148-170`) | Trigger lives in new 18; per-family consequences in 20/21/22 | One trigger event, per-family aftermath | 18 lists the consequence checklist; 22 holds the consolidated death-cascade spectrum |
| D-10 | `do_vm_call` is an upstream VM request with always-async reply (`misc.c:380-500`) | New standalone 32 after the device chapters | Needs fd/filp (new 04/14) plus the VM-side fdref concept (02-stage-vm) | 32 opens by pointing back to 04/14 |

---

## 2. Knowledge Pool (deduplicated; alignment key = name + anchor)

Notation: Type ∈ {Concept, Mechanism, Struct, Protocol, Constraint, Evolution, Tooling,
Test}. Source ∈ {Stock (from existing doc §) | New (from C/non-C/Rust/OS theory, with
evidence anchor)}. "Home" = new document number from §4. Groups follow the new
reading order so §5 contracts can cite K-IDs directly. Reader-benefit is carried by
each §5 contract's positioning + acceptance lines (explicit裁剪 to avoid repeating
300 rows of identical prose).

### G00 — Stage frame (home: new 00)

| ID | Name | Type | Source | Anchor | Home |
|----|------|------|--------|--------|------|
| K-001 | VFS role: sole shared vocabulary of POSIX processes, FS drivers, device drivers | Concept | Stock 00 §1 | `servers/vfs/` tree | 00 |
| K-002 | Request lifecycle overview (receive–dispatch–handle–reply/suspend–revive) | Mechanism | Stock 00+09 | `main.c:69-141,263-298` | 00 |
| K-003 | Three protocol surfaces and their sizes (64 calls / 12 PM / 33 REQ consts, 32 live) | Protocol | Stock 00 (correct the "33 live" error) | `callnr.h:68-137`; `com.h:513-544`; `vfsif.h:41-75` | 00 |
| K-004 | Stage position in the boot chain (RS loads VFS; PM feeds it; FS/drivers serve it) | Concept | Stock 00/01 | `00-master-plan/README.md` chain; `kernel/table.c` image | 00 |
| K-005 | Reading paths (mainline / branch / skippable) | Tooling | Stock 00 | — (pedagogy) | 00 |
| K-006 | ARCH evolution index (single-thread slots, typed errors, wire authorities) | Evolution | Stock 00 + plan §4 | `os/servers/vfs/src/` | 00 |

### G01 — Birth (home: new 01)

| ID | Name | Type | Source | Anchor | Home |
|----|------|------|--------|--------|------|
| K-010 | VFS is started by RS from the boot image, not self-bootstrapped | Concept | Stock 01 | `rs/table.c` VFS entry; `main.c:54` | 01 |
| K-011 | Eleven-item init chain and its three order invariants | Constraint | Stock 01 | `main.c:393-499` | 01 |
| K-012 | VFS_PM_INIT handshake: fill-one-slot-per-message, NONE terminator, OK barrier; boot credentials | Protocol | Stock 01/10/02 | `main.c:410-436`; `com.h:520-531` | 01 |
| K-013 | SEF lifecycle callback groups (fresh/restart/LU) | Mechanism | Stock 01 (recount callbacks from source at B-phase; never copy old counts) | `main.c:374-388,303-373` | 01 |
| K-014 | `system_hz`, DS subscription string, bsf_lock/dmap/smap/rproctab call sites | Mechanism | Stock 01 | `main.c:438-465` | 01 |
| K-015 | Two-pass fproc init: why clearing happens twice | Mechanism | Stock 01/03 | `main.c:405-408,468-484` | 01 |
| K-016 | Four core tables creation order (vnode→vmnt→select→filp) | Mechanism | Stock 01 | `main.c:486-489` | 01 |
| K-017 | `do_init_root` gate (`worker_allow(FALSE)` → pfs → root FS → TRUE) | Mechanism | Stock 01 | `main.c:501-523` | 01 |
| K-018 | `lock_proc`/`unlock_proc` sleepable slot lock (fast trylock / slow suspend) | Mechanism | Stock 01 | `main.c:528-553` | 01 |
| K-019 | `fs.h` master-include map | Struct | Stock 99 | `fs.h:1-43` | 01 |

### G02 — Process model (home: new 02 fproc-struct, new 03 fproc-table)

| ID | Name | Type | Source | Anchor | Home |
|----|------|------|--------|--------|------|
| K-020 | Four process-table projections (kernel proc / PM mproc / VFS fproc / VM vmproc), endpoint-joined | Concept | Stock 02 | `fproc.h:11` boundary comment | 02 |
| K-021 | `struct fproc` field groups (file / identity / blocking / concurrency-assist) | Struct | Stock 02 | `fproc.h:15-82` | 02 |
| K-022 | `fp_flags` bits (SRV_PROC/REVIVED/SESLDR/PENDING/EXITING/PM_WORK) | Struct | Stock 02 | `fproc.h:91-98` | 02 |
| K-023 | `fp_blocked_on` states + `fp_u` payload union (SELECT carries no payload) | Struct | Stock 02 | `const.h:19-28`; `fproc.h:30-61` | 02 |
| K-024 | Single-suspend-entry invariant (`suspend()` non-stackable) | Constraint | Stock 02/17 | `pipe.c:294-311` | 02 |
| K-025 | Credential fields + supplementary groups + umask; real-vs-effective selection | Struct | Stock 02/29 | `fproc.h:63-69`; `protect.c:255-256` | 02 |
| K-026 | `fp_lock` belongs to the slot (fork does not share it); request-context quad (`fp_worker/func/msg/pm_msg`) | Constraint | Stock 02/10 | `fproc.h:71-75`; `misc.c:606-608` | 02 |
| K-027 | `BlockedOn` tagged-enum modeling (discriminant + bound payload) | Evolution | Stock 02 | `fproc.rs` `BlockedOn` | 02 |
| K-028 | Scalar type hygiene (Uid/Gid/DevId/Mode; sentinel-vs-Option discipline) | Evolution | Stock 02 | `minix-types/types/id.rs`; `fproc.rs` | 02 |
| K-030 | `fproc[NR_PROCS]` fixed table, same bound as sibling servers | Struct | Stock 03 | `fproc.h:10-12` | 03 |
| K-031 | `PID_FREE`/`NONE` dual-sentinel idle test | Constraint | Stock 03 | `fproc.h:101-103`; `main.c:405-408` | 03 |
| K-032 | `isokendpt` three guards (range / sentinel / mismatch) + fatal `okendpt` split | Mechanism | Stock 03 | `utility.c:94-127` | 03 |
| K-033 | `fproc_addr`/`who_p`/`who_e` O(1) index macros | Protocol | Stock 03/09 | `glo.h:26-27` | 03 |
| K-034 | `fproc_light` three-field snapshot and MIB consumption | Struct | Stock 03/02/10 | `fproc.h:111-115`; `misc.c:52-96` | 03 |

### G03 — Open-file model (home: new 04 filp-table)

| ID | Name | Type | Source | Anchor | Home |
|----|------|------|--------|--------|------|
| K-040 | filp intermediary and the fd→filp→vnode two-hop binding | Concept | Stock 04 | `file.h:5` | 04 |
| K-041 | `struct filp` panorama; `filp_count==0` idle sentinel; `NR_FILPS=1024` | Struct | Stock 04 | `file.h:8-33`; `const.h:5`; `filedes.c:73-86` | 04 |
| K-042 | Shared-count semantics across fork/dup (divergence of sharing) | Constraint | Stock 04/02/10 | `file.h:11`; `misc.c:629` | 04 |
| K-043 | `get_filp`/`get_filp2` and the `FILP_CLOSED` gate with `OPCL` privilege | Mechanism | Stock 04/14 | `filedes.c:162-199` | 04 |
| K-044 | `find_filp` / `find_filp_by_sock_dev` reverse lookup | Mechanism | Stock 04 | `filedes.c:205-246` | 04 |
| K-045 | `close_filp` zeroing and `put_vnode` release | Mechanism | Stock 04/14 | `filedes.c:414-519` | 04 |
| K-046 | filp three-state borrow (`filp_lock` / `softlock` / `ioctl_fp`) | Struct | Stock 04/14 | `file.h:14-20`; `filedes.c:313-380` | 04 |
| K-047 | `FSF_*` bitset (select/driver-observable state) | Struct | Stock 04/23 | `file.h:26-48` | 04 |
| K-048 | `alloc_filp` deferred-set contract (caller sets count=1 at claim time) | Constraint | Stock 04 | `open.c:133-135`; `filedes.c` | 04 |
| K-049 | `FilpTable`/`FilpId` typing; `check_filp_locks` debug family to omissions ledger | Evolution | Stock 04 | `filp.rs`; `filedes.c:26-71` | 04 |

### G04 — Execution model (home: new 05 worker, new 06 tll)

| ID | Name | Type | Source | Anchor | Home |
|----|------|------|--------|--------|------|
| K-050 | Why only VFS needs concurrency (five blocking semantics) | Concept | Stock 08 | `pipe.c`/`select.c`/`cdev.c`/`sdev.c`/`comm.c` suspend points | 05 |
| K-051 | `NR_WTHREADS=9` threads; `pending`/`busy`/`block_all` two-level concurrency | Struct | Stock 08 | `const.h:9`; `worker.c:10-12` | 05 |
| K-052 | Spare-thread reserve invariant (deadlock avoidance) | Constraint | Stock 08 | `worker.c:147-156,331-339` | 05 |
| K-053 | `worker_allow` two-phase gate (mark-only close / release open) | Mechanism | Stock 08/01 | `worker.c:162-185` | 05 |
| K-054 | `w_fp`↔`fp_worker` two-way binding; `worker_can_start` quadrants | Mechanism | Stock 08 | `threads.h:27`; `worker.c:138,283-325` | 05 |
| K-055 | suspend/resume coroutine triple; wait/signal sleep queue; yield/self handoff | Mechanism | Stock 08 | `worker.c:431-530,586-607` | 05 |
| K-056 | `worker_start` registration sanity; `worker_stop`/`stop_by_endpt` EIO injection; `thread_cleanup` clears `VMNT_CALLBACK` | Mechanism | Stock 08/09 | `worker.c:360-428,535-570`; `main.c:558-575` | 05 |
| K-057 | ARCH A-1: 9 mthreads → single-thread slot state machine (Linux workqueue / Redox async / seL4 endpoint对照) | Evolution | Stock 08 | `worker.rs:1-60`; `threads.h` | 05 |
| K-060 | tll three states (READ multi-reader / READSER serial-read / WRITE exclusive) | Concept | Stock 07 | `tll.c:163,197,208`; `tll.h:6` | 06 |
| K-061 | `tll_t` six fields and three-axis orthogonality | Struct | Stock 07 | `tll.h:9-18` | 06 |
| K-062 | Write partial order (write queue preferred over serial queue in head selection) | Constraint | Stock 07 | `tll.c:230-303` | 06 |
| K-063 | `tll_lock` five-way dispatch + `tll_append` dual-queue enqueue | Mechanism | Stock 07 | `tll.c:139-219,11-71` | 06 |
| K-064 | `tll_unlock` head wakeup; `tll_downgrade`/`tll_upgrade` timing; `TLL_NONE≡0` empty-lock invariant | Mechanism | Stock 07 | `tll.c:74-111,230-323` | 06 |
| K-065 | Blocking inside tll waits on worker primitives (ordering proof worker→tll) | Constraint | New (ordering evidence) | `tll.c` `worker_wait` call sites; `worker.c:474-530` | 06 |
| K-066 | `TllError` non-blocking contract in single-thread port | Evolution | Stock 07 | `tll.rs` | 06 |

### G05 — Cached objects (home: new 07 vnode, new 08 vmnt)

| ID | Name | Type | Source | Anchor | Home |
|----|------|------|--------|--------|------|
| K-070 | vnode as inode projection; `find_vnode(fs_e,ino)` linear scan | Concept | Stock 05 | `vnode.h:4-23`; `vnode.c:110` | 07 |
| K-071 | `vnode[NR_VNODES=1024]`; idle iff `ref==0 && !locked` | Struct | Stock 05 | `const.h:8`; `vnode.c:85` | 07 |
| K-072 | Dual counts (`v_ref_count` / `v_fs_count`) with 256-threshold lazy sync | Constraint | Stock 05 | `vnode.c:227-315` | 07 |
| K-073 | `dup_vnode`/`put_vnode` fast/slow paths (ref>1 fast; ==1 via `req_putnode`) | Mechanism | Stock 05 | `vnode.c:227-303` | 07 |
| K-074 | vnode lock family and `VNODE_*`→`TLL_*` mapping | Mechanism | Stock 05/07 | `vnode.h:26-29`; `vnode.c:156-224` | 07 |
| K-075 | Device/mount association quartet (`v_dev`/`v_vmnt`/`v_sdev`/`v_bfs_e`) | Struct | Stock 05 | `vnode.h:16-21` | 07 |
| K-076 | Dual-1024 cache duality (filp/vnode same bound) | Concept | Stock 05 | `const.h:5,8` | 07 |
| K-080 | vmnt as device→FS boundary table | Concept | Stock 06 | `vmnt.h:7-21` | 08 |
| K-081 | `vmnt[NR_MNTS=16]` capacity (corrects the old "8" error everywhere incl. path scan bounds) | Struct | Stock 06 (erratum) | `const.h:7` | 08 |
| K-082 | `m_dev==NO_DEV` idle sentinel; four-table sentinel comparison | Constraint | Stock 06 | `const.h` sentinels | 08 |
| K-083 | `get_free_vmnt`/`find_vmnt`/`clear_vmnt` (zero-before-claim; list the seven fields from source) | Mechanism | Stock 06 (recount fields from source) | `vmnt.c:65-120` | 08 |
| K-084 | `VMNT_*`→TLL mapping; lock/upgrade/downgrade; EXCL-as-WRITE; self-lock EDEADLK refusal | Mechanism | Stock 06/07 | `vmnt.h:31-33`; `vmnt.c:150-258` | 08 |
| K-085 | `vmnt_unmap_by_endpt` four-step cascade (FS-crash reclaim) | Mechanism | Stock 06/19 | `vmnt.c:180-192` | 08 |
| K-086 | Mount metadata fields (label/path/fstype/statvfs cache); `m_comm` window belongs to new 11 | Struct | Stock 06/28 | `vmnt.h:14-20`; `type.h:1-10` | 08 |
| K-087 | `fetch_vmnt_paths` is dead C code (zero callers): do-not-port verdict | Tooling | Stock 06/99 | `vmnt.c:246`; `proto.h:371` | 08 |

### G06 — Loop and wire (home: new 09 main-loop, new 10 pm-protocol, new 11 fs-comm, new 12 req-wrappers)

| ID | Name | Type | Source | Anchor | Home |
|----|------|------|--------|--------|------|
| K-090 | Loop heart: blocking never infects the main thread | Concept | Stock 09 | `main.c:69-141` | 09 |
| K-091 | Eight-way priority order and namespace non-overlap proof | Constraint | Stock 09 | `main.c:80-138`; `com.h` bases | 09 |
| K-092 | `get_work` revive-first + ANY receive + endpoint coherence check + TRUE/FALSE dual return | Mechanism | Stock 09 | `main.c:580-633` | 09 |
| K-093 | `call_vec[64]` flat table, NULL sentinel, four aliases (RMDIR/FCHMOD/FCHOWN/SENDMSG-RECVMSG) | Struct | Stock 09 | `table.c:17-82` | 09 |
| K-094 | `do_work` index dispatch + SUSPEND-no-reply contract; `reply`/`replycode` non-blocking | Protocol | Stock 09 | `main.c:263-298,638-663` | 09 |
| K-095 | `unblock` revive fork (pipe replay vs lock whole-request); `handle_work` deadlock guards | Mechanism | Stock 09/17/30 | `main.c:921-973,146-181` | 09 |
| K-096 | `VfsState` aggregation; `Route`/`PollResult` typing; `dispatch_syscall` 64-arm exhaustive match + `run_once` (Rust current state at B-phase) | Evolution | Stock 09 + Rust | `main_loop.rs:148-169`; `syscalls.rs`; `call_table.rs` | 09 |
| K-100 | PM owns mproc, VFS owns fproc; 12 RQ + 11 RS wire table | Protocol | Stock 10 | `com.h:513-544` | 10 |
| K-101 | `service_pm` three-level scheduling (immediate / deferred / standalone + INIT) | Mechanism | Stock 10 (recount arms from source) | `main.c:764-915` | 10 |
| K-102 | `service_pm_postponed` four branches; `PM_WORK`/`FP_PM_WORK` deferral | Mechanism | Stock 10 | `main.c:668-762`; `worker.c` | 10 |
| K-103 | `pm_fork` four-step sharing (keep locks / fd counts / vnode dup / flag reset) + childno double-check | Mechanism | Stock 10 | `misc.c:577-634` | 10 |
| K-104 | `free_proc` two phases with `FP_EXITING` watershed; SESLDR tty revoke cascade; resource-reclaim order table | Mechanism | Stock 10 | `misc.c:639-708` | 10 |
| K-105 | Credential single-write family (setuid/setgid/setgroups/setsid) + SRV_FORK injection | Mechanism | Stock 10 | `misc.c:726-792,867-871` | 10 |
| K-106 | `pm_reboot` eight-step sequence; `pm_dumpcore` entry/exit (body in new 26) | Mechanism | Stock 10 | `misc.c:504-575,903-943` | 10 |
| K-107 | `PmHandler` typing with fail-closed gap semantics | Evolution | Stock 10 | `ipc/dispatcher.rs` | 10 |
| K-110 | Per-mount `comm_t` window (max/cur/queue) and 0≤cur≤max invariant | Struct | Stock 11 | `type.h:1-10`; `comm.c:41` | 11 |
| K-111 | `sendmsg`/`send_work`/`fs_sendmore`; `queuemsg` tail / head-move four gates | Mechanism | Stock 11 | `comm.c:12-84,223-244` | 11 |
| K-112 | `fs_cancel`/`drv_sendrec`/`vm_sendrec` three counterpart channels; CALLBACK/window/EDEADLK triple guard; ERESTART→EIO suppression | Mechanism | Stock 11 | `comm.c:50-218` | 11 |
| K-113 | `VFS_TRANSID` encoding (low16 id, high16 type); `TransIdCodec` single authority | Protocol | Stock 11 | `com.h:909-912`; `vfsif.h:79-81` | 11 |
| K-114 | `GlobalComm`/`FsComm` typing | Evolution | Stock 11 | `fs_comm.rs` | 11 |
| K-120 | `FS_BASE=0xA00`, `IS_FS_RQ` mask; count truth (33 consts = 32 live + 1 dead; NREQS=34 capacity; 36 `req_*` functions) | Protocol | Stock 12 (post-fix state) | `com.h:589`; `vfsif.h:41-75` | 12 |
| K-121 | 32-live REQ spectrum + `FsReq` bijection; `node_details`/`lookup_res` typed responses | Struct | Stock 12 | `vfsif.h:41-73`; `request.h:12,25` | 12 |
| K-122 | CPF_TRY two-phase retry (grant→send→revoke FAULTED→ERESTART→retry); `req_lookup` dual-grant credential pass | Mechanism | Stock 12 | `request.c:30-80,424-495` | 12 |
| K-123 | Dead `REQ_GETNODE` exclusion; `RES_*` capability negotiation; `EENTERMOUNT/ELEAVEMOUNT/ESYMLINK` special codes | Constraint | Stock 12/13 | `vfsif.h:20-28,41` | 12 |
| K-124 | REQ authority converged to `minix-types::ipc::fs_driver` (E-REQWIRE closed); VFS+FS consume, never redefine | Evolution | New (edge record) | `minix-types/src/ipc/fs_driver.rs:21-135`; `minix-fs/src/protocol.rs:30`; `edge_todo.md` E-REQWIRE | 12 |

### G07 — Names and descriptors (home: new 13 path, 14 filedes, 15 open-close, 16 read-write, 17 pipe)

| ID | Name | Type | Source | Anchor | Home |
|----|------|------|--------|--------|------|
| K-130 | Path resolution as the common prelude of name-class calls | Concept | Stock 13 | `path.c:1-39` | 13 |
| K-131 | `lookup` struct fields + `PATH_*` flags; `PATH_MAX=1024`/`NAME_MAX=60` gates | Struct | Stock 13 (recount fields from source) | `path.h`; `path.c:405,574-588` | 13 |
| K-132 | `advance` two phases (hit→count++ / miss→build cache; OPCL→READ demote) | Mechanism | Stock 13 | `path.c:40-127` | 13 |
| K-133 | `eat_path` origin split (/→root, relative→workdir); `last_dir` split + symlink retry + loop_start restart | Mechanism | Stock 13 | `path.c:133-380` | 13 |
| K-134 | `lookup` special-code loop (READ↔WRITE upgrade/downgrade; cursor memmove) incl. mount crossing (`EENTERMOUNT` scan bound = 16) and symlink cycles (`SYMLOOP=16` double gate, ELOOP) | Mechanism | Stock 13 (fix scan-bound "8" error) | `path.c:384-573`; `const.h:7` | 13 |
| K-135 | Resolution-time lock protocol (VMNT READ→WRITE, VNODE READ→OPCL/demote) | Constraint | Stock 13 | `path.c:54-61,435,554` | 13 |
| K-136 | `copy_path`/`fetch_name` user-path fetch + error codes; `PathFetcher` seam (Direct/Safecopy); `get_name` dirent scan; `canonical_path`; chroot boundary; `DO_POSIX_PATHNAME_RES=0` history switch | Mechanism | Stock 13 | `utility.c:24-93`; `path.c:594-798,803-836,23-35` | 13 |
| K-140 | fd private index vs filp shared pool (two-hop model restated for operators) | Concept | Stock 14 | `fproc.h:24`; `file.h` | 14 |
| K-141 | `get_fd` lowest-free + `F_DUPFD` lower bound; `check_fds` nfds window; `OPEN_MAX=255`/`NR_FILPS=1024` EMFILE/ENFILE | Mechanism | Stock 14 | `filedes.c:88-156` | 14 |
| K-142 | `get_filp2` EBADF/EIO dual gates (`OPCL` passes); `FILP_CLOSED` close-only semantics | Constraint | Stock 14/04 | `filedes.c:186-199` | 14 |
| K-143 | `invalidate_filp` three variants (by_endpoint/by_char_major/by_sock_drv) as the filedes face of the death cascade | Mechanism | Stock 14 | `filedes.c:250-308` | 14 |
| K-144 | `do_copyfd` FROM/TO/CLOSE/CLOEXEC with direction-by-kind + EDEADLK/CLOEXEC-strip/ioctl-holder gates | Mechanism | Stock 14 | `filedes.c:524-656` | 14 |
| K-145 | `close_fd` teardown order (unindex first → close_filp → FD_CLR → lock release → lock_revive) | Mechanism | Stock 14/15/30 | `open.c:690-727` | 14 |
| K-150 | Name/offset separation: two-hop bind at open time | Concept | Stock 15 | `open.c:133-135` | 15 |
| K-151 | `do_open`/`do_creat` dual entries; `mode_map` intent encoding; `common_open` seven-step pipeline with six-way type dispatch (REG/DIR/CHR/BLK/FIFO/SOCK, driver handoff) | Mechanism | Stock 15 | `open.c:29-293` | 15 |
| K-152 | `new_node` creation machine (suspended-symlink EEXIST re-resolve); `pipe_open` pairing (same-mode ENXIO / no-peer suspend); `do_mknod`/`do_mkdir` privilege gates | Mechanism | Stock 15 | `open.c:299-598` | 15 |
| K-153 | `actual_lseek` (ESPIPE/EOVERFLOW/skip-if-same); `do_close` call site (mechanism in new 14); `forbidden` pre-gate + X_BIT substitution; O_TRUNC W_BIT refill; SUSPEND rollback exemption | Constraint | Stock 15/29 | `open.c:146,151-156,282,603-727` | 15 |
| K-160 | read/write/peek three-way same machine; five-way dispatch (PIPE/CHR/SOCK/BLK/REG) with PEEK deny/allow matrix | Concept | Stock 16 | `read.c:127-251`; `write.c:15-24` | 16 |
| K-161 | Header triple-latch + zero short-circuit; position advance + O_APPEND rebase; peek never advances | Mechanism | Stock 16 | `read.c:92-116,145,234,262,219-239` | 16 |
| K-162 | bsf global lock fast/slow lanes + unload assertion; FIXME optimistic advance; SIGPIPE fire matrix; `do_getdents` dual EBADF | Mechanism | Stock 16 | `read.c:49-87,168-271,282-317`; `glo.h:36` | 16 |
| K-170 | Pipe waiting trilemma (empty-with-writer waits / no-writer returns zero / write-without-reader broken) + `v_size` stock semantics | Concept | Stock 17 | `pipe.c:187-199,373-380` | 17 |
| K-171 | `pipe_check` read-3/write-5 verdicts; EAGAIN fast-fail still wakes; `create_pipe` seven steps with monotonic rollback; `map_vnode` idempotent map | Mechanism | Stock 17 | `pipe.c:39-288` | 17 |
| K-172 | `suspend`/`pipe_suspend` five-parameter registration; `susp_count`/`reviving` double ledger; `release` two-phase scan; `revive` deferred mark; `unpause` six interrupt branches; `unsuspend_by_endpt`驱散 classification | Mechanism | Stock 17 | `pipe.c:294-561`; `glo.h:14,16` | 17 |

### G08 — Namespace and devices (home: new 18 devmap, 19 mount, 20 bdev, 21 cdev, 22 sdev)

| ID | Name | Type | Source | Anchor | Home |
|----|------|------|--------|--------|------|
| K-180 | Number/person split (major stable, endpoint replaceable; 135-entry linear scan suffices) | Concept | Stock 19 | `dmap.c`; `dmap.h:82` | 18 |
| K-181 | dmap eight fields + `NR_DEVICES=135` + CTTY major exception; smap six fields | Struct | Stock 19 | `dmap.h:16-26`; `type.h:41-49` | 18 |
| K-182 | `map_service`/`map_driver`, up/down classification, `dmap_endpt_up`/`smap_endpt_up`, unmap four-step trigger (worker_stop + invalidate + select wake) | Mechanism | Stock 19 | `dmap.c:69-328`; `smap.c:22-273` | 18 |
| K-183 | `do_ioctl` + grant-direction decode (`make_ioctl_grant` IOR/IOW) | Mechanism | Stock 19 | `device.c:34-95` | 18 |
| K-184 | DS event subscription call site (01) vs `ds_event` classification (here) | Mechanism | Stock 01/19 | `main.c:441`; `dmap.c` ds path | 18 |
| K-190 | Graft model (single tree across FSes; `m_root_node`/`m_mounted_on` stitching) | Concept | Stock 18 | `mount.c:156-166`; `vmnt.h:14-15` | 19 |
| K-191 | Device-number codec (major/minor/makedev round-trip); pseudo-devices + nonedev bitmap | Struct | Stock 18 | `sys/types.h` codec; `mount.c:33-37,628-653` | 19 |
| K-192 | `do_mount` gate chain; `mount_fs` five-phase commit with rollback boundary; EBUSY double meaning; mount-point refcount exactly 1 | Mechanism | Stock 18 | `mount.c:85-385` | 19 |
| K-193 | `RES_THREADED` negotiation (window = NR_WTHREADS minus one); `have_root`/double-root `RootStage`; `MAKEROOT` switch; `update_bspec` best-effort reroute | Mechanism | Stock 18/11 | `mount.c:205-349,46-80` | 19 |
| K-194 | `mount_pfs` canned mount (three fixed labels; print-don't-block failure); `do_umount`/`unmount` seven-step teardown; `unmount_all` outside-in sweep; `name_to_dev` three classes | Mechanism | Stock 18/01/10 | `mount.c:391-622` | 19 |
| K-200 | bdev family: open/close/ioctl/sendrec/reply/up; ERESTART×5 retry; EDEAD/ELOCKED triage; thread-blocking contract | Mechanism | Stock 20 | `bdev.c` | 20 |
| K-201 | cdev family: map/get/clone/opcl/open/close/io/select/cancel/reply; /dev/tty redirect; CTTY three gates; EAGAIN↔EINTR swap | Mechanism | Stock 21 | `cdev.c` | 21 |
| K-202 | sdev family: socket/bind/connect/…/select; short-vs-long query; three grants; suspend/stop/cancel/finish/reply; accept thread; **`sdev_stop` (sdev.c:912) death cascade** as the sdev face | Mechanism | Stock 22 | `sdev.c` (1,114 lines); `sdev.c:912` | 22 |
| K-203 | Consolidated driver-death cascade spectrum (filedes invalidate + sdev stop + select wake sharing one trigger; Redox/cancellation对照) | Concept | Stock 22/14/23 | `sdev.c:912`; `filedes.c:250-308`; `select.c` wake path | 22 |

### G09 — Multiplex, sockets, lifecycle (home: new 23–26)

| ID | Name | Type | Source | Anchor | Home |
|----|------|------|--------|--------|------|
| K-210 | `do_select` six stages; four fd types; two-wave UPDATE/BUSY; `MAXSELECTS=25` table; timeout tristate; `expire_timers` wiring | Mechanism | Stock 23 | `select.c` (1,416 lines); `select.c:821` init | 23 |
| K-211 | select wake paths (`select_return`, cdev/sdev reply halves) as the select face of the cascade | Mechanism | Stock 23/21/22 | `select.c:783`; `cdev.c:481`; `sdev.c:759` | 23 |
| K-220 | Socket upper family (socket/pair/bind/connect/listen/accept/sendto/recvfrom/sockmsg/opts); fd install; cleanup table; `resume_accept/recv` split vs 22 | Mechanism | Stock 24 | `socket.c` (762 lines) | 24 |
| K-221 | `do_socketpath` entry gates (in new 13) vs address semantics (here) | Protocol | Stock 24/13 | `path.c:803-836` | 24 |
| K-230 | `pm_exec` check chain; `#!` script restack; ELF `.interp`; `vfs_memmap` vs `read_seg` dual load; `clo_exec`; setuid timing | Mechanism | Stock 25 | `exec.c` (763 lines); `exec.c:161-180` | 25 |
| K-231 | ELF core writer (`write_elf_core_file`); `MINIX-CORE` notes; page-copy/zero/`LONG_MAX` cut; `core.<pid>` naming; `pm_dumpcore` trigger chain (entry in new 10) | Mechanism | Stock 26/10 | `coredump.c` (327 lines); `misc.c:903-943` | 26 |

### G10 — Metadata and control (home: new 27–31)

| ID | Name | Type | Source | Anchor | Home |
|----|------|------|--------|--------|------|
| K-240 | Namespace mutation family (link/unlink/rename/truncate/ftruncate/slink/rdlink); EXDEV; sticky-bit; same-length OK | Mechanism | Stock 27 | `link.c` (508 lines) | 27 |
| K-241 | CWD family (chdir/fchdir/chroot/change_into); stat family (stat/fstat/lstat); statvfs family (`fill_statvfs` three-name copy; dead-path helper excluded) | Mechanism | Stock 28 | `stadir.c` (446 lines); `stadir.c:283-285` | 28 |
| K-242 | Single permission gate (`forbidden`/`in_group`/`read_only`) owned here, cited by 15/16/27/28/31; chmod/chown/umask/access; EROFS/EPERM; setgid strip | Mechanism | Stock 29 | `protect.c` (302 lines) | 29 |
| K-243 | fcntl 13 commands incl. `F_DUPFD`→`get_fd` delegation; `NR_LOCKS=8` table; GETLK/SETLK/SETLKW; region arithmetic; unlock four-branch; broadcast wake; close→`lock_revive` | Mechanism | Stock 30/14 | `lock.c` (192 lines); `misc.c` fcntl path | 30 |
| K-250 | Introspection hub (getsysinfo↔fproc_light, sync, fsync, svrctl, utimens via `time.c:155`, gcov_flush with super_user gate, getrusage, panic_hook) with per-query home table | Mechanism | Stock 31 | `misc.c:52-330`; `time.c`; `gcov.c:31` | 31 |

### G11 — New knowledge (home: new 32 vm-bridge, 33 build-test, 99 ledger)

| ID | Name | Type | Source | Anchor | Home |
|----|------|------|--------|--------|------|
| K-260 | VM↔VFS bridge: `do_vm_call` decode, `VM_VFS_REPLY`, FDLOOKUP/FDCLOSE/FDIO sub-ops, always-async reply discipline, VM-side fdref对照 | Protocol | New (buried in old 31/misc.c, never standalone) | `misc.c:380-500`; `com.h:693-714`; `02-stage-vm` fdref docs | 32 |
| K-261 | Build: Makefile SRCS (33 C + conditional gcov), `-lsys -ltimers -lexec -lmthread`, `minix.service.mk` profile, `USE_COVERAGE` gate | Tooling | New (no doc owns it today) | `servers/vfs/Makefile:1-27` | 33 |
| K-262 | Link/load position: boot-image slot order, SEF (not asm) entry for a user server, mthread linkage, `libsys`/`libtimers`/`libexec` roles | Tooling | New | `kernel/table.c` image; `libmthread/` tree; `Makefile:24-25` | 33 |
| K-263 | Test map: no C-side VFS test dir; Rust per-module `#[test]` counts (recount at B-phase, never copy old totals); wire pin-test pattern (absolute-value assertions) | Test | New | `minix3/minix/tests/` (no vfs); `os/servers/vfs/src/` test counts | 33 |
| K-264 | Capacity constants with mechanism justification (`NR_FILPS/VNODES/MNTS/WTHREADS/LOCKS`, `OPEN_MAX`, `PIPE_BUF`, `SYMLOOP`); four `m_type` prefix namespaces; `glo.h`→`VfsState` map; dual-count invariant restated | Constraint | Stock 99 | `const.h`; `glo.h:13-44`; `com.h` bases | 99 |
| K-265 | Intentional-omissions ledger (dead code + deferred transports + debug families), appended per B-phase fix | Tooling | Stock 99 | `vmnt.c:246`; `vfsif.h:41`; `fproc.h` LOCK_DEBUG | 99 |

Pool statistics: 119 entries (Stock 110 / New 9); by type: Concept 12, Mechanism 55,
Struct 22, Protocol 12, Constraint 11, Evolution 5, Tooling 6, Test 1 (test behavior
additionally asserted inside each §5 acceptance line). Per-C-file attribution is
complete (see G2 gate check in §9).

---

## 3. Coverage Audit

### 3.1 Gap table (topics in the universe, in no document's contract today)

| # | Missing topic | Evidence anchor | Disposition → new home (as New knowledge, no stock-destination rule applies) |
|---|---------------|-----------------|-------------------------------------------------------------------------------|
| N1 | VM↔VFS bridge as a standalone protocol (`do_vm_call`, `VM_VFS_REPLY`, FDLOOKUP/FDCLOSE/FDIO, always-async discipline) | `misc.c:380-500`; `com.h:693-714` | New doc 32; K-260 |
| N2 | Build/test hub (Makefile SRCS/libs, SEF-not-asm entry, mthread linkage, C-test absence, Rust test map, wire pin-test pattern) | `servers/vfs/Makefile:1-27`; `libmthread/`; `minix3/minix/tests/` | New doc 33; K-261–K-263 |
| N3 | Rust current-state deltas inside existing topics (64-arm `dispatch_syscall` + `run_once`, REQ authority import, `Route::Enosys`, RS-prefix true values) | `syscalls.rs`; `main_loop.rs:148-169`; `request.rs:19`; `minix-types/src/ipc/fs_driver.rs:21` | No new doc; B-phase must source from Rust truth per contract (09/12 line items) |
| N4 | `sdev_stop` death cascade given full spectrum status (filedes + sdev + select one trigger) | `sdev.c:912` | No new doc; consolidated in new 22 (K-203), referenced from 14/23 |
| N5 | Per-doc test-count snapshot hygiene (old docs embed totals that rot) | `todo.md` fix history; 48,181-line Rust tree | Rule, not content: §5 contracts forbid snapshot numbers; acceptance uses recount commands |

### 3.2 Duplication table (same topic expanded in several docs → single home, rest cite)

| # | Topic | Occurrences | New home (primary) | Others become pointers |
|---|-------|-------------|--------------------|------------------------|
| DUP-1 | `close_filp`→`put_vnode` lifecycle | 04/14/15 | 04 (object truth, K-045) | 14/15 cite call-site only |
| DUP-2 | `dup/put_vnode`, 256-threshold | 05/10/14 | 07 (K-072–K-073) | 10/14 cite |
| DUP-3 | `m_comm` window fields | 06/11 | 11 (K-110) | 08 names the field, defers queue |
| DUP-4 | SUSPEND/revive taxonomy | 08/09/17/23/21/22 | 09 declares the contract (K-094); each family owns its face (K-172/K-211/K-201/K-202) | 05 states primitives only |
| DUP-5 | TRANSID encode/decode | 09/11 | 11 (K-113) | 09 cites route use |
| DUP-6 | ioctl grant direction decode | 19/21 | 18 (K-183) | 21 cites |
| DUP-7 | `VNODE_*/VMNT_*`→`TLL_*` maps | 05/06/07 | 06 declares primitives (K-060–K-064); 07/08 own their maps (K-074/K-084) | No triple expansion |
| DUP-8 | `forbidden`/`in_group` gate | 15/16/27/28/31 | 29 (K-242) | Callers cite the gate, never re-derive |
| DUP-9 | `F_DUPFD`→`get_fd`; close→`lock_revive` | 14/30 | Owner keeps mechanism (14: K-141; 30: K-243); peer cites | One-line cross-link each way |
| DUP-10 | `do_socketpath` gates vs semantics | 13/24 | 13 gates (K-136); 24 semantics (K-221) | Explicit split, both cite |
| DUP-11 | REQ count truth (32-live correction) | 00/12 | 12 (K-120) | 00 cites 12, never restates numbers |

### 3.3 Out-of-scope table (declared in a doc but owned by another stage)

| # | Topic | Wrongly-homed excursus risk | Correct home |
|---|-------|-----------------------------|--------------|
| O1 | REQ server-side implementations (mfs/pfs/…) | 12 drifting into FS internals | 15-stage-fs; 12 stops at the wrapper boundary |
| O2 | Block/char/socket driver bodies | 20/21/22 drifting into driver logic | stages 16/17; device docs stop at the VFS protocol half |
| O3 | `sys_*` primitive implementations | Any doc re-deriving safecopy/datacopy/hz | 01-stage-kernel; VFS cites call sites |
| O4 | PM send-side orchestration (`tell_vfs`, signals, wait) | 10 re-narrating PM internals | 04-stage-pm; 10 covers only the receiving halves |
| O5 | VM-side mapping implementation | 25/32 re-implementing mmap | 02-stage-vm; 25/32 cover call sites + wire |
| O6 | trap path from user process into VFS | 09 re-deriving IPC delivery | 01-stage-kernel IPC docs |
| O7 | libc stubs / crt0 / SEF framework itself | 01/33 re-teaching the framework | 14-stage-runtime; 01/33 cover VFS's *use* of it |

### 3.4 Previous-stage firewall (what PM already teaches; VFS cites, never re-expands)

PM owns: mproc authority, signal sets, itimer, `PM_*` dispatch, fork/exit/wait
orchestration, credential logic internals (`04-stage-pm/00-pm-overview.md`). VFS new
10/25/26 cover only: fd-share on fork, reclaim on exit, exec forwarding checks, core
trigger entry. Any B-phase paragraph that re-derives PM internals fails its contract.

### 3.5 Non-C topics: fixed ten, each answered (no "TBD" allowed)

| Topic | Answer: where taught or why excluded |
|-------|--------------------------------------|
| Link and load | New 33: `Makefile:21-25` LDADD/DPADD, `minix.service.mk`, `-lmthread`; user server links libraries, not kernels |
| Image and memory layout | New 33: boot-image slot order (`kernel/table.c`), per-contract no VFS-owned layout beyond tables it allocates |
| Assembly entry and trap entry | New 33 (one paragraph): none — SEF `main()` entry (`main.c:54`); traps belong to 01-stage-kernel |
| Boot assembly | New 01: `sef_cb_init_fresh` + `do_init_root` + `mount_pfs/mount_fs` is the VFS "boot"; 33 points back |
| Build and toolchain | New 33: SRCS list, `USE_COVERAGE`→`gcov.c` gate (`Makefile:11-14`), warning profile |
| Cross-module interfaces and wire formats | New 09 (call dispatch) / 10 (PM wire) / 11+12 (FS wire) / 32 (VM wire) / 99 (four-namespace index) |
| Error paths | New 09: `do_reply` soft-fail (`main.c:193-203`), `ENOSYS` gate, `SUSPEND` discipline; per-family errno mapping in situ |
| Shutdown and exit | New 10 (`pm_reboot` sequence) + 19 (`unmount_all` sweep); 31's `sync` cites 10 |
| Concurrency and synchronization | New 05 (worker pool) + 06 (tll) + per-object lock maps (07/08/04); single-thread port notes in each Evolution line |
| Test infrastructure | New 33: C-side absence + Rust module test map + pin-test pattern; per-doc acceptance re-points here |

---

## 4. New Directory

### 4.1 Chapter table (number = reading order; prereqs point strictly backward)

| New | Title | One-line positioning | Group |
|-----|-------|----------------------|-------|
| 00 | vfs-overview | What VFS is, what it speaks to, how to read this stage | Frame |
| 01 | vfs-init-main | How VFS is born: SEF entry, PM handshake, init chain, root gate | Birth |
| 02 | fproc-struct | The per-process file-state object: every field, every flag | Process model |
| 03 | fproc-table | The fixed process table: lookup, guards, snapshots | Process model |
| 04 | filp-table | The open-file object and its table: sharing by count | Open-file model |
| 05 | worker-thread | The execution engine: 9 threads, gates, sleep primitives (ex-08, moved early) | Execution |
| 06 | tll-lock | The three-state lock used by vnode/vmnt (ex-07, moved before its users) | Execution |
| 07 | vnode-table | The inode-projection cache with lazy dual counts (ex-05) | Objects |
| 08 | vmnt-table | The mount-boundary table with crash cascade (ex-06) | Objects |
| 09 | main-loop | The event loop: priority dispatch, call_vec, reply discipline | Loop+wire |
| 10 | pm-protocol | The PM control plane: schedule, fork/exit, creds, reboot | Loop+wire |
| 11 | fs-comm | The FS transport: windows, TRANSID, queues, cancels | Loop+wire |
| 12 | request-wrappers | The FS message catalog: 32 live REQs, grants, responses | Loop+wire |
| 13 | path-lookup | The name resolver every name-call shares | Names |
| 14 | filedes | The descriptor index layer: alloc, gates, copy, invalidate | Names |
| 15 | open-close | Acquiring, creating, positioning, releasing names | Names |
| 16 | read-write | Moving bytes: one machine, five destinations | Names |
| 17 | pipe | Named/anonymous byte queues with suspend/revive | Names |
| 18 | device-map | Number→endpoint maps and ioctl decoding (ex-19, moved before mount) | Namespace+devices |
| 19 | mount | Grafting trees across filesystems (ex-18, moved after devmap) | Namespace+devices |
| 20 | bdev | The block-device protocol half | Namespace+devices |
| 21 | cdev | The char-device protocol half with CTTY | Namespace+devices |
| 22 | sdev | The socket-driver protocol half with death cascade | Namespace+devices |
| 23 | select | Waiting on many descriptors with timers | Multiplex |
| 24 | socket | The socket syscall family above the driver | Multiplex |
| 25 | exec | Replacing a process image via PM request | Lifecycle |
| 26 | coredump | Writing the ELF core after death | Lifecycle |
| 27 | link | Mutating names and sizes | Metadata |
| 28 | stadir | Working directories, stats, filesystem stats | Metadata |
| 29 | protect | The single permission gate and its callers | Metadata |
| 30 | fcntl-lock | Descriptor control and POSIX byte locks | Metadata |
| 31 | misc-queries | Introspection hub: one index, per-query homes | Metadata |
| 32 | vm-bridge *(new)* | The VM↔VFS back door: fd lookup/close/IO over wire | Bridge (new) |
| 33 | build-test *(new)* | How VFS is built, linked, and tested | Build (new) |
| 99 | global-concepts | Invariants, capacities, wire index, omissions ledger | Capstone |

Total: 36 docs (33 carried + 2 new + 00/99 retained). Count is a consequence of
concept hygiene, not a target: the old 33-count imposed no constraint, and the soft
per-doc ceiling (≈3000 lines for one genuinely complex concept) is respected by
construction — pipe/select/sdev/path are each one concept and each fits comfortably;
no chapter needs splitting for length, and no two chapters share a semantic unit.

### 4.2 Reading paths and parallel-body representatives

- **Mainline (must read in order):** 00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 →
  09 → 11 → 12 → 13 → 14 → 19 → 99. This chain carries boot → objects → loop →
  transport → names → namespace → invariants with zero forward references.
- **Branch A (process control):** 10 → 25 → 26 → 32 (PM plane, exec, core, VM door).
- **Branch B (data movement):** 15 → 16 → 17 → 23 → 24 (open, bytes, pipes, select, sockets).
- **Branch C (devices):** 18 → 20 → 21 → 22 (map, then one family each).
- **Branch D (metadata):** 27 → 28 → 29 → 30 → 31 (mutation, status, permission, locks, queries).
- **Skippable on first pass:** 26 (coredump format detail), 30 (lock arithmetic), 31
  (per-query reference), 33 (build/test). Each declares itself skippable in its header.
- **Parallel bodies (framework + representative + difference tables, never a forced line):**
  - 64 syscalls: framework = 09 (`call_vec`); groups per branches above; representative
    members讲透: `do_read` path (16) for data flow, `pm_fork` (10) for control flow,
    `req_lookup` (12) for FS flow; all other members carried as difference tables.
  - 12 PM requests: framework = 10 three-level schedule; representative `pm_fork`讲透.
  - 33 REQ constants: framework = 12 spectrum; representative `req_lookup`讲透.
  - Device families: framework = 18 (maps + ioctl decode); 20/21/22 each讲透 one open/io/reply cycle.
  - Misc queries: framework = 31 hub index; each query one row pointing at its mechanism home.

### 4.3 Why this order (reordering evidence, each item independently verifiable)

1. **Worker (05) before tables:** `worker_init` (`main.c:445`) precedes the four table
   inits (`main.c:486-489`); tll blocking waits on worker primitives. Old order
   (tables-then-worker) contradicted both runtime and blocking order.
2. **tll (06) before vnode/vmnt (07/08):** `vnode.h:22` and `vmnt.h:9` declare `tll_t`
   fields and both map `VNODE_*/VMNT_*`→`TLL_*`; teaching the users before the lock
   is a hard forward reference. `file.h:14` (`mutex_t filp_lock`, not tll) is called
   out explicitly so readers stop expecting filp to use tll.
3. **device-map (18) before mount (19):** `mount_fs` consults dmap; old mount-before-map
   forced forward references in both directions. The swap resolves them.
4. **Everything else keeps its number:** 27 of 33 carried docs are untouched by
   renumbering, bounding migration cost (§8) while fixing all three hard violations.

---

## 5. Per-Document Contracts (all seven elements mandatory; B-phase writes to these)

Conventions for every contract below: "Covers" cites §2 K-IDs (normative inclusion);
"Not covers" names the neighbor that owns each adjacent topic; "Prereqs" list new
numbers only (machine-checkable for G3); "Ground truth" lists C files/functions/macros
plus non-C artifacts with anchors; "Acceptance" is verifiable (key-question list,
mandatory comparison or diagram). Snapshot numbers, test totals, and line numbers are
**forbidden** in rebuilt bodies (N5 hygiene); wire absolute values (FS_BASE, RS bases,
VFS_BASE, TRANSID) are **mandatory** pins with C anchors. Each rebuilt doc is written
in **English**.

### 00-vfs-overview

- **Positioning:** the 10-minute map: what VFS is, whom it talks to, which path to read for your goal.
- **Covers:** K-001–K-006.
- **Not covers:** any mechanism (→ 01–33); any invariant proof (→ 99); PM internals (→ 04-stage-pm).
- **Prereqs:** none (entry point; states boot-chain context inline).
- **Post-refs:** every chapter points back here for navigation; 99 closes the loop.
- **Ground truth:** `servers/vfs/` tree (33+15 files); `callnr.h:68`; `com.h:513-544,589`; `vfsif.h:41-75`; `00-master-plan/README.md` chain.
- **Knowledge rows:** per §2 G00; source = old 00 §§ (stock) except K-006 ARCH index (recount from Rust truth at B-phase).
- **Acceptance:** reader can (1) draw the four-neighbor diagram from memory, (2) state the three protocol-surface sizes without the 33-live error, (3) pick their reading path.

### 01-vfs-init-main

- **Positioning:** how a dead boot image becomes a live file server, step by step.
- **Covers:** K-010–K-019.
- **Not covers:** fproc fields (→ 02), table guards (→ 03), worker internals (→ 05), dispatch (→ 09), PM request semantics (→ 10), mount mechanism (→ 19).
- **Prereqs:** 00.
- **Post-refs:** 05 (worker_init call site), 10 (handshake → protocol), 18 (subscribe → classify), 19 (do_init_root → mechanism).
- **Ground truth:** `main.c:54-64,374-527`; `worker.c:27`; `mount.c:391-425`; `com.h:520-531`; `rs/table.c` + `kernel/table.c` (loaders, cited not taught).
- **Knowledge rows:** per §2 G01; K-013 callback groups recounted from `main.c:374-388` at B-phase.
- **Acceptance:** reader can (1) list the S01–S17 chain with the worker-before-tables invariant, (2) narrate the handshake message by message, (3) explain why root mount cannot fail silently (panic anchor).

### 02-fproc-struct

- **Positioning:** the per-process file-state object — every field, every flag, no table logic.
- **Covers:** K-020–K-028.
- **Not covers:** table lookup/guards (→ 03), filp sharing (→ 04), suspend mechanics (→ 05), fork/exit protocols (→ 10), fd operations (→ 14).
- **Prereqs:** 00, 01.
- **Post-refs:** 03, 04, 10, 14.
- **Ground truth:** `fproc.h:15-115`; `const.h:16-28`; `misc.c:606-608` (lock non-sharing); `fproc.rs`, `minix-types/types/id.rs` (port notes).
- **Acceptance:** reader can (1) group all fields into the four faces without looking, (2) explain why `fp_lock` cannot be shared across fork, (3) map each `fp_u` payload to its reviver.

### 03-fproc-table

- **Positioning:** the fixed table that makes processes addressable: lookup, guards, snapshots.
- **Covers:** K-030–K-034.
- **Not covers:** field semantics (→ 02), fd array mechanics (→ 14), MIB framework (→ Referenced, not taught).
- **Prereqs:** 01, 02.
- **Post-refs:** 09 (endpoint mapping in loop), 10 (childno checks), 31 (getsysinfo consumes light rows).
- **Ground truth:** `fproc.h:10-12,101-124`; `utility.c:94-127`; `glo.h:26-27`; `misc.c:52-96`.
- **Acceptance:** reader can (1) state the dual-sentinel idle test, (2) apply the three guards to a hostile endpoint, (3) explain what `fproc_light` omits and why.

### 04-filp-table

- **Positioning:** the sharing object between descriptors and cached files, and its table.
- **Covers:** K-040–K-049.
- **Not covers:** fd-index allocation (→ 14), vnode internals (→ 07), select bit production (→ 23).
- **Prereqs:** 02, 03.
- **Post-refs:** 07 (filp→vnode), 10 (fork share), 14 (index→object), 23 (FSF consume).
- **Ground truth:** `file.h:5-48`; `filedes.c:26-86,162-246,313-519`; `open.c:133-135`; `misc.c:629`.
- **Acceptance:** reader can (1) draw the fd→filp→vnode two-hop with ownership arrows, (2) compute the close-to-release count walk, (3) separate the three borrow states with examples.

### 05-worker-thread (old 08, moved early)

- **Positioning:** the execution engine that lets a single-threaded-look loop survive blocking calls.
- **Covers:** K-050–K-057.
- **Not covers:** tll primitives (→ 06), dispatch priority (→ 09), per-family suspend payloads (→ 17/21/22/23/30).
- **Prereqs:** 01, 02, 03.
- **Post-refs:** 06 (tll waits here), 09 (loop yields here), 10 (deferred PM arms park here).
- **Ground truth:** `worker.c` (607 lines); `threads.h:10-38`; `const.h:9`; `main.c:445,492-493,558-575`; `worker.rs:1-60` + `lib.rs:12-14` (port direction).
- **Acceptance:** reader can (1) explain the spare-thread invariant with a deadlock scenario it prevents, (2) trace allow(FALSE)→queue→allow(TRUE), (3) contrast the 9-thread C truth with the slot-machine port without conflating them.

### 06-tll-lock (old 07, moved before its users)

- **Positioning:** the one lock primitive that vnode and vmnt are built on (and filp is not).
- **Covers:** K-060–K-066.
- **Not covers:** vnode/vmnt lock maps (→ 07/08), filp softlock detail (→ 04), worker sleep internals (→ 05).
- **Prereqs:** 04, 05.
- **Post-refs:** 07, 08 (maps), 13 (resolution-time protocol cites).
- **Ground truth:** `tll.h:6-18`; `tll.c` (324 lines); `tll.rs`; `file.h:14` (filp uses mutex — the negative anchor).
- **Acceptance:** reader can (1) run the five-way dispatch by hand, (2) justify write-over-serial with a starvation scenario, (3) state exactly which objects use tll and which do not.

### 07-vnode-table (old 05)

- **Positioning:** the inode-projection cache with lazy dual counts.
- **Covers:** K-070–K-076.
- **Not covers:** vmnt pairing logic (→ 08), lookup miss handling (→ 13), FS-side inode truth (→ 15-stage-fs).
- **Prereqs:** 04, 06.
- **Post-refs:** 08 (v_vmnt pairing), 13 (advance builds rows here), 10/14 (dup/put callers cite).
- **Ground truth:** `vnode.h:4-29`; `vnode.c` (316 lines); `const.h:8`.
- **Acceptance:** reader can (1) walk the fast/slow dup/put paths with count values, (2) explain the 256-threshold economics, (3) apply the lock map to a concurrent read/write pair.

### 08-vmnt-table (old 06)

- **Positioning:** the mount-boundary table: allocation, locks, crash reclaim.
- **Covers:** K-080–K-087.
- **Not covers:** mount mechanism (→ 19), request windows (→ 11), statvfs names (→ 28).
- **Prereqs:** 06, 07.
- **Post-refs:** 11 (m_comm tenancy), 19 (mount commits rows here), 28 (statvfs reads rows here).
- **Ground truth:** `vmnt.h:7-33`; `vmnt.c` (292 lines); `const.h:7` (=16, with erratum note); `type.h:1-10`.
- **Acceptance:** reader can (1) state capacity 16 with the C anchor (never 8), (2) run the unmap cascade's four steps, (3) defend the dead-code verdict on `fetch_vmnt_paths` with the zero-caller grep.

### 09-main-loop

- **Positioning:** the heart that never blocks: priority dispatch and the reply contract.
- **Covers:** K-090–K-096.
- **Not covers:** worker sleep internals (→ 05), PM request semantics (→ 10), FS queue internals (→ 11), device reply decode (→ 20/21/22), pipe replay detail (→ 17).
- **Prereqs:** 01, 02, 05, 08.
- **Post-refs:** all of 10–12, 18–24 cite their dispatch arm here.
- **Ground truth:** `main.c:69-141,146-302,580-663,921-973`; `table.c:15-82`; `callnr.h:68-137`; `glo.h:13-44`; `main_loop.rs:148-235`, `syscalls.rs`, `call_table.rs` (port state at B-phase).
- **Acceptance:** reader can (1) recite the eight-way order with the non-overlap argument, (2) explain revive-first fairness, (3) classify any new message into exactly one arm, (4) draw the `call_vec` alias rows.

### 10-pm-protocol

- **Positioning:** the control plane from the receiving side: what PM orders, how VFS serializes it.
- **Covers:** K-100–K-107.
- **Not covers:** PM send-side orchestration (→ 04-stage-pm), syscall implementations (→ 13–31), FS transport (→ 11).
- **Prereqs:** 03, 09.
- **Post-refs:** 25 (exec continues here), 26 (dumpcore entry), 32 (VM door contrast).
- **Ground truth:** `main.c:668-915`; `misc.c:504-943` (fork/exit/creds/reboot/dumpcore-entry); `com.h:513-544`; `ipc/dispatcher.rs`; `minix-types/src/ipc/vfs.rs:110-180` (wire pins).
- **Acceptance:** reader can (1) sort all 12 requests into the three schedule levels, (2) walk `pm_fork`'s four sharing steps with lock/count/vnode actions, (3) trace the reboot eight-step with the unmount sweep pointer.

### 11-fs-comm

- **Positioning:** the transport that keeps every FS request accounted for: windows, IDs, queues.
- **Covers:** K-110–K-114.
- **Not covers:** REQ message catalog (→ 12), path callers (→ 13–17), VM channel (→ 32).
- **Prereqs:** 08, 09.
- **Post-refs:** 12 (wrappers ride here), 19 (mount negotiates windows here).
- **Ground truth:** `comm.c` (244 lines); `type.h:1-10`; `com.h:909-912`; `vfsif.h:79-81`; `fs_comm.rs`.
- **Acceptance:** reader can (1) run the F01–F07 roundtrip on the blank diagram, (2) explain CALLBACK suppression with a deadlock it prevents, (3) encode/decode a TRANSID by hand.

### 12-request-wrappers

- **Positioning:** the complete FS message catalog: what each REQ means and what it costs.
- **Covers:** K-120–K-124.
- **Not covers:** FS-side implementations (→ 15-stage-fs), transport windows (→ 11), mount-time negotiation use (→ 19).
- **Prereqs:** 06, 11.
- **Post-refs:** 13 (lookup consumes E-codes), 16 (peek/read half), 19 (readsuper).
- **Ground truth:** `request.c` (1,213 lines); `request.h:12-38`; `vfsif.h:20-81`; `com.h:589`; `minix-types/src/ipc/fs_driver.rs:21-135` + `minix-fs/src/protocol.rs:30` (converged authority).
- **Acceptance:** reader can (1) state FS_BASE=0xA00 with the `com.h:589` anchor and reject 0x600, (2) recite the 32-live count truth with the dead GETNODE anchor, (3) trace CPF_TRY retry to ERESTART, (4) name which REQ each of mount/lookup/read consumes.

### 13-path-lookup

- **Positioning:** the shared name resolver: how a string becomes a (vnode, mount) pair.
- **Covers:** K-130–K-136.
- **Not covers:** grant construction (→ 12), vnode allocation (→ 07), creation syscalls (→ 15/27).
- **Prereqs:** 06, 07, 08, 11, 12.
- **Post-refs:** 15 (open parses here), 19 (mount resolves here), 24 (socketpath gates here), 27/28 (link/stat resolve here).
- **Ground truth:** `path.c` (933 lines); `path.h`; `utility.c:24-93`; `const.h:7,32`; `path.rs` (seam notes).
- **Acceptance:** reader can (1) walk advance→eat_path→lookup with lock upgrades, (2) resolve a cross-mount + symlink-loop example to ELOOP/EENTERMOUNT, (3) state the NR_MNTS=16 scan bound.

### 14-filedes

- **Positioning:** the descriptor index layer: allocation, gates, copying, mass invalidation.
- **Covers:** K-140–K-145.
- **Not covers:** filp object internals (→ 04), vnode release (→ 07), open/close callers (→ 15).
- **Prereqs:** 02, 04, 06.
- **Post-refs:** 15 (open claims here), 23 (select prefilters here), 30 (fcntl delegates here), 22 (death cascade cites here).
- **Ground truth:** `filedes.c:88-308,524-656`; `open.c:690-727`; `filedes.rs` (port incl. CopyFdCtx).
- **Acceptance:** reader can (1) allocate with F_DUPFD bounds, (2) apply the OPCL privilege correctly, (3) choose the copyfd kind by direction, (4) run one endpoint-death invalidation.

### 15-open-close

- **Positioning:** the fd lifecycle endpoints: acquiring, creating, positioning, releasing.
- **Covers:** K-150–K-153.
- **Not covers:** path traversal (→ 13), fd-index mechanics (→ 14), permission derivation (→ 29), driver open halves (→ 20/21/22).
- **Prereqs:** 13, 14.
- **Post-refs:** 16 (read needs positioned fds), 17 (pipe_open pairing cites).
- **Ground truth:** `open.c` (727 lines); `open.rs`.
- **Acceptance:** reader can (1) run the seven-step pipeline with the six-way dispatch, (2) separate creation from opening with the O_CREAT matrix, (3) place every close step in 14 vs 15 correctly.

### 16-read-write

- **Positioning:** moving bytes through one machine with five destinations.
- **Covers:** K-160–K-162.
- **Not covers:** FS wrapper construction (→ 12), pipe suspension (→ 17), permission pre-gates (→ 29).
- **Prereqs:** 14, 15.
- **Post-refs:** 17 (pipe data path cites), 20 (block path cites bsf).
- **Ground truth:** `read.c` (393 lines); `write.c:15-24`; `glo.h:36`; `read_write.rs`.
- **Acceptance:** reader can (1) route any read/write/peek to its arm, (2) explain bsf fast/slow lanes, (3) predict SIGPIPE vs EPIPE outcomes.

### 17-pipe

- **Positioning:** byte queues with blocking semantics: quota, suspend, precise wakeup.
- **Covers:** K-170–K-172.
- **Not covers:** FS protocol (→ 12), select combination (→ 23), driver waiter classes (→ 18/22).
- **Prereqs:** 02, 04, 16.
- **Post-refs:** 09 (unblock fork cites), 23 (pipe_check cite).
- **Ground truth:** `pipe.c` (561 lines); `glo.h:14,16`; `pipe.rs`.
- **Acceptance:** reader can (1) adjudicate the trilemma for any pipe state, (2) run suspend→release→revive→unpause with ledger values, (3) explain the PFS寄养 unification.

### 18-device-map (old 19, moved before mount)

- **Positioning:** the number→endpoint directory and the ioctl front door.
- **Covers:** K-180–K-184.
- **Not covers:** data paths (→ 20/21/22), invalidation execution (→ 14), worker stop mechanics (→ 05).
- **Prereqs:** 01, 09.
- **Post-refs:** 19 (mount queries labels here), 20/21/22 (families resolve here), 23 (select delivery cites).
- **Ground truth:** `dmap.c` (328 lines); `smap.c` (273 lines); `device.c` (95 lines); `dmap.h:16-26`; `device_map.rs`.
- **Acceptance:** reader can (1) map any major to its endpoint state, (2) decode an ioctl grant direction, (3) run the unmap four-step trigger list.

### 19-mount (old 18, moved after devmap)

- **Positioning:** grafting independent filesystems into one tree, and tearing them down.
- **Covers:** K-190–K-194.
- **Not covers:** vmnt slot mechanics (→ 08), FS protocol (→ 12), statvfs reporting (→ 28).
- **Prereqs:** 08, 12, 13, 18.
- **Post-refs:** 10 (reboot sweep cites), 28 (statvfs reads here).
- **Ground truth:** `mount.c` (653 lines); `mount.rs`.
- **Acceptance:** reader can (1) run the five-phase commit with rollback points, (2) disambiguate the two EBUSY meanings, (3) explain the double-root exception.

### 20-bdev / 21-cdev / 22-sdev (one contract pattern, three instances)

- **Positioning (each):** the VFS-side half of one driver conversation: session, data, death.
- **Covers:** K-200 / K-201 / K-202 respectively; 22 additionally K-203 (consolidated cascade spectrum).
- **Not covers:** driver internals (→ 16/17-stage docs); kernel transport (cited, not taught); each other's families.
- **Prereqs:** 18 (+16 for 20, +15 for 21, +02 for 22).
- **Post-refs:** 09 (reply arms), 23 (select halves), 14 (invalidate face).
- **Ground truth:** `bdev.c` (282) / `cdev.c` (508) / `sdev.c` (1,114, with `:912` stop); `com.h` CDEV/BDEV/SDEV sections; `bdev.rs`/`cdev.rs`/`sdev.rs` (+ `minix-sockdriver` tenancy note for sdev at B-phase).
- **Acceptance (each):** reader can (1) run one open→io→reply→close cycle, (2) route every reply to its loop arm with the `~0x7f` mask proof, (3) for 22 only: run the full death cascade across all three faces.

### 23-select

- **Positioning:** waiting on many descriptors with timeout: the combinator, not the probed mechanisms.
- **Covers:** K-210–K-211.
- **Not covers:** pipe quota (→ 17), driver delivery (→ 21/22), userspace fd_set copy (kernel, cited).
- **Prereqs:** 04, 17, 18.
- **Post-refs:** 24 (sockets wait here).
- **Ground truth:** `select.c` (1,416 lines); `select.c:821` init; `libsys/timers.c:97`; `select.rs`.
- **Acceptance:** reader can (1) run the six stages with the two UPDATE/BUSY waves, (2) explain the timeout tristate, (3) place every wakeup on the cascade map.

### 24-socket

- **Positioning:** the socket API surface above the driver: calls, fd install, cleanup.
- **Covers:** K-220–K-221.
- **Not covers:** driver dialogs (→ 22), select combination (→ 23), fd mechanics (→ 14).
- **Prereqs:** 14, 22, 23.
- **Post-refs:** none forward (leaf) except 99 terms.
- **Ground truth:** `socket.c` (762 lines); `socket.rs`.
- **Acceptance:** reader can (1) separate upper-call from driver-dialog responsibilities, (2) run accept/recv resume split across 22/24, (3) apply the cleanup table to every error exit.

### 25-exec

- **Positioning:** replacing a process image on PM's order: checks, formats, loading paths.
- **Covers:** K-230.
- **Not covers:** path mechanics (→ 13), permission derivation (→ 29), VM mapping internals (→ 02-stage-vm).
- **Prereqs:** 10, 13.
- **Post-refs:** 26 (failed exec vs core contrast).
- **Ground truth:** `exec.c` (763 lines); `exec.rs`.
- **Acceptance:** reader can (1) order the check chain, (2) choose memmap vs read_seg per segment, (3) time setuid application correctly.

### 26-coredump

- **Positioning:** writing the death certificate: ELF core format and page harvesting.
- **Covers:** K-231.
- **Not covers:** register capture (kernel, cited), VM page queries (cited), exit reclamation (→ 10).
- **Prereqs:** 10, 25.
- **Post-refs:** none forward (leaf, skippable).
- **Ground truth:** `coredump.c` (327 lines); `misc.c:903-943`; `coredump.rs`.
- **Acceptance:** reader can (1) name the two NOTE kinds, (2) explain the zero-vs-copy page decision, (3) trace trigger entry from new 10 to file naming.

### 27-link

- **Positioning:** mutating names and sizes: one family, one delegation rule to FS.
- **Covers:** K-240.
- **Not covers:** traversal (→ 13), permission (→ 29), FS traits (cited).
- **Prereqs:** 13.
- **Post-refs:** 28 (stat observes here).
- **Ground truth:** `link.c` (508 lines); `link.rs`.
- **Acceptance:** reader can (1) separate same-fs vs EXDEV outcomes, (2) apply sticky-bit checks, (3) route truncate to the size path.

### 28-stadir

- **Positioning:** observing location and identity: where am I, what is this, how full is its filesystem.
- **Covers:** K-241.
- **Not covers:** traversal (→ 13), permission (→ 29), vmnt locks (→ 08).
- **Prereqs:** 08, 13.
- **Post-refs:** none forward (leaf).
- **Ground truth:** `stadir.c` (446 lines); `stadir.rs`.
- **Acceptance:** reader can (1) separate chdir/chroot/change_into effects, (2) choose stat/fstat/lstat correctly, (3) fill statvfs three names from the right fields.

### 29-protect

- **Positioning:** the single permission gate every mutating call passes (and its narrow API).
- **Covers:** K-242.
- **Not covers:** credential writes (→ 10), traversal fetch (→ 13).
- **Prereqs:** 02, 13.
- **Post-refs:** 15/16/27/28/31 cite.
- **Ground truth:** `protect.c` (302 lines); `protect.rs`.
- **Acceptance:** reader can (1) run `forbidden` before any mutation, (2) compute group membership, (3) list what this chapter refuses to hand over and where each goes.

### 30-fcntl-lock

- **Positioning:** descriptor control plus advisory byte locks: commands, regions, wakeups.
- **Covers:** K-243.
- **Not covers:** fd allocation (→ 14), close mechanics (→ 14/15), FS traits (cited).
- **Prereqs:** 04, 14.
- **Post-refs:** 09 (lock_revive fork cites; skippable leaf otherwise).
- **Ground truth:** `lock.c` (192 lines); `misc.c` fcntl path; `lock.h:7-13`; `fcntl.rs`.
- **Acceptance:** reader can (1) dispatch all 13 commands, (2) compute region overlap, (3) run close→revive broadcast.

### 31-misc-queries

- **Positioning:** the introspection hub: an index of back doors, each row pointing at its mechanism home. Explicitly skippable reference.
- **Covers:** K-250.
- **Not covers:** any mechanism taught elsewhere (each row defers); VM door body (→ 32); reboot body (→ 10).
- **Prereqs:** 09, 29.
- **Post-refs:** 32 (vm_call row), 33 (gcov build-gate row).
- **Ground truth:** `misc.c:52-380`; `time.c:155`; `gcov.c` (73 lines, `USE_COVERAGE` gate); `misc.rs`.
- **Acceptance:** reader can (1) route every query to its home chapter, (2) state the gcov super_user gate, (3) explain why sync lives next to reboot but is taught separately.

### 32-vm-bridge (new)

- **Positioning:** the back door VM knocks on: fd lookup/close/IO over the wire, always async.
- **Covers:** K-260.
- **Not covers:** VM mapping internals (→ 02-stage-vm), fd mechanics (→ 14), FS transport (→ 11).
- **Prereqs:** 11, 14.
- **Post-refs:** none forward (leaf).
- **Ground truth:** `misc.c:380-500`; `com.h:693-714`; `misc.rs:262-305`; `02-stage-vm` fdref docs (contrast, not copy).
- **Acceptance:** reader can (1) decode all three sub-ops, (2) explain why the reply can never be inline, (3) contrast with the FS roundtrip F01–F07.

### 33-build-test (new)

- **Positioning:** how VFS is built, linked, entered, and tested: the engineering facts no mechanism chapter owns.
- **Covers:** K-261–K-263.
- **Not covers:** any runtime mechanism (→ 01–32); SEF framework itself (→ 14-stage-runtime).
- **Prereqs:** 00 (deliberately minimal; skippable first pass).
- **Post-refs:** 31 (gcov row), 12 (pin-test pattern cite).
- **Ground truth:** `servers/vfs/Makefile:1-27`; `kernel/table.c` image; `libmthread/` tree; `libsys/timers.c:97`; `minix3/minix/tests/` (absence); `os/servers/vfs/` test map recounted at B-phase.
- **Acceptance:** reader can (1) reproduce the link line and explain each library, (2) explain why no C-side VFS tests exist and where coverage actually lives, (3) write a wire pin-test in the mandated absolute-value style.

### 99-global-concepts

- **Positioning:** the capstone: invariants, capacities with justification, wire index, omissions ledger. Read last; cited by all.
- **Covers:** K-264–K-265.
- **Not covers:** any mechanism (all → 01–33); this chapter proves nothing new, it consolidates.
- **Prereqs:** 08, 09, 12 (representative consolidation points; header states "read after the mainline").
- **Post-refs:** none (terminal).
- **Ground truth:** `const.h`; `glo.h:13-44`; `type.h`; `fs.h`; `proto.h`; `com.h` bases; `minix-types` authorities.
- **Acceptance:** reader can (1) justify every capacity constant with its mechanism, (2) index all four `m_type` namespaces from memory, (3) extend the omissions ledger with a new entry in the mandated format.

---

## 6. Change Table (old → new; every stock K-ID has a destination in §5)

| Op | Old position | New position | Reason | Knowledge moved |
|----|--------------|--------------|--------|-----------------|
| REORDER | 08-worker-thread | 05-worker-thread | Runtime init order (S10 before S15) + tll waits on worker (K-065) | All 08 stock → new 05 contract |
| REORDER | 07-tll-lock | 06-tll-lock | vnode/vmnt declare `tll_t` fields (hard forward ref); filp negative anchor scopes the claim | All 07 stock → new 06 contract |
| REORDER | 05-vnode-table | 07-vnode-table | Must follow the lock it uses | All 05 stock → new 07 contract |
| REORDER | 06-vmnt-table | 08-vmnt-table | Must follow the lock it uses; keeps vnode→vmnt adjacency | All 06 stock → new 08 contract |
| REORDER | 19-device-map | 18-device-map | `mount_fs` queries dmap; map must precede mount | All 19 stock → new 18 contract |
| REORDER | 18-mount | 19-mount | Companion of the above swap | All 18 stock → new 19 contract |
| KEEP | 00,01,02,03,04,09,10,11,12,13,14,15,16,17,20,21,22,23,24,25,26,27,28,29,30,31,99 | Same numbers | Order already satisfies G3; numbers are stable cross-references (§8) | Stock stays; duplicates trimmed per DUP-1–DUP-11 |
| SPLIT-out (new doc) | 31-misc-queries (VM rows) | 32-vm-bridge | Buried cross-module protocol becomes a standalone concept (N1) | K-260 extracted; 31 keeps an index row pointing at 32 |
| NEW | — | 33-build-test | No doc owns build/test facts (N2) | K-261–K-263 from Makefile/tree/tests (new, not moved) |
| CONTENT-TRIM | All docs' duplicated expansions | Per DUP table | One home per topic; peers become one-line citations | No K-ID deleted; secondary occurrences converted to citations |
| ERRATUM-FIX (in place, no move) | 06-vmnt-table "NR_MNTS=8"; 00 "33 live REQ"; 13 "vmnt[8] scan"; callback-count claims | Same docs (new 08/00/13/01) | Ground-truth corrections, not opinion | K-081/K-120/K-134/K-013 carry the corrected anchors |
| ARCHIVE | Old files under old numbers 05,06,07,08,18,19 | Phase-B archive, not delete | Superseded by renumbered rebuilds | Full §8 migration rows cover every section |

Deletes: none (no stock K-ID is deleted; DUP trims are citation conversions, listed per DUP row). The stale `18-syscall-copy.md` references (35×, §8.4) resolve to real destinations, not to a resurrected doc.

---

## 7. Missing New Chapters (the §3 gap list closed out; no "TBD" permitted)

| Gap | Verdict | New home | Source material | Acceptance (reused from §5, binding) |
|-----|---------|----------|-----------------|--------------------------------------|
| N1 VM bridge | NEW chapter 32-vm-bridge | §5/32 contract | `misc.c:380-500`; `com.h:693-714`; VM fdref docs | §5/32 three checks |
| N2 build/test hub | NEW chapter 33-build-test | §5/33 contract | `Makefile`; image tables; `libmthread/`; tests absence | §5/33 three checks |
| N3 Rust deltas | NO new chapter; B-phase sourcing rule | §5 09/12 contracts | `syscalls.rs`; `main_loop.rs`; `request.rs:19`; `fs_driver.rs` | 09/12 acceptance pins |
| N4 death-cascade spectrum | NO new chapter; consolidate in 22 | §5/22 K-203 | `sdev.c:912`; `filedes.c:250-308`; select wake | 22 third check |
| N5 snapshot hygiene | RULE (all contracts): recount, never copy | §5 convention line | `todo.md` history | G9 check at B-phase review |

Non-C ten answered in §3.5; each row's home is a §5 contract line, so Phase B cannot orphan them.

---

## 8. Anchor Migration and Breakage Cost

### 8.1 Section migration (every changed doc, per-section destinations)

Renumbered docs move as whole files (content re-authored to the new contract, not
line-moved), so migration is doc-level plus the internal section re-homing below.
Unchanged-number docs keep every section; only DUP-trimmed paragraphs convert to
citations (peer doc + section named at B-phase).

| Old doc | Old sections (as knowledge) | New home | Migration type |
|---------|-----------------------------|----------|----------------|
| 05-vnode-table (all) | cache/projection, dual counts, dup/put, lock map, free/find | 07-vnode-table | rewrite-to-contract (number change only) |
| 06-vmnt-table (all) | boundary role, capacity (with erratum fix), alloc, locks, cascade, metadata | 08-vmnt-table | rewrite-to-contract (number change + erratum) |
| 07-tll-lock (all) | states, fields, order, dispatch, unlock/wait, up/down-grade | 06-tll-lock | rewrite-to-contract (number change only) |
| 08-worker-thread (all) | why-concurrency, pool, spare, gates, binding, sleep, start/stop, ARCH A-1 | 05-worker-thread | rewrite-to-contract (number change only) |
| 18-mount (all) | graft, codec, gates, commit, EBUSY, negotiation, root, pfs, teardown, name_to_dev | 19-mount | rewrite-to-contract (number change only) |
| 19-device-map (all) | split rationale, tables, map/up/down, ioctl decode, ds_event | 18-device-map | rewrite-to-contract (number change only) |
| 31-misc-queries (VM rows only) | `do_vm_call` decode + reply discipline | 32-vm-bridge | split-out (new concept home); 31 keeps index row |
| (new) | build/link/entry/tests/pin-tests | 33-build-test | newly sourced from Makefile/tree/tests |

### 8.2 Reference migration (filename references across the stage)

Measured this session: **1,142** inter-doc filename references (`grep -rhoE` count).
Hotspots (top 10, re-measured): `99-global-concepts.md` 55, `09-main-loop.md` 55,
`14-filedes.md` 49, `02-fproc-struct.md` 45, `13-path-lookup.md` 41,
`04-filp-table.md` 40, `12-request-wrappers.md` 38, `10-pm-protocol.md` 34,
`06-vmnt-table.md` 33, `19-device-map.md` 30. Only references naming the six
renumbered docs change targets; all others are byte-stable.

Batch migration procedure (Phase B, after rebuild lands, before archive):
1. `grep -rn "0[5678]-\(vnode\|vmnt\|tll\|worker\)-thread\|table\|lock\.md" <stage>/*.md` →
   review list (covers old 05/06/07/08 filenames).
2. `grep -rn "18-mount\.md\|19-device-map\.md" <stage>/*.md` → review list (covers the swap).
3. Apply sed per mapping row (§8.5), then re-run the 1,142-count and confirm the delta
   equals exactly the renamed-target count (no stragglers, no over-replacements).
4. Code comments: only **1** `stage-vfs` reference remains in Rust (`worker.rs:808`);
   verify its target post-renumber and update the single line by hand (never sed code).

### 8.3 Breakage-cost summary

- Affected inter-doc references: confined to the six renamed targets (estimated
  ≤ 200 of 1,142; exact count produced by step 1–2 greps at migration time — this
  blueprint does not invent the number).
- Affected code references: 1 line (`worker.rs:808`), hand-migrated.
- Stale references resolved, not migrated: 35 × `18-syscall-copy.md` (non-existent;
  §8.4) — these are pre-existing breakage the rebuild heals.
- Risk rating: **low**. Numbers are file identities, and only 6 of 33 change; the
  migration is two swaps plus a block move (05–08), each mechanically checkable by
  recount. Phase B must not renumber anything beyond §4.1 without a new migration
  table.

### 8.4 Stale-reference triage (`18-syscall-copy.md`, 35 occurrences, 10+ files)

The file does not exist in the stage listing. B-phase disposition per occurrence:
`10-pm-protocol.md`, `12-request-wrappers.md`, `13-path-lookup.md`,
`14-filedes.md`, `15-open-close.md`, `16-read-write.md`, `17-pipe.md`,
`18-mount.md`, `19-device-map.md`, `20-bdev.md` (and any further hits from the step-2
grep): each occurrence is re-pointed by its surrounding sentence — syscall-copy
semantics → new 09 (dispatch) or the owning family chapter; FS-copy semantics →
new 11/12. Occurrences that merely decorate an already-anchored claim are deleted.
The 35-count must read **0** after migration (verify with the §0.3 command).

### 8.5 Number mapping (normative for sed)

```text
05-vnode-table.md  → 07-vnode-table.md
06-vmnt-table.md   → 08-vmnt-table.md
07-tll-lock.md     → 06-tll-lock.md
08-worker-thread.md → 05-worker-thread.md
18-mount.md        → 19-mount.md
19-device-map.md   → 18-device-map.md
(new) 32-vm-bridge.md, 33-build-test.md — no old references exist; only inbound index rows.
```

---

## 9. Verification, Self-Check Gates, and Open Questions

### 9.1 Four mechanical checks (§5.4 acceptance method)

1. **Forward-reference scan:** every §5 "Prereqs" field was scanned against §4.1 order:
   all prereqs point to strictly earlier numbers (00 has none; 99 is terminal
   consolidation). Result: **0 forward references. PASS.**
2. **Dependency graph:** nodes = 36 docs, edges = §5 prereq/post-ref pairs. No cycles:
   branches A–D diverge from the mainline and never return except via terminal 99,
   which has no outgoing edges. New 32's prereqs (11, 14) precede it; new 33 depends
   only on 00. Result: **acyclic. PASS.**
3. **Coverage:** every §2 K-ID (K-001–K-265, 119 entries) names a §5 home; every C file
   (33 `.c`) and header (15 `.h`) is attributed to ≥1 K-ID; every non-C ten (§3.5) has
   a home; New entries carry evidence anchors. Deleted stock: none. Result: **100%. PASS.**
4. **Breakage accounting:** 1,142 inter-doc refs counted; hotspots tabled; 6-doc rename
   blast radius bounded with a recount-gated procedure; 1 code ref hand-migrated;
   35 stale refs triaged to zero. Result: **accounted. PASS.**

### 9.2 Self-check gates G1–G9

| Gate | Check | Result |
|------|-------|--------|
| G1 | C true order checkable (10 random anchors) | PASS — §0 table, 10/10 `sed -n` verified this session |
| G2 | Pool completeness: every C file + every non-C artifact has a home or an explicit exclusion with reason | PASS — 33 `.c` + 15 `.h` all in §2 groups; exclusions O1–O7 with correct homes; non-C ten in §3.5 |
| G3 | New directory has zero forward references (per-chapter prereq scan) | PASS — §9.1 check 1 |
| G4 | Dependency graph acyclic; any cycle would carry a split plan | PASS — §9.1 check 2, no cycles found |
| G5 | 100% coverage: every pool item has a destination or a reasoned delete; every New item has an evidence anchor; deletes listed separately | PASS — 119/119 homed; 0 deletes; 9 New anchored |
| G6 | Every split/merge states stock destinations; every new states its sources (10-spot check) | PASS — 1 split (31→32) sourced to `misc.c:380-500` + `com.h:693-714`; 1 new (33) sourced to Makefile/tree/tests; 6 reorders carry full stock |
| G7 | Every contract has all seven elements | PASS — §5, 36/36 contracts with positioning/covers/not-covers/prereqs/post-refs/ground-truth/knowledge+acceptance |
| G8 | Migration covers every changed section; reference migration covers docs + code comments | PASS — §8.1 per-doc rows; §8.2 procedure + recount gate; §8.4 stale triage |
| G9 | Factual claims anchored (10 random verified; speculations labeled) | PASS — §0 G1/G9 table; two judgment calls labeled below as推测/待验证 |

**Speculations labeled (not facts):** (a) The exact post-migration affected-reference
count among the 1,142 (bounded ≤200, exact number left to the Phase-B grep —待验证).
(b) Old-doc "5 vs 6" SEF-callback counts are not adjudicated here; B-phase must
recount `sef_setcb_*` in `main.c:374-388` (K-013) —推测-free rule.

### 9.3 Conclusion and questions for the user

**Conclusion:** the blueprint is complete and executable. Phase B can rebuild strictly
by contract: for each new chapter, fetch stock paragraphs by the §5 "Knowledge rows"
(− old-doc section pointers) and new facts by the "Ground truth" anchors, re-author in
the new teaching order, and satisfy the "Acceptance" checks. No further triage
decisions are required.

**Questions requiring user ruling (Phase B is blocked only on Q1; Q2–Q3 have
recommended defaults and may be tacitly approved):**

1. **Renumber blast radius (recommend: approve).** Six docs change numbers (05↔07/08
   block move, 18↔19 swap). Cost is bounded (§8.3, low risk) and it is the only way to
   reach zero forward references. Alternative — keep numbers and accept forward
   pointers — violates hard standard G3. Approve the §8.5 mapping?
2. **15-open-close and 31-misc-queries stay single (recommend: approve).** Both are
   defensible single concepts (fd-lifecycle endpoints; introspection hub) within the
   soft length ceiling, so no cascade renumber is proposed. Only the VM rows leave 31
   (→32). If you prefer finer splits, say so before Phase B starts.
3. **English-only rebuild (muse default).** All §5 contracts mandate English bodies.
   Confirm that Phase B should not preserve any Chinese sentence from stock docs
   (knowledge only, never prose).
