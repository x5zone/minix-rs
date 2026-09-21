# servers/vfs/vmnt.c 逐行讲解

## 文件概述

**文件路径**: `servers/vfs/vmnt.c`  
**模块归属**: VFS（虚拟文件系统服务器）  
**核心功能**: 实现挂载点管理函数，包括初始化、查找、锁定、解锁等操作

---

## 逐行讲解

### 1. 文件头注释

```c
/* Virtual mount table related routines.
 *
 */
```

**第1-3行**: 文件头注释  
- **Virtual mount table**: 虚拟挂载表
- **related routines**: 相关例程
- **作用**: 说明此文件的功能

---

### 2. 包含头文件

```c
#include "fs.h"
#include "vmnt.h"
#include <assert.h>
#include <string.h>
```

**第5-8行**: 包含头文件  
- `fs.h`: VFS 主头文件
- `vmnt.h`: 挂载点结构体定义
- `assert.h`: 断言宏
- `string.h`: 字符串操作函数

---

### 3. 静态函数声明

```c
static int is_vmnt_locked(struct vmnt *vmp);
static void clear_vmnt(struct vmnt *vmp);
```

**第10-11行**: 静态函数声明  
- `is_vmnt_locked`: 检查挂载点是否被锁定
- `clear_vmnt`: 清空挂载点结构体
- **static**: 仅在本文件内可见

---

### 4. 宏定义

```c
/* Is vmp pointer reasonable? */
#define SANEVMP(v) ((((v) >= &vmnt[0] && (v) < &vmnt[NR_MNTS])))
#define BADVMP(v, f, l) printf("%s:%d: bad vmp %p\n", f, l, v)
/* vp check that panics */
#define ASSERTVMP(v) if(!SANEVMP(v)) { \
	BADVMP(v, __FILE__, __LINE__); panic("bad vmp"); }
```

**第13-18行**: 宏定义  
- `SANEVMP(v)`: 检查指针是否在有效范围内
- `BADVMP(v, f, l)`: 打印错误信息
- `ASSERTVMP(v)`: 断言指针有效，否则 panic

**设计原因**: 
- **防御性编程**: 检查指针有效性
- **调试支持**: 打印错误位置

---

### 5. 锁调试代码

```c
#if LOCK_DEBUG
/*===========================================================================*
 *				check_vmnt_locks_by_me			     *
 *===========================================================================*/
void check_vmnt_locks_by_me(struct fproc *rfp)
{
/* Check whether this thread still has locks held on vmnts */
  struct vmnt *vmp;

  for (vmp = &vmnt[0]; vmp < &vmnt[NR_MNTS]; vmp++) {
	if (tll_locked_by_me(&vmp->m_lock))
		panic("Thread %d still holds vmnt lock on vmp %p call_nr=%d\n",
		      mthread_self(), vmp, job_call_nr);
  }

  if (rfp->fp_vmnt_rdlocks != 0)
	panic("Thread %d still holds read locks on a vmnt (%d) call_nr=%d\n",
	      mthread_self(), rfp->fp_vmnt_rdlocks, job_call_nr);
}
#endif
```

**第20-38行**: 锁调试函数  
- **条件编译**: `#if LOCK_DEBUG`，仅在调试模式下编译
- **功能**: 检查当前线程是否仍持有挂载点锁
- **panic**: 如果发现锁泄漏，触发 panic

**设计原因**: 
- **调试工具**: 帮助发现锁泄漏
- **条件编译**: 生产环境不编译，减少开销

---

### 6. check_vmnt_locks 函数

```c
/*===========================================================================*
 *				check_vmnt_locks			     *
 *===========================================================================*/
void
check_vmnt_locks(void)
{
  struct vmnt *vmp;
  int count = 0;

  for (vmp = &vmnt[0]; vmp < &vmnt[NR_MNTS]; vmp++)
	if (is_vmnt_locked(vmp)) {
		count++;
		printf("vmnt %p is %s, fs_e=%d dev=%llx\n", vmp, (tll_islocked(&vmp->m_lock) ? "locked":"pending locked"), vmp->m_fs_e, vmp->m_dev);
	}

  if (count) panic("%d locked vmnts\n", count);
#if 0
  printf("check_vmnt_locks OK\n");
#endif
}
```

**第40-58行**: 检查所有挂载点锁  
- **遍历**: 遍历所有挂载点
- **检查**: 检查是否被锁定
- **打印**: 打印锁定的挂载点信息
- **panic**: 如果有锁定的挂载点，触发 panic

**设计原因**: 调试工具，检查锁状态

---

### 7. mark_vmnt_free 函数

```c
/*===========================================================================*
 *                             mark_vmnt_free				     *
 *===========================================================================*/
void mark_vmnt_free(struct vmnt *vmp)
{
  ASSERTVMP(vmp);

  vmp->m_fs_e = NONE;
  vmp->m_dev = NO_DEV;
}
```

