# 08-devm-del-device：删除、引用计数与回收

> **定位**：设备生命的终点。本篇回答：DEL 怎么走（找→广播→改态→放引用）、引用计数四函数、回收八步、ZOMBIE 何时出现、多子多绑的账怎么平。
> **源码**：`device.c:424-455`（`do_del_device`）+ `:460-480`（get/put）+ `:485-515`（`del_device`）。
> **Rust 模块**：`os/servers/devman/src/del_device.rs`（`do_del` + `get/put/del_device`）+ 02/03/04/06 四处 additive 扩展（见 §4.4）。
> **前置依赖**：04（树删改原语）、06（REMOVE 行与文件注销）、07（refcount 出生值 1）、05（ENODEV 回复）。
> **不覆盖（移交）**：绑定态转换细节（09 拥有 BOUND 的进出）、事件消费（13）。

---

## 1. 概念：删除是"广播先行，回收随后"

DEL 的顺序初看反直觉：**先广播 REMOVE，再改状态，最后才放引用**（device.c:441-448）。为什么广播不等回收完？因为广播的内容是路径——回收后路径就没了（框架节点已删，`generate_path` 查无）。顺序是数据的依赖关系逼出来的：先把需要"活树"的东西（路径行）做完，再拆树。这也是 07 先落户后广播的镜像（07 §1：顺序不能换）。

ZOMBIE 是这套顺序的 corner：设备 BOUND 着（驱动拿着引用），DEL 来了——不能真删（驱动还指着它），也不能当没看见（REMOVE 已广播，devmand 可能已停驱动）。于是置 ZOMBIE："名分已除，肉身暂留"，最后一个引用放掉才回收（`put → 0 → del`）。UNBOUND 的设备被 DEL 则直接走（无 ZOMBIE 中间态——§2.2 的条件是单向的）。

### 1.1 边界声明

本篇讲删除四函数。不讲：BIND 怎么加引用（09 调 `get` 的语义在 09）、REMOVE 行谁读（13）、grant/回复传输（05）。

---

## 2. C 源码分析

### 2.1 入口：找得到才配拥有广播（device.c:424-440）

本篇覆盖的 C 符号为 `do_del_device` / `devman_device_remove_event` / `_find_dev` / `devman_get_device` / `devman_put_device` / `devman_del_device`（对象形状 `devman_device` / `devman_dev` 见 03；`DEVMAN_DEVICE_ZOMBIE` / `DEVMAN_DEVICE_BOUND` 见 03 §2.3）。

```c
int dev_id = msg->DEVMAN_DEVICE_ID;
int res = 0;
struct devman_device *dev = _find_dev(&root_dev, dev_id);
if (dev == NULL) {
    printf("devman: no dev with id %d\n", dev_id);
    res = ENODEV;
}
```

找不着：打印 + ENODEV + 回复 + **无事件**（`do_del` 单测 `del_missing_is_enodev_without_event` 锁"无广播"——找不着的东西广播什么？路径都拼不出）。`res = 0` 先占位（与 07 的 safecopy-res 同款"成功预设位"，07 §2.1）。

### 2.2 三步：广播 → 改态 → 放引用（device.c:441-448）

```c
if (!res) {
    devman_device_remove_event(dev);
    if (dev->state == DEVMAN_DEVICE_BOUND) {
        dev->state = DEVMAN_DEVICE_ZOMBIE;
    }
    devman_put_device(dev);
}
do_reply(msg, res);
```

REMOVE 先行（§1 论证）；`== BOUND` 才转 ZOMBIE（UNBOUND 不转——条件单向，`del_bound_goes_zombie_first` 与 `del_unbound_…` 双测锁两边）；`put` 收尾（可能当场回收，§2.4）；`do_reply` 恒执行（成功回 0——注意：即使转了 ZOMBIE 也回 OK，删除"受理"成功不等于"回收"完成，调用方只关心前者）。

### 2.3 计数器：get/put 四行不对称（device.c:460-480）

```c
void devman_get_device(struct devman_device *dev) {
    if (dev == NULL || dev == &root_dev) return;
    dev->ref_count++;
}
void devman_put_device(struct devman_device *dev) {
    if (dev == NULL || dev == &root_dev) return;
    dev->ref_count--;
    if (dev->ref_count == 0) devman_del_device(dev);
}
```

NULL/根双守卫（根永生：引用再多不删，引用再少不回收——`put(root)` 直接回，单测锁）。不对称在调用方：`get` 的调用点（add_child 父 +1、bind 成功 +1）与 `put` 的调用点（del_device 子删父 -1、unbind -1、do_del -1）必须成对——配对表见 §4.2，review 时 grep `get_device|put_device` 全树对账（本篇 grep + 09 的 bind 各一对）。

下溢（0 上再 put）C 会 wrap 成 -1（`int--` 无检查）——Rust `EINVAL`（02 `release` 同款硬化，本篇 `put_device` 同）。

### 2.4 回收八步：`devman_del_device`（device.c:485-515）

1. 属性文件逐个：`delete_inode`（框架）+ 摘表 + `free(data)` + `free(inode)`。
2. 设备目录 `delete_inode`。
3. 从父孩子表摘除。
4. `put(parent)`（级联——父可能因此归零，递归回收）。
5. `free(info)`（grant 缓冲物归原主，07 §4 所有权注释闭环）。
6. `free(dev)`。返 0。

