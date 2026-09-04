# 05-is-dump-kernel：内核转储域

> **源码**：`minix3/minix/servers/is/dmp_kernel.c`（全 396 行，本文全覆盖）+
> `kernel/proc.h:22-82`（proc）/`:143-149`（RTS_*）/`:265-274`（ADDR 宏/
> `isemptyp`）+ `kernel/priv.h:21-61`（priv）+ `kernel/type.h:18-26`
> （irq_hook）+ `include/minix/param.h:14-54`（kinfo/boot_image）+
> `type.h:148-176`（boot_image/kmessages）+ `bitmap.h:12-15` +
> `const.h:143-150` + `com.h:48`（IDLE）/`:308`（IRQ_REENABLE）+
> `ipcconst.h:7-16` + `endpoint.h:54-55`（ANY/NONE）+ `sys_config.h:8-9`
> （_NR_PROCS 256）/`:22`（_KMESS 10000）+ `config.h:31-33` +
> `arch/i386/interrupt.h:35-37`（NR_IRQ_VECTORS 16）+
> `arch/earm/multiboot.h:240`（PARAM_BUF 1024）
> **Rust**：`os/servers/is/src/dump_kernel.rs`（新）
> **draft 素材**：`draft/tmp_dmp_kernel.c.md`（行文底料；Rust 块废弃同 01 §3.0）
> **位置**：DumpId 8 变体（Proctab/Image/Privileges/Monparams/Irqtab/
> Kmessages/Kenv/Procstack → 03 表）的实现域；取数通道见 04 §6 首行

---

## 1. 概念：为什么 kernel 转储占八席

### 1.1 问题：最大数据主权者的只读审计

> **目标读者**：读完 01~04 的读者。前置知识：04 五通道（直接使用）。
> 本章不讲 PM/VFS/RS/DS/VM 各表（06~10）与通道机制（04）。

kernel 持有最多的状态：进程表、特权表、系统镜像、启动参数、中断路由、
诊断消息——16 个转储里 8 个是它的。IS 对 kernel 做的事是**只读审计**：
每次转储先从 kernel 拷一份快照到本地（04 通道），再对快照做纯格式化，
绝不回写。审计员不动账本，这是微内核调试面的基本纪律。

### 1.2 类比：年检

把 IS 想象成年检员，kernel 是受检企业。八个转储是八个检查项：花名册
（proctab）、资质证（privileges）、注册表（image）、会议纪要本
（kmessages）、章程修正案（monparams）、门禁布线图（irqtab）、企业简介
（kenv）、员工访谈录（procstack）。年检员每项只看、只记、只贴告示
（warn），从不改账。22 行分页（`--more--`）是"一次只看一页纸"的阅卷纪律。

### 1.3 边界声明

**前置依赖**：04（9 通道：GET×8 + STACKTRACE + kerninfo）；03（DumpId
8 变体归属本篇）。

**本篇职责**：dmp_kernel.c 全部（8 dumps + 4 helpers + 宏）+ 6 布局 ABI
（子集快照）+ 分页格式契约。

**不覆盖（移交）**：dump 体执行（输出通道待 A-6，`run_dump` 续空）；
全布局权威 → kernel crate（本篇快照为 wire 契约提案，A-4）；
PM/VFS/RS/DS/VM 各表 → 06~10。

> **本章小结**：八转储 = 只读审计 + 位编码 + 22 行分页（§1.2 年检）。
> 下一章 §2 把三表、双宏、八函数、四个编码器逐行对应到 `dmp_kernel.c`。

---

## 2. C 源码分析

### 2.1 共享地基：三表、LINES、两宏、一死宏

