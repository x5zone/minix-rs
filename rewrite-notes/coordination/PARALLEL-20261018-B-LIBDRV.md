# 并发任务板 B — 二线·用户态运行时、库与驱动框架（`os/libs/` 逐 crate、`os/drivers/`）

> **这份文件是什么**：`2026-10-18` 把全仓库开放项按**领地互斥**切成三份（A/B/C）中的一份，交给一个独立 agent 并发推进。切分依据是 `coordination/CONCURRENT-WORK-PROTOCOL.md` §2 的四线所有权清单——三份文件的可写路径两两不相交，这是并发不打架的唯一硬保证。
> **本板的领地**：`os/libs/` 的逐个 crate（**`minix-boot` 除外**，它归 C 板）、`os/drivers/`、`rewrite-notes/14-stage-runtime/`、`rewrite-notes/16-stage-drivers/`、本文件。
> **配套件**：`PARALLEL-20261018-A-CMD.md`（三线 服务器/文件系统/网络/命令面）、`PARALLEL-20261018-C-KERN-QEMU.md`（一线 内核/引导，**独占真机窗**）。
> **本板领地的关键性**：协议 §2 写明二线是另外两线的**依赖下游汇聚点**。也就是说本板一次改动会同时改变 A 与 C 的门读数——所以本板的每一条都要「先量下游、再动上游」，且**改公开签名前必须先在 C 板的跨界登记行挂账**。
> **条目状态词**（五值）：`☐` / `🔄`（已领取，别人别碰）/ `⏸`（注明等谁）/ `✅`（带日期与提交号）/ `🚫`（写明为什么现在不做）。

---

## 0. 你（接手的 agent）开工前的四件事

1. **owner 名固定 `zcode-ds`**，领取一律 `bash tools/claim.sh claim <条目ID> zcode-ds`；别手搓 worktree，别在主树 `checkout`/`reset`。**worktree 路径是分支名去掉 `claim/` 后整段小写**（`tools/claim.sh` 的 `wt_path`），以 claim.sh 打印的路径为准。
2. **先复核锚点再领**：本板行号是切板时刻的快照。领之前跑 `grep -n "<条目ID>" rewrite-notes/16-stage-drivers/todo.md`（14 篇同理）。「账面滞后」在这个仓库是常态——`14-stage-runtime/todo.md:23` 已把 `V1-P1-1` 标 ✅，而 `coordination/TODO-LEDGER-OPEN.md:124` 仍记「真机 2」。**遇到这种不一致，先做对账再动码，并把对账读数留在账里。**
3. **一次一条**；改 `os/libs/` 任一 crate 的公开面之前，先跑下游全包门（第 3 节）拿基线。
4. **门读数必须 L1**：命令 + 输出成对，推断不算过。

**组合行的领取办法**：一行出现多个条目 ID 时一次只领一个、开一个分支；没有权威 ID 的行用括号里的追踪键（B6 的 `AT-WINI`）。不要把整行当成一个 claim 键。

## 1. 二线主任务表（按 P0→P1→P2 排，同一轮只做一行）

