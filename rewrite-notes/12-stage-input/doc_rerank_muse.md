# 12-stage-input Document Rebuild Blueprint (muse)

## 0. Metadata

- Executor: muse
- Date: 2026-09-19 (UTC)
- Target directory: `rewrite-notes/12-stage-input/`
- Repository root: `/home/xzhao/github/minix-rs`
- Current commit: `8410fd767e4afa81a5acc43e700ae2743b83fefe`
- Task: Phase R (rebuild blueprint). Output is `target_dir/doc_rerank_muse.md`. No body text modified.
- Constraints honored: no reference to `.design/` or `tmp_design_and_todo/`; the only file written is the one carrying the `_muse` suffix; no other agent's `doc_rerank_*` product was read (the pre-existing `doc_rerank_HY4/deepseek/glm/qwen` files were listed by name only, never opened).
- Output language: English (per muse special requirement).
- Doc-size policy applied: one doc normally carries one independently readable semantic unit and is kept at a comfortable reading length; a single doc of up to ~3000 lines is acceptable only when its concept is genuinely inseparable. Old doc count is not a constraint: this blueprint merges, splits, and creates freely.
- Ground-truth priority applied: Minix3 C source first, then non-C build/boot artifacts, then Rust code, then existing docs last. Existing docs are treated as knowledge sources, not as move targets; several contain stale claims (enumerated in §3), so every reused claim was re-anchored to C or Rust before entering the pool.

### 0.1 Scope

Counted as documents (in scope for rebuild): `00-input-overview.md`, `01` through `14` (`01-input-init-main.md`, `02-chardriver-framework.md`, `03-input-device-structs.md`, `04-input-event-format.md`, `05-input-message-contract.md`, `06-input-open-close.md`, `07-input-read-suspend.md`, `08-input-ioctl-cancel-select.md`, `09-input-event-processing.md`, `10-input-setleds.md`, `11-input-driver-connect.md`, `12-libinputdriver.md`, `13-tty-consumer.md`, `14-pckbd-driver.md`), `99-input-global-concepts.md`. 16 docs, ~4088 lines total.

Counted as reference material (read, not rebuilt): `plan.md` (reorg plan, 483 lines), `todo.md` (arch review TODO, 191 lines, 13 IN-* items all closed), `draft/README.md` (placeholder).

Out of scope: `.design/` and `tmp_design_and_todo/` (never referenced, per project rule); other agents' `doc_rerank_*` files; `../16-stage-drivers/` implementation docs (read-only neighbors); `os/` Rust code (read-only ground truth, second rank).

### 0.2 Reading list (what was actually read)

- All 16 docs: headers (About/Position/Source/Rust-module/Not-covered declarations) fully read; bodies sampled section by section for knowledge extraction (concept introductions, C analysis, Rust decisions, tests, transitions).
- C ground truth: `minix3/minix/servers/input/input.c` (704 lines, all 20 functions + `devs[]` + `input_tab` + 3 macros read in full), `input.h` (45 lines, full), `minix3/minix/lib/libinputdriver/inputdriver.c` (206 lines, full), `minix3/minix/lib/libchardriver/chardriver.c` (600 lines, input-used surface: task/process/announce/reply paths + CDEV table comment), protocol headers `minix/include/minix/input.h` (333 lines, event format + codes), `inputdriver.h` (24 lines), `chardriver.h` (36 lines), `com.h:877-893` + `919-937` + `963` (message + CDEV + BDEV bases), `ipc.h:232-259, 990-1001, 2434-2436, 2517` (four payload structs + union), `dmap.h:78` (INPUT_MAJOR=64), `sys/kbdio.h` + `sys/sys/ttycom.h:174` (KIOCSLEDS), `etc/system.conf:400-403` (service input), `MAKEDEV.sh:330-343` (node table).
- Neighbor C: `drivers/tty/tty/arch/i386/keyboard.c` (`do_input:124-176`, `set_leds:369-384`, `do_fkey_ctl:427-527`, `func_key:532-570`, `debug_fkeys:78,206`), `drivers/tty/tty/tty.c:205-213` (dispatch incl. `TTY_FKEY_CONTROL`), `drivers/hid/pckbd/pckbd.c` (lib-use surface: announce:465-487, kbd/aux event reports:365,394,407, leds:418-431, intr/alarm:434-463, task loop:500-504).
- Rust ground truth: `os/servers/input/src/` all 15 files (connect, dispatcher, effects, error, eventbuf, fkey, handlers, init, lib, main, produce, serve, setleds, structs + line counts in §3), `os/libs/minix-types/src/ipc/input.rs` (569), `input_event.rs` (429), `key_codes.rs` (939), `os/libs/minix-sys/src/inputdriver.rs` (512), `os/libs/minix-chardriver` (`driver.rs` + `protocol.rs`, consumer authority).
- Boundary materials: `rewrite-notes/00-master-plan/README.md` (stage map), `../edge_todo.md` (E-INWIRE, E-CDRCONV, E-TTYEVENT, E-PCKBDREG), `../11-stage-devman/00-devm-overview.md` (previous stage: what RS-load-group/chardriver-shared-framework concepts already exist), `../16-stage-drivers/plan.md` + `06-tty-driver.md` + `13-pckbd-driver.md` (neighbor implementation ownership), `minix3/minix/kernel/table.c` (input absent from boot image, verified by `rg -i input` → no output).
- Commands run (read-only): `ls`, `wc -l`, `sed -n`, `grep -n`, `git log --oneline -- <path>`, `grep -rhoE` for cross-reference census (restricted to non-`doc_rerank` files after one initial over-broad invocation whose `doc_rerank` hits were discarded unread).

### 0.3 Document census (number, title, lines, declared not-covered)

| Doc | Title | Lines | Declared not-covered (header) |
|-----|-------|-------|-------------------------------|
| 00 | input-overview | 107 | all mechanism detail |
| 01 | input-init-main | 334 | framework loop internals (→02); slot contents (→03); post-subscribe (→11); handshake numbers (→05) |
| 02 | chardriver-framework | 301 | handler business logic (→06-11); non-input framework surface; transport |
| 03 | input-device-structs | 306 | event field meanings (→04); op usage of fields (→06-08); ownership changes (→11) |
| 04 | input-event-format | 298 | message enveloping (→05); queue rules (→07/09); scancode translation (→14); localization mapping (→13) |
| 05 | input-message-contract | 380 | char-device requests (→02); per-handler behavior (→06-11); TTY consumption (→13); driver assembly (→12/14) |
| 06 | input-open-close | 220 | read/park (→07); ioctl/cancel/select (→08); minor→slot arithmetic (→03) |
| 07 | input-read-suspend | 279 | wake path (→09); cancel (→08); cross-process copy transport; ioctl/select (→08) |
| 08 | input-ioctl-cancel-select | 268 | LED sending (→10); event wake (→09); park setup (→07); transport (→02) |
| 09 | input-event-processing | 307 | park setup (→07); cancel (→08); TTY consumption (→13); driver assembly (→12); LEDs (→10); connect (→11) |
| 10 | input-setleds | 187 | LED bit translation (→08); send transport; TTY authority rationale (→13); hardware port writes (→14) |
| 11 | input-driver-connect | 355 | driver-side announce (→12); DS internals (→07-stage-ds); event report (→09); LED sending (→10) |
| 12 | libinputdriver | 364 | transport impl; server side (→01-11); hardware detail (→14); DS internals |
| 13 | tty-consumer | 273 | TTY internals (keyboard map, console, line discipline); server side; driver side |
| 14 | pckbd-driver | 276 | port I/O, watchdog, ack protocol, output queue; server side; TTY side; USB drivers |
| 99 | input-global-concepts | 132 | mechanism rationale (points back to 01-14) |

Total ~4088 lines across 16 docs. All headers carry status `rewritten/reviewed`, targets, sources, Rust modules, and reader prerequisites. The header discipline is good and is kept in the rebuild.

### 0.4 C source census (file → owning new doc)

| C file | Lines | New-doc owner |
|--------|-------|---------------|
| `servers/input/input.c` | 704 | 01 (main/startup/init/tab-registration), 03 (map/revmap/macros), 06 (open/close), 07 (copy/read), 08 (ioctl/cancel/select), 09 (process/event), 10 (set_leds), 11 (alloc/connect/disconnect/check/other-DS-half), 05 (other-entry + tab-protocol-face), 02 (other-entry classification context) |
| `servers/input/input.h` | 45 | 03 (struct + constants + DEV indices), 99 (value tables) |
| `lib/libchardriver/chardriver.c` | 600 | 02 (input-used surface only: task/process/announce/reply_task/reply_select/get_minor/do_* + open_devs gate + EDONTREPLY/ERESTART filtering + BDEV_OPEN misdelivery) |
| `lib/libinputdriver/inputdriver.c` | 206 | 12 (all 7 functions + 4 statics) |
| `include/minix/input.h` | 333 | 04 (wire format + pages/values/flags + full code tables), 99 (values) |
| `include/minix/inputdriver.h` | 24 | 12 (callback table + 5 prototypes), 99 |
| `include/minix/chardriver.h` | 36 | 02 (struct + 7 prototypes + cdev_id_t), 99 |
| `include/minix/com.h` (input + CDEV sections) | §§877-893, 919-937, 963 | 05 (numbers), 02 (CDEV bases), 99 |
| `include/minix/ipc.h` (4 structs + union) | :232-259, :990-1001, :2434-2436, 2517 | 05 (layouts), 99 |
| `include/minix/dmap.h:78` | 1 line | 05 (INPUT_MAJOR=64), 99 |
| `sys/kbdio.h` + `sys/sys/ttycom.h:174` | small | 08 (KIOCSLEDS translation), 05 (number construction), 99 |
| `drivers/tty/.../keyboard.c` (do_input/set_leds/do_fkey_ctl/func_key) + `tty.c` dispatch | slices | 13-neighbors (contract face only) |
| `drivers/hid/pckbd/pckbd.c` (lib-use surface) | 507 (sliced) | 13-neighbors (contract face only) |

### 0.5 Non-C artifact census (the eight easily missed items, each answered)

