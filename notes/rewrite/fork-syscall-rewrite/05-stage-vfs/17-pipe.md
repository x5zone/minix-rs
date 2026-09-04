# 17 — pipe：配额定量、挂起账本、唤醒扫描与信号中断

本文讲清管道调用如何在“建管→定量→挂起→唤醒→中断”的五段生命中，以存量、对端、容量为三组开关决定一次读写能走多少字节，以挂起账本记录谁在等，以唤醒扫描精确叫醒对的人，并在信号到达时以“有进展回数、无进展回错”收尾。

前置阅读：`16-read-write.md`（五路分派 verdict 与管道数学）、`02-fproc-struct.md`（`BlockedOn` 的七态与 `PipeBlock` 五字段）、`04-filp-table.md`（`find_filp` 的对端存在性）。

> 本章不讲什么：
> - `req_newnode/req_readwrite` 的 FS 协议执行—— `12-request-wrappers.md`
> - `select_callback/select_unsuspend` 的多路复用执行—— `23-select.md`
> - `sdev_stop/sdev_cancel/cdev_cancel` 的驱动取消执行—— `22-sdev.md` / `21-cdev.md`
> - `replycode` 的主循环回复路由—— `09-main-loop.md`
> - 记录锁的挂起（`FLOCK` 分支的语义）—— `30-fcntl-lock.md`
> - 字符/socket 等待者的完整语义—— `21-cdev.md` / `22-sdev.md`（本篇只给驱散分类）

---

## 1 概念

### 1.1 为什么管道必须会等

普通文件的读写面朝一个不会变的状态：文件在，偏移在，要多少有多少（至多 EOF）。管道面朝的是另一个进程：读时对端可能还没写，写时对端可能还没读，甚至对端可能已经永远消失。`pipe.c:195-199` 的注释把三种处境写成了散文——空管有写者则等、无写者则回零、写时无读者则 broken——定量机的全部逻辑就是这三句话的形式化。

“等”因此不是异常，而是管道的常态机制：挂起（suspend）登记等待，唤醒（release/revive）兑现等待，账本（susp_count）清点等待。三者缺一，管道要么丢唤醒（睡着叫不醒），要么错唤醒（叫醒无关的人）。

### 1.2 配额的双向不对称

读与写的定量看的是不同的东西：读看存量（`v_size` 有多少就最多给多少），写看对端与容量（有人听吗？装得下吗？）。读的矩阵是三路——非空全给、空加写者则等或快失败、空且无写者回零；写的矩阵是五路——无读者 broken、装得下全给、装不下按阻塞与原子性分四种。

不对称的根因是管道的 `v_size` 不是长度而是存量：读消耗存量（减），写增加存量（加）。普通文件的尺寸只增不减（写生长），管道的存量有增有减——这是两类 `v_size` 语义的根本分歧，读懂它就读懂了为什么管道读写不经过 `filp_pos`。

### 1.3 挂起与唤醒的配对

挂起分两步：登记（`suspend` 记原因、`pipe_suspend` 存五参数）与计数（`susp_count++`）。唤醒也分两步：扫描（`release` 按“开配开、读写配调用”找人）与标记（`revive` 置 `FP_REVIVED`、`reviving++`）。登记与标记之间隔着一次主循环——被标记的进程不在 `release` 里直接回复，而由主循环稍后统一处理（09 的管辖）。

延迟回复的原因藏在 `revive` 的注释里（`450-454`）：管道与锁的等待者需要“更多处理”，而其他等待者可直接回复。直接回复要求调用线程不阻塞（`437-439` 的 MUST NOT block），管道的恢复涉及 FS 往返，做不到这一点——于是拆成标记与处理两步。

### 1.4 计数的全局账本

`susp_count` 是全服唯一的等待者计数：挂起时加（只计管道两类），唤醒时减，减到负即 panic。负值 panic 不是防御，而是完整性断言——加减配对是唤醒机制的生命线，一次多减意味着一次唤醒找错了人，继续运行只会错上加错。

