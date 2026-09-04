# 30 — fcntl-lock：fcntl十三命令与八槽记录锁

本文讲清 fcntl 如何以一个系统调用实现十三命令的多路复用（复制→查询标志→设置标志→记录锁→文件打洞→NOSIGPIPE 哨兵→清缓存），以及固定八槽表（NR_LOCKS=8）上的劝告锁语义：读读相容、读写互斥、同进程不冲突；区域由起点终点算术确定，SETLK 冲突即返回 EAGAIN、SETLKW 冲突即挂起 SUSPEND，解锁分四分支（清除/头缩/尾缩/中间分裂），唤醒采用广播方式，close 时清锁并唤醒等待者。

前置阅读：`14-filedes.md`（`get_fd` 最低空闲分配与 `close_fd` 检查）、`04-filp-table.md`（`filp` 结构与 `filp_flags`）、`02-fproc-struct.md`（`BlockedOn::Flock` 阻塞载荷）、`09-main-loop.md`（`ReplyIntent` 与 `SUSPEND` 约定）。

> 本章不讲什么：
> - `fd` 分配的实现与 `close_fd` 的 `EBADF/EIO` 检查—— `14-filedes.md`（本篇只给 fd 下限检查与 close 时清锁）
> - `filp` 字段语义与 `filp_count` 引用计数—— `04-filp-table.md`（本篇只给标志的查询与设置）
> - 阻塞载荷的结构定义—— `02-fproc-struct.md`（本篇只给挂起时的存档内容）
> - FS 下发的执行—— `12-request-wrappers.md`（本篇只给 FS 对话 trait）

---

## 1 概念

### 1.0 引言与前置

三类参数：复制目标（旧 fd 换新 fd，目标下限是多少）、标志查询（当前标志是什么）、锁区域（哪个 vnode、哪个区域、哪个进程）。三类参数确定后，依次通过检查（fd 下限检查/标志位检查/五步类型权限检查），区域计算完成，结果即返回。本章假设读者已知 fd 与 vnode 为何物（见 14/05），只讲“命令分发与记录锁”。

### 1.1 为什么 fcntl 是多路复用调用

一个调用对应十三命令：复制两命令（DUPFD/DUPFD_CLOEXEC）、查询设置四命令（GETFD/SETFD/GETFL/SETFL）、记录锁三命令（GETLK/SETLK/SETLKW）、文件打洞一命令（FREESP）、NOSIGPIPE 哨兵两命令（GET/SETNOSIGPIPE）、清缓存一命令（FLUSH_FS_CACHE）。命令号决定执行哪一段逻辑，参数含义随命令号变化。无对应分支的命令号（GETOWN/SETOWN/CLOSEM/MAXFD）一律返回 `EINVAL`：未知命令无行为。

### 1.2 复制的 DUPFD 目标下限

复制（DUPFD）带一个目标下限：新 fd 不小于 `arg`（`arg ∈ [0, 255)`，越界即 `EINVAL`）。下限之上取最低空闲 fd（实现归 14 的 `get_fd`），旧 fd 与新 fd 共享同一 `filp`（`filp_count++`）。CLOEXEC 变体在复制即带上 CLOEXEC 标志：复制完成、exec 后自动关闭。复制不检查 vnode——vnode 归属由旧 fd 早已确定，访问模式在打开文件即定。

### 1.3 查询与设置标志的不对称读写

查询（GETFL）读三位（NONBLOCK/APPEND/ACCMODE），设置（SETFL）只写两位（NONBLOCK/APPEND）——读三位、写两位，不对称。查询读的是现状，设置改的是意向：访问模式（ACCMODE）在打开文件即定，SETFL 改不动它。cloexec 查询设置同理：查询返回一位（FD_CLOEXEC 或零），设置只能改这一位。不对称的原因是读写权限不同：能读的不一定能写。

### 1.4 劝告锁语义

记录锁是劝告（advisory），不是强制：锁只约束上锁行为，不阻止读写——进程即使不检查锁也能读写文件内容。相容矩阵有三条：读读相容（两个读锁可重叠）、读写互斥（写锁与任何锁都冲突）、同进程不冲突（自己持有的锁不拦自己的新请求）。解锁按区域重叠清除，不限进程：任何进程的 UNLCK 都可以清除重叠区域的锁——清除看的是区域是否重叠，不是看持有者是谁。锁表是固定八槽表（NR_LOCKS=8）：表满则返回 `ENOLCK`，槽位数量固定。

