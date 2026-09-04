# 11-input-driver-connect: 驱动连接与断开

> **状态**: 已改写（2026-09-05，首版完整文档）
> **定位**: 槽位分配、连接配置、断开清理、到来检查（阶段 5，驱动生命周期；主人关系的建立与解除）
> **源码**: `minix3/minix/servers/input/input.c:430-603`（`input_alloc_id`/`input_connect`/`input_disconnect`/`input_check`）+ 数据存储订发布面（`ds.h:25-69` 使用面； `drv.inp.` 前缀两端对照第 12 篇）
> **Rust 模块**: `os/servers/input/src/connect.rs`（分配、连接报告、断开效应、到来过滤）
> **目标读者**: 想理解"驱动来了怎么分房、走了怎么送客、服务怎么知道谁来了谁走了"的读者。前置知识：第 03 篇（槽位）、第 05 篇（配置载荷与单向纪律）、第 10 篇（灯光记忆）。
> **本章不讲什么**: 驱动 side 的宣告与组装（第 12 篇，只对照）；数据存储服务内部实现（07-stage-ds，只用使用面）；事件上报（第 09 篇）；灯光发送（第 10 篇，只读记忆）。

---

## 1. 概念：旅馆的前台账本

### 1.1 把槽位想象成旅馆房间

十个槽位是十间房（第 03 篇），驱动是客人，服务是前台。客人到来不直接敲门——它先去数据存储服务（全系统的布告栏）贴一张条子"某某驱动到了，是键盘/鼠标"；前台订阅了布告栏的"输入驱动"分区（第 01 篇），有新条子就被叫醒，核对身份后分房。客人离开同样先撕条子，前台发现条子没了就退房。布告栏是双方唯一的联络点：前台从不直接和客人谈"你来了"，客人从不直接和前台谈"我走了"——全经布告栏。这种间接带来一个后果：前台看到的永远是"条子的状态"，不是"客人的状态"，两者有延迟（客人死了但条子还没撕，前台以为客人还在——1.4 节讲这个延迟怎么办）。

### 1.2 分房三规则：认牌子、找空房、满了就说没房

分房按键盘、鼠标两类分别进行（键盘客人住 1-4 房，鼠标客人住 6-9 房，多路器房不住客人，见 2.1 节）。三条规则按顺序：

```
规则一  认牌子：同名同类的老客人回来了 → 住回原来的房（主人刷新成新的进程身份），完事。
         |-- 牌子对上，直接回房，不找空房（老客人优先）。
规则二  找空房：从小编号往大找，第一间"没主人且没客人"的房。
         |-- 注意两个条件都要：没主人（上个客人走了）且没客人（没读者在住——读者住着的房不能给新客人，见 1.3 节）。
规则三  满了就说没房：回"未分配"，客人拿着"未分配"配置，该干嘛干嘛（通常是静默——驱动被 effectively 禁用，见 1.5 节）。
```

规则一先于规则二：老客人回来，宁可住回原来的房（读者的队列、灯的记忆都在那间房里，第 10 篇），也不另开新房—— continuity 比"负载均衡"重要得多（四间房谈什么均衡）。

### 1.3 住着读者的空房不能给：断开但没关的保护

规则二有个容易忽略的限定："没主人**且**没客人"。主人走了（断开）但读者还在（没关）的房间，跳过不分。为什么？房间里还有读者的东西：队列里的事件是旧驱动上报的（读者可能正在读），挂起的联系方式是旧读者的（第 07 篇）。把新驱动塞进来，新事件和旧事件混在一个队列，读者分不清谁是谁——"新旧混读"比"暂时没房"糟得多。保护的代价是房间利用率下降（断开没关的房间空占着），但断开没关是过渡态（读者迟早关，第 06 篇），过渡态的保守是对的。这个限定和第 06 篇的关闭修正是同一枚硬币的两面：关闭修正管"读者走时带走挂起"，这里管"驱动走时别动读者的房"。

### 1.4 退房：叫醒、通知、撕牌子，不动行李

