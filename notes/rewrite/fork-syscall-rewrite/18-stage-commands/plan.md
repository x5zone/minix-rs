# 18-stage-commands 文档重组计划（plan.md）

> **状态**: 生效中（2026-08-16 首版，深度 review + minix3 源码回归 review 后定稿）
> **范围**: `notes/rewrite/fork-syscall-rewrite/18-stage-commands/`
> **目标**: 以**用户系统交付因果链为主线**（boot → init → rc → 服务 → 登录 → shell → 命令使用）重组 commands 全部文档；命令**功能域语义分组**为次主线；最终覆盖 Minix3 命令与系统配置层全部语义（bin + sbin + usr.bin + usr.sbin + minix/commands + minix/usr.bin + games + /etc + getty/login），支撑 `os/commands/*`（35 crate）+ `os/etc/` 的彻底 Rust 重写
> **对照**: `01-stage-kernel/`（讲述结构参照）、`02-stage-vm/plan.md` + `14-stage-runtime/plan.md` + `15-stage-fs/plan.md` + `16-stage-drivers/plan.md` + `17-stage-net/plan.md`（plan 结构参照；14/15/16/17 为非 server 主线重定义先例）、`minix3/bin/` + `minix3/sbin/` + `minix3/usr.bin/` + `minix3/usr.sbin/` + `minix3/minix/commands/` + `minix3/minix/usr.bin/` + `minix3/games/` + `minix3/etc/` + `minix3/libexec/getty/`（ground truth）、`os/commands/*` + `os/etc/`（Rust 实现）

---

## 1. 背景与动机

### 1.1 与 VM 的差异：commands 不是 server，主线重新定义

`02-stage-vm` 以 **VM server 启动顺序为主线**——VM 是一个有明确 `init_vm()` 启动链 + 主循环的用户态服务。**18-stage-commands 不是 server，也不是一个子系统**：它覆盖 Minix3 用户可见的**命令/工具层与系统配置层**——328 个命令程序（30 bin + 19 sbin + 141 usr.bin + 25 usr.sbin + 81 minix/commands + 8 minix/usr.bin + 24 games）+ `/etc` 配置数据 + getty/login 登录链路，合计 **865 个 .c / 425,130 行**（`wc -l` 实测，含 `libexec/getty` 与 `usr.bin/login`）：

| 目录 | 命令数 | .c 数 | 行数 | 性质 |
|------|--------|-------|------|------|
| `bin/` | 30 | 143 | 89,623 | POSIX 基础工具 + shell 家族（sh/ksh/csh） |
| `sbin/` | 19 | 69 | 37,582 | 系统管理（init/mount/fsck/ifconfig/ping…） |
| `usr.bin/` | 141 | 338 | 146,364 | 用户工具（文本/进程/网络/文档/归档…） |
| `usr.sbin/` | 25 | 103 | 52,834 | 服务/数据库/系统管理（service/syslogd/pwd_mkdb…） |
| `minix/commands/` | 81 | 98 | 46,160 | Minix 特有命令（MAKEDEV/svrctl/loadkeys/pkgin_*…） |
| `minix/usr.bin/` | 8 | 36 | 20,161 | Minix 用户工具（grep/mined/diff/mtop…） |
| `games/` | 24 | 75 | 30,806 | 游戏与娱乐（stdio / 终端控制 / 文本三类） |
| `libexec/getty/` + `usr.bin/login/` | 2 | 7 | 3,912 | 登录链路 |
| `etc/` | 配置面 | — | — | rc 体系 + 口令/服务数据库 + 终端/启动配置 |

旧内容仅有占位 `README.md`（已移入 `draft/README.md`），其 scope 定义（games 24 / bin / sbin / service+svrctl / etc / /dev / 登录链路 / 终端库决策）保留为素材，本 plan 将其扩展为完整语义覆盖契约。

### 1.2 新主线：用户系统交付因果链（boot → 登录 → 交互）

与 `01-stage-kernel` 相同，采用**读者学习顺序 = 系统实际执行顺序**的组织原则。commands 层没有 server 启动链，但有清晰的**用户系统交付因果链**：内核完成引导后，系统从 INIT 一路交付到交互 shell，用户再按功能域使用命令：

```
内核引导完成（01-stage-kernel 覆盖）→ INIT（sbin/init，PID 1）
  │
  ▼  01：init 读 /etc/rc.conf → rc → rc.d/*（mountcritlocal/fsck/sysctl/network/ttys/LOGIN…）
  │   ├─ 挂载根/usr（mount + fstab）            ← 14
  │   ├─ 设备节点（MAKEDEV/mknod/dev_mkdb）      ← 04
  │   ├─ 网络配置（ifconfig/route/netconf）      ← 18
  │   ├─ 启动服务（service/svrctl → RS：cron/syslogd/inetd…） ← 02/19
  │   └─ 启动 getty（ttys 每行一个）             ← 03
  ▼  登录链路
  ├─ 03：getty（libexec/getty）→ login（usr.bin/login）
  │   ├─ 口令验证：passwd/master.passwd/pwd_mkdb/vipw/user
  │   └─ 会话环境：profile/skel → shell 启动文件
  ▼  05：shell（sh-ash 主线 / ksh / csh）→ 用户日常操作
  ├─ 06/07/08/10/11：文件与文本工具（POSIX 用户工具面）
  ├─ 12/13：进程与终端控制
  ├─ 14~17：文件系统/存储/备份管理
  ├─ 18/19：网络配置与服务
  ├─ 20/21：Minix 特有工具与包管理
  └─ 22~24：游戏与娱乐
```

**每篇文档必须能回答一个问题：它位于用户系统交付因果链的哪个位置（启动/服务/登录/交互/管理），以及属于哪个功能域语义组（文件/文本/进程/终端/存储/网络/系统/游戏）。** 这是从 `01-stage-kernel/00-kernel-overview.md §3` 继承的组织原则，由 `14-stage-runtime/plan.md §1.1`（非 server 主线重定义先例）迁移而来。

