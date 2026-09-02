# 16-stage-drivers 文档重组计划（plan.md）

> **状态**: 生效中（2026-08-16 首版，深度 review + minix3 源码回归 review 后定稿）
> **范围**: `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`
> **目标**: 以 **驱动子系统语义为主线**重组 drivers 全部文档；CDEV/BDEV/NDEV/RTCDEV/USB_RQ 请求协议为次主线；最终覆盖 Minix3 驱动子系统全部语义（libchardriver + libblockdriver + libnetdriver + libbdev + libvirtio + libusb + 5 个次要框架库 + 57 个 driver server），支撑 `os/drivers/*`（57 crate）+ `os/libs/minix-{chardriver,blockdriver,netdriver,bdev,virtio,usb}` 的彻底 Rust 重写
> **对照**: `01-stage-kernel/`（讲述结构参照）、`02-stage-vm/plan.md` + `14-stage-runtime/plan.md` + `15-stage-fs/plan.md`（plan 结构参照；14/15 为非 server 主线重定义先例）、`minix3/minix/drivers/`（290 个 .c / 155087 行）+ `minix3/minix/lib/lib{chardriver,blockdriver,netdriver,bdev,virtio,usb,audiodriver,i2cdriver,inputdriver,sockdriver,devman}/` + `minix3/minix/include/minix/com.h` + `include/minix/*driver.h` + `driver.h` + `partition.h`（ground truth）、`os/drivers/*`（57 stub crate）+ `os/libs/minix-*`（Rust 实现）

---

## 1. 背景与动机

### 1.1 与 VM 的差异：drivers 不是单个 server，主线重新定义

`02-stage-vm` 以 **VM server 启动顺序为主线**——VM 是一个有明确 `init_vm()` 启动链 + 主循环的用户态服务。**16-stage-drivers 不是单个 server**：它覆盖 `minix3/minix/drivers/` 全量 **57 个 driver server**（290 个 .c，155087 行）+ 11 个共享框架库（每个 driver 在 Minix3 中都是独立进程，均以"SEF 启动 → 服务注册 → `*driver_task(&table)` 主循环"为骨架）：

| 层 | 内容 | C 源码 | 行数 | 性质 |
|----|------|--------|------|------|
| 框架 | libchardriver | `lib/libchardriver/chardriver.c` | 600 | 字符驱动主循环 + CDEV 协议适配（tty/pty/log/random 及 memory 的 char 面使用） |
| 框架 | libblockdriver | `lib/libblockdriver/`（driver.c/driver_mt.c/driver_st.c/drvlib.c/liveupdate.c/mq.c/trace.c） | 1857 | 块驱动主循环（单/多线程）+ BDEV 协议 + 分区/几何 |
| 框架 | libnetdriver | `lib/libnetdriver/`（netdriver.c/portio.c） | 1186 | 网卡驱动主循环 + NDEV 协议 + I/O 端口辅助 |
| 框架 | libbdev | `lib/libbdev/`（bdev.c/call.c/driver.c/ipc.c/minor.c） | 1364 | 块设备客户端库（VFS/FS 消费块驱动的接口） |
| 框架 | libvirtio | `lib/libvirtio/`（virtio.c + virtio_ring.h） | 913 | virtio 队列/协商框架（virtio_blk/virtio_net 共用） |
| 框架 | libusb | `lib/libusb/usb.c` | 255 | USB URB 协议封装（usbd 使用） |
| 框架 | 次要 | `libaudiodriver` 977 / `libi2cdriver` 366 / `libinputdriver` 206 / `libdevman` 576 / `libsockdriver` 1150 | 3275 | 音频/总线/输入/设备注册/socket 框架（sockdriver 与 17-stage-net 分工） |
| server | memory | `drivers/storage/memory/memory.c` | 599 | **boot image 成员**（`kernel/table.c:58`）；char+block 双面（/dev/mem、/dev/kmem、/dev/ram*、/dev/null、/dev/zero、/dev/boot、/dev/imgrd） |
| server | tty | `drivers/tty/tty/`（tty.c 1603 + arch/i386/console.c 1233 + rs232.c 861 + keyboard.c 604 + keymaps） | 5241 | **boot image 成员**（`kernel/table.c:59`）；console + 串口 + 键盘、termios、suspend/select |
| server | pty | `drivers/tty/pty/`（pty.c 860 + tty.c 1320） | 2292 | 伪终端对（master/slave）；ptyfs.c（112 行）语义归 `15-stage-fs/20-ptyfs` |
| server | log | `drivers/system/log/`（log.c 360 + diag.c 54 + liveupdate.c 99） | 513 | /dev/log、内核/服务诊断捕获、RS 运行时加载 |
| server | random | `drivers/system/random/`（main.c 268 + random.c 237 + aes 1036） | 1541 | /dev/random、/dev/urandom（含 AES 实现） |
| server | readclock | `drivers/clock/readclock/`（readclock.c 191 + forward.c 120） | 1095 | RTC（RTCDEV 协议，com.h:1002-1008） |
| server | pci | `drivers/bus/pci/`（pci.c 2559 + main.c 740 + pci_table.c 37） | 3336 | PCI 枚举/配置空间/中断路由（libpci 客户端面） |
| server | gpio | `drivers/system/gpio/gpio.c` | 290 | VTreeFS 导出 GPIO（`gpio.c:287 run_vtreefs`） |
| server | pckbd | `drivers/hid/pckbd/`（pckbd.c 507 + table.c 169） | 676 | 键盘/鼠标 → libinputdriver 事件桥 → 12-stage-input |
| 类别 | storage | `drivers/storage/`（ahci 2734 / at_wini 2319 / mmc 3131 / filter 2554 / floppy 1433 / vnd 603 / fbd 928 / virtio_blk 754 / memory 599；ramdisk 目录 0 .c） | 15055 | 磁盘/过滤/内存盘（ramdisk 为 boot 配置非 C 驱动，见 §5.4） |
| 类别 | net | `drivers/net/`（14 个驱动：dp8390 2182 参考 + virtio_net 446 + 其余 12） | 17425 | 网卡驱动（NDEV 之上；与 17-stage-net lwip/uds 分工） |
| 类别 | usb | `drivers/usb/`（usbd 4818 / usb_storage 2244 / usb_hub 1048） | 8110 | USB 栈：HCD + 设备/存储 |
| 类别 | audio | `drivers/audio/`（7 个声卡：es1371 1942 + 其余 6） | 7134 | 声卡驱动（libaudiodriver 之上） |
| 类别 | bus | `drivers/bus/i2c/` 1397 + `ti1225/` 431 | 1828 | I2C 总线/桥（libi2cdriver 之上） |
| 类别 | video | `drivers/video/fb/` 999 + `tda19988/` 1010 | 2009 | 帧缓冲 + HDMI 桥 |
| 类别 | 杂项 | `power/acpi`（ACPICA 移植 83279）/ `printer` 488 / `eeprom` 505 / `sensors` 1504 / `iommu` 477 / `vmm_guest` 1188 / `examples` 158 | ~88k | 后置/策略决策（ACPI = AML 解释器移植策略，非逐行重写） |

