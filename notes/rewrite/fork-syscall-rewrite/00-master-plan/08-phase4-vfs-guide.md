# 阶段4：VFS 层实现指南


> **状态**: ❌ 待实现  
> **对应源码**: `minix/servers/vfs/misc.c`, `minix/servers/vfs/filedes.c`, `minix/servers/vfs/vnode.c`

---

## 1. 目标与范围

实现 VFS (Virtual File System) 层的 `pm_fork()` 处理，完成文件描述符表复制、filp 引用计数管理、vnode 引用计数管理。

**硬件相关**: 磁盘 I/O 操作（使用 Mock）  
**软件逻辑**: filp/vnode 引用计数、文件描述符表复制、工作目录继承（真实实现）

**核心逻辑**: filp/vnode 引用计数、文件描述符表复制、工作目录继承

---

## 2. 任务清单

| # | 任务 | 依赖 | 目标文件 |
|---|------|------|---------|
| 4.1 | 定义 `FpFlags` 位标志 | 无 | `os/servers/vfs/src/fproc.rs` |
| 4.2 | 定义 `FilpRef` 结构体 (index 指向全局 filp 表) | 无 | `os/servers/vfs/src/fproc.rs` |
| 4.3 | 定义 `VNodeRef` 结构体 (index 指向全局 vnode 表) | 无 | `os/servers/vfs/src/fproc.rs` |
| 4.4 | 定义 `Filp` 结构体 (mode, flags, count, vnode, pos) | 4.3 | `os/servers/vfs/src/fproc.rs` |
| 4.5 | 定义 `FProc` 结构体 (对齐 struct fproc) | 4.1-4.4 | `os/servers/vfs/src/fproc.rs` |
| 4.6 | 定义 `VfsProcTable` + 全局 `filps` + `vnodes` | 4.4, 4.5 | `os/servers/vfs/src/fproc.rs` |
| 4.7 | 实现 `dup_vnode()` — v_ref_count++ | 4.3 | `os/servers/vfs/src/fork.rs` |
| 4.8 | 实现 `FProc::fork_from()` — fproc 复制逻辑 | 4.5-4.7 | `os/servers/vfs/src/fork.rs` |
| 4.9 | 实现 filp_count++ 循环 | 4.8 | `os/servers/vfs/src/fork.rs` |
| 4.10 | 实现 fp_lock 保留逻辑 | 4.8 | `os/servers/vfs/src/fork.rs` |
| 4.11 | 实现 `okendpt()` 验证 | 4.5 | `os/servers/vfs/src/fork.rs` |
| 4.12 | 实现 `close_filp()` — filp_count-- 递减 | 4.4 | `os/servers/vfs/src/close.rs` |
| 4.13 | 实现 `put_vnode()` — v_ref_count-- 延迟同步 | 4.3 | `os/servers/vfs/src/close.rs` |
| 4.14 | 更新 `os/servers/vfs/src/lib.rs` 模块导出 | 4.8 | `os/servers/vfs/src/lib.rs` |
| 4.15 | 更新 `os/servers/vfs/Cargo.toml` 依赖 | 4.8 | `os/servers/vfs/Cargo.toml` |
| 4.16 | 编写 VFS 层单元测试 | 4.8 | `os/servers/vfs/src/fork.rs` + `close.rs` |

---

## 3. FProc 结构体设计

### 3.1 核心结构体定义