**第60-68行**: 标记挂载点为空闲  
- **ASSERTVMP**: 检查指针有效性
- **m_fs_e = NONE**: 清除文件系统端点
- **m_dev = NO_DEV**: 清除设备号

**设计原因**: 标记挂载点为可用状态

---

### 8. clear_vmnt 函数

```c
/*===========================================================================*
 *                             clear_vmnt				     *
 *===========================================================================*/
static void clear_vmnt(struct vmnt *vmp)
{
/* Reset vmp to initial parameters */
  ASSERTVMP(vmp);

  vmp->m_fs_e = NONE;
  vmp->m_dev = NO_DEV;
  vmp->m_flags = 0;
  vmp->m_mounted_on = NULL;
  vmp->m_root_node = NULL;
  vmp->m_label[0] = '\0';
  vmp->m_comm.c_max_reqs = 1;
  vmp->m_comm.c_cur_reqs = 0;
  vmp->m_comm.c_req_queue = NULL;
}
```

**第70-86行**: 清空挂载点结构体  
- **重置所有字段**: 将所有字段设置为初始值
- **通信结构**: 初始化通信相关字段

**设计原因**: 完全重置挂载点状态

---

### 9. get_free_vmnt 函数

```c
/*===========================================================================*
 *                             get_free_vmnt				     *
 *===========================================================================*/
struct vmnt *get_free_vmnt(void)
{
  struct vmnt *vmp;

  for (vmp = &vmnt[0]; vmp < &vmnt[NR_MNTS]; ++vmp) {
	if (vmp->m_dev == NO_DEV) {
		clear_vmnt(vmp);
		return(vmp);
	}
  }

  return(NULL);
}
```

**第88-100行**: 获取空闲挂载点  
- **遍历**: 遍历所有挂载点
- **查找**: 找到 `m_dev == NO_DEV` 的空闲条目
- **清空**: 调用 `clear_vmnt` 重置
- **返回**: 返回空闲条目或 NULL

**设计原因**: 分配挂载点条目

---

### 10. find_vmnt 函数

```c
/*===========================================================================*
 *                             find_vmnt				     *
 *===========================================================================*/
struct vmnt *find_vmnt(endpoint_t fs_e)
{
/* Find the vmnt belonging to an FS with endpoint 'fs_e' iff it's in use */
  struct vmnt *vp;

  for (vp = &vmnt[0]; vp < &vmnt[NR_MNTS]; ++vp)
	if (vp->m_fs_e == fs_e && vp->m_dev != NO_DEV)
		return(vp);

  return(NULL);
}
```

**第102-113行**: 根据端点查找挂载点  
- **参数**: `fs_e` 文件系统端点
- **遍历**: 遍历所有挂载点
- **匹配**: 找到端点匹配且正在使用的条目
- **返回**: 返回找到的条目或 NULL

**设计原因**: 通过文件系统端点查找挂载点

---

### 11. init_vmnts 函数

```c
/*===========================================================================*
 *                             init_vmnts				     *
 *===========================================================================*/
void init_vmnts(void)
{
/* Initialize vmnt table */
  struct vmnt *vmp;

  for (vmp = &vmnt[0]; vmp < &vmnt[NR_MNTS]; vmp++) {
	clear_vmnt(vmp);
	tll_init(&vmp->m_lock);
  }
}
```

**第115-126行**: 初始化挂载点表  
- **遍历**: 遍历所有挂载点
- **清空**: 调用 `clear_vmnt` 重置
- **初始化锁**: 调用 `tll_init` 初始化三级锁

**设计原因**: 系统启动时初始化挂载点表

---

### 12. is_vmnt_locked 函数

```c
/*===========================================================================*
 *                             is_vmnt_locked				     *
 *===========================================================================*/
static int is_vmnt_locked(struct vmnt *vmp)
{
  ASSERTVMP(vmp);
  return(tll_islocked(&vmp->m_lock) || tll_haspendinglock(&vmp->m_lock));
}
```

**第128-135行**: 检查挂载点是否被锁定  
- **ASSERTVMP**: 检查指针有效性
- **tll_islocked**: 检查是否已锁定
- **tll_haspendinglock**: 检查是否有等待的锁
- **返回**: 锁定返回非零，否则返回零

**设计原因**: 检查锁状态

---

### 13. lock_vmnt 函数

