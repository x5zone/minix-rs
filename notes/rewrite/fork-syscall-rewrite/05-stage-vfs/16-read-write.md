# 16 — read/write：三向同机、五路分派、位置推进与块串行

本文讲清读写调用如何在“方向解码→头校验→五路分派→位置推进→收尾”的五段管线中，以 `rw_flag` 的方向位、`filp_mode` 的许可位、`v_mode & S_IFMT` 的类型位为三组开关，把 `filp_pos` 的偏移变成返回的字节数，并在块设备上以全局锁串行、在管道上以定量续写、在无读者时以信号收尾。

前置阅读：`15-open-close.md`（fd 落定与 `filp_pos` 的由来）、`04-filp-table.md`（`filp_mode/count/pos` 三字段）、`14-filedes.md`（`get_filp/get_filp2` 的取锁语义）。

> 本章不讲什么：
> - `req_readwrite/req_breadwrite/req_peek` 的 FS 协议细节—— `12-request-wrappers.md`
> - `pipe_check/pipe_suspend` 的阻塞执行—— `17-pipe.md`
> - 字符/块/socket 驱动的数据面执行—— `21-cdev.md` / `20-bdev.md` / `22-sdev.md`
> - `forbidden` 的权限判定—— `29-protect.md`（读写只查 `filp_mode` 许可位，不查 uid）
> - `sys_kill` 的信号投递实现—— 内核侧（本篇只给击发条件）

---

## 1 概念

### 1.1 为什么读写偷看共用一机

读、写、偷看（peek）是同一管线的三个方向：读把字节拷给用户，写把字节从用户拿来，偷看取数但不拷贝也不推进位置。`read.c:150` 的断言 `rw_flag == READING || WRITING || PEEKING` 正是同机的自述——三个方向共享解码、校验、分派、收尾，只在数据面走向不同。

同机的代价是每个分支都要回答“偷看在此有意义吗”：管道与字符、socket 答“否”（`read.c:155,163,203` 三拒绝），块与普通文件答“是”（`219,238`）。三拒绝不是重复代码，而是同一问题在三处管辖的各自回答——偷看对流无意义（管道）、对设备流无快照可取（字符/socket）。

### 1.2 位置的独享与推进

`filp_pos` 是打开时配给的私有偏移（见 15 §1.1），读写的全部职责是“用完推进”：入口取 `position = f->filp_pos`（`read.c:145`），出口写回 `f->filp_pos = position`（`262`），中间的分派只读写局部 `position`。返回值 `cum_io`（累积字节数）与 `position` 的推进量同源——读到多少，位置走多远。

`O_APPEND` 是唯一的例外：写前把起点重定到文件末尾（`234`）。追加语义因此不是“写标志”，而是“起点重定”——位置的计算规则在分派前就被改写。

### 1.3 数据面的五路分派

与 15 的打开分派同型，读写按 vnode 类型走五条路：管道进 `rw_pipe`，字符进 `cdev_io`，socket 进 `sdev_readwrite`，块设备在 `bsf` 锁内走 `req_breadwrite`，普通文件走 `req_readwrite`。分派发生在尺寸守门之后（`152` 的 `SSIZE_MAX` 先行）：先问“要得太多吗”，再问“去哪读”。

字符分支藏着全文件最诚实的一段注释（`182-199` 的 FIXME）：挂起中的字符 I/O 乐观推进位置，附四条假设与一句“whew”。乐观推进是“串行化缺失”的代偿——同一 `filp` 上的并发读写本应排队，但排队需要挂起队列（17 的管辖），此处以“先推进、错了再说”近似。读懂这段注释，就读懂了 VFS 并发模型的边界。

### 1.4 块设备的全局串行

块特殊文件的读写是唯一持全局锁的数据面：`lock_bsf` 进、`unlock_bsf` 出（`217,230`），锁的粒度是整个块设备层。快道是 `trylock` 直取（`53-54`），慢道是挂起自己再阻塞取（`56-61`）——锁竞争不自旋，而是让出 worker（08 的管辖）。