```rust
// os/servers/vfs/src/fproc.rs

/// VFS 进程表条目
pub struct FProc {
    pub flags: FpFlags,              // 进程标志
    pub pid: Pid,                    // 进程 PID
    pub endpoint: Endpoint,          // 内核端点
    pub root_dir: Option<VNodeRef>,  // 根目录 vnode
    pub work_dir: Option<VNodeRef>,  // 工作目录 vnode
    pub filps: [Option<FilpRef>; OPEN_MAX], // 文件描述符表
    pub cloexec_set: u64,            // FD_CLOEXEC 位图
    pub real_uid: Uid,               // 实际用户ID
    pub eff_uid: Uid,                // 有效用户ID
    pub real_gid: Gid,               // 实际组ID
    pub eff_gid: Gid,                // 有效组ID
    pub ngroups: usize,              // 补充组数量
    pub supplemental_groups: [Gid; NGROUPS_MAX], // 补充组
    pub umask: u16,                  // 文件创建掩码
    pub name: [u8; PROC_NAME_LEN],   // 进程名
}

/// 文件描述符引用（指向全局 filp 表）
pub struct FilpRef {
    pub index: usize,  // 在全局 filp 表中的索引
}

/// VNode 引用（指向全局 vnode 表）
pub struct VNodeRef {
    pub index: usize,  // 在全局 vnode 表中的索引
}

/// 文件描述符条目（全局表）
pub struct Filp {
    pub mode: FileMode,       // 打开模式
    pub flags: OpenFlags,     // 打开标志
    pub count: u32,           // 引用计数
    pub vnode: Option<VNodeRef>, // 关联的 vnode
    pub pos: u64,             // 当前文件偏移量（共享）
    pub lock: FileLock,       // 文件锁状态
}

/// VNode 条目（全局表）
pub struct VNodeEntry {
    pub inode_nr: u64,        // inode 号
    pub fs_endpoint: Endpoint, // 文件系统服务端点
    pub ref_count: u32,       // 引用计数
    pub fs_count: u32,        // 文件系统引用计数（延迟同步）
    pub v_mode: FileMode,     // 文件模式
    pub v_size: u64,          // 文件大小
    pub v_uid: Uid,           // 文件所有者
    pub v_gid: Gid,           // 文件组
}
```

### 3.2 FpFlags 位标志

```rust
bitflags! {
    pub struct FpFlags: u32 {
        const NO_FLAGS = 0;
        const EXITING = 0x01;      // 进程正在退出
        const SETUID = 0x02;       // setuid 执行
        const SETGID = 0x04;       // setgid 执行
        const TRACE = 0x08;        // 被追踪
        const TRACED = 0x10;       // 已停止追踪
        const STOPPED = 0x20;      // 进程停止
    }
}
```

---

## 4. 文件描述符表复制

### 4.1 `FProc::fork_from()` 实现

```rust
impl FProc {
    pub fn fork_from(
        parent: &FProc,
        child_pid: Pid,
        child_endpoint: Endpoint,
        filps: &mut Vec<Filp>,
        vnodes: &mut Vec<VNodeEntry>,
    ) -> Self {
        // 复制文件描述符表，递增每个打开文件的引用计数
        let mut child_filps = parent.filps;
        for filp_ref in child_filps.iter().flatten() {
            if filp_ref.index < filps.len() {
                filps[filp_ref.index].count += 1;
            }
        }

        // 工作目录和根目录直接引用计数+1
        if let Some(ref rd) = parent.root_dir {
            if rd.index < vnodes.len() {
                vnodes[rd.index].ref_count += 1;
            }
        }
        if let Some(ref wd) = parent.work_dir {
            if wd.index < vnodes.len() {
                vnodes[wd.index].ref_count += 1;
            }
        }

        FProc {
            flags: FpFlags::empty(), // 清除所有标志
            pid: child_pid,
            endpoint: child_endpoint,
            root_dir: parent.root_dir.clone(),
            work_dir: parent.work_dir.clone(),
            filps: child_filps,
            cloexec_set: parent.cloexec_set, // 继承 cloexec 标志
            real_uid: parent.real_uid,
            eff_uid: parent.eff_uid,
            real_gid: parent.real_gid,
            eff_gid: parent.eff_gid,
            ngroups: parent.ngroups,
            supplemental_groups: parent.supplemental_groups,
            umask: parent.umask,
            name: parent.name,
        }
    }
}
```

---

## 5. Filp 引用计数

