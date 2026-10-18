# 并发任务板 B — 二线·用户态运行时、库与驱动框架（`os/libs/` 逐 crate、`os/drivers/`）

> **这份文件是什么**：`2026-10-18` 把全仓库开放项按**领地互斥**切成三份（A/B/C）中的一份，交给一个独立 agent 并发推进。切分依据是 `coordination/CONCURRENT-WORK-PROTOCOL.md` §2 的四线所有权清单——三份文件的可写路径两两不相交，这是并发不打架的唯一硬保证。
> **本板的领地**：`os/libs/` 的逐个 crate（**`minix-boot` 除外**，它归 C 板）、`os/drivers/`、`rewrite-notes/14-stage-runtime/`、`rewrite-notes/16-stage-drivers/`、本文件。
> **配套件**：`PARALLEL-20261018-A-CMD.md`（三线 服务器/文件系统/网络/命令面）、`PARALLEL-20261018-C-KERN-QEMU.md`（一线 内核/引导，**独占真机窗**）。
> **本板领地的关键性**：协议 §2 写明二线是另外两线的**依赖下游汇聚点**。也就是说本板一次改动会同时改变 A 与 C 的门读数——所以本板的每一条都要「先量下游、再动上游」，且**改公开签名前必须先在 C 板的跨界登记行挂账**。
> **条目状态词**（五值）：`☐` / `🔄`（已领取，别人别碰）/ `⏸`（注明等谁）/ `✅`（带日期与提交号）/ `🚫`（写明为什么现在不做）。

---

## 0. 你（接手的 agent）开工前的四件事

1. **owner 名固定 `zcode-ds`**，领取一律 `bash tools/claim.sh claim <条目ID> zcode-ds`；别手搓 worktree，别在主树 `checkout`/`reset`。**worktree 路径是分支名去掉 `claim/` 后整段小写**（`tools/claim.sh` 的 `wt_path`），以 claim.sh 打印的路径为准。
2. **先复核锚点再领**：本板行号是切板时刻的快照。领之前跑 `grep -n "<条目ID>" rewrite-notes/16-stage-drivers/todo.md`（14 篇同理）。「账面滞后」在这个仓库是常态——`14-stage-runtime/todo.md:23` 已把 `V1-P1-1` 标 ✅，而 `coordination/TODO-LEDGER-OPEN.md:124` 仍记「真机 2」。**遇到这种不一致，先做对账再动码，并把对账读数留在账里。**
3. **一次一条**；改 `os/libs/` 任一 crate 的公开面之前，先跑下游全包门（第 3 节）拿基线。
4. **门读数必须 L1**：命令 + 输出成对，推断不算过。

**组合行的领取办法**：一行出现多个条目 ID 时一次只领一个、开一个分支；没有权威 ID 的行用括号里的追踪键（B6 的 `AT-WINI`）。不要把整行当成一个 claim 键。

## 1. 二线主任务表（按 P0→P1→P2 排，同一轮只做一行）