旧内容仅有占位 `README.md`（已移入 `draft/README.md`），其 scope 定义（boot 关键 tty/memory/log + QEMU 常用 + 框架库）保留为素材，本 plan 将其扩展为完整语义覆盖契约。

### 1.2 新主线：驱动子系统语义（框架 → boot 关键 → 设备类别）

与 `01-stage-kernel` 相同，采用**读者学习顺序 = 系统实际执行顺序**的组织原则。驱动子系统没有单一 server 启动链，但有清晰的 **boot 执行顺序 + 语义依赖顺序**：

```
kernel/table.c:44-64 boot image（driver 仅 2 个：memory :58 / tty :59）
  ├─ memory（MEM_PROC_NR）        ← 05：/dev/ram* + /dev/imgrd（boot ramdisk 后端）
  └─ tty（TTY_PROC_NR）           ← 06：console（boot 输出）+ 串口 + 键盘
  │
  ▼ init → /etc/rc → service up（RS 运行时加载）
  ├─ log（诊断/系统日志后端）       ← 08
  ├─ readclock（RTC 时钟同步）      ← 10
  ├─ pci（总线枚举，设备驱动加载前置）← 11
  ├─ ramdisk 根 FS（MFS 读 /dev/imgrd）← 15-stage-fs/07~17 已覆盖
  │
  ▼ 设备类别（devman 动态注册 + 按需）
  ├─ storage：virtio_blk → ahci/at_wini → 其余   ← 14~17
  ├─ hid：pckbd（输入事件 → 12-stage-input）     ← 13
  ├─ net：NDEV 网卡（→ 17-stage-net lwip/uds）    ← 22/23
  ├─ usb/audio/video/杂项（后置）                  ← 18~21/24
```

每个 driver 的执行骨架相同（SEF 启动 → 服务注册/announce → `*driver_task(&table)` 主循环），因此**框架语义前置**：

```
阶段 1  框架（01~04）：chardriver/blockdriver/netdriver 主循环 + CDEV/BDEV/NDEV 协议 + libbdev 客户端
阶段 2  boot 关键（05~08）：memory → tty → pty → log（最小可启动闭环）
阶段 3  系统服务（09~12）：random → readclock → pci → gpio/devman
阶段 4  输入（13）：pckbd（libinputdriver 事件桥）
阶段 5  存储（14~17）：virtio 框架 → virtio_blk → ahci/ata → 存储杂项
阶段 6  USB（18/19）：libusb+usbd 框架 → usb_storage/usb_hub
阶段 7  显示/音频（20/21）：fb → audio 框架+声卡
阶段 8  网络（22/23）：dp8390 参考 + virtio_net → 其余网卡变体（与 17-stage-net 并行）
阶段 9  杂项（24）：printer/eeprom/sensors/iommu/bus/video 桥/vmm_guest/examples（可 defer）
```

**每篇文档必须能回答一个问题：它位于驱动子系统的哪个语义层（框架/boot 关键/系统服务/设备类别），以及在该层中处于哪个执行位置（启动/主循环/请求服务/中断）。** 这是从 `01-stage-kernel/00-kernel-overview.md §3` 继承的组织原则，由 `14-stage-runtime/plan.md §1.1`（runtime 非 server 主线重定义先例）和 `15-stage-fs/plan.md §1.1`（FS 多 server 主线重定义先例）迁移而来。

### 1.3 请求协议次主线（CDEV/BDEV/NDEV/RTCDEV/USB_RQ）

请求协议不充当概念引入的驱动，而是按**驱动类别**展开（每类一篇），协议常量值集中在 99：

```
VFS/devman → 驱动
  ├─ 01 CDEV_RQ_BASE 0x400：OPEN/CLOSE/READ/WRITE/IOCTL/CANCEL/SELECT（com.h:926-932）
  ├─ 02 BDEV_RQ_BASE 0x500：OPEN/CLOSE/READ/WRITE/GATHER/SCATTER/IOCTL（com.h:970-976）
  ├─ 03 NDEV_RQ_BASE 0x1A00：INIT/CONF/SEND/RECV/IOCTL/STATUS_REPLY（com.h:1096-1101）
  ├─ 10 RTCDEV_RQ_BASE 0x1400：GET_TIME/SET_TIME/PWR_OFF（com.h:1002-1008）
  └─ 18 USB_RQ_*（USB_BASE+0~4：INIT/DEINIT/SEND_URB/CANCEL_URB/SEND_INFO，com.h:816-820）
```

## 2. 新文档编号与阶段划分

> 编号规则与 `01-stage-kernel`/`02-stage-vm`/`14-stage-runtime`/`15-stage-fs` 一致：`NN-短横线语义名.md`；`00-` 为总览；`99-` 为全局概念。draft 素材保留原样（仅 `draft/README.md`）。

### 阶段总览（24 篇 + 00 + 99 = 26 篇）