`check_bsf_lock`（`76-87`）是卸载时的断言式检查（`mount.c:576`）：试锁必须成功，否则“锁着”与“怪状态”各 panic 一种。它的存在说明 `bsf` 锁的持有必须是短临界——卸载时锁必须是自由的，任何泄漏的持有都是 bug。

### 1.5 偷看：取数不推进的语义

`PEEKING` 的三条出路收敛为一句：取数，不拷贝用户缓冲，不推进位置。块分支调 `req_bpeek`（`220`，无 `res_pos` 回写），普通分支调 `req_peek`（`239`，无 `new_pos`），而 `filp_pos` 的回写（`262`）照常执行——写回的是未动过的值，语义上等于不推进。偷看因此不需要独立的位置逻辑，只需要“不取新位置”的调用。

### 1.6 管道的定量与续写

管道读写不经过位置（`position = 0` 且注明“实际不用”，`338`），而经过定量：`pipe_check` 给出本轮配额（`342`），读再截到缓冲存量（`358-360`），做完加减缓冲计数（`377-380`：读减写加——管道的 `v_size` 是存量不是长度）。

配额不足叫 partial：非阻塞直接返回已得数，阻塞则挂起等续（`382-390`）。“大写分多轮”是管道独有的时间结构——普通文件的 `req_readwrite` 一次给完，管道的配额制使一次 `write` 可能跨越多次挂起与唤醒。

### 1.7 与其他 OS 的读写对照

- **Linux** 以 `read_iter/write_iter` 的 `kiocb`（含 `ki_pos` 私有位置与累积返回）+ `O_APPEND` 的写前重定 + `ESPIPE` 的管道拒绝，与 `read_write` 同型；`iov_iter` 的 `cum_io` 累积语义与此处 `cum_io +=` 同序。
- **Redox** 的 `Scheme::read/write` 以 trait 对象隔离各 scheme（`scheme.rs`），VFS 的 `select_route` 纯判定正是同一隔离——数据面执行（驱动/FS）是路由的目标，不在判定层实现；`BsfLock` trait 对应 Redox 以 `Mutex` 守卫 scheme 全局状态的做法，但以显式 trait 使配对可测。
- **seL4** 无读写原语，文件 I/O 全在用户态服务中模拟；VFS 的 `filp_pos` 独享推进在 seL4 中对应客户端自持偏移，而此处由服务器持有——差异的根因是 Minix3 的状态服务器模型 vs seL4 的无状态内核。

### 1.8 小结

读写是五段管线（方向解码→头校验→五路分派→位置推进→收尾），块设备加全局串行，管道加定量续写，无读者加信号收尾。三组开关（方向位/许可位/类型位）决定走向，一条不变量贯穿始终：返回多少字节，位置就走多远——偷看是唯一的例外，它取数但不行路。

---

## 2 C 源码分析

### 2.1 三向常量与入口守门（`const.h:77-79` + `read.c:30-43` + `write.c:15-24`）

`READING 0/WRITING 1/PEEKING 2` 定义于 `minix3/minix/include/minix/const.h:77-79`。`do_read` 与 `do_write` 同型：`cum_io` 入参非零即 `EINVAL`（`read.c:38-39`、`write.c:20-21`，“该域保留内部用”，`32-37`），随后以方向位进 `do_read_write_peek`（`41-42`、`23-24`）。

### 2.2 `bsf` 锁三函数（`read.c:49-87` + `glo.h:36`）

`bsf_lock` 声明于 `minix3/minix/servers/vfs/glo.h:36`（“块特殊文件全局锁”），`main.c:448` 初始化。`lock_bsf`（`49-62`）：`trylock` 成功直返，失败则 `worker_suspend` 让出、`mutex_lock` 阻塞取、`worker_resume` 回来，阻塞取失败即 panic。`unlock_bsf`（`67-71`）：解失败即 panic。`check_bsf_lock`（`76-87`）：试锁返回 `-EBUSY` 即“锁着”panic，其他非零即“怪状态”panic，成功则立即释放试持。

