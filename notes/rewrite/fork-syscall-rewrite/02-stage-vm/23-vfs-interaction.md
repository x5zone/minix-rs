# 23-vfs-interaction: VFS 异步对话 —— fdref、文件映射与缺页 I/O

> **分类**: 阶段 8 — 跨服务协作（VFS ↔ VM 异步对话）
> **源码**: `minix3/minix/servers/vm/vfs.c`（全 143 行：`activate` :43-53 / `ID_MAX` :55 / `vfs_request` :60-104 / `do_vfs_reply` :109-142 / `STATELEN` :31）+ `minix3/minix/servers/vm/fdref.c`（全 177 行：`fdref_sanitycheck` :37-91 / `fdref_new` :93-107 / `fdref_ref` :109-114 / `fdref_deref` :116-154 / `fdref_dedup_or_new` :156-176）+ `minix3/minix/servers/vm/mem_file.c`（全 287 行：`mem_type_mappedfile` :30-49 / `mappedfile_unreference` :51-57 / `cow_block` :59-82 / `mappedfile_pagefault` :84-155 / `mappedfile_writable` :173-175 / `mappedfile_copy` :177-193 / `mappedfile_setfile` :195-247 / `mappedfile_split` :249-267 / `mappedfile_lowshrink` :273-278 / `mappedfile_delete` :280-287）+ `mmap.c`（`mmap_file` :84-133 / `do_vfs_mmap` :135-158 / `mmap_file_cont` :160-195 / `do_mmap` 文件分支 :254-273）+ `pagefaults.c`（`pf_cont` :161-168 / `handle_memory_continue` :170-196 / `handle_memory_final` :198-235 / `handle_memory_step` :336+）+ `minix3/minix/servers/vfs/misc.c`（`do_vm_call` :383-473）+ `minix3/minix/lib/libminixfs/cache.c`（`lmfs_get_block_ino` PEEK 语义 :314-465）+ `minix3/minix/include/minix/com.h`（`VFS_VMCALL_*` :694-699 / `VMVFSREQ_*` :701-704 / `VM_VFS_REPLY` :707-714）+ `minix3/minix/include/minix/ipc.h`（`mess_10` :85-91）+ `minix3/minix/include/minix/ipcconst.h`（`_ASSERT_MSG_SIZE` :17）
> **Rust 模块**: `os/servers/vm/src/vfs_queue.rs`（`VfsRequestType` :21 / `VfsRequestState` :28 / `VfsCallbackFn` :46 / `VfsRequest` :53 / `VfsReply` :65 / `VfsQueueError` :76 / `VfsRequestQueue` :90 / `request` :107 / `handle_reply` :126 / `test_active_request` :170）+ `os/servers/vm/src/fdref.rs`（`FdRefEntry` :16 / `PendingFdClose` :23 / `FdRefTable` :44 / `create` :78 / `dedup_or_new` :114 / `ref_entry` :151 / `deref_entry` :157 / `get` :204）+ `os/servers/vm/src/mmap.rs`（`handle_mmap` :268 / `FileMapParams` :393 / `mmap_file` :416 / `handle_vfs_mmap` :489 / `mmap_file_cont` :540）+ `os/servers/vm/src/memtype.rs`（`PagefaultResult` :173 / `MappedFile` :919 / `ev_pagefault` :954 / `MEM_TYPE_MAPPED_FILE` :1133）+ `os/servers/vm/src/cow_exec_pf.rs`（`handle_pagefault` :23 / `enqueue_fdio` :67 / `mappedfile_pf_cont` :114 / `PagefaultAction` :351）+ `os/servers/vm/src/ipc/dispatcher.rs`（`dispatch_vfs_reply` :337 / `dispatch_vfs_mmap` :420 / VM_VFS_REPLY 分支 :1187-1193）+ `os/servers/vm/src/vm_server.rs`（`dispatch_pagefault` :743 / `parts_mut` :775）+ `os/servers/vm/src/page_cache.rs`（`VMC_NO_INODE` :25 / `CacheKey` :28 / `PageCache` :39）+ `os/libs/minix-types/src/ipc/message.rs`（`m_vm_vfs_reply` :150 / `MessVmVfsReply` :1874）+ `os/libs/minix-types/src/ipc/vm.rs`（`VmVfsReplyIn` :560 / `decode_message` :586）
> **前置**: `12-memtype.md`（memtype 回调体系）、`13-region-mapping.md`（区域结构）、`15-ipc-dispatch.md`（主循环 + SUSPEND + VM_VFS_REPLY 分支）、`20-vm-mmap.md`（mmap 区域建立）
> **说明**: 本文档管 **VM 与 VFS 两个用户态服务器之间的异步对话**——文件后备映射的两条链路：`mmap(文件)` 先向 VFS 要文件元数据（FDLOOKUP），文件页缺页再向 VFS 要页内容（FDIO），最后引用消失时通知 VFS 关 fd（FDCLOSE）。核心机制是串行激活的请求队列（vfs.c）、fdref 引用计数（fdref.c）、文件映射 memtype 的缺页分流（mem_file.c）。**不覆盖**：页缓存内部结构（双哈希/LRU/VMSF_ONCE，24）、mmap 区域建立/拆除全貌（20/21）、CoW 分裂机制本体（17）、主循环 SUSPEND/回复机制（15）。

---

## 1. 概念：VM 与 VFS 的异步对话

### 1.0 章节引言

**目标读者**：已读完 12（memtype）、13（区域）、15（主循环分发）、20（mmap）的读者。本文档回答三个问题：文件映射为什么需要 VM 和 VFS 两个服务器对话？对话为什么必须异步且串行？fd 的引用计数如何保证"映射还在，fd 不关"？

**本章不讲什么**：页缓存的双哈希/LRU（24）、CoW 的物理页分裂（17）、mmap 的地址选择与权限（20）——这里只讲"对话协议"这一侧：请求怎么发、回复怎么接、fd 生命周期怎么管、文件页怎么进来。

### 1.1 为什么 VM 需要和 VFS 对话

文件映射（`mmap(fd)` + 文件缺页）横跨两个服务方：

- **VFS 拥有文件**：fd 对应的 dev/ino、文件大小、文件内容都只有 VFS（及其下的 FS 驱动）知道。
- **VM 拥有内存**：地址空间、物理页、页表、页缓存只有 VM 知道。

Minix3 把两者分开成独立用户态服务器，于是出现三类跨服务对话：

| 请求 | 时机 | VM 要什么 | VFS 做什么 |
|------|------|----------|-----------|
| `VMVFSREQ_FDLOOKUP`（101） | `mmap(fd)` 需要文件元数据 | dup 一个 VM 持有的 fd + dev/ino/size | `dupvm`（vfs/misc.c:399-416）+ 填 `VMV_DEV/INO/SIZE_PAGES/FD` |
| `VMVFSREQ_FDIO`（103） | 文件页缺页、缓存未命中 | 一页文件内容 | `actual_lseek(SEEK_SET)` + `actual_read_write_peek(PEEKING)`（vfs/misc.c:449-457）→ 页落入 VM 页缓存 |
| `VMVFSREQ_FDCLOSE`（102） | fdref 最后引用消失 | 关闭 VM 持有的 dup'd fd | `close_fd`（vfs/misc.c:433-441） |

