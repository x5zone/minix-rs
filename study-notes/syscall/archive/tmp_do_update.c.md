# kernel/system/do_update.c 逐行讲解

> **文件路径**: `minix3/minix/kernel/system/do_update.c`
> **核心功能**: 进程槽位交换系统调用（SYS_UPDATE）
> **系统调用号**: SYS_UPDATE

---

## 一、文件概述

### 1.1 功能说明（是什么）

`do_update.c` 实现了 Minix3 的进程槽位交换功能。这个系统调用允许**交换两个进程的所有状态**，包括进程结构、权限结构、IPC 状态等。

**生活类比**：想象两个演员在舞台上交换角色——演员 A 穿上演员 B 的服装，演员 B 穿上演员 A 的服装，但舞台上的"角色位置"保持不变。`do_update` 做的就是这种"身份交换"：两个进程交换它们的所有属性，但保持原有的槽位位置。

### 1.2 设计原因（为什么）

**热更新需求**：

1. **服务升级**：系统服务（如 VFS、PM）需要升级时，可以启动新版本进程，然后交换槽位，实现无缝切换。

2. **故障恢复**：如果服务进程崩溃，可以启动备用进程并交换槽位，恢复服务。

3. **微内核架构**：在微内核中，服务运行在用户态，需要内核提供安全的进程状态交换机制。

**为什么需要交换而不是直接替换？**

- 保持进程的端点号不变（其他进程通过端点号通信）。
- 保持进程的槽位号不变（内核内部引用）。
- 继承原有的权限和 IPC 关系。

### 1.3 应用场景（什么情景使用）

| 场景 | 说明 |
|------|------|
| 服务热升级 | RS（重启动服务）升级系统服务 |
| 故障恢复 | 服务崩溃后恢复 |
| 进程迁移 | 将进程状态迁移到新槽位 |

---

## 二、逐行详细讲解

### 2.1 文件头注释

```c
/* The kernel call implemented in this file:
 *   m_type:	SYS_UPDATE
 *
 * The parameters for this kernel call are:
 *    m2_i1:	SYS_UPD_SRC_ENDPT 	(source process endpoint)
 *    m2_i2:	SYS_UPD_DST_ENDPT	(destination process endpoint)
 *    m2_i3:	SYS_UPD_FLAGS		(update flags)
 */
```

**逐行解释**：

- **第1-2行**：说明本文件实现 `SYS_UPDATE` 系统调用。

- **第4-7行**：描述参数：
  - `SYS_UPD_SRC_ENDPT`：源进程端点号。
  - `SYS_UPD_DST_ENDPT`：目标进程端点号。
  - `SYS_UPD_FLAGS`：更新标志（如 `SYS_UPD_ROLLBACK`）。

**消息结构**：

```
消息结构:
┌─────────────────────────────────────────────────────────────┐
│ m1_i1 (SYS_UPD_SRC_ENDPT): 源进程端点                        │
│ m1_i2 (SYS_UPD_DST_ENDPT): 目标进程端点                      │
│ m1_i3 (SYS_UPD_FLAGS): 更新标志                              │
└─────────────────────────────────────────────────────────────┘
```

---

### 2.2 头文件包含

```c
#include "kernel/system.h"
#include <string.h>
#include <assert.h>

#if USE_UPDATE

#define DEBUG 0
```

**逐行解释**：

- **第1行**：`#include "kernel/system.h"` — 内核系统调用核心定义。

- **第2行**：`#include <string.h>` — 字符串操作（如 `memcpy`）。

- **第3行**：`#include <assert.h>` — 断言宏。

- **第5行**：`#if USE_UPDATE` — 条件编译开关。

- **第7行**：`#define DEBUG 0` — 调试开关，设为 0 禁用调试输出。

---

### 2.3 可更新性检查宏

```c
#define proc_is_updatable(p) \
    (RTS_ISSET(p, RTS_NO_PRIV) || RTS_ISSET(p, RTS_SIG_PENDING) \
    || (RTS_ISSET(p, RTS_RECEIVING) && !RTS_ISSET(p, RTS_SENDING)))
```

**逐行解释**：

- **第1-3行**：定义 `proc_is_updatable` 宏，检查进程是否可更新。

进程可更新的条件（满足任一）：
1. `RTS_NO_PRIV`：进程没有权限（正在初始化）。
2. `RTS_SIG_PENDING`：进程有待处理的信号。
3. `RTS_RECEIVING && !RTS_SENDING`：进程正在接收消息（阻塞等待），但没有在发送。

**为什么这些状态可以更新？**

这些状态都是"稳定"状态——进程不会突然改变状态：
- 无权限进程不会运行。
- 等待信号的进程不会主动运行。
- 只接收不发送的进程处于稳定的阻塞状态。

**进程状态图**：

```
进程状态与可更新性:
┌─────────────────────────────────────────────────────────────┐
│ 可更新状态                                                   │
│ ┌─────────────────────────────────────────────────────────┐│
│ │ RTS_NO_PRIV (无权限)                                     ││
│ │ RTS_SIG_PENDING (等待信号)                               ││
│ │ RTS_RECEIVING && !RTS_SENDING (只接收)                   ││
│ └─────────────────────────────────────────────────────────┘│
├─────────────────────────────────────────────────────────────┤
│ 不可更新状态                                                 │
│ ┌─────────────────────────────────────────────────────────┐│
│ │ 可运行 (在就绪队列或运行中)                               ││
│ │ 正在发送 (RTS_SENDING)                                   ││
│ │ 其他活跃状态                                              ││
│ └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

---

### 2.4 静态函数声明

```c
static int inherit_priv_irq(struct proc *src_rp, struct proc *dst_rp);
static int inherit_priv_io(struct proc *src_rp, struct proc *dst_rp);
static int inherit_priv_mem(struct proc *src_rp, struct proc *dst_rp);
static void abort_proc_ipc_send(struct proc *rp);
static void adjust_proc_slot(struct proc *rp, struct proc *from_rp);
static void adjust_priv_slot(struct priv *privp, struct priv *from_privp);
static void adjust_asyn_table(struct priv *src_privp, struct priv *dst_privp);
static void swap_proc_slot_pointer(struct proc **rpp, struct proc
	*src_rp, struct proc *dst_rp);
