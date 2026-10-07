# DS (Data Store) Server - store.c 逐行讲解

## 文件概述

**文件路径**: `minix/servers/ds/store.c`

**核心功能**: 实现DS服务器的核心数据存储和订阅管理逻辑。包括数据槽分配、订阅槽分配、数据发布/检索/删除、订阅匹配和通知机制。

**设计思路**: DS作为Minix3的"持久化存储服务"，采用静态数组管理数据槽和订阅槽，避免动态内存分配的复杂性。通过正则表达式实现灵活的订阅匹配，支持发布/订阅模式。

---

## 头文件包含

```c
#include "inc.h"
#include "store.h"
```

**逐行讲解**:
- `#include "inc.h"`：包含主头文件，引入系统头文件和函数原型
- `#include "store.h"`：包含数据存储类型定义
  - `struct data_store`：数据存储条目结构
  - `struct subscription`：订阅结构
  - `NR_DS_KEYS`和`NR_DS_SUBS`：槽位数量定义

---

## 静态存储分配

```c
/* Allocate space for the data store. */
static struct data_store ds_store[NR_DS_KEYS];
static struct subscription ds_subs[NR_DS_SUBS];
```

**逐行讲解**:
- `static struct data_store ds_store[NR_DS_KEYS];`：静态数据存储数组
  - `static`：限制作用域在当前文件
  - `NR_DS_KEYS`：定义为`2*NR_SYS_PROCS`（系统进程数的2倍）
  - 存储位置：数据段（.bss），程序启动时清零
  - 每个元素大小：约`DS_MAX_KEYLEN*2 + 4 + union大小`字节
- `static struct subscription ds_subs[NR_DS_SUBS];`：静态订阅数组
  - `NR_DS_SUBS`：定义为`4*NR_SYS_PROCS`（系统进程数的4倍）

**内存布局示意图**:
```
数据段 (.bss):
┌────────────────────────────────────────────────────┐
│ ds_store[0]  │ flags │ key[40] │ owner[40] │ u    │
├────────────────────────────────────────────────────┤
│ ds_store[1]  │ ...                              │
├────────────────────────────────────────────────────┤
│ ...                                               │
├────────────────────────────────────────────────────┤
│ ds_store[NR_DS_KEYS-1]                            │
└────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────┐
│ ds_subs[0]   │ flags │ owner[40] │ regex │ bitmap │
├────────────────────────────────────────────────────┤
│ ds_subs[1]   │ ...                              │
├────────────────────────────────────────────────────┤
│ ...                                               │
├────────────────────────────────────────────────────┤
│ ds_subs[NR_DS_SUBS-1]                             │
└────────────────────────────────────────────────────┘
```

**设计原因**: 使用静态数组而非动态分配：
1. 避免内存分配失败的风险
2. 简化内存管理，无需释放
3. 固定上限便于系统规划资源
4. 符合内核/服务器代码的确定性要求

---

## alloc_data_slot函数

```c
/*===========================================================================*
 *			      alloc_data_slot				     *
 *===========================================================================*/
static struct data_store *alloc_data_slot(void)
{
/* Allocate a new data slot. */
  int i;

  for (i = 0; i < NR_DS_KEYS; i++) {
	if (!(ds_store[i].flags & DSF_IN_USE))
		return &ds_store[i];
  }

  return NULL;
}
```

**逐行讲解**:
- `static struct data_store *alloc_data_slot(void)`：分配数据槽函数
  - 返回值：指向空闲槽的指针，或NULL（无空闲槽）
- `int i;`：循环计数器，栈上分配，4字节
- `for (i = 0; i < NR_DS_KEYS; i++)`：遍历所有数据槽
- `if (!(ds_store[i].flags & DSF_IN_USE))`：检查槽是否空闲
  - `DSF_IN_USE`：标志位，表示槽正在使用
  - `&`：位与运算，检查该位是否设置
- `return &ds_store[i];`：返回空闲槽的地址
- `return NULL;`：所有槽都在使用，返回空指针

**标志位操作示意**:
```
flags 字段 (int, 32位):
┌───┬───┬───┬───┬───┬───┬───┬───┐
│...│...│...│...│...│...│...│IN_USE│
└───┴───┴───┴───┴───┴───┴───┴───┘
  7   6   5   4   3   2   1   0

检查: flags & DSF_IN_USE
- 结果为0: 槽空闲
- 结果非0: 槽占用
```

**时间复杂度**: O(N)，N为槽位数量。简单但足够，因为NR_DS_KEYS通常不大。

---

## alloc_sub_slot函数

```c
/*===========================================================================*
 *				alloc_sub_slot				     *
 *===========================================================================*/
static struct subscription *alloc_sub_slot(void)
{
/* Return a free subscription slot. */
  int i;

  for (i = 0; i < NR_DS_SUBS; i++) {
	if (!(ds_subs[i].flags & DSF_IN_USE))
		return &ds_subs[i];
  }

  return NULL;
}
```

**逐行讲解**:
- 与`alloc_data_slot`逻辑相同
- 遍历订阅数组，返回第一个空闲槽
- 返回NULL表示无空闲槽

---

## free_sub_slot函数

```c
/*===========================================================================*
 *				free_sub_slot				     *
 *===========================================================================*/
static void free_sub_slot(struct subscription *subp)
{
/* Clean up a previously successfully) allocated subscription slot. */
  assert(subp->flags & DSF_IN_USE);

  regfree(&subp->regex);
  memset(&subp->regex, 0, sizeof(subp->regex));

  subp->flags = 0;
}
```

**逐行讲解**:
- `static void free_sub_slot(struct subscription *subp)`：释放订阅槽
  - 参数：指向订阅结构的指针
- `assert(subp->flags & DSF_IN_USE);`：断言槽正在使用
  - `assert`：调试宏，条件为假时触发断言失败
  - 确保不会重复释放或释放未分配的槽
- `regfree(&subp->regex);`：释放正则表达式资源
  - `regex`字段存储编译后的正则表达式
  - `regfree`：POSIX正则表达式库函数，释放内部资源
- `memset(&subp->regex, 0, sizeof(subp->regex));`：清零正则表达式结构
  - `memset`：内存设置函数，将指定字节设置为0
  - 防止悬空指针
