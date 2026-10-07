# 12-pm-fork-copy: pm_fork — fproc 结构复制

> 本文档分析 `minix3/minix/servers/vfs/misc.c` 中 `pm_fork()` 函数的 fproc 结构复制部分。

---

## 1. 概述

### 1.1 pm_fork 的角色

- `pm_fork()` 是 VFS 侧 fork 操作的核心函数，负责在 PM 完成 mproc 表更新后，同步更新 VFS 的 fproc 表。它执行三个关键操作：① 整体复制父进程的 fproc 到子进程槽位（保留子进程的 fp_lock）；② 遍历子进程的 fp_filp[] 递增每个 filp 的引用计数；③ 设置子进程的 PID、endpoint 和标志位，并递增根目录和工作目录 vnode 的引用计数。函数不返回错误——内部使用 `okendpt()` 和 `assert()` 确保正确性，失败则 panic。
### 1.2 函数签名

  - pproc — 父进程 endpoint
  - cproc — 子进程 endpoint
  - cpid — 子进程 PID

---

## 2. 参数验证

### 2.1 okendpt() — 验证父进程 endpoint

- `okendpt(pproc, &parentno)` 验证父进程 endpoint 的有效性并获取其槽位号。验证逻辑：从 endpoint 提取槽位号 `p = _ENDPOINT_P(pproc)`，检查 `p >= 0 && p < NR_PROCS`，然后检查 `fproc[p].fp_endpoint == pproc`（确保 generation 号匹配）。如果验证失败，`okendpt()` 返回 `EINVAL`，但 `pm_fork()` 不检查返回值——这意味着如果父进程 endpoint 无效，`parentno` 将不被设置，后续的 `fproc[parentno]` 访问将使用未初始化的值，导致未定义行为。这是 Minix3 代码中的一个隐含假设：PM 发送的父进程 endpoint 一定有效。
### 2.2 _ENDPOINT_P() — 提取子进程 slot

- `childno = _ENDPOINT_P(cproc)` 从子进程 endpoint 中提取槽位号。注意：这里不使用 `okendpt()` 验证子进程 endpoint，因为此时子进程的 fproc 中 `fp_endpoint` 尚未设置（仍为 `NONE`），`okendpt()` 的 generation 号检查会失败。PM 直接给出子进程的 endpoint，其中编码了槽位号，VFS 通过 `_ENDPOINT_P()` 宏提取。提取后检查范围 `childno >= 0 && childno < NR_PROCS`，越界则 panic。
### 2.3 断言检查

  - `panic("VFS: bogus child for forking: %d", cproc)` — 无效子进程
  - `panic("VFS: forking on top of in-use child: %d", childno)` — slot 被占用

---

## 3. fproc 整体复制

### 3.1 保存子进程 fp_lock

- `c_fp_lock = fproc[childno].fp_lock`——在整体复制之前，先保存子进程槽位自己的 mutex。fp_lock 属于槽位（slot），不属于进程——每个 fproc 槽位有自己独立的 mutex，用于 VFS 多线程环境下保护该槽位的并发访问。如果整体复制覆盖了子进程的 fp_lock，子进程将使用父进程的 mutex，导致两个不同进程共享同一个锁，破坏互斥语义。
### 3.2 结构体赋值

- `fproc[childno] = fproc[parentno]`——C 语言的 struct 赋值是浅拷贝（shallow copy），逐字节复制所有字段。这意味着子进程的 fproc 与父进程完全相同——包括所有指针值（fp_filp[]、fp_rd、fp_wd）。此时父子进程的 fp_filp[i] 指向同一个 filp 结构体，fp_rd 和 fp_wd 指向同一个 vnode——这正是 POSIX fork 的语义：父子进程共享打开的文件和目录。但引用计数尚未递增，这是下一步的工作。
### 3.3 恢复子进程 fp_lock

- `fproc[childno].fp_lock = c_fp_lock`——将之前保存的子进程 mutex 恢复。这三步操作（保存→复制→恢复）构成了"保留锁"的原子操作模式。在 Rust 版本中，这种模式可以通过将 `fp_lock` 从 `FProc` 结构体中分离出来——放在 `FProcTable` 层面管理，使得 `FProc` 可以安全地 `Clone`/`Copy`，而不影响 mutex 的所有权。
---

## 4. 复制后的状态分析

### 4.1 被正确继承的字段

  - fp_filp[] — 文件描述符指针（接下来需要递增引用计数）
  - fp_cloexec_set — FD_CLOEXEC 位图
  - fp_rd / fp_wd — 目录 vnode 指针（接下来需要递增引用计数）
  - fp_realuid / fp_effuid — UID
  - fp_realgid / fp_effgid — GID
  - fp_sgroups[] — 补充组
  - fp_umask — umask
  - fp_tty — 控制终端
  - fp_name — 进程名

### 4.2 需要修正的字段

  - fp_pid — 需要设为 cpid
  - fp_endpoint — 需要设为 cproc
  - fp_flags — 需要设为 FP_NOFLAGS
  - fp_lock — 已恢复

---

## 5. 与 Kernel do_fork 的对比

| 维度 | Kernel do_fork | VFS pm_fork |
|------|---------------|-------------|
| 复制对象 | `struct proc` | `struct fproc` |
| 复制方式 | `*rpc = *rpp` | `fproc[child] = fproc[parent]` |
| 锁保留 | 无（单核环境） | 保留 fp_lock（多线程环境） |
| 引用计数 | 无（独占结构） | filp_count/v_ref_count 需递增 |
| 验证方式 | isokendpt + isemptyp | okendpt + assert PID_FREE |
| 修正字段 | endpoint/p_nr/rts_flags | pid/endpoint/flags |

---

## 6. C 源码

**文件**: `minix3/minix/servers/vfs/misc.c` (pm_fork 结构复制部分)

```c
void pm_fork(endpoint_t pproc, endpoint_t cproc, pid_t cpid)
{
  struct fproc *cp;
  int i, parentno, childno;
  mutex_t c_fp_lock;

  okendpt(pproc, &parentno);
  childno = _ENDPOINT_P(cproc);

  assert(fproc[childno].fp_pid == PID_FREE);

  /* Save child's own mutex before overwriting */
  c_fp_lock = fproc[childno].fp_lock;
  fproc[childno] = fproc[parentno];       /* Copy entire fproc */
  fproc[childno].fp_lock = c_fp_lock;     /* Restore child's mutex */

  cp = &fproc[childno];
  // ... 后续操作见 13-15 ...
}
```
