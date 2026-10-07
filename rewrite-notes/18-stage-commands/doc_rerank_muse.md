# 18-stage-commands Document Rebuild Blueprint (muse)

## 0. Metadata

- Executor: muse. Date: 2026-09-20. Target: `rewrite-notes/18-stage-commands/`.
  Repo root: `/home/xzhao/github/minix-rs`. HEAD at blueprint time: `25303385e`
  (prior session commit `a0a5b87bc`; tree has uncommitted modifications under `os/`).
- Task: R-phase rebuild blueprint. Deliverable: this file only. No body text modified.
- Scope, in: 26 numbered docs (`00` + `01`–`24` + `99`), `plan.md`, `todo.md`,
  `draft/README.md`. Out of scope: `.design/`, `tmp_design_and_todo/` (never cited),
  other AIs' `doc_rerank_*` products (not read), other stages except as boundary
  evidence (`09-stage-init` overview + `14-stage-runtime/plan.md` headings only).
- Language of this blueprint: English (per muse special requirement). New body docs
  keep the repo's existing language; this blueprint does not change that.
- Read list (all headers + section skeletons read; bodies sampled):
  `00` (67 lines), `01` (265), `02` (207), `03` (215), `04` (180), `05` (238),
  `06` (234), `07` (231), `08` (195), `09` (191), `10` (221), `11` (175),
  `12` (199), `13` (181), `14` (146), `15` (156), `16` (143), `17` (213),
  `18` (192), `19` (178), `20` (182), `21` (190), `22` (226), `23` (181),
  `24` (188), `99` (98), `plan.md` (471), `todo.md` (289), `draft/README.md`,
  `09-stage-init/00-init-overview.md`, `14-stage-runtime/plan.md` (headings).
- C/Rust ground truth spot-read: `minix3/sbin/init/init.c` (states, 1902 lines),
  `minix3/sbin/rcorder/rcorder.c` (REQUIRE/PROVIDE strings), `minix3/etc/rc.d/`
  (33 entries incl. Makefile = 32 scripts), `minix3/libexec/getty/main.c` (711),
  `minix3/usr.bin/login/login.c` (789; `DEFAULT_BACKOFF 3`, `DEFAULT_RETRIES 10`),
  `minix3/bin/sh/builtins.def` + `bin/sh/` listing, `minix3/games/` (24 entries),
  `minix3/etc/` (51 entries), `minix3/minix/commands/` listing,
  `os/commands/*` (24 `Cargo.toml`, enumerated §0.1), `os/libs/minix-sys/src/lib.rs`
  (`pub fn` list), `os/etc/README.md` (placeholder).
- Commands run (read-only): `ls`, `wc -l`, `grep -n`, `rg -n/-l/-c`, `sed -n`,
  `find -name Cargo.toml`, `git rev-parse`, `git status --short`.

### 0.1 Rust domain-crate census (verified 2026-09-20, 24 crates)

`bin/diskimg, bin/editor, bin/fileops, bin/proctools, bin/shell, bin/sysinfo,
bin/termctl, games/stdio-games, games/term-games, games/text-games,
sbin/devdb, sbin/diskfmt, sbin/init, sbin/maint, sbin/mountinfo,
usr-bin/compress, usr-bin/doctools, usr-bin/login, usr-bin/regex,
usr-bin/textfilter, usr-sbin/netconfig, usr-sbin/netservices, usr-sbin/pkgtools,
usr-sbin/svcsched` — exactly 24. The old "35 crates" figure is dead; `00` §1.3
already records the correction. Any surviving "35" mention elsewhere is a bug.

### 0.2 Length discipline for the rebuild (user note)

One concept per doc; group docs target ≤ 350 lines (current 143–265: healthy, keep);
`00`/`99` target ≤ 200 lines (current 67/98: too thin — expand, see §4).
Doc count is not fixed: this blueprint proposes 26 docs (same count, 11 renumbered).
Absolute ceiling per doc ~3000 lines is acknowledged but never to be approached;
anything crossing 400 lines must be split.

## 1. C True Order

Stage type: **command collection with a delivery-chain spine** (R-prompt §nine:
collection type, organized by convergence point + trigger time; do NOT fake a linear
order across the 328 parallel commands). The spine (boot → login → shell) is linear
and runtime-true; the families behind it are parallel, framework-first.

### 1.1 Spine true-order table (each row independently checkable)

| # | Action | C anchor | Note |
|---|--------|----------|------|
| T-01 | Kernel hands control to `init`, PID 1 | `minix3/sbin/init/init.c` (1902 lines); `pathnames.h` | Detail now lives in `09-stage-init` (16 docs); 18 keeps only the rc/script face |
| T-02 | init state machine: `single_user → runcom → read_ttys → multi_user`, exits via `clean_ttys / catatonia / death` | `init.c:141-148` (7 `state_func_t`); `:151` `AUTOBOOT/FASTBOOT`; `:153` `transition()`; `:195` `requested_transition` | Doc `01` §2.1 "seven-state" claim verified verbatim |
| T-03 | `runcom` runs `/etc/rc`; rc layers config-vars + function-lib + script-dir | `minix3/etc/rc` (468 lines); `minix3/etc/rc.subr` (1359); `minix3/etc/rc.conf`; `minix3/etc/defaults/` | `01` §2.5/§2.8 anchors match on-disk files |
| T-04 | `rcorder` sorts `rc.d` by comment contract | `minix3/sbin/rcorder/rcorder.c:78-83` (`# REQUIRE:`/`# REQUIRES:`/`# PROVIDE:`); `minix3/etc/rc.d/` = 32 scripts + `Makefile` (33 entries verified) | "Comment is contract" — `01` §2.6 wording accurate |
| T-05 | `read_ttys`/`multi_user` spawn one session per `ttys` line; `clean_ttys` reaps | `init.c` session fns (`runetcrc`, `do_setttyent`, `start_session_db`); `minix3/etc/ttys` | Contract shared with the login chain |
| T-06 | Per-tty: `getty` claims terminal → `exec login` → password check → `exec shell` | `minix3/libexec/getty/main.c` (711 lines); `minix3/usr.bin/login/login.c` (789; `main` at `:136`; retry/backoff `:116-117` = 10 tries / 3 s; compare at `:424`; teardown at `:604-724`) | Three handoffs, one verification — `03` §1 framing is runtime-true |
| T-07 | Password toolchain: `passwd/chpass/pwhash` edit, `pwd_mkdb` builds DB, `vipw/user` administer | `minix3/usr.bin/passwd,chpass,su,newgrp,pwhash,id/` + `minix3/usr.sbin/pwd_mkdb,vipw,user/` + `minix3/etc/master.passwd` | Two faces (text + DB), one truth |
| T-08 | Shell interprets each line: lex → 7 expansions → redirect → eval/jobs; builtins from registry | `minix3/bin/sh/`: `parser.c` (1686), `expand.c` (1640), `eval.c` (1366), `jobs.c` (1532), `exec.c` (1071), `redir.c` (400), `main.c` (376), `builtins.def` (93, NetBSD Almquist) | Startup files branch login/interactive/non-interactive |
| T-09 | Commands run `fork+exec`, consume syscalls via libc | `minix3/lib/libc/` + `minix3/minix/lib/libsys/`; Rust: `os/libs/minix-sys/src/lib.rs` exposes `send/receive/sendrec/notify/fork/exec/exit/sigreturn/waitpid/kill/open/tcgetattr/tcsetattr/close/read/write/stat/lstat/fstat/ioctl/fcntl/getdents/mmap` | **Drift**: `todo.md` §2 "zero wrappers for stat/getdents/ioctl" is stale since the 09-18 wiring batches; Requires backfill must use this list |
| T-10 | `shutdown/reboot/setup` close the loop | `minix3/sbin/shutdown,reboot/` + `minix3/minix/commands/setup/setup.sh` + `minix3/etc/boot.cfg.default` | Coin-flip with boot — `01` §1.4 framing accurate |

### 1.2 Family dispatch shapes (parallel, framework-first)

- F-A text/file filters: `argv→stdin/stdout→exit-code`; representative `uniq` state machine (257 lines), `cut` option-string contract, `tr` set/map/squeeze, `sort` (418, biggest, execution留白).
- F-B pattern tools: `grep` options-are-semantics (`grep.c` 506; match single-point `util.c:205`) + `sed` parse (`compile.c` 947) and exec with empty-match advance (`process.c` 792).
- F-C storage: dual `mount` (`sbin/mount` vs `minix/commands/mount`), `fsck` pass queue with pass-zero skip (`fsck.c:254`), MBR geometry (`bootblock.h`: off 446, magic `0xAA55`), `dd` operands (`args.c:105-121`), ISO marker `CD001` (`isoread.c:41`).
- F-D network: flags verbs (`SIOCGIFFLAGS/SIOCSIFFLAGS/SIOCSIFMTU`), route socket (`RTM_VERSION`, `RTM_GET/ADD/DELETE`), echo check (`in_cksum`, `ICMP_ECHO/ECHOREPLY`), TTL trace (`IPCTL_DEFTTL`); daemons `inetd` (64 sockets/20 args/builtins) + `syslogd` (facility×severity).
- F-E system/games: `readclock` 5-letter verbs, `sysctl` (`CTLTYPE_NODE`), `ldd` static listing (A-1); games 10 stdio + 7 terminal + 7 text = 24 verified.

