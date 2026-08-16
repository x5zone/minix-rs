# 20-vm-mmap: VM_MMAP / VFS_MMAP / REMAP —— 地址空间分配与内存来源绑定

> **分类**: 阶段 7 — IPC 服务（进程生命周期）
> **源码**: `minix3/minix/servers/vm/mmap.c`（573 行：`mmap_region` :36-83 / `mmap_file` :84-132 / `do_vfs_mmap` :135-158 / `mmap_file_cont` :160-190 / `do_mmap` :200-269 / `map_perm_check` :284-307 / `do_map_phys` :310-363 / `do_remap` :366-435）+ `region.c`（`region_find_slot_range` :302-394 / `region_find_slot` :399-416 / `map_page_region` :463-510）+ `mem_file.c`（`mappedfile_setfile` :191-246）+ `mem_shared.c`（`shared_setsource` :167-205）+ `vfs.c`（`vfs_request`/`do_vfs_reply`）+ `minix3/minix/lib/libc/sys/mmap.c`（`minix_mmap_for`/`minix_vfs_mmap`/`vm_remap`/`vm_remap_ro`）+ `minix3/sys/sys/mman.h`（`MAP_*`/`PROT_*` :62-124）+ `minix3/minix/include/minix/ipc.h`（`mess_mmap` :1582-1592 / `mess_vm_vfs_mmap` :2369-2380 / `mess_lsys_vm_map_phys` :1504-1510 / `mess_lsys_vm_vmremap` :1537-1545）+ `minix3/minix/include/minix/com.h`（`VM_MMAP`/`VM_VFS_MMAP`/`VM_REMAP`/`VM_REMAP_RO`/`VM_MAP_PHYS`/`VMVFSREQ_FDLOOKUP` :702）
> **Rust 模块**: `os/servers/vm/src/mmap.rs`（`MmapFlags`/`ProtFlags` :56-133 / `FILEMAP_ENABLED` :137-146 / `MmapError` :158-174 / `MmapResult` :188-190 / `MMAP_BASE`/`MMAP_TOP` :203-204 / `mmap_region` :226-265 / `handle_mmap` :268-387 / `FileMapParams` :389-412 / `mmap_file` :414-473 / `handle_vfs_mmap` :475-523 / `mmap_file_cont` :525-567）+ `os/servers/vm/src/map_phys.rs`（`handle_map_phys` :48-104 / `map_perm_check` :106-122）+ `os/servers/vm/src/ipc/dispatcher.rs`（`dispatch_mmap` :396-409 / `dispatch_vfs_mmap` :411-425 / `dispatch_map_phys` :427-437 / 主循环分支 :1030/:1034/:1044/:1153 / 错误映射 :1240-1268 / `dispatch_remap_impl` :1347-1449）+ `os/servers/vm/src/vfs_queue.rs`（`VfsRequestState::FdLookup` :29-33 / `request` :107-118 / `handle_reply` :126-148）+ `os/libs/minix-types/src/ipc/vm.rs`（`VmMmapIn` :212-222 / `VmMapPhysIn` :236-244 / `VmVfsMmapIn` :259-270 / `VmRemapIn` :490-504 / `decode_message` 四件套 :837-1011）+ `os/libs/minix-types/src/ipc/message.rs`（union :134-140 / `MessMmap` :1559-1605 / `MessVmVfsMmap` :1607-1656 / `MessLsysVmMapPhys` :1658-1694 / `MessLsysVmVmremap` :1696-1733）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/13-region-mapping.md`（区域结构 + map_page_region 语义）+ `14-region-lookup.md`（AVL/BTreeMap 查找）+ `15-ipc-dispatch.md`（主循环分发与回复）+ `19-vm-brk.md`（堆顶调整，mmap 区间的下界参考）+ `12-memtype.md`（六种 memtype 回调族）
> **说明**: 本文档管 **mmap 服务**——VM 如何响应 `VM_MMAP`/`VM_VFS_MMAP`/`VM_REMAP`/`VM_REMAP_RO`/`VM_MAP_PHYS`：调用者验证（execpriv 分级）→ 地址空间分配（`mmap_region` 三路解析）→ 匿名/文件分流（同步匿名 / VFS 异步文件）→ 辅助路径（物理映射 / 共享区域 remap）。**不覆盖**：munmap（21）、页缓存（24）、VFS 异步对话基础设施（23）、`do_get_phys`/`do_get_refcount`（26 queries）。

---

## 1. 概念：mmap = 地址空间分配 + 内存来源绑定

### 1.0 章节引言

brk（19）只回答"堆顶在哪、怎么调"；mmap 回答更通用的三个问题：

1. **映射到哪**——在进程地址空间里找一段空闲虚拟区间（`mmap_region` 三路解析：MAP_FIXED / 提示地址 / 默认区间，§1.2/§2.1）。
2. **绑定什么**——这段区间绑定到四种内存来源之一：匿名页（`mem_type_anon`）、文件（`mem_type_mappedfile`，经 VFS 异步）、物理内存（`mem_type_directphys`，设备寄存器/DMA）、共享区域（`mem_type_shared`，remap）。
3. **谁允许**——`MAP_THIRDPARTY`（代表他人映射）与 `MAP_UNINITIALIZED`（跳过清零）只有 VFS/RS 特权可用；`MAP_PREALLOC` 连续物理内存必须预分配；`map_perm_check` 把关物理地址映射权限。

它在整个 02-stage-vm 中的位置：

```
13（区域结构）→ 14（查找）→ 15（分发）→ 19（brk 堆顶）→ ★20（mmap 通用映射）
→ 21（munmap 释放）→ 23（VFS 异步）→ 24（页缓存）→ 26（queries）
```

### 1.1 映射在地址空间中的位置

Minix3 的 mmap 区间独立于 text/data/gap/stack。32 位下运行时计算（`vm.h:65-79`）：

- **MAGIC 构建**：`VM_MMAPTOP = VM_STACKTOP - DEFAULT_STACK_LIMIT`（栈顶减 4MB），`VM_MMAPBASE = VM_MMAPTOP / 2`；
- **非 MAGIC**：`VM_MMAPTOP = VM_DATATOP`，`VM_MMAPBASE = VM_PAGE_SIZE`。

```
低地址                                       高地址
├─ text ─┬─ data/bss/heap ─┬─ gap ─┬─ stack ─┤
└─────────── MMAPBASE .. MMAPTOP（mmap 分配区间）──┘
```

minix-rs 为 64 位地址空间（**[ARCH: A-6]**：32 位稀缺 → 64 位余量）：固定 `MMAP_BASE = 0x0000_0001_0000_0000`、`MMAP_TOP = 0x0000_0200_0000_0000`（mmap.rs:203-204），远离 brk/stack，无需运行时重算。

### 1.2 三路地址解析（mmap_region）

C `mmap_region`（mmap.c:36-83）决定新区域落在哪：

| 输入 | C 行为 | 语义 |
|------|--------|------|
| `MAP_FIXED` + addr | 先 `map_unmap_range(addr, len)` 清空，再精确放在 addr（:60-68） | 替换式固定映射 |
| 非 FIXED + addr ≠ 0 | 先 `map_page_region(addr, 0, len)` 精确尝试（:70-76），失败回退全区间 | 提示地址（best-effort） |
| addr == 0 | 直接 `map_page_region(VM_MMAPBASE, VM_MMAPTOP, len)`（:78-80） | 系统选择 |

C 的区间查找（`region_find_slot_range`，region.c:302-394）**顶对齐放置**（`startv = frend - length`，:346）并在每个间隙两侧留 1 页 padding（`FREEVRANGE` 先试收缩区间再试全区间，:350-355）；`region_find_slot`（:399-416）优先从 `vm_region_top` 提示点向上增长。Rust `find_slot`（region_map.rs:157-211）同样是顶对齐，但从低地址向高扫描——分配策略差异（§3.6 #7，外部行为等价：区间内任一合法地址）。

### 1.3 匿名 vs 文件分流

`do_mmap`（mmap.c:227-268）按 `fd == -1 || MAP_ANON` 分流：

- **匿名**：同步创建 `VR_WRITABLE | VR_ANON` 区域（`mem_type_anon`；`MAP_CONTIG` → `mem_type_anon_contig`），物理页惰性分配（缺页时，16 范围）。C 恒 `VR_WRITABLE`（mmap.c:247）——见 §3.6 #1。
- **文件**：两步走——先 `enable_filemap` 守卫（ENXIO）与 `MAP_SHARED && PROT_WRITE` 拒绝（ENXIO），再 `vfs_request(VMVFSREQ_FDLOOKUP, ...)` 向 VFS 查询文件元数据并 `return SUSPEND`（mmap.c:263-268）；VFS 回复后由回调 `mmap_file_cont` 完成映射并解除调用者阻塞。

```
用户 mmap(fd != -1)
  → VM: ENXIO 守卫 → vfs_queue.request(FdLookup) → SUSPEND（不回复）
  → VFS: 回复 VM_VFS_REPLY { fd, dev, ino, size_pages }
  → mmap_file_cont 回调 → mmap_file（页对齐 + 建 MAPPED_FILE 区域 + fdref）
  → ipc_send 回复调用者（transport 接线后，§5.3）
