# 06-event-buf：事件、缓冲与读取

> **定位**：devman 的读半边。本篇回答：`buf.c` 的 skip 机制怎么 work、事件队列先进先出但读两次才算拿走、静态信息文件为什么多一个 `\n`、读 hook 怎么找到文件。只讲机制，事件内容谁生产（07/08）与谁消费（13）各归其篇。
> **源码**：`minix3/minix/servers/devman/buf.c`（129 行，全部）+ `device.c:75-140`（事件入队两函数）+ `:142-183`（两读函数）。
> **Rust 模块**：`os/servers/devman/src/buf.rs`（`Buf`）+ `src/event_queue.rs`（`EventQueue` + 两读函数）+ `src/files.rs`（`FileKind`/`FileStore`/分发）。
> **前置依赖**：01（read_hook 调用点）、02（`read()` 分块循环调本篇函数）、03（`Event` 形状）、05（事件行尾缀预算 -11）。
> **不覆盖（移交）**：事件生产业务（07/08 只调 `push`）、devmand 解析（13）、属性内容管理（07 调 `set_static_text`）。

---

## 1. 概念：读两次，才算拿走一个事件

devman 的事件队列有个让初次见面的人困惑的规矩：**读一次拿不走**。`read(events_fd)` 第一次返回事件文本，队列里还在； offset 挪到末尾再读一次，返回空，这次才真正删掉。用文件术语说：数据读 + EOF 读 = 一次完整消费（ACK）。fd 的 offset 天然推进（第一次读完 offset 落在末尾），所以"读到空"恰好就是第二次同 fd 读——协议与 Unix 语义严丝合缝，只是分了两步走。

为什么这样设计？因为 VFS 的读循环（02 §2.5）是按块取的：大文件分多次 hook 调用才读完。"读完"的唯一可观测信号就是"某次调用返回 0"。事件消费复用同一信号——没有第二套 ACK 机制，没有"已读"标记位。这是 C 用最少状态机完成可靠投递的典型手法，本篇逐行验证它。

### 1.1 边界声明

本篇讲缓冲机制（skip/left/used 三游标）、队列方向（FIFO 哪头进哪头出）、两读函数的逐行语义、分发 wiring。不讲：谁往队列里放（07/08 调 `push` 一行）、devmand 怎么解析行（13）、属性文本谁更新（07 调 `set_static_text`）。

---

## 2. C 源码分析

### 2.1 缓冲三游标：skip/left/used（buf.c:8-28）

C 侧四函数 `buf_init` / `buf_printf` / `buf_append` / `buf_result` 逐一对应 Rust `Buf::init/printf/append/result`（本节 + §2.2；`main` 不在本篇语义域，见 01）。

四个静态量：`buf`（工作区指针，BUF_SIZE）、`left`（还想收几字节）、`used`（已收几字节）、`skip`（开头丢几字节）。`buf_init(ptr, len, start)` 定三游标：`skip=start`，`left=min(len, BUF_SIZE-1)`，`used=0`。注释（buf.c:17-21）交代 `-1` 的去向：vsnprintf 要 NUL 位，最后一字节永不用——01 §2.1 的"4096+1"在此闭环。

三游标的分工：一句话——**skip 是 offset 的化身，left 是 len 的化身，used 是成绩单**。offset 读 == "先丢弃 skip 个产出字节"；len 读 == "最多再收 left 个"。`buf_result()` 返回 `used`（buf.c:122-128，"不计 NUL"——NUL 从不计入成绩）。

### 2.2 printf 与 append：同一漏斗，两种进料（buf.c:33-117）

`buf_printf(fmt, …)`：`left==0` 直接回；`max = min(skip+left+1, BUF_SIZE)` 给 vsnprintf（+1 留 NUL）；超长钳到 `BUF_SIZE-1`；skip 段（buf.c:61-74）：首次 skip 断言 `used==0`（skip 只发生在第一块——后续块 skip 已清零，断言即此）、`skip >= len` 则吃掉配额返回（整块被 offset 吞掉，成绩 0）、否则 `memmove` 前移；再按 `left` 钳；`used+=len, left-=len`。

