# 17-stage-net Document Reconstruction Blueprint (muse)

## 0. Metadata

- Executor: muse. Date: 2026-09-19 (UTC). Target directory: `notes/rewrite/fork-syscall-rewrite/17-stage-net/`. Repo root: `/home/xzhao/github/minix-rs`.
- Current commit at inspection time: `873ac6fe5` (`git log --oneline -3` shows `873ac6fe5`, `2621513b8`, `9cb608cc0`; the working tree also contains unrelated uncommitted modifications under `os/libs/minix-types/`, `os/servers/mib/`, `os/servers/vfs/` — all outside this stage and untouched by this report).
- Task: R-phase reconstruction blueprint. Output: this file only (`target_dir/doc_rerank_muse.md`). No body text of any existing document was modified, renamed, moved, or deleted.
- Constraints observed: `.design/` and `tmp_design_and_todo/` were never read nor cited; no other AI's `doc_rerank_*` product was opened or copied. Disclosure: while running a workspace-wide filename/keyword grep for `minix-sockdriver`, two lines of another blueprint's filename-bearing output scrolled through the tool result. Those lines were not opened, not followed, and none of their judgments are used here. Every conclusion below is derived from first-hand `rg`/`sed`/`wc`/`ls` evidence quoted with anchors. All output prose in this report is English, per the muse special requirement.
- Scope. In scope (numbered documents + global concepts): `00-net-overview.md`, `01-sockdriver-framework.md` through `25-smoltcp-shim.md` (25 files), `99-net-global-concepts.md` — 27 files total. Reference material (read, not rebuilt): `plan.md`, `todo.md`, `draft/README.md`, `archive/todo-N1-archive-2026-09-17.md`. Out of scope: other stages' documents (used only for boundary checks), other AIs' `doc_rerank_*` files (not read), `.design/` + `tmp_design_and_todo/` (not referenced, per project convention).
- Reading list actually consumed:
  1. All 27 in-scope documents: full header declarations (`> **分类** / 源码 / Rust 模块 / 前置 / 不讲什么 / 只回答一个问题`) for every file, plus full-body reads of `00`, `01`, `02`, `03`, `99`, `25`, and targeted section reads of `04`–`24` for the knowledge pool.
  2. Minix3 C sources in full file-list form: `minix3/minix/net/lwip/` (27 `.c`), `minix3/minix/net/uds/` (3 `.c`), `minix3/minix/lib/libsockdriver/sockdriver.c`, `minix3/minix/lib/libsockevent/` (2 `.c`), `minix3/minix/lib/liblwip/` (`lib/` glue + `patches/` + `dist/src/` subset), `minix3/minix/lib/libc/sys/` socket family (16 `.c`), plus headers `com.h`, `ipc.h`, `sockdriver.h`, `sockevent.h`, `lwip.h`, `uds.h`, `ifdev.h`, `netdriver.h`, `minix/if.h`, `rmib.h`.
  3. Rust entries: `os/net/lwip/src/` (24 files), `os/net/uds/src/` (5 files), `os/libs/minix-sockdriver/src/` (3 files), `os/libs/minix-netdriver/src/` (7 files), `os/libs/minix-sys/src/socket.rs`.
  4. Boundary materials: `notes/rewrite/fork-syscall-rewrite/00-master-plan/README.md` (stage table, line 35), `notes/rewrite/fork-syscall-rewrite/edge_todo.md` (E-SDEVOWN, E-DEVWIRE, E-NETSTART, E-RMIBWIRE), `17-stage-net/plan.md`, `17-stage-net/todo.md`.
  5. Previous stage overview: `16-stage-drivers/00-drivers-overview.md` (consumes NDEV driver side; net is its consumer).
  6. Style example: `01-stage-kernel/06-todo.md` (818 lines) — used only as a format model for per-document contracts (what / not-what / handed-to / acceptance), none of its content is reused.
- Commands used and key outputs (evidence excerpts; full outputs are reproducible by re-running):
  - `wc -l notes/rewrite/fork-syscall-rewrite/17-stage-net/*.md` → 27 in-scope docs, 116–256 lines each (total ~4400 lines excluding other-AI blueprints); `plan.md` 460 lines; `todo.md` 78 lines.
  - `wc -l minix3/minix/net/lwip/*.c minix3/minix/net/uds/*.c` → 27 lwIP-server files + 3 UDS files, 27883 lines combined (see §1 for the per-file table).
  - `rg -n "main|startup|^init|alloc_socket|sef_" minix3/minix/net/lwip/lwip.c` → `alloc_socket` :152, `init` :196, `startup` :270, `sef_startup()` :287, `main` :294.
  - `sed -n '196,382p' minix3/minix/net/lwip/lwip.c` → full 17-step `init()` chain + 4-way `main()` dispatch (transcribed in §1).
  - `sed -n '1303,1420p' minix3/minix/net/uds/uds.c` → 5-step `uds_init()` + 2-way `main()` loop (transcribed in §1).
  - `rg -n "sockdriver_task|sockdriver_process|sockdriver_terminate|sockdriver_announce" minix3/minix/lib/libsockdriver/sockdriver.c` → :47 / :1061 / :1120 / :1132; `sed -n '1,30p'` → 17-row may-suspend table (8 yes / 8 no / CANCEL n/a / SELECT special).
  - `rg -n "SOCKID_" minix3/minix/net/lwip/lwip.h` → five bases `0x0/0x00100000/0x00200000/0x00400000/0x00800000` (:58–62).
  - `rg -n "SDEV_RQ_BASE|SDEV_RS_BASE|IS_SDEV" minix3/minix/include/minix/com.h` → `0x1900` / `0x1980` / mask `~0x7f` (:1037–1041).
  - `rg -n "NDEV_RQ_BASE|NDEV_RS_BASE" minix3/minix/include/minix/com.h` → `0x1A00` / `0x1A80` (:1085–1086).
  - `rg -n "sockhash_slot|% 256|>> 16" minix3/minix/lib/libsockevent/sockevent.c` → `(id + (id >> 16)) % SOCKHASH_SLOTS` (:48–57).
  - `sed -n '54,100p' minix3/minix/include/minix/sockevent.h` → 21-callback `sockevent_ops` (`sop_pair`…`sop_free`); `sed -n '1,26p'` → `SEV_* 0x01–0x20`, `SFL_* 0x01–0x10`.
  - `cat minix3/minix/net/lwip/lnksock.c` → `NR_LNKSOCK 4`; `lnksock_socket` accepts only `SOCK_DGRAM`+protocol 0; `lnksock_ops = { .sop_ioctl = ifconf_ioctl, .sop_free = lnksock_free }` (only 2 of 21 callbacks populated).
  - `rg -n "NR_IPV4_MCAST_GROUP|NR_IPV6_MCAST_GROUP" minix3/minix/lib/liblwip/lib/lwipopts.h` → 64 + 64 (:211/:538); `rg -n "MAX_GROUPS_PER_SOCKET" minix3/minix/net/lwip/mcast.c` → 8 (:41).
  - `rg -n "NO_SYS|PBUF_POOL_SIZE|TCP_MSS |TCP_WND|TCP_SND_BUF" minix3/minix/lib/liblwip/lib/lwipopts.h` → 1 / 0 / 1460 / 16384 / 11*MSS.
  - `rg -n "NR_UDSSOCK|UDSHASH" minix3/minix/net/uds/uds.h` → 256 / 64 slots; `rg -n "UDS_CTL_MAX" minix3/minix/net/uds/uds.h` → 4096.
  - `rg -n "SOCKADDR_MAX" minix3/minix/include/minix/sockdriver.h` → `UINT8_MAX + 1` (256).
  - `rg -n "MEMPOOL_DEFAULT_MAX_SLABS" minix3/minix/net/lwip/mempool.c` → 64 (~17 MB, :238).
  - `cat minix3/minix/net/lwip/lwip.conf minix3/minix/net/uds/uds.conf`; `rg -n "up lwip|up uds" minix3/etc/usr/rc` → :259 / :286; `ls minix3/etc/rs.lwip minix3/minix/net/uds/unix.8` → both exist.
  - `ls os/libs/minix-sockdriver/src/ os/libs/minix-netdriver/src/ os/net/lwip/src/ os/net/uds/src/` → 3 / 7 / 24 / 5 files (see §1).
  - `rg -c "#\[test\]" os/net/lwip/src/ os/net/uds/src/ os/libs/minix-netdriver/src/` → per-file test-function counts (see §3; total ≈ 150 test fns).
  - `rg -n "前置" …/06-lwip-ipsock.md` → doc 06 declares doc 07 as prerequisite (forward reference, §3).
  - `rg -n "Rust 模块" …/01-…md …/02-…md` → doc 01 points at `minix-sockdriver/src/sdev.rs` (correct); doc 02 points at `minix-netdriver/src/sockevent.rs` (stale — file moved, §3).

## 1. C True Order (runtime true order, rebuilt from C source, not paraphrased)

Stage-type determination: this stage is a **dual service-event-loop stage with shared framework libraries, plus a syscall-ABI collection and a third-party stack**. It is NOT a linear boot chain (unlike stage 01): the two servers are loaded independently at RS runtime and never call each other. The correct "true order" therefore has three strands: (A) lwIP server birth + loop, (B) UDS server birth + loop, (C) shared-framework call order (sockdriver → sockevent → protocol modules). Strand C is a dependency order, not a temporal one: framework code runs only when a server calls into it.

### 1.1 Strand A — lwIP server (event-loop type: startup segment + loop segment)

