# 11-stage-devman Document Rebuild Blueprint (muse)

```text
your_name(AI agent name) = muse
target_dir(关注的工作目录) = 11-stage-devman
repo_root(仓库根目录) = /home/xzhao/github/minix-rs

任务 = R 相·重建蓝图：输出 target_dir/doc_rerank_<your_name>.md，不改任何正文。
       只读 C 代码不够，还必须读：本目录全部文档的头部声明、os/ 目录对应入口、
       00-*-overview.md 的导航表。
约束 = 不得引用 .design/ 与 tmp_design_and_todo/；任何落盘产物必须带
       _<your_name> 后缀；不得读取或照抄其它 AI 的 doc_rerank_* 产物。
```

> This blueprint is a self-contained, executable rebuild specification for
> `notes/rewrite/fork-syscall-rewrite/11-stage-devman/`. Phase B can take each
> new chapter's contract (§5) and write the body without further triage.
> All factual claims carry anchors. Judgments without anchors are explicitly
> marked as speculation or to-be-verified.
> The executor did not read any other AI's `doc_rerank_*` product; all
> conclusions below were derived independently from C sources, Rust code,
> and the existing documents' header declarations.
> Output language note: per the task's special requirement for the executor
> named "muse", this report is written in English.

---

## 0. Metadata

- Executor: muse. Date (UTC): 2026-09-19. Target directory:
  `notes/rewrite/fork-syscall-rewrite/11-stage-devman/`.
  Repository root: `/home/xzhao/github/minix-rs`.
  Current commit: `046edcf86` (`git rev-parse --short HEAD`, verified 2026-09-19).
- Deliverable: this file only —
  `notes/rewrite/fork-syscall-rewrite/11-stage-devman/doc_rerank_muse.md`.
  No body text of any existing document was modified, renamed, moved, or deleted.
- Scope decision (what counts as a document, what is reference material,
  what is out of scope):
  - In-scope numbered documents (15): `00-devm-overview.md` (184 lines),
    `01-devm-init-main.md` (410), `02-vtreefs-framework.md` (279),
    `03-devm-structs.md` (188), `04-device-tree.md` (211),
    `05-devm-message-contract.md` (198), `06-event-buf.md` (186),
    `07-devm-add-device.md` (171), `08-devm-del-device.md` (166),
    `09-devm-bind-unbind.md` (146), `10-libdevman-client.md` (139),
    `11-usb-device-model.md` (136), `12-rs-integration.md` (159),
    `13-devmand-consumer.md` (149), `99-devm-global-concepts.md` (92).
    (Line counts via `wc -l`, verified 2026-09-19.)
  - Reference material (not chapters, never renumbered into the catalog):
    `plan.md` (482 lines, finalized 2026-08-16, coverage contract + ARCH list),
    `todo.md` (203 lines, DM-P1-1..P3-3 closed 2026-09-15),
    `README.md` (63 lines, index + test baseline),
    `draft/README.md` (old placeholder, source material only).
  - Out of scope: `.design/` and `tmp_design_and_todo/` (never cited, per
    project rule); other AIs' `doc_rerank_*` files (not read);
    `16-stage-drivers/`, `18-stage-commands/`, `15-stage-fs/` documents that
    merely reference devman (recorded as external consumers in §8, not rebuilt).
- Reading list (all actually read by this executor):
  - All 15 documents' header declarations (positioning, sources, Rust module,
    prerequisites, non-coverage hand-off) — full text skimmed, bodies sampled
    for the knowledge pool (§2). Header of every document states positioning,
    prerequisites, and hand-off explicitly; this blueprint trusts those headers
    as the inventory of what each document claims to carry.
  - C ground truth: `minix3/minix/servers/devman/main.c` (93 lines, full),
    `bind.c` (105, full), `buf.c` (129, full), `device.c` (520, full),
    `devman.h` (108, full), `devinfo.h` (35, full), `proto.h` (23, full);
    `minix3/minix/lib/libdevman/generic.c` (275, full), `usb.c` (301, full),
    `local.h` (27, full); `minix3/minix/include/minix/devman.h` (public wire
    mirror, full); `minix3/minix/include/minix/com.h:846-866` (DEVMAN_BASE
    0x1200 + 10 message constants + 5 field macros);
    `minix3/minix/lib/libvtreefs/` file list (14 files) + `vtreefs.h`
    (hook table) + `vtreefs.c:67-88` (`fs_other`, `run_vtreefs`) +
    `mount.c:24-25` (`init_hook` call) + `table.c:6-24` (dispatch table) +
    `file.c:62-79` (`read_hook` call);
    `minix3/minix/servers/rs/manager.c:840-851` (publish handshake),
    `:897-909` (unpublish handshake), `:1742` (`init_slot` inheritance);
    `minix3/etc/system.conf:422-429` (`service devman`);
    `minix3/minix/commands/devmand/main.c` (event parsing `:803-872`, main
    loop `:876-932`, `determine_type` `:519-587`, scripts `:84-154`,
    pid file `:424-431`) + `proto.h` + `usb.y`/`usb_scan.l`/`usb_driver.h`
    (existence verified via `ls`); `minix3/etc/devmand/` (`usb_hub.cfg`,
    `usb_storage.cfg`, `scripts/`, existence verified).
  - Rust implementation entries: `os/servers/devman/src/` (15 files:
    `main.rs`, `hooks.rs`, `server.rs`, `structs.rs`, `wire.rs`,
    `device_tree.rs`, `buf.rs`, `event_queue.rs`, `add_device.rs`,
    `del_device.rs`, `bind.rs`, `rs_contract.rs`, `ipc/`, `vtreefs/`,
    `lib.rs`); `os/libs/minix-sys/src/devman_client.rs` (client codec +
    transport) and `usb_model.rs` (USB modeling); `os/libs/minix-types/src/
    types/com.rs` (DEVMAN constants block).
  - Boundary material:
    `notes/rewrite/fork-syscall-rewrite/00-master-plan/README.md` (stage
    table: devman = RS-loaded runtime service, not in boot_image; causal chain
    kernel → VM → RS → rest); `edge_todo.md` devman entries (E-DMWIRE,
    E-DMCLIENT closed, E-REQWIRE/E-ISWIRE/E-DSWIRE increments, E5(h) lifecycle
    acceptance); `10-stage-mib/00-mib-overview.md` (previous stage overview
    existence confirmed; MIB is boot_image-registered, devman is not — the two
    stages share the RS/DS/VFS neighbor set but no overlapping concepts, so
    this stage's chapter 1 starts from "RS loads devman" with no repeated
    exposition); `01-stage-kernel/06-todo.md` (contract-writing style example
    only — §§1-2, 6-10 skimmed; no content borrowed).
- Commands used and key evidence excerpts:
  - `git rev-parse --short HEAD` → `046edcf86`.
  - `wc -l notes/rewrite/fork-syscall-rewrite/11-stage-devman/*.md` →
    per-document line counts cited above; `plan.md` 482, `todo.md` 203.
  - `ls minix3/minix/servers/devman/ minix3/minix/lib/libdevman/
    minix3/minix/commands/devmand/` → file inventories cited above.
  - `wc -l minix3/minix/servers/devman/*.c *.h
    minix3/minix/lib/libdevman/*.c *.h` → 1616 lines total (server 4 .c = 847;
    libdevman 2 .c = 576).
  - `sed -n '840,870p' minix3/minix/include/minix/com.h` → DEVMAN_BASE 0x1200,
    ADD_DEV+0 … UNBIND+9, five field macros (GRANT_ID/GRANT_SIZE on m4_l1/l2,
    ENDPOINT/DEVICE_ID/RESULT aliased).
  - `rg -c "doc 0[0-9]|doc 1[0-3]|doc 99|devm-" os/servers/devman/src/
    os/libs/minix-sys/src/devman_client.rs
    os/libs/minix-sys/src/usb_model.rs
    os/libs/minix-types/src/types/com.rs` → 22 files carry doc-code anchors
    (one `//!` header each; hooks.rs and vtreefs/inode.rs carry 2 each).
  - `rg -l "devm-|11-stage-devman" notes/rewrite/fork-syscall-rewrite/
    --glob '!*11-stage-devman*'` → external referrers: `edge_todo.md`,
    `edge3.md`, `00-master-plan/README.md`, `08-stage-is/draft/README.md`,
    `09-stage-init/draft/README.md`, `12-stage-input/plan.md`,
    `15-stage-fs/plan.md`, `16-stage-drivers/plan.md`,
    `16-stage-drivers/12-gpio-devman.md`, `18-stage-commands/plan.md`.
  - Intra-stage cross-reference scan (`rg -o "0[0-9]-devm-…|…"` over the 15
    bodies, excluding `doc_rerank_*`) → dense mesh: every chapter cites 2–6
    siblings; heaviest hubs are 05 (message surface), 03 (shapes), 06 (events).
    Full per-section migration is tabulated in §8.

---

## 1. C True Order (runtime sequence reconstructed from C, not from existing docs)

### 1.1 Stage-type determination

11-stage-devman is a **service event-loop stage** (per the R-prompt §9:
"storage management, process management, file systems, registry services …").
It is therefore documented in two segments: a **birth/init segment** (how the
process comes alive and enters the loop) followed by a **loop segment**
(receive → dispatch → handle → reply). It is not a linear boot chain (the
server never exits into a next stage) and not a pure collection (the four
messages share one dispatch surface and one tree, so a unifying framework
chapter plus lifecycle grouping applies).

### 1.2 True-order table

