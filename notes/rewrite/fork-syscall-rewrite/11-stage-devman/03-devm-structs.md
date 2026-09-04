# 03-devm-structs：核心数据结构与 wire 格式

> **定位**：全 stage 的底座。本篇回答：设备长什么样（`devman_device` 全字段）、状态机三态、事件与属性文件的结构、wire 上设备由哪些字节组成。只讲形状，不讲操作（树操作归 04，读写归 06，添删归 07/08）。
> **源码**：`minix3/minix/servers/devman/devman.h`（108 行，全部）+ `devman/devinfo.h`（35 行，全部）+ `minix/include/minix/devman.h:8-20`（wire 公共镜像）+ `lib/libdevman/generic.c:36-99`（`serialize_dev` 布局实证）。
> **Rust 模块**：`os/servers/devman/src/structs.rs`（状态/设备/事件/属性）+ `src/wire.rs`（解析）。
> **前置依赖**：02（inode 挂接：`FileBinding.ino` 即 02 的 `Ino`）。
> **不覆盖（移交）**：树操作（04）、消息面（05）、事件队列与读取（06）、添删改（07/08/09）、客户端构造（10/11）。

---

## 1. 概念：设备对象的三重身份

一个 devman 设备同时是三种东西——三种身份对应三组结构，混在一起读是本章最大的理解障碍，先拆开：

1. **树节点**（`devman_device`）：id、名字、状态、owner、父子链、引用计数——设备**是谁、在哪、归谁**。04 操作这组字段。
2. **文件集合**（`devman_inode` + infos + 属性）：每个设备在 VTreeFS 里表现为一个目录，目录下挂若干属性文件——设备**长什么样**（读出来是什么）。06 读这组字段。
3. **wire 消息**（`devman_device_info` + entries + 字符串区）：驱动在用户态构造设备描述，一次 grant 发过来——设备**怎么来的**（字节长什么样）。07/10 处理这组。

C 把三组塞进互相引用的三个头文件（server `devman.h`、server `devinfo.h`、公共 `minix/devman.h`），指针满天飞。Rust 侧按身份拆成 `Device`（身份 1+2 的静态部分）、`Attribute`（身份 2 的条目）、`wire::ParsedDevice`（身份 3 的解码结果）——同构但解耦。

### 1.1 状态机：三个整数，一个生命周期

`state` 取 0/1/2（`devman.h:89-91`，define 写在 struct 中间——合法 C，文件作用域）：UNBOUND（刚注册，谁也没绑）、BOUND（RS 握手完成，驱动已接管）、ZOMBIE（删了但还有人引用，08 详述）。转换只发生在三处：ADD 置 UNBOUND（07）、BIND 置 BOUND（09）、DEL 置 ZOMBIE（08，BOUND 才转）。本篇只锁定数值与含义，转换归各篇。

### 1.2 边界声明

本篇讲"每个字段是什么、为什么是这个类型、wire 字节怎么摆"。以下不展开：树怎么查（04）、事件怎么进出队列（06）、设备怎么添删（07/08）、驱动侧怎么构造（10/11）。

---

## 2. C 源码分析

### 2.1 `devman_device` 全字段（devman.h:82-107）

| 字段 | 类型 | 含义 | 归属篇 |
|---|---|---|---|
| `dev_id` | int | 设备号（根 0，分配自 `next_device_id`，04） | 04/07 |
| `name` | char* | 设备名（wire 解码填入；根永 NULL——BSS 的 accident，§2.5） | 07 |
| `ref_count` | int | 手动引用计数（get/put，08） | 08 |
| `major` | int | **只写不读**：仅 `device.c:193` 置 -1，全树 0 读者（grep 实证）→ Rust 省略（§3.3） | —（本篇标注删除） |
| `state` | int | 0/1/2 状态机（§1.1） | 07/08/09 |
| `owner` | endpoint_t | 拥有者端点（根初始化 0，device.c:195） | 09 |
| `inode` | devman_inode | 本设备的目录绑定（§2.3） | 04/06 |
| `parent` | 指针 | 父设备（根 NULL） | 04 |
| `info` | 指针 | 解析后的 wire 信息（07 填） | 07 |
| `siblings`/`children` | TAILQ | 兄弟链/孩子表 | 04 |
| `infos` | TAILQ_HEAD | 属性文件表 | 06/07 |