| Step | Action | Anchor | Notes |
|------|--------|--------|-------|
| A-01 | RS loads the service at runtime (`up lwip -dev /dev/bpf -script /etc/rs.lwip`) | `minix3/etc/usr/rc:259`; `minix3/etc/rs.lwip` (restart-recovery script) | Not a boot-image service; waits for `minix.lwip.drivers.pending` (`rc:283`). Service manifest `minix3/minix/net/lwip/lwip.conf` declares domains INET/INET6/ROUTE/LINK + ipc peers SYSTEM/vfs/rs/vm/mib + `system KILL` (SIGPIPE). |
| A-02 | `main()` → `startup()` registers SEF callbacks, then `sef_startup()` | `minix3/minix/net/lwip/lwip.c:294` (`main`), `:270` (`startup`), `:273` (`sef_setcb_init_fresh(init)`), `:287` (`sef_startup()`) | Stateless restart by design: no `_restart` callback (`lwip.c:275-278` comment); live-update and immediate-shutdown are open TODOs in the same comment. |
| A-03 | `init()`: seed RNG | `lwip.c:196` (`init`), `srand48(clock_time(NULL))` | Feeds `lwip_hook_rand()` (`lwip.c:127`). |
| A-04 | `init()`: bring up the third-party library | `lwip_init()` (`lwip.c`, init body) | Stack本体 entry; behavior contract owned by N25. |
| A-05 | `init()`: init event library with the domain dispatcher | `sockevent_init(alloc_socket)` (`lwip.c`, init body) | `alloc_socket` (`lwip.c:152`) dispatches PF_INET/INET6→tcp/udp/raw constructors (:163–172), PF_ROUTE→rtsock, PF_LINK→lnksock, else unsupported (:186). |
| A-06 | `init()`: helper modules | `mempool_init(); tcpisn_init(); mcast_init();` (init body) | Resource / secret / membership foundations. |
| A-07 | `init()`: high-level socket modules | `ipsock_init(); tcpsock_init(); udpsock_init(); rawsock_init();` (init body) | Registration order: common → TCP → UDP → RAW. |
| A-08 | `init()`: interface modules | `ifdev_init(); loopif_init(); ethif_init();` (init body) | Object model → loopback instance → ethernet instance. |
| A-09 | `init()`: NIC-driver consumer | `ndev_init();` (init body) | Max 8 slots; up/down tracked via DS notify (see A-13). |
| A-10 | `init()`: low-level socket modules | `rtsock_init(); lnksock_init();` (init body) | Route socket + link socket (minimal, 2-callback ops). |
| A-11 | `init()`: routing, filter device, MIB capstone | `route_init(); bpfdev_init(); mibtree_init();` (init body) | `mibtree.c:56-66` registers inet/inet6/lwip subtrees via `rmib_register` — AFTER all modules registered theirs (comment in init body). |
| A-12 | `init()`: default configuration (loopback) + master timer | `ifconf_init(); init_timer(&lwip_timer); recheck_timer = TRUE; running = TRUE;` (init body) | `sys_now()` (`lwip.c:24`) = `getticks*1000/sys_hz`; `check_lwip_timer()` polls because lwIP never announces timers. |
| A-13 | `main()` loop, per iteration: `ifdev_poll()` → `check_lwip_timer()` → `sef_receive_status(ANY)` | `lwip.c:303-321` | Loopback queue is drained by polling, not by interrupt. |
| A-14 | Loop dispatch 1 (notify): CLOCK → `expire_timers()`; DS_PROC_NR → `ndev_check()`; else logged+dropped | `lwip.c:324-342` | Driver up/down enters here. |
| A-15 | Loop dispatch 2 (MIB): `rmib_process()` | `lwip.c:345-348` | Remote-MIB subtree queries (edge E-RMIBWIRE on the Rust side). |
| A-16 | Loop dispatch 3 (VFS): `IS_SDEV_RQ` → `sockevent_process()`; `IS_CDEV_RQ/BDEV_RQ` → `bpfdev_process()` | `lwip.c:350-363` | One endpoint (VFS), two device families. |
| A-17 | Loop dispatch 4 (NIC driver): `IS_NDEV_RS` → `ndev_process()`; anything else logged+dropped | `lwip.c:365-377` | Consumer side of the NDEV protocol (`com.h:1085-1144`). |

### 1.2 Strand B — UDS server (event-loop type, smaller and isomorphic)

