# 25-rs-services: RS 服务 —— Live Update 中 VM 的内存状态迁移执行者

> **分类**: 阶段 8 — 跨服务协作（RS 服务：SET_PRIV / PREPARE / UPDATE / MEMCTL）
> **源码**: `minix3/minix/servers/vm/rs.c`（全 391 行：`do_rs_set_priv` :34-66 / `do_rs_prepare` :71-145 / `do_rs_update` :150-213 / `rs_memctl_make_vm_instance` :218-276 / `rs_memctl_heap_prealloc` :281-295 / `rs_memctl_map_prealloc` :300-324 / `rs_memctl_get_prealloc_map` :329-344 / `do_rs_memctl` :349-390）+ 调用面（`main.c`：`map_service` :755-768、`sef_cb_init_fresh` rproctab 复制 :241-260（sys_safecopyfrom :246-250）、`adjust_proc_refs` 调用点 :215/:722；`utility.c`：`adjust_proc_refs` :477-492；`minix/rs.h`：`struct rprocpub` :165-183、`SF_VM_ROLLBACK` :198、`SF_VM_NOMMAP` :199、`IS_RPUB_BOOT_USR` :188；`minix/com.h`：请求码 :724/:736/:738/:766、MEMCTL 子请求 :741-745；`minix/ipc.h`：`mess_lsys_vm_update` :1529-1534）
> **Rust 模块**: `os/servers/vm/src/rs.rs`（`RsError` :34 / `RsMemctlRequest` :72 / `RsMemctlResult` :81 / `RsUpdateResult` :87 / `RsUpdateFlags` :102 / `handle_rs_set_priv` :122 / `handle_rs_prepare` :168 / `handle_rs_update` :285 / `handle_rs_memctl` :339）+ `os/servers/vm/src/vm_server.rs`（`rs_handshake` :659 / `ipc_call_rs_init` :946 / `RprocEntry` :1013 / `RprocTab` :1029 / `RsMemctlAddrLen` 回复编码 :1166）+ `os/servers/vm/src/ipc/dispatcher.rs`（`dispatch_rs_set_priv` :829 / `dispatch_rs_prepare` :843 / `dispatch_rs_update` :858 / `dispatch_rs_memctl` :874 / CALLMAP 解码 :1062-1103 / `From<RsError>` :1284-1306）
> **前置**: `15-ipc-dispatch.md`（主循环分发）、`22-vm-exit.md`（LU 后旧实例退出路径）
> **说明**: 本文档管 **RS（Reincarnation Server）驱动的 live update 中 VM 的全部服务面**——4 个 IPC handler（权限设置、准备、更新、内存控制）、`RprocTab` 握手语义、`adjust_proc_refs`，以及 A-8 缺口契约（RS_PREPARE 第 5 步 / RS_UPDATE 切换未实现）。**不覆盖**：SEF 生命周期与 `map_service` 启动注册的完整叙事（01）、swap_proc_* / map_proc_dyn_data 机制本体（10）、ACL 位图语义（04）、主循环分发框架（15）、查询服务（26）。

---

## 1. 概念：VM 在系统服务热更新里的角色

### 1.0 章节引言

**目标读者**：已读完 15（主循环分发）、10（搬迁机制）、19（brk）、20（mmap）的读者。本文档回答三个问题：为什么 live update 必须由 VM 执行内存迁移？RS 通过哪四个请求指挥 VM？"LU 窗口内不得分配"的约束如何在代码里落地？

**本章不讲什么**：SEF 框架回调注册与 RS_INIT 异步回复的启动叙事（01）；CALLMAP 分发框架与 SUSPEND 机制本身（15）；`swap_proc_slot`/`swap_proc_dyn_data` 的实现细节（10）；ACL 位图与 fail-closed 语义（04）——这里只讲"RS 与 VM 的服务对话"这一侧。

### 1.1 为什么 live update 需要 VM 参与

Minix3 的 live update（LU）允许 RS 在不重启系统的前提下替换一个运行中的服务：RS 创建新实例 → 迁移状态 → 切换端点 → 回收旧实例。整个过程对用户进程透明。

服务更新的本质是**新旧两个进程实例的地址空间交接**——而地址空间（物理页 + 页表 + 区域）只有 VM 拥有。于是：

```
RS（服务管理器，编排者）
  │  4 个 IPC：SET_PRIV / PREPARE / UPDATE / MEMCTL
  ▼
VM（内存状态迁移的执行者）
  ├─ 授权：为新实例设置 VM 调用权限（SET_PRIV → acl_set）
  ├─ 钉住：PREPARE/MEMCTL(PIN) 把双方内存全部映射到物理页，杜绝 LU 窗口内缺页
  ├─ 预分配：HEAP_PREALLOC / MAP_PREALLOC 在窗口前备齐堆与 mmap 区域
  └─ 交换：UPDATE 时内核切 endpoint + VM 交换槽数据与区域，手动回复 + SUSPEND
```

内核只做一件事：`sys_update(src, dst, flags)` 把两个进程的 endpoint 互换（旧进程获得新端点，客户端继续经原端点访问）。真正让新实例"接住"旧实例内存的活全在 VM。

对照另两家 OS：
- **Linux**：内核补丁（kpatch/livepatch）做函数级热替换，**没有进程级 live update**——服务重启 = fork/exec 或容器重建，不迁移地址空间。
- **Redox**：scheme 模型下服务重启即重建进程，内核不提供跨进程地址空间迁移；无 RS 式的"服务编排 + 内存迁移"组合。
- **Minix3 的位置**：微内核用户态服务是系统的基础构件（VFS/PM/RS 都是服务），热更新服务比重启整机划算；LU 是 **RS 编排 + VM 内存迁移 + 内核 endpoint 切换**三者的协作——VM 是唯一同时持有物理页、页表与区域元数据的模块，所以它是内存侧的必然执行者。

**对照要点**：三家的共同语义是"更新必须不中断对外服务"；Minix3 的独特之处是 **地址空间级别的热迁移**（新旧实例共享物理页直到切换），这要求 VM 精确控制"窗口内零分配"。

### 1.2 四个请求：授权、钉住、交换、预分配

| 请求码 | 值 | handler | 功能 | RS 调用时机 |
|--------|----|---------|------|-------------|
| `VM_RS_SET_PRIV` | 0xC25 | `do_rs_set_priv`（rs.c:34） | 设置进程的 VM 调用权限位图 | 启动新服务实例时 |
| `VM_RS_PREPARE` | 0xC30 | `do_rs_prepare`（rs.c:71） | 钉住双方内存 + 对齐堆 + CoW 预映射 mmap | 多组件 LU（含 VM 自身）前 |
| `VM_RS_UPDATE` | 0xC29 | `do_rs_update`（rs.c:150） | 内核切端点 + VM 交换槽与区域 | 切换时刻 |
| `VM_RS_MEMCTL` | 0xC2A | `do_rs_memctl`（rs.c:349） | 5 个子请求：钉住 / 造 VM 实例 / 预分配堆 / 预分配 mmap / 查询预分配 | 准备更新环境时 |

依赖关系：`SET_PRIV` 独立；`MEMCTL` 独立；`PREPARE` 逻辑上依赖 `SET_PRIV`（目标进程必须先有正确权限）；`UPDATE` 依赖 `PREPARE`（内存状态必须已备齐）。

