# 并发任务板 A — 三线·服务器、文件系统、网络与命令面

> **这份文件是什么**：`2026-10-18` 把全仓库开放项按**领地互斥**切成三份（A/B/C）中的一份，交给一个独立 agent 并发推进。切分依据是 `coordination/CONCURRENT-WORK-PROTOCOL.md` §2 的四线所有权清单，不是按难度或按阶段切——三份文件的可写路径两两不相交，这是并发不打架的唯一硬保证。
> **本板的领地**：`os/servers/`、`os/fs/`、`os/net/`、`os/commands/`、`os/etc/`、`os/xtask/`，以及 `rewrite-notes/02-stage-vm/`、`03-stage-rs/`、`04-stage-pm/`、`05-stage-vfs/`、`06-stage-sched/`、`07-stage-ds/`、`08-stage-is/`、`09-stage-init/`、`10-stage-mib/`、`11-stage-devman/`、`12-stage-input/`、`13-stage-ipc/`、`15-stage-fs/`、`17-stage-net/`、`18-stage-commands/`、本文件。
> **配套件**：`PARALLEL-20261018-B-LIBDRV.md`（二线 库与驱动）、`PARALLEL-20261018-C-KERN-QEMU.md`（一线 内核/引导，**独占真机窗**）。
> **覆盖口径**：协议 §2 的三线不只是命令面，还包括服务器、文件系统、网络与对应阶段文档。第 1 节是命令面的收口任务，第 1b 节是三线服务器与跨阶段传输族的承接清单；两节合起来才是本板的全量开放项。B 板只领 `os/libs/`（`minix-boot` 除外）与 `os/drivers/`，C 板只领一线与 `os/libs/minix-boot/`。
> **条目状态词**（五值，见协议 §3）：`☐` 未开工 / `🔄` 进行中（已领取，别人别碰）/ `⏸` 等待（必须注明等谁）/ `✅` 完成（必须带日期与提交号）/ `🚫` 维持登记但不排期（必须写明为什么现在不做）。

---

## 0. 你（接手的 agent）开工前的四件事

1. **先复核锚点再领**。下表每一行的「登记处」是切板那一刻的行号快照；`18-stage-commands/todo.md` 是追加式长文件，别的批次一插行号就漂。领之前跑：
   `grep -n "<条目ID>" rewrite-notes/18-stage-commands/todo.md`
   ——「看不到锚点」不等于「没有锚点」，也不等于「账已清」（判例：批次三百五十三凭猜定位，四处全不命中）。
2. **用 `tools/claim.sh` 领，别手搓 worktree**：`bash tools/claim.sh claim <条目ID> qoder`（owner 名请固定用 `qoder`；另外两板分别用 `zcode-ds`、`zcode-glm`，三个 owner 名互不重叠，`claim.sh list` 一眼能看出谁在做什么）。**worktree 路径是分支名去掉 `claim/` 后整段小写**（`tools/claim.sh` 的 `wt_path`），不要按 `ID-owner` 原文拼 `.wt/` 路径；以 claim.sh 打印的路径为准。
3. **一次只修一条**。顺手修旁边的 TODO 属于协议 §2 的「不许替别人实现他的前置」，尤其不许顺手改 `os/libs/`、`os/drivers/` 或 `os/kernel/`（那是 B、C 的领地，见第 2 节禁碰清单）。
4. **门读数必须 L1**。没有附命令与输出的「✅ 通过」一律作废；推断（L3）算 FAIL，除非写明回退理由。

**组合行的领取办法**：任务表里一行出现多个条目 ID 时（A5、A6、A22 一类），一次只领一个 ID、开一个分支；不要把整行当成一个 claim 键。A5 的两个 ID 分两次领；A6 按行内写的顺序逐个领；A22 的每个族成员单独领。没有权威 ID 的台账卫生行用括号里的追踪键（A10 的 `CMD-LEDGER-RECOUNT`）。

## 1. 命令面任务表（按 P0→P1→P2 排，同一轮只做一行）

