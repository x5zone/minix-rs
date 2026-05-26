# 13-syscall-memory: 内存相关系统调用（sys_vmctl 等）

> **分类**: Kernel 特权与系统调用
> **源码**: `minix3/minix/kernel/system.c`: sys_vmctl / sys_vm_map / vm_ctl_* + `arch/i386/arch_system.c` 相关
> **说明**: VM 完成页表决策后通过 sys_vmctl 通知内核执行——SET_PDBR、MAP_PHYS、GET_MAPPING 等

---

## 1. 概述

### 1.1 概念定义/作用

**内存相关系统调用**是内核为 VM（Virtual Memory）服务提供的特权接口。在 Minix3 的微内核架构中，VM 进程负责内存管理策略（分配/释放/换页），但实际修改页表、刷新 TLB、设置 CR3 等硬件操作必须由内核执行。`sys_vmctl` 就是 VM 通知内核执行这些硬件操作的接口。

此外，内核还提供地址映射（`sys_umap`）、虚拟复制（`sys_vircopy`）、安全复制（`sys_safecopy`）等接口，供系统服务安全地跨地址空间访问内存。

核心设计原则是**策略与机制分离**：VM 决定"映射什么"（策略），内核执行"如何映射"（机制）。VM 不直接操作硬件，而是通过内核调用请求内核代为执行。

### 1.2 与 Minix3 的对应关系

| 功能 | 函数 | 源文件 |
|------|------|--------|
| VM 控制操作 | `do_vmctl()` | system/do_vmctl.c |
| 虚拟→物理地址映射 | `do_umap()` | system/do_umap.c |
| 远程地址映射 | `do_umap_remote()` | system/do_umap_remote.c |
| 向量化地址映射 | `do_vumap()` | system/do_vumap.c |
| 虚拟地址间复制 | `do_vircopy()` | system/do_vircopy.c |
| 物理地址间复制 | `do_copy()` | system/do_copy.c |
| 安全复制（从） | `do_safecopy_from()` | system/do_safecopy.c |
| 安全复制（到） | `do_safecopy_to()` | system/do_safecopy.c |
| 向量化安全复制 | `do_vsafecopy()` | system/do_vsafecopy.c |
| 内存填充 | `do_memset()` | system/do_memset.c |
| 安全内存填充 | `do_safememset()` | system/do_safememset.c |
| 架构相关 VM 操作 | `arch_do_vmctl()` | arch/i386/arch_system.c |

### 1.3 关键状态/机制说明

**VM 请求链（vmrequest chain）**：当进程访问的内存不在物理内存中时，内核将进程挂起并加入 VM 请求链。VM 通过 `VMCTL_MEMREQ_GET` 获取请求信息，处理完页缺失后通过 `VMCTL_MEMREQ_REPLY` 通知内核恢复进程。

**VMINHIBIT 机制**：VM 在修改进程地址空间前，通过 `VMCTL_VMINHIBIT_SET` 通知内核将该进程标记为"VM 干预中"。内核在此期间跳过该进程的异步消息投递（SMP 下地址空间可能不一致），修改完成后通过 `VMCTL_VMINHIBIT_CLEAR` 解除。

**安全复制（safecopy）**：普通 `sys_vircopy` 需要调用方有权限访问源和目标地址空间。`sys_safecopy` 通过预授权的 grant 机制，允许调用方在特定范围内安全地访问其他进程的内存，无需完全的地址空间权限。

### 1.4 行为规则

1. **仅 VM 可调用**：`sys_vmctl` 的大部分子命令仅允许 VM 进程调用
2. **页缺失清除**：`VMCTL_CLEAR_PAGEFAULT` 清除 `RTS_PAGEFAULT` 标志，使进程恢复可运行
3. **VM 请求获取**：`VMCTL_MEMREQ_GET` 遍历请求链，跳过 IPC 过滤器拒绝的请求
4. **VM 请求回复**：`VMCTL_MEMREQ_REPLY` 根据 `p_vmrequest.type` 恢复不同类型的挂起操作
5. **VMINHIBIT 设置**：SMP 下若进程在其他 CPU 上运行，需先迁移到本地 CPU
6. **VMINHIBIT 清除**：SMP 下清除 `MF_SENDA_VM_MISS` 并重新尝试异步消息投递
7. **地址映射权限**：`sys_umap` 检查 `CHECK_MEM` 标志和 `s_mem_tab` 范围

## 2. C 源码分析

### 2.1 相关定义（常量、配置等）

#### 2.1.1 VMCTL 子命令

定义于 `minix3/minix/include/minix/com.h:394-409`：

