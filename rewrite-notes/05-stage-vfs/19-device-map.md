# 19 — device-map：驱动通讯录、套接字表与控制分派

本文讲清设备层如何在“号查人、域查人、标签查人”的三本通讯录中，以 major 号、协议域、DS 标签为三组键，把一次打开或读写路由到对的驱动进程，并在驱动生死更替时以“消失清表、恢复续命”维持通讯录不腐，并在控制命令上以类型三分派、以请求位解码授权方向。

前置阅读：`18-mount.md`（设备号编解码与驱动标签的来源）、`06-vmnt-table.md`（表槽语义类比）、`01-vfs-init-main.md`（启动时的 `init_dmap/init_smap` 调用点）。

> **Rust 实现**: `os/servers/vfs/src/device_map.rs`（本篇判定层：双表/注册/恢复/授权）+ `os/servers/vfs/src/open.rs`（`FileType` 复用）+ `os/servers/vfs/src/pipe.rs`（`driver_vanish_plan`）+ `os/servers/vfs/src/filedes.rs`（失效族）+ `os/libs/minix-types/src/types/errno.rs`（errno 常量表）

> 本章不讲什么：
> - `cdev_io/sdev_readwrite/bdev_*` 的驱动数据面执行—— `21-cdev.md` / `22-sdev.md` / `20-bdev.md`
> - `invalidate_filp` 族的失效执行—— `14-filedes.md`
> - `worker_stop` 的调度执行—— `08-worker-thread.md`
> - `select_callback` 的多路复用执行—— `23-select.md`
> - 授权（grant）的内核侧实现—— `99-global-concepts.md`（本篇只给方向与尺寸解码）
> - DS 标签服务的实现—— DS 侧（本篇只给查询接缝）

---

## 1 概念

### 1.1 为什么号与人分离

设备号命名“哪类设备”（major），端点命名“哪个驱动进程”。两套命名必须分离：驱动会死会重启，端点随之变化，而 `/dev` 下的 major 号百年不变。通讯录就是两套命名之间的映射——号不变，人可换；人换了，表改一行，引用该号的一切（vnode、filp、挂载）无需改动。

分离的代价是查表：每次打开字符设备都要问“major 4 是谁在管”。135 行的小表线性扫描即可——通讯录的规模决定了它不需要索引，这是“够用即好”的工程判断，不是懒惰。

### 1.2 两本通讯录

字符与块设备共用一本（dmap：按 major 查人），套接字驱动另开一本（smap：按协议域查人）。分开的根因是键不同：前者的键是 major 号（设备分类学），后者的键是协议域（`PF_INET` 之类）；一个驱动还可管多个域（一对多），于是 smap 再配一张域→行的小表（pfmap）。

套接字号另起一套编码（行号左移 32 位拼套接字 id），与 major/minor 命名空间刻意不重叠——重叠即混淆，混淆即错路由。`make_smap_dev` 的注释把这点说透了：用 `dev_t` 装它纯为存放方便，查之前必须先验文件类型。

### 1.3 驱动的生死契约

驱动的一生走四态：映射（RS 报号上岗）→ 服务（处理请求）→ 消失（崩溃或退出，清表并失效其 filp）→ 恢复（重启续命，唤醒等待者）。消失与恢复不对称：消失是单方面的（VFS 自己清表），恢复要分种类——块驱动走“续命”（`bdev_up` 重放），字符驱动走“清表”（旧 filp 作废），套接字走“失效”（旧套接字作废）。

重启还有“换人不换号”与“换号”的区别：同标签重启复用旧槽（幂等），端点变了才驱散旧等待者。复用槽而不清域是 bug 之源——旧域残留会把新流量引向已死的映射，所以注册时先清旧域再写新域，顺序不可颠倒。

### 1.4 控制命令的分派与授权

`ioctl` 的分派与读写同型：按 `S_IFMT` 三路（块/字符/socket），余下 `ENOTTY`。“不合适的控制”与“打不开的文件”共用拒绝哲学：类型不对即拒，不问理由。块分支多一道守卫（`filp_ioctl_fp` 的设→调→清）——守卫标记“此次调用正在占用该 filp”，防的是 ioctl 与读写在同一 filp 上的交错。

授权方向的解码是全篇最反直觉的一点：`IOR`（把数据读出给用户）配 `CPF_WRITE`（授权驱动写），`IOW`（把数据写入驱动）配 `CPF_READ`（授权驱动读）。交叉的根因是视角：`IOR/IOW` 站在用户侧命名方向，`CPF_*` 站在驱动侧命名许可——读出即驱动写，写入即驱动读。记住“方向看用户，许可看驱动”，交叉自明。

