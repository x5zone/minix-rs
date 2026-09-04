# 12-libinputdriver: 驱动侧客户端库

> **状态**: 已改写（2026-09-05，首版完整文档）
> **定位**: 驱动 side 的宣告、上报、配置核对、调灯响应、主循环分发（阶段 6，客户端库；阶段内第一次换边）
> **源码**: `minix3/minix/lib/libinputdriver/inputdriver.c`（206 行，全部）+ `minix3/minix/include/minix/inputdriver.h`（回调表与原型，全部）
> **Rust 模块**: `os/libs/minix-sys/src/inputdriver.rs`（注册、判决、核对、分类；传输归未来层）
> **目标读者**: 想理解"键盘驱动 side 如何接入输入服务"的读者（驱动作者视角）。前置知识：第 05 篇（号码载荷）、第 11 篇（服务 side 的连接）、第 02 篇（通知分类）。
> **本章不讲什么**: 传输实现（标签查询、发布、阻塞发送、主循环收信——未来传输层，只定契约）；服务 side（第 01-11 篇，只对照）；具体驱动的硬件细节（第 14 篇）；数据存储服务内部（07-stage-ds）。

---

## 1. 概念：驱动 side 的四件事与一张纸条

### 1.1 驱动的一生：宣告、上报、听命、循环

驱动进程的一生分四幕。第一幕宣告出生：查出自己的注册名，贴条子到布告栏（"某某驱动到了，是键盘/鼠标"），然后等服务来联系——宣告是"我是谁"的单方面广播，不需要回答（单向纪律，第 05 篇）。第二幕上报事件：硬件有动静就组装上报发给服务，发之前看两道门（服务器配好了吗？本类分到槽位了吗？），发的时候用阻塞发送（发不出去就知道服务死了，见 1.2 节）。第三幕听命：服务的配置（"你的槽位是这两个"）与调灯（"灯调成这样"）来了，先验明正身（真是服务发的吗），再照做。第四幕循环：永远等下一封信，分送信——硬件中断、时钟、配置、调灯、其他，各去各的钩子。四幕之外没有第五幕：驱动不维护队列（队列是服务的事，第 09 篇），不记灯光（记忆是服务的事，第 10 篇）——驱动是"手脚"，服务是"脑袋"，手脚只管动，不管记。

### 1.2 阻塞发送的两个理由：不限流与测生死

上报不用异步发送，用阻塞发送，注释写明两个理由（`inputdriver.c:65-71`，难得的长注释，值得全文读）。理由一：异步发送会排队，驱动发得比服务处理得快时，队列越积越长——旧事件堵在队列里，用户的按键延迟越来越大，还占内存；阻塞发送背压到源头（服务忙，驱动等），排队长度恒为零。理由二：阻塞发送失败只有一种可能——服务死了（对端活着，发送必成功）；失败时驱动把服务端点清掉，从此不再发送（等下次配置再连）。异步发送的失败语义是"稍后再说"，阻塞发送的失败语义是"对端没了"——后者恰好是驱动需要的生死信号。用什么发送原语，取决于"失败意味着什么"，这是传输选型的通用原则（背压与生死探测，两个词记住）。

### 1.3 陌生人过滤：配置认标签，调灯认端点

驱动收到配置与调灯，先验明正身再照做，且两种消息验法不同：配置验"标签"（查布告栏"input"标签对应的端点，和发信人比——标签是布告栏的权威说法，不是发信人自称的）；调灯验"端点"（和配置时记下的服务端点比——配置时已经验过标签，调灯时认端点就够了，省一次查询）。两种验法都是"信权威，不信自称"：发信人自称"我是服务"不算数，布告栏说的（配置）或上次验证存的（调灯）才算数。对不上就忽略加日志——和服务 side 的沉默不同（第 09 篇服务 side 沉默），驱动 side 记一笔：驱动的日志是给驱动作者看的排障信息，服务的沉默是防日志洪水（上报是高频的，配置调灯是低频的——频率决定日志政策，这是贯穿全阶段的隐藏规则）。

### 1.4 回调表：四个钩子，没有出生钩

