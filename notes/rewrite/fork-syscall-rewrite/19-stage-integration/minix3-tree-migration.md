# minix3 源码树目录级迁移评估

## 1. 这份文档回答的问题

minix3 参考源码树顶层有 21 个目录。重写工作至今集中在其中一棵子树(`minix/`,内核与服务器),其余目录各自是什么、os/ 侧有没有对应物、值不值得迁移,此前没有一份完整的地图。本文档以目录为粒度逐一回答这三个问题,并给出测试资产的迁移策略。

**评估基准的声明**:minix-rs 是尽力而为的 minix3 重写项目,它的边界由维护者的能力与时间决定,不是冻结态——当前哪些服务能在宿主上跑真代码,只说明今天的落点,不构成对未来边界的预测。因此本文档对每个目录给出的不是"现在能不能迁"的快照判断,而是三样东西:

1. 这个目录在 C 源里的真实角色(带证据锚点);
2. 迁移它依赖什么前提(哪些服务就位、哪个子系统先点亮);
3. 它的价值密度(单位工作量换来的行为保证有多少)。

价值密度高的项,即使依赖深,也值得在路线图上留位置;依赖浅但价值密度低的项,明确标"不做",避免未来反复重新纠结。

## 2. 对照方法:角色映射,不是名字映射

os/ 侧不复制 C 树的目录名,直接拿名字对照会得出大量假阴性。两处结构性差异需要先说明:

**用户态工具:功能组制对逐工具制。** C 树把用户态工具按安装位置分四个目录(`bin/` 下 32 个工具、`usr.bin/` 下 143 个、`sbin/`、`usr.sbin/`);os/ 侧对应物 `os/commands/` 按功能分组——`bin/`、`sbin/`、`usr-bin/`、`usr-sbin/`、`games/` 五个组,组内再按功能聚合(如 `os/commands/bin/shell`、`os/commands/bin/fileops`、`os/commands/sbin/init`)。所以"C 树有 `bin/cp` 而 os/ 没有 `commands/bin/cp`"不构成缺口,要看的是角色:`cp/rm/mv/ln` 这类文件操作归 `fileops`,`sh/ksh/csh` 归 `shell`。

**库与头文件:crate 化。** C 树的 `lib/`(用户态库)与 `include/`、`minix/include/`(头文件)在 os/ 侧对应 `os/libs/` 下二十余个 crate(`minix-types`、`minix-sys`、`minix-rt`、`minix-fs` 等)。头文件的内容进入 crate 的类型与常量定义,不再以 `.h` 形式存在。

按这套角色映射,21 个目录分成三类:角色已被覆盖(第 4 节)、无对应但值得规划(第 5 节)、无对应且不建议迁移(第 6 节)。测试资产(`minix/tests/`)体量与策略特殊,单独成节(第 7 节)。

## 3. 全树对照总表