| # | 条目 ID | 登记处 | 状态 | 这一轮做到哪算完 | 判据与门 |
|---|---|---|---|---|---|
| B1 | `G6` 驱动服务二进制层整体为零 | `16-stage-drivers/todo.md:68`、`:12`；`TODO-LEDGER-OPEN.md:43` | `☐` **非单轮量级** | **按驱动族分批**，每轮一族；每族落地标准：`main.rs` 从占位骨架变成真事件循环 + 至少一枚可跑的宿主门。**`log` 族已落**；**`readclock` 族已落（本笔提交）**：新增 `service.rs` 泵（RTCDEV 专用族：身份门 `CallerIdentity`（生产 `PmIdentity` 走 PM `getnuid` + PM 端点比较）、时间按地址/grant 两路拷贝、RTCDEV 状态回复；7 枚宿主门），`main.rs` 换 `Runtime::serve`（标签 `readclock.drv`，对位 `minix3/etc/system.conf:257`），`init()` 交付 `ReadclockService<NullClock, PmIdentity>`（芯片 NULL 时 EIO 诚实失败，架构时钟是登记硬件缝）；`DriverTransport` 补 `copy_to_addr`/`copy_from_addr`（带默认；KernelTransport 用 `sys_datacopy`）。引导表登记与 freestanding 双入口仍延后归 A/C | 现算口径（本片后）：`find os/drivers -name main.rs \| wc -l` → **55**；`grep -rln 'loop {}' os/drivers --include=main.rs \| wc -l` → **50**（log 52→51、readclock 51→50）；`cargo test -p minix-driver-readclock` → **22/0**、`-p minix-driver-rt` → **14/0**；两 crate clippy 自有 0；`cargo check --workspace` 无 error |
| B2 | `A4` 协议常量单一来源 | `16-stage-drivers/todo.md:120` | `✅`（`2026-10-18`，本笔提交） | 已收口：跨消费者族（CDEV、BDEV）单一权威在 `minix-types::types::device`——chardriver 与 vfs 的 cdev/bdev 消费者侧前轮已收敛；本轮删掉 blockdriver 最后四条本地副本（`BDEV_REQUEST_BASE`/`BDEV_READ_ACCESS`/`BDEV_WRITE_ACCESS`/`BDEV_FORCE_WRITE`）改为 `pub use minix_types::{BDEV_FORCEWRITE, BDEV_R_BIT, BDEV_RQ_BASE, BDEV_W_BIT}`，内部三处与 `os/net/lwip/src/bpfdev.rs` 一行引用随动；device.rs 补块族旗标三枚（com.h:986-988）。单消费者族（NDEV/RTCDEV/USB）按 device.rs 模块头既定边界留在属主 | `grep -rn 'BDEV_REQUEST_BASE\|BDEV_READ_ACCESS\|BDEV_WRITE_ACCESS\|BDEV_FORCE_WRITE' os --include='*.rs'` → 0；`cargo test -p minix-types -p minix-blockdriver` + workspace 编译面 |
| B3 | `SD-11` T4：全局分配器的根修（`PD-28`） | `TODO-LEDGER-OPEN.md:57`；`STRUCTURAL-DEBT-REGISTER-20261008.md` 的 `SD-11` | `☐` **非单轮量级** | 按四级排序选首个迁移服务，落「执行与跟踪分离」的根修；**删除 x86/非 x86 的池分档是显式验收项**。消费者是 VM 服务（A14），本板持 `minix-rt` 侧主线，跨线先登记 | `cargo test -p minix-rt`；A14 的 VM 宿主门；`FixedPoolSupplier` 不再焊死生产全局分配器 |
| B4 | 运行时 csu 链 13 项缺口 | `14-stage-runtime/todo.md:75` | `☐` | `ps_strings`/`environ`/`progname` 全局、`atexit`/`fini_array` 等 13 项，按「先有消费者再有面」的顺序逐项做；`minix-rt` 的诞生链已有交付面（`bin_support` 在用 `minix_rt::crt0::args()`），**别重复造** | `minix-rt` 宿主门 + 三架构 `--bins` 各 `Finished` |
| B5 | `V1-P1-1` 的剩余真机面 | `14-stage-runtime/todo.md:23`（诞生链已标 ✅）与 `TODO-LEDGER-OPEN.md:129`（记「真机 2」） | `✅`（`2026-10-18`，本笔提交） | **对账已完成，无需上机**：「✅ 的那一半」= 诞生链两步（第一步删 `start.rs` 死码、第二步 `rt-birth` 真机五断言）与 `V1-P1-5` 文档落稿，均阶段内闭合；「真机 2」具名 = `real-trap` 与 `panic-handler` 两个 feature 面——前者已改按 `target_os = "none"` 判定（feature 只剩宿主强制，`os/libs/minix-rt/src/init.rs` 的 cfg 与 `rt-birth/Cargo.toml` 的 `T-STAB-1` 注），后者已由 `rt-birth`/`pfs`/`mfs` 等 freestanding 二进制消费；真机验证归总账 §4 的 `E1` 行（C 线窗）。`V1-P1-1` 自身余项 `fini_array`/`init_array`（无消费方）归 B4 的 csu 链 | 台账与阶段账已同步；QEMU 用量 0 |
| B6 | `at_wini`（IDE 存储）真机尾件 | `16-stage-drivers/todo.md:165`（§7 起首）到 `:177`（尾件清单） | `⏸ 等 C 板真机窗` | `w_probe` 的 PCI 枚举与 `BlockDriver::transfer` 的「读设备到暂存 → grant 拷贝 → 每扇区 IRQ/超时」两段；claim 键用 `AT-WINI`（追踪键，不是权威台账 ID） | `cargo test -p minix-driver-at_wini --lib` 基线 44 passed；真机腿走 C 板窗 |