- `subp->flags = 0;`：清零标志位，标记槽为空闲

**资源清理流程**:
```
释放前:
subp->flags = DSF_IN_USE | DSF_TYPE_U32
subp->regex = { 编译后的正则表达式数据 }
subp->owner = "some_process"

释放后:
subp->flags = 0
subp->regex = { 全零 }
subp->owner = "some_process" (保留，但不影响)
```

**设计原因**: 订阅包含动态分配的正则表达式资源，必须显式释放。忘记调用`regfree`会导致内存泄漏。

---

## lookup_entry函数

```c
/*===========================================================================*
 *				lookup_entry				     *
 *===========================================================================*/
static struct data_store *lookup_entry(const char *key_name, int type)
{
/* Lookup an existing entry by key and type. */
  int i;

  for (i = 0; i < NR_DS_KEYS; i++) {
	if ((ds_store[i].flags & DSF_IN_USE) /* used */
		&& (ds_store[i].flags & type) /* same type*/
		&& !strcmp(ds_store[i].key, key_name)) /* same key*/
		return &ds_store[i];
  }

  return NULL;
}
```

**逐行讲解**:
- `static struct data_store *lookup_entry(const char *key_name, int type)`：查找数据条目
  - 参数`key_name`：键名字符串指针
  - 参数`type`：类型标志（如`DSF_TYPE_U32`）
  - 返回值：匹配条目的指针，或NULL
- `for (i = 0; i < NR_DS_KEYS; i++)`：遍历所有数据槽
- 三个条件必须同时满足：
  1. `ds_store[i].flags & DSF_IN_USE`：槽正在使用
  2. `ds_store[i].flags & type`：类型匹配
  3. `!strcmp(ds_store[i].key, key_name)`：键名匹配
- `strcmp`：字符串比较函数，返回0表示相等

**查找逻辑示意**:
```
查找 key="vm.addr", type=DSF_TYPE_U32

ds_store[0]: flags=0 → 跳过（未使用）
ds_store[1]: flags=DSF_IN_USE|DSF_TYPE_LABEL, key="pm" → 跳过（类型不匹配）
ds_store[2]: flags=DSF_IN_USE|DSF_TYPE_U32, key="vm.addr" → 匹配！返回此槽
```

---

## lookup_label_entry函数

```c
/*===========================================================================*
 *			     lookup_label_entry				     *
 *===========================================================================*/
static struct data_store *lookup_label_entry(unsigned num)
{
/* Lookup an existing label entry by num. */
  int i;

  for (i = 0; i < NR_DS_KEYS; i++) {
	if ((ds_store[i].flags & DSF_IN_USE)
		&& (ds_store[i].flags & DSF_TYPE_LABEL)
		&& (ds_store[i].u.u32 == num))
		return &ds_store[i];
  }

  return NULL;
}
```

**逐行讲解**:
- `static struct data_store *lookup_label_entry(unsigned num)`：按编号查找标签条目
  - 参数`num`：标签编号（通常是进程端点）
  - 返回值：匹配条目的指针，或NULL
- 与`lookup_entry`类似，但匹配条件不同：
  1. 槽正在使用
  2. 类型为标签（`DSF_TYPE_LABEL`）
  3. 存储的值（`u.u32`）等于`num`

**标签的特殊性**:
```
标签条目:
key = "vm"           (进程名)
u.u32 = VM_PROC_NR   (进程端点)
flags = DSF_IN_USE | DSF_TYPE_LABEL

用途: 通过端点查找进程名
```

---

## lookup_sub函数

```c
/*===========================================================================*
 *			      lookup_sub				     *
 *===========================================================================*/
static struct subscription *lookup_sub(const char *owner)
{
/* Lookup an existing subscription given its owner. */
  int i;

  for (i = 0; i < NR_DS_SUBS; i++) {
	if ((ds_subs[i].flags & DSF_IN_USE) /* used */
		&& !strcmp(ds_subs[i].owner, owner)) /* same key*/
		return &ds_subs[i];
  }

  return NULL;
}
```

**逐行讲解**:
- `static struct subscription *lookup_sub(const char *owner)`：查找订阅
  - 参数`owner`：订阅者名称
  - 返回值：匹配订阅的指针，或NULL
- 每个进程最多只能有一个订阅
- 通过所有者名称查找

---

## ds_getprocname函数

```c
/*===========================================================================*
 *				ds_getprocname				     *
 *===========================================================================*/
static char *ds_getprocname(endpoint_t e)
{
/* Get a process name given its endpoint. */
	struct data_store *dsp;

	static char *first_proc_name = "ds";
	endpoint_t first_proc_ep = DS_PROC_NR;

	if(e == first_proc_ep)
		return first_proc_name;

	if((dsp = lookup_label_entry(e)) != NULL)
		return dsp->key;

	return NULL;
}
```

**逐行讲解**:
- `static char *ds_getprocname(endpoint_t e)`：根据端点获取进程名
  - 参数`e`：进程端点
  - 返回值：进程名字符串指针，或NULL
- `static char *first_proc_name = "ds";`：静态局部变量
  - 存储位置：数据段
  - DS服务自身的名称
- `endpoint_t first_proc_ep = DS_PROC_NR;`：DS服务的端点号
  - 栈上分配
- `if(e == first_proc_ep)`：检查是否是DS自身
  - DS的标签在初始化时可能还未发布
  - 特殊处理DS自身
- `if((dsp = lookup_label_entry(e)) != NULL)`：查找标签条目
  - 返回标签的键名（进程名）
- `return NULL;`：未找到，返回NULL

**端点到名称的映射**:
```
端点 → 名称
DS_PROC_NR → "ds" (特殊处理)
VM_PROC_NR → "vm" (通过标签查找)
PM_PROC_NR → "pm" (通过标签查找)
...
```

---

## ds_getprocep函数

```c
/*===========================================================================*
 *				ds_getprocep				     *
 *===========================================================================*/
static endpoint_t ds_getprocep(const char *s)
{
/* Get a process endpoint given its name. */
	struct data_store *dsp;

	if((dsp = lookup_entry(s, DSF_TYPE_LABEL)) != NULL)
		return dsp->u.u32;
	panic("ds_getprocep: process endpoint not found");
}
```