驱动离开（条子被撕），前台退房分三步：挂起的读用"输入输出错误"叫醒（驱动没了，等下去没意义——和取消的"被打断"不同，这是"白等了"）；预约的选择者通知"可读"（他再来查，会发现设备没营业，拿错误走人——第 08 篇的"就绪即报错"在此兑现）；牌子撕掉（主人清空）。**不动**的更多：队列不动（旧事件留着——新驱动来之前如果有人读，读到的是旧事件；这是有意的：事件是事实发生过的，不因驱动离开而作废，过期与否由读者判断）、打开标志不动（读者的持有关系没变）、牌子上的名字不动（下次同名客人回来认牌子用，规则一）、灯的记忆不动（第 10 篇）。退房只解除"主人关系"，不碰"读者关系"和"记忆"——三种关系独立，这是本章最核心的不变量。和第 06 篇关闭对比：关闭解除读者关系（清队列清挂起），退房解除主人关系（叫醒挂起但不清队列）——"谁的关系谁解除"，对称而完整。

### 1.5 满房与查无此人：两种温和的失败

分配失败（满房）不报错：配置照发，槽位号填"未分配"，驱动收到后静默（通常禁用自己）。为什么失败不算失败？因为"满房"是 transient 的（别的驱动走了一个就有房），报错会让驱动崩溃重起、重起再来问、还是满房——崩溃循环。静默让驱动活着等（等别的房空出来，下次重连还有机会）。连接时标签对不上（冒名）更温和：记一笔日志，return，连配置都不发——冒名的消息不值得任何回答（单向协议本来就不回答，第 05 篇），日志是给管理员看的（"有人冒充驱动"，安全事件）。

### 1.6 到来检查：轮询布告栏的双向对账

服务被数据存储服务的通知叫醒后，做双向对账：正向，把布告栏里所有"输入驱动"条子过一遍（前缀过滤，非本分区条子跳过——订阅模式是正则，偶尔混进别家条子是正常的），逐个连接（标签核对、分配、发配置、恢复灯光）；反向，把自己账上所有有主人的槽位过一遍，查条子还在不在——条子没了（查无此人）就退房，条子还在就刷新主人（进程重起后身份可能变，刷新是免费的——"反正查都查了，顺手更新"，注释原话"not really necessary"，诚实得可爱）。双向缺一不可：只有正向，死驱动的房永远占着；只有反向，新驱动永远住不进。

### 1.7 本章小结：来有房、走送客、账要对

读完本章，读者应该能不假思索地回答：分房三规则是什么顺序、为什么老客人优先、为什么住着读者的空房不能给、退房哪三步动哪三不动、满房与冒名为什么温和、双向对账为什么缺一不可。下一篇（第 12 篇）讲布告栏的另一边：驱动 side 如何贴条子、组装上报、核对配置、响应调灯。

---

## 2. C 源码分析

> 本章逐段对照原始 C 代码。所有行号以工作区当前 `minix3/` 为准。

### 2.1 分房：认牌子、找空房、满房回空（input_alloc_id，input.c:430-470）

```c
static int
input_alloc_id(int mouse, endpoint_t owner, const char *label)
{
        int n, id, start, end;

        if (!mouse) {
                start = FIRST_KBD_DEV;
                end = LAST_KBD_DEV;
        } else {
                start = FIRST_MOUSE_DEV;
                end = LAST_MOUSE_DEV;
        }

        id = INVALID_INPUT_ID;
        for (n = start; n <= end; n++) {
                if (devs[n].owner != NONE) {
                        if (!strcmp(devs[n].label, label)) {
                                devs[n].owner = owner;
                                return n;
                        }
                /* Do not allocate the ID of a disconnected but open device. */
                } else if (!devs[n].opened && id == INVALID_INPUT_ID) {
                        id = n;
                }
        }

        if (id != INVALID_INPUT_ID) {
                devs[id].owner = owner;
                strlcpy(devs[id].label, label, sizeof(devs[id].label));

                ...
        } else {
                printf("INPUT: out of %s slots for new driver %d\n",
                    mouse ? "mouse" : "keyboard", owner);
        }

        return id;
}
```

窗口按 `mouse` 标志二选一（键盘 1-4，鼠标 6-9——多路器房 0 与 5 永远不在分配范围，前台账上只有 1-4 与 6-9 可分，第 03 篇布局）。循环内两分支：有主人的比牌子（字符串比较，对上就刷新主人、当场返回——注意返回前不碰队列灯光挂起：老客回房，房里一切照旧）；没主人的看两条件（没读者且还没记下空房，就记下——`id == INVALID` 保证记第一间，first-fit）。循环后：有空房就上牌（主人加名字，名字用安全拷贝防溢出），没空房就记一笔"没房了"日志。返回空房号或"未分配"。first-fit 的公平性不需要：四间房，先到先得是最简单的公平。

