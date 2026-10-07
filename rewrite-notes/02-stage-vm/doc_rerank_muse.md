# 02-stage-vm Document Reconstruction Blueprint (muse)

## Execution header

```text
your_name(AI agent name) = muse
target_dir(concerned working directory) = rewrite-notes/02-stage-vm
repo_root(repository root) = /home/xzhao/github/minix-rs

Task = R-phase reconstruction blueprint: output target_dir/doc_rerank_muse.md, modify no body text.
       Beyond reading C code, also read: header declarations of all docs in this directory,
       the corresponding os/ entry points, the 00-*-overview.md navigation tables.
Constraint = must not reference .design/ or tmp_design_and_todo/; any landed artifact must carry
        the _muse suffix; must not read or copy other AIs' doc_rerank_* products.
```

This report is self-contained. It assumes no knowledge of the dialogue process.
All factual claims about C source, Rust code, or build artifacts carry anchors.
Judgments without anchors are explicitly marked "conjecture" or "to be verified".
Output language is English per muse special requirement.

Additional task notes honoured: (1) a single doc may carry a single concept even up to
~3000 lines when that concept is genuinely complex (soft line limit), but length is
otherwise controlled for readability; (2) the old document count is not a constraint,
it may grow or shrink; (3) this task exists to fix problems in existing docs, rebuild
is allowed when needed, old docs may contain errors or unreviewed content, so the
ground truth is minix3 C source plus Rust code, not the old prose.

---

## 0. Metadata

- Executor: muse. Date: 2026-09-19 (UTC). Target: `02-stage-vm`.
- Repo HEAD observed: `3a996d170` (`git log --oneline -3`: 3a996d170 docs(edge3);
  9e1bb978b feat(init); 338c7a301 refactor(init)).
- Deliverable: this file only:
  `rewrite-notes/02-stage-vm/doc_rerank_muse.md`.
  No body text was modified. No file was renamed, moved, or deleted.
- Other-AI products (`doc_rerank_deepseek.md`, `doc_rerank_glm.md`,
  `doc_rerank_qwen.md`, `doc_rerank_HY4.md`) were NOT read. Their byte sizes were
  visible in directory listings but their contents were never opened.

### 0.1 Scope

Counted as documents (formal, numbered, in rebuild scope):

| Doc | Lines (`wc -l`) |
|---|---|
| 00-vm-overview.md | 100 |
| 01-vm-init-main.md | 830 |
| 02-vmproc-struct.md | 548 |
| 03-vmproc-table.md | 444 |
| 04-acl.md | 609 |
| 05-physical-memory.md | 625 |
| 06-page-allocator.md | 554 |
| 07-pagetable-struct.md | 642 |
| 08-pagetable-ops.md | 1098 |
| 09-slab-allocator.md | 685 |
| 10-vm-relocation.md | 669 |
| 11-phys-pagestate.md | 513 |
| 12-memtype.md | 538 |
| 13-region-mapping.md | 596 |
| 14-region-lookup.md | 528 |
| 15-ipc-dispatch.md | 739 |
| 16-pagefault.md | 618 |
| 17-cow-mechanism.md | 476 |
| 18-vm-fork.md | 487 |
| 19-vm-brk.md | 568 |
| 20-vm-mmap.md | 470 |
| 21-vm-munmap.md | 689 |
| 22-vm-exit.md | 595 |
| 23-vfs-interaction.md | 531 |
| 24-page-cache.md | 518 |
| 25-rs-services.md | 691 |
| 26-vm-queries.md | 743 |
| 99-global-concepts.md | 74 |