**逐行讲解**:
- `static endpoint_t ds_getprocep(const char *s)`：根据名称获取端点
  - 参数`s`：进程名称
  - 返回值：进程端点
- `lookup_entry(s, DSF_TYPE_LABEL)`：查找标签条目
- `return dsp->u.u32;`：返回存储的端点值
- `panic(...)`：如果找不到，触发panic
  - 这是致命错误，说明系统状态不一致

**双向映射**:
```
ds_getprocname: 端点 → 名称
ds_getprocep:   名称 → 端点
```

---

## check_auth函数

```c
/*===========================================================================*
 *				 check_auth				     *
 *===========================================================================*/
static int check_auth(const struct data_store *p, endpoint_t ep, int perm)
{
/* Check authorization for a given type of permission. */
	char *source;

	if(!(p->flags & perm))
		return 1;

	source = ds_getprocname(ep);
	return source && !strcmp(p->owner, source);
}
```

**逐行讲解**:
- `static int check_auth(const struct data_store *p, endpoint_t ep, int perm)`：权限检查
  - 参数`p`：数据条目指针
  - 参数`ep`：调用者端点
  - 参数`perm`：权限标志（如`DSF_PRIV_OVERWRITE`）
  - 返回值：1表示有权限，0表示无权限
- `if(!(p->flags & perm))`：检查是否设置了权限限制
  - 如果没有设置该权限标志，表示无限制，返回1
- `source = ds_getprocname(ep);`：获取调用者名称
- `return source && !strcmp(p->owner, source);`：比较所有者
  - `source`非NULL且与所有者匹配，返回1

**权限模型**:
```
数据条目权限:
┌────────────────────────────────────┐
│ DSF_PRIV_OVERWRITE  │ 覆盖权限     │
│ DSF_PRIV_RETRIEVE   │ 检索权限     │
│ DSF_PRIV_SUBSCRIBE  │ 订阅权限     │
└────────────────────────────────────┘

如果设置了权限标志，只有所有者才能执行该操作
如果未设置权限标志，任何人都可以执行
```

---

## get_key_name函数

```c
/*===========================================================================*
 *				get_key_name				     *
 *===========================================================================*/
static int get_key_name(const message *m_ptr, char *key_name)
{
/* Get key name given an input message. */
  int r;

  if (m_ptr->m_ds_req.key_len > DS_MAX_KEYLEN || m_ptr->m_ds_req.key_len < 2) {
	printf("DS: bogus key length (%d) from %d\n", m_ptr->m_ds_req.key_len,
		m_ptr->m_source);
	return EINVAL;
  }
```

**逐行讲解**:
- `static int get_key_name(const message *m_ptr, char *key_name)`：从消息中获取键名
  - 参数`m_ptr`：消息指针
  - 参数`key_name`：输出缓冲区
  - 返回值：OK或错误码
- `m_ptr->m_ds_req.key_len`：消息中的键名长度
- `DS_MAX_KEYLEN`：最大键名长度（通常40字节）
- 检查长度是否有效：
  - 不能超过最大长度
  - 不能小于2（至少一个字符+空终止符）
- `return EINVAL;`：无效参数错误

---

```c
  /* Copy name from caller. */
  r = sys_safecopyfrom(m_ptr->m_source,
	(cp_grant_id_t) m_ptr->m_ds_req.key_grant, 0, 
	(vir_bytes) key_name, m_ptr->m_ds_req.key_len);
  if(r != OK) {
	printf("DS: publish: copy failed from %d: %d\n", m_ptr->m_source, r);
	return r;
  }

  key_name[DS_MAX_KEYLEN-1] = '\0';

  return OK;
}
```

**逐行讲解**:
- `sys_safecopyfrom`：安全内存复制函数
  - 从调用者进程的地址空间复制数据到DS的地址空间
  - 参数：源进程、授权ID、偏移量、目标地址、长度
  - 这是Minix3的安全机制，防止进程直接访问其他进程的内存
- `m_ptr->m_ds_req.key_grant`：授权ID
  - 调用者在发送消息前，先授予DS读取其内存的权限
- `key_name[DS_MAX_KEYLEN-1] = '\0';`：确保字符串终止
  - 防止缓冲区溢出攻击

**安全内存复制流程**:
```
调用者进程              DS服务
┌─────────────┐        ┌─────────────┐
│ key_name    │        │ key_name    │
│ "vm.addr"   │        │ (未初始化)  │
└─────────────┘        └─────────────┘
      │                      ↑
      │ 1. 授权DS读取        │
      │ 2. 发送消息          │
      │                      │
      └──── sys_safecopyfrom ─┘
           (内核执行复制)
```

---

## check_sub_match函数

```c
/*===========================================================================*
 *				check_sub_match				     *
 *===========================================================================*/
static int check_sub_match(const struct subscription *subp,
		struct data_store *dsp, endpoint_t ep)
{
/* Check if an entry matches a subscription. Return 1 in case of match. */
  return (check_auth(dsp, ep, DSF_PRIV_SUBSCRIBE)
	  && regexec(&subp->regex, dsp->key, 0, NULL, 0) == 0)
	  ? 1 : 0;
}
```

**逐行讲解**:
- `static int check_sub_match(...)`：检查数据条目是否匹配订阅
  - 参数`subp`：订阅指针
  - 参数`dsp`：数据条目指针
  - 参数`ep`：调用者端点
  - 返回值：1匹配，0不匹配
- `check_auth(dsp, ep, DSF_PRIV_SUBSCRIBE)`：检查订阅权限
- `regexec(&subp->regex, dsp->key, 0, NULL, 0)`：正则表达式匹配
  - `regexec`：POSIX正则表达式执行函数
  - 返回0表示匹配成功
- 三元运算符：两个条件都满足返回1，否则返回0

**正则表达式匹配示例**:
```
订阅正则: "^vm\..*$"
数据键名: "vm.addr" → 匹配
数据键名: "pm.pid"  → 不匹配
```

---

## update_subscribers函数

