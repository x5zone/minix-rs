# 16-stage-drivers Rebuild Blueprint (muse)

> Execution header (Section 1 of the report, per R-phase prompt).
>
> ```text
> your_name(AI agent name) = muse
> target_dir(working directory) = 16-stage-drivers
>   (full path: notes/rewrite/fork-syscall-rewrite/16-stage-drivers/)
> repo_root(repository root) = /home/xzhao/github/minix-rs
> Task = R-phase rebuild blueprint: produce target_dir/doc_rerank_muse.md only.
>   No body text is modified. Besides C sources, this blueprint also reads: all
>   doc headers in this directory, the os/ entry points, and the 00-*-overview
>   navigation tables.
> Constraints = No references to .design/ or tmp_design_and_todo/; every artifact
>   written to the repo carries the _muse suffix; no reading or copying of other
>   AIs' doc_rerank_* products (doc_rerank_deepseek/glm/qwen present in the
>   directory were NOT opened).
> Output language = English (muse special requirement).
> Date = 2026-09-19. HEAD at writing time = bdc3a0ac8.
> ```

This document is self-contained: it assumes no knowledge of any chat process and
answers four questions — (1) what is this stage's true runtime order, rebuilt
directly from C sources; (2) what knowledge the existing docs carry, deduplicated
into one stage-wide pool; (3) what the rebuilt catalog is, with a contract per
doc; (4) how old maps to new, with anchor migration and breakage cost. Phase B
can execute it without further judgment calls.

Method note (why rebuild, not move): the current catalog is a 27-node web in
which every move re-opens boundary, prerequisite, cross-reference, and code-
comment questions. Rebuilding fixes a new coordinate system first; each new doc
then takes only its contracted knowledge from the pool plus C sources and is
rewritten in the new teaching order. Old docs serve as knowledge sources only.
The contract style follows `01-stage-kernel/06-todo.md` (what a doc teaches /
does not teach / defers to whom / acceptance), not its content.

---

## 0. Metadata

### 0.1 Scope

- **In scope (numbered docs, 27 files):** `00-drivers-overview.md`,
  `01-chardriver-framework.md`, `02-blockdriver-framework.md`,
  `03-netdriver-framework.md`, `04-bdev-client.md`, `05-memory-driver.md`,
  `06-tty-driver.md`, `07-pty-driver.md`, `08-log-driver.md`,
  `09-random-driver.md`, `10-readclock-driver.md`, `11-pci-driver.md`,
  `12-gpio-devman.md`, `13-pckbd-driver.md`, `14-virtio-framework.md`,
  `15-virtio-blk-driver.md`, `16-ahci-ata-driver.md`,
  `17-storage-misc-driver.md`, `18-usb-framework.md`, `19-usb-storage-hub.md`,
  `20-fb-driver.md`, `21-audio-drivers.md`, `22-net-driver-reference.md`,
  `23-net-driver-variants.md`, `24-misc-drivers.md`,
  `25-dma-memory-contract.md`, `99-global-concepts.md`.
- **Reference material (not rebuilt):** `plan.md` (coverage contract, ARCH list),
  `todo.md` (2026-09-17 full code scan: F/B/S/V/N/G/A fix records),
  `draft/README.md`, `edge_todo.md` entries
  E-CDRCONV/E-PCKBDREG/E-DEVWIRE/E-SDEVOWN/E-DMABUF (cross-stage dependencies).
- **Out of scope:** `.design/`, `draft/` beyond README, other AIs'
  `doc_rerank_*` files, all other stage directories (referenced only as
  deferral targets).
- **Late discovery:** none outside the directory that changes the scope; the
  newest doc (`25-dma-memory-contract.md`, edge E-DMABUF fuel contract) is in
  scope and is itself a rebuild input (see §3 loss/gap analysis).

### 0.2 Reading list (all read before concluding)

1. All 27 docs' headers (About/Prerequisites/Not-covered/One-question) — full
   headers read; bodies sampled by section (`##` structure verified for all 24
   numbered docs: uniform Ch1 concept / Ch2 C analysis / Ch3 Rust decision /
   Ch4 errors / Ch5 tests / Ch6 transition / Ch7 references).
2. `plan.md` §§1–7 (main line, 26-doc table, ARCH A-1..A-13, C→doc map §5.1,
   headers §5.2, protocols §5.3, exclusion §5.4, review record).
3. `todo.md` §§0–4 (31 policy crates wired via `char_face`/`block_face`, 6
   framework libs, F/B/S/V/N/G/A records; A2 wiring progress; G5 00/99
   expansion; G7 doc-overclaim list).
4. Minix3 C ground truth: `minix/kernel/table.c:44-64` (boot image),
   `minix/lib/lib{chardriver,blockdriver,netdriver,bdev,virtio,usb,
   audiodriver,i2cdriver,inputdriver,devman}/`, `minix/drivers/` (storage,
   tty, system, clock, bus, hid, usb, video, audio, net + misc families),
   `minix/include/minix/com.h` (protocol constants), `chardriver.h`,
   `blockdriver.h`, `netdriver.h`, `driver.h`, `bdev.h`, `virtio.h`,
   `usb.h`, `devman.h`, `audio_fw.h`, `i2cdriver.h`, `inputdriver.h`,
   `minix/etc/system.conf` + `minix/etc/rc.minix` (service bring-up order).
5. Non-C artifacts: per-driver `Makefile`s, net `*.conf` files, `ramdisk/`
   (proto/rc, no C files), `etc/system.conf` service entries, `etc/rc.minix`
   (`up readclock.drv`, usbd handling).
6. Boundary materials: `00-master-plan/README.md` + roadmap (stage
  划分因果), `edge_todo.md` (E-DEVWIRE/E-SDEVOWN/E-DMABUF/E-CDRCONV/
   E-PCKBDREG excerpts read via grep), `15-stage-fs/00-fs-overview.md`
   (previous-stage style reference: framework→sample→variants line, and the
   ptyfs/VTreeFS boundaries this stage must not re-teach).
7. Rust entries: `os/libs/minix-{chardriver,blockdriver,netdriver,bdev,
   virtio,usb,audiodriver,i2cdriver,driver-rt,types}` and 22 wired
   `char_face`/`block_face` files under `os/drivers/` (existence + shape
   verified by find; bodies not exhaustively audited — faces are evidence of
   *wiring*, C remains behavior authority).
8. Style model: `01-stage-kernel/06-todo.md` §§1–3 (contract writing only).

### 0.3 Commands and key outputs (evidence excerpts)

- `wc -l` over the 27 docs + plan + todo: 6388 lines total; numbered docs
  180–308 lines each; `00` 67, `99` 76, `25` 74 lines.
