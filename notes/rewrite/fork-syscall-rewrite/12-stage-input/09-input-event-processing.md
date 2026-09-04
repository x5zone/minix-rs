# 09-input-event-processing: 事件接收与唤醒

> **状态**: 已改写（2026-09-05，首版完整文档）
> **定位**: 上报校验、入队、多路分流、唤醒挂起读与选择者、终端转交（阶段 4，事件与灯光机制；全阶段的心脏）
> **源码**: `minix3/minix/servers/input/input.c:332-422`（`input_process`/`input_event`）
> **Rust 模块**: `os/servers/input/src/produce.rs`（路由、入队、唤醒）
> **目标读者**: 想理解"驱动上报一个事件之后，服务依次做什么决定、叫醒谁"的读者。前置知识：第 03 篇（槽位与编号）、第 04 篇（事件格式）、第 07 篇（挂起纸条）、第 08 篇（选择者）。
> **本章不讲什么**: 挂起的建立（第 07 篇，只引用）；取消挂起（第 08 篇）；终端收到转交后做什么（第 13 篇）；驱动 side 组装上报（第 12 篇）；灯光（第 10 篇）；连接与断开（第 11 篇）。

---

## 1. 概念：一个事件的三站旅程与三级跳板

### 1.1 旅程总图：校验、落子、叫人

驱动上报一个事件，服务做三件事，顺序固定：

```
驱动上报（槽位号 + 事件四要素 + 发送者身份）
  |
  v
第 1 站  校验：号码合法吗？是你家的槽位吗？
  |-- 号码越界 → 丢掉（不声张，见 1.2 节）
  |-- 不是你的槽位 → 丢掉（不声张）
  v
第 2 站  落子：事件进哪个队列？
  |-- 自己的设备开着 → 进自己的队列
  |-- 自己没开，多路器开着 → 进多路器的队列（三级跳板，见 1.3 节）
  |-- 都没开 → 转交给终端（见 1.4 节）
  v
第 3 站  叫人：队列里有新人了，谁在等？
  |-- 有挂起的读 → 拷一个事件给他，寄迟到的回信，撕掉纸条
  |-- 没挂起但有预约的 → 通知他"有货了"，擦掉名字（只通知一次）
  |-- 都没有 → 什么都不做（事件躺在队列里等下次读）
```

三站之间是"瀑布"关系：校验不过就没有落子，落子之后必有叫人环节（哪怕结论是"没人可叫"）。读者记住"校验、落子、叫人"六个字，就记住了本章全部。

### 1.2 校验为什么沉默：对坏消息最大的惩罚是不理它

两处校验失败都静默丢弃：不记日志、不回消息、不计数。初看像是"错误被吞了"，细想是唯一正确的处理。上报走的是单向协议（第 05 篇），本来就没有回消息的通道——想告诉驱动"你的号码错了"也无处可说。记日志呢？驱动是持续上报的进程，一个发疯的驱动每秒几千条上报，每条记一行日志，日志盘先被写满，服务再被拖慢——用"记一笔"惩罚"发疯"，惩罚落到了自己头上。沉默是深思熟虑的冷处理：坏消息得不到任何反馈，发疯的驱动只能自己超时、自己崩溃、自己重起（重起后重新连接，第 11 篇）。"不理它"在这里不是懒惰，是限流。

### 1.3 三级跳板：具体设备、多路器、终端

落子的三级是精心设计的 fallback 链。第一级是事件自己的设备（"一号键盘的按键进一号键盘的队列"）；设备没开（没人读它），事件跳到第二级多路器（"任意键盘"的队列，读多路器的人通常是"谁按键我都要"的程序，比如终端）；多路器也没开，事件跳到第三级终端（终端永远在——它是系统的脸面，键盘事件最终总要变成屏幕上的字）。三级链保证"事件永不落地"：只要终端活着，按键就不会丢（终端的队列满了是终端的事，第 13 篇）。设计三级而不是两级（去掉多路器）的原因：多路器让"读任意键盘"不需要打开四个设备——打开四个意味着四个独占（第 06 篇），第四个会报正忙；多路器是一个入口解决"全都要"。