1. Link scripts: none for this stage (user-space server, no linker script; link layout is the generic service link — explicitly stated in the new 01 contract so the checklist is closed, not skipped).
2. Assembly entry / trap entry: none owned (no interrupts in the input server; hardware IRQs belong to pckbd — new 13 states this with `pckbd.c:434-463` anchor).
3. Boot chain / boot protocol: RS runtime load via `etc/system.conf:400-403` (`service input { ipc SYSTEM pm vfs rs ds tty vm; priority 1; }`); absent from `kernel/table.c` boot image (verified `rg -i input` → empty). Owned by new 01.
4. Image layout: no boot-image slot; `/dev` node layout via MAKEDEV (`kbdmux c 64 0`, `mousemux c 64 64`, `kbdN c 64 1+N`, `mouseN c 64 65+N`). Owned by new 05.
5. User-space startup / runtime loading: SEF fresh-boot callback + `sef_startup()` then `chardriver_task`; DS subscribe `drv\.inp\..*` + `chardriver_announce` + `TTY_INPUT_UP`. Owned by new 01.
6. Cross-module interfaces / wire formats: 5 one-way messages + 4 `ipc.h` payload structs + 20-byte `struct input_event`. Owned by new 04/05.
7. Build scripts / toolchain: `servers/input/Makefile`, `lib/libinputdriver/Makefile`, `lib/libchardriver/Makefile` (plain lib builds, no codegen). Owned by new 01 §-appendix (one paragraph + paths; no dedicated doc — explicit deny with reason).
8. Test infra / simulator scripts: `cargo test -p minix-input` (98 passing at last count in docs; re-verify at B phase), `-p minix-types`, `-p minix-sys`; no simulator. Owned by new 99 (test census) + per-doc §5 acceptance.
9. Shutdown/exit path (fixed-list item): no graceful-shutdown path in C (server runs `chardriver_task` forever, `main` returns 0 unreachable); driver death is the only teardown and is DS-mediated (`input_disconnect`). Owned by new 11 (explicit section; C has no `terminate` on the server side — `inputdriver_terminate` is driver-side, owned by 12).
10. Concurrency/sync (fixed-list item): single-threaded event loop, no locks; at most one parked read + one selector per device; `EDONTREPLY` pseudo-reply discipline. Owned by new 02 (with the SMP-non-applicability note: user-space server, patterns 26-29 N/A).

### 0.6 Cross-reference census (breakage-cost raw data; detail in §8)

- Doc-to-doc references inside the stage (excluding `doc_rerank_*`): ~100 hits; hottest targets: 05 (11), 09 (10), 03 (10), 10 (9), 08 (9), 04 (9), 02 (8), 11 (7).
- Code→doc references: `os/servers/input/src/*.rs` module docs point at `01-input-init-main.md`, `02-chardriver-framework.md` (main.rs, init.rs, dispatcher.rs, effects.rs, error.rs — ~6 sites).
- Inbound from other stages: `16-stage-drivers/plan.md`, `16-stage-drivers/06-tty-driver.md`, `16-stage-drivers/13-pckbd-driver.md`, `edge_todo.md` (E-INWIRE/E-CDRCONV/E-TTYEVENT/E-PCKBDREG), `00-master-plan/README.md`, `edge2.md`, `edge3.md`, `09-stage-init/draft/README.md`, `08-stage-is/draft/README.md`.
- Outbound to other stages: `../07-stage-ds/` (DS contract), `../16-stage-drivers/06 + 13` (neighbor implementations), `../edge_todo.md` (E-INWIRE etc.).

---

## 1. C True Order (runtime truth table, rebuilt from source, not paraphrased)