### 1.3 次主线：命令功能域语义分组

命令不是按目录（bin/sbin/usr.bin…）讲述，而是按**功能域语义模块**分组——同一组的命令共享核心语义（如"文件复制语义"跨 cp/rcp/xinstall，"正则匹配语义"跨 grep/sed/ed，"口令数据库语义"跨 passwd/vipw/pwd_mkdb/user）。POSIX 安装分层（bin/sbin/usr.bin/usr.sbin）只决定权限与 PATH，不决定语义归属：

```
功能域 ──► 文件操作(06) / 文本过滤(07) / 正则(08) / 编辑(09) / 文档(10) / 归档(11)
        ├─► 进程会话(12) / 终端(13)
        ├─► 存储：挂载检查(14) / 分区格式(15) / 镜像介质(16) / 备份维护(17)
        ├─► 网络：配置诊断(18) / 服务守护(19)
        ├─► 系统：Minix 特有(20) / 包管理(21)
        └─► 游戏：stdio(22) / 终端控制(23) / 文本(24)
```

每个命令的**行为契约**（选项/输入输出/退出码/错误面）在所属文档中以命令契约表逐项展开（§3.6），覆盖契约以 §5.2 全量命令归属表为准绳。

---

## 2. 新文档编号与阶段划分

### 阶段总览（24 篇 + 00 + 99 = 26 篇）

> 编号 = 交付因果链阅读顺序；与实施顺序（§6）解耦。