| # | 条目 ID | 登记处 | 状态 | 这一轮做到哪算完 | 判据与门 |
|---|---|---|---|---|---|
| A1 | `BSD-HEADER-COMPAT-REF` | `18-stage-commands/todo.md:129`、`:173`；本轮条目 `:122` 之前新立的批次三百七十三 | ✅ **`2026-10-18` 落地**（提交 `bc4598315`） | BSD 头补位面按符号落成四处：`tools/atf-c-compat/host-ref/sys/cdefs.h`（`__COPYRIGHT`/`__RCSID`/`__IDSTRING`/`__dead`/`__unused`/`__arraycount`，用 `#include_next` 接住 glibc 的 cdefs）、`sys/endian.h`（BSD 头名译成宿主 `<endian.h>`，补 glibc 没有的 `bswap16/32/64` 与 `LE32TOH` 组）、`bsd-progname.c` + `bsd-compat-proto.h`（`getprogname()` 带 `__GLIBC_PREREQ(2,40)` 判据，宿主 2.39 无、容器 2.41 有）、`tools/build-bsd-ref.sh`（固定项收一处，额外参数原样透传，配方即一条可贴进台账的命令）。**`sys/featuretest.h` 不需要造**：那半套 vendored 头与宿主头互相指错，本行原本写法的解法是「不 `-I minix3/include`」。**`d_namlen` 没改 C 源**：`fortune.c:186` 自带 `#ifndef NAMLEN` 守卫，补面从 `-D` 走源码自己的门进去 | `fortune.c` 与 `strfile.c` 两支参照件 `-Wall` 零警告链出（退码 0）；九形读数全部取到绝对值（`tmp/bsd-ref-373/readings.log`，命令 `bash tmp/bsd-ref-373/shapes.sh` 可重跑）。**本轮最大收获是形 2**：空库（`strfile` 报 `There were 0 strings`）在 C 里既不报 `empty database` 也不报 `Cannot open`，而是 `random() % str_numstr` 除零 → `Floating point exception(core dumped)`、退 **136**、两路输出全空（`fortune.c:956`、`:1007`）；本壳 `os/commands/games/text-games/src/bin/fortune.rs:60` 打的正是那句 `empty database`。⇒ `FORTUNE-EMPTY-ERRLEG`「齐平成 C 的 `Cannot open`」这一问法的前提被读数否证，改判仍属人裁决（本行不动它的状态词）。形 3/4/8 三句是真句形：`not a fortune file or directory`、`Cannot open \`/usr/share/games/fortune/<名>'`、默认目录那句；`.dat` 不可读那一形（形 5）走的仍是 `not a fortune file or directory` 而不是 `Cannot open` |
| A2 | `GETOPT-REPORT-BYTE-LETTER`（乙路切片） | `:210`、`:1592` | `☐` **非单轮量级** | 每轮只换**一个消费者**：`GetoptReport::line` 从 `String` 改成交回字节串，先做 textfilter 里最小的一个壳。**本轮切片边界写进账，别一次扫全表** | 现量规模：114 构造点 / 23 文件 / 39 薄壳 / 97 行门断言（`:210`）。每切片都要重跑全包门与负控 |
| A3 | `SPLIT-STREAMING-MEMORY` 片B | `:330`（片A `-b` 已落）、`:687`、片计划 `:703`–`:710` | ✅ **`2026-10-18` 落地**（提交 `f0fe84209`；前置裸机编译门的 cfg 漏门修在独立一笔 `9d892b46c`） | 行模式已流式化。C 的 `split2` 读完后的结论：**不需要**跨读留存面，缓冲尾巴原样写进还开着的同一枚分片（`split.c:276-280`）。片C（`-n`）仍待做，可直接复用片A 的 `ByteCutter` | `split_face` 子集 **26/0 → 28/0**；宿主整包 1076/0 → **1085/0**（68 段），容器权威门同读数；三方 A/B（旧实现×新实现×`tmp/split-311/split_ref`）45 组不一致 **0**；四枚负控各自咬住（3/14/8/4 枚变红，还原复跑 0）。逐条读数与两处「门存在但无分辨力」见 `18-stage-commands/todo.md` 批次三百七十二 |
| A4 | `CKSUM-HASH-DIGEST` | `:1245` | `☐` | 八个算法的摘要本体 + `MD5 (file) = <digest>` 报文形。体量大，可按算法族拆 2–3 轮 | 每加一族补一枚端到端门；参照件读数走 A1 的补位面或 C 板的真机窗，**不许拿宿主 glibc 输出当真值** |
| A5 | `CKSUM-HASH-STATE` 剩余片 | `:1219` | `☐` | `-a`/`-o` 状态诊断的下一片；一次只做一个算法族的状态腿 | 同 A4；每片重跑 `cargo test -p minix-fileops --lib` 一类本包门 |
| A5b | `CKSUM-FILE-LEGS` | `:1268` | `☐` | `Cannot read %s` 一族的文件腿报文形；与 A5 分开领，不合并成一笔 | 同上；报文形逐字节对参照件 |
| A6a | `SPLIT-N-STDIN-FSTAT-SIZE`（原与 A6b/A6c 挤在一行，本轮按 §0 的领取办法拆成三行） | 登记处 18 篇批次三百三十段内；本轮现查见批次三百七十八 | ⏸ **等 C 板真机窗**（不是等裁决，等的是一条读数） | C 的 `split3` 在读任何字节前就 `fstat(ifd)` 并拿 `sb.st_size` 判「能不能切成这么多片」（`split.c:290-300`）；标准输入是管道时本壳读尽后按 `input.len()` 分片，同一形分叉。追到底的结论：**这一格在仓库内没有真值来源**——Minix 的 `do_fstat` 自己不算任何字段，只 `req_stat(v_fs_e, v_inode_nr, ...)` 转发给该 vnode 的文件系统（`stadir.c:185-187`），管道归 PFS，而 PFS 不在本仓（`ls minix3/fs` 无输出、VFS 全仓 `grep st_size` 零命中）。反向线索：VFS 关最后一个读写者时 `truncate_vnode(vp, vp->v_size)` 注释「Tell PFS about latest pipe size」（`filedes.c:494-500`），即 Minix 给管道记尺寸 ⇒ 完全可能是「已缓冲字节数」，那就是竞态语义，本壳启动时无法对齐。**不硬造门**：现在钉只能是钉住猜的那一种，钉错就把错语义固化成基线。本轮交出的是仪器与判读规则 | `tools/atf-c-compat/probes/split_n_stdin_size.sh` 四形（管道已灌/管道慢写者/规则文件/字符设备），宿主侧已验通：glibc 形 1 与形 3 都报 `can't split into more than 0 files` 退 1 不产物、形 2 退 0（`tmp/split-378/host_readings.log`）。真机拿到读数后按脚本结尾那条判读规则定策再翻 `🔄`。顺路把 A1 补位面补到 `split` 可编：新增 `host-ref/sys/param.h`（`MAXBSIZE (64*1024)`，出处 `compat_defs.h:1150-1151`）与 `host-ref/sys/stat.h`（`DEFFILEMODE 0666`），并在 `bsd-compat-proto.h` 补一行 `#include <sys/stat.h>` 替不自含的源补那层——`split`/`fortune` 两支现都 `-Wall` 零告警 |
| A6b | `SPLIT-CLOSE-HOST-VISIBILITY` | 18 篇批次三百三十段内；本轮现查见批次三百七十九 | ✅ **`2026-10-18` 现查闭合**（提交 `734132d94`） | 登记语「宿主腿看不见 `close` 失败未修」把两件事混成一件，分开后各自有结论：**响应逻辑宿主测得到**——`SinkFailure::Close(code)` → `ChunkWrite::Close(code)`（`bin/split.rs:185`）、取号 `:202`、**报的是刚写完那一枚分片的档名**（`:213`），对得上 C 的 `else if (close(ofd) != 0) err(1, "%s", fname);`（`minix3/usr.bin/split/split.c:321-322`）；两枚宿主门 `the_sink_arms_land_on_their_own_chunk_arms`（`:406`）与 `the_two_write_arms_do_not_swap_their_labels`（`:420`）已钉住，现跑 `cargo test -p minix-textfilter --bin split` → **10/0** 且两枚点名 `ok`。**载体的错宿主造不出**：宿主腿走 `std::fs::File`（`bin_support.rs:409-413`），它由 drop 关闭且**不交回任何号**，std 没有「关闭并取错」的公开接口（`sync_all` 交回的是冲刷错，与 `close` 错不等价）；要造得出只能引一个能调 `close(2)` 的外部依赖（现查本 workspace 无 `libc`、这棵树里也没有 `os/Cargo.lock`，动依赖面与锁按 §2 属共读共享件）或在 `os/libs/` 造宿主 `close` 缝（B 板领地）——两条都不划算，因为**出货那侧本来就把号交回并报了**（真机腿 `bin_support.rs:467-471`/`:477` 走 `minix_sys::close(...).map_err(SinkFailure::Close)`，`SPLIT-CLOSE-ERRLEG` 已闭）。按本行原写口径「宿主看不见的那几形按判据写明、不硬造门」结——补门只会钉住为造错而搭的假注入件 | 见批次三百七十九三件套六条；基线快照 `28250fe6f`。**顺带查出一件值得单独一笔的事 → A6d** |
| A6d | `SPLIT-HOST-FLUSH-AT-SEAL`（批次三百七十九新立，本笔**由提出者本人现查撤销**） | 18 篇批次三百七十九「另记」段；否证见批次三百八十二 | 🚫 **维持登记但不排期——因为本行提的修法经实测不成立** | 那一行写的修法是「给宿主腿加一次显式冲刷并把冲刷错按 `Close` 报」。**探针否证**：对 `/dev/full`，`write_all` 当场回 `Err(code 28 ENOSPC)`，紧随的 `flush()` 回 **`Ok(()))`**——写错在写那一刻就交回来了，`flush` 什么都不知道。原因是宿主落点是 `bin_support.rs:394` 的裸 `std::fs::File`（Rust 里**不缓冲**，`flush` 是 std 的默认空实现；全文件 `BufWriter`/`LineWriter` **零命中**），「冲刷时才浮现」那是 **C 的 stdio** 那一侧的模型，我把两边混了 | 宿主真正不可见的仍然只有 `close(2)` 自身，**与 A6b 是同一条载体限制的两个名字**，出路那两条（引外部 `close(2)` 依赖：本 workspace 无 `libc`、树内无 `os/Cargo.lock`；或动 `os/libs/`：B 板领地）都已在 A6b 写明并结掉。留这一行只会误导出「有个加 flush 的便宜修法」，故 `🚫` 不作 `☐`、也不删条目（防错觉）。读数与三条核实命令见批次三百八十二三件套；基线快照 `dd5e5d764` |
| A6c | `UNIQ-WRITE-ERRLEG` | 18 篇批次三百三十段内；本轮条目见批次三百八十 | ✅ **`2026-10-19` 落地**（提交 `a10f32a10`） | **登记语「报文形状未动」说轻了：C 的 `uniq` 从不查写失手，而本壳两形都报**，其中输出操作数那形还印了一句 C 没有的两段形。C 只在 `file()` 开档失败时 `err(1, "%s", name)`（`minix3/usr.bin/uniq/uniq.c:216-221`），`fprintf` 返回值一律不看，全文件 `ferror`/`fflush` **零命中**（现查）⇒ 写失手不多话、不改退码；参照件实测 `uniq_ref < in >/dev/full` → 退 **0** 且 stderr 空。本壳改前：stdout 形静默但**退 1**（走 `support::emit`，它替调用方退场），输出档形印 `uniq: /dev/full` 退 1。修法 `bin/uniq.rs:107-126` 两条出路都改「失手咽下、末尾 `terminate(0)`」，即 `let _ = sink.finish(&body)` 与 `let _ = support::emit_error(&body)`——**关键是用 `emit_error` 不用 `emit`**，与批次三百六十一给 `fortune` 定的同型判据，属 `Rewrite` 档 | 门：`uniq_face.rs` 新增 `a_stdout_that_stops_taking_bytes_stays_silent_and_exits_zero` 与 `an_output_operand_that_cannot_be_written_stays_silent_and_exits_zero`（13→**15 全绿**）；负控 M1/M2 各咬一枚（失败数各 1、指定门命中、`Compiling` 在日志、还原 md5 一致、复跑 0）。整包 **1087/0**（起点 1085）、三架构 `--bins` 全退 0、clippy **154** 未增、rustfmt 触及两件零漂移（`bin_support.rs:228` 那处既存漂移不属本支，未顺手改）。**参照件为本支而添**：新函数级补面 `host-ref/bsd-fgetln.c`（BSD `fgetln(3)`，glibc 无此名，`#if defined(__GLIBC__)` 判据）+ 原型 + `build-bsd-ref.sh` 改为链全部 `host-ref/bsd-*.c`；`uniq`/`fortune`/`split` 三支回归零告警 |
| A6e | `BSD-GETOPT-DASH-OPTSTRING`（批次三百八十现查新立） | 18 篇批次三百八十「载体限制」段；落地见批次三百八十一 | ✅ **`2026-10-19` 落地**（提交 `d6bc54970`） | **先把上一轮的归因纠正**：那句「BSD `getopt` 用 `-` 交回非选项、glibc 按字符码 1 交回」方向对但不完整——真值本仓就有，`minix3/lib/libc/stdlib/getopt.c:63-143`（Minix 自己 libc）的语义是**非选项就停**（`return -1`，**既不重排 argv 也不把非选项当选项交回**）；optstring 里那个 `-` 只管「孤零零一枚 `-` 当选项」那一形（`:85-96`），`uniq.c:79-81` 的 `case '-': --optind; goto done;` 正是为它写的。glibc 的偏离是**两处**：交回字符码 1（撞 `case '?'`）**外加它会重排 argv**。补面按 Minix 那份照抄、不按手册抄 | 新件 `tools/atf-c-compat/host-ref/bsd-getopt.c`（`#if defined(__GLIBC__)` 判据，复用 glibc 的 `optind`/`optarg`/`optopt`/`opterr` 只换扫描算法，`#define getopt bsd_getopt` 走改名不硬碰系统符号）；`build-bsd-ref.sh` 上一轮已改成链全部 `host-ref/bsd-*.c`，零改动即生效。**六形现跑**（`tmp/getopt-381/`）：两操作数退 0 出 `a|b|c|`；`-c` 带文件操作数宽度 4；输出档开不了三段形退 1；**输出档写不进 → 退 0 一字不报**；stdout 写不进退 0；坏选项 `unknown option -- Z`（Minix 那句，不再是 glibc 的 `invalid option -- 'Z'`）。**两处收获**：① 上一轮 A6c 只能从源码缺省行为推断的那一格（写不进不改退码）本轮量出来了，`a10f32a10` 的修法得证；② A2/A11 那族要对的坏选项句子从此才真的可对，此前对的是宿主 libc 的句子、那种比对无效。四支参照件（`uniq`/`split`/`fortune`/`colrm`）重编 `-Wall` 零告警 |
| A7 | `TR-EQUIV-OOBCH-SET2` | `:1112` | `☐` | `\0` 填充 vs 末字节重复的两形对账 | `tr_face` 门 + 逐形 A/B |
| A8 | `COLRM-ERRLEG-ORDER-UNOBSERVABLE` | `:1316`；本轮条目见 18 篇批次三百七十五 | ⏸ **等人裁决**（`PD-COLRM-STDIO-FLUSH-ORDER`，见 §6 新增行；现查笔提交 `a307714b5`） | **本行原写的「判据在宿主不可观测」已被现查否证**：pty master 在 slave 关闭后先交已缓冲字节、下一口回 `EIO`，正是「有字节在飞、随后读失手」的宿主真载体，探针已入册 `tools/atf-c-compat/probes/colrm_leg_order.py`。四形矩阵实测两处与 C 分叉：小输出＋读坏 C 报 `stdin` 退 1 而本壳报 `stdout`；小输出＋读净＋写坏 C **退 0 一字不报**而本壳退 1 报 `stdout`。不当场修的理由不是「造不出」而是「要四形全对就得在命令层复刻 libc 缓冲阈值与行/全缓冲策略」，与本项目「抽象语义不描述机制」正面冲突，且同族 34 支用静默 `support::emit()` 的壳一并受这条裁决管——两个选项与后果都摆进 §6，等人不降级 | 读数在 18 篇批次三百七十五与 `tmp/colrm-374/matrix_readings.log`；`colrm_face` 16/0 未退化（改的是那枚门注释里被否证的断言，断言本身未动）。裁决落定后验收命令仍是那条探针 |
| A9 | `TEXTGAMES-EMIT-ERRLEG` 的 `wtf` 半 | `:185`（首现）、`:221`、`:226`、`:234`、`:296`；本轮条目见 18 篇批次三百七十六 | ✅ **`2026-10-18` 落地**（提交 `3eefc6c51`） | 写腿那一格的结论是**「判据不归这个程序」，不是「不适用」**：脚本自己没有任何写失手处理逻辑，报文与退码都由解释它的 shell 给——宿主实测（`/bin/sh -> dash`）`ACRONYMDB=<库> sh minix3/games/wtf/wtf nasa >/dev/full` 回 `wtf:79: echo: echo: I/O error` 而**退码 0**（句子是 dash 的内部形，不是 `err(3)` 三段形；`rv` 只由查无此词置）。真机上解释它的是 Minix 的 shell，又是另一套句子 ⇒ 无可对标的程序级判据，**不建门、不改本壳口径**。顺 `wtf` 真跑下去抓到**两枚 P0**（本壳以前没有端到端门，所以一直没被发现）：甲＝`wtf.rs:22` 写死 `minix_sys::{open, read, Fd}`，宿主上真实存在的库档也报 `cannot open`（批次三百六十一给 `fortune` 治过的同一枚），接现成 `slurp_operand`+`OperandFailure{Open}` 并去掉自创的 `read error` 句；乙＝`parse_acronym_line` 按冒号断词，而真库 `minix3/share/misc/acronyms` 是制表符分界（557 行含冒号只有 1 行，还是 `$NetBSD:` 标识行；脚本 `sed "^$target[[:space:]]*\|\|p"` 同样按空白）⇒ **对真库一条都解不开**，改 `split_once(char::is_whitespace)`，空白断词后那枚 `contains(whitespace)` 判据成不可达死码一并删 | 宿主 `cargo test --offline -j 1 -p minix-text-games` → **66/0**（lib 30→31、`random_face` 25、`fortune_face` 5、**新增 `wtf_face` 5**）；容器权威门同读数 **66/0**（8 段，在线腿读数 200，非 `--offline` 降级）；三架构 `cargo build --bins` 全退 0（本轮删的正是真机腿的直接 syscall 用法）；clippy 触及包改前改后都 **13** 未增；rustfmt 零漂移（注意本包是 workspace edition，`--edition 2021` 会把 `acronym.rs:46` 的 let-chain 误报成错）。两枚变异控制各自咬住：分隔符退回冒号红 **5** 枚、宿主开档腿打死红 **6** 枚（并把 `fortune_face` 三枚带红，正证两支共用这条缝）。四处句子分叉另立 **A9b** |
| A9b | `WTF-SCRIPT-PARITY`（真源是 shell 脚本时，脚本**自有的句子**仍是判据） | 批次三百七十六新立；落地见批次三百八十三 | ✅ **`2026-10-19` 落地**（提交 `占位`） | 五形逐一对齐脚本实测读数（真源能直接跑，比读源码更硬）：① 命中打印跟大写化走（`:75` `tr '[a-z]' '[A-Z]'` 后印 `$target`，`nasa` → `NASA: …`）；② 查无此词印**操作数原样** ``wtf: I don't know what `zzz' means!``（`:105`，注意同一条报文里 `$i` 与 `$target` 各有其形，一把梳子会印成 `ZZZ`）；③ 库开不了 `wtf: cannot open acronym database file \`<路径>'`（`:67`，改前本壳是自创的 `wtf: <路径>: cannot open`）；④ **usage 走标准输出**再退 1（`:11-14`，实测 stderr 空）且文本补回 `[-o]`；⑤ 未知选项与裸 `-f` 走 usage——脚本那两形多出的 `Illegal option -Z` / `No arg for -f option` 是 **shell 的 getopts 印的**，不是 wtf 的，故不复创（改前 `-Z` 被当查询词咽掉，等于少一次拒收） | 门：`wtf_face.rs` 五枚全按脚本形重写；负控三枚各咬住（去掉大写化→恰好 2 枚红、usage 改回 stderr→1 枚、句子改回自创→1 枚），还原 md5 一致、复跑归零。读数：`cargo test --no-fail-fast -p minix-text-games` **66/0**；三架构 `--bins` 全退 0；rustfmt 零漂移；**触及路径 clippy 告警 0 条**（整包 15 条全在 `os/libs/` 既存件——顺带定一条方法：clippy 总数会随本次重编了哪些 crate 变，拿总数当棘轮会自己骗自己，要按告警位置判）。过程见批次三百八十三 |
| A9c | `WTF-SCRIPT-FEATURE-PARITY`（批次三百八十三从 A9b 拆出的四条，超出「改句子」量级） | 18 篇批次三百八十三「新立 A9c」段 | `☐` | ① `ACRONYMDB` 环境变量（脚本 `:54-56` 认它，本壳不认，需要环境可读的路子）；② `-o` 的真语义是**通配展开** `/usr/share/misc/acronyms*` 且 `*-o` 那批只在 `-o` 时纳入（`:42-53`），本壳只认单一档名、`-o` 目前是空操作；③ 三级退化查找 `whatis(1)` → `pkg_info(1)` → pkgsrc `make help`（`:83-104`）本壳全无，而这三枚程序在本仓命令面还不存在——这一条要连带定「脚本调外部程序时，本壳该调、该退化、还是该省」的口径；④ `is` 词只移走**开头**那一枚（`:32-34`），本壳在查询循环里跳过任意位置的 `is`（实测 `[is y2k]` 两侧同形，反序 `[y2k is]` 即分叉） | ①②④ 是本壳自己的事可逐条做；③ 牵动别的命令是否存在与「跨程序调用」的口径，先立判据再动。逐条都要拿脚本当参照件出绝对读数 |
| A10 | 台账卫生：命令面状态摘要重立 | `18-stage-commands/todo.md:3`、登记处同文件批次三百六十七 `:179`；总账索引行 `TODO-LEDGER-OPEN.md:133`、那轮实测行 `:149` | ✅ **`2026-10-18` 落地**（提交 `349066507`） | 逐壳清点做完，本账第一行按现数重立：`find os/commands -path '*/src/bin/*.rs' \| wc -l` → **113**；小于 1 KB 的壳 **0**、`unimplemented!`/`todo!` **0** 处；真实缺口是 **15** 枚 bin 里 **23** 处选项级 `not wired`（6 个 crate：`fileops` 6、`proctools` 3、`textfilter` 2、`doctools` 2、`compress` 1、`mountinfo` 1）。未闭合项**不再用自造整数**表示，逐行状态归本板 §1/§1b、跨阶段项归总账。**总账 §6 那行照抄旧摘要的事实本轮不代改**（§2 禁碰：那本账编排线自持），行号与失配证据留在批次三百七十四 | 三条判据命令与读数全留在批次三百七十四；核实日 `2026-10-18`，基线快照 `f28847e10`。两处附带教训：① 「按行数判占位」会误伤 `true.rs`/`false.rs`（各 30 行，C 本身就是一句话的程序）；② `grep --include` 的模式**对文件名求值不对路径求值**，`--include='*/src/bin/*.rs'` 数出 0 差点被当成「已无未接线报文」的假结论 |
| A11 | `REV-NONASCII-OPTLETTER`（集外字母在 C 打**一个字节**，本壳按 `char` 取） | `:2503` | `☐` | 与 A2 `GETOPT-REPORT-BYTE-LETTER` **同族**，最好与 A2 同一人连续做，一次改完两处的报文形 | 逐形 A/B 对参照件 |
| A12 | `RANDOM-DENOM-LEGS` 的三处已声明不追余项（非有限的 `as` 饱和、末单位残差、下溢 `ERANGE`） | `:643`（首现）、`:751`–`:758`（三处不追的声明段）、`:1087`、`:1096`；本轮对账见 18 篇批次三百七十七 | ✅ **`2026-10-18` 对账闭合**（提交 `9930ec40d`；纯台账笔） | 板上原来的 `🚫 不排期` 与账上的 `partially fixed` **两条都不准**，现查后改定：登记的四条敞口里**④（共用 `bin_support::emit` 写缝退码）已由批次三百五十九闭合**——`random.rs:113-114` 换 `support::emit_error`，判据 C `if (ferror(stdout)) err(2, "stdout")`（`minix3/games/random/random.c:131-133` 本轮实读复核），门在 `random_face.rs:456`；剩下 ①—③ **本来就不是待办**，三条各有出处可复核：① C 那一格是未定义行为，x86-64 的 `cvttsd2si` 给 `0x80000000` 而 aarch64 的 `fcvtzs` 饱和，没有单一真值可追；② 末位单位残差来自不做 gdtoa 大数精化，只挪动「恰好压在阈值那一格」的选中判定；③ 子正规立号与否属 gdtoa 的 `Avoid_Underflow` 配置差异。⇒ 取值域三片 + `nan` 载荷早已落地，④ 闭合，①—③ 以**三行「登记不追」继续留在账上**（留名是为了不让后来者当缺口重做），条目按六值词表记 `closed`。这不是「改成 DEFERRED 充数」：闭合依据是每条都有可复核出处 | 本条**不在** `TODO-LEDGER-OPEN.md`（现查 `grep -n "RANDOM-DENOM-LEGS" rewrite-notes/coordination/TODO-LEDGER-OPEN.md` 读数空），状态权威在本阶段账，不存在越权改总账。核实命令五条全列在批次三百七十七的三件套里；基线快照 `e2aefd803` |

