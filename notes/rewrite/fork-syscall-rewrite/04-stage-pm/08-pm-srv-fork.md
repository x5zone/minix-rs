# 08 — RS 专用的 `do_srv_fork`：带身份注入的特权 fork

本文讲清 `do_srv_fork` 如何在 `do_fork` 的 9 步骨架上，以 5 处差异（权限门 `RS→EPERM` / 标志保留 `PRIV_PROC` / 凭证六字段注入 / VFS 载荷真实 `REUID/REGID` / 立即双回复而非 `SUSPEND`）孵化系统服务——子进程为 `PRIV_PROC` 系统进程，凭证由 RS 在 `srv_fork` 消息中一次注入，`VFS_CALL` 仍挂子进程但回复路径经 `VFS_PM_SRV_FORK_REPLY` 空分支立即双回复。

前置阅读：07-pm-fork.md（`do_fork` 9 步全链路与两阶段窗口）、03-mproc-table.md（`can_alloc`/`find_free_slot`/`get_free_pid`）、04-ipc-dispatch.md（`Reply(pid)` vs `ReplyLater` 契约）、05-vfs-interaction.md（`tell_vfs` 三段式与 `handle_vfs_reply` 的 `SRV_FORK` 空分支）。

---

## 1 概念

### 1.0 目标读者与边界

**目标读者**：已理解 `do_fork` 的两阶段与 `VFS_CALL` 延续（07），知道 `ProcTable` 三层身份与 `RemainingFlags` 标志过滤的开发者。

> **本章不讲什么**：
> - 普通 `fork` 的 `PRIV_PROC→User(SCHED)` 接管细节（07-pm-fork.md §2.4）
> - VM 侧 `vm_fork` 的 `sys_fork` 代数递增（02-stage-vm/18-vm-fork.md）
> - VFS 侧 `VFS_PM_SRV_FORK` 的 `fproc` 复制与 `REUID/REGID` 消费（05-stage-vfs）
> - `sched_start_user` 的 `SCHED` 内部（16-scheduling.md）
> - `sig_proc(SIGSTOP)` 的投递（11-signal-core.md）
>
> 本章只回答一个问题：**为什么 RS 需要一个与普通 `fork` 共享 70% 流程却回复语义相反的 `srv_fork`，以及 `PRIV_PROC` 与 `uid/gid` 的注入如何保证系统服务孵化的原子性**。

### 1.1 为什么需要 `srv_fork`：系统服务的孵化器

普通 `fork` 子为 *user* 进程：`PRIV_PROC` 丢弃（`forkexit.c:106` `IN_USE|DELAY_CALL|TAINTED` 未含 `PRIV_PROC`），`scheduler` 接管为 `SCHED_PROC_NR 4`（`forkexit.c:101-103` `RS` 不能调度非系统进程，PM 接管），凭证继承父（`forkexit.c:150-152` `User(creds.clone())`）——这是"用户进程家族"的创建。

系统服务（`RS` 孵化的 `DS`/`VFS`/`VM` 等）需要 *system* 进程：`PRIV_PROC` 保留（`forkexit.c:199-200` `IN_USE|PRIV_PROC|DELAY_CALL` 含 `PRIV_PROC`），`scheduler==NONE`（启动期 `NONE` 直通，不经 `SCHED`），凭证由 RS 在 `srv_fork` 消息中显式注入 `uid/gid` 六字段（`forkexit.c:206-211` `real/eff/saved` 同值）——这是"系统服务家族"的创建。若用普通 `fork` 孵化系统服务，需先 `fork` 为 `user` 再 `setuid` 为 `root`，两步间子进程以旧凭证短暂可观测，RS 集中孵化的 `srv_fork` 将"创建+确权"原子化。

### 1.2 为什么仅 RS 可调用：能力边界

`RS_PROC_NR`（`com.h:62` / `minix/config.h`，`Endpoint::RS`）是 PM 启动链中唯一被信任的系统服务孵化器（`main.c:202-215` 启动填充中 `RS_PROC_NR` 父为 `INIT`，其余系统服务父为 `RS`，形成孵化树）。`do_srv_fork` 首步即权限门（`forkexit.c:159-160`）：

```c
if (mp->mp_endpoint != RS_PROC_NR) return EPERM; // sys/errno.h:1
```

`EPERM`（`Operation not permitted`）而非 `EACCES`，因 `srv_fork` 是能力（capability）而非文件权限——任意进程若可 `srv_fork` 并指定 `uid/gid`，即可伪造 `PRIV_PROC` 系统服务（如伪 `VFS`），微内核的服务隔离即崩。RS 集中孵化的单门与 03-stage-rs 的 `RS` 孵化器模型（`RS` 的 `service_create` / `publish`）同构。