```

### 1.4 权限模型

- **execpriv**（VFS/RS，mmap.c:208-210）：`MAP_THIRDPARTY`（代表 forwhom 映射，EPERM/ESRCH，:211-221）与 `MAP_UNINITIALIZED`（跳过清零）。
- **map_perm_check**（mmap.c:284-307）：TTY/MEM 豁免（TTY 可为任何人 TIOCMAPMEM，MEM 仅自身）；其余 `sys_privquery_mem(target, physaddr, len)` 由内核裁决（PCI 授权）。Rust 中内核 syscall 未实现 → **fail-closed 拒绝**（§5.3 B4）。
- **do_remap**（mmap.c:366-435）：`destination`/`source` 是**消息字段**而非 `m_source`——IPC 服务器替客户端 remap 共享内存（`minix3/minix/servers/ipc/shm.c:159`：`vm_remap(m->m_source, sef_self(), ...)`）。调用者仍需过 ACL 掩码（15 范围），但目标/来源由消息指定。

### 1.5 对照：Redox 与 Linux

- **Linux**：地址分配与映射分离——`get_unmapped_area()`（arch 相关，含 hint 语义）与 `mmap_region()` 独立；`MAP_FIXED` 先 `do_munmap` 旧区间再放置（与 C Minix3 `map_unmap_range` 相同）；`MAP_FIXED_NOREPLACE` 提供"不覆盖已有映射"的安全变体（Minix3 无此能力）。文件映射 `mmap` 走 `file->f_op->mmap`（如 ext4 `ext4_file_mmap`），页回写与缓存由内核统一管理。三家共同点：**hint 地址都是 best-effort**、**MAP_FIXED 替换式**、**匿名页惰性清零（COW/zero-fill）**。
- **Redox**：`mmap` 由内核 `AddrSpace::mmap()` 处理——在进程的 grant BTreeMap 中查找空闲区间（与 Minix3/Rust 的"区间查找"抽象同构），文件映射经 scheme 系统（`MmapPrep`/`RequestMmap` 请求链，与 Minix3 的 VFS FDLOOKUP 异步对话同构），匿名映射走 `MemoryScheme::fmap_anonymous`。
- **对照要点**：Minix3 把 mmap 放在**用户态 VM 服务器**（微内核架构，文件元数据查询要跨服务异步）；Linux/Redox 在内核。三家的"找空闲区间"都抽象为 kernel/VM 侧函数，Minix3 的独特之处是**文件映射的异步两段式**（SUSPEND + 回调）——这是本服务最复杂的控制流。

### 1.6 小结

mmap = 调用者验证（execpriv 分级）→ 地址分配（mmap_region 三路解析，顶对齐）→ 来源绑定（anon 同步 / file 异步 VFS / phys 权限校验 / shared remap）。C 的恒 WRITABLE 匿名与 MAP_SHARED 不传播 VR_SHARED 是历史语义；Rust 在保持外部可观察行为的前提下做了 POSIX 收紧（§3.6）。文件映射的异步两段式是本文档的控制流主角。

---

## 2. C 源码分析

### 2.1 调用路径与消息格式

```
用户 mmap(addr, len, prot, flags, fd, offset)
  → libc minix_mmap_for（forwhom != SELF 自动加 MAP_THIRDPARTY，libc/sys/mmap.c）
  → _syscall(VM_PROC_NR, VM_MMAP, &m)     m.m_mmap.{offset,addr,len,prot,flags,fd,forwhom}
  → kernel 转发（m_source = 调用者 endpoint）
  → VM 主循环（dispatcher.rs:1030 VM_MMAP 分支）
  → do_mmap(msg)                           mmap.c:200-269
  ├─ 匿名 → mmap_region → 回复 vr->vaddr
  └─ 文件 → vfs_request(FDLOOKUP) → SUSPEND → mmap_file_cont → mmap_file → ipc_send