### 1.4 转交是复制粘贴，不是翻译

第三级（转交终端）把上报的五个字段原样抄进转交消息，一字不改。为什么敢不做任何处理？因为上报载荷和转交载荷是同一套字段（第 05 篇 2.2-2.3 节），而终端要的正是驱动给的——服务在转交路径上没有任何信息需要增删。复制粘贴是最诚实的中转：中转商不赚差价（不改数据），也不赔本（不丢数据）。校验（第 1 站）已经做过，转交时不再验一次——验过的不复验，是本阶段统一的纪律（验两次的代码，第二次永远是摆设，还占一次分支）。

### 1.5 叫人只叫一个：挂起优先于预约

队列进新人后，服务先看有没有挂起的读：有，只拷一个事件给他（注意不是"把队列搬空"——挂起的读在队列空时睡下，此时队列里通常只有一个新人；拷一个之后纸条撕掉，任务完成）。没有挂起，才看有没有预约的选择者：有，通知他"有货了"并擦掉名字（一次性通知，下次还要再预约）。两者互斥：`if/else if` 不是 `if/if`——挂起的读者拿走事件，选择者等下一次。这种"读者优先于问者"的顺序是自然的：读者已经承诺了内存（授权），问者还没有；先满足承诺，再处理意向。

### 1.6 溢出覆盖最旧：满了就忘掉最早的

队列满 32 个时再进新人，队尾前进一步（最旧的事件被踩掉），计数先减后加，净长度不变。为什么覆盖最旧而不是拒绝最新？因为输入是"现在进行时"的流：用户关心刚按的键，不关心一秒钟前没读走的键。拒绝最新会让队列凝固在过去（越不读越旧，越旧越不值得读，死锁）；覆盖最旧让队列永远新鲜（读到的总是最近 32 个）。溢出的条件编译打印（`INPUT_DEBUG` 开关）在 Rust 一侧没有对应物——调试开关是构建期配置，不是行为，本篇登记它的存在，实现不跟进（所有权规则：行为才需要实现）。

### 1.7 本章小结：校验落子叫人，外加溢出

读完本章，读者应该能不假思索地回答：三站是什么顺序、校验失败为什么沉默、三级跳板为什么三级、转交为什么是复制粘贴、叫人为什么只叫一个且挂起优先、溢出为什么覆盖最旧。下一章（第 10 篇）讲灯光：事件是"进来"的数据，灯光是"出去"的命令。

---

## 2. C 源码分析

> 本章逐段对照原始 C 代码。所有行号以工作区当前 `minix3/` 为准。

### 2.1 接报与校验：号码与主人（input_event 前半，input.c:376-397）

```c
static void
input_event(message *m)
{
        struct input_dev *input_dev, *mux_dev;
        int r, id;

        /* Unlike minor numbers, device IDs are in fact array indices. */
        id = m->m_linputdriver_input_event.id;
        if (id < 0 || id >= INPUT_DEV_MAX)
                return;

        /* The sender must owner the device. */
        input_dev = &devs[id];
        if (input_dev->owner != m->m_source)
                return;

        /* Input events are also delivered to the respective multiplexer. */
        if (input_dev->minor >= KBD0_MINOR &&
            input_dev->minor < KBD0_MINOR + KBD_MINORS)
                mux_dev = &devs[KBDMUX_DEV];
        else
                mux_dev = &devs[MOUSEMUX_DEV];
```

