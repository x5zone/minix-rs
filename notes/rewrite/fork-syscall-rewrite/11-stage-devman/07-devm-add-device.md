# 07-devm-add-device：设备添加全流程

> **定位**：设备生命的起点。本篇回答：一条 ADD 从 grant 字节变成树上设备 + 属性文件 + 事件 + 回复的完整 8 步，每步的 C 证据与 Rust 落点。
> **源码**：`device.c:223-277`（`do_add_device`）+ `:314-397`（`add_static_info`/`add_child`）+ `:404-418`（`add_info`）。
> **Rust 模块**：`os/servers/devman/src/add_device.rs`（`do_add` + `add_static`）。
> **前置依赖**：03（wire 解码）、04（`insert`/`generate_path`/`alloc_id`)、05（grant 原语 + `apply_reply`）、06（`push` + 文件注册）。
> **不覆盖（移交）**：删除（08）、绑定（09）、客户端构造（10）、事件消费（13）。

---

## 1. 概念：注册是"认领 + 落户 + 广播"

一次 ADD 干三件事，顺序不能换：

1. **认领**：grant 拷入 + wire 解码 + 父存在——确认"这单生意能做"（任一步失败直接回复错误码，树里什么都不剩）。
2. **落户**：对象进设备树 + 目录进框架树 + 属性文件逐个挂 + `devman_id` 文件——确认"设备住下了"（此后 `find` 找得到，`ls` 看得见）。
3. **广播**：ADD 事件进队列 + 回复 dev_id——确认"外界知道了"（devmand 来读，驱动拿 id）。

C 把三段塞进两个函数（`do_add` 管认领+收尾，`add_child` 管落户），Rust 的 `do_add` 一气呵成（§3.1 论证合并不分家）。

### 1.1 边界声明

本篇讲添加 8 步。不讲：grant 拷贝的内核原语（05 §2.2 信任根，传输层实现）、删除与解绑（08/09）、wire 编码侧（10）、事件行谁读（13）。

---

## 2. C 源码分析

### 2.1 认领三错：ENOMEM / EINVAL / ENODEV（device.c:228-256）

本篇覆盖的 C 符号为 `do_add_device` / `devman_dev_add_child` / `devman_dev_add_info` / `devman_dev_add_static_info` / `devman_device_add_event` / `do_reply` / `_find_dev`（树查找见 04 §2.3；对象形状 `devman_device` / `devman_dev` 见 03 §2.1/§2.7；`DEVMAN_DEVICE_UNBOUND` 见 03 §2.3）。

```c
devinf = malloc(msg->DEVMAN_GRANT_SIZE);          /* ENOMEM */
res = sys_safecopyfrom(ep, GRANT_ID, 0, devinf, GRANT_SIZE);  /* EINVAL */
parent = _find_dev(&root_dev, devinf->parent_dev_id);         /* ENODEV */
```

每错都 `free + do_reply + return 0`（除第一错无物可 free）。三错映射 05 §2.2 的信任根论述。注意 `res` 的脆弱传递：成功路径的 `res` 是 safecopy 留下的 `OK`（device.c:237 赋值后未再动）——结尾 `do_reply(msg, res)` 回的正是它（§2.4 展开这是巧合还是设计）。

### 2.2 落户七步：`devman_dev_add_child`（device.c:345-397）

1. `malloc` 设备（NULL → `panic`，:353-356——A-7 直接实例，Rust `ENOMEM` 经框架 `add` 传播）。
2. parent NULL → free + NULL（:358-361——类型层面不可能（`DeviceId` 非空），Rust 无此分支，§3.3）。
3. `ref_count = 1`（:364——新生儿自带 1，08 消费它）。
4. 填 parent/info/`dev_id = next_device_id++`（:367-371——id 分配即自增，无锁无检查，A-5）。
5. `add_inode` 建目录（wire 名，:373-375——**目录名来自 wire**，04 §2.4 的名字来源论述闭环）。
6. 条目循环调 `add_info`（:385-389——返回值**丢弃**，§2.3）。
7. `snprintf(id)` + `add_static_info(dev, "devman_id")`（:392-393——每个设备自带 id 文件，devmand/RS 读它）。
8. 挂父孩子表 + `get(parent)`（:395-397——父引用 +1，Rust 保留，§3.2）。