### 1.3 为什么回复语义相反：`SUSPEND` vs `Reply(pid)`+`reply(child,OK)`

`do_fork` 子为用户进程，需等待 `VFS_PM_FORK_REPLY` 的 `sched_start_user` 双分支（`main.c:369-396` / 05 `ipc/vfs.rs:269` 的 `SCHED` 异步调度，`scheduler != NONE && != KERNEL` 时 `sched_start_user`，成败→`exit_proc` 或双 `reply`），因此 `do_fork` `return SUSPEND`（`forkexit.c:139` → `ReplyLater`，05 的 `VFS_PM_FORK_REPLY` 异步双回复）。

`srv_fork` 子为系统服务，`scheduler==NONE` 直通（`forkexit.c:200` 保留 `PRIV_PROC` 后 `scheduler` 仍 `NONE`），`VFS_PM_SRV_FORK_REPLY 0x989` 在 `main.c:398-401` 为空分支 `/* Nothing to do */`（`ipc/vfs.rs:298` `SrvFork → {}`），VFS 侧 `fproc` 复制后无需 `SCHED` 调度即可立即可运行（`sys_clear` 直毁路径在 09）。因此 `do_srv_fork` 可立即 `reply(child, OK)` 并 `return pid` 同步返父（`forkexit.c:237/239` → `Reply(pid)` + `send(child,OK)`），不经过 05 的异步双分支。差异表见 §1.5。

### 1.4 凭证注入：`m_lsys_pm_srv_fork.{uid,gid}` 六字段同值

普通 `fork` 继承父凭证（`mproc/fork.rs:289` `User(creds.clone())`），`srv_fork` 由 RS 在消息中注入 `uid/gid`（`ipc.h:1422` `mess_lsys_pm_srv_fork { uid, gid, padding[48] }`，56B）：

```c
rmc->mp_realuid = m_in.m_lsys_pm_srv_fork.uid; // forkexit.c:206
rmc->mp_effuid  = m_in.m_lsys_pm_srv_fork.uid; // 207
rmc->mp_svuid   = m_in.m_lsys_pm_srv_fork.uid; // 208
rmc->mp_realgid = m_in.m_lsys_pm_srv_fork.gid; // 209
rmc->mp_effgid  = m_in.m_lsys_pm_srv_fork.gid; // 210
rmc->mp_svgid   = m_in.m_lsys_pm_srv_fork.gid; // 211
```

六字段同值写入与 `getset.c` 的 `do_set` 单字段 `setuid`（`m_in.m_lc_pm_setuid` 的 `uid`）对比，凸显"孵化时一次性确权 vs 运行时单步改权"：前者原子，后者需先以父凭证存在再改，三者（`real/eff/saved`）在改权窗口不一致。

### 1.5 与 `do_fork` 的差异对照图

| 步骤 | `do_fork` (07) | `do_srv_fork` (本章) | 差异原因 |
|------|----------------|---------------------|----------|
| 权限门 | 无（任意进程） | `RS_PROC_NR → EPERM`（`159-160`） | 能力边界：仅 RS 可孵化系统服务 |
| 标志继承 | `IN_USE|DELAY_CALL|TAINTED`（`106`，丢 `PRIV_PROC`） | `IN_USE|PRIV_PROC|DELAY_CALL`（`199-200`，**保留 `PRIV_PROC`**，丢 `TAINTED`） | 子为系统服务 vs 用户进程 |
| 调度接管 | `PRIV_PROC` 父→子 `User(SCHED)`（`101-103`） | 无（子 `scheduler==NONE` 保留） | 系统服务不经 `SCHED` |
| 凭证 | 继承父 `creds` | `m_in.{uid,gid}` 六字段注入（`206-211`） | 孵化时确权 |
| VFS 载荷 | `REUID/REGID=-1` 哨兵（`127-128`） | `REUID/REGID=uid/gid` 真实（`227-228`） | VFS 需为新服务设置凭证 |
| 回复 | `SUSPEND`（`139`，05 异步双回复） | `reply(child,OK)` + `return pid`（`237/239`，同步） | 系统服务立即可调度 |

其余 4 步同源（容量→槽位→`vm_fork`→复制→`get_free_pid`→`tell_vfs`→`SIGSTOP`）在 §2.2–2.7 仅作差异标注，不重复展开。

### 1.6 与其他 OS 的对照

Rust 改写不是照抄 `do_srv_fork` 的裸分支，而是在吸收工业级 OS 的成熟模式后做取舍。

