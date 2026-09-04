# 03-is-dump-dispatch：转储分派

> **源码**：`minix3/minix/servers/is/dmp.c:1-40`（hooks 表）+
> `:70-72`（`pressed` 宏）+ `:73-101`（`do_fkey_pressed`）+
> `:103-117`（`key_name`）+ `:118-132`（`mapping_dmp`）
> （`:44-68` `map_unmap_fkeys` 已归 02）
> **Rust**：`os/servers/is/src/dispatch.rs`（分发表，01 分类器同文件上半）+
> `os/servers/is/src/lib.rs`（`handle_fkey_pressed`/`run_dump`）
> **draft 素材**：`draft/tmp_dmp.c.md` §hooks/`do_fkey_pressed`/`key_name`/
> `mapping_dmp` 段（行文底料；其 Rust 块同 01 §3.0 理由废弃）
> **位置**：主循环 TTY notify 分支的 `do_fkey_pressed` 调用点（01 §2.3）；
> 次主线路径图本篇收口（plan §1.3）

---

## 1. 概念：分派为什么是"查表+循环调用"

### 1.1 问题：一个通知可能对应多个转储吗

> **目标读者**：读完 01/02 的读者。前置知识：EVENTS 拉模式（02 §2.7）、
> notify 分类（01 §2.3）。本章不讲各转储内容（05~10）与数据获取（04）。

02 留了一个问题：EVENTS 一次拉回的是**位图**——12+12 为位里可能同时
有好几个 1（用户在 IS 调度出来之前连按了 F1 和 F5，或者按住没放）。分派
必须回答：多个 pending 键是只处理第一个，还是全处理？按什么顺序？

C 的答案写在循环结构里：`for` 遍历全表，命中的**逐个调用，无 break**
（§2.3）。没有优先级，没有"一次只处理一个"的限制——循环结构本身不定序、
不截断（各转储函数的 effects 是否独立，是 05~10 逐篇的主题，此处不断言）。

### 1.2 类比：电闸箱

把 hooks 表想象成配电箱里的电闸阵列：一轮 EVENTS 位图就是"哪些闸跳了"的
一览。电工的操作是按表顺序把跳了的闸**逐个**推回去——不会只推第一个，
也不会按跳闸时间排序。`do_fkey_pressed` 就是这位电工：拉清单（EVENTS）、
按表走（hooks 序）、逐个合闸（调转储函数）、收工不汇报（恒 EDONTREPLY）。

### 1.3 边界声明

**前置依赖**：01（classify HandleFkey 分支）；02（`pull_events`、位号约定、
`INIT_FKEYS` interim——本篇闭合其 TODO）。

**本篇职责**：hooks 表 16 项 + `pressed` 宏 + `do_fkey_pressed` +
`key_name` + `mapping_dmp` + 次主线路径图。

**不覆盖（移交）**：各 dump 函数体 → 05~10（本篇以 `DumpId` 枚举占位）；
5 条数据获取通道 → 04（dump 体调什么接口是 04 的事）；输出通道 →
A-6（`mapping_dmp` 的行格式常量先行，打印执行待通道）。

> **本章小结**：多键同轮则按表序逐个执行（§1.1 电闸箱）。下一章 §2 把表、
> 宏、循环、命名、格式逐行对应到 `dmp.c`。

---

## 2. C 源码分析

### 2.1 hooks 表 16 项（dmp.c:11-35）

