# 02-is-fkey-contract：功能键观察者协议

> **源码**：`minix3/minix/include/minix/com.h:874-877` +
> `ipc.h:1447-1454`（请求）/`1925-1931`（回复）/`2570`/`2623`（union 槽位）+
> `keymap.h:14-17`（EXT/SHIFT）/`93-104`（F1-F12）/`135-146`（SF1-SF12）+
> `sysutil.h:43-46`（三宏）+ `libsys/fkey_ctl.c`（全 30 行）+
> `minix3/minix/servers/is/dmp.c:44-68`（`map_unmap_fkeys`）+
> `drivers/tty/tty/arch/i386/keyboard.c:60-78`（`obs_t`/数组/`debug_fkeys`）/
> `198-230`（kb 通路）/`401-415`（`kb_init_once`）/`429-527`（`do_fkey_ctl`，
> 含 `func_key` 调用的 `show_key_mappings` 私有段除外）/`532-585`（`func_key`）
> **Rust**：`os/libs/minix-types/src/ipc/tty.rs`（新，A-1）+
> `os/servers/is/src/tty_fkey.rs`（新）
> **draft 素材**：`draft/tmp_dmp.c.md` §map_unmap_fkeys 段（行文底料）
> **位置**：`sef_cb_init_fresh` 的 `map_unmap_fkeys(TRUE)` 调用点（01 §2.7）
> 在本篇落地为注册逻辑；主循环 TTY notify 分支（01 §2.3）的通知语义在本篇
> 落地为 TTY 侧契约

---

## 1. 概念：为什么观察一个按键需要三个命令

### 1.1 问题：通知里没有"哪个键"

> **目标读者**：读完 01 的读者。前置知识：01 的 notify 分类与 EDONTREPLY
> 语义（直接使用）。本章不讲分派（03）与数据获取（04）。

01 留了一个悬念：TTY 发来的通知**不带"哪个键"的信息**——`ipc_notify(dest)`
只取目标端点（`ipc.h:2820`），载荷由内核按固定格式填写（时间戳/挂起位图，
发送者塞不进自定义数据）。这不是 TTY 的吝啬，而是通知机制的先天限制。于是 IS 与
TTY 之间形成一种"订阅—查收"关系：平时 IS 登记"我关心哪些键"，按键发生时
TTY 只喊一声"有动静"，IS 再发一轮消息把位图拉回来读。

### 1.2 类比：报刊订阅

把 TTY 想象成报刊亭，IS 是订户。三个命令对应订阅关系的三个动作：

- **MAP（订阅）**："F1、F3……这些键有动静就喊我一声。"
- **UNMAP（退订）**："我关门了，以后别喊了。"（IS 退出时必须退订，否则
  报刊亭会对着空房子喊——01 §2.8 的清理顺序。）
- **EVENTS（取报）**："刚才喊的那声，是哪些键？"——TTY 把记账本上属于 IS
  的条目抄一份给 IS，同时把记账本清零。

为什么"喊一声"和"取报"要分成两次？因为喊（notify）是异步广播语义，
而取（EVENTS）是同步问答语义。合在一起就退化成"每次按键都发完整位图"的
同步调用，键盘中断路径上多一次大数据拷贝——微内核把高频路径做薄，
把解释工作推给出事件循环的服务进程，这正是 01 §1.1 那笔账的延续。

### 1.3 三层编号：键码、位号、下标

本章最容易晕的地方：同一个 F1 有**三个不同语境下的数字**，初见必须一次
分清，后文不再解释：

| 层 | 名字 | F1 的值 | 谁用 | 出处 |
|---|---|---|---|---|
| 键码 | key code | `0x110` | TTY 键盘解码（`map_key` 产物，`func_key` 的输入） | keymap.h:93 |
| 位号 | bit number | `1` | 线格式（消息位图，IS↔TTY 共同语言） | dmp.c:55-58 |
| 下标 | array index | `0` | TTY 观察者数组（`fkey_obs[0]`） | keyboard.c |

三层换算：键码 − `F1` ＝ 下标；下标 ＋ 1 ＝ 位号。**位号 0 永不使用**
（`1 << 0` 在双方代码中从未出现；源码未解释原因，此处不推测，只锁定行为）。
Rust 侧用 `FkeyId` 枚举让三层经类型
中转，永不直接换算（§3 D2）。