名字比较用 `strcmp`（字节比较，遇到零字节停）——Rust 一侧用"存到零字节为止的切片比较"复刻（第 3 章），截断过的长名字行为一致（存的时候截，比较的时候停，同一套零字节语义）。

### 2.2 连接：核对、分配、发配置、恢复灯光（input_connect，input.c:475-528）

```c
        /* Check the driver's label. */
        if ((r = ds_retrieve_label_name(label, owner)) != OK) { ... return; }
        if (strcmp(label, labelp)) { ... return; }
        ...
        if (typemask & INPUT_DEV_KBD)
                kbd_id = input_alloc_id(FALSE /*mouse*/, owner, label);
        if (typemask & INPUT_DEV_MOUSE)
                mouse_id = input_alloc_id(TRUE /*mouse*/, owner, label);

        memset(&m, 0, sizeof(m));

        m.m_type = INPUT_CONF;
        m.m_input_linputdriver_input_conf.kbd_id = kbd_id;
        m.m_input_linputdriver_input_conf.mouse_id = mouse_id;
        m.m_input_linputdriver_input_conf.rsvd1_id = INVALID_INPUT_ID;
        m.m_input_linputdriver_input_conf.rsvd2_id = INVALID_INPUT_ID;

        if ((r = asynsend3(owner, &m, AMF_NOREPLY)) != OK) ... ;

        /* If a keyboard was registered, also set its initial LED state. */
        if (kbd_id != INVALID_INPUT_ID)
                input_set_leds(devs[kbd_id].minor, devs[kbd_id].leds);
```

先核对牌子（查主人的注册名，对不上条子上的名字就忽略——1.5 节的冒名处理；查不到也忽略：布告栏与注册表之间的竞态，条子先到注册后到，等下次通知再连）。再按类型掩码位分配（键盘位分键盘房，鼠标位分鼠标房，两位都有就分两间——笔记本键盘加指点杆的复合驱动，第 14 篇）。初始槽位号先填"未分配"，分配失败保持"未分配"（注释大段解释：失败也发配置，驱动拿"未分配"静默——1.5 节）。配置消息组装（保留槽填"未分配"，注释标"摇杆？"与"未来用"——第 05 篇 2.2 节），异步发送（单向，失败记日志）。最后，有键盘房就恢复灯光（读记忆，第 10 篇——连接是记忆的读者，1.2 节的遗嘱在此兑现）。

### 2.3 退房：叫醒、通知、撕牌子（input_disconnect，input.c:533-553）

```c
static void
input_disconnect(struct input_dev *input_dev)
{
        ...
        if (input_dev->suspended) {
                chardriver_reply_task(input_dev->caller, input_dev->req_id,
                    EIO);
                input_dev->suspended = FALSE;
        }

        if (input_dev->selector != NONE) {
                chardriver_reply_select(input_dev->selector, input_dev->minor,
                    CDEV_OP_RD);
                input_dev->selector = NONE;
        }

        input_dev->owner = NONE;
}
```

三步与 1.4 节逐行对应：挂起用"输入输出错误"叫醒（不是"被打断"——取消是人为打断，断开是天灾断联，错误号不同，调用者能区分"有人取消我"与"驱动没了"）；选择者通知可读（他再来查就拿错误，第 08 篇语义在此闭环）；主人清空。不动的（队列、打开、名字、灯）在代码里表现为缺席——缺席即设计（1.4 节），不是遗漏（和第 06 篇关闭的缺席是 bug 对照着看：同一作者，一处缺席是 bug，一处缺席是设计——区分的标准永远是"有没有证据链"，第 06 篇 2.3 节的方法在此同样适用：退房不清队列的证据是"事件是事实"的语义，关闭不清挂起的反证据是 VFS 路径）。

### 2.4 到来检查：双向对账（input_check，input.c:558-603）

```c
        /* Check for new (input driver) entries. */
        while (ds_check(key, &type, &owner) == OK) {
                if ((r = ds_retrieve_u32(key, &value)) != OK) { ... continue; }

                /* Only check for input driver registration events. */
                if (strncmp(key, driver_prefix, len))
                        continue;

                /* The prefix is followed by the driver's own label. */
                label = &key[len];

                input_connect(owner, label, value);
        }

        /* Check for removed (label) entries. */
        for (i = 0; i < INPUT_DEV_MAX; i++) {
                /* This also skips the multiplexers. */
                if (devs[i].owner == NONE)
                        continue;

                r = ds_retrieve_label_endpt(devs[i].label, &owner);

                if (r == OK)
                        devs[i].owner = owner;  /* not really necessary */
                else if (r == ESRCH)
                        input_disconnect(&devs[i]);
                else
                        printf(...);
        }
```

