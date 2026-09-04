# 03-input-device-structs: 设备结构与编号映射

> **状态**: 已改写（2026-09-04，首版完整文档）
> **定位**: 设备槽位结构与两种编号的换算（阶段 2，核心数据结构；第 01 篇清表循环与第 02 篇门卫的会合点）
> **源码**: `minix3/minix/servers/input/input.h`（45 行，全部）+ `minix3/minix/servers/input/input.c:22-27`（三个判断宏）+ `input.c:44-80`（正反查函数）
> **Rust 模块**: `os/servers/input/src/structs.rs`（表格与换算）、`os/servers/input/src/error.rs`（查找失败的错误码）
> **目标读者**: 想理解"输入服务记住的每个设备长什么样、两种编号怎么换算"的读者。前置知识：第 01 篇（知道清表循环）、第 02 篇（知道门卫登记的是次设备号）。不需要事件格式知识（第 04 篇）——本章只讲"架子"，架子上挂的事件是什么格式是下一章的事。
> **本章不讲什么**: 事件结构体每个字段的含义（第 04 篇）；打开、读、选择等操作如何使用这些字段（第 06 至第 08 篇）；驱动到来时槽位的所有权如何变更（第 11 篇）。

---

## 1. 概念：一个设备，两个号码，十个房间

### 1.1 为什么需要两种编号：给人看的和给自己用的不是一回事

每个输入设备有两个号码。第一个号码叫**次设备号**，是用户进程在 `/dev` 目录里看到的：打开 `/dev/kbd0` 就是在说"我要 1 号键盘"。这些号码是稀疏的——键盘多路器是 0 号，一至四号键盘是 1 到 4 号，鼠标多路器一下子跳到 64 号，一至四号鼠标是 65 到 68 号。中间 5 到 63 的空位是故意留的：注释写明，将来加第五、第六个键盘时用中间的空号，老设备号不变，老程序不重新编译也能跑（向后兼容是这样用"留白"买来的）。

第二个号码叫**表格下标**，是服务自己在内存数组里找房间用的：十个房间排成一排，0 到 9 号，密不透风。稀疏的对外号码不适合当数组下标（不然数组要开 69 个槽位，其中 59 个永远空着），所以服务维护两张对照表，来回换算。

读者可以这样记：次设备号是门牌号（为了好看和兼容可以跳号），表格下标是房间在走廊里的第几间（为了省地方必须连号）。门卫（第 02 篇）记的是门牌号，服务员干活时找的是房间号，进门先换算。

### 1.2 十个房间的布局：一间大厅、四间包房，再来一遍

十个槽位按固定顺序排列：

```
下标  0            1    2    3    4      5            6    7    8    9
      键盘         一   二   三   四     鼠标         一   二   三   四
      多路器       号   号   号   号     多路器       号   号   号   号
      (次设备      键   键   键   键     (次设备      鼠   鼠   鼠   鼠
       号 0)       盘   盘   盘   盘      号 64)      标   标   标   标
```

第 0 间和第 5 间是多路器："任意键盘"和"任意鼠标"。读者想读"随便哪个键盘的按键"就打开多路器，想读"一号键盘的按键"就打开一号键盘。多路器房间有个特权：即使背后没有任何驱动，它也算"营业中"（1.4 节），这样读者可以在键盘驱动还没到来之前就先排队等着。

### 1.3 每个房间记住十三件事

每个槽位是一个十三字段的结构体。按作用分组理解，而不是死记硬背：

- **身份组**（我是谁、谁在喂我）：次设备号（这个房间的门牌）、拥有者（当前是哪个驱动进程在往这个房间送事件，没有就是"无"）、标签（那个驱动在数据存储服务里的注册名）。
- **队列组**（到了的事件排队）：事件环形缓冲（32 个事件的环）、队尾指针（下一个事件往哪放）、计数（现在存了几个）。
- **读者组**（谁在等数据）：打开标志（有没有读者）、挂起标志（有没有读者在排队等事件）、等待者身份、等待者的内存授权、等待者的请求编号、查询等待者（谁用查询问过"有数据了告诉我"）。
- **灯光组**（跨越重起的记忆）：指示灯掩码（最后一次的灯状态，驱动重起后靠它恢复，第 10 篇）。

十三件事里，门牌号终身不变（房间建好就钉死），其余十二件随运行变化。下一章（第 04 篇）解释队列里每个事件的格式，第 06 至第 08 篇解释读者组每个字段的用法，第 10、第 11 篇解释灯光组和身份组的变更。