| Step | Action | Anchor | Notes |
|------|--------|--------|-------|
| B-01 | RS loads the service (`up uds`) | `minix3/etc/usr/rc:286`; manifest `minix3/minix/net/uds/uds.conf` (domain LOCAL, uid 0 for socketpath/copyfd, ipc SYSTEM/vfs/rs/vm/mib) | Starts after lwIP; no ordering dependency between the two servers. |
| B-02 | `main()` → `uds_startup()` → `sef_startup()` | `minix3/minix/net/uds/uds.c:1384` (`main`), `:1367` (`uds_startup`), `:1371` (`sef_setcb_init_fresh(uds_init)`), `:1374` (signal handler), `:1377` (`sef_startup()`) | |
| B-03 | `uds_init()`: free-list over `uds_array[NR_UDSSOCK]` → `udshash_init()` (64 slots) → `uds_io_init()` → `uds_stat_init()` → `sockevent_init(uds_socket)` | `uds.c:1303-1346`; `uds.h:15` (NR_UDSSOCK=256), `uds.h:18` (UDSHASH_SLOTS=64); `io.c:102` (`uds_io_init`); `stat.c:163` (`uds_stat_init`) | Same framework, different constructor callback than strand A. |
| B-04 | `main()` loop `while (uds_running \|\| uds_in_use > 0)`: `sef_receive_status(ANY)` → MIB? `rmib_process()` : `sockevent_process()` | `uds.c:1384-1414` | Two-way dispatch (vs lwIP's four-way). |
| B-05 | SIGTERM → `uds_signal()`: `uds_running=FALSE`, `sef_cancel()` once drained; exit path `uds_cleanup()` → `uds_stat_cleanup()` | `uds.c:1349-1365`, `uds.c:1338-1346` | Graceful-drain shutdown (contrast lwIP's never-shutdown TODO). |

### 1.3 Strand C — shared-framework call order (dependency order)

| Step | Action | Anchor | Notes |
|------|--------|--------|-------|
| C-01 | Server entry `sockdriver_task()` → per-message `sockdriver_process()` → `sockdriver_terminate()`; `sockdriver_announce()` at birth | `minix3/minix/lib/libsockdriver/sockdriver.c:1132` / `:1061` / `:1120` / `:47` | The `struct sockdriver` callback table (`sockdriver.h`) carries 19 `sdr_*` entries (`sdr_socket`…`sdr_other`, verified by `rg -c` + header read at :83–130). |
| C-02 | May-suspend table gates every request: 8 yes (BIND/CONNECT/ACCEPT/SEND/RECV/IOCTL/CLOSE/SELECT-special), 8 no, CANCEL n/a | `sockdriver.c:8-26` | Layout prefixes `m_vfs_lsockdriver_*` / `m_lsockdriver_vfs_*` (comment :28+). |
| C-03 | Copy/grant helpers: `sockdriver_copyin/out`, `vcopyin/out`, `copyin_opt/copyout_opt`, `pack/unpack_data` | `sockdriver.c` (7-function family) + `sockdriver.h:172` lines | Direction + packing rules, no object semantics. |
| C-04 | `sockevent_init(constructor)` → per-message `sockevent_process()`; objects in 256-slot hash, timers, select, suspend/resume continuations | `sockevent.h:101-109` (API), `sockevent.c:48-57` (hash), `sockevent_proc.c` (52-line continuation pool) | 21-callback `sockevent_ops` (`sockevent.h:54-100`); `SEV_*`/`SFL_*` bit families (`sockevent.h:7-19`). |
| C-05 | UDS object model on top of the framework: 5-state machine (Unconnected/Listening/Connecting/Connected/Disconnected) + limbo sockets + LOCAL_CONNWAIT | `uds.h:86-135` (state machine essay) | Connection semantics live here, not in the framework. |

### 1.4 Rust implementation strand (not order-authoritative; existence check only)

`os/net/lwip/src/` (24 files: `main.rs`+`server.rs`+`startup.rs` loop skeleton, one module per C file, plus `lwip_port.rs` wall + `stack.rs` smoltcp shim); `os/net/uds/src/` (5 files: `main.rs`+`server.rs`+`core.rs`+`io.rs`+`lib.rs`); `os/libs/minix-sockdriver/src/` (3 files: `sdev.rs` 710 lines + `sockevent.rs` 110 lines — E-SDEVOWN single-source vocabulary, commit `907e65f79`); `os/libs/minix-netdriver/src/` (7 files: `sockid.rs`, `socktable.rs`, `service.rs` classifier, `protocol.rs` NDEV, `driver.rs`, `portio.rs`); `os/libs/minix-sys/src/socket.rs` (ABI call list + fallback predicate). Both service crates depend on both framework crates (`os/net/lwip/Cargo.toml:18-19`, `os/net/uds/Cargo.toml:16-17`). `rg -c "#[test]"` totals ≈ 150 test functions across the five crates (see §3).

## 2. Knowledge Pool (deduplicated, stage-wide)

Granularity decision (stated so §9-G2/G5 are checkable): one entry per independently testable semantic unit (constant family, predicate, state machine, init step-group, dispatch arm, lifecycle rule). Per-function exhaustiveness is delegated to each new document's contract (§5 "fact baseline" + acceptance criteria), which must enumerate every C symbol of its files. Entries marked 存量 come from existing docs; 新增 come from C/non-C/Rust sources with evidence anchors. Duplicates merged; the main telling point is recorded.

### 2.1 Framework (Strand C)

| ID | Name | Type | Src | Existing location(s) | Anchor | Reader payoff (question answered) |
|----|------|------|-----|----------------------|--------|-----------------------------------|
| K-001 | SDEV 17-request vocabulary + 6-reply vocabulary + `~0x7f` guard | interface/protocol | 存量 | 01 §2; 99 §1.1 | `com.h:1037-1078` | Which window does a VFS request enter? |
| K-002 | May-suspend table (8 yes / 8 no / CANCEL n/a / SELECT two-phase) | constraint | 存量 | 01 §1.2; 99 §1.1 | `sockdriver.c:8-26` | Which requests may sleep, which must answer immediately? |
| K-003 | `sockdriver_task/process/terminate/announce` entry shape | mechanism | 存量 | 01 (plan §5.2) | `sockdriver.c:47,1061,1120,1132` | Where does a socket driver's `main` live? |
| K-004 | 19-callback `sdr_*` table | interface | 存量 | 01 (plan §5.2) | `sockdriver.h:83-130` | What must a socket server implement? |
| K-005 | Copy/grant helper family (copyin/out, vcopy, opt, pack/unpack) | mechanism | 存量 | 01 (plan §5.2) | `sockdriver.c` + `sockdriver.h` | How do bytes cross the VFS↔server boundary? |
| K-006 | `sockid_t` negative-error channel + 5 class bases + 20-bit index field | datastructure | 存量 | 01 §5.2; 99 §1.2 | `lwip.h:58-62`; `tcpsock.c:tcpsock_get_id` et al.; `uds.c:101` (bare-index cast) | How is a socket named, and how do errors travel in the same word? |
| K-007 | sockevent object model (opaque `struct sock`, hash 256, timer chain, pending-event queue) | datastructure | 存量 | 02 §1.1-1.2 | `sockevent.h:27-52`; `sockevent.c:48-57` | Where does a half-finished request live? |
| K-008 | 21-callback `sockevent_ops` + `SEV_*`/`SFL_*` bits + never-retest-error rule | interface/constraint | 存量 | 02; 99 §1.1 | `sockevent.h:7-19,54-100`; `sockevent.c:817` (`sockevent_fire`) | How is a suspended call resumed, and which bits may never be re-tested? |
| K-009 | Continuation pool (`sockevent_proc`, 52 lines) | mechanism | 存量 | 02 (plan §5.2) | `libsockevent/sockevent_proc.c` | What runs when the waited-for event fires? |
| K-010 | E-SDEVOWN crate split: vocabulary (`minix-sockdriver`: sdev+sockevent) vs object/event-pump (`minix-netdriver`: socktable+service) | arch-evolution | 新增 | (Rust layout; doc 01 migrated, 02 stale — §3 D2) | `os/libs/minix-sockdriver/src/{sdev,sockevent}.rs`; `os/libs/minix-netdriver/src/{socktable,service}.rs`; commit `907e65f79` | Which crate owns which half of the framework? |

### 2.2 Skeleton + resources (Strand A-02–A-06, A-12)

| ID | Name | Type | Src | Existing location(s) | Anchor | Reader payoff |
|----|------|------|-----|----------------------|--------|---------------|
| K-011 | lwIP 17-step `init()` chain + SEF registration + stateless-restart stance | mechanism | 存量 | 03 §1.1; 00 Ch2 | `lwip.c:196-267,270-287` | What are the 13+4 birth steps, in order? |
| K-012 | `main()` loop: poll → timer-check → receive → 4-way dispatch | mechanism | 存量 | 03 §1.2; 00 Ch2 | `lwip.c:294-321,324-377` | Where does each incoming message go? |
| K-013 | `alloc_socket` domain dispatch (INET/INET6/ROUTE/LINK + root gate for RAW) | mechanism | 存量 | 03 (plan §5.2); 10 §1.2 | `lwip.c:152-194` | How is a fresh socket routed to its family? |
| K-014 | `mibtree_init` capstone: three-tree registration, local-fail panics / remote-fail silent | mechanism | 存量 | 03 §1.5 | `mibtree.c:56-66` | When is the management tree complete? |
| K-015 | `sys_now` / tick↔timeval conversion with round-up + overflow guard | mechanism | 存量 | 05 §1.3 | `util.c`; `lwip.c:24` | How does stack time become kernel ticks? |
| K-016 | Custom 512-byte-slice pool (PBUF_POOL_SIZE=0), slab growth to 64 (~17 MB), cur/max gauges | datastructure | 存量 | 04 §1.1/1.2/1.5; 99 §1.5 | `lwipopts.h:49,80`; `mempool.c:38-39,118,234-238,462` | How big is a buffer, and what happens when the pool is empty? |
| K-017 | pchain tools (tail-find, 512-round-up estimate, first-block-512 rule) | mechanism | 存量 | 04 §1.3 | `pchain.c` (154 lines) | How are buffer chains measured without moving them? |
| K-018 | Exhaustion mapping per consumer (UDP/RAW ENOBUFS, BPF ENOMEM, ifdev stop-receive) | constraint | 存量 | 04 §1.4 | `udpsock.c`, `rawsock.c`, `bpfdev.c`, `ifdev.c` call sites | Pool empty: who reports what? |

### 2.3 Stateless helpers (pure functions)

| ID | Name | Type | Src | Existing location(s) | Anchor | Reader payoff |
|----|------|------|-----|----------------------|--------|---------------|
| K-019 | `err_t`→errno bijection (16 arms + logging fallback) | interface | 存量 | 05 §1.1; 99 §1.4 | `util.c:util_convert_err`; `os/net/lwip/src/util.rs` (17-arm table) | What did the stack mean by that negative number? |
| K-020 | Root check as pure comparison (uid==0), queried in service layer | mechanism | 存量 | 05 §1.2; 10 §1.2 | `util.c:util_is_root` call sites | Who may open RAW sockets / privileged ports? |
| K-021 | sockaddr validation (exact-length, family match, zone rules, multicast vetoes, mask contiguity) | constraint | 存量 | 05 §1.4 | `addr.c` (699 lines) | When is a user-supplied address rejected? |
| K-022 | `SOCKADDR_MAX`=256 + `union sockaddr_any` + static assert family | datastructure | 存量 | 05 (header); 99 §1.x | `sockdriver.h:11-18`; `lwip.h:26-47` | How large may any address be? |
| K-023 | RFC 6724 source-selection policy table (label/scope/sort) | mechanism | 存量 | 05 §1.4+; 06 §1.3-1.4 | `addrpol.c` (143 lines) | With several local addresses, which source wins? |
| K-024 | `util_copy_data` / `util_pcblist` split: pure compute vs service-side storage+messaging | boundary | 存量 | 05 (header exclusion) | `util.c` | Which half of a helper belongs in a library? |

### 2.4 Common layers + protocol sockets (A-07)

| ID | Name | Type | Src | Existing location(s) | Anchor | Reader payoff |
|----|------|------|-----|----------------------|--------|---------------|
| K-025 | ipsock common creation (3 address kinds from 2 flag bits; no-resource rule; clone inherits) | mechanism | 存量 | 06 §1.1-1.2 | `ipsock.c`; `ipsock.h:29-33` | What do TCP/UDP/RAW share at birth? |
| K-026 | Source/destination validation chains (multicast, mapped, wildcard, zone, privileged-port) | constraint | 存量 | 06 §1.3-1.5 | `ipsock.c` | Which address checks run before connect/bind/send? |
| K-027 | Two-tier option checking (socket-level vs protocol-level) | constraint | 存量 | 06 §1.6 | `ipsock.c` | Where is a setsockopt validated? |
| K-028 | pktsock shared queue: byte-count gate (not packet-count), 3-bit header flags, create-forwarding layout rule | mechanism | 存量 | 07 §1.1-1.4 | `pktsock.c:59,106`; `pktsock.c:38-40` | When is an arriving datagram admitted or dropped? |
| K-029 | Default buffer tables (UDP 8192/32768; RAW 65535/32768; floor 512; ceiling 65536) | constraint | 存量 | 07 §1.3; 09 §1.2; 10 §1.5 | `udpsock.c:27-34,138-139`; `rawsock.c:48-55,310-311` | What are the default send/receive sizes? |
| K-030 | TCP 5 progress flags + buffer tiers (1 / 32768 / 131072; RX floor = WND 16384) + 3/4-full cautious mark | datastructure | 存量 | 08 §1.1-1.2 | `tcpsock.c:87-90`; `ipsock.h:29-33` | How far has this connection progressed? |
| K-031 | EPIPE reporting by transport, SIGPIPE delivery by VFS (division of labor) | boundary | 存量 | 08 §1.3 | `tcpsock.c` (no raise site — negative evidence, must be re-verified in B) | Who reports vs who signals a broken pipe? |
| K-032 | Documented absences: delayed-ACK refused, MSS read-only, listen-state options EINVAL | constraint | 存量 | 08 §1.4 | `tcpsock.c` comments | Which features must the reader NOT go looking for? |
| K-033 | ISN = SHA256(4-tuple, 16-byte key)[0:32] + 4µs clock; key via hidden sysctl node, boot-time pseudo-key warning | mechanism | 存量 | 08 §1.5 | `tcpisn.c` (203 lines) | How unpredictable is the next sequence number? |
| K-034 | UDP create (protocol 0/17 only, UDP-Lite refused) + TTL=1 / loop-on defaults + two-stage length check | mechanism | 存量 | 09 §1.1/1.2/1.4 | `udpsock.c:116,128,138-139,279` | What does a connectionless socket check? |
| K-035 | RAW full 0–255 protocol range + allocator-side root gate + HDRINCL + forced IPv6-ICMPv6 checksum | constraint | 存量 | 10 §1.1-1.4 | `rawsock.c:290,310-311,473`; `lwip.c:152-194` (gate) | Why is RAW the most capable and most gated family? |
| K-036 | AF_LINK minimal socket (SOCK_DGRAM+0 only, NR_LNKSOCK=4) + link-address length formula (16+nlen+hlen) | mechanism | 存量 | 11 §1.2-1.3 | `lnksock.c:41-77`; `lwip.h:26-47` | What is the narrow configuration handle? |
| K-037 | lldata ARP/ND cache layer, deliberately separate from IP routing (BSD-8 lineage) | boundary | 存量 | 11 §1.4 | `lldata.c` (584 lines) | Where do link-layer resolutions live? |
| K-038 | Multicast two-level caps (64+64=128 global, 8 per socket) + join/leave/check order + ifdown sweep | constraint | 存量 | 12 §1.1-1.4 | `mcast.c:36-47,143`; `lwipopts.h:211,538` | How is a shared group rationed and cleaned? |

### 2.5 Interface plane (A-08, A-09, A-12)

| ID | Name | Type | Src | Existing location(s) | Anchor | Reader payoff |
|----|------|------|-----|----------------------|--------|---------------|
| K-039 | NDEV consumer: 8 slots, queue-depth guarantees, active test, conf/send/recv rules, DS up/down tracking | mechanism | 存量 | 13 (all §1) | `ndev.c` (1019 lines); `ndev.h` (33 lines) | As a NIC driver client, what does the stack guarantee? |
| K-040 | ifdev object + 16-callback `ifdev_ops` + 3-entry HW list (VALID/FACTORY) + loopif MTU/count bounds | datastructure | 存量 | 14 (all §1) | `ifdev.c`+`ifdev.h:28-48`; `ifdev.h:20,56-57`; `loopif.c` (420 lines) | What shape fits every interface? |
| K-041 | ethif medium bounds (MTU 1500, mcast cap, send-reserve 8) + ndev binding | constraint | 存量 | 15 (all §1) | `ethif.c` (1718 lines) | What does the ethernet instance add? |
| K-042 | ifaddr IPv4/IPv6 lists + v6 flag bits + preference order + field-ownership boundary vs ifdev | datastructure | 存量 | 16 (all §1) | `ifaddr.c` (2224 lines); `ifaddr.h:5-7` | Which address fields belong to whom? |
| K-043 | ifconf default loopback + 8-family ioctl dispatch + MINIX extensions (IFMEDIA/IFGCLONERS) | mechanism | 存量 | 17 (all §1) | `ifconf.c:866` (`ifconf_ioctl`); `minix/if.h:39-49` | How is the first interface born, and where do ioctls land? |
| K-044 | BPF device: buffer tiers (32/32768/262144), 512-instruction cap, version gate, single-consumer assumption; NetBSD filter port (mbuf→pbuf) | mechanism | 存量 | 18 (all §1) | `bpfdev.c` (1365 lines); `bpf_filter.c:149` (`bpf_filter_ext`, 561 lines) | How is traffic observed without disturbing forwarding? |

### 2.6 Routing (A-10, A-11)

| ID | Name | Type | Src | Existing location(s) | Anchor | Reader payoff |
|----|------|------|-----|----------------------|--------|---------------|
| K-045 | rttree radix tree with prefix-length-only mask assumption + bit-index math | datastructure | 存量 | 19 (all §1) | `rttree.c:5-8` (assumption), 744 lines | How are prefixes stored? |
| K-046 | route overrides of lwIP `ip4/ip6_route` (weak symbols) + gateway hooks + entry management | mechanism | 存量 | 19 (all §1) | `route.c:7-8` (hooks); `lwiphooks.h` | Where does Minix replace stack routing? |
| K-047 | rtsock `rt_msghdr`/RTA compress-expand, version=4 check, TX≤512 / RX∈[0,65536], no-leak isolation rule | interface | 存量 | 20 (all §1) | `rtsock.c:311` (`rtsock_socket`), 1912 lines | How do user programs read/write the table? |

### 2.7 UDS (Strand B)

| ID | Name | Type | Src | Existing location(s) | Anchor | Reader payoff |
|----|------|------|-----|----------------------|--------|---------------|
| K-048 | UDS core: 256 objects, 5 states + limbo, 64-slot file→socket hash, LOCAL_CONNWAIT two behaviors, drain-exit rule | mechanism | 存量 | 21 (all §1) | `uds.c:5,101,120-123,1303-1346,1384-1414`; `uds.h:15,18,86-135` | How do local sockets connect without a network? |
| K-049 | UDS stat surface (`net.local.*` MIB + `kinfo_pcb`) | interface | 存量 | 21 (plan §5.1) | `stat.c:11,163,186` lines | What can management queries see? |
| K-050 | UDS data plane: 32 KB ring, 4 segment kinds, ancillary/FD-passing with UDS_CTL_MAX=4096, STREAM/SEQPACKET/DGRAM edges | mechanism | 存量 | 22 (all §1) | `io.c:70-95,102,122,148,205-249` (1803 lines); `uds.h:33-36` | How do bytes, FDs, and credentials travel locally? |

### 2.8 ABI + third-party + global

| ID | Name | Type | Src | Existing location(s) | Anchor | Reader payoff |
|----|------|------|-----|----------------------|--------|---------------|
| K-051 | libc 16-call socket family: VFS_SOCKET fast path + legacy device fallback on EAFNOSUPPORT/ENOSYS ([ARCH] N-2) | interface/arch | 存量 | 23 (all §1) | `libc/sys/{socket,socketpair,bind,connect,listen,accept,sendto,recvfrom,sendmsg,recvmsg,setsockopt,getsockopt,getsockname,getpeername,shutdown,loadname}.c` (16 files) | How does a user call reach the server? |
| K-052 | liblwip compile subset (68 files / 58232 lines) + glue (`lwipopts.h`, `lwiphooks.h` 4 hooks, `arch/cc.h`) + 4 patches | tool/artifact | 存量 | 24 §1; plan §5.1 | `lib/liblwip/dist/src/`; `lib/liblwip/lib/`; `lib/liblwip/patches/0001-0004` | What third-party code is compiled in, and what glues it? |
| K-053 | [ARCH] N-1 selection: smoltcp family + in-house semantic shim, wall-behind-FFI-fallback; option values as behavior-contract constants | arch-evolution | 存量 | 24 §1.5; 25 (all); todo N1-P1-3 | `os/net/lwip/src/{lwip_port.rs,stack.rs}`; commits `2621513b8`, `9cb608cc0` | What replaces lwIP, and what numbers must never drift? |
| K-054 | Shim mapping boundary: 7 boundaries, timer synthesis, phasing; route/link tables stay outside smoltcp | arch-evolution | 存量 | 25 (all) | `os/net/lwip/src/stack.rs` (`Stack` impl) | Where exactly is the Minix↔smoltcp line drawn? |
| K-055 | Global constant families (SDEV/NDEV/sockid/errno-map) + endpoint convention + 4-neighbor boundary list | concept | 存量 | 99 (all); 00 Ch3-4 | `com.h`; `ipc.h`; stage plans of 05/10/16/18 | Where is every shared number's single home? |

Statistics: 55 entries — 51 存量 / 4 新增 (K-010, and K-053/K-054 partially新增 as Rust-side decisions; K-024 boundary clarification). By type: concept 1, mechanism 24, datastructure 7, interface/protocol 9, constraint 10, boundary 2, arch-evolution 3, tool/artifact 1 (some entries dual-typed; primary type shown). By existing doc: 00:2, 01:6, 02:4, 03:4, 04:3, 05:6, 06:3, 07:2, 08:4, 09:1, 10:1, 11:2, 12:1, 13–20:1 each, 21:2, 22:1, 23:1, 24:2, 25:1, 99:1. No orphan entries; every in-scope doc contributes ≥1 entry.

## 3. Coverage Audit (leak-proofing, duplication-proofing, boundary-proofing)

### 3.1 Topic universe (four sources)

- C symbols: all functions/structs/macros/constants/state machines/error paths of the 27+3+1+2+16 C files above, plus `USE_INET6` conditional branches (`net/lwip/Makefile`), `SOCKHASH_SLOTS`, `NR_LNKSOCK`, `NR_TCPSOCK`/`NR_UDPSOCK`-class per-family caps, weak-symbol route overrides, `SOCKEVENT_EOF` pseudo-value.
- OS-generic concepts: connection state machines, address spaces of socket namespaces, privilege model (root gates), buffer rationing, timer integration, source selection (RFC 6724), multicast rationing, graceful drain vs stateless restart.
- Non-C artifacts (each answered; see §3.5).
- Boundary contracts: master-plan stage table (`00-master-plan/README.md:35`), `edge_todo.md` E-SDEVOWN/E-DEVWIRE/E-NETSTART/E-RMIBWIRE, `plan.md` §5.3 exclusion table, `16-stage-drivers` NDEV driver side, `05-stage-vfs` SDEV client side, `10-stage-mib` rmib server side, `18-stage-commands` consumers.

### 3.2 C-file → document check (every file has a home)

lwip server 27/27: `lwip.c`+`mibtree.c`→N03; `mempool.c`+`pchain.c`→N04; `util.c`→N05; `addr.c`+`addrpol.c`→N06; `ipsock.c`→N07; `pktsock.c`→N08; `tcpsock.c`+`tcpisn.c`→N09; `udpsock.c`→N10; `rawsock.c`→N11; `lldata.c`→N12; `lnksock.c`→N18 (merged, §4 reason L-1); `mcast.c`→N13; `ndev.c`→N14; `ifdev.c`+`loopif.c`→N15; `ethif.c`→N16; `ifaddr.c`→N17; `ifconf.c`→N18; `bpfdev.c`+`bpf_filter.c`→N19; `rttree.c`+`route.c`→N20; `rtsock.c`→N21.
UDS 3/3: `uds.c`+`stat.c`→N22; `io.c`→N23.
Frameworks: `sockdriver.c`+`sockdriver.h`→N01; `sockevent.c`+`sockevent_proc.c`+`sockevent.h`→N02.
Third-party: `liblwip/dist/src` subset + `lib/lwipopts.h`+`lwiphooks.h`+`arch/cc.h` + 4 patches→N25.
ABI: 16 `libc/sys` socket files→N24. NOTE (stale-figure correction): `plan.md` §1.1/§5.1 says "15 files / 3173 lines"; `ls` shows 16 socket-family `.c` files (the 15 listed plus `loadname.c` helper; `rename.c` is a false grep hit). B phase must recount with `wc` and correct the figure — recorded as finding F-1, not a blocker for the blueprint.
Headers: `com.h` SDEV/NDEV ranges→N01/N14/N99; `ipc.h` message layouts→N01/N99; `sockdriver.h`→N01/N99; `sockevent.h`→N02/N99; `lwip.h` sockid+`sockaddr_dlx`→N01/N06/N99; `uds.h`→N22/N23; `ifdev.h`/`ndev.h`/`route.h`/`rtsock.h`/`rttree.h`/`addr.h`/`util.h`/`mcast.h`/`ethif.h`/`lldata.h`/`tcpisn.h`/`bpfdev.h`/`pchain.h`→respective N-docs; `rmib.h`→N03/N21; `netdriver.h`→N14 (+16-stage-drivers); `minix/if.h`→N18; `net/bpf.h`, `net/if.h`, `sys/socket.h`, `sys/un.h`, `netinet/in.h` constant values→N99 (usage semantics stay in N-docs).

### 3.3 Coverage-gap table (topics in the universe, in no current doc)

| # | Gap | Evidence anchor | Disposition → new home |
|---|-----|-----------------|------------------------|
| G-1 | SEF/RS startup handshake + service lifecycle (restart/drain) on the Rust side | `os/net/lwip/src/{main,server,startup}.rs`, `os/net/uds/src/{main,server}.rs`; edge E-NETSTART (`edge_todo.md:1018`) | New §1.x + §4 lifecycle section inside N03 (lwip) and N22 (UDS); cross-stage delivery tracked by E-NETSTART, not a new chapter. No new knowledge-pool entry needed beyond K-011/K-048 lifecycle facets — recorded here so B phase does not drop them. |
| G-2 | `rmib.h` client surface (`rmib_call/node/oldp/newp`, register/process) as a contract | `minix3/minix/include/minix/rmib.h`; `mibtree.c:56-66`; edge E-RMIBWIRE | New §5 wire-contract section inside N03 (registration timing) + N21 (message translation); server side stays in 10-stage-mib. |
| G-3 | NDEV wire single-sourcing (Rust `protocol.rs` vs C `com.h:1085-1144`) | `os/libs/minix-netdriver/src/protocol.rs`; edge E-DEVWIRE (`edge_todo.md:967`) | Wire-value subsection inside N14; single-source migration itself is edge E-DEVWIRE's job, not a new chapter. |
| G-4 | `minix-sys` socket-client bundling (socket.rs inside a 6-stage crate, no feature gate) | `os/libs/minix-sys/src/socket.rs` (146 lines); edge entry (`edge_todo.md:880`) | Boundary note inside N24; extraction is a workspace decision, not a new chapter. |
| G-5 | `unix.8` man page + `rs.lwip` recovery semantics (TCPISN reload, daemon restart list) as documented behavior | `minix3/minix/net/uds/unix.8`; `minix3/etc/rs.lwip`; `rc:265-275` | New §6 behavior subsections inside N22 (man-page contract) and N03 (restart script); both are small (one section each), not new chapters. |

No gap requires a brand-new standalone chapter: every gap lands as a section inside an existing new document. §6 therefore records "no new chapter needed" per item with the same table (see §6).

### 3.4 Duplication table (one topic, several tellings → one main point)

| # | Topic | Current tellings | Main point in the new catalog | Others become |
|---|-------|------------------|-------------------------------|---------------|
| DUP-1 | SDEV vocabulary (bases, guards, flags) | 01 §1.1 + 99 §1.1 + plan §5.1 | N01 (mechanism + tables) | N99 keeps the value index only (pointer, no re-explanation); plan.md is reference, untouched. |
| DUP-2 | sockid bases + hash rule | 01 §5.2 + 02 (hash use) + 99 §1.2 | N01 (namespace + cast rules) | N02 cites the rule (one line); N99 keeps the value index only. |
| DUP-3 | Suspend/continuation semantics | 01 (table) + 02 (objects/continuations) + 21/22 (use) | N01 (table) + N02 (mechanism); single-principle split, not duplication | N22/N23 cite N02 for the mechanism, keep only UDS-specific wait conditions. |
| DUP-4 | Buffer defaults (UDP/RAW tables) | 07 §1.3 + 09 §1.2 + 10 §1.5 | N08 (shared table, single home) | N10/N11 cite N08's table; keep only family-specific deviations. |
| DUP-5 | Multicast caps (64/64/128/8) | 12 §1.1-1.2 + 09 §1.2 (defaults) + plan §4 N-13 | N13 (single home) | N10 cites; N99 indexes the four numbers. |
| DUP-6 | lwIP option contract (NO_SYS/MSS/WND/SND/POOL) | 24 §1 + 04 §1.5 (slices) + 99 §1.5 | N25 (single home) | N04/N99 cite N25; keep only local consequences. |
| DUP-7 | Root-gate rule | 05 §1.2 + 10 §1.2 + 06 §1.4 | N05 (single home: pure predicate) | N07/N11 cite; keep only call-site context. |
| DUP-8 | Address-length/zone/mask rules | 05 §1.4 + 06 §1.3-1.5 + 11 §1.3 | N06 (single home) | N07/N12 cite; keep only family-specific additions. |

### 3.5 Out-of-scope table (declared boundary crossed → correct home)

| # | Topic | Found in | Correct home |
|---|-------|----------|--------------|
| O-1 | VFS client side (`socket.c` server, `sdev.c`, `smap.c`, select client) | Referenced as prereqs (01, 02, 05, 23 headers) | 05-stage-vfs docs 22/24 + 23-select; N-docs cite, never re-explain. Correctly excluded today; keep excluded. |
| O-2 | NDEV driver side + NIC implementations | 13 header prereq (`../16-stage-drivers/03-…`) | 16-stage-drivers; N14 stays consumer-only. |
| O-3 | MIB server side | 03/20/21 references | 10-stage-mib; N03/N21 keep registration-timing + message-translation only. |
| O-4 | Network commands (ifconfig/route/ping/tcpdump) | 17/18 references | 18-stage-commands. |
| O-5 | `net/gen/*` legacy device protocol | 23 (fallback record) | WONTFIX ([ARCH] N-2 records the discard decision; no behavior to preserve). |
| O-6 | `sys/socket.h` / `netinet/in.h` constant-value canon | 99 (shared with 14-stage-runtime/13) | Values indexed in N99; canon owned by 14-stage-runtime/13; no duplication. |

### 3.6 Non-C topics, answered one by one (fixed checklist)

| Topic | Answer for this stage |
|-------|-----------------------|
| Link & load | No link scripts, no asm: both servers are ordinary user-space ELF programs. Load = RS runtime manifests: `net/lwip/lwip.conf`, `net/uds/uds.conf` (domains, ipc peers, KILL, uds uid 0). Rust: `os/net/lwip/Cargo.toml`, `os/net/uds/Cargo.toml` (both depend on both framework crates). Home: N03 §1 / N22 §1 + N99 endpoint section. |
| Image & memory layout | Neither server is in the boot image. Runtime footprint rules: mempool slabs (N04), per-family socket caps (`NR_LNKSOCK`=4, UDS 256, mcast 128/8), ring sizes (UDS 32 KB). Home: respective N-docs; index in N99. |
| Asm entry & trap entry | None. User-space servers enter via SEF (`sef_startup`) and `sef_receive_status`. The "trap" equivalent is the 4-way / 2-way dispatch. Home: N03/N22. Explicitly state "no asm in this stage" so readers stop looking. |
| Startup assembly | `rc:259` (lwip + `-dev /dev/bpf -script /etc/rs.lwip`), `rc:283` (drivers-pending wait), `rc:286` (uds); `rs.lwip` recovery (TCPISN reload + daemon restarts); `.conf` manifests. Home: N03 + N22 + N99. |
| Build & toolchain | C: `net/lwip/Makefile`, `net/uds/Makefile`, `liblwip/{lib,dist}/Makefile*`, libc `Makefile.inc`. Rust: workspace `os/Cargo.toml` + per-crate manifests. Third-party subset selection = the 68-file list (N25). |
| Cross-module iface & wire formats | SDEV (`com.h:1037-1078`, `ipc.h` 6+5 layouts) → N01/N99; NDEV (`com.h:1085-1144`) → N14/N99; CDEV/BDEV for BPF (`bpfdev_process` arm) → N19 (+16-stage); rmib (`rmib.h`, `mibtree.c:56-66`) → N03/N21 (+10-stage); `SOCKID_*` → N01/N99. |
| Error paths | Every N-doc gets an errno chapter: `util_convert_err` map (N05), per-family deviations (N07–N11), validation rejections (N06), exhaustion reports (N04), drain/shutdown errors (N22/N23), fallback triggers (N24). P0 rule: every error maps to a Minix3 errno value. |
| Shutdown & exit | lwIP: stateless restart, no `_restart` cb, never-shutdown TODO (`lwip.c:275-278`) → N03 lifecycle section. UDS: SIGTERM drain (`uds_running`/`uds_in_use`, `uds_signal`, `uds_cleanup`) → N22 lifecycle section. |
| Concurrency & sync | Single-threaded event loop per server; `NO_SYS=1` contract (N25); suspension = continuation records, not threads (N02); no locks anywhere in C; Rust `!Send`-free policy half + future transport (N02/N03 state it). |
| Test infrastructure | `cargo test -p minix-net-lwip -p minix-net-uds -p minix-sockdriver -p minix-netdriver -p minix-sys` from `os/`; per-module `#[test]` counts locked in each N-doc §5 (provisional totals in §9-G9, verified in B). QEMU smoke + RS loading belong to edge E-NETSTART. |

## 4. New Catalog (26 numbered + 00 + 99 = 28 documents)

Five operations used: reorder (fix prereqs), split (old-05, old-11), merge (lnksock into ifconf), keep (21 docs, rewritten), archive (all 27 old docs exit the formal catalog in B phase; nothing deleted). One soft-limit note per the task brief: no new doc needs 3000 lines; the largest C scopes (tcpsock 2793, ifaddr 2224, rtsock 1912 lines) fit the full Ch1–ChN skeleton in an estimated 400–600 lines each (concept + tables, not line-by-line commentary). Length is controlled by tabulating, not by cutting semantics.

| New # | Title (one-line positioning) | Group | Source material |
|-------|------------------------------|-------|-----------------|
| N00 | `00-net-overview` — subsystem map: two servers, three frameworks, one ABI, reading order | Part 0 map | Old-00 (rewrite EN) + §1 strands + §3.6 |
| N01 | `01-sockdriver-framework` — SDEV vocabulary, suspend table, copy direction, sockid namespace | A framework | Old-01 (keep structure; add K-006/K-010, crate-split note) |
| N02 | `02-sockevent-framework` — objects, 256-hash, continuations, select, timers, event pump | A framework | Old-02 (fix D2 crate paths; add K-010 boundary) |
| N03 | `03-lwip-main-init` — 17-step chain, 4-way loop, allocator dispatch, MIB capstone, lifecycle | B skeleton | Old-03 + G-1/G-2/G-5 lifecycle + rmib sections |
| N04 | `04-lwip-mempool` — 512-slice pool, gauges, pchain tools, exhaustion map | B skeleton | Old-04 (keep; expand symbol tables) |
| N05 | `05-lwip-err-time-priv` — err_t→errno map, tick conversion, root predicate | C helpers | Old-05 util part (split S-1) |
| N06 | `06-lwip-addr-policy` — sockaddr validation + SOCKADDR_MAX + RFC 6724 selection | C helpers | Old-05 addr+addrpol part (split S-1) |
| N07 | `07-lwip-ipsock` — 3 address kinds, validation chains, 2-tier options | D common | Old-06 (fix D1 prereq) |
| N08 | `08-lwip-pktsock` — byte-count gate, defaults table, 3-bit flags, layout rule | D common | Old-07 (keep) |
| N09 | `09-lwip-tcpsock` — 5 progress flags, buffer tiers, EPIPE split, ISN, documented absences | E sockets | Old-08 (keep; expand) |
| N10 | `10-lwip-udpsock` — protocol gate, TTL/loop defaults, 2-stage length check | E sockets | Old-09 (keep) |
| N11 | `11-lwip-rawsock` — 0–255 range, allocator root gate, HDRINCL, ICMPv6 checksum | E sockets | Old-10 (keep) |
| N12 | `12-lwip-lldata` — ARP/ND cache layer and why it is separate from IP routing | E sockets | Old-11 lldata part (split S-2) |
| N13 | `13-lwip-mcast` — 128/8 caps, join/leave order, ifdown sweep, stack mapping | E sockets | Old-12 (renumber only) |
| N14 | `14-lwip-ndev` — 8 slots, queue guarantees, active test, DS tracking, wire values | F interface | Old-13 (renumber; add G-3 wire subsection) |
| N15 | `15-lwip-ifdev` — 16-op table, 3-HW list, loopif bounds | F interface | Old-14 (renumber only) |
| N16 | `16-lwip-ethif` — MTU/mcast/reserve-8 bounds + ndev binding | F interface | Old-15 (renumber only) |
| N17 | `17-lwip-ifaddr` — v6 flags, preference order, field ownership | F interface | Old-16 (renumber only) |
| N18 | `18-lwip-ifconf-lnksock` — loopback default, 8-family dispatch, MINIX extensions, AF_LINK narrow handle | F interface | Old-17 + old-11 lnksock part (merge M-1) |
| N19 | `19-lwip-bpf` — buffer tiers, 512-rule, version gate, single-consumer, filter port | F interface | Old-18 (renumber only) |
| N20 | `20-lwip-route` — prefix-only assumption, weak-symbol overrides, gateway hooks | G routing | Old-19 (renumber only) |
| N21 | `21-lwip-rtsock` — version check, TX/RX bounds, no-leak rule, table→message translation | G routing | Old-20 (renumber; add G-2 message section) |
| N22 | `22-uds-core` — 256 objects, 5 states + limbo, 64-hash, CONNWAIT pair, stat surface, drain exit | H uds | Old-21 + G-1 lifecycle + G-5 man-page sections |
| N23 | `23-uds-io` — 32 KB ring, 4 segment kinds, 4096 ancillary cap, type edges | H uds | Old-22 (renumber only) |
| N24 | `24-libc-socket` — 16 calls, 3 flag bits, VFS path + N-2 fallback record | I abi | Old-23 (fix F-1 file count) |
| N25 | `25-liblwip-port` — 68-file subset, glue, 4 hooks, 4 patches, N-1 selection | J third-party | Old-24 (keep; branch path) |
| N26 | `26-smoltcp-shim` — 7 mapping boundaries, timer synthesis, phasing, out-of-shim tables | J third-party | Old-25 (keep body per §4 decision; add contract + EN check) |
| N99 | `99-net-global-concepts` — 4 constant families, endpoint order, 4-neighbor boundaries, ARCH index | Global | Old-99 (rewrite EN; fix D3 anchors; add K-010 crate note) |

Reading paths. Main line (dependency order): N00 → N01 → N02 → N03 → N04 → N05 → N06 → N07 → N08 → N09 → N10 → N11 → N12 → N13 → N14 → N15 → N16 → N17 → N18 → N19 → N20 → N21 → N22 → N23 → N24 → N99. Branch paths (skippable, rejoin at N99): TCP/IP-only readers skip N22–N23; stack-implementation readers take N25–N26 after N03 (forward pointer in N03, back-references in N09–N11); BPF/observers take N19 standalone after N02+N15.

### 4.1 Change table (every operation with old → new, reason, knowledge IDs, disposition)

| Op # | Type | Old position | New position | Reason | Knowledge IDs | Disposition |
|------|------|--------------|--------------|--------|---------------|-------------|
| X-01 | rewrite-location | Old-06 prereq "05 + 07" | N07 prereq "N05 + N06" | Forward reference D1: 06-before-07 cannot require 07 | K-025–K-027 | Prereq rewritten; no knowledge moved |
| X-02 | fix | Old-02 header/test/closing Rust paths (`netdriver/src/sockevent.rs`) | N02 paths (`sockdriver/src/sockevent.rs` + `netdriver/src/socktable.rs`) | Stale path D2: file moved in `907e65f79` | K-008, K-010 | Path + test-command lines rewritten; add crate-boundary paragraph |
| X-03 | fix | Old-99 line anchors (init L203 / startup L293 "tool generated") | N99 regenerated anchors | Drift D3: true lines are 196/270/287 | K-011 | Re-derive via anchor tooling; Ch2 endpoint order rechecked |
| X-04 | rewrite-voice | Old-01–04 Chinese + analogy nets (post office/hotel/canteen) | N01–N04 English mechanism prose | Voice finding D4 + muse EN mandate | K-001–K-018 | Full Ch1 rewrite; tables/anchors preserved |
| X-05 | split S-1 | Old-05 (util+addr+addrpol, 256 lines) | N05 (K-019/K-020/K-015) + N06 (K-021/K-022/K-023/K-024) | Single-semantic rule: "what did it mean / when" vs "which address" are different reader questions; 3 C files in 256 lines is the thinnest coverage in the stage | K-015, K-019–K-024 | util sections → N05; addr+addrpol sections → N06; shared intro deleted with reason "split, no unique content" |
| X-06 | split S-2 + merge M-1 | Old-11 (lnksock 77-line handle + lldata 584-line cache) | N12 (K-037) + lnksock sections → N18 | L-1: `lnksock_ops` populates only `sop_ioctl=ifconf_ioctl` + `sop_free`; the handle IS an ifconf entry point. L-2: link-cache vs IP-routing separation deserves its own telling | K-036 → N18; K-037 → N12 | lnksock §§ → N18 new §2; lldata §§ → N12; old-11 archived |
| X-07 | renumber | Old-12…old-24 (13 docs) | N13…N25 (shift by +1 after the N05/N06 split; N18 absorbs, so 12→13 … 24→25) | Insert of N06 shifts all later numbers; merge keeps count neutral thereafter | All K in those docs | Bodies unchanged except prereq numbers + cross-ref targets (mechanical, migration table §7) |
| X-08 | keep | Old-25 smoltcp shim (126 lines, 1 day old, commits `2621513b8`/`9cb608cc0`) | N26 (number changes, body kept) | Fresh, decision-backed, single-semantic; rebuilding would destroy value | K-054 | Keep body; add §5 contract + EN-voice check in B |
| X-09 | archive | All 27 old docs | `archive/` (B phase moves, no deletion) | Rebuild-not-relocate: new docs are rewritten from pool + C + Rust, never by moving paragraphs | All K (each has a new home in §5 or a delete reason — none deleted) | Per-section migration rows for split/merge docs in §7; straight renames covered by the mechanical rename map |

### 4.2 Order-difference table (runtime truth vs teaching order; every deviation justified)

| # | Runtime fact (anchor) | Teaching choice | Reason | Back-reference compensation |
|---|-----------------------|-----------------|--------|------------------------------|
| O-1 | Helpers run INSIDE socket paths at runtime (validation on every bind/send) | N05/N06 taught BEFORE N07–N11 | Pure functions first: no service state needed to understand them; sockets chapter then cites instead of re-explaining | N07–N11 cite N05/N06 per call site; N06 lists its consumers |
| O-2 | `lwip_init()` runs at init step A-04, before all socket modules | N25/N26 (stack本体) taught LAST as branch path | Third-party code is not the stage's own semantics; teaching it first would invert the "own code first" principle | N03 §1.4 states the dependency + forward pointer; N09–N11 back-reference N25 for stack behavior |
| O-3 | User calls INITIATE everything (ABI is the runtime entry) | N24 (ABI) taught AFTER both servers | Caller's-view requires knowing what is being called; the machinery must exist first in the reader's mind | N24 §1 maps each call to its server-side handler chapter |
| O-4 | pktsock embeds ipsock (layout dependency both ways in practice) | N07 (ipsock) before N08 (pktsock), prereqs backward-only | Struct dependency `pktsock.first == ipsock` (`pktsock.c`); fixes old D1 forward ref | N08 §1.1 states the layout rule once |
| O-5 | UDS starts after lwIP at runtime (`rc:259` before `:286`) | N22–N23 taught after ALL lwIP docs | Framework-first + lwIP is the complex server; UDS reuses the framework vocabulary | N22 §1 opens with the framework-recap pointer, no re-explanation |

## 5. Per-Document Contracts (all 28; B phase writes bodies from these alone)

Format per contract: positioning, takes (pool IDs), not-takes (with handoff), prereqs (earlier numbers only), postrefs (who cites this), ground truth (C + non-C + Rust anchors), knowledge rows, acceptance checks.

### N00-00-net-overview
- Positioning: the map, not the territory. Answers "what is the net subsystem and in which order do I read it."
- Takes: K-011 (strand summary), K-048 (strand-B summary), K-055 (boundary list), §3.6 (non-C index).
- Not-takes: any mechanism (→ N01–N26); any constant value (→ N99); ARCH detail (→ plan §4 + N-docs).
- Prereqs: none (entry point; 16-stage NDEV-driver-side recommended, not required).
- Postrefs: every N-doc cites N00 for stage placement.
- Ground truth: `net/lwip/lwip.c:196-377`; `net/uds/uds.c:1303-1414`; `lwip.conf`; `uds.conf`; `rc:259,283,286`; `os/net/{lwip,uds}` + `os/libs/{minix-sockdriver,minix-netdriver}` existence.
- Knowledge rows: K-011/K-048/K-055 as listed in §2.
- Acceptance: reader can draw both loops' dispatch arms from memory; every N-doc referenced exactly once in the reading-order table; zero mechanism paragraphs (spot-check: no `sop_*`/`SDEV_*` definitions outside tables).

### N01-01-sockdriver-framework
- Positioning: the 17 windows and their numbering, suspension rights, and copy direction.
- Takes: K-001, K-002, K-003, K-004, K-005, K-006, K-010 (boundary facet).
- Not-takes: object/hash/continuation mechanics (→ N02); constant canon (→ N99); family implementations (→ N07–N13, N22–N23).
- Prereqs: N00.
- Postrefs: N02, N03, N22, N24.
- Ground truth: `sockdriver.c:8-26,47,1061,1120,1132`; `sockdriver.h:11-18,83-130`; `com.h:1037-1078`; `ipc.h` request/reply layouts; `os/libs/minix-sockdriver/src/sdev.rs` (710 lines) + `sockid.rs` (`minix-netdriver`).
- Knowledge rows: K-001…K-006, K-010 per §2.
- Acceptance: 17-request + 6-reply + guard tables complete; suspend table 8/8/1/special reproduced; `#[test]` count locked vs `sdev.rs`+`sockid.rs`; English voice, no analogy net.

### N02-02-sockevent-framework
- Positioning: where half-finished requests live and how they resume.
- Takes: K-007, K-008, K-009, K-010 (boundary facet).
- Not-takes: SDEV encoding (→ N01); family implementations (→ N07–N13, N22–N23); transport/loop (→ N03/N22 service layers).
- Prereqs: N01.
- Postrefs: N03, N09–N11, N18 (select), N22, N23.
- Ground truth: `sockevent.h:7-19,27-52,54-109`; `sockevent.c:48-57,817` + full file; `sockevent_proc.c`; `os/libs/minix-sockdriver/src/sockevent.rs` (110 lines); `os/libs/minix-netdriver/src/socktable.rs` + `service.rs`.
- Knowledge rows: K-007–K-010 per §2.
- Acceptance: 21-callback table complete with per-callback one-line role; hash/timer/select/continuation each with rule + example; crate-boundary paragraph present (D2 fixed); test counts locked for both files.

### N03-03-lwip-main-init
- Positioning: the 13-step climb and the 4-way junction; the anchor chapter every later doc points at.
- Takes: K-011, K-012, K-013, K-014 + G-1 (lifecycle) + G-2 (rmib timing) + G-5 (rs.lwip).
- Not-takes: module internals (→ N04–N21, N25); MIB server side (→ 10-stage-mib); transport流量 (service binary).
- Prereqs: N01, N02.
- Postrefs: N04–N21, N25 (forward pointer).
- Ground truth: §1.1 A-01–A-17 full anchor set; `mibtree.c`; `lwip.conf`; `rc:259,283`; `rs.lwip`; `os/net/lwip/src/{main,server,startup}.rs` + `service.rs::classify`.
- Knowledge rows: K-011–K-014 per §2.
- Acceptance: init chain reproducible step-by-step against `lwip.c:196-267`; 4 dispatch arms each with source+guard+handler; lifecycle section (stateless stance + TODOs + rs.lwip) present; rmib timing paragraph present.

### N04-04-lwip-mempool
- Positioning: how big a buffer is and what "pool empty" reports.
- Takes: K-016, K-017, K-018.
- Not-takes: stack-internal pbuf semantics (→ N25); per-family queue use (→ N09–N11); pool storage impl (service layer).
- Prereqs: N03.
- Postrefs: N08, N09, N10, N19.
- Ground truth: `mempool.c:38-39,118,234-238,462`; `pchain.c`; `lwipopts.h:49,80`; `os/net/lwip/src/mempool.rs`.
- Knowledge rows: K-016–K-018 per §2.
- Acceptance: 512/64/~17 MB derivation shown; cur/max gauge rule stated; per-consumer exhaustion reports enumerated (no "pool empty crashes the service" misconception).

### N05-05-lwip-err-time-priv
- Positioning: the three stateless translations — error numbers, time units, privilege bit.
- Takes: K-019, K-020, K-015.
- Not-takes: address validation (→ N06); call sites per family (→ N07–N11 cite this); console logging (service layer).
- Prereqs: N03, N04 (pool-exhaustion context for the errno table's ENOBUFS arm).
- Postrefs: N06, N07, N11, N24.
- Ground truth: `util.c` (full: convert/root/timeval/copy-split); `lwip.c:24,127`; `os/net/lwip/src/util.rs` (17-arm table).
- Knowledge rows: K-019, K-020, K-015 per §2.
- Acceptance: full 16+1 mapping table; round-up formula with numeric example; overflow-guard bound stated; root predicate shown as pure function with service-side query identified.

### N06-06-lwip-addr-policy
- Positioning: which addresses are legal and, among legal ones, which source wins.
- Takes: K-021, K-022, K-023, K-024.
- Not-takes: per-family use of validation (→ N07–N11); constant canon (→ N99); link-address specifics beyond the shared formula (→ N12/N18).
- Prereqs: N03, N05 (errno arms referenced by validation rejections).
- Postrefs: N07, N12, N17, N18.
- Ground truth: `addr.c` (699 lines, all validators); `addrpol.c` (policy table); `sockdriver.h:11-18`; `lwip.h:26-47`; `os/net/lwip/src/addr.rs`.
- Knowledge rows: K-021–K-024 per §2.
- Acceptance: rejection checklist (length/family/zone/multicast/mask) each with errno; 256-bound + assert family explained; RFC 6724 table summarized with one worked selection; pure-vs-service split (K-024) stated.

### N07-07-lwip-ipsock
- Positioning: what TCP/UDP/RAW share at birth and before connect.
- Takes: K-025, K-026, K-027.
- Not-takes: family protocol semantics (→ N09–N11); queue storage (→ N08); address parsing mechanics (→ N06).
- Prereqs: N05, N06.
- Postrefs: N08, N09, N10, N11.
- Ground truth: `ipsock.c` (761 lines, all); `ipsock.h:1-60,29-33`; `os/net/lwip/src/ipsock.rs`.
- Knowledge rows: K-025–K-027 per §2.
- Acceptance: 3-kinds-from-2-bits truth table; no-resource + clone-inherits rules quoted; validation chains ordered; 2-tier option split with examples.

### N08-08-lwip-pktsock
- Positioning: the shared admission gate and buffer table for UDP and RAW.
- Takes: K-028, K-029.
- Not-takes: option semantics (→ N07); family behaviors (→ N10/N11); queue storage/wakeup (service layer).
- Prereqs: N07 (layout + buffer tiers), N06 (slice-size floor reference).
- Postrefs: N10, N11.
- Ground truth: `pktsock.c:38-40,59,106` + full file; `udpsock.c:27-34,138-139,279`; `rawsock.c:48-55,310-311,473`; `os/net/lwip/src/pktsock.rs`.
- Knowledge rows: K-028, K-029 per §2.
- Acceptance: byte-count gate formula + worked boundary example; defaults table for both families; 3-bit flag meanings; RAW's 3 extra input steps named.

### N09-09-lwip-tcpsock
- Positioning: connection progress, buffer tiers, broken-pipe split, secret sequence numbers.
- Takes: K-030, K-031, K-032, K-033.
- Not-takes: stack-internal TCP machine (→ N25); shared gate (→ N08); SIGPIPE mechanics (→ 14-stage-runtime + VFS).
- Prereqs: N07, N08.
- Postrefs: N25 (stack behavior back-ref).
- Ground truth: `tcpsock.c:87-90` + full 2793 lines; `ipsock.h:29-33`; `tcpisn.c` (203); `os/net/lwip/src/tcpsock.rs`.
- Knowledge rows: K-030–K-033 per §2.
- Acceptance: 5 flags + tiers + 3/4 mark; EPIPE two triggers + delivery split (with re-verified negative grep); 3 absences recorded; ISN input layout + time superposition + key-management story.

### N10-10-lwip-udpsock
- Positioning: what a connectionless socket still checks.
- Takes: K-034 (+ K-029 by citation).
- Not-takes: membership management (→ N13); queue storage (→ N08); option semantics (→ N07).
- Prereqs: N07, N08.
- Postrefs: N13 (membership delegation).
- Ground truth: `udpsock.c:27-34,116,128,138-139,279` + full 997 lines; `os/net/lwip/src/udpsock.rs`.
- Knowledge rows: K-034 per §2.
- Acceptance: protocol gate values; TTL=1/loop-on defaults with rationale; 2-stage length check with numbers; input-delegation statement.

### N11-11-lwip-rawsock
- Positioning: maximum capability, maximum gating.
- Takes: K-035 (+ K-029 by citation).
- Not-takes: ICMP semantics (→ N25); queue storage (→ N08); management/name queries (service layer).
- Prereqs: N07, N08, N10 (shared-rule comparison).
- Postrefs: none (leaf; cited by N25 for capability context).
- Ground truth: `rawsock.c:290,310-311,473` + full 1341 lines; `lwip.c:152-194` (gate); `os/net/lwip/src/rawsock.rs`.
- Knowledge rows: K-035 per §2.
- Acceptance: 0–255 range; gate location named (allocator, not module); HDRINCL send-path effect; IPv6-ICMPv6 forced-checksum rule as pure function.

### N12-12-lwip-lldata
- Positioning: the link-layer cache layer and its deliberate separation from IP routing.
- Takes: K-037.
- Not-takes: AF_LINK handle mechanics (→ N18); IP route internals (→ N20); cache storage/aging (service layer).
- Prereqs: N06 (address-length formula context).
- Postrefs: N18, N20 (peer layers, cited as non-sharing).
- Ground truth: `lldata.c` (584 lines, all entry points); `os/net/lwip/src/` (lldata coverage — B phase confirms file or records gap; see §6).
- Knowledge rows: K-037 per §2.
- Acceptance: ARP/ND two-logic coverage stated; separation reasons (stack isolation + BSD-8 lineage); interface-index association rule.

### N13-13-lwip-mcast
- Positioning: rationing and cleanup of shared groups.
- Takes: K-038.
- Not-takes: IGMP/MLD wire protocols (→ N25); per-socket option use (→ N09/N10).
- Prereqs: N07.
- Postrefs: N10 (delegation target).
- Ground truth: `mcast.c:36-47,143` + full 283 lines; `lwipopts.h:208-213,535-541`; `os/net/lwip/src/mcast.rs`.
- Knowledge rows: K-038 per §2.
- Acceptance: 64/64/128/8 each locked by test; join-check order; leave vs ifdown-sweep notification difference; no-1:1-mapping with stack structs.

### N14-14-lwip-ndev
- Positioning: the stack as a NIC-driver client — slots, depth, liveness.
- Takes: K-039 + G-3 (wire subsection).
- Not-takes: driver-side protocol (→ 16-stage-drivers/03); NIC implementations (→ 16-stage 22/23); interface/address management (→ N15/N17).
- Prereqs: N03 (loop-dispatch arm).
- Postrefs: N15, N16.
- Ground truth: `ndev.c` (1019) + `ndev.h` (33); `com.h:1085-1144`; `os/net/lwip/src/ndev.rs`; `os/libs/minix-netdriver/src/protocol.rs`.
- Knowledge rows: K-039 per §2.
- Acceptance: 8/2/2/8/40 numbers each explained; active test stated as the single condition; no-interface request tracking; DS-notify→check path drawn.

### N15-15-lwip-ifdev
- Positioning: the shape that fits every interface.
- Takes: K-040.
- Not-takes: ethernet specifics (→ N16); address lists (→ N17); ioctl dispatch (→ N18).
- Prereqs: N14.
- Postrefs: N16, N17, N19.
- Ground truth: `ifdev.c` (1064) + `ifdev.h:20,28-48,56-57,73`; `loopif.c` (420); `os/net/lwip/src/ifdev.rs`.
- Knowledge rows: K-040 per §2.
- Acceptance: 16-op table with first/last roles; 3-list + VALID/FACTORY; loopback MTU/count bounds; address-field ownership boundary.

### N16-16-lwip-ethif
- Positioning: what the ethernet instance adds to the generic table.
- Takes: K-041.
- Not-takes: generic registration/polling (→ N15); driver packet flow (service layer); address management (→ N17).
- Prereqs: N14, N15.
- Postrefs: N17 (mcast-list context).
- Ground truth: `ethif.c` (1718); `os/net/lwip/src/ethif.rs`.
- Knowledge rows: K-041 per §2.
- Acceptance: 1500/1500/8/8 numbers; reserve-8 basis; ndev-binding statement; mcast-list cap.

### N17-17-lwip-ifaddr
- Positioning: address flags, preference order, field ownership.
- Takes: K-042.
- Not-takes: list storage/dedup/route-update (service layer); ioctl dispatch (→ N18); policy-label definitions (→ N06).
- Prereqs: N15, N16.
- Postrefs: N18.
- Ground truth: `ifaddr.c` (2224); `ifaddr.h:5-7`; `os/net/lwip/src/ifaddr.rs`.
- Knowledge rows: K-042 per §2.
- Acceptance: 3 v6 flag bits; 2-level preference rule with example; field-ownership boundary vs ifdev.

### N18-18-lwip-ifconf-lnksock
- Positioning: loopback-by-default at birth, family dispatch at runtime, and the narrow AF_LINK handle that is really an ifconf entry point.
- Takes: K-043 + K-036 (merged).
- Not-takes: address add/delete semantics (→ N17); medium parameters (→ N15/N16); user-space ioctl wrappers (→ 18-stage-commands).
- Prereqs: N17 (address semantics), N07 (creation-semantics contrast for lnksock narrowness).
- Postrefs: N12 (peer, non-sharing).
- Ground truth: `ifconf.c:79-625,866` + full 930 lines; `minix/if.h:39-49`; `lnksock.c:41-77` (NR_LNKSOCK=4, 2-callback ops); `os/net/lwip/src/{ifconf,lnksock}.rs`.
- Knowledge rows: K-043, K-036 per §2.
- Acceptance: default-config address set; 8-family dispatch map; 2 MINIX extensions explained; lnksock narrowness (DGRAM+0, cap 4, length formula, ops=ioctl+free) with the merge rationale L-1 stated.

### N19-19-lwip-bpf
- Positioning: observing traffic without disturbing forwarding.
- Takes: K-044.
- Not-takes: NIC drivers (→ 16-stage); tcpdump semantics (→ 18-stage); chardriver generics (→ 16-stage/01).
- Prereqs: N02 (select), N15 (interface model).
- Postrefs: none (leaf).
- Ground truth: `bpfdev.c` (1365); `bpf_filter.c:149` (561); `os/net/lwip/src/bpfdev.rs`.
- Knowledge rows: K-044 per §2.
- Acceptance: 32/32768/262144/512/1/1 numbers; single-consumer assumption; mbuf→pbuf port deltas; [ARCH] N-3/N-9 forward pointers.

### N20-20-lwip-route
- Positioning: prefix-length-only storage and Minix's replacement of stack routing.
- Takes: K-045, K-046.
- Not-takes: rtsock message format (→ N21); stack-internal algorithms (→ N25); ifaddr change semantics (→ N17).
- Prereqs: N03 (init position), N06 (mask-contiguity rule).
- Postrefs: N21.
- Ground truth: `rttree.c:5-8` + full 744 lines; `route.c:7-8` + full 1654; `lwiphooks.h`; `os/net/lwip/src/route.rs`.
- Knowledge rows: K-045, K-046 per §2.
- Acceptance: prefix assumption quoted; 3 bit-math formulas; 4 override/hook points located; entry-management operations listed.

### N21-21-lwip-rtsock
- Positioning: messages, not pointers, cross the control plane.
- Takes: K-047 + G-2 (message section).
- Not-takes: table internals (→ N20); RTA compress/expand code (service layer); MIB registration flow (service layer).
- Prereqs: N20.
- Postrefs: none (leaf; cited by 18-stage route tools).
- Ground truth: `rtsock.c:311` + full 1912 lines; `os/net/lwip/src/rtsock.rs`.
- Knowledge rows: K-047 per §2.
- Acceptance: version-4 check timing; TX≤512 / RX∈[0,65536]; isolation rule quoted; table→message translation points.

### N22-22-uds-core
- Positioning: local sockets — 256 objects, 5 states, file-path lookup, drain exit.
- Takes: K-048, K-049 + G-1 (lifecycle) + G-5 (man page).
- Not-takes: data plane (→ N23); framework generics (→ N02); VFS copyfd client (→ 05-stage-vfs).
- Prereqs: N02 (event dispatch), N03 (loop-shape contrast).
- Postrefs: N23.
- Ground truth: `uds.c:5,38-47,101,120-123,1303-1414`; `uds.h:15,18,33-36,86-135`; `stat.c:11,163-186`; `uds.conf`; `rc:286`; `unix.8`; `os/net/uds/src/{core,server,main}.rs`.
- Knowledge rows: K-048, K-049 per §2.
- Acceptance: 256/5/64 numbers; limbo + CONNWAIT pair drawn; hash formula; drain-exit condition as logic expression; stat surface enumerated.

### N23-23-uds-io
- Positioning: one ring for both directions; data interleaved with metadata.
- Takes: K-050.
- Not-takes: state machine (→ N22); VFS FD-copy client (→ 05-stage-vfs); suspension generics (→ N02).
- Prereqs: N22.
- Postrefs: none (leaf).
- Ground truth: `io.c:70-95,102,122,148,205-249` + full 1803 lines; `uds.h:33-36`; `os/net/uds/src/io.rs`.
- Knowledge rows: K-050 per §2.
- Acceptance: 32768/4096 numbers; ring-advance macro; 4 segment kinds; STREAM/SEQPACKET/DGRAM edge table; FD-queue + credential rules.

### N24-24-libc-socket
- Positioning: how 16 user calls cross one syscall to the filesystem — and when they fall back.
- Takes: K-051 + G-4 (bundling note).
- Not-takes: VFS server side (→ 05-stage-vfs/24); driver/UDS server impl (→ N03/N22); legacy device wire detail (WONTFIX record only).
- Prereqs: N06 (syscall-mechanism background), N03/N22 (request destinations).
- Postrefs: none (leaf; entry for user-space readers).
- Ground truth: 16 `libc/sys` files (§3.2 list); `os/libs/minix-sys/src/socket.rs` (call list + flag map + fallback predicate).
- Knowledge rows: K-051 per §2.
- Acceptance: 16-call list; 3 flag-bit mappings; fallback trigger errors named; F-1 count corrected; [ARCH] N-2 discard decision recorded with review anchor.

### N25-25-liblwip-port
- Positioning: which third-party code is compiled, which options/hooks the service depends on, what replaces it.
- Takes: K-052, K-053.
- Not-takes: Minix-side modules (→ N03–N21); stack-internal algorithms (not our code); replacement implementation (phased, behind the wall).
- Prereqs: N03 (init call surface).
- Postrefs: N26, N09–N11 (back-refs).
- Ground truth: `dist/src` 68-file list; `lwipopts.h:14,49,80,211,259,267,282,538`; `lwiphooks.h` (4 hooks); `arch/cc.h`; `patches/0001-0004`; `os/net/lwip/src/lwip_port.rs` (constants + `Stack`/`StackHooks` wall).
- Knowledge rows: K-052, K-053 per §2.
- Acceptance: subset scale stated; key options as contract constants; 4 hooks + 4 patches each one-lined; N-1 selection + deviation table + FFI-fallback-wall recorded.

### N26-26-smoltcp-shim
- Positioning: the exact Minix↔smoltcp line — shapes, holders, and who answers what when something is missing.
- Takes: K-054.
- Not-takes: NIC data path (→ 16-stage; one `TrunkDevice` seam only); route/link tables (→ N20/N12/N21); per-family operation translation (→ N09–N11 B-phase batches).
- Prereqs: N25 (selection), N02 (event/suspend vocabulary), N01 (request surface).
- Postrefs: none (leaf).
- Ground truth: `os/net/lwip/src/stack.rs` (`Stack` impl); `lwip_port.rs` (wall); C-side semantic sources `lwip.c`/`sockdriver.c`/`sockevent.c`/`lwipopts.h` (shapes being mapped, not re-explained).
- Knowledge rows: K-054 per §2.
- Acceptance: 7 boundaries each with holder + missing-case error; timer synthesis described; phasing stated; out-of-shim tables (route/link) present; body kept from old-25 unless EN-voice check fails.

### N99-99-net-global-concepts
- Positioning: every shared number's single home + the endpoint order + the 4-neighbor boundary list.
- Takes: K-055 + K-010 (crate note) + D3 anchor regeneration.
- Not-takes: all mechanism (→ N00–N26).
- Prereqs: none (reverse-entry readers are sent back to N00).
- Postrefs: every N-doc cites N99 for values.
- Ground truth: `com.h:1037-1078,1085-1144`; `ipc.h` 6+5 layouts; `lwip.h:58-62`; `sockevent.h:7-19`; `sockdriver.h:11-18`; `lwipopts.h` contract values; `minix-sockdriver` + `minix-netdriver` + `minix-types` locations.
- Knowledge rows: K-055 per §2.
- Acceptance: 4 families complete; endpoint order matches `lwip.c:294-377` sequence; boundary list covers 05/10/16/18; test-distribution map (§5-of-which-doc) present; D3 anchors regenerated, zero "tool generated" placeholders.

## 6. Missing-New-Chapter Resolution (fixed checklist — no blanks, no "TBD")

| Item | Verdict |
|------|---------|
| Link/load | No new chapter: one section each in N03 (manifests + rc + rs.lwip) and N22 (uds.conf + rc). |
| Image/memory layout | No new chapter: caps live in N04/N12/N13/N22/N23; N99 indexes them. |
| Asm/trap entry | No new chapter: explicit "no asm in this stage" statement in N03 §1 + N22 §1 (kills the search). |
| Startup assembly | No new chapter: sections in N03/N22/N99 (G-5 included). |
| Build/toolchain | No new chapter: build-file table in N25 §1 (C subset) + N03 §1 (Rust crates). |
| Cross-module iface/wire | No new chapter: N01 (SDEV) + N14 (NDEV) + N19 (CDEV/BDEV arm) + N03/N21 (rmib, G-2); canon in N99. |
| Error paths | No new chapter: errno chapter per N-doc (§5 contract element, P0 rule). |
| Shutdown/exit | No new chapter: lifecycle sections in N03 (stateless + TODOs) and N22 (drain) — G-1. |
| Concurrency/sync | No new chapter: event-loop + NO_SYS + continuation statements in N02/N03/N25. |
| Test infrastructure | No new chapter: per-doc §5 test maps + edge E-NETSTART for QEMU/RS. |
| lldata coverage in Rust | To-confirm in B: `os/net/lwip/src/` listing shows no `lldata.rs`. Either the logic lives inside another module (name it in N12) or it is a genuine implementation gap (file it as a code TODO, not a doc gap). N12's contract requires the answer. |

## 7. Anchor Migration, Reference Migration, Breakage Cost

### 7.1 Rename map (mechanical; applies to all straight renames + body cross-refs)

`01→01, 02→02, 03→03, 04→04, 05→05+06(split, §7.2), 06→07, 07→08, 08→09, 09→10, 10→11, 11→∅(split, §7.2), 12→13, 13→14, 14→15, 15→16, 16→17, 17→18(merge target), 18→19, 19→20, 20→21, 21→22, 22→23, 23→24, 24→25, 25→26, 99→99`. Filenames keep the `NN-short-name.md` convention; short-names become English (`05-lwip-err-time-priv`, `06-lwip-addr-policy`, `12-lwip-lldata`, `18-lwip-ifconf-lnksock`, others keep stems).

### 7.2 Per-section migration for split/merge docs (all sections accounted)

- Old-05 (256 lines: util §§ → N05; addr §§ → N06; addrpol §§ → N06; shared intro → deleted with reason "split, no unique content"; test-map § → split by module). B phase re-derives section numbers from the new bodies; the contract knowledge IDs (§5) are the binding targets, not old section numbers.
- Old-11 (lnksock §§ → N18 §2 "narrow handle"; lldata §§ → N12; shared intro → deleted, same reason; test-map § → split).
- Old-17 (all §§ → N18 §§1,3–5; new §2 reserved for the incoming lnksock handle).
- All other old docs: full-body successor = the mapped N-doc (§7.1); no section survives outside its successor (X-09 disposition).

### 7.3 Reference migration table (retrieved, not estimated)

- Intra-stage doc cross-refs: `rg -c "17-stage-net/[0-9]"` hits per doc range 2–5, ≈ 90 hits across 26 docs → each hit rewritten to the §7.1 target (one `sed` per old number + manual check of surrounding sentence for split docs 05/11).
- Code-comment refs: 5 hits (`os/net/uds/src/lib.rs:7` → old-22 = N23; `os/net/lwip/src/{main,lib}.rs` + `os/net/uds/src/main.rs` stage-level refs stay valid; `os/libs/minix-netdriver/src/lib.rs:23` stage-level, stays valid). Only the `22-uds-io.md` filename hit needs a rename edit; the rest are stage-level and unaffected.
- Inbound refs from other stages (VFS/MIB/drivers/commands docs citing `17-stage-net/NN-…`): B phase runs `rg -l "17-stage-net/0[1-9]|17-stage-net/1[0-9]|17-stage-net/2[0-5]" notes/rewrite/` outside this dir and rewrites targets per §7.1. (Not run in R to avoid cross-AI working-tree interference; the command is specified here so B needs no judgment.)
- Breakage summary: ≈ 90 intra-stage + ≈ 5 code-comment + inbound-TBD (single grep) hits; hotspots are the prereq headers of N07–N11 (old 06–10) and the N99 value-index citations. Batch method: numeric `sed` rename per §7.1 map, then targeted hand-fix of the old-05/old-11 split references (enumerated in §7.2). No dangling reference may survive: B-phase acceptance is `rg "17-stage-net/(0[1-9]|1[0-9]|2[0-5])-"` returning only `archive/` hits.

## 8. Verification & Self-Check Gates

- Forward-reference scan (new prereqs, §5): every `Prereqs` entry points to a smaller number — N07:{N05,N06} ✓, N08:{N07,N06} ✓, N18:{N17,N07} ✓, N22:{N02,N03} ✓, N26:{N25,N02,N01} ✓ (N26 cites N01/N02 backward — allowed, they are earlier). Zero forward references.
- Dependency graph: chain + backward-only branches (N25/N26 branch off N03; N22/N23 chain off N02). No cycles (a cycle would require a prereq pointing forward; none does).
- Coverage: 55/55 pool entries have a home (§5 takes); 27+3+1+2+16 C files + 3 glue headers + 4 patches + 10/10 non-C items all mapped (§3.2/§3.6); explicit delete list = 2 shared-intros (X-05/X-06, reason given); zero unterminated gaps (§6 has no blanks).
- Split/merge accounting: X-05/X-06/X-07 sampled —存量 directions all resolved (§7.2);新增 entries (K-010/K-053/K-054 + G-1/G-2/G-5 sections) all carry evidence anchors.
- Contract completeness: 28/28 contracts carry all 7 elements (spot-verified N00/N07/N18/N26/N99 above).
- Migration coverage: all 27 old docs appear in §7.1/§7.2; doc+code-comment references enumerated in §7.3.

| Gate | Check | Result |
|------|-------|--------|
| G1 | 10 true-order anchors spot-rechecked in-session | PASS: A-02 (`lwip.c:152,196,270,287,294`), B-02/B-03 (`uds.c:1303,1367,1384`), C-01 (`sockdriver.c:47,1061,1120,1132`), `lwip.h:58-62`, `com.h:1037-1041,1085-1086`, `sockevent.c:48-57`, `lnksock.c` ops literal, `mcast.c:41,47` + `lwipopts.h:211,538`, `uds.h:15,18,36`, `rc:259,283,286` — all outputs quoted in §0/§1 |
| G2 | Every C file + non-C artifact has a home or explicit exclusion | PASS (§3.2 file→doc 100%; §3.6 10/10; §3.5 exclusions; one B-confirm: lldata.rs presence — §6) |
| G3 | Zero forward references in the new catalog | PASS (scan above) |
| G4 | Dependency graph acyclic; any cycle gets a split plan | PASS (chain + backward branches; no cycles) |
| G5 | 100% pool coverage with evidence for新增; deletes listed | PASS (55/55; deletes = 2 intros with reasons) |
| G6 | Splits/merges show存量 disposition; new chapters show新增 sources | PASS (X-05/X-06/X-07 + §7.2; G-sections anchored) |
| G7 | 7 elements per contract, all 28 docs | PASS |
| G8 | Migration covers all changed docs' sections + doc/code refs | PASS (§7.1–§7.3) |
| G9 | 10 random factual claims re-verified; guesses labeled | PASS: SPOT1–SPOT10 batch (§0) — 9/10 direct hits; SPOT2's first pattern missed (`sop_ioctl = ifconf_ioctl` uses `=`-with-tab spacing) and was recovered by reading `lnksock.c` in full (corrected anchor recorded, miss disclosed); `plan.md` "15 libc files" figure refuted by `ls` (16 incl. `loadname.c`) and filed as F-1 rather than repeated. No unlabeled guesses remain. |

Conclusion: blueprint COMPLETE and executable. Open questions for the user (decisions, not gaps): (1) confirm English-only rebuilt bodies (muse mandate assumed yes); (2) confirm full renumber (§7.1, ≈100-ref churn) vs minimal-churn alternative (keep old numbers, accept the N05-split insert as `05a/05b` — not recommended: breaks the `NN-` convention); (3) confirm N26 keeps its body (X-08) vs folding the shim into N25; (4) confirm the two shared-intro deletions (X-05/X-06). B phase starts on approval of (1)–(4); default on silence: yes / renumber / keep / delete.
