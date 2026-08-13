# 23-ipc-filter: IPC 过滤

> **分类**: 运行时基础设施
> **源码**: `minix3/minix/kernel/const.h`, `minix3/minix/kernel/priv.h`, `minix3/minix/kernel/ipc.h`, `minix3/minix/kernel/ipc_filter.h`, `minix3/minix/include/minix/ipc_filter.h`, `minix3/minix/kernel/system.c`
> **前置**: 22（权限结构——`s_ipc_to`/`s_k_call_mask`/`s_ipcf` 字段定义）, 12（IPC 原语——send/receive/notify 调用过滤入口）, 13（系统调用分派——`kernel_call_dispatch` 过滤接入点）
> **关联 Rust**: `os/kernel/src/ipc_filter.rs`, `os/kernel/src/kpriv.rs`, `os/kernel/src/syscall.rs`

---

## Ch1: 概念

**核心问题**: 内核如何在一条 IPC 消息送达前，基于发送方权限和接收方偏好做出"放行/拒绝"决策？

IPC 过滤是 Minix3 最小特权原则的执行层。22-privilege.md 定义了权限**结构**（`s_ipc_to`/`s_k_call_mask`/`s_ipcf` 字段）；本章回答这些字段在 IPC 路径上**何时、如何**被检查。CPU 在每次 trap 进入内核后会面对三类决策：调用方能否发起这个系统调用？调用方能否向这个目标发送？接收方是否愿意接收这条消息？三个决策对应三层过滤模型。

### 1.1 三层过滤模型

| 层 | 名称 | 数据结构 | 回答的问题 | 时机 | C 位置 |
|----|------|---------|-----------|------|--------|
| **L1** | 粗粒度位图 | `s_ipc_to` / `s_k_call_mask` | 调用方能否向此目标/发起此调用？ | send/notify/asyncsend/kcall 入口 | priv.h:35,38,86,87；system.c:111 |
| **L2** | 细粒度过滤链 | `s_ipcf` → `ipc_filter_s` 链 | 接收方是否愿意接收此 (src,m_type)？ | receive 反向过滤 | ipc.h:14-22；system.c:803-874 |
| **L3** | 状态报告 | `p_reg.IPC_STATUS_REG` | 过滤结果如何告知用户态？ | receive 完成时 | ipc.h:25-48 |

**L1 是发送方视角的强制检查**——内核代表发送方查 `s_ipc_to` 位图决定能否发出。**L2 是接收方视角的偏好检查**——内核代表接收方查 `s_ipcf` 过滤链决定是否接收。两者方向相反：L1 拦截"不该发出的"，L2 拦截"不想收到的"。

**L1 与 L2 的分工动机**：位图（L1）是 O(1) 查表，适合在热路径上对每个 send 都检查；过滤链（L2）需遍历元素列表，仅在 receive 路径且接收方设置了 `s_ipcf` 时才触发。若把 m_source/m_type 细粒度匹配也放进 send 路径，每次 send 都要遍历目标进程的过滤链，代价过高。

### 1.2 过滤时机

| 操作 | L1 检查 | L2 检查 | C 入口 |
|------|---------|---------|--------|
| 内核系统调用 | `s_k_call_mask` | — | system.c:111 `kernel_call_dispatch` |
| send | `s_ipc_to`（`may_send_to`） | — | proc.c `mini_send` |
| notify | `s_ipc_to`（`may_send_to`） | — | proc.c `sys_notify` |
| asyncsend | `s_ipc_to`（`may_asynsend_to`） | — | proc.c `mini_senda` |
| receive | — | `s_ipcf`（`CANRECEIVE`→`allow_ipc_filtered_msg`） | ipc.h:19-22 |

**关键不对称**：send 路径只查 L1，receive 路径只查 L2。这是因为 L1 的位图是发送方自己的权限（"我能不能发"），L2 的过滤链是接收方自己的偏好（"我想不想收"）——两者归属不同主体，自然在不同时机检查。

### 1.3 `may_send_to` 与 `may_asynsend_to` 的不对称

```c
// priv.h:86-87
#define may_send_to(rp, nr)        (get_sys_bit(priv(rp)->s_ipc_to, nr_to_id(nr)))
#define may_asynsend_to(rp, nr)    (may_send_to(rp, nr) || (rp)->p_nr == nr)
```

`may_asynsend_to` 比 `may_send_to` 多一个 `|| (rp)->p_nr == nr` 分支——**异步 IPC 允许发送给自己**。设计理由：异步消息表（asynmsg_t table）的扫描模型下，self-send 是合法的"延迟自通知"模式（进程向自己的异步表写入待处理消息，后续 receive 时取出）。同步 send 不允许 self-send 是因为会立即死锁（双方互等），但 asyncsend 不阻塞，无死锁风险。

### 1.4 为何不存在 `CHECK_IPC` 标志

Minix3 权限模型有 `CHECK_IO_PORT`/`CHECK_IRQ`/`CHECK_MEM` 三类可选检查标志（按需启用 I/O 端口/IRQ/内存范围检查），但**不存在 `CHECK_IPC`**。原因：IPC 目标过滤（`s_ipc_to`）是**无条件**执行的——`may_send_to` 宏始终查位图，不依赖任何运行时标志。这是设计取舍：IPC 是微内核的核心通信原语，所有系统进程都必须受位图约束，没有"可选关闭"的语义空间；而 I/O 端口等检查对非驱动进程无意义，用标志位按需启用可节省检查开销。

