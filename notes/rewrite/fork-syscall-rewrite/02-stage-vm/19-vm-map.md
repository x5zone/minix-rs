# 19-vm-map: VM_MAP/VM_UNMAP 服务

> **分类**: VM服务  
> **源码**: `minix3/minix/servers/vm/mmap.c`  
> **说明**: VM 对外提供的内存映射服务，支持文件映射、匿名映射、物理内存映射

---

## 1. 概述

VM_MAP/VM_UNMAP 服务提供内存映射功能，实现 POSIX mmap/munmap 系统调用。

**服务类型**

| 消息类型 | 说明 | 对应系统调用 |
|---------|------|-------------|
| `VM_MMAP` | 内存映射 | mmap() |
| `VM_MUNMAP` | 解除映射 | munmap() |
| `VM_MAP_PHYS` | 物理内存映射 | vm_map_phys() |
| `VM_UNMAP_PHYS` | 解除物理映射 | vm_unmap_phys() |

**映射类型**

```
┌─────────────────────────────────────────────────────────────────┐
│                    映射类型                                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ┌─────────────────────────────────────────────────────┐      │
│   │ 匿名映射 (MAP_ANONYMOUS)                             │      │
│   │                                                     │      │
│   │   - 不关联文件                                      │      │
│   │   - 内存初始化为 0                                  │      │
│   │   - 私有映射，写时复制                              │      │
│   │   - 用于 malloc 大块分配                            │      │
│   └─────────────────────────────────────────────────────┘      │
│                                                                 │
│   ┌─────────────────────────────────────────────────────┐      │
│   │ 文件映射 (MAP_FILE)                                  │      │
│   │                                                     │      │
│   │   - 关联文件描述符                                  │      │
│   │   - 支持共享/私有映射                               │      │
│   │   - 按需从文件加载页面                              │      │
│   │   - 用于加载可执行文件、共享库                      │      │
│   └─────────────────────────────────────────────────────┘      │
│                                                                 │
│   ┌─────────────────────────────────────────────────────┐      │
│   │ 物理内存映射 (VM_MAP_PHYS)                           │      │
│   │                                                     │      │
│   │   - 映射指定物理地址                                │      │
│   │   - 仅允许特权进程                                  │      │
│   │   - 用于设备驱动访问硬件                            │      │
│   └─────────────────────────────────────────────────────┘      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**与 Minix3 的对应关系**

```c
/* minix3/minix/include/minix/com.h */
#define VM_RQ_BASE      0xC00
#define VM_MMAP         (VM_RQ_BASE+10)   // mmap 请求
#define VM_MUNMAP       (VM_RQ_BASE+17)   // munmap 请求
```

**Minix3 消息处理**

```c
/* minix3/minix/servers/vm/main.c */
static struct callmap vm_callmap[] = {
    // ...
    CALLMAP(VM_MMAP, do_mmap),       // mmap 处理
    CALLMAP(VM_MUNMAP, do_munmap),   // munmap 处理
    CALLMAP(VM_MAP_PHYS, do_map_phys), // 物理映射
    // ...
};
```

**地址空间布局**

```
┌─────────────────────────────────────────────────────────────────┐
│                    进程地址空间                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   低地址                                                        │
│   ┌─────────────────────────────────────────────────────┐      │
│   │ text | data | bss                                    │      │
│   └─────────────────────────────────────────────────────┘      │
│   ┌─────────────────────────────────────────────────────┐      │
│   │ heap (brk)                                           │      │
│   └─────────────────────────────────────────────────────┘      │
│   ┌─────────────────────────────────────────────────────┐      │
│   │ mmap 区域 (VM_MMAPBASE - VM_MMAPTOP)                 │      │
│   │                                                     │      │
│   │   - 匿名映射                                        │      │
│   │   - 文件映射                                        │      │
│   │   - 共享内存                                        │      │
│   └─────────────────────────────────────────────────────┘      │
│   ┌─────────────────────────────────────────────────────┐      │
│   │ stack                                                │      │
│   └─────────────────────────────────────────────────────┘      │
│   高地址                                                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. IPC 接口说明

### 2.1 VM_MAP

#### 2.1.1 调用者

VM_MMAP 的调用者包括：

**1. 用户进程（直接调用）**

```c
/* minix3/minix/lib/libc/sys/mmap.c */

void *mmap(void *addr, size_t len, int prot, int flags,
    int fd, off_t offset)
{
    return minix_mmap_for(SELF, addr, len, prot, flags, fd, offset);
}
```

用户进程通过 libc 的 mmap() 函数直接向 VM 发送 VM_MMAP 请求。

**2. VFS（文件映射）**

```c
/* VFS 在处理文件映射时调用 */
int minix_vfs_mmap(endpoint_t who, off_t offset, size_t len,
    dev_t dev, ino_t ino, int fd, u32_t vaddr, u16_t clearend,
    u16_t flags);
```

文件映射需要 VFS 和 VM 协作：
1. 用户调用 mmap() → VM 收到 VM_MMAP
2. VM 向 VFS 发送 VMVFSREQ_FDLOOKUP 请求
3. VFS 返回文件信息
4. VM 完成映射

**3. RS（系统服务）**

```c
/* minix3/minix/servers/vm/mmap.c */

/* RS and VFS can do slightly more special mmap() things */
if(m->m_source == VFS_PROC_NR || m->m_source == RS_PROC_NR)
    execpriv = 1;
```

RS 和 VFS 拥有特权，可以执行特殊映射操作（如 MAP_UNINITIALIZED）。

**调用流程**

```
┌─────────────────────────────────────────────────────────────────┐
│                    VM_MMAP 调用流程                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   匿名映射:                                                     │
│   ┌─────────┐     VM_MMAP      ┌─────┐                         │
│   │ 用户进程 │ ────────────────▶│ VM  │                         │
│   └─────────┘                  └─────┘                         │
│                                                                 │
│   文件映射:                                                     │
│   ┌─────────┐     VM_MMAP      ┌─────┐   VMVFSREQ_FDLOOKUP ┌─────┐
│   │ 用户进程 │ ────────────────▶│ VM  │ ──────────────────▶│ VFS │
│   └─────────┘                  └─────┘                     └─────┘
│                                     ▲                         │   │
│                                     │    文件信息              │   │
│                                     └─────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

#### 2.1.2 请求参数

**消息结构**

```c
/* minix3/minix/include/minix/ipc.h */

typedef struct {
    off_t offset;        // 文件偏移
    void *addr;          // 请求的映射地址（提示）
    size_t len;          // 映射长度
    int prot;            // 保护标志
    int flags;           // 映射标志
    int fd;              // 文件描述符（-1 表示匿名映射）
    endpoint_t forwhom;  // 目标进程
    void *retaddr;       // 返回的映射地址
    u32_t padding[5];
} mess_mmap;
```

**参数说明**

| 参数 | 类型 | 说明 |
|------|------|------|
| `addr` | `void*` | 请求的映射地址，NULL 表示由系统选择 |
| `len` | `size_t` | 映射长度（字节），会被页对齐 |
| `prot` | `int` | 保护标志 |
| `flags` | `int` | 映射标志 |
| `fd` | `int` | 文件描述符，-1 表示匿名映射 |
| `offset` | `off_t` | 文件偏移，必须是页大小的倍数 |

**保护标志 (prot)**

```c
/* minix3/sys/sys/mman.h */

#define PROT_NONE   0x00    // 无权限
#define PROT_READ   0x01    // 可读
#define PROT_WRITE  0x02    // 可写
#define PROT_EXEC   0x04    // 可执行
```

**映射标志 (flags)**

```c
/* 共享类型（必须指定其一） */
#define MAP_SHARED    0x0001    // 共享映射
#define MAP_PRIVATE   0x0002    // 私有映射（写时复制）

/* 其他标志 */
#define MAP_FIXED     0x0010    // 必须使用指定地址
#define MAP_ANONYMOUS 0x1000    // 匿名映射（不关联文件）

/* Minix 特有标志 */
#define MAP_UNINITIALIZED 0x040000  // 不清零内存（特权）
#define MAP_PREALLOC      0x080000  // 预分配物理内存
#define MAP_CONTIG        0x100000  // 连续物理内存
#define MAP_LOWER16M      0x200000  // 物理地址低于 16MB
#define MAP_LOWER1M       0x400000  // 物理地址低于 1MB
#define MAP_THIRDPARTY    0x800000  // 代表其他进程映射
```

**标志组合示例**

```c
/* 匿名私有映射（malloc 大块） */
void *mem = mmap(NULL, size, PROT_READ|PROT_WRITE,
                 MAP_PRIVATE|MAP_ANONYMOUS, -1, 0);

/* 文件共享映射 */
void *data = mmap(NULL, size, PROT_READ|PROT_WRITE,
                  MAP_SHARED, fd, 0);

/* 固定地址映射 */
void *fixed = mmap((void*)0x400000, size, PROT_READ,
                   MAP_FIXED|MAP_PRIVATE, fd, 0);
```

#### 2.1.3 返回结果

**成功返回**

```c
/* 成功时返回映射地址 */
m->m_mmap.retaddr = (void *) vr->vaddr;
return OK;
```

返回值通过 `m_mmap.retaddr` 字段传递，是映射区域的起始地址。

**错误返回**

```c
/* minix3/minix/servers/vm/mmap.c */

int do_mmap(message *m)
{
    // ...

    /* "SUSv3 specifies that mmap() should fail if length is 0" */
    if(len <= 0) {
        return EINVAL;
    }

    // ...

    if(!(vr = mmap_region(...))) {
        return ENOMEM;
    }

    // ...
}
```

**错误码**

| 错误码 | 说明 |
|--------|------|
| `EINVAL` | 参数无效（len=0、flags 无效、offset 未对齐） |
| `ENOMEM` | 内存不足或地址空间不足 |
| `EPERM` | 权限不足（如 MAP_THIRDPARTY 无特权） |
| `ESRCH` | 目标进程不存在（MAP_THIRDPARTY） |
| `ENXIO` | 文件映射被禁用或 VFS 请求失败 |
| `EFAULT` | 地址无效（MAP_FIXED 且地址不可用） |

**返回值处理**

```c
/* minix3/minix/lib/libc/sys/mmap.c */

