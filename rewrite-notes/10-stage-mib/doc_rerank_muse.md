# 10-stage-mib Document Rebuild Blueprint (muse)

```text
your_name(AI agent name) = muse
target_dir(Attention working directory) = rewrite-notes/10-stage-mib
repo_root = /home/xzhao/github/minix-rs
commit = 3849cc792 (docs(edge3): S12 follow-up; verified 2026-09-20 by `git log --oneline -3`)
task = R-phase rebuild blueprint: output target_dir/doc_rerank_muse.md, touch no prose files.
       C code alone is not enough; also read: all doc headers in this dir, the os/ entry
       points, the 00-*-overview.md navigation tables.
constraint = Must not reference .design/ or tmp_design_and_todo/; any landed artifact must
       carry the _muse suffix; must not read or copy any other AI's doc_rerank_* product.
doc_language = This blueprint is written in English (per muse special requirement).
       B-phase prose language follows the stage convention (Chinese, as all 24 docs and
       plan.md/todo.md are Chinese) unless the user directs otherwise — recorded as
       open question Q1 in §9.
```

## 0. Metadata

### 0.1 Scope

- **In scope (numbered docs, 24):** `00-mib-overview.md`, `01`–`22-mib-*.md`, `99-mib-global-concepts.md`.
- **Reference material (not rebuilt, read as evidence):** `plan.md` (467 lines, coverage contract + ARCH A-1–A-12),
  `todo.md` (317 lines, first architecture review + execution status), `README.md` (48 lines),
  `draft/README.md` (placeholder), `edge_todo.md` (cross-stage entries, read head only).
- **Out of scope:** any `doc_rerank_*` product by other AIs (not read, per constraint);
  `.design/` and `tmp_design_and_todo/` (not referenced, per project rule);
  Minix3 C sources outside the MIB surface (kernel/PM/VFS/VM/LWIP/UDS/IPC producer internals —
  only their MIB-facing contract points are in scope).
- **Late-found out-of-scope topics** are listed in §3.5, not dropped silently.

### 0.2 Reading list (actually read)

1. All 24 doc headers (§1 concept/§1.1 readers+prereqs/§1.2 non-goals fully read; bodies sampled for
   knowledge-pool extraction). Line counts (§0.4).
2. C ground truth: `minix3/minix/servers/mib/` — all 8 `.c` (4990 lines) + `mib.h` (390 lines);
   function-entry grep for every file (§1 table anchors).
3. Protocol/client surface: `minix/include/minix/com.h` (MIB block), `minix/lib/libsys/rmib.c` (1089 lines),
   `minix/include/minix/rmib.h`, `lib/libc/gen/sysctl.c` (391 lines, CTL_USER branch read),
   `lib/libc/gen/sysctlgetmibinfo.c` (611 lines, existence + size only).
4. Boot evidence: `minix/kernel/table.c:52-64` (slot order read), `minix/servers/rs/table.c:25`
   (`{MIB_PROC_NR,"mib",SRV_F}` read), `servers/mib/Makefile:11` (`PROG= mib` read).
5. Boundary: `00-master-plan/README.md` (full), `edge_todo.md` (head + MIB entries via todo.md §5),
   `09-stage-init/00-init-overview.md` (head + §boundary — previous stage).
6. Rust entries: `os/servers/mib/src/` tree listing + line counts (total 13629 lines, 2026-09-20;
   drift vs todo.md's 7187 lines / 107 tests of 2026-09-15 is growth since the scan — flagged, not
   a contradiction); `os/libs/minix-types` MIB touch points (4 comment anchors read);
   `os/libs/minix-sys/src/rmib.rs` (existence only).
7. Style example: `01-stage-kernel/06-todo.md` NOT re-read in full (contract format already embedded
   in the R-prompt §5 template, which this blueprint follows verbatim); no conclusions borrowed.

### 0.3 Commands run (evidence excerpts)

```bash
wc -l rewrite-notes/10-stage-mib/*.md   # 24 docs: 82–247 lines each (§0.4)
wc -l minix3/minix/servers/mib/*.c *.h                        # 4990 C + 390 h (§1)
git rev-parse --short HEAD                                    # 3849cc792
sed -n '52,64p' minix3/minix/kernel/table.c                   # MIB slot after TTY, before VM
rg -n "mib_dispatch|mib_query|...|mib_mount" tree.c           # all tree.c entries located (§1)
rg -n "^mib_|^update_tables|^get_" remote.c kern.c proc.c hw.c vm.c minix.c  # server entries
rg -n "MIB_PROC_NR|MIB_BASE|MIB_SYSCTL|COMMON_MIB" com.h      # wire numbers
rg -n "CTL_MAXNAME|CREATE_BASE|SYSCTL_VERSION" sysctl.h mib.h tree.c  # constants
rg -n "10-stage-mib|mib-.*\.md" os/ notes/ --glob '*.rs' --glob '*.md' | rg -v doc_rerank
  # code-comment refs: message.rs:3149, sysctl_abi.rs:19, mib.rs:8, com.rs:241-267
ls os/servers/mib/src{,/*} + wc -l                           # 13629 lines, 2026-09-20
```

### 0.4 Document inventory (number, title, lines, header-declared prereqs/boundary)

| # | File | Lines | Header prereqs | Header non-goals (handoffs) |
|---|------|-------|---------------|------------------------------|
| 00 | 00-mib-overview.md | 108 | none | all mechanisms → 01–22/99 |
| 01 | 01-mib-init-main.md | 229 | 00 + C + IPC idea | fields→02; lookup→05/10/09/13–20; copy/auth→06/07; tree→03/04 |
| 02 | 02-mib-message-contract.md | 247 | 01 | triage→01; grants→06/12; flag behavior→03; handlers→09/13–20 |
| 03 | 03-mib-node-model.md | 196 | 02 | macros→04; find→05; dynode life→08; copy/auth/mount→06/07/12 |
| 04 | 04-mib-static-tree-init.md | 153 | 01 + 03 | tables→13/14/15; dynodes→08; lookup→05; endpts→12 |
| 05 | 05-mib-tree-lookup.md | 122 | 03 + 04 | walk→10; insert/delete→08; tables→04/13/14/15 |
| 06 | 06-mib-copy-io.md | 163 | 01 + 02 | execution→transport (A-12); handlers→09; journey→12; auth→07 |
| 07 | 07-mib-auth-model.md | 149 | 02 + 03 | PM answer→transport; call sites→08/09/10/11; copy→06 |
| 08 | 08-mib-dynamic-nodes.md | 165 | 03 + 05 + 07 | surgery→arena; copyin→06; echo→11; enum→11 |
| 09 | 09-mib-data-access.md | 154 | 03 + 06 + 07 | func nodes→13–20; dispatch→10; execution→06/transport |
| 10 | 10-mib-dispatch.md | 174 | 05 + 07 + 09 | find→05; readwrite→09; meta bodies→11/08; remote body→12 |
| 11 | 11-mib-query-describe.md | 157 | 02 + 07 + 10 | dispatch→10; desc ownership→08; copyin_str→06 |
| 12 | 12-mib-remote-subtrees.md | 206 | 02 + 03 + 10 | verdict→10; client letter→22; dispatch call→10 |
| 13 | 13-mib-subtree-kern.md | 185 | 03 + 09 + 10 + 12 | proc queries→17/18/19; fetch execution→transport; 14/15 |
| 14 | 14-mib-subtree-vm-hw.md | 167 | 09 + 13 | fetch execution→transport; 13 done, 15 next |
| 15 | 15-mib-subtree-minix.md | 168 | 02 + 03 + 08 | test87 runner→C contract; procs→16–20; counters→transport |
| 16 | 16-mib-proc-tables.md | 167 | 06 + 10 | row meaning→producers; formats→17–20; copyout→06 |
| 17 | 17-mib-proc-lwp.md | 220 | 16 + 06 + 10 | tables→16; copyout→06; 18/19/20; layout→A-4 (P1-3 note) |
| 18 | 18-mib-proc2.md | 165 | 16 + 17 + 06 + 10 | state→17; tables/time→16; mem→VM; 19/20 |
| 19 | 19-mib-proc-args.md | 152 | 16 + 06 + 10 | state/proc2→17/18; snapshot→20; copyout→06 |
| 20 | 20-mib-minix-proc.md | 141 | 16 + 17 + 02 | state→17; 18/19; copyout→06; ProcFS files→FS stage |
| 21 | 21-mib-client-libc.md | 100 | 02 + 01 | server→01–20; rmib→22; no libc rewrite (A-10) |
| 22 | 22-mib-rmib-client.md | 140 | 12 + 02 + 06 | server side→12; transport execution→other code; libc→21 |
| 99 | 99-mib-global-concepts.md | 82 | all (lookup) | all mechanisms (elsewhere) |

Total prose: 24 docs, 82–247 lines each. **No document exceeds 250 lines — there is no length
monster; the soft 3000-line allowance is not needed here.** Length control (§4.5) therefore means
*keeping* this scale, not splitting for size.

### 0.5 C source inventory (file → existing doc → verdict)

| C file | Lines | Existing coverage | Re-verified entries (this blueprint) |
|--------|-------|-------------------|--------------------------------------|
| main.c | 492 | 01/04/06/07 | mib_root/table :36–64; copy family :64–258; authed :259–275; sysctl :277–383; init :384–413; startup :415–431; main :433–492 |
| tree.c | 1842 | 04/05/06/07/08/09/10/11/12 | find :35; scratch :22–23; copyout_node :91; query :180; check_name :248; copyin_str :372; upgrade :427; add :457; create :487; remove :785; destroy :839; copyout_desc :926; describe :973; getptr :1098; read :1131; readwrite :1309; dispatch :1333; recurse :1480; init :1520; mount :1544; unmount :1790 |
| remote.c | 477 | 12 | remote_init :41; down :56; get_label :82; do_register :110; register :198; do_deregister :240; deregister :292; remote_info :317; remote_call :379 |
| kern.c | 508 | 13 | securelvl :18; clockrate :36; profiling :64; hardclock :77; rootdev :96; ccpu :120; cp_time :135; consdev :193; forkfsleep :209; drivers :223; boottime :287; ipc_info :307; tables+init :319–508 |
| proc.c | 1288 | 16–20 | update_tables :47; get_mslot :144; ticks_to_timeval :164; fill_wmesg :186; get_lwp_stat :225; fill_lwp_* :399–488; kern_lwp :510; fill_proc2_* :601–688; kern_proc2 :792; kern_proc_args :919; minix_list :1178; minix_data :1217 |
| hw.c | 140 | 14 | physmem :19; usermem :46; ncpuonline :82; table+init :98–140 |
| vm.c | 154 | 14 | loadavg :12; uvmexp2 :79; table+init :120–154 |
| minix.c | 89 | 15 | tables :14–84; init :85 |
| mib.h | 390 | 03/04/02 | oldp/newp/call :33–50; flag aliases :72–74; node :188; dynode :244; macros :252–315 |

### 0.6 Non-C artifact inventory (§4.3 item 3, checked one by one)