### 1.5 区域计算

区域由三个数确定：基准（whence：SEEK_SET/SEEK_CUR/SEEK_END）、偏移（start，可正可负）、长度（len，零表示到文件末尾 MAX_FILE_POS）。whence+start 得起点，起点+len-1 得终点；加法带溢出检查（上溢下溢都拒绝），终点先于起点则拒绝；长度为零即到文件末尾：锁到 vnode 末尾，不问 vnode 当前长度——文件长度可变，锁的意图不变。

### 1.6 等与不等：SETLK 与 SETLKW

遇到冲突有两种处理：SETLK 冲突即返回 `EAGAIN`（“现在拿不到，稍后再试”），SETLKW 冲突即挂起 SUSPEND（等到能拿再唤醒）。挂起时保存三项（fd、F_SETLKW 命令、flock 用户地址），唤醒由解锁时唤醒等待者完成。查询（GETLK）不挂起：查询只问“谁拦我”，有冲突则回填拦路者的信息（类型/起点/长度/持有者），无冲突则回填“无人”（F_UNLCK）。

### 1.7 解锁四分支与广播唤醒

解锁按重叠情况分四分支：全覆盖清除（整条锁注销）、头缩（起点后移）、尾缩（终点前移）、中间分裂（一锁裂为二，需要空槽，无空槽则返回 `ENOLCK`）。清除后唤醒：唤醒采用广播唤醒——遍历全表唤醒所有 FLOCK 等待者，误唤醒者经 `unblock` 重判（`main.c:946-954` 重建消息再判，仍冲突则重挂），这是有意的权衡：记录锁本就少用，精确唤醒省不下多少代码。close 时清锁：同进程同 vnode 的锁随 close 清除，有清除则唤醒等待者——锁的终点不在解锁，在 close。

### 1.8 与其他 OS 的锁对照

- **Linux** 以 `fcntl_setlk` 实现同构检查：冲突表（`file_lock` 链表）逐项比较区域、读读相容（`locks_conflict` 的 `F_RDLCK` 互让）、`F_SETLK` 遇阻返回 `EAGAIN`、`F_SETLKW` 挂队（`locks_insert_block`）、解锁分裂（`locks_delete_block` 的头/尾/中三剪对应 1.7 四分支缺一：Linux 无固定槽位，故中间分裂不愁无槽）；close 时清锁对应 `filp_close` 的 `locks_remove_posix`。
- **Redox** 的 fd 标志以 `FD_CLOEXEC` 位集对应查询设置的不对称读写；Redox 劝告锁在方案读写句柄之后，固定八槽表在 Redox 中由分配器替代——表满返回 `ENOLCK` 在 Redox 即分配失败。
- **seL4** 无劝告锁：准入即能力有无——劝告锁语义在 seL4 中由能力推导替代：有能力者恒行，无需上锁。

### 1.9 小结

fcntl 的多路复用是十三命令（复制二/查询设置四/记录锁三/打洞一/哨兵二/清缓存一），检查有三类（fd 下限检查/标志位检查/五步类型权限检查），区域有三个数（基准/偏移/长度），相容矩阵有三条（读读相容/读写互斥/同进程不冲突），冲突有两种处理（SETLK 返回 EAGAIN/SETLKW 挂起），解锁有四分支（清除/头缩/尾缩/分裂），唤醒有一种方式（广播唤醒），终点有一处（close 时清锁）。一条不变量贯穿始终：每步操作都有归属——分配则入表、查询则回填、等待则挂起、解除则唤醒。每步都有归属覆盖，是本篇的核心要求。

---

## 2 C 源码分析

### 2.1 入口与读写锁选择（`misc.c:117-134`）

`do_fcntl`：取 fd、命令号与参数（`125-128`：fd/cmd/arg_int/arg_ptr）→ 读写锁选择（`131`：唯 FREESP 取 `VNODE_WRITE`，其余取 `VNODE_READ`）→ 查 fd（`132`：`get_filp` 不中则 `err_code` 直通）→ 十三命令分流（`135-267`）→ 解 vnode 锁（`269`）→ 返回。