void *mmap(void *addr, size_t len, int prot, int flags,
    int fd, off_t offset)
{
    // ...
    r = _syscall(VM_PROC_NR, VM_MMAP, &m);

    if(r != OK) {
        return MAP_FAILED;  // 错误时返回 MAP_FAILED
    }

    return m.m_mmap.retaddr;  // 成功返回映射地址
}
```

**MAP_FAILED**

```c
/* minix3/sys/sys/mman.h */
#define MAP_FAILED  ((void *) -1)   // mmap 失败时的返回值
```

### 2.2 VM_UNMAP

#### 2.2.1 调用者

VM_MUNMAP 的调用者包括：

**1. 用户进程（直接调用）**

```c
/* minix3/minix/lib/libc/sys/mmap.c */

int munmap(void *addr, size_t len)
{
    message m;

    memset(&m, 0, sizeof(m));
    m.VMUM_ADDR = addr;
    m.VMUM_LEN = len;

    return _syscall(VM_PROC_NR, VM_MUNMAP, &m);
}
```

用户进程通过 libc 的 munmap() 函数直接向 VM 发送 VM_MUNMAP 请求。

**2. VM 自身（内部解除映射）**

```c
/* minix3/minix/servers/vm/mmap.c */

if(m->m_source == VM_PROC_NR) {
    /* VM munmap is a special case, the region we want to
     * munmap may or may not be there in our data structures,
     * depending on whether this is an updated VM instance or not.
     */
    if(!region_search_root(&vmp->vm_regions_avl)) {
        munmap_vm_lin(addr, m->VMUM_LEN);
    }
    else if((vr = map_lookup(vmp, addr, NULL))) {
        if(map_unmap_region(vmp, vr, 0, m->VMUM_LEN) != OK) {
            printf("VM: self map_unmap_region failed\n");
        }
    }
    return SUSPEND;
}
```

**3. 共享内存解除映射**

```c
/* VM_SHM_UNMAP 用于共享内存 */
int vm_unmap(endpoint_t endpt, void *addr)
{
    message m;

    memset(&m, 0, sizeof(m));
    m.m_lc_vm_shm_unmap.forwhom = endpt;
    m.m_lc_vm_shm_unmap.addr = addr;

    return _syscall(VM_PROC_NR, VM_SHM_UNMAP, &m);
}
```

**调用场景**

| 场景 | 消息类型 | 说明 |
|------|---------|------|
| 用户调用 munmap() | VM_MUNMAP | 解除用户映射 |
| VM 自身更新 | VM_MUNMAP | VM 实例更新时 |
| 共享内存分离 | VM_SHM_UNMAP | 解除共享内存映射 |
| 物理映射解除 | VM_UNMAP_PHYS | 解除设备映射 |

#### 2.2.2 请求参数

**消息结构**

```c
/* minix3/minix/include/minix/com.h */

/* VM_MUNMAP 使用 m_mmap 结构 */
# define VMUM_ADDR    m_mmap.addr    // 映射起始地址
# define VMUM_LEN     m_mmap.len     // 映射长度
```

munmap 复用 mmap 的消息结构 `mess_mmap`，只使用 addr 和 len 字段。

**参数说明**

| 参数 | 类型 | 说明 |
|------|------|------|
| `addr` | `void*` | 映射区域的起始地址 |
| `len` | `size_t` | 要解除映射的长度 |

**参数验证**

```c
/* minix3/minix/servers/vm/mmap.c */

int do_munmap(message *m)
{
    // ...

    if(addr % VM_PAGE_SIZE)
        return EFAULT;    // 地址必须页对齐

    len = roundup(m->VMUM_LEN, VM_PAGE_SIZE);  // 长度页对齐

    return map_unmap_range(vmp, addr, len);
}
```

**注意事项**

1. `addr` 必须是页对齐的，否则返回 `EFAULT`
2. `len` 会被向上取整到页大小
3. 可以解除部分映射（区域分割）
4. 对未映射的地址调用 munmap 是允许的（静默忽略）

#### 2.2.3 返回结果

**成功返回**

```c
/* 成功返回 OK (0) */
return map_unmap_range(vmp, addr, len);  // 返回 OK
```

munmap 成功时返回 0。

**错误返回**

```c
/* minix3/minix/servers/vm/mmap.c */

int do_munmap(message *m)
{
    // ...

    if(addr % VM_PAGE_SIZE)
        return EFAULT;    // 地址未对齐

    // ...

    if(!(vr = map_lookup(vmp, addr, NULL))) {
        printf("VM: unmap: address 0x%lx not found in %d\n",
               addr, target);
        return EFAULT;    // 地址未找到
    }

    // ...
}
```

**错误码**

| 错误码 | 说明 |
|--------|------|
| `EFAULT` | 地址未页对齐或地址未映射 |
| `EINVAL` | 无效的进程 endpoint |

**返回值处理**

```c
/* minix3/minix/lib/libc/sys/mmap.c */

int munmap(void *addr, size_t len)
{
    message m;

    memset(&m, 0, sizeof(m));
    m.VMUM_ADDR = addr;
    m.VMUM_LEN = len;

    return _syscall(VM_PROC_NR, VM_MUNMAP, &m);
    // 成功返回 0，失败返回 -1 并设置 errno
}
```

**POSIX 语义**

根据 POSIX 标准，对未映射的地址调用 munmap 应该静默忽略。但 Minix3 实现会返回错误。

### 2.3 VM_MAP_PHYS

#### 2.3.1 调用者

VM_MAP_PHYS 的调用者主要是设备驱动程序，用于将硬件寄存器映射到虚拟地址空间。

**设备驱动程序**

```c
/* minix3/minix/lib/libsys/vm_map_phys.c */

void *vm_map_phys(endpoint_t who, void *phaddr, size_t len)
{
    message m;
    int r;

    memset(&m, 0, sizeof(m));
    m.m_lsys_vm_map_phys.ep = who;
    m.m_lsys_vm_map_phys.phaddr = (phys_bytes)phaddr;
    m.m_lsys_vm_map_phys.len = len;

    r = _syscall(VM_PROC_NR, VM_MAP_PHYS, &m);

    if(r != OK) {
        return MAP_FAILED;
    }

    return m.m_lsys_vm_map_phys.reply;
}
```

**典型调用场景**

```c
/* 网络驱动映射寄存器 */
/* minix3/minix/drivers/net/e1000/e1000.c */

if ((e->regs = vm_map_phys(SELF, (void *)base, size)) == MAP_FAILED)
    panic("e1000: vm_map_phys failed");

/* 显卡驱动映射帧缓冲 */
/* minix3/minix/drivers/tty/tty/arch/i386/console.c */

console_memory = vm_map_phys(SELF, (void *) vid_base, vid_size);

/* USB 控制器映射 */
/* minix3/minix/drivers/usb/usbd/hcd/hcd_common.c */

virt_reg_base = vm_map_phys(SELF, (void *)phys_addr, addr_len);
```

**权限检查**

```c
/* minix3/minix/servers/vm/mmap.c */

static int map_perm_check(endpoint_t caller, endpoint_t target,
    phys_bytes physaddr, phys_bytes len)
{
    /* TTY and MEM are allowed to do anything.
     * TTY even on behalf of anyone for the TIOCMAPMEM ioctl.
     * MEM just for itself.
     */
    if(caller == TTY_PROC_NR)
        return OK;
    if(caller == MEM_PROC_NR)
        return OK;

    /* Anyone else needs explicit permission from the kernel
     * (ultimately set by PCI).
     */
    return sys_privquery_mem(target, physaddr, len);
}
```

**调用者类型**

| 调用者 | 权限 | 用途 |
|--------|------|------|
| TTY | 完全 | 控制台帧缓冲 |
| MEM | 自身 | /dev/mem 实现 |
| PCI 驱动 | 授权设备范围 | 设备寄存器映射 |
| 网络驱动 | 授权设备范围 | NIC 寄存器 |
| 存储驱动 | 授权设备范围 | 控制器寄存器 |

#### 2.3.2 请求参数

**消息结构**

```c
/* minix3/minix/include/minix/ipc.h */

typedef struct {
    endpoint_t  ep;        // 目标进程（SELF 或其他）
    phys_bytes  phaddr;    // 物理地址
    size_t      len;       // 映射长度
    void        *reply;    // 返回的虚拟地址
    uint8_t     padding[40];
} mess_lsys_vm_map_phys;
```

**参数说明**

| 参数 | 类型 | 说明 |
|------|------|------|
| `ep` | `endpoint_t` | 目标进程，SELF 表示当前进程 |
| `phaddr` | `phys_bytes` | 要映射的物理地址 |
| `len` | `size_t` | 映射长度（字节） |

**参数处理**

```c
/* minix3/minix/servers/vm/mmap.c */

int do_map_phys(message *m)
{
    int r, n;
    struct vmproc *vmp;
    endpoint_t target;
    struct vir_region *vr;
    vir_bytes len;
    phys_bytes startaddr;
    size_t offset;

    target = m->m_lsys_vm_map_phys.ep;
    len = m->m_lsys_vm_map_phys.len;

    if (len <= 0) return EINVAL;

    if(target == SELF)
        target = m->m_source;

    if((r=vm_isokendpt(target, &n)) != OK)
        return EINVAL;

    startaddr = (vir_bytes)m->m_lsys_vm_map_phys.phaddr;

    // 权限检查
    if(map_perm_check(m->m_source, target, startaddr, len) != OK) {
        printf("VM: unauthorized mapping of 0x%lx by %d for %d\n",
            startaddr, m->m_source, target);
        return EPERM;
    }

    vmp = &vmproc[n];

    // 页对齐处理
    offset = startaddr % VM_PAGE_SIZE;
    len += offset;
    startaddr -= offset;

    if(len % VM_PAGE_SIZE)
        len += VM_PAGE_SIZE - (len % VM_PAGE_SIZE);

    // ...
}
```

**地址对齐**

```
请求: phaddr=0x1234, len=0x1000

处理:
  offset = 0x1234 % 0x1000 = 0x234
  startaddr = 0x1234 - 0x234 = 0x1000
  len = 0x1000 + 0x234 = 0x1234
  len = round_up(0x1234, 0x1000) = 0x2000

返回: vaddr + 0x234 (保留原始偏移)
```

#### 2.3.3 返回结果

**成功返回**

```c
/* minix3/minix/servers/vm/mmap.c */

if(!(vr = map_page_region(vmp, VM_MMAPBASE, VM_MMAPTOP, len,
    VR_DIRECT | VR_WRITABLE, 0, &mem_type_directphys))) {
    return ENOMEM;
}

phys_setphys(vr, startaddr);

m->m_lsys_vm_map_phys.reply = (void *) (vr->vaddr + offset);

return OK;
```

成功时返回映射的虚拟地址，通过 `reply` 字段传递。

**错误返回**

| 错误码 | 说明 |
|--------|------|
| `EINVAL` | len <= 0 或无效的 endpoint |
| `EPERM` | 无权限映射该物理地址 |
| `ENOMEM` | 地址空间不足 |

**返回值处理**

```c
/* minix3/minix/lib/libsys/vm_map_phys.c */