| 阶段 | 编号 | 文档 | 语义模块 | C 源码 | Rust 模块 | 变更 |
|------|------|------|---------|--------|-----------|------|
| 0 总览 | 00 | `00-drivers-overview.md` | 驱动子系统是什么、语义主线图、boot 因果链（boot image 仅 memory+tty）、文档导航 | `drivers/` 全部 + `lib/lib*driver*` 等 | 全部 | 新建 |
| 1 框架·字符 | 01 | `01-chardriver-framework.md` | libchardriver：chardriver_task 主循环、chardriver_process 分发、CDEV_* 协议、cdr_* 回调表（open/close/read/write/ioctl/cancel/select/intr/alarm/other）、suspend/恢复、reply_select、announce | `lib/libchardriver/chardriver.c` + `include/minix/chardriver.h` | `minix-chardriver` | 新建 |
| 1 框架·块 | 02 | `02-blockdriver-framework.md` | libblockdriver：driver.c（单线程）+ driver_mt.c（多线程）+ driver_st.c、BDEV_* 协议、bdr_* 回调表、分区/几何（drvlib.c + partition.h）、live update、trace、mq 队列 | `lib/libblockdriver/` 全部 + `include/minix/blockdriver.h` + `partition.h` | `minix-blockdriver` | 新建 |
| 1 框架·网 | 03 | `03-netdriver-framework.md` | libnetdriver：netdriver_task、NDEV_* 协议、ndo_* 回调表、portio、链路状态/组播/统计 | `lib/libnetdriver/`（netdriver.c/portio.c）+ `include/minix/netdriver.h` | `minix-netdriver` | 新建 |
| 1 框架·客户端 | 04 | `04-bdev-client.md` | libbdev：bdev_open/close/read/write/gather/scatter/ioctl、异步面（bdev_*_asyn + bdev_wait_asyn + 回调）、minor 映射、driver 绑定 | `lib/libbdev/`（bdev.c/call.c/driver.c/ipc.c/minor.c）+ `include/minix/bdev.h` | `minix-bdev` | 新建 |
| 2 boot 关键 | 05 | `05-memory-driver.md` | memory：char+block 双面（memory.c:64,72 m_cdtab/m_bdtab）、设备表（/dev/mem、/dev/kmem、/dev/ram*、/dev/null、/dev/zero、/dev/boot、/dev/imgrd）、vm_map_phys 物理映射、RAM 盘后端 | `drivers/storage/memory/memory.c` | `os/drivers/storage/memory` | 新建 |
| 2 | 06 | `06-tty-driver.md` | tty：boot 终端、tty_table、console（arch/i386/console.c）+ rs232 + 键盘（keyboard.c/keymaps）、termios、行规则、suspend/select、/dev/log 重定向（tty.c:270） | `drivers/tty/tty/` 全部 | `os/drivers/tty/tty` | 新建 |
| 2 | 07 | `07-pty-driver.md` | pty：伪终端对 master/slave、pty 挂起/取消、与 ptyfs 交互（→15-stage-fs/20-ptyfs）；ptyfs.c 语义不在此 | `drivers/tty/pty/pty.c` + `tty.c` | `os/drivers/tty/pty` | 新建 |
| 2 | 08 | `08-log-driver.md` | log：/dev/log 字符设备、诊断捕获（diag.c）、live update、RS 运行时加载 | `drivers/system/log/`（log.c/diag.c/liveupdate.c） | `os/drivers/system/log` | 新建 |
| 3 系统服务 | 09 | `09-random-driver.md` | random：/dev/random + /dev/urandom、熵池、AES 后端（rijndael）、阻塞语义 | `drivers/system/random/`（main.c/random.c/aes/） | `os/drivers/system/random` | 新建 |
| 3 | 10 | `10-readclock-driver.md` | readclock：RTC、RTCDEV_* 协议、CMOS/EFI 时钟、forward | `drivers/clock/readclock/`（readclock.c/forward.c） | `os/drivers/clock/readclock` | 新建 |
| 3 | 11 | `11-pci-driver.md` | pci：PCI 枚举、配置空间读写、pci_table、中断路由、libpci 客户端面 | `drivers/bus/pci/`（main.c/pci.c/pci_table.c） | `os/drivers/bus/pci` | 新建 |
| 3 | 12 | `12-gpio-devman.md` | gpio + libdevman：VTreeFS 导出 GPIO（gpio.c:287）、libdevman 驱动侧注册（generic.c/usb.c）、设备绑定 | `drivers/system/gpio/gpio.c` + `lib/libdevman/` | `os/drivers/system/gpio` + `minix-devman` | 新建 |
| 4 输入 | 13 | `13-pckbd-driver.md` | pckbd：键盘/鼠标、libinputdriver 事件桥（inputdriver_send_event）、按键映射表（table.c）、LED、与 12-stage-input 交互 | `drivers/hid/pckbd/`（pckbd.c/table.c）+ `lib/libinputdriver/inputdriver.c` | `os/drivers/hid/pckbd` | 新建 |
| 5 存储·virtio | 14 | `14-virtio-framework.md` | libvirtio：virtqueue 描述符/可用/使用环、协商、MMIO/PCI 传输、barrier | `lib/libvirtio/`（virtio.c + virtio_ring.h） | `minix-virtio` | 新建 |
| 5 | 15 | `15-virtio-blk-driver.md` | virtio_blk：块设备、virtio 框架首次完整消费、BDEV 面 | `drivers/storage/virtio_blk/virtio_blk.c` | `os/drivers/storage/virtio_blk` | 新建 |
| 5 | 16 | `16-ahci-ata-driver.md` | ahci + at_wini：AHCI HBA 参考（端口/命令表/PRDT）+ ATA PIO/DMA 变体 | `drivers/storage/ahci/` + `at_wini/` | `os/drivers/storage/ahci` + `at_wini` | 新建 |
| 5 | 17 | `17-storage-misc-driver.md` | floppy/mmc/fbd/filter/vnd：其余存储变体差异矩阵 | `drivers/storage/`（floppy/mmc/fbd/filter/vnd） | `os/drivers/storage/*` | 新建 |
| 6 USB | 18 | `18-usb-framework.md` | libusb + usbd：URB 协议、HCD（hcd.c/hcd_common.c/hcd_schedule.c/hcd_ddekit.c/musb）、枚举 | `lib/libusb/usb.c` + `drivers/usb/usbd/` | `minix-usb` + `os/drivers/usb/usbd` | 新建 |
| 6 | 19 | `19-usb-storage-hub.md` | usb_storage（含 scsi.c）+ usb_hub：BOT 传输、SCSI 命令、hub 端口管理 | `drivers/usb/usb_storage/` + `usb_hub/` | `os/drivers/usb/usb_storage` + `usb_hub` | 新建 |
| 7 显示/音频 | 20 | `20-fb-driver.md` | fb：帧缓冲（fb.c/fb_edid.c/fb_arch.c）、mmap 到用户、EDID | `drivers/video/fb/` | `os/drivers/video/fb` | 新建 |
| 7 | 21 | `21-audio-drivers.md` | libaudiodriver + 7 声卡：audio 请求协议、es1370/es1371 参考 + AC97 + 变体差异 | `lib/libaudiodriver/` + `drivers/audio/`（7 个） | `os/drivers/audio/*` | 新建 |
| 8 网络 | 22 | `22-net-driver-reference.md` | dp8390 参考 + virtio_net：NDEV 完整消费（初始化/收发/链路/组播/统计） | `drivers/net/dp8390/` + `virtio_net/` | `os/drivers/net/dp8390` + `virtio_net` | 新建 |
| 8 | 23 | `23-net-driver-variants.md` | 其余 12 网卡：e1000/rtl8139/rtl8169/fxp/3c90x/atl2/lance/dec21140A/ip1000/lan8710a/vt6105/dpeth 变体差异矩阵 | `drivers/net/`（其余 12 目录） | `os/drivers/net/*` | 新建 |
| 9 杂项 | 24 | `24-misc-drivers.md` | printer/eeprom(cat24c256)/sensors(bmp085/sht21/tsl2550)/iommu(amddev)/bus(i2c/ti1225)/video(tda19988)/vmm_guest(vbox)/examples(hello)/power(tps65217/tps65950/acpi) 差异矩阵 + ACPI/AML 策略 | 上述目录 | `os/drivers/*` | 新建 |
| 99 全局 | 99 | `99-global-concepts.md` | 请求常量全集（CDEV/BDEV/NDEV/RTCDEV/USB_RQ/SDEV）、driver 通用模型（driver.h）、minor 布局、/dev 命名约定、endpoint 约定、errno 映射 | `com.h` + `driver.h` + 各 `*driver.h` | `minix-types` | 新建 |