| # | 条目 ID | 登记处 | 状态 | 这一轮做到哪算完 | 判据与门 |
|---|---|---|---|---|---|
| B1 | `G6` 驱动服务二进制层整体为零 | `16-stage-drivers/todo.md:68`、`:12`；`TODO-LEDGER-OPEN.md:43` | `☐` **非单轮量级** | **按驱动族分批**，每轮一族；每族落地标准：`main.rs` 从占位骨架变成真事件循环 + 至少一枚可跑的宿主门。**已落二十八台（十二族）**：`log`、`readclock`、`random`、`fb`、`pty`、`lance`（netdriver 族首个 + 10 枚泵门）、网络族余四台（`e1000`/`rtl8139`/`dp8390`/`virtio_net`，泵上收为 `minix-netdriver::pump`）；存储族三台（`ahci`/`at_wini`/`virtio_blk`，泵上收为 `minix-blockdriver::pump`）；`printer`（并行口缝 + char 泵）；`pci`（配置空间枚举 + BUSC 查询泵）；`gpio`（vtreefs 文件服务 + 读钩子，补了 `minix-vtreefs::lookup_child` 框架缺口）；`bmp085`（i2c 客户端首用）；`sht21`（CRC-8 测样 + 每秒采样缓存，补 `now_seconds`）；`tsl2550`（ADC 轮询 + 查找表）；**`i2c`（本笔提交，i2c 族总线本体）**：新增 `controller.rs`（SoC `process` 缝 + 诚实空控制器）与 `service.rs`——按 C 建模 label 键控的设备预留（同键重连即刷新 endpoint、异键 EBUSY、VFS 可触未预留设备）、280 字节执行块三态校验、授权拷贝进出（控制器失败不回拷）、typed `BUSC_I2C_RESERVE`/`EXEC` 与编码 `MINIX_I2C_IOCTL_EXEC` 的 ioctl 腿（常量由 ioccom 算式在测试里重导）；`DriverTransport` 再补带默认动词 `label_name`（`KernelTransport` 走 DS 逆向查名）。6 枚新门（卡 6/0）；**`ti1225`（本笔提交，总线族第二台）**：`registers.rs`（PCI 属性偏移 + CardBus CSR 块，`#[repr(C)]` 镜像钉偏移）、`socket.rs`（电源协商分支序：卡检→非卡→已上电→5/3/X/Y，控制字段替换照 C）、`controller.rs`（配置空间 + 映射 CSR + 重扫 + 延时缝）、`service.rs`（C 的 init 序列：探针、清 `INTEXCA`、置三枚掩码、一次插槽过场含事件清、上电等待按读次数上界、CRST 脉冲与次级总线重扫；C 的循环只打印消息，泵按原样惰性），主循环走新增的 `DriverRuntime::serve_quiet`（C 无 announce），11 枚新门（卡 11/0），顺手清 runtime.rs 三枚既有测试告警；**`tps65217`（本笔提交，i2c 消费族第四台）**：`registers.rs`（26 个寄存器偏移 + 芯片/中断/掉电位掩码）、`device.rs`（四芯片掩码识别 + 电源键判定）、`service.rs`（C 的 init：探针→留总线→reserve→芯片门→写 `INT_REG` 默认掩码并回读清挂起→写 `STATUS[7]` 开掉电；通知泵：DS 刷新总线、HARDWARE 读中断，电源键经新增传输动词 `signal` 通知 init，其余消息照 C 丢弃）。6 枚新门（卡 6/0）；**`tps65950`（本笔提交）**：RTCDEV 服务器——`registers.rs`/`rtc.rs`（BCD 与六字段读写、冷启回写默认日期）/`device.rs`（IDCODE 解锁与三版本门）/`service.rs`（四子模块预留、`RTCDEV_GET_TIME_G` 任意调用方经授权拷贝取时、`SET_TIME_G` 按 `getnuid` 超用户门（EPERM）、`PWR_OFF` 答 ENOSYS、其余 EINVAL，回复 `RTCDEV_REPLY` 带状态），11 枚新门（卡 11/0）；**`amddev`（本笔提交）**：`registers.rs`（DEV 函数号/能力位/表几何 + PCI 能力表常量）、`device.rs`（能力字解码、map 值、对齐与清位算术）、`controller.rs`（能力表走查/寄存器/连续表分配/翻译/清位缝）、`service.rs`（init：探针→读能力与错误寄存器→域 0 设表基址→map 0→控制字 `0x1D`；泵：每消息先报清异常，`IOMMU_MAP` 走对齐门→翻译→清位→按 C 以结果作回复类型，其余消息丢弃），9 枚新门（卡 9/0），第二台走 `serve_quiet`（C 无 announce）；**`tda19988`（本笔提交，视频族）**：`registers.rs`（CEC 寄存器 + 七个 HDMI 页与全部控制/EDID/OTP 位）、`hdmi.rs`（页缓存访问器 + 版本门 + DDC 上电 + EDID 触发与轮询）、`service.rs`（**自备块泵**：C 的 transfer 自己取 EDID 又自己拷调用方授权，共享 `BlockDriver` face 无传输句柄，故服务自持 transport 并复用 `minix-blockdriver` 的 `classify` 重启门/字符开拒绝/线格式；读腿=显示器门→hdmi_init→EDID→授权拷贝→按 C 回原始请求长度，写腿 EACCES，ioctl ENOTTY），7 枚新门（卡 7/0）。DS 订阅/状态存续与 RS 标签 `i2c.1` 登记仍留缝；引导表登记与 freestanding 双入口延后归 A/C；**`vbox`（本笔提交，vmm_guest 族）**：VirtualBox 客户机——`vmmdev.rs`（设备协议常量 + 请求页 `#[repr(C)]` 镜像，尺寸/偏移经 `offset_of!` 派生并逐项钉 C 值；参数数组与页表按字节读写，避开 C 的未对齐 u64 读）、`device.rs`（`err.c` 的 203 项结果码表逐条机械转录 + `convert_result` 六码映射 + 漂移规则）、`backdoor.rs`（`Backdoor` 缝：outl/物理地址/时钟/闹钟/`sys_hz`/IRQ 使能/授权映射；`KernelBackdoor` 全走 minix-sys 动词；PCI 探针腿按 C 的 `pci_first_dev`/`next_dev`/`reserve`/`attr_r32`/`attr_r8` 真实查询）、`service.rs`（设备腿：报告来宾→时钟漂移只前进→事件确认与 HGCM 检查；HGCM 腿：四操作全状态机——连接配额、请求页复用、参数重写、`store_pages` 页表构造、异步完成经中断回路、取消与关闭语义），20 枚新门（卡 20/0）；`minix-sys` 补 `sys_vumap`（SYS_VUMAP 的库层包装，单元素向量）与 `pm::stime_via`（PM_STIME），各带线格式门（minix-sys 329/0）。三处登记缝：PCI 属性读（pci 服务自己的缝，现答 ENOSYS，探针诚实失败而不编造端口）、`sys_irqenable` 无 Rust 动词、`-args` 环境；另在 `do_call` 加 count>MAX_PARAMS 的 EINVAL 硬化（C 无界拷贝会溢出 `param[8]`，合法调用方不可见）；**`cat24c256` + 块框架数据面裁决（本笔提交，i2c 消费族第五台 + 框架）**：**裁决 = 数据面进 `minix-driver-rt`**——新增 `plane.rs`：`GrantIpc`（grant/revoke/sendrec 三动词，对 `DriverTransport` 有 blanket impl，故设备腿写一次两处可用）、`TransferPlane: GrantIpc`（`caller` + `copy_from_caller`/`copy_to_caller`，即 C 的 `endpt`+`iov`+`sys_safecopy*`）、`Plane<'a,T>`（真传输上的实现）与 `NullPlane`（拒绝而非假装）。**框架侧**：`BlockDriver::transfer` 首参改为 `&mut dyn TransferPlane`（C 的 hook 收 endpt/iov 并自己拷贝；泵按消息构造 plane 传入），四台消费者（ahci/at_wini/virtio_blk/memory）签名跟随、内存驱动自备泵同步接线；i2c 客户端 `exec` 泛化到 `GrantIpc`，故设备腿既能走传输也能走 plane。**首台消费者 `cat24c256`**：`face.rs`（块面全腿：读 128/写 16 切片、`BDEV_NOPAGE` 单字节地址、copybuf 4K 分片、EOF/越界/ENXIO/vectored 拒绝、openct 计数、OTHER 类型、ENOTTY）+ `service.rs`（总线解析+预留+DS 通知刷新+泵包装）+ `main.rs`（真循环，双标签 `drv.blk.*`/`drv.i2c.*`），16 枚新门。**新登记缺口 `E-BLK-VEC`（P0-design-missing）**：gather/scatter 的 iovec 数组未建模——C 的 `do_rdwt` 从 grant 拷 `count` 个 iovec 再传 hook，Rust 泵把元素数当字节数直传（同族四台 hook 亦未用 `vectored`）；修法（泵内解 iovec + hook 收数组）留后续片。另 `fbd`/`filter`/`vnd` 三台的框架阻塞（transfer 无传输句柄）**就此解除**，各自只剩「下游转发腿 + 真循环」两件；**char 框架同类缺口（`cdr_read/write` 无 plane）**同法可闭，登记为后续片；**`vnd`（本笔提交，存储代理族首台）**：`protocol.rs`（ioctl 号按 `_IOC` 算式重导 + `struct vnd_ioctl`/`vnd_user` 镜像与偏移钉值）、`backend.rs`（`FileBackend` 缝：copyfd/pread/pwrite/fsync/fstat/close；生产 `VfsBackend` 走 `minix-sys::vfs` 客户端，pread/pwrite 照 C 库的三步 lseek 实现）、`face.rs`（块面全腿：open 的未配置/短设备/只读三闸与首开后重解析分区表、transfer 的 64 KiB 分片与 EOF 停、`BDEV_FORCEWRITE` 落 fsync、分区表 MBR 解析走自己的 transfer、几何）、`service.rs`（**控制请求在服务侧解码**——ioctl 钩子无 plane 的缺口用服务代劳：VNDIOCSET/CLR/GET + DIOCOPENCT/DIOCFLUSH 五条腿，其余落泵答 ENOTTY），22 枚新门（卡 22/0）。三处登记缝：`copyfd`（VFS 服务端 `do_copyfd` 已在树、客户端包装未落）、`fsync` 同、`instance` 环境解析；B3 尾注：`vnd` 的 VNDIOCSET 真机腿要等 copyfd 客户端包装落地 | 现算口径（本片后）：`find os/drivers -name main.rs \| wc -l` → **55**；`grep -rln 'loop {}' os/drivers --include=main.rs \| wc -l` → **26**（逐族 -1）；`cargo test --lib`：`minix-driver-vbox` → **20/0**、`minix-sys` → **329/0**；workspace 无 error；clippy 自有 0 |
| B2 | `A4` 协议常量单一来源 | `16-stage-drivers/todo.md:120` | `✅`（`2026-10-18`，本笔提交） | 已收口：跨消费者族（CDEV、BDEV）单一权威在 `minix-types::types::device`——chardriver 与 vfs 的 cdev/bdev 消费者侧前轮已收敛；本轮删掉 blockdriver 最后四条本地副本（`BDEV_REQUEST_BASE`/`BDEV_READ_ACCESS`/`BDEV_WRITE_ACCESS`/`BDEV_FORCE_WRITE`）改为 `pub use minix_types::{BDEV_FORCEWRITE, BDEV_R_BIT, BDEV_RQ_BASE, BDEV_W_BIT}`，内部三处与 `os/net/lwip/src/bpfdev.rs` 一行引用随动；device.rs 补块族旗标三枚（com.h:986-988）。单消费者族（NDEV/RTCDEV/USB）按 device.rs 模块头既定边界留在属主 | `grep -rn 'BDEV_REQUEST_BASE\|BDEV_READ_ACCESS\|BDEV_WRITE_ACCESS\|BDEV_FORCE_WRITE' os --include='*.rs'` → 0；`cargo test -p minix-types -p minix-blockdriver` + workspace 编译面 |
| B3 | `SD-11` T4：全局分配器的根修（`PD-28`） | `TODO-LEDGER-OPEN.md:57`；`STRUCTURAL-DEBT-REGISTER-20261008.md` 的 `SD-11` | `☐` **非单轮量级** | **进度（`2026-10-18`，`claim/SD11-MFS-zcode-ds`）**：按 U-5 裁决边界领首个独占切片——迁移波第九台 MFS（最后一个缓存大头）落地：`os/fs/mfs/src/main.rs` 在 server 构造后、`serve` 前安装 `install_vm_supplier` + `nk4a: mfs-vmbrk probe ok` 探针；宿主 178/0、三架构 none 构建全绿；跨线编辑已在 C 板 §6 登记。**余下两步均门控**：① U-5 执行清单第 2 项（mfs 迁移前后对照：分配路径读数、出生段池峰值、驻留成本）+ x86/aarch64 全量 boot 门——走 C 板真机窗；② 三条件齐备后才删 `alloc.rs` 的 x86/non-x86 `cfg` 分档、统一 1024 页（U-5 裁决明写「验收不过则 T4 保持打开」，不在门禁前改容量）。原执行图：按四级排序选首个迁移服务，落「执行与跟踪分离」的根修；**删除分档是显式验收项**。消费者是 VM 服务（A14），本板持 `minix-rt` 侧主线，跨线先登记。**U-5 对照读数准备（`2026-10-18`，`claim/SD11-MFS2-zcode-ds`）**：② 出生段池峰值探针已加（`nk4a: mfs pre-install pool pages used=<n> total=1024`，安装点前打印，串口读数待真机窗）；③ 驻留 `size` → `.bss 4,195,496 B`（≈4 MiB 池 + 1.2 KiB）；三架构 none 构建与宿主 178/0 复跑通过。**真机窗执行（`2026-10-19` 01:04–01:17，C 板令牌非抢占排队第 33 轮取到；日志在 `tmp/sd11-qemu/`）**：① 双架构冒烟门 **x86 FAIL（rc=1）**——stage 1–4 全过（镜像装配、ESP 十二模块点名、调度器交接、`rc: minimal boot script marker`），stage 5 的 `du /etc` 总行未到，串口实读 `du: fts_read: Input/output error` 两行（`/etc` 目录列腿）；**aarch64 FAIL（rc=1）**，失败行 `FAIL: /etc/rc command face (ls /bin, cat /etc/rc, du /etc, stat /etc/rc) output not fully observed`，尾段 `/etc/rc` 回读与 `e5-ds` not-found 皆正常。两条均归 **A 板 `C-57` 的已登记未闭面**（「`ls`/`du` 的目录列腿（imgrd 根盘 `getdents`）同属可写根盘 E5 面」），非本线新回归。② U-5 读数：`nk4a: mfs pre-install pool pages used=0 total=1024` **取到手**（安装点前固定池峰值 0/1024）；但安装后的 `nk4a: mfs-vmbrk probe ok` **在 300 s 窗口内未出现**，而同法探针的 sched/memory/tty/pfs/mib/ds/pm/vfs 八台都打了行——按代码顺序（读峰值 → `install_vm_supplier` → 取一页 → 打行）这说明 **MFS 的真机安装/取页腿没走完**（VM_BRK 应答停等或取页阻塞，待带诊断的加长窗复跑定性）。**B3 ① 未通过**：U-5 迁移真机腿未完成 + 双架构门带 A 板既有红面；②「删分档、统一 1024 页」按要求继续不开。 | `cargo test -p minix-rt`；A14 的 VM 宿主门；`FixedPoolSupplier` 不再焊死生产全局分配器 |
| B4 | 运行时 csu 链 13 项缺口 | `14-stage-runtime/todo.md:75` | `✅`（`2026-10-18` 逐项对账收口） | 13 项逐项表见 §2.5：8 项已有 Rust 等价面或已落（入口检查、`ps_strings`/`environ` 的访问器等价面、短名、分配器守卫、无参 `main`、入口寄存器、`exit`），2 项静态不可达（动态链接、`MCRT0`），3 项维持登记（preinit/init/fini 数组与 `atexit`——消费者为零：仓内无 `.init_array`/`.fini_array` 使用、无 `atexit` 调用，unknown-none 不链 crt 对象）。**无可实施项，按消费者规则收口**；将来引入 C 对象库或构造器登记再接线 | 逐项表 + 代码锚点重放（无代码改动；`cargo test -p minix-rt` 基线值守） |
| B5 | `V1-P1-1` 的剩余真机面 | `14-stage-runtime/todo.md:23`（诞生链已标 ✅）与 `TODO-LEDGER-OPEN.md:129`（记「真机 2」） | `✅`（`2026-10-18`，本笔提交） | **对账已完成，无需上机**：「✅ 的那一半」= 诞生链两步（第一步删 `start.rs` 死码、第二步 `rt-birth` 真机五断言）与 `V1-P1-5` 文档落稿，均阶段内闭合；「真机 2」具名 = `real-trap` 与 `panic-handler` 两个 feature 面——前者已改按 `target_os = "none"` 判定（feature 只剩宿主强制，`os/libs/minix-rt/src/init.rs` 的 cfg 与 `rt-birth/Cargo.toml` 的 `T-STAB-1` 注），后者已由 `rt-birth`/`pfs`/`mfs` 等 freestanding 二进制消费；真机验证归总账 §4 的 `E1` 行（C 线窗）。`V1-P1-1` 自身余项 `fini_array`/`init_array`（无消费方）归 B4 的 csu 链 | 台账与阶段账已同步；QEMU 用量 0 |
| B6 | `at_wini`（IDE 存储）真机尾件 | `16-stage-drivers/todo.md:165`（§7 起首）到 `:177`（尾件清单） | `⏸ 等 C 板真机窗` | `w_probe` 的 PCI 枚举与 `BlockDriver::transfer` 的「读设备到暂存 → grant 拷贝 → 每扇区 IRQ/超时」两段；claim 键用 `AT-WINI`（追踪键，不是权威台账 ID）。**`2026-10-18`**：服务壳已随 `G6-BLK` 落到 `drv.blk.at_wini`（泵在 `minix-blockdriver::pump`），`--lib` 现 **49/0**（旧基线 44 为更早读数，本片 +1 冒烟门）；真机两段不变。**`2026-10-19` 阻塞定性（本线取窗前置条件）**：真机腿不是「等窗」而是**「等模块入表」**——at_wini 不在 boot 模块表任何一环：`os/servers/rs/src/table.rs` 无 `at_wini` 条目、`os/xtask/src/image.rs` 模块清单无它、`os/qemu-tests/` 无对应腿脚本（三处 grep 均空，`2026-10-19` 实测）。⇒ 请求 **A 线**（RS 装机表加条目：label/endpoint/flags）与 **C 线**（xtask 镜像清单 + 一枚测试腿脚本）按 C 板 §6 登记；两件落地后本线再取窗复跑两段（`w_probe` 的 PCI 枚举 + 读设备到暂存→grant 拷贝→每扇区 IRQ/超时） | `cargo test -p minix-driver-at_wini --lib` → 49/0；真机腿走 C 板窗 |

