# 07-stage-ds Documentation Rebuild Blueprint (muse)

## 0. Metadata

- Executor: muse (AI agent name = muse)
- Date: 2026-09-20
- Target directory: `rewrite-notes/07-stage-ds`
- Repository root: `/home/xzhao/github/minix-rs`
- HEAD commit at survey time: `81699c841` ("feat(commands): S35 batch 21")
- Task: R-phase rebuild blueprint. Output is ONLY this file:
  `rewrite-notes/07-stage-ds/doc_rerank_muse.md`.
  No body text was modified, renamed, moved, or deleted.
- Language: English (per muse special requirement).

### 0.1 Scope

In scope (numbered docs + global-concept doc):

| # | File | Lines | Header claim (what / not-what) |
|---|------|-------|-------------------------------|
| 00 | `00-ds-overview.md` | 70 | Navigation + boot two-level semantics; no mechanism detail |
| 01 | `01-ds-init-main.md` | 167 | `main` / SEF startup / get_work / reply / loop skeleton; not handlers, not message fields |
| 02 | `02-ds-message-contract.md` | 156 | Call numbers, `mess_ds_req`/`mess_ds_reply`/`union ds_val`, DSF flags, grant rules; not table shape, not auth, not client wrappers |
| 03 | `03-ds-data-structures.md` | 161 | Two tables, 192-byte cross-server contract; not slot search, not identity, not heap policy |
| 04 | `04-ds-slot-management.md` | 158 | Six slot primitives; not entry shape, not identity, not handler call sites |
| 05 | `05-ds-identity-auth.md` | 137 | Endpoint<->name translation + selective-protection auth; not boot owner source, not handler call sites |
| 06 | `06-ds-boot-mapping.md` | 138 | `sef_cb_init_fresh`, rproctab grant copy, `map_service`, STATEFUL restart; not SEF registration (01), not label retrieval (08/09) |
| 07 | `07-ds-publish.md` | 142 | `do_publish` full path; not retrieve/delete, not notify internals |
| 08 | `08-ds-retrieve.md` | 124 | `do_retrieve` + `do_retrieve_label`; not publish, not subscribe |
| 09 | `09-ds-delete.md` | 135 | `do_delete` + label cascade; not notify internals |
| 10 | `10-ds-subscribe-check.md` | 170 | `do_subscribe` + `do_check` + match/update helpers + journey diagram; not client wrappers |
| 11 | `11-ds-getsysinfo.md` | 129 | `do_getsysinfo` + IS consumer contract; not 192-byte rationale (03) |
| 12 | `12-ds-client-library.md` | 137 | `libsys/ds.c` 16 APIs, grant packing, NUL discipline; not server internals, not `_taskcall` impl |
| 99 | `99-ds-global-concepts.md` | 66 | Constants, errno set, cross-service references; no mechanism detail |

Total: 14 docs, 1890 lines. All short; no document anywhere near the soft size concern
(a single-concept doc may run to ~3000 lines only if the concept demands it — nothing here does).