驱动向库登记四个钩子（调灯、中断、闹钟、其他），库在对应消息到来时调用。注意没有"出生钩"与"配置钩"：出生（宣告）是库的事（库知道协议，驱动只给类型掩码），配置是库内处理的（存端点存槽位，驱动不需要知道——驱动上报时库自动填槽位号，见 1.5 节）。钩子全可选（空指针表示"这种消息我不关心"，静默吸收）。四个钩子的签名各不相同（调灯要掩码、中断要中断掩码、闹钟要时间戳、其他要整封信）：签名即需求——钩子要什么，库给什么，不多给（"最小参数"原则：给多了，钩子实现者要忽略；忽略的代码是噪音）。

### 1.5 槽位号驱动不用记：库自动填

驱动调上报接口时不传槽位号（接口参数是"鼠标还是键盘"加事件四要素），库按种类查表填号。为什么驱动不自己记？因为槽位是服务分配的、会变的（服务重起后重新分配，号可能不同，第 11 篇），驱动记了就是缓存、缓存就会过期——"谁分配谁记住"是单一事实来源原则：号归库记（库是配置的接收方），驱动只说"种类"。上报的两道门（1.1 节）中"本类分到槽位了吗"就是查这张表：没分到（服务说满房，第 11 篇 1.5 节），事件静默丢弃——驱动被禁用时不抱怨（抱怨了也没用，服务满房不是驱动能解决的）。

### 1.6 主循环与服务 side 的镜像：同形不同命

驱动主循环和服务主循环（第 02 篇）形状相同（等信、分发、不返回，退出靠终止标志加取消），命运不同：服务循环分发七种请求加其他信箱，驱动循环分发通知三路加配置调灯。形状相同是因为"事件循环"是 Minix3 系统进程的统一心跳（第 01 篇、第 02 篇、第 12 篇三处同形）；命运不同是因为两边是协议的两端（一端发的正是另一端收的，第 05 篇方向图）。读第 12 篇时和第 02 篇对照着看：同样的骨架，不同的血肉——这就是"对称"的含义。

### 1.7 本章小结：手脚的四件事

读完本章，读者应该能不假思索地回答：驱动一生四幕是什么、为什么阻塞发送（两个理由）、配置与调灯验法有何不同（标签与端点）、回调表为什么四个钩子且无出生钩、槽位号为什么库记、主循环与服务 side 有何异同。下一篇（第 13 篇）讲终端：转交事件的接收方。

---

## 2. C 源码分析

> 本章逐段对照原始 C 代码。所有行号以工作区当前 `minix3/` 为准。

### 2.1 状态：三个静态变量（inputdriver.c:11-15）

```c
static endpoint_t input_endpt = NONE;
static int kbd_id = INVALID_INPUT_ID;
static int mouse_id = INVALID_INPUT_ID;

static int running;
```

三个协议状态加一个循环标志。服务端点初始"无"，两槽位初始"未分配"——"没配好"的初始值和"配坏了"的值是同一个（`NONE`/`INVALID`），状态机没有第三态，"没配好"与"配坏了"行为一致（都不发，见 2.3 节）——初始值即哨兵值，哨兵值即禁用值，三位一体。循环标志 `running` 与服务 side 的同名标志（第 02 篇 2.3 节）同形同义。

### 2.2 宣告：查名、贴条、等联系（inputdriver_announce，inputdriver.c:20-37）

```c
void
inputdriver_announce(unsigned int type)
{
        const char *driver_prefix = "drv.inp.";
        char key[DS_MAX_KEYLEN];
        char label[DS_MAX_KEYLEN];
        int r;

        /* Publish a driver up event. */
        if ((r = ds_retrieve_label_name(label, sef_self())) != OK)
                panic("libinputdriver: unable to retrieve own label: %d", r);

        snprintf(key, sizeof(key), "%s%s", driver_prefix, label);
        if ((r = ds_publish_u32(key, type, DSF_OVERWRITE)) != OK)
                panic("libinputdriver: unable to publish up event: %d", r);

        /* Now we wait for the input server to contact us. */
}
```

