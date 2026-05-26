# 12-syscall-dispatch: 系统调用路由

> **分类**: Kernel 特权与系统调用
> **源码**: `minix3/minix/kernel/system.c`: kernel_call_dispatch(95) + kernel_call_finish(59) + kernel_call(136)
> **说明**: sys_call 入口 → 查 s_k_call_mask → do_xxx() 分发 → 写回返回值——单次系统调用的完整路径

---

## 1. 概述

### 1.1 概念定义/作用

**系统调用路由**是 Minix3 内核将用户空间的系统调用请求分发到对应处理函数的机制。在微内核架构中，系统服务（PM、VFS、VM 等）运行在用户态，但某些操作必须由内核执行（如修改页表、操作硬件中断、进程调度等）。这些操作通过**内核调用（kernel call）**接口访问——系统服务发送特定类型的消息给 SYSTEM 任务，SYSTEM 任务验证权限后调用对应的处理函数。

内核调用与普通 IPC 消息的区别在于：普通 IPC 是进程间通信，内核仅做消息中转；内核调用是进程请求内核执行特权操作，内核直接处理并返回结果。

整个路由流程为：用户进程 → 陷阱进入内核 → `kernel_call()` → `kernel_call_dispatch()` → 查 `call_vec[]` 表 → `do_xxx()` 处理 → `kernel_call_finish()` → 写回返回值。

### 1.2 与 Minix3 的对应关系

| 功能 | 函数 | 位置 |
|------|------|------|
| 系统调用入口 | `kernel_call()` | system.c:136 |
| 调用号分发 | `kernel_call_dispatch()` | system.c:95 |
| 结果写回 | `kernel_call_finish()` | system.c:59 |
| 调用向量初始化 | `system_init()` | system.c:168 |
| 调用向量映射宏 | `map()` | system.c:54 |
| 调用向量表 | `call_vec[NR_SYS_CALLS]` | system.c:52 |

### 1.3 关键状态/机制说明

**调用向量表（call_vec）**：全局静态数组 `call_vec[NR_SYS_CALLS]`，每个元素是一个函数指针 `int (*)(struct proc *, message *)`。`system_init()` 中通过 `map(SYS_XXX, do_xxx)` 宏将每个内核调用号映射到对应的处理函数。

**权限检查**：`kernel_call_dispatch()` 在分发前检查调用方的 `s_k_call_mask` 位图。若对应位未设置，返回 `ECALLDENIED`。这确保了每个系统服务只能调用被授权的内核操作。

**VMSUSPEND 机制**：某些内核调用（如 `sys_vircopy`）可能触发页缺失，此时处理函数返回 `VMSUSPEND`。`kernel_call_finish()` 检测到此返回值后，保存请求消息到 `p_vmrequest.saved.reqmsg`，设置 `MF_KCALL_RESUME` 标志，等待 VM 处理完页缺失后恢复调用。

**EDONTREPLY 语义**：若处理函数返回 `EDONTREPLY`，`kernel_call_finish()` 不写回返回值——调用方不会收到回复。这用于不需要立即回复的场景。

### 1.4 行为规则

1. **调用号验证**：`call_nr` 必须在 `[0, NR_SYS_CALLS)` 范围内，否则返回 `EBADREQUEST`
2. **权限检查**：`s_k_call_mask` 对应位必须置位，否则返回 `ECALLDENIED`
3. **处理函数必须存在**：`call_vec[call_nr]` 非 NULL，否则返回 `EBADREQUEST`
4. **消息来源标记**：进入 `kernel_call_dispatch()` 前设置 `msg.m_source = caller->p_endpoint`
5. **VMSUSPEND 暂停**：返回 `VMSUSPEND` 时保存请求消息，设置 `MF_KCALL_RESUME`，不写回返回值
6. **正常完成写回**：返回值非 `VMSUSPEND` 且非 `EDONTREPLY` 时，将结果作为消息写回调用方的用户空间缓冲区
7. **用户指针错误处理**：若 `copy_msg_from_user` 或 `copy_msg_to_user` 失败，向调用方发送 SIGSEGV

## 2. C 源码分析

### 2.1 相关定义（常量、配置等）

#### 2.1.1 内核调用号

定义于 `minix3/minix/include/minix/com.h`，按功能分组：

**进程管理**

| 调用号 | 处理函数 | 功能 |
|--------|---------|------|
| `SYS_FORK` | `do_fork()` | 创建子进程 |
| `SYS_EXEC` | `do_exec()` | 执行新程序（更新进程映像） |
| `SYS_CLEAR` | `do_clear()` | 清理退出进程 |
| `SYS_EXIT` | `do_exit()` | 系统进程退出 |
| `SYS_PRIVCTL` | `do_privctl()` | 特权控制 |
| `SYS_TRACE` | `do_trace()` | 进程追踪（ptrace） |
| `SYS_SETGRANT` | `do_setgrant()` | 设置 grant 表 |
| `SYS_RUNCTL` | `do_runctl()` | 运行控制（停止/恢复） |
| `SYS_UPDATE` | `do_update()` | 更新进程（服务热更新） |
| `SYS_STATECTL` | `do_statectl()` | 状态控制 |