**四个请求对应 VM 的四个角色**：
1. **授权者**（SET_PRIV）：`acl_set(vmp, call_mask, is_sys)` 写入进程的 VM 调用权限——新实例能调用哪些 VM 服务由 RS 显式授予。
2. **钉住者**（PREPARE/MEMCTL(PIN)）：`map_pin_memory` 把进程全部区域逐页映射到物理页——LU 窗口内 VM 不能处理缺页（VM 可能正在被替换），所以窗口前把所有页都 fault in。
3. **预分配者**（MEMCTL(HEAP/MAP_PREALLOC)）：在窗口前把堆扩到指定大小、把 mmap 区域建好并打上 `VR_PREALLOC_MAP` 标记。
4. **交换者**（UPDATE）：`sys_update` 之后交换两个 vmproc 槽的数据与动态区域，重新绑定页表。

### 1.3 "LU 窗口内 VM 不得分配新对象"约束

rs.c:73-78 的注释声明了 LU 的根本约束：**多组件更新（含 VM 自身）期间，VM 不能分配任何新对象**（物理页、slab 对象、区域节点），否则回滚时无法恢复——新 VM 实例分配的对象对旧 VM 可见，回滚就失去了"旧状态"。

因此 PREPARE 的每一步都是"把未来可能需要的内存提前备齐"：
- `map_pin_memory`：所有页提前映射，窗口内零缺页；
- 堆对齐（`real_brk`）：宁可多占内存也要保证目标进程窗口内不缺堆——"better safe than sorry"（rs.c:110-117 注释）；
- `map_proc_dyn_data`：源进程的 mmap 区域以 CoW 方式预映射到目标进程——窗口内即使目标进程触碰这些区域也是已映射页，不需要 VM 分配。

**对照 Linux**：`mlock`/`mlockall` 钉住页面是同一思想（禁止换出/缺页）；但 Linux 的"窗口"是用户态进程自身的，没有"VM 正在被替换"这种自举场景。

### 1.4 UPDATE 的 SUSPEND 与手动回复

`do_rs_update` 的回复逻辑是主循环框架的特殊案例（手动回复代码在 rs.c:201-210；主循环框架的 SUSPEND 抑制在 main.c:178-191）：

```c
if(reply_e != VM_PROC_NR) {
    if(reply_e == src_e) reply_e = dst_e;      /* endpoint 已互换，回复对象要对调 */
    else if(reply_e == dst_e) reply_e = src_e;
    m_ptr->m_type = OK;
    ipc_send(reply_e, m_ptr);                  /* 手动回复 */
}
return SUSPEND;                                 /* 阻止主循环自动回复 */
```

为什么不能靠主循环自动回复？`sys_update` 之后 src/dst 两个进程的 endpoint 已经互换——如果主循环按 `m_source` 原样回复，会把回复发给**切换前的**调用者，而调用者可能已经不是 RS 期望的进程。所以 handler 先把 `reply_e` 按 src↔dst 对调，手动 `ipc_send`，再返回 `SUSPEND`（=-998）告诉主循环"我已经回复过了，别动"。主循环收到 SUSPEND 的分支在 15 篇 §1.4（`DispatchAction::Suspend`）。

### 1.5 RprocTab：RS 公开的系统进程表

RS 维护一张**公开的系统进程表** `struct rprocpub[NR_SYS_PROCS]`（rs.h:165-183，NR_SYS_PROCS=64）：每个条目描述一个已注册服务的公开信息——`endpoint`、`vm_call_mask`（VM 调用权限位图）、`label`、`sys_flags` 等。

VM 在 RS_INIT 握手时拿到这张表（main.c:241-260，sys_safecopyfrom :246-250）：

```
RS_INIT（主循环优先级 2，15 篇 §1.2）
  └─ sys_safecopyfrom(RS, rproctab_gid, 0, rprocpub, sizeof(rprocpub))
       └─ for i in 0..NR_BOOT_PROCS: if rprocpub[i].in_use → map_service(&rprocpub[i])
             └─ acl_set(&vmproc[n], rpub->vm_call_mask, !IS_RPUB_BOOT_USR(rpub))
```

`map_service`（main.c:755-768）本质就是 `acl_set` 的一次封装：验证 endpoint → 把 RS 声明的调用掩码写入该进程的 ACL。`IS_RPUB_BOOT_USR(rpub)`（rs.h:188）= `rpub->endpoint == INIT_PROC_NR`——boot 进程里只有 init 是"用户进程"，其余都是系统进程（`!IS_RPUB_BOOT_USR` = is_sys）。

Rust 侧对应：`RprocTab`/`RprocEntry`（vm_server.rs:1013-1046）+ `rs_handshake`（vm_server.rs:659-681）复刻"复制表 → 逐条 acl_set"两步（真实 IPC 复制 DEFERRED，见 §3.9/§4.6）。

### 1.6 `adjust_proc_refs`：交换后的 region parent 重绑定

LU 切换（`swap_proc_slot`）把两个 vmproc 的内容整体交换——每个 `vir_region.parent` 指针原本指向各自的 vmproc，交换后必须重新指向新归属。`adjust_proc_refs`（utility.c:477-492）遍历所有进程的所有区域，把 `vr->parent` 统一重设为 `vmp`。

调用点（main.c:215/:722）：`sef_cb_lu_state_changed`（main.c:196，LU 失败回滚后恢复）与 `sef_cb_init_lu_restart`（main.c:677，重启回调；其内部再调 `sef_cb_init_vm_multi_lu` :725）——即**任何可能发生槽交换的路径结束后**都要重绑 parent。Rust 侧：region 的 parent 建模为 `parent_slot: Option<UserSlot>`（不存裸指针），槽交换后语义天然正确——这是类型系统消除一类 C 指针 bug 的例子（10 篇 §3 已述）。

### 1.7 小结

RS 服务 = **授权**（SET_PRIV → acl_set）+ **钉住/预分配**（PREPARE/MEMCTL，窗口内零分配）+ **交换**（UPDATE，sys_update + 槽交换 + 手动回复 SUSPEND）+ **握手**（RS_INIT → RprocTab → map_service）+ **收尾**（adjust_proc_refs）。Rust 侧对应：`rs.rs`（4 个 handler + 类型化请求/结果）+ `vm_server.rs`（RprocTab + rs_handshake + 回复编码）+ `dispatcher.rs`（解码与分发接线）。

---

## 2. C 源码分析

### 2.1 IPC 接口：4 个请求码与消息字段

请求码（com.h:627 `VM_RQ_BASE 0xC00`）：

| 请求 | 定义 | 值 | 消息结构 |
|------|------|----|---------|
| `VM_RS_SET_PRIV` | com.h:724 | 0xC25 | m2：`VM_RS_NR`=m2_i1（目标 endpoint）、`VM_RS_BUF`=m2_l1（权限位图地址，0=默认）、`VM_RS_SYS`=m2_i2（是否系统进程） |
| `VM_RS_UPDATE` | com.h:736 | 0xC29 | `mess_lsys_vm_update`（ipc.h:1529-1534）：`src` / `dst` / `flags` |
| `VM_RS_MEMCTL` | com.h:738 | 0xC2A | `VM_RS_CTL_ENDPT`=m1_i1（目标）、`VM_RS_CTL_REQ`=m1_i2（子请求）、`VM_RS_CTL_ADDR`=m2_p1（addr 输出）、`VM_RS_CTL_LEN`=m2_i3（len 输入/输出） |
| `VM_RS_PREPARE` | com.h:766 | 0xC30 | `mess_lsys_vm_update`（同 UPDATE） |

MEMCTL 子请求（com.h:741-745）：

| 值 | 常量 | 功能 |
|----|------|------|
| 0 | `VM_RS_MEM_PIN` | 钉住进程内存（仅多 VM 实例时有效） |
| 1 | `VM_RS_MEM_MAKE_VM` | 将进程标记为 VM 实例（多 VM 支持） |
| 2 | `VM_RS_MEM_HEAP_PREALLOC` | 预分配堆空间 |
| 3 | `VM_RS_MEM_MAP_PREALLOC` | 预分配 mmap 区域 |
| 4 | `VM_RS_MEM_GET_PREALLOC_MAP` | 查询预分配的 mmap 区域 |