**Linux `clone` 的 `CLONE_NEWUSER` + `setuid`。** Linux 以 `unshare(CLONE_NEWUSER)` 再 `setuid` 实现"孵化时确权"的两步，`srv_fork` 以 `m_lsys_pm_srv_fork.{uid,gid}` 一步注入原子化——前者通用（任意进程可 `unshare`），后者集中（仅 RS），Minix3 选择集中以保服务树的单根性。

**Redox `Scheme` 的权限注入。** Redox 的 `Scheme` 以 `acquire` 时的 `uid/gid` 决定新资源归属，`srv_fork` 以 `VFS_PM_SRV_FORK` 的 `REUID/REGID` 决定新服务归属，二者同为"创建时确权"，Redox 经 `Scheme` 统一，Minix3 经 `srv_fork` 专用调用。

**Fuchsia 的 `Component` 孵化。** Fuchsia 以 `Component Manager` 集中孵化 `Component` 并注入 `capability`，`RS` 的 `srv_fork` 孵化系统服务并注入 `PRIV_PROC` + `uid/gid` 同为"能力+身份"集中分发，PM 的 `EPERM` 门与 `CNode` 的能力检查同源。

**结论（本章的设计基线）。** 把 `do_srv_fork` 的"权限门→特权保留→凭证注入→真实 `REUID`→同步双回复"改写为"显式协调器 `handle_srv_fork(table, parent_ep, SrvForkParams{uid,gid}, transport)` + 显式构造 `Process::srv_fork_from` + 类型化投递 `VfsCall::SrvFork{reuid,regid}` + 显式双回复 `send(child,OK)`+`Ok(pid)`"，与 `do_fork` 的 `handle_fork` 共享 70% 编排，仅 `fork_from`/`SrvFork`/`Reply` 三处分歧显式化。

### 1.7 小结

1. **为什么 `srv_fork`**——系统服务的特权孵化器，需 `PRIV_PROC` 保留与 `uid/gid` 注入，非普通 `fork` 的用户进程路径可替代。
2. **为什么仅 RS**——`RS_PROC_NR → EPERM` 能力门，防伪造系统服务。
3. **为什么 5 处差异**——权限/标志/凭证/VFS 载荷/回复语义，其余 4 步同源（容量→槽位→VM→复制→PID→VFS→SIGSTOP）。
4. **为什么同步双回复**——系统服务 `scheduler==NONE` 直通，`VFS_PM_SRV_FORK_REPLY` 空分支（`main.c:398-401`）无需 `SCHED` 异步调度。
5. **为什么注入六字段**——`real/eff/saved` 三元组同值原子化，避 `fork→setuid` 窗口。

下一章逐行分析 C 的 `do_srv_fork`；第 3 章给出 Rust 的显式协调器与构造器。

---

## 2 C 源码分析

### 2.1 权限门：`RS_PROC_NR → EPERM`（forkexit.c:159-160）

```c
if (mp->mp_endpoint != RS_PROC_NR) // forkexit.c:159 mp 为当前进程（glo.h:8 mp）
    return EPERM;                   // forkexit.c:160 sys/errno.h:1 Operation not permitted
```

`RS_PROC_NR` 为 `com.h:62` 常量（`Endpoint::RS`，`minix-types/src/types/endpoint.rs` 单一真相），仅 `RS`（`RS` 自身 `endpoint==RS_PROC_NR`，启动期 `main.c:203-204` `RS_PROC_NR` 父为 `INIT`）可过门；`EPERM` 而非 `EACCES`，因 `srv_fork` 是能力（仅 RS 拥有），非文件权限。

### 2.2 容量与槽位轮转（forkexit.c:162-181）

与 `do_fork` `60-75` 同构（`procs_in_use == NR_PROCS || >=NR_PROCS-LAST_FEW && effuid!=0 → EAGAIN`，`next_child` 私有静态 `static unsigned int next_child=0` 在 `153`，各自轮转，双 `panic` 守卫 `can't find child slot` / `finds wrong child slot`），`next_child` 分离 vs 共享轮转的合理性见 §3.6（Rust 侧共享 `ProcTable::next_child` 单一真相，避免双静态漂移）。

### 2.3 `vm_fork` 同步段（forkexit.c:183-185）

与 `do_fork` `78-80` 同构（`vm_fork(rmp->endpoint, next_child, &child_ep)` 同步，失败 `return s`，成功后 `82` 注释不可失败窗口同样适用——`82` 注释在 `do_fork` 块但语义对 `do_srv_fork` 同成立，`kernel/system/do_fork.c:69-72` 的 `generation` 递增在 `sys_fork` 内）。

### 2.4 槽位占位与全量复制：差异在 `PRIV_PROC` 保留与六字段注入（forkexit.c:187-216）

