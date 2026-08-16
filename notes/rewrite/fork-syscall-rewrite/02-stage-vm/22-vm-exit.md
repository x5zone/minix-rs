# 22-vm-exit: VM_EXIT / WILLEXIT / PROCCTL —— 进程退出与进程控制

> **分类**: 阶段 7 — IPC 服务（进程生命周期）
> **源码**: `minix3/minix/servers/vm/exit.c`（全 156 行：`reset_vm_rusage` :25-31 / `free_proc` :33-43 / `clear_proc` :45-58 / `do_exit` :60-92 / `do_willexit` :100-114 / `do_procctl` :117-156）+ `region.c`（`map_subfree` :527-565 / `map_free` :568-585 / `map_free_proc` :589-612）+ `pagetable.c`（`pt_new` :990-1026 / `pt_bind` :1358-1422 / `pt_free` :1427-1437）+ `pb.c`（`pb_unreferenced` :96-133）+ `pagefaults.c`（`handle_memory_once` :245-252 / `handle_memory_start` :254-289）+ `main.c`（VFS transid 路由 :131-148 / `do_procctl_notrans` :419-426 / CALLMAP :543/:546/:548）+ `acl.c`（`acl_clear` :121-130）+ `glo.h`（`num_vm_instances` :46）+ `minix3/minix/include/minix/com.h`（`VM_EXIT` :630 / `VME_ENDPOINT` :631 / `VM_WILLEXIT` :643 / `VMWE_ENDPOINT` :644 / `VM_PROCCTL` :752 / `VMPCTL_*` :753-757 / `VMPPARAM_*` :759-760）+ `minix3/minix/include/minix/ipc.h`（`mess_9` :77-83）+ `minix3/minix/lib/libsys/vm_exit.c` + `minix3/minix/lib/libsys/vm_procctl.c` + `minix3/minix/servers/vfs/comm.c`（`vm_vfs_procctl_handlemem` :198-217）+ `minix3/minix/servers/pm/forkexit.c`（`exit_proc` :332 / `exit_restart` :455）
> **Rust 模块**: `os/servers/vm/src/exit.rs`（`VmExitError` :22 / `handle_vm_exit` :42 / `handle_vm_willexit` :66 / `free_process_phys` :98 / `handle_procctl_clear` :163 / `handle_procctl_handlemem` :215 / `VmProcctlHandlememResult` :281 / `VmProcctlError` :291 / `From<VmProcctlError>` :303 / tests :361-506）+ `os/servers/vm/src/vmproc/vmproc_handle.rs`（`mark_exiting` :171 / `reset_rusage` :469 / `regions_mut`（ActiveProc）:478 / `regions_mut`（ExitingProc）:753 / `reap` :768）+ `os/servers/vm/src/vmproc/vmproc.rs`（`reset_rusage` :122 / `clear` :167）+ `os/servers/vm/src/vmproc/table.rs`（`get_active` :176 / `get_exiting` :192 / `vm_isokendpt` :276）+ `os/servers/vm/src/ipc/dispatcher.rs`（`dispatch_procctl` :219 / `dispatch_exit` :791 / `dispatch_willexit` :804 / VM_PROCCTL 分支 :1181）+ `os/servers/vm/src/vm_server.rs`（`handle_vfs_transid` :688 / `handle_exit` :1221）+ `os/libs/minix-types/src/ipc/vm.rs`（`VmExitIn` :393 / `VmWillexitIn` :404 / `VmProcctlIn` :497 / `decode_message` :856 / `test_vm_procctl_in_decode_message`）+ `os/libs/minix-types/src/ipc/message.rs`（`m_lc_vm_procctl` :147 / `MessLcVmProcctl` :1813）
> **前置**: `03-vmproc-table.md`（endpoint 验证 + typestate 视图）、`13-region-mapping.md`（区域结构与释放）、`15-ipc-dispatch.md`（主循环分发 + SUSPEND + transid 路由）
> **说明**: 本文档管 **进程生命周期的终点**——PM 驱动的两阶段退出协议（`VM_WILLEXIT` 预通知 + `VM_EXIT` 正式退出）如何把地址空间、物理页、页表、ACL 全部归还，兑现 CoW/remap 引用计数的最终承诺；以及 VFS/RS 通过 `VM_PROCCTL` 对运行中进程的内存控制（`VMPPARAM_CLEAR` 清场重建页表、`VMPPARAM_HANDLEMEM` 保证地址可达）。**不覆盖**：查询（do_info/getrusage/get_phys，26）、fdref 表与 VFS 异步续作（23）、页错误/CoW 分裂（16/17）、fork 建进程（18）、brk/mmap 建立与拆除（19/20/21）。

---

## 1. 概念：两阶段退出与进程控制通道

### 1.0 章节引言

**目标读者**：已读完 03（进程表与 endpoint 验证）、13（区域结构）、15（主循环分发）的读者。本文档回答三个问题：进程消亡时 VM 做什么？为什么退出必须是两阶段？VFS/RS 凭什么能控制另一个进程的内存？

**本章不讲什么**：物理页引用计数的内部结构（11-phys-pagestate）、CoW 写保护与页错误分裂（16/17）、fdref 与 VFS 的异步对话（23）、查询接口（26）——这里只讲"释放"这一侧如何消费前面建立的引用计数。

### 1.1 进程消亡的三方协议

进程退出不是 VM 单方面的事，而是 **PM（决定谁死）→ VM（归还资源）→ 物理页（引用计数兑现）** 三方协作：

```
用户调用 exit(status)
  │
  ▼  PM: exit_proc（pm/forkexit.c:332）
  ├─ sys_stop() 停止调度
  ├─ vm_willexit(ep)        ← ① 预通知：VM 冻结进程
  │
  ▼  VM: do_willexit()（exit.c:100）
  └─ 置 VMF_EXITING —— 进程"正在退出"，不再接受新的内存分配
  │
  ▼  PM: exit_restart（pm/forkexit.c:455）
  ├─ sys_clear() 清理内核态
  ├─ vm_exit(ep)            ← ② 正式退出：VM 释放全部资源
  │
  ▼  VM: do_exit()（exit.c:60）
  ├─ 递减 VM_INSTANCE 计数（若退出的是 VM 实例）
  ├─ free_proc()            ← 释放物理页、区域、页表
  └─ clear_proc()           ← 清 ACL、清标志 → 槽位归空
```

