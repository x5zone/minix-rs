# Structural Debt Register — Minix3-in-Rust rewrite, bring-up line

Snapshot: commit `8d0a177ed`, 2026-09-30, working tree of `/home/xzhao/github/minix-rs` (branch `rewrite`).
This document was produced by a read-only debt-hunting session: no code was changed. It consolidates every
design-level structural debt recorded across the line's logs, review reports, incident files, and a
re-verification pass over the code at the snapshot commit. It is written to be self-contained: an external
reviewer with no access to the internal logs should be able to evaluate both the debts and the proposed
solutions from this document alone.

**Revision 2 (2026-10-01).** A second design pass re-examined the recommendations against the Rust source,
with four reference frames in mind: the original C, Linux, Redox, and current Rust practice (including the
Rust-for-Linux kernel work). Three recommendations changed rank, each because the code contains
infrastructure the first pass under-weighted:

- **SD-23 (FPSIMD) now recommends eager save, not lazy trap.** The arch crate already contains a complete
  `stp q0,q1`/`ldp` save/restore implementation and a per-process state slot — built, tested, and wired to
  nothing (the "orphan primitive" shape). Eager is also what Linux/arm64 converged on.
- **SD-4 (linker constants) gains an option E**: consume the linker script's *existing* absolute-symbol
  assignments directly from Rust, making the script the single source at link time rather than at test time.
- **SD-16 (errno sign domain) promotes the newtype option to first-class**: the conversion-table approach
  (A) and the wire-type approach (C) should land together, because C is cheap and closes the class the
  table only enumerates.

Several other entries gained sharper mechanisms without changing rank: SD-10's explicit-flush option gains
a batched design (Linux `mmu_gather` shape), SD-11's VM-backed heap is now costed against the *existing*
`PageSupplier` trait, SD-13's delivery-side fix point is pinned to the `delivermsg`/`UserCopy` seam,
SD-19's mechanical gate is upgraded from grep to the `let_underscore_must_use` lint, and SD-22's
recommended option gains the kernel's existing typed cross-space channel as its vehicle. Appendix B
gained the corresponding verification rows.

---

## 0. How to read this document

- **Debt IDs.** Entries are numbered `SD-1` … `SD-33` and grouped by architectural area. Each entry names
  the ledger where it was originally recorded (e.g. `riscv-reviewlog.md §D 债①`, `edge1 FIXLOG F2`,
  `NK4B-WORKLOG M3.x`). Appendix C cross-references old IDs to new IDs.
