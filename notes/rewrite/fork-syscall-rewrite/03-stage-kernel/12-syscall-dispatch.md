# 12-syscall-dispatch: 系统调用分派

> **源码**: `minix3/minix/kernel/system.c:52-167`
> **前置**: 11（IPC 错误码）
> **C 行数**: ~150 行

---

## Ch1: 概念

### 1.1 系统调用 vs IPC

| 维度 | IPC（do_ipc） | 系统调用（kernel_call） |
|------|--------------|----------------------|
| 本质 | 进程间消息传递，内核只做中转 | 进程请求内核执行特权操作 |
| 入口 | `sys_call()` → `do_ipc()` | `sys_call()` → `kernel_call()` |
| 判断 | `m_type < KERNEL_CALL` | `m_type >= KERNEL_CALL` |
| 权限 | `s_ipc_to` 掩码 | `s_k_call_mask` 掩码 |
| 阻塞 | 可能（SEND/RECEIVE） | 不阻塞（直接执行 handler） |

### 1.2 入口路径

```
用户态: sys_call(SYS_VMCTL, ...)
  → INT 0x80 / SYSENTER / SVC
  → 汇编 sys_call 入口
  → if m_type >= KERNEL_CALL:
      → kernel_call(m_user, caller)
        → copy_msg_from_user → kernel_call_dispatch → call_vec[call_nr] → handler
        → kernel_call_finish
    else:
      → do_ipc(call_nr, r2, r3)
```

C 源码: `system.c:136-167`（kernel_call）、`system.c:95-128`（kernel_call_dispatch）

### 1.3 VMSUSPEND 挂起-恢复协议

当系统调用 handler 需要访问用户空间内存但该内存尚未映射时，返回 `VMSUSPEND(-996)`：

```
kernel_call_dispatch() → handler → return VMSUSPEND
  ↓
kernel_call_finish():
  - 保存请求消息到 p_vmrequest.saved.reqmsg
  - 设置 MF_KCALL_RESUME 标志
  - 进程保持 RTS_VMREQUEST 状态
  ↓
VM 处理缺页，回复内核
  ↓
switch_to_user() 检查 MF_KCALL_RESUME:
  → kernel_call_resume()
    → 重新执行 handler
    → kernel_call_finish()
```

C 源码: `system.c:58-90`（kernel_call_finish）、`system.c:612-638`（kernel_call_resume）

### 1.4 权限检查

每个进程的 `priv` 结构包含 `s_k_call_mask` 位图，控制该进程可调用的系统调用集合：

```
kernel_call_dispatch():
  call_nr = msg->m_type - KERNEL_CALL
  if call_nr < 0 || call_nr >= NR_SYS_CALLS → EBADREQUEST
  if !GET_BIT(priv(caller)->s_k_call_mask, call_nr) → ECALLDENIED
  call_vec[call_nr](caller, msg)
```

C 源码: `system.c:95-116`

---

## Ch2: C 源码分析

### 2.0 Claims-Evidence

| Claim | Evidence | Status |
|-------|----------|--------|
| call_vec 是函数指针数组 | `system.c:53` | ✅ verified |
| NR_SYS_CALLS = 58 | `com.h:262` | ✅ verified |
| map() 宏注册 handler | `system.c:54-58` | ✅ verified |
| kernel_call_finish 处理 VMSUSPEND | `system.c:58-90` | ✅ verified |
| kernel_call_dispatch 做权限检查 | `system.c:95-128` | ✅ verified |
| kernel_call 做消息拷贝 | `system.c:136-167` | ✅ verified |
| kernel_call_resume 恢复挂起调用 | `system.c:612-638` | ✅ verified |

### 2.1 call_vec 定义与注册

```c
// system.c:53
static int (*call_vec[NR_SYS_CALLS])(struct proc * caller, message *m_ptr);

// system.c:54-58
#define map(call_nr, handler) \
  { int call_index = call_nr-KERNEL_CALL; \
    assert(call_index >= 0 && call_index < NR_SYS_CALLS); \
    call_vec[call_index] = (handler); }
```

### 2.2 完整 syscall 列表