### 1.3 Order-difference table (runtime truth vs teaching order)

| # | Runtime fact (anchored) | Teaching choice | Compensation |
|---|-------------------------|-----------------|--------------|
| D-1 | Storage causality is partition/format → mount; old `15` §1.5 admits "use first, prep later" | Keep mount-before-partition (use motivates prep) | N-14 transition forward-points the prep question; N-15 §1.0 answers it by name |
| D-2 | rc scripts touch network/remote mounts (`rc.d/network`, `mountcritremote`) before net chapters | Keep net chapters late (families are parallel, not spine) | N-01 names network call-sites as opaque, with back-pointers to N-18/19 |
| D-3 | Any command runs once shell exists; games need nothing from storage/net | Games last as skippable acceptance layer | `00` path labels N-22–24 skippable; nothing later depends on them |
| D-4 | `getty` needs termios the moment it claims the tty (T-06) | Terminal chapter moves EARLY (new N-04, before login) | Eliminates the old 03→13 forward reference structurally (§4.1) |

## 2. Knowledge Pool

Origin `S` = stock (from existing docs) → §6 gives each item a destination.
Origin `N` = new (from C/non-C/Rust, absent in docs) → evidence anchor given here.
Types: C concept, M mechanism, D data structure, I interface/protocol,
K constraint/invariant, A arch-evolution, T tooling/test.
Duplicate homes merged; primary telling point starred. Reader payoff compressed to
the question the item answers.