开头的注释是全文件最重要的注释之一（plan A-3 的出处）："和次设备号不同，设备编号实际上就是数组下标。"——上报里的 `id` 直接当数组下标用，所以校验是"负数或超长就丢"，没有任何换算（和第 03 篇正查函数的换算对照：门牌号进门要换算，槽位号进门直接进）。主人校验是端点比较（一句，没有日志，1.2 节）。多路选择看次设备号落不落在键盘窗口里（`KBD0_MINOR` 起四个，写法和第 03 篇正查函数同源）；落在外面的一律归鼠标多路器——"一律"包括鼠标设备、多路器自己、乃至不可能的值，但不可能的值到不了这里（主人校验先拦：多路器槽位从没有主人，第 11 篇分配只分 1-4 与 6-9）。注释"事件也送到各自的多路器"点出多路器的存在意义：具体设备与多路器是"也"的关系，不是"或"的关系——事件进具体设备的队列，同时多路器是 fallback，不是镜像（镜像会复制事件，这里只选其一，第 2.2 节）。

### 2.2 落子三级：设备、多路器、终端（input_event 后半，input.c:404-421）

```c
        if (input_dev->opened)
                input_process(input_dev, m);
        else if (mux_dev->opened)
                input_process(mux_dev, m);
        else {
                message fwd;
                mess_input_tty_event *tty_event = &(fwd.m_input_tty_event);

                fwd.m_type = TTY_INPUT_EVENT;
                tty_event->id = m->m_linputdriver_input_event.id;
                tty_event->page = m->m_linputdriver_input_event.page;
                tty_event->code = m->m_linputdriver_input_event.code;
                tty_event->value = m->m_linputdriver_input_event.value;
                tty_event->flags = m->m_linputdriver_input_event.flags;

                if ((r = ipc_send(TTY_PROC_NR, &fwd)) != OK)
                        printf("INPUT: send to TTY failed (%d)\n", r);
        }
```

三级用 `if/else if/else` 写死优先级（1.3 节）。注意判断条件是"开着"（`opened`），不是"营业"（`active`，第 03 篇）：营业是有驱动，多路器永远营业——如果判营业，转交分支永远走不到（多路器恒真），三级变两级。用"开着"判，进第三级的前提是"具体设备没人读、多路器也没人读"，语义精确："没人要才转交"。转交五字段逐个复制（1.4 节），阻塞发送（终端必须收到——转交是事件不落地的最后一站，用阻塞发送确保送达，失败只记日志：终端死了的话，记一笔，事件丢了认了——终端是脸面，脸面没了，事件保住也没意义）。

### 2.3 入队：溢出、落子、计数（input_process 前半，input.c:332-355）

```c
        if (input_dev_buf_full(input_dev)) {
                /* Overflow.  Overwrite the oldest event. */
                input_dev->tail = (input_dev->tail + 1) % EVENTBUF_SIZE;
                input_dev->count--;

#if INPUT_DEBUG
                printf("INPUT: overflow on device %u\n", input_dev - devs);
#endif
        }
        next = (input_dev->tail + input_dev->count) % EVENTBUF_SIZE;
        input_dev->eventbuf[next].page = m->m_linputdriver_input_event.page;
        input_dev->eventbuf[next].code = m->m_linputdriver_input_event.code;
        input_dev->eventbuf[next].value = m->m_linputdriver_input_event.value;
        input_dev->eventbuf[next].flags = m->m_linputdriver_input_event.flags;
        input_dev->eventbuf[next].devid = m->m_linputdriver_input_event.id;
        input_dev->eventbuf[next].rsvd[0] = 0;
        input_dev->eventbuf[next].rsvd[1] = 0;
        input_dev->count++;
```

满了先踩掉最旧（队尾前进一步，计数减一，保持"满"状态），再算落子位置（队尾加计数取模——满的时候计数先减，位置公式统一），五字段复制加槽位号记入来源、保留字清零，计数加一。落子位置公式在满与不满时统一，是这段代码最漂亮的地方：不需要 `if (full) ... else ...` 两套写法，一套算术走天下（Rust 版本逐字保留，第 3 章）。调试打印在条件编译里（1.6 节：登记存在，实现不跟进）。

### 2.4 叫人：挂起优先于预约（input_process 后半，input.c:357-370）

