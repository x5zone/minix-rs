# servers/pm/getset.c 逐行讲解

> **文件路径**: `minix3/minix/servers/pm/getset.c`
> **核心功能**: 进程身份相关的 get/set 系统调用实现
> **代码行数**: 223 行

---

## 文件概述

### 是什么（功能说明）

这个文件实现了与进程身份（UID/GID/PID）相关的系统调用，分为两大类：

**获取类系统调用** (`do_get`):
1. `getuid()` / `geteuid()`: 获取用户 ID
2. `getgid()` / `getegid()`: 获取组 ID
3. `getpid()` / `getppid()`: 获取进程 ID
4. `getpgrp()`: 获取进程组 ID
5. `getsid()`: 获取会话 ID
6. `getgroups()`: 获取补充组列表
7. `issetugid()`: 检查进程是否被"污染"

**设置类系统调用** (`do_set`):
1. `setuid()` / `seteuid()`: 设置用户 ID
2. `setgid()` / `setegid()`: 设置组 ID
3. `setgroups()`: 设置补充组列表
4. `setsid()`: 创建新会话

### 为什么（设计原因）

**UNIX 权限模型**：每个进程有三组用户/组 ID：

| 类型 | Real ID | Effective ID | Saved ID |
|------|---------|--------------|----------|
| 用户 | `mp_realuid` | `mp_effuid` | `mp_svuid` |
| 组 | `mp_realgid` | `mp_effgid` | `mp_svgid` |

**三种 ID 的作用**:
- **Real ID**: 标识"谁启动了这个进程"（登录用户）
- **Effective ID**: 用于权限检查（文件访问、系统调用）
- **Saved ID**: 保存之前的 Effective ID，用于权限切换

**PM 与 VFS 协作**: 设置 ID 需要同步更新 PM 和 VFS 两边的状态：
- PM 管理进程表中的 ID 字段
- VFS 管理文件访问权限检查

### 什么情景使用（应用场景）

| 系统调用 | 使用场景 |
|---------|---------|
| `getpid()` | 创建唯一文件名、日志记录 |
| `setuid()` | 程序降权（如 Web 服务器启动后切换到 nobody 用户）|
| `setsid()` | 创建守护进程（脱离控制终端）|
| `getgroups()` | 检查用户所属组 |

---

## 头文件分析

```c
/* This file handles the 6 system calls that get and set uids and gids.
 * It also handles getpid(), setsid(), and getpgrp().  The code for each
 * one is so tiny that it hardly seemed worthwhile to make each a separate
 * function.
 */
```

**注释翻译**: "这个文件处理获取和设置 uid 和 gid 的 6 个系统调用。它还处理 getpid()、setsid() 和 getpgrp()。每个的代码都很小，几乎不值得为每个单独创建函数。"

**设计思路讲解**: 
- 多个相关系统调用共享一个处理函数
- 通过 `call_nr` 区分具体操作
- 减少代码重复，提高维护效率

```c
#include "pm.h"
```
**讲解**: 包含 PM 模块主头文件。

```c
#include <minix/callnr.h>
```
**讲解**: 包含系统调用号定义，如 `PM_GETUID`、`PM_SETUID` 等。

```c
#include <minix/endpoint.h>
```
**讲解**: 包含端点类型定义。

```c
#include <limits.h>
```
**讲解**: 包含系统限制常量，如 `NGROUPS_MAX`（16）。

```c
#include <minix/com.h>
```
**讲解**: 包含 IPC 消息类型定义，如 `VFS_PM_SETUID`。

```c
#include <signal.h>
```
**讲解**: 包含信号相关定义。

```c
#include "mproc.h"
```
**讲解**: 包含进程控制块定义。关键字段：
- `mp_realuid`, `mp_effuid`, `mp_svuid`: 用户 ID
- `mp_realgid`, `mp_effgid`, `mp_svgid`: 组 ID
- `mp_ngroups`: 补充组数量
- `mp_sgroups[NGROUPS_MAX]`: 补充组数组
- `mp_procgrp`: 进程组 ID
- `mp_pid`: 进程 ID
- `mp_parent`: 父进程索引

---

## 函数详解

### do_get 函数

```c
/*===========================================================================*
 *				do_get					     *
 *===========================================================================*/
int
do_get(void)
{
```

**功能说明**: 处理所有"获取"类系统调用。