```c
/*===========================================================================*
 *                             lock_vmnt				     *
 *===========================================================================*/
int lock_vmnt(struct vmnt *vmp, tll_access_t locktype)
{
  int r;
  tll_access_t initial_locktype;

  ASSERTVMP(vmp);

  initial_locktype = (locktype == VMNT_EXCL) ? VMNT_WRITE : locktype;

  if (vmp->m_fs_e == who_e) return(EDEADLK);

  r = tll_lock(&vmp->m_lock, initial_locktype);

  if (r == EBUSY) return(r);

  if (initial_locktype != locktype) {
	upgrade_vmnt_lock(vmp);
  }

#if LOCK_DEBUG
  if (locktype == VMNT_READ)
	fp->fp_vmnt_rdlocks++;
#endif

  return(OK);
}
```

**第137-163行**: 锁定挂载点  
- **参数**: `vmp` 挂载点指针，`locktype` 锁类型
- **死锁检测**: 如果文件系统端点等于调用者端点，返回 `EDEADLK`
- **初始锁类型**: 如果是 `VMNT_EXCL`，先获取 `VMNT_WRITE`，然后升级
- **获取锁**: 调用 `tll_lock` 获取锁
- **升级锁**: 如果需要，调用 `upgrade_vmnt_lock` 升级
- **调试计数**: 如果是读锁，增加读锁计数

**设计原因**: 
- **死锁预防**: 检测文件系统服务尝试锁定自己的挂载点
- **锁升级**: 支持从写锁升级到独占锁

---

### 14. vmnt_unmap_by_endpt 函数

```c
/*===========================================================================*
 *                             vmnt_unmap_by_endpoint			     *
 *===========================================================================*/
void vmnt_unmap_by_endpt(endpoint_t proc_e)
{
  struct vmnt *vmp;

  if ((vmp = find_vmnt(proc_e)) != NULL) {
	mark_vmnt_free(vmp);
	fs_cancel(vmp);
	invalidate_filp_by_endpt(proc_e);
	if (vmp->m_mounted_on) {
		/* Only put mount point when it was actually used as mount
		 * point. That is, the mount was succesful. */
		put_vnode(vmp->m_mounted_on);
	}
  }
}
```

**第165-180行**: 根据端点取消映射挂载点  
- **参数**: `proc_e` 进程端点
- **查找**: 调用 `find_vmnt` 查找挂载点
- **标记空闲**: 调用 `mark_vmnt_free` 标记为空闲
- **取消操作**: 调用 `fs_cancel` 取消文件系统操作
- **失效文件**: 调用 `invalidate_filp_by_endpt` 使文件失效
- **释放 vnode**: 如果有挂载点 vnode，调用 `put_vnode` 释放

**设计原因**: 文件系统服务退出时清理挂载点

---

### 15. unlock_vmnt 函数

```c
/*===========================================================================*
 *                             unlock_vmnt				     *
 *===========================================================================*/
void unlock_vmnt(struct vmnt *vmp)
{
  ASSERTVMP(vmp);

#if LOCK_DEBUG
  /* Decrease read-only lock counter when not locked as VMNT_WRITE or
   * VMNT_EXCL */
  if (!tll_locked_by_me(&vmp->m_lock))
	fp->fp_vmnt_rdlocks--;
#endif

  tll_unlock(&vmp->m_lock);

#if LOCK_DEBUG
  assert(!tll_locked_by_me(&vmp->m_lock));
#endif

}
```

**第182-200行**: 解锁挂载点  
- **ASSERTVMP**: 检查指针有效性
- **调试计数**: 如果是读锁，减少读锁计数
- **解锁**: 调用 `tll_unlock` 解锁
- **断言**: 确保解锁成功

**设计原因**: 释放挂载点锁

---

### 16. downgrade_vmnt_lock 函数

```c
/*===========================================================================*
 *                             downgrade_vmnt_lock			     *
 *===========================================================================*/
void downgrade_vmnt_lock(struct vmnt *vmp)
{
  ASSERTVMP(vmp);
  tll_downgrade(&vmp->m_lock);

#if LOCK_DEBUG
  /* If we're no longer the owner of a lock, we downgraded to VMNT_READ */
  if (!tll_locked_by_me(&vmp->m_lock)) {
	fp->fp_vmnt_rdlocks++;
  }
#endif
}
```

**第202-215行**: 降级挂载点锁  
- **ASSERTVMP**: 检查指针有效性
- **降级**: 调用 `tll_downgrade` 降级锁
- **调试计数**: 如果降级到读锁，增加读锁计数

**设计原因**: 从写锁降级到读锁

---

### 17. upgrade_vmnt_lock 函数

```c
/*===========================================================================*
 *                             upgrade_vmnt_lock			     *
 *===========================================================================*/
void upgrade_vmnt_lock(struct vmnt *vmp)
{
  ASSERTVMP(vmp);
  tll_upgrade(&vmp->m_lock);
}
```

**第217-224行**: 升级挂载点锁  
- **ASSERTVMP**: 检查指针有效性
- **升级**: 调用 `tll_upgrade` 升级锁

**设计原因**: 从读锁升级到写锁