头顶注释（:483-484）说"有孩子就报错"（`does device have children -> error`）——**代码里没有这检查**。是 bug 吗？算账：进 `del_device` 需 `refcount == 0`；每个活孩子给父 +1（07 §2.2 第 8 步）；故 refcount 归零时孩子必已删光（它们的删除各 put 了一次父）。注释是愿望，账是现实——账平了，检查是冗余的。本篇不加检查（与 C 同），但把这笔账写进 §4.2 配对表（后人删了某处 put，账不平，注释就是抓 bug 的网）。

`dev->name` 不单独 free（指向 `info` 缓冲内，`free(info)` 连带——03 §2.1 的"name 指向 wire 缓冲"论述闭环；悬空指针的反面：用后即焚，焚得干净）。

---

## 3. Rust 设计决策

### 3.1 回收跨四文件的缝合（本篇最大设计面）

一次删除摸四个模块：设备对象（04 墓碑）、框架节点（02 `delete`）、文件注册（06 `unregister`）、事件（06 `push`，调用方注入）。`del_device` 按 C 顺序缝合（属性→目录→摘链→墓碑→级联父），每步 `?` 传播——C 是"断言式推进"（错了就炸），Rust 是"错误式推进"（错了就停，树停在一致前缀：属性删了目录没删？`?` 停在那，对象还在——半拆状态可观测但一致（无悬空：删掉的都摘干净），注释声明"失败停机位"，测试不覆盖（不可达：入参皆来自活树）。

### 3.2 墓碑 vs 摘除（04 扩展论证）

`devices` 改 `Vec<Option<Device>>`（04 私有字段，API 不动）：id 稠密永不复用（04 §4.2-1 不变量保留——墓碑占位不断号）。摘除（`swap_remove`）会换号，违反"find(id) 稳定"（04 单测锁顺序与号）——墓碑是唯一不换号的删法。代价：内存只增不减（删除不缩容）——devman 设备 churn 以十计，1024 上限内可忽略（注释量化）。

### 3.3 引用计数显式化（07 出生 1 的消费）

创建 1（07）+ 绑定 1（09）− 解绑 1（09）− 删除 1（本篇）= 0 回收。`get/put` 守卫 NULL/根（C 双守卫原样）。下溢 EINVAL（C wrap 是 UB adjacent，硬化）。

---

## 4. 实现详解

### 4.1 模块结构

```
os/servers/devman/src/del_device.rs — do_del + get/put/del_device（+4 测试）
跨文件 additive 扩展（API 不动，scan Gate H 记录）：
  02 vtreefs/inode.rs — 已有 delete（两阶段）复用，无改字
  03 structs.rs — Attribute.binding + FileBinding.cookie（08 前置扩展时已加）
  04 device_tree.rs — Vec<Option> 墓碑 + remove/unlink_child/两访问器
  06 files.rs — Option 槽 + unregister + with_files pub(crate)
```

### 4.2 引用配对表（review 对账用）

| +1（get） | −1（put） | 篇 |
|---|---|---|
| add_child 父（07 §2.2-8，Rust 省略，A-2） | del_device 子删父（本篇 §2.4-4，Rust 保留 put 语义） | 非对称省略有据（成员账） |
| 创建 1（07，Rust refcount=1） | do_del −1（本篇 §2.2） | ✅ 配对 |
| bind 成功 +1（09） | unbind −1（09） | ✅ 配对（09 单测锁） |
| — | put 下溢 | EINVAL（硬化） |

### 4.3 与 C 的差异说明

| C | Rust | 分类 |
|---|---|---|
| TAILQ 摘除 | 墓碑（不断号） | 设计决策（号稳定） |
| `free` 链 | drop + unregister（06） | A-2（所有权） |
| put 下溢 wrap | EINVAL | 硬化（02 同款） |
| 注释"有孩子报错"无代码 | 无检查 + 配对表论证 | 沿用（账平则冗余） |
| handler 返回 int | `Result<()>` | A-7 |

---

## 5. 测试要点

| 测试 | 断言 | 对应 |
|---|---|---|
| `del_missing_is_enodev_without_event` | ENODEV + 零事件 | §2.1 |
| `del_unbound_removes_and_emits` | 墓碑 + 失踪 + REMOVE 精确行 | §2.2/§2.4 |
| `del_bound_goes_zombie_first` | ZOMBIE 暂留 + 放引用后回收 | §1/§2.2 |
| `del_parent_with_live_child_survives` | 父广播不回收 + 子删级联回收父 | §2.4 成员账（07 §3.2） |
| `get_put_root_and_null_are_noops` | 根/失踪守卫 | §2.3 |

截至 2026-09-04：`cargo test -p minix-devman` **72 passed / 0 failed**（59 + 07 的 3 + 本篇 4 + 09 的 6）。

---

## 6. 过渡

终点通了：找不着回 ENODEV，找得着先广播后改态再放引用，归零即回收八步。但还有一半故事没讲：引用是谁加的（BIND +1）又是谁放的（UNBIND −1），ZOMBIE 之后 UNBIND 来了怎么办（09 §2.3 的 `!= ZOMBIE` 守卫），转发失败算谁的（09 §2.2 三分支）。09 把 RS 握手、owner 转发、19 特例一次讲完——读 09 时若忘记 `put` 的级联语义，回看 §2.4 第 4 步。

---

## 7. 参见

- `04-device-tree.md` — 墓碑与稠密号（本篇扩展的宿主）
- `06-event-buf.md` — REMOVE 行与注销机制（本篇调的两件套）
- `07-devm-add-device.md` — 出生值与配对表左半边
- `09-devm-bind-unbind.md` — 配对表右半边（bind/unbind 的 ±1）
- C 源：`minix3/minix/servers/devman/device.c:424-515`