- `ls minix/minix/drivers/`: audio, bus, clock, eeprom, examples, hid, iommu,
  net, power, printer, sensors, storage, system, tty, usb, video, vmm_guest.
- `sed -n '44,64p' minix/minix/kernel/table.c`: boot image carries exactly
  two drivers — `MEM_PROC_NR "memory"`, `TTY_PROC_NR "tty"`.
- `sed -n '1,60p' minix/include/minix/chardriver.h`: `struct chardriver` has
  10 hooks (open/close/read/write/ioctl/cancel/select/intr/alarm/other) +
  `chardriver_task/process/announce/terminate`, `reply_task`, `reply_select`.
- `sed -n '20,45p' minix/include/minix/netdriver.h`: `struct netdriver` =
  name + 13 function pointers (`ndr_init/stop/set_mode/set_caps/set_flags/
  set_media/set_hwaddr/recv/send/get_link/intr/tick/other`). This falsifies
  plan.md's `ndo_` prefix (doc 22 already corrects it; plan §2 still stale).
- `com.h` ranges verified: USB `0x1100` (813–841), CDEV RQ `0x400` / RS
  `0x480` (919–956), BDEV RQ `0x500` / RS `0x580` (963–987), RTCDEV RQ
  `0x1400` / RS `0x1480` + `RTCDEV_Y2KBUG 0x01` (995–1012), NDEV RQ `0x1A00` /
  RS `0x1A80`, 9 caps + 4 flags + 3-state link incl. `NDEV_LINK_DOWN 2`
  (1085–1145). This confirms doc 99's table and the F1/F4/G8 fix records.
- `rg sef_startup|chardriver_task|blockdriver_announce memory.c`,
  `driver_receive tty.c:165`, `sef_receive_status readclock.c:53`,
  `sef_startup pci/main.c:721,728`: every driver = SEF startup → announce →
  receive loop (two shapes: framework `*_task` vs hand-written loop).
- `rg 'service up|readclock|usbd' etc/rc.minix`: `up readclock.drv` (128);
  `system.conf`: `service tty/memory/log/pci` entries — RS-loaded services
  after boot image.
- `find os/drivers os/libs -name 'char_face.rs' -o -name 'block_face.rs'`:
  23 face files across 22 crates (memory has both char+block faces; full list
  in §0.3 command output captured at writing time).
- `md5sum 19-*.md 20-*.md`: differ (same byte size 17325 is coincidence, not
  duplication — verified, no action).
- Code→doc references are coarse (`os/libs/minix-chardriver/src/lib.rs:13`,
  `os/libs/minix-driver-rt/src/lib.rs:20`,
  `os/drivers/tty/tty/src/lib.rs:9` point at the directory or `todo.md` A1,
  not at NN numbers) — renumbering breaks few code comments; doc→doc
  NN references are the real cost (see §8).

---

## 1. C true order

### 1.1 Stage-type判定

This stage is **collection-type with per-member service-event-loop shape**
(prompt §9: drivers/commands rule applies). Treatment: framework doc(s)
first (common message format, dispatch, lifecycle, registration), then groups
by role (boot-critical → system services → input → storage → USB →
display/audio → net → platform/misc), representative member taught in full per
group, rest folded into difference matrices. No fake linear order is imposed
across the 57 servers; inside one server the order follows its lifecycle.

### 1.2 Birth segment (process birth → loop entry)

| # | Action | C anchor | Notes |
|---|--------|----------|-------|
| T-1 | Boot image defines only 2 drivers | `minix/kernel/table.c:58-59` (MEM/TTY) | All other drivers are RS-loaded later |
| T-2 | Services declared with PCI matching | `minix/etc/system.conf` (`service tty/memory/log/pci…`, pci class/device stanzas) | Static config, not C |
| T-3 | Boot scripts bring up clock/USB | `minix/etc/rc.minix:128` (`up readclock.drv`), `:218-220` (usbd) | Runtime ordering evidence |
| T-4 | SEF startup + init callbacks | `drivers/storage/memory/memory.c:113-120`, `clock/readclock/readclock.c:154-164`, `bus/pci/main.c:721-728`, `tty/tty/tty.c:303` | `sef_setcb_init_*` + `sef_startup()` |
| T-5 | Announce / DS publish driver-up | `lib/libchardriver/chardriver.c:99-120` (`chardriver_announce` → `sys_statectl` + get-label + publish) | Same shape per family |
| T-6 | Enter family task loop | `chardriver.c:549-565` (`chardriver_task`: `sef_receive_status(ANY…)` loop); `tty.c:146-165` hand loop via `driver_receive`; `readclock.c:53` own RTCDEV loop | Two loop shapes, one discipline |

### 1.3 Loop segment (message → dispatch → hook → reply-or-park)

| # | Action | C anchor | Notes |
|---|--------|----------|-------|
| L-1 | Receive any + IPC status | `chardriver.c:561`, `tty.c:165`, `readclock.c:53` | Notify vs message split |
| L-2 | Classify by family range | `com.h:922-923` (CDEV), `:965-966` (BDEV), `IS_NDEV_*`, `:998-999` (RTCDEV), USB `0x1100+0..8` (813–828) | Mask `& ~0x7f` per family |
| L-3 | Dispatch to hook table | `chardriver.h` 10 hooks; `blockdriver.h` 11 hooks; `netdriver.h` 13 `ndr_` hooks; `audio_fw.h` 14 hooks | Unset hook → default (reply error / ignore) |
| L-4 | Suspend or reply | `chardriver.c:137-161,200-226` (`EDONTREPLY` park, `SUSPEND` rejected by panic) | Parked reads/selects wait for intr/alarm/notify |
| L-5 | Select two-phase reply | `chardriver.h` (`reply_select`), `com.h:935-937,949-956` (SEL1/SEL2, OPS bits) | Immediate vs late notification |
| L-6 | Interrupt/alarm/other side doors | `cdr_intr/cdr_alarm/cdr_other`, `tty.c:179-194` (notify → `expire_timers`, IRQ set), `usb_hub.c:444-470` (error → suspend whole task) | Lamps-then-work in tty |
| L-7 | Restart/live-update gate | `libblockdriver/liveupdate.c`, `system/log/liveupdate.c`, `filter/driver.c:387-392` + `main.c:28` (3-restart budget → mirror off) | Fail-closed states |
| L-8 | Client-side mirror (block) | `libbdev/ipc.c:144-268` (sendrec, first check `BDEV_REPLY`), `bdev.c/call.c/driver.c/ipc.c/minor.c` | Async slots, minor refcounts, flush/resend |

### 1.4 Device-class plug-in order (causal, not temporal)