| ID | Name | Ty | Or | Current home | Anchor | Answers |
|----|------|----|----|--------------|--------|---------|
| K-001 | init 7-state machine | M | S | 01 §2.1* | `init.c:141-148,153` | Which state runs rc, which reaps ttys? |
| K-002 | runcom AUTOBOOT/FASTBOOT | M | S | 01 §2.2* | `init.c:151` | When is fsck skipped? |
| K-003 | ttys session table | D | S | 01 §2.3* + 09-stage-init 06-10 | `init.c:runetcrc/do_setttyent`; `etc/ttys` | Who spawns one getty per line? |
| K-004 | shutdown/reboot/power signals | I | S | 01 §2.4* | `sbin/shutdown,reboot/` | Which signal does what? |
| K-005 | rc 3 layers (conf/subr/rc.d) | M | S | 01 §1.3/§2.5* | `etc/rc,rc.subr,rc.conf,rc.d/` | Where does a boot variable live vs logic? |
| K-006 | rcorder comment contract | M | S | 01 §2.6* | `rcorder.c:78-83` | How is boot order computed? |
| K-007 | shutdown/reboot/setup/boot.cfg | T | S | 01 §2.7* | `sbin/shutdown,reboot/`; `setup.sh`; `boot.cfg.default` | How to stop, restart, first-install? |
| K-008 | rc.conf/defaults face | D | S | 01 §2.8* | `etc/rc.conf`; `etc/defaults/` | Which file overrides which? |
| K-009 | minix-service 9 shapes | I | S | 02 §2.1* | `minix-service.c` + `.8` | What verbs does the RS client speak? |
| K-010 | service script 3-part | M | S | 02 §2.2* | `usr.sbin/service/service` | flags/list/action? |
| K-011 | svrctl root-only 4-part | K | S | 02 §2.3* | `svrctl.c` | Why is svrctl experts-only? |
| K-012 | cron tab parse | M | S | 02 §2.4* | `cron/tab.c:285-360` | How is a time field matched? |
| K-013 | crontab range | I | S | 02 §2.4* | `crontab.c:70-122` | What can a user schedule? |
| K-014 | at/atnormalize two ends | M | S | 02 §2.5* | `at/at.c:26-80`; `atnormalize.c:25+` | One-shot vs periodic? |
| K-015 | update 24-line ticker | M | S | 02 §2.6* | `update.c` (24 lines) | What beats the buffer daemon? |
| K-016 | getty tty claim | M | S | 03 §2.1* | `getty/main.c:190-270` | What happens before login appears? |
| K-017 | ttys as init↔login contract | K | S | 03 §2.2* + 01 §2.3 | `etc/ttys` | Which file both sides read? |
| K-018 | gettytab dialect dict | D | S | 03 §2.3* | `etc/gettytab` | How do baud/parity per-tty vary? |
| K-019 | login verify+setup | M | S | 03 §2.4* | `login.c:136,424,604-724`; retry 10/backoff 3 (`:116-117`) | What does login check, then set up? |
| K-020 | passwd vs master.passwd | D | S | 03 §1.2/§2.6* | `etc/master.passwd` | Two faces, one truth? |
| K-021 | pwd_mkdb/vipw/user/chpass chain | T | S | 03 §2.5* | `usr.sbin/pwd_mkdb,vipw,user/`; `usr.bin/chpass/` | Who builds the DB safely? |
| K-022 | su/newgrp/nologin/id/pwhash | I | S | 03 §1.3* | `usr.bin/su,newgrp,pwhash,id/`; `sbin/nologin/` | How to switch identity? |
| K-023 | profile/skel luggage | D | S | 03 §1.4* | `etc/profile`; `etc/skel/` | What does shell inherit? |
| K-024 | MAKEDEV makedev fn | M | S | 04 §2.1* | `MAKEDEV.sh:56-110` | 7 params, 2 outputs? |
| K-025 | mknod/pack_dev | I | S | 04 §2.2* | `sbin/mknod/mknod.c,pack_dev.c` | How to make one node? |
| K-026 | dev_mkdb walk+build | M | S | 04 §2.3* | `dev_mkdb.c:56-98,178-220` | Who indexes /dev? |
| K-027 | getent table dispatch | M | S | 04 §2.4* | `getent.c:98-136` | One entry, 13 tables? |
| K-028 | mtree spec + cmd | K | S | 04 §2.5* | `etc/mtree/`; `usr.sbin/mtree/` | What SHOULD the tree look like? |
| K-029 | 13 system-DB tables | D | S | 04 §1.4* | `etc/group,shells,hosts,services,protocols,motd,nsswitch.conf` | Which file answers which name? |
| K-030 | shell program+language | C | S | 05 §1.1* | `bin/sh/` | Why is shell both? |
| K-031 | ash/ksh/csh dialects | C | S | 05 §1.2* | `bin/sh,ksh,csh/` | Which dialect for what? |
| K-032 | startup files 3 paths | M | S | 05 §1.3* | `etc/profile,shrc,csh.*` | login vs interactive vs script? |
| K-033 | builtins registry | D | S | 05 §2.6* | `builtins.def` (93) | Which commands must run in-body? |
| K-034 | parser/7 expansions/redir/eval-jobs | M | S | 05 §2.2-2.5* | `parser.c,expand.c,redir.c,eval.c,jobs.c` | How is a line executed? |
| K-035 | env tools | T | S | 05 §1.5* | `env,printenv,getopt,uname,machine,pagesize,hostname,domainname,sysenv` | How to inspect/construct env? |
| K-036 | sh text-vs-exec split | A | S | 05 §3.1/§3.5* | `os/commands/bin/shell` (lexer/expand/redir/script; 36 tests) | What is done, what waits for fork/exec/pipe2? |
| K-037 | mode 12 bits | K | S | 06 §1.1* | `bin/chmod/` | What does each bit mean? |
| K-038 | links as aliases | C | S | 06 §1.2* | `bin/ln/` | hard vs symlink? |
| K-039 | copy/list/find/space fams | M | S | 06 §1.3* | `ls` (715), `cp` (548), `find` (306) | Move, view, find, measure? |
| K-040 | test 4-level recursion | M | S | 06 §2.1* | `test.c` (717) | How is an expression evaluated? |
| K-041 | chmod shell+lib split | M | S | 06 §2.2* | `chmod.c` | options outside, bits inside? |
| K-042 | small-cmd pure fns | M | S | 06 §2.4* | `basename,dirname,mktemp,pathchk,…` | One main, one decision? |
| K-043 | 34-cmd file contract table | I | S | 06 §2.5/§4.5* | plan §5.2 bin+usr.bin+chroot/link/unlink+truncate rows | Per-command options/IO/exit/errors? |
| K-044 | line currency | C | S | 07 §1* | `usr.bin/` 35 cmds | Why is the line the unit? |
| K-045 | head/tail/cut/paste/join/comm | M | S | 07 §1.1/§2.2* | `head` (204), `cut` (306), `uniq` (257) | Take, cut, join? |
| K-046 | width/tab/direction shaping | M | S | 07 §1.2* | `col,expand,fold,rev` | How are columns laid out? |
| K-047 | count/sum/checksum | M | S | 07 §1.3* | `wc` (354), `cksum` | Count vs verify? |
| K-048 | diff/patch/sdiff/compare | M | S | 07 §1.4/§2 (diff engine)* | `minix/usr.bin/diff/` (diffreg port) | How are two files reconciled? |
| K-049 | sort/uniq/tr triad | M | S | 07 §1.6/§2.1/§2.4* | `sort` (418), `tr` (283) | Order, dedupe, translate? |
| K-050 | 39-cmd text contract table | I | S | 07 §4.5* | plan §5.2 07-row | Per-command contract? |
| K-051 | BRE/ERE grammar | C | S | 08 §1.2* | `minix/usr.bin/grep/`; `usr.bin/sed/` | Two spellings, one meaning? |
| K-052 | leftmost-longest-greedy | K | S | 08 §1.3* | POSIX + C behavior | What does a match mean? |
| K-053 | no-exponential-backtrack | K | S | 08 §1.4/§3.1* | `minix-regex` VM (`pattern.rs,matcher.rs`) | Why a VM, not backtracking? |
| K-054 | grep options=semantics | I | S | 08 §2.1* | `grep.c:112-122,67-86,281-451,480,497-505`; `util.c:205` | Which flag changes what? |
| K-055 | sed addr+cmd parse | M | S | 08 §2.3* | `compile.c:81,122,189-196,339,480` | How are addresses/commands read? |
| K-056 | sed exec + empty advance | M | S | 08 §2.4* | `process.c:95-97,113-135,277,403-432` | How does execution avoid stalling? |
| K-057 | ed addr+cmd session | M | S | 09 §1.1-1.2* | `bin/ed/main.c:285,314,465,481+` | 7 ways to name a line? |
| K-058 | ed line ops + ranges | M | S | 09 §2.2* | `main.c:898,917,1051-1242,1271-1297` | How are ranges checked and run? |
| K-059 | buf/io/undo/cbc/glbl | D | S | 09 §2.3-2.4* | `buf.c,io.c,undo.c,re.c,sub.c,cbc.c,glbl.c,ed.h` | Where does text live? |
| K-060 | mined fullscreen | M | S | 09 §1.3* | `mined1.c` (1774), `mined2.c` (1666) | Screen-direct editing? |
| K-061 | no-vi ARCH suspense | A | S | 09 §1.5/§3 (3.5)* | plan A-list (editor selection) | Why no vi, and what is suspended? |
| K-062 | man find+render | M | S | 10 §2.1* | `man.c` (1088), `manconf.c` (272), `etc/man.conf` | How is a page found and shown? |
| K-063 | makewhatis + 3 sisters | M | S | 10 §2.2* | `makewhatis.c` (1174); `apropos,whatis,whereis` | How is the index built/queried? |
| K-064 | cal reform params | K | S | 10 §2.3* | `cal.c` (924; `:65-66,:94-137`) | Gregorian reform in code? |
| K-065 | i18n A-8 defer | A | S | 10 §1.5/§3.4* | `locale,mklocale,mkesdb,mkcsmapper` | What is explicitly postponed? |
| K-066 | typeset/dev-aux families | T | S | 10 §1.1/§1.3/§2.4* | `pr,fmt,nl,indent,m4,ctags,…`; `cawf,spell,prep` | Print vs help-build? |
| K-067 | 35-cmd doc contract table | I | S | 10 §2.5* | plan §5.2 10-row | Per-command contract? |
| K-068 | LZ one-line idea | C | S | 11 §1.1* | `compress.c` (1618; opts `:20-42`) | Dictionary compression? |
| K-069 | transfer coding (uu) | M | S | 11 §1.2/§2.2* | `uuencode.c` (202; `:63-64,:109-113`) | Binary through text channels? |
| K-070 | archive as stream (pax/shar) | M | S | 11 §1.3/§2.3* | `bin/pax/`; `shar.sh` | Many files, one stream? |
| K-071 | integrity verify | K | S | 11 §1.4* | `cksum/crc` | Pressed small, prove intact? |
| K-072 | big-3 duty coverage | I | S | 11 §2.4/§1.5* | `gzip` (2114), `bzip2/`, `unzip` (1074), `bdes` (1076) | Who owns which format? |
| K-073 | ps columns optional | I | S | 12 §1.1/§2.2* | `bin/ps/`; `keyword.c:115-197` | Which columns, and how chosen? |
| K-074 | kill name+number | I | S | 12 §1.2/§2.1* | `kill.c:83,105,119-127,178,188-195`; `signal.h:52-84` | Names, numbers, and the two special cases? |
| K-075 | utmp sole truth | D | S | 12 §1.3/§2.3* | `etc/utmp`; `utmp.h` (36 B) | Who is logged in, says who? |
| K-076 | time+lock tools | T | S | 12 §1.4* | `time,sleep,date,nice,renice,nohup,lock,shlock` | Measure and mutually exclude? |
| K-077 | 28-cmd proc contract table | I | S | 12 §1.5* | plan §5.2 12-row | Per-command contract? |
| K-078 | speed convention | K | S | 13 §1.1* | `stty.c:137` | Numbers behind baud? |
| K-079 | cchar privilege | K | S | 13 §1.2/§2.1* | `cchar.c`; `termios.h:50-79`; `ttydefaults.h` | One byte, special powers? |
| K-080 | 4 flag groups | K | S | 13 §1.3/§2.2* | `modes.c:65-175` | Which switches, in which order? |
| K-081 | capdb (termcap/terminfo) | D | S | 13 §1.4* | `etc/termcap,termcap.big`; `tput,tic,infocmp` | Terminals differ — where recorded? |
| K-082 | loadfont/loadkeys/screendump | T | S | 13 §1.5* | `minix/commands/loadfont,loadkeys,screendump,term,tget` | Font, keys, screen dump? |
| K-083 | 10-cmd term contract table | I | S | 13 §1.6* | plan §5.2 13-row | Per-command contract? |
| K-084 | mount as graft | C | S | 14 §1.1/§2.1* | `minix/commands/mount.c:17,41-60,168`; `sbin/mount/` | 3 params, which flags? |
| K-085 | fstab 6 fields | D | S | 14 §1.2* | `etc/newfstab.sh` (single-arg) + fstab | Static marriage of what? |
| K-086 | fsck pass queue | M | S | 14 §1.3/§2.2* | `fsck.c:254` zero-skip; `preen.c,progress.c`; `fsck.mfs,fsck_ext2fs/` | Ordered checkup? |
| K-087 | MBR geometry | K | S | 15 §2 (`bootblock.h`)* | `bootblock.h:446/0xAA55/4 entries/0x80; :703-714` | Where is the table, what magic? |
| K-088 | part/fdisk/partition family | M | S | 15 §2* | `part.c:47,300,384-482,550-551,691-702`; `fdisk.c`; `autopart,repartition,format,devsize` | Cut, name, format? |
| K-089 | newfs_*/makefs/mkfs | T | S | 15 §2* | `sbin/newfs_ext2fs,msdos,udf,v7fs`; `usr.sbin/makefs` | Which maker for which FS? |
| K-090 | dd operands | I | S | 16 §2 (`args.c:105-121`)* | `bin/dd/` + `conv.c` | Block copy with conversion? |
| K-091 | ISO marker + tools | M | S | 16 §2* | `isoread.c:41` (`CD001`); `writeisofs,dosread,vol,eject,cdprobe` | Whole-disc搬运? |
| K-092 | ramdisk/vnconfig | M | S | 16 §2.3* | `ramdisk,loadramdisk,rawspeed`; `usr.sbin/vnconfig` | Memory discs and vnodes? |
| K-093 | incremental-only backup | K | S | 17 §1.1/§2.1* | `backup.c:52-54` (4096/512/256) | Copy what changed? |
| K-094 | remsync/synctree agree | M | S | 17 §1.2/§2.2* | `remsync.c:~101,~177,~285,~1472`; `synctree.c:55,~200` | Two trees, one truth? |
| K-095 | midnight retention window | K | S | 17 §1.3/§2.3* | `cleantmp.c:31-32,56+` | Why align to midnight? |
| K-096 | progress + tape language | I | S | 17 §1.4/§2.4-2.5* | `progressbar.c:34,11+`; `mt.c:~42,78` | Visible waiting, sequential media? |
| K-097 | rotate/fix/updateboot scripts | T | S | 17 §2.6* | `rotate.sh`; `fix.c:38,11-14`; `updateboot.sh,update_asr.sh` | Logs, diffs, boot refresh? |
| K-098 | iface/route/echo/name 4 Qs | C | S | 18 §1* | `sbin/ifconfig,route,ping,ping6`; `usr.sbin/arp,ndp,traceroute,traceroute6,netstat,rdate,rtadvd`; `netconf,slip,swifi` | What must hold before connected? |
| K-099 | flags/route/cksum mechanics | M | S | 18 §2.1-2.3* | `ifconfig.c:~1050,~1059,~1173`; `route.c:~642`; `ping.c:1266,~897,~1029`; `arp.c:315,350,434` | Read/change/write; same socket, two uses? |
| K-100 | trace + status display | M | S | 18 §2.4* | `traceroute.c:~721,~1350,:472`; `netstat.c:~866` | TTL bound, display choice? |
| K-101 | inetd sleep-until-guest | M | S | 19 §1.1/§2.1* | `inetd.c:276,306,339-348` | 64 sockets, 20 args, builtins? |
| K-102 | syslog facility×severity | D | S | 19 §1.2/§2.2* | `syslogd.c:60-61,130-137,~1488` | How is each message sorted? |
| K-103 | fetch/ftpd/telnetd/zmodem run-errands | I | S | 19 §1.3/§2.3* | `fetch.c:~859`; `libexec/ftpd,telnetd/`; `zmodem/` | URL, session, queue? |
| K-104 | mail/lp/lpd queues | M | S | 19 §1.3* | `usr.bin/mail`; `minix/commands/mail,lp,lpd,fetch` | Deliver vs queue? |
| K-105 | inet/syslog/inet.conf | D | S | 19 §2* | `etc/inetd.conf,syslog.conf,inet.conf` | Which config drives which daemon? |
| K-106 | version/root/clock gazes | M | S | 20 §1.1/§2.1-2.2* | `version.sh`; `printroot.c:25,27,45`; `readclock.c:55,59,73,122,164` | Version file, root scan, 5-letter verbs? |
| K-107 | intr bg+alarm | M | S | 20 §1.2/§2.3* | `intr.c:19,51,140` | Time-box a command? |
| K-108 | sysctl dotted tree | D | S | 20 §1.3/§2.4* | `sysctl.c:~520` (`CTLTYPE_NODE`) | Dotted names and assignment? |
| K-109 | ldd static listing (A-1) | A | S | 20 §1.4/§2.4* | `usr.bin/ldd/ldd.c:~93` | No ld.so — what does ldd print? |
| K-110 | profile/zic/zdump/system.conf | T | S | 20 §2* | `profile,sprofalyze,sprofdiff`; `zic,zdump`; `etc/system.conf` | Profiling, zones, config? |
| K-111 | 3 sets + confirm-skip | M | S | 21 §1.1/§2.1* | `pkgin_sets.sh`; `pkgin_cd.sh` (`packages/{release}/{arch}/All`, `pkg_summary.bz2`); `pkgin_all.sh` | Where does software come from? |
| K-112 | installboot dispatch | M | S | 21 §1.3/§2.3* | `installboot.c:~61,~246,~276` (32 sectors) | Per-FS stage files? |
| K-113 | gcov-pull/mkdep/nbperf/genassym/mk.conf | T | S | 21 §1.4/§2.4-2.6* | `gcov-pull.c` (4 MB); `mkdep.c` (`.depend`); `nbperf.c:139-144`; `genassym.sh`; `mk.conf` (5 lines) | Coverage, deps, hashing, symbols? |
| K-114 | factor 2 paths | M | S | 22 §2.1* | `factor.c:~184,~110,:268` | Output shape, big-number path? |
| K-115 | primes/caesar/morse/pig | M | S | 22 §2.2-2.5* | `primes/pattern/pr_tbl/spsp.c`; `caesar.c:~82,:86,:125`; `morse.c:~96,:140,:219`; `pig.c:~103,:133` | Interval, rotation, dots, pig-latin? |
| K-116 | number/arithmetic/bcd/banner/ppt | M | S | 22 §1.3-1.4/§2.5-2.6* | `number.c`; `arithmetic.c:~99-107`; `bcd.c`; `banner.c:58,~1057,~1064-1066`; `ppt.c` | Words, quiz, big display? |
| K-117 | grid+escape screen | C | S | 23 §1.1* | `worm.c:~120`; `rain.c:~87,~117-118`; `colorbars.c` | Move on a grid? |
| K-118 | 7 shapes + clear | M | S | 23 §1.2/§2.1* | `tetris/shapes.c:46-53,82,97`; `tetris.h:54-56`; `tetris.c:62,109-118` | Fit test, place, clear? |
| K-119 | snake/worm/rain walks | M | S | 23 §1.3/§2.2* | `snake/`; `worm.c:~203-204` | Three crawler walks? |
| K-120 | dungeon rooms | M | S | 23 §1.4/§2.3* | `rogue.h:54-56,293` (`MAXROOMS 9`); `room.c` | Rooms and corridors? |
| K-121 | adventure/monop worlds | M | S | 24 §1.1-1.2/§2.1* | `hdr.h:~78,~101,~116`; `vocab.c:~72`; `monop/{monop,cards,houses,jail}.c` | Directions, money, dice? |
| K-122 | fortune/wtf抽签 | M | S | 24 §1.3/§2.3-2.4* | `fortune.c:~267,~980`; `datfiles/`; `wtf` (`-o/-f`, skip `is`) | Draw and lookup? |
| K-123 | fish/random lots | M | S | 24 §1.4/§2.2/§2.4* | `fish.c:61,64,83,160`; `random.c:~107,:126`; `wargames.sh` | Books, denominator? |
| K-124 | install 4 layers + PATH | D | S | 99 §0* | `bin,sbin,usr.bin,usr.sbin,minix/commands,games,etc/` (49 configs) | Which layer, which PATH? |
| K-125 | layer contract (rt+sys-top only) | K | S | 99 §1* | `os/libs/minix-rt/{crt0.rs,handoff.rs}`; `minix-sys` top fns; `echo.rs` sample | What may a command depend on? |
| K-126 | stdio landing (A-2) | A | S | 99 §1 (stdio list)* | `core::fmt/alloc::format` + `write/read`; `ioctl` for termios; socket for net | Where did "stdio" go? |
| K-127 | POSIX-vs-C benchmark | K | S | 99 §2* | POSIX contracts vs C truth | Which wins on conflict? |
| K-128 | Requires template | T | S | 99 §3* | echo/cat/ls/sh rows | Per-command minimal API? |
| K-129 | DESCRIBE build face | T | S | 99 §0 + plan §5.2 99-row* | `minix/commands/DESCRIBE/DESCRIBE.sh,Makefile` | C build metadata vs Rust domain matrix? |
| K-130 | non-server spine framing | C | S | 00 §1* | plan §1.2 chain diagram | Why delivery-chain, not server? |
| K-131 | dual-track load+depend | C | S | 00 §1.2* | 99 §1 contract table | How does a command start vs what can it call? |
| K-132 | 24-domain matrix | D | S | 00 §1.3* | §0.1 crate census | Which crate owns which doc? |
| K-133 | `sbin/sysctl` vs `usr.sbin/sysctl` dual | N | — | Cover in N-20 §2.4 | TO-VERIFY (§3 G-3): both paths listed in plan §5.2; on-disk check pending | One tool or two? |
| K-134 | `services_mkdb` existence/shape | N | — | Cover in N-03 §2.4 | TO-VERIFY: plan §5.2 assigns `usr.sbin/services_mkdb`→04; `ls minix3/usr.sbin/` check pending | Real command or stale row? |
| K-135 | rc.d keyword files (DAEMON/DISKS/LOGIN/NETWORKING/SERVERS) | N | — | Cover in N-01 §2.5 | `ls minix3/etc/rc.d/` (verified present) | Markers or scripts? |
| K-136 | cron allow/deny files | N | — | Cover in N-02 §2.4 | `minix3/minix/commands/cron/` listing (`cron.8,tab.*` — allow/deny check pending) | Who may schedule? |
| K-137 | IPv6 parity (ping6/traceroute6/ndp) | N | — | Cover in N-18 §2.3-2.4 | `sbin/ping6`; `usr.sbin/traceroute6,ndp` (existence verified in plan rows) | Same contract as v4? |
| K-138 | dual `mail` (usr.bin vs minix/commands) | N | — | Cover in N-19 §2.3 | plan §5.2 assigns both to 19 | Same tool twice — diff table? |
| K-139 | dual `mount` diff table | N | — | Cover in N-14 §2.1 | `sbin/mount/` + `minix/commands/mount/` (both verified) | NetBSD-family vs Minix-native? |
| K-140 | `tip/cu/ssh` absence | N | — | Cover in N-19 scope note | TO-VERIFY: negative `ls` check pending | No dialer/ssh in base? |