```c
rmc = &mproc[next_child];             // 187
procs_in_use++;                       // 189
*rmc = *rmp;                          // 190
rmc->mp_sigact = mpsigact[next_child]; // 191
memcpy(rmc->mp_sigact, rmp->mp_sigact, sizeof(mpsigact[next_child])); // 192
rmc->mp_parent = who_p;               // 193
if (!(rmc->mp_trace_flags & TO_TRACEFORK)) { // 194-198 与 07 91-95 同构
    rmc->mp_tracer = NO_TRACER;
    rmc->mp_trace_flags = 0;
    (void) sigemptyset(&rmc->mp_sigtrace);
}
/* inherit only these flags */         // 199
rmc->mp_flags &= (IN_USE|PRIV_PROC|DELAY_CALL); // 199-200 差异：**保留 PRIV_PROC**，丢 TAINTED
rmc->mp_child_utime = 0;              // 201-205 子资源清零（同 07 107-111）
rmc->mp_child_stime = 0;
rmc->mp_exitstatus = 0;
rmc->mp_sigstatus = 0;
rmc->mp_endpoint = child_ep;          // 205 VM 返回
rmc->mp_realuid = m_in.m_lsys_pm_srv_fork.uid; // 206-211 差异：**六字段注入**（uid/gid 同值）
rmc->mp_effuid = m_in.m_lsys_pm_srv_fork.uid;
rmc->mp_svuid = m_in.m_lsys_pm_srv_fork.uid;
rmc->mp_realgid = m_in.m_lsys_pm_srv_fork.gid;
rmc->mp_effgid = m_in.m_lsys_pm_srv_fork.gid;
rmc->mp_svgid = m_in.m_lsys_pm_srv_fork.gid;
for (i = 0; i < NR_ITIMERS; i++) rmc->mp_interval[i] = 0; // 212-213
rmc->mp_started = getticks();         // 214
assert(rmc->mp_eventsub == NO_EVENTSUB); // 216 与 07 116 同
```

逐行要点：

- `PRIV_PROC` 保留使子 `is_kernel_process()==true` 且 `scheduler==NONE` 保留（`mproc/fork.rs:199` 的 `SRV_FORK_INHERIT_FLAGS` 含 `PRIV_PROC`，子 `scheduler` 不接管为 `SCHED`， vs 07 的 `fork_from` 接管）；
- 六字段注入覆盖父继承，`m_lsys_pm_srv_fork` 为 `ipc.h:1422` 的 `mess_lsys_pm_srv_fork { uid,gid,padding[48] }`（`uid` 为 `UID` 32 位，`gid` 为 `GID` 32 位）；
- `DELAY_CALL` 保留与 07 相同（`IN_USE|PRIV_PROC|DELAY_CALL` 含 `DELAY_CALL`，但 Rust 侧 `BlockState::default` 已不继承 `DELAY_CALL`，与 07 `mproc/fork.rs:307` 注释同理：mid-send 不可 fork，`DELAY_CALL` 继承为 whole-copy 副产物）。

### 2.5 PID 分配（utility.c:34-74）

与 07 `119-120` 同位置（`new_pid = get_free_pid()` → `rmc->mp_pid = new_pid`，`219-220`），`PidGenerator::get_free_pid` 的 `NR_PIDS` 回绕与双字段冲突与 `utility.c:34-74` 逐行对齐。

### 2.6 VFS 投递：`VFS_PM_SRV_FORK` 真实 `REUID/REGID`（forkexit.c:222-230）

```c
memset(&m, 0, sizeof(m));             // 222
m.m_type = VFS_PM_SRV_FORK;           // 223 0x908（com.h:528, RQ_BASE 0x900+8）
m.VFS_PM_ENDPT = rmc->mp_endpoint;    // 224 m7i1 子 endpoint
m.VFS_PM_PENDPT = rmp->mp_endpoint;   // 225 m7i2 父 endpoint
m.VFS_PM_CPID = rmc->mp_pid;          // 226 m7i3 子 PID
m.VFS_PM_REUID = m_in.m_lsys_pm_srv_fork.uid; // 227 m7i4 真实（vs 07 127 -1）
m.VFS_PM_REGID = m_in.m_lsys_pm_srv_fork.gid; // 228 m7i5 真实（vs 07 128 -1）
tell_vfs(rmc, &m);                    // 230 VFS_CALL 置子进程（utility.c:123-139）
```