## 1b. 二线跨阶段承接清单（原板遗漏的库面/驱动面开放项）

这一节与第 1 节平级；一次只领一个条目 ID。账载计数可能早于现树，领取前先复核正文。

| # | 条目/族 | 登记处 | 状态 | 这一轮做到哪算完 | 判据与门 |
|---|---|---|---|---|---|
| B7 | 16 篇其余开口项：`A2` 框架接线计划、`A10` 钩子签名 `Result` 化、`OQ-1`/`E-SDEVOWN`、`OQ-3` pci `BusControl`；另含输入驱动族的 `E-CDRCONV`/`E-TTYEVENT`/`E-PCKBDREG` | `16-stage-drivers/todo.md:76`、`:126`、`:145`、`:147`；`12-stage-input/todo.md:3` | `✅`（`2026-10-18` 逐项对账收口） | `G7` 已收口（doc 04/13/21 共 8 处）。**本行全项已有终态**：`A2` ✅（本笔收口——九批接线主体在树，net 九家 stub 依 edge2 L13 维持占位，typed CDEV 臂尾巴跟踪在 B9）；`A7` 🚫 否决（edge2 L13；`2026-10-18` 误执行一轮已 revert，条目与本节均已内联裁决）；`A10` ✅（edge2 L5 的 `338bfdf9f`，钩子签名迁 `Result` 全部落地）；`OQ-1` ✅（E-SDEVOWN 随 edge2 L8 的 `907e65f79` 迁入 `minix-sockdriver`）；`OQ-3` 🚫 保留（edge2 L13 死代码四裁决之一，服务层落地时再验）；输入三边 `E-CDRCONV`/`E-TTYEVENT`/`E-PCKBDREG` 均 ✅（edge2 L4/L6/L7）。**板行维护教训**：改板与领项前先 grep `edge*.md` 的裁决行，别把已有终态的项当开口项排期 | 每项自己的 crate 门 |
| B8 | `SD-19` 余下库 crate 的静默吞错门 | `TODO-LEDGER-OPEN.md:66`（本片进度注记在 §3 表后） | `✅`（`2026-10-18` 选项①执行） | `minix-vtreefs`/`minix-sffs`/`minix-types`/`minix-driver-rt` 四片早前已落；本笔按选项①把 `os/libs` 剩余十二家接上同款头门（`audiodriver`/`bdev`/`blockdriver`/`boot`/`chardriver`/`elf`/`i2cdriver`/`netdriver`/`platform`/`sockdriver`/`usb`/`virtio`），**二十一家全带门**。真重检新暴露 5 处真触发（`netdriver` 泵 3、`blockdriver` 泵 2，均为回复/状态发送尽力而为），逐站 `//[SD19-gate]`+语句级 `#[allow]` 收编；其余十家自有文件 must_use 真触发 0。**行内旧断言「均非 must_use」被本笔证伪**（旧记未对两家泵代码真跑门）。选项②所需的 B7/G7/B5 亦已全收口。`commands/*` 零散站归 A 板；**不升 `deny`** | 每 crate `cargo clippy -j 1 -p <pkg> --tests` 读数 + 处置表（本笔表见 §3 表后注）；无理由的新 `let _ =` 为 0 |
| B9 | 共享 wire/查询桩：`E-RSWIRE`、`E-KERNINFO`、`E-MINTYPES-RS`、`E-MINTYPES-RUNTIME`、`E-DSWIRE` 客户端半、`E-DMWIRE` 客户端半（边界登记见 C 板 §6）、`E-DEVWIRE` 常量 | `TODO-LEDGER-OPEN.md:80`、`:83`；`16-stage-drivers/todo.md:120`；`14-stage-runtime/todo.md:10` | `✅`（`2026-10-18` 逐项对账收口） | 七项逐项终态：`E-RSWIRE` ✅（DS 侧 shadow 半与 VM 侧 `403e47022` 两段进度注记）；`E-KERNINFO` ✅（字面「余项仅剩归档注记」）；`E-MINTYPES-RS` ✅（观察一/三闭环，观察二随 E7 执行轮）；`E-MINTYPES-RUNTIME` 🚫 执行半按 edge_todo 对账归 14-stage-runtime 轨道；`E-DSWIRE` ✅ 客户端半（DS 传输链真机通电注记）；`E-DMWIRE` ✅ 客户端/RS 半（`b05905406` 落地客户端生产传输与 RS publish 臂；本笔修 grant 收方——原按 `NONE` 签发会被内核 who_to 检查拒为 EPERM，改为按 C `cpf_grant_direct(devman_ep,…)` 绑定收方）；`E-DEVWIRE` ✅（edge2 L5）。**B 侧零剩余**：devman 请求分类器等服务端半按板行归 A（A17/A18）；typed 消息臂由 runtime 轨道接 | 本笔 `cargo test -p minix-sys` → 327/0、clippy 自有文件本门告警 0；其余条目宿主门见各提交 |
| B10 | `P-ALL-08` 信号代理循环与按号分派的消费方盘点 | `TODO-LEDGER-OPEN.md:45` | `✅`（`2026-10-18`，本笔提交） | B 侧收口：框架核与 `PD-27` 逃生门已闭（`cargo test -p minix-sef` → 19/0 + 集成门 3/0）；`SIGKSIG` 代理循环的对位在 PM 自己的 `process_sigmgr_signals`（`os/servers/pm/src/signal.rs:670`，已接线且带测试），minix-sef 零对位是设计边界（搬入即与 PM 双实现）；消费方盘点：`fs-rt`、`minix-driver-rt`、`net/lwip`、`net/uds`、`servers/devman`。B 侧无剩余代码；发方链通电归 A 板 T2 | 台账已同步；QEMU 用量 0 |
| B11 | `E-DMABUF` 与 `E-SDEVOWN` 的驱动侧归属 | `TODO-LEDGER-OPEN.md:83`；`16-stage-drivers/todo.md:145`、`:123` | `⏸ 等 A 板（余项已无可做动作）` | `E-SDEVOWN` 已由 edge2 L8（`907e65f79`）收口：sdev/sockevent 单点进 `minix-sockdriver`，vfs 副本 923→233 行；B 侧无剩余。`E-DMABUF` 的契约半（`minix-types` 的 DMA trait）已在树，驱动侧消费者（音频 `set_dma`、virtio 连续页）等 VM 侧的 `DmaMemory` 实现（A14/S37）；在那之前任何驱动都无法诚实拿到 DMA 地址。**B 侧无待做代码**，等 A 线实现后按族接线 | 确权已由 edge 裁决给出；实现侧门在 A 板 VM 测试 |