关键洞察：**对话的目的不是传输数据本身，而是让 VFS 把页"放进"VM 的页缓存**。FDIO 的回复消息里没有页内容——VFS 用 `vm_map_cacheblock` 把块映射进 VM 缓存（minix3/minix/lib/libminixfs/cache.c:443-451），VM 收到回复后**重试缺页**，第二次命中缓存（§1.4）。

### 1.2 串行激活模型

C 的请求队列（vfs.c:33-41）是一条**单链表 + 单 active 槽**：

```
vfs_request()                          do_vfs_reply()
  ├─ 分配节点，头插 first_queued         ├─ 取 active（assert 存在）
  ├─ 若 !active → activate()            ├─ 校验 req_id == VMV_REQID
  │    └─ active = first_queued         ├─ 执行回调 req_callback(vmp, m, ...)
  │    └─ asynsend3(VFS, AMF_NOREPLY)   ├─ SLABFREE(节点)
  └─ 返回 OK                            └─ 若 first_queued && !active → activate()
```

两个性质值得强调：

1. **任意时刻最多一个在途请求**（`assert(!active)`，vfs.c:45）。VM 对 VFS 的请求全序列化。这不是性能缺陷而是正确性依赖——mem_file.c:126-130 的注释明确说：VMSF_ONCE 缓存页的"一次性使用"语义**依赖 VM 请求 VFS 完全串行**（并发请求下无法判断一个 ONCE 页是"上一次请求残留"还是"本次并发重复"）。
2. **回复路径不阻塞**：`do_vfs_reply` 返回 `SUSPEND`（vfs.c:141），主循环不对 VFS 的回复做普通回复；回调里才真正恢复被挂起的原调用（mmap 的 `mmap_file_cont` 或缺页的 `handle_memory_continue`）。

Rust 侧把这条结构搬进 `VfsRequestQueue`（§3.1），并把"队列状态"从 C 的全局变量变成显式对象。

### 1.3 fdref：文件描述符引用计数

`mmap(fd)` 之后，VM 持有的是 VFS 帮它 `dup` 的 fd（`dupvm`，vfs/misc.c:399-416）。这个 fd 的生命周期必须与**所有引用它的映射**绑定：任何一个映射还活着，fd 就不能关（否则缺页时 `actual_lseek` 会失败）。fdref.c 解决三件事：

1. **一个文件被多个映射引用，不需要多个 fd**：`fdref_dedup_or_new` 发现同 dev+ino 已有条目时复用（fdref.c:161-165），新传入的重复 fd 直接关闭（`mayclose` 时，fdref.c:166-171）。否则 fd 数量会随映射数量线性膨胀。
2. **区域分裂/复制时计数**：`mappedfile_split` 让两个子区域各 `fdref_ref` 一次（mem_file.c:257-258）；fork 复制区域时 `mappedfile_copy` 走 `mappedfile_setfile` 重新登记（mem_file.c:177-193）。引用计数防止"一半映射还活着就把 fd 关了"。
3. **最后引用消失时异步关 fd**：`fdref_deref` 在 refcount 归零时**无条件**发 `VMVFSREQ_FDCLOSE`（fdref.c:150-153）——**`mayclosefd` 只影响 dedup 路径**（要不要立刻关掉新发现的重复 fd），不影响最后引用的关闭。这是本轮 Rust 修复的语义要点（§3.2）。

### 1.4 文件后备缺页：缓存命中 vs VFS I/O

文件页缺页（`mappedfile_pagefault`，mem_file.c:84-155）是一条三岔路：

```
文件页缺页（未映射 slot）
  ├─ 查 VM 页缓存（byino / bydev，VMC_NO_INODE=0 走 bydev）
  │    ├─ 命中 → pb_unreferenced + pb_link（页直接映射）
  │    │        └─ 末页(clearend) 或 写 → cow_block（复制成私有 anon 页）
  │    │        └─ VMSF_ONCE → rmcache（一次性页用后即弃）
  │    └─ 未命中 → vfs_request(FDIO, procfd, vmp, referenced_offset, PAGE_SIZE, cb) 
  │                 → SUSPEND + *io=1
  └─ 已映射：读 → OK；写 → cow_block
```

`referenced_offset = region->param.file.offset + ph->offset`（mem_file.c:93/:107）——文件偏移 = 区域基偏移 + 页内偏移。FDIO 回复后 VM 重试缺页（`handle_memory_continue` → `handle_memory_step(TRUE)`，pagefaults.c:170-196），此时 VFS 已把页读进缓存，第二次走命中路径。**缓存是这条对话的"回程通道"**——没有缓存命中路径，重试会无限循环发 FDIO。

### 1.5 对照：Redox 与 Linux

- **Linux**：文件映射的页错误由内核的 `address_space_operations` 解决——`readpage`/`read_folio` 从后备设备读页进 page cache，`fault`/`map_pages` 把缓存页映射进进程地址空间。读页是**同步阻塞**（或 readahead 异步预读）；页缓存与地址空间同在内核，无需跨服务对话。Minix3 的等价结构把"address_space"拆成 VFS 侧的读路径 + VM 侧的缓存与映射，中间是异步 IPC——**这是用户态服务器架构的必然代价**，也换来 VM/VFS 各自的独立崩溃域与权限隔离。
- **Redox**：`syscall::mmap` 由内核 memory manager 处理；文件内容经内核 `Page` 缓存（`MMM`/`RedoxFS` 通过 scheme 调用传递），用户态 FS 与内核内存管理分离。与 Minix3 相似的是"文件系统在用户态、内存管理在另一侧"，但 Redox 的对话是**内核内**的 scheme 调用，Minix3 则是**两个服务器之间**的 IPC 消息——后者必须自己实现请求-回复协议（本文档主体）。
- **对照要点**：三家的共同语义是"**文件页以缓存为中转、映射与读文件分离**"；Minix3 的独特之处是**全序列化的异步对话 + VM 显式维护 fd 引用**（fdref）——Linux 用 `file->f_count` + `mmap` 持有引用，Redox 用 fd 表 + 内核计数，都不需要"跨进程显式通知关 fd"，而 Minix3 的 fd 属于 VFS、映射属于 VM，必须用 fdref 协议桥接。

### 1.6 小结

文件映射 = **异步对话协议**（FDLOOKUP/FDIO/FDCLOSE，串行激活）+ **fd 引用计数**（fdref，最后引用关 fd）+ **缓存中转**（缺页先查缓存、未命中才对话、回复后重试命中）。Rust 侧对应：`VfsRequestQueue`（队列）、`FdRefTable`（计数）、`MappedFile::ev_pagefault`（分流）。

---

## 2. C 源码分析

### 2.1 vfs.c：异步请求框架（全 143 行）

| 符号 | 行 | 语义 |
|------|-----|------|
| `STATELEN` | 31 | 回调状态拷贝上限 70 字节 |
| `struct vfs_request_node` | 33-41 | 节点：reqmsg + reqstate[70] + opaque + who + req_id + callback + next；全局 `first_queued`/`active` |
| `activate` | 43-53 | `assert(!active)` + `assert(first_queued)`；active = 队头；`asynsend3(VFS_PROC_NR, &reqmsg, AMF_NOREPLY)`（异步发送，不等待）；失败 panic |
| `ID_MAX` | 55 | `LONG_MAX`——reqid 上限（vfs.c 未实际使用，VFS 侧 `VMV_SIZE_PAGES` 用 LONG_MAX 表示"无限大"，vfs/misc.c:411） |
| `vfs_request` | 60-104 | 静态 `reqid` 递增；`SLABALLOC` 失败 → ENOMEM；填 `VFS_VMCALL` 消息（m10 布局，§2.2）；节点头插 `first_queued`；`if(!active) activate()` |
| `do_vfs_reply` | 109-142 | `assert(active)` + `assert(active->req_id == m->VMV_REQID)`（:120-121）；`vm_isokendpt(VMV_ENDPOINT)` 失败 → `vmp=NULL`（:124-126，进程已退出的信号）；取回调 → `active=NULL` → `req_callback(vmp, m, cbarg, reqstate)`（:133）→ SLABFREE → `if(first_queued && !active) activate()`（:137-139）→ 返回 SUSPEND |

