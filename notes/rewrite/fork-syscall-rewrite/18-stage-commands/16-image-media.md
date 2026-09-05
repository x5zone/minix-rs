# 16-镜像与介质工具

> **状态**: 已完成，等待评审收敛
> **定位**: 交付因果链的搬运层——整个盘的复制、光盘镜像、内存盘
> **源码**: `minix3/bin/dd/`（参数表 `args.c` 第 105 到 121 行、转换 `conv.c`）、`minix3/minix/commands/isoread/isoread.c`（标识 `CD001` 第 41 行）、`minix3/minix/commands/writeisofs/`、`minix3/minix/commands/dosread/`、`minix3/minix/commands/vol/`、`minix3/minix/commands/eject/`、`minix3/minix/commands/cdprobe/`、`minix3/minix/commands/ramdisk/`、`minix3/minix/commands/loadramdisk/`、`minix3/minix/commands/rawspeed/`、`minix3/usr.sbin/vnconfig/`
> **Rust 模块**: `os/commands/bin/diskimg`（库包 `minix-diskimg`：`dd.rs`、`iso.rs`、`device.rs`，13 个测试通过）
> **前置依赖**: `14-mount-fsck.md`（挂载在先）、`15-partition-format.md`（切分在先）
> **不覆盖（移交）**: 驱动实现（见 `16-stage-drivers`）、介质控制执行（弹出探测的硬件面）、FAT 解析（后续文件系统阶段）

---

## 1. 概念：搬运整个盘的三件事

### 1.0 本章说明

本章讲盘级搬运：块复制（`dd`）、光盘镜像（ISO）、内存盘与虚拟盘。每件都是"字节从 A 到 B"，区别在寻址与封装。

> **本章不讲什么**：
>
> - 块设备驱动（见驱动阶段）
> - FAT 文件系统结构（后续文件系统阶段）
> - 光盘刻录（写在 `writeisofs` 的生成端，读在 `isoread`）
>
> 本章只讲搬运语言：操作数、卷识别、块寻址。

### 1.1 `dd`：块复制的通用语言

`dd if=输入 of=输出 bs=块长 count=块数 skip=跳过 seek=定位 conv=转换`——十七个操作数讲清一次搬运：从哪来（`if` 缺省标准输入）、到哪去（`of` 缺省标准输出）、多大块（`bs` 统设、`ibs`/`obs`/`cbs` 分设，支持 `K`/`M`/`G` 后缀与 `x` 连乘如 `1Mx2`，另有 `c` 字节、`w` 双字节后缀）、搬多少（`count` 块数、`files` 文件数）、跳过多少（`skip`/`iseek` 输入块、`seek`/`oseek` 输出块）、怎么变（`conv`：不断尾、补零、容错、大小写、字节对换）、怎么开（`iflag`/`oflag` 打开标志）、怎么报（`msgfmt` 人读或标准、`progress` 进度开关）。`dd` 无所不知，除了文件系统——它是"无视文件系统"的终极工具（恢复误删盘、写镜像、测速），也是"无视文件系统"的终极危险（`of` 写错盘即灾难，`conv=notrunc` 之外默认截断输出）。

### 1.2 光盘卷：`CD001` 五字节的身份

ISO 9660 主卷描述符（2048 字节扇区）：0 号字节类型 1、1 到 5 字节标识 `CD001`、6 号字节版本 1、40 到 71 字节卷标、128 起块大小（小端大端各存一遍，互为校验）。`isoread` 认标识读文件，`writeisofs` 写标识制镜像，`vol` 显示卷标，`eject` 弹盘，`cdprobe` 探测。本库 `iso.rs` 认"类型标识版本端序"四项（端序互校：小端大端不一致即撕裂扇区），卷标去尾空——认卷不读文件（文件目录树是后续阶段）。

### 1.3 内存盘与虚拟盘：盘在内存里

- `ramdisk`/`loadramdisk`：把镜像载入内存当盘用（安装盘、无盘站的启动术）。
- `vnconfig`：把常规文件配成虚拟节点盘（镜像不刻盘即挂载，`mount` 的前置动作）。
- `rawspeed`：裸读写测速（存储性能的基准尺）。
- `dosread`：读 FAT 文件（跨系统取文件的摆渡车，FAT 解析后续阶段）。

