# 25-misc-unported: 大纲（Outline v1）

> **文档**: `25-misc-unported.md`
> **状态**: v1 快照（2026-08-01 首次生成，基于 C 源码 + 当前 Rust 实现）
> **教学目标**: 从"内核如何对待未完成移植的系统调用"这一问题出发，建立"分类处理 + 渐进迁移"的策略心智模型

---

## Ch1: 概念（未移植系统调用的处理策略）

### 教学目标
- **核心问题**: 一个微内核有数十个系统调用，重写过程不可能一次性完成全部。内核如何对待"尚未移植"或"故意延迟"的调用，才能既不破坏用户态兼容性，又能渐进推进？
- **目标读者**: 已读 16-24 的读者，理解内核系统调用分发机制与跨地址空间拷贝
- **前置知识**: `dispatch_*` 分发模式、`KcallResult` 返回类型、`data_copy_vmcheck` 跨地址空间拷贝、`ProcessTable`/`PrivTable`

### 1.1 "未移植"的三种语义
- **完全未实现**: 调用号存在但无处理函数 → `ENOSYS`（与 C `do_unused` 一致）
- **部分实现**: 输入验证已对齐 C，核心数据搬运 DEFERRED → 验证通过后返回 `ENOSYS`
- **完全实现**: 输入验证 + 核心逻辑都对齐 C → 正常返回

> **关键区分**: "部分实现"不是"占位"——它真实执行了 C 的所有前置检查（endpoint 合法性、权限、对齐等），只是最后的数据搬运步骤依赖尚未就绪的子系统（Direct Map / arch trait）。这种"前置验证 + 后置 DEFERRED"模式让用户态提前发现参数错误，而不是等到功能完整时才暴露。

### 1.2 系统调用的四类分组
| 分组 | 调用 | 共性 |
|------|------|------|
| 信息查询 | `SYS_GETINFO` | 多子请求，按 request 分派，data_copy 到用户 |
| 进程追踪 | `SYS_TRACE` | 多子请求，跨地址空间拷贝 + 进程表字段读写 |
| 进程更新 | `SYS_UPDATE` | RS 专用，进程槽位交换（live update） |
| 性能分析 | `SYS_SPROF` | 统计采样，依赖时钟/NMI 子系统 |

### 1.3 redox 对照
- **redox**: scheme-based 架构——每个 scheme 自管理请求，未实现的请求自然返回错误；内核不集中维护"未实现调用表"
- **Minix3**: 集中式 `ktasktab[]` 分派表——每个 `SYS_*` 必须有对应 `do_*` 函数，未实现的用 `do_unused` 兜底
- **minix-rs**: 沿用 Minix3 集中分派模型，但用 Rust `enum` + `match` 替代 C 的 `switch`，"未实现"通过 `ENOSYS` 显式表达

### 1.4 本章不讲什么
- 具体子请求的字段语义（Ch2 详述）
- Rust 类型设计（Ch3 详述）
- 已在前序文档覆盖的调用（见 §1.1 已覆盖表）

---

## Ch2: C 源码分析

### 2.1 文件清单与规模
| 文件 | 行数 | 核心函数 | 子请求数 |
|------|------|---------|---------|
| `system/do_getinfo.c` | 227 | `do_getinfo()` + `update_idle_time()` | 18 |
| `system/do_trace.c` | 208 | `do_trace()` + COPYFROMPROC/COPYTOPROC 宏 | 14 |
| `system/do_update.c` | 338 | `do_update()` + 7 helper | — |
| `system/do_sprofile.c` | 131 | `do_sprofile()` + `clean_seen_flag()` | 2 |
| `do_unused` | ~9 | `do_unused()` | — |

### 2.2 do_getinfo 子请求全集（18 个）
- 数据结构查询: `GET_KINFO`/`GET_MACHINE`/`GET_LOADINFO`/`GET_CPUINFO`/`GET_HZ`
- 进程表查询: `GET_PROC`/`GET_PROCTAB`/`GET_PROC2`/`GET_REGS`/`GET_PRIV`/`GET_PRIVTAB`
- 调度/资源: `GET_SCHEDINFO`/`GET_IMAGE`/`GET_MONPARAMS`/`GET_IRQHOOKS`/`GET_IRQACTIDS`
- 随机数: `GET_RANDOMNESS`/`GET_RANDOMNESS_BIN`
- 计时: `GET_IDLETSC`/`GET_CPUTICKS`/`GET_LOCKTIMING`
- 身份: `GET_WHOAMI`（特殊：直接写 reply message，不走 data_copy）
- BIOS: `GET_BIOSCTRS`（x86-only）