**为什么是两阶段**：`VMF_EXITING` 一旦置位，brk/mmap 等分配请求对正在退出的进程必须失效（否则进程一边退出一边申请内存）。C 靠调用方检查标志；Rust 把"正在退出"提升为**类型状态**——`get_active()` 对 EXITING 进程返回 `None`，分配路径在编译期就不可达（§3.1）。

### 1.2 引用计数的最终兑现

进程退出时不能"直接释放所有物理页"——fork/remap 之后父子进程可能**共享**物理页（refcount > 1），必须等最后一个引用者退出。这就是 `pb_reference`/`pb_unreferenced` 记账的终点：

```
独立匿名页   refcount 1 → 0 → ev_unreference → 归还分配器
CoW 共享页   退出一个进程：2 → 1 → 物理页保留（兄弟进程仍映射）
CoW 共享页   两个都退出：2 → 1 → 0 → 最终释放
directphys   1 → 0 → phys_unreference 空操作（设备内存不归 VM 管理）
```

退出路径对每个映射页执行**恰好一次**解除引用；任何漏解除都会泄漏物理页，任何多解除都会破坏共享。Rust 的 PFN 模型把这条链收敛在 `free_process_phys` 一个函数里（§3.2）。

### 1.3 VM_PROCCTL：进程控制通道

`VM_EXIT`/`VM_WILLEXIT` 是 PM 专属；`VM_PROCCTL` 则是 **VFS/RS 对运行中进程**的内存控制通道，由 `do_procctl` 按 `VMPCTL_PARAM` 分派两个子操作：

| 参数 | 调用方 | 语义 | C 行为 |
|------|--------|------|--------|
| `VMPPARAM_CLEAR`（1） | RS 或 VFS | 释放进程全部内存 + 重建全新页表并绑定——进程槽保持 IN_USE（exec 前清场） | `free_proc` → `pt_new` → `pt_bind`（exit.c:130-137） |
| `VMPPARAM_HANDLEMEM`（2） | 仅 VFS | 保证一段地址可访问（exec 参数、路径解析前） | `handle_memory_start(...)` → **无条件 SUSPEND**（exit.c:139-148） |
| 其他 | — | 拒绝 | EINVAL（exit.c:150-151） |

权限语义很关键：**只有 VFS/RS 能替别人清内存**（EPERM 门），普通进程不能。这使 `VM_PROCCTL` 成为系统服务之间"替进程做事"的受信通道。

### 1.4 VFS transid：事务化请求

VFS 的请求可能携带事务 id（`TRNS_GET_ID` 从 `m_type` 低位取 16 位，vfsif.h:79——发送方用 `TRNS_ADD_ID` 把 id 放进低 16 位）；主循环命中 `IS_VFS_FS_TRANSID` 时剥掉 id、路由到 `do_procctl`（main.c:141-148）。这是 VM 侧唯一"服务多个来源、需要区分请求归属"的入口——`VMPPARAM_HANDLEMEM` 的异步续作（SUSPEND → 稍后 VM_VFS_REPLY）正是靠 transid 回连请求的。

### 1.5 对照：Redox 与 Linux

- **Linux**（kernel/exit.c）：进程退出在**内核**完成——`do_exit` → `exit_mm` → `mmput`（地址空间引用计数归零才真正释放）→ `exit_mmap`（逐 VMA unmap + 页引用递减）。两阶段是 **exit/wait 分离 + `mm_users`/`mm_count` 双计数**：`mm_users` 对应"还有多少地址空间在引用这个 mm"，概念上与 Minix3 的 `pb.refcount`（多进程共享物理页）同构。
- **Redox**：`Syscall::exit` 在内核终止进程上下文；地址空间随 `Process` 结构 **drop（RAII）** 释放。与 minix-rs 的显式 `clear()` 哲学不同——VM 进程槽是 `static mut` 复用池，不能依赖 Drop（vmproc.rs Drop 注释），必须显式归位。
- **对照要点**：三家都保证"最后一个引用者释放物理页"；Minix3 的独特之处是释放逻辑在**用户态 VM 服务器**、由 PM 经 IPC 显式驱动、引用计数显式记账（无 RAII 兜底）——这决定了 Rust 侧必须用 typestate + 显式 `reap()` 把"释放一次且仅一次"变成类型约束。

### 1.6 小结

进程退出 = **两阶段协议**（WILLEXIT 冻结 → EXIT 释放）+ **引用计数兑现**（逐页恰好一次解除）+ **结构归位**（区域/页表/ACL/统计全清，槽位归空）。`VM_PROCCTL` 是 VFS/RS 的进程控制通道（CLEAR 清场重建、HANDLEMEM 保证可达），transid 路由是它的 VFS 专用入口。C 用标志位 + 调用方自觉；Rust 用 typestate + 显式生命周期。

---

## 2. C 源码分析

### 2.1 调用路径与消息格式

三个入口的消息格式（全部复用 `m1` 或 `m9` 子格式）：

| 消息 | 值（com.h） | 发送者 | 字段 | C 结构 |
|------|-----------|--------|------|--------|
| `VM_EXIT` | `VM_RQ_BASE+0`（:630） | PM | `VME_ENDPOINT` = `m1_i1`（:631） | libsys/vm_exit.c:15 |
| `VM_WILLEXIT` | `VM_RQ_BASE+5`（:643） | PM | `VMWE_ENDPOINT` = `m1_i1`（:644） | libsys/vm_exit.c:31 |
| `VM_PROCCTL` | `VM_RQ_BASE+45`（:752） | VFS/RS | `VMPCTL_PARAM`=`m9_l1`、`WHO`=`m9_l2`、`M1`=`m9_l3`、`LEN`=`m9_l4`、`FLAGS`=`m9_l5`（:753-757） | libsys/vm_procctl.c / vfs/comm.c:198-217 |

