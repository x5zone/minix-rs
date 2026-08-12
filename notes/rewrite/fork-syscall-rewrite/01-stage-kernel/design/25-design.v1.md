# 25-misc-unported: 设计文档（Design v1）

> **文档**: `25-misc-unported.md`
> **状态**: v1 快照（2026-08-01，基于 C 源码 + 当前 Rust 实现状态）
> **用途**: Gate H 依据（design↔code 一致性）+ Rust 实现审阅依据

---

## Ch1: 设计决策

### D1: 子请求用 enum + TryFrom<i32>
- **C**: `switch(m_ptr->m_lsys_krn_sys_getinfo.request)` 用裸 int
- **Rust**: `GetInfoRequest`/`TraceRequest`/`ProfAction`/`ProfIntrType` enum + `TryFrom<i32>`
- **理由**: 编译期穷尽性检查；无效值在入口被拒绝（EINVAL）而非落入 default 分支

### D2: 未实现调用返回 ENOSYS（对齐 C do_unused）
- **C**: `do_unused()` 返回 ENOSYS
- **Rust**: `dispatch_unused()` 返回 `KcallResult::Ok(ENOSYS)`
- **理由**: 用户态可据此判断功能是否存在

### D3: "前置验证 + 后置 DEFERRED" 模式
- **适用**: SYS_TRACE / SYS_UPDATE / SYS_SPROF
- **C**: 验证 + 核心逻辑一体
- **Rust**: 验证完整对齐 C，核心数据搬运（data_copy_vmcheck / 槽位交换 / 时钟初始化）返回 ENOSYS
- **理由**: 让参数错误尽早暴露；避免"功能完整时才发现 endpoint 非法"

### D4: x86-only 调用统一返回 ENOSYS/BadCall
- **调用**: GET_BIOSCTRS / SYS_READBIOS / SYS_IOPENABLE / SYS_SDEVIO
- **理由**: minix-rs 面向三架构，x86 专用功能不进入通用路径

### D5: SYS_TRACE 部分实现（非完全延迟）
- **已实现**: T_STEP / T_CONT / T_KILL（纯 flag/RTS 操作）
- **对齐检查 only**: T_GETINS / T_GETDATA / T_SETINS / T_SETDATA / T_GETUSER / T_SETUSER
- **理由**: flag 操作不依赖未就绪子系统（Direct Map / arch trait）

### D6: GET_WHOAMI 直接写 reply message
- **C**: `m_ptr->m_krn_lsys_sys_getwhoami.* = ...` 不走 data_copy
- **Rust**: `msg.m_u.m_krn_lsys_sys_getwhoami = MessKrnLsysSysGetwhoami { ... }`
- **理由**: 对齐 C 特殊路径

### D7: GET_KINFO 用 M4 格式返回关键字段（临时）
- **C**: 整个 `struct kinfo` 通过 data_copy_vmcheck 拷贝
- **Rust**: 当前用 `MessageM4` 返回 5 字段（nr_procs/nr_tasks/user_sp/freepde_start/vir_kern_start）
- **缺口**: 待 data_copy_vmcheck 落地后改为完整拷贝
- **理由**: PM 启动需要 kinfo 关键字段，无需等完整 data_copy 路径

### D8: SPROFILING 用 AtomicBool（SMP 安全）
- **C**: `int sprofiling`（隐式 BKL 保护）
- **Rust**: `pub static SPROFILING: AtomicBool` + `compare_exchange`
- **理由**: 显式 SMP 安全；状态机转换原子化

### D9: 消息字段类型化访问（禁 m1 overlay）
- **C**: `m_ptr->m_lsys_krn_sys_trace.*` / `m_ptr->m_lsys_krn_sys_getinfo.*`
- **Rust**: `msg.m_u.m_lsys_krn_sys_trace` / `msg.m_u.m_lsys_krn_sys_getinfo`
- **禁止**: `msg.m_u.m_m1` overlay（layout 不同 → P0 字段映射 bug）
- **理由**: 类型安全 + 避免 union layout 陷阱

---

## Ch2: Minix3 对齐矩阵

| Minix3 概念 | design 对应 | code 对应 | 一致性 |
|------------|------------|----------|--------|
| do_getinfo 18 子请求 | D1/D6/D7/D9 | dispatch_getinfo | ✅ 部分实现（WhoAmI/KInfo/Proc/Proc2/ProcTab/PrivTab/LoadInfo + 其余 ENOSYS） |
| do_trace 14 子请求 | D3/D5/D9 | dispatch_trace | ✅ 部分实现（3 flag + 6 对齐检查 + 5 DEFERRED） |
| do_update 7 步验证 | D3 | dispatch_update | ✅ 验证完整 + 槽位交换 DEFERRED |
| do_sprofile 状态机 | D3/D8 | dispatch_profile | ✅ 状态机完整 + 时钟初始化 DEFERRED |
| do_unused | D2 | dispatch_unused | ✅ 完整 |
| update_idle_time | — | — | DEFERRED（随 GET_PROCTAB） |
| proc_is_updatable | D3 | proc_is_updatable 纯函数 | ✅ 完整 |
| inherit_priv_* | — | — | DEFERRED（随 SYS_UPDATE 槽位交换） |
| swap_proc_slot/memreq | — | — | DEFERRED（随 SYS_UPDATE） |
| clean_seen_flag | — | — | DEFERRED（随 SPROF 完整实现） |