### 2.1 阶段间的叙事衔接

- `01` → `05/06/08`：chardriver 框架 → memory/tty/log 首次消费（boot 路径闭环）
- `02` → `05`：blockdriver 框架 → memory 块面（/dev/ram*、/dev/imgrd）
- `02` → `04`：块协议驱动侧 ↔ 客户端侧（libbdev 由 05-stage-vfs/15-stage-fs 消费）
- `14` → `15`：virtio 框架 → virtio_blk 首次完整消费
- `03` → `22/23`：netdriver 框架 → dp8390 参考 → 变体
- `22/23` → `17-stage-net`：NDEV 驱动面完整后，lwip/uds 消费之（跨 stage）
- `13` → `12-stage-input`：pckbd 事件桥 → input server（跨 stage）
- `18` → `19`：USB 框架 → usb_storage/usb_hub 消费

## 3. 讲述结构规范（参照 01-stage-kernel / 15-stage-fs plan §3）

### 3.1 每篇文档的章节模板

与 `01-stage-kernel`/`02-stage-vm`/`14-stage-runtime`/`15-stage-fs` 一致：

1. **概念**——为什么需要这个机制、类比、边界声明（前置依赖/本篇不覆盖什么）
2. **C 源码分析**——ground truth，必须 grep 实证，禁止凭记忆
3. **Rust 设计决策**——类型系统建模、与 C 的对应、`[ARCH]` 标注
4. **错误处理**——errno 映射（P0：错误类型必须映射 Minix3 errno 值；含协议级错误码）
5. **测试**——该文档语义模块的 Rust 单测清单与统计
6. **过渡**——本阶段在驱动子系统语义层中的位置 + 下一阶段入口
7. **参见**——绝对路径引用（doc/code/C 源），绝不引用 `.design/`/`tmp_design_and_todo/`

### 3.2 引用规则

- 各文档之间用新编号交叉引用（如 `13-pckbd-driver.md` §事件桥）
- 与 kernel/VM/VFS/FS/input/devman 文档交叉引用时用 `../01-stage-kernel/NN-*.md`、`../02-stage-vm/NN-*.md`、`../05-stage-vfs/NN-*.md`、`../11-stage-devman/NN-*.md`、`../12-stage-input/NN-*.md`、`../15-stage-fs/NN-*.md`、`../17-stage-net/NN-*.md`
- 对 draft 素材的引用一律指向 `draft/README.md`，并标注"素材"
- 跨 stage 框架（libinputdriver/libdevman/libvtreefs）：框架语义在本 stage 声明，引用方指向之（如 `12-gpio-devman.md` 声明 libdevman，`../11-stage-devman/` 引用）

### 3.3 每篇文档的边界声明

每篇必须含"前置依赖 / 本篇不覆盖什么"声明，写作时禁止内容交叉。关键边界：

