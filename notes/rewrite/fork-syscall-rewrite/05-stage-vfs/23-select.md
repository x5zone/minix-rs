# 23 — select：一问等一群、两波才收场的就绪多路复用

本文讲清 `select` 如何把"等一群文件描述符"拆成问群→分型→首波→挂起→次波→复活六段：文件常就绪、管道可试探、字符与套接字去问驱动，首波没等到的挂起再等，驱动的次波回答与时钟的超时回答在同一扇门外会合——读写问一个 fd，select 问的是一群 fd 的交集。

前置阅读：`04-filp-table.md`（filp 选择字段）、`02-fproc-struct.md`（`BlockedOn::Select` 挂起记录）、`17-pipe.md`（`pipe_check` 试探执行）、`19-device-map.md`（dmap/smap 忙门）。

> 本章不讲什么：
> - `pipe_check` 的试探执行—— `17-pipe.md`（本篇只给试探的拼合规则）
> - `cdev_select/sdev_select` 的投递执行—— `21-cdev.md`/`22-sdev.md`（本篇只给投递的门与挂起）
> - `suspend/revive/set_timer` 的等待执行—— `08-worker-thread.md`/`09-main-loop.md`（本篇只给 verdict）
> - `fd_set` 的用户拷贝执行—— `../01-stage-kernel/18-syscall-copy.md`（本篇只给取整与方向）
> - 套接字调用与续作执行—— `24-socket.md`（本篇只给驱动对话层）
> - 字符/套接字首波之外的驱动对话—— `21-cdev.md`/`22-sdev.md`

---

## 1 概念

### 1.1 为什么 select 是"反向"调用

`read` 问的是一个 fd："有数据吗"。`select` 问的是一群 fd："你们谁有事"。方向反了：读写是点查询，多路复用是群查询。群查询有两种做法——轮询（一遍遍问"好了没"，CPU 空转）与中断（问一次，好了叫我）。`select` 是中断式：问完即走，有了再叫。代价是状态：轮询无状态，中断要记账——记"谁在等什么"，这是本篇全部复杂度的来源。

三问定去留：现在有吗（首波）→愿意等吗（阻塞位）→等到什么时候（超时）。三问全否即 poll：看一眼就回。

### 1.2 四种 fd 的四种脾气

回答从哪来，决定怎么问。文件最爽快：常就绪，不问自答。管道可试探：探一字节便知深浅（读探一字节、写探一字节）。字符与套接字最麻烦：答案在驱动手里，得发信去问，还得等回信。四分型是分发的前提——问之前先看脾气，脾气错了问也是白问（未知类型直接 `EBADF`）。

分型的顺序即优先级（`fdtypes[]` 表序）：字符、套接字先于文件、管道。一个 fd 只属一种，查中即停。

### 1.3 两波回答

首波是问完即有的回答：文件常就绪、管道探完即知、驱动恰好有数。次波是备好再叫的回答：驱动忙完回首波（"收到，在办"），备好回次波（"好了，来拿"）。两波之间是延期（deferred）：首波信还没回齐，交卷为时过早——`starting` 旗（还在摆桌子）与 `UPDATE|BUSY` 旗（还有信在路上）任一成立即延期。

"忙不重发"是两波之间的护栏：已发未回的 filp 不再发第二问（`FSF_BUSY` 则挂起），回了第一问才有资格发第二问（`restart_filps` 重发 `UPDATE` 者）。

### 1.4 挂起的账本

账本分三层。顶层是选择表：25 槽（`MAXSELECTS`），一槽一调用，空槽以 `requestor == NULL` 为记。中层是 filp 账：五旗（`UPDATE` 待告/`BUSY` 在途/读块/写块/错块）三数（选择人数/待办位/管道暂存）一设备（寄放的驱动号）。底层是进程挂起：`FP_BLOCKED_ON_SELECT`（见 `minix3/minix/servers/vfs/const.h:23`），复用 02 的 `BlockedOn::Select`（无载荷——账在选择表，不在进程）。

挂起即保活（头注释三知识之三）：进程挂在 select 上期间，所有相关 filp 保证不关——要么等完，要么被信号打断。账本的每一笔注销（`cancel`）都配一笔登记，这是 `selectors` 计数的全部意义。