## 1c. `G6` 余下 36 台的分类（`2026-10-19` 盘点，供逐族领取）

清点在 `main` 的 `2026-10-19`（`20e6ce95c`）：36 台 = **12 台有模型无服务壳** + **24 台空 stub**（其中 9 台 net 变体依 edge2 L13 维持占位）。`G6-*` 系列片落地后为 **24 台 = 9 台有模型无服务壳 + 15 台空 stub**（9 台 net 变体在内）。每类的真实缺什么：

| 类 | crate | 已有模型 | 缺什么（决定为何现在不能直接接线） | 建议轮次 |
|---|---|---|---|---|
| i2c 客户端族 | `bmp085` ✅、`sht21` ✅、`tsl2550` ✅、总线本体 `i2c` ✅、`cat24c256` ✅（五片，数据面裁决后首台消费者） | 客户端与总线两侧都已在树：`minix-i2cdriver::client` + 总线 `service.rs` + `minix_driver_rt::plane` | **已闭**：`face.rs` 的 i2c 读写腿走 plane，`service.rs` 负责总线解析/预留/DS 刷新 | — |
| 音频族 | `es1371`、`sb16` | `AudioHooks` 十四钩子、`SubDevice` 分片机、`SpecialFile` | 数据面归属未裁决：C 由框架持有 DMA 缓冲并把地址交给 `drv_set_dma`，Rust 无 DMA 分配器（见 B11）；把缓冲放卡侧要改钩子签名 = 架构演进，需用户批准 | 设计裁决后再动 |
| USB 族 | `usbd`、`usb_hub`、`usb_storage` | 协议/枚举/端口观察/BOT 状态各有模型 | C 全经 DDEkit 宿主抽象；Rust 无对应层，`HostController` 缝（URB 调度、端口复位、地址分配）未设计 | 宿主缝设计后再动 |
| 存储代理族 | `vnd` ✅（首台，本笔提交）、`fbd`、`filter` 余 | 规则/镜像健康/布局模型齐 | **框架侧已备**（数据面裁决后 `transfer` 收 `&mut dyn TransferPlane`；下游块客户端在树 = `minix-bdev::client` 的 `transfer` + `GrantIssuer`）。**但「下游转发腿」自身还剩一个形态裁决**：C 的 `fbd_transfer_direct` 用 `cpf_grant_indirect` 把调用方 grant 直接转发（Rust 无间接授权动词），且转发用 GATHER/SCATTER 的 iovec 数组（与 `E-BLK-VEC` 同一缺口）；可行形态 = 走 C 的**拷贝路径**（`fbd_transfer_copy`：暂存→转发）并**逐元素发连续请求**（语义等价、多一次拷贝，须逐台具名登记）。`vnd` 另需 VFS 客户端（已在树：`minix-sys::vfs` 的 open/read/write/stat/lseek） | 逐台接线轮（先 `vnd`：C 603 行、下游是文件） |
| 存储硬件族 | `mmc`、`floppy` | 命令序列/几何模型 | 传输腿（寄存器/DMA/IRQ）未建模（C 源 1267/1355 行），需逐卡硬件缝 | 逐卡建模轮 |
| 空 stub（6） | 音频 5（als4000/cmi8738/cs4281/es1370/trident）、电源 1（acpi） | 15 行 stub | 无任何 Rust 模型，需从 C 源整台建模 | 逐台建模轮 |
| net 变体（9） | 3c90x/atl2/dec21140a/dpeth/fxp/ip1000/lan8710a/rtl8169/vt6105 | 15 行 stub | **edge2 L13 裁决维持占位**（无芯片语义，face 只能是空壳） | 不排期 |

