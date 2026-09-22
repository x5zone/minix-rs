# 阶段 4：VFS — 文件描述符复制与引用计数实现
> **状态**: ❌ 待实现
> **硬件依赖**: 磁盘IO Mock
> **Mock说明**: 所有磁盘读写、inode操作全部使用Mock，仅实现filp引用计数递增、vnode引用计数管理逻辑
> **对应源码**: `minix/servers/vfs/misc.c`, `minix/servers/vfs/filedes.c`, `minix/servers/vfs/vnode.c`

---

## 一、任务清单
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

## 二、核心实现模板
### 2.1 `FProc::fork_from()` fproc 复制
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

### 2.2 `close_filp()` 引用计数递减
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
            return true;
        }
        // 还有其他引用，仅解锁vnode
        if let Some(ref vn) = self.vnode {
            unlock_vnode(&vnodes[vn.index]);
        }
        false
    }
}
```

### 2.3 `put_vnode()` 延迟同步
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

---

## 三、必须通过的单元测试
```
test_fproc_fork_from_filp_count_incremented()
test_fproc_fork_from_vnode_ref_count_incremented()
test_fproc_fork_from_flags_cleared()
test_fproc_fork_from_pid_endpoint_set()
test_fproc_fork_from_cloexec_inherited()
test_close_filp_decrements_count()
test_close_filp_releases_vnode_at_zero()
test_close_filp_keeps_vnode_above_zero()
test_dup_vnode_increments_ref_count()
test_okendpt_validates_endpoint()
```

---

## 四、检查清单
| # | 逻辑点 | 状态 |
|---|--------|------|
| F-01 | `FProc` 结构体对齐 struct fproc | ❌ |
| F-02 | `FpFlags` 完整位标志定义 | ❌ |
| F-03 | `Filp` 结构体包含 count 引用计数字段 | ❌ |
| F-04 | `Filp` 包含 pos 字段（共享文件偏移量） | ❌ |
| F-05 | `VNodeRef` 结构体定义 | ❌ |
| F-06 | `FilpRef` 结构体定义 | ❌ |
| F-07 | fork 时 filp_count++ 遍历所有打开文件 | ❌ |
| F-08 | 父子进程共享 filp 对象与文件偏移量 | ❌ |
| F-09 | `close_filp()` 递减 filp_count | ❌ |
| F-10 | filp_count 降到0才释放vnode | ❌ |
| F-11 | filp_count>0时仅解锁vnode | ❌ |
| F-12 | fork后父进程close仅递减计数不关闭文件 | ❌ |
| F-13 | `dup_vnode()` 仅对 root/work 目录递增计数 | ❌ |
| F-14 | dup_vnode 不递增 v_fs_count | ❌ |
| F-15 | `put_vnode()` 递减 v_ref_count | ❌ |
| F-16 | v_ref_count>1时仅递减不通知FS | ❌ |
| F-17 | v_ref_count==1时通知FS释放 | ❌ |
| F-18 | v_fs_count 延迟同步逻辑 | ❌ |
| F-19 | 明确区分 filp 间接维护 vnode 引用 vs direct 引用 | ❌ |
| F-20 | `okendpt()` 验证父进程 endpoint | ❌ |
| F-21 | 子进程 slot 不做 endpoint 验证 | ❌ |
| F-22 | 子进程槽位空闲检查 `fp_pid == PID_FREE` | ❌ |
| F-23 | `fp_lock` 保留（不随结构体拷贝） | ❌ |
| F-24 | `fp_pid` / `fp_endpoint` 正确设置 | ❌ |
| F-25 | `fp_flags` 清除为 FP_NOFLAGS | ❌ |
| F-26 | `fp_cloexec_set` 完整继承 | ❌ |