`reviving` 是账本的另一面：已标记、待主循环处理的人数。`unpause` 消费它（`518`），`revive` 生产它（`459`）——生产与消费分属两个函数，配对靠主循环的时序保证。这是全文件最脆弱的契约，也是 Rust 改写把它收敛为显式账本的原因。

### 1.5 信号的中断语义

`unpause` 回答一个问题：睡着的系统调用被信号打断时，返回什么？答案分两层：管道读写有进展就回进展数（partial 的字节数不是错误），其余一律 `EINTR`；计数上，已标记的走 `reviving--`，未标记的管道类走 `susp_count--`。先清阻塞状态再执行取消（`514` 的注释写明“imperative”）——取消动作（`cdev_cancel/sdev_cancel`）可能阻塞，绝不能让 VFS 其他部分在此期间误以为进程还在睡。

socket 是唯一的例外：`sdev_cancel` 自己发回复，`unpause` 直接返回（`548-549`）。例外的原因是 socket 清理太复杂，放不进统一回复——复杂到必须把回复权下放。

### 1.6 命名与匿名管道的统一

`create_pipe` 建的是匿名管（无路径、PFS 寄养、双 fd），`map_vnode` 把命名管（FIFO，有路径、有归属 FS）接到 PFS 代管数据。两者在 `req_newnode(I_NAMED_PIPE)` 处会合——匿名与命名在 PipeFS 看来是同一种 inode，区别只在谁持有目录项。统一之后，读写机无需区分 FIFO 与 pipe：`S_ISFIFO` 分支（16 §2.4）走同一套定量与挂起。

`map_vnode` 的 EBUSY 分支（`162-163`）是全文件最微妙的三行：锁忙说明已持有，不重复解。加锁与解锁的配对义务在此出现唯一的例外——例外本身被显式编码，而不是注释一句了事。

### 1.7 与其他 OS 的管道对照

- **Linux** 以 `pipe_inode_info`（环形缓冲 + `wait_queue_head` 读写等待队列 + 原子 `PIPE_BUF` 4096 语义）实现同构机制：`pipe_check` 的定量矩阵对应 `pipe_readable/pipe_writable` 的等待条件，`susp_count` 对应等待队列长度，`release` 的扫描对应 `wake_up_interruptible` 的精确唤醒。
- **Redox** 的 pipe scheme 以 `VecDeque<u8>` 缓冲 + 阻塞 `read/write`（`scheme.rs` 的 `handle_packet` 挂起）对应挂起三件套；`O_NONBLOCK` 的 `EAGAIN` 快失败语义一致；Rust 改写的 verdict 风格与 Redox 以 `Result` 表达阻塞（`Err(Error::WouldBlock)`）同源。
- **seL4** 无管道原语，流通信由用户态在 endpoint 上以协议模拟；VFS 的内核侧挂起账本在 seL4 中对应客户端自持的等待状态机——差异的根因仍是状态服务器 vs 无状态内核。

### 1.8 小结

管道是五段生命（建管→定量→挂起→唤醒→中断），三组开关（存量/对端/容量）决定配额，一本账本（susp/reviving）清点等待，一套谓词（开配开/读写配调用）精确唤醒。记住“配额守恒、账本配对、唤醒精确、中断回数”四条，就记住了本篇全部。

---

## 2 C 源码分析

### 2.1 `do_pipe2/create_pipe` 建管机（`pipe.c:39-144`）

`do_pipe2` 合并新旧标志位（`45-46`，`oflags` 为兼容保留）后调 `create_pipe`，成功才回填 fd 对（`49-52`）。`create_pipe` 七步：锁 PFS（`70-71`，`PFS_PROC_NR` 见 `minix3/minix/include/minix/com.h:68`，失踪即 panic）→ 取空 vnode（`74-77`）→ 取读 fd（`82-86`，失败只解 vnode 与 vmnt）→ 取写 fd（`89-96`，失败回滚读端）→ `req_newnode(I_NAMED_PIPE)`（`101-102`，失败回滚双端）→ 填 vnode 九字段（`117-127`：双端点、双 inode 号、模式、双计数、引用、尺寸 0、空挂载、`NO_DEV`）→ 填双 filp（`130-138`：同 vnode + `dup_vnode`、读 `O_RDONLY`/写 `O_WRONLY` 叠共享位、双端 CLOEXEC）→ 解锁返回（`140-143`）。

