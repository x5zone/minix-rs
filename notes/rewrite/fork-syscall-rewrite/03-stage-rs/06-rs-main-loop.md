# 06-rs-main-loop: 主循环与消息分发

> **分类**: 阶段 3 — 主循环与监控（RS 运行时心脏）
> **源码**: `minix3/minix/servers/rs/main.c:38-131`（`main()`）、`minix3/minix/servers/rs/utility.c:223-233`（`rs_asynsend`）、`utility.c:309-341`（`reply`/`late_reply`）、`utility.c:351-359`（`rs_isokendpt`）、`utility.c:424-479`（`rs_is_idle`/`rs_idle_period`）、`servers/rs/const.h:45,49`（`RS_SRV_IS_IDLE`/`RS_DELTA_T`）、`main.c:631-704`（signal 回调）、`sys/sys/errno.h:199`（`EDONTREPLY`）
> **Rust 模块**: `os/servers/rs/src/dispatch.rs`（`IpcStatus`/`DispatchKind`/`classify`/`DispatchResult`/`dispatch_request`）、`os/libs/minix-types/src/types/errno.rs`（`EDONTREPLY`）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/01-rs-boot-init.md`（启动链）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md`（`rs_isokendpt`、`RS_DEAD`/`RS_ACTIVE` 位）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/04-rs-access-control.md`（handler 第一行）
> **说明**: boot 完成后（01 的 4 步 + `sys_setalarm`），RS 进入 `main()` 的 while 死循环。主循环由三活动构成（取活/干活/回活），四类消息在分派前分类。本文档是运行时骨架：只定义分类、回复协议与空闲期后台任务，各 handler 机制一律移交（07/12~16）。

---

## 1. 概念：RS 的运行时心脏

### 1.0 章节引言

`01-rs-boot-init.md` 走完 boot 四步后，RS 的 `main()` 进入一个**永不终止**的循环（除非 panic）。这个循环是整个 RS 服务的引擎：系统服务生命周期里的每一个事件（启动请求、心跳、崩溃恢复、Live Update）都从主循环的一个消息开始。

> **本章不讲什么**（机制一律移交）:
> - `do_period` 的周期检查逻辑（`07-rs-period-heartbeat.md`）
> - `do_init_ready`/`do_upd_ready` 的 ready 语义（`12-rs-init-run.md`、update 分支 `16-rs-live-update.md`）
> - 各 `do_*` 请求 handler（`13/14/16`）
> - signal 处理细节与 RS 自身生命周期（`18-rs-self-lifecycle.md`）
>
> 本章只回答一个问题：**主循环怎么拿到消息、怎么分类、怎么回复，以及空闲时做什么**——骨架本身。

### 1.1 三活动结构（WHY）

`main()` 的注释（main.c:40-43）直接定义了主循环：

> "The main loop consists of three major activities: getting new work, processing the work, and sending the reply."

```
while (TRUE) {
    rs_idle_period();          // ① 空闲期后台任务（main.c:59）
    get_work(&m, &ipc_status); // ② 取活：阻塞等消息（main.c:62）
    ... 分类 + 处理 ...        // ③ 干活：四类消息分派（main.c:70-121）
    reply(...);                // ④ 回活：sendnb 回复（main.c:124-129）
}
```

「单线程事件循环」是 Minix3 用户态服务器的标准形态（`01-stage-kernel` 的 SEF 约定）：没有线程，一次只处理一个消息，handler 不得阻塞（阻塞即死锁整个服务）。这直接决定 Rust 侧 `!Send`/`!Sync`/`Rc`/`RefCell` 的合理性（AGENTS.md 执行模型约束）。

### 1.2 四类消息分类（WHAT）

`main()` 的注释（main.c:70-75）列出四类期望消息：

| 类 | 判定 | 分支（main.c） | 处理 | 机制文档 |
|----|------|---------------|------|---------|
| 心跳 notify（服务通知） | `is_ipc_notify` 且非 CLOCK | 80, 85-91 | 记 `r_alive_tm = timestamp`；未知发送者告警 | 07 |
| 系统 notify（同步闹钟） | `is_ipc_notify` 且 `who_p == CLOCK` | 80-83 | `do_period` | 07 |
| 用户请求 | 非 notify 的 `RS_*` | 102-114 | `do_up`/`do_down`/… | 13/14/16 |
| ready 消息 | `RS_INIT`/`RS_LU_PREPARE` | 116-117 | `do_init_ready`/`do_upd_ready` | 12/16 |

两个前置验证在分类前完成：

1. **`rs_isokendpt(who_e, &who_p)`**（main.c:64，utility.c:351-359）：发送者端点必须在 `[-NR_TASKS, NR_PROCS)` 槽位范围，否则 **panic**（"message from bogus source"）——注意是 panic 不是忽略：主循环信任内核的端点合法性，端点越界意味着内核状态损坏。
2. **`call_nr = m.m_type`**（main.c:68）随后用于 `switch`。

**分类顺序**：先 `is_ipc_notify(ipc_status)`（notify 消息不需要回复，main.c:77-79），再按 `who_p == CLOCK` 区分系统 notify 与心跳 notify；非 notify 才进 `switch(call_nr)`。注意 `RS_INIT`/`RS_LU_PREPARE` 虽然叫"消息"，但它们是**非 notify** 请求（服务的 SENDREC 回复），走正常请求分支（main.c:116-117）。

---

## 2. C 源码分析

### 2.1 `main()`（main.c:38-131）逐段

```c
int main(void)                                     /* main.c:38 */
{
  sef_local_startup();                             /* main.c:51 */
  if (OK != (s=sys_getmachine(&machine)))          /* main.c:53 */
	  panic("couldn't get machine info: %d", s);

  while (TRUE) {                                   /* main.c:57 */
      rs_idle_period();                            /* main.c:59 */
      get_work(&m, &ipc_status);                   /* main.c:62 */
      who_e = m.m_source;                          /* main.c:63 */
      if(rs_isokendpt(who_e, &who_p) != OK) {      /* main.c:64 */
          panic("message from bogus source: %d", who_e);
      }
      call_nr = m.m_type;                          /* main.c:68 */
      if (is_ipc_notify(ipc_status)) {             /* main.c:80 */
          switch (who_p) {
          case CLOCK:                              /* main.c:82 */
	      do_period(&m);
	      continue;                              /* main.c:83-84 — notify 不 reply */
	  default:                                   /* main.c:85 */
	      if (rproc_ptr[who_p] != NULL) {        /* main.c:86 */
		  rproc_ptr[who_p]->r_alive_tm = m.m_notify.timestamp; /* 87 */
	      } else {
		  printf("RS: warning: got unexpected notify message ..."); /* 89-90 */
	      }
	  }
      }
      else {
          switch(call_nr) {                        /* main.c:100 */
          case RS_UP:   result = do_up(&m);        break;
          /* ... RS_DOWN..RS_FI, RS_GETSYSINFO, RS_LOOKUP ... */
	  case RS_INIT:  result = do_init_ready(&m); break;    /* 116 */
	  case RS_LU_PREPARE: result = do_upd_ready(&m); break; /* 117 */
          default:                                  /* main.c:118 */
              printf("RS: warning: got unexpected request %d from %d\n", ...);
              result = ENOSYS;                      /* main.c:121 */
          }
          if (result != EDONTREPLY) {               /* main.c:125 */
	      m.m_type = result;                     /* main.c:126 */
              reply(who_e, NULL, &m);               /* main.c:127 */
          }
      }
  }
}
```

要点：

- **notify 消息 `continue` 不回复**（main.c:83-84）——notify 是异步通知，无回复通道。
- **心跳时间戳**：`m.m_notify.timestamp` 记入槽位 `r_alive_tm`（main.c:87），07 的心跳监控靠它判断服务是否存活；发送者无槽位（`rproc_ptr[who_p] == NULL`）→ 告警但**不 panic**（区别于 64 行的端点校验）。
- **handler 负责权限检查**：注释（main.c:99）"Handler functions are responsible for permission checking"——即 04 的 `check_call_permission` 是每个 handler 的第一行，主循环不做统一权限判断。
- **`EDONTREPLY` 抑制回复**（main.c:125）：handler 返回伪 errno `EDONTREPLY`（sys/errno.h:199）时跳过 reply——用于晚回复协议（§2.2）。
- **`m.m_type = result`**（main.c:126）：回复消息的 `m_type` 即 handler 返回值（OK=0 或 errno），这是 Minix3 的约定回复格式。

### 2.2 reply 协议：`reply`/`late_reply`/`EDONTREPLY`/`RS_LATEREPLY`

```c
void reply(who, rp, m_ptr)                         /* utility.c:309 */
{
  if(who == RS_PROC_NR) {                          /* utility.c:317 */
      return;      /* 不给自己回信 */
  }
  r = ipc_sendnb(who, m_ptr);                      /* utility.c:324 */
  if (r != OK)
      printf("RS: unable to send reply to %d: %d\n", who, r);
}

