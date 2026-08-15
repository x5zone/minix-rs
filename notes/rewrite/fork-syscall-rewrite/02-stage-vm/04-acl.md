# 04-acl: 访问控制——谁被允许调用 VM 的哪些服务

> **分类**: 阶段 2 — 访问控制与物理内存（ACL 锚点）
> **源码**: `minix3/minix/servers/vm/acl.c`（129 行，5 个非 static 函数）；`minix3/minix/servers/vm/vmproc.h:23`（`vm_acl` 字段）；`minix3/minix/include/minix/com.h:627/769-770`（调用号与掩码宽度）；`minix3/minix/include/minix/bitmap.h:12-20`（位图宏）；`minix3/minix/include/minix/sys_config.h:9`（`_NR_SYS_PROCS`）
> **Rust 模块**: `os/servers/vm/src/acl.rs`（`AclMask`/`AclState`）+ `os/servers/vm/src/vmproc/vmproc.rs:31`（`vm_acl` 字段）+ `os/servers/vm/src/vmproc/vmproc_handle.rs:236-248/328-333`（typestate 方法族）+ 消费方 `os/servers/vm/src/vm_server.rs:562-587/621-634`、`os/servers/vm/src/rs.rs:115-140`、`os/servers/vm/src/fork.rs:215`
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md`（`acl_init` 调用点）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/02-vmproc-struct.md`（PCB 与生命周期）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/03-vmproc-table.md`（身份验证）
> **说明**: ACL 的语义模块：**初始化 / 检查 / 设置 / fork 派生 / 清除** + 位图语义 + DEFAULT/SYSTEM 分层。**不覆盖**：dispatch 中 `acl_check` 的接线（15-ipc-dispatch）、fork 全流程（18）、退出流程（22）、RS Live Update（25）、RS 握手（01 的 §2.4）。

---

## 1. 概念：调用权限——VM 的"服务门禁"

### 1.0 章节引言

03 文档回答了"VM 如何知道这条消息**来自谁**"（`vm_isokendpt` 身份验证）。本文档回答下一个问题：**就算知道是谁，凭什么让他调用**？——VM 是一个运行在用户态的服务器，任何进程都可以向它的 endpoint 发 IPC 消息。消息到达后、服务函数执行前，必须有一道**权限门禁**：调用者被允许调用哪个 VM 服务？

这道门禁在 Minix3 里叫 **ACL（Access Control List）**，实现为一张"调用号 → 允许/禁止"的位图。本文档从三个问题展开：

1. **权限从哪来**：一个进程的权限是**与生俱来**还是**被授予**的？——答案是后者：由 RS（Reincarnation Server）在服务启动时显式授予（`acl_set`）。
2. **怎么查**：一条消息带着调用号进来，VM 如何判定允许/拒绝？——查调用者位图的对应位（`acl_check`）。
3. **怎么传承**：fork 出来的子进程、退出掉的进程，权限如何继承/回收？（`acl_fork` / `acl_clear`）。

### 1.1 一个容易被忽略的安全面：VM 为什么要自己的访问控制

VM 是 Minix3 里权限最大的用户态服务器：它管理所有进程的页表、物理内存映射、缺页处理。如果**任何进程都能随意调用 VM 的所有服务**，那么：

- 一个普通用户进程可以直接 `VM_MAP_PHYS` 把物理内存映射进自己的地址空间——绕过内核的内存隔离；
- 可以直接 `VM_RS_SET_PRIV` 给自己授予特权——权限模型整体失效；
- 可以直接 `VM_GETPHYS` 读取任意物理地址的内容——机密性被破坏。

所以 VM 必须在自己的服务入口处做**最小权限**控制：普通用户进程只被允许做"与自己内存相关"的操作（`VM_BRK`/`VM_MMAP`/`VM_MUNMAP`……），而物理内存操作、RS 特权操作只对系统服务开放。这就是 ACL 的存在理由。

**注意边界**：ACL 不是 VM 的第一道防线，而是最后一道。消息进入 VM 的顺序是：

```
IPC 到达 → 内核 IPC 过滤（谁允许向 VM 发消息，kernel priv 表）
        → vm_isokendpt（03：caller 身份验证，endpoint → slot）
        → 调用号合法性（main.c:165-166，越界/未注册 → ENOSYS）
        → acl_check（本文档：caller 被允许调用这个服务吗？）→ 服务执行