### 1.5 redox 对照（世界知识）

| 维度 | Minix3 | redox |
|------|--------|-------|
| 模型 | 位图（`s_ipc_to` 按 sys_id 索引）+ 过滤链（`s_ipcf` 按 m_source/m_type） | capability token（scheme 持有 capability，client 持有 capability 描述符） |
| 粒度 | 系统进程级（sys_id 0..64） | 资源级（每个 fd/handle 是独立 capability） |
| 可委托 | 否（位图由 RS 在 privctl 时配置，运行时不可传递） | 是（capability 可 fork/dup 传递） |
| 查找复杂度 | O(1) 位测试（L1）/ O(n) 链遍历（L2） | O(1) 描述符表查找 |
| 审计 | 位图集中可枚举所有授权 | capability 分散在进程 fd 表 |

**minix-rs 立场**：保留 Minix3 位图+过滤链模型（本章职责），不引入 redox capability 模型——这是 22-privilege.md 已确定的重写边界。但借鉴 redox 的"非法状态不可表达"原则：Rust 实现用 `Option`/`enum`/`bitflags` 替代 C 的哨兵值和裸整数（见 Ch3 D5-D8）。

### 1.6 本章不讲什么

- 权限字段定义与 privctl 运行时配置入口 → 见 [22-privilege.md](22-privilege.md) 与 [17-syscall-process.md](17-syscall-process.md)
- IPC 原语（send/receive/notify）的完整语义与阻塞逻辑 → 见 [12-ipc-core.md](12-ipc-core.md)
- 系统调用分派的完整流程 → 见 [13-syscall-dispatch.md](13-syscall-dispatch.md)
- 异步 IPC 消息表与 `s_asyn_pending` 位图 → 见 [12-ipc-core.md](12-ipc-core.md)（异步 IPC 专章）

---

## Ch2: C 源码分析

### 2.1 文件清单与职责

| 文件 | 行数 | 职责 | 本章关注点 |
|------|------|------|-----------|
| `minix3/minix/kernel/const.h` | 53 | 通用宏 | L20-27 `get_sys_bit/set_sys_bit/unset_sys_bit` 位图原语 |
| `minix3/minix/kernel/priv.h` | 105 | `struct priv` 定义 | L35 `s_ipc_to`，L38 `s_k_call_mask`，L46 `s_ipcf`，L84 `nr_to_id`，L86 `may_send_to`，L87 `may_asynsend_to` |
| `minix3/minix/kernel/ipc.h` | 50 | IPC 宏 | L14-17 `WILLRECEIVE`，L19-22 `CANRECEIVE`，L25-48 `IPC_STATUS_*` |
| `minix3/minix/kernel/ipc_filter.h` | 73 | 过滤池与过滤链 | L13-15 `IPCF_NONE/BLACKLIST/WHITELIST`，L19-41 `IPCF_EL_*` 匹配宏，L43-50 `struct ipc_filter_s`，L53 `IPCF_POOL_SIZE`，L57-71 池操作宏 |
| `minix3/minix/include/minix/ipc_filter.h` | 30 | 过滤元素定义 | L10-12 `ANY_USR/SYS/TSK` 特殊端点，L15 `IPCF_MAX_ELEMENTS`，L18-21 `IPCF_MATCH_*` 标志，L23-27 `struct ipc_filter_el_s` |
| `minix3/minix/kernel/system.c` | — | 系统调用分派 | L95-127 `kernel_call_dispatch`（L111 k_call_mask 检查），L803-874 `allow_ipc_filtered_msg` |

### 2.2 位图原语（const.h:20-27）

```c
#define get_sys_bit(map,bit)    ( MAP_CHUNK((map).chunk,bit) & (1 << CHUNK_OFFSET(bit)) )
#define set_sys_bit(map,bit)    ( MAP_CHUNK((map).chunk,bit) |= (1 << CHUNK_OFFSET(bit)) )
#define unset_sys_bit(map,bit)  ( MAP_CHUNK((map).chunk,bit) &= ~(1 << CHUNK_OFFSET(bit)) )
```

`sys_map_t` 是 `struct { bitchunk_t chunk[SYS_MAP_CHUNKS]; }`，`MAP_CHUNK` 选中 bit 所在的 `bitchunk_t`（32 位），`CHUNK_OFFSET` 取 bit 在 chunk 内的偏移。本质是跨多 chunk 的位数组操作宏。Minix3 中 `NR_SYS_PROCS ≤ 64`，故 `s_ipc_to` 实际用 2 个 `bitchunk_t`（64 位）足以覆盖所有 sys_id。

### 2.3 L1 过滤：`may_send_to` / `may_asynsend_to`（priv.h:86-87）

```c
#define may_send_to(rp, nr)        (get_sys_bit(priv(rp)->s_ipc_to, nr_to_id(nr)))
#define may_asynsend_to(rp, nr)    (may_send_to(rp, nr) || (rp)->p_nr == nr)
```

`nr_to_id(nr)`（priv.h:84）将进程号映射到其 priv 结构的 `s_id`。`may_send_to` 查发送方的 `s_ipc_to` 位图中目标 `s_id` 对应的位。`may_asynsend_to` 额外允许 self-send（见 §1.3）。

### 2.4 L1 过滤：`s_k_call_mask` 检查（system.c:111）