```c
#define LINES 22                                                    /* :18 */

#define PRINTRTS(rp) { \                                            /* :20-28 */
	char *procname = "";	\
	printf(" %s", p_rts_flags_str(rp->p_rts_flags));	\
	if (rp->p_rts_flags & RTS_SENDING)				\
		procname = proc_name(_ENDPOINT_P(rp->p_sendto_e)); \
	else if (rp->p_rts_flags & RTS_RECEIVING)			\
		procname = proc_name(_ENDPOINT_P(rp->p_getfrom_e)); \
	printf(" %-7.7s", procname);	\
}

static int pagelines;

#define PROCLOOP(rp, oldrp) \                                       /* :32-40 */
	pagelines = 0; \
	for (rp = oldrp; rp < END_PROC_ADDR; rp++) { \
	  oldrp = BEG_PROC_ADDR; \
	  if (isemptyp(rp)) continue; \
	  if (++pagelines >= LINES) { oldrp = rp; printf("--more--\n"); break; }\
	  if (proc_nr(rp) == IDLE) 	printf("(%2d) ", proc_nr(rp));  \
	  else if (proc_nr(rp) < 0) 	printf("[%2d] ", proc_nr(rp)); 	\
	  else 				printf(" %2d  ", proc_nr(rp));

#define click_to_round_k(n) \                                      /* :42-43 */
	((unsigned) ((((unsigned long) (n) << CLICK_SHIFT) + 512) / 1024))
```

三张全局表（`:55-57`，与 kernel 同名以便复用 `proc.h` 宏——注释亲口承认，
`:51-54`）：`proc[NR_TASKS+NR_PROCS]`、`priv[NR_SYS_PROCS]`、
`image[NR_BOOT_PROCS]`。**注意**：`click_to_round_k` 全树仅定义、无调用
（A-8 死代码，`rg` 实证，§2.12）。`pagelines` 是真全局（非 static），但
同一时刻只有一个 dump 在跑（单线程），无竞争。

PROCLOOP 四段：计数器归零 → 空槽跳过（不计数）→ 满 22 行则**记录断点
`oldrp = rp` 并 break**（下轮从断点续跑——翻页游标的全部秘密）→ 行首三式
（IDLE=`(dd)`/负槽=`[dd]`/用户=` dd `，`IDLE=(endpoint_t)-4`，com.h:48）。

### 2.2 kmessages_dmp：环形展开（:62-88）

`get_minix_kerninfo()->kmessages`（:71，04 §2.3 链）→ 起算
`start = ((km_next + SIZE) - km_size) % SIZE`（:77）→ 拷贝循环 →
`print_buf[r] = 0` 封口（:85）→ 两 `printf`。静态打印缓冲
`print_buf[_KMESS_BUF_SIZE+1]`（:66，SIZE=10000，sys_config.h:22）是
"怕换行截断"的防御：环形回绕的原地打印会撕裂消息，先搬进线性缓冲再
一次性输出。

### 2.3 monparams_dmp：换行展开（:93-116）

`sys_getmonparams(val, sizeof(val))`（:101）→ `do/while` 把每个 NUL 改写
成 `\n`（:107-111，止于双 NUL）→ 标题 + `"\n%s\n"`。Rust 侧硬化为截断
（§3 D5）：C 用定长 1024 栈数组 + 原地改写，长度完全信任内核返回；
重写不继承该信任（`out` 满即停，注释存证）。

### 2.4 irqtab_dmp：双表对读（:121-163）

`sys_getirqhooks` + `sys_getirqactids` 双取（:129/:133，任一失败即返）→
`#if 0` 调试块（:138-143，**编译排除**，标注不移植）→ 表头两行 →
16 钩逐行：`<unused>`（`proc_nr_e==NONE`）或端点/IRQ 号/policy 词
（`IRQ_REENABLE 0x001` → "reenable" 否则 `"    -   "`）/notify id/
掩码判定（`irq_actids[irq] & id` → "masked"，:158-159）。
`NR_IRQ_HOOKS=16`（kernel/config.h:59，x86 档；:61 另有 64 的大配置档，
IS 按 16 编译）、`NR_IRQ_VECTORS=16`（i386 interrupt.h:35-37，同文件另有
64 档——Rust 取 16，x86-64 目标注释存证）。