### 1.5 重启的两种语义

“驱动回来了”有两种含义：无状态重启（新进程，旧状态全丢——驱散旧等待者、作废旧套接字）与有状态续命（同进程，旧状态还在——唤醒等待者继续）。`smap_map` 以端点变没变区分两者：变了交驱散（叫醒挂起者、作废旧套接字这些动作由挂起面执行，它只决定“要不要”），不变走复用。区分的代价是一次比较，混淆的代价是把旧等待者叫醒去读一个已死的状态——唤醒精确性在此即正确性。

块驱动的恢复再分两层：恢复中又坏（停服，不再试）与首次坏（标记恢复中，跑 `bdev_up`）。“恢复中”标志是重入守卫——恢复本身可能耗时，期间再坏说明驱动已无可救，先停服再说人话（打印一句）。

### 1.6 特殊号码：自管与另册

`/dev/tty`（major 5）由 VFS 自己管，不经过任何驱动——控制终端的语义（作业控制、会话）本就属于 VFS 的进程模型，假手驱动反而多一跳。初始化时直接落表（标签 `"vfs"`、端点 VFS 自己），此后一切查询同等对待：特例只出现在落表那一刻，查询路径无分支。

伪设备（18 的 nonedev）是另一册：它们不在 dmap 里，查表查不到是正常的——无盘 FS 本来就没有驱动。两本册子（dmap 与 nonedev 位图）互补：有盘查表，无盘查位图，查错册子即错路由。

### 1.7 与其他 OS 的设备表对照

- **Linux** 以 `cdev_map`（major→`cdev` 的 kobj 映射）+ `def_chrdev` 默认表实现同构通讯录：`map_driver` 的增删对应 `cdev_add/cdev_del`，`dmap_endpt_up` 的恢复对应内核的驱动重绑定（rebind），`invalidate_filp_by_char_major` 对应 `kill_fasync` 的失效通知语义。
- **Redox** 以 scheme 桶（`scheme.rs` 的 `SchemeList` 按 scheme 号分发）扁平化设备命名：major 号对应 scheme 号，`get_dmap_by_endpt` 的线性扫描对应 Redox 按 id 查 scheme 表；Redox 的方案名（`"display"`, `"audio"`）即 DS 标签的同构物。
- **seL4** 无设备表，驱动是持有内存与中断能力的用户进程，路由由各服务的私有表完成；VFS 的集中通讯录在 seL4 中对应“每个服务自带小表”——集中还是分散，是单体内核遗产与微内核现状的分水岭。

### 1.8 小结

设备层是三本通讯录（dmap 按号、smap 按域、DS 按标签）加一套生死契约（映射→服务→消失→恢复）加一路控制分派（类型三路、授权交叉解码）。三组键（major/域/标签）决定路由，一条不变量贯穿始终：号不变，人可换——表改一行，树上万物无感。

---

## 2 C 源码分析

### 2.1 `dmap` 表结构（`minix3/minix/servers/vfs/dmap.h:dmap` + `minix3/minix/include/minix/dmap.h:NR_DEVICES` + `minix3/minix/servers/vfs/dmap.c:init_dmap`）

八字段：端点、标签（`LABEL_MAX 16` 含 NUL，见 `minix3/minix/servers/vfs/const.h:LABEL_MAX`）、选择忙/选择 filp（select 执行态，配 `SEL_RD/WR` 见 `minix3/minix/servers/vfs/const.h:SEL_RD`，23 管辖）、服务线程、锁、恢复标志、tty 可见标志。表长 `NR_DEVICES 135`（见 `minix3/minix/include/minix/dmap.h:NR_DEVICES`）。CTTY 例外：major 5（`minix3/minix/include/minix/dmap.h:CTTY_MAJOR`）由 VFS 自管，端点 `CTTY_ENDPT`（= `VFS_PROC_NR`，`minix3/minix/servers/vfs/const.h:CTTY_ENDPT`），初始化直落 `"vfs"` 标签（`244-246`）。

### 2.2 `lock_dmap/unlock_dmap` 加锁机（`minix3/minix/servers/vfs/dmap.c:lock_dmap`）

加锁断言非空且已映射（`33-34`），挂起自己再取锁（`36-41`），失败 panic；解锁失败 panic（`54-55`）。挂起取锁与 16 的 `lock_bsf` 同型（慢道让出 worker）。