UPDATE/PREPARE 的标志位（rs.h:198-199）：`SF_VM_ROLLBACK 0x080`（回滚更新，反向切换）、`SF_VM_NOMMAP 0x100`（不迁移 mmap 区域）。

### 2.2 do_rs_set_priv — 权限设置（rs.c:34-66）

```c
nr = m->VM_RS_NR;                       // 目标 endpoint
if ((r = vm_isokendpt(nr, &n)) != OK) return EINVAL;
vmp = &vmproc[n];
if (m->VM_RS_BUF) {                     // 有权限位图：从 RS 地址空间复制
    r = sys_datacopy(m->m_source, (vir_bytes) m->VM_RS_BUF, SELF,
        (vir_bytes) call_mask, sizeof(call_mask));
    if (r != OK) return r;
    call_mask_p = call_mask;
} else {                                // 无位图
    if (m->VM_RS_SYS) {                 // 系统进程不能共享默认权限
        printf("VM: do_rs_set_priv: sys procs don't share!\n");
        return EINVAL;
    }
    call_mask_p = NULL;                 // NULL = 默认权限
}
acl_set(vmp, call_mask_p, m->VM_RS_SYS);
```

**关键行为**：① 验证目标 endpoint；② `VM_RS_BUF` 非零 → 经 `sys_datacopy` 从 RS 地址空间复制位图；③ 位图为零且 `VM_RS_SYS` 为真 → EINVAL（系统进程必须有显式位图——"sys procs don't share"）；④ `acl_set(vmp, mask, is_sys)`。`acl_set` 的位图语义见 04 篇 §2.2（`AclState::acl_set`）。

### 2.3 do_rs_prepare — 准备更新（rs.c:71-145）

仅用于**多组件 live update（含 VM 自身）**——此时所有进程都要提前备好，保证它们在新 VM 实例接管期间不需要 VM 做任何不可回滚的动作。5 步：

```c
/* 1. 验证源/目标 endpoint */
vm_isokendpt(src_e, &src_p) / vm_isokendpt(dst_e, &dst_p) → EINVAL

/* 2. 钉住源进程内存 */
map_pin_memory(src_vmp);

/* 3. 若源堆比目标堆大，扩展目标堆到源的大小 */
src_data_vr = region_search(&src->vm_regions_avl, VM_MMAPBASE, AVL_LESS);
dst_data_vr = region_search(&dst->vm_regions_avl, VM_MMAPBASE, AVL_LESS);
src_addr = src_data_vr->vaddr + src_data_vr->length;
dst_addr = dst_data_vr->vaddr + dst_data_vr->length;
if (src_addr > dst_addr) real_brk(dst_vmp, src_addr);

/* 4. 钉住目标进程内存 */
map_pin_memory(dst_vmp);

/* 5. 非 NOMMAP：把源进程的 mmap 区域 CoW 映射到目标进程 */
if (!(sys_upd_flags & SF_VM_NOMMAP)) map_proc_dyn_data(src_vmp, dst_vmp);
```

**设计要点**：堆扩展"宁可浪费不可不足"——目标进程在 LU 窗口内不能分配新内存，所以必须预分配足够堆空间（rs.c:106-115 注释 "Better safe than sorry"）。注意 `region_search(VM_MMAPBASE, AVL_LESS)` 取的是 MMAPBASE 以下最大的区域——在 `_MINIX_MAGIC` 构建下 `VM_MMAPBASE = VM_MMAPTOP/2`（vm.h:74-76，mmap 区域严格高于堆），该区域就是堆/数据区；`map_proc_dyn_data` 的机制（transfer_mmap_regions → map_proc_copy_range）在 10 篇 §2.4。

### 2.4 do_rs_update — 执行更新（rs.c:150-213）

```c
/* 1. 验证端点 */                       → EINVAL
/* 2. 非回滚且非 NOMMAP 时，目标不能带预分配 mmap 区域 */
if((sys_upd_flags & (SF_VM_ROLLBACK|SF_VM_NOMMAP)) == 0) {
    if(map_region_lookup_type(dst_vmp, VR_PREALLOC_MAP)) return ENOSYS;
}
/* 3. 内核先切换端点 */
r = sys_update(src_e, dst_e, sys_upd_flags & SF_VM_ROLLBACK ? SYS_UPD_ROLLBACK : 0);
/* 4. VM 交换槽数据 */
r = swap_proc_slot(src_vmp, dst_vmp);
/* 5. VM 交换动态区域（mmap） */
r = swap_proc_dyn_data(src_vmp, dst_vmp, sys_upd_flags);
/* 6. 重新绑定页表 */
pt_bind(&src_vmp->vm_pt, src_vmp);  pt_bind(&dst_vmp->vm_pt, dst_vmp);
/* 7. 手动回复（端点已互换，见 §1.4）+ SUSPEND */
```

**关键行为**：第 2 步的 ENOSYS 检查——`VR_PREALLOC_MAP` 区域是"为 LU 预留的 mmap"，如果目标进程带预分配区域而本次更新又要迁移 mmap（非回滚非 NOMMAP），两批区域会冲突，直接拒绝（"Can't preallocate when transferring mmapped regions"，rs.c:174-177）。第 3 步 `sys_update` 是内核系统调用（`SYS_UPD_ROLLBACK` 标志位反向切换）。`swap_proc_slot`/`swap_proc_dyn_data` 的机制在 10 篇 §2.5/§2.6。

### 2.5 do_rs_memctl — 内存控制（rs.c:349-390）

```c
ep = m_ptr->VM_RS_CTL_ENDPT;  req = m_ptr->VM_RS_CTL_REQ;
if ((r = vm_isokendpt(ep, &proc_nr)) != OK) return EINVAL;
vmp = &vmproc[proc_nr];
switch(req) {
case VM_RS_MEM_PIN:
    /* 仅当 VM 能从崩溃恢复（多实例）时才真正钉住，省内存 */
    if (num_vm_instances <= 1) return OK;
    return map_pin_memory(vmp);
case VM_RS_MEM_MAKE_VM:      return rs_memctl_make_vm_instance(vmp);
case VM_RS_MEM_HEAP_PREALLOC:return rs_memctl_heap_prealloc(vmp, &addr, &len);
case VM_RS_MEM_MAP_PREALLOC: return rs_memctl_map_prealloc(vmp, &addr, &len);
case VM_RS_MEM_GET_PREALLOC_MAP: return rs_memctl_get_prealloc_map(vmp, &addr, &len);
default:                     return EINVAL;   /* rs.c:386-388 */
}
```

**注意 `VM_RS_CTL_ADDR`/`VM_RS_CTL_LEN` 的输入输出方向**：对 HEAP/MAP_PREALLOC，`len` 是输入（请求预分配的字节数）、`addr` 是输出（VM 写回实际地址）；对 GET_PREALLOC_MAP，两者都是输出。RS 侧 `vm_memctl`（libsys/vm_memctl.c）发送时填 `addr`/`len`，返回时读回——**回复必须写回 addr 与 len 两个槽**（C 布局：`VM_RS_CTL_ADDR`=m2_p1、`VM_RS_CTL_LEN`=m2_i3，com.h:746-747）。minix-rs 消息模型内，请求解码从 m2l1/m2i3 读 addr/len（dispatcher.rs:1094-1098），回复编码写 m1p1/m1i3（= 模型内 m2l1/m2i3 同槽，见 §3.10 #6）——注意 C 布局下 m2_p1（offset 40）≠ m2_l1（offset 24），因此「wire 等价」仅相对 minix-rs 消息模型成立；与 C RS 的真实 wire 传输需要 minix-types 的专用 overlay（A-8 下 transport 未落地，实际无跨进程字节交换）。