pci enumeration (`bus/pci: main.c/pci.c/pci_table.c`) → storage
(virtio_blk → ahci/at_wini → floppy/mmc/fbd/filter/vnd) → hid/pckbd →
input-server bridge → net (NDEV → 17-stage-net lwip/uds) → usb (usbd →
storage/hub) → audio (audiodriver + 7 cards) → fb → misc. Memory+tty form
the minimal bootable closed loop (boot image + `/dev/ram*`+`/dev/imgrd` +
console).

### 1.5 Order-difference table (runtime truth vs teaching order)

| # | Runtime fact (anchor) | Teaching choice | Reason + back-pointer |
|---|-----------------------|-----------------|----------------------|
| D-1 | Drivers boot in config order (table.c + system.conf + rc) | Frameworks + runtime taught before any driver | Common loop/dispatch must be known first; §4 group intros re-anchor T-1..T-6 |
| D-2 | DMA allocation is a boot-time VM service (E-DMABUF) | DMA contract taught early (new-06) with abstract motive | Rings (new-17) need it; contract itself needs no ring detail; worked example forward-points to new-17 |
| D-3 | pci is RS-loaded like others, not boot image | pci taught before storage/net (new-13) | Device discovery is a causal prerequisite; new-13 states load order explicitly |
| D-4 | readclock never enters `chardriver_task` | readclock grouped with system services, not frameworks | Own RTCDEV loop (readclock.c:53); contract states the non-framework dispatch |
| D-5 | pckbd input path bypasses char framework | pckbd after gpio/registration (new-16) | Needs registration + input-server concepts first |

---

## 2. Knowledge pool

Stock = from existing docs (move constrained by §4双方向去向 rule).
New = from C / non-C artifacts / OS theory / wired Rust code the docs do not
yet teach (needs evidence anchor only). Alignment key = name + anchor.
Main telling point recorded for repeats. Types: C=concept, M=mechanism,
D=data structure, I=interface/protocol, K=constraint/invariant, A=arch
evolution, E=engineering/tooling, T=test property.

### 2.1 Runtime + frameworks (new-01..05, 99)

| ID | Name | Type | Src | Current home | Anchor | Reader benefit |
|----|------|------|-----|--------------|--------|----------------|
| K-001 | SEF birth: callbacks → startup → announce → task | M | stock | 01 §2 (chardriver shape), 05 §2, 10 §2 | `chardriver.c:99-120,549-565`; `memory.c:113-162`; `readclock.c:154-164` | Can answer what every driver process does first |
| K-002 | Family classification by masked base | M | stock | 01/02/03 §2, 99 §2.1 | `com.h:922-923,965-966,998-999`; USB 813–828 | Can route any message to its family |
| K-003 | chardriver 10-hook table + defaults | I | stock | 01 §2–3 | `chardriver.h` hooks; `chardriver.c:137-226` | Knows which hook fires for each of 7 CDEV requests |
| K-004 | EDONTREPLY park vs SUSPEND panic | K | stock | 01 §2 | `chardriver.c:200-226` | Knows why parked reads are legal and SUSPEND is not |
| K-005 | Select OPS/STATUS bits + SEL1/SEL2 | I | stock | 01 §2, 06 §2 | `com.h:935-956` | Can implement ready-notification without loss |
| K-006 | CDEV request/response bases 0x400/0x480 | I | stock | 99 §2.1 | `com.h:919-937` | Never mistakes a char reply for a block request (F1 lesson) |
| K-007 | BDEV 7 requests + GATHER/SCATTER | I | stock | 02 §2, 99 §2.2 | `com.h:963-987` | Knows vectored block I/O shape |
| K-008 | Partition parse + geometry report | M | stock | 02 §2 | `libblockdriver/drvlib.c`; `partition.h` | Can map partition offsets to disk LBAs |
| K-009 | Block ST/MT/ST-queue loop variants | M | stock | 02 §2 | `driver.c/driver_mt.c/driver_st.c`; `mq.c/trace.c` | Knows why MT is documented but not replicated (A-5) |
| K-010 | BDEV bases 0x500/0x580 + flags | I | stock | 99 §2.1 | `com.h:963-987` | Anchors block wire identity |
| K-011 | libbdev sync/async + slots + flush/resend | M | stock | 04 §2–3 | `libbdev/bdev.c/call.c/ipc.c:144-268/minor.c` | Knows what happens to in-flight requests on driver restart |
| K-012 | Minor open refcounts (16 devs / 4 minors / 256 calls) | D | stock | 04 §1–2 | `libbdev/const.h/type.h/driver.c/minor.c` | Can size client tables |
| K-013 | netdriver 13-hook `ndr_` table | I | stock+fix | 03 §2, 22 §1 (prefix correction) | `netdriver.h:20-45` | Calls every net hook by its true name |
| K-014 | NDEV 6 requests + 5 config subtypes + 6 mode bits | I | stock | 03 §2, 99 §2.2 | `com.h:1085-1114` | Can drive init/conf/send/recv lifecycle |
| K-015 | 9 cap bits + 4 flag bits + 3-state link | I | stock+fix | 03 §2 (F4/G8 fixed), 99 §2.2 | `com.h:1126-1145` | Expresses link-down (was missing) |
| K-016 | portio abstraction (port I/O helpers) | M | stock | 03 §2–3 | `libnetdriver/portio.c` | Keeps port access behind a trait (A-2) |
| K-017 | Shared ServerState/OpenSet/LoopAction core | A | new | code-only (`minix-driver-rt::core`, A3) | `os/libs/minix-driver-rt/src/core.rs`; ex-`protocol.rs` triplets | One loop machine instead of three copies |
| K-018 | DriverTransport 6 verbs + announce+run shell | A | new | code-only (`minix-driver-rt`, A1) + 00 §3.2 narrative | `os/libs/minix-driver-rt/src/{transport,runtime,kernel}.rs` | Writes a new driver bin as construct-device + run |
| K-019 | SEF cut embedded in transport.receive | A | new | code-only (E-INWIRE decision) | transport receive impl; `todo.md` A1 record | Knows where lifecycle interleaves with I/O |
| K-020 | Driver model: 256 opens, driver_receive, endpoint convention | D | stock | 99 §2.3 | `driver.h:41` + receive decl | Sizes the open table; addresses replies |
| K-021 | Minor numbering per-driver + /dev naming split | C | stock | 99 §2.4 | `dmap.h:85-92`; FS-side tables | Never confuses driver minors with /dev names |
| K-022 | errno-only error discipline (no invented codes) | K | stock | 00 §4, 99 §2.5, per-doc §4 | per-doc error tables; F3 lesson | Maps every failure to a Minix errno |
| K-023 | Restart-gate + live-update hooks | M | stock | 02/08 §2 | `liveupdate.c` (block + log); `filter/driver.c:387-392` | Restarts without resurrecting stale state |