void *vm_map_phys(endpoint_t who, void *phaddr, size_t len)
{
    // ...
    r = _syscall(VM_PROC_NR, VM_MAP_PHYS, &m);

    if(r != OK) {
        return MAP_FAILED;  // 失败返回 MAP_FAILED
    }

    return m.m_lsys_vm_map_phys.reply;  // 成功返回虚拟地址
}
```

**映射示例**

```c
/* 映射物理地址 0xFE000000，长度 4KB */
void *vaddr = vm_map_phys(SELF, (void *)0xFE000000, 0x1000);
if (vaddr == MAP_FAILED) {
    perror("vm_map_phys failed");
    return -1;
}

/* 现在可以通过 vaddr 访问硬件寄存器 */
uint32_t reg = *(volatile uint32_t *)vaddr;
```

---

## 3. C 源码分析

### 3.1 do_mmap - 主处理函数

do_mmap 是 mmap 系统调用的核心处理函数。

**函数原型**

```c
/* minix3/minix/servers/vm/mmap.c */

int do_mmap(message *m)
```

**处理流程**

```c
int do_mmap(message *m)
{
    int r, n;
    struct vmproc *vmp;
    vir_bytes addr = (vir_bytes) m->m_mmap.addr;
    struct vir_region *vr = NULL;
    int execpriv = 0;
    size_t len = (vir_bytes) m->m_mmap.len;

    /* 1. 检查特权 */
    if(m->m_source == VFS_PROC_NR || m->m_source == RS_PROC_NR)
        execpriv = 1;

    /* 2. 确定目标进程 */
    if(m->m_mmap.flags & MAP_THIRDPARTY) {
        if(!execpriv) return EPERM;
        if((r=vm_isokendpt(m->m_mmap.forwhom, &n)) != OK)
            return ESRCH;
    } else {
        if((r=vm_isokendpt(m->m_source, &n)) != OK) {
            panic("do_mmap: message from strange source: %d",
                m->m_source);
        }
    }
    vmp = &vmproc[n];

    /* 3. 验证长度 */
    if(len <= 0) {
        return EINVAL;
    }

    /* 4. 根据映射类型处理 */
    if(m->m_mmap.fd == -1 || (m->m_mmap.flags & MAP_ANON)) {
        /* 匿名映射 */
        mem_type_t *mt = NULL;

        if(m->m_mmap.fd != -1) {
            return EINVAL;
        }

        if((m->m_mmap.flags & (MAP_CONTIG|MAP_PREALLOC)) == MAP_CONTIG) {
            return EINVAL;
        }

        if(m->m_mmap.flags & MAP_CONTIG) {
            mt = &mem_type_anon_contig;
        } else {
            mt = &mem_type_anon;
        }

        if(!(vr = mmap_region(vmp, addr, m->m_mmap.flags, len,
            VR_WRITABLE | VR_ANON, mt, execpriv))) {
            return ENOMEM;
        }
    } else {
        /* 文件映射 - 需要 VFS 协作 */
        if(!enable_filemap) return ENXIO;

        if((m->m_mmap.flags & MAP_SHARED) && 
           (m->m_mmap.prot & PROT_WRITE)) {
            return ENXIO;
        }

        if(vfs_request(VMVFSREQ_FDLOOKUP, m->m_mmap.fd, vmp, 0, 0,
            mmap_file_cont, NULL, m, sizeof(*m)) != OK) {
            return ENXIO;
        }

        return SUSPEND;  // 等待 VFS 回复
    }

    /* 5. 返回映射地址 */
    m->m_mmap.retaddr = (void *) vr->vaddr;

    return OK;
}
```

**流程图**

```
┌─────────────────────────────────────────────────────────────────┐
│                    do_mmap 处理流程                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ┌─────────────┐                                              │
│   │ 接收消息    │                                              │
│   └──────┬──────┘                                              │
│          ▼                                                      │
│   ┌─────────────┐                                              │
│   │ 检查特权    │ ← VFS/RS 有特权                              │
│   └──────┬──────┘                                              │
│          ▼                                                      │
│   ┌─────────────┐                                              │
│   │ 确定目标进程│ ← MAP_THIRDPARTY                             │
│   └──────┬──────┘                                              │
│          ▼                                                      │
│   ┌─────────────┐                                              │
│   │ 验证参数    │ ← len > 0                                    │
│   └──────┬──────┘                                              │
│          ▼                                                      │
│   ┌─────────────┐    fd == -1 或 MAP_ANON?                    │
│   │ 匿名映射?   │─────────────────────────┐                   │
│   └──────┬──────┘                          │                   │
│          │ 是                              │ 否                │
│          ▼                                  ▼                   │
│   ┌─────────────┐                   ┌─────────────┐           │
│   │ 创建匿名区域│                   │ 请求 VFS    │           │
│   └──────┬──────┘                   └──────┬──────┘           │
│          │                                  │                   │
│          │                                  ▼                   │
│          │                          ┌─────────────┐           │
│          │                          │ 等待 VFS    │           │
│          │                          │ 回复        │           │
│          │                          └──────┬──────┘           │
│          │                                  │                   │
│          │                                  ▼                   │
│          │                          ┌─────────────┐           │
│          │                          │ 创建文件区域│           │
│          │                          └──────┬──────┘           │
│          │                                  │                   │
│          ▼◀─────────────────────────────────┘                   │
│   ┌─────────────┐                                              │
│   │ 返回映射地址│                                              │
│   └─────────────┘                                              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 地址选择

#### 3.2.1 固定地址

MAP_FIXED 标志要求映射必须发生在指定地址。

**处理逻辑**

```c
/* minix3/minix/servers/vm/mmap.c */

static struct vir_region *mmap_region(struct vmproc *vmp, vir_bytes addr,
    u32_t vmm_flags, size_t len, u32_t vrflags,
    mem_type_t *mt, int execpriv)
{
    // ...

    if (addr && (vmm_flags & MAP_FIXED)) {
        /* MAP_FIXED: 先解除该地址范围的现有映射 */
        int r = map_unmap_range(vmp, addr, len);
        if(r != OK) {
            printf("mmap_region: map_unmap_range failed (%d)\n", r);
            return NULL;
        }
    }

    if (addr || (vmm_flags & MAP_FIXED)) {
        /* 尝试在指定地址创建映射 */
        vr = map_page_region(vmp, addr, 0, len, vrflags, mfflags, mt);
        if(!vr && (vmm_flags & MAP_FIXED))
            return NULL;  /* MAP_FIXED 失败则返回错误 */
    }

    // ...
}
```

**MAP_FIXED 行为**

| 情况 | 行为 |
|------|------|
| 地址可用 | 在指定地址创建映射 |
| 地址被占用 | 先解除现有映射，再创建新映射 |
| 地址无效 | 返回 NULL（失败） |

**注意事项**

1. MAP_FIXED 会覆盖现有映射，可能导致数据丢失
2. 地址必须是页对齐的
3. 如果指定地址无法使用，MAP_FIXED 会失败而不是选择其他地址

#### 3.2.2 自动选择

当没有指定地址或 MAP_FIXED 失败时，系统自动选择映射地址。

**自动选择逻辑**

```c
/* minix3/minix/servers/vm/mmap.c */

if (!vr) {
    /* 没有指定地址或指定地址已被占用 */
    vr = map_page_region(vmp, VM_MMAPBASE, VM_MMAPTOP, len,
        vrflags, mfflags, mt);
}
```

**地址范围**

```c
/* minix3/minix/include/machine/vmparam.h */

#define VM_MMAPBASE    0x40000000   /* mmap 区域起始 */
#define VM_MMAPTOP     0x70000000   /* mmap 区域结束 */
```

**查找算法**

```
┌─────────────────────────────────────────────────────────────────┐
│                    地址空间查找                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   VM_MMAPBASE                                                   │
│   ┌─────────────────────────────────────────────────────┐      │
│   │                                                     │      │
│   │   已有映射 A                                         │      │
│   │   ┌───────────┐                                     │      │
│   │   │           │                                     │      │
│   │   └───────────┘                                     │      │
│   │                                                     │      │
│   │   空闲区域                                          │      │
│   │   ┌───────────┐                                     │      │
│   │   │ 新映射    │ ← 找到足够大的空洞                  │      │
│   │   └───────────┘                                     │      │
│   │                                                     │      │
│   │   已有映射 B                                         │      │
│   │   ┌───────────┐                                     │      │
│   │   │           │                                     │      │
│   │   └───────────┘                                     │      │
│   │                                                     │      │
│   └─────────────────────────────────────────────────────┘      │
│   VM_MMAPTOP                                                    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**map_page_region 实现**

```c
/* minix3/minix/servers/vm/region.c */

struct vir_region *map_page_region(struct vmproc *vmp,
    vir_bytes minv, vir_bytes maxv, size_t len,
    u32_t vrflags, u32_t mfflags, mem_type_t *mt)
{
    struct vir_region *vr;

    /* 在指定范围内查找空闲区域 */
    vr = region_search_free(&vmp->vm_regions_avl, minv, maxv, len);

    if(!vr) {
        return NULL;  /* 没有找到足够大的空洞 */
    }

    /* 创建新的虚拟区域 */
    // ...
}
```

### 3.3 区域创建

#### 3.3.1 创建 vir_region

根据映射类型创建不同的虚拟区域。

**匿名映射**

```c
/* minix3/minix/servers/vm/mmap.c */

if(m->m_mmap.fd == -1 || (m->m_mmap.flags & MAP_ANON)) {
    mem_type_t *mt = NULL;

    if(m->m_mmap.flags & MAP_CONTIG) {
        mt = &mem_type_anon_contig;  /* 连续物理内存 */
    } else {
        mt = &mem_type_anon;         /* 普通匿名内存 */
    }

    if(!(vr = mmap_region(vmp, addr, m->m_mmap.flags, len,
        VR_WRITABLE | VR_ANON, mt, execpriv))) {
        return ENOMEM;
    }
}
```

**文件映射**

```c
/* 文件映射需要 VFS 协作 */
if(vfs_request(VMVFSREQ_FDLOOKUP, m->m_mmap.fd, vmp, 0, 0,
    mmap_file_cont, NULL, m, sizeof(*m)) != OK) {
    return ENXIO;
}
return SUSPEND;  /* 等待 VFS 回复 */
```

**物理内存映射**

```c
/* minix3/minix/servers/vm/mmap.c - do_map_phys */