```c
struct hook_entry {
	int key;
	void (*function)(void);
	char *name;
} hooks[] = {
	{ F1, 	proctab_dmp, "Kernel process table" },
	{ F3,	image_dmp, "System image" },
	{ F4,	privileges_dmp, "Process privileges" },
	{ F5,	monparams_dmp, "Boot monitor parameters" },
	{ F6,	irqtab_dmp, "IRQ hooks and policies" },
	{ F7,	kmessages_dmp, "Kernel messages" },
	{ F8,	vm_dmp, "VM status and process maps" },
	{ F10,	kenv_dmp, "Kernel parameters" },
	{ SF1,	mproc_dmp, "Process manager process table" },
	{ SF2,	sigaction_dmp, "Signals" },
	{ SF3,	fproc_dmp, "Filesystem process table" },
	{ SF4,	dtab_dmp, "Device/Driver mapping" },
	{ SF5,	mapping_dmp, "Print key mappings" },
	{ SF6,	rproc_dmp, "Reincarnation server process table" },
	{ SF8,  data_store_dmp, "Data store contents" },
	{ SF9,  procstack_dmp, "Processes with stack traces" },
};
```

（`minix3/minix/servers/is/dmp.c:11-35`；`NHOOKS` 即数组长度，`:40`）

三取其要：① 表是**唯一键源**——02 的注册位图与本篇的分派匹配读的是同一
张表（"注册集合 = 转储能力集合"，02 §2.6）；② 缺席的键（F2/F9/F11/F12、
SF7/SF10-12）既订不上也派不上——按了也只在 TTY 计数器里脏增（02 §2.8②），
IS 永远看不见；③ `mapping_dmp` **自指**：SF5 的转储内容就是打印这张表本身
（§2.6）——调试台的自我介绍页。

### 2.2 `pressed` 宏：区间 + 位双检（dmp.c:70-72）

```c
#define pressed(start, end, bitfield, key) \
	(((start) <= (key)) && ((end) >= (key)) && \
	 bit_isset((bitfield), ((key) - (start) + 1)))
```

调用点固定两种：`pressed(F1, F12, fkeys, hooks[h].key)` 与
`pressed(SF1, SF12, sfkeys, hooks[h].key)`（`:90-94`）。宏做两件事：
先验键码落在哪个银行（区间检），再验该银行位图对应位（`key-start+1`
正是 02 §2.3 的位号）。区间检看似冗余（表内键天然分区）——实则是宏的
**银行选择器**：同一个宏服务两个银行，调用方用区间参数 declaratively
指定"查哪本账"。Rust 侧原样镜像为纯函数（§3 D2），含 `+1`。

### 2.3 `do_fkey_pressed`（dmp.c:73-101）

```c
int do_fkey_pressed(m)
message *m;					/* notification message */
{
  int s, h;
  int fkeys, sfkeys;

  /* The notification message does not convey any information, other
   * than that some function keys have been pressed. Ask TTY for details.
   */
  s = fkey_events(&fkeys, &sfkeys);
  if (s < 0) {
      printf("IS: warning, fkey_events failed: %d\n", s);
  }

  /* Now check which keys were pressed: F1-F12, SF1-SF12. */
  for(h=0; h < NHOOKS; h++) {
	if (pressed(F1, F12, fkeys, hooks[h].key)) {
		hooks[h].function();
	} else if (pressed(SF1, SF12, sfkeys, hooks[h].key)) {
		hooks[h].function();
	}
  }

  /* Don't send a reply message. */
  return(EDONTREPLY);
}
```

逐段：

1. **`m` 全程未用**（K&R 形参 `message *m` 仅占位）——注释亲口承认：
   通知除"有键被按了"外不传达任何信息，细节问 TTY。这是 A-2 拉模式在
   C 侧最直白的自白（01 §2.3 的分类器只读发送者、02 §2.7 的消费读，
   到此齐备）。
2. **`s < 0` 告警**：`_taskcall` 成功时返回服务端的 `m_type`（非负结果码），
   仅当 `ipc_sendrec` 本身失败才直返负值（`taskcall.c:1-17` 头注释
   "returns negative error codes directly" + `:14-16` 先判 `status != 0`）。
   所以这个分支**不是**"TTY 报 EPERM"（EVENTS 恒 OK，02 §2.7）——是
   "连 TTY 都没联系上"。
   且告警后**继续分派**：`fkeys/sfkeys` 是栈变量，`fkey_ctl` 入口
   `memset(&m, 0)` 保证失败时写回的是零位图（02 §2.4）——"意外安全"：
   联系不上就当" ничего没按"，不崩，不重试，不升级。