### 2.3 `map_driver` 增删机（`minix3/minix/servers/vfs/dmap.c:map_driver`）

越界 major 即 `ENODEV`（`71`）。端点 `NONE` 即删：先失效字符 filp（`83`，注释写明恢复期亦如此——简单压倒精巧），清端点回 `OK`（`84-85`）。标签超长（`len+1 > 16`）即 `EINVAL`（`89-93`），存标签与端点（`94-98`）。

### 2.4 `do_mapdriver` 入口机（`minix3/minix/servers/vfs/dmap.c:do_mapdriver`）

RS 门（`123`，余者 `EPERM`）→ 标签长度门（`132-135`）→ 拷贝（`136-141`，失败与无终结各 `EINVAL`）→ DS 查端点（`148-152`，未知标签 `EINVAL`）→ 端点复核并标服务（`155-160`）→ 双表提交（`163-173`：dmap 先行，smap 失败则回滚 dmap——提交序的逆序回滚，18 同例）。

### 2.5 `map_service/init_dmap` 启动机（`minix3/minix/servers/vfs/dmap.c:map_service`）

`map_service`：启动用户进程跳过（`206`），端点复核标服务（`209-215`），无设备止步（`218`），有设备走 `map_driver`（`221-224`）。`init_dmap`：全表清零（`235-242`：端点 `NONE`、服务线程非法、锁初始化）+ CTTY 落定（`244-246`，失败 panic——启动不变式）。

### 2.6 查询三函数（`minix3/minix/servers/vfs/dmap.c:dmap_driver_match`）

`dmap_driver_match`（`252-259`）：越界否，未映射否，端点等否，三元合取。`get_dmap_by_major`（`264-270`）：越界或未映射即空。`get_dmap_by_endpt`（`317-328`）：全表线性扫描首命中。

### 2.7 `dmap_endpt_up` 恢复机（`minix3/minix/servers/vfs/dmap.c:dmap_endpt_up`）

`NONE` 直返（`284`）。逐 major 查活行（`287`）：端点命中且块→恢复中又坏则停服清标志（`290-299`），否则置恢复中跑 `bdev_up` 清标志（`300-302`）；端点命中且字符→停服工人并失效 filp（`304-309`，`invalidate_filp_by_char_major`）。

### 2.8 `smap` 表结构与初始化（`minix3/minix/servers/vfs/type.h:smap` + `minix3/minix/servers/vfs/smap.c:init_smap`）

六字段：序号、端点、标签、选择忙、选择 filp（见 `minix3/minix/servers/vfs/type.h:smap`），`sockid_t` 为 `int32`（`minix3/minix/servers/vfs/type.h:sockid_t`）。表长 8（`minix3/minix/servers/vfs/const.h:NR_SOCKDEVS`），域表长 35（`PF_MAX=AF_MAX=35`，见 `minix3/sys/sys/socket.h:PF_MAX` + `minix3/sys/sys/socket.h:AF_MAX`；`PF_UNSPEC=0`，见 `minix3/sys/sys/socket.h:PF_UNSPEC`）。初始化一基编号（`32`，零号留给 `NO_DEV` 避让）+ 端点 `NONE` + 域表清零（`26-37`）。

### 2.9 `smap_map` 注册机（`minix3/minix/servers/vfs/smap.c:smap_map`）

域数门（`54-55`：`1..=NR_DOMAIN`，`NR_DOMAIN=8` 见 `minix3/minix/include/minix/config.h:NR_DOMAIN`）→ 同标签复用旧槽（`62-69`，重启幂等）→ 逐域校验（`75-83`：越界/`UNSPEC`/他占，各 `EINVAL`/`EBUSY`）→ 无复用则占空槽（`89-97`，满 `ENOMEM`）→ 端点变更才驱散失效（`108-120`）→ 清旧域（`122-124`，顺序先清后写）→ 落槽写域（`131-138`）。

### 2.10 `smap` 查询与驱散（`minix3/minix/servers/vfs/smap.c:smap_unmap_by_endpt`）