`mess_9` 的 32 位布局（ipc.h:77-83）：两个 `u64` 打头（offset 0/8），五个 `long` 依次在 offset 16/20/24/28/32，四个 `short` 在 36，padding 到 56 字节。`VMPCTL_*` 宏正是这五个 long——**param@16 / who@20 / m1@24 / len@28 / flags@32**。这是 §3.6 wire format 修复的对照基准。

主循环接线（main.c:141-148 + CALLMAP :543/:546/:548）：

```
主循环收到消息
  ├─ VFS + IS_VFS_FS_TRANSID → TRNS_DEL_ID → do_procctl(&msg, transid)
  ├─ CALLMAP(VM_EXIT, do_exit)        → acl_check → do_exit
  ├─ CALLMAP(VM_WILLEXIT, do_willexit) → acl_check → do_willexit
  └─ CALLMAP(VM_PROCCTL, do_procctl_notrans) → acl_check → do_procctl(&msg, 0)
```

`do_procctl_notrans`（main.c:419-426）是 transid 路径之外的普通入口，`transid = 0`。

### 2.2 do_willexit —— 预通知（exit.c:100-114）

```c
int do_willexit(message *msg)
{
    int proc;
    struct vmproc *vmp;

    if(vm_isokendpt(msg->VMWE_ENDPOINT, &proc) != OK) {
        printf("VM: bogus endpoint VM_EXITING %d\n", msg->VMWE_ENDPOINT);
        return EINVAL;
    }
    vmp = &vmproc[proc];
    vmp->vm_flags |= VMF_EXITING;
    return OK;
}
```

极简：只置 `VMF_EXITING`。副作用是深远的——此后所有分配路径（brk/mmap/fork）对该进程失效。endpoint 无效 → EINVAL（不区分 EINVAL/EDEADEPT）。

### 2.3 do_exit —— 正式退出（exit.c:60-92）

```c
int do_exit(message *msg)
{
    int proc;
    struct vmproc *vmp;

    if(vm_isokendpt(msg->VME_ENDPOINT, &proc) != OK) {
        printf("VM: bogus endpoint VM_EXIT %d\n", msg->VME_ENDPOINT);
        return EINVAL;
    }
    vmp = &vmproc[proc];

    if(!(vmp->vm_flags & VMF_EXITING)) {
        printf("VM: unannounced VM_EXIT %d\n", msg->VME_ENDPOINT);
        return EINVAL;                  /* 必须先 WILLEXIT */
    }
    if(vmp->vm_flags & VMF_VM_INSTANCE) {
        vmp->vm_flags &= ~VMF_VM_INSTANCE;
        num_vm_instances--;             /* VM 实例计数递减 */
    }
    free_proc(vmp);                     /* 释放物理页/区域/页表 */
    clear_proc(vmp);                    /* 清 ACL/标志 → 槽位归空 */
    return OK;
}
```

三个要点：
1. **`VMF_EXITING` 门**（:73-75）：未先 WILLEXIT 直接 EXIT → EINVAL"unannounced"。这是两阶段协议的运行时保证。
2. **VM_INSTANCE 计数**（:76-81）：退出的是 VM 实例（重启旧实例）时递减 `num_vm_instances`（glo.h:46），在 free_proc **之前**完成。
3. **两步释放**：`free_proc`（内存资源）→ `clear_proc`（槽位状态）。

### 2.4 free_proc / clear_proc / reset_vm_rusage（exit.c:25-58）

```c
static void reset_vm_rusage(struct vmproc *vmp)      /* :25-31 */
{
    vmp->vm_total = 0;
    vmp->vm_total_max = 0;
    vmp->vm_minor_page_fault = 0;
    vmp->vm_major_page_fault = 0;
}

void free_proc(struct vmproc *vmp)                   /* :33-43 */
{
    map_free_proc(vmp);            /* 1. 释放全部区域（物理页解除引用） */
    pt_free(&vmp->vm_pt);          /* 2. 释放页表页 */
    region_init(&vmp->vm_regions_avl);  /* 3. 重置区域树 */
#if VMSTATS
    vmp->vm_bytecopies = 0;
#endif
    vmp->vm_region_top = 0;        /* 4. 区域顶清零 */
    reset_vm_rusage(vmp);          /* 5. 统计清零 */
}

void clear_proc(struct vmproc *vmp)                  /* :45-58 */
{
    region_init(&vmp->vm_regions_avl);
    acl_clear(vmp);                /* ACL 引用递减 → NO_ACL */
    vmp->vm_flags = 0;             /* 清 INUSE → 槽位空闲 */
#if VMSTATS
    vmp->vm_bytecopies = 0;
#endif
    vmp->vm_region_top = 0;
    reset_vm_rusage(vmp);
}
```

`free_proc` 与 `clear_proc` 职责分离：前者释放**内容**（物理页、区域、页表），后者重置**槽位**（ACL、标志、统计）——`do_exit` 两步都调用；`do_procctl` 的 CLEAR 只调用 `free_proc`（进程还活着，ACL/IN_USE 不能动）。`reset_vm_rusage` 是两者共享的 static 辅助（`VMSTATS` 编译开关控制 `vm_bytecopies`）。

### 2.5 释放链：map_free_proc → map_free → map_subfree → pb_unreferenced

```
map_free_proc(vmp)            region.c:589-612
  └─ while(region_search_root)  取根区域（AVL 树逐个摘除）
       ├─ region_remove(...)
       └─ map_free(r)            region.c:568-585
            ├─ map_subfree(r, 0, r->length)   region.c:527-565
            │    └─ 逐页 pb_unreferenced(region, pr, 1)   pb.c:96-133
            │         ├─ pb->refcount--（USE 宏：SMP 原子）
            │         ├─ 从 pb->firstregion 链表摘除 pr
            │         └─ refcount==0 → ev_unreference(pr)（mem_anon.c:56 等）
            │                          → SLABFREE(pb)
            ├─ if(region->def_memtype->ev_delete) ev_delete(region)
            │                          /* file 区域 → mappedfile_delete → fdref_deref */
            ├─ free(region->physblocks)
            └─ SLABFREE(region)
  └─ region_init(...)
```