| 阶段 | 编号 | 文档 | 核心语义模块 | C 源码 | Rust 侧 | 状态 |
|------|------|------|-------------|--------|---------|------|
| 0 总览 | 00 | `00-commands-overview.md` | 总览、交付链路图、功能域矩阵、命令面全图、文档导航、设计原则 | — | — | 新建 |
| 1 交付链路 | 01 | `01-init-rc-scripts.md` | init（PID 1 启动链）、rc/rc.conf/rc.subr、rc.d 脚本面、rcorder、shutdown/reboot、setup、boot.cfg 面 | `sbin/init/`、`sbin/rcorder/`、`sbin/shutdown/`、`sbin/reboot/`、`etc/rc*`、`etc/rc.d/*`、`etc/defaults/`、`etc/boot.cfg.default` | `os/commands/sbin/init` | 新建 |
| 1 | 02 | `02-service-scheduler.md` | service/minix-service/svrctl（RS 客户端面）、调度：cron/crontab/at/atnormalize/update | `usr.sbin/service/`、`minix/commands/svrctl/`、`minix/commands/minix-service/`、`minix/commands/cron/`、`crontab/`、`at/`、`atnormalize/`、`update/`、`etc/crontab` | 新 crate（待建） | 新建 |
| 1 | 03 | `03-login-passwd.md` | getty、login、ttys/gettytab、口令数据库面（passwd/master.passwd/pwd_mkdb/vipw/user/chpass/su/newgrp/nologin/id/pwhash）、profile/skel | `libexec/getty/`、`usr.bin/login/`、`passwd/`、`usr.sbin/pwd_mkdb/`、`vipw/`、`user/`、`chpass/`、`su/`、`newgrp/`、`nologin/`、`id/`、`pwhash/`、`etc/gettytab`、`etc/ttys`、`etc/passwd.conf`、`etc/master.passwd`、`etc/skel` | 新 crate（待建） | 新建 |
| 1 | 04 | `04-device-database.md` | /dev 节点面（MAKEDEV/mknod/dev_mkdb/mtree）、系统数据库面（getent + group/shells/hosts/services/protocols/motd/utmp/nsswitch.conf） | `minix/commands/MAKEDEV/`、`sbin/mknod/`、`usr.sbin/dev_mkdb/`、`mtree/`、`getent/`、`etc/group`、`shells`、`hosts`、`services`、`protocols`、`motd`、`utmp`、`nsswitch.conf` | 新 crate（待建） | 新建 |
| 2 shell | 05 | `05-shell-family.md` | sh（ash）主线、ksh、csh、shell 启动文件（shrc/csh.cshrc/csh.login/csh.logout/profile）、环境面（env/printenv/sysenv/hostname/domainname/uname/machine/pagesize/getopt） | `bin/sh/`（ash）、`bin/ksh/`、`bin/csh/`、`bin/hostname/`、`bin/domainname/`、`usr.bin/env/`、`printenv/`、`getopt/`、`uname/`、`machine/`、`pagesize/`、`minix/commands/sysenv/`、`etc/hostname.file`、`etc/shrc`、`etc/csh.*`、`etc/profile` | `os/commands/bin/sh` | 新建 |
| 3 文件/文本 | 06 | `06-file-ops.md` | 文件操作面：cat/cp/mv/rm/ln/ls/mkdir/rmdir/chmod/chown/chroot/link/unlink/df/du/find/xargs/xinstall/mkfifo/mktemp/touch/truncate/stat/pathchk/test/expr/true/false/echo/pwd/sync/flock | `bin/` 15 个 + `usr.bin/` 15 个 + `usr.sbin/chroot,link,unlink` + `minix/commands/truncate` | `os/commands/bin/{cat,cp,echo,ls,mv,rm}` | 新建 |
| 3 | 07 | `07-text-filter.md` | 文本过滤与数据处理：head/tail/sort/tr/uniq/wc/cut/paste/join/comm/cmp/diff/sdiff/patch/col/colrm/expand/unexpand/fold/rev/split/csplit/tee/column/lam/jot/seq/shuffle/hexdump/look/ifdef/unifdef/vis/unvis/yes/cksum/uuidgen/units | `usr.bin/` 35 个 + `minix/commands/{look,ifdef,crc}` + `minix/usr.bin/diff` | 新 crate（待建） | 新建 |
| 3 | 08 | `08-grep-sed.md` | 正则语义面：grep 族（grep/egrep/fgrep 语义）+ sed + 正则表达式语法契约（BRE/ERE） | `minix/usr.bin/grep/`、`usr.bin/sed/` | 新 crate（待建） | 新建 |
| 3 | 09 | `09-editors.md` | 编辑器面：ed + mined；[ARCH] 编辑器选型决策（vi 面） | `bin/ed/`、`minix/usr.bin/mined/` | 新 crate（待建） | 新建 |
| 3 | 10 | `10-doc-man-tools.md` | 文档排版/man/开发辅助：pr/fmt/nl/colcrt/deroff/checknr/indent/cawf/spell/m4/soelim/gencat/lorder/mkstr/xstr/asa/fpr/fsplit/menuc/msgc/ctags/cal/what + man 面（man/apropos/whatis/whereis/makewhatis/man.conf）+ i18n 面（locale/mklocale/mkesdb/mkcsmapper，[ARCH] A-8 defer 候选） | `usr.bin/` 32 个 + `minix/commands/{cawf,spell,prep}` + `etc/man.conf` | 新 crate（待建） | 新建 |
| 3 | 11 | `11-compress-archive.md` | 压缩与归档：gzip/bzip2/bzip2recover/compress/unzip/pax/shar/uuencode/uudecode/bdes | `usr.bin/` 10 个 + `minix/commands/compress/` | 新 crate（待建） | 新建 |
| 4 进程/终端 | 12 | `12-process-tools.md` | 进程与用户会话：ps/kill/nice/renice/nohup/time/sleep/date/lock/leave/ipcs/ipcrm/mtop/ministat/toproto/finger/who/w/last/users/wall/write/mesg/tty/logname/logger/shlock/from | `usr.bin/` 21 个 + `minix/usr.bin/{ministat,mtop,toproto}` | 新 crate（待建） | 新建 |
| 4 | 13 | `13-terminal-termios.md` | 终端控制与数据库：stty/tput/tic/infocmp/term/termcap/tget/loadfont/loadkeys/screendump；termios 面 + [ARCH] A-2 terminfo/curses 决策 | `usr.bin/{stty,tput,tic,infocmp}` + `minix/commands/{term,termcap,tget,loadfont,loadkeys,screendump}` + `etc/fonts`、`etc/termcap*` | 新 crate（待建） | 新建 |
| 5 存储 | 14 | `14-mount-fsck.md` | 挂载与检查：mount/umount/fstab/newfstab.sh + fsck/fsck.mfs/fsck_ext2fs | `sbin/{mount,fsck,fsck_ext2fs}` + `minix/commands/{mount,umount,fsck.mfs}` + `etc/newfstab.sh` | `os/commands/sbin/fsck`、`os/commands/sbin/mkfs`（挂载面） | 新建 |
| 5 | 15 | `15-partition-format.md` | 分区与格式化：fdisk/part/partition/autopart/repartition/format/devsize + newfs_* 族（ext2fs/msdos/udf/v7fs）+ makefs/mkfs | `minix/commands/{fdisk,part,partition,autopart,repartition,format,devsize}` + `sbin/newfs_*` + `usr.sbin/makefs` | `os/commands/sbin/mkfs` | 新建 |
| 5 | 16 | `16-image-media.md` | 镜像与介质：writeisofs/isoread/dosread/vol/eject/cdprobe/ramdisk/loadramdisk/rawspeed/vnconfig/dd | `minix/commands/{writeisofs,isoread,dosread,vol,eject,cdprobe,ramdisk,loadramdisk,rawspeed}` + `usr.sbin/vnconfig` + `bin/dd` | 新 crate（待建） | 新建 |
| 5 | 17 | `17-backup-maintenance.md` | 备份与维护：backup/cleantmp/progressbar/remsync/synctree/update_asr/update_bootcfg/updateboot/rotate/fix/mt | `minix/commands/{backup,cleantmp,progressbar,remsync,synctree,update_asr,update_bootcfg,updateboot,rotate,fix,mt}` | 新 crate（待建） | 新建 |
| 6 网络 | 18 | `18-network-config.md` | 网络配置与诊断：ifconfig/route/netconf/arp/ndp/ping/ping6/traceroute/traceroute6/netstat/rdate/rtadvd/slip/swifi；hosts/services/protocols 使用面 | `sbin/{ifconfig,route,ping,ping6}` + `usr.sbin/{arp,ndp,traceroute,traceroute6,netstat,rdate,rtadvd}` + `minix/commands/{netconf,slip,swifi}` | 新 crate（待建） | 新建 |
| 6 | 19 | `19-network-services.md` | 网络服务与守护：inetd/ftpd/telnetd/rshd/fingerd/httpd/syslogd + 客户端 telnet/ftp/rsh/rcp/rcmd/whois/fetch/zmodem + mail/lp/lpd | `usr.sbin/{inetd,syslogd}` + `libexec/{ftpd,telnetd,rshd,fingerd,httpd}` + `usr.bin/{telnet,ftp,rsh,whois,mail}` + `bin/{rcmd,rcp}` + `minix/commands/{fetch,zmodem,lp,lpd,mail}` + `etc/{inetd.conf,syslog.conf,inet.conf}` | 新 crate（待建） | 新建 |
| 7 系统 | 20 | `20-minix-system.md` | Minix 特有与系统信息：version/readclock/lspci/intr/devsize/dhrystone/worldstone/sprofalyze/sprofdiff/srccrc/printroot/sysctl/eepromread/zdump/zic/profile/playwave/recwave + i2cscan + ldd（[ARCH] A-1） | `minix/commands/` 13 个 + `usr.sbin/{sysctl,zdump,zic,i2cscan}` + `usr.bin/ldd` + `minix/usr.bin/{eepromread,trace}` + `sbin/sysctl` + `etc/system.conf` | 新 crate（待建） | 新建 |
| 7 | 21 | `21-package-tools.md` | 包管理与构建工具：pkgin_all/pkgin_cd/pkgin_sets/postinstall/installboot/gcov-pull/mkdep/nbperf/genassym | `minix/commands/{pkgin_all,pkgin_cd,pkgin_sets}` + `usr.sbin/{postinstall,installboot}` + `minix/commands/gcov-pull` + `usr.bin/{mkdep,nbperf,genassym}` + `etc/mk.conf` | 新 crate（待建） | 新建 |
| 8 游戏 | 22 | `22-stdio-games.md` | 纯 stdio 游戏：factor/primes/bcd/morse/number/pig/arithmetic/caesar/banner/ppt | `games/{factor,primes,bcd,morse,number,pig,arithmetic,caesar,banner,ppt}` | `os/commands/games/` 10 个 | 新建 |
| 8 | 23 | `23-terminal-games.md` | 终端控制游戏（curses/转义序列）：worm/worms/rain/colorbars/tetris/snake/rogue | `games/{worm,worms,rain,colorbars,tetris,snake,rogue}` | `os/commands/games/` 7 个 | 新建 |
| 8 | 24 | `24-text-games.md` | 文本类游戏：adventure/monop/fortune/fish/wargames/wtf/random | `games/{adventure,monop,fortune,fish,wargames,wtf,random}` | `os/commands/games/` 7 个 | 新建 |
| 99 全局 | 99 | `99-global-concepts.md` | 命令安装面（bin/sbin/usr.bin/usr.sbin + PATH）、静态链接（A-1）、退出码/errno 约定、/etc 配置约定、命令契约模板、curses/terminfo 决策汇总、DESCRIBE 构建面 | — | — | 新建 |

