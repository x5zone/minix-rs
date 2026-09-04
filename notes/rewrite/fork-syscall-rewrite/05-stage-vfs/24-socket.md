# 24 — socket：协议域检查、失败清理与阻塞恢复的上层调用面

本文讲清套接字上层如何在"只处理调用面、不处理驱动对话"的分工下，以协议域检查、fd 槽位预检、分配 fd 并安装 vnode 三步创建套接字，以"谁分配谁释放"的失败清理表处理创建中途的失败，以 accept 恢复与 recv 恢复的三分支处理阻塞唤醒后的恢复——下层负责与驱动的数据传输对话，上层负责调用面的检查、分配与恢复；上层准备就绪，驱动对话才有可依附的 fd 与 vnode。

前置阅读：`22-sdev.md`（驱动对话层）、`23-select.md`（选择登记与恢复）、`14-filedes.md`（fd 表执行）、`19-device-map.md`（smap 协议域检查）。

> 本章不讲什么：
> - `sdev_*` 的驱动对话执行—— `22-sdev.md`（本篇只给下发驱动的调用点与顺序）
> - 选择登记与恢复执行—— `23-select.md`（本篇只给阻塞恢复的分类）
> - `check_fds/get_fd/close_fd` 的 fd 表执行—— `14-filedes.md`（本篇只给 fd 槽位预检与清理的规则）
> - `req_newnode` 的 PFS 分配执行—— `12-request-wrappers.md`（本篇只给分配规格）
> - `suspend/revive/reply` 的等待执行—— `08-worker-thread.md`/`09-main-loop.md`（本篇只给 verdict）
> - `do_socketpath` 的路径执行—— `13-path-lookup.md`（定义在 `path.c:803`，本篇只给边界）
> - 授权的内核侧实现—— `99-global-concepts.md`

---

## 1 概念

### 1.1 为什么分上下两层

套接字分调用面（用户请求：创建套接字、绑定、收发）与驱动面（驱动执行：发包、挂起、唤醒）两层。上层把十六种调用转换为驱动请求，把驱动的应答包装为用户回执。分层的原因是复用——调用面只有一套（BSD 套接字语义各系统相同），驱动面按协议族各有一个驱动。上层实现替换不影响驱动，驱动替换也不影响上层调用面。

头注释（`socket.c:1-7`）明确分工：本文件只处理上层，通用文件调用（读/写/ctl/选）直达下层，不经此处——哪些调用不归本文件处理，直接写在注释中。

### 1.2 创建套接字的三步检查

创建一个套接字要过三步检查。协议域检查：该协议域有驱动对应吗（无则 `EAFNOSUPPORT`，直接返回）。fd 槽位预检：进程还有 fd 槽位吗（`check_sock_fds`，低成本检查一次，避免创建完成后无处安装）。分配并安装：下发驱动创建套接字 + 本地分配安装（fd、filp、vnode、PFS 结点一次配齐）。三步的顺序是先检查后创建：协议域检查与 fd 槽位预检在前，分配并安装在后——创建之前的检查成本低，创建之后的每一次失败都要做失败清理。

socketpair 是创建逻辑的双份复用：检查一次协议域、fd 槽位预检两份、创建两个套接字并各分配安装一次——"一次要两个"是同一套创建逻辑走两遍，不是另一套逻辑。

### 1.3 失败清理表

创建中途失败由谁清理，是本篇最实用的一张表。单次创建失败关闭新创建的驱动侧套接字；socketpair 第一次创建失败关闭两个新创建的套接字；socketpair 第二次分配失败先关闭已分配的 fd 再关闭第二次新创建的套接字；accept 恢复中分配失败关闭新创建的套接字。规则只有一条：谁分配谁释放——VFS 分配的 fd 由 VFS 关闭，驱动创建的套接字请驱动关闭（`sdev_close`）。漏关即泄漏：驱动侧的套接字从此无人引用，却一直占用资源。

清理表把三种失败路径的清理动作写成一张表：三路清理各不相同，集中记录才不会遗漏。

### 1.4 fd 有效性与类型检查