关键语义：
- **逐页解除**：`pb_unreferenced` 递减 `pb->refcount`，归零才调 `ev_unreference` 释放物理页——这就是 §1.2 的"恰好一次解除"。
- **区域级 ev_delete**：`map_free` 在页解除后调用 `def_memtype->ev_delete`；file 后备区域的 `mappedfile_delete`（mem_file.c:280-287）会 `fdref_deref`——**退出路径也必须归还 fd 引用**，这是 §3.2 Rust 修复点。
- **顺序**：先解除页引用，再删区域结构——`ev_delete` 可能需要区域的 param（fdref_id），所以放在 `map_subfree` 之后。

### 2.6 pt_free / pt_new / pt_bind（pagetable.c）

- `pt_free(pt)`（:1427-1437）：释放 `pt_pt[i]`（页表页数组），**不**清 PTE——整个页目录随后被丢弃。
- `pt_new(pt)`（:990-1026）：分配新页目录（`vm_allocpages`）、清零、`pt_virtop = 0`、`pt_mapkernel` 映射内核区。**注意**：C 拒绝重分配已存在的页目录（"Don't ever re-allocate/re-move a certain process slot's page directory once it's been created"），所以 CLEAR 必须 `pt_free` 后再 `pt_new`。
- `pt_bind(pt, who)`（:1358-1422）：把页目录物理地址写入内核的 `pagedir_mappings`，并 `sys_vmctl_set_addrspace` 通知内核切换地址空间（:1420-1421）。

`VMPPARAM_CLEAR` 的完整序列 `free_proc → pt_new → pt_bind` 就是"把进程打回刚启动的空地址空间"。

### 2.7 do_procctl：VMPPARAM_CLEAR / VMPPARAM_HANDLEMEM（exit.c:117-156）

```c
int do_procctl(message *msg, int transid)
{
    endpoint_t proc;
    struct vmproc *vmp;

    if(vm_isokendpt(msg->VMPCTL_WHO, &proc) != OK) {
        printf("VM: bogus endpoint VM_PROCCTL %ld\n", msg->VMPCTL_WHO);
        return EINVAL;
    }
    vmp = &vmproc[proc];

    switch(msg->VMPCTL_PARAM) {
        case VMPPARAM_CLEAR:                       /* :130-137 */
            if(msg->m_source != RS_PROC_NR && msg->m_source != VFS_PROC_NR)
                return EPERM;
            free_proc(vmp);
            if(pt_new(&vmp->vm_pt) != OK)
                panic("VMPPARAM_CLEAR: pt_new failed");
            pt_bind(&vmp->vm_pt, vmp);
            return OK;
        case VMPPARAM_HANDLEMEM:                   /* :139-148 */
        {
            if(msg->m_source != VFS_PROC_NR)
                return EPERM;
            handle_memory_start(vmp, msg->VMPCTL_M1,
                msg->VMPCTL_LEN, msg->VMPCTL_FLAGS,
                VFS_PROC_NR, VFS_PROC_NR, transid, 1);
            return SUSPEND;                        /* 无条件 SUSPEND */
        }
        default:
            return EINVAL;
    }
}
```

要点：
- **CLEAR 的权限**：`RS_PROC_NR`（com.h:61，值 2）或 `VFS_PROC_NR`（com.h:60，值 1）——`EPERM` 否则。
- **HANDLEMEM 的权限**：仅 VFS。
- **HANDLEMEM 无条件 SUSPEND**：`handle_memory_start` 的返回值被丢弃，`do_procctl` 恒返回 SUSPEND——主循环不回复，VFS 阻塞等待稍后的 `VM_VFS_REPLY`（页错误续作机制，16 范围）。这是 §3.5 偏差的 C 基准。
- **endpoint 命名陷阱**：C 局部变量叫 `proc` 但存的是 endpoint（`vm_isokendpt` 把 slot 写回同一变量）。Rust 用 `Endpoint`/`UserSlot` 类型区分，消除此类别名混淆。

### 2.8 handle_memory_start（pagefaults.c:254-289）

HANDLEMEM 委托的底层函数：

```c
int handle_memory_start(struct vmproc *vmp, vir_bytes mem, vir_bytes len,
    int wrflag, endpoint_t caller, endpoint_t requestor, int transid, int vfs_avail)
{
    if((o = mem % PAGE_SIZE)) { mem -= o; len += o; }   /* mem 向下对齐 */
    len = roundup(len, PAGE_SIZE);                      /* len 向上圆整 */
    state = { vmp, mem, len, wrflag, ... };
    r = handle_memory_step(&state, FALSE);
    if(r == SUSPEND) { assert(caller != NONE); assert(vfs_avail); }
    else handle_memory_final(&state, r);
    return r;
}
```

语义：把 [mem, mem+len) 对齐到页边界后，逐页 `map_lookup` + wrflag 可写性检查 + CoW 解析（`map_handle_memory`）；文件后备页需要 VFS 提供时返回 SUSPEND（异步续作），否则同步完成并 `handle_memory_final` 回复。`handle_memory_once`（:245-252）是同步变体（断言 `r != SUSPEND`），fork/exec 内部使用。

### 2.9 VFS transid 路由（main.c:131-148）

```c
transid = TRNS_GET_ID(msg.m_type);                    /* m_type 低 16 位 */
if((msg.m_source == VFS_PROC_NR) && IS_VFS_FS_TRANSID(transid)) {
    msg.m_type = TRNS_DEL_ID(msg.m_type);             /* 剥掉 id */
    result = do_procctl(&msg, transid);               /* 只路由 procctl */
}
```

非 VFS 的 procctl 走 `do_procctl_notrans`（transid=0）。transid 进入 `handle_memory_start` 的 state，异步续作时用它匹配 VFS 请求。

### 2.10 C 小结：符号全景

