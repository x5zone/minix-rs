# 15-stage-fs Document Rebuild Blueprint (muse)

## 0. Metadata

- Executor: muse
- Date (UTC): 2026-09-19
- Target directory: `notes/rewrite/fork-syscall-rewrite/15-stage-fs/`
- Repository root: `/home/xzhao/github/minix-rs`
- Commit: `5548a942b2986c76471b80f8d873742a2f28f5f4` (`git rev-parse HEAD`; log head `5548a942b docs(edge3): S33 ...`)
- Task: R-phase rebuild blueprint. Output is `target_dir/doc_rerank_muse.md`. No body text modified.
- Constraints honored: no use of hidden intermediate-directory contents; single landed product with `_muse` suffix; no reading or copying of any other AI's `doc_rerank_*` product; no modification/rename/move/delete of any existing file; no commits; English output (muse special requirement).
- Size/count policy (from task brief, applied throughout): a single document may carry exactly one concept and grow to ~3000 lines if that concept is genuinely complex (soft limit); normally keep length controlled for readability. Old document count is not a constraint: the new catalog may add or remove documents. Old documents may contain errors or unreviewed material, so ground truth priority is `Minix3 C source + Rust code > existing documents`. Existing documents are knowledge sources, not move targets.

### 0.1 Scope

Counted as documents (26 total): `00-fs-overview.md`, `01-fsdriver-task.md` through `24-vbfs-hgfs.md` (24 numbered), `99-global-concepts.md`.

Counted as reference material (not documents): `plan.md`, `todo.md`, `draft/README.md`, `draft/` remainder, `checklist.md` (if present). These inform scope and history but do not receive anchor-migration rows except where they contain normative statements reused by the new catalog (explicitly noted).

Out of scope: other stage directories; `os/` code outside the FS stage list in §0.3; Minix3 sources outside `minix3/minix/fs/`, `minix3/minix/lib/{libfsdriver,libminixfs,libvtreefs,libsffs}/`, `minix3/minix/include/minix/{vfsif.h,fsdriver.h,libminixfs.h,com.h (FS_BASE only),callnr.h}`, `minix3/minix/servers/vfs/main.c` (birth chain only), `minix3/minix/kernel/table.c` (boot_image slots only). Anything important found outside this scope is listed under "Out-of-scope findings" (§3.5, §7).

### 0.2 Reading list (actually read)

1. All 26 target documents: full header declarations (about/what-not/frontier) for every file; body sampled section by section via `grep ^#{1,4}` plus full reads of `00`, `99`, `01`, `03`, `04`, `07`, `13`, `16`, `17`, `22`, `24` (the structurally decisive ones).
2. Minix3 C ground truth, verified file by file (not from the old mapping table alone):
   - `minix3/minix/include/minix/vfsif.h` (84 lines, full read): `REQ_* = FS_BASE+1..33`, `NREQS=34`, `TRNS_*`, `IS_FS_RQ`, `EENTERMOUNT/-301`, `ELEAVEMOUNT/-302`, `ESYMLINK/-303`, `RES_*`, `REQ_RDONLY/ISROOT`, `PATH_*`, `vfs_ucred_t`.
   - `minix3/minix/lib/libfsdriver/`: `fsdriver.h` (84 lines, full), `fsdriver.c` (97 lines, full), `table.c` (40 lines, full), `call.c` (1013 lines, structure + group sampling), `lookup.c` (333 lines, head + main loop), `dentry.c` (99 lines), `utility.c` (105 lines).
   - `minix3/minix/lib/libminixfs/`: `bio.c` (263 lines, head + `lmfs_bio`/`block_prefetch`/`lmfs_bflush`), `cache.c` (1321 lines, structure + LRU/hash/dirty/readahead/heuristic/vmcache sections), `inc.h`.
   - `minix3/minix/fs/mfs/`: `main.c` (102, full), `table.c` (45, full), `cache.c` (109, full), `mount.c` (173, full), plus `wc -l` + symbol sampling for `inode.c` (464), `link.c` (638), `read.c` (556), `write.c` (319), `path.c` (241), `open.c` (270), `super.c` (365), `protect.c` (58), `stadir.c` (104), `time.c` (49), `utility.c` (36), `misc.c` (23), `stats.c` (89).
   - `minix3/minix/fs/pfs/pfs.c` (451, head + mount/unmount/findnode + table sampling).
   - `minix3/minix/lib/libvtreefs/`: all 10 `.c` + headers (`vtreefs.c` 110, `inode.c` 626, `table.c` 24, `path.c` 59, `link.c` 129, `mount.c` 56, `stadir.c` 122, `file.c` 295, `extra.c` 56, `sdbm.c` 30).
   - `minix3/minix/lib/libsffs/`: all 15 `.c` (`wc -l` verified; `main.c` 59, `table.c` 28, `mount.c` 89, `lookup.c` 150, `verify.c` 118, `path.c` 108, `name.c` 52, `handle.c` 77, `dentry.c` 183, `inode.c` 292, `link.c` 366, `read.c` 173, `stat.c` 176, `write.c` 131, `misc.c` 53).
   - `minix3/minix/fs/ext2/`: 17 `.c` file list + `wc -l` verified; `main.c`/`table.c` full reads (both mirror mfs table shape).
   - `minix3/minix/fs/isofs/`: 12 `.c` file list + `table.c` full read (read-only subset with two `#if 0` peek gaps).
   - `minix3/minix/fs/procfs/`, `ptyfs/`, `vbfs/vbfs.c` (141), `hgfs/hgfs.c` (106): file lists + table/entry sampling.
3. Non-C build/boot artifacts: `minix3/minix/fs/Makefile` (SUBDIR list: mfs+pfs always; ext2/isofs/procfs/ptyfs + hgfs/vbfs on i386 non-image-only), `minix3/minix/servers/vfs/main.c:477-516` (`mount_pfs`, `mount_fs(DEV_IMGRD,...,MFS_PROC_NR,...)`), `minix3/minix/kernel/table.c:62-63` (PFS/MFS boot_image slots).
4. Boundary materials: `notes/rewrite/fork-syscall-rewrite/00-master-plan/README.md` (§ stage table + boot two-layer semantics), `notes/rewrite/fork-syscall-rewrite/edge_todo.md` (E-FSRUNTIME/E-FSBDEV/E-FSVMCACHE/E-FSCMDS registration, 2026-09-16 entry), target `plan.md` (full read, §§1-5; §4 ARCH table is design-time candidate list), target `todo.md` (V1 architecture review front matter + coverage matrices §§2.1-2.6 sampled), `draft/README.md` (placeholder scope).
5. Previous stage `14-stage-runtime/00-runtime-overview.md` (runtime-is-library thesis, lifecycle mainline, ARCH A-2): confirms FS stage must not re-explain crt0/allocator/syscall wrapping; first FS document starts at "a born user process that serves VFS requests".
6. Rust implementation entries: `os/libs/minix-fs/src/lib.rs` (module→document map, full), `protocol.rs`/`driver.rs`/`call.rs`/`data.rs`/`dentry.rs`/`lookup.rs`/`cache.rs`/`vm_cache.rs`/`bio.rs`/`bdev_bridge.rs`/`task.rs`/`memfs.rs` (symbol-level `grep pub`), `os/fs/mfs/src/` (18 modules incl. `server.rs` 1527 lines, `second_level.rs`, `dir_io.rs`, `startup.rs` — none in old doc map), `os/fs/fs-rt/src/` (`lib.rs` 38, `wire.rs` 657, `transport.rs` 459, `ipc.rs` 228, `source.rs` 47 — no owning document), `os/fs/{pfs,procfs,ptyfs,ext2,isofs,vbfs,hgfs}/src/`, `os/libs/minix-vtreefs/src/` (`tree.rs` 1073 + `driver.rs` 475 — `driver.rs` not in old doc map), `os/libs/minix-sffs/src/` (5 modules vs C 15 files).
7. Style example `01-stage-kernel/06-todo.md` §§1-3 only (how a contract states what/why/boundary/acceptance; no content borrowed).

