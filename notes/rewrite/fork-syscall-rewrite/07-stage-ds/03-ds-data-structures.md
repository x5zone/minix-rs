# 03 — DS 数据结构：两张定长表和 192 字节的跨服契约

> **分类**: 数据模型 / 存储形状
> **源码**: `minix3/minix/servers/ds/store.h`（38 行）、`store.c:15-17,240-243`、`servers/is/dmp_ds.c:15-45`
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

每个条目（`struct data_store`，`store.h:16-29`）有四栏：

| 栏位 | 类型 | 存什么 | 谁用 |
|------|------|--------|------|
| `flags` | `int` | 在用位 + 类型臂 + 权限门 | 所有 handler 先看它（04 细讲门判） |
| `key[80]` | 字符数组 | 查找键；label 类型时存服务名 | 查找的依据 |
| `owner[80]` | 字符数组 | 发布者进程名；boot 映射填 `"rs"` | 权限的依据（05） |
| `u` | 联合 | 值：数字 / 内存块描述 /（label 复用数字道） | 读写的对象 |

联合（union）的意思是"三选一共用一块内存"：`u32`（4 字节数字）和 `mem`（指针+长度+容量，24 字节）叠在一起，实际用哪个看 `flags` 的类型臂。最容易误会的是 **label 没有自己的臂**：label 的端点值存在 `u32` 臂里（写入在 `store.c:241`，读回在 `dmp_ds.c:41`）。加 label 臂会引入第三种形状，但端点本来就是个数——复用数字道是最省的表示。

`mem` 的三栏（`store.h:23-27`）分工：`data` 是 DS 侧堆缓冲的指针，`length` 是有效长度（含字符串的结束符），`reallen` 是实际分配的长度（复用时 `length > reallen` 才重分配，`store.c:344-348`——小改小不动，省一次 `malloc`）。

### 1.4 订阅：四栏（正则待定）

每个订阅（`struct subscription`，`store.h:31-36`）也有四栏：`flags`（在用位 + 感兴趣的类型掩码）、`owner[80]`（订阅者名）、`regex`（编译后的正则，`^key$` 锚定）、`old_subs`（位图：哪些条目更新了还没取）。`old_subs` 一位对一条目，下标就是条目在表里的序号——"第 5 位置位"意思是"第 5 号条目有更新等你取"。

### 1.5 表为什么定长

```c
#define NR_DS_KEYS  (2 * NR_SYS_PROCS)   // 128，store.h:12
#define NR_DS_SUBS  (4 * NR_SYS_PROCS)   // 256，store.h:13
```

`NR_SYS_PROCS = 64`（`sys_config.h:9`）：全系统最多 64 个系统进程。平均一个服务占 2 个条目、4 个订阅位——这是经验数，不是算出来的。定长的真正原因是下一节：**表要原样拷给别的服务器读**，变长表做不到这一点。

空槽位的判定只有一句话：`!(flags & IN_USE)` 即空（`store.c:15-17`）。没有独立的"空闲链表"，没有计数器——"有没有旗"就是全部真相。

### 1.6 192 字节的跨服契约（A-10）

`do_getsysinfo` 把整个条目表**按原样**拷给调用者（`store.c:671`），而 IS 服务器的 `dmp_ds.c` 把收到的字节**直接按 `struct data_store` 解释**（`:30-41`：读 flags、key、owner、按类型读值）。两边没有任何版本号、没有解析器——**布局本身就是协议**。

算一下：4（flags）+ 80（key）+ 80（owner）+ 24（联合，8 对齐）= 188，按 8 对齐补到 **192**。于是：Rust 侧必须 `#[repr(C)]` 逐字节对齐，并用 `size_of == 192` 断言锁死，加三个偏移锁（key 在 4、owner 在 84、值在 168）。以后谁改了字段顺序，测试会先崩，而不是 IS 在生产环境静默读错——静默读错是最贵的 bug（没有错误码，只有错数据）。

和现代系统的对照：Linux 的 sysfs 用动态树 + 文本解析（灵活，但每次读都要解析）；seL4 用定长 capability 槽（和 DS 一样，定长换可预期）；Redox 用堆上变长名 + slab 槽（灵活，但跨进程要序列化）。DS 选的是"定长 + 直拷"：64 位小表场景下，这是拷贝成本最低、语义最简单的跨服快照。

### 1.7 小结

条目四栏（旗/键/主/值，label 寄数字道），订阅四栏（旗/主/式/图），表定长（128/256，空即无旗），布局是跨服契约（192 字节锁死）。下一站 04 讲"表有了，怎么找空位、怎么查"。

---

## 2 C 源码分析

### 2.1 容量推导（`store.h:11-13` + `sys_config.h:9`）

`_NR_SYS_PROCS = 64` → `NR_DS_KEYS = 2*64 = 128`，`NR_DS_SUBS = 4*64 = 256`。改配置数要 04（分配扫描上界）和 11（镜像总字节数）一起改——三处同源（§4.3 立约）。

### 2.2 条目体（`store.h:16-29`）

`flags`（`:17`）→ `key[80]`（`:18`）→ `owner[80]`（`:19`）→ 联合 `u`（`:21-28`：`u32`（`:22`）/ `data,length,reallen`（`:23-27`））。联合无 label 臂——label 寄数字道（§2.4）。

### 2.3 订阅体（`store.h:31-36`）

`flags`（`:32`）→ `owner[80]`（`:33`）→ `regex_t regex`（`:34`，式 deferred，A-2）→ `old_subs[BITMAP_CHUNKS(128)]`（`:35`，128 位图）。