```c
/* Handle PM_GETUID, PM_GETGID, PM_GETGROUPS, PM_GETPID, PM_GETPGRP, PM_GETSID,
 * PM_ISSETUGID.
 */
  register struct mproc *rmp = mp;
  int r;
  int ngroups;
```

**注释翻译**: "处理 PM_GETUID、PM_GETGID、PM_GETGROUPS、PM_GETPID、PM_GETPGRP、PM_GETSID、PM_ISSETUGID。"

**讲解**: 声明局部变量：
- `rmp`: 指向当前进程的 mproc 结构
- `r`: 返回值
- `ngroups`: 补充组数量

**全局变量 `mp`**: 指向调用进程的 mproc 结构，由 PM 主循环设置。

```c
  switch(call_nr) {
	case PM_GETGROUPS:
		ngroups = m_in.m_lc_pm_groups.num;
		if (ngroups > NGROUPS_MAX || ngroups < 0)
			return(EINVAL);
```

**讲解**: 处理 `getgroups()` 系统调用。

**参数验证**:
- `ngroups`: 用户提供的缓冲区大小
- 必须在 0 到 `NGROUPS_MAX`（16）之间

```c
		if (ngroups == 0) {
			r = rmp->mp_ngroups;
			break;
		}
```
**讲解**: 如果 `ngroups` 为 0，只返回补充组数量，不复制数据。

**POSIX 语义**: `getgroups(0, NULL)` 返回组数量，用于确定缓冲区大小。

```c
		if (ngroups < rmp->mp_ngroups)
			/* Asking for less groups than available */
			return(EINVAL);
```

**注释翻译**: "请求的组数少于可用组数"

**讲解**: 缓冲区太小，返回错误。

```c
		r = sys_datacopy(SELF, (vir_bytes) rmp->mp_sgroups, who_e,
			m_in.m_lc_pm_groups.ptr, ngroups * sizeof(gid_t));

		if (r != OK)
			return(r);

		r = rmp->mp_ngroups;
		break;
```
**讲解**: 复制补充组列表到用户空间：
- `SELF`: 从 PM 自己的地址空间
- `who_e`: 到调用进程的地址空间
- 返回实际组数量

**内存布局**:
```
PM 地址空间                    用户地址空间
+------------------+           +------------------+
| mp_sgroups[16]   |  copy     | 用户缓冲区        |
| [gid0, gid1, ...]| --------> | [gid0, gid1, ...]|
+------------------+           +------------------+
```

```c
	case PM_GETUID:
		r = rmp->mp_realuid;
		rmp->mp_reply.m_pm_lc_getuid.euid = rmp->mp_effuid;
		break;
```
**讲解**: 处理 `getuid()` / `geteuid()` 系统调用。
- 返回值 `r`: real UID
- 通过回复消息返回 effective UID

**getuid() 库函数实现**:
```c
uid_t getuid(void) {
    message m;
    _syscall(PM_PROC_NR, PM_GETUID, &m);
    return m.m_pm_lc_getuid.uid;  // real uid
}

uid_t geteuid(void) {
    message m;
    _syscall(PM_PROC_NR, PM_GETUID, &m);
    return m.m_pm_lc_getuid.euid;  // effective uid
}
```

```c
	case PM_GETGID:
		r = rmp->mp_realgid;
		rmp->mp_reply.m_pm_lc_getgid.egid = rmp->mp_effgid;
		break;
```
**讲解**: 处理 `getgid()` / `getegid()` 系统调用。与 `getuid()` 类似。

```c
	case PM_GETPID:
		r = mproc[who_p].mp_pid;
		rmp->mp_reply.m_pm_lc_getpid.parent_pid = mproc[rmp->mp_parent].mp_pid;
		break;
```
**讲解**: 处理 `getpid()` / `getppid()` 系统调用。
- `who_p`: 调用进程在进程表中的索引
- `mp_pid`: 进程 ID
- `mp_parent`: 父进程索引
- 通过回复消息返回父进程 ID

**为什么用 `mproc[who_p]` 而不是 `rmp`**: 
- `rmp = mp` 已经指向当前进程
- 这里是为了代码清晰，两者等价

```c
	case PM_GETPGRP:
		r = rmp->mp_procgrp;
		break;
```
**讲解**: 处理 `getpgrp()` 系统调用。
- 返回进程组 ID
- 进程组 ID 通常等于进程组组长的 PID

