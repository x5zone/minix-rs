# 05-rs-ipc-sendmask: IPC 发送掩码

> **分类**: 阶段 2 — 权限与隔离（boot Step 1 的 IPC 掩码机制）
> **源码**: `minix3/minix/servers/rs/manager.c:2112-2331`（`get_next_name`/`add_forward_ipc`/`add_backward_ipc`/`init_privs`）、`minix3/minix/servers/rs/utility.c:82-95`（`fill_send_mask` 原语，定义在 03）、`minix3/minix/servers/rs/main.c:272-273`（boot Step 1 的 ALL_M 快捷路径）、`minix3/minix/include/minix/rs.h:29-30`（`RSS_IPC_ALL`/`RSS_IPC_ALL_SYS`）、`minix3/minix/include/minix/priv.h:25,67-69`（`ALL_M`/`SRV_M`/`USR_M`）、`minix3/minix/servers/rs/manager.c:1460-1483`（`edit_slot` 拷入 `r_ipc_list`，归属 08）
> **Rust 模块**: `os/servers/rs/src/ipc_mask.rs`（`IpcListIterator`/`add_forward_ipc`/`add_backward_ipc`/`init_privs`/`update_ipc_mask`）、`os/servers/rs/src/process_table.rs`（`iter_in_use`）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/01-rs-boot-init.md`（boot Step 1）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md`（`ipc_list` 字段归属、表迭代）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/03-rs-privilege.md`（`Privilege::ipc_to`/`SysMap`/`fill_send_mask` 定义、`USER_PRIV_ID`）、`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/23-ipc-filter.md`（内核 send mask 强制语义）
> **说明**: 服务的 `s_ipc_to` 是内核 priv 结构里"允许发往哪些 priv id"的 64 位位图（03 §1.2）。本文档回答两条计算路径：boot Step 1 的**全置快捷路径**（`fill_send_mask(..., ALL_M)`）与动态服务的 **r_ipc_list 解析路径**（`init_privs` → `add_forward_ipc` + `add_backward_ipc`）。后者是 RS 对"服务间消息可达性"的唯一精确配置点，也是 04 的"谁控制谁"之外的另一半隔离语义。

---

## 1. 概念：服务能向谁发消息

### 1.0 章节引言

`03-rs-privilege.md` 建立了权限结构：`s_ipc_to` 是一个 64 位位图，第 i 位对应 priv id i 的目标——内核在服务每次 `send` 时检查该位（`01-stage-kernel/23-ipc-filter.md`）。本文档回答的问题是：**这个位图是怎么算出来的**——具体说，RS 依据什么输入、用什么规则把位图从零构造到完整。

> **本章不讲什么**（机制一律移交）:
> - priv 结构的整体建模与 privctl 提交（`03-rs-privilege.md`）——本文档只产出 `s_ipc_to` 一个字段的值
> - `r_ipc_list` 的拷贝与校验（`edit_slot` 的 `sys_datacopy`，`manager.c:1476-1483`，`08-rs-slot-config.md`）——本文档只消费
> - 运行时掩码更新（`do_edit` → `edit_slot` 重新 `init_privs`，`request.c:348`，`13-rs-control-requests.md`）——本文档只提供原语
> - 内核如何强制执行 send mask（`01-stage-kernel/23-ipc-filter.md`）
> - 信号管理器/调度器等其余 priv 字段（03）
>
> 本章只回答一个问题：**`s_ipc_to` 的两条计算路径，每条路径的输入、规则与 C 行号**。

### 1.1 核心问题：为什么 IPC 可达性需要 RS 配置

微内核的一个核心特性是**服务间隔离**：VM 不需要向 TTY 发消息、MIB 不需要向 PM 发消息。如果所有服务都能互相通信，隔离就形同虚设。内核的 priv 表提供 `s_ipc_to` 位图作为强制机制，但内核**不知道服务间的消息拓扑**——那是系统设计者（服务配置文件）决定的。

RS 的位置使它成为唯一能配置这张拓扑的角色：

```
boot 表 / RS_UP 请求
  │  （r_ipc_list：一个空格分隔的进程名列表，type.h:105）
  ▼
RS 计算 s_ipc_to（本文档的 init_privs）
  │
  ▼  sys_privctl(SET_SYS/UPDATE_SYS)（03）
  ▼
