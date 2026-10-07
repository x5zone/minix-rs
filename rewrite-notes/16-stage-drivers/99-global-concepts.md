# 99-global-concepts — 驱动子系统全局概念

> **状态**: 已展开（2026-09-17 全量扫描后定稿；`.design/` 快照随本版生成）
> **前置依赖**: 无
> **本篇不覆盖**: 常量如何被使用（各文档）；块框架三套循环（02）；字符主循环细节（01）。
> **参见**: `plan.md`（覆盖契约与本篇的产出依据）、`com.h`（C 侧单一权威）

## 1. 概念：一本全子系统共用的电话号码簿

Minix3 的驱动子系统有五十七个进程，但请求编号只有一本电话号码簿：`minix3/minix/include/minix/com.h`。字符设备从 `0x400` 起拨号，块设备从 `0x500` 起，网络从 `0x1A00` 起，实时时钟从 `0x1400` 起，USB 从 `0x1100` 起。号码簿分两栏——请求栏（VFS 打给驱动）与回复栏（驱动答回 VFS），两栏基址不同：字符请求 `0x400` 配回复 `0x480`，块请求 `0x500` 配回复 `0x580`。号码错了，消息会落进别家的号码段——一个 `0x500` 的字符回复在线上就是一个块请求。所以这本簿子的规矩有两条：值不许错（F1 的教训：`CDEV_RS_BASE` 一度写成 `0x500`）；定义不许重复（同名常量在多个 crate 各写一份，迟早漂移，见第 4 节）。

## 2. C 源码分析：五个家族与一个通用模型

### 2.1 请求与回复基址（`com.h`）

| 家族 | 请求基址 | 判别掩码 | 回复基址 | 依据 |
|------|----------|----------|----------|------|
| 字符 CDEV | `0x400`（919） | `& ~0x7f` | `0x480`（920） | `IS_CDEV_RQ`/`IS_CDEV_RS`（922-923） |
| 块 BDEV | `0x500`（963） | `& ~0x7f` | `0x580`（964） | `IS_BDEV_RQ`/`IS_BDEV_RS`（965-966） |
| 网络 NDEV | `0x1A00`（1085） | `& ~0x7f` | `0x1A80`（1086） | `IS_NDEV_RQ`/`IS_NDEV_RS` |
| 时钟 RTCDEV | `0x1400`（995） | `& ~0x7f` | `0x1480`（996） | `IS_RTCDEV_RQ`/`IS_RTCDEV_RS`（998-999） |
| USB USB_RQ | `0x1100`（813） | 逐号 | — | `USB_RQ_INIT`(0) 到 `USB_WITHDRAW_DEV`(8)（813-828） |

判别掩码的含义：消息类型的低七位是家族内编号，掩掉之后剩下的高段就是家族号。同一族内编号加一递增；跨族基址相距至少一百二十八，互不重叠。

### 2.2 家族内编号

- **CDEV 七请求**（`com.h:926-932`）：OPEN(0) CLOSE(1) READ(2) WRITE(3) IOCTL(4) CANCEL(5) SELECT(6)；回复三值（935-937）：`CDEV_REPLY`、`CDEV_SEL1_REPLY`（立即选择答复）、`CDEV_SEL2_REPLY`（迟到的选择通知）。标志（946-956）：`CDEV_NONBLOCK 0x01`、`CDEV_R_BIT 0x01`、`CDEV_W_BIT 0x02`、`CDEV_NOCTTY 0x04`、`CDEV_CLONED 0x2000_0000`、`CDEV_CTTY 0x4000_0000`。
- **BDEV 七请求**（`com.h:970-976`）：OPEN(0) CLOSE(1) READ(2) WRITE(3) GATHER(4) SCATTER(5) IOCTL(6)；标志（982-987）：`BDEV_R_BIT 0x01`、`BDEV_W_BIT 0x02`、`BDEV_FORCEWRITE 0x01`。
- **NDEV 六请求**（`com.h:1096-1101`）：INIT(0) CONF(1) SEND(2) RECV(3) IOCTL(4) STATUS_REPLY(5)；配置子类型五位、模式位六位、能力位九个（六个校验和分项加组播广播改地址三个仅协商位）、专用标志四位、链路三态（1126-1145，详见 03 §2.2）。
- **RTCDEV 五请求**（`com.h:1002-1012`）：GET_TIME SET_TIME PWR_OFF GET_TIME_G SET_TIME_G，回复 `RTCDEV_REPLY`，标志 `RTCDEV_Y2KBUG 0x01`。
- **USB 八请求**（`com.h:813-828`）：INIT(0) DEINIT(1) SEND_URB(2) CANCEL_URB(3) SEND_INFO(4) COMPLETE_URB(6) ANNOUCE_DEV(7) WITHDRAW_DEV(8)。载荷槽位（829-841）：`USB_GRANT_ID`/`USB_GRANT_SIZE` 走 m4 前两长、`USB_URB_ID`/`USB_RESULT` 同理、`USB_DEV_ID`/`USB_DRIVER_EP`/`USB_INTERFACES` 三长、`USB_RB_INIT_NAME` 走 m3 字符臂、`USB_INFO_TYPE`/`USB_INFO_VALUE` 两长。
- **SDEV**（`com.h:1037-1068`，基址 `0x1900`）：sockdriver 语义，归 17-stage-net（`plan.md` §5.4），本篇只记号码。