返 dev（NULL 仅 parent-NULL 路）。结尾注释 `FUTURE TODO: create links(BUS, etc)`（:399）——BUS 链接从未实现（A-6 的又一旁证，05 §2.6 引用）。

### 2.3 条目循环：STATIC 落户，DYNAMIC 静默跳过（device.c:404-418）

```c
switch(entry->type) {
case DEVMAN_DEVINFO_STATIC:
    return devman_dev_add_static_info(dev, buf + name_off, buf + data_off);
case DEVMAN_DEVINFO_DYNAMIC:
    /* TODO */
    /* fall through */
default:
    return -1;
}
```

DYNAMIC（及未知类型）→ `-1`，而调用方**不看返回值**（:387 `devman_dev_add_info(dev, &entries[i], buffer);` 裸调）——静默跳过，A-6 defer 的完整证据链（定义有、发送方恒 0、服务端遇见即弃、返回值都懒得看）。Rust 原样跳过（`continue`，单测 `add_skips_dynamic_silently` 锁定"无错且无文件"）。

`add_static_info`（:314-339）：两次 `malloc`（**不检查 NULL**——双 latent NPE，Rust 不存在此类；`strncpy` + `[127] = 0` **截断语义**（03 §3.5 的答案：静态侧截断，Rust 预截断同字节）；`add_inode`（file stat）+ 挂 `infos` 表；恒返 0。

### 2.4 收尾三行：状态 + 主人 + 事件 + 回复（device.c:268-277）

```c
dev->state = DEVMAN_DEVICE_UNBOUND;
dev->owner = msg->m_source;
msg->DEVMAN_DEVICE_ID = dev->dev_id;
devman_device_add_event(dev);
do_reply(msg, res);   /* res == OK（safecopy 留下的，§2.1） */
return 0;
```

UNBOUND（新生）、owner（发送方端点——09 转发就找它）、DEVICE_ID 回填（`m4_l2` 复用，05 §1.1）、事件（"ADD " + 117 预算路径 + id 尾缀，04 §2.4/06 §2.3）、回复 OK。`return 0` 恒（handler 返回值无人读——proto.h 声明 `int` 纯属 C 习惯，Rust 用 `Result`，A-7）。

---

## 3. Rust 设计决策

### 3.1 两函数合一：`do_add` 全流程（边界在传输，不在函数）

C 分 `do_add`/`add_child` 是因为指针 lifetimes（devinf 的 malloc/free 配对跨函数 manually）。Rust 值语义下 `ParsedDevice` 进、`DeviceId` 出，中间无手动释放——合并不损失清晰度，反而让 8 步顺序一目了然（单测 `add_full_flow` 即 8 步断言表）。

### 3.2 成员账保留：`get(parent)` 不省略（审查中纠正）

初稿曾计划省略父引用计数（"树所有权表达成员关系"），审查中发现删除语义依赖它：C 里有活孩子的父调 `do_del` 只广播不回收（refcount > 0 拦住 `del_device`，device.c:449 语义）——省略 `get` 则删孩子会级联误删父（08 审计发现，见 08 scan P0-1）。故保留完整配对：`insert` 后 `get(parent)`（device.c:395-397 顺序），`del_device` 末 `put(parent)`（08）。树所有权表达**成员关系**，引用计数表达**删除资格**——两套账各管各的（配对表见 08 §4.2）。

### 3.3 不可能分支删除（类型级）

parent-NULL（`DeviceId` 非空）、`add_inode` 返回检查保留（框架 fallible）。删的是"类型不让发生"的分支，留的是"运行时可失败"的分支——原则：assert/防御性分支只留给类型表达不了的。

### 3.4 截断两策：静态预截断 vs 事件严拒（03 §3.5 的答案揭晓）

静态文本超长 → 预截断 127（C 同字节输出，调用点无感）；事件行超长 → `ENAMETOOLONG`（事件行是协议行，截断即 corrupt，宁可失败——C 会溢出 `char[128]`，硬化）。同是超长，一截一拒，理由各写进注释（"为什么不同"比"怎么做"重要，教学性要求）。