### 1.5 超时的三态

等有三种：不等（poll，超时零零）、等到某时（限时）、永远等（不限）。poll 不是"超时为零"的特例，而是与"等"并列的第三态——C 用两布尔（`do_timeout`/`block`）拼出三态，Rust 用枚举写出三态，消灭不存在的第四组合。

限时有两条诚实规则：极大截断（静默饱和 `TMRDIFF_MAX`，不回绕）、零碎取整（不足一 tick 按一 tick，宁多等不少等）。

### 1.6 死亡的善后

两条善后路。进程退场：整槽取消（断言 `FP_EXITING`，走得干脆）。驱动死亡：标就绪而非报错——用户等的是"可读写"，驱动死后读写必错，标就绪让用户的下一次读写自己读出错。这是与 22 对偶的设计：22 的用户等的是单次回答，驱动死只能报 `EIO`；本篇的用户等的是状态，状态"可读"是最诚实的回答（读了就知道错了）。

### 1.7 与其他 OS 的多路复用对照

- **Linux** 以 `file_operations.poll`（每种文件自己的就绪回答）+ `poll_table`（收集等待）实现同构分层：`fdtypes[]` 的四分型对应 `poll` 的分文件实现，`is_deferred` 的延期对应等待队列非空，`restart_filps` 的重发对应 `poll_wait` 的重挂。
- **Redox** 的方案（scheme）以 `Scheme::poll` 回答可轮询句柄的就绪，对应字符/套接字两路投递；Redox 以方案内 Waker 唤醒对应 `select_return` 的复活。
- **seL4** 无多路复用原语，等待由客户端自持——状态服务器把等待做进服务里（本篇的选择表），无状态内核把它留给客户端。

### 1.8 小结

多路复用是六段（问群→分型→首波→挂起→次波→复活），三组开关（脾气四型/超时三态/回答两波）决定走向，一条不变量贯穿始终：每笔登记都有注销——速返注销、复活注销、取消注销、驱散注销。没有悬空的等待，是本篇的全部要求。

---

## 2 C 源码分析

### 2.1 头注释契约（`select.c:1-15`）

三入口（`do_select` 办事/`select_callback` 管道报信/`select_unsuspend_by_endpt` 驱散）→ 最小锁（只锁管道，字符回复不阻塞）→ 挂起即保活（§1.4 第三知识）。三知识是全篇的约法。

### 2.2 选择表与四分型表（`select.c:30-91`）

`MAXSELECTS 25`（`31`）+ 方向常量（`32-33`）+ `USECPERSEC`（`35`）→ `selectentry` 十四字段（`39-53`：请求人/端点/三集/三就绪集/三用户指针/filp 表/类型表/数/就绪数/错/阻塞/启始/到期/钟）→ 前置声明（`55-79`）→ `fdtypes[]` 四分发表（`85-90`：字符/套接字/文件/管道，顺序即优先级）+ `SEL_FDS`（`91`）。

### 2.3 `do_select` 主机一：槽位与超时（`select.c:96-174`）

取参（`114-115`）→ 验 `nfds`（`118`，`EINVAL`）→ 找空槽（`121-124`，满则 `ENOSPC`）→ 清槽寄单（`126-132`：`wipe` + 记请求人/端点/三用户指针）→ 拷 fd 集（`135-138`，败则空槽归还）→ 取超时并验 timeval（`141-156`，三非法 `EINVAL`）→ 算阻塞（`161-166`：无超时则等/有时非零则等/零零则 poll）→ 置到期零（`167`）→ 举启始旗（`173`，锁 filp 期间早到的回复不得提前交卷）。

### 2.4 `do_select` 主机二：验形与分型（`select.c:175-245`）

逐 fd 取兴趣（`183-184`，无兴趣跳过——继承来的 fd 可能不在集中）→ 取 filp（`193`）：取不到分两因——真坏 fd 报 `EBADF`（`194-201`，不断然返回，先记错再断后），失效关闭（`FILP_CLOSED` 的 `EIO`）留待标就绪（`261`）→ 四分型匹配（`225-232`，中即登记选择人数与 `nfds` 高水位）→ 未中报错（`234-237`，`EBADF`）→ 有错全撤（`241-245`：`cancel_all` + 空槽归还）。