| 常量 | 值 | 含义 |
|------|-----|------|
| `VMCTL_CLEAR_PAGEFAULT` | 12 | 清除进程的页缺失标志 |
| `VMCTL_GET_PDBR` | 13 | 获取进程页表根地址（CR3） |
| `VMCTL_MEMREQ_GET` | 14 | 获取下一个 VM 内存请求 |
| `VMCTL_MEMREQ_REPLY` | 15 | 回复 VM 内存请求 |
| `VMCTL_NOPAGEZERO` | 18 | 不将页面零初始化 |
| `VMCTL_I386_KERNELLIMIT` | 19 | 设置内核段限制（x86） |
| `VMCTL_I386_INVLPG` | 25 | 使指定 TLB 条目无效（x86） |
| `VMCTL_FLUSHTLB` | 26 | 刷新全部 TLB |
| `VMCTL_KERN_PHYSMAP` | 27 | 获取内核物理内存映射信息 |
| `VMCTL_KERN_MAP_REPLY` | 28 | 回复内核物理映射 |
| `VMCTL_SETADDRSPACE` | 29 | 设置进程地址空间 |
| `VMCTL_VMINHIBIT_SET` | 30 | 设置 VMINHIBIT 标志 |
| `VMCTL_VMINHIBIT_CLEAR` | 31 | 清除 VMINHIBIT 标志 |
| `VMCTL_CLEARMAPCACHE` | 32 | 清除映射缓存 |
| `VMCTL_BOOTINHIBIT_CLEAR` | 33 | 清除 BOOTINHIBIT 标志 |

#### 2.1.2 VMCTL 消息字段

| 字段宏 | 消息字段 | 含义 |
|--------|---------|------|
| `SVMCTL_WHO` | `m1_i1` | 目标进程 endpoint |
| `SVMCTL_PARAM` | `m1_i2` | VMCTL 子命令号 |
| `SVMCTL_VALUE` | `m1_i3` | 参数值 |
| `SVMCTL_MRG_TARGET` | `m2_i1` | MEMREQ_GET 回复：目标进程 |
| `SVMCTL_MRG_ADDR` | `m2_i2` | MEMREQ_GET 回复：内存地址 |
| `SVMCTL_MRG_LENGTH` | `m2_i3` | MEMREQ_GET 回复：内存长度 |
| `SVMCTL_MRG_FLAG` | `m2_s1` | MEMREQ_GET 回复：写标志 |
| `SVMCTL_MRG_REQUESTOR` | `m2_p1` | MEMREQ_GET 回复：请求者 |
| `SVMCTL_PTROOT` | `m1_i3` | 页表根物理地址 |
| `SVMCTL_PTROOT_V` | `m1_p1` | 页表根虚拟地址 |
| `SVMCTL_MAP_VIR_ADDR` | `m1_p1` | 映射虚拟地址 |
| `SVMCTL_MAP_FLAGS` | `m2_i1` | 映射标志（VMMF_*） |
| `SVMCTL_MAP_PHYS_ADDR` | `m2_l1` | 物理地址 |
| `SVMCTL_MAP_PHYS_LEN` | `m2_l2` | 物理长度 |

#### 2.1.3 VM 请求类型

| 类型 | 含义 |
|------|------|
| `VMSTYPE_KERNELCALL` | 内核调用因页缺失挂起 |
| `VMSTYPE_DELIVERMSG` | 消息投递因页缺失挂起 |
| `VMSTYPE_MAP` | 地址空间映射请求 |

### 2.2 核心数据结构

#### 2.2.1 p_vmrequest 子结构

定义于 `minix3/minix/kernel/proc.h`，VM 请求相关字段：

| 字段 | 类型 | 含义 |
|------|------|------|
| `nextrestart` | `struct proc *` | VM 重启链中下一个进程 |
| `nextrequestor` | `struct proc *` | VM 请求链中下一个请求者 |
| `type` | `int` | 挂起操作类型（VMSTYPE_KERNELCALL/DELIVERMSG/MAP） |
| `saved.reqmsg` | `message` | 挂起的请求消息副本 |
| `req_type` | `int` | VM 请求类型（VMPTYPE_CHECK 等） |
| `target` | `endpoint_t` | VM 请求目标进程 |
| `params.check.start` | `vir_bytes` | 内存范围起始地址 |
| `params.check.length` | `vir_bytes` | 内存范围长度 |
| `params.check.writeflag` | `u8_t` | 写访问标志 |
| `vmresult` | `int` | VM 处理结果 |

#### 2.2.2 vmrequest 全局链表

内核维护一个全局的 VM 请求链表 `vmrequest`，链接所有因页缺失挂起的进程。链表通过 `p_vmrequest.nextrequestor` 指针串联。VM 通过 `VMCTL_MEMREQ_GET` 遍历此链表获取请求。

### 2.3 关键函数分析

#### 2.3.1 do_vmctl()——VM 控制操作

`minix3/minix/kernel/system/do_vmctl.c:17-173`