| # | Artifact class | Found | Location |
|---|---------------|-------|----------|
| 1 | Link scripts | none MIB-specific (user-space server, no own linker script) | — (answered in §3.6-N1) |
| 2 | ASM / trap entries | none MIB-specific (SEF receive via lib) | — (§3.6-N3) |
| 3 | Boot chain + protocol | boot_image slot; RS priv table; VMINHIBIT gate | kernel/table.c:52–64; rs/table.c:25; kernel/main.c:265–267 cited via plan §1.2 (re-verify exact lines in B phase — *not independently re-read*) |
| 4 | Image layout | slot order = module order (ds→…→tty→mib→vm→pfs→mfs→init) | kernel/table.c:52–64 |
| 5 | User-space startup / runtime loading | RS loads non-boot_image servers; MIB is boot_image-direct so not RS-loaded | 00-master-plan/README.md §boot two-layer semantics |
| 6 | Cross-module iface + wire format | com.h / ipc.h / sysctl.h ×2 / COMMON_MIB_* / grants | §0.5 + §1 |
| 7 | Build scripts / toolchain | trivial makefile | servers/mib/Makefile:11 |
| 8 | Test infra + simulator scripts | test87 (3662 lines); rmibtest.c; t_sysctl cited via plan §5.4 (*existence of t_sysctl not re-verified — flagged*) | minix/tests/test87.c; minix/tests/rmibtest/rmibtest.c |

### 0.7 Reference清单

- Doc↔doc: every numbered doc ends with a "stage docs" cross-ref list (sampled: 04/07/08/10/15/21);
  numbering-stable rebuild keeps these cheap (§8.2).
- Code→doc: 4 anchors — `os/libs/minix-types/src/ipc/message.rs:3149` (02),
  `src/types/sysctl_abi.rs:19` (02 + 17/18/20 consumers), `src/ipc/mib.rs:8` (02),
  `src/types/com.rs:241–267` (02 + 01). All point at docs whose numbers this blueprint **keeps**,
  so zero code-comment churn (§8.3).

---

## 1. C True Order (runtime truth, rebuilt from C — not paraphrased from docs)

**Stage-type judgment: service event-loop type** (not a boot-chain type, not a syscall-collection
type). Evidence: `main()` in `main.c:433–492` is `mib_startup()` followed by `for(;;)
sef_receive_status → classify → handler → conditional reply`. Therefore this blueprint uses the
event-loop organization: *why the service exists → birth + init → message interface → core data
structure → per-scenario request handling → enumeration/misc → neighbor protocols* (R-prompt §9).
The boot chain is only the *birth* segment, not the skeleton.

### 1.1 Birth segment (boot → init, straight line)

| # | Action | Anchor | Note |
|---|--------|--------|------|
| T-01 | Boot image reserves MIB slot after TTY, before VM | kernel/table.c:52–64 (`{MIB_PROC_NR,"mib"}` in array) | Slot order ≠ run order (master-plan two-layer semantics) |
| T-02 | RS priv table marks MIB as boot server | rs/table.c:25 (`{MIB_PROC_NR,"mib",SRV_F}`) | SRV_F = server flags class |
| T-03 | Kernel inhibits all user procs until VM builds pages | kernel/main.c:265–267 cited via plan §1.2 | MIB cannot run before VM unblocks it |
| T-04 | `main()` calls `mib_startup()` | main.c:433–436 | |
| T-05 | Fresh-boot and restart both register `mib_init`; restart loses all dynamic state, static tree only | main.c:415–431 (comment "only the static node tree is still better than not running at all" read in §0.2 dispatch-loop excerpt context) | RestartLossy semantics; single most important lifecycle fact |
| T-06 | `mib_init` wires 4 subtree tables (extern-array two-phase fill, sizeof impossible across files) | main.c:384–404 (`mib_kern_init(&mib_table[CTL_KERN])` + vm/hw/minix) | C-limitation shape, vanishes in Rust |
| T-07 | `mib_tree_init` walks the whole tree: parent links, child counts, version inheritance | tree.c:1520 (`mib_tree_init`) + :1480 (`mib_tree_recurse`) | |
| T-08 | `mib_remote_init` zeroes the endpoint table (32 slots = 1<<5) | remote.c:41 + mib.h:181 (`MIB_EID_BITS 5`) + remote.c:25 | |

### 1.2 Loop segment (one request's life — the stage's main line)

| # | Action | Anchor | Note |
|---|--------|--------|------|
| T-09 | `sef_receive_status(ANY)`; non-OK receive panics | main.c:443–448 | |
| T-10 | Notify (non-call status) → print + continue, never answered | main.c:450–455 (`is_ipc_notify`) | |
| T-11 | SENDREC-only gate: non-SENDREC returns EDONTREPLY (silence) | main.c:294–296 (`IPC_STATUS_CALL != SENDREC → EDONTREPLY`) | First of six decode gates |
| T-12 | namelen must satisfy 0 < n ≤ 12 | main.c:303–304 (`CTL_MAXNAME` = 12, sysctl.h:75) | Gate 2 |
| T-13 | Name fetch: ≤8 ints inline from message, >8 via one `sys_datacopy` | main.c:309–318 (`CTL_SHORTNAME` boundary) | Gate 3; the only kernel copy in the entry path |
| T-14 | oldp pairing (forgiving: NULL addr tolerates nonzero len) | main.c:324–334 | Gate 4 |
| T-15 | newp pairing (strict: addr/len must both be nonzero, else both dropped, NetBSD parity) | main.c:340–346 | Gate 5 |
| T-16 | `mib_dispatch(&call, oldp, newp)` consumes the name component by component from `mib_root` | tree.c:1333–1350 (`for (parent=&mib_root; namelen>0; parent=node)`) | Heart of the service |
| T-17 | Negative trailing id = meta-op switch: QUERY/CREATE/DESTROY/DESCRIBE; CREATESYM/MMAP → EOPNOTSUPP; must be last component | tree.c:1361–1382 | Gate 6 (partly) + meta dispatch |
| T-18 | Per level: `mib_find` (static array O(1) / sorted dynamic list O(n) early-stop) | tree.c:35–88 | Hot path, one call per name component |
| T-19 | Per level: PRIVATE check via cached superuser answer (≤1 PM query per call) | tree.c:1393–1394 + main.c:259–275 (`mib_authed`) | Visibility before shape |
| T-20 | REMOTE node → `mib_remote_call`: three grants, forward, three outcomes (done / dead-continue-locally / unmounting-deregister) | remote.c:379–477; dispatch continues tree.c:1396–1430 region (outcome handling; exact lines per plan §5.3, re-verify in B) | Only cross-process leg in the stage |
| T-21 | Leaf → `mib_readwrite`: stage-then-commit, verify callback, bool sanitizing | tree.c:1309–1330 | |
| T-22 | Func node → handler (`mib_func_ptr`), subtree consumed | mib.h:167–169 typedef; call site tree.c:1466 region | |
| T-23 | Return path: short old buffer → partial copy + full length + ENOMEM (not a pure failure) | main.c:341–356 region per plan (re-verify exact lines in B) | Two-call transaction idiom |
| T-24 | Create-collision: EEXIST + existing node echoed + reslen channel | tree.c:652–664 region per plan (re-verify in B) | "Error with length" idiom |
| T-25 | Default message numbers: SENDREC → ENOSYS, others silent | main.c:470–489 region per plan | Second refusal shape |
| T-26 | Reply unless EDONTREPLY; `m_out.m_type = r; ipc_sendnb` | main.c:486–489 region per plan | Single reply rule |

### 1.3 Remote-subtree second line (register → mount → forward → death)

| # | Action | Anchor | Note |
|---|--------|--------|------|
| T-27 | Producer calls `rmib_register` (libsys) → `asynsend3(MIB_REGISTER)` one-way | rmib.c (1089 lines, existence+size verified) | No reply expected (avoids cross-deadlock) |
| T-28 | `mib_register`: SENDREC→ENOSYS reject; DS label lookup; slot allocate; `mib_mount` | remote.c:198–238 | |
| T-29 | `mib_mount` policy: path ≥2 (no top-level takeover); flags highly restricted; temp-node vs obscure-existing; REMOTE stamp + endpoint linkage | tree.c:1544–1780 (policy :1572–1610 read) | Security core of the stage |
| T-30 | Query hitting mountpoint → `mib_remote_call` with 3 relay grants; producer answers COMMON_MIB_CALL | remote.c:379–446 | |
| T-31 | Producer death → `mib_down` clears all its mountpoints; non-temp nodes continue locally (ERESTART) | remote.c:56–80 | |
| T-32 | Producer returns ERESTART → `mib_do_deregister`; exit path → `mib_unmount` restores covered nodes | remote.c:240–315; tree.c:1790 | |

### 1.4 Proc-snapshot line (pull → index → fill → copyout)

| # | Action | Anchor | Note |
|---|--------|--------|------|
| T-33 | `update_tables`: once per tick max; failure latch (never retry); PMAGIC/MP_MAGIC scan; PID hash build | proc.c:47–142 (latch + throttle + magic read) | Most expensive op in the stage (hundreds of KB per pull) |
| T-34 | Helpers: `get_mslot` (PID→slot), `ticks_to_timeval`, `fill_wmesg` (6 wchan classes) | proc.c:144–216 | |
| T-35 | LWP fill (stat machine ZOMB/DEAD/STOP/RUN/SLEEP + common/kern/user) | proc.c:225–595 | → ps/top bytes |
| T-36 | PROC2 fill + filter (ALL/PID/SESSION/PGRP/TTY/UID/RUID/GID/RGID) | proc.c:601–917 | Same data, second layout |
| T-37 | PROC_ARGS page walk with copy budget (attack-aware truncation) | proc.c:919–1176 | Reads *another process's* memory |
| T-38 | MINIX list/data for ProcFS; negative PID = kernel task (differs from LWP -1 = whole table) | proc.c:1178–1288 | ProcFS consumer contract |

---

## 2. Knowledge Pool (deduplicated; C + docs + Rust truth)

Source types: **S** = stock (from existing docs) · **N** = new (from C/non-C/Rust, absent in docs).
Knowledge types: C=concept, M=mechanism, D=data structure, P=interface/protocol, I=constraint/invariant,
A=arch-evolution, T=tool/engineering, E=test-nature.

### Group A — lifecycle + main loop (01, 04-part, 12-part)

| ID | Name | Type | Src | Stock locations (main telling point ★) | Anchor | Reader benefit: answers… |
|----|------|------|-----|----------------------------------------|--------|--------------------------|
| K-001 | Boot slot vs run order (two-layer boot semantics) | C | S | 00§2★, 01§1.3, plan §1.1 | kernel/table.c:52–64; rs/table.c:25 | Why is MIB early in the image but not early to run? |
| K-002 | VMINHIBIT gate (VM unblocks MIB) | M | S | 00§2, 01§1.3★ | kernel/main.c:265–267 via plan | What must happen before MIB is schedulable? |
| K-003 | SEF fresh/restart double registration | M | S | 01§2★, 04§2 | main.c:415–431 | What survives a restart? (static tree only) |
| K-004 | RestartLossy: dynamic nodes all dropped | I | S | 01, 04★, 08 | main.c:415–431 comment | Why does create-then-restart lose data by design? |
| K-005 | Receive/notifyHandling (notify → print + continue) | M | S | 01★ | main.c:443–455 | Why does MIB never answer notifications? |
| K-006 | Three-call switch (SYSCTL/REGISTER/DEREGISTER) | P | S | 01★, 02, 12 | main.c:460–489 region; com.h:1026–1028 | Which three letters does MIB understand? |
| K-007 | Default-number double refusal (SENDREC→ENOSYS vs silent) | M | S | 01★ | main.c:470–489 region | Why do two wrong callers get different treatments? |
| K-008 | Single reply rule (EDONTREPLY suppresses) | I | S | 01★ | main.c:486–489 region | When is silence correct? |

### Group B — wire contract (02, 21-part, 22-part)