`smap_unmap_by_endpt`（`147-167`）：按端点定位→先失效套接字→清端点→清域（顺序：失效先于清除，`160-166`）。挂起面的驱散另有其人：`unsuspend_by_endpt`（`minix3/minix/servers/vfs/pipe.c:unsuspend_by_endpt`，`334-357`）负责叫醒挂在死驱动上的进程（Cdev 挂起回 `EIO`、Sdev 经 smap 行匹配停尸），归 17 篇。`smap_endpt_up`（`173-187`）：定位即失效（上线即旧套接字作废）。`make_smap_dev`（`200-208`）：行号左移 32 拼 id，端点与 id 非负断言。`get_smap_by_dev`（`216-237`）：拆号、零号/越界/负 id 拒绝、一基回表、断言序号一致、端点空拒绝、可选回写 id。`get_smap_by_endpt`（`244-259`）：O(n) 扫描（`249` 的 TODO 诚实保留）。`get_smap_by_domain`（`265-273`）：越界空，余直返（含空）。

### 2.11 `do_ioctl` 分派机（`minix3/minix/servers/vfs/device.c:do_ioctl`）

`get_filp(VNODE_READ)` 取锁（`30-31`）→ 按 `S_IFMT` 三路：块置守卫调 `bdev_ioctl` 清守卫（`35-41`）、字符调 `cdev_io(CDEV_IOCTL,…)`（`43-46`，`CDEV_IOCTL` 见 `minix3/minix/include/minix/com.h:CDEV_IOCTL`）、socket 调 `sdev_ioctl`（`48-50`）→ 余 `ENOTTY`（`52-54`）→ 解锁返回（`56-58`）。

### 2.12 `make_ioctl_grant` 授权机（`minix3/minix/servers/vfs/device.c:make_ioctl_grant`）

方向解码（`76-78`：`IOR→CPF_WRITE`、`IOW→CPF_READ`，`CPF_*` 见 `minix3/minix/include/minix/safecopies.h:CPF_READ`）→ 尺寸解码（`79-82`：`IOC_BIG` 置位走 20 位 8 移，否则 12 位 16 移，见 `minix3/sys/sys/ioccom.h:IOC_BIG`）→ 魔法授权（`89`，注释写明“即使无 I/O 也授权”，`85-88`）→ 无效 panic（`91-92`）。

---

## 3 Rust 设计决策

Rust 改写不是照抄三文件的直线代码，而是吸收 Linux/Redox 的设备模型后做取舍：以下七项决策各自回答“为什么这样表达、为什么不那样表达”。

### D1 表项值化

- **C**：八字段/六字段裸全局数组 + `NONE` 哨兵（`minix3/minix/servers/vfs/dmap.c:dmap`、`smap.c:15-16`）。
- **Rust**：`DmapEntry{driver, label, recovering, servicing, seen_tty, sel_busy, sel_owner}` + `SmapEntry{num, endpt, label, sel_busy, sel_owner}` + 定长数组表（`os/servers/vfs/src/device_map.rs:struct DmapEntry` + `os/servers/vfs/src/device_map.rs:struct SmapEntry`；端点字段是 `Option<Endpoint>` 而非裸 `i32`）。
- **为什么**：`NONE` 即 `None`（17 同例）；一基编号构造固化。锁不入表（锁的持有语义归 07 的模型）；select 执行态与 tty 可见标志与 C 同构保留在行内（回复落地时按位认领，23 消费）。

### D2 目录 trait 化

- **C**：DS 查端点内嵌注册路（`minix3/minix/servers/vfs/dmap.c:do_mapdriver`）。
- **Rust**：`EndpointDirectory` trait（`StaticDir` 有答 vs `EmptyDir` 无答）+ `resolve_driver` 泛型（`os/servers/vfs/src/device_map.rs:trait EndpointDirectory` + `os/servers/vfs/src/device_map.rs:fn resolve_driver`）。
- **为什么**：DS 是外部依赖；trait 使未知标签可单测。替代方案（`HashMap` 直查）被否决：替身仍需 trait，多一层无谓。

### D3 注册纯判定

- **C**：五步直线 + 副作用交织（`minix3/minix/servers/vfs/smap.c:smap_map`）+ 双提交回滚（`minix3/minix/servers/vfs/dmap.c:do_mapdriver`）。
- **Rust**：`register_plan`（复用/占位/校验/副作用计划）+ `DomainCheck` 四值（`check_domain` 逐域判定）（`os/servers/vfs/src/device_map.rs:fn register_plan` + `os/servers/vfs/src/device_map.rs:fn check_domain`）。
- **为什么**：校验与变更分离；端点变更才驱散以计划显式（`ReplaceSideEffects`，由事件循环消费）。双提交的逆序回滚（smap 失败拆 dmap）由事件循环按提交序执行，与 18 同例。

### D4 恢复状态机

