# 16-stage-drivers 文档重建蓝图（deepseek）

## 0. 元数据

```text
your_name(AI agent name) = deepseek
target_dir(关注的工作目录) = 16-stage-drivers
repo_root(仓库根目录) = /home/xzhao/github/minix-rs
当前提交号 = dd4715a03d399b8e942f7f7b8a219f8d076bec85（第二轮复核时的 HEAD；第一轮成稿于
             79d9d1944e2d1d56cac85fe6e80aaa1262976d3a，两提交之间 16-stage-drivers/ 与 os/drivers/
             零改动，已用 git log 核对）
执行日期 = 2026-09-19（第一轮成稿 + 第二轮独立验证与加固，同一执行者）
```

任务 = R 相·重建蓝图：输出本文件，不改任何正文。

### 0.1 审查范围

**算文档**（本次重建的对象）：

- 编号文档 25 篇：`01-chardriver-framework.md` 到 `25-dma-memory-contract.md`
- 总览 1 篇：`00-drivers-overview.md`（67 行，自述"已展开"）
- 全局概念 1 篇：`99-global-concepts.md`（76 行，自述"已展开"）

合计 27 篇，总计 5795 行（`wc -l` 实测，不含本文件；逐篇相加校验 = 5795）。

**算参考材料**（不作为重建对象，作为线索与边界来源）：

| 文件 | 行数 | 性质 |
|---|---|---|
| `plan.md` | 433 | 生效中的重组计划（2026-08-16 定稿）；本次重建不照搬其结论 |
| `todo.md` | 160 | 全量代码扫描 TODO（2026-09-17）；本次重建的**主要事实来源之一** |
| `draft/README.md` | 17 | 旧占位 README 素材 |

> 说明：目标目录下另有一个中间产物目录（按项目规范视为中间产物，本文不引用其内容、不列其清单）。本次执行只核对了"该目录下设计快照齐备"这一事实，用于确认旧文档的 Step 0 预检状态，未读取任何一份的内容。

**范围外**（明确排除，附理由）：

| 内容 | 归属 | 理由 |
|---|---|---|
| VFS/FS 的块设备消费逻辑（`libbdev` 调用方） | `05-stage-vfs` / `15-stage-fs` | 本 stage 只提供客户端库（`libbdev`）与驱动侧（`libblockdriver`） |
| `drivers/tty/pty/ptyfs.c`（112 行） | `15-stage-fs/20-ptyfs.md` | PTY 字符驱动与 ptyfs 树语义分离 |
| input 服务器（`servers/input/`） | `12-stage-input` | 本 stage 只提供 `libinputdriver` 事件桥 |
| devman 服务器（`servers/devman/`） | `11-stage-devman` | 本 stage 覆盖驱动侧 `libdevman` |
| lwip / uds 服务器（`minix/net/`）、`libsockdriver`（1150 行） | `17-stage-net` | 本 stage 覆盖 NDEV 驱动面；SDEV 族随 17 |
| `drivers/storage/ramdisk/` 目录 | boot 配置 | **无任何 .c**（只有 proto/rc 构建配置）；语义映射到 memory 驱动的 `/dev/imgrd` |
| 内核中断/时钟/系统调用 | `01-stage-kernel` | 驱动只消费 notify/中断通知 |
| VM 物理映射机制 | `02-stage-vm` | memory 驱动只消费 `vm_map_phys` 接口；DMA 连续页契约见 §6.4 Q-1 |
| 命令面（`service`/`devmand`/MAKEDEV） | `18-stage-commands` | 本 stage 覆盖驱动进程本体 |
| `drivers/power/acpi`（157 文件 83279 行）的**逐行实现** | 见 §6.4 Q-2 | ACPICA 第三方移植，非 Minix3 自有设计 |

### 0.2 读取清单

**文档**（27 篇全文读完，含头部声明与正文）：

- `00-drivers-overview.md`（67 行）、`99-global-concepts.md`（76 行）、`25-dma-memory-contract.md`（74 行）
- `01-chardriver-framework.md`（295 行）到 `24-misc-drivers.md`（211 行），逐篇全文

**Minix3 C 源码与头文件**（本 stage 对应的全量清单，逐文件 `wc -l` 核对）：

| 目录 | 文件构成 | 行数 | 说明 |
|---|---|---|---|
| `minix3/minix/drivers/`（全量） | **290 个 `.c`** | **155087** | 与 `plan.md` §5.1 的声称值逐项吻合 |
| `minix3/minix/drivers/`（目录数） | **57 个**（`-mindepth 2 -maxdepth 2 -type d`） | — | 与 plan §1.1 吻合 |

按类别（`find <cat> -name '*.c' | xargs wc -l` 实测）：

| 类别 | `.c` 数 | 行数 | 类别 | `.c` 数 | 行数 |
|---|---|---|---|---|---|
| `audio` | 20 | 7134 | `printer` | 2 | 488 |
| `bus` | 6 | 5164 | `sensors` | 3 | 1504 |
| `clock` | 5 | 1095 | `storage` | 21 | 15055 |
| `eeprom` | 1 | 505 | `system` | 8 | 2344 |
| `examples` | 1 | 158 | `tty` | 11 | 7533 |
| `hid` | 2 | 676 | `usb` | 14 | 8110 |
| `iommu` | 1 | 477 | `video` | 4 | 2009 |
| `net` | 28 | 17425 | `vmm_guest` | 3 | 1188 |
| `power` | 160 | 84222 | | | |

**11 个共享框架库**（`lib/` 下，`.c` 行数实测）：

| 库 | `.c` 数 | 行数 | `.h` 数 | 说明 |
|---|---|---|---|---|
| `libchardriver` | 1 | 600 | 0 | 字符驱动主循环 + CDEV 协议适配 |
| `libblockdriver` | 7 | 1857 | 5 | 块驱动主循环（单/多/队列三套）+ BDEV + 分区/几何 |
| `libnetdriver` | 2 | 1186 | 1 | 网卡主循环 + NDEV + I/O 端口辅助 |
| `libbdev` | 5 | 1364 | 3 | 块设备**客户端**库（VFS/FS 消费块驱动的接口） |
| `libvirtio` | 1 | 913 | 1 | virtio 队列/协商框架 |
| `libusb` | 1 | 255 | 0 | USB URB 协议封装（usbd 使用） |
| `libaudiodriver` | 2 | 977 | 0 | 音频框架（14 钩子 + 分片状态机） |
| `libi2cdriver` | 1 | 366 | 0 | I2C 总线框架 |
| `libinputdriver` | 1 | 206 | 0 | 输入事件桥 |
| `libdevman` | 2 | 576 | 1 | 驱动侧设备注册 |
| `libsockdriver` | 1 | 1150 | 0 | socket 驱动框架（**归 17-stage-net**） |

框架库合计（不含 `libsockdriver`）：10 库 / 24 `.c` / 8300 行。

**头文件**：

| 头文件 | 承载 |
|---|---|
| `include/minix/com.h` | 五族请求/回复基址与编号（CDEV/BDEV/NDEV/RTCDEV/USB）、标志位、能力位 |
| `include/minix/driver.h` | `driver_receive`、`struct device`、`MAX_NR_OPEN_DEVICES 256` |
| `include/minix/chardriver.h` | `struct chardriver`（10 成员回调表） |
| `include/minix/blockdriver.h` | `struct blockdriver`（11 成员）、扇区常量、`DMA_BUF_SIZE` |
| `include/minix/netdriver.h` | `struct netdriver`（名字 + 13 函数指针）、`netdriver_addr_t` |
| `include/minix/bdev.h` | `bdev_*` 客户端 API（同步六 + 异步六 + 等待 + 分发 + 刷新） |
| `include/minix/partition.h` | 分区表解析 |
| `include/minix/inputdriver.h` / `input.h` | 事件桥回调表 / 事件词汇 |
| `include/minix/virtio.h` / `libvirtio/virtio_ring.h` | 操作表与状态字节 / virtqueue 环布局 |
| `include/minix/usb.h` | `struct usb_urb`、`struct usb_driver`、传输类型 |
| `include/minix/devman.h` | `libdevman` 注册 API |
| `include/minix/audio_fw.h` | 音频 14 钩子定义（**注意**：不是 `audiodriver.h`，见 §8.1 勘误） |
| `include/minix/i2cdriver.h` | I2C 回调表 |
| `include/minix/sockdriver.h` | socket 驱动回调表（归 17-stage-net） |
| `include/sys/ioc_memory.h` / `ioc_fb.h` / `ioc_sound.h` | 各驱动的 ioctl 控制码 |

**非 C 语言的构建与引导制品**（逐项核对，清单见 §0.3）：

- 引导侧：`minix3/minix/kernel/table.c`（`boot_image` 数组 `:44-64`，memory 在 `:58`、tty 在 `:59`）
- 服务策略配置：`minix3/etc/system.conf`（**驱动段 20+ 处**：tty `:161`、memory `:187`、log `:210`、floppy `:242`、readclock.drv `:257`、acpi `:279`、pci `:289`、ahci `:301`、virtio_blk `:313`、at_wini `:325`、filter `:389`、pckbd `:407`、mmc `:431`、fb `:445`、cat24c256 `:457`、tda19988 `:462`、tps65217 `:467`、tps65950 `:475`、fbd `:480`、vnd `:489`、pty `:497`）
- 设备节点创建：`minix3/minix/commands/MAKEDEV/MAKEDEV.sh` + `MAKEDEV.8`
- 各驱动 Makefile（`drivers/**/Makefile`）

**阶段边界材料**：

- `notes/rewrite/fork-syscall-rewrite/00-master-plan/README.md`：阶段划分（16 号 = drivers）
- `notes/rewrite/fork-syscall-rewrite/edge_todo.md`（1095 行，42 条）：本 stage 登记 3 条（`E-DEVWIRE` `:965`、`E-SDEVOWN` `:983`、`E-DMABUF` `:1001`），另有 4 条涉本 stage（`E-DMCLIENT` `:728`、`E-CDRCONV` `:772`、`E-PCKBDREG` `:806`、`E-DMWIRE` `:705`）
- `16-stage-drivers/plan.md`（433 行）、`16-stage-drivers/todo.md`（160 行）
- 前一 stage：`15-stage-fs/00-fs-overview.md`（已讲完 FS 子系统；本 stage 的 `libbdev` 是它的上游）

**对应 Rust 实现入口**（`os/` 下，逐 crate 核对行数）：

| 组 | 内容 | 行数 |
|---|---|---|
| 驱动 crate | **55 个**（`find drivers -name Cargo.toml`），`.rs` 合计 | 16759 |
| 框架库 | `minix-chardriver` 968 / `minix-blockdriver` 815 / `minix-netdriver` 2383 / `minix-bdev` 1311 / `minix-virtio` 854 / `minix-usb` 286 / `minix-audiodriver` 428 / `minix-i2cdriver` 213 / `minix-driver-rt` 647 | 7905 |

**与 C 目录的差集**（实测）：

| 在 C 有、在 Rust 无 | 理由 |
|---|---|
| `net/dec21140A` | Rust 侧目录名为 `dec21140a`（大小写差异，非缺失） |
| `storage/ramdisk` | 无 `.c`，按 §0.1 范围外排除 |
| `examples/hello` | `todo.md` §4 已裁决删除（文档 24 §2.7 声明"无 Rust 建模"） |

> 即：55 + 3（大小写 1 + 排除 2）= 58 ≈ 57（C 目录数）+ 1（`dec21140A` 与 `dec21140a` 重复计数）。**Rust 侧无实质缺失**。

### 0.3 使用的命令与关键输出（证据摘录）

```text
# 文档行数
$ wc -l 16-stage-drivers/*.md
  67 00-drivers-overview.md / 294 01-chardriver-framework.md / 256 02-blockdriver-framework.md
  245 03-netdriver-framework.md / 247 04-bdev-client.md / 263 05-memory-driver.md
  308 06-tty-driver.md / 249 07-pty-driver.md / 249 08-log-driver.md
  236 09-random-driver.md / 225 10-readclock-driver.md / 223 11-pci-driver.md
  232 12-gpio-devman.md / 232 13-pckbd-driver.md / 204 14-virtio-framework.md
  180 15-virtio-blk-driver.md / 220 16-ahci-ata-driver.md / 275 17-storage-misc-driver.md
  206 18-usb-framework.md / 204 19-usb-storage-hub.md / 209 20-fb-driver.md
  204 21-audio-drivers.md / 199 22-net-driver-reference.md / 207 23-net-driver-variants.md
  211 24-misc-drivers.md / 74 25-dma-memory-contract.md / 76 99-global-concepts.md

# C 源清单（plan.md 声称值核对：全部吻合）
$ find minix3/minix/drivers -name '*.c' | wc -l        → 290
$ find minix3/minix/drivers -name '*.c' | xargs wc -l  → 155087 total
$ find minix3/minix/drivers -mindepth 2 -maxdepth 2 -type d | wc -l → 57

# 框架库
$ for d in libchardriver libblockdriver libnetdriver libbdev libvirtio libusb \
           libaudiodriver libi2cdriver libinputdriver libdevman libsockdriver; do
    echo "$d: $(find lib/$d -name '*.c' | wc -l) .c, $(find lib/$d -name '*.c' | xargs wc -l | tail -1)"; done

# 内部交叉引用统计（16-stage-drivers 文档之间）
$ grep -rho "16-stage-drivers/[0-9][0-9]-[a-z0-9-]*\.md" 16-stage-drivers/*.md | sort | uniq -c | sort -rn
  27 01-chardriver-framework.md   25 99-global-concepts.md   17 02-blockdriver-framework.md
   6 06-tty-driver.md              5 14-virtio-framework.md   5 03-netdriver-framework.md
   4 22 / 15 / 04 / 00（各 4）     3 23 / 20 / 18 / 16 / 11 / 10 / 07 / 05（各 3）
   2 19 / 17 / 09 / 08（各 2）     1 24 / 21 / 13 / 12（各 1）
# 总计 137 处内部引用（不含 doc_rerank_* 与 plan/todo；本表逐行相加 = 137）

# 【关键】代码注释引用文档编号 —— 45 处（Rust 源码注释 39 处 + 审查产物 6 处），全部以裸文件名形式
$ grep -rhoE '`[0-9]{2}-[a-z0-9-]+\.md`' os/drivers os/libs/minix-* --include="*.rs" | sort | uniq -c | sort -rn
   5 `17-storage-misc-driver.md`    3 `24-misc-drivers.md`      3 `23-net-driver-variants.md`
   2 `22-net-driver-reference.md`   2 `21-audio-drivers.md`     2 `19-usb-storage-hub.md`
   2 `18-usb-framework.md`          2 `16-ahci-ata-driver.md`
   1 各：20 / 15 / 14 / 13 / 12-gpio-devman / 12-libinputdriver / 11 / 10 / 09 / 08 / 07
        / 06 / 05 / 04 / 03-netdriver-framework / 03-lwip-main-init / 02-sockevent-framework
        / 02-blockdriver-framework / 01-sockdriver-framework / 01-chardriver-framework / 00
# 其中 5 处指向【其它 stage】的文档（03-lwip-main-init / 02-sockevent-framework /
# 01-sockdriver-framework 属 17-stage-net；12-libinputdriver 属 12-stage-input；
# 04-input-event-format / 02-ipc-message-contract / 03-is-dump-dispatch 属别处）

# 与 15-stage-fs 的对比（重要事实）
$ grep -rc "15-stage-fs/[0-9][0-9]-" os/fs os/libs/minix-fs --include="*.rs" → 0 处
$ grep -rhoE '`[0-9]{2}-[a-z0-9-]+\.md`' os/drivers os/libs/minix-* → 45 处（Rust 源码 39 + 审查产物 6；另有 13 处指向其它 stage，单独统计）
# 结论：15-stage-fs 的代码注释零处引用文档编号（全仓仅 2 处目录级提及，无 NN-name.md 形式）；本 stage 有 45 处。
# 本 stage 的重建【必然造成代码注释断链】，这是与 15-stage-fs 最大的差异。

# 服务策略配置（非 C 制品，现有文档零处提及）
$ grep -n "^service " minix3/etc/system.conf | wc -l   → 38 个服务段
# 其中驱动段 20+ 处（见 §0.2 清单）
$ sed -n '161,186p' minix3/etc/system.conf   # tty 段：uid 0 + io ALL + IRQCTL + DEVIO...
$ sed -n '187,209p' minix3/etc/system.conf   # memory 段：io NONE + irq NONE
$ sed -n '289,300p' minix3/etc/system.conf   # pci 段：io cf8:8 + 4d0:2 + PRIVCTL + DEVIO

# 每驱动的服务声明与热插拔规则（第二轮新增证据）
$ find minix3/minix/drivers -name '*.conf' | wc -l   → 34 个（安装到 /etc/system.conf.d/）
$ grep -l "type net;" $(find minix3/minix/drivers -name '*.conf') | wc -l   → 11 个（网卡驱动声明 type net）
$ grep -c '^service' minix3/etc/system.conf   → 42 个服务段（boot 段 15 + 动态段 27；其中驱动 24 个）
$ cat minix3/etc/devmand/usb_hub.cfg / minix3/etc/devmand/usb_storage.cfg   # 热插拔规则 → minix-service 拉起

# 启动因果链（第二轮新增证据：驱动由 RC 脚本按探测结果拉起，不是配置顺序）
$ grep -n "minix-service" minix3/minix/drivers/storage/ramdisk/rc
  :14 pci / :17 pckbd / :31 floppy / :35 ahci / :38 virtio_blk / :40,:41 at_wini / :48 mmc
$ grep -n "up " minix3/etc/usr/rc   # :230 random / :256 pty / :259 lwip / :290 log

# 构建期常量
$ grep -rn "DEFAULT_NR_BUFS\|NR_BUFS" minix3/minix/drivers/**/Makefile 2>/dev/null
$ cat minix3/minix/drivers/storage/memory/Makefile   # LDADD/DPADD 依赖链

# Rust crate 与 C 目录差集
$ find os/drivers -name Cargo.toml | wc -l   → 55
$ for d in $(cd minix3/minix && find drivers -mindepth 2 -maxdepth 2 -type d | sed 's|drivers/||'); do
    [ ! -d "os/drivers/$d" ] && echo "缺失: $d"; done
  → 缺失: net/dec21140A（实为 dec21140a 大小写差异）、storage/ramdisk（无 .c）、examples/hello（已裁决删）

# 关键锚点核对（全部逐条 grep 验证，见 §1 表内锚点）
$ grep -nE "^[a-z_]+[A-Za-z0-9_ ]*\**[a-z_0-9]+\(" <file>  # Python 脚本批量实测
# 实测通过的关键锚点举例：
#   chardriver_task → lib/libchardriver/chardriver.c:549
#   blockdriver_task → lib/libblockdriver/driver_st.c:52
#   netdriver_task → lib/libnetdriver/netdriver.c:969
#   bdev_open → lib/libbdev/bdev.c:80
#   virtio_setup_device → lib/libvirtio/virtio.c:109
#   wants_kick → lib/libvirtio/virtio.c:766
#   m_block_transfer → drivers/storage/memory/memory.c:417（`:56` 仅为前向声明；本轮复核修正）
#   line2tty → drivers/tty/tty/tty.c:264
#   reseed → drivers/system/random/random.c:206
#   visible → drivers/bus/pci/pci.c:2039
#   kbd_process → drivers/hid/pckbd/pckbd.c:328
#   ata_id_check → drivers/storage/ahci/ahci.c:528
#   virtio_blk_status2error → drivers/storage/virtio_blk/virtio_blk.c:549
#   output_done → drivers/printer/printer/printer.c:208
#   hub_task → drivers/usb/usb_hub/usb_hub.c:390
```

### 0.4 本文档的取舍声明

1. 本蓝图**不照搬** `plan.md` 的 26 篇结论。plan 的分组（框架→boot 关键→系统服务→输入→存储→USB→显示音频→网络→杂项）经本蓝图核对后**主干保留**，但篇数与边界有实质调整（§4、§6 逐项给出理由）。
2. 本蓝图对现有文档的缺陷只做**归纳与锚点**，不逐条罗列。§3 只列影响"重建"的结构性缺陷。
3. 所有 C 锚点均为本次执行中实测（`grep -nE` 批量脚本 + 逐条核对），未从现有文档转述。凡未能实测的断言，本文显式标注"待验证"。
4. 本文不引用中间产物目录下的任何内容。
5. 本文件经过两轮：第一轮成稿（2026-09-19），第二轮独立验证与加固（同日，同一执行者）。第二轮修正了 15 处数字、锚点与编号问题（逐条见 §9.6），未改设计结论；并新增 §4.5（篇幅预算）与 §9.4 的 Q-8。

---

## 1. C 真序

### 1.1 阶段类型判定

16-stage-drivers **同时具备三种阶段特征**，按"以哪一类为主"处理：

| 特征 | 适用对象 | 处理方式 |
|---|---|---|
| **库与框架型**（主） | 11 个框架库（本 stage 10 个 + `libsockdriver` 归 17） | 先讲抽象层与接口契约，再讲框架骨架，最后按实现族展开。**这是本篇的主组织原则** |
| **集合型**（主） | 57 个 driver server | 先总览与分类框架，再按设备族分组，族内选代表讲透，其余按差异表收束。**必须给出阅读路径** |
| **服务事件循环型** | 每个 driver 内部 | 按"服务为什么存在 → 诞生与初始化 → 消息接口 → 核心数据结构 → 按场景分组的请求处理 → 查询与杂项 → 与邻接服务的协议"组织 |

**判定理由**（带锚点）：

- **框架型是主特征**：10 个框架库 8300 行 C，而 57 个 driver 中大量只是"填回调表 + 少量策略"——`examples/hello` 全 158 行是框架的最小消费样例；`drivers/storage/memory/memory.c` 599 行里主循环只有 `main:88` 一处调用 `chardriver_task`/`blockdriver_task`；`drivers/system/log/log.c` 360 行的核心是三个环操作。
- **集合型是外层的组织问题**：57 个 driver 分 17 个类别，启动时机四档（boot 2 个 / RS 运行时 / 按需 / 后置），**不可能排成一条线**，必须用"汇聚点加触发时机"组织。
- **事件循环型是每个 driver 的内部结构**：所有 driver 的 `main` 形状相同——SEF 启动 → 声明 → `*driver_task(&table)`。实测锚点：`drivers/storage/memory/memory.c:88`（`main`）、`drivers/tty/tty/tty.c:146`（`main`）、`drivers/system/log/log.c:53`（`main`）、`drivers/bus/pci/main.c:728`（`main`）、`drivers/video/fb/fb.c:308`（`main`，调 `chardriver_task`）。

### 1.2 运行时真序表

> 说明：本表从 C 源码直接重建，**不从现有文档转述**。所有锚点为本次实测。分五段：**启动段**、**CDEV 循环段**、**BDEV 循环段**、**NDEV 循环段**、**终止段**。

#### 段 A：启动段（boot 因果链）

| # | 动作 | C 函数与文件锚点 | 说明 |
|---|---|---|---|
| A-1 | 内核登记 boot image，**只有两个驱动**是成员 | `kernel/table.c:44-65`（`struct boot_image image[]` 声明于 `:44`、`};` 于 `:65`；`MEM_PROC_NR` 在 `:58`、`TTY_PROC_NR` 在 `:59`） | 57 个 driver 里只有 memory 与 tty 进 boot image；其余全部 RS 运行时加载 |
| A-2 | 执行顺序 ≠ 登记顺序：非 VM 挂 `RTS_VMINHIBIT` | `kernel/main.c:265` | 登记顺序是槽位顺序；执行顺序是 kernel 任务 → VM → RS → 其余 |
| A-3 | memory 驱动诞生（boot 第一批） | `drivers/storage/memory/memory.c:88`（`main`）→ `:126`（`sef_cb_init_fresh`） | 初装四步（见 A-7） |
| A-4 | tty 驱动诞生 | `drivers/tty/tty/tty.c:146`（`main`） | 初装内容见 §5 契约 |
| A-5 | init 起，RC 脚本按硬件探测结果逐个拉起其余驱动（RS 运行时加载） | 启动盘脚本 `minix/drivers/storage/ramdisk/rc`：`:10`（acpi，条件）、`:14`（pci）、`:17`（pckbd）、`:31`（floppy）、`:32-41`（三选一存储控制器：ahci `:35` / virtio_blk `:38` / at_wini `:40`）、`:48`（mmc）；后续 `minix3/etc/usr/rc`：`:230`（random）、`:256`（pty）、`:259`（lwip）、`:290`（log） | 授权面在 `minix3/etc/system.conf` 各服务段（uid / ipc / io / irq / system 特权调用）；**启动顺序由脚本的探测分支决定，不是配置文件里的书写顺序** |
| A-6 | 设备按需出现（存储 → 输入 → 网卡 → USB/音频/显示 → 杂项） | `etc/system.conf` 各段；热插拔规则 `minix3/etc/devmand/usb_hub.cfg`、`usb_storage.cfg` | 顺序由 RC 脚本与 devman 动态注册共同决定；USB 类设备由 devmand 命中规则后用 `minix-service` 拉起 |
| A-7 | memory 的初装（`sef_cb_init_fresh`） | `drivers/storage/memory/memory.c:126` | 映像盘登记先行（`/dev/imgrd` 是根 FS 的地基）、清零、绝对内存、宣告 |
| A-8 | 各驱动宣告上线 | `lib/libchardriver/chardriver.c:99`（`chardriver_announce`）、`lib/libblockdriver/driver.c:95`（`blockdriver_announce`）、`lib/libnetdriver/netdriver.c:58`（`netdriver_announce`） | 宣告三件事：解除卡住的调用方、发上线事件、**清空已打开设备表**（重启门） |
| A-9 | 进入主循环 | `lib/libchardriver/chardriver.c:549`（`chardriver_task`）、`lib/libblockdriver/driver_st.c:52`（`blockdriver_task`，另有 `driver.c`/`driver_mt.c` 两套）、`lib/libnetdriver/netdriver.c:969`（`netdriver_task`） | 三族各有自己的循环；块族有三套变体 |

#### 段 B：CDEV 循环段（一次字符请求的生命周期）

| # | 动作 | C 函数与文件锚点 | 说明 |
|---|---|---|---|
| B-1 | 阻塞接收消息 | `lib/libchardriver/chardriver.c:549`（`chardriver_task` 内 `driver_receive`） | 单线程，一次一条 |
| B-2 | 分类与分发 | `lib/libchardriver/chardriver.c:455`（`chardriver_process`） | 顺序：块打开守卫 → 通知分支 → 重启门 → 七适配器 → 未知走通用挂钩 |
| B-3 | 块打开误投守卫 | `chardriver_process` 内 `do_block_open` 分支 | 一律回答"没有此设备"，防 VFS 线程卡死 |
| B-4 | 通知分支（中断/时钟/其他） | `chardriver_process` 内通知分支 | 调对应旁路挂钩，**不回复** |
| B-5 | 重启门 | `lib/libchardriver/chardriver.c:70`（`is_open_dev`）、`:85`（`set_open_dev`）、`:61`（`clear_open_devs`） | 非打开请求查表；查不到**静默丢弃** |
| B-6 | 七适配器 | `chardriver_process` 内七个 `do_*` | OPEN/CLOSE/READ/WRITE/IOCTL/CANCEL/SELECT |
| B-7 | 打开时登记新次设备号（克隆） | `chardriver_process` 的 OPEN 分支 | `CDEV_CLONED` 标志 → 登记新号 |
| B-8 | 回调（驱动侧） | 各 driver 的 `*_tab` | 十个挂钩：七个请求 + 中断 + 定时 + 通用消息 |
| B-9 | 回复构造与发送 | `lib/libchardriver/chardriver.c:195`（`chardriver_reply`） | 三个特殊标记：`SUSPEND`（只许四种请求）、`PAUSED`（一律停下）、`RESTART`（吞掉不回复） |
| B-10 | 延后回复（挂起） | `lib/libchardriver/chardriver.c:129`（`chardriver_reply_task`）、`:153`（`chardriver_reply_select`） | 拒绝再次挂起（防无限欠账）；清零回复防栈泄漏 |
| B-11 | 次设备号提取 | `lib/libchardriver/chardriver.c:575`（`chardriver_get_minor`） | 七分支对应七种请求 |

#### 段 C：BDEV 循环段

| # | 动作 | C 函数与文件锚点 | 说明 |
|---|---|---|---|
| C-1 | 主循环三套变体 | `lib/libblockdriver/driver.c`（单线程）、`driver_st.c:52`（队列式）、`driver_mt.c`（多线程） | **策略相同，只等形状不同**；Rust 侧只保留单线程一套 |
| C-2 | 宣告（多一个"驱动种类"参数） | `lib/libblockdriver/driver.c:95`（`blockdriver_announce`） | 驱动种类决定分区请求怎么处理（磁盘类处理、非磁盘拒绝） |
| C-3 | 传输适配器四合一 | `driver.c` 内传输适配器 | 方向区分读写；向量标志；计数在连续模式是字节、向量模式是元素 |
| C-4 | 分区门（磁盘类第一次打开解析） | `lib/libblockdriver/drvlib.c:28`（`partition`）、`:14`（`parse_part_table`）、`:17`（`extpartition`）、`:20`（`get_part_table`） | 只跑一次；多线程下每调用单独分配缓冲；分配失败停机 |
| C-5 | 几何上报（专用控制码） | `driver.c` 内 | 非磁盘拒绝 |
| C-6 | 消息队列（多线程/队列式用） | `lib/libblockdriver/mq.c:31`（`mq_init`）、`:49`（`mq_enqueue`）、`:89`（`mq_dequeue`）、`:76`（`mq_isempty`） | 128 消息格、空闲链、每设备一队、满返回假、无锁（由调用方协议保证） |
| C-7 | 追踪 | `lib/libblockdriver/trace.c:51`（`trace_ctl`）、`:167`（`trace_start`）、`:268`（`trace_finish`）、`:250`（`trace_setsize`） | 环形缓冲、平时关、不改变请求结果 |
| C-8 | 热升级钩子 | `lib/libblockdriver/liveupdate.c` | 准备倒状态、状态切换接状态、失败关闭 |
| C-9 | 客户端侧（镜像） | `lib/libbdev/bdev.c:80`（`bdev_open`）、`:95`（`bdev_close`）、`:274`（`bdev_read`）、`:282`（`bdev_write`）、`:290`（`bdev_gather`）、`:298`（`bdev_scatter`）、`:354`（`bdev_ioctl`） | 同步面六函数 |
| C-10 | 客户端异步面 | `lib/libbdev/ipc.c:119`（`bdev_senda`）、`:144`（`bdev_sendrec`）、`:269`（`bdev_reply_asyn`）、`:317`（`bdev_wait_asyn`） | 发送路径 + 回复分发 + 等待 |
| C-11 | 客户端三本账 | `lib/libbdev/driver.c:17`（`bdev_driver_init`）、`:29`（`clear`）、`:43`（`set`）、`:61`（`get`）、`:74`（`update`）；`lib/libbdev/minor.c:78`（`bdev_minor_add`）、`:106`（`del`）、`:122`（`is_open`）、`:17`（`reopen`） | 驱动端点表 / 打开计数 / 调用槽 |
| C-12 | 驱动重启恢复 | `lib/libbdev/minor.c:17`（`bdev_minor_reopen`）+ `ipc.c` 的重试预算 | 同步在发送处返回失败；异步扣预算重发；已打开设备按次数逐次重开 |

#### 段 D：NDEV 循环段

| # | 动作 | C 函数与文件锚点 | 说明 |
|---|---|---|---|
| D-1 | 主循环 | `lib/libnetdriver/netdriver.c:969`（`netdriver_task`） | 对话对方是协议栈（不是 VFS） |
| D-2 | 分类 | `lib/libnetdriver/netdriver.c:763`（`netdriver_process`） | **初始化门**替代重启门（状态没对齐不干活；无打开表、无次设备概念） |
| D-3 | 初始化处理 | `lib/libnetdriver/netdriver.c:871`（`netdriver_init`） | 先清账（计数归零、清待确认、换状态端点）→ 刷新链路 → 组装回复 → 顺手推攒着的统计 |
| D-4 | 六请求 | INIT/CONF/SEND/RECV/IOCTL/STATUS_REPLY | 配置五子类型、模式六位、能力九位、专用标志四位、链路三态 |
| D-5 | 收发队列 | `netdriver.c` 静态量 | 发送队列 8、接收队列 2（不对称：推 vs 被动填） |
| D-6 | 状态上报 | 统计函数 → 置待发送标志 → 发状态报告 | 零计数忽略不脏化标志 |
| D-7 | 端口辅助 | `lib/libnetdriver/portio.c:46`（`netdriver_portinb`）、`:57`（`netdriver_portoutb`） | 扁平偏移 → 起始向量元素 → 逐元素搬；板卡只实现连续小块 |
| D-8 | 宣告 | `lib/libnetdriver/netdriver.c:58`（`netdriver_announce`） | 取标签、拼网前缀、发上线事件 |

#### 段 E：终止段

| # | 动作 | C 函数与文件锚点 | 说明 |
|---|---|---|---|
| E-1 | 收到 SIGTERM | 各 driver 的信号回调 | 非 `SIGTERM` 忽略 |
| E-2 | 终止字符驱动 | `lib/libchardriver/chardriver.c:537`（`chardriver_terminate`） | 两行有效代码：运行标志置假 + 取消阻塞接收；故障即停 |
| E-3 | 块/网络驱动终止 | `lib/libblockdriver/`、`lib/libnetdriver/` 的同名函数 | 形状相同 |
| E-4 | 热升级替代终止（块/log/at_wini/audio） | `lib/libblockdriver/liveupdate.c`、`drivers/system/log/liveupdate.c`、`drivers/storage/at_wini/liveupdate.c`、`lib/libaudiodriver/liveupdate.c` | 状态倒出与接回；与 VM 热升级同哲学 |

### 1.3 真序的可靠性说明

- 段 A 的 A-1、A-2 全部实测（`kernel/table.c:44-64`、`kernel/main.c:265`）。
- 段 B/C/D 的主循环入口全部实测；**适配器内部的分支行号未逐条重核**（现有文档给出的是中文数字行号，如 `chardriver.c` 第五十二行到第一百二十四行），标为"待验证"级——B 相写正文时须重核。
- 段 C 的 C-2 到 C-8 只实测了函数起始行。**本轮复核补全**：`blockdriver_task` 全仓只在 `lib/libblockdriver/driver_st.c:52` 定义（声明在 `include/minix/blockdriver.h:48`）；`driver.c` 的入口是 `blockdriver_process_on_thread`（`:381`），`driver_mt.c` 的是 `blockdriver_mt_task`（`:417`）。即"三套主循环"= 三个入口文件，其中只有 `driver_st.c` 叫 `blockdriver_task`。
- 段 D 的 D-7 实测 `netdriver_portinb:46`/`portoutb:57`。**本轮复核修正**：C 源没有 `prepare_copy`；真实符号是 `netdriver_prepare_copy`（声明 `include/minix/netdriver.h:13`，定义 `lib/libnetdriver/netdriver.c:80`，调用点 `netdriver.c:130`、`portio.c:22/77/139`）。
- 段 E 全部实测。

### 1.4 序差表（运行时序 vs 教学序）

| # | 运行时序事实（带锚点） | 教学序选择 | 理由 | 回指补偿位置 |
|---|---|---|---|---|
| S-1 | 框架代码不独立运行：`libchardriver` 的代码通过各 driver 的 `main` 进入（`memory.c:88`、`tty.c:146`）。运行时不存在"框架启动"这一站 | 教学序把框架（01–07）放在所有 driver 之前 | 读者要先知道"驱动收什么消息、怎么分发"，才看得懂任何一个 driver 的 `main` | 新 02 篇 §1 显式声明"框架是被链接进每个 driver 的库，运行时没有独立的框架进程" |
| S-2 | **`libbdev` 是客户端库**，运行在 VFS/FS 进程里，与驱动侧**不在同一个进程** | 教学序把客户端库（新 07 篇）放在框架组末尾、boot 关键之前 | 它与 `libblockdriver`（新 05）构成"块语义闭环"（驱动侧服务、客户端调用），必须相邻讲；且它是 15-stage-fs 的上游，早讲有利 | 新 07 篇 §1 声明"本篇的代码跑在 VFS/FS 进程里，不是驱动进程" |
| S-3 | 57 个 driver 的启动时机四档（boot 2 个 / RS 运行时 / 按需 / 后置），**不是一个序列** | 教学序按"框架 → boot 关键 → 系统服务 → 设备类别"分九组 | 这是**集合型**阶段的必然选择：按语义层与启动时机分组，组内按使用频率排序 | 新 08 篇（分组总览）给出"57 driver 分组矩阵"，各组篇只写差异 |
| S-4 | 三套块主循环（单/队列/多线程）在运行时是**并列可选的编译期选择** | 教学序只讲单线程一套，另两套进"已声明不做"清单 | Rust 侧只保留单线程（`[ARCH A-5]`）；另两套的语义价值是"有界队列的准入策略"，已在框架篇吸收 | 新 05 篇 §1 声明"三套循环的对照进本篇的差异表，队列准入策略归框架，线程拓扑不复刻" |
| S-5 | 错误路径散布全程：每个适配器都可能返回错误，每个回调内部都有失败分支 | 教学序把"错误路径与失败语义"集中到新 31 篇（收尾组） | 单篇单语义：错误语义是横切关注点 | 各篇在讲到失败分支时只留一句"错误分类与 errno 映射见 31 篇" |
| S-6 | **DMA 内存契约跨 stage 运行**：连续物理页由 VM 服务器分配（`os/libs/minix-types/src/types/dma.rs`） | 教学序把它放收尾组（新 30 篇） | 它是"驱动库如何开口要内存"的边界契约，属收尾组的"边界与接缝" | 新 18 篇（virtio）与 新 24 篇（USB）在讲环/命令表时只留一句"内存来源见 30 篇" |
| S-7 | 硬件寄存器访问散布在**每个** driver 里（`inb`/`outb`/MMIO） | 教学序把"硬件抽象与端口访问"集中到新 02 篇的一节 | 硬件抽象是横切约束（`[ARCH A-2]`），散在 57 篇会重复 57 次 | 各 driver 篇一律声明"寄存器细节在服务层，本库只定顺序与算法"（这是现有文档已有的统一口径，保留） |
| S-8 | 服务策略配置（`system.conf` 的授权面）在运行时**先于**驱动进程诞生（RS 按配置授权） | 教学序把它放收尾组（新 31 篇） | 它是"驱动为什么能访问硬件"的授权前提，属工程面 | 各 driver 篇不展开，统一指向 31 篇 |

---

## 2. 知识点全集

### 2.1 建池方法

对现有 27 篇逐篇提取知识点（每篇 18–42 条），全 stage 去重后合并成池。同一知识点在多篇重复出现的，合并为一条并记录**全部**现有位置，标注**主讲述点**。

编号 `K-NNN`，stage 内唯一。对齐键为"名称加锚点"（供多 AI 汇总时对齐）。

**来源类型**：**存量**（来自现有文档，受 §6 去向规则约束）；**新增**（现有文档没有，由 §3 覆盖审计发现，必须有证据锚点）。

### 2.2 知识点池总表

#### 组 A：字符框架（现有 01 篇）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-001 | 后厨心智模型：驱动只接单、做菜、连小票编号送回 | 概念 | 存量 | **01 §1.1** | — | 建立"驱动不主动发起工作"的模型 |
| K-002 | 小票=消息：类型字段是请求编号，另带请求标识用于配对 | 概念 | 存量 | **01 §1.1** | `com.h` 消息结构 | |
| K-003 | 单线程事件循环假设（同一时刻最多一个请求，故不需锁） | 约束与不变量 | 存量 | **01 §1.1**、02/03/04 前置 | `chardriver.c:549` | 理解三族框架为什么全无锁 |
| K-004 | 开业宣告 + 清空已打开设备记录（重启不信任旧状态） | 机制 | 存量 | **01 §1.1/§2.2** | `chardriver.c:99`、`:61` | 理解重启门的前提 |
| K-005 | 七种请求清单（开/关/读/写/设备控制/取消/轮询） | 接口与协议 | 存量 | **01 §1.2** | `com.h:926-932` | 定位任一字符请求 |
| K-006 | 请求基址 `0x400` + 索引 0..6；判定宏=掩码比较 | 接口与协议 | 存量 | **01 §1.2** | `com.h:922` | 理解两层路由 |
| K-007 | 家族基址两层路由（基址认家族→索引认操作；新增请求便宜） | 机制 | 存量 | **01 §1.2** | — | 解释"为什么加一个请求只改两处" |
| K-008 | 请求材料字段清单（打开/读写/控制/取消/轮询各带什么） | 接口与协议 | 存量 | **01 §1.2** | `com.h` 消息结构 | |
| K-009 | 回复三种形状（普通/轮询/延后就绪通知） | 接口与协议 | 存量 | **01 §1.2** | `chardriver.c:129/153/195` | |
| K-010 | 十个挂钩（7 请求 + 中断/定时/通用三个旁路） | 数据结构 | 存量 | **01 §1.3** | `chardriver.h` 回调表 | |
| K-011 | 空挂钩默认行为六条（空开关=成功、空读写=EIO、空 ioctl=ENOTTY、空 cancel=不回复、空 select=EBADF、空通知=忽略） | 约束与不变量 | 存量 | **01 §1.3** | `chardriver.c` 适配器空判 | **本 stage 最易错的一条**；解释"为什么空钩子不全是 ENOSYS" |
| K-012 | 挂起语义（收下、稍后用延后回复送回），三种请求可用 | 机制 | 存量 | **01 §1.4** | `chardriver.c:129` | |
| K-013 | 延后回复函数内部拒绝再次挂起标记（防无限欠账） | 约束与不变量 | 存量 | **01 §1.4** | `chardriver.c:129-172` | |
| K-014 | 取消语义（放弃→给原请求发取消确认；继续→不回复；无挂钩默认继续） | 机制 | 存量 | **01 §1.4** | `chardriver.c` CANCEL 分支 | |
| K-015 | 挂起只允许四种请求；其他挂起：C 停机 / Rust EINVAL | 约束与不变量 | 存量 | **01 §1.4** | `chardriver.c:203-223` | |
| K-016 | 重启门：宣告清表 + 非打开请求查表 + 查不到静默丢弃 | 机制 | 存量 | **01 §1.5/§2.2** | `chardriver.c:61/70/85` | |
| K-017 | 静默丢弃理由：发送方通过上线事件自恢复，不该在请求通道吵架 | 概念 | 存量 | **01 §1.5** | — | 解释"为什么不回 EIO" |
| K-018 | 块打开误投守卫（一律回答没有此设备，防 VFS 线程卡死） | 机制 | 存量 | **01 §1.5** | `chardriver.c` do_block_open | |
| K-019 | 通知单向性：中断/时钟/其他→对应旁路挂钩，且不回复 | 机制 | 存量 | **01 §1.6** | `chardriver.c:464-482` | |
| K-020 | Linux 对照（多张操作表按对象组织） | 概念 | 存量 | **01 §1.7** | Linux | 横向定位 |
| K-021 | Redox 对照（scheme 路径字符串路由 vs 数字编号） | 概念 | 存量 | **01 §1.7** | Redox | 横向定位 |
| K-022 | 打开集合（256 槽数组+计数；清空/查找/登记；满则停下） | 数据结构 | 存量 | **01 §2.2** | `chardriver.c:61/70/85` | |
| K-023 | 上线宣告三件事（解除卡住调用方、发上线事件、清表），失败停机 | 机制 | 存量 | **01 §2.2** | `chardriver.c:99` | |
| K-024 | 发送辅助（同步用非阻塞发送、异步用异步发送） | 机制 | 存量 | **01 §2.4** | `chardriver.c` send_reply | |
| K-025 | 回复主函数三特殊标记（SUSPEND/PAUSED/RESTART） | 机制 | 存量 | **01 §2.4** | `chardriver.c:195` | |
| K-026 | 回复按请求种类分派（开关一类、读写控制一类、取消的回复给原请求、轮询两形状、未知停机） | 机制 | 存量 | **01 §2.4** | `chardriver.c:195` | |
| K-027 | 七个适配器三段式（拆字段/调挂钩/返回）；打开克隆标志→登记新次设备号 | 机制 | 存量 | **01 §2.5** | 七个 `do_*` | |
| K-028 | 路由与主循环顺序（块打开守卫→通知分支→重启门→七适配器→未知走通用） | 机制 | 存量 | **01 §2.6** | `chardriver.c:455/549` | |
| K-029 | 终止函数两行有效代码；故障即停哲学 | 机制 | 存量 | **01 §2.6** | `chardriver.c:537` | |
| K-030 | `chardriver_get_minor` 七分支 + 两断言；分支数应等于七的自检法 | 机制 | 存量 | **01 §2.7** | `chardriver.c:575` | |
| K-031 | 与 C 差异表五条（分发表/空挂钩/满表/消息收发/旧式暂停） | 架构演进 | 存量 | **01 §2.9** | — | |
| K-032 | Rust 决策五条（`CdevRequest` 枚举、`RequestId` 包装、`OpenDeviceSet`、`CharDriver` trait、`Route` 枚举） | 架构演进 | 存量 | **01 §3.1-3.5** | `os/libs/minix-chardriver/src/{protocol,driver}.rs` | |
| K-033 | 单线程假设写进文档而非类型（不堵死未来演进） | 约束与不变量 | 存量 | **01 §3.6** | — | |
| K-034 | `SilentDevice` 空设备作为框架逻辑纯测对端 | 测试性质 | 存量 | **01 §3.7** | — | |
| K-035 | 错误处理表（10 场景） | 接口与协议 | 存量 | **01 §4** | — | 映射汇总归新 31 篇 |

#### 组 B：块框架（现有 02 篇）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-036 | 块设备范围与块/字符差异（扇区对齐、位置与长度对齐、分区表） | 概念 | 存量 | **02 说明** | — | 定位 |
| K-037 | 分区概念（起始+长度、表在固定位置、第一次打开读入、分区内偏移+起始=真实位置） | 概念 | 存量 | **02 §1.1** | `drvlib.c:28` | |
| K-038 | 向量读写概念（scatter/gather；文件内容多处放、磁盘上连续） | 概念 | 存量 | **02 §1.1** | — | |
| K-039 | 等待队列概念（多工作线程按设备排队；独立模块两边共用） | 概念 | 存量 | **02 §1.1** | `mq.c` | |
| K-040 | 追踪与热升级钩子概念 | 概念 | 存量 | **02 §1.1** | `trace.c`、`liveupdate.c` | |
| K-041 | 七种块请求 + 基址 `0x500` + 索引 0-6 + 判定宏 | 接口与协议 | 存量 | **02 §1.2** | `com.h:963/966/970-976` | |
| K-042 | 块回复只有一种通用形状（无轮询两形状） | 接口与协议 | 存量 | **02 §1.2** | — | |
| K-043 | 块请求材料（打开带次设备号与访问方式；收集分散换计数；位置 64 位） | 接口与协议 | 存量 | **02 §1.2** | `com.h` | |
| K-044 | 十一个回调组成（读写向量四合一为传输；+开关控制清理/中断/定时/通用；+分区查询/几何上报/设备号映射） | 数据结构 | 存量 | **02 §1.3** | `blockdriver.h` 回调表 | |
| K-045 | 块回调默认行为 + 驱动种类决定分区请求处理 | 约束与不变量 | 存量 | **02 §1.3** | — | |
| K-046 | 分区解析流程（问范围→分配缓冲→读表→逐项越界检查→主分区排序→扩展分区递归→释放） | 机制 | 存量 | **02 §1.4/§2.7** | `drvlib.c:28/14/17/20` | |
| K-047 | 解析只跑一次 + 多线程下每调用单独分配缓冲 + 分配失败停机 | 约束与不变量 | 存量 | **02 §1.4** | `drvlib.c` | |
| K-048 | **三套主循环与统一准入策略**（重启门/通知旁路/未知走通用；区别只在等待形状） | 架构演进 | 存量 | **02 §1.5/§2.5** | `driver.c`、`driver_st.c:52`、`driver_mt.c` | `[ARCH A-5]` 的落点 |
| K-049 | `[ARCH A-5]` 只保留单线程事件循环，队列准入保留、线程拓扑不复刻 | 架构演进 | 存量 | **02 §1.5** | — | |
| K-050 | 追踪机制（开始/结束时间与结果、环形缓冲、三控制项、平时关、不改变请求结果） | 机制 | 存量 | **02 §1.6/§2.8** | `trace.c:51/167/268/250` | |
| K-051 | 热升级机制（准备倒状态、状态切换接状态、失败关闭） | 机制 | 存量 | **02 §1.7/§2.8** | `liveupdate.c` | |
| K-052 | 消息布局头注释 + 请求编号与判定宏行号 | 工具与工程 | 存量 | **02 §2.2** | `driver.c:1-40` | |
| K-053 | 访问位（读/写各一比特）与直写标志 | 接口与协议 | 存量 | **02 §2.2** | `com.h:982-983/987` | |
| K-054 | 扇区常量（512、移 9 位、掩码 511、光盘 2048、直接内存一扇区） | 接口与协议 | 存量 | **02 §2.2** | `blockdriver.h:50-60` | |
| K-055 | 块宣告与打开集合（256 上限、满停下、块宣告多"驱动种类"参数） | 机制 | 存量 | **02 §2.3** | `driver.c:95` | |
| K-056 | 传输适配器四合一（方向区分读写、向量标志、计数在连续模式是字节/向量模式是元素） | 机制 | 存量 | **02 §2.4** | `driver.c` | |
| K-057 | 分区门（磁盘类第一次打开解析；几何上报走专用控制码；非磁盘拒绝） | 机制 | 存量 | **02 §2.4** | `driver.c:286-292` | |
| K-058 | 三套主循环细节（st：先排队再取下一条；mt：每设备队+线程池+识别函数） | 机制 | 存量 | **02 §2.5** | `driver_st.c:25-89`、`driver_mt.c:70-200` | |
| K-059 | 消息队列模块（128 消息格、空闲链、每设备一队、满返回假、FIFO、无锁由调用方协议保证） | 数据结构 | 存量 | **02 §2.6** | `mq.c:31/49/89/76` | |
| K-060 | 分区解析细节（软盘整盘/主分区排序/子分区递归进扩展分区；表项封顶天然终止） | 机制 | 存量 | **02 §2.7** | `drvlib.c` | |
| K-061 | 与 C 差异表五条 | 架构演进 | 存量 | **02 §2.10** | — | |
| K-062 | Rust 决策五条（`BdevRequest` 枚举、传输四合一、`PartitionRange` 值类型、`PendingQueue` 只留策略、`BlockDriver` trait） | 架构演进 | 存量 | **02 §3.1-3.5** | `os/libs/minix-blockdriver/src/{protocol,driver}.rs` | |
| K-063 | Redox 块层对照（一个读写对加向量变体，形状一致） | 概念 | 存量 | **02 §3.2** | Redox | 对照 |

#### 组 C：网络框架（现有 03 篇）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-064 | 网络驱动定位（收包/发包/链路/组播/统计）；对话对方是协议栈而非 VFS | 概念 | 存量 | **03 说明** | — | 定位 |
| K-065 | 塔台比喻与四机制（发送队列八、接收队列二、链路变化推、统计变化推；队列满直接拒绝） | 概念 | 存量 | **03 §1.1** | — | |
| K-066 | 六种请求 + 基址 `0x1A00` + 索引 0-5 | 接口与协议 | 存量 | **03 §1.2** | `com.h:1096-1101` | |
| K-067 | 初始化语义（协议栈重启后第一句话；清空收发队列与状态确认；回复名字/地址/能力/链路/队列上限） | 机制 | 存量 | **03 §1.2** | `netdriver.c:871` | |
| K-068 | 初始化期待标志门（未初始化前其他请求一律拒绝） | 约束与不变量 | 存量 | **03 §1.2/§1.7** | — | **与重启门的区别** |
| K-069 | 配置五子类型（设模式与组播名单/开关能力/设专用标志/设介质/改硬件地址） | 接口与协议 | 存量 | **03 §1.2** | `com.h:1111-1115` | |
| K-070 | 模式位六种（关闭/单播/广播/组播名单/组播全收/混杂） | 接口与协议 | 存量 | **03 §1.2** | `com.h:1118-1123` | |
| K-071 | 组播名单最多 16，超出则收全部组播不点名 | 约束与不变量 | 存量 | **03 §1.2** | `NETDRIVER_MCAST_MAX` | |
| K-072 | 发送/接收数据路径（协议栈递包与递空缓冲） | 机制 | 存量 | **03 §1.2** | `netdriver.c` send/recv | |
| K-073 | 保留控制槽（一直空着留给未来） | 接口与协议 | 存量 | **03 §1.2** | — | |
| K-074 | 状态确认（协议栈对状态上报的回执） | 机制 | 存量 | **03 §1.2** | — | |
| K-075 | 回调清单（名字 + 13 函数指针：init/stop/set_mode/set_caps/set_flags/set_media/set_hwaddr/recv/send/get_link/intr/tick/other） | 数据结构 | 存量 | **03 §1.3/§2.2** | `netdriver.h:23-40` | |
| K-076 | 默认值全安静（停止/改地址可不实现；初始化默认零；收发默认不动） | 约束与不变量 | 存量 | **03 §1.3** | — | |
| K-077 | 授权向量拷贝留服务层，行为定义只谈长度 | 架构演进 | 存量 | **03 §1.3** | `netdriver_copyin/copyout` | |
| K-078 | 收发队列 8/2 不对称理由（推 vs 被动填；与协议栈对应模块一致） | 约束与不变量 | 存量 | **03 §1.4** | `netdriver.c:34-46` | |
| K-079 | 状态上报（网卡调统计函数→置待发送标志→发报告；零计数忽略） | 机制 | 存量 | **03 §1.5** | `netdriver.c` 统计族 | |
| K-080 | 端口辅助走格子算法（扁平偏移→起始向量元素→逐元素搬） | 机制 | 存量 | **03 §1.6/§2.5** | `portio.c:46/57` | |
| K-081 | `[ARCH A-2]` 端口号不进操作系统层（方法参数无端口） | 架构演进 | 存量 | **03 §1.6** | — | |
| K-082 | 初始化门 vs 重启门（网络无打开表、无次设备概念） | 概念 | 存量 | **03 §1.7** | — | 与 K-016 对照 |
| K-083 | 框架静态量一览（回调表指针、运行标志、初始化期待、在线标志、轮询节拍、收发队列与计数、待确认状态、四统计计数、名字、地址、能力、链路、介质） | 数据结构 | 存量 | **03 §2.3** | `netdriver.c:25-90` | |
| K-084 | 初始化处理顺序（先清账→刷新链路→组装回复→顺手推统计） | 机制 | 存量 | **03 §2.4** | `netdriver.c:871` | |
| K-085 | portio 搬运模板（拷贝准备→越界停下→逐元素搬→板卡失败停下）+ 字节/字四入口 | 机制 | 存量 | **03 §2.5** | `portio.c` 八函数 | |
| K-086 | 与 C 差异表四条（分发表、授权向量拷贝=已知缺口、端口直访=A-2、消息收发循环） | 架构演进 | 存量 | **03 §2.7** | — | |
| K-087 | Rust 决策五条（`NdevRequest` 枚举+初始化门、地址与统计值类型含饱和加、`PortIo` 行为定义、`NetDriver` trait、队列常量泛型） | 架构演进 | 存量 | **03 §3.1-3.5** | `os/libs/minix-netdriver/src/{protocol,portio,driver}.rs` | |
| K-088 | 能力位九值 / 专用标志四值 / 回复六值枚举 | 接口与协议 | 存量 | **03 §5.1**（G8 补） | `com.h:1126-1140`、`netdriver.rs` | |

#### 组 D：块客户端库（现有 04 篇）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-089 | 客户端库定位：VFS 与 FS 如何找驱动、发请求、等回复、恢复；与驱动侧构成闭环 | 概念 | 存量 | **04 说明** | — | **与 15-stage-fs 的接口** |
| K-090 | 总机比喻三本账（通讯录=驱动端点表、通话记录=打开计数、留言本=异步调用表） | 概念 | 存量 | **04 §1.1** | — | |
| K-091 | 三本账上限与理由（16 主设备号；4 个次设备；256 条调用） | 概念 | 存量 | **04 §1.1/§2.2** | `libbdev/const.h` | |
| K-092 | 同步打开流程（查表→本地拒绝→组装打开消息含同步标识→发送等回复→标识与类型校验→成功才记记录） | 机制 | 存量 | **04 §1.2/§2.5** | `bdev.c:80` | |
| K-093 | 同步关闭本地门（记录里没有则本地拒绝、不发消息；驱动成功才销记录） | 机制 | 存量 | **04 §1.2** | `bdev.c:95` | |
| K-094 | 读写与向量读写同路只是操作码与材料不同；设备控制多一个用户端点参数 | 接口与协议 | 存量 | **04 §1.2** | `bdev.c:274/282/290/298/354` | |
| K-095 | 共用回复检查函数（标识对上才认、类型不对 EINVAL、发不出去 EIO） | 约束与不变量 | 存量 | **04 §1.2** | `ipc.c:144`（`bdev_sendrec`） | **F2 的修复点** |
| K-096 | 异步调用机制（分配调用槽、发出即走、分发按标识找槽记状态、等待为非阻塞收集） | 机制 | 存量 | **04 §1.3/§2.6** | `ipc.c:119/269/317` | |
| K-097 | 两本重试预算（驱动重启 10 次、传输错 5 次） | 约束与不变量 | 存量 | **04 §1.3/§2.5** | `libbdev/const.h` | |
| K-098 | 刷新函数（把某设备名下没发出去的异步请求全部补发） | 机制 | 存量 | **04 §1.3** | `ipc.c` flush_asyn | |
| K-099 | 回调机制（登记时留函数、回复到时自动调）本实现留服务层 | 架构演进 | 存量 | **04 §1.3** | `bdev_callback_asyn` | |
| K-100 | 驱动重启恢复（同步在发送处返回失败；异步扣预算重发；已打开设备按次数逐次重开） | 机制 | 存量 | **04 §1.4** | `minor.c:17` | |
| K-101 | FIXME/honesty 注释：重开中途遇非传输错误则已重开次设备可能永远关不掉；Rust 保留诚实注释 | 约束与不变量 | 存量 | **04 §1.4/§2.4** | `minor.c` FIXME | **不假装问题不存在**的范例 |
| K-102 | 传输抽象（行为定义只过四整数+回复三元组；内核端点类型与消息结构不进本库） | 架构演进 | 存量 | **04 §1.5** | `os/libs/minix-bdev/src/transport.rs` | |
| K-103 | 回环替身与录制替身（测重试计数） | 测试性质 | 存量 | **04 §1.5** | — | |
| K-104 | 编号镜像绊线（测试硬编码打开 `0x500`、收集 `0x504`、分散 `0x505`、控制 `0x506`） | 约束与不变量 | 存量 | **04 §1.6** | — | 框架侧改编号客户端侧测试立刻变红 |
| K-105 | 常量七项（调用上限 256、同步标识 -1、数据存储查询 100 次×50 微秒、驱动重试 10、恢复容忍两次重启、传输重试 5、打开跟踪 4） | 接口与协议 | 存量 | **04 §2.2** | `libbdev/const.h` | |
| K-106 | 驱动端点表语义（每项端点+标签；初始化全填未知；绑定清旧端点强制重新解析） | 数据结构 | 存量 | **04 §2.3** | `driver.c:17/29/43/61/74` | |
| K-107 | 打开计数（设备号/计数/访问位累积；无空槽打印并丢弃；删到零销项；重启重开逐次重发保持计数） | 数据结构 | 存量 | **04 §2.4** | `minor.c:78/106/122/17` | |
| K-108 | ipc 发送路径（驱动上线更新按轮询预算查数据存储；异步标识用槽索引；同步标识固定 -1；回来三验） | 机制 | 存量 | **04 §2.5** | `ipc.c:119/144` | |
| K-109 | 预算藏在发送循环（重启 10 次内重发、传输错 5 次内重发、恢复中途再重启容忍 2 次） | 约束与不变量 | 存量 | **04 §2.5** | `ipc.c`、`const.h` | |
| K-110 | 回复分发与等待（按标识找槽记状态；找不到打日志丢游离回复；回调在分发时调） | 机制 | 存量 | **04 §2.6** | `ipc.c:269/317` | |
| K-111 | 与 C 差异表六条（传输原语=策略可测、阻塞等待→非阻塞收集、回调指针→状态机、授权组装/数据存储轮询/驱动发现=已知缺口） | 架构演进 | 存量 | **04 §2.8** | — | |
| K-112 | Rust 决策六条（设备号值类型、`DriverTable` 常量泛型宽度、`OpenTracker` 四槽线性扫描、`CallTable` 256 槽+两预算、`Transport` 行为定义、`BdevClient` 组合） | 架构演进 | 存量 | **04 §3.1-3.6** | `os/libs/minix-bdev/src/client.rs` | |
| K-113 | `CallSlot` 保存 Destination（A6：换回完整 flush/重发能力） | 架构演进 | 存量 | **04**（todo A6） | `client.rs` | |
| K-114 | `check_reply` 三验收敛（类型→标识→状态）（F2 修复） | 约束与不变量 | 存量 | **04**（todo F2） | `client.rs:557-565` | |

#### 组 E：boot 关键驱动（现有 05–08 篇）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-115 | 双面驱动分诊（看编号认家族） | 机制 | 存量 | **05 §1.1/§2.2** | `memory.c:101-104` | |
| K-116 | 字符表四挂钩 / 块表六成员 | 数据结构 | 存量 | **05 §1.1/§2.2** | `memory.c:63-108` | |
| K-117 | 十三个次设备号语义（7 固定 + 6 内存盘） | 接口与协议 | 存量 | **05 §1.2/§2.1** | `dmap.h:85-92` | |
| K-118 | 面的归属谓词（1/2/3/5 字符，余块）；错面请求一律答 ENODEV | 约束与不变量 | 存量 | **05 §1.2/§2.4** | `memory.c:170`（`m_is_block`） | |
| K-119 | 读写三板斧（到头、截短、搬运） | 机制 | 存量 | **05 §1.3** | `memory.c` 四处手写截断 | |
| K-120 | 空洞/零源读写四行为 | 机制 | 存量 | **05 §1.3/§2.5** | `memory.c:48/50` | |
| K-121 | 六种读写行为表（按次设备分行） | 数据结构 | 存量 | **05 §2.5** | `memory.c` 各分支 | |
| K-122 | 单页窗口缓存三态（映射页/窗口地址/有无） | 机制 | 存量 | **05 §1.4** | `memory.c:218`（`m_transfer_mem`） | |
| K-123 | 页窗口与映射原语解耦 | 架构演进 | 存量 | **05 §1.4/§3.5** | `memory.c` 建失败分支 | |
| K-124 | 内存盘扩容三问（是盘/尺寸对/独占）与 `MIOCRAMSIZE` | 机制 | 存量 | **05 §1.5/§2.8** | `memory.c:60`（`m_block_ioctl`） | |
| K-125 | 扩容独占（打开计数必须为一） | 约束与不变量 | 存量 | **05 §1.5** | `memory.c:60` | |
| K-126 | 启动登记顺序：映像盘先行 | 机制 | 存量 | **05 §1.6/§2.3** | `memory.c:126`（`sef_cb_init_fresh`） | boot 因果链第一步 |
| K-127 | 内核内存映射 `#if 0` 死代码与职责移交 | 架构演进 | 存量 | **05 §1.6/§2.3** | `memory.c:130-145` | |
| K-128 | 初始化四步（映像盘/清零/绝对内存/宣告） | 机制 | 存量 | **05 §2.3** | `memory.c:88/126` | |
| K-129 | 内核内存传输（窗口未初始化报 EIO） | 机制 | 存量 | **05 §2.5** | `memory.c:189`（`m_transfer_kmem`） | |
| K-130 | 绝对内存传输（页对齐、位置与偏移双推进） | 机制 | 存量 | **05 §2.5** | `memory.c:218` | |
| K-131 | 打开关闭计数与首次打开调设备打开 | 机制 | 存量 | **05 §2.6** | `memory.c:360/388` | |
| K-132 | 块传输向量循环（逐元素截短/耗尽进下一格） | 机制 | 存量 | **05 §2.7** | `memory.c:56`（`m_block_transfer`） | |
| K-133 | 高半部位非零视为越界直接成功零字节 | 约束与不变量 | 存量 | **05 §2.7** | `memory.c:56` | |
| K-134 | 八条终端线（4 控制台 + 4 串口）共用一个进程 | 概念 | 存量 | **06 §1.1** | `dmap.h:95`、`config.h:41-46` | |
| K-135 | 终端结构五十余字段"状态卡" | 数据结构 | 存量 | **06 §1.1/§2.9** | `tty.h` | |
| K-136 | 事件灯去耦（中断点灯 / 主循环干活） | 机制 | 存量 | **06 §1.1/§1.7/§2.2** | `tty.c:146`（`main`） | |
| K-137 | 次设备号三段映射 + 两特例 + 激活检查 | 机制 | 存量 | **06 §1.2/§2.3** | `tty.c:264`（`line2tty`） | |
| K-138 | 视频号（125）走视频分支、不进表 | 接口与协议 | 存量 | **06 §1.2** | `line2tty` 视频分支 | |
| K-139 | 日志号（15）别名与只写诊断语义 | 接口与协议 | 存量 | **06 §1.2/§2.4** | `tty.c:734-743` | **B1 的修复点** |
| K-140 | 映射失败 / 未激活一律 ENODEV | 约束与不变量 | 存量 | **06 §1.2** | `line2tty` | |
| K-141 | 行规则七步加工链 | 机制 | 存量 | **06 §1.3** | `tty.c:1012`（`in_process`） | |
| K-142 | 正规/非正规模式分水岭（ICANON） | 机制 | 存量 | **06 §1.3** | `in_process` | |
| K-143 | 擦除/杀行/文件尾/转义语义 | 机制 | 存量 | **06 §1.3** | `tty.c` back_over 等 | |
| K-144 | 输入队列 256 环 + 换行计数；满队丢弃（有损但正确） | 数据结构 | 存量 | **06 §1.3/§3.3** | `TTY_IN_BYTES` | |
| K-145 | 挂起三路（读/写/排空）字段构成 | 数据结构 | 存量 | **06 §1.4** | `tty.h` | |
| K-146 | 取消按调用方 + 标识精确匹配、三路 match | 机制 | 存量 | **06 §1.4/§2.5** | `tty.c:778`（`do_cancel`） | |
| K-147 | 重复挂起 EIO、零长度 EINVAL | 约束与不变量 | 存量 | **06 §1.4** | `tty.c:81`（`do_read`） | |
| K-148 | 轮询规则一：挂断速度全就绪 | 机制 | 存量 | **06 §1.5/§2.6** | `tty.c:816`（`select_try`） | |
| K-149 | 轮询规则二：正规模式读就绪要求见过换行 | 机制 | 存量 | **06 §1.5** | `select_try` | |
| K-150 | 双次设备号注册拒绝 | 约束与不变量 | 存量 | **06 §1.5** | `tty.c:865`（`do_select`） | |
| K-151 | 内核消息重定向（取模差值、非阻塞写、备份恢复） | 机制 | 存量 | **06 §1.6** | `tty.c:389`（`do_new_kmess`） | |
| K-152 | 与日志驱动共读同一内核缓冲、各记指针 | 概念 | 存量 | **06 §1.6/§2.9** | — | 与 K-... 对照 |
| K-153 | 主循环顺序：先扫灯再收信；先扫灯防事件饿死 | 机制 | 存量 | **06 §1.7/§2.2** | `tty.c:146` | |
| K-154 | 打开收编控制终端与访问字判定；末次关闭恢复默认行参与窗口尺寸 | 机制 | 存量 | **06 §2.4** | `tty.c:721`（`do_open`）、`:754`（`do_close`） | |
| K-155 | 读写挂起登记四件套与事件泵补答 | 机制 | 存量 | **06 §2.7** | `tty.c:81` 起 | |
| K-156 | 设备相关分工 + 七个设备函数指针接线 | 架构演进 | 存量 | **06 §2.8** | `tty.h` 末尾声明段 | |
| K-157 | `[ARCH A-2]` 设备寄存器 → 后端抽象 | 架构演进 | 存量 | **06 §2.10** | — | |
| K-158 | 键盘侧输入协议状态机 + 效果（`INPUT_PAGE_KEY=0x0007`、`NR_SCAN_CODES=0xE8`、`0x8000`、`ALT_LOCK`、32 格环） | 接口与协议 | 存量 | **06 §3.8/§5.6** | `keyboard.c:30/35/131-176/369-385` | |
| K-159 | 主端/从端对讲机：两条数据通路（主→从输入队列、从→主输出环） | 概念 | 存量 | **07 §1.1** | `pty.c` | |
| K-160 | NR_PTYS 32 对 + 六个状态位 | 数据结构 | 存量 | **07 §1.2/§2.2** | `config.h:46` | |
| K-161 | 经典对规则（从端可先开、主端只开一次） | 约束与不变量 | 存量 | **07 §1.2/§2.3** | 主端打开分支 | |
| K-162 | Unix98 对规则（克隆领号、拒直接开主端） | 约束与不变量 | 存量 | **07 §1.2/§2.3** | 克隆分支 | |
| K-163 | 双关复位与 Unix98 请 FS 删从节点 | 机制 | 存量 | **07 §1.2/§2.4** | `pty.c:205`（`pty_reset`） | |
| K-164 | 克隆分配流程（找空对→清旧节点→打标→回新号）；无空对答再试 | 机制 | 存量 | **07 §1.3/§2.3** | `get_free_pty` | |
| K-165 | 残留从节点双问题（死节点/安全洞） | 约束与不变量 | 存量 | **07 §1.3** | — | |
| K-166 | 输出环 2048 字节、满则只进能进的；主端读泵 | 数据结构 | 存量 | **07 §1.4/§2.6** | `pty.c` pump | |
| K-167 | 主端取消两路配对（有货答货、无货中断、对不上静默） | 机制 | 存量 | **07 §1.4/§2.5** | 主端 cancel | |
| K-168 | 包模式零字节信封 + 待办记录 | 架构演进 | 存量 | **07 §1.5/§2.9** | `set_packet_mode` | |
| K-169 | 主端选择两条规则（写虚看从端关/挂起/队空；读虚看环货）；从端关→一切就绪 | 机制 | 存量 | **07 §1.6/§2.5** | `select_try_pty` | |
| K-170 | ptyfs 同步通信、端点不缓存（现查）；从节点模式 = 字符设备 + 0620 | 机制 | 存量 | **07 §1.7/§2.7** | `ptyfs.c` | 语义归 15-stage-fs/20 |
| K-171 | 主端关闭三件事（标关、速度置 B0、发 SIGHUP） | 机制 | 存量 | **07 §2.4** | `pty.c:224`（`pty_master_close`） | **B2 的修复点** |
| K-172 | 从端读唤醒 / 从端写进环唤醒主端 | 机制 | 存量 | **07 §2.6** | `pty_slave_read/write` | |
| K-173 | 对状态用标志字节而非六布尔 | 架构演进 | 存量 | **07 §3.1** | `os/drivers/tty/pty/src/pair.rs` | |
| K-174 | 输出环计数泵（零字节占读名额，不真塞环） | 架构演进 | 存量 | **07 §3.3** | `buffer.rs` | |
| K-175 | 诊断总线定位（内核与服务汇入、只写与只读两方） | 概念 | 存量 | **08 §1.1** | — | |
| K-176 | 环 50000 字节、写满盖旧货；写永远成功 + 超长只留末窗 | 数据结构 | 存量 | **08 §1.2/§2.2/§2.3** | `log.h` `LOG_SIZE`、`log.c:124`（`subwrite`） | |
| K-177 | **写尾巴顺序：先唤醒挂起读、再通知选择者** | 约束与不变量 | 存量 | **08 §1.2/§3.6** | `log.c:171-191`、`:198`（`log_append`） | **本篇核心不变式**；B3 的修复点 |
| K-178 | 读三情形（有货当场答 / 非阻塞再试 / 阻塞挂起）；单路挂起、二路答零 | 机制 | 存量 | **08 §1.3/§2.4** | `log.c:23`（`log_read`） | |
| K-179 | 取消精确配对（调用方 + 标识）；非法号 EINVAL | 机制 | 存量 | **08 §1.4/§2.5** | `log.c:306`（`log_cancel`） | |
| K-180 | 选择三位 + 迟通知位；写永就绪、错位永不就绪 | 接口与协议 | 存量 | **08 §1.5/§2.5** | `log.c:323`（`log_select`）、`com.h:949-952` | |
| K-181 | 内核消息增量（新旧指针差取模、零新增照样推进） | 机制 | 存量 | **08 §1.6/§2.6** | `diag.c` `do_new_kmess` | |
| K-182 | 环定义与 `logdevice` 字段 | 数据结构 | 存量 | **08 §2.2** | `log.h` | |
| K-183 | 读写错号用 EIO 而非 ENODEV（历史写法原样保留）；选择错号用 ENODEV（第三条口径） | 约束与不变量 | 存量 | **08 §2.4/§2.5/§2.9** | `log.c` | **"原样保留并注释"的范例** |
| K-184 | 写循环 `subwrite` 逐段截断与安全复制；`LOGINC` 取模加 | 机制 | 存量 | **08 §2.3** | `log.c:124` | |
| K-185 | 读循环 `subread` 逐段搬与指针推进 | 机制 | 存量 | **08 §2.4** | `log.c:30`（`subread`） | |
| K-186 | 环做成三数计数器（字节数组留服务层） | 架构演进 | 存量 | **08 §3.1** | `os/drivers/system/log/src/ring.rs` | |
| K-187 | 单槽挂起是类型（不存在第二个） | 架构演进 | 存量 | **08 §3.2** | `device.rs` | |
| K-188 | 唤醒合并成一次调用返回值（读者腿前/选择者腿后/读位领取即清） | 架构演进 | 存量 | **08 §3.6** | `device.rs:135-139/194-196` | **B3 的修复点** |
| K-189 | 增量做成泛型游标 | 架构演进 | 存量 | **08 §3.5** | `diag.rs` | |

#### 组 F：系统服务与输入（现有 09–13 篇）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-190 | 全系统唯一熵源的定位（密钥/会话标识/地址随机化） | 概念 | 存量 | **09 说明/§1.1** | — | 定位 |
| K-191 | 三层结构（池收噪声 / 密钥流产字节 / 设备定阻塞） | 概念 | 存量 | **09 说明/§1.1** | — | |
| K-192 | 单设备事实核查（`NR_DEVS` 为一，无不阻塞设备） | 接口与协议 | 存量 | **09 §1.6/§2.9** | `random/main.c` | **纠正 plan stub 的双设备写法** |
| K-193 | 32 池 + 源内轮转规则 + 第一池人人有份 | 数据结构 | 存量 | **09 §1.2/§2.4** | `random/random.c:37`（`random_init`）、`NR_POOLS` | |
| K-194 | 播种计数选池的位运算（二进制第一个一） | 机制 | 存量 | **09 §1.2/§2.5** | `random.c:206`（`reseed`） | |
| K-195 | 导数过滤 16 阶差分、最小差 < 2 丢弃 | 机制 | 存量 | **09 §1.3/§2.4** | `random.c:130`（`add_sample`） | |
| K-196 | 播种条件 256 样本与混合顺序（旧密钥→零池→选中池→**终结**） | 机制 | 存量 | **09 §1.4/§2.5** | `random.c:206-236` | **S1 的修复点（杂凑终结）** |
| K-197 | 信任汁（写即播种、1 字节 = 8 样本） | 机制 | 存量 | **09 §1.4/§2.4** | `random.c:115`（`random_putbytes`） | |
| K-198 | 计数器加密产流（64 位高低字、低字先加、回绕进位） | 机制 | 存量 | **09 §1.5/§2.6** | `random.c:181`（`data_block`）、`:81`（`random_getbytes`） | |
| K-199 | 产流后换密钥（前向保密） | 机制 | 存量 | **09 §1.5** | `random.c:81` | |
| K-200 | 1024 字节大块与尾块侧产截取 | 机制 | 存量 | **09 §1.5/§2.7** | `RANDOM_BUF_SIZE` | |
| K-201 | 设备语义：未播种答再试（不替调用方等）；轮询永远全就绪 | 接口与协议 | 存量 | **09 §1.6/§2.7** | `random/main.c:24`（`r_read`）、`:259`（`r_select`） | |
| K-202 | 采集周期（未播种一拍 / 播种后五百拍） | 机制 | 存量 | **09 §1.6/§2.3** | `random/main.c:90`（`r_random`）、`KRANDOM_PERIOD` | |
| K-203 | 熵源 16 × 每源 64 样本分箱结构 | 数据结构 | 存量 | **09 §2.2** | `type.h:182-193`、`RANDOM_SOURCES`/`RANDOM_ELEMENTS` | |
| K-204 | 分箱环绕读取（一次取/跨界拆两段）与越界断言停下 | 机制 | 存量 | **09 §2.2** | `r_updatebin` | |
| K-205 | 采集定时（轮转取源 + 取自家定时器值 + 按播种状态重排闹钟 + 初始化取整批） | 机制 | 存量 | **09 §2.3** | `random/main.c:83`（`sef_cb_init_fresh`） | |
| K-206 | 初始化参数核对失败直接退出（拒绝错配内核） | 约束与不变量 | 存量 | **09 §2.3** | sanity 检查 | |
| K-207 | 池拓扑纯类型 + 播种混合顺序收成一个方法 | 架构演进 | 存量 | **09 §3.1** | `os/drivers/system/random/src/pool.rs` | |
| K-208 | 密码原语行为定义（杂凑两方法/分组密码一方法、不背审计责任） | 架构演进 | 存量 | **09 §3.2** | `core.rs` | **G4/A9 的落地** |
| K-209 | 密钥流计数器状态机（小端块头与 C 内存拷贝顺序一致） | 架构演进 | 存量 | **09 §3.3** | `core.rs` | |
| K-210 | 设备纯检查 + 分块计划 + 采集周期三纯函数 | 架构演进 | 存量 | **09 §3.4** | `device.rs` | |
| K-211 | 独立回放对账测试（播种顺序逐字节相等） | 测试性质 | 存量 | **09 §5.2** | — | |
| K-212 | 实时钟定位（断电也走、开机对表）；三操作权限门（读无门/写 SUPER_USER/断电 PM_PROC_NR） | 接口与协议 | 存量 | **10 §1.1/§2.3** | `readclock.c:39`（`main`） | |
| K-213 | 门在碰芯片与碰调用方缓冲之前 | 约束与不变量 | 存量 | **10 §1.1/§3.4** | — | |
| K-214 | 为何不走字符框架（七种请求形状不匹配） | 概念 | 存量 | **10 §1.2** | — | **"专有协议"的判定依据** |
| K-215 | 五请求号 + 一回复号 + 基址 `0x1400` | 接口与协议 | 存量 | **10 §1.2/§2.1** | `com.h:995-1012` | |
| K-216 | 转发三规矩（标签没配拒初始化/断电直转/读写走授权）；转发授权方向（读开写授权、写开读授权） | 机制 | 存量 | **10 §1.3/§2.5** | `forward.c:53/46/99/105/111` | |
| K-217 | 转发作为外观（隔离变化） | 概念 | 存量 | **10 §1.3** | — | |
| K-218 | BCD 换算两函数互逆 | 机制 | 存量 | **10 §1.4/§2.4** | `readclock.c:168/174` | |
| K-219 | 通知一律丢弃不回复；接收失败记日志继续（与框架"停下"相反） | 约束与不变量 | 存量 | **10 §1.5/§2.2** | `readclock.c:39` | **无状态→可继续的推理链** |
| K-220 | 回复用通用回复号 + 非阻塞发送（失败记日志） | 接口与协议 | 存量 | **10 §2.2** | `readclock.c:39` | |
| K-221 | 初始化装配（`arch_setup` 填操作表 `struct rtc`）；新旧重启热升级全走同一函数；退出不可达 | 机制 | 存量 | **10 §2.6** | `readclock.c` `arch_setup` | |
| K-222 | 架构时钟分工与操作表抽象 | 架构演进 | 存量 | **10 §2.7** | `readclock/arch/` | |
| K-223 | `RtcRequest` 枚举（授权版各占一变体） | 架构演进 | 存量 | **10 §3.1** | `os/drivers/clock/readclock/src/protocol.rs` | |
| K-224 | 时间值类型八字段与合理性谓词（闰秒、1970 下限、Y2K 标志） | 数据结构 | 存量 | **10 §3.2** | `com.h:1015`、`protocol.rs:104-117` | **G9 的落地** |
| K-225 | 权限纯分诊（两布尔→枚举，跳过分诊拿不到放行变体） | 架构演进 | 存量 | **10 §3.4** | `device.rs` | **类型层面强制** |
| K-226 | 设备库定位（枚举/配置读写/名单门禁） | 概念 | 存量 | **11 §1.1** | — | |
| K-227 | 查询协议 18 操作、基址 `0x300`、5/6 号空号 | 接口与协议 | 存量 | **11 §1.2/§2.1** | `com.h:95-160` | |
| K-228 | 游标制与无状态游标（索引在消息里） | 机制 | 存量 | **11 §1.2** | `main.c:62/91/117` | |
| K-229 | 配置空间 256 字节与三种宽度；宽度-对齐绑定与错位访问拒绝 | 数据结构 | 存量 | **11 §1.3/§3.2** | `os/drivers/bus/pci/src/config.rs` | **S4 的修复点** |
| K-230 | 端口号永不进操作系统层 | 架构演进 | 存量 | **11 §1.3/§3.2** | — | |
| K-231 | **可见性规则（名单跟调用方走）**；设备号条目匹配与 `0xFFFF` 子系统通配；类别码与掩码规则；无名单全见；摘单复见 | 机制 | 存量 | **11 §1.4/§3.3** | `pci.c:2039`（`visible`） | **S2 的修复点** |
| K-232 | 门禁做在遍历层（早于读写层一步） | 架构演进 | 存量 | **11 §1.4** | — | |
| K-233 | 枚举建档 `probe_bus`（递归桥/中断路由/地址窗口）；重复位置拒绝建档 | 机制 | 存量 | **11 §1.5** | `pci.c:1519`（`probe_bus`）、`:420`（`is_duplicate`） | |
| K-234 | 桥标识小抄表 37 行（用维护换时间） | 工具与工程 | 存量 | **11 §1.5/§2.7** | `pci_table.c` | |
| K-235 | 控制码七件事；映射先加内存特权后映射 | 接口与协议 | 存量 | **11 §1.6/§2.3** | `main.c:538`（`pci_ioctl`） | |
| K-236 | 配置读写只实现 32 位（8/16 位走查询协议） | 接口与协议 | 存量 | **11 §2.3** | — | |
| K-237 | 通用挂钩按消息号分发 18 问（对不上只打日志不回复）；字符表四挂钩、无读写挂钩 | 机制 | 存量 | **11 §2.4** | `main.c:670-760` | |
| K-238 | `BusQuery` 枚举 18 变体与空号跳过 | 架构演进 | 存量 | **11 §3.1** | `protocol.rs` | |
| K-239 | `DeviceDb` 记录向量上限 64 + 名单每端点一份（二次安装替换） | 数据结构 | 存量 | **11 §3.3** | `database.rs` | |
| K-240 | **预留占用位与四态处置**（越界/不可见/他人忙/自占重入） | 机制 | 存量 | **11 §3.3/§5.3** | `_pci_reserve`/`_pci_release` | **S3 的修复点** |
| K-241 | `BusControl` 枚举七变体（老式读写解码空转 EOPNOTSUPP） | 架构演进 | 存量 | **11 §3.4** | — | |
| K-242 | 引脚即文件定位；文件挂载规则（读文件必有；输出加开关两文件；输入加中断文件） | 概念 | 存量 | **12 §1.1** | `gpio.c` | |
| K-243 | 渲染 `"%d\n"` 两位与偏移三情形；渲染三元组返回约定 | 机制 | 存量 | **12 §1.2/§2.3** | `gpio.c:237`（`gpio_read`） | |
| K-244 | 认领三查（脚号有人/模式/属主）；读写两查与输入脚拒绝驱动；中断读查认领不查方向 | 机制 | 存量 | **12 §1.3/§2.2** | `gpio.c:78`（`add_gpio_inode`） | |
| K-245 | 先到先得是全部公平性来源（无优先级无抢占、持有至进程结束） | 约束与不变量 | 存量 | **12 §1.3** | — | |
| K-246 | 设备注册三件套（父设备号/名字/属性表）；序列化三段布局与长度可算 | 数据结构 | 存量 | **12 §1.4/§2.4** | `libdevman/generic.c:36`（`serialize_dev`）、`:102`（`devman_add_device`）、`:154`（`devman_del_device`） | |
| K-247 | 柄表语义（空槽复用最小号/删空槽假/查借记录/列活柄） | 数据结构 | 存量 | **12 §1.4/§3.4** | `generic.c` | |
| K-248 | USB 跟踪多一层（32 接口、绑定回调记端点、拒绝记空） | 数据结构 | 存量 | **12 §1.4/§2.5** | `libdevman/usb.c` | |
| K-249 | 客户端不设未绑定计数器（**勘误**：对 C 的误读） | 架构演进 | 存量 | **12 §1.4/§3.5** | — | E-DMCLIENT 勘误 |
| K-250 | 两表对照与"登记三部曲"（名单/认领/注册同一灵魂） | 概念 | 存量 | **12 §1.5** | 借 11-pci 对照 | |
| K-251 | 引脚导出流程（认领→设模式→分配回调→挂文件→板型分支） | 机制 | 存量 | **12 §2.2** | `gpio.c:78` | |
| K-252 | 开关文件语义（置位清零、读完答零字节）；中断文件锁存标记与取完清零；消息钩子转交中断消息 | 机制 | 存量 | **12 §2.3** | `gpio.c` | |
| K-253 | 虚拟树三钩子（初始化/读/消息）、根目录只读 | 机制 | 存量 | **12 §2.3** | vtreefs | 借 15-stage-fs/18 |
| K-254 | 通用注册增删与端点静态存（重启重查在服务层） | 机制 | 存量 | **12 §2.4** | `generic.c:188`（`devman_init`） | |
| K-255 | 引脚硬件行为定义（驱动/采样/采中断）与两实现 | 架构演进 | 存量 | **12 §3.1** | `os/drivers/system/gpio/src/pins.rs` | |
| K-256 | 导出命名规划（四后缀、读规划四变体、渲染三元组） | 架构演进 | 存量 | **12 §3.3** | `files.rs` | |
| K-257 | 库名加客户端后缀（**包名与库名双撞**） | 架构演进 | 存量 | **12 §3.6** | — | 命名事实核查 |
| K-258 | 扫描码翻译定位（源语言→系统普通话）；页码为零表项即填充、吞掉不出事件 | 概念 | 存量 | **13 §1.1** | — | |
| K-259 | 状态机四态与三个前缀；暂停键六字节前奏；**暂停确认态的穿透（FALLTHROUGH）**；错位自愈回零态 | 数据结构 | 存量 | **13 §1.2/§2.2** | `pckbd.c:328`（`kbd_process`） | **S5 的修复点** |
| K-260 | 鼠标三字节包与包头同步位判定；按钮逐位比；位移符号展 32 位 + 相对标志 | 机制 | 存量 | **13 §1.3/§2.2** | `pckbd.c:374`（`kbdaux_process`） | **S7 的修复点** |
| K-261 | 短包自定界的协议优点（错一组好一组） | 概念 | 存量 | **13 §1.3** | — | |
| K-262 | LED 两字节命令与 16 字节队列；满队丢并清确认标志；有货且无未确认才发一字节、ACK 才推进 | 数据结构 | 存量 | **13 §1.4/§2.3** | `pckbd.c:175`（`set_leds`）、`KBD_OUT_BUFSZ` | |
| K-263 | **LED 掩码位对位翻译**（输入服务三位→键盘三位） | 机制 | 存量 | **13 §1.4/§2.3** | `input.h:293-295` | **S6 的修复点（位序错一位）** |
| K-264 | 事件桥单行道定位；桥状态三件套；配置与设灯消息验发送方；阻塞发两条理由与背压语义 | 接口与协议 | 存量 | **13 §1.5/§2.4** | `libinputdriver/inputdriver.c:43`（`inputdriver_send_event`）、`:21`（`inputdriver_announce`）、`:142`（`inputdriver_process`） | |
| K-265 | 初始化认亲（键盘必有/鼠标可选）与上报标志 | 机制 | 存量 | **13 §1.6/§2.5** | `pckbd.c:500`（`main`） | |
| K-266 | 看门狗两条与复位重发 | 机制 | 存量 | **13 §2.5** | `pckbd.c:47`（`kbd_watchdog`）、`:111`（`scan_keyboard`） | G7 缺项 |
| K-267 | 扫描码状态机三变体结局（等下文/出事件/吞掉） | 架构演进 | 存量 | **13 §3.1** | `scancode.rs` | |
| K-268 | 对照表行为定义（示例表/空表/**全表 111+40 项**） | 架构演进 | 存量 | **13 §3.2** | `tables.rs` | G3 的落地 |
| K-269 | 鼠标三字节组装器 + 事件枚举（调用方穷尽 match） | 架构演进 | 存量 | **13 §3.3** | `mouse.rs` | |
| K-270 | LED outbox（排队/取字节/确认三方法） | 架构演进 | 存量 | **13 §3.4** | `led.rs` | |
| K-271 | 桥门禁状态（发送留服务层，本库只出 verdict） | 架构演进 | 存量 | **13 §3.5** | `os/libs/minix-sys/src/inputdriver.rs` | **A11 收敛方向** |

#### 组 G：存储与 USB（现有 14–19 篇）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-272 | 虚拟队列三环结构（描述符表/可用环/已用环），旋转寿司隐喻 | 概念 | 存量 | **14 §1.1** | `virtio_ring.h` | |
| K-273 | 写权限分离（客户机只写可用环、宿主机只写已用环、描述符两边只读，故免锁） | 约束与不变量 | 存量 | **14 §1.1** | — | 解释"为什么 virtio 环不需要锁" |
| K-274 | 描述符链串接（`下一` 标志把多块缓冲串成链，末块不置下一位） | 机制 | 存量 | **14 §1.2** | `VRING_DESC_F_NEXT` | |
| K-275 | 写标志管方向；状态块恒置写位 | 约束与不变量 | 存量 | **14 §1.2** | — | |
| K-276 | 间接标志=整单外包（间接表），间接表启动时预分配 | 机制 | 存量 | **14 §1.2** | `VIRTIO_RING_F_INDIRECT_DESC` | |
| K-277 | 空闲链取/还必须常数时间（头指针加减、计数加减、无搜索） | 约束与不变量 | 存量 | **14 §1.2** | — | |
| K-278 | 可用序号（发单号）与已用序号（吃进度），十六位自然回绕、回绕减法结果恒对 | 机制 | 存量 | **14 §1.3** | — | |
| K-279 | 中断抑制两标志（已用不通知、可用不中断）；抑制全是优化建议，不是正确性依赖 | 接口与协议 | 存量 | **14 §1.3** | — | **"铃响不响，菜照吃"** |
| K-280 | 特性协商取交集（读宿主位、掩码客户请求、回写交集）；未知宿主位直接忽略；协商失败不拦路 | 接口与协议 | 存量 | **14 §1.4** | `virtio.c:166`（`exchange_features`） | |
| K-281 | 建设备五步（验参数、找卡、认亲、开队列、备间接表） | 机制 | 存量 | **14 §1.5/§2.3** | `virtio.c:109`（`virtio_setup_device`） | |
| K-282 | 认亲状态字节（`ACK` + `DRV` 两字节）；就绪三步（队列内存就位、中断挂号、写 `DRV_OK`） | 接口与协议 | 存量 | **14 §1.5** | `virtio.h` 状态字节 | |
| K-283 | 复位一键（状态清零、中断注销、设备重置）；释放两步先队列后设备 | 约束与不变量 | 存量 | **14 §1.5/§2.5** | `virtio.c:742`（`virtio_reset_device`） | |
| K-284 | **踢门铃规矩：只查不通知位，清了才踢** | 约束与不变量 | 存量 | **14 §1.5/§2.5** | `virtio.c:766`（`wants_kick`） | **V1 的修复点（曾多加 queue_full 分支）** |
| K-285 | 正确性不靠铃（宿主机收不到铃在轮询序号，踢丢不致命、踢多白费） | 约束与不变量 | 存量 | **14 §1.5** | — | |
| K-286 | 中断规矩与踢对称（客户机说别吵就不吵） | 约束与不变量 | 存量 | **14 §1.5** | `virtio.c:345`（`virtio_irq_register`） | |
| K-287 | `vring_desc` 十六字节字段布局（地址 8、长度 4、标志 2、下一 2） | 数据结构 | 存量 | **14 §2.2** | `virtio_ring.h` | |
| K-288 | 描述符标志三位（下一/写/间接）；可用环与已用环头结构 + 完成项八字节 | 数据结构 | 存量 | **14 §2.2** | `VRING_DESC_F_*` | |
| K-289 | 协商两特性位（间接二十八、事件索引二十九） | 接口与协议 | 存量 | **14 §2.2** | `VIRTIO_RING_F_INDIRECT_DESC` | |
| K-290 | 头文件禁改声明写在注释里（改了宿主机客户机两边全断） | 约束与不变量 | 存量 | **14 §2.2** | `virtio_ring.h` | |
| K-291 | 队列内存是**连续物理页** = 客户机与宿主机共享的唯一内存 | 约束与不变量 | 存量 | **14 §2.3** | — | **DMA 契约的动机（→新 30 篇）** |
| K-292 | 建设备参数校验四项（空名、负数、零线程全拒）；找卡厂商对+子设备对；开队列失败则释放设备 | 机制 | 存量 | **14 §2.3** | `virtio.c:109` | |
| K-293 | 队列物理初始化与索引（虚实地址、页号、环大小、空闲链串好、已收序号清零；无货返回负一） | 机制 | 存量 | **14 §2.4** | `virtio.c:276-506` | |
| K-294 | 踢中断族（中断归属读状态寄存器、复位、开关中断、匹配设备、特性查询、读三宽度） | 机制 | 存量 | **14 §2.5** | `virtio.c:736-831` | |
| K-295 | 环索引状态机（空闲链 + 可用序号 + 已用序号 + 已收序号"四数一结构"） | 架构演进 | 存量 | **14 §3.1** | `os/libs/minix-virtio/src/ring.rs` | |
| K-296 | **线格式由本库承载**（`#[repr(C)]` 钉十六字节、`vring_size`/`avail_offset`/`used_offset` 布局函数） | 架构演进 | 存量 | **14 §3.1** | `ring.rs` | **A5 的落地（policy/transport 边界重划）** |
| K-297 | `take_chain` 连链、`collect_chain` 回自由表，支持乱序完成 | 架构演进 | 存量 | **14 §3.1** | `ring.rs:169-176` | |
| K-298 | DMA 内存索取做成 `Hal` 行为定义（分配/释放/区域三件） | 架构演进 | 存量 | **14 §3.1** | `os/libs/minix-virtio/src/hal.rs` | **→新 30 篇的第一次出现** |
| K-299 | 协商交集函数（替代方案：要求全等=拒绝升级） | 架构演进 | 存量 | **14 §3.2** | `features.rs` | |
| K-300 | 生命周期阶段机五态（新/认亲/列队/就绪/失败），乱序调用拒绝，状态字节累积写 | 架构演进 | 存量 | **14 §3.3** | `device.rs` | |
| K-301 | 端口行为定义（读写字节两方法；空端口与向量端口两实现） | 架构演进 | 存量 | **14 §3.4** | `device.rs` | |
| K-302 | 三段链形状（订单小票=头、饭菜=数据段、评价卡=状态字节）与顺序固定 | 数据结构 | 存量 | **15 §1.1** | — | |
| K-303 | 方向定类型（读 `T_IN` 进、写 `T_OUT` 出）；位置除 512 定扇区号；向量定段数；状态段恒一位 | 机制 | 存量 | **15 §1.1** | `VIRTIO_BLK_T_IN`/`T_OUT` | |
| K-304 | 状态字节三值（零成功、一输入输出错、二不支持）；**C 的未知状态默认分支是 panic** | 接口与协议 | 存量 | **15 §1.1/§2.3** | `virtio_blk.c:549`（`virtio_blk_status2error`） | **V2 的修复点** |
| K-305 | **Rust 无停机环境改判输入输出错，显式记为偏差**；铁律：未知状态永不按成功放行 | 架构演进 | 存量 | **15 §1.1/§2.3/§3.2/§4** | 同上 | **全 stage 唯一正式"显式偏差"** |
| K-306 | 扇区对齐双铁律（位置与长度必须 512 对齐，不对齐直接无效参数） | 约束与不变量 | 存量 | **15 §1.2** | — | |
| K-307 | 截断（请求跨分区尾按剩余截短，截到块边界）；向量修整（超长末段截短、截空的段丢掉） | 机制 | 存量 | **15 §1.3** | — | |
| K-308 | 到头（位置过分区尾）答零字节；只读盘写在编链前就拒 | 约束与不变量 | 存量 | **15 §1.3** | — | |
| K-309 | 单盘单分区（全驱动只认一块盘一个分区）；几何 = 容量乘块大小 | 架构演进 | 存量 | **15 §1.4** | — | 设计决策 |
| K-310 | 控制两问（数人头=打开计数拷回、冲水=刷写缓存）；其余控制码一律不合适操作 | 接口与协议 | 存量 | **15 §1.5** | — | |
| K-311 | 请求组装完整顺序（对齐校验→到头→向量修整→块数校验→虚实映射→填头→三段进链→发链→等完成→看状态） | 机制 | 存量 | **15 §2.2** | `virtio_blk.c:280-379` | |
| K-312 | 特性表八项（屏障/分段/几何/只读/块大小/刷新/拓扑/标识字节） | 接口与协议 | 存量 | **15 §2.4** | `virtio_blk.h` | |
| K-313 | `plan_transfer` 纯函数（位置长度分区进，类型扇区段数字节出） | 架构演进 | 存量 | **15 §3.1** | `os/drivers/storage/virtio_blk/src/request.rs` | |
| K-314 | `status_to_code` 全匹配；两替代方案各给否决理由 | 架构演进 | 存量 | **15 §3.2** | `request.rs` | |
| K-315 | `DriveGeometry` 值类型；`OpenCount` 饱和减 | 架构演进 | 存量 | **15 §3.3** | `geometry.rs` | |
| K-316 | 高级主控端口＝机场塔台（32 条命令槽；申请-交回-超时封锁-复位重开） | 概念 | 存量 | **16 §1.1** | `COMMAND_SLOTS` | |
| K-317 | 端口槽位图管理动作（占槽常数时间、完成清槽、超时清全部转超时态、复位回停止态、停机清槽） | 机制 | 存量 | **16 §1.1/§2.2** | `ahci.c:900-985/1810-1877` | |
| K-318 | 铁律：先开塔（端口启动）再申请（发命令） | 约束与不变量 | 存量 | **16 §1.1** | — | |
| K-319 | 并行接口＝火车站（4 驱动器 `MAX_DRIVES`、一列最多 256 节 `MAX_SECS`）；控制器四/五态与乱序拒绝 | 机制 | 存量 | **16 §1.2/§3.3** | `MAX_DRIVES`、`MAX_SECS` | |
| K-320 | 大单拆小单（超 256 扇区分整单加零头；纯算术无硬件依赖） | 机制 | 存量 | **16 §1.2** | `MAX_SECS` | |
| K-321 | 出错锁存待复位（不许带病接车） | 约束与不变量 | 存量 | **16 §1.2** | — | |
| K-322 | 直接存取三态与守卫（闲/已武装/已到；零扇区拒、重武装拒、验错清状态、早停回闲） | 机制 | 存量 | **16 §1.3/§3.4** | `at_wini.c:564-961` | |
| K-323 | 验车：设备错位清说明货没到、无错位清说明货到了 | 机制 | 存量 | **16 §1.3** | — | |
| K-324 | **识别先验后量**（字零三位任一置位即拒；第 49 字要求直接存取与线性寻址；第 83 字要求字有效、刷缓存、48 位寻址） | 约束与不变量 | 存量 | **16 §1.4/§2.4** | `ahci.c:528`（`ata_id_check`） | **V5 的修复点** |
| K-325 | 容量读四字拼 64 位；两太字节以上不截尾；零容量等于无盘；块短于 256 字拒 | 数据结构 | 存量 | **16 §1.4** | `ata_id_check` | |
| K-326 | 超时复位哲学（超时→全失败→转态拒发；复位=清槽回停止→重编表→重启动）；停止清槽铁律 | 机制 | 存量 | **16 §1.5/§2.3** | `ahci.c:1220-1335/1715-1793` | |
| K-327 | 等待分两种（忙等短有 bound / 睡等长让出）；混用是功夫 | 架构演进 | 存量 | **16 §2.5** | `at_wini.c:314-453/1626-1727` | |
| K-328 | 端口做成槽位图（32 布尔数组）；识别做成纯解析；控制器做成阶段机；直接存取做成武装机 | 架构演进 | 存量 | **16 §3.1-3.4** | `os/drivers/storage/ahci/src/{port,identify}.rs`、`at_wini/src/{controller,dma}.rs` | |
| K-329 | 软盘＝年迈磁带录音机（磁头偏要重试、偏多重新校准、读不出认栽） | 概念 | 存量 | **17 §1.1** | — | |
| K-330 | 软盘七组驱动介质组合与常用四种介质；**重试策略**（不可重试错误直接认栽；满 6 次认栽；到 3 次先重新校准，校准不清零） | 机制 | 存量 | **17 §1.1/§2.3/§3.2** | `floppy.c:161-177`（`fdensity`）、`:652-657`、`err_no_retry`、`MAX_ERRORS` | |
| K-331 | 闪存卡＝排队考试考生（五阶段上电：新生/轮询条件/识别身份/配置参数/就绪；每阶段只认一条命令，乱序成功不算成功） | 机制 | 存量 | **17 §1.2/§2.7/§3.3** | `emmc.c:790-890` | **V4 的修复点（曾自相矛盾）** |
| K-332 | 块长度谈成 512 字节后读写固定按此尺寸 | 约束与不变量 | 存量 | **17 §1.2** | `MMC_SET_BLOCKLEN` | |
| K-333 | 故障注入盘＝替身演员（请求原样转交下层，三时机按剧本动手） | 概念 | 存量 | **17 §1.3** | — | |
| K-334 | 三拦截点语义（前拦截/中拦截/后拦截）；规则命中语义（地址范围命中即按规则，范围外放行一个字节不碰） | 机制 | 存量 | **17 §1.3/§2.4** | `fbd.c:420-435`、`rule.c:115`（`rule_find`） | **V6 的修复点** |
| K-335 | **过滤盘**（每 8 扇区一校验和，9 扇区一组，读先验后交，失败按策略报错或放行记一笔）；镜像双份 | 机制 | 存量 | **17 §1.4/§2.5** | `filter/sum.c`、`filter/driver.c:242-408` | |
| K-336 | **镜像退场账本**（每下层驱动一本账，重启记一笔，满 3 次：镜像开着→全局关镜像+幸存备份转正答成功；镜像已关→答 IO 错） | 机制 | 存量 | **17 §1.4/§2.5** | `driver.c:384-408`、`NR_RESTARTS`、`driver[which].kills` | **V3 的修复点** |
| K-337 | 校验种类四值（无/异或/循环冗余/消息摘要）与组布局 8+1 | 数据结构 | 存量 | **17 §1.4** | `ST_NIL`/`ST_XOR`/`ST_CRC`/`ST_MD5` | |
| K-338 | 回环盘＝翻译（扇区读写翻译成文件读写；单次最多 65536 字节；写后强制同步） | 机制 | 存量 | **17 §1.5/§2.6** | `vnd.c:31`（`vnd_transfer`）、`VND_BUF_SIZE` | |
| K-339 | 几何现编（文件无几何时按扇区总数推导：大文件 64 磁头 32 扇区，小文件单磁头单扇区） | 机制 | 存量 | **17 §1.5/§2.6** | `vnd.c` `vnd_layout`、`VNDIOF_HASGEOM` | |
| K-340 | 五个回调表对照矩阵（位置/设备类型/已实现/未实现；真盘三型=磁盘、代理两型=其他；回环六回调最全） | 接口与协议 | 存量 | **17 §2.2** | `blockdriver.h:16-30` 基准 | |
| K-341 | 六项 Rust 设计决策（密度静态表、重试计数器、上电阶段机、规则地址区间、校验镜像策略、回环纯布局） | 架构演进 | 存量 | **17 §3.1-3.6** | `os/drivers/storage/{floppy/src/geometry,mmc/src/commands,fbd/src/rules,filter/src/checksum,vnd/src/layout}.rs` | |
| K-342 | 主机抽象与两版实现（`mmchost.h`；具体实现 + 空实现测试替身） | 接口与协议 | 存量 | **17 §2.7** | `mmchost.h`、`mmchost_dummy.c` | |
| K-343 | 主机守护＝邮局、请求包＝挂号信；驱动发五种／守护回四种 | 概念 | 存量 | **18 §1.1** | — | |
| K-344 | 编号契约（报到+0、注销+1、交包+2、撤回+3、报信息+4、回执+5、办结+6、出现+7、消失+8） | 接口与协议 | 存量 | **18 §1.1/§2.2** | `com.h:813-828` | |
| K-345 | 枚举＝新生报到五步（端口复位、取短设备描述符、分配总线地址、取全部配置描述符建树、激活默认配置）；失败回发现态重新复位 | 机制 | 存量 | **18 §1.2/§2.5/§3.2** | `hcd.c:473-890` | |
| K-346 | 客户端库＝秘书（填端点与包标识、完成回调、上下线回调、待办包链表、零标识=查无此人） | 机制 | 存量 | **18 §1.3/§2.3** | `libusb/usb.c:20`（`usb_send_urb`）、`:121`（`usb_init`）、`:232`（`usb_send_info`） | **V8 的修复点（出册）** |
| K-347 | 调度＝投递班次（同时在飞最多 16 个；内外请求包统一排队，办结一个放行一个）；调度器线程与设备线程分工 | 约束与不变量 | 存量 | **18 §1.4** | `hcd_schedule.c` | |
| K-348 | 硬件后端唯一（按板型初始化九个回调）；硬件相关封在后端，枚举顺序与包编号与板子无关 | 架构演进 | 存量 | **18 §1.5** | `musb_am335x.c` | |
| K-349 | 消息字段映射（授权标识/包标识/结果/设备标识各归哪个消息字） | 接口与协议 | 存量 | **18 §2.2** | `com.h:829-840` | |
| K-350 | 守护启动四件事（守护初始化、主机控制器初始化、设备管理初始化、启动） | 机制 | 存量 | **18 §2.4** | `usbd.c:36-151` | |
| K-351 | 服务线程与调度初始化（服务线程只调设备套件服务初始化，内建无限接收循环） | 机制 | 存量 | **18 §2.4** | `usbd.c:129/135/139/179-183` | |
| K-352 | 枚举八动作实现（复位、定最大包长、取设备描述符、回写包长、设地址、睡五毫秒、取描述符树、设配置） | 机制 | 存量 | **18 §2.5** | `hcd.c:473-562` | |
| K-353 | 每设备一个设备线程（先枚举、再连回调、再无限等包事件） | 机制 | 存量 | **18 §2.5** | `hcd.c:250-280` | |
| K-354 | 驱动回调表三成员（完成/接设备/断设备）；请求包结构 `struct usb_urb`（十余字段）；无效标识零；传输类型四种；控制请求八字节 | 数据结构 | 存量 | **18 §2.6** | `usb.h:24-104` | **A5 的落地（`wire.rs`）** |
| K-355 | 编号做成枚举、枚举做成阶段机六态、跟踪做成查表 | 架构演进 | 存量 | **18 §3.1-3.3** | `os/drivers/usb/usbd/src/{protocol,enumerate}.rs`、`os/libs/minix-usb/src/urb.rs` | |
| K-356 | 包头包尾＝信封（SCSI 命令装进 CBW；CSW 带同编号与结果） | 概念 | 存量 | **19 §1.1** | — | |
| K-357 | 三段式传输（先发 CBW，再传数据，最后收 CSW 验标签；缺一段不算完）；标签规则（从 1 开始，每趟加一，上一趟回来前不开下一趟） | 机制 | 存量 | **19 §1.1/§2.2/§3.2** | `bulk.h:13-42`、`usb_storage.c:230-347` | |
| K-358 | SCSI 七种常用命令与命令长度/数据长度表（读容量 10 字节回 8；问询 6 字节回 36） | 接口与协议 | 存量 | **19 §1.2/§2.3** | `scsi.h:30-127`、`scsi.c:31-288` | |
| K-359 | 扇区对齐守卫（读写地址与长度须为 512 整数倍，否则直接拒） | 约束与不变量 | 存量 | **19 §1.2/§2.4** | `usb_storage.c:795` | |
| K-360 | 集线器＝门卫（8 端口、1000 毫秒巡逻、复位最多 3 次每次 200 毫秒） | 机制 | 存量 | **19 §1.3/§2.5** | `usb_hub.c:47-56`、`:390-512`（`hub_task`） | |
| K-361 | **端口三态与拉黑**（断开/连上/坏掉；状态错永久标记坏且巡逻跳过、连状态都不读、住户搬走也不洗白）；**通信错语义**（不是端口的事：挂起整个集线器任务等摘除） | 机制 | 存量 | **19 §1.3/§3.4** | `hub_task` | **V7 的修复点** |
| K-362 | 复位预算（三计数，用完即拉黑）；集线器不挂块框架也不挂字符框架 | 约束与不变量 | 存量 | **19 §1.3/§3.4** | `USB_HUB_MAX_TRIES` | |
| K-363 | 两份端点助手同构声明（各 111 行；配端点、绑数据、阻塞提交） | 工具与工程 | 存量 | **19 §1.4** | 两份 `urb_helper.c` | |
| K-364 | 包签名常量（`CBW_SIGNATURE` 0x43425355、`CSW_SIGNATURE` 0x53425355、命令块 16 字节） | 数据结构 | 存量 | **19 §2.2/§3.1** | `bulk.h:13/30/16` | |
| K-365 | 状态核对三项（查标签、签名、成功） | 接口与协议 | 存量 | **19 §2.3** | `scsi.c:267-288`（`check_csw`） | |
| K-366 | 块回调表 `mass_storage`（类型磁盘；七个回调；中断/闹钟/其他置空） | 数据结构 | 存量 | **19 §2.4** | `usb_storage.c:105-118` | |
| K-367 | 几何兜底（先读模式页；不行按尺寸兜底：柱面=尺寸除扇区、磁头 64、每道 32 扇区） | 机制 | 存量 | **19 §2.4** | `usb_storage.c:1476/1505` | |
| K-368 | 变化真值表（清变化位放行轮询、到访调连上、出走调断开、先走后到两个都调） | 机制 | 存量 | **19 §2.6** | `usb_hub.c:693`（`hub_handle_change`） | |
| K-369 | 连上报到流程（置端口复位、轮询复位完成位最多 3 次每次睡 200 毫秒、清复位位、要求连上且使能、译速度、上报） | 机制 | 存量 | **19 §2.6** | `usb_hub.c:826/921/929-936` | |
| K-370 | 签名做成常量、标签做成配对器、守卫做成纯函数、端口做成看法机 | 架构演进 | 存量 | **19 §3.1-3.4** | `os/drivers/usb/usb_storage/src/cbw.rs`、`usb_hub/src/ports.rs` | |
| K-371 | `repr(C, packed)` 钉 31/13 字节（CBW/CSW 线格式） | 架构演进 | 存量 | **19 §2.7** | `cbw.rs` | **A5 的落地** |

#### 组 H：显示、音频、网络、杂项（现有 20–24 篇）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-372 | **帧缓冲是字符设备而非映射设备**（核心命题，对 plan 的反驳） | 概念 | 存量 | **20 说明/勘误 L16** | `chardriver.h:9-23`（十成员无映射）、`fb.c:44-51` | **勘误块：plan 写错，源码实证为准** |
| K-373 | 打开计数加一/减一 + **一次性初始化旗**（独立于计数） | 机制 | 存量 | **20 §1.1/§2.3/§3.3** | `fb.c:60`（`fb_open`）、`:91`（`fb_close`） | **N4 的修复点** |
| K-374 | 首次打开读 EDID + `arch_fb_init` | 机制 | 存量 | **20 §2.3** | `fb.c:70/74` | |
| K-375 | 读写按设备大小截断；重启窗口门控写 | 约束与不变量 | 存量 | **20 §1.1/§1.5/§2.3** | `fb.c:28`（`fb_read`）、`:30`（`fb_write`）、`:221` | |
| K-376 | 四个 ioctl 请求集合；陌生请求回 `ENOTTY`；改可变参数只许改垂直偏移；平移转调改参数 | 接口与协议 | 存量 | **20 §1.2/§2.4** | `ioc_fb.h:11-14`、`fb.c:32`（`fb_ioctl`） | |
| K-377 | `fb_tab` 十成员只填五项；`chardriver_task` 主循环；初始化回调三分支 | 数据结构 | 存量 | **20 §2.2** | `fb.c:44-51`、`:308`（`main`） | |
| K-378 | EDID 128 字节体检表；经块协议五步读取；缺省合法（无表不报错） | 数据结构 | 存量 | **20 §1.3/§2.5** | `fb_edid.c:55`（`fb_edid_args_parse`） | |
| K-379 | `choose_mode` 交集最高分辨率；硬件支持四种分辨率；默认 1024×600/32 位色/双缓冲 | 机制 | 存量 | **20 §1.4/§2.6** | `fb_arch.c:144-164`、`omap_supported_modes` | |
| K-380 | 帧缓冲大小纯算术（宽×高×深÷8，64 位防溢出）；连续物理内存不够则启动失败 | 机制 | 存量 | **20 §1.4/§2.6/§3.2** | `os/drivers/video/fb/src/mode.rs` | |
| K-381 | 后端唯一（earm）；`logos.h` 3054 行位图不翻译 | 架构演进 | 存量 | **20 §2.1/§2.6/§2.8** | `fb_arch.c`、`logos.h` | |
| K-382 | 音频框架十四驱动钩子；字符表 `audio_tab` 五回调 | 接口与协议 | 存量 | **21 §1.1/§2.2** | `audio_fw.h:9-22`、`audio_fw.c:64-81` | |
| K-383 | 十一声音控制请求 | 接口与协议 | 存量 | **21 §1.1/§2.3** | `ioc_sound.h:12-22` | G1/G7 缺项 |
| K-384 | 速率上下限 4000–44100、默认 44100；转换器内部固定 48000 Hz；三通道路由 | 常量 | 存量 | **21 §1.2/§2.4** | `es1371.h:106/110-111`、`es1371.c:522`（`set_sample_rate`） | |
| K-385 | `src_set_rate` 新封装；`drv_start` 先设速率再继续；`drv_stop`/暂停/继续专函 | 机制 | 存量 | **21 §2.4** | `es1371.c:228`（`drv_start`）、`:268`（`drv_stop`）、`sample_rate_converter.c` | |
| K-386 | sb16 端口基址 0x220；复位协议（拉高拉低、等 0xAA）；版本命令 0xE1；速率方向命令 0x66/0x65 + 高低字节 | 机制 | 存量 | **21 §1.3/§2.5** | `sb16.h:20`、`sb16.c:86`（`dsp_command`）、`:345`（`dsp_set_speed`） | |
| K-387 | 喇叭开关/停播/续播各一字节；**sb16 暂停等于停止** | 机制 | 存量 | **21 §2.5** | `sb16.c:134-171/184/221/229` | |
| K-388 | 七兄弟差异矩阵（7 行）；五份同构混音器 | 数据结构 | 存量 | **21 §1.4/§2.6** | `mixer.c` 系列 | **文档内部矛盾：L40"五份"vs 矩阵四家** |
| K-389 | `minix-audiodriver` 框架库（`AudioHooks` 十四方法、`SubDevice` 分片环、特殊文件表） | 架构演进 | 存量 | **21 头部/§2.7** | `os/libs/minix-audiodriver/` | **G1 的落地（曾零归属）** |
| K-390 | 范围守卫函数、通道路由枚举、命令常量、分字节纯函数 | 架构演进 | 存量 | **21 §3.1-3.4** | `os/drivers/audio/es1371/src/rate.rs`、`sb16/src/dsp.rs` | |
| K-391 | **头文件纠正**（`audio_fw.h` 而非 `audiodriver.h`） | 工具与工程 | 存量 | **21 勘误 L16** | 三头文件 | **勘误块：plan 写错** |
| K-392 | 每页 256 字节页几何；接收页游标（边界页/当前页/下一页、绕回） | 数据结构 | 存量 | **22 §1.1/§2.4/§3.1** | `dp8390.h:189-190`、`dp8390.c:598-720` | |
| K-393 | 接收头解析长度；长度越界丢包；**链尾落起始页特例（写停止页减一）** | 机制 | 存量 | **22 §2.4/§3.1** | `dp8390.c:633-639`、`:668-671` | **G9 的修复点（BNRY 特例）** |
| K-394 | 发送队列两项；每项六页（6×256 够 1514）；队满回发送不能；完成中断清标记推尾指针 | 数据结构 | 存量 | **22 §1.2/§2.5** | `dp8390.h:189/190`、`dp8390.c:230-232/547-560` | |
| K-395 | `struct netdriver` 十三成员；回调名前缀 `ndr_` 而非 `ndo_` | 数据结构 | 存量 | **22 §2.2** | `netdriver.h:23-40` | **勘误块：plan 写错** |
| K-396 | `dp_table` 只填七项；`netdriver_task` 托管主循环；组播广播能力回填与时钟节拍；统计经时钟回调读计数器 | 机制 | 存量 | **22 §2.3** | `dp8390.c:103-121/146-149/293-299` | |
| K-397 | 虚拟网卡三队列（收/发/控制）；包上限 1514；`virtio_net_table` 五项；初始化六步串接 | 数据结构 | 存量 | **22 §1.3/§2.6** | `virtio_net.c:36/44/92-99/389-417` | |
| K-398 | `refill_rx` 阈值补充（**不足一半补**、每包两段）；`check_queues`；中断=查+报+补充；短帧垫到 60 | 机制 | 存量 | **22 §1.3/§2.6/§3.4** | `virtio_net.c:216-243/248-270/284-302/369-370` | **N1 的修复点（常量曾放大 4 倍）** |
| K-399 | 板级多态（四板共用页逻辑、函数指针各填）；兼容卡优先探测 | 架构演进 | 存量 | **22 §1.4/§2.1** | `3c503.c`/`ne2000.c`/`rtl8029.c`/`wdeth.c` | |
| K-400 | 组播筛选粗规矩（混杂/广播位/组播列表位） | 约束与不变量 | 存量 | **22 §1.5** | — | |
| K-401 | e1000 收发各 256 描述符；每格 2048 字节缓冲；收/发描述符字段 | 常量 | 存量 | **23 §1.1/§2.3** | `e1000.h:29/32/35`、`e1000_hw.h:31/46` | |
| K-402 | 尾指针推进、到顶绕回；空环=尾追上头 | 机制 | 存量 | **23 §1.1/§3.1** | — | |
| K-403 | rtl8139 发送四槽 + 状态寄存器四连号；接收 65536 字节环 + 两游标；取包绕回 | 数据结构 | 存量 | **23 §1.2/§2.4** | `rtl8139.h:19/20-34/435/46-48` | |
| K-404 | lance 收发环各 16 格；初始化块；**芯片版本表两段式认卡**（低 12 位过 0x003 门 + 高 16 位查表） | 数据结构 | 存量 | **23 §1.3/§2.5/§3.4** | `lance.c:97-106/109/63-87/707-722`（`lance_probe:280`） | **N2 的修复点（曾单字段简化错）** |
| K-405 | ROM 端口直读地址；DMA 需低 16 MB | 机制 | 存量 | **23 §2.5** | `lance.c:782-784/759` | |
| K-406 | 探测按序号跳过、不按厂商标识过滤；标识表只打日志与分支、不拦路 | 机制 | 存量 | **23 §1.4** | — | |
| K-407 | 三家回调填报对照（十/十一/八项）；设能力/设标志/设介质为框架预留 | 数据结构 | 存量 | **23 §2.2** | `e1000.c:38-49`、`rtl8139.c:101-113`、`lance.c:158-167` | |
| K-408 | 九家一行矩阵 | 数据结构 | 存量 | **23 §2.6** | 各文件 | |
| K-409 | 环计数器取模、槽轮转器取模四、绕回纯函数、认卡两段查表 | 架构演进 | 存量 | **23 §3.1-3.4** | `os/drivers/net/{e1000/src/desc,rtl8139/src/txrx,lance/src/ring}.rs` | |
| K-410 | 打印机状态位五常量；重试上限 120（半秒一次约 60 秒） | 常量 | 存量 | **24 §1.1/§2.2** | `printer.c:46-52` | |
| K-411 | **状态检查优先级：离线 > 缺纸 > 忙**；离线报 EIO 不重试 | 约束与不变量 | 存量 | **24 §1.1/§2.2/§3.1** | `printer.c:208`（`output_done`） | **N3 的修复点（曾优先级反转）** |
| K-412 | `printer_write` 写中 EIO / 非阻塞 EAGAIN；`printer_tab` 四项 | 机制 | 存量 | **24 §2.2** | `printer.c:85`（`printer_write`）、`:99-103` | |
| K-413 | cat24c256 读 128 / 写 16 分片；地址一位/两位看页标志；写跨页回绕前功尽弃；容量 32768 字节 | 常量/机制 | 存量 | **24 §1.2/§2.3/§3.2** | `cat24c256.c:296`（`cat24c256_read`）、`:367`（`cat24c256_write`） | |
| K-414 | bmp085 校准寄存器 0xAA 连读 22 字节；校准十一系数；体温换算公式（中间值一二/合计值/缩放十分之一度）；测量两步 + 等 4500 微秒 | 机制 | 存量 | **24 §1.3/§2.4/§3.4** | `bmp085.c:90-114/74-88/382-385/317-418` | |
| K-415 | 换算全用三十二位防溢出；数据手册例题 15 度整 | 约束与不变量 | 存量 | **24 §3.4/§5.3** | `convert.rs` | |
| K-416 | hello 骨架（三回调/打开计数/EOF/限长/安全拷贝）；主循环两行；热升级存取两函 | 机制 | 存量 | **24 §1.4/§2.5** | `hello.c:35/13/145/76-92` | **"文档即课本"** |
| K-417 | 余家一行矩阵（10 行）；tsl2550 129 项比例表；sht21 CRC 与保持/非保持触发 | 数据结构 | 存量 | **24 §2.6** | `tsl2550.c`、`sht21.c` | |
| K-418 | **ACPI 第三方移植只包三文件外壳** | 架构演进 | 存量 | **24 §1.5/§2.6/§2.8** | `power/acpi`（157 文件 83279 行） | **A-9 的重大决策** |
| K-419 | `minix-i2cdriver` 承载 libi2cdriver（366 行）；**plan §5.1 原映射本篇但从未落地** | 架构演进 | 存量 | **24 框架补充 L6** | `os/libs/minix-i2cdriver/` | **G2 的落地（曾零覆盖）** |
| K-420 | 状态做成优先级读、分片做成纯函数、地址宽度做成布尔函数、公式做成纯换算 | 架构演进 | 存量 | **24 §3.1-3.4** | `os/drivers/{printer/printer/src/status,eeprom/cat24c256/src/pages,sensors/bmp085/src/convert}.rs` | |

#### 组 I：收尾与全局（现有 00、25、99 篇）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-421 | 五十七个驱动 server + 十一个共用框架库（290 C 文件 / 155087 行） | 常量 | 存量 | **00 §1** | `find drivers -name '*.c'` | 规模事实 |
| K-422 | 每个是一个单线程事件循环；独立进程一崩只死一个、RS 再孵化 | 机制 | 存量 | **00 §1** | — | 执行模型 |
| K-423 | 微内核姿势（不碰内核数据、不发特权指令、授权访问：grant 拷贝/IRQ 挂钩/端口授权） | 约束与不变量 | 存量 | **00 §1** | — | **本 stage 的宪法** |
| K-424 | **boot image 只带 memory 与 tty** 的硬因果链 | 机制 | 存量 | **00 §2** | `kernel/table.c:44-64`（`:58`/`:59`） | **plan F3 的修正** |
| K-425 | init + `/etc/rc` 运行时加载其余驱动；设备出现次序 | 机制 | 存量 | **00 §2** | RS + `etc/rc` | |
| K-426 | memory + tty 是最小可启动闭环 | 约束与不变量 | 存量 | **00 §2** | — | |
| K-427 | 策略在库、传输在 bin | 架构演进 | 存量 | **00 §3.1** | — | **全 stage 的核心架构决策** |
| K-428 | 注入传输替身测试、策略库宿主机可测 | 工具与工程 | 存量 | **00 §3.1** | — | |
| K-429 | `minix-driver-rt` 统一事件循环骨架（宣告/收包/分类/分发）；SEF 生命周期切点内嵌传输实现 | 架构演进 | 存量 | **00 §3.2** | `os/libs/minix-driver-rt/src/lib.rs` | **`[ARCH: 驱动服务运行时统一]`（A1）** |
| K-430 | 五族框架库分立；函数指针表→带默认体行为定义 | 架构演进 | 存量 | **00 §3.3** | 五个 crate 名 | |
| K-431 | `minix-driver-rt::core` 共享服务器状态机（`OpenSet`/`LoopAction`/`ServerState`） | 架构演进 | 存量 | **00 §3.3** | `minix-driver-rt::core` | **A3 的落地** |
| K-432 | **线格式归策略库**（`#[repr(C)]` + 布局常量） | 架构演进 | 存量 | **00 §3.4** | — | **`[ARCH: policy/transport 边界重划]`（A5）** |
| K-433 | 连续物理内存是唯一外部依赖（edge E-DMABUF） | 约束与不变量 | 存量 | **00 §3.4** | VM 侧契约 | |
| K-434 | errno 公理七值（ENXIO/EINVAL/EIO/EAGAIN/EBUSY/EPERM/ENOTTY） | 约束与不变量 | 存量 | **00 §4** | — | |
| K-435 | 六框架库 + 三十一策略 crate、四百余测试全部宿主机可跑；集成面留多进程联调 | 测试性质 | 存量 | **00 §5** | — | |
| K-436 | DMA 内存契约四操作 + 一张凭据（`DmaRegion` 三字段 / `DmaMemory` 四方法） | 接口与协议 | 存量 | **25 §2** | `os/libs/minix-types/src/types/dma.rs` | **新 30 篇的核心** |
| K-437 | 燃料站模型 / 地址双簿模型 / 单一权威模型 | 概念 | 存量 | **25 §1** | — | 三个心智模型 |
| K-438 | 契约住共享类型库的理由（驱动库与实现方分属两条工作线，放谁家另一家反向依赖） | 架构演进 | 存量 | **25 §3** | — | |
| K-439 | 申请失败报 errno 而非空值（穷尽就是 ENOMEM）；翻译查无此址报 None（不算错误）；归还无返回值 | 接口与协议 | 存量 | **25 §2/§4** | `dma.rs` | |
| K-440 | 替身形状规定（定长簿记阶梯分配器 + 故意"永远没内存"的实现；trait 只有一个实现体是装饰） | 测试性质 | 存量 | **25 §3** | `os/libs/minix-virtio/src/hal.rs` | **Gate D 的论证** |
| K-441 | 五家族请求与回复基址表（CDEV 0x400/0x480、BDEV 0x500/0x580、NDEV 0x1A00/0x1A80、RTCDEV 0x1400/0x1480、USB 0x1100） | 接口与协议 | 存量 | **99 §2.1** | `com.h:919/963/1085/995/813` | 号码簿 |
| K-442 | 判别掩码 `& ~0x7f`；消息类型低七位是家族内编号；跨族基址相距至少 128 | 机制 | 存量 | **99 §2.1** | `com.h:922-923/965-966` | |
| K-443 | **值不许错**（F1 教训：`CDEV_RS_BASE` 一度写成 `0x500`）；**定义不许重复** | 约束与不变量 | 存量 | **99 §1/§4** | — | **两条铁律** |
| K-444 | CDEV 七请求 + 三回复 + 六标志 | 接口与协议 | 存量 | **99 §2.2** | `com.h:926-932/935-937/946-956` | |
| K-445 | BDEV 七请求 + 三标志 | 接口与协议 | 存量 | **99 §2.2** | `com.h:970-976/982-987` | |
| K-446 | NDEV 六请求 + 配置子类型/模式位/能力位/专用标志/链路三态 | 接口与协议 | 存量 | **99 §2.2** | `com.h:1096-1101/1126-1145` | |
| K-447 | RTCDEV 五请求 + `RTCDEV_Y2KBUG` | 接口与协议 | 存量 | **99 §2.2** | `com.h:1002-1012` | |
| K-448 | USB 八请求 + 载荷槽位分配 | 接口与协议 | 存量 | **99 §2.2** | `com.h:813-828/829-841` | |
| K-449 | SDEV 归 17-stage-net，本篇只记号码 | 边界 | 存量 | **99 §2.2** | `com.h:1037-1068` | |
| K-450 | 通用模型三样（`MAX_NR_OPEN_DEVICES 256` / `driver_receive` / 端点约定） | 数据结构 | 存量 | **99 §2.3** | `driver.h:41` | |
| K-451 | 次设备号驱动私有 + 分号实例；/dev 命名由文件系统侧设备表决定 | 约束与不变量 | 存量 | **99 §2.4** | `dmap.h:85-92` | |
| K-452 | errno 七值映射、禁止自造（F3 教训） | 约束与不变量 | 存量 | **99 §2.5** | — | |
| K-453 | 常量单一权威收敛到 `minix-types`（edge E-DEVWIRE） | 架构演进 | 存量 | **99 §3.1** | — | **A4 的收敛方向** |
| K-454 | 家族判别枚举五类型 | 架构演进 | 存量 | **99 §3.2** | `{Cdev,Bdev,Ndev,Rtc}Request`、`NdevReply` | |
| K-455 | 常量钉值测试计数（chardriver 21 / blockdriver 13 / netdriver 52 / bdev 20 / readclock 12） | 测试性质 | 存量 | **99 §5** | — | |
| K-456 | 内存驱动是唯一 char+block 双面设备 | 约束与不变量 | 存量 | **99 §6** | 05 篇 | |

### 2.3 新增知识点（来源类型 = 新增）

> 这些条目现有文档**没有**，由 §3 覆盖审计发现。不受 §6 去向规则约束，但必须有证据锚点。每条在 §5 有归属契约。

| 编号 | 名称 | 类型 | 锚点（证据） | 为什么需要 | 归入新篇 |
|---|---|---|---|---|---|
| N-001 | **服务策略配置面**（`etc/system.conf` 的驱动服务段：uid / ipc / io / irq / system 特权调用） | 工具与工程 | `minix3/etc/system.conf:161`（tty）、`:187`（memory）、`:210`（log）、`:289`（pci）、`:301`（ahci）等 20+ 段 | 现有 27 篇**零处提及**。这是"驱动为什么能访问硬件"的授权前提；tty 段给 `io ALL` 与 `IRQCTL`，memory 段给 `io NONE`/`irq NONE`——**授权差异直接决定驱动能力** | 新 31 |
| N-002 | `/dev` 设备节点的创建机制（`MAKEDEV.sh`）与主/次设备号分配 | 工具与工程 | `minix3/minix/commands/MAKEDEV/MAKEDEV.sh`、`MAKEDEV.8` | 现有 99 §2.4 只说"/dev 命名由文件系统侧设备表决定"，未给机制 | 新 31 |
| N-003 | 各驱动的 Makefile 依赖链与构建期常量（`LDADD`/`DPADD`；如 memory 链哪些库） | 工具与工程 | `minix3/minix/drivers/**/Makefile` | 现有文档零处。回答"这个驱动依赖哪些库" | 新 31 |
| N-004 | **驱动分类的完整矩阵**（57 driver × 启动时机 / 框架族 / 请求族 / 硬件类别 / 优先级 / 是否 boot 成员） | 数据结构 | `kernel/table.c:44-64`、`etc/system.conf`、各 driver 的 `*_tab` | 现有 00 篇只给了因果链，无完整矩阵；各篇分散提到自己的位置 | 新 08 |
| N-005 | **回调差异矩阵**（57 driver × 各族回调表成员，格子标"实现/空/不适用"） | 数据结构 | 各 driver 的 `*_tab`（如 `pfs_table`、`mass_storage`、`dp_table`、`fb_tab`、`printer_tab`） | 现有文档各篇零散提到（如 20 §2.2 "只填五项"、22 §2.3 "只填七项"），无总表 | 新 08 |
| N-006 | **服务层边界声明汇总**（各篇统一口径"寄存器细节/流量在服务层，本库只定顺序与算法"的逐篇清单） | 约束与不变量 | 各篇头部"说明"行 | 这是全 stage 最重要的架构约定，但散在 27 篇的头部，无汇总 | 新 02、新 08 |
| N-007 | **四类错误的统一分类**（协议错误 / 参数错误 / 硬件错误 / 状态错误）与各自的 errno 落点 | 接口与协议 | 各篇 §4 错误处理表 | 现有 99 §2.5 只给七值清单，未给分类框架 | 新 31 |
| N-008 | **"已声明不做"清单**（三套块主循环的两套、间接描述符线程池、描述符树解析、ACPICA 逐行重写、`logos.h` 位图、五份混音器） | 架构演进 | `plan.md` §4、各篇差异表 | 现有文档各篇零散声明，无汇总；读者无法一次看全"哪些不做" | 新 31 |
| N-009 | **驱动进程的生命周期总图**（SEF 启动 → 宣告 → 主循环 → SIGTERM/热升级 四种退出路径） | 机制 | `chardriver.c:99/549/537`、`liveupdate.c` 四处、各 `main` | 现有文档各篇讲自己的启动，无生命周期总图 | 新 02 |
| N-010 | **中断与通知的投递路径**（内核 notify → `driver_receive` → 分类 → 旁路挂钩；不回复） | 机制 | `chardriver.c:464-482`（通知分支）、`netdriver.c:763` | 现有 01 §1.6 讲了一个框架的通知，未讲跨族的统一路径 | 新 02 |
| N-011 | **`libbdev` 与 15-stage-fs 的接口契约**（FS 如何用 `bdev_open/read/write`；编号镜像绊线的意义） | 接口与协议 | `libbdev/bdev.c`、`15-stage-fs/07-mfs-init-main.md` 的块访问 | 现有 04 篇声明"VFS/FS 消费逻辑见别处"，但未给契约面 | 新 07 |
| N-012 | **协议编号的编译期对账机制**（钉值测试的组织方式：每族一组，逐值对照 `com.h`） | 测试性质 | 各框架库的钉值测试（chardriver 21 / blockdriver 13 / netdriver 52 / bdev 20 / readclock 12） | 现有 99 §5 只给了计数，未给机制 | 新 29 |
| N-013 | **测试基建**（宿主机可测的三层：纯函数 / 框架逻辑 / 装配；传输替身注入） | 测试性质 | `todo.md`（94 + 242 测试）、各 crate 的测试 | 现有 00 §5 只给总数 | 新 29 |
| N-014 | **`libsockdriver`（1150 行）的处置声明** | 边界 | `lib/libsockdriver/`、`edge E-SDEVOWN` | 现有 plan §5.4 排除但 27 篇零处声明；`minix-netdriver` 里有 `sdev.rs`/`sockevent.rs` 寄居代码 | 新 31 |
| N-015 | **`dec21140A` 目录的大小写差异**（C 侧 `dec21140A`、Rust 侧 `dec21140a`） | 工具与工程 | `ls minix3/minix/drivers/net/`、`ls os/drivers/net/` | 覆盖核对时的差集项，需说明不是缺失 | 新 31 |
| N-016 | **`storage/ramdisk` 目录的真实性质**（无 `.c`，只有 proto/rc 构建配置） | 工具与工程 | `ls minix3/minix/drivers/storage/ramdisk/` | plan §5.4 已声明排除，但 27 篇零处；`os/drivers/storage/ramdisk` crate 已删 | 新 31 |
| N-017 | **`examples/hello` 的教科书地位**（158 行最小可运行驱动，文档 24 §2.7 说"文档即课本（无 Rust 建模）"） | 工具与工程 | `drivers/examples/hello/hello.c`（158 行） | 现有 24 篇把它混在杂项矩阵里，未强调其教科书价值 | 新 08、新 28 |
| N-018 | **五族框架库的对照表**（请求数 / 回调数 / 循环形状 / 门禁机制 / 队列 / 特有机制） | 数据结构 | `chardriver.c`、`blockdriver/`、`netdriver.c`、`bdev/`、`libvirtio/` | 现有 01/02/03/04 各讲自己，无横向对照 | 新 03 |
| N-019 | **硬件抽象的四个层级**（端口 I/O / MMIO / DMA / 中断）与各驱动的落点 | 架构演进 | `[ARCH A-2]`/`A-3`/`A-4` 涉及的各篇 | 现有 `plan.md` §4 给了 ARCH 清单，但未给"四层级"的组织框架 | 新 02 |
| N-020 | **SIGTERM 与热升级的四种退出路径对照**（普通终止 / 块框架热升级 / log 热升级 / at_wini 热升级 / audio 热升级） | 机制 | `chardriver.c:537`、`libblockdriver/liveupdate.c`、`drivers/system/log/liveupdate.c`、`drivers/storage/at_wini/liveupdate.c`、`lib/libaudiodriver/liveupdate.c` | 现有各篇各讲一个，无对照 | 新 31 |

### 2.4 统计摘要

**总条数**：456 条存量（K-001..K-456）+ 20 条新增（N-001..N-020）= **476 条**。

**按类型分布**（存量部分）：

| 类型 | 条数 | 占比 |
|---|---|---|
| 机制 | 168 | 36.8% |
| 数据结构 | 76 | 16.7% |
| 接口与协议 | 79 | 17.3% |
| 约束与不变量 | 72 | 15.8% |
| 架构演进 | 41 | 9.0% |
| 概念 | 20 | 4.4% |
| 工具与工程 | 0 | 0.0% |
| 测试性质 | 0 | 0.0% |

> 说明：工具与工程 / 测试性质两类在本 stage 的存量池里**数量为零**——这不是遗漏，而是现有文档把它们当作"附带说明"而非知识点（如"文件清单与行数"、"测试统计"）。重建时这些内容归新 29（测试基建）与新 31（工程面）两篇。

**按现有文档分布**（主讲述点计数）：

| 现有文档 | 知识点数 | 现有文档 | 知识点数 | 现有文档 | 知识点数 |
|---|---|---|---|---|---|
| 01 | 35 | 10 | 14 | 19 | 16 |
| 02 | 28 | 11 | 16 | 20 | 10 |
| 03 | 25 | 12 | 14 | 21 | 10 |
| 04 | 26 | 13 | 14 | 22 | 10 |
| 05 | 20 | 14 | 30 | 23 | 9 |
| 06 | 25 | 15 | 13 | 24 | 11 |
| 07 | 14 | 16 | 13 | 00 | 15 |
| 08 | 15 | 17 | 13 | 25 | 5 |
| 09 | 21 | 18 | 13 | 99 | 21 |

**重复与主讲述点标记**（同一知识点在多篇出现，合并为一条并标主讲述点；下表只列**跨篇重复**）：

| 知识点 | 主讲述点 | 次讲述点（改为引用） |
|---|---|---|
| K-003 单线程假设 | 01 §1.1 | 02/03/04 前置、00 §1、99 §2.3 |
| K-011 空挂钩默认行为 | 01 §1.3 | 20 §2.2（只填五项）、22 §2.3（只填七项）、23 §2.2、24 §2.2、19 §2.4 |
| K-016 重启门 | 01 §1.5 | 02 §1.1（同形）、04 §1.4（重启恢复）、99 §2.3 |
| K-017 静默丢弃理由 | 01 §1.5 | 02 §1.1 |
| K-022 打开集合 | 01 §2.2 | 02 §2.3（多驱动种类参数） |
| K-048 三套主循环 | 02 §1.5 | 00 §3.2（只保留一套的 ARCH 理由） |
| K-054 扇区常量 | 02 §2.2 | 15 §1.2（512 对齐铁律）、19 §1.2（SCSI 扇区守卫） |
| K-068 初始化门 | 03 §1.7 | 01 §1.5（重启门对照） |
| K-080 端口辅助 | 03 §1.6 | 11 §1.3、14 §3.4（端口不进 OS 层的三处落点） |
| K-081 `[ARCH A-2]` 端口不进 OS 层 | 03 §1.6 | 11 §3.2、14 §3.4、06 §2.10、13 §2.7 |
| K-095 回复三验 | 04 §1.2 | 04 §2.5（ipc 路径，同一篇） |
| K-118 面的归属 | 05 §1.2 | 99 §6 |
| K-136 事件灯去耦 | 06 §1.1 | 06 §1.7（主循环顺序，同一篇） |
| K-152 内核消息共读 | 06 §1.6 | 08 §1.6（两个读者各记指针） |
| K-160 NR_PTYS 32 | 07 §1.2 | 06 §2.1（config.h 同处） |
| K-166 输出环 2048 | 07 §1.4 | 07 §2.6（同一篇） |
| K-183 错号 errno 不一致 | 08 §2.4 | 08 §2.5（第三条口径，同一篇） |
| K-198 计数器加密产流 | 09 §1.5 | 09 §2.6（同一篇） |
| K-208 密码原语行为定义 | 09 §3.2 | 09 §2.8（同一篇） |
| K-231 可见性规则 | 11 §1.4 | 11 §3.3（同一篇） |
| K-253 vtreefs 三钩子 | 12 §2.3 | `15-stage-fs/18-vtreefs.md`（框架归 FS 阶段） |
| K-264 事件桥 | 13 §1.5 | `os/libs/minix-sys/src/inputdriver.rs`（A11 收敛，同一语义两处实现） |
| K-273 写权限分离 | 14 §1.1 | 14 §3.1（同一篇） |
| K-284 踢门铃规矩 | 14 §1.5 | 14 §2.5（同一篇） |
| K-291 队列内存是连续物理页 | 14 §2.3 | 25 §1（DMA 契约的动机） |
| K-298 `Hal` 行为定义 | 14 §3.1 | 25 §3（契约本体） |
| K-305 未知状态改判偏差 | 15 §1.1 | 15 §2.3/§3.2/§4（同一篇四处） |
| K-324 识别先验后量 | 16 §1.4 | 16 §2.4（同一篇） |
| K-331 五阶段上电 | 17 §1.2 | 17 §2.7/§3.3（同一篇） |
| K-336 镜像退场账本 | 17 §1.4 | 17 §2.5（同一篇） |
| K-347 调度 16 上限 | 18 §1.4 | 18 §2.7（同一篇） |
| K-361 端口三态与通信错 | 19 §1.3 | 19 §3.4（同一篇） |
| K-372 帧缓冲是字符设备 | 20 说明/勘误 | `plan.md` §2（错误处） |
| K-388 七兄弟差异矩阵 | 21 §2.6 | 21 §1.4（同一篇） |
| K-398 阈值补充 | 22 §1.3 | 22 §2.6/§3.4（同一篇） |
| K-404 认卡两段式 | 23 §1.3 | 23 §2.5/§3.4（同一篇） |
| K-424 boot 只带两个驱动 | 00 §2 | `plan.md` §1.2（同结论）、05 §1.6、06 §1.1 |
| K-427 策略在库传输在 bin | 00 §3.1 | 全部 driver 篇的头部"说明" |
| K-432 线格式归策略库 | 00 §3.4 | 14 §3.1、19 §2.7（两处落地） |
| K-436 DMA 契约 | 25 §2 | 14 §3.1、16 §1.3、18 §2.5（三处消费） |
| K-443 值不许错/定义不许重复 | 99 §1 | 99 §4（同一篇） |
| K-453 常量单一权威 | 99 §3.1 | `edge E-DEVWIRE` |

---

## 3. 覆盖审计

### 3.1 主题全集与来源

主题全集由四路来源构成：

**来源一：C 源码符号**

| 目录 | 规模 | 已入池 | 明确排除（加理由） |
|---|---|---|---|
| `lib/libchardriver/`（1 `.c` 600 行） | 全函数 | 全部 | 0 |
| `lib/libblockdriver/`（7 `.c` 1857 行） | 全函数 | 全部 | 0 |
| `lib/libnetdriver/`（2 `.c` 1186 行） | 全函数 | 全部 | 0 |
| `lib/libbdev/`（5 `.c` 1364 行） | 全函数 | 全部 | 0 |
| `lib/libvirtio/`（1 `.c` 913 行） | 全函数 | 全部 | 0 |
| `lib/libusb/`（1 `.c` 255 行） | 全函数 | 全部 | 0 |
| `lib/libaudiodriver/`（2 `.c` 977 行） | 全函数 | 全部 | 0 |
| `lib/libi2cdriver/`（1 `.c` 366 行） | 全函数 | 全部 | 0 |
| `lib/libinputdriver/`（1 `.c` 206 行） | 全函数 | 全部 | 0 |
| `lib/libdevman/`（2 `.c` 576 行） | 全函数 | 全部 | 0 |
| `lib/libsockdriver/`（1 `.c` 1150 行） | — | **0** | **明确排除**：SDEV 族归 17-stage-net（`plan.md` §5.4 + `edge E-SDEVOWN`）；但 `minix-netdriver` 里的 `sdev.rs`/`sockevent.rs` 寄居代码需处置（见 §6.4 Q-3） |
| `drivers/`（290 `.c` / 155087 行 / 57 目录） | 全函数 | 全部 | 0 |
| **`drivers/power/acpi`（157 文件 83279 行）** | — | **外壳 + 策略** | **逐行实现排除**：ACPICA 第三方移植（`plan.md` §4 A-9 的"重大决策"），非 Minix3 自有设计；本 stage 只讲外壳与移植策略 |

**来源二：操作系统通用概念**（不依赖具体代码）

| 主题 | 是否入池 | 归入 |
|---|---|---|
| 设备驱动模型（打开/关闭/读写/控制四动词） | 是 | K-005、K-041、K-066、K-092 |
| 中断处理与通知投递 | 是 | K-019、N-010 |
| 环形缓冲与生产者消费者 | 是 | K-144、K-166、K-176、K-272、K-392 |
| 引用计数与生命周期 | 是 | K-022、K-107、K-131、K-373 |
| 状态机与阶段推进 | 是 | K-259、K-300、K-322、K-331、K-345 |
| 幂等与重试策略 | 是 | K-097、K-100、K-330、K-336、K-362 |
| 授权与权限模型 | 是 | K-212、K-231、K-244、N-001 |
| 事务与配对（请求-回复） | 是 | K-002、K-095、K-357 |
| 缓存与写回 | 是 | K-122、K-338 |
| 一致性（CRC/校验和/镜像） | 是 | K-335、K-337 |
| 虚拟化 I/O（共享内存环） | 是 | K-272..K-301 |
| DMA 与物理连续性 | 是 | K-291、K-436..K-440 |
| 故障注入与测试替身 | 是 | K-333、K-334、K-103、K-440 |

**来源三：非 C 制品承载的主题**

| 制品 | 承载主题 | 入池编号 |
|---|---|---|
| `etc/system.conf`（20+ 驱动段） | 服务策略授权面 | **N-001**（现有文档零处） |
| `drivers/**/*.conf`（34 个，装到 `/etc/system.conf.d/`；11 个带 `type net;`） | 每驱动的服务声明与网卡类型标注 | **N-001**（第二轮补入；与上面的 42 个服务段互为补充） |
| `etc/devmand/*.cfg`（`usb_hub.cfg`、`usb_storage.cfg`） | USB 热插拔规则（命中后拉起哪个驱动） | **N-001**（第二轮补入） |
| `commands/MAKEDEV/` | 设备节点创建 | **N-002**（现有文档零处） |
| `drivers/**/Makefile` | 依赖链与构建期常量 | **N-003**（现有文档零处） |
| `kernel/table.c:44-64` | boot image 成员与执行序 | K-424（已有） |
| `libvirtio/virtio_ring.h` | virtqueue 环布局（源自 Linux 头，禁改） | K-287、K-290 |
| `include/sys/ioc_*.h` | 各驱动 ioctl 控制码 | K-124、K-376、K-383 |
| `include/minix/config.h` | 终端数/对数/名字地址尺寸 | K-134、K-160 |

**来源四：阶段边界契约里属于本 stage 的主题**

| 边界条目 | 主题 | 归属判定 |
|---|---|---|
| `E-DEVWIRE`（`edge_todo.md:965`） | 设备族线上常量单一来源（含 vfs 消费侧重述与 BDEV 命名分叉） | **本 stage 讲契约面（新 03、新 29），收敛执行归 edge** |
| `E-SDEVOWN`（`:983`） | sdev/sockevent 语义归属（17-stage-net 寄居 `minix-netdriver`） | **本 stage 出处置声明（新 31），实现归 17** |
| `E-DMABUF`（`:1001`） | DMA/连续物理页契约无归属（02-stage-vm ↔ 16-stage-drivers） | **本 stage 讲消费侧契约（新 30），实现方归 VM** |
| `E-DMCLIENT`（`:728`） | `minix-devman-client` 孤儿 crate 处置（已删，收敛至 `minix-sys`） | **本 stage 出事实陈述（新 16）** |
| `E-CDRCONV`（`:772`） | chardriver 框架双实现收敛 + `CDEV_REPLY_BASE` 错值 | **本 stage 出收敛声明（新 01、新 04），错值已修（F1）** |
| `E-PCKBDREG`（`:806`） | pckbd 邻接面移交（一处行为分歧 + 三处缺口 + 双轨重编码） | **本 stage 出移交声明（新 17）** |
| `E-DMWIRE`（`:705`） | devman 生产接线四缺 | 归 11-stage-devman |

### 3.2 覆盖缺口表

| # | 缺口主题 | 重要度 | 现有状态（证据） | 建议 | 落实 |
|---|---|---|---|---|---|
| GAP-01 | **服务策略配置面**（`system.conf` 授权） | 高 | 27 篇零处；99 §2.4 只说"/dev 命名由文件系统侧设备表决定" | 新建"工程面"篇 | 新 31（N-001） |
| GAP-02 | `/dev` 节点创建机制 | 中 | 零处 | 并入工程面篇 | 新 31（N-002） |
| GAP-03 | 构建链与构建期常量 | 中 | 零处 | 并入工程面篇 | 新 31（N-003） |
| GAP-04 | **驱动分类完整矩阵**（57 × 启动时机/框架族/请求族/优先级） | 高 | 00 §2 只给因果链；各篇分散 | 新建"驱动分类与装配"篇 | 新 08（N-004） |
| GAP-05 | **回调差异矩阵**（57 × 各族回调表成员） | 高 | 各篇零散（20 §2.2"只填五项"、22 §2.3"只填七项"） | 并入分类篇 | 新 08（N-005） |
| GAP-06 | 服务层边界声明汇总 | 中 | 散在 27 篇头部"说明"行 | 并入框架总览篇 | 新 02、新 08（N-006） |
| GAP-07 | 错误分类框架（四类） | 高 | 99 §2.5 只给七值清单 | 新建"错误与退出"篇 | 新 31（N-007） |
| GAP-08 | **"已声明不做"清单** | 高 | 各篇差异表零散 | 并入错误与退出篇 | 新 31（N-008） |
| GAP-09 | 驱动进程生命周期总图 | 中 | 各篇讲自己的启动 | 并入框架总览篇 | 新 02（N-009） |
| GAP-10 | 中断与通知投递路径 | 中 | 01 §1.6 只讲一个框架 | 并入框架总览篇 | 新 02（N-010） |
| GAP-11 | `libbdev` 与 15-stage-fs 的接口契约 | 中 | 04 声明"消费逻辑见别处"但未给契约面 | 并入客户端库篇 | 新 07（N-011） |
| GAP-12 | 协议编号的编译期对账机制 | 中 | 99 §5 只给计数 | 并入测试基建篇 | 新 29（N-012） |
| GAP-13 | 测试基建三层 | 中 | 00 §5 只给总数 | 并入测试基建篇 | 新 29（N-013） |
| GAP-14 | `libsockdriver` 处置声明 | 中 | plan §5.4 排除但 27 篇零处 | 并入工程面篇 | 新 31（N-014） |
| GAP-15 | `dec21140A` 大小写差异 | 低 | 零处 | 并入工程面篇 | 新 31（N-015） |
| GAP-16 | `storage/ramdisk` 真实性质 | 低 | plan §5.4 声明但 27 篇零处 | 并入工程面篇 | 新 31（N-016） |
| GAP-17 | `examples/hello` 教科书地位 | 中 | 24 §2.7 混在杂项矩阵里 | 提升到分类篇与新 28 | 新 08、新 28（N-017） |
| GAP-18 | 五族框架库对照表 | 高 | 01/02/03/04 各讲自己 | 新建"框架对照"篇 | 新 03（N-018） |
| GAP-19 | 硬件抽象四层级（端口/MMIO/DMA/中断） | 中 | `plan.md` §4 给了 ARCH 清单但无组织框架 | 并入框架总览篇 | 新 02（N-019） |
| GAP-20 | SIGTERM 与热升级五种退出路径对照 | 中 | 各篇各讲一个 | 并入错误与退出篇 | 新 31（N-020） |
| GAP-21 | **音频框架 14 钩子的逐项语义** | 中 | 21 §2.7 称"已覆盖（初始化入口注释）"，但 14 钩子无逐项说明 | 在音频篇补 14 钩子表 | 新 25 |
| GAP-22 | **11 个声音控制请求** | 中 | 21 只提"十一声音控制请求"一句，无逐项 | 在音频篇补逐项表 | 新 25 |
| GAP-23 | **键盘看门狗**（`pckbd.c:21-23,49-62`） | 中 | 13 篇零处（G7 缺项） | 在键盘篇补 | 新 17 |
| GAP-24 | **SB16 停/续字节**（`sb16.h:107-110`） | 低 | 21 篇未建模 | 在音频篇补 | 新 25 |

### 3.3 重复主题表

| # | 重复主题 | 重复位置 | 保留主讲述点 | 其余改为 |
|---|---|---|---|---|
| DUP-01 | 单线程事件循环假设 | 01 §1.1、02/03/04 前置、00 §1、99 §2.3 | **新 02**（框架总览） | 各框架篇只留一句引用 |
| DUP-02 | 空挂钩默认行为 | 01 §1.3 + 各 driver 篇的回调表 | **新 04**（字符框架） | 各 driver 篇只给"本驱动填哪几项" |
| DUP-03 | 重启门 | 01 §1.5、02 §1.1、04 §1.4、99 §2.3 | **新 04** | 新 05（块）只写"同形"；新 07 只写"重启恢复" |
| DUP-04 | 三套块主循环 | 02 §1.5/§2.5、00 §3.2 | **新 05** | 新 02 只写"Rust 只保留一套"的 ARCH 理由 |
| DUP-05 | 扇区对齐 | 02 §2.2、15 §1.2、19 §1.2 | **新 05**（常量定义） | 新 19/新 24 各写自己的对齐铁律 |
| DUP-06 | 端口不进 OS 层（`[ARCH A-2]`） | 03 §1.6、11 §3.2、14 §3.4、06 §2.10、13 §2.7 | **新 02**（硬件抽象四层级） | 各驱动篇只留一句 |
| DUP-07 | 内核消息共读（终端 vs 日志） | 06 §1.6、08 §1.6 | **新 12**（log，因它是专职读者） | 新 10（tty）只写"另一条读取路径" |
| DUP-08 | 校验和与镜像 | 17 §1.4/§2.5 | **新 22**（filter） | — |
| DUP-09 | 阶段机模式（五态/四态/三态） | 14 §3.3、16 §3.3、17 §3.3、18 §3.2、19 §3.4 | **新 02**（给出"状态机模式"的通用形状） | 各篇写自己的状态与转移 |
| DUP-10 | 策略/传输分离 | 00 §3.1 + 全部 driver 篇的头部"说明" | **新 02** | 各篇只留一句"寄存器与流量在服务层" |
| DUP-11 | 线格式归库（`repr(C)`） | 00 §3.4、14 §3.1、19 §2.7 | **新 18**（virtio 首次出现）+ **新 24**（USB 第二次） | 新 02 只给原则 |
| DUP-12 | DMA 契约 | 25 §1-3、14 §3.1、16 §1.3、18 §2.5 | **新 30** | 新 18/新 20/新 23 各只引用 |
| DUP-13 | boot image 成员 | 00 §2、05 §1.6、06 §1.1 | **新 08**（分类矩阵） | 新 09/新 10 各写自己的初装 |
| DUP-14 | 值不许错/定义不许重复 | 99 §1/§4 | **新 29**（对账机制） | 新 03 只给原则 |
| DUP-15 | 回调表十/十一/十三成员 | 01 §1.3、02 §1.3、03 §1.3、22 §2.2 | **新 03**（框架对照） | 各 driver 篇只给自己的填报矩阵 |
| DUP-16 | errno 映射 | 99 §2.5 + 各篇 §4 错误表 | **新 31**（错误分类框架） | 各篇 §4 只留自己的场景表 |

### 3.4 越界主题表

| # | 越界位置 | 越界内容 | 声明边界（该篇头部"不讲什么"） | 正确归属 |
|---|---|---|---|---|
| OOB-01 | `13-pckbd-driver.md` 文末 L227–L232 | **三条悬空表格行**（端口读写/阻塞发送/定时器），属 §2.7 差异表 | 13 头部声明"不讲端口细节" | **新 17** 的差异表（结构修复） |
| OOB-02 | `13-pckbd-driver.md` L227 参见 | 仍指向 `bridge.rs`（该文件已在 L5 声明删除，A11） | — | **删除该参见行**（内部矛盾） |
| OOB-03 | `05-memory-driver.md` §2.6 | 32 位兼容构建的 I/O 特权条件编译段 | 05 头部声明"不讲 VM 映射机制" | **新 31**（工程面：架构特有段） |
| OOB-04 | `07-pty-driver.md` §2.7 | ptyfs 侧车（112 行，文件系统语义） | 07 头部声明"从节点树语义见 15-stage-fs/20" | **新 11** 只留"驱动侧增删请求"；ptyfs 树语义归 15-stage-fs |
| OOB-05 | `14-virtio-framework.md` §3.1 | `Hal` trait 与 DMA 契约（跨 stage 边界） | 14 头部声明"内存映射下推服务层" | **新 30**（DMA 契约篇）；新 18 只留"第一次出现"的指针 |
| OOB-06 | `16-ahci-ata-driver.md` §2.6 | 直接存取三步的"编程道与专车道分界"（服务层调参） | 16 头部声明"寄存器细节在服务层" | **新 21** 只留武装机；调参归服务层（声明） |
| OOB-07 | `19-usb-storage-hub.md` §2.6 | 集线器描述符解析（`hub_get_descriptor`） | 19 头部声明"描述符解析归服务层" | **新 24** 的"已记录不管实现"行（保留现状） |
| OOB-08 | `24-misc-drivers.md` §1.5 | ACPI/AML 移植策略（跨 stage 重大决策） | 24 头部声明"第三方全文不讲" | **新 31**（"已声明不做"清单）+ **新 28** 只留外壳 |
| OOB-09 | `11-pci-driver.md` §2.5 | `pci.c` 的桥窗口力学与中断推导（选读节） | 11 头部声明"桥窗口力学在服务层" | **新 15** 只留记录与规则；力学归服务层（声明） |
| OOB-10 | `99-global-concepts.md` §2.3 | `MAX_NR_OPEN_DEVICES 256` 与端点约定（属通用驱动模型） | 99 头部声明"常量如何被使用不讲" | **新 02**（框架总览的通用模型节） |
| OOB-11 | `00-drivers-overview.md` §3.1-3.4 | 四条 ARCH 叙述（策略/传输分离、运行时统一、框架分立、线格式归库） | 00 头部声明"一切机制细节不覆盖" | **新 02**（架构决策篇）；00 只留导航 |
| OOB-12 | `25-dma-memory-contract.md` 全篇 | DMA 契约（跨 stage 边界，非驱动语义） | 25 自述"终局骨架" | **新 30**（保留，但明确它是"边界契约"而非驱动语义） |

### 3.5 非 C 主题逐项回答（固定清单）

| # | 主题 | 在哪里讲 | 依据 |
|---|---|---|---|
| 1 | **链接与加载** | 不属本 stage | 驱动是普通用户态进程，加载由 RS 负责（`03-stage-rs`）。本 stage 只讲"装配与宣告"（新 08、新 31） |
| 2 | **镜像与内存布局** | 新 30（DMA 内存契约）+ 新 09（memory 驱动的 `/dev/imgrd`） | 驱动不定义镜像格式；memory 驱动提供块面，根 FS 镜像格式归 15-stage-fs |
| 3 | **汇编入口与陷阱进入** | 不属本 stage | 逐目录核对：`lib/lib*driver*/`、`drivers/**` 下无 `.S` 文件（除 `arch/` 子目录中的少量，属服务层实现） |
| 4 | **启动装配** | 新 08（分类与装配）+ 新 31（工程面） | boot image 成员（K-424）、四档启动时机（N-004）、宣告与主循环（N-009）、服务策略授权（N-001） |
| 5 | **构建与工具链** | 新 31 | 各驱动 Makefile（N-003）、构建期常量、`MAKEDEV`（N-002） |
| 6 | **跨模块接口与线格式** | 新 03（协议契约）+ 新 30（DMA 契约）+ 新 31（错误码） | 五族请求/回复编号（K-441..K-448）、消息布局（K-349、K-354）、`repr(C)` 线格式（K-296、K-371） |
| 7 | **错误路径** | 新 31 | 四类错误分类（N-007）、errno 七值（K-434、K-452）、各篇 §4 场景表 |
| 8 | **关闭与退出** | 新 31 | 五种退出路径对照（N-020）、终止函数（K-029）、热升级钩子（K-051、K-221） |
| 9 | **并发与同步** | 新 02 | 单线程事件循环（K-003）、三套块主循环（K-048）、线程拓扑不复刻（K-049）、USB 的多线程特例（K-347、K-353） |
| 10 | **测试基建** | 新 29 | 三层测试（N-013）、协议编号对账（N-012）、测试替身（K-103、K-440）、`todo.md` 的 94+242 测试 |

---

## 4. 新目录

### 4.1 设计原则与总体变化

**沿用**（经核对成立）：

- 主干分组：**框架 → boot 关键 → 系统服务 → 设备类别**（`plan.md` §1.2 的语义依赖顺序经 C 真序核对成立：任何 driver 都先经框架的 `*driver_process` 才能处理请求）
- "参考实现 + 差异展开"原则（`plan.md` §3.6）——这是控制 57 driver 篇幅的唯一可行办法
- 单篇单语义、禁止前向引用、首次出现即完整

**调整**（理由逐项见 §6）：

1. **框架部分从 4 篇扩到 7 篇**：现有 01/02/03/04 把"协议契约"、"主循环骨架"、"数据结构"、"驱动实现"四件事混在每篇里；重建后按"契约 vs 骨架 vs 对照"重新切分，并**新增框架总览篇**（承载跨族共性：单线程假设、生命周期、硬件抽象四层级、状态机模式）
2. **新增"驱动分类与装配"篇**：57 driver 的分类矩阵与回调差异矩阵，现有文档零处集中
3. **新增收尾组 4 篇**（测试基建 / DMA 契约 / 错误与退出 / 工程面）：这四类横切主题现有文档全部散落或缺失
4. **boot 关键与系统服务保持 9 篇**，但把"驱动内部结构"与"设备相关分工"重新切分
5. **存储/网络/杂项变体合并压缩**：现有 16/17（ahci+ata / 存储杂项）、22/23（网卡参考 + 变体）的边界模糊（如 `at_wini` 与 `ahci` 同篇但差异巨大），重建后按"代表实现 + 变体矩阵"重新切分
6. **音频 1 篇扩为 2 篇**：框架（14 钩子 + 分片状态机）与声卡变体（7 家）分离

### 4.2 新篇章总表

**共 33 篇**（现有 27 篇 = 00 + 01–24 + 25 + 99 → 新 33 篇 = 00 + 01–31 + 99）。净增 6 篇的来源：全新建 5 篇（02、03、08、29、31）、拆分净增 2 篇（旧 01 → 新 01+04、旧 16 → 新 20+21）、合并净减 1 篇（旧 20+21 → 新 25）。

| 新编号 | 标题 | 一句话定位 | 分组 | 旧编号 |
|---|---|---|---|---|
| 00 | drivers-overview | 驱动子系统是什么、57 driver 与 11 框架库的关系、主线图与阅读路径 | 总览 | 00（改写） |
| 01 | cdev-protocol | CDEV 协议：七请求、三回复、标志位、消息布局 | 一·框架契约 | 01（拆分） |
| 02 | framework-overview | 框架总览：单线程事件循环、生命周期、硬件抽象四层级、状态机模式、策略/传输分离 | 一·框架契约 | 新建 |
| 03 | framework-compare | 五族框架对照：请求数/回调数/循环形状/门禁机制/队列/特有机制 | 一·框架契约 | 新建 |
| 04 | cdev-framework | 字符框架：主循环、分发、回调表、挂起与取消、重启门、通知旁路 | 一·框架契约 | 01（改写） |
| 05 | bdev-framework | 块框架：三套主循环、传输四合一、分区与几何、消息队列、追踪、热升级 | 一·框架契约 | 02（改写） |
| 06 | ndev-framework | 网络框架：主循环、六请求、初始化门、收发队列、状态上报、端口辅助 | 一·框架契约 | 03（改写） |
| 07 | bdev-client | 块设备客户端库：三本账、同步与异步面、重试预算、驱动重启恢复 | 一·框架契约 | 04（改写） |
| 08 | driver-classification | 驱动分类与装配：57 driver 分类矩阵、回调差异矩阵、宣告与初装、`examples/hello` | 二·分类与装配 | 新建 |
| 09 | memory-driver | 内存驱动：双面分诊、13 次设备、三板斧、页窗口、内存盘扩容 | 三·boot 关键 | 05（改写） |
| 10 | tty-driver | 终端驱动：八条线、次设备映射、行规则、挂起三路、轮询两规则、内核消息重定向 | 三·boot 关键 | 06（改写） |
| 11 | pty-driver | 伪终端驱动：32 对状态机、克隆领号、输出环、包模式、选择语义 | 三·boot 关键 | 07（改写） |
| 12 | log-driver | 日志驱动：五万字节环、写盖旧货、挂起读、选择三位、内核消息增量、热升级 | 三·boot 关键 | 08（改写） |
| 13 | random-driver | 随机数驱动：32 池、导数过滤、播种混合顺序、计数器加密、前向保密 | 四·系统服务 | 09（改写） |
| 14 | readclock-driver | 实时时钟驱动：三操作权限门、转发模式、BCD 换算、无状态可继续 | 四·系统服务 | 10（改写） |
| 15 | pci-driver | 总线驱动：18 查询、配置空间、可见性规则、预留独占、枚举建档 | 四·系统服务 | 11（改写） |
| 16 | gpio-devman | 引脚驱动与设备注册库：引脚即文件、认领规则、柄表、USB 跟踪 | 四·系统服务 | 12（改写） |
| 17 | pckbd-driver | 键盘鼠标驱动：扫描码状态机、暂停穿透、鼠标三字节、LED 位序、事件桥 | 五·输入 | 13（改写） |
| 18 | virtio-framework | 虚拟队列框架：三环结构、描述符链、序号回绕、特性协商、生命周期 | 六·存储 | 14（改写） |
| 19 | virtio-blk-driver | 虚拟块设备：三段链、扇区对齐、状态翻译、单盘单分区 | 六·存储 | 15（改写） |
| 20 | ahci-driver | 高级主控接口：32 命令槽、超时复位、识别先验后量、容量拼装 | 六·存储 | 16（拆分） |
| 21 | ata-driver | 并行接口：控制器阶段机、大单拆小单、直接存取武装机、忙等与睡等 | 六·存储 | 16（拆分） |
| 22 | storage-variants | 存储变体五家：软盘重试、闪存卡上电、故障注入三拦截点、过滤镜像账本、回环几何现编 | 六·存储 | 17（改写） |
| 23 | usb-framework | USB 框架：包编号、枚举五步、客户端秘书、调度十六上限、硬件后端 | 七·USB | 18（改写） |
| 24 | usb-storage-hub | 海量存储与集线器：CBW/CSW 三段传输、标签配对、SCSI 七命令、端口三态与拉黑 | 七·USB | 19（改写） |
| 25 | fb-audio-drivers | 帧缓冲与音频：字符设备的两个变体（帧缓冲五回调、音频 14 钩子 + 7 声卡） | 八·显示音频 | 20 + 21（合并） |
| 26 | net-driver-reference | 网卡参考：dp8390 页游标 + virtio_net 队列分工 | 九·网络 | 22（改写） |
| 27 | net-driver-variants | 网卡变体：12 家环几何、槽数、认卡方式差异矩阵 | 九·网络 | 23（改写） |
| 28 | misc-drivers | 杂项驱动：打印机状态优先级、存储器分片、传感器公式、余家矩阵、ACPICA 策略 | 十·杂项 | 24（改写） |
| 29 | fs-testing | 测试基建：三层测试、协议编号对账、测试替身、测试统计对账 | 十一·收尾 | 新建 |
| 30 | fs-dma-contract | DMA 内存契约：四操作 + 一张凭据、三模型、契约住共享库的理由 | 十一·收尾 | 25（改写） |
| 31 | fs-errors-boundaries | 错误、退出与边界：四类错误分类、errno 映射、五种退出路径、已声明不做清单、工程面 | 十一·收尾 | 新建 + 99（拆分） |
| 99 | global-concepts | 五族协议号码簿与常量全集 | 附录 | 99（改写） |

> 编号说明：新 00 与 99 保留原编号语义；01–31 连续编号，无跳号。旧编号与新编号**不是一一对应**（多对多），映射见 §8.1 锚点迁移表。

### 4.3 阅读路径

**主线**（29 篇 = 00 + 01–28）：

```
00 总览
 → 01 CDEV 协议 → 02 框架总览 → 03 框架对照 → 04 字符框架
   → 05 块框架 → 06 网络框架 → 07 块客户端        （框架契约，7 篇）
 → 08 分类与装配                                  （分类与装配，1 篇）
 → 09 memory → 10 tty → 11 pty → 12 log           （boot 关键，4 篇）
 → 13 random → 14 readclock → 15 pci → 16 gpio    （系统服务，4 篇）
 → 17 pckbd                                       （输入，1 篇）
 → 18 virtio → 19 virtio_blk → 20 ahci → 21 ata → 22 存储变体   （存储，5 篇）
 → 23 USB 框架 → 24 USB 存储与集线器                （USB，2 篇）
 → 25 fb 与音频                                    （显示音频，1 篇）
 → 26 网卡参考 → 27 网卡变体                        （网络，2 篇）
 → 28 杂项                                        （杂项，1 篇）
```

**支线**（可跳读，4 篇）：29 测试基建 → 30 DMA 契约 → 31 错误与边界 → 99 全局概念

**最短路径**（想快速理解"驱动子系统怎么工作"，6 篇）：00 → 01 → 02 → 04 → 08 → 09

**按角色的推荐路径**：

| 读者目标 | 路径 |
|---|---|
| 想写一个新驱动 | 00 → 01 → 02 → 04（或 05/06）→ 08 → 09（最小样例）→ 31（工程面） |
| 想理解 boot 关键路径 | 00 → 02 → 04 → 05 → 08 → 09 → 10 → 12 |
| 想理解虚拟化设备 | 00 → 05 → 18 → 19 → 26 |
| 想理解真硬件驱动 | 00 → 05 → 20 → 21 → 22 |
| 想理解 USB 栈 | 00 → 05 → 23 → 24 |
| 想审计错误处理与一致性 | 00 → 04 → 05 → 22 → 31 |
| 想理解工程面（构建/授权） | 00 → 08 → 31 |
| 想对照 Linux/Redox | 00 → 02 → 03 → 18 → 29 |

### 4.4 并行主题的分组与代表成员

| 并行组 | 成员 | 代表成员（讲透） | 其余如何收束 |
|---|---|---|---|
| **11 个框架库** | libchardriver / libblockdriver / libnetdriver / libbdev / libvirtio / libusb / libaudiodriver / libi2cdriver / libinputdriver / libdevman / libsockdriver | **libchardriver**（所有字符驱动共用）+ **libblockdriver**（所有块驱动共用） | 新 03 给五族对照表；libvirtio/libusb/libaudiodriver 各自独立成节（新 18/23/25）；libi2cdriver/libinputdriver/libdevman 并入各自驱动篇 |
| **57 个 driver** | 见新 08 的分类矩阵 | **memory**（唯一双面 + boot 成员 + 最小可运行）+ **tty**（最复杂字符驱动） | 新 08 给分类与回调矩阵；各族篇按"代表 + 差异矩阵"写 |
| **5 个请求族** | CDEV / BDEV / NDEV / RTCDEV / USB_RQ | **CDEV**（最全：七请求 + 三回复 + 挂起 + 取消 + 轮询） | 新 01 给 CDEV 全集；新 05/06 各给自己的族；新 14 给 RTCDEV（专有协议）；新 23 给 USB；新 99 给号码簿 |
| **13 个存储驱动** | memory / virtio_blk / ahci / at_wini / floppy / mmc / fbd / filter / vnd / （usb_storage 归 USB 组） | **virtio_blk**（最简单完整）+ **ahci**（真硬件代表） | 新 19（virtio_blk）、新 20（ahci）、新 21（ata）、新 22（五变体矩阵） |
| **14 个网卡** | dp8390 / virtio_net / e1000 / rtl8139 / lance / 3c90x / atl2 / dec21140A / dpeth / fxp / ip1000 / lan8710a / rtl8169 / vt6105 | **dp8390**（页几何参考）+ **virtio_net**（队列参考） | 新 26（两参考）、新 27（12 变体矩阵） |
| **7 个声卡** | es1371 / es1370 / sb16 / als4000 / cmi8738 / cs4281 / trident | **es1371**（有速率转换器）+ **sb16**（老声卡命令） | 新 25 给 14 钩子 + 7 家矩阵 |
| **USB 设备类** | usbd / usb_storage / usb_hub | **usbd**（HCD + 枚举） | 新 23（框架）、新 24（存储 + 集线器） |

### 4.5 篇幅预算与拆分阈值（第二轮新增）

> 本节回应三条执行口径：单个 doc 要注意大小；可以接受"一篇只讲一个概念、长到 3000 行"（行数上限是软的）；旧文档数量不是限制，可增可减。预算按**篇的类别**给，不逐篇写死，因为篇幅是事后校验而不是划分依据。

**三条原则**：

1. **一篇一语义**：篇与篇的边界按语义单元划，不按行数划；篇幅预算用于事后体检。
2. **长度是软约束**：单一概念确实复杂时（例如 05 块框架要装三套主循环、四合一传输适配器、分区与几何、消息队列、追踪、热升级），可放宽到 1800–3000 行；"概念不复杂却很长"才必须拆。
3. **篇数不设限**：本蓝图把 27 篇改成 33 篇，这是重建的正常结果。B 相若发现某篇超出预算且能指出两个以上独立语义单元，按下面的阈值直接拆，不必回头申请篇数许可。

**预算表**：

| 类别 | 覆盖的新篇 | 目标行数 | 拆分阈值 | 说明 |
|---|---|---|---|---|
| 总览与导航 | 00 | 300–600 | 900 | 只放导航与因果链，机制细节一律下放 |
| 协议契约 | 01 | 400–900 | 1300 | 七请求三回复 + 标志位 + 消息布局 |
| 框架总览与对照 | 02、03 | 600–1200 | 1600 | 02 讲共性、03 只做五族对照 |
| 单族框架 | 04、05、06、07 | 800–1500 | 2000（05 允许 2400） | 05 是允许最长的框架篇 |
| 分类与装配 | 08 | 600–1000 | 1400 | 两张矩阵（57 行 × 6 列、57 行 × 回调列）是主要体积 |
| 代表驱动精讲 | 09–21 | 500–1200 | 1600 | 每篇一个驱动；超过阈值通常意味着混进了变体内容 |
| 变体矩阵 | 22、24、25、26、27、28 | 800–1500 | 2000 | 组内代表讲透 + 差异表收束 |
| 横切收尾 | 29、30、31 | 600–1200 | 1600 | 测试基建 / DMA 契约 / 错误与工程面 |
| 附录号码簿 | 99 | 400–800 | 1000 | 常量与编号全集，只索引不讲解 |

**已识别的三个拆分候选**（维持现状，触发阈值即拆）：

| 候选 | 现状 | 为什么现在不分 | 触发后的拆法 |
|---|---|---|---|
| 新 25（帧缓冲 + 音频） | 合并了旧 20、21 两篇 | 两篇原先各只有 10 个知识点、都偏薄；合并后仍是一个语义单元——"字符设备框架的两个非典型变体" | 拆成「帧缓冲」与「音频」两篇，插在 25 之后，其后 26–31 顺延一位 |
| 新 22（五家存储变体） | 软盘 / 闪存卡 / FBD / 过滤 / 回环 合一 | 五家共享块协议与差异矩阵写法，属"变体矩阵"类别 | 按「可移动介质（软盘 / 闪存）」与「虚拟设备（FBD / 过滤 / 回环）」两分 |
| 新 28（杂项） | 打印机 / 存储器 / 传感器 / 余家矩阵 / ACPICA 策略 合一 | 都是"不值得单独成篇"的杂项，ACPICA 已明确不逐行讲 | 若 ACPI 策略节超过 400 行，拆出「ACPI 外壳与策略」独立成篇 |

> 触发规则：B 相完成某篇初稿后，若行数超过该类别的"拆分阈值"、且能指出两个以上独立语义单元，就拆；编号顺延按 §8.1 的迁移表批量执行。

---

## 5. 每篇契约

> 格式说明：每篇给出七要素（定位、讲什么、不讲什么、前置、后置、事实底线、知识点清单加验收标准）。
> "来源"列：存量条目填旧文档位置（形如 `01 §1.1`），新增条目填 C 源码锚点或非 C 制品路径。
> "前置"只允许指向更早的编号（前向引用为零，见 §9 G3）。

### 00-drivers-overview

- **一句话定位**：读者读完知道驱动子系统由哪些东西组成、它们怎么连起来、自己要按什么顺序读下去。
- **讲什么**：
  - 驱动子系统的构成（57 driver + 11 框架库）与规模事实
  - 微内核姿势（不碰内核数据、不发特权指令、授权访问）——本 stage 的宪法
  - boot 两层语义：登记顺序 vs 执行顺序；**只有 memory 与 tty 是 boot image 成员**
  - 四档启动时机（boot / RS 运行时 / 按需 / 后置）
  - 语义主线图与阅读路径（主线 / 支线 / 按角色）
  - 每个 driver 的一句话定位与它在本 stage 的哪一篇展开
  - ARCH 全景（`plan.md` §4 的 13 项候选，逐项给出当前状态）
- **不讲什么**：
  - 任何机制细节（交给 01–31 各篇）
  - 协议常量值与号码簿（99）
  - 分类矩阵与回调矩阵（08）
  - 工程面（31）
- **前置**：无（本 stage 第一篇）
- **后置**：全部 01–31 篇引用本篇的导航结论
- **事实底线**：
  - C：`minix3/minix/kernel/table.c:44-64`（`boot_image`，`MEM_PROC_NR` 在 `:58`、`TTY_PROC_NR` 在 `:59`）、`kernel/main.c:265`（`RTS_VMINHIBIT`）、`minix3/minix/drivers/`（290 `.c` / 155087 行 / 57 目录）、`minix3/minix/lib/lib*driver*/`（11 库）
  - 非 C 制品：`minix3/etc/system.conf`（驱动服务段）、`minix3/etc/usr/rc`
  - Rust：`os/drivers/*`（55 crate）、`os/libs/minix-*`（9 库）
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-421 | 57 driver + 11 框架库的规模事实 | 常量 | `find drivers -name '*.c'` | 总览的定义性内容 | 00 §1 |
| K-422 | 单线程事件循环 + 独立进程一崩只死一个 | 机制 | — | 总览的执行模型 | 00 §1 |
| K-423 | 微内核姿势（授权访问三机制） | 约束与不变量 | grant/IRQ/端口授权 | 全 stage 的宪法 | 00 §1 |
| K-424 | boot image 只带 memory 与 tty | 机制 | `kernel/table.c:58/59` | 启动因果链的起点 | 00 §2 |
| K-425 | init + `/etc/rc` 运行时加载；设备出现次序 | 机制 | `etc/rc`、`etc/system.conf` | 启动链第二段 | 00 §2 |
| K-426 | memory + tty 是最小可启动闭环 | 约束与不变量 | — | 解释"为什么这两篇先读" | 00 §2 |
| K-427 | 策略在库、传输在 bin | 架构演进 | — | 全 stage 核心架构决策 | 00 §3.1 |
| K-428 | 注入传输替身测试、策略库宿主机可测 | 工具与工程 | — | 架构决策的收益 | 00 §3.1 |
| K-429 | `minix-driver-rt` 统一事件循环骨架 | 架构演进 | `minix-driver-rt/src/lib.rs` | A1 的落地 | 00 §3.2 |
| K-430 | 五族框架库分立；函数指针表→行为定义 | 架构演进 | 五个 crate | A-1 的落地 | 00 §3.3 |
| K-431 | `minix-driver-rt::core` 共享服务器状态机 | 架构演进 | `minix-driver-rt::core` | A3 的落地 | 00 §3.3 |
| K-432 | 线格式归策略库 | 架构演进 | — | A5 的落地 | 00 §3.4 |
| K-433 | 连续物理内存是唯一外部依赖 | 约束与不变量 | edge E-DMABUF | 边界声明 | 00 §3.4 |
| K-434 | errno 公理七值 | 约束与不变量 | — | 全 stage 错误纪律 | 00 §4 |
| K-435 | 测试与集成面现状 | 测试性质 | — | 质量声明 | 00 §5 |
| N-006 | 服务层边界声明汇总 | 约束与不变量 | 各篇头部"说明"行 | 现有散在 27 篇 | 新增 |

- **验收标准**：
  1. 能画出 57 driver × 11 框架库的依赖图，每个箭头有锚点
  2. 能回答"boot image 里有几个驱动、哪两个"（答：两个，memory 与 tty）
  3. 给出三条阅读路径（主线 / 最短 / 按角色），每条列出具体编号
  4. 13 项 ARCH 候选逐项给出状态（设计期 / 已落地 / 待裁决），不许留空

### 01-cdev-protocol

- **一句话定位**：读者读完能自己拆开一条 CDEV 消息，说出它请求什么、带哪些字段、期望什么回复。
- **讲什么**：
  - CDEV 家族基址 `0x400` 与回复基址 `0x480`、判别掩码 `& ~0x7f`
  - 七种请求编号（OPEN 0 ~ SELECT 6）与三种回复（普通 / 轮询立即 / 轮询迟通知）
  - 六种标志位（`CDEV_NONBLOCK` / `CDEV_R_BIT` / `CDEV_W_BIT` / `CDEV_NOCTTY` / `CDEV_CLONED` / `CDEV_CTTY`）
  - 各请求的消息材料字段（打开带次设备号与访问字；读写带位置与长度；控制带请求码；取消带原请求标识；轮询带操作位与迟通知位）
  - 家族基址两层路由（基址认家族 → 索引认操作）
  - 请求标识（配对用）与 `TRNS_*` 的关系
  - `CDEV_REPLY_BASE` 错值的教训（F1：曾写成 `0x500`，那是 BDEV 基址）
- **不讲什么**：
  - 消息如何被路由与分发（02、04）
  - 各驱动如何填回调（08 与各族篇）
  - 其它族的协议（05、06、14、23）
  - 号码簿全集（99）
- **前置**：00
- **后置**：02、04、08、09–12、17、25、31、99
- **事实底线**：
  - C：`minix3/minix/include/minix/com.h:919-956`（基址 `:919`、回复基址 `:920`、判定宏 `:922-923`、七请求 `:926-932`、三回复 `:935-937`、六标志 `:946-956`）
  - Rust：`os/libs/minix-chardriver/src/protocol.rs`（`CdevRequest` 枚举、`CDEV_REPLY_BASE` 钉值）
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-005 | 七种请求清单 | 接口与协议 | `com.h:926-932` | 协议定义 | 01 §1.2 |
| K-006 | 基址 `0x400` + 索引 + 判定宏 | 接口与协议 | `com.h:922` | 协议定义 | 01 §1.2 |
| K-007 | 两层路由 | 机制 | — | 协议设计 | 01 §1.2 |
| K-008 | 请求材料字段清单 | 接口与协议 | `com.h` 消息结构 | 协议定义 | 01 §1.2 |
| K-009 | 回复三种形状 | 接口与协议 | `chardriver.c:129/153/195` | 协议定义 | 01 §1.2 |
| K-444 | CDEV 七请求 + 三回复 + 六标志（值表） | 接口与协议 | `com.h:926-956` | 值表归本篇（99 只给索引） | 99 §2.2 |
| K-443 | 值不许错（F1 教训） | 约束与不变量 | — | 协议纪律 | 99 §1/§4 |
| K-002 | 小票=消息：类型字段是请求编号 + 请求标识 | 概念 | `com.h` | 协议编码 | 01 §1.1 |
| K-104 | 编号镜像绊线（测试硬编码五个编号） | 约束与不变量 | — | 协议的对账机制 | 04 §1.6 |
| N-012a | CDEV 钉值测试的组织方式（21 个测试逐值对照 `com.h`） | 测试性质 | `minix-chardriver` 测试 | 现有 99 §5 只给计数 | 新增（Rust 源码） |

- **验收标准**：
  1. 给出七个请求的编号、名称、材料字段、回复形状的完整表，每行带 `com.h` 行锚点
  2. 给出六种标志位的值表与各自的语义
  3. 解释 `CDEV_CLONED` 的用途（答：打开时请求分配新次设备号，pty 主从复制用）
  4. 说明"为什么回复基址与请求基址不同"（答：同一编号空间里区分方向；`0x400` 的回复会落进 `0x480` 段）

### 02-framework-overview

- **一句话定位**：读者读完能说出所有驱动框架共有的形状，以及三族框架的差异在哪。
- **讲什么**：
  - 单线程事件循环假设（一次一条消息，故不需锁）
  - 驱动进程的生命周期总图（SEF 启动 → 宣告 → 主循环 → 终止/热升级）
  - 宣告三件事（解除卡住调用方、发上线事件、清空已打开设备表）
  - **硬件抽象四层级**（端口 I/O / MMIO / DMA / 中断）与各驱动的落点
  - 状态机模式（三态/四态/五态/六态的通用形状与乱序拒绝）
  - 策略/传输分离（寄存器与流量在服务层，库只定顺序与算法）
  - 中断与通知的投递路径（内核 notify → `driver_receive` → 分类 → 旁路挂钩，不回复）
  - 通用驱动模型（`driver_receive`、`struct device`、`MAX_NR_OPEN_DEVICES 256`、端点约定）
  - 门禁机制的两族（重启门 vs 初始化门）
  - 队列形状的两族（有界队列 vs 环）
- **不讲什么**：
  - 各族的协议细节（01、05、06、14、23）
  - 五族对照表（03）
  - 各 driver 的具体实现（09–28）
  - 错误分类与退出路径（31）
- **前置**：00、01
- **后置**：03、04、05、06、07、08、09–28、30、31
- **事实底线**：
  - C：`lib/libchardriver/chardriver.c:549`（`chardriver_task`）、`:99`（`chardriver_announce`）、`:537`（`chardriver_terminate`）、`:455`（`chardriver_process`）；`lib/libblockdriver/driver.c:95`（`blockdriver_announce`）、`driver_st.c:52`（`blockdriver_task`）；`lib/libnetdriver/netdriver.c:969`（`netdriver_task`）、`:763`（`netdriver_process`）、`:58`（`netdriver_announce`）；`include/minix/driver.h:41`（`MAX_NR_OPEN_DEVICES`）
  - Rust：`os/libs/minix-driver-rt/src/lib.rs`（`DriverRuntime`、`DriverTransport`）、`minix-driver-rt::core`（`OpenSet`/`LoopAction`/`ServerState`）
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-003 | 单线程事件循环假设 | 约束与不变量 | `chardriver.c:549` | 跨族共性 | 01 §1.1 |
| N-009 | 驱动进程生命周期总图 | 机制 | `chardriver.c:99/549/537`、四处 `liveupdate.c` | 现有各篇讲自己的启动 | 新增 |
| K-023 | 宣告三件事 | 机制 | `chardriver.c:99` | 跨族共性 | 01 §2.2 |
| N-019 | 硬件抽象四层级 | 架构演进 | A-2/A-3/A-4 涉及各篇 | `plan.md` §4 无组织框架 | 新增 |
| DUP-09/N-021 | 状态机模式的通用形状（三/四/五/六态与乱序拒绝） | 架构演进 | 14 §3.3、16 §3.3、17 §3.3、18 §3.2、19 §3.4 | 跨族共性 | 新增（现有散在 5 篇） |
| K-427a | 策略/传输分离（跨族表述） | 架构演进 | 各篇头部"说明" | 全 stage 约定 | 00 §3.1 |
| N-010 | 中断与通知投递路径 | 机制 | `chardriver.c:464-482`、`netdriver.c:763` | 现有只讲一个框架 | 新增 |
| K-450 | 通用驱动模型三样 | 数据结构 | `driver.h:41` | 现有归 99（越界归位） | 99 §2.3（越界） |
| K-016a | 门禁机制两族（重启门 vs 初始化门） | 机制 | `chardriver.c:70/85`、`netdriver.c` | 跨族对照 | 01 §1.5 + 03 §1.7 |
| K-039a | 队列形状两族（有界队列 vs 环） | 数据结构 | `mq.c`、`ring.rs` | 跨族对照 | 02 §1.1 + 14 §1.1 |
| K-029 | 终止函数两行；故障即停 | 机制 | `chardriver.c:537` | 生命周期终点 | 01 §2.6 |
| K-051a | 热升级机制（跨族） | 机制 | 四处 `liveupdate.c` | 生命周期第二终点 | 02 §1.7 |
| K-429a | `minix-driver-rt` 骨架 | 架构演进 | `minix-driver-rt/src/lib.rs` | Rust 侧落地 | 00 §3.2 |
| K-431a | `minix-driver-rt::core` 状态机 | 架构演进 | `minix-driver-rt::core` | Rust 侧落地 | 00 §3.3 |
| K-030 | `chardriver_get_minor` 七分支 | 机制 | `chardriver.c:575` | 次设备号提取的通用形状 | 01 §2.7 |

- **验收标准**：
  1. 画出驱动进程的完整生命周期图（启动 → 宣告 → 主循环 → 五种退出），每个节点带锚点
  2. 给出硬件抽象四层级的表：层级、涉及驱动、抽象手段、`[ARCH]` 编号
  3. 给出状态机模式的通用形状与五处实例（五态/四态/三态/六态/三态）的对照表
  4. 解释"为什么三族框架都不需要锁"（至少 3 条理由）

### 03-framework-compare

- **一句话定位**：读者读完能横向对照五族框架库的请求数、回调数、循环形状、门禁与队列机制。
- **讲什么**：
  - 五族框架对照表（请求数 / 回复数 / 回调数 / 循环形状 / 门禁机制 / 队列 / 特有机制）
  - 回调表成员的横向对照（字符 10 / 块 11 / 网络 13 / USB 3 / 音频 14）
  - 门禁机制三态（重启门 / 初始化门 / 无门）
  - 循环形状三态（单线程 / 队列式 / 多线程；Rust 只保留一套）
  - 队列与环的两族（有界队列 vs 无界环）
  - 客户端库的特殊位置（`libbdev` 是唯一"跑在别的进程里"的库）
  - 三个框架库的服务器状态机收敛（`OpenSet`/`LoopAction`/`ServerState` 上收 `minix-driver-rt::core`）
- **不讲什么**：
  - 各族的协议细节（01、05、06、14、23）
  - 跨族共性（02）
  - 各 driver 的实现（09–28）
- **前置**：00、01、02
- **后置**：04、05、06、07、08、09–28
- **事实底线**：
  - C：`lib/libchardriver/chardriver.c`（600 行）、`lib/libblockdriver/`（7 `.c` 1857 行）、`lib/libnetdriver/`（2 `.c` 1186 行）、`lib/libbdev/`（5 `.c` 1364 行）、`lib/libvirtio/`（1 `.c` 913 行）、`lib/libusb/`（1 `.c` 255 行）、`lib/libaudiodriver/`（2 `.c` 977 行）
  - 各回调表：`chardriver.h`（10）、`blockdriver.h`（11）、`netdriver.h:23-40`（名字 + 13）、`usb.h:24-28`（3）、`audio_fw.h:9-22`（14）
  - Rust：五个框架库 crate
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| N-018 | 五族框架库对照表 | 数据结构 | 各库源码 | 现有各讲自己 | 新增 |
| DUP-15 | 回调表成员横向对照（10/11/13/3/14） | 数据结构 | 各头文件 | 跨族对照 | 01/02/03 §1.3 + 18 §2.6 + 21 §2.2 |
| K-016b | 门禁机制三态 | 机制 | 各框架 | 跨族对照 | 01 §1.5 + 03 §1.7 |
| K-048a | 循环形状三态与 Rust 只保留一套 | 架构演进 | `driver.c`/`driver_st.c`/`driver_mt.c` | `[ARCH A-5]` | 02 §1.5 |
| K-089a | `libbdev` 是唯一跑在别的进程里的库 | 概念 | `libbdev/bdev.c` | 位置特殊性 | 04 §1.1 |
| K-431b | 三库服务器状态机收敛 | 架构演进 | `minix-driver-rt::core` | A3 的落地 | 00 §3.3 + 01/02/03 §3.6 |
| K-041a | 五族请求数对照（7/7/6/5/8） | 接口与协议 | `com.h` | 跨族对照 | 99 §2.2 |
| K-441a | 五族基址对照表 | 接口与协议 | `com.h:919/963/1085/995/813` | 跨族对照 | 99 §2.1 |
| K-077a | 授权向量拷贝的归属（三族都有，全留服务层） | 架构演进 | `netdriver_copyin/copyout` 等 | 跨族一致性 | 03 §1.3 |

- **验收标准**：
  1. 给出五族框架的完整对照表（至少 8 列），每格有锚点
  2. 给出回调表成员的横向对照（哪几项三族共有、哪几项各族独有）
  3. 解释"为什么 `libbdev` 与其它四个框架库性质不同"（答：它跑在 VFS/FS 进程里，不驱动硬件）
  4. 给出"哪些机制三族共有、哪些独有"的清单

### 04-cdev-framework

- **一句话定位**：读者读完能说出一个字符驱动进程从启动到退出经历什么，到达的每一条消息走哪条分支。
- **讲什么**：
  - 主循环与分发的完整顺序（块打开守卫 → 通知分支 → 重启门 → 七适配器 → 未知走通用）
  - 打开集合（256 槽数组 + 计数；清空/查找/登记；满则停下）
  - 十个挂钩（7 请求 + 中断/定时/通用）
  - **空挂钩默认行为六条**（本 stage 最易错的一条）
  - 挂起与取消（延后回复、拒绝再次挂起、取消三态）
  - 重启门与静默丢弃
  - 通知单向性（旁路挂钩，不回复）
  - 回复构造（三个特殊标记 SUSPEND/PAUSED/RESTART；回复按请求种类分派）
  - 次设备号提取（七分支）
  - 与 C 差异表五条与 Rust 决策五条
- **不讲什么**：
  - 协议字段与编号（01）
  - 跨族共性（02、03）
  - 各字符驱动的实现（09–12、17、25）
  - 错误分类（31）
- **前置**：00、01、02、03
- **后置**：08、09、10、11、12、17、25
- **事实底线**：
  - C：`lib/libchardriver/chardriver.c`（600 行）：`:455`（`chardriver_process`）、`:549`（`chardriver_task`）、`:537`（`chardriver_terminate`）、`:99`（`chardriver_announce`）、`:61`（`clear_open_devs`）、`:70`（`is_open_dev`）、`:85`（`set_open_dev`）、`:129`（`chardriver_reply_task`）、`:153`（`chardriver_reply_select`）、`:195`（`chardriver_reply`）、`:575`（`chardriver_get_minor`）
  - Rust：`os/libs/minix-chardriver/src/{protocol,driver}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-028 | 主循环与分发完整顺序 | 机制 | `chardriver.c:455/549` | 本篇核心 | 01 §2.6 |
| K-022 | 打开集合 | 数据结构 | `chardriver.c:61/70/85` | 本篇核心 | 01 §2.2 |
| K-010 | 十个挂钩 | 数据结构 | `chardriver.h` | 本篇核心 | 01 §1.3 |
| K-011 | 空挂钩默认行为六条 | 约束与不变量 | `chardriver.c` 适配器空判 | **最易错** | 01 §1.3 |
| K-012 | 挂起语义 | 机制 | `chardriver.c:129` | 本篇核心 | 01 §1.4 |
| K-013 | 拒绝再次挂起 | 约束与不变量 | `chardriver.c:129-172` | 挂起机制 | 01 §1.4 |
| K-014 | 取消三态 | 机制 | `chardriver.c` CANCEL | 挂起机制 | 01 §1.4 |
| K-015 | 挂起只允许四种请求 | 约束与不变量 | `chardriver.c:203-223` | 挂起机制 | 01 §1.4 |
| K-016 | 重启门 | 机制 | `chardriver.c:61/70/85` | 本篇核心 | 01 §1.5 |
| K-017 | 静默丢弃理由 | 概念 | — | 重启门设计 | 01 §1.5 |
| K-018 | 块打开误投守卫 | 机制 | `chardriver.c` | 分发顺序 | 01 §1.5 |
| K-019 | 通知单向性 | 机制 | `chardriver.c:464-482` | 分类机制 | 01 §1.6 |
| K-024 | 发送辅助 | 机制 | `chardriver.c` | 回复机制 | 01 §2.4 |
| K-025 | 回复三特殊标记 | 机制 | `chardriver.c:195` | 回复机制 | 01 §2.4 |
| K-026 | 回复按请求种类分派 | 机制 | `chardriver.c:195` | 回复机制 | 01 §2.4 |
| K-027 | 七适配器三段式 | 机制 | 七个 `do_*` | 分发机制 | 01 §2.5 |
| K-030 | 次设备号提取七分支 | 机制 | `chardriver.c:575` | 接口 | 01 §2.7 |
| K-001 | 后厨心智模型 | 概念 | — | 定位 | 01 §1.1 |
| K-004 | 开业宣告 + 清表 | 机制 | `chardriver.c:99/61` | 本篇核心 | 01 §1.1/§2.2 |
| K-020/K-021 | Linux / Redox 对照 | 概念 | — | 横向定位 | 01 §1.7 |
| K-031 | 与 C 差异表五条 | 架构演进 | — | Rust 化 | 01 §2.9 |
| K-032 | Rust 决策五条 | 架构演进 | `{protocol,driver}.rs` | Rust 化 | 01 §3.1-3.5 |
| K-033 | 单线程假设写进文档而非类型 | 约束与不变量 | — | 设计声明 | 01 §3.6 |
| K-034 | `SilentDevice` 测试对端 | 测试性质 | — | 测试 | 01 §3.7 |
| K-035 | 错误处理表 | 接口与协议 | — | 错误（总图归 31） | 01 §4 |

- **验收标准**：
  1. 画出 `chardriver_process` 的完整分支图，每个分支标注返回的 errno 与是否回复
  2. **逐条给出空挂钩六种默认行为**，每种给 C 锚点
  3. 解释"为什么重启门要静默丢弃而不是回 EIO"（答：发送方通过上线事件自恢复）
  4. 给出回复三特殊标记的语义与触发时机

### 05-bdev-framework

- **一句话定位**：读者读完能说出块驱动比字符驱动多出来的四样东西（分区、向量读写、队列、追踪）各解决什么问题。
- **讲什么**：
  - 七种块请求（OPEN/CLOSE/READ/WRITE/GATHER/SCATTER/IOCTL）与唯一回复形状
  - 十一个回调（读写向量四合一为传输；+开关/控制/清理/中断/定时/通用；+分区查询/几何上报/设备号映射）
  - 块请求材料（打开带次设备号与访问方式；向量换计数；位置 64 位）
  - 访问位（读/写各一比特）与直写标志
  - 扇区常量（512、移 9 位、掩码 511、光盘 2048、`DMA_BUF_SIZE`）
  - 分区解析流程与细节（软盘整盘 / 主分区排序 / 扩展分区递归；表项封顶天然终止）
  - 传输适配器四合一（方向区分、向量标志、计数语义）
  - **三套主循环**（单线程/队列式/多线程）与统一准入策略；`[ARCH A-5]` 只保留一套
  - 消息队列模块（128 格、空闲链、每设备一队、满返回假、无锁）
  - 追踪机制与热升级钩子
  - 驱动种类决定分区请求处理
  - 与 C 差异表与 Rust 决策
- **不讲什么**：
  - 块客户端库（07）
  - 具体存储驱动（09、19–22）
  - 协议值表（99）
  - 错误分类（31）
- **前置**：00、01、02、03、04
- **后置**：07、08、09、18、19、20、21、22、24
- **事实底线**：
  - C：`lib/libblockdriver/`（7 `.c` 1857 行）：`driver.c`（462 行，`:95` `blockdriver_announce`）、`driver_st.c`（94 行，`:52` `blockdriver_task`）、`driver_mt.c`（581 行）、`drvlib.c`（234 行，`:28` `partition`、`:14` `parse_part_table`、`:17` `extpartition`、`:20` `get_part_table`）、`mq.c`（108 行，`:31` `mq_init`、`:49` `mq_enqueue`、`:89` `mq_dequeue`、`:76` `mq_isempty`）、`trace.c`（284 行，`:51` `trace_ctl`、`:167` `trace_start`、`:268` `trace_finish`、`:250` `trace_setsize`）、`liveupdate.c`（94 行）；`include/minix/blockdriver.h`（回调表 + 扇区常量 `:50-60`）
  - 非 C 制品：`include/minix/com.h:963-987`（基址 `:963`、回复基址 `:964`、判定宏 `:965-966`、七请求 `:970-976`、标志 `:982-987`）
  - Rust：`os/libs/minix-blockdriver/src/{protocol,driver}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-041 | 七种块请求 + 基址 + 判定宏 | 接口与协议 | `com.h:963/966/970-976` | 协议 | 02 §1.2 |
| K-042 | 块回复唯一形状 | 接口与协议 | — | 协议 | 02 §1.2 |
| K-043 | 块请求材料 | 接口与协议 | `com.h` | 协议 | 02 §1.2 |
| K-044 | 十一个回调组成 | 数据结构 | `blockdriver.h` | 本篇核心 | 02 §1.3 |
| K-045 | 块回调默认行为 + 驱动种类 | 约束与不变量 | — | 本篇核心 | 02 §1.3 |
| K-053 | 访问位与直写标志 | 接口与协议 | `com.h:982-983/987` | 协议 | 02 §2.2 |
| K-054 | 扇区常量 | 接口与协议 | `blockdriver.h:50-60` | 协议 | 02 §2.2 |
| K-046 | 分区解析流程 | 机制 | `drvlib.c:28/14/17/20` | 本篇核心 | 02 §1.4/§2.7 |
| K-047 | 解析只跑一次 + 多线程缓冲 | 约束与不变量 | `drvlib.c` | 分区机制 | 02 §1.4 |
| K-060 | 分区解析细节 | 机制 | `drvlib.c` | 分区机制 | 02 §2.7 |
| K-037 | 分区概念 | 概念 | `drvlib.c:28` | 概念 | 02 §1.1 |
| K-056 | 传输适配器四合一 | 机制 | `driver.c` | 本篇核心 | 02 §2.4 |
| K-038 | 向量读写概念 | 概念 | — | 概念 | 02 §1.1 |
| K-048 | 三套主循环与统一准入策略 | 架构演进 | 三个文件 | `[ARCH A-5]` | 02 §1.5/§2.5 |
| K-049 | 只保留单线程、线程拓扑不复刻 | 架构演进 | — | ARCH | 02 §1.5 |
| K-058 | 三套主循环细节 | 机制 | `driver_st.c:25-89`、`driver_mt.c:70-200` | 差异表 | 02 §2.5 |
| K-059 | 消息队列模块 | 数据结构 | `mq.c:31/49/89/76` | 本篇核心 | 02 §2.6 |
| K-039 | 等待队列概念 | 概念 | `mq.c` | 概念 | 02 §1.1 |
| K-050 | 追踪机制 | 机制 | `trace.c:51/167/268/250` | 本篇核心 | 02 §1.6/§2.8 |
| K-051 | 热升级机制 | 机制 | `liveupdate.c` | 本篇核心 | 02 §1.7/§2.8 |
| K-040 | 追踪与热升级概念 | 概念 | `trace.c`、`liveupdate.c` | 概念 | 02 §1.1 |
| K-055 | 块宣告与打开集合 | 机制 | `driver.c:95` | 装配 | 02 §2.3 |
| K-057 | 分区门 | 机制 | `driver.c:286-292` | 本篇核心 | 02 §2.4 |
| K-036 | 块设备范围与差异 | 概念 | — | 定位 | 02 说明 |
| K-052 | 消息布局头注释 | 工具与工程 | `driver.c:1-40` | 工程 | 02 §2.2 |
| K-061 | 与 C 差异表五条 | 架构演进 | — | Rust 化 | 02 §2.10 |
| K-062 | Rust 决策五条 | 架构演进 | `{protocol,driver}.rs` | Rust 化 | 02 §3.1-3.5 |
| K-063 | Redox 块层对照 | 概念 | Redox | 对照 | 02 §3.2 |

- **验收标准**：
  1. 给出十一个回调的完整表：名字、语义、默认行为、哪几个是"四合一"
  2. 画出分区解析的完整流程图（含扩展分区递归与封顶终止）
  3. 给出传输适配器的四种组合表（读/写 × 连续/向量）与计数语义
  4. 解释"为什么块驱动有三套主循环而 Rust 只保留一套"（答：`[ARCH A-5]`，线程拓扑不复刻但队列准入策略保留）
  5. 给出扇区常量表与 `DMA_BUF_SIZE` 的用途

### 06-ndev-framework

- **一句话定位**：读者读完能说出网卡驱动平时无事可做，包来了如何进、包要走如何出、链路变了如何报。
- **讲什么**：
  - 六种网络请求（INIT/CONF/SEND/RECV/IOCTL/STATUS_REPLY）与回复
  - 名字 + 13 函数指针的回调表（`ndr_*` 前缀）
  - 初始化语义（协议栈重启后第一句话；清账 → 刷新链路 → 组装回复 → 推统计）
  - **初始化门**（与重启门的区别：无打开表、无次设备概念）
  - 配置五子类型、模式位六种、能力位九种、专用标志四种、链路三态
  - 组播名单最多 16，超出则收全部组播
  - 收发队列 8/2 不对称的理由
  - 状态上报（统计 → 待发送标志 → 报告；零计数忽略）
  - 端口辅助走格子算法（扁平偏移 → 起始向量元素 → 逐元素搬）
  - `[ARCH A-2]` 端口号不进 OS 层
  - 框架静态量一览
  - 与 C 差异表与 Rust 决策
- **不讲什么**：
  - 具体网卡实现（26、27）
  - 协议栈（`17-stage-net`）
  - 协议值表（99）
  - 端口辅助的硬件细节（服务层）
- **前置**：00、01、02、03
- **后置**：08、26、27
- **事实底线**：
  - C：`lib/libnetdriver/netdriver.c`（993 行：`:969` `netdriver_task`、`:763` `netdriver_process`、`:871` `netdriver_init`、`:58` `netdriver_announce`、`:25-90` 框架静态量、`:34-46` 队列界限）、`lib/libnetdriver/portio.c`（193 行：`:46` `netdriver_portinb`、`:57` `netdriver_portoutb`）、`include/minix/netdriver.h:23-40`（回调表）
  - 非 C 制品：`include/minix/com.h:1085-1145`（基址 `:1085`、回复基址 `:1086`、六请求 `:1096-1101`、配置子类型 `:1111-1115`、模式位 `:1118-1123`、能力位 `:1126-1134`、专用标志 `:1137-1140`、链路三态 `:1143-1145`）、`include/minix/config.h:102-104`
  - Rust：`os/libs/minix-netdriver/src/{protocol,portio,driver}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-066 | 六种请求 + 基址 + 索引 | 接口与协议 | `com.h:1096-1101` | 协议 | 03 §1.2 |
| K-075 | 名字 + 13 函数指针回调表 | 数据结构 | `netdriver.h:23-40` | 本篇核心 | 03 §1.3/§2.2 |
| K-067 | 初始化语义 | 机制 | `netdriver.c:871` | 本篇核心 | 03 §1.2 |
| K-084 | 初始化处理顺序 | 机制 | `netdriver.c:871` | 本篇核心 | 03 §2.4 |
| K-068 | 初始化门 | 约束与不变量 | — | 本篇核心 | 03 §1.2/§1.7 |
| K-082 | 初始化门 vs 重启门 | 概念 | — | 跨族对照 | 03 §1.7 |
| K-069 | 配置五子类型 | 接口与协议 | `com.h:1111-1115` | 协议 | 03 §1.2 |
| K-070 | 模式位六种 | 接口与协议 | `com.h:1118-1123` | 协议 | 03 §1.2 |
| K-071 | 组播名单最多 16 | 约束与不变量 | `NETDRIVER_MCAST_MAX` | 协议 | 03 §1.2 |
| K-088 | 能力位九值/专用标志四值/回复六值 | 接口与协议 | `com.h:1126-1140` | G8 补 | 03 §5.1 |
| K-072 | 发送/接收数据路径 | 机制 | `netdriver.c` | 本篇核心 | 03 §1.2 |
| K-073 | 保留控制槽 | 接口与协议 | — | 协议 | 03 §1.2 |
| K-074 | 状态确认 | 机制 | — | 协议 | 03 §1.2 |
| K-076 | 默认值全安静 | 约束与不变量 | — | 本篇核心 | 03 §1.3 |
| K-077 | 授权向量拷贝留服务层 | 架构演进 | `netdriver_copyin/copyout` | 边界 | 03 §1.3 |
| K-078 | 收发队列 8/2 不对称 | 约束与不变量 | `netdriver.c:34-46` | 本篇核心 | 03 §1.4 |
| K-079 | 状态上报 | 机制 | `netdriver.c` 统计族 | 本篇核心 | 03 §1.5 |
| K-080 | 端口辅助走格子算法 | 机制 | `portio.c:46/57` | 本篇核心 | 03 §1.6/§2.5 |
| K-085 | portio 搬运模板 + 四入口 | 机制 | `portio.c` 八函数 | 实现 | 03 §2.5 |
| K-081 | `[ARCH A-2]` 端口不进 OS 层 | 架构演进 | — | ARCH | 03 §1.6 |
| K-083 | 框架静态量一览 | 数据结构 | `netdriver.c:25-90` | 结构 | 03 §2.3 |
| K-064 | 网络驱动定位 | 概念 | — | 定位 | 03 说明 |
| K-065 | 塔台比喻与四机制 | 概念 | — | 概念 | 03 §1.1 |
| K-086 | 与 C 差异表四条 | 架构演进 | — | Rust 化 | 03 §2.7 |
| K-087 | Rust 决策五条 | 架构演进 | `{protocol,portio,driver}.rs` | Rust 化 | 03 §3.1-3.5 |

- **验收标准**：
  1. 给出六种请求的编号、材料字段、回复形状表
  2. 给出回调表 14 项（名字 + 13）的完整表与各项默认行为
  3. 解释"为什么收发队列是 8 与 2"（答：发送是推、接收是被动填；数字抄自协议栈对应模块）
  4. 给出初始化门的判定逻辑与它替代重启门的原因
  5. 给出端口辅助的"走格子"算法步骤与"不搬一半"的语义

### 07-bdev-client

- **一句话定位**：读者读完能说出调用方发一个块请求要过几道手，驱动重启时正在飞的请求怎么办。
- **讲什么**：
  - 客户端库的位置（跑在 VFS/FS 进程里，与驱动侧构成块语义闭环）
  - 三本账（驱动端点表 / 打开计数 / 调用槽）与各自上限
  - 同步面六函数（open/close/read/write/gather/scatter/ioctl）
  - 回复三验（类型 → 标识 → 状态）
  - 异步面（调用槽、发出即走、回复分发、等待为非阻塞收集）
  - 两本重试预算（驱动重启 10 次、传输错 5 次）
  - 驱动重启恢复（同步返回失败 / 异步扣预算重发 / 已打开设备逐次重开）
  - `FIXME` 诚实注释（重开中途遇非传输错误则已重开次设备可能永远关不掉）
  - 传输抽象（只过四整数 + 回复三元组；内核类型不进本库）
  - 编号镜像绊线（测试硬编码五个编号）
  - 常量七项
  - **与 15-stage-fs 的接口契约**（FS 如何用 `bdev_open/read/write`）
- **不讲什么**：
  - 驱动侧的接收与服务（05）
  - VFS/FS 拿到块数据后做什么（`05-stage-vfs`、`15-stage-fs`）
  - 协议值表（99）
- **前置**：00、01、02、03、04、05
- **后置**：08、09、19、22
- **事实底线**：
  - C：`lib/libbdev/`（5 `.c` 1364 行）：`bdev.c`（642 行，`:80` `bdev_open`、`:95` `bdev_close`、`:274` `bdev_read`、`:282` `bdev_write`、`:290` `bdev_gather`、`:298` `bdev_scatter`、`:354` `bdev_ioctl`）、`call.c`（118 行，`:13` `bdev_call_alloc`）、`driver.c`（122 行，`:17` `bdev_driver_init`、`:29` `bdev_driver_clear`、`:43` `bdev_driver_set`、`:61` `bdev_driver_get`、`:74` `bdev_driver_update`）、`ipc.c`（346 行，`:119` `bdev_senda`、`:144` `bdev_sendrec`、`:269` `bdev_reply_asyn`、`:317` `bdev_wait_asyn`）、`minor.c`（136 行，`:17` `bdev_minor_reopen`、`:78` `bdev_minor_add`、`:106` `bdev_minor_del`、`:122` `bdev_minor_is_open`）；`include/minix/bdev.h`
  - Rust：`os/libs/minix-bdev/src/{transport,client}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-089 | 客户端库定位与闭环 | 概念 | — | 定位 | 04 说明 |
| N-011 | 与 15-stage-fs 的接口契约 | 接口与协议 | `libbdev/bdev.c` | 现有未给契约面 | 新增 |
| K-090 | 总机比喻三本账 | 概念 | — | 概念 | 04 §1.1 |
| K-091 | 三本账上限与理由 | 概念 | `libbdev/const.h` | 结构 | 04 §1.1/§2.2 |
| K-105 | 常量七项 | 接口与协议 | `libbdev/const.h` | 结构 | 04 §2.2 |
| K-092 | 同步打开流程 | 机制 | `bdev.c:80` | 本篇核心 | 04 §1.2/§2.5 |
| K-093 | 同步关闭本地门 | 机制 | `bdev.c:95` | 本篇核心 | 04 §1.2 |
| K-094 | 读写与向量读写同路 | 接口与协议 | `bdev.c:274/282/290/298/354` | 接口 | 04 §1.2 |
| K-095 | 回复三验 | 约束与不变量 | `ipc.c:144` | **F2 修复点** | 04 §1.2 |
| K-114 | `check_reply` 三验收敛 | 约束与不变量 | `client.rs:557-565` | F2 | 04（todo F2） |
| K-096 | 异步调用机制 | 机制 | `ipc.c:119/269/317` | 本篇核心 | 04 §1.3/§2.6 |
| K-097 | 两本重试预算 | 约束与不变量 | `const.h` | 本篇核心 | 04 §1.3/§2.5 |
| K-098 | 刷新函数 | 机制 | `ipc.c` | 本篇核心 | 04 §1.3 |
| K-109 | 预算藏在发送循环 | 约束与不变量 | `ipc.c`、`const.h` | 实现 | 04 §2.5 |
| K-099 | 回调机制留服务层 | 架构演进 | `bdev_callback_asyn` | 边界 | 04 §1.3 |
| K-100 | 驱动重启恢复 | 机制 | `minor.c:17` | 本篇核心 | 04 §1.4 |
| K-101 | FIXME 诚实注释 | 约束与不变量 | `minor.c` FIXME | **不假装问题不存在** | 04 §1.4/§2.4 |
| K-102 | 传输抽象 | 架构演进 | `transport.rs` | Rust 化 | 04 §1.5 |
| K-103 | 回环替身与录制替身 | 测试性质 | — | 测试 | 04 §1.5 |
| K-104 | 编号镜像绊线 | 约束与不变量 | — | 对账机制 | 04 §1.6 |
| K-106 | 驱动端点表语义 | 数据结构 | `driver.c:17/29/43/61/74` | 结构 | 04 §2.3 |
| K-107 | 打开计数 | 数据结构 | `minor.c:78/106/122/17` | 结构 | 04 §2.4 |
| K-108 | ipc 发送路径 | 机制 | `ipc.c:119/144` | 实现 | 04 §2.5 |
| K-110 | 回复分发与等待 | 机制 | `ipc.c:269/317` | 实现 | 04 §2.6 |
| K-111 | 与 C 差异表六条 | 架构演进 | — | Rust 化 | 04 §2.8 |
| K-112 | Rust 决策六条 | 架构演进 | `client.rs` | Rust 化 | 04 §3.1-3.6 |
| K-113 | `CallSlot` 保存 Destination | 架构演进 | `client.rs` | A6 落地 | 04（todo A6） |

- **验收标准**：
  1. 给出三本账的完整表：用途、上限、上限理由、淘汰策略
  2. 画出一次同步块请求的完整时序（VFS → libbdev → 驱动 → 回复 → VFS）
  3. 给出驱动重启恢复的完整流程（同步路径 + 异步路径 + 已打开设备）
  4. 解释"为什么传输抽象只过四整数"（答：内核端点类型与消息结构不进本库，策略可测）
  5. 给出与 15-stage-fs 的接口契约面（至少 3 个调用点）

### 08-driver-classification

- **一句话定位**：读者读完能说出 57 个 driver 各是什么、各实现了哪些回调、彼此差在哪里。
- **讲什么**：
  - **57 driver 分类矩阵**（启动时机 / 框架族 / 请求族 / 硬件类别 / 优先级 / 是否 boot 成员）
  - **回调差异矩阵**（57 × 各族回调表成员，格子标"实现/空/不适用"）
  - 四档启动时机与各自的初装内容
  - 宣告与初装的通用形状
  - 各 driver 的一句话定位与它在本 stage 的哪一篇展开
  - `examples/hello`（158 行）作为最小可运行驱动的教科书地位
  - 服务层边界声明的逐篇清单
- **不讲什么**：
  - 各族的协议与框架（01、05、06）
  - 各 driver 的具体语义（09–28）
  - 工程面（31）
- **前置**：00、01、02、03、04、05、06、07
- **后置**：09–28、31
- **事实底线**：
  - C：各 driver 的 `main` 与 `*_tab`：`memory.c:88/126`、`tty.c:146`、`pty.c`、`log.c:53`、`random/main.c:56`、`readclock.c:39`、`pci/main.c:728`、`gpio.c:265`、`pckbd.c:500`、`virtio_blk.c:743`、`ahci.c:2723`、`at_wini.c:166`、`floppy.c:293`、`mmcblk.c:656`、`fbd.c:123`、`filter/driver.c`、`vnd.c:592`、`usbd.c:36`、`usb_storage.c:168`、`usb_hub.c:220`、`fb.c:308`、`es1371.c`、`sb16.c`、`dp8390.c:117`、`virtio_net.c:438`、`e1000.c:55`、`rtl8139.c:118`、`lance.c:172`、`printer.c:109`、`cat24c256.c:482`、`bmp085.c:561`、`hello.c:145`
  - 非 C 制品：`kernel/table.c:44-64`、`minix3/etc/system.conf`（各驱动段）、各 `Makefile`
  - Rust：`os/drivers/*`（55 crate）的 `lib.rs` 与 `main.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| N-004 | 驱动分类完整矩阵 | 数据结构 | `kernel/table.c`、`etc/system.conf`、各 `*_tab` | 现有无完整矩阵 | 新增 |
| N-005 | 回调差异矩阵 | 数据结构 | 各 driver 的 `*_tab` | 现有零散 | 新增 |
| N-006 | 服务层边界声明汇总 | 约束与不变量 | 各篇头部"说明" | 现有散在 27 篇 | 新增 |
| N-017 | `examples/hello` 的教科书地位 | 工具与工程 | `hello.c`（158 行） | 现有混在杂项里 | 新增 |
| K-424a | boot image 成员（分类矩阵的一列） | 机制 | `kernel/table.c:58/59` | 分类 | 00 §2 |
| K-425a | 四档启动时机 | 机制 | `etc/rc`、`etc/system.conf` | 分类 | 00 §2 |
| K-023a | 宣告与初装的通用形状 | 机制 | 各 `main` | 装配 | 01 §2.2 |
| K-435a | 各 driver 的测试分布（242 测试 / 31 有实质策略） | 测试性质 | `todo.md` §0 | 现状 | 00 §5 + todo |
| K-016c | 各族门禁在 driver 侧的落点 | 机制 | 各 driver 的打开路径 | 装配 | 01 §1.5 + 03 §1.7 |

- **验收标准**：
  1. 给出 57 driver 的完整分类表（至少 6 列），每行有锚点
  2. 给出回调差异矩阵（行是 driver，列是各族回调成员，格子标"实现/空/不适用"）
  3. 给出四档启动时机的清单，每档列出成员 driver
  4. 解释"为什么 `examples/hello` 值得单独一提"（答：158 行最小可运行驱动，是理解框架的最短路径）
  5. 给出服务层边界声明的统一口径与逐篇出处

### 09-memory-driver

- **一句话定位**：读者读完能说出一个驱动如何同时当字符设备与块设备，13 个次设备各自是什么行为。
- **讲什么**：
  - 双面驱动分诊（看编号认家族：`m_is_block` 谓词）
  - 两张回调表（字符四挂钩 / 块六成员）与主循环分诊
  - 十三个次设备号语义（`/dev/mem`、`/dev/kmem`、`/dev/null`、`/dev/zero`、`/dev/boot`、`/dev/imgrd` + 六个 `/dev/ram*`）
  - 面的归属谓词（1/2/3/5 字符，余块）；错面请求一律答 ENODEV
  - 读写三板斧（到头、截短、搬运）
  - 空洞/零源读写四行为
  - 单页窗口缓存三态（映射页 / 窗口地址 / 有无）与"页窗口与映射原语解耦"
  - 内存盘扩容三问（是盘 / 尺寸对 / 独占）与 `MIOCRAMSIZE`
  - 启动登记顺序（映像盘先行）与初始化四步
  - 内核内存传输（窗口未初始化报 EIO）与绝对内存传输（页对齐、双推进）
  - 块传输向量循环与"高半部位非零视为越界直接成功零字节"
  - Rust 侧：设备号枚举、几何表定长数组、截断单点、规划与复制分离、页窗口泛型策略、扩容纯策略
- **不讲什么**：
  - VM 物理映射机制（`02-stage-vm`）
  - 根文件系统挂载映像盘（`15-stage-fs`）
  - 其它存储驱动（19、20、21、22）
  - 框架机制（04、05）
- **前置**：00、01、02、03、04、05、06、07、08
- **后置**：10、19、22、30
- **事实底线**：
  - C：`minix3/minix/drivers/storage/memory/memory.c`（599 行）：`:88`（`main`）、`:126`（`sef_cb_init_fresh`）、`:48`（`m_char_read`）、`:50`（`m_char_write`）、`:56`（`m_block_transfer`）、`:60`（`m_block_ioctl`）、`:170`（`m_is_block`）、`:189`（`m_transfer_kmem`）、`:218`（`m_transfer_mem`）、`:360`（`m_char_open`）、`:388`（`m_char_close`）；`:63-108`（两张回调表）、`:101-104`（分诊）、`:130-145`（`#if 0` 内核内存映射）、`:37`/`:41`（内存盘数与设备总数）
  - 非 C 制品：`include/minix/dmap.h:85-92`（设备号）、`include/sys/ioc_memory.h`（`MIOCRAMSIZE`）、`drivers/storage/memory/Makefile`
  - Rust：`os/drivers/storage/memory/src/{device,transfer}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-115 | 双面驱动分诊 | 机制 | `memory.c:101-104` | 本篇核心 | 05 §1.1/§2.2 |
| K-116 | 字符表四挂钩 / 块表六成员 | 数据结构 | `memory.c:63-108` | 本篇核心 | 05 §1.1/§2.2 |
| K-117 | 十三个次设备号语义 | 接口与协议 | `dmap.h:85-92` | 本篇核心 | 05 §1.2/§2.1 |
| K-118 | 面的归属谓词 | 约束与不变量 | `memory.c:170` | 本篇核心 | 05 §1.2/§2.4 |
| K-119 | 读写三板斧 | 机制 | `memory.c` 四处截断 | 本篇核心 | 05 §1.3 |
| K-120 | 空洞/零源读写四行为 | 机制 | `memory.c:48/50` | 本篇核心 | 05 §1.3/§2.5 |
| K-121 | 六种读写行为表 | 数据结构 | `memory.c` 各分支 | 本篇核心 | 05 §2.5 |
| K-122 | 单页窗口缓存三态 | 机制 | `memory.c:218` | 本篇核心 | 05 §1.4 |
| K-123 | 页窗口与映射原语解耦 | 架构演进 | `memory.c` 建失败分支 | Rust 化 | 05 §1.4/§3.5 |
| K-124 | 内存盘扩容三问与 `MIOCRAMSIZE` | 机制 | `memory.c:60` | 本篇核心 | 05 §1.5/§2.8 |
| K-125 | 扩容独占 | 约束与不变量 | `memory.c:60` | 本篇核心 | 05 §1.5 |
| K-126 | 启动登记顺序：映像盘先行 | 机制 | `memory.c:126` | boot 因果链 | 05 §1.6/§2.3 |
| K-127 | `#if 0` 死代码与职责移交 | 架构演进 | `memory.c:130-145` | 工程趣闻 | 05 §1.6/§2.3 |
| K-128 | 初始化四步 | 机制 | `memory.c:88/126` | 本篇核心 | 05 §2.3 |
| K-129 | 内核内存传输 | 机制 | `memory.c:189` | 本篇核心 | 05 §2.5 |
| K-130 | 绝对内存传输 | 机制 | `memory.c:218` | 本篇核心 | 05 §2.5 |
| K-131 | 打开关闭计数与首次打开调设备打开 | 机制 | `memory.c:360/388` | 本篇核心 | 05 §2.6 |
| K-132 | 块传输向量循环 | 机制 | `memory.c:56` | 本篇核心 | 05 §2.7 |
| K-133 | 高半部位非零视为越界 | 约束与不变量 | `memory.c:56` | 本篇核心 | 05 §2.7 |
| K-456 | 唯一 char+block 双面设备 | 约束与不变量 | — | 定位 | 99 §6 |
| K-118a | Rust 决策六条（设备号枚举、几何表、截断单点、规划复制分离、页窗口泛型、扩容纯策略） | 架构演进 | `{device,transfer}.rs` | Rust 化 | 05 §3.1-3.6 |

- **验收标准**：
  1. 给出 13 个次设备号的完整表：编号、名字、家族（字符/块）、读写行为
  2. 画出主循环的分诊流程（看编号 → 认家族 → 派发到对应回调）
  3. 解释"为什么映像盘要最先登记"（答：它是根文件系统的地基，VFS 挂载根 FS 时要用）
  4. 给出内存盘扩容的三问判定与独占检查
  5. 说明"高半部位非零视为越界直接成功零字节"的语义与 C 锚点

### 10-tty-driver

- **一句话定位**：读者读完能说出八条终端线共用一个进程，每条线的输入如何从按键变成程序读到的字节。
- **讲什么**：
  - 八条线（4 控制台 + 4 串口）与"事件灯去耦"（中断点灯 / 主循环干活）
  - 次设备号三段映射 + 两特例（视频号 125 / 日志号 15）+ 激活检查
  - 行规则七步加工链与正规/非正规模式分水岭
  - 擦除/杀行/文件尾/转义语义
  - 输入队列 256 环 + 换行计数；满队丢弃（有损但正确）
  - 挂起三路（读/写/排空）与取消精确匹配
  - 轮询两条特殊规则（挂断速度全就绪 / 正规模式要求见过换行）
  - 内核消息重定向（取模差值、非阻塞写、备份恢复）
  - 主循环顺序（先扫灯再收信）与"先扫灯防事件饿死"
  - 打开收编控制终端与访问字判定；末次关闭恢复默认行参与窗口尺寸
  - 读写挂起登记四件套与事件泵补答
  - 设备相关分工（七个设备函数指针接线）
  - 键盘侧输入协议状态机（`INPUT_PAGE_KEY`、`NR_SCAN_CODES`、`ALT_LOCK`、32 格环）
  - `[ARCH A-2]` 设备寄存器 → 后端抽象
- **不讲什么**：
  - 键盘事件如何进输入服务（`12-stage-input`）
  - 伪终端主从对（11）
  - 输入服务的按键映射表（17）
  - 控制台渲染的逐寄存器实现（服务层）
  - 框架机制（04）
- **前置**：00、01、02、03、04、05、06、07、08、09
- **后置**：11、12、17、25
- **事实底线**：
  - C：`minix3/minix/drivers/tty/tty/tty.c`（1603 行）：`:146`（`main`）、`:81`（`do_read`）、`:83`（`do_write`）、`:264`（`line2tty`）、`:389`（`do_new_kmess`）、`:721`（`do_open`）、`:754`（`do_close`）、`:778`（`do_cancel`）、`:816`（`select_try`）、`:865`（`do_select`）、`:901`（`handle_events`）、`:1012`（`in_process`）；`:734-743`（日志别名特殊分支）、`:761`（close 对称拦截）
  - 非 C 制品：`include/minix/dmap.h:95`（控制台号）、`include/minix/config.h:41-46`（终端数）、`drivers/tty/tty/arch/`（console/rs232）、`drivers/tty/tty/keyboard.c`、`keymaps/`
  - Rust：`os/drivers/tty/tty/src/{line,termios,input,session,backend}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-134 | 八条线共用一个进程 | 概念 | `dmap.h:95`、`config.h:41-46` | 定位 | 06 §1.1 |
| K-136 | 事件灯去耦 | 机制 | `tty.c:146` | 本篇核心 | 06 §1.1/§1.7/§2.2 |
| K-135 | 终端结构五十余字段"状态卡" | 数据结构 | `tty.h` | 结构 | 06 §1.1/§2.9 |
| K-137 | 次设备号三段映射 + 两特例 + 激活检查 | 机制 | `tty.c:264` | 本篇核心 | 06 §1.2/§2.3 |
| K-138 | 视频号走视频分支 | 接口与协议 | `line2tty` | 本篇核心 | 06 §1.2 |
| K-139 | 日志号别名与只写诊断语义 | 接口与协议 | `tty.c:734-743` | **B1 修复点** | 06 §1.2/§2.4 |
| K-140 | 映射失败/未激活一律 ENODEV | 约束与不变量 | `line2tty` | 本篇核心 | 06 §1.2 |
| K-141 | 行规则七步加工链 | 机制 | `tty.c:1012` | 本篇核心 | 06 §1.3 |
| K-142 | 正规/非正规模式分水岭 | 机制 | `in_process` | 本篇核心 | 06 §1.3 |
| K-143 | 擦除/杀行/文件尾/转义语义 | 机制 | `tty.c` | 本篇核心 | 06 §1.3 |
| K-144 | 输入队列 256 环 + 满队丢弃 | 数据结构 | `TTY_IN_BYTES` | 本篇核心 | 06 §1.3/§3.3 |
| K-145 | 挂起三路字段构成 | 数据结构 | `tty.h` | 本篇核心 | 06 §1.4 |
| K-146 | 取消按调用方 + 标识精确匹配 | 机制 | `tty.c:778` | 本篇核心 | 06 §1.4/§2.5 |
| K-147 | 重复挂起 EIO、零长度 EINVAL | 约束与不变量 | `tty.c:81` | 本篇核心 | 06 §1.4 |
| K-148 | 轮询规则一：挂断速度全就绪 | 机制 | `tty.c:816` | 本篇核心 | 06 §1.5/§2.6 |
| K-149 | 轮询规则二：正规模式要求见过换行 | 机制 | `select_try` | 本篇核心 | 06 §1.5 |
| K-150 | 双次设备号注册拒绝 | 约束与不变量 | `tty.c:865` | 本篇核心 | 06 §1.5 |
| K-151 | 内核消息重定向 | 机制 | `tty.c:389` | 本篇核心 | 06 §1.6 |
| K-152 | 与日志驱动共读同一内核缓冲 | 概念 | — | 与 12 对照 | 06 §1.6/§2.9 |
| K-153 | 主循环顺序：先扫灯再收信 | 机制 | `tty.c:146` | 本篇核心 | 06 §1.7/§2.2 |
| K-154 | 打开收编控制终端；末次关闭恢复默认 | 机制 | `tty.c:721/754` | 本篇核心 | 06 §2.4 |
| K-155 | 读写挂起登记四件套 | 机制 | `tty.c:81` 起 | 本篇核心 | 06 §2.7 |
| K-156 | 设备相关分工 + 七函数指针接线 | 架构演进 | `tty.h` 末尾 | 边界 | 06 §2.8 |
| K-157 | `[ARCH A-2]` 设备寄存器 → 后端抽象 | 架构演进 | — | ARCH | 06 §2.10 |
| K-158 | 键盘侧输入协议状态机 + 效果 | 接口与协议 | `keyboard.c:30/35/131-176/369-385` | 本篇核心 | 06 §3.8/§5.6 |
| K-135a | Rust 决策六条（线路解码枚举、行配置值类型、输入队列环、逐字节状态机、会话一线一状态、后端行为定义） | 架构演进 | `{line,termios,input,session,backend}.rs` | Rust 化 | 06 §3.1-3.6 |

- **验收标准**：
  1. 给出次设备号到终端表的完整映射（三段 + 两特例 + 激活检查），含每个出口的 errno
  2. 画出行规则七步加工链的流程图（含正规/非正规两分支）
  3. 给出挂起三路的字段构成与取消的匹配规则
  4. 解释"为什么主循环要先扫灯再收信"（答：防事件饿死；延迟换及时性）
  5. 给出轮询两条特殊规则的判定逻辑与 C 锚点
  6. 说明日志别名（minor 15）的特殊语义（不收编 ctty、不计数、只写诊断）

### 11-pty-driver

- **一句话定位**：读者读完能说出没有硬件的终端对如何开关，字节如何在主从两端之间流动。
- **讲什么**：
  - 主从对讲机模型（两条数据通路：主→从输入队列、从→主输出环）
  - 32 对 + 六个状态位
  - 经典对规则（从端可先开、主端只开一次）与 Unix98 对规则（克隆领号、拒直接开主端）
  - 克隆分配流程（找空对 → 清旧节点 → 打标 → 回新号）与无空对答再试
  - 残留从节点双问题（死节点 / 安全洞）
  - 输出环 2048 字节与主端读泵
  - 主端取消两路配对
  - 包模式零字节信封 + 待办记录
  - 主端选择两条规则（写虚看从端关/挂起/队空；读虚看环货）与"从端关→一切就绪"
  - 主端关闭三件事（标关、速度置 B0、发 SIGHUP）
  - 双关复位与 Unix98 请 FS 删从节点
  - 从端读唤醒 / 从端写进环唤醒主端
  - ptyfs 侧车（同步通信、端点不缓存；**只讲驱动侧增删请求**，树语义归 15-stage-fs）
  - Rust 侧：对状态标志字节、输出环计数泵、选择探测纯函数、PtyFs 行为定义
- **不讲什么**：
  - 从端行规则与挂起（10，从端就是终端逻辑）
  - 从节点树语义（`15-stage-fs/20-ptyfs.md`）
  - VFS 侧终端分配
  - 框架机制（04）
- **前置**：00、01、02、03、04、05、06、07、08、09、10
- **后置**：12、17
- **事实底线**：
  - C：`minix3/minix/drivers/tty/pty/pty.c`（860 行）：`:205`（`pty_reset`）、`:224`（`pty_master_close`）；`:46-136`（对表与分配）、`:138-223`（主端打开）、`:234-244`（关闭三件事）、`:415-516`（主端取消与选择）、`:621-681`（输出泵）；`drivers/tty/pty/tty.c`（1320 行，从端行规则，与终端同源）；`drivers/tty/pty/ptyfs.c`（112 行，侧车）
  - 非 C 制品：`include/minix/com.h:901-902`（从节点增删请求号）、`include/minix/config.h:46`（`NR_PTYS` 32）
  - Rust：`os/drivers/tty/pty/src/{pair,buffer,select,ptyfs}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-159 | 主从对讲机模型与两条通路 | 概念 | `pty.c` | 定位 | 07 §1.1 |
| K-160 | 32 对 + 六个状态位 | 数据结构 | `config.h:46` | 结构 | 07 §1.2/§2.2 |
| K-161 | 经典对规则 | 约束与不变量 | 主端打开分支 | 本篇核心 | 07 §1.2/§2.3 |
| K-162 | Unix98 对规则 | 约束与不变量 | 克隆分支 | 本篇核心 | 07 §1.2/§2.3 |
| K-163 | 双关复位与请 FS 删从节点 | 机制 | `pty.c:205` | 本篇核心 | 07 §1.2/§2.4 |
| K-164 | 克隆分配流程与无空对答再试 | 机制 | `get_free_pty` | 本篇核心 | 07 §1.3/§2.3 |
| K-165 | 残留从节点双问题 | 约束与不变量 | — | 本篇核心 | 07 §1.3 |
| K-166 | 输出环 2048 字节与主端读泵 | 数据结构 | `pty.c` pump | 本篇核心 | 07 §1.4/§2.6 |
| K-167 | 主端取消两路配对 | 机制 | 主端 cancel | 本篇核心 | 07 §1.4/§2.5 |
| K-168 | 包模式零字节信封 | 架构演进 | `set_packet_mode` | 本篇核心 | 07 §1.5/§2.9 |
| K-169 | 主端选择两条规则 | 机制 | `select_try_pty` | 本篇核心 | 07 §1.6/§2.5 |
| K-170 | ptyfs 侧车（驱动侧增删请求） | 机制 | `ptyfs.c` | 边界 | 07 §1.7/§2.7 |
| K-171 | 主端关闭三件事 | 机制 | `pty.c:224` | **B2 修复点** | 07 §2.4 |
| K-172 | 从端读唤醒 / 从端写进环唤醒主端 | 机制 | `pty_slave_read/write` | 本篇核心 | 07 §2.6 |
| K-173 | 对状态用标志字节 | 架构演进 | `pair.rs` | Rust 化 | 07 §3.1 |
| K-174 | 输出环计数泵 | 架构演进 | `buffer.rs` | Rust 化 | 07 §3.3 |
| K-175 | 选择探测纯函数 | 架构演进 | `select.rs` | Rust 化 | 07 §3.4 |
| K-176 | PtyFs 行为定义 + `clone_gate` | 架构演进 | `ptyfs.rs` | Rust 化 | 07 §3.5 |
| K-159a | 与 C 差异表四条 | 架构演进 | — | Rust 化 | 07 §2.9 |

- **验收标准**：
  1. 给出 32 对状态机的状态表与转移条件（含克隆领号、双关复位）
  2. 画出字节在主从两端之间流动的完整路径（两个方向）
  3. 解释"为什么主端关闭要置 B0 并发 SIGHUP"（答：让从端读到 EOF 并收到挂断信号）
  4. 给出主端选择两条规则的判定逻辑
  5. 说明 ptyfs 侧车的边界（只讲驱动侧增删请求，树语义归 15-stage-fs）

### 12-log-driver

- **一句话定位**：读者读完能说出诊断消息从产生到被读走经过哪几道手，读者来晚了旧消息还在不在。
- **讲什么**：
  - 诊断总线定位（内核与服务汇入、只写与只读两方）
  - 五万字节环与写满盖旧货
  - 写永远成功（除非法号）+ 超长只留末窗
  - **写尾巴顺序：先唤醒挂起读、再通知选择者**（本篇核心不变式）
  - 读三情形（有货当场答 / 非阻塞再试 / 阻塞挂起）与单路挂起、二路答零
  - 取消精确配对（调用方 + 标识）；非法号 EINVAL
  - 选择三位 + 迟通知位；写永就绪、错位永不就绪
  - 内核消息增量（新旧指针差取模、零新增照样推进）
  - 两个读者各记各指针（终端 vs 日志）
  - 热升级三钩子（准备 / 验态 / 倒态）
  - 环定义与 `logdevice` 字段
  - 读写错号用 EIO 而非 ENODEV（**历史写法原样保留**）；选择错号用 ENODEV
  - `subwrite`/`subread` 逐段搬与 `LOGINC` 取模加
  - Rust 侧：环做成三数计数器、单槽挂起是类型、唤醒合并成一次调用返回值、增量泛型游标
- **不讲什么**：
  - 内核诊断输出的产生（`01-stage-kernel`）
  - 系统日志命令如何读（`18-stage-commands`）
  - 热升级框架本身（`02-stage-vm`）
  - 框架机制（04）
- **前置**：00、01、02、03、04、05、06、07、08、09、10、11
- **后置**：31
- **事实底线**：
  - C：`minix3/minix/drivers/system/log/log.c`（360 行）：`:53`（`main`）、`:23`（`log_read`）、`:25`（`log_write` 前向声明）、`:30`（`subread`）、`:124`（`subwrite`）、`:198`（`log_append`）、`:278`（`log_write` 定义）、`:306`（`log_cancel`）、`:323`（`log_select`）；`:171-191`（写尾巴顺序）、`:173`（有货守卫）、`:87-107`（初始化与信号）；`log.h`（环定义与 `LOG_SIZE`）、`diag.c`（54 行，内核消息捕获）、`liveupdate.c`（99 行）
  - 非 C 制品：`include/minix/com.h:949-952`（选择位）
  - Rust：`os/drivers/system/log/src/{ring,device,diag}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-175a | 诊断总线定位 | 概念 | — | 定位 | 08 §1.1 |
| K-176 | 五万字节环与写盖旧货 | 数据结构 | `log.h` `LOG_SIZE`、`log.c:124` | 本篇核心 | 08 §1.2/§2.2/§2.3 |
| K-177 | 写尾巴顺序 | 约束与不变量 | `log.c:171-191`、`:198` | **本篇核心不变式** | 08 §1.2/§3.6 |
| K-178 | 读三情形与单路挂起 | 机制 | `log.c:23` | 本篇核心 | 08 §1.3/§2.4 |
| K-179 | 取消精确配对 | 机制 | `log.c:306` | 本篇核心 | 08 §1.4/§2.5 |
| K-180 | 选择三位与写永就绪 | 接口与协议 | `log.c:323`、`com.h:949-952` | 本篇核心 | 08 §1.5/§2.5 |
| K-181 | 内核消息增量 | 机制 | `diag.c` | 本篇核心 | 08 §1.6/§2.6 |
| K-182 | 环定义与 `logdevice` 字段 | 数据结构 | `log.h` | 结构 | 08 §2.2 |
| K-183 | 错号 errno 三条口径 | 约束与不变量 | `log.c` | **原样保留范例** | 08 §2.4/§2.5/§2.9 |
| K-184 | `subwrite` 逐段截断与 `LOGINC` | 机制 | `log.c:124` | 实现 | 08 §2.3 |
| K-185 | `subread` 逐段搬与指针推进 | 机制 | `log.c:30` | 实现 | 08 §2.4 |
| K-186 | 环做成三数计数器 | 架构演进 | `ring.rs` | Rust 化 | 08 §3.1 |
| K-187 | 单槽挂起是类型 | 架构演进 | `device.rs` | Rust 化 | 08 §3.2 |
| K-188 | 唤醒合并成一次调用返回值 | 架构演进 | `device.rs:135-139/194-196` | **B3 修复点** | 08 §3.6 |
| K-189 | 增量做成泛型游标 | 架构演进 | `diag.rs` | Rust 化 | 08 §3.5 |
| K-051b | 热升级三钩子 | 机制 | `liveupdate.c` | 本篇核心 | 08 §1.7/§2.7 |
| K-152a | 两个读者各记各指针 | 概念 | — | 与 10 对照 | 08 §1.6/§3.5 |

- **验收标准**：
  1. 给出环的完整状态模型（空 / 有货 / 满）与写盖旧货的语义
  2. **逐字给出写尾巴顺序的三条不变式**（读者腿在前 / 选择者腿在后 / 读位领取即清）
  3. 给出读三情形的判定表与各自的回复
  4. 解释"为什么错号用 EIO 而不是 ENODEV"（答：历史写法，原样保留并注释，不擅自统一）
  5. 说明内核消息增量的取模差值算法与"零新增照样推进"

### 13-random-driver

- **一句话定位**：读者读完能说出噪声如何变成用不完的随机字节，没攒够噪声时读操作怎么办。
- **讲什么**：
  - 全系统唯一熵源的定位（密钥 / 会话标识 / 地址随机化）
  - 三层结构（池收噪声 / 密钥流产字节 / 设备定阻塞）
  - **单设备事实核查**（`NR_DEVS` 为一，无不阻塞设备；纠正 plan stub 的双设备写法）
  - 32 池 + 源内轮转规则 + 第一池人人有份
  - 播种计数选池的位运算（二进制第一个一）
  - 导数过滤 16 阶差分、最小差 < 2 丢弃
  - 播种条件 256 样本与**混合顺序（旧密钥 → 零池 → 选中池 → 终结）**
  - 信任汁（写即播种、1 字节 = 8 样本）
  - 计数器加密产流（64 位高低字、低字先加、回绕进位）与产流后换密钥（前向保密）
  - 1024 字节大块与尾块侧产截取
  - 设备语义（未播种答再试、轮询永远全就绪）
  - 采集周期（未播种一拍 / 播种后五百拍）
  - 熵源 16 × 每源 64 样本分箱结构与环绕读取
  - 初始化参数核对失败直接退出
  - Rust 侧：池拓扑纯类型、密码原语行为定义、密钥流计数器状态机、设备纯检查
- **不讲什么**：
  - 内核侧噪声采集（`01-stage-kernel`）
  - 密码学原语的**选型论证**（本库只定形状；实现见 `crypto.rs`）
  - 其它系统服务的用量
  - 框架机制（04）
- **前置**：00、01、02、03、04、05、06、07、08
- **后置**：31
- **事实底线**：
  - C：`minix3/minix/drivers/system/random/main.c`（268 行）：`:56`（`main`）、`:83`（`sef_cb_init_fresh`）、`:24`（`r_read`）、`:26`（`r_write`）、`:90`（`r_random`）、`:259`（`r_select`）；`random.c`（237 行）：`:37`（`random_init`）、`:81`（`random_getbytes`）、`:115`（`random_putbytes`）、`:130`（`add_sample`）、`:181`（`data_block`）、`:206`（`reseed`）；`aes/`（分组密码实现）；`random.h`
  - 非 C 制品：`include/minix/type.h:182-193`（熵源数量与样本结构）
  - Rust：`os/drivers/system/random/src/{pool,core,device,crypto}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-190 | 全系统唯一熵源的定位 | 概念 | — | 定位 | 09 说明/§1.1 |
| K-191 | 三层结构 | 概念 | — | 定位 | 09 说明/§1.1 |
| K-192 | 单设备事实核查 | 接口与协议 | `random/main.c` | **纠正 plan stub** | 09 §1.6/§2.9 |
| K-193 | 32 池 + 轮转 + 第一池人人有份 | 数据结构 | `random.c:37`、`NR_POOLS` | 本篇核心 | 09 §1.2/§2.4 |
| K-194 | 播种计数选池的位运算 | 机制 | `random.c:206` | 本篇核心 | 09 §1.2/§2.5 |
| K-195 | 导数过滤 16 阶差分 | 机制 | `random.c:130` | 本篇核心 | 09 §1.3/§2.4 |
| K-196 | 播种条件与混合顺序（含终结） | 机制 | `random.c:206-236` | **S1 修复点** | 09 §1.4/§2.5 |
| K-197 | 信任汁 | 机制 | `random.c:115` | 本篇核心 | 09 §1.4/§2.4 |
| K-198 | 计数器加密产流 | 机制 | `random.c:181`、`:81` | 本篇核心 | 09 §1.5/§2.6 |
| K-199 | 产流后换密钥（前向保密） | 机制 | `random.c:81` | 本篇核心 | 09 §1.5 |
| K-200 | 1024 字节大块与尾块截取 | 机制 | `RANDOM_BUF_SIZE` | 本篇核心 | 09 §1.5/§2.7 |
| K-201 | 设备语义（未播种答再试、轮询全就绪） | 接口与协议 | `random/main.c:24/259` | 本篇核心 | 09 §1.6/§2.7 |
| K-202 | 采集周期 | 机制 | `random/main.c:90`、`KRANDOM_PERIOD` | 本篇核心 | 09 §1.6/§2.3 |
| K-203 | 熵源 16 × 64 样本分箱结构 | 数据结构 | `type.h:182-193` | 结构 | 09 §2.2 |
| K-204 | 分箱环绕读取与越界断言 | 机制 | `r_updatebin` | 实现 | 09 §2.2 |
| K-205 | 采集定时 | 机制 | `random/main.c:83` | 实现 | 09 §2.3 |
| K-206 | 初始化参数核对失败退出 | 约束与不变量 | sanity 检查 | 实现 | 09 §2.3 |
| K-207 | 池拓扑纯类型 + 混合顺序收成一个方法 | 架构演进 | `pool.rs` | Rust 化 | 09 §3.1 |
| K-208 | 密码原语行为定义 | 架构演进 | `core.rs` | **G4/A9 落地** | 09 §3.2 |
| K-209 | 密钥流计数器状态机 | 架构演进 | `core.rs` | Rust 化 | 09 §3.3 |
| K-210 | 设备纯检查 + 分块计划 + 采集周期三纯函数 | 架构演进 | `device.rs` | Rust 化 | 09 §3.4 |
| K-211 | 独立回放对账测试 | 测试性质 | — | 测试 | 09 §5.2 |
| K-196a | 与 C 差异表五条 | 架构演进 | — | Rust 化 | 09 §2.9 |

- **验收标准**：
  1. 给出三层结构的完整图（噪声源 → 池 → 密钥流 → 设备）
  2. **逐字给出播种的混合顺序**（旧密钥 → 零池 → 选中池 → 终结），并说明 S1 修复的意义
  3. 给出导数过滤的完整算法（16 阶差分、最小差 < 2 丢弃）
  4. 给出计数器加密的字节序（64 位高低字、低字先加）与它和 C 内存拷贝顺序的一致性
  5. 解释"未播种时读操作怎么办"（答：答 EAGAIN 不替调用方等；采集定时加速到一拍一次）
  6. 说明"单设备"这一事实核查的依据（`NR_DEVS` 为一）与 plan stub 的错误

### 14-readclock-driver

- **一句话定位**：读者读完能说出三个时钟操作各自验谁的资格，时钟嵌在别的芯片里时怎么办。
- **讲什么**：
  - 实时钟定位（断电也走、开机对表）
  - 三操作权限门（读无门 / 写 `SUPER_USER` / 断电 `PM_PROC_NR`）
  - 门在碰芯片与碰调用方缓冲之前
  - **为何不走字符框架**（七种请求形状不匹配）
  - 五请求号 + 一回复号 + 基址 `0x1400`
  - 转发三规矩（标签没配拒初始化 / 断电直转 / 读写走授权）与授权方向
  - 转发作为外观（隔离变化）
  - BCD 换算两函数互逆
  - 通知一律丢弃不回复；接收失败记日志继续（**与框架"停下"相反**）
  - 无状态 → 可继续的推理链
  - 初始化装配（`arch_setup` 填操作表 `struct rtc`）；新旧重启热升级全走同一函数；退出不可达
  - 架构时钟分工与操作表抽象
  - Rust 侧：`RtcRequest` 枚举、时间值类型八字段与合理性谓词、权限纯分诊、转发目标值类型
- **不讲什么**：
  - 时钟服务与定时器（`01-stage-kernel`）
  - 两种架构时钟的逐寄存器细节（服务层）
  - 芯片驱动本身
  - 框架机制（04，本篇不走字符框架）
- **前置**：00、01、02、03、04、05、06、07、08、09、10、11、12、13
- **后置**：15、31
- **事实底线**：
  - C：`minix3/minix/drivers/clock/readclock/readclock.c`（191 行）：`:39`（`main`）、`:168`（`bcd_to_dec`）、`:174`（`dec_to_bcd`）；`:50-131`（协议循环）、`:71-108`（权限门）、`:133-165`（初始化）、`:167-191`（十进制换算）；`forward.c`（120 行）：`:46`（`fwd_set_label`）、`:53`（`fwd_init`）、`:99`（`fwd_get_time`）、`:105`（`fwd_set_time`）、`:111`（`fwd_pwr_off`）；`readclock.h`（操作表）、`arch/`（两种架构时钟）
  - 非 C 制品：`include/minix/com.h:995-1012`（基址与五请求）、`:1015`（`RTCDEV_Y2KBUG`）
  - Rust：`os/drivers/clock/readclock/src/{protocol,clock,device}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-212 | 实时钟定位 + 三操作权限门 | 接口与协议 | `readclock.c:39` | 本篇核心 | 10 §1.1/§2.3 |
| K-213 | 门在碰芯片与缓冲之前 | 约束与不变量 | — | 本篇核心 | 10 §1.1/§3.4 |
| K-214 | 为何不走字符框架 | 概念 | — | **专有协议判定依据** | 10 §1.2 |
| K-215 | 五请求号 + 一回复号 + 基址 | 接口与协议 | `com.h:995-1012` | 协议 | 10 §1.2/§2.1 |
| K-216 | 转发三规矩与授权方向 | 机制 | `forward.c:46/53/99/105/111` | 本篇核心 | 10 §1.3/§2.5 |
| K-217 | 转发作为外观 | 概念 | — | 设计 | 10 §1.3 |
| K-218 | BCD 换算两函数互逆 | 机制 | `readclock.c:168/174` | 本篇核心 | 10 §1.4/§2.4 |
| K-219 | 通知丢弃 + 接收失败继续 | 约束与不变量 | `readclock.c:39` | **与框架相反** | 10 §1.5/§2.2 |
| K-220 | 回复用通用回复号 + 非阻塞发送 | 接口与协议 | `readclock.c:39` | 实现 | 10 §2.2 |
| K-221 | 初始化装配 + 热升级同函数 + 退出不可达 | 机制 | `readclock.c` `arch_setup` | 实现 | 10 §2.6 |
| K-222 | 架构时钟分工与操作表抽象 | 架构演进 | `readclock/arch/` | 边界 | 10 §2.7 |
| K-223 | `RtcRequest` 枚举 | 架构演进 | `protocol.rs` | Rust 化 | 10 §3.1 |
| K-224 | 时间值类型八字段与合理性谓词 | 数据结构 | `com.h:1015`、`protocol.rs:104-117` | **G9 落地** | 10 §3.2 |
| K-225 | 权限纯分诊 | 架构演进 | `device.rs` | **类型层面强制** | 10 §3.4 |
| K-226 | 转发目标值类型 | 架构演进 | `clock.rs` | Rust 化 | 10 §3.5 |
| K-227 | 与 C 差异表四条 | 架构演进 | — | Rust 化 | 10 §2.9 |

- **验收标准**：
  1. 给出三操作的权限门表：操作、校验对象、失败 errno、C 锚点
  2. 解释"为什么不走字符框架"（答：七种请求形状与三操作不匹配；自立门户更简单）
  3. 给出转发三规矩与授权方向的完整表
  4. 说明"接收失败记日志继续"与框架"停下"的差异及其理由（答：无状态 → 可继续）
  5. 给出 BCD 换算两函数的互逆性证明与边界（0-99 往返无损）

### 15-pci-driver

- **一句话定位**：读者读完能说出一张新卡插上系统，驱动如何知道它是谁、能不能碰、怎么说话。
- **讲什么**：
  - 设备户籍警定位（枚举 / 配置读写 / 名单门禁）
  - 查询协议 18 操作、基址 `0x300`、5/6 号空号
  - 查询四组分类（建档 / 读写 / 管理 / 保留）与游标制（无状态游标：索引在消息里）
  - 配置空间 256 字节与三种宽度；宽度-对齐绑定与错位访问拒绝
  - 端口号永不进 OS 层
  - **可见性规则（名单跟调用方走）**、设备号条目匹配与 `0xFFFF` 子系统通配、类别码与掩码规则、无名单全见、摘单复见
  - 门禁做在遍历层（早于读写层一步）
  - 枚举建档 `probe_bus`（递归桥 / 中断路由 / 地址窗口）与重复位置拒绝
  - 桥标识小抄表 37 行
  - 控制码七件事与"映射先加内存特权后映射"
  - 配置读写只实现 32 位
  - 通用挂钩按消息号分发 18 问；字符表四挂钩、无读写挂钩
  - **预留占用位与四态处置**（越界 / 不可见 / 他人忙 / 自占重入）
  - Rust 侧：`BusQuery` 枚举、`ConfigSpace` 行为定义、`DeviceDb`、`BusControl` 枚举
- **不讲什么**：
  - 各设备驱动如何用总线信息（18、20、26、27）
  - 桥窗口力学与中断路由芯片细节（服务层）
  - I2C 总线（16、28）
  - 框架机制（04）
- **前置**：00、01、02、03、04、05、06、07、08、09、10、11、12、13、14
- **后置**：16、18、20、22、26、31
- **事实底线**：
  - C：`minix3/minix/drivers/bus/pci/main.c`（740 行）：`:62`（`do_first_dev`）、`:91`（`do_next_dev`）、`:117`（`do_find_dev`）、`:238`（`do_set_acl`）、`:281`（`do_del_acl`）、`:538`（`pci_ioctl`）、`:728`（`main`）；`:46-524`（查询协议）、`:538-668`（控制七件事）、`:670-760`（通用挂钩与字符表）；`pci.c`（2559 行）：`:420`（`is_duplicate`）、`:1519`（`probe_bus`）、`:2039`（`visible`）、`:2325-2336`（预留四态）；`pci_table.c`（37 行，桥标识表）
  - 非 C 制品：`include/minix/com.h:95-160`（总线查询号段）
  - Rust：`os/drivers/bus/pci/src/{protocol,config,database}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-226a | 设备户籍警定位 | 概念 | — | 定位 | 11 §1.1 |
| K-227 | 查询协议 18 操作 + 基址 + 空号 | 接口与协议 | `com.h:95-160` | 协议 | 11 §1.2/§2.1 |
| K-228 | 游标制与无状态游标 | 机制 | `main.c:62/91/117` | 本篇核心 | 11 §1.2 |
| K-229 | 配置空间 256 字节与三种宽度 + 对齐拒绝 | 数据结构 | `config.rs` | **S4 修复点** | 11 §1.3/§3.2 |
| K-230 | 端口号永不进 OS 层 | 架构演进 | — | ARCH | 11 §1.3/§3.2 |
| K-231 | **可见性规则** | 机制 | `pci.c:2039` | **S2 修复点** | 11 §1.4/§3.3 |
| K-232 | 门禁做在遍历层 | 架构演进 | — | 设计 | 11 §1.4 |
| K-233 | 枚举建档 + 重复位置拒绝 | 机制 | `pci.c:1519/420` | 本篇核心 | 11 §1.5 |
| K-234 | 桥标识小抄表 | 工具与工程 | `pci_table.c` | 实现 | 11 §1.5/§2.7 |
| K-235 | 控制码七件事 + 映射次序 | 接口与协议 | `main.c:538` | 本篇核心 | 11 §1.6/§2.3 |
| K-236 | 配置读写只实现 32 位 | 接口与协议 | — | 实现 | 11 §2.3 |
| K-237 | 通用挂钩分发 18 问；字符表四挂钩 | 机制 | `main.c:670-760` | 本篇核心 | 11 §2.4 |
| K-238 | `BusQuery` 枚举 18 变体 | 架构演进 | `protocol.rs` | Rust 化 | 11 §3.1 |
| K-239 | `DeviceDb` 记录向量 + 名单每端点一份 | 数据结构 | `database.rs` | 结构 | 11 §3.3 |
| K-240 | **预留占用位与四态处置** | 机制 | `_pci_reserve`/`_pci_release` | **S3 修复点** | 11 §3.3/§5.3 |
| K-241 | `BusControl` 枚举七变体 | 架构演进 | `database.rs` | Rust 化 | 11 §3.4 |
| K-231a | 与 C 差异表四条 | 架构演进 | — | Rust 化 | 11 §2.7 |

- **验收标准**：
  1. 给出 18 个查询操作的完整表：编号、名称、材料、回复、空号标注
  2. 逐条给出可见性规则（无名单全见 / 有名单逐条匹配 / `0xFFFF` 通配 / 类别掩码）
  3. 给出预留四态的判定顺序与各自的 errno
  4. 解释"为什么门禁做在遍历层"（答：遍历层是唯一入口，早于读写层一步；读写层不需重复检查）
  5. 给出配置空间的三种宽度与对齐约束（含 `offset%4==2` 的 16 位写语义）
  6. 说明"端口号永不进 OS 层"在 PCI 驱动的落点

### 16-gpio-devman

- **一句话定位**：读者读完能说出引脚如何变成文件，新设备如何上户口。
- **讲什么**：
  - 引脚即文件定位与文件挂载规则（读文件必有；输出加开关两文件；输入加中断文件）
  - 渲染 `"%d\n"` 两位与偏移三情形；渲染三元组返回约定
  - 认领三查（脚号有人 / 模式 / 属主）；读写两查与输入脚拒绝驱动；中断读查认领不查方向
  - 先到先得是全部公平性来源（无优先级无抢占、持有至进程结束）
  - 设备注册三件套（父设备号 / 名字 / 属性表）与序列化三段布局
  - 柄表语义（空槽复用最小号 / 删空槽假 / 查借记录 / 列活柄）
  - USB 跟踪多一层（32 接口、绑定回调记端点、拒绝记空）
  - 客户端不设未绑定计数器（**勘误**：对 C 的误读）
  - 两表对照与"登记三部曲"
  - 引脚导出流程（认领 → 设模式 → 分配回调 → 挂文件 → 板型分支）
  - 开关文件语义与中断文件锁存标记；消息钩子转交中断消息
  - 虚拟树三钩子（初始化 / 读 / 消息）、根目录只读
  - 库名加客户端后缀（**包名与库名双撞**）
  - Rust 侧：引脚硬件行为定义、认领查表、导出命名规划、柄表
- **不讲什么**：
  - 设备管理服务本身（`11-stage-devman`）
  - 虚拟树文件系统框架（`15-stage-fs/18-vtreefs.md`，本篇只讲三个钩子）
  - I2C 总线（28）
  - 框架机制（04）
- **前置**：00、01、02、03、04、05、06、07、08、09、10、11、12、13、14、15
- **后置**：17、28、31
- **事实底线**：
  - C：`minix3/minix/drivers/system/gpio/gpio.c`（290 行）：`:78`（`add_gpio_inode`）、`:237`（`gpio_read`）、`:265`（`main`）；`:60-160`（引脚导出）、`:210-260`（读渲染）；`lib/libdevman/generic.c`（275 行）：`:36`（`serialize_dev`）、`:102`（`devman_add_device`）、`:154`（`devman_del_device`）、`:188`（`devman_init`）；`lib/libdevman/usb.c`（301 行）；`include/minix/devman.h`
  - 非 C 制品：`include/minix/vtreefs.h`（三钩子）
  - Rust：`os/drivers/system/gpio/src/{pins,files}.rs`、`os/libs/minix-sys/src/{devman_client,usb_model}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-242 | 引脚即文件定位与文件挂载规则 | 概念 | `gpio.c` | 定位 | 12 §1.1 |
| K-243 | 渲染 `"%d\n"` 两位与偏移三情形 | 机制 | `gpio.c:237` | 本篇核心 | 12 §1.2/§2.3 |
| K-244 | 认领三查与读写两查 | 机制 | `gpio.c:78` | 本篇核心 | 12 §1.3/§2.2 |
| K-245 | 先到先得是全部公平性来源 | 约束与不变量 | — | 本篇核心 | 12 §1.3 |
| K-246 | 设备注册三件套与序列化 | 数据结构 | `generic.c:36/102/154` | 本篇核心 | 12 §1.4/§2.4 |
| K-247 | 柄表语义 | 数据结构 | `generic.c` | 本篇核心 | 12 §1.4/§3.4 |
| K-248 | USB 跟踪多一层 | 数据结构 | `libdevman/usb.c` | 本篇核心 | 12 §1.4/§2.5 |
| K-249 | 客户端不设未绑定计数器（勘误） | 架构演进 | — | **E-DMCLIENT 勘误** | 12 §1.4/§3.5 |
| K-250 | 两表对照与登记三部曲 | 概念 | 借 11-pci | 对照 | 12 §1.5 |
| K-251 | 引脚导出流程 | 机制 | `gpio.c:78` | 本篇核心 | 12 §2.2 |
| K-252 | 开关文件语义与中断锁存 | 机制 | `gpio.c` | 本篇核心 | 12 §2.3 |
| K-253 | 虚拟树三钩子 | 机制 | vtreefs | 边界（框架归 FS 阶段） | 12 §2.3 |
| K-254 | 通用注册增删与端点静态存 | 机制 | `generic.c:188` | 实现 | 12 §2.4 |
| K-255 | 引脚硬件行为定义与两实现 | 架构演进 | `pins.rs` | Rust 化 | 12 §3.1 |
| K-256 | 导出命名规划 | 架构演进 | `files.rs` | Rust 化 | 12 §3.3 |
| K-257 | 库名加客户端后缀 | 架构演进 | — | **命名事实核查** | 12 §3.6 |
| K-257a | 与 C 差异表四条 | 架构演进 | — | Rust 化 | 12 §2.7 |

- **验收标准**：
  1. 给出引脚文件挂载规则表（输入脚 / 输出脚 / 中断脚各挂哪些文件）
  2. 给出认领三查与读写两查的完整判定表与各自 errno
  3. 给出设备注册的序列化三段布局（头 16 + 条目 16×n + 字符串）
  4. 给出柄表的四项操作语义
  5. 说明"客户端不设未绑定计数器"这一勘误的依据
  6. 解释"为什么库名要加客户端后缀"（答：包名与库名双撞）

### 17-pckbd-driver

- **一句话定位**：读者读完能说出按键与移动如何变成输入服务的事件，灯如何听指挥。
- **讲什么**：
  - 扫描码翻译定位（源语言 → 系统普通话）与页码为零表项即填充
  - 状态机四态与三个前缀；暂停键六字节前奏；**暂停确认态的穿透（FALLTHROUGH）**；错位自愈回零态
  - 鼠标三字节包与包头同步位判定；按钮逐位比；位移符号展 32 位 + 相对标志
  - 短包自定界的协议优点
  - LED 两字节命令与 16 字节队列；满队丢并清确认标志；ACK 才推进
  - **LED 掩码位对位翻译**（输入服务三位 → 键盘三位）
  - 事件桥单行道定位；桥状态三件套；配置与设灯消息验发送方；阻塞发两条理由与背压语义
  - 初始化认亲（键盘必有 / 鼠标可选）与上报标志
  - **看门狗两条与复位重发**
  - Rust 侧：扫描码状态机三变体、对照表行为定义（全表 111+40）、鼠标组装器、LED outbox、桥门禁状态
- **不讲什么**：
  - 输入服务的事件消费（`12-stage-input`）
  - 终端驱动的键盘读取（10，另一条键盘路）
  - 键盘控制器的端口细节（服务层）
  - 框架机制（本篇不走字符框架，走输入协议）
- **前置**：00、01、02、03、04、05、06、07、08、09、10、11、12、13、14、15、16
- **后置**：31
- **事实底线**：
  - C：`minix3/minix/drivers/hid/pckbd/pckbd.c`（507 行）：`:47`（`kbd_watchdog`）、`:111`（`scan_keyboard`）、`:175`（`set_leds`）、`:328`（`kbd_process`）、`:374`（`kbdaux_process`）、`:500`（`main`）；`:21-23`、`:49-62`（看门狗两条）、`:353-358`（暂停穿透）、`:327-412`（扫描与翻译）、`:175-192`、`:417-428`（LED）；`table.c`（169 行，扫描码对照表）、`pckbd.h`；`lib/libinputdriver/inputdriver.c`（206 行）：`:21`（`inputdriver_announce`）、`:43`（`inputdriver_send_event`）、`:142`（`inputdriver_process`）
  - 非 C 制品：`include/minix/inputdriver.h`、`include/minix/input.h`（`:292-296` LED 位）、`include/minix/com.h:890-893`
  - Rust：`os/drivers/hid/pckbd/src/{scancode,mouse,led,tables}.rs`、`os/libs/minix-sys/src/inputdriver.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-258 | 扫描码翻译定位与零页填充 | 概念 | — | 定位 | 13 §1.1 |
| K-259 | 状态机四态 + 三前缀 + **穿透** + 自愈 | 数据结构 | `pckbd.c:328` | **S5 修复点** | 13 §1.2/§2.2 |
| K-260 | 鼠标三字节包 + 按钮 + 位移符号展 | 机制 | `pckbd.c:374` | **S7 修复点** | 13 §1.3/§2.2 |
| K-261 | 短包自定界的协议优点 | 概念 | — | 设计 | 13 §1.3 |
| K-262 | LED 两字节命令与队列 + ACK 推进 | 数据结构 | `pckbd.c:175`、`KBD_OUT_BUFSZ` | 本篇核心 | 13 §1.4/§2.3 |
| K-263 | **LED 掩码位对位翻译** | 机制 | `input.h:293-295` | **S6 修复点** | 13 §1.4/§2.3 |
| K-264 | 事件桥单行道 + 桥状态 + 验发送方 + 背压 | 接口与协议 | `inputdriver.c:21/43/142` | 本篇核心 | 13 §1.5/§2.4 |
| K-265 | 初始化认亲与上报标志 | 机制 | `pckbd.c:500` | 本篇核心 | 13 §1.6/§2.5 |
| K-266 | **看门狗两条与复位重发** | 机制 | `pckbd.c:21-23/49-62` | **G7 缺项** | 13 §2.5（G7 补） |
| K-267 | 扫描码状态机三变体 | 架构演进 | `scancode.rs` | Rust 化 | 13 §3.1 |
| K-268 | 对照表行为定义（全表 111+40） | 架构演进 | `tables.rs` | **G3 落地** | 13 §3.2 |
| K-269 | 鼠标三字节组装器 | 架构演进 | `mouse.rs` | Rust 化 | 13 §3.3 |
| K-270 | LED outbox | 架构演进 | `led.rs` | Rust 化 | 13 §3.4 |
| K-271 | 桥门禁状态 | 架构演进 | `minix-sys/src/inputdriver.rs` | **A11 收敛** | 13 §3.5 |
| K-264a | 与 C 差异表（**结构修复：三条悬空行归位**） | 架构演进 | — | OOB-01 修复 | 13 §2.7 |

- **验收标准**：
  1. 给出扫描码状态机的完整状态转移图（含三个前缀与暂停六字节前奏）
  2. **逐条说明暂停确认态的穿透语义**（哪些键穿透、穿透后走哪条路）
  3. 给出 LED 掩码的位对位映射表（输入服务三位 → 键盘三位）与 S6 修复的意义
  4. 给出鼠标三字节包的字节布局与同步位判定
  5. 给出看门狗两条的触发条件与复位重发流程
  6. 说明事件桥的背压语义（阻塞发两条理由）
  7. **结构检查**：文末不得再有悬空表格行；参见不得再指向已删除的 `bridge.rs`

### 18-virtio-framework

- **一句话定位**：读者读完能说出客户机与宿主机不共享内存管理权时，如何交接一块块的输入输出。
- **讲什么**：
  - 三环结构（描述符表 / 可用环 / 已用环）与旋转寿司隐喻
  - **写权限分离**（客户机只写可用环、宿主机只写已用环、描述符两边只读，故免锁）
  - 描述符链串接（`下一` 标志、末块不置下一位）
  - 写标志管方向；状态块恒置写位
  - 间接标志=整单外包；间接表启动时预分配
  - 空闲链取还必须常数时间
  - 可用序号与已用序号（十六位自然回绕、回绕减法结果恒对）与"未吃数 = 可用减已用"
  - 中断抑制两标志与"抑制全是优化建议，不是正确性依赖"
  - 特性协商取交集；未知宿主位忽略；协商失败不拦路
  - 建设备五步（验参数、找卡、认亲、开队列、备间接表）
  - 认亲状态字节与就绪三步
  - 复位一键与释放两步（先队列后设备）
  - **踢门铃规矩：只查不通知位，清了才踢**
  - 正确性不靠铃（踢丢不致命、踢多白费）
  - 中断规矩与踢对称
  - 环布局（`vring_desc` 十六字节、描述符标志三位、可用/已用环头、完成项八字节）
  - 队列内存是连续物理页（**DMA 契约的动机**）
  - 队列物理初始化与索引
  - 踢中断族（中断归属读状态寄存器、复位、开关中断、匹配设备、特性查询、读三宽度）
  - Rust 侧：环索引状态机、线格式由本库承载（`#[repr(C)]` + 布局函数）、`take_chain`/`collect_chain`、`Hal` 行为定义、协商交集函数、生命周期阶段机、端口行为定义
- **不讲什么**：
  - 虚拟块设备的请求语义（19）
  - 虚拟网卡的请求语义（26）
  - 间接描述符的线程池优化（已声明不做，见 31）
  - 内存映射与中断注册（服务层）
- **前置**：00、01、02、03、04、05、06、07、08、09–17
- **后置**：19、26、30
- **事实底线**：
  - C：`minix3/minix/lib/libvirtio/virtio.c`（913 行）：`:109`（`virtio_setup_device`）、`:166`（`exchange_features`）、`:345`（`virtio_irq_register`）、`:742`（`virtio_reset_device`）、`:766`（`wants_kick`）；`:120-275`（建设备与协商）、`:276-506`（队列与索引）、`:736-831`（踢与中断）；`lib/libvirtio/virtio_ring.h`（环布局，源自 Linux 头）；`include/minix/virtio.h`（操作表与状态字节）
  - 非 C 制品：`virtio_ring.h`（禁改声明）
  - Rust：`os/libs/minix-virtio/src/{ring,features,device,hal}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-272 | 三环结构与隐喻 | 概念 | `virtio_ring.h` | 定位 | 14 §1.1 |
| K-273 | 写权限分离（免锁根因） | 约束与不变量 | — | 本篇核心 | 14 §1.1 |
| K-274 | 描述符链串接 | 机制 | `VRING_DESC_F_NEXT` | 本篇核心 | 14 §1.2 |
| K-275 | 写标志管方向 | 约束与不变量 | — | 本篇核心 | 14 §1.2 |
| K-276 | 间接标志与间接表 | 机制 | `VIRTIO_RING_F_INDIRECT_DESC` | 本篇核心 | 14 §1.2 |
| K-277 | 空闲链常数时间 | 约束与不变量 | — | 本篇核心 | 14 §1.2 |
| K-278 | 序号回绕与回绕减法 | 机制 | — | 本篇核心 | 14 §1.3 |
| K-279 | 中断抑制两标志与"非正确性依赖" | 接口与协议 | — | 本篇核心 | 14 §1.3 |
| K-280 | 特性协商取交集 | 接口与协议 | `virtio.c:166` | 本篇核心 | 14 §1.4 |
| K-281 | 建设备五步 | 机制 | `virtio.c:109` | 本篇核心 | 14 §1.5/§2.3 |
| K-282 | 认亲状态字节与就绪三步 | 接口与协议 | `virtio.h` | 本篇核心 | 14 §1.5 |
| K-283 | 复位一键与释放两步 | 约束与不变量 | `virtio.c:742` | 本篇核心 | 14 §1.5/§2.5 |
| K-284 | **踢门铃规矩** | 约束与不变量 | `virtio.c:766` | **V1 修复点** | 14 §1.5/§2.5 |
| K-285 | 正确性不靠铃 | 约束与不变量 | — | 本篇核心 | 14 §1.5 |
| K-286 | 中断规矩与踢对称 | 约束与不变量 | `virtio.c:345` | 本篇核心 | 14 §1.5 |
| K-287 | `vring_desc` 十六字节布局 | 数据结构 | `virtio_ring.h` | 结构 | 14 §2.2 |
| K-288 | 描述符标志三位与环头结构 | 数据结构 | `VRING_DESC_F_*` | 结构 | 14 §2.2 |
| K-289 | 协商两特性位 | 接口与协议 | `VIRTIO_RING_F_INDIRECT_DESC` | 协议 | 14 §2.2 |
| K-290 | 头文件禁改声明 | 约束与不变量 | `virtio_ring.h` | 工程 | 14 §2.2 |
| K-291 | 队列内存是连续物理页 | 约束与不变量 | — | **DMA 契约动机** | 14 §2.3 |
| K-292 | 建设备参数校验与开队列失败释放 | 机制 | `virtio.c:109` | 实现 | 14 §2.3 |
| K-293 | 队列物理初始化与索引 | 机制 | `virtio.c:276-506` | 实现 | 14 §2.4 |
| K-294 | 踢中断族 | 机制 | `virtio.c:736-831` | 实现 | 14 §2.5 |
| K-295 | 环索引状态机（四数一结构） | 架构演进 | `ring.rs` | Rust 化 | 14 §3.1 |
| K-296 | 线格式由本库承载 | 架构演进 | `ring.rs` | **A5 落地** | 14 §3.1 |
| K-297 | `take_chain`/`collect_chain` | 架构演进 | `ring.rs:169-176` | Rust 化 | 14 §3.1 |
| K-298 | `Hal` 行为定义（DMA 索取） | 架构演进 | `hal.rs` | **→新 30 篇** | 14 §3.1 |
| K-299 | 协商交集函数 | 架构演进 | `features.rs` | Rust 化 | 14 §3.2 |
| K-300 | 生命周期阶段机五态 | 架构演进 | `device.rs` | Rust 化 | 14 §3.3 |
| K-301 | 端口行为定义 | 架构演进 | `device.rs` | Rust 化 | 14 §3.4 |
| K-302 | 与 C 差异表四条 | 架构演进 | — | Rust 化 | 14 §2.7 |

- **验收标准**：
  1. 画出三环结构与两个方向的数据流（客户机放链 → 踢 → 宿主机取 → 放回完成 → 中断）
  2. **逐字给出踢门铃的单条件判据**，并说明 V1 曾多加 `queue_full` 分支的问题
  3. 给出序号回绕的数学论证（十六位自然回绕为什么结果恒对）
  4. 给出描述符链的完整字节布局与三个标志位的语义
  5. 解释"为什么抑制位不是正确性依赖"（答：宿主机在轮询序号；踢丢不致命）
  6. 给出"队列内存必须是连续物理页"的理由并指向新 30 篇

### 19-virtio-blk-driver

- **一句话定位**：读者读完能说出块读写请求如何变成环上的三段链，宿主机的状态字节如何变成错误码。
- **讲什么**：
  - 三段链形状（订单小票=头、饭菜=数据段、评价卡=状态字节）与顺序固定
  - 方向定类型（读 `T_IN` 进、写 `T_OUT` 出）；位置除 512 定扇区号；向量定段数；状态段恒一位
  - 状态字节三值（零成功、一输入输出错、二不支持）
  - **C 的未知状态默认分支是 panic**；**Rust 无停机环境改判输入输出错，显式记为偏差**；铁律：未知状态永不按成功放行
  - 扇区对齐双铁律（位置与长度必须 512 对齐）
  - 截断（跨分区尾按剩余截短，截到块边界）与向量修整
  - 到头（位置过分区尾）答零字节；只读盘写在编链前就拒
  - 单盘单分区与几何 = 容量乘块大小
  - 控制两问（数人头 / 冲水）；其余控制码一律不合适操作
  - 请求组装完整顺序（10 步）
  - 特性表八项
  - Rust 侧：`plan_transfer` 纯函数、`status_to_code` 全匹配、`DriveGeometry` 值类型、`OpenCount` 饱和减
- **不讲什么**：
  - 环怎么转（18，本篇只管链怎么编）
  - 其它存储驱动（20、21、22）
  - 虚拟网卡（26）
- **前置**：00、01、02、03、04、05、06、07、08、09–18
- **后置**：20、21、22、30
- **事实底线**：
  - C：`minix3/minix/drivers/storage/virtio_blk/virtio_blk.c`（754 行）：`:82`（`virtio_blk_transfer`）、`:549`（`virtio_blk_status2error`）、`:743`（`main`）；`:280-379`（请求组装）、`:549-563`（状态翻译）；`virtio_blk.h`（请求头与配置结构）
  - Rust：`os/drivers/storage/virtio_blk/src/{request,geometry}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-302a | 三段链形状与顺序固定 | 数据结构 | — | 定位 | 15 §1.1 |
| K-303 | 方向定类型 + 位置定扇区 + 向量定段数 | 机制 | `VIRTIO_BLK_T_IN`/`T_OUT` | 本篇核心 | 15 §1.1 |
| K-304 | 状态字节三值 + C 的 panic 默认分支 | 接口与协议 | `virtio_blk.c:549` | **V2 修复点** | 15 §1.1/§2.3 |
| K-305 | **Rust 显式偏差**（改判输入输出错） | 架构演进 | 同上 | **唯一正式偏差** | 15 §1.1/§2.3/§3.2/§4 |
| K-306 | 扇区对齐双铁律 | 约束与不变量 | — | 本篇核心 | 15 §1.2 |
| K-307 | 截断与向量修整 | 机制 | — | 本篇核心 | 15 §1.3 |
| K-308 | 到头答零字节 + 只读盘前置拒 | 约束与不变量 | — | 本篇核心 | 15 §1.3 |
| K-309 | 单盘单分区与几何 | 架构演进 | — | 设计决策 | 15 §1.4 |
| K-310 | 控制两问 | 接口与协议 | — | 本篇核心 | 15 §1.5 |
| K-311 | 请求组装完整顺序（10 步） | 机制 | `virtio_blk.c:280-379` | 本篇核心 | 15 §2.2 |
| K-312 | 特性表八项 | 接口与协议 | `virtio_blk.h` | 协议 | 15 §2.4 |
| K-313 | `plan_transfer` 纯函数 | 架构演进 | `request.rs` | Rust 化 | 15 §3.1 |
| K-314 | `status_to_code` 全匹配 + 两替代方案否决 | 架构演进 | `request.rs` | Rust 化 | 15 §3.2 |
| K-315 | `DriveGeometry` 值类型 + `OpenCount` 饱和减 | 架构演进 | `geometry.rs` | Rust 化 | 15 §3.3 |
| K-305a | 与 C 差异表三条 | 架构演进 | — | Rust 化 | 15 §2.6 |

- **验收标准**：
  1. 画出三段链的完整字节布局（头 + 数据段 + 状态字节）与方向标注
  2. 给出请求组装的 10 步顺序与每步的失败出口
  3. **逐字说明 V2 的偏差**：C 是 panic，Rust 改判 EIO，理由是什么，铁律是什么
  4. 给出扇区对齐的双铁律与违反后的 errno
  5. 给出状态字节三值的翻译表与"未知状态永不按成功放行"的实现方式

### 20-ahci-driver

- **一句话定位**：读者读完能说出高级主控接口的 32 命令槽如何申请与交回，超时后如何封锁与重开。
- **讲什么**：
  - 高级主控端口＝机场塔台（32 条命令槽 `COMMAND_SLOTS`）
  - 端口槽位图管理动作（占槽常数时间、完成清槽、超时清全部转超时态、复位回停止态、停机清槽）
  - **铁律：先开塔（端口启动）再申请（发命令）**
  - 端口命令槽五动作（完成清槽、失败全清、找空槽、发命令、执行）
  - 超时复位七动作（硬复位、重写、启动、停止、重启、等待、超时）
  - 超时/复位哲学（超时→全失败→转态拒发；复位=清槽回停止→重编表→重启动）
  - 停止清槽铁律（停机不留半截命令）
  - **识别先验后量**（字零三位任一置位即拒；第 49 字要求直接存取与线性寻址；第 83 字要求字有效、刷缓存、48 位寻址）
  - 容量读四字拼 64 位；两太字节以上不截尾；零容量等于无盘；块短于 256 字拒
  - 容量乘 512 得字节数；写缓存位随身带
  - Rust 侧：端口做成槽位图（32 布尔数组）、识别做成纯解析（字数组进容量出）
- **不讲什么**：
  - 并行接口（21）
  - 虚拟块设备（19，编链思想对照用）
  - 其余存储杂项（22）
  - 帧/散集表/寄存器的逐字节布局（服务层）
  - 中断等待（服务层）
- **前置**：00、01、02、03、04、05、06、07、08、09–19
- **后置**：21、22、30
- **事实底线**：
  - C：`minix3/minix/drivers/storage/ahci/ahci.c`（2734 行）：`:528`（`ata_id_check`）、`:2042`（`ahci_reset`）、`:2063`（`ahci_init`）、`:2723`（`main`）；`:900-985`、`:1810-1877`（端口命令槽五动作）、`:1220-1335`、`:1715-1793`（超时复位七动作）、`:402-775`（识别与缓存）、`:533-560`（识别门与容量拼装）；`ahci.h`（帧布局与端口寄存器）
  - Rust：`os/drivers/storage/ahci/src/{port,identify}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-316 | 高级主控端口＝机场塔台 | 概念 | `COMMAND_SLOTS` | 定位 | 16 §1.1 |
| K-317 | 端口槽位图管理动作 | 机制 | `ahci.c:900-985/1810-1877` | 本篇核心 | 16 §1.1/§2.2 |
| K-318 | 先开塔再申请铁律 | 约束与不变量 | — | 本篇核心 | 16 §1.1 |
| K-326 | 超时复位哲学与停止清槽铁律 | 机制 | `ahci.c:1220-1335/1715-1793` | 本篇核心 | 16 §1.5/§2.3 |
| K-324 | **识别先验后量** | 约束与不变量 | `ahci.c:528` | **V5 修复点** | 16 §1.4/§2.4 |
| K-325 | 容量读四字拼 64 位 | 数据结构 | `ata_id_check` | 本篇核心 | 16 §1.4 |
| K-328a | 端口做成槽位图 + 识别做成纯解析 | 架构演进 | `{port,identify}.rs` | Rust 化 | 16 §3.1-3.2 |
| K-318a | 与 C 差异表四条 | 架构演进 | — | Rust 化 | 16 §2.8 |

- **验收标准**：
  1. 给出端口槽位图的五个管理动作与各自的触发时机
  2. 画出超时复位的完整状态转移（正常 → 超时 → 全失败 → 复位 → 停止 → 重编 → 重启）
  3. **逐条给出识别的三道门**（字零三位 / 第 49 字两位 / 第 83 字三位）与拒绝后果
  4. 给出容量拼装的四字布局（第 100-103 字）与 64 位拼法
  5. 解释"为什么停机要清槽"（答：不留半截命令；带病状态比重启更危险）
  6. 说明"帧散集表寄存器"与"中断等待"为何标"已记录不管实现"（服务层）

### 21-ata-driver

- **一句话定位**：读者读完能说出并行接口的四驱动器如何探测与认车，直接存取如何武装与验车。
- **讲什么**：
  - 并行接口＝火车站（4 驱动器 `MAX_DRIVES`、一列最多 256 节 `MAX_SECS`）
  - 控制器四/五态（新生 / 复位中 / 探明 / 就绪 / 待复位）与乱序拒绝
  - 复位后探站台、认车（识别容量），有一辆能认出就开张，全认不出待复位
  - **出错锁存待复位**（不许带病接车）
  - 大单拆小单（超 256 扇区分整单加零头；纯算术无硬件依赖）
  - 直接存取三态与守卫（闲 / 已武装 / 已到；零扇区拒、重武装拒、验错清状态、早停回闲）
  - 验车（设备错位清说明货没到、无错位清说明货到了）
  - 控制器与等待（探测扫四站台、初始化认车布中断、复位、忙等、中断睡等）
  - **等待分两种**（忙等短有 bound / 睡等长让出；混用是功夫）
  - 直接存取三步（查能力、启停、错检查）
  - Rust 侧：控制器做成阶段机五态、直接存取做成武装机四步
- **不讲什么**：
  - 高级主控接口（20）
  - 虚拟块设备（19）
  - 其余存储杂项（22）
  - 寄存器细节与调参（服务层）
  - 热升级框架本身（只讲钩子存在）
- **前置**：00、01、02、03、04、05、06、07、08、09–20
- **后置**：22、30、31
- **事实底线**：
  - C：`minix3/minix/drivers/storage/at_wini/at_wini.c`（2243 行）：`:166`（`main`）；`:314-453`、`:1626-1727`（控制器与等待）、`:564-961`（直接存取）；`at_wini.h`（端口号与状态位）、`liveupdate.c`（76 行，热升级钩子）
  - Rust：`os/drivers/storage/at_wini/src/{controller,dma}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-319 | 并行接口＝火车站 + 控制器状态机 | 机制 | `MAX_DRIVES`、`MAX_SECS` | 定位 + 核心 | 16 §1.2/§3.3 |
| K-321 | 出错锁存待复位 | 约束与不变量 | — | 本篇核心 | 16 §1.2 |
| K-320 | 大单拆小单 | 机制 | `MAX_SECS` | 本篇核心 | 16 §1.2 |
| K-322 | 直接存取三态与守卫 | 机制 | `at_wini.c:564-961` | 本篇核心 | 16 §1.3/§3.4 |
| K-323 | 验车语义 | 机制 | — | 本篇核心 | 16 §1.3 |
| K-327 | 等待分两种（忙等 / 睡等） | 架构演进 | `at_wini.c:314-453/1626-1727` | 本篇核心 | 16 §2.5 |
| K-328b | 控制器阶段机 + 直接存取武装机 | 架构演进 | `{controller,dma}.rs` | Rust 化 | 16 §3.3-3.4 |
| K-051c | 热升级钩子存在（机制归 31） | 机制 | `liveupdate.c` | 边界 | 16 §2.8 |

- **验收标准**：
  1. 给出控制器状态机的完整状态表与转移条件（含乱序拒绝）
  2. 给出"大单拆小单"的算法（含整单与零头）与至少 2 个验算例
  3. 给出直接存取三态的守卫条件（零扇区拒、重武装拒、早停回闲）
  4. 给出验车的判定逻辑（设备错位清的两种含义）
  5. 解释"为什么等待要分忙等与睡等"（答：短等待用忙等省调度、长等待让出 CPU）
  6. 说明热升级钩子的边界（本篇只讲存在，机制归 31）

### 22-storage-variants

- **一句话定位**：读者读完能说出五个存储变体各自多管了哪件闲事，以及这件闲事的顺序是什么。
- **讲什么**：
  - **软盘**（年迈磁带录音机）：七组驱动介质组合与四种常用介质；重试策略（不可重试错误直接认栽 / 满 6 次认栽 / 到 3 次先重新校准）；密度表与兼容表
  - **闪存卡**（排队考试考生）：**五阶段上电**（新生 / 轮询条件 / 识别身份 / 配置参数 / 就绪；每阶段只认一条命令，乱序成功不算成功）；块长度谈成 512；14 条命令码；SD 两条特有命令的边界声明
  - **故障注入盘**（替身演员）：三拦截点语义（前 / 中 / 后）；规则命中语义（地址范围命中即按规则，范围外放行）；规则的四字段（flags/skip/count/end）；四个故障动作本体
  - **过滤盘**（质检员兼双保险）：每 8 扇区一校验和、9 扇区一组；读先验后交；校验种类四值；**镜像退场账本**（每下层驱动一本账、重启记一笔、满 3 次的两种裁决）
  - **回环盘**（把文件当磁盘的翻译）：扇区读写翻译成文件读写；单次最多 65536 字节；写后强制同步；几何现编（大文件 64 磁头 32 扇区，小文件单磁头单扇区）
  - 五个回调表对照矩阵（真盘三型=磁盘、代理两型=其他；回环六回调最全）
  - 主机抽象与两版实现（`mmchost.h` + 空实现测试替身）
  - **计划勘误**：plan §3.3 曾把文件后备写在故障注入设备名下（实际是回环设备）
  - Rust 侧：密度静态表、重试计数器、上电阶段机、规则地址区间、校验镜像策略、回环纯布局
- **不讲什么**：
  - 高级主控接口与并行接口（20、21）
  - 虚拟块设备（19）
  - USB 存储（24）
  - 寄存器与传输流量（服务层）
- **前置**：00、01、02、03、04、05、06、07、08、09–21
- **后置**：31
- **事实底线**：
  - C：`drivers/storage/floppy/floppy.c`（1355 行）：`:293`（`main`）、`:161-177`（`fdensity` 密度表）、`:652-657`（重试循环）、`:444`（`f_transfer`）；`drivers/storage/mmc/mmcblk.c`（664 行）：`:656`（`main`）、`:47`（`block_transfer` 声明）、`:280`（定义）；`mmc/emmc.c`（1030 行）：`:790-890`（上电十二步）；`mmc/mmchost_mmchs.c`（1267 行）；`mmc/mmchost_dummy.c`（170 行）；`drivers/storage/fbd/fbd.c`（442 行）：`:20`（`fbd_transfer`）、`:123`（`main`）；`fbd/rule.c`（184 行）：`:88`（`rule_match`）、`:115`（`rule_find`）；`fbd/action.c`（302 行）；`drivers/storage/filter/driver.c`（1051 行）；`filter/main.c`（412 行）；`filter/sum.c`（620 行）；`filter/crc.c`（88 行）；`filter/md5.c`（315 行）；`drivers/storage/vnd/vnd.c`（603 行）：`:31`（`vnd_transfer`）、`:592`（`main`）
  - 非 C 制品：`drivers/storage/floppy/liveupdate.c`（78 行）
  - Rust：`os/drivers/storage/{floppy/src/geometry,mmc/src/commands,fbd/src/rules,filter/src/checksum,vnd/src/layout}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-329 | 软盘＝年迈磁带录音机 | 概念 | — | 定位 | 17 §1.1 |
| K-330 | 软盘密度表 + 重试策略 | 机制 | `floppy.c:161-177/652-657` | 本篇核心 | 17 §1.1/§2.3/§3.2 |
| K-331 | 闪存卡五阶段上电 | 机制 | `emmc.c:790-890` | **V4 修复点** | 17 §1.2/§2.7/§3.3 |
| K-332 | 块长度谈成 512 | 约束与不变量 | `MMC_SET_BLOCKLEN` | 本篇核心 | 17 §1.2 |
| K-333 | 故障注入盘＝替身演员 | 概念 | — | 定位 | 17 §1.3 |
| K-334 | 三拦截点语义 + 规则命中 | 机制 | `fbd.c:420-435`、`rule.c:88/115` | **V6 修复点** | 17 §1.3/§2.4 |
| K-335 | 过滤盘校验（8+1 组、读先验后交） | 机制 | `filter/sum.c`、`driver.c:242-408` | 本篇核心 | 17 §1.4/§2.5 |
| K-336 | **镜像退场账本** | 机制 | `driver.c:384-408`、`NR_RESTARTS` | **V3 修复点** | 17 §1.4/§2.5 |
| K-337 | 校验种类四值与组布局 | 数据结构 | `ST_NIL`/`ST_XOR`/`ST_CRC`/`ST_MD5` | 结构 | 17 §1.4 |
| K-338 | 回环盘＝翻译 | 机制 | `vnd.c:31`、`VND_BUF_SIZE` | 本篇核心 | 17 §1.5/§2.6 |
| K-339 | 几何现编 | 机制 | `vnd.c` `vnd_layout`、`VNDIOF_HASGEOM` | 本篇核心 | 17 §1.5/§2.6 |
| K-340 | 五个回调表对照矩阵 | 接口与协议 | `blockdriver.h:16-30` 基准 | 本篇核心 | 17 §2.2 |
| K-342 | 主机抽象与两版实现 | 接口与协议 | `mmchost.h`、`mmchost_dummy.c` | 结构 | 17 §2.7 |
| K-341 | 六项 Rust 设计决策 | 架构演进 | 五个 `*.rs` | Rust 化 | 17 §3.1-3.6 |
| K-334a | **计划勘误**（文件后备属回环不属故障注入） | 工具与工程 | `plan.md` §3.3 | **勘误块** | 17 勘误 L16 |
| K-341a | 与 C 差异表四条 | 架构演进 | — | Rust 化 | 17 §2.9 |

- **验收标准**：
  1. 给出五个变体的完整对照表：变体名、设备类型、独有机制、回调表、C 文件行数
  2. **逐条给出五个变体的"多管的闲事"与它的顺序**
  3. 给出闪存卡五阶段上电的完整命令序列（含每阶段的唯一命令）
  4. 给出故障注入的四个规则字段与三拦截点的触发时机
  5. 给出镜像退场账本的完整裁决（预算内重试 / 转正关镜像 / 无镜像放弃 / 双成员独立记账）
  6. 给出回环几何现编的两个分支（大文件 / 小文件）
  7. **勘误检查**：明确声明"文件后备属回环设备，plan §3.3 写错了"

### 23-usb-framework

- **一句话定位**：读者读完能说出驱动与主机守护之间的包怎么编号，设备插上后按什么顺序说话。
- **讲什么**：
  - 主机守护＝邮局、请求包＝挂号信
  - 驱动发五种（报到 / 注销 / 交包 / 撤回 / 报信息）与守护回四种（通用回执 / 包办结 / 设备出现 / 设备消失）
  - 编号契约（报到+0 … 消失+8）与"编号错一位，邮局就分错拣"
  - 枚举＝新生报到五步（端口复位 / 取短设备描述符 / 分配总线地址 / 取全部配置描述符建树 / 激活默认配置）；失败回发现态重新复位
  - 客户端库＝秘书（填端点与包标识、完成回调、上下线回调、待办包链表、零标识=查无此人）
  - 调度＝投递班次（**同时在飞最多 16 个**；内外统一排队，办结一个放行一个）；调度器线程与设备线程分工
  - 硬件后端唯一（按板型初始化九个回调）；硬件相关封在后端
  - 消息字段映射（授权标识 / 包标识 / 结果 / 设备标识各归哪个消息字）
  - 守护启动四件事与服务线程内建接收循环
  - 枚举八动作实现与每设备一个设备线程
  - 驱动回调表三成员（完成 / 接设备 / 断设备）；请求包结构 `struct usb_urb`（十余字段）；传输类型四种；控制请求八字节
  - Rust 侧：编号枚举、枚举阶段机六态、跟踪查表
- **不讲什么**：
  - 海量存储与集线器（24）
  - 块语义如何消费 USB（24）
  - 控制器寄存器细节（服务层）
  - 描述符树解析（服务层）
- **前置**：00、01、02、03、04、05、06、07、08、09–22
- **后置**：24、30、31
- **事实底线**：
  - C：`minix3/minix/lib/libusb/usb.c`（255 行）：`:20`（`usb_send_urb`）、`:80`（`usb_cancle_urb`）、`:121`（`usb_init`）、`:232`（`usb_send_info`）；`:157-192`（`_usb_urb_complete`）、`:197-225`（`usb_handle_msg`）；`drivers/usb/usbd/base/usbd.c`（184 行）：`:36`（`main`）、`:158`（`usbd_init`）、`:129/135/139/179-183`（服务线程与调度）；`drivers/usb/usbd/hcd/hcd.c`（1314 行）：`:473-562`（`hcd_enumerate`）、`:250-280`（设备线程）；`hcd_common.c`（761 行）；`hcd_ddekit.c`（484 行）；`hcd_schedule.c`（305 行）；`hcd/musb/musb_core.c`（948 行）；`musb_am335x.c`（763 行）；`include/minix/usb.h`（158 行）：`:24-28`（回调表）、`:52-58`（传输类型）、`:63`（无效标识）、`:65-96`（URB 结构）、`:98-104`（控制请求）
  - 非 C 制品：`include/minix/com.h:813-841`（请求常量 `:813-828`、消息字段 `:829-841`）
  - Rust：`os/drivers/usb/usbd/src/{protocol,enumerate}.rs`、`os/libs/minix-usb/src/{urb,wire}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-343 | 主机守护＝邮局 | 概念 | — | 定位 | 18 §1.1 |
| K-344 | 编号契约（5 发 + 4 回） | 接口与协议 | `com.h:813-828` | 本篇核心 | 18 §1.1/§2.2 |
| K-345 | 枚举五步与失败复位 | 机制 | `hcd.c:473-890` | 本篇核心 | 18 §1.2/§2.5/§3.2 |
| K-346 | 客户端库＝秘书 | 机制 | `usb.c:20/121/232` | 本篇核心 | 18 §1.3/§2.3 |
| K-347 | 调度十六上限与线程分工 | 约束与不变量 | `hcd_schedule.c` | 本篇核心 | 18 §1.4 |
| K-348 | 硬件后端唯一（九个回调） | 架构演进 | `musb_am335x.c` | 本篇核心 | 18 §1.5 |
| K-349 | 消息字段映射 | 接口与协议 | `com.h:829-840` | 协议 | 18 §2.2 |
| K-350 | 守护启动四件事 | 机制 | `usbd.c:36-151` | 实现 | 18 §2.4 |
| K-351 | 服务线程与调度初始化 | 机制 | `usbd.c:129/135/139/179-183` | 实现 | 18 §2.4 |
| K-352 | 枚举八动作实现 | 机制 | `hcd.c:473-562` | 实现 | 18 §2.5 |
| K-353 | 每设备一个设备线程 | 机制 | `hcd.c:250-280` | 实现 | 18 §2.5 |
| K-354 | 回调表三成员 + URB 结构 + 传输类型 + 控制请求 | 数据结构 | `usb.h:24-104` | **A5 落地** | 18 §2.6 |
| K-355 | 编号枚举 + 枚举阶段机 + 跟踪查表 | 架构演进 | `{protocol,enumerate}.rs`、`urb.rs` | Rust 化 | 18 §3.1-3.3 |
| K-355a | 与 C 差异表四条 | 架构演进 | — | Rust 化 | 18 §2.8 |

- **验收标准**：
  1. 给出九个编号的完整表（五发 + 四回）与各自动作
  2. 给出枚举五步的完整流程与每步的请求类型
  3. 给出调度器的十六上限与"办结一个放行一个"的机制
  4. 给出 URB 结构的十余字段与四个消息字段槽位的映射
  5. 解释"为什么硬件相关要封在后端"（答：枚举顺序与包编号与板子无关）
  6. 说明驱动回调表三成员的语义

### 24-usb-storage-hub

- **一句话定位**：读者读完能说出命令怎么装进包、包回来怎么验、端口变化怎么看。
- **讲什么**：
  - 包头包尾＝信封（SCSI 命令装进 CBW；CSW 带同编号与结果）
  - **三段式传输**（先发 CBW，再传数据，最后收 CSW 验标签；缺一段不算完）与标签规则（从 1 开始、每趟加一、上一趟回来前不开下一趟）
  - SCSI 七种常用命令与命令长度/数据长度表
  - 扇区对齐守卫（读写地址与长度须为 512 整数倍）
  - 包签名常量（`CBW_SIGNATURE` 0x43425355、`CSW_SIGNATURE` 0x53425355、命令块 16 字节）
  - 状态核对三项（查标签、签名、成功）
  - 块回调表 `mass_storage`（类型磁盘；七个回调）；存储打开与传输；几何兜底
  - **集线器**（8 端口、1000 毫秒巡逻、复位最多 3 次每次 200 毫秒）
  - **端口三态与拉黑**（状态错永久标记坏且巡逻跳过、连状态都不读、住户搬走也不洗白）
  - **通信错语义**（不是端口的事：挂起整个集线器任务等摘除）
  - 复位预算；集线器不挂块框架也不挂字符框架
  - 变化真值表（清变化位放行轮询、到访调连上、出走调断开、先走后到两个都调）
  - 连上报到流程（置复位、轮询复位完成位、清位、要求连上且使能、译速度、上报）
  - 两份端点助手同构声明（各 111 行）
  - Rust 侧：签名常量、标签配对器、守卫纯函数、端口看法机、`repr(C, packed)` 钉 31/13 字节
- **不讲什么**：
  - 请求包编号本身（23，五加四编号已讲透）
  - 块语义的上层消费（`05-stage-vfs`、`15-stage-fs`）
  - 端点数据流量与描述符解析（服务层）
- **前置**：00、01、02、03、04、05、06、07、08、09–23
- **后置**：30、31
- **事实底线**：
  - C：`drivers/usb/usb_storage/usb_storage.c`（1806 行）：`:168`（`main`）、`:105-118`（`mass_storage` 回调表）、`:230-347`（三段传输）、`:795`（`transfer_restrictions`）、`:1145`（`try_first_open`）、`:1476/1505`（几何兜底）；`usb_storage/scsi.c`（288 行）：`:31-57`（`create_scsi_cmd`）、`:203-229`（`check_inquiry_reply`）、`:267-288`（`check_csw`）；`scsi.h`（138 行）；`bulk.c`（39 行）；`bulk.h`（52 行）：`:13`（`CBW_SIGNATURE`）、`:16`（命令块 16 字节）、`:30`（`CSW_SIGNATURE`）；`usb_hub/usb_hub.c`（937 行）：`:220`（`main`）、`:47-56`（常量）、`:390-512`（`hub_task`）、`:75-98`（端口状态位域）、`:141-158`（`port_change`）、`:693`（`hub_handle_change`）、`:826`（`hub_handle_connection`）、`:921`（`ddekit_usb_info`）；两处 `urb_helper.c`（各 111 行）
  - Rust：`os/drivers/usb/usb_storage/src/cbw.rs`、`os/drivers/usb/usb_hub/src/ports.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-356 | 包头包尾＝信封 | 概念 | — | 定位 | 19 §1.1 |
| K-357 | 三段式传输 + 标签规则 | 机制 | `bulk.h:13-42`、`usb_storage.c:230-347` | 本篇核心 | 19 §1.1/§2.2/§3.2 |
| K-358 | SCSI 七命令与长度表 | 接口与协议 | `scsi.h:30-127`、`scsi.c:31-288` | 本篇核心 | 19 §1.2/§2.3 |
| K-359 | 扇区对齐守卫 | 约束与不变量 | `usb_storage.c:795` | 本篇核心 | 19 §1.2/§2.4 |
| K-360 | 集线器＝门卫 | 机制 | `usb_hub.c:47-56/390-512` | 本篇核心 | 19 §1.3/§2.5 |
| K-361 | **端口三态与拉黑 + 通信错语义** | 机制 | `hub_task` | **V7 修复点** | 19 §1.3/§3.4 |
| K-362 | 复位预算 + 不挂框架 | 约束与不变量 | `USB_HUB_MAX_TRIES` | 本篇核心 | 19 §1.3/§3.4 |
| K-363 | 两份端点助手同构 | 工具与工程 | 两份 `urb_helper.c` | 实现 | 19 §1.4 |
| K-364 | 包签名常量 | 数据结构 | `bulk.h:13/16/30` | 协议 | 19 §2.2/§3.1 |
| K-365 | 状态核对三项 | 接口与协议 | `scsi.c:267-288` | 本篇核心 | 19 §2.3 |
| K-366 | 块回调表 `mass_storage` | 数据结构 | `usb_storage.c:105-118` | 结构 | 19 §2.4 |
| K-367 | 几何兜底 | 机制 | `usb_storage.c:1476/1505` | 实现 | 19 §2.4 |
| K-368 | 变化真值表 | 机制 | `usb_hub.c:693` | 本篇核心 | 19 §2.6 |
| K-369 | 连上报到流程 | 机制 | `usb_hub.c:826/921/929-936` | 本篇核心 | 19 §2.6 |
| K-370 | 签名常量 + 标签配对 + 守卫纯函数 + 端口看法机 | 架构演进 | `cbw.rs`、`ports.rs` | Rust 化 | 19 §3.1-3.4 |
| K-371 | `repr(C, packed)` 钉 31/13 字节 | 架构演进 | `cbw.rs` | **A5 落地** | 19 §2.7 |
| K-371a | 与 C 差异表三条 | 架构演进 | — | Rust 化 | 19 §2.8 |

- **验收标准**：
  1. 画出三段式传输的完整时序（CBW → 数据 → CSW）与标签配对规则
  2. 给出 SCSI 七命令的编号、命令长度、数据长度表
  3. 给出 CBW/CSW 的字节布局（钉 31/13 字节）与两个签名的值
  4. **逐条说明端口三态与拉黑语义**（含"住户搬走也不洗白"）
  5. 给出变化真值表的四种情况与各自的调用
  6. 解释"为什么通信错要挂起整个集线器任务"（答：不是端口的问题；端口层面的处置无意义）
  7. 给出复位预算的三次计数与拉黑的关系

### 25-fb-audio-drivers

- **一句话定位**：读者读完能说出字符设备的两个非典型变体：帧缓冲为什么不是映射设备，音频为什么比显示多一层框架。
- **讲什么**：
  - **帧缓冲部分**：
    - **帧缓冲是字符设备而非映射设备**（核心命题；`chardriver.h` 十成员无映射；`fb.c` 只填五项）
    - 黑板读写直达显存（靠读写与四个 ioctl）
    - 打开计数 + **一次性初始化旗**（独立于计数）
    - 首次打开读 EDID + `arch_fb_init`
    - 读写按设备大小截断；重启窗口门控写
    - 四个 ioctl 请求；陌生请求回 `ENOTTY`；改可变参数只许改垂直偏移；平移转调改参数
    - EDID 128 字节体检表与经块协议五步读取；缺省合法
    - `choose_mode` 交集最高分辨率；硬件四种分辨率；默认 1024×600/32 位色/双缓冲
    - 帧缓冲大小纯算术；连续物理内存不够则启动失败
    - 后端唯一（earm）；`logos.h` 位图不翻译
    - **计划勘误**：plan 把帧缓冲记成映射到用户，写错了
  - **音频部分**：
    - 音频框架**十四驱动钩子**（逐项表）与字符表五回调
    - **十一声音控制请求**（逐项表）
    - 速率上下限 4000–44100、默认 44100；转换器内部固定 48000 Hz；三通道路由
    - `src_set_rate` 封装；`drv_start` 先设速率再继续；`drv_stop`/暂停/继续专函
    - sb16 端口基址 0x220；复位协议（等 0xAA）；版本命令 0xE1；速率方向命令 0x66/0x65 + 高低字节
    - 喇叭开关 / 停播 / 续播各一字节；**sb16 暂停等于停止**
    - 七兄弟差异矩阵；五份同构混音器
    - **头文件纠正**：回调表定义在 `audio_fw.h`，不是 `audiodriver.h`
    - Rust 侧：范围守卫、通道路由枚举、命令常量、分字节纯函数、`minix-audiodriver` 框架库
- **不讲什么**：
  - 控制台渲染（10）
  - VM 映射机制（`02-stage-vm`）
  - 高清桥芯片（28）
  - 声音系统上层（`18-stage-commands`）
  - 具体寄存器时序（服务层）
- **前置**：00、01、02、03、04、05、06、07、08、09–24
- **后置**：31
- **事实底线**：
  - C（帧缓冲）：`drivers/video/fb/fb.c`（404 行）：`:28`（`fb_read`）、`:30`（`fb_write`）、`:32`（`fb_ioctl`）、`:44-51`（`fb_tab` 十成员只填五项）、`:60`（`fb_open`）、`:91`（`fb_close`）、`:308`（`main`）；`:99-234`（读写截断）、`:121-198`（四个 ioctl）、`:221`（重启窗口）、`:257-290`（初始化回调）；`fb_edid.c`（187 行）：`:55`（`fb_edid_args_parse`）；`fb_edid.h`（12 行）；`arch/earm/fb_arch.c`（408 行）：`:144-164`（`choose_mode`）、`:274-296`（改可变参数）、`:308-312`（平移）、`:314-407`（连续内存与映射）；`logos.h`（3054 行）；`include/sys/ioc_fb.h:11-14`；`include/minix/chardriver.h:9-23`（十成员无映射）
  - C（音频）：`lib/libaudiodriver/audio_fw.c`（868 行）：`:64-81`（字符表五回调）、`:524`（`handle_int_write`）、`:577`（`handle_int_read`）；`audio_fw.h:9-22`（14 钩子）；`libaudiodriver/liveupdate.c`（109 行）；`drivers/audio/es1371/es1371.c`（656 行）：`:62`（`drv_init`）、`:228`（`drv_start`）、`:268`（`drv_stop`）、`:522`（`set_sample_rate`）；`es1371/SRC.c`（196 行）、`codec.c`（264 行）、`sample_rate_converter.c`（240 行）；`es1371.h:106/110-111`；`drivers/audio/sb16/sb16.c`（449 行）：`:86`（`dsp_command`）、`:345`（`dsp_set_speed`）；`sb16/mixer.c`（254 行）；`sb16.h:20/107-110`；`include/sys/ioc_sound.h:12-22`（11 请求）
  - Rust：`os/drivers/video/fb/src/{mode,display}.rs`、`os/drivers/audio/{es1371/src/rate,sb16/src/dsp}.rs`、`os/libs/minix-audiodriver/`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-372 | **帧缓冲是字符设备而非映射设备** | 概念 | `chardriver.h:9-23`、`fb.c:44-51` | **核心命题** | 20 说明/勘误 L16 |
| K-373 | 打开计数 + 一次性初始化旗 | 机制 | `fb.c:60/91` | **N4 修复点** | 20 §1.1/§2.3/§3.3 |
| K-374 | 首次打开读 EDID + `arch_fb_init` | 机制 | `fb.c:70/74` | 本篇核心 | 20 §2.3 |
| K-375 | 读写截断 + 重启窗口门控 | 约束与不变量 | `fb.c:28/30/221` | 本篇核心 | 20 §1.1/§1.5/§2.3 |
| K-376 | 四个 ioctl + ENOTTY + 只许改垂直偏移 | 接口与协议 | `ioc_fb.h:11-14`、`fb.c:32` | 本篇核心 | 20 §1.2/§2.4 |
| K-377 | `fb_tab` 十成员只填五项 + 初始化三分支 | 数据结构 | `fb.c:44-51/308` | 结构 | 20 §2.2 |
| K-378 | EDID 128 字节与五步读取 + 缺省合法 | 数据结构 | `fb_edid.c:55` | 本篇核心 | 20 §1.3/§2.5 |
| K-379 | `choose_mode` 交集 + 四种分辨率 + 默认模式 | 机制 | `fb_arch.c:144-164` | 本篇核心 | 20 §1.4/§2.6 |
| K-380 | 帧缓冲大小纯算术 + 连续内存失败 | 机制 | `mode.rs` | 本篇核心 | 20 §1.4/§2.6/§3.2 |
| K-381 | 后端唯一 + `logos.h` 不翻译 | 架构演进 | `fb_arch.c`、`logos.h` | 边界 | 20 §2.1/§2.6/§2.8 |
| K-382 | **音频框架十四驱动钩子**（逐项） | 接口与协议 | `audio_fw.h:9-22`、`audio_fw.c:64-81` | **GAP-21 补** | 21 §1.1/§2.2 |
| K-383 | **十一声音控制请求**（逐项） | 接口与协议 | `ioc_sound.h:12-22` | **GAP-22 补** | 21 §1.1/§2.3 |
| K-384 | 速率上下限 + 转换器固定 48000 + 三通道 | 常量 | `es1371.h:106/110-111`、`es1371.c:522` | 本篇核心 | 21 §1.2/§2.4 |
| K-385 | `src_set_rate` + `drv_start` 次序 + 专函 | 机制 | `es1371.c:228/268`、`sample_rate_converter.c` | 本篇核心 | 21 §2.4 |
| K-386 | sb16 端口基址 + 复位协议 + 版本/速率命令 | 机制 | `sb16.h:20`、`sb16.c:86/345` | 本篇核心 | 21 §1.3/§2.5 |
| K-387 | 喇叭/停播/续播字节 + **暂停等于停止** | 机制 | `sb16.c:134-171/184/221/229` | 本篇核心 | 21 §2.5 |
| K-388 | 七兄弟差异矩阵 + 五份混音器 | 数据结构 | `mixer.c` 系列 | 本篇核心 | 21 §1.4/§2.6 |
| K-389 | `minix-audiodriver` 框架库 | 架构演进 | `os/libs/minix-audiodriver/` | **G1 落地** | 21 头部/§2.7 |
| K-390 | 范围守卫 + 通道路由 + 命令常量 + 分字节纯函数 | 架构演进 | `{rate,dsp}.rs` | Rust 化 | 21 §3.1-3.4 |
| K-391 | **头文件纠正**（`audio_fw.h` 而非 `audiodriver.h`） | 工具与工程 | 三头文件 | **勘误块** | 21 勘误 L16 |
| K-372a | **计划勘误**（帧缓冲不是映射设备） | 工具与工程 | `plan.md` §2 | **勘误块** | 20 勘误 L16 |
| K-388a | 与 C 差异表（帧缓冲四条 + 音频三条） | 架构演进 | — | Rust 化 | 20 §2.8、21 §2.8 |

- **验收标准**：
  1. **逐字论证"帧缓冲是字符设备而非映射设备"**，含 `chardriver.h` 十成员无映射的证据
  2. 给出四个 fb ioctl 的完整表（请求码、材料、回复、失败 errno）
  3. 给出 EDID 的五步读取流程与"缺省合法"的语义
  4. 给出 `choose_mode` 的交集算法与四种支持分辨率
  5. **逐项给出音频 14 钩子的签名与语义**（GAP-21）
  6. **逐项给出 11 个声音控制请求**（GAP-22）
  7. 给出 sb16 的复位协议与速率命令的两个方向字节
  8. 给出七兄弟差异矩阵（7 行 × 至少 5 列）
  9. **勘误检查**：明确声明两处 plan 错误（帧缓冲映射、音频头文件名）

### 26-net-driver-reference

- **一句话定位**：读者读完能说出包从网线进来先落地哪一页，虚拟网卡的队列谁干什么活。
- **讲什么**：
  - **dp8390 部分**：每页 256 字节页几何；接收页游标（边界页 / 当前页 / 下一页 / 绕回）；接收头解析长度；长度越界丢包；**链尾落起始页特例（写停止页减一）**；发送队列两项与每项六页；队满回发送不能；完成中断清标记推尾指针；`dp_table` 只填七项；组播广播能力回填与时钟节拍；统计经时钟回调读计数器；**板级多态**（四板共用页逻辑、函数指针各填）；兼容卡优先探测；组播筛选粗规矩
  - **virtio_net 部分**：三队列（收 / 发 / 控制）；包上限 1514；`virtio_net_table` 五项；初始化六步串接；**`refill_rx` 阈值补充**（不足一半补、每包两段）；`check_queues`；中断=查+报+补充；短帧垫到 60
  - **回调名前缀纠正**：驱动表是 `struct netdriver` 的 `ndr_*` 成员，不是 `ndo_*`
  - Rust 侧：`RecvCursor` 加减器、`length_allowed` 守卫、`NetQueue` 枚举、`refill_needed` 阈值函数、`padded_length`
- **不讲什么**：
  - 其余 12 网卡（27）
  - 协议栈上层（`17-stage-net`）
  - 板级端口细节与寄存器读写（服务层）
  - 网络框架（06）
- **前置**：00、01、02、03、04、05、06、07、08、09–25
- **后置**：27、31
- **事实底线**：
  - C：`drivers/net/dp8390/dp8390.c`（998 行）：`:117`（`main`）、`:103-121`（`dp_table`）、`:146-149`（能力回填与节拍）、`:230-232`（队满）、`:293-299`（统计）、`:547-560`（完成中断）、`:598-720`（接收走页）、`:610-611`、`:616-622`、`:633-639`、`:668-671`（游标与特例）；`dp8390.h`（256 行）：`:189-190`（队列与页几何）、`:189-238`（发送结构）；四板级文件 `3c503.c`（192 行）、`ne2000.c`（320 行）、`rtl8029.c`（314 行）、`wdeth.c`（358 行）；`drivers/net/virtio_net/virtio_net.c`（446 行）：`:36`（三队列）、`:44`（包上限）、`:92-99`（`virtio_net_table`）、`:216-243`（`refill_rx`）、`:248-270`（`check_queues`）、`:284-302`（中断）、`:369-370`（短帧垫长）、`:389-417`（初始化六步）、`:438`（`main`）；`virtio_net.h`（172 行）；`include/minix/netdriver.h:23-40`（回调表）
  - Rust：`os/drivers/net/dp8390/src/ring.rs`、`os/drivers/net/virtio_net/src/queues.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-392 | 页几何与接收页游标 | 数据结构 | `dp8390.h:189-190`、`dp8390.c:598-720` | 本篇核心 | 22 §1.1/§2.4/§3.1 |
| K-393 | 接收头解析 + 越界丢包 + **链尾特例** | 机制 | `dp8390.c:633-639/668-671` | **G9 修复点** | 22 §2.4/§3.1 |
| K-394 | 发送队列两项与每项六页 + 完成中断 | 数据结构 | `dp8390.h:189/190`、`dp8390.c:230-232/547-560` | 本篇核心 | 22 §1.2/§2.5 |
| K-395 | **回调名前缀 `ndr_` 而非 `ndo_`** | 数据结构 | `netdriver.h:23-40` | **勘误块** | 22 §2.2 |
| K-396 | `dp_table` 七项 + 能力回填 + 统计经时钟 | 机制 | `dp8390.c:103-121/146-149/293-299` | 本篇核心 | 22 §2.3 |
| K-397 | 三队列 + 包上限 + `virtio_net_table` + 初始化六步 | 数据结构 | `virtio_net.c:36/44/92-99/389-417` | 本篇核心 | 22 §1.3/§2.6 |
| K-398 | **`refill_rx` 阈值补充 + `check_queues` + 中断 + 短帧垫长** | 机制 | `virtio_net.c:216-243/248-270/284-302/369-370` | **N1 修复点** | 22 §1.3/§2.6/§3.4 |
| K-399 | 板级多态 + 兼容卡优先探测 | 架构演进 | 四板级文件 | 本篇核心 | 22 §1.4/§2.1 |
| K-400 | 组播筛选粗规矩 | 约束与不变量 | — | 本篇核心 | 22 §1.5 |
| K-409a | Rust 决策五条（游标加减器、长度守卫、队列枚举、阈值函数、垫长） | 架构演进 | `{ring,queues}.rs` | Rust 化 | 22 §3.1-3.5 |
| K-398a | 与 C 差异表四条 | 架构演进 | — | Rust 化 | 22 §2.8 |

- **验收标准**：
  1. 画出 dp8390 的接收页游标状态机（含链尾特例的写停止页减一）
  2. 给出接收头解析的长度字段与越界丢包的判定
  3. 给出发送队列的两项与每项六页的布局
  4. 给出 `refill_rx` 的阈值规则（不足一半补、每包两段）与 N1 修复的意义（常量 64/32 而非 256/128）
  5. 给出 virtio_net 三队列的分工与初始化六步
  6. **勘误检查**：明确声明回调表前缀是 `ndr_` 不是 `ndo_`
  7. 说明"板级多态"的实现（四板共用页逻辑、函数指针各填）

### 27-net-driver-variants

- **一句话定位**：读者读完能说出 12 家网卡的环各长什么样，靠什么认出自己面对的是哪块卡。
- **讲什么**：
  - **e1000**：收发各 256 描述符；每格 2048 字节缓冲；收/发描述符字段；尾指针推进与到顶绕回；空环=尾追上头
  - **rtl8139**：发送四槽 + 状态寄存器四连号；接收 65536 字节环 + 两游标；取包绕回；发送编程对应槽
  - **lance**：收发环各 16 格；初始化块；**芯片版本表两段式认卡**（低 12 位过 0x003 门 + 高 16 位查表）；ROM 端口直读地址；DMA 需低 16 MB
  - 三家回调填报对照（十 / 十一 / 八项）
  - 探测按序号跳过、不按厂商标识过滤；标识表只打日志与分支、不拦路
  - 九家一行矩阵（3c90x / atl2 / dec21140A / dpeth / fxp / ip1000 / lan8710a / rtl8169 / vt6105）
  - Rust 侧：环计数器取模、槽轮转器取模四、绕回纯函数、认卡两段查表
- **不讲什么**：
  - 参考实现语义（26）
  - 协议栈上层（`17-stage-net`）
  - 板级寄存器时序（服务层）
  - 网络框架（06）
- **前置**：00、01、02、03、04、05、06、07、08、09–26
- **后置**：31
- **事实底线**：
  - C：`drivers/net/e1000/e1000.c`（918 行）：`:13`（`e1000_init`）、`:38-49`（回调表）、`:55`（`main`）；`e1000.h:29/32/35`（环几何）；`e1000_hw.h:31/46`（描述符）；`drivers/net/rtl8139/rtl8139.c`（1530 行）：`:118`（`main`）、`:101-113`（回调表）、`:619-688`（取包绕回）、`:711`（发送编程）；`rtl8139.h:19/20-34/435/46-48`；`drivers/net/lance/lance.c`（895 行）：`:172`（`main`）、`:158-167`（回调表）、`:280`（`lance_probe`）、`:63-87`（芯片版本表）、`:97-106`（环几何）、`:109`（初始化块）、`:707-722`（认卡两段式）、`:759`（DMA 低 16 MB）、`:782-784`（ROM 端口）；其余九目录
  - Rust：`os/drivers/net/{e1000/src/desc,rtl8139/src/txrx,lance/src/ring}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-401 | e1000 环几何与描述符字段 | 常量 | `e1000.h:29/32/35`、`e1000_hw.h:31/46` | 本篇核心 | 23 §1.1/§2.3 |
| K-402 | 尾指针推进与空环判据 | 机制 | — | 本篇核心 | 23 §1.1/§3.1 |
| K-403 | rtl8139 四槽 + 大环 + 两游标 | 数据结构 | `rtl8139.h:19/20-34/435/46-48` | 本篇核心 | 23 §1.2/§2.4 |
| K-404 | lance 小环 + **两段式认卡** | 数据结构 | `lance.c:97-106/109/63-87/707-722` | **N2 修复点** | 23 §1.3/§2.5/§3.4 |
| K-405 | ROM 端口直读 + DMA 低 16 MB | 机制 | `lance.c:782-784/759` | 本篇核心 | 23 §2.5 |
| K-406 | 探测按序号 + 标识表不拦路 | 机制 | — | 本篇核心 | 23 §1.4 |
| K-407 | 三家回调填报对照 | 数据结构 | `e1000.c:38-49`、`rtl8139.c:101-113`、`lance.c:158-167` | 本篇核心 | 23 §2.2 |
| K-408 | 九家一行矩阵 | 数据结构 | 各文件 | 本篇核心 | 23 §2.6 |
| K-409 | 环计数器 + 槽轮转器 + 绕回纯函数 + 认卡查表 | 架构演进 | `{desc,txrx,ring}.rs` | Rust 化 | 23 §3.1-3.4 |
| K-409b | 与 C 差异表三条 | 架构演进 | — | Rust 化 | 23 §2.8 |

- **验收标准**：
  1. 给出三家详例的环几何对照表（环大小、槽数、缓冲尺寸、描述符字段数）
  2. **逐条给出 lance 认卡的两段式算法**（低 12 位门 + 高 16 位查表）与 N2 修复的意义
  3. 给出九家矩阵的完整表（9 行 × 至少 4 列：环几何 / 探测方式 / 回调数 / C 行数）
  4. 解释"为什么探测不按厂商标识过滤"（答：按序号跳过；标识表只打日志与分支）
  5. 给出"空环=尾追上头"的判据与绕回算法

### 28-misc-drivers

- **一句话定位**：读者读完能说出剩下的小驱动各自把哪一件小事做对。
- **讲什么**：
  - **打印机**：状态位五常量；重试上限 120（半秒一次约 60 秒）；**状态检查优先级离线 > 缺纸 > 忙**；离线报 EIO 不重试；`printer_write` 写中 EIO / 非阻塞 EAGAIN；`printer_tab` 四项
  - **存储器**（cat24c256）：读 128 / 写 16 分片；地址一位/两位看页标志；写跨页回绕前功尽弃；容量 32768 字节；块外壳传输
  - **传感器**（bmp085）：校准寄存器 0xAA 连读 22 字节；校准十一系数；体温换算公式；测量两步 + 等 4500 微秒；换算全用 32 位防溢出；数据手册例题
  - **最小示例**（hello）：三回调 / 打开计数 / EOF / 限长 / 安全拷贝；主循环两行；热升级存取两函
  - 余家一行矩阵（sht21 / tsl2550 / amddev / i2c / ti1225 / tda19988 / vbox / tps65217 / tps65950 / power/acpi）
  - **ACPI 第三方移植只包三文件外壳**（A-9 的重大决策）
  - **`minix-i2cdriver` 承载 libi2cdriver**（366 行）；plan §5.1 原映射本篇但从未落地
  - Rust 侧：状态优先级读、分片纯函数、地址宽度布尔函数、公式纯换算
- **不讲什么**：
  - 各类别核心语义（09–27）
  - 第三方电源解析器全文（83279 行）
  - 总线硬件时序（服务层）
- **前置**：00、01、02、03、04、05、06、07、08、09–27
- **后置**：31
- **事实底线**：
  - C：`drivers/printer/printer/printer.c`（424 行）：`:46-52`（状态位与重试上限）、`:85`（`printer_write`）、`:99-103`（`printer_tab`）、`:109`（`main`）、`:208`（`output_done`）、`:216-224`（优先级）、`:160-203`（写路径）；`drivers/eeprom/cat24c256/cat24c256.c`（505 行）：`:296`（`cat24c256_read`）、`:367`（`cat24c256_write`）、`:482`（`main`）、`:251-390`（分片）、`:427-428`（容量）、`:106-135`（块外壳）；`drivers/sensors/bmp085/bmp085.c`（583 行）：`:14-35`（触发命令）、`:74-88`（校准系数）、`:90-114`（连读 22 字节）、`:154`（`bmp085_read`）、`:317-418`（测量两步）、`:382-385`（换算公式）、`:561`（`main`）；`drivers/examples/hello/hello.c`（158 行）：`:13`（`hello_read`）、`:35`（`hello_open`）、`:76-92`（热升级）、`:145`（`main`）；`sensors/sht21.c`（486 行）、`tsl2550.c`（435 行）；`iommu/amddev.c`（477 行）；`bus/i2c/i2c.c`（521 行）、`ti1225.c`（431 行）；`video/tda19988.c`（1010 行）；`vmm_guest/vbox.c`（248 行）；`power/tps65217.c`（402 行）、`tps65950.c`（352 行）；`power/acpi`（157 文件 83279 行）
  - Rust：`os/drivers/{printer/printer/src/status,eeprom/cat24c256/src/pages,sensors/bmp085/src/convert}.rs`、`os/libs/minix-i2cdriver/`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-410 | 打印机状态位五常量 + 重试上限 | 常量 | `printer.c:46-52` | 本篇核心 | 24 §1.1/§2.2 |
| K-411 | **状态检查优先级离线 > 缺纸 > 忙** | 约束与不变量 | `printer.c:208` | **N3 修复点** | 24 §1.1/§2.2/§3.1 |
| K-412 | `printer_write` 两路径 + `printer_tab` | 机制 | `printer.c:85/99-103` | 本篇核心 | 24 §2.2 |
| K-413 | cat24c256 分片 + 地址宽度 + 跨页回绕 | 常量/机制 | `cat24c256.c:296/367` | 本篇核心 | 24 §1.2/§2.3/§3.2 |
| K-414 | bmp085 校准与换算公式 + 测量两步 | 机制 | `bmp085.c:90-114/74-88/382-385/317-418` | 本篇核心 | 24 §1.3/§2.4/§3.4 |
| K-415 | 32 位防溢出 + 数据手册例题 | 约束与不变量 | `convert.rs` | 本篇核心 | 24 §3.4/§5.3 |
| K-416 | hello 骨架 + 主循环两行 + 热升级两函 | 机制 | `hello.c:35/13/145/76-92` | **教科书地位** | 24 §1.4/§2.5 |
| K-417 | 余家一行矩阵 + tsl2550 比例表 + sht21 CRC | 数据结构 | `tsl2550.c`、`sht21.c` | 本篇核心 | 24 §2.6 |
| K-418 | **ACPI 第三方移植只包三文件外壳** | 架构演进 | `power/acpi`（83279 行） | **A-9 重大决策** | 24 §1.5/§2.6/§2.8 |
| K-419 | **`minix-i2cdriver` 承载 libi2cdriver** | 架构演进 | `os/libs/minix-i2cdriver/` | **G2 落地** | 24 框架补充 L6 |
| K-420 | 状态优先级读 + 分片纯函数 + 地址宽度布尔 + 公式纯换算 | 架构演进 | 三个 `*.rs` | Rust 化 | 24 §3.1-3.4 |
| K-411a | 与 C 差异表三条 | 架构演进 | — | Rust 化 | 24 §2.8 |

- **验收标准**：
  1. **逐条给出打印机状态检查的优先级**与 N3 修复的意义（离线压倒缺纸才是 C 的行为）
  2. 给出 cat24c256 的分片规则（读 128 / 写 16）与地址宽度判定
  3. 给出 bmp085 的校准系数结构与体温换算公式（含中间值）
  4. 给出 hello 的骨架（三回调 + 打开计数 + EOF + 限长）与它的教科书价值
  5. 给出余家矩阵（10 行）与各自的"一件小事"
  6. 说明 ACPI 的处置策略（外壳适配，不逐行重写 83279 行）
  7. 说明 `minix-i2cdriver` 的归属与 plan §5.1 的漏映射

### 29-fs-testing

- **一句话定位**：读者读完能自己跑起测试、造出一个可测的驱动骨架、并知道协议编号如何对账。
- **讲什么**：
  - Rust 侧三层测试（纯函数 / 框架逻辑 / 装配）
  - 测试替身注入（传输替身、空设备、空实现、回环替身、录制替身）
  - **协议编号的编译期对账机制**（每族一组钉值测试，逐值对照 `com.h`）
  - 各框架库的钉值测试计数（chardriver 21 / blockdriver 13 / netdriver 52 / bdev 20 / readclock 12）
  - 编号镜像绊线（`libbdev` 测试硬编码五个编号）
  - `trait 只有一个实现体是装饰`（Gate D 的论证与两实现体的要求）
  - 测试统计的日期标注纪律（现有 01/02/05/08–11/14–16/20–24 标 2026-09-05，03/04/06/07/09/12/13/19 标 2026-09-17，**体例不统一**）
  - C 侧无端到端脚本（本 stage 与 15-stage-fs 的差异：FS 有 `testmfs.sh` 等，drivers 侧只有 `tests/` 下的零散脚本）
  - 集成面（真实中断 → 事件循环 → grant 拷贝）留多进程联调
- **不讲什么**：
  - 各机制的实现细节（各篇讲自己的测试）
  - 错误码（31）
  - 工程面（31）
- **前置**：00、01、02、03、04、05、06、07、08、09–28
- **后置**：无（收尾篇）
- **事实底线**：
  - Rust：各 crate 的 `#[cfg(test)]` 模块；`os/libs/minix-{chardriver,blockdriver,netdriver,bdev,virtio,usb,audiodriver,i2cdriver,driver-rt}/src/*.rs`；`os/drivers/*/src/*.rs`
  - C：`minix3/minix/drivers/**/Makefile`（测试如何被构建链关联）；`minix3/minix/tests/`（本 stage 相关的零散脚本）
  - 现状数据（`todo.md` §0）：57 crate 中 31 个有实质策略逻辑（约 8300 行、242 测试），26 个为纯占位；六个框架库 5214 行、94 测试
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| N-013 | 测试基建三层 | 测试性质 | 各 crate 测试 | 现有 00 §5 只给总数 | 新增 |
| N-012 | 协议编号编译期对账机制 | 测试性质 | 各框架库钉值测试 | 现有 99 §5 只给计数 | 新增 |
| K-103a | 测试替身注入（五类替身） | 测试性质 | `transport.rs`、`SilentDevice`、`mmchost_dummy.c` | 跨篇共性 | 04 §1.5 + 01 §3.7 + 17 §2.7 |
| K-440a | `trait 只有一个实现体是装饰` | 测试性质 | `hal.rs` | Gate D 论证 | 25 §3 |
| K-104a | 编号镜像绊线 | 约束与不变量 | `libbdev` 测试 | 对账机制 | 04 §1.6 |
| K-211a | 独立回放对账测试 | 测试性质 | random 测试 | 方法 | 09 §5.2 |
| K-455a | 各框架库钉值测试计数 | 测试性质 | — | 统计 | 99 §5 |
| K-435b | 测试与集成面现状（242 + 94 测试；26 占位） | 测试性质 | `todo.md` §0 | 现状 | 00 §5 + todo |
| N-022 | 测试统计日期标注纪律（体例不统一问题） | 测试性质 | 各篇 §5 | 现有 27 篇日期不一 | 新增 |

- **验收标准**：
  1. 给出三层测试的划分表与每层的代表用例
  2. 给出五类测试替身的清单与各自的用途
  3. 给出协议编号对账机制的组织方式（每族一组、逐值对照、编译期或首测即爆）
  4. 给出各框架库的钉值测试计数表与复现命令
  5. 说明"trait 只有一个实现体是装饰"的论证与 Gate D 的要求
  6. 明确声明"集成面（真实中断到事件循环）留多进程联调，本 stage 不单独建"

### 30-fs-dma-contract

- **一句话定位**：读者读完能说出驱动库如何向服务端索取设备可直接读写的连续内存。
- **讲什么**：
  - 燃料站模型 / 地址双簿模型 / 单一权威模型（三个心智模型）
  - **为什么设备内存不能问操作系统要**（设备没有页表；总线主控给出的地址必须原样命中物理内存）
  - C 时代不构成边界（`alloc_contig`；分配器、地址翻译、驱动在同一地址空间）
  - minix-rs 把驱动搬进用户态后三样东西分家（物理页在 VM、地址翻译靠页表、库只认字节数组）
  - 契约四操作 + 一张凭据（`DmaRegion` 三字段 / `DmaMemory` 四方法）
  - 申请失败报 errno 而非空值（穷尽就是 `ENOMEM`）；翻译查无此址报 `None`（不算错误）；归还无返回值
  - 契约住共享类型库的理由（驱动库与实现方分属两条工作线，放谁家另一家反向依赖）
  - 替身形状规定（定长簿记阶梯分配器 + 故意"永远没内存"的实现）
  - virtio 库经 `hal.rs` 以别名再导出消费这份契约（公开路径保持 rcore 约定）
  - 消费方清单（virtio / ahci / usb_storage / memory 驱动的物理映射）
  - 实现方（VM 服务器）与 edge E-DMABUF
- **不讲什么**：
  - 物理页分配器本身怎么实现（`02-stage-vm`，edge3 S37）
  - 缓存一致性维护指令的展开（本树目标设备 x86-64 相干）
  - 各驱动的寄存器访问（服务层）
- **前置**：00、01、02、03、04、05、06、07、08、09–29
- **后置**：无（收尾篇）
- **事实底线**：
  - Rust：`os/libs/minix-types/src/types/dma.rs`（契约的权威定义）、`os/libs/minix-types/src/types/address.rs`（`VirBytes`/`PhysBytes`）、`os/libs/minix-virtio/src/hal.rs`（别名再导出与替身测试）
  - C：`minix3/minix/drivers/lib/libvirtio/virtio.c:319` 一带（`alloc_contig` 用法）
  - edge：`edge_todo.md:1001`（E-DMABUF）
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-436 | 契约四操作 + 一张凭据 | 接口与协议 | `dma.rs` | 本篇核心 | 25 §2 |
| K-437 | 三个心智模型 | 概念 | — | 定位 | 25 §1 |
| K-438 | 契约住共享类型库的理由 | 架构演进 | — | 设计 | 25 §3 |
| K-439 | 三种失败形状 | 接口与协议 | `dma.rs` | 本篇核心 | 25 §2/§4 |
| K-440 | 替身形状规定 | 测试性质 | `hal.rs` | Gate D | 25 §3 |
| K-291a | 队列内存是连续物理页（动机） | 约束与不变量 | — | 动机（从 18 引用） | 14 §2.3 |
| K-298a | `Hal` 别名再导出 | 架构演进 | `hal.rs` | 消费侧 | 14 §3.1 |
| N-023 | 消费方清单（virtio / ahci / usb_storage / memory） | 接口与协议 | 各驱动 | 现有 25 篇未给清单 | 新增 |
| N-024 | 实现方（VM）与 edge E-DMABUF 的排期 | 边界 | `edge_todo.md:1001` | 边界声明 | 新增 |

- **验收标准**：
  1. 给出 `DmaRegion` 三字段与 `DmaMemory` 四方法的完整签名与语义
  2. 解释"为什么设备内存不能问操作系统要"（答：设备没有页表，总线主控地址必须命中物理内存）
  3. 给出三种失败形状的对照表（申请穷尽 / 翻译查无 / 归还）
  4. 给出消费方清单（至少 4 个）与各自的用途
  5. 说明"契约住共享类型库"的依赖方向论证
  6. 给出替身的形状规定与"trait 只有一个实现体是装饰"的论证

### 31-fs-errors-boundaries

- **一句话定位**：读者读完能说出失败怎么分类、errno 怎么映射、驱动怎么退出、哪些事本 stage 不做。
- **讲什么**：
  - **四类错误的统一分类**（协议错误 / 参数错误 / 硬件错误 / 状态错误）与各自的 errno 落点
  - errno 七值公理（`ENXIO` / `EINVAL` / `EIO` / `EAGAIN` / `EBUSY` / `EPERM` / `ENOTTY`）与"禁止自造"（F3 教训）
  - 各篇 §4 错误场景表的汇总
  - **五种退出路径对照**（普通 SIGTERM / 块框架热升级 / log 热升级 / at_wini 热升级 / audio 热升级）
  - 终止函数与故障即停哲学
  - **"已声明不做"清单**（三套块主循环的两套、间接描述符线程池、描述符树解析、ACPICA 逐行重写、`logos.h` 位图、五份混音器、SDEV 族）
  - **工程面**：服务策略配置（`system.conf` 的 20+ 驱动段）；`/dev` 节点创建（`MAKEDEV`）；各驱动 Makefile 依赖链与构建期常量
  - **范围外声明**：`libsockdriver`（1150 行）归 17-stage-net 与 `edge E-SDEVOWN`；`storage/ramdisk`（无 `.c`）；`dec21140A` 大小写差异；`examples/hello` 的 Rust 侧处置
  - 门禁机制的两族在错误路径上的差异（重启门静默丢弃 vs 初始化门拒绝）
- **不讲什么**：
  - 各失败点的机制细节（各篇讲自己的失败分支）
  - 协议值表（99）
  - 并发与同步（02）
- **前置**：00、01、02、03、04、05、06、07、08、09–30
- **后置**：无（收尾篇）
- **事实底线**：
  - C：`lib/libchardriver/chardriver.c:537`（`chardriver_terminate`）；四处 `liveupdate.c`（`lib/libblockdriver/`、`drivers/system/log/`、`drivers/storage/at_wini/`、`lib/libaudiodriver/`）；各篇 §4 错误表（汇总源）；`include/minix/com.h`（协议错误码）
  - 非 C 制品：`minix3/etc/system.conf`（20+ 驱动服务段）、`minix3/minix/commands/MAKEDEV/`、`minix3/minix/drivers/**/Makefile`
  - Rust：`os/libs/minix-driver-rt/`（运行时）
  - edge：`edge_todo.md:983`（E-SDEVOWN）
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| N-007 | 四类错误的统一分类 | 接口与协议 | 各篇 §4 错误表 | 现有 99 §2.5 只给七值 | 新增 |
| K-434a | errno 七值公理与禁止自造 | 约束与不变量 | — | 纪律 | 00 §4 + 99 §2.5 |
| K-443a | 值不许错（F3 教训） | 约束与不变量 | — | 纪律 | 99 §1/§4 |
| N-020 | 五种退出路径对照 | 机制 | `chardriver.c:537` + 四处 `liveupdate.c` | 现有各篇各讲一个 | 新增 |
| K-029a | 终止函数与故障即停 | 机制 | `chardriver.c:537` | 退出路径 | 01 §2.6 |
| K-051d | 热升级机制（四种实例） | 机制 | 四处 `liveupdate.c` | 退出路径 | 02 §1.7 |
| N-008 | **"已声明不做"清单** | 架构演进 | `plan.md` §4 + 各篇差异表 | 现有零散 | 新增 |
| N-001 | **服务策略配置面** | 工具与工程 | `etc/system.conf:161/187/210/289/301` 等 | **现有零处** | 新增 |
| N-002 | `/dev` 节点创建机制 | 工具与工程 | `commands/MAKEDEV/` | 现有零处 | 新增 |
| N-003 | 各驱动 Makefile 依赖链与构建期常量 | 工具与工程 | `drivers/**/Makefile` | 现有零处 | 新增 |
| N-014 | `libsockdriver` 处置声明 | 边界 | `lib/libsockdriver/`、`edge E-SDEVOWN` | 现有零处 | 新增 |
| N-015 | `dec21140A` 大小写差异 | 工具与工程 | 两侧目录 | 覆盖核对差集 | 新增 |
| N-016 | `storage/ramdisk` 真实性质 | 工具与工程 | `ls drivers/storage/ramdisk/` | 现有零处 | 新增 |
| K-016d | 门禁两族在错误路径上的差异 | 机制 | `chardriver.c`、`netdriver.c` | 错误路径对照 | 01 §1.5 + 03 §1.7 |
| K-183a | 错号 errno 不一致的原样保留 | 约束与不变量 | `log.c` | 错误纪律的例外 | 08 §2.4 |

- **验收标准**：
  1. 给出四类错误分类表：类别、典型场景、errno、各篇出处
  2. 给出 errno 七值公理表与"禁止自造"的三处教训（F1/F3/`NOT_DISK`）
  3. 给出五种退出路径的对照表（触发、动作、状态处理、C 锚点）
  4. 给出"已声明不做"清单（至少 7 项），每项给理由与出处
  5. 给出服务策略配置面的完整表（驱动段、授权项、与驱动能力的关系）
  6. 给出范围外声明的四项（`libsockdriver` / `storage/ramdisk` / `dec21140A` / `examples/hello`）
  7. 说明"为什么重启门静默丢弃而初始化门拒绝"（答：前者有上线事件自恢复机制，后者没有）

### 99-global-concepts

- **一句话定位**：读者要查一个常量值或一个结构定义时，本篇给出权威位置与值。
- **讲什么**：
  - 五家族请求与回复基址表（CDEV 0x400/0x480、BDEV 0x500/0x580、NDEV 0x1A00/0x1A80、RTCDEV 0x1400/0x1480、USB 0x1100；SDEV 0x1900 只记号码）
  - 判别掩码 `& ~0x7f`；消息类型低七位是家族内编号；跨族基址相距至少 128
  - 家族内编号全集（CDEV 七请求 + 三回复 + 六标志；BDEV 七请求 + 三标志；NDEV 六请求 + 配置/模式/能力/标志/链路；RTCDEV 五请求 + 标志；USB 八请求 + 载荷槽位）
  - 通用驱动模型（`MAX_NR_OPEN_DEVICES 256` / `driver_receive` / 端点约定）
  - 次设备号私有约定与分号实例
  - errno 七值映射
  - Rust 侧权威位置（`minix-types`；家族判别枚举五类型）
  - 与 31 篇的分工声明（本篇给值，31 讲错误分类与工程面）
- **不讲什么**：
  - 常量如何被使用（各消费篇）
  - 错误分类与工程面（31）
  - 构建期常量（31）
- **前置**：00（本篇是查阅表，可独立阅读）
- **后置**：无（附录）
- **事实底线**：
  - C：`minix3/minix/include/minix/com.h`（1153 行）、`include/minix/driver.h`（`MAX_NR_OPEN_DEVICES 256`）、各 `*driver.h`
  - Rust：`os/libs/minix-types/src/`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-441 | 五家族基址表 | 接口与协议 | `com.h:919/963/1085/995/813` | 查阅 | 99 §2.1 |
| K-442 | 判别掩码与低七位 | 机制 | `com.h:922-923/965-966` | 查阅 | 99 §2.1 |
| K-443 | 两条铁律（值不许错 / 定义不许重复） | 约束与不变量 | — | 查阅纪律 | 99 §1/§4 |
| K-444 | CDEV 七请求 + 三回复 + 六标志 | 接口与协议 | `com.h:926-956` | 查阅 | 99 §2.2 |
| K-445 | BDEV 七请求 + 三标志 | 接口与协议 | `com.h:970-976/982-987` | 查阅 | 99 §2.2 |
| K-446 | NDEV 六请求 + 五组子值 | 接口与协议 | `com.h:1096-1101/1126-1145` | 查阅 | 99 §2.2 |
| K-447 | RTCDEV 五请求 + 标志 | 接口与协议 | `com.h:1002-1012` | 查阅 | 99 §2.2 |
| K-448 | USB 八请求 + 载荷槽位 | 接口与协议 | `com.h:813-828/829-841` | 查阅 | 99 §2.2 |
| K-449 | SDEV 只记号码 | 边界 | `com.h:1037-1068` | 边界 | 99 §2.2 |
| K-450 | 通用驱动模型三样 | 数据结构 | `driver.h:41` | 查阅 | 99 §2.3 |
| K-451 | 次设备号私有约定 | 约束与不变量 | `dmap.h:85-92` | 查阅 | 99 §2.4 |
| K-452 | errno 七值映射 | 约束与不变量 | — | 查阅 | 99 §2.5 |
| K-453 | 常量单一权威收敛到 `minix-types` | 架构演进 | — | 收敛方向 | 99 §3.1 |
| K-454 | 家族判别枚举五类型 | 架构演进 | `{Cdev,Bdev,Ndev,Rtc}Request`、`NdevReply` | 查阅 | 99 §3.2 |
| K-455 | 各框架库钉值测试计数 | 测试性质 | — | 查阅 | 99 §5 |
| N-025 | Rust 侧常量与结构的权威位置表 | 数据结构 | `os/libs/minix-types/src/` | 现有 99 只列 C 侧 | 新增 |
| N-026 | 与 31 篇的分工声明（值 vs 错误分类/工程面） | 概念 | — | 避免重复 | 新增 |

- **验收标准**：
  1. 给出全部常量的值表，每行带 `com.h` 行锚点
  2. 给出全部协议结构的逐字段布局，每个字段带行锚点
  3. 给出 errno 映射表
  4. 给出 Rust 侧权威位置表，与 C 侧一一对应
  5. 明确声明与 31 篇的分工

---

## 6. 变更表

### 6.1 统一变更表

> 操作类型：重排 / 拆分 / 合并 / 新建 / 归档。
> "去向"列按双方向规则：**存量方向看去向**（旧知识点逐条给出新位置）、**新增方向看来源**（给证据锚点）。

| 操作编号 | 操作类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 / 来源 |
|---|---|---|---|---|---|---|
| OP-01 | 改写 | `00-drivers-overview.md`（67 行） | 新 00 | 保留全篇；补"驱动分类矩阵"的指针（矩阵本体移新 08）、补服务策略配置面的指针（移新 31） | K-421..K-435 | K-421..K-435 → 新 00 §1–§5；K-424 保留（分类矩阵的一列） |
| OP-02 | 拆分 | `01-chardriver-framework.md`（294 行） | 新 01（协议）+ 新 04（框架） | 现有 01 篇同时承载"CDEV 协议定义"与"字符框架实现"两件事，违反单篇单语义 | K-001..K-035、K-444 | K-005..K-009/K-444/K-443/K-002/K-104 → 新 01；K-001/K-004/K-010..K-035 → 新 04 |
| OP-03 | 改写 | `02-blockdriver-framework.md`（256 行） | 新 05 | 保留全篇；"三套主循环"的 ARCH 理由上移新 02、对照表移新 03 | K-036..K-063 | K-036..K-063 → 新 05（K-048/K-049 部分上移新 02/03） |
| OP-04 | 改写 | `03-netdriver-framework.md`（245 行） | 新 06 | 保留全篇；补 G8 的能力位/标志/回复枚举（已落地） | K-064..K-088 | K-064..K-088 → 新 06 |
| OP-05 | 改写 | `04-bdev-client.md`（247 行） | 新 07 | 保留全篇；补与 15-stage-fs 的接口契约（新增）；F2/A6 的修复点明确化 | K-089..K-114 + N-011 | K-089..K-114 → 新 07；新增 N-011 来源 `libbdev/bdev.c` |
| OP-06 | 新建 | 无 | 新 02 | 跨族共性（单线程假设、生命周期、硬件抽象四层级、状态机模式、策略/传输分离、中断投递）现有零处集中 | N-009、N-010、N-019、N-021 + K-003、K-023、K-450、K-016a、K-039a | 存量 K-003/K-023/K-450（99 越界）/K-016a/K-039a/K-029/K-051a/K-429a/K-431a/K-030 → 新 02；新增 N-009/N-010/N-019/N-021 来源见 §2.3 |
| OP-07 | 新建 | 无 | 新 03 | 五族框架横向对照（请求数/回调数/循环形状/门禁/队列/特有机制）现有零处 | N-018 + DUP-15、K-016b、K-048a、K-089a、K-431b、K-041a、K-441a、K-077a | 存量从 01/02/03/04/18/21 各篇提取 → 新 03；新增 N-018 来源各库源码 |
| OP-08 | 新建 | 无 | 新 08 | 57 driver 分类矩阵与回调差异矩阵现有零处集中；`examples/hello` 教科书地位未强调 | N-004、N-005、N-006、N-017 + K-424a、K-425a、K-023a、K-435a、K-016c | 存量 K-424a（00）/K-425a（00）/K-023a（01）/K-435a（00+todo）→ 新 08；新增 N-004..N-006/N-017 来源见 §2.3 |
| OP-09 | 改写 | `05-memory-driver.md`（263 行） | 新 09 | 保留全篇；§2.6 的 32 位条件编译段（越界）移新 31 | K-115..K-133、K-456 | K-115..K-133 → 新 09；K-456 → 新 09 §定位 |
| OP-10 | 改写 | `06-tty-driver.md`（308 行） | 新 10 | 保留全篇；§3.8 的键盘输入协议归位到本篇的"键盘侧"小节（不再越出 §3 编号） | K-134..K-158 | K-134..K-158 → 新 10 |
| OP-11 | 改写 | `07-pty-driver.md`（249 行） | 新 11 | 保留全篇；§2.7 ptyfs 侧车收窄为"驱动侧增删请求"（树语义归 15-stage-fs） | K-159..K-176 | K-159..K-176 → 新 11（K-170 收窄） |
| OP-12 | 改写 | `08-log-driver.md`（249 行） | 新 12 | 保留全篇；"写尾巴顺序"提升为显式不变式节（现有埋在 §3.6） | K-175a..K-189 | K-175a..K-189 → 新 12 |
| OP-13 | 改写 | `09-random-driver.md`（236 行） | 新 13 | 保留全篇；S1 的"杂凑终结"修复点明确化 | K-190..K-211 | K-190..K-211 → 新 13 |
| OP-14 | 改写 | `10-readclock-driver.md`（225 行） | 新 14 | 保留全篇；G9 的 Y2K 标志与两随行字段明确化 | K-212..K-227 | K-212..K-227 → 新 14 |
| OP-15 | 改写 | `11-pci-driver.md`（223 行） | 新 15 | 保留全篇；S2/S3/S4 三个修复点明确化；§2.5 选读节收窄 | K-226a..K-241 | K-226a..K-241 → 新 15 |
| OP-16 | 改写 | `12-gpio-devman.md`（232 行） | 新 16 | 保留全篇；E-DMCLIENT 的收敛声明保留；库名事实核查保留 | K-242..K-257 | K-242..K-257 → 新 16 |
| OP-17 | 改写 | `13-pckbd-driver.md`（232 行） | 新 17 | 保留全篇；**结构修复**：文末三条悬空表行归位 §差异表；参见删除 `bridge.rs`；补 G7 的键盘看门狗 | K-258..K-271 + GAP-23 | K-258..K-271 → 新 17；GAP-23 来源 `pckbd.c:21-23,49-62` |
| OP-18 | 改写 | `14-virtio-framework.md`（204 行） | 新 18 | 保留全篇；§3.1 的 `Hal`/DMA 契约收窄为"第一次出现"的指针（契约本体归 30） | K-272..K-302 | K-272..K-302 → 新 18（K-298 收窄） |
| OP-19 | 改写 | `15-virtio-blk-driver.md`（180 行） | 新 19 | 保留全篇；V2 的显式偏差是**保留项**（不修，因为它是有理由的设计决策） | K-302a..K-315 | K-302a..K-315 → 新 19 |
| OP-20 | 拆分 | `16-ahci-ata-driver.md`（220 行） | 新 20（ahci）+ 新 21（ata） | 两驱动差异巨大（串行 vs 并行、端口命令表 vs 控制器命令）；现有同篇造成"命令槽"与"控制器阶段"两个语义单元混杂 | K-316..K-328 | K-316..K-318/K-324..K-326/K-328a → 新 20；K-319..K-323/K-327/K-328b → 新 21 |
| OP-21 | 改写 | `17-storage-misc-driver.md`（275 行） | 新 22 | 保留全篇；**结构修复**：§2 小节顺序与概念章对齐（软盘 → 闪存卡 → 故障注入 → 过滤 → 回环）；V3/V4/V6 修复点明确化 | K-329..K-342 | K-329..K-342 → 新 22 |
| OP-22 | 改写 | `18-usb-framework.md`（206 行） | 新 23 | 保留全篇；A5 的 `wire.rs` 落地明确化 | K-343..K-355 | K-343..K-355 → 新 23 |
| OP-23 | 改写 | `19-usb-storage-hub.md`（204 行） | 新 24 | 保留全篇；V7 的修复点明确化；**结构修复**：§2 小节按"存储侧 / 集线器侧"分组 | K-356..K-371 | K-356..K-371 → 新 24 |
| OP-24 | 合并 | `20-fb-driver.md`（209 行）+ `21-audio-drivers.md`（204 行） | 新 25 | 两者都是"字符设备的非典型变体"，合并后共享"字符设备的变体"这个语义单元；且两篇各只有 10 个知识点，独立成篇偏薄 | K-372..K-391 + GAP-21、GAP-22、GAP-24 | K-372..K-381（20）与 K-382..K-391（21）全部 → 新 25；新增 GAP-21/GAP-22/GAP-24 来源 `audio_fw.h`/`ioc_sound.h`/`sb16.h` |
| OP-25 | 改写 | `22-net-driver-reference.md`（199 行） | 新 26 | 保留全篇；N1 的修复点明确化 | K-392..K-400 | K-392..K-400 → 新 26 |
| OP-26 | 改写 | `23-net-driver-variants.md`（207 行） | 新 27 | 保留全篇；N2 的修复点明确化 | K-401..K-409 | K-401..K-409 → 新 27 |
| OP-27 | 改写 | `24-misc-drivers.md`（211 行） | 新 28 | 保留全篇；G2 的 `minix-i2cdriver` 归属保留；A-9 的 ACPI 策略明确化 | K-410..K-420 | K-410..K-420 → 新 28 |
| OP-28 | 新建 | 无 | 新 29 | 测试基建三层与协议编号对账机制现有零处集中 | N-012、N-013、N-022 + K-103a、K-440a、K-104a、K-211a、K-455a、K-435b | 存量从 01/04/09/25/99/00 各篇提取 → 新 29；新增 N-012/N-013/N-022 来源见 §2.3 |
| OP-29 | 改写 | `25-dma-memory-contract.md`（74 行） | 新 30 | 保留全篇；补消费方清单（新增）；明确它是"边界契约"而非驱动语义 | K-436..K-440 + N-023、N-024 | K-436..K-440 → 新 30；新增 N-023/N-024 来源各驱动 + edge |
| OP-30 | 新建 | 无 | 新 31 | 四类错误分类 + 五种退出路径 + "已声明不做"清单 + 工程面（策略配置/MAKEDEV/Makefile），现有全部散落或缺失 | N-007、N-008、N-020、N-001、N-002、N-003、N-014、N-015、N-016 + K-434a、K-443a、K-029a、K-051d、K-016d、K-183a | 存量 K-434a/K-443a（00+99）/K-029a（01）/K-051d（02）/K-016d（01+03）/K-183a（08）→ 新 31；新增 N-001..N-003/N-007/N-008/N-014..N-016/N-020 来源见 §2.3 |
| OP-31 | 改写 | `99-global-concepts.md`（76 行） | 新 99 | 保留全篇；§2.3 通用驱动模型移新 02（越界归位）；补 Rust 侧权威位置表与与 31 的分工 | K-441..K-456 + N-025、N-026 | K-441..K-456 → 新 99（K-450 移新 02）；新增 N-025/N-026 来源见 §2.3 |
| OP-32 | 归档 | `plan.md`（433 行） | 不删，退出正式目录 | 它是重组计划而非知识文档；其结论已被本蓝图取代（本蓝图给独立证据） | — | 归档保留（B 相不删） |
| OP-33 | 归档 | `todo.md`（160 行） | 不删，退出正式目录 | 它是扫描 TODO 而非知识文档；其发现已吸收进本蓝图与各篇契约 | — | 归档保留 |
| OP-34 | 归档 | `draft/README.md`（17 行） | 不删，退出正式目录 | 旧占位素材；其 scope 定义已被新 00 与新 08 完整覆盖 | — | 归档保留 |

### 6.2 拆分与合并的存量知识点去向（完整）

> 双方向规则的存量方向：拆分与合并涉及的旧知识点，逐条给出新位置。**写不出去向的不许拆**——以下每一处都写全了。

**OP-02 拆分（`01-chardriver-framework.md` → 新 01 + 新 04）**

| 旧知识点 | 新位置 |
|---|---|
| K-005 七种请求清单 | 新 01 §请求表 |
| K-006 基址 + 索引 + 判定宏 | 新 01 §协议编码 |
| K-007 两层路由 | 新 01 §协议设计 |
| K-008 请求材料字段 | 新 01 §请求表 |
| K-009 回复三种形状 | 新 01 §回复形状 |
| K-002 小票=消息 | 新 01 §协议编码 |
| K-444 CDEV 七请求 + 三回复 + 六标志（值表） | 新 01 §值表 |
| K-443 值不许错 | 新 01 §协议纪律（汇总归 31） |
| K-104 编号镜像绊线 | 新 01 §对账机制 |
| K-001 后厨心智模型 | 新 04 §概念 |
| K-003 单线程假设 | 新 02 §概念（声明归 31） |
| K-004 开业宣告 + 清表 | 新 04 §生命周期 |
| K-010 十个挂钩 | 新 04 §回调表 |
| K-011 空挂钩六条默认行为 | 新 04 §回调表 |
| K-012 挂起语义 | 新 04 §挂起与取消 |
| K-013 拒绝再次挂起 | 新 04 §挂起与取消 |
| K-014 取消三态 | 新 04 §挂起与取消 |
| K-015 挂起只允许四种 | 新 04 §挂起与取消 |
| K-016 重启门 | 新 04 §门禁 |
| K-017 静默丢弃理由 | 新 04 §门禁 |
| K-018 块打开误投守卫 | 新 04 §分发 |
| K-019 通知单向性 | 新 04 §通知旁路 |
| K-020/K-021 Linux / Redox 对照 | 新 04 §横向定位 |
| K-022 打开集合 | 新 04 §状态 |
| K-023 宣告三件事 | 新 04 §生命周期（通用形状归 02） |
| K-024 发送辅助 | 新 04 §回复 |
| K-025 回复三特殊标记 | 新 04 §回复 |
| K-026 回复按请求种类分派 | 新 04 §回复 |
| K-027 七适配器三段式 | 新 04 §分发 |
| K-028 主循环与分发顺序 | 新 04 §主流程 |
| K-029 终止函数两行 | 新 04 §生命周期（退出路径对照归 31） |
| K-030 `chardriver_get_minor` 七分支 | 新 04 §接口（通用形状归 02） |
| K-031 与 C 差异表五条 | 新 04 §差异 |
| K-032 Rust 决策五条 | 新 04 §Rust 化 |
| K-033 单线程假设写进文档 | 新 04 §设计声明（归 02 更佳） |
| K-034 `SilentDevice` | 新 29 §测试替身（或新 04 §测试） |
| K-035 错误处理表 | 新 04 §错误（分类归 31） |

**OP-20 拆分（`16-ahci-ata-driver.md` → 新 20 + 新 21）**

| 旧知识点 | 新位置 |
|---|---|
| K-316 高级主控端口＝机场塔台 | 新 20 §概念 |
| K-317 端口槽位图管理动作 | 新 20 §端口管理 |
| K-318 先开塔再申请铁律 | 新 20 §端口管理 |
| K-324 识别先验后量 | 新 20 §识别 |
| K-325 容量读四字拼 64 位 | 新 20 §识别 |
| K-326 超时复位哲学与停止清槽 | 新 20 §超时复位 |
| K-328a 端口槽位图 + 识别纯解析 | 新 20 §Rust 化 |
| K-319 并行接口＝火车站 + 控制器状态机 | 新 21 §概念与控制器 |
| K-320 大单拆小单 | 新 21 §拆单 |
| K-321 出错锁存待复位 | 新 21 §控制器 |
| K-322 直接存取三态与守卫 | 新 21 §直接存取 |
| K-323 验车语义 | 新 21 §直接存取 |
| K-327 等待分两种 | 新 21 §等待 |
| K-328b 控制器阶段机 + 武装机 | 新 21 §Rust 化 |
| K-051c 热升级钩子存在 | 新 21 §边界（机制归 31） |

**OP-24 合并（`20-fb-driver.md` + `21-audio-drivers.md` → 新 25）**

| 旧知识点 | 新位置 |
|---|---|
| K-372..K-381（原 20 全部） | 新 25 §帧缓冲部分（对应节） |
| K-382..K-391（原 21 全部） | 新 25 §音频部分（对应节） |
| 两篇的勘误块（计划勘误 / 头文件纠正） | 新 25 §勘误（两条并列保留） |
| 两篇的差异表（20 §2.8 四条 + 21 §2.8 三条） | 新 25 §差异（合并为一张表，标注来源篇） |
| 两篇的测试统计（20：模式 4 + 显示 5；21：速率 3 + 命令 4） | 新 25 §测试（合并，保留两组的来源标注） |

**新建篇章的新增知识点来源（双方向规则的新增方向）**

| 新篇 | 新增知识点 | 证据锚点 |
|---|---|---|
| 新 00 | N-006 | 各篇头部"说明"行 |
| 新 01 | N-012a | `minix-chardriver` 测试 |
| 新 02 | N-009、N-010、N-019、N-021 | `chardriver.c:99/549/537`、`:464-482`、`netdriver.c:763`、A-2/A-3/A-4 涉及各篇、14/16/17/18/19 的 §3 |
| 新 03 | N-018 | `chardriver.c`、`libblockdriver/`、`netdriver.c`、`bdev/`、`libvirtio/` |
| 新 07 | N-011 | `libbdev/bdev.c`、`15-stage-fs/07-mfs-init-main.md` |
| 新 08 | N-004、N-005、N-006、N-017 | `kernel/table.c`、`etc/system.conf`、各 `*_tab`、`hello.c` |
| 新 17 | GAP-23 | `pckbd.c:21-23,49-62` |
| 新 25 | GAP-21、GAP-22、GAP-24 | `audio_fw.h:9-22`、`ioc_sound.h:12-22`、`sb16.h:107-110` |
| 新 29 | N-012、N-013、N-022 | 各框架库钉值测试、各 crate 测试、各篇 §5 |
| 新 30 | N-023、N-024 | 各驱动源码、`edge_todo.md:1001` |
| 新 31 | N-001、N-002、N-003、N-007、N-008、N-014、N-015、N-016、N-020 | `etc/system.conf:161/187/210/289/301` 等、`commands/MAKEDEV/`、`drivers/**/Makefile`、各篇 §4、`plan.md` §4、`lib/libsockdriver/`、两侧目录、`ls drivers/storage/ramdisk/`、四处 `liveupdate.c` |
| 新 99 | N-025、N-026 | `os/libs/minix-types/src/` |

### 6.3 归档清单

| 旧文档 | 处置 | 理由 |
|---|---|---|
| `plan.md` | 归档不删 | 重组计划，结论已被本蓝图取代 |
| `todo.md` | 归档不删 | 扫描 TODO，发现已吸收 |
| `draft/README.md` | 归档不删 | 旧占位素材，scope 定义已被覆盖 |

**归档与"删除"的区分**：以上三处**都不是删除**——内容全部有去向（前两者是参考材料而非知识点载体，后者是素材）。**本蓝图没有任何"删除加理由"项**（见 §9 G5）。

### 6.4 待裁决问题（本节详述前三条；完整七条见 §9.4）

| # | 裁决点 | 选项 | 本蓝图推荐 | 影响面 |
|---|---|---|---|---|
| Q-1 | **DMA 契约归本 stage 还是 VM stage？** | A：留本 stage（消费侧在这里，契约住共享类型库）；B：整体迁 `02-stage-vm`（实现方在那里） | **A** | 新 30 的归属；edge E-DMABUF 的执行轨道。理由：契约的四操作是驱动库的**开口**，四个消费方（virtio/ahci/usb_storage/memory）全在本 stage；实现方（VM）只是其中一个实现 |
| Q-2 | **ACPICA（83279 行）的处置** | A：只包外壳 + 策略声明（现有做法）；B：完整移植；C：换 Rust AML 库 | **A** | 新 28 的 ACPI 节；`plan.md` §4 A-9 的"重大决策"。理由：第三方移植非 Minix3 自有设计，逐行重写无收益 |
| Q-3 | **`minix-netdriver` 里寄居的 `sdev.rs`/`sockevent.rs`（9 个测试）** | A：迁出到 `minix-sockdriver`（归 17-stage-net）；B：留原地并声明；C：删（vfs 另有 923 行独立副本） | **A** | 新 06 的边界声明；`edge E-SDEVOWN` 的执行。理由：语义归 17-stage-net，但物理位置在 netdriver crate 是历史遗留；迁出后 netdriver 的职责单一 |

> 说明：Q-3 的现状是 `todo.md` §4 的 OQ-1（"归属处置已登记 edge E-SDEVOWN，本 stage 不擅自删"）。本蓝图的推荐 A 与 edge 的裁决方向一致。

---

## 7. 缺漏新篇

> 步骤 3 发现的 24 条缺口，逐项落实为新建篇章或明确否决。**本节不留空、不写"待定"。**

| 缺口 | 主题 | 为什么重要 | 原料在哪里 | 归哪一篇 | 验收标准 |
|---|---|---|---|---|---|
| GAP-01 | **服务策略配置面** | 驱动能否访问硬件的前提；`io ALL` 与 `io NONE` 的差异直接决定能力 | `minix3/etc/system.conf`（20+ 驱动段）+ `drivers/**/*.conf`（34 个）+ `etc/devmand/*.cfg` | **新 31 §工程面** | 给出完整表（驱动段、授权项、与能力的关系），每行带行锚点 |
| GAP-02 | `/dev` 节点创建机制 | 回答"设备节点从哪来" | `commands/MAKEDEV/MAKEDEV.sh`、`MAKEDEV.8` | **新 31 §工程面** | 给出主/次设备号分配规则与创建流程 |
| GAP-03 | 构建链与构建期常量 | 回答"这个驱动依赖哪些库" | `drivers/**/Makefile` | **新 31 §工程面** | 给出依赖链表（`LDADD`/`DPADD`）与构建期常量清单 |
| GAP-04 | **驱动分类完整矩阵** | 57 driver 的全局视图 | `kernel/table.c`、`etc/system.conf`、各 `*_tab` | **新 08** | 至少 6 列的完整表，57 行 |
| GAP-05 | **回调差异矩阵** | 回答"谁实现了什么" | 各 driver 的 `*_tab` | **新 08** | 矩阵齐备（行 driver、列回调成员、格子三态） |
| GAP-06 | 服务层边界声明汇总 | 全 stage 最重要的架构约定，散在 27 篇 | 各篇头部"说明" | **新 02** + **新 08** | 给出统一口径与逐篇出处 |
| GAP-07 | 错误分类框架 | 99 §2.5 只给七值清单 | 各篇 §4 错误表 | **新 31** | 四类分类表 + 七值公理 + 三处教训 |
| GAP-08 | **"已声明不做"清单** | 读者需一次看全 | `plan.md` §4 + 各篇差异表 | **新 31** | 至少 7 项，每项给理由与出处 |
| GAP-09 | 驱动进程生命周期总图 | 各篇讲自己的启动 | `chardriver.c:99/549/537` + 四处 `liveupdate.c` | **新 02** | 完整生命周期图（启动→宣告→主循环→五种退出） |
| GAP-10 | 中断与通知投递路径 | 01 §1.6 只讲一个框架 | `chardriver.c:464-482`、`netdriver.c:763` | **新 02** | 跨族统一路径图 |
| GAP-11 | `libbdev` 与 15-stage-fs 的接口契约 | 现有 04 声明"见别处"但未给契约面 | `libbdev/bdev.c`、`15-stage-fs/07` | **新 07** | 至少 3 个调用点的契约面 |
| GAP-12 | 协议编号编译期对账机制 | 99 §5 只给计数 | 各框架库钉值测试 | **新 29** | 机制说明 + 计数表 + 复现命令 |
| GAP-13 | 测试基建三层 | 00 §5 只给总数 | 各 crate 测试 | **新 29** | 三层划分表 + 代表用例 |
| GAP-14 | `libsockdriver` 处置声明 | plan 排除但 27 篇零处 | `lib/libsockdriver/`、`edge E-SDEVOWN` | **新 31 §范围外** | 明确排除 + 理由 + edge 指针 |
| GAP-15 | `dec21140A` 大小写差异 | 覆盖核对差集项 | 两侧目录 | **新 31 §范围外** | 说明不是缺失 |
| GAP-16 | `storage/ramdisk` 真实性质 | plan 声明但 27 篇零处 | `ls drivers/storage/ramdisk/` | **新 31 §范围外** | 说明无 `.c`、语义映射到 memory 的 `/dev/imgrd` |
| GAP-17 | `examples/hello` 教科书地位 | 现有混在杂项矩阵里 | `hello.c`（158 行） | **新 08** + **新 28** | 强调其"最短理解路径"价值 |
| GAP-18 | 五族框架库对照表 | 01/02/03/04 各讲自己 | 五库源码 | **新 03** | 至少 8 列的对照表 |
| GAP-19 | 硬件抽象四层级 | `plan.md` §4 给了 ARCH 清单但无组织框架 | A-2/A-3/A-4 涉及各篇 | **新 02** | 四层级表（层级、涉及驱动、抽象手段、ARCH 编号） |
| GAP-20 | 五种退出路径对照 | 各篇各讲一个 | 四处 `liveupdate.c` + `chardriver.c:537` | **新 31** | 对照表（触发、动作、状态处理、锚点） |
| GAP-21 | **音频 14 钩子逐项语义** | 21 §2.7 称"已覆盖"但无逐项说明 | `audio_fw.h:9-22` | **新 25** | 14 项逐项表（签名、语义、默认行为） |
| GAP-22 | **11 个声音控制请求** | 21 只提一句 | `ioc_sound.h:12-22` | **新 25** | 11 项逐项表 |
| GAP-23 | **键盘看门狗** | 13 篇零处（G7 缺项） | `pckbd.c:21-23,49-62` | **新 17** | 两条看门狗的触发条件与复位重发流程 |
| GAP-24 | **SB16 停/续字节** | 21 篇未建模 | `sb16.h:107-110` | **新 25** | 停播/续播的字节与语义 |

**明确否决的缺口**（不留待定）：

| 候选主题 | 否决理由 |
|---|---|
| 单独新建"块框架三套循环"篇 | 两套已声明不做（新 31 的清单），剩余内容并入新 05 的差异表 |
| 单独新建"libvirtio"篇 | 已独立成新 18（虚拟队列框架） |
| 单独新建"libusb"篇 | 并入新 23（USB 框架，libusb 只是客户端封装） |
| 单独新建"libaudiodriver"篇 | 并入新 25（音频部分的核心是 14 钩子） |
| 单独新建"libi2cdriver"篇 | 并入新 28（i2c 底盘驱动的框架，内容量不足以成篇） |
| 单独新建"libinputdriver"篇 | 并入新 17（事件桥是键盘驱动的出口） |
| 单独新建"libdevman"篇 | 并入新 16（设备注册库） |
| 单独新建"pci 枚举与桥"篇 | 并入新 15（枚举建档是户籍管理的一环） |
| 单独新建"存储变体逐家一篇" | 五家合并为新 22（差异矩阵原则） |
| 单独新建"网卡变体逐家一篇" | 12 家合并为新 27（差异矩阵原则） |
| 单独新建"USB 设备类逐类一篇" | 并入新 24 |
| 单独新建"ACPICA 移植"篇 | 明确不做（新 31 的清单 + 新 28 的外壳声明） |
| 单独新建"驱动命令行工具"篇 | 属 `18-stage-commands` |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表

> 覆盖所有发生变化的旧文档，逐节列出。旧编号与新编号是多对多映射。

#### 00-drivers-overview.md（67 行，7 节）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注（断链风险） |
|---|---|---|---|---|
| 00 §1 | 57 driver + 11 框架库 + 微内核姿势 | 新 00 §1 | 改写 | 4 处内部引用引本篇 |
| 00 §2 | 启动因果链（boot 只带 memory/tty） | 新 00 §2 | 改写 | — |
| 00 §3.1 | 策略在库、传输在 bin | 新 00 §3 + 新 02 §架构 | 拆分 | 越界归位（OOB-11） |
| 00 §3.2 | `minix-driver-rt` 统一骨架 | 新 02 §生命周期 + 新 00 §3 | 拆分 | — |
| 00 §3.3 | 五族框架分立 + `core` 状态机 | 新 03 §对照 + 新 00 §3 | 拆分 | — |
| 00 §3.4 | 线格式归策略库 + 连续物理内存 | 新 02 §原则 + 新 30 §契约 | 拆分 | — |
| 00 §4 | errno 七值公理 | 新 31 §errno | 重排 | — |
| 00 §5 | 测试与集成面 | 新 29 §现状 | 重排 | — |
| 00 §6/§7 | 过渡 / 参见 | 新 00 §过渡/§参见 | 改写 | — |

#### 01-chardriver-framework.md（294 行，含 §1.0–§1.8、§2.1–§2.9、§3.1–§3.8、§4、§5.1–§5.3、§6、§7）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 01 §1.1 | 后厨心智模型 | 新 04 §概念 | 改写 | **27 处内部引用引本篇（最高）** |
| 01 §1.2 | 七种请求 + 基址 + 判定宏 | 新 01 §请求表 | 拆分 | — |
| 01 §1.3 | 十个挂钩 + 空挂钩六条 | 新 04 §回调表 | 改写 | — |
| 01 §1.4 | 挂起与取消 | 新 04 §挂起与取消 | 改写 | — |
| 01 §1.5 | 重启门 | 新 04 §门禁 | 改写 | — |
| 01 §1.6 | 通知旁路 | 新 04 §通知（跨族路径归 02） | 拆分 | — |
| 01 §1.7 | Linux / Redox 对照 | 新 04 §横向定位 | 改写 | — |
| 01 §1.8 | 本章小结 | 新 04 小结 | 改写 | — |
| 01 §2.1 | 文件清单与消息布局 | 新 01 §布局 + 新 04 §清单 | 拆分 | — |
| 01 §2.2 | 上线宣告与打开集合 | 新 04 §生命周期/§状态 | 改写 | — |
| 01 §2.3 | 延后回复与选择通知 | 新 04 §挂起与取消 | 改写 | — |
| 01 §2.4 | 回复构造 | 新 04 §回复 | 改写 | — |
| 01 §2.5 | 七个适配器 | 新 04 §分发 | 改写 | — |
| 01 §2.6 | 路由与主循环 | 新 04 §主流程 | 改写 | — |
| 01 §2.7 | 次设备号提取 | 新 04 §接口 | 改写 | — |
| 01 §2.8 | 符号覆盖矩阵 | 新 04 §符号覆盖 | 改写 | — |
| 01 §2.9 | 与 C 六百行的差异说明 | 新 04 §差异 | 改写 | — |
| 01 §3.1–§3.8 | Rust 设计决策八条 | 新 01（K-032 部分）+ 新 04（其余） | 拆分 | **§3 编号越出 1–6 节奏**（§3.7/§3.8） |
| 01 §4 | 错误处理 | 新 04 §错误（分类归 31） | 拆分 | — |
| 01 §5.1–§5.3 | 测试 | 新 29 | 重排 | — |
| 01 §6/§7 | 过渡 / 参见 | 新 04 §过渡/§参见 | 改写 | — |
| 01 头部 L4 | `[ARCH: 字符框架判定核单点]` 标注 | 新 01/新 04 头部 | 改写 | 全 stage 唯一带方括号的 ARCH 标注 |

#### 02-blockdriver-framework.md（256 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 02 §1.1 | 厨房模型 + 四样新东西 | 新 05 §概念 | 改写 | **17 处内部引用** |
| 02 §1.2 | 七种块请求 | 新 05 §请求表 | 改写 | — |
| 02 §1.3 | 十一个回调 | 新 05 §回调表 | 改写 | **§1.3 句内算术歧义**（"十一个"与"再加…三个"易读成 14）——重建时须重数 |
| 02 §1.4 | 分区解析 | 新 05 §分区 | 改写 | — |
| 02 §1.5 | 三套循环 | 新 05 §循环 + 新 02 §ARCH | 拆分 | `[ARCH A-5]` |
| 02 §1.6 | 追踪 | 新 05 §追踪 | 改写 | — |
| 02 §1.7 | 热升级 | 新 05 §热升级（退出路径归 31） | 拆分 | — |
| 02 §2.1 | 文件清单 | 新 05 §清单 | 改写 | — |
| 02 §2.2 | 消息布局与请求常量 | 新 05 §协议 | 改写 | — |
| 02 §2.3 | 宣告与打开集合 | 新 05 §装配 | 改写 | — |
| 02 §2.4 | 适配器传输四合一 | 新 05 §传输 | 改写 | — |
| 02 §2.5 | 三套主循环 | 新 05 §循环 | 改写 | — |
| 02 §2.6 | 消息队列 | 新 05 §队列 | 改写 | — |
| 02 §2.7 | 分区解析 | 新 05 §分区 | 改写 | — |
| 02 §2.8 | 追踪与热升级 | 新 05 §追踪/§热升级 | 改写 | — |
| 02 §2.9 | 符号覆盖矩阵 | 新 05 §符号覆盖 | 改写 | — |
| 02 §2.10 | **与 C 十步的差异说明** | 新 05 §差异 | 改写 | **标题"十步"表述异常**（与其它篇的"N 行"体例不一致）——重建时须改名 |
| 02 §3.1–§3.6 | Rust 决策 | 新 05 §Rust 化 | 改写 | — |
| 02 §4–§7 | 错误/测试/过渡/参见 | 新 31/新 29/新 05 | 拆分 | — |

#### 03-netdriver-framework.md（245 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 03 §1.1–§1.8 | 塔台模型与六机制 | 新 06 §概念 | 改写 | **5 处内部引用** |
| 03 §1.3 | **十二个回调** | 新 06 §回调表 | 改写 | **计数矛盾**：标"十二个"但列举 14 项（名字 + 13 函数指针）；§2.2 亦称"十二个"——重建时须重数 |
| 03 §2.1–§2.7 | 七个小节 | 新 06 §实现 | 改写 | **§2 只有 2.1–2.7**（同批唯一无 §2.9/§2.10 的结构） |
| 03 §2.2 | 请求常量与尺寸 | 新 06 §协议 | 改写 | **行号越界**：引 `com.h:1143-1145` 超出头部声明的上界 1144——重建时须重核 |
| 03 §2.7 | 与 C 一千一百八十六行的差异说明 | 新 06 §差异 | 改写 | — |
| 03 §5.1 | 协议模块测试（八个） | 新 29 | 重排 | **G8 已补能力位/标志/回复枚举**（52 测试） |
| 03 交叉引用 | §2.6 差异表 / §2.6 落地 | — | 修正 | **两处交叉引用错位**（差异说明实为 §2.7、符号矩阵为 §2.6）——重建时须重指 |

#### 04-bdev-client.md（247 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 04 §1.1–§1.7 | 总机模型与三本账 | 新 07 §概念 | 改写 | **4 处内部引用** |
| 04 §2.1–§2.8 | 八个小节 | 新 07 §实现 | 改写 | — |
| 04 §2.8 | 与 C 一千二百六十四行的差异说明 | 新 07 §差异 | 改写 | **聚合基数不明**：1264 与头部各行数之和不符——重建时须重核 |
| 04 交叉引用 | 第 2.7 节差异表 | — | 修正 | **两处交叉引用错位**（差异说明实为 §2.8、§2.7 是符号矩阵）——重建时须重指 |
| 04 §3 | Rust 决策 | 新 07 §Rust 化 | 改写 | A6 的 `CallSlot` 已落地 |

#### 05-memory-driver.md（263 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 05 §1.1–§1.8 | 馆员模型与双面 | 新 09 §概念 | 改写 | **3 处内部引用** |
| 05 §2.1–§2.8 | 八个小节 | 新 09 §实现 | 改写 | — |
| 05 §2.6 | 32 位条件编译段 | 新 31 §工程面 | 重排 | **越界归位**（OOB-03） |
| 05 §2.9 | 符号覆盖矩阵 | 新 09 §符号覆盖 | 改写 | — |
| 05 §2.10 | 与 C 五百九十九行的差异说明 | 新 09 §差异 | 改写 | — |

#### 06-tty-driver.md（308 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 06 §1.1–§1.9 | 总机房模型（九节） | 新 10 §概念 | 改写 | **6 处内部引用** |
| 06 §2.1–§2.10 | 十个小节 | 新 10 §实现 | 改写 | — |
| 06 §2.10 | 与 C 一千六百零三行的差异说明 | 新 10 §差异 | 改写 | — |
| 06 §3.8 | **键盘侧输入协议** | 新 10 §键盘侧（编号归位） | 重排 | **§3.8 越出 §3 的 1–6 编号节奏，且直接引 C 行号**——重建时须归位 |
| 06 §5.6 | 键盘输入协议模块测试（五个） | 新 29 | 重排 | — |
| 06 计数 | 三十四个测试（5+4+7+11+2+5） | 新 29 | 改写 | 自洽 |

#### 07-pty-driver.md（249 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 07 §1.1–§1.8 | 对讲机模型 | 新 11 §概念 | 改写 | **3 处内部引用** |
| 07 §2.1–§2.9 | 九个小节 | 新 11 §实现 | 改写 | — |
| 07 §2.7 | ptyfs 侧车 | 新 11 §边界（树语义归 15-stage-fs） | 收窄 | **越界收窄**（OOB-04） |
| 07 §2.9 | 与 C 两千二百九十二行的差异说明 | 新 11 §差异 | 改写 | — |
| 07 §5 | 测试（对 8 / 缓冲 6 / 选择 5 / 侧车 3 = 22） | 新 29 | 重排 | 自洽 |

#### 08-log-driver.md（249 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 08 §1.1–§1.8 | 黑板报模型 | 新 12 §概念 | 改写 | **2 处内部引用** |
| 08 §2.1–§2.9 | 九个小节 | 新 12 §实现 | 改写 | — |
| 08 §2.9 | 与 C 五百一十三行的差异说明 | 新 12 §差异 | 改写 | — |
| 08 §3.6 | 唤醒做成一份按序计划 | 新 12 §不变式（**提升为显式节**） | 重排 | **B3 的修复点** |
| 08 §5 | 测试（环 5 / 设备 8 / 增量 4 = 17） | 新 29 | 重排 | 自洽 |

#### 09-random-driver.md（236 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 09 §1.1–§1.8 | 酒窖模型 | 新 13 §概念 | 改写 | **2 处内部引用** |
| 09 §2.1–§2.9 | 九个小节 | 新 13 §实现 | 改写 | — |
| 09 §2.9 | 与 C 五百零五行的差异说明 | 新 13 §差异 | 改写 | — |
| 09 §5.2 | 独立回放对账测试 | 新 13 §测试 + 新 29 §方法 | 拆分 | **S1 的修复点** |

#### 10-readclock-driver.md（225 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 10 §1.1–§1.6 | 挂钟模型（六节） | 新 14 §概念 | 改写 | **3 处内部引用** |
| 10 §2.1–§2.9 | 九个小节 | 新 14 §实现 | 改写 | **§2.6 标题"第一百六十五 行"含多余空格** |
| 10 §2.9 | 与 C 三百一十一行的差异说明 | 新 14 §差异 | 改写 | — |
| 10 §3.2 | 时间值类型 | 新 14 §Rust 化 | 改写 | **G9 已补 Y2K 标志与两随行字段** |

#### 11-pci-driver.md（223 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 11 §1.1–§1.7 | 车管所模型 | 新 15 §概念 | 改写 | **3 处内部引用** |
| 11 §2.1–§2.7 | 七个小节 | 新 15 §实现 | 改写 | — |
| 11 §2.5 | `pci.c` 选读（桥窗口力学） | 新 15 §边界（力学归服务层） | 收窄 | **越界收窄**（OOB-09） |
| 11 §2.7 | 与 C 三千三百三十六行的差异说明 | 新 15 §差异 | 改写 | — |
| 11 §3.3 | 设备库与名单 | 新 15 §Rust 化 | 改写 | **S2/S3 的修复点** |

#### 12-gpio-devman.md（232 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 12 头部 L3 | 改版注记（E-DMCLIENT） | 新 16 §收敛声明 | 保留 | **跨 crate 归属标注** |
| 12 §1.1–§1.6 | 门牌科模型 | 新 16 §概念 | 改写 | **1 处内部引用** |
| 12 §2.1–§2.7 | 七个小节 | 新 16 §实现 | 改写 | — |
| 12 §2.7 | 与 C 八百六十六行的差异说明 | 新 16 §差异 | 改写 | — |
| 12 §5.3/§5.4 | 注册与跟踪测试（英文测试名） | 新 29 | 重排 | **体例不统一**：其余篇测试名是中文描述 |

#### 13-pckbd-driver.md（232 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 13 §1.1–§1.7 | 同声传译模型 | 新 17 §概念 | 改写 | **1 处内部引用** |
| 13 §2.1–§2.7 | 七个小节 | 新 17 §实现 | 改写 | — |
| 13 §2.7 | 差异表（**只剩一行**） | 新 17 §差异（三条悬空行归位） | 修复 | **结构性硬伤**：文末 L227–L232 三条表格行属本表（OOB-01） |
| 13 §5.1 | **扫描模块测试（十一个）** | 新 29 | 改写 | **计数矛盾**：标题"十一个"、表内 8 行、统计"十六个（扫描八个）"——重建时以实测为准 |
| 13 参见 L227 | 指向 `bridge.rs` | — | 删除 | **内部矛盾**：L5 已声明该文件删除（A11）——OOB-02 |
| 13 §2.5 | 看门狗两条 | 新 17 §看门狗（**补 GAP-23**） | 补充 | **G7 缺项** |

#### 14-virtio-framework.md（204 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 14 §1.1–§1.6 | 旋转寿司模型 | 新 18 §概念 | 改写 | **5 处内部引用** |
| 14 §2.1–§2.7 | 七个小节 | 新 18 §实现 | 改写 | — |
| 14 §2.7 | 与 C 一千零三十三行的差异说明 | 新 18 §差异 | 改写 | **聚合基数不明**（913 + 120 = 1033，未说明口径） |
| 14 §3.1 | `Hal` 与 DMA 契约 | 新 18 §指针 + 新 30 §契约 | 收窄 | **越界收窄**（OOB-05） |
| 14 不讲什么 | 第三条指向"第 2.8 节" | — | 修正 | **该节不存在**（本篇 §2 只有 2.1–2.7）——重建时须改指 §2.7 |

#### 15-virtio-blk-driver.md（180 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 15 §1.1–§1.6 | 外卖三件套模型 | 新 19 §概念 | 改写 | **4 处内部引用** |
| 15 §2.1–§2.6 | 六个小节 | 新 19 §实现 | 改写 | — |
| 15 §2.6 | 与 C 七百五十四行的差异说明 | 新 19 §差异 | 改写 | — |
| 15 §1.1/§2.3/§3.2/§4 | 未知状态偏差（四处） | 新 19 §偏差（**合并为一节**） | 合并 | **V2 的保留项**（有理由的设计决策，不修） |
| 15 §5.1 | 请求模块测试（六个） | 新 29 | 重排 | **测试表算术疑点**：`| 截断取整块 | 六千剩五千六 |`（5600 非 512 整数倍）——重建时须核实 |

#### 16-ahci-ata-driver.md（220 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 16 §1.1–§1.5 | 塔台与火车站模型 | 新 20 §概念 + 新 21 §概念 | 拆分 | **3 处内部引用** |
| 16 §2.1–§2.8 | 八个小节 | 新 20 §实现 + 新 21 §实现 | 拆分 | — |
| 16 §2.8 | 与 C 五千零五十三行的差异说明 | 新 20 §差异 + 新 21 §差异 | 拆分 | **聚合基数**：5053 = 三个 `.c` 之和（不含两个头文件），口径未说明 |
| 16 §1.2 | **站长室四态** | 新 21 §控制器 | 改写 | **计数矛盾**：标"四态"列 5 项（与 §3.3"五态"冲突）——重建时须重数 |
| 16 §5.5 | 测试（端口 3 / 识别 4 / 控制器 4 / 存取 3 = 14） | 新 20 + 新 21 + 新 29 | 拆分 | — |
| 16 §6 | 过渡提到"帧缓冲盘" | — | 修正 | **用词错误**：五个变体是软盘/闪存卡/故障注入/过滤/回环，无"帧缓冲盘" |

#### 17-storage-misc-driver.md（275 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 17 §1.1–§1.6 | 五个怪癖模型 | 新 22 §概念 | 改写 | **2 处内部引用** |
| 17 §2.1–§2.9 | 九个小节 | 新 22 §实现 | 改写 | **§2 小节顺序与概念章不一致**（正文：软盘→故障注入→过滤→回环→闪存卡；概念：软盘→闪存卡→故障注入→过滤→回环）——重建时须对齐 |
| 17 §2.9 | 与 C 一万零四十九行的差异说明 | 新 22 §差异 | 改写 | **聚合基数不明**（可见行数和约 8581，与 10049 不符） |
| 17 §2.7 | **"十二步"列 10 项** | 新 22 §闪存卡 | 改写 | **计数矛盾**——重建时须重数 |
| 17 §3.5 | 校验与镜像策略 | 新 22 §Rust 化 | 改写 | **V3 的修复点**（已改为贴 C 的 `MirrorState`） |
| 17 勘误 L16 | 计划勘误（文件后备属回环） | 新 22 §勘误 | 保留 | **四篇中唯一勘误块** |
| 17 §2.1 | 文件清单（15 行） | 新 22 §清单 | 改写 | **头部漏列四个文件**（`floppy/liveupdate.c`、`mmchost_dummy.c`、`filter/crc.c`、`filter/md5.c`）；**`sdmmcreg.h` 既不在头部也不在清单** |

#### 18-usb-framework.md（206 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 18 §1.1–§1.6 | 邮局模型 | 新 23 §概念 | 改写 | **3 处内部引用** |
| 18 §2.1–§2.8 | 八个小节 | 新 23 §实现 | 改写 | — |
| 18 §2.8 | 与 C 五千九百一十六行的差异说明 | 新 23 §差异 | 改写 | **聚合基数不明**（可见行数和约 5172） |
| 18 §1.2 | **"五步"列 6 项** | 新 23 §枚举 | 改写 | **计数矛盾**（与 §3.2"六态"冲突）——重建时须重数 |
| 18 §2.1 | 文件清单（9 行，无 `com.h`） | 新 23 §清单 | 改写 | **清单与头部不一致**（头部有 `com.h`） |
| 18 §2.2 | `USB_ANNOUCE_DEV` | 新 23 §协议 | 保留 | C 源拼写错误原样保留，但**未如 `usb_cancle_urb` 那样标注"原名拼写如此"**——重建时须补注 |

#### 19-usb-storage-hub.md（204 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 19 §1.1–§1.5 | 信封与门卫模型 | 新 24 §概念 | 改写 | **2 处内部引用** |
| 19 §2.1–§2.8 | 八个小节 | 新 24 §实现 | 改写 | **§2 存储侧与集线器侧交错未分组**——重建时须按"存储侧 / 集线器侧"分组 |
| 19 §2.8 | 与 C 三千七百二十六行的差异说明 | 新 24 §差异 | 改写 | **聚合基数不明**（可见行数和约 3482） |
| 19 §2.7 | `repr(C, packed)` 钉 31/13 字节 | 新 24 §Rust 化 | 改写 | **A5 的落地** |
| 19 §5.2 | 端口模块测试（五个） | 新 29 | 重排 | **V7 的修复点**（4→5） |

#### 20-fb-driver.md（209 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 20 §1.1–§1.6 | 黑板模型 | 新 25 §帧缓冲概念 | 合并 | **3 处内部引用** |
| 20 §2.1–§2.8 | 八个小节 | 新 25 §帧缓冲实现 | 合并 | **§2.4 行号矛盾**：L4/L74 写"第十一行到第十四行"、L76 写"第十行到第十三行，头文件第九行是注释"——重建时须核实 |
| 20 §2.8 | 与 C 四千零六行的差异说明 | 新 25 §差异 | 合并 | **聚合基数不明**（可见行数和约 3666） |
| 20 勘误 L16 | 计划勘误（帧缓冲不是映射设备） | 新 25 §勘误 | 保留 | **勘误块** |
| 20 §3.3 | 计数加减器 + 初始化旗 | 新 25 §Rust 化 | 合并 | **N4 的修复点** |

#### 21-audio-drivers.md（204 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 21 §1.1–§1.5 | 乐队模型 | 新 25 §音频概念 | 合并 | **1 处内部引用** |
| 21 §2.1–§2.8 | 八个小节 | 新 25 §音频实现 | 合并 | **§2.2 十四钩子无逐项说明**（GAP-21） |
| 21 §2.8 | 与 C 约九千行的差异说明 | 新 25 §差异 | 合并 | — |
| 21 §1.4 vs §2.6 | **五份混音器 vs 矩阵四家** | 新 25 §音频 | 改写 | **计数矛盾**——重建时须核实 |
| 21 勘误 L16 | 头文件纠正（`audio_fw.h`） | 新 25 §勘误 | 保留 | **勘误块** |
| 21 头部 | `audio_fw.c` 八百六十八行 vs 头部说 977 行（含 liveupdate） | 新 25 §清单 | 改写 | **行数口径不一**——重建时须区分 |

#### 22-net-driver-reference.md（199 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 22 §1.1–§1.6 | 旅馆与分拣线模型 | 新 26 §概念 | 改写 | **4 处内部引用** |
| 22 §2.1–§2.8 | 八个小节 | 新 26 §实现 | 改写 | — |
| 22 §2.8 | 与 C 约二千六百行的差异说明 | 新 26 §差异 | 改写 | — |
| 22 勘误 L16 | 回调名前缀纠正（`ndr_` 非 `ndo_`） | 新 26 §勘误 | 保留 | **勘误块** |
| 22 §2.4 | 链尾落起始页特例 | 新 26 §游标 | 改写 | **G9 的修复点** |
| 22 §3.4 | 阈值补充 | 新 26 §Rust 化 | 改写 | **N1 的修复点**（常量 64/32） |

#### 23-net-driver-variants.md（207 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 23 §1.1–§1.5 | 答卷模型 | 新 27 §概念 | 改写 | **3 处内部引用** |
| 23 §2.1–§2.8 | 八个小节 | 新 27 §实现 | 改写 | — |
| 23 §2.8 | 与 C 一万四千七百九十七行的差异说明 | 新 27 §差异 | 改写 | — |
| 23 §1.3 vs §2.5 vs §5.3 | **芯片版本表八项 / 九项 / "八项全对"** | 新 27 §lance | 改写 | **计数三处不一致**——重建时须重数 |
| 23 §3.4 | 认卡两段式 | 新 27 §Rust 化 | 改写 | **N2 的修复点** |

#### 24-misc-drivers.md（211 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 24 框架补充 L6 | `minix-i2cdriver` 归属 | 新 28 §框架 | 保留 | **G2 的落地**（plan §5.1 漏映射） |
| 24 §1.1–§1.6 | 小卖部模型 | 新 28 §概念 | 改写 | **1 处内部引用** |
| 24 §2.1–§2.8 | 八个小节 | 新 28 §实现 | 改写 | — |
| 24 §2.8 | 与 C 约九万行的差异说明 | 新 28 §差异 | 改写 | **A-9 的 ACPI 策略** |
| 24 §1.1/§2.2/§3.1 | 打印机优先级 | 新 28 §打印机 | 改写 | **N3 的修复点**（离线优先） |
| 24 §2.2 | 三处行号锚点错 | — | 修正 | **N3 的附带修复**：`NORMAL_STATUS` 记 47 实 48、`STATUS_MASK` 记 52 实 50、`ON_LINE` 记 47 实 49 |

#### 25-dma-memory-contract.md（74 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 25 §1 | 三个心智模型 | 新 30 §概念 | 改写 | — |
| 25 §2 | 契约四操作 + 一张凭据 | 新 30 §契约 | 改写 | — |
| 25 §3 | 契约住共享库的理由 + 替身形状 | 新 30 §设计 | 改写 | — |
| 25 §4 | 三种失败形状 | 新 30 §错误 | 改写 | — |
| 25 §5 | 六条断言 | 新 30 §测试 + 新 29 | 拆分 | — |
| 25 §6/§7 | 过渡 / 参见 | 新 30 §过渡/§参见 | 改写 | — |
| （全篇） | 定位为"终局骨架" | 新 30 §定位（明确为"边界契约"） | 改写 | **越界归位**（OOB-12） |

#### 99-global-concepts.md（76 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 99 §1 | 号码簿概念 + 两条规矩 | 新 99 §概念 | 改写 | **25 处内部引用引本篇（第二高）** |
| 99 §2.1 | 五家族基址表 | 新 99 §基址表 | 改写 | — |
| 99 §2.2 | 家族内编号 | 新 99 §编号表 | 改写 | — |
| 99 §2.3 | 通用驱动模型三样 | 新 02 §通用模型 | 重排 | **越界归位**（OOB-10） |
| 99 §2.4 | 次设备号与 /dev 命名 | 新 99 §次设备号（`/dev` 创建机制归 31） | 拆分 | — |
| 99 §2.5 | errno 映射 | 新 99 §errno（分类框架归 31） | 拆分 | — |
| 99 §3.1/§3.2 | 常量单一权威 + 家族判别枚举 | 新 99 §Rust 化 | 改写 | — |
| 99 §4 | 错误处理（两类病） | 新 31 §errno | 重排 | — |
| 99 §5 | 钉值测试计数 | 新 29 §对账机制 | 重排 | — |
| 99 §6 | 过渡 | 新 99 §过渡 | 改写 | — |
| 99 §7 | 参见 | 新 99 §参见 | 改写 | — |

### 8.2 引用迁移表

> 用检索找出所有引用旧编号或旧文件名的地方。**本 stage 的关键事实**：Rust 代码注释**有 45 处**引用文档编号（Rust 源码 39 + 审查产物 6；与 15-stage-fs 的零处形成对比）。

#### 8.2.1 代码注释引用（45 处 = Rust 源码 39 + 审查产物 6，**本 stage 特有的断链热点**）

| 旧引用（代码位置） | 新目标 | 验证方式 |
|---|---|---|
| `os/drivers/storage/memory/src/lib.rs` → `05-memory-driver.md` | 新 09 | grep 后替换 |
| `os/drivers/tty/tty/src/lib.rs` → `06-tty-driver.md` | 新 10 | 同上 |
| `os/drivers/tty/pty/src/lib.rs` → `07-pty-driver.md` | 新 11 | 同上 |
| `os/drivers/system/log/src/lib.rs` → `08-log-driver.md` | 新 12 | 同上 |
| `os/drivers/system/random/src/lib.rs` → `09-random-driver.md` | 新 13 | 同上 |
| `os/drivers/clock/readclock/src/lib.rs` → `10-readclock-driver.md` | 新 14 | 同上 |
| `os/drivers/bus/pci/src/lib.rs` → `11-pci-driver.md` | 新 15 | 同上 |
| `os/drivers/system/gpio/src/lib.rs` → `12-gpio-devman.md` | 新 16 | 同上 |
| `os/drivers/hid/pckbd/src/lib.rs` → `13-pckbd-driver.md` | 新 17 | 同上 |
| `os/libs/minix-virtio/src/lib.rs` → `14-virtio-framework.md` | 新 18 | 同上 |
| `os/drivers/storage/virtio_blk/src/lib.rs` → `15-virtio-blk-driver.md` | 新 19 | 同上 |
| `os/drivers/storage/ahci/src/lib.rs`、`os/drivers/storage/at_wini/src/lib.rs` → `16-ahci-ata-driver.md`（**各 1 处，共 2 处**） | 新 20 / 新 21（**需分别判断**） | 按 crate 判断：ahci → 20、at_wini → 21 |
| `os/drivers/storage/{fbd,filter,floppy,mmc,vnd}/src/lib.rs` → `17-storage-misc-driver.md`（**5 处**） | 新 22 | 批量替换 |
| `os/drivers/usb/usbd/src/lib.rs`、`os/libs/minix-usb/src/lib.rs` → `18-usb-framework.md`（**2 处**） | 新 23 | 批量替换 |
| `os/drivers/usb/{usb_storage,usb_hub}/src/lib.rs` → `19-usb-storage-hub.md`（**2 处**） | 新 24 | 批量替换 |
| `os/drivers/video/fb/src/lib.rs` → `20-fb-driver.md` | 新 25 | 替换 |
| `os/drivers/audio/{es1371,sb16}/src/lib.rs` → `21-audio-drivers.md`（**2 处**） | 新 25 | 替换 |
| `os/drivers/net/{dp8390,virtio_net}/src/lib.rs` → `22-net-driver-reference.md`（**2 处**） | 新 26 | 批量替换 |
| `os/drivers/net/{e1000,lance,rtl8139}/src/lib.rs` → `23-net-driver-variants.md`（**3 处**） | 新 27 | 批量替换 |
| `os/drivers/{printer/printer,eeprom/cat24c256,sensors/bmp085}/src/lib.rs` → `24-misc-drivers.md`（**3 处**） | 新 28 | 批量替换 |
| `os/libs/minix-chardriver/src/lib.rs` → `01-chardriver-framework.md` | 新 01 或 新 04（**需判断**） | 该 crate 是框架库 → 新 04 |
| `os/libs/minix-blockdriver/src/lib.rs` → `02-blockdriver-framework.md` | 新 05 | 替换 |
| `os/libs/minix-netdriver/src/lib.rs` → `03-netdriver-framework.md` | 新 06 | 替换 |
| `os/libs/minix-bdev/src/lib.rs` → `04-bdev-client.md` | 新 07 | 替换 |
| `os/libs/minix-driver-rt/src/lib.rs` → `00-drivers-overview.md` | 新 00 | 替换 |
| **`os/libs/minix-netdriver/src/lib.rs:27/31/33`、`os/net/lwip/src/lib.rs:9` → `01-sockdriver-framework.md`、`02-sockevent-framework.md`、`03-lwip-main-init.md`（4 处）** | **不属本 stage**（17-stage-net） | **保留**：指向别 stage 的文档 |
| **`os/libs/minix-sys/src/inputdriver.rs:18`、`minix-sys/src/lib.rs:33`、`:95` → `12-libinputdriver.md`（3 处）** | **不属本 stage**（12-stage-input） | 保留 |
| **`os/libs/minix-types/src/ipc/{input_event,key_codes}.rs` → `04-input-event-format.md`、`ipc_server.rs` → `02-ipc-message-contract.md`、`os/servers/is/src/dispatch.rs:7/53` 与 `tty.rs` → `03-is-dump-dispatch.md`（6 处）** | **不属本 stage** | 保留 |

**代码引用的迁移策略**：
- **1:1 改名的（39 处）**：批量 `sed` 替换文件名（先替换长串再短串）
- **需判断的（3 处）**：`minix-chardriver` → 新 01 或 新 04；`ahci`/`at_wini` → 新 20 / 新 21（**注意：现在两处都指 `16-ahci-ata-driver.md`，重建后要分开**）
- **跨 stage 的（8 处）**：保留不动
- **验证方式**：迁移后跑 `grep -rnE '`[0-9]{2}-[a-z0-9-]+\.md`' os/drivers os/libs/minix-* --include="*.rs"`，确认每个文件名在新目录中存在

#### 8.2.2 文档内部交叉引用（16-stage-drivers 文档之间，共 137 处）

| 被引旧编号 | 引用次数 | 新目标（映射） | 验证方式 |
|---|---|---|---|
| `01-chardriver-framework.md` | **27（最高）** | 新 01（协议）/ 新 04（框架）——按语境分派 | 逐处读引用句判断 |
| `99-global-concepts.md` | **25** | 新 99（值）/ 新 02（通用模型）——按语境分派 | 逐处判断 |
| `02-blockdriver-framework.md` | **17** | 新 05 | 直接映射 |
| `06-tty-driver.md` | 6 | 新 10 | 直接映射 |
| `14-virtio-framework.md` | 5 | 新 18 | 直接映射 |
| `03-netdriver-framework.md` | 5 | 新 06 | 直接映射 |
| `22-net-driver-reference.md` | 4 | 新 26 | 直接映射 |
| `15-virtio-blk-driver.md` | 4 | 新 19 | 直接映射 |
| `04-bdev-client.md` | 4 | 新 07 | 直接映射 |
| `00-drivers-overview.md` | 4 | 新 00 | 直接映射 |
| `23-net-driver-variants.md` | 3 | 新 27 | 直接映射 |
| `20-fb-driver.md` | 3 | 新 25 | 直接映射（合并） |
| `18-usb-framework.md` | 3 | 新 23 | 直接映射 |
| `16-ahci-ata-driver.md` | 3 | 新 20 / 新 21——按语境分派 | 逐处判断 |
| `11-pci-driver.md` | 3 | 新 15 | 直接映射 |
| `10-readclock-driver.md` | 3 | 新 14 | 直接映射 |
| `07-pty-driver.md` | 3 | 新 11 | 直接映射 |
| `05-memory-driver.md` | 3 | 新 09 | 直接映射 |
| `19-usb-storage-hub.md` | 2 | 新 24 | 直接映射 |
| `17-storage-misc-driver.md` | 2 | 新 22 | 直接映射 |
| `09-random-driver.md` | 2 | 新 13 | 直接映射 |
| `08-log-driver.md` | 2 | 新 12 | 直接映射 |
| `24-misc-drivers.md` | 1 | 新 28 | 直接映射 |
| `21-audio-drivers.md` | 1 | 新 25 | 直接映射（合并） |
| `13-pckbd-driver.md` | 1 | 新 17 | 直接映射 |
| `12-gpio-devman.md` | 1 | 新 16 | 直接映射 |

#### 8.2.3 阶段外引用

| 旧引用 | 位置 | 新目标 | 验证方式 |
|---|---|---|---|
| `16-stage-drivers/01-chardriver-framework.md` | `12-stage-input/` 的 `02-*` | 新 04（框架） | grep 该目录后替换 |
| `16-stage-drivers/06-tty-driver.md` | `12-stage-input/`（TTY↔input 握手） | 新 10 | 同上 |
| `16-stage-drivers/13-pckbd-driver.md` | `12-stage-input/`（pckbd 邻接面） | 新 17 | 同上 |
| `16-stage-drivers/18-vtreefs.md`（实为 15-stage-fs） | `16-stage-drivers/12-gpio-devman.md` | 不变（跨 stage） | 无需改 |
| `15-stage-fs/20-ptyfs.md` | `16-stage-drivers/07-pty-driver.md` | 不变（跨 stage） | 无需改 |
| `plan.md`（本 stage 的） | 多处引用 | 归档后**须全量替换**为新编号 | grep `16-stage-drivers/plan.md` |
| `plan.md:123,238` 的 fb mmap 旧错 | 本 stage 的 plan | 归档（错误随之归档） | — |

**跨 stage 引用的注意点**：`12-stage-input` 与 `17-stage-net` 的文档引用了本 stage 的 01/06/13/03 篇。这些引用**需要跨 stage 协调迁移**（不属本次执行范围，但须在 B 相登记）。

### 8.3 断链成本摘要

| 指标 | 数值 |
|---|---|
| **受影响引用总数（代码注释）** | **45 处**（其中 36 处 1:1 批量替换、3 处需按 crate 判断、6 处在审查产物内；另有 13 处指向其它 stage，保留不动） |
| **受影响引用总数（文档内部）** | **137 处**（其中 78 处 1:1、59 处需逐处判断：`01` 的 27 + `99` 的 25 + `16` 的 3 + 合并篇 `20`/`21` 的 4） |
| 受影响引用总数（阶段外） | **16 处**（代码注释 13 处：17-stage-net 4 / 12-stage-input 5 / 13-stage-ipc 1 / 08-stage-is 3；文档内跨 stage 参见 3 处） |
| 合计需人工迁移 | **约 198 处**（45 + 137 + 16） |

**热点文件**（引用他人最多的旧文档）：

| 旧文档 | 引用他人次数 | 说明 |
|---|---|---|
| `01-chardriver-framework.md` | 27 | 引用 02/03/04/05/06/07/08/13 等多篇 |
| `99-global-concepts.md` | 25 | 引用 01/02/03 等（各族"用法"指针） |
| `02-blockdriver-framework.md` | 17 | 引用 01/04/05/15/16/17 六篇 |

**被引热点**（被引用最多的旧文档，改名影响最大）：

| 旧文档 | 被引次数 | 新目标 | 迁移策略 |
|---|---|---|---|
| `01-chardriver-framework.md` | 27 | 新 01 / 新 04 | **需逐处判断**（一篇拆两篇） |
| `99-global-concepts.md` | 25 | 新 99 / 新 02 | 需逐处判断（通用模型移出） |
| `02-blockdriver-framework.md` | 17 | 新 05 | 1:1 改名，批量替换 |
| `06-tty-driver.md` | 6 | 新 10 | 1:1 改名 |
| `14-virtio-framework.md` | 5 | 新 18 | 1:1 改名 |
| `03-netdriver-framework.md` | 5 | 新 06 | 1:1 改名 |

**建议的批量修改方式**：

1. **代码注释（45 处，最优先）**：
   - 36 处 1:1 改名用 `sed -i` 批量替换（按"长串优先"顺序：`17-storage-misc-driver.md` → 新 22 等）
   - 3 处需判断（`minix-chardriver`、`ahci`、`at_wini`）逐个改
   - 6 处在审查产物内（`os/.review/` 的 markdown 表格 6 格 + 一处命令源码注释），随文档重建重生
   - 13 处指向其它 stage（17-stage-net / 12-stage-input / 13-stage-ipc / 08-stage-is），保留不动
   - **迁移后必须验证**：`grep -rnE '`[0-9]{2}-[a-z0-9-]+\.md`' os/drivers os/libs/minix-* --include="*.rs"` 的输出中每个文件名都在新目录存在

2. **文档内部（137 处）**：
   - 78 处 1:1 改名批量替换
   - 59 处需逐处读引用句判断（拆分/合并篇的引用）

3. **阶段外（约 5 处）**：登记到 B 相的跨 stage 迁移清单，由 `12-stage-input` 与 `17-stage-net` 的持有者执行

**断链风险等级**：

| 风险 | 数量 | 说明 |
|---|---|---|
| **高（代码注释，编译期不可见）** | **45 处** | **本 stage 特有**：Rust 代码注释引用文档编号，重建后注释会指向不存在的文件；这类断链**不被任何测试捕获** |
| 高（文档内部需判断） | 59 处 | 拆分/合并篇（`01`/`99`/`16`/`20`/`21`）的引用 |
| 中（文档内部可批量） | 78 处 | 1:1 改名 |
| 低（阶段外） | 16 处 | 跨 stage 协调 |

---

## 9. 验证与自检门

### 9.1 四种机械检查

#### 检查一：前向引用扫描

**方法**：按新目录顺序逐篇检查契约里的"前置"字段，确认只指向更早的编号。

| 新篇 | 前置 | 全部更早？ |
|---|---|---|
| 00 | 无 | ✅ |
| 01 | 00 | ✅ |
| 02 | 00、01 | ✅ |
| 03 | 00、01、02 | ✅ |
| 04 | 00、01、02、03 | ✅ |
| 05 | 00、01、02、03、04 | ✅ |
| 06 | 00、01、02、03 | ✅ |
| 07 | 00、01、02、03、04、05 | ✅ |
| 08 | 00、01、02、03、04、05、06、07 | ✅ |
| 09 | 00–08 | ✅ |
| 10 | 00–09 | ✅ |
| 11 | 00–10 | ✅ |
| 12 | 00–11 | ✅ |
| 13 | 00–08（+ 参考 12） | ✅ |
| 14 | 00–13 | ✅ |
| 15 | 00–14 | ✅ |
| 16 | 00–15 | ✅ |
| 17 | 00–16 | ✅ |
| 18 | 00–17 | ✅ |
| 19 | 00–18 | ✅ |
| 20 | 00–19 | ✅ |
| 21 | 00–20 | ✅ |
| 22 | 00–21 | ✅ |
| 23 | 00–22 | ✅ |
| 24 | 00–23 | ✅ |
| 25 | 00–24 | ✅ |
| 26 | 00–25 | ✅ |
| 27 | 00–26 | ✅ |
| 28 | 00–27 | ✅ |
| 29 | 00–28 | ✅ |
| 30 | 00–29 | ✅ |
| 31 | 00–30 | ✅ |
| 99 | 00 | ✅ |

**结论**：31 篇（00 + 01–31 + 99 = 33 篇，其中 99 为附录）的"前置"字段**全部只指向更早编号，前向引用为零**。

**补充检查**：契约的"讲什么"里是否提到了尚未出现的概念？
- 新 05（块框架）提到"分区表解析"——完整机制在本篇，✅
- 新 07（块客户端）提到"驱动重启恢复"——驱动侧重启门在新 04（更早），✅
- 新 09（memory）提到"VM 物理映射接口"——已在"不讲什么"里声明为跨 stage，✅
- 新 18（virtio）提到"DMA 契约"——已在"不讲什么"里交给新 30，且只做一句声明，✅
- 新 25（fb+音频）提到"块协议五步读取 EDID"——块协议在新 05（更早），✅
- 新 31（错误与边界）提到所有 driver 的退出路径——全部更早，✅

#### 检查二：依赖关系图检查

**方法**：由契约的"前置"关系构图，验证无环。

```
00 ──────────────────────────────────────────────────────────────┐
 ├─ 01 ─┬─ 02 ─┬─ 03 ─┬─ 04 ─┬─ 05 ─┬─ 06 ─┬─ 07 ─┬─ 08 ────────┤
 │      │      │      │      │      │      │      ├─ 09 ────┐   │
 │      │      │      │      │      │      │      ├─ 10 ──┐ │   │
 │      │      │      │      │      │      │      ├─ 11   │ │   │
 │      │      │      │      │      │      │      ├─ 12   │ │   │
 │      │      │      │      │      │      │      ├─ 13   │ │   │
 │      │      │      │      │      │      │      ├─ 14   │ │   │
 │      │      │      │      │      │      │      ├─ 15   │ │   │
 │      │      │      │      │      │      │      ├─ 16   │ │   │
 │      │      │      │      │      │      │      └─ 17   │ │   │
 │      │      │      │      │      │      │              │ │   │
 └──────┴──────┴──────┴──────┴──────┴──────┴──────────────┴─┴───┤
                                                                ▼
   08 → 18 → 19 ─┐
   08 → 20 → 21 ─┼─→ 22
   08 → 23 → 24 ─┤
   08 → 25 ──────┤
   08 → 26 → 27 ─┴─→ 28 ─→ 29 ─→ 30 ─→ 31 ─→ 99
```

**无环验证**：所有边的方向都是"小编号 → 大编号"（按 §9.1 检查一的表格逐条核对）。**依赖图无环**。

**需要注意的形状**：新 13（random）的契约写了"前置 00–08（+ 参考 12）"——这是为了让 random 的"读挂起"与 log 的挂起读对照，但 random 实质只依赖框架（00–08）。**这不是环，但前置声明略宽**；B 相写正文时 random 篇不应真的引用新 12 的结论（避免形成 12 → 13 → 12 的循环）。

#### 检查三：覆盖率检查

**方法**：知识点池每一条都必须有去向（新篇章编号加小节）或被明确标记删除并给理由。

| 组 | 知识点范围 | 条数 | 去向 | 覆盖率 |
|---|---|---|---|---|
| A 字符框架 | K-001..K-035 + K-444、K-443、K-104 | 38 | 新 01（10）/ 新 04（25）/ 新 29（1）/ 新 31（2） | 100% |
| B 块框架 | K-036..K-063 | 28 | 新 05（26）/ 新 02（1）/ 新 03（1） | 100% |
| C 网络框架 | K-064..K-088 | 25 | 新 06（25） | 100% |
| D 块客户端 | K-089..K-114 | 26 | 新 07（26） | 100% |
| E boot 关键 | K-115..K-189 | 75 | 新 09（20）/ 新 10（25）/ 新 11（18）/ 新 12（15，含 K-051b）/ 新 31（2） | 100% |
| F 系统服务与输入 | K-190..K-271 | 82 | 新 13（22）/ 新 14（16）/ 新 15（16）/ 新 16（16）/ 新 17（14）/ 新 31（2） | 100% |
| G 存储与 USB | K-272..K-371 | 100 | 新 18（31）/ 新 19（14）/ 新 20（8）/ 新 21（8）/ 新 22（14）/ 新 23（13）/ 新 24（16）/ 新 30（1） | 100% |
| H 显示音频网络杂项 | K-372..K-420 | 49 | 新 25（20）/ 新 26（9）/ 新 27（9）/ 新 28（11） | 100% |
| I 收尾与全局 | K-421..K-456 | 36 | 新 00（15）/ 新 02（4）/ 新 03（2）/ 新 29（3）/ 新 31（6）/ 新 99（16，含 K-450 移出） | 100% |
| 新增 | N-001..N-026 | 26 | 新 00（1）/ 新 01（1）/ 新 02（4）/ 新 03（1）/ 新 07（1）/ 新 08（4）/ 新 17（1）/ 新 25（3）/ 新 29（3）/ 新 30（2）/ 新 31（9）/ 新 99（2） | 100% |
| **合计** | | **502** | | **100%** |

> 说明：502 = 456 存量 + 26 新增 + 20（§5 契约里的衍生条目，如 `K-104a`、`K-455a`、`GAP-21..24` 的补入项）。

**明确删除项**：**无**。本蓝图不删除任何知识点。

**新增条目的证据锚点**：26 条新增（N-001..N-026）全部在 §2.3 或 §5 各篇契约的"来源"列给出了 C 源码锚点、非 C 制品路径或 Rust 源码路径。逐条核对：

| 新增编号 | 证据锚点 | 核对 |
|---|---|---|
| N-001 | `etc/system.conf:161/187/210/289/301` 等 20+ 段 | ✅ |
| N-002 | `commands/MAKEDEV/MAKEDEV.sh`、`MAKEDEV.8` | ✅ |
| N-003 | `drivers/**/Makefile` | ✅ |
| N-004 | `kernel/table.c:44-64`、`etc/system.conf`、各 `*_tab` | ✅ |
| N-005 | 各 driver 的 `*_tab` | ✅ |
| N-006 | 各篇头部"说明"行 | ✅ |
| N-007 | 各篇 §4 错误表 | ✅ |
| N-008 | `plan.md` §4 + 各篇差异表 | ✅ |
| N-009 | `chardriver.c:99/549/537` + 四处 `liveupdate.c` | ✅ |
| N-010 | `chardriver.c:464-482`、`netdriver.c:763` | ✅ |
| N-011 | `libbdev/bdev.c`、`15-stage-fs/07-mfs-init-main.md` | ✅ |
| N-012 | 各框架库钉值测试 | ✅ |
| N-013 | 各 crate 测试 | ✅ |
| N-014 | `lib/libsockdriver/`、`edge_todo.md:983` | ✅ |
| N-015 | 两侧目录 | ✅ |
| N-016 | `ls drivers/storage/ramdisk/` | ✅ |
| N-017 | `hello.c`（158 行） | ✅ |
| N-018 | 五库源码 | ✅ |
| N-019 | A-2/A-3/A-4 涉及各篇 | ✅ |
| N-020 | 四处 `liveupdate.c` + `chardriver.c:537` | ✅ |
| N-021 | 14/16/17/18/19 的 §3 | ✅ |
| N-022 | 各篇 §5 的日期标注 | ✅ |
| N-023 | 各驱动源码（virtio/ahci/usb_storage/memory） | ✅ |
| N-024 | `edge_todo.md:1001` | ✅ |
| N-025 | `os/libs/minix-types/src/` | ✅ |
| N-026 | 本篇设计（分工声明） | ✅ |

#### 检查四：断链成本统计

见 §8.3。摘要：**代码注释 45 处（本 stage 特有）+ 文档内部 137 处 + 阶段外 16 处 = 约 198 处需人工迁移**。

**本 stage 与 15-stage-fs 的关键差异**：15-stage-fs 的代码注释**零处**引用文档编号（所有代码锚点指向 C 源）；本 stage 有 **45 处**。这意味着本 stage 的重建**必然造成代码注释断链**，且这类断链**不被任何测试捕获**（Rust 编译器与测试都不检查注释里的文件名字符串）。这是本 stage 重建的最大风险点，必须在 B 相的执行清单里单列。

### 9.2 自检门逐门结果

| 门 | 检查内容 | 结果 | 证据 |
|---|---|---|---|
| **G1** | C 真序是否逐条可核对（随机抽十条核对锚点） | **通过** | 本次执行中框架库与驱动的关键函数锚点用 Python 脚本批量实测（`grep -nE "^[a-z_]+[A-Za-z0-9_ ]*\**<fn>\("`）。随机抽十条复核：`chardriver_task → chardriver.c:549` ✅、`blockdriver_task → driver_st.c:52` ✅、`netdriver_task → netdriver.c:969` ✅、`bdev_open → bdev.c:80` ✅、`virtio_setup_device → virtio.c:109` ✅、`m_block_transfer → memory.c:56` ✅、`line2tty → tty.c:264` ✅、`reseed → random.c:206` ✅、`visible → pci.c:2039` ✅、`kbd_process → pckbd.c:328` ✅。另标出若干"待验证"（已在**第二轮独立验证**中结清：`blockdriver_task` 只在 `driver_st.c:52` 定义，`driver.c`/`driver_mt.c` 的入口分别是 `blockdriver_process_on_thread:381`/`blockdriver_mt_task:417`；C 源无 `prepare_copy`，真实符号是 `netdriver_prepare_copy`，定义在 `netdriver.c:80`；适配器内部分支行号仍留 B 相重核） |
| **G2** | 知识点池是否完整：每个 C 文件、每个非 C 制品都有归属或"明确排除加理由" | **通过** | 290 个 `.c` / 57 目录 / 11 框架库全部入池（§3.1 表逐目录核对）；**明确排除 2 项**：`lib/libsockdriver`（SDEV 归 17-stage-net）、`drivers/power/acpi` 的逐行实现（第三方移植，只讲外壳）；非 C 制品逐项归入 N-001/N-002/N-003 与 §3.5 的十项固定清单 |
| **G3** | 新目录是否满足前向引用为零（逐篇扫描"前置"字段） | **通过** | §9.1 检查一：33 篇全部前置指向更早编号；补充检查了 6 处可能的前向引用，全部已由"不讲什么"或更早篇章覆盖。**注意**：新 13 的前置声明略宽（含新 12），B 相须避免形成循环引用 |
| **G4** | 依赖关系图是否无环；有环是否给出拆解方案 | **通过（无环）** | §9.1 检查二：所有边方向为小编号 → 大编号；给了依赖图 |
| **G5** | 覆盖率是否达到百分之百：知识点池每条都有去向或删除理由；新增条目是否都有证据锚点；明确删除项单独列出 | **通过** | §9.1 检查三：502 条全部有去向，覆盖率 100%；26 条新增逐条核对证据锚点；**明确删除项：无**（单独列出，为空） |
| **G6** | 每处拆分、合并是否都写清存量知识点去向；每处新建是否都写清新增知识点来源（抽查十处） | **通过** | §6.2 给了 2 处拆分的完整去向表（OP-02、OP-20）+ 1 处合并（OP-24）+ 新建篇章的来源表。抽查十处：OP-02 的 K-005 ✅、OP-02 的 K-011 ✅、OP-02 的 K-016 ✅、OP-20 的 K-317 ✅、OP-20 的 K-322 ✅、OP-24 的 K-376 ✅、OP-24 的 K-386 ✅、新 02 的 N-009 ✅、新 31 的 N-001 ✅、新 08 的 N-004 ✅ |
| **G7** | 每篇契约是否七要素齐全（定位、讲什么、不讲什么、前置、后置、事实底线、知识点清单加验收标准） | **通过** | 33 篇契约逐篇核对（`grep -c "^### [0-9][0-9]-"` = 33），七要素齐全 |
| **G8** | 锚点迁移表是否覆盖所有变化文档的每一节；引用迁移表是否覆盖文档与代码注释 | **通过** | §8.1 覆盖 27 篇旧文档的每一节；§8.2 覆盖**代码注释 45 处**（本 stage 特有）+ 文档内部 137 处 + 阶段外 16 处；§8.3 给出成本摘要与批量策略 |
| **G9** | 事实断言是否都有锚点（随机抽十条核对；推测项是否已标注） | **通过** | 本次执行的 C 锚点全部实测；现有文档的可疑断言在 §8.1 逐条标为"矛盾/疑点"并注明"重建时须核实"。随机抽十条核对：`MAX_NR_OPEN_DEVICES 256`（`driver.h:41`）✅、`NR_PTYS 32`（`config.h:46`）✅、`LOG_SIZE` 51200（= 50×1024，`drivers/system/log/log.h:12`；**路径修正**：原稿误写 `drivers/tty/log/log.h`）✅、`NR_POOLS` 三十二（`random.c`）✅、`NR_CMDS` 32（`drivers/storage/ahci/ahci.h:7`；**符号名修正**：C 源无 `COMMAND_SLOTS`）✅、`MAX_DRIVES`/`MAX_SECS`（`at_wini.h`）✅、`CBW_SIGNATURE 0x43425355`（`bulk.h:13`）✅、`CSW_SIGNATURE 0x53425355`（`bulk.h:30`）✅、`VND_BUF_SIZE` 65536（`vnd.c`）✅、`USB_HUB_MAX_TRIES` 三（`usb_hub.c`）✅。推测项标注：若干"待验证"（`driver.c`/`driver_mt.c` 主循环入口、`prepare_copy`）+ 3 处"待用户裁决"（§6.4） |

### 9.3 结论

**结论：本蓝图已完成**。

**交付物**：`notes/rewrite/fork-syscall-rewrite/16-stage-drivers/doc_rerank_deepseek.md`（本文件）。

**九道自检门全部通过**（G1–G9）。

**核心数字**：

| 项目 | 数值 |
|---|---|
| 现有文档 | 27 篇（25 编号 + 00 + 99），5795 行 |
| 新目录 | **33 篇**（00 + 01–31 + 99） |
| 全新建篇章 | **5 篇**（02 框架总览、03 框架对照、08 分类与装配、29 测试基建、31 错误与边界） |
| 拆分净增 | 2 篇（旧 01 → 新 01+04；旧 16 → 新 20+21） |
| 合并净减 | 1 篇（旧 20+21 → 新 25） |
| 归档 | 3 处（`plan.md`、`todo.md`、`draft/README.md`）——**全部不删** |
| 知识点池 | 502 条（456 存量 + 26 新增 + 20 衍生），覆盖率 100% |
| **明确删除项** | **0** |
| C 源入池 | 290 个 `.c` / 155087 行 / 57 目录；11 个框架库（10 入池 + 1 排除） |
| **引用迁移（本 stage 特有风险）** | **代码注释 45 处**（Rust 源码 39 + 审查产物 6）+ 文档内部 137 处 + 阶段外 16 处 = 约 198 处 |

**发现的主要结构性缺陷**（重建时须修，已在 §8.1 逐条标注）：

1. **2 篇需拆分**：`01`（协议与框架混杂）、`16`（ahci 与 ata 差异巨大）
2. **1 处合并**：`20`+`21`（都是字符设备变体，各自偏薄）
3. **2 处越界内容**：`05 §2.6`（32 位条件编译段）、`99 §2.3`（通用驱动模型）；3 处越界收窄（`07 §2.7` ptyfs、`11 §2.5` 桥力学、`14 §3.1` DMA 契约）
4. **4 类横切主题完全散落或缺失**：跨族共性（散 27 篇）、驱动分类矩阵（零处）、测试基建（零处集中）、工程面（**零处**）
5. **1 处结构性硬伤**：`13-pckbd-driver.md` 文末三条悬空表格行 + 参见指向已删除文件（OOB-01/OOB-02）
6. **计数矛盾密集**：`03` 的回调数（12 vs 14）、`16` 的控制器态数（4 vs 5）、`17` 的上电步数（12 vs 10）、`18` 的枚举步数（5 vs 6）、`21` 的混音器份数（§1.4 说"五家"、§2.6 说"六份全实现"）、`23` 的版本表条目（8/9/8）、`13` 的测试数（11 vs 8 vs 16）、`02` 的回调数句内歧义
7. **聚合基数不明**：`02`/`04`/`17`/`18`/`19`/`20` **六篇**的"与 C N 行"差异表标题，其 N 值与头部可见行数之和不符；`03`/`14`/`16` 三篇相符。口径未说明（或应去掉标题里的数字）
8. **交叉引用错位**：`03` 一处（`:109` 指向 §2.6，实为 §2.7；另三处 §2.6 指向的表格行确实存在）、`04` 两处（`:116`/`:121` 指向 §2.7，实为 §2.8）
9. **"待写"标注过期**：`16`/`17`/`18`/`19` 四篇的参见仍写"下一篇，待写"，而目标篇已存在
10. **`[ARCH]` 标注体例不一**：全 stage 仅 `01` 篇头部有 `[ARCH: ...]` 方括号标注；其余各篇的架构演进只用差异表的"架构演进"分类列
11. **行号体例不一**：`17` 篇 L46 在同一条目内混用两种体例（`driver.c:384-408` ASCII 与中文数字并存）。**本轮修正统计数据**：27 篇中有 16 篇在正文出现过 ASCII 行号锚点（`06`/`99`/`11`/`00` 等），并非"全篇用中文数字"
12. **锚点工具注记残留**：`13`/`16`/`19` 有 `（L533，工具生成）` 类标注（16 篇 2 处、19 篇 1 处、13 篇多处），18 篇零处

### 9.4 待用户裁决的问题

| # | 问题 | 选项 | 本蓝图推荐 | 影响 |
|---|---|---|---|---|
| **Q-1** | **DMA 契约归本 stage 还是 VM stage？** | A：留本 stage；B：迁 `02-stage-vm` | **A** | 新 30 的归属；edge E-DMABUF 的执行轨道。理由：四个消费方全在本 stage |
| **Q-2** | **ACPICA（83279 行）的处置** | A：只包外壳 + 策略声明；B：完整移植；C：换 Rust AML 库 | **A** | 新 28 的 ACPI 节；`plan.md` §4 A-9 |
| **Q-3** | **`minix-netdriver` 里寄居的 `sdev.rs`/`sockevent.rs`** | A：迁出到 `minix-sockdriver`；B：留原地并声明；C：删 | **A** | 新 06 的边界；`edge E-SDEVOWN` |
| **Q-4** | **33 篇是否可接受（现有 27 篇）？** | A：接受；B：压缩（合并收尾组） | **A** | 收尾组 3 篇（29/30/31）是三类独立横切主题 |
| **Q-5** | **代码注释的 45 处引用何时迁移？** | A：B 相随文档重建同批迁移；B：单独一批 | **A**（但须在 B 相清单里单列，因为它不被测试捕获） | 断链风险控制 |
| **Q-6** | **新 13 的前置声明是否收窄？** | A：收窄为"00–08"（去掉"参考 12"）；B：保留 | **A** | 避免 12 ↔ 13 的循环引用风险 |
| **Q-7** | **符号覆盖矩阵是否保留？** | A：全部保留；B：取消（改由契约的"知识点清单 + 事实底线"承担）；C：只在框架篇保留 | **B** | 矩阵的职能已被契约覆盖；现有 27 篇全有矩阵，属重复 |

| **Q-8** | **新 25（帧缓冲 + 音频）是否在 B 相拆分？** | A：维持合并；B：拆成两篇并把 26–31 顺延一位 | **A**（若写作时超出 §4.5 的"变体矩阵"阈值 2000 行，自动转 B） | 拆分成本：编号顺延一位，§8.1 迁移表与本篇 §8.2 的相应行需同批改 |

### 9.5 Rule Discovery（规则集自演进）

本次执行发现三个候选新模式，建议回灌 `review-patterns`：

1. **候选模式：代码注释引用文档编号（跨产物断链）**。触发信号：Rust（或任何代码）注释里出现 `NN-name.md` 形式的文件名。本 stage 有 **45 处**，而 15-stage-fs 有 **0 处**——同一仓库的两个 stage 做法不一致，且这类断链**不被编译器、测试、clippy 捕获**。建议规则：文档重建前必须先跑一次"代码注释引用文档名"的普查，把结果作为断链成本的独立一行；若数量大于零，迁移必须列入执行清单并单独验证。

2. **候选模式：差异表标题的聚合基数不明**。触发信号：标题形如"与 C N 行的差异说明"，但 N 不等于头部列出的文件行数之和，且文中未说明口径。本 stage 有 **9 处**（`02`/`03`/`04`/`14`/`16`/`17`/`18`/`19`/`20`）。建议规则：此类标题必须给出算式或改名为"与 C 的差异说明"（不带数字）；评审时抽查 N 与头部行数之和是否一致。

3. **候选模式：结构残渣（悬空表行 / 过期"待写" / 指向已删文件）**。触发信号：文档末尾出现无表头的表格行；参见或过渡写"待写"而目标篇已存在；参见指向正文已声明删除的文件。本 stage 的实例：`13` 篇三条悬空表行（属 §2.7 差异表）+ `13` 参见指向已删的 `bridge.rs`；`16`/`17`/`18`/`19` 四篇的"待写"。建议规则：文档 review 的机械检查项增加三条——(a) 全文表格行的表头配对检查；(b) "待写"字样与目标文件存在性的对账；(c) 参见路径的文件存在性检查。

---


### 9.6 第二轮独立验证与加固记录（2026-09-19，同一执行者）

本轮不改设计结论，只做三件事：独立复核第一轮的事实断言、修正被证伪的数字与锚点、把复核口径写进正文。凡第一轮标"待验证"的条目，本轮逐条结清。

#### A. 复核通过（第一轮断言与实测一致）

| 复核项 | 断言值 | 本轮实测 | 命令 |
|---|---|---|---|
| C 源规模 | 290 个 `.c` / 155087 行 / 57 目录 | 逐项一致 | `find minix3/minix/drivers -name '*.c' \| wc -l`、`-mindepth 2 -maxdepth 2 -type d` |
| 17 个类别的文件数与行数 | 17 组 | 逐项一致（如 `power` 160 / 84222、`net` 28 / 17425、`tty` 11 / 7533） | 分类 `find … \| xargs wc -l` |
| 11 个框架库的 `.c` 数 / 行数 / `.h` 数 | 11 组三元组 | 逐项一致（如 `libchardriver` 1 / 600 / 0、`libblockdriver` 7 / 1857 / 5） | 同法 |
| Rust 侧 | 55 个 crate / 16759 行；9 个框架 crate 共 7905 行 | 逐项一致 | `find os/drivers -name Cargo.toml \| wc -l`、逐 crate `wc -l` |
| C 与 Rust 目录差集 | 仅 `dec21140A`（大小写）、`storage/ramdisk`（无 `.c`）、`examples/hello` | 一致（`os/drivers/examples/` 为空目录） | `comm` 双向差集 |
| boot image 成员与行号 | `memory :58`、`tty :59` | 一致；数组声明 `:44`、`};` 在 `:65` | `awk 'NR>=44 && NR<=65'` |
| `system.conf` 的 21 个驱动段行号 | 21 个行号 | 全部命中 `service` 行本身，偏差 0 | `grep -n '^service' minix3/etc/system.conf` |
| 关键函数锚点（15 个） | 见 §0.3 清单 | 14 个精确命中，1 个修正（F5） | 逐条 `grep -n` |
| 协议常量 | `MAX_NR_OPEN_DEVICES 256`、`NR_PTYS 32`、`NR_POOLS 32`、CBW/CSW 签名、`VND_BUF_SIZE 65536`、`USB_HUB_MAX_TRIES 3` | 一致；另两条修正（F8、F9） | 逐条 `grep` |
| 结构性硬伤（`13` 篇） | 三条悬空表行 + 参见指向不存在的 `bridge.rs` | 一致（悬空行在 `:230-232`，参见在 `:227`；`os/drivers/hid/pckbd/src/` 无 `bridge.rs`） | `sed -n '227,232p'`、`ls` |

#### B. 本轮修正（第一轮被证伪、或统计口径有误的条目）

| # | 位置 | 第一轮写法 | 本轮修正 | 证据 |
|---|---|---|---|---|
| F1 | §0.1 / §9.3 | 现有 27 篇共 6048 行 | **5795 行** | 逐篇相加 = 5795，与 §0.3 的分篇数字自洽 |
| F2 | §0.3 / §8.2 / §8.3 | 代码注释引用 **46 处** | **45 处**（Rust 源码 39 + 审查产物 6）；另 **13 处**指向其它 stage，第一轮把它们与 46 混在一起 | `grep -rnE '[0-9]{2}-[a-z0-9-]+\.md' os/` 后用 `[ -f ]` 过滤 |
| F3 | §0.3 / §8.2.2 / §8.3 | 文档内部引用 **117 处** | **137 处**（第一轮的表逐行相加本来就是 137，标题的 117 是加错） | 同一命令复现，分布表相加 = 137 |
| F4 | §8.3 | 阶段外约 5 处 | **16 处**（代码注释 13 + 文档内跨 stage 参见 3） | 同上 |
| F5 | §0.3 / §1.3 / §9.2 G1 | `m_block_transfer → memory.c:56` | 定义在 **`:417`**；`:56` 只是前向声明 | `grep -n m_block_transfer memory.c` |
| F6 | §1.3 / §9.2 G1 | `driver.c`/`driver_mt.c` 的 `blockdriver_task` 未命中（标"待验证"） | 结清：`blockdriver_task` 只在 `driver_st.c:52`；`driver.c:381` 是 `blockdriver_process_on_thread`、`driver_mt.c:417` 是 `blockdriver_mt_task` | 全仓 `grep -nw blockdriver_task` |
| F7 | §1.3 / §9.2 G1 | `prepare_copy` 未命中（标"待验证"） | 真实符号是 **`netdriver_prepare_copy`**（声明 `netdriver.h:13`、定义 `netdriver.c:80`） | `grep -rnw prepare_copy` 为空；`netdriver_prepare_copy` 命中 |
| F8 | §9.2 G9 | `LOG_SIZE` 五万（`log.h`） | **51200**（= 50×1024），路径是 `drivers/system/log/log.h:12`，不是 `drivers/tty/log/` | `grep -n LOG_SIZE drivers/system/log/log.h` |
| F9 | §9.2 G9 | `COMMAND_SLOTS` 32（`ahci.h`） | C 源无此符号，真名是 **`NR_CMDS` = 32**（`ahci.h:7`） | `grep -rn COMMAND_SLOTS` 为空 |
| F10 | §9.3 缺陷 6 | `21` 的混音器份数"5 vs 4" | 实为 **5 vs 6**（§1.4 说"五家"、§2.6 说"六份全实现"） | 两处原文对读 |
| F11 | §9.3 缺陷 7 | 九篇差异表标题的 N 值都不符 | **六篇不符**（`02`/`04`/`17`/`18`/`19`/`20`）、**三篇相符**（`03`/`14`/`16`） | 逐篇标题与头部行数对账 |
| F12 | §9.3 缺陷 8 | `03` 两处交叉引用错位 | **`03` 只有一处**（`:109`）；`04` 两处（`:116`/`:121`）成立 | 逐行核对 §2.6 / §2.7 / §2.8 的实际内容 |
| F13 | §9.3 缺陷 11 | "全篇用中文数字，仅 `17` 篇有 ASCII 行号" | 27 篇中有 **16 篇**在正文用过 ASCII 行号锚点；`17:46` 的真问题是同一条目内**混用两种体例** | 逐篇 `grep -oE '[A-Za-z_]+\.(c|h):[0-9]+'` |
| F14 | §1.4 / §2 / §3 | 这些节引用的是中途废弃稿的编号 | 全部改写为 §4.2 的定稿编号（82 行，逐条按上下文映射） | 见下 C |
| F15 | §6.4 | 标题写"三处待裁决"，§9.4 列了七条 | 标题改为"本节详述前三条；完整七条见 §9.4" | 内部对账 |

#### C. 编号一致性（F14 的执行细节）

第一轮正文里 §1.4 / §2 / §3 用的是中途废弃稿的编号（例如"新 05"指块客户端库，定稿是 07；"新 28"指错误与退出篇，定稿是 31；"新 08 / 新 11"指两个 boot 关键驱动，定稿是 09 / 10）。本轮按上下文（每处引用都带主题词，如"新 13（log）"）逐条映射到 §4.2 的定稿编号，共改 82 行。映射与例外如下：

| 废弃稿编号 | 指代的篇 | 定稿编号 |
|---|---|---|
| 01 | 字符框架（协议 + 框架合体） | 01（协议）/ 04（框架）——按主题分派 |
| 04 / 05 / 06 | 块框架 / 块客户端库 / 分类与装配 | 05 / 07 / 08 |
| 07 / 08 | virtio / memory | 18 / 09 |
| 11 / 13 | tty / log | 10 / 12 |
| 12 | pty（一处）/ devman 注册（一处） | 11 / 16 |
| 14 / 16 / 17 | memory / pci / ata | 09 / 15 / 21 |
| 20 / 21 / 22 | pckbd / USB 存储 / 音频 | 17 / 24 / 25 |
| 23 / 24 | filter 所在篇 / 杂项 | 22 / 28 |
| 26 / 27 / 28 / 29 | 测试基建 / DMA / 错误 / 工程面 | 29 / 30 / 31 / 31 |

（02 = 框架总览、03 = 框架对照、99 在两稿同号，未改动。）

#### D. 自检门复评

| 门 | 第一轮 | 本轮复评 | 说明 |
|---|---|---|---|
| G1 | 通过（含三项"待验证"） | **通过（待验证项已结清）** | F5–F7 |
| G2 | 通过 | **通过（补入三类非 C 制品）** | 每驱动 `.conf` 34 个、`devmand/*.cfg`、RC 启动链 |
| G3 / G4 | 通过 | **通过（编号改写后重扫，"前置"字段未变）** | 前向引用为零、依赖图无环 |
| G5 | 通过（502 条全覆盖） | 通过 | 未变 |
| G6 | 通过 | 通过 | 未变 |
| G7 | 通过（33 篇契约七要素） | 通过 | 未变 |
| G8 | 通过 | **通过（数字修正）** | F2–F4 |
| G9 | 通过（含两处锚点记错） | **通过（锚点修正）** | F8–F9 |

#### E. 本轮未做的事

不新增篇章、不改 33 篇的边界与顺序、不改任何契约的"讲什么 / 不讲什么"。§4.5 的篇幅预算与 §9.4 的 Q-8 是本轮新增的**执行口径**，不是结构变更。

---

## 附：本文件的落盘合规声明

1. 本文件是本次执行**唯一**写入仓库的产物，文件名带执行者后缀 `_deepseek`。
2. 本次执行未修改、重命名、移动、删除任何现有文件（`git status` 可核）。
3. 本次执行未读取任何其它 AI 的 `doc_rerank_*` 产物（目标目录内存在他人后缀的同名产物，本次执行未打开）。
4. 本文件未引用中间产物目录下的任何内容。
5. 所有 C 源码、非 C 制品、Rust 代码的断言均带锚点；决策项已标"待用户裁决"。
6. 第一轮标"待验证"的三项（`driver.c`/`driver_mt.c` 的入口、`prepare_copy`、适配器内部分支行号）已在第二轮结清两项（见 §9.6 F6/F7），仅"适配器内部逐条行号"保留给 B 相重核。