**已闭、勿重开的负控项**：`批次三百二十六的负控补账` 已由批次三百七十连同批次三百二十五至三百二十九的六枚旧负控一次清空（提交 `d4b9fd1a3`，账见 `18-stage-commands/todo.md:131`–`:133`）。不要再按 `:1602` 那一行把这条开成新任务；若在别的批次看到「六枚负控欠账」，先确认它是不是同一笔。

**不在本板的已闭项**（别去翻案，闭单勿领）：`SPLIT-NONUTF8-DROP`、`SPLIT-TAIL-NOEOL-NEGCTL`、`SPLIT-READ-STAT-ERRLEG`（读腿，批次三百三十八已闭；`:1153` 那句「下一轮首动作」是陈旧指针，它的 `stat` 腿已并入 `SPLIT-STREAMING-MEMORY`，见 `:1143`）、`FORTUNE-READ-ERRLEG`、`FORTUNE-OPEN-ERRLEG`、`FORTUNE-HOSTED-FILE-SEAM`、`RANDOM-STDOUT-EMIT-RC`、`REV-STDIN-ERRLEG`、`COLRM-IO-ERRLEG`、`SPLIT-CLOSE-ERRLEG`、`SPLIT-WRITE-ERRNO`、`JOT-SEED-CAST-UNIT-TEST`、`BIN-SUPPORT-HOSTED-ARGV-NONUTF8`、`COLUMN-TAIL-WARN-COUNT`、`JOT-PREC-MINUS-ONE-FORMAT`、`TRUNCATE-RELATIVE-WRAP`、六枚负控欠账（批次三百七十已清空）。