```

ACL 管的是"**已通过身份验证的进程**能调用哪些服务"——它与 03 的 `vm_isokendpt` 构成两级防线：**身份门禁**（你是谁）在前，**权限门禁**（你能做什么）在后。

### 1.2 两个正交维度：生命周期状态 × 权限策略

阅读 `acl.c` 最容易犯的错误是把 `vm_acl` 当成"权限等级"（越大越强）。实际上它同时承载**两个正交维度**：

| 维度 | 问题 | 取值 | 本质 |
|------|------|------|------|
| 生命周期状态 | 这个进程**是否已被权限系统接管**？ | `NO_ACL (-1)` | 尚未被 RS 管理 |
| 权限策略 | 允许调用哪些服务？ | `USER_ACL (0)` / 系统槽 `1..63` | 策略内容 |

`NO_ACL` 不是"零权限"，而是"**尚未进入权限系统**"——它是进程生命周期里的一个暂时状态。这解释了为什么 C 代码对 `NO_ACL` 的处理是"放行 + 告警"而不是"拒绝"：因为启动早期（RS 尚未接管任何进程之前）确实存在"没有 ACL 但必须能跑起来"的窗口。**但这个"暂时"在 C 里没有截止日期**——`acl.c:44` 的注释 `all calls are allowed.. for now` 自己承认了这一点（见 §2.3 与 §3.3 的语义偏移讨论）。

Rust 侧把这两个维度重新显式建模为 `AclState` 枚举的三个变体（§3.1），并修正了"暂时放行"这个安全缺口（§3.3，`[ARCH: A-11]`）。

### 1.3 门禁的形态：调用号位图（call mask）

权限策略的载体是一张**位图**：每个 VM 调用号（`VM_RQ_BASE+0` 到 `VM_RQ_BASE+48`，共 49 个，见 `com.h:627-766`）对应一个位，1 = 允许、0 = 拒绝。调用号减去 `VM_RQ_BASE` 得到位偏移：

```
VM_EXIT(0xC00)  → 位 0    VM_FORK(0xC01) → 位 1    VM_BRK(0xC02) → 位 2
VM_MMAP(0xC0A)  → 位 10   VM_MAP_PHYS(0xC0F) → 位 15 ...
VM_RS_PREPARE(0xC30) → 位 48
```

三个值得注意的事实：

1. **`VM_PAGEFAULT`（偏移 0xFF）不在位图内**：它是内核直接注入的缺页通知，不走 IPC 请求通道，偏移 255 也远超 64 位宽度——因此天然被排除在 ACL 之外（`com.h:773` 的注释 `not handled as a normal VM call` 印证）。
2. **49 个调用号全部落在 64 位内**：Minix3 用一个 `bitchunk_t`（32 位）数组、每进程 2 个 chunk 表达；Rust 直接用一个 `u64`（§3.2）。
3. **位图是"允许列表"（deny-by-default）**：某位为 0 就是拒绝——与 Linux seccomp 的"过滤器未允许即拒绝"同构（§3.7 对比）。

### 1.4 权限的完整生命周期：init → set → fork → clear

权限不是静态配置，它随进程的生死流转。Minix3 用 5 个函数覆盖整个生命周期：

```
init_vm() ──► acl_init()：全表复位（所有进程 = NO_ACL，所有位图清零）
                    │
    ┌───────────────┼──────────────────────────┐
    ▼               ▼                          ▼
RS 接管服务      fork 子进程                 进程退出
acl_set()       acl_fork()                  acl_clear()
│               │                           │
│ 授予/更新权限  ├─ 用户进程：继承 Default   └─ 释放槽位 + 复位 NO_ACL
│               └─ 系统进程：不继承 → NO_ACL
└─ 先清旧、后设新
```

这四条边对应进程生命周期状态机（02 文档）的"激活/复制/回收"三阶段：ACL 是挂在该状态机上的一个**可独立流转的属性**。这正是 Rust 把它设计成 `AclState` 值类型（`Copy`、随状态机赋值/替换）的原因（§3.1、§3.6）。

### 1.5 与 03 的分工边界：身份门禁 vs 权限门禁

| 层 | 文档 | 回答的问题 | 失败后果 |
|----|------|-----------|---------|
| 身份（endpoint→slot） | 03-vmproc-table | 这条消息来自**哪个进程**？ | 拒绝/panic（EINVAL/EDEADEPT） |
| 权限（call→mask） | 本文档 | 这个进程**被允许调用**这个服务吗？ | 拒绝（EPERM） |

两者的差异决定了不同的实现策略：

- `vm_isokendpt` 需要**查全局表**（endpoint 编码 → 槽号 → 身份比对），所以它是表层的函数；
- `acl_check` 只需要**查调用者自己的位图**，与表无关——所以 Rust 把它设计成 `AclState` 的方法（§3.5），与 typestate 层级解耦。

### 1.6 本章小结

VM 的 ACL 是服务入口的最后一道门禁，管理"已通过身份验证的进程能调用哪些 VM 服务"：

- **两个正交维度**：生命周期状态（是否被 RS 接管）× 权限策略（允许哪些调用）——C 用一个 `int` 承载两维，Rust 用 `AclState` 枚举显式建模；
- **位图门禁**：49 个调用号 → 64 位允许列表，`VM_PAGEFAULT` 天然排除；
- **生命周期**：`acl_init`（复位）→ `acl_set`（授予）→ `acl_fork`（继承）→ `acl_clear`（回收）；
- **两级防线**：03 的身份验证（你是谁）+ 本文档的权限验证（你能做什么）。

后续章节依次回答：C 如何用全局槽位表实现（§2）→ Rust 如何用类型系统重表达（§3）→ 代码如何落地（§4）→ 测试如何证明（§5）。

---

## 2. C 源码分析

### 2.1 常量与数据结构（acl.c:10-15）

```c
#define NO_ACL		-1        /* acl.c:10 */
#define USER_ACL	 0        /* acl.c:11 */
#define FIRST_SYS_ACL	 1        /* acl.c:12 */