> **本章小结**：三命令（订/退订/取报，§1.2）+ 三层编号（键码/位号/下标，
> §1.3）。下一章 §2 把每条对应到 C 源码的行号。

### 1.4 边界声明

**前置依赖**：01（init 锚点 + notify 分类）；`com.h` 通知语义常识。

**本篇职责**：FKEY 三命令语义 + 两消息布局 + 位图约定 + `fkey_ctl` 客户端 +
IS 侧 `map_unmap_fkeys` + TTY 侧 `do_fkey_ctl`/`func_key`/kb 通路契约。

**不覆盖（移交）**：hooks 表内容（key→函数→描述）→ 03，本篇只消费 key
列表；`do_fkey_pressed` 如何读位图分派 → 03；转储内容 → 05~10；TTY 键盘
扫描/`kb_read` 全貌/`show_key_mappings` 私有输出 → TTY 自身（未 staging）。

---

## 2. C 源码分析

### 2.1 三命令与控制码（com.h:874-877）

```c
#define TTY_RQ_BASE 0x1300

#define TTY_FKEY_CONTROL	(TTY_RQ_BASE + 1) /* control an F-key at TTY */
#  define    FKEY_MAP		10	/* observe function key */
#  define    FKEY_UNMAP		11	/* stop observing function key */
#  define    FKEY_EVENTS	12	/* request open key presses */
```

（`minix3/minix/include/minix/com.h:874-877`）

控制码 `TTY_FKEY_CONTROL = 0x1301` 是消息类型（`m_type`），三个请求码
（10/11/12）装在载荷的 `request` 字段里——**两级编码**：外层选服务操作，
内层选子命令。这是 Minix3 驱动协议的常见形状（对比 RS 的 `RS_RQ_BASE`
家族），Rust 侧以 `FkeyReq` 枚举表达内层（§3 D3）。

### 2.2 两消息布局（ipc.h）

请求（IS→TTY）：

```c
typedef struct {
	int request;
	int fkeys;
	int sfkeys;

	uint8_t padding[44];
} mess_lsys_tty_fkey_ctl;
_ASSERT_MSG_SIZE(mess_lsys_tty_fkey_ctl);
```

（`minix3/minix/include/minix/ipc.h:1447-1454`，union 槽位 `:2570`）

回复（TTY→IS，同一消息原地写回）：

```c
typedef struct {
	int fkeys;
	int sfkeys;

	uint8_t padding[48];
} mess_tty_lsys_fkey_ctl;
_ASSERT_MSG_SIZE(mess_tty_lsys_fkey_ctl);
```

（`ipc.h:1925-1931`，union 槽位 `:2623`）

三点注意：① 载荷均为 56 字节（3×4+44 / 2×4+48），`_ASSERT_MSG_SIZE`
锁死，Rust 侧同等断言（§5）；② 回复**没有 `request` 回显位**——调用方
凭上下文知道自己问的是什么；③ 同一 `message` 缓冲区原地写回（请求发出去，
回复写进来），所以 `fkey_ctl` 的签名是"传入传出双向指针"形状（§2.4）。

### 2.3 键码表与位号约定（keymap.h + dmp.c）

```c
#define EXT	0x0100		/* Normal function keys		*/   /* keymap.h:14 */
#define SHIFT	0x0400		/* Shift key			*/   /* keymap.h:16 */
#define F1	(0x10 + EXT)   /* = 0x110 */   /* keymap.h:93-104，F1-F12 连续 */
#define SF1	(0x10 + SHIFT) /* = 0x410 */   /* keymap.h:135-146，SF1-SF12 连续 */
```

F1–F12 = `0x110–0x11B`（连续，12 个）；SF1–SF12 = `0x410–0x41B`（连续）。
连续性是 `func_key` 能用区间判断（`F1 <= key && key <= F12`）的前提，
 evidence 见 §2.8。

位号约定在 IS 侧组位代码里：

