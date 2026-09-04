# 13-devmand-consumer：devmand 消费契约

> **定位**：次主线中段（外部进程）。本篇回答：devmand 如何轮询事件、解析行、判定类型、生成 USB id、匹配驱动、启停进程、管理 major 号、跑脚本。全部是外部可观察契约——devmand 本体**不实现**（用户态守护进程，非 server 重写范围，plan §5.5）。
> **源码**：`minix3/minix/commands/devmand/main.c`（942 行）+ `usb.y`（134）+ `usb_scan.l`（43）+ `usb_driver.h`（匹配标志）+ `etc/devmand/`（`usb_hub.cfg`/`usb_storage.cfg`/`scripts/block`）。
> **Rust 实现**：无（外部契约文档；本篇 §5 为契约测试点——可执行断言写在 06/11 侧，此处只列对照表）。
> **前置依赖**：06（事件行格式）、04（路径）、11（属性拼法）、05（-11 预算）。
> **不覆盖（移交）**：server 内部（01~09）、RS 流程（12）、驱动实现（usbd 等）。

---

## 1. 概念：devmand 是事件行的第一个读者

devman 生产事件行（06），devmand 是它的**第一个也是唯一读者**：死循环打开 `/sys/events`（默认 `args.path` + `"events"`，main.c:890-897），`fgets` 一行，处理一行，关文件，50ms 后再来（main.c:876-932 主循环——**轮询，无 inotify**：`fopen` 每次重开，空读 `usleep(50000)`，唯 `ENFILE` 重试、`open` 失败即退出并 `cleanup`）。一次只读一行（`fgets(buf, 256)` 即使文件里攒了多行——攒行的消费是"一次循环一行"，慢消费者不丢事件（队列在 devman 侧攒着），只延迟。

读到行后三岔：ADD → 解析→判型→USB 接口则匹配起驱动；REMOVE → 按 id 停驱动；其他 → 警告忽略。以下逐段锁定可观察行为（devman 侧的输出必须满足这些解析器——**消费者契约反向约束生产者**，06/11 的单测即按此写）。

### 1.1 边界声明

本篇是契约（devmand 被允许假设什么），不是手册（devmand 内部怎么组织代码）——后者随 devmand 版本变，前者是跨进程承诺。实现细节只写到"约束 devman 输出"的深度。

---

## 2. C 源码分析（契约抽取）

### 2.1 行解析：`sscanf` 双格式（main.c:803-872）

```c
res = sscanf(event, "ADD %s 0x%x", tmp_path, &dev_id);      /* ADD */
res = sscanf(event, "REMOVE %s 0x%x", tmp_path, &dev_id);   /* REMOVE */
```

`res != 2` 即警告 + 忽略（:815-818）。约束 devman 侧（06 §2.4 的回声）：行首 `ADD `/`REMOVE ` 前缀 + 空格 + 路径（`%s` 遇空即停——**路径禁空格**，隐含契约）+ 空格 + `0x%x`（小写 hex 前缀；`%x` 大小写皆收，但生产侧恒小写，09 测试锁小写）+ 无尾换行要求（`fgets` 带 `\n` 进来也照解——`%x` 停在换行前）。

### 2.2 类型判定：读 `dev_type` 文件（main.c:519-560）

`path + "dev_type"`（`DEVMAN_TYPE_NAME`，:16）→ `fscanf "%255s"` → `"USB_DEV"`/`"USB_INTF"`/其他（→ `DEV_TYPE_UNKOWN`——C 拼写 sic，:53-56，记录不继承）。文件打不开/解析失败 → 同样 UNKNOWN（:542/:550——**缺文件不是错误**，是"未知"，流程继续走 default 分支忽略）。

约束 devman 侧（11 §2.1 的回声）：每个设备/接口目录下必须有 `dev_type` 文件，内容恰为 `USB_DEV`/`USB_INTF`（11 的属性拼法与之一一对应，devmand 的 `strcmp` 是字节级）。

### 2.3 分流：设备忽略，接口干活（main.c:821-835）

```c
case DEV_TYPE_USB_DEVICE:  dbg("USB device added: ommited...."); break;  /* 忽略 */
case DEV_TYPE_USB_INTF:    usb_intf_add_event(path, dev_id); return;
default:                   fprintf(stderr, "WARN: ommiting event\n");
```

**USB 设备事件被故意忽略**（"ommit[ting] … for now"，拼写 sic）——只有接口事件起驱动。约束含义：设备 ADD 行是纯信息（devmand 不消费，但行必须合法——解析器先于分流执行，坏行在分流前就被警告了）。