| 符号 | 位置 | 一句话语义 |
|------|------|-----------|
| `reset_vm_rusage` | exit.c:25 | 清零 4 个统计字段 |
| `free_proc` | exit.c:33 | 释放区域+物理页+页表+统计 |
| `clear_proc` | exit.c:45 | 清 ACL+标志 → 槽位归空 |
| `do_exit` | exit.c:60 | EXITING 门 + VM_INSTANCE 递减 + free+clear |
| `do_willexit` | exit.c:100 | 置 EXITING |
| `do_procctl` | exit.c:117 | CLEAR/HANDLEMEM 分派 + 权限门 |
| `map_free_proc/map_free/map_subfree` | region.c:589/568/527 | 区域释放链 |
| `pb_unreferenced` | pb.c:96 | 引用计数归零释放 |
| `pt_free/pt_new/pt_bind` | pagetable.c:1427/990/1358 | 页表生命周期 |
| `handle_memory_start/once` | pagefaults.c:254/245 | 地址可达性保证 |
| `do_procctl_notrans` | main.c:419 | transid=0 的 procctl 包装 |
| `acl_clear` | acl.c:121 | ACL 引用递减 |
| `num_vm_instances` | glo.h:46 | VM 实例计数 |

---

## 3. Rust 设计决策

### 3.1 D1：typestate 表达两阶段协议

**决策**：`ActiveProc --mark_exiting()--> ExitingProc --reap()--> EmptySlot`（vmproc_handle.rs:171/:768）；`handle_vm_exit` 用 `table.get_exiting()`（table.rs:192）取 ExitingProc，`get_exiting` 返回 `None` 即"未先 WILLEXIT"。

**为什么好**：C 的 `VMF_EXITING` 是运行时标志，靠调用方自觉检查（brk/mmap 是否检查取决于实现）；Rust 把"正在退出"变成类型——`get_active()`（table.rs:176）对 EXITING 返回 `None`，brk/mmap/fork 的分配路径在**编译期**无法拿到 EXITING 进程的 `ActiveProc` 视图。§1.1 的"EXITING 冻结"从约定变成类型系统保证。

**边界**：`reap()` 是 `unsafe`（SAFETY：单线程事件循环 + 无并发引用 + 页表已脱离硬件）；`mark_exiting` 之后进程不可逆地走向退出（无回到 Active 的路径——C 也没有）。

### 3.2 D2：资源释放链（free_process_phys + clear）

**决策**：`exit::free_process_phys(regions: &mut RegionMap, frames, page_alloc)`（exit.rs:98）逐区域完成三件事，然后 `reap()` → `VmProc::clear()`（vmproc.rs:167）归位槽位。

```
free_process_phys（exit.rs:98-148）
  对每个区域（iter_mut）：
    ① 捕获 VrParam::File 的 fdref_id（ev_delete 会清掉它）
    ② def_memtype.ev_delete(&mut region)     ← C map_free 的 ev_delete 调用
    ③ 逐 mapped slot：
         ev_unreference(frames, pfn)          ← C pb_unreferenced 的 memtype 回调
         frames.refcount--                     ← C pb->refcount--
         refcount==0 && !IN_CACHE → page_alloc.free_pfn(pfn)   ← C free_mem
    ④ fdref.deref_entry(id) → PendingFdClose 本地暂存（VFS_FDCLOSE 发送 DEFERRED，23 范围）
reap() → clear()（vmproc.rs:167-208）
    regions.clear() + PageTable::destroy()（非 test）+ dec_vm_instance() + ACL 归 Uninitialized
    + flags/endpoint/boot/统计全清零 → EmptySlot
```

**与 C 的对应**：
- `free_proc` 的内容面（map_free_proc + pt_free + 统计）≈ `free_process_phys` + `clear()` 的资源部分；
- `clear_proc` 的槽位面（acl_clear + flags=0）≈ `clear()` 的槽位部分；
- `VM_INSTANCE` 递减：C 在 free_proc 前（exit.c:76-81），Rust 在 `clear()` 内（vmproc.rs）——释放路径不读计数，顺序等价，文档标注。

**ev_unreference 的 PFN 模型说明**：匿名/direct 内存的 `ev_unreference` 是空操作——refcount 递减和物理页归还由调用方（free_process_phys）负责；文件后备区域的 ev_delete 由区域级步骤 ② 负责。这个职责划分与 munmap 路径 `free_region_pages`（region/mod.rs）完全一致，保证**两条释放路径的 fdref 平衡语义相同**。

### 3.3 D3：错误码映射（本轮修复）

C 退出路径的错误码高度统一，Rust 修复前有两处偏差：

| C 场景 | C errno | Rust 修复前 | Rust 修复后 |
|--------|---------|------------|------------|
| do_exit/do_willexit endpoint 无效 | EINVAL（exit.c:69/:108） | `VmExitError::ProcessNotFound → EINVAL` | ✓ 原本正确 |
| 未 willexit 直接 exit | EINVAL（exit.c:74） | `NotExiting → EINVAL` | ✓ 原本正确 |
| procctl endpoint 无效 | EINVAL（exit.c:122-125） | `InvalidEndpoint → ESRCH` ❌ | `InvalidProcess → EINVAL`（exit.rs:306-309） |
| procctl 越权 | EPERM（exit.c:131-132/:141-142） | `PermissionDenied → EPERM` | ✓ 原本正确 |
| procctl 未知参数 | EINVAL（exit.c:150-151） | `InvalidAddress → EFAULT` ❌ | `InvalidParam → EINVAL`（dispatcher.rs:278） |
| procctl who<=0 | EINVAL（exit.c:122-125） | `InvalidEndpoint → ESRCH` ❌ | `InvalidProcess → EINVAL`（dispatcher.rs:231） |

修复理由：C `do_procctl` 对 `vm_isokendpt` 的两种失败（EINVAL/EDEADEPT）统一折叠为 EINVAL（exit.c:122-125）；未知参数 default 分支返回 EINVAL（exit.c:150-151）。Rust 的 `VmError::InvalidEndpoint` 语义是"endpoint 解析失败 → ESRCH"（vm.rs:693，to_errno 映射），用于其他模块的查询路径——procctl 必须显式选择 `InvalidProcess`（EINVAL）。这是 21-P1-4（errno 映射）的同族修复。

### 3.4 D4：VMPPARAM_CLEAR（rusage 补全）

**决策**：`handle_procctl_clear`（exit.rs:163-197）按 C 序列 `free_proc → pt_new → pt_bind` 实现：