**信号处理**

| 调用号 | 处理函数 | 功能 |
|--------|---------|------|
| `SYS_KILL` | `do_kill()` | 发送信号 |
| `SYS_GETKSIG` | `do_getksig()` | 获取待处理内核信号 |
| `SYS_ENDKSIG` | `do_endksig()` | 结束信号处理 |
| `SYS_SIGSEND` | `do_sigsend()` | 发送 POSIX 信号 |
| `SYS_SIGRETURN` | `do_sigreturn()` | 从信号处理返回 |

**设备 I/O**

| 调用号 | 处理函数 | 功能 |
|--------|---------|------|
| `SYS_IRQCTL` | `do_irqctl()` | 中断控制 |
| `SYS_DEVIO` | `do_devio()` | I/O 端口读写（x86） |
| `SYS_VDEVIO` | `do_vdevio()` | 批量 I/O 端口操作（x86） |

**内存管理**

| 调用号 | 处理函数 | 功能 |
|--------|---------|------|
| `SYS_MEMSET` | `do_memset()` | 内存填充 |
| `SYS_VMCTL` | `do_vmctl()` | VM 控制操作 |

**地址映射与复制**

| 调用号 | 处理函数 | 功能 |
|--------|---------|------|
| `SYS_UMAP` | `do_umap()` | 虚拟地址→物理地址映射 |
| `SYS_UMAP_REMOTE` | `do_umap_remote()` | 远程进程地址映射 |
| `SYS_VUMAP` | `do_vumap()` | 向量化地址映射 |
| `SYS_VIRCOPY` | `do_vircopy()` | 虚拟地址间复制 |
| `SYS_PHYSCOPY` | `do_copy()` | 物理地址间复制 |
| `SYS_SAFECOPYFROM` | `do_safecopy_from()` | 安全复制（从） |
| `SYS_SAFECOPYTO` | `do_safecopy_to()` | 安全复制（到） |
| `SYS_VSAFECOPY` | `do_vsafecopy()` | 向量化安全复制 |
| `SYS_SAFEMEMSET` | `do_safememset()` | 安全内存填充 |

**时钟**

| 调用号 | 处理函数 | 功能 |
|--------|---------|------|
| `SYS_TIMES` | `do_times()` | 获取运行时间 |
| `SYS_SETALARM` | `do_setalarm()` | 设置同步闹钟 |
| `SYS_STIME` | `do_stime()` | 设置启动时间 |
| `SYS_SETTIME` | `do_settime()` | 设置系统时间 |
| `SYS_VTIMER` | `do_vtimer()` | 虚拟定时器 |

**系统控制**

| 调用号 | 处理函数 | 功能 |
|--------|---------|------|
| `SYS_ABORT` | `do_abort()` | 终止 MINIX |
| `SYS_GETINFO` | `do_getinfo()` | 获取系统信息 |
| `SYS_DIAGCTL` | `do_diagctl()` | 诊断控制 |
| `SYS_SPROF` | `do_sprofile()` | 统计式 profile |

**x86 特有**

| 调用号 | 处理函数 | 功能 |
|--------|---------|------|
| `SYS_READBIOS` | `do_readbios()` | 读取 BIOS 数据 |
| `SYS_IOPENABLE` | `do_iopenable()` | 启用 I/O 权限 |
| `SYS_SDEVIO` | `do_sdevio()` | 安全设备 I/O |

**调度**

| 调用号 | 处理函数 | 功能 |
|--------|---------|------|
| `SYS_SCHEDULE` | `do_schedule()` | 调度进程 |
| `SYS_SCHEDCTL` | `do_schedctl()` | 调度器控制 |

**机器上下文**

| 调用号 | 处理函数 | 功能 |
|--------|---------|------|
| `SYS_SETMCONTEXT` | `do_setmcontext()` | 设置机器上下文 |
| `SYS_GETMCONTEXT` | `do_getmcontext()` | 获取机器上下文 |

#### 2.1.2 特殊返回值

| 常量 | 值 | 含义 |
|------|-----|------|
| `VMSUSPEND` | 特殊负值 | 调用因页缺失暂停，等待 VM 恢复 |
| `EDONTREPLY` | 特殊值 | 不向调用方发送回复 |

### 2.2 核心数据结构

#### 2.2.1 call_vec 调用向量表

