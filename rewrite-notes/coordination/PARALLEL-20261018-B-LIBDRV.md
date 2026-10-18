# 并发任务板 B — 二线·用户态运行时、库与驱动框架（`os/libs/` 逐 crate、`os/drivers/`）

> **这份文件是什么**：`2026-10-18` 全仓库开放项按**领地互斥**切成的三份之一。切分依据是 `coordination/CONCURRENT-WORK-PROTOCOL.md` §2 的四线所有权清单——三份文件的可写路径两两不相交，这是并发不打架的唯一硬保证。
> **配套件**：`PARALLEL-20261018-A-CMD.md`（三线 命令面）、`PARALLEL-20261018-C-KERN-QEMU.md`（一线 内核/引导，**独占真机窗**）。
> **本板领地的关键性**：协议 §2 写明二线是另外两线的**依赖下游汇聚点**。也就是说本板一次改动会同时改变 A 与 C 的门读数——所以本板的每一条都要「先量下游、再动上游」，且**改公开签名前必须先在 C 板的跨界登记行挂账**。
> **条目状态词**（五值）：`☐` / `🔄` / `⏸`（注明等谁）/ `✅`（带日期与提交号）/ `🚫`（写明为什么现在不做）。

---

## 0. 你（接手的 agent）开工前的四件事

1. **owner 名固定 `zcode-ds`**，领取一律 `bash tools/claim.sh claim <条目ID> zcode-ds`；别手搓 worktree，别在主树 `checkout`/`reset`。
2. **先复核锚点再领**：本板行号是切板时刻的快照。领之前跑 `grep -n "<条目ID>" rewrite-notes/16-stage-drivers/todo.md`（14/11 篇同理）。「账面滞后」在这个仓库是常态——`14-stage-runtime/todo.md:23` 已把 `V1-P1-1` 标 ✅，而 `coordination/TODO-LEDGER-OPEN.md:124` 仍记「真机 2」。**遇到这种不一致，先做对账再动码，并把对账读数留在账里。**
3. **一次一条**；改 `os/libs/` 任一 crate 的公开面之前，先跑下游全包门（第 3 节）拿基线。
4. **门读数必须 L1**：命令 + 输出成对，推断不算过。

## 1. 任务表（按 P0→P1→P2 排）

| # | 条目 ID | 登记处 | 状态 | 这一轮做到哪算完 | 判据与门 |
|---|---|---|---|---|---|
| B1 | `G6` 服务二进制层整体为零（57 个占位 `main.rs`） | `rewrite-notes/16-stage-drivers/todo.md:68` | `☐` **非单轮量级** | **按驱动族分批**，每轮一族（先串口/控制台族，再存储族，再网络族），每族落地标准是那个 `main.rs` 从占位骨架变成真实事件循环 + 至少一枚可跑的门。**一轮内不扫全 57 个** | 每批重跑 `-p` 该 crate 的宿主门；占位计数用 `for f in $(find os/drivers -name main.rs); do [ $(wc -l <$f) -le 20 ]; done | wc -l` 这类可重跑命令量，别用印象 |
| B2 | `A4` 协议常量单一来源 | `16-stage-drivers/todo.md:120` | `☐` | C 的 `com.h` 是一份头文件，Rust 侧常量散落 → 收敛到单一来源并被消费。范围只在本板领地内的驱动常量，**跨到 `os/libs/minix-types` 的那部分先登记** | 逐常量对 `minix3/` 原头文件，附文件行号 |
| B3 | `E-DMWIRE` 设备管理生产四缺 | `rewrite-notes/11-stage-devman/todo.md:201` | `☐` | 四缺逐条读原文再定切片；本条**领地归属需要确认**（11 阶段落在 devman，物理路径含 `os/drivers/` 与 `os/servers/`）——**开工前先在 C 板的跨界登记行挂一行**，写明你要碰哪几个文件 | 每缺一枚装配门 |
| B4 | 运行时 csu 链 13 项缺口 | `rewrite-notes/14-stage-runtime/todo.md:75` | `☐` | `ps_strings`/`environ`/`progname` 全局、`atexit`/`fini_array` 等 13 项，按「先有消费者再有面」的顺序逐项做；`minix-rt` 的诞生链已有交付面（`bin_support` 在用 `minix_rt::crt0::args()`），**别重复造** | `minix-rt` 宿主门 + 三架构 `--bins` 各 `Finished` |
| B5 | `V1-P1-1` 第二步真机验收 | `14-stage-runtime/todo.md:23`（已标 ✅）与 `TODO-LEDGER-OPEN.md:124`（记「真机 2」） | `⏸ 先对账` | 两本账不一致：先复核「✅ 的那一半」是否覆盖真机验收；若确实要上机，走 **C 板真机窗**，不在本板起 QEMU | 见 C 板第 4 节令牌配方 |
| B6 | `at_wini`（IDE 存储）真机尾件 | `16-stage-drivers/todo.md:165`（§7「at_wini 实装进度与真机尾件归属」起首，尾件清单在该节内到 `:177` 一带） | `⏸ 等 C 板真机窗` | 同上 | 同上 |