void late_reply(rp, code)                          /* utility.c:332 */
{
  if(rp->r_flags & RS_LATEREPLY) {                 /* utility.c:337 */
      message m;
      m.m_type = code;
      reply(rp->r_caller, NULL, &m);               /* utility.c:344 */
      rp->r_flags &= ~RS_LATEREPLY;                /* utility.c:345 */
  }
}
```

协议语义：

1. **`reply` 用 `ipc_sendnb`（非阻塞 send）**（utility.c:324）：主循环不阻塞等待接收方。给自己（RS）不回复（utility.c:316-318）。
2. **`EDONTREPLY` 是"晚回复"入口**：某些 handler（如 `do_up`/`do_down`/`do_update`）需要等目标服务初始化完成后才回复（`RS_LATEREPLY` 标志，const.h:33）——此时 handler 返回 `EDONTREPLY`，主循环跳过立即回复（main.c:125）；后续 `late_reply`（utility.c:332-347）在事件完成时补发（12/15 使用）。
3. `late_reply` 只对置了 `RS_LATEREPLY` 的槽生效，回复后清标志（utility.c:337-345）。
4. `EDONTREPLY = _SIGN 203`（sys/errno.h:199）——"pseudo-code: don't send a reply"，不是真实 errno。Rust 常量在 `minix-types`（errno.rs），`DispatchResult::is_reply_suppressed()` 封装判定（dispatch.rs）。

### 2.3 `rs_isokendpt`（utility.c:351-359）

```c
int rs_isokendpt(endpoint_t endpoint, int *proc)
{
	*proc = _ENDPOINT_P(endpoint);                 /* utility.c:354 */
	if(*proc < -NR_TASKS || *proc >= NR_PROCS)     /* utility.c:355 */
		return EINVAL;
	return OK;
}
```

端点槽位校验：`[-NR_TASKS, NR_PROCS)` 范围。02 §3.5 已建模为 `RProcTable::isokendpt`（process_table.rs，纯函数 `Result<i32, Errno>`）。注意它只验**槽位范围**，不验槽位是否在 RS 表内——表内性由各 handler 用 `lookup_*` 系列确认。

> **R12（2026-08-16）**：06 接线时 `classify` 前必须跑 `isokendpt`（main.c:63-66）——快速索引
> `endpoint_slot`/`set_endpoint_index` 已对越界端点（`NONE`/`ANY`/`SELF`）fail-closed（02 §3.1），
> 但"拒绝非法源"仍是主循环的前置职责（C 里是 panic 级别的内核状态损坏信号；Rust 骨架在
> `classify` 后由 handler 返回 `EINVAL`，接线时按 06 语义选择 panic 或 `EINVAL`）。

### 2.4 `rs_asynsend`（utility.c:223-233）

```c
int rs_asynsend(struct rproc *rp, message *m_ptr, int no_reply)
{
  if(no_reply) {
      r = asynsend3(rpub->endpoint, m_ptr, AMF_NOREPLY);  /* utility.c:231 */
  } else {
      r = asynsend(rpub->endpoint, m_ptr);               /* utility.c:234 */
  }
  return r;
}
```

RS 向服务发**异步消息**的封装：`no_reply` 用 `AMF_NOREPLY`（ipc.h:2760）标记不期待回复（如 `fi_service` 的故障注入，14）。Rust 侧由 19 的 minix-sys 接线（DEFERRED），本文档只陈述语义。

### 2.5 `rs_is_idle`/`rs_idle_period`（utility.c:424-479）+ `RS_SRV_IS_IDLE`（const.h:45）

```c
int rs_is_idle()                                     /* utility.c:424 */
{
  for (slot_nr = 0; slot_nr < NR_SYS_PROCS; slot_nr++) {
      rp = &rproc[slot_nr];
      if (!(rp->r_flags & RS_IN_USE)) continue;      /* utility.c:430 */
      if(!RS_SRV_IS_IDLE(rp)) return 0;              /* utility.c:433-434 */
  }
  return 1;
}
```

`RS_SRV_IS_IDLE(S)`（const.h:45）= `(S)->r_flags & RS_DEAD` **或** `(S)->r_flags & ~(RS_IN_USE|RS_ACTIVE|RS_CLEANUP_DETACH|RS_CLEANUP_SCRIPT) == 0`——即：槽要么已标记死亡（清理中），要么只有"空闲位"（in-use/active/清理态），没有任何进行中操作位（LATEREPLY/INITIALIZING/EXITING/TERMINATED/…）。全部槽满足 → RS 空闲。

```c
void rs_idle_period()                                /* utility.c:443 */
{
  if(!shutting_down && !rs_is_idle()) return;        /* utility.c:453-454 */
  /* 清理死服务 */
  for (rp=BEG_RPROC_ADDR; rp<END_RPROC_ADDR; rp++) {
      if((rp->r_flags & (RS_IN_USE|RS_DEAD)) == (RS_IN_USE|RS_DEAD)) { /* 459 */
          cleanup_service(rp);                       /* 460 → 15 */
      }
  }
  if (shutting_down) return;                         /* utility.c:464 */
  /* 补 replica */
  for (rp=...; rp<END_RPROC_ADDR; rp++) {
      if((rp->r_flags & RS_ACTIVE) && (rpub->sys_flags & SF_USE_REPL)
          && rp->r_next_rp == NULL) {                /* utility.c:469 */
          if(rpub->endpoint == VM_PROC_NR && (rp->r_old_rp || rp->r_new_rp)) {
              continue;  /* VM 同一时刻最多一个 replica */ /* utility.c:470-472 */
          }
          clone_service(rp, RST_SYS_PROC, 0);        /* utility.c:474 → 10 */
      }
  }
}
```

两个后台任务：

1. **RS_DEAD 清理**（utility.c:457-462）：标记死亡的服务槽（`RS_IN_USE|RS_DEAD`）执行 `cleanup_service`（→15）。
2. **补 replica**（utility.c:466-479）：`SF_USE_REPL` 且无 `r_next_rp` 的活跃服务 → `clone_service`（→10）；**VM 例外**：update 期间（`r_old_rp || r_new_rp`）最多一个 replica（utility.c:470-472）。

**`shutting_down` 覆盖**（utility.c:449-455, 464）：关停期间即使不空闲也执行清理（注释：避免死锁——关停时死服务必须被清掉），但不再补 replica。

### 2.6 signal 回调（main.c:631-704）

```c
static void sef_cb_signal_handler(int signo)         /* main.c:631 */
{
  switch(signo) {
      case SIGCHLD: do_sigchld(); break;             /* main.c:635-637 → 07 */
      case SIGTERM: do_shutdown(NULL); break;        /* main.c:638-640 → 13 */
  }
}