`VFS_PM_SRV_FORK 0x908` 与 `VFS_PM_FORK 0x907` 仅 `m7i4/m7i5` 差异（`com.h:579-580`），`tell_vfs(rmc)` 的 `rmp` 即子 `rmc`，`VFS_CALL` 置子槽（`utility.c:138`），延续由 05 的 `VFS_PM_SRV_FORK_REPLY` 空分支消费（`main.c:398-401` `/* Nothing to do */`，`ipc/vfs.rs:298`）。

### 2.7 tracer 信号（forkexit.c:232-234）

与 07 `133-134` 同构（`if (mp_tracer != NO_TRACER) sig_proc(rmc, SIGSTOP, trace)`，`signal.c:384`，11 章详述，本章 DEFERRED）。

### 2.8 立即双回复 vs `SUSPEND`（forkexit.c:236-239）

```c
reply(rmc-mproc, OK);                 // 237 立即唤醒子（m_type=OK 0）
return rmc->mp_pid;                   // 239 同步返父 pid（main.c:106 Reply(pid)）
```

vs `do_fork` `139` `return SUSPEND`（`ReplyLater`，05 的 `VFS_PM_FORK_REPLY` 异步双回复 `reply(child,OK)`+`reply(parent,child_pid)` 且 `NEW_PARENT` 保护，`main.c:369-396`）。差异因调度直通（`scheduler==NONE` 无需 `sched_start_user`）。

### 2.9 对端视角

- **VM 侧**：`VM_FORK 0xC01` 同 07，`02-stage-vm/18`
- **VFS 侧**：`VFS_PM_SRV_FORK 0x908` → `VFS_PM_SRV_FORK_REPLY 0x988`（`com.h:541`），`REUID/REGID` 真实用于设置新服务 `fproc` 凭证，PM 侧 `tell_vfs` 后 VFS 侧 `05-stage-vfs` 落地

### 2.10 不变式分类

| 类别 | 检测 | 触发 | 严重度 |
|------|------|------|--------|
| `EPERM` | `forkexit.c:159-160` | `endpoint != RS_PROC_NR` | 可恢复（非 RS 误调用） |
| `EAGAIN` | `forkexit.c:166-171` | 满表/近满非 root | 可恢复 |
| `panic("can't find child slot")` | `forkexit.c:178-179` | 全表扫描仍 `IN_USE` | 不可达（容量检查已保证） |
| `assert(eventsub==NO_EVENTSUB)` | `forkexit.c:216` | 新子仍挂游标 | 不可恢复（06 不变量） |
| `reply(child,OK)`+`return pid` | `forkexit.c:237/239` | 成功孵化 | 同步双回复（`Reply(pid)`） |

---

## 3 Rust 设计决策

Rust 改写遵循"显式协调器 + 显式构造 + 类型化凭证"的 5 处差异显式化，其余 4 步同源复用 07 的编排。以下决策对应 `.design/08-design.v1.md` 的 D1–D8。

### D1：权限门 `RS → EPERM`（ARCH A-3）

`Endpoint::RS`（`minix-types/src/types/endpoint.rs` 单一真相，`RS_PROC_NR`）在 `handle_srv_fork` 首检 `if parent_ep != Endpoint::RS { return Err(EPERM) }`（`forkexit.c:159-160`），与 `Endpoint::PM`/`VFS` 等同源；`PmError::PermissionDenied → EPERM` 映射（`sys/errno.h:1`）。

### D2：标志继承 `IN_USE|PRIV_PROC|DELAY_CALL`（ARCH A-2）

`mproc/fork.rs:199` 新增 `SRV_FORK_INHERIT_FLAGS = IN_USE|PRIV_PROC|DELAY_CALL`（`RemainingFlags` 的 `PRIV_PROC` 位 + `BlockState` 的 `DELAY_CALL` 位），`srv_fork_from` 的 `RemainingFlags` 过滤仅保留 `PRIV_PROC`（`TAINTED` 丢弃，与 07 的 `FORK_INHERIT_FLAGS=TAINTED` 正交），`BlockState` 仍 `default` 不继承 `DELAY_CALL`（同 07 论证）。

### D3：凭证注入 `SrvForkParams{uid,gid}` 六字段（ARCH A-11）

`mess_lsys_pm_srv_fork` 的 `uid: u32, gid: u32`（`ipc.h:1422`）在 `minix-types` 建模为 `SrvForkParams { uid: Uid, gid: Gid }`（`mproc/credentials.rs` 的 `Credentials::new(uid,gid)` 六字段同值：`real=eff=saved=uid`，`group` 同理），`srv_fork_from` 覆盖父 `Credentials`，与 `getset.c` 的单字段 `do_set` 对比为孵化时确权。

### D4：`VfsCall::SrvFork{reuid,regid}` 真实 vs `Fork` 哨兵