```c
int do_vmctl(struct proc *caller, message *m_ptr)
```

**功能**：处理 VM 进程的各种控制请求。

**行为**（按子命令）：

**VMCTL_CLEAR_PAGEFAULT**：清除进程的 `RTS_PAGEFAULT` 标志，使进程恢复可运行。VM 处理完页缺失后调用。

**VMCTL_MEMREQ_GET**：遍历 VM 请求链，找到第一个通过 IPC 过滤器的请求，返回请求信息（目标进程、地址、长度、写标志、请求者），并从链表中移除。若链表为空或所有请求被过滤器拒绝，返回 `ENOENT`。

**VMCTL_MEMREQ_REPLY**：VM 处理完内存请求后回复。根据 `p_vmrequest.type` 恢复不同类型的挂起操作：
- `VMSTYPE_KERNELCALL`：设置 `MF_KCALL_RESUME`，下次调度时恢复内核调用
- `VMSTYPE_DELIVERMSG`：消息投递恢复
- `VMSTYPE_MAP`：地址空间映射恢复
- 清除 `RTS_VMREQUEST` 标志

**VMCTL_KERN_PHYSMAP**：获取内核物理内存映射信息，委托给 `arch_phys_map()`。

**VMCTL_KERN_MAP_REPLY**：回复内核物理映射，委托给 `arch_phys_map_reply()`。

**VMCTL_VMINHIBIT_SET**：设置 `RTS_VMINHIBIT` 标志。SMP 下若进程在其他 CPU，调用 `smp_schedule_vminhibit()` 迁移；设置 `MF_FLUSH_TLB` 标志。

**VMCTL_VMINHIBIT_CLEAR**：清除 `RTS_VMINHIBIT` 标志。SMP 下清除 `MF_SENDA_VM_MISS` 并重新尝试异步消息投递；标记所有 CPU 的 TLB 为过期。

**VMCTL_CLEARMAPCACHE**：清除内核的映射缓存，委托给 `mem_clear_mapcache()`。

**VMCTL_BOOTINHIBIT_CLEAR**：清除 `RTS_BOOTINHIBIT` 标志，使启动等待的进程恢复。

**默认**：委托给 `arch_do_vmctl()` 处理架构相关的子命令（如 `VMCTL_GET_PDBR`、`VMCTL_I386_KERNELLIMIT`、`VMCTL_I386_INVLPG` 等）。

#### 2.3.2 do_umap()——虚拟地址映射

`minix3/minix/kernel/system/do_umap.c`

```c
int do_umap(struct proc *caller, message *m_ptr)
```

**功能**：将调用方地址空间中的虚拟地址映射为物理地址。

**行为**：
1. 从消息中提取虚拟地址和长度
2. 检查 `CHECK_MEM` 标志和 `s_mem_tab` 范围权限
3. 通过页表遍历将虚拟地址转换为物理地址
4. 返回物理地址，失败返回 `EFAULT`

#### 2.3.3 do_vircopy()——虚拟地址间复制

`minix3/minix/kernel/system/do_vircopy.c`

```c
int do_vircopy(struct proc *caller, message *m_ptr)
```

**功能**：在两个进程的虚拟地址空间之间复制数据。

**行为**：
1. 从消息中提取源进程、源地址、目标进程、目标地址、长度
2. 验证源和目标进程的 endpoint
3. 通过 `umap_local()` 将虚拟地址映射为物理地址
4. 使用 `phys_copy()` 在物理地址间复制数据
5. 可能返回 `VMSUSPEND`（若目标页面不在物理内存中）

#### 2.3.4 do_safecopy_from/to()——安全复制

`minix3/minix/kernel/system/do_safecopy.c`

```c
int do_safecopy_from(struct proc *caller, message *m_ptr)
int do_safecopy_to(struct proc *caller, message *m_ptr)
```

**功能**：通过预授权的 grant 安全地跨地址空间复制数据。

**行为**：
1. 从消息中提取 grant ID、源/目标地址、偏移量、长度
2. 验证 grant 的有效性（grant 表中存在且范围匹配）
3. 通过 grant 定位物理内存
4. 执行数据复制

**与 do_vircopy 的区别**：`do_vircopy` 需要调用方有完整的地址空间访问权限；`do_safecopy` 通过 grant 机制，仅允许在预授权范围内访问，更安全。

#### 2.3.5 do_memset()——内存填充

`minix3/minix/kernel/system/do_memset.c`

```c
int do_memset(struct proc *caller, message *m_ptr)
```

**功能**：将指定值填充到进程地址空间的指定区域。

**行为**：映射虚拟地址为物理地址后，使用 `phys_memset()` 填充。

### 2.4 调用关系/调用点分析

#### 2.4.1 页缺失处理完整路径