- **C**：恢复三态散在循环中（`minix3/minix/servers/vfs/dmap.c:dmap_endpt_up`）。
- **Rust**：`recover_step(recovering, servicing) -> RecoverVerdict::{FailoverStop, BeginRecover, Steady}` + `classify_vanish` 三分类（`os/servers/vfs/src/device_map.rs:fn recover_step` + `os/servers/vfs/src/device_map.rs:fn classify_vanish`）。
- **为什么**：两转移（又坏停服/首坏续命）纯函数化后全覆盖；执行（停工人/清表/续命）留 08/14/20。

### D5 授权解码纯函数

- **C**：宏解码内嵌授权调用（`minix3/minix/servers/vfs/device.c:make_ioctl_grant`）。
- **Rust**：`ioctl_access`（交叉保留）+ `ioctl_size`（BIG 分流）（`os/servers/vfs/src/device_map.rs:fn ioctl_access` + `os/servers/vfs/src/device_map.rs:fn ioctl_size`）。
- **为什么**：交叉语义（读出配写、写入配读）是最反直觉的一点，纯函数使之可测锁定。位值全部 sync 树可验证，不编造。

### D6 控制分派复用

- **C**：`S_IFMT` 三路 + `ENOTTY`（`minix3/minix/servers/vfs/device.c:do_ioctl`）。
- **Rust**：`ioctl_route(ft: FileType)` 复用 15 类型 + `BLOCK_NEEDS_GUARD` 守卫常量（`os/servers/vfs/src/device_map.rs:fn ioctl_route` + `os/servers/vfs/src/device_map.rs:const BLOCK_NEEDS_GUARD`）。
- **为什么**：不重复定义分派（15 权威）；守卫是时序义务，以常量声明，执行归 20。

### D7 锁协议化（复用）