static bitchunk_t acl_mask[NR_SYS_PROCS][VM_CALL_MASK_SIZE];   /* acl.c:14 */
static bitchunk_t acl_inuse[BITMAP_CHUNKS(NR_SYS_PROCS)];      /* acl.c:15 */
```

| 符号 | 值 | 语义 |
|------|----|------|
| `NO_ACL` | -1 | 生命周期状态：进程尚未被 RS 管理（不是"零权限"） |
| `USER_ACL` | 0 | **所有用户进程共享**的默认权限槽 |
| `FIRST_SYS_ACL` | 1 | 系统进程独占权限槽的起始索引 |
| `acl_mask[64][2]` | 64 × 64 位 | 权限位图表：第一维是槽索引，第二维是调用号位图 |
| `acl_inuse[2]` | 64 位 | 槽占用位图：`SET_BIT` = 该槽至少被一个进程引用 |

两个 static 数组的规模由系统常量决定（`sys_config.h:9` `_NR_SYS_PROCS = 64`、`com.h:769-770` `NR_VM_CALLS = 49` → `VM_CALL_MASK_SIZE = BITMAP_CHUNKS(49) = 2`，`bitmap.h:12-13`）。**这是典型的共享引用计数式设计**：64 个系统进程槽，多个进程可以共享同一个槽位（`acl_inuse` 记录"至少一个进程在用"，§2.4/§2.5 展示它如何被引用与释放）。

### 2.2 acl_init()：一次性初始化（acl.c:21-30）

```c
void acl_init(void)
{
	int i;

	for (i = 0; i < ELEMENTS(vmproc); i++)
		vmproc[i].vm_acl = NO_ACL;

	memset(acl_mask, 0, sizeof(acl_mask));
	memset(acl_inuse, 0, sizeof(acl_inuse));
}
```

在 `init_vm()` 里、进程表 `memset` + `vm_slot` 赋值之后被调用（`main.c:465`，03 文档 §2.2）。它做三件事：

1. 全表 `vm_acl = NO_ACL`——**所有进程回到"未被接管"状态**；
2. `acl_mask` 清零——所有权限位图清空；
3. `acl_inuse` 清零——所有系统槽释放。

Rust 侧没有对应函数：进程表在编译期就构造为 `AclState::Uninitialized`，位图内联于进程状态，无全局表可清（§3.4）。

### 2.3 acl_check()：单点门禁（acl.c:37-61）

```c
int acl_check(struct vmproc *vmp, int call)
{
	/* VM makes asynchronous calls to itself.  Always allow those. */
	if (vmp->vm_endpoint == VM_PROC_NR)        /* acl.c:41-42 */
		return OK;

	/* If the process has no ACL, all calls are allowed.. for now. */
	if (vmp->vm_acl == NO_ACL) {               /* acl.c:45-54 */
		/* RS instrumented with ASR may call VM_BRK at startup. */
		if (vmp->vm_endpoint == RS_PROC_NR)
			return OK;

		printf("VM: calling process %u has no ACL!\n",
		    vmp->vm_endpoint);
		return OK;
	}

	/* See if the call is allowed. */
	if (!GET_BIT(acl_mask[vmp->vm_acl], call)) /* acl.c:57-58 */
		return EPERM;

	return OK;
}
```

检查逻辑是三层递进：

| 层 | 条件 | 结果 | 防什么 |
|----|------|------|--------|
| 1 | `vm_endpoint == VM_PROC_NR` | 恒放行 | VM 对自己做异步调用（如 `VM_BRK`）不需要授权 |
| 2 | `vm_acl == NO_ACL` | 放行 + 告警（RS 例外不告警） | 启动窗口期：进程尚未被 RS 接管但必须能运行 |
| 3 | 位图对应位为 0 | `EPERM` | 未被授权的服务调用 |

**诚实声明**：第 2 层是**fail-open**（放行 + 告警）。`acl.c:44` 的注释 `all calls are allowed.. for now` 明确承认这是临时放松——但 C 侧没有任何机制让它"到期"，这个窗口会一直开着。Rust 侧对此做了安全修正（§3.3，`[ARCH: A-11]`），这是本文档最重要的语义偏移。

**唯一调用点**：`acl_check` 只在主循环分发处被调用（`main.c:168`），在 `callnr` 合法性检查（`main.c:165-166`，`c < 0 || !vm_calls[c].vmc_func`）之后、`vm_calls[c].vmc_func(&msg)` 之前。拒绝时打印 `VM: unauthorized %s by %d`（`main.c:169-170`）。**注意回复 errno**：`result` 在分发前被初始化为 `ENOSYS`（`main.c:145`，注释 "Out of range or restricted calls return this."），拒绝路径不会覆盖它——所以调用者收到的是 `ENOSYS`，而非 `acl_check` 内部的 `EPERM`。这一"单点门禁"的接线细节归 15-ipc-dispatch，本文档只确认它的存在与语义。

### 2.4 acl_set()：槽位分配与授权（acl.c:70-101）

```c
void acl_set(struct vmproc *vmp, bitchunk_t *mask, int sys_proc)
{
	int i;

	acl_clear(vmp);                            /* acl.c:74：先清旧 */

	if (sys_proc) {
		for (i = FIRST_SYS_ACL; i < NR_SYS_PROCS; i++)
			if (!GET_BIT(acl_inuse, i))    /* acl.c:77-79：找空闲槽 */
				break;
		if (i == NR_SYS_PROCS) {             /* acl.c:86-89：耗尽 */
			printf("VM: no ACL entries available!\n");
			return;
		}
	} else
		i = USER_ACL;                        /* acl.c:90-91 */

	if (!GET_BIT(acl_inuse, i) && mask == NULL)
		printf("VM: WARNING: inheriting uninitialized ACL mask\n"); /* acl.c:93-94 */

	SET_BIT(acl_inuse, i);                     /* acl.c:96：占用标记 */
	vmp->vm_acl = i;                           /* acl.c:97：挂到进程 */

	if (mask != NULL)
		memcpy(&acl_mask[vmp->vm_acl], mask, sizeof(acl_mask[0])); /* acl.c:99-100 */
}
```

语义要点：

1. **先清后设**（`acl_clear` 在函数开头）——同一函数内完成"旧权限回收 + 新权限授予"；
2. **用户进程固定共享槽 0**（`USER_ACL`），不分配新槽；
3. **系统进程线性扫描 `acl_inuse`**，从 `FIRST_SYS_ACL` 起找第一个空闲槽——多个系统进程可以共享同一槽位（`acl_inuse` 的位表示"该槽被引用"，`acl_clear` 时才清除）；
4. **槽耗尽**：64 个槽全被占用时打印 `no ACL entries available!` 并**静默放弃**（进程保持原 ACL）——注释（`acl.c:81-85`）说"这不该发生，否则意味着用户进程被分别分配了独立掩码，是 RS 的责任"；
5. **空掩码告警**：向一个未初始化的槽继承 mask（`mask == NULL`）时打印 WARNING。

**调用点**（两个）：

- `map_service()`（`main.c:765`）：boot 阶段 RS 握手后为每个 boot 服务调用——`acl_set(&vmproc[proc_nr], rpub->vm_call_mask, !IS_RPUB_BOOT_USR(rpub))`；`IS_RPUB_BOOT_USR` 只在 `endpoint == INIT_PROC_NR` 时为真（`rs.h:188`），所以 **INIT 以外的 boot 服务都是系统槽**；
- `do_rs_set_priv()`（`rs.c:34-64`）：运行时 RS 显式授权——`mask` 来自 `VM_RS_BUF` 的 `sys_datacopy`；无 buffer 且 `VM_RS_SYS` 时返回 `EINVAL`（`rs.c:56-58`，"sys procs don't share!"）。

### 2.5 acl_fork() / acl_clear()：继承与释放（acl.c:110-129）

```c
void acl_fork(struct vmproc *vmp)              /* acl.c:110-114 */
{
	if (vmp->vm_acl != USER_ACL)
		vmp->vm_acl = NO_ACL;
}
```

**继承规则一句话**：用户进程的子进程继承共享的 `USER_ACL`；其他任何状态（`NO_ACL` 或系统槽）的子进程都回到 `NO_ACL`。设计意图（`acl.c:103-108` 注释）：**系统特权不随 fork 扩散**——系统服务的子进程必须等 RS 显式重新授权才能运行。

调用点：`do_fork`（`fork.c:84-86`）——C 的 fork 先 `*vmc = *vmp` 整结构复制父进程（包括 `vm_acl`），再 `acl_fork(vmc)` 修正。**"复制后修正"是 C 的实现细节**：Rust 的 typestate 没有整结构复制，改为显式逐字段 + `copy_acl_from`（§3.6）。

```c
void acl_clear(struct vmproc *vmp)             /* acl.c:121-129 */
{
	if (vmp->vm_acl != NO_ACL) {
		if (vmp->vm_acl != USER_ACL)
			UNSET_BIT(acl_inuse, vmp->vm_acl);  /* 释放系统槽引用 */
		vmp->vm_acl = NO_ACL;
	}
}
```

**释放规则**：系统槽释放引用标记（`UNSET_BIT(acl_inuse, ...)`），用户槽（`USER_ACL`）是永久共享的不释放，`NO_ACL` 时直接跳过（幂等）。调用点：`clear_proc()`（`exit.c:48`）——进程槽回收时一并清权限。

**两个小陷阱**（C 语义的边界，供 Rust 对照）：

1. `acl_clear` 只 `UNSET_BIT`，**不清空 `acl_mask` 里的位图内容**——槽位复用后新进程 `memcpy` 新 mask 前，旧位图残留可见；
2. `acl_fork` 只对**子进程**操作——父进程的 ACL 保持不变，这是"继承"语义（复制）而非"转移"。

### 2.6 调用点全景：6 个接线点

| 调用点 | C 位置 | 函数 | 语义归属 |
|--------|--------|------|---------|
| `acl_init()` | `main.c:465` | 初始化 | 本文档（§2.2） |
| `acl_check()` | `main.c:168` | 主循环分发门禁 | 接线归 15-ipc-dispatch；语义归本文档 |
| `acl_set()` | `main.c:765`（`map_service`） | boot 服务授权 | 握手流程归 01/15；授权语义归本文档 |
| `acl_set()` | `rs.c:63`（`do_rs_set_priv`） | RS 运行时授权 | 归 25-rs-services；授权语义归本文档 |
| `acl_fork()` | `fork.c:86` | fork 权限派生 | 全流程归 18-vm-fork；派生语义归本文档 |
| `acl_clear()` | `exit.c:48`（`clear_proc`） | 进程回收清权 | 全流程归 22-vm-exit；清除语义归本文档 |

**位置可回答性**：本文档位于 `init_vm()` 时序的 `acl_init()` 锚点（`main.c:465`），并在运行期主循环分发处（`main.c:168`）作为服务执行前的最后一道门禁存在——门禁的接线细节由 15 文档讲述。

### 2.7 位图宏与掩码宽度（bitmap.h:12-20、com.h:769-773）

```c
#define BITCHUNK_BITS   (sizeof(bitchunk_t) * CHAR_BIT)   /* bitmap.h:12 */
#define BITMAP_CHUNKS(nr_bits) (((nr_bits)+BITCHUNK_BITS-1)/BITCHUNK_BITS) /* bitmap.h:13 */
#define MAP_CHUNK(map,bit) (map)[((bit)/BITCHUNK_BITS)]   /* bitmap.h:14 */
#define CHUNK_OFFSET(bit) ((bit)%BITCHUNK_BITS)           /* bitmap.h:15 */
#define GET_BIT(map,bit) ( MAP_CHUNK(map,bit) & (1 << CHUNK_OFFSET(bit) )) /* bitmap.h:16 */
#define SET_BIT(map,bit) ( MAP_CHUNK(map,bit) |= (1 << CHUNK_OFFSET(bit) )) /* bitmap.h:17 */
#define UNSET_BIT(map,bit) ( MAP_CHUNK(map,bit) &= ~(1 << CHUNK_OFFSET(bit) )) /* bitmap.h:18 */
```

`bitchunk_t` 是 32 位（`typedef uint32_t bitchunk_t`，`minix3/sys/sys/types.h:124`），所以 `NR_VM_CALLS=49` 需要 2 个 chunk（64 位）；`acl_inuse` 需要 `BITMAP_CHUNKS(64)=2` 个 chunk。这些宏在 `acl.c` 里既用于**调用号位图**（`acl_check` 的 `GET_BIT`、`acl_set` 的 `memcpy`）也用于**槽占用位图**（`acl_inuse` 的 `GET_BIT`/`SET_BIT`/`UNSET_BIT`）——同一套宏服务于两种不同语义的位图，是 C 侧"位图语义混杂"的一个小注脚（Rust 用 `AclMask` bitflags 与枚举变体区分，§3.1/§3.2）。

---

## 3. Rust 设计决策

> 本节 6 条设计决策均附 C 对照点、理由与行为契约，自包含可读；快照类中间产物不在此引用。

### 3.1 D1: AclState enum——两维正交性的类型表达

- **C**: `int vm_acl`（`vmproc.h:23`），一个 int 同时表达生命周期状态（-1）与权限策略（0 或 1..63）
- **Rust**: `enum AclState { Uninitialized, Default, System(AclMask) }`（`acl.rs:74-79`）

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AclState {
    Uninitialized,
    Default,
    System(AclMask),
}
```

