# 08-worker-thread: 工作线程框架

> 本文档分析 `minix3/minix/servers/vfs/threads.h` 和 `worker.c` 中的工作线程机制。

---

## 1. 概述

### 1.1 VFS 多线程模型

VFS 是 Minix3 中唯一使用多线程 (mthread) 的服务器
多线程的原因——阻塞 I/O 操作不能阻塞整个 VFS，如管道读、设备 I/O、文件系统请求等
NR_WTHREADS = 9 个工作线程——VFS 启动时创建 9 个工作线程

### 1.2 设计原则

主线程负责接收消息和分发——get_work() 接收消息，send_work() 分发给工作线程
工作线程处理具体的系统调用——通过 call_vec[] 调度到具体处理函数
每个工作线程与一个 fproc 关联 (w_fp)——w_fp 指向当前正在处理的进程

---

## 2. Worker Thread 结构体

### 2.1 worker_thread 字段

各字段含义：w_fp (关联的 fproc), w_task (等待的 FS 端点), w_sendrec (发送/接收的消息), w_thread (mthread 线程 ID), w_self (工作线程数组索引)
  - w_tid — mthread 线程 ID
  - w_event_mutex / w_event — 线程等待/唤醒条件变量
  - w_fp — 关联的 fproc 指针
  - w_m_in / w_m_out — 输入/输出消息
  - w_err_code — 错误码
  - w_sendrec — IPC send/receive 消息
  - w_drv_sendrec — 驱动通信消息
  - w_task — 通信目标端点
  - w_dmap — 关联的设备映射
  - w_next — 链表指针

---

## 3. Worker Thread 生命周期

### 3.1 线程初始化

VFS 启动时工作线程的创建——sef_cb_init_fresh() 中调用 worker_init() 创建 NR_WTHREADS 个工作线程
线程进入等待状态的初始行为——创建后立即进入等待状态，等待主线程分发工作

### 3.2 工作分发

`worker_start()` 函数——将工作分配给空闲线程，设置 w_fp 并唤醒线程
`send_work()` 函数——主循环调用，分发待处理请求给空闲工作线程

### 3.3 工作执行

`do_work()` 函数——工作线程的主循环，通过 call_vec[callnr] 调度到具体处理函数
通过 call_vec[] 调度到具体处理函数——call_vec 是函数指针数组，索引为消息类型

### 3.4 线程暂停与恢复

`worker_suspend()` / `worker_resume()` 函数——阻塞 I/O 时暂停当前线程，I/O 完成后恢复
阻塞 I/O 操作时的线程暂停机制——worker_suspend() 让出执行权，worker_resume() 唤醒线程

### 3.5 线程终止

`worker_stop_by_endpt()` 函数——取消指定进程关联的工作线程
进程退出时取消其关联工作线程——pm_exit() 调用 worker_stop_by_endpt() 确保退出进程的线程被清理

---

## 4. Worker Thread 与 fork

### 4.1 fork 请求的处理

VFS_PM_FORK 消息由主线程分发给空闲工作线程处理
pm_fork() 在工作线程上下文中执行——不是主线程，而是被分配的工作线程

### 4.2 子进程的 worker 关联

子进程初始时没有活跃的工作线程 (w_fp = NULL)——子进程还未发起任何系统调用
子进程第一次发起系统调用时会被分配工作线程——get_work() 中主线程分配空闲线程

### 4.3 fp_worker 字段

fproc 中的 `struct worker_thread *fp_worker` 字段——指向当前处理该进程请求的工作线程
fork 后子进程的 fp_worker 被复制但应该为 NULL——整体复制后 fp_worker 指向父进程的工作线程，但子进程不应关联父进程的线程
pm_fork 设置 `fp_flags = FP_NOFLAGS` 不直接重置工作线程关联——fp_worker 的处理在整体复制后由后续操作修正

---

## 5. 线程安全考量

### 5.1 fproc 锁 (fp_lock)

pm_fork 时必须保留子进程 fp_lock 的原因——其他工作线程可能正在操作子进程 slot
其他工作线程可能正在操作子进程 slot——如果覆盖 mutex，会导致死锁或等待队列丢失

### 5.2 全局数据结构保护

filp/vnode/vmnt 的锁机制如何与工作线程交互——TLL 锁保护数据结构，fp_lock 保护 fproc，两者协同确保并发安全
pm_fork 操作不需要额外锁保护的原因——pm_fork 在工作线程上下文中执行，fproc 的 fp_lock 已保护关键操作

---

## 6. C 源码

**文件**: `minix3/minix/servers/vfs/threads.h`

```c
struct worker_thread {
  thread_t w_tid;
  mutex_t w_event_mutex;
  cond_t w_event;
  struct fproc *w_fp;
  message w_m_in;
  message w_m_out;
  int w_err_code;
  message *w_sendrec;
  message *w_drv_sendrec;
  endpoint_t w_task;
  struct dmap *w_dmap;
  struct worker_thread *w_next;
};
```