已接的三十一台（`log`/`readclock`/`random`/`fb`/`pty`/net 五台/存储三台/`printer`/`pci`/`gpio`/`bmp085`/`sht21`/`tsl2550`/`i2c`/`ti1225`/`tps65217`/`tps65950`/`amddev`/`tda19988`/`vbox`/`cat24c256`/`vnd`）之外的 24 台，**没有一台是“只差把现有 face 接到泵上”**——这是本轮清点的结论，后续领取按上表选类，别再按“占位计数”直接接。

## 2. ⛔ 禁碰清单

| 不可写 | 属于谁 | 为什么硬 |
|---|---|---|
| `os/libs/minix-boot/` | **C 板**（协议 §2 明文确权：它的唯一消费者全在一线 `boot-shim`/`kernel`/`arch`/`qemu-tests`） | 引导契约面，改一次要三架构同时验 |
| `os/kernel/`、`os/arch/`、`os/plat/`、`os/boot-shim/`、`os/kernel-image/`、`os/qemu-tests/` | C 板 | 真机面 |
| `os/servers/`、`os/fs/`、`os/net/` | **A 板**（三线服务器/文件系统/网络） | 与 B3/B9/B10 有交接，最容易撞；交接前先在 C 板 §6 挂账 |
| `os/commands/`、`os/etc/`、`os/xtask/` | A 板 | 命令面薄壳与 `bin_support` 双生面 |
| `rewrite-notes/01-stage-kernel/*`、`00-master-plan/*` | C 板 + **当前活跃文档线程** | 只读 |
| `rewrite-notes/02`–`13`、`15`、`17`、`18` 阶段账 | A 板 | 三线的阶段文档 |
| `rewrite-notes/coordination/TODO-LEDGER-OPEN.md`、`TODO-LEDGER-DONE.md`、`new_edge*.md`、`PARALLEL-*`（别人的板）、`STRUCTURAL-DEBT-REGISTER-20261008.md`、`DOC-ITER-LEDGER.md`、`CONCURRENT-WORK-PROTOCOL.md`、`README.md` | 编排线 / 各板自持 | 协议 §3：多写者必覆盖 |
| `os/Cargo.toml`、`os/Cargo.lock`、`.github/workflows/`、`tools/`、`AGENTS.md` | 共读共享件 | 加成员/改特性/动 CI 先在 C 板 §6 登记；`Cargo.lock` 只由实际构建产生，不手改 |