---

## 要点总结

### 1. 核心知识点

1. **挂载点管理**: 初始化、查找、分配、释放挂载点
2. **三级锁**: 使用 `tll_lock` 实现读锁、读序列化锁、写锁
3. **死锁预防**: 检测文件系统服务尝试锁定自己的挂载点

### 2. 设计亮点

- **防御性编程**: 使用 `ASSERTVMP` 检查指针有效性
- **调试支持**: `LOCK_DEBUG` 条件编译提供锁调试
- **锁升级/降级**: 支持锁的动态升级和降级

### 3. 内存模型

```
静态数据段:
┌─────────────────────────────────┐
│ vmnt[0]                         │
│  ├─ m_fs_e = NONE               │
│  ├─ m_dev = NO_DEV              │
│  ├─ m_lock (tll_t)              │
│  └─ ...                         │
├─────────────────────────────────┤
│ vmnt[1]                         │
│  └─ ...                         │
└─────────────────────────────────┘
```

---

## 灾难预演

### 场景 1: 删除 `ASSERTVMP` 宏

**后果**: 
- 无效指针导致内存错误
- 难以调试的崩溃

**症状**: 随机段错误

### 场景 2: 死锁检测失效

**后果**: 
- 文件系统服务锁定自己的挂载点
- 死锁，系统挂起

**症状**: 文件操作永久阻塞

### 场景 3: 锁升级失败

**后果**: 
- 读锁未升级到写锁
- 数据竞争

**症状**: 数据不一致

---

## 互动自测

### 问题 1: 死锁检测

**问**: 为什么 `lock_vmnt` 检查 `vmp->m_fs_e == who_e`？

**答**: 
- 如果文件系统服务尝试锁定自己的挂载点，会死锁
- 文件系统服务处理请求时，VFS 已经锁定了挂载点
- 文件系统服务再次请求锁，会永久等待

### 问题 2: 锁升级

**问**: 为什么 `VMNT_EXCL` 需要先获取 `VMNT_WRITE` 再升级？

**答**: 
- 三级锁可能不支持直接获取独占锁
- 先获取写锁，再升级到独占锁
- 避免竞争条件

### 问题 3: 调试支持

**问**: `LOCK_DEBUG` 的作用是什么？

**答**: 
- 检查锁泄漏
- 跟踪读锁计数
- 帮助调试并发问题

---

## Rust 实现对比

### C 版本（原始）

```c
int lock_vmnt(struct vmnt *vmp, tll_access_t locktype)
{
  int r;
  tll_access_t initial_locktype;

  ASSERTVMP(vmp);

  initial_locktype = (locktype == VMNT_EXCL) ? VMNT_WRITE : locktype;

  if (vmp->m_fs_e == who_e) return(EDEADLK);

  r = tll_lock(&vmp->m_lock, initial_locktype);

  if (r == EBUSY) return(r);

  if (initial_locktype != locktype) {
	upgrade_vmnt_lock(vmp);
  }

  return(OK);
}
```

### Rust 版本（安全抽象）

```rust
use std::sync::RwLock;

enum VmntLock {
    Read,
    ReadSer,
    Write,
    Excl,
}

impl Vmnt {
    fn lock(&mut self, locktype: VmntLock) -> Result<(), i32> {
        if self.m_fs_e == who_e {
            return Err(EDEADLK);
        }

        let initial_locktype = match locktype {
            VmntLock::Excl => VmntLock::Write,
            other => other,
        };

        self.m_lock.lock(initial_locktype)?;

        if initial_locktype != locktype {
            self.m_lock.upgrade()?;
        }

        Ok(())
    }
}
```

### 关键改进

1. **类型安全**: 使用 `enum` 而非整数
2. **Result**: 使用 `Result` 而非整数错误码
3. **所有权**: Rust 编译器强制正确使用锁

---

## 理论关联

### 1. 挂载点管理

**操作系统概念**: 挂载点是将文件系统连接到目录树的点

**Minix3 实现**:
- `vmnt` 结构体描述挂载点
- `init_vmnts` 初始化挂载点表
- `get_free_vmnt` 分配挂载点

### 2. 三级锁

**操作系统概念**: 读写锁允许多个读者或一个写者

**Minix3 实现**:
- 三级锁扩展了读写锁
- 支持读锁、读序列化锁、写锁

### 3. 死锁预防

**操作系统概念**: 死锁是多个进程互相等待资源

**Minix3 实现**:
- 检测文件系统服务尝试锁定自己的挂载点
- 返回 `EDEADLK` 错误

---

## 总结

`vmnt.c` 实现了 Minix3 VFS 的挂载点管理函数。通过初始化、查找、锁定、解锁等操作，实现了高效、安全的挂载点管理。理解这些函数是理解 VFS 挂载机制的关键。