三个变体与 C 的映射：`Uninitialized ↔ NO_ACL(-1)`、`Default ↔ USER_ACL(0)`、`System(mask) ↔ vm_acl >= FIRST_SYS_ACL`。

**为什么值得做这个建模**：C 的 `int` 允许大量非法组合（负值、槽 0 之外的任何"用户进程"取值、越界索引），而 enum 让**非法状态不可表示**。特别是：

- `Default` 不带数据——共享默认权限是系统常量，不应被修改，也不需要为每个进程存一份；
- `System(mask)` 携带自己的位图——"恰好拥有默认权限的系统进程"与"用户进程"在类型上是不同的变体，语义不会被位图内容掩盖。

**为什么保留 `Uninitialized` 而不是消灭它**：它对应真实的生命周期阶段（进程槽创建后、RS 接管前），且 §1.2 强调它是"生命周期状态"而非"权限策略"。消灭它会让"尚未被 RS 管理"这个事实变得不可见。

### 3.2 D2: AclMask bitflags——单源位偏移

- **C**: `acl_mask[64][2]` + 手工 `GET_BIT/SET_BIT` 宏；位偏移 = 调用号 - `VM_RQ_BASE`，需要在 `com.h`（定义调用号）与 `acl.c`（使用位偏移）之间手工保持一致
- **Rust**: `bitflags! struct AclMask: u64`，每一位由 `minix-types` 的 `VM_*` 常量直接计算（`acl.rs:26-60`）：