## 1b. 三线服务器与跨阶段传输族（承接清单，原三块板遗漏的整条线）

这一节的任务与第 1 节平级，同样按 P0→P1→P2 排；服务器线的每一行都先读阶段账的正文，**账面计数可能早于现树**（例如 17 篇的 `N1-P1-2`/`N1-P1-4` 条目行已标落地，摘要行仍写 `open`）。一次只领一个条目 ID。

| # | 条目/族 | 登记处 | 状态 | 这一轮做到哪算完 | 判据与门 |
|---|---|---|---|---|---|
| A14 | 02 虚存服务器：热更/重启状态机、启动进程的可执行文件装载链；账载 `open` 2 + `stopgap landed` 3 | `02-stage-vm/todo.md:3` | `☐` | 先按阶段账逐条复核；优先做启动进程装载链，它挡 E1/E5。**非单轮量级**，一轮只做一条腿 | `cargo test -p minix-vm --lib`；真机腿走 C 板第 4 节 |
| A15 | 03 根服务：二十四个符号的生产接线面、根服务自升级（E-1/E9）；账载 `open` 2 | `03-stage-rs/todo.md:3`、`:164` | `☐` | 一条符号族一轮；自升级先读 19 阶段接线面，不代做 19 的前置 | `cargo test -p minix-rs --lib` |
| A16 | 04 进程管理：核心转储路径、调度服务缺席导致的假端点、E7 调用号双址；账载 `open` 3 | `04-stage-pm/todo.md:3`、`:115` | `☐` | 核心转储与假端点各一轮；调用号双址归 E7 共享面，先登记跨界再动 | `cargo test -p minix-pm --lib` |
| A17 | `E-DMWIRE` 设备管理生产四缺（server transport、请求分类器、装配半、client/RS 侧） | `11-stage-devman/todo.md:201`、`TODO-LEDGER-OPEN.md:83`、`:89` | `☐` | 四缺逐条读原文再定切片。**client/RS 半在 `minix-sys`（B 板）**，动服务器前先在 C 板 §6 挂一行；本行持主线 | 每缺一枚装配门；client 半与 B 板对账 |
| A18 | 07 数据存储：标签发布/事件编码的生产调用方、`E-DSWIRE` | `07-stage-ds/todo.md:3`、`TODO-LEDGER-OPEN.md:75`、`:83` | `☐` | `E5(f)` 已有首份真机终态；发布/删除/订阅/notify 四族缺的是可具名的测试载体，先做载体再谈全链 | `cargo test -p minix-ds`；真机腿走 C 板窗 |
| A19 | 09 初始化：崩溃处理器的提供形式（C-21）、加密后端余量 | `09-stage-init/todo.md:3` | `☐` | C-21 归编排线扩面，先确认它是否仍挡本服务；「加密后端余量」先对账它是否仍存在、归哪条线（若指 random 驱动的密码学后端，则归 B 板的驱动族，不要在 init 里找） | `cargo test -p minix-init --lib`（若对账后本行无活，标 `✅` 并写明去向，不硬造任务） |
| A20 | 15 文件系统：ext2/isofs/hgfs/vbfs 四家入口未接；`E-FSBDEV`/`E-FSRUNTIME`/`E-FSVMCACHE`/`E-FSCMDS` | `15-stage-fs/todo.md:3`、`:4`、`:31`（edge×4 登记行）、`:80`–`:84`、`:89`、`:225` | `☐` **非单轮量级** | 四家按「有真盘才接」的次序：vbfs/hgfs 先做无盘可用形，ext2/isofs 等 `E-FSBDEV` 真镜像；一轮一家。`E-FSVMCACHE`（vm cache 传输缝）与本行四家同账本，逐项领 | 11 个 crate 的全量命令见 `:241`（基线 289 passed）；单包门另跑 |
| A21 | 17 网络：协议栈本体、传输、硬件路径；账载 `open` 2 | `17-stage-net/todo.md:3`、`:19`、`:24`–`:27` | `☐` **非单轮量级** | 先复核 `N1-P1-2`/`N1-P1-4` 的落地行是否覆盖生产主循环；再挑一条协议栈切片。**policy 半在 `os/libs/minix-netdriver`（B 板）**，本板只动 `os/net/lwip`、`os/net/uds`，跨线先登记 | `cargo test -p minix-net-lwip -p minix-net-uds`（包名先 `cargo metadata --no-deps` 复核） |
| A22 | 跨阶段传输族（服务侧）：`E1`、`E6`/`E7`、`E8`、`E9`、`E-ISWIRE`/`E-ISPROD`/`E-ISBOOT`、`E-RMIBWIRE`/`E-MIBPROD`、`E-INWIRE`、`E-NETSTART`、`E-INITSYS`、`E-IPCWIRE`、`E-VMTLB`/`E-PREEMPTFLAG` | `TODO-LEDGER-OPEN.md:76`–`:82`、`:83`、`:123`（`E-IPCWIRE` 登记行）；`13-stage-ipc/todo.md:3` | `☐` | **逐项领，一次一个**；每项先读 §4 原行与它自己的缺件清单，不要把整族当一轮。内核侧的那一半归 C 板 | 每项自己的端到端门；真机腿走 C 板窗 |
| A23 | `C-46` 挂载/卸载余量、`C-57` 命令冒烟写侧、`RM-ERRNO`（unlink 对不存在路径的 errno） | `TODO-LEDGER-OPEN.md:98`、`:99`、`:145` | `☐` | `C-57` 写侧受可写根盘门控；`RM-ERRNO` 的根因在根盘 FS（imgrd 回 EIO/EROFS），不在 rm 壳；先做 C-46 | `C-57` 三架构 smoke；`RM-ERRNO` 等 E5 盘上集成 |
| A24 | `SD-36` vmddm 旁路、`SD-40` RegionMap 静默回退（VM 服务侧） | `STRUCTURAL-DEBT-REGISTER-20261008.md:764`、`:795`；`TODO-LEDGER-OPEN.md:21` | `⏸ 等 C1` | 两者都绑在 riscv64 内存污染案卷的收口上；案卷不结不做，别提前删回退面 | 案卷收口批一并命名 |
| A25 | `SD-19` 余下服务 crate 的静默吞错门 | `TODO-LEDGER-OPEN.md:67` | `☐` | `minix-sys`/`minix-rt` 已接；服务侧按 crate 扫 `let _ =`，逐 crate 接同款门并写处置表 | 每 crate clippy 读数 + 处置表 |
| A26 | `K2`/`S-6.3` 每核运行队列的文档记账半 | `TODO-LEDGER-OPEN.md:24`；领地 `06-stage-sched/` | `☐` 低优先 | 大内核锁下就绪队列由锁串行化、按核分队退化为单队列——这条「记账取代实现」的理由按 `:24` 行要求写进 06 篇对应节。先对账 06 篇是否已写：已写明则标 `✅` 附行号，不硬造任务 | 只改文档不改码（码侧记账已在 `os/kernel/src/sched.rs`，那是 C 板领地）；本篇 `doc-style-lint` 零 error |