```c
// system.c:95-127 kernel_call_dispatch
call_nr = msg->m_type - KERNEL_CALL;
if (call_nr < 0 || call_nr >= NR_SYS_CALLS) {
    result = EBADREQUEST;
}
else if (!GET_BIT(priv(caller)->s_k_call_mask, call_nr)) {
    result = ECALLDENIED;           // errno.h:206
} else {
    result = (*call_vec[call_nr])(caller, msg);
}
```

`GET_BIT` 是独立宏（bitmap.h:16）——展开结构（`MAP_CHUNK(map,bit) & (1 << CHUNK_OFFSET(bit))`）与 `get_sys_bit`（const.h:20，作用于 `sys_map_t` 结构体）相似，但参数类型不同（裸 `bitchunk_t` 数组），并非别名。`s_k_call_mask` 是 `bitchunk_t[SYS_CALL_MASK_SIZE]`（priv.h:38），`NR_SYS_CALLS=58`（com.h:270），故 `SYS_CALL_MASK_SIZE=2`（2×32=64 ≥ 58）。检查顺序：先验 call_nr 范围（EBADREQUEST），再查位图（ECALLDENIED）。

### 2.5 L2 过滤：`CANRECEIVE` / `WILLRECEIVE`（ipc.h:14-22）

```c
#define WILLRECEIVE(src_e,dst_ptr,m_src_v,m_src_p) \
    ((RTS_ISSET(dst_ptr, RTS_RECEIVING) && !RTS_ISSET(dst_ptr, RTS_SENDING)) && \
     CANRECEIVE(dst_ptr->p_getfrom_e, src_e, dst_ptr, m_src_v, m_src_p))

#define CANRECEIVE(receive_e, src_e, dst_ptr, m_src_v, m_src_p) \
    (((receive_e) == ANY || (receive_e) == (src_e)) && \
     (priv(dst_ptr)->s_ipcf == NULL || \
      allow_ipc_filtered_msg(dst_ptr, src_e, m_src_v, m_src_p)))
```

`CANRECEIVE` 是 receive 路径的反向过滤入口：接收方若未设 `s_ipcf`（NULL），无条件接收；否则调用 `allow_ipc_filtered_msg` 按 (src_e, m_type) 匹配过滤链。`WILLRECEIVE` 在 `CANRECEIVE` 基础上额外要求接收方处于 `RTS_RECEIVING` 且未在 `RTS_SENDING`——即"正在阻塞等收"。

### 2.6 L2 过滤：`allow_ipc_filtered_msg`（system.c:803-874）

```c
int allow_ipc_filtered_msg(struct proc *rp, endpoint_t src_e,
    vir_bytes m_src_v, message *m_src_p)
{
    ipc_filter_t *ipcf = priv(rp)->s_ipcf;
    if (ipcf == NULL) return TRUE;              // L812: 无过滤链，放行

    if (m_src_p == NULL) {                       // L815: 需要从源进程拷贝 m_type
        get_mtype = FALSE;
        do {
            if (ipcf->flags & IPCF_MATCH_M_TYPE) { get_mtype = TRUE; break; }
            ipcf = ipcf->next;
        } while (ipcf);
        ipcf = priv(rp)->s_ipcf;                 // L831: 重置到链首
        if (get_mtype) {
            r = data_copy(src_e, m_src_v + offsetof(message, m_type), ...);
            if (r != OK) return TRUE;            // L844: 拷贝失败则放行（后续会失败）
        }
        m_src_p = &m_buff;
    }
    m_src_p->m_source = src_e;

    allow = (ipcf->type == IPCF_BLACKLIST);      // L853: blacklist 默认 allow
    do {
        if (allow != (ipcf->type == IPCF_WHITELIST)) {  // 仅当当前过滤器的决策与"默认"相反时才需检查
            for (i = 0; i < ipcf->num_elements; i++) {
                if (IPCF_EL_MATCH(&ipcf->elements[i], m_src_p)) {
                    allow = (ipcf->type == IPCF_WHITELIST);  // 命中后翻转
                    break;
                }
            }
        }
        ipcf = ipcf->next;
    } while (ipcf);
    return allow;
}
```

**算法要点**：
1. **blacklist 默认 allow，命中翻转 false**：blacklist 列出"拒绝列表"，不在列表中的消息放行。
2. **whitelist 默认 deny，命中翻转 true**：whitelist 列出"允许列表"，仅在列表中的消息放行。
3. **链式组合**：多个过滤器链接，每个过滤器独立决策，后续过滤器可覆盖前面的决策。链遍历用 `ipcf = ipcf->next`，到 NULL 结束。
4. **延迟拷贝 m_type**：仅当过滤链中任一元素设置了 `IPCF_MATCH_M_TYPE` 标志时，才从源进程地址空间拷贝 m_type 字段（避免无谓拷贝）。

### 2.7 过滤元素匹配：`IPCF_EL_MATCH` + 辅助宏（ipc_filter.h:19-41）

`IPCF_EL_MATCH` 是单元素匹配的顶层宏，组合 m_type 与 m_source 两个维度的匹配结果。其底层依赖 5 个辅助宏完成端点类型判断与标志位校验：