查自己名字（查不到就崩溃——连自己是谁都不知道的驱动不可能正确工作，和第 01 篇订阅失败崩溃同一逻辑），拼键（前缀加名字，键拼写与服务 side 过滤前缀同源，第 11 篇 2.4 节），发布类型掩码（覆盖写——重起的驱动覆盖旧条子，布告栏永远最新）。两个失败都崩溃：宣告是出生的第一件事，第一件事就办不成，活着也没意义——"早崩溃"哲学在驱动 side 的实例（对照服务 side 第 01 篇 2.4 节的订阅崩溃）。末尾注释"现在等服务联系我们"——宣告之后驱动什么都不做，等配置上门（配置是服务 side 发起的，第 11 篇）：出生顺序是"驱动先吆喝，服务后联系"，吆喝与联系之间隔着布告栏（1.1 节）。

### 2.3 上报：两道门、阻塞发送、生死复位（inputdriver_send_event，inputdriver.c:42-74）

```c
void
inputdriver_send_event(int mouse, unsigned short page, unsigned short code,
        int value, int flags)
{
        message m;
        int id;

        if (input_endpt == NONE)
                return;

        id = mouse ? mouse_id : kbd_id;
        if (id == INVALID_INPUT_ID)
                return;

        memset(&m, 0, sizeof(m));

        m.m_type = INPUT_EVENT;
        m.m_linputdriver_input_event.id = id;
        ...
        if (ipc_send(input_endpt, &m) != OK)
                input_endpt = NONE;
}
```

两道门（服务端点配好、本类槽位分到）都在组装之前——门在组装之前，组装是浪费（组装完再发现没槽位，组装的活白干；门是便宜的整数比较，组装是清零加五次赋值——便宜的先行，是全阶段统一的顺序纪律）。参数用"鼠标还是键盘"布尔选槽位（1.5 节：驱动不记号）。阻塞发送加失败复位（1.2 节：生死信号）。注意失败复位只清端点不清槽位号——槽位号是服务"上次"分配的，服务重起后会重新配置（配置来了全覆盖），留着旧号无害（发送门先查端点，端点没了发不出去，旧号够不着）；清了旧号反而有害（配置只来一半时，剩下一半的旧号还能对照排障）。"清什么留什么"的标准：清的是"继续用的依据"（端点），留的是"等待覆盖的数据"（槽位）。

### 2.4 配置：验标签、存三项、满房提示（do_conf，inputdriver.c:82-111）

```c
        /* Make sure that the sender is actually the input server. */
        if ((r = ds_retrieve_label_endpt("input", &ep)) != OK) { ... return; }

        if (ep != m_ptr->m_source) { ... return; }

        /* Save the new state. */
        input_endpt = m_ptr->m_source;
        kbd_id = m_ptr->m_input_linputdriver_input_conf.kbd_id;
        mouse_id = m_ptr->m_input_linputdriver_input_conf.mouse_id;

        /* If the input server is "full" there's nothing for us to do. */
        if (kbd_id == INVALID_INPUT_ID && mouse_id == INVALID_INPUT_ID)
                printf("libinputdriver: no IDs given, driver disabled\n");
```

注释开门见山（"确认发送者真是输入服务"）。查"input"标签（查不到就忽略——布告栏病了，不是配置错了，不存半截状态，1.3 节），比端点（对不上就忽略——冒名，1.5 节温和失败的驱动 side 实例）。存三项无条件（端点加两槽位，含"未分配"——存"未分配"不是 bug，是"记住自己被禁用"，1.5 节）。两槽全"未分配"提示"没分到号，驱动禁用"（提示，不是报错：满房是 transient，第 11 篇 1.5 节）。注意"服务崩溃重起后配置会再来一次"（注释第 78-80 行）——配置是幂等的（存三项，存几遍结果一样），幂等是重起恢复的基础（重起的服务把配置重发一遍，驱动状态自动回到正轨，不需要"重连握手"这种额外协议）。

### 2.5 调灯：认端点、调回调、可空（do_setleds，inputdriver.c:119-135）