```c
/*===========================================================================*
 *			     update_subscribers				     *
 *===========================================================================*/
static void update_subscribers(struct data_store *dsp, int set)
{
/* If set = 1, set bit in the sub bitmap of any subscription matching the given
 * entry, otherwise clear it. In both cases, notify the subscriber.
 */
	int i;
	int nr = dsp - ds_store;
	endpoint_t ep;

	for(i = 0; i < NR_DS_SUBS; i++) {
		if(!(ds_subs[i].flags & DSF_IN_USE))
			continue;
		if(!(ds_subs[i].flags & dsp->flags & DSF_MASK_TYPE))
			continue;

		ep = ds_getprocep(ds_subs[i].owner);
		if(!check_sub_match(&ds_subs[i], dsp, ep))
			continue;

		if(set == 1) {
			SET_BIT(ds_subs[i].old_subs, nr);
		} else {
			UNSET_BIT(ds_subs[i].old_subs, nr);
		}
		ipc_notify(ep);
	}
}
```

**逐行讲解**:
- `static void update_subscribers(struct data_store *dsp, int set)`：更新订阅者
  - 参数`dsp`：发生变化的数据条目
  - 参数`set`：1表示设置位，0表示清除位
- `int nr = dsp - ds_store;`：计算条目的索引号
  - 指针减法：得到数组下标
- 遍历所有订阅：
  1. 跳过未使用的订阅槽
  2. 检查类型是否匹配
  3. 检查正则表达式是否匹配
- `SET_BIT(ds_subs[i].old_subs, nr);`：设置位图中的位
  - `old_subs`：位图，记录哪些条目发生了变化
- `ipc_notify(ep);`：发送异步通知
  - 通知订阅者有数据变化

**位图操作示意**:
```
old_subs 位图 (假设NR_DS_KEYS=64):
┌───┬───┬───┬───┬───┬───┬───┬───┐
│ 0 │ 0 │ 1 │ 0 │ 0 │ 0 │ 0 │ 0 │ ...
└───┴───┴───┴───┴───┴───┴───┴───┘
  0   1   2   3   4   5   6   7

位2被设置，表示ds_store[2]的数据发生了变化
订阅者收到通知后，调用do_check获取具体信息
```

---

## map_service函数

```c
/*===========================================================================*
 *		               map_service                                   *
 *===========================================================================*/
static int map_service(const struct rprocpub *rpub)
{
/* Map a new service by registering its label. */
  struct data_store *dsp;

  /* Allocate a new data slot. */
  if((dsp = alloc_data_slot()) == NULL) {
	return ENOMEM;
  }

  /* Set attributes. */
  strcpy(dsp->key, rpub->label);
  dsp->u.u32 = (u32_t) rpub->endpoint;
  strcpy(dsp->owner, "rs");
  dsp->flags = DSF_IN_USE | DSF_TYPE_LABEL;

  /* Update subscribers having a matching subscription. */
  update_subscribers(dsp, 1);

  return(OK);
}
```

**逐行讲解**:
- `static int map_service(const struct rprocpub *rpub)`：映射服务
  - 参数`rpub`：进程信息结构
  - 返回值：OK或错误码
- `alloc_data_slot()`：分配数据槽
- `strcpy(dsp->key, rpub->label)`：复制服务名称
- `dsp->u.u32 = (u32_t) rpub->endpoint`：存储端点
- `strcpy(dsp->owner, "rs")`：设置所有者为RS
- `dsp->flags = DSF_IN_USE | DSF_TYPE_LABEL`：设置标志
- `update_subscribers(dsp, 1)`：通知订阅者

**服务映射流程**:
```
RS启动新服务 → 调用map_service → DS存储标签
                                    ↓
其他服务可以通过标签查找新服务的端点
```

---

## sef_cb_init_fresh函数

```c
/*===========================================================================*
 *		            sef_cb_init_fresh                                *
 *===========================================================================*/
int sef_cb_init_fresh(int UNUSED(type), sef_init_info_t *info)
{
/* Initialize the data store server. */
	int i, r;
	struct rprocpub rprocpub[NR_BOOT_PROCS];

	/* Reset data store: data and subscriptions. */
	for(i = 0; i < NR_DS_KEYS; i++) {
		ds_store[i].flags = 0;
	}
	for(i = 0; i < NR_DS_SUBS; i++) {
		ds_subs[i].flags = 0;
	}
```

**逐行讲解**:
- `int sef_cb_init_fresh(int UNUSED(type), sef_init_info_t *info)`：首次启动回调
  - 参数`type`：标记为未使用
  - 参数`info`：初始化信息结构
- `struct rprocpub rprocpub[NR_BOOT_PROCS];`：启动进程信息数组
  - 栈上分配
- 清零所有数据槽和订阅槽的标志位

---

```c
	/* Map all the services in the boot image. */
	if((r = sys_safecopyfrom(RS_PROC_NR, info->rproctab_gid, 0,
		(vir_bytes) rprocpub, sizeof(rprocpub))) != OK) {
		panic("sys_safecopyfrom failed: %d", r);
	}
	for(i=0;i < NR_BOOT_PROCS;i++) {
		if(rprocpub[i].in_use) {
			if((r = map_service(&rprocpub[i])) != OK) {
				panic("unable to map service: %d", r);
			}
		}
	}

	return(OK);
}
```

**逐行讲解**:
- `sys_safecopyfrom(RS_PROC_NR, ...)`：从RS复制启动进程表
  - RS在启动时准备了所有启动进程的信息
- 遍历启动进程表，为每个进程创建标签
- `map_service(&rprocpub[i])`：映射服务

**初始化流程**:
```
系统启动
    ↓
RS准备启动进程表
    ↓
DS启动，调用sef_cb_init_fresh
    ↓
从RS复制进程表
    ↓
为每个进程创建标签条目
    ↓
其他服务可以通过DS查找进程端点
```

---

## do_publish函数

```c
/*===========================================================================*
 *				do_publish				     *
 *===========================================================================*/
int do_publish(message *m_ptr)
{
  struct data_store *dsp;
  char key_name[DS_MAX_KEYLEN];
  char *source;
  int flags = m_ptr->m_ds_req.flags;
  size_t length;
  int r;
```

**逐行讲解**:
- `int do_publish(message *m_ptr)`：处理发布请求
  - 参数：消息指针
  - 返回值：OK或错误码
