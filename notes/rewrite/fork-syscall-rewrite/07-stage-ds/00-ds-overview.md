# 00-ds-overview: DS 整体架构概览

> **重写**: 2026-09-20（edge3 卡N/S41）
> **状态**: 正文
> **定位**: 全文档导航（阶段 0 总览）
> **源码**: `minix3/minix/servers/ds/`（main.c 132 行 + store.c 679 行，合计 811 行）+ boot 证据（`kernel/table.c:52`、`kernel/main.c:263-268`）
> **Rust 模块**: `os/servers/ds/` 全部（112 测试）
> **draft 素材**: `draft/README.md`（占位，已并入各篇）

## 1 概念：DS 是什么，为什么它是这个形状

### 1.1 一张发布/订阅键值表，撑起整个系统的"谁在哪"

DS（Data Store，数据存储服务器）是 Minix3 的发布/订阅数据存储服务：任何系统服务都可以把一段字节挂在一个键上（publish），别的服务按键取回（retrieve），键消失或更新时订阅者能被唤醒（subscribe/check）。它最广为人知的角色是**动态注册中心**——驱动把"label → endpoint"的映射发布上去（`ds.h` 的键名约定 `drv.<class>.<name>`），RS、VFS、输入服务器各自来查。没有 DS，标签就只是字符串；有了 DS，标签才是一张可以握手的名片。

它为什么是这个形状？因为 Minix3 的服务是互相不知情的用户态进程：RS 重启一个驱动后端点会变，VFS 不能把端点写死在代码里。DS 把这个变化收敛成一张表——变化的双方各自与 DS 打交道，彼此永不直连。整张协议面因此可以很窄：六个请求（`DS_PUBLISH`/`RETRIEVE`/`SUBSCRIBE`/`CHECK`/`DELETE`/`RETRIEVE_LABEL`，`DS_RQ_BASE 0x800` 起，`com.h:498-507`；Rust 单一事实源在 `minix-types/src/types/com.rs` 的 `DS_*`）。

minix-rs 保留这套协议与形状，把 DS 改写成单线程事件循环服务器。键表的数据结构与身份判定规则见 `03-ds-data-structures` 与 `05-ds-identity-auth`；全 crate 112 个测试守住协议面。

### 1.2 启动主线：boot 第一个用户态服务

DS 在 boot 链里的位置很特殊——它是**登记顺序第一个**的用户态服务（`kernel/table.c:52`，`{DS_PROC_NR, "ds"}` 紧随 kernel/HARDWARE 之后），但**执行顺序**上并不特殊：所有用户态进程都要等 VM 建好页表才放行（`kernel/main.c:263-268` 的 `RTS_VMINHIBIT | RTS_BOOTINHIBIT`）。"登记第一"与"执行被抑制"是两层语义， DS 的启动主线按这条线走：

```
boot 镜像装载（table.c:52 登记 DS_PROC_NR）
  └─ main() (main.c)                        ← 01-ds-init-main
       └─ sef_local_startup()               三次 SEF 注册
       └─ sef_cb_init_fresh()               清键表
       └─ 主循环                            ← 02-ds-message-contract
            receive → 六请求分发（publish/retrieve/
            subscribe/check/delete/retrieve_label）
```

### 1.3 服务面：三个数字

六个协议请求（`com.h:498-507`）、一张键表（槽位与标志位见 `04-ds-slot-management`，`DSF_IN_USE 0x001` 起，`ds.h:12`；Rust 权威 `minix-types/src/types/ds_store.rs`）、一个系统信息出口（`SI_DATA_STORE = 5`，`sysinfo.h:13`，让 IS 能整表快照，`11-ds-getsysinfo`）。

### 1.4 设计原则（全阶段文档共守）

- **位置可回答性**：任何机制问题都有一个确定的篇章可以回答，不靠全库搜索
- **禁止前向引用**：阅读路径只向后依赖，机制细节在所属篇章，调用点只留锚
- **每篇一个语义单元**：一篇讲透一个机制，不做"杂物章"
- **ARCH 三处一致**：架构级改写必须同时标注在文档正文、design 快照、代码注释三处

## 2 C 源码分析：两个文件的分组地图

| 文件 | 行数 | 职责 | 对应篇章 |
|---|---|---|---|
| `main.c` | 132 | 入口、SEF 注册、主循环分发 | 01、02 |
| `store.c` | 679 | 键表数据结构、六个请求 handler、订阅队列 | 03~11 |

共享面的对位：客户端库 `minix3/minix/lib/libds/`（`12-ds-client-library`）→ Rust `minix-sys` 的 `DsClient`；键表快照的线格式在 `minix-types/src/types/ds_store.rs`（生产者/消费者共用的单一权威）。

## 3 文档导航：14 篇的阅读顺序

- **阶段 1 启动与协议**：01（init/main）、02（消息契约）
- **阶段 2 数据面**：03（键表结构）、04（槽位管理）、05（身份判定）、06（boot 键映射）
- **阶段 3 协议实现**：07（publish）、08（retrieve）、09（delete）、10（subscribe/check）、11（getsysinfo 快照）
- **阶段 4 消费端**：12（客户端库）
- **阶段 5 收口**：99（全局常量、错误码、跨服务引用）

## 4 边界

- **前置依赖**: 无（DS 是 boot 链最底层的服务之一）
- **不覆盖（移交）**: 一切机制细节（见 01~12、99）；订阅方如何消费唤醒见各消费端 stage；驱动标签的语义约定见 16-stage 驱动线

## 7 参见

- 蓝图与多轮重写记录：本目录 `doc_rerank_*.md`
- 消费方全景：99 篇第 3 节的跨服务引用表