注意两个"软"点，Rust 必须显式建模：

1. **回调可以重新激活队列**（`if(first_queued && !active)` 的 `!active` 检查）：回调内部可能又发了新请求（如缺页续作），此时 `active` 已非 NULL，不能覆盖。
2. **endpoint 已退出**：`vm_isokendpt` 失败传 `vmp=NULL`，回调必须容忍（`mmap_file_cont` 里 `if(vmp)` 之类；实际 mmap_file_cont 直接用 `vmp->vm_endpoint`——C 在这里对已退出进程是未定义行为，Rust 用 `VmProcTable` 查找失败返回错误）。

### 2.2 消息格式：VFS_VMCALL 与 VM_VFS_REPLY（m10 布局）

两个方向的消息都复用 `mess_10`（ipc.h:85-91）：

```c
typedef struct {
	u64_t m10ull1;                 /* offset 0  */
	int m10i1, m10i2, m10i3, m10i4;/* offset 8/12/16/20 */
	long m10l1, m10l2, m10l3;      /* offset 24/32/40 */
	uint8_t padding[20];           /* offset 48, 共 56 字节 */
} mess_10;
```

**`long` 在本 ABI 是 4 字节**——证据是 `_ASSERT_MSG_SIZE(mess_10)` 要求 `sizeof == 56`（ipcconst.h:17-18：`sizeof(msg_type) == 56 ? 1 : -1`）。8 + 4×4 + 3×4 + 20 = 56，若 long 为 8 字节则 68 字节编译不过。这正是 23-P0-1 wire-format 系列的对照基准（§3.4）。

| 消息 | 字段（com.h） | 偏移 |
|------|-------------|------|
| `VFS_VMCALL`（VM→VFS） | `VFS_VMCALL_OFFSET` = `m10_ull1`（:698）@0 / `VFS_VMCALL_REQ` = `m10_i1`（:694）@8 / `VFS_VMCALL_FD` = `m10_i2`（:695）@12 / `VFS_VMCALL_REQID` = `m10_i3`（:696）@16 / `VFS_VMCALL_ENDPOINT` = `m10_i4`（:697）@20 / `VFS_VMCALL_LENGTH` = `m10_l3`（:699）@40 | vfs.c:83-89 |
| `VM_VFS_REPLY`（VFS→VM） | `VMV_ENDPOINT` = `m10_i1`（:708）@8 / `VMV_RESULT` = `m10_i2`（:709）@12 / `VMV_REQID` = `m10_i3`（:710）@16 / `VMV_DEV` = `m10_i4`（:711）@20 / `VMV_INO` = `m10_l1`（:712）@24 / `VMV_FD` = `m10_l2`（:713）@28 / `VMV_SIZE_PAGES` = `m10_l3`（:714）@32 | vfs/misc.c 回复侧 |

请求码：`VMVFSREQ_FDLOOKUP` = 101 / `VMVFSREQ_FDCLOSE` = 102 / `VMVFSREQ_FDIO` = 103（com.h:701-704）。

**wire 陷阱**：`m10ull1` 占据 offset 0，意味着 VM_VFS_REPLY 的字段全部从 offset 8 起。若用 `mess_1`（m1 布局 m1i1@0 起）解码，所有 `VMV_*` 字段整体偏移 8 字节——旧 Rust `MessageM1::decode(m1)` 正是如此，导致 `VMV_DEV/INO/FD/SIZE_PAGES` 全部错位（22-P0-1/21-P1-1/19-P1-1/16-P0-1 同族，§3.4）。

### 2.3 fdref.c：fd 引用计数（全 177 行）

| 符号 | 行 | 语义 |
|------|-----|------|
| `fdref_sanitycheck` | 37-91 | SANITYCHECKS 门控：双重遍历检查重复 fd/重复 dev+ino；统计每个区域对 fdref 的引用数 == refcount（Rust 以 cfg feature 替代，A-7） |
| `fdref_new` | 93-107 | 分配节点：refcount=0，**头插** `fdrefs`（最近创建在前） |
| `fdref_ref` | 109-114 | `region->param.file.fdref = ref; ref->refcount++` |
| `fdref_deref` | 116-154 | `refcount--`；若 >0 返回；归零：链表摘除 + SLABFREE + **无条件** `vfs_request(VMVFSREQ_FDCLOSE, fd, region->parent, 0, 0, NULL, NULL, NULL, 0)`（:150-153，无回调——关闭失败只是诊断） |
| `fdref_dedup_or_new` | 156-176 | 从头扫描：同 dev+ino 且同 fd → 复用（:163-164）；同 dev+ino 不同 fd 且 `mayclose` → 发 FDCLOSE 关新 fd + 复用（:166-171）；不同 fd 且 `!mayclose` → 继续扫（:166 `if(!mayclose) continue;`）；无匹配 → `fdref_new` |

`mayclosefd` 的语义边界（fdref.c 注释 :15-18 + mmap.c:128 传参）：**它只决定"发现重复 fd 时是否立刻关掉新传入的 fd"**。调用方是谁决定了它的值：

- 用户 `mmap(fd)`：`mmap_file_cont` 传 `mayclosefd=1`（mmap.c:185）——用户持有的原始 fd 由用户自己管理，VM dup 出的重复 fd 可以立刻关。
- VFS 主动映射（`do_vfs_mmap`）：`mmap_file(..., mayclosefd=0)`（mmap.c:152-157）——fd 是 VFS 传下来的，VFS 自己负责，VM 不能关。

### 2.4 mem_file.c：文件映射 memtype（全 287 行）

**回调表**（:30-49）——15 个回调里的 11 个实现：

| 回调 | 行 | 语义 |
|------|-----|------|
| `ev_unreference` | 51-57 | `assert(refcount==0)`；非 MAP_NONE → `free_mem` |
| `ev_pagefault` | 84-155 | 三岔路分流（§1.4）；`procfd = region->param.file.fdref->fd`（:93） |
| `ev_sanitycheck` | 157-162 | 门控 |
| `writable` | 173-175 | **永远 0**——文件页从不直接可写，写权限由缺页时 CoW 决定 |
| `ev_copy` | 177-193 | fork 复制：`mappedfile_setfile(newvr->parent, newvr, fd, offset, dev, ino, clearend, 0, 0)`（prefill=0, mayclosefd=0）——**新进程用新 fd**（注释 fdref.c:19-21：源进程可能消失） |
| `ev_split` | 249-267 | 两个子区域各 `fdref_ref` 一次（:257-258）；`r1.clearend=0`；`r2.offset += r1->length`（:260-262） |
| `ev_lowshrink` | 273-278 | `vr->param.file.offset += len` |
| `ev_delete` | 280-287 | `fdref_deref(region)`（最后引用消失 → FDCLOSE）+ `inited=0` |
| `pt_flags` | 32-35（mappedfile_pt_flags） | ARM 才返回 CACHED；x86 返回 0 |