**服务器线的顺序建议**（不是硬规则）：A14/A18/A17 先做，因为它们直接解锁 E5 的服务参战面；A20/A21 体量大，按「一轮一家/一条协议栈切片」推进；A22 的机器读数统一排 C 板窗，不在本板起 QEMU。

## 2. ⛔ 禁碰清单（越界要先在 C 板 §6 的跨界登记行里挂账，本板默认不越界）

| 不可写 | 属于谁 | 为什么硬 |
|---|---|---|
| `os/libs/` 全部 crate（含 `minix-sys`、`minix-types`、`minix-rt`；`minix-boot` 归 C） | B 板（`minix-boot` 已单独确权给 C 板） | 命令面与服务器面的 `emit`/`warn_line`/`err_line` 都从这里来，改了会让 B/C 的门同时变红 |
| `os/kernel/`、`os/arch/`、`os/plat/`、`os/boot-shim/`、`os/kernel-image/`、`os/qemu-tests/` | C 板 | 真机面 |
| `os/drivers/` | B 板 | 驱动框架与逐驱动实装 |
| `rewrite-notes/01-stage-kernel/*`、`00-master-plan/*` | C 板 + **当前活跃的文档线程** | 该目录今天有未提交在制品 |
| `rewrite-notes/14-stage-runtime/*`、`16-stage-drivers/*` | B 板 | 二线的阶段账 |
| `rewrite-notes/coordination/TODO-LEDGER-OPEN.md`、`TODO-LEDGER-DONE.md`、`new_edge*.md`、`PARALLEL-*`（别人的板）、`STRUCTURAL-DEBT-REGISTER-20261008.md`、`DOC-ITER-LEDGER.md`、`CONCURRENT-WORK-PROTOCOL.md`、`README.md` | 编排线 / 各板自持 | 协议 §3：进度只记线文件与本板，不回写权威台账（多写者必覆盖） |
| `os/Cargo.toml`、`os/Cargo.lock`、`.github/workflows/`、`tools/`、`AGENTS.md` | 共读共享件 | 加 workspace 成员、改特性、动 CI 都算跨界：先在 C 板 §6 登记一行；`Cargo.lock` 只由实际构建产生，不手改 |

