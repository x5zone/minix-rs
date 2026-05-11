# 27-vm-init-main: 所有零件怎么组装启动

> **分类**: VM 初始化与主循环
> **源码**: `minix3/minix/servers/vm/main.c`, `pagetable.c(pt_init)`, `utility.c(get_mem_chunks)`
> **Rust 对应**: `main.rs`, `global.rs`, `vmproc/table.rs`
> **说明**: VM 进程的完整启动流程、初始化顺序、主循环消息分发、SEF 框架

---

## 1. 概述

### 1.1 VM 启动的核心问题

VM 是系统中最先启动的用户态服务之一。它面临一个**鸡生蛋**的问题：

```
VM 需要内存来运行 → 但内存管理是 VM 的职责 → VM 怎么管理自己的内存？
```

解决方案：**分阶段初始化**——先使用内核提供的静态内存，再逐步建立自己的内存管理系统。

### 1.2 初始化的依赖链

```
┌─────────────────────────────────────────────────────────────┐
│                    VM 初始化依赖链                            │
│                                                             │
│  1. 内核提供的信息                                           │
│     sys_getkinfo() → kernel_boot_info                       │
│     ├── memmap[]: 物理内存布局                               │
│     ├── boot_procs[]: 启动进程列表                           │
│     └── module_list[]: 内核模块列表                          │
│          │                                                  │
│          ▼                                                  │
│  2. 基础数据结构（不需要堆）                                  │
│     memset(vmproc, 0) → 进程表清零                           │
│     acl_init() → ACL 初始化                                  │
│     map_region_init() → 区域管理初始化                        │
│          │                                                  │
│          ▼                                                  │
│  3. 物理内存分配器（不需要堆）                                │
│     get_mem_chunks() → 解析内存映射                          │
│     mem_init() → 初始化位图分配器                             │
│          │                                                  │
│          ▼                                                  │
│  4. 页表系统（需要物理页，但用保留页池）                       │
│     init_proc(VM_PROC_NR) → VM 自身进程槽                   │
│     pt_init() → 建立页表 + 保留页池                          │
│          │                                                  │
│          ▼                                                  │
│  5. 堆可用（pt_init 之后）                                   │
│     __minix_init() → IPC 向量初始化                          │
│     SLABALLOC 可用 → 可以分配 VirRegion 等                   │
│          │                                                  │
│          ▼                                                  │
│  6. 启动进程设置                                             │
│     exec_bootproc() → 为每个启动进程建立地址空间              │
│     CALLMAP → 注册系统调用处理函数                            │
│          │                                                  │
│          ▼                                                  │
│  7. SEF 启动 → 进入主循环                                    │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. C 源码分析

### 2.1 main() — 入口函数

```c
int main(void)
{
    message msg;
    int result, who_e, rcv_sts;
    int caller_slot;

    /* 1. 首次启动时初始化 VM */
    if (is_first_time()) {
        init_vm();
        __vm_init_fresh = 1;
    }

    /* 2. SEF 框架启动 */
    sef_local_startup();
    __vm_init_fresh = 0;

    /* 3. 主循环 */
    while (TRUE) {
        if(missing_spares > 0) {
            alloc_cycle();  /* 补充保留页池 */
        }

        /* 接收消息 */
        sef_receive_status(ANY, &msg, &rcv_sts);

        /* 处理消息... */
    }
}
```

**is_first_time()**：检查 RS（重启服务器）是否还在启动中。如果是首次启动，RS 会有 `RTS_BOOTINHIBIT` 标志。如果是重启后恢复，则跳过 `init_vm()`。

### 2.2 init_vm() — 完整初始化流程

```c
void init_vm(void)
{
    int s, i;
    static struct memory mem_chunks[NR_MEMS];
    struct boot_image *ip;

    /* ===== 阶段 1: 获取内核信息 ===== */
    sys_getkinfo(&kernel_boot_info);
    assert(kernel_boot_info.mmap_size > 0);
    assert(kernel_boot_info.mods_with_kernel > 0);

    /* ===== 阶段 2: 基础数据结构（不需要堆） ===== */

    /* 解析物理内存布局 */
    get_mem_chunks(mem_chunks);

    /* 进程表清零 */
    memset(vmproc, 0, sizeof(vmproc));
    for(i = 0; i < ELEMENTS(vmproc); i++) {
        vmproc[i].vm_slot = i;
    }

    /* ACL 初始化 */
    acl_init();

    /* 区域管理初始化 */
    map_region_init();

    /* ===== 阶段 3: 物理内存分配器 ===== */
    mem_init(mem_chunks);

    /* ===== 阶段 4: 页表系统 ===== */
    init_proc(VM_PROC_NR);   /* VM 自身进程槽 */
    pt_init();               /* 建立页表 + 保留页池 */

    /* ===== 阶段 5: 堆可用 ===== */
    __minix_init();          /* IPC 向量初始化 */

    /* 修正总页数（内核模块占用的内存） */
    for(mod = ...) {
        mem_add_total_pages(len / VM_PAGE_SIZE);
    }
    mem_add_total_pages((kern_dyn + kern_static) / VM_PAGE_SIZE);

    /* ===== 阶段 6: 启动进程设置 ===== */
    for(ip = boot_procs[0]; ip < boot_procs[NR_BOOT_PROCS]; ip++) {
        if(ip->proc_nr < 0) continue;
        if(ip->proc_nr == VM_PROC_NR) continue;  /* VM 已设置 */

        vmp = init_proc(ip->proc_nr);
        exec_bootproc(vmp, ip);  /* 为启动进程建立地址空间 */
        free_mem(ABS2CLICK(ip->start_addr), ABS2CLICK(ip->len));
    }

    /* ===== 阶段 7: 注册系统调用 ===== */
    memset(vm_calls, 0, sizeof(vm_calls));
    CALLMAP(VM_MMAP, do_mmap);
    CALLMAP(VM_MUNMAP, do_munmap);
    CALLMAP(VM_MAP_PHYS, do_map_phys);
    CALLMAP(VM_EXIT, do_exit);
    CALLMAP(VM_FORK, do_fork);
    CALLMAP(VM_BRK, do_brk);
    /* ... 更多 CALLMAP ... */

    /* 标记 VM 实例 */
    num_vm_instances = 1;
    vmproc[VM_PROC_NR].vm_flags |= VMF_VM_INSTANCE;
}
```

### 2.3 pt_init() — 页表初始化

```c
void pt_init(void)
{
    /* 1. 保留页池：使用 BSS 段的静态内存 */
    sparepages_mem = (vir_bytes) static_sparepages;

    /* 2. 创建保留队列 */
    spare_pagequeue = reservedqueue_new(SPAREPAGES, 1, 1, 0);

    /* 3. 将静态内存注册到保留队列 */
    for(s = 0; s < STATIC_SPAREPAGES; s++) {
        void *v = (void *)(sparepages_mem + s * VM_PAGE_SIZE);
        phys_bytes ph;
        sys_umap(SELF, VM_D, v, VM_PAGE_SIZE * SPAREPAGES, &ph);
        reservedqueue_add(spare_pagequeue, v, ph);
    }

    /* 4. 建立内核映射 */
    while(sys_vmctl_get_mapping(pindex, &addr, &len, &flags) == OK) {
        kern_mappings[pindex] = ...;
        sys_vmctl_reply_mapping(pindex, vir);
    }

    /* 5. 分配内核映射页表 */
    pt_allocate_kernel_mapped_pagetables();

    /* 6. 复制当前页目录到 VM 自己的结构 */
    newpt = &vmprocess->vm_pt;
    pt_new(newpt);
    sys_vmctl_get_pdbr(SELF, &mypdbr);
    sys_vircopy(NONE, mypdbr, SELF, currentpagedir, VM_PAGE_SIZE, 0);

    /* 7. 建立页目录项 */
    for(pde = 0; pde < ARCH_VM_DIR_ENTRIES; pde++) {
        /* 复制内核映射 */
        /* 设置 VM 自身的映射 */
    }

    /* 8. 切换到新页表 */
    pt_map_page(newpt, ...);
    sys_vmctl_set_pdbr(SELF, newpt->pt_dir_phys);

    /* 9. 映射 VM 自身的堆 */
    pt_map_in_vm(VM_OWN_HEAPBASE, ...);
}
```

**关键洞察**：`pt_init()` 是最复杂的初始化步骤。它必须在**没有堆分配器**的情况下工作，使用 BSS 段的 `static_sparepages` 作为临时页表页。

### 2.4 init_proc() — 进程槽初始化

```c
static struct vmproc *init_proc(endpoint_t ep_nr)
{
    struct boot_image *ip;