3. **循环无 break**（§1.1 电闸箱）：多匹配顺序执行，表序即执行序。
   `else if` 的含义是"单钩分属一银行"，不是"单轮只派一个"。
4. **恒 `EDONTREPLY`**（`:99`）：分派永不回复——转储输出走的是打印通道，
   不是 IPC 回复（输出通道见 A-6）。

### 2.4 key_name：静态缓冲三式（dmp.c:103-117）

```c
static char *key_name(int key)
{
	static char name[15];

	if(key >= F1 && key <= F12)
		snprintf(name, sizeof(name), " F%d", key - F1 + 1);
	else if(key >= SF1 && key <= SF12)
		snprintf(name, sizeof(name), "Shift+F%d", key - SF1 + 1);
	else
		strlcpy(name, "?", sizeof(name));
	return name;
}
```

`" F%d"`（前导空格凑列宽）/ `"Shift+F%d"` / `"?"`（越界兜底）。`static`
缓冲 = 不可重入，但 IS 单线程 + 调用点（§2.6 `printf` 实参）立即消费，
实践安全。Rust 侧消掉缓冲，变无状态纯函数；`"?"` 分支因 `FkeyId` 类型
不可达而消除（§3 D5）——C 的防御性 else 在枚举世界里没有对应物，
这是重写（非直译）的一处小而干净的证据。

### 2.5 mapping_dmp：列宽格式（dmp.c:118-132）

```c
void mapping_dmp(void)
{
  int h;

  printf("Function key mappings for debug dumps in IS server.\n");
  printf("        Key   Description\n");
  printf("-------------------------------------");
  printf("------------------------------------\n");

  for(h=0; h < NHOOKS; h++)
      printf(" %10s.  %s\n", key_name(hooks[h].key), hooks[h].name);
  printf("\n");
}
```

行格式 `" %10s.  %s\n"`：键名（`key_name` 已带前导空格，`"Shift+F12"` 9 字符
是最大值，`%10s` 恰好收住）+ 句点 + 描述。标题三行 + 16 行 + 空行。
本篇只钉**格式常量**（§4），打印执行待 A-6 通道——与 01 §3 D4 同一通道，
`mapping_dmp` 体随 05~10 期落地（`DumpId::Mapping` 占位）。

### 2.6 次主线路径图：一次 F1 按压的旅程

plan §1.3 的次主线在本篇收口（02 §2.9 的 MAP 旅程是上篇， formation 以下
是下篇——按键发生后的完整链路）：

```text
键盘中断（F1 按下）
  → TTY kb 通路：debug_fkeys 门（开）→ func_key：fkey_obs[0].events++ →
    owner == IS → ipc_notify(IS)（无载荷，02 §2.8）
  → IS 主循环：get_work → is_notify ✓ → slot == TTY ✓ → do_fkey_pressed（01 §2.3）
  → 本篇：fkey_events 拉位图（02 §2.7 消费读）→ pressed(F1..F12) 命中第 0 钩
  → proctab_dmp()（05）→ 数据面 sys_getinfo（04）→ printf 输出
  → return EDONTREPLY → 主循环不回复（01 §2.5）
```

每段的证据行号：keyboard.c:206/532-585 → main.c:48-52 → dmp.c:83-99 →
05/04 各篇。读者至此能回答 plan §1.2 的问题："03 位于主循环 dispatch 的
TTY 分支内，次主线的心脏位置。"

---

## 3. Rust 设计决策

### 3.1 D1：hooks 表→数据（表序即语义）

`Hook { key: FkeyId, name: &'static str, dump: DumpId }` + `HOOKS: &[Hook; 16]`
（§2.1 逐项对，`NHOOKS` → `.len()`）。函数指针→`DumpId` 枚举（变体序 =
表序）：03 没有可指的函数体（05~10 未写），指针表既不可建也不可测；
枚举是"注册表与执行分离"（Redox 同款：scheme 表只命名能力，执行另绑定）。
否决：`fn` 指针表直译（悬空 + 不可单测）。