**`cow_block`**（:59-82）：`mem_cow(region, ph, MAP_NONE, MAP_NONE)` → `ph->memtype = &mem_type_anon`（文件页转匿名）→ 若 `clearend`（末页部分填充）：`sys_memset(NONE, 0, phaddr+VM_PAGE_SIZE-clearend, clearend)`（:71-80 尾清零）。

**`mappedfile_pagefault`**（:84-155）的完整分流（与 §1.4 一致，这里补 C 细节）：

```c
if(ph->ph->phys == MAP_NONE) {                       /* 全新页 */
	referenced_offset = region->param.file.offset + ph->offset;
	if(ino == VMC_NO_INODE) cp = find_cached_page_bydev(dev, referenced_offset, ...);
	else                     cp = find_cached_page_byino(dev, ino, referenced_offset, 1);
	if(cp && (!cb || !(cp->flags & VMSF_ONCE))) {   /* 缓存命中 */
		pb_unreferenced(region, ph, 0);
		pb_link(ph, cp->page, ph->offset, region);
		if(roundup(ph->offset+clearend, PAGE) >= region->length) result = cow_block(...);
		else if(result == OK && write)              result = cow_block(..., 0);
		if(result == OK && (cp->flags & VMSF_ONCE)) rmcache(cp);   /* 一次性页用后即弃 */
		return result;
	}
	if(!cb) return EFAULT;                           /* 无回调：无法续作 */
	if(vfs_request(VMVFSREQ_FDIO, procfd, vmp, referenced_offset,
		VM_PAGE_SIZE, cb, NULL, state, statelen) != OK) return ENOMEM;
	*io = 1;                                         /* 标记 io 挂起 */
	return SUSPEND;
}
if(!write) return OK;                                /* 已映射读 → OK */
return cow_block(vmp, region, ph, 0);                /* 已映射写 → CoW */
```

**`mappedfile_setfile`**（:195-247）：`fdref_dedup_or_new`（:203）→ `assert(!inited)` → `fdref_ref(newref, region)`（:207）→ 填 offset/clearend/inited=1 → 可选 `prefill`（:215-239）：逐页查缓存，命中且非 ONCE → `pb_reference` + `map_ph_writept` 预映射（`roundup(vaddr+clearend, PAGE) >= length` 时 break——末页不预映射，留给缺页 CoW）。

### 2.5 mmap.c 调用面

| 符号 | 行 | 语义 |
|------|-----|------|
| `do_mmap`（文件分支） | 254-273 | 非 anon：`enable_filemap` 检查（:256，关则 ENXIO）；共享+写拒绝（:261-263，ENXIO）；`vfs_request(VMVFSREQ_FDLOOKUP, fd, vmp, 0, 0, mmap_file_cont, NULL, m, sizeof(*m))`（:265-266）→ 失败 ENXIO → **SUSPEND**（:272，不回复，等 mmap_file_cont） |
| `mmap_file` | 84-133 | 页对齐（:97-101 把 file_offset 向下取整、len 补上余量）；`mmap_region(..., &mem_type_mappedfile, 0)`（:121）；成功 → `mappedfile_setfile(..., prefill=1, mayclosefd)`（:128-129）；`retaddr = vr->vaddr + page_offset`（:125） |
| `mmap_file_cont` | 160-195 | FDLOOKUP 回复回调：`writable = origmsg->m_mmap.prot & PROT_WRITE`（:169-170）；`VMV_RESULT != OK` → result=errno（:172-177）；否则 `mmap_file(vmp, VMV_FD, offset, flags, VMV_INO, VMV_DEV, VMV_SIZE_PAGES*PAGE_SIZE, addr, len, &v, 0, writable, 1)`（:180-185，mayclosefd=1）；最后 `ipc_send` 解阻塞原进程（:188-194） |
| `do_vfs_mmap` | 135-158 | VFS 主动映射（exec 加载/共享库）：直接 `mmap_file(..., mayclosefd=0)`（§2.3） |

### 2.6 pagefaults.c 续作：缺页侧的回复回调

| 符号 | 行 | 语义 |
|------|-----|------|
| `pf_cont` | 161-168 | 普通（非 handle_memory）路径的 VFS 回调：`vm_isokendpt` 失败返回（进程已死，信号）；否则重入 `handle_pagefault(ep, vaddr, err, 1)` |
| `handle_memory_continue` | 170-196 | **FDIO 回复回调**：`m->VMV_RESULT != OK` → `handle_memory_final(state, errno)`；否则 `handle_memory_step(TRUE /*retry*/)`（重试缺页——第二次命中缓存）；重试仍 SUSPEND → 返回（继续等下一个回复）；否则 `handle_memory_final(state, r)` |
| `handle_memory_final` | 198-235 | 收尾：KERNEL 调用方 → `sys_vmctl(VMCTL_MEMREQ_REPLY)`；其他 → asynsend3 带 errno（VFS transid 时 AMF_NOREPLY + TRNS_ADD_ID）；`memset(state, 0)` 防复用 |
| `handle_memory_step` | 336+ | 逐页处理；关键防循环：文件映射且 `(!hmstate->vfs_avail || retry)` 时以 **NULL 回调**调 `map_handle_memory`（:394-400 附近）——**第二次不叫 VFS**，缓存未命中则 EFAULT，防 FS 出错时无限循环 |

### 2.7 VFS 侧 do_vm_call（vfs/misc.c:383-473）

VFS 收到 `VFS_VMCALL` 后按 `VFS_VMCALL_REQ` 分派：

- `VMVFSREQ_FDLOOKUP`（:399-421）：`dupvm(rfp, req_fd, &procfd, &f)`（dup 一份 fd 到 VM 进程）；块设备 → `VMV_DEV = v_sdev` + `VMV_INO = VMC_NO_INODE` + `VMV_SIZE_PAGES = LONG_MAX`；普通文件 → `VMV_DEV/INO` + `VMV_SIZE_PAGES = roundup(v_size, PAGE)/PAGE`；`VMV_FD = procfd`。
- `VMVFSREQ_FDCLOSE`（:433-441）：`close_fd(fp, req_fd, FALSE /*may_suspend*/)`；失败打印诊断（fdref.c:146-149 说"close 失败无法处理"）。
- `VMVFSREQ_FDIO`（:449-457）：`actual_lseek(fp, req_fd, SEEK_SET, offset)` → `actual_read_write_peek(fp, PEEKING, req_fd, 0, length)`——**PEEKING 读**（见下）。
- 回复：固定 `m_type = VM_VFS_REPLY` + `VMV_*` 字段 + `asynsend3(VM_PROC_NR, ..., 0)` + 返回 `SUSPEND`（:461-473）。

**PEEK 语义**（minix3/minix/lib/libminixfs/cache.c）：`lmfs_get_block_ino(..., PEEKING)`——FS 读文件页时先查自己缓存（:343-377），未命中则试 `vm_map_cacheblock(dev, dev_off, ino, ino_off, ...)`（:443-451）**从 VM 缓存要页**；VM 也没有 → `PEEK` 返回 `ENOENT`（:459-465）且不分配数据页。整个 FDIO 的闭环是：VM 缓存未命中 → FDIO → VFS lseek+peek 读盘 → **页经 vm_map_cacheblock 写进 VM 缓存** → VM 重试命中。Rust 侧"重试命中"由 `MappedFile::ev_pagefault` 的缓存查找实现（§3.5）。