### 2.3 `actual_read_write_peek` 校验机（`read.c:92-130`）

写转独占锁、余转共享锁（`101-104`），`get_filp2` 失败透传 `err_code`（`104-105`），`count>0` 断言（`107`），方向许可位缺失即 `EBADF`（`109-112`），零字节先解锁后回 0（`113-116`，“字符设备免检零”），随后进 `read_write` 再解锁返回（`118-121`）。`do_read_write_peek`（`127-130`）只是绑定 `fp` 的薄包装。

### 2.4 `read_write` 五路分派（`read.c:135-251`）

入口取位置与清零累积（`145-148`），方向断言（`150`），超 `SSIZE_MAX` 即 `EINVAL`（`152`，`LONG_MAX`，见 `minix3/sys/sys/common_limits.h:55`）。FIFO（`154-161`）：偷看拒绝，`fd != -1` 断言，`VFS_READ/WRITE` 进 `rw_pipe`。CHR（`162-201`）：偷看拒绝，`NO_DEV` panic，`CDEV_READ/WRITE` 进 `cdev_io`；同步返回（“本不该发生”）记数推进，`SUSPEND` 乐观推进整单（`200`，FIXME 假设链见 §1.3）。SOCK（`202-212`）：偷看拒绝，`NO_DEV` panic，进 `sdev_readwrite`。BLK（`213-230`）：`NO_DEV` panic，加 `bsf`，偷看走 `req_bpeek`，余走 `req_breadwrite`（成功则取回新位置与增量），解锁。REG（`231-251`）：写前 `O_APPEND` 重定（`234`），偷看走 `req_peek`，余走 `req_readwrite`（非负即取新位置与增量）。

### 2.5 收尾三件套（`read.c:253-276`）

写且（普通或目录）且超长则生长尺寸（`254-260`），位置回写 `filp`（`262`），`EPIPE` 写且无 `O_NOSIGPIPE` 则 `sys_kill(SIGPIPE)`（`264-271`，`O_NOSIGPIPE` 见 `minix3/sys/sys/fcntl.h:124`），`OK` 回累积数否则透传错误（`273-276`）。

### 2.6 `do_getdents` 目录机（`read.c:282-317`）

`cum_io` 非零拒绝（`292-293`），`get_filp(VNODE_READ)` 取锁（`300-301`，注意此处是 `get_filp` 而非 `get_filp2`——目录项读取不需要写锁分化），无读位或非目录皆 `EBADF`（`303-306`），`req_getdents` 取项（`309-310`），正数推进位置（`312`），解锁返回（`315-316`）。

### 2.7 `rw_pipe` 管道机（`read.c:323-393`）

双断言开场（`333-334`：vnode 锁我持 + filp 锁我持；`340`：方向非偷看），`oflags` 与位置清零（`336-338`）。`pipe_check` 定量（`342`）：`SUSPEND` 即 `pipe_suspend` 登记后返回（`344-345`），错误（含 partial 残量，NetBSD 对齐注释 `347-350`）直接返回。读截缓冲（`358-360`），未映射 panic（`362-363`），经映射端点 `req_readwrite`（`365-366`，位置恒 0），非 `OK` 返回（`368-371`，断言非 `SUSPEND`）。推进缓冲与剩余（`373-380`），partial 非阻塞返回计数、阻塞续挂（`382-390`，`PIPE_BUF` 原子阈见 `minix3/sys/sys/syslimits.h:66`，`__minix` 下 32768），返回累积（`392`）。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `read.c` 的直线代码，而是吸收 Linux/Redox 的读写模型后做取舍。以下决策对应 `.design/16-design.v1.md` D1-D7。

### D1 方向类型化

- **C**：`int rw_flag` 裸整数 + `assert` 三值（`read.c:150`）。
- **Rust**：`RwDir::{Read, Write, Peek}`（`TryFrom<i32>` 拒绝三值之外）+ `wants_write/lock_kind` 派生（`os/servers/vfs/src/read_write.rs:44`）。
- **为什么**：断言是运行到才知道错；枚举使第四值不可构造。替代方案（保留整数 + 注释）被否决：方向在七处分支被匹配，整数的每次匹配都要重写拒绝臂，枚举由编译器检查穷尽。

