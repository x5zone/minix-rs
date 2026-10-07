# do_safecopy.c 详细讲解

**文件位置**: `minix3/minix/kernel/system/do_safecopy.c`

**总行数**: 448 行

**作用**: 实现 `SYS_SAFECOPYFROM`、`SYS_SAFECOPYTO`、`SYS_VSAFECOPY` 系统调用，是 Grant 机制的核心使用者

---

## 一、文件概述

### 1.1 是什么（What）

`do_safecopy.c` 实现了 MINIX3 的**安全拷贝机制**，是 Grant 授权表的核心使用者：

| 系统调用 | 功能 |
|---------|------|
| `SYS_SAFECOPYFROM` | 从授权方拷贝数据到调用者 |
| `SYS_SAFECOPYTO` | 从调用者拷贝数据到授权方 |
| `SYS_VSAFECOPY` | 向量安全拷贝（批量操作） |

### 1.2 为什么需要（Why）

**问题**：Grant 机制设置了授权表，但谁来验证和使用这些授权？

**答案**：`do_safecopy.c` 负责验证授权并执行实际的内存拷贝。

```
┌─────────────────────────────────────────────────────────────────────────┐
│  Grant 机制的完整流程                                                    │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  1. 设置授权表（do_setgrant.c）                                         │
│     └──► 进程 A 设置自己的授权表                                        │
│                                                                         │
│  2. 创建授权（用户态 libsys）                                           │
│     └──► VFS 调用 cpf_grant_magic() 创建授权                            │
│                                                                         │
│  3. 验证授权 + 执行拷贝（do_safecopy.c）  ← 本文件                       │
│     └──► 内核验证授权有效性                                             │
│     └──► 执行实际的内存拷贝                                             │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 1.3 使用场景（When）

| 场景 | 系统调用 | 说明 |
|------|---------|------|
| 文件读取 | `SYS_SAFECOPYFROM` | 文件系统进程读取用户缓冲区 |
| 文件写入 | `SYS_SAFECOPYTO` | 文件系统进程写入用户缓冲区 |
| 批量 I/O | `SYS_VSAFECOPY` | 多个读写操作合并为一个系统调用 |

---

## 二、逐行讲解

### 2.1 文件头注释（第 1-16 行）

```c
/* The kernel call implemented in this file:
 *   m_type:	SYS_SAFECOPYFROM or SYS_SAFECOPYTO or SYS_VSAFECOPY
 *
 * The parameters for this kernel call are:
 *    	m_lsys_kern_safecopy.from_to	other endpoint
 *    	m_lsys_kern_safecopy.gid	grant id
 *    	m_lsys_kern_safecopy.offset	offset within granted space
 *	m_lsys_kern_safecopy.address	address in own address space
 *    	m_lsys_kern_safecopy.bytes	bytes to be copied
 *
 * For the vectored variant (do_vsafecopy):
 *      m_lsys_kern_vsafecopy.vec_addr   address of vector
 *      m_lsys_kern_vsafecopy.vec_size   number of significant elements in vector
 */
```

**逐字段解析**：

| 字段 | 类型 | 含义 |
|------|------|------|
| `from_to` | `endpoint_t` | 授权方进程端点 |
| `gid` | `cp_grant_id_t` | 授权 ID |
| `offset` | `size_t` | 授权空间内的偏移量 |
| `address` | `vir_bytes` | 调用者地址空间中的地址 |
| `bytes` | `size_t` | 拷贝字节数 |

### 2.2 头文件包含（第 18-22 行）

```c
#include <assert.h>

#include "kernel/system.h"
#include "kernel/vm.h"
```

| 头文件 | 作用 |
|--------|------|
| `<assert.h>` | 断言宏，用于调试检查 |
| `"kernel/system.h"` | 系统调用框架，`struct proc` 定义 |
| `"kernel/vm.h"` | 虚拟内存接口，`virtual_copy()` 函数 |

### 2.3 宏定义（第 24-26 行）

```c
#define MAX_INDIRECT_DEPTH 5	/* up to how many indirect grants to follow? */

#define MEM_TOP 0xFFFFFFFFUL
```

**逐个解析**：

| 宏 | 值 | 含义 | 设计原因 |
|----|-----|------|----------|
| `MAX_INDIRECT_DEPTH` | 5 | 间接授权的最大追踪深度 | 防止循环授权导致无限循环 |
| `MEM_TOP` | 0xFFFFFFFF | 32 位地址空间顶部 | 用于检测地址回绕 |

**设计原因**：间接授权（Indirect Grant）允许 A 把从 B 收到的授权转给 C，形成授权链。限制深度是为了：
1. 防止恶意构造的循环授权
2. 限制内核栈使用
3. 快速失败，避免资源耗尽

### 2.4 函数前向声明（第 28-30 行）

```c
static int safecopy(struct proc *, endpoint_t, endpoint_t,
	cp_grant_id_t, size_t, vir_bytes, vir_bytes, int);