Pool statistics: 140 items — C:9, M:61, D:19, I:18, K:17, A:6, T:10.
Stock 132 / New 8. By current doc: 01:8, 02:7, 03:8, 04:6, 05:7, 06:7, 07:7, 08:6,
09:5, 10:6, 11:5, 12:5, 13:6, 14:3, 15:3, 16:3, 17:5, 18:3, 19:5, 20:5, 21:3,
22:3, 23:4, 24:3, 99:6, 00:3, new:8.

## 3. Coverage Audit

Topic universe built from: (a) C symbols/dirs in §1 tables; (b) OS-generic concepts
(state machines, address/name spaces, permission models); (c) non-C artifacts (§7
checklist); (d) stage-boundary contracts (plan §5.4 exclusion table).

### 3.1 Coverage gaps (universe topics with no home)

| # | Topic | Evidence | Disposition → new home |
|---|-------|----------|------------------------|
| G-1 | `sbin/sysctl` vs `usr.sbin/sysctl` duality | plan §5.2 lists both; `20` §2.4 cites only one face | K-133 → N-20 §2.4 diff-note (verify on disk first) |
| G-2 | `services_mkdb` reality check | plan §5.2 `usr.sbin` row assigns it to 04; `04` never mentions it | K-134 → N-03 §2.4 (verify; delete row if stale) |
| G-3 | rc.d keyword files | `ls` shows `DAEMON DISKS LOGIN NETWORKING SERVERS` among 33 entries | K-135 → N-01 §2.5 |
| G-4 | cron allow/deny | `cron/` has `cron.8,tab.c/h,misc.c/h` — access-control files unchecked | K-136 → N-02 §2.4 |
| G-5 | IPv6 parity statement | ping6/traceroute6/ndp assigned but never contrasted with v4 | K-137 → N-18 §2.3–2.4 |
| G-6 | dual `mail`, dual `mount` diff tables | Both dualities verified on disk; docs describe one face each | K-138 → N-19 §2.3; K-139 → N-14 §2.1 |
| G-7 | `tip/cu/ssh` absence note | No negative check on record | K-140 → N-19 scope note (verify) |
| G-8 | Requires backfill for 19 docs | `Requires` hits only in 06/07/08/09/22 (+99 template); 02/03/04/05/10–21/23/24 have zero | P1-1 closure → N-doc §5 each (use T-09's current `minix-sys` list, not todo.md's stale list) |
| G-9 | `00` too thin to navigate by (67 lines, defers everything to plan §5) | §0 headings: concept + scale + nav + boundary only | Expand per N-00 contract (§5) |