if(!(vr = map_page_region(vmp, VM_MMAPBASE, VM_MMAPTOP, len,
    VR_DIRECT | VR_WRITABLE, 0, &mem_type_directphys))) {
    return ENOMEM;
}
phys_setphys(vr, startaddr);
```

#### 3.3.2 设置 mem_type

mem_type 决定了内存的分配和访问方式。

**内存类型定义**

```c
/* minix3/minix/servers/vm/mem_type.c */

mem_type_t mem_type_anon = {
    .name = "anonymous memory",
    .ev_alloc = anon_alloc,
    .ev_free = anon_free,
    .ev_unreference = anon_unreference,
    .ev_copy = anon_copy,
    .ev_resize = anon_resize,
};

mem_type_t mem_type_mappedfile = {
    .name = "mapped file",
    .ev_alloc = mappedfile_alloc,
    .ev_free = mappedfile_free,
    .ev_copy = mappedfile_copy,
};

mem_type_t mem_type_directphys = {
    .name = "direct physical",
    .ev_alloc = directphys_alloc,
    .ev_free = directphys_free,
};
```

**类型选择**

| 映射类型 | mem_type | 特点 |
|---------|----------|------|
| 匿名映射 | `mem_type_anon` | 按需分配，写时复制 |
| 连续匿名 | `mem_type_anon_contig` | 物理连续，DMA 用 |
| 文件映射 | `mem_type_mappedfile` | 按需从文件加载 |
| 物理映射 | `mem_type_directphys` | 直接映射物理地址 |

**区域标志**

```c
/* minix3/minix/servers/vm/region.h */

#define VR_ANON      0x01    /* 匿名内存 */
#define VR_WRITABLE  0x02    /* 可写 */
#define VR_DIRECT    0x04    /* 直接映射 */
#define VR_SHARED    0x08    /* 共享映射 */
```

### 3.4 文件映射

#### 3.4.1 VFS 交互

文件映射需要 VFS 提供文件信息。

**交互流程**

```c
/* minix3/minix/servers/vm/mmap.c */

/* VM 向 VFS 发送请求 */
if(vfs_request(VMVFSREQ_FDLOOKUP, m->m_mmap.fd, vmp, 0, 0,
    mmap_file_cont, NULL, m, sizeof(*m)) != OK) {
    return ENXIO;
}
return SUSPEND;  /* 等待 VFS 回复 */
```

**VFS 返回的信息**

```c
/* VFS 回复消息 */
replymsg->VMV_RESULT     /* 操作结果 */
replymsg->VMV_FD         /* 文件描述符 */
replymsg->VMV_INO        /* inode 号 */
replymsg->VMV_DEV        /* 设备号 */
replymsg->VMV_SIZE_PAGES /* 文件大小（页数） */
```

**交互流程图**

```
┌─────────────────────────────────────────────────────────────────┐
│                    文件映射 VFS 交互                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ┌─────────┐     VM_MMAP      ┌─────┐                         │
│   │ 用户进程 │ ────────────────▶│ VM  │                         │
│   └─────────┘                  └─────┘                         │
│                                     │                           │
│                                     │ VMVFSREQ_FDLOOKUP         │
│                                     ▼                           │
│                                 ┌─────────┐                     │
│                                 │   VFS   │                     │
│                                 └─────────┘                     │
│                                     │                           │
│                                     │ 文件信息:                 │
│                                     │ - fd, dev, ino            │
│                                     │ - size, offset            │
│                                     ▼                           │
│                                 ┌─────┐                         │
│                                 │ VM  │                         │
│                                 └─────┘                         │
│                                     │                           │
│                                     │ 创建文件映射              │
│                                     ▼                           │
│   ┌─────────┐     映射地址      ┌─────┐                         │
│   │ 用户进程 │ ◀────────────────│ VM  │                         │
│   └─────────┘                  └─────┘                         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**VFS 回调处理**

```c
/* minix3/minix/servers/vm/mmap.c */

static void mmap_file_cont(struct vmproc *vmp, message *replymsg, void *cbarg,
    void *origmsg_v)
{
    message *origmsg = (message *) origmsg_v;
    message mmap_reply;
    int result;
    vir_bytes v = (vir_bytes) MAP_FAILED;

    if(replymsg->VMV_RESULT != OK) {
        result = replymsg->VMV_RESULT;
    } else {
        /* 完成 mmap */
        result = mmap_file(vmp, replymsg->VMV_FD, origmsg->m_mmap.offset,
            origmsg->m_mmap.flags, 
            replymsg->VMV_INO, replymsg->VMV_DEV,
            (u64_t) replymsg->VMV_SIZE_PAGES*PAGE_SIZE,
            (vir_bytes) origmsg->m_mmap.addr,
            origmsg->m_mmap.len, &v, 0, writable, 1);
    }

    /* 解除进程阻塞 */
    memset(&mmap_reply, 0, sizeof(mmap_reply));
    mmap_reply.m_type = result;
    mmap_reply.m_mmap.retaddr = (void *) v;

    ipc_send(vmp->vm_endpoint, &mmap_reply);
}
```

#### 3.4.2 页缓存

文件映射与文件系统缓存的关系。

**按需加载**

```c
/* minix3/minix/servers/vm/mem_mappedfile.c */

/* 文件映射的缺页处理 */
static int mappedfile_pagefault(struct vir_region *region,
    struct phys_region *pr, int write)
{
    /* 从文件读取页面 */
    /* 这里使用 VFS 的页缓存机制 */
}
```

**缓存策略**

```
┌─────────────────────────────────────────────────────────────────┐
│                    文件映射与页缓存                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ┌─────────────────────────────────────────────────────┐      │
│   │ 文件映射                                             │      │
│   │                                                     │      │
│   │   虚拟地址空间                                       │      │
│   │   ┌───────┬───────┬───────┬───────┐                │      │
│   │   │ Page 0│ Page 1│ Page 2│ Page 3│                │      │
│   │   └───┬───┴───┬───┴───────┴───────┘                │      │
│   │       │       │                                      │      │
│   └───────┼───────┼──────────────────────────────────────┘      │
│           │       │                                              │
│           ▼       ▼                                              │
│   ┌─────────────────────────────────────────────────────┐      │
│   │ VFS 页缓存                                           │      │
│   │                                                     │      │
│   │   缓存的文件页面                                     │      │
│   │   ┌───────┬───────┬───────┬───────┐                │      │
│   │   │Cache 0│Cache 1│Cache 2│Cache 3│                │      │
│   │   └───────┴───────┴───────┴───────┘                │      │
│   │                                                     │      │
│   └─────────────────────────────────────────────────────┘      │
│           │       │                                              │
│           ▼       ▼                                              │
│   ┌─────────────────────────────────────────────────────┐      │
│   │ 磁盘文件                                             │      │
│   │                                                     │      │
│   │   ┌───────┬───────┬───────┬───────┐                │      │
│   │   │ Block │ Block │ Block │ Block │                │      │
│   │   └───────┴───────┴───────┴───────┘                │      │
│   │                                                     │      │
│   └─────────────────────────────────────────────────────┘      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**MAP_SHARED vs MAP_PRIVATE**

| 类型 | 行为 |
|------|------|
| MAP_SHARED | 修改写回文件，与其他映射共享 |
| MAP_PRIVATE | 写时复制，修改不影响文件 |

**Minix3 限制**

```c
/* minix3/minix/servers/vm/mmap.c */

/* Minix3 不支持可写的 MAP_SHARED 文件映射 */
if((m->m_mmap.flags & MAP_SHARED) && (m->m_mmap.prot & PROT_WRITE)) {
    return ENXIO;
}
```

### 3.5 do_munmap - 解除映射

#### 3.5.1 查找区域

根据地址查找要解除映射的区域。

**查找函数**

```c
/* minix3/minix/servers/vm/mmap.c */

int do_munmap(message *m)
{
    int r, n;
    struct vmproc *vmp;
    struct vir_region *vr;
    vir_bytes addr, len;

    // ...

    if(!(vr = map_lookup(vmp, addr, NULL))) {
        printf("VM: unmap: address 0x%lx not found in %d\n",
               addr, target);
        return EFAULT;
    }

    // ...
}
```

**map_lookup 实现**

```c
/* minix3/minix/servers/vm/region.c */

struct vir_region *map_lookup(struct vmproc *vmp, vir_bytes v,
    struct vir_region **prev)
{
    struct vir_region *vr;

    /* 在 AVL 树中查找包含地址 v 的区域 */
    vr = region_search(&vmp->vm_regions_avl, v, AVL_EQUAL);

    if(vr && vr->vaddr <= v && v < vr->vaddr + vr->length) {
        return vr;
    }

    return NULL;
}
```

**地址匹配规则**

```
┌─────────────────────────────────────────────────────────────────┐
│                    地址匹配                                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   区域: vaddr=0x1000, length=0x2000                             │
│   ┌───────────────────────────────────────┐                    │
│   │         已映射区域                     │                    │
│   │    0x1000                        0x3000                    │
│   └───────────────────────────────────────┘                    │
│                                                                 │
│   请求: addr=0x1500                                             │
│   ✓ 匹配成功 (0x1000 <= 0x1500 < 0x3000)                       │
│                                                                 │
│   请求: addr=0x0800                                             │
│   ✗ 匹配失败 (0x0800 < 0x1000)                                  │
│                                                                 │
│   请求: addr=0x3000                                             │
│   ✗ 匹配失败 (0x3000 >= 0x3000)                                 │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

#### 3.5.2 部分解除

当解除映射的范围只覆盖区域的一部分时，需要分割区域。

**分割场景**

```c
/* minix3/minix/servers/vm/region.c */

int map_unmap_range(struct vmproc *vmp, vir_bytes addr, vir_bytes len)
{
    struct vir_region *vr;

    /* 查找起始地址所在区域 */
    vr = map_lookup(vmp, addr, NULL);

    if(!vr) {
        return OK;  /* 没有找到区域，静默成功 */
    }

    /* 情况 1: 解除区域开头部分 */
    if(vr->vaddr == addr && len < vr->length) {
        /* 缩小区域，调整起始地址 */
        // ...
    }

    /* 情况 2: 解除区域结尾部分 */
    if(addr > vr->vaddr && addr + len >= vr->vaddr + vr->length) {
        /* 缩小区域，减小长度 */
        // ...
    }

    /* 情况 3: 解除区域中间部分 */
    if(addr > vr->vaddr && addr + len < vr->vaddr + vr->length) {
        /* 分割成两个区域 */
        // ...
    }

    /* 情况 4: 解除整个区域 */
    if(vr->vaddr == addr && len >= vr->length) {
        /* 完全释放 */
        // ...
    }
}
```