### 2.6 rs_memctl_* 辅助函数

#### rs_memctl_make_vm_instance（rs.c:218-276）

把一个进程标记为 VM 实例（用于 VM 自身热更新）：

1. 断言 `num_vm_instances == 1 || 2`；若已是 2 → `EPERM`（"no more than 2 VM instances"）；
2. `new_vm_vmp->vm_flags |= VMF_VM_INSTANCE; num_vm_instances++`；
3. `map_pin_memory(new_vm_vmp)` 钉住新实例；
4. 为当前 VM 与新实例在 `[VM_OWN_HEAPBASE, VM_DATATOP]` 预分配页表（`pt_ptalloc_in_range`）；
5. `pt_ptmap(this_vm, new_vm)` / `pt_ptmap(new_vm, new_vm)` 让新实例映射自己的页表。

#### rs_memctl_heap_prealloc（rs.c:281-295）

```c
if(*len <= 0) return EINVAL;
data_vr = region_search(&vmp->vm_regions_avl, VM_MMAPBASE, AVL_LESS);
*addr = data_vr->vaddr + data_vr->length;   /* 当前堆顶 */
bytes = *addr + *len;                        /* 目标堆顶 */
return real_brk(vmp, bytes);                 /* ENOMEM on failure */
```

#### rs_memctl_map_prealloc（rs.c:300-324）

```c
if(*len <= 0) return EINVAL;
*len = CLICK_CEIL(*len);                     /* 页对齐 */
is_vm = (vmp->vm_endpoint == VM_PROC_NR);
base = is_vm ? VM_OWN_MMAPBASE : VM_MMAPBASE;   /* VM 自身用 OWN 区间 */
top  = is_vm ? VM_OWN_MMAPTOP  : VM_MMAPTOP;
vr = map_page_region(vmp, base, top, *len,
    VR_ANON|VR_WRITABLE|VR_UNINITIALIZED, MF_PREALLOC, &mem_type_anon);
if (!vr) return ENOMEM;
vr->flags |= VR_PREALLOC_MAP;                /* 打上预分配标记 */
*addr = vr->vaddr;
```

`MF_PREALLOC` 的语义在 `map_page_region`（region.c:492-504）：`map_handle_memory` **立即分配全部页**（窗口内零分配的关键），然后**清除 VR_UNINITIALIZED**（"Pre-allocations should be uninitialized, but after that it's a different story"）。minix-rs 侧此"立即分配"是已知缺口（doc 20 backlog B1，见 §4.8）。

#### rs_memctl_get_prealloc_map（rs.c:329-344）

`map_region_lookup_type(vmp, VR_PREALLOC_MAP)` 找带预分配标记的区域；无 → `*addr=0, *len=0`；有 → `*addr=vr->vaddr, *len=vr->length`。

### 2.7 map_service 与 RprocTab 握手（main.c:755-768、241-260）

见 §1.5。握手路径是启动期一次性流程（`sef_cb_init_fresh`），Rust 侧由 `rs_handshake` 复刻（§4.6）。

### 2.8 adjust_proc_refs（utility.c:477-492）

```c
for(vmp = vmproc; vmp < &vmproc[VMP_NR]; vmp++) {
    if(!(vmp->vm_flags & VMF_INUSE)) continue;
    region_start_iter_least(&vmp->vm_regions_avl, &iter);
    while((vr = region_get_iter(&iter))) {
        USE(vr, vr->parent = vmp;);
        region_incr_iter(&iter);
    }
}
```

### 2.9 C 源码覆盖完整性

**语义范围**：rs.c 全部 8 个函数 + 调用面（map_service / adjust_proc_refs / rprocpub / SF_VM_* / 消息字段）。

| 符号 | 类型 | 源码位置 | 本篇覆盖 |
|------|------|---------|---------|
| do_rs_set_priv | 函数 | rs.c:34-66 | §2.2 |
| do_rs_prepare | 函数 | rs.c:71-145 | §2.3 |
| do_rs_update | 函数 | rs.c:150-213 | §2.4 |
| rs_memctl_make_vm_instance | 静态函数 | rs.c:218-276 | §2.6 |
| rs_memctl_heap_prealloc | 静态函数 | rs.c:281-295 | §2.6 |
| rs_memctl_map_prealloc | 静态函数 | rs.c:300-324 | §2.6 |
| rs_memctl_get_prealloc_map | 静态函数 | rs.c:329-344 | §2.6 |
| do_rs_memctl | 函数 | rs.c:349-390 | §2.5 |
| map_service | 静态函数 | main.c:755-768 | §2.7 |
| adjust_proc_refs | 函数 | utility.c:477-492 | §2.8 |
| struct rprocpub | 结构 | rs.h:165-183 | §1.5/§2.7 |
| SF_VM_ROLLBACK / SF_VM_NOMMAP | 宏 | rs.h:198-199 | §2.1 |
| VM_RS_SET_PRIV/UPDATE/MEMCTL/PREPARE | 宏 | com.h:724/:736/:738/:766 | §2.1 |
| VM_RS_MEM_*（5 个） | 宏 | com.h:741-745 | §2.1 |

`swap_proc_slot`（utility.c:188-219）/`swap_proc_dyn_data`（utility.c:312-359）/`map_proc_dyn_data`（utility.c:282-307）机制本体归 10 篇，本篇仅消费其语义。

---

## 3. Rust 设计决策

### 3.1 模块组织：一服务一文件 + 三处接线

4 个 RS 服务共享 `rs.rs`（都围绕 live update 一个核心场景）；类型与握手状态分属三处：

```
os/servers/vm/src/
├── rs.rs            # 4 个 handler + RsError + RsMemctlRequest/Result + RsUpdateFlags/Result
├── vm_server.rs     # RprocTab/RprocEntry + rs_handshake + RsMemctlAddrLen 回复编码
└── ipc/dispatcher.rs # dispatch_rs_* 4 个 + CALLMAP 解码 + From<RsError> for VmError
```

### 3.2 错误处理：统一 RsError（D1）

C 的 4 个 handler 各自散落 errno；Rust 用 `RsError`（rs.rs:34-55）统一，errno 映射集中在 `From<RsError> for VmError`（dispatcher.rs:1284-1306）→ `VmError::to_errno()`（minix-types 单一来源）。errno 契约表（对照 C ground truth）：

| RsError | C 条件 | C errno | VmError |
|---------|--------|---------|---------|
| ProcessNotFound | vm_isokendpt 失败（4 个 handler） | EINVAL | InvalidProcess |
| SysProcNoMask | SET_PRIV sys proc 无 mask（rs.c:56-58） | EINVAL | InvalidProcess |
| PinFailed | map_pin_memory 失败（C panic，fail-closed） | （C 无） | NotImplemented |
| HeapExtendFailed | real_brk 失败（break.c:63-68） | ENOMEM | OutOfMemory |
| PreallocMapConflict | UPDATE 目标带 PREALLOC_MAP 且非回滚/非 NOMMAP（rs.c:175-180） | ENOSYS | NotImplemented |
| UpdateNotImplemented | UPDATE 切换 DEFERRED（A-8） | — | NotImplemented |
| InvalidRequest | MEMCTL 未知子请求（rs.c:386-388） | EINVAL | InvalidParam |
| MakeVmFailed | MAKE_VM 不支持（C：2 实例时 EPERM rs.c:231-234） | EPERM | PermissionDenied |
| HeapPreallocFailed | HEAP_PREALLOC 失败（C：real_brk 内部 errno） | ENOMEM* | NotImplemented |
| MapPreallocFailed | MAP_PREALLOC 失败（C：ENOMEM rs.c:319） | ENOMEM* | NotImplemented |
| InvalidLength | `*len <= 0`（rs.c:287-288/:307-308） | EINVAL | InvalidParam |