`buf_append(data, len)`：同漏斗的裸字节版（buf.c:90-117）——skip 吃配额（指针前移而非 memmove，被跳过的数据根本不进缓冲）、`left` 钳、`memcpy`。两函数唯一区别是进料方式（格式化 vs 直拷），漏斗（skip→钳→收）同一套。Rust 的 `emit()` 即此漏斗，`printf`/`append` 只是两种进料头（§3.2）。

树内实际格式串只有两种：`"%s"`（事件行）与 `"%s\n"`（静态信息）——Rust 的格式化子集（`%s`/`%%`）覆盖全部调用点，多一分都是 YAGNI（§3.2 论证 + 单测锁两种）。

### 2.3 队列方向：头进尾出（device.c:75-140 读法）

本篇触及的 C 结构为 `devman_event` / `devman_event_inode` / `devman_inode` / `devman_device`（全字段见 03；`devman_dev` 系客户端结构，见 03 §2.7，归 10），常量 `BUF_SIZE`（见 01）与 `ADD_STRING`（事件行前缀，见 03 §2.3）。

生产者（`devman_device_add_event` :75-102、`remove_event` :108-136）：`malloc` 事件（失败 `panic`——注意 :84 的串写的是对面的函数名（add 里写 remove），抄错了一处，行为无影响；:117 的 remove 写对了）→ `memset` 清零 → `strncpy(ADD_STRING)` → `generate_path(…, 128-11)`（-11 实证，04 §2.4）→ `snprintf(" 0x%08x")` 拼尾缀 → **`TAILQ_INSERT_HEAD`** 入队。

消费者（`devman_event_read`）取 **`TAILQ_LAST`**。HEAD 进、LAST 出——**FIFO**（新来的在头，最老的在尾；读走最老的）。方向读反（以为栈）是本节唯一的坑，单测 `fifo_oldest_first` 锁顺序。

### 2.4 消费规则：空读才删（device.c:142-168）

```c
n = (struct devman_event_inode *) data;
if (!TAILQ_EMPTY(&n->event_queue))
    ev = TAILQ_LAST(&n->event_queue, event_head);

buf_init(ptr, len, offset);
if (ev != NULL)
    buf_printf("%s", ev->data);
r = buf_result();

/* read all (EOF)? */
if (ev != NULL && r == 0) {
    TAILQ_REMOVE(&n->event_queue, ev, events);
    free(ev);
}
return r;
```

逐行：有队则取最老（无队则 ev NULL）；按 offset/len 格式化；**有事件且成绩为 0 才删**。成绩为 0 的两种情形：offset 吞掉了全文（`skip >= len`，buf.c:64-68），或 len 本来就是 0。于是：数据读（r>0）不删 + EOF 读（r==0）删 = §1 的两步 drain。空队读（ev NULL）返 0，无事发生——"读空文件"与"删完了"是同一个 0，调用方不区分，也不需要区分。

注意事件行**无尾 `\n`**（`buf_printf("%s", …)`，:156）——行边界靠"一次一事件"隐含，不靠换行符。devmand 按次解析（13 实证，13 会展示它读一次处理一个）。

### 2.5 静态读：多一个换行（device.c:173-183）

```c
buf_init(ptr, len, offset);
buf_printf("%s\n", n->data);
return buf_result();
```

与事件读的唯一区别：格式串是 `"%s\n"`。静态信息文件内容自带换行结尾，事件行不带——不对称，但两边消费者各取所需（devmand `determine_type` 按行读属性，`handle_event` 按次读事件，13 双实证）。Rust 的 `read_static` 原样加 `\n`（单测锁 `USB_DEV\n`）。

### 2.6 分发：私房数据的另一端（main.c:64-66 回顾）