正向循环读布告栏更新（`ds_check` 逐条吐出，读一条处理一条）：取值失败跳过（条子与值之间的竞态，值还没写好，下次再来）；前缀不对跳过（订阅正则偶尔混进别家）；前缀对就连（标签是前缀之后的部分——指针加法，不拷贝）。反向循环查账上有主人的槽位（无主跳过——注释点明"顺带跳过多路器"，多路器恒无主，第 03 篇）：条子还在就刷新主人（注释"其实没必要"，但顺手，免费）；条子没了（查无此人，`ESRCH`）就退房；别的错误记日志（布告栏自己病了，不是驱动的事，不退房——"布告栏病了就撕客人的房"是错的，错误要归因）。

### 2.5 数据存储使用面（ds.h:25-69）

```c
#define DSF_OVERWRITE           0x01000
#define DSF_INITIAL             0x02000
int ds_publish_u32(const char *name, u32_t val, int flags);
int ds_retrieve_u32(const char *name, u32_t *val);
int ds_retrieve_label_name(char *ds_name, endpoint_t endpoint);
int ds_retrieve_label_endpt(const char *ds_name, endpoint_t *endpoint);
int ds_subscribe(const char *regex, int flags);
int ds_check(char *ds_name, int *type, endpoint_t *owner_e);
```

本篇用六个调用、两个标志：订阅用正则加"立即检查一次"（第 01 篇），发布用覆盖（驱动侧，第 12 篇），取值、名查端点、端点查名、逐条检查（本篇正反向）。数据存储服务内部实现是 07-stage-ds 的事，本篇只用"布告栏"的六个动作——使用面契约（plan A-2）：键以前缀命名、值是类型掩码、删键即死亡（`ESRCH`）。键前缀两端的拼写（发布侧 `drv.inp.` 加标签，过滤侧 `drv.inp.` 前缀比较）由共享常量锁死（第 05 篇本轮新增，第 12 篇同用）。

### 2.6 覆盖核对

| 符号 | 源码位置 | 本文档位置 | Rust 对应（connect.rs） |
|------|---------|-----------|------------------------|
| `input_alloc_id` | input.c:430-470 | 2.1 节 | `alloc_id`（认牌子找空房满房） |
| `input_connect` | input.c:475-528 | 2.2 节 | `connect_driver` + `wants_from_typemask`（核对归传输层） |
| `input_disconnect` | input.c:533-553 | 2.3 节 | `disconnect_device` + `DisconnectEffects` |
| `input_check` | input.c:558-603 | 2.4 节 | `key_is_new_driver`（过滤；循环归传输层） |
| 数据存储使用面 | ds.h:25-69 | 2.5 节 | 键前缀共享常量（05 篇） |
| 标签核对 | input.c:487-495 | 2.2 节 | 归传输层（本篇声明去向） |

---

## 3. Rust 设计决策

> 本章解释"为什么这样设计"。每个决策先说备选方案，再说选择的理由。

### 3.1 分配是独占可变借用的单函数，不拆判断执行

`alloc_id` 拿整个表格的可变借用，扫描认牌、找空、上牌一气呵成——第 06 篇的判断执行分离在这里**不适用**。理由：分配的判断（哪间空）与执行（占下它）之间不能插任何东西：查到空房和占下空房必须是原子动作（单线程里"原子"意味着"同一函数内完成"，中间不返回）。拆成"先看房再占房"，两次调用之间（单线程没有之间——但代码结构上拆开就是邀请未来的人在中间加东西）是隐患。C 同样是一个函数（`input_alloc_id` 查改一体），这里不是"没拆"，是"不该拆"。判断执行分离是手段（可测性），不是教条；分配的可测性由"初始表格→调用→断言表格"端到端保证，不需要中间状态可见。

备选方案是硬拆（`find_slot` 纯查加 `claim_slot` 执行）。拒绝的理由如上：拆开的两个函数之间， tablas 状态可能被谁改——单线程里没人改，但类型签名保证不了"没人改"（两个 `&mut` 先后借用，编译器允许中间插代码）。不拆，编译器连"中间插代码"的机会都不给。

### 3.2 连接报告是数据，不是动作