## 3. 门配方（本板最要紧的是「下游基线」）

**动上游之前先量下游**，否则改完不知是谁变红：

- 下游基线（A 板的两个包，切板时的读数；领取时复跑一次）：
  `cargo test --offline -j 1 -p minix-textfilter` → **1076 passed / 0 failed**（68 target 段）；`-p minix-text-games` → **60 passed / 0 failed**。
- 本板包的门：先跑 `cargo test --offline -j 1 -p <pkg>` 取一次基线，**把数字写进本表对应行的「判据与门」列**（本板切板时未替你量，因为二线的包多、必须按你实际领的那一族来定）。拿包名先 `cargo metadata --no-deps --format-version 1`，不要按目录名猜。
- 内核宿主测必须带形态：`cargo test -p minix-kernel --features mock`（裸跑会被 `os/kernel/src/lib.rs` 的 `form_contract` 编译期绊线挡住——那是设计行为，别放宽它）。这条对 B3/B10 尤其相关。
- 权威容器门与在线/离线秒判：**照 A 板第 3 节原样执行**（`-u "$(id -u):$(id -g)"` 不可省；`000`/`28` 是宿主代理假 IP 面没起来，不是 Docker 坏）。
- 三架构 `--bins` 编译面：`cargo check`/`build` 按 `os/` 根执行；**真机跑归 C 板窗**。