| C 树目录 | 是什么 | os/ 对应 | 判断 |
|---|---|---|---|
| `minix/` | 内核、11 个服务器、驱动、Minix 专属库与系统头——源码的心脏 | `os/kernel`、`os/servers`、`os/drivers`、`os/fs`、`os/libs`、`os/arch` | 重写主对象,进行中 |
| `tests/` | 从 NetBSD 导入的 ATF 测试树,约 1950 个文件 | 无(`os/tests` 是自己的接缝测试,不对应它) | 不迁移(见 6.1) |
| `bin/` | /bin 下 32 个基础工具 | `os/commands/bin` | 角色已覆盖 |
| `sbin/` | /sbin 下 21 个系统工具 | `os/commands/sbin` | 角色已覆盖 |
| `usr.bin/` | 143 个用户工具 | `os/commands/usr-bin` | 角色部分覆盖 |
| `usr.sbin/` | 9 个管理工具 | `os/commands/sbin` | 角色部分覆盖 |
| `games/` | 24 个游戏 | `os/commands/games` | 角色已覆盖 |
| `etc/` | /etc 配置与启动脚本 | `os/etc` | 部分覆盖,随系统面扩张 |
| `lib/` | 用户态库:NetBSD libc/libm 全套 + Minix 专属库(`libsys`、`libminc`、`libmthread` 等) | `os/libs`(crate 化) | Minix 专属库角色已覆盖;libc 兼容层未覆盖(见 7.2) |
| `include/`、`minix/include/`、`sys/` | 用户态头、Minix 系统头、NetBSD 内核源树 | crate 内类型定义 | 语义已覆盖 |
| `tools/` | 交叉构建的宿主工具链支撑(build.sh 体系) | `os/xtask` | 角色已覆盖 |
| `minix/tests/` | Minix3 自己的 POSIX 合规测试套件(94 个编号程序 + 4 类子系统测试) | 无 | **本树最值得迁移的测试资产**(见第 7 节) |
| `libexec/` | 守护进程:`getty`、`ld.elf_so`、`ftpd`、`telnetd`、`rshd`、`httpd` | 无 | 部分规划:`ld.elf_so` 关系动态库决策,`getty` 关系登录链;网络守护暂缓 |
| `distrib/` | 发行集构建(sets 体系) | 无(`os/xtask`、`os/kernel-image` 承担装机角色的一部分) | 待装机链成型后补 |
| `releasetools/` | 发布镜像工具(ARM SD 镜像、mkboot、u-boot 脚本) | 无 | 同上 |
| `share/` | man 手册、locale、terminfo、mk 片段 | 无 | man 面长期有价值,优先级低 |
| `docs/` | 仅 `UPDATING`、`profiling.txt` 两个文件 | 无 | 不迁移 |
| `external/` | NetBSD external 导入树(bsd/gpl2/gpl3/lgpl3/mit/public-domain/historical) | 无 | 不迁移(见 6.2) |
| `common/` | NetBSD 导入代码的公共支撑(libc/libprop/libutil 的共享段) | 无 | 不迁移 |
| `crypto/` | OpenSSL 导入 | 无(`os/libs/minix-crypt` 是另一回事:文件系统加密库,不对应它) | 不迁移 |
| `dist/` | 仅 pf 包过滤器 | 无 | 不迁移 |
| `gnu/` | gcc/binutils 构建 | 无 | 不迁移(宿主工具链,不属于重写对象) |

## 4. 角色已被覆盖的目录

### 4.1 `minix/`:重写主对象

`minix/` 内含 `minix/servers/`(devman、ds、input、ipc、is、mib、pm、rs、sched、vfs、vm,共 11 个)、`minix/kernel/`、`minix/drivers/`、`minix/fs/`(各文件系统实现)、`minix/lib/`(Minix 专属库)、`minix/include/`(系统头)、`minix/net/`(lwip 与 uds)。os/ 侧的 `os/servers/` 与这 11 个服务器一一对应,文件系统实现在 `os/fs/`(mfs、ext2、isofs、pfs、procfs、ptyfs、hgfs、vbfs),库面在 `os/libs/`。这是各 stage 文档的主战场,本文档不再展开。

### 4.2 用户态工具面:`bin/`、`sbin/`、`usr.bin/`、`usr.sbin/`、`games/`

C 树按安装位置分目录,os/ 侧 `os/commands/` 按功能组聚合(组织差异见第 2 节)。角色覆盖情况:

- `bin`:32 个工具中的基础面(shell、文件操作、进程工具、终端)已由 `os/commands/bin/` 的功能组承担;
- `sbin`:`init` 有直接对应物 `os/commands/sbin/init`;`fsck/mount/newfs_*` 这类文件系统管理工具随 `os/fs` 各文件系统的成熟逐步补;`ifconfig/route/ping` 属网络面,依赖 INET;
- `usr.bin`:143 个工具覆盖面最广,`os/commands/usr-bin/` 目前承担了 compress、文本过滤、正则、文档工具、login 五组,长尾工具(编辑器之外的大量小工具)是后续按需扩容的区域;
- `usr.sbin`:9 个工具(chroot、inetd、installboot、makefs 等),装机与网络相关的暂缓,其余随需;
- `games`:C 树 24 个游戏在 `os/commands/games/` 有三个功能组对应。游戏不是装饰——它们是终端语义(`stty`/行规程/信号)与进程语义的轻量真实负载,fortune、snake 这类程序在 C 树测试体系之外提供了"真实程序能跑"的烟雾测试价值。

### 4.3 `lib/`:库面与 libc 分层的证据