`minix-types/src/ipc/vfs.rs:377` 的 `VfsCall::SrvFork { child, parent, child_pid, reuid, regid }` 已有 `reuid/regid` 字段，`encode` 的 `m7i4=reuid/m7i5=regid` 真实 vs `VfsCall::Fork` 的 `-1` 哨兵（`com.h:579-580`），`tell_vfs(child_slot, SrvFork{..., uid.into(), gid.into()}, transport)` 三段式同 07 的 `Fork`（`VFS_CALL` 置子进程）。

### D5：立即双回复 vs `SUSPEND`（ARCH A-6）

`handle_srv_fork` 成功路径 `tell_vfs` 后 `transport.send(child_ep, OK)` 立即唤醒子（`forkexit.c:237` `reply(rmc-mproc, OK)`），`Ok(pid)` 由 `init.rs:PM_SRV_FORK` 拦截映射 `Reply(pid)` 同步返父；`PmCall::SrvFork=41` 的 `dispatch_pm_call` 在 `init.rs` 拦截前为 `Reply(ENOSYS)` 占位，本章拦截后 `Reply(pid)`；`VFS_PM_SRV_FORK_REPLY 0x988` 空分支由 `ipc/vfs.rs:298` 已实现 `SrvFork → {}`，不 `sched_start_user`。

### D6：`next_child` 共享轮转的合理性

C 各自 `static next_child` 分离，Rust 侧 `ProcTable::next_child: Cell<usize>` 共享单轮转（`mproc/table.rs:138`），轮转语义 `(next_child+1)%NR_PROCS` 先递增后检查与 `forkexit.c:69/175` 同序，共享不影响正确性（`n<=NR_PROCS` 全表扫描保证找到空槽）且避免双静态漂移（`07-design.v1.md D2` 已论证，08 保留同论证）。

### D7：5 步同构（容量→槽位→`vm_fork`→复制→`get_free_pid`→`tell_vfs`→`SIGSTOP`）

`handle_srv_fork` 复用 `handle_fork` 的 9 步编排，仅 `fork_from` 改 `srv_fork_from`、`VfsCall::Fork` 改 `SrvFork`、`ReplyLater` 改 `Reply(pid)` + `send(child,OK)`，其余 `can_alloc`/`find_free_slot`/`vm_fork`/`get_free_pid`/`SIGSTOP` 同序。

### D8：`VFS_PM_SRV_FORK_REPLY` 空分支对照

`main.c:398-401` `case VFS_PM_SRV_FORK_REPLY: /* Nothing to do */ break` vs `main.c:369-396` `VFS_PM_FORK_REPLY` 的 `sched_start_user` 双分支（`ipc/vfs.rs:298` 空分支已对齐），系统服务 `scheduler==NONE` 直通。

---

## 4 实现详解

### 4.1 跨服务编排层（`os/servers/pm/src/fork.rs`）

`handle_srv_fork(table, parent_ep, params: SrvForkParams{uid,gid}, transport) -> Result<Pid, ForkCoordError>`（`fork.rs:320`，`PmError → EPERM/EAGAIN`）9 步：

1. `if parent_ep != Endpoint::RS { return Err(EPERM) }`（`D1`）；
2. `can_alloc_for_user(is_root)`（`D1`，`effuid==0` 即 `is_root`，`LAST_FEW=2`）；
3. `find_free_slot()`（`D2`）；
4. `vm_fork(parent_ep, child_slot)`（`D7`，失败 `Err(VmError)` 即返）；
5. `procs_in_use++` 后 `srv_fork_from(parent, child_idx, child_pid, child_ep, parent_idx, uid, gid)`（`D2/D3`，`PRIV_PROC` 保留，六字段注入）；
6. `get_free_pid`（`D3`，`NR_PIDS` 回绕 + 双字段冲突）；
7. `tell_vfs(SrvFork{child,parent,child_pid,reuid=uid,regid=gid})`（`D4`，`VFS_CALL` 置子进程）；
8. `sig_proc(SIGSTOP)` DEFERRED（11）；
9. `transport.send(child_ep, OK)` 立即唤醒子 + `Ok(pid)`（`D5`，同步返父）。

### 4.2 进程复制层（`os/servers/pm/src/mproc/fork.rs`）

`Process::srv_fork_from(parent, child_idx, child_pid, child_ep, parent_idx, uid, gid)`（`mproc/fork.rs:360`，与 `fork_from` 对照）：`Identity`（`pid/endpoint/procgrp`）→ `State`（`Running`/`BlockState::default`/`Normal{parent}`/`TraceState::default`）→ `Privilege::Kernel` 保留（`is_kernel_process()==true`）→ `Credentials::new(uid,gid)` 六字段注入（覆盖父）→ `Resources`（`child_utime/stime=0`/`intervals=0`/`scheduler=NONE`/`RemainingFlags::PRIV_PROC`）→ `SignalState` 克隆→ `Ipc::default`。差异仅 3 处：`privilege` 保留 `Kernel`、 `scheduler` 不接管、`flags` 保留 `PRIV_PROC`。

