# kernel/system/do_statectl.c 逐行讲解

**文件路径**: `minix3/minix/kernel/system/do_statectl.c`

**总行数**: 53 行

**作用**: 实现 `SYS_STATECTL` 系统调用，处理多种进程状态控制请求

---

## 1. 头文件和注释

```c
/* The kernel call implemented in this file:
 *   m_type:	SYS_STATECTL
 *
 * The parameters for this kernel call are:
 *   m_lsys_krn_sys_statectl.request	(state control request)
 */

#include "kernel/system.h"
```

**SYS_STATECTL 是什么？**

```
STATECTL = STATE ConTroL（状态控制）

这是一个多功能的系统调用：
- 不像 do_runctl 只有一个功能（停止/恢复）
- statectl 有多个子功能
- 通过 request 字段区分

类似厨房的多功能料理机：
┌─────────────────────────────────────────────────────────────────────┐
│  do_runctl  = 单功能机器（只能搅拌）                                  │
│  do_statectl = 多功能机器（搅拌、切菜、研磨...）                      │
│                                                                     │
│  通过 request 参数选择功能：                                         │
│  - SYS_STATE_CLEAR_IPC_REFS      → 清理 IPC 引用                    │
│  - SYS_STATE_SET_STATE_TABLE      → 设置状态表                      │
│  - SYS_STATE_ADD_IPC_BL_FILTER    → 添加黑名单                      │
│  - SYS_STATE_ADD_IPC_WL_FILTER    → 添加白名单                      │
│  - SYS_STATE_CLEAR_IPC_FILTERS    → 清除过滤器                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 2. 函数签名

```c
int do_statectl(struct proc * caller, message * m_ptr)
```

**参数说明**：

| 参数 | 类型 | 含义 |
|------|------|------|
| `caller` | struct proc * | 调用者进程（谁在请求） |
| `m_ptr` | message * | 消息指针（包含请求参数） |

**注意**：没有提取 target endpoint！因为这个系统调用是**让调用者操作自己**。

```
do_runctl：
- 操作其他进程
- 需要 target endpoint