*注：HeapPreallocFailed/MapPreallocFailed 折叠了底层 brk/mmap 错误，标 ENOSYS（fail-closed）——C 的 real_brk 返回 ENOMEM；RS 侧不区分底层失败原因（LU 窗口本身被 A-8 门控），brk 的精确 ENOMEM 映射保留在 HeapExtendFailed。

**设计理由**：与 19/20/21/22 篇的 `BrkError`/`MmapError`/`MunmapError`/`VmExitError` 同构——每个服务模块一个错误类型，dispatch 层折叠进 `VmError`。RS 服务的特殊性：`EndpointError` 的两个变体（InvalidSlot/DeadEndpoint）折叠为同一个 `ProcessNotFound`（C 的 `vm_isokendpt` 失败统一 EINVAL，rs.c:42-45 等处）。

### 3.3 子请求与结果类型化（D2）

C 用裸整数 `VM_RS_CTL_REQ` + switch；Rust 用 `RsMemctlRequest` enum（rs.rs:72-79）：

```rust
pub(crate) enum RsMemctlRequest {
    Pin,
    MakeVmInstance,
    HeapPrealloc { addr: VirBytes, len: usize },
    MapPrealloc  { addr: VirBytes, len: usize },
    GetPreallocMap,
}
```

`addr` 字段保留在 HeapPrealloc/MapPrealloc 变体中：C 的消息面有 `VM_RS_CTL_ADDR` 输入槽（handler 只写不读），保留字段使解码面与 C 消息面一一对应；handler 用 `{ len, .. }` 忽略。dispatcher 解码（dispatcher.rs:895-915 `decode_rs_memctl_request`，MEMCTL 臂 :1091-1102 调用）把 req_code 0-4 映射为 enum，未知码 → `VmError::InvalidParam`（EINVAL，C default 分支）。结果侧 `RsMemctlResult::Ok / AddrLen { addr, len }`（rs.rs:81-84）对应 C 的 `return OK`（PIN）与 `*addr/*len` 写回（PREALLOC/GET_PREALLOC）。

### 3.4 SUSPEND 语义：RsUpdateResult（D3）

C 用魔法返回值 `SUSPEND`（=-998）约定"别自动回复"；Rust 用结果类型表达：

```rust
pub(crate) enum RsUpdateResult { Ok, Suspend }
```

dispatcher（dispatcher.rs:866-872）：`Ok(Suspend)` → `VmReply::Suspend` → 主循环 `DispatchAction::Suspend`（15 篇 §1.4）。**为什么独立结果类型**：`Result<RsUpdateResult, RsError>` 让"成功但别自动回复"与"成功并回复"在类型层面区分——非法状态不可表示。

### 3.5 SET_PRIV 的 Option<AclMask> 建模（D4）

C 的 `call_mask_p`（NULL 或数组指针）+ `VM_RS_SYS` 两个输入；Rust 用 `Option<AclMask>`：

```rust
pub(crate) fn handle_rs_set_priv(
    table: &VmProcTable,
    _caller: Endpoint,
    target: Endpoint,
    mask: Option<AclMask>,
    is_sys_proc: bool,
) -> Result<(), RsError>
```

- `Some(mask)` → 显式位图（C：`VM_RS_BUF` 非零 + sys_datacopy）；
- `None` + `is_sys_proc` → `SysProcNoMask`（EINVAL，C "sys procs don't share!"）；
- `None` + 非 sys → `AclState::acl_set(false, None)` 默认权限。

**缺口**：位图经 `sys_datacopy` 传输依赖内核 safecopy 机制（与 `ipc_call_rs_init` 同源 DEFERRED）。当前 dispatcher 解码恒传 `mask = None`（dispatcher.rs:1069-1072）——user proc 得到默认 ACL，sys proc fail-closed（EINVAL）。A-8 契约登记（§4.8）。

### 3.6 PREPARE 分步实现契约（D5）

C 的 5 步在 Rust 中分步落地（rs.rs:168-248）：

| 步骤 | C | Rust | 状态 |
|------|---|------|------|
| 1 | vm_isokendpt(src/dst) | `table.vm_isokendpt` | ✅ |
| 2 | map_pin_memory(src) | `crate::region::map_pin_memory(regions_mut(), frames, page_alloc)` | ✅ |
| 3 | src 堆 > dst 堆 → real_brk(dst, src_end) | `find_less(MMAP_BASE)` 取数据区末端，`src_end > dst_end` 才 `handle_brk` | ✅ |
| 4 | map_pin_memory(dst) | 同 2 | ✅ |
| 5 | !NOMMAP → map_proc_dyn_data(src, dst) | **DEFERRED**（A-8） | ❌ |

**步骤 3 的实现细节**：C 用 `region_search(VM_MMAPBASE, AVL_LESS)` 取"MMAPBASE 以下最大区域"的末端作为当前堆顶；Rust 用 `regions().find_less(VirBytes(MMAP_BASE))`（region_map.rs:99）。`src_end > dst_end` 条件**必须保留**——无条件调用 `handle_brk` 会把更大的 dst 堆收缩到 src 的大小（C 只扩不缩）。`handle_brk` 内部以 `vm_region_top` 为当前堆顶（19 篇 §2），与"数据区末端"在 brk 不变量下相等（brk 每次增长/收缩同步更新 region_top 与数据区长度）。

**步骤 5 为什么 DEFERRED**：`map_proc_dyn_data` 需要"范围受限的 CoW 区域复制 + 活动进程页表同步"——`fork_region`（fork.rs:92，18 篇 §2.2）是整地址空间复制且目标是**新建**进程（页表未绑定，PTE 写入无 TLB 负担）；LU 的目标是**已运行**进程，向已绑定页表写 PTE 需要 TLB 处理与内核协作。这与 UPDATE 的 `sys_update` 同属 A-8（内核 IPC/多组件 LU 未落地）。

**设计立场**：分步实现中每一步单独正确；由于 UPDATE fail-closed（`UpdateNotImplemented`，ENOSYS），部分成功的 PREPARE 不可能导致错误的 LU 切换——RS 的 LU 流程会在 UPDATE 处停止。

### 3.7 UPDATE 验证面完成 + 切换 DEFERRED（D6）

C 的 7 步中 Rust 实现 1-2（rs.rs:285-325），3-7 DEFERRED：

| 步骤 | C | Rust | 状态 |
|------|---|------|------|
| 1 | 验证端点 | `table.vm_isokendpt` | ✅ |
| 2 | 非回滚非 NOMMAP + 目标带 PREALLOC_MAP → ENOSYS | `RsUpdateFlags::from_bits_truncate` + 区域扫描 | ✅ |
| 3 | sys_update（内核切 endpoint） | **DEFERRED**（依赖 IpcTransport/`SYS_UPDATE`） | ❌ |
| 4 | swap_proc_slot | **DEFERRED**（typestate 视图存在 vmproc_handle.rs:629，缺 LU 编排） | ❌ |
| 5 | swap_proc_dyn_data | **DEFERRED**（10 篇机制未落地） | ❌ |
| 6 | pt_bind | **DEFERRED** | ❌ |
| 7 | 手动回复 + SUSPEND | **DEFERRED**（依赖真实 IPC 发送面） | ❌ |

**设计理由**：第 1-2 步是纯验证（无副作用），提前实现让调用方在真正的 LU 落地前就能得到正确的 EINVAL/ENOSYS 反馈；第 3-7 步整体是 A-8 缺口（§4.8）。