Stage type判定: **service event-loop**. Evidence: `main()` (`input.c:696-704`) does exactly two things — `input_startup()` (SEF callback + `sef_startup()`) then `chardriver_task(&input_tab)` (infinite `sef_receive_status(ANY)` loop, `chardriver.c:549-573`, dispatched by `chardriver_process`, `chardriver.c:455-536`). No linear boot chain past init; everything after init is arrivals. Per §9, this blueprint organizes as: why the service exists → birth + init → message interface → core structs → per-scenario request handling → queries/misc → neighbor protocols. The request lifecycle (one read's park→wake journey) is the main thread through the handler docs.

### 1.1 Startup segment (process birth → serving)

| Step | Action | Anchor | Note |
|------|--------|--------|------|
| S1 | RS loads `input` at runtime (not in boot image) | `etc/system.conf:400-403`; `kernel/table.c` has no input entry (`rg -i input` → empty) | RS-load group, same class as devman/IS/ipc |
| S2 | `main` registers fresh-boot callback, runs SEF startup | `input.c:686-694` (`input_startup`: `sef_setcb_init_fresh(input_init)` + `sef_startup()`); `input.c:696-704` | |
| S3 | `input_init`: zero 10 slots, back-fill minors | `input.c:647-663` loop (`minor=input_revmap(i)`, owner=NONE, tail=count=0, opened/suspended=FALSE, selector=NONE, leds=0) | |
| S4 | `input_init`: subscribe driver arrivals | `input.c:665` (`ds_subscribe("drv\\.inp\\..*", DSF_INITIAL)`, panic on failure) | DSF_INITIAL = initial dump included |
| S5 | `input_init`: announce to VFS side | `input.c:669` (`chardriver_announce()`: publish `drv.chr.<label>`, `CLEAR_IPC_REFS`, clear open_devs; `chardriver.c:99-127`) | |
| S6 | `input_init`: handshake terminal | `input.c:671-677` (`TTY_INPUT_UP` via blocking `ipc_send(TTY_PROC_NR)`; failure only logged) | No reply exists (protocol has no replies, `com.h:886`) |
| S7 | Enter framework loop forever | `input.c:699-701` (`chardriver_task(&input_tab)`); loop `chardriver.c:549-573` | `main` return is unreachable |

### 1.2 Loop segment (one arrival → classify → gate → handler → reply discipline)

| Step | Action | Anchor | Note |
|------|--------|--------|------|
| L1 | Receive any message | `chardriver.c:549-573` (`sef_receive_status(ANY)`) | |
| L2 | Classify: notify / BDEV_OPEN / CDEV request / other | `chardriver.c:455-536` (`chardriver_process`); CDEV field table `chardriver.c:4-32` | BDEV_OPEN answered ENXIO without disturbing the server |
| L3 | Gate: restarted-window check on open_devs set | `chardriver.c:61-85,438-453` (`is_open_dev/set_open_dev/clear_open_devs/do_block_open`) | Non-open traffic to never-opened windows is dropped |
| L4 | Dispatch CDEV_OPEN/CLOSE/READ/IOCTL/CANCEL/SELECT via `input_tab` | `input.c:31-42` (callback table) → `input_open/close/read/ioctl/cancel/select` | |
| L5 | Reply discipline: real reply vs `EDONTREPLY` (park) vs `ERESTART` (never to VFS — filtered) | `chardriver.c:129-174` (reply_task/reply_select), `:195-232` (reply mapping), `:255-270` (cancel replies to the *original* req_id) | |
| L6 | Non-framework traffic → `input_other` | `input.c:608-644`: DS notify → `input_check()`; `INPUT_EVENT` → `input_event()` unconditionally; `INPUT_SETLEDS` only from TTY (`m_source==TTY_PROC_NR`, else fall-through to unexpected log, `:630-636`); default → unexpected log | |

### 1.3 Per-scenario chains (the three threads handlers serve)

**A. Open → read (park) → event → wake → reply.** `input_open` (`:85-105`: map→ENXIO; active→ENXIO; opened→EBUSY; opened=TRUE) → `input_read` (`:162-201`: map→ENXIO; inactive-or-parked→EIO; size/event_bytes==0→EIO; empty+NONBLOCK→EAGAIN; empty→park caller/grant/req_id + EDONTREPLY; else clamp + `input_copy_events`) → `input_copy_events` (`:130-160`: panic if count<asked; two-segment `sys_safecopyto`; advance tail/count only on success; return bytes) → event arrival `input_event` (`:376-428`: id bounds→drop; owner match→else drop; kbd-vs-mouse mux pick by minor range; opened→own queue / mux opened→mux queue / neither→forward `TTY_INPUT_EVENT` via blocking `ipc_send`, failure only logged) → `input_process` (`:332-374`: full→overwrite oldest (tail++, count--); enqueue with devid + rsvd=0; parked→copy exactly 1 + `chardriver_reply_task` + unpark unconditionally; else selector→`chardriver_reply_select` + clear).

**B. Driver life (announce → connect → CONF → events → disconnect).** Driver `inputdriver_announce` (`inputdriver.c:20-39`: own label → publish `drv.inp.<label>` = typemask) → server `input_check` new-half (`:559-587`: `ds_check` loop, `ds_retrieve_u32`, `drv.inp.` prefix filter, `input_connect(owner,label,value)`) → `input_connect` (`:475-531`: DS label re-verify via `ds_retrieve_label_name`, mismatch→ignore; alloc kbd+mouse via `input_alloc_id`; CONF via `asynsend3 AMF_NOREPLY` with rsvd slots = INVALID; kbd success→`input_set_leds(minor, saved leds)` restore) → `input_alloc_id` (`:430-473`: same-label-same-type rejoin wins (owner refresh, return); else first free slot that is unowned AND unopened; else INVALID + log) → events flow (chain A) → label vanishes → `input_check` departed-half (`:588-606`: owned slots re-queried via `ds_retrieve_label_endpt`; ESRCH→`input_disconnect`) → `input_disconnect` (`:533-556`: parked→EIO reply + unpark; selector→select-wake + clear; owner=NONE; queue contents and opened flag deliberately left alone).

**C. LED loop (TTY/ioctl → save + broadcast → driver port → restore).** Producer: TTY `set_leds` (`keyboard.c:369-384`: `INPUT_SETLEDS` via `asynsend3`, NONE-gated) or local `input_ioctl` KIOCSLEDS (`:241-280`: `sys_safecopyfrom` kio_leds_t, KBD_LEDS_*→INPUT_LED_* bit translation, unknown→ENOTTY) → `input_set_leds` (`:204-239`: KBDMUX address = all 4 kbd slots, single address = one slot, mouse range = empty set by loop bounds; save-then-send per slot; send via `asynsend3`, failure only logged) → driver `do_setleds` (`inputdriver.c:119-138`: endpoint check vs saved input_endpt, `idr_leds(mask)` callback) → pckbd port write (`pckbd.c:418-431`) → on reconnect, `input_connect` restores saved mask.

**Driver-side loop (for 12/13):** `inputdriver_task` (`inputdriver.c:188-206`: `sef_receive_status(ANY)` loop) → `inputdriver_process` (`:141-174`: notify first — HARDWARE→idr_intr, CLOCK→idr_alarm, else idr_other; then INPUT_CONF→do_conf (DS label check, save endpt+ids, double-INVALID→disabled log), INPUT_SETLEDS→do_setleds, default→idr_other). `inputdriver_send_event` (`:42-79`: NONE/INVALID gated, blocking `ipc_send`, failure resets input_endpt=NONE — backpressure + liveness probe). `inputdriver_terminate` (`:177-186`).

---

## 2. Knowledge Pool (deduplicated, stage-unique K-ids)

Source-type legend: **S** = already in existing docs (a §6 change entry must give it a destination); **N** = newly admitted from C / non-C artifacts / OS theory (must carry an evidence anchor; exempt from destination rules). Primary telling point = the doc that explains it fully today (or `—` for N items).

### 2.1 Service existence and topology (why input exists)

| ID | Name | Type | Src | Primary location today | Anchor | Reader payoff (answers…) |
|----|------|------|-----|------------------------|--------|--------------------------|
| K-001 | Input service as system event sink (drivers report, service routes) | concept | S | 00 §1.1 | `servers/input/input.c` (whole file: no HW access) | Why is there a separate process between drivers and readers? |
| K-002 | Three boundaries (no HW, no key meaning, no block data) | constraint | S | 00 §1.2 | `input.c` (no port/IRQ includes); TTY map ownership `keyboard.c` | What does input NOT do, and who does it instead? |
| K-003 | RS-load-group membership (not in boot image) | mechanism | S | 01 §1.1 + plan §1.2 | `system.conf:400-403`; `kernel/table.c` (absent) | When does input start, and who starts it? |
| K-004 | Mailroom analogy + default-agent (TTY) rule | concept | S | 00 §1.1, 09 (forward) | `input.c:404-421` | Where do events for nobody go? |

### 2.2 Startup and framework (birth + loop shell)

| ID | Name | Type | Src | Primary location today | Anchor | Reader payoff |
|----|------|------|-----|------------------------|--------|---------------|
| K-005 | SEF fresh-boot registration + startup order | mechanism | S | 01 (init_plan, 4 steps) | `input.c:646-704` | What runs, in what order, before the first message? |
| K-006 | `input_tab` callback table (7 entries) | datastructure | S | 01 (registration face) | `input.c:31-42` | How does the framework know the server's functions? |
| K-007 | chardriver_task receive loop | mechanism | S | 02 | `chardriver.c:549-573` | Where does every arrival enter? |
| K-008 | chardriver_process 4-way classification | mechanism | S | 02 (three questions) | `chardriver.c:455-536` | What happens between arrival and handler? |
| K-009 | open_devs restart gate | mechanism | S | 02 | `chardriver.c:61-85,438-453` | Why is traffic dropped after a restart? |
| K-010 | CDEV request field layout (m10 table) | interface | S | 02 | `chardriver.c:4-32` | How are open/read/ioctl/cancel/select encoded? |
| K-011 | Reply discipline (REPLY/SEL1/SEL2/EDONTREPLY; ERESTART filtered; cancel→original req_id) | mechanism | S | 02 | `chardriver.c:129-174,195-270` | When is "no reply now" legal, and who answers later? |
| K-012 | `chardriver_announce` (publish + CLEAR_IPC_REFS + clear opens) | mechanism | S | 02 (+01 step) | `chardriver.c:99-127` | How do stale callers unblock across a restart? |
| K-013 | Dispatcher `handle_arrival` (one arrival → state + effects) | mechanism | S | 01 §4.5 (misplaced; move to 02) | `os/servers/input/src/dispatcher.rs` (Rust; C counterpart L2-L6 above) | Where is the whole loop decision in one function? |
| K-014 | Effect vocabulary (ReplyTask/ReplySelect/SendDriverAsync/SendTerminalBlocking) | interface | S | 02 §4.5 | `os/servers/input/src/effects.rs`; C send points `input.c:231,419,514-523,672-677` | What can a decision emit, and how is it executed? |
| K-015 | Serve shell (Transport trait, classify/perform/serve, KernelTransport dual carrier) | mechanism | N | — (serve.rs undocumented) | `os/servers/input/src/serve.rs:41-253`; E-INWIRE commits `593c2daa6/794d3eb91/7dfa450c2` | How do decisions become syscalls on the wire? |
| K-016 | Honest-parking policy (no fake serving before transport lands) | constraint | S | 00 §1.3/§4 + main.rs comments | `os/servers/input/src/main.rs` + edge E-INWIRE | How does the project mark unwired seams? |

### 2.3 Device table and numbering

| ID | Name | Type | Src | Primary location today | Anchor | Reader payoff |
|----|------|------|-----|------------------------|--------|---------------|
| K-017 | 10-slot layout (kbdmux, kbd0-3, mousemux, mouse0-3) | datastructure | S | 03 | `input.h:18-26` | Which index is which device? |
| K-018 | Minor↔index mapping (sparse 0,1-4,64,65-68 ↔ dense 0-9) + map/revmap | mechanism | S | 03 | `input.c:44-82` | How is a /dev minor converted to a slot? |
| K-019 | revmap failure = C panic → Rust Option/Result [ARCH] | arch-evolution | S | 03 (A-7) | `input.c:67-82` | What happens on a corrupt reverse lookup? |
| K-020 | `struct input_dev` 13 fields | datastructure | S | 03 | `input.h:29-43` | What state lives per device? |
| K-021 | active/empty/full macros | constraint | S | 03 (defined) / 06/07 (used) | `input.c:24-27` | When is a device "in business"? |
| K-022 | EVENTBUF_SIZE=32 ring + overwrite-oldest | mechanism | S | 03/07/09 | `input.h:7`; `input.c:338-346` | What happens when events arrive faster than reads? |
| K-023 | Parked-read triple (caller/grant/req_id) + selector slot | datastructure | S | 03/07/08 | `input.h:36-40` | Where is a waiting reader remembered? |
| K-024 | leds mask field + cross-restart memory | mechanism | S | 03/10/11 | `input.h:41`; `input.c:228,525-527` | Where does LED state survive a driver death? |

### 2.4 Event vocabulary and wire protocol

| ID | Name | Type | Src | Primary location today | Anchor | Reader payoff |
|----|------|------|-----|------------------------|--------|---------------|
| K-025 | 20-byte `struct input_event` (page/code/value/flags/devid/rsvd[2]) | interface | S | 04 | `include/minix/input.h:25-32` | What is the one language all input speaks? |
| K-026 | Pages GD/KEY/LED/BUTTON/CONS + HID-US framing | concept | S | 04 | `input.h:34-44` (+17-22 comment) | Which page for mouse/key/light/button/media? |
| K-027 | PRESS/RELEASE values + ABS/REL flags | interface | S | 04 | `input.h:46-53` | How are press vs release, absolute vs relative encoded? |
| K-028 | Full code tables (215 key codes + GD/LED/button/consumer) | interface | S | 04 (+key_codes.rs, machine-generated) | `input.h:55-333` | Which numeric code is key A / button 2 / mute? |
| K-029 | minix-types single-authority migration [ARCH: New] | arch-evolution | S | 04 frontmatter | `minix-types/.../input_event.rs`, `key_codes.rs`; IN-P2-2 | Where is the vocabulary's authoritative home? |
| K-030 | 5 one-way messages + no-reply discipline | interface | S | 05 | `com.h:877-893` (+:886 comment) | Which 5 messages exist, and why is none answered? |
| K-031 | 4 payload layouts (CONF/SETLEDS/tty-event/driver-event, 56B + padding) | interface | S | 05 | `ipc.h:232-259,990-1001` | What bytes ride inside each message? |
| K-032 | DEV_KBD/DEV_MOUSE/INVALID_INPUT_ID `_SYSTEM` visibility | constraint | S | 05/99/plan §5.5 | `input.h:6-15` | Who may see device-type constants? |
| K-033 | Reserved lanes (rsvd[2] timestamp-future; CONF rsvd1/rsvd2 = INVALID) | constraint | S | 05/plan §5.5 | `input.h:31`; `ipc.h:235-236`; `input.c:520-521` | What must forward-compatible code do with reserved fields? |
| K-034 | INPUT_MAJOR=64 + MAKEDEV node table | interface | S | 05 | `dmap.h:78`; `MAKEDEV.sh:330-343` | Which /dev node reaches which minor? |
| K-035 | system.conf service-input permissions | interface | S | 05/01 | `system.conf:400-403` | Which IPC targets may input talk to? |
| K-036 | KIOCSLEDS number construction (`_IOW('k',2,kio_leds)`) | interface | S | 05 §2.6 / 08 | `ttycom.h:174`; `kbdio.h` | How is the LED ioctl number built? |

### 2.5 Character-device operations

| ID | Name | Type | Src | Primary location today | Anchor | Reader payoff |
|----|------|------|-----|------------------------|--------|---------------|
| K-037 | open 3 questions (map/active/single-opener) + ENXIO/ENXIO/EBUSY | mechanism | S | 06 | `input.c:85-105` | When is open refused, and with which errno? |
| K-038 | close cleanup + [ARCH: close-cleanup] full-clear fix | arch-evolution | S | 06 §3.2 | `input.c:107-127` vs `handlers.rs:94-103` (+filedes.c:453 evidence) | What does close forget in C, and what does Rust additionally clear? |
| K-039 | read 3-way (give/park/refuse) + EIO/EAGAIN/EDONTREPLY | mechanism | S | 07 | `input.c:162-201` | How does a read answer "give N / sleep / try later"? |
| K-040 | Copy geometry (two-segment safecopy, advance-only-on-success, byte-count reply) + plan/commit split + EventCount/ByteCount [ARCH] | mechanism | S | 07 | `input.c:130-160`; `eventbuf.rs` plan/commit; IN-P1-3/IN-P2-1 | How do wrap-around bytes reach the reader without losing events on failure? |
| K-041 | KIOCSLEDS bit translation (kl_bits→mask, unknown bits ignored) + ENOTTY | mechanism | S | 08 | `input.c:241-280`; `kbdio.h` | How do caller LED bits become the service mask? |
| K-042 | cancel triple-match → EINTR to the *original* read; else silence | mechanism | S | 08 | `input.c:282-301` + `chardriver.c:255-261`; `cancel_parked_read` combo | How is a sleeping read woken as "interrupted"? |
| K-043 | select (RD ready-or-parked-or-empty+NOTIFY→remember; WR always ready as immediate-error) | mechanism | S | 08 | `input.c:303-330` | How does "ask without waiting" treat errors as readiness? |

### 2.6 Event intake and LEDs

| ID | Name | Type | Src | Primary location today | Anchor | Reader payoff |
|----|------|------|-----|------------------------|--------|---------------|
| K-044 | input_event checks (id bounds→drop; owner→drop) + id-is-index [ARCH A-3] | mechanism | S | 09 | `input.c:376-390` | Which reports are silently discarded, and why? |
| K-045 | Mux 3-level springboard (own queue → mux queue → TTY forward) + kbd/mouse pick rule | mechanism | S | 09 | `input.c:392-421` | Where does an event land when its device is closed? |
| K-046 | Wake exactly one (parked→copy-1+reply; else selector-notify) + unconditional unpark + two-phase GrantCopy | mechanism | S | 09 (+07/08) | `input.c:361-369`; `wake_on_event`/`complete_answered_reader`; IN-P1-2 | What exactly happens the moment an event meets a waiting reader? |
| K-047 | TTY forward construction (blocking send, failure logged) | mechanism | S | 09/13 | `input.c:408-421` | How is an unwanted event handed to the terminal? |
| K-048 | input_other entry (notify-first; SETLEDS TTY-only + fall-through log) [ARCH A-10] | mechanism | S | 05/09/10/11 | `input.c:608-644` | Which messages reach which handler, and which fall into the log? |
| K-049 | set_leds broadcast (mux=all-kbd, single=one, mouse=empty) + save-then-send + asynsend-log-only-failure | mechanism | S | 10 | `input.c:204-239` | Who gets a LED command, what is remembered, what if the send fails? |
| K-050 | LED restore on connect | mechanism | S | 10/11 | `input.c:525-527` | How do lights come back after a driver restart? |

### 2.7 Driver lifecycle and driver-side library

| ID | Name | Type | Src | Primary location today | Anchor | Reader payoff |
|----|------|------|-----|------------------------|--------|---------------|
| K-051 | `drv.inp.<label>` publish/subscribe contract + DSF_INITIAL | interface | S | 11/12 | `inputdriver.c:20-39`; `input.c:665`; `ds.h` face | How do server and driver find each other without direct contact? |
| K-052 | input_check new-half (ds_check loop + prefix filter + u32 typemask) | mechanism | S | 11 | `input.c:559-587` | How are arrivals discovered? |
| K-053 | input_check departed-half (ESRCH→disconnect; owner refresh on OK) | mechanism | S | 11 | `input.c:588-606` | How are deaths discovered? |
| K-054 | input_connect (DS label re-verify + alloc + CONF(+INVALID lanes) + LED restore) | mechanism | S | 11 | `input.c:475-531` | What happens, step by step, when a driver registers? |
| K-055 | input_alloc_id (rejoin-wins; free = unowned AND unopened; else INVALID+log) | mechanism | S | 11 | `input.c:430-473` | Which slot does a (re)connecting driver get? |
| K-056 | input_disconnect (parked→EIO; selector→wake; owner=NONE; queue/opened untouched) | mechanism | S | 11 | `input.c:533-556` | What is cleaned, and what is deliberately left, on departure? |
| K-057 | Driver announce + blocking-send event report (backpressure + liveness) | mechanism | S | 12 | `inputdriver.c:20-79` (incl. :65-73 comment) | How does a driver publish itself and report without queue bloat? |
| K-058 | do_conf (DS-label check, save endpt+ids, double-INVALID→disabled) | mechanism | S | 12 | `inputdriver.c:82-117` | How does a driver verify and store its slot assignment? |
| K-059 | do_setleds (endpoint check + idr_leds callback) + callback table (leds/intr/alarm/other, all optional) | interface | S | 12 | `inputdriver.c:119-138`; `inputdriver.h` struct | How do LED orders reach driver code? |
| K-060 | inputdriver_process dispatch (notify-first: HW→intr, CLOCK→alarm; then CONF/SETLEDS/other) + task loop + terminate | mechanism | S | 12 | `inputdriver.c:141-206` | What does the driver main loop do with each arrival? |

### 2.8 Neighbors (TTY + pckbd + FKEY)

| ID | Name | Type | Src | Primary location today | Anchor | Reader payoff |
|----|------|------|-----|------------------------|--------|---------------|
| K-061 | TTY do_input (UP handshake + endpoint save + LED re-emit; EVENT filter KEY-page + NR_SCAN_CODES + RELEASE_BIT + 32-slot ring, drop-new) | mechanism | S | 13 | `keyboard.c:124-176`; `tty.c:205-213` | What does the terminal do with forwarded events? |
| K-062 | TTY set_leds producer (NONE-gated asynsend INPUT_SETLEDS) | mechanism | S | 13 | `keyboard.c:369-384` | When does the terminal originate a LED order? |
| K-063 | pckbd announce-after-probe + kbd/aux translation (prefix state machine; 3-byte mouse packets; change-only report) + leds port writeback + intr/alarm + task | mechanism | S | 14 | `pckbd.c:365,394,407,418-431,434-487,500-504` | How does a real driver use the client library? |
| K-064 | FKEY observer registry (MAP/UNMAP/EVENTS; 12-slot × 2 banks; bit i+1; debug_fkeys gate; func_key interception) | mechanism | N | — (fkey.rs undocumented; 05/13 one-line mentions) | `keyboard.c:72-80,206,224,406-570`; `tty.c:206-208`; `os/servers/input/src/fkey.rs:21-253` (FkeyObserver/FkeyTable/control/key_press/hit_of) | Who watches function keys, and where is that state kept? |
| K-065 | Cross-stage ownership (TTY impl → 16-stage 06; pckbd impl → 16-stage 13; FKEY IS-half → os/servers/is) | constraint | S | 13/14 frontmatter + IN-D4 | `16-stage-drivers/06-tty-driver.md`, `13-pckbd-driver.md`; `os/drivers/hid/pckbd`; edge E-TTYEVENT/E-PCKBDREG | Where does the contract end and the implementation live? |

### 2.9 Statistics

65 knowledge items: 62存量 (S) + 3新增 (N: K-015 serve shell, K-064 FKEY registry, plus the stale-claim corrections folded into K-013/K-016 destinations rather than separate items). By type: concept 5, mechanism 35, datastructure 4, interface 13, constraint 6, arch-evolution 4 (K-019, K-029, K-038, K-040-part). Duplicated-today items (merged in pool, primary point kept): LED logic (08/10/11/12/13/14 → K-041/K-049/K-050/K-059/K-062), CONF (05/11/12 → K-031/K-054/K-058), mux routing (03/09 → K-045), park triple (03/07/08/09 → K-023/K-039/K-046), TTY handshake (01/05/09/13 → K-006-step/S6/K-047/K-061).

---

## 3. Coverage Audit

### 3.1 Topic universe (four roads)

1. **C symbols**: 20 input.c functions + `devs[]` + `input_tab` + 3 macros + `struct input_dev` + all constants; 7 libinputdriver functions + 4 statics + callback struct; chardriver input-used surface (~20 symbols); protocol numbers/layouts; TTY/pckbd contract points (§0.4).
2. **OS-generic concepts**: event-sink topology, single-threaded loop, park/unpark, backpressure vs queueing, save-then-send crash ordering, change-only reporting, wire-compat reservations.
3. **Non-C artifacts**: §0.5 ten items.
4. **Boundary contracts**: DS face (`drv.inp.`), VFS CDEV face, RS loading, TTY/pckbd neighbor faces, edge E-* seams.

### 3.2 Gap table (in universe, told nowhere adequate)

| # | Topic | Evidence anchor | Disposition |
|---|-------|-----------------|-------------|
| G-01 | Serve shell: Transport trait, `classify`, `perform`, `serve`, KernelTransport dual carrier, CDEV_REPLY constants | `os/servers/input/src/serve.rs:37-253`; commits `593c2daa6/794d3eb91/7dfa450c2` | NEW doc 02 half (contract §02-C); K-015 |
| G-02 | FKEY observer registry semantics + TTY contract (MAP/UNMAP/EVENTS, banks, bit numbering, debug gate) | `keyboard.c:72-80,206,224,406-570`; `fkey.rs:21-253` | NEW section in merged doc 13 (contract §13-C) + relocation edge item; K-064 |
| G-03 | Shutdown/exit story (server has none; driver death is the teardown) | `input.c:696-704` (no exit path); `input_disconnect` | Explicit section in new 11 (was implied, never stated); no new doc |
| G-04 | Build/toolchain face (3 Makefiles, no codegen) | `servers/input/Makefile`, `lib/libinputdriver/Makefile`, `lib/libchardriver/Makefile` | Appendix paragraph in new 01 (explicit, then closed) |
| G-05 | Stale claims masquerading as current (00's "mechanical half unwired / main.rs empty park"; 00 nav table's `{framework,effects}.rs`; 02 body `framework.rs/CharacterRequest`; 01's `framework.rs announce_effects`; 99's `framework.rs` authority line) | `main.rs` (now calls `serve::serve`); `framework.rs` deleted (E-CDRCONV `bd06cab21`); 02 only frontmatter-patched | Rewritten in place as part of new 00/01/02/99 (no separate doc; each stale line gets a §6 change row) |

### 3.3 Duplication table (one keeper each, rest become pointers)

| # | Topic spread today | Keeper (new doc) | Rule |
|---|--------------------|------------------|------|
| D-01 | LED: bit translation (08), broadcast+memory (10), restore (11), driver callback (12), TTY produce (13), pckbd consume (14) | New 10 (unified LED story: translate→broadcast→remember→restore→callback, with producer/consumer faces) | 08 keeps 3-line translate-and-hand-off; 11 keeps restore-call line; 12/13/14 keep one-paragraph faces pointing at 10 |
| D-02 | CONF: wire layout (05), server alloc+send (11), driver verify+store (12) | Split by layer (no merge): 05 wire, 11 decision, 12 receipt — each states its layer in §1 | Add "three faces of CONF" box in 05 pointing at 11/12 |
| D-03 | Mux routing: table (03) + decision (09) | 03 owns layout, 09 owns decision; 03 drops the decision paragraph it currently previews | One-line cross-link both ways |
| D-04 | Park triple: struct (03), setup (07), cancel (08), wake (09) | 07 owns lifecycle; 03 owns memory layout; 08/09 own their exits | 07 gains the lifecycle diagram; others reference it |
| D-05 | TTY handshake: step (01), numbers (05), forward (09), consume (13) | 13 owns terminal side; 01 owns send step; 05 owns numbers; 09 owns forward construction | 01/05/09 each ≤5 lines on the handshake, pointer to 13 |
| D-06 | Framework names: `framework.rs`/`CharacterRequest` still in 02 body + 00/01/99 lines | New 02 (minix-chardriver authority + old→new name map) | Single renaming table; all other docs use new names |

### 3.4 Out-of-scope table (told elsewhere or explicitly declined)

| # | Topic some reader will look for | Home |
|---|---------------------------------|------|
| O-01 | chardriver full-framework semantics (non-input faces, block paths) | Shared-lib authority (`minix-chardriver` docs) + 16-stage 01; new 02 keeps input-used surface + pointer |
| O-02 | DS server internals (subscribe/check/retrieve mechanics) | 07-stage-ds; new 11 keeps use-face only |
| O-03 | TTY internals (keymap, console, line discipline, kb_read) | 16-stage 06; new 13 keeps input-related contract only |
| O-04 | pckbd hardware (ports, watchdog, ack protocol, output queue, scan tables) | 16-stage 13; new 13 keeps lib-use face only |
| O-05 | USB HID drivers | No libinputdriver use in tree (`grep -rln inputdriver_ drivers/` → pckbd only); protocol face unchanged; declined with reason in new 13 |
| O-06 | INPUT_DEBUG conditional compilation | Behavior docs do not expand; one line in new 99 |
| O-07 | VFS CDEV transport internals (grant copy mechanics, VFS request origin) | VFS stage + 02 use-face; new 07 states geometry contract only |

### 3.5 Non-C ten-item close-out

Link/load: new 01 (declined-with-reason: generic service link, no stage script). Assembly/traps: new 13 (none owned; IRQs are pckbd's). Boot chain: new 01 (S1-S2). Image layout: new 05 (nodes) + 01 (no boot slot). User-space startup: new 01 (S3-S6). Cross-module wire: new 04/05. Build scripts: new 01 appendix. Test infra: new 99 + per-doc §5. Error paths: every handler doc (errno tables; 99 rolls up). Shutdown/exit: new 11 (G-03). Concurrency: new 02 (single-thread + EDONTREPLY discipline + SMP-N/A note).

---

## 4. New Catalog

15 docs (16 → 15: 13+14 merged; dispatcher/effects/serve consolidated into 02; LEDs unified into 10; FKEY absorbed into 13; no doc exceeds a comfortable reading length — the largest, 02, is budgeted ~700-900 lines for three tightly coupled loop halves that cannot be understood separately).

| New # | File | One-line positioning | Group |
|-------|------|----------------------|-------|
| 00 | `00-input-overview.md` | The map: what input is, the two threads, the reading order | map |
| 01 | `01-input-startup.md` | Birth: RS load → SEF → 4-step init → loop entry (dispatcher moved out) | A birth |
| 02 | `02-loop-framework-effects-transport.md` | The loop: classify → gate → dispatch → effects → transport execution | A birth |
| 03 | `03-device-table.md` | The ten rooms and the two numbering schemes | B vocabulary |
| 04 | `04-event-vocabulary.md` | The one language: 20-byte events + all codes | B vocabulary |
| 05 | `05-wire-protocol.md` | The five letters: numbers, payloads, one-way discipline, nodes | B vocabulary |
| 06 | `06-open-close.md` | Opening: three questions; closing: cleanup + [ARCH: close-cleanup] | C requests |
| 07 | `07-read-park-copy.md` | Reading: give / sleep / refuse + copy geometry + park lifecycle | C requests |
| 08 | `08-ioctl-cancel-select.md` | Side channels: LED translate, triple-match cancel, ask-without-waiting | C requests |
| 09 | `09-event-intake.md` | Intake: checks → routing → enqueue → wake-one → TTY fallback | D events |
| 10 | `10-leds.md` | Lights: translate → broadcast → remember → restore → driver callback (unified) | D events |
| 11 | `11-driver-lifecycle.md` | Guests: arrival discovery → slot allocation → CONF → departure → teardown | E lifecycle |
| 12 | `12-driver-library.md` | The other side's handbook: announce, report, obey, loop | E lifecycle |
| 13 | `13-neighbor-contracts.md` | The two neighbors + FKEY: TTY consume/produce, pckbd translate, FKEY registry | F neighbors |
| 99 | `99-input-constants.md` | The number book: every value, its authority, its meaning-doc | roll-up |

Reading paths: main `01→02→03→04→05→06→07→08→09→10→11→12→13→99`. Fast path (operators): `00→03→05→09→11`. Driver authors: `00→04→05→12→13`. Skippable: `13` halves independently skippable (TTY-only vs pckbd-only readers); `99` is a lookup table, never read linearly. Parallel bodies (LED faces, CONF faces, two neighbors) follow the converge-point rule: framework doc first (02/10/05), then per-role sections with a representative member worked fully (keyboard path) and the mouse/TTY/pckbd differences in tables.

Stage-type handling: service event-loop per §9 — 01 birth+init, 02 message interface + loop, 03-05 structs/vocabulary/protocol, 06-08 per-scenario requests, 09-10 data path + side loop, 11 lifecycle + neighbor protocol, 12-13 the mirrored client + external consumers, 99收口. The request lifecycle (open→park→wake) is the main thread; driver lifecycle and LED loop are the two secondary threads (§1.3 chains A/B/C).

### Order-difference table (runtime truth vs teaching order)

| # | Runtime fact (anchor) | Teaching choice | Reason + back-pointer |
|---|-----------------------|-----------------|-----------------------|
| OD-1 | Handlers need message numbers at runtime, but numbers are taught in 05 after 03/04 | 03→04→05 before 06-08 | Vocabulary before use (K-025-K-036 are prerequisites of every handler); 02 describes classification structurally without decoding payloads, back-pointer "payload meanings → 04/05" |
| OD-2 | `input_other` (09/10/11 traffic) is dispatched by the same loop as CDEV (02), but taught 7 docs later | 02 teaches the entry + routing table only; 09/10/11 teach the targets | Entry-vs-target split (K-048); 02 ends with "three envelopes leave this room → 09/10/11" |
| OD-3 | LED restore happens inside `input_connect` at runtime (`:525-527`), but is taught in 10 before 11 | 10 owns save/broadcast/restore semantics; 11 owns the call site | Concept (memory) before occasion (reconnect); 11 states the call in 2 lines + pointer |
| OD-4 | `TTY_INPUT_UP` is sent during init (S6) but consumed by TTY (13, last) | 01 states the send in 5 lines; 13 owns the handshake | Birth order preserved for the send; neighbor order preserved for the meaning |

Forward-reference scan of the new catalog: every contract's "Prerequisites" below points at strictly earlier numbers (00 позволено everywhere as map; 99 points anywhere as lookup). Dependency graph is acyclic by construction (map → birth → vocabulary → requests → events → lifecycle → neighbors → roll-up).

---

## 5. Per-Document Contracts (all 15; B phase executes these verbatim)

### 00-input-overview.md

- Positioning: the map a first-time reader finishes in 15 minutes; every mechanism question it raises names the doc that answers it.
- Covers: K-001, K-002, K-003 (1 line + pointer), K-004, startup thread (S1-S7 compressed to one diagram), event thread (chain A compressed), lifecycle thread (chain B compressed), LED thread (chain C, 3 lines), catalog table (15 rows: file, one line, Rust home), reading paths (main/fast/driver-author/skippable), four project principles (pure decisions + effects; C order→type discipline; explicit errors; honest parking — each 2 lines + pointer, no re-argument).
- Does NOT cover: any handler errno, any struct field table, any code table, any FKEY detail (→13), any transport code (→02). Explicitly: no mechanism expansion beyond the three thread diagrams.
- Prerequisites: none (entry; one-sentence microkernel + message-passing premise stated up front).
- Referenced by: every doc's "map" link; code module docs keep pointing here for topology.
- Ground truth: `servers/input/input.c` (existence + size), `system.conf:400-403`, `kernel/table.c` (absence), `os/servers/input/src/{main,init,dispatcher,serve}.rs` (presence, names only).
- Knowledge rows: K-001 (why separate process), K-002 (three boundaries), K-003 (RS-load pointer), K-004 (default-agent rule) — each with anchor + why-here (orientation, not mechanism).
- Acceptance: reader can draw the two threads from memory; every §2 table row resolves; zero forward references beyond numbered pointers; stale lines from old 00 (empty-park claim, `{framework,effects}.rs` nav entry) gone.

### 01-input-startup.md

- Positioning: how the process is born and made ready; ends at the loop's door, does not enter it.
- Covers: K-003 (full: system.conf stanza + boot-image absence proof), K-005 (S2-S6 in C order; the 4-step init_plan with per-step dependencies), K-006 (`input_tab` 7 entries as the handoff to 02), S6 handshake send (≤5 lines + pointer to 13), K-035 (permissions, ≤5 lines + pointer to 05), build appendix (3 Makefiles, no codegen — closes G-04), dispatcher-foreshadow (1 paragraph: decisions live in 02, not here).
- Does NOT cover: loop classification/gating/replies (→02); slot contents (→03); post-subscribe discovery (→11); handshake numbers (→05); TTY-side reaction (→13). Dispatcher `handle_arrival` is explicitly NOT here (moved to 02; old 01 §4.5 is deleted, not moved-and-copied).
- Prerequisites: 00.
- Referenced by: 02 (birth context), 11 (subscribe step), 13 (UP send site).
- Ground truth: `input.c:646-704` (init/startup/main), `input.c:31-42` (tab), `system.conf:400-403`, `init.rs` + `main.rs` (order-as-data), 3 Makefiles (paths).
- Knowledge rows: K-003, K-005, K-006 (+S6/K-035/K-004-touch as pointer rows).
- Acceptance: reader can list S1-S7 with anchors; can state why UP failure is logged-not-fatal; build appendix answers "what builds this" with paths; no `handle_arrival` content remains.

### 02-loop-framework-effects-transport.md

- Positioning: everything that happens between "a message arrives" and "effects leave the decision world + get executed"; the loop's three halves (classify/gate/reply; decide; execute) in one place because none is intelligible alone.
- Covers: K-007, K-008, K-009, K-010, K-011, K-012, K-013 (full `handle_arrival`: Arrival variants, gate verdicts, per-handler fan-out, GrantCopy half-stop + `complete_grant_copy`), K-014 (effect vocabulary + per-effect C send-point constructors), K-015 (Transport trait, classify/perform/serve, KernelTransport dual carrier, CDEV_REPLY constants, SEF switch point, E-INWIRE status — closes G-01), K-016 (parking policy, 1 paragraph), K-048-entry-half (routing table to 09/10/11), concurrency note (single-thread, ≤1 park + ≤1 selector per device, SMP-N/A), D-06 renaming table (old `framework.rs/CharacterRequest` → `minix-chardriver driver.rs/protocol.rs CdevRequest`).
- Does NOT cover: handler business rules (→06-11, each ≤3-line summary at fan-out); event payload meanings (→04/05); slot layout (→03); DS internals (→11 use-face pointer). chardriver non-input faces are declined with pointer (O-01).
- Prerequisites: 00, 01.
- Referenced by: 06-11 (gate/reply discipline), 07/09 (GrantCopy insertion point), 11 (announce effects).
- Ground truth: `chardriver.c:4-32,61-85,99-174,195-270,438-573`; `input.c:31-42,608-644` (entry); `minix-chardriver driver.rs/protocol.rs` (authority); `dispatcher.rs`, `effects.rs`, `serve.rs` (Rust halves); `com.h:919-937,963`.
- Knowledge rows: K-007..K-016 + K-048-entry (11 rows).
- Acceptance: reader can trace one CDEV_READ and one INPUT_EVENT from arrival to effect list to executed reply; can state when EDONTREPLY vs real reply; can name the SEF switch point; zero `framework.rs`-as-current references outside the renaming table.

### 03-device-table.md

- Positioning: the ten rooms and the two numbering schemes; the meeting point of the gate (02) and every handler.
- Covers: K-017, K-018 (both directions + sparse-reservation rationale), K-019 ([ARCH] panic→Option/Result), K-020 (13 fields, one-line role each), K-021 (active/empty/full + where used), K-022 (size + overwrite-oldest statement, mechanics →07/09), K-023 (triple + selector memory, lifecycle →07), K-024 (leds field existence, semantics →10).
- Does NOT cover: event field meanings (→04); op behavior (→06-08); ownership changes (→11); mux decision (→09 — the preview paragraph is deleted per D-03).
- Prerequisites: 00, 01, 02 (gate registers minors — the only loop concept needed).
- Referenced by: 04 (element type), 05 (minor/ID distinction), 06-11 (slot access).
- Ground truth: `input.h` (whole, 45 lines), `input.c:22-27,44-82`, `structs.rs` + `error.rs`.
- Knowledge rows: K-017..K-024 (8 rows).
- Acceptance: reader can convert any minor↔index both ways; can state the free-slot rule's two conjuncts; revmap failure policy stated with [ARCH] triple annotation.

### 04-event-vocabulary.md

- Positioning: the one language: what 20 bytes mean + every code's value.
- Covers: K-025 (byte table), K-026 (pages + HID-borrow rationale + US-layout note), K-027 (values/flags), K-028 (full tables incl. 215 key codes + generation note), K-029 ([ARCH: New] migration story + single-authority rule: look up in minix-types, change in minix-types, never re-state).
- Does NOT cover: message enveloping (→05); queue rules (→07/09); scancode translation (→13-pckbd); localized mapping (→13-TTY).
- Prerequisites: 00, 03 (queue-element context, 1 line).
- Referenced by: 05 (payload event fields), 07/09 (what is copied/enqueued), 12/13 (produce/consume faces).
- Ground truth: `include/minix/input.h` (333 lines, all sections incl. `_SYSTEM` note + rsvd), `input_event.rs` + `key_codes.rs`.
- Knowledge rows: K-025..K-029.
- Acceptance: reader can decode a 20-byte event by hand; can state the three responsibility layers (vocabulary→driver translation→reader localization); authority rule stated once, referenced thereafter.

### 05-wire-protocol.md

- Positioning: the five letters: numbers, envelopes, the no-reply law, and the /dev + permission facts that make the letters routable.
- Covers: K-030 (5-number table + com.h:886 law + per-arrow direction diagram), K-031 (4 layouts + 56B + padding + union membership), K-032 (`_SYSTEM` visibility), K-033 (reserved lanes policy), K-034 (node table), K-035 (permissions, full stanza), K-036 (KIOCSLEDS construction), K-048-entry-half (input_other routing table — entry only, targets →09/10/11), "three faces of CONF" box (→11 decision, →12 receipt; D-02), TTY_FKEY_CONTROL one-line existence pointer (→13; FKEY proper NOT here).
- Does NOT cover: char-device requests (→02); handler behavior (→06-11); TTY consumption (→13); driver assembly (→12/13).
- Prerequisites: 00, 03, 04.
- Referenced by: 01 (handshake numbers), 08 (ioctl number), 09/10/11 (message targets), 12 (numbers received), 13 (numbers consumed).
- Ground truth: `com.h:877-893` (+TTY_RQ_BASE), `ipc.h:232-259,990-1001,2434-2436,2517`, `dmap.h:78`, `MAKEDEV.sh:330-343`, `system.conf:400-403`, `ttycom.h:174` + `kbdio.h` (number only; translation →08), `input.rs` + `message.rs`.
- Knowledge rows: K-030..K-036 + K-048-entry.
- Acceptance: reader can name all 5 numbers + directions + payloads; can state why no reply ever exists; can map a /dev path to a minor.

### 06-open-close.md

- Positioning: the first stop after the gate: three admission questions in, cleanup discipline out.
- Covers: K-037 (ordered triple + exact errnos + why double-ENXIO), K-038 (C clears 3, Rust clears 5 — [ARCH: close-cleanup] triple-annotated with filedes.c:453 evidence + "order of clearing" note).
- Does NOT cover: read/park (→07); ioctl/cancel/select (→08); map arithmetic (→03, conclusion cited).
- Prerequisites: 00, 02, 03.
- Referenced by: 07 (opened state), 11 (opened-conjunct in alloc).
- Ground truth: `input.c:85-127`, `handlers.rs` open/close, `filedes.c:453` (no pre-cancel evidence).
- Knowledge rows: K-037, K-038.
- Acceptance: reader can replay open's 3 questions with errnos; can state exactly which 5 fields close clears in Rust vs 3 in C and why the difference is safe.

### 07-read-park-copy.md

- Positioning: the main activity after opening: the 3-way answer + the geometry that moves bytes + the lifecycle of a sleeping reader (the pool's D-04 keeper).
- Covers: K-039 (EIO/EAGAIN/park + NONBLOCK chooser + single-park EIO), K-040 (two-segment safecopy + advance-only-on-success + byte-count + plan/commit split + EventCount/ByteCount [ARCH] + EVENT_BYTES home), K-023-lifecycle (park→resume/cancel/disconnect diagram; K-046/K-042/K-056 exits named, mechanics →09/08/11), GrantCopy insertion point (→02/09 wiring, 1 paragraph).
- Does NOT cover: wake construction (→09); cancel matching (→08); grant-copy transport mechanics (states geometry contract only); ioctl/select (→08).
- Prerequisites: 00, 02, 03, 06.
- Referenced by: 08 (park slip), 09 (wake target), 11 (disconnect exit).
- Ground truth: `input.c:130-201`, `handlers.rs` read section, `eventbuf.rs` (plan/commit/counts).
- Knowledge rows: K-039, K-040, K-023-lifecycle.
- Acceptance: reader can compute reply bytes for a wrap-around copy; can state the failure invariant ("failed copy moves nothing"); can list a parked read's three exits.

### 08-ioctl-cancel-select.md

- Positioning: the three side channels, each with its own contract; the other two exits of a parked read.
- Covers: K-041 (safecopyfrom + bit map + ignore-unknown + ENOTTY + hand-off-to-10 line), K-042 (triple match, EINTR-to-original via CDEV_REPLY(original req_id), mismatch silence + EDONTREPLY legality), K-043 (RD three sub-cases incl. immediate-error + NOTIFY-remember + second-selector-overwrite; WR-always-ready + input-is-read-only rationale).
- Does NOT cover: broadcast mechanics (→10); wake construction (→09); park setup (→07, cited); transport (→02).
- Prerequisites: 00, 02, 05 (KIOCSLEDS number cited), 07.
- Referenced by: 10 (translate face), 09 (selector face), 11 (no direct use — stated to prevent hunting).
- Ground truth: `input.c:241-330`, ` handlers.rs` three sections, `kbdio.h` (bits), `chardriver.c:255-270` (cancel reply path).
- Knowledge rows: K-041, K-042, K-043.
- Acceptance: reader can translate a kl_bits sample; can state cancel's two replies (to canceller: normal; to sleeper: EINTR); can state select's four outcomes incl. "ready-means-error".

### 09-event-intake.md

- Positioning: the heart: what happens from a driver's report to a reader waking (or TTY inheriting).
- Covers: K-044 (bounds + owner + silent-drop policy + frequency rationale; [ARCH A-3] id-is-index), K-045 (minor-range pick rule + 3-level springboard + per-level anchor), K-022-mechanics (enqueue + overwrite-oldest + devid/rsvd fill), K-046 (wake-exactly-one + unconditional unpark + GrantCopy two-phase + selector branch), K-047 (forward construction + blocking-send + log-only-failure).
- Does NOT cover: park setup (→07); cancel (→08); TTY consumption (→13); driver assembly (→12); LEDs (→10); connect/disconnect (→11).
- Prerequisites: 00, 03, 04, 05, 07, 08 (selector cited).
- Referenced by: 10 (event-vs-LED direction split), 11 (events flow after connect), 13 (forward receipt).
- Ground truth: `input.c:332-428`, `produce.rs` (route/enqueue/wake/combos), `eventbuf.rs` (enqueue mechanics).
- Knowledge rows: K-044, K-045, K-022-mech, K-046, K-047.
- Acceptance: reader can route any (id, owner, opened-matrix) input to its outcome; can state the wake-one invariant with the queue-advance timing; can build the TTY forward message.

### 10-leds.md

- Positioning: the unified LED story (the pool's D-01 keeper): one doc owns translate→broadcast→remember→restore→callback end to end; producers and consumers keep faces only.
- Covers: K-041-face (translate cited from 08, 3 lines), K-049 (address→set mapping incl. mouse-empty-by-construction; save-then-send ordering + crash argument; asynsend-log-only + "lights aren't worth crashing" policy), K-024-semantics (memory field role), K-050 (restore call + timing), K-059-face (driver callback cited), K-062-face (TTY producer cited), pckbd-consumer face (cited →13).
- Does NOT cover: ioctl transport (→08); connect mechanics (→11, call site cited); hardware port writes (→13-pckbd); TTY lock-state maintenance (→13-TTY).
- Prerequisites: 00, 03, 04 (LED codes cited), 05 (SETLEDS payload cited), 08.
- Referenced by: 08 (send target), 11 (restore call), 12 (callback meaning), 13 (both faces).
- Ground truth: `input.c:204-239,525-527`, `setleds.rs`, `inputdriver.c:119-138` (receipt face), `keyboard.c:369-384` (produce face), `pckbd.c:418-431` (consume face).
- Knowledge rows: K-049 (full), K-050, + 4 face rows.
- Acceptance: reader can list a SETLEDS order's full journey both directions; can state the save-before-send crash argument; can state why mouse addresses evaporate (loop bounds, not an if).

### 11-driver-lifecycle.md

- Positioning: the guest book: how strangers get rooms, how the desk notices they left, what the bill (CONF) contains, and that there is no checkout ceremony on the server side (G-03).
- Covers: K-051 (prefix + typemask + DSF_INITIAL + symmetric publish face →12), K-052 (check loop mechanics), K-054 (label re-verify strcmp + alloc + CONF-with-INVALID-lanes + LED restore call), K-055 (rejoin-wins + free-conjunct + INVALID+log + "silently disabled" consequence), K-053 (departed-half + owner-refresh + ESRCH-else-log), K-056 (EIO-wake + selector-wake + owner=NONE + deliberately-untouched queue/opened + why), G-03 (no server shutdown path; death-is-teardown statement).
- Does NOT cover: driver-side assembly (→12); DS internals (07-stage-ds pointer); event intake (→09); LED sending (→10, call cited).
- Prerequisites: 00, 03, 05, 10 (restore cited).
- Referenced by: 01 (subscribe step), 12 (mirror), 13-pckbd (announce face).
- Ground truth: `input.c:430-606`, `connect.rs` (alloc/report/departure/labels_match), `ds.h` face, `minix-sys ds.rs` (client verbs cited, not taught).
- Knowledge rows: K-051..K-056 + G-03.
- Acceptance: reader can replay connect with a label-mismatch and a full-house outcome; can state what disconnect preserves and why readers-not-queues are woken.

### 12-driver-library.md

- Positioning: the handbook for the other side's author: four acts (announce, report, obey, loop) + the paper slip (callback table).
- Covers: K-057 (label publish + NONE/INVALID gates + blocking-send backpressure + liveness-reset + the :65-73 comment read in full), K-058 (endpoint-via-DS check + save + disabled-log), K-059 (endpoint check + callback + table with 4 optional hooks + minimal-parameter principle + log-policy-by-frequency note), K-060 (notify-first dispatch + task loop + terminate + EINTR-break), `drv.inp.` symmetric face (→11).
- Does NOT cover: transport implementation (future layer; contracts only); server side (→01-11, contrasted); hardware detail (→13-pckbd); DS internals.
- Prerequisites: 00, 05, 11.
- Referenced by: 13-pckbd (every call mapped), 11 (mirror).
- Ground truth: `inputdriver.c` (all 206 lines), `inputdriver.h` (all 24), `inputdriver.rs` (verdicts/checks/classification; transport cited as future).
- Knowledge rows: K-057..K-060 + K-051-face.
- Acceptance: driver author can write announce→report→obey→loop from this doc alone; can state the two reasons for blocking send; can state why CONF checks labels but SETLEDS checks endpoints.

### 13-neighbor-contracts.md

- Positioning: the two adjacent rooms + the borrowed registry: what TTY does with our envelopes, what pckbd puts in them, and where the F-key watchers live — all contract faces, all implementations elsewhere (two independently skippable halves + FKEY appendix).
- Covers (TTY half): K-061 (UP verify-via-DS + endpoint save + LED re-emit; EVENT page-filter + NR_SCAN_CODES bound + RELEASE_BIT + 32-ring drop-new + drop-new-vs-overwrite-oldest contrast box with 09), K-062 (NONE-gated asynsend produce), tty.c dispatch map (incl. FKEY_CONTROL row → appendix). (pckbd half): K-063 (announce-after-probe + flags condition; kbd prefix state machine + table lookup + swallow-on-nopage + bit7 press/release; mouse 3-byte packets + change-only + zero-displacement silence; leds writeback; intr/alarm; task; USB-declined O-05 with grep evidence). (FKEY appendix): K-064 (MAP/UNMAP/EVENTS + banks + bit-i+1 + debug gate + func_key interception + IS-half pointer + fkey.rs placement note + relocation edge item). K-065 (ownership map for all three).
- Does NOT cover: TTY keymap/console/line-discipline (→16-stage 06); pckbd ports/watchdog/ack/queue/scan-tables (→16-stage 13); server side (→01-11, contrasted).
- Prerequisites: 00, 04, 05, 09 (forward cited), 10 (LED cited), 12 (library cited).
- Referenced by: 01 (UP receipt), 09 (forward target), 10 (producer/consumer faces).
- Ground truth: `keyboard.c:30-80,124-176,206,224,294,369-408,427-570`; `tty.c:205-213`; `keymap.h:NR_SCAN_CODES`; `pckbd.c` lib-use slices + `pckbd.h`/`table.c` (existence); `fkey.rs` (current Rust home, placement-flagged).
- Knowledge rows: K-061, K-062, K-063, K-064, K-065.
- Acceptance: TTY-only reader can implement do_input+set_leds handling; pckbd-only reader can implement the lib-use face; FKEY reader can state bank/bit/gate rules + where each half lives; O-05 declined with evidence.

### 99-input-constants.md

- Positioning: the number book, not a second source of truth: every value with its authority file and its meaning-doc; the test census; the lookup tables.
- Covers: message numbers, event format + code values (pointer to 04 tables, values repeated with authority tags), device numbers (minors + indices), errno roll-up (ENXIO/EBUSY/EIO/EAGAIN/EINTR/ENOTTY/EINVAL + EDONTREPLY pseudo + panic→error [ARCH] notes), endpoint/DS-label conventions, global state (devs array), cross-service map (VFS/DS/RS/TTY/pckbd + edge E-* statuses), CDEV bases + CDEV_REPLY_BASE erratum note (E-CDRCONV, do-not-copy value), INPUT_DEBUG one-liner (O-06), test census method (per-doc counts + `rg #\[test\]` re-verify instruction; stale IN-D1 class of drift explicitly guarded by dating every count).
- Does NOT cover: any mechanism rationale (points back to 01-13).
- Prerequisites: none (lookup; used alongside any doc).
- Referenced by: nobody normatively (lookup only) — enforced to prevent second-truth drift.
- Ground truth: `com.h`, `input.h` (both), `inputdriver.h`, `chardriver.h`, `dmap.h:78`, `kbdio.h`, `ttycom.h:174`; `minix-types` authorities; `cargo test` censuses at B-phase time.
- Knowledge rows: roll-up rows for K-030/K-025/K-017/errnos, each with authority + meaning-doc columns.
- Acceptance: every number in 01-13 appears here with matching value; every row names an authority file; test counts carry dates; erratum values are flagged not copied.

---

## 6. Change Table (old → new; every stock item has a destination)

| Op# | Type | Old location | New location | Reason + knowledge IDs |
|-----|------|--------------|--------------|------------------------|
| X-01 | move | 01 §4.5 dispatcher (`handle_arrival`, Server, GrantCopy) | 02 §C (loop decision core) | Dispatcher is loop, not birth (K-013); 01 keeps 1-paragraph foreshadow |
| X-02 | move+new | (nothing: serve.rs undocumented) + 00 "empty park" lines + 01 E-INWIRE notes | 02 §D (serve shell: Transport/classify/perform/serve/KernelTransport) | G-01/K-015; updates the stale "unwired" story to landed-with-seams |
| X-03 | rewrite-in-place | 02 body (`framework.rs/CharacterRequest` as current) | 02 (minix-chardriver authority + D-06 rename table) | G-05/D-06; frontmatter patch already exists, body follows |
| X-04 | rewrite-in-place | 00 ("mechanical half unwired", `{framework,effects}.rs` nav row, empty-loop line) | 00 (landed-with-seams wording; `{dispatcher,effects,serve}` + shared-lib rows) | G-05/K-016; 00 stays a map, gains no mechanics |
| X-05 | rewrite-in-place | 99 (`framework.rs` authority line) | 99 (shared-lib authority + erratum note kept) | G-05/D-06 |
| X-06 | merge | 13-tty-consumer + 14-pckbd-driver | 13-neighbor-contracts (TTY half + pckbd half + FKEY appendix) | Two thin parallel contracts (~275 lines each) become one 600-750-line doc with shared wire context; K-061..K-065 |
| X-07 | absorb | FKEY one-liners in 05/13 + `fkey.rs` (undocumented) | 13 appendix (FKEY registry contract + placement note + relocation edge) | G-02/K-064 |
| X-08 | unify | LED paragraphs in 08/11/12/13/14 | 10-leds (single owner); donors keep faces (D-01) | D-01; K-049/K-050 centered |
| X-09 | split-clarify | CONF triple-told in 05/11/12 | Unchanged homes + "three faces" box in 05 (D-02) | D-02; layer confusion removed without moves |
| X-10 | trim | 03 mux-decision preview; 01 handshake-number detail; 09 TTY-consume detail | Deleted at source (destination = keeper doc per D-03/D-05) | D-03/D-05; each deletion listed in §8 migration rows |
| X-11 | new-section | (nothing: shutdown story) | 11 §exit (G-03: no server exit; death-is-teardown) | G-03 |
| X-12 | new-appendix | (nothing: build face) | 01 appendix (3 Makefiles; G-04) | G-04 |
| X-13 | renumber | 01..14,99 file names | 01..13,99 (`01-input-startup.md`, `02-loop-framework-effects-transport.md`, `03-device-table.md`, `04-event-vocabulary.md`, `05-wire-protocol.md`, `06-open-close.md`, `07-read-park-copy.md`, `08-ioctl-cancel-select.md`, `09-event-intake.md`, `10-leds.md`, `11-driver-lifecycle.md`, `12-driver-library.md`, `13-neighbor-contracts.md`, `99-input-constants.md`) | Old numbers collide after merge (old 13/14 → new 13); clean renumber with full migration table (§8) beats permanent "13-means-two-things" ambiguity |
| X-14 | archive | All 16 old docs (whole files) | Archive, not delete (B phase moves to archive prefix; §8 per-section destinations) | Rebuild-not-patch premise: new docs are written from pool+C, not by editing old prose |

Explicit deletions (nothing silently dropped): only stale-claim lines (G-05 enumerated strings) and preview-paragraph trims (D-03/D-05) are deleted; every knowledge item K-001..K-065 has a §5 home (G5 check in §9).

---

## 7. Missing-New-Sections (fixed checklist, no blanks, no "TBD")

- Link/load: new 01 appendix — generic service link, no stage script, paths given. Decided: no dedicated doc (reason: zero stage-specific content).
- Image/memory layout: new 05 — MAKEDEV table + INPUT_MAJOR; boot-image absence in 01. Closed.
- Assembly/trap entry: new 13 — none owned; pckbd IRQ ownership stated with anchor. Closed by denial with evidence.
- Boot assembly: new 01 S1-S2 — RS stanza + SEF. Closed.
- Build/toolchain: new 01 appendix — 3 Makefiles, no codegen. Closed.
- Cross-module wire: new 04/05 — full. Closed.
- Error paths: per-doc errno tables + 99 roll-up. Closed.
- Shutdown/exit: new 11 §exit — server runs forever; teardown = DS-mediated disconnect. Closed (G-03).
- Concurrency/sync: new 02 — single-thread + park/selector bounds + EDONTREPLY + SMP-N/A. Closed.
- Test infra: new 99 census + per-doc §5; `cargo test -p minix-input/-p minix-types/-p minix-sys` re-run at B phase (old counts 98/220/149 are dated, not trusted). Closed.

---

## 8. Anchor Migration and Breakage Cost

### 8.1 Section-level migration (old section → new home + type)

| Old position | One-line content | New home | Type |
|--------------|------------------|----------|------|
| 00 §1 thread diagrams | startup/event/lifecycle panoramas | 00 (redrawn against landed transport) | rewrite |
| 00 §2 nav table (16 rows) | per-doc one-liners + Rust homes | 00 (15 rows; 02/13/99 rows rewritten) | rewrite |
| 00 stale lines (§1.3 "honest parking/empty loop", nav `{framework,effects}.rs`) | unwired-era story | 00 (landed-with-seams wording) | rewrite (G-05) |
| 01 §§1-3 (birth, RS load, init steps) | startup narrative + init_plan | 01 (kept, re-anchored; +build appendix) | move-verbatim-then-polish |
| 01 §4.5 dispatcher | handle_arrival/Server/GrantCopy | 02 §C | move (X-01) |
| 01 handshake-number asides | TTY_UP details | 05 (numbers) + 13 (meaning); 01 keeps 5 lines | split (D-05) |
| 02 §§1-2 (lobby, 3 questions) | classification/gate/reply teaching | 02 (kept; names switched to shared-lib terms + D-06 table) | rewrite-names |
| 02 §4.5 effects | effect vocabulary | 02 (kept; serve-execution half appended, X-02) | merge-expand |
| 02 `framework.rs` body refs | Rust names as current | 02 rename table + new names everywhere | rewrite (G-05) |
| 03 (all) | slots/numbers/map/fields/macros | 03 (kept; mux-decision preview deleted →09) | move + trim (D-03) |
| 04 (all) | format/pages/codes/migration | 04 (kept; authority rule hardened) | move-verbatim-then-polish |
| 05 (all) | numbers/layouts/one-way/nodes/perms | 05 (kept; +CONF-3-faces box + FKEY pointer) | move + append |
| 06 (all) | open triple + close + [ARCH: close-cleanup] | 06 (kept; ARCH triple re-verified) | move-verbatim-then-polish |
| 07 (all) | 3-way + geometry + plan/commit + counts | 07 (kept; +park-lifecycle diagram D-04) | move + append |
| 08 LED-translate § | kl_bits→mask | 08 (kept 3 lines) + 10 (full journey) | face/owner split (D-01) |
| 08 cancel/select §§ | triple-match + select | 08 (kept) | move-verbatim-then-polish |
| 09 (all) | checks/route/enqueue/wake/forward | 09 (kept; TTY-consume detail trimmed →13) | move + trim (D-05) |
| 10 (all) | broadcast/memory/restore | 10 (kept; +translate/producer/consumer faces absorbed) | merge-expand (D-01) |
| 11 (all) | check/alloc/connect/disconnect/DS face | 11 (kept; +§exit G-03; restore-call shortened →10) | move + append |
| 12 (all) | announce/send/conf/setleds/process/task | 12 (kept; CONF-face box pointer →05) | move-verbatim-then-polish |
| 13 (all) | do_input/set_leds/dispatch-row | 13 TTY half (kept; +drop-contrast box +FKEY appendix) | merge (X-06) |
| 14 (all) | announce/translate/leds/intr/task faces | 13 pckbd half (kept; +O-05 denial) | merge (X-06) |
| 99 tables | numbers/codes/errnos/refs | 99 (kept; framework authority row rewritten; +test-census method) | move + rewrite-row (G-05) |

### 8.2 Reference migration

Every in-stage doc-to-doc link is rewritten to new numbers at B-phase time (mechanical: old `NN-*.md` filenames change for 01,02,03,04,05,06,07,08,09,10,11,12,13 and old 14 disappears into new 13 — hence full renumber X-13 rather than partial). Code-comment references (`main.rs`→01, `init.rs`→01, `dispatcher.rs`→01/02, `effects.rs`→02, `error.rs`→02) are updated to new 01/02 filenames with a single `rg` pass. Inbound references (16-stage plan/06/13, edge_todo E-*, 00-master-plan README, edge2/edge3) are notified, not edited here: the migration table above is the handoff (old 13/14 → new 13 is the only cross-stage-visible semantic move; everything else is same-number-or-appendix).

### 8.3 Breakage-cost summary

- Affected in-stage references: ~100 doc-to-doc hits + ~6 code-comment sites + nav tables in 00/99 + plan.md §2/§5/§6 tables (plan.md itself is reference material; B phase files a plan-addendum, does not rewrite history).
- Hot files: 05 (11 inbound), 09 (10), 03 (10) — these three keep their numbers AND their core topics, so most inbound links survive the renumber with filename-only changes.
- Bulk method: `rg -l '1[0-4]-[a-z-]+\.md|02-chardriver-framework|13-tty-consumer|14-pckbd-driver' notes/ os/` → apply §8.1 filename map → re-run forward-reference scan (§9 G3). Code comments: 6 sites, hand-edited with ±5-line reads per fix-guard.
- Riskiest move: X-13 renumber (every old filename changes). Mitigation: single bulk pass + G3/G8 re-verification; old files remain in archive under old names so stale external links (book/, 16-stage docs) degrade to "archived, see migration table" rather than 404.

---

## 9. Verification and Self-Gates

G1 (true-order checkable): 22 S/L/A/B/C rows each carry file+line anchors (§1). Spot-check 10: S1 (system.conf:400-403 ✓, table.c absence ✓), S3 (`input.c:652-662` ✓), L2 (`chardriver.c:455-536` ✓), L6 (`input.c:608-644` ✓), A-park (`input.c:187-194` ✓), A-copy (`input.c:144-154` ✓), A-forward (`input.c:408-421` ✓), B-alloc (`input.c:441-458` ✓), C-restore (`input.c:525-527` ✓), driver-send (`inputdriver.c:65-79` ✓). PASS.

G2 (pool completeness): every C file in §0.4 has an owning new doc; every non-C item in §0.5 is closed in §7; Rust files: 15/15 placed (connect→11, dispatcher→02, effects→02, error→03-face/99, eventbuf→07/09, fkey→13-appendix, handlers→06/07/08, init→01, lib→01-face, main→01/02, produce→09, serve→02, setleds→10, structs→03); minix-types 3 + minix-sys 1 placed (04/05/12). Explicit excludes O-01..O-07 with reasons. PASS.

G3 (zero forward reference): contracts' Prerequisites are earlier-only (verified by inspection while writing §5; B phase re-scans mechanically). 99 exempt as lookup. PASS by construction, re-verify at B.

G4 (acyclic): chain map→birth→vocabulary→requests→events→lifecycle→neighbors→roll-up has no back edge; OD-table back-pointers are citations, not prerequisites. PASS.

G5 (100% coverage): all 65 K-items have §5 homes (§6 X-table + §8 migration); N-items carry anchors; deletions enumerated (G-05 stale lines, D-03/D-05 previews — no knowledge lost, keepers named). PASS.

G6 (split/merge destinations): X-01/X-02/X-06/X-08 splits/merges each name per-section destinations (§8.1, 30 rows). Spot-check 10 rows against C anchors — done in §8. PASS.

G7 (7-element contracts): all 15 contracts carry positioning/covers/not-covers/prerequisites/referenced-by/ground-truth/rows/acceptance. PASS.

G8 (migration coverage): §8.1 covers all 16 old docs section by section; §8.2 covers doc-doc + code comments + inbound handoff. PASS.

G9 (anchored assertions): every C/Rust factual claim above carries a path+line anchor; judgments without anchors are labeled (e.g., "budgeted ~700-900 lines", "15 minutes", "mechanical B-phase re-scan" are estimates/plans, marked as such by context). Spot-check 10 anchors — same 10 as G1. PASS.

Conclusion: blueprint COMPLETE and directly executable by B phase. Open questions for the reader (no TBDs in the build itself): (1) confirm merged-13 vs split-13/14 taste — blueprint recommends merged with skippable halves; if rejected, split new 13 back into 13-TTY + 14-pckbd and move the FKEY appendix with the TTY half; (2) confirm X-13 full renumber vs minimal rename (keep old filenames, accept stale semantics in names) — blueprint recommends clean renumber with migration table; (3) fkey.rs relocation (move to TTY crate vs document-in-place) is an edge item for the 16-stage owner, not decided here.