01 §2.3 的 `d_inode->read_fn(…)` 跳转——两种 `read_fn` 就是本篇两函数：events 文件绑 `devman_event_read`（04 §2.2 接线），属性文件绑 `devman_static_info_read`（07 挂载时绑）。`data` 指针一端是文件状态（队列头 / 文本），另一端由各文件出生时填（04 填事件线，07 填属性线）。

---

## 3. Rust 设计决策

### 3.1 全局变结构：静态四元组 → `Buf`

C 的四个 static（单进程单缓冲——同一时刻只能服务一次读，多读串行排队，在单线程事件循环里恰好安全）。Rust `Buf` 是值：谁读谁 `new()`（`try_reserve` 失败 → ENOMEM，C 的静态区永不失败是"内存足够"的隐含假设，显式化无坏处）。顺带消除一隐患：C 的 `buf` 指针由 vtreefs 静态区提供（02 的 `io_buf` 同款），两处静态同名不同命——Rust 各归各的 struct，无混淆面。

### 3.2 格式化子集：`%s`/`%%` 足矣（附等价性论证）

C 用真 `vsnprintf`（全格式），Rust 手写两分支。等价论证：树内格式串全集 = {"%s", "%s\n"}（grep 实证：`rg buf_printf` 全树两处调用点）；输入恒 ≤129 字节（03 cap + `\n`），render-then-emit 与 capped-vsnprintf 逐字节等价（§2.2 的 max/skip/left 三步在 `emit` 里重走，输入有界时无分歧——论证写进 `printf` 注释）。未知 `%x` 按字面拷贝（有此分支就不静默错；真有人加新格式，测试会逼他扩展子集——`printf_s_and_newline` 只锁两种）。

### 3.3 队列：VecDeque 前后对应

`push_back` = INSERT_HEAD（新在尾），`front` = TAILQ_LAST（老在头）——方向注释写死在类型上（`push` 文档 + `fifo_oldest_first` 单测双保险）。`malloc` 失败 panic → `try_reserve` → ENOMEM（A-7，队列满与内存竭同一码，调用方只认 ENOMEM）。

### 3.4 分发：索引 cookie + 进程表（无裸指针、无 unsafe 别名）

C 的 cookie 是裸指针（`&event_inode`/`&devman_inode`），Rust 用**下标**：`FileStore`（append-only `Vec`，下标永稳）+ 进程单例（`AssumeSyncCell<RefCell<…>>`，VM 同款单线程论证，见 06 scan 引用行）。未知 cookie → EOF（fail-closed；C 会解引用 NULL 炸——拒绝是硬化，单测锁 `c+9999 → 0`）。

`RefCell` 而非裸 `&mut`：hook 签名固定（01 形状）拿不到表引用，单例是唯一通道；`RefCell` 把"重入即 UB"降级为"重入即 panic"——而分发路径不重入（hook → Buf only），panic 分支不可达（注释 + 无测试覆盖此分支是**对的**：不可达分支不配测试，配注释）。

`read_hook` 本体（hooks.rs）即 `dispatch_read` 转交——01 §2.3 预告的第二次兑现（第一次是 05 的分发）。01 doc 仍无需改字（"06 wires the real dispatch" 现在就是）。

### 3.5 07/13 的接口面（本篇是机制层）

- 07/08 生产：`queue.push(Event::new(行)?)`（行构造归生产方，`Event::new` 卡 128——超长生产方截断并注释，06 不代劳，03 §3.5 同款诚实）。
- 07 内容管理：`set_static_text(cookie, text)`（属性变了刷新文本；非静态 cookie → EINVAL）。
- 13 消费：读两次（数据 + EOF），格式见 §2.4/§2.5（`\n` 有无对照表放 13，06 只给字节）。

---

## 4. 实现详解

### 4.1 模块结构