```c
  for (h = 0; h < NHOOKS; h++) {
      if (hooks[h].key >= F1 && hooks[h].key <= F12)
          bit_set(fkeys, hooks[h].key - F1 + 1);
      else if (hooks[h].key >= SF1 && hooks[h].key <= SF12)
          bit_set(sfkeys, hooks[h].key - SF1 + 1);
  }
```

（`minix3/minix/servers/is/dmp.c:50-58`；`bit_set(mask,n)` 即
`mask |= 1 << n`，`bitmap.h:5`）

F1→bit1 …… F12→bit12。bit0 空置——不是保留字段，是**历史写法**（`i+1`
循环习惯）沉淀成的线格式，双方都 innocent 地沿用。TTY 侧以
`bit_isset(mask, i+1)` 回读（keyboard.c:443/457 模式），闭环对称。

### 2.4 客户端 `fkey_ctl`（libsys，全文 30 行）

```c
int fkey_ctl(request, fkeys, sfkeys)
int request;				/* request to perform */
int *fkeys;				/* bit masks for F1-F12 keys */
int *sfkeys;				/* bit masks for Shift F1-F12 keys */
{
    message m;
    int s;
    memset(&m, 0, sizeof(m));
    m.m_lsys_tty_fkey_ctl.request = request;
    m.m_lsys_tty_fkey_ctl.fkeys = (fkeys) ? *fkeys : 0;
    m.m_lsys_tty_fkey_ctl.sfkeys = (sfkeys) ? *sfkeys : 0;
    s = _taskcall(TTY_PROC_NR, TTY_FKEY_CONTROL, &m);
    if (fkeys) *fkeys = m.m_tty_lsys_fkey_ctl.fkeys;
    if (sfkeys) *sfkeys = m.m_tty_lsys_fkey_ctl.sfkeys;
    return(s);
}
```

（`minix3/minix/lib/libsys/fkey_ctl.c` 全文；K&R 形参风格，注意非 ANSI 原型）

四取其要：① **NULL 指针容忍**（`(fkeys) ? *fkeys : 0`）——调用方可只传
一半位图；② `_taskcall` 是到 TTY 的**同步调用**（sendrec 语义），**无 grant、
无内存共享**，纯寄存器消息（plan §3.4"直接消息"即此意）；③ 返回状态码 `s`
与写回位图是**两个独立通道**：状态说"整体成败"，位图说"哪些位被消费了"；
④ 写回语义 = **失败位保留**：TTY 只 `bit_unset` 成功的位（§2.7），剩下的位
原样返回，调用方可重试或告警。

> **注释过时警告**：函数头注释称"Enabling succeeds unless the key is already
> bound to another process"（fkey_ctl.c:11-14），但 TTY 侧 MAP 的 EBUSY 检查
> 早被 `#if DEAD_CODE` 包住（§2.7）——注释说"会失败"，代码说"覆盖登记"。
> **以代码为准**，注释过时。Rust 侧不复述该注释（§3 D3）。

### 2.5 三宏（sysutil.h:43-46）

```c
#define fkey_map(fkeys, sfkeys) fkey_ctl(FKEY_MAP, (fkeys), (sfkeys))
#define fkey_unmap(fkeys, sfkeys) fkey_ctl(FKEY_UNMAP, (fkeys), (sfkeys))
#define fkey_events(fkeys, sfkeys) fkey_ctl(FKEY_EVENTS, (fkeys), (sfkeys))
```

薄封装，无逻辑。注意参数是**指针**（`int *`），`map_unmap_fkeys` 传栈变量
地址（§2.6），写回直接落回局部量。

### 2.6 IS 侧 `map_unmap_fkeys`（dmp.c:44-68）

```c
void
map_unmap_fkeys(int map)
{
  int fkeys, sfkeys;
  int h, s;

  fkeys = sfkeys = 0;

  for (h = 0; h < NHOOKS; h++) {   /* §2.3 组位循环，略 */
      ...
  }

  if (map) s = fkey_map(&fkeys, &sfkeys);
  else s = fkey_unmap(&fkeys, &sfkeys);

  if (s != OK)
	printf("IS: warning, fkey_ctl failed: %d\n", s);
}
```

（`minix3/minix/servers/is/dmp.c:44-68`；组位循环见 §2.3）

