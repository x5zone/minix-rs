# 12-rs-integration：RS 集成契约

> **定位**：devman 进程生死两端的 RS 侧。本篇回答：RS 何时替服务 BIND/UNBIND（publish/unpublish 流程）、devman_id 怎么继承、devman 自身以什么权限被拉起、失败意味着什么。
> **源码**：`minix3/minix/servers/rs/manager.c:840-851`（publish 握手）、`:897-909`（unpublish 握手）、`:1742`（`init_slot` 继承）、`minix/include/minix/rs.h:139,182`（`devman_id` 字段）、`minix3/etc/system.conf:422-429`（service 权限）。
> **Rust 模块**：`os/servers/devman/src/rs_contract.rs`（握手决策 + 传输注入）。
> **前置依赖**：09（转发的 devman 侧）、05（RS-only 门与 Endpoint 预填）。
> **不覆盖（移交）**：RS 内部实现（RS stage）、DS 实现（接口注入）、驱动侧（10/11）。

---

## 1. 概念：RS 是 devman 的出生证明与婚姻介绍所

RS 与 devman 有两层关系。**纵向**：RS 按 `system.conf` 把 devman 进程拉起来（uid 0 + 两项 VM 特权，01 §2.8 已见）——devman 的"第零步"在 RS。**横向**：每个服务 publish 时，RS 若见其 `devman_id != 0`，就替它向 devman 发 BIND（unpublish 时发 UNBIND）——RS 是服务与驱动的介绍人，devman 只认证介绍人（RS-only 门，05 §2.4），不认证新人。

`devman_id` 的一生三站：boot 镜像带来（`init_slot` 继承，:1742）→ publish 时用掉（BIND，:840-851）→ unpublish 时注销（UNBIND，:897-909）。本篇锁定三站的 Rust 对应物。

### 1.1 边界声明

本篇讲 RS 侧契约（时机、填字、失败处置）与 devman 视角的建模。不讲：RS 内部（slot 管理、kill 实现）、DS 查询实现、驱动行为。

---

## 2. C 源码分析

### 2.1 publish 握手：查表 → 填字 → sendrec → 不成即杀（manager.c:840-851）

```c
if (rpub->devman_id != 0) {
    r = ds_retrieve_label_endpt("devman", &ep);
    if (r != OK) {
        return kill_service(rp, "devman not running?", r);
    }
    m.m_type = DEVMAN_BIND;
    m.DEVMAN_ENDPOINT  = rpub->endpoint;
    m.DEVMAN_DEVICE_ID = rpub->devman_id;
    r = ipc_sendrec(ep, &m);
    if (r != OK || m.DEVMAN_RESULT != OK) {
        return kill_service(rp, "devman bind device failed", r);
    }
}
```

四步：id 门槛（0 即无设备服务，大多数服务走空路——`NotApplicable`，单测锁零流量）；DS 查 devman 端点（查不着即杀，串名 "devman not running?"）；填字（ENDPOINT = 服务端点，DEVICE_ID = devman_id——09 §2.3"原样转交"的上游证据，字是 RS 填的）；`sendrec` 同步等（传输错**或** RESULT 非 OK 都杀——"devman bind device failed"，连 strat 都不给：publish 失败的服务不许带病上线）。

杀的含义：`kill_service` 终结正 publish 的服务（manager.c:850-851 行内，无需展开实现——契约是"失败=死"，不是"怎么死"）。

### 2.2 unpublish 握手：同形不同命（manager.c:897-909）

```c
if (rpub->devman_id != 0) {
    r = ds_retrieve_label_endpt("devman", &ep);
    if (r != OK) {
        printf("RS: devman not running?");
    } else {
        m.m_type = DEVMAN_UNBIND;
        m.DEVMAN_ENDPOINT  = rpub->endpoint;
        m.DEVMAN_DEVICE_ID = rpub->devman_id;
        r = ipc_sendrec(ep, &m);
        if (r != OK || m.DEVMAN_RESULT != OK) {
            printf("RS: devman unbind device failed");
        }
    }
}
```

publish 的镜像，但失败只 `printf`（:894/:908）——unpublish 永不杀人（服务都要下了，杀它何用？失败语义是"warned"，单测锁三路全 Warned/Unbound）。不对称是刻意的：**上线严格，下线宽容**，与 09 §2.2 的"缺席 ENODEV 回复"对照读（两边都不为难下线）。

### 2.3 继承：`init_slot` 抄写（manager.c:1742）

```c
rpub->devman_id = rs_start->devman_id;
```