### 2.5 `do_select` 主机三：首波与去留（`select.c:246-341`）

逐 fd 首波（`248-294`）：无兴趣跳过 → 失效 filp 标读写就绪（`260-263`，§1.6 的"读了就知道错"）→ 模式位快路（`264-271`：要读而无读权即标可读，要写而无写权即标可写——读写必错即就绪）→ 共享感知（`274`：`filp_select_ops` 已含所要则跳过——同 filp 上的第二个 select 不重复问）→ 加锁问型（`280-282`）→ 错则记错断后（`283-286`）→ 有果记果（`292`）→ 落启始旗（`297`）→ 去留门（`299-315`：有就绪/有错/不等 且 非延期 → 拷回/记错 + 全撤 + 交卷）→ 限时设钟（`320-336`：截断 + 取整 + `set_timer`）→ 挂起（`339`，`FP_BLOCKED_ON_SELECT`）返 `SUSPEND`（`340`）。

### 2.6 延期门与四分型谓词（`select.c:343-404`）

`is_deferred`（`346-362`）：启始中即延期；任一 filp 有 `UPDATE|BUSY` 即延期。四谓词（`368-404`）：常文件看 `S_ISREG`、管道看 `S_ISFIFO`、字符看 `S_ISCHR`、套接字看 `S_ISSOCK`——皆不阻塞、加锁与否皆可（注释明示）。

### 2.7 `select_filter` 过滤机（`select.c:406-457`）

默认无事（`420`）→ 非阻塞快路（`433-443`：旗稳（无 `UPDATE`/`BUSY`）且有 `BLOCKED` 守望 → 剪去已被守望的位；剪空即返 0；注释自承"危险的过早优化"，坏了就删）→ 置 `UPDATE`（`445`）→ 阻塞则加 `NOTIFY` 并武装三块（`446-451`）→ 正忙则挂起（`453-454`），否则交新位（`456`）。

### 2.8 字符与套接字请求机（`select.c:459-562`）

字符路（`462-522`）：`cdev_map` 重映射（`489-490`，`CTTY` 唯一例外，败则 `ENXIO`）→ 双 TTY 判死（`492-502`，同 filp 两控终端即 `EIO`）→ 先寄 dev 再问（`503`，信号中途拔终端也不乱）→ 过滤（`505-506`）→ 忙门（`509-510`，驱动同时只一问）→ 清 `UPDATE` 发问（`512-515`）→ 标忙主（`517-519`，`sel_busy` + `sel_filp` + `FSF_BUSY`）返 `SUSPEND`（`521`）。套接字路（`527-562`）同体：查表（`541-542`）→ 寄 dev（`544`）→ 过滤 → 忙门（`549-550`）→ 发问标忙（`552-560`）。

### 2.9 文件与管道请求机（`select.c:564-616`）

文件路（`567-572`）：常就绪，原样返回——最短的函数即最强的保证。管道路（`577-616`）：记原兴趣（`585`）→ 读探（`587-596`：`pipe_check` 一字节只查，成则 `RD`、败则 `ERR`）→ 写探（`598-607`：同理 `WR`/`ERR`）→ 与原兴趣取交（`610`）→ 阻塞且无果则暂存原兴趣（`612-613`，`filp_pipe_select_ops`）→ `OK`（`615`）。

### 2.10 位图双译与 fd 集拷贝（`select.c:618-708`）

`tab2ops`（`621-629`）：三集读三位。`ops2tab`（`635-654`）：三条件去重（提了才记/没记过才数/用户要了才写）+ `nreadyfds` 累加。`copy_fdsets`（`660-708`）：验界（`670-671`，到此必过，`panic` 守不可能）→ `howmany` 取整（`674`，只拷用户要的位数）→ 定源宿（`677-678`）→ 逐集拷（`680-705`，空指针集跳过）。

### 2.11 取消机（`select.c:710-778`）

`cancel_all`（`714-735`）：逐槽释 filp（`723-727`）→ 停钟清期（`729-732`）→ 空槽归还（`734`）。成功也取消——交卷即清账。`cancel_filp`（`740-778`）：三断言（`749-751`）→ 减数（`752`）→ 末人清零三数（`753-757`）→ 进行中的驱动查询标 stale（`765-776`：`sel_filp` 置空、`busy` 照旧——回信到时自知无主）。