### 3.8 MEMCTL 委托 brk/mmap（D7）

| 子请求 | C | Rust | 状态 |
|--------|---|------|------|
| PIN | num_vm_instances<=1 → OK；否则 map_pin_memory | `crate::global::vm_instance_count() <= 1 → Ok`；否则 `map_pin_memory` | ✅ |
| MAKE_VM | rs_memctl_make_vm_instance | DEFERRED → `MakeVmFailed`（EPERM，C 2 实例时同码） | ❌ |
| HEAP_PREALLOC | real_brk | `region_top()` + `checked_add` + `handle_brk` | ✅ |
| MAP_PREALLOC | map_page_region(..., MF_PREALLOC) | 委托 `handle_mmap`（RS 第三方映射） | ✅（立即分配缺口 B1） |
| GET_PREALLOC_MAP | map_region_lookup_type | 区域扫描找 `PREALLOC_MAP` | ✅ |

**HEAP_PREALLOC**（rs.rs:369-394）：`region_top()` 取当前堆顶（= C 的 data 区末端）；`checked_add` 是 **tightening**——C 的 `bytes = *addr + *len` 在溢出时静默回绕，会把堆 shrink 成小值，Rust fail-closed（EINVAL）；委托 `handle_brk`（19 篇 §2.3）。

**MAP_PREALLOC 的 RS 第三方映射建模**（rs.rs:395-442）：C 的 `map_page_region` 是内部调用，没有 `do_mmap` 的 execpriv 门控；Rust 的 `handle_mmap` 把 `MAP_UNINITIALIZED` 门控在 execpriv（VFS/RS，mmap.rs:313-315）。而 `do_rs_memctl` 语义上**永远由 RS 发起**——所以 Rust 把请求建模为 **RS 对目标的第三方映射**：`caller = Endpoint::RS`、`forwhom = target`、`flags = PRIVATE|ANONYMOUS|PREALLOC|UNINITIALIZED|THIRDPARTY`。这精确表达了 C 的特权模型（RS 可以替其他进程做特殊 mmap），且 `handle_mmap` 内部会重新 `vm_isokendpt(target)` 验证。

**未建模分支**：C 的 `is_vm` 分支（目标为 VM 自身时用 `VM_OWN_MMAPBASE`/`VM_OWN_MMAPTOP`，rs.c:312-314）在 minix-rs 未区分——仅当 MEMCTL 目标是 VM 自身时可达，而 VM 自更新属 A-8（MAKE_VM DEFERRED，不存在第二个 VM 实例），实际不可达；若未来落地 VM 自更新需补该分支。

**GET_PREALLOC_MAP**（rs.rs:443-457）：遍历 regions 找 `VrFlags::PREALLOC_MAP`（C `map_region_lookup_type` 等义），无 → `addr=0/len=0`。

### 3.9 RprocTab（D8）

```rust
struct RprocEntry { in_use: bool, endpoint: Endpoint, call_mask: u32, is_user: bool }
struct RprocTab { entries: [RprocEntry; 32] }
```

- **32 槽**：握手 stub 的占位容量——与 C 的 `rprocpub[NR_SYS_PROCS]=64`（main.c:63，sys_config.h:9）及 minix-rs 的 `NR_PROCS=256`（minix-types types/com.rs:36，`VM_PROC_COUNT=NR_PROCS+1=257`，vmproc/table.rs:41）均不同源；常量表只用于握手 stub 的条目形态，真实解码落地时以 minix-rs 的进程表容量为准（差异诚实标注）。
- `is_user` 字段：C 的判定是 `IS_RPUB_BOOT_USR`（endpoint==INIT_PROC_NR）；Rust 由未来握手解码填充（当前 stub 恒 false）。
- `rs_handshake`（vm_server.rs:659-681）复刻 `sef_cb_init_fresh` 两步：`ipc_call_rs_init()` 取表 → 逐条 `acl_set`。`ipc_call_rs_init`（:946-980）当前返回 `RprocTab::empty()`——真实 IPC 依赖 IpcTransport/safecopy（DEFERRED，A-8）。

### 3.10 差异清单

| # | C | Rust | 性质 |
|---|----|------|------|
| 1 | `SUSPEND` 魔法返回值 | `RsUpdateResult::Suspend` 类型 | 类型安全（rewrite） |
| 2 | 子请求裸整数 + switch | `RsMemctlRequest` enum | 类型安全（rewrite） |
| 3 | `call_mask` 指针（NULL=默认） | `Option<AclMask>` | 类型安全（rewrite） |
| 4 | errno 散落各函数 | `RsError` + `From` 集中映射 | 结构简化 |
| 5 | `vr->parent` 裸指针 + adjust_proc_refs | `parent_slot: Option<UserSlot>`（10 篇 §3） | 结构简化（类型消除） |
| 6 | C 布局：addr→m2_p1、len→m2_i3（com.h:746-747） | minix-rs 模型内回复编码 m1p1/m1i3（=m2l1/m2i3 同槽，请求解码同槽回读） | 模型内等价（25-R2 修正；C wire 偏移差异属 minix-types 消息模型，A-8） |
| 7 | MAP_PREALLOC 经内部 map_page_region | 经 handle_mmap + RS 第三方映射 | 特权模型等价（25-R1 修正） |
| 8 | `_MINIX_MAGIC` 下 VM_MMAPBASE=MMAPTOP/2 | 固定 `MMAP_BASE = 0x1_0000_0000`（ARCH A-6） | 架构演进 |
| 9 | RS_PREPARE 第 5 步 / RS_UPDATE 切换 | DEFERRED（fail-closed） | **A-8 缺口** |
| 10 | MAKE_VM 多实例支持 | DEFERRED（EPERM） | **A-8 缺口** |

---

## 4. 实现详解

### 4.1 模块结构与数据流

```
RS（m_source）→ 主循环 dispatch_on_msg（vm_server.rs:565）
  ├─ CALLMAP 解码（dispatcher.rs:1062-1103）：按请求码拆字段 → RsMemctlRequest / 直接参数
  ├─ dispatch_rs_set_priv/prepare/update/memctl（dispatcher.rs:829-892）
  └─ handle_rs_*（rs.rs:122/168/285/339）→ VmReply → 回复编码（vm_server.rs:1100+）

握手（启动期）：RS_INIT（主循环优先级 2，vm_server.rs:582-586）
  └─ rs_handshake（vm_server.rs:659）→ ipc_call_rs_init（:946，stub）→ RprocTab → 逐条 acl_set
```

### 4.2 handle_rs_set_priv（rs.rs:122-142）

```rust
let slot = table.vm_isokendpt(target)?;              // C: rs.c:42-45 EINVAL
let mut active = table.get_active(slot)
    .ok_or(RsError::ProcessNotFound)?;
if mask.is_none() && is_sys_proc {
    return Err(RsError::SysProcNoMask);              // C: rs.c:56-58 "sys procs don't share!"
}
let acl = AclState::acl_set(is_sys_proc, mask);      // C: rs.c:63 acl_set
active.set_acl(acl);
Ok(())
```

调用者验证：`_caller` 参数当前未使用——C 中 `do_rs_set_priv` 经 CALLMAP 分发前已被 `acl_check` 把关（15 篇 §2.5）；Rust 同样在 `dispatch_on_msg` 的优先级 4 统一 `acl_check`。参数保留供未来"仅 RS 可调"的显式门控（当前仅 RS 拥有 SET_PRIV 权限位，ACL 已隐式保证）。

### 4.3 handle_rs_prepare（rs.rs:168-248）