### 0.3 Commands and key outputs (evidence excerpts)

- `wc -l notes/rewrite/fork-syscall-rewrite/15-stage-fs/*.md | sort -n`: 26 docs, 62 (`99`) .. 358 (`01`) lines; total 12471 incl. plan/todo. Byte sizes 3.8K (`99`) .. 36K (`01`). Mean density ~100 bytes/line: dense exposition, not long-form. No document approaches the 3000-line soft cap; length pressure is absent, compression pressure is present (§3).
- `ls minix3/minix/fs/`: `ext2 hgfs isofs mfs pfs procfs ptyfs vbfs` (+ `Makefile`, `Makefile.inc`). `libfsdriver`: 6 `.c` + `fsdriver.h`. `libminixfs`: `bio.c cache.c inc.h`. `libvtreefs`: 10 `.c`. `libsffs`: 15 `.c`.
- `cat minix3/minix/include/minix/vfsif.h`: 33 `REQ_*` (`FS_BASE+1..33`), `NREQS 34`, `TRNS_GET_ID/ADD_ID/DEL_ID`, `IS_FS_RQ (((type)&~0xff)==FS_BASE)`, `-301/-302/-303`, `RES_THREADED/HASPEEK/64BIT`, `REQ_RDONLY/ISROOT`, `PATH_RET_SYMLINK/GET_UCRED`, `vfs_ucred_t`.
- `cat minix3/minix/lib/libfsdriver/fsdriver.c`: `fsdriver_process` (notify-or-non-VFS → `fdr_other`, no reply; else `mounted || READSUPER` gate → `call_nr-=FS_BASE` → `callvec` or `ENOSYS`; reply `TRNS_ADD_ID(r,transid)`; `fdr_postcall`), `fsdriver_terminate` (`running=FALSE; sef_cancel`), `fsdriver_task` (`while(running||mounted) sef_receive_status(ANY)` → `fsdriver_process(...,FALSE)`).
- `cat` of all six `table.c` files: `fsdriver_callvec` has 32 slots (no `REQ_GETNODE`, no `REQ_MOUNTPOINT` — both `ENOSYS` by absent slot); `mfs_table`/`ext2_table` full 31-hook tables; `isofs_table` read-only subset (no write/link/trunc/create family; two `#if 0` peek gaps); `sffs_dtable` 17-hook subset (`do_*` names); `vtreefs_table` 17-hook subset + `fdr_other`.
- `cat minix3/minix/fs/mfs/main.c`: `main = env_setargs + sef_local_startup + fsdriver_task(&mfs_table)`; `sef_cb_init_fresh = may_use_vmcache(1) + zero inode table + init_inode_cache + buf_pool`; `signal = SIGTERM-only → fs_sync + terminate`.
- `cat minix3/minix/fs/mfs/mount.c`: `fs_mount = bdev_open → read_super → unclean→readonly-downgrade (+WARNING print) → set_blocksize → set_blockusage → get_inode(ROOT) → fill root_node → clear CLEAN → OK`; `fs_unmount = busy-check print + put root + fs_sync + set CLEAN + bdev_close + lmfs_invalidate + s_dev=NO_DEV`.
- `grep -rn` cross-reference count for old NN slugs inside the stage: 428 hits (dense internal mesh; renumber/add/remove must migrate, §8).
- `grep -rln 15-stage-fs` in `05-stage-vfs/*.md`: `plan.md` + `lib.rs` only at surface; deep VFS↔FS wire references use symbol anchors (`vfsif.h`, `TRNS_*`, `FS_BASE`), not stage paths — migration risk is concentrated inside the stage, not in VFS (§8.3).
- `git rev-parse HEAD` → `5548a942b...`; `git log --oneline -3` confirms edge3 S33 head.

---

## 1. C True Order

### 1.1 Stage-type judgment

**Service event-loop type** (per §9 of the R-prompt), not boot-chain, not syscall-set, not driver-set. Evidence: every FS server `main` has the same three-step skeleton (`env_setargs → sef_local_startup → fsdriver_task(&table)`; `mfs/main.c:14-42`; pfs/ext2/isofs/procfs/ptyfs/vbfs/hgfs mains same shape by file-list + `mfs/main.c` ground read; behavior bottom line is the C skeleton, details per server vary only in the table contents). There is no linear startup chain across servers: PFS and MFS are boot-image members mounted by VFS (`vfs/main.c:510 mount_pfs`, `:516 mount_fs(...,MFS_PROC_NR,...)`; `kernel/table.c:62-63`), the other six are RS-runtime-loaded on demand (`fs/Makefile` SUBDIR gating; procfs/ptyfs/ext2/isofs/vbfs/hgfs absent from boot_image). The teachable mainline is therefore **one request's lifecycle** (VFS drives everything), preceded by a short birth segment and organized by parallel request families (§5.2 rules). The boot-mount causal chain (PFS-first, MFS-root) is the secondary spine used to order the server chapters (pfs before mfs before the rest), recorded here as fact, not as the primary skeleton.

### 1.2 Runtime truth table

Each row is independently checkable against the cited anchor. `T-000..T-003` = birth; `T-100..` = loop; `T-200..` = per-family dispatch facts; `T-300..` = teardown.