### 2.1 阶段间的叙事衔接

每个阶段结尾的文档需含"过渡"小节（参照 `01-stage-kernel` 每篇 §6"过渡"），说明本阶段在交付因果链中的位置与下一阶段的入口：

```
00（总览）→ 01（init/rc）→ 02（服务/调度）→ 03（登录链路）→ 04（设备/数据库）
→ 05（shell：交付链终点，交互起点）
→ 06~11（文件/文本/编辑/文档/归档：日常操作主力）
→ 12/13（进程/终端：shell 之上的会话控制）
→ 14~17（存储：mount→分区→镜像→备份）
→ 18/19（网络：配置→服务）
→ 20/21（Minix 特有/包管理）
→ 22~24（游戏：交付链验收展示）
→ 99（全局概念）
```

---

## 3. 讲述结构规范（参照 01-stage-kernel / 02-stage-vm plan §3 / 14-stage-runtime plan §3）

### 3.1 每篇文档的章节模板

与 `01-stage-kernel` 一致（见 `01-stage-kernel/03-kmain-cstart.md` 等）：

1. **概念**——为什么需要这个功能域、类比、边界声明（前置依赖/本篇不覆盖什么）
2. **C 源码分析**——ground truth，必须 grep 实证，禁止凭记忆
3. **Rust 设计决策**——类型系统建模、与 C 的差异（含 ARCH 标注）
4. **实现详解**——Rust 模块结构、关键不变量
5. **测试要点**——相关测试函数 + 测试总数声明
6. **过渡**——在交付因果链/功能域中的位置，下一篇入口
7. **参见**——跨文档引用（新编号）

### 3.2 引用规则

- 各文档之间用新编号交叉引用（如 `06-file-ops.md §cp 语义`）
- 与 server/驱动文档交叉引用时用 `../NN-stage-*/NN-*.md`（如 `../11-stage-devman/`、`../16-stage-drivers/`、`../17-stage-net/`）
- 对 draft 素材的引用一律指向 `draft/README.md`，并标注"素材"
- 对 C 源码的引用使用绝对仓库路径 `minix3/...`（项目引用约定）

### 3.3 每篇文档的边界声明

> 写作每篇文档前，先按本表声明**前置依赖 / 本篇职责 / 不覆盖（移交）**，防止内容交叉与遗漏。边界以 §5.2 命令归属表为准绳。

| 编号 | 前置依赖 | 本篇职责（核心语义） | 不覆盖（移交） |
|------|---------|---------------------|--------------|
| 00 | 无 | 总览、交付链路图、功能域矩阵、文档导航、设计原则 | 一切命令细节 |
| 01 | 00 + kernel 启动完成 + 14-stage-runtime（exec/exit） | init 启动链、rc 脚本面、rcorder、shutdown/reboot | 服务管理协议（02）、getty 启动（03） |
| 02 | 01（rc 调用面）+ 03-stage-rs（RS 协议） | service/svrctl 客户端语义、调度（cron/at/update） | RS server 端实现（03-stage-rs） |
| 03 | 02 + 13（termios）+ 04-stage-pm（认证/权限） | getty/login 链路、口令数据库面、会话初始化 | 口令存储实现（PM）、shell 启动文件（05） |
| 04 | 00 + 11-stage-devman | MAKEDEV/mknod/dev_mkdb/mtree 命令面、系统数据库文件面 | devman server 实现（11-stage-devman） |
| 05 | 03（登录后入口）+ 13（termios） | sh/ksh/csh 语义、启动文件、环境命令 | 命令工具本体（06~24） |
| 06 | 05（shell 内建面）+ 14-stage-runtime（文件 syscall） | 文件操作命令行为契约、权限/链接/查找语义 | 文本处理（07）、存储管理（14~17） |
| 07 | 06 + 08（正则概念序前置） | 文本过滤/比较/数据转换命令契约 | 正则实现细节（08）、编辑器（09） |
| 08 | 07 | 正则语法契约（BRE/ERE）、grep/sed 行为 | 编辑器中的正则使用（09） |
| 09 | 08 | ed/mined 编辑器语义、[ARCH] 编辑器选型 | shell 行编辑（05）、终端控制（13） |
| 10 | 07/08 | 排版/man/开发辅助命令契约、i18n defer 决策 | 文档内容生成（游戏数据，24） |
| 11 | 06 | 压缩/归档/编解码命令契约 | 文本压缩算法库实现（14-stage-runtime 或独立库） |
| 12 | 05 | 进程/会话/用户信息命令契约 | 终端控制（13）、网络会话（19） |
| 13 | 12 + 14-stage-runtime（termios ABI） | 终端控制命令、terminfo/curses 决策 | 终端驱动（16-stage-drivers） |
| 14 | 06 + 15-stage-fs（FS 服务端语义） | mount/umount/fstab 命令面、fsck 族 | FS server 实现（15-stage-fs）、块驱动（16） |
| 15 | 14 | 分区/格式化命令契约 | 分区格式解析库（16-stage-drivers 或独立库） |
| 16 | 14 | 镜像/介质命令契约、dd 语义 | 驱动实现（16-stage-drivers） |
| 17 | 14/15 | 备份/同步/引导更新命令契约 | 备份格式设计（本 stage 决策） |
| 18 | 13 + 17-stage-net（socket ABI） | 网络配置/诊断命令契约 | lwip/uds 实现（17-stage-net） |
| 19 | 18 | 守护进程/网络客户端/邮件打印命令契约 | 各守护进程协议实现（17-stage-net 或库层） |
| 20 | 00 | Minix 特有/系统信息命令契约、sysctl 面、ldd 决策 | 硬件信息获取（16-stage-drivers 接口） |
| 21 | 02（服务管理面） | 包管理/构建工具命令契约 | 构建链（00-master-plan 外） |
| 22 | 06（stdio 面） | 纯 stdio 游戏行为契约 | 终端控制（23） |
| 23 | 13（termios/curses） | 终端控制游戏行为契约 | curses 库实现（13/99 决策） |
| 24 | 06/10 | 文本游戏行为契约 + 文本数据文件面 | 数据文件格式（本 stage 决策） |
| 99 | 全部 | 安装面/PATH/静态链接/退出码/errno/配置约定/命令契约模板 | 一切机制细节 |

