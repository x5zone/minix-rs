# 04-device-tree：根、查找与寻路

> **定位**：设备树的三种操作。本篇回答：树怎么出生（`devman_init_devices`）、给定 id 怎么找到设备（DFS）、给定设备路径字符串怎么写（`generate_path` 的 `./` 前缀与尾斜杠从哪来）。
> **源码**：`minix3/minix/servers/devman/device.c:16-39`（静态区）、`:45-70`（`devman_generate_path`）、`:187-207`（`devman_init_devices`）、`:283-308`（`_find_dev`/`devman_find_device`）。
> **Rust 模块**：`os/servers/devman/src/device_tree.rs`（`DeviceTree` + 默认 stat +寻路）。
> **前置依赖**：03（`Device` 形状）、02（框架树 `add`/`name`）、01（`FirstGuard` 守卫语义）。
> **不覆盖（移交）**：事件队列与读取（06）、添删改业务（07/08）、绑定（09）。

---

## 1. 概念：两棵树，一次出生

devman 维护**两棵**树，别混成一棵：

1. **VTreeFS 框架树**（02 的 `InodeTree`）：VFS 看到的文件——`/` 下挂 `devices/` 目录与 `events` 文件，往后每个设备一个子目录。这是**外表**。
2. **设备对象树**（本篇的 `DeviceTree`）：`Device` 结构体父子链，管 id/状态/owner。这是**实质**。

两棵树用 `binding.ino` 缝合：每个 `Device` 记住自己在框架树里的编号。C 用裸指针（`devman_inode.inode` 指向框架 `inode`），Rust 用 `Ino` 句柄——指针会悬空（删节点后），句柄只会查无（`find` 返 `None`），这是 02 §3.3 同款"句柄代替指针"的第二次应用。

出生只有一次：`init_hook`（mount 触发，01 §2.2）调 `devman_init_devices`，建根 + 顶层两文件。01 的 `FirstGuard` 保证只生一次——框架每 mount 调一次钩子，守卫把重复出生拦掉。两篇的分工：01 管"只一次"，本篇管"生什么"。

### 1.1 边界声明

本篇讲出生、查找、寻路三操作。不讲：事件文本格式（06）、设备怎么加进来（07 的 `add_child` 调本篇的 `insert`）、怎么删（08 调本篇的字段）、路径被谁消费（06 产、13 吃）。

---

## 2. C 源码分析

### 2.1 静态区：6 个全局 + 2 套默认属性（device.c:16-39）

| 符号 | 行 | 含义 |
|---|---|---|
| `next_device_id = 1` | :16 | 下一个设备号（根占 0，见 §2.2） |
| `default_dir_stat` | :18-24 | 目录默认属性（S_IFDIR\|0444，01 已验） |
| `default_file_stat` | :25-31 | 文件默认属性（S_IFREG\|0444，**size 0x1000**——目录是 0，文件是 4096，静态信息文件对外报告一页，06 依赖） |
| `root_dev` | :35 | 根设备对象（半初始化，§2.2） |
| `event_inode_data` | :36-38 | 事件队列头（静态初始化宏，06 拥有语义） |
| `event_inode` | :39 | events 文件绑定（`data` + `read_fn` 在 §2.2 接线） |

本篇操作的 C 结构为 `devman_device` / `devman_inode` / `devman_event`（全字段见 03；`devman_dev` 系客户端结构，见 03 §2.7，归 10），常量 `BUF_SIZE` 见 01（本篇预算另用 `DEVMAN_STRING_LEN`，§2.4）。

### 2.2 出生：`devman_init_devices`（device.c:187-207）

```c
event_inode.data    =  &event_inode_data;
event_inode.read_fn =  devman_event_read;

root_dev.dev_id =    0;
root_dev.major  =   -1;
root_dev.owner  =    0;
root_dev.parent = NULL;

root_dev.inode.inode =
    add_inode(get_root_inode(), "devices",
        NO_INDEX, &default_dir_stat, 0, &root_dev.inode);

event_inode.inode =
    add_inode(get_root_inode(), "events",
        NO_INDEX, &default_file_stat, 0, &event_inode);

TAILQ_INIT(&root_dev.children);
TAILQ_INIT(&root_dev.infos);
```

四点解读。第一，事件线先接：`event_inode` 的 `data` 指向队列、`read_fn` 指向 `devman_event_read`（06 实现）——events 文件出生即能读（读出空，队列空）。第二，根只设 4 字段（03 §2.5 已剖析 BSS accident：state/ref/name/info 全靠零值）。第三，两次 `add_inode` 都在 VFS 根下、都传 NO_INDEX（02 §2.3 的第 1、2 个证据；第 3、4 个在 07）。第四，`add_inode` 的返回值（框架节点指针）直接存进 `root_dev.inode.inode`——这就是 §1 的"缝合"，C 用指针，Rust 用 `Ino`。

