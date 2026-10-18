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
| B1 | `G6` 驱动服务二进制层整体为零 | `16-stage-drivers/todo.md:68`、`:12`；`TODO-LEDGER-OPEN.md:43` | `☐` **非单轮量级** | **按驱动族分批**，每轮一族；每族落地标准：`main.rs` 从占位骨架变成真事件循环 + 至少一枚可跑的宿主门。**已落十五台（九族）**：`log`、`readclock`、`random`、`fb`、`pty`、`lance`（netdriver 族首个 + 10 枚泵门）、网络族余四台（`e1000`/`rtl8139`/`dp8390`/`virtio_net`，泵上收为 `minix-netdriver::pump`）；存储族三台（`ahci`/`at_wini`/`virtio_blk`，泵上收为 `minix-blockdriver::pump`）；`printer`（并行口缝 + char 泵）；**`pci`（本笔提交，总线族）**：新增 `scan.rs`（按 `ConfigSpace` 枚举 bus/device/function，function 0 缺席即停该设备，多函数逐格扫；桥的次级总线与 BAR 定容不收，已写明）与 `service.rs`（typed BUSC 查询泵：INIT/FIRST/NEXT/FIND/IDS/RESERVE/RESCAN + char 开闭；未建模的属性读写/ACL/BAR/名字腿答 `ENOSYS`，`/dev/pci` ioctl 面答 `ENOTTY`；探针在配置空间不可读时按 `ENODEV` 不起），`main.rs` 换 `Runtime::serve`（标签 `drv.chr.pci`），15 枚新门（卡 33/0），顺手清两枚 crate 既有 clippy 告警。寄存器/中断源/队列搬动仍留卡缝，引导表登记与 freestanding 双入口延后归 A/C | 现算口径（本片后）：`find os/drivers -name main.rs \| wc -l` → **55**；`grep -rln 'loop {}' os/drivers --include=main.rs \| wc -l` → **37**（逐族 -1）；`cargo test --lib`：`minix-driver-pci` → **33/0**；workspace 无 error；clippy 自有 0 |
| B2 | `A4` 协议常量单一来源 | `16-stage-drivers/todo.md:120` | `✅`（`2026-10-18`，本笔提交） | 已收口：跨消费者族（CDEV、BDEV）单一权威在 `minix-types::types::device`——chardriver 与 vfs 的 cdev/bdev 消费者侧前轮已收敛；本轮删掉 blockdriver 最后四条本地副本（`BDEV_REQUEST_BASE`/`BDEV_READ_ACCESS`/`BDEV_WRITE_ACCESS`/`BDEV_FORCE_WRITE`）改为 `pub use minix_types::{BDEV_FORCEWRITE, BDEV_R_BIT, BDEV_RQ_BASE, BDEV_W_BIT}`，内部三处与 `os/net/lwip/src/bpfdev.rs` 一行引用随动；device.rs 补块族旗标三枚（com.h:986-988）。单消费者族（NDEV/RTCDEV/USB）按 device.rs 模块头既定边界留在属主 | `grep -rn 'BDEV_REQUEST_BASE\|BDEV_READ_ACCESS\|BDEV_WRITE_ACCESS\|BDEV_FORCE_WRITE' os --include='*.rs'` → 0；`cargo test -p minix-types -p minix-blockdriver` + workspace 编译面 |
| B3 | `SD-11` T4：全局分配器的根修（`PD-28`） | `TODO-LEDGER-OPEN.md:57`；`STRUCTURAL-DEBT-REGISTER-20261008.md` 的 `SD-11` | `☐` **非单轮量级** | **进度（`2026-10-18`，`claim/SD11-MFS-zcode-ds`）**：按 U-5 裁决边界领首个独占切片——迁移波第九台 MFS（最后一个缓存大头）落地：`os/fs/mfs/src/main.rs` 在 server 构造后、`serve` 前安装 `install_vm_supplier` + `nk4a: mfs-vmbrk probe ok` 探针；宿主 178/0、三架构 none 构建全绿；跨线编辑已在 C 板 §6 登记。**余下两步均门控**：① U-5 执行清单第 2 项（mfs 迁移前后对照：分配路径读数、出生段池峰值、驻留成本）+ x86/aarch64 全量 boot 门——走 C 板真机窗；② 三条件齐备后才删 `alloc.rs` 的 x86/non-x86 `cfg` 分档、统一 1024 页（U-5 裁决明写「验收不过则 T4 保持打开」，不在门禁前改容量）。原执行图：按四级排序选首个迁移服务，落「执行与跟踪分离」的根修；**删除分档是显式验收项**。消费者是 VM 服务（A14），本板持 `minix-rt` 侧主线，跨线先登记。**U-5 对照读数准备（`2026-10-18`，`claim/SD11-MFS2-zcode-ds`）**：② 出生段池峰值探针已加（`nk4a: mfs pre-install pool pages used=<n> total=1024`，安装点前打印，串口读数待真机窗）；③ 驻留 `size` → `.bss 4,195,496 B`（≈4 MiB 池 + 1.2 KiB）；三架构 none 构建与宿主 178/0 复跑通过。 | `cargo test -p minix-rt`；A14 的 VM 宿主门；`FixedPoolSupplier` 不再焊死生产全局分配器 |
| B4 | 运行时 csu 链 13 项缺口 | `14-stage-runtime/todo.md:75` | `✅`（`2026-10-18` 逐项对账收口） | 13 项逐项表见 §2.5：8 项已有 Rust 等价面或已落（入口检查、`ps_strings`/`environ` 的访问器等价面、短名、分配器守卫、无参 `main`、入口寄存器、`exit`），2 项静态不可达（动态链接、`MCRT0`），3 项维持登记（preinit/init/fini 数组与 `atexit`——消费者为零：仓内无 `.init_array`/`.fini_array` 使用、无 `atexit` 调用，unknown-none 不链 crt 对象）。**无可实施项，按消费者规则收口**；将来引入 C 对象库或构造器登记再接线 | 逐项表 + 代码锚点重放（无代码改动；`cargo test -p minix-rt` 基线值守） |
| B5 | `V1-P1-1` 的剩余真机面 | `14-stage-runtime/todo.md:23`（诞生链已标 ✅）与 `TODO-LEDGER-OPEN.md:129`（记「真机 2」） | `✅`（`2026-10-18`，本笔提交） | **对账已完成，无需上机**：「✅ 的那一半」= 诞生链两步（第一步删 `start.rs` 死码、第二步 `rt-birth` 真机五断言）与 `V1-P1-5` 文档落稿，均阶段内闭合；「真机 2」具名 = `real-trap` 与 `panic-handler` 两个 feature 面——前者已改按 `target_os = "none"` 判定（feature 只剩宿主强制，`os/libs/minix-rt/src/init.rs` 的 cfg 与 `rt-birth/Cargo.toml` 的 `T-STAB-1` 注），后者已由 `rt-birth`/`pfs`/`mfs` 等 freestanding 二进制消费；真机验证归总账 §4 的 `E1` 行（C 线窗）。`V1-P1-1` 自身余项 `fini_array`/`init_array`（无消费方）归 B4 的 csu 链 | 台账与阶段账已同步；QEMU 用量 0 |
| B6 | `at_wini`（IDE 存储）真机尾件 | `16-stage-drivers/todo.md:165`（§7 起首）到 `:177`（尾件清单） | `⏸ 等 C 板真机窗` | `w_probe` 的 PCI 枚举与 `BlockDriver::transfer` 的「读设备到暂存 → grant 拷贝 → 每扇区 IRQ/超时」两段；claim 键用 `AT-WINI`（追踪键，不是权威台账 ID）。**`2026-10-18`**：服务壳已随 `G6-BLK` 落到 `drv.blk.at_wini`（泵在 `minix-blockdriver::pump`），`--lib` 现 **49/0**（旧基线 44 为更早读数，本片 +1 冒烟门）；真机两段不变 | `cargo test -p minix-driver-at_wini --lib` → 49/0；真机腿走 C 板窗 |