```c
        /*
         * There is new input.  Revive a suspended reader if there was one.
         * Otherwise see if we should reply to a select query.
         */
        if (input_dev->suspended) {
                r = input_copy_events(input_dev->caller, input_dev->grant, 1,
                    input_dev);
                chardriver_reply_task(input_dev->caller, input_dev->req_id, r);
                input_dev->suspended = FALSE;
        } else if (input_dev->selector != NONE) {
                chardriver_reply_select(input_dev->selector, input_dev->minor,
                    CDEV_OP_RD);
                input_dev->selector = NONE;
        }
```

注释把优先级写明了（"先救挂起的读，否则看选择者"）。叫醒挂起只拷一个事件（1.5 节：新人通常只有一个），拷贝结果（字节数或拷贝错误）直接当回信状态——拷贝失败也叫醒（带着错误醒，比睡死强；且失败时队尾没动，第 07 篇 2.2 节，事件还在，下次读还能拿到）。叫醒后撕纸条（标志复位）。无挂起才看选择者：通知"可读"，擦掉名字（一次性）。注意叫醒用的是 07 篇的拷贝与 02 篇的回答通道——本章是它们的第一次联合作战，第 07 篇的纸条在这里兑现，第 02 篇的异步回答在这里使用。

### 2.5 覆盖核对

| 符号 | 源码位置 | 本文档位置 | Rust 对应（produce.rs） |
|------|---------|-----------|------------------------|
| `input_event` 校验与分流 | input.c:376-403 | 2.1 节 | `route_event`（校验） |
| 多路选择 | input.c:392-397 | 2.1 节 | `multiplexer_for` |
| 落子三级 | input.c:404-421 | 2.2 节 | `route_event`（三级）+ `forward_to_terminal` |
| 入队与溢出 | input.c:332-355 | 2.3 节 | `stored_event` + `enqueue` |
| 叫醒挂起与选择者 | input.c:357-370 | 2.4 节 | `decide_wake` + 两个 apply |
| 调试打印 | input.c:343-345 | 1.6 节登记 | 无（构建期配置） |

---

## 3. Rust 设计决策

> 本章解释"为什么这样设计"。每个决策先说备选方案，再说选择的理由。

### 3.1 路由是纯函数，返回"去哪"而不是"去做"

`route_event` 输入表格、槽位号、发送者，输出三选一（送某槽、转交、丢弃加原因），不碰队列、不发消息。理由：路由是判断，入队与转交是执行——第 06 篇 3.1 节的分离手法在本章第三次使用（前两次：打开关闭、读）。三选一枚举让调用者（分发层）用 `match` 穷举：漏掉"转交"分支编译失败——C 里漏掉 `else` 分支只是少一次转发，静默丢事件，编译器不吭声。丢弃原因（号码坏还是主人不对）装进变体：日志（传输层想记时）能说出"为什么丢"，而 C 的沉默丢弃在需要排障时两眼一抹黑——变体不改变线缆行为（照样沉默），但给排障留了把手。

备选方案是路由入队一体（查到槽位直接塞）。拒绝的理由：一体函数测试"路由错了"时必须检查队列副作用（塞没塞、塞哪了），判断的测试被执行污染；且转交分支需要组装转交消息，一体函数里"组装"与"发送"混在一起，发送失败的测试需要伪造终端——纯路由函数一个整数就能测。

### 3.2 多路选择保留"一律归鼠标"的兜底形状

`multiplexer_for` 对键盘窗口之外的一切返回鼠标多路器，包括多路器自己与不可能的值——和 C 逐字一样。有人可能想"收紧"（不可能的值返回空）：拒绝的理由是收紧改变不了任何可达行为（不可达输入到不了这里，主人校验先拦），却让函数与 C 在"全体输入"上不一致——将来有人拿着一个奇怪的次设备号来问"归谁"，C 与 Rust 答案不同，排障时多一层困惑。兜底形状是免费的（一个 `else`），一致性是无价的（排障时少一个问号）。测试把"键盘窗内归零、其余全归五（含 0、负数、大数）"锁死，保证兜底永远在。

