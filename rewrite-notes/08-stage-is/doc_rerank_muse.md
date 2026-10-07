# 08-stage-is Document Rebuild Blueprint (muse)

> `your_name(AI agent name) = muse`
> `target_dir = notes/rewrite/fork-syscall-rewrite/08-stage-is`
> `repo_root = /home/xzhao/github/minix-rs`
> Task = R-phase rebuild blueprint: produce `target_dir/doc_rerank_muse.md`; modify no body text.
> Constraint = no citation of `.design/` or `tmp_design_and_todo/`; every landed artifact carries the `_muse` suffix; no reading or copying of other AIs' `doc_rerank_*` products.
> Language = English (per muse special requirement). All section headings and prose below are in English; identifiers, paths, and quoted C/Rust symbols keep their original form.

> How to read this blueprint (standalone): Section 0 fixes scope and evidence. Section 1 rebuilds the
> runtime truth directly from C source (the fact base for everything after). Section 2 pools every
> knowledge item the stage must teach. Section 3 audits coverage against the C symbols, OS concepts,
> non-C artifacts, and stage boundaries. Section 4 designs the new catalog. Section 5 contracts every
> new document (the B-phase work orders). Section 6 lists every change. Section 7 assigns every gap a
> home. Section 8 prices the breakage. Section 9 mechanically verifies the blueprint itself.
> Core verdict up front: **the 12-number catalog (00, 01–10, 99) is kept unchanged; every document is
> rebuilt in place from the pool + C source + current Rust truth; `draft/` is archived; no number is
> added, removed, merged, or split.** Reasons are in Section 4.5.

## 0. Metadata

- Executor: `muse`. Date: 2026-09-20 (UTC). Target: `notes/rewrite/fork-syscall-rewrite/08-stage-is/`.
  Repo HEAD at generation time: `e67086298` (verified with `git rev-parse --short HEAD`; an earlier
  probe in the same session returned `41000abfc`, so B-phase must re-verify HEAD before writing).
- Task type: service event-loop stage (IS = Information Server, the debug-dump aggregator user-space
  server). R-phase output only; no body text was modified to produce this file.
- Scope — counted as documents (rebuilt in place, 12): `00-is-overview.md`, `01-is-init-main.md`,
  `02-is-fkey-contract.md`, `03-is-dump-dispatch.md`, `04-is-data-acquisition.md`,
  `05-is-dump-kernel.md`, `06-is-dump-pm.md`, `07-is-dump-vfs.md`, `08-is-dump-rs.md`,
  `09-is-dump-ds.md`, `10-is-dump-vm.md`, `99-is-global-concepts.md`.
- Scope — reference material (read, not rebuilt): `plan.md` (340 lines, organisation + ARCH table +
  coverage contract, partly stale — see G-03/G-07/G-08), `todo.md` (271 lines, V1 architecture-review
  ledger with 8 closed fixes, current),   `draft/` (9 files: placeholder README + 8 `tmp_*.md`
  line-by-line source material, archived by this blueprint, never cited as authority).
- Scope — out of scope: `.design/`, `tmp_design_and_todo/`, any other AI's `doc_rerank_*` product
  (none was read; `ls` confirmed three exist, contents untouched), all other stage directories except
  the previous stage overview (`07-stage-ds/00-ds-overview.md`, read for boundary only) and the master
  plan README (read for stage position only).
- Reading list actually consumed:
  1. All 12 numbered documents (headers fully, bodies skimmed for knowledge extraction; line counts:
     00:60, 01:707, 02:572, 03:360, 04:492, 05:323, 06:191, 07:177, 08:153, 09:174, 10:208, 99:58).
  2. All Minix3 C sources of the stage: `minix3/minix/servers/is/` — `main.c` 148, `dmp.c` 132,
     `dmp_kernel.c` 396, `dmp_pm.c` 109, `dmp_fs.c` 83, `dmp_rs.c` 74, `dmp_ds.c` 52, `dmp_vm.c` 157
     (1151 lines of `.c`), plus `inc.h` 32, `glo.h` 16, `proto.h` 34, `Makefile` 21.
  3. Protocol and peer sources: `minix3/minix/include/minix/com.h` (is_notify, TTY_FKEY_CONTROL,
     FKEY_*, GET_*, DIAGCTL_CODE_*, VM_INFO/VMIW_*), `ipc.h` (fkey message structs),
     `keymap.h` (F1–F12/SF1–SF12), `sysinfo.h` (SI_*), `sysutil.h`/`syslib.h` (client macros),
     `minix3/minix/drivers/tty/tty/arch/i386/keyboard.c` (TTY peer: `do_fkey_ctl`, `func_key`),
     `minix3/minix/lib/libsys/` clients (`fkey_ctl.c`, `getsysinfo.c`, `vm_info.c`,
     `sys_diagctl.c`), boot evidence (`minix3/minix/kernel/table.c:44-64`,
     `minix3/etc/rc.minix:117`, `minix3/etc/system.conf:271-277`).
  4. Rust implementation entry: `os/servers/is/src/` — 13 files, 6266 lines total
     (`acquire.rs` 1523, `dispatch.rs` 419, `dump_ds.rs` 277, `dump_kernel.rs` 963,
     `dump_pm.rs` 346, `dump_rs.rs` 202, `dump_vfs.rs` 375, `dump_vm.rs` 483, `lib.rs` 747,
     `main.rs` 35, `sef.rs` 368, `state.rs` 51, `tty_fkey.rs` 477).
  5. Boundary materials: `notes/rewrite/fork-syscall-rewrite/00-master-plan/README.md` (stage order and
     two-level boot semantics), `notes/rewrite/fork-syscall-rewrite/edge_todo.md` (E-ISWIRE, E-ISPROD,
     E-ISKMESS, E-ISBOOT pointers), `01-stage-kernel/06-todo.md` (contract-writing style example only —
     its outline-per-document task-book form is imitated, none of its content is reused).
- Commands executed and key outputs (evidence excerpts):
  - `wc -l notes/rewrite/fork-syscall-rewrite/08-stage-is/*.md` → 12 docs, 3475 lines of numbered-doc
    body (excl. plan/todo/rerank products); thinnest 99:58 and 00:60, densest 01:707.
  - `wc -l minix3/minix/servers/is/*` → 1254 lines total, 1151 in `.c` (matches plan §1 claim).
  - `rg -n "IS_PROC_NR" minix3/minix` → zero hits (A-9 holds: no fixed endpoint).
  - `rg -n "GET_KMESSAGES" minix3/minix` → zero hits in C (A-3 premise holds on the C side; the Rust
    side has since added `GET_KMESSAGES=7` — see G-08, a B-phase doc-sync item, not a contradiction).
  - `rg -n "click_to_round_k" minix3/minix/servers/is/` → definition line only (dead code, excluded).
  - `rg -n "diag_buf" minix3/minix` → declaration in `glo.h` only (dead extern, excluded).
  - `sed -n '44,64p' minix3/minix/kernel/table.c` → 17 boot_image entries, no `is` (IS is not a boot
    member; RS loads it at runtime).
  - `sed -n '110,125p' minix3/etc/rc.minix` → `up -n is -period 5HZ` gated on `sysenv debug_fkeys != 0`.
  - `sed -n '265,285p' minix3/etc/system.conf` → `service is { vm INFO; uid 0; }`.
  - `rg -n "covered in|see .*\.md|08-stage-is|is-init|is-dump" os/servers/is/src/` → ~17 code-comment
    doc references (migration load, see Section 8).
  - `cargo test -p minix-is` (run in `os/servers/is/`) → **125 passed / 0 failed** at blueprint time.
    Every existing document §5 claims 86; `todo.md` cites 106→110 across fixes. All three numbers are
    stale — G-03 mandates a per-document test-count refresh in B-phase, re-verified at write time.

## 1. C True Order (rebuilt from source, not paraphrased from existing docs)

- Stage-type verdict: **service event-loop type**. IS has a conditional birth segment (it may never
  exist) followed by an infinite notify-classify-dispatch loop. It is not a linear startup chain
  (nothing runs after it), not a syscall set, not a driver/commands collection. Organisation follows
  the event-loop rule: why the service exists → birth and initialisation → message interface →
  acquisition framework → per-scenario dump handling → neighbour protocols and global close.
- Fact base for the order-difference table (Section 4.6): teaching order equals runtime order except
  two deliberate, compensated inversions (O-1: the TTY peer contract is taught in 02 before its first
  runtime use is fully understood; O-2: the five acquisition channels are taught as one framework in
  04 before any dump uses them, while at runtime each dump fetches lazily).

### Table T1 — Conditional birth chain (may not run at all)

| # | Action | C anchor | Note |
|---|--------|----------|------|
| T1-1 | Operator enables `debug_fkeys`; otherwise IS never exists | `minix3/etc/rc.minix:117` (`if [ "$(sysenv debug_fkeys)" != 0 ]`), `keyboard.c:78,206,406` (`debug_fkeys` switch on the TTY side) | Conditional existence is the single most misunderstood fact of this stage; new 00 owns it. |
| T1-2 | `up -n is -period 5HZ`: RS loads the IS ELF, assigns a dynamic endpoint (`-n` = NOBLOCK, `-period 5HZ` = liveness ping) | `minix3/etc/rc.minix:117`, `minix-service.c:87` (`-n`), SEF ping path `sef.h:121-122` + `sef_ping.c:21` | No `IS_PROC_NR` anywhere (`rg` zero hits); permission face `service is { vm INFO; uid 0; }` at `minix3/etc/system.conf:271-277`. |
| T1-3 | `main(argc, argv)` → `env_setargs` → `sef_local_startup()` registers 4 callbacks then `sef_startup()` | `minix3/minix/servers/is/main.c:31-42,76-89` | The 4 registrations: init_fresh, init_lu, init_restart → same `sef_cb_init_fresh` (STATELESS); signal handler → `sef_cb_signal_handler`. |
| T1-4 | `sef_cb_init_fresh()` (boot anchor) → `map_unmap_fkeys(TRUE)` builds `fkeys`/`sfkeys` bitmaps with `bit_set` and calls `fkey_map` | `minix3/minix/servers/is/main.c:94-102`, `minix3/minix/servers/is/dmp.c:46-65` | Failure warns and continues (`dmp.c:63-65`); 16 hooks scanned, F1–F12 vs SF1–SF12 split. |
| T1-5 | TTY records IS as observer (`fkey_obs[]`); IS enters the main loop | `keyboard.c:401-415` (`kb_init_once`), `keyboard.c:429-527` (`do_fkey_ctl` FKEY_MAP arm: overlay registration, no EBUSY) | Overlay (cover-write) registration and the EPERM owner check on UNMAP are peer-contract facts owned by new 02. |

### Table T2 — Steady-state main loop (forever unless panic)

| # | Action | C anchor | Note |
|---|--------|----------|------|
| T2-1 | `get_work()`: `sef_receive(ANY, &m_in)` blocks; sets `who_e = m_source`, `callnr = m_type`; failure panics | `minix3/minix/servers/is/main.c:44-46,121-130` | SEF ping NOTIFYs are intercepted inside `sef_receive` (`sef.c:150,208-214`); the IS loop never sees them — a fact the loop document must state, not rediscover. |
| T2-2 | `is_notify(callnr)?` — the only accepted class is notification | `minix3/minix/servers/is/main.c:48`, `com.h:93` (`is_notify`) | Non-notify requests print a warning and take EDONTREPLY (`main.c:59-63`). |
| T2-3 | Notify from `TTY_PROC_NR` → `do_fkey_pressed(&m_in)`; notify from anyone else → silent EDONTREPLY (FIXME in source) | `minix3/minix/servers/is/main.c:48-58` | The asymmetry (warn the illegal-request case, stay silent on the wrong-notifier case) is a behaviour contract; Rust must preserve it, docs must not "fix" it with a log line. |
| T2-4 | `reply(who_e, result)` unless `result == EDONTREPLY`; `ipc_send` failure panics | `minix3/minix/servers/is/main.c:65-68,135-146` | Transport failure = the loop is dead → panic; acquisition failure = this screen is void → warn and continue (panic/warn boundary owned by 04 + 99). |
| T2-5 | SIGTERM → `map_unmap_fkeys(FALSE)` → `exit(0)`; anything else ignored | `minix3/minix/servers/is/main.c:107-116` | Unmap-before-exit order is an invariant; restart re-registers via the same fresh callback (STATELESS). |