### 3.2 Duplication (one topic, several tellings → keep primary, others cite)

| # | Topic | Homes | New primary | Others become |
|---|-------|-------|-------------|---------------|
| R-1 | init internals (states, session tables, utmp) | 18-`01` §§2.1–2.4 + `09-stage-init` (16 docs, v1 2026-09-18) | `09-stage-init` | N-01 keeps summary + rc/script face only; init mechanics shrink to ≤ 40 lines + pointer |
| R-2 | `mount` dual faces | `14` §2.1 (+ `sbin/mount` vs `minix/commands/mount`) | N-14 §2.1 diff table | Single telling, no split |
| R-3 | `mail` dual faces | `19` (both rows assigned, one face described) | N-19 §2.3 diff table | Same |
| R-4 | termios/terminfo fragments | `03` (needs), `05` (needs), `13` (owns), `23` (uses) | N-04 (owns all) | N-05/N-23 cite; no re-explanation (first-appearance-complete) |
| R-5 | stdio/"what is stdio" explanation | 99 §1 owns; plan `:206,:407` rephrase; todo P0-1 | 99 §1 | N-docs cite one line; never re-derive |
| R-6 | `test` builtins vs external | `05` §1.4 (builtin face) + `06` §2.1 (external face) | Keep both, contract the seam | N-06 §1.4 names the builtin wrapper delegation explicitly |

### 3.3 Out-of-scope (taught elsewhere; each row names the owner)

devmand server → `11-stage-devman` (MAKEDEV/mknod stay); RS service-management
server side → `03-stage-rs` (service/svrctl clients stay); TTY/readclock/audio/NIC
drivers → `16-stage-drivers`; libc/libsys impl → `14-stage-runtime` (commands consume
only top wrappers); lwip/uds stack → `17-stage-net`; host toolchain (`tools/`,
`gnu/`, `external/`, `tests/`) → build chain; xorg → deferred; FS server internals →
`15-stage-fs`; `ld.elf_so` → A-1 will-not-build; `etc/devmand`, `etc/xorg.conf` →
owner stages; `etc/root` home template → N-05 skel face (explicit, not dropped).

### 3.4 Non-C ten-item answers (where each is taught, or why not here)

Link/load: static binaries, no `ld.so` → N-99 (A-1) + N-20 (`ldd` face).
Image/memory layout: boot slots are kernel business → pointer to `01-stage-kernel`;
command install layers + PATH → N-99 (K-124).
Asm/trap entry: `minix-rt` (`crt0.rs`, `handoff.rs`) → N-99 (K-125), detail in
`14-stage-runtime`. Boot assembly: rc scripts are the command layer's boot →
N-01 (spine). Build/toolchain: `DESCRIBE.sh` + per-dir Makefiles → N-99 (K-129);
host toolchain excluded (§3.3). Cross-module interfaces/wire: `minix-sys` top
wrappers + Requires per command → N-99 + every N-doc §5. Error paths: POSIX codes
preserved, `Result`+errno mapping (A-5) → N-99 + per-command contract rows.
Shutdown/exit: N-01 (T-10). Concurrency: commands are single-threaded; thread/futex
model lives with `14-stage-runtime`/`04-stage-pm` (edge E-THREAD-MODEL, not here).
Test infra: per-doc §6 + `cargo test -p minix-*` (prefix corrected per todo C-4);
`tools/check-command-boundary.sh` guards the layer rule (todo C-7).

## 4. New Catalog

Core rebuild decision: **move devices (old 04) and terminal (old 13) ahead of login**,
so the spine reads boot → services → devices → terminal → login → shell with zero
forward references. Eleven files renumber; fourteen keep numbers. Old→new map is §6.

| New # | Title | One-line charter | Group |
|-------|-------|------------------|-------|
| N-00 | Command landscape & delivery chain | Map of the whole face: spine, dual track, 24-domain matrix, reading paths | overview |
| N-01 | init, rc scripts, shutdown | First user process + 3-layer rc + order tool + stop/restart/install (init guts → 09-stage-init) | spine |
| N-02 | Service management & scheduling | Looked-after processes + calendar/one-shot/heartbeat | spine |
| N-03 | Device nodes & system databases | /dev truth + 13-name-tables + one query entry (moved up: login needs names) | spine foundation |
| N-04 | Terminal control & capability DB | termios + termcap/terminfo + stty family + font/keys/dump (moved up: getty needs it now) | spine foundation |
| N-05 | Login chain & password DB | Three handoffs, one verification + toolchain + session luggage | spine |
| N-06 | Shell family & environment | Program+language, 3 dialects, 3 startup paths, builtins, text layer | spine end / interaction start |
| N-07 | File-operation commands | 4 families + 34 contracts + Requires | family file/text |
| N-08 | Text filters & data tools | Line currency + 39 contracts + Requires | family file/text |
| N-09 | grep & sed: pattern face | BRE/ERE + VM engine + 2 contracts | family file/text |
| N-10 | Editors | ed sessions + mined screen + no-vi suspense | family file/text |
| N-11 | Doc typeset, man, dev-aux | 3 printing steps + index sisters + 35 contracts + i18n defer | family file/text |
| N-12 | Compress & archive | 3 shrinkings, 2 packings + 10 contracts | family file/text |
| N-13 | Process & session tools | 3 watching eyes + 28 contracts | family session |
| N-14 | Mount & filesystem check | Graft + fstab + pass queue (+ dual-mount diff) | family storage |
| N-15 | Partition & format | Cut, name, format (+ prep-answers-use) | family storage |
| N-16 | Image & media tools | Whole-disc搬运 + memory discs | family storage |
| N-17 | Backup & maintenance | Incremental, agree, retain, visible waiting | family storage |
| N-18 | Network config & diagnosis | 4 connectivity questions (+ IPv6 parity) | family network |
| N-19 | Network services & daemons | Sleep-until-guest + sort-every-message + errands (+ dual-mail diff) | family network |
| N-20 | Minix-specific & system info | 4 gazes + dotted tree + static listing (+ sysctl duality note) | family system |
| N-21 | Packages & build tools | 3 sets + boot install + dep/symbol helpers | family system |
| N-22 | stdio games | Prove the system alive by talking | acceptance (skippable) |
| N-23 | Terminal games | Prove the cursor moves | acceptance (skippable) |
| N-24 | Text games | Prove stories keep people | acceptance (skippable) |
| N-99 | Global concepts | Install face + layer contract + POSIX-vs-C + Requires master + build face | global |

Reading paths: main `N-00→01→02→03→04→05→06`, then any family in any order
(file/text `07–12`, session `13`, storage `14–17`, network `18–19`, system `20–21`);
branch `22–24` skippable; `99` consulted alongside, read fully at the end.
Parallel groups give framework first (N-08 before N-09; N-07 before N-08;
N-14 before N-15/16/17; N-18 before N-19) with a worked representative + diff
tables inside each group doc.

## 5. Per-Doc Contracts (B-phase task orders)