- 局部变量：
  - `dsp`：数据条目指针（4字节，栈上）
  - `key_name[DS_MAX_KEYLEN]`：键名缓冲区（40字节，栈上）
  - `source`：调用者名称指针（4字节，栈上）
  - `flags`：标志位（4字节，栈上）
  - `length`：数据长度（4/8字节，栈上）
  - `r`：返回值（4字节，栈上）

**栈帧布局**:
```
栈:
┌────────────────────────────┐
│ r (4字节)                  │
│ length (4/8字节)           │
│ flags (4字节)              │
│ source (4字节)             │
│ key_name[40] (40字节)      │
│ dsp (4字节)                │
│ 返回地址                   │
└────────────────────────────┘
```

---

```c
  /* Lookup the source. */
  source = ds_getprocname(m_ptr->m_source);
  if(source == NULL)
	  return EPERM;

  /* Only RS can publish labels. */
  if((flags & DSF_TYPE_LABEL) && m_ptr->m_source != RS_PROC_NR)
	  return EPERM;
```

**逐行讲解**:
- 获取调用者名称，如果失败返回`EPERM`（权限错误）
- 标签类型只能由RS发布
  - 标签涉及进程端点映射，是敏感信息
  - 防止恶意进程伪造标签

---

```c
  /* Get key name. */
  if((r = get_key_name(m_ptr, key_name)) != OK)
	return r;

  /* Lookup the entry. */
  dsp = lookup_entry(key_name, flags & DSF_MASK_TYPE);
  /* If type is LABEL, also try to lookup the entry by num. */
  if((flags & DSF_TYPE_LABEL) && (dsp == NULL))
	dsp = lookup_label_entry(m_ptr->m_ds_req.val_in.ep);
```

**逐行讲解**:
- 获取键名
- 查找现有条目
- 对于标签类型，也尝试按端点查找

---

```c
  if(dsp == NULL) {
	/* The entry doesn't exist, allocate a new data slot. */
	if((dsp = alloc_data_slot()) == NULL)
		return ENOMEM;
  } else if (flags & DSF_OVERWRITE) {
	/* Overwrite. */
	if(!check_auth(dsp, m_ptr->m_source, DSF_PRIV_OVERWRITE))
		return EPERM;
  } else {
	/* Don't overwrite and return error. */
	return EEXIST;
  }
```

**逐行讲解**:
- 三种情况：
  1. 条目不存在：分配新槽
  2. 条目存在且允许覆盖：检查权限
  3. 条目存在且不允许覆盖：返回`EEXIST`（已存在错误）

---

```c
  /* Store! */
  switch(flags & DSF_MASK_TYPE) {
  case DSF_TYPE_U32:
	dsp->u.u32 = m_ptr->m_ds_req.val_in.u32;
	break;
  case DSF_TYPE_LABEL:
	dsp->u.u32 = m_ptr->m_ds_req.val_in.ep;
	break;
```

**逐行讲解**:
- 根据类型存储数据
- `DSF_TYPE_U32`：32位无符号整数
- `DSF_TYPE_LABEL`：端点值

---

```c
  case DSF_TYPE_STR:
  case DSF_TYPE_MEM:
	length = m_ptr->m_ds_req.val_len;
	/* Allocate a new data buffer if necessary. */
	if(!(dsp->flags & DSF_IN_USE)) {
		if((dsp->u.mem.data = malloc(length)) == NULL)
			return ENOMEM;
		dsp->u.mem.reallen = length;
	} else if(length > dsp->u.mem.reallen) {
		free(dsp->u.mem.data);
		if((dsp->u.mem.data = malloc(length)) == NULL)
			return ENOMEM;
		dsp->u.mem.reallen = length;
	}

	/* Copy the memory range. */
	r = sys_safecopyfrom(m_ptr->m_source, m_ptr->m_ds_req.val_in.grant,
	        0, (vir_bytes) dsp->u.mem.data, length);
	if(r != OK) {
		printf("DS: publish: memory map/copy failed from %d: %d\n",
			m_ptr->m_source, r);
		free(dsp->u.mem.data);
		return r;
	}
	dsp->u.mem.length = length;
	if(flags & DSF_TYPE_STR) {
		((char*)dsp->u.mem.data)[length-1] = '\0';
	}
	break;
  default:
	return EINVAL;
  }
```

**逐行讲解**:
- 字符串和内存类型需要动态分配缓冲区
- `malloc(length)`：分配内存
- 如果现有缓冲区不够大，重新分配
- `sys_safecopyfrom`：从调用者复制数据
- 字符串类型确保以空字符结尾

**内存管理示意**:
```
发布字符串 "hello world":

DS数据槽:
┌─────────────────────────────────┐
│ key = "my.string"               │
│ u.mem.data ─────────────────┐   │
│ u.mem.length = 12           │   │
│ u.mem.reallen = 12          │   │
└─────────────────────────────────┘
                               ↓
堆内存:
┌─────────────────────────────────┐
│ "hello world\0"                 │
└─────────────────────────────────┘
```

---

```c
  /* Set attributes. */
  strcpy(dsp->key, key_name);
  strcpy(dsp->owner, source);
  dsp->flags = DSF_IN_USE | (flags & DSF_MASK_INTERNAL);

  /* Update subscribers having a matching subscription. */
  update_subscribers(dsp, 1);

  return(OK);
}
```

**逐行讲解**:
- 设置键名和所有者
- 设置标志位
- 通知订阅者

---

## do_retrieve函数

```c
/*===========================================================================*
 *				do_retrieve				     *
 *===========================================================================*/
int do_retrieve(message *m_ptr)
{
  struct data_store *dsp;
  char key_name[DS_MAX_KEYLEN];
  int flags = m_ptr->m_ds_req.flags;
  int type = flags & DSF_MASK_TYPE;
  size_t length;
  int r;

  /* Get key name. */
  if((r = get_key_name(m_ptr, key_name)) != OK)
	return r;

  /* Lookup the entry. */
  if((dsp = lookup_entry(key_name, type)) == NULL)
	return ESRCH;
  if(!check_auth(dsp, m_ptr->m_source, DSF_PRIV_RETRIEVE))
	return EPERM;
```

**逐行讲解**:
- 获取键名
- 查找条目，未找到返回`ESRCH`（未找到错误）
- 检查检索权限

---