### 2.2 复制（`misc.c:136-148`）

`F_DUPFD/DUPFD_CLOEXEC`：fd 下限检查（`139`：`arg<0 或 ≥OPEN_MAX` 即 `EINVAL`）→ 最低空闲 fd（`140`：`get_fd(fp, arg, ...)`，实现归 14）→ 引用计数共享（`141-142`：`filp_count++` + 新槽位指向旧 vnode）→ 断言无残留位（`143`）→ 复制即带 CLOEXEC（`144-145`）→ 返回新 fd（`146`，返回值即 fd 号）。

### 2.3 标志查询与设置（`misc.c:150-175`）

`GETFD`（`150-155`：置位返回 `FD_CLOEXEC`，否则返回零）→ `SETFD`（`157-163`：按 `FD_CLOEXEC` 位置位或清位）→ `GETFL`（`165-169`：`filp_flags` 交三位掩模）→ `SETFL`（`171-175`：唯两位可写，其余位保留）。

### 2.4 记录锁转交（`misc.c:177-182`）

`GETLK/SETLK/SETLKW` 三命令皆转 `lock_op(fd, req, addr)`（`181`）：转交不判定，判定归五步类型权限检查（§2.7）。

### 2.5 文件打洞（`misc.c:184-237`）

`FREESP`：仅常规文件（`191`：非常规 vnode 即 `EINVAL`）→ 写权限检查（`192`：无 `W_BIT` 即 `EBADF`）→ 拷贝参数（`195-196`）→ 确定基准（`205-210`：SEEK_SET/SEEK_CUR/SEEK_END 三基准）→ 双向溢出检查（`214-215`）→ 负起点拒绝（`218`）→ 非零长度三钳制（`222-225`：起点越过文件尾即拒绝、回绕即拒绝、超过文件尾则钳制）→ 下发 FS（`231`：`req_ftrunc(fs, ino, start, end)`）→ 零长度截尾（`233-234`：`v_size = start`）。

### 2.6 哨兵与清缓存（`misc.c:238-264`）

`GETNOSIGPIPE`（`239`：有标志返回一）→ `SETNOSIGPIPE`（`241-246`：非零置位、零清位）→ `FLUSH_FS_CACHE`（`251` 非 root 即 `EPERM` → `253` 块设备走 `req_flush(v_bfs_e, v_sdev)` → `256` 常规目录走 `req_flush(v_fs_e, v_dev)` → `260-261` 其余 `ENODEV`（C 自注"Meaning unclear"））→ 未知命令（`265-266`：其余命令号皆 `EINVAL`）。

### 2.7 五步类型权限检查（`lock.c:36-49`）

`lock_op`：断言三种命令（`31`）→ 断言有 vnode（`34`）→ 拷贝用户 flock（`37-39`：拷贝失败即 `EINVAL`）→ 一问识别类型（`44`：唯读锁/写锁/解锁）→ 二问查询配解锁拒绝（`45`：查询配解锁即拒绝）→ 三问常规块设备检查（`46-47`：非常规块设备皆拒绝）→ 四问读权限检查（`48`：读锁需 `R_BIT`，否则 `EBADF`）→ 五问写权限检查（`49`：写锁需 `W_BIT`，否则 `EBADF`）。

### 2.8 区域计算（`lock.c:51-67`）

确定基准（`52-57`：SEEK_SET/SEEK_CUR（`filp_pos`）/SEEK_END（`v_size`）三基准，其余基准即拒绝）→ 双向溢出检查（`60-63`：加后回绕比较）→ 起点（`64`）→ 终点（`65`：`first + len - 1`）→ 长度零即到文件末尾（`66`：`last = MAX_FILE_POS`）→ 终点先于起点则拒绝（`67`）。

### 2.9 冲突扫描与三种处理（`lock.c:69-132`）