```c
	case PM_GETSID:
	{
		struct mproc *target;
		pid_t p = m_in.m_lc_pm_getsid.pid;
		target = p ? find_proc(p) : &mproc[who_p];
		r = ESRCH;
		if(target)
			r = target->mp_procgrp;
		break;
	}
```
**讲解**: 处理 `getsid()` 系统调用。

**参数**:
- `p`: 目标进程 PID
- 如果 `p == 0`，查询当前进程的会话 ID

**逻辑**:
1. 如果 `p` 非零，查找对应进程
2. 如果 `p` 为零，使用当前进程
3. 找到进程则返回其进程组 ID（会话 ID）
4. 找不到返回 `ESRCH`（进程不存在）

**会话与进程组的关系**: 在 Minix 中，会话 ID 等于会话组长的进程组 ID。

```c
	case PM_ISSETUGID:
		r = !!(rmp->mp_flags & TAINTED);
		break;
```
**讲解**: 处理 `issetugid()` 系统调用。

**TAINTED 标志**: 
- 当进程通过 setuid/setgid 程序启动时设置
- 表示进程可能具有提升的权限
- 用于安全检查

**返回值**:
- 1: 进程被"污染"（需要额外安全检查）
- 0: 进程未被污染

```c
	default:
		r = EINVAL;
		break;
  }
  return(r);
}
```
**讲解**: 未知调用号返回 `EINVAL`。

---

### do_set 函数

```c
/*===========================================================================*
 *				do_set					     *
 *===========================================================================*/
int
do_set(void)
{
```

**功能说明**: 处理所有"设置"类系统调用。

```c
/* Handle PM_SETUID, PM_SETEUID, PM_SETGID, PM_SETGROUPS, PM_SETEGID, and
 * SETSID. These calls have in common that, if successful, they will be
 * forwarded to VFS as well.
 */
  register struct mproc *rmp = mp;
  message m;
  int r, i;
  int ngroups;
  uid_t uid;
  gid_t gid;
```

**注释翻译**: "处理 PM_SETUID、PM_SETEUID、PM_SETGID、PM_SETGROUPS、PM_SETEGID 和 SETSID。这些调用的共同点是，如果成功，它们也会被转发给 VFS。"

**设计思路讲解**: 
- 设置操作需要同步 PM 和 VFS 两边的状态
- PM 先更新本地状态，再通知 VFS
- 使用 `SUSPEND` 返回值，等待 VFS 完成后再回复用户

```c
  memset(&m, 0, sizeof(m));
```
**讲解**: 初始化消息结构，清零所有字段。

```c
  switch(call_nr) {
	case PM_SETUID:
		uid = m_in.m_lc_pm_setuid.uid;
```
**讲解**: 处理 `setuid()` 系统调用。获取目标 UID。

```c
		/* NetBSD specific semantics: setuid(geteuid()) may fail. */
		if (rmp->mp_realuid != uid && rmp->mp_effuid != SUPER_USER)
			return(EPERM);
```

**注释翻译**: "NetBSD 特定语义：setuid(geteuid()) 可能失败。"

**权限检查**:
- 如果新 UID 等于 real UID，允许
- 如果当前进程是超级用户，允许
- 否则拒绝

**POSIX vs BSD 语义**:
- POSIX: `setuid(uid)` 只需要 `uid` 等于 real/effective/saved UID 之一
- BSD: 更严格的语义

```c
		/* BSD semantics: always update all three fields. */
		rmp->mp_realuid = uid;
		rmp->mp_effuid = uid;
		rmp->mp_svuid = uid;
```

**注释翻译**: "BSD 语义：总是更新所有三个字段。"

**讲解**: BSD 语义下，`setuid()` 同时设置 real、effective、saved UID。

**为什么这样设计**: 简化权限管理，避免复杂的权限切换。

```c
		m.m_type = VFS_PM_SETUID;
		m.VFS_PM_ENDPT = rmp->mp_endpoint;
		m.VFS_PM_EID = rmp->mp_effuid;
		m.VFS_PM_RID = rmp->mp_realuid;

		break;
```
**讲解**: 构造发给 VFS 的消息：
- 消息类型：`VFS_PM_SETUID`
- 目标进程端点
- 新的 effective UID 和 real UID