| 文档 | 前置依赖 | 职责 | 不覆盖（移交） |
|------|---------|------|---------------|
| 00 | 无 | 驱动子系统全景、主线图、导航 | 一切机制细节（01~24） |
| 01 | 00、`../05-stage-vfs/11-fs-comm.md`（VFS 侧 CDEV 消费） | 字符驱动主循环/CDEV 协议/回调表 | 具体驱动实现（05~08/13）、协议常量值（99） |
| 02 | 00 | 块驱动主循环/BDEV 协议/分区 | 具体存储驱动（05/15~17）、libbdev 客户端（04） |
| 03 | 00 | 网卡主循环/NDEV 协议/portio | 具体网卡实现（22/23）、lwip/uds（17-stage-net） |
| 04 | 02 | libbdev 客户端 API/异步面 | VFS/FS 消费逻辑（05-stage-vfs/15-stage-fs）、驱动侧（02） |
| 05 | 01/02 | memory 双面驱动、RAM 盘语义 | VM 物理映射机制（02-stage-vm）、root FS 挂载（15-stage-fs） |
| 06 | 01 | console/串口/键盘、termios、行规则 | 键盘事件协议（12-stage-input）、PTY（07） |
| 07 | 01 | 伪终端对语义 | ptyfs（15-stage-fs/20）、VFS 侧 /dev/pts |
| 08 | 01 | /dev/log、诊断捕获 | syslog 命令（18-stage-commands）、内核诊断输出（01-stage-kernel） |
| 09 | 01 | 熵源/RNG 语义 | 内核随机性（01-stage-kernel）、密码学库选型 |
| 10 | 01 | RTC/RTCDEV 协议 | 时钟服务（01-stage-kernel/15-clock-timer） |
| 11 | 00 | PCI 枚举/配置空间 | 各设备驱动如何用 PCI（14/16/22/23）、I2C（24） |
| 12 | 00 | GPIO 导出、libdevman 注册 | devman server（11-stage-devman）、VTreeFS 框架（15-stage-fs/18） |
| 13 | 01 | 键盘/鼠标事件桥 | input server 事件消费（12-stage-input）、TTY 键盘读取（06） |
| 14 | 00 | virtqueue/协商/传输 | virtio_blk/virtio_net 设备语义（15/22） |
| 15 | 14/02 | virtio_blk 块设备 | virtio 框架（14）、其他存储（16/17） |
| 16 | 02 | AHCI/ATA 语义 | virtio 存储（15）、存储杂项（17） |
| 17 | 02 | floppy/mmc/fbd/filter/vnd 差异 | AHCI/ATA（16）、virtio（15） |
| 18 | 00 | URB/HCD/枚举 | usb_storage/usb_hub（19）、libusb 客户端 |
| 19 | 18 | BOT/SCSI/hub | USB 框架（18）、存储语义（15~17） |
| 20 | 00 | 帧缓冲/mmap | 控制台渲染（06）、VM mmap 机制（02-stage-vm） |
| 21 | 00 | audio 协议/声卡差异 | 声音系统上层（18-stage-commands） |
| 22 | 03/14 | dp8390 参考 + virtio_net | 网卡变体（23）、lwip（17-stage-net） |
| 23 | 22 | 其余 12 网卡差异矩阵 | dp8390/virtio_net（22） |
| 24 | 00 | 杂项驱动差异矩阵 + ACPI 策略 | 各类别核心语义（05~23） |
| 99 | 无 | 常量值全集/结构定义 | 常量如何被使用（各文档） |

### 3.4 测试基线（截至 2026-08-16）

- `os/drivers/*` 57 个 crate 为 stub；`os/libs/minix-{chardriver,blockdriver,netdriver,bdev,virtio,usb}` 为空/最小；`cargo test` 无实质测试
- 每篇改写完成时在文末更新该模块测试统计（review-doc-skill §2.4j）

### 3.5 Review gate 要求（每篇改写必检）

- 每篇新文档创建时同步生成 `.design/{NN}-outline.md` + `{NN}-outline-review.md` + `{NN}-design.md`（Step 0.3 嵌入生成，Gate H.6，不允许 N/A）
- 每篇改写后按 review 工作流跑 Blocker Gates（0/A/B/C/D/D-6/E/G/H），scan 产物写入 `.review/codex/drivers/{NN}-{name}/`
- P0 未清不得标完成；doc 与 code 保持同步

### 3.6 变体文档写作原则（参考实现 + 差异展开）

net（14 个同构网卡）、storage（ahci/at_wini/floppy/mmc/fbd/filter/vnd）、audio（7 个声卡）、usb（storage/hub）存在大量同构变体。**参考语义只写一次**，每篇变体文档按以下结构写作：

1. **启动/初始化差异**——SEF init 内容、硬件探测/注册差异（如 PCI 厂商/设备 ID 表）
2. **框架回调差异矩阵**——该驱动实现/未实现的回调（如 cdr_*/bdr_*/ndo_*），未实现回调 → 默认行为
3. **特有硬件协议**——寄存器集、MMIO 基址、DMA/PRDT 布局（如 ahci 端口、mmc 命令集、AC97 编解码）
4. **特有交互**——如 filter 的 checksum/sum 语义、vnd 的回环、tda19988 HDMI 桥
5. **C 文件 API 面映射表**——该 driver 全部 .c 文件逐一对齐到文档小节（覆盖契约，§5.1 核对列）

此原则保证：框架/参考语义只写一次，变体只写差异，总文档数可控（26 篇），且每个 driver 的 C 文件全部有归属（§5.1 无遗漏）。

---

## 4. 架构演进（ARCH）清单

> 按 review-core 要求，ARCH 必须三处一致标注：Minix3 行为对照点 + design doc + 代码注释。**drivers crate 当前为 stub，以下为设计期候选 ARCH 项**，写文档时必须逐项确认/更新状态；minix-rs 侧已实现的 ARCH（如 02-stage-vm 的 Direct Map）不属于本 stage。