记录首个空槽（`70-75`：空槽即记录首个空槽）→ 四项跳过（`76` 不同 vnode 跳过 → `77-78` 区域不重叠跳过 → `79` 读读相容跳过 → `80` 同进程豁免跳过，唯解锁不清跳过）→ 命中则三种处理（`83-99`：查询则跳出上报 → 设置读锁/写锁遇阻，SETLK 即返回 `EAGAIN`、SETLKW 则保存三项挂起 `SUSPEND`）→ 解锁四分支（`101-132`：全覆盖清除计数减 → 头缩起点后移 → 尾缩终点前移 → 中间分裂需空槽、无空槽则 `ENOLCK`、分裂后计数加）。

### 2.10 查询回填与锁入库（`lock.c:133-165`）

有解锁则唤醒（`133`：`lock_revive`，方式见 §2.11）→ 查询命中回填锁信息（`136-143`：类型/零基准/起点/长度/持有者五项回填）→ 查询未命中回填解锁态（`144-147`：类型置 `F_UNLCK`）→ 拷贝回复给用户（`150-152`）→ 纯解锁无命中即返回（`155`）→ 无空槽拒绝（`158`：`ENOLCK`）→ 存五项入库（`159-164`：类型/持有者/vnode/起点/终点 + 计数加）→ 返回（`165`）。

### 2.11 广播唤醒（`lock.c:172-192`）

`lock_revive`：注释在前（`175-182`：广播换代码量，记录锁本就少用）→ 遍历全表（`186-191`：存活且 `FP_BLOCKED_ON_FLOCK` 者逐个 `revive`）。

### 2.12 close 时清锁（`open.c:690-724`）

`close_fd`：定位（`699`）→ 关闭 vnode（`705`）→ 清 cloexec（`707-711`）→ 有锁则扫描（`713`：`nr_locks>0` 方扫描）→ 同进程同 vnode 全部清除（`715-721`：类型清零 + 计数减）→ 有清除则唤醒（`722-723`：`lock_revive`）。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `misc.c` 的开关与 `lock.c` 的指针表，而是吸收 Linux/Redox 的锁模型后做取舍。以下决策对应 `.design/30-design.v1.md` D1-D7。

### D1 调用枚举

- **C**：十三命令开关 + 无对应分支的命令号（`misc.c:135-267`，`F_GETOWN/F_SETOWN/F_CLOSEM/F_MAXFD` 无分支）。
- **Rust**：`FcntlCmd` 十三值 + `from_raw()`（`os/servers/vfs/src/fcntl.rs:106,137`）+ `wants_write_lock()`（唯 FreeSp，`157`）+ `is_lock_cmd()`（记录锁三命令，`162`）。
- **为什么**：命令号决定执行分支是多路复用的常识；无对应分支的命令号译 `None`（未知无行为，变体即死代码）。替代方案（十三值 + Unknown 变体）被否决：未知无后继行为，变体不可达。

### D2 复制与标志检查

- **C**：fd 下限检查（`139`）+ cloexec 查询设置（`150-163`）+ 不对称读写（`165-175`）+ 哨兵双向（`238-246`）。
- **Rust**：`dupfd_arg_check()`（`fcntl.rs:170`，`[0, OPEN_MAX)` 复用 02）+ `cloexec_get/apply()`（`178,183`）+ `status_get/set()`（`189,195`，写掩模两位）+ `nosigpipe_get/set()`（`200,205`）。
- **为什么**：目标下限是复制的前置条件（`new_fd ≥ arg` 的前提）；不对称读写以掩模常量保证，散写即漏掩。替代方案（调用点散写掩模）被否决：掩模散则漏掩。

### D3 锁类型路标

- **C**：五步类型权限检查（`lock.c:44-49`）。
- **Rust**：`LockType::{Read, Write, Unlock}` + `Whence::{Set, Cur, End}`（`fcntl.rs:215,238`）+ `lock_gate()`（274，五步依次）+ `LockOp::{Query, Set, Unlock}` + `from_req()`（357，查询配解锁不可构）。
- **为什么**：五步检查是记录锁的前置条件；`R_BIT/W_BIT` 复用 29（同源常量两处即漂移）；`FileType` 复用 15（六向开关不另判）；查询配解锁不可构（`from_req` 返回 `None`）使 `45` 之拒成类型事实。替代方案（本模块另定义 R_BIT）被否决：同源两处即漂移风险。

### D4 区域计算