### 3.4 测试基线（截至 2026-08-16）

- `os/commands/*` 全部为 stub（`exit(0)` 占位），`cargo test` 无命令行为测试
- 每篇改写完成时在该命令 crate 内补充行为测试（输入/输出/退出码/错误面），文末更新测试统计
- 命令级验收基线：`cargo test -p commands-*` 各 crate 独立可跑

### 3.5 Review gate 要求（每篇改写必检）

- 每篇新文档创建时同步生成 `.design/{NN}-outline.md` + `{NN}-outline-review.md` + `{NN}-design.md`（Step 0.3 嵌入生成，Gate H.6，不允许 N/A）
- 每篇改写后按 review 工作流跑 Blocker Gates（0/A/B/C/D/D-6/E/G/H），scan 产物写入 `.review/codex/18-stage-commands/{NN}-{name}/`
- P0 未清不得标完成；doc 与 code 保持同步

### 3.6 命令组文档写作原则（组内逐命令行为契约）

命令组文档（06~24）以**组内共享语义**为主线，逐命令以契约表展开：

```
## <功能域> 共享语义（概念 + C 源码分析 + Rust 设计决策）
## 命令契约表
| 命令 | C 源 | 职责 | 关键选项 | 输入/输出 | 退出码 | 错误面 | Rust 模块 |
## 实现详解 / 测试要点 / 过渡 / 参见
```

- 契约表必须覆盖组内**全部命令**（以 §5.2 归属表核对），不允许"典型命令详述、其余略过"
- 纯 stdio 类命令（22 等）只依赖 `exec + stdio + exit`，最早可验收（§6 批 1）
- 终端控制类命令（13/23）依赖 termios/转义序列落地，先定 [ARCH] A-2 再实装

---

## 4. 架构演进（ARCH）清单

> 按 review-core 要求，ARCH 必须三处一致标注：Minix3 行为对照点 + design doc + 代码注释。**commands crate 当前为 stub，以下为设计期候选 ARCH 项**，写文档时必须逐项确认/更新状态；minix-rs 侧已实现的 ARCH（如 02-stage-vm 的 Direct Map）不属于本 stage。