- **C**：断言门 + 挂起取锁 + panic（`minix3/minix/servers/vfs/dmap.c:lock_dmap`）。
- **Rust**：不另设锁类型；C 的两条断言（行非空、行已映射）化为调用点前置检查——`map_driver` 的越界分支给 `MapError::NoDev`，锁本身沿用 07 的持有模型。
- **为什么**：锁执行已在 07 建模；另设即第二套锁抽象，两套模型对同一把锁的持有语义会漂移。断言的“不可能”以调用点前置检查表达。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-1 单线程事件循环（mthread→状态机） | mutex→持有语义（07 复用）；挂起执行留 08 | `os/servers/vfs/src/device_map.rs:struct DmapEntry`（不含锁字段）+ 本文档 D7 + §2.2 |
| A-8 64 位类型映射 | `dev_t=u64` 套接字号 32+32 拼合 | `os/servers/vfs/src/device_map.rs:fn make_smap_dev` + 本文档 D1 + §1.2 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── device_map.rs         — 本篇：双表/注册/恢复/授权判定
├── open.rs               — FileType 复用（15，不重复分派）
├── fproc.rs              — FP_SRV_PROC 语义（02，标服务位归属）
└── mount.rs              — DevCodec 对照（18，major/minor 另册）
```

> 设计决策：§3 D1（表项值化）/ D3（注册纯判定）/ D5（授权解码）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `dmap` 八字段 | `minix3/minix/servers/vfs/dmap.h:dmap` | `os/servers/vfs/src/device_map.rs:struct DmapEntry` | 路由四位 |
| `NR_DEVICES` | `minix3/minix/include/minix/dmap.h:NR_DEVICES` | `os/servers/vfs/src/device_map.rs:const NR_DEVICES` | 135 |
| `CTTY` 例外 | `minix3/minix/servers/vfs/dmap.c:init_dmap` | `os/servers/vfs/src/device_map.rs:struct DmapTable` + `os/servers/vfs/src/device_map.rs:fn map_driver` | 自管落定 |
| `map_driver` | `minix3/minix/servers/vfs/dmap.c:map_driver` | `os/servers/vfs/src/device_map.rs:fn map_driver` | 增删二路 |
| 查询三函数 | `minix3/minix/servers/vfs/dmap.c:dmap_driver_match` | `os/servers/vfs/src/device_map.rs:fn driver_match` + `os/servers/vfs/src/device_map.rs:fn get_by_major` + `os/servers/vfs/src/device_map.rs:fn get_by_endpt` | 纯谓词 |
| 驱散计数 | `minix3/minix/servers/vfs/dmap.c:dmap_unmap_by_endpt` | `os/servers/vfs/src/device_map.rs:fn unmap_by_endpt` | 精确计数 |
| DS 接缝 | `minix3/minix/servers/vfs/dmap.c:do_mapdriver` | `os/servers/vfs/src/device_map.rs:trait EndpointDirectory` + `os/servers/vfs/src/device_map.rs:fn resolve_driver` | trait 双实现 |
| RS 门/服务分类 | `minix3/minix/servers/vfs/dmap.c:do_mapdriver` + `minix3/minix/servers/vfs/dmap.c:map_service` | `os/servers/vfs/src/device_map.rs:fn check_mapper` + `os/servers/vfs/src/device_map.rs:fn classify_service` | 门 + 三分类 |
| `smap` 六字段 | `minix3/minix/servers/vfs/type.h:smap` | `os/servers/vfs/src/device_map.rs:struct SmapEntry` + `os/servers/vfs/src/device_map.rs:struct SmapTable` | 一基固化 |
| 注册机 | `minix3/minix/servers/vfs/smap.c:smap_map` | `os/servers/vfs/src/device_map.rs:fn register_plan` | 校验 + 计划 |
| 槽位 helpers | `minix3/minix/servers/vfs/smap.c:smap_map` | `os/servers/vfs/src/device_map.rs:fn find_slot_by_label` + `os/servers/vfs/src/device_map.rs:fn find_free_slot` | 复用/占位 |
| 套接字号编解码 | `minix3/minix/servers/vfs/smap.c:make_smap_dev` + `minix3/minix/servers/vfs/smap.c:get_smap_by_dev` | `os/servers/vfs/src/device_map.rs:fn make_smap_dev` + `os/servers/vfs/src/device_map.rs:fn split_smap_dev` | 拼合/拆分 |
| 端点/域查询 | `minix3/minix/servers/vfs/smap.c:get_smap_by_endpt` + `minix3/minix/servers/vfs/smap.c:get_smap_by_domain` | `os/servers/vfs/src/device_map.rs:fn smap_by_endpt` + `os/servers/vfs/src/device_map.rs:fn smap_by_domain` | O(n) 诚实 |
| 恢复机 | `minix3/minix/servers/vfs/dmap.c:dmap_endpt_up` | `os/servers/vfs/src/device_map.rs:fn recover_step` + `os/servers/vfs/src/device_map.rs:fn classify_vanish` | 三态 + 分类 |
| 控制分派 | `minix3/minix/servers/vfs/device.c:do_ioctl` | `os/servers/vfs/src/device_map.rs:fn ioctl_route` | 复用 + 守卫位 |
| 授权解码 | `minix3/minix/servers/vfs/device.c:make_ioctl_grant` | `os/servers/vfs/src/device_map.rs:fn ioctl_access` + `os/servers/vfs/src/device_map.rs:fn ioctl_size` | 交叉 + 分流 |
| 错误族 | 三文件全文件 | `os/servers/vfs/src/device_map.rs:enum MapError`（`to_errno` 两入口） | 7 变体→errno，无自创 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 号人互查 | `driver_match` | 三元合取 | `minix3/minix/servers/vfs/dmap.c:dmap_driver_match` |
| 标签有界 | `validate_label` | 16 含 NUL | `minix3/minix/servers/vfs/dmap.c:map_driver` |
| 重启幂等 | `find_slot_by_label` | 同标签复用 | `minix3/minix/servers/vfs/smap.c:smap_map` |
| 授权交叉 | `ioctl_access` | 读出配写 | `minix3/minix/servers/vfs/device.c:make_ioctl_grant` |
| 一基编号 | `SmapTable::new` | `i+1` 固化 | `minix3/minix/servers/vfs/smap.c:init_smap` |

### 4.4 驱动死亡级联的编排

C 的“驱动消失”没有单一入口，是三处同族动作的合集：`map_driver` 的 unmap 支先做字符失效再清行（`minix3/minix/servers/vfs/dmap.c:map_driver`，`82-85`）、`dmap_endpt_up` 的 `worker_stop + invalidate` 支（`minix3/minix/servers/vfs/dmap.c:dmap_endpt_up`，`304-309`）、smap 侧三处 `invalidate_filp_by_sock_drv(sp->smap_num)`（`minix3/minix/servers/vfs/smap.c:smap_map` 与同文件的 `smap_unmap_by_endpt`/`smap_endpt_up`，`119/160/186`）；挂起面的散射在 `unsuspend_by_endpt`（`minix3/minix/servers/vfs/pipe.c:unsuspend_by_endpt`，`334-357`）：Cdev 挂起点端点匹配即 `revive(EIO)`、Sdev 挂起经 smap 行匹配即 `sdev_stop`，select 等待者另有 `select_unsuspend_by_endpt` 一面。

Rust 把决策面收成两个编排函数。`driver_death_cascade`（`os/servers/vfs/src/device_map.rs:fn driver_death_cascade`）做“身份扫描 + 失效执行 + 家族分类”：按端点扫 dmap 表拿 major、扫 smap 表拿一基行号，字符家族经 `invalidate_by_char_major`（`os/servers/vfs/src/filedes.rs:fn invalidate_by_char_major`）、socket 家族经 `invalidate_by_sock_drv`（`os/servers/vfs/src/filedes.rs:fn invalidate_by_sock_drv`），产出 `CascadeOutcome { notice, char_major, sock_num, invalidated }`；块家族不出失效，`notice = RecoverBlock` 提示调用方走 `bdev_up` 恢复路径（执行归 `20-bdev.md`）。`driver_vanish_plan`（`os/servers/vfs/src/pipe.rs:fn driver_vanish_plan`）做 fproc 扫描：活槽按挂起面分类，Cdev 端点匹配 → `ReviveEio`、Sdev 经 `stop_matches` → `StopSdev`，产出 `(UserSlot, DriverWake)` 计划；第三面 select 归 select 模块。两个函数都是“计划/决策”位：复活入队、sdev 槽收尾与表清理的执行属事件循环层的编排。

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 为 **538 passed / 0 failed**（`minix-types` 独立计数）。
> 本节列出与本模块直接相关的 14 个（`device_map.rs` 全量；完整清单：`rg "fn test_" os/servers/vfs/src/device_map.rs`）。

| 测试名 | 覆盖 C 位置 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_dmap_init_and_ctty` | `minix3/minix/servers/vfs/dmap.c:init_dmap` | 清零 + CTTY 落定 + 越界空 | `os/servers/vfs/src/device_map.rs:fn test_dmap_init_and_ctty` |
| `test_map_driver_lifecycle` | `minix3/minix/servers/vfs/dmap.c:map_driver` + `minix3/minix/servers/vfs/dmap.c:dmap_unmap_by_endpt` + `minix3/minix/servers/vfs/dmap.c:dmap_driver_match` | 增删查驱散全周期 | `os/servers/vfs/src/device_map.rs:fn test_map_driver_lifecycle` |
| `test_mapper_gate_and_directory` | `minix3/minix/servers/vfs/dmap.c:do_mapdriver` + `minix3/minix/servers/vfs/dmap.c:map_service` | RS 门 + 目录双实现 + 服务分类 | `os/servers/vfs/src/device_map.rs:fn test_mapper_gate_and_directory` |
| `test_smap_init` | `minix3/minix/servers/vfs/smap.c:init_smap` | 一基编号 + 全空 | `os/servers/vfs/src/device_map.rs:fn test_smap_init` |
| `test_register_plan_matrix` | `minix3/minix/servers/vfs/smap.c:smap_map` | 复用/占位/校验/副作用/满 | `os/servers/vfs/src/device_map.rs:fn test_register_plan_matrix` |
| `test_domain_checks` | `minix3/minix/servers/vfs/smap.c:smap_map` | 四值 + 槽 helpers | `os/servers/vfs/src/device_map.rs:fn test_domain_checks` |
| `test_smap_dev_codec` | `minix3/minix/servers/vfs/smap.c:make_smap_dev` + `minix3/minix/servers/vfs/smap.c:get_smap_by_dev` | 拼合拆分 + 查询 | `os/servers/vfs/src/device_map.rs:fn test_smap_dev_codec` |
| `test_recover_step_matrix` | `minix3/minix/servers/vfs/dmap.c:dmap_endpt_up` | 三态 + 三分类 | `os/servers/vfs/src/device_map.rs:fn test_recover_step_matrix` |
| `test_ioctl_route_and_grant` | `minix3/minix/servers/vfs/device.c:do_ioctl` + `minix3/minix/servers/vfs/device.c:make_ioctl_grant` | 三路 + 交叉 + 分流 | `os/servers/vfs/src/device_map.rs:fn test_ioctl_route_and_grant` |
| `test_errno_map_covers_device_c` | 三文件全文件 | 7 变体→errno 全映射 | `os/servers/vfs/src/device_map.rs:fn test_errno_map_covers_device_c` |
| `test_death_cascade_char_invalidates_by_major` | `minix3/minix/servers/vfs/dmap.c:dmap_endpt_up` | 字符家族：按 major 全量失效 | `os/servers/vfs/src/device_map.rs:fn test_death_cascade_char_invalidates_by_major` |
| `test_death_cascade_sock_invalidates_by_num` | `minix3/minix/servers/vfs/smap.c:smap_endpt_up` | socket 家族：按一基号失效 | `os/servers/vfs/src/device_map.rs:fn test_death_cascade_sock_invalidates_by_num` |
| `test_death_cascade_block_is_recover_notice` | `minix3/minix/servers/vfs/dmap.c:dmap_endpt_up` | 块家族：只出恢复通知 | `os/servers/vfs/src/device_map.rs:fn test_death_cascade_block_is_recover_notice` |
| `test_death_cascade_no_row_is_inert` | 三文件全文件 | 无行死驱动：零失效（fail-safe） | `os/servers/vfs/src/device_map.rs:fn test_death_cascade_no_row_is_inert` |

