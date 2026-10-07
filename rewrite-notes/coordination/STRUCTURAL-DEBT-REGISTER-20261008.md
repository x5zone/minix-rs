# Structural Debt Register — Minix3-in-Rust rewrite, bring-up line

Snapshot: commit `e24963e55`, 2026-10-08, working tree of `/home/xzhao/github/minix-rs` (branch `rewrite`).
**Revision 3. This document fully supersedes `STRUCTURAL-DEBT-REGISTER-20260930.md` (Revisions 1–2), which
is reduced to a pointer stub.** It carries forward every entry of the superseded register — updated,
re-verified against the code at this snapshot, and closed or re-ranked where October's execution wave
decided or falsified them — and adds the debts discovered since. It was produced by a read-only
debt-hunting session (no production code changed): five scan passes over the coordination corpus plus a
grep-level re-verification of every load-bearing claim (Appendix B). It is written to be self-contained:
an external reviewer with no access to the internal logs should be able to evaluate both the debts and
the proposed solutions from this document alone.

**What changed between the two snapshots (2026-09-30 → 2026-10-08).** All three terminal goals of the
bring-up campaign were achieved and independently re-certified by the review line on a clean checkout:
x86_64 (boot marker stable at `-smp 4`), aarch64 (ATF suite 36/36), riscv64 (boot + command face, then
36/36 on 2026-10-05; the last blocker was a riscv `_start` missing TLS `tp` initialization, not a
structural fault). The worklog grew from ~8,800 to 14,189 lines (entries 续-89 … 续-439+). Two new ledgers
were created and are now the operative registers: `PENDING-DECISIONS-3ARCH-PARITY.md` (34 frozen
decisions, PD-01…PD-34) and `TODO-3ARCH-PARITY-20261006.md` (the claim ledger, P-* items). The old
register was adopted by the main thread as its design-level authority ("择机按 §5.1 认领") and six of its
items were closed in the wave. This register reconciles all three layers.

---

## 0. How to read this document

- **Debt IDs.** `SD-1` … `SD-43`, grouped by architectural area. IDs are stable across register
  revisions: an SD entry here is the same debt as in the 2026-09-30 register. New since Revision 2:
  SD-34 … SD-43 (Part H).
- **Status vocabulary.** `open` (recorded, nothing landed) · `stopgap landed` (a temporary form works,
  the principled fix does not exist) · `partially fixed` · `adjudicated` (a frozen decision exists in the
  PD register; where implementation has not started, the entry says so) · `closed` (fixed and verified —
  the entry records what closed it and any residual) · `dropped-from-ledgers` (the item has no row in
  either operative ledger — the highest re-registration risk, called out explicitly).
- **The three-layer ledger landscape** (who owns what):
  1. **This register** — the design-level superset: why each debt is structural, the option space
     (mechanism, reference frames, cost), and the recommendation. 
  2. **`PENDING-DECISIONS-3ARCH-PARITY.md`** — the decision authority: 34 entries, frozen after ten
     rounds, each with the chosen option, rejected options, and reasons. Where a PD decision exists, this
     register does not re-adjudicate; it records the decision and tracks only the implementation gap.
  3. **`TODO-3ARCH-PARITY-20261006.md`** — the claim ledger (P-* items, work-in-progress tracking).
  Conflicts between layers are listed in §7 (one exists: PD-29 vs worklog 续-397).
- **Anchors.** `path:symbol` anchors; line numbers are tool-derived at this snapshot and will drift —
  the symbol is the stable part. Appendix B records the verification commands and verdicts.
- **`[ARCH: ...]` tag.** The repository's convention for architectural evolution: the change must be
  marked identically in the design doc, design notes, and code.

---

## 1. What the system is

**The artifact.** A semantic rewrite of the Minix3 operating system in Rust (`#![no_std]`), under `os/`.
Minix3 is a microkernel OS: drivers and OS services are user processes talking by synchronous messages.
The C tree `minix3/` is ground truth; external behavior is preserved while internals are re-expressed in
Rust.

**The components.** `os/kernel` (microkernel: IPC, scheduling, traps, syscalls; SMP with a big kernel
lock); `os/servers/` (pm, vfs, vm, rs, ds, sched, devman, input, is, mib, ipc-server); `os/fs/`
(filesystems + `fs-rt` transport runtime); `os/boot-shim` (UEFI / OpenSBI-U-Boot loaders); `os/kernel-image`
(standalone linked kernel ELF; since the last snapshot it grew two *self-boot faces*, see SD-1);
`os/libs/` (minix-rt runtime+allocator+crt0, minix-sys syscall/IPC ABI, minix-types, minix-boot,
minix-platform, minix-sef); `os/xtask` (build/image automation — including per-invocation codegen flags,
see SD-23); `os/qemu-tests` (machine gates); `os/tests` (cross-crate integration scenarios).

**The campaign.** Drive x86_64 / aarch64 / riscv64 each through cold boot → INIT → rc marker → command
face → the Minix3 ATF test suite on QEMU. **All three are achieved**; the riscv suite gate runs at
`-smp 2` (36/36). The current frontier is goal ③ (the wider minix3 test surface / 18-stage parity), the
SEF framework completion line (T1–T7 executed, 续-408…439), a workspace-stability defect (T-STAB-1,
SD-30), and one open research-grade case: the riscv non-deterministic memory-corruption writer
("问题乙", PD-16 / SD-10), which is a written precondition for true SMP.

**Why a structural-debt register.** The line's logs separate one-off correctness bugs (routed to fix
rounds) from *structural debt* — design-level deficiencies that keep generating bugs or maintenance cost
and whose remedy is a design decision, not a patch. The 2026-09-30 register became the line's
design-level authority; October's wave closed six of its entries, froze decisions covering a dozen more,
and surfaced ten new ones. Registers rot at ~20% per audit cycle (SD-31) — this document is the
scheduled refresh.

---

## 2. Where every debt came from

Sources scanned (base `rewrite-notes/coordination/` unless noted), 2026-10-08:

| Source | Yield |
|---|---|
| `STRUCTURAL-DEBT-REGISTER-20260930.md` (Rev 1–2, 33 entries) | carried forward in full |
| `PENDING-DECISIONS-3ARCH-PARITY.md` (698 lines, 34 frozen decisions + re-anchor gate) | adjudication status for ~15 SD items; hard-order sequencing |
| `TODO-3ARCH-PARITY-20261006.md` (433 lines, 23 P-items + exclusions) | claim-ledger statuses; P-ALL-01/06/07/12 new items |
| `NK4C-BUG-RISCV64-MEMORY-CORRUPTION.md` (772) | the open 问题乙 case file: F1–F11 below |
| `NK4C-BUG-RISCV64-TRANSIENT-PTE.md` (1078) + `riscv瞬态页表崩溃取证方法论.md` (164) | closed case: canonical-address root cause, methodology corpus, asset inventory |
| `NK4C-RETRO-AUDIT-20261001.md` (203) | AF-1…AF-12 residues (FPU family, workspace compile face, pattern candidates 84–86) |
| `ADDRESS-CONSTANT-AUDIT.md` (237) | user-VA constant audit: A1–A4 fixed, A5–A13 open |
| `NK4C-WORKLOG.md` (14,189 lines; delta 续-89…续-439 read via head blocks, all 38 `SD-` mentions, keyword clusters, tail) | per-SD status evidence; T-STAB-1; OOM recurrence; SEF line |
| `NK4C-REVIEW-REPORT-R31/R32/R33/R34` (185/211/188/161) + audit tables | review-line open findings (F/G/H/J generations) |
| Eight 接续/NEW-MACHINE prompt files | carried deferred lists, red lines, honesty boundaries |
| git commit messages `8d0a177ed…e24963e55` (~500 commits, 129 debt-keyword) | closure/rollback evidence |
| Code re-verification at snapshot | Appendix B (~30 checks) |

Zero-increment / clean sources, cited for completeness: the five `.review/zcode/edge*/FIXLOG.md` files
(unchanged since 09-23 — the edge threads closed before this window); `edge_todo_archive.md` (closed-only);
the five `new_todo_*` scans (absorbed by the PD/P ledgers).

**Recommended sweep surfaces, ranked by yield this round:** (1) git commit messages — again the highest
density of root-cause statements per unit length, and the only source that dates every closure; (2) the
two operative ledgers (PD + P) — but always re-anchored, they rot too (three stale rows found, §SD-31);
(3) the two riscv case files — incident analyses are where design-level residue concentrates; (4) review
reports' "open findings" sections across generations (the F/G/H/J backlog was never registered anywhere —
three generations of drift); (5) `tools/pattern-gate.sh` + baseline (institutionalized bug patterns);
(6) `os/tests/` (one file per cross-server contract — the composition-seam map); (7) the per-stage
`.design/` snapshots.

---

## 3. The register

Entries carry a **status line** and, where October decided something, a **decision** block quoting the
frozen PD ruling. Closed entries keep their identity, closure evidence, and residuals — they are the
register's track record and their residuals remain live debts.

### Part A — Boot and image formation

#### SD-1. Boot entry forms: five now, and the equivalence-assertion layer that was promised has not landed (债①; `[ARCH: boot-form-unification]`) — **open, family grew**

**Decision (PD-07, frozen):** keep multiple forms (option 乙): explicit interface +
**boot-postcondition equivalence assertions** (not entry-structure equality) + form-count accounting.
Unification rejected as premature migration cost; unify-inline rejected as hardware-falsified (aarch64
dual root). The 2026-09-30 register's E.2 user pre-adjudication (no unification work before riscv Plan A
closes; then standalone project) is absorbed by this ruling — riscv Plan A has since closed, so the
standalone-project trigger is *armed but intentionally not pulled* (no pain point; see SD-2).