### 1.4 三个判断宏：营业吗？空吗？满吗？

C 用三个单行宏表达三个最常用的判断：设备/traffic 是否营业（有拥有者，或者是两个多路器之一——多路器永远营业，见 1.2 节）；队列是否空（计数为零，读者来了只能排队）；队列是否满（计数达到 32，再来新事件就要挤掉最旧的，第 09 篇讲挤掉规则）。三个判断被用在打开、读、事件处理三处（第 06、第 07、第 09 篇），定义却只出现一次——本章是它们唯一的家，使用方只引用。

### 1.5 本章小结：架子搭好，换算表备好

读完本章，读者应该能不假思索地回答：为什么需要两种编号；十个槽位怎么排、门牌号分别是多少；每个槽位十三字段按什么分组；三个判断宏各自回答什么问题。下一章（第 04 篇）往架子上挂东西：队列里每个事件的二十字节长什么样，三百多个事件码如何组织。

---

## 2. C 源码分析

> 本章逐段对照原始 C 代码。所有行号以工作区当前 `minix3/` 为准。

### 2.1 配置常量：缓冲 32，键盘四鼠标四（input.h:6-15）

```c
/* Configuration. */
#define EVENTBUF_SIZE           32

#define KBDMUX_MINOR            0
#define KBD0_MINOR              1
#define KBD_MINORS              4

#define MOUSEMUX_MINOR          64
#define MOUSE0_MINOR            65
#define MOUSE_MINORS            4
```

`EVENTBUF_SIZE 32` 是每个房间的事件队列长度。32 不是算出来的，是经验值：一次调度的时间片里，快速打字能产生个位数事件，32 个足够扛过调度延迟；真溢出了就挤掉最旧的（第 09 篇），输入场景下"最新事件比最旧事件重要"（用户更关心刚按的键）。

键盘和鼠标各有一个多路器号、一个起始号、一个数量。注意写法上的不对称：键盘多路器 0 和起始 1 是连着的，鼠标多路器 64 和起始 65 也是连着的，但键盘区和鼠标区之间隔着 5 到 63 的空位——这就是 1.1 节说的留白。数量都是 4：四个键盘加四个鼠标，是 2006 年前后"一台机器上能接的输入设备"的合理上限；真超了要改头文件重新编译，这是用灵活性换简单性的典型（数组长度编译期固定，见 2.3 节）。

### 2.2 槽位编号：十个名字的由来（input.h:17-26）

```c
/* Constants. */
#define KBDMUX_DEV              0
#define FIRST_KBD_DEV           1
#define LAST_KBD_DEV            (FIRST_KBD_DEV + KBD_MINORS - 1)

#define MOUSEMUX_DEV            (LAST_KBD_DEV + 1)
#define FIRST_MOUSE_DEV         (MOUSEMUX_DEV + 1)
#define LAST_MOUSE_DEV          (FIRST_MOUSE_DEV + MOUSE_MINORS - 1)

#define INPUT_DEV_MAX           (1 + KBD_MINORS + 1 + MOUSE_MINORS)
```

槽位编号用"第一个加数量"的方式推导，而不是手写 0 到 9：最后一个键盘是"第一个键盘加数量减一"，鼠标多路器是"最后一个键盘的下一间"，设备总数是"一加四加一加四"。这样如果将来键盘数量改成 6，只改 `KBD_MINORS` 一处，所有推导自动跟上——手写 0 到 9 的写法改一处会漏三处。Rust 版本保留同样的推导精神（`structs.rs` 用常量表达式，测试锁死总数为 10）。

### 2.3 房间结构：十三个字段（input.h:29-43）

```c
struct input_dev {
        devminor_t minor;                 /* minor number of this device */
        endpoint_t owner;                 /* owning driver endpoint, or NONE */
        char label[DS_MAX_KEYLEN];        /* label of owning driver */
        struct input_event eventbuf[EVENTBUF_SIZE]; /* event ring buffer */
        unsigned int tail;                /* tail into ring buffer */
        unsigned int count;               /* number of elements in ring buffer */
        int opened;                       /* has a process opened the device? */
        int suspended;                    /* is a process suspended on a read? */
        endpoint_t caller;                /* endpoint for suspended read */
        cp_grant_id_t grant;              /* grant for suspended read */
        cdev_id_t req_id;                 /* request ID for suspended read */
        endpoint_t selector;              /* read-selecting endpoint, or NONE */
        unsigned int leds;                /* LED mask - saved across connects */
};
```