```c
#define IPCF_EL_MATCH(E,M) \
    (IPCF_EL_MATCH_M_TYPE(E,M) && IPCF_EL_MATCH_M_SOURCE(E,M))

#define IPCF_EL_MATCH_M_TYPE(E,M) \
    (!((E)->flags & IPCF_MATCH_M_TYPE) || (E)->m_type == (M)->m_type)

#define IPCF_EL_MATCH_M_SOURCE(E,M) \
    (!((E)->flags & IPCF_MATCH_M_SOURCE) || \
     (E)->m_source == (M)->m_source || \
     IPCF_EL_MATCH_M_SOURCE_ANY_EP((E)->m_source, (M)->m_source))

#define IPCF_EL_MATCH_M_SOURCE_ANY_EP(ES,MS) \
    (((ES) == ANY_USR && IPCF_IS_USR_EP(MS)) || \
     ((ES) == ANY_SYS && IPCF_IS_SYS_EP(MS)) || \
     ((ES) == ANY_TSK && IPCF_IS_TSK_EP(MS)))
```

**端点类型判断辅助宏**（ipc_filter.h:24-28，被 `IPCF_EL_MATCH_M_SOURCE_ANY_EP` 调用）：

```c
#define IPCF_IS_USR_EP(EP)  (!(priv(proc_addr(_ENDPOINT_P((EP))))->s_flags & SYS_PROC))
#define IPCF_IS_TSK_EP(EP)  (iskerneln(_ENDPOINT_P((EP))))
#define IPCF_IS_SYS_EP(EP)  (!IPCF_IS_USR_EP(EP) && !IPCF_IS_TSK_EP(EP))
#define IPCF_IS_ANY_EP(EP)  ((EP) == ANY_USR || (EP) == ANY_SYS || (EP) == ANY_TSK)
```

- `IPCF_IS_USR_EP`：通过查 priv 表的 `SYS_PROC` 标志判断是否为用户进程（无 SYS_PROC 即用户态）。
- `IPCF_IS_TSK_EP`：通过 `iskerneln()` 判断端点号是否为内核任务（proc_nr < 0）。
- `IPCF_IS_SYS_EP`：排除用户和任务后即为系统服务进程。
- `IPCF_IS_ANY_EP`：判断是否为三个通配端点之一（用于运行时检查，不参与匹配逻辑本身）。

**元素有效性校验宏**（ipc_filter.h:19-23）：

```c
#define IPCF_EL_CHECK(E) \
    ((((E)->flags & IPCF_MATCH_M_TYPE) || \
      ((E)->flags & IPCF_MATCH_M_SOURCE)) && \
     (!(((E)->flags & IPCF_MATCH_M_SOURCE)) || \
      IPCF_IS_ANY_EP((E)->m_source) || isokendpt((E)->m_source, &_ipcf_nr)))
```

`IPCF_EL_CHECK` 在元素被加入过滤链前校验：至少有一个匹配维度被启用（避免无意义元素），且若启用 m_source 匹配则 m_source 必须是合法端点或通配端点。`_ipcf_nr` 是 `EXTERN int`（ipc_filter.h:18）的临时变量，供 `isokendpt` 写入。

**匹配逻辑**：
- 若 `flags` 未设 `IPCF_MATCH_M_TYPE`，则 m_type 不参与匹配（视为匹配）；否则要求 `m_type` 相等。
- 若 `flags` 未设 `IPCF_MATCH_M_SOURCE`，则 m_source 不参与匹配；否则要求 `m_source` 相等或命中 `ANY_USR/SYS/TSK` 通配端点。
- `ANY_USR` 匹配任意用户进程端点，`ANY_SYS` 匹配任意系统进程，`ANY_TSK` 匹配任意内核任务（include/minix/ipc_filter.h:10-12）。

### 2.8 L3 状态报告：`IPC_STATUS_*`（ipc.h:25-48）

```c
#define IPC_STATUS_GET(p)        ((p)->p_reg.IPC_STATUS_REG)
#define IPC_STATUS_CLEAR(p)      ((p)->p_reg.IPC_STATUS_REG = 0)
#define IPC_STATUS_ADD(p, m)     do { \
    if(!((p)->p_misc_flags & MF_REPLY_PEND)) { \
        (p)->p_reg.IPC_STATUS_REG |= (m); \
    } \
} while(0)
#define IPC_STATUS_ADD_CALL(p, call)  IPC_STATUS_ADD(p, IPC_STATUS_CALL_TO(call))
#define IPC_STATUS_ADD_FLAGS(p, flags) IPC_STATUS_ADD(p, IPC_STATUS_FLAGS(flags))
```

`IPC_STATUS_REG` 是接收进程寄存器中的一个字段，记录本次 RECEIVE 的过滤结果（哪个调用被过滤、哪些标志被设置）。`MF_REPLY_PEND` 检查避免在 SENDREC（原子 send+receive）的 send 阶段误设状态码——注释（ipc.h:28-39）说明 SENDREC 在 Posix 信号处理下非原子，上下文可能切换。

### 2.9 行为契约表（5 函数 × 8 字段，Gate B 依据）