### 2.8 C 小结：符号全景

```
对话协议（vfs.c）      fdref（fdref.c）       mem_file（mem_file.c）        调用方
vfs_request ──────────► fdref_dedup_or_new ─► mappedfile_setfile ◄── mmap_file（mmap.c:128）
  │                       │                      │
  │                       └─ fdref_ref ──────────┘（split :257 / copy :186）
  │                       └─ fdref_deref ───────► mappedfile_delete（mem_file.c:285）
  │                             └─ FDCLOSE 请求
  ├─ FDLOOKUP ────────────────────────────────► mmap_file_cont（mmap.c:160）
  ├─ FDIO ◄──────────────────────────────────── mappedfile_pagefault（mem_file.c:148）
  │     └─ 回复 → handle_memory_continue（pagefaults.c:170）→ 重试 → 缓存命中
  └─ do_vfs_reply（vfs.c:109）── 回调分派
```

**明确排除/移交**：`fdref_sanitycheck`（A-7 cfg feature 替代，plan.md §5.4）；`mappedfile_sanitycheck`（同上）；页缓存查找函数本体（cache.c，24）；`mem_type_mappedfile.pt_flags` ARM 分支（x86 返回 0，标注即可）。

---

## 3. Rust 设计决策

### 3.1 D1：VfsRequestQueue —— 串行激活模型（类型化替代全局 vfs_rq）

**C**：全局 `first_queued`/`active` 单链表（vfs.c:33-41）。
**Rust**：`VfsRequestQueue { queued: VecDeque<VfsRequest>, active: Option<VfsRequest>, next_id: u32, max_queued: usize }`（vfs_queue.rs:90-95）。

- `request()`（:107-124）：`QueueFull` 检查（`max_queued=64`，C 无上限——Rust 防御性上限，映射 ENOMEM）→ 分配 `req_id`（wrapping_add）→ 入队 → 无 active 时 `activate()`。
- `activate()`（:118-124）：队首移入 active。**注意：不真正发送 IPC**——C 的 `asynsend3`（vfs.c:51）在 Rust 侧没有对应物（`IpcSender` trait 已移除，vfs_queue.rs:86-88 注释）。这是 transport 缺口（§3.6）。
- `handle_reply()`（:126-148）：`active.take()`（无 active → `NoActiveRequest`）→ `req_id` 不匹配 → 还原 + `UnexpectedReply`（C 是 `assert`，Rust 返回错误）→ 取 `(callback, state)` → 队非空则自动激活下一个（对应 vfs.c:137-139）。
- **结构收益**：C 的链表头插 = 最近请求在队首（LIFO 语义）；Rust `VecDeque` 是 FIFO。**语义差异**：C 回复后取 `first_queued`（最后插入的），Rust 取最先插入的。单请求场景下不可观察（VM 串行化后同时只有一个请求在途，排队期间第二个请求到达的窗口内顺序确实不同），文档如实标注（§3.6 差异 4）。
- 回调经 `VfsCallbackFn = fn(&mut VmServer, &VfsReply, &VfsRequestState) -> Result<(), VfsQueueError>`（:46-51）——函数指针 + 显式状态枚举，替代 C 的 `(vmp, m, cbarg, reqstate)` 四元组（§4.1）。

### 3.2 D2：fdref 显式引用计数（fdref_id + PendingFdClose 返回）

**C**：fdref 对象链表 + 区域 `param.file.fdref` 指针（fdref.c:35）。
**Rust**：`FdRefTable`（`BTreeMap<u32, FdRefEntry>` + `dev_ino_index` 反索引 + `next_id`，fdref.rs:44-48）+ 区域 `VrParam::File.fdref_id: Option<u32>`（region/vir_region.rs:48）。

- `create(fd, dev, ino)`（:78-96）：refcount 0（= C `fdref_new`）；登记反索引。
- `dedup_or_new(fd, dev, ino, may_close) -> (u32, Option<PendingFdClose>)`（:114-149）：**完整对齐 C**（fdref.c:161-177）——
  - 同 dev+ino 同 fd → 复用 `(id, None)`；
  - 同 dev+ino 不同 fd 且 `may_close` → `(id, Some(PendingFdClose{fd,dev,ino}))`（调用方入队 FDCLOSE）；
  - `may_close=false` → 继续扫描精确 fd 匹配；
  - 无匹配 → `create`。
  - **扫描顺序**：`entries.iter().rev()`（BTreeMap 反向）≈ C 链表头插的最近优先。
- `ref_entry(id)`（:151-155）：refcount +1（= C `fdref_ref`）。
- `deref_entry(id) -> Option<PendingFdClose>`（:157-192）：refcount--；**归零总是返回 close**（对应 C fdref.c:150-153 无条件 FDCLOSE）+ 清理反索引。
- `get(id)`（:204-206）：复制条目（避免 UnsafeCell 别名问题）。

**本轮语义修复（23-P0-1 系列 / fdref 语义）**：旧实现给 `FdRefEntry` 加了 `may_close` 字段，`deref_entry` 在 `!may_close` 时**不**返回 close——这违反了 C"最后引用总是关 fd"（fdref.c:150-153）。`mayclosefd` 只作用于 dedup 路径（§2.3）。修复：删除 entry 上的 `may_close`，`deref_entry` 无条件返回 `PendingFdClose`（fdref.rs:180-189 注释）。

**为什么不用 Arc/Rc Drop**：`refcount==0` 时必须发异步 FDCLOSE（需要 `VfsRequestQueue`），`Drop` 拿不到队列——显式 `deref_entry` 返回 `PendingFdClose` 由调用方入队（fdref.rs:8-14 注释）。这与退出路径（22 篇）的 `free_process_phys` 暂存 close 的模型一致。

### 3.3 D3：NeedVfsIo —— 决策与执行分离

**C**：`mappedfile_pagefault` 内部做完"查缓存 → 决定 → 发请求"（mem_file.c:102-155）。
**Rust**：决策与执行分层——`MappedFile::ev_pagefault`（memtype.rs:954）只**决定**动作，`handle_pagefault`（cow_exec_pf.rs:23）**执行**：

```
ev_pagefault（决策）                    handle_pagefault（执行）
  未初始化 → NeedNewPage                  NeedNewPage → alloc_and_map
  已映射写 → NeedCow                       NeedCow     → cow_resolve
  已映射读 → Handled                       Handled     → Handled
  未映射 → 缓存命中？                      NeedVfsIo   → enqueue_fdio → Suspended
     ├─ 命中 → 映射缓存页（写/末页→NeedCow）
     └─ 未命中 → NeedVfsIo
```