### 2.2 `map_vnode` 映射机（`pipe.c:151-182`）

已映射短路（`157`）→ 目标 vmnt 缺席 panic（`159-160`）→ 加写锁，`EBUSY` 记免解（`161-167`）→ `req_newnode` 落定三字段（`172-176`：映射端点、映射 inode、映射计数 1）→ 按需解锁返回（`179-181`）。

### 2.3 `pipe_check` 定量机（`pipe.c:187-288`）

读三路（`217-235`）：非空全给（`234`）；空加写者则非阻塞 `EAGAIN`/阻塞 `SUSPEND`（`222-225`），且有等待者即唤醒写者（`229-230`，check-only 亦不豁免）；空且无写者回 0（`232`）。写五路（`237-287`）：无读者 `EPIPE`（`238-240`）；超容量非阻塞且原子尺寸 `EAGAIN`（`244-248`）；超容量非阻塞大写 partial 减容并唤醒读者（`250-257`，满则 `EAGAIN`）；超容量阻塞 partial 减容唤醒（`264-274`）；满则 `SUSPEND`（`279`）。空管写唤醒读者（`283-284`，check-only 豁免），余量全给（`287`）。

### 2.4 `suspend/pipe_suspend` 登记机（`pipe.c:295-328`）

`suspend` 断言未阻塞（`302`），管道两类计数（`304-306`），登记原因（`308`）。`pipe_suspend` 存五参数（`322-326`：调用号/fd/缓冲/余量/已得）后登记 `FP_BLOCKED_ON_PIPE`（`327`）——五字段与 02 的 `PipeBlock` 同构。

### 2.5 `release` 扫描机（`pipe.c:363-429`）

select 相（`379-394`）：读写操作映射 `SEL_RD/WR`（`380-383`），扫 filp 表回调并清位（`385-393`）。proc 相（`397-427`）：`op` 取 `VFS_OPEN/VFS_READ/VFS_WRITE`（见 `minix3/minix/include/minix/callnr.h:72-75`），开配开、读写配调用、未复活三元合取（`403-407`），按挂起原因取 fd（`411-414`），跳过空 filp/已关（`416-417`）与异 vnode（`418-419`），`revive` 后递减计数（`422-423`），负值 panic（`424-425`），配额耗尽早停（`426`）。

### 2.6 `revive` 标记机（`pipe.c:435-492`）

坏端点/未阻塞/已复活三门直返（`445-448`）。管道与锁标记延迟（`456-459`：置 `FP_REVIVED`、`reviving++`）。余者立即回复：开回复 fd（`463-466`）、选择回复码（`467-469`）、字符先撤销授权再回复（`470-481`）、socket 禁用（`482-487`，panic）、未知 panic（`488-490`）。

### 2.7 `unpause` 中断机（`pipe.c:498-561`）

未阻塞直返（`506`）。先清阻塞状态（`514`，注释 imperative），消费复活标记（`516-520`）。六分支（`522-553`）：管道有进展回数余 `EINTR`（`523-529`）、锁与开回 `EINTR`（`531-539`）、选择遗忘（`534-536`）、字符取消（`541-545`）、socket 自回复直返（`547-549`）、未知 panic（`551-553`）。管道类未复活减账本（`555-558`），统一回复（`560`）。

### 2.8 `unsuspend_by_endpt` 驱散机（`pipe.c:335-357`）

扫 proc 表：字符等待者匹配端点即 `EIO` 复活（`344-346`），socket 等待者匹配设备即 `sdev_stop`（`347-350`）。select 等待者另行驱散（`354`，23 管辖）。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `pipe.c` 的扫描循环，而是吸收 Linux/Redox 的等待队列模型后做取舍。以下决策对应 `.design/17-design.v1.md` D1-D7。