| # | ARCH 项 | Minix3 现状 | minix-rs 演进（候选） | 涉及新文档 | 状态 |
|---|---------|------------|---------------------|-----------|------|
| A-1 | 驱动回调表 | `struct chardriver`/`struct blockdriver` 函数指针表 | Rust trait（`CharDriver`/`BlockDriver`/`NetDriver`）+ 枚举分发器，类型化回调 | 01/02/03 | 设计期（stub，待确认） |
| A-2 | I/O 端口/MMIO | `inb/outb` + 直接端口访问（arch/i386） | 硬件抽象 trait（arch_mapping.md），OS 层不暴露端口；MMIO 统一 `PhysMap` | 01/06/11/14 | 设计期 |
| A-3 | 中断 | `cdr_intr` 钩子 + notify（chardriver.c） | 中断回调 trait（内核 notify → 事件循环唤醒），no_std | 01/06/18 | 设计期 |
| A-4 | DMA | `DMA_BUF_SIZE` 静态缓冲 + phys 地址（blockdriver.h:60） | DMA 抽象（physmap + 一致性缓冲），与 02-stage-vm 物理映射衔接 | 02/14/16/18 | 设计期 |
| A-5 | 单线程执行模型 | chardriver 单线程；blockdriver 可选 MT（driver_mt.c） | Rust 统一单线程事件循环（`!Send`/`!Sync`/`Rc`/`RefCell` 合理）；MT 语义不复刻 | 02/00 | 设计期 |
| A-6 | 物理内存映射（memory 驱动） | `vm_map_phys`（memory.c:141） | 依赖 `02-stage-vm` DirectMap/物理映射语义（跨 stage） | 05 | 设计期 |
| A-7 | 块设备协议异步面 | `bdev_*_asyn` + `bdev_wait_asyn` + 回调（bdev.h） | 事件循环风格异步（minix-bdev），阻塞 IO 语义保留 | 04 | 设计期 |
| A-8 | virtio 队列 | `virtio_ring` 结构 + barrier（virtio.c 913 行） | unsafe 降级 + 类型安全队列描述符；MMIO/PCI 传输抽象 | 14 | 设计期 |
| A-9 | ACPI/AML | ACPICA 移植（`drivers/power/acpi` 83279 行） | **策略决策**：移植/裁剪 ACPICA vs Rust AML 库；非逐行重写 | 24/00 | 设计期（重大决策） |
| A-10 | 熵源 | random + AES（rijndael） | 密码学 Rust 库（RNG 后端）；no_std 约束 | 09 | 设计期 |
| A-11 | Live Update | `blockdriver_liveupdate`（liveupdate.c 94 行）+ log liveupdate | 与 02-stage-vm A-8 同步 fail-closed | 02/08 | 设计期 |
| A-12 | 键盘映射 | `keymaps/genmap.c` 生成表 | 静态表生成（build 时），console 渲染层 | 06/13 | 设计期 |
| A-13 | 帧缓冲 | fb + mmap 帧缓冲（fb.c） | 帧缓冲 mmap 与 02-stage-vm mmap 语义衔接 | 20 | 设计期 |

---

## 5. 覆盖完整性核对（对照 minix3 源码）

### 5.1 C 源文件 → 新文档映射（.c 全量，290 个文件 155087 行）

**框架库（`minix3/minix/lib/`，文件级）**

| C 文件 | 行数 | 新文档 | 核对 |
|--------|------|--------|------|
| `libchardriver/chardriver.c` | 600 | 01 | 已核对 |
| `libblockdriver/driver.c` | 462 | 02 | 已核对 |
| `libblockdriver/driver_mt.c` | 581 | 02 | 已核对 |
| `libblockdriver/driver_st.c` | 94 | 02 | 已核对 |
| `libblockdriver/drvlib.c` | 234 | 02 | 已核对 |
| `libblockdriver/liveupdate.c` | 94 | 02 | 已核对 |
| `libblockdriver/mq.c` | 108 | 02 | 已核对 |
| `libblockdriver/trace.c` | 284 | 02 | 已核对 |
| `libnetdriver/netdriver.c` | 993 | 03 | 已核对 |
| `libnetdriver/portio.c` | 193 | 03 | 已核对 |
| `libbdev/bdev.c` | 642 | 04 | 已核对 |
| `libbdev/call.c` | 118 | 04 | 已核对 |
| `libbdev/driver.c` | 122 | 04 | 已核对 |
| `libbdev/ipc.c` | 346 | 04 | 已核对 |
| `libbdev/minor.c` | 136 | 04 | 已核对 |
| `libvirtio/virtio.c` | 913 | 14 | 已核对 |
| `libusb/usb.c` | 255 | 18 | 已核对 |
| `libaudiodriver/audio_fw.c` + `liveupdate.c` | 977 | 21 | 已核对 |
| `libi2cdriver/i2cdriver.c` | 366 | 24 | 已核对 |
| `libinputdriver/inputdriver.c` | 206 | 13 | 已核对 |
| `libdevman/generic.c` + `usb.c` | 576 | 12 | 已核对 |
| `libsockdriver/sockdriver.c` | 1150 | 17-stage-net | 已核对（排除，见 §5.4） |

**driver server（`minix3/minix/drivers/`，目录级：57 目录全覆盖，逐文件 API 面映射在每篇文档 §3.6.5）**