boot 镜像的 `devman_id` 抄进运行 slot（周围是 `dev_nr`/domain/pci_acl 的同形抄写，:1739-1750——devman_id 只是其中一行，无特殊）。纯拷贝，Rust `inherit_devman_id` 即恒等函数（单测锁 7→7/0→0——"恒等也测"是契约锁定的态度：抄写关系一旦变（比如将来校验），测试即红）。

字段定义：`rs.h:139`（`rss_*` 侧）与 `:182`（`rpub` 侧）各一处 `int devman_id`（boot 像与运行像各存一份，继承即前者→后者）。

### 2.4 出生权限：`system.conf:422-429`

```conf
service devman
{
    uid 0;
    vm
        SETCACHEPAGE
        CLEARCACHE
    ;
};
```

uid 0（系统所有）+ 两项 VM 特权（清/设缓存页——设备内存映射所需，01 §2.8 引、此处收口为"为什么是这两项"：devman 映射设备内存进 VTreeFS 缓冲，`buf` 即 06 的 `Buf` 后端，权限与机制对照）。99 收口跨服务权限表时复引本节。

---

## 3. Rust 设计决策

### 3.1 决策即枚举：`PublishOutcome/UnpublishOutcome`

C 用"继续/杀/打印"三种控制流表达契约，Rust 用枚举（`NotApplicable/Bound/KillService(&'static str)` 与 `NotApplicable/Unbound/Warned`）——kill 串用 C 原文（grep 可溯）。传输（DS 查 + sendrec）注入 `RsTransport`（测试替身三路全锁：零 id 静默/失败杀/成功绑；下线三路 Warned）。

### 3.2 devman 视角的 RS（本模块住 devman crate 的理由）

RS 实现归 RS stage，但 devman 必须**可测试地**谈论 RS（09 的 `check_rs` 认端点、本篇的握手认流程）——契约放 devman 侧，RS stage 实现时反向引用（单向依赖不断：devman 不依赖 rs crate，只依赖 `Endpoint` 数值 2，05 已收）。

---

## 4. 实现详解

### 4.1 模块结构

```
os/servers/devman/src/rs_contract.rs — PublishInfo/RsTransport/PublishOutcome/publish/UnpublishOutcome/unpublish/inherit_devman_id（+4 测试）
```

### 4.2 关键不变量

1. `devman_id == 0` 零流量（publish/unpublish 皆 `NotApplicable`，传输零调用——单测锁 `log.is_empty()`）。
2. publish 失败 ⟺ Kill（含 DS 败/sendrec 败/RESULT 非零三路）。
3. unpublish 永不 Kill（三路 Warned/Unbound）。
4. 继承即拷贝。

### 4.3 与 C 的差异说明

| C | Rust | 分类 |
|---|---|---|
| kill_service 调用 | `KillService(&str)` 返回（调用方执行） | 分层（决策与执行分离，单测无需 RS） |
| printf 警告 | `Warned` 返回 | 同上 |
| DS/sendrec 直调 | `RsTransport` 注入 | 注入（DS/IPC 未落地） |

---

## 5. 测试要点

| 测试 | 断言 | 对应 |
|---|---|---|
| `publish_zero_id_is_silent` | NotApplicable + 零传输 | §4.2-1 |
| `publish_failures_kill` | DS 败杀/结果败杀/成功绑 | §4.2-2 |
| `unpublish_never_kills` | 三路 Warned/Unbound | §4.2-3 |
| `inherit_is_copy` | 7→7/0→0 | §2.3 |

截至 2026-09-04：`cargo test -p minix-devman` **77 passed / 0 failed**（73 + 本篇 4）。`cargo clippy` devman 部分 0 警告。

---

## 6. 过渡

RS 侧闭环：出生（system.conf）→ 继承（init_slot）→ 上线握手（publish/BIND）→ 下线握手（unpublish/UNBIND）。还剩两个外部视角：devmand（轮询 events、匹配驱动、启停进程——13，用户态契约）与全局收口（常量/错误码/跨服务表——99）。读 13 时若忘记事件行长什么样，回看 06 §2.4（`ADD ./devices/usb/ 0x%08x` 无尾换行）与 11 §2.1（属性拼法对照 devmand 正则）。

---

## 7. 参见

- `05-devm-message-contract.md` — RS-only 门（本篇握手的 devman 侧门卫）
- `09-devm-bind-unbind.md` — 转发与应答（握手到达 devman 之后）
- `06-event-buf.md` — 事件行格式（devmand 解析对象）
- `11-usb-device-model.md` — 属性拼法（devmand 匹配对象）
- `13-devmand-consumer.md` — 消费全景（本篇的下线之后）
- `99-devm-global-concepts.md` — 权限/常量收口
- C 源：`minix3/minix/servers/rs/manager.c:840-851,897-909,1742`、`minix3/minix/include/minix/rs.h:139,182`、`minix3/etc/system.conf:422-429`