- **C**：三基准 + 双向溢出检查 + 长度零即到文件末尾 + 终点先于起点则拒绝（`lock.c:51-67`）。
- **Rust**：`LockRegion{first, last}` + `compute_region()`（`fcntl.rs:298,310`，`checked_add` 双向检查）。
- **为什么**：C 的加后回绕比较意在防溢出不在试探——译意不译形。替代方案（wrapping 加后比较）被否决：wrapping 即 C 写法，与 Rust 语义相悖。

### D5 固定八槽锁表

- **C**：`file_lock[8]`（类型/持有者/vnode/起点/终点，`lock.h:7-13`）+ `nr_locks`（`glo.h:15`）+ 冲突三种处理 + 解锁四分支 + 查询回填与入库（`lock.c:69-165`）。
- **Rust**：`VnodeKey{fs, ino}`（`fcntl.rs:330`，指针判等译值判等）+ `FileLock`（`342`，空槽即 `None`）+ `LockTable{slots, nr}`（`449`）+ `lock_op_decision()`（`478`）+ `LockOutcome::{Granted, QueryHit, QueryMiss, Wait, Unlocked}`（`430`）+ `LockAnswer`（`406`）+ `FlockWait{fd}`（`423`，命令恒为 `F_SETLKW` 不重复存、参数随调用者恢复记录）。
- **为什么**：空槽 `None` 化是“有无即类型”；同进程不冲突是劝告锁语义的核心；五种结果各有归属（入库/回填/挂起/清除/授权），枚举使归属不散。替代方案（布尔三元组）被否决：归属散落调用点。

### D6 close 清锁与广播唤醒

- **C**：`close_fd:713-724`（清除同进程同 vnode 的锁 + 有清除则唤醒）+ `lock_revive:172-192`（遍历全表，注释说明广播换代码量）。
- **Rust**：`release_for()`（`fcntl.rs:625`，有清除返回真、唤醒归调用者）+ `SuspendedProc`（649，存活/锁等待两态）+ `revive_all()`（666，过滤锁等待者全返回）。
- **为什么**：锁的终点在 close（无显式“关锁”调用）；广播唤醒是设计约定（误唤醒者经 `unblock` 重判，`main.c:946-954`；`pipe.c:531` 之 `break` 无撤销即证无害）。替代方案（精确唤醒）被否决：与 C 注释约定相悖（P0 级偏移）。

### D7 FS 对话与返回判定