```c
static void
do_setleds(struct inputdriver *idp, message *m_ptr)
{
        unsigned int mask;

        if (m_ptr->m_source != input_endpt) { ... return; }

        mask = m_ptr->m_input_linputdriver_setleds.led_mask;

        if (idp->idr_leds)
                idp->idr_leds(mask);
}
```

认端点（配置时存的，不查标签——1.3 节两种验法的第二种），取掩码，回调可空（没灯的驱动（鼠标）调灯回调是空指针，空就吸收——"可空钩子静默吸收"是回调表的统一语义，四个钩子人人如此：中断闹钟其他三处同样先判空再调，2.6 节）。文件头注释解释"为什么是掩码而不是逐个灯事件"（"图方便"，原文"for convenience reasons only"——作者诚实得可爱：没有深奥理由，就是方便。教学文档如实转述，不替作者编造深刻）。

### 2.6 分发：通知三路、消息三向、永不回答（inputdriver_process，inputdriver.c:141-172）

```c
void
inputdriver_process(struct inputdriver *idp, message *m_ptr, int ipc_status)
{
        /* Check for notifications first. */
        if (is_ipc_notify(ipc_status)) {
                switch (_ENDPOINT_P(m_ptr->m_source)) {
                case HARDWARE:
                        if (idp->idr_intr)
                                idp->idr_intr(m_ptr->m_notify.interrupts);
                        break;

                case CLOCK:
                        if (idp->idr_alarm)
                                idp->idr_alarm(m_ptr->m_notify.timestamp);
                        break;

                default:
                        if (idp->idr_other)
                                idp->idr_other(m_ptr, ipc_status);
                }

                return;
        }

        switch (m_ptr->m_type) {
        case INPUT_CONF:            do_conf(m_ptr);             break;
        case INPUT_SETLEDS:         do_setleds(idp, m_ptr);     break;
        default:
                if (idp->idr_other)
                        idp->idr_other(m_ptr, ipc_status);
        }
}
```

通知先行（和服务 side 第 02 篇 2.4 节同形）：硬件中断走中断钩，时钟走闹钟钩，其余走其他钩——钩子空就吸收（判空调用，四个 `if` 如出一辙）。消息按号：配置、调灯、各归其函数（2.4、2.5 节），其余走其他钩。注释收束全章（"输入协议全单向，所以永不回答"，1.1 节单向纪律的驱动 side 实例）。和服务 side 分发对照：服务 side 有门卫（重起幽灵，第 02 篇），驱动 side 无门卫——驱动不需要门卫（驱动不记"谁开过"，只记"服务是谁"；重起的服务发新配置覆盖旧状态，天然免疫幽灵）。

### 2.7 回调表与原型（inputdriver.h，全部）

```c
struct inputdriver {
        void (*idr_leds)(unsigned int leds);
        void (*idr_intr)(unsigned int mask);
        void (*idr_alarm)(clock_t stamp);
        void (*idr_other)(message *m_ptr, int ipc_status);
};
```

四个钩子，无出生钩（1.4 节）。原型五个（宣告、上报、分发、终止、主循环）——终止与主循环和服务 side 同形（1.6 节），分发是本章，宣告上报是 2.2-2.3 节。头文件 30 行，无多余一字：回调表加原型，库的全部契约。

### 2.8 主循环与终止（inputdriver_task/terminate，inputdriver.c:177-206）

与字符驱动主循环（第 02 篇 2.3 节）同形：运行标志、收任意信、信号加停标志则退、否则崩溃、分发。终止函数清标志加取消收信。同形不同命（1.6 节）。Rust 一侧主循环归传输层（收信发信是传输），分类（本章已定）与钩子（本章已定）先行——循环是胶水，胶水等传输（第 11 篇 3.4 节的纯胶水分离，同手法）。

### 2.9 覆盖核对