### Table T3 — One function-key press (sub-mainline, the only work IS ever does)

| # | Action | C anchor | Note |
|---|--------|----------|------|
| T3-1 | Key interrupt → TTY `func_key()` bumps events → `ipc_notify(IS endpoint)` (payload-free) | `keyboard.c:532-585` | The notify carries no key identity — hence the pull model. |
| T3-2 | `do_fkey_pressed`: `fkey_events(&fkeys, &sfkeys)` pulls the bitmaps; failure warns and continues with stale/zero maps | `minix3/minix/servers/is/dmp.c:73-86` | Consuming read: TTY clears the events bitmap on FKEY_EVENTS (`keyboard.c` EVENTS arm). |
| T3-3 | Linear scan of all 16 hooks; `pressed(F1,F12,…)` / `pressed(SF1,SF12,…)` matches fire **each in turn, no break** | `minix3/minix/servers/is/dmp.c:70-72,88-94`, hooks table `dmp.c:18-35` | Multi-key press = multiple dumps in table order; no priority. |
| T3-4 | Each dump acquires a local copy through exactly one of five channels, then formats with `printf` in 22-line (VM: 24-line) pages with `--more--` and a static cursor | `dmp_kernel.c` (8 dumps), `dmp_pm.c`, `dmp_fs.c`, `dmp_rs.c`, `dmp_ds.c`, `dmp_vm.c`; `LINES 22` at `dmp_kernel.c:18`, `LINES 24` at `dmp_vm.c:9` | Channel choice per dump is fixed (see pool K-021–K-028); formatting never writes back to the source. |
| T3-5 | `mapping_dmp` (SF5) prints the key→description table generated from the same hooks array | `minix3/minix/servers/is/dmp.c:120-132`, `key_name` at `dmp.c:103-114` | Self-describing table: descriptions live next to function pointers. |
| T3-6 | Return EDONTREPLY: a key press is never replied to | `minix3/minix/servers/is/dmp.c:96-98` | EDONTREPLY is a reply-suppression sentinel (203), not an error code. |

- Spot-check performed for G1: 10 of the 30 anchors above re-verified with `sed -n`/`rg` during
  blueprint construction (T1-2 rc line, T1-4 `map_unmap_fkeys`, T2-1 `sef_receive`, T2-3 TTY branch,
  T3-3 `pressed` macro + hooks count 16, `LINES` both values, `is_notify` com.h:93, boot_image
  absence, system.conf service block, SI_* header). All 10 resolve.

## 2. Knowledge Pool (deduplicated; the no-knowledge-lost baseline)

Source types: **S** = stock (already in some existing document; the Location column lists every
occurrence, primary telling point first with `*`); **N** = new (carried by C source, non-C artifact,
OS theory, or current Rust truth, but by no existing document — each needs an evidence anchor, never
appears from nowhere). Types: C=concept, M=mechanism, D=data structure, P=interface/protocol,
V=constraint/invariant, E=arch-evolution, T=tooling/engineering, Q=test property.

### Birth and loop (K-001–K-008)

| ID | Name | Type | Src | Existing location | Anchor | Reader benefit (answers) |
|----|------|------|-----|-------------------|--------|--------------------------|
| K-001 | Debug-aggregation purpose: viewers leave the viewed address space | C | S | *01 §1.1, 00 §1 | `main.c:1-8` header comment | Why IS exists at all instead of in-kernel printf. |
| K-002 | Conditional existence (`debug_fkeys` gate → `up -n is`) | M | S | *00 §2-3, 01 (scattered), plan §1.2 | `etc/rc.minix:117` | When IS exists and when the whole stage is vacuous. |
| K-003 | No boot_image slot; RS runtime loading + dynamic endpoint | V | S | *00 §3, 01 §2.11, 99 §1 | `kernel/table.c:44-64` (absence), `rg IS_PROC_NR` = ∅ | Why no fixed endpoint constant may ever be introduced. |
| K-004 | Service permission face (`service is { vm INFO; uid 0; }`) | P | S | *00 §2, plan §1.2 | `etc/system.conf:271-277` | Which kernel calls IS may legally make. |
| K-005 | SEF 4-registration lifecycle + STATELESS restart (one fresh callback ×3) | M | S | *01 §2 | `main.c:76-102` | How birth, live-update, and restart collapse into one path. |
| K-006 | Main-loop skeleton: receive → notify-classify → work-or-ignore → conditional reply | M | S | *01 §2 | `main.c:44-69` | Where every later document attaches in the loop. |
| K-007 | Asymmetric refusal: non-notify warns, wrong-notifier stays silent (FIXME preserved) | V | S | *01 §2 | `main.c:48-63` | Which refusal branch may be logged and which must not. |
| K-008 | Unmap-before-exit + SIGTERM-only shutdown | V | S | *01 §2.8, 00 §3 | `main.c:107-116` | The shutdown order invariant and what restart redoes. |

### Function-key observer protocol (K-009–K-015)