- **C**：打洞钳制（`184-237`）+ 清缓存分流（`247-264`）+ `req_ftrunc/req_flush` 下发。
- **Rust**：`FreespSpan::{TruncateTo, TruncateSize}` + `freesp_span()`（`fcntl.rs:676,693`，钳制上界 `min(end, v_size)`）+ `FlushTarget::{BlockDev, HostingFs}` + `flush_target()`（`716,728`，其余 `ENODEV` 原样保留）+ `FcntlFs{ftrunc, flush}`（743，`ScriptedFcntl` 按脚本应答 vs `RefusingFcntl` 常拒）+ `FcntlVerdict::{Done, Suspend}`（813，唯等待挂起，`From<LockOutcome>`）。
- **为什么**：打洞不超文件尾（钳制上界）与清缓存按目标分流（块设备清对端、常规目录清宿主）是业务规则；截断刷盘皆 FS 职责，本地算即 P0 偏移。替代方案（本地算截断）被否决：与 C 相悖。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-1 单线程事件循环（mthread→状态机） | 锁配对转借用注记；锁表无锁 | `fcntl.rs:1` 模块注记 + 本文档 D5/D6 + 30 正文 §1.7 |
| A-5 SUSPEND/revive 显式化 | 等待即 `Suspend`，其余皆 `Done` | `fcntl.rs:813` + 本文档 D5/D7 + 30 正文 §1.6 |
| A-8 64 位类型映射 | 起点终点 `i64`、`MAX_FILE_POS` 即文件末尾 | `fcntl.rs:298` + 本文档 D4 + 30 正文 §1.5 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── fcntl.rs              — 本篇：命令/复制与标志/锁类型/区域/锁表/close 清锁/FS 对话
├── fproc.rs              — 目标下限常量（02，OPEN_MAX）与阻塞载荷（FlockBlock）
├── filedes.rs            — 最低空闲实现（14，get_fd 对端）
├── open.rs               — vnode 类型开关（15，FileType 对端）
├── protect.rs            — 读写位（29，R_BIT/W_BIT 对端）
└── minix-types           — Endpoint/errno 值（types，域类型层）
```

> 设计决策：§3 D1（调用枚举）/ D3（锁类型路标）/ D5（固定八槽锁表）/ D7（FS 对话）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 十三命令 | `fcntl.h:178-196,328-329` | `fcntl.rs:31,106` | 未知命令返回 `None` |
| 锁类型/基准 | `fcntl.h:203-205`/`unistd.h:174-176` | `fcntl.rs:215,238` | 识别类型、拒绝非法值 |
| 复制与标志检查 | `misc.c:139,150-175,238-246` | `fcntl.rs:170,178,183,189,195,200,205` | 目标下限/查询设置/不对称读写/哨兵 |
| 五步类型权限检查 | `lock.c:44-49` | `fcntl.rs:274` + 357 | 检查与构造 |
| 区域计算 | `lock.c:51-67` | `fcntl.rs:298,310` | 溢出拒绝/长度零到文件末尾 |
| 固定八槽锁表 | `lock.h:7-13` + `lock.c:69-165` | `fcntl.rs:330,342,430,449,478` | 入库/回填/挂起/清除 |
| close 清锁与广播唤醒 | `open.c:713-724` + `lock.c:172-192` | `fcntl.rs:625,649,666` | 清除与唤醒 |
| 打洞与清缓存 | `misc.c:184-264` | `fcntl.rs:676,693,716,728,743,752,794` | 钳制/分流/双实现 |
| 返回判定 | `lock.c:97` + A-5 | `fcntl.rs:813` | 唯等待挂起 |
| 错误族 | `misc.c`/`lock.c` 全文件 | `fcntl.rs:836,855 FcntlError::to_errno` | 7 变体→errno，无自创 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| DUPFD 目标下限（arg ∈ [0,255)） | `dupfd_arg_check` | 越界拒绝 | `misc.c:139` |
| SETFL 只写两位（其余位保留） | `status_set` 掩模 | 其余位保留 | `misc.c:173-174` |
| 同进程不冲突（设置查询跳过同进程） | 冲突扫描 `same pid → continue` | 解锁不跳过 | `lock.c:80` |
| 读读相容 | `RDLCK vs RDLCK → continue` | 写到则冲突 | `lock.c:79` |
| 溢出拒绝（双向） | `checked_add` | 回绕即拒绝 | `lock.c:60-63` |
| 表满拒绝（入库分裂皆 ENOLCK） | `empty == NULL` + `nr == 8` | 无槽即拒绝 | `lock.c:121,158` |
| close 时清锁（清除同进程同 vnode） | `release_for` | 有清除则唤醒 | `open.c:713-724` |
| 广播唤醒（遍历锁等待者） | `revive_all` | 误唤醒重判 | `lock.c:186-191` |
| 打洞不超文件尾（钳制上界） | `freesp_span` | 超尾则钳制 | `misc.c:225` |
| 清缓存按目标分流 | `flush_target` | 其余 `ENODEV` | `misc.c:253-261` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **326 passed / 0 failed**（既有 318 + 本篇新增 8；`minix-types` 独立）。
> 本章直接影响 8 项新增。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_cmd_decode_and_dupfd_door` | `misc.c:131,135-148,265-266` | 十三命令 + 未知命令返回 EINVAL + 读写锁选择 + 目标下限 | `fcntl.rs:892` |
| `test_narrow_flag_doors` | `misc.c:150-175,238-246` | cloexec 查询设置/状态不对称读写/哨兵双向 | `fcntl.rs:936` |
| `test_lock_door_five_questions` | `lock.c:44-57` + `fcntl.h` | 类型识别/基准/五步检查/锁构造 | `fcntl.rs:967` |
| `test_region_arithmetic` | `lock.c:59-67` | 起点终点/长度零到文件末尾/双向溢出/终点先于起点 | `fcntl.rs:1045` |
| `test_conflict_matrix_and_unlock_shapes` | `lock.c:69-165` | 跳过矩阵/三种处理/查询命中与未命中/四分支/表满/返回判定 | `fcntl.rs:1080` |
| `test_close_release_and_revive` | `open.c:713-724` + `lock.c:172-192` | close 时清锁/广播过滤 | `fcntl.rs:1267` |
| `test_freesp_and_flush_dialogue` | `misc.c:184-264` | 钳制/截尾/分流/脚本与常拒双实现 | `fcntl.rs:1310` |
| `test_errno_map_covers_fcntl_c` | `misc.c`/`lock.c` 全文件 | 7 变体→errno + 行号值 | `fcntl.rs:1389` |