```c
  /* Copy the requested data. */
  switch(type) {
  case DSF_TYPE_U32:
	m_ptr->m_ds_reply.val_out.u32 = dsp->u.u32;
	break;
  case DSF_TYPE_LABEL:
	m_ptr->m_ds_reply.val_out.ep = dsp->u.u32;
	break;
  case DSF_TYPE_STR:
  case DSF_TYPE_MEM:
	length = MIN(m_ptr->m_ds_req.val_len, dsp->u.mem.length);
	r = sys_safecopyto(m_ptr->m_source, m_ptr->m_ds_req.val_in.grant, 0,
		(vir_bytes) dsp->u.mem.data, length);
	if(r != OK) {
		printf("DS: retrieve: copy failed to %d: %d\n",	
			m_ptr->m_source, r);
		return r;
	}
	m_ptr->m_ds_reply.val_len = length;
	break;
  default:
	return EINVAL;
  }

  return OK;
}
```

**逐行讲解**:
- 根据类型返回数据
- 整数和标签类型：直接放入消息
- 字符串和内存类型：使用`sys_safecopyto`复制到调用者缓冲区
- `MIN(...)`：取最小值，防止缓冲区溢出

---

## do_retrieve_label函数

```c
/*===========================================================================*
 *				do_retrieve_label			     *
 *===========================================================================*/
int do_retrieve_label(const message *m_ptr)
{
  struct data_store *dsp;
  int r;

  /* Lookup the label entry. */
  if((dsp = lookup_label_entry(m_ptr->m_ds_req.val_in.ep)) == NULL)
	return ESRCH;

  /* Copy the key name. */
  r = sys_safecopyto(m_ptr->m_source,
	(cp_grant_id_t) m_ptr->m_ds_req.key_grant, (vir_bytes) 0,
	(vir_bytes) dsp->key, strlen(dsp->key) + 1);
  if(r != OK) {
	printf("DS: copy failed from %d: %d\n", m_ptr->m_source, r);
	return r;
  }

  return OK;
}
```

**逐行讲解**:
- 根据端点查找标签
- 返回进程名称（键名）

---

## do_subscribe函数

```c
/*===========================================================================*
 *				do_subscribe				     *
 *===========================================================================*/
int do_subscribe(message *m_ptr)
{
  char regex[DS_MAX_KEYLEN+2];
  struct subscription *subp;
  char errbuf[80];
  char *owner;
  int type_set;
  int r, e, b;

  /* Find the owner. */
  owner = ds_getprocname(m_ptr->m_source);
  if(owner == NULL)
	  return ESRCH;

  /* See if the owner already has an existing subscription. */
  if ((subp = lookup_sub(owner)) != NULL) {
	/* If a subscription exists but we can't overwrite, return error. */
	if (!(m_ptr->m_ds_req.flags & DSF_OVERWRITE))
		return EEXIST;
	/* Otherwise just free the old one. */
	free_sub_slot(subp);
  }
```

**逐行讲解**:
- 获取调用者名称
- 检查是否已有订阅
- 如果已有订阅且不允许覆盖，返回错误
- 如果允许覆盖，释放旧订阅

---

```c
  /* Find a free subscription slot. */
  if ((subp = alloc_sub_slot()) == NULL)
	return EAGAIN;

  /* Copy key name from the caller. Anchor the subscription with "^regexp$" so
   * substrings don't match. The caller will probably not expect this,
   * and the usual case is for a complete match.
   */
  regex[0] = '^';
  if((r = get_key_name(m_ptr, regex+1)) != OK)
	return r;
  strcat(regex, "$");
```

**逐行讲解**:
- 分配订阅槽
- 构建正则表达式：`^pattern$`
  - `^`：匹配字符串开头
  - `$`：匹配字符串结尾
  - 确保完全匹配，而非子串匹配

**正则表达式锚定**:
```
用户输入: "vm\..*"
实际正则: "^vm\..*$"

匹配: "vm.addr" ✓
不匹配: "system.vm.addr" ✗ (因为^要求开头)
```

---

```c
  /* Compile regular expression. */
  if((e=regcomp(&subp->regex, regex, REG_EXTENDED)) != 0) {
	regerror(e, &subp->regex, errbuf, sizeof(errbuf));
	printf("DS: subscribe: regerror: %s\n", errbuf);
	memset(&subp->regex, 0, sizeof(subp->regex));
	return EINVAL;
  }

  /* If type_set = 0, then subscribe all types. */
  type_set = m_ptr->m_ds_req.flags & DSF_MASK_TYPE;
  if(type_set == 0)
	  type_set = DSF_MASK_TYPE;

  subp->flags = DSF_IN_USE | type_set;
  strcpy(subp->owner, owner);
  for(b = 0; b < BITMAP_CHUNKS(NR_DS_KEYS); b++)
	subp->old_subs[b] = 0;
```

**逐行讲解**:
- `regcomp`：编译正则表达式
  - `REG_EXTENDED`：使用扩展正则语法
- 如果编译失败，返回`EINVAL`
- 设置订阅类型，如果为0则订阅所有类型
- 初始化位图为全0

---

```c
  /* See if caller requested an instant initial list. */
  if(m_ptr->m_ds_req.flags & DSF_INITIAL) {
	int i, match_found = FALSE;
	for(i = 0; i < NR_DS_KEYS; i++) {
		if(!(ds_store[i].flags & DSF_IN_USE))
			continue;
		if(!(ds_store[i].flags & type_set))
			continue;
		if(!check_sub_match(subp, &ds_store[i], m_ptr->m_source))
			continue;

		SET_BIT(subp->old_subs, i);
		match_found = TRUE;
	}

	/* Notify in case of match. */
	if(match_found)
		ipc_notify(m_ptr->m_source);
  }

  return OK;
}
```

**逐行讲解**:
- `DSF_INITIAL`标志：请求立即获取现有匹配项
- 遍历所有数据条目，检查是否匹配
- 如果有匹配，设置位图并发送通知

---

## do_check函数