- 缓存查找在 `ev_pagefault` 内（对齐 C 的结构——memtype 回调拥有"这个类型的页从哪来"的知识）：`fdref.ino == VMC_NO_INODE` 走 `find_by_device`，否则 `find_by_inode`（memtype.rs:1002-1008）；命中 → `increase_refcount` + `map_page`（= C pb_link）+ 写/末页 → `NeedCow`（= cow_block 语义，clearend 清零见 §3.6 差异 3）；未命中 → `NeedVfsIo`。
- `enqueue_fdio`（cow_exec_pf.rs:67-112）：从 `VrParam::File` 取 `fdref_id` → `FdRefTable::get` 得 fd；`referenced_offset = file_offset + offset`；构造 `VfsRequest { FdIo, fd, offset, length: PAGE_SIZE, callback: mappedfile_pf_cont, state: FdIo{region_vaddr, page_offset, write, caller_endpoint} }` 入队；失败 → `CowError::NoMemory`（C: ENOMEM，mem_file.c:151）。
- `mappedfile_pf_cont`（cow_exec_pf.rs:114-167）：回复 OK → 重定位区域 → 重试 `handle_pagefault`（第二次命中缓存，循环终止）；回复错误 → 交付 errno（transport 缺口，§3.6 差异 1）；重试仍 Suspended → 继续等下一个回复（对齐 `handle_memory_continue` 的 `if(r == SUSPEND) return;`，pagefaults.c:189-191）。

### 3.4 D4：wire format 修复（MessVmVfsReply overlay，23-P0-1 系列）

**根因**（§2.2）：`VM_VFS_REPLY` 是 `mess_10` 布局（com.h:707-714 + ipc.h:85-91），旧 Rust 用 `MessageM1::decode(m1)` 解码，所有字段偏移 8 字节错位——22-P0-1/21-P1-1/19-P1-1/16-P0-1 同族（每个都用错 overlay 解过 m10 消息）。

**修复**：`MessVmVfsReply`（repr(C)，message.rs:1874-1896）：`ull1:u64@0` / `endpoint:i32@8` / `result:i32@12` / `reqid:i32@16` / `dev:i32@20` / `ino:u32@24` / `fd:u32@28` / `size_pages:u32@32` / `_padding:[u8;20]`；`MessageUnion.m_vm_vfs_reply`（message.rs:150）；`VmVfsReplyIn::decode_message`（vm.rs:586）从 overlay 读取；删除错误的 `DecodeFromM1` impl。dispatcher 的 VM_VFS_REPLY 分支改走 `decode_message`（ipc/dispatcher.rs:1187-1193），`dispatch_vfs_reply` 把 `request.ino` 传入 `VfsReply.ino`（原来是硬编码 0——`mmap_file_cont` 的 fdref dedup 需要 ino，mmap.rs:583）。

**为什么 ino 必须进 reply**：`mmap_file_cont` → `mmap_file` → `dedup_or_new(fd, dev, ino, ...)` 的复用判断靠 (dev, ino)（§2.3）。ino 错位为 0 会导致 dedup 全部失效（每次 mmap 都新建 fdref + FDCLOSE 风暴）。

### 3.5 D5：缓存命中 vs FDIO（mappedfile_pagefault 的 Rust 对应）

`MappedFile::ev_pagefault`（memtype.rs:954-1043）完整实现 §1.4 的三岔路：

- **未初始化** → `NeedNewPage`（C 的 `assert(region->param.file.inited)` 前置，mem_file.c:96-99 断言 inited；未 inited 是 mmap 未完成前的病态访问，Rust 防御性返回）。
- **已映射**：读 → Handled；写 → NeedCow（C: `return cow_block(...)`）。
- **未映射 + 缓存命中**：`cache.increase_refcount(&key)` + `region.map_page(frames, offset, pfn, &MEM_TYPE_MAPPED_FILE)`（= C `pb_unreferenced + pb_link`，mem_file.c:118-120）；写或末页（`roundup(offset+clearend, PAGE) >= length`，C :124-126）→ `NeedCow`（`cow_resolve_core` 做 mem_cow + anon 转换，cow_exec_pf.rs:200-230）；否则 Handled。
- **未映射 + 缓存未命中** → `NeedVfsIo` → §3.3 的 FDIO 接线。

**VMSF_ONCE 与 rmcache**：C 在命中 ONCE 页后 `rmcache`（mem_file.c:133-135）；Rust `PageCacheEntry` 只有 `{pfn, refcount}`（page_cache.rs:34-38），未建模 flags——ONCE 语义整体移交 24（§3.6 差异 2）。

### 3.6 差异清单（C ↔ Rust，诚实标注）

| # | 差异 | 级别 | 标注 |
|---|------|------|------|
| 1 | **transport 缺口**：`VfsRequestQueue::activate` 只移动 active 槽，不发送 `VFS_VMCALL`（IpcSender 已移除）；`mappedfile_pf_cont`/`mmap_file_cont` 的回复交付（`ipc_send` / `sys_vmctl(VMCTL_CLEAR_PAGEFAULT)` / asynsend3）未接线 | P1 | vfs_queue.rs:86-88、mmap.rs:534-538、cow_exec_pf.rs:127-131/:158-164 注释 |
| 2 | **VMSF_ONCE/rmcache 未建模**：PageCacheEntry 无 flags；ONCE 一次性页语义移交 24 | P1（backlog） | page_cache.rs:37-39 |
| 3 | **clearend 尾清零未建模**：`cow_block` 的 `sys_memset`（mem_file.c:72-79）在 Rust 侧无对应；末页 CoW 会拷贝整页含尾部脏数据 | P1（backlog） | memtype.rs:1052-1059 注释 |
| 4 | **队列序差异**：C 链表头插 = LIFO 激活；Rust VecDeque = FIFO。单请求串行下不可观察 | P2 | vfs_queue.rs:107-124 |
| 5 | **无回调缺页路径**：C 的 `if(!cb) return EFAULT`（mem_file.c:143-146）在 Rust 无对应（当前无无回调缺页入口） | P2（文档化） | memtype.rs 注释 |
| 6 | **队列上限**：C 无上限（SLABALLOC 失败才 ENOMEM）；Rust `max_queued=64` → QueueFull → ENOMEM | P2 | vfs_queue.rs:103-109 |
| 7 | **`vm_isokendpt` 失败**：C 传 `vmp=NULL` 给回调（vfs.c:124-126），回调可能解引用 NULL（未定义行为）；Rust 查找失败返回错误 | P2（收紧） | cow_exec_pf.rs:135-136 |

---

## 4. 实现详解

### 4.1 模块结构

```
os/servers/vm/src/
├── vfs_queue.rs        VFS 请求队列（协议状态机）
│   ├── VfsRequestType    {FdLookup, FdIo, FdClose}（对齐 com.h:701-704）
│   ├── VfsRequestState   {FdLookup{mmap}, FdIo{region_vaddr,page_offset,write,caller_endpoint}, FdClose{fd}}
│   ├── VfsCallbackFn     回调签名 fn(&mut VmServer, &VfsReply, &VfsRequestState)
│   ├── VfsRequest        请求（type/req_id/caller/fd/offset/length/callback/state）
│   ├── VfsReply          回复（req_id/result/data_phys/fd/dev/ino/size_pages）
│   └── VfsRequestQueue   串行激活队列（§3.1）
├── fdref.rs            fd 引用计数表（§3.2）
├── mmap.rs             文件映射入口（FDLOOKUP 路径）
│   ├── handle_mmap        do_mmap 文件分支：FDLOOKUP 入队 → Suspended（mmap.rs:359-374）
│   ├── mmap_file          完成映射 + mappedfile_setfile 等价（fdref + File param）
│   ├── handle_vfs_mmap     do_vfs_mmap（mayclosefd=0）
│   └── mmap_file_cont      FDLOOKUP 回复回调 → mmap_file → 解阻塞（transport 缺口）
├── memtype.rs          MappedFile memtype（§3.3/§3.5）
├── cow_exec_pf.rs      缺页执行器（NeedVfsIo → enqueue_fdio → mappedfile_pf_cont）
└── ipc/dispatcher.rs   VM_VFS_REPLY 分支（:1187-1193）→ dispatch_vfs_reply（:337）
```