### 2.5 image_dmp：名实不符的表头（:168-185）

表头 `"---name- -nr- flags -stack-\n"`（:179）承诺四列，行只打两列
（`"%8s %4d"` 名 + 号，:182）——**表头是谎言**（历史残留，无人修）。
Rust 格式常量原样保留谎言（行为兼容；注释点名，不"修复"表头——修了反而
与 C 输出漂移）。

### 2.6 kenv_dmp：取而不用（:191-213）

取 `kinfo` + `machine` 双份（:197/:201），**只打印 kinfo 四字段**
（nr_procs/nr_tasks/release/version，:209-211；`%.6s` 定宽）。`machine`
全程未读——不是"备用"，是历史冗余（取数不要钱时代的写法）。Rust 快照
无 machine（§3 D1 子集原则的实例：不用即不建模）。

### 2.7 三编码器：位→字符契约（A-12）

`s_flags_str`（:218-231）："PBDSIQM" 七位（PREEMPTIBLE 0x002/BILLABLE
0x004/DYN_PRIV_ID 0x008/SYS_PROC 0x010/CHECK_IO_PORT 0x020/CHECK_IRQ
0x040/CHECK_MEM 0x080，const.h:143-150）+ `\0`，`static char str[10]`。
`s_traps_str`（:236-247）："SARBN" 五位（`1<<SEND` 等，SEND=1/RECEIVE=2/
SENDREC=3/NOTIFY=4/SENDA=16，ipcconst.h:7-16）——**注意**：`s_trap_mask`
是 `short`，`1<<SENDA`（bit16）在 C 里靠整型提升**仅当掩码为负才命中**
（Rust 单测锁定该边角，`dump_kernel.rs` 注释）。
`p_rts_flags_str`（:300-313）："sSRIPTp" 七位（RTS_PROC_STOP 0x02…
RTS_NO_PRIV 0x80，proc.h:143-149）。三者各持**独立**静态缓冲（非共享——
`privileges_dmp` 的 `printf` 链连续调两次（:282-283）天然安全：各写各的
缓冲，且实参求值后立即消费；单线程下无任何覆盖窗口）。

### 2.8 privileges_dmp：回退与位图列（:252-295）

双取（privtab + proctab，:261/:265）→ 表头 → PROCLOOP 内：proc→priv
线性匹配（`s_proc_nr == p_nr`，:274-276）；**匹配失败回退**
`USER_PRIV_ID`（:277-279，"无 priv 的进程看平民权限"，static_priv_id
语义，priv.h:18）→ 行：`(s_id) 名 flags traps grants` + `s_ipc_to` 按
`BITCHUNK_BITS` 分块打 `%08x`（:284-286）+ `s_k_call_mask` 同式
（:289-291）。位图列是"每块 8 十六进制"的原始转储（无解析，审计员只
负责复印）。

### 2.9 proctab_dmp：双架构分支（:318-353）

`#if defined(__i386__)` 真体（:318-346）：取表 → 表头 → PROCLOOP 内打
端点（`_ENDPOINT_G` 世代 + 裸值）、名（`%-8.8s`）、优先级/量子/用户/系统
时间 → `PRINTRTS`（:342，含 from/to 名解析）→ 换行。`#if defined(__arm__)`
空体（:348-353，`"LSC FIXME: Not implemented for arm"`）——**A-7：x86-64
目标不移植**（标注排除）。

### 2.10 procstack_dmp：多一行的谜（:358-380）

结构同 proctab，但循环体内 `printf("\n"); pagelines++;`（:377）——打印行
之后**手动再计一行**（栈回溯输出占行，分页器不知道）。随后
`sys_diagctl_stacktrace(rp->p_endpoint)`（:378，04 §2.2）。`pagelines++`
的位置（`printf` 之后、stacktrace 之前）说明计数的是"已输出行"而非
"已处理表项"——Rust 游标以 `Emit` 计数天然对齐（§3 D3）。

