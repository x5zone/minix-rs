# servers/vfs/tll.c 逐行讲解

## 文件概述

**文件路径**: `servers/vfs/tll.c`  
**模块归属**: VFS（虚拟文件系统服务器）  
**核心功能**: 实现三级锁（Three-Level Lock），支持读锁、写锁、串行锁

---

## 逐行讲解

### 1. 文件头注释

```c
/* This file contains the implementation of the three-level-lock. */
```

**第1行**: 文件头注释  
- **three-level-lock**: 三级锁

**设计原因**: 支持多种访问模式

---

### 2. 包含头文件

```c
#include "fs.h"
#include "glo.h"
#include "tll.h"
#include "threads.h"
#include <assert.h>
```

**第3-8行**: 包含头文件  
- `fs.h`: VFS 主头文件
- `glo.h`: 全局变量
- `tll.h`: TLL 定义
- `threads.h`: 线程相关
- `assert.h`: 断言宏

---

### 3. tll_append 函数

```c
static int tll_append(tll_t *tllp, tll_access_t locktype)
{
  struct worker_thread *queue;

  assert(self != NULL);
  assert(tllp != NULL);
  assert(locktype != TLL_NONE);

  /* Read-only and write-only requests go to the write queue. Read-serialized
   * requests go to the serial queue. Then we wait for an event to signal it's
   * our turn to go. */
  queue = NULL;
  if (locktype == TLL_READ || locktype == TLL_WRITE) {
	if (tllp->t_write == NULL)
		tllp->t_write = self;
	else
		queue = tllp->t_write;
  } else {
	if (tllp->t_serial == NULL)
		tllp->t_serial = self;
	else
		queue = tllp->t_serial;
  }

  if (queue != NULL) {	/* Traverse to end of queue */
	while (queue->w_next != NULL) queue = queue->w_next;
	queue->w_next = self;
  }
  self->w_next = NULL; /* End of queue */

  /* Now wait for the event it's our turn */
  worker_wait();

  tllp->t_current = locktype;
  tllp->t_status &= ~TLL_PEND;
  tllp->t_owner = self;

  if (tllp->t_current == TLL_READ) {
	tllp->t_readonly++;
	tllp->t_owner = NULL;
  } else if (tllp->t_current == TLL_WRITE)
	assert(tllp->t_readonly == 0);

  /* Due to the way upgrading and downgrading works, read-only requests are
   * scheduled to run after a downgraded lock is released (because they are
   * queued on the write-only queue which has priority). This results from the
   * fact that the downgrade operation cannot know whether the next locktype on
   * the write-only queue is really write-only or actually read-only. However,
   * that means that read-serialized requests stay queued, while they could run
   * simultaneously with read-only requests. See if there are any and grant
   * the head request access */
  if (tllp->t_current == TLL_READ && tllp->t_serial != NULL) {
	tllp->t_owner = tllp->t_serial;
	tllp->t_serial = tllp->t_serial->w_next;
	tllp->t_owner->w_next = NULL;
	assert(!(tllp->t_status & TLL_PEND));
	tllp->t_status |= TLL_PEND;
	worker_signal(tllp->t_owner);
  }

  return(OK);
}
```

**第10-73行**: 将请求加入等待队列  
- **断言**: 检查参数有效性
- **队列选择**: 
  - 读锁和写锁：加入 `t_write` 队列
  - 串行锁：加入 `t_serial` 队列
- **等待**: 调用 `worker_wait` 等待轮到自己
- **设置状态**: 设置当前锁类型和所有者
- **读锁特殊处理**: 读锁增加 `t_readonly` 计数，清除所有者
- **串行锁唤醒**: 如果当前是读锁且有串行锁等待，唤醒串行锁

**设计原因**: 
- **公平性**: 使用队列保证公平性
- **写偏置**: 写队列优先于串行队列
- **并发**: 读锁和串行锁可以并发

---

### 4. tll_downgrade 函数