### 2.12 复活机（`select.c:780-816,1271-1313`）

`select_return`（`783-802`）：断言非延期（`790`）→ 全撤（`792`）→ 错或拷回（`794-797`）→ 取就绪数（`799`）→ `revive`（`801`）。`restart_proc`（`1304-1313`）：与速返同门（§2.5 去留门复用）。`select_callback`（`808-816`）：管道专用，转交 `filp_status`（持 filp 锁）。`filp_status`（`1274-1299`）：全表扇出同 filp 的槽（`1288-1294`：错记错、果记果）→ 有中则复活（`1296-1298`）。

### 2.13 遗忘与超时机（`select.c:818-878`）

`init_select`（`821-827`）：逐槽 `init_timer`（`main.c:488` 调用点）。`select_forget`（`833-855`）：信号打断后的全忘——按 `fp` 找槽（`843-847`，无则返）→ 断言非启始（`851`）→ 全撤（`854`，此处不验延期：取消进行中的查询也安全）。`select_timeout_check`（`861-878`）：界内（`868`）→ 有主（`871`）→ 有期（`872`）→ 清期（`873`）→ 非延期复活（`874-875），延期转 poll（`876-877`：`block = 0`，钟响得太早当 poll 办）。

### 2.14 驱散机（`select.c:880-951`）

`select_unsuspend_by_endpt`：取驱动表（`895-896`，双空即非驱动）→ 逐槽（`900-940`）：退场进程整槽取消（`903-907`，断言 `FP_EXITING`）→ 非驱动跳过贵查（`910-911`）→ 字符死标读写就绪释账（`918-926`）→ 套接字死同理（`927-935`）→ 有中则复活（`938-939`）→ 永无回答的查询清忙（`943-950`，断言 `sel_filp == NULL`）。

### 2.15 首波回复机（`select.c:953-1106`）

`select_reply1`（`956-999`）：双断言（`962-963`）→ 清 `BUSY`（`965`）→ 三分支记账（`977-981`：无 `UPDATE` 无 `BLOCKED` 则清零/有态且就绪则减已就绪/余留）→ `BLOCK` 清零四式（`984-996`：零且被守望则静默/就绪清对应块/错清全部块）→ 广播（`998`）。`select_cdev_reply1`（`1004-1055`）：查端点（`1015`）→ 忙门（`1021-1025`，非预期回信只告警）→ stale 容忍（`1031-1043`：`sel_filp` 空则看后继，验 dev 防驱动错乱）→ 清忙（`1046-1047`）→ 记账（`1050-1051`）→ 重发（`1054`）。`select_sdev_reply1`（`1060-1106`）同体（查表 `1068`、忙门 `1072`、验 dev `1086`）。

### 2.16 次波回复机（`select.c:1108-1269`）

`select_reply2`（`1111-1162`）：全表扇出（`1121-1159`）：验类验 dev（`1128-1131`）→ 就绪分流（`1133-1146`：非 `UPDATE` 则减已就绪、清对应块、记果）→ 错分流（`1147-1150`：清 `BLOCKED`、记错）→ 有中则复活（`1157-1158`）→ 重发（`1161`）。`select_cdev_reply2`（`1167-1193`）：零态拒收（`1177-1181`）→ 查端点组 dev（`1184-1190`）→ 扇出（`1192`）。`select_sdev_reply2`（`1198-1212`）同体。`select_restart_filps`（`1217-1269`）：只看延期槽（`1233`）→ 只重发 `UPDATE` 且非 `BUSY` 的字符/套接字 filp（`1243-1260`，管道断言排除在外——同锁重入会死锁）→ 错则记错复活（`1261-1264`）→ 有果记果（`1266`）。

### 2.17 锁型与排错机（`select.c:1315-1416`）

`wipe_select`（`1318-1332`）：数清零、表清零、六集清零（`starting` 不归它——调用点举旗）。`select_lock_filp`（`1337-1351`）：默认共享读（`1344`），写/错兴趣要独占写（`1346-1348`）。`select_dump`（`1356-1416`）：排错打印（四类 filp 各显其主：常/管/字符显 dmap 忙主/套接字显 smap 忙主）。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `select.c` 的表与旗，而是吸收 Linux/Redox 的轮询模型后做取舍。以下决策对应 `.design/23-design.v1.md` D1-D7。

### D1 四分型枚举

- **C**：`fdtypes[]` 函数指针表 + 四谓词（`select.c:81-90,368-404`）。
- **Rust**：`FdKind::{Char, Sock, File, Pipe}` + `classify()` 全映射（`os/servers/vfs/src/select.rs:76,91`）。
- **为什么**：回答来源是请求点最需要的一比特；枚举使"未知类型→EBADF"可读。替代方案（函数指针表）被否决：`match` 即表，且可穷举测试。

### D2 位图代数

- **C**：`tab2ops` + `ops2tab`（去重三条件）+ `copy_fdsets` 取整（`select.c:621-708`）。
- **Rust**：`SelOps` 位旗 + `tab2ops()`/`ops2tab_apply()` + `fdset_bytes()` + `CopyDir`（`os/servers/vfs/src/select.rs:51,106,141,153,165`）。
- **为什么**：双译是"用户说的"与"内核记得的"两种语言的翻译；去重三条件各防一漏（重复计数/幻影就绪/空指针写）。替代方案（`libc fd_set`）被否决：`no_std` 无 libc，且位语义需单测锁定。

### D3 过滤状态机

- **C**：`select_filter` 三段（快路→置位→忙判，`select.c:409-457`）。
- **Rust**：`filter_step() -> FilterOutcome::{ReadyNone, Suspend, Query}` 纯函数（`os/servers/vfs/src/select.rs:176,202`）。
- **为什么**：过滤是"这次要不要打扰驱动"的知识；三出口对应剪枝/挂起/投递，调用点各走各路。替代方案（布尔返回值）被否决：三出口非二值，布尔装不下。

### D4 超时三态

- **C**：`do_timeout` + `block` 两布尔（`select.c:140-167`）+ ticks 换算截断（`327-336`）。
- **Rust**：`TimeoutPlan::{Poll, Forever, Until}` + `plan_timeout()` + `block_of()`（`os/servers/vfs/src/select.rs:248,269,297`）。
- **为什么**：两布尔四组合有一非法，枚举消灭非法态；"超时零零即 poll"一测即知。截断饱和与零碎取整收敛一处。

### D5 去留双门

- **C**：同一守卫出现三处（速返 `299-300`/复活 `1311`/超时 `874`）。
- **Rust**：`is_deferred()` + `should_return()` 三处复用（`os/servers/vfs/src/select.rs:308,317`）。
- **为什么**：三处同条件是同一知识（"现在能交卷吗"）；复用使改一处即改三处。替代方案（三处各写）被否决：C 已证明漂移风险。

### D6 回复记账

- **C**：`select_reply1` 三分支 + `BLOCK` 清零 + `select_reply2` 扇出（`select.c:956-999,1108-1162`）。
- **Rust**：`reply1_step()` + `reply2_hit()` 纯函数（`os/servers/vfs/src/select.rs:323,374,339,392`）。
- **为什么**：记账是"还欠驱动什么、欠用户什么"的知识；算与写分离（先算对再写入）可矩阵测试。`UPDATE` 置位时不减掩码——另一 select 还欠着这些位。

### D7 驱动对话 trait 化

- **C**：字符/套接字双路同体 + 管道试探 + 驱散分类（`select.c:459-616,880-951`）。
- **Rust**：`SelectDriver{query}`（`ScriptedDriver` 按脚本应答 vs `RefusingDriver` 常拒）+ `PipeProbe{probe_read/probe_write}`（`ScriptedProbe` 编程回答 vs `ClosedProbe` 固定失败）+ `DeathKind` 四值 + `unsuspend_hit()`（`os/servers/vfs/src/select.rs:490,499,543,567,576,597,648,667`）。
- **为什么**：投递是唯一的不可测点；字符/套接字只是查表不同，忙门/投递/挂起同构，一 trait 覆盖双路（测试矩阵不翻倍）。死亡标就绪是与 22 对偶的设计（§1.6）。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-1 单线程事件循环（mthread→状态机） | 挂起/等待皆 verdict，执行留 08/09 | `select.rs:694` + 本文档 D4/D5 + 23 正文 §1.4 |
| A-5 SUSPEND/revive 显式化 | `SelectVerdict::{Done, Suspend}`，复活门复用 | `select.rs:694,317` + 本文档 D5/D6 + 23 正文 §1.3 |
| A-13 select 定时器类型化 | `TimeoutPlan::Until{ticks}` 纯算，投递留时钟层 | `select.rs:248,269` + 本文档 D4 + 23 正文 §1.5 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── select.rs               — 本篇：分型/位图/过滤/超时/去留/记账/驱动判定
├── filp.rs                 — FsfFlags 复用（04，不重复定义，select.rs:36）
├── fproc.rs                — OPEN_MAX/BlockedOn::Select 复用（02/03，select.rs:41）
├── cdev.rs                 — CDEV_OP 位值对照（21，同源语义）
├── sdev.rs                 — SDEV_OP 位值对照 + 选择直转调用点（22）
└── device_map.rs           — dmap/smap 忙门对照（19，DriverBid 同构）
```