    for(ip = boot_procs[0]; ip < boot_procs[NR_BOOT_PROCS]; ip++) {
        if(ip->proc_nr != ep_nr) continue;

        vmp = &vmproc[ip->proc_nr];
        assert(!(vmp->vm_flags & VMF_INUSE));
        clear_proc(vmp);
        vmp->vm_flags = VMF_INUSE;
        vmp->vm_endpoint = ip->endpoint;
        vmp->vm_boot = ip;

        return vmp;
    }
    panic("no init_proc");
}
```

### 2.5 主循环 — 消息分发

```c
while (TRUE) {
    /* 补充保留页池 */
    if(missing_spares > 0) alloc_cycle();

    /* 接收消息 */
    sef_receive_status(ANY, &msg, &rcv_sts);

    /* 忽略通知 */
    if(is_ipc_notify(rcv_sts)) continue;

    /* 验证调用者 */
    who_e = msg.m_source;
    vm_isokendpt(who_e, &caller_slot);

    /* 分发消息 */
    type = msg.m_type;
    c = CALLNUMBER(type);
    result = ENOSYS;

    if(VFS 事务) {
        result = do_procctl(&msg, transid);
    } else if(RS_INIT) {
        result = do_sef_init_request(&msg);
        result = SUSPEND;  /* 不回复 RS */
    } else if(VM_PAGEFAULT) {
        do_pagefaults(&msg);
        continue;  /* 不回复，内核通过 sys_vmctl 解除阻塞 */
    } else if(c >= 0 && vm_calls[c].vmc_func) {
        if(acl_check(&vmproc[caller_slot], c) == OK) {
            result = vm_calls[c].vmc_func(&msg);
        }
    }

    /* 发送回复（除非 SUSPEND） */
    if(result != SUSPEND) {
        msg.m_type = result;
        ipc_send(who_e, &msg);
    }
}
```

**消息分发的优先级**：

| 优先级 | 消息类型 | 处理方式 | 回复 |
|--------|---------|---------|------|
| 1 | VFS 事务 | `do_procctl` | 正常回复 |
| 2 | RS_INIT | `do_sef_init_request` | SUSPEND（不回复） |
| 3 | VM_PAGEFAULT | `do_pagefaults` | 不回复（内核解除阻塞） |
| 4 | 普通请求 | `vm_calls[c].vmc_func` | 正常回复 |
| 5 | 无效请求 | ENOSYS | 错误回复 |

### 2.6 SEF 框架

SEF（System Event Framework）是 Minix3 的服务管理框架：

```c
static void sef_local_startup(void)
{
    sef_setcb_init_fresh(sef_cb_init_fresh);       /* 首次启动 */
    sef_setcb_init_lu(sef_cb_init_lu_restart);     /* Live Update */
    sef_setcb_init_restart(sef_cb_init_lu_restart); /* 重启 */
    sef_setcb_lu_state_changed(sef_cb_lu_state_changed);
    sef_setcb_signal_handler(sef_cb_signal_handler);
    sef_startup();
}
```

**SEF 回调**：

| 回调 | 触发时机 | VM 的处理 |
|------|---------|----------|
| `sef_cb_init_fresh` | 首次启动 | `map_service()` 注册服务权限 |
| `sef_cb_init_lu` | Live Update | 恢复状态 |
| `sef_cb_init_restart` | 重启 | 恢复状态 |
| `sef_cb_signal_handler` | 信号 | `SIGKMEM` → `do_memory()` |

### 2.7 SIGKMEM 信号处理

```c
static void sef_cb_signal_handler(int signo)
{
    switch(signo) {
        case SIGKMEM:
            do_memory();  /* 内核请求内存 */
            break;
    }

    if(missing_spares > 0) {
        alloc_cycle();  /* 补充保留页池 */
    }

    pt_clearmapcache();  /* 清除页表映射缓存 */
}
```

`SIGKMEM` 是内核发送的信号，表示内核需要分配内存（如新的页表页）。VM 在信号处理中分配内存并映射给内核。

---

## 3. Rust 设计

### 3.1 现有 Rust 代码状态

| 组件 | 状态 | 说明 |
|------|------|------|
| `main.rs` | ⚠️ 骨架 | `VmServer` 结构体，空的 `init()` 和 `run()` |
| `global.rs` | ✅ 已实现 | `BOOT_INFO`, `TOTAL_PAGES`, `VM_INSTANCE_COUNT` |
| `VmProcTable` | ✅ 已实现 | 进程表，`AssumeSyncCell` 后端 |
| `VmPageAllocator` | ✅ 已实现 | 物理页分配器 |
| `PhysAllocator` | ✅ 已实现 | 位图/buddy/线段树分配器 |
| `Paging` trait | ✅ 已实现 | 页表操作 |
| `MemType` trait | ✅ 已实现 | 内存类型系统 |
| `PageCache` | ❌ 不存在 | 26-cache-memtypes 中已设计 |
| `VfsRequestQueue` | ❌ 不存在 | 24-vfs-interaction 中已设计 |
| IPC 分发 | ❌ 不存在 | 需要实现 |
| SEF 框架 | ❌ 不存在 | 需要设计 |

### 3.2 VmServer 重构

```rust
pub(crate) struct VmServer {
    proc_table: VmProcTable,
    page_alloc: VmPageAllocator,
    page_cache: PageCache,
    vfs_queue: VfsRequestQueue,
    call_table: [Option<VmCallEntry>; NR_VM_CALLS],
}