| 编号 | 常量 | Handler | 分类 |
|------|------|---------|------|
| 0 | SYS_FORK | do_fork | 进程管理 |
| 1 | SYS_EXEC | do_exec | 进程管理 |
| 2 | SYS_CLEAR | do_clear | 进程管理 |
| 3 | SYS_SCHEDULE | do_schedule | 调度 |
| 4 | SYS_PRIVCTL | do_privctl | 特权 |
| 5 | SYS_TRACE | do_trace | 调试 |
| 6 | SYS_KILL | do_kill | 信号 |
| 7 | SYS_GETKSIG | do_getksig | 信号 |
| 8 | SYS_ENDKSIG | do_endksig | 信号 |
| 9 | SYS_SIGSEND | do_sigsend | 信号 |
| 10 | SYS_SIGRETURN | do_sigreturn | 信号 |
| 13 | SYS_MEMSET | do_memset | 内存 |
| 14 | SYS_UMAP | do_umap | 内存 |
| 15 | SYS_VIRCOPY | do_vircopy | 内存 |
| 16 | SYS_PHYSCOPY | do_copy | 内存 |
| 17 | SYS_UMAP_REMOTE | do_umap_remote | 内存 |
| 18 | SYS_VUMAP | do_vumap | 内存 |
| 19 | SYS_IRQCTL | do_irqctl | 设备 |
| 21 | SYS_DEVIO | do_devio | 设备(x86) |
| 22 | SYS_SDEVIO | do_sdevio | 设备(x86) |
| 23 | SYS_VDEVIO | do_vdevio | 设备(x86) |
| 24 | SYS_SETALARM | do_setalarm | 时钟 |
| 25 | SYS_TIMES | do_times | 时钟 |
| 26 | SYS_GETINFO | do_getinfo | 信息 |
| 27 | SYS_ABORT | do_abort | 系统 |
| 28 | SYS_IOPENABLE | do_iopenable | 设备(x86) |
| 31 | SYS_SAFECOPYFROM | do_safecopy_from | 内存 |
| 32 | SYS_SAFECOPYTO | do_safecopy_to | 内存 |
| 33 | SYS_VSAFECOPY | do_vsafecopy | 内存 |
| 34 | SYS_SETGRANT | do_setgrant | 内存 |
| 35 | SYS_READBIOS | do_readbios | 设备(x86) |
| 36 | SYS_SPROF | do_sprofile | 调试 |
| 39 | SYS_STIME | do_stime | 时钟 |
| 40 | SYS_SETTIME | do_settime | 时钟 |
| 43 | SYS_VMCTL | do_vmctl | VM |
| 44 | SYS_DIAGCTL | do_diagctl | 诊断 |
| 45 | SYS_VTIMER | do_vtimer | 时钟 |
| 46 | SYS_RUNCTL | do_runctl | 进程管理 |
| 50 | SYS_GETMCONTEXT | do_getmcontext | 上下文 |
| 51 | SYS_SETMCONTEXT | do_setmcontext | 上下文 |
| 52 | SYS_UPDATE | do_update | 进程管理 |
| 53 | SYS_EXIT | do_exit | 进程管理 |
| 54 | SYS_SCHEDCTL | do_schedctl | 调度 |
| 55 | SYS_STATECTL | do_statectl | 进程管理 |
| 56 | SYS_SAFEMEMSET | do_safememset | 内存 |
| 57 | SYS_PADCONF | do_padconf | 设备(ARM) |

C 源码: `system.c:193-268`

### 2.3 kernel_call_dispatch

```c
// system.c:95-128
static int kernel_call_dispatch(struct proc * caller, message *msg)
{
  int result = OK;
  int call_nr = msg->m_type - KERNEL_CALL;

  if (call_nr < 0 || call_nr >= NR_SYS_CALLS) {
    result = EBADREQUEST;
  }
  else if (!GET_BIT(priv(caller)->s_k_call_mask, call_nr)) {
    result = ECALLDENIED;
  } else {
    if (call_vec[call_nr])
      result = (*call_vec[call_nr])(caller, msg);
    else {
      result = EBADREQUEST;
    }
  }
  return result;
}
```

### 2.4 kernel_call_finish