> 设计决策：§3 D1（四分型）/ D2（位图代数）/ D7（驱动 trait 化）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 槽数/取整 | `select.c:31,35` | `select.rs:38,41` | 25 槽/百万微秒 |
| 四分型 | `select.c:81-90` | `select.rs:76,91` | 表序即优先级 |
| 位图双译 | `select.c:621-654` | `select.rs:51,106,126,141` | 去重三条件 |
| 拷贝方向 | `select.c:660-708` | `select.rs:153,165` | 取整 + 方向 |
| 过滤机 | `select.c:409-457` | `select.rs:176,202` | 三出口 |
| 超时三态 | `select.c:140-167,320-336` | `select.rs:248,269,297` | 截断 + 取整 |
| 去留双门 | `select.c:299,874,1311` | `select.rs:308,317` | 三处复用 |
| 首波记账 | `select.c:956-999` | `select.rs:323,339` | 三分支 |
| 次波记账 | `select.c:1108-1162` | `select.rs:374,392` | 扇出 |
| filp 账本 | `file.h:26-32` | `select.rs:429,459` | 登记/注销配对 |
| 驱动投递 | `select.c:489-561` | `select.rs:477,490,499,543` | 忙门 + 脚本/常拒双实现 |
| 管道试探 | `select.c:577-616` | `select.rs:553,567,576,597,610,624` | 一字节探 + 暂存 |
| 驱散分类 | `select.c:884-951` | `select.rs:648,667` | 死亡标就绪 |
| 锁型 | `select.c:1337-1351` | `select.rs:676,684` | 读写二值 |
| 错误族 | `select.c` 全文件 | `select.rs:705,720 SelectError::to_errno` | 5 变体→errno，无自创 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 挂起配对（投递必有忙门，忙门必有复活） | `DriverBid` busy/owner | 发问标忙 + 回复清忙 | `select.c:509-519,552-559` |
| 记账守恒（ops 只减不增，BLOCK 回复路只清） | `reply1_step/reply2_hit` 单向 | 非 UPDATE 才减 | `select.c:977-995,1137-1144` |
| 换码单向（去程 SEL 位，回程同位） | D2 双译互逆测试 | SEL 即 CDEV_OP | `const.h:41-44` |
| 死亡就绪口径（驱动死→RD\|WR 就绪） | `unsuspend_hit` 分类 | 双路同值 | `select.c:922,930` |
| 忙不重发（BUSY 则挂起不再投） | `filter_step` BUSY 出口 | 发前先查 | `select.c:453-454` |
| 机械函数收敛（wipe/lock/dump 不入判定层） | `FilpSel::default`/`lock_kind` | 一行即全 | `select.c:1318-1351` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **280 passed / 0 failed**（既有 271 + 本篇新增 9；`minix-types` 独立）。
> 本章直接影响 9 项新增。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_classify_covers_four_kinds` | `select.c:85-90,225-237` | 四分型 + 表序 + 未知→EBADF | `select.rs:738` |
| `test_tab2ops_ops2tab_roundtrip` | `select.c:621-708` | 双译 + 去重三条件 + 取整 | `select.rs:750` |
| `test_filter_matrix` | `select.c:409-457` | 快路/忙挂/投递/非稳四形 | `select.rs:781` |
| `test_timeout_plans` | `select.c:140-167,320-336` | 三态 + 非法 + 截断 + 取整 | `select.rs:819` |
| `test_leave_gate_shared` | `select.c:299,874,1311` | 去留门五形 | `select.rs:842` |
| `test_reply1_branches` | `select.c:956-999` | 三分支 + 清零 + 广播 | `select.rs:852` |
| `test_reply2_and_restart` | `select.c:1108-1162` | 扇出 + 存错 + 失配 | `select.rs:887` |
| `test_pipe_request_and_drivers` | `select.c:577-616,489-561,740-778,884-951,1337-1351` | 试探 + 暂存 + 双驱动 + 注销 + 驱散 + 锁型 | `select.rs:912` |
| `test_errno_map_covers_select_c` | `select.c` 全文件 | 5 变体→errno 全映射 | `select.rs:966` |

测试策略：分型以四谓词全枚举锁定；位图以双译互逆 + 去重三条件锁定；过滤以快路/忙/投递/非稳四形覆盖；超时以三态 + 三非法 + 截断覆盖；去留以五形门覆盖；记账以三分支 + 扇出覆盖；驱动以脚本/常拒双实现覆盖；错误以 5 变体全映射覆盖。

### 5.1 测试统计（截至 2026-09-03）

- `cargo test -p minix-vfs --lib`：**280 passed / 0 failed**
- 本节列出与本模块直接相关的 9 个（子集）
- 完整测试清单：`rg "fn test_" os/servers/vfs/src/select.rs`

---

## 6 过渡

本篇在 19（设备表）/21（字符）/22（套接字）之后、24（套接字调用）之前，是"等一群 fd"的归属层：19 只管查到驱动，21/22 只管单次对话，本篇管"一群等待"的记账（分型/挂起/两波/超时/驱散）；没有本篇，24 的套接字 `select` 不知向谁登记，09 的主循环不知 `SUSPEND` 之后谁来复活。

```
01-vfs-init-main: init_select() (调用点)
   │