```c
	case PM_SETEUID:
		uid = m_in.m_lc_pm_setuid.uid;
		/* BSD semantics: seteuid(geteuid()) may fail. */
		if (rmp->mp_realuid != uid && rmp->mp_svuid != uid &&
		    rmp->mp_effuid != SUPER_USER)
			return(EPERM);
		rmp->mp_effuid = uid;
```
**讲解**: 处理 `seteuid()` 系统调用。

**权限检查**:
- 新 UID 必须等于 real/saved/effective UID 之一
- 或者当前进程是超级用户

**与 setuid() 的区别**:
- `seteuid()` 只修改 effective UID
- `setuid()` 修改所有三种 UID

```c
		m.m_type = VFS_PM_SETUID;
		m.VFS_PM_ENDPT = rmp->mp_endpoint;
		m.VFS_PM_EID = rmp->mp_effuid;
		m.VFS_PM_RID = rmp->mp_realuid;

		break;
```
**讲解**: 构造 VFS 消息（与 `setuid()` 相同）。

```c
	case PM_SETGID:
		gid = m_in.m_lc_pm_setgid.gid;
		if (rmp->mp_realgid != gid && rmp->mp_effuid != SUPER_USER)
			return(EPERM);
		rmp->mp_realgid = gid;
		rmp->mp_effgid = gid;
		rmp->mp_svgid = gid;

		m.m_type = VFS_PM_SETGID;
		m.VFS_PM_ENDPT = rmp->mp_endpoint;
		m.VFS_PM_EID = rmp->mp_effgid;
		m.VFS_PM_RID = rmp->mp_realgid;

		break;
```
**讲解**: 处理 `setgid()` 系统调用。与 `setuid()` 类似，但操作的是组 ID。

**权限检查**: 使用 `mp_effuid` 检查是否是超级用户（不是 `mp_effgid`）。

```c
	case PM_SETEGID:
		gid = m_in.m_lc_pm_setgid.gid;
		if (rmp->mp_realgid != gid && rmp->mp_svgid != gid &&
		    rmp->mp_effuid != SUPER_USER)
			return(EPERM);
		rmp->mp_effgid = gid;

		m.m_type = VFS_PM_SETGID;
		m.VFS_PM_ENDPT = rmp->mp_endpoint;
		m.VFS_PM_EID = rmp->mp_effgid;
		m.VFS_PM_RID = rmp->mp_realgid;

		break;
```
**讲解**: 处理 `setegid()` 系统调用。与 `seteuid()` 类似。

```c
	case PM_SETGROUPS:
		if (rmp->mp_effuid != SUPER_USER)
			return(EPERM);
```
**讲解**: 处理 `setgroups()` 系统调用。

**权限检查**: 只有超级用户可以设置补充组。

```c
		ngroups = m_in.m_lc_pm_groups.num;

		if (ngroups > NGROUPS_MAX || ngroups < 0)
			return(EINVAL);

		if (ngroups > 0 && m_in.m_lc_pm_groups.ptr == 0)
			return(EFAULT);
```
**讲解**: 参数验证：
- 组数量必须在有效范围内
- 如果组数量 > 0，指针不能为空

```c
		r = sys_datacopy(who_e, m_in.m_lc_pm_groups.ptr, SELF,
			     (vir_bytes) rmp->mp_sgroups,
			     ngroups * sizeof(gid_t));
		if (r != OK)
			return(r);
```
**讲解**: 从用户空间复制组列表。

```c
		for (i = 0; i < ngroups; i++) {
			if (rmp->mp_sgroups[i] > GID_MAX)
				return(EINVAL);
		}
```
**讲解**: 验证每个 GID 值的有效性。

```c
		for (i = ngroups; i < NGROUPS_MAX; i++) {
			rmp->mp_sgroups[i] = 0;
		}
		rmp->mp_ngroups = ngroups;
```
**讲解**: 清零剩余的组槽位，保存组数量。

```c
		m.m_type = VFS_PM_SETGROUPS;
		m.VFS_PM_ENDPT = rmp->mp_endpoint;
		m.VFS_PM_GROUP_NO = rmp->mp_ngroups;
		m.VFS_PM_GROUP_ADDR = (char *) rmp->mp_sgroups;

		break;
```
**讲解**: 构造 VFS 消息，包含组数量和组数组地址。

```c
	case PM_SETSID:
		if (rmp->mp_procgrp == rmp->mp_pid) return(EPERM);
		rmp->mp_procgrp = rmp->mp_pid;
```
**讲解**: 处理 `setsid()` 系统调用。

