# 03-fproc-cred: FProc 凭证字段

> 本文档分析 `minix3/minix/servers/vfs/fproc.h` 中的 UID/GID/umask 等凭证相关字段。

---

## 1. 概述

### 1.1 凭证系统的作用

凭证字段控制文件系统访问权限——每个文件操作（open/read/write/exec）都需要检查进程的 UID/GID 是否具有相应权限
POSIX 进程凭证模型：real/effective UID/GID + 补充组。real UID/GID 标识进程的真实身份（由 login 设置），effective UID/GID 用于权限检查（setuid 时可不同），补充组扩展了进程的组权限

---

## 2. UID 字段

### 2.1 fp_realuid

`uid_t fp_realuid`——真实用户 ID，标识进程的实际拥有者
real UID 由 login 设置，fork 时继承。只有通过 setuid() 系统调用才能改变 real UID（需要特权）

### 2.2 fp_effuid

`uid_t fp_effuid`——有效用户 ID，用于文件系统权限检查
effective UID 用于权限检查——VFS 在 forbidden() 中使用 fp_effuid 判断进程是否有权访问文件
setuid 程序的 effective UID 与 real UID 不同——执行 setuid 程序时，fp_effuid 被设为文件属主的 UID，而 fp_realuid 保持不变。程序结束后恢复
fork 时子进程继承父进程的 effective UID——pm_fork() 整体复制 fproc，fp_effuid 自然被复制

### 2.3 super_user 宏

- `glo.h` 中的 `super_user` 宏：判断 fp_effuid 是否等于 SU_UID (0)，即当前进程是否为超级用户
SU_UID = 0 (root)——UID 为 0 的用户是超级用户，拥有所有权限。super_user 宏在权限检查中被广泛使用

---

## 3. GID 字段

### 3.1 fp_realgid

`gid_t fp_realgid`——真实组 ID，标识进程的实际组归属
fork 时继承——pm_fork() 整体复制 fproc，fp_realgid 自然被复制

### 3.2 fp_effgid

`gid_t fp_effgid`——有效组 ID，用于文件系统组权限检查
fork 时继承——pm_fork() 整体复制 fproc，fp_realgid 自然被复制

---

## 4. 补充组

### 4.1 fp_ngroups

`int fp_ngroups`——补充组数量，0 表示无补充组
0 表示无补充组。initgroups() 系统调用设置补充组列表

### 4.2 fp_sgroups[NGROUPS_MAX]

`gid_t fp_sgroups[NGROUPS_MAX]`——补充组数组，存储进程所属的额外组 ID
fork 时整个数组被复制——pm_fork() 整体复制 fproc，fp_sgroups 自然被复制

---

## 5. umask

### 5.1 fp_umask (隐含字段)

fproc 中 umask 字段位于 fproc.h 第 77 行：`mode_t fp_umask;`
umask 对文件创建权限的影响：新文件的最终权限 = 创建模式 ~fp_umask。例如 umask=0022 时，open("file", 0666) 创建的文件权限为 0644
fork 时子进程继承父进程的 umask——pm_fork() 整体复制 fproc，fp_umask 自然被复制

---

## 6. 权限检查相关函数

### 6.1 forbidden() — 权限检查

`protect.c` 中的 `forbidden()` 函数——VFS 的核心权限检查函数，判断进程是否有权访问指定 vnode
权限判断逻辑：先检查 super_user（fp_effuid == 0），若非特权则检查文件属主/组/其他的权限位，组匹配时遍历 fp_sgroups
与 fork 的间接关系——子进程继承凭证后，权限检查自然适用。forbidden() 使用的是 fp_effuid/fp_effgid/fp_sgroups，这些字段在 fork 时被完整复制

### 6.2 read_only() — 只读检查

`protect.c` 中的 `read_only()` 函数——检查 vnode 所属的挂载点是否为只读
挂载点只读标志检查：read_only() 通过 vnode 的 v_vmnt 指针找到 vmnt，检查 m_flags 中的 VMNT_READONLY 标志。写操作前必须调用 read_only() 确认

---

## 7. fork 时的处理

| 字段 | fork 处理 | 说明 |
|------|----------|------|
| `fp_realuid` | 整体复制 | 继承真实 UID |
| `fp_effuid` | 整体复制 | 继承有效 UID |
| `fp_realgid` | 整体复制 | 继承真实 GID |
| `fp_effgid` | 整体复制 | 继承有效 GID |
| `fp_ngroups` | 整体复制 | 继承补充组数量 |
| `fp_sgroups[]` | 整体复制 | 继承补充组列表 |
| `fp_umask` | 整体复制 | 继承 umask |

**POSIX 语义**: fork 后子进程完全继承父进程的凭证，这是 POSIX 规定的行为。

---

## 8. C 源码

**文件**: `minix3/minix/servers/vfs/fproc.h`

```c
uid_t fp_realuid, fp_effuid;	/* user and group IDs */
gid_t fp_realgid, fp_effgid;
int fp_ngroups;			/* supplementary group count */
gid_t fp_sgroups[NGROUPS_MAX];	/* supplementary groups */
mode_t fp_umask;		/* umask */
```