| ID | Name | Type | Src | Stock locations | Anchor | Reader benefit |
|----|------|------|-----|-----------------|--------|----------------|
| K-009 | Call numbers + NR_MIB_CALLS=3 + IS_MIB_CALL | P | S | 02★, 99 | com.h:1022–1030 | How are MIB calls recognized on the wire? |
| K-010 | Six message structs (3 in + 3 out directions) | D | S | 02★ | ipc.h (plan §5.2; re-verify lines in B) | What fits in 56 bytes and what needs a copy? |
| K-011 | CTL_SHORTNAME=8 inline vs datacopy name fetch | M | S | 01★, 02, 06 | main.c:309–318 | Why do long names cost a kernel copy? |
| K-012 | sysctlnode/sysctldesc exchange layouts (NetBSD VERS_1) | D | N | 02 (values), 11 (produce)★ | sys/sys/sysctl.h:1382–1450 region via plan; Rust sysctl_abi.rs | What bytes does userland parse? |
| K-013 | SYSCTL_VERSION pin (VERS_1) + version gates (create/query/mount/describe) | I | S | 02, 08, 11★ | sysctl.h:132–133; tree.c:194,536,862,987; :1583 | Why do wrong-version requests die at four doors? |
| K-014 | ENOMEM-with-length (partial copy + full length, two-call idiom) | M | S | 01★, 02, 09, 21 | main.c:341–356 region | How does the caller size its retry buffer? |
| K-015 | EEXIST-with-echo (collision carries existing node + reslen) | M | S | 08★, 01, 02 | tree.c:652–664 region | How does create report "already there, here it is"? |
| K-016 | ERESTART as internal remote signal (continue locally / deregister) | M | S | 10★, 12 | tree.c:1410–1416 region; remote.c:56–80,240–315 | When does "dead producer" become "answer locally"? |
| K-017 | CTLTYPE_/CTLFLAG_ value tables + mask discipline | D | S | 02 (values)★, 03 (behavior) | sysctl.h:92–164 region | Which bits are wire, which are behavior? |
| K-018 | Meta-identifier set (QUERY/CREATE/DESTROY/DESCRIBE + EOPNOTSUPP pair) | P | S | 10★, 02 | tree.c:1361–1382 | Which negative ids are verbs, which are refused? |

### Group C — tree model + static init + lookup (03, 04, 05)

| ID | Name | Type | Src | Stock locations | Anchor | Reader benefit |
|----|------|------|-----|-----------------|--------|----------------|
| K-019 | mib_node giant union (5 roles, field meaning depends on type+flags) | D | S | 03★ | mib.h:188–220 | How do you read any field correctly? |
| K-020 | 4-way node matrix (PARENT × REMOTE) + leaf/func/data shapes | C | S | 03★, 10 | mib.h:72–74 aliases; tree.c:1333+ dispatch | What shapes can a node take? |
| K-021 | mib_dynode (inline name/data/desc, OWNDATA/OWNDESC ownership) | D | S | 03★, 08 | mib.h:244–250 | Who frees what on destroy? |
| K-022 | Counts (nodes/objects/remotes) + minix.mib.* self-report | M | S | 03, 04★, 15 | tree.c counters; minix.c tables | How does the tree report on itself? |
| K-023 | Scratch buffer (shared, page-or-desc sized, aligned) | D | S | 03★, 06, 09 | tree.c:22–23 | Where do oversized temporaries live? |
| K-024 | IS_STATIC_ID rule + static O(1) vs sorted-list O(n) early-stop | M | S | 05★, 03 | tree.c:21–27,34–88 | Why two lookup paths, when does each hit? |
| K-025 | Empty static slot ≠ miss (must still walk dynamic list) | I | S | 05★ | tree.c:34–88 | What lookup bug does this rule prevent? |
| K-026 | Seven top slots + MIB_* macros + two-phase wiring (extern-array sizeof limit) | M | S | 04★ | main.c:36–64,384–404; mib.h:252–315 | Why "empty slots first, fill at boot"? |
| K-027 | mib_tree_init/recurse (parent links, counts, version inheritance) | M | S | 04★ | tree.c:1480–1536 | When does the tree become walkable? |
| K-028 | Static-slot parity (kern 45+39 / vm 4+9 / hw 10+6, exclusions 1:1 with C comments) | E | S | 13/14★ (todo §1.2) | kern.c:332–498; vm.c:120–144; hw.c:98–130 | How do you prove no slot was lost? |

### Group D — copy IO + auth (06, 07)

| ID | Name | Type | Src | Stock locations | Anchor | Reader benefit |
|----|------|------|-----|-----------------|--------|----------------|
| K-029 | oldp/newp opaque bodies (endpt+addr+len+cursor) | D | S | 06★ | main.c:64–90 region | What travels with every byte move? |
| K-030 | copyout clamp (old buffer is a cap, surplus silently truncated) | M | S | 06★, 09 | main.c:106–147 region | Why is short-buffer a success-with-ENOMEM, not corruption? |
| K-031 | copyin exact-match + copyin_aux assertion-contract | M | S | 06★ | main.c:159–210 region | When is new data accepted byte-exact? |
| K-032 | copyin_str paged guessing (NUL scan across pages) | M | S | 06★ | tree.c:372–421 | How are strings copied without trusting length? |
| K-033 | relay_oldp/relay_newp direction+presence verdicts | M | S | 06★, 12 | main.c:211–258 region | Which way do grants point for remote legs? |
| K-034 | datacopy vs grant transport model (A-12 seam) | P | S | 06★ | safecopies.h:64–65; kernel copy/grant via plan | Who actually moves bytes in production? |
| K-035 | mib_authed cached superuser (≤1 PM getnuid per call) | M | S | 07★ | main.c:259–275 | Why doesn't every gate pay an IPC? |
| K-036 | Four permission bits (PRIVATE/ANYWRITE/READWRITE/PERMANENT) + superuser model | I | S | 07★, 03 | const.h SUPER_USER; tree.c gates | Which bit gates visibility vs write vs structure? |

### Group E — dynamics + data + dispatch + query (08, 09, 10, 11)

| ID | Name | Type | Src | Stock locations | Anchor | Reader benefit |
|----|------|------|-----|-----------------|--------|----------------|
| K-037 | check_name/scan (legality + conflict + insert-point report) | M | S | 08★, 05 | tree.c:248–360 | Where does a new node go, or why can't it? |
| K-038 | create validation chain (cheapest-first, all before malloc) | M | S | 08★ | tree.c:487–777 | Why does create check 15 things before allocating? |
| K-039 | Dynamic id allocation (CREATE_BASE=1024, max(parent_size,…)) | M | S | 08★ | sysctl.h:78; tree.c:324 | How are runtime ids chosen without collision? |
| K-040 | add/remove surgery + RemoveDelta + version bump | M | S | 08★ | tree.c:427–481,785–917 | What changes in the tree on birth/death? |
| K-041 | upgrade chain-climb (parent version cascade) | M | S | 08★ | tree.c:427–456 region | How do readers detect concurrent change? |
| K-042 | getptr lane select + read/write/readwrite stage-then-commit | M | S | 09★ | tree.c:1098–1330 | Why never update a node in place? |
| K-043 | String length idiom (read len+1 with NUL; write tolerates missing NUL; full-slot-no-NUL refuses) | I | S | 09★ | tree.c:1141,1203,1211–1214,1274–1286 | What is the string invariant? |
| K-044 | verify callback + bool sanitizing + unprivileged-big-write EPERM | M | S | 09★, 07 | tree.c:1160–1300 region | Which writes need a second opinion? |
| K-045 | dispatch per-level wheel (consume-one-component + 8 judgments) | M | S | 10★ | tree.c:1333–1475 | What happens at each name level? |
| K-046 | Remote-outcome contract (done / dead-continue / unmounting) owned by 10 | C | S | 10★, 12 | tree.c:1396–1430 region | What three fates can a remote leg have? |
| K-047 | query enumeration (static-then-dynamic, unsorted, userland sorts) | M | S | 11★, 21 | tree.c:180–241 | Why doesn't the server sort? |
| K-048 | describe (desc stream, 4-align, write-once set path) | M | S | 11★ | tree.c:973–1090 | How are descriptions read and set? |
| K-049 | copyout_node/copyout_desc serialization (PRIVATE shape-kept-zeroed / length-zero) | M | S | 11★ | tree.c:91–174,926–967 | What does an unprivileged enumerator see? |

### Group F — remote server + rmib client (12, 22)

| ID | Name | Type | Src | Stock locations | Anchor | Reader benefit |
|----|------|------|-----|-----------------|--------|----------------|
| K-050 | endpts table (32 slots, eid addressing, nodes chain heads) | D | S | 12★ | remote.c:25–39; mib.h:181 | Where do mounted subtrees hang? |
| K-051 | One-way register/deregister (SENDREC→ENOSYS anti-deadlock) | P | S | 12★, 02 | remote.c:198–315 | Why must mount letters never wait? |
| K-052 | DS label check (mib_get_label) as mount identity | M | S | 12★ | remote.c:82–108 | How does MIB know who is knocking? |
| K-053 | Mount walk (fathers must be true-local-nonprivate) + temp-vs-obscure | M | S | 12★ | tree.c:1600–1780 | When is a node covered vs freshly made? |
| K-054 | COMMON_MIB_INFO name/desc fetch for temp nodes | P | S | 12★ | remote.c:317–378 | Where do temp nodes get names? |
| K-055 | COMMON_MIB_CALL forward + 3-grant open/reverse-revoke + death detect | M | S | 12★ | remote.c:379–477 | How is a question lent to its owner? |
| K-056 | rmib lightweight replica + sparse nodes (sorted-unique-nonempty rules) | D | S | 22★ | rmib.c:1–60 region; rmib.h | Why does the client re-implement half the server? |
| K-057 | Producer set (kern.ipc / net.inet+inet6+minix.lwip / net.local) | C | S | 12★, plan §5.4 | ipc/main.c:24–112; lwip/mibtree.c:35–72; uds/stat.c:68–169 | Whose subtrees are borrowed? |

### Group G — subsystem subtrees (13, 14, 15)

| ID | Name | Type | Src | Stock locations | Anchor | Reader benefit |
|----|------|------|-----|-----------------|--------|----------------|
| K-058 | kern table: 2 verify-writable + 10 computed + 45 data + 39 excluded + ipc placeholder | D | S | 13★ | kern.c:17–508 | What does the OS say about itself? |
| K-059 | A-9 exclusion contract (40+ "not yet supported" slots kept, honest errors) | I | S | 13★, 14, 02 | kern.c/vm.c/hw.c comment slots | Why are empty slots a promise, not debt? |
| K-060 | vm computed trio (loadavg windows, page-shift, width truncation) | M | S | 14★ | vm.c:12–110 | Which three numbers need real math? |
| K-061 | hw table (physmem/usermem/ncpuonline computed + build strings) | D | S | 14★ | hw.c:18–140 | Which hardware facts are live vs baked? |
| K-062 | minix test subtree (12 nodes, each an assertion incl. alignment) | E | S | 15★ | minix.c:14–84; mib.h:23 | What does the self-test tree prove? |
| K-063 | mib self-counters + proc-table pointers (minix subtree wiring) | M | S | 15★, 04 | minix.c tables | How does the test tree point at live data? |

### Group H — proc family + libc + tests + close (16–22, 99)