### 2.11 proc_name：四规则（:385-395）

`ANY→"ANY"` / `NONE→"NONE"`（endpoint.h:54-55，"bogus"注释原文）/
`nr < -NR_TASKS || nr >= NR_PROCS`（-5/256）→`"BOGUS"` /
空槽→`"EMPTY"` / 否则表名。PRINTRTS 的 from/to 解析经此（:24/:26）。

### 2.12 明确排除

| 项 | 处理 | 依据 |
|---|---|---|
| arm 分支（:348-353） | A-7 不移植 | x86-64 目标 |
| `click_to_round_k`（:42-43） | A-8 死代码 | `rg` 全树仅定义 |
| glo.h 死 extern×5 | A-8 跳过 | 01 §1.3 延续，plan §5.4 |
| `#if 0` 调试块（:138-143） | 编译排除，不移植 | 预处理即死 |
| machine 布局 | 不建模（kenv 不用） | §2.6 |

---

## 3. Rust 设计决策

### 3.1 D1：Wire 快照（子集 + A-4 兼容方向）

六快照（§4.2）：`KProcSnap`（10 字段）/`KPrivSnap`（5）/`BootImageSnap`（2）/
`KinfoSnap`（4）/`KmessagesSnap`（2）/`IrqHookSnap`（5）。`#[repr(C)]` +
C 声明序 + 类型映射注记（§4.1 头注释）。`[ARCH: A-4]` 三处：本节 +
design D1 + 快照模块头 TODO（kernel 侧 GETINFO 生产者对齐待办）。
否决全镜像（指针/arch 段）与复用 KProcess（活体非线格式）。

### 3.2 D2：编码器→const fn 字节数组

三编码器返 `[u8;N]`（NUL 封口与 C 同形，便于 `grep` 对拍）。静态缓冲→
值语义（重入免费）。A-12 的 Display 演进：05~10 统一字节数组 + 格式化层
集中转 `Display`，本篇不定（注释存证，避免各篇自造格式）。

### 3.3 D3：分页→PageCursor（三实例隔离）

`push_row(rts_flags, nr) -> Skip/Emit/More` 纯状态机；`procstack` 的
`pagelines++` 之谜由"Emit 即计数"自然覆盖（调用方每输出一行调一次，
与 C 的手动++同构）。三 dump 各持一实例（C 三 static oldrp 的对应物；
全局静态→实例是可测性演进，注释存证）。行首三式→`RowHead` 枚举。

### 3.4 D4：环形→kmess_start

起算式原样（含 `%`）；拷贝循环归输出层（平凡搬运，不单列）。

### 3.5 D5：换行→expand（截断硬化）

C 越界赌注不继承：`out` 满即停（注释存证为 deliberate hardening）。
空串/无终止/截断三单测。

### 3.6 D6：proc_name→分类枚举

`NameClass` 五变体 + `classify(nr, is_empty)`；表查阅归 kernel 侧。

### 3.7 D7：体延后（A-6 延续）

8 体待输出通道；`run_dump` 续空；本篇交付格式常量 12 项（8 标题/列头 +
MORE/UNUSED/REENABLE/SIZE）逐字锁定。

---

## 4. 实现详解

### 4.1 模块树（增量）

```text
os/servers/is/src/dump_kernel.rs — 6 快照 + 3 编码器 + PageCursor/RowHead + kmess_start + expand_newlines + NameClass + 12 格式常量（D1-D7）
os/servers/is/src/lib.rs         — pub use PageAction/PageCursor（05 起消费）
```

### 4.2 关键签名（与 §3 一致，Gate D-5 依据）