struct VmCallEntry {
    handler: fn(&Message, &mut VmServer) -> Result<(), VmError>,
    name: &'static str,
}

impl VmServer {
    pub(crate) fn new() -> Self {
        Self {
            proc_table: VmProcTable::new(),
            page_alloc: VmPageAllocator::new(Box::new(BitmapAllocator::new())),
            page_cache: PageCache::new(),
            vfs_queue: VfsRequestQueue::new(),
            call_table: [None; NR_VM_CALLS],
        }
    }
}
```

### 3.3 初始化流程

```rust
impl VmServer {
    /// 阶段 1-3: 获取内核信息 + 基础数据结构 + 物理内存分配器
    pub(crate) fn init_phase1(&mut self) -> Result<(), VmError> {
        /* 获取内核启动信息 */
        let kinfo = sys_getkinfo()?;

        /* 解析物理内存布局 */
        let mem_chunks = get_mem_chunks(&kinfo);

        /* 初始化进程表 */
        self.proc_table.clear_all();

        /* 初始化 ACL */
        self.proc_table.acl_init();

        /* 初始化物理内存分配器 */
        self.page_alloc.init_from_chunks(&mem_chunks);

        /* 设置全局变量 */
        unsafe { global::init(self.page_alloc.total_pages()); }

        Ok(())
    }

