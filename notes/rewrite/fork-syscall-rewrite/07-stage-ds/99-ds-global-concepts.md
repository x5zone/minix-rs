# 99-ds-global-concepts: DS 全局概念收口

> **重写**: 2026-09-20（edge3 卡N/S41）
> **状态**: 正文
> **定位**: 全局概念（阶段 99：常量表、错误码、跨服务引用）
> **源码**: `minix3/minix/include/minix/com.h:498-507`、`minix3/minix/include/minix/ds.h`、`minix3/minix/include/minix/sysinfo.h:13`
> **Rust 模块**: `minix-types/src/types/com.rs`、`minix-types/src/types/ds_store.rs`、`minix-sys/src/ds.rs`

## 1 概念：DS 的常量是三方合约的签名

### 1.1 协议族——一个名字空间，六个动词

DS 的全部请求住在 `DS_RQ_BASE 0x800`（`com.h:498`）里：`DS_PUBLISH`（+0）、`DS_RETRIEVE`（+1）、`DS_SUBSCRIBE`（+2）、`DS_CHECK`（+3）、`DS_DELETE`（+4）、`DS_RETRIEVE_LABEL`（+6）（`com.h:500-507`；Rust 单一事实源 `minix-types/src/types/com.rs` 的 `DS_*`）。六个动词就是发布/订阅语义的完整词汇表——协议面之外没有任何后门，`05-ds-identity-auth` 的身份判定与 `04-ds-slot-management` 的槽位管理都只为这六个动词服务。

### 1.2 键表标志与容量——一张槽位表的自述

键表槽位的标志位从 `DSF_IN_USE 0x001`（`ds.h:12`）起（Rust 权威 `minix-types/src/types/ds_store.rs` 的 `DSF_*` 族与 `DsEntrySnap` 快照行——生产者与 IS 快照消费共用的单一权威）。槽位数量与回收规则见 `04-ds-slot-management`。

### 1.3 系统信息出口

`SI_DATA_STORE = 5`（`sysinfo.h:13`）：IS 的 `dump_ds` 通过 getsysinfo 取整张键表的快照（`11-ds-getsysinfo`）。这是 DS 唯一的"整表出口"，快照行格式与键表内部结构共用同一份定义。

## 2 错误码——A-9 全集

DS 的拒绝面收敛在八个错误码（架构裁决 A-9）：

| 错误码 | 判据 | 代表场景 |
|---|---|---|
| `EPERM` | 身份不符 | 非 owner 改写/删除别人的键（05） |
| `EINVAL` | 参数非法 | 键名超长、负载越界（07） |
| `ESRCH` | 键不存在 | retrieve/delete 落空（08/09） |
| `ENOENT` | 订阅项缺失 | check 落空（10） |
| `EEXIST` | 键已存在 | publish 撞名（07） |
| `EAGAIN` | 资源暂不可得 | 槽位满（04） |
| `ENOMEM` | 内存不足 | 负载拷贝失败（07） |
| `EDONTREPLY` | 挂起不回复 | subscribe 等唤醒（10） |

Rust 侧全部消费 `minix_types` 常量，DS 服务器不发明新错误码。

## 3 跨服务引用——谁在用 DS，用什么姿势

| 消费方 | 锚点 | 用法 |
|---|---|---|
| RS | `servers/rs/manager.c` 的 `rproc` 表（L513 附近）与重启流程 | 驱动/服务生命周期键的发布与更新 |
| VFS | `servers/vfs/main.c:105`（`ds_event` 进入主循环）、`misc.c:960-985`（`ds_check` 排空 + `ds_retrieve_u32`，`value != DS_DRIVER_UP` 跳过，`ds.h:32`） | 订阅驱动上线事件（`drv.*` 键），驱动死亡级联的触发源 |
| PM | `servers/pm/misc.c` | 键查询 |
| INPUT | `servers/input`（`input.c:488-593` 一带） | 设备标签发布 |
| 驱动库 | `DS_DRIVER_UP` 值语义（`ds.h:32`） | 驱动上线键的约定值 |
| IS | `dmp_ds.c`（`SI_DATA_STORE` 快照消费） | 整表 dump |

Rust 侧的对位：VFS 的订阅在 `sef.rs`/启动段（`minix-sys` `DsClient::subscribe`，S13 W5），键回拷走 `DsClient::check`（C-13 销账），标签查询走 `DsClient::retrieve_label_name`（C-9 销账）。

## 4 执行模型声明

DS 是单线程事件循环服务器：一个 `receive` 循环顺序处理六请求，订阅唤醒靠下次事件驱动，无内部并发。订阅者的"被唤醒"不是 DS 主动推送——是订阅者的 check 循环或 SEF 通知抵达后按 10 号篇的规则回查。

## 5 有意省略表（intentional omissions）

- **订阅匹配的模式语法**（通配符规则）的完整规则见 10 号篇；
- **键名命名约定的完整表**（`drv.*` 等前缀语义）归各消费方 stage 与 16-stage 驱动线；
- **libds 客户端的函数清单**见 12 号篇。

## 6 参见

- 阶段内：02（消息契约的线格式）、04（槽位）、10（订阅/check 全语义）、11（快照出口）、12（客户端库）
- 本线状态：`../../edge3.md` S24 行（mib_get_label 接线，C-9 销账）