### 5.1 引用计数递增（fork 时）

```rust
// fork 时遍历所有打开的文件描述符，递增引用计数
for fd in 0..OPEN_MAX {
    if let Some(ref filp_ref) = parent.filps[fd] {
        filps[filp_ref.index].count += 1;
    }
}
```

### 5.2 引用计数递减（close 时）

```rust
impl Filp {
    pub fn close(&mut self, vnodes: &mut Vec<VNodeEntry>) -> bool {
        self.count -= 1;
        if self.count == 0 {
            // 最后一个引用，释放关联vnode
            if let Some(ref vn) = self.vnode {
                vnodes[vn.index].ref_count -= 1;
            }
            self.vnode = None;
            return true; // 可以释放 filp 槽位
        }
        // 还有其他引用，仅解锁vnode
        if let Some(ref vn) = self.vnode {
            unlock_vnode(&vnodes[vn.index]);
        }
        false // filp 仍在使用中
    }
}
```

### 5.3 关键规则

| 场景 | 行为 |
|------|------|
| fork | filp_count++，父子共享文件偏移量 |
| close (count > 1) | filp_count--，不关闭文件 |
| close (count == 1) | filp_count--，释放 vnode，关闭文件 |

---

## 6. VNode 引用计数

### 6.1 `dup_vnode()` 递增引用

```rust
pub fn dup_vnode(vnodes: &mut Vec<VNodeEntry>, vnode_ref: &VNodeRef) {
    if vnode_ref.index < vnodes.len() {
        vnodes[vnode_ref.index].ref_count += 1;
        // 注意：v_fs_count 不递增，延迟同步
    }
}
```

### 6.2 `put_vnode()` 延迟同步

```rust
pub fn put_vnode(vp: &mut VNodeEntry) {
    if vp.ref_count > 1 {
        vp.ref_count -= 1;
        // v_fs_count 延迟压缩，超过阈值才批量处理
        if vp.fs_count > 256 {
            vnode_clean_refs(vp);
        }
        return;
    }
    // 最后一个引用，通知底层文件系统释放
    req_putnode(vp.fs_endpoint, vp.inode_nr, vp.fs_count);
    vp.fs_count = 0;
    vp.ref_count = 0;
}
```

### 6.3 引用计数关系

```
┌─────────────────────────────────────────────────────────────┐
│                    VFS 引用计数关系图                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  父进程 fproc                    子进程 fproc               │
│  ┌─────────────┐                ┌─────────────┐             │
│  │ fp_filp[0] ─┼──┐         ┌──┼─ fp_filp[0] │             │
│  │ fp_filp[1] ─┼──┼──┐   ┌──┼──┼─ fp_filp[1] │             │
│  └─────────────┘  │  │   │  │  └─────────────┘             │
│                   │  │   │  │                               │
│                   ▼  ▼   ▼  ▼                               │
│              全局 filp 表（引用计数）                         │
│              ┌─────────────────┐                            │
│              │ filp[0]: count=2 │◄── 共享文件偏移量          │
│              │ filp[1]: count=2 │◄── 共享文件偏移量          │
│              └─────────────────┘                            │
│                      │                                      │
│                      ▼                                      │
│              全局 vnode 表（引用计数）                        │
│              ┌─────────────────┐                            │
│              │ vnode[0]: ref=2 │                            │
│              │ vnode[1]: ref=2 │                            │
│              └─────────────────┘                            │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 7. 工作目录处理

### 7.1 根目录和工作目录继承

```rust
// fork 时只递增 root_dir 和 work_dir 的引用计数
// 不遍历整个 vnode 表