| 符号 | 源码位置 | 本文档位置 | Rust 对应（minix-sys inputdriver.rs） |
|------|---------|-----------|-------------------------------------|
| 三静态变量 | inputdriver.c:11-15 | 2.1 节 | `DriverRegistration`（三字段） |
| `inputdriver_announce` | inputdriver.c:20-37 | 2.2 节 | `announce_key` + `announce_type`（查名发布归传输层） |
| `inputdriver_send_event` | inputdriver.c:42-74 | 2.3 节 | `decide_report` + `note_server_lost`（组装发送归传输层） |
| `do_conf` | inputdriver.c:82-111 | 2.4 节 | `verify_conf_sender` + `apply_conf`（查询归传输层） |
| `do_setleds` | inputdriver.c:119-135 | 2.5 节 | `accept_setleds`（回调归驱动） |
| `inputdriver_process` | inputdriver.c:141-172 | 2.6 节 | `classify_incoming` + `DriverIncoming` |
| 回调表 | inputdriver.h:11-16 | 2.7 节 | `DriverHooks`（四钩子，无 trait） |
| 原型 | inputdriver.h:19-25 | 2.7 节 | 各函数归属（传输层待定） |
| task/terminate | inputdriver.c:177-206 | 2.8 节 | 循环归传输层（分类钩子先行） |

---

## 3. Rust 设计决策

> 本章解释"为什么这样设计"。每个决策先说备选方案，再说选择的理由。

### 3.1 静态变量变结构体：一进程一驱动的限制要可见

C 用三个文件静态变量记住服务端点与两槽位。Rust 装进 `DriverRegistration` 结构体。理由：静态变量让"一进程一驱动"成为 ambient 的事实（代码里看不见限制，用两个驱动就踩坑）；结构体让限制可见（想管两个驱动？实例化两个结构体——能编译，但主循环一次只能跑一个，限制从"看不见"变成"看得见但需要绕"，绕的时候自然会想"我是不是该有两个进程"——答案是该，Minix3 驱动本就是一进程一设备类）。可见的限制比隐藏的限制好：隐藏的限制在违反时爆炸，可见的限制在违反前提醒。

### 3.2 传输一律在外：查名、发布、发送、收信都不碰

`announce_key` 只拼键（查名与发布是传输），`decide_report` 只判决（组装与发送是传输），`verify_conf_sender` 只比端点（标签查询是传输），`classify_incoming` 只分类（收信是传输）。理由和 `devman_client` 一致（同库先例）：传输没落地之前，任何"调传输"的代码都是 `todo!()`，`todo!()` 进生产构建等于埋雷；纯函数现在就能测（本章 11 个测试零传输依赖）。边界标准统一：第 11 篇 3.4 节的纯胶水分离，同手法第四次（前三次：06、07、08）。

备选方案是调库里的 `todo!()` 传输桩（`minix_sys::send` 就是桩）。拒绝的理由：测试一旦碰到桩就崩溃，纯函数的测试必须绕开桩——绕桩的测试比函数本身还复杂；且桩落地的那天，所有调用处要重审。纯函数加传输待定，桩落地时只加新代码，不改旧代码。

### 3.3 槽位用 Option，不用哨兵

C 用 `INVALID_INPUT_ID`（-1）表示"没分到"，Rust 用 `Option<i32>`（`None`）。理由：第 03 篇 3.1 节的双类型思想在这里的延续——哨兵值是"用魔法数字表达缺席"，`Option` 是"用类型表达缺席"。转换函数 `slot_or_none` 守在边界（配置存入时转一次），内部流转只有 `Option`——哨兵只在线缆上出现一次（构造解码处，第 05 篇），进库就消失。备选方案是 `i32` 加哨兵比较（C 直译）。拒绝的理由：每个 `if id == INVALID` 都是"记得检查"的负担，漏一次就是把"没分到"当"分到负一号"用；`Option` 漏处理编译失败（`match` 穷举/`if let`），负担归编译器。

### 3.4 钩子用 Option 函数指针，不用 trait