### 2.2 Boot-critical (new-07..10)

| ID | Name | Type | Src | Current home | Anchor | Reader benefit |
|----|------|------|-----|--------------|--------|----------------|
| K-030 | memory dual-face triage (char+block) | M | stock | 05 §1–2 | `memory.c:101-104,64,72` (m_cdtab/m_bdtab) | Understands the only dual-family driver |
| K-031 | 13 minors: 7 fixed + 6 expandable ramdisks | D | stock | 05 §1–2 | `memory.c`; `dmap.h:85-92`; `ioc_memory.h` | Predicts each /dev node behavior |
| K-032 | vm_map_phys consumption (no VM mechanism) | I | stock | 05 §2–3 | `memory.c:141` | Uses physical mapping without re-teaching VM |
| K-033 | tty 8 lines: console+rs232+keyboard, termios, line discipline | M | stock | 06 §1–2 | `tty.c` (1603) + `arch/i386/console.c` + `rs232.c` + `keyboard.c` | Traces keypress → bytes |
| K-034 | Hangup/suspend/select pairing + ctty/log alias | M | stock+fix | 06 §2 (B1 fixed) | `tty.c:733-743,761`; `sigchar(SIGHUP)` tty.c:1474 | Closes sessions without polluting log alias |
| K-035 | pty 32 pairs, master/slave flow, packet mode | M | stock | 07 §1–2 | `pty.c` (860) + pty-side `tty.c` (1320); `config.h:46` | Runs remote-login plumbing |
| K-036 | pty master-close hangup effect (B0 + SIGHUP) | M | stock+fix | 07 §2 (B2 fixed) | `pty.c:241-250,778-780` | EOF + hangup on master close |
| K-037 | ptyfs split (driver vs tree semantics) | K | stock | 07 §1 | `ptyfs.c` (112) → 15-stage-fs/20 | Never re-teaches the filesystem side |
| K-038 | log 50k ring: overwrite-wins, parked readers, select bits | M | stock | 08 §1–2 | `log.c` (360) + `log.h`; `com.h:949-952` | Reads diagnostics lossily but live |
| K-039 | WakePlan: readers-first, select-second, atomic take | M | stock+fix | 08 §3 (B3 fixed) | `device.rs after_write`; `log.c:173` guard | Ordering guaranteed by type, not docs |
| K-040 | diag capture + kernel-message redirect | M | stock | 08 §2, 06 §2 | `diag.c` (54); `tty.c:270` | Knows both readers of kernel print |

### 2.3 System services + input (new-11..16)

| ID | Name | Type | Src | Current home | Anchor | Reader benefit |
|----|------|------|-----|--------------|--------|----------------|
| K-050 | random 3 layers: pools → keystream → blocking | M | stock | 09 §1–2 | `main.c/random.c/random.h`; `type.h:182-193` | Explains entropy → unlimited bytes |
| K-051 | reseed = hash-finalize over old key + digests | M | stock+fix | 09 §2 (S1 fixed) | `random.c:216-232` via `PoolHash` | No linear-mix impostor |
| K-052 | AES-256-CTR + SHA-256 self-contained backend | E | stock+new | 09 §2.8 (G4) | `crypto.rs` + FIPS vectors | Auditable no-std crypto with pinned vectors |
| K-053 | RTCDEV 5 requests + grant variants + Y2KBUG | I | stock+fix | 10 §2 (G9 fixed) | `com.h:995-1012`; `forward.c` | Reads/sets clock, forwards nested chips |
| K-054 | Clock permission gates (read-open, write-superuser, poweroff-PM) | K | stock | 10 §1–2 | `readclock.c:71-117` | Checks qualification before touching chip/grant |
| K-055 | pci enum + config space + bridge table + IRQ routing | M | stock | 11 §1–2 | `main.c/pci.c/pci_table.c` | Finds and characterizes any card |
| K-056 | Per-caller ACL model (vid/did/sub + class mask + InUse) | K | stock+fix | 11 §1.4 (S2/S3 fixed) | `pci.c:2039-2044,2320-2344`; `main.c:233-238,275-281` | Visible + reservable without global hiding |
| K-057 | 16-bit config shift + odd-alignment tests | T | stock+fix | 11 §5 (S4) | `config.rs:140-161` | No half-word write bug |
| K-058 | gpio pins-as-files (claim/drive/render/intr) | M | stock | 12 §1–2 | `gpio.c` (290, `:287 run_vtreefs`) | Turns pins into files |
| K-059 | Driver-side registration: generic + USB tracking | I | stock | 12 §1–2 | `libdevman/generic.c/usb.c`; `devman.h` | Declares devices from the driver side |
| K-060 | minix-sys authority (devman_client/usb_model/inputdriver) | A | stock+fix | 12 header note, 13 header (A11, E-DMCLIENT) | `os/libs/minix-sys/src/{devman_client,usb_model,inputdriver}.rs` | One authority per protocol, no twin bridge |
| K-061 | pckbd 3-state scancode machine + fall-through state 3 | M | stock+fix | 13 §1–2 (S5 fixed) | `pckbd.c:353-358` | `E1 1D 1C` yields ENTER |
| K-062 | Full 0x80-pair scan tables as consts | D | stock+new | 13 §3 (G3) | `table.c:11-169`; `tables.rs` | Complete translation without gaps |
| K-063 | Mouse 3-byte packets + RELATIVE flag | M | stock+fix | 13 §2 (S7) | `mouse.rs` + `input.h` line shape | Relative motion survives the bridge |
| K-064 | LED outbox + ACK + STATUS_TIMEOUT gate + bit order 1/2/3 | M | stock+fix | 13 §2 (S6/S7 fixed) | `pckbd.c:129`; `input.h:292-296`; `handlers.rs:213-227` | Caps/Num/Scroll lamps obey the server |
| K-065 | Keyboard watchdog absence (documented gap) | T | new | G7 list (unfixed doc gap) | `pckbd.c:21-23,49-62` | Knows the one unwired timer |

### 2.4 Storage (new-17..22)