**权限检查**: 进程不能已经是进程组组长。

**操作**: 创建新会话，进程成为会话组长。

**会话创建规则**:
1. 调用进程不能是进程组组长
2. 创建新会话，进程成为会话组长
3. 进程也成为新进程组的组长
4. 进程失去控制终端

```c
		m.m_type = VFS_PM_SETSID;
		m.VFS_PM_ENDPT = rmp->mp_endpoint;

		break;

	default:
		return(EINVAL);
  }
```
**讲解**: 构造 VFS 消息，通知 VFS 更新状态。

```c
  /* Send the request to VFS */
  tell_vfs(rmp, &m);

  /* Do not reply until VFS has processed the request */
  return(SUSPEND);
}
```

**注释翻译**: "发送请求给 VFS。在 VFS 处理完请求之前不要回复。"

**讲解**: 
- `tell_vfs()`: 异步发送消息给 VFS
- `SUSPEND`: 告诉 PM 主循环不要立即回复用户进程
- VFS 处理完成后会发送回复消息，PM 再回复用户

**异步处理流程**:
```
用户进程          PM              VFS
   |              |               |
   | setuid()     |               |
   |------------->|               |
   |              | VFS_PM_SETUID |
   |              |-------------->|
   |              |               | 更新文件权限
   |              |<--------------|
   |              | 回复          |
   |<-------------|               |
   | 返回结果     |               |
```

---

## 要点总结

### 核心知识点

1. **三种用户/组 ID**:
   - Real ID: 标识用户身份
   - Effective ID: 用于权限检查
   - Saved ID: 保存之前的权限

2. **PM 与 VFS 协作**: 设置 ID 需要同步两边状态，使用异步消息传递。

3. **权限检查规则**: 
   - 普通用户只能切换到 real/saved/effective ID 之一
   - 超级用户可以切换到任意 ID

### 关键常量

| 常量 | 值 | 含义 |
|------|-----|------|
| `NGROUPS_MAX` | 16 | 最大补充组数量 |
| `SUPER_USER` | 0 | 超级用户 UID |
| `TAINTED` | 0x40000 | 进程污染标志 |

---

## 灾难预演

### 场景 1: setuid 权限检查缺失

**如果删除权限检查**:
```c
// if (rmp->mp_realuid != uid && rmp->mp_effuid != SUPER_USER)
//     return(EPERM);
```

**后果**: 普通用户可以切换到任意 UID，包括 root（UID 0），完全破坏系统安全。

### 场景 2: setgroups 不验证 GID 值

**如果删除 GID 验证**:
```c
// for (i = 0; i < ngroups; i++) {
//     if (rmp->mp_sgroups[i] > GID_MAX)
//         return(EINVAL);
// }
```

**后果**: 可能设置无效的 GID 值，导致后续权限检查出现未定义行为。

### 场景 3: setsid 不检查进程组组长

**如果删除检查**:
```c
// if (rmp->mp_procgrp == rmp->mp_pid) return(EPERM);
```

**后果**: 进程组组长调用 `setsid()` 会创建混乱的会话/进程组关系，违反 POSIX 语义。

---

## 互动自测

### 问题 1: real UID 和 effective UID 有什么区别？

**答案**:
- **Real UID**: 标识"谁启动了这个进程"，通常不变
- **Effective UID**: 用于权限检查，可以通过 `seteuid()` 切换
- 例如：`passwd` 程序 real UID 是普通用户，effective UID 是 root

### 问题 2: 为什么设置 ID 需要通知 VFS？

**答案**: VFS 负责文件访问权限检查。当进程的 UID/GID 改变时，VFS 需要更新其内部的权限缓存，否则文件访问检查会使用旧的 ID。

### 问题 3: setsid() 有什么作用？

**答案**:
1. 创建新会话，调用进程成为会话组长
2. 创建新进程组，调用进程成为进程组组长
3. 进程失去控制终端
4. 常用于创建守护进程

---

## Rust 实现对比

### 类型安全的 ID 类型