`DriverHooks` 四个 `Option<fn>` 字段，无 trait。理由：trait 需要两个行为不同的实现才有意义（review 规则 §2.5），一进程一驱动只有一个实现——单实现 trait 是装饰。C 的回调表就是四个函数指针，Rust 的 `Option<fn>` 是它的诚实翻译（注意：这是"翻译结构"不是"翻译逻辑"——逻辑（分发、核对、判决）全重写成枚举与纯函数，只有"钩子可空"这个结构保留，因为"可空"本身就是语义）。函数地址永不比较（clippy 规则，注释写明），存在性用 `is_some` 查。

### 3.5 配置幂等写进测试，不止写进文档

`apply_conf` 存三项可重复调，测试连调两次断言状态相同（隐含在 `test_conf_sender_check_matches_c` 的"接受后存储"段——连存两次同一值，结果不变，幂等由"无条件覆盖"的结构保证）。理由：幂等是重起恢复的基础（2.4 节），基础要用测试锁死，不能只靠"结构上显然"。实际上"存三项"怎么调都不可能不幂等——测试的意义不在"发现 bug"，在"声明保证"：后人重构（比如加"只在变化时存"的优化）时，测试会问他"幂等还要不要"。

---

## 4. 实现详解

> 完整代码在 `os/libs/minix-sys/src/inputdriver.rs`。本章按"注册、宣告、上报、配置、调灯、钩子、分类"的顺序展开，每个小节标注对应的第 3 章决策。

### 4.1 注册：`DriverRegistration`（对应决策 3.1、3.3）

```rust
pub struct DriverRegistration {
    pub server: Option<Endpoint>,       // 服务端点（无即未配置）
    pub keyboard_slot: Option<i32>,    // 键盘槽（无即未分配）
    pub mouse_slot: Option<i32>,       // 鼠标槽（同上）
}
```

`new` 全无（C 静态初值），`is_connected` 快检，`apply_conf` 存三项（含"未分配"照存——禁用状态是合法状态，不是错误），`is_disabled` 双无判定，`note_server_lost` 忘端点留槽位（2.3 节清留标准）。`Default` 派生等价（`new` 相同，clippy 规则）。

### 4.2 宣告：`announce_key` 与 `announce_type`（对应决策 3.2）

```rust
pub fn announce_key(label: &str) -> String  // "drv.inp.<label>"（共享前缀）
pub const fn announce_type(is_keyboard: bool, is_mouse: bool) -> u16  // 两位掩码
```

键拼写用共享前缀常量（第 05 篇本轮新增，两端同源）。掩码用两布尔组装（调用者摆不出保留位，1.1 节 pckbd 双类型场景由调用者传两个真）。

### 4.3 上报：`ReportVerdict` 与 `decide_report`（对应决策 3.2、3.3）

```rust
pub enum ReportVerdict {
    Send { slot: i32 },  // 发，槽位号附上
    DropUnconnected,     // 没配好，静默
    DropUnassigned,      // 本类没分到，静默
}
```

两道门两种静默（1.1 节含义不同，变体不同）。槽位号由库填（1.5 节），调用者只说种类（布尔）。

### 4.4 配置：`ConfOutcome` 与 `verify_conf_sender`（对应决策 3.2）

三变体（接受、外人、查不到），标签查询归传输层（参数 `looked_up` 是查好的端点，不是标签名——函数签名即边界：传端点进来，不传标签名，查标签的动作在墙外）。

### 4.5 调灯：`SetledsOutcome` 与 `accept_setleds`（对应 1.3 节）

认端点（配置时存的），陌生发送者与未配置同归忽略。回调可空由驱动侧在调用前检查（C 的 `if (idp->idr_leds)` 在 Rust 侧是 `if let Some`）——检查发生在库外、驱动进程内调用回调的地方，不在本模块。

### 4.6 钩子：`DriverHooks`（对应决策 3.4）

四钩子全可选，默认全空（`Default`）。无 trait（决策 3.4），无比较（注释写明）。

### 4.7 分类：`DriverIncoming`、`NotifyKind`、`classify_incoming`（对应 2.6 节）

```rust
pub enum DriverIncoming {
    HardwareInterrupt, ClockAlarm, OtherNotify,  // 通知三路
    Configure, SetLights, OtherMessage,          // 消息三向
}
```