| ID | Name | Type | Src | Stock locations | Anchor | Reader benefit |
|----|------|------|-----|-----------------|--------|----------------|
| K-064 | Snapshot pull discipline (tick throttle + failure latch + magic scan) | M | S | 16★ | proc.c:47–142 | Why is proc data at most one tick old? |
| K-065 | PID hash + get_mslot + ticks_to_timeval + wmesg 6-class encoding | M | S | 16★ | proc.c:143–216 | How are rows indexed and times worded? |
| K-066 | LWP stat machine + three fills + whole-table (-1) idiom | M | S | 17★ | proc.c:225–595 | How is "state + where asleep" answered? |
| K-067 | PROC2 second layout + 8-way filter + compute-once-copy (17→18) | M | S | 18★ | proc.c:601–917 | Why two formats for one dataset? |
| K-068 | PROC_ARGS page walk + copy budget (adversarial truncation) | M | S | 19★ | proc.c:919–1176 | How do you read another process's argv safely? |
| K-069 | MINIX list/data + negative-PID=kernel-task rule (≠ LWP -1) | M | S | 20★ | proc.c:1178–1288 | What does ProcFS consume? |
| K-070 | libc CTL_USER local subtree + __sysctl forward + shared length idiom | P | S | 21★ | sysctl.c:75–90; __sysctl.c | Why do constants never take an IPC? |
| K-071 | Producer/consumer/test map (ProcFS/IPC/LWIP/UDS + ps/top/sysctl + test87/rmibtest) | E | S | 20/12/21★, plan §5.4 | tree.c/pid.c (ProcFS, via plan); test87.c 3662 lines | Who consumes every byte MIB emits? |
| K-072 | Endpoint/number tables (MIB_PROC_NR=7, MIB_BASE, E-grant fix fordern) | D | S | 99★ | com.h:66,1022; Endpoint enum | Where is the single truth for numbers? |

### New knowledge (N) added by coverage audit (§3) — full rows, not pointers

| ID | Name | Type | Anchor (evidence) | Assigned to |
|----|------|------|-------------------|-------------|
| K-073 | Transport seam: datacopy/grant/getnuid/DS-label/sendrec verbs + SysTransport EIO honest stubs | P | Rust transport.rs (488 lines); io/relay.rs grant open/close; todo P1-4 | 23 (new) |
| K-074 | Execution walker: find→see→judge→act loop + all terminal states | M | Rust walker.rs (1151 lines); todo P1-2 | 23 (new) |
| K-075 | Arena: Slot/NodeId/ChildMap(BTreeMap) + reserve/place + budget (MibBudget 256KiB, EINVAL-on-exhaust) | D | Rust tree/arena.rs, heap.rs; todo P2-3/P2-4 | 23 (new) |
| K-076 | Server assembly: MibServer + MibIpc/MibKernel/MibServices + run_once + register-arm order | M | Rust server.rs (883 lines); todo P1-1 | 23 (new) |
| K-077 | Wire ABI layouts pinned (SysctlNode 96B / SysctlDesc 16B / KinfoLwp 128B / KinfoProc2 680B + offsets) | D | Rust sysctl_abi.rs; todo P1-3 | 02-pointer + 11/17/18/20 consume notes |
| K-078 | test87 behavior contract (3662 lines, type/perm/meta/remote/dynamic matrix) | E | minix/tests/test87.c (size verified) | 24 (new) |
| K-079 | rmibtest + t_sysctl contract faces (registration order, far-end refusals, base face) | E | rmibtest.c (verified); t_sysctl via plan (re-verify in B) | 24 (new) |
| K-080 | Restart-test + mount-policy test faces (RestartLossy clear, no-top-level-takeover EPERM) | E | main.c:415–431; tree.c:1572–1580 | 24 (new) |

**Pool statistics:** 80 entries (72 stock + 8 new). By type: C 5, M 35, D 15, P 9, I 8, A 0
(ARCH decisions stay in plan.md §4 / code comments — this stage documents behavior, not ARCH catalog),
T 0 (build trivia answered once in §3.6, not pooled), E 8.
By stock location: 01:5, 02:8, 03:7, 04:4, 05:3, 06:6, 07:3, 08:6, 09:4, 10:4, 11:4, 12:8, 13:3,
14:3, 15:3, 16:3, 17:2, 18:2, 19:2, 20:2, 21:2, 22:2, 00/99:4. **Every stock doc contributes;
no doc is empty.**

---

## 3. Coverage Audit

### 3.1 Topic universe (4 routes)

1. **C symbols:** ~80 functions (§0.5) + mib.h macros/structs + sysctl.h constants + error paths +
   per-arch branches (`hw.c:6–9` mach i386/evbarm — 2 lines, real arch fork).
2. **OS concepts:** event-loop server, name resolution, visibility vs writability, delegation/mount,
   snapshot isolation, adversarial copy budgets, exchange-ABI stability.
3. **Non-C artifacts:** §0.6 table (8 rows).
4. **Boundary contracts:** init's sysctl use (09-stage-init/12), ProcFS consumption, producer set,
   libc external contract (A-10), RMIB cross-stage halves (E-RMIBWIRE/E-MIBPROD).

### 3.2 Coverage-gap table → each row becomes K-073–K-080 or an explicit denial

| # | Topic in universe, in no doc | Verdict | Becomes |
|---|------------------------------|---------|---------|
| G-1 | Transport verbs + honest stubs (the production half of 06/07/12) | NEW chapter | K-073 → 23 |
| G-2 | Walker + arena + server assembly (the execution half of 04/08/10) | NEW chapter | K-074/075/076 → 23 |
| G-3 | Pinned ABI offsets (the bytes 11/17/18/20 emit) | POINTER updates + consume notes | K-077 → 02/11/17/18/20 |
| G-4 | test87 as a readable contract (not just "see plan §3.5") | NEW chapter | K-078 → 24 |
| G-5 | rmibtest/t_sysctl faces | NEW chapter (t_sysctl re-verify in B) | K-079 → 24 |
| G-6 | Restart + mount-policy test faces | NEW chapter | K-080 → 24 |
| G-7 | `hw.c:6–9` mach arch fork (i386 vs evbarm strings) | DENY as standalone; one paragraph in 14 (2 lines of C, no reader payoff beyond "build-time string") | — (reason recorded) |
| G-8 | `servers/mib/Makefile` beyond PROG=mib | DENY (non-semantic, WONTFIX per plan §5.5) | — |

### 3.3 Repeat table (same topic expanded in ≥2 docs → keep one telling point, rest become pointers)

| # | Topic | Occurrences | Keep in | Others become |
|---|-------|-------------|---------|---------------|
| R-1 | ENOMEM-with-length | 01/02/06/09/21 | 01 (entry-path telling) | one-line pointer + use-site note |
| R-2 | SYSCTL_VERSION gates | 02/08/11/12-mount | 02 (value) + call-site one-liners | no re-explanation |
| R-3 | Scratch buffer | 03/06/09 | 03 (shape) | "borrowed from 03" |
| R-4 | RestartLossy | 01/04/08 | 01 (lifecycle fact) | sequence-pointer only |
| R-5 | Counts/counters | 03/04/15 | 04 (init census) | consume-pointer in 15 |
| R-6 | PRIVATE semantics | 02/03/07/11 | 07 (gates) | value-pointer in 02/03, call note in 11 |
| R-7 | CTL_USER local | 02/21 | 21 (owner) | boundary one-liner in 02 |
| R-8 | Three-grant relay | 06/12 | 12 (mechanism) | direction verdict only in 06 |
| R-9 | ERESTART idiom | 10/12 | 10 (outcome contract) | mechanism only in 12 |
| R-10 | Collision echo | 08/11 | 08 (behavior) | byte layout only in 11 |
| R-11 | PID-hash/time/wmesg reuse in 17–20 | 16/17/18/19/20 | 16 (definitions) | "computed in 16" call notes |
| R-12 | Loadavg formula | 14/16-shared-source | 14 (math) | source-pointer in 16 |

### 3.4 Out-of-scope table (taught elsewhere or denied with reason)

| # | Topic taught in a MIB doc today | Correct home | Action |
|---|---------------------------------|--------------|--------|
| X-1 | Init boot internals (when MIB is cited as "init needs sysctl early") | 09-stage-init | 01 keeps one sentence + pointer |
| X-2 | Kernel/VFS/PM table row meanings (16 skirts them) | producer stages | 16 keeps "pull, don't interpret" boundary (already correct — preserve) |
| X-3 | ProcFS file layout (20 skirts it) | 15-stage-fs | keep boundary, add pointer |
| X-4 | LWIP/UDS/IPC subtree internals | producer stages | 12 keeps mount/forward face only |
| X-5 | libc internals beyond the contract (sorting, __cvt_node_out) | libc (external) | 21 keeps behavior contract only |
| X-6 | Grant/copy kernel internals | 01-stage-kernel | 23 keeps verb face + parity notes |

### 3.5 Late-found out-of-scope confirmations

- `t_sysctl` (`tests/kernel/t_sysctl.c` per plan §5.4): existence not re-verified; B phase must confirm
  path + 74-line claim before citing. Does not block the blueprint (G-5 carries the flag).
- ipc.h six-struct exact lines: cited via plan §5.2; B phase re-verifies (mode-83 discipline).

### 3.6 Non-C answers (fixed 10-item list — each answered "where taught" or "why not in stage")

- **Link/load:** no MIB link script exists (user-space server). Taught where needed: boot-image
  membership (K-001 → 00/01). No chapter.
- **Image/memory layout:** slot order (K-001 → 00). No chapter.
- **ASM/trap entry:** none MIB-specific; SEF receive is library behavior. One paragraph in 01.
- **Boot assembly:** T-01–T-08 → 01 (sequence) + 04 (tree part) + 12 (endpts part). Covered.
- **Build/toolchain:** `PROG= mib` one-liner in 01; rest WONTFIX (G-8).
- **Cross-module iface + wire:** core of 02 (numbers/structs) + 12 (COMMON_MIB_*) + 23 (grant verbs). Covered.
- **Error paths:** centralized in 24 (G-4/5/6) with use-site pointers; dialect values stay in 02. Covered.
- **Shutdown/exit:** no MIB shutdown path exists in C (only restart-loss). Taught in 01 (K-004). No chapter.
- **Concurrency/sync:** single-threaded loop, no locks; nested-sendrec blocking = C parity (todo L2).
  One section in 23 (with the parity rationale). Covered.
- **Test infra:** test87/rmibtest/t_sysctl → 24 (new). Covered.

---

## 4. New Directory

**Decision (independent judgment, costed in §8.4): KEEP all 24 numbers; ADD 2 chapters (23, 24).
Zero renumbers, zero merges, zero splits.** Rationale:

1. The existing order already satisfies the four hard standards *after* the contract-boundary fixes
   in §5 (order-difference table §4.4 records the three apparent forward refs and their
   compensation — all three dissolve into backward-safe contracts).
2. The historic lesson (01-stage-kernel/todo.md I-14: renumber churn cost > benefit) applies at full
   force here: 4 code-comment anchors + ~40 intra-stage refs + plan/todo cross-tables all use current
   numbers. A cascade renumber (e.g. splitting 14 or 16) buys no readability (those docs are 167
   lines each, single-sitting reads) and costs a stage-wide anchor migration.
3. "Rebuild, not patch" is honored at *content* level: every chapter is rewritten from the knowledge
   pool + C truth under a fresh contract (§5); old narrative structures are discarded; old docs are
   archived as units in B phase. Stable coordinates ≠ patch list — the spec below defines each
   chapter's mission, boundary, and acceptance from scratch.
4. Doc-count freedom is used where it pays: +2 chapters for the two real gaps (execution seam 23,
   test contracts 24). The 3000-line allowance is not needed: per-chapter budgets stay ≤260 lines.

### 4.1 Chapter master table (number, title, one-line mission, group)