`connect_driver` 返回 `ConnectReport`（两槽位加灯光恢复项），不发配置、不调置灯。理由：发配置与恢复灯光是传输与跨模块调用（配置走异步发送，第 05 篇载荷；恢复调第 10 篇逻辑），纯函数不碰它们——第 06 篇 3.1 节的分离手法在本章第四次使用。报告把"该发的配置内容"与"该恢复的灯"一次装好，分发层照单执行：先发配置（无条件，即使两槽全空——1.5 节），再恢复灯光（有键盘槽才恢复）。报告结构体是"待办清单"的数据形态。

### 3.3 断开效应与关闭清理对照设计，不合并

`disconnect_device` 与第 06 篇 `apply_close` 长得很像（都碰挂起与选择者），但故意不合并、不复用。理由：两者语义相反——关闭清队列不清主人（主人本来就没有，关闭是读者的事），断开不清队列只撕主人（读者还在，队列是读者的东西）。合并成一个"清理函数"加标志位（`clear_queue: bool`），是"用参数区分语义"的坏味道：调用者传错标志位，关闭变断开，悄无声息。两个函数各做各的，名字即语义，误用在编译期表现为"调错函数"（review 时一眼看见），而不是"传错参数"（review 时需要查标志含义）。"相似的代码"与"相同的代码"只差一个误用成本，成本高的不合并。

### 3.4 到来过滤是纯字符串函数，循环归传输层

`key_is_new_driver` 只做前缀判断，布告栏轮询循环（`ds_check` 的 `while`）归传输层。理由：循环的终止条件依赖传输返回值（`ds_check` 返回非 OK 即停），纯函数表达不了"读一条处理一条"——硬表达需要把传输回调当参数传进来（依赖注入），为一个 `while` 引入一套注入机制，得不偿失。过滤条件（前缀比较）是纯逻辑，可测（`test_arrival_filter_matches_c`），循环是胶水，胶水等传输落地。纯与胶水的分界标准：能用输入输出讲清的是纯，需要外部世界配合的是胶水。

### 3.5 标签比较复刻零字节语义，不简化为字符串相等

`label_eq` 比较"存到零字节为止"与"入参到零字节为止"，而不是直接比较字节数组。理由：C 的 `strcmp` 在零字节停，存储用安全拷贝截断——两种零字节语义叠加后，"截断过的长名字"与"完整长名字"比较必须不等（存的短，入参的长，strcmp 在存的零字节处停，两边长度不同→不等），与"短名字"比较必须等。直接比较 80 字节数组在"入参短、存的长"的场景下会错（存的尾部是零，入参没尾部——长度都不一样）。零字节语义是 C 字符串的灵魂，复刻它，而不是"看起来差不多"的数组比较。测试覆盖三种情况（相等、前缀不等、截断一致）——最后一种最容易错，单列一个断言。

---

## 4. 实现详解

> 完整代码在 `os/servers/input/src/connect.rs`。本章按"过滤、分配、连接、断开"的顺序展开，每个小节标注对应的第 3 章决策。

### 4.1 过滤：`key_is_new_driver`（对应决策 3.4）

```rust
pub fn key_is_new_driver(key: &str) -> Option<&str> {
    key.strip_prefix(DRIVER_KEY_PREFIX)  // input.c:578-582
}
```

前缀常量来自共享侧（第 05 篇本轮新增），两端同拼写。返回标签部分（不拷贝，切片引用——标签的后续核对只读不写，引用足够）。

### 4.2 分配：`alloc_id` 与 `label_eq`（对应决策 3.1、3.5）

```rust
pub fn alloc_id(
    table: &mut InputTable,
    mouse: bool,       // 窗口选择（input.c:435-441）
    owner: Endpoint,   // 新主人
    label: &[u8],      // 牌子（字节比较，零字节语义）
) -> Option<DeviceIndex>  // 空房或复用成功；满房 None（C 的 INVALID）
```

窗口常量复用第 03 篇（`FIRST_KEYBOARD_INDEX` 等，无重复定义）。扫描低到高：有主人比牌子（对上刷新返回），无主人无读者记第一间。落定后上牌（主人加安全拷贝名字，`set_label` 第 03 篇）。`label_eq` 实现零字节比较（决策 3.5）。

### 4.3 连接：`ConnectReport`、`connect_driver`、`wants_from_typemask`（对应决策 3.2）

```rust
pub struct ConnectReport {
    pub keyboard_slot: Option<DeviceIndex>,
    pub mouse_slot: Option<DeviceIndex>,
    pub restore_lights: Option<(Minor, u32)>,  // 有键盘槽才有
}
```