| ID | Name | Type | Src | Existing location | Anchor | Reader benefit |
|----|------|------|-----|-------------------|--------|----------------|
| K-009 | Pull model: payload-free notify + EVENTS bitmap pull | C | S | *02 §1.1, 01 §2.3 | `main.c:48-52`, `dmp.c:82`, `ipc.h:2820` (`ipc_notify` takes only dest) | Why three commands exist for one key. |
| K-010 | FKEY_MAP/UNMAP/EVENTS + TTY_FKEY_CONTROL wire format (request/reply fields) | P | S | *02 §2, 99 §1 | `com.h:874-877`, `ipc.h:1447-1454/1925-1931` | The exact bits IS and TTY exchange. |
| K-011 | `map_unmap_fkeys`: bit_set assembly → fkey_map/unmap → warn-on-failure | M | S | *02 §2 | `dmp.c:46-65` | How 16 hooks become two bitmaps. |
| K-012 | TTY peer contract: observer array, overlay registration, EPERM owner check, EVENTS consumption, `func_key`→notify | P | S | *02 §2.7-2.8 | `keyboard.c:401-415,429-527,532-585` | What the IS side may assume about TTY (and what belongs to TTY's own future stage). |
| K-013 | F1–F12/SF1–SF12 constants + EXT/SHIFT encoding | D | S | *02, 99 §1 | `keymap.h:93-104,135-146` | How key identities are named on the wire. |
| K-014 | Registration-failure warning invariant (MAP failure → warn, boot survives) | V | S | *02 §4.3, todo V1-P1-1 | `dmp.c:63-65` | The one observable clue when keys silently stop working. |
| K-015 | SEF ping interception (loop never sees liveness pings) | M | S | *01 §2 | `sef.h:121-122`, `sef.c:150,208-214`, `sef_ping.c:21` | Why the classifier has no ping arm. |

### Dump dispatch (K-016–K-020)

| ID | Name | Type | Src | Existing location | Anchor | Reader benefit |
|----|------|------|-----|-------------------|--------|----------------|
| K-016 | 16-entry hooks table (key→function→description) + NHOOKS | D | S | *03 §2 | `dmp.c:18-40` | The single registry binding keys to dump capabilities. |
| K-017 | `do_fkey_pressed`: pull → dual-bitmap match → call each, no break | M | S | *03 §2 | `dmp.c:73-98` | Multi-press ordering semantics (table order, all fire). |
| K-018 | `pressed` range macro | M | S | *03 §2 | `dmp.c:70-72` | How F-range vs SF-range bits are tested. |
| K-019 | `key_name` + `mapping_dmp` self-description table | M | S | *03 §2 | `dmp.c:103-132` | How IS documents its own keys at runtime. |
| K-020 | EDONTREPLY as suppression sentinel (not an error) | V | S | *01 §2.5, *03 §2, 99 §2 | `sys/errno.h:199` (=203) | When a reply must not be sent. |

### Five acquisition channels (K-021–K-028)

| ID | Name | Type | Src | Existing location | Anchor | Reader benefit |
|----|------|------|-----|-------------------|--------|----------------|
| K-021 | `sys_getinfo(GET_*)`: 8 sub-requests used by IS, store-to-caller copy semantics | P | S | *04 §2.1, 05 | `com.h:316-331`, `libsys/sys_getinfo.c` | Which kernel tables arrive by which code. |
| K-022 | `sys_diagctl(STACKTRACE)`: stack-trace channel + kernel-side unwired (ENOSYS) forward reference | P | S | *04 §2.2, 05 | `com.h:252,412-415`, `syslib.h:164-168` | How procstack output is requested and its present limit. |
| K-023 | kerninfo direct read of the kmessages ring + usermapped-removal evolution (GET_KMESSAGES) | E | S | *04 §2.3 (A-3), *05 §4.1 | `type.h:214-232`, `dmp_kernel.c:63-88` | Why the kernel-message dump needs a new channel in the rewrite. |
| K-024 | `getsysinfo(SI_*)`: who→callnr map, exact-size match, sys_datacopy, root check (both ends) | P | S | *04 §2.4, 06/07/08/09 | `libsys/getsysinfo.c`, `sysinfo.h:11-17`, `pm/misc.c:105`, `vfs/misc.c:61` | The cross-server copy contract every SI dump relies on. |
| K-025 | `vm_info_*` (STATS/USAGE/REGION) batch protocol | P | S | *04 §2.5, 10 | `com.h:729-734`, `libsys/vm_info.c` | How variable-length VM data is pulled in batches. |
| K-026 | Panic/warn boundary: transport failure panics, acquisition failure warns-and-continues | V | S | *04 §2.6, 99 §3 | `main.c:124-130,143-146` vs each `printf("IS: warning…"); return/continue` | Which failure kills the server and which voids one screen. |
| K-027 | Per-file CPPFLAGS include visibility (why IS sees `kernel/proc.h`, `vfs/fproc.h`) | T | N | — | `servers/is/Makefile:10-14` | Build fact with semantic consequence: snapshot layout visibility. Holds the only Makefile-derived knowledge; the rest of the Makefile is excluded. |
| K-028 | Post-V1 typed acquisition outlets + `Acquires` supertrait + `diag_out` sink decision | E | N | todo Fix #5 (code only; docs carry only an appended note) | `os/servers/is/src/acquire.rs` (typed `out: &mut […]` methods, `Acquires` blanket impl), `sef.rs` (`diag_out`) | The seam shape every dump body now builds on; base doc text predates it. |

### Kernel dump domain, 8 seats (K-029–K-034)

| ID | Name | Type | Src | Existing location | Anchor | Reader benefit |
|----|------|------|-----|-------------------|--------|----------------|
| K-029 | Local snapshot triple (`proc`/`priv`/`image` globals mirroring kernel names) | D | S | *05 §2 | `dmp_kernel.c:55-57` | Why kernel macros apply unmodified in IS. |
| K-030 | PROCLOOP 22-line paging + oldrp resume cursor; PRINTRTS from/to rendering | M | S | *05 §2 | `dmp_kernel.c:20-40` | How long tables page and how IPC blockage prints. |
| K-031 | Four flag encoders (`s_flags`/`s_traps`/`p_rts_flags`/`proc_name`, incl. ANY/NONE/BOGUS/EMPTY) | M | S | *05 §2 | `dmp_kernel.c:218-231,236-248,300-313,386-395` | The fixed bit→character contracts. |
| K-032 | Eight dump bodies: proctab / procstack / privileges / image / irqtab / kmessages / monparams / kenv | M | S | *05 §2 | `dmp_kernel.c:63,94,122,169,192,253,319,359` | What each kernel seat shows and which channel feeds it. |
| K-033 | Dead/excluded kernel-side items: `click_to_round_k`, ARM `proctab_dmp` stub | V | S | *05, plan §5.4, 99 §4 | `dmp_kernel.c:42-43,349-355` | What must never be ported and why. |
| K-034 | Snapshot authorities moved to minix-types (post-E-ISPROD) + GET_KMESSAGES=7 (post-E-ISKMESS) | E | N | todo progress notes only | `minix-types::types::{proc_info,priv_info,irq_hook,boot_image,kinfo}`, `GET_KMESSAGES=7` | Current layout truth the rebuilt 04/05 must be written against. |

### Getsysinfo consumer family: PM / VFS / RS / DS (K-035–K-042)

| ID | Name | Type | Src | Existing location | Anchor | Reader benefit |
|----|------|------|-----|-------------------|--------|----------------|
| K-035 | PM `mproc_dmp`: pid/ppid/uid/gid/nice + 11-bit `flags_str` + 22-line `prev_i` paging | M | S | *06 §2 | `dmp_pm.c:21-69` | The personnel-roll call dump. |
| K-036 | PM `sigaction_dmp`: ignore/catch/block/pending + `getticks` alarm remainder clock face | M | S | *06 §2 | `dmp_pm.c:75-107` | The only clock-face read in the stage. |
| K-037 | VFS `fproc_dmp`: fd counting over OPEN_MAX, session/blocked/revived bits, CDEV endpoint branch (+ SDEV TODO gap) | M | S | *07 §2 | `dmp_fs.c:25-61` | The file-system roll call incl. its known blind spot. |
| K-038 | VFS `dtab_dmp`: label→major→driver map, NONE skipped, single screen | M | S | *07 §2 | `dmp_fs.c:67-83` | The shelf-map dump and why it never pages. |
| K-039 | RS `rproc_dmp`: dual-table (PUB+PRIV) fetch-or-nothing + IN_USE filter + 6-bit dual-source encoding | M | S | *08 §2 | `dmp_rs.c:26-58,61-73` | Identity table joined to status table per slot. |
| K-040 | DS `data_store_dmp`: 128-slot stock-take, IN_USE skip, 4-type rows, conditional-bound cursor with early-return replay | M | S | *09 §2 | `dmp_ds.c:9-51` | Sparse-table paging vs dense one-screen policy. |
| K-041 | Five-cursor taxonomy (pre-compare / candidate-break / conditional-bound / batch-dual) with intentional PM/VFS duplication | C | S | *09 §2.3, todo V1-P3-5 | `dump_pm.rs:99`, `dump_vfs.rs:152-156`, `dump_ds.rs:103`, `dump_vm.rs:101,162`, `dump_kernel.rs:239` | Why five pagers coexist and when they may merge. |
| K-042 | `PCStr` byte-string Display + `DumpState` per-domain cursor instantiation (post-V1 render architecture) | E | N | todo Fix #6 (code only) | `os/servers/is/src/lib.rs` (`PCStr`, `DumpState`, 16 `run_dump` arms, `render_*` fns) | How C `%-8.8s`/`%08x` specs map to `write!`; base doc text predates it. |

### VM dump domain (K-043–K-044)

| ID | Name | Type | Src | Existing location | Anchor | Reader benefit |
|----|------|------|-----|-------------------|--------|----------------|
| K-043 | `vm_dmp` batch state machine: stats-first-screen → proctab → usage+region batches, `prev_i`/`prev_base`, header-fit guard | M | S | *10 §2 | `dmp_vm.c:55-156` | How unbounded region data is copied in bounded batches. |
| K-044 | `print_region` repeat-folding + `rwx` rendering + end-of-list NULL sentinel | M | S | *10 §2 | `dmp_vm.c:11-52` | How identical contiguous regions collapse to one line. |

### Cross-cutting close (K-045–K-050)

| ID | Name | Type | Src | Existing location | Anchor | Reader benefit |
|----|------|------|-----|-------------------|--------|----------------|
| K-045 | Wire-constant authorities (9 families: fkey / GET+SI+DIAGCTL / VMIW / NOTIFY+TTY_SLOT / SIGTERM / EDONTREPLY / LINES) | D | S | *99 §1, todo §1.2 | `com.h`, `keymap.h`, `sysinfo.h`, `signal.h:67`, `errno.h:199` | The single-authority table for every magic number. |
| K-046 | Error-codeCall-site map: EPERM (getsysinfo root gate, TTY UNMAP owner gate), EINVAL (size mismatch), ENOSYS (unknown target, DIAGCTL unwired) | V | S | *99 §2 (thin) | `pm/misc.c`, `vfs/misc.c`, `keyboard.c` UNMAP arm | Which error comes from which gate. |
| K-047 | Single-threaded event-loop execution model (`!Send` sound, no `Rc`/`RefCell`, zero-alloc) vs kernel SMP+BKL | C | S | *00 §4, *99 §3, 01 §4 | `os/servers/is/src/lib.rs` (`IsServer` single owner, no `extern crate alloc`) | The concurrency contract and its kernel-side opposite. |
| K-048 | Dead declarations excluded with evidence: `diag_buf/diag_next/diag_size/sys_panic/dont_reply`, `DIAG_BUF_SIZE`, `_SYSTEM` | V | S | *99 §4, plan §5.4 | `glo.h:6-15`, `inc.h:7`, `rg` zero-consumer proofs | What is deliberately not taught, with grep proof. |
| K-049 | No C-side tests for IS; Rust test baseline and its staleness (86 claimed → 106 → 110 → **125 actual**) | Q | N | plan §3.5 (stale baseline), todo §0/§8 | `cargo test -p minix-is` = 125 passed | The verification baseline B-phase must refresh per document. |
| K-050 | Neighbour-producer boundary: TTY/VM/PM/VFS/RS/DS/kernel producer internals belong to their stages; IS owns only the consumer snapshot contract | V | S | *04/05–10 boundary rows, 99 §5 | Respective stage docs (`02-stage-vm`, `03-stage-rs`, `04-stage-pm`, `05-stage-vfs`, `07-stage-ds`, `01-stage-kernel`) | Where IS documentation stops and the producer stage takes over. |

- Statistics: 50 items (S=42 stock, N=8 new: K-027, K-028, K-034, K-042, K-049, plus K-023/K-026 extensions excluded from the new count — counted once each as listed). By type:
  C=4, M=22, D=5, P=6, V=9, E=4, T=2 (K-027 + test-infra half of K-049), Q=1 (K-049 counted once under Q).
  By current home: 00:3, 01:7, 02:7, 03:5, 04:8, 05:6, 06–09:8 (2 each), 10:2, 99:5, unowned-new: 8 rows
  flagged N above. Coverage check in Section 9 requires every row to land in exactly one new contract
  or carry an explicit delete-with-reason (only K-033's ARM half and K-048 rows are delete-from-port;
  they are still *documented* as exclusions, i.e. they land in 05/99 exclusion sections, not in code).

## 3. Coverage Audit (leak-proof, repetition-proof, boundary-proof)

### 3.1 Topic universe (four sources)

1. **C symbols**: 33 function definitions across 8 `.c` files (main.c 6 incl. `main` itself, dmp.c 4,
   dmp_kernel.c 13 definitions = 12 semantic functions with the dual-arch `proctab_dmp`, dmp_pm.c 3,
   dmp_fs.c 2, dmp_rs.c 2, dmp_ds.c 1, dmp_vm.c 2), 3 snapshot globals (`proc`/`priv`/`image`),
   3 global server statics (`m_in`/`m_out`/`who_e`/`callnr` = 4 names), 3 macros
   (`pressed`, `PROCLOOP`, `PRINTRTS`), 2 `LINES` constants (22/24), 5 per-file static cursors,
   4+3+1+1 flag encoders, `hooks[]`+`NHOOKS`, `glo.h`/`inc.h`/`proto.h` declarations.
2. **OS general concepts** for these mechanisms: observer/notify-pull models, debug-aggregation and
   audit discipline, snapshot-vs-live reads, paging of unbounded diagnostics, wire-layout (repr(C))
   consumer contracts, single-threaded server execution, liveness pinging, conditional service
   existence.
3. **Non-C artifacts** (Section 3.4, N-01–N-10): build includes, image/boot absence, no asm, rc+RS
   loading, permissions, wire formats, error paths, shutdown, concurrency, test infra.
4. **Boundary contracts**: previous stage `07-stage-ds` (DS slots, identity, `SI_DATA_STORE` producer,
   client library — DS stage owns all of it; IS consumes read-only), master-plan stage order
   (IS after DS, before INIT; RS-loaded group), `edge_todo.md` edge entries (E-ISWIRE production
   transport, E-ISPROD layout alignment — now landed for kernel+VFS, E-ISKMESS — landed, E-ISBOOT
  integration testing pending).

### 3.2 Coverage-gap table (every row gets a home in Section 7; none is left "TBD")

| Gap | Topic in universe, missing or decayed in current docs | Evidence | Disposition (new-doc / merge-into / other-stage / wont-do + reason) |
|-----|--------------------------------------------------------|----------|---------------------------------------------------------------------|
| G-01 | Boot-precondition chain end-to-end (debug_fkeys gate + `up -n` + 5HZ ping + RS dynamic endpoint + `service is` perms) has no single owner; pieces scatter across 00 §2, 01 body, plan §1.2 | 00 is 60 lines with no chain table; 01 buries `rc` facts mid-text | Merge-into: new 00 (chain table, reader-facing) + new 01 (mechanism: env_setargs/SEF/ping/perms). No new document: two homes already exist, the defect is consolidation, not absence. |
| G-02 | Per-file CPPFLAGS include visibility (`-I minix` for dmp_kernel/dmp_rs/dmp_vm, `-I servers -I fs` for dmp_fs) and the `USE_APIC` effect on the IRQ-hook table width | `servers/is/Makefile:10-18`; current docs never mention it | Merge-into: new 04 (one subsection: which snapshot header becomes visible to which dump, and why the IRQ table size follows the kernel build flag). Rest of the Makefile stays excluded (build glue, no runtime semantics). |
| G-03 | Test-count staleness: docs claim 86, todo cites 106→110, actual is 125 passed | `cargo test -p minix-is` = 125; `rg "86 passed" 08-stage-is/` hits 10 documents | Merge-into: every new 01–10 contract carries a §5 refresh duty (re-run, re-list, re-count at B-phase write time). Not a structural gap. |
| G-04 | Rust §3/§4 patchwork: V1 fixes (warn_fkey_ctl, void `request_fkey_map`, typed outlets, `Acquires`, `diag_out`, `PCStr`, `DumpState`, snapshot imports) live as appended notes over pre-V1 base text | todo Fix #1–#8 vs current §3/§4 bodies; e.g. 01 §4.1 alloc sentence fixed once (Fix #7) while surrounding text still describes the ENOSYS-stub era | Merge-into: rewrite-from-truth duty in every contract (re-derive §3/§4 from `os/servers/is/src/` HEAD + minix-types authorities; keep C-anchored §2, rewrite the rest). This is the central rebuild justification. |
| G-05 | 00 has no true-order table, no reading paths, no position-answerability (60-line nav stub) | 00 body §§1–7 vs prompt §5.1 standards | Merge-into: new 00 contract (full entry: what/when/how-to-read + chain table + paths). |
| G-06 | 99 has no value tables (constant values, error call-sites) and no exclusion proofs inline (58 lines) | 99 body §§1–6 vs K-045/K-046/K-048 | Merge-into: new 99 contract (9-family value table + error gate table + exclusion grep proofs). |
| G-07 | E-ISPROD landing (kernel+VFS snapshots moved to minix-types single authority) postdates the docs' A-4 proposal prose | edge_todo E-ISPROD 2026-09-16 progress (ProcInfoStruct/PrivInfoStruct/irq_hook/boot_image/kinfo upper-homed; VFS SI_* re-export) | Merge-into: new 04 + 05 + 07 contracts (replace "proposal" language with "authority" language + import paths; PM/RS/DS/V M producer alignment stays pending and stays marked as such). |
| G-08 | E-ISKMESS landing (GET_KMESSAGES=7 + kernel ring-copy arm + IS `KernelKmessTransport`) postdates the docs' A-3 "to-be-designed" prose | edge_todo E-ISKMESS closure entry 2026-09-17; IS tests 106→110 | Merge-into: new 04 (channel spec) + 05 (kmessages seat rewrite against the real channel). |

- New knowledge rows created for the audit: K-027 (from G-02), K-028/K-034/K-042 (from G-04/G-07/G-08),
  K-049 (from G-03). All carry evidence anchors in Section 2 and land in Section 5 contracts.

### 3.3 Repetition table (one primary telling point; all others become references)

| Topic repeated today | Occurrences | New-catalog primary | Others become |
|---|---|---|---|
| D-01 EDONTREPLY semantics | 01 §2.5, 03 §2, 99 §2 | 01 (loop contract) | 03 one-line use + pointer; 99 table row + pointer. |
| D-02 LINES=22 / VM LINES=24 / `--more--` | 05, 06, 07, 09, 10, 99 §1 | 05 (first paging exposition) + 09 (cursor taxonomy with comparison table) | 06/07/10 use + pointer; 99 constants row + pointer. |
| D-03 SI_PROC_TAB meaning | 04 §2.4, 06, 07, 99 §1 | 04 (channel contract) | 06/07/08 use + pointer; 99 row + pointer. |
| D-04 FKEY constants | 02, 99 §1 | 02 (protocol contract) | 99 row + pointer. |
| D-05 Panic-vs-warn boundary | 01, 04 §2.6, 99 §3 | 04 (acquisition error surface; the rule is about fetching) | 01 loop-side restatement reduced to one sentence + pointer; 99 row + pointer. |
| D-06 Single-threaded model statement | 00 §4, 01 §4, 99 §3 | 99 (execution-model close) | 00/01 shrink to positioning sentences + pointers. |

### 3.4 Out-of-scope table (correct home named for each)

| Topic some current doc touches | Verdict | Correct home |
|---|---|---|
| O-01 TTY internals beyond the fkey observer/notify face (tty main loop, keyboard scanning, `show_key_mappings` private section) | Out of IS scope | Future TTY stage (16-stage-drivers family); new 02 cites only the contract face. |
| O-02 Kernel producer internals (`do_getinfo` arms, `do_diagctl` wiring, kerninfo construction, grant/copy primitives) | Out of IS scope | `01-stage-kernel` (25-misc-unported, 28-usermapped-data, 32-stack-tracing); new 04 cites call numbers + message shapes only. |
| O-03 PM/VFS/RS/DS/VM producer internals (table management, `do_getsysinfo` server sides, VM region bookkeeping) | Out of IS scope | `04-stage-pm`, `05-stage-vfs`, `03-stage-rs`, `07-stage-ds`, `02-stage-vm`; new 06–10 cite layout ABIs only. |
| O-04 SDEV blocked-on endpoint (C TODO: endpoint unavailable) | Out, inherited gap | VFS crate to resolve; new 07 carries the inherited-TODO marker, does not design the fix. |
| O-05 `proctab_dmp` ARM empty branch | Excluded, will not port | x86-64 target; new 05 exclusion section (A-7). |
| O-06 Dead code/declarations (`click_to_round_k`, `diag_buf` family, `DIAG_BUF_SIZE`, `_SYSTEM`, Makefile except CPPFLAGS) | Excluded with grep proof | New 05 (K-033) + new 99 (K-048) exclusion sections. |
| O-07 End-to-end bring-up (rc equivalent, RS dynamic loading, TTY staging, integration-test suite) | Out of stage scope | E-ISBOOT in `edge_todo.md`; new 00/01 mark the seam, do not design it. |

### 3.5 Non-C topics: fixed-list answers (each says where it is taught or why not in this stage)

| # | Fixed-list topic | Finding | Taught in |
|---|------------------|---------|-----------|
| N-01 | Linking and loading | Standard `minix.service.mk` service link; no custom linker script, no section tricks | Nowhere (no semantics); stated once in 99 exclusions with the Makefile path as proof. |
| N-02 | Image and memory layout | The fact is an *absence*: no boot_image slot, no reserved memory, no `.usermapped` reliance after A-3 | New 00 §2 (absence table) + new 01 (no `IsProcNr`); K-003. |
| N-03 | Assembly entry and trap entry | None: pure user-space C, no arch asm; the only arch-conditional is the ARM `proctab_dmp` stub (excluded) | New 05 exclusion note (A-7); nowhere else. |
| N-04 | Boot assembly (rc + RS load + perms + ping) | Conditional chain T1-1–T1-5, fully evidenced | New 00 (reader chain) + new 01 (mechanism); K-002–K-005. |
| N-05 | Build and toolchain | Only CPPFLAGS visibility is semantic (G-02); `USE_APIC` follows the kernel flag for IRQ width | New 04 subsection (K-027); rest excluded in 99. |
| N-06 | Cross-module interfaces and wire formats | TTY_FKEY_CONTROL / GET_* / SI_* / VMIW_* / DIAGCTL codes + 4 libsys clients | New 02 (fkey wire) + new 04 (data wire) + new 99 (value tables); K-010/K-013/K-021–K-025/K-045. |
| N-07 | Error paths | warn-and-continue per dump / per-batch item; panic only on transport; error-code gate map | New 04 (surface) + new 10 (batch-item continue) + new 99 (code→gate table); K-026/K-046. |
| N-08 | Shutdown and exit | SIGTERM-only, unmap-then-exit, STATELESS restart | New 01; K-008. |
| N-09 | Concurrency and synchronisation | None needed: single-threaded loop, static globals single-owned, per-dump static cursors; five-cursor taxonomy in 09 | New 99 (model) + new 09 (cursors) + new 01 (state ownership); K-007/K-041/K-047. |
| N-10 | Test infrastructure | No C tests; Rust suite is the contract (125 passed at blueprint time; refresh duty per doc) | Per-document §5 (K-049); plan §3.5 baseline superseded by this blueprint. |

- Incidental terminology defect (not a knowledge gap — recorded so B-phase does not propagate it):
  the current 08/09 headers cite an "STDO trap" allegedly taught in 04 §2.4/§6, but 04 never defines
  the acronym (`rg STDO` over `04-is-data-acquisition.md` returns nothing; verified 2026-09-20,
  while 08/09 cite it four times). The underlying concept is real — the RS dual-fetch short-circuit
  (`dmp_rs.c:33-34` `||`: first table fails → return) — and is carried under its plain name in K-039
  and the 04/08 contracts. The rebuilt 08/09 use the plain name; the acronym is retired, not migrated.

## 4. New Catalog (numbers kept, contents rebuilt)

### 4.1 New-document master table (12 rows; titles unchanged — all 12 titles already name their semantic unit correctly)

| New No. | Title | One-line mission | Group |
|---|---|---|---|
| 00 | IS architecture overview | What IS is, when it exists, how to read this stage | Entry |
| 01 | Startup entry and main-loop skeleton | Birth (SEF, endpoint, perms, ping) + loop (classify, reply gate, shutdown) | Entry |
| 02 | Function-key observer protocol | The three-command subscribe–pull contract, IS side + TTY peer face | Protocol |
| 03 | Dump dispatch | The 16-hook registry and the pull–match–fire loop | Dispatch |
| 04 | Data acquisition: five channels | The only fetching framework all dumps build on (channels, wire, errors, build visibility) | Framework |
| 05 | Kernel dump domain | Eight kernel seats + snapshot triple + paging/encoding idiom | Domain |
| 06 | PM dump domain | mproc roll call + signal/alarm face (getsysinfo representative) | Domain |
| 07 | VFS dump domain | fproc roll call + device map (sparse vs dense paging contrast) | Domain |
| 08 | RS dump domain | Dual-table identity/status join + dual-source encoding | Domain |
| 09 | DS dump domain | 128-slot stock-take + five-cursor taxonomy close | Domain |
| 10 | VM dump domain | Batch state machine + region folding (most complex cursor) | Domain |
| 99 | Global concepts close | Value tables, error gates, execution model, exclusions with proofs | Close |

- Length guidance (soft, per the task brief — readability over uniformity): 00 ~250 lines; 01 ~600;
  02 ~550; 03 ~350; 04 ~600; 05 ~700 (8 seats need room; single concept = "the kernel audit",
  acceptable as one long doc); 06/07/08 ~250 each; 09 ~300 (carries the cursor taxonomy);
  10 ~350; 99 ~300. Total ≈ 4500 lines vs today's 3475 — the growth is the missing content
  (G-01/G-02/G-05/G-06/G-07/G-08), not padding.

### 4.2 Reading paths

- Main path (every reader): 00 → 01 → 02 → 03 → 04 → 06 (representative getsysinfo dump, read fully)
  → 05 → 10 → 99. Rationale: birth, protocol, dispatch, framework, one thin dump to ground the
  pattern, then the two heavy domains, then the close.
- Branch paths (by interest): 07, 08, 09 after 06 — each is self-contained given 04 + the 06 pattern;
  any order works; the contracts forbid forward references between them so no order is privileged.
- Skippable on first reading: 05's per-seat format derivations (keep the snapshot/paging/encoding
  idiom, skip individual `printf` derivations), 10's batch-boundary defence derivations, 99's
  exclusion proofs (keep the value tables). Skippability is declared inside each document, not just here.