逐字段对照 1.3 节的分组：身份组三字段里，`label` 是 80 字节定长数组（`DS_MAX_KEYLEN` 含结尾零，`ds.h:29`），存驱动在数据存储服务里的注册名——房间记住喂它的人的名字，驱动离开时靠名字认尸（第 11 篇）；队列组三字段里，`tail` 是"下一个事件写哪里"，`count` 是"现在有几个"，读位置不需要存（读位置等于队尾减计数，环形缓冲的经典省法，第 07 篇展开）；读者组六字段里，`suspended` 和 `caller`/`grant`/`req_id` 是挂起读的完整联系方式（谁在等、数据送到他哪块内存、回信上写哪个编号），`selector` 是查询等待者的联系方式（第 08 篇）；灯光组 `leds` 是注释写明的"跨连接保存"（saved across connects）——驱动重起后，新驱动一到就先把灯调成这个样子（第 10 篇）。

类型细节：`tail`/`count`/`leds` 用无符号整型（计数和掩码不可能是负数），`opened`/`suspended` 用普通整型当布尔（C 时代没有好用的布尔类型，这是历史痕迹，Rust 版本用真正的布尔类型，见 3.3 节），`owner`/`caller`/`selector` 用端点类型（进程间地址），`grant` 用授权类型（跨进程内存访问的凭证）。

### 2.4 正查：门牌号找房间（input_map，input.c:44-62）

```c
static struct input_dev *
input_map(devminor_t minor)
{
        /*
         * The minor device numbers were chosen not to be equal to the array
         * slots, so that more keyboards can be added without breaking backward
         * compatibility later.
         */
        if (minor == KBDMUX_MINOR)
                return &devs[KBDMUX_DEV];
        else if (minor >= KBD0_MINOR && minor < KBD0_MINOR + KBD_MINORS)
                return &devs[FIRST_KBD_DEV + (minor - KBD0_MINOR)];
        else if (minor == MOUSEMUX_MINOR)
                return &devs[MOUSEMUX_DEV];
        else if (minor >= MOUSE0_MINOR && minor < MOUSE0_MINOR + MOUSE_MINORS)
                return &devs[FIRST_MOUSE_DEV + (minor - MOUSE0_MINOR)];
        else
                return NULL;
}
```

四个分支按 1.2 节的布局逐个命中，分支顺序就是房间顺序。开头的注释是全文最重要的注释之一：它亲口承认"门牌号和房间号故意不一样"，理由是"以后加键盘不断兼容"。找不到返回空指针，调用方（打开函数）回答"查无此设备"（第 06 篇）。注意区间写法是"大于等于起始、小于起始加数量"，上界用加法而不用"最后一个加一"——两种写法等价，但加法写法在数量变化时自动适应（和 2.2 节的推导哲学一致）。

### 2.5 反查：房间找门牌号，查无则崩溃（input_revmap，input.c:67-80）

```c
static devminor_t
input_revmap(int id)
{
        if (id == KBDMUX_DEV)
                return KBDMUX_MINOR;
        else if (id >= FIRST_KBD_DEV && id <= LAST_KBD_DEV)
                return KBD0_MINOR + (id - FIRST_KBD_DEV);
        else if (id == MOUSEMUX_DEV)
                return MOUSEMUX_MINOR;
        else if (id >= FIRST_MOUSE_DEV && id <= LAST_MOUSE_DEV)
                return MOUSE0_MINOR + (id - FIRST_MOUSE_DEV);
        else
                panic("reverse-mapping invalid ID %d", id);
}
```

正查的镜像：同样的四个分支反向走。唯一的不同是失败处理：正查失败返回空（调用者的错，调用者传了个不存在的门牌），反查失败直接崩溃（服务自己的错——服务只应该反查自己拥有的房间号，传个野下标进来说明服务内部逻辑坏了）。"外部的错返回错误，内部的错崩溃"，和第 01 篇的失败哲学一脉相承。Rust 版本对这条崩溃有不同处理（3.4 节），是本篇最重要的架构演进。

### 2.6 三个判断宏（input.c:22-28）