19-device-map: dmap/smap 忙门 ──┐
21-cdev: cdev_select 投递 ──────┼─► 本篇：classify 分型 → filter 过滤 → plan 超时
22-sdev: sdev_select 投递 ──────┘   → gate 去留 → reply 记账 → driver 驱散
           │                              │
           ├─► 24-socket：上层调用与续作执行（本篇只给登记层）
           └─► 09-main-loop：SUSPEND 挂起与 revive 复活的执行
```

阅读顺序提示：若关心"调用是谁发起的"，下一站 `24-socket.md`（`do_select` 之外的另一群等者）；若关心"挂起是谁执行的"，回看 `08-worker-thread.md`（等待）与 `09-main-loop.md`（分发与复活）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/select.c:1-1416`（三十函数全族）、`minix3/minix/servers/vfs/const.h:23,41-44`（`FP_BLOCKED_ON_SELECT` 与 `SEL_*`）、`minix3/minix/servers/vfs/file.h:22-48`（filp 选择字段与 `FSF_*`）、`minix3/minix/include/minix/com.h:932,949-952`（`CDEV_SELECT` 与 `CDEV_OP_*`）
- 阶段文档：`04-filp-table.md`（filp 选择字段）、`02-fproc-struct.md`（`BlockedOn::Select`）、`17-pipe.md`（管道试探执行）、`19-device-map.md`（dmap/smap 忙门）、`21-cdev.md`（字符投递）、`22-sdev.md`（套接字投递与 EIO 对偶）、`24-socket.md`（上层调用点）、`09-main-loop.md`（挂起与复活执行）
- Rust 实现：`os/servers/vfs/src/select.rs:1`（本篇判定层）、`os/servers/vfs/src/filp.rs:36`（`FsfFlags` 复用）、`os/servers/vfs/src/fproc.rs:41,106`（`OPEN_MAX` 与 `BlockedOn::Select` 复用）、`os/libs/minix-types/src/types/errno.rs:15`（errno 值）
- 内核侧：`../01-stage-kernel/18-syscall-copy.md`（fd 集拷贝语义）