    /// 阶段 4: 页表系统
    pub(crate) fn init_phase2(&mut self) -> Result<(), VmError> {
        /* 初始化 VM 自身进程槽 */
        let mut vm_proc = self.proc_table
            .get_empty(VM_SLOT)
            .unwrap()
            .activate(Endpoint::VM);

        vm_proc.init_page_table()?;
        vm_proc.init_regions();

        /* 建立保留页池 */
        /* pt_init() 等价操作... */

        Ok(())
    }

    /// 阶段 5-6: 堆可用 + 启动进程设置
    pub(crate) fn init_phase3(&mut self) -> Result<(), VmError> {
        /* IPC 向量初始化 */
        /* __minix_init() 等价操作... */

        /* 为每个启动进程建立地址空间 */
        let boot_info = global::boot_info();
        for i in 0..NR_BOOT_PROCS {
            let ip = &boot_info[i];
            if ip.proc_nr < 0 { continue; }
            if ip.proc_nr == VM_PROC_NR as i32 { continue; }

            let mut vmp = self.proc_table
                .get_empty(UserSlot::new(ip.proc_nr as usize))
                .unwrap()
                .activate(Endpoint::from(ip.endpoint));

            vmp.init_page_table()?;
            vmp.init_regions();

            exec_bootproc(&mut vmp, ip, &mut self.page_alloc)?;
        }

        Ok(())
    }