```c
// system.c:58-90
static void kernel_call_finish(struct proc * caller, message *msg, int result)
{
  if(result == VMSUSPEND) {
    assert(RTS_ISSET(caller, RTS_VMREQUEST));
    assert(caller->p_vmrequest.type == VMSTYPE_KERNELCALL);
    caller->p_vmrequest.saved.reqmsg = *msg;
    caller->p_misc_flags |= MF_KCALL_RESUME;
  } else {
    caller->p_vmrequest.saved.reqmsg.m_source = NONE;
    if (result != EDONTREPLY) {
      msg->m_source = SYSTEM;
      msg->m_type = result;
      if (copy_msg_to_user(msg, (message *)caller->p_delivermsg_vir)) {
        cause_sig(proc_nr(caller), SIGSEGV);
      }
    }
  }
}
```

### 2.5 kernel_call

```c
// system.c:136-167
void kernel_call(message *m_user, struct proc * caller)
{
  int result = OK;
  message msg;

  caller->p_delivermsg_vir = (vir_bytes) m_user;
  if (copy_msg_from_user(m_user, &msg) == 0) {
    msg.m_source = caller->p_endpoint;
    result = kernel_call_dispatch(caller, &msg);
  } else {
    cause_sig(proc_nr(caller), SIGSEGV);
    return;
  }

  kbill_kcall = caller;
  kernel_call_finish(caller, &msg, result);
}
```

### 2.6 kernel_call_resume

```c
// system.c:612-638
void kernel_call_resume(struct proc * caller)
{
  int result;
  message msg_copy;
  struct vmrequest *vmr;

  assert(caller->p_misc_flags & MF_KCALL_RESUME);
  assert(RTS_ISSET(caller, RTS_VMREQUEST));
  assert(caller->p_vmrequest.type == VMSTYPE_KERNELCALL);

  vmr = &caller->p_vmrequest;
  msg_copy = vmr->saved.reqmsg;
  caller->p_misc_flags &= ~MF_KCALL_RESUME;

  result = kernel_call_dispatch(caller, &msg_copy);
  kernel_call_finish(caller, &msg_copy, result);
}
```

### 2.7 system_init() — 调用向量初始化

```c
// system.c:168-270
void system_init(void)
{
  // 1. 初始化 IRQ 钩子数组：标记所有钩子为可用
  // 2. 初始化所有特权结构的闹钟定时器
  // 3. 清空调用向量表：call_vec[i] = NULL
  // 4. 通过 map() 宏映射所有已知的内核调用号到处理函数
  // 5. 条件编译：x86 特有的 SYS_DEVIO/SYS_VDEVIO/SYS_READBIOS 等
}
```

### 2.8 设计要点

**消息复制与 TOCTOU 防护**：`kernel_call()` 先将用户空间消息复制到内核栈，处理完后再复制回去。这防止了 TOCTOU 攻击——调用方无法在内核检查参数后、内核使用参数前修改消息内容。代价是两次消息复制（进+出），但这是安全性的必要开销。

**call_vec 函数指针设计**：使用函数指针数组而非 switch-case，优势：(1) 可扩展性——添加新内核调用只需 `map(SYS_XXX, do_xxx)` 一行；(2) O(1) 分发——数组索引直接定位处理函数；(3) 条件编译友好——架构相关的调用可以条件性地 map 或不 map。

**EDONTREPLY 使用场景**：用于不需要立即回复的内核调用。典型场景是 `sys_exit()`——退出的系统进程不需要收到回复。处理函数返回 `EDONTREPLY` 后，`kernel_call_finish()` 不写回返回值，调用方也不会被解除阻塞。

**VMSUSPEND 透明性**：某些内核调用（如 `sys_vircopy`）需要访问调用方的地址空间，但目标页面可能不在物理内存中。VMSUSPEND 机制确保页缺失对调用方是透明的——调用方不知道内核调用被暂停过。

### 2.9 架构差异

| 维度 | x86 | ARM | RISC-V |
|------|-----|-----|--------|
| SYS_DEVIO | ✅ | ❌ | ❌ |
| SYS_SDEVIO | ✅ | ❌ | ❌ |
| SYS_VDEVIO | ✅ | ❌ | ❌ |
| SYS_IOPENABLE | ✅ | ❌ | ❌ |
| SYS_READBIOS | ✅ | ❌ | ❌ |
| SYS_PADCONF | ❌ | ✅ | ❌ |

C 源码: `system.c:215-216`（`#if defined(__i386__)` 条件编译）

---

## Ch3: Rust 设计决策