**Current state (verified).** The entry-form family grew from three-plus-forming to **five real members**:
(a) x86_64 UEFI inline (`arch_boot` in-process in the shim); (b) aarch64 UEFI cross-image
(`BootHandoff` → `kernel.elf`); (c) riscv64 OpenSBI self-boot (`os/kernel-image/src/bootface.rs`, 续-88);
(d) **aarch64 QEMU direct-core self-boot** (`os/kernel-image/src/bootface_a64.rs`, 续-406 — loads via
`-kernel`, the fourth independent entry); (e) the riscv64 early-hart-filter parking mailbox (续-400,
registered at birth as a member of this family). Verified at snapshot: `bootface.rs` and `bootface_a64.rs`
both present; `os/boot-shim/src/main.rs` still carries the inlined `arch_boot` leg for x86_64.

**The un-landed half of the decision.** PD-07's equivalence-assertion layer has no evidence of landing
(续-406 added a fourth form with no assertions; the same entry recorded a third strike of the
"paper constants" lesson — an `entry_start_pa` class typo). Every boot-leg change still pays a per-form
real-machine signature; the known pit class (shared helpers assuming one `.bss` world — the §1.115
incident where a shared helper zeroed the live root page) remains guarded only by convention.

**Why structural (unchanged).** Each form instantiates kernel global state differently; the pits are
invisible in shared helpers' signatures; the real-machine regression matrix scales with form count.

**Options.** A — execute PD-07's assertion layer: a host-side **boot-postcondition test battery** (same
`KernelInfo` invariants, same post-boot memory map facts, asserted per form) plus a form-count constant
accounted in one place (cheapest; already adjudicated — this is execution debt, not a new decision).
B — full unification on `BootHandoff` (SD-2 trigger list governs; not now). C — freeze and count (the
status quo minus the assertions — what exists today).

**Recommendation.** **A, as a ride-along on the next boot-face batch** (each new form that lands without
the assertion battery re-pays a full exploration cycle; bootface_a64 already demonstrated the cost).

#### SD-2. Whether x86_64 migrates to the standalone-ELF form (债②) — **closed as adjudicated: no migration**

PD-07 froze "不迁移"; the trigger list from the 2026-09-30 register (another inline-form boot defect; a
standalone-ELF consumer; a third `BootHandoff` consumer; install-surface change) remains the reopening
condition. riscv Plan A closed without becoming a `BootHandoff` consumer (it self-bootstraps), so trigger
(3) is still unsatisfied. **No action pending.**

#### SD-3. Platform descriptor / memory-map firmware source channel (债③) — **open (riscv leg landed; aarch64/x86 legs pending)**

**Landed in the wave:** riscv64 Plan A parses the DTB from `a1` (`bootface.rs`), replacing the hardcoded
QEMU-virt memmap; the aarch64 direct-core face (bootface_a64) parses its own DTB (with an upstream
library defect worked around, SD-38). **Still open:** the aarch64 UEFI chain's descriptor source
(adjudicated D1: ship a QEMU `dumpdtb` blob as an ESP file — not started); x86_64 keeps config tables.
The arch-neutral channel (`KernelInfo.platform_sources` + `parse_by_kind`) exists; what remains is the
per-arch "last wire" and retiring the remaining hardcoded memmap fallbacks (see SD-4/ADDRESS-AUDIT A6).

**Why structural (unchanged).** The memory map is a safety surface (frame-pool sizing); a lying or
hardcoded source silently mis-sizes boot memory — the §1.101 boot-OOM family already paid for this.

**Options.** As the 2026-09-30 register: A per-arch normalization (in progress, riscv done) · B explicit
normalized-descriptor channel (`[ARCH]`) · C hardcoded + dev fallback (rejected for production).

**Recommendation.** **Finish A**: the aarch64 D1 batch is the one remaining wire; retire the
`vm_server.rs` x86-shaped mock fallback (A6) in the same batch.

#### SD-4. Address constants in parallel worlds (债④) — **open; user-VA half converged by the T13 audit, linker/load half untouched**

**What October did — the user-VA boundary half.** The transient-PTE case root-caused a VFS-side hardcoded
`DEFAULT_USER_SP = 0x7fff_ffff_f000` (an x86 mental model, non-canonical under Sv39 — the CPU faults
before walking, so all software walks called the crash "architecturally impossible"). The fix created a
single authority: `USER_STACK_TOP` / `USER_VA_LIMIT` in `os/libs/minix-types/src/types/boot.rs` (verified:
arch-conditional constants + compile-time alignment/limit assertions at :183-184), and the `ADDRESS-CONSTANT-AUDIT.md`
sweep fixed four findings (mmap top out-of-bounds on riscv, duplicated remap literals, **no upper-bound
validation at all** on mmap hints — the only finding reachable by a single guest syscall — and the
aarch64 VA-limit comment corruption). **Open residues of that audit:** A5 `BootParams::simple` (a fixture
with x86 stack top, *not* cfg(test)-gated, living in the production compile face — "exactly the defense
that failed for DEFAULT_USER_SP"); A6 VM kernel-layout mock fallback (`vm_server.rs:617-626`, x86-shaped,
unreachable with v5 handoff, needs a rejection assertion); A7/A8 dead probes with x86 thresholds that can
never fire on riscv; A9/A10 52+ test-fixture hardcodes; A12 comment-layer residues including
`message.rs` stating the stack top as an arch-independent wire fact; A13 seven probe-local KDM copies.