| Step | Action | C anchor | Explanation |
|------|--------|----------|-------------|
| T-000 | Process born (RS loads ELF or boot_image slot) | `minix3/minix/kernel/table.c:62-63` (slots); `mfs/main.c:14-22` (`env_setargs`) | Birth mechanism itself belongs to RS/runtime; FS stage consumes a born process. |
| T-001 | SEF local startup registers init-fresh/restart/signal callbacks, then `sef_startup()` | `mfs/main.c:30-42` | Same shape in all 8 servers (speculation only for exact callback sets per server beyond mfs/pfs reads; each new server chapter must cite its own `main.c`). |
| T-002 | Init-fresh builds server state: vmcache opt-in, zero inode table, `init_inode_cache`, `lmfs_buf_pool` | `mfs/main.c:47-65` | Per-server init bodies differ (pfs builds 512 free list `pfs.c:87-104`; ext2 asserts LE + parses options; isofs sets nothing disk-mutable). New chapters cite per-server `main.c`. |
| T-003 | VFS mounts the FS: `REQ_READSUPER` is the only request honored while unmounted | `libfsdriver/fsdriver.c:36-46` (`mounted \|\| READSUPER` gate) | Mount is a message, not a boot step. PFS mount returns zeroed node (`pfs.c:87-116`, no root); MFS mount opens device + reads superblock (`mfs/mount.c:11-90`). |
| T-100 | Server blocks in `sef_receive_status(ANY)` | `libfsdriver/fsdriver.c:75-88` | Single-threaded: one message at a time. EINTR (from `sef_cancel`) retries; other errors panic. |
| T-101 | `fsdriver_process` classifies: notify or `m_source != VFS_PROC_NR` → `fdr_other` or drop, **no reply** | `libfsdriver/fsdriver.c:25-32` | Non-FS traffic (e.g. ptyfs `PTYFS_SET/DEL`, DS labels, signals) never enters the FS path. `fdr_other==NULL` means drop. |
| T-102 | `transid = TRNS_GET_ID(m_type)`; `call_nr = TRNS_DEL_ID(m_type)` | `vfsif.h:77-79`; `fsdriver.c:33-34` | Low 16 = transaction id, high 16 (signed short) = request or reply code. |
| T-103 | Mount gate; `call_nr -= FS_BASE` (wrapping intended); `callvec[call_nr]` or `ENOSYS`; unmounted non-super → `EINVAL` | `fsdriver.c:36-46`; `table.c:1-40` (32 slots) | `REQ_GETNODE(+1)` and `REQ_MOUNTPOINT(+27)` have no slot → always `ENOSYS`. This is deliberate C fact, not a gap. |
| T-104 | Adapter extracts params, validates, calls `fdr_*` hook, builds reply struct | `libfsdriver/call.c` (1013 lines; 31 adapters) | Six adapter families: mount (5), data (8), namespace (9), metadata (5), block (4). Details in new N02 contract. |
| T-105 | Leaf `fs_*`/`do_*` executes against in-memory tables, block cache, or host ops table | `mfs/*.c`; `libvtreefs/*.c`; `libsffs/*.c`; `ext2/*.c`; `isofs/*.c` | Three leaf kinds: disk (block cache + bitmaps), memory-tree (vtree/sffs node tables), host-bridge (op-table round-trips). |
| T-106 | Reply `m_out.m_type = TRNS_ADD_ID(r,transid)` via `ipc_send` (or `asynsend` for threaded) then `fdr_postcall` | `fsdriver.c:49-61` | `r` is signed errno or OK; `RES_THREADED` servers may reply async. |
| T-200 | Mount family: `READSUPER → MOUNTPOINT → NEWNODE → UNMOUNT → SYNC` | `call.c` mount group; `mfs/mount.c`; `mfs/misc.c:fs_sync` | `SYNC` here means whole-FS flush (`lmfs_flushall` + inode flush); per-file flush is `FLUSH` in block family. |
| T-201 | Node-lifecycle family: `PUTNODE` (count-1 batch release), `INHIBREAD` | `mfs/inode.c:38-64` (`fs_putnode`); `call.c` adapters | `PUTNODE` carries a count that includes the in-hand reference (subtract-one discipline). |
| T-202 | Data family: `READ/WRITE/PEEK/GETDENTS/FTRUNC` (+ `SEEK` internal, not a REQ) | `mfs/read.c` (556) `mfs/write.c` (319); `call.c` data group | `PEEK` = read without promotion side effects where `RES_HASPEEK`; isofs `#if 0`s out both peek slots (subpage-block FIXME). |
| T-203 | Namespace family: `LOOKUP/MKNOD/MKDIR/CREATE/LINK/UNLINK/RMDIR/RENAME/SLINK/RDLINK` | `mfs/path.c open.c link.c`; `libfsdriver/lookup.c` (framework walk) + per-FS single step | Framework walk handles mountpoints/symlinks (`EENTERMOUNT/ELEAVEMOUNT/ESYMLINK`); leaf handles one component. |
| T-204 | Metadata family: `STAT/CHOWN/CHMOD/UTIME/STATVFS` | `mfs/protect.c stadir.c time.c` | Field-only ops; permission *checks* live in VFS, *mutations* here. |
| T-205 | Block family: `BREAD/BWRITE/BPEEK/BFLUSH/NEW_DRIVER/FLUSH` | `libminixfs/bio.c`; `libfsdriver/call.c` block group | `NEW_DRIVER` rebinds device label; `FLUSH` = per-fd cache flush; `BFLUSH` = device flush + invalidate. |
| T-300 | SIGTERM → `fs_sync` → `fsdriver_terminate` (`running=FALSE; sef_cancel`) → loop exits once unmounted | `mfs/main.c:70-78`; `fsdriver.c:64-70,75-88` | `while(running\|\|mounted)`: TERM alone does not exit a still-mounted server; unmount (VFS-driven) is the other half. |
| T-301 | Unmount leaf: busy-check (print only), release root, `fs_sync`, mark CLEAN, `bdev_close`, `lmfs_invalidate`, `s_dev=NO_DEV` | `mfs/mount.c:118-173` | VFS expects success either way; the busy print is integrity info, not a refusal. PFS unmount only warns (`pfs.c:118-130`). |

Order-difference table (teaching order vs runtime fact; required by §5.1 rule 2):

| # | Runtime fact (+anchor) | Teaching choice | Reason + back-pointer compensation |
|---|------------------------|-----------------|-------------------------------------|
| OD-1 | Birth (T-000..T-002) precedes the loop (T-100..) at runtime. | Framework loop (N01/N02) taught before birth assembly (N29). | 8 servers share one birth skeleton but 8 different table/state bodies; teaching birth first would forward-reference every framework type. N01 states the assumption up front ("assumes a born process; birth in N29"); N29 back-points to N01/N09 per server. |
| OD-2 | Framework walk (T-203 upper half, `lookup.c`) calls the leaf single-step at runtime. | Framework walk (N05) taught before leaf steps (N13 etc). | Leaf steps are variant-specific (7 variants); framework chapter declares only the leaf *contract* (signature + putnode discipline + error alphabet), cites N13 as first implementation. No leaf internals in N05. |
| OD-3 | Mount (T-003) needs superblock + inode table, which init (T-002) only zeroes. | Order N09 (startup/wiring) → N10 (super) → N11 (inode) → N12 (mount vertical). | Matches causal dependency (zero → parse → instantiate → serve); no difference, recorded for completeness. |
| OD-4 | `PUTNODE` (T-201) interleaves every lookup path at runtime. | Node lifecycle taught once in N11 (inode), referenced from N05/N13/N15. | Avoids repeating the count-1 discipline in 4 places; first occurrence (N11) is complete, later uses cite it. |

---

## 2. Knowledge Pool

Numbering `K-001..`; `S` = stock (from existing docs), `N` = new (from C/non-C/Rust, absent in old docs). `Primary` marks the single main-telling point; other locations become references after rebuild. Types: C=concept, M=mechanism, D=data structure, I=interface/protocol, V=constraint/invariant, A=arch-evolution, T=tooling/engineering, E=test-character.