```c
static struct input_dev devs[INPUT_DEV_MAX];

#define input_dev_active(dev)           ((dev)->owner != NONE || \
                                         (dev)->minor == KBDMUX_MINOR || \
                                         (dev)->minor == MOUSEMUX_MINOR)
#define input_dev_buf_empty(dev)        ((dev)->count == 0)
#define input_dev_buf_full(dev)         ((dev)->count == EVENTBUF_SIZE)
```

十个房间的数组在第 22 行，一次分配、终身使用——没有动态增长，没有释放，服务的内存占用在编译期就定了。活跃判断是"有拥有者，或者是两个多路器之一"：多路器没有拥有者也营业，这是读者能提前排队等的法律基础（1.2 节）。空判断和满判断都是计数的直接比较，简单到不可能写错——这种"简单到不可能错"的性质，正是它们被写成宏放在头文件旁边的理由：用得多，必须一眼看懂。

### 2.7 覆盖核对：本篇语义范围内的符号一个不少

| 符号 | 源码位置 | 本文档位置 | Rust 对应（structs.rs） |
|------|---------|-----------|------------------------|
| `EVENTBUF_SIZE` | input.h:7 | 2.1 节 | `EVENT_BUFFER_SIZE = 32` |
| 六个次设备号常量 | input.h:9-15 | 2.1 节 | `KEYBOARD_MULTIPLEXER_MINOR` 等六个常量 |
| 七个槽位编号 | input.h:18-26 | 2.2 节 | `KEYBOARD_MULTIPLEXER_INDEX` 等常量 |
| `struct input_dev` 十三字段 | input.h:29-43 | 2.3 节 | `InputDevice` 十三字段 |
| `input_map` | input.c:44-62 | 2.4 节 | `map_minor_to_index` |
| `input_revmap` | input.c:67-80 | 2.5 节 | `minor_of_index`（失败改返回，见 3.4 节） |
| `input_dev_active`/`buf_empty`/`buf_full` | input.c:24-28 | 2.6 节 | `is_active`/`is_buffer_empty`/`is_buffer_full` 方法 |
| `devs` 数组 | input.c:22 | 2.6 节 | `InputTable` |

---

## 3. Rust 设计决策

> 本章解释"为什么这样设计"。每个决策先说备选方案，再说选择的理由。

### 3.1 两种编号写成两种类型，而不是两种整数

C 里门牌号和房间号都是整数（`devminor_t` 本质是整型，下标就是整型），靠程序员自觉不混用。问题在于两种号长得太像了：门牌 1 到 4 和房间 1 到 4 恰好重合，混用了在键盘区还测不出来，到鼠标区（门牌 65 对房间 6）才爆炸——这种"部分重合"的混淆是最难查的 bug。Rust 版本给每种编号一个新类型（`Minor` 包门牌号，`DeviceIndex` 包房间号），函数签名写明收哪种：收门牌的函数传房间号进来，编译失败。类型转换只发生在正反查两个函数里，全 crate 只有这两个地方允许"跨界"，审查时只看两处。

备选方案是保留整数加注释提醒。拒绝的理由：注释靠自觉，类型靠编译器；且键盘区的重合会让"自觉"在测试里永远不被挑战——测试恰好覆盖键盘区时，混用的代码也能跑过，bug 睡到鼠标区才醒。

### 3.2 正反查写成全函数，而不是查表

C 用分支函数换算，Rust 版本同样用分支函数（`map_minor_to_index`、`minor_of_index`），没有换成对照数组。理由是分支即文档：四个 `if` 的顺序就是 1.2 节房间布局的顺序，读者对照着读，一眼看出"分支顺序等于房间顺序"。对照数组更快（一次下标访问），但十个元素的分支预测几乎零开销，而数组写法把"哪个门牌对哪个房间"的知识藏进数组下标，读者需要左右横跳才能验证。性能不敏感的路径上，可读性优先——这是 Redox 代码里反复出现的取舍（Redox 的方案路由代码同样在冷路径上用枚举匹配而不用查表）。

但建表时例外：`InputTable::fresh` 用编译期数组（`MINOR_OF_SLOT`）逐个填门牌，因为建表循环调分支函数需要处理"万一失败"，而 0 到 9 号按定义不可能失败——用不可能失败的写法表达不可能失败的事。两个拼写（分支函数与常量数组）用测试锁在一起（`structs.rs` 的 `test_minor_table_matches_map_functions`）， drift 了测试会叫。

### 3.3 布尔就是布尔，端点哨兵收拢到一处