### 2.2 文件三件套（devman.h:61-80）

- `devman_inode`（:75-80）：`inode*`（框架节点，02 的 `Ino` 对应物）+ `read_fn`（函数指针，哪种文件怎么读，06 实现两种）+ `data`（私房数据，01 §2.3 的 cbdata 另一端）+ 链表链。
- `devman_static_info_inode`（:61-64）：`dev*` 回指 + `data[128]` 文本（属性文件内容，06 读它）。
- `devman_event`（:66-69）+ `devman_event_inode`（:71-73）：`data[128]` 事件文本 + 队列头（队列操作全归 06，本篇只锁结构）。
- `devman_read_fn` typedef（:53-54）：`(char*, size_t, off_t, void*) → ssize_t`——与 01 的 `ReadHookFn` 同形（buf/len/offset/cbdata→i64），01 的省略 inode 参数决策（01 §2.3/§4 已修）在此得到呼应：两种读函数都不需要 inode 指针。

### 2.3 常量与类型枚举

- `DEVMAN_STRING_LEN` 128（devman.h:42）：事件/属性文本上限（含 NUL）。
- `ADD_STRING` `"ADD "` / `REMOVE_STRING` `"REMOVE "`（devman.h:44-45）：事件行前缀——**定义在本篇，语义归 06**（事件格式），此处只登记。
- `enum devman_inode_type`（devman.h:47-51）：STATIC 0 / DYNAMIC 1 / DEVICE 2——wire entry 的 `type` 取值（§2.4）。
- `BUF_SIZE`（devman.h:39）：01 已收编，此处登记不重复。

### 2.7 同名双生：两个 `devman_dev`（布局不同，wire 才是真契约）

`rg struct devman_dev` 命中两处**不同定义**：服务端 `devinfo.h:5`的 `devman_dev`（`dev_id`/`parent_dev_id`/`name*`/`subsys*`/`data*`/`attrs`——解码目标，07 用）与客户端 `local.h:9` 的 `devman_dev`（`name[32]` 定长数组 + `bind_cb`/`unbind_cb` 回调 + `dev_list` 链——10 用）。同名不同形：两者永不在同一编译单元相遇（server 含 devinfo.h，lib 含 local.h），真正的接口是 wire 字节（§2.4）而非任一 struct。Rust 侧无此问题：`wire::ParsedDevice`（解码）与 10 的客户端类型（编码，10 建）各一名。03 登记此处，10 §2 会从客户端复述并指回本节。

### 2.4 wire 格式：头 + 条目 + 字符串区（devman.h:8-20 公共镜像 ≡ devinfo.h:21-33 服务端镜像）

两处定义逐字段相同（diff 空——公共头是契约，服务端镜像是为免 include 公共头的重复，注释无说明，属 C 组织债；Rust 只实现一份 `wire.rs`）。布局以 `serialize_dev`（generic.c:36-99）为实证：

```
头 16B：count i32 | parent_dev_id i32 | name_offset u32 | subsystem_offset u32
条目 ×count，每 16B：type u32 | name_offset u32 | data_offset u32 | req_nr u32
字符串区：NUL 结尾串，offset 从 buffer 起算
```

三个需特别处理的 wire 字段（细读 serializer 逐行确认）：