除创建套接字外的每个调用先做 fd 有效性与类型检查：fd 是否有效（无效返回 `EBADF`），是否为套接字类型（不是返回 `ENOTSOCK`）。两种错误各有 errno。检查附带一条解锁说明：检查完即解锁，调用期间不访问 filp——进程为此次调用而阻塞，fd 不会被释放；唯一的例外是 accept 恢复，阻塞唤醒后重新检查一次（监听套接字可能已关闭）。

### 1.5 阻塞恢复的三分支

阻塞中的调用唤醒后按三分支处理。accept 恢复最复杂：有错误且无新套接字→直接返回错误，不再阻塞；监听套接字已关闭→返回 `EIO`，不关闭新套接字；有新套接字但地址复制失败→先关闭新套接字再返回错误；成功（有新套接字且无错误）→按掩码继承标志（只继承三位标志），返回新 fd。两路 recv 恢复（recvfrom/recvmsg）最简单：正数返回字节数，负数返回错误，更新 msghdr 消息头三字段，不再阻塞——"MUST NOT block"写进注释，对应类型上的约束：恢复函数只取纯输入，不取分配器，无法再次分配阻塞。

### 1.6 msghdr 消息头

`sendmsg/recvmsg` 用 msghdr 消息头组织散件：数据、控制、地址各归其位。但消息头有个硬性限制：散件最多一项（`iovlen > 1` 即 `EMSGSIZE`）——libc 负责把散件合并为一项，内核不合并。收发消息头不对称：recv 恢复带上用户消息头地址（恢复时要回拷并修改三字段），send 带零（发送完成无需回拷）。消息头是调用面最接近协议的地方：字段逐一对应，错一位即错一事。

### 1.7 与其他 OS 的套接字上层对照

- **Linux** 以 `net/socket.c` 的调用面（`__sock_create` 协议族检查、`socket_file` 分配安装、`sock_sendmsg` 分发）覆盖同构语义：`check_sock_fds` 的 fd 槽位预检对应 `sock_alloc` 前的 `get_unused_fd_flags` 检查，`compensate` 的失败清理表对应 `sock_release` 的失败 unwind，`resume_accept` 的 accept 恢复三分支对应 `accept` 的 `sock_alloc` + `fd_install` 两步。
- **Redox** 的网络方案以 `Socket` 句柄（`open` 创建 + `read`/`write` 数据传输 + 选项 `ioctl`）对应创建/数据传输两段；Redox 以方案内状态机恢复对应 `classify_accept` 的 accept 恢复分支。
- **seL4** 无套接字原语，网络栈是持有网卡能力的用户服务；VFS 的 fd 有效性与类型检查在 seL4 中由能力检查替代——无能力者无法发起调用，更不必区分 `EBADF` 与 `ENOTSOCK`。

### 1.8 小结

套接字上层处理三类操作（创建→数据传输→阻塞恢复），三组条件（协议域是否有驱动对应/fd 槽位是否充足/消息头是否合法）决定流程走向，一条不变量贯穿始终：每次下发驱动前必先检查——创建先做协议域检查与 fd 槽位预检，传输先做 fd 有效性与类型检查，恢复先做分支分类。每步都有检查与清理覆盖，是本篇的核心要求。

---

## 2 C 源码分析

### 2.1 头注释契约（`socket.c:1-32`）

上下层分工（`1-7`：本文件只处理上层，通用调用直达下层）→ 十六种调用消息头表（`9-32`：调用号/请求布局/回执布局三列，`m_lc_vfs_` 去/`m_vfs_lc_` 回，无专用回执者只用 `m_type`）。

### 2.2 `get_sock_flags` 流程（`socket.c:40-57`）

`get_sock_flags`：三位映射（`49-54`：`SOCK_CLOEXEC→O_CLOEXEC`/`NONBLOCK→O_NONBLOCK`/`NOSIGPIPE→O_NOSIGPIPE`），其余位即套接字类型本体。换算只认三位，其余位静默忽略。

### 2.3 `check_sock_fds` 流程（`socket.c:59-75`）