static int sef_cb_signal_manager(endpoint_t target, int signo)  /* main.c:647 */
{
  /* rs_isokendpt + 槽查找；RS_TERMINATED 非 EXITING → EDEADEPT；inactive → OK */
  /* SIGS_IS_STACKTRACE → sys_diagctl_stacktrace（→19） */
  if(SIGS_IS_TERMINATION(signo)) {                   /* main.c:686 */
      rp->r_flags |= RS_TERMINATED;                  /* main.c:687 */
      terminate_service(rp);                         /* main.c:688 → 15 */
      rs_idle_period();                              /* main.c:689 */
      return EDEADEPT;                               /* main.c:691 */
  }
  if (rp->r_pub->endpoint == VM_PROC_NR) return OK;  /* main.c:695 — 不转发 VM */
  /* 非终止信号 → SIGS_SIGNAL_RECEIVED 消息 */
}
```

两个回调（01 的 SEF 注册表，机制在 18 展开）：

- **`sef_cb_signal_handler`**：RS 自己收到的信号。SIGCHLD → `do_sigchld`（07 的子进程清理）；SIGTERM → `do_shutdown`（13 的关停）。
- **`sef_cb_signal_manager`**：内核转发的**系统信号**（RS 是系统服务信号管理器）。终止信号 → 置 `RS_TERMINATED` + `terminate_service`（→15）+ 立即 `rs_idle_period`；非终止信号 → 转成 `SIGS_SIGNAL_RECEIVED` 消息发给服务；**VM 免转发**（main.c:695）。

---

## 3. Rust 设计决策

### 3.1 dispatch.rs 分类器（D1）

`dispatch.rs`（在 00/01 骨架期建立，本文档正式归属）：

```rust
pub struct IpcStatus { pub flags: u32 }              // ipc_status 字
impl IpcStatus { pub fn is_notify(&self) -> bool }   // 低 6 位 == NOTIFY(4)

