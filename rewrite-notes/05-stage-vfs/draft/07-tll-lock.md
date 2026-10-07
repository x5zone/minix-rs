# 07-tll-lock: 三级锁机制 (Three-Level Lock)

> 本文档分析 `minix3/minix/servers/vfs/tll.h` 和 `tll.c` 中的三级锁实现。

---

## 1. 概述

### 1.1 TLL 的角色

TLL 是 VFS 特有的锁机制，支持三种访问级别——READ (共享读), READSER (串行读), WRITE (独占写)
TLL 用于保护 vnode 和 vmnt 的并发访问——vnode.v_lock 和 vmnt.m_lock 都是 tll_t 类型
VFS 多线程环境下 TLL 的必要性——worker thread 模型下多个线程可能同时操作同一 vnode/vmnt，需要锁保护

### 1.2 设计原则

三级访问：READ (共享读) → READSER (串行读) → WRITE (独占写)——级别递增，互斥性增强
升级支持：READ 可以升级为 READSER 或 WRITE——tll_upgrade() 实现锁升级
等待队列：读请求和写请求分别排队——tll_t 中的 wait_for_lock 字段管理等待队列

---

## 2. TLL 类型定义

### 2.1 tll_access_t — 访问级别

四种访问级别：TLL_NONE (无锁), TLL_READ (共享读), TLL_READSER (串行读), TLL_WRITE (独占写)
  - TLL_NONE — 无锁
  - TLL_READ — 共享只读
  - TLL_READSER — 串行只读
  - TLL_WRITE — 独占读写

### 2.2 tll_status_t — 锁状态

三种锁状态：未锁定, 读锁定 (可多个读者), 写锁定 (独占)
  - TLL_DFLT — 默认
  - TLL_UPGR — 正在升级
  - TLL_PEND — 有挂起的升级请求

### 2.3 tll_t — 锁结构体

tll_t 结构体各字段：tll_access (当前访问级别), tll_owner (锁持有者), tll_readers (读者计数), wait_for_lock (等待队列)
  - t_current — 当前访问类型
  - t_owner — 非只读锁的持有者 (worker_thread)
  - t_readonly — 当前只读访问的数量
  - t_status — 锁状态
  - t_write — 写/只读访问请求者队列
  - t_serial — 串行读访问请求者队列

---

## 3. TLL 操作函数

### 3.1 tll_lock()

获取锁的函数——tll_lock() 根据请求的访问级别决定是否可以立即获取或需要等待
不同访问级别的获取逻辑：READ 可与 READ 共存，READSER 互斥，WRITE 完全独占

### 3.2 tll_unlock()

释放锁的函数——tll_unlock() 释放锁并唤醒等待队列中的下一个请求
释放后唤醒等待者——tll_unlock() 检查等待队列，按优先级唤醒写请求或读请求

### 3.3 tll_upgrade()

锁升级函数 (READ → READSER/WRITE)——tll_upgrade() 将 READ 锁升级为更高级别
升级冲突时的等待机制——若升级目标与其他锁冲突，当前线程等待直到可以升级

### 3.4 tll_havelock()

检查当前线程是否持有锁——tll_held_by_self() 判断当前线程是否持有指定 TLL 锁

---

## 4. TLL 与 vnode/vmnt 的映射

### 4.1 VNode 锁映射

VNODE_NONE → TLL_NONE
VNODE_READ → TLL_READ
VNODE_OPCL → TLL_READSER
VNODE_WRITE → TLL_WRITE

### 4.2 VMnt 锁映射

VMNT_READ → TLL_READ
VMNT_WRITE → TLL_READSER
VMNT_EXCL → TLL_WRITE

---

## 5. TLL 与 fork

### 5.1 fork 时不涉及 TLL

pm_fork() 不操作任何 TLL 锁——fork 只增加引用计数，不改变锁状态
fork 只增加引用计数，不改变锁状态——dup_vnode() 只递增 v_ref_count，不获取 TLL 锁

### 5.2 fork 后的锁继承

子进程的 fp_rd/fp_wd 指向的 vnode 的 TLL 锁状态不变——fork 不改变 vnode 的锁状态
多个 fproc 引用同一 vnode 时，TLL 锁正确工作——TLL 锁保护的是操作（读/写），不是引用计数

---

## 6. C 源码

**文件**: `minix3/minix/servers/vfs/tll.h`

```c
typedef enum { TLL_NONE, TLL_READ, TLL_READSER, TLL_WRITE } tll_access_t;
typedef enum { TLL_DFLT = 0x0, TLL_UPGR = 0x1, TLL_PEND = 0x2 } tll_status_t;

typedef struct {
  tll_access_t t_current;
  struct worker_thread *t_owner;
  signed int t_readonly;
  tll_status_t t_status;
  struct worker_thread *t_write;
  struct worker_thread *t_serial;
} tll_t;
```