## 3. 门配方与基线（先复跑，再声称）

- 宿主（工作目录 `os/`，fresh shell 要 `export PATH="$HOME/.cargo/bin:$PATH"`）：
  `ulimit -v 3145728; cargo test --offline -j 1 -p minix-textfilter` → 基线 **1076 passed / 0 failed**（68 target 段，退码 0）；`-p minix-text-games` → **60 passed / 0 failed**（lib 30 + `random_face` 25 + `fortune_face` 5）。
- 权威容器编译门（**在仓库根执行**，挂载仓库根并把工作目录指到 `os/`；claim 树里把 `-w` 换成 `/work/.wt/<id>-qoder/os`）：
  `docker run --rm -u "$(id -u):$(id -g)" -v "$PWD:/work" -v "$PWD/tmp/ci-cargo:/cargo" -v "$PWD/tmp/ci-target:/tgt" -e CARGO_HOME=/cargo -e CARGO_TARGET_DIR=/tgt -w /work/os -m 2g minix-ci:1.94 cargo test -j 1 -p <pkg>`
  `-u` 不是可省的：不带它，容器内 root 会把挂载的 `os/target` 写成 root 属主，`release` 就删不掉那棵树（判例 C-103，本会话刚回收了 3.5G 的这类残骸）。
- **在线/离线先秒判**（别再拿 `--offline` 冒充在线门）：
  `docker run --rm minix-ci:1.94 sh -lc 'curl -sS -o /dev/null -w "%{http_code}\n" --max-time 10 https://index.crates.io/config.json'`
  → `200` 走在线门；`000` 或 curl 退码 `28` 是**宿主透明代理的假 IP 面（`198.18.0.0/15`）没起来**，等它或修它，不改配方（根因见批次三百六十八）。