do_statectl：
- 操作调用者自己
- 不需要 target endpoint
```

---

## 3. 分派请求

```c
  switch(m_ptr->m_lsys_krn_sys_statectl.request)
  {
```

**request 字段的类型**：

```
m_lsys_krn_sys_statectl 是消息的联合体（union）

┌─────────────────────────────────────────────────────────────────────┐
│  struct {                                                            │
│      int request;           ← 请求类型                             │
│      vir_bytes address;     ← 地址（用于过滤器）                   │
│      size_t length;         ← 长度（用于过滤器）                    │
│  } m_lsys_krn_sys_statectl;                                          │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 4. 子功能 1：清除 IPC 引用

```c
  case SYS_STATE_CLEAR_IPC_REFS:
	/* Clear IPC references for all the processes communicating
	 * with the caller.
	 */
	clear_ipc_refs(caller, EDEADSRCDST);
	return(OK);
```

**IPC 引用是什么？**

```
当进程 A 和进程 B 通信时：

┌─────────────────────────────────────────────────────────────────────┐
│  进程 A                        进程 B                                │
│     │                            │                                  │
│     │  send(B, msg)             │                                  │
│     │ ─────────────────────────► │                                  │
│     │                            │                                  │
│     │  A 的 PCB 中记录：          │                                  │
│     │  "我正在和 B 通信"         │                                  │
│     │                            │                                  │
│     │                            │  B 的 PCB 中记录：               │
│     │                            │  "A 正在和我通信"                 │
│     │                            │                                  │
└─────────────────────────────────────────────────────────────────────┘

这些记录就是"IPC 引用"

当进程 A 退出时：
- 需要清理这些引用
- 否则 B 不知道 A 已经退出
- B 继续等待 A 的回复 → 死锁
```

**EDEADSRCDST 是什么？**

```
EDEADSRCDST = DEAD SRoC + DEAD DSTination

错误码，表示"源和目标都已死亡"

用于 IPC 引用清理：
- 当清除引用时
- 如果发现对方进程已不存在
- 返回这个特殊错误码
- 让接收方知道通信对象已死亡
```

**为什么要清除自己所有的 IPC 引用？**

```
场景：进程 A 正在和 B、C、D 通信

┌─────────────────────────────────────────────────────────────────────┐
│  进程 A                                                           │
│    │                                                                │
│    ├────► 进程 B（等待 B 的回复）                                   │
│    ├────► 进程 C（正在发送消息）                                     │
│    └────► 进程 D（已收到 D 的消息）                                  │
└─────────────────────────────────────────────────────────────────────┘

当 A 要退出时：
1. 如果不清理 IPC 引用：
   - B、C、D 以为 A 还活着
   - B、C、D 继续等待 → 死锁

2. 如果清理 IPC 引用：
   - B、C、D 收到通知（A 已死亡）
   - 可以采取相应措施
   - 避免死锁
```

---

## 5. 子功能 2：设置状态表

```c
  case SYS_STATE_SET_STATE_TABLE:
	/* Set state table for the caller. */
	priv(caller)->s_state_table = (vir_bytes) m_ptr->m_lsys_krn_sys_statectl.address;
	priv(caller)->s_state_entries = m_ptr->m_lsys_krn_sys_statectl.length;
	return(OK);
```

**状态表是什么？**

```
状态表 = State Table

用于管理进程的状态转换

┌─────────────────────────────────────────────────────────────────────┐
│  状态表示例：                                                        │
│  ┌─────────────┬─────────────┬─────────────┐                       │
│  │  当前状态    │   事件       │   下一状态   │                       │
│  ├─────────────┼─────────────┼─────────────┤                       │
│  │  IDLE       │ 收到消息     │  READY      │                       │
│  │  READY      │ 调度选中     │  RUNNING    │                       │
│  │  RUNNING    │ 时间片用完    │  READY      │                       │
│  │  RUNNING    │ 等待 I/O     │  BLOCKED    │                       │
│  │  BLOCKED    │ I/O 完成     │  READY      │                       │
│  └─────────────┴─────────────┴─────────────┘                       │
└─────────────────────────────────────────────────────────────────────┘

priv(caller)->s_state_table：
- 指向状态表的指针
- vir_bytes = 虚拟地址（用户空间地址）

priv(caller)->s_state_entries：
- 状态表条目数量
```

**为什么要设置状态表？**

```
场景：进程需要自定义状态转换逻辑

某些特殊进程（如网络协议栈）可能需要：
- 更复杂的状态机
- 不只是简单的 BLOCKED/READY/RUNNING

通过设置状态表：
- 进程告诉内核自己的状态转换规则
- 内核按照自定义规则管理进程状态
```

**priv(caller) 是什么？**

```
priv = Privilege（特权）

每个进程有一个 privilege 结构：

┌─────────────────────────────────────────────────────────────────────┐
│  struct priv {                                                      │
│      ...                                                            │
│      vir_bytes s_state_table;     // 状态表地址                     │
│      int s_state_entries;         // 状态表条目数                   │
│      ...                                                            │
│  };                                                                 │
└─────────────────────────────────────────────────────────────────────┘

priv(caller) = 获取调用者的特权结构指针
```

---

## 6. 子功能 3：添加 IPC 黑名单

```c
  case SYS_STATE_ADD_IPC_BL_FILTER:
	/* Add an IPC blacklist filter for the caller. */
	return add_ipc_filter(caller, IPCF_BLACKLIST,
	    (vir_bytes) m_ptr->m_lsys_krn_sys_statectl.address,
	    m_ptr->m_lsys_krn_sys_statectl.length);
```

**IPC 黑名单是什么？**

```
黑名单 = Blacklist = 禁止列表

┌─────────────────────────────────────────────────────────────────────┐
│  进程 A 的 IPC 黑名单：                                             │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │  禁止和进程 5 通信   ← 进程 5 在黑名单中                         │ │
│  │  禁止和进程 10 通信  ← 进程 10 在黑名单中                        │ │
│  └─────────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  如果 A 尝试和 5 或 10 通信：                                        │
│  - 内核拒绝                                                         │
│  - 返回错误                                                         │
└─────────────────────────────────────────────────────────────────────┘
```

**使用场景**：

```
安全隔离：
┌─────────────────────────────────────────────────────────────────────┐
│  恶意软件防护：                                                      │
│                                                                     │
│  假设进程 A 是安全关键进程                                            │
│  - 不希望被恶意软件攻击                                               │
│  - 设置黑名单：禁止和已知恶意软件通信                                 │
│                                                                     │
│  隔离保护：                                                          │
│  - 某些敏感进程不希望与网络进程通信                                   │
│  - 防止数据泄露                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 7. 子功能 4：添加 IPC 白名单

```c
  case SYS_STATE_ADD_IPC_WL_FILTER:
	/* Add an IPC whitelist filter for the caller. */
	return add_ipc_filter(caller, IPCF_WHITELIST,
	    (vir_bytes) m_ptr->m_lsys_krn_sys_statectl.address,
	    m_ptr->m_lsys_krn_sys_statectl.length);
```

**IPC 白名单是什么？**

```
白名单 = Whitelist = 允许列表

┌─────────────────────────────────────────────────────────────────────┐
│  进程 A 的 IPC 白名单：                                              │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │  只允许和进程 5 通信   ← 进程 5 在白名单中                        │ │
│  │  只允许和进程 10 通信  ← 进程 10 在白名单中                       │ │
│  └─────────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  如果 A 尝试和其他进程通信：                                          │
│  - 内核拒绝（不在白名单中）                                           │
│  - 返回错误                                                         │
└─────────────────────────────────────────────────────────────────────┘
```

**使用场景**：

```
最小权限原则：
┌─────────────────────────────────────────────────────────────────────┐
│  栗色安全模型（最小权限）：                                           │
│                                                                     │
│  默认：禁止所有通信                                                   │
│  白名单：只允许与必要的进程通信                                       │
│                                                                     │
│  示例：                                                              │
│  - 文件服务器只需要和 VFS 通信                                        │
│  - 不需要和其他用户进程直接通信                                       │
│  - 白名单限制只和 VFS 通信                                           │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 8. 子功能 5：清除 IPC 过滤器

```c
  case SYS_STATE_CLEAR_IPC_FILTERS:
	/* Clear any IPC filter for the caller. */
	clear_ipc_filters(caller);
	return OK;
```

**为什么要清除过滤器？**

```
过滤器管理：

┌─────────────────────────────────────────────────────────────────────┐
│  初始状态：                                                          │
│  - 没有过滤器                                                        │
│  - 可以和任何进程通信                                                │
│                                                                     │
│  添加黑名单：                                                        │
│  - 添加后，只能和不在黑名单中的进程通信                                │
│                                                                     │
│  添加白名单：                                                        │
│  - 添加后，只能和白名单中的进程通信                                    │
│                                                                     │
│  清除过滤器：                                                        │
│  - 恢复到初始状态                                                    │
│  - 可以和任何进程通信                                                │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 9. 默认处理

```c
  default:
	printf("do_statectl: bad request %d\n",
		m_ptr->m_lsys_krn_sys_statectl.request);
	return EINVAL;
```

**为什么要 printf？**

```
这是内核代码，不能用用户空间的日志系统
printf 直接输出到控制台

为什么要打印错误的 request 值？
- 帮助调试
- 如果有未知请求，输出日志
- 方便定位问题
```

---

## 10. 完整流程图

```
┌─────────────────────────────────────────────────────────────────────┐
│                    do_statectl 完整流程                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  开始                                                                │
│    │                                                                │
│    ▼                                                                │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  switch(request)  ← 根据请求类型分派                         │   │
│  └─────────────────────────────────────────────────────────────┘   │
│    │                                                                │
│    ├───────────────────────────────────────────────────────────    │
│    │                                                               │
│    ▼                                                               │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐       │
│  │ CLEAR_IPC_REFS │  │SET_STATE_TABLE│  │ ADD_IPC_BL     │       │
│  ├────────────────┤  ├────────────────┤  ├────────────────┤       │
│  │clear_ipc_refs │  │ 设置状态表     │  │add_ipc_filter │       │
│  │  (caller)      │  │               │  │  BLACKLIST    │       │
│  └────────────────┘  └────────────────┘  └────────────────┘       │
│                                                     │              │
│                                                     ▼              │
│                        ┌────────────────┐  ┌────────────────┐       │
│                        │ADD_IPC_WL      │  │CLEAR_FILTERS  │       │
│                        ├────────────────┤  ├────────────────┤       │
│                        │add_ipc_filter │  │clear_ipc_    │       │
│                        │  WHITELIST    │  │  filters      │       │
│                        └────────────────┘  └────────────────┘       │
│                                                     │              │
│                                                     ▼              │
│                                              return OK              │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 11. 使用场景总结

| 子功能 | 使用场景 |
|--------|----------|
| CLEAR_IPC_REFS | 进程退出时清理通信引用 |
| SET_STATE_TABLE | 设置进程状态转换表 |
| ADD_IPC_BL_FILTER | 安全隔离，禁止与某些进程通信 |
| ADD_IPC_WL_FILTER | 最小权限，只允许与特定进程通信 |
| CLEAR_IPC_FILTERS | 移除所有通信限制 |

---

## 12. IPC 过滤器详解

**过滤器结构**：

```
┌─────────────────────────────────────────────────────────────────────┐
│  IPCF_BLACKLIST vs IPCF_WHITELIST：                                 │
│                                                                     │
│  黑名单模型（默认允许）：                                             │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │  规则：禁止与名单中的进程通信                                     │ │
│  │  其他：允许                                                     │ │
│  │                                                                 │ │
│  │  示例：禁止和恶意软件通信                                        │ │
│  └─────────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  白名单模型（默认禁止）：                                             │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │  规则：只允许与名单中的进程通信                                   │ │
│  │  其他：禁止                                                     │ │
│  │                                                                 │ │
│  │  示例：最小权限原则                                              │ │
│  └─────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 13. 要点总结

1. **多子功能**：通过 request 字段分派到不同处理函数
2. **IPC 引用清理**：进程退出时清理与其他进程的通信引用
3. **状态表**：管理进程状态转换规则
4. **IPC 过滤器**：黑名单/白名单控制通信权限

---

## 14. 灾难预演

### 如果不清除 IPC 引用

```
后果：
1. 正在通信的进程不知道对方已退出
2. 等待回复的进程永远阻塞
3. 死锁
```

### 如果白名单设置错误

```
后果：
1. 进程无法和必要的服务通信
2. 系统调用失败
3. 应用程序崩溃
```

### 如果让任何进程都能设置状态表

```
后果：
1. 恶意进程修改状态转换规则
2. 可能绕过安全检查
3. 系统不安全
```

---

## 15. 互动自测

1. **问题**：do_statectl 和 do_runctl 的区别？
   **答案**：do_runctl 操作其他进程（需要 endpoint），do_statectl 操作调用者自己。

2. **问题**：为什么要清除 IPC 引用？
   **答案**：避免其他进程因为不知道对方已退出而永久等待（死锁）。

3. **问题**：IPC 黑名单和白名单的区别？
   **答案**：黑名单默认允许（禁止特定），白名单默认禁止（只允许特定）。

4. **问题**：EDEADSRCDST 是什么意思？
   **答案**：源和目标都已死亡，用于 IPC 引用清理时表示通信双方都不存在了。

5. **问题**：谁可以调用 do_statectl？
   **答案**：任何进程都可以调用，但只能操作自己的状态（不能操作其他进程）。

---

## 16. Rust 重构建议

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum StateCtlRequest {
    ClearIpcRefs = SYS_STATE_CLEAR_IPC_REFS,
    SetStateTable = SYS_STATE_SET_STATE_TABLE,
    AddIpcBlFilter = SYS_STATE_ADD_IPC_BL_FILTER,
    AddIpcWlFilter = SYS_STATE_ADD_IPC_WL_FILTER,
    ClearIpcFilters = SYS_STATE_CLEAR_IPC_FILTERS,
}

pub enum StateCtlError {
    InvalidRequest,
    FilterError(FilterError),
}

pub fn do_statectl(
    caller: &Proc,
    request: &StateCtlRequest,
    address: VirtAddr,
    length: usize,
) -> Result<(), StateCtlError> {
    match request {
        StateCtlRequest::ClearIpcRefs => {
            clear_ipc_refs(caller, ErrCode::DeadSrcDst);
        }
        StateCtlRequest::SetStateTable => {
            let priv_data = caller.privilege_mut();
            priv_data.set_state_table(address, length);
        }
        StateCtlRequest::AddIpcBlFilter => {
            add_ipc_filter(caller, FilterType::Blacklist, address, length)
                .map_err(StateCtlError::FilterError)?;
        }
        StateCtlRequest::AddIpcWlFilter => {
            add_ipc_filter(caller, FilterType::Whitelist, address, length)
                .map_err(StateCtlError::FilterError)?;
        }
        StateCtlRequest::ClearIpcFilters => {
            clear_ipc_filters(caller);
        }
    }
    Ok(())
}
```

---

**文档版本**: 2026-03-30