掩码解码（`wants_from_typemask`，`const fn` 位测试）→ 按需分配 → 组装报告（含读记忆恢复项，`remembered_lights` 第 10 篇）。标签核对（传输层，2.2 节）与配置发送（传输层，第 05 篇载荷）在报告之外——报告是待办清单，不是执行。

### 4.4 断开：`DisconnectEffects` 与 `disconnect_device`（对应 1.4 节、决策 3.3）

```rust
pub struct DisconnectEffects {
    pub answer_reader: Option<(Endpoint, u32)>,   // 挂起→EIO回答
    pub notify_selector: Option<(Endpoint, Minor)>, // 选择者→可读通知
}
```

挂起回答错误、选择者通知可读、主人清空；队列打开名字灯光不动（1.4 节逐项对照）。与 `apply_close` 不合并（决策 3.3）。

### 4.5 与 C 的差异说明

| C 行为 | Rust 对应 | 差异分类 |
|--------|----------|---------|
| 分配扫描认牌找空满房 | 同扫描同规则 | 无差异 |
| 连接核对分配发配置恢复灯 | 核对发送归传输层，其余同 | 设计决策：判断执行分离（核对发送是传输） |
| 断开三步与不动项 | 同三步同不动 | 无差异 |
| 检查双向循环 | 过滤纯函数，循环归传输层 | 设计决策：纯胶水分离（决策 3.4） |
| 其余 | 逐项对应 | 无差异 |

---

## 5. 测试要点

> 测试代码在 `os/servers/input/src/connect.rs` 的测试模块。运行方法：`cargo test -p minix-input`（全 crate 通过，当前 66 个）。

| 测试函数 | 验证什么 | 对应的 C 行为 |
|---------|---------|--------------|
| `test_arrival_filter_matches_c` | 前缀过滤与标签截取 | input.c:577-582 |
| `test_alloc_claims_lowest_free_unopened` | 低到高首空房，主人名字落定 | input.c:444-459 |
| `test_alloc_reuses_same_label_with_new_owner` | 同牌回房，主人刷新 | input.c:445-449 |
| `test_alloc_skips_disconnected_open_slot` | 断开未关跳过，满房回空 | input.c:450-452,464-469 |
| `test_connect_reports_slots_and_light_restore` | 双槽分配、灯光恢复、满房仍报告 | input.c:509-527 |
| `test_typemask_decoding_matches_c` | 四种掩码组合 | input.c:509-512 |
| `test_disconnect_wakes_and_frees` | 叫醒通知撕牌子，不动四项 | input.c:540-552 |

### 5.1 测试统计（截至 2026-09-05）

- `cargo test -p minix-input`：**66 个通过，0 个失败**。
- 其中与本篇直接相关的 7 个（上表）；其余分属第 01 篇（5 个）、第 02 篇（8 个）、第 03 篇（6 个）、第 04 篇（8 个）、第 06 篇（4 个加共用 1 个）、第 07 篇（9 个加共用 1 个）、第 08 篇（5 个）、第 09 篇（7 个）、第 10 篇（4 个）与错误码模块（2 个）。
- 完整测试清单：`rg "#\[test\]" os/servers/input/src/connect.rs`

---

## 6. 过渡：主人定了，客人登场

本篇结束时，驱动的来去有了完整的账：到时分房、走时送客、账本双向对。但账本的另一边——客人自己——还没讲：驱动 side 如何贴条子（宣告）、组装上报、核对配置、响应调灯。下一篇（第 12 篇）是阶段内第一次"换边"：从服务 side 跨到驱动 side，讲客户端库。两边用同一套号码、同一套键拼写（第 05 篇的共享常量），对称开来，就是完整的输入世界。

---

## 7. 参见

- 第 01 篇 `01-input-init-main.md`：订阅（到来检查的上游）。
- 第 03 篇 `03-input-device-structs.md`：槽位窗口；标签存储。
- 第 05 篇 `05-input-message-contract.md`：配置载荷形状；键前缀共享常量；单向纪律。
- 第 06 篇 `06-input-open-close.md`：关闭清理（断开清理的对照）；独占政策。
- 第 08 篇 `08-input-ioctl-cancel-select.md`：取消清理（断开叫醒的对照）。
- 第 09 篇 `09-input-event-processing.md`：主人校验（分配建立的关系的使用方）。
- 第 10 篇 `10-input-setleds.md`：灯光记忆（连接时恢复）。
- 第 12 篇 `12-libinputdriver.md`：布告栏另一边（键发布、上报组装）。
- `os/servers/input/src/connect.rs`：本篇全部 Rust 实现。