### 2.4 标签寄数字道（`store.c:241` + `dmp_ds.c:38-41`，双向实证）

写入：`map_service` 把端点存进 `u.u32`（`:241`）；读出：IS 按 `LABEL` 类型读 `u.u32`（`dmp_ds.c:41`）。授受同道，无歧义。

### 2.5 镜像消费（`dmp_ds.c:15-45`，ABI 实证）

IS 用 `getsysinfo(DS, SI_DATA_STORE, buf, sizeof)` 取镜像（`:15`），逐槽读旗/键/主/值（`:30-41`）。注意 `:36-41` 的 STR 分支直接解引用 DS 的指针——在 IS 地址空间里那个指针是无效的，只能显示垃圾。这是消费者侧的已知事实（IS 重写时要修），不是 DS 的 bug，但写在这里提醒：**指针过镜像边界即失效**，这也是 Rust 侧堆指针只定"宽度"不定"语义"的原因（D5）。

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | 双体直译，名取字节数组 | `char[80]` | `DataEntry`（`store.rs`）+ `Subscription`（`subscription.rs`），名主取 `[u8; 80]` | 顺序即契约（02 的消息顺序即内存顺序）；名不用 `&str`——借用会让表的寿命系在借主身上，表必须自立 |
| D2 | 值用联合 | `union dsi_u` + label 寄数字道 | `DataBody` union（`u32` / `MemBody`，`#[repr(C)]`），label 无臂 | 加 label 臂等于发明第三种形状；用 `enum` 会改布局（判别字占空间，A-10 破） |
| D3 | 表定长，空即 `None` | 静态数组 + 无旗即空 | `DsStore = [Option<DataEntry>; 128]` / `DsSubs = [Option<Subscription>; 256]` | `None` 即空，类型即真相；`Vec` 会让界可违（界变则 IS 误读） |
| D4 | 位图复用 | `bitchunk_t old_subs[]` | `minix-types::Bitmap` 128 位（A-5） | 位运算有一源，不手写第二遍；容量随表（`Bitmap::new(128)`） |
| D5 | 堆指针只定宽度 | `void *data` + `malloc/free` | `MemBody { data: *mut u8, length, reallen }` + 所有权约（分配/释放在 07/09，A-3） | 镜像只要求指针占 8 字节，不要求现在就有分配器；`Vec<u8>` 会引入"堆谁建"的未决问题 |
| D6 | 布局等价锁死（A-10） | 192 字节天然成立 | `#[repr(C)]` + `size_of == 192` 断言 + 三偏移锁 | 偏离的代价是 IS 静默误读——不断言等于裸奔 |
| D7 | 正则式 deferred（A-2） | `regex_t` 槽内 | 存源文 `pattern` 栏（`subscription.rs`），引擎走 trait（10） | 编译后的正则没有 `no_std` 现成实现；臆造一个占位布局等于伪造契约。不如存源文，引擎到了即插 |

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
| 条目体 | `store.h:16-29` | `store.rs`（`DataEntry`） | 四栏顺序直译 |
| 值联合 | `store.h:21-28` | `store.rs`（`DataBody` / `MemBody`） | 数存共联，label 无臂 |
| 条目表 | `store.h:12` | `store.rs`（`DsStore`） | 128 定长，空即 `None` |
| 订阅体 | `store.h:31-36` | `subscription.rs`（`Subscription`） | 旗主式图（式存源文） |
| 订阅表 | `store.h:13` | `subscription.rs`（`DsSubs`） | 256 定长，空即 `None` |

### 4.3 不变量

| 不变量 | 守卫 | 证据 |
|--------|------|------|
| 空即无旗（空即 `None`） | `is_vacant` 双谓词 | `store.c:15-17` |
| 容量三处同源 | `NR_DS_KEYS/SUBS` 常量 + `Bitmap::new(128)` | `store.h:12-13` |
| 镜像 192 字节 | `size_of` 断言 + 偏移锁 | `dmp_ds.c:15-45` |
| label 走数字道 | 类型无 label 臂 | `store.c:241` |

---

## 5 测试要点

> 基线：`cargo test -p minix-ds --lib`，本篇 7 个测试。

| 测试名 | 覆盖 C 位置 | 行为 |
|--------|-------------|------|
| `test_entry_layout` | `store.h:16-29` | 192 框 + 三偏移 + 联合宽度 |
| `test_vacant_rule` | `store.c:15-17` | 空即 `None`（双表）+ 容量 |
| `test_label_lane` | `store.c:241` | label 端点经数字道往返 |
| `test_sub_table_capacity` | `store.h:13` | 256 定长 + 空即 `None` |
| `test_old_bitmap` | `store.h:35` | 128 位图置取清 |
| `test_flags_roundtrip` | `store.h:17,32` | 标志存取 |
| `test_key_owner_bytes` | `store.h:18-19` | 名主字节往返 |

---

## 6 过渡

表和布局讲完了。表是死的，得有人找空位、有人查——下一站 04（槽位分配与查找：三个取、一个放、三个查）。

## 7 参见

- C 源：`minix3/minix/servers/ds/store.h`、`store.c:15-17,240-243`、`servers/is/dmp_ds.c:15-45`
- 阶段文档：`02-ds-message-contract.md`（上一站）、`04-ds-slot-management.md`（下一站）、`11-ds-getsysinfo.md`（镜像的读者）
- Rust 实现：`os/servers/ds/src/store.rs`、`os/servers/ds/src/subscription.rs`