### 2.1 Framework: driver, adapters, data channel, dentry, lookup (old 01/02/03 → new N01-N05)

| ID | Name | Type | S/N | Existing location(s) | Anchor | Reader benefit (answers) |
|----|------|------|-----|----------------------|--------|--------------------------|
| K-001 | Server event-loop skeleton (receive→classify→adapt→reply) | M | S | 01 §§1.1,2.2-2.3 | `libfsdriver/fsdriver.c:17-88` | Where does every FS server spend its life? |
| K-002 | Mounted-state gate (`mounted \|\| READSUPER`, else EINVAL) | V | S | 01 §§1.4,2.2 | `fsdriver.c:36-46` | Why is mount the only pre-mount request? |
| K-003 | transid pack/unpack (`TRNS_*`, low-16 id, high-16 code) | I | S | 01 §§1.2,2.5; 99 §1.2 | `vfsif.h:77-79` | How do async replies find their request? |
| K-004 | `IS_FS_RQ` prefix test | I | S | 01 §2.5 | `vfsif.h:76` | How is FS traffic told apart from VFS/PM traffic? |
| K-005 | 33 REQ names + `NREQS=34` + slot-0 unused | I | S | 01 §§1.2,2.5; 99 §1.1 | `vfsif.h:41-75` | What is the full operation alphabet? |
| K-006 | `REQ_GETNODE` dead constant ("Should be removed", no slot) | V | S | 01 §2.4; 05-vfs/12 (peer) | `vfsif.h:42`; `table.c:1-40` (absent) | Why does +1 always return ENOSYS? |
| K-007 | `REQ_MOUNTPOINT` has no slot (always ENOSYS via callvec) | V | S | 01 §2.4 | `table.c:1-40` (absent) | Which documented request can never dispatch? |
| K-008 | `fsdriver_callvec` 32-slot table + wrapping `call_nr-=FS_BASE` | D | S | 01 §§1.3,2.4 | `table.c:1-40`; `fsdriver.c:39` | How is dispatch O(1) and total? |
| K-009 | `fdr_other` non-FS path (no reply) | M | S | 01 §1.5 | `fsdriver.c:25-32` | Where do control/DS/notify messages go? |
| K-010 | `fdr_postcall` hook | M | S | 01 §2.2 | `fsdriver.c:60-61` | What runs after every reply? |
| K-011 | `fsdriver_terminate` + `while(running\|\|mounted)` exit condition | M | S | 01 §2.3 | `fsdriver.c:64-88` | Why doesn't SIGTERM alone exit a mounted server? |
| K-012 | `struct fsdriver` callback table (31 hooks) | I | S | 01 §2.7 | `libfsdriver/fsdriver.h:52-105` | What must a server implement, and what defaults to ENOSYS? |
| K-013 | Capability flags `RES_THREADED/HASPEEK/64BIT` | I | S | 01 §1.6; 99 §1.3 | `vfsif.h:19-22` | How does a server advertise async/peek/64-bit? |
| K-014 | Mount flags `REQ_RDONLY/ISROOT` | I | S | 01 §2.6; 10 §1.1 | `vfsif.h:8-9` | What does the mount message promise? |
| K-015 | Special codes `-301/-302/-303` (mount/symlink traversal) | I | S | 01 §2.6; 99 §1.3 | `vfsif.h:26-28` | How does path resolution restart in VFS? |
| K-016 | `vfs_ucred_t` credential struct | D | S | 01 §2.6 | `vfsif.h:31-37` | Who is asking, and with which groups? |
| K-017 | 31 adapters grouped in 6 families (mount/node/data/ns/meta/block) | M | S | 02 §§1.2,2.1-2.5 | `call.c` (1013) | What does each request extract, check, call, and return? |
| K-018 | Adapter validation triad (position/length/count) | V | S | 02 §§1.3,2.2,2.5 | `call.c` per-adapter checks | Which rejections happen before the leaf is touched? |
| K-019 | Dot-name universal law (per-request name rules) | V | S | 02 §1.4 | `call.c` name checks | What names are legal everywhere? |
| K-020 | Peek-via-read simulation + `grant=-1` convention | M | S | 02 §1.5 | `call.c` peek adapters; `05-vfs/12` peer | How do servers without peek still answer? |
| K-021 | Reply shapes (position advance + byte counts) | I | S | 02 §1.6 | `call.c` reply construction | What does each reply carry back? |
| K-022 | Adapter↔transport decoupling (Rust) | A | S | 02 §3.2 | `os/libs/minix-fs/src/call.rs` (880) | Why can adapters be tested without IPC? |
| K-023 | Caller-data channel (`copyin/copyout/zero`, two doors one corridor) | M | S | 03 §§1.1-1.2,2.1 | `utility.c:4-78` | How do bytes cross the protection boundary? |
| K-024 | `fsdriver_getname` four gates (grant→string) | M | S | 03 §§1.3,2.2 | `utility.c:83-105` | How does untrusted input become a usable name? |
| K-025 | Dentry staging encoder (`init/add/finish`) | M | S | 03 §§1.4,2.3 | `dentry.c:1-99` | How are getdents replies assembled in segments? |
| K-026 | Framework walk: `access_as_dir/next_name/resolve_link` + main loop | M | S | 03 §§1.5,2.4-2.5 | `lookup.c:1-333` | How is a long path walked one component at a time? |
| K-027 | Symlink depth cap (7) + tail reassembly | V | S | 03 §2.4 | `lookup.c:60-112` | When does a link storm stop? |
| K-028 | Putnode discipline on every walk return | V | S | 03 §§1.6,2.5 | `lookup.c:117-333` | Who releases the nodes the walk pinned? |
| K-029 | `PATH_RET_SYMLINK/GET_UCRED` option semantics | I | S | 03 §2.5; 01 §2.6 | `vfsif.h:11-17` | How does the caller ask for the link itself or pass creds by grant? |
| K-030 | Production transport + wire layout (Rust `fs-rt`) | I | N | — (no doc; code exists) | `os/fs/fs-rt/src/wire.rs` (657), `transport.rs` (459) | How do adapters reach the wire in production vs test? |
| K-031 | `NullDriver`/test doubles proving the trait | E | N | — (code only) | `os/libs/minix-fs/src/driver.rs:667`; `memfs.rs` (708) | What is the smallest thing that satisfies the framework? |

### 2.2 Block layer (old 04/05 → new N06/N07)