```

**消息结构 `mess_mmap`**（ipc.h:1582-1592，32 位 C 线格式）：

| 字段 | 类型 | 载荷偏移 | 说明 |
|------|------|---------|------|
| `offset` | off_t (u64) | 0 | 文件偏移 |
| `addr` | void* (u32) | 8 | 映射地址提示（MAP_FIXED 时为精确地址） |
| `len` | size_t (u32) | 12 | 长度（0 → EINVAL） |
| `prot` | int | 16 | PROT_READ/WRITE/EXEC |
| `flags` | int | 20 | MAP_SHARED/PRIVATE/FIXED/ANON/CONTIG/... |
| `fd` | int | 24 | -1 表示匿名 |
| `forwhom` | endpoint_t | 28 | THIRDPARTY 目标 |
| `retaddr` | void* | 32 | 回复：映射地址 |

`minix_vfs_mmap`（VFS → VM，`mess_vm_vfs_mmap` ipc.h:2369-2380）：`offset`(u64@0)/`dev`(u64@8)/`ino`(u64@16)/`who`(@24)/`vaddr`(@28)/`len`(@32)/`flags`(@36)/`fd`(@40)/`clearend`(@44)。其中 `flags` 是 u16 位图，`MVM_WRITABLE = 0x8000`（vm.h:34）由 VFS exec 设置（vfs/exec.c:167-173）。VFS 用它映射 ELF 段（ld.so/可执行文件），语义是 `MAP_PRIVATE|MAP_FIXED` + 页按需加载。

### 2.2 do_mmap 分步（mmap.c:200-269）

1. **execpriv**（:208-210）：`m_source == VFS_PROC_NR || RS_PROC_NR`。
2. **THIRDPARTY**（:211-221）：无特权 → EPERM；`vm_isokendpt(forwhom)` 失败 → ESRCH；否则 `vmp = &vmproc[forwhom]`。
3. **长度**（:224）：`len <= 0` → EINVAL（SUSv3）。
4. **匿名分流**（:227-253）：`fd == -1 || MAP_ANON`：
   - `fd != -1` → printf + EINVAL（MAP_ANON 带 fd 的矛盾请求）；
   - `(flags & (CONTIG|PREALLOC)) == CONTIG` → EINVAL（连续物理内存必须预分配）；
   - `MAP_CONTIG` → `mem_type_anon_contig`，否则 `mem_type_anon`；
   - `mmap_region(vmp, addr, flags, len, VR_WRITABLE|VR_ANON, mt, execpriv)` 失败 → ENOMEM。
5. **文件分流**（:255-268）：
   - `!enable_filemap` → ENXIO（全局开关，glo.h:22，main.c:447 默认 1，env_parse "filemap"）；
   - `(flags & MAP_SHARED) && (prot & PROT_WRITE)` → ENXIO；
   - `vfs_request(VMVFSREQ_FDLOOKUP, fd, vmp, 0, 0, mmap_file_cont, NULL, m, sizeof(*m))` 失败 → ENXIO；
   - 成功 → **SUSPEND**（主循环不回复，等 VFS）。
6. **回复**：`m_mmap.retaddr = vr->vaddr`，`m_type = OK`。

**注意 C 不做的事**：不校验 MAP_SHARED/MAP_PRIVATE 互斥（flags=0 的匿名映射照常成功）；不把 MAP_SHARED 传播到 `VR_SHARED`（VR_SHARED 全仓库只在 do_remap 设置，mmap.c:413，region.c:1433 仅 usage 统计读取）——即 **Minix3 用户态 MAP_SHARED 匿名映射在 fork 时按 COW 私有复制处理**，与 MAP_PRIVATE 无异。Rust 保持该语义（§3.6 #2）。

### 2.3 mmap_region 分步（mmap.c:36-83）

1. **flags → vrflags 转换**（:39-50）：LOWER16M → VR_LOWER16MB；LOWER1M → VR_LOWER1MB；ALIGNMENT_64KB → VR_PHYS64K；PREALLOC → `MF_PREALLOC`（map_page_region 立即分配页）；UNINITIALIZED 需 execpriv（否则 NULL → ENOMEM）→ VR_UNINITIALIZED。
2. **长度**（:54-58）：`len <= 0` → NULL；非页对齐向上圆整。
3. **MAP_FIXED + addr**（:60-68）：`map_unmap_range(vmp, addr, len)` 清空；失败 → NULL。
4. **addr 或 FIXED**（:70-76）：`map_page_region(vmp, addr, 0, len, ...)`（maxv=0 = 精确放置）；FIXED 且失败 → NULL。
5. **回退**（:78-80）：`map_page_region(vmp, VM_MMAPBASE, VM_MMAPTOP, len, ...)`。

`map_page_region`（region.c:463-510）：`region_find_slot` 找地址 → `region_new` → `ev_new` 回调 → `MF_PREALLOC` 则 `map_handle_memory` 立即分配 → 清 UNINITIALIZED → 插入 AVL。

### 2.4 mmap_file 与 mmap_file_cont（mmap.c:84-132/160-190）

`mmap_file(vmp, vmfd, file_offset, flags, ino, dev, filesize, addr, len, *retaddr, clearend, writable, mayclosefd)`：

1. `writable` → VR_WRITABLE（:90）。
2. **页对齐**（:91-96）：`page_offset = file_offset % VM_PAGE_SIZE`；非零则 `file_offset -= page_offset; len += page_offset`；`len = roundup(len)`。断言三者页对齐（:98-102）。
3. `mmap_region(vmp, addr, flags, len, vrflags, &mem_type_mappedfile, 0)`（:112-119）：失败 → ENOMEM；成功 → `*retaddr = vr->vaddr + page_offset`（**调用者看到原始偏移语义**）。
4. `mappedfile_setfile(vmp, vr, vmfd, file_offset, dev, ino, clearend, 1, mayclosefd)`（:128）——记录文件身份供页缓存/COW 使用（mem_file.c:191-246，23/24 范围）。

`mmap_file_cont(vmp, replymsg, cbarg, origmsg)`（:160-190，VFS 回复回调）：

1. `writable = origmsg->m_mmap.prot & PROT_WRITE`（:166-168）。
2. `replymsg->VMV_RESULT != OK` → `result = VMV_RESULT`（VFS 的 errno 直通）。
3. 成功 → `mmap_file(vmp, VMV_FD, origmsg->offset, origmsg->flags, VMV_INO, VMV_DEV, VMV_SIZE_PAGES*PAGE_SIZE, origmsg->addr, origmsg->len, &v, 0, writable, 1)`（:171-181）——**clearend=0、mayclosefd=1**（用户持有原 fd，映射可超期存活）。
4. `memset(&mmap_reply,0); mmap_reply.m_type = result; mmap_reply.m_mmap.retaddr = v; ipc_send(vmp->vm_endpoint, &mmap_reply)`（:184-189）——直接解除调用者阻塞。

### 2.5 do_vfs_mmap（mmap.c:135-158）

1. `!enable_filemap` → ENXIO（:143）。
2. `vm_isokendpt(m_vm_vfs_mmap.who)` → panic 于坏端点（:144-146）。
3. `mmap_file(vmp, fd, offset, MAP_PRIVATE|MAP_FIXED, ino, dev, LONG_MAX*PAGE_SIZE, vaddr, len, &v, clearend, flags, 0)`（:148-155）——`writable` 参数直接传 VFS 的 `flags` 位图（MVM_WRITABLE=0x8000），`mayclosefd=0`（VFS 持有 fd）。
4. 同步回复 `v`。

### 2.6 map_perm_check 与 do_map_phys（mmap.c:284-363）

`map_perm_check(caller, target, physaddr, len)`（:284-307）：

- `caller == TTY_PROC_NR` → OK（TTY 可为任何人映射，TIOCMAPMEM ioctl）；
- `caller == MEM_PROC_NR` → OK（MEM 仅自身）；
- 其余 → `sys_privquery_mem(target, physaddr, len)`（内核查询 PCI 授予的物理区间权限）。

`do_map_phys`（:310-363）：`len <= 0` → EINVAL；`target == SELF` → m_source；`vm_isokendpt` 失败 → EINVAL；`map_perm_check` 失败 → EPERM；`offset = startaddr % PAGE_SIZE; len += offset; startaddr -= offset;` 圆整；`map_page_region(VM_MMAPBASE, VM_MMAPTOP, len, VR_DIRECT|VR_WRITABLE, 0, &mem_type_directphys)` → ENOMEM；`phys_setphys(vr, startaddr)`；回复 `vr->vaddr + offset`。

### 2.7 do_remap（mmap.c:366-435）

1. `m_type == VM_REMAP` → readonly=0；`VM_REMAP_RO` → readonly=1（:378-385）。
2. `size <= 0` → EINVAL（:387）。
3. `vm_isokendpt(destination/source)` 失败 → EINVAL（:389-392）。
4. `map_lookup(svmp, sa)` 无 → EINVAL（:396）。
5. `src_region->vaddr != sa` → EFAULT（:398-400，必须区域起点）。
6. `size` 圆整 ≠ `src_region->length` → EFAULT（:402-406，必须整区域）。
7. `flags = VR_SHARED` (+WRITABLE 若非 RO)（:409-411）。
8. `da` 给定 → `map_page_region(dvmp, da, 0, size, ...)`；否则 `VM_MMAPBASE..VM_MMAPTOP`（:412-415）；失败 → ENOMEM。
9. `shared_setsource(vr, svmp->vm_endpoint, src_region)`（:417）——记录源（mem_shared.c:167-205），`srcvr->remaps++`（:195）。
10. 回复 `vr->vaddr`（:431）。

### 2.8 C 小结：符号全景

| 符号 | C 位置 | 语义 |
|------|--------|------|
| `do_mmap` | mmap.c:200 | 主入口：验证 + 分流 + 回复 |
| `mmap_region` | mmap.c:36 | 三路地址解析 + 建区域 |
| `mmap_file` | mmap.c:84 | VFS 元数据就绪后完成文件映射 |
| `mmap_file_cont` | mmap.c:160 | VFS 回调：续作 + ipc_send 解除阻塞 |
| `do_vfs_mmap` | mmap.c:135 | VFS 主动映射（ELF 段） |
| `map_perm_check` | mmap.c:284 | 物理地址权限（TTY/MEM 豁免） |
| `do_map_phys` | mmap.c:310 | 物理地址 → 虚拟区间 |
| `do_remap` | mmap.c:366 | 共享区域重映射（destination/source 消息字段） |
| `do_get_phys`/`do_get_refcount` | mmap.c:438-485 | 查询（26 范围，不覆盖） |
| `do_munmap`/`munmap_vm_lin` | mmap.c:488-573 | 释放（21 范围，不覆盖） |

---

## 3. Rust 设计决策

### 3.1 D1：mmap_region 三路解析独立成函数（mmap.rs:226-265）

`handle_mmap` 与 `mmap_file` 共享 `mmap_region`（返回解析后的 vaddr，区域由调用者创建），对应 C 同名函数：

- MAP_FIXED：`addr == 0` 或非页对齐 → `BadAddress`（**收紧**，C 会映射到 0/接受未对齐，§3.6 #4/#5）；否则 `munmap::unmap_range` 清空后返回 addr。
- 提示地址：页对齐且 `find_overlap` 确认 [addr, addr+len) 空闲 → 返回 addr；否则回退全区间。
- 全区间：`find_slot(MMAP_BASE, MMAP_TOP, len)`。

### 3.2 D2：handle_mmap 编排（mmap.rs:268-387）

校验顺序对齐 C：THIRDPARTY（EPERM → forwhom 解析）→ `vm_isokendpt` → `len == 0` → flags 互斥（**收紧**）→ 匿名/文件分流。错误语义（dispatcher.rs:1240-1258）：

| MmapError | C errno | 本轮变化 |
|-----------|---------|---------|
| `InvalidLength` | EINVAL（mmap.c:224） | **修复**：原 EFAULT（InvalidAddress）→ `VmError::InvalidParam`（EINVAL，新增变体） |
| `InvalidFlags` | EINVAL（mmap.c:229-245） | **修复**：原 EFAULT → InvalidParam |
| `BadAddress` | EFAULT（收紧专用） | 保持 InvalidAddress |
| `FileMapDisabled` | ENXIO（mmap.c:255-261） | **修复**：原 ENOSYS（NotImplemented）→ `VmError::NoDevice`（ENXIO，新增变体） |
| `PermissionDenied` | EPERM | 保持 |
| `ProcessNotFound` | ESRCH（mmap.c:217 THIRDPARTY） | 保持 |
| `OutOfMemory` | ENOMEM | 保持 |

### 3.3 D3：文件映射异步两段式（mmap.rs:389-567）

`handle_mmap` 文件分支：`enable_filemap` 守卫（`FILEMAP_ENABLED` AtomicBool，mmap.rs:137-146，C glo.h:22/main.c:447 默认 1）→ MAP_SHARED+WRITE 拒绝 → 组装 `VfsRequest{ FdLookup, callback: mmap_file_cont, state: FdLookup{ mmap: *request } }` 入队 → `MmapResult::Suspended`。入队失败 → ENXIO（C vfs_request 失败 :266-268）。

`mmap_file_cont`（mmap.rs:525-567）从 `VfsRequestState::FdLookup { mmap }` 恢复原始请求（C origmsg，:169-181）：`reply.result != OK` → 保序返回（回复 errno 待 transport）；成功 → `mmap_file`（clearend=0、mayclosefd=1、writable=prot&WRITE）。**ipc_send 解除阻塞依赖 KernelIpcTransport（transport.rs 未实现）——backlog B3（§5.3）**。

### 3.4 D4：mmap_file 与 VrParam::File（mmap.rs:414-473）

`FileMapParams` 聚合两路调用者的参数（VFS 消息 vs 原始 mmap + FDLOOKUP 回复）。`mmap_file`：`page_offset` 进位（retaddr = vaddr + page_offset，C mmap.c:98-100/:125）→ `mmap_region(MAPPED_FILE)` → fdref 表登记（`find_by_dev_ino` 命中则 ref，未命中 create+ref，替代 C `mappedfile_setfile`）→ `VrParam::File { inited: true, fdref_id, offset: file_offset, clearend }`。

### 3.5 D5：do_remap / map_phys 语义对齐（dispatcher.rs:1347-1449 + map_phys.rs:48-122）

- **do_remap**：`destination`/`source` 来自消息字段（非 m_source，§1.4）；零长度/坏端点/源区域缺失 → EINVAL（**本轮修复**：原 EFAULT/ESRCH）；非区域起点/长度不匹配 → EFAULT；`VR_SHARED`(+WRITABLE)；`VrParam::Shared{ep,vaddr,id}` + `remaps++`。
- **map_phys**：`len==0` → EINVAL（**本轮修复**：原 EFAULT）；`map_perm_check` TTY/MEM 豁免、其余 fail-closed 拒绝（内核 `sys_privquery_mem` 未实现，backlog B4）；偏移进位 + 圆整；`VR_DIRECT|WRITABLE` + `MEM_TYPE_DIRECT` + `VrParam::Direct{phys}`。

### 3.6 差异清单（C ↔ Rust，诚实标注）

| # | C 语义 | Rust 现状 | 状态 |
|---|--------|----------|------|
| 1 | 匿名区恒 `VR_WRITABLE`（mmap.c:247，PROT_READ-only 也可写） | 按 `PROT_WRITE` 派生 WRITABLE（POSIX 正确：PROT_READ-only 写入 → SIGSEGV） | ✅ 收紧（外部 API 等价，行为更符合 POSIX/Linux/Redox） |
| 2 | MAP_SHARED 不传播 `VR_SHARED`（仅 do_remap 设） | 同 C（to_vr_flags 不含 SHARED→VR_SHARED，mmap.rs:102-121） | ✅ 等价（Minix3 用户 MAP_SHARED 匿名 fork 时按私有 COW 处理） |
| 3 | `MAP_UNINITIALIZED` 无特权 → ENOMEM（mmap_region NULL） | → EINVAL（InvalidFlags） | ✅ 收紧（错误更准确，fail-fast） |
| 4 | `MAP_FIXED` addr=0 → 尝试映射地址 0 | → EFAULT（BadAddress） | ✅ 收紧（映射地址 0 无意义且危险） |
| 5 | `MAP_FIXED` 非页对齐地址被接受 | → EFAULT（POSIX 要求页对齐） | ✅ 收紧 |
| 6 | 未对齐提示地址被精确采用 | 视为无提示，回退全区间 | ✅ 收紧（区域槽位恒页对齐） |
| 7 | 分配策略：C `vm_region_top` 提示 + 顶对齐 | Rust `find_slot` 底向上扫描 + 顶对齐 | ✅ 策略差异（区间内任一合法地址，外部等价） |
| 8 | `len <= 0` → EINVAL | InvalidLength → EINVAL（本轮修复原 EFAULT） | ✅ 修复（20-P1-1 语义侧） |
| 9 | flags 无效组合不检查（flags=0 匿名成功） | SHARED/PRIVATE 互斥校验 → EINVAL | ✅ 收紧 |
| 10 | `MAP_PREALLOC` → `MF_PREALLOC` 立即分配页 | 仅标 `PREALLOC_MAP`，页仍惰性分配 | ⚠️ 缺口（backlog B1，RS GET_PREALLOC_MAP 可查询到标志） |
| 11 | `vfs_request` 经 IPC 直发 VFS | 入队 `VfsRequestQueue`（KernelIpcTransport 未接线） | ⚠️ 部分（backlog B2） |
| 12 | `mmap_file_cont` `ipc_send` 解除阻塞 | 回调建区域；回复依赖 transport | ⚠️ 部分（backlog B3） |
| 13 | `map_perm_check` 经 `sys_privquery_mem` 内核裁决 | TTY/MEM 豁免 + fail-closed 拒绝 | ⚠️ 部分（backlog B4） |
| 14 | `do_mmap` 主循环直接 vfs_request（C 静态全局 vfs_rq） | `VfsRequestQueue` 显式队列（23 设计，vfs_queue.rs） | ✅ 结构差异（23 范围） |

---

## 4. 实现详解

### 4.1 消息路径（dispatcher.rs:1030-1162 → mmap.rs）

```
主循环（dispatcher.rs）
  VM_MMAP（:1030）→ VmMmapIn::decode_message(msg)          ← 20-P1-1：m_source + m_mmap overlay
    → dispatch_mmap(table, page_alloc, frames, vfs_queue, request)   :396-409
    → mmap::handle_mmap（mmap.rs:268）
      → Complete → VmReply::Mmap(VmMmapOut{ret_addr})
      → Suspended → VmReply::Suspend（主循环不回复）
      → Err → VmReply::Error(e.into())（dispatcher.rs:1240 映射）

  VM_VFS_MMAP（:1044）→ VmVfsMmapIn::decode_message(msg)
    → dispatch_vfs_mmap → handle_vfs_mmap（mmap.rs:475）→ 同步 Complete

  VM_MAP_PHYS（:1034）→ VmMapPhysIn::decode_message(msg)
    → dispatch_map_phys → map_phys::handle_map_phys（map_phys.rs:48）

  VM_REMAP / VM_REMAP_RO（:1153/:1162）→ VmRemapIn::decode_message(msg)
    → dispatch_remap / dispatch_remap_ro → dispatch_remap_impl（:1347，readonly 由 call 决定）

  VM_VFS_REPLY → dispatch_vfs_reply（:328）→ VfsRequestQueue::handle_reply
    → 返回 VfsReplyResult{ reply: Suspend, callback: mmap_file_cont }
    → 主循环 dispatch_on_msg 在 vfs_queue 借用释放后执行 callback（vm_server.rs:626-628）