```rust
// dump_kernel.rs
pub struct KProcSnap { p_rts_flags: u32, p_getfrom_e/sendo: i32, p_name: [u8;16], p_priority: i8, p_quantum_size_ms: u32, p_user/sys_time: i32, p_endpoint/p_nr: i32 }
pub struct KPrivSnap { s_proc_nr/s_id: i32, s_flags/s_trap_mask: i16, s_grant_entries: i32 }
pub struct BootImageSnap { proc_nr: i32, proc_name: [u8;16] }
pub struct KinfoSnap { nr_procs/nr_tasks: i32, release/version: [u8;6] }
pub struct KmessagesSnap { km_next/km_size: i32 }
pub struct IrqHookSnap { proc_nr_e/irq: i32, policy/notify_id: u32, id: i32 }
pub const fn s_flags_str(i16) -> [u8;8]; s_traps_str(i16) -> [u8;6]; p_rts_flags_str(u32) -> [u8;8];
pub const LINES: u32 = 22; MORE_MARKER: &str; RTS_SLOT_FREE: u32;
pub enum PageAction { Skip, Emit(RowHead), More } / RowHead { Idle, Task, User }
pub struct PageCursor; pub const fn push_row(&mut self, u32, i32) -> PageAction;
pub const fn kmess_start(i32, i32) -> i32;
pub fn expand_newlines(&[u8], &mut [u8]) -> usize;
pub enum NameClass { Any, None, Bogus, Empty, Named } + classify(i32, bool);
```

常量权威位置（§2.4g）：LINES/MORE/快照/编码器唯一定义于 `dump_kernel.rs`
（06~10 各自模块同体例，不交叉引用）。

### 4.3 关键不变量

1. 快照字段序 == C 声明序（子集相对序）。
2. 编码器输出 NUL 封口（与 C `str[N]` 同形）。
3. 游标 22 界 + 空槽不计数 + 三实例隔离。
4. `key_name` 宽度与本篇无关（03 已锁）。
5. 体执行待 A-6（`run_dump` 空体延续）。

---

## 5. 测试要点

| # | 场景 | 期望 | C 依据 |
|---|---|---|---|
| T1 | 三编码器零/全/单比特（含 SENDA 负掩码边角） | — | §2.7 |
| T2 | 游标 21Emit+More/空槽百跳/三实例隔离 | — | §2.1 |
| T3 | 行首三式（-4/-2/0/11） | — | §2.1 |
| T4 | 环形起算（含回绕/空） | — | §2.2 |
| T5 | 换行（含空/截断） | — | §2.3 |
| T6 | 分类矩阵 8 格（ANY/NONE/±界/空/名） | — | §2.11 |
| T7 | 格式串逐字 + 快照零构造 | — | §2.4/§2.5/§2.8/§2.9 |

### 5.3 测试统计（截至 2026-09-04）

- `cargo test -p minix-is`：**59 passed, 0 failed**（`dump_kernel` 新增 12）。
- `cargo test -p minix-types`：**122 passed**（无新增）。
- `cargo clippy/check -p minix-is`：本 crate 零警告。
- 完整清单：`rg "#\[test\]" os/servers/is/src/dump_kernel.rs`。

---

## 6. 过渡

kernel 域闭环（8 dumps + 布局 + 分页）。下一篇 06（`06-is-dump-pm.md`）
只取一条通道（getsysinfo SI_PROC_TAB）+ 一个布局（mproc）+ 时钟面
（getticks）——对照阅读：本篇是"九通道直连"，06 起是"单通道 + 对方布局"，
05~10 的体例从 06 开始收敛（布局 ABI + 分页格式 + 时钟/游标特例）。

---

## 7. 参见

- `04-is-data-acquisition.md` §2.1-§2.3/§6 首行：本篇 9 取数点
- `03-is-dump-dispatch.md`：DumpId 8 变体归属本篇
- `06-is-dump-pm.md`（待写）：单通道体例的下一站
- `../01-stage-kernel/28-usermapped-data.md`：A-3 上游
- `../01-stage-kernel/32-stack-tracing.md`：STACKTRACE 通道现状
- `draft/tmp_dmp_kernel.c.md`：行文底料
- plan §4（A-3/A-4/A-7/A-8/A-12）/§5.3（05 函数清单）