| ID | Name | Type | S/N | Existing | Anchor | Benefit |
|----|------|------|-----|----------|--------|---------|
| K-032 | `struct buf` + `NORMAL/PEEK/ONE_SHOT` fetch modes | D | S | 04 §§1.2,1.4,2.1 | `libminixfs.h:buf`; `cache.c:38-71` | What is cached, and with what intent? |
| K-033 | Hash (`BUFHASH`) + LRU (`front/rear`) + `count` pinning | M | S | 04 §§1.2,2.2,2.5-2.6 | `cache.c:44-71,298-608` | How is a block found, kept, and evicted? |
| K-034 | Dirty triple (`markdirty/markclean/isclean`) | M | S | 04 §§1.3,2.4 | `cache.c:164-177` | When does a modification become a writeback obligation? |
| K-035 | `ONE_SHOT` front-insert (single-use blocks) | M | N | — (code gap per todo V1-P2-8) | `cache.c:533-544` | Why do some blocks skip the LRU rear? |
| K-036 | Readahead (`readahead/readahead_limit/prefetch` bitmap-run) | M | S+N | 04 §§1.5,2.8 (partial) | `cache.c:987-1131` | Why does sequential read accelerate? |
| K-037 | Dirty ordered bulk write (by block number) | M | N | — | `cache.c:884-885` | In what order do dirty blocks hit the disk? |
| K-038 | `cache_resize/set_blocksize/buf_pool` pool management | M | S | 04 §§2.10 | `cache.c:1192-1293` | How big is the pool, and who may resize it at runtime? |
| K-039 | Heuristic sizing (`fs_bufs_heuristic` + `change_blockusage`) | M | S | 04 §§1.6,2.3 | `cache.c:73-162` | What sizes the cache from FS usage + free memory? |
| K-040 | Scatter/gather (`rw_scattered`, `bdev_gather`) | M | N | 04 §2.7 (named) | `cache.c:840-982` | How are multi-block transfers batched? |
| K-041 | VM second-level cache (flags word, tags, page handoff, 4 wire calls) | I | S | 04 §§1.5,2.x; 99 §3 | `cache.c:443-451` + `os/libs/minix-fs/src/vm_cache.rs` (1228) | How are hot pages shared with VM without copying? |
| K-042 | `lmfs_bio` chunked walk (partition trim, head/tail partial, whole middle) | M | S | 05 §§1.1-1.3,2.4 | `bio.c:116-263` | How does (dev,pos,len) become block ops? |
| K-043 | `block_prefetch` best-effort window | M | S | 05 §§1.4,2.3 | `bio.c:59-116` | What is prefetched, and what may silently fail? |
| K-044 | Whole-block overwrite skips read (no-fetch write) | V | S | 05 §1.5 | `bio.c` write path | When is the old bytes' read skipped? |
| K-045 | `lmfs_bflush` flush-then-invalidate | M | S | 05 §§1.6,2.5 | `bio.c:lmfs_bflush` | Why must close follow flush with invalidation? |
| K-046 | `lmfs_driver` label binding (major-only) | I | S | 05 §§1.7,2.2 | `bio.c:48-53` | How does a device number find its driver? |
| K-047 | `NEW_DRIVER` handling path | M | S | 05 §2.4 (in block group) | `call.c` block group + `bio.c` | What happens when the driver below changes? |
| K-048 | `bdev_bridge` + `PendingBlockSource` fail-closed seam | I | N | — (code, E-FSBDEV) | `os/libs/minix-fs/src/bdev_bridge.rs` (313) | Where does the FS block layer stop and the real driver begin? |

### 2.3 PFS minimal server (old 06 → new N08)

| ID | Name | Type | S/N | Existing | Anchor | Benefit |
|----|------|------|-----|----------|--------|---------|
| K-049 | PFS-first boot fact (first mounted FS) | C | S | 06 §1.1; 00 §1.3 | `vfs/main.c:510`; `kernel/table.c:62` | Why is pfs the minimal-server example? |
| K-050 | 512 static inode table + reverse-fill free list | D | S | 06 §§1.2,2.1-2.2 | `pfs.c:13-49,87-104` | How are pipe/clone nodes stored without a disk? |
| K-051 | No-root mount (zeroed node, symmetry only) | V | S | 06 §2.2 | `pfs.c:104-116` | What does "mount" mean for a rootless FS? |
| K-052 | 9-callback subset + table shape | I | S | 06 §§2.4-2.8 | `pfs.c:380-451` (table) | Which 9 of 31 hooks make a complete tiny server? |
| K-053 | Pipe read/write (stream, compaction, limits) | M | S | 06 §§1.4,2.6 | `pfs.c:217-298` | How do pipe bytes flow without blocks? |
| K-054 | Lazy time accounting (`i_update` ATIME/MTIME/CTIME) | M | S | 06 §1.5 | `pfs.c:18-22` | When are timestamps actually stamped? |

### 2.4 MFS reference implementation (old 07-17 → new N09-N20)