REMOVE 更值得多看一眼（:837-871）：`usb_intf_remove_event(path, dev_id)` 被调时 **`path` 是空串**（:807 `path[0]=0`，REMOVE 分支不拼路径——拼路径的代码被 `#if 0` 包着，:856-870）！约束含义：REMOVE 的清理**只认 dev_id**（`find_instance(dev_id)`，:211-222），路径字段可空——devman 侧 REMOVE 行的路径是"仅供人类阅读"的（devmand 根本不用它）。本契约明确记录（后人若"修复"空路径，反而破坏"只认 id"的简洁性——**quirk 即契约**，模式 78 的反面：C 的怪但无害，不标 BUG 不修）。

### 2.4 USB id 生成：8 次读，设备零（main.c:633-690）

`generate_usb_device_id(path, is_interface)`：接口 → 读 5 个 `../` 设备属性（idVendor/idProduct/bDeviceClass/bDeviceSubClass/bDeviceProtocol）+ 3 个 `/` 接口属性（Class/SubClass/Protocol），格式 `"0x%x\n"`（`read_hex_uint`，:573-591——**缺文件返 `EEXIST`**（:585，错码命名实锤：缺文件与"已存在"无关，调用方只判零非零，行为正确名字错误，记录不继承）+ 解析失败 `EINVAL`）；失败任一即整单作废（`goto err` → NULL → 事件忽略，:701-709）。

设备（`is_interface == FALSE`）→ **零读取**（`if` 块整体跳过，:645-678）→ 返回全零 id。全零 id 参与匹配（§2.5）——实际效果：USB 设备事件本就被 §2.3 忽略，此函数 device 分支近乎死代码（"近乎"：留着供未来，契约上 devman 不 needs to care）。

`#if 0` 的 bcdDevice 块（:654-659）：死代码，11 不拼它（11 §2.1 无此属性——对照一致）。

### 2.5 匹配：9 标志与 1 个复制粘贴 bug（main.c:238-296 + usb_driver.h:7-15）

`match_usb_id` 逐标志比对（9 项全等即中；`match == 0` 返 0——空规则集永不中，:289-291）。**`USB_MATCH_DEVICE_CLASS`（1<<3）从未被检查**：函数体查了 `DEVICE_PROTOCOL` **两次**（:247-248 与 :251-252 同条件），漏了 `DEVICE_CLASS`（:250 应为 CLASS，实为 PROTOCOL 复述）。grep 实证 + 行号双锚。约束含义：devman 侧 `bDeviceClass` 属性**可写可不写**——写了也没人看（但 11 照写：拼法完整性与匹配有效性是两回事，11 §4 注记"写而不读"，与 03 §2.4 的 subsystem 双死呼应成趣）。

`match_usb_driver`（:274-287）：drivers 表 × ids 表双循环，首中即返（**首配优先**，非最优——配置顺序即优先级，运维含义）。

### 2.6 DSL：`usb_driver` 块（usb.y + usb_scan.l + etc/devmand/*.cfg）

```conf
usb_driver usb_storage
{
    binary = /service/usb_storage;
    id { bInterfaceClass = 0x08; }
    devprefix = usb_disk;
    upscript = /etc/devmand/scripts/block;
}
```

yacc 文法（usb.y：token :28，`drivers` 规则 :32，`driver` 规则 :40-80）：`usb_driver NAME { 语句* }`，语句含 `binary/id/devprefix/upscript/downscript`（token 表见 usb.y:28）。`id { … }` 内条目置对应 match 标志（某 .y 动作段，行号从简——DSL 语义稳定，行号次要，scan 记录 L3 证据降级说明）。现货两配置：hub（intf class 09，前缀 usb_hub）与 storage（intf class 08，前缀 usb_disk，up 脚本 block）。

### 2.7 启停：`minix-service` 命令行 + 脚本契约（main.c:82-233）

- 起驱动：`minix-service up <binary> -major <major> -devid <dev_id> -label <label>`（:202-205），label = `devprefix + dev_id`（:192-197，超 `DEVMAND_DRIVER_LABEL_LEN` 即 `ENOMEM` 罢工）。
- 停驱动：`minix-service down <label> <dev_id>`（:171-174）。
- up 脚本：`<upscript> up <label> <major> <dev_id>`（:91-94）；down 脚本：`<downscript> down <label> <major>`（:140-142）；clean 脚本：`<upscript> clean <devprefix>`（:114-116）。失败皆 `EINVAL`（:96/:144/:118——脚本错即事件错，不重试）。
- block 脚本现货（etc/devmand/scripts/block）：`up` → `mknod /dev/<label> b <major> <minor>` 一组（p0/p1…/p3s3 全家桶，:9-30）；`down` 对称删除（:31 起）。**`/dev` 节点是脚本 mknod 的，不是 devman 建的**——devman 只管 `/sys` 树，`/dev` 树归脚本（两树分工，00 §1.1 的"设备树即文件树"特指前者，此处正名）。
- 顺序（`usb_intf_add_event` 全序，:691-762）：id 生成 → 匹配（无匹配静默返）→ 实例 calloc → `get_major`（无号即返，:726-731）→ `start_driver`（**返回值被忽略**，:741 裸调——起失败照样入表占 major，无 `put_major` 回滚）→ upscript（败则停驱动+释放实例）→ 入表。失败步步返（唯 start 例外），无回滚之外。

