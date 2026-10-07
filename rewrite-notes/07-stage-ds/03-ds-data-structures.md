# 03 — DS 数据结构：两张定长表和一条跨服的布局契约

> **分类**: 数据模型 / 存储形状
> **源码**: `minix3/minix/servers/ds/store.h`（38 行）、`minix3/minix/servers/ds/store.c:alloc_data_slot`、`minix3/minix/servers/ds/store.c:map_service`、`minix3/minix/servers/ds/store.c:do_getsysinfo`、`minix3/minix/servers/is/dmp_ds.c:data_store_dmp`
> **Rust 实现**: `os/servers/ds/src/store.rs`, `os/servers/ds/src/subscription.rs`, `os/libs/minix-types/src/types/com.rs`, `os/libs/minix-types/src/types/bitmap.rs`, `os/libs/minix-types/src/types/ds_store.rs`, `os/servers/ds/src/getsysinfo.rs`, `os/servers/ds/src/server.rs`
> **说明**: DS 内存里只有两张表：128 个条目、256 个订阅。本文讲清每个字段的含义、表为什么定长，以及为什么条目的内存布局是跨服务器的契约（改一个字节，IS 就读错）。

---

## 1 概念

### 1.1 目标读者与前置知识

面向要读写 DS 表的读者。前置知识：02 的标志位（`IN_USE`、四个类型臂），C 结构体。联合（union）的概念在 1.3 现场解释。

### 1.2 本章不讲什么

- 槽位怎么分配和查找——那是 04 的事（本篇只讲"表长什么样"，不讲"怎么找空位"）。
- 名字和权限怎么判定——那是 05 的事。
- 字符串数据的堆内存怎么分配——那是 07/09 的事（本篇只定"堆指针放在哪"，不定"堆怎么管"）。

### 1.3 条目：四栏

每个条目（`struct data_store`，`minix3/minix/servers/ds/store.h:16-29`）有四栏：

| 栏位 | 类型 | 存什么 | 谁用 |
|------|------|--------|------|
| `flags` | `int` | 在用位 + 类型臂 + 权限门 | 所有 handler 先看它（04 细讲门判） |
| `key[80]` | 字符数组 | 查找键；label 类型时存服务名 | 查找的依据 |
| `owner[80]` | 字符数组 | 发布者进程名；boot 映射填 `"rs"` | 权限的依据（05） |
| `u` | 联合 | 值：数字 / 内存块描述 /（label 复用数字道） | 读写的对象 |

联合（union）的意思是"三选一共用一块内存"：`u32`（4 字节数字）和 `mem`（指针+长度+容量，24 字节）叠在一起，实际用哪个看 `flags` 的类型臂。最容易误会的是 **label 没有自己的臂**：label 的端点值存在 `u32` 臂里（写入在 `minix3/minix/servers/ds/store.c:map_service`，读回在 `minix3/minix/servers/is/dmp_ds.c:data_store_dmp` 的 `DSF_TYPE_LABEL` 分支）。加 label 臂会引入第三种形状，但端点本来就是个数——复用数字道是最省的表示。

`mem` 的三栏（`minix3/minix/servers/ds/store.h:23-27`）分工：`data` 是 DS 侧堆缓冲的指针，`length` 是有效长度（含字符串的结束符），`reallen` 是实际分配的长度（复用时 `length > reallen` 才重分配，见 `minix3/minix/servers/ds/store.c:do_publish`——小改小不动，省一次 `malloc`）。

### 1.4 订阅：四栏

每个订阅（`struct subscription`，`minix3/minix/servers/ds/store.h:31-36`）也有四栏：`flags`（在用位 + 感兴趣的类型掩码）、`owner[80]`（订阅者名）、`regex`（编译后的正则，`^key$` 锚定；它和槽同寿——槽释放时 `regfree`，见 `minix3/minix/servers/ds/store.c:free_sub_slot`）、`old_subs`（位图：哪些条目更新了还没取）。`old_subs` 一位对一条目，下标就是条目在表里的序号——"第 5 位置位"意思是"第 5 号条目有更新等你取"。

### 1.5 表为什么定长

```c
#define NR_DS_KEYS  (2 * NR_SYS_PROCS)   // 128，minix3/minix/servers/ds/store.h:12
#define NR_DS_SUBS  (4 * NR_SYS_PROCS)   // 256，minix3/minix/servers/ds/store.h:13
```