| ID | Name | Type | S/N | Existing | Anchor | Benefit |
|----|------|------|-----|----------|--------|---------|
| K-055 | MFS birth 4-step + signal-only-decides (Rust 2-step) | M | S | 07 §§1.1-1.2,2.1-2.3 | `mfs/main.c:14-78`; `os/fs/mfs/src/startup.rs` (178) | What turns an empty process into a ready server? |
| K-056 | `mfs_table` 31-hook wiring (which C fn serves which hook) | I | S | 07 §§1.3,2.4 | `mfs/table.c:13-45` | Where does each request land in mfs? |
| K-057 | `get_block/alloc_zone/free_zone` cache wrappers + hint | M | S | 07 §§1.4-1.5,2.5-2.7 | `mfs/cache.c:22-109` | What house rule sits atop the generic cache? |
| K-058 | Server assembly (`ServerCore`+`InodeTable`+`Superblock` → `FsDriver`) | D | N | — (code; todo V1-P0-1 closure) | `os/fs/mfs/src/server.rs` (1527) | How do 26 capabilities become one reachable server? |
| K-059 | On-disk panorama (boot/super/bitmaps/inodes/data) + 31-byte super format | D | S | 08 §§1.1,2.1 | `mfs/super.h:4-60` | What does the disk look like? |
| K-060 | Three magic gates (V2/V3 + rev-endian) | V | S | 08 §§1.2,2.7 | `mfs/super.c:241-355` | How is "my format" decided? |
| K-061 | Block-size 5-checks + first-zone computation + geometry sanity | V | S | 08 §§1.3-1.5,2.7 | `mfs/super.c:241-355` | Which numbers are rejected before touching bitmaps? |
| K-062 | Bitmap ledger (`alloc_bit/free_bit`, imap/zmap, search hint) | M | S | 08 §§1.6,2.3-2.4 | `mfs/super.c:29-156` | How is one bit one block accounted? |
| K-063 | CLEAN flag + unclean→readonly downgrade | V | S | 08 §1.7; 10 §1.1 | `mfs/super.c`; `mfs/mount.c:33-48` | What does an unclean shutdown cost at next mount? |
| K-064 | 512-slot + 128-bucket inode table, `i_count` borrowing | D | S | 09 §§1.1-1.3,2.1 | `mfs/inode.h:20-50`; `inode.c:70-174` | How do disk bytes become working objects? |
| K-065 | `get/find/put/dup/alloc/wipe/free` lifecycle + hash add/remove | M | S | 09 §§1.4-1.5,2.5-2.11,2.14 | `mfs/inode.c:96-343,455-463` | Who may hold an inode, and who releases it? |
| K-066 | `fs_putnode` count-1 batch discipline | V | S | 09 §2.3 | `mfs/inode.c:38-64` | Why does release subtract one more than the count? |
| K-067 | `d2_inode` 64-byte disk codec (`rw_inode/new_icopy`) + lazy time | M | S | 09 §§1.1,1.6,2.2,2.12-2.13 | `mfs/type.h:8-17`; `inode.c:349-449` | How is the 64-byte record encoded and when stamped? |
| K-068 | Mount vertical (7 steps: open→super→downgrade→blocksize→usage→root→dirty) | M | S | 10 §§1.1,2.x | `mfs/mount.c:11-90` | How does a disk become a usable tree? |
| K-069 | Unmount vertical (busy-print→put-root→sync→clean→close→invalidate→NO_DEV) | M | S | 10 §1.x | `mfs/mount.c:118-173` | How does a tree leave gracefully? |
| K-070 | `fs_mountpt` gate (busy/special/dir checks) | V | S | 10 §2.x | `mfs/mount.c:93-116` | What may be mounted upon? |
| K-071 | `struct direct` 64-byte dirent + 4-mode walk (`advance/search_dir`) | D+M | S | 11 §§1.x,2.x | `mfs/mfsdir.h:20`; `path.c:241` | How are names stored, entered, and deleted? |
| K-072 | Single-step `fs_lookup` + empty-name/deleted-dir rules | M | S | 11 §2.x | `mfs/path.c` | What does one path component resolve to? |
| K-073 | Create quartet (lookup-empty→alloc→write→enter) + per-step rollback | M | S | 12 §§1.x,2.x | `mfs/open.c:270` | How is a name born as a file, and how does each failure unwind? |
| K-074 | `new_node` + `fs_seek` (seek only marks) | M | S | 12 §2.x | `mfs/open.c` | Where do mkdir/mknod/slink share code? |
| K-075 | Namespace mutation set (link/unlink/rmdir/rename/slink/rdlink) + guarantees | M | S | 13 §§1.x,2.x | `mfs/link.c:638` | What relation changes are legal between names and files? |
| K-076 | Truncate *decision* (plan: which zones go) vs *execution* (write path frees) | M | S | 13 §plan; 15 §exec | `link.c` plan fns + `write.c` `write_map(WMAP_FREE)` | Where is the plan/execution seam? (Moved wholly to N17 by this blueprint.) |
| K-077 | Block-number translation (direct/single/double-indirect cascade) | M | S | 14 §§1.x,2.x | `mfs/read.c:89-111,156-...` | Which disk block holds byte N? |
| K-078 | Hole reads zero without allocating | V | S | 14 §hole | `read.c` read path | Why does read never grow a file? |
| K-079 | `fs_getdents` batched enumeration + cold-dir behavior | M | S | 14 §getdents | `read.c` getdents | How is a directory listed in chunks? |
| K-080 | Write cascade (missing blocks grow; indirect levels built one by one) | M | S | 15 §§1.x,2.x | `mfs/write.c:319` (`write_map/new_block/wr_indir/empty_indir`) | How do holes become allocations? |
| K-081 | Whole-block no-read + zeroing (`clear_zone/zero_block`) | M | S | 15 §no-read | `write.c` | When is the old block not read? |
| K-082 | Length-change timing (when `i_size` moves) | V | S | 15 §length | `write.c` | At which point does the file grow? |
| K-083 | Permission/owner/time mutation (`chmod/chown/utime`, setuid-clear, 9-bit mask) | M | S | 16 §§protect/time | `mfs/protect.c:58`; `time.c:49` | How is descriptive info changed, and what is masked? |
| K-084 | `stat/statvfs` queries + free-bit counting (bounded, no overrun) | M | S | 16 §stadir; 17 §stats | `stadir.c:104`; `stats.c:89` | How is "how much is free" counted without counting padding? |
| K-085 | `conv2/conv4` endian conversion + native flag | M | S | 16 §utility | `utility.c:36` | How do disk bytes survive a foreign endianness? |
| K-086 | `fs_sync` order (inodes first, then blocks) + why | V | S | 17 §§1.1 | `misc.c:23` + cache flush | Why does order match the dependency direction? |
| K-087 | Read-only dirty-mark tripwire (block + inode guards) | V | S | 17 §§1.3; 09/16 guards | `clean.h:14`; `mfs_cache` guards | What fires when a read-only mount tries to dirty? |
| K-088 | Constant directory (`const.h` 68 lines: authority per group) | T | S | 17 §§1.4 | `mfs/const.h` | Where does each constant authoritatively live? |
| K-089 | Global-state disposition (5 names → owners) | T | S | 17 §§1.5 | `mfs/glo.h:22` | What happened to each global? |

### 2.5 Virtual-tree branch (old 18/19/20 → new N21-N23)

| ID | Name | Type | S/N | Existing | Anchor | Benefit |
|----|------|------|-----|----------|--------|---------|
| K-090 | In-memory tree (identity/meta/position/hash-chain 4 field groups; short-name inline) | D | S | 18 §§1.2-1.3 | `libvtreefs/inode.c:626`; `inode.h` | How is a diskless tree stored? |
| K-091 | Dual hashes (by-name sdbm + by-index) + bucket=count | M | S | 18 §§1.3 | `sdbm.c:30`; `inode.c` | How are both lookup directions fast? |
| K-092 | Two-phase delete (unlist now, reclaim at zero) + parent chain | M | S | 18 §§1.4-1.5 | `link.c:129`; `inode.c` | Why can `.`/`..` still walk out of a deleted dir? |
| K-093 | Hooks (content/dir-refresh/stat-change) + static-vs-dynamic nodes | I | S | 18 §§1.1,1.6-1.7 | `vtreefs.c:110`; `extra.c:56` | Where does framework end and content begin? |
| K-094 | procfs static table recursion + 7 root files (+cpuinfo on x86) | M | S | 19 §§1.1 | `procfs/tree.c:462`; `root.c:227` | How is the static tree grown at startup? |
| K-095 | Two-pass pid refresh (delete pass + create pass; lazy lookup vs eager list) | M | S | 19 §§1.2-1.3 | `procfs/pid.c:230` | How does the tree stay isomorphic to the snapshot? |
| K-096 | Whole-render window-slice output staging (`buf.c`) + hand-rolled decimal | M | S | 19 §§1.4 | `procfs/buf.c:125` | How is file content generated on read? |
| K-097 | Loadavg/uptime/hz generators (fixed-point, tick arithmetic) | M | S | 19 §§1.5-1.6 | `procfs/util.c` + `cpuinfo.c:153` | How are numbers rendered without libc? |
| K-098 | procfs service subtree (RS dialogue family) | M | S+N | 19 §service (thin) | `procfs/service.c:348`; `os/fs/procfs/src/service.rs` | What lives under `/service`? |
| K-099 | ptyfs fixed 32-node table + bitmap, numeric names, bidirectional conversion | D | S | 20 §§1.1-1.2 | `ptyfs/ptyfs.c:434`; `node.c:84` | How do driver add/del requests become numbered files? |
| K-100 | ptyfs control gate (service-label auth → act → sync/async reply) | I | S | 20 §§1.6 | `ptyfs.c` other-path | Why must creation be synchronous? |

### 2.6 Disk/host variants (old 21-24 → new N24-N28)