| 目录 | .c 行数 | 新文档 | 核对 |
|------|---------|--------|------|
| `storage/memory` | 599 | 05 | 已核对 |
| `tty/tty` | 5241 | 06 | 已核对 |
| `tty/pty` | 2292 | 07（ptyfs.c 112 行 → 15-20） | 已核对 |
| `system/log` | 513 | 08 | 已核对 |
| `system/random` | 1541 | 09 | 已核对 |
| `clock/readclock` | 1095 | 10 | 已核对 |
| `bus/pci` | 3336 | 11 | 已核对 |
| `system/gpio` | 290 | 12 | 已核对 |
| `hid/pckbd` | 676 | 13 | 已核对 |
| `storage/virtio_blk` | 754 | 15 | 已核对 |
| `storage/ahci` | 2734 | 16 | 已核对 |
| `storage/at_wini` | 2319 | 16 | 已核对 |
| `storage/floppy` | 1433 | 17 | 已核对 |
| `storage/mmc` | 3131 | 17 | 已核对 |
| `storage/fbd` | 928 | 17 | 已核对 |
| `storage/filter` | 2554 | 17 | 已核对 |
| `storage/vnd` | 603 | 17 | 已核对 |
| `storage/ramdisk` | 0 | 排除（§5.4） | 已核对 |
| `usb/usbd` | 4818 | 18 | 已核对 |
| `usb/usb_storage` | 2244 | 19 | 已核对 |
| `usb/usb_hub` | 1048 | 19 | 已核对 |
| `video/fb` | 999 | 20 | 已核对 |
| `audio/als4000` | 911 | 21 | 已核对 |
| `audio/cmi8738` | 867 | 21 | 已核对 |
| `audio/cs4281` | 942 | 21 | 已核对 |
| `audio/es1370` | 894 | 21 | 已核对 |
| `audio/es1371` | 1942 | 21 | 已核对 |
| `audio/sb16` | 703 | 21 | 已核对 |
| `audio/trident` | 875 | 21 | 已核对 |
| `net/dp8390` | 2182 | 22 | 已核对 |
| `net/virtio_net` | 446 | 22 | 已核对 |
| `net/3c90x` | 1155 | 23 | 已核对 |
| `net/atl2` | 925 | 23 | 已核对 |
| `net/dec21140A` | 509 | 23 | 已核对 |
| `net/dpeth` | 2578 | 23 | 已核对 |
| `net/e1000` | 918 | 23 | 已核对 |
| `net/fxp` | 1982 | 23 | 已核对 |
| `net/ip1000` | 989 | 23 | 已核对 |
| `net/lan8710a` | 955 | 23 | 已核对 |
| `net/lance` | 895 | 23 | 已核对 |
| `net/rtl8139` | 1626 | 23 | 已核对 |
| `net/rtl8169` | 1468 | 23 | 已核对 |
| `net/vt6105` | 797 | 23 | 已核对 |
| `bus/i2c` | 1397 | 24 | 已核对 |
| `bus/ti1225` | 431 | 24 | 已核对 |
| `eeprom/cat24c256` | 505 | 24 | 已核对 |
| `examples/hello` | 158 | 24 | 已核对 |
| `iommu/amddev` | 477 | 24 | 已核对 |
| `power/acpi` | 83279 | 24 | 已核对 |
| `power/tps65217` | 402 | 24 | 已核对 |
| `power/tps65950` | 541 | 24 | 已核对 |
| `printer/printer` | 488 | 24 | 已核对 |
| `sensors/bmp085` | 583 | 24 | 已核对 |
| `sensors/sht21` | 486 | 24 | 已核对 |
| `sensors/tsl2550` | 435 | 24 | 已核对 |
| `video/tda19988` | 1010 | 24 | 已核对 |
| `vmm_guest/vbox` | 1188 | 24 | 已核对 |

### 5.2 头文件覆盖

| 头文件 | 归属 | 核对 |
|--------|------|------|
| `include/minix/chardriver.h`（struct chardriver/chardriver_task） | 01/99 | 已核对 |
| `include/minix/blockdriver.h`（struct blockdriver/blockdriver_task/分区类型） | 02/99 | 已核对 |
| `include/minix/netdriver.h`（struct netdriver/netdriver_addr_t） | 03/99 | 已核对 |
| `include/minix/bdev.h`（bdev_* 客户端 API + 异步面） | 04/99 | 已核对 |
| `include/minix/driver.h`（driver_receive/struct device/MAX_NR_OPEN_DEVICES） | 01/02/99 | 已核对 |
| `include/minix/partition.h`（分区表解析） | 02 | 已核对 |
| `include/minix/inputdriver.h`（inputdriver 事件 API） | 13 | 已核对 |
| `include/minix/virtio.h`/`libvirtio/virtio_ring.h`（virtqueue 结构） | 14 | 已核对 |
| `include/minix/usb.h`（USB_* 常量/URB 结构） | 18/99 | 已核对 |
| `include/minix/devman.h`（libdevman 注册 API） | 12 | 已核对 |
| `include/minix/audiodriver.h`（audio 回调表） | 21 | 已核对 |
| `include/minix/i2cdriver.h`（i2c 回调表） | 24 | 已核对 |
| `include/minix/sockdriver.h`（socket 驱动回调表） | 17-stage-net | 已核对（排除） |
| `include/minix/com.h`（CDEV/BDEV/NDEV/RTCDEV/USB_RQ/SDEV 请求常量） | 99 | 已核对 |

### 5.3 协议与消息布局

- CDEV 请求协议全集：`CDEV_RQ_BASE 0x400`，`CDEV_OPEN(0)`~`CDEV_SELECT(6)`（com.h:919-932），`IS_CDEV_RQ` 判定 → 01/99
- BDEV 请求协议全集：`BDEV_RQ_BASE 0x500`，`BDEV_OPEN(0)`~`BDEV_IOCTL(6)`（com.h:963-976），`IS_BDEV_RQ` 判定；BDEV_R_BIT/W_BIT、BDEV_FORCEWRITE 标志 → 02/99
- NDEV 请求协议全集：`NDEV_RQ_BASE 0x1A00`，`NDEV_INIT(0)`~`NDEV_STATUS_REPLY(5)`（com.h:1085-1101）→ 03/99
- RTCDEV 请求协议：`RTCDEV_RQ_BASE 0x1400`，`GET_TIME/SET_TIME/PWR_OFF`（com.h:995-1008）→ 10/99
- USB 请求协议：`USB_RQ_INIT(0)`~`USB_RQ_SEND_INFO(4)`（com.h:816-820）→ 18/99
- SDEV 请求协议（sockdriver）：`SDEV_RQ_BASE 0x1900`（com.h:1037-1068）→ 17-stage-net（本 stage 排除）
- 消息布局（`m_device`/`m_dev_ioctl` 等 ipc.h 结构体）→ 01/02/99

### 5.4 排除表（非本 stage 语义）