C 的 `opened`/`suspended` 是整型（历史痕迹，2.3 节），Rust 版本用真正的布尔类型。C 的"无拥有者""无等待者"用 `NONE` 端点表示，Rust 版本保留 `Endpoint::NONE`（这是跨进程地址的统一空值，整个系统都用它，改掉反而和全系统不一致），但把"有没有等待者"的判断收拢成方法（`has_selector`），调用方不再手写"端点等不等于 NONE"的比较。原则是：语言层面的历史包袱（整型布尔）去掉，系统层面的统一约定（NONE 空值）保留——区分"包袱"和"约定"的标准是：去掉之后读者是否还需要去别处查它的含义。布尔不需要查，NONE 需要（它是全系统约定），所以一个去掉一个保留。

### 3.4 反查失败返回空，而不是崩溃：本篇唯一的架构演进

C 的反查在下标非法时崩溃（2.5 节），理由是"服务不应该传野下标"。Rust 版本返回空（`minor_of_index` 返回 `Option`）。这不是心软，而是责任划分：C 把"调用者别传错"和"传错了就死"绑在一起，Rust 把"传错了"变成类型系统可见的分支，调用方必须处理。实际效果是调用方（第 09 篇的事件入口）把"非法下标"和"越界事件编号"合并成同一条丢弃路径——而 C 那里是两条路径（一条崩溃一条丢弃），崩溃那条在生产环境意味着整个服务陪葬一个 malformed 消息。差异分类是架构演进（A-7）：崩溃改显式错误，可观察行为从"服务死亡"变成"消息被丢弃"，后者明确地更符合"畸形输入不应该杀死服务"的原则。测试把"10 号和最大数下标都返回空"锁死，保证这条宽容永远不会被悄悄收紧。

### 3.5 标签截断保留终结符，数组尾部清零

C 的标签是 80 字节定长数组，`set_label` 复制时最多取 79 字节并保证第 80 个是零。Rust 版本逐字保留这个行为，包括"数组尾部清零"这个容易被忽略的细节：不清零的话，上次的长名字残留在尾部，这次的短名字读出来会带上上次的尾巴。截断加清零各有一行测试（`structs.rs` 的 `test_label_truncates_with_terminator`），包括"超长输入被截到 79"和"最后一个字节恒为零"。

---

## 4. 实现详解

> 完整代码在 `os/servers/input/src/structs.rs`（表格与换算）和 `os/servers/input/src/error.rs`（查找失败的错误码）。本章按"常量、类型、换算、结构、建表"的顺序展开，每个小节标注对应的第 3 章决策。

### 4.1 常量：门牌六个，房间七个，总数一个（对应 2.1、2.2 节）

次设备号六个常量（多路器两个、起始两个、数量两个），槽位编号六个常量（多路器两个、首尾各两个），总数一个（`DEVICE_COUNT = 10`）。命名上，门牌常量以 `MINOR` 结尾，槽位常量以 `INDEX` 结尾——读到名字就知道是哪种号，和 3.1 节的类型区分互相印证。测试把每个数值和 C 头文件逐个锁死（`structs.rs` 的 `test_minor_constants_match_c`）。

### 4.2 类型：`Minor` 与 `DeviceIndex`（对应决策 3.1）

```rust
pub struct Minor(pub i32);        // 门牌号：用户进程看到的号码
pub struct DeviceIndex(pub usize); // 房间号：数组下标
```

门牌用 32 位整数（C 的 `devminor_t` 是整型，门牌可能被直接比较大小，区间判断依赖整数语义）；房间用无符号地址宽度（它就是数组下标）。`Minor::is_multiplexer` 回答"是不是两个多路器之一"，`DeviceIndex::new` 校验外部传进来的下标（事件消息里带的就是下标，第 09 篇），非法返回 `InvalidDeviceIndex` 错误而不是崩溃（决策 3.4）。

### 4.3 换算：`map_minor_to_index` 与 `minor_of_index`（对应决策 3.2、3.4）

两个 `const fn`，分支顺序和 C 的四个分支一一对应，注释标出每个分支的 C 行号。正查失败返回空（调用方在第 06 篇回答查无此设备），反查失败返回空（决策 3.4）。往返测试（`test_minor_of_index_round_trips_map`）验证"每个正查接受的门牌反查都回到自己"，外加"10 号和最大数下标反查为空"。

### 4.4 结构：`InputDevice` 十三字段（对应 2.3 节、决策 3.3）