**分割示意图**

```
┌─────────────────────────────────────────────────────────────────┐
│                    区域分割                                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   原始区域:                                                     │
│   ┌───────────────────────────────────────────────┐            │
│   │              0x1000 - 0x4000                  │            │
│   └───────────────────────────────────────────────┘            │
│                                                                 │
│   情况 1: 解除开头 (munmap(0x1000, 0x1000))                    │
│   ┌───────────────────────────────────────────────┐            │
│   │ XXXXXXXX │        0x2000 - 0x4000             │            │
│   └───────────────────────────────────────────────┘            │
│             └─────────────────────────────────────┘ 新区域     │
│                                                                 │
│   情况 2: 解除结尾 (munmap(0x3000, 0x1000))                    │
│   ┌───────────────────────────────────────────────┐            │
│   │        0x1000 - 0x3000             │ XXXXXXXX │            │
│   └───────────────────────────────────────────────┘            │
│   └─────────────────────────────────────┘ 新区域               │
│                                                                 │
│   情况 3: 解除中间 (munmap(0x2000, 0x1000))                    │
│   ┌───────────────────────────────────────────────┐            │
│   │ 0x1000-0x2000 │ XXXXXXXX │ 0x3000-0x4000     │            │
│   └───────────────────────────────────────────────┘            │
│   └───────────────┘           └───────────────┘ 两个新区域    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

#### 3.5.3 完全解除

释放整个映射区域。

**释放流程**

```c
/* minix3/minix/servers/vm/region.c */

int map_unmap_region(struct vmproc *vmp, struct vir_region *vr,
    vir_bytes offset, vir_bytes len)
{
    /* 1. 从 AVL 树中移除 */
    region_remove(&vmp->vm_regions_avl, vr);

    /* 2. 释放物理页面 */
    for(i = 0; i < phys_slot(vr->length); i++) {
        struct phys_region *pr = vr->physblocks[i];
        if(pr) {
            /* 减少引用计数 */
            if(pr->ph->refcount > 0) {
                pr->ph->refcount--;
            }
            /* 如果引用计数为 0，释放物理页面 */
            if(pr->ph->refcount == 0) {
                free_phys_block(pr->ph);
            }
        }
    }

    /* 3. 释放区域结构 */
    free(vr->physblocks);
    free(vr);

    return OK;
}
```

**资源释放**

```
┌─────────────────────────────────────────────────────────────────┐
│                    完全解除映射                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   释放前:                                                       │
│   ┌───────────────────────────────────────────────┐            │
│   │ vir_region                                    │            │
│   │   vaddr = 0x1000                              │            │
│   │   length = 0x3000                             │            │
│   │   physblocks[0..2]                            │            │
│   └───────────────────────────────────────────────┘            │
│              │              │              │                    │
│              ▼              ▼              ▼                    │
│         ┌────────┐    ┌────────┐    ┌────────┐                │
│         │phys blk│    │phys blk│    │phys blk│                │
│         │ref=1   │    │ref=2   │    │ref=1   │                │
│         └────────┘    └────────┘    └────────┘                │
│                                                                 │
│   释放后:                                                       │
│         ┌────────┐    ┌────────┐    ┌────────┐                │
│         │freed   │    │ref=1   │    │freed   │                │
│         └────────┘    └────────┘    └────────┘                │
│              ↑                             ↑                    │
│              └─────────────────────────────┘                    │
│                    ref=0 的页面被释放                           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 3.6 do_map_phys - 物理内存映射

#### 3.6.1 权限检查

物理内存映射需要特权检查，防止普通进程随意访问硬件。

**权限检查函数**

```c
/* minix3/minix/servers/vm/mmap.c */

static int map_perm_check(endpoint_t caller, endpoint_t target,
    phys_bytes physaddr, phys_bytes len)
{
    /* TTY 和 MEM 可以做任何事 */
    if(caller == TTY_PROC_NR)
        return OK;
    if(caller == MEM_PROC_NR)
        return OK;

    /* 其他进程需要内核授权 */
    return sys_privquery_mem(target, physaddr, len);
}
```

**权限来源**

```
┌─────────────────────────────────────────────────────────────────┐
│                    物理映射权限                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ┌─────────────────────────────────────────────────────┐      │
│   │ 特权进程                                             │      │
│   │                                                     │      │
│   │   TTY (终端驱动)                                    │      │
│   │     - 可以映射任何物理地址                          │      │
│   │     - 用于帧缓冲访问                                │      │
│   │                                                     │      │
│   │   MEM (/dev/mem)                                    │      │
│   │     - 可以映射自身地址空间                          │      │
│   │     - 用于 /dev/mem 实现                            │      │
│   └─────────────────────────────────────────────────────┘      │
│                                                                 │
│   ┌─────────────────────────────────────────────────────┐      │
│   │ 授权进程                                             │      │
│   │                                                     │      │
│   │   PCI 设备驱动                                      │      │
│   │     - 由 PCI 子系统授权设备寄存器范围               │      │
│   │     - 通过 sys_privquery_mem() 检查                 │      │
│   │                                                     │      │
│   │   网络驱动、存储驱动等                              │      │
│   │     - 只能映射其控制的设备范围                      │      │
│   └─────────────────────────────────────────────────────┘      │
│                                                                 │
│   ┌─────────────────────────────────────────────────────┐      │
│   │ 普通进程                                             │      │
│   │                                                     │      │
│   │   不允许调用 vm_map_phys                            │      │
│   │   返回 EPERM                                        │      │
│   └─────────────────────────────────────────────────────┘      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**内核授权机制**

```c
/* 内核检查进程是否有权访问指定物理地址 */
int sys_privquery_mem(endpoint_t ep, phys_bytes addr, phys_bytes len)
{
    /* 检查进程的 I/O 权限位图 */
    /* 由 PCI 驱动在设备枚举时设置 */
}
```

#### 3.6.2 直接映射

使用 VR_DIRECT 标志创建物理内存直接映射。

**do_map_phys 实现**

```c
/* minix3/minix/servers/vm/mmap.c */

int do_map_phys(message *m)
{
    int r, n;
    struct vmproc *vmp;
    struct vir_region *vr;
    vir_bytes len;
    phys_bytes startaddr;

    /* 1. 确定目标进程 */
    if((r = vm_isokendpt(m->m_source, &n)) != OK) {
        panic("do_map_phys: message from strange source: %d", m->m_source);
    }
    vmp = &vmproc[n];

    /* 2. 获取参数 */
    startaddr = (phys_bytes) m->m_lsys_vm_map_phys.phys_addr;
    len = (vir_bytes) m->m_lsys_vm_map_phys.len;

    /* 3. 权限检查 */
    if(map_perm_check(m->m_source, vmp->vm_endpoint, startaddr, len) != OK) {
        return EPERM;
    }

    /* 4. 创建直接映射区域 */
    if(!(vr = map_page_region(vmp, VM_MMAPBASE, VM_MMAPTOP, len,
        VR_DIRECT | VR_WRITABLE, 0, &mem_type_directphys))) {
        return ENOMEM;
    }

    /* 5. 设置物理地址 */
    phys_setphys(vr, startaddr);

    /* 6. 返回映射地址 */
    m->m_lsys_vm_map_phys.ret_addr = (void *) vr->vaddr;

    return OK;
}
```

**VR_DIRECT 特性**

```
┌─────────────────────────────────────────────────────────────────┐
│                    直接物理映射                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   普通映射 (匿名/文件):                                         │
│   ┌───────────────────────────────────────────────┐            │
│   │ 虚拟地址                                       │            │
│   │   0x40000000                                  │            │
│   └───────────────────────────────────────────────┘            │
│              │                                                  │
│              │ 页表映射                                         │
│              ▼                                                  │
│   ┌───────────────────────────────────────────────┐            │
│   │ 物理页面 (动态分配)                            │            │
│   │   由 VM 管理的普通内存                         │            │
│   └───────────────────────────────────────────────┘            │
│                                                                 │
│   直接映射 (VR_DIRECT):                                         │
│   ┌───────────────────────────────────────────────┐            │
│   │ 虚拟地址                                       │            │
│   │   0x40000000                                  │            │
│   └───────────────────────────────────────────────┘            │
│              │                                                  │
│              │ 直接映射 (无额外分配)                            │
│              ▼                                                  │
│   ┌───────────────────────────────────────────────┐            │
│   │ 设备寄存器 / 物理内存                          │            │
│   │   0xFEC00000 (I/O APIC)                       │            │
│   │   0xA0000 (VGA 帧)                            │            │
│   └───────────────────────────────────────────────┘            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**mem_type_directphys**

```c
/* minix3/minix/servers/vm/mem_directphys.c */

mem_type_t mem_type_directphys = {
    .name = "direct physical",
    .ev_alloc = directphys_alloc,      /* 无需分配 */
    .ev_free = directphys_free,        /* 无需释放 */
    .ev_pagefault = directphys_pagefault,  /* 直接映射 */
};
```

---

## 4. Rust 设计决策

### 4.1 MmapRequest/MmapResponse

类型安全的 IPC 消息结构。

**MmapRequest**

```rust
use crate::ipc::{Endpoint, Message};
use crate::vm::MmapFlags;

#[derive(Debug, Clone)]
pub struct MmapRequest {
    pub addr: Option<VirtualAddress>,
    pub length: usize,
    pub prot: ProtectionFlags,
    pub flags: MmapFlags,
    pub fd: Option<FileDescriptor>,
    pub offset: u64,
}

impl MmapRequest {
    pub fn is_anonymous(&self) -> bool {
        self.flags.contains(MmapFlags::MAP_ANONYMOUS) || self.fd.is_none()
    }

    pub fn is_fixed(&self) -> bool {
        self.flags.contains(MmapFlags::MAP_FIXED)
    }
}
```

**MmapResponse**

```rust
#[derive(Debug, Clone)]
pub enum MmapResponse {
    Success {
        addr: VirtualAddress,
    },
    Error(MmapError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmapError {
    InvalidArgument,
    PermissionDenied,
    OutOfMemory,
    InvalidAddress,
    FileMappingNotSupported,
}
```

**MunmapRequest/MunmapResponse**

```rust
#[derive(Debug, Clone)]
pub struct MunmapRequest {
    pub addr: VirtualAddress,
    pub length: usize,
}

#[derive(Debug, Clone)]
pub enum MunmapResponse {
    Success,
    Error(MunmapError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MunmapError {
    InvalidArgument,
    NotMapped,
}
```

**MapPhysRequest/MapPhysResponse**