失败路径：无。两次 `add_inode` 的名字都短（"devices"/"events" < 24），唯一 NULL 路径（长名 malloc 失败，02 §2.3）走不到——C 不检查返回值在这里是**安全的巧合**（名字是字面量，长度编译期已知）。Rust 的 `new()` 照样返回 `Result`（统一签名，失败分支不可达——注释写明，不留"不可能失败所以 unwrap"的潇洒）。

### 2.3 查找：DFS 先序，孩子按序（device.c:283-308）

```c
static struct devman_device *
_find_dev(struct devman_device *dev, int dev_id)
{
    if(dev->dev_id == dev_id)
        return dev;
    TAILQ_FOREACH(_dev, &dev->children, siblings) {
        struct devman_device *t = _find_dev(_dev, dev_id);
        if (t != NULL)
            return t;
    }
    return NULL;
}

struct devman_device *devman_find_device(int dev_id)
{
    return _find_dev(&root_dev, dev_id);
}
```

先序（自己先比，再依次递归孩子），找不到返 NULL。包装函数存在的唯一理由是 C 递归需要游标（从 `&root_dev` 开始）——Rust 无递归游标，`find` 与 `find_device` 同体（§3.3 记录这层"包装蒸发"）。复杂度 O(n)，n ≤ 1024——C 没建 id 索引表（dev_id 查找每次全遍历，设备上千才疼，devman 场景无感），Rust 同样不建（§3.3：索引是过早优化，且双索引一致性是 bug 温床，02 §3.2 同款论证）。

### 2.4 寻路：递归拼串，预算检查（device.c:45-70）

```c
static int
devman_generate_path(char* buf, int len, struct devman_device *dev)
{
    int res = 0;
    const char * name = ".";
    const char * sep = "/";

    if (dev != NULL) {
        res = devman_generate_path(buf, len, dev->parent);
        if (res != 0) return res;
        name = get_inode_name(dev->inode.inode);
    } else {
    }

    /* does it fit? */
    if (strlen(buf) + strlen(name) + strlen(sep) + 1 > len) {
        return ENOMEM;
    }

    strcat(buf, name);
    strcat(buf, sep);
    return 0;
}
```

执行走读（以 usb 设备的子设备为例）：`gen(dev)` → 递归 `gen(parent=root)` → 递归 `gen(NULL)`（根的 parent）→ dev==NULL 走 `else {}`（**空 else**，L60-61，纯造型代码）→ name="." → 检查 → buf="./" → 回到根层：name="devices"（inode 名）→ buf="./devices/" → 回到设备层：buf="./devices/usb/"。**路径自带 `./` 前缀与尾斜杠**——这不是 bug，是事件行的实际格式（06 的 `"ADD ./devices/usb/ 0x%08x"`，13 按此解析）。实现必须逐字节复刻（单测锁三串）。

 预算公式：`len(buf)+len(name)+len("/")+1 > budget → ENOMEM`（+1 是 NUL 位，与 BUF_SIZE 的 +1 同款思维，01 §2.1）。`budget` 即 C 的 `len` 形参：裸路径传 128，事件行传 `128-11`（device.c:92/119 两处生产者实证——11 是尾缀 `" 0x%08x"` 的长度：空格 + `0x` + 8 hex，`char buf[12]` 装它，device.c:78/105）。Rust 签名保留 `budget` 参数（与 C 同形，不固定 128——固定了反而在 117~127 路径段产生分歧，审查中纠正，见 04 scan P0-1）。

注意名字来源：`get_inode_name(dev->inode.inode)`——**框架 inode 名**，不是 `dev->name`（wire 名，07 填）。两者正常一致（07 建目录时用 wire 名），但类型上是两个来源——Rust 的 `generate_path` 同样读框架树（`framework.name(ino)`），保持"路径即文件树位置"的语义（万一 wire 名与目录名分叉，路径跟文件走——与 C 逐行一致）。

---

## 3. Rust 设计决策

### 3.1 双树缝合：`binding: Option<Ino>`

`Device.binding`（03 定义）由本篇填：根在 `new()` 里填，孩子在 07 里填（07 调框架 `add` 拿 `Ino` 再调本篇 `insert`）。`None` 只存在于"对象已造、还没种进框架"的瞬间——07 的构造顺序保证 `insert` 前绑定已就绪（07 §4 会断言这条）。