三个语义：① 位图由 hooks 表驱动（16 项，内容归 03）——**注册集合 = 转储
能力集合**，加一个转储就是多订一个键，天然同步；② `map` 非零即 MAP，
为零即 UNMAP（`int` 当 bool，Rust 侧正名为 `bool`）；③ 失败只告警不 panic
（对比 01 的 transport panic：注册失败不致命——TTY 可能还没起，IS 照常进
主循环；收不到通知而已）。写回的剩余位图被**丢弃**（局部量出作用域即弃）——
C 认为"告警里已有状态码，够了"。

### 2.7 TTY 侧 `do_fkey_ctl`：三命令状态机

观察者槽位类型与数组：

```c
typedef struct observer { endpoint_t proc_nr; int events; } obs_t;  /* keyboard.c:71 */
static obs_t  fkey_obs[12];	/* observers for F1-F12 */          /* :72 */
static obs_t sfkey_obs[12];	/* observers for SHIFT F1-F12 */    /* :73 */
```

每键一个槽：`proc_nr`（观察者端点，`NONE` 表空）+ `events`（待取计数器）。
初始化在 `kb_init_once`（`:401-415`，全置 `NONE`/0）。

**MAP——覆盖登记**（`keyboard.c:439-475`）：

```c
  case FKEY_MAP:			/* request for new mapping */
      result = OK;			/* assume everything will be ok*/
      for (i=0; i < 12; i++) {		/* check F1-F12 keys */
          if (bit_isset(m_ptr->m_lsys_tty_fkey_ctl.fkeys, i+1) ) {
#if DEAD_CODE
	/* Currently, we don't check if the slot is in use, so that IS
	 * can recover after a crash by overtaking its existing mappings.
	 * In future, a better solution will be implemented.
	 */
              if (fkey_obs[i].proc_nr == NONE) { 
#endif
    	          fkey_obs[i].proc_nr = m_ptr->m_source;
    	          fkey_obs[i].events = 0;
    	          bit_unset(m_ptr->m_lsys_tty_fkey_ctl.fkeys, i+1);
#if DEAD_CODE
    	      } else {
    	          printf("WARNING, fkey_map failed F%d\n", i+1);
    	          result = EBUSY;	/* report failure, but try rest */
    	      }
#endif
    	  }
      }
      /* ... SF 同构 ... */
```

先给乐观值 `OK`，逐位处理：置位位 → 登记发送者 + 计数器清零 + **回清请求位**
（写回语义的源头，§2.4③）。`#if DEAD_CODE` 段是理解关键：EBUSY 检查被
注释掉，理由写在注释里——**让崩溃重启的 IS 能抢回自己旧的登记**
（"overtaking its existing mappings"）。旧 IS 死的时候没 UNMAP（没机会），
新 IS 出生时 MAP 同一批键：若检查槽空，新 IS 永远订不上——STATELESS
（01 §2.6/A-10）在这里才真正闭环：**无状态重启的前提是注册端允许覆盖**。
SF 半同构（`:457-475`）。

**UNMAP——owner 检查**（`:477-507`）：置位位仅当
`fkey_obs[i].proc_nr == m_ptr->m_source` 才释放（清 NONE + 计数清零 +
回清位）；否则 `result = EPERM`，"report failure, but try rest"——**部分失败
不中断循环**，最终状态码是"最坏情况"，位图写回精确到每一位。这是"状态码 +
位图双通道"（§2.4）存在的原因：单状态码表达不了"12 个里成了 10 个"。

**EVENTS——消费读**（`:509-526`）：

```c
  case FKEY_EVENTS:
      result = OK;			/* everything will be ok*/
      m_ptr->m_tty_lsys_fkey_ctl.fkeys = m_ptr->m_tty_lsys_fkey_ctl.sfkeys = 0;
      for (i=0; i < 12; i++) {		/* check (Shift+) F1-F12 keys */
          if (fkey_obs[i].proc_nr == m_ptr->m_source) {
              if (fkey_obs[i].events) { 
                  bit_set(m_ptr->m_tty_lsys_fkey_ctl.fkeys, i+1);
                  fkey_obs[i].events = 0;
              }
          }
          /* ... SF 同构 ... */
      }
```