```c
/*===========================================================================*
 *				do_check				     *
 *===========================================================================*/
int do_check(message *m_ptr)
{
  struct subscription *subp;
  char *owner;
  endpoint_t entry_owner_e;
  int r, i;

  /* Find the subscription owner. */
  owner = ds_getprocname(m_ptr->m_source);
  if(owner == NULL)
	  return ESRCH;

  /* Lookup the owner's subscription. */
  if((subp = lookup_sub(owner)) == NULL)
	return ESRCH;

  /* Look for an updated entry the subscriber is interested in. */
  for(i = 0; i < NR_DS_KEYS; i++) {
	if(GET_BIT(subp->old_subs, i))
		break;
  }
  if(i == NR_DS_KEYS)
	return ENOENT;
```

**逐行讲解**:
- 查找订阅者的订阅
- 在位图中查找第一个被设置的位
- 如果没有更新，返回`ENOENT`

---

```c
  /* Copy the key name. */
  r = sys_safecopyto(m_ptr->m_source,
	(cp_grant_id_t) m_ptr->m_ds_req.key_grant, (vir_bytes) 0, 
	(vir_bytes) ds_store[i].key, strlen(ds_store[i].key) + 1);
  if(r != OK) {
	printf("DS: check: copy failed from %d: %d\n", m_ptr->m_source, r);
	return r;
  }

  /* Copy the type and the owner of the original entry. */
  entry_owner_e = ds_getprocep(ds_store[i].owner);
  m_ptr->m_ds_req.flags = ds_store[i].flags & DSF_MASK_TYPE;
  m_ptr->m_ds_req.owner = entry_owner_e;

  /* Mark the entry as no longer updated for the subscriber. */
  UNSET_BIT(subp->old_subs, i);

  return OK;
}
```

**逐行讲解**:
- 复制键名到调用者缓冲区
- 返回类型和所有者
- 清除位图中的位，表示已处理

---

## do_delete函数

```c
/*===========================================================================*
 *				do_delete				     *
 *===========================================================================*/
int do_delete(message *m_ptr)
{
  struct data_store *dsp;
  char key_name[DS_MAX_KEYLEN];
  char *source;
  char *label;
  int type = m_ptr->m_ds_req.flags & DSF_MASK_TYPE;
  int i, r;

  /* Lookup the source. */
  source = ds_getprocname(m_ptr->m_source);
  if(source == NULL)
	  return EPERM;

  /* Get key name. */
  if((r = get_key_name(m_ptr, key_name)) != OK)
	return r;

  /* Lookup the entry. */
  if((dsp = lookup_entry(key_name, type)) == NULL)
	return ESRCH;

  /* Only the owner can delete. */
  if(strcmp(dsp->owner, source))
	return EPERM;
```

**逐行讲解**:
- 只有所有者才能删除数据
- 查找条目，验证权限

---

```c
  switch(type) {
  case DSF_TYPE_U32:
	break;
  case DSF_TYPE_LABEL:
	label = dsp->key;

	/* Clean up subscriptions. */
	for (i = 0; i < NR_DS_SUBS; i++) {
		if ((ds_subs[i].flags & DSF_IN_USE)
			&& !strcmp(ds_subs[i].owner, label)) {
			free_sub_slot(&ds_subs[i]);
		}
	}

	/* Clean up data entries. */
	for (i = 0; i < NR_DS_KEYS; i++) {
		if ((ds_store[i].flags & DSF_IN_USE)
			&& !strcmp(ds_store[i].owner, label)) {
			update_subscribers(&ds_store[i], 0);

			ds_store[i].flags = 0;
		}
	}
	break;
  case DSF_TYPE_STR:
  case DSF_TYPE_MEM:
	free(dsp->u.mem.data);
	break;
  default:
	return EINVAL;
  }
```

**逐行讲解**:
- 根据类型执行不同的清理操作
- 标签类型：清理相关订阅和数据条目
  - 当服务终止时，其标签被删除
  - 同时清理该服务的订阅和发布的数据
- 字符串/内存类型：释放动态分配的内存

**级联删除示意**:
```
删除标签 "vm":
1. 查找所有者是"vm"的订阅 → 释放
2. 查找所有者是"vm"的数据条目 → 通知订阅者 → 清除
3. 清除标签条目本身
```

---

```c
  /* Update subscribers having a matching subscription. */
  update_subscribers(dsp, 0);

  /* Clear the entry. */
  dsp->flags = 0;

  return OK;
}
```

**逐行讲解**:
- 通知订阅者数据已删除
- 清除条目标志

---

## do_getsysinfo函数

```c
/*===========================================================================*
 *				do_getsysinfo				     *
 *===========================================================================*/
int do_getsysinfo(const message *m_ptr)
{
  vir_bytes src_addr;
  size_t length;
  int s;

  switch(m_ptr->m_lsys_getsysinfo.what) {
  case SI_DATA_STORE:
	src_addr = (vir_bytes)ds_store;
	length = sizeof(struct data_store) * NR_DS_KEYS;
	break;
  default:
  	return EINVAL;
  }

  if (length != m_ptr->m_lsys_getsysinfo.size)
	return EINVAL;

  if (OK != (s=sys_datacopy(SELF, src_addr,
		m_ptr->m_source, m_ptr->m_lsys_getsysinfo.where, length))) {
	printf("DS: copy failed: %d\n", s);
	return s;
  }

  return OK;
}
```

**逐行讲解**:
- 返回DS内部数据结构
- 用于调试和监控
- `sys_datacopy`：内核函数，复制数据到目标进程

---

## 要点总结

1. **静态数组管理**: DS使用固定大小的静态数组管理数据槽和订阅槽，避免动态内存分配的不确定性。

2. **正则表达式订阅**: 通过POSIX正则表达式实现灵活的订阅匹配，支持模式匹配而非精确匹配。

3. **权限模型**: 每个数据条目可以设置权限标志，限制谁可以覆盖、检索或订阅该数据。

---

## 灾难预演

**如果删除`free_sub_slot`中的`regfree`调用会怎样？**

每次订阅都会调用`regcomp`分配正则表达式内部资源。如果不调用`regfree`释放，这些资源会泄漏。随着订阅的创建和销毁，内存会逐渐耗尽，最终导致系统崩溃。

**如果删除`update_subscribers`中的`ipc_notify`调用会怎样？**

订阅者永远不会收到数据变化的通知。订阅机制完全失效。订阅者会一直等待通知，导致服务挂起或超时。

---

## 互动自测

1. **内存模型**: `ds_store`数组存储在哪个内存段？它的大小是多少字节（假设NR_DS_KEYS=128，DS_MAX_KEYLEN=40）？