`NR_SYS_PROCS = 64`（`minix3/minix/include/minix/config.h:NR_SYS_PROCS` 是别名，值定义在 `minix3/minix/include/minix/sys_config.h:_NR_SYS_PROCS`）：全系统最多 64 个系统进程。平均一个服务占 2 个条目、4 个订阅位——这是经验数，不是算出来的。定长的真正原因是下一节：**表要原样拷给别的服务器读**，变长表做不到这一点。

空槽位的判定只有一句话：`!(flags & IN_USE)` 即空（`minix3/minix/servers/ds/store.c:alloc_data_slot`）。没有独立的"空闲链表"，没有计数器——"有没有旗"就是全部真相。

### 1.6 布局就是跨服契约（A-10）

`do_getsysinfo` 把整个条目表交给调用者（`minix3/minix/servers/ds/store.c:do_getsysinfo`），而 IS 服务器的 `dmp_ds.c` 把收到的字节**直接按 `struct data_store` 解释**（`minix3/minix/servers/is/dmp_ds.c:data_store_dmp`：读 flags、key、owner、按类型读值）。两边没有任何版本号、没有解析器——**布局本身就是协议**。

先算进程内的条目：4（flags）+ 80（key）+ 80（owner）+ 24（联合，8 对齐）= 188，按 8 对齐补到 **192**。Rust 侧的条目类型按 `#[repr(C)]` 逐字节对齐这个形状，再用 `size_of == 192` 加三个偏移锁（key 在 4、owner 在 84、值在 168）把它钉住：谁改了字段顺序，测试先崩，而不是同一进程内的读者静默读错。

跨服的那一份要再想一步：联合的宽臂里放的是指针，指针只在产生它的地址空间里有意义——IS 拿到它再解引用只会得到垃圾（C 的 STR 分支正是这么打的）。所以 Rust 交出去的镜像行不是 192 字节的条目，而是 **168 字节的标量面快照**：flags + key + owner + 一个标量槽（U32/LABEL 取数字臂，STR/MEM 取长度），宽度由 `os/servers/ds/src/getsysinfo.rs:image_bytes` 给出，生产方是 `os/servers/ds/src/server.rs:render_image`。这是一次架构演进（`[ARCH: A-4]`，与 `11-ds-getsysinfo.md` 的标法一致）：进程内布局仍与 C 等价（192），跨服面有意收窄成"读者能解释的字节"（168）。两份宽度各有守卫——内部的靠 `size_of` 断言，跨服的靠生产方与消费方同取一个算式。

和现代系统的对照：Linux 的 sysfs 用动态树 + 文本解析（灵活，但每次读都要解析）；seL4 用定长 capability 槽（和 DS 一样，定长换可预期）；Redox 用堆上变长名 + slab 槽（灵活，但跨进程要序列化）。DS 选的是"定长 + 直拷"：64 位小表场景下，这是拷贝成本最低、语义最简单的跨服快照。

### 1.7 小结

条目四栏（旗/键/主/值，label 寄数字道），订阅四栏（旗/主/式/图），表定长（128/256，空即无旗），布局是跨服契约（内部条目 192 字节锁死，跨服镜像行 168 字节）。下一站 04 讲"表有了，怎么找空位、怎么查"。

---

## 2 C 源码分析

### 2.1 容量推导（`minix3/minix/servers/ds/store.h:11-13` + `minix3/minix/include/minix/config.h:NR_SYS_PROCS` + `minix3/minix/include/minix/sys_config.h:_NR_SYS_PROCS`）

`_NR_SYS_PROCS = 64`（`NR_SYS_PROCS` 是它在 `minix3/minix/include/minix/config.h:32` 的别名）→ `NR_DS_KEYS = 2*64 = 128`，`NR_DS_SUBS = 4*64 = 256`。改配置数要 04（分配扫描上界）和 11（镜像总字节数）一起改——三处同源（§4.3 立约）。

### 2.2 条目体（`minix3/minix/servers/ds/store.h:16-29`）

`flags`（`:17`）→ `key[80]`（`:18`）→ `owner[80]`（`:19`）→ 联合 `u`（`:21-28`：`u32`（`:22`）/ `data,length,reallen`（`:23-27`））。联合无 label 臂——label 寄数字道（§2.4）。