### 3.2 D2：pressed 宏→纯函数（原样镜像）

`pressed(start, end, bitfield, key)` 含 `+1` 原样保留——连"怪味"一起镜像，
单测喂码值对拍（§5）。宏变函数的唯一改动是获得类型（`u32` 位图非 `int`）。
否决：内联进循环（双检语义值得独立命名 + 独立真值表）。

### 3.3 D3：分派→回调式（零分配）

`dispatch_each(fkeys, sfkeys, visit: impl FnMut(&Hook))` 表序遍历，无 break
（§2.3③单测锁定）。回调式代替返回 `Vec`：`no_std` 无 alloc 依赖（01 同款
约束）；代替 16 臂 switch（表驱动更近 C 语义——C 本来就是表）。
`if/else-if` 写成 `||`（clippy 同形分支告警；单钩单银行故等价，注释存证）。

### 3.4 D4：m 参数消失

`handle_fkey_pressed()` 取无 `Message` 参数——C 的 `m` 本就未用（§2.3①），
占位形参是 K&R 残留。A-2 在此的签名级体现。否决 `_m: &Message` 占位
（translate 残留，模式 65）。

### 3.5 D5：key_name→静态 str（"?" 分支的类型级消除）

24 臂字面量匹配；C 的 `else "?"` 因 `FkeyId` 全覆盖而不可达——防御性 else
在枚举世界里没有对应物，直接删除（§2.4 末的"小而干净的证据"）。
列宽不变式（最长 9 < 10）单测锁定（§5）。

### 3.6 D6：run_dump 空体 + 恒抑制 + 失败告警通道

C dump 体 void → `run_dump(DumpId)` 空体（05~10 按变体填体）；`handle`
恒返 `EDONTREPLY`（§2.3④）。EVENTS `status < 0` → 新增
`SefTransport::warn_fkey_events`（dmp.c:84-86 告警的通道化；与
`warn_illegal` 同属 A-6 诊断通道，分方法保调用点可 grep）。
02 合约两修订（`pull_events` 三元组 + `INIT_FKEYS` 删除→hooks 派生）
见 §4.4 同步段。

---

## 4. 实现详解

### 4.1 模块树（增量）

```text
os/servers/is/src/dispatch.rs — 上半 01 分类器 + 下半 03 分发表（D1-D3/D5）
os/servers/is/src/lib.rs      — handle_fkey_pressed 填实 + run_dump 空体（D6）
os/servers/is/src/sef.rs      — SefTransport += warn_fkey_events（D6）
os/servers/is/src/tty_fkey.rs — pull_events 三元组（02 修订）；INIT_FKEYS 已删
```

### 4.2 关键签名（与 §3 一致，Gate D-5 依据）

```rust
// dispatch.rs（03 新增）
pub enum DumpId { Proctab, Image, Privileges, Monparams, Irqtab, Kmessages, Vm, Kenv, Mproc, Sigaction, Fproc, Dtab, Mapping, Rproc, DataStore, Procstack }
pub struct Hook { pub key: FkeyId, pub name: &'static str, pub dump: DumpId }
pub static HOOKS: &[Hook; 16];
pub const fn pressed(start: i32, end: i32, bitfield: u32, key: i32) -> bool;
pub fn dispatch_each(fkeys: u32, sfkeys: u32, visit: impl FnMut(&Hook));
pub const fn key_name(key: FkeyId) -> &'static str;
pub const MAPPING_TITLE: &str; pub const MAPPING_COLUMNS: &str;
// lib.rs（03 填实）
fn handle_fkey_pressed(&mut self) -> i32;   // EVENTS + 分派 + 恒 EDONTREPLY
fn run_dump(&mut self, _dump: DumpId);       // 空体（05~10 填体）
```