| # | ARCH 项 | Minix3 现状 | minix-rs 演进（候选） | 涉及文档 | 状态 |
|---|---------|------------|---------------------|---------|------|
| A-1 | 用户态静态链接 | `libexec/ld.elf_so` 动态链接 + `ldd` | 静态链接（14-stage-runtime 已定 [ARCH] 决策）；`ldd` 语义变为静态归档查看；`ld.elf_so` 不实现 | 99/20/05 | 已定（14 决策） |
| A-2 | 终端数据库 | `lib/libterminfo` + termcap 兼容 + `/usr/share/terminfo`；6 个游戏链接 `-lterminfo`（games/*/Makefile 实测：worm/worms/tetris/rogue/colorbars/rain） | **重大决策**：移植 terminfo 数据+解析器 vs 仅转义序列封装；决定 23 与 13 的实现面 | 13/23/99 | 设计期（重大决策） |
| A-3 | 口令数据库 | `master.passwd` + `pwd_mkdb`（Berkeley DB）+ `vipw` | 文件面 passwd/master.passwd + Rust 认证 API；DB 面由 PM/IS 决定 | 03 | 设计期 |
| A-4 | 命令参数框架 | getopt + usage + man page（C 传统） | Rust 轻量自研 argparse（避免 clap 依赖面）+ `--help`/usage 约定；man page 面（A-11） | 99/06~21 | 设计期 |
| A-5 | errno/退出码 | C `errno` 全局 + `exit(EXIT_FAILURE)` | `Result` + errno 映射（14-stage-runtime minix-sys）；命令退出码保持 POSIX 语义 | 99/全部命令文档 | 设计期 |
| A-6 | rc 配置面 | shell 脚本 rc.subr 函数 + rc.conf 变量 + rcorder | **重大决策**：保留 shell rc 脚本（依赖 05 shell 就绪）vs 编译期静态 rc（Rust 数据面） | 01/05 | 设计期（重大决策） |
| A-7 | 网络命令 socket 面 | raw socket（ping/traceroute）+ ioctl（ifconfig/route） | 经 17-stage-net libc socket 封装；ioctl 面需 termios/net ABI 落地 | 18/19 | 设计期（依赖 17） |
| A-8 | 多字节/国际化 | `mklocale/mkesdb/mkcsmapper` + locale 面 | x86-64 UTF-8 简化：i18n 工具 defer 候选（locale/colcrt 语义收敛） | 10/99 | defer 候选 |
| A-9 | 64 位/大文件 | `off_t` 32/64 混合（dd/df/du/stat） | `u64` 全程（继承 15-stage-fs A-4） | 06/14/16/99 | 设计期 |
| A-10 | 游戏移植策略 | C 源直接移植（adventure/monop/fortune 大文本数据文件） | **重大决策**：语义重写 vs 数据文件迁移（文本数据归 Rust assets） | 22/24/99 | 设计期（重大决策） |
| A-11 | man page 面 | `minix3/man/` + man.conf + makewhatis | minix-rs 是否携带 man 数据（体积决策）vs `--help` 自描述 | 10/99 | defer 候选 |
| A-12 | 权限命令 | su/newgrp/passwd setuid root | 依赖 04-stage-pm 权限面 + IS；setuid 语义 | 03 | 设计期（依赖 04） |
| A-13 | 音频工具 | playwave/recwave（`/dev/audio`） | 依赖 16-stage-drivers audio；defer 候选 | 20 | defer 候选 |
| A-14 | /dev 动态节点 | MAKEDEV 静态 + devmand 动态 | devman（11）动态 + MAKEDEV 兜底（draft/README.md 已列） | 04 | 设计期 |

---

## 5. 覆盖完整性核对（对照 minix3 源码）

> 本节是 plan.md 的语义覆盖契约。所有断言以 grep 实证为准（review-core：禁止凭记忆）。逐项核对见 §7。

### 5.1 C 源目录 → 新文档映射（.c 全量）

| C 源目录 | .c 数 | 行数 | 覆盖文档 | 核对 |
|----------|-------|------|---------|------|
| `minix3/bin/` | 143 | 89,623 | 05、06、09、11、12、13、16、19 | 已核对（§7.2 F2/F10） |
| `minix3/sbin/` | 69 | 37,582 | 01、03、04、06、14、15、18、20 | 已核对 |
| `minix3/usr.bin/` | 338 | 146,364 | 03~13、18~21 | 已核对（§7.2 F3） |
| `minix3/usr.sbin/` | 103 | 52,834 | 02、03、04、06、15、16、18、19、20、21 | 已核对（§7.2 F10） |
| `minix3/minix/commands/` | 98 | 46,160 | 01~21、99（81 命令全部分配；devmand 排除至 11-stage-devman） | 已核对（§7.2 F4/F10） |
| `minix3/minix/usr.bin/` | 36 | 20,161 | 07、08、09、12、20 | 已核对 |
| `minix3/games/` | 75 | 30,806 | 22、23、24 | 已核对（§7.2 F5） |
| `minix3/libexec/getty/` | 3 | 1,600 | 03 | 已核对 |
| `minix3/usr.bin/login/` | 4 | 2,312 | 03 | 已核对 |
| **合计** | **865** | **425,130** | 26 篇 | 0 遗漏 |

### 5.2 命令归属表（全量命令 → 文档）

> 每行 = 目录内全部命令的完整分配。归属表是 §3.6 命令契约表的核对基准。

**`bin/`（30 命令）**

| 文档 | 命令 |
|------|------|
| 05 | sh、ksh、csh、hostname、domainname |
| 06 | cat、chmod、cp、df、echo、expr、ln、ls、mkdir、mv、pwd、rm、rmdir、sync、test |
| 09 | ed |
| 11 | pax |
| 12 | date、kill、ps、sleep |
| 13 | stty |
| 16 | dd |
| 19 | rcmd、rcp |

**`sbin/`（19 命令）**

| 文档 | 命令 |
|------|------|
| 01 | init、rcorder、reboot、shutdown |
| 03 | nologin |
| 04 | mknod |
| 06 | chown |
| 14 | fsck、fsck_ext2fs、mount |
| 15 | newfs_ext2fs、newfs_msdos、newfs_udf、newfs_v7fs |
| 18 | ifconfig、ping、ping6、route |
| 20 | sysctl |

**`usr.bin/`（141 命令）**

| 文档 | 命令 |
|------|------|
| 03 | chpass、id、login、passwd、pwhash、su、newgrp |
| 04 | getent |
| 05 | env、getopt、machine、pagesize、printenv、uname |
| 06 | basename、dirname、du、false、find、flock、mkfifo、mktemp、pathchk、printf、stat、touch、true、xargs、xinstall |
| 07 | cksum、cmp、col、colrm、column、comm、csplit、cut、expand、fold、head、hexdump、join、jot、lam、paste、patch、rev、sdiff、seq、shuffle、sort、split、tail、tee、tr、unexpand、unifdef、uniq、units、unvis、uuidgen、vis、wc、yes |
| 08 | sed |
| 10 | apropos、asa、cal、calendar、checknr、colcrt、ctags、deroff、fmt、fpr、fsplit、gencat、indent、locale、lorder、m4、man、menuc、mkcsmapper、mkesdb、mklocale、mkstr、msgc、nl、pr、soelim、tsort、ul、what、whatis、whereis、xstr |
| 11 | bdes、bzip2、bzip2recover、gzip、shar、unzip、uudecode、uuencode |
| 12 | finger、from、ipcrm、ipcs、last、leave、lock、logger、logname、mesg、nice、nohup、renice、shlock、time、tty、users、w、wall、who、write |
| 13 | infocmp、tic、tput |
| 18 | netstat |
| 19 | ftp、mail、rsh、telnet、whois |
| 20 | ldd |
| 21 | genassym、make、mkdep、nbperf |
| 22 | banner |

**`usr.sbin/`（25 命令）**

| 文档 | 命令 |
|------|------|
| 02 | service |
| 03 | pwd_mkdb、user、vipw |
| 04 | dev_mkdb、mtree、services_mkdb |
| 06 | chroot、link、unlink |
| 15 | makefs |
| 16 | vnconfig |
| 18 | arp、ndp、rdate、rtadvd、traceroute、traceroute6 |
| 19 | inetd、syslogd |
| 20 | i2cscan、zdump、zic |
| 21 | installboot、postinstall |

**`minix/commands/`（81 命令）**

| 文档 | 命令 |
|------|------|
| 01 | setup |
| 02 | at、atnormalize、cron、crontab、minix-service、svrctl、update |
| 04 | MAKEDEV |
| 05 | sysenv |
| 06 | truncate |
| 07 | crc、ifdef、look |
| 10 | cawf、prep、spell |
| 11 | compress |
| 13 | loadfont、loadkeys、screendump、term、termcap、tget |
| 14 | fsck.mfs、mount、umount |
| 15 | autopart、devsize、fdisk、format、part、partition、repartition |
| 16 | cdprobe、dosread、eject、isoread、loadramdisk、ramdisk、rawspeed、vol、writeisofs |
| 17 | backup、cleantmp、fix、mt、progressbar、remsync、rotate、synctree、update_asr、update_bootcfg、updateboot |
| 18 | netconf、slip、swifi |
| 19 | fetch、lp、lpd、mail、zmodem |
| 20 | dhrystone、intr、lspci、playwave、printroot、profile、readclock、recwave、sprofalyze、sprofdiff、srccrc、version、worldstone |
| 21 | gcov-pull、pkgin_all、pkgin_cd、pkgin_sets |
| 99 | DESCRIBE（构建面） |
| 排除 | devmand（→ 11-stage-devman） |

**`minix/usr.bin/`（8 命令）**

| 文档 | 命令 |
|------|------|
| 07 | diff |
| 08 | grep |
| 09 | mined |
| 12 | ministat、mtop、toproto |
| 20 | eepromread、trace |

**`games/`（24 游戏）**

| 文档 | 游戏 |
|------|------|
| 22 | arithmetic、banner、bcd、caesar、factor、morse、number、pig、ppt、primes |
| 23 | colorbars、rain、rogue、snake、tetris、worm、worms |
| 24 | adventure、fish、fortune、monop、random、wargames、wtf |

**登录链路**

| 文档 | 内容 |
|------|------|
| 03 | `libexec/getty/`（3 .c）+ `usr.bin/login/`（4 .c）+ `etc/ttys` + `etc/gettytab` |

### 5.3 配置文件面映射（`etc/` 全量）

> `minix3/etc/`（51 项，排除 Makefile/Makefile.params 后 49 项）逐项分配：

| 文档 | 配置文件 |
|------|---------|
| 01 | boot.cfg.default、rc、rc.capes、rc.cd、rc.conf、rc.d/*（32 脚本）、rc.minix、rc.shutdown、rc.subr、defaults/*（minix.rc.conf/rc.conf） |
| 02 | crontab、rs.lwip、rs.single |
| 03 | gettytab、master.passwd、passwd.conf、skel、ttys |
| 04 | group、hosts、motd、mtree、nsswitch.conf、protocols、services、shells、usr |
| 05 | csh.cshrc、csh.login、csh.logout、hostname.file、profile、shrc |
| 10 | man.conf |
| 12 | utmp |
| 13 | fonts、termcap、termcap.big |
| 14 | newfstab.sh |
| 19 | inet.conf、inetd.conf、named.conf、namedb、syslog.conf |
| 20 | system.conf |
| 21 | mk.conf |
| 排除 | devmand（→ 11-stage-devman）、xorg.conf（图形栈 defer）、root（home 模板，归 03 skel 面） |

### 5.4 排除表（非本 stage 语义）

| 内容 | 归属 | 说明 |
|------|------|------|
| devman server 实现 | 11-stage-devman | 本 stage 只覆盖 MAKEDEV/mknod/dev_mkdb 用户命令面 |
| RS 服务管理 server 端 | 03-stage-rs | 本 stage 只覆盖 service/svrctl 客户端命令面 |
| 驱动实现（tty/readclock/audio/网络等） | 16-stage-drivers | 命令只经 `/dev` 接口消费 |
| ld.elf_so / 动态链接 | [ARCH] A-1 | 静态链接，不实现 |
| libc/libminc/libsys 实现 | 14-stage-runtime | 命令只消费 exec/stdio/termios/socket API |
| lwip/uds 网络协议实现 | 17-stage-net | 命令只消费 libc socket 封装 |
| 构建工具链（gcc/binutils/make 宿主面、tools/awk、tools/sed、gnu/dist、tests/） | 构建链 | 本 stage 只覆盖系统内命令 |
| xorg/图形栈 | defer | 无图形子系统；xorg.conf 不覆盖 |
| FS 服务端挂载/检查实现 | 15-stage-fs | mount/fsck 命令面在本 stage，FS 内部语义归 15 |
| devmand 启动配置（etc/devmand） | 11-stage-devman | 配置数据归 devman stage |

---

## 6. 实施顺序（自下而上，每批可独立验收）

> 阅读顺序 = §2 阶段序；实施顺序按依赖与验收粒度重排。命令实装依赖 14-stage-runtime（exec/stdio/termios）与 17-stage-net（socket）先行。

1. **stdio 批**：06（file-ops）→ 07（text-filter）→ 08（grep-sed）→ 22（stdio 游戏）——只需 `exec + stdio + exit`，最早可验收
2. **shell 批**：05（sh 主线，依赖 13 termios 决策）→ 09（ed/mined）
3. **文档/归档/进程批**：10 → 11 → 12（依赖 06/07/08）
4. **终端批**：13（termios/terminfo 决策先行，[ARCH] A-2）→ 23（终端游戏）
5. **存储批**：14（mount/fsck）→ 15（分区/格式化）→ 16（镜像/介质）→ 17（备份/维护）
6. **网络批**：18（配置/诊断）→ 19（服务/守护，依赖 17-stage-net）
7. **系统批**：20（Minix 特有）→ 21（包管理）→ 24（文本游戏）
8. **交付链收尾**：04（设备/数据库）→ 02（服务/调度）→ 01（init/rc）→ 03（登录链路，依赖 PM/VFS/终端全部就绪）
9. **定稿**：00 总览 + 99 全局概念 + checklist.md 更新

---

## 7. Review 记录

### 7.1 深度 review（语义全覆盖 + 模块拆分合理性）

> 首轮深度 review 结论（2026-08-16）：
> - **覆盖结论**：865 个 .c / 425,130 行全量映射（§5.1），328 个命令全量归属（§5.2），`etc/` 49 项配置全量分配（§5.3），0 遗漏。
> - **拆分结论**：26 篇（00 + 24 + 99），按「功能域语义模块」拆分（§1.3），每篇一个语义单元；拆分原则继承 02-stage-vm plan §3.2（概念首次出现即完整解释 / 禁止前向引用 / 每篇一个语义单元 / 位置可回答性）。
> - **结构修正 R1**：登录链路（getty/login）从"命令批"提为交付链第 03 篇（因果链：服务启动 → 登录 → shell）；`devmand` 排除至 11-stage-devman（命令面与 server 面分离）；games 按依赖面拆三篇（stdio/终端/文本），与 draft/README.md 的三分类一致。
> - **结构修正 R2**：网络拆两篇（18 配置诊断 / 19 服务守护），客户端命令（ftp/telnet/rsh）与守护进程（inetd/ftpd/telnetd）同组以保持"服务面"语义完整。
> - P0/P1/P2 全部闭环后本 plan 方可进入实施。

### 7.2 minix3 源码回归 review

> 逐目录 grep 实证（2026-08-16），见 §5 核对列；发现遗漏/错配时在此记录并回填 §5。

**首轮回归 review 事实修正（已回填 §5/§2）：**

| # | 修正项 | 原值 | 实证值（grep/wc） |
|---|--------|------|------------------|
| F1 | `ed` 位置 | 假设 `usr.bin/ed/` | 实际 `bin/ed/`（`ls bin/` 实测）→ 09 的 C 源改为 `bin/ed/` |
| F2 | `bin/` 命令数 | 30 | 实测 30（`ls -d bin/*/`），§5.2 全部分配 |
| F3 | `usr.bin/` 命令数 | 141 | 实测 141（`ls -d usr.bin/*/`），§5.2 全部分配 |
| F4 | `minix/commands/` 命令数 | 81 | 实测 81（`ls -d minix/commands/*/`），§5.2 全部分配；DESCRIBE 为构建面目录（DESCRIBE.sh） |
| F5 | games 数 | 24 | 实测 24（`ls games/` 去除 Makefile*），§5.2 全部分配 |
| F6 | `libexec/getty` + `login` 行数 | — | getty 3 .c / 1,600 行、login 4 .c / 2,312 行（`wc -l`）→ §5.1 |
| F7 | 总 .c 数 | — | 865 .c / 425,130 行（`find` + `wc -l` 实测）→ §5.1 |
| F8 | curses 依赖游戏 | 假设 4 个 | 实测 6 个（`rg -l 'curses|terminfo' games/*/Makefile` → worm/worms/tetris/rogue/colorbars/rain）→ §4 A-2 |
| F9 | `minix/usr.bin` 命令名 | 假设 `toprolo` | 实测 `toproto`（`ls minix/usr.bin/`）→ §5.2/§2 已改 |
| F10 | 命令归属遗漏/错配 | 首轮 §5.2 比对 | 漏 df/hostname/domainname/rcmd/rcp（bin）、head/tail/printf/true/false/colcrt/ldd/netstat（usr.bin）、i2cscan（usr.sbin）、sysenv（minix/commands）；误把 true/false 归 bin、truncate 归 usr.bin、netstat 归 usr.sbin → §5.2 已回填，脚本比对 328/328 通过 |
| F11 | `etc/` 面 | 假设 45 项、rc.d 34 脚本 | 实测 49 项（`ls etc/` 排除 Makefile*）、rc.d 32 脚本（`ls etc/rc.d/`）→ §5.3 已改 |

**自动化覆盖检查**：`ls -d` 全目录比对（bin/sbin/usr.bin/usr.sbin/minix/commands/minix/usr.bin/games），§5.2 归属表 **328/328 命令无遗漏、无越界**（脚本比对通过；同名跨目录命令如 mount/mail/banner/login/truncate 为真实双目录存在，非重复分配）。

### 7.3 自检清单

- [ ] 9 个 C 源目录全量映射（§5.1 无遗漏）
- [ ] 328 个命令全部归属（§5.2 无遗漏、无重复）
- [ ] `etc/` 45 项配置全部分配（§5.3 无遗漏）
- [ ] 每个语义模块恰好一篇，无内容交叉（§3.3 边界表）
- [ ] ARCH 项全部标注三处一致（§4）
- [ ] 排除表明确（§5.4），无越界

---

## 8. 参见

- `01-stage-kernel/`——讲述结构模板（章节模板/组织原则）
- `02-stage-vm/plan.md`——plan 结构参照（§3.1 章节模板、§3.2 组织原则、§3.4 边界表）
- `14-stage-runtime/plan.md`——非 server 主线重定义先例
- `15-stage-fs/plan.md`、`16-stage-drivers/plan.md`、`17-stage-net/plan.md`——多组件 stage plan 先例
- `draft/README.md`——旧占位素材（scope 定义保留为素材）
- `os/commands/*`、`os/etc/`——Rust 实现侧现状
- `minix3/bin/`、`minix3/sbin/`、`minix3/usr.bin/`、`minix3/usr.sbin/`、`minix3/minix/commands/`、`minix3/minix/usr.bin/`、`minix3/games/`、`minix3/etc/`、`minix3/libexec/getty/`——ground truth