## 4. 模式与红线（本板专属）

- **`!Send`/`!Sync`/`Rc`/`RefCell` 只在用户态服务器的事件循环里合理**；一旦你的改动落到 `os/drivers/` 里被内核侧消费的面，就要按内核 SMP+BKL 口径重审（禁 `Rc`/`RefCell` 跨 CPU，共享数据 `Arc`+`Mutex`/`Atomic`）。
- 错误类型必须映射到 Minix3 的 errno 值，**禁止自造错误码**；传输失手走既有的 E-SYSCALL-SIGN 口径。
- 硬件细节不许越过语义层：驱动框架里不要把 CR3/PTE 之类名字暴露给 OS 语义面（判据见 `prompt/review-rules/review-core-semantics.md`）。
- 三档术语必须三处一致标注（文档 + 设计 + 代码）：Rewrite / Refactor / Architectural Evolution `[ARCH: ...]`。

## 5. 每轮收尾（顺序固定）

claim 树内一笔提交（改动 + 本板状态 + 阶段账批次条目）→ 主树 `git merge --ff-only claim/<ID>-zcode-ds` → `bash tools/claim.sh release <ID> zcode-ds` → 五道校验（A 板第 7 节那五条命令）→ 若 ff 失败按 A 板第 7 节的合并法在自己分支里解冲突。
**销账前确认树真的从磁盘消失**：`bash tools/claim.sh sweep` 判「可回收/需核对」，别留 root 属主残骸（本会话刚为这类残骸回收了 3.5G）。
**`doc-style-lint --diff` 若报 `rewrite-notes/01-stage-kernel/todo.md` 的命中**，那是文档线程的在制品，不是你的改动；只处理自己路径的命中。