`check_sock_fds`：只检查 fd 槽位（`74`，`check_fds`），注释说明原因（`67-73`）：进程为调用而阻塞，槽位数量只减不增，检查一次覆盖全程；filp/vnode/PFS 结点由各自归属方管理，创建完成时再检查。

### 2.4 `make_sock_fd` 流程（`socket.c:77-170`）

`make_sock_fd` 九步：断言掩码（`94`）→ 调试查重（`104-108`，复用编号即驱动返回异常，`EIO`）→ 找 PFS（`118-119`，不存在则 panic）→ 锁读取（`120-121`，TODO 保留：锁定对象尚未确定）→ 取 vnode（`124-127`）→ 锁 OPCL（`128`）→ 取 fd（`131-135`，`R|W`）→ PFS 分配（`138-144`，`S_IFSOCK|0777`）→ 填充关联（`147-163`：vnode 九字段 + filp 三字段（`O_RDWR|flags`、`count=1`）+ CLOEXEC 位图）→ 解锁并返回 fd（`166-169`）。失败返回负错误码（`83`：套接字保持开放，清理由调用者负责）。

### 2.5 `do_socket` 流程（`socket.c:172-218`）

`do_socket`：取参数（`182-184`）→ 协议域检查（`187-188`，无对应驱动则 `EAFNOSUPPORT`）→ fd 槽位预检一份（`204-205`，长注释 `190-203` 说明先创建后安装与先安装后创建两种顺序均可，当前实现够用）→ 剥离标志（`207-208`）→ 下发驱动创建套接字（`210-212`，单个）→ 分配并安装（`214`，失败则关闭新创建的套接字 `215`）→ 返回 fd（`217`）。

### 2.6 `do_socketpair` 流程（`socket.c:220-267`）

`do_socketpair`：同一流程走两遍（协议域检查 `235-236` → fd 槽位预检两份 `242-243` → 剥离标志 `245-246` → 下发驱动创建一对套接字 `248-250`）→ 首次创建失败关闭两个新套接字（`252-255`）→ 第二次分配失败关闭已分配的 fd0 再关闭第二个新套接字（`258-261`）→ 回执 fd 对（`264-265`）→ `OK`（`266`）。

### 2.7 `get_sock` 流程（`socket.c:269-302`）

`get_sock`：取 filp（`280-281`，失败返回 `err_code`）→ 类型检查（`283-286`，非套接字返回 `ENOTSOCK`）→ 取设备号与标志（`288-290`，flags 可空）→ 解锁说明（`292-300`：阻塞期间 fd 不会释放，accept 恢复需重新检查）→ `OK`（`301`）。

### 2.8 `do_bind`/`do_connect` 流程（`socket.c:304-338`）

`do_bind`（`308-320`）/`do_connect`（`326-338`）流程相同：取 fd（`313/331`）→ fd 有效性与类型检查并取设备号与标志（`315/333`）→ 下发驱动（`318-319/336-337`，地址三项 + flags）。

### 2.9 `do_listen` 流程（`socket.c:340-359`）

`do_listen`：取 fd 与 backlog（`349-350`）→ fd 有效性与类型检查（`352-353`，不需要 flags）→ 负值钳为零（`355-356`）→ 下发驱动（`358`）。

### 2.10 `do_accept` 流程（`socket.c:361-380`）

`do_accept`：取 fd（`370`）→ fd 有效性与类型检查并取标志（`372-373`）→ fd 槽位预检一份（`375-376`，accept 恢复需分配安装）→ 下发驱动（`378-379`，记录 listen fd 供恢复时验证监听套接字）。

### 2.11 `resume_accept` 流程（`socket.c:382-477`）

`resume_accept` 三分支说明（`384-397`）→ #3 有错误且无新套接字直接返回（`411-415`，不再阻塞）→ 验证监听套接字（`430-434`：监听套接字已关闭返回 `EIO` 且不关闭新套接字；`428` 断言同线程）→ 同驱动断言（`437`）→ #2 有新套接字但有错误先关闭再返回（`444-450`，地址复制失败的处理）→ #1 按掩码继承标志（`459`，只继承三位标志）→ 分配并安装（`461`，失败则关闭新创建的套接字 `462-466`）→ 回执（`473-476`：清零 + `socklen.len` + 新 fd）。