## 1b. 二线跨阶段承接清单（原板遗漏的库面/驱动面开放项）

这一节与第 1 节平级；一次只领一个条目 ID。账载计数可能早于现树，领取前先复核正文。

| # | 条目/族 | 登记处 | 状态 | 这一轮做到哪算完 | 判据与门 |
|---|---|---|---|---|---|
| B7 | 16 篇其余开口项：`A2` 框架接线计划、`A7` 共享 NIC 环策略、`A10` 钩子签名 `Result` 化、`OQ-1`/`E-SDEVOWN`、`OQ-3` pci `BusControl`；另含输入驱动族的 `E-CDRCONV`/`E-TTYEVENT`/`E-PCKBDREG` | `16-stage-drivers/todo.md:76`、`:123`、`:126`、`:145`、`:147`；`12-stage-input/todo.md:3` | `☐` | `G7` 已收口（doc 04/13/21 共 8 处，见本笔提交与阶段账）。一项一轮：A2 按 plan §6 波次；A7/A10 是框架层重构；OQ-1 的 sdev/sockevent 归属与 17 篇/`minix-netdriver` 有交叉，先确权再动；输入驱动三条与 A 板的 input 服务器交界，先确权再动 | 每项自己的 crate 门 |
| B8 | `SD-19` 余下库 crate 的静默吞错门 | `TODO-LEDGER-OPEN.md:66`（本片进度注记在 §3 表后） | `☐` | `minix-vtreefs`、`minix-sffs`、`minix-types`、`minix-driver-rt` 四片已落（vtreefs 20/0；sffs+types 19/0 与 331/0；driver-rt 14/0；门武装实验分别坐实 1/3/3 条真触发）。**余面二选一**：① 把 `os/libs/*` 未接门的 crate 一次机械扫尾——已无已知 must_use 真触发（audiodriver/blockdriver/chardriver/netdriver 的 9–10 处 `let _` 均非 must_use），收益是防未来回归、不是修现状；② 转 B7 `G7`/B5 这类可整项收口的条目。`commands/*` 零散站归 A 板。**不要升 `deny`** | 每 crate `cargo clippy -j 1 -p <pkg> --tests` 读数 + 处置表；无理由的新 `let _ =` 应为 0 |
| B9 | 共享 wire/查询桩：`E-RSWIRE`、`E-KERNINFO`、`E-MINTYPES-RS`、`E-MINTYPES-RUNTIME`、`E-DSWIRE` 客户端半、`E-DMWIRE` 客户端半（边界登记见 C 板 §6）、`E-DEVWIRE` 常量 | `TODO-LEDGER-OPEN.md:80`、`:83`；`16-stage-drivers/todo.md:120`；`14-stage-runtime/todo.md:10` | `☐` | 先做 `minix-types`/`minix-sys` 里的解码与查询桩，再做 `ds.rs` 客户端半；服务端半归 A 板（A17/A18）。逐项领，不整族扫；`E-MINTYPES-RUNTIME` 先复核它是否仍有缺口 | 每 crate 宿主门；客户端与服务端的 wire 字段对账表 |
| B10 | `P-ALL-08` 信号代理循环与按号分派的消费方盘点 | `TODO-LEDGER-OPEN.md:45` | `✅`（`2026-10-18`，本笔提交） | B 侧收口：框架核与 `PD-27` 逃生门已闭（`cargo test -p minix-sef` → 19/0 + 集成门 3/0）；`SIGKSIG` 代理循环的对位在 PM 自己的 `process_sigmgr_signals`（`os/servers/pm/src/signal.rs:670`，已接线且带测试），minix-sef 零对位是设计边界（搬入即与 PM 双实现）；消费方盘点：`fs-rt`、`minix-driver-rt`、`net/lwip`、`net/uds`、`servers/devman`。B 侧无剩余代码；发方链通电归 A 板 T2 | 台账已同步；QEMU 用量 0 |
| B11 | `E-DMABUF` 与 `E-SDEVOWN` 的驱动侧归属 | `TODO-LEDGER-OPEN.md:83`；`16-stage-drivers/todo.md:145`、`:123` | `⏸ 先确权` | `E-DMABUF` 是 DMA 契约；`E-SDEVOWN` 是 sdev/sockevent 的归属（`minix-netdriver` 零引用，`os/servers/vfs` 另有 923 行独立副本）。**先与 A 板确权再动**，不擅自删任何一份 | 确权结论进 C 板 §6；每 crate 门；两份副本的去留要有文字依据 |