| ID | Name | Type | Src | Current home | Anchor | Reader benefit |
|----|------|------|-----|--------------|--------|----------------|
| K-070 | virtio 3-ring contract (desc/avail/used) + ownership split | D | stock | 14 §1–2 | `virtio.c`; `virtio_ring.h:61-86` | Hands I/O across the VM boundary lock-free |
| K-071 | repr(C) ring structs + layout fns + chain lifecycle | A | new | code-only (A5) | `ring.rs` (Descriptor/Avail/Used + `vring_size`…); `hal.rs` | Byte-exact rings with tested layouts |
| K-072 | Feature negotiation (intersect) + kick iff !no_notify | M | stock+fix | 14 §2 (V1 fixed) | `virtio.c:766-783` | No queue_full inventor kick |
| K-073 | virtio_blk 3-segment chain + status→errno (unknown→EIO, C panics) | M | stock+fix | 15 §1–2 (V2 fixed) | `virtio_blk.c:549-563` | Never releases unknown status as success |
| K-074 | ahci HBA: 32 slots, cmd tables, PRDT, timeout→reset | M | stock | 16 §1–2 | `ahci/ahci.c` + `ahci.h:103-106` | Issues/waits/resets SATA commands |
| K-075 | identify 4-word capacity + support gates (GCAP/LBA48/FLUSH…) | K | stock+fix | 16 §1 (V5 fixed) | `ahci.c:537-560` | Rejects ATAPI/removable/no-LBA48, no 2TiB truncation |
| K-076 | at_wini controller phases + DMA policy + LU hook | M | stock | 16 §1–2 | `at_wini/at_wini.c` + `liveupdate.c` (76) | Drives legacy ATA contrasted with AHCI |
| K-077 | floppy density table + 6-retry recalibration | M | stock | 17 §1–2 | `floppy/floppy.c` | Handles slowest media honestly |
| K-078 | mmc 12-step fixed order (eMMC); SD paths absent-by-design | M | stock+fix | 17 §2.7–2.8 (V4 fixed) | `mmc/emmc.c:790-890` | Walks the real card sequence |
| K-079 | fbd rule match (overlap/dir/skip/count/aggregate) + pure action fns | M | stock+fix | 17 §3 (V6 fixed) | `fbd/rule.c:88-140`; `action.c:105-216` | Fault injection that actually faults |
| K-080 | filter restart budget (3 kills → mirror off / survivor promotion) | K | stock+fix | 17 §1.4/2.5 (V3 fixed) | `filter/driver.c:384-408`; `main.c:28` | Mirror health = restart ledger, not checksum votes |
| K-081 | vnd file-as-disk layout + geometry derivation | M | stock | 17 §2.6 (+plan-boundary correction note) | `vnd/vnd.c` (603) | Loop device without pretending to be fbd |

### 2.5 USB + display/audio + net + misc (new-23..30)

| ID | Name | Type | Src | Current home | Anchor | Reader benefit |
|----|------|------|-----|--------------|--------|----------------|
| K-090 | USB 5+4 numbers + wire slots (grant/urb/dev/info) | I | stock | 18 §2, 99 §2.2 | `com.h:813-841`; `usb.h` (158) | Numbers every urb conversation |
| K-091 | usbd HCD: enum order + schedule + musb backend | M | stock | 18 §2 | `usbd/hcd*.c`; `musb_core.c` | Brings devices up in order |
| K-092 | URB registry in/out symmetry (remove_pending shared) | M | stock+fix | 18 §3 (V8 fixed) | `libusb/usb.c:65-68,157-186` | Completion and cancel share one exit |
| K-093 | CBW/CSW/CDB constructors + tag pairing + transfer guard | A | new | code-only (A5) | `cbw.rs` (31/13-byte asserts, 7 CDBs); `wire.rs` | BOT transfers that verify |
| K-094 | hub port opinion + reset budget + comm-error suspends task | K | stock+fix | 19 §1–3 (V7 fixed) | `usb_hub.c:444-491` | Blacklist immune to walk-away; errors park the task |
| K-095 | fb char-device doctrine (5 hooks, no mmap) + EDID + logos | K | stock | 20 §1–2 (+plan correction) | `fb.c:44-51,63-83,115,230`; `ioc_fb.h:11-14`; `chardriver.h:9-23` | Opens/reads screen memory correctly |
| K-096 | libaudiodriver 14 hooks + fragment ring + special files | I | stock+new | 21 §2–3 (G1 crate) | `audio_fw.c`; `audio_fw.h:9-22`; `ioc_sound.h:12-22` | One conductor for 7 cards |
| K-097 | es1371/sb16 reference + AC97/codec + rate tables | M | stock | 21 §2 | `es1371/*`; `sb16.c` + `sb16.h:107-110` | Samples before waveforms |
| K-098 | dp8390 page cursor (BNRY wrap, == stoppage-1) | M | stock+fix | 22 §2 (G9 fixed) | `dp8390.c:610-611,668-671` | Walks pages without off-by-one |
| K-099 | virtio_net queue split + 64/32 refill rule | M | stock+fix | 22 §2 (N1 fixed) | `virtio_net.c:38,218` | Refills at half, not at fantasy 128 |
| K-100 | 12-card matrix: rings, probe, identify (lance 2-field incl.) | D | stock+fix | 23 §2 (N2 fixed) | `lance.c:707-722`; per-card `.c` | Tells cards apart, ring by ring |
| K-101 | printer offline-over-paper priority + 120×0.5s budget | K | stock+fix | 24 §1–3 (N3 fixed) | `printer.c:216-224` | Never waits 60s on a dead printer |
| K-102 | eeprom paging + sensor conversion formulas | M | stock | 24 §2 | `cat24c256.c`; `bmp085.c` | Small jobs done right |
| K-103 | i2c keys + addr check + reg op sequences | I | stock+new | 24 §2.6 (G2 crate) | `i2cdriver.c:12-116` | Shared chassis for i2c-bottom drivers |
| K-104 | ACPI/AML policy (port, not line-rewrite) + power misc | A | stock | 24 §2.6 | `power/acpi` (83279 lines); plan A-9 | Decides instead of drowning |
| K-105 | DMA contract: DmaRegion + DmaMemory 4 ops, ENOMEM/None/free-void | I | stock | 25 §§2–4 | `minix-types/.../dma.rs`; `virtio/hal.rs`; `virtio.c:~319` | Requests device-visible memory correctly |
| K-106 | sdev/sockdriver exclusion → 17-stage-net | K | stock | 99 §2.2 note; plan §5.4 | `com.h:1037-1068` (`0x1900`); E-SDEVOWN | Keeps socket semantics out of this stage |
| K-107 | ptyfs/vtreefs/devman-server/lwip/commands splits | K | stock | 07/12/13/22 headers; plan §5.4 | `ptyfs.c`; `servers/{input,devman}`; `minix/net`; `service/devmand` | Every neighbor has exactly one home |

Statistics: 63 pool rows (≈ stock 48 incl. fixed, new 15). By type:
C 3, M 30, D 7, I 15, K 12, A 8, E 2, T 2. Repeats merged with main-telling
points: CDEV constants (main: 99; users: 01/05–08), block loop (main: 02;
mirror: 04), select (main: 01; users: 06/07/08), announce (main: new-01;
family notes: pointer only).

---

## 3. Coverage audit