## 6. 与另两板的依赖边

| 本板条目 | 关系 | 说明 |
|---|---|---|
| B1/B2/B7/B8/B9 | **A 板是下游** | 命令面与服务器面通过 `minix-sys` 的 wrapper 调你的面；改签名前在 C 板 §6 挂账 |
| B3 `SD-11` | **A14 是消费者** | 分配器根修落在 `minix-rt`，VM 服务的安装点是 A14；两边同轮验收 |
| B5/B6 真机 | 等 C 板 | 走令牌窗，不自起 QEMU |
| B9 `E-DSWIRE`/`E-DMWIRE` | A 板持服务端主线 | 本板只做 `minix-sys` 客户端半与 `minix-types` 解码 |
| B10 `P-ALL-08` | A 板持服务器侧 | 本板只动 `minix-sef` 框架 |
| B11 `E-SDEVOWN` | 与 A 板确权 | `sdev/sockevent` 与 vfs 副本的去留必须一处说话 |
| B2 `A4` 常量 | A 板持消费者侧 | `os/servers/vfs` 的重述归 A，本板不代改 |
| B1 的 2 个 fs `loop {}` | A 板（A20） | 驱动侧的 52 个归本板，`os/fs` 的 2 个归 A，`os/boot-shim` 的 1 个归 C |