---

## 2. C 源码分析

### 2.1 `args.c` 第 105 到 121 行：操作数表

十七操作数各一行（名、处理函数、标志位）：`bs` 统设块长（附带输入输出双标志）、`cbs` 转换块、`conv` 转换表、`count` 计数、`files` 文件数、`ibs`/`obs` 分设、`if`/`of` 出入、`iflag`/`oflag` 打开标志（词表另见 `olist`）、`iseek`/`oseek` 与 `seek`/`skip` 同处理函数（别名关系代码即证）、`msgfmt`/`progress` 报告开关。`conv.c` 是转换实现（大小写、字节对换、补零容错）。Rust 侧 `dd.rs` 的 `CopyPlan`（全量解析为字节数，含文件数、标志词、报告开关）与 `Conversions`（六标志位）与之逐项对应；`block`、`unblock`、`ascii`、`ebcdic`、`ibm` 五转换识别但透传（需码表，后续小任务，代码注释写明）。

### 2.2 `isoread.c` 第 41 行：标识即本质

`ISO9660_ID "CD001"` 一行定义全篇的识别逻辑：卷识别不靠扩展名（`.iso` 可随便改），靠扇区里的五字节。Rust 侧 `parse_primary` 的四项检查（类型、标识、版本、端序互校）是"标识即本质"的完整实现——扩展名在解析器眼里不存在。

### 2.3 介质三件与内存盘族：职责级覆盖

`vol`（显示卷标）、`eject`（弹盘）、`cdprobe`（探测）各一句话；`ramdisk`、`loadramdisk`、`vnconfig`、`rawspeed`、`dosread` 各一句话（见 1.3 节）。源码存在性逐目录验证（11 目录全在）。

---

## 3. Rust 设计决策

### 3.1 为什么块设备是接口

复制、读像、校验都要"按号读块"，内存像与驱动盘行为一致（块大小、块数、按号读）。`BlockDevice` 接口配 `SliceDevice`（内存像：块即切片，短尾报错不编造）与 `EmptyDevice`（全缺席）——与 `09` 篇存储接口、`12` 篇进程表同一形状。本阶段第六次应用，不再论证，只记录。

### 3.2 为什么操作数全量解析为字节

`bs=1Mx2`、`skip=1`（输入块）、`seek=2`（输出块）的单位各异（字节、输入块、输出块），调用方（泵字节循环）只认字节。`CopyPlan` 把块长、跳过、定位全换算成字节（`copy_bytes` 给出总数，溢出即错）——单位换算在边界做一次，循环里全是字节。这是"边界换算、内部统一"的老规则（`15` 篇大小解析同理）。

### 3.3 为什么端序互校而不是只读一端

ISO 格式存双端序本就是为了检错：只读小端，对端损坏浑然不知。互校（不等即错）把"撕裂扇区"挡在识别层之外——认卷是信任的起点，信任必须验证。这是"格式自带的校验不用就是浪费"的实例（`11` 篇校验层同理）。

---

## 4. 实现详解

### 4.1 模块结构

`os/commands/bin/diskimg`（库包名 `minix-diskimg`）共 4 个源文件：

| Rust 文件 | 对应 C 源码位置 | 职责 |
|-----------|----------------|------|
| `lib.rs` | — | 错误类型（`ImageError`，22 对应参数无效、12 对应缓冲不足）与模块组织 |
| `dd.rs` | `args.c:105-121`（操作数）、`conv.c` 思想 | 操作数解析（`parse_operands`）与复制计划（`CopyPlan`） |
| `iso.rs` | `isoread.c:41`（标识） | 主卷识别（`parse_primary`，四项检查） |
| `device.rs` | 块寻址思想 | `BlockDevice` 接口、`SliceDevice` 与 `EmptyDevice` |

### 4.2 关键类型与不变量