```

### 4.2 伪码总结（mmap.rs:268-567）

```
handle_mmap(table, page_alloc, frames, vfs_queue, req)          :268
  execpriv = caller ∈ {VFS, RS}
  THIRDPARTY ? { !execpriv → EPERM; target = forwhom } : target = caller
  slot = vm_isokendpt(target)?                                → ProcessNotFound
  len == 0 → InvalidLength                                    （C mmap.c:224）
  !flags.is_valid() → InvalidFlags                            （收紧）
  anon (fd==-1 || ANON):
    fd != -1 → InvalidFlags                                   （C :229-233）
    CONTIG && !PREALLOC → InvalidFlags                        （C :242-245）
    UNINITIALIZED && !execpriv → InvalidFlags                 （C :46-50，收紧）
    vr_flags = prot.to_vr_flags(flags) | ANON
    mem_type = CONTIG ? CONTIG_ANON : ANON
    vaddr = mmap_region(addr, flags, aligned_len)             :226
    insert(region); add_total
    → Complete(vaddr)
  file:
    !filemap_enabled() → FileMapDisabled                      （C :255，ENXIO）
    SHARED && WRITE → FileMapDisabled                         （C :258-261，ENXIO）
    vfs_queue.request(FdLookup{ mmap: *req }, cb=mmap_file_cont)
      .map_err(|_| FileMapDisabled)?                          （C vfs_request 失败 :266）
    → Suspended