### 2.3 订阅体（`minix3/minix/servers/ds/store.h:31-36`）

`flags`（`:32`）→ `owner[80]`（`:33`）→ `regex_t regex`（`:34`，式 deferred，A-2）→ `old_subs[BITMAP_CHUNKS(128)]`（`:35`，128 位图）。

### 2.4 标签寄数字道（`minix3/minix/servers/ds/store.c:map_service` + `minix3/minix/servers/is/dmp_ds.c:data_store_dmp`，双向实证）

写入：`map_service` 把端点存进 `dsp->u.u32`；读出：`data_store_dmp` 在 `DSF_TYPE_LABEL` 分支按数字读 `p->u.u32`。授受同道，无歧义。

### 2.5 镜像消费（`minix3/minix/servers/is/dmp_ds.c:data_store_dmp`，ABI 实证）

IS 用 `getsysinfo(DS, SI_DATA_STORE, buf, sizeof)` 取镜像，逐槽读旗/键/主/值（`:24-40`）。注意 STR 分支（`:32-34`）直接解引用 DS 的指针——在 IS 地址空间里那个指针是无效的，只能显示垃圾。这是消费者侧的已知事实（IS 重写时要修），不是 DS 的 bug，但写在这里提醒：**指针过镜像边界即失效**，这也是 Rust 侧堆指针只定"宽度"不定"语义"的原因（D5）。

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | 双体直译，名取字节数组 | `char[80]` | `DataEntry`（`store.rs`）+ `Subscription`（`subscription.rs`），名主取 `[u8; 80]` | 顺序即契约（02 的消息顺序即内存顺序）；名不用 `&str`——借用会让表的寿命系在借主身上，表必须自立 |
| D2 | 值用联合 | `union dsi_u` + label 寄数字道 | `DataBody` union（`u32` / `MemBody`，`#[repr(C)]`），label 无臂 | 加 label 臂等于发明第三种形状；用 `enum` 会改布局（判别字占空间，A-10 破） |
| D3 | 表定长，空即 `None` | 静态数组 + 无旗即空 | `DsStore = [Option<DataEntry>; 128]` / `DsSubs = [Option<Subscription>; 256]` | `None` 即空，类型即真相；`Vec` 会让界可违（界变则 IS 误读） |
| D4 | 位图复用 | `bitchunk_t old_subs[]` | `minix-types::Bitmap` 128 位（A-5） | 位运算有一源，不手写第二遍；容量随表（`Bitmap::new(128)`） |
| D5 | 堆指针只定宽度 | `void *data` + `malloc/free` | `MemBody { data: *mut u8, length, reallen }` + 所有权约（分配/释放在 07/09，A-3） | 镜像只要求指针占 8 字节，不要求现在就有分配器；`Vec<u8>` 会引入"堆谁建"的未决问题 |
| D6 | 进程内条目布局等价锁死（A-10） | 192 字节天然成立 | `#[repr(C)]` + `size_of == 192` 断言 + 三偏移锁 | 同进程内的读者按字节解释这张表，字段顺序就是协议；不断言等于裸奔 |
| D7 | 正则式存源文（A-2） | `regex_t` 槽内 | 存源文 `pattern` 栏（`subscription.rs`），匹配期经 `EreMatcher` 现解析（`pattern.rs`，10） | 编译后的正则没有 `no_std` 现成实现；存源文让订阅表零引擎生命周期（无 `regfree` 对应物），引擎按全匹配语义现解析——源文栏即 C 锚定语义的完整陈述 |
| D8 | 跨服镜像行收窄到标量面（`[ARCH: A-4]`） | 原样拷 `struct data_store`（192×128） | 镜像行取标量面快照（168 = flags + key + owner + 标量槽）；生产方 `os/servers/ds/src/server.rs:render_image`，宽度 `os/servers/ds/src/getsysinfo.rs:image_bytes` | 宽臂的指针跨进程无意义（IS 解引用只会得到垃圾）；把指针挡在 wire 之外，镜像只带读者能解释的字节——内部 192 不变，跨服面 168 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/ds/src/
├── store.rs        — 本篇：DataEntry / DataBody / MemBody / DsStore
├── subscription.rs — 本篇：Subscription / DsSubs
os/libs/minix-types/src/
├── types/com.rs    — DsFlags（02 的旗面）
└── types/bitmap.rs — Bitmap（位图一源）
```

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 条目体 | `minix3/minix/servers/ds/store.h:16-29` | `store.rs`（`DataEntry`） | 四栏顺序直译 |
| 值联合 | `minix3/minix/servers/ds/store.h:21-28` | `store.rs`（`DataBody` / `MemBody`） | 数存共联，label 无臂 |
| 条目表 | `minix3/minix/servers/ds/store.h:12` | `store.rs`（`DsStore`） | 128 定长，空即 `None` |
| 订阅体 | `minix3/minix/servers/ds/store.h:31-36` | `subscription.rs`（`Subscription`） | 旗主式图（式存源文） |
| 订阅表 | `minix3/minix/servers/ds/store.h:13` | `subscription.rs`（`DsSubs`） | 256 定长，空即 `None` |
| 镜像行 | `minix3/minix/servers/ds/store.h:16-29`（C 侧整结构外借） | `os/libs/minix-types/src/types/ds_store.rs`（`DsEntrySnap`） | 168 字节标量面（`[ARCH: A-4]`，指针不过 wire） |

### 4.3 不变量

| 不变量 | 守卫 | 证据 |
|--------|------|------|
| 空即无旗（空即 `None`） | `is_vacant` 双谓词 + 查表侧 `Some ∧ !is_vacant` | `minix3/minix/servers/ds/store.c:alloc_data_slot` |
| 容量同源 | `NR_DS_KEYS` / `NR_DS_SUBS` 常量 + `Bitmap::new(NR_DS_KEYS)`（位图容量随条目数） | `minix3/minix/servers/ds/store.h:12-13,35` |
| 进程内条目 192 字节 | `size_of` 断言 + 三偏移锁 | `minix3/minix/servers/ds/store.h:16-29` |
| 跨服镜像行 168 字节 | `image_bytes()` 与消费方同取一算式 | `minix3/minix/servers/is/dmp_ds.c:data_store_dmp` + `os/servers/ds/src/getsysinfo.rs:image_bytes` |
| label 走数字道 | 类型无 label 臂 | `minix3/minix/servers/ds/store.c:map_service` |

---

## 5 测试要点

> 基线：`cargo test -p minix-ds --lib`，本篇 7 个测试。

| 测试名 | 覆盖 C 位置 | 行为 |
|--------|-------------|------|
| `test_entry_layout` | `minix3/minix/servers/ds/store.h:16-29` | 192 框 + 三偏移 + 联合宽度 |
| `test_vacant_rule` | `minix3/minix/servers/ds/store.c:alloc_data_slot` | 空即 `None`（条目表）+ 表长 |
| `test_label_lane` | `minix3/minix/servers/ds/store.h:21-28` | 联合窄臂的写入与读回（label 端点的承载臂）；端到端往返在 06/07 篇的发布路径 |
| `test_sub_table_capacity` | `minix3/minix/servers/ds/store.h:13` | 256 定长 + 空即 `None` |
| `test_old_bitmap` | `minix3/minix/servers/ds/store.h:35` | 128 位图置取清 |
| `test_flags_roundtrip` | `minix3/minix/servers/ds/store.h:17,32` | 标志存取 |
| `test_key_owner_bytes` | `minix3/minix/servers/ds/store.h:18-19` | 名主字节往返 |

---

## 6 过渡

表和布局讲完了。表是死的，得有人找空位、有人查——下一站 04（槽位分配与查找：三个取、一个放、三个查）。

## 7 参见

- C 源：`minix3/minix/servers/ds/store.h`、`minix3/minix/servers/ds/store.c:alloc_data_slot`、`minix3/minix/servers/ds/store.c:map_service`、`minix3/minix/servers/ds/store.c:do_getsysinfo`、`minix3/minix/servers/is/dmp_ds.c:data_store_dmp`
- 阶段文档：`02-ds-message-contract.md`（上一站）、`04-ds-slot-management.md`（下一站）、`11-ds-getsysinfo.md`（镜像的读者）
- Rust 实现：`os/servers/ds/src/store.rs`、`os/servers/ds/src/subscription.rs`、`os/libs/minix-types/src/types/ds_store.rs`（镜像行）、`os/servers/ds/src/getsysinfo.rs`（镜像宽度）