    /// 阶段 7: 注册系统调用
    pub(crate) fn init_phase4(&mut self) {
        self.call_table[CALLNUMBER(VM_MMAP)] = Some(VmCallEntry {
            handler: do_mmap, name: "VM_MMAP",
        });
        self.call_table[CALLNUMBER(VM_MUNMAP)] = Some(VmCallEntry {
            handler: do_munmap, name: "VM_MUNMAP",
        });
        self.call_table[CALLNUMBER(VM_MAP_PHYS)] = Some(VmCallEntry {
            handler: do_map_phys, name: "VM_MAP_PHYS",
        });
        self.call_table[CALLNUMBER(VM_EXIT)] = Some(VmCallEntry {
            handler: do_exit, name: "VM_EXIT",
        });
        self.call_table[CALLNUMBER(VM_FORK)] = Some(VmCallEntry {
            handler: do_fork, name: "VM_FORK",
        });
        self.call_table[CALLNUMBER(VM_BRK)] = Some(VmCallEntry {
            handler: do_brk, name: "VM_BRK",
        });
        /* ... 更多注册 ... */

        global::inc_vm_instance();
    }

    /// 完整初始化
    pub(crate) fn init(&mut self) {
        self.init_phase1().expect("VM phase1 init failed");
        self.init_phase2().expect("VM phase2 init failed");
        self.init_phase3().expect("VM phase3 init failed");
        self.init_phase4();
    }
}
```

### 3.4 主循环

```rust
impl VmServer {
    pub(crate) fn run(&mut self) -> ! {
        loop {
            /* 补充保留页池 */
            if self.page_alloc.needs_refill() {
                self.page_alloc.refill_reserved();
            }

            /* 接收消息 */
            let (msg, rcv_status) = ipc_receive(ANY);

            /* 忽略通知 */
            if is_ipc_notify(rcv_status) {
                continue;
            }

            let who_e = msg.m_source;
            let caller_slot = match self.proc_table.find_slot_by_endpoint(who_e) {
                Some(slot) => slot,
                None => {
                    log::warn!("invalid caller {}", who_e);
                    continue;
                }
            };

            let result = self.dispatch(&msg, caller_slot);

            match result {
                DispatchResult::Reply(code) => {
                    let mut reply = msg;
                    reply.m_type = code;
                    ipc_send(who_e, &reply);
                }
                DispatchResult::Suspend => {}
                DispatchResult::NoReply => {}
            }
        }
    }

    fn dispatch(&mut self, msg: &Message, caller_slot: UserSlot) -> DispatchResult {
        let type_val = msg.m_type;
        let c = CALLNUMBER(type_val);

        /* 特殊消息优先处理 */
        if msg.m_source == VFS_PROC_NR {
            if let Some(transid) = extract_vfs_transid(type_val) {
                let result = do_procctl(msg, transid, self);
                return DispatchResult::Reply(result);
            }
        }

        if msg.m_type == RS_INIT && msg.m_source == RS_PROC_NR {
            do_sef_init_request(msg, self);
            return DispatchResult::Suspend;
        }

        if msg.m_type == VM_PAGEFAULT {
            if !is_from_kernel(rcv_status) {
                log::warn!("faked VM_PAGEFAULT from {}", msg.m_source);
            }
            do_pagefaults(msg, self);
            return DispatchResult::NoReply;
        }

        /* 普通请求：查表分发 */
        if let Some(entry) = &self.call_table[c] {
            if self.proc_table.acl_check(caller_slot, c) != Ok(()) {
                log::warn!("unauthorized {} by {}", entry.name, msg.m_source);
                return DispatchResult::Reply(EACCES);
            }
            match (entry.handler)(msg, self) {
                Ok(()) => DispatchResult::Reply(OK),
                Err(VmError::Suspend) => DispatchResult::Suspend,
                Err(e) => DispatchResult::Reply(e.as_errno()),
            }
        } else {
            DispatchResult::Reply(ENOSYS)
        }
    }
}

enum DispatchResult {
    Reply(i32),
    Suspend,
    NoReply,
}
```

### 3.5 系统调用处理函数签名

```rust
type VmCallHandler = fn(&Message, &mut VmServer) -> Result<(), VmError>;

fn do_mmap(msg: &Message, server: &mut VmServer) -> Result<(), VmError> {
    let caller = server.proc_table.find_active_by_endpoint(msg.m_source)?;
    mmap_region(caller, msg, &mut server.page_alloc)?;
    Ok(())
}