static void swap_memreq(struct proc *src_rp, struct proc *dst_rp);
```

**逐行解释**：

声明了多个辅助函数：

| 函数 | 功能 |
|------|------|
| `inherit_priv_irq` | 继承中断权限 |
| `inherit_priv_io` | 继承 I/O 权限 |
| `inherit_priv_mem` | 继承内存权限 |
| `abort_proc_ipc_send` | 中止 IPC 发送 |
| `adjust_proc_slot` | 调整进程槽位 |
| `adjust_priv_slot` | 调整权限槽位 |
| `adjust_asyn_table` | 调整异步消息表 |
| `swap_proc_slot_pointer` | 交换进程槽位指针 |
| `swap_memreq` | 交换内存请求 |

---

### 2.5 do_update 函数开头

```c
/*===========================================================================*
 *				do_update				     *
 *===========================================================================*/
int do_update(struct proc * caller, message * m_ptr)
{
/* Handle sys_update(). Update a process into another by swapping their process
 * slots.
 */
  endpoint_t src_e, dst_e;
  int src_p, dst_p, flags;
  struct proc *src_rp, *dst_rp;
  struct priv *src_privp, *dst_privp;
  struct proc orig_src_proc;
  struct proc orig_dst_proc;
  struct priv orig_src_priv;
  struct priv orig_dst_priv;
  int i, r;
```

**逐行解释**：

- **第1-3行**：函数头注释，标准格式。

- **第4行**：`int do_update(struct proc * caller, message * m_ptr)` — 函数签名。

- **第5-7行**：注释说明功能——通过交换进程槽位来更新进程。

- **第8-18行**：局部变量声明：
  - `src_e, dst_e`：源/目标端点号。
  - `src_p, dst_p`：源/目标槽位号。
  - `flags`：更新标志。
  - `src_rp, dst_rp`：源/目标进程指针。
  - `src_privp, dst_privp`：源/目标权限指针。
  - `orig_*`：原始数据备份（用于交换）。
  - `i, r`：循环变量和返回值。

**栈帧布局**：

```
do_update 栈帧:
┌─────────────────────────────────┐ ← 高地址
│ 返回地址                         │
├─────────────────────────────────┤
│ caller, m_ptr (参数)            │
├─────────────────────────────────┤
│ src_e, dst_e (endpoint_t)       │
│ src_p, dst_p, flags (int)       │
│ src_rp, dst_rp (指针)           │
│ src_privp, dst_privp (指针)     │
├─────────────────────────────────┤
│ orig_src_proc (struct proc)     │ ← 较大结构
│ orig_dst_proc (struct proc)     │
│ orig_src_priv (struct priv)     │
│ orig_dst_priv (struct priv)     │
├─────────────────────────────────┤
│ i, r (int)                      │
└─────────────────────────────────┘ ← 低地址 (栈顶)
```

---

### 2.6 源进程验证

```c
  /* Lookup slots for source and destination process. */
  flags = m_ptr->SYS_UPD_FLAGS;
  src_e = m_ptr->SYS_UPD_SRC_ENDPT;
  if(!isokendpt(src_e, &src_p)) {
      return EINVAL;
  }
  src_rp = proc_addr(src_p);
  src_privp = priv(src_rp);
  if(!(src_privp->s_flags & SYS_PROC)) {
      return EPERM;
  }
```

**逐行解释**：

- **第1行**：注释说明查找槽位。

- **第2行**：`flags = m_ptr->SYS_UPD_FLAGS;` — 获取更新标志。

- **第3行**：`src_e = m_ptr->SYS_UPD_SRC_ENDPT;` — 获取源进程端点。

- **第4-6行**：验证源端点：
  - `isokendpt()` 检查端点有效性并转换为槽位号。
  - 如果无效，返回 `EINVAL`。

- **第7行**：`src_rp = proc_addr(src_p);` — 获取进程指针。

- **第8行**：`src_privp = priv(src_rp);` — 获取权限指针。

- **第9-11行**：检查是否是系统进程：
  - `SYS_PROC` 标志表示系统进程。
  - 如果不是系统进程，返回 `EPERM`（权限不足）。

**为什么只允许系统进程？**

- 系统进程（如 VFS、PM、RS）需要热更新功能。
- 普通用户进程不需要此功能。
- 安全考虑：防止用户进程滥用。

---

### 2.7 目标进程验证

```c
  dst_e = m_ptr->SYS_UPD_DST_ENDPT;
  if(!isokendpt(dst_e, &dst_p)) {
      return EINVAL;
  }
  dst_rp = proc_addr(dst_p);
  dst_privp = priv(dst_rp);
  if(!(dst_privp->s_flags & SYS_PROC)) {
      return EPERM;
  }
```

**逐行解释**：

与源进程验证相同，验证目标进程：
1. 端点有效性。
2. 必须是系统进程。

---

### 2.8 可运行性断言

```c
  assert(!proc_is_runnable(src_rp) && !proc_is_runnable(dst_rp));
```

**逐行解释**：

- 断言两个进程都不可运行。
- `proc_is_runnable()` 检查进程是否在就绪队列或正在运行。
- 如果断言失败，内核会崩溃（调试时发现问题）。

**为什么必须不可运行？**

如果进程正在运行，交换其状态会导致：
- 数据不一致。
- 寄存器状态混乱。
- 可能的崩溃。

---

### 2.9 可更新性检查

```c
  /* Check if processes are updatable. */
  if(!proc_is_updatable(src_rp) || !proc_is_updatable(dst_rp)) {
      return EBUSY;
  }
```

**逐行解释**：

- 使用 `proc_is_updatable` 宏检查两个进程。
- 如果任一进程不可更新，返回 `EBUSY`（资源忙）。

---

### 2.10 调试输出

```c
#if DEBUG
  printf("do_update: updating %d (%s, %d, %d) into %d (%s, %d, %d)\n",
      src_rp->p_endpoint, src_rp->p_name, src_rp->p_nr, priv(src_rp)->s_proc_nr,
      dst_rp->p_endpoint, dst_rp->p_name, dst_rp->p_nr, priv(dst_rp)->s_proc_nr);

  proc_stacktrace(src_rp);
  proc_stacktrace(dst_rp);
  printf("do_update: curr ptproc %d\n", get_cpulocal_var(ptproc)->p_endpoint);
  printf("do_update: endpoint %d rts flags %x asyn tab %08x asyn endpoint %d grant tab %08x grant endpoint %d\n", src_rp->p_endpoint, src_rp->p_rts_flags, priv(src_rp)->s_asyntab, priv(src_rp)->s_asynendpoint, priv(src_rp)->s_grant_table, priv(src_rp)->s_grant_endpoint);
  printf("do_update: endpoint %d rts flags %x asyn tab %08x asyn endpoint %d grant tab %08x grant endpoint %d\n", dst_rp->p_endpoint, dst_rp->p_rts_flags, priv(dst_rp)->s_asyntab, priv(dst_rp)->s_asynendpoint, priv(dst_rp)->s_grant_table, priv(dst_rp)->s_grant_endpoint);
#endif
```

**逐行解释**：

- 调试模式下打印详细信息：
  - 进程端点、名称、槽位号。
  - 栈跟踪。
  - 当前页表进程。
  - RTS 标志、异步表、授权表等。

---

### 2.11 继承权限

```c
  /* Let destination inherit allowed IRQ, I/O ranges, and memory ranges. */
  r = inherit_priv_irq(src_rp, dst_rp);
  if(r != OK) {
      return r;
  }
  r = inherit_priv_io(src_rp, dst_rp);
  if(r != OK) {
      return r;
  }
  r = inherit_priv_mem(src_rp, dst_rp);
  if(r != OK) {
      return r;
  }
```

**逐行解释**：

- **第1行**：注释说明让目标进程继承权限。

- **第2-5行**：继承中断权限：
  - `inherit_priv_irq()` 将源进程的中断权限添加到目标进程。

- **第6-9行**：继承 I/O 权限。

- **第10-13行**：继承内存权限。

**权限继承图示**：

```
权限继承:
┌─────────────────────────────────────────────────────────────┐
│ 源进程 (src_rp)                                              │
│ ┌─────────────────────────────────────────────────────────┐│
│ │ 中断权限: IRQ 0, 1, 2                                    ││
│ │ I/O 权限: 端口 0x60-0x64, 0x3F8-0x3FF                    ││
│ │ 内存权限: 0x0000-0xFFFF, 0x10000-0x1FFFF                 ││
│ └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
        │
        │ inherit_priv_*()
        ▼
┌─────────────────────────────────────────────────────────────┐
│ 目标进程 (dst_rp)                                            │
│ ┌─────────────────────────────────────────────────────────┐│
│ │ 中断权限: IRQ 0, 1, 2 (继承)                             ││
│ │ I/O 权限: 端口 0x60-0x64, 0x3F8-0x3FF (继承)             ││
│ │ 内存权限: 0x0000-0xFFFF, 0x10000-0x1FFFF (继承)          ││
│ └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

---

### 2.12 继承 IPC 目标掩码

```c
  /* Let destination inherit the target mask from source. */
  for (i=0; i < NR_SYS_PROCS; i++) {
      if (get_sys_bit(priv(src_rp)->s_ipc_to, i)) {
          set_sendto_bit(dst_rp, i);
      }
  }
```

**逐行解释**：

- **第1行**：注释说明让目标进程继承 IPC 目标掩码。

- **第2-5行**：遍历所有系统进程：
  - `get_sys_bit()` 检查源进程是否可以向进程 i 发送消息。
  - `set_sendto_bit()` 设置目标进程的对应位。

**IPC 目标掩码**：

```
IPC 目标掩码 (s_ipc_to):
┌───┬───┬───┬───┬───┬───┬───┬───┐
│ 0 │ 1 │ 0 │ 1 │ 0 │ 0 │ 1 │...│
└───┴───┴───┴───┴───┴───┴───┴───┘
  ↓   ↓       ↓           ↓
  PM  VFS     VM          RS

位图说明:
- 1: 可以向该进程发送消息
- 0: 不能向该进程发送消息
```

---

### 2.13 保存原始数据

```c
  /* Save existing data. */
  orig_src_proc = *src_rp;
  orig_src_priv = *(priv(src_rp));
  orig_dst_proc = *dst_rp;
  orig_dst_priv = *(priv(dst_rp));
```

**逐行解释**：

- 备份源进程和目标进程的所有数据：
  - `orig_src_proc`：源进程结构副本。
  - `orig_src_priv`：源权限结构副本。
  - `orig_dst_proc`：目标进程结构副本。
  - `orig_dst_priv`：目标权限结构副本。

**为什么需要备份？**

后续交换操作需要原始数据：
1. 交换时需要知道原始值。
2. 调整槽位时需要引用原始值。

---

### 2.14 调整异步表

```c
  /* Adjust asyn tables. */
  adjust_asyn_table(priv(src_rp), priv(dst_rp));
  adjust_asyn_table(priv(dst_rp), priv(src_rp));
```

**逐行解释**：

- 调整两个进程的异步消息表。
- 双向调整，确保异步消息正确传递。

---

### 2.15 中止 IPC 发送

```c
  /* Abort any pending send() on rollback. */
  if(flags & SYS_UPD_ROLLBACK) {
      abort_proc_ipc_send(src_rp);
  }
```

**逐行解释**：

- **第1行**：注释说明在回滚时中止待处理的发送。

- **第2-4行**：检查 `SYS_UPD_ROLLBACK` 标志：
  - 如果设置，调用 `abort_proc_ipc_send()` 中止源进程的待处理发送。
  - `SYS_UPD_ROLLBACK` 定义为 `0x1`。

**回滚场景**：

```
回滚场景:
1. 新版本服务启动失败
2. 需要回滚到旧版本
3. 设置 SYS_UPD_ROLLBACK 标志
4. 中止待处理的消息发送
5. 防止消息发送到错误的进程
```

---

### 2.16 交换槽位

```c
  /* Swap slots. */
  *src_rp = orig_dst_proc;
  *src_privp = orig_dst_priv;
  *dst_rp = orig_src_proc;
  *dst_privp = orig_src_priv;
```

**逐行解释**：

- **第1行**：注释说明交换槽位。

- **第2行**：`*src_rp = orig_dst_proc;` — 源槽位现在存储目标进程数据。

- **第3行**：`*src_privp = orig_dst_priv;` — 源权限槽位存储目标权限数据。

- **第4行**：`*dst_rp = orig_src_proc;` — 目标槽位存储源进程数据。

- **第5行**：`*dst_privp = orig_src_priv;` — 目标权限槽位存储源权限数据。

**交换效果**：

```
交换前:
槽位 src_p:
┌─────────────────────────────────────────────────────────────┐
│ 进程 A 数据                                                  │
│ - p_endpoint = A_endpoint                                   │
│ - p_name = "process_A"                                      │
│ - ...                                                       │
└─────────────────────────────────────────────────────────────┘

槽位 dst_p:
┌─────────────────────────────────────────────────────────────┐
│ 进程 B 数据                                                  │
│ - p_endpoint = B_endpoint                                   │
│ - p_name = "process_B"                                      │
│ - ...                                                       │
└─────────────────────────────────────────────────────────────┘

交换后:
槽位 src_p:
┌─────────────────────────────────────────────────────────────┐
│ 进程 B 数据 (原 dst)                                         │
│ - p_endpoint = B_endpoint                                   │
│ - p_name = "process_B"                                      │
│ - ...                                                       │
└─────────────────────────────────────────────────────────────┘

槽位 dst_p:
┌─────────────────────────────────────────────────────────────┐
│ 进程 A 数据 (原 src)                                         │
│ - p_endpoint = A_endpoint                                   │
│ - p_name = "process_A"                                      │
│ - ...                                                       │
└─────────────────────────────────────────────────────────────┘
```

---

### 2.17 调整进程槽位

```c
  /* Adjust process slots. */
  adjust_proc_slot(src_rp, &orig_src_proc);
  adjust_proc_slot(dst_rp, &orig_dst_proc);
```

**逐行解释**：

- 调整进程槽位内部字段：
  - 保持端点号不变。
  - 保持槽位号不变。
  - 保持权限指针不变。

---

### 2.18 调整权限槽位

```c
  /* Adjust privilege slots. */
  adjust_priv_slot(priv(src_rp), &orig_src_priv);
  adjust_priv_slot(priv(dst_rp), &orig_dst_priv);
```

**逐行解释**：

- 调整权限槽位内部字段：
  - 保持权限 ID 不变。
  - 保持待处理事件不变。

---

### 2.19 交换全局指针

```c
  /* Swap global process slot addresses. */
  swap_proc_slot_pointer(get_cpulocal_var_ptr(ptproc), src_rp, dst_rp);
```

**逐行解释**：

- 交换全局进程指针。
- `ptproc` 是指向当前页表所属进程的指针。
- 如果 `ptproc` 指向源进程，更新为指向目标进程，反之亦然。

**ptproc 的作用**：

```
ptproc (页表进程):
- 指向当前使用的页表所属进程
- 在进程切换时更新
- 交换进程槽位后需要更新此指针
```

---

### 2.20 交换内存请求

```c
  /* Swap VM request entries. */
  swap_memreq(src_rp, dst_rp);
```

**逐行解释**：

- 交换内存请求链表中的进程指针。
- 如果进程正在等待 VM（虚拟内存管理器）处理请求，需要更新链表。

---

### 2.21 调试输出和 TLB 失效

```c
#if DEBUG
  printf("do_update: updated %d (%s, %d, %d) into %d (%s, %d, %d)\n",
      src_rp->p_endpoint, src_rp->p_name, src_rp->p_nr, priv(src_rp)->s_proc_nr,
      dst_rp->p_endpoint, dst_rp->p_name, dst_rp->p_nr, priv(dst_rp)->s_proc_nr);

  proc_stacktrace(src_rp);
  proc_stacktrace(dst_rp);
  printf("do_update: curr ptproc %d\n", get_cpulocal_var(ptproc)->p_endpoint);
  printf("do_update: endpoint %d rts flags %x asyn tab %08x asyn endpoint %d grant tab %08x grant endpoint %d\n", src_rp->p_endpoint, src_rp->p_rts_flags, priv(src_rp)->s_asyntab, priv(src_rp)->s_asynendpoint, priv(src_rp)->s_grant_table, priv(src_rp)->s_grant_endpoint);
  printf("do_update: endpoint %d rts flags %x asyn tab %08x asyn endpoint %d grant tab %08x grant endpoint %d\n", dst_rp->p_endpoint, dst_rp->p_rts_flags, priv(dst_rp)->s_asyntab, priv(dst_rp)->s_asynendpoint, priv(dst_rp)->s_grant_table, priv(dst_rp)->s_grant_endpoint);
#endif

#ifdef CONFIG_SMP
  bits_fill(src_rp->p_stale_tlb, CONFIG_MAX_CPUS);
  bits_fill(dst_rp->p_stale_tlb, CONFIG_MAX_CPUS);
#endif

  return OK;
}
```

**逐行解释**：

- **第1-14行**：调试输出（与之前相同）。

- **第16-19行**：SMP 系统中的 TLB 失效：
  - `CONFIG_SMP` 表示多处理器配置。
  - `p_stale_tlb` 是过期 TLB 位图。
  - `bits_fill()` 填充所有位，表示所有 CPU 的 TLB 都需要刷新。

**为什么需要 TLB 失效？**

在 SMP 系统中：
1. 进程槽位交换后，进程的内存映射可能改变。
2. 其他 CPU 的 TLB 可能缓存了旧的映射。
3. 设置 `p_stale_tlb` 标志，触发 IPI（处理器间中断）刷新 TLB。

---

### 2.22 inherit_priv_irq 函数

```c
/*===========================================================================*
 *			     inherit_priv_irq 				     *
 *===========================================================================*/
int inherit_priv_irq(struct proc *src_rp, struct proc *dst_rp)
{
  int i, r;
  for (i= 0; i<priv(src_rp)->s_nr_irq; i++) {
      r = priv_add_irq(dst_rp, priv(src_rp)->s_irq_tab[i]); 
      if(r != OK) {
          return r;
      }
  }

  return OK;
}
```

**逐行解释**：

- **第1-3行**：函数头注释。

- **第4行**：`int i, r;` — 循环变量和返回值。

- **第5-10行**：遍历源进程的中断表：
  - `s_nr_irq`：中断数量。
  - `s_irq_tab`：中断表数组。
  - `priv_add_irq()`：为目标进程添加中断权限。
  - 如果失败，返回错误。

- **第12行**：`return OK;` — 成功返回。

---

### 2.23 inherit_priv_io 函数

```c
/*===========================================================================*
 *			     inherit_priv_io 				     *
 *===========================================================================*/
 int inherit_priv_io(struct proc *src_rp, struct proc *dst_rp)
{
  int i, r;
  for (i= 0; i<priv(src_rp)->s_nr_io_range; i++) {
      r = priv_add_io(dst_rp, &(priv(src_rp)->s_io_tab[i])); 
      if(r != OK) {
          return r;
      }
  }

  return OK;
}
```

**逐行解释**：

与 `inherit_priv_irq` 类似：
- `s_nr_io_range`：I/O 范围数量。
- `s_io_tab`：I/O 范围表。
- `priv_add_io()`：添加 I/O 权限。

---

### 2.24 inherit_priv_mem 函数

```c
/*===========================================================================*
 *			     inherit_priv_mem 				     *
 *===========================================================================*/
int inherit_priv_mem(struct proc *src_rp, struct proc *dst_rp)
{
  int i, r;
  for (i= 0; i<priv(src_rp)->s_nr_mem_range; i++) {
      r = priv_add_mem(dst_rp, &(priv(src_rp)->s_mem_tab[i])); 
      if(r != OK) {
          return r;
      }
  }

  return OK;
}
```

**逐行解释**：

与前两个函数类似：
- `s_nr_mem_range`：内存范围数量。
- `s_mem_tab`：内存范围表。
- `priv_add_mem()`：添加内存权限。

---

### 2.25 abort_proc_ipc_send 函数

```c
/*===========================================================================*
 *			    abort_proc_ipc_send				     *
 *===========================================================================*/
void abort_proc_ipc_send(struct proc *rp)
{
  if(RTS_ISSET(rp, RTS_SENDING)) {
      struct proc **xpp;
      RTS_UNSET(rp, RTS_SENDING);
      rp->p_misc_flags &= ~MF_SENDING_FROM_KERNEL;
      xpp = &(proc_addr(_ENDPOINT_P(rp->p_sendto_e))->p_caller_q);
      while (*xpp) {
          if(*xpp == rp) {
              *xpp = rp->p_q_link;
              rp->p_q_link = NULL;
              break;
          }
          xpp = &(*xpp)->p_q_link;
      }
  }
}
```

**逐行解释**：

- **第1-3行**：函数头注释。

- **第4行**：`if(RTS_ISSET(rp, RTS_SENDING))` — 检查进程是否正在发送。

- **第5行**：`struct proc **xpp;` — 指向指针的指针，用于遍历队列。

- **第6行**：`RTS_UNSET(rp, RTS_SENDING);` — 清除发送标志。

- **第7行**：`rp->p_misc_flags &= ~MF_SENDING_FROM_KERNEL;` — 清除内核发送标志。

- **第8行**：`xpp = &(proc_addr(_ENDPOINT_P(rp->p_sendto_e))->p_caller_q);` — 获取目标进程的调用者队列头指针。

- **第9-16行**：从队列中移除进程：
  - 遍历调用者队列。
  - 找到当前进程。
  - 将其从队列中移除。

**IPC 发送队列**：

```
调用者队列:
┌─────────────────────────────────────────────────────────────┐
│ 目标进程的 p_caller_q                                        │
│                                                              │
│ ┌─────────┐    ┌─────────┐    ┌─────────┐                   │
│ │ 进程 A  │───→│ 进程 B  │───→│ 进程 C  │───→ NULL          │
│ │p_q_link│    │p_q_link│    │p_q_link│                    │
│ └─────────┘    └─────────┘    └─────────┘                   │
│                                                              │
│ 等待向目标进程发送消息的进程链表                              │
└─────────────────────────────────────────────────────────────┘
```

---

### 2.26 adjust_proc_slot 函数

```c
/*===========================================================================*
 *			     adjust_proc_slot				     *
 *===========================================================================*/
static void adjust_proc_slot(struct proc *rp, struct proc *from_rp)
{
  /* Preserve endpoints, slot numbers, priv structure, and IPC. */
  rp->p_endpoint = from_rp->p_endpoint;
  rp->p_nr = from_rp->p_nr;
  rp->p_priv = from_rp->p_priv;
  priv(rp)->s_proc_nr = from_rp->p_nr;

  rp->p_caller_q = from_rp->p_caller_q;

  /* preserve scheduling */
  rp->p_scheduler = from_rp->p_scheduler;
#ifdef CONFIG_SMP
  rp->p_cpu = from_rp->p_cpu;
  memcpy(rp->p_cpu_mask, from_rp->p_cpu_mask,
		  sizeof(bitchunk_t) * BITMAP_CHUNKS(CONFIG_MAX_CPUS));
#endif
}
```

**逐行解释**：

- **第1-3行**：函数头注释。

- **第4行**：注释说明保持端点、槽位号、权限结构和 IPC。

- **第5行**：`rp->p_endpoint = from_rp->p_endpoint;` — 保持原端点号。

- **第6行**：`rp->p_nr = from_rp->p_nr;` — 保持原槽位号。

- **第7行**：`rp->p_priv = from_rp->p_priv;` — 保持原权限指针。

- **第8行**：`priv(rp)->s_proc_nr = from_rp->p_nr;` — 更新权限结构中的进程号。

- **第10行**：`rp->p_caller_q = from_rp->p_caller_q;` — 保持调用者队列。

- **第12-13行**：注释说明保持调度信息。

- **第14行**：`rp->p_scheduler = from_rp->p_scheduler;` — 保持调度器指针。

- **第15-18行**：SMP 配置下保持 CPU 亲和性：
  - `p_cpu`：当前运行的 CPU。
  - `p_cpu_mask`：允许运行的 CPU 位图。

**调整原则**：

交换后需要保持：
- 端点号（其他进程通过端点号通信）。
- 槽位号（内核内部引用）。
- 权限指针（权限结构不随进程移动）。
- IPC 队列（保持消息传递关系）。
- 调度信息（保持调度策略）。

---

### 2.27 adjust_asyn_table 函数

```c
/*===========================================================================*
 *			     adjust_asyn_table				     *
 *===========================================================================*/
static void adjust_asyn_table(struct priv *src_privp, struct priv *dst_privp)
{
  /* Transfer the asyn table if source's table belongs to the destination. */
  endpoint_t src_e = proc_addr(src_privp->s_proc_nr)->p_endpoint;
  endpoint_t dst_e = proc_addr(dst_privp->s_proc_nr)->p_endpoint;

  if(src_privp->s_asynsize > 0 && dst_privp->s_asynsize > 0 && src_privp->s_asynendpoint == dst_e) {
      if(data_copy(src_e, src_privp->s_asyntab, dst_e, dst_privp->s_asyntab,
	  src_privp->s_asynsize*sizeof(asynmsg_t)) != OK) {
	  printf("Warning: unable to transfer asyn table from ep %d to ep %d\n",
	      src_e, dst_e);
      }
      else {
          dst_privp->s_asynsize = src_privp->s_asynsize;
      }
  }
}
```

**逐行解释**：

- **第1-3行**：函数头注释。

- **第4行**：注释说明如果源进程的异步表属于目标进程，则传输。

- **第5-6行**：获取源和目标进程的端点号。

- **第8-16行**：条件检查和复制：
  - `s_asynsize > 0`：两个进程都有异步表。
  - `s_asynendpoint == dst_e`：源进程的异步表属于目标进程。
  - `data_copy()`：复制异步表数据。
  - 如果失败，打印警告。
  - 如果成功，更新目标进程的异步表大小。

---

### 2.28 adjust_priv_slot 函数

```c
/*===========================================================================*
 *			     adjust_priv_slot				     *
 *===========================================================================*/
static void adjust_priv_slot(struct priv *privp, struct priv *from_privp)
{
  /* Preserve privilege ids and non-privilege stuff in the priv structure. */
  privp->s_id = from_privp->s_id;
  privp->s_asyn_pending = from_privp->s_asyn_pending;
  privp->s_notify_pending = from_privp->s_notify_pending;
  privp->s_int_pending = from_privp->s_int_pending;
  privp->s_sig_pending = from_privp->s_sig_pending;
  privp->s_alarm_timer = from_privp->s_alarm_timer;
  privp->s_diag_sig = from_privp->s_diag_sig;
}
```

**逐行解释**：

- 保持权限结构中的以下字段：
  - `s_id`：权限 ID。
  - `s_asyn_pending`：待处理的异步消息。
  - `s_notify_pending`：待处理的通知。
  - `s_int_pending`：待处理的中断。
  - `s_sig_pending`：待处理的信号。
  - `s_alarm_timer`：闹钟定时器。
  - `s_diag_sig`：诊断信号。

---

### 2.29 swap_proc_slot_pointer 函数

```c
/*===========================================================================*
 *			   swap_proc_slot_pointer			     *
 *===========================================================================*/
static void swap_proc_slot_pointer(struct proc **rpp, struct proc *src_rp,
    struct proc *dst_rp)
{
  if(*rpp == src_rp) {
      *rpp = dst_rp;
  }
  else if(*rpp == dst_rp) {
      *rpp = src_rp;
  }
}
```

**逐行解释**：

- 如果指针指向源进程，更新为指向目标进程。
- 如果指针指向目标进程，更新为指向源进程。
- 否则不做修改。

---

### 2.30 swap_memreq 函数

```c
/*===========================================================================*
 *				swap_memreq				     *
 *===========================================================================*/
static void swap_memreq(struct proc *src_rp, struct proc *dst_rp)
{
/* If either the source or the destination process is part of the VM request
 * chain, but not both, then swap the process pointers in the chain.
 */
  struct proc **rpp;

  if (RTS_ISSET(src_rp, RTS_VMREQUEST) == RTS_ISSET(dst_rp, RTS_VMREQUEST))
	return; /* nothing to do */

  for (rpp = &vmrequest; *rpp != NULL;
     rpp = &(*rpp)->p_vmrequest.nextrequestor) {
	if (*rpp == src_rp) {
		dst_rp->p_vmrequest.nextrequestor =
		    src_rp->p_vmrequest.nextrequestor;
		*rpp = dst_rp;
		break;
	} else if (*rpp == dst_rp) {
		src_rp->p_vmrequest.nextrequestor =
		    dst_rp->p_vmrequest.nextrequestor;
		*rpp = src_rp;
		break;
	}
  }
}

#endif /* USE_UPDATE */
```

**逐行解释**：

- **第1-3行**：函数头注释。

- **第4-5行**：注释说明如果只有一个进程在 VM 请求链中，则交换指针。

- **第6行**：`struct proc **rpp;` — 指向指针的指针。

- **第8-9行**：如果两个进程都在或都不在 VM 请求链中，无需操作。

- **第10-21行**：遍历 VM 请求链：
  - `vmrequest` 是链表头。
  - 找到源进程或目标进程。
  - 交换链表中的指针。

- **第24行**：`#endif` — 条件编译结束。

**VM 请求链**：

```
VM 请求链:
┌─────────────────────────────────────────────────────────────┐
│ vmrequest (链表头)                                           │
│     │                                                        │
│     ▼                                                        │
│ ┌─────────┐    ┌─────────┐    ┌─────────┐                   │
│ │ 进程 A  │───→│ 进程 B  │───→│ 进程 C  │───→ NULL          │
│ │next    │    │next    │    │next    │                    │
│ └─────────┘    └─────────┘    └─────────┘                   │
│                                                              │
│ 等待 VM 处理内存请求的进程链表                                │
└─────────────────────────────────────────────────────────────┘
```

---

## 三、理论关联

### 3.1 进程热更新流程

```
进程热更新流程:
┌─────────────────────────────────────────────────────────────┐
│ 1. 启动新版本进程                                            │
│    - RS 启动新版本服务进程                                   │
│    - 新进程处于 RTS_NO_PRIV 状态                             │
└─────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ 2. 停止旧版本进程                                            │
│    - 发送信号停止旧进程                                      │
│    - 旧进程进入可更新状态                                    │
└─────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ 3. 调用 SYS_UPDATE                                           │
│    - 继承权限                                                │
│    - 交换进程槽位                                            │
│    - 调整内部指针                                            │
└─────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ 4. 恢复新进程                                                │
│    - 新进程接管旧进程的端点号                                │
│    - 新进程继承旧进程的权限                                  │
│    - 新进程开始运行                                          │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 操作系统概念映射

| 代码结构 | 操作系统概念 | 说明 |
|----------|--------------|------|
| `proc_is_updatable` | 进程状态 | 检查进程是否处于稳定状态 |
| `inherit_priv_*` | 权限管理 | 继承中断、I/O、内存权限 |
| `swap_proc_slot_pointer` | 全局状态 | 更新全局进程引用 |
| `p_stale_tlb` | TLB 一致性 | SMP 系统中的 TLB 同步 |
| `SYS_UPD_ROLLBACK` | 故障恢复 | 支持回滚操作 |

---

## 四、Rust 实现与对比

### 4.1 数据结构定义

```rust
#![no_std]

use core::mem::size_of;

#[repr(C)]
pub struct Proc {
    pub p_endpoint: i32,
    pub p_nr: i32,
    pub p_priv: *mut Priv,
    pub p_rts_flags: u32,
    pub p_misc_flags: u32,
    pub p_caller_q: *mut Proc,
    pub p_q_link: *mut Proc,
    pub p_sendto_e: i32,
    pub p_scheduler: *mut Proc,
    // ... 其他字段
}

#[repr(C)]
pub struct Priv {
    pub s_flags: u32,
    pub s_proc_nr: i32,
    pub s_id: i32,
    pub s_nr_irq: i32,
    pub s_irq_tab: [u8; 16],
    pub s_nr_io_range: i32,
    pub s_io_tab: [IoRange; 8],
    pub s_nr_mem_range: i32,
    pub s_mem_tab: [MemRange; 8],
    pub s_ipc_to: [u32; 8],
    pub s_asyn_pending: u32,
    pub s_notify_pending: u64,
    // ... 其他字段
}

pub const RTS_NO_PRIV: u32 = 0x00000100;
pub const RTS_SIG_PENDING: u32 = 0x00000080;
pub const RTS_RECEIVING: u32 = 0x00000001;
pub const RTS_SENDING: u32 = 0x00000002;
pub const RTS_VMREQUEST: u32 = 0x00000040;
pub const SYS_PROC: u32 = 0x00000001;
pub const SYS_UPD_ROLLBACK: i32 = 0x1;
```

### 4.2 错误处理对比

**C 语言版本**：
```c
if(!isokendpt(src_e, &src_p)) {
    return EINVAL;
}
// 问题：错误码可能被忽略
```

**Rust 版本**：
```rust
#[derive(Debug)]
pub enum UpdateError {
    InvalidEndpoint,
    NotSystemProcess,
    ProcessBusy,
    NotRunnable,
    InheritIrqFailed(i32),
    InheritIoFailed(i32),
    InheritMemFailed(i32),
}

fn validate_process(endpt: i32) -> Result<(i32, *mut Proc), UpdateError> {
    let nr = is_ok_endpoint(endpt).ok_or(UpdateError::InvalidEndpoint)?;
    let rp = proc_addr(nr);
    let privp = unsafe { (*rp).p_priv };
    if unsafe { (*privp).s_flags & SYS_PROC == 0 } {
        return Err(UpdateError::NotSystemProcess);
    }
    Ok((nr, rp))
}
```

### 4.3 核心交换逻辑

```rust
#![no_std]

use core::ptr;

pub struct ProcessUpdater;

impl ProcessUpdater {
    pub fn do_update(
        caller: &Proc,
        src_endpt: i32,
        dst_endpt: i32,
        flags: i32,
    ) -> Result<(), UpdateError> {
        let (src_nr, src_rp) = validate_process(src_endpt)?;
        let (dst_nr, dst_rp) = validate_process(dst_endpt)?;
        
        let src_privp = unsafe { (*src_rp).p_priv };
        let dst_privp = unsafe { (*dst_rp).p_priv };
        
        if unsafe { !(*src_privp).s_flags & SYS_PROC != 0 } {
            return Err(UpdateError::NotSystemProcess);
        }
        if unsafe { !(*dst_privp).s_flags & SYS_PROC != 0 } {
            return Err(UpdateError::NotSystemProcess);
        }
        
        assert!(!proc_is_runnable(src_rp) && !proc_is_runnable(dst_rp));
        
        if !proc_is_updatable(src_rp) || !proc_is_updatable(dst_rp) {
            return Err(UpdateError::ProcessBusy);
        }
        
        Self::inherit_priv_irq(src_rp, dst_rp)?;
        Self::inherit_priv_io(src_rp, dst_rp)?;
        Self::inherit_priv_mem(src_rp, dst_rp)?;
        
        Self::inherit_ipc_target_mask(src_rp, dst_rp);
        
        let orig_src_proc = unsafe { *src_rp };
        let orig_src_priv = unsafe { *src_privp };
        let orig_dst_proc = unsafe { *dst_rp };
        let orig_dst_priv = unsafe { *dst_privp };
        
        if flags & SYS_UPD_ROLLBACK != 0 {
            Self::abort_proc_ipc_send(src_rp);
        }
        
        unsafe {
            ptr::write(src_rp, orig_dst_proc);
            ptr::write(src_privp, orig_dst_priv);
            ptr::write(dst_rp, orig_src_proc);
            ptr::write(dst_privp, orig_src_priv);
        }
        
        Self::adjust_proc_slot(src_rp, &orig_src_proc);
        Self::adjust_proc_slot(dst_rp, &orig_dst_proc);
        Self::adjust_priv_slot(src_privp, &orig_src_priv);
        Self::adjust_priv_slot(dst_privp, &orig_dst_priv);
        
        Self::swap_proc_slot_pointer(get_ptproc_ptr(), src_rp, dst_rp);
        Self::swap_memreq(src_rp, dst_rp);
        
        #[cfg(feature = "smp")]
        {
            bits_fill(unsafe { &mut (*src_rp).p_stale_tlb }, CONFIG_MAX_CPUS);
            bits_fill(unsafe { &mut (*dst_rp).p_stale_tlb }, CONFIG_MAX_CPUS);
        }
        
        Ok(())
    }
    
    fn proc_is_updatable(rp: *mut Proc) -> bool {
        unsafe {
            let flags = (*rp).p_rts_flags;
            (flags & RTS_NO_PRIV != 0)
                || (flags & RTS_SIG_PENDING != 0)
                || (flags & RTS_RECEIVING != 0 && flags & RTS_SENDING == 0)
        }
    }
    
    fn adjust_proc_slot(rp: *mut Proc, from_rp: &Proc) {
        unsafe {
            (*rp).p_endpoint = from_rp.p_endpoint;
            (*rp).p_nr = from_rp.p_nr;
            (*rp).p_priv = from_rp.p_priv;
            (*(*rp).p_priv).s_proc_nr = from_rp.p_nr;
            (*rp).p_caller_q = from_rp.p_caller_q;
            (*rp).p_scheduler = from_rp.p_scheduler;
        }
    }
    
    fn swap_proc_slot_pointer(rpp: *mut *mut Proc, src_rp: *mut Proc, dst_rp: *mut Proc) {
        unsafe {
            if *rpp == src_rp {
                *rpp = dst_rp;
            } else if *rpp == dst_rp {
                *rpp = src_rp;
            }
        }
    }
}
```

### 4.4 Rust 优势分析

| 方面 | C 语言 | Rust |
|------|--------|------|
| 类型安全 | 指针直接操作 | 需要 `unsafe` 块显式标记 |
| 错误处理 | 返回码可能被忽略 | `Result<T, E>` 强制处理 |
| 内存安全 | 可能悬垂指针 | 借用检查器防止 |
| 状态管理 | 全局变量 | 结构体封装 |
| 并发安全 | 无保证 | `AtomicPtr` 等同步原语 |

---

## 五、要点总结

### 5.1 核心知识点

1. **进程槽位交换**：通过交换进程结构实现进程热更新，保持端点号和槽位号不变。

2. **权限继承**：新进程继承旧进程的中断、I/O、内存权限和 IPC 目标掩码。

3. **状态一致性**：需要调整全局指针、VM 请求链、TLB 等多个数据结构。

### 5.2 设计亮点

- **条件检查**：确保进程处于可更新状态，防止数据不一致。
- **回滚支持**：`SYS_UPD_ROLLBACK` 标志支持回滚操作。
- **SMP 支持**：自动处理多处理器系统的 TLB 一致性。

---

## 六、灾难预演

### 6.1 如果交换时进程正在运行

**后果**：进程状态不一致。

**现象**：
- 寄存器值与进程结构不匹配。
- 内存映射混乱。
- 内核崩溃。

### 6.2 如果忘记 TLB 失效

**后果**：SMP 系统中其他 CPU 使用旧的地址映射。

**现象**：
- 内存访问错误。
- 数据损坏。
- 随机崩溃。

### 6.3 如果不调整全局指针

**后果**：全局指针指向错误的进程。

**现象**：
- `ptproc` 指向错误的页表。
- 内存访问失败。
- 系统崩溃。

---

## 七、互动自测

### 问题 1：为什么需要备份原始数据？

<details>
<summary>点击查看答案</summary>

交换操作需要原始数据的原因：

1. **交换操作**：交换槽位时需要知道原始值，以便正确赋值。

2. **调整槽位**：调整函数需要引用原始值来保持端点号、槽位号等不变。

3. **原子性**：备份确保交换操作的原子性——要么全部完成，要么全部不完成。

如果不备份，直接交换会导致数据丢失或覆盖。
</details>

### 问题 2：`proc_is_updatable` 检查什么条件？

<details>
<summary>点击查看答案</summary>

进程可更新的条件（满足任一）：

1. `RTS_NO_PRIV`：进程没有权限，正在初始化。
2. `RTS_SIG_PENDING`：进程有待处理的信号，处于稳定的等待状态。
3. `RTS_RECEIVING && !RTS_SENDING`：进程正在接收消息（阻塞等待），但没有在发送。

这些状态都是"稳定"状态——进程不会突然改变状态，可以安全地进行交换操作。
</details>

### 问题 3：为什么需要 TLB 失效？

<details>
<summary>点击查看答案</summary>

在 SMP（多处理器）系统中：

1. 进程槽位交换后，进程的内存映射可能改变。
2. 其他 CPU 的 TLB（转换后备缓冲区）可能缓存了旧的虚拟地址到物理地址的映射。
3. 如果不刷新 TLB，其他 CPU 可能使用旧的映射访问错误的物理地址。

设置 `p_stale_tlb` 标志后，内核会发送 IPI（处理器间中断）通知其他 CPU 刷新 TLB，确保地址映射的一致性。
</details>