| ID | Name | Type | S/N | Existing | Anchor | Benefit |
|----|------|------|-----|----------|--------|---------|
| K-101 | Block groups (per-group bitmaps/tables, locality unit) | D | S | 21 §§1.1 | `ext2/super.c:459` | How is the disk cut into autonomous chunks? |
| K-102 | 4 placement policies (Orlov/hash/old/first-fit) + dir-spread/file-cling | M | S | 21 §§1.2-1.3 | `ext2/balloc.c:362`; `ialloc.c:476` | Where do new blocks/inodes live, and why? |
| K-103 | ext2 super validation (magic/size/inode-size/feature 3 gates + dirty-state mount fate) | V | S | 21 §§1.4 | `ext2/super.c` | When is a disk rejected, downgraded, or accepted? |
| K-104 | 6 mount options (super-pos/Orlov/old/ref/reserve/prealloc) + LE assert | I | S | 21 §§1.5 | `ext2/main.c:111`; `misc.c:35` | Which knobs change allocator choice? |
| K-105 | Varlen dirent (self-length, delete-merge, no holes) + length-first decode | D | S | 22 §§1.1 | `ext2/path.c:314` (via `dir.rs` 203) | How do names pack without waste, and what breaks the walk? |
| K-106 | 128-byte inode record (15 pointers) + sector-count duality | D | S | 22 §§1.2 | `ext2/inode.c:420` (via `inode.rs` 145) | Where do mode/owner/times live in ext2? |
| K-107 | Triple-indirect decomposition (direct12 + single/double/triple, cubic cap) | M | S | 22 §§1.3 | `ext2/read.c:557`/`write.c:376` (via `mapping.rs` 150) | How are offsets decomposed one level deeper than mfs? |
| K-108 | Fast symlink (target <60B in pointer area, no block) | M | S | 22 §§1.4 | `ext2/link.c:656` (shape note) | When does a link cost no data block? |
| K-109 | Volume discovery (20-sector scan, sig+version+blocksize 3 gates, root extent) | M | S | 23 §§1.1-1.2 | `isofs/super.c:121`; `mount.c:66` | How is a disc found? |
| K-110 | Dual-endian records (read LE half) + zero-len = pad-to-EOB | D | S | 23 §§1.3 | `isofs` record parsing (`record.rs` 206) | How are directory records decoded? |
| K-111 | Extent runs (interleave-aware block landing) | M | S | 23 §§1.4 | `isofs/read.c:105` | Which sector holds file block N? |
| K-112 | Rock Ridge tail (NM/PX/SL/RE entries, unknown-skip, bound-checked concat) | M | S | 23 §§1.5 | `susp.c:132`; `susp_rock_ridge.c:289` | How is Unix personality patched onto ISO9660? |
| K-113 | Read clamp + chunked loop + position semantics (isofs) | M | S | 23 §§1.6 | `read.c`; `lib.rs:91` (clamp) | How is a read-only file served? |
| K-114 | SFFS 15-op narrow table (6 groups; guest calls table, host fills table) | I | S | 24 §§1.1 | `libsffs/table.c:28`; `proto.h` | Where is the guest/host firewall? |
| K-115 | SFFS options (prefix/owner/masks/case) + mask-is-subtraction | I | S | 24 §§1.2 | `mount.c:89`; `params.rs` (108) | What identity does the guest show, and why only less permission? |
| K-116 | Guest-chain→host-path join (right-to-left equivalent) + pop/push bounds | M | S | 24 §§1.3 | `path.c:108`; `path.rs` (132) | How is a guest node spelled as a host path? |
| K-117 | Case-fold compare+hash pairing | V | S | 24 §§1.4 | `name.c:52`; `name.rs` (44) | Why must hash fold exactly when compare folds? |
| K-118 | Freshness tribunal (guilty-until-verified: delete/stale/passthrough/fresh) | M | S | 24 §§1.5 | `verify.c:118`; `verify.rs` (118) | When is a cached node trusted? |
| K-119 | Lazy-open quiet-close handles (dir/file split, ro-fallback) | M | S | 24 §§1.6 | `handle.c:77`; `handles.rs` (52) | Why are round-trips deferred until use? |
| K-120 | vbfs vs hgfs assembly (options + host-lib init + same framework loop) | M | S | 24 §§assembly | `vbfs.c:141`; `hgfs.c:106` | How do two bridges share one framework? |

### 2.7 New knowledge absent from old docs (all N, grounds for new chapters)

| ID | Name | Anchor | Destined chapter |
|----|------|--------|------------------|
| K-121 | SEF birth per server (init-fresh/restart/signal sets differ) | 8× `main.c` (mfs read; others per-chapter cite) | N29 |
| K-122 | RS load vs boot_image membership per server | `kernel/table.c`; `fs/Makefile`; `vfs/main.c:510-516` | N29 (+N00 chain) |
| K-123 | `fs-rt` wire/transport/source/ipc modules | `os/fs/fs-rt/src/*.rs` (2956 lines total) | N29 |
| K-124 | `mfs server.rs` assembly + assembly smoke (mount→lookup→read→write→sync) | `os/fs/mfs/src/server.rs` (1527) | N09 |
| K-125 | `dir_io.rs` bridge (dir mirror ↔ cache load/store) | `os/fs/mfs/src/dir_io.rs` (413) | N13 |
| K-126 | `second_level.rs` (mfs side of vmcache) | `os/fs/mfs/src/second_level.rs` (564) | N06 |
| K-127 | `vtreefs driver.rs` (TreeServer FsDriver impl) | `os/libs/minix-vtreefs/src/driver.rs` (475) | N21 |
| K-128 | sffs `do_*` dispatch + inode/lookup core (Rust missing; C exists) | `libsffs/inode.c:292 lookup.c:150 link.c:366 read.c:173 write.c:131 stat.c:176` | N27 (explicit gap) |
| K-129 | ext2 data-path execution (balloc/ialloc exec, rw_chunk, link/truncate/protect exec — Rust missing) | `ext2/balloc.c ialloc.c link.c read.c write.c` | N24/N25 (explicit gap) |
| K-130 | isofs lookup/stat/getdents/inode-cache/mount-lifecycle (Rust missing) | `isofs/inode.c:492 path.c:76 stadir.c:27` | N26 (explicit gap) |
| K-131 | bdev fail-closed seam + E-FSBDEV handoff | `bdev_bridge.rs` (313); edge E-FSBDEV | N07 |
| K-132 | Shutdown/exit vertical per server (TERM+unmount two halves) | `fsdriver.c:64-88`; `mfs/mount.c:118-173` | N20 (new consolidation) |
| K-133 | Concurrency invariant (single-threaded loop; no locks in cache/inode tables) | `driver.rs` docs; `task.rs` (1197) | N01 (+N06/N11 echo by ref) |
| K-134 | Test assets per crate (single-count table; 388 total per 99 §2) | `cargo test -p <crate>`; `99 §2` | N99 |
| K-135 | fsck/mkfs command surface (claimed by E-FSCMDS, owned by commands stage) | edge E-FSCMDS | N99 omissions + §7 |