先清零回复位图，只抄**属于请求者且计数非零**的位，抄完清零计数器——
**读即消费**（destructive read）。恒返回 OK（无失败模式）。
注意所有权检查：只能取走**自己的**事件，别人的计数不受影响。

尾声（`:528-531`）：`m_type = result` + `ipc_sendnb`（**非阻塞发送**——TTY
是中断上下文相邻路径，不为回复阻塞）+ 失败 `printf`（TTY 侧私事）。

### 2.8 TTY 侧 `func_key` 与 kb 通路：通知从哪来

```c
static int func_key(scode)
int scode;			/* scan code for a function key */
{
  int key;
  int proc_nr;

  /* Ignore key releases. If this is a key press, get full key code. */
  if (scode & RELEASE_BIT) return(FALSE);	/* key release */
  key = map_key(scode);		 		/* include modifiers */

  if (F1 <= key && key <= F12) {		/* F1-F12 */
      proc_nr = fkey_obs[key - F1].proc_nr;	
      fkey_obs[key - F1].events ++ ;	
  } else if (SF1 <= key && key <= SF12) {	/* Shift F2-F12 */
      proc_nr = sfkey_obs[key - SF1].proc_nr;	
      sfkey_obs[key - SF1].events ++;	
  }
  else {
      return(FALSE);				/* not observable */
  }

  /* See if an observer is registered and send it a message. */
  if (proc_nr != NONE) { 
      ipc_notify(proc_nr);
  }
  return(TRUE);
}
```

（`keyboard.c:532-585`；`else if` 注释"Shift F2-F12"系原文笔误，应为 F1，
行为按代码 `SF1 <= key` 为准）

四取其要：① 释放键（`RELEASE_BIT`）直接忽略——通知只对应**按下**；
② `events++` **先于且独立于**观察者检查：即使 `proc_nr == NONE`（无人订阅），
计数器照样自增（脏计数，无人来取而已）；③ `ipc_notify(proc_nr)` **无载荷**
（01 §1.2 的源头）；④ 返回 TRUE/FALSE 给调用方 `kb_read`：

```c
	/* Function keys are being used for debug dumps (if enabled). */
	if (debug_fkeys && func_key(scode)) continue;   /* keyboard.c:205-206 */
```

`func_key` 返回 TRUE（可观察键）→ `continue`：该键**被吞掉**，不进入正常
输入处理（连转义序列都不产生）。反之当 `debug_fkeys` 为 0，`func_key` 根本
不被调用，F/SF 键走转义序列分支（`:223-224`），CTRL+Fn（`CF1-CF12`）仅在
`!debug_fkeys` 时才产生序列——**同一批物理键在两种模式下是互斥解释**，
`func_key` 注释里"CTRL/ALT reserved"即此意。

`debug_fkeys` 开关三处（`:78` 默认 1；`:206` 吞键门；`:406` `env_parse`
可配）。**辨析**：它与 `rc.minix:117` 的 `sysenv debug_fkeys` **同名不同层**——
前者是 TTY 驱动进程的环境变量（管"TTY 拦不拦截"），后者是系统环境（管
"RS 起不起 IS"）。两处都关才彻底无声；只关一处会出现"IS 活着但永远收不到
通知"或"TTY 吞了键但无人消费"的半开状态。排障时先查这一对。

### 2.9 小结：一次 MAP 的旅程

`sef_cb_init_fresh` → `map_unmap_fkeys(TRUE)`（§2.6，hooks 组位）→
`fkey_map` 宏（§2.5）→ `fkey_ctl`（§2.4，`_taskcall` 同步）→ TTY
`do_fkey_ctl` MAP 分支（§2.7，覆盖登记 + 回清位）→ 返回 OK。之后键盘中断
→ `func_key` 计数 + `ipc_notify`（§2.8）→ IS 主循环 notify 分支（01 §2.3）
→ `do_fkey_pressed` 发 `FKEY_EVENTS`（§2.7 消费读）→ 03 分派。完整旅程在
03 §1 以次主线路径图收口，本篇只负责把每段契约钉死。

---

## 3. Rust 设计决策