### 4.3 协议层（`os/libs/minix-types/src/ipc/{vfs,message}.rs`）

- `VFS_PM_SRV_FORK 0x908`/`REPLY 0x988` + `VfsCall::SrvFork{reuid,regid}` 真实（`vfs.rs:377`），`MessLsysPmSrvFork { uid,gid }` 56B（`message.rs:MessLsysPmSrvFork`，`uid`@0 `gid`@4）；
- `VmForkIn` 同 07，`02-stage-vm/18`。

### 4.4 分发与 `run_once` 接线（`os/servers/pm/src/ipc/calls.rs` + `init.rs`）

- `PmCall::SrvFork=41`（`calls.rs:175`）`from_call_nr` 与 `table.c:23` 同注册；
- `init.rs:PM_SRV_FORK` 拦截 `if m_type==41 { if parent_ep != RS → EPERM else handle_srv_fork → Ok(pid)→Reply(pid) / Err→Reply(errno) }`（与 `PM_FORK` 的 `ReplyLater` 分支正交，`init.rs:332` 新增）；
- `ipc/vfs.rs:298` 空分支 `SrvFork → {}` 已对齐 `main.c:398-401`。

### 4.5 不变量表

| # | 不变量 | C 锚点 | Rust 表达 |
|---|--------|--------|-----------|
| 1 | 非 RS → `EPERM` | `forkexit.c:159-160` | `if parent_ep != RS → EPERM` |
| 2 | `PRIV_PROC` 保留 | `forkexit.c:199-200` | `SRV_FORK_INHERIT_FLAGS` 含 `PRIV_PROC` |
| 3 | 六字段同值注入 | `forkexit.c:206-211` | `Credentials::new(uid,gid)` |
| 4 | `VFS_CALL` 置子进程且 `REUID/REGID` 真实 | `forkexit.c:227-230` | `tell_vfs(child_slot, SrvFork{reuid,regid})` |
| 5 | `reply(child,OK)`+`return pid` vs `SUSPEND` | `forkexit.c:237/239` | `send(child,OK)`+`Ok(pid)→Reply(pid)` |
| 6 | `VFS_PM_SRV_FORK_REPLY` 空分支 | `main.c:398-401` | `VfsReply::SrvFork → {}` |

---

## 5 测试矩阵

### 5.1 `mproc/fork.rs`（`srv_fork_from` 显式构造）

- `test_srv_fork_privilege_retained`：`Kernel` 父→子 `is_kernel_process()==true` 且 `scheduler==NONE`（`PRIV_PROC` 保留）
- `test_srv_fork_credentials_injected`：`uid=1001,gid=100 → child.credentials.user.real==1001` 六字段同值
- `test_srv_fork_flags_inheritance`：`PRIV_PROC` 保留，`TAINTED` 丢弃（与 `fork` 正交）
- `test_srv_fork_no_tainted`：`ALARM_ON|TAINTED` → 仅 `PRIV_PROC` 保留（若父有）
- `test_srv_fork_intervals_cleared`：`intervals` 清零
- `test_srv_fork_ipc_reset`：`reply/event_subscriber` 清零（`NO_EVENTSUB`）

### 5.2 `fork.rs`（`handle_srv_fork` 编排）

- `test_srv_fork_eperm`：`parent_ep != RS → Err(EPERM)`（`find_parent` 前即拦）
- `test_srv_fork_success`：`RS → Ok(pid)` 且子 `PRIV_PROC` 且 `VFS_CALL` 且 `transport` 含 `VFS_PM_SRV_FORK` 且 `tracer` 分支
- `test_srv_fork_parent_not_found`：`InvalidEndpoint`（`find_parent_slot` 失败）
- `test_srv_fork_table_full`：`can_alloc` 满表 → `EAGAIN`
- `test_srv_fork_vfs_call`：`VfsCall::SrvFork` 的 `reuid/regid` 真实 vs `Fork` 的 `-1`（`m7i4/m7i5`）
- `test_srv_fork_immediate_reply`：`handle_srv_fork` 后 `transport` 含 `send(child,OK)` 立即唤醒子（`VFS_CALL` 同时置于子进程）

### 5.3 集成与跨文档