## 1b. 二线跨阶段承接清单（原板遗漏的库面/驱动面开放项）

这一节与第 1 节平级；一次只领一个条目 ID。账载计数可能早于现树，领取前先复核正文。

| # | 条目/族 | 登记处 | 状态 | 这一轮做到哪算完 | 判据与门 |
|---|---|---|---|---|---|
| B7 | 16 篇其余开口项：`A2` 框架接线计划、`A10` 钩子签名 `Result` 化、`OQ-1`/`E-SDEVOWN`、`OQ-3` pci `BusControl`；另含输入驱动族的 `E-CDRCONV`/`E-TTYEVENT`/`E-PCKBDREG` | `16-stage-drivers/todo.md:76`、`:126`、`:145`、`:147`；`12-stage-input/todo.md:3` | `✅`（`2026-10-18` 逐项对账收口） | `G7` 已收口（doc 04/13/21 共 8 处）。**本行全项已有终态**：`A2` ✅（本笔收口——九批接线主体在树，net 九家 stub 依 edge2 L13 维持占位，typed CDEV 臂尾巴跟踪在 B9）；`A7` 🚫 否决（edge2 L13；`2026-10-18` 误执行一轮已 revert，条目与本节均已内联裁决）；`A10` ✅（edge2 L5 的 `338bfdf9f`，钩子签名迁 `Result` 全部落地）；`OQ-1` ✅（E-SDEVOWN 随 edge2 L8 的 `907e65f79` 迁入 `minix-sockdriver`）；`OQ-3` 🚫 保留（edge2 L13 死代码四裁决之一，服务层落地时再验）；输入三边 `E-CDRCONV`/`E-TTYEVENT`/`E-PCKBDREG` 均 ✅（edge2 L4/L6/L7）。**板行维护教训**：改板与领项前先 grep `edge*.md` 的裁决行，别把已有终态的项当开口项排期 | 每项自己的 crate 门 |
| B8 | `SD-19` 余下库 crate 的静默吞错门 | `TODO-LEDGER-OPEN.md:66`（本片进度注记在 §3 表后） | `✅`（`2026-10-18` 选项①执行） | `minix-vtreefs`/`minix-sffs`/`minix-types`/`minix-driver-rt` 四片早前已落；本笔按选项①把 `os/libs` 剩余十二家接上同款头门（`audiodriver`/`bdev`/`blockdriver`/`boot`/`chardriver`/`elf`/`i2cdriver`/`netdriver`/`platform`/`sockdriver`/`usb`/`virtio`），**二十一家全带门**。真重检新暴露 5 处真触发（`netdriver` 泵 3、`blockdriver` 泵 2，均为回复/状态发送尽力而为），逐站 `//[SD19-gate]`+语句级 `#[allow]` 收编；其余十家自有文件 must_use 真触发 0。**行内旧断言「均非 must_use」被本笔证伪**（旧记未对两家泵代码真跑门）。选项②所需的 B7/G7/B5 亦已全收口。`commands/*` 零散站归 A 板；**不升 `deny`** | 每 crate `cargo clippy -j 1 -p <pkg> --tests` 读数 + 处置表（本笔表见 §3 表后注）；无理由的新 `let _ =` 为 0 |
| B9 | 共享 wire/查询桩：`E-RSWIRE`、`E-KERNINFO`、`E-MINTYPES-RS`、`E-MINTYPES-RUNTIME`、`E-DSWIRE` 客户端半、`E-DMWIRE` 客户端半（边界登记见 C 板 §6）、`E-DEVWIRE` 常量 | `TODO-LEDGER-OPEN.md:80`、`:83`；`16-stage-drivers/todo.md:120`；`14-stage-runtime/todo.md:10` | `✅`（`2026-10-18` 逐项对账收口） | 七项逐项终态：`E-RSWIRE` ✅（DS 侧 shadow 半与 VM 侧 `403e47022` 两段进度注记）；`E-KERNINFO` ✅（字面「余项仅剩归档注记」）；`E-MINTYPES-RS` ✅（观察一/三闭环，观察二随 E7 执行轮）；`E-MINTYPES-RUNTIME` 🚫 执行半按 edge_todo 对账归 14-stage-runtime 轨道；`E-DSWIRE` ✅ 客户端半（DS 传输链真机通电注记）；`E-DMWIRE` ✅ 客户端/RS 半（`b05905406` 落地客户端生产传输与 RS publish 臂；本笔修 grant 收方——原按 `NONE` 签发会被内核 who_to 检查拒为 EPERM，改为按 C `cpf_grant_direct(devman_ep,…)` 绑定收方）；`E-DEVWIRE` ✅（edge2 L5）。**B 侧零剩余**：devman 请求分类器等服务端半按板行归 A（A17/A18）；typed 消息臂由 runtime 轨道接 | 本笔 `cargo test -p minix-sys` → 327/0、clippy 自有文件本门告警 0；其余条目宿主门见各提交 |
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