```
1. 验证 src/dst endpoint（Endpoint::NONE → InvalidRequest；vm_isokendpt → EINVAL）
2. map_pin_memory(src)        —— 收集区域 (vaddr, length) 快照后逐区域 handle_memory_once(wrflag=true)
3. src_data_end = find_less(MMAP_BASE) 区域末端
   dst_data_end = find_less(MMAP_BASE) 区域末端
   if src_data_end > dst_data_end → handle_brk(dst, src_data_end)   // C: real_brk
4. map_pin_memory(dst)
5. (DEFERRED) map_proc_dyn_data(src, dst) —— A-8
```

`map_pin_memory` 的"先收集快照再处理"设计（region/mod.rs:128-153）：`handle_memory_once` 需要 `&mut RegionMap`，不能持有迭代器同时修改——与 fork.rs 的 `parent.regions().iter().collect()` 同构（18 篇 §4.3）。

### 4.4 handle_rs_update（rs.rs:285-325）

```
1. 验证 src/dst endpoint
2. flags = RsUpdateFlags::from_bits_truncate(flags)
   if !ROLLBACK && !NOMMAP:
       目标区域含 PREALLOC_MAP → PreallocMapConflict（ENOSYS）
3-7. (DEFERRED) sys_update + swap_proc_slot + swap_proc_dyn_data + pt_bind + 手动回复 —— A-8
   → Err(RsError::UpdateNotImplemented)  // fail-closed
```

### 4.5 handle_rs_memctl（rs.rs:339-459）

```
slot = vm_isokendpt(target)                       // C: rs.c:359-362 EINVAL
match request:
  Pin:            vm_instance_count() <= 1 → Ok；否则 map_pin_memory
  MakeVmInstance: Err(MakeVmFailed)               // A-8（C: 2 实例时 EPERM）
  HeapPrealloc:   len==0 → InvalidLength；region_top() + checked_add + handle_brk
                  → AddrLen { addr: 原堆顶, len: 输入 len }
  MapPrealloc:    len==0 → InvalidLength；页对齐；handle_mmap(RS 第三方映射)
                  → AddrLen { addr: 映射基址, len: 对齐后 len }
  GetPreallocMap: 区域扫描 PREALLOC_MAP → AddrLen / (0, 0)
```

### 4.6 RprocTab 与 rs_handshake（vm_server.rs:659-681、946-980）

```rust
fn rs_handshake(&mut self) -> Result<(), VmError> {
    let rproctab = ipc_call_rs_init().map_err(|_| VmError::InternalError)?;  // C: sys_safecopyfrom
    for entry in rproctab.iter() {
        if !entry.in_use { continue; }
        let slot = table.vm_isokendpt(entry.endpoint).map_err(|_| VmError::InvalidProcess)?;
        let mask = Some(AclMask::from_bits_truncate(entry.call_mask as u64));
        proc.set_acl(AclState::acl_set(!entry.is_user, mask));   // C: map_service → acl_set
    }
    Ok(())
}
```

`ipc_call_rs_init` 的 DEFERRED 依赖（注释列三缺一）：IpcTransport（ipc/transport.rs:150/:164 仍 `unimplemented!()`）、sys_safecopyfrom 内核调用、Endpoint↔ProcNr 转换。当前返回 `RprocTab::empty()` 让 `rs_handshake` 不 panic、VM 继续服务 PM/SYS 请求。

### 4.7 修复记录（本轮写作 + 实现）

| # | 级别 | 位置 | 修复 |
|---|------|------|------|
| 25-R1 | P0-code-bug | rs.rs MapPrealloc | `flags: 0x1002`（PRIVATE\|ANONYMOUS）漏掉 PREALLOC(0x080000)/UNINITIALIZED(0x040000)/THIRDPARTY(0x800000)——region 不带 `PREALLOC_MAP`，GET_PREALLOC_MAP 永远查不到；改为命名常量位组合 + caller=RS 第三方映射 |
| 25-R2 | P0-code-bug | vm_server.rs 回复编码 | `RsMemctlAddrLen` 把 len 写 `m1i1`（=m2_i1，VM_RS_CTL_ENDPT 槽）→ 改为 `m1i3`（=m2_i3，VM_RS_CTL_LEN 槽）；RS 侧 `vm_memctl` 读回 len 才正确 |
| 25-R3 | P0-code-bug | dispatcher.rs From<RsError> | `InvalidRequest`/`InvalidLength` 原映射 EFAULT → 修正 EINVAL（C rs.c:386/:285/:305） |
| 25-R4 | 实现补全 | rs.rs PREPARE | 第 3 步（堆对齐）从 DEFERRED 落地：`find_less(MMAP_BASE)` + `src_end > dst_end` 守卫 + `handle_brk` |
| 25-R5 | 实现补全 | rs.rs MEMCTL(PIN) | 从"恒 OK"改为 `vm_instance_count() <= 1 → Ok`、否则 `map_pin_memory`（C rs.c:368-371 等价） |
| 25-R6 | tightening | rs.rs HEAP_PREALLOC | `checked_add` 防溢出（C 静默回绕会 shrink 堆）→ fail-closed EINVAL |

### 4.8 A-8 缺口契约（SEF / Live Update）

plan.md §4 A-8 行：`rs.c` 全量实现 vs minix-rs `RS_PREPARE`/`RS_UPDATE` 未实现（NotImplemented，fail-closed）。本篇范围内未落地清单：

| 缺口 | C 语义 | minix-rs 现状 | 依赖 |
|------|--------|--------------|------|
| PREPARE 第 5 步 `map_proc_dyn_data` | CoW 迁移源进程 mmap 区域到目标 | DEFERRED（PREPARE 仍返回 Ok，步骤 1-4 已实现） | 范围受限 CoW 复制 + 活动进程页表同步（fork_region 是整空间 + 新进程路径） |
| UPDATE 切换（第 3-7 步） | sys_update + swap + pt_bind + 手动回复 | DEFERRED（验证面已实现，返回 ENOSYS） | IpcTransport / 内核 `SYS_UPDATE` / swap_proc_dyn_data 机制（10 篇未落地） |
| MAKE_VM 多 VM 实例 | rs_memctl_make_vm_instance（pt_ptmap/自映射） | DEFERRED（EPERM fail-closed） | pt_ptmap / VM 自映射机制（07 篇） |
| SET_PRIV 位图传输 | sys_datacopy 复制 call_mask | dispatcher 恒传 `mask=None`（user→默认 ACL，sys→EINVAL） | 内核 safecopy 机制 |
| RS_INIT 握手取表 | sys_safecopyfrom rproctab | `ipc_call_rs_init` 返回空表 | IpcTransport / safecopy / Endpoint↔ProcNr |
| MAP_PREALLOC 立即分配 | MF_PREALLOC → map_handle_memory 立即分配全部页 + 清 UNINITIALIZED | 仅标 `PREALLOC_MAP`，页仍惰性分配 | doc 20 backlog B1（06/13 范围） |

**fail-closed 保证**：任何未落地路径都返回显式错误（ENOSYS/EPERM/EINVAL），绝不静默成功——RS 的 LU 流程必然在 UPDATE 处停止，不会进入错误的切换状态。

### 4.9 与相邻文档的关系

- **10-vm-relocation**：`swap_proc_slot`/`swap_proc_dyn_data`/`map_proc_dyn_data` 机制本体；本篇 UPDATE/PREPARE 消费其语义。
- **15-ipc-dispatch**：CALLMAP 4 分支 + SUSPEND 框架 + RS_INIT 优先级 2；`rs_handshake` 的分发入口。
- **19-vm-brk**：`handle_brk`（PREPARE 步骤 3 / HEAP_PREALLOC 落点）。
- **20-vm-mmap**：`handle_mmap`（MAP_PREALLOC 落点）+ `MMAP_BASE/MMAP_TOP`（A-6）。
- **04-acl**：`acl_set`（SET_PRIV / map_service 落点）。
- **22-vm-exit**：LU 后旧实例退出路径（`do_procctl`/`free_proc`）。
- **01-vm-init-main**：SEF 生命周期 + `map_service` 启动注册的完整叙事。