pub enum DispatchKind {
    ClockNotify,                                     // main.c:82
    HeartbeatNotify(Endpoint),                       // main.c:85-91
    InitReady,                                       // RS_INIT → 12
    LuPrepareReady,                                  // RS_LU_PREPARE → 12/16
    Request(i32),                                    // RS_* → 13/14/16
}

pub fn classify(ipc_status, who_p, call_nr) -> DispatchKind
pub struct DispatchResult(pub i32);                  // handler 返回值
impl DispatchResult { pub const fn is_reply_suppressed(&self) -> bool }
pub fn dispatch_request(call_nr) -> DispatchResult   // 目前全 ENOSYS（fail-closed）
```

设计差异：

- **`DispatchKind` 编码 C 的分类树**：notify 先分（CLOCK vs 心跳），非 notify 按 `call_nr` 分（ready vs 请求）——C 的 if/switch 嵌套变成显式枚举，`match` 穷尽。
- **`DispatchResult` 携带 EDONTREPLY 语义**：`is_reply_suppressed()`（dispatch.rs）对应 main.c:125 的判定；handler 尚未接线前全部 `ENOSYS`，与 C 的 `default` 分支（main.c:118-121）一致（fail-closed）。
- **`Endpoint` 槽位校验前置**：`classify` 的调用方（未来的 `main.rs`）先做 `RProcTable::isokendpt`（02 §3.5）再分类——与 C 的 main.c:64 顺序一致。

### 3.2 消息类型归属 minix-types（D2，A-2）

RS_* 常量统一来自 `minix-types::ipc::rs`（04 §3.5 的 `ipc/rs.rs`，A-2 唯一权威来源）；`dispatch.rs` 顶部 `use minix_types::{RS_UP, RS_INIT, …}` 直接导入，无常量区。值断言测试（`test_rs_constants_match_c`）对照 com.h:465-482 防漂移。

### 3.3 reply 协议抽象（D3，DEFERRED→19）

`reply`/`late_reply`/`rs_asynsend` 依赖 `ipc_sendnb`/`asynsend`/`asynsend3`——19 的 minix-sys 接线项。Rust 侧设计为 `ReplyChannel` trait（DEFERRED，占位在 19）：`fn reply(who, m) -> Result<(), i32>`、`fn late_reply(slot, code)`。本文档只定义语义契约（§2.2），不落实现。

### 3.4 idle 逻辑纯函数化（D4）

`rs_is_idle`/`rs_idle_period` 是纯表操作，Rust 侧两个可测函数（DEFERRED 实现，07/10/15 的依赖方）：

```rust
fn is_idle(table: &RProcTable) -> bool;             // RS_SRV_IS_IDLE 全表（const.h:45）
fn idle_period_cleanup(table) -> Vec<SlotId>;       // RS_DEAD 槽列表（→15 cleanup_service）
fn idle_period_replicas(table) -> Vec<SlotId>;      // 缺 replica 的活跃槽（→10 clone_service）
```

`shutting_down` 全局 → 参数注入（同 04 的 `updating` 模式）。

---

## 4. 实现详解（dispatch.rs）

模块结构：

```
dispatch.rs
├─ RS_* 常量导入（use minix_types，04 ipc/rs.rs 唯一权威）
├─ IpcStatus（ipc_status 字 + is_notify）
├─ DispatchKind（五变体，四类消息 + Request）
├─ classify(status, who_p, call_nr)（main.c:70-127 分类树）
├─ DispatchResult(i32) + is_reply_suppressed()（main.c:124-129）
├─ dispatch_request(call_nr)（switch 归属表，全 ENOSYS）
└─ #[cfg(test)] 7 个测试：四类分类 + 未知请求 ENOSYS + EDONTREPLY 判定 + 常量断言
```

关键不变量：

1. **分类必须先于一切**：`classify` 不触表、不校验端点——校验由调用方（02 的 `isokendpt`）先做，保持纯函数。
2. **notify 永不回复**：`DispatchKind::ClockNotify/HeartbeatNotify` 不产生 `DispatchResult`——类型系统上"无回复通道"。
3. **未知请求 fail-closed**：`dispatch_request` 的 `_ => ENOSYS` 与 C 的 default（main.c:118-121）一致；handler 接线后逐项替换。
4. **`RS_INIT`/`RS_LU_PREPARE` 是请求不是 notify**：`classify` 先判 `is_notify` 再 match，与 C 顺序一致（main.c:80 与 116-117）。

---

## 5. 测试要点

`cargo test -p minix-rs --lib dispatch` 中 dispatch 相关测试（dispatch.rs `#[cfg(test)]`，7 项，7/7 已落地；全局测试数是并行模块增长快照，非承诺）：