- clippy / rustfmt：只判**本次触及件**，`cargo clippy --all-targets -p <pkg>` 与 `rustfmt --edition 2021 --check`。
- 服务器线的本包门按 A14–A25 行内写出的 `-p` 包名跑；拿不到包名时先 `cargo metadata --no-deps --format-version 1 | python3 -c '…'` 取，不要按目录名猜。

## 4. 负控（变异控制）的三条硬判据

一枚注入**编不过就不是负控**。判定必须同时满足：① 注入编译通过（日志里无 `error[`、无 `could not compile`）；② 失败数 > 0；③ **期望的那一枚门出现在变红名单里**。此外注入前取整文件 md5、跑完整文件还原复验、再复跑同一目标确认基线归零；还原一律用内存/临时副本，**不用 `git checkout --`**（在途工作会一起没了）。

**两个坑本板刚踩过，直接抄现法**：① cargo 报的是带模块前缀的全名（`column::tests::test_...`），拿裸名求交集必然落空；② `---- 名字 stdout ----` 那类详情行也是缩进行，算进名单会把读数从 2 枚虚报成 30 枚。可抄的实现：`tmp/negctl-369/ctl369.py`（批次三百七十的四枚全咬住就是它跑的）。

## 5. 需要真机（QEMU）的条目：不在本板做，走 C 板的窗