```rust
#[derive(Debug, Clone)]
pub struct MapPhysRequest {
    pub phys_addr: PhysicalAddress,
    pub length: usize,
}

#[derive(Debug, Clone)]
pub enum MapPhysResponse {
    Success {
        virt_addr: VirtualAddress,
    },
    Error(MapPhysError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapPhysError {
    PermissionDenied,
    OutOfMemory,
    InvalidAddress,
}
```

### 4.2 映射标志

使用 bitflags 定义类型安全的映射标志。

**MmapFlags**

```rust
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MmapFlags: u32 {
        const MAP_SHARED        = 0x01;
        const MAP_PRIVATE       = 0x02;
        const MAP_FIXED         = 0x10;
        const MAP_ANONYMOUS     = 0x20;
        const MAP_CONTIG        = 0x40;
        const MAP_PREALLOC      = 0x80;
    }
}

impl MmapFlags {
    pub fn is_valid_combination(&self) -> bool {
        let shared_or_private = self.contains(Self::MAP_SHARED) 
            ^ self.contains(Self::MAP_PRIVATE);
        shared_or_private
    }
}
```

**ProtectionFlags**

```rust
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ProtectionFlags: u32 {
        const PROT_NONE   = 0x00;
        const PROT_READ   = 0x01;
        const PROT_WRITE  = 0x02;
        const PROT_EXEC   = 0x04;
    }
}

impl ProtectionFlags {
    pub fn to_page_flags(&self) -> PageFlags {
        let mut flags = PageFlags::empty();
        if self.contains(Self::PROT_READ) {
            flags |= PageFlags::READ;
        }
        if self.contains(Self::PROT_WRITE) {
            flags |= PageFlags::WRITE;
        }
        if self.contains(Self::PROT_EXEC) {
            flags |= PageFlags::EXECUTE;
        }
        flags
    }
}
```

**标志对应关系**

| POSIX 标志 | Minix3 标志 | Rust 标志 | 说明 |
|-----------|-------------|-----------|------|
| MAP_SHARED | MAP_SHARED | MAP_SHARED | 共享映射 |
| MAP_PRIVATE | MAP_PRIVATE | MAP_PRIVATE | 私有映射 |
| MAP_FIXED | MAP_FIXED | MAP_FIXED | 固定地址 |
| MAP_ANONYMOUS | MAP_ANON | MAP_ANONYMOUS | 匿名映射 |
| - | MAP_CONTIG | MAP_CONTIG | 连续物理内存 |
| - | MAP_PREALLOC | MAP_PREALLOC | 预分配 |

### 4.3 错误处理

统一的错误处理策略。

**VmMapError**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmMapError {
    InvalidArgument,
    InvalidAddress,
    InvalidLength,
    AddressNotMapped,
    PermissionDenied,
    OutOfMemory,
    FileMappingNotSupported,
    VfsError(VfsError),
    KernelError(KernelError),
}

impl From<VmMapError> for i32 {
    fn from(err: VmMapError) -> i32 {
        match err {
            VmMapError::InvalidArgument => 22,      // EINVAL
            VmMapError::InvalidAddress => 14,       // EFAULT
            VmMapError::InvalidLength => 22,        // EINVAL
            VmMapError::AddressNotMapped => 14,     // EFAULT
            VmMapError::PermissionDenied => 1,      // EPERM
            VmMapError::OutOfMemory => 12,          // ENOMEM
            VmMapError::FileMappingNotSupported => 6, // ENXIO
            VmMapError::VfsError(_) => 5,           // EIO
            VmMapError::KernelError(_) => 5,        // EIO
        }
    }
}
```

**错误场景**

```rust
impl VmHandler {
    pub fn handle_mmap(&mut self, req: &MmapRequest) -> Result<VirtualAddress, VmMapError> {
        if req.length == 0 {
            return Err(VmMapError::InvalidLength);
        }

        if !req.flags.is_valid_combination() {
            return Err(VmMapError::InvalidArgument);
        }

        if let Some(addr) = req.addr {
            if !addr.is_page_aligned() {
                if req.is_fixed() {
                    return Err(VmMapError::InvalidAddress);
                }
            }
        }

        if req.is_fixed() {
            if let Some(addr) = req.addr {
                if !self.is_valid_mmap_range(addr, req.length) {
                    return Err(VmMapError::InvalidAddress);
                }
            }
        }

        self.find_or_create_mapping(req)
    }
}
```

**错误处理流程**

```
┌─────────────────────────────────────────────────────────────────┐
│                    错误处理流程                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   mmap 请求                                                     │
│       │                                                         │
│       ▼                                                         │
│   ┌─────────────────┐                                          │
│   │ 参数验证        │                                          │
│   └────────┬────────┘                                          │
│            │                                                    │
│       失败 │ 成功                                               │
│            ▼                                                    │
│   ┌─────────────────┐    ┌─────────────────┐                   │
│   │ EINVAL          │    │ 权限检查        │                   │
│   └─────────────────┘    └────────┬────────┘                   │
│                                   │                             │
│                              失败 │ 成功                        │
│                                   ▼                             │
│                          ┌─────────────────┐                   │
│                          │ EPERM           │    ┌───────────┐  │
│                          └─────────────────┘    │ 地址查找  │  │
│                                                  └─────┬─────┘  │
│                                                        │        │
│                                                   失败 │ 成功   │
│                                                        ▼        │
│                                                ┌───────────┐    │
│                                                │ ENOMEM    │    │
│                                                └───────────┘    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 5. 实现详解

### 5.1 消息处理入口

VM 服务消息分发处理。

**VmHandler 消息分发**

```rust
impl VmHandler {
    pub fn handle_message(&mut self, msg: &Message) -> Result<Message, VmError> {
        match msg.message_type() {
            MessageType::VmMap => self.handle_mmap_message(msg),
            MessageType::VmUnmap => self.handle_munmap_message(msg),
            MessageType::VmMapPhys => self.handle_map_phys_message(msg),
            MessageType::VmUnmapPhys => self.handle_unmap_phys_message(msg),
            _ => Err(VmError::UnknownMessageType),
        }
    }

    fn handle_mmap_message(&mut self, msg: &Message) -> Result<Message, VmError> {
        let request: MmapRequest = msg.try_into()?;
        let result = self.do_mmap(&request);
        
        let response = match result {
            Ok(addr) => MmapResponse::Success { addr },
            Err(e) => MmapResponse::Error(e),
        };
        
        Ok(Message::from(response))
    }

    fn handle_munmap_message(&mut self, msg: &Message) -> Result<Message, VmError> {
        let request: MunmapRequest = msg.try_into()?;
        let result = self.do_munmap(&request);
        
        let response = match result {
            Ok(()) => MunmapResponse::Success,
            Err(e) => MunmapResponse::Error(e),
        };
        
        Ok(Message::from(response))
    }
}
```

**do_mmap 实现**

```rust
impl VmHandler {
    pub fn do_mmap(&mut self, req: &MmapRequest) -> Result<VirtualAddress, MmapError> {
        let process = self.process_table.find_by_endpoint(req.caller)?;
        
        if req.length == 0 {
            return Err(MmapError::InvalidLength);
        }

        let length = round_up_to_page(req.length);

        let region = if req.is_anonymous() {
            self.create_anonymous_mapping(&process, req, length)?
        } else {
            self.create_file_mapping(&process, req, length)?
        };

        Ok(region.start_address())
    }
}
```

### 5.2 地址空间查找

在进程地址空间中查找合适的空闲区域。

**查找空闲区域**

```rust
impl AddressSpace {
    const MMAP_BASE: VirtualAddress = VirtualAddress::new(0x4000_0000);
    const MMAP_TOP: VirtualAddress = VirtualAddress::new(0x7000_0000);

    pub fn find_free_region(
        &self,
        hint: Option<VirtualAddress>,
        length: usize,
        flags: MmapFlags,
    ) -> Result<VirtualAddress, MmapError> {
        let length = round_up_to_page(length);

        if let Some(addr) = hint {
            if flags.contains(MmapFlags::MAP_FIXED) {
                self.unmap_range(addr, length)?;
                return Ok(addr);
            }

            if let Some(found) = self.try_address(addr, length) {
                return Ok(found);
            }
        }

        self.find_free_in_range(Self::MMAP_BASE, Self::MMAP_TOP, length)
            .ok_or(MmapError::OutOfMemory)
    }

    fn try_address(&self, addr: VirtualAddress, length: usize) -> Option<VirtualAddress> {
        if self.is_range_free(addr, length) {
            Some(addr)
        } else {
            None
        }
    }

    fn find_free_in_range(
        &self,
        start: VirtualAddress,
        end: VirtualAddress,
        length: usize,
    ) -> Option<VirtualAddress> {
        self.regions.find_gap(start, end, length)
    }
}
```

**AVL 树查找间隙**

```rust
impl<V: VirtualRegion> RegionTree<V> {
    pub fn find_gap(
        &self,
        start: VirtualAddress,
        end: VirtualAddress,
        length: usize,
    ) -> Option<VirtualAddress> {
        let mut current_addr = start;

        for region in self.iter() {
            if current_addr + length <= region.start() {
                return Some(current_addr);
            }
            current_addr = region.end();
            if current_addr >= end {
                return None;
            }
        }

        if current_addr + length <= end {
            Some(current_addr)
        } else {
            None
        }
    }
}
```

### 5.3 区域创建与插入

创建虚拟区域并插入 AVL 树。

**区域创建**

```rust
impl AddressSpace {
    pub fn create_region(
        &mut self,
        addr: VirtualAddress,
        length: usize,
        flags: RegionFlags,
        mem_type: MemoryType,
    ) -> Result<Arc<VirtualRegion>, MmapError> {
        let region = VirtualRegion::new(addr, length, flags, mem_type);

        self.regions.insert(region.clone())?;

        Ok(region)
    }
}

pub struct VirtualRegion {
    start: VirtualAddress,
    length: usize,
    flags: RegionFlags,
    mem_type: MemoryType,
    phys_blocks: Vec<Option<Arc<PhysBlock>>>,
}

impl VirtualRegion {
    pub fn new(
        start: VirtualAddress,
        length: usize,
        flags: RegionFlags,
        mem_type: MemoryType,
    ) -> Arc<Self> {
        let page_count = length / PAGE_SIZE;
        Arc::new(Self {
            start,
            length,
            flags,
            mem_type,
            phys_blocks: vec![None; page_count],
        })
    }
}
```

**AVL 树插入**