| 测试 | 覆盖 |
|------|------|
| `test_classify_clock_notify` | CLOCK notify → `ClockNotify` |
| `test_classify_heartbeat_notify` | 其他 notify → `HeartbeatNotify(endpoint)` |
| `test_classify_ready` | `RS_INIT`/`RS_LU_PREPARE` → ready 类 |
| `test_classify_request` | `RS_UP` → `Request(RS_UP)` |
| `test_classify_request_unknown` | 未知调用 → `Request(9999)` → `ENOSYS` |
| `test_dispatch_result_reply_suppression` | `EDONTREPLY` 抑制回复（main.c:125） |
| `test_rs_constants_match_c` | RS_* 常量值断言（com.h:465-482） |

---

## 6. 过渡：从"骨架"到"心跳"

主循环是**骨架**：它分类消息、转发给 handler、管理回复，但自身不含任何业务逻辑。下一站 `07-rs-period-heartbeat.md` 填充第一个机制：`do_period`（周期检查）+ 心跳状态机（`r_alive_tm`/`r_check_tm`/backoff）——它们由主循环的 `ClockNotify` 分支（main.c:82）与心跳 notify 分支（main.c:85-91）驱动。

生命周期位置：**运行时的第一层**——服务创建（08~11）、初始化（12）、控制/查询（13/14）、终止恢复（15）、Live Update（16~18）全部经由主循环分派；04 的访问控制是每个 handler 的第一行，05 的掩码早已固化进内核。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/01-rs-boot-init.md` — 启动链与 SEF 注册
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md` — `rs_isokendpt`、`RS_DEAD`/`RS_ACTIVE`/`RS_LATEREPLY` 位
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/04-rs-access-control.md` — handler 第一行权限检查
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/07-rs-period-heartbeat.md` — do_period/心跳/do_sigchld
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/12-rs-init-run.md`、`16-rs-live-update.md` — ready 消息
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/13-rs-control-requests.md`、`14-rs-query-requests.md` — 请求 handler
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/15-rs-terminate-restart.md` — cleanup_service/terminate_service
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/18-rs-self-lifecycle.md` — signal 回调细节
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/19-rs-external-interfaces.md` — reply/rs_asynsend 接线
- `minix3/minix/servers/rs/main.c:38-131`、`utility.c:223-233,309-359,424-479`、`const.h:45,49`、`sys/sys/errno.h:199` — ground truth
- `os/servers/rs/src/dispatch.rs`、`os/libs/minix-types/src/types/errno.rs` — Rust 实现