Uniform body skeleton (fixes the `4.5-under-5` misnumbering in old 07/08 and the
uneven `4.5` in old 06): §1 concept · §2 C analysis · §3 Rust decisions ·
§4 implementation · §5 contract + Requires · §6 tests · §7 transition · §8 refs.
Banned from bodies: `附：验证记录` (review scratch → archive) and `批次随记`
(batch log → todo.md). Length caps: group docs ≤ 350 lines; N-00/N-99 ≤ 200.

### N-00 Command landscape & delivery chain (old 00, expand 67 → ~180)

- Charter: the reader's map — finishes this chapter able to locate any command.
- Covers: K-130, K-131, K-132. Ground truth: §0.1 crate census; plan §5 counts.
- Not: any command detail (delegate by number); init mechanics (→ 09-stage-init).
- Prereqs: none. Post: all reference it.
- Knowledge rows: full spine diagram + dual-track + domain matrix (self-contained;
  retire the "no second copy" habit — overview states first, others cite).
- Accept: a reader names any of 10 sampled commands' home chapter without searching.

### N-01 init, rc scripts, shutdown (old 01, slim 265 → ~220)

- Charter: how PID 1 brings the system to multi-user; init guts shrink to ≤ 40
  lines + pointer (R-1).
- Covers: K-001 (summary only), K-002, K-004, K-005, K-006, K-007, K-008, K-135.
- Not: state-machine/session-table internals (→ 09-stage-init 01–11); service
  protocol (→ N-02); getty spawn detail (→ N-05); mount verbs (→ N-14).
- Prereqs: N-00. Post: N-02, N-05 cite the rc call-sites.
- Ground truth: T-01–T-05, T-10; `etc/rc.d/` keyword files enumerated (G-3).
- Accept: reader writes which layer a new boot variable belongs in, and predicts
  `rcorder` output order for a 3-script fixture.

### N-02 Service management & scheduling (old 02, keep ~210)

- Charter: looked-after processes by day, automatic errands by night.
- Covers: K-009–K-015, K-136. Not: RS server side (→ 03-stage-rs); daemon bodies
  (→ N-19 for net daemons; cron daemon face stays as scheduler user).
- Prereqs: N-01 (rc call-sites). Post: N-05 (watched getty relation), N-21.
- Ground truth: `minix-service.c`, `service` script, `svrctl.c`, `cron/tab.c:285-360`,
  `crontab.c:70-122`, `at.c:26-80`, `update.c`; allow/deny check (G-4).
- Accept: reader parses 5 cron lines incl. `?` and colon-step, and states what
  `svrctl` refuses for non-root.

### N-03 Device nodes & system databases (old 04 → N-03, keep ~185)

- Charter: the foundation login stands on — devices are files, names are contracts.
- Covers: K-024–K-029, K-134. Not: devman server (→ 11-stage-devman); termcap
  table (→ N-04); net-DB consumers (→ N-18, now backward-cited).
- Prereqs: N-00. Post: N-05 (consumer), N-18.
- Ground truth: `MAKEDEV.sh:56-110`, `mknod.c/pack_dev.c`, `dev_mkdb.c:56-98,178-220`,
  `getent.c:98-136`, `etc/mtree/`; `services_mkdb` verify-or-delete (G-2).
- Accept: reader creates the right node kind from a 4-row spec and queries 3 DBs.

### N-04 Terminal control & capability DB (old 13 → N-04, keep ~185)

- Charter: the tactile layer — everything on screen passes through here; sole owner
  of termios/capability knowledge (R-4).
- Covers: K-078–K-083. Not: TTY driver (→ 16-stage-drivers); line discipline impl
  (→ runtime); terminfo ship decision implementation (suspended, states terms).
- Prereqs: N-03. Post: N-05, N-06, N-23 cite; never re-explained after.
- Ground truth: `cchar.c`, `modes.c:65-175`, `stty.c:137`, `termios.h:50-79`,
  `ttydefaults.h`, `etc/termcap*`; termios wire now in `minix-types`
  (`types/termios.rs`) + `tcgetattr/tcsetattr` in `minix-sys` (C-23 batch).
- Accept: reader predicts `stty` display for a flag fixture and resolves a
  capability query by the three-method rule.

### N-05 Login chain & password DB (old 03 → N-05, keep ~220)

- Charter: from blank screen to own shell — three handoffs, one verification.
- Covers: K-016–K-023. Not: hash/auth backend (→ 04-stage-pm); shell startup files
  (→ N-06, now backward); TTY driver (→ drivers).
- Prereqs: N-01 (who spawns), N-03 (names), N-04 (attrs) — all backward now.
- Post: N-06 (next stop), N-13 (record reader).
- Ground truth: T-06, T-07; `ttys/gettytab/master.passwd/skel` instances.
- Accept: reader traces a login line-by-line across the three handoffs and states
  where a locked account fails.

### N-06 Shell family & environment (old 05 → N-06, keep ~240)

- Charter: the translator — program and language; text layer done, exec layer
  named (K-036: fork/exec/pipe2/fcntl gaps owned by runtime batches).
- Covers: K-030–K-036. Not: tool bodies (N-07–N-24); job-control process groups
  (→ 04-stage-pm/thread-model item).
- Prereqs: N-05 (handoff in), N-04 (line discipline). Post: N-07 (first errands).
- Ground truth: T-08; `minix-shell` lexer/expand/redir/script (36 tests).
- Accept: reader classifies 8 startup scenarios into the 3 paths and states which
  3 syscalls block the executor batch.

### N-07 File-operation commands (old 06 → N-07, keep ~240)

- Charter: move, view, judge files — 4 families, 34 contracts, Requires each (G-8).
- Covers: K-037–K-043. Not: text processing (→ N-08); storage mgmt (→ N-14–17).
- Prereqs: N-06 (builtin vs external seam R-6). Post: N-08, N-14, N-22.
- Ground truth: `test.c`, `chmod.c`, `ls/cp/find`, small pure fns; `minix-fileops`
  (mode/testexpr/path/echo/…; echo end-to-end template).
- Accept: Requires column complete for all 34; `ls` row cites `getdents+stat`
  (now existing per T-09).

### N-08 Text filters & data tools (old 07 → N-08, keep ~235)

- Charter: lines are currency — take, shape, count, compare; keep the §1.5
  reading-order note (now structurally true: N-08 before N-09).
- Covers: K-044–K-050. Not: regex engine (→ N-09); editors (→ N-10).
- Prereqs: N-07. Post: N-09, N-11.
- Ground truth: `uniq.c`, `cut.c:88`, `wc.c`, `tr.c`, `sort.c` (exec留白);
  `minix-textfilter` (203+ tests incl. diff engine batch).
- Accept: Requires complete for all 39; diff/patch engine paragraph present.

### N-09 grep & sed (old 08 → N-09, keep ~200)

- Charter: describe lines in one sentence — BRE/ERE + VM engine + two tools.
- Covers: K-051–K-056. Not: editor regex use (→ N-10).
- Prereqs: N-08 (line intuition; self-contained theory). Post: N-10.
- Ground truth: `grep.c/util.c:205`, `compile.c/process.c`; `minix-regex` VM.
- Accept: reader states match semantics (K-052/053) and exit-code rule; Requires
  rows for grep/sed complete.

### N-10 Editors (old 09 → N-10, keep ~195)

- Charter: live inside lines vs pass over them — ed sessions + mined screen.
- Covers: K-057–K-061. Not: shell line editing (→ N-06); terminal control (→ N-04).
- Prereqs: N-09 (shared grammar). Post: N-11.
- Ground truth: `ed/main.c` dispatch/ranges/ops; `mined1/2.c`; no-vi suspense terms.
- Accept: 7 address forms demoed; Requires rows for ed/mined complete.

### N-11 Doc typeset, man, dev-aux (old 10 → N-11, ~225; batch note evicted)

- Charter: printed layer — 3 steps to viewable docs + self-describing manuals.
- Covers: K-062–K-067. Not: typeset engine impl (留白); spelling dict data; i18n
  impl (A-8 defer states terms).
- Prereqs: N-08, N-09. Post: N-12 (man compression consumes it).
- Ground truth: `man.c/manconf.c`, `makewhatis.c`, `cal.c`; `minix-doctools`
  (cal/whatis/apropos wired).
- Accept: 35 contracts complete; i18n defer paragraph present with A-8 ref.

### N-12 Compress & archive (old 11 → N-12, keep ~180)

- Charter: press small, pack as stream, prove intact.
- Covers: K-068–K-072. Not: container multi-member framing (later); bzip2 chain,
  gzip transform coding, DES (each names its later home).
- Prereqs: N-07. Post: N-13.
- Ground truth: `compress.c`, `uuencode.c`, `shar.sh`, big-3 duty rows.
- Accept: 10 contracts complete; each留白 names an owner stage/batch.

### N-13 Process & session tools (old 12 → N-13, keep ~205)