mmap_region(active, page_alloc, frames, addr, flags, len)      :226
  FIXED ? { addr==0 || 非页对齐 → BadAddress; unmap_range; → addr }
  hint(addr≠0 页对齐) && find_overlap(addr, addr+len).none → addr
  → find_slot(MMAP_BASE, MMAP_TOP, len) → OutOfMemory

mmap_file(active, page_alloc, frames, params)                  :414
  page_offset = file_offset % PAGE_SIZE
  file_offset -= page_offset; len = roundup(len + page_offset)
  vr_flags = writable ? WRITABLE : empty
  vaddr = mmap_region(params.addr, params.flags, len)
  fdref_id = find_by_dev_ino(dev, ino) ? ref : create(fd,dev,ino,mayclosefd)+ref
  region = VirRegion::with_memtype(vaddr, len, vr_flags, MAPPED_FILE)
  region.param = File { inited: true, fdref_id, offset: file_offset, clearend }
  insert; add_total
  → Complete(vaddr + page_offset)

handle_vfs_mmap(table, page_alloc, frames, req)                :475
  !filemap_enabled() → FileMapDisabled
  slot = vm_isokendpt(req.who)?
  mmap_file(addr=vaddr, flags=PRIVATE|FIXED, len, offset,
            fd/dev/ino, clearend, writable=flags≠0, mayclosefd=false)
  → Complete