fn do_brk(msg: &Message, server: &mut VmServer) -> Result<(), VmError> {
    let caller = server.proc_table.find_active_by_endpoint(msg.m_source)?;
    real_brk(caller, msg, &mut server.page_alloc)?;
    Ok(())
}

fn do_fork(msg: &Message, server: &mut VmServer) -> Result<(), VmError> {
    vm_fork(msg, &mut server.proc_table, &mut server.page_alloc)?;
    Ok(())
}

fn do_exit(msg: &Message, server: &mut VmServer) -> Result<(), VmError> {
    vm_exit(msg, &mut server.proc_table, &mut server.page_alloc)?;
    Ok(())
}

fn do_munmap(msg: &Message, server: &mut VmServer) -> Result<(), VmError> {
    let caller = server.proc_table.find_active_by_endpoint(msg.m_source)?;
    unmap_region(caller, msg, &mut server.page_alloc)?;
    Ok(())
}
```

---

## 4. 初始化顺序的约束分析

### 4.1 堆依赖的三层

| 层级 | 可用时机 | 分配方式 | 典型用途 |
|------|---------|---------|---------|
| **L0: 静态** | 编译时 | BSS 段 | `vmproc[]`, `static_sparepages`, `free_pages_bitmap` |
| **L1: 保留页** | `pt_init()` 后 | `reservedqueue_alloc()` | 页表页（pt_new） |
| **L2: 堆** | `pt_init()` 后 | `SLABALLOC` / `Box::new` | `VirRegion`, `PhysBlock`, `cached_page` |

### 4.2 各组件的初始化顺序约束

```
时间 ──────────────────────────────────────────────────────►

L0可用  L1可用  L2可用
  │       │       │
  │       │       │
  ├── vmproc 清零 (memset)
  ├── acl_init
  ├── map_region_init
  ├── mem_init (物理分配器)
  │       │
  │       ├── init_proc(VM) → VM 进程槽
  │       ├── pt_init → 保留页池 + 页表
  │       │       │
  │       │       ├── __minix_init → IPC 向量
  │       │       ├── exec_bootproc → VirRegion (L2!)
  │       │       ├── CALLMAP 注册
  │       │       └── sef_startup
  │       │
  │       └── 主循环开始
  │
  └── 全程可用: 全局静态变量
```

### 4.3 Rust 中的等价约束

| Minix3 | Rust | 约束 |
|--------|------|------|
| `memset(vmproc, 0)` | `VmProcTable::new()` | L0: 静态数组 |
| `acl_init()` | `Acl::new()` | L0: 位图 |
| `mem_init()` | `BitmapAllocator::new()` | L0: 静态位图 |
| `static_sparepages` | `ReservedRegion::new()` // Direct Map 下不再需要 | L0: BSS 段 |
| `pt_new()` | `Paging::new()` | L1: 保留页 |
| `SLABALLOC` | `Box::new()` | L2: 堆 |
| `VirRegion::new()` | `VirRegion::new()` | L2: Vec 分配 |

**Rust 优势**：通过类型系统可以在编译时检查堆依赖——需要堆的类型不能在 L0/L1 阶段构造。

---

## 5. SEF 框架的 Rust 设计

### 5.1 简化版 SEF

Minix3 的 SEF 框架很复杂（Live Update、状态转移等）。Rust 版本先实现核心功能：

```rust
pub(crate) struct SefFramework {
    init_fresh_cb: Option<fn(&mut VmServer) -> Result<(), i32>>,
    init_restart_cb: Option<fn(&mut VmServer) -> Result<(), i32>>,
    signal_cb: Option<fn(i32, &mut VmServer)>,
}

impl SefFramework {
    pub(crate) fn new() -> Self {
        Self {
            init_fresh_cb: None,
            init_restart_cb: None,
            signal_cb: None,
        }
    }

    pub(crate) fn set_init_fresh(&mut self, cb: fn(&mut VmServer) -> Result<(), i32>) {
        self.init_fresh_cb = Some(cb);
    }

    pub(crate) fn set_signal_handler(&mut self, cb: fn(i32, &mut VmServer)) {
        self.signal_cb = Some(cb);
    }