`minix3/lib/` 是 NetBSD libc 全套(`libc`、`libm`、`libedit`、`libcurses`、`libz` 等),`minix3/minix/lib/` 是 Minix 专属库:`libsys`(系统调用桩)、`libminc`(Minix 对 libc 的扩展与差异面)、`libminixfs`(文件系统服务器公共框架)、`libbdev`/`libblockdriver`/`libchardriver`(驱动框架)、`libtimers`、`libexec`、`libmthread`(用户态线程库)、`libddekit`(Linux 驱动移植层)、`liblwip` 等。

os/ 侧 `os/libs/` 已按 crate 对应了 Minix 专属库的大部分角色:`minix-sys`(对应 libsys)、`minix-fs`、`minix-bdev`、`minix-blockdriver`、`minix-chardriver`、`minix-rt`(对应 libexec/csu 的运行时角色:`crt0`、`alloc`、`signals`)、`minix-sef`、`minix-timers` 所在的 `minix-platform` 等。

关键结构事实:**C 树自己就把"POSIX 通用面"与"Minix 专属面"分开了**——NetBSD libc 提供前者,`libminc`/`libsys` 提供后者。这个分层直接支撑第 7 节的双腿策略:C 树里没有"一整个要重写的 libc",只有一层薄薄的 Minix 粘合面 + 一大块不打算重写的 BSD 通用面。

### 4.4 头文件与 `sys/`:语义覆盖,形式不同

`minix3/include/` 是 NetBSD 用户态头(stdio.h、unistd.h 全套);`minix3/minix/include/` 是 Minix 系统头(`minix/`、`sys/`、`net/`、`arch/`、`ddekit/`);`minix3/sys/` 是 NetBSD 内核源树布局(altq、ufs、uvm、netinet 等)。第三者需要说明:Minix3 不构建 BSD 内核,`sys/` 树在构建里的实际引用点只有版本信息脚本(`share/mk/bsd.own.mk` 引用 `sys/conf/newvers.sh` 与 `sys/conf/osrelease.sh`)和 libc 个别体系结构目录的粘合文件,BSD 内核本体(如 uvm)没有被 Minix 构建引用。os/ 侧不需要它的对应物,头文件的语义已进 crate 类型定义。

### 4.5 `tools/`:构建工具角色由 xtask 承担

C 树的 `tools/` 服务于 NetBSD build.sh 交叉构建体系(宿主 awk、binutils、gcc 包装等)。os/ 侧的构建编排由 `os/xtask` 承担,不迁移。

## 5. 无对应但值得规划的目录

### 5.1 `libexec/`:登录链与动态链接器

内容是 `getty`、`ld.elf_so`(动态链接器)、`ftpd`、`telnetd`、`rshd`、`httpd`、`fingerd`、`makewhatis`。两个成员有真实的路线图含义:

- `ld.elf_so` 是动态库决策的实体:如果未来决定支持用户态动态链接(见 7.4 对 test63 的讨论),这个组件及其加载语义就要有对应物;决定不做,它就保持不存在。
- `getty` 是登录链的一环(内核 → init → getty → login → shell)。`os/commands/usr-bin/login` 已存在,getty 是它前面的缺环。

网络守护(ftp/telnet/http)依赖 INET 与用户态网络面,排在网络面点亮之后。

### 5.2 `distrib/` 与 `releasetools/`:装机链

`distrib/` 是发行集(文件集划分与打包),`releasetools/` 是发布镜像工具(ARM SD 镜像脚本、mkboot、u-boot 引导参数生成)。os/ 侧已有承担部分装机角色的组件(`os/kernel-image`、`os/boot-shim`、`os/xtask` 的镜像目标),但"从构建产物到可启动介质"的完整链条尚无对应物。这一链路在载体能稳定引导用户态之后价值陡增——它是"系统能交付给别人跑"的最后一公里。建议在装机链成型阶段统一补,不提前拆散迁移。

### 5.3 `share/`:man 手册面

man 手册、locale 数据、terminfo。手册面长期有价值(重写文档可以对照 C 手册核对语义),但它是文档资产而非代码资产,不阻塞任何功能,优先级低。

## 6. 无对应且不建议迁移的目录