常量权威位置（§2.4g）：hooks 表唯一定义于 `dispatch.rs::HOOKS`
（02 的 `INIT_FKEYS` 已删除，无双源）；键码常量沿用 `minix-types::tty`
（02 既有）。

### 4.3 关键不变量

1. 表序 == dmp.c:18-35 行序（16 项逐项对，§5 锁死）。
2. 多匹配顺序执行无 break（§2.3③）。
3. `handle` 恒 `EDONTREPLY`（§2.3④）；`m` 不存在（D4）。
4. EVENTS 失败（<0）告警且继续分派（§2.3②；memset 零兜底见 02 §2.4）。
5. `key_name` 输出恒 ≤10 列（`%10s` 不变量）。

### 4.4 双文档同步（02 修订 + 01 行文）

- 02 `pull_events` → 三元组（02 §3 D5/§4.2/§4.3/§5 T13 已同步）。
- 02 `INIT_FKEYS` + TODO → 已删除，键源改 hooks 派生（02 §3 D5/§4.3 已同步"已接管"）。
- 01 §3 D3/§4.2/§5 T10-T12 → handle 填实后的行为（抑制无 send、send panic 不可达）已同步。
- 01 快照 v1 为首轮历史记录，不改（03 scan 记录增量方法 `warn_fkey_events`）。

---

## 5. 测试要点

| # | 场景 | 期望 | C 依据 |
|---|---|---|---|
| T1 | pressed 真值表（命中/位清/越界/跨银行） | 6 断言 | dmp.c:70-72 |
| T2 | 多匹配表序无 break（F1+F3+SF9） | [Proctab, Image, Procstack] | dmp.c:89-95 |
| T3 | 空位图 | 零访问 | 同上 |
| T4 | 银行隔离（全 F 位/全 SF 位） | 各 8 | 同上 |
| T5 | key_name 三式抽查 + 24 全宽 ≤10 | — | dmp.c:103-117/125 |
| T6 | 表 16 项序 + 名列逐项 | — | dmp.c:18-35 |
| T7 | handle：TTY 通知 + F1 pending | 分派 + 抑制无 send + 无告警 | dmp.c:73-101 |
| T8 | handle：EVENTS status<0 | 告警恰一次 + 仍抑制 | dmp.c:84-86 |
| T9 | send 通道经 step 不可达 | fail_send 下仍无 send | main.c:65-66 + dmp.c:99 |

### 5.3 测试统计（截至 2026-09-04）

- `cargo test -p minix-is`：**41 passed, 0 failed**（`state` 1 + `dispatch` 15 +
  `sef` 4 + `lib` 10 + `tty_fkey` 11；含 01/02 全部；`#[should_panic]` 1 个）。
- `cargo test -p minix-types`：**119 passed**（02 的 4 个，无新增）。
- `cargo clippy/check -p minix-is`：本 crate 零警告。
- 完整清单：`rg "#\[test\]" os/servers/is/src/dispatch.rs os/servers/is/src/lib.rs`。

---

## 6. 过渡

本篇闭环了主循环 TTY 分支（拉取→匹配→执行→抑制）与次主线路径图。但
`run_dump` 还是空体——每个 `DumpId` 背后真正的"数据从哪来"（`sys_getinfo`
/`getsysinfo`/`vm_info`/kerninfo 5 条通道）是下一篇 04
（`04-is-data-acquisition.md`）的前置，之后 05~10 逐个填体。04 是转储域篇
的数据面总闸。

---

## 7. 参见

- `01-is-init-main.md` §2.3/§2.5：classify TTY 分支 + 回复门（本篇调用方）
- `02-is-fkey-contract.md` §2.7/§3 D5：EVENTS 契约 + INIT_FKEYS 交接（已闭合）
- `04-is-data-acquisition.md`（待写）：5 条数据获取通道（`run_dump` 填体的前置）
- `draft/tmp_dmp.c.md` §hooks/`do_fkey_pressed`/`key_name`/`mapping_dmp` 段：行文底料
- plan §1.3（次主线）/§3.4（03 职责行）/§5.3（03 函数清单）