    pub(crate) fn startup(&self, server: &mut VmServer) {
        if let Some(cb) = self.init_fresh_cb {
            cb(server).expect("SEF init_fresh failed");
        }
    }
}
```

### 5.2 信号处理

```rust
fn sef_signal_handler(signo: i32, server: &mut VmServer) {
    match signo {
        SIGKMEM => {
            do_memory(server);
        }
        _ => {}
    }

    if server.page_alloc.needs_refill() {
        server.page_alloc.refill_reserved();
    }
}
```

---

## 6. exec_bootproc — 启动进程地址空间建立

### 6.1 C 源码逻辑

```c
/* main.c 中 */
for(ip = boot_procs[0]; ip < boot_procs[NR_BOOT_PROCS]; ip++) {
    if(ip->proc_nr < 0) continue;
    if(ip->proc_nr == VM_PROC_NR) continue;

    vmp = init_proc(ip->proc_nr);
    exec_bootproc(vmp, ip);  /* 为启动进程建立地址空间 */
    free_mem(ABS2CLICK(ip->start_addr), ABS2CLICK(ip->len));
}
```

`exec_bootproc` 的核心操作：
1. 为进程创建页表
2. 将进程的二进制代码映射到其地址空间
3. 设置进程的堆和栈区域
4. 释放启动时占用的物理内存

### 6.2 Rust 设计

```rust
fn exec_bootproc(
    vmp: &mut ActiveProc<'_>,
    boot_image: &BootImage,
    page_alloc: &mut VmPageAllocator,
) -> Result<(), VmError> {
    /* 1. 初始化页表 */
    vmp.init_page_table()?;

    /* 2. 初始化区域管理 */
    vmp.init_regions();

    /* 3. 映射代码段 */
    let text_region = map_page_region(
        vmp,
        boot_image.start_addr as usize,
        (boot_image.start_addr + boot_image.text_len) as usize,
        boot_image.text_len as usize,
        VrFlags::READ | VrFlags::EXEC,
        0,
        &MEM_TYPE_ANON,
    )?;

    /* 4. 映射数据段 */
    let data_region = map_page_region(
        vmp,
        boot_image.start_addr as usize + boot_image.text_len as usize,
        ...,
        VrFlags::READ | VrFlags::WRITE,
        0,
        &MEM_TYPE_ANON,
    )?;

    /* 5. 设置堆区域 */
    vmp.set_heap_base(data_region.vaddr + data_region.length);

    /* 6. 释放启动占用的物理内存 */
    page_alloc.free_phys(PhysBytes::new(boot_image.start_addr as u64),
        boot_image.len as usize / 4096);

    Ok(())
}
```

---

## 7. 完整的初始化时序图

```
┌────────┐     ┌──────────┐     ┌──────────┐     ┌──────────┐
│ Kernel │     │ VM main  │     │ PhysAlloc│     │ PageTable│
└───┬────┘     └────┬─────┘     └────┬─────┘     └────┬─────┘
    │               │                │                │
    │ sys_getkinfo  │                │                │
    │──────────────►│                │                │
    │ kinfo         │                │                │
    │◄──────────────│                │                │
    │               │                │                │
    │               │ get_mem_chunks │                │
    │               │───────────────►│                │
    │               │                │ mem_init()     │
    │               │                │───────────┐   │
    │               │                │◄──────────┘   │
    │               │                │                │
    │               │ init_proc(VM)  │                │
    │               │───────────────────────────────►│
    │               │                │                │
    │               │ pt_init()      │                │
    │               │───────────────────────────────►│
    │               │                │   reservedqueue_new
    │               │                │   sys_umap(sparepages)
    │               │                │   pt_new(vm_pt)
    │               │                │   sys_vmctl_get_pdbr
    │               │                │   复制页目录
    │               │                │   sys_vmctl_set_pdbr
    │               │◄───────────────────────────────│
    │               │                │                │
    │               │ __minix_init() │                │
    │               │───────┐        │                │
    │               │◄──────┘        │                │
    │               │                │                │
    │               │ exec_bootproc  │                │
    │               │───────────────────────────────►│
    │               │  (for each boot proc)          │
    │               │  init_proc + map regions       │
    │               │◄───────────────────────────────│
    │               │                │                │
    │               │ CALLMAP 注册   │                │
    │               │───────┐        │                │
    │               │◄──────┘        │                │
    │               │                │                │
    │               │ sef_startup()  │                │
    │               │───────┐        │                │
    │               │◄──────┘        │                │
    │               │                │                │
    │  ═══════════ 主循环开始 ═══════════                │
    │               │                │                │
    │   IPC msg     │                │                │
    │──────────────►│                │                │
    │               │ dispatch       │                │
    │               │───────────────►│                │
    │               │◄───────────────│                │
    │   IPC reply   │                │                │
    │◄──────────────│                │                │
    │               │                │                │