```

**参数解析**：

| 参数位置 | 类型 | 含义 |
|---------|------|------|
| 1 | `struct proc *` | 调用者进程 |
| 2 | `endpoint_t` | 授权方端点 |
| 3 | `endpoint_t` | 被授权方端点 |
| 4 | `cp_grant_id_t` | 授权 ID |
| 5 | `size_t` | 拷贝字节数 |
| 6 | `vir_bytes` | 授权空间偏移 |
| 7 | `vir_bytes` | 调用者地址 |
| 8 | `int` | 访问方向（CPF_READ/CPF_WRITE） |

### 2.5 辅助宏（第 32-33 行）

```c
#define HASGRANTTABLE(gr) \
	(priv(gr) && priv(gr)->s_grant_table)
```

**功能**：检查进程是否有授权表

**逻辑**：
1. `priv(gr)` - 获取进程的特权结构
2. `priv(gr)->s_grant_table` - 检查授权表地址是否非零

**返回**：非零表示有授权表，零表示没有

### 2.6 软故障信息结构体（第 35-41 行）

```c
struct cp_sfinfo {		/* information for handling soft faults */
	int try;		/* if nonzero, try copy only, stop on fault */
	endpoint_t endpt;	/* endpoint owning grant with CPF_TRY flag */
	vir_bytes addr;		/* address to write mark upon soft fault */
	cp_grant_id_t value;	/* grant ID to use as mark value to write */
};
```

**逐字段解析**：

| 字段 | 类型 | 大小 | 含义 | 内存位置 |
|------|------|------|------|----------|
| `try` | `int` | 4 字节 | 是否尝试拷贝（遇故障停止） | 栈 |
| `endpt` | `endpoint_t` | 4 字节 | 拥有授权的进程端点 | 栈 |
| `addr` | `vir_bytes` | 4/8 字节 | 写入标记的地址 | 栈 |
| `value` | `cp_grant_id_t` | 4 字节 | 要写入的标记值 | 栈 |

**设计原因**：`CPF_TRY` 标志允许"尝试拷贝"，如果遇到页面故障（如内存映射文件），不阻塞而是返回错误。这对于文件系统避免死锁很重要。

---

## 三、verify_grant 函数详解（核心函数）

### 3.1 函数签名（第 43-53 行）

```c
int verify_grant(
  endpoint_t granter,		/* copyee */
  endpoint_t grantee,		/* copyer */
  cp_grant_id_t grant,		/* grant id */
  vir_bytes bytes,		/* copy size */
  int access,			/* direction (read/write) */
  vir_bytes offset_in,		/* copy offset within grant */
  vir_bytes *offset_result,	/* copy offset within virtual address space */
  endpoint_t *e_granter,	/* new granter (magic grants) */
  struct cp_sfinfo *sfinfo	/* storage for soft fault information */
)
```

**参数详解**：

| 参数 | 方向 | 含义 |
|------|------|------|
| `granter` | 输入 | 授权方进程端点（提供内存的一方） |
| `grantee` | 输入 | 被授权方进程端点（访问内存的一方） |
| `grant` | 输入 | 授权 ID |
| `bytes` | 输入 | 要拷贝的字节数 |
| `access` | 输入 | 访问方向（CPF_READ 或 CPF_WRITE） |
| `offset_in` | 输入 | 授权空间内的偏移量 |
| `offset_result` | 输出 | 实际虚拟地址偏移 |
| `e_granter` | 输出 | 实际授权方（magic grant 会改变） |
| `sfinfo` | 输出 | 软故障信息存储 |

**返回值**：
- `OK` - 验证成功
- `EINVAL` - 无效参数
- `EPERM` - 权限不足
- `ENOTREADY` - 授权表未就绪（live update 场景）
- `ELOOP` - 间接授权链太长

### 3.2 局部变量（第 54-59 行）

```c
	cp_grant_t g;
	int proc_nr;
	const struct proc *granter_proc;
	int grant_idx, grant_seq;
	int depth = 0;