## 2. ⛔ 禁碰清单

| 不可写 | 属于谁 | 为什么硬 |
|---|---|---|
| `os/libs/minix-boot/` | **C 板**（协议 §2 明文确权：它的唯一消费者全在一线 `boot-shim`/`kernel`/`arch`/`qemu-tests`） | 引导契约面，改一次要三架构同时验 |
| `os/kernel/`、`os/arch/`、`os/plat/`、`os/boot-shim/`、`os/kernel-image/`、`os/qemu-tests/` | C 板 | 真机面 |
| `os/servers/`、`os/fs/`、`os/net/` | **A 板**（三线服务器/文件系统/网络） | 与 B3/B9/B10 有交接，最容易撞；交接前先在 C 板 §6 挂账 |
| `os/commands/`、`os/etc/`、`os/xtask/` | A 板 | 命令面薄壳与 `bin_support` 双生面 |
| `rewrite-notes/01-stage-kernel/*`、`00-master-plan/*` | C 板 + **当前活跃文档线程** | 只读 |
| `rewrite-notes/02`–`13`、`15`、`17`、`18` 阶段账 | A 板 | 三线的阶段文档 |
| `rewrite-notes/coordination/TODO-LEDGER-OPEN.md`、`TODO-LEDGER-DONE.md`、`new_edge*.md`、`PARALLEL-*`（别人的板）、`STRUCTURAL-DEBT-REGISTER-20261008.md`、`DOC-ITER-LEDGER.md`、`CONCURRENT-WORK-PROTOCOL.md`、`README.md` | 编排线 / 各板自持 | 协议 §3：多写者必覆盖 |
| `os/Cargo.toml`、`os/Cargo.lock`、`.github/workflows/`、`tools/`、`AGENTS.md` | 共读共享件 | 加成员/改特性/动 CI 先在 C 板 §6 登记；`Cargo.lock` 只由实际构建产生，不手改 |

## 3. 门配方（本板最要紧的是「下游基线」）

**动上游之前先量下游**，否则改完不知是谁变红：

- 下游基线（A 板的两个包，切板时的读数；领取时复跑一次）：
  `cargo test --offline -j 1 -p minix-textfilter` → **1076 passed / 0 failed**（68 target 段）；`-p minix-text-games` → **60 passed / 0 failed**。