### 4.2 关键流程伪码

**mmap(文件) 路径**（对照 C do_mmap → FDLOOKUP → mmap_file_cont）：

```
handle_mmap（mmap.rs:268）
  ├─ 非 anon + 文件映射使能 + 非共享写（对齐 mmap.c:254-263）
  ├─ VfsRequest{FdLookup, fd, callback: mmap_file_cont,
  │            state: FdLookup{mmap: 原请求}} 入队（mmap.rs:359-374）
  └─ Ok(Suspended)                       ← 主循环不回复
        ...
  VM_VFS_REPLY（ipc/dispatcher.rs:1187-1193）
  └─ VmVfsReplyIn::decode_message → dispatch_vfs_reply（:337）
       └─ VfsRequestQueue::handle_reply → Ok(Some((mmap_file_cont, reply, state)))
            └─ 主循环执行回调（vm_server.rs:632-634）
                 └─ mmap_file_cont（mmap.rs:540）
                      ├─ reply.result != OK → 携带 errno（回复 transport 缺口）
                      ├─ mmap_file（mmap.rs:416）
                      │    ├─ 页对齐 + mmap_region
                      │    ├─ dedup_or_new(fd, dev, ino, mayclosefd=true)
                      │    │    └─ Some(close) → FdClose 入队（mmap.rs:454-467）
                      │    ├─ ref_entry + VrParam::File{inited, fdref_id, offset, clearend}
                      │    └─ 区域插入
                      └─ 解阻塞原进程（ipc_send，transport 缺口）
```

**文件页缺页路径**（对照 C do_pagefaults → mappedfile_pagefault → handle_memory_continue）：

```
dispatch_pagefault（vm_server.rs:743）
  └─ handle_pagefault（cow_exec_pf.rs:23）
       ├─ MappedFile::ev_pagefault（memtype.rs:954）
       │    ├─ 缓存命中 → map_page + （写/末页 → NeedCow）
       │    └─ 缓存未命中 → NeedVfsIo
       └─ NeedVfsIo → enqueue_fdio（cow_exec_pf.rs:67）
            ├─ VfsRequest{FdIo, fd: fdref.fd, offset: file_offset+page_offset,
            │             length: PAGE_SIZE, callback: mappedfile_pf_cont,
            │             state: FdIo{region_vaddr, page_offset, write, caller}}
            └─ Ok(Suspended)
        ...
  VM_VFS_REPLY → dispatch_vfs_reply → handle_reply → mappedfile_pf_cont（cow_exec_pf.rs:114）
       ├─ reply.result != OK → 交付 errno（transport 缺口）
       ├─ 重试 handle_pagefault（第二次：VFS 已把页读进缓存 → 命中 → Handled）
       ├─ 仍 Suspended → 继续等（多页/连续未命中）
       └─ 完成 → 解阻塞（sys_vmctl / asynsend3，transport 缺口）
```

**munmap/exit 路径的 fdref 兑现**（22 篇的移交接口）：`munmap_vm_lin`/`free_process_phys` 在 `VrParam::File` 区域上调用 `fdref.deref_entry(id)` → `Some(PendingFdClose)` → `VfsRequest{FdClose, fd}` 入队（fdref.rs:157-192 + munmap.rs 接线，21 篇已修）。

### 4.3 本轮修复记录（2026-08-16）

| ID | 级别 | 内容 |
|----|------|------|
| 23-P0-1 | P0 | wire format：`VM_VFS_REPLY` 从 m1 错位解码改为 `MessVmVfsReply` overlay（message.rs:1874 + vm.rs:586 + ipc/dispatcher.rs:1187-1193）；`dispatch_vfs_reply` 把 `request.ino` 传入 `VfsReply.ino`（原硬编码 0） |
| 23-P0-1b | P0 | fdref 语义：删除 `FdRefEntry.may_close`；`deref_entry` refcount==0 无条件返回 `PendingFdClose`（对齐 fdref.c:150-153） |
| 23-P0-1c | P0 | fdref dedup 语义完整对齐：`dedup_or_new(fd, dev, ino, may_close)` 四态（§3.2），删除旧 `create` 的 may_close 参数 |
| 23-P1-1 | P1 | `MappedFile::ev_pagefault` 缓存命中路径（find_byino/bydev + increase_refcount + map_page + 写/末页 NeedCow）；`VMC_NO_INODE` 常量（page_cache.rs:25） |
| 23-P1-2 | P1 | 缺页 FDIO 接线：`handle_pagefault` 增 `cache`/`vfs_queue` 参数；`enqueue_fdio` + `mappedfile_pf_cont`（cow_exec_pf.rs:67/:114）；`dispatch_pagefault` 透传（vm_server.rs:763-766） |
| 23-P2-1 | P2 | 测试：新增 11 个（fdref dedup 5 + memtype mapped 4 + cow_exec_pf 2）；测试总数 395 → 406（§5.4） |

### 4.4 与 15/16/20/24 的关系

- **15-ipc-dispatch**：VM_VFS_REPLY 是主循环分发的一个分支（ipc/dispatcher.rs:1187-1193）；SUSPEND 语义（不回复、等续作）在这里兑现；transid 路由（22 篇 §1.4）的 `handle_memory_final` 侧（TRNS_ADD_ID + AMF_NOREPLY）在 transport 落地后接入。
- **16-pagefault**：`handle_pagefault` 是本篇 FDIO 接线的宿主；`PagefaultAction::Suspended` 由 16 篇的状态机消费。
- **20-vm-mmap**：`mmap_file`/`mmap_file_cont` 的 FDLOOKUP 路径是 20 篇"文件映射"入口的续作；本篇补全其异步协议侧。
- **24-page-cache**：本篇只消费 `PageCache` 的 `find_by_*`/`increase_refcount`；缓存内部（双哈希/LRU/VMSF_ONCE/mapcache 等）全部移交 24。

---

## 5. 测试要点

### 5.1 单元测试清单（grep 实证，2026-08-16）

**vfs_queue.rs**（5 个）：

| 测试 | 行 | 覆盖 |
|------|-----|------|
| `test_vfs_queue_request_activate` | 180 | 首个请求立即激活 |
| `test_vfs_queue_serial_activation` | 200 | 第二个请求排队、激活序 |
| `test_vfs_queue_handle_reply` | 235 | 回复时队列状态 |
| `test_vfs_queue_no_active_reply` | 280 | 空队列错误路径 |
| `test_vfs_request_types` | 299 | 请求类型枚举判别 |

**fdref.rs**（13 个，其中 dedup 5 个本轮新增）：

| 测试 | 行 | 覆盖 |
|------|-----|------|
| `test_fdref_create_and_get` | 226 | create/get |
| `test_fdref_ref_deref_cycle` | 238 | ref/deref 往返 |
| `test_fdref_deref_always_closes_at_zero` | 261 | **归零总是 close**（23-P0-1b） |
| `test_fdref_dedup` | 274 | 复用语义 |
| `test_fdref_invalid_id` | 286 | 无效 id |
| `test_find_by_dev_ino_o1_lookup` | 298 | 反索引查找 |
| `test_find_by_dev_ino_clears_on_deref_to_zero` | 313 | 反索引清理 |
| `test_find_by_dev_ino_index_survives_collision` | 329 | 碰撞防御 |
| `test_dedup_or_new_same_fd_reuses` | 349 | 同 fd 复用 |
| `test_dedup_or_new_different_fd_may_close_closes_new_fd` | 361 | may_close 关新 fd |
| `test_dedup_or_new_different_fd_no_may_close_scans_for_exact_fd` | 375 | !may_close 继续扫描 |
| `test_dedup_or_new_no_exact_fd_creates` | 390 | 扫描后创建 |
| `test_dedup_or_new_no_match_creates` | 402 | 无匹配创建 |