## 2. ⛔ 禁碰清单

| 不可写 | 属于谁 | 为什么硬 |
|---|---|---|
| `os/libs/minix-boot/` | **C 板**（协议 §2 明文确权：它的唯一消费者全在一线 `boot-shim`/`kernel`/`arch`/`qemu-tests`） | 引导契约面，改一次要三架构同时验 |
| `os/kernel/`、`os/arch/`、`os/plat/`、`os/boot-shim/`、`os/qemu-tests/` | C 板 | 真机面 |
| `os/commands/`、`os/etc/`、`os/xtask/` | A 板 | 命令面薄壳与 `bin_support` 双生面 |
| `os/servers/`、`os/fs/`、`os/net/` | 三线服务器腿（本轮无人持有，动前在 C 板跨界登记行挂账） | 与 B3/B6 有交集，最容易撞 |
| `rewrite-notes/01-stage-kernel/*`、`00-master-plan/*` | C 板 + **当前活跃文档线程**（今天有未提交在制品） | 只读 |
| `rewrite-notes/coordination/TODO-LEDGER-OPEN.md`、`TODO-LEDGER-DONE.md`、`new_edge*.md` | 编排线收敛 | 协议 §3：多写者必覆盖 |

## 3. 门配方（本板最要紧的是「下游基线」）

**动上游之前先量下游**，否则改完不知是谁变红：

- 下游基线（A 板的两个包，本会话刚复跑过的读数）：
  `cargo test --offline -j 1 -p minix-textfilter` → **1076 passed / 0 failed**（68 target 段）；`-p minix-text-games` → **60 passed / 0 failed**。
- 本板包的门：先跑 `cargo test --offline -j 1 -p <pkg>` 取一次基线，**把数字写进本表对应行的「判据与门」列**（本板切板时未替你量，因为二线的包多、必须按你实际领的那一族来定）。
- 内核宿主测必须带形态：`cargo test -p minix-kernel --features mock`（裸跑会被 `os/kernel/src/lib.rs` 的 `form_contract` 编译期绊线挡住——那是设计行为，别放宽它）。这条对 B3/B4 尤其相关。
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

## 6. 与另两板的依赖边

| 本板条目 | 关系 | 说明 |
|---|---|---|
| B1/B2 全部 | **A 板是下游** | 命令面薄壳通过 `os/libs/minix-sys` 的 wrapper 调你的面；改签名前在跨界行挂账 |
| B4 csu 13 项 | **C 板是上游** | 诞生链与 `minix-boot` 的契约面在一线；不许替一线实现它的前置 |
| B5/B6 真机 | 等 C 板 | 走令牌窗，不自起 QEMU |
| B3 `E-DMWIRE` | 领地需确权 | 先登记再动，避免与服务器腿撞 |