Topic universe = C symbols (§5.1 file list: 290 `.c`, 155087 lines; 57
server dirs) + OS-generic concepts (driver lifecycle, address spaces,
permission) + non-C artifacts (§3.7 list) + boundary contracts (plan §5.4,
edge entries). Checked against pool + current docs.

### 3.1 Gap table (universe present, no doc teaches)

| # | Topic | Evidence anchor | Suggestion | Pool fate |
|---|-------|-----------------|------------|-----------|
| GAP-1 | Unified driver runtime (announce+run shell, transport verbs, SEF cut, shared ServerState) as a *taught* concept | `os/libs/minix-driver-rt/src/{core,runtime,transport,kernel}.rs`; `chardriver.c:99-120,549-565` | NEW doc new-01 (contract §5.1) | K-017/018/019 new |
| GAP-2 | Driver-side registration as its own semantic (generic records + USB tracking + minix-sys authority) | `libdevman/generic.c/usb.c`; `minix-sys/.../devman_client.rs` | NEW doc new-15 (split from old-12) | K-059/060 stock |
| GAP-3 | AHCI and AT_WINI each complete (identify gates done V5; controller/DMA/reset contrast thin in shared doc) | `ahci.c` 2734 + `at_wini.c` 2243 lines vs shared 220-line doc | SPLIT old-16 → new-19 + new-20 | K-074/075/076 stock |
| GAP-4 | mmc/filter/fbd depth (12-step order V4, restart budget V3, rule+action V6 landed in code, docs lag in places per G7) | `emmc.c:790-890`; `filter/driver.c:384-408`; `rule.c/action.c` | SPLIT old-17 → legacy (new-21) + proxy (new-22); sync pass | K-077..080 stock |
| GAP-5 | Misc single-semantic violation (10 families in one doc; i2c chassis landed G2 but bus/bridge/platform still one bag) | `bus/i2c` 1397 + `ti1225` 431 + `tda19988` 1010 + `iommu` 477 + `vmm` 1188 + sensors/power/acpi | SPLIT old-24 → new-29 + new-30 | K-101..104 stock |
| GAP-6 | DMA contract position (exists as old-25 but taught *after* its consumers 14/16/18) | old-25 refs from 14/16/18 docs | MOVE old-25 → new-06 (early), content kept | K-105 stock |
| GAP-7 | pckbd keyboard watchdog (C has it, Rust+doc lack it) | `pckbd.c:21-23,49-62`; todo G7 | Fold into new-16 contract as named gap with C anchor (service-layer wiring) | K-065 new |
| GAP-8 | Shared NIC ring policy across dp8390/e1000/rtl8139/lance | per-card `ring/desc/txrx.rs`; todo A7 open | Do NOT split yet: record as open proposal in new-27/28 contracts (options: `minix-nic-policy` vs merge into netdriver) | speculative, marked |
| GAP-9 | Hook signatures `Result<usize,Errno>` migration (remaining half of A10) | `GrantId` alias `id.rs:75`; todo A10 | Record as open proposal in new-01 contract; batch with E-DEVWIRE | speculative, marked |
| GAP-10 | ramdisk boot-config semantics (no C, proto/rc only) | `drivers/storage/ramdisk/` (0 `.c`); plan §5.4 | Explicit NOT-TAUGHT → 18-stage-commands boot layout; pointer in new-07 | exclusion recorded |

No other universe member lacks a home: 57 server dirs + 22 framework files
map per plan §5.1 (audited 2026-08-16, re-confirmed by crate listing:
audio 7, bus i2c/pci/ti1225, clock readclock, hid pckbd, net 14, storage
8+memory, system gpio/log/random, tty 2, usb 3, video fb + tda19988, plus
power/printer/eeprom/sensors/iommu/vmm/examples in misc).

### 3.2 Repeat table (one topic, several explainers)

| # | Topic | Occurrences | New home (main) | Others become |
|---|-------|-------------|-----------------|---------------|
| REP-1 | Open-set/LoopAction/notify/server-state machine | old-01/02/03 protocol modules | new-01 (K-017) | back-pointers in new-02/03/04 |
| REP-2 | CDEV bases/flags/enums (libs vs vfs copies) | libs + `servers/vfs cdev.rs/bdev.rs` + 99 | 99 (K-006/010) + E-DEVWIRE convergence | type aliases + pinned tests; docs cite 99 |
| REP-3 | inputdriver bridge (crate twin deleted A11) | old-13 + minix-sys authority | new-16 via K-060 | single pointer, no re-teach |
| REP-4 | Suspend/select wording (tty/pty/log) | old-06/07/08 §2 | new-01 discipline (K-004/005) | per-driver usage only |
| REP-5 | virtio kick/queue wording | old-14 + code comment (V1 fixed) | new-17 (K-072) | single statement |
| REP-6 | virtio unknown-status wording | old-15 + request.rs comment (V2 fixed) | new-18 (K-073) | difference table only |

### 3.3 Overreach table (taught outside its boundary)

| # | Location | Content | Correct home |
|---|----------|---------|--------------|
| OVR-1 | 99 §2.2 SDEV paragraph | Socket request numbers | 17-stage-net (pointer only); K-106 |
| OVR-2 | old-12 §3.4/§5.3–5.4 deleted-design narrative | `minix-devman-client` design | Delete; minix-sys authority K-060 in new-15 |
| OVR-3 | old-05/06/08 VM/syslog/kernel mechanism asides | VM mapping, syslog commands, kernel diag production | Pointers to 02-stage-vm / 18-stage-commands / 01-stage-kernel |
| OVR-4 | plan.md stale lines (fb mmap, `ndo_`→`ndr_`, audiodriver header) | Corrected in docs 20/22/21 | Plan corrigenda in §8.4 (plan is reference, not rebuilt) |
| OVR-5 | old-24 hello-example crate contradiction | Doc says no-Rust-model, stub existed | Resolved: crate deleted (todo §4); new-30 teaches policy only |

### 3.4 Non-C topics: fixed ten, each answered (where taught or why not here)

- Link & load: drivers are statically listed servers (table.c/system.conf);
  no stage-owned loader → new-00 states it, new-01 shows the birth it feeds.
- Image & memory layout: boot image (table.c:44-64) + `/dev/imgrd` ramdisk
  backend → new-00 + new-07; ramdisk proto/rc → 18-stage-commands.
- Assembly entry & trap ingress: no driver asm; IRQ arrives as notify via
  kernel, ports via portio/arch traits → new-01 (K-016 + notify path).
- Boot assembly: table.c + system.conf + rc.minix order → new-00 causal
  chain + new-01 birth table T-1..T-3.
- Build & toolchain: per-driver Makefiles, net `*.conf`, lib selection →
  new-00 notes + per-group contracts' ground-truth rows (no separate doc;
  build files are listed, not taught).