### 3.2 包装蒸发与索引不建（§2.3 两结论的代码落点）

`find` == `find_device`（同一方法，注释引 C 双函数行号，防后人"补包装"）。迭代版先序（显式栈，逆序压孩子保证弹出顺序 == C 递归顺序——单测 DFS 断言锁顺序）。无 id 索引（O(n) ≤ 1024，§2.3 论证）。

### 3.3 id 分配：`alloc_id` 顺序 + 溢出 ENOMEM（[ARCH:A-5]）

`next_id` 起 1（C :16），`insert` 校验稠密（`id == len`——07 按序分配，错位即调用方 bug，EINVAL）。`u32::MAX` 上溢 → `ENOMEM`（C 无界自增， wraps 未定义；`checked_add` 显式化）。

### 3.4 `insert` 的归属： linking 归树，校验归业务，线画在 id 上

`insert(parent, dev)` 只做三校验（parent 对得上、id 稠密、父存在）+ 链入。名字合法性、wire 解析、属性挂载全是 07 的——`insert` 不看 `name` 内容（只在测试 helper 里用）。这条线保证 04 可独立测试（本篇测试全用 `insert_dir` 自举，不依赖 07）。

---

## 4. 实现详解

### 4.1 模块结构

```
os/servers/devman/src/device_tree.rs — DeviceTree / default_dir_stat / default_file_stat（+6 测试）
```

### 4.2 关键不变量

1. `devices[i].id == i`（稠密，永不复用——C 亦然）。
2. 根 id 0、parent None、无名（`name: None`，C BSS NULL 显式化）。
3. `generate_path(root) == "./devices/"`（字节级锁定）。
4. 预算公式与 C 同形（`+1+1 > budget → ENOMEM`，budget 参数化）。

### 4.3 与 C 的差异说明

| C | Rust | 分类 |
|---|---|---|
| 递归 DFS | 显式栈迭代（同序） | 设计决策（等价，防深递归直觉性质疑—— depth ≤ 1024 其实两者都安全，迭代纯为可读） |
| `find`/`find_device` 双函数 | 单方法 | 包装蒸发（注释引双行号） |
| `next_device_id++` 无界 | `checked_add` + ENOMEM | A-5 |
| BSS 半初始化根 | `Device::root()` 全显式（03） | 显式化 |
| `dev == NULL` 空 else | `None => "./"` 分支（有实义） | 显式化（空 else 删除） |
| 裸指针缝合 | `Option<Ino>` 句柄 | 句柄化（02 同款） |

---

## 5. 测试要点

| 测试 | 断言 | 对应 |
|---|---|---|
| `init_builds_root_and_top_files` | 根 + devices/events 落框架树 + 根形状 | §2.2 |
| `default_stats_match_c` | dir 0444/size 0；file 0444/size 0x1000 | §2.1 |
| `find_is_dfs_preorder` | 先序 + 失踪 None + lookup_child | §2.3 |
| `generate_path_reproduces_c_strings` | 三串字节级（`./devices/`/`./devices/usb/`/`./devices/usb/0/`） | §2.4 |
| `generate_path_budget_is_enomem` | 20 层深链 ENOMEM | §2.4 预算 |
| `generate_path_event_budget_117` | 119 字符路径：128 收 / 117 拒 | §2.4 -11 实证 |
| `alloc_id_sequence_and_mismatch_rejected` | 1,2 顺序 + 错位 EINVAL | §3.3 |

截至 2026-09-04：`cargo test -p minix-devman` **45 passed / 0 failed**（31 + 03 的 7 + 本篇 7）。

---

## 6. 过渡

树已种好：根在 `./devices/`，查找是先序，寻路逐字节复刻。但树里的节点还是空壳——`attrs` 是空的（属性文件归 07 挂），`info` 是 None（wire 解析归 07 填），事件队列还不存在（06 建）。下一站 05 先把消息面钉死（ADD 长什么样、谁有资格发），然后 06 给树接上事件流（ADD 行怎么写进 events 文件），07 再把三者串成添加全流程。读 07 时若忘记 `insert` 的三校验，回看 §3.4 的画线。

---

## 7. 参见

- `03-devm-structs.md` — `Device` 形状（本篇操作的对象）
- `02-vtreefs-framework.md` — 框架 `add`/`name`（本篇种树用的铲子）
- `05-devm-message-contract.md` — ADD 消息形状（`insert` 的调用方）
- `06-event-buf.md` — 事件行格式（`generate_path` 的消费者）
- `07-devm-add-device.md` — `insert` 的调用方 + wire→Device 构造
- C 源：`minix3/minix/servers/devman/device.c:16-70,187-207,283-308`