- `ipc/calls.rs:243` `test_dispatch_srv_fork_is_reply`：`PmCall::SrvFork → Reply(pid)`（`forkexit.c:239`）vs `Fork → ReplyLater`
- `init.rs:865` `test_run_once_srv_fork_immediate_reply`：`run_once(PM_SRV_FORK)` → `Handled` 且 `send(child,OK)` + `send(parent,pid)` 双回复（`main.c:237/239`）
- `ipc/vfs.rs:310` `test_srv_fork_is_noop_then_tail`（05 §5.2）：`VFS_PM_SRV_FORK_REPLY → {}` + `restart_sigs` 尾部（空分支后仍 `IN_USE→restart_sigs`）

完整清单：`rg "^\s*fn test_srv" os/servers/pm/src/{fork,mproc/fork}.rs`（~8）+ `rg "srv_fork" os/servers/pm/src/{init,ipc}`（3）— 本章直接相关 **~11** 项；`cargo test -p minix-pm --lib` 截至 2026-09-03 为 **160 passed**（含 07 的 `fork.rs` 4 + `mproc/fork.rs` 15，已在基线内，07 后 `cargo test` 仍 160/108，本章新增 6 项后将至 166/108）。

---

## 6 过渡

`do_srv_fork` 在主循环 `PM_SRV_FORK` 与 `VFS_PM_SRV_FORK_REPLY` 空分支之间的位置，与 `do_fork` 共享 70% 流程但回复语义相反的对照图，为下游 09/10（`exit`/`wait` 的 `PRIV_PROC` 特权进程直毁与 `procs_in_use` 回收）的前置——`srv_fork` 子为 `PRIV_PROC` 系统服务，其 `exit_proc` 走 `sys_clear` 直毁（`forkexit.c:361-369`，09），无需 `tracer` 的 `SIGSTOP` 恢复（11）。

**下一入口**：

- **09-pm-exit.md**——`exit_proc` / `exit_restart` / `zombify` / `check_parent` / `disinherit`（`NEW_PARENT` 真实设置点）与 `fork`/`srv_fork` 的 `procs_in_use` 计数闭环；
- **03-stage-rs**——RS 侧 `service_create` 如何经 `PM_SRV_FORK` 孵化新服务（`RS_PROC_NR` 能力门的上游调用者）。

---

## 7 参见

- C 源（ground truth）：`minix3/minix/servers/pm/forkexit.c:142-240`（`do_srv_fork`）、`minix3/minix/servers/pm/utility.c:34-74`（`get_free_pid`）、`minix3/minix/include/minix/ipc.h:1422`（`mess_lsys_pm_srv_fork`）、`minix3/minix/include/minix/com.h:528/541/579-580`（`VFS_PM_SRV_FORK` 字段）、`minix3/minix/include/minix/callnr.h:54`（`PM_SRV_FORK 41`）、`minix3/minix/servers/pm/main.c:398-401`（`SRV_FORK_REPLY` 空分支）、`minix3/minix/servers/pm/signal.c:384`（`sig_proc`）
- 设计契约：`.design/08-design.v1.md`（D1–D8 与行为契约表）、`.design/08-outline.v1.md`、`.design/08-outline-review.v1.md`
- PM 阶段文档：07-pm-fork.md（`do_fork` 全链路，差异对照主）、03-mproc-table.md（`can_alloc`/`find_free_slot`/`get_free_pid`）、04-ipc-dispatch.md（`Reply(pid)` vs `ReplyLater`）、05-vfs-interaction.md（`tell_vfs` 与 `handle_vfs_reply` 的 `SRV_FORK` 空分支）、02-mproc-struct.md（`PRIV_PROC` 与 `Credentials`）、`minix/ipc.h:1422`（`mess_lsys_pm_srv_fork`）、16-scheduling.md（`scheduler==NONE`）、11-signal-core.md（`sig_proc`）、08-pm-srv-fork.md（`PRIV_PROC` 差异）
- 对端实现：`02-stage-vm/18-vm-fork.md`（`vm_fork` 对端）、`05-stage-vfs`（`VFS_PM_SRV_FORK` 对端）
- 内核接口：`01-stage-kernel/06-proc-init-boot-proc.md`（`boot_image`）、`01-stage-kernel/19-syscall-signal.md`（`sig_proc` 内核路径）
- Rust 实现：`os/servers/pm/src/fork.rs`（`handle_srv_fork` 协调器）、`os/servers/pm/src/mproc/fork.rs`（`Process::srv_fork_from`）、`os/libs/minix-types/src/ipc/vfs.rs`（`VfsCall::SrvFork`）、`os/libs/minix-types/src/ipc/message.rs`（`MessLsysPmSrvFork`）