### 6.1 `tests/`:NetBSD ATF 测试树

约 1950 个文件,按 NetBSD 源树结构组织(README 明确目录结构跟随 NetBSD 源树位置),依赖 ATF/kyua 测试框架、rump 内核与 NetBSD 专属设施。**Minix3 自己没有构建它**:`share/mk/bsd.own.mk` 中 `MKATF:=no`、`MKRUMP:=no`,且 `tests/Makefile` 的子目录清单注释明确标注 "Unsupported on MINIX: net" 并裁掉了 crypto/ipf/share 等子树。它测的是 NetBSD 的实现细节,不是 Minix 的外部行为,对重写无验收价值。

它的正确用法是**查漏参考**:写某个系统调用的 Rust 测试时,`tests/lib/libc/sys/` 下同名测试的用例集可以当用例清单核对一遍(哪个边角 C 语义被漏了),仅此而已。

### 6.2 `external/`、`common/`、`crypto/`、`dist/`、`gnu/`、`docs/`

- `external/`:NetBSD 按许可证分目录的导入树。Minix3 实际用到的导入件(lwip)在 `minix/net/lwip`,不在 external/ 下;其余主体不参与 Minix 构建。
- `common/`:导入代码的公共支撑段(libc/libprop/libutil 的共享实现),只服务于 external 导入件的构建。
- `crypto/`:OpenSSL 导入。`os/libs/minix-crypt` 名字相近但角色不同(文件系统加密),不存在对应缺口。
- `dist/`:仅 pf 包过滤器。做包过滤才需要,当前边界外。
- `gnu/`:gcc/binutils。编译器属于宿主工具链,不属于重写对象。
- `docs/`:仅 `UPDATING`、`profiling.txt` 两个文件,信息量可忽略。

## 7. 测试迁移专项:minix/tests 的双腿策略

### 7.1 这套资产是什么

`minix3/minix/tests/` 是 Minix3 自己的测试套件:`run` 脚本自述 "Running POSIX compliance test suite",安装位置是 `/usr/tests/minix-posix`(`minix/tests/Makefile` 的 `BINDIR`)。构成:

- **94 个编号 C 程序**(test1 – test94,编号有空洞),每个是自包含的用户态程序,只依赖 libc,进程退出码即结果;`run` 逐个执行并统计,`-T` 选项输出 TAP 格式(可直接接 CI);
- **4 类子系统测试**:`kernel/sys_vumap` 与 `kernel/sys_padconf`(通过真实驱动进程测内核的 vumap/padconf 系统调用)、`ds/`(DS 服务器全 API)、`blocktest`(块设备)、`ddekit`(驱动移植层);
- **一组脚本**:`testmfs.sh`(构建 MFS 镜像并校验 sha1)、`testisofs.sh`、`testvnd.sh`、`testinterp.sh`(#! 解释器)、`testsh1.sh`/`testsh2.sh`(shell 行为)、`testrmib.sh`、`testrelpol.sh`。

它的目标:把整套系统(内核 + PM + VFS + FS + INET + tty)当黑盒,从用户态验证 POSIX 外部行为,包括 errno 这类细节语义。对以"外部行为保持"为纲领的重写,这套套件就是外部行为的可执行定义。

### 7.2 为什么是两条腿,而不是"保持 C 原样"一句话

此前评估建议"C 原样保留,不翻 Rust"。这个结论在"项目自带 libc"的前提下成立,但 minix-rs 现在没有、也不计划近期实现 libc。C 测试要跑起来,路径必须经过 libc:stdio、字符串、信号封装,最终落到系统调用。没有 libc,C 量尺无处安放。所以策略修正为双腿:

**C 腿(量尺保持原样,暂挂起)。** 94 个程序原样保留,一个字不改。它们在两种未来里复活:

1. 某天实现 libc 兼容层(Redox relibc 形态:给 native 库加一层 POSIX 兼容面)——这批 C 测试**就是那个兼容层的验收套件**,一层 libc 写得对不对,拿 Minix3 的合规套件跑一遍就知道;
2. 载体若选择直接复用某个现成 libc(如 musl 适配),C 腿变成"libc 适配 + 系统语义"的联合验收。

C 腿的价值不因挂起而贬值:它是量尺,量尺的正确性不能依赖被测系统,翻译量尺反而引入翻译错误风险。

**C 腿激活路径:复用树内 libc,不新写兼容层。** 点亮 C 腿的最短路径不是写一套 relibc 式薄层——自写层自己的缺陷会污染测试结果,量尺失真正是保持 C 原样的全部理由——而是把树里现成的 NetBSD libc(`lib/libc`)装上腿:Minix3 用户态用的就是它,本体是架构无关的 C,零重写。需要补的是三样:各架构陷阱桩(`libc/arch` 只有 arm/i386、`libsys/arch` 只有 earm/i386,三个目标架构都没有;x86_64 的 IPC 桩对齐 `minix-sys` 已定义的陷入面——其 IpcTransport 方法逐条对位 C `_ipc.S` 的寄存器约定,桩逻辑无需改)、C 的 crt0 对接 `minix-rt` 的 handoff ABI(kerninfo 页、栈与参数布局),以及交叉 C 构建胶水(项目至今是纯 Rust 构建面,引入 clang 交叉 + sysroot 是最大单项)。

**这套 libc 的年代与冻结定位。** 抽样 RCS 标识跨 2004-2015(printf.c 2013、strftime.c 与 getaddrinfo.c 2015),主体是 NetBSD 6/7 时代随 Minix3 最终状态(约 2015)冻结的快照;今天上游已是 NetBSD 10/11。对本项目的用途,冻结是特性而非缺陷:C 编号测试与这套 libc 和头文件同步演化,断言里的 errno 值、结构布局、行为边角都是对着这个年代校准的——把 libc 升到上游新版本反而制造量尺与被测系统的代差。政策:**冻结快照,不追上游**;确证的缺陷个案选择性回补,个案在测试挂账里记录。arch 目录的古早(earm/i386)不参与构建,无影响。

**64 位头文件是真正的适配工作。** 探针实证(libc 三个代表文件用现代 gcc 编译):`gen/errno.c` 干净通过,`stdio/printf.c` 仅告警通过——libc 的 C 本体与现代工具链兼容。拦截点是头文件树:用户态头(`include/`)是 32 位 i386 时代产物,64 位编译器下 `string.h` 的 memchr、`stdio.h` 的 fwrite 与内建声明冲突(gcc 判 error),`sys/sys/types.h` 引用的 `pthread_types.h` 树内缺失。这是已知形状的工作:参照上游 NetBSD 自身 amd64 化的演进给头文件做 LP64 化(尺寸类型加 `_LP64` 守卫、补缺失头)。依赖顺序不变:载体用户态可跑(进行中)→ test12 试点(fork/wait ×1000,libc 面最小,一次验证 fork/waitpid/stdio 三件事)→ 按域铺开。

**Rust 腿(native 面直测,现在就可以动)。** 把编号测试的**语义内容**(不是逐行代码)翻译进 `os/tests/`,直接调用 `minix-rt`/`minix-sys` 的 native 接口驱动服务器真代码。现有 `os/tests/ds_publish_subscribe.rs` 已经示范了这个形态:宿主上跑真服务器逻辑,IPC 与 grant 传输用脚本接缝,测试文档注释里明确记录了"宿主半能跑、真机三链挂后续里程碑"的分工。Rust 腿的每个测试文件头标注它翻译自哪个 testN,与 C 腿共享编号空间,结果可比对。

**关于 native 库的命名**:不需要新造名字。现有 `minix-rt`(crt0、alloc、signals)加 `minix-sys`(IPC、grant、系统调用面)已经是 native 用户态面的载体;未来若做 POSIX 兼容薄层,自然的名字是 `minix-libc`——这恰好复刻 C 树自身的三层结构(NetBSD libc 通用面 / libminc Minix 差异面 / libsys 桩面),Rust 侧对应为 minix-libc 兼容面 / minix-rt+minix-sys native 面。Redox 的 relibc 是同一思路的先例。

**已翻译映射(Rust 腿第一轮,`os/tests/` 包 minix-tests)**:九个域文件,每个测试函数头带 C 锚点,`#[ignore]` 注明点亮前提,随系统成熟逐域摘帽:

| 测试文件 | 翻译自 | 层面 |
|---|---|---|
| `t_proc_lifecycle.rs` | test13/70 | VFS filp 共享继承 + test70 消息字段对账(C 父子各持独立文件,共享偏移为 POSIX 继承语义推论) |
| `t_signals.rs` | test5/37/41/52 | PM 信号处置、掩码 pending、kill 权限、alarm |
| `t_credentials.rs` | test11/46/89 | PM 凭证:saved-id 舞步、权限门、setgroups、setsid |
| `t_fs_dir_ops.rs` | test19/21/22/28/32/34/61/78 选集 | MFS 真盘:目录/链接/symlink/chmod |
| `t_fs_special.rs` | test58/78 | MFS 真盘:mknod rdev、lstat 本体、被删目录消解 |
| `t_pipe_select_locks.rs` | test7/8/19/20/29/40 | VFS 决策面:管道矩阵、select 三集、记录锁 |
| `t_memory_vm.rs` | test6/44/64 | minix-sys brk/mmap/vm_fork 的 wire 字节契约 |
| `t_tty_termios.rs` | test74/77 | termios 44 字节布局、tty ioctl 请求号 ABI |
| `t_net_sockets.rs` | test48/56/90 策略面 | UDS 准入/环/控制长度、lwip 地址工具 |

**跨包桥 v0(LOOKUP 段已通,`os/tests/t_vfs_mfs_bridge.rs`)**。桥的形态:外层循环"`flush_pending_fs` 发送半 → `task::run` 驱动 fs-rt 解码/分派 → `handle_fs_reply` + `run_worker_continuations` 续接",直到 syscall 完成。两项设计定案:

1. **grant 宿主模型 = 真地址直读**。VFS 的 `GrantTable` 登记本就持有真实宿主地址(worker scratch、用户缓冲),桥经 `minix-sys` 新增的内核视角读取 API(`GrantTable::probe`)取窗后按裸指针直读直写,越界即 `EINVAL`——与 VFS 自身宿主模型及 mib_sysctl 裸指针回放判例一致。
2. **请求映射 = FS_BASE 偏移**。`pending_fs.req` 的 m_type 经 `TransId::add` 盖 transid 章后,`TransactionId::decode` 拆出请求号,`- FS_BASE` 即 fs-rt `RequestNumber` 表索引(两套常量同源于 C `vfsif.h`)。

扩展段(数据面读写、ReadSuper 经桥、Create/Delete 族)按同一循环逐请求类型接入;每接入一类,对应编号测试语义即可在宿主全链点亮。

test1/2/12 的 fork/wait 语义不重复翻译,由 `pm_vm_fork.rs` 与 `servers/pm/tests/run_once_integration.rs` 承接(文件头映射表有对照)。翻译与产品实现的对账现状:VFS fork 全链计数(`copy_fproc` 递增、退出侧 `close_fd`/`put_vnode` 清减,对位 `misc.c:616-617`/`632-633`/`651-660`)、MFS `FsDriver::rename` 臂(引擎 `link.rs:429` 的入口)、`minix_sys::vm::fork_address_space_via` 的命名 `mess_1` 车道(`com.h:633-635`)三处与 C 对位;对应测试均为运行态转绿。

### 7.3 编号测试全景清单(含决策驱动项)

清单按领域组织。每行给出:测试号、验证内容、迁移腿(全部先走 Rust 腿;C 腿整体挂起)、依赖前提。"暂缓/边界外"不是永久排除——每一条都是扩大边界时的决策入口,理由写在行内。

**进程与信号(12 项)**

| 测试 | 验证内容 | 依赖前提 |
|---|---|---|
| test1 | fork 基础与信号递送到子进程(`test1.c`:`test1a`/`test1b`) | PM + 内核调度 |
| test2 | fork/wait/pipe 混合计时与僵尸回收 | PM + VFS |
| test5 | 同 uid 父子 kill 互发、被信号打断的 EINTR、setuid/setgid(root) | PM 权限面 |
| test12 | fork 简单语义 | PM |
| test13 | pipe + fork 继承 | VFS + PM |
| test37 | 信号处置 + setjmp/longjmp 穿越 | PM 信号面 |
| test38 | 信号与文件操作交错 | PM + VFS |
| test41 | alarm 定时与信号 | 时钟 + PM |
| test42 | ptrace 全套(1509 行,套件最大单文件) | PM ptrace 面;若不做调试器支持则是边界决策项 |
| test52 | pipe + fork 轮替计算,SIGCHLD handler 内 wait | PM + VFS |
| test57 | 信号后寄存器恢复(`test57loop.S`,i386 专属) | 体系结构相关,按载体体系结构决定 |
| test62 | 信号时序(i386 专属) | 同上 |

**文件系统语义(27 项,最大价值密度区)**

| 测试 | 验证内容 | 依赖前提 |
|---|---|---|
| test4 | open/read/lseek/unlink 基础 | VFS + MFS |
| test7 | pipe/mkfifo/fcntl 组合 | VFS |
| test8 | pipe 行为细化 | VFS |
| test10 | execl 执行语义 | PM exec |
| test11 | exec 与 UID/GID 语义 | PM 权限面 |
| test14–test36(23 项) | 目录操作、link、rename、umask、时间戳(test16)、access(test33)、utime(test35)、mkdir/rmdir(test28)、chdir、sync、mkfifo、symlink、dup/fcntl——主题不随编号单调,逐个迁移前先核对头注释 | VFS + MFS;宿主接缝上可先行(Rust 腿) |
| test43 | realpath(3) 解析(lstat 为其中一环) | VFS |
| test46 | getgroups/setgroups 专项(root);rename 仅作写权限探针 | VFS + PM |
| test50 | truncate(2) 家族(ftruncate/ftruncate 到超界) | VFS + MFS |
| test54、test55 | close/unlink、write 边角 | VFS |
| test58 | 当前工作目录被删除/替换后的行为(`test58.c` 头注释) | VFS |
| test61 | 悬空符号链接消解 + mknod 已存在名 EEXIST | VFS |
| test65 | setuid 下 mkdir | VFS + PM |
| test70 | 多进程并发 lseek 的消息字段竞争回归(父子各持独立临时文件;VFS 单线程架构下按字段对账钉住) | VFS |
| test73 | VM 二级缓存黑盒测试(testvm 服务;umask/setuid 仅为运行前提) | VM + 载体 |
| test78 | mknod/symlink/lstat 组合 | VFS |
| test86 | chmod 对 exec 的影响 | PM + VFS |

**select/终端/杂项(10 项)**

| 测试 | 验证内容 | 依赖前提 |
|---|---|---|
| test40 | select + exec(t40a–t40g 七个变体) | VFS + PM |
| test47 | 信号(i386 专属) | 按载体决定 |
| test53 | setjmp/longjmp + 信号 | PM 信号面 |
| test68 | 信号 + pipe + exec 组合 | PM + VFS |
| test69 | time 精度 | 时钟 |
| test74 | mmap + select + ioctl + 文件系统缓存(挂 testcache) | VM + VFS |
| test76 | select + socket + pipe | VFS + INET |
| test77 | termios/pty/setsid | tty 面 |
| test79 | select + pipe + setuid | VFS + PM |
| test84 | posix_spawn + 不可执行脚本拒绝(`test84.c` 头注明改编自 NetBSD `t_spawn.c`) | PM spawn 面 |

**内存与 VM(5 项)**

| 测试 | 验证内容 | 依赖前提 |
|---|---|---|
| test6 | brk/sbrk | PM + VM |
| test44 | mmap 映射与读 | VM |
| test64 | mmap 跨 fork 继承 | VM + PM |
| test75 | mmap + getrusage + fork | VM + PM |
| test87 | mmap + setuid | VM + PM |

**块设备与 SysV IPC(2 项)**

| 测试 | 验证内容 | 依赖前提 |
|---|---|---|
| test85 | 块设备 I/O 末尾截断行为(`test85.c` 头注释:end-of-file during block device I/O) | 块设备栈 |
| test88 | System V 信号量(semget/semop)+ mmap | SysV IPC 面;当前边界外,是否实现是决策项 |

**网络(11 项,整体排最后)**

| 测试 | 验证内容 | 依赖前提 |
|---|---|---|
| test48 | IP 地址工具与网络信息 | INET |
| test56 | Unix 域套接字全套(`test56.c` 头注释) | UDS 服务器(C 树在 `minix/net/uds`) |
| test67 | TCP 客户端/服务端基本链 | INET |
| test80 | TCP(复用 test56 的用例函数) | INET |
| test81 | UDP(同上) | INET |
| test82 | 与远端服务器的 HTTP(需 `$USENETWORK=yes`,**依赖外网**,CI 不可用) | 外网,建议永久排除出自动跑 |
| test83 | 坏报文容忍 | INET |
| test90 | Unix 域套接字进阶(`test90.c` 头注释) | UDS |
| test91 | TCP/UDP 进阶(lwip) | INET |
| test92 | RAW 套接字(lwip) | INET |
| test93 | 网络接口与路由(lwip) | INET |
| test94 | BPF 设备(lwip) | BPF 面,边界决策项 |

**编译器/libc 自检类(7 项,不迁移但保留为记录)**

test3(库函数而非系统调用)、test9(setjmp 寄存器变量)、test45(strtol 族)、test49(整数大小/符号性/printf 格式)、test51(浮点打印)、test57(见进程组)、test66(64 位算术除法)。这批验证的是编译器与 libc 自身,与 OS 正确性无关——除非未来自建工具链或 libc 兼容层,届时它们自动恢复价值。

**决策驱动项(单独列出,定期回看)**

| 测试 | 提示的边界问题 |
|---|---|
| test42 | 要不要实现 ptrace?影响调试器、strace 类工具的可能性 |
| test59 | 要不要实现 mthread(用户态线程库,`minix/lib/libmthread`)?Rust 侧有原生线程模型,此项是"复刻 Minix 特有库还是借 Rust 生态"的决策 |
| test63 | 要不要支持运行时动态加载共享对象?牵动 `libexec/ld.elf_so` |
| test66/49/45 | 要不要自建工具链?(默认否) |
| test82 | 网络测试要不要接外网?(建议否,保持 CI 离线) |
| test88 | 要不要实现 SysV IPC 信号量?(POSIX 语义完整性 vs 使用频率) |
| test94 | 要不要 BPF?(抓包/防火墙的前置) |

### 7.4 子系统测试对照

| 子目录 | 测什么 | os/ 侧现状 |
|---|---|---|
| `kernel/sys_vumap` | sys_vumap 虚拟→物理映射全语义(`vumaptest.c`,驱动进程经 relay 测) | 对应 grant/vumap 面;适合做接缝级 Rust 对照 |
| `kernel/sys_padconf` | 引脚配置系统调用 | 平台面对应物就位后 |
| `ds/` | DS 全 API(U32/STR/MEM/LABEL,publish/subscribe/check) | `os/tests/ds_publish_subscribe.rs` 已覆盖宿主半的三链,`dstest.c` 可当用例清单核对漏项 |
| `blocktest/` | 块设备驱动通用语义 | 随块设备栈 |
| `ddekit/` | Linux 驱动移植层 | 边界外:Rust 重写引入 Linux C 驱动的动机与 C 树不同 |
| `testmfs.sh` 等脚本 | 镜像构建校验、shell/#! 行为 | 载体用户态成型后价值最高 |

## 8. 建议的迁移顺序

按依赖关系排序,不按当前实现快照排序——顺序表达的是"谁就位后谁解锁",不是时间承诺:

1. **立即可动**:Rust 腿翻译从文件系统语义区(test14–36)与 DS/MIB/vumap 子系统对照开始。这一区价值密度最高、接缝依赖最少(宿主上脚本 IPC + 服务器真代码即可驱动),且每完成一项就为对应服务提供真实回归防护。
2. **随服务点亮**:进程/信号区(test1/2/5/12/13/37/38)等 PM 信号面、exec 面成熟后接上;内存区(test6/44/64/75)等 VM 从契约态走向可执行态后接上。
3. **等载体用户态**:整体 C 腿(若 libc 兼容层立项)与脚本类测试(镜像校验、shell 行为)在真机用户态能跑之后接管端到端验收。
4. **边界决策项**:第 7.3 节决策表中的条目,每条在被认真考虑扩大边界时单独评估;网络区(test56/67/80/81/90–93)随 INET 点亮自然解锁,仅 test82(外网依赖)建议永久排除出自动执行。