```
进程访问未映射内存 → 页缺失异常
  └─ 内核异常处理程序
       ├─ 设置 RTS_PAGEFAULT
       ├─ 填充 p_vmrequest 字段
       ├─ 加入 vmrequest 链表
       ├─ 设置 RTS_VMREQUEST
       └─ 通知 VM 进程

VM 处理页缺失
  ├─ VMCTL_MEMREQ_GET → 获取请求信息
  ├─ VM 分配物理页面、建立映射
  ├─ VMCTL_CLEAR_PAGEFAULT → 清除页缺失标志
  └─ VMCTL_MEMREQ_REPLY → 恢复挂起操作
       ├─ VMSTYPE_KERNELCALL → MF_KCALL_RESUME
       ├─ VMSTYPE_DELIVERMSG → 恢复消息投递
       └─ VMSTYPE_MAP → 恢复地址空间映射
```

#### 2.4.2 VMINHIBIT 使用路径

```
VM 修改进程地址空间
  ├─ VMCTL_VMINHIBIT_SET → RTS_VMINHIBIT
  │    └─ SMP: smp_schedule_vminhibit() 迁移进程
  ├─ VM 修改页表映射
  └─ VMCTL_VMINHIBIT_CLEAR → 清除 RTS_VMINHIBIT
       ├─ SMP: 清除 MF_SENDA_VM_MISS
       ├─ SMP: try_deliver_senda() 重新投递异步消息
       └─ SMP: 标记所有 CPU TLB 过期
```

#### 2.4.3 内存复制调用路径

```
系统服务调用 sys_vircopy(src_e, src_addr, dst_e, dst_addr, len)
  └─ SYS_VIRCOPY → do_vircopy()
       ├─ umap_local(src, src_addr) → phys_src
       ├─ umap_local(dst, dst_addr) → phys_dst
       └─ phys_copy(phys_src, phys_dst, len)

系统服务调用 sys_safecopyfrom(grant, offset, addr, len)
  └─ SYS_SAFECOPYFROM → do_safecopy_from()
       ├─ 验证 grant
       ├─ 通过 grant 定位物理内存
       └─ 执行复制
```

### 2.5 设计要点/特殊处理

#### 2.5.1 策略与机制分离

Minix3 的内存管理严格遵循策略与机制分离：

- **VM（策略）**：决定映射哪些页面、何时换入换出、如何分配物理内存
- **内核（机制）**：执行页表修改、TLB 刷新、CR3 加载等硬件操作

`sys_vmctl` 是两者之间的桥梁——VM 通过它告诉内核"做什么"，内核负责"怎么做"。

#### 2.5.2 VM 请求链与 IPC 过滤器

`VMCTL_MEMREQ_GET` 遍历请求链时检查 IPC 过滤器，这是因为 VM 在服务更新期间可能限制接收的请求来源。若某个请求的源进程被 IPC 过滤器拒绝，VM 不会收到该请求，内核继续查找下一个。

#### 2.5.3 VMSUSPEND 与透明恢复

当 `do_vircopy` 等函数发现目标页面不在物理内存中时，返回 `VMSUSPEND`。内核保存完整的请求上下文（`p_vmrequest.saved.reqmsg`），等待 VM 处理完页缺失后自动恢复。这对调用方完全透明——调用方不知道内核调用被暂停过。

#### 2.5.4 SMP 下的 VMINHIBIT 处理

SMP 配置下，VM 修改进程地址空间时必须确保该进程不在其他 CPU 上运行。`VMCTL_VMINHIBIT_SET` 在 SMP 下通过 `smp_schedule_vminhibit()` 通知目标 CPU 将进程移出运行队列。修改完成后，`VMCTL_VMINHIBIT_CLEAR` 标记所有 CPU 的 TLB 为过期，确保下次访问时使用新的映射。

#### 2.5.5 安全复制的 grant 机制

`sys_safecopy` 使用 grant（预授权凭证）而非直接地址空间权限。进程 A 可以向进程 B 授予一个 grant，允许 B 在特定范围内读写 A 的内存。内核在执行安全复制时验证 grant 的有效性，确保 B 只能在授权范围内操作。这比 `sys_vircopy` 的粗粒度权限检查更安全。

#### 2.5.6 架构相关的 VMCTL 子命令

`do_vmctl()` 将无法识别的子命令委托给 `arch_do_vmctl()`。x86 架构下处理的子命令包括：
- `VMCTL_GET_PDBR`：获取进程的 CR3 值
- `VMCTL_I386_KERNELLIMIT`：设置内核代码段限制
- `VMCTL_I386_INVLPG`：使指定虚拟地址的 TLB 条目无效
- `VMCTL_FLUSHTLB`：刷新全部 TLB
- `VMCTL_SETADDRSPACE`：切换进程地址空间

这种分层设计使得通用逻辑与架构特定逻辑分离。