| 函数 | C 行为 | Rust 行为 | 差异类型 | 严重度 | C 证据 | Rust 证据 | 备注 |
|------|--------|----------|---------|--------|--------|----------|------|
| `may_send_to` | `get_sys_bit(s_ipc_to, nr_to_id(nr))` 查位图 | `caller_priv.may_send_to(target_sys_id)` 委派 `s_ipc_to` u64 位测试 | 一致 | — | priv.h:86 | kpriv.rs:387 | D3 u64 统一 |
| `may_asynsend_to` | `may_send_to(rp,nr) \|\| rp->p_nr == nr` 允许 self-send | 未实现（当前用 `may_send_to` 替代） | 覆盖缺口 | P1 | priv.h:87 | — | 异步 IPC 允许 self-send |
| `GET_BIT(s_k_call_mask, call_nr)` | 位图查 call_nr 是否允许 | `kcall_filter_check(caller_priv, call_nr)` u64 位测试 | 一致 | — | system.c:111 | ipc_filter.rs:72-80 | D3 u64 统一 |
| `allow_ipc_filtered_msg` | 遍历 s_ipcf 过滤链，按 m_source/m_type 匹配，blacklist 默认 allow | 未实现 | 覆盖缺口 | P1 | system.c:803-874 | — | 细粒度过滤 |
| `IPCF_POOL_ALLOCATE_SLOT` | 扫描池找 `type==IPCF_NONE` 槽位 | `IpcFilterPool::allocate` 找 `None` 槽位 | 一致（语义等价） | — | ipc_filter.h:59-70 | ipc_filter.rs:222-230 | D5 Option 替代哨兵 |

---

## Ch3: Rust 设计决策

9 项 hypothesis-driven 决策，每项给出选项、结论、理由、C 对齐。

### D1: 位图操作 — 内联函数 vs 宏

- **选项 A**：宏（C 风格 `get_sys_bit(map,bit)`）
- **选项 B**：内联函数 `fn get_sys_bit(map: u64, id: u16) -> bool`
- **结论**：**B 内联函数**
- **理由**：C 用宏是因无类型系统约束；Rust 内联函数获得类型安全（`u64` 区分 map，`u16` 区分 index），编译期仍内联（`#[inline]`）。宏版本在 Rust 中需 `unsafe` transmute 或 `macro_rules!`，丧失类型检查。
- **C 对齐**：const.h:20-27 `get_sys_bit/set_sys_bit/unset_sys_bit` 语义保持。
- **实现**：ipc_filter.rs:86-111。

### D2: 过滤函数位置 — 独立函数 vs 内联

- **选项 A**：内联到调用点（`mini_send` / `kernel_call_dispatch` 内直接展开）
- **选项 B**：独立函数 `fn ipc_filter_check / kcall_filter_check`
- **结论**：**B 独立函数**
- **理由**：单一职责 + 可测试性。C `may_send_to` 是宏自动内联，但 Rust 语义下独立函数 + `#[inline]` 仍可内联且可独立单元测试（9 个测试见 Ch5）。
- **C 对齐**：priv.h:86 `may_send_to` + system.c:111 `GET_BIT(s_k_call_mask)` 语义保持。
- **实现**：ipc_filter.rs:59-80。

### D3: `s_k_call_mask` 类型 — u64 vs `[u32; 2]`

- **选项 A**：`[u32; SYS_CALL_MASK_SIZE]`（C 直译）
- **选项 B**：`u64` 单字段
- **结论**：**B `u64`（读取层）**，但字段存储仍保留 `[u32; 2]` 对齐 C 布局
- **理由**：Minix3 `NR_SYS_CALLS=58`，1 个 u64（64 位）足够。对齐 22-privilege IpcMask newtype 设计。C 用 `[u32; 2]` 是因 `bitchunk_t` 为 32 位。
- **C 对齐**：priv.h:38 `s_k_call_mask[SYS_CALL_MASK_SIZE]` → 读取时组合为 u64（语义等价）。
- **实现妥协**：`kcall_filter_check` 内部按 `[u32; 2]` 读字段（`caller_priv.ipc.s_k_call_mask[0/1]`）再组合为 u64，未来可统一到 IpcMask newtype。这是 anti-translate 的部分妥协——字段布局对齐 C 便于 privctl 序列化，读取层用 u64 简化位运算。

### D4: 过滤失败返回 — EPERM vs panic

- **选项 A**：panic（违反不变量）
- **选项 B**：返回 `false`，由调用方返回 EPERM/ECALLDENIED
- **结论**：**B 返回 false + 调用方 ECALLDENIED**
- **理由**：过滤失败是正常路径（恶意/越权调用），非不变量违反。C system.c:111-114 返回 `ECALLDENIED`（errno.h:206）而非 panic。
- **C 对齐**：system.c:111-114 `ECALLDENIED` 语义。Rust `KcallResult::CallDenied`（syscall.rs:209）映射到 `ECALLDENIED`。
- **实现**：ipc_filter.rs 返回 bool；syscall.rs:458-459 转换为 `KcallResult::CallDenied`。

### D5: 过滤池空闲槽 — Option vs `type==IPCF_NONE`

- **选项 A**：`type: IpcFilterType` 字段 + `IPCF_NONE` 哨兵表示空闲（C 直译）
- **选项 B**：`Option<IpcFilterSlot>`，`None` = 空闲
- **结论**：**B Option**
- **理由**：Rust "illegal states unrepresentable" 原则。C `IPCF_NONE` 是哨兵值（模式 17），用 `type` 字段同时表达"是否分配"和"分配后的类型"是状态混用——若代码忘记检查 `type==IPCF_NONE` 就读 `elements`，会读到未初始化数据。Option 强制调用方处理 None 分支，编译期消除此类 bug。
- **C 对齐**：ipc_filter.h:13 `IPCF_NONE` / ipc_filter.h:58 `IPCF_POOL_IS_FREE_SLOT` 语义保持（NONE 迁移到 Option）。
- **实现**：ipc_filter.rs:199-201 `slots: [Option<IpcFilterSlot>; IPCF_POOL_SIZE]`。