```

| 变量 | 类型 | 大小 | 用途 |
|------|------|------|------|
| `g` | `cp_grant_t` | ~24 字节 | 存储从授权表读取的授权条目 |
| `proc_nr` | `int` | 4 字节 | 进程槽号 |
| `granter_proc` | `const struct proc *` | 4/8 字节 | 授权方进程指针 |
| `grant_idx` | `int` | 4 字节 | 授权索引 |
| `grant_seq` | `int` | 4 字节 | 授权序列号 |
| `depth` | `int` | 4 字节 | 间接授权追踪深度 |

### 3.3 主循环开始（第 61-73 行）

```c
	do {
		/* Get granter process slot (if valid), and check range of
		 * grant id.
		 */
		if(!isokendpt(granter, &proc_nr) ) {
			printf(
			"grant verify failed: invalid granter %d\n", (int) granter);
			return(EINVAL);
		}
		if(!GRANT_VALID(grant)) {
			printf(
			"grant verify failed: invalid grant %d\n", (int) grant);
			return(EINVAL);
		}
		granter_proc = proc_addr(proc_nr);
```

**逐行解析**：

| 行号 | 代码 | 功能 |
|------|------|------|
| 66-70 | `isokendpt()` | 验证端点有效性，获取进程槽号 |
| 71-74 | `GRANT_VALID()` | 验证授权 ID 有效性（非负） |
| 75 | `proc_addr()` | 根据槽号获取进程指针 |

**设计原因**：所有验证在最开始进行，快速失败原则。

### 3.4 临时授权表检查（第 76-93 行）

```c
		/* If the granter has a temporary grant table, always allow
		 * requests with unspecified access and return ENOTREADY if
		 * no grant table is present or if the grantee's endpoint is not
		 * the endpoint the table belongs to. When ENOTREADY is returned
		 * the same verify_grant() request will be replayed again in a
		 * while until the grant table is final. This is necessary to
		 * avoid races at live update time.
		 */
		if(priv(granter_proc)->s_grant_endpoint != granter_proc->p_endpoint) {
			if(!access) {
				return OK;
			}
			else if(!HASGRANTTABLE(granter_proc) || grantee != priv(granter_proc)->s_grant_endpoint) {
				return ENOTREADY;
			}
		}
```

**这是我们之前讨论的关键代码！**

**逻辑流程**：

```
检查 s_grant_endpoint != 当前进程端点
    │
    ├── 不相等（临时授权表）
    │   │
    │   ├── access == 0（仅查询）
    │   │   └── 返回 OK
    │   │
    │   └── access != 0（实际访问）
    │       │
    │       ├── 没有授权表
    │       │   └── 返回 ENOTREADY
    │       │
    │       └── grantee != s_grant_endpoint
    │           └── 返回 ENOTREADY
    │
    └── 相等（正常授权表）
        └── 继续验证
```

**设计原因**：Live Update 场景下，进程可能正在更新授权表。`ENOTREADY` 告诉调用者"稍后重试"，避免竞争条件。

### 3.5 授权表存在性检查（第 95-107 行）

```c
		/* If there is no priv. structure, or no grant table in the
		 * priv. structure, or the grant table in the priv. structure
		 * is too small for the grant, return EPERM.
		 */
		if(!HASGRANTTABLE(granter_proc)) {
			printf(
			"grant verify failed: granter %d has no grant table\n",
			granter);
			return(EPERM);
		}

		grant_idx = GRANT_IDX(grant);
		grant_seq = GRANT_SEQ(grant);

		if(priv(granter_proc)->s_grant_entries <= grant_idx) {
				printf(
				"verify_grant: grant verify failed in ep %d "
				"proc %d: grant 0x%x (#%d) out of range "
				"for table size %d\n",
					granter, proc_nr, grant, grant_idx,
					priv(granter_proc)->s_grant_entries);
			return(EPERM);
		}
```

**逐行解析**：

| 步骤 | 代码 | 功能 |
|------|------|------|
| 1 | `HASGRANTTABLE()` | 检查授权表是否存在 |
| 2 | `GRANT_IDX()` | 从授权 ID 提取索引 |
| 3 | `GRANT_SEQ()` | 从授权 ID 提取序列号 |
| 4 | 索引范围检查 | 确保索引不超出表大小 |

**授权 ID 结构**：

```
授权 ID (32 位):
┌─────────────────────┬─────────────────────┐
│   序列号 (16 位)     │   索引 (16 位)       │
└─────────────────────┴─────────────────────┘
```

### 3.6 读取授权条目（第 109-123 行）

```c
		/* Copy the grant entry corresponding to this ID's index to see
		 * what it looks like. If it fails, hide the fact that granter
		 * has (presumably) set an invalid grant table entry by
		 * returning EPERM, just like with an invalid grant id.
		 */
		if(data_copy(granter, priv(granter_proc)->s_grant_table +
			sizeof(g) * grant_idx,
			KERNEL, (vir_bytes) &g, sizeof(g)) != OK) {
			printf(
			"verify_grant: grant verify: data_copy failed\n");
			return EPERM;
		}
```

**关键点**：授权表存储在**用户空间**，内核需要用 `data_copy()` 从用户空间读取。

**内存布局**：

```
用户空间（授权方进程）:
┌─────────────────────────────────────────────────────────────────┐
│  s_grant_table ──► [grant_0] [grant_1] [grant_2] ... [grant_N]  │
│                     ↑                                           │
│                     │                                           │
│                     grant_idx * sizeof(cp_grant_t)              │
└─────────────────────────────────────────────────────────────────┘

内核空间:
┌─────────────────────────────────────────────────────────────────┐
│  &g ──► [本地 cp_grant_t 结构体]                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 3.7 授权有效性检查（第 125-138 行）

```c
		/* Check validity: flags and sequence number. */
		if((g.cp_flags & (CPF_USED | CPF_VALID)) !=
			(CPF_USED | CPF_VALID)) {
			printf("verify_grant: grant failed: invalid flags "
			    "(0x%x, 0x%lx)\n", grant, g.cp_flags);
			return EPERM;
		}

		if (g.cp_seq != grant_seq) {
			printf("verify_grant: grant failed: invalid sequence "
			    "(0x%x, %d vs %d)\n", grant, grant_seq, g.cp_seq);
			return EPERM;
		}
```

**双重验证**：

| 检查 | 目的 |
|------|------|
| `CPF_USED` | 授权条目已分配使用 |
| `CPF_VALID` | 授权条目有效（未被撤销） |
| 序列号匹配 | 防止授权 ID 重用攻击 |

**设计原因**：序列号机制防止"授权 ID 重用攻击"：
1. 授权条目被释放后可重用
2. 但序列号递增
3. 旧的授权 ID（带旧序列号）无法通过验证

### 3.8 间接授权处理（第 140-172 行）

```c
		/* The given grant may be an indirect grant, that is, a grant
		 * that provides permission to use a grant given to the
		 * granter (i.e., for which it is the grantee). This can lead
		 * to a chain of indirect grants which must be followed back.
		 */
		if((g.cp_flags & CPF_INDIRECT)) {
			/* Stop after a few iterations. There may be a loop. */
			if (depth == MAX_INDIRECT_DEPTH) {
				printf(
					"verify grant: indirect grant verify "
					"failed: exceeded maximum depth\n");
				return ELOOP;
			}
			depth++;

			/* Verify actual grantee. */
			if(g.cp_u.cp_indirect.cp_who_to != grantee &&
				grantee != ANY &&
				g.cp_u.cp_indirect.cp_who_to != ANY) {
				printf(
					"verify_grant: indirect grant verify "
					"failed: bad grantee\n");
				return EPERM;
			}

			/* Start over with new granter, grant, and grantee. */
			grantee = granter;
			granter = g.cp_u.cp_indirect.cp_who_from;
			grant = g.cp_u.cp_indirect.cp_grant;
		}
	} while(g.cp_flags & CPF_INDIRECT);
```

**间接授权示意图**：

```
间接授权链：

进程 A ──授权1──► 进程 B ──授权2──► 进程 C
                   │
                   └── 授权2 是间接授权，指向授权1

验证过程：
1. C 请求使用授权2
2. 内核发现授权2 是间接授权
3. 内核追踪到授权1（A 授权给 B）
4. 最终验证：A 授权 B 的内存给 C
```

**设计原因**：间接授权允许"转授权"，但需要限制深度防止循环。

### 3.9 访问权限检查（第 174-181 行）

```c
	/* Check access of grant. */
	if(((g.cp_flags & access) != access)) {
		printf(
	"verify_grant: grant verify failed: access invalid; want 0x%x, have 0x%x\n",
			access, g.cp_flags);
		return EPERM;
	}
```

**访问权限位**：

| 标志 | 值 | 含义 |
|------|-----|------|
| `CPF_READ` | 0x01 | 允许读取 |
| `CPF_WRITE` | 0x02 | 允许写入 |

**检查逻辑**：`(g.cp_flags & access) == access` 确保请求的所有权限都被授权。

### 3.10 直接授权处理（第 183-212 行）

```c
	if((g.cp_flags & CPF_DIRECT)) {
		/* Don't fiddle around with grants that wrap, arithmetic
		 * below may be confused.
		 */
		if(MEM_TOP - g.cp_u.cp_direct.cp_len + 1 <
			g.cp_u.cp_direct.cp_start) {
			printf(
		"verify_grant: direct grant verify failed: len too long\n");
			return EPERM;
		}

		/* Verify actual grantee. */
		if(g.cp_u.cp_direct.cp_who_to != grantee && grantee != ANY
			&& g.cp_u.cp_direct.cp_who_to != ANY) {
			printf(
		"verify_grant: direct grant verify failed: bad grantee\n");
			return EPERM;
		}

		/* Verify actual copy range. */
		if((offset_in+bytes < offset_in) ||
		    offset_in+bytes > g.cp_u.cp_direct.cp_len) {
			printf(
		"verify_grant: direct grant verify failed: bad size or range. "
		"granted %d bytes @ 0x%lx; wanted %d bytes @ 0x%lx\n",
				g.cp_u.cp_direct.cp_len,
				g.cp_u.cp_direct.cp_start,
				bytes, offset_in);
			return EPERM;
		}

		/* Verify successful - tell caller what address it is. */
		*offset_result = g.cp_u.cp_direct.cp_start + offset_in;
		*e_granter = granter;
	}
```

**直接授权结构**：

```c
// cp_grant_t 中的直接授权部分
struct {
    endpoint_t cp_who_to;    // 被授权方
    vir_bytes cp_start;      // 授权内存起始地址
    vir_bytes cp_len;        // 授权内存长度
} cp_direct;
```

**验证步骤**：

| 步骤 | 检查 | 目的 |
|------|------|------|
| 1 | 地址回绕 | 防止 `start + len` 溢出 |
| 2 | 被授权方匹配 | 确保授权给正确的进程 |
| 3 | 范围检查 | 确保 `offset + bytes` 不超出授权范围 |

### 3.11 魔法授权处理（第 213-244 行）

```c
	else if(g.cp_flags & CPF_MAGIC) {
		/* Currently, it is hardcoded that only VFS and MIB may do
		 * magic grants.  TODO: this should be a system.conf flag.
		 */
		if(granter != VFS_PROC_NR && granter != MIB_PROC_NR) {
			printf(
		"verify_grant: magic grant verify failed: granter (%d) "
		"not allowed\n", granter);
			return EPERM;
		}

		/* Verify actual grantee. */
		if(g.cp_u.cp_magic.cp_who_to != grantee && grantee != ANY
			&& g.cp_u.cp_direct.cp_who_to != ANY) {
			printf(
		"verify_grant: magic grant verify failed: bad grantee\n");
			return EPERM;
		}

		/* Verify actual copy range. */
		if((offset_in+bytes < offset_in) ||
		    offset_in+bytes > g.cp_u.cp_magic.cp_len) {
			printf(
		"verify_grant: magic grant verify failed: bad size or range. "
		"granted %d bytes @ 0x%lx; wanted %d bytes @ 0x%lx\n",
				g.cp_u.cp_magic.cp_len,
				g.cp_u.cp_magic.cp_start,
				bytes, offset_in);
			return EPERM;
		}

		/* Verify successful - tell caller what address it is. */
		*offset_result = g.cp_u.cp_magic.cp_start + offset_in;
		*e_granter = g.cp_u.cp_magic.cp_who_from;
	}
```

**魔法授权结构**：

```c
// cp_grant_t 中的魔法授权部分
struct {
    endpoint_t cp_who_to;     // 被授权方（文件系统进程）
    endpoint_t cp_who_from;   // 实际内存拥有者（用户进程）
    vir_bytes cp_start;       // 用户进程中的地址
    vir_bytes cp_len;         // 长度
} cp_magic;
```

**关键点**：魔法授权改变了 `e_granter`！

```
直接授权：granter = 实际内存拥有者
魔法授权：granter = VFS，但 e_granter = 用户进程
```

**设计原因**：只有 VFS 和 MIB 被允许创建魔法授权，这是安全限制。

### 3.12 软故障信息存储（第 246-255 行）

```c
	/* If requested, store information regarding soft faults. */
	if (sfinfo != NULL && (sfinfo->try = !!(g.cp_flags & CPF_TRY))) {
		sfinfo->endpt = granter;
		sfinfo->addr = priv(granter_proc)->s_grant_table +
		    sizeof(g) * grant_idx + offsetof(cp_grant_t, cp_faulted);
		sfinfo->value = grant;
	}
```

**CPF_TRY 标志**：
- 如果设置，拷贝时遇到页面故障不阻塞
- 而是返回 EFAULT，并标记授权条目

**设计原因**：文件系统处理内存映射文件时，页面可能不在内存中。如果阻塞等待页面加载，可能导致死锁。`CPF_TRY` 允许快速失败。

---

## 四、safecopy 函数详解

### 4.1 函数签名（第 260-273 行）

```c
static int safecopy(
  struct proc * caller,
  endpoint_t granter,
  endpoint_t grantee,
  cp_grant_id_t grantid,
  size_t bytes,
  vir_bytes g_offset,
  vir_bytes addr,
  int access			/* CPF_READ for a copy from granter to grantee, CPF_WRITE
				 * for a copy from grantee to granter.
				 */
)
```

**参数说明**：

| 参数 | 含义 |
|------|------|
| `caller` | 调用者进程（用于 VM 检查） |
| `granter` | 授权方端点 |
| `grantee` | 被授权方端点 |
| `grantid` | 授权 ID |
| `bytes` | 拷贝字节数 |
| `g_offset` | 授权空间偏移 |
| `addr` | 调用者地址 |
| `access` | CPF_READ（从授权方读）或 CPF_WRITE（写到授权方） |

### 4.2 局部变量（第 274-278 行）

```c
	static struct vir_addr v_src, v_dst;
	static vir_bytes v_offset;
	endpoint_t new_granter, *src, *dst;
	int r;
	struct cp_sfinfo sfinfo;
```

**注意**：`v_src`、`v_dst`、`v_offset` 是 `static`，存储在数据段而非栈上。

**设计原因**：减少栈使用，但需要注意并发安全（内核是单线程的，所以安全）。

### 4.3 端点有效性检查（第 280-283 行）

```c
	if(granter == NONE || grantee == NONE) {
		printf("safecopy: nonsense processes\n");
		return EFAULT;
	}
```

### 4.4 确定源和目标（第 285-293 行）

```c
	/* Decide who is src and who is dst. */
	if(access & CPF_READ) {
		src = &granter;
		dst = &grantee;
	} else {
		src = &grantee;
		dst = &granter;
	}
```

**逻辑**：

| access | 源 | 目标 | 数据流向 |
|--------|-----|------|----------|
| CPF_READ | 授权方 | 被授权方 | 授权方 → 被授权方 |
| CPF_WRITE | 被授权方 | 授权方 | 被授权方 → 授权方 |

### 4.5 调用 verify_grant（第 295-303 行）

```c
	/* Verify permission exists. */
	if((r=verify_grant(granter, grantee, grantid, bytes, access,
	    g_offset, &v_offset, &new_granter, &sfinfo)) != OK) {
		if(r == ENOTREADY) return r;
			printf(
		"grant %d verify to copy %d->%d by %d failed: err %d\n",
				grantid, *src, *dst, grantee, r);
		return r;
	}
```

**关键点**：`verify_grant` 返回 `v_offset`（实际地址偏移）和 `new_granter`（魔法授权会改变）。

### 4.6 更新授权方（第 305-307 行）

```c
	/* verify_grant() can redirect the grantee to someone else,
	 * meaning the source or destination changes.
	 */
	granter = new_granter;
```

**魔法授权场景**：
- `granter` 原本是 VFS
- `new_granter` 是用户进程
- 实际拷贝发生在用户进程和文件系统进程之间

### 4.7 设置虚拟地址结构（第 309-322 行）

```c
	/* Now it's a regular copy. */
	v_src.proc_nr_e = *src;
	v_dst.proc_nr_e = *dst;

	/* Now the offset in virtual addressing is known in 'offset'.
	 * Depending on the access, this is the source or destination
	 * address.
	 */
	if(access & CPF_READ) {
		v_src.offset = v_offset;
		v_dst.offset = (vir_bytes) addr;
	} else {
		v_src.offset = (vir_bytes) addr;
		v_dst.offset = v_offset;
	}
```

**vir_addr 结构**：

```c
struct vir_addr {
    endpoint_t proc_nr_e;  // 进程端点
    vir_bytes offset;      // 虚拟地址偏移
};
```

### 4.8 执行拷贝（第 324-354 行）

```c
	/* Do the regular copy. */
	if (sfinfo.try) {
		/*
		 * Try copying without transparently faulting in pages.
		 * TODO: while CPF_TRY is meant to protect against deadlocks on
		 * memory-mapped files in file systems, it seems that this case
		 * triggers faults a whole lot more often, resulting in extra
		 * overhead due to retried file system operations.  It might be
		 * a good idea to go through VM even in this case, and have VM
		 * fail (only) if the affected page belongs to a file mapping.
		 */
		r = virtual_copy(&v_src, &v_dst, bytes);
		if (r == EFAULT_SRC || r == EFAULT_DST) {
			/*
			 * Mark the magic grant as having experienced a soft
			 * fault during its lifetime.  The exact value does not
			 * matter, but we use the grant ID (including its
			 * sequence number) as a form of protection in the
			 * light of CPU concurrency.
			 */
			r = data_copy(KERNEL, (vir_bytes)&sfinfo.value,
			    sfinfo.endpt, sfinfo.addr, sizeof(sfinfo.value));
			/*
			 * Failure means the creator of the magic grant messed
			 * up, which can only be unintentional, so report..
			 */
			if (r != OK)
				printf("Kernel: writing soft fault marker %d "
				    "into %d at 0x%lx failed (%d)\n",
				    sfinfo.value, sfinfo.endpt, sfinfo.addr,
				    r);

			return EFAULT;
		}
		return r;
	}
	return virtual_copy_vmcheck(caller, &v_src, &v_dst, bytes);
}
```

**两种拷贝模式**：

| 模式 | 函数 | 特点 |
|------|------|------|
| CPF_TRY | `virtual_copy()` | 遇故障返回 EFAULT，不阻塞 |
| 普通 | `virtual_copy_vmcheck()` | 通过 VM 处理页面故障 |

---

## 五、系统调用入口函数

### 5.1 do_safecopy_to（第 359-365 行）

```c
int do_safecopy_to(struct proc * caller, message * m_ptr)
{
	return safecopy(caller, m_ptr->m_lsys_kern_safecopy.from_to, caller->p_endpoint,
		(cp_grant_id_t) m_ptr->m_lsys_kern_safecopy.gid,
		m_ptr->m_lsys_kern_safecopy.bytes, m_ptr->m_lsys_kern_safecopy.offset,
		(vir_bytes) m_ptr->m_lsys_kern_safecopy.address, CPF_WRITE);
}
```

**功能**：从调用者拷贝数据到授权方

**场景**：文件写入 - 用户缓冲区 → 文件系统进程

### 5.2 do_safecopy_from（第 371-377 行）

```c
int do_safecopy_from(struct proc * caller, message * m_ptr)
{
	return safecopy(caller, m_ptr->m_lsys_kern_safecopy.from_to, caller->p_endpoint,
		(cp_grant_id_t) m_ptr->m_lsys_kern_safecopy.gid,
		m_ptr->m_lsys_kern_safecopy.bytes, m_ptr->m_lsys_kern_safecopy.offset,
		(vir_bytes) m_ptr->m_lsys_kern_safecopy.address, CPF_READ);
}
```

**功能**：从授权方拷贝数据到调用者

**场景**：文件读取 - 文件系统进程 → 用户缓冲区

### 5.3 do_vsafecopy（第 383-448 行）

```c
int do_vsafecopy(struct proc * caller, message * m_ptr)
{
	static struct vscp_vec vec[SCPVEC_NR];
	static struct vir_addr src, dst;
	int r, i, els;
	size_t bytes;

	/* Set vector copy parameters. */
	src.proc_nr_e = caller->p_endpoint;
	assert(src.proc_nr_e != NONE);
	src.offset = (vir_bytes) m_ptr->m_lsys_kern_vsafecopy.vec_addr;
	dst.proc_nr_e = KERNEL;
	dst.offset = (vir_bytes) vec;

	/* No. of vector elements. */
	els = m_ptr->m_lsys_kern_vsafecopy.vec_size;
	bytes = els * sizeof(struct vscp_vec);

	/* Obtain vector of copies. */
	if((r=virtual_copy_vmcheck(caller, &src, &dst, bytes)) != OK)
		return r;

	/* Perform safecopies. */
	for(i = 0; i < els; i++) {
		int access;
		endpoint_t granter;
		if(vec[i].v_from == SELF) {
			access = CPF_WRITE;
			granter = vec[i].v_to;
		} else if(vec[i].v_to == SELF) {
			access = CPF_READ;
			granter = vec[i].v_from;
		} else {
			printf("vsafecopy: %d: element %d/%d: no SELF found\n",
				caller->p_endpoint, i, els);
			return EINVAL;
		}

		/* Do safecopy for this element. */
		if((r=safecopy(caller, granter, caller->p_endpoint,
			vec[i].v_gid,
			vec[i].v_bytes, vec[i].v_offset,
			vec[i].v_addr, access)) != OK) {
			return r;
		}
	}

	return OK;
}
```

**向量拷贝结构**：

```c
struct vscp_vec {
    endpoint_t v_from;     // 源进程（SELF 表示调用者）
    endpoint_t v_to;       // 目标进程
    cp_grant_id_t v_gid;   // 授权 ID
    vir_bytes v_offset;    // 授权偏移
    vir_bytes v_addr;      // 本地地址
    size_t v_bytes;        // 字节数
};
```

**设计原因**：批量操作减少系统调用次数，提高效率。

---

## 六、现代硬件适配建议

| 方面 | C 语言实现 | 现代硬件适配 |
|------|-----------|-------------|
| 授权表查找 | 线性内存读取 | 缓存授权条目 |
| 地址翻译 | 软件计算 | 利用 EPT/NPT |
| 拷贝操作 | `virtual_copy()` | 使用 `copy_from_user()` 类似接口 |
| 间接授权追踪 | 循环 | 尾递归优化 |

---

## 七、Rust 重构建议

```rust
use core::result::Result;

bitflags! {
    pub struct GrantFlags: u32 {
        const CPF_USED = 0x01;
        const CPF_VALID = 0x02;
        const CPF_READ = 0x04;
        const CPF_WRITE = 0x08;
        const CPF_DIRECT = 0x10;
        const CPF_INDIRECT = 0x20;
        const CPF_MAGIC = 0x40;
        const CPF_TRY = 0x80;
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SafeCopyError {
    InvalidEndpoint,
    InvalidGrant,
    PermissionDenied,
    NotReady,
    LoopDetected,
    Fault,
    RangeExceeded,
}

pub struct GrantEntry {
    pub flags: GrantFlags,
    pub seq: u16,
    pub data: GrantData,
}

pub enum GrantData {
    Direct {
        who_to: Endpoint,
        start: VirtAddr,
        len: usize,
    },
    Indirect {
        who_to: Endpoint,
        who_from: Endpoint,
        grant: GrantId,
    },
    Magic {
        who_to: Endpoint,
        who_from: Endpoint,
        start: VirtAddr,
        len: usize,
    },
}

pub fn verify_grant(
    granter: Endpoint,
    grantee: Endpoint,
    grant: GrantId,
    bytes: usize,
    access: GrantFlags,
    offset: usize,
) -> Result<(VirtAddr, Endpoint), SafeCopyError> {
    let proc = process::from_endpoint(granter)?;
    
    let grant_table = proc.privilege()
        .ok_or(SafeCopyError::PermissionDenied)?
        .grant_table();
    
    let entry = grant_table.get(grant.index())?;
    
    if !entry.flags.contains(GrantFlags::CPF_USED | GrantFlags::CPF_VALID) {
        return Err(SafeCopyError::InvalidGrant);
    }
    
    if entry.seq != grant.sequence() {
        return Err(SafeCopyError::InvalidGrant);
    }
    
    if !entry.flags.contains(access) {
        return Err(SafeCopyError::PermissionDenied);
    }
    
    match entry.data {
        GrantData::Direct { who_to, start, len } => {
            if who_to != grantee && grantee != ANY {
                return Err(SafeCopyError::PermissionDenied);
            }
            if offset + bytes > len {
                return Err(SafeCopyError::RangeExceeded);
            }
            Ok((start + offset, granter))
        }
        GrantData::Magic { who_to, who_from, start, len } => {
            if who_to != grantee && grantee != ANY {
                return Err(SafeCopyError::PermissionDenied);
            }
            if offset + bytes > len {
                return Err(SafeCopyError::RangeExceeded);
            }
            Ok((start + offset, who_from))
        }
        GrantData::Indirect { .. } => {
            // 递归处理间接授权
            todo!("handle indirect grants")
        }
    }
}
```

---

## 八、要点总结

### 核心知识点

1. **verify_grant 是核心**：
   - 验证授权有效性
   - 处理三种授权类型
   - 返回实际地址和授权方

2. **三种授权类型**：
   - CPF_DIRECT：直接授权自己的内存
   - CPF_INDIRECT：转授权
   - CPF_MAGIC：授权别人的内存

3. **s_grant_endpoint 的作用**：
   - 区分授权表属于哪个进程
   - 处理用户进程共享 USER_PRIV_ID 的问题
   - 支持 live update 场景

---

## 九、灾难预演

### 场景 1：如果删掉序列号检查

```
后果：
1. 授权 ID 重用攻击
2. 恶意进程可以访问已释放的授权
3. 安全漏洞
```

### 场景 2：如果删掉间接授权深度限制

```
后果：
1. 循环授权导致无限循环
2. 内核栈溢出
3. 系统崩溃
```

### 场景 3：如果魔法授权不检查 granter

```
后果：
1. 任何进程可以创建魔法授权
2. 可以访问任意进程的内存
3. 完全绕过安全检查
```

---

## 十、互动自测

1. **问题**：verify_grant 为什么需要返回 `new_granter`？
   **答案**：魔法授权改变了实际授权方。VFS 创建魔法授权时，granter 是 VFS，但实际内存属于用户进程，所以需要返回用户进程作为 new_granter。

2. **问题**：为什么需要 CPF_TRY 标志？
   **答案**：文件系统处理内存映射文件时，页面可能不在内存中。如果阻塞等待页面加载，可能导致死锁。CPF_TRY 允许快速失败。

3. **问题**：间接授权的使用场景是什么？
   **答案**：进程 A 授权给进程 B，进程 B 可以转授权给进程 C，而不需要 A 的参与。

---

## 十一、参考文献

| 文件 | 描述 |
|------|------|
| `kernel/priv.h` | 特权结构体定义，s_grant_table 等字段 |
| `include/minix/safecopies.h` | 授权相关宏和类型定义 |
| `kernel/system/do_setgrant.c` | 设置授权表 |
| `lib/libsys/safecopies.c` | 用户态授权创建函数 |

---

*讲解日期：2026-03-30*
*讲解者：Minix-rs 学习助手*