- Cross-module iface & wire format: CDEV/BDEV/NDEV/RTCDEV/USB slots +
  `m_device`/`m_dev_ioctl` + USB m4/m3 slots → 99 + new-02/03/04/12/23
  (family slices), single-authority direction E-DEVWIRE.
- Error paths: errno tables + EDONTREPLY/SUSPEND + panic→EIO deltas (V2) +
  fail-closed chains → new-01 + per-doc §4.
- Shutdown & exit: SEF restart/LU, terminate, filter restart budget →
  new-01 (K-023) + new-22.
- Concurrency & sync: single-thread loop; block MT documented-not-replicated;
  virtio ownership split → new-01 + new-03/04 + new-17.
- Test infrastructure: host-side policy tests + fakes (ladder allocator,
  script transports), `cargo test -p <crate>`, qemu-tests for power-on,
  multi-process integration deferred → per-doc §5 + new-00 test map.

---

## 4. New catalog

32 files: `00` + `01..30` + `99`. English titles (muse requirement; also the
stage-wide writing rule — old docs are Chinese, B-phase rewrites in English).

| New # | Title | One-line positioning | Group |
|-------|-------|----------------------|-------|
| 00 | drivers-overview | Subsystem map, boot causal chain, navigation | overview |
| 01 | driver-runtime | Every driver's birth + loop + reply discipline (SEF/announce/transport/shared state) | foundation |
| 02 | chardriver-framework | CDEV protocol + 10-hook table | foundation |
| 03 | blockdriver-framework | BDEV protocol + 11 hooks + partition/mq/trace/LU | foundation |
| 04 | netdriver-framework | NDEV protocol + 13-hook table + portio + queues | foundation |
| 05 | bdev-client | Block calling side: sync/async, slots, minors, restart | foundation |
| 06 | dma-memory-contract | Device-visible memory: DmaRegion + 4 ops (moved from old-25) | foundation |
| 07 | memory-driver | Boot member 1: dual char+block, 13 minors | boot-critical |
| 08 | tty-driver | Boot member 2: 8 lines, termios, hangup/select | boot-critical |
| 09 | pty-driver | 32 master/slave pairs, packet mode, ptyfs split | boot-critical |
| 10 | log-driver | Diagnostic bus: 50k ring, WakePlan, diag capture | boot-critical |
| 11 | random-driver | Entropy pools → AES-CTR stream, reseeding | system service |
| 12 | readclock-driver | RTCDEV 5 ops + grant variants + permission gates | system service |
| 13 | pci-driver | Enumeration, config space, per-caller ACL + reserve | system service |
| 14 | gpio-driver | Pins as files (split half A of old-12) | system service |
| 15 | driver-registration | Driver-side registry: generic + USB tracking, minix-sys authority (split half B, NEW) | system service |
| 16 | pckbd-driver | Scancode machine, mouse, LEDs, input bridge | input |
| 17 | virtio-framework | Rings, negotiation, kick rule, Hal | storage |
| 18 | virtio-blk-driver | First full tenant: 3-chain + status truth | storage |
| 19 | ahci-driver | HBA ports/cmd tables/PRDT/identify gates (split A) | storage |
| 20 | ata-driver | Legacy controller phases/DMA/reset/LU (split B) | storage |
| 21 | storage-legacy | floppy + mmc + vnd: slow/removable/file-backed (split) | storage |
| 22 | storage-proxy | fbd + filter: pass-through policies (split) | storage |
| 23 | usb-framework | URB numbers, HCD/enumeration/schedule, registry | usb |
| 24 | usb-storage-hub | BOT/SCSI + hub ports (first full consumption) | usb |
| 25 | fb-driver | Char-device framebuffer, EDID, modes | display |
| 26 | audio-drivers | 14-hook framework + 7 cards (ref + matrix) | audio |
| 27 | net-reference | dp8390 pages + virtio_net queues | net |
| 28 | net-variants | Other 12 NICs: rings + probe (worked 3 + matrix 9) | net |
| 29 | bus-bridges | i2c chassis + ti1225 + tda19988 + iommu + vmm_guest | platform |
| 30 | platform-sensors | printer + eeprom + sensors + power/acpi policy + examples | platform |
| 99 | global-concepts | Number phonebook only (constants + model + errno axiom) | reference |

Reading paths:

- Main line (boot + causal spine): 00 → 01 → 02 → 06 → 07 → 08 → 13 →
  15 → 16 → 17 → 18 → 23 → 24 → 25 → 27 → 99 as consulted.
- Branch A (block users): 03 → 05 → 07(block face) → 18 → 19/20 → 21/22.
- Branch B (char users): 02 → 09 → 10 → 11 → 14 → 26.
- Branch C (bus/device enablement): 13 → 17/19/20/27/28.
- Skippable on first pass: 21, 22, 28 (worked example + matrix only), 29,
  30, 12 (forward mode), 09 packet-mode minutiae. 99 is consulted, not read
  linearly. 06 is required before 17/19/20/23/24 (back-references suffice).

Parallel groups (representative + matrix): storage (17/18 representative;
19/20 contrast pair; 21/22 matrices), net (27 representative; 28 matrix with
e1000/rtl8139/lance worked), audio (es1371+sb16 worked; 5-card matrix),
platform (printer/eeprom/bmp085 worked; rest matrix).

---

## 5. Per-doc contracts

Format per doc: positioning; teaches (K IDs); not-teaches (with home);
prerequisites (earlier numbers only); post (who cites this); ground truth
(C + non-C anchors); knowledge rows (ID/anchor/why-here/source); acceptance.
G7 verdict: every contract below has all seven elements.

### 00-drivers-overview

- Positioning: the map, not the territory — answers where everything lives.
- Teaches: K-001 (birth shape, one paragraph), boot chain T-1..T-3, group
  structure, reading paths, test map, neighbor splits (K-106/107).
- Not-teaches: any hook/ring/register detail → 01..30; constant values → 99.
- Prerequisites: none. Post: all docs cite 00 for position.
- Ground truth: `table.c:44-64`; `system.conf` service stanzas; `rc.minix`
  order; 57-dir + 290-file counts (plan §5.1).
- Knowledge: K-001/106/107 (why: orientation; source: stock).
- Acceptance: a newcomer can name the doc answering any driver question
  without searching; boot chain reproducible from anchors.

### 01-driver-runtime (NEW — GAP-1)

- Positioning: what every driver process does from birth to steady loop.
- Teaches: K-001, K-002 (classification only, values in 99), K-004, K-005,
  K-017, K-018, K-019, K-023, K-022 (discipline statement).
- Not-teaches: family hook tables → 02/03/04/12/26; wire values → 99; DMA
  backing → 06; device specifics → 07+.