### 3.1 D1：协议类型入 `minix-types`（A-1 兑现）

`[ARCH: A-1]` 三处：① 本节（Minix3 `mess_*` int 位图 → 强类型载荷）；
② design D1；③ `tty.rs` 模块注释。内容：`MessLsysTtyFkeyCtl`
（request/fkeys/sfkeys + 44B pad = 56B）与 `MessTtyLsysFkeyCtl`
（fkeys/sfkeys + 48B pad = 56B），`#[repr(C)]` + 56B 编译断言 +
布局表注释（`MessLsysKrnSysGetinfo` 同款体例）；常量
`TTY_RQ_BASE`/`TTY_FKEY_CONTROL`/`FKEY_MAP`/`FKEY_UNMAP`/`FKEY_EVENTS`/
`FKEY_COUNT`/`F1..F12`/`SF1..SF12`（C 行号逐项引用）；`MessageUnion`
新增 `m_lsys_tty_fkey_ctl` / `m_tty_lsys_fkey_ctl` 两槽位（union 已有
`raw` + 不透明 Debug，增槽不影响既有布局）。

归属论证：协议跨 TTY/IS 两服务，`minix-types` 是唯一权威（crate 文档
"protocol between services"）；放 is crate 会与未来 TTY crate 重复定义
（跨文档重复模式 A）。键码常量（keymap.h 源）一并入 `tty.rs`：03 的
hooks 表从这里 import，不在 03 重定义（§2.4g 跨篇权威）。

### 3.2 D2：三层编号经 `FkeyId` 中转

```rust
pub enum FkeyId { F1, F2, ..., F12, Sf1, ..., Sf12 }

impl FkeyId {
    pub const fn bit(self) -> u32;      // F1→1 … SF12→12（无 0）
    pub const fn key_code(self) -> i32; // F1→0x110 …（minix-types 常量）
}
```

`fkey_bits(&[FkeyId]) -> (u32, u32)` 纯函数组位。位号 0 没有对应变体——
"bit0 不用"从注释变成**不可编译**。下标层（0..11）只出现在 fake TTY
镜像内部，不出模块（§3 D4）。否决裸 `u32` 位图（translate 味道，模式 16：
bit0 误用、F/SF 混层无拦截）。

### 3.3 D3：客户端 trait 缝 + 过时注释点破

`enum FkeyReq { Map, Unmap, Events }`（值 10/11/12）；
`trait FkeyCtlTransport { fn fkey_ctl(&mut self, req, fkeys, sfkeys)
-> (status, fkeys, sfkeys) }`——三元组即 §2.4 的"状态 + 双写回"双通道。
生产实现 forward ref（`minix-sys` `_taskcall` 接线时落地，01 §3 D2 同款）；
MAP/UNMAP 调用方判 `status != OK` 告警（dmp.c:63-65），EVENTS 取位图
（恒 OK）。否决备选：裸 `i32` 传 request（调用方易传错命令，且与三元组写回
语义无类型关联）；把 `_taskcall` 直接写进 `IsServer`（与传输缝重复，
且不可单测）。fkey_ctl 头注释的"bound 则失败"与 DEAD_CODE 现状矛盾：
文档 §2.4 已点破，代码注释引 keyboard.c DEAD_CODE 段，**不复述旧注释**。

### 3.4 D4：TTY 状态机只契约、不实现

do_fkey_ctl/func_key/kb 通路归未来 TTY crate。本篇交付：① 契约表
（§2.7/§2.8逐条行为）；② `fake_tty` 镜像状态机（test 脚手架，模块名明示
非实现）：12+12 槽位 × MAP 覆盖/UNMAP-owner-EPERM/EVENTS-消费清零 ×
func_key 计数-通知语义。镜像与 C 逐条对断言（§5），是"契约可执行化"，
不是第二实现（Redox 同款做法：hosted 测试替身与内核实现分离命名）。

### 3.5 D5：`map_unmap` 入参 keys 化，interim 键源显式化