if let Some(ref rd) = parent.root_dir {
    dup_vnode(vnodes, rd);  // ref_count++
}
if let Some(ref wd) = parent.work_dir {
    dup_vnode(vnodes, wd);  // ref_count++
}
```

### 7.2 为什么不对所有 fd 的 vnode 调用 dup_vnode

- **filp 已经维护 vnode 引用**: 每个 filp 指向一个 vnode，filp_count 间接维护 vnode 生命周期
- **避免重复计数**: fork 时 filp_count++ 已经保证 vnode 不会被释放
- **延迟同步优化**: 只有 root/work 目录需要直接递增 v_ref_count

---

## 8. 测试用例

```rust
#[test]
fn test_fproc_fork_from_filp_count_incremented() {
    let parent = create_parent_with_open_files();
    let filp_count_before = filps[0].count;
    
    let child = FProc::fork_from(&parent, child_pid, child_ep, &mut filps, &mut vnodes);
    
    assert_eq!(filps[0].count, filp_count_before + 1);
}

#[test]
fn test_fproc_fork_from_vnode_ref_count_incremented() {
    let parent = create_parent_with_work_dir();
    let vnode_ref_before = vnodes[work_dir_index].ref_count;
    
    let child = FProc::fork_from(&parent, child_pid, child_ep, &mut filps, &mut vnodes);
    
    assert_eq!(vnodes[work_dir_index].ref_count, vnode_ref_before + 1);
}

#[test]
fn test_close_filp_decrements_count() {
    let mut filp = Filp { count: 2, /* ... */ };
    filp.close(&mut vnodes);
    assert_eq!(filp.count, 1);
}

#[test]
fn test_close_filp_releases_vnode_at_zero() {
    let mut filp = Filp { count: 1, vnode: Some(vnode_ref), /* ... */ };
    let released = filp.close(&mut vnodes);
    assert!(released);
    assert!(filp.vnode.is_none());
}
```

---

## 9. 检查清单

| # | 逻辑点 | 状态 |
|---|--------|------|
| F-01 | `FProc` 结构体对齐 struct fproc | ⬜ |
| F-02 | `FpFlags` 完整位标志定义 | ⬜ |
| F-03 | `Filp` 结构体包含 count 引用计数字段 | ⬜ |
| F-04 | `Filp` 包含 pos 字段（共享文件偏移量） | ⬜ |
| F-05 | `VNodeRef` 结构体定义 | ⬜ |
| F-06 | `FilpRef` 结构体定义 | ⬜ |
| F-07 | fork 时 filp_count++ 遍历所有打开文件 | ⬜ |
| F-08 | 父子进程共享 filp 对象与文件偏移量 | ⬜ |
| F-09 | `close_filp()` 递减 filp_count | ⬜ |
| F-10 | filp_count 降到0才释放vnode | ⬜ |
| F-11 | filp_count>0时仅解锁vnode | ⬜ |
| F-12 | fork后父进程close仅递减计数不关闭文件 | ⬜ |
| F-13 | `dup_vnode()` 仅对 root/work 目录递增计数 | ⬜ |
| F-14 | dup_vnode 不递增 v_fs_count | ⬜ |
| F-15 | `put_vnode()` 递减 v_ref_count | ⬜ |
| F-16 | v_ref_count>1时仅递减不通知FS | ⬜ |
| F-17 | v_ref_count==1时通知FS释放 | ⬜ |
| F-18 | v_fs_count 延迟同步逻辑 | ⬜ |
| F-19 | 明确区分 filp 间接维护 vnode 引用 vs direct 引用 | ⬜ |
| F-20 | `okendpt()` 验证父进程 endpoint | ⬜ |
| F-21 | 子进程 slot 不做 endpoint 验证 | ⬜ |
| F-22 | 子进程槽位空闲检查 `fp_pid == PID_FREE` | ⬜ |
| F-23 | `fp_lock` 保留（不随结构体拷贝） | ⬜ |
| F-24 | `fp_pid` / `fp_endpoint` 正确设置 | ⬜ |
| F-25 | `fp_flags` 清除为 FP_NOFLAGS | ⬜ |
| F-26 | `fp_cloexec_set` 完整继承 | ⬜ |