```rust
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct AclMask: u64 {
        const VM_EXIT = 1 << (VM_EXIT - VM_RQ_BASE);
        const VM_FORK = 1 << (VM_FORK - VM_RQ_BASE);
        // ... 全部 30 个已定义调用，位偏移 0..=48 ...
    }
}
```

**单源**：调用号只在 `minix-types`（`os/libs/minix-types/src/ipc/vm.rs`）定义一次，位偏移由常量表达式推导——消除了 C 中"com.h 与 acl.c 两处维护"的漂移面。`u64` 覆盖全部 49 位（§1.3）；`VM_PAGEFAULT`（偏移 0xFF）无法表示也不需要（§1.3 事实 1）。我把 C 的 `acl_mask[i]` 语义完整保留为 `AclMask` 的位集合，并预定义了用户共享集：

```rust
impl AclMask {
    pub(crate) const DEFAULT: Self = Self::from_bits_truncate(
        Self::VM_EXIT.bits() | Self::VM_FORK.bits() | Self::VM_BRK.bits()
        | Self::VM_EXEC_NEWMEM.bits() | Self::VM_WILLEXIT.bits()
        | Self::VM_MMAP.bits() | Self::VM_MUNMAP.bits()
    );
}
```

`DEFAULT` 包含 7 个调用：退出、fork、brk、exec 新内存、将退出、mmap、munmap——恰好是**普通进程生命周期必需**的操作集合（对应 C 中由 RS 在启动时为用户进程写入的 `acl_mask[0]` 面）。

### 3.3 D3: Uninitialized fail-closed——语义偏移（`[ARCH: A-11]`）

这是本文档**最重要的诚实声明**：Rust 对 `Uninitialized` 的检查策略与 C 不同。

| 方面 | Minix3 C | minix-rs |
|------|----------|----------|
| 检查代码 | `acl.c:45-54`：`NO_ACL` → 放行 + 告警（RS 例外） | `acl.rs:102-116`：`Uninitialized` → 仅放行 `DEFAULT` 集 |
| 默认策略 | **fail-open**（"for now" 无截止日期） | **fail-closed**（未授权即拒绝） |
| 启动窗口兼容 | 全放行保证不破坏早期启动 | `DEFAULT` 覆盖启动所需（RS 的 `VM_BRK`、用户进程的基础调用） |

```rust
AclState::Uninitialized => {
    // SECURITY FIX [ARCH: A-11]: Restrict to DEFAULT calls instead of allowing all.
    // Minix3's NO_ACL allows all calls ("for now" — acl.c:44-53) ...
    let call_flag = AclMask::from_bits_truncate(1u64 << call);
    if AclMask::DEFAULT.contains(call_flag) {
        Ok(())
    } else {
        Err(VmError::PermissionDenied)
    }
}
```

**为什么这是正确的演进**：

1. C 的 fail-open 是**历史债务**——注释自己承认 "for now"，但没有任何机制让窗口关闭；一个被遗忘的、未接管的进程将永久拥有全部 VM 权限；
2. 特权调用（`VM_MAP_PHYS`、`VM_RS_SET_PRIV`、`VM_RS_PREPARE` 等）必须经 RS 显式授权（`acl_set`/`do_rs_set_priv`）——这与"最小权限"原则一致：**没有证据表明被授权 = 没有权限**；
3. `DEFAULT` 集覆盖启动路径的真实需求：RS 在 ASR 场景下调用 `VM_BRK`（C 注释 `acl.c:46` 明说），用户进程调用退出/映射基础服务——fail-closed 不会破坏正常启动；
4. 与 Linux seccomp 的哲学同构：**过滤器未显式允许的调用一律拒绝**（§3.7）。

**风险登记**：若未来 C 侧 RS 在 boot 早期依赖某个不在 `DEFAULT` 内的特权调用，fail-closed 会拒绝它——届时需要把该调用加入 `DEFAULT` 或提前完成 RS 握手授权。这是可预期的、需要显式决策的边界（已同步到 plan.md §4 A-11）。

### 3.4 D4: 内联权限——消除全局槽位表

- **C**: `acl_mask[64][2]`（512B）+ `acl_inuse[2]` static 全局；`acl_set` 线性扫描槽位、`SET_BIT/UNSET_BIT` 引用计数、耗尽时放弃（§2.4）
- **Rust**: 无全局表；`System(AclMask)` 权限内联于进程状态（8 字节）；`acl_set` 为**纯函数**返回新状态（`acl.rs:147-160`）；`acl_clear` 幂等返回 `Uninitialized`（`acl.rs:179-181`）