```rust
#![no_std]

use core::result::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Uid(u32);

impl Uid {
    pub const ROOT: Self = Uid(0);
    pub const MAX: u32 = 2147483647; // UID_MAX
    
    pub fn new(value: u32) -> Result<Self, InvalidId> {
        if value <= Self::MAX {
            Ok(Uid(value))
        } else {
            Err(InvalidId)
        }
    }
    
    pub fn is_root(&self) -> bool {
        self.0 == 0
    }
    
    pub fn value(&self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Gid(u32);

impl Gid {
    pub const ROOT: Self = Gid(0);
    pub const MAX: u32 = 2147483647;
    
    pub fn new(value: u32) -> Result<Self, InvalidId> {
        if value <= Self::MAX {
            Ok(Gid(value))
        } else {
            Err(InvalidId)
        }
    }
}

#[derive(Debug)]
pub struct InvalidId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pid(u32);

impl Pid {
    pub fn new(value: u32) -> Option<Self> {
        if value > 0 && value <= 30000 {
            Some(Pid(value))
        } else {
            None
        }
    }
}
```

### 进程身份结构

```rust
pub struct ProcessIdentity {
    real_uid: Uid,
    effective_uid: Uid,
    saved_uid: Uid,
    
    real_gid: Gid,
    effective_gid: Gid,
    saved_gid: Gid,
    
    supplementary_groups: [Option<Gid>; NGROUPS_MAX],
    num_groups: usize,
    
    process_group: Pid,
    pid: Pid,
}

const NGROUPS_MAX: usize = 16;

impl ProcessIdentity {
    pub fn new(pid: Pid, uid: Uid, gid: Gid) -> Self {
        Self {
            real_uid: uid,
            effective_uid: uid,
            saved_uid: uid,
            real_gid: gid,
            effective_gid: gid,
            saved_gid: gid,
            supplementary_groups: [None; NGROUPS_MAX],
            num_groups: 0,
            process_group: pid,
            pid,
        }
    }
    
    pub fn getuid(&self) -> Uid {
        self.real_uid
    }
    
    pub fn geteuid(&self) -> Uid {
        self.effective_uid
    }
    
    pub fn setuid(&mut self, uid: Uid) -> Result<(), PermissionError> {
        if self.real_uid != uid && !self.effective_uid.is_root() {
            return Err(PermissionError::NotPermitted);
        }
        
        self.real_uid = uid;
        self.effective_uid = uid;
        self.saved_uid = uid;
        
        Ok(())
    }
    
    pub fn seteuid(&mut self, uid: Uid) -> Result<(), PermissionError> {
        if self.real_uid != uid && self.saved_uid != uid && !self.effective_uid.is_root() {
            return Err(PermissionError::NotPermitted);
        }
        
        self.effective_uid = uid;
        Ok(())
    }
    
    pub fn getgroups(&self) -> &[Option<Gid>] {
        &self.supplementary_groups[..self.num_groups]
    }
    
    pub fn setgroups(&mut self, groups: &[Gid]) -> Result<(), PermissionError> {
        if !self.effective_uid.is_root() {
            return Err(PermissionError::NotPermitted);
        }
        
        if groups.len() > NGROUPS_MAX {
            return Err(PermissionError::TooManyGroups);
        }
        
        for (i, gid) in groups.iter().enumerate() {
            self.supplementary_groups[i] = Some(*gid);
        }
        for i in groups.len()..NGROUPS_MAX {
            self.supplementary_groups[i] = None;
        }
        self.num_groups = groups.len();
        
        Ok(())
    }
    
    pub fn setsid(&mut self) -> Result<Pid, PermissionError> {
        if self.process_group == self.pid {
            return Err(PermissionError::AlreadyProcessGroupLeader);
        }
        
        self.process_group = self.pid;
        Ok(self.pid)
    }
}

#[derive(Debug)]
pub enum PermissionError {
    NotPermitted,
    TooManyGroups,
    AlreadyProcessGroupLeader,
}
```

### Rust 实现的优势

1. **类型安全**: `Uid`、`Gid`、`Pid` 是不同类型，编译时防止混淆。

2. **封装**: `ProcessIdentity` 封装了所有身份相关字段，提供清晰的接口。

3. **错误处理**: 使用 `Result<T, E>` 显式处理错误，不会遗漏。

4. **不变性**: 可以使用 `&self` 和 `&mut self` 区分只读和修改操作。

5. **数组安全**: 使用 `Option<Gid>` 和切片避免空指针和越界访问。

### Rust 实现的权衡

1. **运行时检查**: 类型转换需要运行时验证。

2. **内存开销**: `Option<Gid>` 比原始 `gid_t` 数组占用更多空间。

3. **与 C 交互**: 需要使用 `unsafe` 块与现有 C 代码交互。