Counted as reference material (not rebuilt, used as evidence):
`plan.md` (443 lines), `todo.md` (truncated read, V13 round + Fix #63-#81),
`checklist.md` (867 lines), `draft/` (32 files: 00-26 + 99 + TODO + 3 review
products), `edge_todo.md` (1095 lines).

Out of scope: `.design/`, `tmp_design_and_todo/` (never referenced);
other stages except `01-stage-kernel/00-kernel-overview.md` (previous-stage
boundary) and `00-master-plan/README.md` (stage partition + boot causal chain).

### 0.2 Reading list

1. All 28 formal docs above: header declarations read for all (classification,
   C/Rust anchors, prerequisites, non-coverage statements); bodies skimmed for
   knowledge extraction plus targeted deep reads where duplication or boundary
   disputes were suspected (05/06/07, 11/12/13, 20/21, 09/10, 15/01, 25/10).
2. C ground truth: `minix3/minix/servers/vm/` — 24 `.c` files (9260 lines total:
   `region.c` 1555, `pagetable.c` 1500, `main.c` 768, `mmap.c` 573,
   `alloc.c` 548, `slaballoc.c` 528, `utility.c` 494, `pagefaults.c` 418,
   `rs.c` 391, `cache.c` 332, `mem_cache.c` 324, `mem_file.c` 287,
   `mem_shared.c` 211, `fdref.c` 177, `pb.c` 168, `exit.c` 156,
   `mem_anon.c` 151, `vfs.c` 143, `mem_anon_contig.c` 132, `acl.c` 129,
   `fork.c` 116, `mem_directphys.c` 79, `break.c` 69, `regionavl.c` 11)
   plus headers (`vm.h`, `vmproc.h`, `glo.h`, `pt.h`, `region.h`,
   `phys_region.h`, `memtype.h`, `cache.h`, `proto.h`, `sanitycheck.h`,
   `cavl_if.h`/`cavl_impl.h`/`unavl.h`/`regionavl.h`/`regionavl_defs.h`,
   `memlist.h`, `util.h`, `fdref.h`, `arch/i386/pagetable.h`,
   `arch/earm/pagetable.h`) and non-C artifacts
   (`arch/earm/vm.lds`, `arch/*/Makefile.inc`, `Makefile`,
   `minix3/minix/include/minix/com.h`, `ipc.h`, `endpoint.h`, `rs.h`).
3. Rust entry points: `os/servers/vm/src/` (acl, alloc_page, alloc_stats, audit,
   boot, brk, cow_exec_pf, direct_map, dma, exit, fdref, fork, global,
   heap_arena, kernel_gateway, main, map_phys, memtype, mmap, munmap,
   page_cache, query, region/*, rs, sanity, vfs_queue, vm_server, vmproc/*,
   ipc/*, pagetable/*, phys_mem/*) plus `os/libs/minix-types/src/ipc/vm.rs`
   and `message.rs` for wire truth.
4. Boundary materials: `00-master-plan/README.md` (fork demoted to secondary
   line, boot-order stages), `edge_todo.md` (E1/E2/E-RSWIRE/E-VFSWIRE/E-VMTLB),
   `02-stage-vm/plan.md` §1-§8 (used as clue only, conclusions re-derived),
   `01-stage-kernel/00-kernel-overview.md` §§1.1-1.4 (kernel/VM split:
   kernel holds scheduling + IPC primitives, VM holds address-space semantics).
5. Style example: `01-stage-kernel/06-todo.md` header + table of contents
   (learned only its contract-writing form: what/why-not/hand-off/acceptance;
   none of its content is reused).

### 0.3 Commands and key outputs (evidence excerpts)

```text
$ ls rewrite-notes/02-stage-vm/*.md | xargs wc -l
-> 28 formal docs, 100-1098 lines each (table in §0.1); 08 longest (1098),
   01 second (830). No doc exceeds the 3000-line soft single-concept ceiling.

$ wc -l minix3/minix/servers/vm/*.c | sort -n
-> 9260 total; top: region.c 1555, pagetable.c 1500, main.c 768.

$ rg -n "^[a-z].*\(|^static.*\(" minix3/minix/servers/vm/main.c | head
-> is_first_time:79, main:93, sef_cb_lu_state_changed:196,
   sef_local_startup:219, sef_cb_init_fresh:241, init_proc:262,
   libexec_copy_physcopy:294, boot_alloc:305, libexec_alloc_vm_prealloc:317,
   libexec_alloc_vm_ondemand:324, exec_bootproc:331, do_procctl_notrans:419,
   init_vm:428, sef_cb_init_vm_multi_lu:592, sef_cb_init_lu_restart:677,
   sef_cb_signal_handler:731, map_service:755.

$ sed -n '428,520p' minix3/minix/servers/vm/main.c  (init_vm body)
-> sys_getkinfo -> get_mem_chunks -> memset(vmproc)+slot ids -> acl_init ->
   map_region_init -> mem_init -> init_proc(VM_PROC_NR)+pt_init ->
   __minix_init -> mem_add_total_pages (boot modules + kernel sizes) ->
   per-boot-proc init_proc+exec_bootproc+free_mem(blob).

$ sed -n '93,195p' minix3/minix/servers/vm/main.c  (main loop)
-> is_first_time?init_vm -> sef_local_startup -> while(TRUE):
   missing_spares>0?alloc_cycle -> sef_receive_status -> is_ipc_notify?continue
   -> vm_isokendpt?panic -> VFS-transid?do_procctl : RS_INIT?do_sef_init_request
   (SUSPEND) : VM_PAGEFAULT?do_pagefaults (no reply) : CALLMAP+acl_check+handler
   -> result!=SUSPEND?ipc_send.

$ sed -n '520,580p' minix3/minix/servers/vm/main.c (CALLMAP registration)
-> 26 registered calls (MMAP/MUNMAP/MAP_PHYS/UNMAP_PHYS/EXIT/FORK/BRK/WILLEXIT/
   PROCCTL/VFS_REPLY/VFS_MMAP/RS_SET_PRIV/RS_PREPARE/RS_UPDATE/RS_MEMCTL/
   REMAP/REMAP_RO/GETPHYS/SHM_UNMAP/GETREF/INFO/MAPCACHE/SETCACHE/
   FORGETCACHE/CLEARCACHE/GETRUSAGE). EXEC_NEWMEM/ADDDMA/DELDMA/GETDMA have
   call numbers but no handler (both sides answer ENOSYS).

$ rg -n "define VM_|NR_VM_CALLS|VM_BASIC" minix3/minix/include/minix/com.h
-> VM_RQ_BASE 0xC00 (:627); EXIT+0 (:630), FORK+1 (:632), BRK+2 (:636),
   EXEC_NEWMEM+3 (:637), WILLEXIT+5 (:643), MMAP+10 (:647), MUNMAP+17 (:649),
   ADDDMA+12 (:656), DELDMA+13 (:664), GETDMA+14 (:672), MAP_PHYS+15 (:677),
   UNMAP_PHYS+16 (:679), MAPCACHE+26 (:682), SETCACHE+27 (:685),
   FORGETCACHE+28 (:688), CLEARCACHE+29 (:691), VFS_REPLY+30 (:707),
   REMAP+33 (:716), SHM_UNMAP+34 (:718), GETPHYS+35 (:720), GETREF+36 (:722),
   RS_SET_PRIV+37 (:724), INFO+40 (:729), RS_UPDATE+41 (:736),
   RS_MEMCTL+42 (:738), REMAP_RO+44 (:749), PROCCTL+45 (:752),
   VFS_MMAP+46 (:762), GETRUSAGE+47 (:764), RS_PREPARE+48 (:766),
   NR_VM_CALLS 49 (:769), VM_PAGEFAULT BASE+0xff (:773).

$ ls minix3/minix/servers/vm/arch/{earm,i386}/; cat Makefile (VM)
-> arch/earm/{Makefile.inc,pagetable.h,vm.lds}; arch/i386/{Makefile.inc,
   pagetable.h}; top Makefile exists; no Makefile.inc at top level.

$ rg -c "02-stage-vm|01-vm|15-ipc" rewrite-notes/02-stage-vm/*.md
-> every formal doc carries 2-12 self cross-references; heaviest: 01 (12),
   20 (11), 06 (11), 21 (10), 13/14/15/04 (10 each). Dense mesh: moving one
   doc disturbs ~10 neighbours on average.

$ rg -n "covered in|see .*\.md|02-stage-vm" os/servers/vm/src (sample)
-> vm_server.rs, main.rs, exit.rs, boot.rs, region/mod.rs, phys_mem/*,
   pagetable/mod.rs reference 02-stage-vm docs/todo/plan by name
   (examples: plan.md §4 A-5, 05-physical-memory.md §3.3, todo.md G-V12-10,
   15-ipc-dispatch.md §3.7, 22-vm-exit.md §3 D5). Code-comment breakage is
   real and counted in §8.

$ git rev-parse --short HEAD -> 3a996d170.
```

---

## 1. C true order (runtime order reconstructed from source, not from prose)

### 1.1 Stage-type determination

02-stage-vm is a **service event-loop stage with a linear boot segment**.
Reason: `main.c:93-195` shows a one-shot `init_vm()` chain followed by an
infinite `while(TRUE)` receive-dispatch-reply loop; `main.c:536-575` shows a
static call table (not a syscall trap table, not a driver registry).
Therefore the blueprint follows the service pattern: why the service exists
-> birth and init -> message interface -> core data structures -> request
handling grouped by scenario -> queries and miscellany -> neighbour protocols.
A single request lifecycle is the main line inside each service group, not the
source-file alphabetical order. Startup-chain sections (§1.2) use call order;
loop sections (§1.3) use priority order.

### 1.2 Boot segment true-order table (init_vm + entry)

| # | Action | C anchor | Note |
|---|---|---|---|
| B-1 | Kernel loads VM ELF as ptproc; VM starts at `main` | `main.c:93`; `01-stage-kernel/09-vm-boot-protocol.md:27` (boundary: VM creates page tables for later services) | Stage entry causal link; VM is the first runnable user service (`00-master-plan/README.md` boot chain). |
| B-2 | First-time gate | `main.c:79-91 is_first_time` | Decides whether `init_vm` runs. |
| B-3 | Fetch boot parameters from kernel | `main.c:442 sys_getkinfo(&kernel_boot_info)` | Panics on failure; `kernel_boot_info` carries memmap, modules, boot procs. |
| B-4 | File-map default + env override | `main.c:447-448 enable_filemap=1; env_parse("filemap",...)` | Rust: `FILEMAP_ENABLED` AtomicBool (`mmap.rs:137`). |
| B-5 | Build available-memory chunk list | `utility.c:get_mem_chunks` (called `main.c:455`) | Click-rounding lives here, not in `main.c`. |
| B-6 | Zero process table, assign slot ids | `main.c:458-462 memset(vmproc,0); vm_slot=i` | Grounds 02/03. |
| B-7 | Init access-control bitmaps | `main.c:466 acl_init()` → `acl.c:21` | Grounds 04. |
| B-8 | Init region management | `main.c:469 map_region_init()` → `region.c:36` | Grounds 13 (empty maps per slot). |
| B-9 | Hand all physical memory to allocator | `main.c:471 mem_init(mem_chunks)` → `alloc.c:306` | Grounds 05. |
| B-10 | Create VM's own slot + page-table skeleton | `main.c:474-475 init_proc(VM_PROC_NR); pt_init()`; `init_proc main.c:262-285`; `pt_init pagetable.c:1088-1349` | Grounds 02/03 + 07/08. |
| B-11 | Acquire kernel IPC vectors available only after kernel mappings known | `main.c:478-481 __minix_init()` | Heap-availability divide (grounds 09: no heap before, heap after). |
| B-12 | Calibrate total pages for boot modules + kernel sizes | `main.c:483-496 mem_add_total_pages` loops; `alloc.c:281` | Grounds 01/05 boundary (call site vs definition). |
| B-13 | Instantiate every other boot process: init slot, load segments, free file blob | `main.c:498-520 init_proc + exec_bootproc + free_mem`; `exec_bootproc main.c:331-417`; `libexec_* main.c:294-330` | Grounds 01 (ELF loading) + 13 (regions) + 07/08 (mappings). |
| B-14 | Register CALLMAP table | `main.c:536-575` (26 entries) | Grounds 15 (runtime registration, not compile-time list). |
| B-15 | Mark VM instance, enter SEF startup, clear fresh flag, enter loop | `main.c:577-579 VMF_VM_INSTANCE`; `main.c:219 sef_local_startup`; `main.c:101-109` | Grounds 01 (SEF) + 15 (loop entry). |

Order-difference note O-1: at runtime, Direct Map machinery (Rust) / `pt_init`
spare-page replacement (C, `pagetable.c:1311-1345`) happens at B-10, but the
existing teaching order consumes Direct Map concepts already in 05/06 (which
numerically precede 07). Teaching compensation is documented in §4/§5
(99-primer + forward pointer), not by reordering runtime facts.

### 1.3 Loop segment true-order table (one `while(TRUE)` iteration)

| # | Action | C anchor | Note |
|---|---|---|---|
| L-1 | Optional alloc pressure hook | `main.c:118-119 missing_spares>0?alloc_cycle()`; `alloc.c:227` | Rust structurally eliminates spare pool ([ARCH A-1]); pressure accounting survives in other form. |
| L-2 | Blocking receive | `main.c:121 sef_receive_status(ANY,...)` | Rust: `run_once` receive via transport seam. |
| L-3 | Drop kernel notifications | `main.c:123-127 is_ipc_notify?continue` | |
| L-4 | Authenticate caller endpoint | `main.c:128-130 vm_isokendpt?panic`; `utility.c:84-101` | Grounds 03; failure is fail-fast panic in C. |
| L-5 | Priority 1: VFS transaction route | `main.c:137-144 TRNS_GET_ID + do_procctl` | VFS transid path outranks everything. |
| L-6 | Priority 2: RS birth handshake | `main.c:145-149 RS_INIT?do_sef_init_request` → `SUSPEND` (no reply) | Grounds 01/25 boundary. |
| L-7 | Priority 3: page-fault path (no reply; unblock via sys_vmctl or panic) | `main.c:150-164 VM_PAGEFAULT?do_pagefaults + continue` | Grounds 16; includes forged-source printf (`:154-157`). |
| L-8 | Priority 4: CALLMAP + ACL gate + handler | `main.c:165-176 CALLNUMBER, vm_calls, acl_check, vmc_func` | Grounds 15 + every service doc 18-26. |
| L-9 | Priority 5: out-of-range/unknown → ENOSYS | `main.c:165-167 result=ENOSYS default` | |
| L-10 | Conditional reply (SUSPEND suppresses) | `main.c:178-190 result!=SUSPEND?ipc_send` | SUSPEND is a pseudo-code, not an errno. |

Async corollary: `pagefaults.c:161-168 pf_cont`, `:170-235 handle_memory_continue/final`,
`vfs.c:60-142 vfs_request/do_vfs_reply`, `mmap.c:160-190 mmap_file_cont` show
that long operations suspend the reply and resume on a later message. This is
why 15 must precede all service docs: SUSPEND is the loop's control plane.

### 1.4 Request-lifecycle lines (scenario main lines inside the loop)

- Fork lifecycle: CALLMAP VM_FORK → `fork.c:32 do_fork` → slot check
  (`utility.c:vm_isokendpt`) → ACL inherit (`acl.c:110`) → region copy
  (`region.c:933-999 map_proc_copy`) → pagetable copy (`pagetable.c:1069
  pt_copy`, `:685 pt_ptmap`) → refcount bump (`pb.c:73`) → memtype hooks
  (`memtype.h` ev_reference/ev_copy) → kernel registration → reply.
- brk lifecycle: VM_BRK → `break.c:44 do_brk` → `real_brk :62` →
  `region.c:1002-1060 map_region_extend_upto_v` → `mem_anon.c:115 anon_resize`.
- mmap lifecycle: VM_MMAP/VFS_MMAP → `mmap.c:200 do_mmap` / `:135 do_vfs_mmap`
  → `mmap_region :36` (three-way address resolution) → anon vs file split
  (`mmap_file :84`, `mmap_file_cont :160`, `vfs.c` queue) → `map_perm_check
  :284` + `do_map_phys :310` for device memory → `do_remap :366` for shared.
- munmap lifecycle: VM_MUNMAP/UNMAP_PHYS/SHM_UNMAP → `mmap.c:512 do_munmap`
  / `:488 munmap_vm_lin` → `region.c:1222-1294 map_unmap_range`
  (four cases) / `:1065-1147 map_unmap_region` (three cases) →
  `pb.c:96 pb_unreferenced` → `region.c:568 map_free` (+ fdref for files).
- exit lifecycle: PM-driven two phases → `exit.c:100 do_willexit`
  (freeze) → `exit.c:60 do_exit` (`free_proc :33`, `clear_proc :45`,
  `reset_vm_rusage :25`) → `region.c:589 map_free_proc` + pagetable destroy
  + fdref drain; PROCCTL (`exit.c:117`) handles CLEAR/HANDLEMEM on live procs.
- File-page lifecycle: mmap(file) FDLOOKUP →缺-page FDIO → eviction/close:
  `vfs.c:60 vfs_request` queue → `fdref.c:93/109/116/156` refcounting →
  `mem_file.c:84 mappedfile_pagefault` / `:59 cow_block` / `:195 setfile`.
- Cache lifecycle: FS-driven directory ops: `mem_cache.c:95 do_mapcache`
  / `:196 do_setcache` / `:283 do_forgetcache` / `:315 do_clearcache` over
  `cache.c` directory (`addcache :216`, `rmcache :259`, LRU `:29-74`,
  pressure `cache_freepages :288` called from `alloc.c:260-262`).
- RS lifecycle: SET_PRIV → PREPARE → UPDATE → MEMCTL sub-requests
  (`rs.c:34/71/150/349` + `rs_memctl_* :218-344`) with `utility.c:477
  adjust_proc_refs` and `main.c:755 map_service` registration.
- Query lifecycle: read-only projections over the ledger:
  `utility.c:100 do_info` / `:426 do_getrusage`, `mmap.c:438 do_get_phys` /
  `:463 do_get_refcount`, `region.c:1323-1505 map_get_phys/map_get_ref/
  get_usage_info*/get_region_info`, `cache.c:328 get_stats_info`.

---

## 2. Knowledge pool (deduplicated, stage-wide)

Source types: STOCK = present in existing docs (later needs a destination);
NEW = absent from existing docs but carried by C / non-C artifacts / OS theory
(later needs an evidence anchor, not a destination). Alignment key for later
merging is name + anchor. Main-telling-point marks the doc that teaches the
item fully; all other occurrences become references.

### 2.1 Pool table

| ID | Name | Type | Source | Existing location(s) | Anchor | Reader benefit (question it answers) |
|---|---|---|---|---|---|---|
| K-001 | VM role: sole owner of address-space semantics | concept | STOCK | 00 §1.1 (main) | `minix3/minix/servers/vm/` (whole dir) + `01-stage-kernel/00-kernel-overview.md §1.3` | Why does a user-space server manage memory at all? |
| K-002 | Single-threaded event-loop execution model | concept | STOCK | 00 §1.2; 15 §1.1 | `main.c:111-112 while(TRUE)`; `os/servers/vm/src/vm_server.rs:run/run_once` | What concurrency can I assume when reasoning about VM state? |
| K-003 | Boot causal chain into VM (kernel loads VM ELF, VM enables the rest) | concept | STOCK | 00 §1; 01 §1; 99 (implicit) | `main.c:93 main`; `00-master-plan/README.md` boot chain; `01-stage-kernel/09-vm-boot-protocol.md:27` | Where does VM sit in system bring-up? |
| K-004 | Five-priority dispatch order | mechanism | STOCK | 15 §1.2 (main); 01 (entry mention) | `main.c:137-176` (L-5..L-9) | In what order are competing messages decided? |
| K-005 | SUSPEND pseudo-code / deferred-reply protocol | mechanism | STOCK | 15 §1.3 (main); 16 §1.4; 23 (usage) | `main.c:178-190`; `com.h` SUSPEND | How do async operations coexist with synchronous IPC? |
| K-006 | CALLMAP registration (26 entries, 4 parity-ENOSYS) | interface | STOCK | 15 (main); 99 (numbers) | `main.c:536-575`; `os/servers/vm/src/ipc/dispatcher.rs:975-1000`; `NR_VM_CALLS com.h:769` | Which calls exist and where are they registered? |
| K-007 | `init_vm` 15-step chain | mechanism | STOCK | 01 (main); plan §1.2 diagram | `main.c:428-520` (B-3..B-13) | What runs before the loop, in what order? |
| K-008 | SEF lifecycle (fresh/LU-restart/signal/LU-state) | mechanism | STOCK | 01 §2.4 (main) | `main.c:196,219,241,592,677,731` | How is VM born, restarted, and signalled? |
| K-009 | Boot ELF loading (exec_bootproc + libexec alloc/copy) | mechanism | STOCK | 01 (main) | `main.c:294-417` | How do boot processes get address spaces? |
| K-010 | Boot image layout + memmap + module blobs | data | STOCK | 01 (main); 05 (chunks) | `kernel_boot_info` (`glo.h:25`); `get_mem_chunks utility.c`; `main.c:483-520` | What does the kernel hand VM at birth? |
| K-011 | `struct vmproc` full field set | data | STOCK | 02 (main) | `vmproc.h:14` | What per-process state does VM remember? |
| K-012 | VMF_* orthogonal flags + lifecycle state machine | mechanism | STOCK | 02 (main) | `vmproc.h:35-37`; `exit.c:25-58`; `main.c:262-285` | How is slot liveness expressed? |
| K-013 | `init_proc` slot activation semantics | mechanism | STOCK | 02 (main, slot semantics); 01 (call site) | `main.c:262-285` | What does activating a slot establish? |
| K-014 | Process table + slot allocation/search/iteration | mechanism | STOCK | 03 (main) | `glo.h:20 vmproc[]`; `main.c:457-462` | How does VM remember ALL processes? |
| K-015 | `vm_isokendpt` endpoint-to-slot translation + generation | mechanism | STOCK | 03 (main) | `utility.c:84-101`; `endpoint.h:_ENDPOINT_GENERATION_SHIFT` | How is a message sender authenticated? |
| K-016 | VMP_EXECTMP reserved slot | constraint | STOCK | 03 (main) | `glo.h:VMP_EXECTMP` | Where does transient exec state live? |
| K-017 | ACL bitmap semantics + DEFAULT/SYSTEM layers | mechanism | STOCK | 04 (main) | `acl.c:21-130`; `com.h:769-770` mask width | Who may call which VM service? |
| K-018 | ACL five operations (init/check/set/fork/clear) | mechanism | STOCK | 04 (main) | `acl.c:21,37,70,110,120` | What is the permission lifecycle? |
| K-019 | ACL fail-closed hardening [ARCH A-11] | arch-evolution | STOCK | 04 (main) | `acl.c:44-53` vs `os/servers/vm/src/acl.rs:97-116` | How does Rust differ on unregistered callers, and why? |
| K-020 | ACL gate wiring in dispatch (incl. None-means-deny) | mechanism | STOCK | 15 (wiring); 04 (structure) | `main.c:168`; `vm_server.rs:1266-1271` + Fix #66 | Where is the gate installed in the loop? |
| K-021 | Physical memory inventory (memmap → chunks → clicks) | mechanism | STOCK | 05 (main) | `utility.c:get_mem_chunks`; `main.c:455`; `type.h:memory`; `param.h:MAXMEMMAP` | How does VM take over physical RAM? |
| K-022 | alloc_mem/free_mem arbitrary-size contiguous allocation | mechanism | STOCK | 05 (main) | `alloc.c:242,289,404,465` | How are variable-size runs allocated? |
| K-023 | Triple-backend allocator (bitmap+buddy+segment-tree) [ARCH A-5] | arch-evolution | STOCK | 05 (main) | `alloc.c` (single bitmap) vs `os/servers/vm/src/phys_mem/` + `PhysAllocator` trait | Why three implementations behind one trait? |
| K-024 | Reserved queue + alloc_cycle + missing_spares accounting | mechanism | STOCK | 05/06 (main, split by definition/use) | `alloc.c:56-58,74,100-237`; `main.c:118-119`; Rust deletion per Fix #70 | What survives of the reserve mechanism under Direct Map? |
| K-025 | memstats/usedpages accounting + diagnostics | tool | STOCK | 05 (main); 26 (consumption) | `alloc.c:348,486,501-509` | How is memory pressure observed? |
| K-026 | PAF_* allocation flags + NO_MEM/Result mapping | interface | STOCK | 05/99 (main) | `vm.h:22-27,62` | How do callers constrain allocation? |
| K-027 | VM self-page allocation (vm_allocpage/pages/mappages/freepages) | mechanism | STOCK | 06 (main) | `pagetable.c:235-440` | How does VM allocate pages for itself? |
| K-028 | Spare-pool elimination under Direct Map [ARCH A-1] | arch-evolution | STOCK | 06 §3.3 (main) | `pagetable.c:55-57,60-77,1311-1345` vs `direct_map.rs` + `alloc_page::vm_pt_alloc` | Why does Rust delete the reserve pool? |
| K-029 | vm_pagelock/vm_addrok + get_vm_self_pages | mechanism | STOCK | 06 (main) | `pagetable.c:403,440,1500` | How are self pages locked/counted/validated? |
| K-030 | pt_t structure + 2-level (C) vs 4-level (Rust) hierarchy [ARCH A-2] | data | STOCK | 07 (main) | `pt.h:11`; `arch/i386/pagetable.h`; `arch/earm/pagetable.h` | What does a page table look like? |
| K-031 | Direct Map dual views + constant-offset translation [ARCH A-1] | mechanism | STOCK | 07 (main, explicit chapter); 05/06 (consumption) | `os/arch/src/direct_map.rs`; `direct_map.rs`; C absence (`PMAP_DIRECT_MAP` never defined) | How does VM access any physical page without aliasing? |
| K-032 | VM self-map pagetable [ARCH A-9] | mechanism | STOCK | 07 (main) | `pagetable.c:static_sparepagedirs` vs `pagetable/vm_self_map.rs` | How does VM access its own tables? |
| K-033 | 64-bit address-space width [ARCH A-6] | constraint | STOCK | 00/07/20 (main) | `vm.h:65-79` vs `MMAP_BASE/MMAP_TOP mmap.rs:203-204` | Where may mmap regions live? |
| K-034 | Multi-arch paging trait (i386+earm → x86-64+arm64+riscv64) [ARCH A-10] | arch-evolution | STOCK | 07 (main) | `arch/*/pagetable.h` vs `minix_arch::paging` | How is paging abstracted across ISAs? |
| K-035 | Pagetable lifecycle (new/free/bind) | mechanism | STOCK | 08 (main) | `pagetable.c:990,1358,1427` | How are per-process tables born/destroyed/activated? |
| K-036 | Mapping write path (writemap + WMF_* + write_pte_dm + per-entry invlpg) | mechanism | STOCK | 08 (main, incl §1.8) | `pagetable.c:784-934`; `os/arch/src/x86_64/paging.rs:write_pte_dm` | How do PTE updates become visible to hardware? |
| K-037 | On-demand pagetable-page allocation (ptalloc/in_range) | mechanism | STOCK | 08 (main) | `pagetable.c:494,545` | How are intermediate levels materialised? |
| K-038 | Cross-table copy (pt_copy/pt_map_in_range/pt_ptmap) | mechanism | STOCK | 08 (main) | `pagetable.c:631,685,1069` | How are mappings duplicated across tables? |
| K-039 | Range/query helpers (checkrange/writable/clearmapcache/freepde/kernel-mapped) | mechanism | STOCK | 08 (main) | `pagetable.c:751,761,943,1028,1035,1442` | What auxiliary operations exist and which are eliminated under DM? |
| K-040 | Slab size-class allocator → HeapArena+VmAllocator [ARCH A-3] | arch-evolution | STOCK | 09 (main) | `slaballoc.c` (528) vs `heap_arena.rs` + `global.rs` free-list | How does VM get a heap after bootstrap? |
| K-041 | SLABALLOC/SLABFREE typed macros + MEMPROTECT + slabstats | mechanism | STOCK | 09 (main) | `proto.h:133-134`; `vm.h:13`; `slaballoc.c:504` | What C heap surface is intentionally not ported? |
| K-042 | Bootstrap relocation (static→dynamic, spare-pool replacement) | mechanism | STOCK | 10 (main) | `pagetable.c:1311-1345`; `alloc.c:reservedqueue_*` | How does VM switch from temporary to steady-state allocation? |
| K-043 | LU support mechanics (swap slots/dyn-data, transfer regions, setparent) | mechanism | STOCK | 10 (main, mechanism) vs 25 (service flow) | `utility.c:188,228,283,312`; `region.c:1535 map_setparent` | What primitives let live update migrate state? |
| K-044 | phys_block/phys_region two-layer objects + PBF_* | data | STOCK | 11 (main, C side) | `pb.c`; `region.h:23`; `phys_region.h:8` | How did C represent per-page state? |
| K-045 | PFN-indexed global array model (PageFrames/PageSlot/PageState) | arch-evolution | STOCK | 11 (main, Rust side) | `region/page_state.rs` | Why replace heap objects with a PFN array? |
| K-046 | refcount/link/unreference/CoW-split entry (pb_* + mem_cow) | mechanism | STOCK | 11 (definitions); 17 (CoW consumption) | `pb.c:32,54,61,73,96,136` | How is sharing counted and split? |
| K-047 | mem_type_t 15-callback table × 6 instances | interface | STOCK | 12 (main) | `memtype.h:12`; `mem_anon.c:34` et al. | What is the polymorphic memory contract? |
| K-048 | Per-type behaviour (anon/contig/direct/shared/cache/mappedfile) | mechanism | STOCK | 12 (main, six sections) | `mem_anon.c`, `mem_anon_contig.c`, `mem_directphys.c`, `mem_shared.c`, `mem_cache.c`, `mem_file.c` | What does each memory kind do on fault/copy/delete? |
| K-049 | vir_region/phys_region mapping + 22 framework ops | mechanism | STOCK | 13 (main) | `region.c` (1555); `region.h:37-79` | When is each region op called (framework "when")? |
| K-050 | Region lookup index (AVL → BTreeMap) [ARCH A-4] | arch-evolution | STOCK | 14 (main) | `regionavl.c` + `cavl_*.h` (1426 template lines) vs `region/region_map.rs` | How are address→region and free-gap queries O(log n)? |
| K-051 | Pagefault two entries (passive VM_PAGEFAULT + active SIGKMEM/do_memory) | mechanism | STOCK | 16 (main) | `pagefaults.c:76,240,294`; `main.c:153-164,731-750` | How do CPU faults and kernel self-guarantees converge on map_pf? |
| K-052 | handle_memory state machine (start/once/step/final/continue) | mechanism | STOCK | 16 (main) | `pagefaults.c:170-417` | How are multi-step guarantees driven to completion? |
| K-053 | VFS callback + SUSPEND resume for file pages | mechanism | STOCK | 16 (protocol); 23 (queue) | `pagefaults.c:161-168 pf_cont`; `vfs.c` | How does a fault survive a VFS round trip? |
| K-054 | TLB discipline invariant (VM only edits non-running tables) + per-entry invlpg | constraint | STOCK | 16 §3.7 + 08 §1.8 (main) | `pagetable.c:799-815,928-934` (C inhibit batch) vs `write_pte_dm` + Fix #69 | Why is per-entry invalidation sufficient? |
| K-055 | CoW establish→protect→split protocol | mechanism | STOCK | 17 (main) | `pb.c:136 mem_cow`; `mem_anon.c:105 anon_writable`; `region.c:257 map_ph_writept` | How does write sharing work? |
| K-056 | CoW fast-path gate (exclusive AND writable) + mappedfile retype to anon | mechanism | STOCK | 17/16 (main, Fix #63) | `mem_file.c:59-82 cow_block` (`ph->memtype=&mem_type_anon`); `cow_exec_pf.rs:268-285` | When may a fault flip permission instead of copying? |
| K-057 | Fork orchestration (verify→init→tables→copy→register→reply, rollback) | mechanism | STOCK | 18 (main, secondary trunk) | `fork.c:32-115`; `region.c:933-999`; `acl.c:110` | How is address-space sharing organised as one service? |
| K-058 | brk grow/shrink/no-change + DATA/STACK_CHANGED | mechanism | STOCK | 19 (main) | `break.c:44-69`; `region.c:1002-1060`; `mem_anon.c:115` | How does the heap top move? |
| K-059 | brk shrink divergence (C ignores, Rust frees) + resize-in-extend [ARCH A-12] | arch-evolution | STOCK | 19 (main) | `break.c` vs `brk.rs:128-204`; `vir_region.rs:127-140` | Which heap behaviours intentionally differ? |
| K-060 | mmap three-way address resolution (fixed/hint/default) | mechanism | STOCK | 20 (main) | `mmap.c:36-83 mmap_region`; `region.c:302-416` | Where do new intervals come from? |
| K-061 | mmap four source bindings (anon/file/phys/shared) + VFS async file path | mechanism | STOCK | 20 (main) | `mmap.c:84-190,200-269`; `mem_file.c:191 setfile`; `mem_shared.c:167 setsource` | What backs a new interval? |
| K-062 | MAP_PHYS establishment + map_perm_check privilege tiers | mechanism | STOCK | 20 (main after fix); 21 (moved out) | `mmap.c:284-363` | Who may map device memory? |
| K-063 | munmap teardown (range four-cases + region three-cases + split) | mechanism | STOCK | 21 (main) | `mmap.c:488-573`; `region.c:527-612,616-641,1065-1294` | How are partial overlaps dismantled safely? |
| K-064 | Unmap resource return (pb_unreferenced + map_free + fdref) | mechanism | STOCK | 21 (main) | `pb.c:96`; `region.c:568`; `fdref.c:116` | Where do freed pages/structures go? |
| K-065 | Exit two phases (WILLEXIT freeze + EXIT release) + PROCCTL CLEAR/HANDLEMEM | mechanism | STOCK | 22 (main) | `exit.c:25-156`; `region.c:589 map_free_proc` | How are address spaces finally returned? |
| K-066 | VFS async dialogue (FDLOOKUP/FDIO/FDCLOSE) + serial queue | mechanism | STOCK | 23 (main) | `vfs.c:33-142`; `mmap.c:135-195`; `pagefaults.c:pf_cont` | How do VM and VFS cooperate across servers? |
| K-067 | fdref four-state dedup + refcount + unconditional close-at-zero | mechanism | STOCK | 23 (main) | `fdref.c:93-176` (Fix 23-P0-1b/c) | How are VM-held fds lifetime-managed? |
| K-068 | MappedFile fault分流 (hit/ONCE/miss/clearend tail) | mechanism | STOCK | 23 (main); 12 (callbacks) | `mem_file.c:30-287`; Fix #80 tail-zero | How do file pages enter memory? |
| K-069 | Page-cache directory (dual keys + exact LRU + refcount eviction) | mechanism | STOCK | 24 (main) | `cache.c:21-331`; `cache.h:2-21` | Why does the disk-block directory live in VM? |
| K-070 | Four cache IPCs (map/set/forget/clearcache) | interface | STOCK | 24 (main) | `mem_cache.c:95,196,283,315` | How do filesystems manipulate cached blocks? |
| K-071 | Cache pressure path (alloc failure → free_pages batch) | mechanism | STOCK | 24 (main); 05 (hook) | `alloc.c:260-262`; `cache.c:288`; `page_cache.rs:free_pages` | How does memory pressure evict cache? |
| K-072 | RS four services + RprocTab handshake + adjust_proc_refs | mechanism | STOCK | 25 (main) | `rs.c:34-390`; `main.c:241-260,592-768`; `utility.c:477-492` | How does live update command VM? |
| K-073 | RS gaps (PREPARE step 5 / UPDATE switch / MAKE_VM) [ARCH A-8] | constraint | STOCK | 25 (main, gap contracts) | `rs.c:71-276` vs `rs.rs` NotImplemented + Fix #68 | What LU surface is deferred and closed? |
| K-074 | Four query services (INFO/GETPHYS/GETREF/GETRUSAGE) + consumers | interface | STOCK | 26 (main) | `utility.c:100,426`; `mmap.c:438,463`; `region.c:1323-1505`; `cache.c:328` | What read-only windows does the ledger offer? |
| K-075 | Region-id/refcount-token semantics (GETPHYS/GETREF naming trap) | concept | STOCK | 26 (main) | `mem_anon.c:132,142`; `mem_shared.c:99,207` | Why "get phys" does not return a physical address? |
| K-076 | Endpoint/generation + VM_*/VMP_*/PAF/VR/PBF/VMSF constant families | data | STOCK | 99 (main); 00 (map) | `com.h`; `vm.h`; `vmproc.h`; `region.h`; `cache.h`; `endpoint.h` | What is the shared vocabulary across all docs? |
| K-077 | vm.lds link script + arch Makefiles + top Makefile (build shape) | tool | NEW | nowhere fully (01 touches ELF loading only) | `arch/earm/vm.lds`; `arch/*/Makefile.inc`; `Makefile` | What links VM and what is intentionally non-semantic? |
| K-078 | Boot image + kernel_boot_info wire (memmap/modules/boot_procs) | interface | NEW | 01 (partial: consumption, not layout) | `glo.h:25`; `main.c:442-455,483-520`; `minix/type.h:memory` | What exact bytes does the kernel hand VM? |
| K-079 | Kernel↔VM trap/protocol surface (pagefault forward, sys_vmctl, SIGKMEM, transid) | interface | NEW | 15/16 (partial per-path, no unified table) | `main.c:131-164`; `pagefaults.c`; `com.h`; `ipc.h`; `vfsif.h:79-81`; `ipcconst.h:28,34` | What crosses the kernel boundary, in which direction? |
| K-080 | minix-types wire structs + 56-byte assertions + overlay corrections | interface | NEW | per-doc wire notes (scattered; V13-P2-6) | `os/libs/minix-types/src/ipc/vm.rs`; `message.rs`; `ipc.h` | Where is the byte-exact IPC contract pinned? |
| K-081 | Errno/reply mapping (per-call errors, ENOSYS/ENOMEM/EINVAL/EFAULT/EPERM/SUSPEND) | constraint | NEW | per-doc error lines (scattered, no stage table) | `main.c:137-190`; `dispatcher.rs` error maps; `brk.rs/mmap.rs/munmap.rs` | Which failures produce which replies? |
| K-082 | Shutdown/panic model (VM never exits; panic=abort; fail-fast vs fail-closed) | constraint | NEW | 00 §1.3 sketch; 15 (transport limit); 22 (exit of others) | `main.c:130,147,186 panic`; `os/Cargo.toml:panic=abort`; `vm_server.rs:991,1069` | What happens when VM itself fails? |
| K-083 | Concurrency/TLB/SMP story (single-thread + inhibit history + E-VMTLB edge) | constraint | NEW | 16 §3.7 + 08 §1.8 (recent, thin pointers elsewhere) | `pagetable.c:799-815`; `kernel/proc.c:345-347 MF_FLUSH_TLB`; edge E-VMTLB | What breaks on true SMP and where is it tracked? |
| K-084 | Test seams (KernelGateway/MockGateway, SimPaging, allocator parity, CALLMAP guard) | tool | NEW | 00 §4.1 sketch + per-doc §5 tables (no unified seams chapter) | `kernel_gateway.rs`; `pagetable/sim.rs`; `phys_mem/allocator_tests.rs`; `dispatcher.rs:1263-1284` | How is hardware-free testing organised? |
| K-085 | Dead/deferred surface (EXEC_NEWMEM/DMA×3, RS PREPARE/UPDATE, dead decls, sanity family) | constraint | STOCK | plan §5.4 + checklist + 25 (scattered) | `proto.h map_memory/unmap_memory`; `region.c:copy_abs2region`; `sanitycheck.h`; `com.h:637,656,664,672` | What is intentionally not implemented? |
| K-086 | VMP categories / WMF flags / MAP_NONE sentinel encodings | data | STOCK | 08/99 (main) | `vm.h:49-62` | How are alloc-reason/write-mode/empty-slot encoded? |
| K-087 | Region split/middle/head/tail + vrallocflags→PAF mapping | mechanism | STOCK | 13/21 (main, split across framework/service) | `region.c:1150-1294`; `vir_region.rs:279-368` | How do partial operations preserve the no-overlap invariant? |
| K-088 | Pin/lock semantics (map_pin_memory, vm_pagelock, MEMPROTECT compile-dead) | constraint | STOCK | 13/06/09 (main, scattered) | `region.c` pin; `pagetable.c:403`; `vm.h:13` | What "do not move/reclaim" means in each layer? |
| K-089 | Usage/stats plumbing (total_pages, vm_self_pages, cache stats, rusage) | data | STOCK | 05/06/24/26/22 (main, scattered) | `glo.h:45`; `pagetable.c:1500`; `cache.c:328`; `exit.c:25` | How do counters flow to queries? |
| K-090 | Debug/diagnostic surface (printmap/printregionstats/pf_errstr/slabstats) | tool | STOCK | 13/16/05/09 (main, debug corners) | `region.c:40,98`; `pagefaults.c:59`; `alloc.c:486`; `slaballoc.c:504` | What is debug-only and replaced by cfg/tests? |

Statistics: 90 items; by type: concept 6, mechanism 48, data 9, interface 12,
constraint 10, arch-evolution 8 (embedded), tool 5. By source: STOCK 80, NEW 10
(K-077..K-084 plus parts of K-078..K-080 that consolidate scattered notes).
Duplicates merged: Direct Map (05/06/07→K-031 main in 07); memtype callbacks
(12 defs vs 23/24 uses→K-048 vs K-068/K-070); map_phys (20+21→K-062 single
owner); region framework vs service splits (13 vs 21/22/26→K-049 vs
K-063/K-065/K-074); exit rusage vs queries (22 vs 26→K-065 vs K-089);
dispatch gate (04 vs 15→K-019 vs K-020).

### 2.2 Ordering note for Part F (11/12/13/14)

Numeric order keeps 11→12→13→14 (no renumber; see §4/§8 for cost). To satisfy
zero-forward-reference, contracts for 11 and 12 require self-contained minimal
region sketches (a 5-line "interval + per-page slots" cartoon with explicit
"full treatment in 13" pointer) rather than prerequisite links to 13. Readers
may still take the pedagogical branch 13→14→11→12 after 10; that branch is
declared skippable-reorder in §4, not the normative numbering.

---

## 3. Coverage audit

### 3.1 Theme universe (four sources)

1. C symbols: 371 extracted symbols (coverage-extract 2026-09-09: doc-covered
   92.5%); 175 item-by-item semantic judgments in todo §17.1 (COVERED 103 /
   ARCH-EVOLVED 32 / DEBUG-ONLY 27 / COMPILE-FLAG 4 / OBSOLETE 8 / REAL-GAP
   1→0 after Fix #64). All 24 `.c` files have owning docs (plan §5.1 table,
   re-verified by `ls` + per-file `rg` during this task).
2. OS-generic concepts: PCB/table, ACL/capability, allocator, paging,
   heap bootstrap, ledger/index, dispatch, fault handling, CoW, fork/brk/mmap/
   munmap/exit lifecycles, file mapping, page cache, live update, read-only
   queries, generation-protected identity, fail-stop.
3. Non-C artifacts (§3.4 fixed list).
4. Boundary contracts: `00-master-plan/README.md` (VM is first user service,
   enables the rest; RS/PM/VFS follow), `edge_todo.md` (E1/E2/E-RSWIRE/
   E-VFSWIRE/E-VMTLB), `01-stage-kernel/00-kernel-overview.md` (kernel/VM
   split), `02-stage-vm/plan.md` + `todo.md` (declared non-goals).

### 3.2 Coverage-gap table (universe themes with no owning doc)

| # | Theme | Suggested disposition | Evidence anchor for new STOCK |
|---|---|---|---|
| G-1 | Unified kernel↔VM boundary table (fault forward, sys_vmctl set, SIGKMEM, transid, notify filter) | Merge into 15 contract (new §1.6) + 99 wire rows; no new doc | `main.c:123-164`; `pagefaults.c`; `vfsif.h:79-81`; `ipcconst.h:28,34` |
| G-2 | Boot-image byte layout + kerninfo handoff fields | Merge into 01 contract (explicit §2 table); no new doc | `glo.h:25`; `main.c:442-520`; `minix/type.h:memory` |
| G-3 | Stage-wide errno/reply matrix (per-call errors + SUSPEND vs errno) | Merge into 15 (matrix) + per-service errno rows; no new doc | `main.c:137-190`; `dispatcher.rs` maps |
| G-4 | Shutdown/panic/fail-stop model for VM itself | Merge into 00 §1 + 15 transport-limit + 22 (others' exit); no new doc | `main.c:130,147,186`; `panic=abort`; `vm_server.rs:991,1069` |
| G-5 | Test-seam catalogue (gateway/mock/sim/parity/guard) | Merge into 00 §4.1 expansion + per-doc §5 rows; no new doc | `kernel_gateway.rs`; `pagetable/sim.rs`; `allocator_tests.rs` |
| G-6 | Dead/deferred register (single table, no longer scattered) | Merge into 99 appendix + per-doc NOT-covered rows; no new doc | plan §5.4; `proto.h`; `sanitycheck.h`; `com.h` DMA/EXEC |
| G-7 | Link/build shape (vm.lds + Makefiles + non-semantic ruling) | Merge into 01 boundary (explicit WONTFIX row); no new doc | `arch/earm/vm.lds`; `arch/*/Makefile.inc`; `Makefile` |

No gap requires a new numbered doc. Each gap becomes NEW knowledge
(K-077..K-084) with anchors above and lands in an existing contract. There is
no "to be determined": every non-C checklist item is answered in §3.4.

### 3.3 Duplication table (same theme taught in ≥2 docs; first = main point)

| # | Theme | Occurrences | New rule (keep main, others reference) |
|---|---|---|---|
| D-1 | Direct Map concept | 05 (consumption) / 06 (consumption + §3.3) / 07 (main §definition) | Keep 07 main; 05/06 keep 6-line consumption cartoon + pointer; 99 holds constants. |
| D-2 | MappedFile/Cache callback semantics | 12 (definitions) / 23 (file consumption) / 24 (cache consumption) | Keep 12 definitions; 23/24 describe consumption only, no callback re-definition. |
| D-3 | MAP_PHYS establishment | 20 (establishment) + 21 (establishment repeated) | Single owner 20 (K-062); 21 keeps teardown only; migrate 21 §§ establishment to 20 (see §8). |
| D-4 | Region split/unmap mechanics | 13 (framework) / 21 (service use) | Keep 13 framework; 21 narrates service orchestration, calls framework by name. |
| D-5 | Exit resource return vs query counters | 22 (release) / 26 (readout) | Keep 22 release; 26 reads counters, never re-explains release. |
| D-6 | ACL structure vs gate wiring | 04 (structure) / 15 (wiring) | Keep 04 structure; 15 wiring only. |
| D-7 | Main-loop text (entry vs loop) | 01 (entry + loop pointer) / 15 (loop) | Keep 01 entry; loop body lives only in 15. |
| D-8 | LU mechanics vs LU service flow | 10 (primitives) / 25 (orchestration) / 01 (SEF birth) | Keep 10 primitives; 25 orchestration; 01 birth only. Cross-pointers, no re-teaching. |
| D-9 | Stats plumbing (self-pages/total/cache/rusage) | 05/06/24/26/22 corners | Single flow documented in K-089; each doc covers its segment + pointer. |

### 3.4 Out-of-scope table (taught doc went beyond its boundary; correct home)

| # | Item | Currently in | Correct home |
|---|---|---|---|
| O-1 | `get_mem_chunks` full semantics inside boot narrative | 01 (call-site doc) | 05 owns semantics; 01 keeps call-site row + pointer. |
| O-2 | Dispatch wiring (`acl_check` gate, SUSPEND) inside ACL structure | 04 corners | 15 owns wiring; 04 keeps structure + pointer. |
| O-3 | `map_phys` establishment inside teardown doc | 21 | 20 (see D-3). |
| O-4 | `map_free_proc` full release inside framework doc | 13 corners | 22 owns full release; 13 keeps framework signature + pointer. |
| O-5 | Query-counter definitions inside release doc | 22 corners | 26 owns readout; 22 keeps clear-timing row. |
| O-6 | Slab C-internals dominating Rust heap decision | 09 weight | Rebalance: C slab → summary table; HeapArena decision → main. |

### 3.5 Non-C themes: fixed-list answers (each answered, none left open)

Link/load: answered in 01 (ELF load + boot alloc/copy) + 01 WONTFIX row for
`vm.lds` (link layout is build-only, no runtime semantics; anchor
`arch/earm/vm.lds`). Image/memory layout: answered in 01 (boot-image table +
kerninfo fields; anchors `glo.h:25`, `main.c:442-520`). Asm/trap entry:
answered in 15/16 (notify filter, fault forward, SIGKMEM; anchors
`main.c:123-164`, `pagefaults.c`, `ipcconst.h`). Boot chain: answered in
00/01 (kernel→VM→RS→rest; anchor master-plan README + `09-vm-boot-protocol`).
Build/toolchain: answered in 01 boundary (Makefiles are build-only WONTFIX;
anchors `Makefile`, `arch/*/Makefile.inc`). Cross-module iface/wire:
answered in 15 + 99 + per-doc wire rows (CALLMAP, transid, SUSPEND,
minix-types 56-byte pins; anchors `com.h`, `ipc.h`, `vm.rs`, `message.rs`).
Error paths: answered in 15 matrix + per-service errno rows (anchors §3.2
G-3). Shutdown/exit: answered in 00/15/22 (VM fail-stop; others' two-phase
exit; anchors §3.2 G-4). Concurrency/sync: answered in 02/16/08 (single
thread + TLB invariant + E-VMTLB edge; anchors §3.2 G-5/§2 K-083). Test
infra: answered in 00 + per-doc §5 (gateway/mock/sim/parity/guard; anchors
§3.2 G-5).

---

## 4. New catalogue

Decision: keep the 28-number scheme (00, 01-26, 99). No renumber. Reasons:
(1) runtime order (§1) and teaching order already agree on the backbone
(00→birth→ground→tables→ledger→heart→services→collaboration); the remaining
defects are boundary/weight/ownership defects, not ordering defects;
(2) breakage cost of renumbering exceeds benefit (see §8: ~10 cross-refs per
doc plus code-comment anchors); (3) length analysis shows no doc exceeds the
soft single-concept ceiling (max 1098 < 3000), so splits are not forced;
targeted fixes (single-owner rule, self-contained intros, weight rebalance)
achieve zero-forward-reference without moving numbers.

### 4.1 New chapter table (number, title, one-line定位, group)

| New No. | Title | One-line定位 | Group |
|---|---|---|---|
| 00 | vm-overview | Entry map: role boundary, event-loop skeleton, module layers, reading routes | A. Overview + vocabulary |
| 99 | global-concepts | Shared vocabulary: numbers, flags, sentinels, globals, dead-register | A. Overview + vocabulary |
| 01 | vm-init-main | Birth chain from kernel handoff to loop entry (SEF, ELF, image, heap divide) | B. Birth |
| 02 | vmproc-struct | One PCB: fields, orthogonal flags, lifecycle state machine | B. Birth |
| 03 | vmproc-table | All PCBs: table, slot allocation, endpoint authentication | B. Birth |
| 04 | acl | Service gate data: bitmap, layers, five-operation lifecycle | C. Admission + ground |
| 05 | physical-memory | Ground ledger: inventory, triple-backend alloc, accounting, reserves | C. Admission + ground |
| 06 | page-allocator | Self pages: VM's own allocation under Direct Map | D. Pages + tables |
| 07 | pagetable-struct | Table shape: hierarchy, dual views, self-map, width, arch trait | D. Pages + tables |
| 08 | pagetable-ops | Table verbs: lifecycle, writes, on-demand levels, cross-copy, queries | D. Pages + tables |
| 09 | slab-allocator | Heap: C slab summary → HeapArena decision (rebalanced weight) | E. Heap + bootstrap terminus |
| 10 | vm-relocation | Bootstrap terminus: static→dynamic + LU primitives (two halves, one doc) | E. Heap + bootstrap terminus |
| 11 | phys-pagestate | Per-page state: C two-layer objects → PFN array (self-contained intro) | F. Address-space ledger |
| 12 | memtype | Policy table: 15 callbacks × 6 types (self-contained intro) | F. Address-space ledger |
| 13 | region-mapping | Framework: 22 ops, when each is called | F. Address-space ledger |
| 14 | region-lookup | Index: ordered search + gap finding in O(log n) | F. Address-space ledger |
| 15 | ipc-dispatch | Heart: five priorities, CALLMAP, SUSPEND, transid, gate wiring, errno matrix | G. Heart |
| 16 | pagefault | Fault protocol: two entries, state machine, VFS resume, TLB invariant | G. Heart |
| 17 | cow-mechanism | Sharing protocol: establish→protect→split + fast-path gate | G. Heart |
| 18 | vm-fork | Secondary trunk: fork orchestration with rollback | H. Services |
| 19 | vm-brk | Heap service: grow/shrink/no-change + divergences | H. Services |
| 20 | vm-mmap | Establishment: address resolution + four bindings + MAP_PHYS (single owner) | H. Services |
| 21 | vm-munmap | Teardown: range/region cases + resource return (no establishment) | H. Services |
| 22 | vm-exit | Terminus: two-phase exit + PROCCTL live control | H. Services |
| 23 | vfs-interaction | Cross-server file dialogue: queue, fdref, mapped-file faults | I. Collaboration |
| 24 | page-cache | Disk-block directory: dual keys, LRU, eviction, four cache IPCs | I. Collaboration |
| 25 | rs-services | Live-update command: four RS services + handshake + gap contracts | I. Collaboration |
| 26 | vm-queries | Read-only windows: stats/usage/region/phys/ref/rusage + consumers | I. Collaboration |

Groups: A(00,99) → B(01-03) → C(04-05) → D(06-08) → E(09-10) → F(11-14) →
G(15-17) → H(18-22) → I(23-26). Normative order is numeric. Pedagogical
branch allowed once: after 10, readers may take 13→14→11→12 instead of
11→12→13→14 if they prefer framework-first; both converge at 15. This branch
is declared here, not hidden.

### 4.2 Reading paths

- Main line (all readers): 00 → 99 (skim) → 01 → 02 → 03 → 04 → 05 → 06 →
  07 → 08 → 09 → 10 → 13 → 14 → 11 → 12 (or numeric 11→12→13→14; same
  convergence) → 15 → 16 → 17 → 18 → 19 → 20 → 21 → 22 → 23 → 24 → 25 → 26.
- Branchable: 09 (heap decision) skippable until 19/20 need heap context;
  24 skippable until file pressure matters; 25 skippable until LU matters;
  26 skippable (diagnostics) any time after 15.
- Parallel bodies (no fake linear order inside): syscall services
  (18/19/20/21/22) share the loop + ledger framework (15 + 13/14); representative
  member taught fully is 18 (fork: verify→copy→protect→register→rollback
  exercises every layer), others as difference tables against it. Device/cache
  parallelism (20 MAP_PHYS vs 24 cache IPCs) is grouped by scenario, not merged.

### 4.3 Splits/merges considered and rejected (with reason + cost)

- Split 08 (1098 lines: lifecycle/write/alloc/copy/query): rejected. Single
  concept (table verbs) holds; 1098 < 3000 soft ceiling. Split would force
  08a/08b insertion + downstream renumber (≈10 inbound refs + arch-crate
  consumer comments). Instead enforce internal §1-§7 template + per-op
  subsections.
- Split 01 (830 lines: init chain + SEF + ELF + image + heap divide): rejected
  for same reason (single concept = birth). Fix by moving loop body strictly
  to 15 (D-7) and keeping 01 as entry.
- Split 26 (743 lines, four queries): rejected. Single concept (read-only
  windows) + parallel-body diff table is the prescribed organisation for
  parallel syscalls; split would quadruple navigation cost for diagnostics
  readers.
- Split 10 (bootstrap + LU primitives): rejected. The two halves share the
  "temporary→steady" idea; split would duplicate background in 25. Fix by
  explicit two-half contract + pointers.
- Swap 11/12 before 13/14 (framework-first numbering): rejected. Benefit is
  one fewer forward-pointer; cost is 4-doc renumber + ~30 inbound refs
  (16/18/19/20/21/22/26 reference 13 by number) + code comments. Fix by
  self-contained intros (§2.2) achieves the same pedagogy at near-zero cost.
- New wire/build/TLB/test docs: rejected as numbered docs (see §3.2 G-1..G-7).
  Each lands as an explicit section inside an existing owner; a new number
  would create a 29th node in an already dense mesh for cross-cutting
  concerns better kept local.

---

## 5. Per-document contracts (all 28; seven elements each; B-phase task orders)

Format per doc: positioning; teaches (K refs); does-not-teach (with handoff);
prerequisites (earlier numbers only); post-dependents; ground truth (C +
non-C anchors); knowledge list (one row per item); acceptance (checkable).

### 00-vm-overview

- Positioning: answers "what is VM, where in boot, how to read the other 27".
- Teaches: K-001, K-002, K-003, K-006 (map only), K-082 (model), K-084 (seam map), K-085 (pointer to 99 register).
- Does-not-teach: any mechanism detail → respective doc; constants → 99;
  boot ELF bytes → 01; dispatch priorities → 15.
- Prerequisites: none.
- Post-dependents: all docs reference this map.
- Ground truth: `minix3/minix/servers/vm/` (24 .c, 9260 lines);
  `os/servers/vm/src/` (whole crate); `00-master-plan/README.md` boot chain;
  `01-stage-kernel/00-kernel-overview.md §1.3`.
- Knowledge list:
  | K-001 | VM role | concept | §1 map + kernel/VM split | STOCK 00 §1.1 |
  | K-002 | event loop model | concept | §1 single-thread + borrowck-as-audit | STOCK 00 §1.2 |
  | K-003 | boot chain place | concept | §2 causal chain kernel→VM→RS→rest | STOCK 00 §1 + master-plan |
  | K-082 | fail-stop model | constraint | §3 VM never exits; panic paths | NEW kern panic + abort |
  | K-084 | seam map | tool | §4 gateway/mock/sim/parity/guard pointers | NEW gateway/sim/tests |
- Acceptance: reader can state VM's three authorities + loop skeleton +
  9-group map without opening another doc; source-map table has zero
  file→doc mismatches (spot-check 10 rows against `ls` + §1 tables).

### 99-global-concepts

- Positioning: answers "what does this number/flag/sentinel mean, authoritatively".
- Teaches: K-076, K-086, K-026, K-006 (numbers), K-085 (dead-register appendix),
  K-080 (wire-pointer rows), K-081 (errno names, not matrix).
- Does-not-teach: mechanism use → consumer doc; dispatch matrix → 15;
  boot layout bytes → 01.
- Prerequisites: 00.
- Post-dependents: all mechanism docs cite this for magic numbers.
- Ground truth: `glo.h`, `vm.h`, `com.h`, `ipc.h`, `endpoint.h`, `region.h`,
  `cache.h`; `os/libs/minix-types/src/ipc/vm.rs`, `message.rs`;
  `os/servers/vm/src/global.rs`, `mmap.rs`, `page_cache.rs`.
- Knowledge list:
  | K-076 | constant families | data | call/flag/sentinel tables | STOCK 99 |
  | K-086 | VMP/WMF/MAP_NONE | data | encoding rows | STOCK 08/99 |
  | K-085 | dead/deferred register | constraint | appendix table + per-doc pointers | STOCK plan §5.4 |
- Acceptance: every magic number in mechanisms resolves here with C anchor +
  Rust anchor + same-value check; dead items listed once with reason.

### 01-vm-init-main

- Positioning: answers "how does VM go from kernel-given temp environment to
  self-managing before the loop".
- Teaches: K-007, K-008, K-009, K-010, K-078, K-077 (WONTFIX row), K-003.
- Does-not-teach: vmproc fields → 02; table/endpoint → 03; ACL depth → 04;
  allocator semantics → 05; loop dispatch → 15; LU service flow → 25;
  `get_mem_chunks` semantics → 05 (call-site row only).
- Prerequisites: 00, 99.
- Post-dependents: 02, 03, 04, 05, 15, 25.
- Ground truth: `main.c:79-109,196-520,592-768`; `utility.c:get_mem_chunks`;
  `alloc.c:mem_init/mem_add_total_pages`; `arch/earm/vm.lds`; `Makefile`;
  `os/servers/vm/src/main.rs,boot.rs,global.rs,vm_server.rs:init`.
- Knowledge list:
  | K-007 | init chain | mechanism | B-2..B-15 narrative | STOCK 01 |
  | K-008 | SEF lifecycle | mechanism | fresh/LU/signal/state | STOCK 01 §2.4 |
  | K-009 | ELF loading | mechanism | exec + libexec alloc/copy | STOCK 01 |
  | K-078 | image+handoff | interface | boot-image table + kerninfo fields | NEW glo.h + main.c |
  | K-077 | link/build | tool | vm.lds/Makefiles WONTFIX row | NEW vm.lds |
- Acceptance: reader can place every B-step on the init timeline with anchor;
  heap-divide (`__minix_init`) stated once; no loop-body text beyond a
  5-line pointer to 15.

### 02-vmproc-struct

- Positioning: answers "what does VM remember about ONE process".
- Teaches: K-011, K-012, K-013, K-083 (single-thread premise pointer).
- Does-not-teach: table/search/verify → 03; ACL → 04; regions → 13; exit
  release → 22.
- Prerequisites: 00, 01, 99.
- Post-dependents: 03, 04, 18, 22.
- Ground truth: `vmproc.h:14`; `main.c:262-285,458-462,498-520`;
  `exit.c:25-107`; `os/servers/vm/src/vmproc/vmproc.rs,flags.rs,vmproc_handle.rs`.
- Knowledge list:
  | K-011 | vmproc fields | data | full-field table | STOCK 02 |
  | K-012 | flags+machine | mechanism | orthogonal bits, not enum | STOCK 02 |
  | K-013 | init_proc | mechanism | slot activation | STOCK 02 + 01 site |
- Acceptance: every `vmproc.h` field has a row (purpose + writer + reader);
  lifecycle diagram has no missing transition named in `exit.c`.

### 03-vmproc-table

- Positioning: answers "how does VM remember ALL processes and authenticate senders".
- Teaches: K-014, K-015, K-016.
- Does-not-teach: field details → 02; ACL → 04; fork flow → 18; LU → 25.
- Prerequisites: 00, 01, 02, 99.
- Post-dependents: 15, 18, 22, 26.
- Ground truth: `glo.h:20`; `utility.c:84-101,186-219`; `main.c:128-130,457-462`;
  `endpoint.h`; `os/servers/vm/src/vmproc/table.rs`.
- Knowledge list:
  | K-014 | table+alloc | mechanism | init/search/iterate | STOCK 03 |
  | K-015 | endpoint auth | mechanism | translation + generation | STOCK 03 |
  | K-016 | EXECTMP | constraint | honest transient role | STOCK 03 |
- Acceptance: reader can trace ANY→slot→PCB for each L-4 outcome including
  panic path; generation-wrap reasoning present.

### 04-acl

- Positioning: answers "who may call which VM service, over what lifecycle".
- Teaches: K-017, K-018, K-019.
- Does-not-teach: gate wiring/SUSPEND → 15; fork orchestration → 18;
  exit clearing timing → 22; RS handshake bytes → 25.
- Prerequisites: 01, 02, 03.
- Post-dependents: 15, 18, 25.
- Ground truth: `acl.c` (129, 5 fns); `vmproc.h:vm_acl`; `com.h:769-770`;
  `os/servers/vm/src/acl.rs`; `vmproc_handle.rs` typestate; `rs.rs:NOMMAP`.
- Knowledge list:
  | K-017 | bitmap+layers | mechanism | DEFAULT/SYSTEM | STOCK 04 |
  | K-018 | five ops | mechanism | init/check/set/fork/clear | STOCK 04 |
  | K-019 | fail-closed [A-11] | arch | C allow-all vs Rust deny | STOCK 04 |
- Acceptance: DEFAULT vs SYSTEM decided by table, not prose; None-means-deny
  named as 15 wiring (not re-taught), with pointer.

### 05-physical-memory

- Positioning: answers "how does VM take RAM and hand out runs".
- Teaches: K-021, K-022, K-023, K-024 (definition half), K-025, K-026, K-089 (segment).
- Does-not-teach: self-page consumption → 06; DM internals → 07 (cartoon +
  pointer only); relocation → 10; cache pressure → 24; `vm_allocpage` → 06.
- Prerequisites: 00, 01, 99 (NOT 07; DM appears only as 6-line cartoon).
- Post-dependents: 06, 07, 10, 24, 26.
- Ground truth: `alloc.c` (548); `utility.c:get_mem_chunks`;
  `type.h:memory`; `param.h:MAXMEMMAP`; `vm.h:PAF_*`;
  `os/servers/vm/src/phys_mem/*,boot.rs,global.rs`.
- Knowledge list:
  | K-021 | inventory | mechanism | memmap→chunks→clicks | STOCK 05 |
  | K-022 | alloc/free runs | mechanism | variable runs | STOCK 05 |
  | K-023 | triple backend [A-5] | arch | trait + parity tests | STOCK 05 |
  | K-025 | accounting | tool | memstats/usedpages | STOCK 05 |
- Acceptance: triple-backend parity claim cites `allocator_tests.rs`; reserve
  text does not re-teach 06 consumption.

### 06-page-allocator

- Positioning: answers "how does VM allocate pages for itself without circular dependency".
- Teaches: K-027, K-028, K-029, K-024 (consumption half).
- Does-not-teach: backend internals → 05; DM internals → 07; heap use → 09;
  cache reclaim → 24.
- Prerequisites: 01, 05, 99 (NOT 07 normatively; forward pointer only).
- Post-dependents: 07, 08, 09.
- Ground truth: `pagetable.c:235-440,1311-1345`; `alloc.c:reservedqueue_*,alloc_cycle`;
  `main.c:118-119`; `os/servers/vm/src/alloc_page.rs,global.rs,direct_map.rs`.
- Knowledge list:
  | K-027 | self alloc | mechanism | four self fns | STOCK 06 |
  | K-028 | spare elimination [A-1] | arch | circular-dep argument | STOCK 06 §3.3 |
  | K-029 | lock/validate/count | mechanism | pagelock/addrok/self-pages | STOCK 06 |
- Acceptance: dual-address problem stated before solution; deleted chain
  (Fix #70) named once with pointer, not re-litigated.

### 07-pagetable-struct

- Positioning: answers "what a table looks like and how VM reaches any page".
- Teaches: K-030, K-031, K-032, K-033, K-034.
- Does-not-teach: table verbs → 08; allocation → 06; backend → 05.
- Prerequisites: 01, 05, 06.
- Post-dependents: 08, 10, 13, 18.
- Ground truth: `pt.h`; `arch/i386+earm/pagetable.h`; `pagetable.c:pt_init,bind,mapkernel`;
  `os/arch/src/arch/{paging,direct_map,dm_coverage}.rs`; `direct_map.rs`;
  `pagetable/vm_self_map.rs`; `os/kernel/src/dm_coverage.rs`.
- Knowledge list:
  | K-030 | hierarchy [A-2] | data | 2 vs 4 levels, u32 vs u64 PTE | STOCK 07 |
  | K-031 | Direct Map [A-1] | mechanism | dual views, offset math (MAIN) | STOCK 07 |
  | K-032 | self-map [A-9] | mechanism | adoption model | STOCK 07 |
  | K-033 | width [A-6] | constraint | 32 vs 64 ranges | STOCK 07/20 |
  | K-034 | arch trait [A-10] | arch | three-arch abstraction | STOCK 07 |
- Acceptance: offset translation worked once numerically; C-absence claim
  cites never-defined guard.

### 08-pagetable-ops

- Positioning: answers "how tables are created, changed, copied, queried, destroyed".
- Teaches: K-035, K-036, K-037, K-038, K-039, K-054 (hardware half), K-086.
- Does-not-teach: table shape → 07; service flows → 16/18/20/21/22/25.
- Prerequisites: 06, 07, 01.
- Post-dependents: 16, 18, 20, 21, 22.
- Ground truth: `pagetable.c:494-1500` (14 named ops); `vm.h:WMF_*`;
  `os/arch/src/arch/paging.rs` (+`clone_range,map_kernel`);
  `os/arch/src/x86_64/paging.rs:write_pte_dm`; `vmproc_handle.rs`;
  `vm_self_map.rs`.
- Knowledge list: K-035..K-039 + K-054-hw + K-086 (one row each; no new prose here).
- Acceptance: all 14 C ops have rows (implement / inline / structurally
  eliminated with reason); invlpg-after-write stated once with anchor.

### 09-slab-allocator

- Positioning: answers "how VM gets a heap; why Rust does not port slab".
- Teaches: K-040, K-041 (summary weight).
- Does-not-teach: page supply → 06; table access → 07/08; relocation → 10;
  usage stats → 26.
- Prerequisites: 01, 06, 07, 08.
- Post-dependents: 10.
- Ground truth: `slaballoc.c` (528); `proto.h:SLABALLOC`; `vm.h:MEMPROTECT,VMP_SLAB`;
  `os/servers/vm/src/heap_arena.rs,global.rs:VmAllocator`;
  `pagetable/vm_self_map.rs`; `direct_map.rs:HEAP_*`.
- Knowledge list:
  | K-040 | HeapArena [A-3] | arch | slab→arena decision + free-list v2 | STOCK 09 |
  | K-041 | slab surface | mechanism | SUMMARY table (not full internals) | STOCK 09 slimmed |
- Acceptance: C slab internals occupy at most one summary table; decision
  section carries allocator-choice comparison (Linux/Redox/bump/free-list).

### 10-vm-relocation

- Positioning: answers "how VM ends bootstrap: temp→steady + LU primitives".
- Teaches: K-042, K-043 (primitives; flow → 25).
- Does-not-teach: backend → 05; table verbs → 08; heap → 09; RS flow → 25.
- Prerequisites: 05, 06, 07, 08, 09.
- Post-dependents: 25, 16.
- Ground truth: `pagetable.c:59-110,328,1311-1345`; `alloc.c:reservedqueue_*`;
  `utility.c:188,228,283,312`; `region.c:1535`; `main.c:118-119`;
  `os/servers/vm/src/vm_server.rs:relocate`; `global.rs:heap_arena_grow`;
  `phys_mem/*`; `vmproc/table.rs:swap_slots`; `rs.rs`.
- Knowledge list:
  | K-042 | static→dynamic | mechanism | spare replacement, metadata move | STOCK 10 |
  | K-043 | LU primitives | mechanism | swap/transfer/setparent (flow→25) | STOCK 10 |
- Acceptance: two halves labelled in-doc; LU flow never re-taught (pointer
  to 25); boot hook (`alloc_cycle`) named once.

### 11-phys-pagestate

- Positioning: answers "what state each physical page is in and who shares it".
- Teaches: K-044, K-045, K-046 (definitions; split consumption → 17).
- Does-not-teach: slot mounting → 13; callbacks → 12; split mechanics → 17;
  cache holding → 24.
- Prerequisites: 05, 10 (NOT 13 normatively; 5-line region cartoon inside).
- Post-dependents: 12, 13, 17, 24.
- Ground truth: `pb.c` (168); `region.h:23`; `phys_region.h:8`;
  `region.c:60-72,168-250`; `os/servers/vm/src/region/page_state.rs`;
  `sanity.rs:verify_refcounts`.
- Knowledge list: K-044/045/046 rows.
- Acceptance: C two-layer vs PFN-array mapping table complete; refcount
  ownership transfer named once.

### 12-memtype

- Positioning: answers "what each memory kind promises at each hook".
- Teaches: K-047, K-048.
- Does-not-teach: framework call sites → 13; fault drive → 16; split → 17;
  file/cache depth → 23/24.
- Prerequisites: 05, 11 (NOT 13 normatively; cartoon only).
- Post-dependents: 13, 16, 17, 23, 24.
- Ground truth: `memtype.h`; `mem_anon.c`, `mem_anon_contig.c`,
  `mem_directphys.c`, `mem_shared.c`, `mem_cache.c`, `mem_file.c`;
  `os/servers/vm/src/memtype.rs` (1366); `region/page_state.rs:memtype mount`.
- Knowledge list: K-047 + six K-048 rows (one per type).
- Acceptance: 6×15 callback matrix present; default-vs-override made explicit;
  Fix #64落点 (deletion-funnel, not ev_delete) stated once.

### 13-region-mapping

- Positioning: answers "when the framework calls which region operation".
- Teaches: K-049, K-087, K-088 (pin segment), O-4 boundary.
- Does-not-teach: index search → 14; fault drive → 16; split → 17;
  fork/munmap/exit/query flows → 18/21/22/26; LU → 25.
- Prerequisites: 11, 12.
- Post-dependents: 14, 16, 18, 19, 20, 21, 22, 26.
- Ground truth: `region.c` (1555); `region.h:37-79`; `phys_region.h`;
  `os/servers/vm/src/region/{region_map.rs,vir_region.rs,mod.rs}`.
- Knowledge list: K-049 + K-087 + pin row.
- Acceptance: 22 framework ops each have callers listed; `map_free_proc`
  row points to 22 for full release.

### 14-region-lookup

- Positioning: answers "how address→region and free-gap queries stay O(log n)".
- Teaches: K-050.
- Does-not-teach: region lifecycle → 13.
- Prerequisites: 13.
- Post-dependents: 16, 19, 20.
- Ground truth: `regionavl.c`; `cavl_*.h`, `unavl.h`, `regionavl.h/defs.h`;
  `os/servers/vm/src/region/region_map.rs:find_slot et al.`
- Knowledge list: single K-050 row expanded to five-way search + iterator.
- Acceptance: AVL→BTreeMap equivalence argued once with complexity + test pointer.

### 15-ipc-dispatch

- Positioning: answers "how the heart beats: receive, prioritise, gate, handle, reply".
- Teaches: K-004, K-005, K-006, K-020, K-079 (boundary table), K-081 (matrix),
  K-079-notify/transid rows.
- Does-not-teach: handler bodies → 16-26; ACL structure → 04; VFS queue → 23;
  SEF birth → 01.
- Prerequisites: 01 plus 02-14 ready (explicit list, all earlier).
- Post-dependents: 16-26.
- Ground truth: `main.c:112-192,520-580`; `com.h:VM_RQ_BASE/NR_VM_CALLS/SUSPEND`;
  `vfsif.h:TRNS_*`; `ipcconst.h`; `os/servers/vm/src/ipc/dispatcher.rs`;
  `ipc/transport.rs`; `ipc/encode.rs`; `ipc/cache_handlers.rs` (moved);
  `vm_server.rs:run/run_once/dispatch_on_msg/reply paths`.
- Knowledge list: K-004/005/006/020 + K-079 + K-081-matrix.
- Acceptance: five priorities walkable with anchors; CALLMAP 26/26 + 4
  parity-ENOSYS table matches `build_callmap` guard test; SUSPEND vs errno
  confusion impossible after reading.

### 16-pagefault

- Positioning: answers "how faults are arbitrated: validate, lookup, check, dispatch, resume".
- Teaches: K-051, K-052, K-053, K-054 (invariant home), K-056 (consumer side).
- Does-not-teach: split mechanics → 17; queue/fdref → 23; cache → 24;
  mmap build → 20.
- Prerequisites: 13, 15, 12.
- Post-dependents: 17, 18.
- Ground truth: `pagefaults.c` (418); `region.c:map_pf/handle_memory/lookup`;
  `main.c:P3 + SIGKMEM`; `os/servers/vm/src/cow_exec_pf.rs`;
  `vm_server.rs:dispatch_pagefault`; `fork.rs:handle_memory_once`;
  `memtype.rs:PagefaultResult`; `minix-types ipc/vm.rs:VM_PAGEFAULT`.
- Knowledge list: K-051/052/053/054 rows.
- Acceptance: both entries traced to `map_pf` confluence; TLB invariant
  table lists every path's quiescence mechanism; forged-source observability
  (audit vs printf) stated.

### 17-cow-mechanism

- Positioning: answers "how sharing is established, protected, and split".
- Teaches: K-055, K-056 (gate home), K-046 (consumption).
- Does-not-teach: fault drive → 16; fork orchestration → 18; file VFS → 23.
- Prerequisites: 11, 12, 16.
- Post-dependents: 18.
- Ground truth: `pb.c:136-168`; `mem_anon.c:33-113`; `region.c:pr_writable,
  map_ph_writept,map_writept,map_copy_region`; `mem_file.c:cow_block,writable`;
  `mem_shared.c`; `os/servers/vm/src/cow_exec_pf.rs:cow_resolve_core`;
  `region/{vir_region,page_state}.rs`; `vmproc_handle.rs`.
- Knowledge list: K-055 + K-056-gate + clearend boundary pointer.
- Acceptance: fast-path conjunction (`exclusive AND writable`) + mappedfile
  retype-to-anon both Produces PTE-RW assertions in tests.

### 18-vm-fork

- Positioning: answers "how one service call clones an address space safely".
  Secondary trunk of the stage.
- Teaches: K-057.
- Does-not-teach: split internals → 17; fault drive → 16; fdref → 23;
  exit inverse → 22; queries → 26.
- Prerequisites: 02, 03, 04, 06, 07, 08, 11, 12, 13, 15, 17.
- Post-dependents: 22.
- Ground truth: `fork.c:32-115`; `region.c:map_proc_copy(_range)`;
  `acl.c:acl_fork`; `utility.c:vm_isokendpt`; `kernel/system/do_fork.c`;
  `com.h:VMF_*`; `os/servers/vm/src/fork.rs`; `vmproc_handle.rs`;
  `vmproc/table.rs`; `dispatcher.rs:dispatch_fork`; `minix-types vm.rs fork`.
- Knowledge list: single K-057 expanded to phase+rollback table.
- Acceptance: secondary-trunk path diagram present; every phase has failure
  rollback stated; eager-CoW gating (msgaddr) points to E1/E2 edge.

### 19-vm-brk

- Positioning: answers "how the heap top moves up and down".
- Teaches: K-058, K-059.
- Does-not-teach: generic mapping → 20; libc malloc → out of stage;
  stack growth → not handled (explicit).
- Prerequisites: 13, 14, 15, 05.
- Post-dependents: 20, 22.
- Ground truth: `break.c:44-69`; `region.c:map_region_extend_upto_v`;
  `mem_anon.c:anon_resize`; `libc/sys/brk.c`; `com.h:VM_BRK`;
  `ipc.h:mess_lc_vm_brk`; `os/servers/vm/src/brk.rs`;
  `dispatcher.rs:dispatch_brk`; `minix-types vm.rs brk`.
- Knowledge list: K-058 + K-059 (shrink + A-12 rows).
- Acceptance: three-state orchestration diagram; C-vs-Rust shrink divergence
  tabled with errno consequences.

### 20-vm-mmap

- Positioning: answers "how new intervals are addressed AND what backs them".
  SOLE owner of MAP_PHYS establishment after fix.
- Teaches: K-060, K-061, K-062 (establishment home).
- Does-not-teach: teardown → 21; cache internals → 24; queue → 23;
  phys/ref readout → 26.
- Prerequisites: 12, 13, 14, 15, 19.
- Post-dependents: 21, 23, 24, 26.
- Ground truth: `mmap.c:36-435`; `region.c:find_slot*,map_page_region`;
  `mem_file.c:setfile`; `mem_shared.c:setsource`; `vfs.c`;
  `libc/sys/mmap.c`; `mman.h`; `ipc.h:mess_mmap family`; `com.h` mmap codes;
  `os/servers/vm/src/mmap.rs,map_phys.rs`; `dispatcher.rs` mmap arms;
  `vfs_queue.rs:FdLookup`; `minix-types vm.rs mmap family`.
- Knowledge list: K-060/061/062 rows.
- Acceptance: three-way resolution + four bindings + privilege tiers each
  have a worked example; file-async path points to 23 without re-teaching.

### 21-vm-munmap

- Positioning: answers "how mappings are dismantled and resources returned".
  Teardown ONLY (establishment migrated to 20).
- Teaches: K-063, K-064.
- Does-not-teach: establishment → 20; full-process release → 22;
  fdref/VFS depth → 23; cache → 24; queries → 26.
- Prerequisites: 13, 15, 20, 12.
- Post-dependents: 22.
- Ground truth: `mmap.c:488-573`; `region.c:map_subfree,map_free,map_lookup,
  map_unmap_region,split_region,map_unmap_range`; `mem_directphys.c`;
  `pb.c:pb_unreferenced`; `mem_file.c:delete`; `fdref.c:deref`;
  `main.c:CALLMAP`; `ipc.h:shm_unmap/map_phys/unmap_phys`;
  `os/servers/vm/src/munmap.rs,region/mod.rs:free_region_pages,
  vir_region.rs:split/free_range,memtype.rs:DirectPhysical`.
- Knowledge list: K-063 + K-064 rows.
- Acceptance: four range cases + three region cases each mapped to code;
  shared/device/file pages have distinct return paths.

### 22-vm-exit

- Positioning: answers "how processes die and how live ones are controlled".
- Teaches: K-065.
- Does-not-teach: queries → 26; fdref depth → 23; fault/CoW → 16/17;
  fork build → 18; brk/mmap build/teardown → 19/20/21.
- Prerequisites: 03, 13, 15.
- Post-dependents: 25, 26.
- Ground truth: `exit.c` (156); `region.c:map_subfree,map_free,map_free_proc`;
  `pagetable.c:pt_new,pt_bind,pt_free`; `pb.c`; `pagefaults.c:handle_memory_*`;
  `main.c:transid + do_procctl_notrans + CALLMAP`; `acl.c:acl_clear`;
  `glo.h:num_vm_instances`; `com.h:EXIT/WILLEXIT/PROCCTL`;
  `os/servers/vm/src/exit.rs`; `vmproc_handle.rs`; `vmproc/table.rs`;
  `dispatcher.rs:procctl/exit/willexit`; `minix-types vm.rs exit family`.
- Knowledge list: single K-065 expanded to two-phase + PROCCTL table.
- Acceptance: two-phase sequence diagram; CLEAR vs HANDLEMEM distinguished
  with permission checks and errno.

### 23-vfs-interaction

- Positioning: answers "how VM and VFS conduct async file dialogue".
- Teaches: K-066, K-067, K-068.
- Does-not-teach: cache internals → 24; mmap address choice → 20;
  CoW body → 17; loop/SUSPEND → 15.
- Prerequisites: 12, 13, 15, 20.
- Post-dependents: 24.
- Ground truth: `vfs.c` (143); `fdref.c` (177); `mem_file.c` (287);
  `mmap.c:mmap_file*,do_mmap-file-branch`; `pagefaults.c:pf_cont*`;
  `vfs/misc.c:do_vm_call`; `libminixfs/cache.c:PEEK`;
  `com.h:VFS_VMCALL_*`; `os/servers/vm/src/vfs_queue.rs,fdref.rs,mmap.rs(file),
  memtype.rs:MappedFile,cow_exec_pf.rs,page_cache.rs:VMC_NO_INODE`;
  `minix-types message.rs:m_vm_vfs_reply + vm.rs:VmVfsReplyIn`.
- Knowledge list: K-066/067/068 rows.
- Acceptance: three request kinds traced end-to-end with queue states;
  fdref four-state table matches `fdref.c:156-176`.

### 24-page-cache

- Positioning: answers "how disk blocks are directory-indexed, aged, evicted, served".
- Teaches: K-069, K-070, K-071.
- Does-not-teach: queue/fdref → 23; callbacks def → 12; allocator → 05/06;
  loop → 15.
- Prerequisites: 12, 23.
- Post-dependents: none (leaf) except pressure pointer from 05.
- Ground truth: `cache.c` (332); `cache.h`; `mem_cache.c` (324);
  `mem_file.c` consumers; `main.c:CALLMAP`; `alloc.c:cache_freepages call`;
  `libminixfs/cache.c:ONE_SHOT→VMSF_ONCE`;
  `os/servers/vm/src/page_cache.rs`; `ipc/cache_handlers.rs`;
  `memtype.rs:CacheMemory/MappedFile`; `region/page_state.rs:IN_CACHE`;
  `minix-types message.rs:MessVmmcp* + vm.rs:VmCacheIn`.
- Knowledge list: K-069/070/071 rows.
- Acceptance: dual-key model + exact-LRU + refcount-eviction each have an
  invariant stated; four IPC handlers have pre/post tables.

### 25-rs-services

- Positioning: answers "how RS commands VM during live update".
- Teaches: K-072, K-073.
- Does-not-teach: SEF birth → 01; swap primitives → 10; ACL depth → 04;
  loop → 15; queries → 26.
- Prerequisites: 15, 22 (10 primitives referenced, earlier).
- Post-dependents: none (leaf orchestration).
- Ground truth: `rs.c` (391); `main.c:map_service,sef_cb_init_fresh,
  adjust_proc_refs sites`; `utility.c:adjust_proc_refs`; `rs.h:rprocpub`;
  `com.h:RS codes + MEMCTL subs`; `ipc.h:mess_lsys_vm_update`;
  `os/servers/vm/src/rs.rs`; `vm_server.rs:rs_handshake,ipc_call_rs_init,
  RprocEntry/Tab`; `dispatcher.rs:rs arms`.
- Knowledge list: K-072 + K-073-gap rows.
- Acceptance: four services have pin/prepare/update/memctl tables; A-8 gaps
  have fail-closed contracts + edge pointers (E-RSWIRE).

### 26-vm-queries

- Positioning: answers "how outsiders read the ledger without touching it".
- Teaches: K-074, K-075, K-089 (readout half).
- Does-not-teach: ledger lifecycle → 13; cache stats calibre → 24;
  rusage clear timing → 22 (row only); loop → 15; callback defs → 12.
- Prerequisites: 13, 15.
- Post-dependents: none (leaf).
- Ground truth: `utility.c:do_info,do_getrusage`; `mmap.c:do_get_phys,
  do_get_refcount`; `region.c:map_get_phys,map_get_ref,get_usage_info*,
  get_region_info,is_stack_region`; `cache.c:get_stats_info`;
  `com.h` query codes; `vm.h:vm_*_info`; `libsys/vm_info.c`;
  `mem_anon.c:regionid/refcount`; `mem_shared.c`;
  `os/servers/vm/src/query.rs`; `dispatcher.rs` query arms;
  `vm_server.rs:usage_sources/encode branches`; `memtype.rs` supports_*;
  `boot.rs:vm_allocated_bytes`; `minix-types vm.rs:VmRegionInfo/Reply`.
- Knowledge list: K-074 + K-075-naming-trap + consumer table (MIB/procfs/
  VFS-coredump/shm/PM).
- Acceptance: four services have question/consumer/permission rows; REGION
  pagination + GETPHYS naming trap explicitly warned.

---

## 6. Change table (old → new; every op has reason + K refs + destination)

Operation IDs: MV (move section), RB (rebalance weight), OW (single-owner fix),
PR (prerequisite repair), FX (fact fix). No renumber, no new numbers, no archive
of formal docs. `draft/` stays archived (no action).

| Op | Type | Old position | New position | Reason | Knowledge + destination |
|---|---|---|---|---|---|
| C-01 | OW | 20 + 21 both teach `do_map_phys/map_perm_check` establishment | 20 sole establishment; 21 teardown only | Single-semantic: setup vs teardown split across two docs; 21 currently both builds and destroys | K-062 → 20 §§ establishment; 21 keeps K-063/K-064; migrate 21 establishment §§ to 20 |
| C-02 | PR | 05 lists 07 as prerequisite (forward ref); 06 lists 07 (forward) | 05/06 prerequisites drop 07; 6-line DM cartoon + pointer to 07 | Zero-forward-reference without renumber; DM runtime (B-10) postdates 05/06 consumption in teaching | K-031 main 07; cartoon rows in 05/06 contracts |
| C-03 | PR | 11 lists 13 as concept-order prereq (forward) | 11 self-contained 5-line region cartoon; prereq 05,10 only | Same as C-02; preserves numbers, kills forward edge | K-049 cartoon in 11 §1; full in 13 |
| C-04 | PR | 12 implicitly needs region frame (forward to 13) | 12 self-contained cartoon; prereq 05,11 only | Same | K-049 cartoon in 12 §1 |
| C-05 | RB | 09 C slab internals dominate (≈60% of 685 lines) over HeapArena decision | C slab → one summary table; decision → main (comparison + free-list v2) | Weight follows ground truth (Rust deleted slab); reader value is decision, not deleted internals | K-041 slimmed; K-040 expanded in 09 |
| C-06 | FX | 00 §2.1 source map: "memtype policy 10–14, 12" (10 is relocation, not policy) | Correct map: policy 12; ledger 11/13/14; bootstrap 09/10 | Fact error in navigation table | K-001 map; acceptance spot-check |
| C-07 | MV | `get_mem_chunks` semantics live in 01 narrative | Semantics → 05; 01 keeps call-site row | Boundary O-1 | K-021 → 05; pointer in 01 |
| C-08 | MV | Gate/SUSPEND corners inside 04 | Wiring → 15; 04 keeps structure | Boundary O-2/D-6 | K-020 → 15 |
| C-09 | MV | `map_free_proc` full release inside 13 corners | Full release → 22; 13 keeps signature | Boundary O-4 | K-065 → 22 |
| C-10 | FX | 15/16/23 each partially describe kernel↔VM boundary, no unified table | Unified table → 15 §1.6 + 99 rows | Gap G-1; prevents triple partial teaching | K-079 → 15 |
| C-11 | FX | Errno scattered, no matrix; SUSPEND confusable with errno | Matrix → 15 + per-service rows | Gap G-3 | K-081 → 15 + services |
| C-12 | FX | Shutdown/panic, test seams, dead register, boot bytes scattered/thin | Explicit sections → 00/01/15/99 per §3.2 | Gaps G-2,G-4,G-5,G-6,G-7 | K-077..K-085 → owners |
| C-13 | RB | 10 two halves unlabeled (bootstrap vs LU) | Label halves in-contract; flow stays in 25 | Prevents 10/25 re-teaching (D-8) | K-042 vs K-043 split statement |
| C-14 | RB | 23/24 re-define file/cache callbacks already in 12 | Consumption-only rule for 23/24 | Duplication D-2 | K-048 defs stay in 12 |

Rejected ops (documented, not executed): renumber 11-14 (cost §8);
split 01/08/10/26 (cost §4.3); new wire/build/TLB/test docs (cost §4.3).

---

## 7. Missing-chapter disposition (no TBD left; no new numbers)

Each §3.2 gap lands as follows with原料 (source excerpts) and acceptance:

- G-1 boundary table → 15 new §1.6 (原料: `main.c:123-164`, `pagefaults.c`
  headers, `vfsif.h:79-81`, `ipcconst.h:28,34`; acceptance: direction table
  kernel→VM vs VM→kernel with message names).
- G-2 boot bytes → 01 explicit table (原料: `glo.h:25`, `main.c:442-520`,
  `minix/type.h:memory`; acceptance: every `kernel_boot_info` field consumed
  somewhere in B-3..B-13).
- G-3 errno matrix → 15 matrix + service rows (原料: `main.c:137-190`,
  dispatcher error maps; acceptance: SUSPEND absent from errno columns).
- G-4 fail-stop → 00 §1 + 15 limit + 22 (原料: panics, `panic=abort`,
  `vm_server.rs:991,1069`; acceptance: "VM never exits" + two fail modes
  distinguished).
- G-5 seams → 00 §4.1 + §5 rows (原料: gateway, sim, parity tests, CALLMAP
  guard; acceptance: each seam names production vs test type).
- G-6 dead register → 99 appendix (原料: plan §5.4, `proto.h`, `sanitycheck.h`,
  DMA/EXEC codes; acceptance: one row per dead item with reason class).
- G-7 link/build → 01 WONTFIX row (原料: `vm.lds`, Makefiles; acceptance:
  non-semantic ruling with "build-only" reason).

---

## 8. Anchor migration and breakage cost

### 8.1 Section migration table (only moved sections listed; all else rewrites in place)

| Old location (doc + section) | Old content (one line) | New location | Migration type | Breakage note |
|---|---|---|---|---|
| 21 §§ `do_map_phys` + `map_perm_check` establishment | Device-memory setup + permission tiers | 20 §§ establishment (K-062) | move | 21 inbound refs to those §§ must retarget to 20; code anchors unchanged |
| 21 `map_phys.rs:handle_map_phys` wire/error rows | Establishment wire format | 20 wire rows | move | dispatcher anchors stay; doc pointer moves |
| 01 §§ `get_mem_chunks` semantics | Chunk-list construction + click rounding | 05 §§ inventory (K-021) | move | 01 keeps call-site row pointing to 05 |
| 04 §§ gate wiring corners | `acl_check` in-loop wiring, None-deny | 15 §§ gate (K-020) | move | 04 keeps structure + pointer |
| 13 §§ `map_free_proc` full narrative | Whole-process release walk | 22 §§ release (K-065) | move | 13 keeps signature + pointer |
| 09 §§ slab internals (GETSLAB/ADDHEAD/lists/stats depth) | Deleted-allocator mechanics | 09 appendix summary (K-041) | rewrite-shrink | No inbound deep links observed beyond §-level; retarget to summary |
| 05/06/11/12 intros referencing later docs | Forward-prereq prose | Self-contained cartoons + pointers (C-02..C-04) | rewrite | Prereq fields change; body anchors unchanged |
| 00 §2.1 source-map rows | File→doc map with 10–14 error | Corrected map (C-06) | rewrite-fix | Inbound map citations update |
| 15 (new §1.6), 01 (image table), 99 (appendix), 00 (§1/§4.1) | Scattered/thin non-C notes | Consolidated per §7 | rewrite-expand | New anchors point to non-C paths |

### 8.2 Reference migration table (old reference → new target + check)

- Doc→doc cross-refs: full-text `rg` for `NN-*.md` citations required at B-phase
  start. Hot files by current self-count: 01 (12), 20 (11), 06 (11), 21 (10),
  13/14/15/04 (10 each). Every moved row above generates 2-5 retargets;
  mechanical check: `rg -n "21-vm-munmap|20-vm-mmap|04-acl|15-ipc-dispatch|
  13-region-mapping|05-physical-memory|01-vm-init-main" rewrite-notes/
  fork-syscall-rewrite/02-stage-vm/*.md` must show zero stale section numbers
  after B-phase.
- Kernel-side refs: `01-stage-kernel` prose references to VM docs (plan §3.3
  rule: `../01-stage-kernel/NN-*.md` both directions). Check: `rg -n
  "02-stage-vm" rewrite-notes/01-stage-kernel/*.md` and
  retarget any moved-section citations (only C-01/C-07..C-09 produce moves).
- Code-comment refs: `rg -n "02-stage-vm|15-ipc-dispatch|05-physical-memory|
  22-vm-exit|plan\.md|todo\.md" os/servers/vm/src` enumerated the live set
  (§0.3 sample: vm_server.rs, main.rs, exit.rs, boot.rs, region/mod.rs,
  phys_mem/*, pagetable/mod.rs). Rule: code anchors point to doc numbers,
  never to section numbers, so C-01..C-09 moves do not break code comments
  except the 21→20 establishment pointer (one `map_phys` comment family in
  `map_phys.rs`/`dispatcher.rs` doc-comments must be re-pointed and verified
  by `rg` re-run).

### 8.3 Breakage-cost summary

- Affected references total (conjecture, to be verified by B-phase `rg`):
  on the order of 100-150 doc→doc hits (28 docs × ~10 self-refs, many
  self-local) plus ~15-25 code-comment hits. Hotspots: 01, 20/21 pair,
  06/07 pair, 13 hub.
- No renumber means no filename changes: batch modification is section-level
  retargeting via `sed` on doc bodies + one `rg` verification pass, not a
  filename migration. Suggested batch: apply C-01 first (only cross-doc move
  with inbound refs), verify with §8.2 queries, then C-07..C-09, then
  prerequisite-field pass (C-02..C-04), then §7 expansions.
- Renumber alternative cost (rejected): 28 filename changes × ~10 inbound
  refs each + code-comment filename refs + kernel-side refs ≈ 300+ edits
  with high misfire risk, for pedagogy gains achievable by cartoons. Hence
  rejected in §4.

---

## 9. Verification, self-check gates, conclusion

### 9.1 Four mechanical checks (results on this blueprint)

1. Forward-reference scan: every contract's Prerequisites list contains only
   earlier numbers (00<01<02...<99 placed first pedagogically but numbered
   99; 99 is vocabulary, allowed as prereq for all). The two known runtime
   inversions (DM use before 07; ledger use before 13) are handled by
   in-doc cartoons, not prerequisite edges → PASS (zero forward edges).
2. Dependency graph: nodes 00,99,01-26 with edges from §5 Prerequisites.
   Graph is acyclic (edges strictly increase except 99→all which is a source;
   25 lists 22 then 15: both earlier; 18 lists 17 earlier). No cycles → PASS.
   Pedagogical branch 13→14→11→12 is declared alternate traversal, not an edge.
3. Coverage: all 90 K items have owners in §5 (STOCK destinations or NEW
   evidence anchors); deleted items listed once (K-085/99 appendix) → 100% →
   PASS. C-file check: 24/24 `.c` have owners via §5 ground-truth rows.
4. Breakage accounting: moved sections enumerated (§8.1, 9 rows); reference
   families enumerated with verification queries (§8.2); totals estimated
   with hotspots → PASS (complete; exact counts verified at B-phase `rg`).

### 9.2 Self-check gates G1-G9

| Gate | Check | Result |
|---|---|---|
| G1 | C true order checkable (10 random条目 re-checked: B-3/B-6/B-10/B-14/L-4/L-5/L-7/L-8 + fork/exit lifecycles) | PASS (anchors in §1) |
| G2 | Pool completeness: every C file + non-C artifact has owner or explicit exclusion with reason | PASS (§5 ground truth + §3.4 WONTFIX rows for vm.lds/Makefiles/sanity/dead) |
| G3 | Zero forward references in new prereqs | PASS (§9.1.1) |
| G4 | Acyclic graph or split plan for cycles | PASS, acyclic (§9.1.2) |
| G5 | 100% K coverage; NEW items have evidence; deletions listed separately | PASS (§9.1.3; deletions in 99 appendix) |
| G6 | Every split/merge has STOCK destinations; every NEW has source (10 spot checks: C-01..C-09 + G-1..G-7) | PASS (§6 + §7) |
| G7 | Seven contract elements per doc (28/28) | PASS (§5) |
| G8 | Migration covers every changed doc section + doc/code refs | PASS (§8; unchanged sections rewrite in place, no migration needed) |
| G9 | Factual claims carry anchors (10 spot checks: B-table rows, CALLMAP 26, NR 49, file sizes, `vm.lds` existence, `vm_isokendpt` panic, SUSPEND shape, Direct Map absence, slab 528, regionavl 11+1426) | PASS; two estimates marked conjecture (§8.3 totals, §2 dup counts) |

### 9.3 Conclusion and questions for decision

Conclusion: the existing 28-number scheme is retained with targeted surgery,
not wholesale renumbering. B-phase can proceed doc by doc from §5 contracts
without further trade-off decisions, except the questions below. Total new
material is bounded: one moved ownership (MAP_PHYS), four prerequisite
repairs, two weight rebalances, seven non-C consolidations, one map correction.

Questions requiring user ruling (no work blocked except where noted):

1. MAP_PHYS single-owner (C-01): confirm 20 as establishment home (vs keeping
   a duplicate summary in 21). B-phase 21 rewrite blocked until decided
   (recommendation: 20, per single-semantic rule).
2. Pedagogical branch 13→14→11→12: confirm declaring it explicitly in 10/11
   transition text (recommendation: yes; zero-cost pedagogy win).
3. 09 slab weight: confirm shrinking C internals to one summary table
   (recommendation: yes; aligns doc weight with Rust ground truth).
4. Dead-register home: confirm 99 appendix as single home (vs plan §5.4
   remaining canonical; recommendation: 99 appendix + plan pointer).

---

*End of blueprint. B-phase entry: start at C-01 (MAP_PHYS move) + §8.2
verification queries, then execute §5 contracts in numeric order.*