- **Status vocabulary.** `open` (recorded, nothing landed), `stopgap landed` (a temporary form works, the
  principled fix does not exist), `partially fixed` (one leg landed, the family is not closed),
  `rolled back` (a fix was implemented and reverted after real-machine falsification), `adjudicated,
  not implemented` (the owner has picked a direction; code pending), `converged` (the scan line that
  produced the entry stopped under the line's stopping rule).
- **Anchors.** Factual claims carry `path:symbol` anchors. Where a line number appears it was derived by
  tool (grep/sed) on the snapshot commit and will drift; the symbol is the stable part. Appendix B records
  the verification commands and verdicts for the load-bearing claims.
- **Adjudication constraints.** Some debts are bound by the user's recorded pre-adjudications
  (`riscv-reviewlog.md` chapter E, reproduced in §5.2 here). Where those constraints exist, this register
  does not re-adjudicate; it marks the constraint and the trigger conditions.
- **`[ARCH: ...]` tag.** The repository's convention for an architectural evolution: the change must be
  marked identically in the design doc, the design notes, and the code. Options labeled `[ARCH]` require
  that process; they are not ordinary fix batches.

---

## 1. What the system is

**The artifact.** A semantic rewrite of the Minix3 operating system in Rust (`#![no_std]`), under `os/`.
Minix3 is a microkernel OS: drivers and OS services run as user processes that talk by synchronous
messages (`send` / `sendnb` / `sendrec` — a blocking round-trip). The rewrite preserves external behavior
(the C tree `minix3/` is ground truth) while re-expressing internals in Rust.

**The components.**

- `os/kernel` (`minix-kernel`): the microkernel — IPC, scheduling, traps, syscalls. Runs SMP with a big
  kernel lock (BKL). Kernel call numbers, message layouts and the wire format follow the C original.
- `os/servers/`: user-space servers — `pm` (process manager: fork/exec/exit/signals), `vfs` (filesystem
  switch), `vm` (memory server: page tables, page faults), `rs` (reincarnation server: restarts crashed
  servers), `ds` (data store), `sched`, `devman`, `input`, `mib`/`is` (info servers).
- `os/fs/`: filesystem implementations (`mfs`, `pfs`, `procfs`, `ext2`, …) plus `fs-rt`, the shared
  transport runtime a filesystem uses to talk to VFS.
- `os/boot-shim`: the loader crate (UEFI for x86_64/aarch64; OpenSBI/U-Boot helpers for riscv64). Builds
  page tables, assembles `KernelInfo` (the boot information contract), and starts the kernel.
- `os/kernel-image`: a standalone linked kernel ELF. Historically the "layout contract slot" producer; it
  is now becoming riscv64's own bootloader (`src/bootface.rs`, untracked work in progress at snapshot).
- `os/libs/minix-rt`, `os/libs/minix-sys`: the user-space runtime (allocator, crt0) and syscall/IPC ABI.
- `os/libs/minix-types`, `os/libs/minix-boot`, `os/libs/minix-platform`: shared contracts.

**The campaign this register belongs to** (the `NK4-A/B/C` worklog threads, "fork-syscall line"): drive
each of three architectures — x86_64, aarch64, riscv64 — through cold boot → INIT → the `/etc/rc` boot
script printing its fixed marker line ("rc marker", the boot-success metric) → the full command surface →
the original Minix3 test suite, on QEMU. Status at snapshot: x86_64 passes (stable at `-smp 4`); aarch64
is blocked by one open memory-corruption failure mode ("mode ①", a ~4 GiB bogus allocation inside INIT);
riscv64 has its boot leg landing but its raw IPC leg still answers `-ENOSYS`.

**Why a structural-debt register.** The line's logs record two kinds of findings: one-off correctness bugs
(routed to fix rounds) and *structural debt* — design-level deficiencies that keep generating bugs or
maintenance cost and whose remedy is a design decision rather than a patch. The line already maintains a
deep structural-debt analysis for thirteen debts (`rewrite-notes/coordination/riscv-reviewlog.md`
chapter D, "债①–⑬", declared converged 2026-09-27). This register (a) refreshes those thirteen against
the current code, (b) adds the debts recorded in other ledgers that never made it into chapter D, and
(c) adds debts that are visible in the logs and code but were never formally registered anywhere.

---

## 2. Where every debt came from

Sources scanned in full or by systematic keyword sweep, 2026-09-30:

| Source | Size | Yield |
|---|---|---|
| `riscv-reviewlog.md` ch. D + E (the existing structural-debt register) | 1579 lines | SD-1…SD-13 backbone (债①–⑬) |
| `NK4C-WORKLOG.md` (main worklog) | 8787 lines | 债 registrations, §1.115 boot-debt section, §1.120 recent entries |
| `.review/zcode/edge1..4/FIXLOG.md` + archive | ~9,000 lines | 54 tracked items; design-level residue extracted |
| `NK4A-QWEN-WORKLOG.md`, `NK4B-WORKLOG.md`, handoff/status/review files | ~4,500 lines | RBX status register, torn register file, platform-descriptor ruling, kernel-image .bss |
| `trap-boundary-message-materialization.md`, `NK4C-AARCH64-EXEC-REBIND-LIVELOCK.md`, `NK4C-BUG-AARCH64-VEC-CAP{,-GLM}.md` | ~1,000 lines | trap-boundary design tension, cross-holder metadata law, layout divergence |
| `PATTERN-SCAN-REPORT-20260923.md` §9, `misc_concepts.md` (lesson corpus) | ~1,850 lines | the error-swallowing taxonomy and the ten-family structural-debt taxonomy |
| `NK4C-MIGRATION-20260930.md`, `NK4C-REVIEW-REPORT*` (3 rounds), `NK4C-REVIEWER-SESSION-RECORD-20260930.md` | ~1,000 lines | true-SMP outstanding arms, per-round open issues |
| `new_todo_*` (five assistant-session scans), prompt/handoff files | ~2,500 lines | thread-model/futex ownership, endpoint-encoding OQ |
| git commit messages (debt/DEFERRED/OQ keyword sweep) | ~150 commits | fix-vs-rollback history for status roll-up |
| Code re-verification (grep/sed at snapshot) | 14 targeted checks | Appendix B |

Files scanned and found to contain no unique design-debt content (cited for completeness): `edge1.md`–`edge3.md`
(frozen pointer-only indexes), `NK4A-TODO.md` / `NK4B-TODO.md` (task books), `edge_todo_archive.md`
(closed entries only), `NK4C-R31-P1-AUDIT-TABLE-20260930.txt` (mechanical audit table).

**Recommended additional sweep surfaces** (used here; increasingly valuable over time): git commit
messages on this branch (the highest density of "trap pinned here" and root-cause statements per unit
length); `.review/{claude,codex,trae}/…/STATE.md` (per-tool review states — only the `zcode` tool's
FIXLOGs were in this corpus); `.zcode/plans/`; `tools/pattern-gate.sh` + its baseline file
(institutionalized bug patterns); `os/tests/` (one integration file per cross-server contract — a map of
the composition seams); and the per-stage design documents under `rewrite-notes/` (each stage's
`.design/` folder).

---

## 3. The register

### Part A — Boot and image formation

#### SD-1. Three boot entry forms coexist, and a fourth is forming (债①, `[ARCH: boot-form-unification]`)

**Recorded in:** NK4C-WORKLOG §1.115 遗留 债①; riscv-reviewlog §D.1.

**Current state (verified).** The same kernel crate is started in three different ways:
(a) **x86_64 inline** — the kernel is linked as an rlib into the UEFI boot-shim, which calls
`minix_kernel::arch_boot` in-process (`os/boot-shim/src/main.rs`, comment "x86_64 / riscv64 keep the
inlined arch_boot path");
(b) **aarch64 cross-image** — the shim builds page tables, writes a `BootHandoff` payload
(`os/libs/minix-boot/src/handoff.rs`, `#[repr(C)]` with a single-page compile-time assertion) and jumps
into an independently linked high-half `kernel.elf`; the kernel side is
`bootstrap_to_kernel_image` / `arch_boot_resume_high_half` (`os/kernel/src/lib.rs`, `#[cfg(all(not(feature="mock"), target_arch="aarch64"))]`);
(c) **riscv64 inline + image contract slot** — the riscv64 UEFI binary is never produced; the production
image only emits the layout contract. A **fourth variant is forming**: riscv64 "Plan A" makes
`kernel-image` its own bootloader (`os/kernel-image/src/bootface.rs`, untracked at snapshot), which parses
the device tree passed in register `a1`, copies the 12 boot modules, and calls the real boot gate itself.

**Why structural.** The two live forms instantiate kernel global state differently. The concrete trap:
shared boot helpers assume one `.bss` world, but each form has its own; calling the shared
`build_bootstrap_root_and_enable` from the kernel-image side zeroes the *already active* root page and
destroys the high-half mapping (the §1.115 incident class). This pit is invisible in every shared helper's
signature. Every boot-leg change (handoff contract, bump-cursor continuation, high-half mapping) must be
verified on the real machine per form — the maintenance surface is ×2 and rising.

**Reference frames.** minix3 C: one form — the boot monitor loads the one kernel image, which carries its
own unpaged startup segment, reclaimed after boot. Linux: per-arch but single-product — `head_64.S` builds
the initial page tables *inside* the kernel and jumps to the high half; bootloaders only hand
`boot_params`/DTB; forms never fork. Redox: the closest to a unified form — one Rust bootloader loads one
kernel ELF and hands memory info; the kernel has no second form. OS theory: boot staging should converge
one-way; the entry contract should be minimal (registers + one page payload); form count multiplies the
real-machine regression matrix.

**Options.**

| Option | Mechanism | Cost / risk |
|---|---|---|
| A — unify on independent ELF + `BootHandoff` for all three archs | x86_64 abandons the inline form; shim becomes a pure table builder | Re-pays x86_64 real-machine verification that just turned green; `[ARCH]` three-place consistency |
| B — keep multiple forms, make the fork explicit | Shared boot helpers become crate-public API; both forms run host-equivalence tests; the inline form compile-time-asserts it never touches the handoff surface | Small change, but dual real-machine signatures remain; assertions only cover known pit classes |
| C — unify on inline | aarch64 rolls back | **Vetoed** — the TTBR0/TTBR1 dual-root inline form was proven dead on real hardware (§1.114) |
| D — riscv64 lands Plan A; x86_64/aarch64 stay; form count is booked | Equivalence assertions from B land with it | Form count temporarily +1; riscv64 is not blocked on boot-shim assembly work |

**Recommendation.** **D now (already adjudicated for riscv64) → B lands with D → A only when SD-2
triggers.** B's equivalence assertions are nearly free while D is in flight, and they convert the worst
pit class (`.bss` dual-world assumptions) into compile/host-time failures.

**Status / owner.** Open; sequencing bound by pre-adjudication E.2 (§5.2): unification work must not
start before riscv64 Plan A closes, and then only as a standalone project; the final form choice is a
user adjudication.

#### SD-2. Whether x86_64 migrates to Plan A (债②, hanging OQ)

**Recorded in:** NK4C-WORKLOG §1.115 遗留 债②; riscv-reviewlog §D.2.

**Current state.** x86_64 is immune to the root-switching problem by three stacked facts (single CR3;
`inherit_supervisor_half` copying PML4[256..512]; global pages force-inherited). aarch64 *must* use Plan A
(TTBR0/TTBR1 dual root; the inline form never entered TTBR1 — proven by incident). riscv64's single `satp`
is naturally immune, i.e. in the same "does not need Plan A" group as x86_64.

**Why structural.** Two live forms means every boot fix needs a dual real-machine signature. But the
review rounds' explicit conclusion: *no preventive refactoring* — the pain is potential, the risk of a
migration during an active bring-up campaign is real.

**Trigger conditions that reopen the OQ** (recorded in §D.2): (1) the inline form produces another
boot-time defect that is hard to localize (especially the `.bss`/relocation class); (2) a consumer of a
standalone kernel ELF appears (boot measurement, reboot reusing the startup segment, dual-kernel A/B
loading); (3) the `BootHandoff` contract needs a third real consumer to validate arch-neutrality (note:
riscv64 Plan A is *not* such a consumer — it self-bootstraps); (4) the x86_64 install surface leaves the
UEFI disk form.

**Options.** Hold the OQ with the trigger list (recommended) / migrate-all linked to SD-1 option A /
half-migration (produce a standalone artifact without switching the default — rejected: it precisely
creates fourth and fifth variants).

**Status / owner.** Open; user, half-pre-adjudicated by E.1 (§5.2): when aarch64 Plan A is stable for ≥10
real-machine verification rounds *and* x86_64 has no in-flight boot campaign, the main thread may open a
migration-evaluation project without asking; the endgame adjudication is still the user's.

#### SD-3. Platform descriptor / memory-map firmware source channel (债③)

**Recorded in:** NK4B-WORKLOG M3.4 (aarch64 ruling D1 > B > A, C dead); NK4C prompt stage 3 (riscv64
Plan A adjudicated); riscv-reviewlog §D.3.

**Current state (verified).** "Where does the production chain get platform facts (memory map, interrupt
controller, console)" has three archs and three answers. The arch-neutral channel exists
(`KernelInfo.platform_sources` + `parse_by_kind`, `os/libs/minix-platform/src/kind.rs`) — what was missing
is each arch's last wire. x86_64/aarch64 read UEFI config tables (ACPI/DTB); AAVMF supplies no DTB GUID and
its GICR fields stay zero, which killed the ACPI route for aarch64 (M3.4 forensics). riscv64's OpenSBI
hands the DTB physical address in `a1`, but the production entry ignored it; the boot-shim helper
hardcoded the QEMU-virt single-region memory map (`os/boot-shim/src/opensbi_helpers.rs`, `build_memmap`).

**Landing progress (verified, untracked work in progress).** `os/kernel-image/src/bootface.rs` is the
riscv64 Plan-A bootloader face: it parses `/memory` out of the DTB in `a1` (explicitly replacing the
hardcoded memmap, judged "production unusable"), consumes a boot file table (`MNXBOOT1`) to load the 12
modules, and reuses boot-shim's `build_kernel_info`. This is exactly the riscv64 leg of SD-3 landing.

**Why structural.** A lying or hardcoded memory map is a safety surface: the frame-pool bitmap capacity is
sized from it, and a wrong map silently mis-sizes boot memory (the §1.101 boot-OOM family). "Lucky memory
map masking the real problem" is the failure mode the repo has already paid for.

**Options.** A — per-arch firmware sources normalized into the existing channel (riscv64: DTB from `a1` —
landing; aarch64: ruling D1, a QEMU `dumpdtb` blob shipped as an ESP file; x86_64: config tables as now).
B — an explicit normalized-descriptor channel written by the install surface (`[ARCH]` KernelInfo contract
change). C — hardcoded constants + dev fallback (current boot-shim shape; rejected for production: it is
the lying surface itself).

**Recommendation.** A, which is what is landing. B only if a cross-arch descriptor need appears.

**Status / owner.** Adjudicated (per-arch normalization); riscv64 leg in flight; aarch64 D1 batch pending
on the main thread.

#### SD-4. Linker-script constants declared in parallel worlds (债④, second-source-of-truth family)

**Recorded in:** NK4B-WORKLOG M4.2 decision one; riscv-reviewlog §D.4.

**Current state (verified, worse than recorded).** The kernel's load addresses exist as *hand-maintained
copies* in at least four places: the production linker scripts (`os/kernel-image/{x86_64,aarch64,riscv64}.ld`,
e.g. `KERNEL_VIRT_BASE`/`KERNEL_PHYS_BASE`); the old per-arch `os/kernel/src/arch/*/link.ld` files, which
no build references but which doc comments and host tests still point at; the new bootface constants
(`os/kernel-image/src/bootface.rs`, `KERN_PHYS_BASE`/`KERN_VIRT_BASE`/`KERN_SIZE`, whose own comment admits
they are "same-source as riscv64.ld; drift guarded by check-layout.sh and on-machine reconciliation");
and the arch-generic direct-map base constants (`os/arch/src/arch/direct_map.rs`, `KERNEL_DIRECT_MAP_BASE`
per arch, encoding the same facts a third way). The guard against drift is a shell script plus human
discipline — not the compiler.

**Why structural.** This is the register's canonical "second source of truth": a parallel fact that
neither links nor dies. Deleting it has friction (doc anchors point at it); keeping it guarantees drift.
A wrong constant here does not fail a test — it fails a machine at 3 a.m.

**Options.**

| Option | Mechanism | Cost |
|---|---|---|
| A — delete the old three `link.ld` files; docs point at `kernel-image/*.ld` | Single fact source | Fix three doc anchors + host-test comments |
| B — reverse-absorb: old scripts become the contract | — | Highest risk; old-script semantics are incompatible with the boot-shim contract |
| C — keep both, annotate "documentation reference only" | — | Cheapest; leaves the pit |
| D — host tests read the *production* scripts | The host layout test switches its read target from the old scripts to `kernel-image/*.ld` and asserts the Rust-side constants against the parsed values — single-sourcing enforced by a test, not a comment | Small; the repo precedent is the check-layout "script ↔ expected-table" reconciliation and the in-repo `const _: () = assert!` base-address cross-checks |
| E — consume the script's own symbols from Rust (link-time single-sourcing) | The scripts *already assign* `KERNEL_VIRT_BASE`/`KERNEL_PHYS_BASE` as absolute linker symbols — Rust simply never reads them; option E declares them as `unsafe extern "C" { static KERNEL_VIRT_BASE: u8; }` and takes `&KERNEL_VIRT_BASE as *const u8 as u64`, exactly how C kernels consume `__bss_start`/`_end`. The hand-typed Rust copies (`bootface.rs`, `direct_map.rs` where the value is a link fact) become derived values, and drift becomes a link error or an obviously-wrong value, not a silent second source | Small per site; caveats: the absolute symbol must be kept alive (a real relocation must reference it, or `EXTERN(...)` in the script — the in-repo precedent consumes `__bss_start` from asm, so the asm-side reference pattern is proven but the Rust-side extern-static pattern is new here); applies only to constants that are genuinely *link-time facts* (link base, load base, size) — mapping configuration the linker does not know (e.g. `KERNEL_DIRECT_MAP_BASE`) stays a typed constant and keeps its host-test guard |

**Recommendation (revised in R2).** **E wherever the constant is a link-time fact** (the script is then the
single source in the strongest sense — the consumer *cannot* disagree), **D as the test-side complement**
for the remaining mapping-config constants, A as the tidy-up that follows naturally. B rejected. C is the
status quo and is the debt itself. Linux's per-arch `vmlinux.lds.S` + `asm-offsets` mechanism is the same
doctrine at kernel scale: derived constants are extracted from the artifact, never re-typed by hand.

**Status / owner.** Open; low-risk batch, main thread may carry it ride-along (§D.9 places it in the
"immediate" batch).

#### SD-5. Early console: four transmitters, one wrong port, one unbounded loop (债⑤)

**Recorded in:** NK4B-WORKLOG M3.3 review; riscv-reviewlog §D.5.

**Current state (verified).** Four independent serial emitters exist: the three `os/plat/src/{x86_64,arm64,riscv64}/early_console.rs`
backends (each with its own `write_byte`/`write_str`/`write_hex`; the hex loop is byte-identical three
times), plus a fourth, `os/boot-shim/src/lib.rs::emit_byte`, which re-implements COM1 output *and polls
the wrong port*: it reads `0x3f9` (IER) where the 16550 line-status register is `0x3fd`, so the
transmit-holding-register-empty bit never arrives and every byte spins the full 100,000-iteration cap
before passing. QEMU's synchronous serial drain keeps the production chain green — on a real machine the
throttling is simply void. Separately, the x86_64 plat backend's wait loop is *unbounded*
(`while (inb(COM1_BASE + 5) & 0x20) == 0 {}`): a serial fault on real hardware becomes an infinite loop.
The PL011 backend is the family's positive precedent: its transmit judgment was extracted into a shared,
bounded, host-testable pure function (`tx_wait_then_send`, commit `1a2a8eb61`).

**Why structural.** The transmit *contract* (wait bounded, CRLF normalization, upper limit) lives in N
copies with different semantics. Early console is the one channel every forensic round depends on; its
failure modes (silent stall, busy spin) are exactly the failures a forensics round cannot see past.

**Options.** A — boot-shim drops its private asm and uses the plat backend; x86_64 backend gets the
correct port + bounded wait (generalizes the PL011 precedent); one transmit contract in the shared layer.
B — minimal: fix the port and bound the loop, keep four implementations. C — freeze current behavior with
contract tests (locks the wrong port in place; rejected as a fix, valid as a stopgap).

**Recommendation.** **B first (same vehicle as the next x86_64 fix batch), A as a standalone structural
unit.** riscv64 needs no code — its SBI `console_putchar` ecall is firmware-managed; only a comment
alignment.

**Status / owner.** Open except PL011 (fixed); main thread.

#### SD-6. Boot bump pool has no reclaim path; kerninfo mapping responsibility not handed to VM (债⑬, "structure-debt triple" member two)

**Recorded in:** NK4C-WORKLOG §1.55/§1.56 (three-piece set); riscv-reviewlog §D.14.

**Current state (verified).** The loader's page-table page pool is pure bump: `prepare_boot(1024)`
(`os/boot-shim/src/main.rs`) — grown once from 128 pages after the ~40-bind deterministic panic
(incident B30). The whole `os/kernel/src/boot_alloc.rs` API has no free. Two welded costs: the *whole*
region is deducted from the VM free list whether used or not (4 MiB per boot), and every new-root bind
burns 1–3 pages for the kerninfo injection chain, so a bind-count ceiling remains. The kernel-side kerninfo
doc (`os/kernel/src/kerninfo.rs`) states the intended end state — the C design has VM map the kerninfo
section into every process and tell the kernel where; this kernel maps one reserved page itself "until VM
takes over" — but the takeover trigger has no implementation scheduled. Debt migrates with form: under
riscv64 Plan A the same no-free bump now lives in the OpenSBI helper / bootface path (the new
`opensbi_helpers::bump_alloc_pages` at least makes bootface a *second consumer of one implementation*
instead of a copy — the right instinct).

**Why structural.** A bootstrap allocator may be free-less, but must have an explicit handover moment;
Linux's counterpart (`free_init_pages` / memblock handover) returns boot allocations to the buddy system.
Here the design exists (kerninfo.rs) and the implementation does not; kexec/reboot/image-reload features
hit the ceiling the moment they appear.

**Options.** A — hand the kerninfo mapping responsibility to VM when it takes over user page tables
(established design; `[ARCH]` kerninfo contract face). B — make boot page-table pages reclaimable after
the direct map is up (needs tracking which bump pages are still referenced). C — keep re-growing the pool
by capacity estimate (costs welded, ceiling unchanged; short-term only).

**Recommendation.** **A** (design already exists), scheduled with the VM handoff closing batch; B as an
optional transition.

**Status / owner.** Open; main thread, VM-handoff batch. ("Structure-debt triple" member three,
`MAX_BIG_BLOCKS` hardwired 32, is **closed** — `MAX_BIG_BLOCKS` is now derived from pool pages,
`os/libs/minix-rt/src/alloc.rs`; member one is SD-11 here.)

#### SD-7. kernel-image entry does not clear `.bss` (riscv64 leg fixed; the class remains)

**Recorded in:** NK4B-WORKLOG (fact five of the M4.4 analysis).

**Current state (verified).** At snapshot, the riscv64 entry asm in `os/kernel-image/src/main.rs` clears
`.bss` via `__bss_start`/`__bss_end` (added to `os/kernel-image/riscv64.ld`), with the honest comment that
OpenSBI hands over bare RAM and NOBITS sections are not written — garbage there made the first allocation
return a garbage address. The x86_64/aarch64 `kernel-image` legs do not have the loop, but those legs are
"contract-slot producers" (the UEFI shim loads and zeros their images), so the exposure is latent, not
live.

**Why structural.** The entry contract of a standalone kernel image ("what state may I assume") was
implicit; the riscv64 leg made it explicit. The x86_64/aarch64 legs still rely on their loader doing the
zeroing — an assumption documented nowhere.

**Options.** A — add the same clear loop to all three entry paths (symmetry; harmless even where the
loader zeroes). B — document the per-arch entry contract in one place (kernel-image module header —
already a registered R3 backlog item) and leave the code asymmetry. C — status quo.

**Recommendation.** **A** — it is a few instructions per arch and removes an assumption class; pair with
the module-header documentation from B.

**Status / owner.** Partially fixed (riscv64); main thread ride-along.

---

### Part B — Memory: allocator, VM bootstrap, TLB

#### SD-8. VM never pre-materializes its working memory (C `reservedqueue` / `MAP_PREALLOC` missing; stopgap only) (债⑥)

**Recorded in:** NK4C-WORKLOG §1.55/§1.59/§1.92–§1.95; riscv-reviewlog §D.6.

**Current state (verified).** C's VM pre-maps its entire working memory *before starting* — a user-mode
page-fault servicer cannot service its own page faults without help. C has two layers: the reserved-queue
(`minix3/minix/servers/vm/alloc.c`, `MAXRESERVEDPAGES=300`, per-region `mappedin` flag) and `MAP_PREALLOC`,
which takes frames at region-creation time (`minix3/minix/servers/vm/region.c:492-499`). The Rust VM has
neither: `MmapFlags::PREALLOC` only records the `VrFlags::PREALLOC_MAP` bit (`os/servers/vm/src/mmap.rs`),
and `impl MemType for AnonymousMemory` (`os/servers/vm/src/memtype.rs`) has no eager-materialization
method (`ev_new` exists only on `ContiguousAnonymous`). What landed instead (§1.92–§1.95) is a two-layer
stopgap: a fixed 256 KiB initial-stack runway in the shared loader, and `MAX_BIG_BLOCKS` bound to pool
pages.

**Why structural.** The stopgap's boundary is a heuristic that is layout-sensitive: the §1.59 experiment
matrix proved that merely resizing static allocator constants shifts `.bss`/stack bases and crashes the
page-fault servicer itself. C's pre-map semantics do not depend on layout luck; the stopgap does, and the
"temporary form" is documented only in worklogs.

**Options.**

| Option | Mechanism | Cost / risk |
|---|---|---|
| A — eager materialization for PREALLOC regions | `AnonymousMemory` takes and maps frames at mmap time (C `region.c` counterpart) | Frame budget rises (the §1.59 family); needs a discriminating regression test |
| B — VM-backed growing heap | Heap supply goes through VM service (`allocator` → `VM_BRK` client leg; watch allocation re-entrancy) — "triple" member one, see SD-11 | Larger refactor; C `brk` fidelity is the payoff |
| C — honestly close the stopgap | Write the runway semantics into the design docs as "an evaluated temporary form", with the layout-sensitivity warning | No code; risk is successors mistaking it for the final state |
| D — extend kernel proxy service into VM runtime | Grow `establish_boot_dm`'s bootstrap role into VM's runtime | Blurs the kernel/VM responsibility boundary |

**Recommendation.** **C immediately (docs), A as an S-class unit gated on a discriminating test.** D
rejected. B is tracked separately as SD-11.

**Status / owner.** Stopgap landed; principled fix not implemented; main thread (C in the immediate
batch).

#### SD-9. `PAF_CLEAR`: the zero-fill pipeline exists end-to-end, but the demand-fault funnel never asks for it (债⑨)

**Recorded in:** NK4C-WORKLOG §1.101 (review MUST-1(a) follow-up); riscv-reviewlog §D.10.

**Current state (verified).** C's allocation requests carry an explicit "clear physical memory" demand
(`minix3/minix/servers/vm/vm.h:22`, `#define PAF_CLEAR 0x01`), with `VR_UNINITIALIZED` as the explicit
exemption. The Rust VM has the request bit (`os/servers/vm/src/phys_mem/types.rs`, `CLEAR = 0x01`), *all
three* allocator backends implement it (bitmap/buddy/segment-tree each have the zero-fill branch), and the
translation function exists (`to_alloc_flags`, `os/servers/vm/src/region/vir_region.rs`, carrying the
counterpart exemption logic). The one break: the demand-fault funnel `alloc_and_map`
(`os/servers/vm/src/cow_exec_pf.rs`) calls `alloc_pfn_reclaiming` (`os/servers/vm/src/alloc_page.rs`) —
whose signature has **no flags parameter**. Demand-fault frames never request CLEAR; nothing downstream
clears either. A comment in `vm_server.rs` registers exactly this as the follow-up.

**Why structural.** Today new frames happen to be all-zero — because boot has no page cache to reclaim
yet. `alloc_pfn_reclaiming`'s name says a reclaim path exists or will exist; the moment it serves a
previously-used frame, `.bss`/heap tails silently read the previous tenant's dirty bytes. Betting
correctness on an allocator *behavior* invariant instead of an explicit *request* is the textbook form of
"implicit invariant replacing explicit contract" — and it is a realistic candidate source for the stale
data the aarch64 mode-① investigation keeps circling.

**Options.** A — faithful C form: `alloc_and_map` constructs flags by region type (non-UNINITIALIZED →
CLEAR) and passes them through; `to_alloc_flags` is regularized (drop its `dead_code` allowance). B —
subset: only the demand-fault funnel passes CLEAR; the boot eager path keeps its own explicit zeroing.
C — delete the whole pipeline and document "new frames are zero" as an allocator contract
(rotts silently the day reclamation lands; rejected). D — debug-build poisoning at CLEAR-requested sites
(two divergent behaviors; rejected).

**Recommendation.** **A (or B — two tiers of the same fix)**, with the discriminating test that is easy to
construct (poison a fake frame; assert an UNINITIALIZED region is exempt and a normal region is cleared).
Linux's `__GFP_ZERO` is the same shape: zero-fill as an allocation *request*, never an allocator invariant.

**Status / owner.** Open; main-thread batch, recommended to ride the riscv64 Plan-A landing batch.

#### SD-10. The VM writes process page tables with no TLB invalidation on its side; the kernel's flush legs exist and are unused by design, and one design claim no longer holds (债⑪ + E-VMTLB family)

**Recorded in:** NK4C-WORKLOG §1.111 (original wording), §1.120 续-37/续-41/续-77a; riscv-reviewlog §D.12;
edge1 FIXLOG F2 ("kernel-assisted flush channel not wired", the `VmDm` present→X gap); edge_todo E-VMTLB
(kernel-side target-process TLB flush, true-SMP precondition).

**Current state (verified, with one leg landed since recording).** The kernel's proxy-flush legs are
implemented on all three archs (`os/kernel/src/syscall.rs`, `VmCtlParam::FlushTlb`/`InvlPg` →
`CurrentTlbArch`), and a "mark for flush before resume" half exists too (`mark_flush_tlb`, C counterpart
`MF_FLUSH_TLB`). The VM server calls none of them — *by design*: the claim was that every PTE write
self-flushes because the VM writes PTEs through its `PteChannel`-gated direct-map path, and the target
process's root switch on wake covers the rest. Two corrections to the recorded state: (1) riscv64's
`write_pte_dm` used to emit an unconditional `sfence.vma` from user mode (illegal instruction — making
VM's map leg non-functional on riscv64); that gating has **landed**
(`os/arch/src/riscv64/paging.rs::write_pte_dm` now flushes only on `PteChannel::KernelDm`, with the VmDm
rationale in the doc comment). (2) The remaining substance is real: the "every PTE write self-flushes"
claim covers only the PTE *page's own* translation, not the *target page's* stale translation in the
target process's TLB. For "exists→modify" operations (unmap/remap/permission flip through the VmDm
channel) on another process's address space, **nothing flushes the target's stale entry**. Unicore, the
window closes by accident of scheduling (the target always switches roots on wake); it reopens under SMP,
lazy root switching, or any modify-without-wake path. x86_64 masks the whole class because a CR3 rewrite
discards non-global TLB entries; aarch64 with stable ASIDs does not mask it.

**Why structural.** This is the "composition seam" genus: two individually-accepted designs — "VmDm writes
need no flush" and "kernel flush legs unused by VM" — compose into a hole that only exists between them.
It is also the registered true-SMP precondition (E-VMTLB): cross-CPU TLB correctness has no mechanism at
all yet.

**Options.** A — keep the design, close the riscv64 gating (done), and write the reopen conditions (SMP /
lazy-root / modify-without-wake) into the design claim and the ledger as standing constraints. B — VM
issues an explicit `vmctl InvlPg/FlushTlb` at the tail of every exists→modify VmDm operation (legs exist;
returns to C's model where VM self-flushes at four sites; costs one IPC round-trip per PTE modify). C — VM
marks the target process for flush before resume (`mark_flush_tlb` half exists; needs new "mark on behalf
of" vmctl semantics).

If B is ever chosen, it should be designed as a **batched flush, not a per-PTE IPC**: Linux's `mmu_gather`
is the reference shape — accumulate the affected address range (or a full-flush flag once the batch
overflows a threshold) across one logical page-table operation (unmap a region, remap an exec image,
flip a permission range), then issue *one* vmctl at the tail. A naive per-PTE vmctl turns every unmap
into N round-trips and would price the correctness fix out of the exec path. The C original gets this
batching implicitly (its four self-flush sites sit at operation tails, not per entry); the design doc for
B should state the batching contract explicitly rather than rediscover it under load.

**Recommendation.** **A now (the comment/ledger half remains), with B or C decided at the SMP milestone
when multi-core TLB reality exists to test against.** Do not pre-commit B/C before that evidence.

**Status / owner.** Partially fixed (riscv64 gating landed); the SMP-window decision is deferred by
design; main thread.

#### SD-11. Server heaps: fixed static pool with a bump-only supplier; the VM-backed growing heap (C `brk` fidelity) never landed ("structure-debt triple" member one)

**Recorded in:** NK4C-WORKLOG §1.54–§1.60 (B29/B30/B34/B35 incident chain); riscv-reviewlog §D.6 option B;
`os/libs/minix-rt/src/alloc.rs` module doc (self-identified).

**Current state (verified).** Every server image carries a fixed 1024-page `.bss` pool
(`GLOBAL_POOL_BYTES`, `os/libs/minix-rt/src/alloc.rs`); the supplier serves multi-page runs only from the
never-reclaimed bump region because the free stack cannot promise contiguity (the module's own comment
names "a coalesced free list, or a VM-backed heap" as the deferred structural fix); `release_page`
silently drops pages when its free stack is full. C servers keep their tables in BSS and grow a real heap
through `brk` to VM (`minix3/minix/servers/pm/brk.c` → `VM_PROC_NR`/`VM_BRK`). The capacity chain this
caused — four OOM incidents (B29 VFS heap, B30 page-table pool, B34 `MAX_BIG_BLOCKS` premature OOM, B35
conservative rebind) — consumed multiple forensic rounds. The two cheap closures *did* land:
`MAX_SLABS`/`MAX_BIG_BLOCKS` are now derived from pool size instead of hardwired.

**Why structural.** The pool is a capacity ceiling on every server's real working set; exec-time growth
(the thing that made `/bin/sh`'s rc workload OOM) is exactly what a fixed pool cannot absorb. The
allocator→VM leg has a known re-entrancy trap to design around (allocation during the VM call that
supplies pages).

**Options.** A — VM-backed growing heap via `VM_BRK` client leg (C-faithful; the "triple" member one).
B — coalescing free list inside the fixed pool (no VM dependency; still a fixed ceiling). C — status quo
with per-server capacity budgeting (what the capacity rounds effectively did).

**What the code inspection adds (R2).** The seam for option A **already exists and is the allocator's
designed extension point**: `pub trait PageSupplier { fn supply_pages(&mut self, page_count: usize) ->
Option<*mut u8>; fn release_page(&mut self, page: *mut u8); }` (`os/libs/minix-rt/src/alloc.rs:225`, with
a doc comment saying exactly this), and the fixed `.bss` pool is merely the current trait
implementation. Option A is therefore *one more trait implementation plus its wiring*, not a redesign —
the cost estimate above was pessimistic. The two design points that remain real: (1) **re-entrancy** —
the standard pattern is a small reserve: when `supply_pages` must itself allocate (its IPC buffers), it
serves from a pre-claimed reserve block, so no allocation ever recurses into the supplier; the reserve
size is a named constant with a diagnostic counter, not a heuristic. (2) **bootstrap ordering** — PM/VFS
start before or alongside VM, so the VM-backed supplier must be installable *after* birth: the allocator
starts on the static pool (today's behavior) and the supplier is swapped in once the server's first
`VM_BRK` completes. That swap-in point is one line at a server's init, and until it runs, behavior is
byte-identical to today — the migration is per-server and incremental. Redox is the live reference:
its user-space programs use a conventional growing allocator over pages faulted in from the `memory:`
scheme; the "fixed pool welded into `.bss`" shape exists nowhere in a production Rust OS.

**Recommendation.** **A**, using the existing `PageSupplier` seam, with the reserve pattern for
re-entrancy and the per-server install-after-birth ordering written down before code. B remains the
fallback only for servers that must run before VM exists at all.

**Status / owner.** Open, deferred repeatedly as "architecture-adjudication level"; user + main thread.

#### SD-12. The low-physical identity window is supervisor RWX (W^X debt)

**Recorded in:** edge1 FIXLOG Fix #9 F3; NK4A-REVIEW-REPORT §六 F3; NK4A-HANDOFF-STATUS §5 ("least
confident item #1"); R3 architecture panel (strict W^X registered as a pending unit).

**Current state.** The whole low-physical window — including reserved regions — is mapped supervisor
RWX because the UEFI memory map carries no W^X intent. The kernel executes from it with no per-section
permissions, a conscious divergence from C's per-section discipline, pinned by a mock assertion and an
`[ARCH]` annotation.

**Why structural.** It is not a bug today; it is a standing security posture decision that every later
hardening effort inherits, and it interacts with SD-1 (any boot-form change re-touches the mapping).

**Options.** A — boot-shim passes its own segment table (text/data/bss physical ranges) via an extended
KernelInfo; the window is split RX/RW per section (`[ARCH]`). B — keep RWX with the annotation until the
security milestone (status quo, honestly booked). 

**Recommendation.** **B now, A as the registered strict-W^X unit** — the segment-table channel is also
exactly the data SD-1 option A's handoff contract would carry, so A should be scheduled *with* whichever
boot-form project happens, not as an independent rework of the same mapping code.

**Status / owner.** Open/deferred by annotation; user (security milestone).

---

### Part C — Trap boundary, IPC, and ABI

#### SD-13. Message materialization at the trap boundary: the send side is closed by a barrier; the delivery side, `senda`, kernel calls, and riscv64 are open — and the real cure (a by-value IPC ABI) is a deliberately deferred architecture decision

**Recorded in:** `trap-boundary-message-materialization.md` (dedicated design analysis); NK4C-WORKLOG
§1.120 续-22…续-30; pattern-gate check P16; misc_concepts §3.33.

**The design tension.** An IPC message is "a block of memory plus an address." At the trap boundary the
same memory is observed by three parties with inconsistent guarantees: the programmer's sequential
semantics, the compiler's freedom to keep pending stores in registers, and the kernel's address-based
physical read. The API shape — a pointer converted to `usize` and pushed through inline asm — makes the
defect not just possible but *necessary* under any reordering. C Minix3 was immune only by implementation
convention (volatile, non-inlined libcall boundaries), not by language guarantee.

**Current state.** Fixed for the send/sendrec/sendnb legs: an explicit whole-object `commit_message_to_memory`
(`read_volatile`) barrier at the library boundary before `ipc_trap` (commit `2113be7fc`), plus a mechanical
pattern-gate check (P16). Open, all recorded in the design doc's §8.1: (1) the **delivery-side twin** —
the kernel copying a message *into* user memory (deferred-reply path) has the same reordering window; this
is the suspected mechanism family behind aarch64 mode ① (a kernel-delivered message with an illegal
`m_type` polluting INIT's stack page, inherited by fork, misread as a Vec capacity → ~4 GiB OOM). Root
cause chain converged; fix point not yet pinned ("not pinned, not patched" is the line's discipline).
(2) `senda` and the kernel-host `kernel_call` channel have the same-shaped window, deliberately not
barriered, pending an architecture review of long-term immunity. (3) riscv64: the barrier is cfg-gated
out because its kernel IPC leg is not wired yet; when wired, the barrier must be re-evaluated.

**The deferred decision.** The analysis explicitly records (§5.3) and rejects-for-now the structural cure:
a **by-value IPC ABI** (small messages passed in registers/copy-based like seL4/Zircon) eliminates the
window by construction instead of by barrier — but it is an IPC-ABI redesign, an architecture-evolution
decision that must not ride in on a bug fix. Linux/Redox/seL4/Zircon all eliminate this class by design
(register ABIs, `copy_from_user`, by-value payloads).

**Options.**

| Option | Mechanism | Assessment |
|---|---|---|
| A — complete the barrier family | Same explicit materialization barrier at: the delivery leg (kernel→user copy), `senda`, `kernel_call`; keep the P16 gate extended to those legs | Small, local, proven shape; keeps the pointer ABI |
| B — by-value / copy-based IPC ABI | Messages small enough travel in registers or are copied by the kernel on both legs; the pointer-plus-memory form remains only for bulk transfers | Eliminates the defect class by construction; full IPC ABI redesign; `[ARCH]` of the largest size; must be its own project |
| C — compiler-enforced boundary | Type the boundary so a non-materialized message cannot cross (e.g. an affine handle that `commit_message_to_memory` consumes) | Elegant; depends on unstable/advanced Rust features at the asm boundary; verify feasibility before committing |

**Recommendation.** **A now** (the delivery-side twin is the same fix shape as the landed send-side
barrier and is the highest-value leg because mode ① is blocking aarch64). **B as a scheduled user
adjudication** (it interacts with SD-18 endpoint encoding and the frozen message layout — one consultation,
not two). C only if B is rejected and the window keeps producing incidents.

**What the code inspection adds (R2).** Two facts de-risk option A. First, the delivery leg already has a
*single* trait seam: kernel-side delivery funnels through `delivermsg(proc, &dyn UserCopy)`
(`os/kernel/src/ipc.rs:747`) — the materialization requirement (the kernel's copy into user memory must be
a completed, non-speculative store sequence before the receiver runs) lands in one implementation, not per
callsite; the same holds for the send legs, which all pass through the one `IpcTransport` trait
(`os/libs/minix-sys/src/ipc.rs:503`). The barrier family is therefore four seam points, not dozens.
Second, the landed `commit_message_to_memory` (`ipc.rs:567`) already documents *why* it is a volatile
read rather than `compiler_fence`: an ordering-only fence does not force a register-resident store back
to the alloca; the volatile load does. That reasoning is correct and carries over verbatim to the
delivery side (a plain `ptr::copy` into user memory is not guaranteed visible to the receiving CPU's
fetch of the same physical frame on weakly-ordered archs without the completion barrier) — the mechanism
generalizes; only the call sites are missing.

**Status / owner.** Send side fixed; delivery side open (owned by the active mode-① investigation);
B awaiting user.

#### SD-14. The IPC status register lives on a callee-saved register (x86_64 RBX) — a port-side choice with no C authority, blocking milestone decisions

**Recorded in:** NK4A-QWEN-WORKLOG (triple identity); NK4B-WORKLOG P1 ("为什么本轮不直接修——上交裁决");
misc_concepts §3.2.

**Current state.** The C original defines `IPC_STATUS_REG` for i386 (`bx`) and for the ARM port (`r1`);
it defines **nothing for x86_64**. This rewrite's x86_64 port chose RBX — a callee-saved register the
compiler may keep live values in across the IPC boundary. The register now has three identities (birth
parameter, IPC status, compiler-live value), and the incident history (RS RBX clobber, commits c19a–c21a)
is the tuition. Mitigations landed (the wrapper saves/restores user RBX; conversion points have owners),
but the ABI choice itself stands, and NK4B's P2–P6 milestones hard-depend on the ruling.

**Why structural.** A cross-privilege-boundary state lane must live in a register the calling convention
does not guarantee to preserve — otherwise every compiler upgrade re-rolls the dice. This is the
single highest-value open ABI decision in the corpus.

**Options.** A — move the status register to a caller-saved register (r10/r11), matching the ARM port's
philosophy (status in a register the compiler will not rely on across the call); `[ARCH]` change.
B — restrict status writes to inside the IPC trap window (needs window detection; fragile).
C — take the status out of user registers entirely: a separate context field, never delivered to user
registers (largest behavior deviation from the C wire semantics).

**Recommendation.** **A** — no C x86_64 definition exists to deviate from, so choosing a caller-saved
register is a free-arch choice, not a fidelity loss; B and C both add machinery to defend a bad premise.
Present to the user with the milestone-dependency map.

**Status / owner.** Open; user adjudication required.

#### SD-15. Torn register file: the trap frame and the CPU context are two structures that must be kept in agreement by convention

**Recorded in:** NK4A-QWEN-WORKLOG (第六轮补充, explicitly labeled "structural risk record, not an issue of
this round").

**Current state.** Returning to user mode assembles the register file from two sources — the trap frame
(for the interrupt/iret payload) and the saved CPU context (for general registers, e.g. RBX restored by
offset into `X86_64CpuContext`). Any path that updates one and not the other delivers a *torn* register
file. Today this is safe only because one known rebuild path (`finish_and_restore` rebuilding the frame
from the context) maintains the pairing — an invariant held by convention, not by types. The aarch64
mode-① investigation spent seven refutation rounds on exactly this family before excluding it. A second,
arch-side instance (verified R2): `AArch64CpuContext.gp_regs` — the 30-entry X1–X30 save area — carries
the doc comment "Updated by trap entry path (future)" (`os/arch/src/arm64/boot.rs`); today it is written
only by kernel-side construction (context build, signal helpers) while hardware trap entry saves into the
separate frame structure. The split is per-arch real, and the "(future)" marker is precisely the
un-wired half of the pairing made visible.

**Why structural.** The type system permits states (half-updated pairs) that the invariant forbids; every
new trap/return path re-risks the pairing by hand.

**Options.** A — unify ownership: the frame lives inside the context (or vice versa) with one accessor
surface; no second mutable copy exists. B — debug-build pairing assertion: a version/tag word written to
both halves on every full save, checked on every restore (cheap, catches tearing at the moment it
happens). C — formalize the convention: one documented mutator API and a compile-time test that no other
code path touches the frame fields.

**Recommendation.** **B + C now** (cost is near zero; B catches the class at runtime, C freezes the
surface), A as the long-term shape if the family produces another incident.

**Status / owner.** Open; main-thread hardening batch.

#### SD-15b. The frame/context split has a concrete second front: `p_fault_addr` is filled on all three archs but consumed on one

**Recorded in:** NK4C-WORKLOG §1.120 续-40 §F ("a real design gap, registered as candidate independent
hardening").

**Current state (verified).** `forward_pagefault_to_vm` stores the fault address into the process
(`os/kernel/src/trap_dispatch.rs`, `os/kernel/src/page_fault.rs::set_page_fault_pending`) on all three
archs. The consumption leg — flushing the faulting page's stale TLB entry in the *resumed process's* context
before `iretq` — exists only on x86_64 (`os/kernel/src/lib.rs:3753-3760`, `invlpg`, with the real-machine
evidence comment from 2026-09-22: RS stuck in permanent same-address fault-refill because the clear point
in VM's CR3 could not kill the process's stale entry). aarch64/riscv64 fill the field and never read it.
A fix was implemented arch-neutrally (`CurrentTlbArch::flush_addr`) and **rolled back** after the real
machine falsified it as the mode-① root cause — the gap itself remains real and is recorded as a candidate
independent hardening, to be re-landed with its own evidence chain.

**Options.** A — re-land the arch-neutral flush at the resume point (the rolled-back change; needs its own
discriminating evidence per the line's discipline). B — delete the field on archs that do not consume it
(removes the one-sided contract; loses the designed resume point). C — leave + annotate (status quo).

**Recommendation.** **A**, bundled with whatever fix finally lands for aarch64 mode ① (the evidence chain
will exist then); C until that moment.

#### SD-16. Error-code sign domain: positive internal errnos and negative wire errnos cross several unguarded boundaries (family)

**Recorded in:** edge1 FIXLOG F10 (kernel-call positive-errno convention, whole-repo ABI reconciliation,
explicitly deferred as its own task); edge3 FIXLOG (fs_comm ERESTART fold on the wrong sign domain);
edge4 FIXLOG D36; NK4C §1.57/§1.63 (B33a); riscv-reviewlog §D.13 (station `calls.rs`); pattern-scan
"multi-arch lane-migration tails".

**Current state (verified).** The wire convention (C `main.c`) is negative-errno in the message type slot;
several internal Rust representations use positive errnos. Incidents: B33a — `ExecError::to_errno()`
returns positive; INIT's success lane swallowed it into a misleading `EIO`. The *masking* half was fixed
(folding to negative at the pm_exec boundary, NK4C §1.63). The *wire-encoding* half is still open:
`os/servers/pm/src/ipc/calls.rs` (the `PmCall::Exec` arm) still replies `e.to_errno()` un-negated while
sibling arms negate (`-PmError::from(e).to_errno()`), and `ExecError::to_errno` still returns positive
constants (`os/servers/pm/src/exec.rs`). Same family: the `fs_comm.rs` ERESTART fold keyed on the positive
domain while the wire state is negative; `Route::Pm` error branches that reply a bare negative errno
without echoing the endpoint, so PM mis-routes and the original VFS call hangs (registered as B18
adjacent-edge); riscv64's reply lane not yet migrated to the negative-encoding helper (`reply_wire`) that
x86_64/aarch64 use — a lane-migration tail that already flipped signs live once when a gate was relaxed
before its backend was ready.

**Why structural.** One semantic (an errno) has two encodings whose conversion is done at N ad-hoc sites
instead of one boundary. Each new leg is a fresh chance to flip a sign; the failures it produces are
wrong-error-code bugs that mask the real diagnosis (B33a masked EACCES as EIO).

**Options.** A — one boundary conversion: every server-internal classification keeps one signed domain;
a single `reply_wire`-style conversion at each server's reply edge; a same-type-site table enumerates the
legs (the repo's established doctrine for multi-arch families). B — mechanical gate: extend pattern-gate
with a check for un-negated `to_errno()` in reply positions (converts the family from "auditable" to
"blocked"). C — type-level: a `WireErrno` newtype so raw positive integers cannot be constructed in reply
positions.

**Recommendation (revised in R2).** **B immediately (it is one grep pattern), then A and C together.** The
first pass held C back as "durable if the table keeps growing"; on reflection that is backwards. A's
conversion table makes the *known* legs correct but leaves the *class* open — every future reply site is a
fresh chance to forget the negation, which is exactly how the `calls.rs` arm regressed in the first place.
C closes the class: a `WireErrno(i32)` newtype (or a `reply_wire`-style constructor that only accepts the
internal error enum, never a raw `i32`) makes the wrong thing unrepresentable — a positive errno in a
reply position becomes a type error, not a review finding. This is idiomatic Rust at near-zero cost
(newtype + one `From` impl per server), and it is the same doctrine the repo already applied to message
layouts ("single authoritative struct in `minix-types` + compile-time assertions"). C does not replace A's
per-leg migration work; it replaces A's *reliance on enumeration*, which is the part that keeps failing.

**Status / owner.** Partially fixed (B33a masking); family open; main thread; the riscv64 tail rides the
riscv IPC-bridge batch.

#### SD-17. `ps_strings`: the boot leg and the exec leg disagree about the argc slot width (4 vs 8 bytes)

**Recorded in:** NK4C-WORKLOG §1.120 续-34b §D (independently verified); riscv-reviewlog snapshot noted.

**Current state (verified).** The boot path sizes the argc slot as `size_of::<i32>()` (4 bytes) and places
`argvstr` at `sp + 4` (`os/arch/src/arch/boot.rs`, with the u64 pointer consequently not 8-byte aligned);
the exec path resolves the same layout with full-word (8-byte) slots (`os/libs/minix-sys/src/stack.rs`,
`os/commands/sbin/init/src/execve.rs`, shell `exec_frame.rs`, all standardized on `SLOT = 8`). No crash
today because the boot leg always writes `ps_nargvstr`/`ps_nenvstr` = 0, so nothing consumes the boot-side
argv area.

**Why structural.** Two producers of the same initial stack hold different contracts. The divergence is
the exact shape of the "one field, two meanings" family that produced the C `sizeof(argc)` LP64 bug in the
first place; the exec-side module doc records the correction, and the boot leg never followed it.

**Options.** A — boot leg adopts the 8-byte slot (one-line-class fix, aligns with the LP64 correction
already documented). B — cross-lock: a `const` assertion (or a shared constant in `minix-types`) that both
legs must consume, so the two can never silently diverge again. C — leave + annotate (current registered
state).

**Recommendation.** **A + B together** — A removes the divergence, B makes the class structurally
impossible. Neither has a behavior change while boot counts are zero, so this is a safe ride-along.

**Status / owner.** Open (registered "for the record"); main-thread ride-along.

#### SD-18. Endpoint encoding evolution (OQ-1) — the largest frozen-ABI decision, deliberately parked

**Recorded in:** edge4/new_edge4 OQ queue (OQ-1, "待用户"); riscv-reviewlog §E context.

**Current state.** The endpoint type's wire encoding has three recorded evolution options (narrow the
width / widen to 64 bits / decouple endpoint identity from the wire value), each with ten recorded
sub-questions (Q1–Q10). The queue records the discipline: all message-layout work is frozen while this is
open; nothing may pre-commit a layout.

**Why structural.** Every message-layout debt above (SD-13 ABI, SD-16 sign lanes, lane widths) inherits
the current encoding; the decision gates them all.

**Options.** As recorded (A narrow / B widen / C decouple). This register adds no option and makes no
recommendation: the queue's own discipline defers it to the user, and pre-adjudicating it here would
manufacture exactly the "hollow assertion" failure mode the lesson corpus warns about.

**Status / owner.** Open; user.

---

### Part D — Error-path discipline

#### SD-19. Silent error swallowing: 31 stations / 13 files, dispositioned; the type-level erase is the structural residue (债⑫)

**Recorded in:** riscv-reviewlog §D.13 (31-station point-by-point disposition table, round 6);
PATTERN-SCAN-REPORT §9.2 (three-layer taxonomy: tool-boundary swallowing, behavior-level swallowing, and
the message layer this entry covers); NK4C §1.106/§1.118–§1.119 forensics.

**Current state.** The family in one signature: `os/fs/fs-rt/src/transport.rs::copy_out` — **returns `()`**
and internally does `let _ = self.ipc.copy_to(...)`; a grant out-of-bounds (which the kernel hard-fails
with EPERM — established on the real machine: "out-of-bounds is not truncated, it is whole-block
rejected") is erased into a no-event at the *type level*. The real-machine consequence is on file: exec's
first block read returned `Ok` while the header bytes were zeros (comment in
`os/servers/vfs/src/exec_worker.rs`). The family's full inventory: ~30 `let _ =` stations wrapping
IPC/copy/reply calls across 12 files, plus the literal `-1` fallback reply that swallowed the real
EPERM for two forensic rounds (§1.118–§1.119), plus the positive-errno success lane (SD-16). The round-6
disposition table applied three questions per station (is the swallowed thing a hard failure or
best-effort / is it on the boot-marker path / does the C counterpart actually check) and dispositioned:
**7 stations need a diagnostic floor** (the error must land on a diagnostic surface; including the
`calls.rs` exec arm), **23 stations maintain-and-annotate** (the best-effort notify/reply swallow matches
the C-conventional form), **1 station needs a signature change** (the fs-rt `copy_out` above).

**Why structural.** Fail-stop beats fail-silent: silent errors shift debugging cost onto the next
forensics round — the two-round EPERM mystery is the measured cost. But blanket cleanup is explicitly
forbidden: the C counterparts genuinely do not check most notify paths, and "C fidelity" is the rewrite's
iron rule. The durable residue is the *type-level* erase (a `Result` squashed to `()` in a signature),
which no amount of call-site annotation can fix.

**Options.** A — signature rework: `copy_out`-class APIs return `Result`; ripples through filesystem
implementations (`[ARCH]`-class trait contract change). B — diagnostic floor: keep signatures; every
swallow passes endpoint + grant + offset + errno to the diagnostic channel; flagship points first.
C — family gate into the baseline: the grep patterns (`let _ =` × copy/send/reply + literal `-1` reply
legs) join the pattern-gate family; stock enters the baseline, new occurrences are blocked.

**Recommendation (mechanism upgraded in R2).** **C immediately → B (the 7 dispositioned stations) → A
(the one signature, as an S-unit).** The three hard-fail/critical-path fixes are: the `calls.rs` exec arm
(SD-16 overlap), the VFS main-loop FS-reply station, and the fs-rt `copy_out` signature.

Option C has a sharper tool than grep. The IPC surface already returns `Result` from a single trait
(`IpcTransport::send/sendrec/sendnb/notify`, `os/libs/minix-sys/src/ipc.rs:503`) — and `Result` is
`#[must_use]` in core, so every `let _ = transport.send(...)` station is *already* a silenced must-use
warning, silenced by the one idiom `let _ =` exists for. The clippy restriction lint
`let_underscore_must_use` flags exactly that idiom on must-use values. Enabling it converts the family
from "found by audit" to "found at compile time" for all future stations, while the 23
dispositioned-as-maintain sites get `#[allow(clippy::let_underscore_must_use)]` *plus the explanatory
comment the disposition table already demands* — the allow and the C-fidelity annotation are the same
line of maintenance, so the lint costs nothing extra. (Repo measurement R2: `#[must_use]` appears once
across `os/servers`, `os/libs/minix-sys`, and `os/fs/fs-rt` — the lint surface is untapped.) The literal
`-1` reply legs and the `()`-returning signatures are not must-use-shaped, so those two sub-families keep
the pattern-gate grep arm of C.

**Status / owner.** Disposition decided (round 6); individual dispositions open for implementation; main
thread.

---

### Part E — Per-architecture CPU contracts

#### SD-20. Cache maintenance on data→instruction transitions: one scattered `fence.i`, no aarch64 `dc cvau`/`ic iallu`, no contract (债⑩)

**Recorded in:** NK4C §1.115 CodeReview CONSIDER-5; riscv-reviewlog §D.10 (round-5 external correction).

**Current state (verified).** When executable bytes are written as data and then fetched as instructions,
the architectures disagree about what is required: x86_64 is self-consistent (nothing needed); aarch64
requires `dc cvau` + `ic iallu` + `isb` (ARMv8 D7.5.9) — a repo-wide grep finds **zero** hits; riscv64
requires `fence.i`, which exists in exactly one place (`os/kernel/src/arch/riscv64/higher_half.rs:54`,
the boot transition, hart0, one-shot — with the SAFETY comment saying so). The uncovered faces: user text
frames written by the ELF loader and then executed — every hart, every exec; and cross-hart instruction
sync after SMP bring-up (`fence.i` is hart-local; remote sync needs SBI `remote_fence_i`, which is
Linux's `flush_icache_range` + SBI remote fence shape). QEMU's functional model never reproduces the
fault class — QEMU-green must never serve as immunity evidence (a named lesson in the repo's taxonomy).

**Why structural.** This is a three-arch contract missing its abstraction; the scatter proves the
primitive is necessary on riscv64's boot path already.

**Options.** A — a per-arch cache-maintenance contract in the existing arch-trait family (`sync_icache(range)`:
aarch64 = the ARMv8 sequence; riscv64 = `fence.i`, later + SBI remote fence; x86_64 = no-op), called at
the ELF-loader tail and after boot-shim copies (`[ARCH]`; the in-repo `TlbArch`/`CurrentTlbArch` trait is
the direct precedent). B — riscv64 minimal: one `fence.i` at the exec image-building leg (leaves the
aarch64 face hanging; creates a second scatter). C — keep registered with a QEMU-only boundary declaration
(real-machine bring-up will rework it).

**Recommendation.** **A** — round 5 tightened the recommendation to A precisely because the scatter
already proves the primitive; B's savings are negligible against A's asm volume, and B adds scatter debt.

**Status / owner.** Open; main thread; rides the aarch64 closing batch / riscv batch.

#### SD-21. Inbound user-stack pointer is not realigned on aarch64/riscv64 (x86_64 has `and rsp, -16`) — a real ABI asymmetry, fix falsified as an unrelated root cause and rolled back

**Recorded in:** NK4C-WORKLOG §1.120 续-38/续-39; commit messages 续-39/续-40.

**Current state (verified).** `os/libs/minix-rt/src/crt0.rs`: x86_64 `_start` realigns (`and rsp, -16`);
aarch64 (`bl {birth}`) and riscv64 (`call {birth}`) do not — the 16-byte SP rule holds only if the
kernel-provided SP happens to be aligned. The measured boot processes were inbound with `sp % 16 == 8`.
An alignment fix was implemented, CodeReviewed clean, and rolled back after the real machine proved
alignment was *not* the OOM root cause (the poison value shifted by exactly the alignment slack, proving
the mechanism was elsewhere). The registration is explicit: the realignment is a true ABI violation, it
"should not be discarded, but it also should not ride on this OOM's fix."

**Options.** A — re-land the realignment as its own unit (both archs, plus the riscv64 symmetric TODO the
CodeReview added). B — align at the *source*: the kernel-side stack builder rounds to the ABI rule (then
crt0 needs nothing). C — annotate and defer until mode ① closes (its fix may share the vehicle).

**Recommendation.** **C now, then A or B with the mode-① batch** (if mode ①'s root cause touches the
stack-fill path, B is strictly better; if not, A is a two-line-per-arch change). The lesson to carry:
the fix was correct-by-inspection and still wrong-to-land — process discipline (falsification before
landing) worked, and the debt registration is the reason the work is not lost.

**Status / owner.** Rolled back; registered; main thread.

#### SD-22. `sstatus.SUM` / `PSTATE.PAN`: the kernel's user-memory access model is undecided on riscv64/aarch64 (DM-window copy is the recorded recommendation)

**Recorded in:** NK4B-WORKLOG (M3.x ruling section, options 甲/乙/丙 with 丙 recommended); riscv-reviewlog
§D7 (the same-shape decision table).

**Current state.** riscv64's `sstatus.SUM` is never set in production; aarch64's `PSTATE.PAN` likewise —
both archs rely on the two direct user-VA access sites being benign today. The recorded options: (甲) set
SUM per-process (coarse, must be annotated temporary); (乙) a copy-window guard (Linux
`enable_user_access()` style; the window pattern exists in the carrier test but would need promotion to
the arch layer); (丙, recommended) translate VA→PA and copy through the direct-map window — not a new
abstraction but the repo's standard three-step cross-space path, fixing riscv64 SUM, aarch64 PAN, and
future x86_64 SMAP in one change; only new code is page-crossing segmentation, with the precondition
"PA must be direct-map-covered" (a validator already exists).

**Options / recommendation.** As recorded: **丙**. The debt is that the decision is recorded and nothing
schedules it; it blocks aarch64 PAN verification (which in turn cannot be verified until SD-3's aarch64
descriptor batch lands).

**What the code inspection adds (R2).** Option 丙's vehicle already exists in the kernel and is in active
use: the typed cross-space channel `AddressRef::Process { .. }` + `CrossSpaceResult` (consumed at
`os/kernel/src/syscall_signal.rs:647-667` and `os/kernel/src/proc.rs:2364`, with the
`CrossSpaceResult::Suspended` arm already wired to `KcallResult::VmSuspend`). Finishing 丙 is therefore
migrating the remaining direct user-VA sites onto this channel and deleting the raw paths — not building
a new abstraction. The reference frame worth adopting alongside it is Rust-for-Linux's `UserSlice`:
there, user memory is not addressable as pointers at all; it is a type with `read`/`write` accessors
that perform the copy through the one audited channel. Giving `AddressRef` the same closed surface (no
public constructor from a raw user pointer that bypasses the DM-copy path on PAN/SUM archs) makes "kernel
accidentally dereferences user VA" a type error rather than a review finding — the same
make-illegal-states-unrepresentable doctrine as SD-16's `WireErrno`, applied to memory access.

**Status / owner.** Adjudicated-by-recommendation, not implemented; main thread.

#### SD-23. aarch64 FPSIMD state is never saved or restored across traps (independently confirmed defect, deliberately not fixed in passing)

**Recorded in:** NK4C-WORKLOG §1.120 续-42/续-43/续-44 (disassembly-quantified: INIT executes 163 vector
instructions, 110 of them vector stores targeting the stack); misc_concepts §5.26 (forensic by-products
get independent filings).

**Current state (verified; sharpened R2 — the first pass missed that the fix is mostly *built*).** The
context-switch path calls `FpuArch::enable/disable`, which are no-ops on aarch64; `CPACR_EL1.FPEN` is
globally open; no path saves or restores Q0–Q31. But the eager-save machinery **already exists, tested
and orphaned**: `os/arch/src/arm64/fpu.rs` implements `save(&mut State)` and `restore(&State)` as
complete `stp q0,q1 [{ptr}], #32` / `ldp` loops over a 528-byte `AArch64FpuState` (with size, alignment,
and zero-default unit tests), and every process already carries the slot (`KProcess.fpu_state:
CurrentFpuState`, `os/kernel/src/proc.rs:1010`). A repo-wide grep finds **zero callers** of save/restore
outside the fpu module's own tests. Two promise-comments describe the missing wiring as if it existed:
`enable`'s comment says "Context switch uses explicit save/restore instead of trap-on-use" (it does
not), and the `fpu_state` field doc says it is "saved by `FpuArch::save` when the process releases FPU
ownership" (no such call). This is the register's canonical orphan-primitive shape — the same genus as
the unwired `send_work()` drain primitive — wearing two promise-comments.

**Why structural.** The cross-arch FPU abstraction was migrated with the arch leg built and the switch
leg never wired — a composition seam, not missing code. And the incident discipline worked: the defect
was confirmed, quantified, and *not* fixed in passing because it was proven not to be mode ①'s cause.

**Options.** A — lazy restore mirroring C (`copr_not_available_handler`): FPEN off; trap on first FP
instruction saves/restores — C-faithful, zero steady-state cost for FP-free paths. B — eager full
Q0–Q31 save/restore at context switch — the machinery exists; the work is wiring the save/restore calls
into the switch path plus an FPU-ownership field (which process's state is live). C — scope-limit
declaration (no FP in kernel-adjacent user paths) — honest but unenforceable.

**Recommendation (re-ranked in R2).** **B**, not A. Three arguments converged: (1) *Cost asymmetry* — B
is wiring two existing, already-tested calls (512 bytes via paired STP/LDP per switch); A is a new trap
class, an ownership/`fpu_enable_el0` state machine, and interaction with signal delivery — the C i386
tuition this repo has already paid once on x86_64's CR0.TS. (2) *Industry practice* — Linux/arm64 moved
off trap-based lazy FPSIMD to eager save (lazy mode dropped as the default in the 4.5-era kernels):
trap-lazy is fragile with signal handlers and multi-CPU, and eager's 512-byte cost is cheaper than the
fragility; Redox likewise saves FPU/SIMD state eagerly in its context structure. (3) *Behavior
fidelity* — lazy vs eager is an internal strategy; externally both preserve C-observable behavior, so B
does not violate the rewrite's iron rule. Keep A in the ledger as a possible later optimization only if
a real-machine profile ever shows switch-path cost that matters; do not pre-build it.

**Status / owner.** Open ("independent project, worth its own fix + CodeReview"); main thread. The
promise-comments (fpu.rs `enable`, proc.rs `fpu_state` doc) should be corrected in the same batch that
wires the calls — a comment describing absent machinery is the taxonomic "promise-comment counted as
implementation" failure, and it is what let this debt hide through multiple review rounds.

---

### Part F — Scheduling and SMP

#### SD-24. True SMP is fenced off by a transitional clamp with three un-repaid prerequisites

**Recorded in:** NK4C-MIGRATION-20260930 §5 (the CONTRACT three-piece set); NK4C-WORKLOG 续-55…续-73;
misc_concepts §4.6 (transitional deviations must register removal conditions — this is the discipline
working as designed).

**Current state (verified).** `clamp_cpu_to_bsp` (`os/kernel/src/sched.rs:307`) pins all scheduling to the
bootstrap processor; its comment registers the removal conditions. The un-repaid arms, recorded as a
CONTRACT set in code comments (not in a debt ledger — this register promotes it): (1) the interrupt-entry
`context_stop_idle` arm — the port's idle flag is only ever set true, never cleared (the C counterpart
`arch_clock.c` clears and re-arms the timer), an E8-class one-sided capability; (2) the enqueue wake-IPI
arm (C `proc.c` `smp_schedule`) — without it, a woken task on another CPU stays unscheduled; (3) the
CPU-affinity guard is landed as a *transitional* clamp itself (`clamp_cpu_to_bsp` doubles as arm 3 until
the real affinity guard lands). Beneath these: the shared single IDLE process slot (C gives every CPU its
own idle process) — confirmed not to be the current corruption's cause but registered "still to do"; and
the large open forensics: AP-runs-user-process cross-CPU VM corruption, sixteen rounds 续-57…续-72, one
arm fixed (idle timer stop), the family not closed.

**Why structural.** SMP correctness here is not one bug but a set of missing arms whose absence is masked
by the clamp. The clamp converts a correctness domain into a scheduling restriction — fine as a transition,
but the removal conditions are the actual deliverable.

**Options.** A — land the three arms as one batch (idle-clear + wake IPI + real affinity guard), then
remove the clamp, gated on the AP-user-process forensics closing. B — keep the clamp, land arms one at a
time with per-arm real-machine signatures. C — formalize: promote the CONTRACT comment set into the debt
ledger with owners (this register's act), defer the code.

**Recommendation.** **C is done by this register; A is the target shape but B's per-arm signatures are the
safer execution** given the AP forensics is not closed. The clamp must not be removed by any batch that
does not name all three arms as landed.

**Status / owner.** Open; main thread, SMP window.

---

### Part G — Diagnostics, tooling, and process

#### SD-25. Committed probe and diagnostic-facility governance (债⑧)

**Recorded in:** riscv-reviewlog §D.8; R3 §五.1; NK4-REGRESSION-REVIEW 观察-2; edge1 FIXLOG (probe
inventory awaiting the task1-close adjudication).

**Current state.** ~160 committed probe references across 29 files — intentionally retained forensic
instrumentation (schedctl/en/rcvi/…), with two registered dead probes (`vmpt2bf`, `sas-send`) awaiting the
established "task1-close" grand adjudication. The §1.119 lesson raises the stakes: serial evidence under
flood is unreliable, and probe credibility determines forensic round cost. A newer lesson (续-86/87)
sharpens it: *any per-iteration probe changes handshake timing* — probes are not passive.

**Options.** A — the established task1-close adjudication (per-branch keep/delete). B — a layered
diagnostic regime: permanent boot milestones / limited-count forensic probes (cap + gate + rollback
discipline) / committed diagnostic facilities (formal API), with a checklist script joining pattern-gate.
C — delete everything non-milestone (loses live breadcrumbs; rejected).

**Recommendation.** **Use B's layered contract as the criterion for A's adjudication** (first decide what
may be committed, then judge branch by branch).

**Status / owner.** Open; the established task1-close session.

#### SD-26. Observability is layout-coupled: inserting a probe into PM changes memory layout and breaks boot (registered, never fixed, worked around for months)

**Recorded in:** NK4C-WORKLOG §1.33 ("attack PM layout fragility, otherwise all PM development is
blocked"); B17/B21/B22 (successive fixes explicitly designed to not touch the PM crate).

**Current state.** A fixed-address assumption (or module-page ceiling) in the PM path means any PM-side
instrumentation shifts layout and deterministically regresses boot. The recorded decision was to *bypass*
rather than fix; multiple later fixes carry "deliberately does not touch PM" as a design constraint.

**Why structural.** The observability channel — the thing every forensic round needs most — is the one
crate it cannot safely instrument. This is a debt that *amplifies every other debt's* diagnosis cost.

**Options.** A — find and remove the fixed-address assumption (the §1.33 plan A; needs the layout audit
nobody has had bandwidth for). B — instrumentation that is layout-neutral by construction (probe via the
existing diagnostic syscall channel instead of in-band code). C — continue bypassing (the implicit status
quo).

**Recommendation.** **B as the immediate unblock** (diagnostic-channel probes are how the recent rounds
already work around it — formalize that), **A as the structural unit** when the PM crate is next open for
a batch.

**Status / owner.** Open; main thread.

#### SD-27. Six groups of wrong C anchor references, and the anchor-checking tool cannot see Rust

**Recorded in:** edge1 FIXLOG (registration table); NK4B-WORKLOG (same table, plus the tooling gap).

**Current state.** Eighteen lines across the `smp.rs` family and five other sites cite C files/line ranges
that do not exist (nonexistent files, off-by-N line ranges, a 262-310 citation in a 243-line file). The
anchor-resolver script only checks documentation, not Rust comments.

**Why structural.** The rewrite's ground-truth chain (C source > design doc > Rust code) is enforced
through these anchors; wrong anchors are fake provenance that survives every review because the tool does
not look.

**Options.** A — batch correction of the six groups. B — extend the anchor resolver with a Rust-comment
mode (`tools/anchor-resolve.sh` gains `.rs` scanning). C — both.

**Recommendation.** **C** — mechanical, low-risk, and it converts a recurring review finding into a gate.

**Status / owner.** Open; independent batch proposed in the FIXLOG.

#### SD-28. The multi-arch test harness swallows build failures

**Recorded in:** edge1 FIXLOG (environment-trap registration, two incidents).

**Current state.** `run_all.sh`'s arch loops invoke builds with `|| echo "(build failed)"` — an exit-code
swallow that converted two real cross-arch build-break classes into "passing" harness runs. Related
residual: aarch64 `smp-topo` test has been red since a platform commit while still registered as PASS in
the run list (fixing it means changing the gate — an architecture decision, per the FIXLOG).

**Options.** A — propagate exit codes (fail the loop). B — A + the red-test gate decision as part of the
SMP window.

**Recommendation.** **A immediately** (one line each), B with SD-24's batch.

**Status / owner.** Open; harness batch.

#### SD-29. Cross-crate testability surfaces: several servers cannot be integration-tested from the host because their internals are `pub(crate)`

**Recorded in:** edge4 FIXLOG/E5(b) (VM queue surface); edge4.md C-4 (VM paging smoke surface); edge2
FIXLOG TIMES (PM clock source has no injection seam).

**Current state.** `minix-vm`'s pending-VFS-call queue and paging surfaces are `pub(crate)`, so the
cross-crate integration tests cannot reach them; PM's time source hard-wires its transport, so the hosted
dispatch unit tests cannot substitute a fake clock. Each is small; together they define which contracts
can be *tested* vs only *reviewed*.

**Options.** A — export deliberate test-facing surfaces per server (the repo's "host-linkable surface"
doctrine, already uneven per server). B — injection parameters at construction sites (clock source; queue
handle). C — accept wire-contract cross-verification for the un-linkable servers (the doctrine's fallback).

**Recommendation.** **B where a constructor seam exists (PM clock), A for the VM queue family** (the
integration tests are already written against it and blocked).

**Status / owner.** Open; rides whichever server's next batch opens the crate.

#### SD-30. Workspace feature-unification: the mock umbrella feature prevents single-invocation builds and multi-arch checks

**Recorded in:** edge2 FIXLOG NL5③/NL4; new_edge1 NK2-A; NK4A-HANDOFF-STATUS §6 (the "host fake-green"
environment trap).

**Current state.** `minix-kernel` defaults to `feature = "mock"`; production builds must opt out; a bare
`cargo check --workspace` pulls std paths and fails with hundreds of unrelated errors; the CI therefore
splits invocations, and "the host test passed" has repeatedly meant "the mock configuration passed" — a
recurring false-green trap (the FIXLOG calls it 宿主假绿).

**Options.** A — flip the default: production features by default, `mock` opt-in (touches the workspace's
most load-bearing Cargo.toml; needs the CI matrix re-verified). B — keep defaults; add a gate script that
asserts which feature set each invocation used (cheap; converts the trap from silent to loud). C — split
mock into a separate crate (largest rework, cleanest semantics).

**Recommendation.** **B now, A as the structural fix when the CI matrix is next touched** (A was already
deferred "to ride the NS8 wave" and that wave has since closed — re-register rather than let it lapse).

**Status / owner.** Open/deferred; build-infra batch.

#### SD-31. The ledgers themselves are unreliable (meta-debt: 10 of 52 open items stale at audit)

**Recorded in:** edge2 FIXLOG LEDGER-AUDIT ("台账失真率" — ledger distortion rate; "never trust the
ledger, verify against code before picking a task"); edge3 FIXLOG Fix #131 (a ⏳ row claimed pending work
that had already landed); the five `new_todo_*` scans whose items were superseded by evolution.

**Why structural.** Every coordination artifact in this line (todo ledgers, FIXLOG status marks, worklog
summaries) is hand-maintained; the audit found ~20% of open-marks wrong in both directions. Wrong "open"
marks cause duplicate work; wrong "closed" marks cause silently dropped debts — this register itself had
to re-verify statuses against code (Appendix B) because at least two recorded statuses were stale
(a fix claimed by one ledger, still-open by another).

**Options.** A — periodic audit sweeps (what happened in edge2; labor-intensive). B — machine-checkable
status: every open item carries a grep-able anchor and a verification command; a script re-checks anchors
and flags stale rows (the Appendix B table of this register is the prototype). C — single-source the open
items into one ledger with mandatory anchor fields.

**Recommendation.** **B**, grown out of this register's Appendix B — it is the cheapest mechanism that
makes staleness *visible* instead of merely occasional.

**Status / owner.** Open; process.

#### SD-32. Thread model and futex have no owner

**Recorded in:** edge4/new_edge4 OQ-4 ruling (thread model → 14-stage-runtime, futex as an `[ARCH]`
evolution sub-item "needs PM/kernel-coordinated reopening of a cross-boundary change point");
new_todo_qwen P2-5 (the same gap, five days earlier, from an independent scan).

**Current state.** Minix3 commands are single-threaded, so nothing blocks — and precisely therefore the
gap survives. The OQ-4 ruling assigned ownership to the runtime stage but no batch exists.

**Options.** A — open the `[ARCH]` thread-model design in the runtime stage as planned. B — keep deferred
with an explicit re-registration at each planning cycle. 

**Recommendation.** **B** — it is genuinely non-blocking; the failure mode to prevent is the silent lapse,
which the re-registration handles.

**Status / owner.** Open/deferred by ruling; user + runtime stage.

---

## 4. Cross-cutting families: what the thirty-three debts have in common

The thirteen chapter-D debts close with a thesis; extending it across the whole corpus:

> The shared root cause is that **contract surfaces grew parallel implementations during the port**, and
> the mechanisms that collapse parallels (table-driven gates, contract tests, trait promotion, layered
> regimes) already have successful in-repo precedents. Implementation cost is rarely the constraint;
> the scarce resources are adjudication rhythm and real-machine verification batches.

Ten recurring families, each with its in-repo cure precedent:

1. **Second sources of truth** — facts declared in two+ places with no compiler-enforced link (SD-4 linker
   constants ×4 worlds, SD-17 argc slot, SD-27 anchors). Cure precedent: check-layout "script ↔ table"
   reconciliation; `const _: () = assert!` cross-checks; host tests that read the production artifact.
2. **Composition seams between individually-accepted parts** — every piece passes acceptance; the risk
   lives between them (SD-10 VmDm gating × "every write self-flushes"; SD-9 backend-ready/funnel-blind;
   SD-13 barrier on send legs only; SD-23's arch-side save/restore built and never called by the switch
   path). The FPU case adds the sharpest sub-shape: an orphan primitive plus a promise-comment that
   describes the wiring as existing. Cure precedent: pattern-gate P16; the os/tests per-contract files;
   and for orphan primitives specifically, a "public API with zero production callers" dead-code sweep
   (the code-excellence rule) — both fpu save/restore and the unwired `send_work()` drain would have
   surfaced in one.
3. **Type-level erasure of errors** — signatures that squash `Result` into `()` (SD-19 flagship), positive
   errno lanes (SD-16). Cure precedent: the round-6 three-question disposition table; clippy must-use
   linting.
4. **Architecture-fork double maintenance** — three archs × N forms with per-arch tails (SD-1, SD-16
   riscv reply lane, SD-20 cache maintenance, SD-23 FPSIMD, SD-22 SUM/PAN). Cure precedent: the
   same-type-site table doctrine ("same-type site tables for new arch porting"); `TlbArch` trait
   promotion.
5. **Promise-comments counted as implementation** — "SUM is set later", "notification would happen
   automatically" (SD-24's CONTRACT comments until now; the exec-rebind livelock's root). Cure precedent:
   the fail-closed pt_bind redesign; promise-phrase detection in review checklists.
6. **Environmental-luck invariants** — "new frames are zero", "page reuse masks stale roots" (SD-9, SD-10
   unicore window). Cure precedent: explicit request flags (`__GFP_ZERO` shape); fail-closed validators.
7. **Capacity ceilings deferred by capacity rounds** — SD-6, SD-11 (four OOM incidents before the cheap
   closures landed). Cure precedent: deriving limits from pool size instead of hardwiring.
8. **Silent no-op layers** — riscv port_io stubs returning success; the wrong-port console poll that
   "works" on QEMU (SD-5). Cure precedent: the per-arch no-op-stub checklist bound to a wiring plan.
9. **Transitional guards without enforced removal conditions** — SD-24's clamp (correctly self-registering),
   SD-8's runway. Cure precedent: misc_concepts §4.6 — "transitional deviations must register removal
   conditions in the same commit."
10. **Blocking semantics structurally inexpressible in the current ABI** — the riscv dispatch function
    returning unit cannot express park (riscv IPC-bridge step 1), and the pointer-plus-memory IPC ABI
    whose full cure is by-value message passing (SD-13). These are the two genuine *architecture
    evolution* items; everything else is consolidation.

---

## 5. Remediation strategy

### 5.1 The batch plan (extends riscv-reviewlog §D.9 with the new entries)

| Batch | Contents | Precondition | Adjudication |
|---|---|---|---|
| **0 — immediate, ride-along (docs/tests/one-liners)** | SD-4 D (host tests read production ld scripts); SD-5 comment alignment; SD-7 A (.bss symmetry); SD-8 C (stopgap documented as evaluated-temporary); SD-16 B (pattern-gate sign check); SD-17 A+B (argc slot); SD-19 C (swallow-family gate + the `let_underscore_must_use` lint with allow-annotated maintain sites); SD-27 C (anchor tool); SD-28 A (exit codes); SD-24 C (done by this register) | none | main thread自主 |
| **1 — riscv64 Plan-A landing batch (already adjudicated)** | SD-3 riscv leg (in flight); SD-9 A; SD-20 A riscv face; SD-10 A's riscv gating (done) + comment; SD-1 B's equivalence asserts; SD-16 riscv reply lane; SD-21 C→re-evaluate | Plan A works on the real machine | adjudicated (NK4C prompt stage 3) |
| **2 — aarch64 closing batch** | SD-13 A delivery-side barrier (with mode-① fix); SD-20 aarch64 face; SD-23 B (eager FPSIMD — wire the existing save/restore); SD-22 丙; SD-3 aarch64 D1 | mode ① root cause lands | main thread |
| **3 — error-path & sign-domain batch** | SD-16 A+C; SD-19 B (the 7 stations) + A (fs-rt signature, S-unit); SD-5 B (port + bounded) | none | main thread |
| **4 — VM handoff closing batch** | SD-6 A (kerninfo responsibility to VM); SD-8 A (eager materialization, with discriminating test); SD-11 A design (re-entrancy) | VM takes over user PTE work (established trigger) | design exists; scheduling = main thread |
| **5 — SMP window batch** | SD-24 A/B (three arms, then clamp removal); SD-10 B-or-C (multi-core TLB decision with real evidence); SD-28 B | AP-user-process forensics closes | user (gate changes) |
| **6 — ABI consultations (user, not code batches)** | SD-13 B (by-value IPC) + SD-18 (endpoint encoding) as one consultation; SD-14 (status register) with milestone map; SD-2/SD-1 endgame per E.1/E.2 | per trigger conditions | **user** |
| **7 — standing hygiene** | SD-25 (probe regime via task1-close); SD-26 B then A; SD-29; SD-30 B then A; SD-31 B (anchor-checked ledger); SD-32 B | none | main thread / process |

### 5.2 Binding pre-adjudications (reproduced constraints — this register does not override them)

From `riscv-reviewlog.md` chapter E (user pre-adjudications, 2026-09-27; revision requires the user's
personal consent):

- **E.1 (SD-2)** — pre-adjudication is *half*: when aarch64 Plan A is stable for ≥10 real-machine
  verification rounds **and** x86_64 has no in-flight boot campaign, the main thread may open the
  migration-evaluation project without asking; the endgame adjudication (migrate or suspend) is still the
  user's.
- **E.2 (SD-1)** — the three-form unification must **not** start before riscv64 Plan A closes; after
  closing it is a **standalone project** (not piggybacked); the final form choice is an endgame user
  adjudication.
- **E.3 (riscv64 install/handoff endgame)** — explicitly **not** pre-adjudicated: the choice depends on
  riscv real-machine measured results that static analysis cannot substitute for; it merges with E.2's
  endgame into one consultation.
- Chapter discipline: pre-adjudication ≠ final adjudication; where this chapter conflicts with chapter D's
  ownership notes, chapter E prevails; conflicts get errata added under chapter E entries, without
  retro-editing chapter D.

### 5.3 What NOT to do (recorded vetoes and their reasons)

1. **No blanket error-swallow purge** (SD-19) — most swallows are C-faithful best-effort forms; the
   three-question table is the filter.
2. **No preventive x86_64 boot-form migration** (SD-2) — the trigger list, not aesthetics, opens the OQ.
3. **No probe mass-deletion** (SD-25) — live breadcrumbs earned their keep across §1.100+ rounds; the
   layered regime decides, not a purge.
4. **No "fix in passing" for confirmed-but-unrelated defects** (SD-23 FPSIMD, SD-21 realignment) — the
   line's discipline (falsify, file independently, land with its own evidence) already saved mode ①'s
   investigation twice.
5. **No message-layout pre-commitment** (SD-18) — frozen until the endpoint-encoding consultation.

---

## 6. The single highest-leverage recommendation

If only one thing is done from this register, it is **Batch 1 (the riscv64 Plan-A landing batch), executed
with Batch 0's ride-alongs attached** — not because riscv64 is the most important goal in itself, but
because:

1. It is **already adjudicated** (no consultation latency — the scarce resource per the corpus's own
   thesis).
2. Its change surface carries **four debts' fixes in passing** (SD-3 riscv leg, SD-9, SD-20 riscv face,
   SD-10's comment half) plus SD-1's equivalence asserts and SD-16's lane tail — the highest debt-density
   of any available batch.
3. It opens the **third real-machine evidence channel**, which batches 2 and 5 both explicitly wait for.

And the one *meta* investment that compounds across every future batch: **SD-31 option B — the
anchor-checked ledger.** Every debt in this register carries a grep-able anchor; a script that re-checks
them (as Appendix B did by hand) makes ledger staleness — the corpus's most repeated meta-failure —
visible for the cost of a cron job. The second-compounding investment is the **contract-test collapse
pattern**: SD-4 D, SD-5 A, SD-10 A, SD-20 A, SD-15 B/C, SD-16 B/C all convert a real-machine-only
verification into a host-test verification. Every debt fixed that way stops consuming real-machine
rounds — the truly scarce resource.

---

## 7. Questions reserved for the user

The register's own analysis answers everything below except these; they are listed so an external review
can weigh in:

1. **IPC ABI endgame (SD-13 B + SD-18).** Should the line schedule the by-value/copy-based IPC message
   evaluation jointly with the endpoint-encoding consultation, or keep the barrier-family approach
   (option A) as the permanent answer for the pointer ABI?
2. **x86_64 IPC status register (SD-14).** Confirm option A (caller-saved status register) or pick B/C —
   this blocks the NK4B milestone chain.
3. **Server heap endgame (SD-11).** VM-backed growing heap (C-faithful, needs bootstrap-ordering design
   for servers that start before VM) vs coalescing free list inside the fixed pool?
4. **W^X posture (SD-12).** Confirm deferral to the security milestone, with the segment-table channel
   coupled to whichever boot-form project happens (SD-1).
5. **Strict-W^X aside — thread model (SD-32).** Confirm continued deferral with re-registration, or open
   the `[ARCH]` design now.
6. **Register maintenance (SD-31).** Adopt the anchor-checked ledger (option B) as a standing tool?

---

## Appendix A — Corpus scanned

Register sources (all under `rewrite-notes/` unless noted): `riscv-reviewlog.md`,
`NK4C-WORKLOG.md`, `NK4B-WORKLOG.md`, `NK4A-QWEN-WORKLOG.md`, `misc_concepts.md`,
`PATTERN-SCAN-REPORT-20260923.md`, `trap-boundary-message-materialization.md`,
`NK4C-AARCH64-EXEC-REBIND-LIVELOCK.md`, `NK4C-BUG-AARCH64-VEC-CAP.md`, `NK4C-BUG-AARCH64-VEC-CAP-GLM.md`,
`NK4C-REVIEW-REPORT-20260923.md`, `NK4C-REVIEW-REPORT-20260923-R2.md`, `NK4C-REVIEW-REPORT-20260927-R3.md`,
`NK4C-R31-P1-AUDIT-TABLE-20260930.txt`, `NK4C-REVIEWER-SESSION-RECORD-20260930.md`,
`NK4C-MIGRATION-20260930.md`, `NK4A-REVIEW-REPORT.md`, `NK4-REGRESSION-REVIEW-20260922.md` (+PART2),
`NK4A-HANDOFF-STATUS.md`, `edge1..4.md`, `new_edge1..4.md`, `edge_todo.md`, `edge_todo_archive.md`,
`new_todo_{HY4,deepseek,glm,muse,qwen}.md`, `NK4C-{OPENING,RESUME,GLM53}-PROMPT.md`,
`MISC-CONCEPTS-SESSION-HANDOFF-20260927.md`, `.review/zcode/edge{1,2,3,4}/FIXLOG.md` (+ edge3 archive).

Scanned, no unique design-debt content: `edge1..3.md` (frozen pointer indexes), `NK4A-TODO.md`,
`NK4B-TODO.md`, `edge_todo_archive.md` (closed-only), `NK4C-R31-P1-AUDIT-TABLE-20260930.txt` (mechanical).

## Appendix B — Anchor verification log (snapshot `8d0a177ed`, 2026-09-30)

| Claim | Evidence (tool-derived) | Verdict |
|---|---|---|
| Three boot forms exist | `os/boot-shim/src/main.rs` "inlined arch_boot path" comment; `os/kernel/src/lib.rs` `bootstrap_to_kernel_image` cfg; `build_bootstrap_root_and_enable` shared | present |
| Fourth form forming | `os/kernel-image/src/bootface.rs` (untracked, 423 lines): DTB `/memory` parse, `MNXBOOT1` file table, reuses `build_kernel_info` | present (WIP) |
| Boot bump pool no free; 1024 pages; whole-region deduction | `os/boot-shim/src/main.rs::prepare_boot(1024)` + comments; `os/kernel/src/boot_alloc.rs` has no free API | present |
| kerninfo handover not implemented | `os/kernel/src/kerninfo.rs` doc ("moves to VM"); grep `kerninfo` in `os/servers/vm/src/` = zero | present |
| `MAX_BIG_BLOCKS` family closed | `os/libs/minix-rt/src/alloc.rs` `MAX_BIG_BLOCKS = GLOBAL_POOL_PAGES` | fixed |
| Bump supplier no-coalesce; `release_page` silent drop | `alloc.rs::supply_pages` comment; `release_page` | present |
| PREALLOC records bit only; `AnonymousMemory` no `ev_new` | `os/servers/vm/src/mmap.rs`; `os/servers/vm/src/memtype.rs::impl MemType for AnonymousMemory` | present |
| PAF_CLEAR funnel break | `os/servers/vm/src/cow_exec_pf.rs:291,392` → `alloc_page.rs:236::alloc_pfn_reclaiming` (no flags param); `to_alloc_flags` unconsumed | present |
| VM zero TLB flush; riscv gating landed | grep `tlbi|invlpg|sfence|flush_addr` in `os/servers/vm/src/` = 0; `os/arch/src/riscv64/paging.rs::write_pte_dm` flushes only `KernelDm` | present / partially fixed |
| p_fault_addr consumed only on x86_64 | fill: `trap_dispatch.rs` / `page_fault.rs::set_page_fault_pending`; consume: `os/kernel/src/lib.rs:3753-3760` `invlpg` (x86 asm) | present |
| `quiet_wait` still a poll loop | `os/commands/sbin/init/src/driver.rs:383` `loop { let _ = host.waitpid(-1, 0); }` | present |
| ps_strings 4-vs-8 | `os/arch/src/arch/boot.rs` (`size_of::<i32>()` argc slot, counts always 0) vs `os/libs/minix-sys/src/stack.rs` / shell `exec_frame.rs` (`SLOT = 8`) | present |
| crt0 realignment x86-only | `os/libs/minix-rt/src/crt0.rs`: x86 `and rsp,-16`; aarch64/riscv64 bare call | present |
| Linker constants ×4 worlds | `os/kernel-image/*.ld`; old `os/kernel/src/arch/*/link.ld` (unreferenced by build); `bootface.rs` `KERN_*` consts (comment admits script-guarded); `os/arch/src/arch/direct_map.rs` `KERNEL_DIRECT_MAP_BASE` | present |
| Console: 4 emitters; wrong port; unbounded poll; PL011 fixed | `os/plat/src/*/early_console.rs` (identical `write_hex` ×3); `os/boot-shim/src/lib.rs::emit_byte` polls `0x3f9`; x86 plat unbounded loop; `tx_wait_then_send` shared fn | present |
| fence.i single scatter; aarch64 cvau/iallu absent | `os/kernel/src/arch/riscv64/higher_half.rs:38,54`; grep `cvau|iallu` in arm64 trees = 0 | present |
| Exec reply arm positive errno | `os/servers/pm/src/exec.rs::ExecError::to_errno` returns positive constants; `os/servers/pm/src/ipc/calls.rs` `PmCall::Exec` arm replies `e.to_errno()` un-negated (sibling arms negate) | present (masking half fixed at §1.63) |
| fs-rt `copy_out` erases errors at type level | `os/fs/fs-rt/src/transport.rs:268` returns `()`; internal `let _ =` copy | present |
| clamp_cpu_to_bsp present | `os/kernel/src/sched.rs:307` | present |
| Error-swallow family scale | grep `let _ =` in `os/servers/pm/src` ≈82, `os/servers/vfs/src` ≈123 (incl. benign); §D.13 dispositioned 31 stations / 13 files | present |

R2 pass additions (2026-10-01, same snapshot):

| Claim | Evidence (tool-derived) | Verdict |
|---|---|---|
| `PageSupplier` trait exists as the allocator's designed extension seam (SD-11) | `os/libs/minix-rt/src/alloc.rs:225` `pub trait PageSupplier { fn supply_pages(..) -> Option<*mut u8>; fn release_page(..); }` + module doc; static pool is an impl of it | present |
| FPU eager save/restore built but unwired (SD-23) | `os/arch/src/arm64/fpu.rs::save` (`stp q0,q1 [{ptr}], #32` loop) / `::restore` (`ldp` loop), `FPSIMD_SIZE=528`, 3 unit tests; `KProcess.fpu_state: CurrentFpuState` (`os/kernel/src/proc.rs:1010`); repo-wide grep of `FpuArch::save/restore` callers outside fpu.rs = **0**; promise-comments in `fpu.rs::enable` and the `fpu_state` field doc | orphan primitive (built, unwired) |
| `#[must_use]` effectively unused (SD-19 upgrade basis) | grep `must_use` across `os/servers`, `os/libs/minix-sys/src`, `os/fs/fs-rt/src` = 1 hit (`os/libs/minix-sys/src/time.rs:109`); `IpcTransport` trait methods (`os/libs/minix-sys/src/ipc.rs:503-515`) return `Result` with no added must-use/lint layer | present |
| Kernel delivery copy has a single trait seam (SD-13 delivery leg) | `os/kernel/src/ipc.rs:747` `pub fn delivermsg(proc: &mut KProcess, user_copy: &dyn UserCopy)` | present |
| `commit_message_to_memory` deliberately volatile-read, not fence | `os/libs/minix-sys/src/ipc.rs:567-575` — comment rules out `compiler_fence` (does not force register-resident store to the alloca) | landed design is correct |
| aarch64 context GP area un-wired from trap entry (SD-15 second instance) | `os/arch/src/arm64/boot.rs` `AArch64CpuContext.gp_regs` doc "Updated by trap entry path (future)"; current writers = context construction + signal helpers only | present |
| Typed cross-space channel in kernel use (SD-22 vehicle) | `AddressRef::Process` + `CrossSpaceResult` consumed at `os/kernel/src/syscall_signal.rs:647-667` (incl. `Suspended → KcallResult::VmSuspend`), `os/kernel/src/proc.rs:2364` | present |
| Linker scripts already define the constants as symbols (SD-4 option E basis) | `os/kernel-image/{x86_64,aarch64,riscv64}.ld` assign `KERNEL_VIRT_BASE`/`KERNEL_PHYS_BASE` etc.; no Rust `extern` consumption exists (only asm-side `la t0, __bss_start`, `os/kernel-image/src/main.rs:133`) | single-source seam unused |

## Appendix C — Cross-reference to the original ledgers

| This register | Original |
|---|---|
| SD-1 | riscv-reviewlog §D.1 = 债①; NK4C §1.115 债① |
| SD-2 | §D.2 = 债②; NK4C §1.115 债②; E.1 |
| SD-3 | §D.3 = 债③; NK4B M3.4; M4.4 fact one |
| SD-4 | §D.4 = 债④; NK4B M4.2 decision one; M3.2 residual |
| SD-5 | §D.5 = 债⑤; NK4B M3.3; edge2 NL1 residual |
| SD-6 | §D.14 = 债⑬; NK4C "三件套" member two |
| SD-7 | NK4B M4.4 fact five |
| SD-8 | §D.6 = 债⑥; NK4C §1.55/§1.92–95; misc_concepts 3.15 |
| SD-9 | §D.10 = 债⑨; NK4C §1.101 MUST-1(a) |
| SD-10 | §D.12 = 债⑪; edge1 F2; edge_todo E-VMTLB; NK4C 续-37/41/77a |
| SD-11 | "三件套" member one (§D.6 option B); NK4C §1.54–1.60 |
| SD-12 | edge1 F3; NK4A F3; R3 strict-W^X unit |
| SD-13 | trap-boundary-message-materialization.md; NK4C 续-22…30; P16 |
| SD-14 | NK4B P1; NK4A-QWEN 遗留 1; misc_concepts 3.2 |
| SD-15 | NK4A-QWEN 第六轮 structural risk |
| SD-15b | NK4C 续-40 §F; 续-39/40 commits |
| SD-16 | edge1 F10; B33a (NK4C §1.57/63); edge3 fs_comm fold; B18 adjacent edge; pattern-scan lane tails |
| SD-17 | NK4C 续-34b §D |
| SD-18 | edge4 OQ-1 |
| SD-19 | §D.13 = 债⑫; PATTERN-SCAN §9.2; NK4C §1.106/118/119 |
| SD-20 | §D.11 = 债⑩; NK4C §1.115 CONSIDER-5 |
| SD-21 | NK4C 续-38/39 §E; commits 续-39/40 |
| SD-22 | NK4B M3.x 甲/乙/丙; riscv-reviewlog §D7 |
| SD-23 | NK4C 续-42/43/44 defect E; misc_concepts 5.26 |
| SD-24 | NK4C-MIGRATION §5; 续-55…73; misc_concepts 4.6/3.30 |
| SD-25 | §D.8 = 债⑧; R3 §五.1 |
| SD-26 | NK4C §1.33; B17/B21/B22 |
| SD-27 | edge1/NK4B anchor table; tooling gap |
| SD-28 | edge1 environment traps; OQ-1(CI) methodology note |
| SD-29 | edge4 E5(b)/C-4; edge2 TIMES |
| SD-30 | edge2 NL5③/NL4; NK4A §6 宿主假绿 |
| SD-31 | edge2 LEDGER-AUDIT; edge3 Fix #131 |
| SD-32 | OQ-4; new_todo_qwen P2-5 |

*End of register.*