**What October did not do — the linker/load-constants half.** The four parallel worlds are intact
(verified at snapshot): `os/kernel-image/{x86_64,aarch64,riscv64}.ld` assignments; the hand-typed
`bootface.rs` `KERN_PHYS_BASE/KERN_VIRT_BASE` constants; `os/arch/src/arch/direct_map.rs`
`KERNEL_DIRECT_MAP_BASE` per-arch constants; plus the review line's G3 finding — the pool boundary
`[0x82000000,0x84000000)` magic numbers written in two places against bootface's constants. The Rev-2
option E (consume the linker script's *existing* absolute symbols from Rust via extern statics) was
recommended as a ride-along on riscv `.ld` work and **missed its window** (that work landed without it);
bootface_a64 then added a new member of the same family.

**Why structural (unchanged).** A wrong constant here does not fail a test — it fails a machine; guards
are scripts and discipline, not the compiler. The October lesson sharpened it: *a test proves one path's
cognition, not that all paths producing the same semantic share one authority* ("test the semantic
boundary").

**Options.** E — linker-side single-sourcing (extern statics consuming the scripts' symbol assignments;
applies to link-time facts) **+ D** (host tests read the production scripts; applies to mapping-config
constants) **+ A** (delete the unreferenced old `link.ld` trio) — the Rev-2 recommendation, still
unexecuted. New from October: **T13-style authority sweep** for the remaining open A5–A13 residues
(mechanical, same shape as the executed half).

**Recommendation.** **T13-residue sweep immediately (A5/A6 are reachable-shape hazards); E+D on the next
`.ld` touch — and if no `.ld` touch is scheduled, schedule E standalone**, because the family has now
grown twice since it was registered.

#### SD-5. Early console (债⑤) — **closed (续-395), and closed above the recommended bar**

The wrong-port poll (`0x3f9` IER instead of `0x3fd` LSR — every byte spinning the full 100k cap) and the
unbounded wait loop were both fixed, and the fix landed as the *structural* option rather than the
minimal one: a shared, bounded, host-testable `tx_wait_then_send` now lives in the shared layer
(`os/plat/src/early_console.rs:23`, verified), the shim's `emit_byte` reads `0x3fd` (verified
`os/boot-shim/src/lib.rs:140`), and the PL011 precedent generalized. Measured 2.1× boot-serial speedup;
x86 command-face gate PASS. **Residual:** none for the transmit contract; one adjacent true-hardware
latent item was later filed by the review line (PL011 page mapped Normal-cacheable instead of Device
attributes — SD-42).

#### SD-6. Boot bump pool has no reclaim path; kerninfo mapping responsibility not handed to VM (债⑬) — **open, and dropped from both operative ledgers**

Verified unchanged at snapshot: `prepare_boot(1024)` (`os/boot-shim/src/main.rs:61`); `os/kernel/src/boot_alloc.rs`
has no free API (grep `fn free` = 0); the kerninfo module doc still records "moves to VM" as designed but
unscheduled; `vm_handoff` still deducts the whole 4 MiB region from the VM free list. Nothing in the PD
or P ledgers represents this item — after the 2026-09-30 register it is the clearest case of a registered
debt falling out of the operative books (the P-ALL-11 row covers SD-1/2/3/4 only).

**Why structural (unchanged).** Welded costs (4 MiB/boot + bind-count ceiling); kexec/reboot/image-reload
hit the ceiling on arrival; Linux's counterpart (`free_init_pages`/memblock handover) is the standard
answer.

**Options (unchanged).** A — hand the kerninfo mapping responsibility to VM at the established trigger
(VM taking over user-PTE work) · B — make boot page-table pages reclaimable after the direct map is up ·
C — capacity re-estimation (rejected as permanent).

**Recommendation.** **A with the VM-handoff moment (batch with SD-8/PD-28's VM work, whenever it opens);
re-register in the claim ledger now** — see §7.

#### SD-7. kernel-image entries not clearing `.bss` — **closed (续-396, defensive)**

All three entry legs now carry explicit `__bss_start/__bss_end` zero loops (verified: x86_64 `lea rdi`,
aarch64 `adrp x9`, riscv64 `la t0` in `os/kernel-image/src/main.rs`), landing as defensive symmetry under
the SD-1 multi-form reality rather than as a bug fix. Dual real-machine gates PASS. **No residual.**

---

### Part B — Memory: allocator, VM bootstrap, TLB

#### SD-8. VM never pre-materializes its working memory (C `reservedqueue` / `MAP_PREALLOC`) (债⑥) — **adjudicated: the principled fix will NOT be built for VM; stopgap is the accepted form**

**Decision (PD-28, frozen, covering SD-8 + SD-11):** server heaps migrate to a VM-backed growth model
(`PageSupplier` + `VM_BRK`) — **except VM itself, which keeps its static pool as a declared bootstrap
special case**. The C `reservedqueue`/pre-map idea is thereby not adopted: the kernel bootstrap root +
runway stopgap remains VM's accepted form, with its layout-sensitivity documented as an evaluated
temporary shape.

**Residual.** The stopgap's boundary is still heuristic (§1.59 layout razor-edge); the acceptance note
("evaluated temporary form + warning") belongs in the design doc, not only in worklogs. Low urgency: VM
has been stable through the whole October wave.

#### SD-9. `PAF_CLEAR`: the zero-fill pipeline's funnel joint — **closed (the funnel now keys CLEAR)**

Verified at snapshot (`os/servers/vm/src/cow_exec_pf.rs::alloc_and_map`): the demand-fault funnel now
maps region flags through `to_alloc_flags()` and zeroes the frame via `clear_phys_page` when
`PageAllocFlags::CLEAR` is present — the C `vrallocflags → PAF_CLEAR → sys_memset` semantics (the comment
even cites the aarch64 mode-① signature as the motivating case, which later turned out to be FPSIMD, not
stale frames — the fix was still correct on its own terms). The shape differs from the 2026-09-30 option
A sketch (no flags parameter was added to `alloc_pfn_reclaiming`; the clear happens VM-side after
allocation) — outcome-equal. **No residual.** The "environmental-luck invariant" lesson stands as a
pattern-family entry (§4.6).

#### SD-10. VM writes process page tables with no local TLB invalidation; the proxy-flush legs exist unused; and the riscv corruption writer is the line's largest open case (债⑪ + E-VMTLB + 问题乙) — **open, escalated**

This entry now aggregates three linked faces:

1. **The design claim and its window (unchanged in substance).** The kernel's proxy-flush legs exist
   three-arch (`vmctl FlushTlb/InvlPg` → `CurrentTlbArch`); the VM server still issues none, by design
   ("every PTE write self-flushes via the PteChannel-gated direct-map path; the target's root switch on
   wake covers the rest"). That claim still covers only the PTE *page's* translation, not the *target
   page's* stale entry; the unicore window still closes by scheduling accident; SMP / lazy-root /
   modify-without-wake still reopen it. riscv64's `write_pte_dm` gating landed before the last snapshot.

2. **The riscv non-deterministic corruption writer (问题乙, PD-16, P-RV-02) — the line's largest open
   design-level case.** Small, precise, wrong writes observed on riscv multi-core boots; two
   deterministic look-alikes were fixed (the exit-chain signal-number-always-zero → silent-drop →
   permanently-parked-pagefault chain, 续-383; the t_strerror TLS-`tp` miss, 续-384), three same-shape
   trap-leg asm defects were fixed with machine-code verification (kernel-leg t0 clobber 续-370, user-leg
   t0 clobber 续-371, return-leg unmasked window 续-373), 19+ boots ran clean — but the writer was never
   caught in the act, and the case is *unscheduled*: it no longer blocks any goal, dropped out of every
   handover queue, and the clamp-removal condition text still names "根治跨 CPU 的内存腐蚀" as a
   precondition (a research-grade open bug wired in as an engineering gate). PD-16 froze the strategy:
   keep hunting with a mechanical stop-loss — a per-blind-spot coverage triple (observation point,
   execution record, matrix mark) plus a fixed budget N registered once before execution, immutable
   in-batch; no rerun-based release claims. Open instrumentation gaps (SD-26): the reachability
   telemetry was rolled away with the probe family (续-385), so the full-gate rounds have no paired
   reachability readings; the walk-reconciliation probe covers two of the three copy sites
   (`cross_space_copy` uncovered) and both arms share one walk function, so it proves repeatability,
   never correctness; the trap-frame per-entry integrity checksum (the one instrument that can answer
   "were restored register values correct") was recommended and never built.

3. **The `vmddm` kernel-mediated bypass ([ARCH: riscv-vmddm]) — live code, undecided disposition.** To
   keep the VM server's riscv map leg out of user-mode-`sfence` illegality, the VM now calls through a
   bridge (`os/servers/vm/src/vmdm_bridge.rs`, verified) into kernel-mediated KDM writes that flush
   kernel-side. The review line registered the fork honestly: per-operation kernel-call cost, and the
   `PteRead/Write/Zero` arms accept arbitrary physical addresses (H5 — an unbounded physical R/W surface
   reachable from VM), with no RAM-bound on `pa`. Its disposition (keep as a riscv escape hatch with a
   bound + guard; generalize as the batched explicit-flush vehicle from the 2026-09-30 register; or
   delete once a VM-side write path is proven safe) is deliberately tied to 问题乙's closure.

**Why structural (unchanged, sharpened).** Composition seam: two individually-accepted designs
("VmDm writes need no flush" × "flush legs unused by VM") compose into a hole; and the investigation
itself is now a structural object — its instruments were deleted, its budget regime is frozen, and its
closure gates three cleanup batches (probe residue, RegionMap fallback SD-40, vmddm disposition).

**Options.** (1) Execute PD-16 as frozen: rebuild the coverage triple + trap-frame checksum + the third
walk-reconciliation site; fixed budget; no rerun claims. (2) For the design claim: at the SMP milestone,
choose explicit flush — **batched, Linux `mmu_gather` shape** (one vmctl per logical operation, not per
PTE) — or mark-before-resume (`mark_flush_tlb` half exists); evidence first. (3) For vmddm: see above,
decide at case closure.

**Recommendation.** **(1) now — the instruments are cheap and the case gates SMP; (2)/(3) are decided
events, not now-work.**

#### SD-11. Server heaps: fixed static pool, bump-only supplier (债⑥-option-B / "triple" member one) — **open; recurred as a live OOM; root fix frozen (PD-28) but not started**

**The recurrence (strongest evidence yet).** 续-278c/279a: after goal-②, the MFS filesystem server
exhausted its fixed pool on a real workload (big-block slots 950/1024, then page exhaustion — the
fourth incident of this family: B29/B30/B34/B35/MFS). Stopgap (续-279c): pool raised 1024→2048 pages
*per-arch split* — non-x86 2048, x86 stays 1024 (verified: `GLOBAL_POOL_BYTES = 2048 * PAGE_BYTES` with
an x86 `cfg` split, and honest comments recording that the x86 limit point is **not root-caused**, with
candidate legs named and a warning not to attribute it to demand-fill). The review line's G8: the split
is regression containment, and "删除分档" (un-split) is untracked.

**Decision (PD-28, frozen):** root fix = VM-backed growth heap via the existing `PageSupplier` trait +
`VM_BRK` client leg (C `brk`-faithful; `mmap` channel as later evolution); VM itself keeps the static
pool (SD-8); first migration by a four-level mechanical ordering (candidate table = implementation
artifact). Capacity rounds rejected (fragmentation whack-a-mole); bounds-only rejected.

**What exists already (verified):** the `PageSupplier` trait is the allocator's designed extension seam
(`os/libs/minix-rt/src/alloc.rs:225`); the static pool is one impl of it; a VM-backed impl plus
install-after-birth wiring is the work. The 2026-09-30 register's design notes stand: re-entrancy via a
pre-claimed reserve (serve the supplier's own IPC buffers from it), per-server incremental install.

**Recommendation.** **Execute PD-28** (first migration server: whichever the four-level ordering picks);
track "remove the x86/non-x86 split" as an explicit acceptance item of that migration, since a
migration that leaves the split has not paid its full debt.

#### SD-12. Identity window supervisor RWX / kernel W^X — **adjudicated (PD-22, three steps), not started**

**Decision (frozen):** ① drop W on the kernel-text mapping in process roots (precondition: a static
check that no runtime writer patches text); ② the low-identity window **de-depend and exit** —
acceptance: after the switch+first-jump+high-half interval, identity is no longer in the normal process
address space (not "keep but shrink permissions"); ③ optional boot-root split. Scope: runtime process
roots only; DM windows already NX. Fact correction recorded during freezing: identity *is* replayed
into every process root today. No implementation. Linux `set_memory_*`/rodata is the reference frame;
the acceptance criterion ("must not exist", not "smaller") is the strong form — worth keeping verbatim
when this batch opens.

---

### Part C — Trap boundary, IPC, and ABI

#### SD-13. Trap-boundary message materialization (债=trap-boundary doc; P-ALL-13-in-PD-only) — **mostly closed on the user side; delivery leg re-qualified; by-value ABI still the parked endgame**

**Decision (PD-19, frozen):** user→kernel materialization landed on **all** legs — send / sendrec /
sendnb / senda / kernel_call, all three arches (`commit_message_to_memory`,
`os/libs/minix-sys/src/ipc.rs:567`; the SYSCALL-leg barrier was added in the same wave, 续-130). The
kernel→user **delivery leg is re-qualified as a completion/visibility requirement**: nothing to do today
(same-core program order + the SMP clamp make it unreachable), and it lands as enqueue-wakeup lock
ordering (DSB/release semantics) **as a hard precondition of any PD-02 unclamp** — not as a standalone
patch. The by-value / copy-based IPC ABI (seL4/Zircon shape) remains the registered architecture
evolution, tied to the endpoint-encoding consultation (SD-18/PD-18), explicitly not to be snuck in via a
bug fix.

**Residual bookkeeping:** the item number P-ALL-13 exists only inside the PD file (§8.3) — the claim
ledger has no row; and the landed `commit_message_to_memory` doc comment (why a volatile read and not
`compiler_fence`: an ordering-only fence does not force a register-resident store back to the alloca)
is the reusable mechanism note for the delivery leg.

**Recommendation.** No code work now. **Bind the delivery-barrier task to the same milestone event as
the PD-19 precondition says (unclamp), so it cannot be forgotten when that day comes** — and give
P-ALL-13 its claim-ledger row.

#### SD-14. IPC status register on callee-saved RBX — **closed (R10 migration; PD-17 追认)**

The status register moved to a caller-saved register (R10 family) with three anchor sites; RBX retains
two identities with six push/pop guards. PD-17 ratified the code as the closure ("降为追认结案") — the
2026-09-30 recommendation (option A) is exactly what landed. **Residual:** ledger refresh (the P-X86-05
row still reads open) and releasing the NK4B P2–P6 dependency chain that was blocked on this ruling.

#### SD-15. Torn register file: trap frame vs CPU context, two structures kept in agreement by convention — **open (low priority; one instrument recommended)**

Unchanged in substance. New October evidence: the aarch64 context's GP save area doc still says
"Updated by trap entry path (future)"; the 问题乙 case file recommends (as its "single minimal next
step") a **trap-frame per-entry integrity checksum** — the only instrument that can answer "were
restored register values correct" — which doubles as the runtime guard for this debt (Rev-2 option B).
No ledger row exists. **Recommendation:** build the checksum with the 问题乙 instrument rebuild (one
stone, two birds — see SD-10/SD-26); otherwise leave documented.

#### SD-15b. `p_fault_addr` filled on three archs, consumed on one — **adjudicated (PD-04), nearly closed**

**Decision:** re-qualified as a *diagnostic record* (the authority path is far_el1/stval → VM_PAGEFAULT
message, already landed); delete the zero-call getter; keep the rolled-back consumer arm registered.
The fill was never a one-sided contract — it is shared code with a real consumer. **Residual:** the
getter deletion (P-ALL-02, open).

#### SD-16. Error-code sign domain — **closed at the convention level (续-388); the type-level guard remains open**

**What landed:** 41 reply sites in `pm/ipc/calls.rs` flipped to negative errno; the `positive_errno`
helper deleted; two long-standing test failures turned green as a result (they had been pinning the
bug); three-machine regression. Verified: no `positive_errno` hits; 49 `Reply(-` sites. **What did
not:** the `WireErrno` newtype / constructor-only-from-enum (Rev-2's co-recommended option C) — the
class stays open: every future reply site can still forget the negation. **Recommendation:** keep as a
small hygiene item; land the newtype with the next PM reply-surface change, not standalone.

#### SD-17. `ps_strings` argc slot width 4 vs 8 — **closed (续-388)**

Boot leg unified to the LP64 full-word form (`ARGC_ARGV_ENVP = 3 * size_of::<usize>()`, verified at
`os/arch/src/arch/boot.rs:421`), fixing the boot-leg stack alignment incidentally; three-machine
re-verified. **Residual:** PD-09 froze the general obligation — a six-dimension cross-arch birth table
(thread pointer, stack alignment, argc slot width, exit report, initial register ABI, trap save/restore
contract) — which generalizes this debt class; the table is not produced (it feeds PD-01's x86-into-ATF
work).

#### SD-18. Endpoint encoding evolution (OQ-1) — **adjudicated: frozen bit-identical to C; terminal form explicitly not pre-decided**

**Decision (PD-18):** none of the three evolution options; current encoding frozen (i32, shift 15,
NR_PROCS=256 — bit-identical to C); the single reopening trigger is Q1–Q10 showing a blocked real need;
corrective fixes (like PD-20's message-size repair) are exempt from the freeze. This closes the
2026-09-30 "largest frozen-ABI decision" item as *decided-to-stay*. **No action.**

---

### Part D — Error-path discipline

#### SD-19. Silent error swallowing (债⑫) — **open; disposition table stands; new live instances; dropped from both ledgers**

The round-6 disposition table (31 stations = 7 diagnostic-floor / 23 maintain+comment / 1 signature
rework) remains the plan of record; none of the three steps (pattern-gate arm, lint, flagship fixes
beyond SD-16) landed, and — critically — **the family produced two more live incidents in October while
holding no row in either operative ledger**:

- The 问题甲 stall chain (续-383): dump-core notification reads a termination status that only exists
  two steps later → signal number always 0 → VFS rejects → PM's reply-recognition arm does not accept
  negative errno → silent drop → no restart → the waiter is never released (permanently parked
  PAGEFAULT). Fixed; but the *rule* it exposed is structural: "when a C `panic` is politely converted
  into a refused service, the converter must answer *who releases the waiter*" — and the silent-drop arm
  shape is exactly this family.
- The review line's G2: a RegionMap defensive fallback prints three diagnostics **then stays silent
  forever** — now gated on 问题乙 closure (see SD-40).
- P1-待办② persists across four review rounds: a comment-vs-code contradiction on ring-buffer-full
  semantics (comment says drop-oldest, code drops-newest) — the same "narrative and mechanism disagree"
  genus.

**Options (unchanged from Rev 2, mechanism upgraded):** C — mechanical gate: the clippy restriction
lint `let_underscore_must_use` (the IPC surface returns `Result` from one trait; `let _ =` is the
silencer; repo lint surface verified untapped) + pattern-gate arms for the literal-fallback and
`()`-signature sub-families · B — diagnostic floor on the dispositioned stations · A — the one
signature rework (`fs-rt copy_out`).

**Recommendation.** **C immediately (it converts the family from audit-found to compile-found), then
the two named live shapes (PM negative-errno recognition rule; SD-40's RegionMap) — and re-register in
the claim ledger** (§7).

---

### Part E — Per-architecture CPU contracts

#### SD-20. Cache maintenance on data→instruction transitions (债⑩) — **open (P-A64RV-02, untouched)**

Unchanged: aarch64 `dc cvau`/`ic iallu` remain absent; riscv64's `fence.i` coverage beyond the boot-leg
scatter was not re-verified this round (the October riscv boot chain reached all gates, which exercises
exec heavily on QEMU — but QEMU does not model this fault class, so gates-green is not evidence; the
ledger row stays open and is the authority). The frozen recommendation (Rev 2/§D.11): a per-arch
`sync_icache(range)` contract in the arch-trait family (`[ARCH]`), aarch64 = the ARMv8 sequence, riscv64
= `fence.i` (+ SBI remote fence at SMP), x86_64 = no-op; called at the ELF-loader tail. **This remains
the highest-certainty real-hardware time bomb in the register** — nothing in the QEMU gates can ever
clear it.

#### SD-21. Inbound user-SP 16-byte realignment — **adjudicated: explicitly deferred (续-395 option C)**

The rollback discipline held (no re-landing without an owner defect); the boot-leg alignment instance
was incidentally fixed by the SD-17 unification (续-388). The remaining face (exec-side child SP on
aarch64/riscv64) waits on P-A64-03's ruling context. **No action.**

#### SD-22. Kernel↔user memory access model (SUM/PAN/SMAP) — **adjudicated (PD-21, two layers); software layer landed on the message face**

**Decision (frozen):** layer 丙 — a formal software access rule: *all* kernel reads **and** writes of
user space go root→walk→permission/boundary→PA→DM-coverage (multiple entry functions allowed, no
bypass; no new direct dereference; static guard). Landed on the IPC message face
(`[ARCH: user-copy-via-dm]`, `copy_msg_from_user`/`copy_via_root_pages`/`cross_space_copy`). Layer 乙 —
hardware window guards (PAN/SUM/SMAP) as the second line, batched with PD-22; with the frozen caveat
that hardware bits catch only U=1-mapped direct access, **not** `*(direct_map(wrong_pa))` — the
software rule is the load-bearing one.

**Residuals (open):** the static guard's write-down (the rule is enforced by review, not yet by a
gate); the VM server's **four direct `minix_arch::riscv64::paging::vmdm::*` imports** (registered
refactor todo with a three-option scope choice — fold into the vmddm disposition, SD-10); and H5
(`PteRead/Write/Zero` arms accept arbitrary `pa` — an unbounded physical R/W face from VM, the
`direct_map(wrong_pa)` hole in concrete form: needs a RAM-bound + permission check). The
Rust-for-Linux `UserSlice` closed-surface precedent remains the target shape for the remaining sites.

#### SD-23. aarch64/riscv64 FPSIMD state across traps — **closed, by a different design than recommended; five residuals live**

This entry is the register's most instructive closure. **Root cause (定谳 续-129):** the aarch64 trap
stub saved only x0–x30; Q0–Q31 were never saved and `CPACR_EL1.FPEN` was stuck open — aarch64 codegen
keeps 8-byte loop-invariant constants in FP registers (`ldr d8, [literal pool]`), so every
trap+schedule round-trip overwrote them with another context's residue. This was the long-standing
aarch64 mode-① (~4 GiB bogus allocation) — the register's own "orphan primitive" finding (the save/restore
asm existed with zero callers) was the other half of the same fact.

**The fix that landed was NOT the Rev-2 recommendation (eager wiring).** It was: (1) **ban FP in the
kernel entirely** — the kernel is compiled `-neon,-fp-armv8` on the kernel-link cargo invocation only
(`os/xtask/src/image.rs:796`, verified — user modules keep FP); (2) **lazy-FPU ownership single-pointed**
— the per-dispatch CPACR write was removed (it was also *wrong*: comment said 0b01, code wrote 0b11 via
`orr`, and `orr` cannot clear), the dead `cpacr` module deleted, `fpu.rs` init moved to 0b01, policy
owned by `finish_and_restore` enable/disable + an EC=0x07 trap rotation leg; wiring `FpuArch::init` into
boot was explicitly rejected (first-disable establishes the same state). riscv64 got the sibling fix
(kernel codegen FP ban + `sstatus.FS` lazy gating + global_asm save/restore + scause=2 leg, 续-133).
Result: INIT reached Runcom/MultiUser, markers, and eventually all gates.

**Why the divergence is fine (and worth recording):** the eager-vs-lazy argument changed premises —
once the kernel itself provably uses zero FP instructions (enforced by codegen flags, not convention),
the trap-cost asymmetry that favored eager disappears, and lazy single-ownership matches the C i386
shape. The Rev-2 recommendation was right about the *diagnosis* (built-but-unwired machinery +
promise-comments) and wrong about the *strategy*. Linux/arm64's eager choice is not binding here
because Linux's kernel *does* use FP (NEON in crypto) — ours does not.

**Residuals (all open, verified or ledgered):**
- **AF-1:** `fpsimd_restore_raw` deliberately does not declare v8–v15 as clobbers (declaring them would
  make LLVM reload d8–d15 *after* the `ldp`, re-corrupting the restored user lane) — a narrow
  call-graph safety argument with **no test guard**; the promised dual-FP-user regression test never
  landed.
- **AF-3 (SF-2):** `release_fpu` not wired in aarch64 exec/clear/sigsend — verified comment-only sites
  at `os/kernel/src/syscall_process.rs:435/:582/:1466`; a process holding FPU ownership through exec
  inherits stale Q registers.
- **AF-4:** precompiled `core` contains ~11 V instructions not rebuilt by the kernel's `RUSTFLAGS` —
  weakens AF-1's call-graph argument (build-std leg missing).
- **AF-8/9/10:** the lazy-gating bit values (FS_INITIAL/FS_DIRTY vs predicate `FS==Off`) were found
  wrong/inconsistent in retro-audit; the corruption case file marks the current field semantics
  `[待验证]` — treat as not independently confirmed closed.
- **Production deviation, documented:** on both current boot chains the effective FPEN/FS policy leaves
  user-mode FP traps OFF at boot (UEFI: AAVMF leaves 0b11; direct-core: `arch_init` re-writes 0b11), so
  **lazy rotation is not active in production today** — the minix-sef wake-up fallback callback
  compensates (registered deviation, 续-414). Plus **E6/SD-39**: the kernel `SigSet` is 64-bit, so the
  wakeup-only signals above 64 are number-invisible.

**Recommendation.** One small hardening batch: the AF-1 test guard + AF-3 wiring + AF-8/9/10 field
verification, as a single "FPU family residuals" unit — it is the only residual cluster where a
*verified-fixed* debt still carries an unverified gate predicate.

---

### Part F — Scheduling and SMP

#### SD-24. True SMP fenced by the transitional clamp — **adjudicated (PD-02: no full SMP this arc; subgate only); the line advanced *around* the clamp; four named sub-debts**

**Decision (frozen):** no full SMP adoption this arc; the gate is "AP start + basic irq/exception
survival". That gate has since **passed on all three archs** (riscv 续-403/404/407 at `-smp 2` with the
36/36 suite; aarch64 续-405/406 direct-core) — with the clamp empirically respected ("AP wfi 驻留…多核
拓扑对单核调度器零干扰"). The un-repaid arms (per-CPU runqueue, enqueue wake-IPI, preemption delivery)
remain; `-smp ≥3` is unverified. Red line (frozen): unclamping without per-core runqueues converts loud
failure into silent whole-machine stall. **PD-19's delivery-barrier precondition is written into the
unclamp list** (see SD-13), as is 问题乙's cure (SD-10).

**Sub-debts inside this entry (new since Rev 2):**
- **Single-record AP parking mailbox** (续-402/407): one parking record per hart boundary, untested
  beyond 2 CPUs — a capacity ceiling nobody owns.
- **AAVMF/EDK2 SMP delivery defect** (续-405④): EDK2 DXE wakes secondary cores into its own trampoline,
  overwritten at ExitBootServices; `CPU_ON` cannot re-deliver cores the firmware already accounts ON.
  Worked around by abandoning UEFI for the aarch64 SMP gate (direct-core). The production x86_64 chain
  is still UEFI — if SMP ever goes full (PD-02 甲), this is a blocking firmware fork. Options: direct-core
  for SMP needs (current), firmware work (out of repo), park.
- **Transition double-bookkeeping**: the scheduler server's ledger records processes on the *requested*
  CPU while kernel/monitor always read BSP — two books, kernel authoritative, accepted only for the
  clamp period.
- **AF-7 (review-registered):** a BKL two-step write inside the idle-wake IF=1 window — a same-CPU
  self-deadlock surface that must close before multi-core stress (suggested: merge into a single
  `AtomicIsize` owner word).

**Recommendation.** No unclamp work until its precondition list is *fully* green (per-CPU runqueue,
wake IPI, delivery barrier, 问题乙 cure); land AF-7's owner-word merge and the mailbox capacity question
as part of the same milestone's opening audit.

---

### Part G — Diagnostics, tooling, and process

#### SD-25. Probe and diagnostic-facility governance (债⑧) — **largely executed; residue gated on 问题乙**

续-385 executed the big roll: ≈2,900 diagnostic lines deleted, serial noise −95%, six gates green
after. PD-10 froze the three-tier rule (roll-after-use / promote to committed facility / production
guard) — the tier contract this register recommended in Rev 1. **Residue:** ~30 stations left, plus the
case-bound items (`fa0`/`pfwd-lethal` probes, `NK4C_CLI` asm flag, the probe-family match patterns that
must extend beyond the `nk4a:` prefix) all deliberately waiting on 问题乙 closure so the case's
instruments are not deleted under it. The October lessons that hardened the regime: probes are not
passive (print volume alone moved the observed "stall point"; any per-iteration probe perturbs
handshake timing); off-experiments need positive controls; "probe zero hits" must be read as "leg never
ran", never as "path clean" (pattern candidate 86).

#### SD-26. Observability is layout-coupled, and October deleted its instruments — **open; strongest evidence yet; dropped from both ledgers**

The 2026-09-30 entry (PM layout fragility — instrumentation changes layout and breaks boot) gained a
systemic sibling: **the acceptance criteria need instruments that were rolled away with the probes**.
The frozen three-track criteria (deterministic + statistical + reachability) have no reachability arm
in the current gate rounds — the telemetry counters were deleted (续-385), so full-gate runs produce
final-set counts that are structurally blind to probabilistic defects (exactly the 问题乙 class); the
remaining walk-reconciliation probe covers two of three copy funnels and shares one walk function
between arms (repeatability, not correctness); readings went stale across configurations (collected at
`-smp 1`, gates now `-smp 2`/`-smp 4`); and the one instrument that would answer register-file integrity
(trap-frame checksum) was recommended twice and never built. The methodology corpus now carries the
rules (observer-effect spectrum; detector credibility grading; no reads added to production paths — all
diagnostic reads were `git checkout`-reverted) — but rules without instruments are post-hoc.

**Why structural.** The observability channel is the multiplier on every other debt's diagnosis cost;
this register's own case files (MEMORY-CORRUPTION §4.1: the instrument fabricated the criterion —
four "same-length" logs differed in content) are the evidence.

**Options.** A — rebuild the instrument layer as *committed facilities* under the PD-10 tier contract:
reachability counters (per-probe-arm execution bits readable via diagctl, zero serial volume), the
trap-frame checksum, the third walk-reconciliation funnel — designed layout-neutral (diagctl channel,
not in-band code) so the SD-26-original layout coupling does not reintroduce itself. B — rebuild only
when 问题乙 next runs (cheaper, but the gates stay blind until then). C — accept final-set counting
(rejected: it is the blind spot's definition).

**Recommendation.** **A, batched with SD-10's instrument rebuild** (they are the same work); the
committed-facility tier exists precisely for this.

#### SD-27. Wrong C anchors; the anchor tool cannot see Rust — **open; class keeps striking**

Fresh instances in the window: a retro-audit NIT (`proc.c:1923` → 1922), a worklog anchor-correction
round (续-413), review-line F5/G5/H4 anchor-drift findings, and a stale skeleton header surviving in
`ap_early_entry.rs` after the code beneath it matured. The tool gap is unchanged: `tools/anchor-resolve.sh`
checks documents, not Rust comments. **Recommendation unchanged:** batch-correct + extend the tool with
a `.rs` mode; mechanical, low-risk, and it converts a recurring review finding into a gate.

#### SD-28. Test harness swallows build failures — **closed (续-386)**

Verified: `run_all.sh` now has a `build_pkg` helper with `BUILD_FAIL` counting and exit-code semantics
(`os/qemu-tests/run_all.sh:30/:66-76`). The long-red topology gate was re-consigned honestly. **No
residual** (its CI successor is PD-30, still open — see SD-30).

#### SD-29. Cross-crate testability surfaces — **open, unclaimed; absorbed context from the parity scan**

The specifics (VM queue/paging `pub(crate)`, PM clock-source seam) are unchanged and unclaimed. The
parity scan added the aggregate framing: P-ALL-07 counts 57 service `main.rs` placeholder /
`UnimplementedTransport` faces (tier-4, out of the parity matrix per PD-06), and P-ALL-06 tracks
hand-copied struct duplicates (`KProcSnap` vs `ProcInfoStruct`, `SI_PROC_TAB`, cdev constant drift) whose
root cure is the same doctrine (single authority in a shared types crate; PD-13/PD-23 froze exactly
that for the net byte-order/sock structs and the PmCall enum). **Recommendation:** fold the 2026-09-30
per-server items into P-ALL-06/07 rows when those open; do not work them standalone.

#### SD-30. Feature-unification / mock umbrella default — **open, with a live decision-register conflict and a new deterministic failure (T-STAB-1). Needs user resolution (§7).**

Three facts now stack:

1. **PD-29 froze 甲: remove `mock` from the default features** (hard order: SD-28 fix → PD-30 CI leg →
   unmask → red-list triage; SD-28 is done, PD-30 is not).
2. **Worklog 续-397 then adjudicated in place: "摘除默认不成立"** — the mock default *is* the host test
   form; removing it deletes the host form entirely (no_std crates cannot link a test harness without
   it), and the entry closed P-ALL-10 on that basis — **without citing or reopening PD-29**. One of the
   two is stale; a frozen decision silently contradicted by an execution entry is SD-31's failure mode
   occurring inside the decision register itself.
3. **T-STAB-1 (registered, unexecuted; worklog tail)**: `cargo test --workspace` on current HEAD
   deterministically fails 11 targets — ten test binaries crash with SIGSEGV (minix-driver-rt, init, is,
   mib, net-lwip, rs, rt, sys, vfs, vm) and one assertion fails (ds: hosted `SysKernel` methods return
   ENOSYS where the test pins EIO — verified at `os/servers/ds/src/server.rs:1000-1004`). Three controls
   proved: each package green when built singly; **workspace vs single-package builds produce two
   different test binaries** (cargo feature unification compiles the same crate with other crates'
   feature arms); pre-migration control fails the identical set. This is the mock-umbrella design's
   dark side made deterministic: the workspace-level test surface is a different program than the one
   the per-crate baselines certify — and four independent audits concurred.

**Options.** A — resolve the conflict in favor of PD-29 as written (mock out of default; CI gains a
production-compile leg; host tests move to explicit `--features mock` invocations; the host form
survives, only the *default* changes — 续-397's objection may be answering a stronger claim than PD-29
makes). B — resolve in favor of 续-397 (keep default; formally errata PD-29; then the workspace face
needs its own gate: T-STAB-1's 丙 — `--no-fail-fast` + known-failure whitelist — and its 乙 crash-root
cause: shared-init divergence vs per-crate overruns, representative frame on record). C — split the
difference: keep the default, add the CI compile leg + T-STAB-1 whitelist gate, errata PD-29 with the
reasoning.

**Recommendation.** **Whichever way the user rules, the register's point is the *sequence*: T-STAB-1's
crash face (乙) should be root-caused first** — ten SIGSEGVs under feature unification are a real
defect class regardless of the default-features question (and plausibly the same static/table-size
divergence family as the old B34 layout incidents). The default flip is then a small, evidence-based
choice instead of a stale-decision coin toss.

#### SD-31. The ledgers themselves rot — **open; compounded inside its own successors; institutional countermeasures exist but don't cover the P ledger**

October's evidence, in-register: three stale rows found in the claim ledger itself (P-A64RV-01 "skeleton"
long after landing; P-RV-01 core counts superseded by `-smp 2`; `paging.rs:switch` listed after its
deletion); **three generations of review findings (F/G/H/J) with no registration commits anywhere**
(J3); the register + retro-audit themselves sat untracked past an audit boundary (G10); one frozen
decision's premise was falsified after freezing (PD-26's devman-hook deletion — ground truth showed
devman inherits a handler via libvtreefs; execution correctly deviated to faithful wiring, but the
decision text was not errata'd); and the SD-30 conflict above. Countermeasures that *did* land: the PD
register's **re-anchor gate** (ten fact anchors re-run, zero flips, before freezing — the
anchor-checked-ledger idea from Rev 1, institutionalized) and the round-10 discipline. What has no
countermeasure: the P ledger and the review findings backlog.

**Options.** A — extend the re-anchor gate to the claim ledger (every P row carries a grep-able anchor
+ verification command; a script re-checks and flags stale rows — Appendix B of this register is the
template). B — one registration commit per review round (the review line files its F/G/H/J findings
into the P ledger as part of the round, not as free-floating reports). C — periodic audit sweeps
(the historical answer; labor-intensive).

**Recommendation.** **A + B.** A is the same script pattern as PD's re-anchor gate; B is one workflow
rule. This debt compounds silently — every stale row costs a future session real rework (four observed
instances this window).

#### SD-32. Thread model and futex — **adjudicated: deferred with per-cycle re-registration (PD-11)**

Trigger = first multithreaded consumer. **No action.**

---

### Part H — New since Revision 2

#### SD-34. ~90 host-unreachable arch-gated tests display green (P-ALL-01) — **open, being worked (31/86 pinned)**

`#[cfg(target_arch = "...")]`-gated unit tests inside arch/server crates never compile in the host test
form, yet the suites report green (the tests are simply absent). Census: ~90 (arm64 39, riscv64 40, plat
7, +5); x86_64's 106 run natively; riscv has a two-file `include_str!` mitigation; aarch64 zero. The fix
pattern established in-window (31 of 86 done): move the assertions into `<crate>/tests/*.rs` as
text/shape checks (the "template四件套"), with positive controls. **Why structural:** it changes how
every "has tests" cell in the parity matrix is read — a green suite proves nothing about uncompiled
arch arms. Tier-1 in the claim ledger. **Recommendation: finish the sweep**; add a lint/gate that
counts `cfg(target_arch)`-gated `#[test]` per crate so the class cannot regrow silently (same shape as
pattern 86: "zero hits" must not read as "passing").

#### SD-35. Message union vs the 56-byte payload invariant (P-ALL-12) — **closed (续-433); recorded as the register's exemplar closure**

The breach (registered 续-416): six union members measured 64–72 bytes against the repo's own pinned
56-byte payload invariant (`MessageUnion` 72, `Message` 80 vs C's 64); the largest came from a
hand-computed padding error (`_pad2=[u8;44]` where 32 sufficed) whose own doc still said "padding to
56"; `MessageUnion::default()` writes only `raw[0..56]`, so bytes 56..72 were never written by any
constructor; and the kernel copies `size_of::<Message>()` whole (`os/kernel/src/ipc.rs` message-move
sites) — 16 indeterminate bytes could ride into user buffers (no reader exceeded 56 today), plus one
real out-of-bounds write (DumpCore's named-value field at 44..60). **Closure (PD-20 frozen form +
续-433 executed):** per-member padding repaired against C `ipc.h` anchors; a **109-member
strict-equality assertion chain** (`test_every_union_member_is_exactly_56_bytes`, verified at snapshot —
strict `==`, deliberately not `<=`, per C's `_ASSERT_MSG_SIZE`); `Message` 80→64; three-arch gates
green. The coupled surfaces that had to move in one batch (the regression surface to watch):
`16 + size_of::<Message>()` async-slot layouts (`ipc.rs:634/:704`) and the minix-sys mirror.
**Lesson banked:** "the 56-byte invariant became true for the first time" — an invariant pinned by
assertions but never satisfied was protecting nothing; C's per-member `_ASSERT_MSG_SIZE` is the
blueprint (assertion *chains* at the type layer, not just totals).

#### SD-36. The `vmddm` kernel-mediated bypass — live architecture fork, disposition undecided ([ARCH: riscv-vmddm]) — **open, tied to 问题乙 closure**

Covered under SD-10 face 3. Separate ID because its disposition is a *design* decision (cost model of a
kernel call per PTE op; the H5 unbounded-`pa` surface; whether it generalizes into the batched
explicit-flush vehicle or dies when a VM-side path is proven). **Recommendation:** decide at 问题乙
closure; until then, add the H5 RAM-bound + permission check (it is cheap and independent of the
disposition).

#### SD-37. UEFI/AAVMF cannot re-deliver secondary cores (firmware fork under the SMP gate) — **open, parked by workaround**

Covered under SD-24 sub-debts. Options: direct-core for SMP needs (current), firmware fix (out of
repo), park until PD-02 甲. **Recommendation:** park, but write the fork into the PD-02 unclamp
precondition list so full-SMP-on-UEFI is not discovered late.

#### SD-38. Upstream `fdt 0.1.5` `find_node` defect — **open (dependency debt)**

`find_node` returns None on QEMU direct-core rebuilt blobs; bootface_a64 works around it via
`all_nodes`. Any new DTB parsing reintroduces the trap; it is recorded only in a prompt file, no ledger.
**Options:** pin + patched fork · vendor the walker · replace the crate. **Recommendation:** vendor or
fork-pin at the next DTB consumer, with a host test pinning the QEMU blob shape that breaks upstream.

#### SD-39. Kernel `SigSet` is 64-bit; signals ≥64 are number-invisible (E6) — **open (honest comment on file)**

C's `sigset_t` is 128 bits; the kernel's `SigSet(u64)` makes `p_pending.add(70)` a no-op — the
wakeup arrives but the *number* never reaches the manager (SIGSNDELAY=70; the SIGKSIG-family proxy
bits 70..73 ride the same limit). Verified: the limitation is documented in place
(`os/kernel/src/syscall_signal.rs:86-94`) and "tracked separately". Widening is a cross-cutting wire
change (GET_PROCTAB / GET_PRIVTAB / notify / GETKSIG message fields + PM mirror) — i.e., it is
message-layout work under the PD-18 freeze. **Recommendation:** batch with the first real consumer of
signals ≥64 (none exists); until then the honest comment is the correct form.

#### SD-40. RegionMap defensive fallback: "three prints, then silent forever" (review G2) — **open, gated on 问题乙**

An error-swallowing-family instance with a twist: the fallback was *defensive* (added during the
corruption hunt), and deleting or fail-fast-ing it now would change the case's instrument surface.
**Options:** delete at case closure · fail-fast at case closure · keep (rejected — silent-forever is
the SD-19 genus). **Recommendation:** the case-closure batch must name it; it is already listed as a
binding condition (G1/G2/H3 family).

#### SD-41. BKL two-step write in the idle-wake IF=1 window (AF-7) — **open (small, pre-SMP)**

Covered under SD-24. Fix shape suggested: merge the two fields into one `AtomicIsize` owner word so the
wake path is a single atomic RMW. **Recommendation:** land with the unclamp opening audit.

#### SD-42. PL011 UART page mapped Normal-cacheable (review G6) — **open (true-hardware latent)**

The early-console/UART mapping uses Normal cacheable attributes, not Device — QEMU-silent, real-hardware
latent (MMIO ordering/duration semantics). Same epistemic class as SD-20 (gates cannot clear it).
**Options:** MAIR entry + device attributes for the UART page per arch. **Recommendation:** batch with
SD-20's `sync_icache` unit — both are "real hardware will collect" items, one design review.

#### SD-43. `supports_resize` semantic inversion vs C (F10) — **open (P-ALL-05 residue)**

The `MemType::supports_resize` default returns true where the C counterparts (`anon_contig`,
`cache_resize`) unconditionally return ENOMEM; reachability unproven. One of the two remaining PD-12
semantic deviations (with X-7). **Recommendation:** flip the two impls to C truth with the reachability
note; one-line class.

---

## 4. Cross-cutting families (updated)

The Rev-2 thesis stands — *contract surfaces grew parallel implementations during the port; the
mechanisms that collapse parallels (table-driven gates, contract tests, trait promotion, tiered
regimes) have in-repo precedents; implementation cost is rarely the constraint; adjudication rhythm and
real-machine batches are* — and October added a second-order observation: **once a register exists,
its own rot becomes a debt family** (SD-31's four observed instances; the re-anchor gate is the
in-repo cure precedent). The families, updated:

1. **Second sources of truth** — SD-4 (linker half open, user-VA half converged by T13), SD-27,
   G3/A13. Cure: single authority + assertion chains (the 109-member strict-`==` chain of SD-35 is now
   the in-repo gold standard).
2. **Composition seams / orphan primitives** — SD-10 (flush claim × channel gating), SD-36, the
   historical `send_work()`; the FPU closure (SD-23) is the family's positive exemplar: found by the
   "public API with zero production callers" sweep, closed by removing the premise. Cure: the
   zero-caller sweep + per-contract integration tests.
3. **Type-level error erasure** — SD-19 open (lint untapped), SD-16's convention fix without the
   newtype, SD-40. Cure: `let_underscore_must_use` + signature rework + fail-fast conversions.
4. **Architecture-fork double maintenance** — SD-1 grew to five forms *without* its assertion layer;
   SD-20/SD-42 untouched; SD-23 closed. Cure: same-type-site tables (PD-09's six-dimension birth
   table is the frozen form), trait promotion, and the "gates cannot clear real-hardware-only
   classes" epistemic rule.
5. **Promise-comments as implementation** — struck three more times (the CPACR 0b01/0b11 comment-vs-code
   that was *part of the mode-① root cause*; `gp_regs "(future)"`; the stale skeleton header). Cure:
   promise-phrase detection in review; retro-audit pattern 84–86 candidates now registered.
6. **Environmental-luck invariants** — SD-9 closed (the funnel now asks); the *class* remains the
   SD-10 hunting premise. Cure: explicit request flags; positive controls on off-experiments.
7. **Capacity ceilings via capacity rounds** — SD-11 recurred (fifth incident) and now carries a frozen
   root fix; the 2048/1024 split is the family's "containment without tracking" shape. Cure: root-fix
   the supplier; track the containment's removal.
8. **Silent no-op layers** — no new strikes in-window (the riscv port-io stub family was cleaned with
   the boot chain); kept for the A5/A6 audit residues.
9. **Transitional guards with removal conditions** — SD-24's clamp is the disciplined exemplar
   (condition list written, includes two other debts' closures); the 2048/1024 pool split is the
   undisciplined sibling. Cure: misc-concepts §4.6 rule — removal conditions in the same commit.
10. **Blocking semantics inexpressible in the current ABI** — narrowed: PD-19 landed the user-side
    barrier family; what remains is the delivery-leg precondition and the parked by-value ABI (SD-13).
11. **NEW — Green-but-uncompiled test faces** (SD-34, T-STAB-1): two instances of "the test surface the
    baselines certify is not the program that runs" (arch-gated tests; workspace feature unification).
    Cure: shape/text tests outside cfg gates; `--no-fail-fast` whitelisted workspace gate; count-gated
    `cfg(target_arch)` tests.
12. **NEW — Frozen-decision vs execution drift** (SD-30 conflict; PD-26 premise falsified): decisions
    frozen on stale premises, or contradicted by later entries without errata. Cure: the PD file's own
    revision rule (errata under the entry), applied; re-anchor gates extended to execution entries.

---

## 5. Remediation strategy

### 5.1 Status and claim map

| Item | Status | Operative-ledger row | Next action |
|---|---|---|---|
| SD-1 | open (grew) | P-ALL-11 | assertion battery with next boot batch |
| SD-2 | closed-adjudicated | PD-07 | none (trigger list) |
| SD-3 | open (riscv done) | P-ALL-11 | aarch64 D1 wire + A6 retirement |
| SD-4 | open (half converged) | P-ALL-11 | A5/A6 sweep now; E+D on next `.ld` touch |
| SD-5 | **closed** 续-395 | — | — |
| SD-6 | open — **dropped** | none | re-register; batch with VM handoff |
| SD-7 | **closed** 续-396 | — | — |
| SD-8 | adjudicated PD-28 | (PD-28) | acceptance note into design doc |
| SD-9 | **closed** | — | — |
| SD-10 | open (escalated) | P-RV-02 / PD-16 | instrument rebuild + coverage triple |
| SD-11 | open (recurred) | PD-28 | execute PageSupplier+VM_BRK; track split removal |
| SD-12 | adjudicated PD-22 | (PD-22) | not started |
| SD-13 | mostly closed | P-ALL-13 (PD-only) | give it a P row; delivery leg bound to unclamp |
| SD-14 | **closed** | P-X86-05 (stale) | ledger refresh + NK4B release |
| SD-15 | open (low) | none | trap-frame checksum w/ SD-10 |
| SD-15b | adjudicated PD-04 | P-ALL-02 | delete zero-call getter |
| SD-16 | closed (convention) | P-ALL-05 | newtype with next PM surface change |
| SD-17 | **closed** 续-388 | P-ALL-05 | PD-09 six-dimension birth table |
| SD-18 | closed-adjudicated PD-18 | (PD-18) | none |
| SD-19 | open — **dropped** | none | lint + 2 named stations; re-register |
| SD-20 | open | P-A64RV-02 | `sync_icache` unit (+SD-42 same review) |
| SD-21 | deferred 续-395 | P-A64RV-03 | none |
| SD-22 | adjudicated PD-21 | P-A64RV-04 | guard write-down; vmdm imports; H5 bound |
| SD-23 | **closed** (divergent plan) | P-A64-03 | residuals batch: AF-1/3/8-10 test+wiring |
| SD-24 | adjudicated PD-02 | P-ALL-03 | unclamp precondition list audit |
| SD-25 | largely executed | P-ALL-04 | tier-rule sweep of ~30 stations |
| SD-26 | open — **dropped** | none | instrument rebuild w/ SD-10; re-register |
| SD-27 | open | none | batch-correct + tool `.rs` mode |
| SD-28 | **closed** 续-386 | P-ALL-09 (stale) | — |
| SD-29 | open | P-ALL-06/07 | fold, don't standalone |
| SD-30 | open + **conflict** | P-ALL-10 / PD-29 | user resolution; T-STAB-1 crash RCA first |
| SD-31 | open (compounded) | — | re-anchor gate → P ledger; review rounds file findings |
| SD-32 | deferred PD-11 | (PD-11) | none |
| SD-34 | open (worked 31/86) | P-ALL-01 | finish sweep + count gate |
| SD-35 | **closed** 续-433 | P-ALL-12 | — (watch async-slot surface) |
| SD-36 | open | (SD-10 face 3) | H5 bound now; disposition at case closure |
| SD-37 | open (parked) | (SD-24 sub) | write into PD-02 precondition list |
| SD-38 | open | none | fork-pin at next DTB consumer |
| SD-39 | open (batched) | none | with first ≥64-signal consumer |
| SD-40 | open (gated) | (case closure list) | name in case-closure batch |
| SD-41 | open (pre-SMP) | none | unclamp opening audit |
| SD-42 | open (latent) | none | batch with SD-20 |
| SD-43 | open | P-ALL-05 | flip two impls to C truth |

**Dropped-from-ledgers callout:** SD-6, SD-15, SD-19, SD-26, SD-27 (+ SD-36…SD-43 born here) have no
row in either operative ledger. Either they get P rows (this register's recommendation — the rows are
cheap) or the register itself must be carried in every planning cycle as the missing-ledger layer.

**Sequencing authority.** The PD register's frozen hard orders govern where they overlap: the
credibility chain (SD-28 ✅ → PD-30 CI leg → PD-29 resolution → red-list triage → PD-31 clippy ratchet);
the security pair (PD-21 + PD-22 same batch); PD-19's delivery barrier before any unclamp; the budget
competition rule (PD-16 keeps budget over PD-02 beyond its subgate). This register adds one hard order
of its own: **the 问题乙 instrument rebuild (SD-10/SD-26/SD-15) precedes any further gate-metric claims**
— final-set counting over deleted instruments is not evidence.

### 5.2 Binding constraints carried forward

- PD register decisions (PD-01…PD-34) are frozen; revision requires the register's own errata process —
  the one known deviation (PD-26, premise falsified, execution changed to faithful wiring) is recorded
  as such, not silently rewritten.
- SMP unclamp precondition list is conjunctive: per-CPU runqueues + wake IPI + PD-19 delivery barrier +
  问题乙 cure (+ this register adds: SD-37 written in).
- Message-layout work stays frozen per PD-18; corrective repairs (SD-35-class) are exempt.
- The user's 2026-09-27 pre-adjudications (E.1/E.2) are absorbed by PD-07; their spirit (no
  preventive boot-form surgery) stands.
- No fix-in-passing for confirmed-but-unrelated defects; falsify, file, land with own evidence.

### 5.3 What NOT to do (recorded vetoes)

1. No eager-FPU rework — SD-23 closed by lazy single-ownership under a codegen-enforced zero-FP kernel;
   do not "complete" the old plan (only the residuals batch is live).
2. No blanket error-swallow purge (SD-19) — the disposition table filters; C fidelity governs.
3. No preventive boot-form unification (SD-1) — the assertion layer is the adjudicated execution, not
   migration.
4. No probe mass-deletion beyond the tier rule (SD-25) — the case-bound instruments wait for 问题乙.
5. No rerun-based "zero occurrences" release claims for 问题乙 (PD-16 frozen).
6. No message-layout pre-commitment (PD-18).
7. Do not resolve SD-30 by flipping the default before T-STAB-1's crash face is root-caused — the crash
   is real regardless of who wins the conflict.

---

## 6. The highest-leverage moves

1. **The 问题乙 instrument rebuild** (SD-10 + SD-26 + SD-15, one batch): reachability counters as
   committed facilities, the third walk-reconciliation funnel, the trap-frame checksum. It is the
   precondition work for the line's largest open case, it re-arms the gates' probabilistic blind spot,
   and it closes two other debts' instruments in the same pass. Everything else in this register is
   smaller.
2. **The credibility chain's next link**: SD-28 is closed; PD-30 (CI kernel-tests leg) + the SD-30
   conflict resolution + T-STAB-1 RCA are the sequence — this is what makes "green" mean the same
   thing in every configuration (families 11/12 are both fed by this).
3. **The two real-hardware time bombs, one design review**: SD-20 (`sync_icache`) + SD-42 (device
   attributes). Neither can ever be cleared by a QEMU gate; both are cheap now and expensive on
   hardware day.
4. **Ledger hygiene as infrastructure** (SD-31): extend the re-anchor gate to the P ledger; review
   rounds file their findings as P rows. The register's own history (six closures, three stale rows,
   one frozen-decision conflict, five dropped items) is the cost model — the mechanisms are scripts.

---

## 7. Questions reserved for the user

1. **SD-30 / PD-29 conflict.** PD-29 froze "mock out of default"; 续-397 closed the P-ALL-10 row on
   "removal is impossible (it would delete the host form)". Rule which governs — or accept this
   register's option C (keep default + CI compile leg + T-STAB-1 whitelist gate + errata PD-29). In all
   cases: T-STAB-1's ten SIGSEGVs deserve their own root-cause regardless.
2. **Dropped items.** Re-register SD-6 / SD-15 / SD-19 / SD-26 / SD-27 (+ Part H items) as P rows, or
   accept this register as their standing home with per-cycle re-check?
3. **问题乙 budget.** PD-16 froze keep-hunting with a mechanical stop-loss. Confirm the instrument
   rebuild (§6.1) as the next funded step, or schedule the case as dormant (park the SMP-unclamp
   dependency explicitly until then).
4. **vmddm disposition (SD-36).** Decide at 问题乙 closure as planned, or earlier if the H5 bound
   (unbounded `pa`) is deemed a security-relevant surface now.

---

## Appendix A — Corpus scanned

Under `rewrite-notes/coordination/`: `STRUCTURAL-DEBT-REGISTER-20260930.md` (Rev 1–2, carried in full);
`PENDING-DECISIONS-3ARCH-PARITY.md`; `TODO-3ARCH-PARITY-20261006.md`;
`NK4C-BUG-RISCV64-MEMORY-CORRUPTION.md`; `NK4C-BUG-RISCV64-TRANSIENT-PTE.md`;
`riscv瞬态页表崩溃取证方法论.md`; `NK4C-RETRO-AUDIT-20261001.md`; `ADDRESS-CONSTANT-AUDIT.md`;
`NK4C-WORKLOG.md` (14,189 lines: head blocks, all 38 `SD-` mentions, keyword clusters, tail);
`NK4C-REVIEW-REPORT-R31/R32/R33/R34-*.md` + the four P1 audit tables; `NK4C-REVIEWER-SESSION-RECORD-20260930.md`;
eight 接续/NEW-MACHINE prompt files; `NK4C-QEMU-ENVIRONMENTS.md` (context).
Plus: `.review/zcode/edge*/FIXLOG.md` ×5 (verified zero-increment since 09-23); git history
`8d0a177ed…e24963e55` (commit-message sweep); the code at snapshot (Appendix B).

## Appendix B — Anchor verification log (snapshot `e24963e55`, 2026-10-08)

| Claim | Evidence (tool-derived) | Verdict |
|---|---|---|
| Five boot forms (SD-1) | `bootface.rs` + `bootface_a64.rs` both in `os/kernel-image/src/`; `mod bootface_a64` wired at `main.rs:270/:320`; shim inlined leg intact | present, family grew |
| Console fix (SD-5) | `os/boot-shim/src/lib.rs:140` polls `0x3fd`; shared `tx_wait_then_send` at `os/plat/src/early_console.rs:23` | closed |
| `.bss` clear ×3 legs (SD-7) | `os/kernel-image/src/main.rs:122` (x86 `lea rdi`), `:177-178` (aarch64 `adrp`), `:222` (riscv `la`) | closed |
| errno convention (SD-16) | `grep positive_errno os/servers/pm/src` = 0 hits; 49 `Reply(-` sites in `calls.rs` | closed (convention) |
| argc slot unify (SD-17) | `os/arch/src/arch/boot.rs:421` `ARGC_ARGV_ENVP = 3 * size_of::<usize>()` | closed |
| CLEAR funnel (SD-9) | `cow_exec_pf.rs::alloc_and_map` keys `region.flags.to_alloc_flags().contains(PageAllocFlags::CLEAR)` → `clear_phys_page` | closed |
| Pool split (SD-11) | `alloc.rs:165` `GLOBAL_POOL_BYTES = 2048 * PAGE_BYTES` with x86 `cfg` split to 1024 at `:180`; honest "limit point not root-caused" comments | stopgap verified |
| Boot pool no-free (SD-6) | `prepare_boot(1024)` at `boot-shim/src/main.rs:61`; `boot_alloc.rs` `fn free` count = 0 | open |
| Linker consts ×4 worlds (SD-4) | `direct_map.rs:82/:117` `KERNEL_DIRECT_MAP_BASE` consts; `bootface.rs:115/:118` `KERN_*` consts; `.ld` assignments; no Rust extern consumption | open (option E not done) |
| Message invariant (SD-35) | `message.rs:27` `MESSAGE_PAYLOAD_SIZE=56`; `test_every_union_member_is_exactly_56_bytes` (strict `==`, 109 members); `test_message_total_size_pinned` "Message 64" | closed as recorded |
| Barriers (SD-13) | `ipc.rs:567` + three call sites (`:596/:653/:699`); volatile-vs-fence doc comment intact | user legs landed |
| vmddm bridge (SD-36) | `os/servers/vm/src/vmdm_bridge.rs` present, 7 riscv64 refs; VM crate TLB mentions are comment-only (3, all `vmdm`/`alloc_page` notes) | live fork |
| Kernel SigSet 64-bit (SD-39) | `syscall_signal.rs:86-94` honest comment; `SIGSNDELAY=70` | open as recorded |
| USER_STACK_TOP authority (SD-4/T13) | `minix-types/src/types/boot.rs:146/:173` arch-conditional + const asserts `:183-184` | landed |
| NEON ban (SD-23) | `os/xtask/src/image.rs:796` kernel-only `-neon,-fp-armv8` cargo invocation | landed |
| release_fpu unwired (SD-23 AF-3) | `syscall_process.rs:435/:582/:1466` comment-only | residual verified |
| fpsimd_restore_raw (SD-23 AF-1) | `arm64/fpu.rs:147` | residual verified |
| clamp intact (SD-24) | `sched.rs:307` `clamp_cpu_to_bsp` | present |
| run_all build gate (SD-28) | `run_all.sh:30` `BUILD_FAIL=0`; `:66-76` `build_pkg` | closed |
| T-STAB-1 assertion face (SD-30) | `os/servers/ds/src/server.rs:1000-1004` pins `Err(EIO)` ×3 | present |
| VM TLB callers (SD-10) | non-sim grep = 3 comment-only hits | design unchanged |

## Appendix C — Cross-reference

| This register | 2026-09-30 register | Operative ledgers |
|---|---|---|
| SD-1..SD-17, SD-19..SD-33 | same IDs, statuses updated | see §5.1 map |
| SD-18 | SD-18 (OQ-1) | PD-18 |
| SD-23 residuals | (Rev-2 R2 additions) | P-A64-03; retro-audit AF-1/3/4/8-10 |
| SD-34 | — | P-ALL-01 |
| SD-35 | — | P-ALL-12 / PD-20 (closed) |
| SD-36 | SD-10 face 3 (new as ID) | R3.3 §三.1; PD-21 residuals |
| SD-37 | SD-24 sub-debt (new as ID) | 续-405④ |
| SD-38 | — | prompt 20261006q only |
| SD-39 | SD-23 E6 residue (new as ID) | 续-412 comment; T1 residue |
| SD-40 | SD-19 family instance | R3.2 G2 |
| SD-41 | SD-24 sub-debt | retro-audit AF-7 |
| SD-42 | SD-5 adjacent | R3.2 G6 |
| SD-43 | — | P-ALL-05 / F10 |

*End of register. Supersedes `STRUCTURAL-DEBT-REGISTER-20260930.md` in full.*