```c
void tll_downgrade(tll_t *tllp)
{
/* Downgrade three-level-lock tll from write-only to read-serialized, or from
 * read-serialized to read-only. Caveat: as we can't know whether the next
 * lock type on the write queue is actually read-only or write-only, we can't
 * grant access to that type. It will be granted access once we unlock. Also,
 * because we apply write-bias, we can't grant access to read-serialized
 * either, unless nothing is queued on the write-only stack. */

  assert(self != NULL);
  assert(tllp != NULL);
  assert(tllp->t_owner == self);

  switch(tllp->t_current) {
    case TLL_WRITE: tllp->t_current = TLL_READSER; break;
    case TLL_READSER:
	/* If nothing is queued on write-only, but there is a pending lock
	 * requesting read-serialized, grant it and keep the lock type. */

	if (tllp->t_write == NULL && tllp->t_serial != NULL) {
		tllp->t_owner = tllp->t_serial;
		tllp->t_serial = tllp->t_serial->w_next; /* Remove head */
		tllp->t_owner->w_next = NULL;
		assert(!(tllp->t_status & TLL_PEND));
		tllp->t_status |= TLL_PEND;
		worker_signal(tllp->t_owner);
	} else {
		tllp->t_current = TLL_READ;
		tllp->t_owner = NULL;
	}
	tllp->t_readonly++; /* Either way, there's one more read-only lock */
	break;
    default: panic("VFS: Incorrect lock state");
  }

  if (tllp->t_current != TLL_WRITE && tllp->t_current != TLL_READSER)
	assert(tllp->t_owner == NULL);
}
```

**第75-113行**: 降级锁  
- **断言**: 检查参数和所有权
- **写锁降级**: 写锁 → 串行锁
- **串行锁降级**: 
  - 如果写队列为空且有串行锁等待：唤醒串行锁
  - 否则：串行锁 → 读锁
  - 增加 `t_readonly` 计数

**设计原因**: 
- **降级**: 允许从高级别锁降级到低级别锁
- **唤醒**: 降级时唤醒等待的锁

---

### 5. tll_init 函数

```c
void tll_init(tll_t *tllp)
{
/* Initialize three-level-lock tll */
  assert(tllp != NULL);

  tllp->t_current = TLL_NONE;
  tllp->t_readonly = 0;
  tllp->t_status = TLL_DFLT;
  tllp->t_write = NULL;
  tllp->t_serial = NULL;
  tllp->t_owner = NULL;
}
```

**第115-126行**: 初始化 TLL  
- **断言**: 检查参数有效性
- **初始化**: 设置所有字段为默认值

**设计原因**: 初始化锁结构

---

### 6. tll_islocked 函数

```c
int tll_islocked(tll_t *tllp)
{
  assert(tllp >= (tll_t *) PAGE_SIZE);
  return(tllp->t_current != TLL_NONE);
}
```

**第128-132行**: 检查锁是否被持有  
- **断言**: 检查指针有效性（大于页大小）
- **返回**: 如果当前锁类型不是 `TLL_NONE`，返回真

**设计原因**: 检查锁状态

---

### 7. tll_locked_by_me 函数

```c
int tll_locked_by_me(tll_t *tllp)
{
  assert(tllp >= (tll_t *) PAGE_SIZE);
  assert(self != NULL);
  return(tllp->t_owner == self && !(tllp->t_status & TLL_PEND));
}
```

**第134-139行**: 检查当前线程是否持有锁  
- **断言**: 检查指针和线程有效性
- **返回**: 如果所有者是当前线程且没有等待锁，返回真

**设计原因**: 检查当前线程是否持有锁

---

### 8. tll_lock 函数

```c
int tll_lock(tll_t *tllp, tll_access_t locktype)
{
/* Try to lock three-level-lock tll with type locktype */

  assert(self != NULL);
  assert(tllp >= (tll_t *) PAGE_SIZE);
  assert(locktype != TLL_NONE);

  self->w_next = NULL;

  if (locktype != TLL_READ && locktype != TLL_READSER && locktype != TLL_WRITE)
	panic("Invalid lock type %d\n", locktype);
```