| 决策 | 选项 | 结论 | 理由 |
|------|------|------|------|
| D1: handler 返回值 | `i32` vs `enum` | `enum KcallResult { Ok(i32), VmSuspend, NoReply, BadCall }` | 类型安全，消除魔术数 |
| D2: call_vec | 函数指针数组 vs `match` | **`match`** | 编译期穷尽检查，无函数指针 |
| D3: syscall 号 | 常量 vs `enum` | `enum Syscall` + `TryFrom<u16>` | 类型安全，无效值编译期排除 |
| D4: VMSUSPEND | 魔术数 vs 类型变体 | `KcallResult::VmSuspend` | 消除 -996 魔术数 |
| D5: 权限检查 | 运行时位图 vs 编译时 | 运行时 `s_k_call_mask` 位图 | 与 C 一致，进程权限动态配置 |
| D6: 架构特定 syscall | `#[cfg]` 条件编译 vs 运行时返回 | 运行时返回 `BadCall` | 避免行为选择泄漏到编译期 |
| D7: 消息拷贝 | 直接 `copy_from_user` vs trait | `trait MessageCopier` | 抽象用户空间访问 |

### D1: KcallResult 类型

C 的 `kernel_call_dispatch` 返回 `int`，用魔术数区分 OK/VMSUSPEND/EDONTREPLY/EBADREQUEST。Rust 用 `enum KcallResult`：

```rust
pub enum KcallResult {
    Ok(i32),        // result >= 0 或 result == OK
    VmSuspend,      // VMSUSPEND = -996
    NoReply,        // EDONTREPLY
    BadCall,        // EBADREQUEST (212) — 无效 syscall 号
    CallDenied,     // ECALLDENIED (210) — s_k_call_mask 权限拒绝
}
```

### D2: match 替代 call_vec

C 使用 `call_vec[NR_SYS_CALLS]` 函数指针数组 + `map()` 宏注册。Rust 使用 `enum Syscall` + `match`：

- 编译期穷尽检查：新增 syscall 必须在 match 中处理
- 无函数指针：消除间接调用开销和安全风险
- 无 `map()` 宏：不需要运行时 assert

### D6: 架构特定 syscall

C 使用 `#if defined(__i386__)` 条件编译。Rust 将所有 syscall 编入 `enum Syscall`，在 dispatch 时对不支持的架构返回 `BadCall`：

```rust
#[cfg(not(target_arch = "x86_64"))]
fn dispatch_arch_devio(_: &mut KProcess, _: &Message) -> KcallResult {
    KcallResult::BadCall
}
```

这避免了 `#[cfg(target_arch)]` 选择行为（违反硬件抽象原则），同时保持 API 一致性。

---

## Ch4: 实现

### 4.1 Syscall 枚举

```rust
// syscall.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Syscall {
    Fork = 0,
    Exec = 1,
    // ... 全部 58 个变体
    Padconf = 57,
}

impl TryFrom<u16> for Syscall {
    type Error = ();
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Syscall::Fork),
            // ...
            _ => Err(()),
        }
    }
}
```