```rust
impl<V: VirtualRegion> RegionTree<V> {
    pub fn insert(&mut self, region: Arc<V>) -> Result<(), MmapError> {
        if self.overlaps(&region) {
            return Err(MmapError::InvalidAddress);
        }

        self.root = self.insert_node(self.root.take(), region);
        Ok(())
    }

    fn insert_node(
        &self,
        node: Option<Box<Node<V>>>,
        region: Arc<V>,
    ) -> Option<Box<Node<V>>> {
        match node {
            None => Some(Box::new(Node::new(region))),
            Some(mut n) => {
                if region.start() < n.region.start() {
                    n.left = self.insert_node(n.left.take(), region);
                } else {
                    n.right = self.insert_node(n.right.take(), region);
                }
                n.update_height();
                Some(self.balance(n))
            }
        }
    }
}
```

### 5.4 页表映射

按需映射或预映射页面。

**按需映射**

```rust
impl VirtualRegion {
    pub fn handle_page_fault(
        &mut self,
        fault_addr: VirtualAddress,
        write: bool,
    ) -> Result<(), PageFaultError> {
        let page_index = self.page_index(fault_addr)?;

        if self.phys_blocks[page_index].is_none() {
            let phys_block = self.mem_type.allocate_page()?;
            self.phys_blocks[page_index] = Some(phys_block);
        }

        let phys_block = self.phys_blocks[page_index].as_ref().unwrap();

        let mut flags = PageFlags::PRESENT | PageFlags::USER;
        if self.flags.contains(RegionFlags::WRITABLE) && write {
            flags |= PageFlags::WRITABLE;
        }
        if !self.flags.contains(RegionFlags::WRITABLE) {
            flags |= PageFlags::NO_EXECUTE;
        }

        self.map_page(fault_addr, phys_block.phys_addr(), flags)?;

        Ok(())
    }
}
```

**预映射 (MAP_PREALLOC)**

```rust
impl VirtualRegion {
    pub fn preallocate_pages(&mut self) -> Result<(), MmapError> {
        for i in 0..self.phys_blocks.len() {
            if self.phys_blocks[i].is_none() {
                let phys_block = self.mem_type.allocate_page()?;
                
                let virt_addr = self.start + i * PAGE_SIZE;
                self.map_page(virt_addr, phys_block.phys_addr(), self.page_flags())?;
                
                self.phys_blocks[i] = Some(phys_block);
            }
        }
        Ok(())
    }
}
```

**页表映射接口**

```rust
pub trait PageTableMapper {
    fn map_page(
        &mut self,
        virt_addr: VirtualAddress,
        phys_addr: PhysicalAddress,
        flags: PageFlags,
    ) -> Result<(), MapError>;

    fn unmap_page(&mut self, virt_addr: VirtualAddress) -> Result<PhysicalAddress, UnmapError>;

    fn protect_page(
        &mut self,
        virt_addr: VirtualAddress,
        flags: PageFlags,
    ) -> Result<(), MapError>;
}
```

### 5.5 解除映射处理

区域分割和释放。

**解除映射**

```rust
impl AddressSpace {
    pub fn unmap_range(
        &mut self,
        addr: VirtualAddress,
        length: usize,
    ) -> Result<(), MunmapError> {
        let length = round_up_to_page(length);
        let end_addr = addr + length;

        while let Some(region) = self.regions.find_containing(addr) {
            if addr <= region.start() && end_addr >= region.end() {
                self.unmap_entire_region(region)?;
            } else if addr > region.start() && end_addr < region.end() {
                self.split_region(region, addr, length)?;
            } else if addr > region.start() {
                self.trim_region_end(region, addr)?;
            } else {
                self.trim_region_start(region, end_addr)?;
            }
        }

        Ok(())
    }
}
```

**区域分割**

```rust
impl AddressSpace {
    fn split_region(
        &mut self,
        region: Arc<VirtualRegion>,
        split_addr: VirtualAddress,
        split_length: usize,
    ) -> Result<(), MunmapError> {
        self.regions.remove(&region);

        let left_length = (split_addr - region.start()) as usize;
        let right_start = split_addr + split_length;
        let right_length = (region.end() - right_start) as usize;

        if left_length > 0 {
            let left_region = region.split_left(left_length)?;
            self.regions.insert(left_region);
        }

        if right_length > 0 {
            let right_region = region.split_right(right_start, right_length)?;
            self.regions.insert(right_region);
        }

        region.release_pages(split_addr, split_length)?;

        Ok(())
    }
}
```

**区域释放**

```rust
impl VirtualRegion {
    pub fn release(&self) {
        for block in &self.phys_blocks {
            if let Some(phys_block) = block {
                phys_block.decrement_refcount();
            }
        }
    }
}

impl PhysBlock {
    pub fn decrement_refcount(&self) {
        if self.refcount.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.free();
        }
    }

    fn free(&self) {
        FRAME_ALLOCATOR.lock().free(self.phys_addr);
    }
}
```

---

## 6. 内存类型与映射

### 6.1 匿名映射

匿名内存类型实现。

**MemoryType trait**

```rust
pub trait MemoryType: Send + Sync {
    fn name(&self) -> &'static str;

    fn allocate_page(&self) -> Result<Arc<PhysBlock>, AllocError>;

    fn free_page(&self, block: &PhysBlock);

    fn handle_page_fault(
        &self,
        region: &VirtualRegion,
        page_index: usize,
        write: bool,
    ) -> Result<(), PageFaultError>;

    fn copy_for_fork(
        &self,
        region: &VirtualRegion,
        child_space: &mut AddressSpace,
    ) -> Result<Arc<VirtualRegion>, ForkError>;
}
```

**AnonymousMemory 实现**

```rust
pub struct AnonymousMemory {
    allocator: Arc<FrameAllocator>,
}

impl MemoryType for AnonymousMemory {
    fn name(&self) -> &'static str {
        "anonymous"
    }

    fn allocate_page(&self) -> Result<Arc<PhysBlock>, AllocError> {
        let frame = self.allocator.allocate()?;
        Ok(Arc::new(PhysBlock::new(frame)))
    }

    fn free_page(&self, block: &PhysBlock) {
        if block.refcount() == 0 {
            self.allocator.free(block.phys_addr());
        }
    }

    fn handle_page_fault(
        &self,
        region: &VirtualRegion,
        page_index: usize,
        write: bool,
    ) -> Result<(), PageFaultError> {
        if region.phys_blocks[page_index].is_none() {
            let block = self.allocate_page()?;
            region.phys_blocks[page_index] = Some(block);
        }
        Ok(())
    }

    fn copy_for_fork(
        &self,
        region: &VirtualRegion,
        child_space: &mut AddressSpace,
    ) -> Result<Arc<VirtualRegion>, ForkError> {
        let child_region = VirtualRegion::new(
            region.start,
            region.length,
            region.flags,
            MemoryTypeEnum::Anonymous,
        );

        for (i, block) in region.phys_blocks.iter().enumerate() {
            if let Some(b) = block {
                b.increment_refcount();
                child_region.phys_blocks[i] = Some(b.clone());
            }
        }

        Ok(child_region)
    }
}
```

### 6.2 文件映射

文件映射内存类型实现。

**MappedFileMemory 实现**

```rust
pub struct MappedFileMemory {
    allocator: Arc<FrameAllocator>,
    vfs_client: Arc<VfsClient>,
}

impl MappedFileMemory {
    pub fn new(allocator: Arc<FrameAllocator>, vfs_client: Arc<VfsClient>) -> Self {
        Self { allocator, vfs_client }
    }
}

impl MemoryType for MappedFileMemory {
    fn name(&self) -> &'static str {
        "mapped file"
    }

    fn allocate_page(&self) -> Result<Arc<PhysBlock>, AllocError> {
        let frame = self.allocator.allocate()?;
        Ok(Arc::new(PhysBlock::new(frame)))
    }

    fn free_page(&self, block: &PhysBlock) {
        if block.refcount() == 0 {
            self.allocator.free(block.phys_addr());
        }
    }

    fn handle_page_fault(
        &self,
        region: &VirtualRegion,
        page_index: usize,
        write: bool,
    ) -> Result<(), PageFaultError> {
        if region.phys_blocks[page_index].is_none() {
            let block = self.allocate_page()?;
            
            let file_offset = region.file_offset + (page_index * PAGE_SIZE) as u64;
            self.vfs_client.read_page(
                region.file_handle,
                file_offset,
                block.phys_addr(),
            )?;
            
            region.phys_blocks[page_index] = Some(block);
        }
        Ok(())
    }

    fn copy_for_fork(
        &self,
        region: &VirtualRegion,
        child_space: &mut AddressSpace,
    ) -> Result<Arc<VirtualRegion>, ForkError> {
        let child_region = VirtualRegion::new(
            region.start,
            region.length,
            region.flags,
            MemoryTypeEnum::MappedFile,
        );

        child_region.file_handle = region.file_handle;
        child_region.file_offset = region.file_offset;

        for (i, block) in region.phys_blocks.iter().enumerate() {
            if let Some(b) = block {
                b.increment_refcount();
                child_region.phys_blocks[i] = Some(b.clone());
            }
        }

        Ok(child_region)
    }
}
```

**文件映射参数**

```rust
pub struct FileMappingParams {
    pub file_handle: FileHandle,
    pub file_offset: u64,
    pub file_size: u64,
}
```

### 6.3 物理内存映射

直接物理内存映射类型实现。

**DirectPhysicalMemory 实现**

```rust
pub struct DirectPhysicalMemory;

impl MemoryType for DirectPhysicalMemory {
    fn name(&self) -> &'static str {
        "direct physical"
    }

    fn allocate_page(&self) -> Result<Arc<PhysBlock>, AllocError> {
        Ok(Arc::new(PhysBlock::new_direct()))
    }

    fn free_page(&self, _block: &PhysBlock) {
        // 直接映射不需要释放物理页面
    }

    fn handle_page_fault(
        &self,
        region: &VirtualRegion,
        page_index: usize,
        write: bool,
    ) -> Result<(), PageFaultError> {
        let phys_addr = region.phys_base + (page_index * PAGE_SIZE);
        
        let block = PhysBlock::new_direct_mapped(phys_addr);
        region.phys_blocks[page_index] = Some(Arc::new(block));
        
        Ok(())
    }

    fn copy_for_fork(
        &self,
        region: &VirtualRegion,
        child_space: &mut AddressSpace,
    ) -> Result<Arc<VirtualRegion>, ForkError> {
        let child_region = VirtualRegion::new(
            region.start,
            region.length,
            region.flags,
            MemoryTypeEnum::DirectPhysical,
        );

        child_region.phys_base = region.phys_base;

        for (i, block) in region.phys_blocks.iter().enumerate() {
            child_region.phys_blocks[i] = block.clone();
        }

        Ok(child_region)
    }
}
```