| 内容 | 归属 | 说明 |
|------|------|------|
| 块设备消费方（VFS/FS 的 bdev 用法） | 05-stage-vfs / 15-stage-fs | 本 stage 只提供 libbdev 客户端库（04）与驱动侧（02） |
| ptyfs（`drivers/tty/pty/ptyfs.c`） | 15-stage-fs/20-ptyfs | PTY 字符驱动（07）与 ptyfs 树语义分开 |
| input server（`servers/input/`） | 12-stage-input | 本 stage 只提供 libinputdriver 事件桥（13） |
| devman server（`servers/devman/`） | 11-stage-devman | 本 stage 覆盖驱动侧 libdevman（12） |
| lwip/uds server（`minix/net/`） | 17-stage-net | 本 stage 覆盖 NDEV 驱动面（22/23）；libsockdriver 随 17 |
| `storage/ramdisk/` 目录（proto/rc 等） | boot 配置（非 C 驱动） | 无 .c；语义映射到 memory 驱动的 /dev/imgrd（05）+ boot 布局（18-stage-commands） |
| 内核中断/时钟/系统调用 | 01-stage-kernel | 驱动只消费 notify/中断通知 |
| VM 物理映射机制 | 02-stage-vm | memory 驱动（05）只消费 vm_map_phys 接口 |
| 命令面（`service`/`devmand`/MAKEDEV） | 18-stage-commands | 本 stage 覆盖驱动进程本体 |

---

## 6. 实施顺序（自下而上，每批可独立验收）

1. **框架批**：01 → 02 → 03 → 04（对应 `minix-chardriver`/`minix-blockdriver`/`minix-netdriver`/`minix-bdev` crate 骨架）
2. **boot 关键批**：05（memory）→ 06（tty）→ 07（pty）→ 08（log）——最小可启动闭环
3. **系统服务批**：09（random）→ 10（readclock）→ 11（pci）→ 12（gpio/devman）
4. **输入批**：13（pckbd，依赖 12-stage-input 的 input server）
5. **存储批**：14（virtio 框架）→ 15（virtio_blk）→ 16（ahci/ata）→ 17（存储杂项）
6. **USB 批**：18 → 19（可 defer 至 QEMU 场景）
7. **显示/音频批**：20（fb）→ 21（audio，可 defer）
8. **网络批**：22 → 23（与 17-stage-net 并行；非 boot 关键可 defer）
9. **杂项批**：24（printer/sensors/iommu/bus/vmm_guest/examples/ACPI 策略，可 defer）
10. **收尾**：00 总览定稿（吸收各批成果）+ 99 全局概念 + checklist 更新

---

## 7. Review 记录

### 7.1 深度 review（语义全覆盖 + 模块拆分合理性）

> 首轮深度 review 结论（2026-08-16）：
> - **结构修正 R1**：net 从"单篇"拆分为 `22-net-driver-reference`（dp8390 参考 + virtio_net）+ `23-net-driver-variants`（其余 12 个差异矩阵）——17425 行单篇不可实施；storage 同理拆 ahci/ata（16）与杂项（17），audio 合并为单篇（libaudiodriver + 7 声卡同构，参考+差异矩阵写法）。
> - **结构修正 R2**：`04-bdev-client` 独立成篇（libbdev 是 VFS/FS 消费块协议的客户端面，与驱动侧 02 双向构成块语义闭环），而非并入 02。
> - **事实修正 R3**：boot image 中 driver 仅 memory + tty（`kernel/table.c:44-64` 实证）；log/random/readclock/pci 等均为 RS 运行时加载（README 原素材"boot 关键 tty/memory/log"中 log 非 boot image 成员，改按"boot 路径关键"表述）。
> - **事实修正 R4**：`storage/ramdisk` 目录无任何 .c（proto/rc 构建产物），非 C 驱动，排除并映射到 memory 的 /dev/imgrd。
> - P0/P1/P2 全部闭环后本 plan 方可进入实施。

### 7.2 minix3 源码回归 review

> 逐文件 grep/`wc -l` 实证（2026-08-16），见 §5 核对列；发现遗漏/错配时在此记录并回填 §5/§2。

**首轮回归 review 事实修正（已回填 §5/§2）：**

| # | 修正项 | 原值 | 实证值（grep/wc） |
|---|--------|------|------------------|
| F1 | C 源总数 | — | 290 个 .c / 155087 行（`find minix3/minix/drivers -name '*.c' \| xargs wc -l`） |
| F2 | driver 目录数 | 57（README 素材） | 57（`find minix3/minix/drivers -mindepth 2 -maxdepth 2 -type d`） |
| F3 | boot image driver | "tty/memory/log"（README 素材） | 仅 memory + tty（`kernel/table.c:44-64`）；log 为 RS 运行时加载 |
| F4 | pty 行数 | ~2180 | 2292（pty.c 860 + tty.c 1320 + ptyfs.c 112） |
| F5 | usbd 行数 | — | 4818（含 hcd 子目录） |
| F6 | power/acpi 行数 | — | 83279（ACPICA 移植，3.7MB） |
| F7 | tty 行数 | ~4300 | 5241（tty.c 1603 + console.c 1233 + rs232.c 861 + keyboard.c 604 + 其余） |
| F8 | storage/ramdisk | QEMU 常用（README 素材） | 无 .c（proto/rc 构建配置）→ 排除表 §5.4 |
| F9 | libbdev 行数 | — | 1364（bdev.c 642 + call.c 118 + driver.c 122 + ipc.c 346 + minor.c 136） |

**自动化覆盖检查**：57 个 driver 目录（155087 行）+ 24 个框架 .c 文件（9450 行）全量映射，0 遗漏（脚本比对 §5.1 显式清单；行数逐项核对全部吻合）。

### 7.3 自检清单

- [ ] 57 个 driver 目录 + 11 个框架库全部映射（§5.1 无遗漏）
- [ ] CDEV/BDEV/NDEV/RTCDEV/USB_RQ 协议常量全部覆盖（§5.3 → 01/02/03/10/18/99）
- [ ] 每个语义模块恰好一篇，无内容交叉（§3.3 边界表）
- [ ] ARCH 项全部标注三处一致（§4）
- [ ] 排除表明确（§5.4），无越界
- [ ] 文档数可实施（26 篇），每篇一个语义单元