**第141-154行**: 尝试获取锁  
- **断言**: 检查参数有效性
- **检查**: 检查锁类型是否有效

**设计原因**: 获取锁的入口函数

---

## 要点总结

### 1. 核心知识点

1. **三级锁**: 读锁、写锁、串行锁
2. **队列管理**: 使用队列管理等待的线程
3. **降级**: 支持锁降级

### 2. 设计亮点

- **写偏置**: 写队列优先于串行队列
- **并发**: 读锁和串行锁可以并发
- **公平性**: 队列保证公平性

### 3. 内存模型

```
TLL 结构:
┌─────────────────────────────────┐
│ t_current = TLL_READ            │
│ t_readonly = 2                  │
│ t_status = TLL_DFLT             │
│ t_write -> 等待队列             │
│ t_serial -> 等待队列            │
│ t_owner = NULL                  │
└─────────────────────────────────┘
```

---

## 灾难预演

### 场景 1: 死锁

**后果**: 
- 所有线程等待
- 系统挂起

**症状**: VFS 无响应

### 场景 2: 锁泄漏

**后果**: 
- 锁永不释放
- 其他线程无法获取锁

**症状**: 进程挂起

### 场景 3: 锁降级错误

**后果**: 
- 状态不一致
- 数据损坏

**症状**: 文件系统错误

---

## 互动自测

### 问题 1: 三级锁

**问**: 为什么需要三级锁？

**答**: 
- **读锁**: 允许多个读者并发
- **写锁**: 独占访问
- **串行锁**: 特殊操作（如属性修改）

### 问题 2: 写偏置

**问**: 为什么写队列优先于串行队列？

**答**: 
- **避免饥饿**: 写操作可能被读操作饥饿
- **性能**: 写操作通常更重要

### 问题 3: 锁降级

**问**: 为什么支持锁降级？

**答**: 
- **优化**: 从写锁降级到读锁，允许其他读者
- **灵活性**: 支持不同的访问模式

---

## Rust 实现对比

### C 版本（原始）

```c
void tll_init(tll_t *tllp)
{
  assert(tllp != NULL);

  tllp->t_current = TLL_NONE;
  tllp->t_readonly = 0;
  tllp->t_status = TLL_DFLT;
  tllp->t_write = NULL;
  tllp->t_serial = NULL;
  tllp->t_owner = NULL;
}
```

### Rust 版本（安全抽象）

```rust
impl Tll {
    fn new() -> Self {
        Tll {
            t_current: TllAccess::None,
            t_readonly: 0,
            t_status: TLL_DFLT,
            t_write: None,
            t_serial: None,
            t_owner: None,
        }
    }
}
```

### 关键改进

1. **Option**: 使用 `Option` 表示可能为空
2. **枚举**: 使用枚举表示锁类型
3. **构造函数**: 使用 `new` 构造函数

---

## 理论关联

### 1. 读写锁

**操作系统概念**: 读写锁允许多个读者或一个写者

**Minix3 实现**:
- TLL 扩展了读写锁
- 增加串行锁支持特殊操作

### 2. 锁降级

**操作系统概念**: 锁降级从高级别锁到低级别锁

**Minix3 实现**:
- 写锁 → 串行锁 → 读锁
- 降级时唤醒等待的锁

### 3. 公平性

**操作系统概念**: 公平性保证线程不被饥饿

**Minix3 实现**:
- 使用队列管理等待线程
- 写偏置避免写饥饿

---

## 总结

`tll.c` 实现了 Minix3 VFS 的三级锁。通过读锁、写锁、串行锁、队列管理、锁降级等设计，实现了高效、公平的并发控制。理解 TLL 的状态转换和队列管理是理解 VFS 并发模型的关键。