---

## 5. 测试要点

### 5.1 测试覆盖矩阵（`os/servers/vm/src/rs.rs` tests 模块，18 个）

| 测试函数 | 验证 | C 对照 |
|----------|------|--------|
| `test_set_priv_sys_proc_no_mask` | sys proc 无位图 → SysProcNoMask | rs.c:56-58 EINVAL |
| `test_set_priv_user_proc_not_found` | 无效 endpoint → ProcessNotFound | rs.c:42-45 EINVAL |
| `test_set_priv_updates_acl` | 正常设置 → ACL 落库（`active.acl()` 断言） | rs.c:63 acl_set |
| `test_set_priv_sys_proc_with_mask_ok` | sys proc 带位图 → Ok | rs.c:48-53 |
| `test_memctl_pin_not_found` | PIN 无效 endpoint → ProcessNotFound | rs.c:359-362 |
| `test_memctl_pin_single_instance_ok` | 单 VM 实例 → Ok（不钉住） | rs.c:368-371 |
| `test_memctl_make_vm_not_found` | MAKE_VM 无效 endpoint → ProcessNotFound | rs.c:359-362 |
| `test_memctl_heap_prealloc_zero_len` | len=0 → InvalidLength | rs.c:287-288 EINVAL |
| `test_memctl_heap_prealloc_grows_heap` | 堆预分配 → region_top 增长 + AddrLen 回写 | rs.c:281-295 |
| `test_memctl_map_prealloc_zero_len` | len=0 → InvalidLength | rs.c:307-308 EINVAL |
| `test_memctl_map_prealloc_sets_prealloc_flag` | MAP_PREALLOC → 区域带 PREALLOC_MAP + GET_PREALLOC_MAP 命中（25-R1 回归） | rs.c:300-344 |
| `test_prepare_invalid_endpoint` | 无效 dst → ProcessNotFound | rs.c:92-95 EINVAL |
| `test_prepare_extends_dst_heap_to_src` | src 堆大 → dst 堆扩到 src 末端（25-R4 回归） | rs.c:116-126 |
| `test_prepare_does_not_shrink_dst_heap` | dst 堆大 → 不动（守卫条件） | rs.c:125 |
| `test_update_invalid_endpoint` | 无效 dst → ProcessNotFound | rs.c:163-166 |
| `test_update_flags_rollback_bypasses_prealloc_check` | ROLLBACK 跳过 PREALLOC 检查 | rs.c:175-180 |
| `test_update_flags_nommap_bypasses_prealloc_check` | NOMMAP 跳过 PREALLOC 检查 | rs.c:175-180 |
| `test_error_errno_mapping` | RsError → VmError → errno 全表（含 25-R3 修正断言） | rs.c 各 errno |

**验证命令**：

```bash
cargo test -p minix-vm --lib rs::   # 18 passed / 0 failed
```

### 5.2 测试统计（截至 2026-08-16）

- `cargo test -p minix-vm --lib`：**419 passed / 1 failed**（`region::vir_region::tests::test_map_lazy` 为 13 篇范围 pre-existing；基线 407/1 → 416/1（首轮：7 个 rs 测试 + 2 个 MEMCTL 解码回归测试（25-P0-1））→ 419/1（卓越性轮 +3 个 vm_server.rs RS 回归测试：`test_encode_reply_rs_memctl_addr_len_slots`（25-R2 编码回归）+ `test_rproctab_empty_32_slots_all_not_in_use` / `test_rproctab_empty_entry_defaults`（D8 stub 语义））
- 本节列出与 RS 服务直接相关的测试：18 个 rs.rs tests 模块测试（下表）+ 3 个 vm_server.rs tests 模块回归测试（25-R2 编码 / RprocTab stub）
- 完整测试清单：`rg "^\s*fn test_" os/servers/vm/src/rs.rs`

---

## 6. 过渡

### 6.1 位置可回答性

**init_vm 阶段**：本篇无显式初始化——RS 服务是**主循环分发**的服务面；启动期的 RS_INIT 握手（`rs_handshake`）是 15 篇优先级 2 的入口，其表数据（RprocTab）在此定义。

**主循环阶段**（main.c:112-194）：

```
主循环
  ├─ 优先级 2：RS_INIT → rs_handshake（RprocTab → acl_set）   ← 15 篇 + 本篇 §4.6
  ├─ 优先级 4（CALLMAP）：
  │    ├─ VM_RS_SET_PRIV → dispatch_rs_set_priv               ← 本篇
  │    ├─ VM_RS_PREPARE  → dispatch_rs_prepare                ← 本篇
  │    ├─ VM_RS_UPDATE   → dispatch_rs_update                 ← 本篇（A-8：切换 DEFERRED）
  │    └─ VM_RS_MEMCTL   → dispatch_rs_memctl                 ← 本篇
  └─ SUSPEND 抑制自动回复（UPDATE 语义）                        ← 15 篇 §1.4
```

**与相邻文档**：本篇是 15 篇"4 个 RS CALLMAP 分支"的服务体落地；PREPARE 的堆扩展消费 19 篇 brk、mmap 预分配消费 20 篇 mmap；UPDATE 的交换机制依赖 10 篇（未落地则 fail-closed）。

### 6.2 下游移交（对照 plan.md §3.4 第 25 行）

- **前置依赖**: 15（主循环分发）/22（exit/procctl，LU 后旧实例退出）
- **本篇职责**: `do_rs_set_priv/prepare/update/memctl`、`rs_memctl_*`、`map_service`、`adjust_proc_refs`、`RprocTab`、A-8 缺口契约
- **不覆盖（移交）**: 查询（26）；swap_proc_*/map_proc_dyn_data 机制（10）；ACL 本体（04）；SEF 生命周期（01）；分发框架（15）
- **下一篇**：`26-vm-queries`（INFO/GETPHYS/GETREF/GETRUSAGE/region_info/usage——查询服务面）

---

## 7. 参见

| 文档 | 关系 |
|------|------|
| `01-vm-init-main.md` | SEF 生命周期 + `map_service` 启动注册叙事（rs_handshake 的 C 源语义） |
| `03-vmproc-table.md` | `vm_isokendpt` / slot 验证（4 个 handler 共同入口） |
| `04-acl.md` | `acl_set` 位图语义（SET_PRIV / map_service 落点） |
| `10-vm-relocation.md` | `swap_proc_slot`/`swap_proc_dyn_data`/`map_proc_dyn_data` 机制本体（UPDATE/PREPARE 消费） |
| `15-ipc-dispatch.md` | CALLMAP 4 分支 + SUSPEND 框架 + RS_INIT 优先级 2 |
| `19-vm-brk.md` | `handle_brk`（PREPARE 步骤 3 / HEAP_PREALLOC 落点） |
| `20-vm-mmap.md` | `handle_mmap` + `MMAP_BASE/MMAP_TOP`（A-6）+ MAP_PREALLOC 立即分配缺口（backlog B1） |
| `22-vm-exit.md` | LU 后旧实例退出路径（`do_procctl`/`free_proc`） |
| `26-vm-queries.md` | 下一篇：查询服务（INFO/GETPHYS/GETREF/GETRUSAGE） |
| `99-global-concepts.md` | 常量表（VM_RQ_BASE / VM_RS_* / SF_VM_*） |