```rust
pub struct FkeyCtlError { pub status: i32, pub leftover_fkeys: u32, pub leftover_sfkeys: u32 }

pub fn map_unmap_keys<C: FkeyCtlTransport>(client: &mut C, map: bool, keys: &[FkeyId])
    -> Result<(), FkeyCtlError>;

pub fn pull_events<C: FkeyCtlTransport>(client: &mut C) -> (i32, u32, u32);
```

> **03 修订**：`pull_events` 初版返回 `(u32, u32)`（"恒 OK，不断言状态"），
> 03 的 `do_fkey_pressed` 需要判 `s < 0`（dmp.c:84，传输失败告警），故改为
> 三元组（状态直通）。调用方契约同步见 §4.3(3)。

C 的 `map_unmap_fkeys(map)` 读全局 hooks（§2.6）；hooks 表归 03，故本篇
函数取显式 `keys`。否决备选：02 自建 hooks 表副本（与 03 双源，pattern A）。
01 的 `request_fkey_map` 缝经此填实。
> **03 接管**：键源初版为 interim `INIT_FKEYS` 常量 + TODO（P1/code/factual）；
> 03 以 hooks 派生列表接管，常量已删除（见 `03-is-dump-dispatch.md` §4）。

---

## 4. 实现详解

### 4.1 模块树

```text
os/libs/minix-types/src/ipc/tty.rs   — 常量 + 两载荷类型 + 布局单测（D1）
os/libs/minix-types/src/ipc/message.rs — union 增两槽位（D1）
os/servers/is/src/tty_fkey.rs        — FkeyId/位组装/transport/编排/fake（D2-D5）
os/servers/is/src/lib.rs             — IsServer<T,F> 双泛型 + request_fkey_map 填实
os/servers/is/src/main.rs            — 二进制配 (UnimplementedTransport, UnimplementedFkeyCtl)
```

### 4.2 关键签名（与 §3 一致，Gate D-5 依据）

```rust
// minix-types tty.rs
pub const TTY_RQ_BASE: i32 = 0x1300; pub const TTY_FKEY_CONTROL: i32 = 0x1301;
pub const FKEY_MAP: i32 = 10; pub const FKEY_UNMAP: i32 = 11; pub const FKEY_EVENTS: i32 = 12;
pub const FKEY_COUNT: usize = 12;
pub const F1: i32 = 0x110; /* … F12 = 0x11B */ pub const SF1: i32 = 0x410; /* … SF12 = 0x41B */
pub struct MessLsysTtyFkeyCtl { pub request: i32, pub fkeys: i32, pub sfkeys: i32, pub _padding: [u8; 44] }
pub struct MessTtyLsysFkeyCtl { pub fkeys: i32, pub sfkeys: i32, pub _padding: [u8; 48] }

// is tty_fkey.rs
pub enum FkeyId { F1,…,F12, Sf1,…,Sf12 }
pub enum FkeyReq { Map, Unmap, Events }
pub trait FkeyCtlTransport { fn fkey_ctl(&mut self, req: FkeyReq, fkeys: u32, sfkeys: u32) -> (i32, u32, u32); }
pub struct FkeyCtlError { pub status: i32, pub leftover_fkeys: u32, pub leftover_sfkeys: u32 }
pub fn map_unmap_keys<C: FkeyCtlTransport>(client: &mut C, map: bool, keys: &[FkeyId]) -> Result<(), FkeyCtlError>;
pub fn pull_events<C: FkeyCtlTransport>(client: &mut C) -> (i32, u32, u32);
```

常量权威位置（§2.4g）：FKEY/键码常量唯一定义于 `minix-types::ipc::tty`
（03 hooks 表从此 import）；`TTY_PROC_NR` 沿用 `Endpoint::TTY`
（01 既有，不另定义）。

### 4.3 关键不变量

1. `fkey_bits` 纯函数；bit0 永不置位（无对应变体）。
2. MAP 失败（非 OK）→ 调用方告警，不 panic（§2.6）；UNMAP 部分失败→
   `FkeyCtlError` 带 leftovers（§2.7 双通道）。
3. `pull_events` 返回三元组（状态直通；EVENTS 良好调用恒 OK，`s < 0` 仅传输失败——03 修订，见 §3 D5）。
4. fake TTY 与 §2.7/§2.8 逐条对（镜像表见 §5.2）。
5. 键源 = hooks 派生（03 起；`INIT_FKEYS` interim 已删除）。