2. **所有权流转**: 当调用`do_publish`发布字符串类型数据时，数据的内存所有权如何流转？谁负责释放？

3. **设计选择**: 为什么DS限制每个进程只能有一个订阅？如果允许多个订阅会有什么问题？

---

## Rust实现对比

```rust
#![no_std]

use core::mem;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use minix_rs::ds::*;
use minix_rs::ipc::{Message, Endpoint};
use minix_rs::sys::{OK, EINVAL, ENOMEM, EPERM, ESRCH, EEXIST};

extern crate alloc;

const NR_DS_KEYS: usize = 128;
const NR_DS_SUBS: usize = 256;
const DS_MAX_KEYLEN: usize = 40;

#[derive(Clone, Copy)]
struct DataStore {
    flags: u32,
    key: [u8; DS_MAX_KEYLEN],
    owner: [u8; DS_MAX_KEYLEN],
    value: DataValue,
}

#[derive(Clone, Copy)]
union DataValue {
    u32_val: u32,
    mem: MemData,
}

#[derive(Clone, Copy)]
struct MemData {
    data: *mut u8,
    length: usize,
    real_length: usize,
}

struct Subscription {
    flags: u32,
    owner: [u8; DS_MAX_KEYLEN],
    regex: Option<Regex>,
    old_subs: [u64; NR_DS_KEYS / 64],
}

static mut DS_STORE: [DataStore; NR_DS_KEYS] = [DataStore {
    flags: 0,
    key: [0; DS_MAX_KEYLEN],
    owner: [0; DS_MAX_KEYLEN],
    value: DataValue { u32_val: 0 },
}; NR_DS_KEYS];

static mut DS_SUBS: [Option<Box<Subscription>>; NR_DS_SUBS] = [None; NR_DS_SUBS];

fn alloc_data_slot() -> Option<&'static mut DataStore> {
    unsafe {
        for i in 0..NR_DS_KEYS {
            if DS_STORE[i].flags & DSF_IN_USE == 0 {
                return Some(&mut DS_STORE[i]);
            }
        }
    }
    None
}

fn alloc_sub_slot() -> Option<&'static mut Option<Box<Subscription>>> {
    unsafe {
        for i in 0..NR_DS_SUBS {
            if DS_SUBS[i].is_none() {
                return Some(&mut DS_SUBS[i]);
            }
        }
    }
    None
}

pub fn do_publish(m: &mut Message) -> Result<(), i32> {
    let key_name = get_key_name(m)?;
    let flags = m.m_ds_req.flags;
    
    let source = ds_getprocname(m.m_source)
        .ok_or(EPERM)?;
    
    if flags & DSF_TYPE_LABEL != 0 && m.m_source != RS_PROC_NR {
        return Err(EPERM);
    }
    
    let dsp = lookup_entry(&key_name, flags & DSF_MASK_TYPE);
    
    let slot = match dsp {
        Some(entry) if flags & DSF_OVERWRITE != 0 => {
            check_auth(entry, m.m_source, DSF_PRIV_OVERWRITE)?;
            entry
        }
        Some(_) => return Err(EEXIST),
        None => alloc_data_slot().ok_or(ENOMEM)?,
    };
    
    match flags & DSF_MASK_TYPE {
        DSF_TYPE_U32 => {
            slot.value.u32_val = m.m_ds_req.val_in.u32;
        }
        DSF_TYPE_STR | DSF_TYPE_MEM => {
            let length = m.m_ds_req.val_len;
            let data = copy_from_caller(m.m_source, m.m_ds_req.val_in.grant, length)?;
            
            unsafe {
                if slot.flags & DSF_IN_USE != 0 && (*slot.value.mem.data).is_null() {
                    dealloc(slot.value.mem.data, slot.value.mem.real_length);
                }
                slot.value.mem.data = Box::into_raw(data) as *mut u8;
                slot.value.mem.length = length;
                slot.value.mem.real_length = length;
            }
        }
        _ => return Err(EINVAL),
    }
    
    copy_str(&key_name, &mut slot.key);
    copy_str(source, &mut slot.owner);
    slot.flags = DSF_IN_USE | (flags & DSF_MASK_INTERNAL);
    
    update_subscribers(slot, true);
    
    Ok(())
}

fn check_auth(p: &DataStore, ep: Endpoint, perm: u32) -> Result<(), i32> {
    if p.flags & perm == 0 {
        return Ok(());
    }
    
    let source = ds_getprocname(ep)
        .ok_or(EPERM)?;
    
    if compare_str(source, &p.owner) {
        Ok(())
    } else {
        Err(EPERM)
    }
}

fn update_subscribers(dsp: &DataStore, set: bool) {
    let nr = unsafe {
        (dsp as *const DataStore).offset_from(DS_STORE.as_ptr()) as usize
    };
    
    for i in 0..NR_DS_SUBS {
        let sub = unsafe { &mut DS_SUBS[i] };
        if let Some(ref mut sub) = sub {
            if sub.flags & dsp.flags & DSF_MASK_TYPE == 0 {
                continue;
            }
            
            let ep = ds_getprocep(&sub.owner);
            if !check_sub_match(sub, dsp, ep) {
                continue;
            }
            
            if set {
                set_bit(&mut sub.old_subs, nr);
            } else {
                unset_bit(&mut sub.old_subs, nr);
            }
            
            ipc_notify(ep);
        }
    }
}
```

### Rust改进点

1. **Option类型**: 使用`Option<Box<Subscription>>`替代标志位检查，类型系统保证空槽不会误用。

2. **Result错误处理**: 使用`Result<T, E>`和`?`运算符，错误处理更加清晰，不会遗漏错误检查。

3. **内存安全**: `Box`提供自动内存管理，`drop` trait确保资源释放。

4. **unsafe隔离**: 只有访问静态全局变量和union需要unsafe，最小化不安全代码范围。

### unsafe说明

- `DS_STORE`和`DS_SUBS`的访问需要`unsafe`，因为它们是可变静态变量
- `DataValue` union的访问需要`unsafe`，因为Rust无法保证union的安全性
- 保证：DS是单线程服务，不会发生数据竞争
- 未来改进：使用`spin::Mutex`包装全局状态，实现线程安全