```rust
pub(crate) fn acl_set(sys_proc: bool, mask: Option<AclMask>) -> Self {
    if sys_proc {
        match mask {
            Some(m) => AclState::System(m),
            None => AclState::System(AclMask::empty()),  // C: "WARNING: inheriting uninitialized ACL mask"
        }
    } else {
        AclState::Default
    }
}
```

**为什么消除槽位表**：

1. 槽位表是 1980 年代的内存优化（64 槽 × 64 位 = 512B）；内联后每进程 8B × 257 槽 ≈ 2KB——数量级相同，但消灭了共享引用计数；
2. **槽位耗尽**（C 的 `no ACL entries available!` 静默放弃）在类型层面不可能发生——这是 C 侧一个"RS 负责不犯错"的注释级假设（`acl.c:81-85`），Rust 直接删掉了这个失败面；
3. **共享引用计数是经典 bug 面**：`acl_set` 先清后设、`acl_clear` 只在非 `USER_ACL` 时 `UNSET_BIT`——C 的"哪条路径该释放、哪条不该"容易出错；Rust 值语义下，赋值即替换，**不存在"忘记清旧"的路径**；
4. `AclState` 是 `Copy` 值类型——`set_acl(acl_set(...))` 原子完成"清旧 + 设新"，对应 C 的 `acl_set` 内部先调 `acl_clear` 的语义（§2.4 要点 1）。

**语义核对**：`acl_set(sys=true, None)` → `System(empty)`。C 中"空 mask 继承"会打印 WARNING 且**继承槽内已有位图**；Rust 中"空掩码"是显式的零权限（fail-closed 一致）——行为不同但更安全，属于 D3 同族的安全演进，非静默偏差。

### 3.5 D5: acl_check 签名——endpoint 解耦

- **C**: `acl_check(struct vmproc *vmp, int call)`（`acl.c:37`）——依赖 `vmp->vm_endpoint` 判断 VM 自调用豁免
- **Rust**: `AclState::acl_check(&self, endpoint: Endpoint, call: u32) -> Result<(), VmError>`（`acl.rs:96`）

```rust
pub(crate) fn acl_check(&self, endpoint: Endpoint, call: u32) -> Result<(), VmError> {
    if endpoint == Endpoint::VM {
        return Ok(());                     // acl.c:41-42：VM 异步自调用恒放行
    }
    match self { ... }
}
```

**理由**：ACL 检查只依赖两个输入（调用者 endpoint + 调用号），与进程表的 typestate 层级（`Empty/Active/Exiting` 视图）无关。把检查挂在 `AclState` 上、由 typestate 层的薄封装转发（§4.3），既保留了"权限内联于进程"的模型，又不让 ACL 逻辑感知表结构。错误类型 `VmError::PermissionDenied` 映射 `EPERM`（`os/libs/minix-types/src/ipc/vm.rs:618`），与 C 的 `EPERM` 精确对齐。

### 3.6 D6: 继承与清除——typestate 层集成

C 的 fork 继承是"整结构 memcpy + `acl_fork` 修正"（`fork.c:84-86`）。Rust 的 typestate 没有整结构复制，改为：

```rust
// vmproc_handle.rs:314-322：init_from_fork 明确不设置 ACL
// vmproc_handle.rs:331-333：调用方显式继承
pub(crate) fn copy_acl_from(&mut self, parent: &ActiveProc<'_>) {
    self.inner.vm_acl = parent.inner.vm_acl.acl_fork();
}
```

- `init_from_fork` 只初始化 endpoint/内存统计/region_top（`vmproc_handle.rs:314-322`），**ACL 留白**；
- 调用方在 fork 流程中显式 `child.copy_acl_from(&parent)`（`fork.rs:215`）——等价于 C 的"复制 + 修正"两段式，但**不存在"复制了旧特权忘了修正"的路径**（`AclState::acl_fork` 对 `System(_)` 返回 `Uninitialized`，§1.4 继承规则原样保留）；
- 进程回收：`VmProc::clear()`（`vmproc.rs:139-185`）内置 `self.vm_acl = AclState::Uninitialized`（`vmproc.rs:185`）——对应 C 的 `clear_proc → acl_clear`（`exit.c:48`）。

### 3.7 与 Redox / 主流 OS 权限模型的对比

> 目的：验证"调用号位图"这一抽象在 2026 年的语境下是否仍然合理，以及 minix-rs 的设计吸取了哪些同行经验。

**Redox OS：资源句柄（scheme）模型，不做按调用号过滤。** Redox 的内核把 I/O 抽象为命名空间式 scheme（`file:`/`display:`/`irq:`……），进程通过持有资源句柄获得访问权——权限附着在**句柄**上，而不是在**系统调用号**上。对 Redox 而言，"VM 服务"不存在：内存管理是内核职责，调用者要么持有对应 capability，要么根本走不到内核服务。**启示**：句柄模型适合"权限可以随资源传递"的场景（打开文件 → 得到句柄 → 用句柄 I/O）；而 VM 的 ACL 面对的是**无状态的服务调用面**（每个请求独立、无握有物），按调用号过滤是这一场景的正确粒度——把 Minix3 的位图换成句柄体系，会不必要地引入与现有 RS 授权协议（`rprocpub.vm_call_mask`）不兼容的架构变化。minix-rs 保留位图语义（外部行为不变），但把"全局共享槽表"改为"每进程内联掩码"——这一步向"能力集随进程携带"（seL4 式）收敛，是演进而非翻译。