| Step | Action | C anchor | Note |
|------|--------|----------|------|
| T-0 | RS loads devman at runtime (not in boot_image) | `minix3/minix/kernel/table.c:44-64` (no devman entry; verified by 00 §1.1); `minix3/etc/system.conf:422-429` (`service devman { uid 0; vm SETCACHEPAGE CLEARCACHE }`) | Precondition owned by stage 03 (RS); this stage starts here with a one-sentence import, no re-exposition. |
| T-1 | `main` fills the hook table (init/read/message) | `minix3/minix/servers/devman/main.c:70-91` (`memset` + 3 assignments) | Three slots of the 13-slot `fs_hooks` table are filled. |
| T-2 | `main` fixes `root_stat` (S_IFDIR\|0444, uid/gid 0, size 0, NO_DEV) | `main.c:78-83` | Root appearance contract. |
| T-3 | `main` calls `run_vtreefs(&hooks, 1024, 0, &root_stat, 0, BUF_SIZE)` and never returns | `main.c:89`; `minix3/minix/lib/libvtreefs/vtreefs.c:88` signature; `minix3/minix/include/minix/vtreefs.h:70` declaration | The single entry into the framework; `main.rs:35-37` spin-park is the Rust placeholder for this step (E-DMWIRE). |
| T-4 | SEF startup: `sef_local_startup` → resource allocation (inode pool, I/O buffer; failure = `panic`) | `vtreefs.c:init_server` (framework half, cited by 02 §1.3); production `SefHooks` (`DevmanSef`) landed 2026-09-16 per edge_todo E-ISWIRE note | Framework "alive" step before "in business" step. |
| T-5 | `fsdriver_task` loop starts; 17 VFS request types dispatch via `vtreefs_table`, everything else via `fs_other` | `minix3/minix/lib/libvtreefs/table.c:6-24` (17 slots, `.fdr_other = fs_other`); `vtreefs.c:67-78` (`fs_other` → `message_hook` if non-NULL) | The only two doors into devman logic. |
| T-6 | VFS mounts `/sys` → `fs_mount` calls `init_hook` → first-time guard → `devman_init_devices` builds root + `devices/` + `events` | `mount.c:24-25`; `main.c:36-44` (`static int first`); `device.c:187-207` | Lazy birth: the tree does not exist before the first mount. Rust: `Server::ensure_devices` (DM-P1-3). |
| T-7a | VFS read path: `fs_read` → `read_hook` → per-inode `read_fn` | `file.c:62-79`; `main.c:60-68` (`read_hook` casts `cbdata` to `devman_inode` and calls `read_fn`) | Two `read_fn` targets only: `devman_event_read`, `devman_static_info_read`. |
| T-7b | `devman_event_read`: oldest event rendered; second (empty) read consumes it | `device.c:142-168` (`TAILQ_LAST` + `buf_result()==0` ⇒ remove+free) | Two-read consume protocol (ACK = EOF). |
| T-7c | `devman_static_info_read`: attribute text + `\n` | `device.c:173-183` | Per-attribute files. |
| T-8 | `fs_other` → `message_hook` switch over 4 cases **with no `break`** (fall-through) | `main.c:46-58` | C defect: every message runs all four handlers in order. Rust fixes to single dispatch ([ARCH:A-3], `ipc/dispatch.rs`). |
| T-9 | ADD_DEV: grant allocation failure → ENOMEM; `sys_safecopyfrom` failure → EINVAL; parent lookup failure → ENODEV; else build child + reply with DEVICE_ID | `device.c:223-277` (`do_add_device`); grant copy `device.c:239-240` | Three ordered claim checks before any state change. |
| T-10 | `devman_dev_add_child`: allocate `Device`, id = `next_device_id++`, `add_inode` directory, per-entry static infos, `devman_id` file, link into parent, `get(parent)` | `device.c:345-397`; id alloc `:371`; `add_inode` `:373-375` (return value unchecked in C); `devman_id` file `:393-394` | Birth references: self 1 + parent 1. |
| T-11 | ADD event enqueued (`"ADD <path> 0x%08x"`), `TAILQ_INSERT_HEAD` (newest at head, oldest consumed from tail) | `device.c:75-102`; budget `DEVMAN_STRING_LEN-11` with prefix already in buffer (`:91`) | Queue direction: HEAD-insert / TAIL-consume = FIFO. |
| T-12 | DEL_DEV: lookup failure → ENODEV (after `printf`); else REMOVE event first, BOUND→ZOMBIE if bound, `put` (refcount 0 ⇒ `devman_del_device` reaps) | `device.c:424-455`; `devman_put_device` `:471-480`; `devman_del_device` `:485-515` | Broadcast precedes teardown (path needs a live tree). |
| T-13 | BIND (RS-only): non-RS source → EPERM, **no reply** (`return 0`); device found → rewrite `m_type=DEVMAN_BIND`, `ipc_sendrec(owner)`, driver OK → BOUND + `get`, else propagate error; reply to RS | `bind.c:7-55`; RS gate `:14`; forward `:32`; state+ref `:43-44` | Three-party handshake; devman is the broker, not the decider. |
| T-14 | UNBIND (RS-only): same gate; driver result OK **or 19 (ENODEV)** accepted; non-ZOMBIE → UNBOUND; `put`; RESULT forced OK; reply to RS | `bind.c:56-105`; 19-tolerance `:85`; forced OK `:93` | "Driver already deleted the device" tolerance. |
| T-15 | Driver side (birth of the journey): `devman_init` (DS label lookup), `serialize_dev` (offset layout), grant + `sendrec`, store returned id, local list; `devman_handle_msg` answers forwarded BIND/UNBIND via callbacks | `generic.c:14-21` (state), `:36-99` (serialize), `:102-152` (add), `:154-186` (del), `:188-205` (init), `:207-275` (do_bind/do_unbind/handle_msg) | Panic-on-any-error policy (5 panics). |
| T-16 | USB modeling: descriptor fields formatted as text attributes (`0x%02x`/`0x%04x`), `dev_type` last; device = one ADD, each interface = one more ADD parented to it; remove = interfaces first, then device | `usb.c:24-141` (attributes), `:143-173` (new/delete), `:219-301` (add/remove/init) | `cb_data` carries `(dev_id, interface|-1)`. |
| T-17 | RS publish path: `devman_id != 0` → DS lookup → fill ENDPOINT+DEVICE_ID → `sendrec` → failure kills the service | `manager.c:840-851`; `rs.h:139,182` (`devman_id` field) | Unpublish mirrors with UNBIND, failure only logged (`:897-909`). |
| T-18 | devmand loop: reopen `/sys/events` (default path), `fgets` one line per iteration, `sscanf` ADD/REMOVE, `determine_type` via `<path>/dev_type`, USB id from 8 sysfs attributes, DSL match (`usb.y`/`usb_scan.l`), `minix-service up/down`, up/down/clean scripts, major bitmap, pid file, 50 ms poll | `devmand/main.c:803-872` (parse), `:876-932` (loop), `:519-587` (type), `:633-680` (usb id), `:238-296` (match), `:84-154` (scripts), `:592-628` (major), `:424-431` (pid) | External consumer: documented, never implemented. No shutdown handshake with devman exists (polling + open-fail exit only). |

### 1.3 Order-difference table (teaching order vs runtime order)

| # | Runtime fact (anchor) | Teaching-order choice | Reason | Back-reference compensation |
|---|-----------------------|----------------------|--------|-----------------------------|
| OD-1 | The framework (`run_vtreefs`, T-3..T-5) executes before any shape (device structs) is touched, and mount-triggered birth (T-6) needs shapes immediately. | Teach framework (new-02) **before** shapes (new-03). | The hook table filled in T-1 is meaningless without the dispatcher mental model (two doors: table vs `fs_other`); shapes can then be hung on a known skeleton. Cognitive rule: concrete mechanism (loop) before abstract inventory (fields). | New-02 states up front which shapes it borrows (Device, Event) with forward pointers to new-03; new-03 never forward-references. |
| OD-2 | The device journey starts at the driver (T-15/16: init → serialize → ADD). | Teach server handlers (new-07/08/09) **before** the client library (new-10/11). | Stage ownership: this stage rebuilds the server; the client is the mirror half. Teaching the store before the speaker lets every client claim ("server will reply DEVICE_ID") resolve to an already-taught mechanism. | A declared journey path (new-00 §4) offers the runtime-order reading 10→07→06→13→12→09 as an explicit alternate route. |
| OD-3 | RS loads devman (T-0) before `main` (T-1), and RS handshakes (T-17) interleave with bind (T-13/14). | Teach RS integration (new-12) **after** bind (new-09) and after the client (new-10/11). | RS behavior is only intelligible once both ends of the handshake (server broker + driver callback) are known. T-0 is imported as a one-sentence precondition in new-01, not re-taught. | New-09 treats the driver reply as an opaque OK/error (no dependence on new-10); new-10 deepens it. New-12 back-references new-09/05 only. |
| OD-4 | devmand consumes events (T-18) continuously between ADD (T-11) and BIND (T-13). | Teach devmand (new-13) **last** among semantic chapters. | Consumer-contract reversal: the consumer constrains the producer, so producer chapters (06 events, 11 attributes) must lock their output formats first; the consumer chapter then reads as a checklist against locked formats. | New-06/11 each carry a "consumer constraint" subsection naming the exact devmand parser lines that pin their output. |

---

## 2. Knowledge Pool (deduplicated, stage-wide)

Source types: **S** = stock (from existing documents), **N** = new (from C / non-C
artifacts / OS theory via the §3 audit; evidence anchor mandatory).
Knowledge types: C=concept, M=mechanism, D=data structure, P=interface/protocol,
I=constraint/invariant, A=architectural evolution, T=tooling/engineering,
E=test property.