```c
static int (*call_vec[NR_SYS_CALLS])(struct proc * caller, message *m_ptr);
```

- 类型：函数指针数组，每个元素签名为 `int (struct proc *, message *)`
- 索引：`call_nr - KERNEL_CALL`（内核调用号减去偏移量）
- 初始化：`system_init()` 中通过 `map()` 宏设置，未映射的槽位为 NULL
- 查找：`kernel_call_dispatch()` 中通过 `call_vec[call_nr]` 直接索引

#### 2.2.2 内核调用消息格式

内核调用使用标准 `message` 结构，关键字段：

| 字段 | 含义 |
|------|------|
| `m_source` | 调用方 endpoint（由内核设置） |
| `m_type` | 内核调用号（`KERNEL_CALL + call_index`） |
| 其余字段 | 调用特定参数（每个 do_xxx 函数自行解析） |

### 2.3 关键函数分析

#### 2.3.1 kernel_call()——系统调用入口

`minix3/minix/kernel/system.c:136-163`

```c
void kernel_call(message *m_user, struct proc *caller)
```

**功能**：内核调用的主入口，由体系结构相关的陷阱处理代码调用。

**行为**：
1. 记录调用方的用户空间消息缓冲区地址：`p_delivermsg_vir = m_user`
2. 从用户空间复制消息到内核栈：`copy_msg_from_user(m_user, &msg)`
3. 若复制失败：向调用方发送 SIGSEGV，直接返回
4. 设置消息来源：`msg.m_source = caller->p_endpoint`
5. 分发调用：`result = kernel_call_dispatch(caller, &msg)`
6. 记录计费信息：`kbill_kcall = caller`
7. 处理结果：`kernel_call_finish(caller, &msg, result)`

**关键设计**：消息先从用户空间复制到内核栈，处理完后再从内核栈复制回用户空间。这确保了内核不会直接访问用户空间指针，避免了 TOCTOU（Time-of-Check-to-Time-of-Use）安全问题。

#### 2.3.2 kernel_call_dispatch()——调用号分发

`minix3/minix/kernel/system.c:95-127`

```c
static int kernel_call_dispatch(struct proc *caller, message *msg)
```

**功能**：验证调用号和权限，分发到对应的处理函数。

**行为**：
1. 提取调用号：`call_nr = msg->m_type - KERNEL_CALL`
2. 范围检查：`call_nr < 0 || call_nr >= NR_SYS_CALLS` → `EBADREQUEST`
3. 权限检查：`!GET_BIT(priv(caller)->s_k_call_mask, call_nr)` → `ECALLDENIED`
4. 调用处理函数：`result = (*call_vec[call_nr])(caller, msg)`
5. 若 `call_vec[call_nr] == NULL` → `EBADREQUEST`
6. 返回处理结果

**权限检查的粒度**：`s_k_call_mask` 是一个位数组，每一位对应一个内核调用号。这允许精确控制每个系统服务能调用哪些内核操作——例如用户进程的 `s_k_call_mask` 全部清零（`NO_C`），不允许任何内核调用。

#### 2.3.3 kernel_call_finish()——结果写回

`minix3/minix/kernel/system.c:59-93`

```c
static void kernel_call_finish(struct proc *caller, message *msg, int result)
```

**功能**：处理内核调用的返回值，将结果写回调用方的用户空间。

**行为**（两条路径）：

**VMSUSPEND 路径**（`result == VMSUSPEND`）：
1. 断言 `RTS_VMREQUEST` 已设置且 `p_vmrequest.type == VMSTYPE_KERNELCALL`
2. 保存请求消息：`p_vmrequest.saved.reqmsg = *msg`
3. 设置 `MF_KCALL_RESUME` 标志
4. 不写回返回值——等待 VM 恢复后重新执行

**正常完成路径**：
1. 清除保存的请求消息：`p_vmrequest.saved.reqmsg.m_source = NONE`
2. 若 `result != EDONTREPLY`：
   - 设置返回消息：`msg->m_source = SYSTEM`, `msg->m_type = result`
   - 将消息复制回用户空间：`copy_msg_to_user(msg, p_delivermsg_vir)`
   - 若复制失败：向调用方发送 SIGSEGV

#### 2.3.4 system_init()——调用向量初始化

`minix3/minix/kernel/system.c:168-270`

```c
void system_init(void)
```

**功能**：初始化 SYSTEM 任务的调用向量表和 IRQ 钩子。

**行为**：
1. 初始化 IRQ 钩子数组：标记所有钩子为可用
2. 初始化所有特权结构的闹钟定时器
3. 清空调用向量表：`call_vec[i] = NULL`
4. 通过 `map()` 宏映射所有已知的内核调用号到处理函数
5. 条件编译：x86 特有的 `SYS_DEVIO/SYS_VDEVIO/SYS_READBIOS` 等