### 2.12 `do_sendto`/`do_recvfrom` 流程（`socket.c:479-519`）

`do_sendto`（`484-498`）/`do_recvfrom`（`505-519`）流程相同：取 fd（`488/510`）→ fd 有效性与类型检查并取标志（`490/511`）→ 下发驱动读写（`493-497/514-518`：控制与地址为空 + 地址对 + 用户标志 + 读写方向 + filp 标志 + recv 恢复回执地址为零）。

### 2.13 `resume_recvfrom` 流程（`socket.c:521-537`）

`resume_recvfrom`：说明不再阻塞（`522-524`）→ 正数回执（`530-534`：`socklen.len=addr_len` + 返回字节数）→ 负数返回错误（`535-536`）。

### 2.14 `do_sockmsg` 流程（`socket.c:539-595`）

`do_sockmsg`：断言调用号（`552`）→ 取 fd 与消息头地址（`554-555`）→ fd 有效性与类型检查（`557-558`）→ 复制消息头进入内核（`560-562`）→ 单项检查（`566-574`：零则为空/一项则取值/多项则 `EMSGSIZE`，注释说明 libc 负责合并）→ 复制散件进入内核（`576-578`）→ SSIZE 检查（`580-581`）→ 取散件（`583-586`）→ 下发驱动（`589-594`：数据/控制/地址 + 用户标志 + 收发方向 + filp 标志 + recv 恢复带消息头回执地址/send 带零）。

### 2.15 `resume_recvmsg` 流程（`socket.c:597-651`）

`resume_recvmsg`：说明（`599-606`：出错则数据丢失，与其他实现处理相同）→ 出错直接返回（`614-618`）→ 回拷消息头（`630-638`，三选一论证 `623-629` 选择重新复制，失败则打印并转换错误）→ 修改三字段（`641-644`：控制长度/标志必改，地址长度有则改）→ 回拷用户缓冲（`646-648`，失败则转换错误）→ 返回字节数（`650`）。

### 2.16 `do_setsockopt`/`do_getsockopt` 流程（`socket.c:653-695`）

`do_setsockopt`（`657-670`：fd 有效性与类型检查 + 下发驱动五项参数）/`do_getsockopt`（`676-695`：fd 有效性与类型检查 + 取长度 + 下发驱动 + 成功回写 len `692-693`）。

### 2.17 `do_getsockname`/`do_getpeername` 流程（`socket.c:697-741`）

`do_getsockname`（`701-718`）/`do_getpeername`（`724-741`）流程相同：取 fd 与长度（`707-708/730-731`）→ fd 有效性与类型检查（`710/734`）→ 下发驱动（`713/736`）→ 成功回写 len（`715-716/738-739`）。

### 2.18 `do_shutdown` 流程（`socket.c:743-762`）

`do_shutdown`：取 fd 与关闭方向（`752-753`）→ fd 有效性与类型检查（`755-756`）→ 关闭方向检查（`758-759`，其余值返回 `EINVAL`）→ 下发驱动（`761`）。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `socket.c` 的检查与分配流程，而是吸收 Linux/Redox 的上层模型后做取舍。以下决策对应 `.design/24-design.v1.md` D1-D7。

### D1 调用枚举（复用调用号）

- **C**：头注释十六种调用消息头表（`socket.c:9-32`）。
- **Rust**：`SockCall` 十六值 + `reply_layout()`（三种回执布局）+ `needs_fd_precheck()`（三个需 fd 槽位预检的创建调用）+ `SocketPathRef` 占位（`os/servers/vfs/src/socket.rs:77,115,124`）。
- **为什么**：消息头布局与是否需 fd 槽位预检是调用点最需要的两项信息；调用号数值复用 `call_table.rs:VfsCallNum`。替代方案（重定义调用号）被否决：single source 在 09，重复即漂移。

### D2 标志换算