### 2.8 major 位图：16×8（main.c:596-631）

`get_major` 从 `major_offset` 起找首个置位（`major_bitmap[16]` 全 `0xff` 初始化，main_loop :883），清零返号；穷尽返 `INVAL_MAJOR`（-1，:18）。`put_major` 逆运算（`assert(major >= 0)`）。位图 128 号，上限即契约（128 个设备文件实例，多了就 `INVAL_MAJOR` 拒）。

### 2.9 主循环与杂项（main.c:876-942/418-430）

轮询（§1 已述）+ 路径长度 guard（`len > 128-7` 即 `"pathname to long"` sic + 退出，:887-892）+ `create_pid_file`（:418-430，路径从简）+ `cleanup`（实例逐个 down+stop+释放，:397-416，退出路径）。

---

## 3. Rust 设计决策（契约无实现，决策即"devman 侧如何满足"）

本篇无 Rust 模块，设计决策体现为对 devman 侧（01~12）的约束反查——每条都是"若 devman 改 X，devmand 哪行会炸"：

| devmand 假设 | devman 侧落实 | 断裂后果 |
|---|---|---|
| 行格式 `ADD %s 0x%x` | 06 事件行（无尾 `\n` 可，有亦可） | 解析 WARN+忽略 |
| `dev_type` 文件内容字节级 | 11 拼法（USB_DEV/INTF 压轴） | UNKNOWN→忽略 |
| 路径禁空格（`%s`） | 07 空白名拒绝（OQ-3 已决：认领期 `EINVAL`，零副作用） | 有主（07 §3.5） |
| 属性 `0x%x` 可解析 | 11 `0x%02x/%04x` | EINVAL→事件忽略 |
| REMOVE 只认 id | 09/08 id 稳定（墓碑不断号！） | 错杀——04 墓碑设计的外部理由在此 |
| 8 属性可读 | 11 属性全集（含 bDeviceClass 虽无人读） | EEXIST→事件忽略 |
| major 上限 128 | devman 侧无对应（实例数由驱动数定） | devmand 拒（devman 无感） |

OQ-3（已决，用户批准）：设备名空格——07 认领期拒绝含 ASCII 空白名（`EINVAL`）。倾向即决议（错配静默比失败更坏）；C 无此检查（新行为，已标注）。

---

## 4. 实现详解（无模块，契约测试点对照表）

| devmand 行为 | devman 侧可执行断言位置 | 状态 |
|---|---|---|
| ADD 行解析 | 07 `add_full_flow` 事件行精确串 | ✅ 已有 |
| REMOVE 行解析 | 08 `del_unbound_removes_and_emits` 精确串 | ✅ 已有 |
| dev_type 字节级 | 11 `attributes_match_c_spellings` 压轴断言 | ✅ 已有 |
| 属性 hex 可解析 | 11 同上（`0x1234` 形） | ✅ 已有 |
| 路径禁空格 | —（OQ-3 缺口） | ⚠️ 待决 |
| REMOVE 只认 id | 08 墓碑测试（号稳定） | ✅ 已有 |
| 两步 drain | 06 `drain_takes_two_reads` | ✅ 已有（devmand 双读隐含依赖） |

---

## 5. 测试要点（契约篇：无专属测试，§4 表即测试清单）

Gate D/E 对契约篇的适用性（00 §5 同款）：D-1 N/A（无专属测试函数；覆盖由 06/07/08/11 单测承载，§4 表逐项映射）/ D-2 N/A（无 trait）/ D-3 ✅（引用文件全存在）/ D-4 ✅（无算法声明）/ D-5 ✅（§2 行号 vs C 全一致，见 scan）。

---

## 6. 过渡

外部视角其一完毕：devmand 轮询、解析、匹配、启停、脚本、major，全是可观察契约。还剩 99 把全 stage 常量/错误码/跨服务引用收成一表——读 99 时若忘记某常数在哪定义的，回看各篇 §2 的行号（本篇的可 grep 性即 99 的输入）。

---

## 7. 参见

- `06-event-buf.md` — 事件行格式（本篇 §2.1 的生产侧）
- `11-usb-device-model.md` — 属性拼法（本篇 §2.2/§2.4 的生产侧）
- `04-device-tree.md` — 路径与墓碑（本篇 §2.3/§3 表的支撑）
- `12-rs-integration.md` — 另一外部视角（RS 侧）
- `99-devm-global-concepts.md` — 常量收口（含本篇 `dev_type` 等约定）
- C 源：`minix3/minix/commands/devmand/main.c`、`usb.y`、`usb_scan.l`、`usb_driver.h`、`minix3/etc/devmand/`