**Linux seccomp：按系统调用过滤，fail-closed。** seccomp 过滤器对"未显式允许的调用"按配置动作拒绝（常见 `SECCOMP_RET_ERRNO` 返回 `EPERM`，或 `KILL`），且过滤规则附着在**任务**上（每任务一份）。minix-rs 的 `Uninitialized` fail-closed（§3.3）与 seccomp 的默认拒绝哲学一致；`AclMask` 内联于进程状态也与"过滤规则随任务"同构。C 的全局槽位表（共享 + 引用计数）反而是异类——它把"每进程策略"退化成"每槽策略 + 进程到槽的映射"，徒增别名与耗尽问题。

**seL4：能力（capability）与访问权。** seL4 的每个对象调用都要求线程持有对应 capability，且 capability 携带访问权（Read/Write/Grant）。对照 Minix3 的 ACL 槽：`acl_inuse` 的 `SET_BIT/UNSET_BIT` 本质上是**无引用计数的共享能力位**——多个进程共享一个槽、释放只清位不清内容（§2.5 陷阱 1），这是能力系统中最容易出错的部分。minix-rs 用 `System(mask)` 值语义消灭了共享与计数，与 seL4 "每线程持有自己的能力"原则对齐。

**结论**：调用号位图在 VM 场景仍然正确（Redox 的句柄模型不适用于无状态服务面）；但实现应从"全局共享槽表"演进为"每进程内联掩码 + fail-closed"（seccomp/seL4 的共同教训），这正是 D3/D4 的设计选择。

---

## 4. 实现详解

### 4.1 `os/servers/vm/src/acl.rs`：AclMask + AclState

| 位置 | 内容 | 对应 C |
|------|------|--------|
| `acl.rs:26-60` | `AclMask: u64` bitflags，30 个调用位 | `acl_mask[i]` 64 位面（`acl.c:14`） |
| `acl.rs:62-72` | `AclMask::DEFAULT`（7 位） | 用户进程共享位图（`acl.c:11` 的 `USER_ACL` 槽内容） |
| `acl.rs:74-79` | `AclState` 三变体 | `vm_acl` 取值（`vmproc.h:23`） |
| `acl.rs:81-85` | `Default for AclState → Uninitialized` | `acl_init` 的 `vm_acl = NO_ACL`（`acl.c:26`） |
| `acl.rs:96-135` | `acl_check` | `acl.c:37-61`（D3 偏移） |
| `acl.rs:147-160` | `acl_set`（纯函数） | `acl.c:70-101`（D4：无槽位管理） |
| `acl.rs:166-170` | `acl_fork`（纯函数） | `acl.c:110-114` |
| `acl.rs:179-181` | `acl_clear`（幂等） | `acl.c:121-129`（无槽位释放） |
| `acl.rs:183-189` | `mask()` → `Option<AclMask>` | 无直接对应（诊断/序列化辅助） |

`mask()` 的语义值得说明：`Uninitialized → None`（"未被显式赋权"，而非"零权限"）、`Default → Some(DEFAULT)`、`System(m) → Some(m)`——为诊断与未来 RS 查询预留。

### 4.2 vmproc 集成：字段与 clear()

- `vmproc.rs:31`：`vm_acl: AclState` 字段，构造时 `AclState::Uninitialized`（`vmproc.rs:73`）——进程表编译期即"未接管"；
- `vmproc.rs:185`：`clear()` 内 `self.vm_acl = AclState::Uninitialized`——对应 C 的 `clear_proc → acl_clear`（`exit.c:48`）；
- 与 `VmFlags`/typestate 的关系：ACL 是 `VmProc` 的一个普通字段，不参与 typestate 视图切换（`Empty/Active/Exiting`），但**读写都经过 typestate 方法**（§4.3）——视图边界内无裸访问。

### 4.3 typestate 层：acl() / set_acl() / acl_check() / copy_acl_from()

`os/servers/vm/src/vmproc/vmproc_handle.rs` 提供四个方法：

```rust
pub(crate) fn acl(&self) -> AclState                        // vmproc_handle.rs:236
pub(crate) fn set_acl(&mut self, acl: AclState)             // vmproc_handle.rs:241
pub(crate) fn acl_check(&self, call: u32) -> Result<(), VmError>  // vmproc_handle.rs:246
pub(crate) fn copy_acl_from(&mut self, parent: &ActiveProc<'_>)    // vmproc_handle.rs:331
```

- `acl_check`（`vmproc_handle.rs:246-247`）是薄转发：`self.inner.vm_acl.acl_check(self.inner.vm_endpoint, call)`——把 endpoint 从进程状态注入 `AclState` 方法（§3.5）；
- `copy_acl_from`（`vmproc_handle.rs:331-333`）＝ `parent.inner.vm_acl.acl_fork()`——继承规则见 §3.6；
- `init_from_fork`（`vmproc_handle.rs:318`）的文档注释明确声明"ACL 不在此设置，调用方必须显式 `copy_acl_from`"——把 C 的隐式 memcpy 继承变成显式步骤。

### 4.4 消费方接线：分发门禁 / boot 授权 / RS_SET_PRIV / fork

**分发门禁**（`vm_server.rs:562-587`）——主循环优先级 4（普通 VM 调用）：

```rust
// C: acl_check(&vmproc[caller_slot], c)
if let Some(proc) = table.get_active(caller_slot) {
    if proc.acl_check(c as u32).is_err() {
        // 审计通道（VMA-1）：test/vm_acl_audit 下 eprintln!，release 编译剔除
        return DispatchAction::Reply(VmReply::Error(VmError::NotImplemented));
    }
}
```

拒绝路径回复 `NotImplemented`（→ `ENOSYS`），精确复刻 C 的行为（`main.c:145` 初始化 `result = ENOSYS`，拒绝路径不覆盖——见 §2.3 的"注意回复 errno"）；`AclState::acl_check` 内部仍返回 `Err(PermissionDenied)`（= C `acl_check` 的 `EPERM`），仅作为门禁判定用。审计通道（`vm_server.rs:570-583`）是 checklist VMA-1 修复：拒绝事件不再被静默丢弃，`--features vm_acl_audit` 时输出 `[VM ACL] denied: ...`。