- **C**：`get_sock_flags` 三位 + `type & ~SOCK_FLAGS_MASK` 剥离（`socket.c:44-57,207`）。
- **Rust**：`sock_flags()` + `strip_sock_type()`（`os/servers/vfs/src/socket.rs:149,165`）。
- **为什么**：换算与剥离针对同一 type 字段的两种用途；未知位静默忽略可用单测直接验证。替代方案（bitflags 双向）被否决：SOCK_ 与 O_ 分属两个域，单个旗集会抹平域界。

### D3 fd 有效性与类型检查 trait 化

- **C**：`get_sock` 取 filp/类型检查/取设备号标志/解锁四步 + 解锁说明（`socket.c:276-302`）。
- **Rust**：`SockLookup{lookup}`（`TableLookup` 按表应答 vs `EmptyTable` 常拒）+ `SockTarget{dev, flags: Option}`（`os/servers/vfs/src/socket.rs:171,186,193,232`）。
- **为什么**：filp 表是唯一的不可测点，以 trait 隔离便于测试；`flags: Option` 对应 `flags != NULL`（listen 不需要 flags）。解锁说明转为文档注释（单线程借用下自然成立）。替代方案（直连 filp 表）被否决：判定层不碰活表。

### D4 分配器 trait 化

- **C**：`make_sock_fd` 九步（`socket.c:86-170`）。
- **Rust**：`FdAllocator{alloc}`（`ScriptedAlloc` 按脚本发 fd vs `FailingAlloc` 常败）+ `SockFdSpec::new` 掩码校验 + `ACCEPT_INHERIT_MASK`（`os/servers/vfs/src/socket.rs:242,278,285,345`）。
- **为什么**：九步中的执行留给 14/12/05，判定层只保留规格；断言转为 verdict 是 ARCH 加固（C 的 assert 在 release 下消失）。替代方案（九步直译）被否决：直译执行逻辑后判定层过重，且 PFS/fd 表不可单测。

### D5 失败清理表

- **C**：三路清理各不相同（`socket.c:214-215,252-261,461-466`）。
- **Rust**：`BuildStep` 四值 + `compensate()` + `CloseTarget::{DriverSock, ProcFd}` + `CloseList`（`os/servers/vfs/src/socket.rs:355,364,404,421`）。
- **为什么**：清理规则是谁分配谁释放：VFS 分配的 fd 由 VFS 关闭，驱动创建的套接字请驱动关闭；用清单表示清理几个、清理谁、按何顺序，逐项可数。替代方案（调用点各写）被否决：与 23-D5 同理，同一逻辑复制即漂移。

### D6 阻塞恢复分类

- **C**：accept 恢复三分支 + 监听套接字验证 + 两路 recv 恢复（`socket.c:399-477,526-537,608-651`）。
- **Rust**：`classify_accept()` 四分支 + `accept_inherit_flags()` + `classify_recvfrom()` + `recvmsg_update()`（`os/servers/vfs/src/socket.rs:437,458,472,478,491,501,511`）。
- **为什么**：阻塞恢复处理唤醒后的分支选择；四分支穷举使监听套接字验证失败无处遗漏；不再阻塞由类型保证（纯输入，无分配器）。替代方案（布尔三参）被否决：四分支非布尔可读。

### D7 消息头检查