| # | Title | Mission (reader's question answered) | Group |
|---|-------|--------------------------------------|-------|
| 00 | MIB overview: the questioning tree | What is MIB and where do I read what? | Frame |
| 01 | Server birth + main loop: three letters, two refusals, one reply rule | How is the server born, what does its loop do? | Frame |
| 02 | Wire contract: envelopes, exchange layouts, error dialect values | What is on the wire? | Frame |
| 03 | Node model: five roles, one union, four matrix cells | How do I read any node field? | Tree core |
| 04 | Static tree + init census: seven slots, four wires, one walk | How does the tree stand up at boot? | Tree core |
| 05 | Lookup: static O(1), dynamic O(n) early-stop | How is one id found? | Tree core |
| 06 | Copy IO verdicts: caps, exact-match, paged strings, relay directions | Which bytes move, which way? | Primitives |
| 07 | Auth: ask PM at most once, gate everywhere | Who may see, write, reshape? | Primitives |
| 08 | Dynamic nodes: cheapest-checks-first birth, guarded death | How are runtime nodes born and killed? | Dynamics |
| 09 | Data access: stage-then-commit, strings, verify, bool hygiene | How do leaf bytes change safely? | Dynamics |
| 10 | Dispatch: one component per round, three fates per level | How does a name become an answer? | Dynamics |
| 11 | Query + describe: snapshots and desc streams | How is the tree enumerated? | Dynamics |
| 12 | Remote subtrees: mount policy, forward, death cleanup | How is someone else's subtree hosted? | Remote |
| 13 | kern subtree: the OS introduces itself | What does CTL_KERN contain? | Subtrees |
| 14 | vm + hw subtrees: two small tables, three computed numbers | What do CTL_VM/HW contain? | Subtrees |
| 15 | minix subtree: self-test tree, self-counters, proc pointers | What is CTL_MINIX for? | Subtrees |
| 16 | Proc snapshot foundation: pull discipline, index, time, wmesg | How is others' table data obtained + indexed? | Proc |
| 17 | LWP: state machine, wchan, three fills | What state is each thread in? | Proc |
| 18 | PROC2: same data, second layout, eight filters | How is the second format filled + filtered? | Proc |
| 19 | PROC_ARGS: cross-address page walk with budget | How is argv/env read safely? | Proc |
| 20 | MINIX list/data: ProcFS's two servings, negative-PID rule | What does ProcFS consume? | Proc |
| 21 | libc client: local-first, then forward (external contract) | What does userland do before IPC? | Clients |
| 22 | rmib client: the server's lightweight twin + sparse nodes | How do services mount themselves? | Clients |
| 23 | **NEW** Execution seam: walker, arena, transport, assembly | How do verdicts become a running server? | Execution |
| 24 | **NEW** Test contracts: test87, rmibtest, restart + policy faces | How is "correct" checked? | Close |
| 99 | Global concepts: numbers, errors, cross-service map (lookup) | Where do I look up any constant? | Close |

### 4.2 Reading paths

- **Main line (runtime order):** 00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 →
  12 → 13 → 14 → 15 → 16 → 17 → 18 → 19 → 20 → 23 → 24. (21/22 read with 02/12 as pairs.)
- **Branch (skippable):** 14 (small tables), 15 (test tree) — skippable after 13 if only the core
  mechanism is wanted; 17–20 skippable as a block if proc-info consumers are not needed.
- **Pairs (read together):** 02+21 (wire both ends), 12+22 (mount both ends), 06+23§copy (verdict +
  execution), 10+23§walker (verdict + execution), 11+24§enum (mechanism + contract test).
- **Lookup only:** 99 (never read cover-to-cover).

### 4.3 Parallel-body rule (no fake linearity)

- The 10 kern computed handlers (K-058) share one frame (func-node contract) → 13 teaches the frame
  once + a per-handler difference table (inputs, fetch verb, output layout, exclusion note).
- The 5 proc chapters share the snapshot foundation → 16 teaches pull+index once; 17–20 carry only
  their fill/filter differences + "computed in 16" pointers (R-11).
- The 3 remote outcomes (K-046) are taught once in 10; 12 teaches only the producing mechanism.

### 4.4 Order-difference table (teaching order vs runtime truth)

| # | Runtime fact (anchor) | Teaching choice | Reason + back-pointer compensation |
|---|----------------------|-----------------|-------------------------------------|
| O-1 | Dispatch calls remote-call inline (tree.c:1396+); remote taught in 12, after 10 | 10 owns the 3-outcome *contract* (K-046); 12 owns the *mechanism* | Outcome vocabulary must exist before handlers; 12§1 opens with "the three fates promised in 10 are produced here" |
| O-2 | Create-collision echo needs copyout_node bytes (11) but create taught in 08 | 08 owns the *behavior* (EEXIST+echo+reslen); 11 owns the *bytes* | Behavior is needed for the lifecycle story; 08§echo ends with "byte layout in 11, not repeated" |
| O-3 | mib_init calls mib_remote_init (endpts zeroing) inside 04's sequence | 04 owns the *sequence fact*; 12 owns the *table shape* | Sequence completeness beats table detail at boot; 04 cites "32 zeroed slots, shape in 12" |

### 4.5 Length budgets (soft caps; readability control)

00:120 · 01:230 · 02:250 · 03:200 · 04:160 · 05:130 · 06:170 · 07:150 · 08:170 · 09:160 · 10:180 ·
11:160 · 12:210 · 13:190 · 14:170 · 15:170 · 16:170 · 17:220 · 18:170 · 19:160 · 20:150 ·
21:110 · 22:150 · 23:260 · 24:200 · 99:100. No chapter may exceed 300 lines without a split review.

---

## 5. Per-Chapter Contracts (mandatory; B phase writes prose from these — no further triage)

### 00 — MIB overview

- One-line mission: orient the reader in 5 minutes: what MIB is, the two journeys, where to read.
- Teaches: K-001, K-003(in one line), K-006(names only), K-057(names only), reading paths (§4.2).
- Does NOT teach: any mechanism (→01–24); any constant values (→99); ARCH catalog (→plan.md §4).
- Prereqs: none.
- Follow-ups: all chapters cite 00's map; 01 is the first substantive read.
- Ground truth: kernel/table.c:52–64; rs/table.c:25; main.c:415–492 (shape only); servers/mib/ file list.
- Knowledge rows: K-001/boot-slot · K-006/three-letters · K-057/producer-names · paths.
- Acceptance: a newcomer can state (a) why MIB exists, (b) the two journeys, (c) which chapter holds
  any given question — without opening another chapter.

### 01 — Server birth + main loop

- Mission: the server's life: birth sequence, loop, six decode gates, refusals, reply rule.
- Teaches: K-001, K-002, K-003, K-004, K-005, K-006, K-007, K-008, K-011, K-014(behavior), K-070(one line).
- Does NOT teach: field meanings (→02); tree contents (→03/04); copy/auth bodies (→06/07);
  dispatch interior (→10); producer internals (→their stages).
- Prereqs: 00.
- Follow-ups: 02 (fields), 04 (init body), 06/07 (primitives), 10 (dispatch).
- Ground truth: main.c:277–383 (six gates); :384–431 (init+startup); :433–492 (loop);
  kernel/table.c:52–64; rs/table.c:25; sysctl.c:75–90 (local-first one-liner).
- Rows: K-001..K-008, K-011, K-014.
- Acceptance: reader can walk T-01–T-16 + T-23–T-26 on demand; can state the restart-loss rule;
  can classify any incoming message into the 3+2+1 taxonomy (3 letters, 2 refusals, 1 reply rule).

### 02 — Wire contract

- Mission: every byte on the wire: numbers, six structs, exchange layouts (values+ABI pointers),
  version pin, error-dialect values.
- Teaches: K-009, K-010, K-012(values + K-077 pointer), K-013(values), K-014/K-015/K-016(names only,
  behaviors elsewhere), K-017, K-018(names), K-070-pointer.
- Does NOT teach: decode flow (→01); flag behavior (→03/07); handler semantics (→09/13–20);
  grant execution (→23); test faces (→24).
- Prereqs: 01.
- Follow-ups: all handler chapters (09/11–20), 21/22 (both ends), 23 (verbs), 24 (faces).
- Ground truth: com.h:613–622,1022–1030; ipc.h six structs (re-verify lines); sys/sys/sysctl.h:75–164,
  1382–1450 region; minix/sysctl.h (97 lines); sysctl_abi.rs (K-077 pin).
- Rows: K-009, K-010, K-012, K-013, K-017, K-018 + K-077 pointer.
- Acceptance: reader can encode/decode all six structs by hand; can state version-pin + all four
  version-gate sites; can list the error-dialect values with correct "not what it sounds like" notes.

### 03 — Node model

- Mission: read any node field correctly: union roles, matrix, dynode, counts, scratch.
- Teaches: K-019, K-020, K-021, K-022(shape), K-023, K-013-pointer, K-036-pointer(values only).
- Does NOT teach: macros/bodies (→04); lookup (→05); lifecycle (→08); copy/auth/mount (→06/07/12).
- Prereqs: 02.
- Follow-ups: 04, 05, 08, 10, 12.
- Ground truth: mib.h:33–50,72–74,167–250; tree.c:21–27 (IS_STATIC_ID); :22–23 (scratch).
  NOTE: existing 03 header anchors `mib.h:CTLFLAG_REMOTE (L110, tool-generated)` etc. are
  generator garbage — B phase re-anchors every claim to the lines above.
- Rows: K-019, K-020, K-021, K-022, K-023.
- Acceptance: given any (type, flags) pair, reader predicts which union arm is live; states
  OWNDATA/OWNDESC free responsibility; states why empty-slot ≠ miss (hands to 05).

### 04 — Static tree + init census

- Mission: how the tree stands up: seven slots, macro family, two-phase wiring, full walk, endpts reset.
- Teaches: K-026, K-027, K-022(census), K-004(sequence pointer), endpts-reset sequence fact (O-3).
- Does NOT teach: subtree table contents (→13/14/15); dynodes (→08); lookup use (→05); endpts shape (→12).
- Prereqs: 01, 03.
- Follow-ups: 05 (consumes csize/clen), 13/14/15 (fill the wires), 23 (arena build mirrors).
- Ground truth: main.c:36–64,384–413; tree.c:1476–1536; kern.c:504; vm.c:150–154; hw.c:136–140;
  minix.c:85; remote.c:41 (call only).
- Rows: K-026, K-027, K-022.
- Acceptance: reader explains the two-phase wiring as a C limitation (and why Rust drops it);
  states the three init phases in order; counts what the census counts.

### 05 — Lookup

- Mission: find one id: static O(1), dynamic ordered O(n) early-stop, insert-point bonus.
- Teaches: K-024, K-025, K-039-pointer (id space only).
- Does NOT teach: multi-level walk (→10); insert/delete bodies (→08); table contents (→04/13–15).
- Prereqs: 03, 04.
- Follow-ups: 08 (insert point), 10 (per-level caller), 23 (production consumer).
- Ground truth: tree.c:21–27,34–88.
- Rows: K-024, K-025.
- Acceptance: reader traces all three check stages (negative→static→dynamic) including the
  empty-slot-continues rule; states the early-stop condition and what scan reuses.

### 06 — Copy IO verdicts

- Mission: which bytes move: caps, exact-match, paged strings, relay directions; verdict/execution split.
- Teaches: K-029, K-030, K-031, K-032, K-033, K-034(verdict face; execution →23).
- Does NOT teach: transport execution (→23); readwrite orchestration (→09); remote journey (→12);
  auth (→07).
- Prereqs: 01, 02.
- Follow-ups: 09 (consumer), 12 (relay consumer), 23 (execution).
- Ground truth: main.c:64–258 region; tree.c:361–421 (copyin_str); safecopies.h:64–65.
  NOTE: existing 06 header anchors (`main.c:mib_node (L64, tool-generated)`) are generator garbage —
  re-anchor to the function ranges above.