### 3.3 入队返回是否溢出，而不是溢出计数

`enqueue` 返回布尔值（这次有没有踩掉旧事件），不返回踩掉几个（永远最多一个：满时先减一再加一，净覆盖恰好一个）。理由：调用者（统计、日志、测试）只关心"丢没丢"，不关心"丢几个"——丢几个恒为一，返回计数是虚假的精确。布尔值是诚实的精度。溢出时不记日志（C 在非调试构建同样沉默）：每个溢出都记日志等于给"读得慢的读者"配一个"写日志的惩罚"，惩罚落错了对象（1.2 节沉默哲学的又一次应用）。

### 3.4 叫醒顺序写进函数结构，而不是注释

`decide_wake` 先查挂起再查选择者，用 `if/else if` 的结构表达优先级——和 C 一样。但 Rust 多一步：返回的枚举变体把"叫醒谁、带什么去"装好（调用者加编号 / 选择者加次设备号），传输层照单执行。结构表达顺序的好处：调换两行就是调换优先级，review 时一眼看见；注释表达顺序（"先救挂起"）需要读者相信注释与代码一致。结构不会说谎。

### 3.5 窄化转换集中在一处，加注释

上报的整型字段变事件的 16 位字段，转换集中在 `stored_event` 一处，注释写明"槽位号小于十、页码事件码是协议小量，与 C 的隐式转换同效"。理由：窄化（`as` 截断）是全 crate 最需要盯的地方，分散在各处等于到处埋雷；集中一处，review 时只看一处。槽位号小于十由路由保证（路由先行，入队在后——顺序即安全），页码事件码小量由协议保证（第 04 篇）——两个保证各有主人，转换处只引用，不复验（验过的不复验，1.4 节）。

---

## 4. 实现详解

> 完整代码在 `os/servers/input/src/produce.rs`。本章按"路由、入队、叫醒"的顺序展开，每个小节标注对应的第 3 章决策。

### 4.1 路由：`EventIntake`、`route_event`、`multiplexer_for`（对应决策 3.1、3.2）

```rust
pub enum EventIntake {
    Deliver { target: DeviceIndex },  // 进某槽队列
    ForwardToTerminal,                // 转交终端
    Drop(DropReason),                 // 沉默丢弃加原因
}
```

`route_event` 按 C 顺序：负数拒（注意 `i32` 先判负再转无符号——转之前不验负，大数回绕是经典 bug，本函数第一行就是负数检查）、越界拒（复用 `DeviceIndex::new`，第 03 篇）、主人拒、自己开着送自己、多路器开着送多路器、否则转交。`multiplexer_for` 是 `const fn`（纯查表逻辑，编译期可求值）。

### 4.2 入队：`stored_event` 与 `enqueue`（对应决策 3.3、3.5）

`stored_event` 组装存储形态（五字段加来源加清零保留），`enqueue` 执行"满则踩、新则落、计数加一"并报告是否溢出。位置公式 `(tail + count) % 32` 与 C 同式（含满时先减一的统一技巧，2.3 节）。

### 4.3 叫醒：`WakeDirective`、`decide_wake` 与两个 apply（对应决策 3.4）

```rust
pub enum WakeDirective {
    AnswerReader { caller: Endpoint, request_id: u32 },
    NotifySelector { selector: Endpoint, minor: Minor },
    Nobody,
}
```

判断只读标志（挂起优先），执行只改标志（叫醒复位挂起、通知擦掉名字）。拷贝一个事件与两封回答走传输层（07 篇的拷贝、02 篇的通道），判断执行分离保证"拷贝失败不撕纸条"（传输先成功才调 apply，第 07 篇 4.4 节的插入点纪律）。

### 4.4 转交：`ForwardedEvent` 与 `forward_to_terminal`（对应 1.4 节）