- **C**：单项检查 + SSIZE 检查 + 回执规则 + 关断方向检查 + 监听 backlog 钳零（`socket.c:573-581,593-594,758-759,355-356`）。
- **Rust**：`iov_gate()` + `check_iov_len()` + `reply_msgbuf()` + `check_shutdown_how()`（复用 22 的 `SHUT_*`）+ `clamp_backlog()`（`os/servers/vfs/src/socket.rs:521,531,540,549,555,565,578`）。
- **为什么**：五项检查都是消息头与参数合法性检查；集中实现则改一处即改五处。替代方案（检查散落各 do_*）被否决：调用点的共用检查应集中管理。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-1 单线程事件循环（mthread→状态机） | fd 检查解锁说明转文档注释；阻塞恢复不再阻塞由类型保证 | `socket.rs:186,437` + 本文档 D3/D6 + 24 正文 §1.4/§1.5 |
| A-5 SUSPEND/revive 显式化 | `SockVerdict::{Done, Suspend}`，回执布局枚举 | `socket.rs:584` + 本文档 D1/D6 + 24 正文 §1.5 |
| A-11 socket 驱动模型类型化 | 上层只认 `SockCall`，驱动对话复用 22 的 `SdevOp`/`SHUT_*` | `socket.rs:77,555` + 本文档 D1/D7 + 24 正文 §1.1 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── socket.rs               — 本篇：调用/换算/检查/分配/清理/恢复/消息头判定
├── sdev.rs                 — SHUT_* 复用 + sdev_* 下发驱动对照（22，不重复定义）
├── call_table.rs           — VfsCallNum 调用号复用（09，不重定义）
├── open.rs                 — O_RDWR 语义对照（15，同源；O_ 三旗本篇定义）
├── filedes.rs              — check_fds/get_fd/close_fd 执行对照（14）
├── fproc.rs                — 进程槽对照（02/03，检查与分配的宿主）
└── device_map.rs           — smap 协议域检查对照（19，协议域检查层）
```

> 设计决策：§3 D1（调用枚举）/ D3（fd 有效性与类型检查）/ D5（失败清理表）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| SOCK 三旗/掩码 | `socket.h:113-116` | `socket.rs:37,43` | 域内换算 |
| O_ 三旗/RDWR | `fcntl.h:81,118,124,66` | `socket.rs:46,55` | 域内映射 |
| 新结点模式/SSIZE | `stat.h:162,189`/`common_limits.h:55` | `socket.rs:61,65,68` | 分配模式/散件上限 |
| 十六调用 | `socket.c:9-32` | `socket.rs:77,115,124` | 三种回执布局 + 三个需 fd 槽位预检的创建调用 |
| 换算剥离 | `socket.c:44-57,207` | `socket.rs:149,165` | 三位 + 掩码 |
| fd 有效性与类型检查 | `socket.c:276-302` | `socket.rs:171,186,193,232` | 表/空双实现 |
| 分配规格 | `socket.c:86-170` | `socket.rs:242,278,285,345` | 掩码校验 + 脚本/常败双实现 |
| 失败清理表 | `socket.c:214,252-261,461` | `socket.rs:355,364,404,421` | 四步→清单 |
| accept 恢复 | `socket.c:399-477` | `socket.rs:437,458,472` | 四分支 + 标志继承 |
| recv 恢复 | `socket.c:526-537,608-651` | `socket.rs:478,491,501,511` | 二值 + 三字段 |
| 消息头五项检查 | `socket.c:573-581,593,758,355` | `socket.rs:521,531,540,549,555,565,578` | 检查集中 |
| 错误族 | `socket.c` 全文件 | `socket.rs:595,612 SockError::to_errno` | 7 变体→errno，无自创 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 协议域检查先行（无对应驱动不创建） | 调用点检查 | fd 槽位预检之前 | `socket.c:187,235` |
| 谁分配谁释放（失败清理闭合） | `compensate` 全映射 | 四步无遗漏 | `socket.c:214,252-261,461-466` |
| 阻塞恢复不再阻塞 | 恢复函数纯输入 | 无分配器参数 | `socket.c:396,522-524,605` |
| 消息头单项（散件至多一项） | `iov_gate` | 多则 EMSGSIZE | `socket.c:573-574` |
| 标志继承（accept 只继承三位标志） | `ACCEPT_INHERIT_MASK` | 掩码范围可数 | `socket.c:459` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **288 passed / 0 failed**（既有 280 + 本篇新增 8；`minix-types` 独立）。
> 本章直接影响 8 项新增。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_calls_cover_sixteen` | `socket.c:9-32` | 十六种调用 + 三种回执布局 + 三个需 fd 槽位预检的创建调用 | `socket.rs:632` |
| `test_flag_conversion` | `socket.c:44-57,207-208` | 三位换算 + 剥离 + 未知位忽略 | `socket.rs:668` |
| `test_lookup_gate` | `socket.c:276-302` | 检查 + ENOTSOCK 分流 + 空表 | `socket.rs:685` |
| `test_landing_spec_and_allocator` | `socket.c:86-170,459` | 掩码校验 + 模式 + 脚本/常败 + 标志继承 | `socket.rs:704` |
| `test_compensation_table` | `socket.c:214-215,252-261,461-466` | 三路清理 + 空表 | `socket.rs:726` |
| `test_resume_classification` | `socket.c:399-477,526-651` | accept 恢复四分支 + recv 分流 + 消息头改写 | `socket.rs:755` |
| `test_envelope_doors` | `socket.c:573-581,593-594,758-759,355-356` | 五项检查全矩阵 | `socket.rs:795` |
| `test_errno_map_covers_socket_c` | `socket.c` 全文件 | 7 变体→errno 全映射 | `socket.rs:820` |