1. **`subsystem_offset` 从不写入**：`serialize_dev` 没有对它的赋值语句（generic.c:78-83 只写 count/parent/name 三项）——wire 上是 malloc 残留垃圾，server 从不读（device.c 零引用，grep 实证）。Rust 解码忽略、编码写 0（10 实现时）。
2. **`req_nr` 读写两端皆死**：serializer 无赋值（条目循环只写 type/name/data，generic.c:86-94），devman 域内零读取（`rg req_nr servers/devman/ lib/libdevman/ include/minix/devman.h` 唯一命中是 `devinfo.h:32` 定义本身；全树另有 mfs 的同名无关变量，已排除）——每条目 4 字节垃圾，wire 兼容保留。Rust 原样携带不解释。
3. **`type` 恒 0**：`entry->type = 0; /* TODO: use macro */`（generic.c:88）——客户端永远发 STATIC；DYNAMIC 只存在于枚举定义与 device.c:413 的 TODO fall-through（A-6，05/07）。

另有 `#if 0` 包住的 `bus` 字段（generic.c:80-83）：死代码，连编译都不进——03 登记，10 不实现。

### 2.5 根的半初始化：BSS 零值恰好是对的（device.c:187-207 读法）

`devman_init_devices` 只显式设 4 个字段（dev_id 0、major -1、owner 0、parent NULL）+ 两棵树 + 事件线。不设的（state、ref_count、name、info）靠 static 零初始化恰好是 UNBOUND/0/NULL/NULL——**正确的 accident**：将来有人把 root 改成栈上分配就会炸。Rust 的 `Device::root()` 把 6 项全显式写出（§3.3），并把 -1 的 major 删掉（§2.1）。

### 2.6 死结构与死宏（plan §5.5 实证复核）

- `devman_device_file`（`struct` 定义 devman.h:56-59）：`rg` 全树仅定义 1 处 → 死结构，不建模（代码注释挂名备查，防后人"补全"）。
- `DEVMAN_DEFAULT_MODE`（devman.h:41）：同上，死宏。
- `devman_dev.subsys`/`subsystem_offset`：wire 兼容字段（§2.4 坑 1），保留忽略 + 标注。

---

## 3. Rust 设计决策

### 3.1 三身份 → 三类型（§1 拆分落地）

`Device`（树节点 + 文件绑定）/ `Attribute`（属性条目，两处 C 结构统一：lib 侧 `static_attribute` 与 server 侧 `static_info_inode` 都是名值对，文件绑定归 06）/ `wire::ParsedDevice`（解码结果）。`Event` 独立（队列归 06，文本归 03，长度守卫在此）。

### 3.2 `DeviceState` 枚举 + 未知拒绝

C 存 int，比较全是 `== 0/1/2`。Rust `enum { Unbound=0, Bound=1, Zombie=2 }` + `from_i32`（未知 → EINVAL——C 会存下垃圾值继续跑，Rust 拒绝；非法状态不可表达）。

### 3.3 省略与显式化清单

| C | Rust | 分类 |
|---|---|---|
| `major`（只写不读） | 省略 | 死字段删除（grep 0 读者为证） |
| BSS 零值（state/ref/name/info） | `Device::root()` 全显式 | 显式化（accident→契约） |
| `TAILQ_ENTRY` 链 | 无（`Vec` 索引） | A-2（单线程所有权） |
| `char*` 名/串 | `String`/`Option<String>` | A-2（根名 None 显式化 BSS NULL） |
| `endpoint_t owner = 0` | `Option<Endpoint>`（None ≡ 0） | 类型收窄 |
| 双 wire 镜像头 | 一份 `wire.rs` | 去重（C 组织债不继承） |

### 3.4 wire 解析：显式边界检查（[ARCH:A-4]）

`parse_device` 对每个偏移做 `get()` 范围检查：短头/条目缺失 → Truncated；负 count/乘法溢出 → BadCount；串越界/无 NUL → BadOffset；非 UTF-8 → NonUtf8（C 是字节串，Minix 名实际 ASCII——假设写进注释，遇到再议）。调用方（07）统一映 EINVAL。`subsystem_offset` 跳过不读（§2.4 坑 1）。DYNAMIC 原样保留（07 判，A-6）。