---

## 5. 测试要点

### 5.1 minix-types 布局（tty.rs 单测）

| # | 断言 | C 依据 |
|---|---|---|
| T1 | `size_of::<MessLsysTtyFkeyCtl>() == 56` | ipc.h:1454 `_ASSERT_MSG_SIZE` |
| T2 | `size_of::<MessTtyLsysFkeyCtl>() == 56` | ipc.h:1931 同上 |
| T3 | 控制码/请求码值（0x1301/10/11/12） | com.h:874-877 |
| T4 | 键码抽查（F1=0x110/F12=0x11B/SF1=0x410/SF12=0x41B + 连续性） | keymap.h:93-104/135-146 |

### 5.2 IS 侧行为（tty_fkey.rs + lib.rs，fake transport）

| # | 场景 | 期望 | C 依据 |
|---|---|---|---|
| T5 | 位号真值（F1→1…F12→12，SF1→1…） | 全 24 断言 | §2.3 |
| T6 | `fkey_bits(&[])` 空集 | (0,0) | §2.6 零初值 |
| T7 | fake MAP 覆盖登记（先 A 后 B 同键） | B 持有 + OK + 回清位 | §2.7 DEAD_CODE |
| T8 | fake UNMAP 非 owner | EPERM + 位保留 | §2.7 owner 检查 |
| T9 | fake EVENTS 消费 | 仅属己非零位 + 计数清零 + 他人不动 | §2.7 消费读 |
| T10 | func_key 无观察者仍计数、有观察者才 notify | 计数器+1 & notify 记录 | §2.8 |
| T11 | `map_unmap_keys` EPERM 路径 | Err 携带 leftovers | §2.4 双通道 |
| T12 | `startup` 经 02 缝 | Ok(OK) + mapped 置位（更新 01 的 ENOSYS 旧断言） | §2.6 |
| T13 | `pull_events` 三元组直通 | fake 预置计数 → `(OK, fkeys, sfkeys)`（03 起含状态） | §2.7 |
| T14 | `FkeyReq::code` 映射（10/11/12） | 枚举→线值 | com.h:875-877 |

### 5.3 测试统计（截至 2026-09-04）

- `cargo test -p minix-is`：**32 passed, 0 failed**（`state` 1 + `dispatch` 7 +
  `sef` 4 + `lib` 9 + `tty_fkey` 11；含 01 的 21 个；其中 2 个 `#[should_panic]`
  锁定 transport 失败路径）。
- `cargo test -p minix-types`：**119 passed, 0 failed**（`tty.rs` 新增 4 个布局/
  码值断言；其余为既有）。
- `cargo check/clippy -p minix-is -p minix-types`：两 crate 零新增警告
 （`minix-sys` stub 的预先存在警告与本篇无关）。
- 完整清单：`rg "#\[test\]" os/servers/is/src/tty_fkey.rs os/libs/minix-types/src/ipc/tty.rs`。

---

## 6. 过渡

本篇闭环了 01 的 init 锚点（注册逻辑落地，`startup` 转 Ok）与 notify 语义
（TTY 侧契约钉死）。但收到通知后"哪个键→哪个函数"仍未知：`do_fkey_pressed`
如何拉 EVENTS 位图、如何按 hooks 表匹配、16 项 key→函数→描述是什么——
是下一篇 03（`03-is-dump-dispatch.md`）的入口。`pull_events` 已备好，
03 只管调用。

---

## 7. 参见

- `01-is-init-main.md` §2.7/§2.3：init 锚点 + notify 分支（本篇的调用方）
- `03-is-dump-dispatch.md`（待写）：hooks 表 + `do_fkey_pressed`（本篇 key 列表的消费者 + `pull_events` 调用方）
- `../01-stage-kernel/12-ipc-core.md`：`_taskcall`/notify 原语
- `draft/tmp_dmp.c.md` §map_unmap_fkeys 段：行文底料（其 Rust 块同 01 §3.0 理由废弃）
- plan §3.4（02 职责行）/§4（A-1/A-2）/§5.2（头文件表）/§5.3（02 函数清单）