- Rows: K-029–K-034.
- Acceptance: reader states the two core constraints (old-len-is-cap; new-len-exact) and derives
  every function's behavior from them; draws the grant-direction diagram for 12 to consume.

### 07 — Auth

- Mission: one cached superuser answer, three gate kinds, nine call sites catalogued.
- Teaches: K-035, K-036, nine-site catalog (tree.c:156–174,565–570 region + 7 more, re-verify in B).
- Does NOT teach: PM round-trip execution (→23); per-handler call context (→08/09/10/11 — one-line each).
- Prereqs: 02.
- Follow-ups: 08, 09, 10, 11.
- Ground truth: main.c:259–275; const.h SUPER_USER; tree.c gate sites.
- Rows: K-035, K-036.
- Acceptance: reader explains the Unknown→Yes/No latch as a structure (why double-ask is
  unrepresentable); classifies any gate into visibility/write/structure.

### 08 — Dynamic nodes

- Mission: birth with cheapest-checks-first, death with guards; versions; ownership.
- Teaches: K-037, K-038, K-039, K-040, K-041, K-015(behavior), K-013(create gate), 11-pointer (O-2).
- Does NOT teach: pointer surgery execution (→23 arena); copyin bytes (→06); echo bytes (→11).
- Prereqs: 03, 05, 06, 07.
- Follow-ups: 09 (uses live nodes), 11 (echo/iteration), 23 (arena executes).
- Ground truth: tree.c:242–360 (check+scan); :426–481 (upgrade+add); :483–777 (create); :784–917
  (remove+destroy); sysctl.h:78 (CREATE_BASE).
- Rows: K-037–K-041, K-015.
- Acceptance: reader orders the create chain by cost and justifies "everything checkable before
  malloc"; states id-allocation rule; traces upgrade cascade; states destroy's guard list.

### 09 — Data access

- Mission: safe leaf mutation: lane select, stage-then-commit, strings, verify, bool hygiene.
- Teaches: K-042, K-043, K-044.
- Does NOT teach: func nodes (→13–20); dispatch arrival (→10); copy execution (→06/23).
- Prereqs: 03, 06, 07.
- Follow-ups: 13–20 (all reuse readwrite), 23 (execution).
- Ground truth: tree.c:1098–1330.
- Rows: K-042, K-043, K-044.
- Acceptance: reader states the no-in-place-update prohibition and derives the staging flow;
  recites the string invariant + the single full-slot exception; lists verify + bool + EPERM-big-write.

### 10 — Dispatch

- Mission: the name-to-answer wheel: consume-one-per-round, meta switch, three node kinds,
  ERESTART continuation; carries the sysctl journey map.
- Teaches: K-018(body), K-045, K-046 (outcome contract — owner), K-016(behavior), journey map (plan §1.3).
- Does NOT teach: find/readwrite/query/describe/remote bodies (→05/09/11/12 — switch targets only).
- Prereqs: 05, 07, 09 (+10-owns-46 for 12).
- Follow-ups: 11 (meta bodies), 12 (remote fate producer), 13–20 (func endpoints), 23 (walker executes).
- Ground truth: tree.c:1332–1475.
  NOTE: existing 10 header anchor (`tree.c:mib_readwrite (L1332, tool-generated)`) names the wrong
  function — re-anchor to `mib_dispatch`.
- Rows: K-018, K-045, K-046, K-016.
- Acceptance: reader walks any name (incl. negative-meta + ENOTDIR/EISDIR + write-gate + remote
  fates) to its terminal; draws the journey map from memory.

### 11 — Query + describe

- Mission: enumeration: two serializations, visibility filtering, write-once desc path.
- Teaches: K-047, K-048, K-049, K-077 consume note (offsets pinned, not hand-packed).
- Does NOT teach: dispatch arrival (→10); desc ownership (→08); copyin_str (→06).
- Prereqs: 02, 07, 10.
- Follow-ups: 21 (sysctlgetmibinfo peer), 24 (enum faces).
- Ground truth: tree.c:90–241 (copyout_node+query); :918–1096 (copyout_desc+describe); sysctl_abi.rs.
- Rows: K-047, K-048, K-049 + K-077.
- Acceptance: reader states static-then-dynamic-unsorted + userland-sorts; the two PRIVATE
  filterings (shape-kept-zeroed vs length-zero); the desc alignment walk; the version-carry idiom.

### 12 — Remote subtrees

- Mission: hosting others' subtrees: endpts, one-way letters, mount policy, forward, death.
  Carries the remote journey map.
- Teaches: K-050, K-051, K-052, K-053, K-054, K-055, K-057, K-046-producer face.
- Does NOT teach: outcome contract (→10 owner); client letter writing (→22); producer internals.
- Prereqs: 02, 03, 06, 10 (fates vocabulary).
- Follow-ups: 22 (peer), 23 (grant execution), 24 (far-end faces).
- Ground truth: remote.c:40–477 (all 9); tree.c:1543–1842 (mount/unmount).
  NOTE: existing 12 header anchor (`tree.c:mib_tree_init (L1543, tool-generated)`) names the wrong
  function — re-anchor to `mib_mount`.
- Rows: K-050–K-055, K-057.
- Acceptance: reader states the mount policy (≥2, flag discipline, temp-vs-obscure, no-top-takeover
  + TODO); the one-way rationale; the grant open/reverse-revoke order; down-vs-ERESTART outcomes.

### 13 — kern subtree

- Mission: first full func subtree: frame (table + verdict list + fetch verbs) taught once; 10 computed
  + 2 verify + 45 data + 39 excluded + ipc placeholder in a difference table.
- Teaches: K-058, K-059 (owner of the exclusion-contract statement), K-028(kern row).
- Does NOT teach: proc queries (→17/18/19 — name registrations only); fetch execution (→23);
  other subtrees (→14/15).
- Prereqs: 03, 06, 07, 09, 10, 12 (placeholder-overlaid face).
- Follow-ups: 14, 15 (same frame, second/third use), 23 (fetch verbs).
- Ground truth: kern.c (all 508 lines).
- Rows: K-058, K-059, K-028.
- Acceptance: reader fills any kern slot's row (kind, inputs, verb, layout, exclusion-or-value);
  explains the placeholder-overlay (mock invisible when IPC mounts); recites A-9 as contract.

### 14 — vm + hw subtrees