**直接映射区域**

```rust
pub struct DirectPhysicalRegion {
    phys_base: PhysicalAddress,
}

impl VirtualRegion {
    pub fn set_direct_physical(&mut self, phys_addr: PhysicalAddress) {
        self.phys_base = phys_addr;
        self.flags |= RegionFlags::DIRECT;
    }
}
```

**使用场景**

```
┌─────────────────────────────────────────────────────────────────┐
│                    物理内存映射使用场景                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   1. 设备寄存器映射                                             │
│      - VGA 帧缓冲区 (0xA0000)                                   │
│      - PCI 配置空间                                             │
│      - APIC 寄存器                                              │
│                                                                 │
│   2. DMA 缓冲区                                                 │
│      - 网络驱动 DMA 区域                                        │
│      - 存储驱动 DMA 区域                                        │
│                                                                 │
│   3. 共享内存                                                   │
│      - 进程间共享物理内存                                       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 7. 测试与验证

### 7.1 匿名映射测试

测试匿名映射功能。

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anonymous_mmap_basic() {
        let mut vm = VmHandler::new_test();
        let process = vm.create_test_process();

        let request = MmapRequest {
            addr: None,
            length: 4096,
            prot: ProtectionFlags::PROT_READ | ProtectionFlags::PROT_WRITE,
            flags: MmapFlags::MAP_ANONYMOUS | MmapFlags::MAP_PRIVATE,
            fd: None,
            offset: 0,
        };

        let result = vm.do_mmap(&request);
        assert!(result.is_ok());

        let addr = result.unwrap();
        assert!(addr.is_page_aligned());
        assert!(addr >= AddressSpace::MMAP_BASE);
        assert!(addr < AddressSpace::MMAP_TOP);
    }

    #[test]
    fn test_anonymous_mmap_fixed() {
        let mut vm = VmHandler::new_test();
        let process = vm.create_test_process();

        let fixed_addr = VirtualAddress::new(0x5000_0000);

        let request = MmapRequest {
            addr: Some(fixed_addr),
            length: 4096,
            prot: ProtectionFlags::PROT_READ | ProtectionFlags::PROT_WRITE,
            flags: MmapFlags::MAP_ANONYMOUS | MmapFlags::MAP_PRIVATE | MmapFlags::MAP_FIXED,
            fd: None,
            offset: 0,
        };

        let result = vm.do_mmap(&request);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), fixed_addr);
    }

    #[test]
    fn test_anonymous_mmap_multiple() {
        let mut vm = VmHandler::new_test();
        let process = vm.create_test_process();

        let mut addresses = Vec::new();

        for _ in 0..10 {
            let request = MmapRequest {
                addr: None,
                length: 4096,
                prot: ProtectionFlags::PROT_READ | ProtectionFlags::PROT_WRITE,
                flags: MmapFlags::MAP_ANONYMOUS | MmapFlags::MAP_PRIVATE,
                fd: None,
                offset: 0,
            };

            let addr = vm.do_mmap(&request).unwrap();
            addresses.push(addr);
        }

        for i in 0..addresses.len() {
            for j in (i + 1)..addresses.len() {
                assert_ne!(addresses[i], addresses[j]);
            }
        }
    }
}
```

### 7.2 文件映射测试

测试文件映射功能。

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_mmap_readonly() {
        let mut vm = VmHandler::new_test();
        let process = vm.create_test_process();
        let file = vm.create_test_file(b"Hello, World!");

        let request = MmapRequest {
            addr: None,
            length: 4096,
            prot: ProtectionFlags::PROT_READ,
            flags: MmapFlags::MAP_PRIVATE,
            fd: Some(file.fd),
            offset: 0,
        };

        let result = vm.do_mmap(&request);
        assert!(result.is_ok());

        let addr = result.unwrap();
        let data = vm.read_memory(process, addr, 13);
        assert_eq!(&data[..], b"Hello, World!");
    }

    #[test]
    fn test_file_mmap_offset() {
        let mut vm = VmHandler::new_test();
        let process = vm.create_test_process();
        let file = vm.create_test_file(b"0123456789ABCDEF");

        let request = MmapRequest {
            addr: None,
            length: 4096,
            prot: ProtectionFlags::PROT_READ,
            flags: MmapFlags::MAP_PRIVATE,
            fd: Some(file.fd),
            offset: 8,
        };

        let result = vm.do_mmap(&request);
        assert!(result.is_ok());

        let addr = result.unwrap();
        let data = vm.read_memory(process, addr, 8);
        assert_eq!(&data[..], b"ABCDEF");
    }

    #[test]
    fn test_file_mmap_private_copy_on_write() {
        let mut vm = VmHandler::new_test();
        let process = vm.create_test_process();
        let file = vm.create_test_file(b"Original Data");

        let request = MmapRequest {
            addr: None,
            length: 4096,
            prot: ProtectionFlags::PROT_READ | ProtectionFlags::PROT_WRITE,
            flags: MmapFlags::MAP_PRIVATE,
            fd: Some(file.fd),
            offset: 0,
        };

        let addr = vm.do_mmap(&request).unwrap();

        vm.write_memory(process, addr, b"Modified   ");

        let data = vm.read_file(file.fd, 0, 13);
        assert_eq!(&data[..], b"Original Data");
    }
}
```

### 7.3 物理映射测试

测试物理内存映射功能。

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_phys_privileged() {
        let mut vm = VmHandler::new_test();
        let process = vm.create_privileged_process(Endpoint::TTY);

        let request = MapPhysRequest {
            phys_addr: PhysicalAddress::new(0xA0000),
            length: 4096,
        };

        let result = vm.do_map_phys(&request);
        assert!(result.is_ok());

        let virt_addr = result.unwrap();
        assert!(virt_addr.is_page_aligned());
    }

    #[test]
    fn test_map_phys_permission_denied() {
        let mut vm = VmHandler::new_test();
        let process = vm.create_user_process();

        let request = MapPhysRequest {
            phys_addr: PhysicalAddress::new(0xA0000),
            length: 4096,
        };

        let result = vm.do_map_phys(&request);
        assert!(matches!(result, Err(MapPhysError::PermissionDenied)));
    }

    #[test]
    fn test_map_phys_device_register() {
        let mut vm = VmHandler::new_test();
        let process = vm.create_privileged_process(Endpoint::PCI_DRIVER);

        let request = MapPhysRequest {
            phys_addr: PhysicalAddress::new(0xFEC00000),
            length: 4096,
        };

        let result = vm.do_map_phys(&request);
        assert!(result.is_ok());

        let virt_addr = result.unwrap();

        vm.write_memory(process, virt_addr, &[0x01, 0x02, 0x03, 0x04]);

        let phys_data = vm.read_physical_memory(0xFEC00000, 4);
        assert_eq!(phys_data, &[0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn test_map_phys_unmap() {
        let mut vm = VmHandler::new_test();
        let process = vm.create_privileged_process(Endpoint::TTY);

        let request = MapPhysRequest {
            phys_addr: PhysicalAddress::new(0xB8000),
            length: 4096,
        };

        let virt_addr = vm.do_map_phys(&request).unwrap();

        let unmap_request = MunmapRequest {
            addr: virt_addr,
            length: 4096,
        };

        let result = vm.do_munmap(&unmap_request);
        assert!(result.is_ok());
    }
}
```

### 7.4 解除映射测试

测试解除映射功能。

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_munmap_complete() {
        let mut vm = VmHandler::new_test();
        let process = vm.create_test_process();

        let mmap_request = MmapRequest {
            addr: None,
            length: 4096 * 3,
            prot: ProtectionFlags::PROT_READ | ProtectionFlags::PROT_WRITE,
            flags: MmapFlags::MAP_ANONYMOUS | MmapFlags::MAP_PRIVATE,
            fd: None,
            offset: 0,
        };

        let addr = vm.do_mmap(&mmap_request).unwrap();

        let munmap_request = MunmapRequest {
            addr,
            length: 4096 * 3,
        };

        let result = vm.do_munmap(&munmap_request);
        assert!(result.is_ok());

        let lookup_result = vm.lookup_region(process, addr);
        assert!(lookup_result.is_none());
    }

    #[test]
    fn test_munmap_partial_start() {
        let mut vm = VmHandler::new_test();
        let process = vm.create_test_process();

        let addr = vm.do_mmap(&MmapRequest {
            addr: None,
            length: 4096 * 4,
            prot: ProtectionFlags::PROT_READ | ProtectionFlags::PROT_WRITE,
            flags: MmapFlags::MAP_ANONYMOUS | MmapFlags::MAP_PRIVATE,
            fd: None,
            offset: 0,
        }).unwrap();

        let result = vm.do_munmap(&MunmapRequest {
            addr,
            length: 4096,
        });
        assert!(result.is_ok());

        assert!(vm.lookup_region(process, addr).is_none());
        assert!(vm.lookup_region(process, addr + 4096).is_some());
    }

    #[test]
    fn test_munmap_partial_middle() {
        let mut vm = VmHandler::new_test();
        let process = vm.create_test_process();

        let addr = vm.do_mmap(&MmapRequest {
            addr: None,
            length: 4096 * 4,
            prot: ProtectionFlags::PROT_READ | ProtectionFlags::PROT_WRITE,
            flags: MmapFlags::MAP_ANONYMOUS | MmapFlags::MAP_PRIVATE,
            fd: None,
            offset: 0,
        }).unwrap();

        let result = vm.do_munmap(&MunmapRequest {
            addr: addr + 4096,
            length: 4096 * 2,
        });
        assert!(result.is_ok());

        assert!(vm.lookup_region(process, addr).is_some());
        assert!(vm.lookup_region(process, addr + 4096).is_none());
        assert!(vm.lookup_region(process, addr + 4096 * 3).is_some());
    }

    #[test]
    fn test_munmap_not_mapped() {
        let mut vm = VmHandler::new_test();
        let process = vm.create_test_process();

        let result = vm.do_munmap(&MunmapRequest {
            addr: VirtualAddress::new(0x5000_0000),
            length: 4096,
        });

        assert!(result.is_ok());
    }
}
```

---

## 8. 参见

- [12-vir-region.md](12-vir-region.md) - 区域创建与管理
- [11-memtype.md](11-memtype.md) - 不同映射类型的内存类型
- [13-region-avl.md](13-region-avl.md) - 区域查找与插入

---

*分类: VM服务 | IPC接口: VM_MAP, VM_UNMAP, VM_MAP_PHYS | 调用者: VFS, 进程自身, 驱动*