```
os/servers/devman/src/
  buf.rs          — Buf（new/init/printf/append/result + 4 测试）
  event_queue.rs  — EventQueue（push/oldest/consume/read_oldest/read_static + 3 测试）
  files.rs        — FileKind/EventFile/StaticFile/FileEntry/FileStore/单例/dispatch_read（+2 测试）
  hooks.rs        — devman_read_hook 本体改为 dispatch_read 转交（01 桩兑现×2）
```

### 4.2 关键不变量

1. `Buf` 输出恒 ≤ `min(len, 4097)`（init 钳 + emit 钳双保险）。
2. 队列 FIFO（push_back/front，单测锁顺序）。
3. 有事件 + 空成绩 ⇔ 消费恰一次（`read_oldest` 内原子：格式化与 pop 同函数，无中间态）。
4. 未知 cookie 读 EOF（fail-closed）。
5. `dispatch_read` 永不 panic（`unwrap_or_default` 兜底 + copy 上限三取 min）。

### 4.3 与 C 的差异说明

| C | Rust | 分类 |
|---|---|---|
| 四静态量 | `Buf` 值类型 | 去全局（多实例安全） |
| 真 vsnprintf | `%s`/`%%` 子集 | 等价子集（调用点全集论证） |
| 裸指针 cookie | 下标 cookie + 进程表 | 可验证性（未知→EOF 硬化） |
| malloc 失败 panic（串错函数名） | ENOMEM | A-7（+ 顺手不继承串名 panic） |
| 事件行无 `\n` / 静态有 `\n` | 原样（一无一有，单测双锁） | 沿用（不对称是契约） |

---

## 5. 测试要点

| 测试 | 断言 | 对应 |
|---|---|---|
| `init_caps_len` | len 钳 4096 | §2.1 |
| `printf_s_and_newline` | 两种格式串 | §2.2 调用点全集 |
| `skip_eats_output_then_eof` | 5 跳余量 / 10 跳全空 | §2.1 skip 语义 |
| `left_zero_short_circuits` | 0 长双空 | §2.2 早退 |
| `fifo_oldest_first` | 老先出 + 非消费读不删 | §2.3 方向 |
| `drain_takes_two_reads` | 数据读 + EOF 删 + 空队空读 | §2.4 两步 |
| `static_appends_newline` | `\n` 有 + offset 同漏斗 | §2.5 |
| `register_resolves_index` | 注册/越界 None/静态改文/非静态 EINVAL | §3.4 |
| `dispatch_end_to_end` | 全局表 + hook 形调用 + 未知 EOF | §3.4 |

截至 2026-09-04：`cargo test -p minix-devman` **59 passed / 0 failed**（45 + 05 的 5 + 本篇 9：buf 4 + queue 3 + files 2）。`cargo clippy` devman 部分 0 警告。

---

## 6. 过渡

读半边通了：缓冲漏斗、队列 FIFO、两步 drain、`\n` 有无、分发 wiring。下一站 07 走完全流程——收 ADD（05 原语）→ grant 拷入（05 §2.2）→ wire 解码（03）→ 种树（04 `insert` + 框架 `add`）→ 发事件（本篇 `push`，行格式 `"ADD " + 路径(117预算) + " 0x%08x"`）→ 回复（05 `apply_reply`）。07 会第一次调通本篇的全部生产接口；读 07 时若忘记事件行谁拼的，回看 §2.3 的生产者三段式（malloc→snprintf→HEAD）。

---

## 7. 参见

- `01-devm-init-main.md` — read_hook 调用点（分发源头）
- `02-vtreefs-framework.md` — `read()` 分块循环（本篇函数的调用方）
- `03-devm-structs.md` — `Event` 形状（本篇队列的货）
- `04-device-tree.md` — 路径预算 -11（本篇事件行的长度账）
- `05-devm-message-contract.md` — 回复原语（事件生产后的收尾在 07 用本篇 + 05）
- `07-devm-add-device.md` — 生产方（`push` + `set_static_text` 的调用方）
- `13-devmand-consumer.md` — 消费方（两步 drain 的另一端）
- C 源：`minix3/minix/servers/devman/buf.c`、`device.c:75-183`