五字段逐个复制的函数形态。发送（阻塞）与失败日志归传输层——函数只组装，组装是纯的，可测的（`test_forward_copies_lanes`）。

### 4.5 与 C 的差异说明

| C 行为 | Rust 对应 | 差异分类 |
|--------|----------|---------|
| 校验沉默丢弃 | 同沉默（原因装变体，不记日志） | 无差异（线缆一致，排障把手是加法） |
| 落子三级与优先级 | 同三级同优先级 | 无差异 |
| 入队公式与溢出 | 同公式同覆盖 | 无差异 |
| 叫醒顺序与一次性 | 同顺序（结构表达） | 无差异 |
| 调试打印 | 无 | 非行为（构建期配置，1.6 节登记） |
| 窄化隐式转换 | 集中显式转换加注释 | 表达强化：行为一致 |

---

## 5. 测试要点

> 测试代码在 `os/servers/input/src/produce.rs` 的测试模块。运行方法：`cargo test -p minix-input`（全 crate 通过，当前 66 个）。

| 测试函数 | 验证什么 | 对应的 C 行为 |
|---------|---------|--------------|
| `test_route_drops_bad_slots` | 负数、超长、极值丢弃 | input.c:383-385（含注释 A-3） |
| `test_route_drops_foreign_source` | 主人不对丢弃 | input.c:387-390 |
| `test_route_prefers_device_then_mux_then_terminal` | 三级跳板四场景 | input.c:404-421 |
| `test_multiplexer_selection_matches_c` | 键盘窗归零、其余归五 | input.c:392-397 |
| `test_forward_copies_lanes` | 五字段原样复制 | input.c:412-417 |
| `test_enqueue_appends_and_reports_overflow` | 追加、满 32、溢出踩旧、位置回绕 | input.c:338-355 |
| `test_wake_prefers_reader_over_selector` | 无人、选择者、挂起优先、撕纸条擦名 | input.c:361-370 |

### 5.1 测试统计（截至 2026-09-05）

- `cargo test -p minix-input`：**66 个通过，0 个失败**。
- 其中与本篇直接相关的 7 个（上表）；其余分属第 01 篇（5 个）、第 02 篇（8 个）、第 03 篇（6 个）、第 04 篇（8 个）、第 06 篇（4 个加共用 1 个）、第 07 篇（9 个加共用 1 个）、第 08 篇（5 个）、第 10 篇（4 个）、第 11 篇（7 个）与错误码模块（2 个）。
- 完整测试清单：`rg "#\[test\]" os/servers/input/src/produce.rs`

---

## 6. 过渡：进来之后，灯光出去

本篇结束时，事件从驱动进来，经过校验落子叫人，全程有了归宿。但服务还有反方向的活：终端说"把灯调成这样"，服务要把命令送到驱动（第 10 篇）；驱动来了走了，槽位要分配回收（第 11 篇）；驱动 side 的代码长什么样（第 12 篇）。下一篇（第 10 篇）讲灯光：掩码怎么广播、为什么记住、重连时怎么从记忆里恢复。

---

## 7. 参见

- 第 03 篇 `03-input-device-structs.md`：槽位号校验；多路器营业性质。
- 第 04 篇 `04-input-event-format.md`：事件四要素含义。
- 第 05 篇 `05-input-message-contract.md`：上报与转交载荷形状；单向纪律。
- 第 07 篇 `07-input-read-suspend.md`：挂起纸条（叫醒的目标）；拷贝一个事件。
- 第 08 篇 `08-input-ioctl-cancel-select.md`：选择者（通知的目标）；取消（叫醒之外的出口）。
- 第 10 篇 `10-input-setleds.md`：灯光（反方向的活）。
- 第 11 篇 `11-input-driver-connect.md`：槽位分配（主人关系的建立）。
- 第 13 篇 `13-tty-consumer.md`：转交的接收方。
- `os/servers/input/src/produce.rs`：本篇全部 Rust 实现。