字段顺序和 C 结构体一致，方便左右对照。布尔字段用布尔类型，端点字段用端点类型并默认 `NONE`，授权用 32 位整数，请求编号用无符号 32 位（C 的 `cdev_id_t` 是无符号整型）。三个判断写成方法（`is_active`、`is_buffer_empty`、`is_buffer_full`），`is_active` 里"多路器永远营业"的分支和 C 宏逐字对应。标签读写两个方法（`set_label`、`label_bytes`）实现 3.5 节的截断语义。

### 4.5 建表：`InputTable::fresh`（对应 2.6 节、第 01 篇清表循环）

```rust
pub fn fresh() -> Self  // 对应 input_init 第 652-662 行的清表循环
```

从全空模板开始，逐个槽位填门牌（门牌来自编译期数组，不可能失败），其余字段保持 resting 值。测试（`test_fresh_table_matches_input_init_loop`）逐槽位断言八个 resting 值，外加"两个多路器营业、普通槽位不营业"——第 01 篇的清表循环和第 02 篇门卫依赖的"多路器营业"性质，在这里第一次被同一个测试同时验证。

### 4.6 与 C 的差异说明

| C 行为 | Rust 对应 | 差异分类 |
|--------|----------|---------|
| 反查非法下标崩溃（input.c:79） | 返回空，调用方丢弃 | 架构演进 A-7：见 3.4 节 |
| 整型布尔字段 | 真布尔类型 | 语言层演进：行为一致 |
| 其余常量、字段、分支、判断 | 逐项对应 | 无差异 |

---

## 5. 测试要点

> 测试代码在 `os/servers/input/src/structs.rs` 的测试模块。运行方法：`cargo test -p minix-input`（当前全 crate 共 28 个测试，全部通过）。

| 测试函数 | 验证什么 | 对应的 C 行为 |
|---------|---------|--------------|
| `test_minor_constants_match_c` | 六个门牌常量、槽位编号、总数 10 | input.h:7-26 |
| `test_map_minor_to_index_matches_c_branches` | 四个分支全命中，外加七个非法门牌 | input.c:44-62 |
| `test_minor_of_index_round_trips_map` | 正反查互逆，非法下标返回空 | input.c:67-80（崩溃改返回） |
| `test_minor_table_matches_map_functions` | 分支函数与常量数组永不 drift | 实现内部一致性 |
| `test_fresh_table_matches_input_init_loop` | 建表八个 resting 值，多路器营业 | input.c:652-662，input.c:24-26 |
| `test_label_truncates_with_terminator` | 超长截断、尾部清零、终结符恒在 | label 数组语义 |

### 5.1 测试统计（截至 2026-09-04）

- `cargo test -p minix-input`：**29 个通过，0 个失败**。
- 其中与本篇直接相关的 6 个（上表）；其余分属第 01 篇（5 个）、第 02 篇（8 个）、第 04 篇（8 个）与错误码模块（2 个）。
- 完整测试清单：`rg "#\[test\]" os/servers/input/src/`

---

## 6. 过渡：房间有了，事件的格式还没定

本篇结束时，十个房间已经盖好，门牌和房间的对照表也已备好，三个"营业吗、空吗、满吗"的判断各就各位。但房间里的队列目前只能存"某种 20 字节的东西"——那种东西长什么样，哪个字节表示按键、哪个字节表示鼠标位移，要到下一篇（第 04 篇）才揭晓。第 04 篇的事件格式会直接成为本篇 `events` 数组的元素类型，两篇在那里会合：本篇定义"架子"，下篇定义"挂在架子上的东西"。

---

## 7. 参见

- 第 01 篇 `01-input-init-main.md`：清表循环（本篇 `InputTable::fresh` 的调用方）。
- 第 02 篇 `02-chardriver-framework.md`：门卫登记簿（记的是本篇的门牌号）。
- 第 04 篇 `04-input-event-format.md`：事件格式（本篇队列的元素类型）。
- 第 06 篇 `06-input-open-close.md`：正查失败与活跃判断的使用方。
- 第 09 篇 `09-input-event-processing.md`：下标校验与队列读写的使用方。
- `minix3/minix/servers/input/input.h`：本篇全部 C 依据（45 行）。
- `minix3/minix/servers/input/input.c:22-80`：判断宏与换算函数。
- `os/servers/input/src/structs.rs`：本篇全部 Rust 实现。