### 3.5 长度守卫位置：`Event::new` 不静默截断

C `char[128]` 是硬截断的物理现实（snprintf 调用方各异，06 逐个核对）。`Event::new` 超长返回 `ENAMETOOLONG` 而非截断——截断是信息丢失，必须由调用方显式做（06 决定每处截断并注释），构造函数保持诚实。

---

## 4. 实现详解

### 4.1 模块结构

```
os/servers/devman/src/
  structs.rs — DeviceState / DeviceId / Attribute / Event / FileBinding / Device（+3 测试）
  wire.rs    — EntryType / WireEntry / ParsedDevice / parse_device（+4 测试）
```

### 4.2 关键不变量

1. `DeviceState` 数值 == C（单测锁定 0/1/2 + 非法拒绝）。
2. `Device::root()` 六项显式 == C BSS 语义（单测逐项）。
3. `Event` 文本恒 `< 128`（构造期守卫）。
4. `parse_device` 永不 panic（所有索引经 `get`，单测 4 类畸形全覆盖）。

### 4.3 与 C 的差异说明

| C | Rust | 分类 |
|---|---|---|
| 双头文件镜像 | 一份 wire.rs | 去重 |
| major 字段 | 省略 | 死字段删除（0 读者） |
| BSS 半初始化 | root() 全显式 | 显式化 |
| int state 可存任意值 | 未知 EINVAL | 非法不可表达 |
| device_file/DEFAULT_MODE | 注释挂名不建模 | 死代码标注 |
| 字节串 | UTF-8 String（NonUtf8→错） | 假设显式化（ASCII 实际） |

---

## 5. 测试要点

| 测试 | 断言 | 对应 |
|---|---|---|
| `state_values_match_c` | 0/1/2 + 非法 EINVAL | §4.2-1 |
| `root_shape_matches_c_bss` | 六项显式 | §4.2-2 |
| `event_text_capped` | 127 收/128 拒 | §4.2-3 |
| `entry_values_match_c` | 0/1/2 + 非法 EINVAL | §2.3 枚举 |
| `roundtrip_mirrors_serialize_dev` | 按 serializer 布局手编 buffer 解码全对 | §2.4 |
| `malformed_is_rejected` | 空/短头/负 count/坏 offset/缺条目 | §4.2-4 |
| `dynamic_type_preserved_for_07` | DYNAMIC 原样保留 | A-6 |

截至 2026-09-04：`cargo test -p minix-devman` **44 passed / 0 failed**（31 + 本篇 7：structs 3 + wire 4）。

---

## 6. 过渡

形状就绪：设备是什么（Device）、事件文本多长（Event）、wire 字节怎么摆（parse_device）。下一步 04 把这些形状**种进两棵树**：`Device` 进设备树（`DeviceTree::insert`），目录/文件进框架树（`tree.add`），并回答"给定 id，路径字符串是什么"（`generate_path`）。读 04 时若忘记 `binding` 是哪根线，回看 §2.2 的 `devman_inode` 三元组。

---

## 7. 参见

- `02-vtreefs-framework.md` — `Ino` 句柄与树存储（`binding` 的另一端）
- `04-device-tree.md` — 种树与寻路（本篇形状的消费者）
- `05-devm-message-contract.md` — 未实现消息与 DYNAMIC（A-6 正文）
- `06-event-buf.md` — 事件队列与 `read_fn` 实现（`Event`/`FileBinding` 的消费者）
- `07-devm-add-device.md` — wire 解码的调用方（`ParsedDevice` → `Device`）
- `10-libdevman-client.md` — `serialize_dev` 编码侧（本篇解码的镜像）
- C 源：`minix3/minix/servers/devman/devman.h`、`devinfo.h`、`minix3/minix/include/minix/devman.h:8-20`、`minix3/minix/lib/libdevman/generic.c:36-99`