### 2.3 do_trace 子请求全集（14 个）
- 控制流: `T_STOP`/`T_RESUME`/`T_STEP`/`T_SYSCALL`/`T_DETACH`/`T_EXIT`
- 内存读写: `T_GETINS`/`T_GETDATA`/`T_SETINS`/`T_SETDATA`/`T_READB_INS`/`T_WRITEB_INS`
- 进程表读写: `T_GETUSER`/`T_SETUSER`（含架构特定段寄存器保护）

### 2.4 do_update 流程（7 步）
1. endpoint 验证（src/dst）
2. SYS_PROC 权限验证
3. `proc_is_updatable` 状态检查
4. `inherit_priv_irq/io/mem` 继承
5. 保存原始状态 + 调整 asyn_table
6. 槽位交换（proc + priv）
7. `adjust_proc_slot`/`adjust_priv_slot`/`swap_proc_slot_pointer`/`swap_memreq`

### 2.5 do_sprofile 状态机
- `PROF_START`: 检查 `sprofiling` → 验证 endpoint → 设置参数 → 初始化时钟/NMI → `sprofiling=1`
- `PROF_STOP`: 检查 `!sprofiling` → `sprofiling=0` → 停止时钟 → data_copy 结果到用户

---

## Ch3: Rust 设计决策

### 3.1 D1: 子请求用 enum 而非裸整数
- C: `switch(m_ptr->request)` 用 int
- Rust: `GetInfoRequest`/`TraceRequest`/`ProfAction`/`ProfIntrType` enum + `TryFrom<i32>`
- 理由: 类型安全，编译期穷尽性检查

### 3.2 D2: 未实现调用返回 ENOSYS（对齐 C `do_unused`）
- 理由: 用户态可据此判断"功能是否存在"，不会静默成功

### 3.3 D3: "前置验证 + 后置 DEFERRED" 模式
- 对 `SYS_TRACE`/`SYS_UPDATE`/`SYS_SPROF`，先执行所有 C 的输入验证（endpoint/权限/对齐/状态机），核心数据搬运返回 `ENOSYS`
- 理由: 让参数错误尽早暴露，不等到功能完整

### 3.4 D4: x86-only 调用统一返回 BadCall/ENOSYS
- `GET_BIOSCTRS`/`SYS_READBIOS`/`SYS_IOPENABLE`/`SYS_SDEVIO` 等 x86 专用
- 理由: minix-rs 面向三架构，x86-only 功能不进入通用路径

### 3.5 D5: SYS_TRACE 部分实现（非完全延迟）
- 纯 flag/RTS 操作（`T_STEP`/`T_CONT`/`T_KILL`）已实现
- 跨地址空间拷贝（`T_GETINS` 等）做对齐检查后返回 ENOSYS
- 进程表字段读写（`T_GETUSER`/`T_SETUSER`）做对齐检查后返回 ENOSYS
- 理由: flag 操作不依赖未就绪子系统，可立即实现

### 3.6 D6: GET_WHOAMI 直接写 reply message（对齐 C）
- C: 直接写 `m_ptr->m_krn_lsys_sys_getwhoami.*`，不走 data_copy
- Rust: 同样直接写 `msg.m_u.m_krn_lsys_sys_getwhoami`

### 3.7 D7: GET_KINFO 用 M4 格式返回关键字段（临时方案）
- C: 整个 `struct kinfo` 通过 data_copy 拷贝
- Rust: 当前用 `MessageM4` 返回 5 个关键字段（nr_procs/nr_tasks/user_sp/freepde_start/vir_kern_start）
- 缺口: 待 data_copy_vmcheck 落地后改为完整拷贝

### 3.8 D8: SPROFILING 用 AtomicBool（SMP 安全）
- C: `int sprofiling`（隐式 BKL 保护）
- Rust: `AtomicBool` + `compare_exchange` 状态机
- 理由: 显式 SMP 安全，不依赖隐式 BKL 假设