### D1 定量纯函数

- **C**：`pipe_check` 内嵌 `find_filp` 扫描与 `release` 副作用（`pipe.c:220,229,238,256,273,284`）。
- **Rust**：`pipe_check_decision(dir, buffered, capacity, reader, writer, nonblock, touch, requested, waiters) -> PipeCheckVerdict::{Allow{bytes,wake}, Suspend(WakePlan), Reject}`（`os/servers/vfs/src/pipe.rs:86`）。
- **为什么**：定量数学与对端查询/唤醒执行分离；`Allow` 自带唤醒计划使调用点无需二次查账。替代方案（verdict 不带唤醒、调用点重算）被否决：重算即重复矩阵，重复即漂移——EAGAIN 快失败仍欠一次唤醒（`229`）是最好的证据。

### D2 建管分阶段与回滚表

- **C**：七阶段直线代码，回滚散在三处（`pipe.c:82-96,104-113`）。
- **Rust**：`PipeNodeFactory` trait（`MemPipeFs` 常成功 vs `FailPipeFs` 常 `EIO`）+ `CreateStage` 四值 + `rollback_for` 纯表 + `end_flags` 标志拆分（`os/servers/vfs/src/pipe.rs:254,189,216,286`）。
- **为什么**：回滚义务随阶段单调增长，表使“第 N 步失败回滚什么”一测即知。`dup_vnode` 语义留调用点（05 管辖），`PFS` 加锁留 06（vmnt 管辖）。

### D3 账本显式

- **C**：`susp_count++/--` 裸整数 + 负值 panic（`pipe.c:306,423-425`）。
- **Rust**：`SuspLedger/ReviveLedger` 新型 + `dec_checked` 守卫 + `susp_delta` 计数规则（`os/servers/vfs/src/pipe.rs:296,353,331`）。
- **为什么**：加减配对跨三函数，裸整数无法表达义务；守卫把“双唤醒 bug”变成可断言的返回值而非崩溃。替代方案（`debug_assert`）被否决：测试构建中断言触发即失败，不可测——守卫值才是可测的。

### D4 唤醒谓词

- **C**：匹配规则内嵌 proc 扫描循环（`pipe.c:403-419`）。
- **Rust**：`release_match(live, blocked, op, revived, filp_ok, same_vnode) -> bool` 六元合取 + `select_ack` 位清除（`os/servers/vfs/src/pipe.rs:421,440`）。
- **为什么**：六元合取的组合可单测全覆盖；select 相与 proc 相分离与 C 同构（`379`/`397` 两相）。

### D5 复活 verdict

- **C**：两门 + 标记 + 四回复分支 + SDEV panic（`pipe.c:445-490`）。
- **Rust**：`revive_decision -> ReviveVerdict::{Noop, MarkReviving, Reply(i32), Invalid}`（`os/servers/vfs/src/pipe.rs:487`）；SDEV/未知 → `Invalid→EIO`。
- **为什么**：标记与回复的分化是复活的核心知识；`Invalid` 是 ARCH 加固（C 视其为不可能，panic 即整服崩溃）。

### D6 中断 verdict

- **C**：六分支的回复值×取消动作×计数调整交织（`pipe.c:522-560`）。
- **Rust**：`unpause_decision -> UnpausePlan{reply, cancel, dec_susp, dec_reviving}`（`os/servers/vfs/src/pipe.rs:565`）；`CancelOp::{None, ForgetSelect, CancelCdev, CancelSdev}`。
- **为什么**：三元组是中断的全部知识；SDEV 自回复以 `CancelSdev` 显式（调用点直返，不经过统一 replycode）。先清状态的时序义务留调用点注释。

### D7 映射 verdict