```
① free_process_phys(regions_mut)    ← C map_free_proc
② regions_mut().clear()             ← C region_init
③ set_region_top(0) + reset_rusage() ← C free_proc 的 vm_region_top/reset_vm_rusage（本轮补全）
④ free_page_table()（unsafe）        ← C pt_free
⑤ init_page_table()                 ← C pt_new（含内核区映射）
⑥ bind_page_table()                 ← C pt_bind（sys_vmctl_set_addrspace 等价）
```

**本轮补全**：C `free_proc` 会重置 `vm_region_top` + 4 个 rusage 字段（exit.c:41-42）；修复前 Rust 的 CLEAR 路径漏掉这些统计清零。新增 `VmProc::reset_rusage`（vmproc.rs:122，对应 checklist F-042）+ `ActiveProc::reset_rusage`（vmproc_handle.rs:469），CLEAR 路径与退出路径共享同一语义。

**保留**：ACL 与 IN_USE 标志不变（C 只调 free_proc 不调 clear_proc）；endpoint 不变。
**收紧（文档化）**：C 对 EXITING 进程的 CLEAR 无检查；Rust `get_active()` 返回 `None` → EINVAL——病理场景（RS/VFS 不该 CLEAR 正在退出的进程），类型系统防呆。

### 3.5 D5：VMPPARAM_HANDLEMEM 同步路径（SUSPEND 偏差）

**决策**：`handle_procctl_handlemem`（exit.rs:215-278）用 `fork::handle_memory_once` 同步解析（页对齐 + 逐页 map_lookup + wrflag 检查 + CoW 分裂），成功后返回 `VmReply::Ok`；文件后备页 → `NotImplemented`（ENOSYS）。

**偏差（诚实标注）**：C 无条件 SUSPEND（exit.c:148），VFS 阻塞到 VM_VFS_REPLY 续作；Rust 同步完成、立即回复。可观察差异是回复**时序**（C 延后至续作点；Rust 即时），成功/失败 errno 等价。这个近似的前提是**已映射的匿名页**（exec 参数/路径解析的主要场景）同步可解析——`handle_memory_once` 只对已映射共享页做 CoW 分裂，**未映射页（含匿名）当前被静默跳过**（C 会经 `map_pf` 分配或向 VFS 取页），见 §3.7 #9；文件后备区域需要 VFS 提供页，C 会 SUSPEND + 异步，Rust 显式 NotImplemented 而非假装成功（本轮实现：范围起点所在区域为文件后备即拒绝）。**backlog B1**：23 范围接入 VM_VFS_REPLY + transid 状态机后改回 SUSPEND。

**防御性收紧（文档化）**：`len <= 0` → `InvalidAddress`（EFAULT）。C 对 len==0 会空操作回复 OK（handle_memory_step 循环不执行）；Rust 拒绝——malformed VFS 请求，正常 VFS 恒传正 len。

### 3.6 D6：wire format 修复（MessLcVmProcctl overlay）

**决策**：`VmProcctlIn::decode_message`（vm.rs:856）从新增的 `MessageUnion::m_lc_vm_procctl`（message.rs:147）读取；`MessLcVmProcctl`（message.rs:1813）按 C `mess_9` 32 位布局定义：`ull1@0 / ull2@8 / param@16 / who@20 / m1@24 / len@28 / flags@32 / shorts@36 / padding@44`（56 字节）。

**为什么修**：修复前 `VmProcctlIn::decode` 从 `MessageM1` 重排（param@0 / who@16 / m1@24 / len@32 / flags@8）——与 C 发送方（libsys `vm_procctl.c`、vfs `comm.c:198-217` 写 `m9_*` 字段）**字节级不兼容**。这是 16-P0-1 / 19-P1-1 / 21-P1-1 的 wire-format 同族问题：其他 VM 请求都已用专用 overlay 对齐 C 布局，procctl 是最后一个遗留。修复后与 C 发送方字节级兼容，与项目模式统一（"Do NOT decode from MessageM1"——vm.rs:850-855 注释）。

### 3.7 差异清单（C ↔ Rust，诚实标注）

| # | 差异 | C 行为 | Rust 行为 | 判定 |
|---|------|--------|----------|------|
| 1 | HANDLEMEM 回复时序 | SUSPEND + VM_VFS_REPLY 续作 | 同步完成立即 Ok | P1 偏差（B1，23 范围改回） |
| 2 | 文件后备 HANDLEMEM | VFS 提供页 | NotImplemented | P1 缺口（B2） |
| 3 | CLEAR 对 EXITING 进程 | 无检查，允许 | get_active=None → EINVAL | 收紧（B5） |
| 4 | HANDLEMEM len<=0 | 空操作 OK | InvalidAddress（EFAULT） | 收紧 |
| 5 | VM_INSTANCE 递减时序 | free_proc 前 | clear() 内 | 等价（顺序标注） |
| 6 | free_proc/clear_proc 拆分 | 两个函数 | free_process_phys + clear() 合并 | 结构等价 |
| 7 | endpoint/slot 同名变量 | `proc` 一会是 endpoint 一会是 slot | `Endpoint`/`UserSlot` 类型区分 | 类型安全改进 |
| 8 | 页表释放 | pt_free 显式 | clear() 内 destroy（测试构建跳过） | 结构等价（B4） |
| 9 | 未映射页处理 | `map_pf` 逐页分配（anon）/向 VFS 取页（file，SUSPEND） | `handle_memory_once` 仅对已映射共享页做 CoW 分裂；未映射页静默跳过（Ok） | 缺口（B2 相关，23 范围随 VM_VFS_REPLY 一并处理） |

---

## 4. 实现详解

### 4.1 消息路径（dispatcher → exit.rs）

```
VM_EXIT / VM_WILLEXIT / VM_PROCCTL 到达
  │
  ▼ 主循环分发（15-ipc-dispatch）
  ├─ VM_EXIT      → handle_exit（vm_server.rs:1221）→ dispatch_exit（dispatcher.rs:791）
  │                 → exit::handle_vm_exit（exit.rs:42）→ VmReply::Exit（errno 0）
  ├─ VM_WILLEXIT  → dispatch_willexit（dispatcher.rs:804）
  │                 → exit::handle_vm_willexit（exit.rs:66）→ VmReply::Willexit（errno 0）
  └─ VM_PROCCTL   → dispatch_procctl（dispatcher.rs:219）
                    ├─ VFS transid 路径：handle_vfs_transid（vm_server.rs:688）→ decode_message → dispatch_procctl
                    └─ 普通路径：decode_message（dispatcher.rs:1181 分支）→ dispatch_procctl
```