测试策略：命令以十三值全枚举锁定（含 5/6/10/11 无分支拒绝）；复制以目标下限与不对称读写掩模覆盖；记录锁以五步检查逐项覆盖；区域以双向溢出与长度零到文件末尾覆盖；冲突以跳过矩阵（不同 vnode/不重叠/读读相容/同进程）覆盖；命中以拒绝/等待/分支覆盖；解锁以四分支（全覆盖清除/头缩/尾缩/中间分裂 + 无槽分裂拒绝）覆盖；查询以命中（表序首锁）与未命中（自锁不报）覆盖；close 以同进程同 vnode 精确释放覆盖；唤醒以存活锁等待过滤覆盖；打洞以钳制上界与零长度截尾覆盖；清缓存以 root 检查与块设备常规目录分流覆盖；错误以 7 变体全映射覆盖。

### 5.1 测试统计（截至 2026-09-03）

- `cargo test -p minix-vfs --lib`：**326 passed / 0 failed**
- 本节列出与本模块直接相关的 8 个（子集）
- 完整测试清单：`rg "fn test_" os/servers/vfs/src/fcntl.rs`

---

## 6 过渡

本篇在 29（权限）之后、31（杂项）之前，是记录锁的归属层：29 只管操作 vnode 之前检查权限，本篇管操作过程中协调记录锁；没有本篇，14 的 close 不知锁要清除，09 的主循环不知冲突要挂起。

```
29-protect: forbidden_decision 确定档位（操作 vnode 之前检查权限）
   │
14-filedes: close_fd 关闭 fd ────────────┐
                                     ├─► 本篇：lock_op_decision 冲突判定 → FcntlVerdict::Suspend 挂起
02-fproc-struct: BlockedOn::Flock ────┘   → release_for 清除记录锁 → revive_all 广播唤醒
           │                              │
           ├─► 09-main-loop：ReplyLater 待复（挂起去向的下一站）
           └─► 31-misc-queries：sync/fsync/getsysinfo（其余控制命令的下一站）
```

阅读顺序提示：若关心“挂起去向的下文”，下一站 `09-main-loop.md`（`ReplyLater` 待复）；若关心“其余控制命令”，再下一站 `31-misc-queries.md`（杂项查询）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/misc.c:117-271`（`do_fcntl` 十三命令）、`minix3/minix/servers/vfs/lock.c:1-192`（`lock_op` 检查区域冲突清除 + `lock_revive` 广播唤醒）、`minix3/minix/servers/vfs/lock.h:1-15`（固定八槽表）、`minix3/minix/servers/vfs/open.c:690-724`（`close_fd` 的 close 时清锁）、`minix3/minix/servers/vfs/main.c:946-954`（`unblock` 锁等待重建）、`minix3/minix/servers/vfs/pipe.c:498-561`（`unpause` 锁等待无撤销）、`minix3/minix/servers/vfs/const.h:6,21`（`NR_LOCKS/FP_BLOCKED_ON_FLOCK`）、`minix3/sys/sys/fcntl.h:178-205,328-329`（命令/锁类型/打洞清缓存号）
- 阶段文档：`14-filedes.md`（最低空闲分配与 close 检查）、`04-filp-table.md`（filp 结构）、`02-fproc-struct.md`（阻塞载荷）、`09-main-loop.md`（待复去向）、`29-protect.md`（上一站）、`31-misc-queries.md`（下一站）
- Rust 实现：`os/servers/vfs/src/fcntl.rs:1`（本篇判定层）、`os/servers/vfs/src/fproc.rs:160`（`FlockCmd::SetLkw` 恒定命令）、`os/libs/minix-types/src/types/errno.rs:15`（errno 值）
- 内核侧：`../01-stage-kernel/18-syscall-copy.md`（用户缓冲拷贝语义）