- 本板包的门：先跑 `cargo test --offline -j 1 -p <pkg>` 取一次基线，**把数字写进本表对应行的「判据与门」列**（本板切板时未替你量，因为二线的包多、必须按你实际领的那一族来定）。拿包名先 `cargo metadata --no-deps --format-version 1`，不要按目录名猜。
- 内核宿主测必须带形态：`cargo test -p minix-kernel --features mock`（裸跑会被 `os/kernel/src/lib.rs` 的 `form_contract` 编译期绊线挡住——那是设计行为，别放宽它）。这条对 B3/B10 尤其相关。
- 权威容器门与在线/离线秒判：**照 A 板第 3 节原样执行**（`-u "$(id -u):$(id -g)"` 不可省；`000`/`28` 是宿主代理假 IP 面没起来，不是 Docker 坏）。
- 三架构 `--bins` 编译面：`cargo check`/`build` 按 `os/` 根执行；**真机跑归 C 板窗**。

## 4. 模式与红线（本板专属）

- **`!Send`/`!Sync`/`Rc`/`RefCell` 只在用户态服务器的事件循环里合理**；一旦你的改动落到 `os/drivers/` 里被内核侧消费的面，就要按内核 SMP+BKL 口径重审（禁 `Rc`/`RefCell` 跨 CPU，共享数据 `Arc`+`Mutex`/`Atomic`）。
- 错误类型必须映射到 Minix3 的 errno 值，**禁止自造错误码**；传输失手走既有的 E-SYSCALL-SIGN 口径。
- 硬件细节不许越过语义层：驱动框架里不要把 CR3/PTE 之类名字暴露给 OS 语义面（判据见 `prompt/review-rules/review-core-semantics.md`）。
- 三档术语必须三处一致标注（文档 + 设计 + 代码）：Rewrite / Refactor / Architectural Evolution `[ARCH: ...]`。

## 5. 每轮收尾（顺序固定）

claim 树内一笔提交（改动 + 本板状态 + 阶段账批次条目）→ 主树 `git merge --ff-only claim/<ID>-zcode-ds` → `bash tools/claim.sh release <ID> zcode-ds` → 五道校验（A 板第 7 节那五条命令）→ 若 ff 失败按 A 板第 7 节的合并法在自己分支里解冲突。
**销账前确认树真的从磁盘消失**：`bash tools/claim.sh sweep` 判「可回收/需核对」，别留 root 属主残骸（本会话刚为这类残骸回收了 3.5G）。
**`doc-style-lint --diff` 若报 `rewrite-notes/01-stage-kernel/todo.md` 的命中**，那是文档线程的在制品，不是你的改动；只处理自己路径的命中。

## 6. 与另两板的依赖边

| 本板条目 | 关系 | 说明 |
|---|---|---|
| B1/B2/B7/B8/B9 | **A 板是下游** | 命令面与服务器面通过 `minix-sys` 的 wrapper 调你的面；改签名前在 C 板 §6 挂账 |
| B3 `SD-11` | **A14 是消费者** | 分配器根修落在 `minix-rt`，VM 服务的安装点是 A14；两边同轮验收 |
| B5/B6 真机 | 等 C 板 | 走令牌窗，不自起 QEMU |
| B9 `E-DSWIRE`/`E-DMWIRE` | A 板持服务端主线 | 本板只做 `minix-sys` 客户端半与 `minix-types` 解码 |
| B10 `P-ALL-08` | A 板持服务器侧 | 本板只动 `minix-sef` 框架 |
| B11 `E-SDEVOWN` | 与 A 板确权 | `sdev/sockevent` 与 vfs 副本的去留必须一处说话 |
| B2 `A4` 常量 | A 板持消费者侧 | `os/servers/vfs` 的重述归 A，本板不代改 |
| B1 的 2 个 fs `loop {}` | A 板（A20） | 驱动侧的 52 个归本板，`os/fs` 的 2 个归 A，`os/boot-shim` 的 1 个归 C |
