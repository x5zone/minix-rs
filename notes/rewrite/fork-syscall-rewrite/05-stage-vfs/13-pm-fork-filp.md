# 13-pm-fork-filp: pm_fork — filp 引用计数递增

> 本文档分析 `minix3/minix/servers/vfs/misc.c` 中 `pm_fork()` 函数的 filp 引用计数处理。

---

## 1. 概述

### 1.1 文件描述符共享语义

- POSIX 规定 fork 后父子进程共享所有打开的文件描述符——不是复制文件状态，而是共享同一个打开文件描述（open file description）。这意味着父子进程的 `fp_filp[fd]` 指向同一个 `filp` 结构体，共享文件偏移量（`filp_pos`）和文件状态标志（`filp_flags`）。当父进程 write() 后，子进程的 read() 从更新后的偏移量继续——这是管道和重定向的基础。共享通过指针复制实现：`pm_fork()` 中 `fproc[child] = fproc[parent]` 的浅拷贝使子进程的 `fp_filp[]` 与父进程指向相同的 filp，然后递增 `filp_count` 记录新的引用者。
---

## 2. filp 引用计数递增

### 2.1 遍历文件描述符表

  ```c
  cp = &fproc[childno];
  for (i = 0; i < OPEN_MAX; i++)
      if (cp->fp_filp[i] != NULL)
          cp->fp_filp[i]->filp_count++;
  ```

### 2.2 NULL 检查

- `if (cp->fp_filp[i] != NULL)`——只对已打开的文件描述符递增引用计数。NULL 表示该 fd 未使用，无需处理。遍历范围是 `0..OPEN_MAX`（0 到 254），线性扫描所有可能的文件描述符。这是 O(OPEN_MAX) 的操作，但在实践中 OPEN_MAX=255 很小，性能影响可忽略。
### 2.3 指针共享

- 整体复制后，子进程的 `fp_filp[i]` 与父进程指向同一个 filp 结构体——这是浅拷贝的自然结果。递增 `filp_count` 是必须的，因为现在有两个进程引用同一个 filp：父进程和子进程。如果不递增，当一方 close() 时 `filp_count` 降到 0 会释放 filp 和 vnode，而另一方仍在使用，导致 use-after-free。`filp_count` 记录了有多少个 fproc 槽位引用该 filp，fork 时从 N 变为 N+1。
---

## 3. 引用计数的效果

### 3.1 close() 行为

  - 父进程 close(fd): filp_count 从 2 降到 1，文件不关闭
  - 子进程 close(fd): filp_count 从 2 降到 1，文件不关闭
  - 双方 close(fd): filp_count 降到 0，调用 put_vnode() 释放 vnode

### 3.2 共享偏移量

  - 父进程 write(): filp_pos 更新，子进程看到新的偏移量
  - 子进程 read(): 从父进程 write 后的偏移量继续读

### 3.3 lseek() 行为

  - 任何一方的 lseek() 都影响另一方（因为共享 filp_pos）

---

## 4. FD_CLOEXEC 继承

### 4.1 fp_cloexec_set 的复制

- `fp_cloexec_set` 是 `fd_set` 类型的位图，标记哪些 fd 在 exec 时应关闭。整体复制 fproc 时，子进程自然继承了父进程的 `fp_cloexec_set`——因为 `fproc[child] = fproc[parent]` 是逐字节复制。这意味着如果父进程的 fd 3 设置了 FD_CLOEXEC，子进程的 fd 3 也会有 FD_CLOEXEC 标记。这是正确的 POSIX 语义：fork 后子进程继承父进程的所有 fd 属性，包括 close-on-exec 标志。
### 4.2 exec 时的行为

- exec 时，`pm_exec()` 遍历 `fp_filp[]`，检查 `fp_cloexec_set` 位图，对设置了 FD_CLOEXEC 的 fd 调用 `close_fp()` 关闭。`close_fp()` 递减 `filp_count`，若降到 0 则释放 filp 并调用 `put_vnode()` 释放 vnode。未设置 FD_CLOEXEC 的 fd 保持打开——这是 exec 后文件描述符保留的机制。fork 时继承的 cloexec 设置确保子进程在 exec 时只关闭应该关闭的文件。
---

## 5. 特殊文件描述符

### 5.1 标准输入/输出/错误

- fd 0（stdin）、fd 1（stdout）、fd 2（stderr）在 fork 时与其他文件描述符完全相同——共享 filp 并递增 `filp_count`。没有特殊处理。shell 在 fork 后通常会通过 `dup2()` 重定向子进程的标准 I/O（如管道），此时会创建新的 filp 替换原有的。fork 时共享标准 I/O 的典型场景是：前台进程组中，所有子进程共享终端设备的 filp。
### 5.2 管道文件描述符

- 管道文件描述符在 fork 时的处理与普通文件相同——共享 filp 并递增 `filp_count`。但管道的语义使 fork 特别重要：`pipe()` 创建两个 fd（读端和写端），fork 后父子进程都持有两端。典型用法是父进程关闭读端、子进程关闭写端（或反之），形成单向数据流。管道的 filp 指向 PFS（Pipe File System）的 vnode，`filp_count` 递增确保管道在所有引用者关闭前不会被释放。
### 5.3 设备文件描述符

- 设备文件描述符（如 `/dev/tty`）在 fork 时的处理与普通文件相同——共享 filp 并递增 `filp_count`。设备文件的 filp 指向设备驱动对应的 vnode，`filp_count` 递增确保设备文件在所有引用者关闭前不会被释放。设备文件的特殊之处在于 I/O 操作可能阻塞（如终端输入），但这对 fork 的 filp 处理没有影响——阻塞只影响工作线程调度，不影响引用计数。
---

## 6. 与 Kernel 的对比

### 6.1 Kernel 无引用计数

- Kernel 的 `do_fork()` 不需要引用计数机制，因为内核的 `struct proc` 不包含共享资源——每个进程有独立的地址空间、寄存器状态和内核栈。而 VFS 的 `fproc` 包含指向共享资源（filp、vnode）的指针，这些资源可能被多个进程同时引用。因此 VFS 需要引用计数来跟踪共享资源的用户数，确保最后一个用户关闭时才释放。
### 6.2 "先复制，后递增"模式

- `pm_fork()` 采用"先复制，后递增"模式：先通过 `fproc[child] = fproc[parent]` 整体复制（此时子进程的 filp 指针与父进程相同，但引用计数未更新），然后遍历子进程的 `fp_filp[]` 递增每个 filp 的 `filp_count`。这种模式简洁高效——避免了逐字段复制时遗漏的风险。在引用计数递增之前，存在一个短暂的"不一致窗口"：filp 已被两个进程引用，但 `filp_count` 只记录了一个。由于 `pm_fork()` 在主线程或工作线程中同步执行，不会被中断，因此这个窗口不会导致问题。
---

## 7. C 源码

**文件**: `minix3/minix/servers/vfs/misc.c` (filp 引用计数部分)

```c
/* Increase reference counts on open files */
cp = &fproc[childno];
for (i = 0; i < OPEN_MAX; i++)
    if (cp->fp_filp[i] != NULL)
        cp->fp_filp[i]->filp_count++;
```