### D2 `bsf` 协议化

- **C**：`mutex_t` 全局锁 + 快慢道内嵌调用点（`read.c:49-62`）。
- **Rust**：`trait BsfLock` + `ImmediateBsf`（常快道）vs `ContendedBsf`（首试失败走慢道）双实现 + `BsfGuard` 的 Drop 配对（`os/servers/vfs/src/read_write.rs:104,203`）。
- **为什么**：单线程下 mutex 退化为持有位 + 慢道挂起；trait 使“快慢分化”与“配对义务”可测。替代方案（`Cell<bool>` 直写调用点）被否决：配对义务散落五分支，遗漏即死锁——`BsfGuard` 使遗漏不可表达。

### D3 头校验收敛

- **C**：锁分化 + 模式位 + 零短路散在 `actual_read_write_peek` 三十行中（`read.c:101-116`）。
- **Rust**：`validate_head(mode, dir, nbytes) -> HeadVerdict::{Proceed(LockKind), Zero}`（`os/servers/vfs/src/read_write.rs:258`）。
- **为什么**：三检查是同一扇门的三闩；收敛后“读端无读位但零字节”仍 `EBADF`（校验先于短路，C `109` 先于 `113` 同序）一测即知。

### D4 分派纯判定与 `NO_DEV` 加固

- **C**：五分支内嵌驱动调用 + `NO_DEV` 三 panic（`read.c:168-169,208-209,214-215`）。
- **Rust**：`select_route(ft, dir) -> IoRoute` 纯判定 + `check_dev(dev) -> Result`（`os/servers/vfs/src/read_write.rs:307,341`）；`NO_DEV → NoDev→ENXIO`。
- **为什么**：判定与执行分离（执行归 12/17/20~22）。panic→错误是 ARCH 加固：C 视 `NO_DEV` 为不可能（open 已守门），panic 即整服崩溃；`ENXIO` 在可达路径等价、不可达路径更优雅。替代方案（`panic!` 直译）被否决：见 15 D7 同例。

### D5 推进显式

- **C**：append 重定、尺寸生长、位置回写散在三处（`read.c:234,254-262`）。
- **Rust**：`apply_append/grow_size` 双纯函数 + 调用点一次回写（`os/servers/vfs/src/read_write.rs:350,360`）。
- **为什么**：三处同属“位置推进”一知识；收敛后“读不生长/目录可生长/append 读不重定”三边界各一测。

### D6 收尾表

- **C**：SIGPIPE 三元条件 + 返回二选散在收尾（`read.c:264-276`）。
- **Rust**：`should_signal_pipe(err, writing, nosigpipe)` + `finish(r, cum_io)`（`os/servers/vfs/src/read_write.rs:372,377`）。
- **为什么**：击发矩阵（错误×方向×标志）纯函数化后全覆盖四样本；`sys_kill` 执行留调用点（内核侧管辖）。

### D7 管道数学与执行分离