- Charter: ward layer — which processes, who logs in, signals how.
- Covers: K-073–K-077. Not: terminal control (→ N-04); net sessions (→ N-19);
  kernel table reads (later kernel-interface batch).
- Prereqs: N-06, N-05 (record writer). Post: N-04 already taught — transition
  points backward ("now you can read what you use"); N-14 next.
- Ground truth: `kill.c`, `keyword.c:115-197`, `utmp.h`, `signal.h:52-84`;
  `minix-proctools` (kill/who wired + stamp seam).
- Accept: 28 contracts complete; signal name↔number both directions demoed.

### N-14 Mount & filesystem check (old 14, keep ~150)

- Charter: storage layer — graft, then checkup queue; dual-mount diff table (K-139).
- Covers: K-084–K-086. Not: FS server (→ 15-stage-fs); block drivers (→ drivers).
- Prereqs: N-07, 15-stage-fs server semantics. Post: N-15, N-16, N-17.
- Ground truth: `mount.c:17,41-60,168`, `umount.c`, `fsck.c:254`, `newfstab.sh`.
- Accept: reader writes a valid fstab line and predicts fsck pass order; Requires rows.

### N-15 Partition & format (old 15, keep ~160)

- Charter: prep layer — cut, name, format; §1.0 answers the use-first question (D-1).
- Covers: K-087–K-089. Not: write/format execution (waits block iface); FS innards.
- Prereqs: N-14 (use taught first, prep now). Post: N-16, N-17.
- Ground truth: `bootblock.h`, `part.c`, `fdisk.c`, `newfs_*`, `makefs`.
- Accept: reader decodes an MBR row and picks the maker per FS; Requires rows.

### N-16 Image & media tools (old 16, keep ~150)

- Charter:搬运 whole discs — copy, ISO, memory discs.
- Covers: K-090–K-092. Not: driver impl; eject/probe hardware face; FAT parse (later).
- Prereqs: N-14, N-15. Post: N-17, then network.
- Ground truth: `dd/args.c:105-121`, `isoread.c:41`, ramdisk/vnconfig.
- Accept: reader builds a correct `dd` operand set incl. conversion; Requires rows.

### N-17 Backup & maintenance (old 17, keep ~215)

- Charter: insurance layer — incremental copy, agree at distance, midnight
  retention, visible waiting, tape language, log rotation.
- Covers: K-093–K-097. Not: traversal/clock execution (FS iface batch); tape
  ioctls (→ drivers); screen repaint escapes (→ N-04).
- Prereqs: N-14, N-15. Post: N-18.
- Ground truth: `backup.c:52-54`, `remsync.c`, `synctree.c:55`, `cleantmp.c:31-32`,
  `progressbar.c:34`, `mt.c:~42,78`, `rotate.sh`, `fix.c`, update scripts.
- Accept: reader states the three retention numbers and the chunk constants; Requires.

### N-18 Network config & diagnosis (old 18, keep ~195)

- Charter: connectivity layer — 4 questions before connected (+ IPv6 parity K-137).
- Covers: K-098–K-100. Not: stack/socket innards (→ 17-stage-net); driver TX/RX.
- Prereqs: N-04 (display), 17-stage-net socket face. Post: N-19.
- Ground truth: `ifconfig.c`, `route.c`, `ping.c`, `arp.c`, `traceroute.c`, `netstat.c`.
- Accept: reader diagnoses 3 down-scenarios to the right tool; Requires rows.

### N-19 Network services & daemons (old 19, keep ~180)

- Charter: husbandry after connectivity — sleep-until-guest, sort every message,
  run errands (+ dual-mail diff K-138; absence note K-140).
- Covers: K-101–K-105. Not: protocol state machines; fork/setuid execution (proc
  iface batch); mail/spool backend execution (queue semantics only).
- Prereqs: N-18. Post: N-20.
- Ground truth: `inetd.c`, `syslogd.c`, `fetch.c`, `ftpd/`, `telnetd/`, `zmodem/`,
  `inetd.conf/syslog.conf/inet.conf`.
- Accept: reader writes both conf files' minimal working forms; Requires rows.

### N-20 Minix-specific & system info (old 20, keep ~185)

- Charter: observation layer — 4 gazes + dotted tree + static listing.
- Covers: K-106–K-110, K-133. Not: driver innards; kernel param effect logic;
  audio capture/play (A-13 defer).
- Prereqs: N-00. Post: N-21, then games.
- Ground truth: `version.sh`, `printroot.c`, `readclock.c`, `intr.c`, `sysctl.c`,
  `ldd.c`, profile/zic/zdump, `system.conf`.
- Accept: sysctl duality resolved (one tool or two, with evidence); Requires rows.

### N-21 Packages & build tools (old 21, keep ~195)

- Charter: supply layer — where software comes from, boot blocks written in,
  deps computed, symbols extracted.
- Covers: K-111–K-113. Not: host toolchain; package transport/disk-write execution;
  perfect-hash impl (method choice only).
- Prereqs: N-02 (service face first, supply after). Post: N-22 acceptance.
- Ground truth: `pkgin_*.sh`, `installboot.c`, `postinstall`, `gcov-pull.c`,
  `mkdep.c`, `nbperf.c`, `genassym.sh`, `mk.conf`.
- Accept: reader replays the cd-repo lookup order and the 32-sector rule; Requires.

### N-22 stdio games (old 22, keep ~230)

- Charter: acceptance group 1 — prove alive by talking; Requires complete.
- Covers: K-114–K-116. Not: terminal control (→ N-23).
- Prereqs: N-07 (stdio face). Post: N-23.
- Ground truth: `factor,primes,caesar,morse,pig,number,arithmetic,banner,bcd,ppt`;
  `minix-stdio-games` (9 thin shells wired; banner waits bitmap asset — stated).
- Accept: per-game contracts + Requires; banner wait stated with owner.

### N-23 Terminal games (old 23, keep ~185)

- Charter: acceptance group 2 — prove the cursor moves; no capdb port (A-2 terms).
- Covers: K-117–K-120. Not: capdb port (decision states terms); key read + true
  paint (per-game binaries); monsters/items/score (later).
- Prereqs: N-04. Post: N-24.
- Ground truth: `tetris/`, `worm/`, `rain/`, `colorbars/`, `snake/`, `rogue/`;
  rotation-formula and snake-tail rules stated as invariants.
- Accept: board/geometry constants asserted with tests; Requires rows.

### N-24 Text games (old 24, keep ~190; batch note evicted)

- Charter: acceptance group 3 — keep people with stories; data-vs-rules split.
- Covers: K-121–K-123. Not: big-text data landing (A-10 decision); read/player
  interaction (binaries); deal scripts (later).
- Prereqs: N-07, N-11. Post: N-99.
- Ground truth: `adventure/`, `monop/`, `fortune/` + `datfiles/`, `fish.c`,
  `random.c`, `wtf`, `wargames.sh`; wtf/fortune/random wired.
- Accept: data-format decisions recorded per game; Requires rows.

### N-99 Global concepts (old 99, expand 98 → ~190)

- Charter: the stage's constitution — install face, layer contract, POSIX-vs-C
  benchmark, Requires master table, build face. Read alongside everything, fully
  at the end.
- Covers: K-124–K-129. Not: any mechanism detail (→ N-01–N-24).
- Prereqs: all (consulted throughout). Post: none.
- Ground truth: 49-config `etc/` map; layer table (commands → `minix-rt` +
  `minix-sys` top only; `minix_sys::ipc` forbidden — enforced by
  `tools/check-command-boundary.sh`); stdio landing list; 4-row Requires examples
  extended to a master per-API consumer count (feeds 14-stage-runtime order).
- Accept: a contributor answers "where do I add X" for 5 sampled Xs without
  opening another chapter.

## 6. Change Table

Ops use old→new file names. Stock direction gives destinations (§2 IDs);
new direction gives source anchors. Anything without a destination is listed
under Delete-with-reason — there is exactly one class (review scratch).