```

---

## 8. 实现清单

### 8.1 需要修改的文件

| 文件 | 修改内容 | 优先级 |
|------|---------|--------|
| `main.rs` | 重构 `VmServer`，实现完整初始化和主循环 | 🔴 P0 |
| `global.rs` | 添加 `get_mem_chunks` 等辅助函数 | 🔴 P0 |

### 8.2 需要新增的文件

| 文件 | 内容 | 优先级 |
|------|------|--------|
| `dispatch.rs` | 消息分发逻辑、CALLMAP 表 | 🔴 P0 |
| `sef.rs` | 简化版 SEF 框架 | 🟡 P1 |
| `boot.rs` | `exec_bootproc` 启动进程设置 | 🟡 P1 |

### 8.3 测试计划

| 测试 | 描述 |
|------|------|
| `test_init_phase1` | 阶段1初始化：内核信息+进程表+分配器 |
| `test_init_phase2` | 阶段2初始化：页表系统 |
| `test_init_phase3` | 阶段3初始化：启动进程 |
| `test_dispatch_mmap` | 消息分发：VM_MMAP |
| `test_dispatch_brk` | 消息分发：VM_BRK |
| `test_dispatch_pagefault` | 消息分发：VM_PAGEFAULT |
| `test_dispatch_unknown` | 消息分发：未知请求 → ENOSYS |
| `test_dispatch_unauthorized` | 消息分发：ACL 拒绝 |
| `test_dispatch_suspend` | 消息分发：SUSPEND 不回复 |
| `test_reserved_refill` | 保留页池补充 |

---

## 9. 设计洞察

### 9.1 VmServer 的所有权模型

Minix3 的所有全局变量（`vmproc[]`, `total_pages`, `vm_calls[]` 等）在 Rust 中被封装到 `VmServer` 结构体中：

| Minix3 全局变量 | Rust 字段 | 所有权 |
|----------------|----------|--------|
| `vmproc[]` | `proc_table: VmProcTable` | `VmServer` 独占 |
| `total_pages` | `page_alloc.total_pages()` | `VmServer` 独占 |
| `vm_calls[]` | `call_table: [Option<VmCallEntry>]` | `VmServer` 独占 |
| `free_pages_bitmap[]` | `page_alloc.phys_alloc` | `VmServer` 独占 |
| `cache_hash_bydev[]` | `page_cache.by_dev` | `VmServer` 独占 |
| `first_queued` (VFS) | `vfs_queue.queued` | `VmServer` 独占 |

**优势**：所有状态都在 `VmServer` 中，主循环 `run(&mut self)` 独占 `&mut VmServer`，不存在数据竞争。

### 9.2 初始化的不可逆性

VM 的初始化是**不可逆**的——一旦某个阶段完成，就不能回退。这意味着：
- 如果 `pt_init()` 失败，只能 panic
- 如果 `exec_bootproc()` 失败，只能 panic
- 初始化阶段不处理 IPC 请求

Rust 中用 `Result<(), VmError>` + `expect()` 表达这种不可逆性。

### 9.3 主循环的 SUSPEND 语义

`SUSPEND` 是 VM 的核心异步机制：
- **VFS 请求**：VM 发出异步请求后返回 SUSPEND，不回复调用者
- **VFS 回复到达**：VM 处理回复，然后回复原始调用者
- **Pagefault**：不回复，内核通过 `sys_vmctl` 解除进程阻塞

Rust 中用 `DispatchResult` 枚举清晰表达三种回复语义。

### 9.4 与 Live Update 的关系

Minix3 的 SEF 支持 Live Update（热更新）——在不停止系统的情况下更新 VM 代码。这需要：
1. 新旧 VM 实例共享内存
2. 状态转移（序列化/反序列化）
3. IPC 过滤（更新期间只允许安全消息）

Rust 版本暂不实现 Live Update，但 `VmServer` 的设计应预留状态序列化接口。