测试策略：表以初始化/越界/生命周期覆盖；注册以复用/占位/校验四值/副作用/满覆盖；编解码以往返/拒绝/查询覆盖；恢复以三态/三分类覆盖；授权以交叉/分流/位值覆盖；错误以 7 变体全映射覆盖；死亡级联以三家族 + 空扫描覆盖。

---

## 6 过渡

本篇在 18（挂载标签来源）之后、20（块设备执行）之前，是“路由”的归属层：18 只管问标签，本篇管标签背后的活人；没有本篇的通讯录，20 的 `bdev_open` 不知标签去哪投，21/22 的 `cdev/sdev` 不知请求从哪来。

```
18-mount: mount_fs 问驱动标签（提交第 2 段）
   │
   └─► 本篇：dmap 按号查人 + smap 按域查人 + DS 按标签查人 / 生死更替清表续命 / ioctl 三路分派
          │                        │                        │
          ├─► 20-bdev：块设备打开与改道的执行（标签投递的终点）
          ├─► 21-cdev / 22-sdev：字符/socket 数据面的执行
          └─► 23-select：select 相回调与 cdev/sdev select 的执行
```

阅读顺序提示：若关心“标签投递之后块请求走哪”，下一站 `20-bdev.md`（`bdev_sendrec` 与 `bdev_reply`）；若关心“字符设备的打开执行”，下一站 `21-cdev.md`（`cdev_open` 与 `cdev_map`）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/dmap.c:1-328`（`lock_dmap/unlock_dmap/map_driver/do_mapdriver/dmap_unmap_by_endpt/map_service/init_dmap/dmap_driver_match/get_dmap_by_major/dmap_endpt_up/get_dmap_by_endpt`）、`minix3/minix/servers/vfs/smap.c:1-273`（`init_smap/smap_map/smap_unmap_by_endpt/smap_endpt_up/make_smap_dev/get_smap_by_dev/get_smap_by_endpt/get_smap_by_domain`）、`minix3/minix/servers/vfs/device.c:1-95`（`do_ioctl/make_ioctl_grant`）、`minix3/minix/servers/vfs/dmap.h:dmap`（表结构）、`minix3/minix/include/minix/dmap.h:NONE_MAJOR` + `minix3/minix/include/minix/dmap.h:NR_DEVICES`、`minix3/minix/include/minix/com.h:CDEV_IOCTL` + `minix3/minix/include/minix/com.h:CDEV_OP_RD`、`minix3/minix/include/minix/safecopies.h:CPF_READ`（`CPF_*`）、`minix3/sys/sys/ioccom.h:IOC_BIG`（尺寸位）、`minix3/sys/sys/socket.h:AF_MAX` + `minix3/sys/sys/socket.h:PF_MAX`
- 阶段文档：`18-mount.md`（标签来源）、`06-vmnt-table.md`（表语义类比）、`01-vfs-init-main.md`（启动调用点）、`20-bdev.md` / `21-cdev.md` / `22-sdev.md`（驱动执行）、`09-main-loop.md`（`SUSPEND` 路由）
- Rust 实现：`os/servers/vfs/src/device_map.rs`（本篇判定层）、`os/servers/vfs/src/open.rs`（`FileType` 复用）、`os/libs/minix-types/src/types/errno.rs`（errno 常量表，`EPERM` 等）
- 内核侧：`../01-stage-kernel/18-syscall-copy.md`（标签拷贝语义）