| Op# | Op | Old → New | Reason | Knowledge |
|-----|----|-----------|--------|-----------|
| C-01 | move-up | `04-device-database.md` → `03-…` (N-03) | Login needs names before it runs; kills old 04→18 forward ref | K-024–K-029 → N-03 §§1–2; K-134 → N-03 §2.4 |
| C-02 | move-up | `13-terminal-termios.md` → `04-…` (N-04) | getty needs termios at claim time; kills old 03→13, 05→13 refs | K-078–K-083 → N-04; R-4 single-owner rule |
| C-03 | move-down | `03-login-passwd.md` → `05-…` (N-05) | All prereqs (N-01/N-03/N-04) now earlier | K-016–K-023 → N-05 |
| C-04 | shift +1 | `05-shell-family.md` → `06-…` | Spine order | K-030–K-036 → N-06 |
| C-05 | shift +1 | `06-file-ops.md` → `07-…` | Keep file→text→pattern chain adjacent | K-037–K-043 → N-07 |
| C-06 | shift +1 | `07-text-filter.md` → `08-…` | Framework before pattern; kills 07→08 forward ref | K-044–K-050 → N-08 (§4.5→§5 renumber) |
| C-07 | shift +1 | `08-grep-sed.md` → `09-…` | Follows its framework | K-051–K-056 → N-09 (§4.5→§5 renumber) |
| C-08 | shift +1 | `09-editors.md` → `10-…` | Chain order | K-057–K-061 → N-10 |
| C-09 | shift +1 | `10-doc-man-tools.md` → `11-…` | Chain order; evict batch note → todo | K-062–K-067 → N-11; 批次随记 → todo.md batch log |
| C-10 | shift +1 | `11-compress-archive.md` → `12-…` | Chain order | K-068–K-072 → N-12 |
| C-11 | shift +1 | `12-process-tools.md` → `13-…` | Session group stays before storage | K-073–K-077 → N-13 |
| C-12 | slim | `01-init-rc-scripts.md` → N-01 (same number) | R-1: mechanics → 09-stage-init; keep rc/script face + G-3 | K-001 summary ≤40 lines; K-002–K-008, K-135 stay |
| C-13 | keep | `02-service-scheduler.md` → N-02 | No order fault; add G-4 | K-009–K-015 + K-136 |
| C-14 | keep×11 | `14–24` numbers unchanged | No forward refs found in 14–24 (all prereqs backward or cross-stage) | Add G-5/G-6/G-7, G-8 Requires |
| C-15 | expand | `00` → N-00 (~180 lines) | G-9: overview must stand alone | K-130–K-132 self-contained |
| C-16 | expand | `99` → N-99 (~190 lines) | Requires master + consumer counts (feeds runtime order) | K-124–K-129 |
| C-17 | delete | `附：验证记录` in all 24 docs | Review scratch, not reader content; single-semantic rule | Archive whole (B-phase moves to `archive/`), zero knowledge loss (process notes, not facts) |
| C-18 | relocate | `06 §4.5` + `07/08 §4.5-under-§5` → uniform §5 | Section-number discipline (§5 header) | Contract rows move verbatim, renumbered |
| C-19 | new-content | 8 new items K-133–K-140 | Coverage audit G-1–G-7 | Anchors in §2 table; homes in §5 contracts |

## 7. Missing-Chapters Resolution (fixed non-C list, no blanks)

Link/load → N-99 (A-1). Image/layout → N-99 install face + kernel pointer.
Asm/trap → N-99 + 14-stage-runtime. Boot assembly → N-01. Build/toolchain →
N-99 (K-129); host toolchain excluded with reason (§3.3). Cross-module/wire →
N-99 + per-doc §5 Requires. Error paths → N-99 (A-5) + contract rows.
Shutdown/exit → N-01. Concurrency → not here (E-THREAD-MODEL owner named).
Test infra → per-doc §6 + boundary script. No new chapter file is needed: every
gap item already has a numbered home above. This section is therefore complete
by assignment, not by creation.

## 8. Anchor Migration & Breakage Cost

### 8.1 Section migration (old → new; bodies are short so mapping is per-§)

- Old 01 §§1–2.5–2.8 → N-01 (same); §§2.1–2.4 shrink to summary + pointer to
  `09-stage-init/01–11` (rewrite, not move — old paragraphs archive).
- Old 04 all §§ → N-03 (rename only); old 13 all §§ → N-04 (rename only);
  old 03 all §§ → N-05 (rename only); old 05–12 all §§ → N-06–N-13 (rename + §4.5→§5
  for 06/07/08; batch-note eviction for 10).
- Old 14–24 §§ unchanged in place; append: diff tables (N-14 §2.1, N-19 §2.3),
  parity notes (N-18, N-20 §2.4), Requires §5 everywhere (G-8).
- Old 00 → N-00 rewrite (expand); old 99 → N-99 rewrite (expand + master table).
- `附：验证记录` (24×) → `archive/` (delete from bodies, keep on disk).
  Migration type per row: rename (11 files), rewrite-in-place (00/99/01-partial),
  append (14–24), delete-process-material (24 appendices + 2 batch notes).

### 8.2 Reference migration

- Internal: every doc header's `前置依赖`/`不覆盖` filename refs must be rewritten
  to new numbers (11 renames × ~4 refs each ≈ 44 header edits + in-body `见 NN`
  refs; B-phase rewrites bodies anyway, so marginal cost ≈ 0 beyond the rewrite).
- Code comments: `rg` shows `os/commands/*/src/lib.rs` reference crate names and
  doc titles loosely (e.g. boundary notes), not `NN-*.md` filenames — no code
  edits required; verify with `rg -l "13-terminal|03-login|04-device" os/` during B.
- External inbound: `00-master-plan/README.md`, `15-stage-fs/plan.md`,
  `edge3.md`, `14-stage-runtime/doc_rerank_*`, `09-stage-init/doc_rerank_*` mention
  `18-stage-commands` paths. Redirect table (old → new) ships with the consensus
  blueprint; owners update on touch (no flag-day). Counted external hits: README +
  15-plan + edge3 + 09/14 reranks (reranks are ephemeral — ignore after consensus).

### 8.3 Breakage-cost summary

Affected refs total ≈ 44 internal header refs + in-body `见` refs (rewritten inside
the already-planned B rewrite) + 3 durable external files (README, 15-plan, edge3).
Hot spots: N-05 header (3 prereqs change numbers), N-03/N-04 headers (position
change), `00` nav table (full renumber). Batch method: `rg -n "0[1-9]-|1[0-3]-" 
*.md` before/after; every old filename must hit zero. Cost verdict: contained —
the renumber touches names, not knowledge; knowledge moves are already tabulated
in §6 with per-item destinations.

## 9. Verification & Self-Gates

### 9.1 Four mechanical checks

1. Forward-ref scan on NEW catalog: every contract's 前置 points earlier-or-equal
   or cross-stage → PASS by construction (the 5 old violations C-01–C-03/C-06
   eliminated; D-1–D-3 compensated).
2. Dependency DAG: spine chain + family fan-out + 99 global → acyclic (99 is
   consulted, never a prereq). PASS by construction; B-phase re-runs on bodies.
3. Coverage: all 140 pool items have a home (§5) or explicit delete reason (C-17:
   process notes only). New items K-133–K-140 all homed. PASS, modulo 3 verifications.
4. Breakage census: §8.2–8.3 counted with commands. PASS (method + zero-target given).

### 9.2 Self-gates G1–G9

| Gate | Result |
|------|--------|
| G1 C-order checkable (10 spot anchors re-verified: `init.c:141-148`, `rcorder.c:78-83`, rc.d 33 entries, `login.c:116-117`, `builtins.def`, games 24, `etc/` 51, 24 Cargo.toml, `minix-sys pub fn` list, `isoread.c:41`) | PASS |
| G2 pool completeness: every C dir + non-C item homed or excluded with reason | PASS except 3 on-disk verifications (G-1/G-2/G-7 → K-133/K-134/K-140) |
| G3 zero forward refs in new catalog | PASS by construction |
| G4 DAG acyclic | PASS by construction |
| G5 100% coverage: destinations or delete-reasons; new items anchored | PASS (C-17 deletes are process-only) |
| G6 splits/merges destinations (C-12 slim, C-09/C-10 evictions) + new sources | PASS |
| G7 seven-element contracts ×26 | PASS (§5; skeleton rule included) |
| G8 migration covers all changed docs' sections + refs | PASS (§8.1–8.2) |
| G9 assertions anchored (10/10 G1 samples carry file:line) | PASS; speculation labeled TO-VERIFY (3) |

### 9.3 Verdict & rulings requested (B-phase blocked until answered)

Blueprint status: **COMPLETE as a spec, NOT READY for B-phase** — 3 open questions:

- **OQ-1 (renumber)**: approve the 11-file renumber (C-01–C-11)? Alternative is
  keep-numbers + 5 documented forward refs, which violates hard standard 1.
  Recommendation: approve.
- **OQ-2 (09/18 boundary)**: confirm R-1 — init mechanics owned by `09-stage-init`,
  N-01 keeps ≤ 40-line summary + pointer? Alternative: 18 keeps full telling and
  09 trims. Recommendation: 09 owns (16 docs already written, v1 2026-09-18).
- **OQ-3 (stale-todo drift)**: `todo.md` §§2–4 predate the 09-17/09-18 wiring
  batches (`stat/getdents/ioctl` now exist; Requires still un-backfilled in 19
  docs). Confirm B-phase batch 0 = "re-baseline todo.md against T-09 list".
  Recommendation: approve.

Rule-discovery (§5.7 of the review process): one candidate pattern observed —
**"contract-without-Requires drifts stale"**: any dependency claim not tied to a
named `minix-sys` function rots within days (P0-1 → C-1 → C-23 chain). Proposed
rule: every cross-stage dependency sentence must name the function or the edge
ID, or be flagged TO-VERIFY. Offered for the pattern library, not asserted.