### 4.2 KcallResult

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KcallResult {
    Ok(i32),
    VmSuspend,
    NoReply,
    BadCall,
    CallDenied,  // C: ECALLDENIED (210) — s_k_call_mask 权限拒绝
}
```

### 4.3 kernel_call_dispatch

```rust
pub fn kernel_call_dispatch(
    caller: &mut KProcess,
    msg: &Message,
    priv_table: &PrivTable,
) -> KcallResult {
    let call_nr = msg.m_type as u16;
    let syscall = match Syscall::try_from(call_nr) {
        Ok(s) => s,
        Err(()) => return KcallResult::BadCall,
    };

    // C: `else if (!GET_BIT(priv(caller)->s_k_call_mask, call_nr))` — system.c:107
    // 检查调用方是否有权调用此系统调用。
    // 未分配权限（priv_id == None）的进程被拒绝所有内核调用。
    let call_denied = match caller.priv_id {
        Some(priv_id) => {
            match priv_table.get(priv_id) {
                Some(caller_priv) => !kcall_filter_check(caller_priv, call_nr as u32),
                None => true, // 无效 priv_id — 拒绝
            }
        }
        None => true, // 无权限分配 — 拒绝
    };
    if call_denied {
        return KcallResult::CallDenied;
    }

    match syscall {
        Syscall::Fork => dispatch_fork(caller, msg),
        Syscall::Exec => dispatch_exec(caller, msg),
        // ... 穷尽匹配
    }
}
```

**设计决策 D5 实现**：`s_k_call_mask` 在 dispatch 入口处作为运行时位图检查，与 C 行为一致。
`kcall_filter_check()` 定义在 `ipc_filter.rs`（见 22-ipc-filter.md §4.1），将 `[u32; 2]` 合并为 `u64` 后按位测试。
`KProcess::priv_id` 字段存储 `PrivId`（即 `u16`），对应 C 的 `p_priv` 指针，通过 `PrivTable::get()` 查找。

### 4.4 kernel_call_finish

```rust
pub fn kernel_call_finish(caller: &mut KProcess, msg: &Message, result: KcallResult) {
    match result {
        KcallResult::VmSuspend => {
            // 保存请求消息
            caller.p_vmrequest.saved.reqmsg = msg.clone();
            caller.p_misc_flags.set(MiscFlagsBits::KCALL_RESUME);
        }
        KcallResult::Ok(ret) => {
            // 清除保存的消息
            caller.p_vmrequest.saved.reqmsg.m_source = Endpoint::NONE;
            // 拷贝结果到用户空间
            let reply = Message { m_source: Endpoint::SYSTEM, m_type: ret, ..msg.clone() };
            // TODO: copy_msg_to_user
        }
        KcallResult::NoReply => {
            caller.p_vmrequest.saved.reqmsg.m_source = Endpoint::NONE;
        }
        KcallResult::BadCall => {
            // 返回错误码 EBADREQUEST (212)
        }
        KcallResult::CallDenied => {
            // 返回错误码 ECALLDENIED (210) — system.c:108
        }
    }
}
```

### 4.5 kernel_call_resume

```rust
pub fn kernel_call_resume(caller: &mut KProcess, priv_table: &PrivTable) {
    assert!(caller.p_misc_flags.is_set(MiscFlagsBits::KCALL_RESUME));

    let msg_copy = caller.p_vmrequest.saved.reqmsg.clone();
    caller.p_misc_flags.clear(MiscFlagsBits::KCALL_RESUME);

    let result = kernel_call_dispatch(caller, &msg_copy, priv_table);
    kernel_call_finish(caller, &msg_copy, result);
}
```

---

## Ch5: 测试

### 5.1 Syscall 号映射

| 测试 | 验证 |
|------|------|
| `Syscall::try_from(0) == Ok(Syscall::Fork)` | Fork 编号正确 |
| `Syscall::try_from(11) == Err(())` | 未使用编号返回错误 |
| `Syscall::try_from(58) == Err(())` | 超出范围返回错误 |
| `Syscall::Fork as u16 == 0` | 枚举值与 C 一致 |

### 5.2 Dispatch 行为

| 测试 | 验证 |
|------|------|
| 无效 syscall 号 → `BadCall` | `kernel_call_dispatch` 对无效号返回 BadCall |
| 权限拒绝 → `CallDenied` | `s_k_call_mask` 未设置的 syscall 返回 CallDenied |
| 无 priv_id → `CallDenied` | 未分配权限的进程被拒绝所有内核调用 |
| 有效 syscall → 对应 handler | dispatch 正确路由 |

### 5.3 VMSUSPEND 协议

| 测试 | 验证 |
|------|------|
| handler 返回 VmSuspend → MF_KCALL_RESUME 被设置 | kernel_call_finish 正确处理 |
| resume 重新 dispatch | kernel_call_resume 重新执行 handler |
| resume 后清除 MF_KCALL_RESUME | 标志正确清除 |

### 5.4 架构特定 syscall

| 测试 | 验证 |
|------|------|
| x86 特定 syscall 在非 x86 → BadCall | 运行时返回错误 |
| ARM 特定 syscall 在非 ARM → BadCall | 运行时返回错误 |

---

## 参考文献

1. `minix3/minix/kernel/system.c:52-167` — call_vec, kernel_call_dispatch, kernel_call_finish, kernel_call
2. `minix3/minix/kernel/system.c:612-638` — kernel_call_resume
3. `minix3/minix/include/minix/com.h:207-262` — SYS_* 常量定义
4. `minix3/minix/include/minix/com.h:262` — NR_SYS_CALLS = 58
5. `os/kernel/src/syscall.rs` — Rust 实现