- **C**：定量数学与 `req_readwrite/pipe_suspend` 执行交织（`read.c:342-392`）。
- **Rust**：`pipe_chunk/pipe_apply/after_partial` 三纯函数（`os/servers/vfs/src/read_write.rs:386,395,414`）；`pipe_check/pipe_suspend` 本体归 17。
- **为什么**：数学（纯知识）与执行（17 管辖）分离；`position = 0 未用`以“不取位置参数”显式——签名即文档。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-1 单线程事件循环（mthread→状态机） | mutex→持有位 + 慢道挂起移交 08；`Suspend` 只作 verdict | `read_write.rs:104,203` + 本文档 D2 + 16 正文 §1.4 |
| A-8 64 位类型映射 | `off_t→u64` 位置、`SSIZE_MAX=LONG_MAX` 上界 | `read_write.rs:36,279` + 本文档 D5 + 16 正文 §2.4 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── read_write.rs         — 本篇：方向/校验/分派/推进/收尾判定
├── open.rs               — FileType/R_BIT/W_BIT 复用（15）
├── filedes.rs            — get_filp 族执行（14）
├── filp.rs               — filp_pos 归属（04）
└── request.rs            — FsClient/req_* 执行（12）
```

> 设计决策：§3 D1（方向类型化）/ D2（bsf 协议化）/ D4（分派纯判定）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `READING/WRITING/PEEKING` | `const.h:77-79` | `read_write.rs:24/26/28 + 44 RwDir` | 三值枚举，余值不可构造 |
| `bsf_lock` | `glo.h:36` | `read_write.rs:104 BsfLock + 203 BsfGuard` | 快慢道 + Drop 配对 |
| `check_bsf_lock` | `read.c:76` | `read_write.rs:232 check_bsf_free` | 空闲断言类型化 |
| 头校验三闩 | `read.c:101-116` | `read_write.rs:258 validate_head` | 锁分化 + EBADF + 零短路 |
| `cum_io` 守门 | `read.c:38,292` | `read_write.rs:271 check_cum_io_zero` | 非零→EINVAL |
| `SSIZE_MAX` 守门 | `read.c:152` | `read_write.rs:279 check_size` | 超界→EINVAL |
| 五路分派 | `read.c:154-251` | `read_write.rs:288,307 select_route` | 纯 verdict + PEEK 三拒绝 |
| `NO_DEV` | `read.c:168,208,214` | `read_write.rs:341 check_dev` | 加固为 ENXIO |
| append/生长 | `read.c:234,254-260` | `read_write.rs:350,360` | 双纯函数 |
| SIGPIPE/返回 | `read.c:264-276` | `read_write.rs:372,377` | 矩阵 + 二选 |
| 管道数学 | `read.c:342-390` | `read_write.rs:386,395,414` | 定量 + 加减 + 续挂判定 |
| getdents 门 | `read.c:300-312` | `read_write.rs:426,438` | 双 EBADF + 推进 |
| 错误族 | `read.c` 全文件 | `read_write.rs:450,467 IoError::to_errno` | 6 变体→errno，无自创 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 方向三值 | `RwDir::try_from` | 余值拒绝 | `read.c:150` |
| bsf 配对 | `BsfGuard` Drop | 离域即释放 | `read.c:217,230` |
| 零短路不进分派 | `HeadVerdict::Zero` | 调用点直返 0 | `read.c:113-116` |
| 偷看不推进 | 调用点不取新位置 | `req_bpeek/req_peek` 无回写 | `read.c:219-220,238-239` |
| 返回即推进量 | `finish` + 调用点回写 | `cum_io` 与位置同源 | `read.c:262,273-274` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **215 passed / 0 failed**（既有 203 + 本篇新增 12；`minix-types` 独立）。
> 本章直接影响 12 项新增。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_direction_decode_fourth_rejected` | `const.h:77-79` + `read.c:103,150` | 三值解码 + 第四值拒绝 + 锁分化 | `read_write.rs:485` |
| `test_entry_zero_guard` | `read.c:38,152,292` + `write.c:20` | 零守门 + 尺寸上界 | `read_write.rs:501` |
| `test_head_gate_matrix` | `read.c:101-116` | 锁分化 + EBADF + 零短路 + 校验序 | `read_write.rs:513` |
| `test_route_five_way_and_peek_rejections` | `read.c:154-251` | 五路 verdict + PEEK 三拒绝两放行 | `read_write.rs:535` |
| `test_bsf_fast_and_slow_paths` | `read.c:49-62` + Guard | 快慢道 + Drop 配对 + trait 多态 | `read_write.rs:567` |
| `test_bsf_check_free` | `read.c:76-87` | 空闲断言 + 持有拒绝 | `read_write.rs:598` |
| `test_position_advance_trio` | `read.c:234,254-262,273-276` | 重定 + 生长 + 返回二选 | `read_write.rs:610` |
| `test_sigpipe_matrix` | `read.c:264-271` | 击发矩阵四样本 | `read_write.rs:626` |
| `test_no_dev_hardening` | `read.c:168,208,214` | 通过 + ENXIO 加固 | `read_write.rs:636` |
| `test_pipe_math` | `read.c:342-390` | 定量截断 + 加减 + 续挂判定 | `read_write.rs:644` |
| `test_getdents_gate` | `read.c:300-312` | 双 EBADF + 推进条件 | `read_write.rs:662` |
| `test_errno_map_covers_read_c` | `read.c` 全文件 | 6 变体→errno 全映射 | `read_write.rs:675` |