### 4.3 Parallel-body organisation (no faked linear order among the six dump domains)

- Unified framework first: 04 owns all five channels, the wire shapes, the error surface, and the
  build-visibility note. No dump document re-explains a channel.
- Scenario grouping: the four SI-family dumps (06/07/08/09) form one group; **06 (PM) is the
  representative member read in full** — single table, single SI code, clock-face bonus — while
  07/08/09 converge through difference tables against 06 (different table, different SI code,
  different filter/encoding/paging delta). Kernel (05, three channels + 8 seats) and VM (10, batch
  protocol + dual cursor) stand outside the group as specials with their own idiom sections.
- Main vs branch: the F1–F8/F10 kernel+VM seats and SF1/SF2/SF6 PM/RS seats are the main path;
  SF5 mapping self-description, SF8/SF9 DS/procstack seats, and the F5/F7 monitor/kernel-message
  seats are declared branch (skippable) inside 03's journey map and 05/09 bodies.

### 4.4 Forward-reference and dependency properties (verified in Section 9, G3/G4)

- Prerequisite chain: 00 → 01 → 02 → 03 → 04 → {05, 06} → {07, 08, 09} → 10 → 99. The 07/08/09 set
  depends only on 04 + the 06 pattern (never on each other); 10 depends on 04 + 09's cursor taxonomy;
  99 depends on all. The graph is acyclic by construction; the check is mechanical (scan each
  contract's Prerequisites field for higher numbers only).
- First-occurrence completeness: K-001 (purpose) first in 00; K-009 (pull model) first in 02;
  K-021–K-025 (channels) first in 04; K-030 (paging idiom) first in 05; K-041 (cursor taxonomy)
  first in 09; K-045 (value tables) first in 99. Later documents reference, never re-teach.

### 4.5 Why no renumber, merge, split, or new number (five reasons; this is a verdict, not a deferral)

1. Teaching order already equals runtime order (T1→T2→T3 maps to 00/01→02/03→04→05–10→99); a reorder
   would manufacture churn with zero pedagogical gain.
2. File↔document 1:1 traceability (`dmp_*.c` ↔ 03/05–10, `main.c` ↔ 01, fkey halves of `dmp.c` ↔ 02)
   is load-bearing for grep verification (plan §5.1, Gate A semantic map); merging would break it.
3. Code↔document 1:1 traceability (`dump_*.rs`, `dispatch.rs`, `tty_fkey.rs`, `acquire.rs`,
   `sef.rs` ↔ same documents) is load-bearing for the 17 code-comment doc references; merging or
   renumbering would orphan them (see breakage pricing in Section 8).
4. Forward-reference-zero already holds for this numbering (Section 9, G3 passes on the contract
   graph); splits solve no existing violation.
5. Every genuine gap found (G-01–G-08) is a *depth/completeness/staleness* defect inside an existing
   document's remit, not a missing remit — so each gap merges into an existing contract (Section 7)
   instead of minting a document. Minting one (e.g. a separate "birth preconditions" doc) would
   create a 100-line fragment that weakens 00/01 rather than strengthening them.

### 4.6 Order-difference table (runtime truth vs teaching order; both deviations compensated)

| # | Runtime fact (anchor) | Teaching choice | Reason + compensation |
|---|-----------------------|-----------------|----------------------|
| O-1 | TTY's `do_fkey_ctl`/`func_key` run *before* IS exists (observer registered at T1-5, notification at T3-1) | 02 teaches the TTY peer face *after* 01's birth | Birth must come first or registration has no actor. Compensation: 01 states the peer dependency up front ("registration needs a TTY that honours three commands — see 02") without teaching it. |
| O-2 | At runtime each dump fetches lazily when its key fires (no framework phase exists) | 04 front-loads all five channels before any dump | Six dumps × five channels taught per-dump would repeat the wire five times (D-03 pattern at scale). Compensation: 04 ends with a per-dump channel-routing table that each of 05–10 quotes on entry. |

## 5. Per-Document Contracts (B-phase work orders; one missing = no delivery)

### 00 — IS architecture overview (entry)

- Mission: answer "what is IS, when does it exist, how do I read this stage" and nothing else.
- Covers: K-001 (purpose), K-002 (conditional existence), K-003 (no boot slot / dynamic endpoint),
  K-004 (permission face), K-047 positioning sentence (D-06), K-050 boundary pointer.
- Non-covers: any mechanism detail → 01–10; any value table → 99; producer internals → neighbour
  stages (table in §4 of the new 00, pointing at 02-stage-vm/03-stage-rs/04-stage-pm/05-stage-vfs/
  07-stage-ds/01-stage-kernel).
- Prerequisites: none (may cite the master-plan stage order and 07-stage-ds/00-ds-overview.md §1.2
  boot position as already-read context, not as requirements).
- Post-refs: 01 (birth mechanism for the chain below), 02 (peer face), 04 (framework), 99 (close).
- Ground truth: `servers/is/` file map (8 `.c` + role line each); `kernel/table.c:44-64` (17 entries,
  absence shown); `etc/rc.minix:117`; `etc/system.conf:271-277`; `os/servers/is/` module map
  (13 files + role line each).
- Knowledge rows (why-here + source): K-001 — the audit-discipline thesis, stock from 01 §1.1,
  promoted here because purpose precedes mechanism; K-002 — reader-facing chain table (gate →
  `up -n is -period 5HZ` → RS load → register → loop), stock consolidated from 00 §2/01/plan §1.2;
  K-003 — absence table (boot_image: no; fixed endpoint: no; reserved memory: no), stock from 00 §3;
  K-004 — permission block quote, stock from 00 §2; K-047 — two positioning sentences + pointer to 99.
- Acceptance: a newcomer can state (1) the three birth preconditions, (2) why IS has no number
  constant, (3) the 12-document map with main/branch paths, without opening any other document;
  forward-reference scan of its Prerequisites field is empty.

### 01 — Startup entry and main-loop skeleton (entry)

- Mission: own birth (SEF, endpoint injection, perms, ping invisibility) and the steady-state loop
  (classify, warn/silent asymmetry, reply gate, shutdown) — the skeleton everything else hangs on.
- Covers: K-002 mechanism half, K-003 `IsProcNr` ban, K-005 (SEF lifecycle), K-006 (loop skeleton),
  K-007 (refusal asymmetry), K-008 (shutdown), K-015 (ping interception), K-020 loop half (D-01),
  K-026 loop half (D-05, one sentence + pointer), K-028 `diag_out` decision restated as consumer
  (Acquires/`diag_out` signatures quoted, design rationale stays in 04), K-047 state-ownership half.
- Non-covers: fkey registration body → 02 (01 states only the call site `map_unmap_fkeys(TRUE)` at
  `main.c:99`); dispatch body → 03 (01 states only the `do_fkey_pressed` call site at `main.c:51`);
  channel mechanics → 04; dump bodies → 05–10; integration/startup-wiring production impl → E-ISWIRE
  (01 marks the `UnimplementedTransport` seam and stops).
- Prerequisites: 00.
- Post-refs: 02 (registration + peer), 03 (dispatch entry), 04 (error-surface owner), 99 (model close).
- Ground truth: `main.c` full (148 lines: `main:31`, `sef_local_startup:76`, `sef_cb_init_fresh:94`,
  `sef_cb_signal_handler:107`, `get_work:121`, `reply:135`, globals `:14-17`); `inc.h` include face;
  `sef.h:121-122`, `sef.c:150,208-214`, `sef_ping.c:21`; `com.h:64,90-93`; `signal.h` SIGTERM=15;
  Rust `lib.rs` (`IsServer`, `IsServerState`, classifier, reply gate), `main.rs`, `sef.rs`
  (incl. `diag_out` + `warn_fkey_ctl`), `state.rs`, `dispatch.rs` classifier half — **re-derived
  from HEAD** (G-04: current text describes the ENOSYS-stub era around the V1 notes; the boot
  anchor, void `request_fkey_map`, and three-generic `IsServer<T,F,A: Acquires>` must be primary
  text, not notes).
- Knowledge rows: K-005 — birth table T1-3–T1-4 quoted, stock from 01 §2, refreshed against
  `sef.rs` HEAD; K-006 — loop table T2-1–T2-4, stock; K-007 — asymmetry diagram (notify? → TTY? →
  warn?/silent?), stock, with the FIXME honesty marker kept; K-008 — shutdown sequence + STATELESS
  note, stock; K-015 — ping-interception box ("the classifier has no ping arm because…"), stock;
  K-020 — sentinel definition + call sites (`main.c:55,62`, `dmp.c:97`), primary (D-01); K-028 —
  consumer restatement (signatures + "rationale in 04"), new.
- Acceptance: reader can draw the loop flowchart with all four exits (work / silent-ignore /
  warn-ignore / reply) and state the transport-panics vs fetch-warns split in one sentence; every
  Rust signature quoted matches HEAD (`rg` re-check at write time); §5 test table refreshed to the
  then-current `cargo test -p minix-is` count with named classifier/lifecycle tests.

### 02 — Function-key observer protocol (protocol)

- Mission: own the three-command subscribe–pull contract end to end: IS-side assembly, wire format,
  libsys client, and the TTY peer face IS relies on.
- Covers: K-009 (pull model), K-010 (wire format), K-011 (`map_unmap_fkeys`), K-012 (TTY peer),
  K-013 (key constants), K-014 (failure-warning invariant), K-004 TTY-notification half
  (notify carries no payload → pull is forced).
- Non-covers: dispatch on the pulled bitmaps → 03; dump contents → 05–10; TTY internals beyond the
  fkey face (O-01) → future TTY stage; production `_taskcall` wiring → E-ISWIRE (02 marks the seam).
- Prerequisites: 01 (init anchor `map_unmap_fkeys(TRUE)`; loop notify branch).
- Post-refs: 03 (bitmap consumer), 04 (EVENTS pull vs data pull contrast — one paragraph), 99
  (value-table rows).
- Ground truth: `dmp.c:46-65` (registration half); `com.h:874-877`; `ipc.h:1447-1454/1925-1931`
  + union slots; `keymap.h` F/SF ranges; `sysutil.h:43-46`; `libsys/fkey_ctl.c` (30 lines);
  `keyboard.c:60-78` (obs array), `:401-415` (init), `:429-527` (`do_fkey_ctl` three arms incl.
  overlay + EPERM), `:532-585` (`func_key` → `ipc_notify`); Rust `minix-types::ipc::tty`
  (TtyFkeyCtlReq/Reply, A-1 landed) + `os/servers/is/src/tty_fkey.rs` re-derived from HEAD
  (incl. `warn_fkey_ctl` path from Fix #1 — primary text, not a note).
- Knowledge rows: K-009 — pull-model diagram (notify → EVENTS → match), stock from 02 §1.1, kept as
  the chapter thesis; K-010 — request/reply field table with widths, stock; K-011 — bit_set assembly
  walkthrough, stock; K-012 — peer state-machine table (MAP overlay / UNMAP owner-gated / EVENTS
  consuming) + `func_key` notify semantics, stock, with the explicit boundary fence ("deeper TTY
  behaviour belongs to the TTY stage"); K-014 — invariant box + the `FakeFkey` failure test that
  nails it, stock + refreshed; K-013 — constant table (primary, D-04).
- Acceptance: reader can state the three commands' effects on TTY state, the reply-bitmap write-back
  rule, and the exact observable when registration fails; no sentence describes TTY internals beyond
  the cited ranges; §5 refreshed.

### 03 — Dump dispatch (dispatch)

- Mission: own the hooks registry and the pull–match–fire loop plus the sub-mainline journey map
  that routes readers to 04 and 05–10.
- Covers: K-016 (hooks table), K-017 (`do_fkey_pressed`), K-018 (`pressed`), K-019
  (`key_name`/`mapping_dmp`), K-020 dispatch half (one-line use + pointer, D-01), K-042 render
  architecture restated as consumer (16 `run_dump` arms = "fetch then call the domain renderer";
  signatures quoted, rationale stays in 05–10), parallel-group routing (which key → which domain doc).
- Non-covers: registration → 02; channels → 04; any dump body → 05–10.
- Prerequisites: 01 (loop call site), 02 (EVENTS pull semantics).
- Post-refs: 04 (next: how each fired dump fetches), 05–10 (per-key destinations), 09 (cursor
  taxonomy preview — one sentence).
- Ground truth: `dmp.c:17-40` (16 hooks — count asserted), `:70-72` (macro), `:73-101` (handler),
  `:103-132` (naming + self-table); Rust `dispatch.rs` full (HOOKS table, `dispatch_each`,
  `key_name`, mapping format constants) + `lib.rs` `handle_fkey_pressed`/`run_dump` re-derived from
  HEAD (post-Fix #4: no `matched` buffer, keys derived via `HOOKS.map`; post-Fix #6: 16 filled arms).
- Knowledge rows: K-016 — registry table (all 16 rows: key, function, description), stock, count
  asserted in text and test; K-017 — loop walkthrough emphasising no-break multi-fire in table
  order, stock; K-018 — range-test explanation, stock; K-019 — self-description mechanism, stock;
  journey map (T3-1–T3-6 condensed to a figure with document pointers at each hop), stock from
  plan §1.3, kept as the chapter close.
- Acceptance: reader can list all 16 bindings from memory of the table's structure and predict the
  behaviour of a two-key press; the journey map names the exact next document at each hop; §5 refreshed.

### 04 — Data acquisition: five channels (framework)

- Mission: be the single framework every dump builds on — five channels, wire shapes, error surface,
  snapshot authorities, and the one semantic build note. Nothing in 05–10 may re-derive a channel.
- Covers: K-021 (`sys_getinfo`), K-022 (`sys_diagctl`/STACKTRACE + ENOSYS forward ref), K-023
  (kerninfo ring + GET_KMESSAGES evolution — rewritten against the landed channel, G-08), K-024
  (`getsysinfo` both ends), K-025 (`vm_info_*`), K-026 error surface (primary, D-05), K-027 (CPPFLAGS
  visibility — the one semantic build note, new), K-028 typed outlets + `Acquires` + `diag_out`
  (primary design text, new), K-034 authority half (import paths + "proposal→authority" language
  change, G-07), K-050 consumer/producer boundary restated for fetchers.
- Non-covers: layout interpretation of any fetched bytes → 05–10; per-dump formatting/paging → 05–10;
  producer server internals (O-02/O-03) → neighbour stages; kernel-side GET_KMESSAGES arm mechanics
  beyond the IS-visible contract → 01-stage-kernel (04 cites the sub-request number + copy shape only).
- Prerequisites: 01 (panic/warn vocabulary), 02 (taskcall sync semantics + EVENTS-pull contrast),
  03 (which dumps need which channel — the routing table quotes it back).
- Post-refs: 05–10 (consumers, each named with its channel + SI/GET code), 99 (code→gate rows).
- Ground truth: `com.h` GET_* block `:316-331`, SYS_DIAGCTL `:252`, DIAGCTL_CODE `:412-415`,
  RS/DS_GETSYSINFO `:476/:507`, VM_INFO/VMIW `:729-734`; `sysinfo.h:11-17`; `callnr.h`
  PM_GETSYSINFO/VFS_GETSYSINFO; `type.h:214-232` (kerninfo, historical read path); `syslib.h:164-187`
  (shorthand macros); `libsys/{getsysinfo,vm_info,sys_diagctl,sys_getinfo}.c`; `libc/sys/init.c:1-32`
  + `kernel/usermapped_data.c:4-15` (removed path, cited as history for A-3); `pm/misc.c:105-145`,
  `vfs/misc.c:52-113` (server-side size-match + datacopy + root check); IS use sites per channel;
  `servers/is/Makefile:10-14` (K-027); Rust `minix-types::ipc::sysinfo` + `ipc::vm` + `acquire.rs`
  HEAD (typed outlets, `Acquires` supertrait, `SiWhat`/wire helpers demoted — G-04 rewrite, not notes);
  `sef.rs` `diag_out` (A-6 landed choice: `&mut dyn core::fmt::Write`, cursor/channel orthogonality).
- Knowledge rows: K-021 — per-code message-shape + copy-semantics table (8 rows), stock, refreshed;
  K-022 — STACKTRACE request shape + ENOSYS forward-reference box, stock; K-023 — ring layout +
  `kmess_start` + GET_KMESSAGES=7 contract, rewritten (was "to-be-designed", now "the channel");
  K-024 — both-ends contract (client map + size-exact + datacopy SELF→requester + root gate), stock;
  K-025 — STATS/USAGE/REGION batch verbs, stock; K-026 — error-surface table (which call fails →
  which warning → continue), primary; K-027 — visibility note (which `-I` exposes which header to
  which dump), new; K-028 — seam design text (why typed slices, why not `&[u8]`/`Vec`, supertrait
  rationale, sink choice with Redox/`core::fmt::Write` precedent), new as primary text.
- Acceptance: reader can name all five channels with their IS-used codes and the failure behaviour
  of each; a dump author can implement a new seat using only this document + the target layout ABI;
  per-dump routing table quoted verbatim by 05–10 entries (checked in Section 9); §5 refreshed.

### 05 — Kernel dump domain (domain, heaviest)

- Mission: own the kernel audit: snapshot triple, 8 seats, paging/encoding idiom, exclusions.
- Covers: K-029 (snapshot triple), K-030 (PROCLOOP/PRINTRTS/paging — first full paging exposition,
  D-02), K-031 (four encoders), K-032 (8 bodies), K-033 (dead/ARM exclusions), K-022 consumer half
  (procstack call shape + ENOSYS pointer), K-023 consumer half (kmessages seat against the landed
  channel), K-034 kernel-snapshot imports (G-07), K-042 kernel renderers (`render_*` + PCStr usage).
- Non-covers: channel mechanics → 04; PM/VFS/RS/DS/VM tables → 06–10; kernel producer internals →
  01-stage-kernel; ARM port → excluded (A-7).
- Prerequisites: 04 (channels + routing row); 03 (which keys land here: F1,F3–F7,F10,SF9).
- Post-refs: 06 (representative thin dump — "same paging, one table"), 09 (cursor taxonomy —
  "kernel uses the pre-compare cursor"), 99 (constant rows used here).
- Ground truth: `dmp_kernel.c` full (396 lines) + `kernel/proc.h` (proc/RTS/ADDR/`isemptyp`) +
  `kernel/priv.h` + `kernel/type.h` (irq_hook) + `param.h` (kinfo/boot_image) + `type.h:148-176`
  (boot_image/kmessages) + `bitmap.h` BITCHUNK_BITS + `com.h:48` (IDLE) + `:308` (IRQ_REENABLE) +
  `ipcconst.h` + `endpoint.h` ANY/NONE + `sys_config.h` sizes + `config.h` + `interrupt.h`
  NR_IRQ_VECTORS + `multiboot.h:240` PARAM_BUF; Rust `dump_kernel.rs` HEAD (imported snapshots,
  PageCursor, kmess_start, expand_newlines with the V1-P1-4-corrected trailing-newline semantics,
  NameClass, format constants, 8 renderers) — re-derived, G-04.
- Knowledge rows: K-029 — triple table (which snapshot each seat reads), stock; K-030 — PROCLOOP
  walkthrough + PRINTRTS from/to rendering, primary paging text, stock; K-031 — four encoder tables
  (bit→char, incl. SENDA negative-mask corner), stock; K-032 — eight seat subsections (channel +
  layout + format derivation each; branch seats F5/F7/SF9 marked skippable), stock, with kmessages
  and procstack subsections rewritten (G-08/G-07); K-033 — exclusion box with grep proofs, stock.
- Acceptance: reader can explain any of the 8 outputs' columns from the encoder tables; the paging
  idiom section is quotable by 06/07/10 (one pointer each, no re-derivation); §5 refreshed with
  per-seat render assertions named.

### 06 — PM dump domain (domain, group representative — read fully)

- Mission: be the fully-worked getsysinfo-consumer example: one table, one SI code, filter, paging,
  plus the only clock face in the stage.
- Covers: K-035 (`mproc_dmp`), K-036 (`sigaction_dmp` + getticks clock), K-024 consumer half
  (SI_PROC_TAB use + pointer), K-030 reuse (paging by pointer + PM delta: candidate-break `>22`
  vs kernel pre-compare — stated, not re-derived), K-041 PM half, K-046 alarm/EPERM rows as used.
- Non-covers: channel mechanics → 04; PM server internals → 04-stage-pm; full mproc authority →
  PM crate (06 documents the consumed snapshot subset + A-4 alignment marker).
- Prerequisites: 04 (SI channel), 05 (paging idiom — used by contrast), 03 (SF1/SF2 keys).
- Post-refs: 07/08/09 (difference-table source — "vs PM:" rows), 99 (rows).
- Ground truth: `dmp_pm.c` full (109) + `pm/mproc.h:28-99` (consumed fields + 11 flags) +
  `timers.h` + `libsys/getticks.c` + `sigtypes.h:61-62` + `com.h:59` (PM=0); Rust `dump_pm.rs` HEAD
  (PmCursor, snapshot import, two renderers, uptime channel) re-derived.
- Knowledge rows: K-035 — roll-call walkthrough (filter `mp_pid==0`, 22-page, `--more--\r`), stock;
  K-036 — signal table + alarm-remainder arithmetic (`tmr_exp_time - uptime`, snapshot-time caveat
  across pages), stock; difference-table seed rows (filter/paging/encoding deltas vs the other three
  SI dumps — completed by 07/08/09 quoting back), stock pattern from 07's existing "intentional
  duplication" note, promoted to an explicit table.
- Acceptance: reader can implement any SI-family dump from this document + 04; the "vs PM" rows in
  07/08/09 match this document's table verbatim; §5 refreshed.

### 07 — VFS dump domain (domain, group delta)

- Mission: own the two-table VFS consumer and the dense-vs-sparse paging contrast
  (paged roll call vs one-screen map).
- Covers: K-037 (`fproc_dmp`), K-038 (`dtab_dmp`), K-024 consumer half (two SI codes), O-04
  inherited SDEV TODO marker, K-041 VFS half (intentional PmCursor duplication stated as policy).
- Non-covers: channel mechanics → 04; VFS internals → 05-stage-vfs; SDEV endpoint design → VFS
  crate (07 marks, does not fix).
- Prerequisites: 04 (both SI codes), 06 (representative pattern — "vs PM" difference table entry
  point), 03 (SF3/SF4 keys).
- Post-refs: 09 (sparse-table contrast — dmap one-screen vs DS stock-take), 99 (rows).
- Ground truth: `dmp_fs.c` full (83) + `vfs/fproc.h:16-114` + `vfs/dmap.h:16-18` + `vfs/const.h`
  (BLOCKED_ON/LABEL_MAX) + `com.h:60` + `dmap.h:82` (NR_DEVICES 135) + `syslimits.h` OPEN_MAX 255;
  Rust `dump_vfs.rs` HEAD (VfsCursor three-state `VfsAction`, FProcSnap `nfds`/`fp_cdev_endpt`
  extensions from Fix #6, two renderers) re-derived.
- Knowledge rows: K-037 — fd-counting walkthrough + FP-bit table + CDEV/SDEV branch (with the SDEV
  TODO box quoting the C comment), stock; K-038 — map walkthrough (NONE skip, 135 slots, one
  screen), stock; "vs PM" delta rows (same SI family, second code, fd-counting instead of flags,
  skip-vs-filter), stock pattern made explicit.
- Acceptance: reader can state when to page and when to print one screen, with the data-volume rule
  quoted by 09; SDEV marker present and un-designed; §5 refreshed.

### 08 — RS dump domain (domain, group delta)

- Mission: own the dual-table join (identity + status) and the dual-source encoding.
- Covers: K-039 (`rproc_dmp` + `s_flags_str`), K-024 consumer half (two SI codes, all-or-nothing
  fetch), K-041 RS half (RsCursor IN_USE-skip as the fourth cursor shape).
- Non-covers: channel mechanics → 04; RS internals → 03-stage-rs; full rproc authority → RS crate.
- Prerequisites: 04 (dual-fetch short-circuit `dmp_rs.c:33-34`, SI codes), 06 ("vs PM" pattern), 03 (SF6 key).
- Post-refs: 09 (fourth cursor shape feeds the taxonomy), 99 (rows).
- Ground truth: `dmp_rs.c` full (74) + `rs/type.h:63-79` + `rs.h` (LABEL 16, rprocpub, SF_USE_*) +
  `rs/const.h` (CMD 512, RS_*) + `com.h:61`; Rust `dump_rs.rs` HEAD (RprocSnap `r_args[512]`
  extension from Fix #6, RsCursor, renderer) re-derived.
- Knowledge rows: K-039 — join walkthrough (slot-aligned PUB+PRIV rows, IN_USE filter, 22-page),
  stock; dual-source encoding table (RS_* + SF_USE_* → 6 chars), stock; all-or-nothing fetch note
  (both tables or neither — single `rs_tables` method post-V1), stock updated to Fix #5 shape;
  "vs PM" delta rows (two codes, join vs single, dual-source vs single-source encoding), explicit.
- Acceptance: reader can explain why two fetches behave as one and what each of the 6 flag chars
  means; §5 refreshed.

### 09 — DS dump domain (domain + cursor-taxonomy close)

- Mission: own the DS stock-take and close the stage's cursor taxonomy (four shapes, merge policy).
- Covers: K-040 (`data_store_dmp`), K-024 consumer half (SI_DATA_STORE + DS A-10 consumer-half
  marker), K-041 taxonomy (primary — the comparison table for all five cursors + merge-trigger
  policy from V1-P3-5), K-030/K-038 contrast (conditional-bound vs pre-compare vs one-screen).
- Non-covers: channel mechanics → 04; DS internals + producer side → 07-stage-ds (A-10 production
  half); full store authority → DS crate.
- Prerequisites: 04 (SI_DATA_STORE), 06 ("vs PM" pattern), 07 (sparse contrast), 08 (fourth shape).
- Post-refs: 10 (taxonomy consumer — "VM uses the batch-dual shape"), 99 (rows).
- Ground truth: `dmp_ds.c` full (52) + `ds/store.h:12-29` + `ds.h:12-29` (DSF_*/KEYLEN) +
  `sys_config.h` NR_SYS_PROCS + `com.h:65`; Rust `dump_ds.rs` HEAD (DsCursor conditional-bound +
  early-return replay, renderer) re-derived.
- Knowledge rows: K-040 — stock-take walkthrough (resume-at-`prev_i`, skip unused, 4-type rows,
  wrap to zero, `--more--\r`), stock; K-041 — taxonomy table (shape × documents × break condition
  × resume semantics) + merge policy ("three break shapes mirror three C sources; merge only when
  two cursors change together"), stock from todo V1-P3-5, promoted to primary text.
- Acceptance: reader can place any of the five cursors in the taxonomy and state the merge trigger;
  10 quotes the batch-dual row verbatim; §5 refreshed (incl. the resume/replay tests).

### 10 — VM dump domain (domain, most complex)

- Mission: own the batch state machine and region folding — the unbounded-data answer.
- Covers: K-043 (`vm_dmp` state machine), K-044 (`print_region` folding), K-025 consumer half
  (three vm_info verbs as used), K-026 batch-item half (per-process error → warn + continue),
  K-041 batch-dual row (by pointer + VM delta: `prev_base` region offset).
- Non-covers: channel mechanics → 04; VM internals → 02-stage-vm; region semantics → VM crate.
- Prerequisites: 04 (vm_info verbs + region triple), 09 (taxonomy — batch-dual row), 03 (F8 key).
- Post-refs: 99 (rows + model example of warn-continue inside a batch).
- Ground truth: `dmp_vm.c` full (157) + `vm.h:40-71` (3 structs) + `mman.h:62-65` (PROT bits);
  Rust `dump_vm.rs` HEAD (BatchCursor + FoldState, stats/usage/region outlets, renderer) re-derived.
- Knowledge rows: K-043 — state-machine walkthrough (stats first screen → proctab → per-process
  usage + region batches; `prev_i`/`prev_base`; header-fit guard; `n > LINES` internal-error trip),
  stock; K-044 — folding walkthrough (repeat detection across `vri_prev`, flush line, NULL sentinel
  end-of-list), stock; batch-error table (which failure continues vs voids), stock.
- Acceptance: reader can trace a two-process, multi-batch dump across screens with cursors; the
  header-fit guard and the internal-error trip are both stated with line anchors; §5 refreshed.

### 99 — Global concepts close (close)

- Mission: close the stage with checkable tables: constant values, error gates, execution model,
  exclusions with proofs, neighbour map. No mechanism teaching.
- Covers: K-045 (9-family value tables — primary, expanded from name-only to name+value+site),
  K-046 (error gate table — expanded from names to code→gate→call-site rows), K-047 execution model
  (primary, D-06), K-048 exclusions with grep proofs (primary), K-050 neighbour map (table of
  producer stage per dump), K-003/K-014/K-020/K-026 single rows + pointers (no re-teaching).
- Non-covers: everything taught in 00–10 (99 rows point back; the one-paragraph-per-family limit is
  enforced to prevent drift into a second teaching).
- Prerequisites: all (99 is the last document; its Prerequisites field lists 00–10 as a set).
- Post-refs: none inside the stage (99 is terminal); outward pointers to the six neighbour stages'
  producer documents (listed, not taught).
- Ground truth: `com.h` value lines (NOTIFY_MESSAGE 0x1000, TTY_PROC_SLOT, FKEY_*, GET_* numbers,
  DIAGCTL codes, VMIW_*, VM_INFO), `keymap.h` ranges, `sysinfo.h` SI numbers, `signal.h:67`,
  `errno.h:199`, per-`dmp_*.c` LINES/MORE constants (quoted from the dump documents, not re-extracted),
  `glo.h` + `inc.h:7` + `rg` zero-consumer proofs, `os/servers/is/src/` local constants
  (`dispatch.rs:20/24`, `sef.rs:16`) with the single-authority verdict re-checked at write time,
  VFS `misc.rs` SI copy deletion status (E-ISPROD item — stated as open or closed with date).
- Knowledge rows: K-045 — nine value tables (family × C site × value × Rust authority), expanded
  from the current name-only table; K-046 — gate table (code × gate × C call site × IS observable),
  expanded; K-047 — model box (single-threaded loop, `!Send` sound, zero-alloc, no `Rc`/`RefCell`,
  kernel SMP+BKL contrast), stock, shrunk sources elsewhere (D-06); K-048 — exclusion ledger
  (item × proof command + output × why no runtime semantics), stock, kept; K-050 — neighbour table
  (dump → producer stage → producer document), stock.
- Acceptance: every magic number in 00–10 resolves to a 99 row; every error code in 00–10 resolves
  to a gate row; exclusion rows each carry a re-runnable proof; single-authority verdict re-verified
  (zero cross-crate duplicates except the tracked VFS item).

## 6. Change Table (old → new; every stock row has a destination, every new row a source)

Operation kinds: **R** = rebuild in place (same number; old body is source material, new body is
written from pool + C + Rust truth); **A** = archive (leave the formal catalog); **U** = update a
process record (not a numbered document). There is deliberately no reorder / split / merge / new-number
operation (reasons: Section 4.5).

| Op | Kind | Old position | New position | Reason | Knowledge rows carried | Direction note |
|----|------|--------------|--------------|--------|------------------------|----------------|
| C-01 | R | 00 (60-line nav stub) | 00 (full entry, ~250 lines) | G-05: no chain table, no paths, no position answers | K-001 (promoted here), K-002 chain table, K-003 absence table, K-004, K-047 positioning, K-050 pointer | Stock: consolidate from 00 §1–4 + 01 + plan §1.2. New: chain-table form (source: C + rc + conf anchors). |
| C-02 | R | 01 (707-line mixed-era body) | 01 (birth+loop, ~600 lines, re-derived §3/§4) | G-04 (stub-era base + V1 notes) + G-01 mechanism half + G-03 §5 | K-002 mech, K-003 ban, K-005–K-008, K-015, K-020, K-028 consumer, K-047 ownership | Stock: §2 C-walkthrough kept as quarry. New: HEAD-derived §3/§4 (source: `lib.rs`/`sef.rs`/`state.rs` HEAD). |
| C-03 | R | 02 (572-line protocol) | 02 (protocol, ~550 lines, re-derived Rust half) | G-04 (tty_fkey.rs HEAD: warn_fkey_ctl, void map) + peer-fence hardening (O-01) | K-009–K-015 | Stock: §§1–2 kept as quarry. New: Fix #1 shapes as primary text (source: `tty_fkey.rs` + `sef.rs` HEAD). |
| C-04 | R | 03 (360-line dispatch) | 03 (dispatch + journey, ~350 lines, re-derived run_dump) | G-04 (`run_dump` filled ×16, Fix #4/#6) | K-016–K-020, K-042 consumer | Stock: hooks/handler/naming quarry. New: 16-arm dispatch + journey pointers (source: `dispatch.rs` + `lib.rs` HEAD). |
| C-05 | R | 04 (492-line framework) | 04 (framework, ~600 lines, authority language) | G-02 (build note) + G-07/G-08 (landed channels) + G-04 (typed outlets primary) | K-021–K-028, K-034, K-050 fetcher half | Stock: five-channel exposition kept as quarry. New: K-027, K-028 primary, K-023/K-034 rewritten (sources: Makefile, `acquire.rs` HEAD, minix-types authorities). |
| C-06 | R | 05 (323-line kernel domain) | 05 (kernel audit, ~700 lines) | 8 seats under-taught at 323 lines + G-07/G-08 seat rewrites + G-04 renderers | K-022/023 consumer, K-029–K-034, K-042 kernel renderers | Stock: seat walkthroughs kept as quarry. New: per-seat derivations + landed-channel seats (sources: `dmp_kernel.c`, `dump_kernel.rs` HEAD, minix-types). |
| C-07 | R | 06 (191-line PM) | 06 (representative, ~250 lines + difference seed) | Promote to group representative (Section 4.3); G-04 cursor/renderer refresh | K-024 consumer, K-035, K-036, K-041 PM half | Stock: body kept as quarry. New: "vs PM" seed table (source: comparison of the four SI dumps' C sources). |
| C-08 | R | 07 (177-line VFS) | 07 (group delta, ~250 lines) | Difference-table form; G-07 VFS re-export status; G-04 VfsAction/`nfds` refresh | K-024 consumer, K-037, K-038, O-04 marker | Stock: body kept as quarry. New: delta rows vs 06 (source: `dmp_fs.c` vs `dmp_pm.c` diff + `dump_vfs.rs` HEAD). |
| C-09 | R | 08 (153-line RS) | 08 (group delta, ~250 lines) | Difference-table form; G-04 `rs_tables` single-method + `r_args` refresh | K-024 consumer, K-039, K-041 RS half | Stock: body kept as quarry. New: delta rows vs 06 (source: `dmp_rs.c` vs `dmp_pm.c` + `dump_rs.rs` HEAD). |
| C-10 | R | 09 (174-line DS) | 09 (stock-take + taxonomy, ~300 lines) | Taxonomy promotion (K-041 primary); G-04 DsCursor refresh | K-024 consumer, K-040, K-041 primary | Stock: body + §2.3 comparison kept as quarry. New: taxonomy table + merge policy (source: five cursor implementations + V1-P3-5). |
| C-11 | R | 10 (208-line VM) | 10 (batch machine, ~350 lines) | Deepest cursor deserves full derivation; G-04 BatchCursor/FoldState refresh; batch-error table | K-025 consumer, K-026 batch half, K-041 batch row, K-043, K-044 | Stock: body kept as quarry. New: state-machine trace + error table (sources: `dmp_vm.c`, `dump_vm.rs` HEAD). |
| C-12 | R | 99 (58-line close stub) | 99 (checkable close, ~300 lines) | G-06: values, gates, proofs missing | K-003/K-014/K-020/K-026 rows, K-045–K-048, K-050 map | Stock: constant/error/model/exclusion rows kept as quarry. New: value tables + gate table + re-verified authority verdict (sources: headers + `rg` proofs at write time). |
| C-13 | A | `draft/` (9 files) | `draft/` archived, uncited | Rebuild-not-move premise: the draft material served (line-by-line base for 01/03/05–10 now superseded by two review generations + V1 fixes); retaining as citable source invites stale-quote drift | — (all draft knowledge already re-homed in K-rows via the documents that consumed them) | Stock direction only: nothing moves out of `draft/` again; B-phase cites C + Rust + pool, never `draft/`. No deletion (history preserved). |
| C-14 | U | `plan.md` §6.1 status table (says 00 skipped, 99 pending; §3.5 stub baseline; §4 A-3/A-6 "to design") | Status + baseline + ARCH rows corrected | Process record, not a numbered doc: states facts the rebuild changes (Fix #8 already landed 00/99; tests 86→125; A-3/A-6 landed) | Pointers to G-03/G-07/G-08 | Update-in-place with dated correction rows; plan's architecture (§1–§5) is otherwise affirmed, not rewritten. |

- Delete-with-reason list (the only knowledge destroyed, all with exclusion homes): ARM `proctab_dmp`
  body (→ 05 exclusion, A-7); `click_to_round_k` (→ 05 exclusion); `diag_buf` family + `DIAG_BUF_SIZE`
  + `_SYSTEM` (→ 99 exclusion ledger); Makefile beyond CPPFLAGS (→ 99 exclusion line); TTY-internal
  detail beyond the fkey face (→ O-01, never taught); producer internals (→ O-02/O-03, never taught).
  Ten-change spot-check for G6 (Section 9) draws from C-02, C-05, C-06, C-08, C-10, C-12.

## 7. Missing-New Mapping (Section 3.2 gaps closed one by one; this section may not be left empty)

- G-01 (boot chain, no owner) → new 00 (reader chain table) + new 01 (mechanism: env_setargs, SEF
  registrations, dynamic endpoint injection, `service is` perms, ping invisibility). Acceptance: the
  three preconditions + the two evidences (`rc` line, conf block) appear in 00; the five mechanism
  steps appear in 01 with C + Rust anchors. No new document (Section 4.5.5).
- G-02 (CPPFLAGS visibility) → new 04 subsection (K-027: which `-I` exposes which header to which
  dump; `USE_APIC`→IRQ-width note). Acceptance: a reader can explain why `dmp_fs.c` includes
  `vfs/fproc.h` while `dmp_kernel.c` includes `kernel/proc.h`. Raw material: `servers/is/Makefile`.
- G-03 (test staleness) → every new 01–10 contract §5 (refresh duty: re-run `cargo test -p minix-is`,
  re-list named tests, re-count; the 86 figure must not survive anywhere). Acceptance: `rg "86 passed"
  08-stage-is/*.md` returns only historical mentions (plan/todo ledgers) after B-phase.
- G-04 (Rust §3/§4 patchwork) → rewrite-from-truth duty in contracts 01–10 (re-derive from
  `os/servers/is/src/` HEAD + minix-types authorities; V1-note appendices are removed, their facts
  absorbed as primary text). Acceptance: no "V1 update note" appendix survives; every quoted
  signature re-greps at write time.
- G-05 (00 stub) → contract 00 (Section 5). Acceptance: Section 5's 00 acceptance box.
- G-06 (99 stub) → contract 99 (Section 5). Acceptance: Section 5's 99 acceptance box.
- G-07 (E-ISPROD landed) → new 04 (authority language + import paths) + new 05/07 (import-based
  snapshots; PM/RS/DS/VM alignment still marked pending with edge pointers). Acceptance: no
  "A-4 proposal" future-tense survives for kernel/VFS; pending producers each carry an E-ISPROD
  pointer, not a design.
- G-08 (E-ISKMESS landed) → new 04 (GET_KMESSAGES=7 channel spec) + new 05 (kmessages seat against
  the real channel + `KernelKmessTransport` consumer note). Acceptance: no "A-3 to-be-designed"
  survives; F7 chain test named in 04/05 §5.

## 8. Anchor Migration and Breakage Cost (price the breakage before rebuilding)

### 8.1 Section-migration table (every changed old document, section by section)

| Old position | Old content in one sentence | New position | Migration kind | Breakage note |
|---|---|---|---|---|
| 00 §§1–4 (purpose, boot line, no-image note, model note) | Thin nav + scattered chain facts | 00 §§1–4 (purpose thesis, chain table, absence table, nav) | Rewrite (quarry) | Old 00 has no section anchors cited elsewhere (only plan/todo mention it by file); low risk. |
| 00 §5 nav table (01–10 one-liners) | Number→topic map | 00 §5 (map + main/branch/skippable paths) | Rewrite | Same file; in-stage pointers updated to new section numbers at write time. |
| 01 §1 (concept: why a separate service) | Audit-discipline motivation | 00 §1 (thesis, promoted) + 01 §1 (pointer back) | Split (promotion) | 01 §1.1 quoted by no code comment; move is safe. |
| 01 §2 (C walkthrough: main/loop/refusal/ping) | Loop mechanics vs stub-era Rust | 01 §2 (kept structure, refreshed facts) + 04 (error-surface half) | Rewrite | `dispatch.rs:40,68` comments cite "01 §2.3" (classifier orientation + silent-notifier rule) — new 01 must keep a §2.3 with the same two rulings or update 2 comment sites. |
| 01 §§3–4 (design + impl, stub era + V1 notes) | ENOSYS-stub design + appended fixes | 01 §§3–4 (HEAD-derived) | Rewrite | `lib.rs:7-8`, `main.rs:4`, `sef.rs:1,254-262`, `state.rs:1`, `tty_fkey.rs:168` cite "01 §3/§4.1/§4.2/D2" — 9 sites; new 01 keeps §3 D-series numbering stable (D1–D5 + V1 notes absorbed) to avoid touching code comments. |
| 02 §§1–2 (pull model + wire + peer) | Protocol exposition (current, best-aged doc) | 02 §§1–2 (same shape, HEAD-refreshed Rust half) | Rewrite (light) | No code comment cites 02 by section; only 01/03 prose pointers — cheap. |
| 02 §4.3 invariant 2 (MAP-failure warning) | Landed invariant + test | 02 §4.3 (unchanged text, refreshed test name) | Verbatim carry | Test name may change in B-phase; 02 §5 and the invariant box must be edited together (same commit). |
| 03 §§1–3 (dispatch + journey stub) | Table/loop/naming + journey | 03 §§1–3 (same shape, 16-arm run_dump + full journey map) | Rewrite (light) | `dispatch.rs:53` cites "03 §4.1" (`do_fkey_pressed` arm) and `:7` cites "03 §4.1" (table) — new 03 must keep a §4.1 owning the table + arm, or update 2 sites. Per-domain `dump_*.rs:1` cites "0X §4.1" each — new 05–10 all keep a §4.1 owning the renderer (6 sites protected by convention). |
| 04 §§2–4 (channels + error + seam) | Five channels + pre-V1 seam | 04 §§2–4 (channels refreshed, seam replaced by K-028 primary, +K-027 note) | Rewrite | No code comment cites 04 by section (only prose); moderate risk, all inside docs. |
| 05 §§2–4 (8 seats + paging + stub-era snapshot prose) | Seat walkthroughs + idiom | 05 §§2–4 (seats incl. 2 rewritten, idiom kept, snapshot imports) | Rewrite | `dump_kernel.rs:1` cites "05 §4.1" — keep §4.1. 06/07/10 prose pointers to 05's paging section: new 05 keeps the idiom in §2.4 (same number) to hold 3 pointers. |
| 06/07/08 §§2–4 (seat bodies + cursors) | Per-dump walkthroughs | Same shapes + "vs PM" delta rows + HEAD cursors | Rewrite (light) | `dump_pm.rs:1`, `dump_vfs.rs:1`, `dump_rs.rs:1` cite "0X §4.1" — kept. `dump_vfs.rs:152-154` duplication-policy comment pairs with 09's taxonomy — edit together. |
| 09 §2.3 (cursor comparison) | Three-cursor contrast | 09 §2.3 (five-cursor taxonomy table + merge policy) | Expand | 10 cites it ("09 §2.3 closing row"); new 09 keeps the taxonomy in §2.3. |
| 10 §§2–4 (batch machine + folding) | State machine walkthrough | Same shape, HEAD BatchCursor/FoldState + error table | Rewrite (light) | `dump_vm.rs:1` cites "10 §4.1" — kept. |
| 99 §§1–5 (names-only tables + model + exclusions) | Close stub | 99 §§1–5 (value tables, gate table, model, proofs, neighbour map) | Rewrite | 00/01 prose pointers to 99 ("model close", "value table") resolve to new §2 (errors) / §3 (model) — pointer targets recorded here so B-phase fixes them in one pass. |
| plan.md §6.1 + §3.5 + §4 A-rows | Stale process facts | Same sections, dated correction rows | Update | plan.md is cited by 00 §7/99-adjacent "See also" lines — keep the cited section numbers stable. |
| `draft/` 8 tmp files | Line-by-line material (superseded) | Archived (no new citations; history kept) | Archive | `draft/README.md` is cited by the current 00 header ("draft material" pointer) — new 00 drops the draft pointer (C-13), closing the last live reference. |

### 8.2 Reference-migration table (everything that cites an old number or filename)

| # | Citing location (old reference) | Old target | New target | Verification |
|---|---|---|---|---|
| R-01–R-09 | `os/servers/is/src/` comments citing "01 §2.3/§3/§4.1/§4.2/D2" (dispatch.rs:40,68; lib.rs:7-8; main.rs:4; sef.rs:1,254,258,262; state.rs:1; tty_fkey.rs:168) | 01 current sections | 01 new §§2.3/3/4.1/4.2/D-series (numbers held stable by contract) | `rg "01-is-init-main" os/servers/is/src/` re-run at B-phase end; zero dangling. |
| R-10–R-17 | `dispatch.rs:7,53` ("03 §4.1"); `dump_kernel.rs:1` ("05 §4.1"); `dump_pm.rs:1` ("06 §4.1"); `dump_vfs.rs:1` ("07 §4.1"); `dump_rs.rs:1` ("08 §4.1"); `dump_ds.rs:1` ("09 §4.1"); `dump_vm.rs:1` ("10 §4.1") | 03/05–10 §4.1 | Same §4.1 numbers in the rebuilt docs (contract-mandated) | Same `rg` family per doc; 8 sites. |
| R-18 | `dump_vfs.rs:152-154` duplication-policy comment ↔ 09 taxonomy | 09 §2.3 (3-cursor) | 09 §2.3 (5-cursor table) | Co-edit in one commit; quote the merge-trigger line both sides. |
| R-19 | In-stage prose pointers (01→02/03, 02→03, 03→05–10, 04→05–10 routing, 05→06/09, 06→07/08/09, 09→10, all→99) ≈ 40 pointers | Current section numbers | Section 8.1's held-stable numbers (§2.3, §4.1, §2.4-paging, §2.3-taxonomy) or the recorded new targets | B-phase checklist: `rg "§[0-9]" 08-stage-is/0*.md 08-stage-is/99*.md` resolved one by one. |
| R-20 | `plan.md` ↔ docs ("See also" both directions, ~6 links) | Current §§ | C-14 holds plan's cited numbers stable | Re-read plan §8 "See also" at B-phase end. |
| R-21 | `todo.md` Fix #1–#8 doc-sync notes (01 §3/§4.2/§5.2, 02 §4.3, 04 §3/§4/§5, 03/05–10 §3 notes) | Pre-rebuild text | Absorbed as primary text (notes removed) | Each B-phase commit deletes its note line; `rg "V1.*(update|note)" 08-stage-is/0*.md` must empty (except todo.md history, which stays). |
| R-22 | Cross-stage inbound pointers to 08-stage-is (edge_todo E-IS* entries; 07-stage-ds/04-stage-pm/03-stage-rs todos; master README stage table) | Current 08 numbers/titles | Unchanged numbers + unchanged titles → no migration needed | Titles frozen by Section 4.1 precisely to zero this cost; verify with `rg -l "08-stage-is" notes/rewrite/ \| wc -l` before/after (expect equal, minus archived draft pointer). |
| R-23 | "86 passed" test-count claims in 10 documents' §5 | Stale count | Then-current count + named tests (G-03 duty) | `rg -n "86 passed" 08-stage-is/0*.md 08-stage-is/99*.md` must return ∅ after B-phase. |

### 8.3 Breakage-cost summary

- Affected references total: ~17 code-comment sites (R-01–R-18) + ~40 in-stage prose pointers (R-19) +
  ~10 process-ledger lines (R-20/R-21) + ~10 stale test claims (R-23) ≈ **77 touch points**.
- Hotspots: new 01 (§2.3/§4.1/§4.2/D-series — 9 code sites depend on these numbers) and the §4.1
  convention (8 code sites across 03/05–10). Both are protected by contract (numbers held stable), so
  the expected code-comment edit count is **zero** if B-phase honours the contracts — the single
  most valuable constraint in this blueprint.
- Suggested batch method: (1) write new 01 first and freeze its §2.3/§3/§4 numbers; (2) write new
  03/05–10 with §4.1 frozen; (3) mechanical `rg` sweep per R-19/R-23 checklist; (4) absorb V1 notes
  (R-21) one document per commit with `rg` proof in the message; (5) final `rg -l "08-stage-is"`
  count comparison for R-22. No sed bulk-rewrite of prose (number stability makes it unnecessary).

## 9. Verification Gates and Self-Check (mechanical, not by feel)

| Gate | Check | Result |
|------|-------|--------|
| G1 | C true order re-checkable (10 anchors spot-checked) | PASS — 10/10 resolve (Section 1 closing line; commands in Section 0). |
| G2 | Pool completeness: every C file and non-C artifact has a home or an explicit exclusion with reason | PASS — 8 `.c` + 3 headers route to 01/02/03/05–10 (Section 5 ground-truth rows); Makefile routes to 04-note + 99-exclusion (N-01/N-05); no-asm recorded (N-03); rc/conf/ping route to 00/01 (N-04); wire/errors/shutdown/concurrency/tests route per N-06–N-10. |
| G3 | Zero forward references in the new catalog (Prerequisites scan) | PASS — chain 00→01→02→03→04→{05,06}→{07,08,09}→10→99; the 07/08/09 set takes only 04+06; 10 takes 04+09; 99 takes all. No contract lists a higher-numbered prerequisite. |
| G4 | Dependency graph acyclic; any cycle gets a split plan | PASS — DAG verified above; no cycle, no split plan needed. |
| G5 | 100% pool coverage: every K-row has a destination or a delete reason; every new row has evidence | PASS — 50/50: 42 stock rows land in Sections 5–6 (C-01–C-12), 8 new rows (K-027/028/034/042/049 + three rewritten) each carry C/Rust/command anchors; the 6 destroyed items (Section 6 closing list) each carry a delete reason + exclusion home. |
| G6 | Every rebuild states stock destinations; every new shaping states new sources (10-spot check) | PASS — checked C-02 (01: §2 quarry + HEAD §3/§4 sources named), C-05 (04: quarry + Makefile/`acquire.rs`/minix-types named), C-06 (05: quarry + `dmp_kernel.c`/`dump_kernel.rs`/authorities named), C-08 (07: quarry + diff sources named), C-10 (09: quarry + five implementations + V1-P3-5 named), C-12 (99: quarry + header/`rg` sources named), C-01/C-03/C-04/C-11 (same pattern, verified while writing). 10/10. |
| G7 | Every contract has all seven elements (mission, covers, non-covers, prerequisites, post-refs, ground truth, rows + acceptance) | PASS — 12/12 in Section 5 (each with acceptance box; rows tables carry id/type/anchor/why-here/source). |
| G8 | Migration covers every changed document's sections + doc/code-comment references | PASS — Section 8.1 covers all 12 docs + plan + draft (15 rows); Section 8.2 covers ~17 code sites + ~40 prose pointers + ledgers + counts (R-01–R-23). |
| G9 | Factual claims carry anchors (10-spot check; guesses labelled) | PASS — checked: 1151-line total (wc), 16 hooks (dmp.c:18-35), LINES 22/24 (both files), `IS_PROC_NR` ∅ (rg), boot_image absence (table.c:44-64), rc line (rc.minix:117), conf block (system.conf:271-277), SI_* values (sysinfo.h), GET list (com.h:316-331), 125 tests (cargo run). Zero guesses presented as fact; the two open points below are labelled questions, not claims. |

- Conclusion: blueprint **COMPLETE and executable** — B-phase needs no further triage decisions, only
  writing labour in Op order C-02 → C-03/C-04 → C-05 → C-06/C-07 → C-08/C-09/C-10 → C-11 → C-01/C-12
  → C-13/C-14 (skeleton before domains before entry/close; 01 frozen first per Section 8.3).
- Questions for the user (decisions, not defects — B-phase can start before they are answered, defaults
  in brackets):
  1. Catalogue verdict: keep all 12 numbers with in-place rebuilds [default YES] vs any merge/split
     you still want (name it — the blueprint's Section 4.5 argues against, but the call is yours).
  2. Representative choice: PM (06) as the fully-worked SI example [default YES] vs VFS or RS.
  3. Test baseline: refresh each §5 to the then-current `cargo test -p minix-is` count at write time
     [default YES] vs freezing a single stage-wide count up front (risks instant staleness again).

---

*End of blueprint. Single landed file: `notes/rewrite/fork-syscall-rewrite/08-stage-is/doc_rerank_muse.md`
(this file). No body text modified. No `.design/` or `tmp_design_and_todo/` cited. No other AI's
`doc_rerank_*` product read. Written in English per the muse requirement.*