### 2.3 通用驱动模型（`driver.h`）

所有家族共享三样：`MAX_NR_OPEN_DEVICES 256`（driver.h:41，重启门的开机表宽）；`driver_receive`（统一收包入口）；端点约定（消息的 `m_source` 由内核填发送方端点，驱动回信时 `m_source` 由调用方覆盖为本进程端点）。

### 2.4 次设备号与 /dev 命名

次设备号是驱动私有的地址空间，家族内无全局约定——字符驱动自行分号（tty：控制台 0..3、日志 15、视频 125；内存：0..12，`dmap.h:85-92`），块驱动以主设备号挂表。/dev 命名由文件系统侧的设备表决定，驱动只认号。

### 2.5 errno 映射

请求处理失败一律回负的 Minix3 errno：无此设备 ENXIO、无效参数 EINVAL、输入输出错 EIO、稍后重试 EAGAIN、忙 EBUSY、不许做 EPERM、不合适的控制请求 ENOTTY。每篇文档的错误表给出各自场景到这些码的映射；禁止自造号码（F3 的教训）。

## 3. Rust 设计决策

### 3.1 常量的单一权威

C 一份头文件喂饱所有进程；Rust 的常量天然按 crate 散布，生产侧（框架库 `protocol.rs`）与消费侧（`servers/vfs` 的 cdev.rs、bdev.rs；`servers/input` 的 framework.rs）各自定义过同名值，其中一处已经分叉出错误值（F1）。收敛方向：设备族线上常量上收 `minix-types` 单点（对齐 E-REQWIRE/E-MINTYPES-SYS 的既定纪律，登记为 edge E-DEVWIRE），各框架库以类型别名保留家族名，测试对 `com.h` 逐值钉死防漂移。号码簿的权威在本篇的表里；实现位置随收敛进度更新。

### 3.2 家族判别做成枚举

C 用掩码加差值解出家族内编号；Rust 每族一个枚举（`CdevRequest`、`BdevRequest`、`NdevRequest`、`RtcRequest`、`NdevReply`），`decode` 失败返回空，越界编号在类型外。判别掩码 `& !0x7f` 逐族保留为谓词函数。

## 4. 错误处理

本篇无自身错误路径。号码簿自身的两类病——错值与重复定义——分别在 F1（已修）与 edge E-DEVWIRE（收敛中）跟踪。

## 5. 测试

各框架库的常量钉值测试逐值对照本篇表格：chardriver 21 中含回复基址钉子；blockdriver 13、netdriver 52（含能力位九值、标志四位、回复六值）、bdev 20、readclock 12（`cargo test -p {crate} --lib`）。

## 6. 过渡

号码簿在手，随后的每一篇都是拿号码办事：01 拿 CDEV 七号开字符主循环，02 拿 BDEV 七号开块主循环，05 一篇里同时用两族号码（内存驱动是唯一 char+block 双面设备）。

## 7. 参见

- `minix3/minix/include/minix/com.h`：C 侧单一权威（全部行号锚点的出处）。
- `minix3/minix/include/minix/driver.h`：通用驱动模型与打开设备表宽。
- `rewrite-notes/16-stage-drivers/01-chardriver-framework.md`：字符家族用法。
- `rewrite-notes/16-stage-drivers/02-blockdriver-framework.md`：块家族用法。
- `rewrite-notes/16-stage-drivers/03-netdriver-framework.md`：网络家族用法。
- edge `E-DEVWIRE`：常量单一来源的收敛执行条目。