`dispatch_procctl` 的编排（dispatcher.rs:219-280）：

```
1. who <= 0 → InvalidProcess（EINVAL，C exit.c:122-125 折叠）
2. match param:
     1 (CLEAR):     caller ∈ {RS, VFS}？否则 PermissionDenied（EPERM）
                   → handle_procctl_clear → Ok/Error
     2 (HANDLEMEM): caller == VFS？否则 PermissionDenied（EPERM）
                   → handle_procctl_handlemem → Completed → Ok；Err → Error
     _             → InvalidParam（EINVAL，C exit.c:150-151 default）
```

### 4.2 伪码总结

```
handle_vm_exit(table, alloc, frames, ep):
    slot = table.vm_isokendpt(ep)?            // ProcessNotFound → EINVAL
    exiting = table.get_exiting(slot)?        // NotExiting → EINVAL（未 WILLEXIT）
    free_process_phys(exiting.regions_mut(), frames, alloc)
    unsafe { exiting.reap() }                 // clear()：区域/页表/ACL/统计/计数全清 → EmptySlot

handle_vm_willexit(table, ep):
    slot = table.vm_isokendpt(ep)?
    active = table.get_active(slot)?
    active.mark_exiting()                     // 置 EXITING，返回 ExitingProc（丢弃）

handle_procctl_clear(table, alloc, frames, ep):
    slot = table.vm_isokendpt(ep)?            // InvalidEndpoint → EINVAL
    proc = table.get_active(slot)?            // ProcessNotFound → EINVAL（含 EXITING 进程）
    free_process_phys(proc.regions_mut(), frames, alloc)
    proc.regions_mut().clear()
    proc.set_region_top(0); proc.reset_rusage()
    unsafe { proc.free_page_table() }
    proc.init_page_table()?; proc.bind_page_table()?   // PageTableError → EIO

handle_procctl_handlemem(table, alloc, frames, ep, mem, len, wrflag):
    slot = table.vm_isokendpt(ep)?
    proc = table.get_active(slot)?
    if len <= 0: return InvalidAddress        // 防御性收紧（D5）
    if 范围起点区域是 VrParam::File: return NotImplemented   // D4（文件后备需 VFS 供页，B2）
    handle_memory_once(regions_mut, frames, alloc, align(mem), roundup(len), wrflag)
        // PageNotMapped → EFAULT / CowAllocFailed → ENOMEM
    → Completed → VmReply::Ok
```

### 4.3 本轮修复记录（2026-08-16）

| ID | 修复 | 文件 | 对应 C |
|----|------|------|--------|
| 22-P1-1 | `MessLcVmProcctl` overlay + `decode_message`（wire format 对齐 C m9） | message.rs / vm.rs / vm_server.rs / dispatcher.rs | com.h:753-757 |
| 22-P1-2 | procctl errno：InvalidEndpoint→EINVAL、未知参数→EINVAL、who<=0→EINVAL | exit.rs / dispatcher.rs | exit.c:122-125/:149 |
| 22-P1-3 | exit 路径 ev_delete + fdref deref（文件后备区域） | exit.rs | region.c:578-580 / mem_file.c:280-287 |
| 22-P1-4 | CLEAR 路径 rusage/region_top 清零（`reset_rusage` 落地） | exit.rs / vmproc.rs / vmproc_handle.rs | exit.c:41-42（checklist F-042） |

### 4.4 与 03/13/15 的关系

- **03（进程表）**：`vm_isokendpt`（table.rs:276）+ typestate 视图（get_active/get_exiting）是全部退出逻辑的前置；`EndpointError → VmExitError::ProcessNotFound` 折叠来自 03 的约定（munmap/brk 同款）。
- **13（区域映射）**：`RegionMap::iter_mut` + `VirRegion::param`（VrParam::File）+ `ev_delete` 调用面；`free_region_pages`（munmap 路径）与 `free_process_phys`（exit 路径）共享 fdref 平衡语义。
- **15（IPC 分发）**：主循环把三个入口路由到 dispatcher；SUSPEND 语义（HANDLEMEM 的 C 行为）与 VmReply::Suspend 的表达。

---

## 5. 测试要点

> **本节基于 Ch2 C 行为与 Ch3 设计决策，列出测试覆盖要点；§5.1 为 grep 实证清单。**

### 5.1 单元测试清单（grep 实证，2026-08-16）

**exit.rs（10 个）**：

| 测试 | 验证点 | 证据 |
|------|--------|------|
| `test_exit_error_to_errno` | VmExitError 两变体 → EINVAL | `rg "fn test_exit_error_to_errno" exit.rs` → :361 |
| `test_procctl_error_to_errno`（本轮新增） | procctl errno：EINVAL/EPERM/EFAULT | `rg "fn test_procctl_error_to_errno" exit.rs` → :368 |
| `test_procctl_handlemem_rejects_non_positive_len`（本轮新增） | len<=0 → InvalidAddress | `rg "fn test_procctl_handlemem_rejects_non_positive_len" exit.rs` → :381 |
| `test_procctl_handlemem_process_not_found`（本轮新增） | 无效 endpoint → InvalidEndpoint | `rg "fn test_procctl_handlemem_process_not_found" exit.rs` → :398 |
| `test_procctl_clear_process_not_found`（本轮新增） | CLEAR 无效 endpoint → EINVAL | `rg "fn test_procctl_clear_process_not_found" exit.rs` → :410 |
| `test_procctl_handlemem_file_backed_not_implemented`（本轮新增） | 文件后备区域 → NotImplemented（D4） | `rg "fn test_procctl_handlemem_file_backed_not_implemented" exit.rs` → :424 |
| `test_exit_process_not_found` | 无效 endpoint → ProcessNotFound | `rg "fn test_exit_process_not_found" exit.rs` → :456 |
| `test_exit_without_willexit_fails` | 未 WILLEXIT 直接 EXIT → NotExiting | `rg "fn test_exit_without_willexit_fails" exit.rs` → :466 |
| `test_willexit_then_exit_succeeds` | 两阶段正常 → Ok + 槽归空 | `rg "fn test_willexit_then_exit_succeeds" exit.rs` → :478 |
| `test_exit_slot_reusable` | exit 后 get_empty 可用 | `rg "fn test_exit_slot_reusable" exit.rs` → :491 |