#### 2.3.5 map() 宏——调用号映射

`minix3/minix/kernel/system.c:54-57`

```c
#define map(call_nr, handler) \
    { int call_index = call_nr - KERNEL_CALL; \
      assert(call_index >= 0 && call_index < NR_SYS_CALLS); \
      call_vec[call_index] = (handler); }
```

**功能**：将内核调用号映射到处理函数，编译时验证调用号合法性。

**行为**：
1. 计算调用索引：`call_index = call_nr - KERNEL_CALL`
2. 编译时断言索引在合法范围内
3. 设置 `call_vec[call_index] = handler`

### 2.4 调用关系/调用点分析

#### 2.4.1 完整系统调用路径

```
用户空间系统服务调用 sys_xxx()
  └─ 内核陷阱（int 0x30 / syscall 指令）
       └─ 体系结构相关处理
            └─ kernel_call(m_user, caller)
                 ├─ copy_msg_from_user(m_user, &msg)
                 │    └─ [失败?] → cause_sig(SIGSEGV)
                 ├─ msg.m_source = caller->p_endpoint
                 ├─ kernel_call_dispatch(caller, &msg)
                 │    ├─ call_nr = msg->m_type - KERNEL_CALL
                 │    ├─ [范围检查?] → EBADREQUEST
                 │    ├─ [权限检查?] → ECALLDENIED
                 │    └─ call_vec[call_nr](caller, &msg)
                 │         └─ do_xxx() → 返回 result
                 └─ kernel_call_finish(caller, &msg, result)
                      ├─ [VMSUSPEND?] → 保存请求, MF_KCALL_RESUME
                      └─ [正常?] → copy_msg_to_user() 写回结果
```

#### 2.4.2 VMSUSPEND 恢复路径

```
VM 处理完页缺失后通知内核
  └─ VM 回复消息
       └─ 内核恢复调用
            └─ kernel_call_dispatch(caller, &saved_msg)
                 └─ 重新执行 do_xxx()
```

#### 2.4.3 kernel_call 的调用者

| 调用者 | 场景 |
|--------|------|
| `do_ipc()` | SENDREC 到 SYSTEM 任务时 |
| 陷阱处理程序 | 系统服务直接执行内核调用时 |
| `kernel_call_resume()` | VMSUSPEND 恢复后重新执行 |

### 2.5 设计要点/特殊处理

#### 2.5.1 内核调用 vs IPC 消息

内核调用和 IPC 消息使用不同的路径：

- **IPC 消息**：进程间通信，内核仅做消息中转，不处理消息内容
- **内核调用**：进程请求内核执行特权操作，内核直接处理并返回结果

两者共享同一个陷阱入口（`do_ipc()`），但通过 `m_type` 区分：若 `m_type >= KERNEL_CALL`，走内核调用路径；否则走 IPC 路径。

#### 2.5.2 消息复制与 TOCTOU 防护

`kernel_call()` 先将用户空间消息复制到内核栈，处理完后再复制回去。这防止了 TOCTOU 攻击——调用方无法在内核检查参数后、内核使用参数前修改消息内容。

代价是两次消息复制（进+出），但这是安全性的必要开销。

#### 2.5.3 VMSUSPEND 与内核调用恢复

某些内核调用（如 `sys_vircopy`）需要访问调用方的地址空间，但目标页面可能不在物理内存中。此时处理函数返回 `VMSUSPEND`，内核：

1. 保存完整的请求消息到 `p_vmrequest.saved.reqmsg`
2. 设置 `MF_KCALL_RESUME` 标志
3. 通知 VM 处理页缺失
4. VM 完成后，`kernel_call_resume()` 从保存的消息重新执行调用

这确保了页缺失对调用方是透明的——调用方不知道内核调用被暂停过。

#### 2.5.4 call_vec 的函数指针设计

`call_vec` 使用函数指针数组而非 switch-case，优势：

1. **可扩展性**：添加新内核调用只需 `map(SYS_XXX, do_xxx)` 一行
2. **O(1) 分发**：数组索引直接定位处理函数，无需逐 case 比较
3. **条件编译友好**：架构相关的调用可以条件性地 `map()` 或不 `map()`

代价是函数指针间接调用的开销（比直接调用多一次内存访问），但在系统调用路径上这个开销可以忽略。

#### 2.5.5 EDONTREPLY 的使用场景

`EDONTREPLY` 用于不需要立即回复的内核调用。典型场景是 `sys_exit()`——退出的系统进程不需要收到回复。处理函数返回 `EDONTREPLY` 后，`kernel_call_finish()` 不写回返回值，调用方也不会被解除阻塞。