mmap_file_cont(server, reply, state)                           :525
  FdLookup{ mmap } = state
  reply.result != OK → Ok(())（errno 保序，回复待 transport）
  target = THIRDPARTY ? forwhom : caller
  mmap_file(addr, flags, len, offset, reply.fd/dev/ino,
            clearend=0, writable=prot&WRITE, mayclosefd=true)
  → Ok(())（ipc_send 解除阻塞：backlog B3）
```

### 4.3 20-P1-1 wire format 修复记录（本轮）

- **问题**：`VmMmapIn`/`VmVfsMmapIn`/`VmMapPhysIn`/`VmRemapIn` 四个 `DecodeFromM1` stub 从 `MessageM1` 读字段，与 C 线格式全错位——真实 libc 发送者 `memset(&m,0)` 后按 `mess_mmap` 布局写载荷：M1 decode 的 `prot/flags/fd/offset` 读到 0、`dev/ino/clearend` 读到 0、remap 的 destination/source 读到错位值。**每个 mmap() 的请求参数都会丢失**（19-P1-1/16-P0-1 同族）。
- **修复**：
  1. `message.rs:134-140`——四个专用 union 成员（`m_mmap`/`m_vm_vfs_mmap`/`m_lsys_vm_map_phys`/`m_lsys_vm_vmremap`），结构体忠实 C 32 位布局（`MessMmap` :1559-1605 等）；
  2. `vm.rs:837-1011`——`VmMapPhysIn`/`VmMmapIn`/`VmVfsMmapIn`/`VmRemapIn` 各实现 `decode_message(msg)`，caller 一律取 `m_source`，载荷读 overlay；删除 4 个 M1 stub；
  3. `dispatcher.rs:1030/:1034/:1044/:1153-1162`——主循环改用 `decode_message`；
  4. `VmRemapIn` 新增 `destination` 字段（消息字段，非 m_source），`dispatch_remap_impl` 用它解析目标端点；
  5. 回归测试：vm.rs 4 个 `decode_message` 测试（m_source + overlay 字段断言）+ dispatcher remap 4 测试 errno 更新。
- **验证**：`cargo test -p minix-types --lib ipc::vm` → 21 passed；`cargo test -p minix-vm --lib` → 374 passed / 1 failed（test_map_lazy pre-existing，13 范围）。

### 4.4 错误语义修复记录（本轮，与 20-P1-1 同批）

| 消息 | C errno | 修复前 Rust | 修复后 |
|------|---------|------------|--------|
| do_mmap len<=0 | EINVAL | EFAULT | `VmError::InvalidParam`（EINVAL，新增变体） |
| do_mmap 无效 flags | EINVAL | EFAULT | InvalidParam |
| do_mmap 文件 ENXIO | ENXIO | ENOSYS | `VmError::NoDevice`（ENXIO，新增变体） |
| do_map_phys len<=0 | EINVAL | EFAULT | InvalidParam |
| do_remap size<=0 | EINVAL | EFAULT | InvalidParam |
| do_remap 坏端点 | EINVAL | ESRCH | InvalidParam |
| do_remap 源区域缺失 | EINVAL | EFAULT | InvalidParam |

---

## 5. 测试要点

### 5.1 单元测试清单（grep 实证，2026-08-16）

| 测试 | 位置 | 覆盖 |
|------|------|------|
| `test_mmap_anonymous_basic` | mmap.rs:626 | 匿名映射基本创建 |
| `test_mmap_zero_length_fails` | mmap.rs:641 | len=0 → InvalidLength（EINVAL） |
| `test_mmap_flags_validation` | mmap.rs:665 | flags 无效（无 SHARED/PRIVATE）→ InvalidFlags |
| `test_mmap_anon_with_fd_rejected` | mmap.rs:689 | MAP_ANON+fd → InvalidFlags（C :229-233） |
| `test_mmap_error_to_errno` | mmap.rs:704 | 全错误路径 errno 映射（EINVAL/EFAULT/ENOMEM/EPERM/ENXIO/ESRCH） |
| `test_mmap_contig_without_prealloc_fails` | mmap.rs:717 | CONTIG 无 PREALLOC → InvalidFlags |
| `test_mmap_thirdparty_no_priv` | mmap.rs:731 | THIRDPARTY 无特权 → PermissionDenied |
| `test_mmap_uninitialized_no_priv` | mmap.rs:745 | UNINITIALIZED 无特权 → InvalidFlags |
| `test_mmap_fixed_unmaps_existing` | mmap.rs:759 | MAP_FIXED 覆盖已占用区间 |
| `test_mmap_fixed_zero_addr_rejected` | mmap.rs:801 | FIXED addr=0 → BadAddress（收紧） |
| `test_mmap_hint_exact_fit` | mmap.rs:825 | 提示地址精确命中 |
| `test_mmap_hint_taken_falls_back` | mmap.rs:850 | 提示被占回退全区间 |
| `test_mmap_file_disabled` | mmap.rs:892 | enable_filemap 关闭 → FileMapDisabled（ENXIO） |
| `test_mmap_file_shared_write_rejected` | mmap.rs:919 | SHARED+WRITE 文件映射 → FileMapDisabled |
| `test_mmap_file_enqueues_vfs_request` | mmap.rs:944 | 文件映射入队 FdLookup + Suspended |
| `test_vfs_mmap_basic` | mmap.rs:972 | VFS_MMAP 基本 + 只读不可写 + VrParam::File |
| `test_vfs_mmap_writable_flag` | mmap.rs:1006 | MVM_WRITABLE(0x8000) → WRITABLE |
| `test_vfs_mmap_page_offset` | mmap.rs:1035 | 页偏移进位 retaddr + len 圆整 |
| `test_vfs_mmap_disabled` | mmap.rs:1073 | VFS_MMAP 关闭 → FileMapDisabled |
| `test_map_phys_basic/zero_length/error_to_errno` | map_phys.rs:153-181 | 基本/零长度/errno |
| `test_dispatch_remap_rejects_*`（4 个） | dispatcher.rs:1632-1708 | 零长度/端点/RO 分支 |
| `test_mmap_file_cont_creates_region` | vm_server.rs:1448 | 回调端到端（区域创建 + File 参数 + 只读） |
| `test_vm_mmap_in/vfs_mmap/map_phys/remap_in_decode_message` | vm.rs:1082-1199 | 20-P1-1 wire format 回归 |

### 5.2 覆盖维度

- **错误语义**：7 条 C errno 路径全部单测断言（§4.4 表）。
- **地址解析**：FIXED 替换 / FIXED addr=0 拒绝 / 提示精确 / 提示回退 / 默认区间。
- **权限**：THIRDPARTY EPERM / UNINITIALIZED 收紧 / map_perm_check TTY/MEM 豁免。
- **文件路径**：ENXIO 双守卫 / FdLookup 入队 / mmap_file_cont 端到端 / VFS_MMAP 同步（含 MVM_WRITABLE 与页偏移）。
- **wire format**：4 个 decode_message 单测覆盖 m_source 来源与 overlay 字段完整性。

### 5.3 覆盖缺口与诚实标注

| # | 缺口 | 原因 | 归属 |
|---|------|------|------|
| B1 | MAP_PREALLOC 真预分配 | 页分配器按需路径已通，立即分配未实现 | backlog（06/13 范围） |
| B2 | VFS 请求真实发送 | `KernelIpcTransport` 未实现（transport.rs） | backlog（23/26） |
| B3 | mmap_file_cont 回复解除阻塞 | 同上 | backlog（23/26） |
| B4 | map_perm_check 内核裁决 | `sys_privquery_mem` 未实现 → fail-closed | backlog（26） |
| B5 | 文件映射页缓存/COW 端到端 | mem_type_mappedfile pagefault 走 cache | 24 范围 |
| B6 | `test_map_lazy`（region/vir_region.rs:504） | 归 13 范围 pre-existing | 13 backlog |

### 5.4 测试统计（截至 2026-08-16）

- `cargo test -p minix-vm --lib`：**374 passed / 1 failed**（上一轮基线 360/1；本轮新增 14 个：mmap 12 + map_phys 0 改 + vm_server 1 + 全量回归；唯一失败 `region::vir_region::tests::test_map_lazy` 归 13 范围 pre-existing）。
- `cargo test -p minix-types --lib`：**80 passed / 0 failed**（ipc::vm 21 个含 4 个新 decode_message 回归）。
- 定向：`cargo test -p minix-vm --lib mmap` → 21 passed（含 rs.rs `test_update_flags_nommap_bypasses_prealloc_check` 1 个 nommap 误匹配）；`cargo test -p minix-vm --lib remap` → 4 passed。

---

## 6. 过渡

### 6.1 位置可回答性

本文档是阶段 7（IPC 服务）的"通用映射服务"篇。启动时序中的位置：13/14（区域结构与查找）→ 15（分发）→ 19（brk 堆顶）→ **20（mmap 通用映射）** → 21（munmap 释放）。

- **前置可回答**：区域怎么建（13）、怎么找（14）、消息怎么分发（15）、堆顶在哪（19）、六种 memtype 是什么（12）。
- **本文档回答**：地址区间怎么分配（三路解析）、四种内存来源怎么绑定、文件映射的 VFS 异步两段式、物理映射权限、remap 共享语义。
- **未回答（移交）**：映射怎么释放（21 `do_munmap`/`munmap_vm_lin`）；文件区域缺页怎么走页缓存（24）；fdref 生命周期与 VFS 对话基础设施（23）；`do_get_phys`/`do_get_refcount`（26）。

### 6.2 下游移交（对照 plan.md §3.4）

| 移交项 | 目标文档 | 交接内容 |
|--------|---------|---------|
| munmap | 21 | `do_munmap`/`munmap_vm_lin`/`VM_UNMAP_PHYS`/`VM_SHM_UNMAP`（mmap.c:488-573） |
| map_phys 细节 | 21 | `do_map_phys` 已实现（map_phys.rs:48-122，F-138）；21 侧重 unmap 侧 |
| VFS 异步对话 | 23 | `VfsRequestQueue` 序列激活模型 + fdref 表（本文件映射入队/回调已接线） |
| 页缓存 | 24 | `mem_type_mappedfile` pagefault → cache（23/24 链路） |
| queries | 26 | `do_get_phys`/`do_get_refcount`（mmap.c:438-485） |

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/13-region-mapping.md`——区域结构与 `map_page_region` 语义（mmap_region 的落点）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/14-region-lookup.md`——`find_slot`/`find_overlap` 查找语义（地址解析的索引面）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/15-ipc-dispatch.md`——主循环分发与 `VmReply::Suspend` 语义
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/19-vm-brk.md`——堆顶调整（mmap 区间与堆的边界）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/12-memtype.md`——六种 memtype 回调族（anon/mappedfile/directphys/shared）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/16-pagefault.md`——惰性页分配与缺页状态机（匿名映射的物理页来源）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/21-vm-munmap.md`——释放路径（映射的生命周期终点）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/23-vfs-interaction.md`——VFS 异步对话与 fdref（文件映射的基础设施）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/24-page-cache.md`——页缓存（文件映射缺页的缓存层）
- `os/servers/vm/src/mmap.rs`、`os/servers/vm/src/map_phys.rs`——Rust 实现
- `minix3/minix/servers/vm/mmap.c`——C 真相源（573 行）