Reference material (read, not rebuilt): `plan.md` (332 lines, design baseline with ARCH A-1..A-10),
`todo.md` (first-round arch review + Fix #1..#8 execution log, all closed 2026-09-15),
`draft/README.md` (old placeholder) + `draft/tmp_main.c.md` + `draft/tmp_store.c.md` (line-by-line素材).
Out of scope and never cited: `.design/` and `tmp_design_and_todo/` contents
(existence of `07-stage-ds/.design/00-design.v1.md` noted from directory listing only; not opened).

Other-AI products: `doc_rerank_deepseek.md` / `doc_rerank_glm.md` / `doc_rerank_qwen.md`
exist in this directory. Their contents were NOT read (only filenames + line counts were
incidentally visible in a `wc -l` directory listing during Step 0 inventory).

### 0.2 Read list

1. All 14 docs: header declarations fully read; body read at header + targeted-section depth
   (concept §1, C-analysis §2 openings, design-decision tables) plus full-text `grep` for
   `update_subscribers`, `check_sub_match`, cross-doc numeric references, and §-level structure.
2. C ground truth, fully read: `minix3/minix/servers/ds/main.c` (132),
   `store.c` (679), `store.h` (38), `inc.h` (33), `proto.h` (20),
   `minix3/minix/include/minix/ds.h` (74), `minix3/minix/lib/libsys/ds.c` (219).
3. C ground truth, excerpt-read with anchors: `include/minix/com.h` (DS_RQ_BASE block :498-507,
   `is_notify` :90-93, endpoint numbers :60-70), `include/minix/ipc.h`
   (`union ds_val` :94-99, `mess_ds_reply` :100-105, `mess_ds_req` :107-115, MessageUnion arms :2420-2421),
   `include/minix/sysinfo.h` (SI_* :12-18), `include/minix/sef.h` (`sef_init_info_t` :44-53,
   `SEF_CB_INIT_RESTART_STATEFUL` :85), `include/minix/rs.h` (`struct rprocpub` :165-183),
   `minix3/minix/servers/ds/Makefile` (full, 15 lines), `minix3/minix/kernel/table.c` (:44-64),
   `minix3/minix/kernel/main.c` (:190-200, :260-270), `minix3/minix/servers/is/dmp_ds.c` (full, 60 lines),
   `minix3/minix/lib/libsys/getsysinfo.c` (:1-40), `minix3/minix/lib/libmagicrt/magic_ds.c` (transfer
   callback excerpt), `minix3/sys/sys/errno.h` (`EDONTREPLY` definition site).
4. Non-C artifacts: all itemized in §3.5.
5. Boundary material: `00-master-plan/README.md` (stage table + boot two-level semantics),
   `edge_todo.md` (E-DSWIRE, E-MINTYPES-SYS DS items, E5(f) DS litmus chains — excerpted),
   `06-stage-sched/00-sched-overview.md` (previous stage; file list only, to fix the start point),
   `01-stage-kernel/06-todo.md` (§1-§3: contract-writing style reference only, no content reuse).
6. Rust implementation: `os/servers/ds/src/` file list + line counts (22 files, 5877 lines),
   doc-number references in comments (`grep`), test result `cargo test -p minix-ds`
   (workspace root `os/`): **112 passed / 0 failed** — matches the 00 doc's claim.

Key commands run (read-only): `wc -l`, `grep -rn`, `sed -n`, `git log --oneline -3`,
`cargo test -p minix-ds` (in `os/`), `ls`. Full outputs are embedded as anchors below.

---

## 1. C True Order (runtime truth, rebuilt from source, not paraphrased)

Stage-type verdict: **service event-loop type** (§9 of the R prompt). Output is two segments:
a boot segment (birth + init + entering the loop) and a loop segment
(receive -> dispatch -> handle -> reply). There is no linear startup chain past `sef_startup()`,
and no syscall/dispatch-table collection beyond the 7-arm switch. Request lifecycle
(publish -> notify -> check -> retrieve -> delete) is the pedagogical main line;
source call order is the factual baseline recorded in the order-delta table (§4.5).

### 1.1 Boot segment (birth -> init -> loop entry)

| Step | Action | Anchor | Note |
|------|--------|--------|------|
| T-1 | Boot image reserves the slot: `{DS_PROC_NR, "ds"}` is the first user service after 5 kernel tasks | `minix3/minix/kernel/table.c:52` (`:44-64` array) | Registration order, NOT execution order |
| T-2 | Kernel inhibits scheduling for non-VM services (`RTS_VMINHIBIT \| RTS_BOOTINHIBIT`) until VM builds page tables | `minix3/minix/kernel/main.c:265-267` (schedulable set `:196-200`: kernel tasks + RS + VM only) | DS actually runs after VM/RS despite T-1 |
| T-3 | `main(argc, argv)`: `env_setargs` | `servers/ds/main.c:38` | Startup args |
| T-4 | `sef_local_startup()`: register fresh-init callback, stateful-restart callback, live-update hook; `sef_startup()` | `main.c:93-104` (`sef_setcb_init_fresh`, `sef_setcb_init_restart(SEF_CB_INIT_RESTART_STATEFUL)`, `sef_llvm_ds_st_init`) | Three registrations; restart keeps table state |
| T-5 | `sef_cb_init_fresh()`: zero all `ds_store`/`ds_subs` flags | `store.c:260-266` | Table reset |
| T-6 | Copy `rprocpub[NR_BOOT_PROCS]` from RS via `sys_safecopyfrom(RS_PROC_NR, info->rproctab_gid, ...)`; panic on failure | `store.c:269-272`; `struct rprocpub` in `include/minix/rs.h:165-183`; `rproctab_gid` field in `include/minix/sef.h:44-53` | Grant-based boot table pull |
| T-7 | For each in-use entry: `map_service()` — alloc slot, `key=rpub->label`, `u.u32=endpoint`, `owner="rs"`, `flags=IN_USE\|TYPE_LABEL`, `update_subscribers(dsp,1)`; panic on failure | `store.c:229-249` (body `:235-248`), call loop `:273-279` | Genesis labels; notify ring runs (normally empty) |
| T-8 | Enter main loop | `main.c:42` | Boot segment ends |

### 1.2 Loop segment (one iteration)

| Step | Action | Anchor | Note |
|------|--------|--------|------|
| L-1 | `get_work(&m)`: `sef_receive(ANY, m)`; panic on failure; set globals `who_e=m_source`, `callnr=m_type` | `main.c:109-118`, globals `:15-16` | Single-threaded; two globals are the whole "current request" state |
| L-2 | If `is_notify(callnr)`: warn + `result=EINVAL`, skip to reply | `main.c:47-51`; `is_notify` = `(a-NOTIFY_MESSAGE)<0x100` in `com.h:90-93`, `NOTIFY_MESSAGE=0x1000` | DS never processes notify bodies |
| L-3 | Dispatch: `DS_PUBLISH->do_publish` / `DS_RETRIEVE->do_retrieve` / `DS_RETRIEVE_LABEL->do_retrieve_label` / `DS_DELETE->do_delete` / `DS_SUBSCRIBE->do_subscribe` / `DS_CHECK->do_check` / `DS_GETSYSINFO->do_getsysinfo` / default: warn + `EINVAL` | `main.c:53-78`; call numbers `com.h:498-507` (`DS_RQ_BASE=0x800`) | Source order is PUBLISH, RETRIEVE, SUBSCRIBE, CHECK, DELETE, RETRIEVE_LABEL(snapped: +5 `DS_SNAPSHOT` has NO arm), GETSYSINFO |
| L-4 | Reply unless `EDONTREPLY`: `m.m_type=result; reply(who_e,&m)` = `ipc_send`; send failure only warns | `main.c:80-85`, `reply` `:123-131`; `EDONTREPLY` defined in `minix3/sys/sys/errno.h` | No current handler returns `EDONTREPLY`; branch kept by convention |

### 1.3 Handler internals (factual order inside each arm; the teaching order deliberately differs — see §4.5)

- `do_publish` (`store.c:287-378`): source-name lookup (`ds_getprocname`, NULL->`EPERM` :297-299) ->
  LABEL-only-RS gate (:302) -> `get_key_name` (:306) -> `lookup_entry` + label-by-number second chance
  (:310-313) -> alloc (`ENOMEM`) / overwrite-gate (`EEXIST`/`EPERM` :315-326) -> 4-type store
  (U32 :331, LABEL :334, STR/MEM malloc/reuse/safecopyfrom/NUL-pin :336-364, unknown `EINVAL` :366) ->
  `flags = IN_USE \| (flags & INTERNAL)` (:372) -> `update_subscribers(dsp,1)` (:375).
- `do_retrieve` (`:383-427`): key (:393) -> lookup (`ESRCH` :397) -> `check_auth(RETRIEVE)` (`EPERM` :399) ->
  per-arm output incl. `MIN(want,have)` truncation + `val_len` writeback (:403-421), unknown arm `EINVAL` (:423).
- `do_retrieve_label` (`:432-451`): lookup by endpoint (`ESRCH`) -> safecopyto key+c-string. No auth gate.
- `do_subscribe` (`:456-531`): owner (NULL->`ESRCH`) -> existing-sub `EEXIST`/overwrite-free (:471-477) ->
  alloc (`EAGAIN` :480-481) -> key + `^...$` anchoring (:487-490) -> `regcomp(REG_EXTENDED)` (`EINVAL` :493-498) ->
  type mask default-all (:501-503) -> write slot + zero bitmap (:505-508) -> optional `DSF_INITIAL` instant
  sweep + notify (:511-528).
- `do_check` (`:536-578`): owner/sub lookup (`ESRCH`) -> first set bit (`ENOENT` :557-558) ->
  safecopyto key -> write back `m_ds_req.flags/owner` (:571-572) -> `UNSET_BIT` (:575, after successful copy).
- `do_delete` (`:583-648`): source (NULL->`EPERM`) -> key -> lookup (`ESRCH`) -> owner-equality
  (NOT `check_auth`, :606) -> per-type teardown (U32 nop; LABEL cascade subs-then-entries with per-victim
  `update_subscribers(...,0)` :613-631; STR/MEM `free` :635; unknown `EINVAL`) ->
  `update_subscribers(dsp,0)` (:642) -> `flags=0` (:645). Ring-before-clear order.
- `do_getsysinfo` (`:653-678`): `what==SI_DATA_STORE`? (`EINVAL`) -> `size==sizeof(ds_store)` exactly?
  (`EINVAL`) -> `sys_datacopy(SELF->caller)` whole table.
- Helpers: `alloc_data_slot` (:11-22), `alloc_sub_slot` (:27-38), `free_sub_slot` (assert+`regfree`+zero :43-52),
  `lookup_entry` (flag+type+name :57-70), `lookup_label_entry` (flag+LABEL+number :75-88),
  `lookup_sub` (flag+owner :93-105), `ds_getprocname` (self->"ds" / label-reverse / NULL :110-125),
  `ds_getprocep` (label-forward + `panic` :130-138), `check_auth` (gate-unset->allow; else owner-compare :143-153),
  `get_key_name` (length 2..80 + safecopyfrom + tail-pin :158-181),
  `check_sub_match` (auth + `regexec` :186-193),
  `update_subscribers` (type-intersect -> endpoint resolve -> match -> SET/UNSET_BIT(nr) + `ipc_notify` :198-224).

Random-10 anchor self-check (G1, all verified above by direct read):
T-1, T-2, T-4, L-2, L-3, publish LABEL-gate, subscribe anchoring, check writeback, delete cascade,
getsysinfo exact-size — each traced to file:line in §§1.1-1.3.

---

## 2. Knowledge Pool (deduplicated, stage-wide)

Source types: **S** = stock (from existing docs), **N** = new (from C / non-C artifacts / OS theory,
absent from existing docs). Alignment key for multi-AI merge: name + anchor.
Types: C=concept, M=mechanism, D=data structure, P=interface/protocol, K=constraint/invariant,
A=arch-evolution, T=tooling/engineering, E=test-nature.

### 2.1 Stock knowledge (from existing docs, deduplicated; repeated occurrences merged, master point marked *)

| ID | Name | Type | Existing locations (master *) | Anchor | Reader benefit (answers) |
|----|------|------|-------------------------------|--------|--------------------------|
| K-001 | DS role: pub/sub registry, label->endpoint | C | 00 §1.1*, 12 §1.1, 99 §3 | `main.c:1-9` header comment | What DS is for in one paragraph |
| K-002 | Boot two-level semantics (registration order vs execution order) | C | 00 §1.2*, 06 §1.3, draft/README | `kernel/table.c:52`; `kernel/main.c:196-200,265-267` | Why "first service" still runs after VM/RS |
| K-003 | Single-threaded event loop, `who_e`/`callnr` globals | M | 01 §1.3*, 07/09 (implicit) | `main.c:15-16,42-88` | Why no locks; where request state lives |
| K-004 | SEF triple registration (fresh / stateful-restart / LU hook) | M | 01 §1.3-§2*, 06 §1.5 | `main.c:93-104`; `sef.h:44-53,85` | What survives a restart and why |
| K-005 | Receive/dispatch/reply skeleton; notify menolak; default EINVAL; EDONTREPLY skip | M | 01 §1.3* | `main.c:42-88`; `com.h:90-93`; `sys/sys/errno.h` (EDONTREPLY) | Where every request enters and leaves |
| K-006 | Seven call numbers on `DS_RQ_BASE=0x800`, incl. dead `DS_SNAPSHOT` (+5, no arm) | P | 01 §1.3, 02 §1.3*, 99 §1.1 | `com.h:498-507` | The complete verb vocabulary + the hole |
| K-007 | `mess_ds_req` 7 fields (key_grant/key_len/flags/val_in/val_len/owner/padding) | D | 02 §1.3* | `ipc.h:107-115` | How to fill a request |
| K-008 | `mess_ds_reply` (val_out/val_len) + `union ds_val` (grant/u32/ep; label rides u32) | D | 02 §1.3*, 03 §1.3 | `ipc.h:94-105` | How to read a reply; why no label arm |
| K-009 | DSF flag universe + masks (IN_USE, 3 PRIV gates, 4 TYPE arms, OVERWRITE, INITIAL, MASK_TYPE 0xFF0, MASK_INTERNAL 0xFFF, dead PRIV_SNAPSHOT alias) | K | 02 §1.4*, 03/04/05 (use) | `ds.h:11-32` | The bit vocabulary every gate speaks |
| K-010 | Key-grant direction rule (CHECK/RETRIEVE_LABEL = 80B writable; else strlen+1 readable) | P | 02 §1.3*, 12 §1.3 | `libsys/ds.c:7-33` | Which direction memory flows per call |
| K-011 | `do_check` reply reuses request fields (flags/owner writeback) | P | 02 §1.3*, 10 (use), 12 §1.5 | `store.c:570-572`; `libsys/ds.c:209-219` | Where to read check results (not m_ds_reply) |
| K-012 | `struct data_store` 4 columns (flags/key[80]/owner[80]/union u) | D | 03 §1.3* | `store.h:16-29` | What one row holds |
| K-013 | `union dsi_u` sharing (u32 vs mem{data,length,reallen}); reallen reuse rule | D | 03 §1.3* | `store.h:21-28`; `store.c:344-348` | When publish reallocates and when it does not |
| K-014 | `struct subscription` 4 columns (flags/owner/regex/old_subs bitmap, index==entry slot) | D | 03 §1.4* | `store.h:31-36` | How "who waits for what" is stored |
| K-015 | Fixed capacities NR_DS_KEYS=128 / NR_DS_SUBS=256 (=2x/4x NR_SYS_PROCS=64) | K | 03 §1.5* | `store.h:12-13`; `sys_config.h:9` (`_NR_SYS_PROCS 64`) | Why the tables never grow |
| K-016 | Empty = `!(flags & IN_USE)`; no free list, no counter | K | 03 §1.5*, 04 §1.3 | `store.c:15-17` | The single emptiness test |
| K-017 | 192-byte cross-server layout ABI (4+80+80+24, 8-aligned) consumed raw by IS | K | 03 §1.6*, 11 §1.4 | `store.c:661-672`; `servers/is/dmp_ds.c:15-45` | Why field order is protocol |
| K-018 | First-fit ascending allocation = stable mirror order | M | 04 §1.4* | `store.c:11-38` | Why slot order must not change |
| K-019 | Three lookups: by name+type / by label-number / by sub-owner (flag-first, then content) | M | 04 §1.3* | `store.c:57-105` | How each verb finds its row |
| K-020 | `free_sub_slot`: assert-held -> regfree -> zero -> clear flag | M | 04 §1.3* | `store.c:43-52` | Why double-free crashes by design |
| K-021 | Endpoint->name 3-way (self "ds" / label reverse / NULL); name->endpoint forward + panic | M | 05 §1.3* | `store.c:110-138` | How kernel numbers become table names |
| K-022 | Selective protection: gate-unset->allow; gate-set->owner-compare; NULL always loses | M | 05 §1.4* | `store.c:143-153` | DS gates are opt-in, not default-deny |
| K-023 | `map_service` (slot/key/u32/owner="rs"/LABEL flags + notify ring) | M | 06 §1.4* | `store.c:229-249` | Where genesis labels come from |
| K-024 | `sef_cb_init_fresh`: clear -> grant-copy rproctab -> loop map_service (panics) | M | 06 §1.4* | `store.c:254-282` | The 3-step first boot |
| K-025 | STATEFUL restart keeps tables (no re-fresh) | M | 06 §1.5*, 01 §1.3 | `main.c:97` + `SEF_CB_INIT_RESTART_STATEFUL` | Why a registry survives restarts |
| K-026 | Publish 7-gate sequence + order-is-semantics | M | 07 §1.3* | `store.c:287-326` | Exact rejection order and codes |
| K-027 | Publish two landings (create / overwrite+EEXIST+PRIV_OVERWRITE) + label-by-number second chance | M | 07 §1.4* | `store.c:310-326` | How re-publish and label rename-races resolve |
| K-028 | Publish 4-type writes incl. heap grow/copy-fail-rollback/NUL-pin + INTERNAL-only persisted flags | M | 07 §1.4* | `store.c:329-377` | What hits the heap vs the row |
| K-029 | Retrieve two paths (by-name gated / by-endpoint ungated) + MIN truncation + val_len | M | 08 §1.3-§1.5* | `store.c:383-451` | Read mirror of publish; asymmetric gates |
| K-030 | Delete owner-only (name compare, not check_auth) + per-type teardown | M | 09 §1.3* | `store.c:593-609,633-648` | Why delete has no "publicly deletable" |
| K-031 | Label cascade: subs first, then owned entries (per-victim ring), then self; ring-before-clear | M | 09 §1.4* | `store.c:613-631,642-645` | Deleting a service unpublishes its world |
| K-032 | Subscribe 6 steps incl. `^...$` anchoring, REG_EXTENDED compile, type-default-all, DSF_INITIAL sweep+notify, EAGAIN-on-full | M | 10 §1.4* | `store.c:456-531` | How interest is registered |
| K-033 | Match = auth-gate AND regexec; anchored full-match semantics (EreMatcher, ARCH A-2) | M | 10 §1.5* | `store.c:186-193,487-498`; `os/servers/ds/src/pattern.rs` | What counts as "this entry interests me" |
| K-034 | Fan-out = type-intersect -> endpoint resolve -> match -> SET/UNSET_BIT(nr) + ipc_notify, for BOTH set and clear | M | 10 (journey §1.3)*, 06/07/09 (call) | `store.c:198-224` | Push+pull split: ring shouts, readers pull |
| K-035 | Check consume: first-set-bit / ENOENT / key copy / type+owner writeback / UNSET after success | M | 10 (check half)* | `store.c:536-578` | How one pending update is taken exactly once |
| K-036 | Getsysinfo 2 gates (what + exact size) + whole-table sys_datacopy | M | 11 §1.3* | `store.c:653-678` | The only whole-table exit |
| K-037 | Client 16 APIs in 3 shapes (inline / grant-block / subscribe-check) via `do_invoke_ds` | P | 12 §1.3* | `libsys/ds.c:7-219` | The other end of the wire |
| K-038 | String NUL discipline both directions (pin-before-lend on publish, pin-after-copy on retrieve, +1 in lengths) | P | 12 §1.4* | `libsys/ds.c:79-90,150-157` | Why string lengths always carry the terminator |
| K-039 | Errno closed set (EPERM/EINVAL/ESRCH/ENOENT/EEXIST/EAGAIN/ENOMEM/EDONTREPLY) | K | 99 §2* (+ARCH A-9) | `store.c` passim; `ds.h`; `errno.h` | No invented codes |
| K-040 | Cross-service users: RS label publish, VFS drv.* subscribe+ds_event, PM readclock lookup, INPUT/filter/i2c/lwIP subscriptions, IS dump, DS_DRIVER_UP convention | C | 99 §3*, 10 (VFS pattern), 11 (IS) | `rs/manager.c:513,800`; `vfs/main.c:441`; `vfs/misc.c:960-985`; `pm/misc.c:212`; `input/input.c:665`; `filter/main.c:385`; `i2c.c:452`; `ndev.c:164`; `ds.h:32` | Who depends on DS and how |

### 2.2 New knowledge (absent from existing docs; each gets an evidence anchor, never appears from thin air)

| ID | Name | Type | Anchor | Why it belongs (B-phase home) |
|----|------|------|--------|-------------------------------|
| K-041 (N) | DS Makefile: `PROG=ds`, `SRCS=main.c store.c`, `-lsys`, and the magic-instrumentation workaround `CPPFLAGS.store.c += -Dregcomp=_regcomp -Dregfree=_regfree` | T | `minix3/minix/servers/ds/Makefile` (full 15-line read) | Build seam: explains why `regcomp` in DS is really `_regcomp`; 01 gains a build-seam subsection |
| K-042 (N) | `EDONTREPLY` lives in the NetBSD-derived `sys/sys/errno.h`, not in `minix/com.h`/`const.h` (grep over `minix/include/` empty; hit only in `sys/sys/errno.h`) | P | `minix3/sys/sys/errno.h`; use site `main.c:82` | 01 reply rule + 99 errno table get the correct home for the constant |
| K-043 (N) | Real-client subscription patterns ALL contain metacharacters (VFS `drv\.[bc]..\..*`, INPUT `drv\.inp\..*`, filter `drv\.blk\..*`, i2c dynamic regex, lwIP `drv\.net\..*`) — literal-only matching never occurs in-tree | E | `vfs/main.c:441`; `input/input.c:665`; `filter/main.c:385`; `i2cdriver.c:110`; `i2c.c:452`; `ndev.c:164` (all `grep`-verified this survey) | Acceptance evidence for the push-half doc (07-new): engine tests must cover these five patterns |
| K-044 (N) | `tests/ds/` behavioral contracts: u32 publish/retrieve/EEXIST-overwrite/delete-ESRCH roundtrip + 2s sleep for subscriber catch-up; str truncation vs `strncmp`; mem partial-read `get_len=8`; label self EPERM pair; subs.c notify->check->retrieve loop with ENOENT-as-deleted | E | `minix3/minix/tests/ds/dstest.c` (178, fully read), `subs.c` (91, fully read) | Per-handler acceptance criteria (no new doc; plan §5.4 already rules this; confirmed correct) |

### 2.3 Statistics

- Total: 44 items (40 stock + 4 new). By type: C 4, M 25, D 4, P 7, K 8 (counting K-009/K-015..K-017/K-039 as K), A 0 (ARCH items live in plan.md/todo.md and are referenced, not re-owned), T 1, E 2.
  (IDs carry their type in the table; exact histogram is informational, not a gate.)
- By existing doc: 00:2, 01:3, 02:6, 03:6, 04:3, 05:2, 06:3, 07:3, 08:1, 09:2, 10:4, 11:1, 12:2, 99:2.
- Densest carrier: 10-subscribe-check (owns K-032..K-035 = match+fan-out+consume+journey) — the structural
  reason for the single MOVE in §4 (see §4.1, finding F-1).

---

## 3. Coverage Audit

### 3.1 Theme universe (four sources)

1. **C symbols**: 25 functions + 2 tables (`ds_store`, `ds_subs`) + 2 globals (`who_e`, `callnr`) +
   flag universe (`ds.h`) + message structs (`ipc.h`) + call numbers (`com.h`) + `SI_DATA_STORE` +
   error paths (8 errnos + 2 panics: `ds_getprocep` :137, grant-copy paths :272, :276-277) +
   arch-dependent part: NONE (DS has no `#ifdef ARCH` branches — verified by full read of both .c files).
2. **OS-generic concepts**: registry/name-service, pub/sub (push+pull split), selective protection,
   fixed-table snapshot ABI, graceful vs crashing lookup failure, at-most-once bitmap consumption.
3. **Non-C artifacts**: see §3.5 checklist (10 items, all answered).
4. **Boundary contracts**: master-plan stage table (DS = registry for RS-loaded services);
   `edge_todo.md` E-DSWIRE (transport/client wiring — Rust-side, not doc scope), E-MINTYPES-SYS DS items
   (`SI_DATA_STORE`/`NOTIFY_MESSAGE`常量收敛 — reflects in 02/11/99 constant homes), E5(f) DS litmus
   (maps to K-044 acceptance, no new doc).

### 3.2 Coverage-gap table

| # | Theme in universe, in no doc | Verdict |
|---|------------------------------|---------|
| GAP-1 | Makefile magic-workaround (`-Dregcomp=_regcomp`): no doc explains it | NEW knowledge K-041 -> 01 build-seam subsection (contract §5/01). Only true gap found. |
| GAP-2 | `EDONTREPLY` constant home: docs use it, none says where it is defined | NEW knowledge K-042 -> one line in 01 + 99 table footnote. |
| — | Everything else (all 25 functions, both tables, flags, messages, errnos, panics, cascade, INITIAL sweep, exact-size gate, NUL discipline, dead APIs) | Covered (see §2.1 anchors). No new chapter needed. |
| — | `do_snapshot` (proto.h:16, no definition), `DS_SNAPSHOT`, `ds_*_map` family, `DSF_PRIV_SNAPSHOT` alias | Explicitly excluded everywhere (A-7); exclusion is the correct coverage state, kept in 02 + 12 + 99. |

New-gap entries K-041/K-042 enter contracts with evidence anchors (Step-4新增方向看来源 — satisfied).

### 3.3 Duplicate-theme table (same theme expanded in >1 doc; one master kept, rest become citations)

| # | Theme | Occurrences | Master kept | Others become |
|---|-------|-------------|-------------|---------------|
| DUP-1 | `update_subscribers`+`check_sub_match` mechanics | Owned by 10; invoked/described in 06, 07, 09 | New 07 (moved 10) — the ONLY full telling | 06/08-new/10-new keep one-line effect summaries + pointer (no mechanism repeat) |
| DUP-2 | Boot two-level semantics | 00 §1.2, 06 §1.3, draft/README | 00 (one-paragraph) + 06 (full) | draft stays archived; 00 keeps map, 06 keeps mechanism — already clean, no change |
| DUP-3 | `DS_PROC_NR` / flag values / errno values quoted in passing | 00/01/06/99, 02/03/04/05 | 99 (tables) + 02 (flags) | Passing mentions stay as anchored literals — not duplication, no change |
| DUP-4 | VFS `drv.*` pattern as engine evidence | 10 §1.5 + todo.md | New 07 §acceptance (K-043 five-pattern table) | Single table; 12 cites it for client pattern advice — no change beyond move |

### 3.4 Out-of-scope theme table (taught outside its declaring doc's boundary; correct home given)

| # | Theme | Currently in | Correct home | Disposition |
|---|-------|--------------|--------------|-------------|
| OUT-1 | `DS_SNAPSHOT`/`do_snapshot`/map-family/dead alias | 02 §1.4 + 12 + 99 (exclusion notes) | 02 (exclusion registry) | Keep: exclusion notes are correctly placed; 12/99 keep one-line pointers |
| OUT-2 | `_taskcall`/`cpf_grant_direct` implementation | 12 (correctly deferred to minix-sys/A-8) | 14-stage-runtime / minix-sys docs | Keep deferral; 12 keeps packing rules only |
| OUT-3 | IS pagination display logic (`prev_i` rotation, 22-line pages) | 11 (correctly excluded) | 08-stage-is | Keep one-line requirement-side note in 11 |
| OUT-4 | Live-update magic traversal internals (`magic_ds.c` per-union callbacks) | 01/06 (correctly: hook existence + explicit-transfer ARCH A-6) | No doc (unported mechanism) | Keep; Rust explicit export/import covered via 06 contract |

### 3.5 Non-C topics — answered one by one (no blank, no "TBD")

| Fixed item | Finding | Told where / why not in this stage |
|------------|---------|-------------------------------------|
| Link & load | No linker script for DS (plain user service; linked by `minix.service.mk` via Makefile) | 01 build-seam subsection (K-041); one paragraph, no chapter |
| Image & memory layout | Boot-image slot (`table.c:44-64`) | 06 §1.3 (two-level semantics); no chapter |
| ASM entry & trap entry | None — entry is `main()` via SEF; receives via `sef_receive` | 01 §1.3; no chapter |
| Boot assembly | `table.c` registration + RS pull (`rproctab_gid`) + kernel inhibit | 06 (full); no chapter |
| Build & toolchain | `servers/ds/Makefile` (PROG/SRCS/LIBS + regcomp alias workaround) | 01 build-seam (new, K-041); no chapter |
| Cross-module iface & wire format | `com.h`/`ipc.h`/`ds.h`/`sysinfo.h` + minix-types mirrors | 02 (full); no chapter |
| Error paths | 8 errnos + 2 panic sites + grant-copy failure printf paths | 99 §2 (table) + per-handler §C-analysis; no chapter |
| Shutdown & exit | DS never exits (`while(TRUE)`, `return OK` unreachable :87); EDONTREPLY suspend convention; STATEFUL restart | 01 (loop) + 06 §1.5; no chapter |
| Concurrency & sync | Single-threaded; no locks; bitmap consumed by one reader loop | 01 §1.3 + new-07 check half; no chapter |
| Test infrastructure | `tests/ds/` dstest.c+subs.c+run+system.conf (347 lines) | Behavioral contracts folded into per-doc acceptance (K-044); standalone test-infra doc explicitly NOT created |

---

## 4. New Catalog

### 4.1 Design decisions (my evidence and judgment — NOT copied from plan.md/todo.md)

- **F-1 (the one structural defect; verified by grep, not by opinion).** The notify fan-out
  (`update_subscribers` + `check_sub_match`, `store.c:198-224`) is fully told in doc 10 but fires from
  three earlier-taught call sites: `map_service` (06, `:246`), `do_publish` (07, `:375`),
  `do_delete` (09, `:642` + cascade `:628`). Both 07 (`07-ds-publish.md:18`: "本篇只讲调了它，不讲它干了什么")
  and 09 (`09-ds-delete.md:17,64`) contain explicit **forward reading-dependencies** on 10.
  Hard standard §5.1(1) (no forward references) is therefore violated twice, by the docs' own admission.
  Alternatives weighed: (a) inline 3-line effect summaries in 07/09 and keep numbers — leaves
  "first occurrence incomplete" (§5.1(3)) since publish is where the ring first fires at runtime;
  (b) split 10 into registration vs delivery halves — severs `DSF_INITIAL` from `do_subscribe`
  (its own function, `:511-528`) and strands `check` between two homes; (c) MOVE the intact 10
  ahead of publish. **(c) wins**: one file rename, zero content severing, journey diagram intact,
  and the causal chain becomes register -> publish -> consume.
- **F-2 (sizes need no surgery).** All 14 docs are 66-170 lines; the densest (10, 170 lines) stays one
  semantic unit ("the push half": interest + wake + consume — one journey, §5.1(4) satisfied).
  Doc count is not a constraint (may grow or shrink); nothing here justifies a split or merge.
  The soft 3000-line single-concept allowance is nowhere near invoked.
- **F-3 (old docs contain real defects, so rebuild framing applies — but the defects are localized).**
  Verified against C/Rust ground truth (not against prior reviews): (i) the F-1 ordering defect;
  (ii) GAP-1/GAP-2 omissions (Makefile alias, EDONTREPLY home); (iii) an internal tension between
  03 §1.6 (192-byte raw-copy ABI, test-locked) and 11 §1.2/§1.4 (168-byte scalar snapshot rows
  `DsEntrySnap`, `image_bytes() == 168*128`) — C ground truth (`store.c:671`, `dmp_ds.c:30-41`)
  copies/interprets the raw 192-byte struct, so any 168-byte rendering needs an explicit
  compatibility argument at exactly one place. This is recorded as open question Q-1 (§9),
  NOT unilaterally decided. Everything else in the 14 docs checked out against the C source
  (spot-verified gate sequences, flag values, grant rules, cascade order, writeback fields).
  Conclusion: rebuild = **1 move + retargeted intros/outros + 1 new subsection + 1 adjudication**,
  not a from-scratch rewrite. Per the R-prompt methodology this is still "rebuild" (new catalog
  as the coordinate system; each doc re-sourced from pool + C + artifacts), but B-phase work
  reuses surviving prose wherever the contract says "keep".

### 4.2 New chapter table (number, title, one-line positioning, group)

| New # | Title | Positioning | Group |
|-------|-------|-------------|-------|
| 00 | ds-overview | Stage map, boot two-level semantics, reading paths | 0 Overview |
| 01 | ds-init-main | Entry, SEF triple registration, receive/dispatch/reply skeleton (+ build seam) | 1 Startup & protocol |
| 02 | ds-message-contract | Wire structs, flag universe, grant rules, dead-API registry | 1 Startup & protocol |
| 03 | ds-data-structures | Two tables, 192-byte layout ABI | 2 Data plane: state |
| 04 | ds-slot-management | Six slot primitives, first-fit order contract | 2 Data plane: state |
| 05 | ds-identity-auth | Endpoint<->name translation, selective protection | 2 Data plane: state |
| 06 | ds-boot-mapping | First-boot clear-copy-register, STATEFUL restart | 3 Birth |
| 07 | ds-subscribe-check-notify (MOVED from 10) | The push half: register interest, fan-out, consume exactly once | 4 Verbs: push first |
| 08 | ds-publish (from 07) | Gated write path, 4-type stores, fires the ring | 4 Verbs |
| 09 | ds-retrieve (from 08) | Gated read mirror, truncation, label reverse lookup | 4 Verbs |
| 10 | ds-delete (from 09) | Owner-only teardown, label cascade, ring-before-clear | 4 Verbs |
| 11 | ds-getsysinfo | Whole-table snapshot exit, IS consumer contract | 4 Verbs |
| 12 | ds-client-library | 16 client APIs, packing rules, NUL discipline | 5 Wire other end |
| 99 | ds-global-concepts | Constants, errno set, cross-service users | 99 Close |

Filenames: keep the `NN-ds-<slug>.md` scheme; 07-new file takes the name
`07-ds-subscribe-check.md` (content of old 10), 08/09/10 take old 07/08/09 contents under new numbers.
(Exact rename mechanics are B-phase work; §8 lists every affected anchor.)

### 4.3 Reading paths

- **Main line** (startup + one entry's journey): 00 -> 01 -> 02 -> 03 -> 04 -> 05 -> 06 ->
  07 (register) -> 08 (publish fires ring) -> 07 §journey (wake) -> 07 §check (consume) ->
  09 (pull data) -> 10 (teardown) -> 11 (snapshot) -> 12 (client view) -> 99.
- **Branch line** (skippable): 11 §IS-pagination (IS-side display), 12 §dead-API pointers,
  99 §3 rows for services the reader does not run, K-041 Makefile alias box in 01.
- **No parallel-body problem**: DS verbs are one switch, not a 40-driver collection; §5.2 grouping
  rules do not trigger. Representative-member treatment is unnecessary (each verb is fully told).

### 4.4 First-appearance map (§5.1(3) compliance)

- `update_subscribers`/`check_sub_match`/bitmap/`ipc_notify`: first fully told in 07-new (its ONLY home).
- Publish/delete/boot-mapping mention ring EFFECTS in one line each and cite 07-new (no re-telling).
- `mess_ds_req`/`ds_val`/DSF universe: first in 02; later docs cite field names only.
- `data_store`/`subscription`/capacities: first in 03; later docs cite.
- Translation/auth: first in 05; handlers cite gate names only.
- `rprocpub`/grant-copy/`map_service`: first in 06; 12 cites the label rule only.
- Check-writeback quirk: first in 02 (protocol fact), used in 07-new §check + 12 (no re-explaining).

### 4.5 Order-delta table (runtime truth vs teaching order; every deviation justified + compensated)

| # | Runtime fact (anchor) | Teaching choice | Reason | Compensation (back-pointer) |
|---|-----------------------|-----------------|--------|------------------------------|
| OD-1 | Switch order is PUBLISH, RETRIEVE, SUBSCRIBE, CHECK, DELETE (`main.c:53-71`) | Teach SUBSCRIBE/CHECK (07-new) BEFORE PUBLISH (08-new) | Causal order: interest must be registered before a publish can wake anyone; eliminates the two admitted forward refs (F-1) | 07-new §1 states its runtime arms (+2/+3) explicitly; 01 dispatch table keeps RUNTIME order with new numbers |
| OD-2 | `map_service`'s ring call (:246) runs before ANY subscription can exist | Teach ring (07-new) AFTER boot mapping (06) | Boot reader meets the call first; effect ("normally empty at boot") told inline in 06 in one line | 06 cites 07-new for mechanism; no reader needs the mechanism to understand boot |
| OD-3 | `do_check` writeback fields live in the request struct | Teach struct quirk in 02 (protocol), mechanics in 07-new | Protocol facts belong with the wire doc (§5.1(4) single semantics) | 07-new §check + 12 §check cite 02, one line each |
| OD-4 | `DS_GETSYSINFO` is switch-last (+7) but taught (11) before client lib (12) | Keep 11 before 12 | Snapshot is server behavior; client lib is the other end (12 closes the loop) | 12 cites 11 for the `getsysinfo()` generic wrapper path |

---

## 5. Per-Document Contracts (mandatory; one missing = no delivery)

Legend: Covers list K-IDs from §2. Knowledge-list rows: `ID | name | type | anchor | why-here | source`.

### 00-ds-overview

- Positioning: the stage map. Answers "what is DS, where in boot, what order to read".
- Covers: K-001, K-002, K-006 (counts), K-017 (one line), K-040 (table pointers).
- Not covers: any mechanism (→ 01-12, 99); boot execution detail (→ 06); flag table (→ 02/99).
- Prereqs: none (plus: concepts already taught in 06-stage-sched need no repeat — start point confirmed
  by previous-stage file list: scheduler init/main/loop/smp/pick-cpu; DS assumes a scheduled event loop).
- Post-refs: all 01-12, 99 cite 00 as the map (00 cites none back except kernel/RS boot anchors).
- Ground truth: `servers/ds/` (811 lines: main.c 132 + store.c 679); `kernel/table.c:52`;
  `kernel/main.c:196-200,265-267`; `os/servers/ds/` (22 files, 112 tests).
- Knowledge list:
  | K-001 | pub/sub registry role | C | `main.c:1-9` | stage thesis | S (00 §1.1) |
  | K-002 | boot two-level semantics | C | `table.c:52`; `main.c:265-267` | start-point justification | S (00 §1.2) |
- Acceptance: (1) nav table lists new numbers 01-12,99 with one-line roles and matches §4.2 exactly;
  (2) reading paths (§4.3) reproduced verbatim; (3) boot diagram shows T-1..T-8 positions per handler doc;
  (4) test-count claim re-verified (`cargo test -p minix-ds` in `os/`, currently 112).

### 01-ds-init-main

- Positioning: how one event loop carries the whole registry. Answers "receive, dispatch, reply".
- Covers: K-003, K-004, K-005, K-025 (registration half), K-041 (N, NEW build-seam subsection), K-042 (N, EDONTREPLY home line).
- Not covers: SEF callback BODIES (→ 06); handler implementations (→ 07-new..11); message fields (→ 02); table shape (→ 03/04).
- Prereqs: 00.
- Post-refs: 06 (SEF bodies), 07-new..11 (arms), 12 (client entry).
- Ground truth: `main.c` full (132); `sef.h:44-53,85`; `com.h:90-93`; `sys/sys/errno.h` (EDONTREPLY);
  `servers/ds/Makefile` (K-041); Rust `os/servers/ds/src/main.rs`, `server.rs` (run/run_once), `sef.rs`, `dispatch.rs`.
- Knowledge list:
  | K-003 | single-threaded loop + globals | M | `main.c:15-16,42-88` | loop skeleton | S (01 §1.3) |
  | K-004 | SEF triple registration | M | `main.c:93-104` | birth contract | S (01 §1.3) |
  | K-005 | receive/dispatch/reply + notify-refuse + EDONTREPLY skip | M | `main.c:42-88`; `com.h:90-93` | loop semantics | S (01 §1.3) |
  | K-041 | Makefile + regcomp alias workaround | T | `servers/ds/Makefile` | build seam (NEW subsection) | N (C artifact) |
  | K-042 | EDONTREPLY defined in sys/sys/errno.h | P | `sys/sys/errno.h`; `main.c:82` | constant home (one line) | N (C artifact) |
- Acceptance: (1) dispatch table in RUNTIME order with NEW numbers + dead `DS_SNAPSHOT` row (no arm);
  (2) build-seam box answers "why does DS see `_regcomp`" (magic-instrumentation link pass);
  (3) key-question list: where does request state live / what happens to a notify / what skips a reply;
  (4) relationship diagram of get_work -> switch -> reply.

### 02-ds-message-contract

- Positioning: every envelope field's meaning. Answers "how to fill / read a DS message".
- Covers: K-006, K-007, K-008, K-009, K-010, K-011, GAP-2 exclusion registry (dead APIs).
- Not covers: table semantics (→ 03-05); handler flows (→ 07-new..11); client wrappers (→ 12).
- Prereqs: 01 (dispatch names exist).
- Post-refs: 03 (flag use), 07-new..11 (fields), 12 (grant rules), 99 (constant tables).
- Ground truth: `com.h:498-507`; `ipc.h:94-115`; `ds.h:11-35`; `sysinfo.h:13`; `libsys/ds.c:7-33`;
  Rust `minix-types` MessDsReq/MessDsReply + DsFlags + `NOTIFY_MESSAGE` (E-MINTYPES-SYS homes).
- Knowledge list:
  | K-006 | 7 call numbers + dead +5 | P | `com.h:498-507` | verb vocabulary | S (02) |
  | K-007 | mess_ds_req fields | D | `ipc.h:107-115` | request filling | S (02 §1.3) |
  | K-008 | mess_ds_reply + ds_val (label rides u32) | D | `ipc.h:94-105` | reply reading | S (02 §1.3) |
  | K-009 | DSF universe + masks + dead alias | K | `ds.h:11-32` | bit vocabulary | S (02 §1.4) |
  | K-010 | grant direction rule | P | `libsys/ds.c:7-33` | memory flow | S (02 §1.3) |
  | K-011 | check writeback quirk | P | `store.c:570-572` | misplaced-reply fact | S (02 §1.3) |
- Acceptance: (1) field table complete per struct (7 req fields incl. padding-is-meaningless);
  (2) flag table has all 12 rows incl. `0x80` hole note + dead-alias row; (3) grant-direction two-rule box;
  (4) dead-API registry (DS_SNAPSHOT/map-family/PRIV_SNAPSHOT) with "no definition" evidence each.

### 03-ds-data-structures

- Positioning: what the two tables look like. Answers "row shapes, capacities, why layout is protocol".
- Covers: K-012, K-013, K-014, K-015, K-016, K-017.
- Not covers: slot search/alloc (→ 04); translation/auth (→ 05); heap policy (→ 08-new/10-new + A-3).
- Prereqs: 02 (flags/type arms).
- Post-refs: 04 (shapes used), 05 (owner column), 11 (layout consumed), 07-new (bitmap index).
- Ground truth: `store.h` full (38); `store.c:15-17,240-243`; `is/dmp_ds.c:15-45`; `sys_config.h:9`;
  Rust `store.rs`/`subscription.rs` (+ 192B layout asserts) and Q-1 adjudication pointer (§9).
- Knowledge list:
  | K-012 | data_store 4 columns | D | `store.h:16-29` | row shape | S (03 §1.3) |
  | K-013 | dsi_u sharing + reallen rule | D | `store.h:21-28`; `store.c:344-348` | heap-cell shape | S (03 §1.3) |
  | K-014 | subscription 4 columns, index==slot | D | `store.h:31-36` | waiter shape | S (03 §1.4) |
  | K-015 | 128/256 from NR_SYS_PROCS=64 | K | `store.h:12-13`; `sys_config.h:9` | fixed-size rationale | S (03 §1.5) |
  | K-016 | empty == no IN_USE | K | `store.c:15-17` | emptiness test | S (03 §1.5) |
  | K-017 | 192B ABI consumed raw by IS | K | `store.c:661-672`; `dmp_ds.c:30-41` | layout-is-protocol | S (03 §1.6) |
- Acceptance: (1) byte math 4+80+80+24=188→192 shown; (2) IS raw-interpret walkthrough;
  (3) Q-1 compatibility note present with pointer to 11's verdict (exactly one place argues it);
  (4) modern-system contrast kept (sysfs/seL4/slab) only if it stays under 10 lines.

### 04-ds-slot-management

- Positioning: six primitives over two tables. Answers "find empty, find row, free waiter".
- Covers: K-016 (use), K-018, K-019, K-020.
- Not covers: row shapes (→ 03); name/auth sources (→ 05); handler call sites (→ 07-new..10-new).
- Prereqs: 03.
- Post-refs: 07-new (lookups), 08-new/10-new (alloc), 11 (order consumed).
- Ground truth: `store.c:11-107`; Rust `slots.rs` (+ EntrySlot/SubSlot newtypes, take-modify-put).
- Knowledge list:
  | K-018 | first-fit ascending = mirror order | M | `store.c:11-38` | order contract | S (04 §1.4) |
  | K-019 | three lookups, flag-first | M | `store.c:57-105` | row finding | S (04 §1.3) |
  | K-020 | free_sub_slot assert+regfree+zero | M | `store.c:43-52` | waiter teardown | S (04 §1.3) |
- Acceptance: (1) six-row primitive table with full/empty outcomes incl. assert-crash row;
  (2) "order is contract" argument with IS-mirror consequence; (3) pointer-vs-slot box (borrow/IPC/testability);
  (4) test table re-registered against `slots.rs` (Gate E carry-over).

### 05-ds-identity-auth

- Positioning: numbers become names, then gates decide. Answers "who may touch whose row".
- Covers: K-021, K-022.
- Not covers: table/slot shapes (→ 03/04); boot owner source (→ 06); handler call sites (→ 07-new..10-new).
- Prereqs: 03, 04 (lookups used by translators).
- Post-refs: 07-new (match gate), 08-new (RETRIEVE gate), 08-new/10-new (publish/delete paths).
- Ground truth: `store.c:110-156`; Rust `identity.rs` (panic→Option handoff) + `auth.rs`.
- Knowledge list:
  | K-021 | 3-way forward + forward-with-panic | M | `store.c:110-138` | translation | S (05 §1.3) |
  | K-022 | selective protection, 3 independent gates | M | `store.c:143-153` | authorization | S (05 §1.4) |
- Acceptance: (1) panic-site box (why C crashes, who owns the decision in Rust);
  (2) gate-truth table (unset→allow / set+match→allow / set+mismatch→deny / NULL→deny);
  (3) translate-vs-judge split box (table-touching vs pure).

### 06-ds-boot-mapping

- Positioning: where the first rows come from. Answers "clear, copy, register one by one".
- Covers: K-002 (mechanism half), K-023, K-024, K-025, K-034-effect (one line + pointer to 07-new).
- Not covers: SEF registration mechanics (→ 01); label retrieval (→ 09-new); client label publish (→ 12).
- Prereqs: 01 (callbacks registered), 03/05 (row shape, owner names).
- Post-refs: 07-new (ring mechanism), 09-new (label reads), 12 (RS publish path).
- Ground truth: `store.c:229-282`; `table.c:44-64`; `kernel/main.c:196-200,265-267`;
  `rs.h:165-183`; `sef.h:44-53,85`; Rust `boot.rs` (+ D4/D5 hook honesty).
- Knowledge list:
  | K-023 | map_service genesis row | M | `store.c:229-249` | first data | S (06 §1.4) |
  | K-024 | fresh 3-step with panics | M | `store.c:254-282` | boot anchor | S (06 §1.4) |
  | K-025 | STATEFUL keeps tables | M | `main.c:97` | restart semantics | S (06 §1.5) |
- Acceptance: (1) two-level order diagram (table slot vs execution); (2) clear-copy-register walkthrough
  with both panic sites named; (3) ring-effect one-liner cites 07-new (NO mechanism repeat — DUP-1 rule);
  (4) owner="rs" boxed as the genesis instance of the label-only-RS rule.

### 07-ds-subscribe-check-notify (MOVED content of old 10; the only renumbered body)

- Positioning: the push half. Answers "register interest, get woken, take each update exactly once".
- Covers: K-032, K-033, K-034 (ONLY full telling — DUP-1 master), K-035, K-043 (N, acceptance patterns), K-011 (use).
- Not covers: regex engine byte-behavior ledger (→ `pattern.rs` doc-block + ARCH A-2 record; consumption only here);
  client wrappers (→ 12); publish/delete bodies (→ 08-new/10-new).
- Prereqs: 02 (flags/messages), 03 (bitmap index), 04 (sub slots), 05 (auth gate). ALL earlier-numbered. No forward refs.
- Post-refs: 06 (ring call), 08-new + 10-new (ring callers), 09-new (pull leg), 12 (client shapes).
- Ground truth: `store.c:456-531` (subscribe), `store.c:536-578` (check), `store.c:186-224` (match+fan-out);
  five real-client patterns (`vfs/main.c:441`, `input/input.c:665`, `filter/main.c:385`,
  `i2c.c:452`, `ndev.c:164`); `tests/ds/subs.c` (notify->check->retrieve loop, ENOENT-as-deleted);
  Rust `subscribe.rs`, `check.rs`, `notify.rs`, `pattern.rs` (EreMatcher).
- Knowledge list:
  | K-032 | subscribe 6 steps + INITIAL sweep | M | `store.c:456-531` | registration | S (old-10 §1.4) |
  | K-033 | match = gate AND full-match | M | `store.c:186-193,487-498` | interest test | S (old-10 §1.5) |
  | K-034 | fan-out set+clear with notify | M | `store.c:198-224` | wake semantics | S (old-10 §1.3) |
  | K-035 | check consume-once order | M | `store.c:536-578` | taking updates | S (old-10 check half) |
  | K-043 | five real patterns all metachar | E | 5 anchors above | engine acceptance | N (client tree) |
- Acceptance: (1) journey diagram kept, re-anchored (publish=08-new, retrieve=09-new, delete=10-new);
  (2)_intro rewritten_: "why registration is taught before publish" (OD-1) + runtime arms (+2/+3) stated;
  (3) five-pattern table with match/non-match pairs (K-043) registered as engine tests;
  (4) type-mask gate (`& DSF_MASK_TYPE` :210) boxed — the exact line the Rust P1-2 bug once dropped;
  (5) check-order lock: UNSET only after successful copy (:575) + subs.c ENOENT reading.

### 08-ds-publish (content of old 07)

- Positioning: the gated write path. Answers "who may plant, plant-or-cover, what the heap does".
- Covers: K-026, K-027, K-028, K-034-effect (one line + pointer to 07-new).
- Not covers: retrieve (→ 09-new); delete (→ 10-new); ring internals (→ 07-new); heap allocator choice (A-3, recorded, not decided here).
- Prereqs: 02, 03, 04, 05, 07-new (ring). ALL earlier. Zero forward refs (F-1 fixed).
- Post-refs: 09-new (mirror), 10-new (overwrite twin), 07-new (ring fired).
- Ground truth: `store.c:287-378` + `get_key_name` `:158-181`; `tests/ds/dstest.c` u32/str/mem sections;
  Rust `publish.rs` (plan/commit split), `heap.rs` (A-3 pool).
- Knowledge list:
  | K-026 | 7-gate sequence | M | `store.c:287-326` | rejection order | S (old-07 §1.3) |
  | K-027 | create vs overwrite + label 2nd chance | M | `store.c:310-326` | landing modes | S (old-07 §1.4) |
  | K-028 | 4-type writes + heap + NUL + INTERNAL mask | M | `store.c:329-377` | write effects | S (old-07 §1.4) |
- Acceptance: (1) gate ladder with codes in C order; (2) ring-effect one-liner cites 07-new
  (old "那是10的事" rewritten as backward pointer); (3) dstest roundtrips (EEXIST, overwrite, truncation)
  mapped to test names; (4) judge-vs-commit split box kept.

### 09-ds-retrieve (content of old 08)

- Positioning: the gated read mirror. Answers "read by name, read by endpoint, how much, who may not".
- Covers: K-029, K-011 (use).
- Not covers: publish writes (→ 08-new); subscribe flow (→ 07-new); client wrappers (→ 12).
- Prereqs: 02, 03, 05, 08-new (mirror reference — backward).
- Post-refs: 07-new (pull leg of journey), 12 (raw wrappers).
- Ground truth: `store.c:383-451`; `tests/ds/dstest.c` retrieve halves; Rust `retrieve.rs`.
- Knowledge list:
  | K-029 | two paths + MIN + gates | M | `store.c:383-451` | read semantics | S (old-08) |
- Acceptance: (1) asymmetric-gate box (by-name EPERM-able, by-endpoint never EPERM — public registry rationale);
  (2) truncation rule with `val_len` distinguishability; (3) 4-refusal ladder incl. unknown-arm EINVAL.

### 10-ds-delete (content of old 09)

- Positioning: owner-only teardown. Answers "destroy, cascade a service, ring before clearing".
- Covers: K-030, K-031, K-034-effect (one line + pointer to 07-new).
- Not covers: ring internals (→ 07-new); heap allocator choice (A-3; handoff descriptors recorded); client wrappers (→ 12).
- Prereqs: 02, 03, 04, 05, 07-new (ring). ALL earlier. Zero forward refs (F-1 fixed).
- Post-refs: 07-new (ring fired), 09-new (teardown vs read twin).
- Ground truth: `store.c:583-648`; `tests/ds/dstest.c` delete assertions (incl. label self-EPERM pair);
  Rust `delete.rs` (victim-snapshot + wake callback, C-leak superset annotation).
- Knowledge list:
  | K-030 | owner-only + per-type teardown | M | `store.c:593-609,633-648` | destroy rule | S (old-09 §1.3) |
  | K-031 | label cascade order + ring-before-clear | M | `store.c:613-631,642-645` | service removal | S (old-09 §1.4) |
- Acceptance: (1) three-differences-vs-retrieve table kept; (2) cascade order box (subs→entries→self)
  with per-victim ring cites to 07-new; (3) heap-handoff (not leak) box + C-cascade-leak superset note;
  (4) dstest delete assertions mapped.

### 11-ds-getsysinfo

- Positioning: the whole-table exit. Answers "two gates, one copy, IS reads it raw".
- Covers: K-036, K-017 (use), Q-1 verdict home (see §9).
- Not covers: 192-byte rationale (→ 03); IS display (→ 08-stage-is); generic getsysinfo wrapper (one line + cite).
- Prereqs: 02 (message), 03 (layout), 04 (order).
- Post-refs: 03 (layout home), 12 (wrapper mention).
- Ground truth: `store.c:653-678`; `is/dmp_ds.c:8-45`; `libsys/getsysinfo.c:22`;
  Rust `getsysinfo.rs` + `server.rs` render path.
- Knowledge list:
  | K-036 | what+exact-size gates + datacopy | M | `store.c:653-678` | snapshot exit | S (11 §1.3) |
- Acceptance: (1) exact-size insurance argument (short=truncation, long=over-read);
  (2) three-party兑现 chain (03 layout + 04 order + this doc's gates) with the IS silent-misread cost;
  (3) Q-1 verdict recorded HERE (168-vs-192) with the single compatibility argument and the
  hand-written-constant test lock (`168*128` style, never `size_of` self-proof); (4) call chain one-liner.

### 12-ds-client-library

- Positioning: the other end of the wire. Answers "pack, lend, pin, read the misplaced reply".
- Covers: K-037, K-038, K-010 (use), K-011 (use).
- Not covers: server internals (→ 03-11); `_taskcall`/grant implementation (→ minix-sys/A-8); dead APIs (pointer to 02).
- Prereqs: 02 (messages+grants), 07-new..11 (server behaviors mirrored).
- Post-refs: none inside stage (terminal); out-of-stage to minix-sys transport when it lands (E-DSWIRE).
- Ground truth: `libsys/ds.c` full (219); `ds.h:40-68`; Rust `client.rs` (three packing contracts).
- Knowledge list:
  | K-037 | 16 APIs, 3 shapes | P | `libsys/ds.c:7-219` | client surface | S (12 §1.3) |
  | K-038 | two-direction NUL discipline | P | `libsys/ds.c:79-90,150-157` | string safety | S (12 §1.4) |
- Acceptance: (1) three-shape table with packing per shape (sizes, directions, val_len);
  (2) two-pin box with `+1`-in-lengths invariant + named tests; (3) check-misplaced-reply box (read request栏).

### 99-ds-global-concepts

- Positioning: the close. Answers "constants, codes, who uses DS".
- Covers: K-006 (table home), K-009 (pointer to 02), K-039, K-040, K-042 (footnote on EDONTREPLY home).
- Not covers: any mechanism (pointers only).
- Prereqs: all (terminal by construction; cites only, no new teaching).
- Post-refs: RS/VFS/PM/INPUT/driver-lib/IS rows point OUT of stage (with anchors).
- Ground truth: `com.h:65,498-507`; `ds.h`; `sysinfo.h:13`; `sys/sys/errno.h`;
  Rust `minix-types` com/ds_store (+ E-MINTYPES-SYS homes when landed), `minix-sys/src/ds.rs`.
- Knowledge list:
  | K-039 | 8-code closed set | K | `store.c` passim | refusal surface | S (99 §2) |
  | K-040 | cross-service user rows | C | 10 anchors (§2.1) | dependency map | S (99 §3) |
- Acceptance: (1) verb table numbers match §4.2 exactly (post-move audit);
  (2) errno rows each name the deciding doc section; (3) every §3 row carries a C anchor
  (no anchorless "RS uses DS" claims); (4) EDONTREPLY footnote gives `sys/sys/errno.h` home (K-042).

---

## 6. Change Table (old -> new, why, knowledge touched, direction rule applied)

Operation IDs: MV = move (intact), SH = shift-number (intact), RW = rewrite section, NW = new content,
AN = anchor/cross-ref update, AR = archive (no delete).

| ID | Type | Old | New | Reason | Knowledge (S=outgoing mapped / N=sourced) |
|----|------|-----|-----|--------|-------------------------------------------|
| C-1 | MV | 10-ds-subscribe-check (170 lines, all sections) | 07-ds-subscribe-check-notify | F-1: eliminate two admitted forward refs; causal register-before-publish | S: K-032..K-035, K-011-use → new §7 (all sections land; §6.1 anchor table) |
| C-2 | SH | 07-ds-publish | 08-ds-publish | Number cascade from C-1; content intact except intro/outro + ring pointer direction | S: K-026..K-028 → new §8 |
| C-3 | SH | 08-ds-retrieve | 09-ds-retrieve | Same cascade; mirror reference now points BACKWARD (08-new) | S: K-029 → new §9 |
| C-4 | SH | 09-ds-delete | 10-ds-delete | Same cascade; cascade-ring pointer now backward | S: K-030..K-031 → new §10 |
| C-5 | RW | 06 §ring one-liner + D4 area; 08-new §1.2/outro; 10-new §1.2/§1.6/outro; 07-new §1.1/intro/outro | Same files | Retarget pointers: 06/08-new/10-new cite 07-new (backward); 07-new intro justifies OD-1 + states runtime arms | S: K-034-effect lines (4 files × 1-3 lines) |
| C-6 | RW | 00 nav table + stage groups + §1.2/§1.3 numbers; 01 dispatch table + "07~11" shorthands; 99 §1.1/§2 rows | Same files | Number consistency post-move | S: numbering literals (AN sweep §8.2) |
| C-7 | NW | — | 01 new "build seam" subsection (Makefile + regcomp alias, ~15 lines) | GAP-1: only true coverage gap; K-041 has C evidence | N: K-041 sourced from `servers/ds/Makefile` |
| C-8 | NW | — | 01 reply-rule line + 99 errno footnote (EDONTREPLY home) | GAP-2 | N: K-042 sourced from `sys/sys/errno.h` |
| C-9 | RW | 11 §1.2/§1.4 snapshot-row narrative | 11 (Q-1 verdict box, exactly one place) | 03-vs-11 tension must resolve to a single argued verdict, not two parallel stories | S: K-017-use; pending Q-1 adjudication (§9) |
| C-10 | AN | In-stage numeric refs (16 hits), Rust comment refs (11 hits), external stage refs (10-stage-mib ×5 + 04-stage-pm ×1 + edge3.md) | New numbers | Breakage accounting (§8); code-comment edits are B-phase with fix-guard | — (mechanical) |
| C-11 | AR | draft/ (README + tmp_main.c.md + tmp_store.c.md); old filenames 07/08/09/10 | Archived, not deleted; old numbers recorded as redirect note in 00 | R-method: old docs are sources, not move targets | — |

Going-splitting rule check (sample of 10, per G6): C-1's stock (K-032..K-035 + journey diagram + INITIAL sweep +
EAGAIN row + anchor table rows) ALL land in new-07 contract (§5/07) — nothing unwritten; C-7/C-8 new knowledge
each carries a C evidence anchor — no invention. Full per-section landing proof is §8.1.

---

## 7. Missing-New-Chapter Resolution (Step 6: every §3 gap + every non-C item; no blanks)

- GAP-1 (Makefile alias) -> NW subsection in 01 (C-7). No new chapter: a 15-line build note does not
  carry a chapter (single-semantics: it belongs to the entry doc that owns `main.c`).
- GAP-2 (EDONTREPLY home) -> two one-liners (C-8). No new chapter.
- Non-C 10 items: ALL resolved "told where / why no chapter" in §3.5. Zero new chapters.
- Tests/ds behavior (K-044): folded into 07-new/08-new/09-new/10-new acceptance. Standalone test doc
  explicitly NOT created (plan §5.4 rule re-confirmed: test contracts ride with handlers).
- 192-vs-168 (Q-1): resolved INSIDE 11 (C-9), not as a new chapter.
- Net new chapters: **0**. The catalog keeps 14 docs; the rebuild's new-content surface is
  one subsection + retargeted seams + one adjudication box. This is stated plainly so B-phase
  cannot mistake "rebuild" for "rewrite every line": §5 contracts mark keep-vs-change per doc.

---

## 8. Anchor Migration & Breakage Cost

### 8.1 Section migration (all changed docs, per-section)

Conventions: O = move as-is (file rename only), R = reword seam, N = new.

**Old 10 -> New 07** (file rename `10-ds-subscribe-check.md` -> `07-ds-subscribe-check.md`):

| Old section | Content one-liner | New home | Type | Breakage note |
|-------------|-------------------|----------|------|---------------|
| 10 §1.1-§1.2 (readers/non-goals) | Subscriber-flow readers; engine-detail deferral | 07-new §1.1-§1.2 | R (deferral pointer stays `pattern.rs`; client pointer 12 unchanged) | Low |
| 10 §1.3 journey diagram | 8-step publish->notify->check->retrieve->delete flow | 07-new §1.3 | R (3 doc numbers re-anchored: 07->08-new, 08->09-new, 09->10-new) | Medium (most-copied figure) |
| 10 §1.4 subscribe 6 steps | Owner/existing/alloc/anchor/compile/write+INITIAL | 07-new §1.4 | O | None |
| 10 §1.5 match two gates | auth AND regexec; EreMatcher anchored semantics | 07-new §1.5 | O | None |
| 10 §2 C-analysis (subscribe/check/match/update) | Per-function C walks | 07-new §2 | O + intro line (OD-1 justification) | Low |
| 10 Rust/design/test/transition/see-also | pattern.rs wiring, D-table, test table, transition | 07-new §3-§7 | R (transition rewritten: "next is publish"; test table +K-043 rows) | Medium |

**Old 07 -> New 08 / Old 08 -> New 09 / Old 09 -> New 10**: all sections O (file rename only) EXCEPT:
each §1.2 "not covers" line + each outro "next stop" line + each ring-effect line (R -> backward pointer to 07-new).
Section counts: 07 has ~6 §§, 08 ~5 §§, 09 ~6 §§ — all land 1:1 (§8.1 table compressed by file since interiors are untouched).

**00 / 01 / 99 / 06 / 11 / 12**: interiors O; nav/dispatch/error tables + numeric shorthands R (C-6);
01 gains NW build-seam subsection (C-7); 11 gains Q-1 verdict box (C-9); 06 ring one-liner R (C-5).

### 8.2 Reference migration (every place citing an old number or filename)

| # | Citing location | Old reference | New target | Verify by |
|---|-----------------|---------------|------------|-----------|
| R-01..R-06 | 01 dispatch table + "07~11" shorthands (01-ds-init-main.md) | 07/08/09/10/11 numbers | 08/09/10/07/11 (runtime order kept, numbers new) | `grep -n "07~\|07-ds\|08-ds\|09-ds\|10-ds" 01-*.md` → 0 old hits |
| R-07..R-09 | 00 nav + groups (00-ds-overview.md) | stage-3 list 07..11 | new §4.2 table verbatim | diff nav vs §4.2 |
| R-10..R-11 | 99 §1.1 verbs + §2 EDONTREPLY row (99-*.md) | old numbers | new numbers + K-042 footnote | grep old numbers → 0 |
| R-12..R-16 | 06 ring pointer, 02 "handler chapters" shorthands, 03/05 transition lines, 12 intro chain | "10 细讲" / "07~11" / "下一站 07/10/12" | 07-new / new chain | in-stage grep sweep (16-hit baseline → 0, see cost) |
| R-17..R-27 | Rust comments: `notify.rs:5`, `check.rs:4`, `subscribe.rs:4`, `retrieve.rs:4`, `delete.rs:4`, `lib.rs:36,42-43,50` (+ 4 more `grep` hits = 11 total) | `10-ds-subscribe-check.md` / `08-ds-retrieve.md` / `09-ds-delete.md` / `07-ds-publish.md` | new filenames | `grep -rn "ds-publish\|ds-retrieve\|ds-delete\|ds-subscribe" os/servers/ds/src/` re-run in B-phase (code edits under fix-guard) |
| R-28..R-33 | External: `10-stage-mib/12-mib-remote-subtrees.md`, `10-stage-mib/plan.md`, `02-mib-message-contract.md`, `01-mib-init-main.md`, `06-mib-copy-io.md`, `04-stage-pm/06-event-subscription.md`, `edge3.md` (+ 2 `.design` hits: EXCLUDED, never touched) | `07-stage-ds` links / old numbers | new numbers | per-file grep in B-phase; `.design` hits explicitly out of scope |

### 8.3 Breakage-cost summary

- In-stage numeric references to moving docs: **16 hits** (measured `grep -c`, §0.2) across 01/00/99/06/02/03/05/12.
- Rust code-comment references: **11 hits in 6 files** (mechanical, B-phase + `cargo test -p minix-ds` re-green).
- External stage references: **7 files** (5 MIB + 1 PM + edge3.md); `.design` hits excluded by project rule.
- Hotspot: the journey diagram (old-10 §1.3) — most-copied figure; its 3 number swaps are the only
  medium-risk edit. Suggested batch method: B-phase edits C-1..C-6 in one pass per file with
  `grep -n "07-ds\|08-ds\|09-ds\|10-ds\|07~\|10 细讲\|那是 10\|那是 07" <file>` before/after each file
  (fix-guard: read ±5, one fix at a time, grep-verify, record fix-status).
- Total mechanical surface: ~35 reference edits + 4 file renames + 1 new subsection + 1 verdict box +
  6 retargeted seams. Rebuild cost is small BECAUSE the prose is already good (F-3) — the costing
  table exists precisely so "看不清成本的重建不做" is satisfied: cost is now visible.

---

## 9. Verification & Self-Gates

### 9.1 Four mechanical checks (§5.4)

1. **Forward-reference scan** (new order 00,01,02,03,04,05,06,07-new,08-new,09-new,10-new,11,12,99):
   each contract's Prereqs point only to earlier numbers — PASS (audited per contract in §5;
   07-new's "publish/delete will fire the ring" are declared post-pointers, listed in Post-refs, not Prereqs).
2. **Dependency DAG**: 00->01->02->03->04->05->06->07-new->{08-new,09-new,10-new}->11->12->99; 08-new<->09-new
   mirror is a backward cite, not a cycle — ACYCLIC, PASS. (No ring to break up.)
3. **Coverage**: all 44 pool items have a home (§5 knowledge lists) or explicit exclusion (GAP-2 registry in 02);
   K-041/K-042 carry C anchors — 100%, PASS. Deleted items: none (AR only archives draft/old filenames).
4. **Breakage accounting**: §8.2 lists every hit class with verify commands; totals 16+11+7 — COUNTED, PASS.

### 9.2 Self-gates (no skipping)

| Gate | Check | Result |
|------|-------|--------|
| G1 | C true order checkable (random 10 anchored) | PASS (§1 random-10, all file:line verified by direct read) |
| G2 | Pool complete: every C file + every non-C artifact has a home or explicit exclusion | PASS (§3.1-§3.2, §3.5 ten-for-ten; arch-dependent: none — verified) |
| G3 | New catalog zero forward reading-refs (per-doc Prereqs scan) | PASS (§9.1(1); the two admitted old forward refs are the reason for C-1) |
| G4 | DAG acyclic; any ring gets a split plan | PASS (§9.1(2), acyclic, no split needed) |
| G5 | 100% coverage: every pool item homed or deleted-with-reason; new items evidenced; deletions listed | PASS (§9.1(3); deletions: none) |
| G6 | Every split/merge writes stock-outgoing; every new states source (sample 10) | PASS (§6 table + C-1 full landing in §8.1; C-7/C-8 sourced; no merges exist) |
| G7 | Every contract has 7 elements (position/covers/non-covers/prereqs/post-refs/ground-truth/knowledge-list/acceptance) | PASS (14/14 in §5) |
| G8 | Anchor table covers every changed doc per section; ref table covers docs + code comments | PASS (§8.1 per-section for 4 moved files + seam files; §8.2 R-01..R-33) |
| G9 | Factual claims anchored (random 10 re-checked; speculations labeled) | PASS — anchors re-verified: `table.c:52`, `main.c:265-267`, `com.h:498-507`, `ipc.h:94-115`, `ds.h:11-32`, `store.c:210` (mask gate), `store.c:372` (INTERNAL mask), `store.c:570-572` (writeback), `dmp_ds.c:30-41` (raw interpret), `errno.h` (EDONTREPLY). Speculative/open items explicitly labeled: Q-1, Q-2 below. |

### 9.3 Conclusion

Blueprint COMPLETE and directly executable: B-phase needs no further trade-off decisions except
two adjudications left explicitly to the user (below). Default execution: C-1..C-11 in §6 order,
acceptance per §5 contracts, batch method per §8.3, then `cargo test -p minix-ds` (112-green baseline)
+ full `grep` zero-old-hit sweep.

### 9.4 Questions for user adjudication (no guessing on these two)

- **Q-1 (192 vs 168 — one compatibility verdict needed).** 03 §1.6 locks the 192-byte raw-copy ABI
  (C `store.c:671` + IS `dmp_ds.c` raw interpret agree); 11 §1.2/§1.4 describes production rendering
  168-byte scalar snapshot rows (`DsEntrySnap`) with an `image_bytes() == 168*128` lock. Both cannot be
  "the" IS-facing truth without an explicit compatibility argument (translate layer? IS-side migration?
  dual-test lock?). Proposed: B-phase writes the single verdict box in 11 (C-9) in the variant you pick:
  (a) 192-raw stays the wire truth and 168-rows are internal-only (then 11's test lock must target the
  192 rendering); (b) 168-rows are the new wire truth (then 03's ABI section + IS consumer contract need
  joint rewrite with 08-stage-is). My recommendation: (a) — C ground truth + IS C reader both say 192.
- **Q-2 (confirm the move).** The catalog's only structural change is C-1 (old-10 -> new-07) to kill two
  admitted forward references. If you judge rename churn cost > forward-ref cost, the fallback is:
  keep numbers, downgrade 07/09 ring lines to effect-summaries + keep 10 where it is, and record a
  standing exception to §5.1(1) in 00. My recommendation: keep C-1 (35 mechanical edits is cheap;
  a standing exception to the no-forward-ref hard standard is expensive — every future doc copies it).