**boot 服务授权**（`vm_server.rs:621-634`，`rs_handshake`）——对应 C 的 `map_service` 循环（`main.c:755-768`）：

```rust
let is_sys = !entry.is_user;   // C: !IS_RPUB_BOOT_USR(rpub)
let mask = Some(crate::acl::AclMask::from_bits_truncate(entry.call_mask as u64));
proc.set_acl(crate::acl::AclState::acl_set(is_sys, mask));
```

**诚实声明**：`rs_handshake` 的 IPC 尚未接线——`ipc_call_rs_init()` 目前返回 `Ok(RprocTab::empty())`（`vm_server.rs:898-931`，DEFERRED），`is_user`/`call_mask` 字段来自占位结构。因此该路径的**协议语义**已对齐（`!IS_RPUB_BOOT_USR` ↔ `!entry.is_user`），但**运行时数据流**依赖 01/15 的 RS 握手落地。

**RS 运行时授权**（`rs.rs:115-140`，`handle_rs_set_priv`）——对应 C 的 `do_rs_set_priv`（`rs.c:34-64`）：

```rust
if mask.is_none() && is_sys_proc {
    return Err(RsError::SysProcNoMask);   // C: rs.c:56-58 → EINVAL
}
let acl = AclState::acl_set(is_sys_proc, mask);
active.set_acl(acl);
```

**fork 派生**（`fork.rs:215`）：`child.copy_acl_from(&parent)`——子进程槽激活后、页表/区域复制前完成权限派生（§3.6）。

---

## 5. 测试要点

### 5.1 单元测试清单（`os/servers/vm/src/acl.rs`，13 个）

| 测试 | 验证目标 |
|------|---------|
| `test_acl_state_default` | 默认状态 = `Uninitialized` |
| `test_acl_check_uninitialized` | fail-closed：DEFAULT 7 调用放行、特权调用（`VM_MAP_PHYS`/`VM_RS_SET_PRIV`/`VM_RS_PREPARE`）拒绝 |
| `test_acl_check_vm_proc` | VM 端点恒放行（任意调用号） |
| `test_acl_check_default` | Default 与 Uninitialized 检查结果一致（DEFAULT 集） |
| `test_acl_check_system` | `System(mask)` 按位图检查（授权放行 / 未授权拒绝） |
| `test_acl_fork_default` / `_uninitialized` / `_system` | 三种继承路径（Default→Default、Uninitialized→Uninitialized、System→Uninitialized） |
| `test_acl_set_user` | `acl_set(false, _)` → `Default`（掩码无关） |
| `test_acl_set_system` | `acl_set(true, Some(m))` → `System(m)`；`acl_set(true, None)` → `System(empty)` |
| `test_acl_clear` | 三态清除均返回 `Uninitialized`（幂等） |
| `test_acl_mask_default` | DEFAULT 成员/非成员断言 |
| `test_acl_state_mask` | `mask()` 映射（None / DEFAULT / 自定义） |

### 5.2 覆盖维度

- **三态 × 检查**：`Uninitialized`/`Default`/`System` 的 `acl_check` 全覆盖（含 VM 豁免）；
- **继承三路径**：`acl_fork` 全覆盖；
- **赋值语义**：`acl_set` 四象限（user×mask、sys×mask）全覆盖；
- **errno 对齐**：`AclState::acl_check` 内部 `VmError::PermissionDenied → EPERM`（`os/libs/minix-types/src/ipc/vm.rs:618`，= C `acl_check` 的 `EPERM`）；分发处**回复** `NotImplemented → ENOSYS`（= C 主循环 `main.c:145` 的回复 errno，§2.3/§4.4）；`RsError::SysProcNoMask → InvalidProcess → EINVAL`（= C `rs.c:56-58`，`rs.rs:579` 有映射测试）；
- **位偏移单源**：`AclMask` 位由 `minix-types` 常量编译期推导——调用号常量变更会直接改变位图，无手工同步面。

### 5.3 覆盖缺口与建议

| 缺口 | 说明 | 归属 |
|------|------|------|
| `rs.rs` 的 `SysProcNoMask` 直接测试 | `handle_rs_set_priv` 的 sys+None → EINVAL 路径仅有 `RsError` 映射测试（`rs.rs:579`），无 handler 级测试 | 25-rs-services |
| 分发处 EPERM 回复端到端测试 | `vm_server.rs:565` 的拒绝 → `DispatchAction::Reply(PermissionDenied)` 依赖真实 IPC 消息构造 | 15-ipc-dispatch |
| `copy_acl_from` 集成测试 | fork 路径中"父 System → 子 Uninitialized"的集成断言 | 18-vm-fork |

### 5.4 测试统计（截至 2026-08-15）

- `cargo test -p minix-vm --lib`：**346 passed / 3 failed（pre-existing**，`alloc_page` 2 项 + `vir_region` 1 项，属 mock 全局态竞态，归 06/13 范围**，与 ACL 无关**）
- 本节列出的 13 个测试为 `acl.rs` 直接相关子集
- 完整清单：`rg "fn test_" os/servers/vm/src/acl.rs`

---

## 6. 过渡

下一站是 **05-physical-memory**：物理内存布局与分配器（`mem_init`/`get_mem_chunks`），它是 `acl_init` 之后、`init_proc` 之前的初始化链下一环。本文档的 ACL 概念会在 15（分发接线）、18（fork 派生）、22（exit 清理）、25（RS 授权）中被反复引用——届时都以"§1.4 生命周期 + §3 设计决策"为语义锚点。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/plan.md` §2（04 职责）、§3.4（边界表）、§4（ARCH A-11）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md` §2.2（`acl_init` 在 init_vm 中的位置）、§2.4（RS 握手）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/02-vmproc-struct.md`（PCB 与生命周期状态机）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/03-vmproc-table.md` §1.4（身份门禁 vs 权限门禁的分工）
- `minix3/minix/servers/vm/acl.c`（ground truth）
- `os/servers/vm/src/acl.rs`（Rust 实现）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/draft/03-acl.md`（素材）