**dispatcher.rs（procctl 相关 5 个）**：`test_dispatch_procctl_rejects_negative_param`（param=-1 → InvalidParam）/ `test_dispatch_procctl_rejects_zero_who`（who=0 → InvalidProcess）/ `test_dispatch_procctl_clear_rejects_unauthorized_caller`（EPERM）/ `test_dispatch_procctl_handlemem_rejects_non_vfs_caller`（EPERM）/ `test_dispatch_procctl_unknown_param_returns_einval`（param=99 → EINVAL）。

**vm_server.rs**：`test_handle_vfs_transid_zero_transid` / `test_handle_vfs_transid_invalid_endpoint`（overlay 编码 + EINVAL）/ `test_vm_server_handle_exit_not_found`。

**minix-types vm.rs**：`test_vm_procctl_in_decode_message`（本轮新增——C m9 布局 5 字段解码断言）。

### 5.2 覆盖维度

| 维度 | 覆盖 |
|------|------|
| 两阶段协议 | willexit→exit 全流程、未 willexit 拒绝、endpoint 无效、槽位复用 |
| 错误码 | 退出路径 EINVAL 全折叠；procctl EPERM/EINVAL/EFAULT |
| 权限 | CLEAR 非 RS/VFS、HANDLEMEM 非 VFS 拒绝 |
| wire format | 5 字段按 C m9 偏移解码 |
| HANDLEMEM 文件后备 | 文件后备区域 → NotImplemented（ENOSYS） |
| 资源释放 | 物理页 refcount 递减 + free_pfn（`test_willexit_then_exit_succeeds` 隐式）；fdref/ev_delete 无独立单测（B3） |

### 5.3 覆盖缺口与诚实标注

| # | 缺口 | 原因 | 归属 |
|---|------|------|------|
| G1 | CLEAR 正路径（free+pt_new+bind 全链） | `init_page_table` 在测试构建访问 mock 物理内存 SIGSEGV（exit.rs 测试注释）；真实页表依赖 QEMU | B4（08/01 范围） |
| G2 | 文件后备区域退出时的 fdref 递减断言 | 需构造 VrParam::File 区域 + FdRefTable 状态；当前 mmap file 路径未接线 VFS | B3（23 范围） |
| G3 | HANDLEMEM 匿名页成功路径 | 需真实区域 + CoW 状态；`fork::handle_memory_once` 的测试在 fork.rs 覆盖 | fork.rs 范围 |
| G4 | EXITING 进程被 CLEAR 拒绝断言 | 需先 willexit 再 CLEAR；正路径受 G1 限制 | B5 |

### 5.4 测试统计（截至 2026-08-16）

`cargo test -p minix-vm --lib`：**395 passed / 1 failed**（本轮新增 6 个测试：exit.rs ×5 + vm.rs ×1；基线 390/1，唯一失败 `region::vir_region::tests::test_map_lazy` 为 13 范围 pre-existing）。`cargo check -p minix-vm` 无 error；改动文件无新增 clippy 警告。

---

## 6. 过渡

### 6.1 位置可回答性

本文档位于主循环的**进程生命周期服务组**：`VM_EXIT`/`VM_WILLEXIT` 经 CALLMAP 分发（main.c:543/:546），`VM_PROCCTL` 经 VFS transid 路径或 `do_procctl_notrans`（main.c:141-148/:548）。在启动时序上对应 `init_vm` 之后、任何进程死亡/exec 时的运行时路径——"VM 何时释放资源"由本文档回答。

### 6.2 下游移交（对照 plan.md §3.4）

| 下游 | 移交内容 |
|------|---------|
| 23-vfs-interaction | HANDLEMEM 的 VM_VFS_REPLY 续作 + transid 状态机（B1）；VFS_FDCLOSE 发送（B3）；fdref 表细节 |
| 26-vm-queries | getrusage 读取的统计字段（本 doc 释放时清零） |
| 24-page-cache | IN_CACHE 页在退出时的保留语义（free_process_phys 的 IN_CACHE 门控） |
| 18-vm-fork | fork 建立的共享页在本 doc 退出时按引用计数释放 |

---

## 7. 参见

- [03-vmproc-table.md](03-vmproc-table.md) — endpoint 验证 + typestate 视图（`get_active`/`get_exiting`/`vm_isokendpt`）
- [13-region-mapping.md](13-region-mapping.md) — 区域结构与 `map_free_*` 释放链
- [15-ipc-dispatch.md](15-ipc-dispatch.md) — 主循环分发、SUSPEND 语义、transid 路由
- [11-phys-pagestate.md](11-phys-pagestate.md) — 物理页引用计数（退出时兑现）
- [16-pagefault.md](16-pagefault.md) — 页错误与 `handle_memory_*` 状态机
- [17-cow-mechanism.md](17-cow-mechanism.md) — CoW 分裂（HANDLEMEM 的 CoW 解析）
- [21-vm-munmap.md](21-vm-munmap.md) — 单区域拆除（`free_region_pages`，exit 全释放的粒度对照）
- [23-vfs-interaction.md](23-vfs-interaction.md) — fdref/VFS 异步续作（B1/B3 归属）
- [26-vm-queries.md](26-vm-queries.md) — 查询（getrusage 等，本 doc 不覆盖）
- `minix3/minix/servers/vm/exit.c` — C ground truth（全 156 行）
- `minix3/minix/servers/pm/forkexit.c` — PM 侧退出驱动（:332/:455）
- `minix3/minix/lib/libsys/vm_exit.c`、`minix3/minix/lib/libsys/vm_procctl.c` — libsys 发送方
- `minix3/minix/servers/vfs/comm.c` — VFS 侧 `vm_vfs_procctl_handlemem`（:198-217）