内核按位图强制：send 到未授权目标 → EPERM
```

`r_ipc_list`（`type.h:105`，`MAX_IPC_LIST=256` 字节，const.h:22）就是服务的"消息拓扑声明"：列出它需要通信的进程名。RS 把它翻译成 priv id 位图。

### 1.2 两条计算路径（WHAT）

| 路径 | 触发点 | 输入 | 规则 | 结果 |
|------|--------|------|------|------|
| **boot 全置** | boot Step 1（main.c:272-273） | 静态表 `SRV_M`/`USR_M` | `SRV_OR_USR(rp, SRV_M, USR_M) == ALL_M` → `fill_send_mask(mask, TRUE)` | 全 1（64 位全置） |
| **动态精确** | `init_slot`→`edit_slot`→`init_privs`（manager.c:1700,1794） | `r_ipc_list` 字符串 | `IPC_ALL`/`IPC_ALL_SYS` 全置（带例外）或 forward+backward 组合 | 精确位图 |

为什么 boot 全置？因为 `SRV_M` 和 `USR_M` 在 `priv.h:67-69` 里**都定义为 `ALL_M`**——boot 阶段的内核任务/系统服务之间"什么都可发"。这是 Minix3 的 boot 简化：boot 服务是内核信任的基石集合，运行后由动态服务承担更细的隔离。文档 03 §4.2 已把该路径建模进 `Privilege::boot_priv`（`ipc_to: SysMap::all()`）。

动态精确路径的三元语义是本文档核心：

```
r_ipc_list 内容                 → s_ipc_to
──────────────────────────────────────────────────
"IPC_ALL"                      → 全 1（含 USER_PRIV_ID，manager.c:2327）
"IPC_ALL_SYS"                  → 全 1 除 USER_PRIV_ID（manager.c:2327）
"vm pm tty"（普通列表）         → forward ∪ backward
```

### 1.3 forward 与 backward：为什么需要两次扫描

一个服务的 IPC 列表描述的是**它想发给谁**（forward，`add_forward_ipc`）。但消息拓扑是**双向**的——A 发给 B 意味着 B 也可能发给 A。C 注释（manager.c:2234-2239）说明了 backward 的必要性：

> "We need to add these permissions now because the current process may not yet have existed at the time that the other process was initialized."

即：`add_forward_ipc` 对列表中匹配不到的服务（尚未启动）**静默容忍**（manager.c:2183-2188），当目标后来启动时，由 **`add_backward_ipc`** 扫描**所有其他服务的列表**，把"列表里写了本进程名"的服务补进来。C 注释（manager.c:2236）"as the kernel guarantees send mask symmetry"是行为依据：内核在发送检查时要求双向权限（详见 `01-stage-kernel/23-ipc-filter.md`），所以 RS 必须保证位图对称。

> **对称性注解**：Rust 实现保留该注释为行为说明（`add_backward_ipc` 文档）；内核侧的对称性检查不在本文档范围。

---

## 2. C 源码分析

### 2.1 `fill_send_mask`（utility.c:82-95，定义在 03）

```c
void fill_send_mask(send_mask, set_bits)               /* utility.c:82 */
sys_map_t *send_mask;
int set_bits;
{
  int i;
  for (i = 0; i < NR_SYS_PROCS; i++) {                 /* utility.c:89 */
	if (set_bits)
		set_sys_bit(*send_mask, i);                  /* utility.c:91 */
	else
		unset_sys_bit(*send_mask, i);                /* utility.c:93 */
  }
}
```

"全置/全清"原语。03 定义了其 Rust 等价（`SysMap::all()`/`SysMap::empty()`，privilege.rs:260-267）。本文档两个消费点：boot 全置（§2.2）与 `init_privs` 的清零（manager.c:2308）。

### 2.2 boot Step 1 的 ALL_M 快捷（main.c:272-273）

```c
      ipc_to = SRV_OR_USR(rp, SRV_M, USR_M);                /* main.c:272 */
      fill_send_mask(&rp->r_priv.s_ipc_to, ipc_to == ALL_M);/* main.c:273 */