测试策略：方向以三值 + 越界拒绝覆盖；头校验以“许可缺失/零短路/校验序”矩阵覆盖；分派以五路 + 偷看五样本（3 拒绝 2 放行）覆盖；bsf 以快慢道 + 配对 + 断言覆盖；推进以三边界覆盖；收尾以击发矩阵覆盖；管道以定量/加减/续挂覆盖；错误以 6 变体全映射覆盖。

### 5.1 测试统计（截至 2026-09-03）

- `cargo test -p minix-vfs --lib`：**215 passed / 0 failed**
- 本节列出与本模块直接相关的 12 个（子集）
- 完整测试清单：`rg "fn test_" os/servers/vfs/src/read_write.rs`

---

## 6 过渡

本篇在 15（fd 落定）之后、17（管道执行）之前，是“偏移”与“字节”之间的推进层：没有 15 的 `filp_pos` 落定，本篇无位置可推进；没有本篇的分派 verdict，17 的 `pipe_check` 不知配额为何物，12 的 `req_readwrite` 不知向谁发。

```
15-open-close: fp_filp[fd]=filp/count=1/vno=vp 落定
   │
   └─► 本篇：方向解码 → 头校验 → 五路分派 → 位置推进 → 收尾（SIGPIPE/cum_io）
          │                    │                    │
          ├─► 12-request-wrappers：req_readwrite/breadwrite/peek/getdents 的协议执行
          ├─► 17-pipe：pipe_check 定量与 pipe_suspend 续挂的执行
          └─► 20~22：cdev/sdev/bdev 的驱动数据面执行
```

阅读顺序提示：若关心“配额挂起是谁执行的”，下一站 `17-pipe.md`（`suspend/revive` 的完整状态机）；若关心“请求发给 FS 后如何回来”，下一站 `11-fs-comm.md`（`transid` 路由与 `do_reply`）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/read.c:1-393`（`do_read/lock_bsf/unlock_bsf/check_bsf_lock/actual_read_write_peek/do_read_write_peek/read_write/do_getdents/rw_pipe`）、`minix3/minix/servers/vfs/write.c:1-25`（`do_write`）、`minix3/minix/servers/vfs/glo.h:36`（`bsf_lock`）、`minix3/minix/include/minix/const.h:77-79`（`READING/WRITING/PEEKING`）、`minix3/sys/sys/common_limits.h:55`（`SSIZE_MAX`）、`minix3/sys/sys/fcntl.h:124`（`O_NOSIGPIPE`）、`minix3/sys/sys/syslimits.h:66`（`PIPE_BUF`）
- 阶段文档：`15-open-close.md`（fd 落定）、`04-filp-table.md`（`filp_pos` 归属）、`14-filedes.md`（取锁语义）、`12-request-wrappers.md`（FS 协议执行）、`17-pipe.md`（管道执行）、`09-main-loop.md`（`SUSPEND` 回复路由）
- Rust 实现：`os/servers/vfs/src/read_write.rs:1`（本篇判定层）、`os/servers/vfs/src/open.rs:1`（`FileType` 与许可位复用）、`os/servers/vfs/src/request.rs:1`（`FsClient` 执行层）、`os/libs/minix-types/src/types/errno.rs:15`（errno 值）
- 内核侧：`../01-stage-kernel/18-syscall-copy.md`（用户缓冲拷贝语义）