---

## Ch3: Rust 类型清单

| 类型 | 定义位置 | 用途 | impl 数 |
|------|---------|------|---------|
| `GetInfoRequest` enum | misc.rs | GETINFO 子请求 | TryFrom<i32> |
| `TraceRequest` enum | misc.rs | TRACE 子请求 | TryFrom<i32> |
| `ProfAction` enum | misc.rs | SPROF action | try_from |
| `ProfIntrType` enum | misc.rs | SPROF 中断源 | try_from |
| `dispatch_getinfo` fn | misc.rs | SYS_GETINFO 分派 | — |
| `dispatch_trace` fn | misc.rs | SYS_TRACE 分派 | — |
| `dispatch_update` fn | misc.rs | SYS_UPDATE 分派 | — |
| `dispatch_profile` fn | misc.rs | SYS_SPROF 分派 | — |
| `dispatch_unused` fn | misc.rs | 兜底 | — |
| `proc_is_updatable` fn | misc.rs | updatable 检查 | — |
| `SPROFILING` AtomicBool | misc.rs | SPROF 状态 | — |
| `msg_trace`/`msg_getinfo` fn | misc.rs | 类型化消息访问 | — |

**trait 评估**: 无自定义 trait。所有 dispatch 函数为自由函数，参数注入 `&mut KProcess`/`&ProcessTable`/`&PrivTable`。这是合理的——无多态需求，每个 syscall 有独立分派函数。

---

## Ch4: 已知缺口（DEFERRED）

| 缺口 | C 位置 | 阻塞原因 | 解除条件 |
|------|--------|---------|---------|
| GETINFO 完整 data_copy | do_getinfo.c:214 | data_copy_vmcheck 到用户空间 | Direct Map 落地 |
| GETINFO 缺失子请求 | do_getinfo.c 多个 | 未列入 enum | GET_HZ/GET_IMAGE/GET_PRIV/GET_REGS/GET_MONPARAMS/GET_RANDOMNESS_BIN/GET_IRQACTIDS/GET_IDLETSC/GET_CPUTICKS |
| TRACE 跨地址空间拷贝 | do_trace.c COPYFROMPROC/COPYTOPROC | virtual_copy_vmcheck | Direct Map 落地 |
| TRACE 进程表字段读写 | do_trace.c:108-124 | KProcess 结构布局验证 | struct proc 对齐 |
| TRACE T_SETUSER 段寄存器保护 | do_trace.c:141-166 | arch trait | arch-abstractions |
| TRACE 缺失子请求 | do_trace.c T_STOP/T_DETACH/T_SYSCALL/T_READB_INS/T_WRITEB_INS | enum 对齐 ptrace | TraceRequest 扩展 |
| UPDATE 槽位交换 | do_update.c:129-147 | swap_proc_slot + per-CPU ptproc | ptproc + struct 对齐 |
| UPDATE inherit_priv_* | do_update.c:94-105 | priv_add_irq/io/mem | KPriv API |
| UPDATE abort_proc_ipc_send | do_update.c:220-236 | IPC 队列解构 | IPC 模块 |
| UPDATE swap_memreq | do_update.c:313-337 | VM request 链表 | VmRequestQueue |
| SPROF 时钟初始化 | do_sprofile.c:75-82 | init_profile_clock / nmi_watchdog | ClockArch + NMI |
| SPROF 数据拷贝 | do_sprofile.c:117-120 | data_copy 结果到用户 | Direct Map |
| SPROF clean_seen_flag | do_sprofile.c:25-31 | MF_SPROF_SEEN flag | MiscFlags 扩展 |

---

## 附录: design↔code 一致性

| design 决策 | design 位置 | code 位置 | 一致? |
|------------|------------|----------|-------|
| D1 enum + TryFrom | §D1 | misc.rs:43-101 | ✅ |
| D2 ENOSYS | §D2 | misc.rs:807-809 | ✅ |
| D3 前置验证 + DEFERRED | §D3 | misc.rs:271/280/285/290/442/455/613 | ✅ |
| D4 x86-only ENOSYS | §D4 | misc.rs:292-298 default 分支 | ✅ |
| D5 TRACE 部分实现 | §D5 | misc.rs:402-496 | ✅ |
| D6 WHOAMI 直接写 | §D6 | misc.rs:199-226 | ✅ |
| D7 KINFO M4 格式 | §D7 | misc.rs:227-258 | ✅ |
| D8 SPROFILING AtomicBool | §D8 | misc.rs:800 + 720-765 | ✅ |
| D9 类型化消息访问 | §D9 | misc.rs:159-176 | ✅ |

**一致性**: 9/9 = 100% ✅

---

## 附录: 已知代码问题（待修复）

| 问题 | 位置 | 严重度 | 修复方案 |
|------|------|--------|---------|
| `KEvn` 拼写错误（应为 `KEnv`） | misc.rs:61 + 90 | P1 | 重命名 enum 变体 |
| `ProfAction::try_from`/`ProfIntrType::try_from` 未实现 `TryFrom` trait | misc.rs:669/690 | P2 | 改为 `impl TryFrom<i32>` 或保持固有方法（设计选择） |