### D6: 过滤链 next — `Option<usize>` vs 裸指针

- **选项 A**：`*mut ipc_filter_s` 裸指针（C 直译）
- **选项 B**：`Option<usize>` 池内索引
- **结论**：**B Option<usize>**
- **理由**：避免 `unsafe` + 索引边界检查。C 用裸指针是因无所有权概念，释放后 `next` 仍指向已释放槽位是潜在 bug（use-after-free）。Rust 用池内索引：`free` 时 `slot = None` 自动断开链，悬垂索引在 `get(index)` 时返回 `None` 而非 UB。
- **C 对齐**：ipc_filter.h:47 `struct ipc_filter_s *next` → `Option<usize>`（语义等价）。
- **实现**：ipc_filter.rs:167 `pub next: Option<usize>`。

### D7: `IPCF_MATCH_M_SOURCE/M_TYPE` — bitflags vs 裸 u32

- **选项 A**：`flags: u32` 裸整数（C 直译，用 `&` 位运算）
- **选项 B**：`bitflags! struct IpcFilterElFlags: u32`
- **结论**：**B bitflags**（当前用 const 常量过渡，未来迁移到 bitflags 宏）
- **理由**：类型安全 + 可组合（`MATCH_M_SOURCE | MATCH_M_TYPE`）。C 裸 int 用 `&` 位运算易写错（`flags & IPCF_MATCH_M_TYPE` 漏 `&` 不报错），Rust bitflags 提供编译期检查 + `contains`/`insert` 等 API。
- **C 对齐**：include/minix/ipc_filter.h:18-19 `IPCF_MATCH_M_SOURCE/M_TYPE` 语义保持。
- **实现妥协**：当前用 `IpcFilterElFlags` struct + `const MATCH_M_SOURCE: u32 = 0x1`（ipc_filter.rs:124-130），未引入 `bitflags!` 宏（避免新依赖）。语义等价，未来可平滑迁移到 bitflags。

### D8: `filter_type` — enum vs int

- **选项 A**：`type: int`（C 直译，0/1/2 表示 NONE/BLACKLIST/WHITELIST）
- **选项 B**：`enum IpcFilterType { Blacklist, Whitelist }`（NONE 由 Option 表达，见 D5）
- **结论**：**B enum**
- **理由**：穷尽匹配 + 编译器检查。C `IPCF_NONE=0` 用 int 表示，但 Rust 中 `Option<IpcFilterSlot>` 已表达 NONE，enum 只需 Blacklist/Whitelist 两个变体。未来新增类型（如 rate-limit）时 match 会编译报错提示补全分支。
- **C 对齐**：ipc_filter.h:13-15 `IPCF_NONE/BLACKLIST/WHITELIST` 语义保持（NONE 迁移到 Option）。
- **实现**：ipc_filter.rs:117-120 `enum IpcFilterType { Blacklist, Whitelist }`。

### D9: IPC_STATUS 机制 — ✅ IMPLEMENTED (P9-2, 2026-08-13)