- **C**：短路 + panic + EBUSY 免解 + 落定散列（`pipe.c:157-181`）。
- **Rust**：`map_decision -> MapVerdict::{AlreadyMapped, Proceed(UnlockNote), Absent}`（`os/servers/vfs/src/pipe.rs:610`）。
- **为什么**：免解是映射最微妙的知识；`UnlockNote::{Unlock, SkipUnlock}` 使调用点解锁义务显式，不可遗漏。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-1 单线程事件循环（mthread→状态机） | 挂起/唤醒皆 verdict，执行留 08/09 | `pipe.rs:59,469,549` + 本文档 D1/D5/D6 + 17 正文 §1.3 |
| A-4 全局聚合（glo.h→状态聚合） | `SuspLedger/ReviveLedger` 新型先行 | `pipe.rs:296,353` + 本文档 D3 + 17 正文 §1.4 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── pipe.rs               — 本篇：定量/账本/唤醒/复活/中断判定
├── fproc.rs              — PipeBlock/PipeIo 复用（02，不重复定义）
├── read_write.rs         — RwDir 复用 + PartialVerdict 承接（16）
└── open.rs               — end_flags 的 ACCMODE 语义（15）
```

> 设计决策：§3 D1（定量纯函数）/ D3（账本显式）/ D5（复活 verdict）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `PIPE_BUF` | `syslimits.h:66` | `pipe.rs:25` | 32768（`__minix`） |
| `do_pipe2` 标志合并 | `pipe.c:45-46` | `pipe.rs:29 merge_pipe2_flags` | 或合并 |
| `pipe_check` | `pipe.c:187` | `pipe.rs:86 pipe_check_decision` | 读写矩阵纯函数 |
| 空读 EAGAIN 唤醒 | `pipe.c:229` | `pipe.rs:140 read_reject_wake` | 快失败仍欠唤醒 |
| `create_pipe` | `pipe.c:60` | `pipe.rs:254,189,216,286` | 工厂 + 阶段 + 回滚 + 标志 |
| `susp_count` | `glo.h:14` | `pipe.rs:296 SuspLedger` | 加减 + 下溢守卫 |
| `reviving` | `glo.h:16` | `pipe.rs:353 ReviveLedger` | 标记计数 |
| `pipe_suspend` | `pipe.c:315` | `pipe.rs:385 suspend_record` | 复用 PipeBlock |
| `release` 匹配 | `pipe.c:403-419` | `pipe.rs:421 release_match` | 六元合取 |
| select 清位 | `pipe.c:392` | `pipe.rs:440 select_ack` | 位清除 |
| 驱散分类 | `pipe.c:344-350` | `pipe.rs:457 classify_driver_waiter` | 三分类 |
| `revive` | `pipe.c:435` | `pipe.rs:469,487` | verdict 五值 |
| `unpause` | `pipe.c:498` | `pipe.rs:527,549,565` | 回复 + 取消 + 计划 |
| `map_vnode` | `pipe.c:151` | `pipe.rs:591,610` | verdict 三值 |
| 错误族 | `pipe.c` 全文件 | `pipe.rs:629,644 PipeError::to_errno` | 5 变体→errno，无自创 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 配额守恒 | `pipe_check_decision` | 读≤存量，写≤容量 | `pipe.c:234,251,266` |
| 账本配对 | `dec_checked` | 下溢可断言 | `pipe.c:423-425` |
| 唤醒精确 | `release_match` | 六元合取 | `pipe.c:403-407` |
| 中断回数 | `UnpausePlan.reply` | 进展回数余 EINTR | `pipe.c:527-528` |
| 映射幂等 | `MapVerdict::AlreadyMapped` | 已映射短路 | `pipe.c:157` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **228 passed / 0 failed**（既有 215 + 本篇新增 13；`minix-types` 独立）。
> 本章直接影响 13 项新增。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_pipe2_flag_merge` | `pipe.c:45-46,133-134` | 标志合并 + 双端拆分 | `pipe.rs:660` |
| `test_create_rollback_table` | `pipe.c:82-113` | 回滚单调三样本 | `pipe.rs:674` |
| `test_factories_differ` | design D2 | 双工厂分化 + trait 多态 | `pipe.rs:700` |
| `test_read_sizing_matrix` | `pipe.c:217-235` | 读三路 + 快失败唤醒 + check-only | `pipe.rs:712` |
| `test_write_sizing_matrix` | `pipe.c:237-287` | 写五路 + 原子/EAGAIN 边界 | `pipe.rs:750` |
| `test_susp_ledgers` | `pipe.c:304-306` + glo.h | 计数规则 + 加减守卫 | `pipe.rs:808` |
| `test_suspend_record_reuses_pipe_block` | `pipe.c:322-327` | 五字段复用 | `pipe.rs:827` |
| `test_release_match_conjunction` | `pipe.c:403-419,392` | 六元合取 9 样本 + 清位 | `pipe.rs:837` |
| `test_revive_verdicts` | `pipe.c:445-490` | 两门 + 标记 + 回复 + 禁用 | `pipe.rs:857` |
| `test_unpause_plans` | `pipe.c:503-560` | 回数/EINTR + 取消三类 + 计数 | `pipe.rs:881` |
| `test_map_verdicts` | `pipe.c:157-167` | 短路 + 缺席 + 免解 | `pipe.rs:905` |
| `test_driver_waiter_classification` | `pipe.c:344-350` | 三分类 + 优先 | `pipe.rs:916` |
| `test_errno_map_covers_pipe_c` | `pipe.c` 全文件 | 5 变体→errno 全映射 | `pipe.rs:925` |