测试策略：调用以十六值全枚举锁定；换算以三位 + 剥离覆盖；检查以有效/类型错误/空表三分流覆盖；分配以掩码 + 脚本/常败覆盖；清理以三路 + 空表覆盖；恢复以四分支 + 二值 + 三字段覆盖；消息头以五项检查矩阵覆盖；错误以 7 变体全映射覆盖。

### 5.1 测试统计（截至 2026-09-03）

- `cargo test -p minix-vfs --lib`：**288 passed / 0 failed**
- 本节列出与本模块直接相关的 8 个（子集）
- 完整测试清单：`rg "fn test_" os/servers/vfs/src/socket.rs`

---

## 6 过渡

本篇在 22（驱动对话）之后，是上层调用的归属层：22 只管找到驱动之后如何对话，本篇管调用如何发起（创建套接字的三步检查/fd 有效性与类型检查/消息头检查）与阻塞唤醒后如何恢复（accept 恢复/recv 恢复）；没有本篇，09 的主循环不知套接字调用从何登记，23 的选择回复不知上层恢复结果向谁返回。

```
22-sdev: sdev_* 驱动对话（如何对话）
   │
   └─► 本篇：SockCall 定义调用 → lookup 检查 → alloc 分配 → compensate 清理
              │                        │                  │
              ├─► resume_*：阻塞恢复的执行（accept 恢复/recv 恢复的归属）
              ├─► 23-select：选择登记与回复的执行（SEL1/SEL2 的终点）
              └─► 09-main-loop：VFS_SOCKET* 分发与 SUSPEND 挂起
```

阅读顺序提示：若关心"分配执行（fd/vnode/PFS）"，回看 `14-filedes.md`（fd 表）与 `12-request-wrappers.md`（PFS）；若关心"进程执行关联"，下一站 `25-exec.md`（`pm_exec` 的执行）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/socket.c:1-762`（十八函数全族）、`minix3/sys/sys/socket.h:113-116,604-606`（`SOCK_*/SHUT_*`）、`minix3/sys/sys/fcntl.h:66,81,118,124`（`O_*`）、`minix3/sys/sys/stat.h:162,189`（`S_IFSOCK/ACCESSPERMS`）、`minix3/sys/sys/common_limits.h:55`（`SSIZE_MAX`）、`minix3/minix/servers/vfs/path.c:801-830`（`do_socketpath` 执行，归 13）
- 阶段文档：`22-sdev.md`（驱动对话与 SHUT 复用）、`23-select.md`（选择登记与恢复）、`19-device-map.md`（smap 协议域检查）、`14-filedes.md`（fd 表执行）、`12-request-wrappers.md`（PFS 分配）、`04-filp-table.md`（filp 检查）、`09-main-loop.md`（调用分发）
- Rust 实现：`os/servers/vfs/src/socket.rs:1`（本篇判定层）、`os/servers/vfs/src/sdev.rs:91`（`SHUT_*` 复用）、`os/servers/vfs/src/call_table.rs:78`（调用号复用）、`os/libs/minix-types/src/types/errno.rs:15`（errno 值）
- 内核侧：`../01-stage-kernel/18-syscall-copy.md`（消息头拷贝语义）