- **选项 A**：实现 `IPC_STATUS_ADD/ADD_CALL/ADD_FLAGS` 宏语义
- **选项 B**：DEFERRED，标 TODO
- **结论**：**A IMPLEMENTED**（原 DEFERRED，P9-2 落地）
- **理由**：IPC_STATUS 在 RECEIVE 完成时设置状态码到 `p_reg.IPC_STATUS_REG`，原标注 DEFERRED 因 Rust RECEIVE 路径未完整实现。P9-2 落地后 RECEIVE 路径已接通，IPC_STATUS 有消费方，故实现。诚实标注优于假装实现。
- **实现路径**：
  - `CpuContextArch::or_ipc_status_reg(ctx, value)` trait 方法 + 三架构 impl（[arm64/boot.rs:161](file:///home/xzhao/github/minix-rs/os/arch/src/arm64/boot.rs)、[riscv64/boot.rs:146](file:///home/xzhao/github/minix-rs/os/arch/src/riscv64/boot.rs)、x86_64 同）
  - `proc.rs:1654-1679` 实现 `ipc_status_add_call` / `ipc_status_add_flags` 两个 helper（C 的 `IPC_STATUS_ADD` 内联进两者，无独立 `ipc_status_add`）
  - `ipc.rs` 4 路径 wire：SEND (line 841/858) / NOTIFY (line 950) / SENDA (line 959) / SENDA target (line 1002/1007)
- **C 对齐**：`ipc.h:25-48 IPC_STATUS_*` 语义已实现（对应 C 的宏展开）。

---

## Ch4: 实现要点

### 4.1 位图原语（const.h:20-27 对齐）

```rust
// ipc_filter.rs:86-111
#[inline]
pub fn set_sys_bit(map: &mut u64, id: u16) {
    if (id as usize) < 64 { *map |= 1u64 << id; }
}
#[inline]
pub fn unset_sys_bit(map: &mut u64, id: u16) {
    if (id as usize) < 64 { *map &= !(1u64 << id); }
}
#[inline]
pub fn get_sys_bit(map: u64, id: u16) -> bool {
    if (id as usize) >= 64 { return false; }
    (map & (1u64 << id)) != 0
}
```

C 的 `sys_map_t` 是多 chunk 位数组（`bitchunk_t chunk[]`），minix-rs 统一用 `u64`（64 位足够覆盖 `NR_SYS_PROCS ≤ 64`）。`id >= 64` 的边界检查是 Rust 安全保证——C 宏直接 `MAP_CHUNK` 无边界检查，越界访问是 UB。

### 4.2 L1 过滤函数（priv.h:86 + system.c:111 对齐）

```rust
// ipc_filter.rs:59-80
#[inline]
pub fn ipc_filter_check(caller_priv: &KPriv, target_sys_id: u16) -> bool {
    caller_priv.may_send_to(target_sys_id)
}

#[inline]
pub fn kcall_filter_check(caller_priv: &KPriv, call_nr: u32) -> bool {
    if call_nr as usize >= 64 { return false; }
    let mask = caller_priv.ipc.s_k_call_mask[0] as u64
        | ((caller_priv.ipc.s_k_call_mask[1] as u64) << 32);
    (mask & (1u64 << call_nr)) != 0
}
```

**签名说明**：`ipc_filter_check` 参数为 `target_sys_id: u16` 而非 `target_priv: &KPriv`。原因：调用方通常已有 target 的 `s_id`（从 endpoint 查找得到），无需再传入整个 `KPriv` 引用。C `may_send_to()` 也只用 `target_priv->s_id`（经 `nr_to_id(nr)` 转换），直接传 `s_id` 更高效且语义等价。

**kcall_filter_check 的 u64 组合**：字段存储为 `[u32; 2]` 对齐 C 布局（privctl 序列化），读取时组合为 u64 简化位运算（D3 妥协）。

### 4.3 L2 过滤池（ipc_filter.h 对齐）

```rust
// ipc_filter.rs:115-169
pub(crate) enum IpcFilterType { Blacklist, Whitelist }  // D8

pub(crate) struct IpcFilterElement {                    // include/minix/ipc_filter.h:23-27
    pub flags: u32,
    pub m_source: i32,
    pub m_type: i32,
}

pub(crate) struct IpcFilterSlot {                        // ipc_filter.h:43-49
    pub filter_type: IpcFilterType,
    pub num_elements: usize,
    pub flags: i32,
    pub next: Option<usize>,                             // D6: 池内索引替代裸指针
    pub elements: [IpcFilterElement; IPCF_MAX_ELEMENTS],
}
```

```rust
// ipc_filter.rs:199-257
pub(crate) struct IpcFilterPool {
    slots: [Option<IpcFilterSlot>; IPCF_POOL_SIZE],     // D5: Option 替代 type==IPCF_NONE
}

impl IpcFilterPool {
    pub const fn new() -> Self { /* 全 None，等价 C IPCF_POOL_INIT memset(0) */ }
    pub(crate) fn allocate(&mut self, filter_type: IpcFilterType) -> Option<usize> { /* 找首个 None */ }
    pub(crate) fn free(&mut self, index: usize) { /* slots[index] = None */ }
    pub(crate) fn get(&self, index: usize) -> Option<&IpcFilterSlot> { ... }
    pub(crate) fn get_mut(&mut self, index: usize) -> Option<&mut IpcFilterSlot> { ... }
}
```

**关键 anti-translate**：
- `IPCF_NONE` 哨兵 → `Option::None`（D5）：非法状态（已分配但 type=NONE）不可表达。
- `*mut ipc_filter_s next` → `Option<usize>`（D6）：释放后悬垂指针在类型层面不可能。
- `int type` → `enum IpcFilterType`（D8）：穷尽匹配，编译器检查分支完备性。

### 4.4 与 syscall.rs 集成（system.c:95-127 对齐）

```rust
// syscall.rs:447-460
// C: `else if (!GET_BIT(priv(caller)->s_k_call_mask, call_nr))` — system.c:111
let call_denied = caller.priv_id
    .and_then(|id| priv_table.get(id))
    .is_none_or(|caller_priv| !kcall_filter_check(caller_priv, call_nr as u32));
if call_denied {
    return KcallResult::CallDenied;   // C: ECALLDENIED (errno.h:206)
}
```

`KcallResult::CallDenied`（syscall.rs:209）对应 C `ECALLDENIED`（errno.h:206）。`priv_id.and_then(...).is_none_or(...)` 链处理三种情况：无 priv_id（deny）、priv_id 但无 priv 表项（deny）、有 priv 表项（查 mask）。已实现，见 `os/kernel/src/syscall.rs`。

### 4.5 未实现的 C 函数（诚实标注缺口）

| C 函数 | C 位置 | Rust 状态 | 依赖 |
|--------|--------|----------|------|
| `allow_ipc_filtered_msg` | system.c:803-874 | 未实现 | 12-ipc-core RECEIVE 路径实现后才有消费方 |
| `allow_ipc_filtered_memreq` | system.c:879+ | 未实现 | 同上（VM 页错误请求过滤） |
| `may_asynsend_to` 不对称 | priv.h:87 | 未实现（当前用 `may_send_to` 替代） | 异步 IPC 路径完整接入 |
| `IPCF_EL_MATCH` 宏链 | ipc_filter.h:19-41 | 未实现 | `allow_ipc_filtered_msg` 的子逻辑 |
| `IPC_STATUS_*` | ipc.h:25-48 | ✅ 已实现 (P9-2) | `CpuContextArch::or_ipc_status_reg` + `proc.rs:1654-1679` + `ipc.rs` 4 路径 wire |

---

## Ch5: 测试

测试位于 `os/kernel/src/ipc_filter.rs:261+`（9 个单元测试），验证 L1 过滤与过滤池契约。

| 测试函数 | 覆盖点 | C 对齐 |
|---------|--------|--------|
| `test_ipc_filter_check_allowed` | `s_ipc_to` 位图允许/拒绝 | priv.h:86 `may_send_to` |
| `test_ipc_filter_check_no_targets` | `s_ipc_to=0` 时全拒绝 | priv.h:86 |
| `test_kcall_filter_check` | `s_k_call_mask` 位图允许/拒绝 | system.c:111 `GET_BIT` |
| `test_kcall_filter_check_out_of_range` | `call_nr >= 64` 返回 false | 边界保护（C 无显式检查） |
| `test_sys_bit_operations` | set/unset/get 位图原语 | const.h:20-27 |
| `test_sys_bit_out_of_range` | `id >= 64` set 为 no-op、get 返回 false | 边界保护 |
| `test_ipc_filter_pool_new_is_empty` | 新池 `allocated_count()==0` | ipc_filter.h:71 `IPCF_POOL_INIT` |
| `test_ipc_filter_pool_allocate_and_free` | 分配/释放/复用槽位 | ipc_filter.h:59-70 `IPCF_POOL_ALLOCATE_SLOT` |
| `test_ipc_filter_pool_free_out_of_range_is_noop` | `free(9999)` 不 panic | 边界保护 |

**测试层级**：
- **L1 对偶**：`test_ipc_filter_check_allowed` / `test_kcall_filter_check` 验证 Rust 位图检查与 C `may_send_to`/`GET_BIT` 行为一致。
- **L2 契约**：`test_ipc_filter_pool_*` 验证 `IpcFilterPool` allocate/free 契约（池满返回 None、free 后可复用、越界 free 不 panic）。
- **边界**：`*_out_of_range` 覆盖 `id >= 64` / `call_nr >= 64` 的边界保护（C 宏无此检查，是 Rust 安全增强）。

**未覆盖**（依赖缺口）：`allow_ipc_filtered_msg` 的过滤链遍历、blacklist/whitelist 翻转逻辑、`IPCF_EL_MATCH` 匹配——待 L2 过滤实现后补充。

**IPC_STATUS_* 测试**（P9-2 落地）：IPC_STATUS helper（`ipc_status_add_call` / `ipc_status_add_flags`，[proc.rs:1654-1679](file:///home/xzhao/github/minix-rs/os/kernel/src/proc.rs)，C 的 `IPC_STATUS_ADD` 内联进两者）无独立单元测试，由 `ipc.rs` 4 路径 wire 点的集成测试间接覆盖（SEND/NOTIFY/SENDA 路径在 [ipc.rs:841/950/959/1002](file:///home/xzhao/github/minix-rs/os/kernel/src/ipc.rs) 调用 helper 设置状态码）。

---

## Ch6: 已知缺口与限制

| 缺口 | C 位置 | Rust 状态 | 优先级 | 依赖 |
|------|--------|----------|--------|------|
| `allow_ipc_filtered_msg` | system.c:803-874 | 未实现 | P1 | 12-ipc-core RECEIVE 路径（当前 skeleton） |
| `may_asynsend_to` 不对称 | priv.h:87 | 未实现（用 `may_send_to` 替代） | P1 | 异步 IPC 路径完整接入 |
| `IPC_STATUS_*` 机制 | ipc.h:25-48 | ✅ 已实现 (P9-2) | — | — |
| `IPCF_EL_MATCH` 宏链 | ipc_filter.h:19-41 | 未实现 | P2 | `allow_ipc_filtered_msg` 子逻辑 |
| `allow_ipc_filtered_memreq` | system.c:879+ | 未实现 | P2 | VM 页错误请求过滤 |

**缺口影响评估**：
- L1 过滤（send/notify/asyncsend/kcall）**已完整实现**，IPC 安全的基础强制检查可用。
- L2 过滤链（receive 路径偏好过滤）**未实现**——当前 `s_ipcf` 字段存在但无消费方。影响：接收方无法按 m_source/m_type 细粒度过滤，所有未被 L1 拦截的消息都会被接收。这是功能缺失而非安全漏洞（L1 已保证基本权限），但限制了一些靠 L2 实现的用例（如 VM 只接收特定类型的内存请求）。

---

## Ch7: 参见

**上游（字段定义与配置入口）**：
- [22-privilege.md](22-privilege.md) — `s_ipc_to` / `s_k_call_mask` / `s_ipcf` 字段定义与 privctl 配置
- [17-syscall-process.md](17-syscall-process.md) — `SYS_PRIVCTL` 运行时权限控制入口（设置 s_ipcf）

**调用方（过滤接入点）**：
- [12-ipc-core.md](12-ipc-core.md) — send/receive/notify/asyncsend 调用 L1/L2 过滤；异步 IPC 消息表与 `may_asynsend_to`
- [13-syscall-dispatch.md](13-syscall-dispatch.md) — `kernel_call_dispatch` 中 `kcall_filter_check` 接入点

**下游（L2 过滤消费方）**：
- [24-cross-space-runtime.md](24-cross-space-runtime.md) — `allow_ipc_filtered_memreq` 用于 VM 页错误请求过滤（依赖 L2 实现）