```

`SRV_OR_USR`（const.h:71）= `rp->r_priv.s_flags & SYS_PROC ? SRV_M : USR_M`。而 `SRV_M` 与 `USR_M` 都等于 `ALL_M`（priv.h:67-69），所以 `ipc_to == ALL_M` 恒真 → 全置。**boot 服务的 `s_ipc_to` 是 64 位全 1**。这段代码在 03 §4.2 的 `boot_priv` 中已建模（`ipc_to: SysMap::all()`），本文档不再展开。

### 2.3 `get_next_name`（manager.c:2115-2152）——IPC 列表分词器

```c
static char *get_next_name(ptr, name, caller_label)    /* manager.c:2115 */
{
	char *p, *q;
	size_t len;

	for (p= ptr; p[0] != '\0'; p= q)                 /* manager.c:2125 */
	{
		while (p[0] != '\0' && isspace((unsigned char)p[0])) /* 2128-2129 */
			p++;
		q= p;
		while (q[0] != '\0' && !isspace((unsigned char)q[0])) /* 2133-2134 */
			q++;
		if (q == p)                                  /* 2135 */
			continue;
		len= q-p;                                    /* 2137 */
		if (len > RS_MAX_LABEL_LEN)                  /* 2138 */
		{
			printf("rs:get_next_name: bad ipc list entry ..."); /* 2140-2141 */
			continue;                                /* 2143 */
		}
		memcpy(name, p, len);                        /* 2145 */
		name[len]= '\0';                             /* 2146 */
		return q; /* found another */                /* 2148 */
	}
	return NULL; /* done */                          /* 2151 */
}
```

语义要点：

1. **空白分隔**：跳过前导空白，单词到下一个空白或 `'\0'` 结束（manager.c:2125-2134）。
2. **超长条目跳过而非截断**：`len > RS_MAX_LABEL_LEN`（16）时打印诊断并 `continue`（manager.c:2138-2143）——错误条目被丢弃，不截断（截断会产生歧义名，可能误匹配到其他服务）。
3. **NUL 终止**：for 循环条件 `p[0] != '\0'`（manager.c:2125）保证列表以 NUL 结束；`r_ipc_list` 由 `edit_slot` 在拷贝后补 NUL（manager.c:1483）。
4. 返回 `q`（下一个词的起点）作为新指针，循环到返回 NULL。

Rust 对应 `IpcListIterator`（ipc_mask.rs:43-90）：纯迭代器，无 unsafe；NUL 与空白都终止单词（与 C 的 `q[0] != '\0' && !isspace(...)` 一致）；超长条目递归跳过（保留 C 的语义，丢弃 no_std 下的 printf 诊断，A-11 家族）。

### 2.4 `add_forward_ipc`（manager.c:2157-2224）——正向扫描

```c
void add_forward_ipc(rp, privp)                        /* manager.c:2157 */
{
	p = rp->r_ipc_list;                                /* manager.c:2173 */
	while ((p = get_next_name(p, name, rpub->label)) != NULL) { /* 2175 */
		if (strcmp(name, "SYSTEM") == 0)               /* 2177 */
			endpoint= SYSTEM;                          /* 2178 */
		else if (strcmp(name, "USER") == 0)            /* 2179 */
			endpoint= INIT_PROC_NR; /* all user procs */ /* 2180 */
		else
		{
			for (rrp=BEG_RPROC_ADDR; rrp<END_RPROC_ADDR; rrp++) { /* 2189 */
				if (!(rrp->r_flags & RS_IN_USE))       /* 2190 */
					continue;
				if (!strcmp(rrp->r_pub->proc_name, name)) { /* 2193 */
					priv_id= rrp->r_priv.s_id;         /* 2200 */
					set_sys_bit(privp->s_ipc_to, priv_id); /* 2201 */
				}
			}
			continue;
		}
		if ((r = sys_getpriv(&priv, endpoint)) < 0)    /* 2209 */
		{
			printf("add_forward_ipc: unable to get priv_id ..."); /* 2211-2213 */
			continue;
		}
		priv_id= priv.s_id;                            /* 2221 */
		set_sys_bit(privp->s_ipc_to, priv_id);         /* 2222 */
	}
}
```

语义要点：

1. **两个伪名例外**：`"SYSTEM"` → 内核 SYSTEM 端点（所有内核任务）；`"USER"` → `INIT_PROC_NR`（= 11，所有用户进程共享的 priv）。两者的 priv id 不是静态的，需要 `sys_getpriv` 查询（manager.c:2208-2215）——T5 后由 shell 以 `priv_id_of` 闭包注入（§3.1）。
2. **普通名按 `proc_name` 匹配**：遍历 in-use 槽，`proc_name` 相等的**全部**匹配（可能有多个副本），每个都置位（manager.c:2181-2205）。
3. **未匹配容忍**：注释明确（manager.c:2183-2188）"It is perfectly fine if this loop does not find any matches, as the target process(es) may not have been started yet. See add_backward_ipc() below."——缺位由 backward 补。
4. `sys_getpriv` 失败（目标端点不存在）→ 诊断 + 继续，不报错。

### 2.5 `add_backward_ipc`（manager.c:2230-2294）——反向补位

```c
void add_backward_ipc(rp, privp)                       /* manager.c:2230 */
{
	proc_name = rp->r_pub->proc_name;                  /* manager.c:2246 */
	for (rrp=BEG_RPROC_ADDR; rrp<END_RPROC_ADDR; rrp++) { /* 2248 */
		if (!(rrp->r_flags & RS_IN_USE))               /* 2249 */
			continue;
		if (!rrp->r_ipc_list[0])                       /* 2252 */
			continue;
		rrpub = rrp->r_pub;
		is_ipc_all = !strcmp(rrp->r_ipc_list, RSS_IPC_ALL);     /* 2261 */
		is_ipc_all_sys = !strcmp(rrp->r_ipc_list, RSS_IPC_ALL_SYS); /* 2262 */
		if (is_ipc_all ||
			(is_ipc_all_sys && (privp->s_flags & SYS_PROC))) { /* 2264-2265 */
			priv_id= rrp->r_priv.s_id;                 /* 2270 */
			set_sys_bit(privp->s_ipc_to, priv_id);     /* 2271 */
			continue;
		}
		p = rrp->r_ipc_list;                           /* 2280 */
		while ((p = get_next_name(p, name, rrpub->label)) != NULL) { /* 2282-2284 */
			if (!strcmp(proc_name, name)) {            /* 2283 */
				priv_id= rrp->r_priv.s_id;             /* 2289 */
				set_sys_bit(privp->s_ipc_to, priv_id); /* 2290 */
			}
		}
	}
}
```

语义要点：

1. **遍历其他所有服务的列表**，找"谁把我列进了它的 IPC 列表"——补 forward 阶段因目标未启动而缺的位。
2. **空列表跳过**（`!rrp->r_ipc_list[0]`，manager.c:2252）。
3. **`IPC_ALL`/`IPC_ALL_SYS` 特判**：别的服务声明 `IPC_ALL`（或 `IPC_ALL_SYS` 且本服务是系统进程）→ 无条件置位（manager.c:2261-2270）。注意 `privp->s_flags & SYS_PROC` 判的是**目标（本服务）**是不是系统进程。
4. **列表扫描**：普通列表则逐个名字与 `proc_name` 比较（manager.c:2276-2292），匹配即置位——一个服务可能被多个其他服务的列表命中。

### 2.6 `init_privs`（manager.c:2300-2331）+ `RSS_IPC_*`（rs.h:29-30）

```c
void init_privs(rp, privp)                             /* manager.c:2300 */
{
	fill_send_mask(&privp->s_ipc_to, FALSE);           /* manager.c:2308 清零 */
	is_ipc_all = !strcmp(rp->r_ipc_list, RSS_IPC_ALL); /* 2310 */
	is_ipc_all_sys = !strcmp(rp->r_ipc_list, RSS_IPC_ALL_SYS); /* 2311 */
	if (!is_ipc_all && !is_ipc_all_sys)
	{
		add_forward_ipc(rp, privp);                    /* manager.c:2319 */
		add_backward_ipc(rp, privp);                   /* manager.c:2320 */
	}
	else
	{
		for (i= 0; i<NR_SYS_PROCS; i++)                /* manager.c:2325 */
		{
			if (is_ipc_all || i != USER_PRIV_ID)       /* manager.c:2327 */
				set_sys_bit(privp->s_ipc_to, i);       /* manager.c:2328 */
		}
	}
}
```

`RSS_IPC_ALL`/`RSS_IPC_ALL_SYS` 定义在 `rs.h:29-30`（字符串 `"IPC_ALL"`/`"IPC_ALL_SYS"`，来自 `rs_start` 协议，08 展开）。

三元语义收束：

- `IPC_ALL` → `is_ipc_all` 真 → 全部 64 位置位（含 `USER_PRIV_ID`——普通用户进程的共享 priv）。
- `IPC_ALL_SYS` → `is_ipc_all` 假但 `is_ipc_all_sys` 真 → 除 `USER_PRIV_ID` 外全置。语义：能发给所有系统服务，但不能发给普通用户进程。
- 普通列表 → forward ∪ backward。

`USER_PRIV_ID = static_priv_id(ROOT_USR_PROC_NR) = NR_TASKS + INIT_PROC_NR = 5 + 11 = 16`（`priv.h:18`，`com.h:56,72,78`）。Rust 侧常量 `USER_PRIV_ID`（ipc_mask.rs:28-31）由 `minix_types::NR_TASKS + Endpoint::INIT.slot()` 计算，与 `PrivId::static_priv_id(11)` 有测试断言相等（`test_constants`）。

### 2.7 调用链：`init_slot` → `edit_slot` → `init_privs`

```
RS_UP（request.c:15 do_up）
  → create_service（10）
    → init_slot（manager.c:1708）
      → edit_slot（manager.c:1794，init_slot 尾部调用）
        → sys_datacopy 拷入 r_ipc_list（manager.c:1476-1483）
        → init_privs(rp, &rp->r_priv)（manager.c:1700）   ← 本文档