- Mission: second use of the frame: two small tables + three computed numbers, honestly fenced.
- Teaches: K-060, K-061, K-059(vm/hw rows), K-028(vm/hw rows), K-062? no.
- Does NOT teach: fetch execution (→23, shared with 16's sources); kern/minix (→13/15).
- Prereqs: 09, 13 (frame).
- Follow-ups: 15, 16 (shared vm_info sources pointer).
- Ground truth: vm.c (all 154); hw.c (all 140, incl. :6–9 mach fork — covered as G-7 one-paragraph,
  not a section).
- Rows: K-060, K-061, K-059, K-028.
- Acceptance: reader recomputes the loadavg example end-to-end; states page-shift + truncation rules;
  lists live-vs-baked hw rows + all six hw exclusions.

### 15 — minix subtree

- Mission: densest small tree: 12-node self-test assertions, self-counters, proc pointers; LWIP absence
  explained as runtime-mount (not missing).
- Teaches: K-062, K-063, K-022-consume, MINIX_TEST_SUBTREE gate.
- Does NOT teach: test87 runner (→24); proc bodies (→16–20); counter execution (→23).
- Prereqs: 02, 03, 08.
- Follow-ups: 16–20 (pointed nodes), 24 (test faces).
- Ground truth: minix.c (all 89); mib.h:23.
- Rows: K-062, K-063.
- Acceptance: reader maps each of the 12 test nodes to its assertion (incl. alignment self-test);
  explains why old-field edits break the suite; states why net trees are absent by design.

### 16 — Proc snapshot foundation

- Mission: obtain + index others' tables: pull discipline, three tables, PID hash, time, wmesg.
  (Four helpers stay together: they are consumed as one unit by 17–20; 167-line budget holds.)
- Teaches: K-064, K-065, K-071-source face, "pull don't interpret" boundary (X-2).
- Does NOT teach: row meanings (→producers); four output formats (→17–20); copyout (→06).
- Prereqs: 02, 06, 10.
- Follow-ups: 17, 18, 19, 20, 23 (pull verbs).
- Ground truth: proc.c:1–40 (tables+consts); :46–142 (update_tables); :143–216 (helpers).
- Rows: K-064, K-065.
- Acceptance: reader states throttle + latch + magic-scan + hash-build in order; justifies the
  hundreds-of-KB cost argument; encodes any wchan class.

### 17 — LWP

- Mission: state + wchan + numbers in kinfo_lwp bytes (first format).
- Teaches: K-066 + K-077 lwp offsets.
- Does NOT teach: tables (→16); copyout (→06); other formats (→18/19/20); layout pinning (→02-pointer).
- Prereqs: 16 (+06/10 assumed).
- Follow-ups: 18 (copies 17's outputs), 24 (lwp faces).
- Ground truth: proc.c:224–595.
  NOTE: existing 17 header anchor (`fill_wmesg (L224, tool-generated)`) mismatches the function at
  that line — re-anchor to `get_lwp_stat` + fills + `mib_kern_lwp`.
- Rows: K-066 + K-077.
- Acceptance: reader runs the stat machine (incl. all four direct-return non-sleep states + SLEEP
  wchan classes); states the pid/elsz/elmax query idiom incl. -1; fills kinfo_lwp field-by-field.

### 18 — PROC2

- Mission: second layout: compute-once-copy, eight filters, param discipline.
- Teaches: K-067 + K-077 proc2 offsets.
- Does NOT teach: state math (→17); tables/time (→16); mem source (→23 verb).
- Prereqs: 16, 17.
- Follow-ups: 19, 20, 24.
- Ground truth: proc.c:596–917.
  NOTE: existing 18 header anchor (`mib_kern_lwp (L600, tool-generated)`) names the previous
  chapter's entry — re-anchor to `fill_proc2_*` + `mib_kern_proc2`.
- Rows: K-067 + K-077.
- Acceptance: reader lists both shared-field (copied, 17 authoritative) and fresh-field sets;
  runs all eight filters incl. SESSION-no-job-control + TTY_REVOKE TODOs; states sparse-array idiom.

### 19 — PROC_ARGS

- Mission: cross-address argv/env: three-step walk, copy budget, truncation honesty.
- Teaches: K-068.
- Does NOT teach: state/proc2 (→17/18); snapshot (→20); copyout (→06).
- Prereqs: 16 (+06/10).
- Follow-ups: 20, 24.
- Ground truth: proc.c:918–1176.
  NOTE: existing 19 header anchor (`mib_kern_proc2 (L918, tool-generated)`) names the previous
  chapter's entry — re-anchor to `mib_kern_proc_args`.
- Rows: K-068.
- Acceptance: reader derives the budget formula and the adversarial arithmetic behind it; walks
  count-vs-content queries; states estimate-call short-circuit + truncation-without-error rule.

### 20 — MINIX list/data

- Mission: ProcFS's two servings: full-table sweep, single-PID fetch, negative-PID rule.
- Teaches: K-069 + K-071 ProcFS face + A-5 layout pointer.
- Does NOT teach: state math (→17); other formats (→18/19); ProcFS file layout (→FS stage, X-3).
- Prereqs: 16, 17, 02.
- Follow-ups: 24 (faces), FS stage (consumer).
- Ground truth: proc.c:1177–1288; minix/sysctl.h PROC_LIST/DATA (re-verify lines).
  NOTE: existing 20 header anchor (`mib_kern_proc_args (L1177, tool-generated)`) names the previous
  chapter's entry — re-anchor to `mib_minix_proc_list/data`.
- Rows: K-069.
- Acceptance: reader contrasts negative-PID (kernel task) vs LWP -1 (whole table) from memory;
  fills both structs; states estimate-call + zombie-flag + pseudo-row-skip rules.

### 21 — libc client (external contract, not implemented)

- Mission: userland's first hop: CTL_USER local, __sysctl forward, shared length idiom. A-10 fence.
- Teaches: K-070, K-047-pointer (sorting peer), K-014-consumer face.
- Does NOT teach: server interior (→01–20); rmib (→22); any libc rewrite (denied, A-10).
- Prereqs: 01, 02.
- Follow-ups: 22.
- Ground truth: lib/libc/gen/sysctl.c:61–90+ (local branch read); sysctlbyname/nametomib/getmibinfo
  (existence + roles); minix/lib/libc/sys/__sysctl.c (forward).
- Rows: K-070.
- Acceptance: reader states the first-component分流 rule and its cost rationale; the shared
  length idiom on both sides; what "external contract" forbids changing.

### 22 — rmib client

- Mission: the mount side: register/deregister/reregister/process/call, sparse nodes, server twin.
- Teaches: K-056, K-051-peer face, K-054-peer face.
- Does NOT teach: server-side mount walk (→12); grant/copy execution (→23); libc side (→21).
- Prereqs: 02, 06, 12.
- Follow-ups: 24 (registration-order faces), edge E-RMIBWIRE (protocol halves).
- Ground truth: minix/lib/libsys/rmib.c (1089); minix/include/minix/rmib.h (188);
  minix-sys rmib.rs (bookkeeping half only — fence).
- Rows: K-056.
- Acceptance: reader writes the register→mount→forward→death sequence from the client side;
  states sparse-node's three construction rules and who enforces them; fences bookkeeping vs protocol.

### 23 — NEW Execution seam (walker, arena, transport, assembly)

- Mission: how verdicts become a running server: the four execution halves + their seams.
- Teaches: K-073, K-074, K-075, K-076, K-034-execution, K-035-execution (ask), concurrency parity note.
- Does NOT teach: verdict logic (→06/07/08/10 — consumed, not repeated); kernel verb internals
  (→01-stage-kernel, X-6); producer data paths (→edge E-MIBPROD).
- Prereqs: 06, 07, 08, 10, 12, 16 (all verdict halves).
- Follow-ups: 24 (execution faces).
- Ground truth: Rust os/servers/mib/src — server.rs (assembly+run_once+register arm),
  walker.rs (dispatch loop), tree/arena.rs + heap.rs (arena+budget), transport.rs (MibKernel/
  MibServices + SysTransport EIO stubs), io/relay.rs (grant open/close), io/copy.rs (Oldp/Newp),
  auth.rs ask; C counterparts main.c:64–275, tree.c:1333+, remote.c:341–446, proc.c:47–142.
- Rows: K-073, K-074, K-075, K-076.
- Acceptance: (a) draws verdict/execution cut for all four halves; (b) states ChildMap choice +
  reserve/place + budget→EINVAL mapping; (c) states grant open/reverse-revoke order; (d) states
  the nested-sendrec blocking parity + no-timeout decision; (e) register-arm execution order
  (gate→label→slot→mount→chain) from memory.

### 24 — NEW Test contracts

- Mission: what "correct" means executably: test87 matrix, rmibtest faces, restart + policy faces.
- Teaches: K-078, K-079, K-080, K-047-face, K-056-face (registration order).
- Does NOT teach: test runners/build (→repo test docs); server/producer fixes (→their stages/edges).
- Prereqs: 08, 10, 11, 12, 15, 22, 23.
- Follow-ups: 99 (error table peer); edge E5(g)联调 face.
- Ground truth: minix/tests/test87.c (3662); minix/tests/rmibtest/rmibtest.c (267);
  main.c:415–431 (restart face); tree.c:1572–1610 (policy face); t_sysctl (re-verify path in B).
- Rows: K-078, K-079, K-080.
- Acceptance: reader maps each test group to its MIB mechanism + expected terminal;
  states the restart-face (dynamic gone, static alive) and policy-face (top-takeover EPERM) asserts.

### 99 — Global concepts (lookup)

- Mission: single lookup for numbers, errors, cross-service map. Values pinned to minix-types tests.
- Teaches: K-072, K-009-pointer, K-017-pointer, error summary (peer of 02/24), producer map (K-071 names).
- Does NOT teach: any mechanism (→numbered chapters).
- Prereqs: used on demand from any chapter.
- Follow-ups: none (terminal).
- Ground truth: com.h:66,1022–1030; Endpoint enum; errno set (re-verify); producer/consumer sites (§0.7).
- Rows: K-072.
- Acceptance: any constant/error/producer question resolves in ≤2 jumps with a pinned value.

---

## 6. Change Table

Operation kinds: REWRITE (same number, new prose from pool) · NEW · RETIRE (old doc archived, no
successor number) · REDIRECT (pointer-only stub left at B-phase discretion — default: none).

| Op # | Kind | Old location | New location | Reason | Knowledge involved | Destination |
|------|------|--------------|--------------|--------|--------------------|-------------|
| C-01 | REWRITE | 00 (108 lines) | 00 (budget 120) | Nav must cover 23/24 + pair paths; current 00 predates them | K-001,K-006,K-057 + §4.2 | same-doc rewrite |
| C-02 | REWRITE | 01 (229) | 01 (230) | Fix scope creep (decode gates stay, bodies point out); add restart-face pointer to 24 | K-001–K-008,K-011,K-014 | same-doc rewrite |
| C-03 | REWRITE | 02 (247) | 02 (250) | Add K-077 ABI-pin pointers; fix ipc.h line drift (mode-83) | K-009,K-010,K-012,K-013,K-017,K-018 + K-077 | same-doc rewrite |
| C-04 | REWRITE | 03 (196) | 03 (200) | Purge generator-garbage anchors; keep union/matrix/dynode/counts/scratch | K-019–K-023 | same-doc rewrite |
| C-05 | REWRITE | 04 (153) | 04 (160) | Unify arena-promise pointers (Promised-Landing Drift fix → single truth in 23) | K-026,K-027,K-022 | same-doc rewrite |
| C-06 | REWRITE | 05 (122) | 05 (130) | Zero-change except anchor re-verify; insert-point contract sharpened for 08/23 | K-024,K-025 | same-doc rewrite |
| C-07 | REWRITE | 06 (163) | 06 (170) | Draw verdict/execution cut explicitly; grant directions feed 12/23 | K-029–K-034 | same-doc + cut markers |
| C-08 | REWRITE | 07 (149) | 07 (150) | Nine-site catalog completed (re-verify 7 unconfirmed sites); ask→23 pointer | K-035,K-036 | same-doc rewrite |
| C-09 | REWRITE | 08 (165) | 08 (170) | O-2 compensation line; arena-promise unified to 23 | K-037–K-041,K-015 | same-doc rewrite |
| C-10 | REWRITE | 09 (154) | 09 (160) | String invariant + EPERM-big-write made prominent (audit M-7 class) | K-042,K-043,K-044 | same-doc rewrite |
| C-11 | REWRITE | 10 (174) | 10 (180) | Own K-046 contract (O-1); fix header anchor (dispatch, not readwrite); journey map redrawn | K-018,K-045,K-046,K-016 | same-doc rewrite |
| C-12 | REWRITE | 11 (157) | 11 (160) | Add K-077 consume note (offsets, not hand-pack); P1-3 pointer already present — keep | K-047,K-048,K-049 + K-077 | same-doc rewrite |
| C-13 | REWRITE | 12 (206) | 12 (210) | Fix header anchor (mount, not tree_init); open with O-1 back-pointer; journey map kept | K-050–K-055,K-057 | same-doc rewrite |
| C-14 | REWRITE | 13 (185) | 13 (190) | Difference-table format; exclusion contract prominent; arena-promise unified | K-058,K-059,K-028 | same-doc rewrite |
| C-15 | REWRITE | 14 (167) | 14 (170) | G-7 one-paragraph (mach fork); shared-source pointer with 16 | K-060,K-061,K-059,K-028 | same-doc rewrite |
| C-16 | REWRITE | 15 (168) | 15 (170) | Alignment self-test prominent; LWIP-absence-by-design fenced | K-062,K-063 | same-doc rewrite |
| C-17 | REWRITE | 16 (167) | 16 (170) | Four helpers fenced as one foundation unit (no split — §4 rationale); X-2 boundary kept | K-064,K-065 | same-doc rewrite |
| C-18 | REWRITE | 17 (220) | 17 (220) | Fix header anchor; add K-077 consume note (already has P1-3 note — keep + sharpen) | K-066 + K-077 | same-doc rewrite |
| C-19 | REWRITE | 18 (165) | 18 (170) | Fix header anchor (proc2, not lwp); compute-once-copy made structural | K-067 + K-077 | same-doc rewrite |
| C-20 | REWRITE | 19 (152) | 19 (160) | Fix header anchor (proc_args, not proc2); budget arithmetic worked-example | K-068 | same-doc rewrite |
| C-21 | REWRITE | 20 (141) | 20 (150) | Fix header anchor (list/data, not proc_args); negative-PID contrast boxed | K-069 | same-doc rewrite |
| C-22 | REWRITE | 21 (100) | 21 (110) | A-10 fence kept; length-idiom both-sides table | K-070 | same-doc rewrite |
| C-23 | REWRITE | 22 (140) | 22 (150) | Bookkeeping-vs-protocol fence (E-RMIBWIRE); sparse rules boxed | K-056 | same-doc rewrite |
| C-24 | NEW | — (G-1/G-2 raw: todo P1 halves, Rust server.rs/walker.rs/arena.rs/transport.rs) | 23 (budget 260) | Execution halves have no home; verdict docs must not absorb them (single-semantic) | K-073–K-076 | new doc from Rust + C counterparts |
| C-25 | NEW | — (G-4/5/6 raw: test87, rmibtest, restart/policy faces) | 24 (budget 200) | "Correct" has no readable home (plan §3.5 is a pointer, not prose) | K-078–K-080 | new doc from test sources |
| C-26 | REWRITE | 99 (82) | 99 (100) | Value tables re-pinned to minix-types tests; producer map added | K-072 | same-doc rewrite |
| C-27 | RETIRE | draft/README.md (placeholder) | archive (no successor) | Superseded since 2026-09-04 wave; keep as build artifact only | — | archive, remove from nav |

**No merges, no splits, no renumbers.** The rebuild is content-level for 24 chapters + 2 new builds.

---

## 7. Gap Chapters (fixed list — none left TBD)

Every §3.2 gap is resolved here (no "to be determined"):

| Gap | Resolution | Raw material | Home | Acceptance |
|-----|-----------|--------------|------|------------|
| G-1 transport verbs | NEW 23 §verbs | Rust transport.rs + io/relay.rs + C main.c:64–275/remote.c:379–446 | 23 | §5-23 (d) |
| G-2 walker/arena/assembly | NEW 23 §§walker/arena/assembly | Rust walker.rs/arena.rs/heap.rs/server.rs + C tree.c:1333+/proc.c:47–142 | 23 | §5-23 (a–c,e) |
| G-3 ABI offsets | POINTERS, no chapter | Rust sysctl_abi.rs | 02/11/17/18/20 | §5 each |
| G-4 test87 | NEW 24 §test87 | minix/tests/test87.c | 24 | §5-24 |
| G-5 rmibtest/t_sysctl | NEW 24 §far-faces (+B-phase re-verify flag for t_sysctl path) | rmibtest.c; plan §5.4 citation | 24 | §5-24 |
| G-6 restart/policy faces | NEW 24 §lifecycle-faces | main.c:415–431; tree.c:1572–1610 | 24 | §5-24 |
| G-7 mach fork | one paragraph, no chapter | hw.c:6–9 | 14 | §5-14 |
| G-8 Makefile | denied (WONTFIX) | servers/mib/Makefile:11 | 01 one-liner | — |

Non-C list: all 10 items answered in §3.6 — none outstanding.

---

## 8. Anchor Migration + Break Cost

### 8.1 Section migration (every changed old doc → new home, per section)

Because numbers are stable, most rows are "same doc, rewritten"; moved knowledge is listed explicitly.
(Old § numbers below are the current docs' top-level sections, sampled 2026-09-20.)

| Old location | Old content (one line) | New location | Move type | Break risk |
|--------------|------------------------|--------------|-----------|------------|
| 00 §§1–8 | nav + boot chart + journeys + principles | 00 §§1–7 (same) | rewrite | low (nav only) |
| 01 §§1–6 + tests | loop + gates + ENOMEM + tests | 01 (same) + test detail →24 | rewrite + extract | low |
| 02 §§1–5 + wire tables | numbers/structs/formats/errors | 02 (same) + offsets →K-077 pointers | rewrite | low |
| 03 §§1–6 | union/matrix/dynode/counts/scratch | 03 (same), anchors re-laid | rewrite | low |
| 04 §§1–5 + arena promise | slots/macros/walk + "arena at 13" promise | 04 (same) + promise →23 (single truth) | rewrite + redirect | medium — 4 promise sites (04:82/:121, 08:82, 13:112) unified; grep-verify in B |
| 08 §§2–4 + echo | lifecycle + collision echo | 08 (same; echo behavior) + echo bytes stay 11 | rewrite (O-2 line added) | low |
| 10 §§1–5 + journey map | wheel + meta + fates + map | 10 (same; K-046 owned) | rewrite | low |
| 12 §§1–5 + journey map | endpts/mount/forward/death + map | 12 (same; O-1 back-pointer added) | rewrite | low |
| 13 §§1–5 + table | kern handlers + exclusions | 13 (same; difference-table format) | rewrite | low |
| 14 §§1–4 | vm + hw in one doc | 14 (same; kept merged — no split) | rewrite | none |
| 16 §§1–4 | pull + hash + time + wmesg | 16 (same; fenced as foundation unit) | rewrite | none |
| 17–20 §§1–5 each | fills + filters + walk + list/data | 17–20 (same; anchors fixed, K-077 notes) | rewrite | low |
| 06/07/12/16 transport remarks | scattered "execution later" notes | 23 (single home) | extract + redirect | medium — ~7 follow-up markers; grep list in B |
| plan §3.5 test pointers | test87/rmibtest/t_sysctl as pointers | 24 (readable prose) | extract | low |
| draft/README.md | placeholder | archive | retire | none (already de-navved) |

### 8.2 Reference migration (all old-number/file citations → new targets)

| Old reference | New target | Verify by |
|---------------|-----------|-----------|
| Intra-stage `NN-mib-*.md` links (~40; every doc's tail list + inline) | unchanged (numbers stable) — only tail lists gain 23/24 rows | `rg -n "mib-.*\.md" 1*.md 2*.md 0*.md 9*.md` in B; count ≈ 40 + 2×24 added rows |
| `../edge_todo.md` pointers (E-RMIBWIRE/E-MIBPROD/E-MIBGRANT/E-DSWIRE/E-ISWIRE/E5(g)) | unchanged (edge file untouched) | no-op |
| `../01-stage-kernel/*`, `../02-stage-vm/*`, `../09-stage-init/12-*` links | unchanged | no-op |
| plan.md §5.3/§6.1 tables citing doc numbers | unchanged numbers; §6.1 gains 23/24 rows (B-phase plan touch-up, not this blueprint) | plan edit in B |
| todo.md §1–§5 anchors | unchanged | no-op |

### 8.3 Code-comment migration

| Comment | Points at | Action |
|---------|-----------|--------|
| message.rs:3149 (02) | 02 kept | none |
| sysctl_abi.rs:19 (02 + 17/18/20) | all kept | none |
| mib.rs:8 (02) | 02 kept | none |
| com.rs:241–267 (02 + 01) | both kept | none |

**Code-comment churn: zero.**

### 8.4 Break-cost summary

- Affected doc-internal refs: ~0 renames (stable numbers); ~48 tail-list rows added (23/24 in 24 docs).
- Affected code comments: 0.
- Affected plan/todo/edge text: plan §6.1 +2 rows; README table +2 rows (both B-phase, mechanical).
- Hot spots: 4 arena-promise sites (§8.1 C-05 row); ~7 transport follow-up markers; 6 generator-garbage
  header anchors (03/04/06/10/12/17/18/19/20 — §5 notes each).
- Recommended batch method (B phase): per-chapter rewrite commits (one chapter per commit) + a final
  `rg "mib-.*\.md" | wc -l` + `rg -n "2595|arena.*(13|15)" ` drift re-scan + anchor spot-check (mode-83:
  10 random anchors re-grepped).

---

## 9. Verification, Self-Check Gates, Conclusion

### 9.1 Four mechanical checks (per R-prompt §5.4)

1. **Forward-ref scan:** every §5 contract's "Prereqs" points only at earlier numbers:
   02→01 ✓; 03→02 ✓; 04→01,03 ✓; 05→03,04 ✓; 06→01,02 ✓; 07→02 ✓; 08→03,05,06,07 ✓;
   09→03,06,07 ✓; 10→05,07,09 ✓ (+12 listed as *peer-producer*, not prereq — O-1);
   11→02,07,10 ✓; 12→02,03,06,10 ✓; 13→03,06,07,09,10,12 ✓; 14→09,13 ✓; 15→02,03,08 ✓;
   16→02,06,10 ✓; 17→16 ✓; 18→16,17 ✓; 19→16 ✓; 20→16,17,02 ✓; 21→01,02 ✓; 22→02,06,12 ✓;
   23→verdict chapters (all earlier) ✓; 24→08,10,11,12,15,22,23 ✓; 99→none (lookup) ✓.
   **Result: zero forward refs — PASS by construction** (O-1/O-2/O-3 compensations recorded, none hidden).
2. **Dependency graph:** frame(00–02) → core(03–07) → dynamics(08–11) → remote(12) → subtrees(13–15) →
   proc(16–20) → clients(21–22) → execution(23) → close(24/99). No cycles (23 consumes verdicts, never
   vice versa; 24 consumes mechanisms, never vice versa). **PASS.**
3. **Coverage:** all 80 pool entries have a home (§2 Assigned/Stock columns); all §3.2 gaps resolved
   (§7); denials G-7/G-8 carry reasons. **100% — PASS.**
4. **Break cost:** counted in §8.4 (refs ≈ 48 added rows + 0 renames + 0 code churn). **PASS** (bounded,
   mechanical).

### 9.2 Self-check gates (all nine answered, none skipped)

| Gate | Check | Result |
|------|-------|--------|
| G1 | C true order checkable (10 random anchors) | PASS — T-01(kernel/table.c:52–64), T-05(main.c:415–431), T-11(main.c:294–296), T-13(main.c:309–318), T-16(tree.c:1333–1350), T-17(tree.c:1361–1382), T-19(main.c:259–275), T-29(tree.c:1572–1610), T-33(proc.c:47–142), T-37(proc.c:919–1176): all re-grepped or read 2026-09-20 except T-11/T-23/T-24/T-26 exact lines cited via plan (flagged for B re-verify — counted as pass-with-flag, not silent) |
| G2 | Pool complete: every C file + non-C item has a home or explicit denial | PASS — 9 C files in §0.5; 8 non-C rows in §0.6; denials G-7/G-8 with reasons |
| G3 | New directory zero forward refs (per-chapter prereq scan) | PASS — §9.1(1) |
| G4 | Dependency graph acyclic; cycles get a split plan | PASS — §9.1(2), no cycles found |
| G5 | 100% coverage: every pool row homed or deleted-with-reason; new rows have evidence anchors | PASS — §9.1(3); deletions: none (only 2 denials, both reasoned); new rows K-073–K-080 all anchored |
| G6 | Every split/merge has stock-destination lines; every NEW has source lines (10-spot check) | PASS — no splits/merges by design (§4); NEWs 23/24 source lists in C-24/C-25 + §7; 10-spot: all §6 NEW rows + 8 sampled REWRITE rows carry destinations |
| G7 | Every contract has 7 elements (mission/teaches/not-teaches/prereqs/follow-ups/ground-truth/rows/acceptance) | PASS — §5 all 26 contracts carry all 8 fields (superset of the 7 required) |
| G8 | Migration covers every changed doc's sections + doc/code refs | PASS — §8.1 (15 rows), §8.2 (ref classes), §8.3 (4 code anchors, zero churn) |
| G9 | Factual claims anchored (10 random checks; speculation labeled) | PASS — §9.1-G1 sample doubles as the 10; speculation/flagged items: VMINHIBIT exact lines (plan-cited), T-23–T-26 exact lines (plan-cited), ipc.h lines (plan-cited), t_sysctl existence (flagged), Rust 13629-line count (measured 2026-09-20, drift vs todo explained) |

### 9.3 Conclusion + questions for the user

**Conclusion:** the blueprint is complete and directly executable: B phase can rewrite chapters
00–24 + 99 one by one from §5 contracts pulling §2 pool rows, with §8 migration applied per commit.
No further triage decisions are needed except the three questions below.

**Open questions (need user ruling before B phase):**

- **Q1 (prose language):** this blueprint is English per the muse requirement; the stage's 24 docs,
  plan.md, and todo.md are Chinese. Should B-phase prose stay Chinese (stage convention), switch to
  English, or be bilingual (concepts English + narrative Chinese)? Recommendation: keep Chinese
  prose with English symbol/term anchors (current stage style) — cheapest for review continuity.
- **Q2 (23's size):** 23 bundles four execution halves (walker/arena/transport/assembly, budget 260).
  If B-phase drafting exceeds 300 lines, split into 23-walker/arena + 24-transport/assembly and shift
  test-contracts to 25 — or hold the line with tighter prose? Recommendation: hold the line; split
  only on measured overflow.
- **Q3 (t_sysctl):** plan §5.4 cites `tests/kernel/t_sysctl.c` (74 lines) which this blueprint did not
  re-verify. If B phase finds it missing/renamed, drop the G-5 t_sysctl face to a one-line pointer or
  keep the gap row open? Recommendation: degrade to pointer, keep 24 shippable.

---

## Appendix: stage-convention notes for B phase (not normative, reminders only)

- Single-threaded event-loop voice: verdict-first, execution-sealed, no locks invented.
- Error-dialect discipline: ENOMEM/EEXIST/ERESTART/EDONTREPLY are never "just errors" — each carries
  a length/channel/continuation idiom; 24 owns the test faces, 02 owns the values.
- Anchor discipline (mode-83): every C claim re-grepped at write time; the six generator-garbage
  headers (§5 notes) must not be copied forward.
- Doc-code sync: 23/24 prose lands together with the Rust halves they describe; plan §6.1 + README
  gain their 23/24 rows in the same commits.
- Length discipline: soft budgets in §4.5; any chapter crossing 300 lines triggers a split review,
  never silent growth.