### 3.5 名字空格拒绝（OQ-3 决议，用户批准的新行为）

devmand 用 `%s` 劈事件行（13 §2.1）——名含 ASCII 空白则路径静默错配（错配比失败更坏）。C 无检查（加了算新行为，OQ-3 讨论记录），用户批准加：`do_add` 认领期拒含空白名（`EINVAL`，副作用零——检查在 `alloc_id` 之前）。单测 `add_whitespace_name_is_einval` 锁（拒 + 零事件 + 树仅根）。

---

## 4. 实现详解

### 4.1 模块结构

```
os/servers/devman/src/add_device.rs — do_add + add_static（+3 测试）
```

### 4.2 关键不变量

1. `do_add` 成功 ⟺ 树里有设备 + 框架里有目录 + 属性文件数 == STATIC 条目数 + 1（devman_id）+ 事件恰 1。
2. 失败路径：认领期（parent 缺失/解析失败）三无残留（各 `?` 早返在任何落子之前）；落户期后失败理论上留框架残留（`fw.add` 成功、后继失败）——但后继（属性注册/事件行）对合法输入不可失败（路径 ≤111 恒进 128，算术见 04 §2.4），残留分支不可达（注释声明"与 C 同无回滚"；C 在同处直接 `panic`，Rust 的 `Err` 已是更体面的死法）。
3. `refcount == 1` 出生（C :364 同值）。

### 4.3 与 C 的差异说明

| C | Rust | 分类 |
|---|---|---|
| do_add/add_child 双函数 | do_add 单流程 | 合并（值语义无手动释放） |
| `get(parent)` 成员账 | 保留（见 §3.2；DEL 存活语义依赖） | 配对保留（非省略） |
| parent-NULL 分支 | 删除 | 类型不可能 |
| handler 返回 int（恒 0） | `Result<DeviceId>` | A-7 |
| malloc 失败 panic/不检查 | ENOMEM 传播 | A-7 |
| 事件行超长溢出 | ENAMETOOLONG | 安全硬化 |
| 名字含 ASCII 空白 | EINVAL（认领期，零副作用） | 新行为（OQ-3 决议：错配比失败更坏） |
| DYNAMIC 静默跳过 | `continue` + 单测锁定 | 沿用（A-6） |

---

## 5. 测试要点

| 测试 | 断言 | 对应 |
|---|---|---|
| `add_full_flow` | id/名/UNBOUND/owner/ref1/属性2/框架镜像/事件行精确 | §2 全流程 8 步 |
| `add_bad_parent_is_enodev` | ENODEV + 零事件 | §2.1 第三错 |
| `add_skips_dynamic_silently` | 仅 devman_id + 事件 1 + 无错 | §2.3 A-6 |
| `add_whitespace_name_is_einval` | 空白名 EINVAL + 零事件 + 树仅根 | §3.5 OQ-3 |

截至 2026-09-04：`cargo test -p minix-devman` **78 passed / 0 failed**（59 + 07 的 4 + 08 的 4 + 09 的 6——07~09 同批落地 + OQ-3 追补 1，计数见 09 §5汇总）。

---

## 6. 过渡

起点通了：ADD 进来，设备住下，事件出去。但住下的设备怎么**搬走**（DEL 的引用计数与 ZOMBIE）是 08 的，搬走前怎么**交接给驱动**（BIND 转发与 owner）是 09 的。08 会第一次调通 `put_device` 的删除级联；09 会把 05 的 `check_rs` 与转发闭环。读 08 时若忘记 refcount 出生值，回看 §2.2 第 3 步。

---

## 7. 参见

- `03-devm-structs.md` — wire 解码对象（`ParsedDevice` 的来源）
- `04-device-tree.md` — `insert`/`generate_path`/`alloc_id`（本篇调的三件套）
- `05-devm-message-contract.md` — grant 原语 + `apply_reply`（本篇的进与出）
- `06-event-buf.md` — `push` + 文件注册（本篇调的机制）
- `08-devm-del-device.md` — 删除级联（refcount 的消费者）
- C 源：`minix3/minix/servers/devman/device.c:223-277,314-418`