do_edit（request.c:298）
  → edit_slot（request.c:348）→ init_privs（manager.c:1700）
```

grep 实证：`init_privs` 在全部 C 源码中**只有 manager.c:1700 一个调用点**（`rg -n 'init_privs' servers/rs/*.c`），即 `edit_slot`。boot 路径不走此函数（§2.2 的 ALL_M 快捷）。这意味着：**每个动态服务创建/编辑时都重算一次 IPC 掩码**，boot 服务则保持全置。

---

## 3. Rust 设计决策

### 3.1 模块与签名（D1）

`os/servers/rs/src/ipc_mask.rs`：

```rust
pub struct IpcListIterator<'a> { ... }                 // get_next_name（manager.c:2115）
pub fn add_forward_ipc(rp, table, priv_id_of) -> SysMap // manager.c:2157
pub fn add_backward_ipc(target, table) -> SysMap       // manager.c:2230
pub fn init_privs(rp, table, priv_id_of) -> SysMap     // manager.c:2300
pub fn update_ipc_mask(rp: &mut ServiceSlot, table, priv_id_of) // init_privs + 写回 ipc_to
pub const RSS_IPC_ALL: &str = "IPC_ALL";               // rs.h:29
pub const RSS_IPC_ALL_SYS: &str = "IPC_ALL_SYS";       // rs.h:30
pub const USER_PRIV_ID: i32 = ...;                     // priv.h:18
```

设计差异：

- **返回值替代写参数**：C 的 `privp` 是 `in/out` 参数（`set_sys_bit(privp->s_ipc_to, ...)`），Rust 返回新 `SysMap` 由调用方写回——纯函数可测，`update_ipc_mask` 提供组合捷径（对应 C 的"调完就提交"习惯）。
- **`priv_id_of` 解析器注入（T5，2026-08-16）**：仅 forward 的 SYSTEM/USER 例外需要 priv id
  （`sys_getpriv`，manager.c:2209）。shell 把查询能力以闭包注入
  （`priv_id_of: impl FnMut(Endpoint) -> Option<PrivId>`，`None` = C 的 `sys_getpriv` 失败跳过），
  `ipc_mask.rs` 不再 import `KernelApi`——syscall 面只出现在接线层（monitor 模式，todo §13）。
  backward 不需要解析器。
- **表迭代**：C 的 `for (rrp=BEG_RPROC_ADDR; rrp<END_RPROC_ADDR; rrp++)` 全表扫描 → `RProcTable::iter_in_use()`（process_table.rs，ARCH A-4 家族的只读迭代），只产出 `RS_IN_USE` 行，与 C 的 `if (!(r_flags & RS_IN_USE)) continue` 一致。

### 3.2 分词器：`IpcListIterator`（D2）

`get_next_name` 的 C 指针循环改为迭代器，三点差异：

1. **无 unsafe**：C 用 `char*` 就地改写 + NUL 终止，Rust 用 `&[u8]` 切片 + `Label` 值拷贝。
2. **边界一致**：NUL 与空白都终止单词（`b == 0 || b.is_ascii_whitespace()`），匹配 C 的 `q[0] != '\0' && !isspace(...)`（manager.c:2133-2134）；列表以 NUL 结尾即结束（`None`），匹配 C 的 for 条件（manager.c:2125）。
3. **超长条目递归跳过**：`word.len() > RS_MAX_LABEL_LEN` → `self.next()`（ipc_mask.rs:85-87），等价 C 的 `continue`（manager.c:2143）。no_std 下丢弃 printf 诊断（A-11 调试面家族）。

### 3.3 forward/backward：借用表 + `SysMap::set`（D3）

- forward 的伪名分支：`priv_id_of(ep)` → `map.set(id.0 as usize)`（ipc_mask.rs:104-115）；C 的
  `sys_getpriv` 失败容忍（manager.c:2208-2215）→ 解析器返回 `None` 静默跳过。shell 侧的映射是
  `|ep| sys.getpriv(ep).ok().map(|p| p.id)`（19 接线，惰性——仅列表出现 SYSTEM/USER 时才查询，
  与 C 的按需 `sys_getpriv` 一致）。
- 普通名分支：`iter_in_use()` + `proc_name == name`（`Label` 的 `PartialEq<&str>`）→ `map.set(rrp.priv_.id.0 as usize)`（ipc_mask.rs:117-123）。
- backward 的 `is_ipc_all_sys && privp->s_flags & SYS_PROC`（manager.c:2264-2265）→ `target.priv_.is_sys_proc()`（ipc_mask.rs:144）——`PrivFlags::SYS_PROC` 判定（03）。

### 3.4 空列表与 `ipc_list_eq`（D4）

C 用 `!rrp->r_ipc_list[0]`（manager.c:2252）与 `strcmp(list, RSS_IPC_ALL)`（manager.c:2261-2262）。Rust 的 `ipc_list` 是 `[u8; MAX_IPC_LIST]`（256 字节，NUL 填充），因此：

- `ipc_list_eq(list, s)`：取 NUL 前缀与字符串比较（ipc_mask.rs:32-35），等价 `strcmp`。
- 空列表判定 `rrp.ipc_list[0] == 0` 原样保留（ipc_mask.rs:138-139）。

### 3.5 与 `Privilege::ipc_to` 的整合（D5）

`init_privs` 产出 `SysMap`，消费方：

- `update_ipc_mask`（ipc_mask.rs:191-193）写回 `rp.priv_.ipc_to`——对应 `edit_slot` 的 `init_privs(rp, &rp->r_priv)`（manager.c:1700）。
- 随后由 03 的 privctl 流程（`SYS_PRIV_SET_SYS` 创建时 / `SYS_PRIV_UPDATE_SYS` 编辑时）提交内核。
- boot 路径不经过 `init_privs`：`boot_priv` 直接 `SysMap::all()`（privilege.rs:393，§2.2）。

---

## 4. 实现详解（ipc_mask.rs）

模块结构：

```
ipc_mask.rs
├─ RSS_IPC_ALL / RSS_IPC_ALL_SYS（rs.h:29-30）
├─ USER_PRIV_ID（priv.h:18 计算）
├─ ipc_list_eq（strcmp 等价）
├─ IpcListIterator（get_next_name，manager.c:2115-2152）
│   └─ next(): 跳空白 → NUL 判定 → 词边界（空白|NUL）→ 超长跳过 → Label
├─ add_forward_ipc（manager.c:2157-2224）
│   ├─ SYSTEM/USER 伪名 → priv_id_of(ep) → set（manager.c:2177-2180, 2208-2222）
│   └─ proc_name 匹配 → set（manager.c:2181-2205）
├─ add_backward_ipc（manager.c:2230-2294）
│   ├─ 空列表跳过（2252）
│   ├─ IPC_ALL / (IPC_ALL_SYS && target SYS_PROC) → set（2261-2270）
│   └─ 列表扫描 proc_name → set（2277-2289）
├─ init_privs（manager.c:2300-2331）
│   ├─ IPC_ALL → all() / IPC_ALL_SYS → all() 除 USER_PRIV_ID（2322-2326）
│   └─ 普通 → forward | backward（2319-2320）
├─ update_ipc_mask（写回 rp.priv_.ipc_to）
└─ #[cfg(test)] 11 个测试（§5）
```

关键不变量：

1. **位图语义 = priv id**：`set(priv_id)` 的位号就是目标服务的 `s_id`（C 的 `priv_id= rrp->r_priv.s_id`，manager.c:2200-2201）——位图不存 endpoint，存 priv id。priv id 的分配与同步在 03。
2. **容忍失败**：forward 的 `getpriv` 失败（伪名）与"名字未匹配"都不报错——C 是诊断+继续，Rust 静默跳过（no_std 无诊断面）；backward 在目标启动后补位。
3. **对称性依赖内核**：位图计算假设内核强制双向检查（manager.c:2236 注释），Rust 不复制该强制，只保证位图构造。
4. **boot 不走此路径**：boot 全置由 `boot_priv` 承担，本文档的 `init_privs` 只服务动态路径——两条路径的边界在 §2.7 明确。

---

## 5. 测试要点

`cargo test -p minix-rs --lib ipc_mask` 中 ipc_mask 相关测试（ipc_mask.rs `#[cfg(test)]`，11 项，11/11 已落地；全局测试数是并行模块增长快照，非承诺）：

| 测试 | 覆盖 |
|------|------|
| `test_get_next_name_basic` | 空白/多词/尾部空白 + NUL 终止 |
| `test_get_next_name_empty_and_whitespace` | 空列表/纯空白列表 → 0 词 |
| `test_get_next_name_oversized_skipped` | 超长条目跳过、后续词保留 |
| `test_init_privs_ipc_all` | `IPC_ALL` 含 `USER_PRIV_ID`（manager.c:2327） |
| `test_init_privs_ipc_all_sys` | `IPC_ALL_SYS` 不含 `USER_PRIV_ID` |
| `test_init_privs_list_forward_backward` | 普通列表 forward 置位 |
| `test_forward_system_user` | `SYSTEM`/`USER` 伪名经注入的 `priv_id_of` 解析器置位（SYSTEM→4，USER→`USER_PRIV_ID`） |
| `test_forward_unmatched_name_tolerated` | 未匹配名字 → 空位图（容忍） |
| `test_backward_ipc_all_others` | 其他服务 `IPC_ALL` → backward 补位 |
| `test_update_ipc_mask_writes_back` | `update_ipc_mask` 写回 `priv_.ipc_to` |
| `test_constants` | `RSS_IPC_*` 字符串 + `USER_PRIV_ID == static_priv_id(11)` |

**T5 后测试不再需要 mock**：`priv_id_of` 由测试闭包直接提供（`mock_priv_id`），`table()` 构造含
tty（priv id 10）/vm（priv id 13）两槽的表，纯数据驱动，不依赖 `KernelApi` 实现。

---

## 6. 过渡：从"掩码机制"到"运行时心脏"

本文档完成 boot Step 1 的机制面三件套的最后一件：03（权限结构 + privctl）→ 04（谁有资格命令 RS）→ 05（服务能向谁发消息）。三者在 boot Step 1 的执行顺序（main.c:244-346，priv 位图段 262-280）与请求入口（04）中交汇：

- boot 服务：`boot_priv` 全置掩码（03 §4.2），不经本文档的 `init_privs`；
- 动态服务：创建/编辑时经 `init_slot`→`edit_slot`→`init_privs`（§2.7）精确计算。

下一篇 `06-rs-main-loop.md` 进入**运行时**：boot 完成后 RS 进入主循环（main.c:57-130），分类 `RS_*` 消息、管理 reply/late-reply。04 的访问控制是主循环每个请求 handler 的第一行，05 的掩码则早已在服务创建时固化进内核——运行时不再重算。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/01-rs-boot-init.md` — boot Step 1 时序
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md` — `ipc_list` 字段、`RProcTable`/`iter_in_use`
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/03-rs-privilege.md` — `Privilege::ipc_to`/`SysMap`/`fill_send_mask` 定义、boot 全置路径、privctl 提交
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/04-rs-access-control.md` — 并行的隔离面（谁控制谁）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/08-rs-slot-config.md` — `r_ipc_list` 拷贝（edit_slot，manager.c:1476-1483）与 `init_privs` 调用点（manager.c:1700）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/13-rs-control-requests.md` — `do_edit` 重新计算掩码（request.c:348）
- `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/23-ipc-filter.md` — 内核 send mask 强制与对称性检查
- `minix3/minix/servers/rs/manager.c:2112-2331`、`utility.c:82-95`、`main.c:272-273`、`include/minix/rs.h:29-30`、`include/minix/priv.h:18,67-69`、`servers/rs/type.h:105` — ground truth
- `os/servers/rs/src/ipc_mask.rs`、`os/servers/rs/src/process_table.rs` — Rust 实现