通知先行（与服务 side 第 02 篇同形），消息按号（配置、调灯、其余）。纯函数（通知种类由传输层映射端点，参数进，不查表）。`tty_numbers_disjoint` 钉死号段不交（测试锁死——未来有人在输入号段附近加号，加到 TTY 家门口时测试会响）。

### 4.8 与 C 的差异说明

| C 行为 | Rust 对应 | 差异分类 |
|--------|----------|---------|
| 三静态变量 | 结构体三字段 | 表达强化：行为一致（单驱动语义不变） |
| 宣告查名发布 | 拼键加掩码组装（查发归传输层） | 设计决策：纯胶水分离 |
| 上报两道门阻塞发送 | 两道门判决（组装发送归传输层） | 同上 |
| 配置验标签存三项 | 验端点（查询归传输层）加存储 | 同上 |
| 调灯认端点调回调 | 同认同调（回调驱动侧） | 无差异 |
| 分发通知消息六向 | 同六向分类 | 无差异 |
| 回调表四钩子 | 四 Option 钩子（无 trait） | 表达强化：行为一致 |
| 主循环 | 归传输层（分类钩子先行） | 设计决策：胶水待定 |
| 崩溃（查名发布失败、收信失败） | 显式结局（忽略/禁用/重配） | 架构演进 A-7（库不替驱动决定生死） |

---

## 5. 测试要点

> 测试代码在 `os/libs/minix-sys/src/inputdriver.rs` 的测试模块。运行方法：`cargo test -p minix-sys`（全 crate 通过，当前 22 个）。

| 测试函数 | 验证什么 | 对应的 C 行为 |
|---------|---------|--------------|
| `test_announce_spelling_matches_c` | 键拼写、掩码四组合 | inputdriver.c:23,32（类型掩码语义） |
| `test_registration_lifecycle_matches_c` | 初值、配置存储、禁用、丢端点留槽 | inputdriver.c:11-13,49-54,72-73,103-110 |
| `test_conf_sender_check_matches_c` | 三结局、接受后存储双槽 | inputdriver.c:88-106 |
| `test_setleds_source_check_matches_c` | 本人调用、陌生忽略、未配置忽略 | inputdriver.c:124-129 |
| `test_classify_routes_like_inputdriver_process` | 通知三路、消息三向、空钩吸收 | inputdriver.c:145-171 |
| `test_number_families_stay_disjoint` | 号段不交 | com.h 号段布局 |

### 5.1 测试统计（截至 2026-09-05）

- `cargo test -p minix-sys`：**22 个通过，0 个失败**。
- 其中与本篇直接相关的 6 个（上表）；其余 16 个为既有测试（回归无破坏）。
- 完整测试清单：`rg "#\[test\]" os/libs/minix-sys/src/inputdriver.rs`

---

## 6. 过渡：客人讲完了，邻居登场

本篇结束时，输入世界的主角（服务）与客人（驱动）都讲完了：服务九篇（01-11），驱动一篇（12）。剩下两篇是邻居：终端（第 13 篇，转交事件的接收方、调灯的发起方）与键盘驱动（第 14 篇，客户端库的第一个真实用户、中断与端口的拥有者）。第 13 篇验证第 09 篇的转交（转交的事件终端怎么吃），第 14 篇验证第 12 篇的库（库的接口盖不盖得住真实驱动）。对称的另一半在后面。

---

## 7. 参见

- 第 05 篇 `05-input-message-contract.md`：号码载荷（本篇用的合同）；单向纪律。
- 第 11 篇 `11-input-driver-connect.md`：服务 side 连接（本篇的对端）；键前缀两端对照。
- 第 02 篇 `02-chardriver-framework.md`：通知分类（同形）；主循环（同形）。
- 第 09 篇 `09-input-event-processing.md`：上报的接收方（本篇发的对端）。
- 第 10 篇 `10-input-setleds.md`：调灯的发送方（本篇接的对端）。
- 第 14 篇 `14-pckbd-driver.md`：第一个真实用户（本篇接口的验证方）。
- `os/libs/minix-sys/src/inputdriver.rs`：本篇全部 Rust 实现。