### 3.9 D9: 消息字段用类型化访问（非 m1 overlay）
- `SYS_TRACE`/`SYS_GETINFO` 用 `mess_lsys_krn_sys_trace`/`mess_lsys_krn_sys_getinfo` union 成员
- 禁止用 `m_m1` overlay（字段布局不同会导致 P0 字段映射 bug）
- 理由: 类型安全 + 避免 layout 陷阱

---

## Ch4: 实现详解

### 4.1 GetInfoRequest enum（15 变体，覆盖已实现的子集）
- 知识点: `#[repr(i32)]` + `TryFrom<i32>` + 缺失变体的 DEFERRED 策略

### 4.2 dispatch_getinfo 分派
- 知识点: `msg_getinfo` 类型化访问 + WhoAmI/KInfo/Proc/ProcTab/PrivTab/LoadInfo 分支

### 4.3 dispatch_trace 分派
- 知识点: `msg_trace` 类型化访问 + 4 类请求处理（flag/内存/进程表/对齐检查）

### 4.4 dispatch_update 验证
- 知识点: 7 步验证 + `proc_is_updatable` 纯函数 + 槽位交换 DEFERRED

### 4.5 dispatch_profile 状态机
- 知识点: `SPROFILING` AtomicBool + `compare_exchange` + rollback on validation failure

### 4.6 dispatch_unused 兜底
- 知识点: 返回 ENOSYS

---

## Ch5: 测试要点

### 5.1 enum TryFrom 测试
- `GetInfoRequest`/`TraceRequest` 边界值

### 5.2 dispatch_trace 测试
- flag 操作副作用（MF_STEP/RTS_P_STOP）
- 对齐检查（unaligned → EFAULT）
- endpoint 验证（invalid → EINVAL, kernel → EPERM）

### 5.3 dispatch_update 测试
- 7 步验证每步的拒绝路径
- `proc_is_updatable` 三种状态

### 5.4 dispatch_profile 状态机测试
- double start → EBUSY
- stop without start → EBUSY
- rollback on invalid endpoint

### 5.5 dispatch_getinfo 测试
- WhoAmI reply message
- Proc endpoint SELF 替换
- invalid endpoint → EINVAL

---

## Ch6: 参见

- [22-privilege.md](22-privilege.md) — SYS_PRIVCTL（权限控制）
- [17-cross-space-copy.md](17-cross-space-copy.md) — data_copy_vmcheck
- [24-cross-space-runtime.md](24-cross-space-runtime.md) — 跨地址空间运行时
- [20-syscall-device.md](20-syscall-device.md) — x86-only 调用处理

---

## 覆盖矩阵

| C 符号 | Ch1 概念 | Ch2 分析 | Ch3 设计 | Ch4 实现 | Ch5 测试 |
|--------|---------|---------|---------|---------|---------|
| `do_getinfo` | ✅ §1.2 | ✅ §2.2 | ✅ D1/D6/D7/D9 | ✅ §4.1/4.2 | ✅ §5.5 |
| `do_trace` | ✅ §1.2 | ✅ §2.3 | ✅ D3/D5/D9 | ✅ §4.3 | ✅ §5.2 |
| `do_update` | ✅ §1.2 | ✅ §2.4 | ✅ D3 | ✅ §4.4 | ✅ §5.3 |
| `do_sprofile` | ✅ §1.2 | ✅ §2.5 | ✅ D3/D8 | ✅ §4.5 | ✅ §5.4 |
| `do_unused` | ✅ §1.1 | ✅ §2.1 | ✅ D2 | ✅ §4.6 | ✅ §5.1 |
| `update_idle_time` | — | ✅ §2.2 | — | — | — |
| `proc_is_updatable` | — | ✅ §2.4 | ✅ D3 | ✅ §4.4 | ✅ §5.3 |
| `inherit_priv_*` | — | ✅ §2.4 | — | DEFERRED | — |
| `swap_proc_slot` | — | ✅ §2.4 | — | DEFERRED | — |
| `clean_seen_flag` | — | ✅ §2.5 | — | DEFERRED | — |