| ID | Name | Type | Src | Stock location(s) (main telling point bold) | Anchor (C / artifact / doc) | Reader benefit (what question it answers) |
|----|------|------|-----|---------------------------------------------|-----------------------------|-------------------------------------------|
| K-001 | Devman is a file-shaped userland server (device tree = file tree, events = file contents) | C | S | **00 §1.1**, 01 §1.1 | `device.c:187-207` (tree birth); `main.c:89` (VTreeFS entry) | Why does a device manager serve files at all? |
| K-002 | RS-load-group membership (not in boot_image; RS runtime load, uid 0 + VM privileges) | C | S | **01 §2.8**, 00 §1.1, 12 (permissions) | `kernel/table.c:44-64` (absence); `system.conf:422-429` | Where in the boot story does devman appear? |
| K-003 | Startup mainline main → run_vtreefs → mount → init_hook → devices | M | S | **01 (whole)**, 00 §1.2 | `main.c:70-91`; `mount.c:24-25` | At which point of process life is each structure born? |
| K-004 | Three hooks registration (init/read/message into fs_hooks) | M | S | **01 §1.3** | `main.c:73-76`; `vtreefs.h:25-43` | What does devman customize vs inherit? |
| K-005 | root_stat contract (S_IFDIR\|0444, uid/gid 0, size 0, NO_DEV) | P | S | **01** | `main.c:78-83` | What does VFS see as `/`? |
| K-006 | SEF lifecycle (fresh/restart; init_server alloc vs mount-time tree build) | M | S | **01**, 02 §1.3 | `vtreefs.c:init_server`; Rust `DevmanSef` (E-ISWIRE 2026-09-16) | Why is the tree lazy and where can startup fail? |
| K-007 | First-only guard (`static int first` → `ensure_devices`/`Option<DeviceTree>`) | I | S | **01**, 04 | `main.c:37-42`; `server.rs` (`ensure_devices`, DM-P1-3) | Why does remount not rebuild the tree? |
| K-008 | VTreeFS two-door dispatch (17-slot table vs fs_other) | M | S | **02 §1.2** | `table.c:6-24`; `vtreefs.c:67-78` | Which door does each request take? |
| K-009 | Unfilled hook slots walk framework defaults (ls works with 3 hooks) | C | S | **02 §1.2** | `file.c` lookup/getdents (no hook); `vtreefs.h` 13 slots | Why is filling 3 of 13 slots sufficient? |
| K-010 | Framework inode tree API (add/delete/get_root/get_name/stat/indexed slots) | P | S | **02** | `libvtreefs/inode.c`, `mount.c`, `file.c`, `vtreefs.h` | What tree operations can the server assume? |
| K-011 | Framework read chunk loop (hook called per chunk; offset-driven) | M | S | **02 §2.5**, 06 | `file.c:62-79` | Why does the event protocol reuse "return 0" as ACK? |
| K-012 | Inode-content ownership (`InodeContent::{Dir,Static,Events}` replaces cookie side-table + unsafe static) | A | S | **02**, 06 | DM-P1-5; `vtreefs/inode.rs` | Where does readable content live after the cleanup? |
| K-013 | `devman_device` full-field shape (id/name/ref/state/owner/parent/children/infos/inode/info) | D | S | **03 §1** (identity 1) | `devman.h:74-101` | What is a device, field by field? |
| K-014 | Device/file/wire triple identity (tree node vs file set vs wire bytes) | C | S | **03 §1** | `devman.h` + `devinfo.h` + `minix/devman.h:8-20` | Which struct do I look at for which job? |
| K-015 | State machine UNBOUND 0 / BOUND 1 / ZOMBIE 2 + transition sites | M | S | **03 §1.1**, 07/08/09 | `devman.h:89-91`; transitions `device.c:365/:443`, `bind.c:43` | What states exist and who moves them? |
| K-016 | Wire layout (`devman_device_info` + entries + string area, offset addressing) | P | S | **03 §2.4**, 10 | `minix/devman.h:8-20`; `generic.c:36-99` (`serialize_dev`) | What bytes cross the grant? |
| K-017 | `subsystem_offset` written by client, never read by server | I | S | **03**, 05, 99 | `devinfo.h:25` defined; zero refs in `device.c` (grep) | Which wire field is compatibility-only? |
| K-018 | `devman_event` (128-byte line) + `devman_event_inode` queue head | D | S | **03**, 06 | `devman.h:62-72` | What is an event, as stored? |
| K-019 | `devman_inode` (inode ptr + read_fn + data) + static-info/file/event inode variants | D | S | **03**, 06 | `devman.h:52-72` | How is "readable" attached to a node? |
| K-020 | Integer narrowing (int ids/counts → `DeviceId`/`u32` newtypes; overflow handling) | A | S | **03/04/08** | [ARCH:A-5]; `device_tree.rs:155-159` | Where did bare ints go and what guards the wrap? |
| K-021 | Dual-tree model (framework InodeTree appearance vs DeviceTree substance, joined by Ino handle) | C | S | **04 §1** | `device.c` (`inode.inode` ptr) vs `device_tree.rs` (`binding.ino`) | Why are there two trees and how are they stitched? |
| K-022 | `devman_init_devices` birth content (root id 0/major -1 + `devices/` + `events`) | M | S | **04** | `device.c:187-207` | What exists right after first mount? |
| K-023 | DFS lookup (`_find_dev` recursion + wrapper) | M | S | **04** | `device.c:283-308` | How is id → device resolved? |
| K-024 | Path generation (recursive strcat, `./` prefix + trailing slash provenance, ENOMEM budget) | M | S | **04**, 06 | `device.c:45-70` | Where do event-line paths come from? |
| K-025 | `next_device_id` allocator (starts 1, unbounded, dense-check couples it to Vec length in Rust) | M | S | **04** | `device.c:16`; `device_tree.rs:155-174`; DM-P1-1 | What breaks if an ADD fails after allocating? |
| K-026 | Default stats (dir 0444 size 0 / file 0444 size 0x1000) | P | S | **04** | `device.c:18-33` | What stat does each new node carry? |
| K-027 | DEVMAN_* constant table (BASE 0x1200, 10 messages +0..+9, 5 field macros) | P | S | **05**, 99 | `com.h:846-866` | What numbers travel in `m_type`? |
| K-028 | m4 three-word dual face (GRANT_ID/SIZE vs DEVICE_ID/RESULT vs ENDPOINT aliasing) | P | S | **05 §1.1** | `com.h:859-864`; `ipc/message.rs` phase view | How do three words serve four messages? |
| K-029 | Grant copy primitive (`sys_safecopyfrom`, three ordered errors) | M | S | **05 §2.2**, 07 | `device.c:239-240` | How do wire bytes arrive safely? |
| K-030 | `do_reply` (REPLY + RESULT, single-word; ADD also sets DEVICE_ID pre-reply) | M | S | **05**, 07 | `device.c:213-219`; `:270` | What does a reply carry? |
| K-031 | RS-only gate (non-RS → EPERM, no reply, warning print) | I | S | **05 §2.4**, 09, 12 | `bind.c:14,63` (`src = m->m_source`) | Who may bind, and what happens to intruders? |
| K-032 | message_hook fall-through (4 handlers run in order per message; C defect; Rust single dispatch) | A | S | **05 (A-3 verdict)**, 01 §2.4 (phenomenon) | `main.c:46-58`; `ipc/dispatch.rs:1-13` | Is the cascade a feature? (No — fixed.) |
| K-033 | Unimplemented surface (5 com.h messages + DYNAMIC info type; fail-closed, constants kept) | I | S | **05 (A-6 verdict)**, 03, 07 | `com.h:850-856` (zero users, grep); `device.c:404-418` | What is declared but must be refused? |
| K-034 | buf three cursors (skip/left/used; BUF_SIZE-1 NUL reserve) | M | S | **06 §2.1** | `buf.c:8-84`; `buf.rs:53-71` | How does offset reading avoid re-rendering? |
| K-035 | Event queue direction (HEAD-insert, TAIL-consume = FIFO) | I | S | **06** | `device.c:96,130` vs `:150` (`TAILQ_LAST`) | Which end is newest? |
| K-036 | Two-read consume protocol (data read + EOF read = one delivery; consumed event freed) | M | S | **06 §1** | `device.c:142-168` | When is an event really gone? |
| K-037 | Event line format (`"ADD <path> 0x%08x"` / `"REMOVE …"`, 128 cap, -11 id-suffix budget, prefix-in-buffer accounting) | P | S | **06**, 07/08 | `device.c:75-136`; `devman.h:44-45,67`; DM-P1-4 | What exact bytes does devmand parse? |
| K-038 | Static-info read (`data + "\n"`, truncation at 127+NUL on write) | M | S | **06** | `device.c:173-183`; `devman_dev_add_static_info` `:314-339` | Why does every attribute read end with newline? |
| K-039 | read_hook dispatch (cbdata → read_fn; two targets only) | M | S | **01/06** | `main.c:60-68` | How does a read find its function? |
| K-040 | ADD 8-step flow (claim → settle → broadcast + id reply) | M | S | **07** | `device.c:223-277,345-397` | What happens between grant arrival and reply? |
| K-041 | ADD claim triple (ENOMEM alloc / EINVAL copy / ENODEV parent, in order, no state change) | I | S | **07 §2.1** | `device.c:228-256` | Which failures leave zero residue? |
| K-042 | `devman_id` accessibility file (decimal id as static info) | P | S | **07** | `device.c:393-394` | How does userland learn its own id? |
| K-043 | STATIC-only attribute loop (DYNAMIC → -1 TODO) | I | S | **07** | `device.c:404-418` | Which attribute kinds survive registration? |
| K-044 | ADD failure rollback (id-slot + staged inode unwind; NEW failure mode from Rust error propagation) | M | N | todo DM-P1-1/DM-P1-4; `add_device.rs` | `device_tree.rs:155-174` vs `device.c:371-375` (C unchecked) | Why must allocation move after fallible steps? |
| K-045 | DEL flow (find → REMOVE event → maybe ZOMBIE → put → reap at zero) | M | S | **08** | `device.c:424-515` | Why is broadcast first and reaping last? |
| K-046 | Refcount four functions (get/put skip NULL+root; birth 1; parent +1 per child) | M | S | **08**, 07/09 | `device.c:460-480`; `:394-397` | Who holds a reference and when is it dropped? |
| K-047 | ZOMBIE semantics (bound-but-deleted: name gone, body kept until last put) | C | S | **08**, 03 | `device.c:443-445`; `devman_del_device` `:485-515` | What does "deleted but still referenced" mean? |
| K-048 | Reap 8 steps (infos unlink+free, device inode delete, parent unlink + parent put, info free, self free) | M | S | **08** | `device.c:485-515` | What does final freeing touch, in order? |
| K-049 | Bind 4-step handshake (RS gate → find → forward via sendrec → state+ref on driver OK) | M | S | **09** | `bind.c:7-55` | Why is devman a broker, not the decider? |
| K-050 | Unbind 19-tolerance + forced-OK + ZOMBIE guard | I | S | **09** | `bind.c:85,93,89-91` | When does failure still count as success? |
| K-051 | Server assembly (`Server::handle_other`, `OutAction`, `DevmanMsg` classify; unified run loop) | M | S | **09 (assembly half)**, 02/05 | `server.rs:91-170`; DM-P1-2/DM-P2-1/DM-P2-2 | Where do all four messages meet one loop? |
| K-052 | Client codec mirror (save_string offsets, serialize layout = server parse layout, subsystem zero-hygiene) | P | S | **10**, 03 | `generic.c:24-99`; `devman_client.rs:101` (`encode_device`) | What must change in three places at once? |
| K-053 | Client lifecycle (init via DS label → add/del via grant+sendrec → local dev_list → handle_msg sort) | M | S | **10** | `generic.c:102-275` | How does a driver speak devman? |
| K-054 | Client panic policy (any failure aborts the driver; Rust → Result) | I | S | **10 §2.3** | 5 `panic` sites in `generic.c` | Who decides to die on error? |
| K-055 | Driver-side bind response (`do_bind`/`do_unbind`: dev_list search, callback, ENODEV default) | M | S | **10**, 09 | `generic.c:207-256` | What answers the forwarded BIND? |
| K-056 | USB attribute spelling (5 device numbers + conditional strings + dev_type last; 6 interface numbers + dev_type) | P | S | **11 §2.1** | `usb.c:24-141`; `usb_model.rs:301` (byte-locked tests) | Which exact strings does devmand match on? |
| K-057 | One-device-many-interfaces (device ADD + N interface ADDs; cb_data `(id, iface|-1)`) | C | S | **11 §1** | `usb.c:219-274` | Why is one plug N registrations? |
| K-058 | USB remove-before-delete order (interfaces del, then device del; delete frees attrs) | M | S | **11** | `usb.c:276-293,174-193` | What order undoes an add? |
| K-059 | RS publish/unpublish handshake (DS lookup, fill ENDPOINT+DEVICE_ID, sendrec, kill-vs-log asymmetry) | M | S | **12** | `manager.c:840-851,897-909`; `rs_contract.rs` | When does RS speak for a service? |
| K-060 | devman_id three stations (boot-image inherit → publish consume → unpublish release) | C | S | **12** | `manager.c:1742` + `:840-851` + `:897-909` | What is the lifetime of a devman_id? |
| K-061 | devmand event parsing (one line per 50 ms iteration; sscanf dual format; unknown lines warned+ignored) | M | S | **13 §2.1** | `devmand/main.c:803-872,876-932` | How are event bytes turned into actions? |
| K-062 | devmand type judgment + USB id synthesis (dev_type file read; 8-attribute id) | M | S | **13** | `main.c:519-587,633-680` | How is "what kind of device" decided? |
| K-063 | Driver match DSL (`usb.y`/`usb_scan.l` + `match_usb_driver`) and start/stop via `minix-service` + major bitmap + up/down/clean scripts + pid file | P | S | **13** | `usb.y` (134 lines), `usb_scan.l` (43); `main.c:84-231,592-628,424-431`; `etc/devmand/*.cfg` | How does a matched driver become a process and a `/dev` node? |
| K-064 | Dead code verdicts (`devman_device_file`, `DEVMAN_DEFAULT_MODE`, `major` legacy, `PNAME_MAX` doc-only) | I | S | **03/05/99**, plan §5.5 | grep: definition-only sites | What exists in headers but must never be modeled? |
| K-065 | No-shutdown fact (no devman exit handshake; devmand exits on open-fail; RS kill path is the only death) | I | N | devmand `main.c:904-911`; RS `kill_service` | No `cleanup` counterpart in `servers/devman/` (grep) | How does devman die? (It doesn't — it is killed.) |
| K-066 | Single-threaded invariant (event loop, no locks; `Rc`/`RefCell` legitimate; files.rs unsafe removed) | I | S | **00 §3.1**, 01; DM-P1-5 | `lib.rs:8-14`; `cargo grep unsafe` = 0 post-fix | What concurrency model may the rewrite assume? |
| K-067 | Production-transport absence (parked main, test-only Transport/ClientTransport/RsTransport, classifier missing) | T | N | todo §1.2; edge E-DMWIRE | `main.rs:35-37`; `vtreefs/mod.rs:322`; `server.rs:91` | What still separates the semantic library from a running server? |
| K-068 | Error-style migration (panic/printf-warn → `Result`/`Errno`; ENOMEM/EINVAL/ENODEV/EPERM/OK table) | A | S | **99** (codes), handlers | [ARCH:A-7]; `bind.c:14,63`, `device.c:228-256` | Which C failure becomes which Rust error? |
| K-069 | Wire-encode test duality (no cross-crate roundtrip test; triple-mirror review discipline) | E | S | **10 §1** | `encode_device` vs `wire.rs` parse vs `serialize_dev` | How is codec agreement verified without a shared test? |
| K-070 | Journey path map (ADD → event → devmand → RS bind → BOUND, drawn in 09 + 13) | C | S | **09 §1.2**, 13, 00 §1.2 | Cross-chapter figure | Where is the whole trip drawn? |

Statistics: 70 entries — by source: stock 65, new 5 (K-044, K-065, K-066-record,
K-067, K-069-record; K-044/K-065/K-067 carry C/artifact anchors, the rest
record decisions already closed in todo.md). By type: C 9, M 26, D 4, P 15,
I 11, A 3, T 1, E 1. By stock home: 00:2, 01:7, 02:6, 03:9, 04:6, 05:8, 06:7,
07:5, 08:4, 09:4, 10:4, 11:3, 12:2, 13:3, 99:2 (multi-homed entries counted at
their main telling point; counts overlap intentionally and sum > 70).

---

## 3. Coverage Audit

### 3.1 Topic universe (four sources)

1. **C symbols**: all top-level definitions in `servers/devman/` (27 functions
   + 6 statics, per plan §5.3 — spot re-verified: `main.c` 4 fns, `device.c`
   17 fns + 6 statics, `bind.c` 2 fns, `buf.c` 4 fns), `libdevman` 16
   functions + 2 static callbacks (`generic.c` 8, `usb.c` 8+2),
   `devman.h`/`devinfo.h`/`minix/devman.h`/`local.h` structures, `com.h`
   10 constants + 5 macros, `vtreefs.h` 13 hook slots + `run_vtreefs`,
   RS `manager.c` 3 sites + `rs.h` 2 fields, devmand `main.c` 12 key
   functions + `usb.y`/`usb_scan.l` DSL + `usb_driver.h` flags.
2. **OS-theory concepts**: pseudo-filesystem device exposure (sysfs/scheme
   analogues), event-broadcast reliability (at-least-once via two-read ACK),
   three-party bind brokerage, reference-counted teardown with zombie
   intermediate, lazy mount-triggered initialization, fail-closed unknown
   messages, single-threaded server invariant.
3. **Non-C artifacts**: link scripts, asm/trap entries, boot chain, image
   layout, userland startup, cross-module wire, build scripts, test
   infrastructure, plus devmand configs/scripts and system.conf service
   declaration (see §3.4 for the fixed 10-item checklist).
4. **Boundary contracts**: master-plan stage table (devman = RS-loaded group
   with IS/INPUT/IPC), edge_todo E-DMWIRE/E-REQWIRE/E-ISWIRE/E-DSWIRE/E5(h),
   todo.md DM-items (closed), previous stage (MIB) concepts (no overlap).

### 3.2 Coverage-gap table (topics in the universe, covered nowhere)

| # | Gap topic | Evidence anchor | Recommendation → pool action |
|---|-----------|-----------------|------------------------------|
| G-1 | **Server assembly as a documented unit**: `Server::run` unified loop, `OutAction` execution, `DevmanMsg::classify`, transport seams (Transport/ClientTransport/RsTransport/SefHooks-production) and their E-DMWIRE absence. Today split across 09 (assembly half), 02, 01 without a home chapter. | `server.rs:91-170`; `main.rs:35-37`; E-DMWIRE (edge_todo) | **New chapter new-14** (contract §5.14). No new pool entries needed (K-051, K-067 already cover it); chapter owns the assembly narrative. |
| G-2 | **Shutdown/exit story**: no devman-side cleanup exists; devmand exits on events-open failure; RS `kill_service` is the only death path. No chapter states this. | devmand `main.c:904-911`; `manager.c` `kill_service`; zero cleanup symbols in `servers/devman/` (grep) | Fold into new-14 as a closed subsection (K-065 added to pool). Explicit "there is no shutdown protocol" verdict. |
| G-3 | **Concurrency invariant home**: single-threaded rationale scattered (00 §3.1, 01, DM-P1-5). Needs one authoritative statement with the `unsafe`=0 evidence. | `lib.rs:8-14`; DM-P1-5 | Fold into new-14 §1 (K-066). New-00..13 chapters cite it instead of re-arguing. |
| G-4 | **Request classifier shape** (raw IPC → `Request`/`DevmanMsg`; grant-copy production side; dirent/stat encoding ownership vs E-REQWIRE). Declared "transport business" in code comments but undescribed anywhere. | `vtreefs/mod.rs:50-56` self-note; `add_device.rs:7-9` comment; E-REQWIRE | Fold into new-14 as the "wiring backlog" section with per-seam acceptance tests. No implementation (blocked), but the shape contract lands. |

No other gaps: every C function/structure/constant, every devmand key
function, every boundary contract has a stock home (verified against the pool:
K-001..K-070 cover T-0..T-18 exhaustively). The four gaps above are all
assembly/backlog topics, i.e. the stage's semantic surface is complete and its
production-wiring surface is not — exactly the todo.md §1.2 verdict, reached
here by independent audit.

### 3.3 Duplicate-topic table (same topic expanded in several documents)

| # | Topic | Occurrences (main telling point bold) | New-catalog ruling (keep as main; rest become references) |
|---|-------|---------------------------------------|----------------------------------------------------------|
| D-1 | message_hook fall-through phenomenon vs verdict | 01 §2.4 (phenomenon) + **05 (A-3 verdict)** | Keep verdict in new-05; new-01 keeps a 3-line phenomenon box pointing to new-05. |
| D-2 | Dead-code verdicts (dead struct/macro, legacy major, subsystem_offset) | 03 + 05 + **99** + plan §5.5 | Single verdict home: new-99 table; new-03/05 keep one-line pointers. (K-064.) |
| D-3 | Event-line format + budget (-11, prefix accounting) | 06 + **07/08 (production)** + 13 (consumption) + 04 (path half) | Format authority: new-06 (K-037). New-07/08 cite the format, own only their call-site budget constants. New-13 cites, never restates. New-04 owns path construction only. |
| D-4 | Wire layout triple mirror (server parse / client encode / doc layout) | **03 (layout)** + 10 (encode) + `wire.rs`/`devman_client.rs` | Layout authority: new-03. New-10 owns encode; the triple-change discipline (K-052/K-069) is stated once in new-10, cited from new-03. |
| D-5 | RS-only gate | **05 (gate)** + 09 (use) + 12 (RS view) | Gate authority: new-05 (K-031). New-09/12 cite. |
| D-6 | Lazy-birth guard (`static int first` → ensure_devices) | 01 + **04 (birth content)** | Guard-mechanism authority: new-01 (K-007). New-04 owns birth content (K-022). New-04's guard paragraph shrinks to a pointer. |
| D-7 | Single-threaded justification | 00 §3.1 + 01 + DM-P1-5 notes | Single authority: new-14 (K-066). Others cite. |
| D-8 | Journey-path figure | 00 §1.2 + **09 §1.2** + 13 | Canonical figure: new-09 (binding leg) + new-13 (consumer leg); new-00 keeps the thumbnail route only (K-070). |

### 3.4 Out-of-scope-topic table (taught beyond declared boundary)

| # | Topic | Current location | Correct home / ruling |
|---|-------|------------------|-----------------------|
| O-1 | Server assembly (`Server`, `OutAction`, `DevmanMsg`) inside the bind chapter | 09 second half | **Move to new-14.** New-09 keeps handshake only (K-049/050). This is the single structural split of the rebuild. |
| O-2 | VTreeFS full-framework semantics (getdents/indexed slots/statvfs/link) beyond devman-used surface | 02 edges | Confirm 02's "devman-used surface" cut (plan §5.5); full framework belongs to the procfs/FS stage. New-02 states the cut explicitly with the `NO_INDEX` evidence. |
| O-3 | USB descriptor decoding (assumed decoded on entry to usb.c modeling) | 11 edge (correctly excluded) | Keep excluded; new-11 states the assumption once (`minix-usb` stub). |
| O-4 | RS internals (slot management, kill implementation, DS implementation) | 12 edges (correctly excluded) | Keep excluded; new-12 stays at the handshake contract. |

### 3.5 Non-C topics: fixed checklist, ten explicit answers

| Topic | Where it is taught, or why not in this stage |
|-------|----------------------------------------------|
| Linking & loading | New-01: `service devman` is fork+exec'd by RS (`system.conf:422-429`); no link script of its own (single `Makefile` per directory). One subsection, closed. |
| Image & memory layout | Not in this stage: devman has no image slot (`table.c` absence) and no memory layout of its own (heap `malloc`/TAILQ → `Box`/`VecDeque`, A-2, taught in new-03/04/06). Explicit exclusion with reason. |
| Assembly entry & trap entry | Not in this stage: userland server, no asm entry; trap-to-IPC bridge belongs to the kernel/IPC stages. Explicit exclusion with reason. |
| Boot assembly | New-01 imports T-0 in one sentence (RS-load group, master-plan causal chain); boot detail belongs to 01-stage-kernel/03-stage-rs. |
| Build & toolchain | `servers/devman/Makefile`, `lib/libdevman/Makefile`, `commands/devmand/Makefile` + `devmand.cfg`: one closed subsection in new-13 (consumer build) and one line in new-01 (server build). No full build chapter (nothing nonstandard). |
| Cross-module interfaces & wire formats | New-05 (DEVMAN messages), new-03/10 (device wire), new-06/13 (event lines), new-04/13 (sysfs paths), new-12 (RS handshake words). Fully covered — this stage's backbone. |
| Error paths | New-99 error-code table (K-068) + per-handler ordered errors (K-041, T-9/T-12/T-13/T-14). Fully covered. |
| Shutdown & exit | New-14 (K-065, G-2): the no-shutdown verdict. |
| Concurrency & synchronization | New-14 (K-066, G-3): the single-threaded invariant. |
| Test infrastructure | Per-chapter §5 test lists (existing convention) + new-14 acceptance section for the E2E lifecycle chain (E5(h) four links). No separate test chapter (tests are unit-level per module; integration is blocked on transport). |

---

## 4. New Catalog

Design decisions (mine, with reasons):

1. **Numbers are stable: no renumbering.** Rationale: 22 Rust files carry
   `doc NN-…` anchors (§0 evidence), 10+ external documents reference this
   stage, and the current numbering already satisfies forward-reference-freedom
   (§9 G3 check). Renumbering cost (history lesson: I-14 "renumber churn risk
   exceeds benefit") repeats for zero pedagogical gain — the one genuine
   structural defect (O-1) is fixed by *appending*, not renumbering.
2. **One split, one addition, zero archives.** The 15 existing chapters all
   carry exactly one semantic unit each — except new-09, which carries two
   (handshake + assembly). Split off the assembly half into **new-14**;
   everything else keeps its number, title, and boundary (with the duplicate
   rulings of §3.3 applied as in-chapter edits, not moves).
3. **Parallel topics use converge-plus-trigger organization** (§5.2 of the
   prompt): the message surface (new-05) is the converge chapter for the four
   messages; handlers (new-07/08/09) group by lifecycle role (birth/death/
   readiness) with ADD worked in full as the representative member and
   DEL/BIND/UNBIND each carrying a difference table against it (error order,
   reply shape, state touched).

### 4.1 New chapter master table

| New # | Title | One-line positioning | Group |
|-------|-------|----------------------|-------|
| 00 | devm-overview | The map: four-layer territory, two mainlines, reading routes | A. Map |
| 01 | devm-init-main | Birth: RS load → main → hooks → run_vtreefs → lazy mount birth | B. Birth & frame |
| 02 | vtreefs-framework | The loop's other half: dispatch table, inode tree, read chunking | B. Birth & frame |
| 03 | devm-structs | The shapes: device/event/inode structures + wire layout | C. Shapes & ground |
| 04 | device-tree | The tree: birth content, DFS lookup, path building, id allocation | C. Shapes & ground |
| 05 | devm-message-contract | The only write channel: constants, grant copy, reply, RS gate, fall-through verdict | D. Write & read halves |
| 06 | event-buf | The read half: buf cursors, event queue, two-read ACK, static reads | D. Write & read halves |
| 07 | devm-add-device | Life begins: the 8-step ADD, worked in full (representative member) | E. Lifecycle handlers |
| 08 | devm-del-device | Life ends: DEL + refcount + ZOMBIE + reap (difference table vs 07) | E. Lifecycle handlers |
| 09 | devm-bind-unbind | Readiness: RS handshake + owner forwarding + 19-tolerance (assembly moved out) | E. Lifecycle handlers |
| 10 | libdevman-client | The mirror half: driver-side codec, lifecycle, bind response | F. Driver side |
| 11 | usb-device-model | The registration source: USB attributes, device+interfaces, callbacks | F. Driver side |
| 12 | rs-integration | The registrar: publish/unpublish handshake, devman_id stations, permissions | G. Neighbors |
| 13 | devmand-consumer | The first reader: event parsing, type judgment, DSL match, start/stop scripts | G. Neighbors |
| 14 | server-assembly | **NEW.** The wiring: unified run loop, OutAction, transport seams, no-shutdown verdict, E2E acceptance | H. Wiring & backlog |
| 99 | devm-global-concepts | The catch-all: constants, error codes, cross-service refs, dead-code verdicts | Z. Catch-all |

Filenames: existing chapters keep their filenames byte-for-byte;
new-14 is `14-server-assembly.md`. Total: 16 chapters + plan/todo/README/draft
references (unchanged roles).

### 4.2 Reading paths

- **Mainline** (default, numeric): 00 → 01 → 02 → 03 → 04 → 05 → 06 →
  07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 99.
- **Journey path** (one device from plug to bound): 10 → 11 → 07 → 06 →
  13 → 12 → 09 → (03 state machine on demand).
- **Server-builder path** (implement the rewrite): 01 → 02 → 03 → 04 →
  05 → 06 → 07 → 08 → 09 → 14 → (10/11/12/13 as contract checks).
- **Skippable**: 11 (USB-only), 13 internals beyond the contract tables
  (devmand yacc/lex detail), 99 (reference; consult on demand).
- Parallel groups: handlers {07 representative; 08, 09 difference tables};
  driver side {10 framework; 11 USB family}; neighbors {12 registrar; 13
  consumer}. Each group chapter declares mainline/branch/skippable status in
  its header.

---

## 5. Per-Chapter Contracts (all 16; missing one blocks delivery)

### 00-devm-overview

- One-line positioning: answers "what is devman, how big is the whole, in
  what order do I read" — map only, no mechanism.
- Covers: K-001, K-002 (one sentence each), K-070 (thumbnail routes),
  chapter navigation table with mainline positions, ARCH index (A-1..A-10
  one line each), test-baseline convention.
- Explicitly not covers: any mechanism (→ 01..14); constant values (→ 99);
  assembly shape (→ 14); journey detail (→ 09/13 figures).
- Prerequisites: none (entry point; kernel/RS background imported in one
  sentence each).
- Post-position: all chapters cite this map; none is required before it.
- Ground truth: `servers/devman/` (4 .c + 3 .h, 847+191 lines);
  `lib/libvtreefs/` (14 files, devman-used surface);
  `lib/libdevman/` (2 .c + 1 .h, 576+27 lines); `com.h:846-866`;
  `minix/devman.h`; RS/devmand/usbd externals (paths only).
- Knowledge items:
  | K-001 | File-shaped server | C | map §1 | S: 00 §1.1 |
  | K-002 | RS-load group | C | map §1 | S: 01 §2.8 (import) |
  | K-070 | Journey thumbnail | C | routes §4 | S: 09 §1.2 + 13 |
- Acceptance: a new reader can state the four layers, both mainlines, and
  the three reading paths without opening any other chapter; every
  mechanism question raised here resolves to exactly one chapter link.

### 01-devm-init-main

- One-line positioning: the temporal anchor — how the devman process comes
  alive and mounts itself into VFS.
- Covers: K-002, K-003, K-004, K-005, K-006, K-007.
- Explicitly not covers: framework internals (→ 02); dispatch verdicts
  (→ 05, keeps only a 3-line fall-through phenomenon box per D-1); handler
  bodies (→ 07..09); shapes (→ 03); tree content (→ 04); reads (→ 06).
- Prerequisites: 00 only.
- Post-position: 02 (needs the call site), 04 (needs the guard), 05 (needs
  the hook), 14 (needs SEF + park status).
- Ground truth: `servers/devman/main.c` (all 93 lines);
  `vtreefs.h:25-43,70` (hook slots + run declaration);
  `vtreefs.c:88` (run), `mount.c:24-25` (init_hook call);
  `system.conf:422-429`; Rust `hooks.rs` + `main.rs` (+ `DevmanSef`).
- Knowledge items:
  | K-002 | RS-load membership | C | `table.c` absence + system.conf | S: 01 §2.8 |
  | K-003 | Startup mainline | M | `main.c:70-91` | S: 01 |
  | K-004 | Three-hook fill | M | `main.c:73-76` | S: 01 §1.3 |
  | K-005 | root_stat | P | `main.c:78-83` | S: 01 |
  | K-006 | SEF lifecycle | M | `vtreefs.c:init_server` + DevmanSef | S: 01 |
  | K-007 | First-only guard | I | `main.c:37-42` | S: 01 |
- Acceptance: reader can narrate T-0..T-6 with an anchor per step; can
  explain why the tree is absent before first mount and why remount does
  not rebuild; fall-through box contains zero verdict language (verdict
  lives in 05).

### 02-vtreefs-framework

- One-line positioning: everything after `run_vtreefs` that is framework,
  not business — dispatch doors, inode tree, read chunking.
- Covers: K-008, K-009, K-010, K-011, K-012.
- Explicitly not covers: business shapes (→ 03); birth content (→ 04);
  read_fn bodies (→ 06); message verdicts (→ 05); full-framework surface
  beyond devman use (→ procfs/FS stage, O-2 cut stated).
- Prerequisites: 01 (call site, hook shape, SEF).
- Post-position: 03 (shapes hang here), 04 (joins here), 06 (chunk loop
  assumed), 14 (classifier backlog references the table).
- Ground truth: `lib/libvtreefs/` devman-used surface (`vtreefs.c`,
  `table.c:6-24`, `inode.c`, `mount.c`, `file.c:62-79`, `path.c`,
  `stadir.c`) + `vtreefs.h` (13 slots, `NO_INDEX`); Rust `vtreefs/`
  (`inode.rs` tree + `mod.rs` dispatch/transport).
- Knowledge items:
  | K-008 | Two-door dispatch | M | table.c + vtreefs.c:67-78 | S: 02 §1.2 |
  | K-009 | Default slots | C | file.c lookup/getdents | S: 02 §1.2 |
  | K-010 | Inode-tree API | P | inode.c/mount.c/file.c | S: 02 |
  | K-011 | Read chunk loop | M | file.c:62-79 | S: 02 §2.5 |
  | K-012 | InodeContent ownership | A | DM-P1-5 | S: 02 + 06 |
- Acceptance: reader can route any arriving message to its door; can state
  the O-2 cut (which framework parts devman does NOT use and where they
  belong); can explain why `ls /sys` works with 3 hooks.

### 03-devm-structs

- One-line positioning: the shape base — every field of every
  structure plus the wire layout; operations elsewhere.
- Covers: K-013, K-014, K-015, K-016, K-017, K-019, K-020, K-064 (shape
  half of dead verdicts, pointer to 99).
- Explicitly not covers: tree ops (→ 04); message surface (→ 05);
  queue/reads (→ 06); add/del (→ 07/08); client construction (→ 10/11).
- Prerequisites: 02 (inode attachment: FileBinding.ino = 02's Ino).
- Post-position: 04, 05, 06, 07, 10 (all consume shapes).
- Ground truth: `servers/devman/devman.h` (108, all) + `devinfo.h` (35,
  all) + `minix/devman.h:8-20` + `generic.c:36-99` (layout proof);
  Rust `structs.rs` + `wire.rs`.
- Knowledge items:
  | K-013 | devman_device fields | D | devman.h:74-101 | S: 03 §1 |
  | K-014 | Triple identity | C | 3 headers | S: 03 §1 |
  | K-015 | State machine | M | devman.h:89-91 | S: 03 §1.1 |
  | K-016 | Wire layout | P | minix/devman.h + serialize | S: 03 §2.4 |
  | K-017 | subsystem_offset compat | I | grep (server never reads) | S: 03 |
  | K-019 | inode variants | D | devman.h:52-72 | S: 03 |
  | K-020 | Integer narrowing | A | A-5 | S: 03 |
  | K-064 | Dead struct/macro (shape half) | I | grep definition-only | S: 03 (verdict → 99) |
- Acceptance: reader can draw the three identities and their Rust
  counterparts; can state the wire byte order (header + entries + strings)
  and which field is compat-only; state values 0/1/2 locked.

### 04-device-tree

- One-line positioning: the tree's three operations — birth, find, path —
  plus id allocation.
- Covers: K-021, K-022, K-023, K-024, K-025, K-026.
- Explicitly not covers: events/reads (→ 06); add/del business (→ 07/08,
  which call insert); bind (→ 09); guard mechanism (→ 01 per D-6, pointer
  only).
- Prerequisites: 03 (Device shape), 02 (framework add/name), 01 (guard).
- Post-position: 06 (paths consumed), 07/08 (insert/delete callers), 13
  (path consumer).
- Ground truth: `device.c:16-39` (statics + default stats), `:45-70`
  (generate_path), `:187-207` (init_devices), `:283-308` (find);
  Rust `device_tree.rs`.
- Knowledge items:
  | K-021 | Dual-tree + Ino stitch | C | inode ptr vs Ino | S: 04 §1 |
  | K-022 | Birth content | M | device.c:187-207 | S: 04 |
  | K-023 | DFS lookup | M | device.c:283-308 | S: 04 |
  | K-024 | Path generation | M | device.c:45-70 | S: 04 |
  | K-025 | next_device_id | M | device.c:16 + DM-P1-1 | S: 04 |
  | K-026 | Default stats | P | device.c:18-33 | S: 04 |
- Acceptance: reader can explain two trees + stitch; birth content listed
  exactly; path `./`-prefix/trail-slash provenance shown; id-dense
  invariant and its failure mode (DM-P1-1) stated.

### 05-devm-message-contract

- One-line positioning: the protocol surface — the only write channel, its
  constants, copy primitive, reply shape, permission gate, and the two
  verdicts (A-3 fix, A-6 defer). Verdicts made once, cited everywhere.
- Covers: K-027, K-028, K-029, K-030, K-031, K-032, K-033.
- Explicitly not covers: handler bodies (→ 07/08/09, which call these
  primitives); event bytes (→ 06); client construction (→ 10/11); RS
  publish flow (→ 12).
- Prerequisites: 01 (message_hook call site), 03 (wire struct).
- Post-position: 07/08/09 (primitives), 10 (client mirror), 12 (gate RS
  view), 14 (classifier backlog).
- Ground truth: `com.h:846-866`; `device.c:213-219` (do_reply),
  `:228-256` (grant triple); `bind.c:7-19,56-68` (RS gate);
  `main.c:46-58` (dispatch shape); Rust `ipc/` + `minix-types com.rs`.
- Knowledge items:
  | K-027 | Constant table | P | com.h:846-866 | S: 05 |
  | K-028 | m4 dual face | P | com.h:859-864 | S: 05 §1.1 |
  | K-029 | Grant copy | M | device.c:239-240 | S: 05 §2.2 |
  | K-030 | do_reply shapes | M | device.c:213-219,270 | S: 05 |
  | K-031 | RS-only gate | I | bind.c:14,63 | S: 05 §2.4 |
  | K-032 | Fall-through verdict A-3 | A | main.c:46-58 | S: 05 |
  | K-033 | Unimplemented A-6 | I | com.h:850-856 + :404-418 | S: 05 |
- Acceptance: reader can decode any of the 4 messages' words by role; can
  state the fall-through consequence (ADD→DEL+2×EPERM) and the fix; can
  list the 5 dead messages and the fail-closed rule.

### 06-event-buf

- One-line positioning: the read half — buf cursors, queue direction,
  two-read ACK, static reads, hook wiring. Format authority for event lines.
- Covers: K-034, K-035, K-036, K-037 (authority), K-038, K-039.
- Explicitly not covers: event production business (→ 07/08 call `push`);
  devmand parsing (→ 13); attribute content management (→ 07).
- Prerequisites: 01 (read_hook site), 02 (chunk loop), 03 (Event shape),
  05 (id-suffix budget -11).
- Post-position: 07/08 (producers), 13 (consumer checklist target), 04
  (path supplier).
- Ground truth: `buf.c` (129, all) + `device.c:75-183`; Rust `buf.rs` +
  `event_queue.rs` + `InodeContent`.
- Knowledge items:
  | K-034 | skip/left/used | M | buf.c:8-84 | S: 06 §2.1 |
  | K-035 | HEAD-in/TAIL-out FIFO | I | device.c:96,130 vs 150 | S: 06 |
  | K-036 | Two-read ACK | M | device.c:142-168 | S: 06 §1 |
  | K-037 | Line format + budget | P | device.c:75-136 | S: 06 (authority) |
  | K-038 | Static read +\n | M | device.c:173-183 | S: 06 |
  | K-039 | read_hook wiring | M | main.c:60-68 | S: 01/06 |
- Acceptance: reader can simulate the two-read consume on a 2-event queue
  and show when freeing happens; can write the exact line grammar including
  the 128 cap and prefix accounting; "consumer constraint" subsection names
  the devmand parser lines pinning this output.

### 07-devm-add-device

- One-line positioning: life's beginning — the ADD 8-step flow worked in
  full; the representative member for the handler group.
- Covers: K-040, K-041, K-042, K-043, K-044.
- Explicitly not covers: grant-copy kernel primitive (→ 05 trust root);
  delete/unbind (→ 08/09); client encode (→ 10); event consumption (→ 13).
- Prerequisites: 04 (insert/path/alloc), 05 (grant + reply), 06 (push +
  file registration), 03 (wire decode).
- Post-position: 08 (deletes what this creates; refcount birth value),
  09 (owner source), 13 (event consumer).
- Ground truth: `device.c:223-277` + `:314-397` + `:404-418`; Rust
  `add_device.rs`.
- Knowledge items:
  | K-040 | 8-step flow | M | device.c:223-277,345-397 | S: 07 |
  | K-041 | Claim triple | I | device.c:228-256 | S: 07 §2.1 |
  | K-042 | devman_id file | P | device.c:393-394 | S: 07 |
  | K-043 | STATIC-only loop | I | device.c:404-418 | S: 07 |
  | K-044 | Failure rollback | M | DM-P1-1/P1-4 | N: add_device.rs |
- Acceptance: reader can execute the 8 steps with per-step C evidence and
  Rust landing; can state which three failures leave zero residue and why
  allocation moved after fallible steps; order rationale (claim→settle→
  broadcast) explicit.

### 08-devm-del-device

- One-line positioning: life's end — DEL + refcount + ZOMBIE + reap, with
  a difference table against 07 (mirror order, shared primitives).
- Covers: K-045, K-046, K-047, K-048.
- Explicitly not covers: BOUND enter/exit detail (→ 09 owns bound
  transitions); REMOVE-line consumption (→ 13); grant/reply transport
  (→ 05).
- Prerequisites: 04 (tree mutation), 06 (REMOVE line + file unregister),
  07 (birth refcount 1), 05 (ENODEV reply).
- Post-position: 09 (bind-state interplay), 14 (reap in E2E chain).
- Ground truth: `device.c:424-455` + `:460-480` + `:485-515`; Rust
  `del_device.rs` (+ 02/03/04/06 additive extensions).
- Knowledge items:
  | K-045 | DEL flow | M | device.c:424-455 | S: 08 |
  | K-046 | get/put quartet | M | device.c:460-480 | S: 08 |
  | K-047 | ZOMBIE | C | device.c:443-445 | S: 08 |
  | K-048 | Reap 8 steps | M | device.c:485-515 | S: 08 |
- Acceptance: difference table vs 07 filled (order mirrored and why:
  path needs a live tree); multi-child/multi-bind ledger balanced in an
  example; ZOMBIE emergence condition stated as one-way (BOUND only).

### 09-devm-bind-unbind

- One-line positioning: readiness — the three-party handshake and its two
  tolerances. Assembly material moved to 14 (O-1).
- Covers: K-049, K-050 (handshake only).
- Explicitly not covers: client bind_cb bodies (→ 10/11); RS publish
  rationale (→ 12); transport send (→ transport layer, 14 backlog);
  server-loop assembly (→ 14).
- Prerequisites: 05 (gate + reply + result phases), 08 (put cascade +
  pairing), 07 (owner source). Forward pointer to 10 (response deepening;
  no dependence — driver reply treated as opaque OK/error here).
- Post-position: 12 (RS side of the same handshake), 10 (response side),
  14 (loop that routes here).
- Ground truth: `bind.c` (105, all); Rust `bind.rs` (do_bind/do_unbind +
  answer halves, minus Server assembly).
- Knowledge items:
  | K-049 | 4-step handshake | M | bind.c:7-55 | S: 09 |
  | K-050 | 19-tolerance/forced-OK/ZOMBIE-guard | I | bind.c:85,93,89-91 | S: 09 |
- Acceptance: journey figure's binding leg drawn end-to-end (RS→devman→
  driver→devman→RS); reader can state all three refusal modes (EPERM
  silent / ENODEV / driver-error) and the two tolerances without opening
  10 or 14.

### 10-libdevman-client

- One-line positioning: the mirror half — how a driver encodes, sends,
  tracks, and answers.
- Covers: K-052, K-053, K-054, K-055, K-069 (discipline statement).
- Explicitly not covers: USB modeling (→ 11); server behavior (→ 01..09);
  DS implementation (interface only, injected in tests).
- Prerequisites: 05 (phase table + EPERM-silent client mirror), 03 (layout
  mirrored), 09 (forwarded half of the conversation).
- Post-position: 11 (builds on add/del/callbacks), 14 (production
  ClientTransport backlog).
- Ground truth: `lib/libdevman/generic.c` (275, all) + `local.h` (27,
  all); Rust `devman_client.rs`.
- Knowledge items:
  | K-052 | Codec mirror | P | generic.c:24-99 | S: 10 |
  | K-053 | Client lifecycle | M | generic.c:102-275 | S: 10 |
  | K-054 | Panic policy → Result | I | 5 panic sites | S: 10 §2.3 |
  | K-055 | do_bind/do_unbind response | M | generic.c:207-256 | S: 10 |
  | K-069 | Triple-change discipline | E | encode vs parse vs layout | S: 10 §1 |
- Acceptance: reader can trace init→add→wait→del with the exact message
  per transition; can state the triple-mirror rule and where each mirror
  lives; panic→Result mapping table complete.

### 11-usb-device-model

- One-line positioning: the registration source — USB descriptors into text
  attributes, device+interfaces, callback wiring, remove/delete split.
- Covers: K-056, K-057, K-058.
- Explicitly not covers: descriptor decoding itself (assumed done,
  `minix-usb` stub); server receipt (→ 07); devmand matching detail (→ 13).
- Prerequisites: 10 (add/del + callback signatures + transport injection).
- Post-position: 13 (attribute consumer), 14 (E2E example uses USB attrs).
- Ground truth: `lib/libdevman/usb.c` (301, all) +
  `minix/devman.h:22-69`; Rust `usb_model.rs`.
- Knowledge items:
  | K-056 | Attribute spelling | P | usb.c:24-141 | S: 11 §2.1 |
  | K-057 | Device + N interfaces | C | usb.c:219-274 | S: 11 §1 |
  | K-058 | Remove/delete split | M | usb.c:276-293,174-193 | S: 11 |
- Acceptance: attribute order (TAIL-append, dev_type last) locked with the
  devmand-regex correspondence noted per string; `(id, iface|-1)` routing
  explained; add/remove symmetry shown as an N+1 ledger.

### 12-rs-integration

- One-line positioning: both ends of process life on the RS side —
  publish/unpublish handshake, id inheritance, launch permissions, failure
  meaning.
- Covers: K-059, K-060.
- Explicitly not covers: RS internals (slot/kill/DS implementations);
  driver behavior (→ 10/11); devmand (→ 13).
- Prerequisites: 09 (devman side of forwarding), 05 (RS-only gate +
  ENDPOINT prefill).
- Post-position: 14 (RS transport production backlog), 09 (handshake
  counterpart).
- Ground truth: `manager.c:840-851,897-909,1742` + `rs.h:139,182` +
  `system.conf:422-429`; Rust `rs_contract.rs`.
- Knowledge items:
  | K-059 | Publish/unpublish handshake | M | manager.c sites | S: 12 |
  | K-060 | devman_id three stations | C | inherit→consume→release | S: 12 |
- Acceptance: reader can state the exact words RS fills and the
  kill-vs-log asymmetry with its rationale (publish failure = broken
  service; unpublish failure = best-effort cleanup); permission triple
  (uid 0 + 2 VM privs) quoted.

### 13-devmand-consumer

- One-line positioning: the first reader's contract — parse, judge, match,
  start/stop, scripts. External: documented, never implemented.
- Covers: K-061, K-062, K-063.
- Explicitly not covers: server internals (→ 01..09); RS flow (→ 12);
  driver implementations (usbd et al.); yacc/lex internals beyond the
  match contract.
- Prerequisites: 06 (line format), 04 (paths), 11 (attribute spelling),
  05 (-11 budget).
- Post-position: 14 (E2E chain uses devmand as the reader), 06/11
  (contract reverses onto producers).
- Ground truth: `commands/devmand/main.c` (942) + `usb.y` (134) +
  `usb_scan.l` (43) + `usb_driver.h` + `etc/devmand/` configs/scripts.
- Knowledge items:
  | K-061 | Line parsing | M | main.c:803-872,876-932 | S: 13 §2.1 |
  | K-062 | Type + USB-id synthesis | M | main.c:519-587,633-680 | S: 13 |
  | K-063 | DSL match + start/stop + scripts + major + pid | P | usb.y/scan.l; main.c:84-231,592-628,424-431 | S: 13 |
- Acceptance: contract tables (not a manual): for each devman output
  (event line, dev_type, 8 attrs, devman_id, /sys paths) the exact parser
  expectation; build files (`Makefile`, `devmand.cfg`) inventoried;
  "not implemented" banner with the plan §5.5 pointer.

### 14-server-assembly (NEW)

- One-line positioning: the wiring that turns the semantic library into a
  running server — unified loop, reply execution, transport seams and
  their backlog, the no-shutdown verdict, E2E acceptance.
- Covers: K-051, K-065, K-066, K-067.
- Explicitly not covers: handler business (→ 07/08/09); framework
  data-plane (→ 02); message verdicts (→ 05); production transport
  implementation itself (blocked on E1/E2/DS — this chapter contracts the
  shape, edge_todo executes it).
- Prerequisites: 01 (park status + SEF), 02 (VTreeFs::run side), 05
  (DevmanMsg + reply primitives), 09 (handle_other arms' business).
- Post-position: 99 (seam constants registered); edge_todo E-DMWIRE
  (execution pointer, both directions).
- Ground truth: Rust `server.rs:91-170` (handle_other), `vtreefs/mod.rs:
  322-365` (run + err_marker removal), `main.rs:35-37` (park),
  `hooks.rs:255-271` (SefHooks → DevmanSef production, E-ISWIRE),
  `ipc/message.rs:64-75` (apply_reply_with_id), C `fsdriver_task` table
  (single-loop reference); E-DMWIRE/E-REQWIRE/E-ISWIRE/E-DSWIRE/E5(h).
- Knowledge items:
  | K-051 | Unified run + OutAction + DevmanMsg | M | server.rs + DM-P1-2/P2-1/P2-2 | S: 09 assembly half (moved) |
  | K-065 | No-shutdown verdict | I | devmand open-fail + kill_service; zero cleanup in servers/devman/ | N: new verdict |
  | K-066 | Single-threaded invariant | I | lib.rs:8-14; unsafe=0 | S: scattered (consolidated) |
  | K-067 | Transport backlog (4 seams) | T | E-DMWIRE four items | N: edge pointer |
- Raw material: stock assembly prose from 09's second half + 02's
  Transport section + 01's park/SEF notes (moved, not copied — rewritten
  to the loop-first narrative); new verdicts K-065/K-067 written from
  anchors above.
- Acceptance: reader can narrate mount→register→event→bind→delete through
  ONE loop function; each of the 4 transport seams has shape + blocked-on
  + acceptance test; no-shutdown stated with both-sided evidence (no
  server cleanup × consumer open-fail exit); E2E chain E5(h) four links
  mapped to chapters.

### 99-devm-global-concepts

- One-line positioning: the catch-all — numbers, codes, neighbors, global  state, dead-code verdicts. Registers, never invents mechanism.
- Covers: K-027 (index entry → 05), K-031 (index → 05), K-068, K-064
  (verdict home per D-2), global state (next_device_id/root_dev/event
  statics), cross-service map (RS/VFS/DS/usbd/devmand/minix-service).
- Explicitly not covers: any mechanism detail (→ 01..14; new mechanism
  here = this chapter overreaching, bounce to owner).
- Prerequisites: all (00..14).
- Post-position: none (terminal reference).
- Ground truth: `com.h:846-866` + `devman.h:39-51` + `system.conf:422-429`
  + `rs.h` + per-chapter line numbers; Rust `minix-types` (com.rs DEVMAN
  block + Errno) + per-module states/consts.
- Knowledge items:
  | K-068 | Errno table | A | handler sites | S: 99 |
  | K-064 | Dead verdicts (home) | I | grep definition-only | S: 99 (D-2) |
  | K-027/031 | Constant/gate index | P/I | com.h; bind.c | S: 99 (index → 05) |
- Acceptance: every constant/error/neighbor in the stage resolves to
  exactly one owning chapter; dead items listed with keep/drop ruling;
  no paragraph here that belongs in 01..14 (bounce test).

---

## 6. Change Table (old → new, one row per operation)

| Op # | Operation | Old position | New position | Reason | Knowledge items | Disposition |
|------|-----------|--------------|--------------|--------|-----------------|-------------|
| C-01 | Retain | 00 (all sections) | new-00 (same number/title) | Map chapter already satisfies single-unit + no-forward-ref; only needs route updates (journey path gains 14 as loop home; server-builder path added). | K-001, K-002, K-070 | Identity; §4 route table refreshed. |
| C-02 | Retain + trim | 01 (all sections) | new-01 | Temporal anchor correct; trim = fall-through verdict language → 3-line phenomenon box (D-1); guard stays (D-6 authority). | K-002..K-007 | Verdict sentences moved to new-05 (rewrite, not cut-paste). |
| C-03 | Retain | 02 (all sections) | new-02 | Framework contract is one semantic unit; O-2 cut made explicit. | K-008..K-012 | Identity + cut statement. |
| C-04 | Retain + trim | 03 (all sections) | new-03 | Shape base correct; dead-verdict paragraphs shrink to pointers → 99 (D-2); triple-mirror pointer → 10 (D-4). | K-013..K-017, K-019, K-020, K-064 | Pointers rewritten, layout prose untouched. |
| C-05 | Retain + trim | 04 (all sections) | new-04 | Tree ops correct; guard paragraph shrinks to pointer → 01 (D-6). | K-021..K-026 | Pointer rewritten. |
| C-06 | Retain + absorb | 05 + verdict sentences from 01 | new-05 | Protocol surface is the converge chapter; absorbs the A-3/A-6 verdict monopoly (D-1). | K-027..K-033 | Verdict monopoly declared; 01/09/12 cite. |
| C-07 | Retain (authority declared) | 06 (all sections) | new-06 | Read half is one unit; declared format authority (D-3) + consumer-constraint subsections added. | K-034..K-039 | Format tables stay; back-refs from 07/08/13 added. |
| C-08 | Retain | 07 (all sections) | new-07 (representative member) | ADD fully worked; group template for 08/09 difference tables. | K-040..K-044 | Identity + group-role banner. |
| C-09 | Retain | 08 (all sections) | new-08 (difference table vs 07 added) | DEL mirrors ADD; the mirror rationale made explicit. | K-045..K-048 | Difference table added. |
| C-10 | **Split** | 09 handshake half (§1 concept + bind/unbind mechanism) | new-09 (same number/title, handshake only) | Single-unit rule: handshake ≠ assembly. | K-049, K-050 | Mechanism prose stays. |
| C-11 | **Split (move)** | 09 assembly half (Server/handle_other/OutAction/DevmanMsg/run-unification) | new-14 §§2-3 | O-1: assembly needs its own contract (transport seams, backlog, E2E). | K-051 | Rewritten loop-first, not relocated verbatim. |
| C-12 | Retain | 10 (all sections) | new-10 | Mirror half is one unit; triple-discipline statement kept (D-4). | K-052..K-055, K-069 | Identity. |
| C-13 | Retain | 11 (all sections) | new-11 (skippable branch declared) | USB family is one unit; branch status declared. | K-056..K-058 | Identity + branch banner. |
| C-14 | Retain | 12 (all sections) | new-12 | Registrar contract is one unit. | K-059, K-060 | Identity. |
| C-15 | Retain | 13 (all sections) | new-13 (contract-checklist shape confirmed) | Consumer contract complete; yacc/lex stay at match-contract depth. | K-061..K-063 | Identity + not-implemented banner kept. |
| C-16 | **New** | — (raw: G-1..G-4; 09 assembly half; scattered invariant notes) | new-14 `14-server-assembly.md` | Assembly/backlog has no home; invariant + no-shutdown need single authority. | K-051, K-065, K-066, K-067 | Written from contracts (§5.14); §7 details acceptance. |
| C-17 | Retain + absorb | 99 + dead-verdict paragraphs from 03/05 | new-99 (verdict home) | D-2 monopoly: one verdict table for all dead items. | K-064, K-068 (+ K-027/031 index) | Verdict table; 03/05 point here. |
| C-18 | Archive (no delete) | draft/README.md (already archived) | stays in draft/ | Placeholder material, history only. | — | No action. |
| C-19 | Reference-only | plan.md / todo.md / README.md | unchanged roles | plan = coverage baseline (frozen 2026-08-16, historical — new-14 deltas recorded as addendum, plan itself not rewritten); todo = closed ledger; README = index (add new-14 row + update test counts). | — | README gains one row; plan/todo untouched. |

Stock-direction check (splits/merges must account for every stock item):
C-10/C-11 split covers all of old-09's sections (handshake §§1-4 → new-09;
`Server`/loop/`OutAction`/transport §§ → new-14); no other chapter is split
or merged, so no other stock item needs a disposition. New-direction check
(new items need evidence, not disposition): K-065 (devmand open-fail +
kill_service + zero-cleanup grep), K-067 (E-DMWIRE four items), K-044
(DM-P1-1/P1-4 fixes) all carry anchors in §2.

---

## 7. Missing-New-Chapter Resolution (no "TBD" allowed)

| Item | Verdict |
|------|---------|
| New-14 server-assembly | **Create** (`14-server-assembly.md`, contract §5.14). Raw material: old-09 assembly half + 02 Transport section + 01 park/SEF notes + G-1..G-4 verdicts. Acceptance: §5.14 items. |
| Non-C checklist (10 items) | **Resolved without new chapters**: 5 taught in place (01 build+boot-import, 05/03/10 wire, 06/13 event lines, 99 codes, 14 shutdown/concurrency/tests), 3 explicitly excluded with reasons (image layout, asm/trap, full-framework surface — see §3.5). |
| Shutdown story | **No new chapter**: closed subsection in new-14 (K-065). |
| Concurrency story | **No new chapter**: consolidated section in new-14 (K-066). |
| Test infrastructure | **No new chapter**: per-chapter §5 convention + new-14 E2E acceptance (E5(h)). |
| USB/RS/devmand families | **No new chapters**: 11/12/13 each already single-unit; USB non-device families (if any future driver needs them) belong to 16-stage-drivers, not here. |

---

## 8. Anchor Migration & Breakage Cost

### 8.1 Section-migration table (only changed chapter: 09 → 09 + 14)

| Old position | Old content (one line) | New position | Migration type | Breakage note |
|--------------|------------------------|--------------|----------------|---------------|
| 09 §1 (three-party handshake concept + journey leg figure) | Binding as brokerage | new-09 §1 | Verbatim keep | None (same number). |
| 09 §§2-3 (do_bind/do_unbind walkthrough + 19-tolerance) | Handshake mechanism | new-09 §§2-3 | Verbatim keep | None. |
| 09 §4 (Server/handle_other/OutAction/assembly) | Assembly | new-14 §§2-3 | **Rewrite** (loop-first narrative) | Old §4 anchors cited from new-14 once, then retired. |
| 09 §5 (tests for assembly) | Assembly tests | new-14 §5 | Move + extend (seam acceptance added) | Test names unchanged (no code move required). |
| 09 §6 (transition/journey close) | Journey close | new-09 §6 (kept) + new-14 §6 (loop close) | Split close | Both closes point at each other. |
| 01 fall-through verdict sentences | Verdict language | new-05 §2 (A-3) | Rewrite as cite | 01 keeps phenomenon box. |
| 03/05 dead-code verdict paragraphs | Verdicts | new-99 verdict table | Condense to row + pointer | Full prose lives once. |
| 04 guard paragraph | Guard mechanism | new-01 (authority) | Condense to pointer | Birth content stays in 04. |
| 00 route table | 15-chapter routes | new-00 §4 (16 chapters + 3 paths) | Extend | Additive only. |
| README index table | 15 rows | 16 rows (add 14) + test-count refresh | Extend | Additive only. |

All other sections in 00-08, 10-13, 99: **identity** (no migration rows needed;
in-chapter pointer insertions per D-1..D-8 are Phase-B edits, not moves).

### 8.2 Reference-migration table

| Referrer | Old reference | New target | Verification |
|----------|---------------|------------|--------------|
| 22 Rust files (`//! doc NN-…` headers, §0 count) | `doc 09-devm-bind-unbind` (assembly files: `server.rs`, `ipc/*`) | Unchanged for handshake files (`bind.rs`); `server.rs`/`ipc/*` headers gain `+ doc 14-server-assembly` (one-line header edit, Phase B) | `rg "doc 14-server-assembly" os/` non-empty post-B |
| Intra-stage cross-refs (every chapter cites 2–6 siblings) | References to old-09 §4 (assembly) | Retarget to new-14 §§2-3 | `rg "09-devm-bind-unbind §4" notes/.../11-stage-devman/` empty post-B |
| `16-stage-drivers/12-gpio-devman.md` | devman client contract (10/11 area) | Unchanged (10/11 numbers stable) | No edit needed |
| `16-stage-drivers/plan.md`, `18-stage-commands/plan.md`, `15-stage-fs/plan.md`, `12-stage-input/plan.md`, `edge_todo.md`, `edge3.md`, master-plan README | Stage-level pointers (`11-stage-devman`, map/table rows) | Unchanged (no renumber, no rename) | No edit needed |
| plan.md §5.3/§3.4 (function→chapter map, boundary table) | 09 owns `Server` assembly implicitly | Addendum line: assembly → new-14 (plan itself frozen; addendum appended, not rewritten) | Addendum present post-B |

### 8.3 Breakage-cost summary

- Affected references total: ~30 intra-stage section links to old-09 §4/class
  assembly prose (upper bound; exact count = Phase-B `rg` per §8.2 row 2) +
  2 Rust file headers (`server.rs`, `ipc/*` module headers) + README 1 row +
  plan addendum 1 line. **All other anchors (20 file headers, all external
  stage pointers, all 10/11/12/13 contract links) are byte-stable.**
- Hotspot: old-09 §4 (sole content hotspot); no filename hotspot (zero
  renames).
- Batch method: (1) `rg "09-devm-bind-unbind" notes/ os/` → retarget assembly
  hits to `14-server-assembly`; (2) header-line patch on `server.rs`,
  `ipc/mod.rs`, `ipc/message.rs`, `ipc/dispatch.rs`; (3) README + plan
  addendum. Verify with the two `rg` checks in §8.2 (empty/non-empty).
- Cost verdict: one-appendix rebuild — two orders of magnitude below a
  renumber rebuild, with zero filename churn. This is why §4 decision 1
  (numbers stable) holds.

---

## 9. Verification, Self-Gates, Conclusions

### 9.1 Four mechanical checks (§5.4 of the prompt)

1. **Forward-reference scan** (per new-chapter "Prerequisites" fields):
   00→∅; 01→{00}; 02→{01}; 03→{02}; 04→{03,02,01}; 05→{01,03}; 06→{01,02,03,
   05}; 07→{04,05,06,03}; 08→{04,06,07,05}; 09→{05,08,07} (+forward *pointer*
   to 10: allowed — response deepening, not dependence; driver reply treated
   as opaque here); 10→{05,03,09}; 11→{10}; 12→{09,05}; 13→{06,04,11,05};
   14→{01,02,05,09}; 99→{all}. Every prerequisite number is smaller than the
   chapter, except 10→09 (backward: 9<10 ✓) and the declared 09⇢10 pointer
   (forward pointer, zero dependence — verified: 09's contract never names a
   10-owned symbol). **PASS** (with the one declared pointer).
2. **Dependency graph**: edges above; linear backbone 00..09 + side branches
   10/11, 12/13, terminal 14/99. No cycles (branch chapters never point back
   into unfinished backbone topics; 14 points only backward; 99 terminal).
   **PASS**, no decomposition needed.
3. **Coverage**: all 70 pool items have dispositions (§6 table: every K-ID
   appears in exactly one new chapter's "Covers" list — verified while
   writing §5; deletion set = ∅, nothing deleted). New items K-044/K-065/
   K-066-record/K-067/K-069-record carry evidence anchors. **PASS.**
4. **Breakage census**: §8.3 totals + hotspots + batch method. **PASS**
   (counted, not estimated-away).

### 9.2 Self-gates G1–G9

| Gate | Result |
|------|--------|
| G1 C true order checkable (10 random anchors) | PASS. Spot-verified: `main.c:89` (run call), `mount.c:24-25` (init_hook), `main.c:46-58` (no-break switch), `device.c:239-240` (grant copy), `device.c:371` (id alloc), `device.c:96/:150` (HEAD-in/TAIL-out), `device.c:142-168` (two-read ACK), `bind.c:14` (RS gate), `bind.c:85` (19-tolerance), `generic.c:112-122` (grant+sendrec). All read in full during this session. |
| G2 Pool completeness (every C file / non-C artifact has a home or explicit exclusion) | PASS. Server 4 .c + 3 .h → 01/03/04/05/06/07/08/09/14; libdevman 2 .c + 1 .h → 10/11; vtreefs used surface → 02; com.h/devman.h → 05/03/99; RS sites → 12; system.conf → 01/12; devmand + cfgs + scripts → 13 (+build note); Makefiles → 01/13 one-liners; link/asm/image-layout → explicit exclusions §3.5. |
| G3 Zero forward references | PASS with one declared forward *pointer* (09⇢10, dependence-free — see §9.1 check 1). |
| G4 Acyclicity | PASS (backbone + backward-only branches; §9.1 check 2). |
| G5 100% coverage incl. new-item anchors + explicit deletion list | PASS (70/70 placed; deletion list empty by design — nothing removed). |
| G6 Splits show stock disposition; new chapters show new sources (10-spot check) | PASS. The only split (09→09+14): C-10/C-11 rows account for every old-09 section (§8.1); new-14 sources listed (§5.14 raw-material line). Other chapters un-split → no disposition owed. |
| G7 Every contract has 7 elements | PASS (16/16 contracts in §5 carry positioning/covers/not-covers/prerequisites/post-position/ground-truth/items/acceptance). |
| G8 Migration covers all changed sections + doc/code references | PASS (§8.1 all changed sections; §8.2 all reference classes incl. code headers). |
| G9 Factual claims anchored (10-spot check; speculation labeled) | PASS. All C-behavior claims cite file:line read this session. Two speculation-class statements, both labeled: (a) §1.3 OD-1..OD-4 "reasons" are pedagogical judgments (mine), not C facts; (b) §4 decision rationale "zero pedagogical gain" is a judgment. No unlabeled speculation. |

### 9.3 Conclusions

1. **Rebuild shape**: append-only — 15 chapters stable, 1 chapter added
   (new-14 server-assembly), 1 chapter split (09), 0 archives, 0 renumbers.
   Phase B workload: 1 new chapter + 09 §4 rewrite-as-move + pointer
   insertions (D-1..D-8) + README/plan-addendum/header batch (§8.2).
2. **The stage's semantic surface is complete; its wiring surface is not.**
   The audit independently reproduces todo.md's verdict: all device-model
   knowledge has a home; only assembly/backlog topics (G-1..G-4) needed a
   new chapter.
3. **Order-difference debt is paid explicitly**: OD-1..OD-4 record every
   teaching-vs-runtime inversion with reason + compensation, so Phase B
   never has to re-litigate sequence.

### 9.4 Questions for user decision (nothing here blocks Phase-B start)

1. New-14 filename `14-server-assembly.md` continues the numeric series but
   sits outside the lifecycle order — acceptable, or should it be named to
   signal appendix status (e.g. `14-…` kept, with the map marking group H
   as off-mainline)? AI recommendation: keep `14-` + group-H marking (map
   already signals it); renaming to 98/99-family would collide with 99's
   catch-all role.
2. OD-3/09⇢10 forward pointer: acceptable as contracted (09 treats driver
   reply as opaque), or should the client chapters move before bind
   (10/11 ahead of 09)? AI recommendation: keep server-first (stage
   ownership + anchor stability outweigh journey purity; journey path
   covers the runtime-order reading).
3. plan.md treatment: frozen + addendum (this blueprint's recommendation,
   §6 C-19) vs reopening plan §5.3 to register new-14 natively. AI
   recommendation: addendum (plan is a dated finalized record; rewriting
   history costs more than it clarifies).