第 1b 节标 `⏸ 等 C1` 的行、A22 里带真机腿的项，以及 `COMM-DIR-OPERAND`（`:2143`，且 `:1473`–`:1476` 已改标「需平台裁决 + 真机」）、`CMP-DIR-OPERAND`（`:2088`，同一行也提到 `COMM`）、`JOT-PREC-RANDOM-AB-ON-MACHINE`（`:1214`）——**这些条目一律不在本板起 QEMU**。需要时按 C 板第 4 节的令牌配方排队；拿到读数后回本板把状态翻 `✅` 并附日期与提交号。

## 6. 需人裁决，不要自己拍

- `FORTUNE-DAT-FORMAT`（`:173`）：C 的读档前置是配对的 `.dat` 与定界串（`fortune.c:1064-1068`、`:268-269`），而本仓 `fortune` 是按行切格言、取模抽取的**重新设计过的过滤器**。要不要实现 `.dat` 是 `Refactor`/`[ARCH]` 级裁决。
- `FORTUNE-EMPTY-ERRLEG`（`:185`）：卡在上一条后面，那句「空库」措辞不是措辞问题。
- **`PD-COLRM-STDIO-FLUSH-ORDER`（A8 现查后新增，`2026-10-18`）**：命令层该不该复刻 libc 的 stdout 缓冲策略。事实面已由探针钉住（`tools/atf-c-compat/probes/colrm_leg_order.py`，读数在 18 篇批次三百七十五）：小输出时 C 参照件要么报 `stdin`（读也坏）要么**退 0 一字不报**（只有写坏），而本壳两种都报 `stdout` 退 1。两个选项与后果：**甲**＝在命令层复刻缓冲（按 `BUFSIZ`＝1024 攒满才真写、退出时那次失败静默、且字符设备落点走行缓冲——`minix3/include/stdio.h:182`、`minix3/lib/libc/stdio/makebuf.c:168-178`），四形外部行为全对齐，代价是把一个 libc 机制写进 OS 语义层，并与本项目第一原则（`AGENTS.md`「抽象语义，不描述机制」）相冲；**乙**＝维持现状（本壳无缓冲、写不进去就报），保住语义层干净，代价是这两形的外部行为（含退码）与真源不符，且这不只是报文问题——`$?` 都不同。**影响面不止 colrm**：同 crate 还有 34 支壳在用静默的 `support::emit()`（批次三百三十二 `:1366` 的现查读数），这条裁决一并管它们，所以请按族定规、别按单支定。裁决前 A8 行保持 `⏸`，不动生产码也不建门。
- A24 的 `SD-36`/`SD-40` 不是人裁决项，是等 C1 的案卷收口；不要把它们改成 `☐` 提前开工。
两条 fortune 项都**保持 `☐/🔄 不降级`**，写清「等什么」；把 TODO 改成 DEFERRED 充数 = P0-process-violation。

## 7. 每轮的收尾动作（顺序固定）

在自己 claim 树内：一笔提交（改动 + 本板状态列 + 批次条目）→ 回主树 `git merge --ff-only claim/<ID>-qoder` → `bash tools/claim.sh release <ID> qoder` → 跑收尾五道校验：
`bash tools/todo-reconcile-scan.sh`；`python3 tools/notes-link-check.py rewrite-notes/`；`bash tools/todo-staleness-check.sh rewrite-notes/coordination/TODO-LEDGER-OPEN.md`；`bash tools/doc-style-lint.sh --diff`；`bash tools/check-staged-inflight.sh`。
**若 ff-merge 因别的线先落地而失败**：把 `rewrite` 合进你的 claim 分支（`git -C .wt/<id>-qoder merge rewrite`），只在你自己的分支里解冲突，再 ff 回主线；**绝不在主树 `checkout`/`reset --force`**（C-35 判例）。
**`doc-style-lint --diff` 若报 `rewrite-notes/01-stage-kernel/todo.md` 的命中**，那是文档线程的在制品，不是你的改动；只处理自己路径的命中。

## 8. 本板与另两板的依赖边（谁等谁）

| 本板条目 | 等谁 | 等什么 |
|---|---|---|
| A1 `BSD-HEADER-COMPAT-REF` | 不等 | 它反而是 B/C 若干「参照件对账」的前置 |
| A4/A5/A5b `CKSUM-*` | A1 | 有参照件才敢定报文形 |
| A17 `E-DMWIRE` | B 板（`minix-sys` 的 client 半） | 服务器侧本板持主线；client 面改动先登记，不代 B 改 |
| A18 `E-DSWIRE` | B 板（`minix-sys ds.rs` 客户端） | 服务端本板做；客户端半归 B |
| A19 `C-21` | 编排线 | 崩溃处理器扩面不是本板能代决的 |
| A20 `E-FSBDEV`/`E-FSRUNTIME` | B 板（块层/驱动） | FS 侧接线与驱动侧真盘交付必须成对 |
| A21 网络 | B 板（`minix-netdriver` policy 半） | `os/net` 与 `os/libs/minix-netdriver` 分线，跨线先登记 |
| A22 带真机腿的项 | C 板 | 真机窗与 E5 读数 |
| A25 `SD-19` 服务 crate | 无 | 只动服务侧；libs 的同类门由 B 板做 |