**mmap.rs**（19 个，与 20 篇共享基线：15 个匿名/常规 + 4 个 vfs_mmap；20 篇 §5.1 的 mmap.rs 测试行号为 23 轮前旧值，见 backlog 23-B3）：`test_mmap_file_enqueues_vfs_request`（:959，FDLOOKUP 入队断言）为本篇核心；`test_vfs_mmap_*`（:987-1091）覆盖 do_vfs_mmap 路径（mayclosefd=0 语义）。

**cow_exec_pf.rs**（7 个，2 个本轮新增）：

| 测试 | 行 | 覆盖 |
|------|-----|------|
| `test_handle_pagefault_need_vfs_io_enqueues_fdio` | 495 | FDIO 请求构造（fd/offset/length/callback/state 全字段断言） |
| `test_handle_pagefault_retry_cache_hit_no_fdio_loop` | 537 | 回复→重试→缓存命中→无第二次 FDIO（循环终止性） |

**memtype.rs**（MappedFile 4 个本轮新增）：`test_mapped_file_pagefault_uninitialized_need_new_page`（:1231）/ `..._cache_miss_need_vfs_io`（:1266）/ `..._cache_hit_links_page`（:1296）/ `..._device_cache_hit`（:1340，VMC_NO_INODE 走 bydev）。

### 5.2 覆盖维度

- **队列状态机**：激活/排队/回复/无 active/req_id 不匹配（vfs_queue 5 个）。
- **fdref 语义**：ref/deref 平衡、归零关闭、dedup 四态（fdref 13 个）。
- **缺页分流**：缓存命中/未命中/设备页/未初始化（memtype 4 个）。
- **FDIO 接线**：请求构造字段 + 重试循环终止（cow_exec_pf 2 个）。
- **mmap 文件路径**：FDLOOKUP 入队 + 回复回调（mmap 19 个）。

### 5.3 覆盖缺口与诚实标注

| 缺口 | 状态 |
|------|------|
| `mappedfile_pf_cont` 回调级集成测试（需构造 VmServer + 真实进程槽 + 回复驱动） | 未覆盖（单测只到 handle_pagefault 层）；诚实标注——回调本体是"重试 handle_pagefault + transport 缺口交付"，其核心逻辑已被 `test_handle_pagefault_retry_cache_hit_no_fdio_loop` 覆盖 |
| VMSF_ONCE/rmcache（24 范围） | 未覆盖（PageCacheEntry 无 flags） |
| clearend 尾清零 | 未覆盖（backlog） |
| FDCLOSE 发送端到端（dedup 返回 close → 入队） | 部分覆盖（mmap.rs:454-467 接线，无端到端测试） |
| transport（VFS_VMCALL 发送 / 回复交付） | 未覆盖（IpcSender 已移除，§3.6 差异 1） |
| 20-vm-mmap.md §5.1 mmap.rs 测试行号陈旧（23 轮 mmap.rs 改动后偏移，19 个测试当前行号见本表） | backlog 23-B3：20 篇行号待 20 回归轮统一修正 |

### 5.4 测试统计（截至 2026-08-16）

- `cargo test -p minix-vm --lib`：**406 passed / 1 failed**（`test_map_lazy` pre-existing，13 篇 backlog；本节与本篇无直接关系）
- `cargo test -p minix-types`：**85 passed**
- 本轮新增：fdref 5 + memtype 4 + cow_exec_pf 2 = 11 个；测试总数 395 → 406
- 完整测试清单：`rg "^\s*fn test_" os/servers/vm/src/{vfs_queue,fdref,mmap,cow_exec_pf,memtype}.rs`

---

## 6. 过渡

### 6.1 位置可回答性

本文档的机制在主循环的**两个位置**被消费：

1. **P4 分发（普通 VM 调用）**：`handle_mmap` 文件分支入队 FDLOOKUP 后返回 `MmapResult::Suspended` → `DispatchAction::Suspend`（主循环不回复，vm_server.rs:636-637）。
2. **VM_VFS_REPLY 分支（ipc/dispatcher.rs:1187-1193）**：VFS 回复到达 → `dispatch_vfs_reply` → `handle_reply` → 回调（`mmap_file_cont` / `mappedfile_pf_cont`）在主循环执行（vm_server.rs:632-634 `result.vfs_callback`）。
3. **缺页分支（P3）**：`dispatch_pagefault`（vm_server.rs:743）→ `handle_pagefault` → 文件页未命中 → FDIO 入队 → `DispatchAction::NoReply`（进程保持挂起）。

即：**请求从 P4/P3 入口入队，回复从 VM_VFS_REPLY 分支消费**——这正是 C 主循环里 `do_vfs_reply` 的位置（main.c:150 附近）。

### 6.2 下游移交（对照 plan.md §3.4）

- **24-page-cache**：`MappedFile::ev_pagefault` 消费的 `find_by_ino/bydev`、`increase_refcount` 只是 PageCache 的查找/计数面；缓存内部结构（双哈希、LRU、VMSF_ONCE、mapcache/setcache/forgetcache/clearcache 4 个 IPC handler）全部移交 24。本文档的 VMSF_ONCE 与 rmcache 缺失（§3.6 差异 2）在 24 闭环。
- **transport 落地**：VFS_VMCALL 发送 + 回复交付（`ipc_send`/`sys_vmctl`/`asynsend3`）依赖 IPC transport 层（15 篇范围）；落地后移除 §3.6 差异 1 的所有"transport 缺口"标注。
- **clearend 尾清零**（§3.6 差异 3）：cow_block 的 `sys_memset` 语义，随 24 的 ONCE/缓存页生命周期一起补。

---

## 7. 参见

- `12-memtype.md` §回调体系——memtype 15 回调签名与语义（本文档的消费面）
- `13-region-mapping.md` §区域结构——`VrParam::File` 的宿主
- `15-ipc-dispatch.md` §SUSPEND / §主循环——回复语义与 VM_VFS_REPLY 分支
- `16-pagefault.md` —— `handle_pagefault` 状态机（NeedVfsIo 的消费方）
- `17-cow-mechanism.md` §mem_cow——`cow_block`/`cow_resolve_core` 的分裂机制本体
- `20-vm-mmap.md` —— mmap 区域建立（FDLOOKUP 路径的前置）
- `21-vm-munmap.md` §fdref 平衡——munmap 侧 `deref_entry` 兑现
- `22-vm-exit.md` §释放链——退出路径的 fdref 暂存（23 的 FDCLOSE 入口之一）
- `24-page-cache.md` —— 页缓存（本文档的缓存命中路径的完整实现，下一篇）
- `99-global-concepts.md` —— endpoint/常量表
- 素材：`notes/rewrite/fork-syscall-rewrite/02-stage-vm/draft/23-vfs-interaction.md`（1494 行，历史设计素材）