- **复制计划 `CopyPlan`**：出入路径、块长、计数、跳定位、转换。不变量：块长非零；`bs` 统设双向；后词覆盖先词；未知名即错；计数溢出即错。
- **转换 `Conversions`**：六标志位。不变量：未知词即错；块类五转换识别透传（码表后续）；空表即错。
- **主卷 `PrimaryVolume`**：卷标与块大小。不变量：2048 字节整扇；类型 1；标识五字节；版本 1；双端序一致且非零；卷标去尾空。
- **块设备 `BlockDevice`**：块大小、块数、按号读。不变量：零块长即错；越界即错；短尾不编造；空设备全缺席。

### 4.3 函数一览

| 函数 | 输入 | 输出 | 对应 C 行为 |
|------|------|------|------------|
| `parse_operands` | 参数表 | 复制计划 | 操作数语义 |
| `parse_size` | 大小词 | 字节数 | 大小后缀语义 |
| `parse_primary` | 2048 字节 | 卷标块大小 | 卷识别语义 |
| `read_block` | 块号与缓冲 | 块内容 | 块寻址语义 |

---

## 5. 测试要点

`cargo test -p minix-diskimg`：**13 个测试，全部通过**（截至 2026-09-06）。

重点行为与测试的对应（以下函数名均可用 `rg "fn 测试名" os/commands/bin/diskimg` 复现）：

- **操作数**（`dd.rs`，6 个）：`test_full_invocation`（完整调用：路径、块长、总数）、`test_multiplication_and_suffixes`（连乘、后缀、字后缀）、`test_skip_seek_count`（跳定位计数）、`test_conversions`（三转换置位与未置位）、`test_extended_operands`（定位别名、文件数、标志词、报告开关）、`test_bad_operands_rejected`（未知名、空值、十六进制、未知转换、零块长、未知标志词、未知报告格式）。
- **块设备**（`device.rs`，2 个）：`test_slice_reads_blocks`（分块读与越界）、`test_empty_device_misses`（空设备）。
- **卷识别**（`iso.rs`，5 个）：`test_primary_recognised`（卷标块大小）、`test_wrong_type_rejected`（错类型）、`test_bad_identifier_rejected`（错标识）、`test_endian_mismatch_rejected`（端序互斥）、`test_short_sector_rejected`（短扇区）。

尚未覆盖、随后续阶段补齐的：泵字节循环（块设备执行层）、介质控制执行（弹出探测硬件面）、FAT 解析（文件系统阶段）、码表转换（`block` 等五转换的表）。操作数卷块三层是全覆盖的，执行层是显式留白的。

---

## 6. 过渡：搬得动之后，去配网络

本篇走完了搬运：块复制语言、卷识别、块寻址。盘级数据来去自如。

存储四篇（`13` 终端、`14` 挂载、`15` 分区、`16` 镜像）至此闭合：从"说话"到"存物"、从"切分"到"搬运"，数据的物理层全齐。下半场是网络两篇（`18-network-config.md` 的配置诊断、`19-network-services.md` 的服务守护）——请沿因果链继续向下走：先会"搬运实体"，再会"连通远方"。

---

## 7. 参见

- `14-mount-fsck.md`、`15-partition-format.md`——挂载与切分（本篇的前置）
- `18-network-config.md`——网络配置（下一步：连通远方）
- `minix3/bin/dd/args.c:105-121`——操作数表（`CopyPlan` 的逐行对照）
- `minix3/minix/commands/isoread/isoread.c:41`——卷标识（`parse_primary` 的一句话来源）

---

## 附：验证记录（评审用，可跳过）

- `rg -n '"bs"|"conv"|"skip"' args.c` → 105、107、121 行命中。
- `rg -n "CD001" isoread.c` → 第 41 行命中。
- 11 目录存在性：`writeisofs`、`isoread`、`dosread`、`vol`、`eject`、`cdprobe`、`ramdisk`、`loadramdisk`、`rawspeed`、`vnconfig`、`dd`，逐个验证全在。
- `cargo test -p minix-diskimg` → 13 通过、0 失败；`cargo clippy` 无警告。
- 本文档引用的 `file:line` 均来自正文写作前实际执行的 `rg -n` 与 `sed -n` 输出，非凭记忆书写。