Statistics: 135 knowledge items (101 S, 34 N). By type: C 3, M 62, D 16, I 24, V 21, A 1, T 4, E 4. By existing doc: 01:16, 02:6, 03:7, 04:10, 05:7, 06:6, 07-17:35, 18-20:11, 21-24:20, 99:3, none:14 (the N-residue that forces N27-gap/N29/N07-seam/N20/N99 entries). Duplicated-telling candidates: lookup (K-026 vs K-072/K-094/K-099/K-105/K-110), getdents (K-025 vs K-079), constants (K-088 vs K-005/K-013/K-014/99-table), free-count (K-084 vs K-062), readonly-guard (K-087 vs K-063/K-070), mount-flags (K-014 vs K-068). Primary assignments in §3.3.

---

## 3. Coverage Audit

### 3.1 Topic universe (four sources)

- TU-A (C symbols): all functions/structs/macros/constants/state machines/error paths in the section 0.2 C list (about 230 symbols; full enumeration is B-phase material-gathering duty using per-file coverage matrices already present in old docs sections 2.9/2.7 — those matrices are reused as checklists, not as prose).
- TU-B (OS concepts): process birth, single-threaded server, mount namespace, path resolution, credentials, block caching, writeback ordering, allocation, truncate, permissions, stat, sync/shutdown, virtual trees, host bridging, wire protocol, error taxonomy.
- TU-C (non-C artifacts): §3.4 ten fixed items.
- TU-D (boundary contracts): VFS drives all requests (05-stage-vfs); RS loads non-boot servers (03-stage-rs/master-plan); VM owns page handoff peer (02-stage-vm); block drivers own media (16-stage-drivers); commands own fsck/mkfs (18-stage); devman/gpio reuse vtreefs (11-stage); kernel owns boot_image slots (01-stage-kernel).

### 3.2 Coverage-gap table (topic in universe, no telling point)

| # | Topic | Evidence anchor | Disposition → new knowledge → chapter |
|---|-------|-----------------|----------------------------------------|
| G-01 | FS birth assembly per server + `fs-rt` production transport (2956 lines, zero docs) | `os/fs/fs-rt/src/*.rs`; 8× `main.c` | NEW chapter N29; K-121/122/123 |
| G-02 | `mfs server.rs` assembly (1527 lines; 26 capabilities → one `FsDriver`) | `os/fs/mfs/src/server.rs` | Merge into N09; K-058 |
| G-03 | `dir_io.rs` dir-mirror↔cache bridge (413 lines) | `os/fs/mfs/src/dir_io.rs` | Merge into N13; K-125 |
| G-04 | `second_level.rs` mfs-side vmcache + `vm_cache.rs` 1228-line body beyond old 04 §2.x | `second_level.rs` (564); `vm_cache.rs` (1228) | Expand N06; K-126 |
| G-05 | `vtreefs driver.rs` TreeServer impl (475 lines) | `minix-vtreefs/src/driver.rs` | Expand N21; K-127 |
| G-06 | sffs `do_*` full dispatch + inode/lookup/read/write/stat/link exec (C exists, Rust 5/15 files) | `libsffs/inode.c lookup.c link.c read.c stat.c write.c` vs `minix-sffs/src/` (5 files) | N27 documents C fully, marks Rust gap explicitly (staged, not defect); K-128 |
| G-07 | ext2 exec blocks (alloc exec, rw_chunk/rahead, link/truncate/protect/time exec) | `ext2/balloc.c ialloc.c link.c read.c write.c protect.c` vs `os/fs/ext2/src/` (7 files, parse-layer only) | N24/N25 diff + explicit staged-gap ledger; K-129 |
| G-08 | isofs exec blocks (lookup/search, stat date7, getdents dispatch, inode cache, mount lifecycle) | `isofs/inode.c path.c stadir.c mount.c` vs `os/fs/isofs/src/` (4 files) | N26 same treatment; K-130 |
| G-09 | bdev seam fail-closed (`PendingBlockSource`) + E-FSBDEV | `bdev_bridge.rs`; edge E-FSBDEV | Expand N07; K-131 |
| G-10 | Shutdown vertical consolidated (TERM + unmount + CLEAN + invalidate per server) | `fsdriver.c:64-88`; `mfs/mount.c:118-173` | NEW consolidation N20 (collects old 17-misc + per-server signal/maint); K-132 |
| G-11 | `const.h`/`glo.h`/`clean.h` authority catalog (68+22+14 lines) scattered as prose in old 17 | `mfs/const.h glo.h clean.h` | Move to N99 appendix (single authority); old 17 stops repeating constants; K-088/089 |
| G-12 | Truncate planning homeless (old 13 plans, old 15 executes; neither owns the seam) | `link.c` plan fns; `write.c WMAP_FREE` | Consolidate wholly in N17; N15 renamed namespace-only; K-076 |
| G-13 | Concurrency invariant stated once, used everywhere (cache/inode/vtree/sffs assume single-thread) | `task.rs`; `driver.rs` docs | State normatively in N01, cite by ref elsewhere; K-133 |
| G-14 | ONE_SHOT front-insert, bitmap-run prefetch, dirty-by-number bulk order, runtime resize | `cache.c:533-544,1089-1130,884-885,1192-1293` | Expand N06 (were named-but-unexplained); K-035/037/038 |
| G-15 | isofs subpage-block peek gaps (`#if 0` ×2, "FIXME") | `isofs/table.c` two `#if 0` | N26 documents as known C gap (not Rust defect); no new code |
| G-16 | ptyfs `signal/other` event-loop wiring (deferred to runtime track) | `ptyfs.c` other-path; edge E-FSRUNTIME | N23 documents contract, marks production wiring as E-FSRUNTIME; not a doc gap after marking |

### 3.3 Duplication table (same topic told in ≥2 places → keep primary, others become references)

| # | Topic | Current tellings | Primary after rebuild | Others → |
|---|-------|------------------|-----------------------|----------|
| D-01 | Framework vs leaf lookup | 03 §1.5/2.4-2.5 + 11 + 19 + 20 + 22 + 23 | N05 = framework walk ONLY (contract + mount/symlink/ucred); leaves = single-step ONLY | N13/N22-content/N23/N25/N26 each cite N05, never re-explain walk |
| D-02 | getdents assembly | 03 §1.4/2.3 + 14 §getdents + 20/22/23 enumerations | N04 = encoder ONLY; N16 = mfs enumeration logic; variants cite both | Remove encoder prose from N16+ |
| D-03 | Constant values | 01 §2.5-2.6 + 08 §2.2 + 09-notes + 17 §1.4 + 99 §1 | N99 = values ONLY; all users cite N99 | Old 17 §1.4 deleted (moved) |
| D-04 | Free-bit counting | 08 §2.3-2.4 (ledger) + 16 §statvfs + 17 §stats | N10 = ledger; N19 = query that calls it | N19 cites N10, no re-derivation |
| D-05 | Read-only guards | 09/16/17 guard prose + 10 downgrade | N20 = guard system ONLY (downgrade + tripwires + CLEAN) | N10/N11/N18 cite N20 |
| D-06 | Mount flags/ 
...[truncated 20862 chars]