测试策略：定量以读写矩阵全样本覆盖（含 EAGAIN 仍唤醒的反直觉项与 atomic 边界项）；建管以回滚单调性覆盖；账本以加减守卫覆盖；唤醒以合取真值表覆盖；复活/中断以分支全覆盖；错误以 5 变体全映射覆盖。

### 5.1 测试统计（截至 2026-09-03）

- `cargo test -p minix-vfs --lib`：**228 passed / 0 failed**
- 本节列出与本模块直接相关的 13 个（子集）
- 完整测试清单：`rg "fn test_" os/servers/vfs/src/pipe.rs`

---

## 6 过渡

本篇在 16（分派 verdict）之后、18（挂载）之前，是“挂起执行”的归属层：16 只判定“该挂起”，本篇执行挂起的登记、扫描、标记与中断；没有本篇，`SUSPEND` 只是无人兑现的承诺。

```
16-read-write: select_route → Pipe verdict（该挂起）
   │
   └─► 本篇：pipe_check 定量 → suspend 登记 → release 扫描 → revive 标记 → unpause 中断
          │                        │                  │
          ├─► 08-worker-thread：suspend/resume 的调度执行
          ├─► 09-main-loop：reviving 的延迟处理与 replycode
          └─► 23-select：select 相回调与驱散的执行
```

阅读顺序提示：若关心“标记之后谁来处理”，下一站 `09-main-loop.md`（`reviving` 的延迟处理）；若关心“根文件系统从哪来”，下一站 `18-mount.md`（`mount_fs` 与根挂载）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/pipe.c:1-561`（`do_pipe2/create_pipe/map_vnode/pipe_check/suspend/pipe_suspend/unsuspend_by_endpt/release/revive/unpause`）、`minix3/minix/servers/vfs/glo.h:14-16`（`susp_count/reviving`）、`minix3/minix/servers/vfs/const.h:19-25`（`FP_BLOCKED_ON_*`）、`minix3/minix/include/minix/com.h:1151`（`SUSPEND`）、`minix3/sys/sys/syslimits.h:66`（`PIPE_BUF`）
- 阶段文档：`16-read-write.md`（分派 verdict）、`02-fproc-struct.md`（`BlockedOn/PipeBlock`）、`04-filp-table.md`（对端存在性）、`09-main-loop.md`（延迟处理）、`08-worker-thread.md`（挂起执行）
- Rust 实现：`os/servers/vfs/src/pipe.rs:1`（本篇判定层）、`os/servers/vfs/src/fproc.rs:94`（`BlockedOn` 七态）、`os/servers/vfs/src/read_write.rs:1`（`RwDir` 与续挂判定）、`os/libs/minix-types/src/types/errno.rs:15`（errno 值）
- 内核侧：`../01-stage-kernel/18-syscall-copy.md`（用户缓冲拷贝语义）