- Prerequisites: 00. Post: every 02..30 cites 01 for loop/reply/restart.
- Ground truth: `chardriver.c:99-120,137-226,549-565`; `driver_receive`
  (`driver.h`); `memory.c:113-162`; `tty.c:146-194,303`;
  `readclock.c:39-64,154-164`; `pci/main.c:721-728`;
  `os/libs/minix-driver-rt/src/{core,runtime,transport,kernel}.rs`;
  `system.conf` + `rc.minix` (T-2/T-3).
- Knowledge rows: K-001 (birth; stock 01/05/10), K-002 (masks; stock),
  K-004/005 (park/select; stock 01), K-017/018/019 (shared core/transport/
  SEF cut; new, code anchors), K-023 (restart/LU; stock 02/08), K-022
  (errno axiom pointer; stock 00/99).
- Acceptance: reader can narrate birth→announce→receive→classify→dispatch→
  reply-or-park→notify for a framework and a hand-loop driver; can state
  where SEF interleaves and where SUSPEND dies; A10/GAP-9 recorded as open.

### 02-chardriver-framework (old-01)

- Positioning: the CDEV family contract + 10-hook table.
- Teaches: K-003, K-006 (usage; values defer to 99), K-020 (open width use).
- Not-teaches: runtime loop shape → 01; constant table → 99; block/net →
  03/04; concrete drivers → 07..10/14/25/26.
- Prerequisites: 00, 01. Post: 07/08/09/10/14/25/26 cite hook semantics.
- Ground truth: `libchardriver/chardriver.c`; `chardriver.h`;
  `com.h:919-956`; `driver.h:41`; `minix-chardriver/{protocol,driver}.rs`
  (+ A3 alias note).
- Knowledge: K-003/006/020 (stock).
- Acceptance: for each of 7 requests: message shape, hook, park/reply rule,
  default when hook unset.

### 03-blockdriver-framework (old-02)

- Positioning: the BDEV family contract + partition/geometry/queues.
- Teaches: K-007, K-008, K-009, K-010 (usage).
- Not-teaches: runtime → 01; calling side → 05; concrete storage → 07/18..22.
- Prerequisites: 00, 01 (02 char-routing reuse becomes pointer).
- Post: 05/07/18/19/20/21/22 cite.
- Ground truth: `libblockdriver/{driver,driver_mt,driver_st,drvlib,
  liveupdate,mq,trace}.c`; `blockdriver.h`; `partition.h`; `com.h:963-987`.
- Knowledge: K-007/008/009/010 (stock; K-009 states MT non-replication).
- Acceptance: ST vs MT vs queue loops contrasted; partition parse worked;
  GATHER/SCATTER vectored example; defaults enumerated.

### 04-netdriver-framework (old-03)

- Positioning: the NDEV family contract + portio + queue/status policy.
- Teaches: K-013, K-014, K-015, K-016.
- Not-teaches: runtime → 01; lwip/uds → 17-stage-net; concrete NICs → 27/28.
- Prerequisites: 00, 01. Post: 27/28 cite.
- Ground truth: `libnetdriver/{netdriver,portio}.c`; `netdriver.h:20-45`;
  `com.h:1085-1145`; `config.h:102-104`.
- Knowledge: K-013/014/015/016 (stock incl. F4/G8 fixes).
- Acceptance: 6-request lifecycle narrated; queue bounds (8 tx / 2 rx),
  link/caps/flags, status-push direction; sdev exclusion stated (K-106).

### 05-bdev-client (old-04)

- Positioning: the calling side of block — find/send/wait/recover.
- Teaches: K-011, K-012.
- Not-teaches: driver-side service → 03; VFS/FS consumption → 05-stage-vfs /
  15-stage-fs; runtime → 01.
- Prerequisites: 00, 01, 03. Post: 07/18..22 cite restart behavior.
- Ground truth: `libbdev/{bdev,call,driver,ipc,minor}.c` + `const.h/type.h`;
  `bdev.h`; `ipc.c:144-268` (reply-type check F2).
- Knowledge: K-011/012 (stock incl. F2/A6).
- Acceptance: sync vs async call traced; slot/destination/flush story (A6);
  minor-reopen overclaim (G7) corrected or explicitly excluded.

### 06-dma-memory-contract (moved old-25 — GAP-6)

- Positioning: the fuel contract — device-visible memory, early so rings can
  cite it. Content kept; motive kept abstract; worked example forward-points
  to 17.
- Teaches: K-105 (+ K-071 Hal half stays in 17; 06 owns the contract).
- Not-teaches: ring layouts → 17; allocator internals → 02-stage-vm (edge3
  S37); cache-coherence expansion → future.
- Prerequisites: 00, 01. Post: 17/19/20/23/24 cite.
- Ground truth: `minix-types/.../dma.rs`; `minix-virtio/.../hal.rs`;
  `virtio.c:~319` (alloc_contig use); E-DMABUF record.
- Knowledge: K-105 (stock, moved).
- Acceptance: 4 ops + region credential narrated; ENOMEM/None/void-free
  table; fake vs production implementability stated.

### 07-memory-driver (old-05)

- Positioning: boot member 1 — the only dual-face driver.
- Teaches: K-030, K-031, K-032.
- Not-teaches: VM mapping internals → 02-stage-vm; root-FS mount →
  15-stage-fs; other storage → 18..22.
- Prerequisites: 00, 01, 02, 03, 05, 06. Post: 00 cites minimal loop.
- Ground truth: `storage/memory/memory.c` (`:64,72,101-104,141`);
  `dmap.h:85-92`; `ioc_memory.h`; memory `device/transfer/char_face/
  block_face` sources.
- Knowledge: K-030/031/032 (stock).
- Acceptance: triage rule stated; 13 minors' behaviors enumerated; ramdisk
  exclusion pointer (GAP-10) present.

### 08-tty-driver (old-06)

- Positioning: boot member 2 — 8 lines, line discipline, suspend/select.
- Teaches: K-033, K-034.
- Not-teaches: input-server consumption → 12-stage-input; pty → 09;
  keymaps → 16; console raster → device detail section only.
- Prerequisites: 00, 01, 02. Post: 09 cites slave-line reuse.
- Ground truth: `tty/tty.c` (`:144-194,270,733-743,761,1474`);
  `arch/i386/console.c`; `rs232.c`; `keyboard.c/keymaps`;
  `dmap.h:95`; `config.h:41-46`.
- Knowledge: K-033/034 (stock incl. B1).
- Acceptance: minor→slot map (3 ranges + 2 specials); canonical vs raw
  path; suspend/cancel pairing; log-alias non-pollution (B1 test cited).

### 09-pty-driver (old-07)

- Positioning: hardwar
...[truncated 21165 chars]