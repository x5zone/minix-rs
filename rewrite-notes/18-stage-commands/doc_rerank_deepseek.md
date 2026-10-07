# 18-stage-commands 文档重建蓝图（deepseek）

```text
your_name(AI agent name) = deepseek
target_dir(关注的工作目录) = 18-stage-commands
repo_root(仓库根目录) = /home/xzhao/github/minix-rs
报告日期 = 2026-09-19
当前提交号 = 2d9d1f0aa32da37b0b0761a2d79d32010ada5d5c（分支 rewrite，2026-09-19）
任务 = R 相·重建蓝图：输出 target_dir/doc_rerank_deepseek.md，不改任何正文
约束 = 不引用项目规范列为中间产物的两个目录（本报告全部事实只锚到 C 源码、Rust 代码与非 C 制品）；未读取任何其它 AI 的 doc_rerank_* 产物
本轮修订 = 2026-09-19（补做轮）：把 §2 池里最后 10 条无 §5 引用的知识点逐条落位——K-013 进契约 03、K-429/K-430 进契约 09、K-105/K-174/K-191/K-211 进契约 99 账本；K-218/K-318 的去向改指 §3.2 GAP-7（原写"§3.1 缺口表"，节号有误）；K-388（旧 99 的成稿进度 stub）补入 §6 明确删除表并给理由。
```

---

## 0. 元数据

### 0.1 审查范围

**范围内（本报告的处理对象）**

1. `18-stage-commands/` 下的编号文档 26 篇：`00-commands-overview.md`、`01-init-rc-scripts.md` 到 `24-text-games.md`、`99-global-concepts.md`。
2. 本报告视为"参考材料"（不参与新目录编号，但参与知识来源）：`plan.md`（覆盖契约与实施顺序）、`todo.md`（实施规格缺口与批次进度）、`draft/README.md`（旧占位素材）。
3. 本 stage 覆盖的 Minix3 C 源：`minix3/bin/`、`sbin/`、`usr.bin/`、`usr.sbin/`、`minix/commands/`、`minix/usr.bin/`、`games/`、`libexec/getty/`、`etc/`，以及被这些命令消费的 `include/`、`lib/csu/`、`lib/libc/gen/` 中的相关文件。
4. 非 C 制品：各命令目录的 `Makefile` 与 `Makefile.inc`（`BINDIR`/`SUBDIR`/`HIDEGAME` 登记）、349 个手册页、`etc/` 下的配置数据文件、`yacc`/`lex` 文法源、游戏数据文件、`minix/commands/DESCRIBE/DESCRIBE.sh`。

**范围外（只做边界声明与交叉引用，不在本蓝图设计之内）**

| 范围外对象 | 归属 | 说明 |
|---|---|---|
| `minix3/devmand/`（server 本体） | `11-stage-devman` | 本 stage 只覆盖 `MAKEDEV`/`mknod`/`dev_mkdb`/`devmand` 命令面 |
| 重生服务（RS）服务端 | `03-stage-rs` | 本 stage 只覆盖 `service`/`minix-service`/`svrctl` 客户端面 |
| 各驱动实现（tty、readclock、audio、网卡、块设备） | `16-stage-drivers` | 命令只经 `/dev` 与 ioctl 消费 |
| libc / libminc / libsys 实现、`crt0` 运行时装载 | `14-stage-runtime` | 命令只消费 `minix-sys` 顶层封装与 `minix-rt` |
| lwip / uds / socket 协议栈 | `17-stage-net` | 命令只消费 socket 封装 |
| 文件系统服务端（VFS 回调、MFS/ext2 内部、inode/块布局） | `05-stage-vfs`、`15-stage-fs` | 本 stage 只覆盖挂载/检查/格式化命令面 |
| 构建工具链的宿主面（gcc/binutils/make、`tools/awk`、`gnu/dist`） | 构建链 | 本 stage 只覆盖系统内命令 |
| 图形栈（`xorg.conf`） | 延期，无归属 | 项目无图形子系统 |
| 同目录其它 AI 的重建产物 | 不读、不引 | 按任务规则 |

**范围外发现（末尾发现、只登记不处理）**

- `minix3/tests/` 下的命令测试基建（NetBSD 移植的 ATF 测试树）在本 stage 的 plan 与文档中都没有归属，见 §3.5 第 7 项。
- `minix3/releasetools/release.sh` 与 `distrib/sets/`（发行集合清单：哪些命令进哪个集合）没有任何一篇文档覆盖，见 §3.5 第 2 项。

### 0.2 读取清单

**文档（26 篇全部读完；头部声明全读，正文按篇精读或由独立读篇作业逐行读完并回填锚点）**

`00`、`01`–`24`、`99` 全部读毕。`plan.md`（472 行）与 `todo.md`（274 行）通读。

**C 源码（按目录全量清点，按主题精读）**

- 交付链：`sbin/init/init.c`（1902 行，逐状态读完）、`sbin/rcorder/rcorder.c`、`sbin/shutdown/`、`sbin/reboot/`、`etc/rc`、`etc/rc.subr`、`etc/rc.conf`、`etc/rc.minix`、`etc/rc.cd`、`etc/rc.d/`（32 个脚本逐个读头部关键字）、`etc/defaults/`、`etc/boot.cfg.default`、`bin/sh/main.c`、`bin/sh/options.c`。
- 登录链：`libexec/getty/{main.c,init.c,subr.c}`、`usr.bin/login/login.c`、`usr.bin/login/common.c`、`etc/ttys`、`etc/gettytab`。
- 命令统一框架：`lib/csu/common/crt0-common.c`、`lib/csu/arch/x86_64/crt0.S`、`lib/libc/stdlib/getopt.c`、`include/getopt.h`、`include/paths.h`。
- 命令面：`bin/`、`sbin/`、`usr.bin/`、`usr.sbin/`、`minix/commands/`、`minix/usr.bin/`、`games/` 七个目录的 `ls -d` 全量清点与 `find -name '*.c'` 计数；重点命令逐行读（`test.c`、`chmod.c`、`ls.c`、`cp.c`、`find.c`、`uniq.c`、`cut.c`、`tr.c`、`sort.c`、`grep.c`、`sed/compile.c`、`sed/process.c`、`ed/main.c`、`man.c`、`cal.c`、`compress.c`、`dd/args.c`、`isoread.c`、`fsck.c`、`mount.c`、`part.c`、`fdisk.c`、`backup.c`、`remsync.c`、`synctree.c`、`cleantmp.c`、`progressbar.c`、`mt.c`、`fix.c`、`ifconfig.c`、`route.c`、`ping.c`、`traceroute.c`、`inetd.c`、`syslogd.c`、`fetch.c`、`ftpd/`、`MAKEDEV.sh`、`mknod/pack_dev.c`、`dev_mkdb.c`、`getent.c`、`cron/tab.c`、`at.c`、`update.c`、`readclock.c`、`intr.c`、`sysctl.c`、`ldd.c`、`installboot.c`、`stty/{cchar,modes,print}.c`、`keyword.c`、`utmp.h`、`signal.h`、`bootblock.h`、`expr.y`、`expr/` 等）。
- `games/` 的 10 个 stdio 游戏、7 个终端游戏、7 个文本游戏按分析需要精读。

**非 C 制品（逐项确认实际位置）**

| 类别 | 实测位置与数量 |
|---|---|
| 安装分层 | `bin/Makefile.inc:7`（`BINDIR?= /bin`）、`sbin/Makefile.inc:7`（`/sbin`）、`usr.bin/Makefile.inc:5`（`/usr/bin`）、`usr.sbin/Makefile.inc:4`（`/usr/sbin`）、`games/Makefile.inc:7`（`/usr/games/hide`）、`games/Makefile.inc:17`（`/usr/games`） |
| 命令登记清单 | `usr.bin/Makefile`（`SUBDIR=` 全量）、`minix/commands/Makefile`、各目录 `Makefile` |
| 默认查找路径 | `include/paths.h:45`（`_PATH_DEFPATH "/usr/bin:/bin:/usr/pkg/bin:/usr/local/bin"`） |
| 手册页 | 命令目录下 349 个 `*.[1-9]` 手册页；`etc/man.conf` 配置面 |
| 处理程序启动脚本 | `sbin/init` 的 `pathnames.h`、`libexec/getty/pathnames.h` |
| 构建面工具 | `minix/commands/DESCRIBE/DESCRIBE.sh`（设备清单生成）、各命令目录的 `.y`/`.l` 文法源（`bin/sh/arith.y`、`bin/expr/expr.y`、`usr.bin/m4/parser.y`、`usr.bin/menuc/parse.y`、`usr.bin/msgc/msgparse.y`、`usr.bin/mklocale/yacc.y`、`usr.bin/mkesdb/yacc.y`、`usr.bin/mkcsmapper/yacc.y`） |
| 游戏数据 | `games/adventure/glorkz`（剧本）、`games/fortune/datfiles/`、`games/wtf/wtf`（词条文件）、`games/banner`（字形位图内嵌） |
| 配置数据 | `etc/` 51 项（排除两个 Makefile 后 49 项）、`etc/rc.d/` 32 个脚本 + `Makefile` |
| 命令侧 Rust 实现 | `os/commands/` 24 个 crate（227 个 `.rs` / 41,981 行 / 990 个 `#[test]`）、`os/etc/README.md`（占位）、`tools/check-command-boundary.sh` |

**阶段边界材料**

`00-master-plan/README.md`、`edge_todo.md`（E-CMDSYSFACE、E-SYSCALL-SIGN、E-FSCMDS 等条目）、`edge2.md`（L10 命令层 wrapper 族）、`edge3.md`（S35/S36/S40）、`edge4.md`、目标目录 `plan.md`、`todo.md`。前一个 stage `17-stage-net/00-net-overview.md` 与 `14-stage-runtime` 相关篇的边界声明。

### 0.3 使用的命令与关键输出（证据摘录）

```text
# 1. 命令数与 .c 计数（2026-09-19 实测，逐个目录）
$ cd minix3 && for d in bin sbin usr.bin usr.sbin minix/commands minix/usr.bin games; do
    echo "$d dirs=$(ls -d $d/*/ | wc -l) c=$(find $d -name '*.c' | wc -l) lines=$(find $d -name '*.c' -exec cat {} + | wc -l)"; done
bin: dirs=30 c=143 lines=89623      sbin: dirs=19 c=69 lines=37582
usr.bin: dirs=141 c=338 lines=146364  usr.sbin: dirs=25 c=103 lines=52834
minix/commands: dirs=81 c=98 lines=46160   minix/usr.bin: dirs=8 c=36 lines=20161
games: dirs=24 c=75 lines=30806
$ find libexec/getty usr.bin/login -name '*.c' | wc -l   → 7 ；行数 → 3912
$ ls etc/ | wc -l → 51 ；ls etc/rc.d/ | wc -l → 33（32 脚本 + Makefile）
```

统计口径的两处更正（对照 `plan.md` §5.1/§5.2/§1.1）：

1. `plan.md` §1.1 与 `00-commands-overview.md` 说"328 个命令程序"，实测 328 是**目录数**：其中 `minix/commands/DESCRIBE/` 是构建面（不是命令程序）、`minix/commands/devmand/` 是明确排除给 `11-stage-devman` 的 server。**真正的命令程序是 326 个**。
2. `plan.md` §5.1 的九行相加是 869 个 `.c` / 427,442 行，与"合计 865 / 425,130"不符——因为 `usr.bin/` 那一行（338 / 146,364）**已经包含** `usr.bin/login/`（4 / 2,312）。把 login 行当加数会双重计数；正确算法是把 login 视为 `usr.bin/` 的子行。

```text
# 2. 归属表复核（plan §5.2 对 328 个目录逐目录 diff）
$ comm -3 <(ls -d bin/*/) <(plan.md §5.2 的 bin 段列出的名字)   → 空
（bin 30/30、sbin 19/19、usr.bin 141/141、usr.sbin 25/25、
  minix/commands 81/81、minix/usr.bin 8/8、games 24/24）
结论：0 遗漏、0 幽灵、0 跨篇重复分配。真实跨目录同名者只有 3 组：
  mount（sbin + minix/commands）、mail（usr.bin + minix/commands）、banner（usr.bin + games）。
```

```text
# 3. 交付链真序的关键锚点（可直接核对）
$ grep -n "transition\|requested_transition" minix3/sbin/init/init.c | head
init.c:133-146 状态常量（DEATH/SINGLE_USER/RUNCOM/READ_TTYS/MULTI_USER/CLEAN_TTYS/CATATONIA）
init.c:195 缺省 requested_transition = runcom
init.c:624 transition() 主循环：s = (*s)()
init.c:975 runcom()；init.c:899 argv = sh /etc/rc autoboot；init.c:913 execv
init.c:1222-1285 read_ttys()（setttyent + getttyent 循环 + new_session）
init.c:1528-1564 multi_user()（每终端一个 getty）；init.c:1321-1370 start_getty()
init.c:1460-1497 collect_child()（getty 退出即重生）
```

```text
# 4. rc.d 的真实形态：大量脚本是空壳，真正干活的是 rc.minix
$ sed -n '1,8p' minix3/etc/rc.d/fsck      → 只有 PROVIDE/REQUIRE 注释与一行 :（stub）
$ sed -n '1,6p' minix3/etc/rc.d/minixrc   → BEFORE: DISKS ；start_cmd="sh /etc/rc.minix start"
$ grep -n 'fsck\|mount -a\|utmp' minix3/etc/rc.minix
etc/rc.minix:144 fsck -x / $fflag $fsckopts
etc/rc.minix:145 mount -a
etc/rc.minix:155-156 >/var/run/utmp ；>/var/run/utmpx
etc/rc.minix:159-179 minix-service edit rs/vm/pm/sched/vfs/ds/tty/memory/mib/pfs/init
etc/rc.minix:200-203 devmand -d /etc/devmand -d /usr/pkg/etc/devmand &
（对照：现有 01 篇对 rc.minix / rc.cd / rc.capes / rc.shutdown 的提及次数为 0，$ grep -c 实测）
```

```text
# 5. 命令的统一生命周期锚点
$ sed -n '145,191p' minix3/lib/csu/common/crt0-common.c
  145 ___start(...)
  154 environ = ps_strings->ps_envstr;
  158 __progname = ps_strings->ps_argvstr[0];
  191 exit(main(ps_strings->ps_nargvstr, ps_strings->ps_argvstr, environ));
$ sed -n '44,52p' minix3/lib/csu/arch/x86_64/crt0.S   → _ENTRY(__start) 把参数搬进 %rdi/%rsi/%rdx 后跳 ___start
$ ls minix3/lib/libc/stdlib/getopt.c                  → 选项解析的单一实现（所有命令共用）
```

```text
# 6. 断链成本（引用计数）
$ 逐篇统计 stage 内对 NN-*.md 的引用 → 167 处（00 篇 0，01 篇 15 最多）
$ rg -c '18-stage-commands/[0-9a-zA-Z._-]+\.md' 排除本目录 → 
   os/commands/*/src/lib.rs 24 处（每个域 crate 一行 //! Covers ...）
   os/commands/bin/fileops/src/bin/echo.rs 1 处（99 §1）
   prompt/todo_plan.md 3、edge_todo.md 3、edge3.md 3
   tools/check-command-boundary.sh 1、os/Cargo.toml 1
$ 本目录内 plan.md 33 处、todo.md 16 处
机械修改总量约 250 处。
```

---

## 1. C 真序（运行时真序）

### 1.1 阶段类型判定与理由

本 stage 同时具备三种形态，按**集合型**为主处理，理由是：

1. **集合型（主）**：326 个命令程序分属 7 个源目录，彼此没有调用关系，也没有共同的初始化函数。它们只有三条共同线索：都从 `_start` 交棒进来、都用同一份 `getopt` 解析选项、都以退出码向调用者汇报。按集合型处理，就要先给统一框架篇（生命周期、交付形态、跨篇约定），再按命令族分组，组内选代表精讲、其余以差异表收束，并明确给出阅读路径。
2. **启动链型（次）**：从内核交棒给 `init` 到用户拿到 shell，是一条 51 步的线性链，每一箭头都是 fork/exec 进程边界，必须按运行时序讲述。这条链只占 5 篇，但它是整个 stage 的骨架。
3. **服务事件循环型（弱）**：只适用于 `init`（状态机 + 主循环 + 信号）、`cron`、`inetd`、`syslogd` 四个程序，它们各自有启动段与循环段。这四个不单独立 stage，按"一次请求的生命周期"讲（见 §1.3 真序 C）。

对应到讲述结构（第三部分步骤 4 的组织规则）：统一框架先行（新 00–03），交付链按运行时序（新 04–08），命令族按功能域分组并给出代表成员与差异表（新 09–30），实现账本与全局索引收尾（新 99）。

### 1.2 真序 A：用户系统交付链（线性段）

下表每一步都可核对；锚点一律给 `minix3/` 下的相对路径与行号，行号取自本次实测。

| # | 动作 | C 函数与文件锚点 | 说明 |
|---|---|---|---|
| 1 | 内核把控制权交给 PID 1 | `sbin/init/init.c:229`（`main`），uid/pid 守卫 `:242-249` | 内核阶段结束点，本 stage 起点 |
| 2 | init 建会话、开日志、解析 `-sf` | `init.c:255`（`setsid`）、`:278`（`openlog`）、`:287-298`（`getopt "sf"`） | `-s` 置单用户，`-f` 置快速启动 |
| 3 | 七状态枚举与初始转移 | `init.c:133-146`（`DEATH 'd'`、`SINGLE_USER 's'`、`RUNCOM 'r'`、`READ_TTYS 't'`、`MULTI_USER 'm'`、`CLEAN_TTYS 'T'`、`CATATONIA 'c'`）、`:195`（缺省 `runcom`） | 状态用返回值串成链 |
| 4 | 装信号处理 | `init.c:315-320`；重启 `:517-525`、断电 `:530-538`、灾难 `:504-511`、转移 `:1502-1522` | `SIGABRT`→`/sbin/shutdown -r now CTRL-ALT_DEL`；`SIGUSR1`→断电；`SIGHUP/SIGTERM/SIGTSTP`→转移；全树无 `SIGEMT`（grep 实测为空） |
| 5 | 进入状态机主循环 | `init.c:358`（初始调用）、`:624-640`（`transition` 循环 `s = (*s)()`） | 函数指针状态机 |
| 6 | `runcom` 状态执行启动脚本 | `init.c:974-1014`；`:980` 调 `runetcrc(0)`；`:1013` 转 `read_ttys` | |
| 7 | fork + exec `/etc/rc autoboot` | `init.c:879-969`；fork `:887`、`setctty` `:895`、argv 组装 `:897-900`、`execv` `:913` | 解释器是 `/bin/sh`（`init.c:105` + `include/paths.h:125`） |
| 8 | 启动脚本非零退出 → 掉回单用户 | `init.c:959-966` | 交付链的失败回退路径 |
| 9 | 单用户兜底（可选根口令校验） | `init.c:694-874`；fork `:732`、口令检查 `:747-762`、argv[0] 置 `-sh` `:797`、`execv` `:803/807` | 退出后置 `FASTBOOT`（`:866`）回到 `runcom`（`:870`） |
| 10 | `/etc/rc` 入口与配置层 | `etc/rc:467`（`rc_real_work "$@" \| rc_postprocess`）、`:17-21` 引入 `rc.subr`、`:22` 引入 `rc.conf`、`:31-34` `rc_configured` 闸门、`:36-39` `autoboot` 置 `rc_fast` | 三层结构：变量（`rc.conf`）＋函数库（`rc.subr`）＋脚本目录（`rc.d`） |
| 11 | rcorder 生成启动顺序 | `etc/rc:147-152`：`files=$(rcorder -s nostart ${rc_rcorder_flags} ${scripts})`；实现 `sbin/rcorder/rcorder.c:166-205`（`getopt "dk:s:"`）、`:78-91`（关键字）、`:405-475`（`crunch_file`）、`:587-626`（`satisfy_req`，缺提供者置 `exit_code=1`）、`:749-771`（`generate_ordering`） | 关键字契约：`PROVIDE`/`REQUIRE`/`BEFORE`/`KEYWORD`（含复数拼写） |
| 12 | 按序执行每个脚本的 `start` | `etc/rc:161-165`（`run_rc_script $_rc_elem start`）、`:317-331`（失败清单与 `/var/run/rc.log` 在 `mountcritlocal` 之后才落盘） | |
| 13 | 单个脚本的分派 | `etc/rc.subr:849-907`（`run_rc_script`：`.sh` 用 source、可执行文件起子 shell、不可执行文件忽略、`*~`/`*.OLD` 跳过）、`:478-804`（`run_rc_command`：`fast`/`force`/`one` 前缀、`rcvar` 闸门、`start` 路径的 `required_*` 检查与 `_precmd`） | 这是"标准动作集"（start/stop/reload/restart/status/poll）的实现 |
| 14 | **第一个被执行的脚本是 `minixrc`** | `etc/rc.d/minixrc:6-7`（`PROVIDE: minixrc`、`BEFORE: DISKS`）、`:13`（`start_cmd="sh /etc/rc.minix start"`） | 现有 01 篇完全没有这一环 |
| 15 | `rc.minix` 接管真实启动动作 | `etc/rc.minix:78-88`（`sysenv bootopt` + `getopts 'saf'`）、`:144`（`fsck -x /`）、`:145`（`mount -a`）、`:147-152`（卸根内存盘）、`:155-156`（清空 `utmp`/`utmpx`）、`:159-179`（`minix-service edit` 十个服务域）、`:182-187`（单用户进 `sh`）、`:200-203`（起 `devmand`）、`:229-237`（站点 rc 与 `/etc/issue`） | 存储、系统数据库、服务注册的真实执行面 |
| 16 | 介质根的特殊分支 | `etc/rc.minix:100-103` 引入 `etc/rc.cd`；`etc/rc.cd:26-29` 建 `/var`、`/tmp`、`/usr/run`、`/root` 内存盘；`:10-13`（`bootramdisk` 分支，`/etc/rc.ramdisk` 不在本 C 树内） | 安装盘与无盘站路径 |
| 17 | rc 退出 0 → init 读终端表 | `init.c:968` / `:1013`；`read_ttys` `:1222-1285`（`do_setttyent` `:1792-1806`、`getttyent` 循环 `:1279-1281`）；`_PATH_TTYS` 见 `include/ttyent.h:40` | 进入扇出点 |
| 18 | 每个 `TTY_ON` 条目建一个会话对象 | `init.c:1142-1180`（`new_session`，`se_device = _PATH_DEV + ty_name` `:1159`）、`:1185-1217`（`setupargv` 组装 `"<ty_getty> <ty_name>"`）、`:1101-1118`（`construct_argv`） | 会话表 `session_t` 见 `:157-165` |
| 19 | `multi_user` 为每终端派一个 getty | `init.c:1528-1564`；提 securelevel `:1543-1544`、逐个 `start_getty` `:1546-1557`、`waitpid/collect_child` `:1559-1561` | 链在这里第一次扇出成 N 路 |
| 20 | getty 的 spawn 与重生 | `init.c:1321-1370`（fork `:1331`、限速 `:1350-1355`、`execv` `:1365`）；`collect_child` 发现会话子进程退出即重生 `:1487`、`:1495` | 交互会话永不缺席的机制 |
| 21 | 会话拆除三条路 | `clean_ttys` `:1569-1629`（重读 `/etc/ttys`、杀/建会话）、`catatonia` `:1634-1643`（封登录）、`death` `:1661-1698`（按 `death_sigs` 逐个 HUP/TERM/KILL） | |
| 22 | getty 认领终端 | `libexec/getty/main.c:184-457`；忽略 `SIGINT` `:197`、取 tty 名 `:234-242`、`chown/chmod` 收紧 `:261-262`、打开重试 `:274-283`、`login_tty` `:291`、`tcgetattr` `:296` | |
| 23 | getty 查能力表 | `main.c:302-315`（`gettable("default")`/`gendefaults`/`gettable(tname)`/`setdefaults`）；字段表 `libexec/getty/init.c:58-118`（`lo` 缺省 `/usr/bin/login` `:69`、`lm` 缺省 `"login: "` `:62`）；查表实现 `libexec/getty/subr.c:70-113`（`cgetent` 系）；表文件 `_PATH_GETTYTAB` = `/etc/gettytab`（`include/paths.h:73`） | 终端方言词典 |
| 24 | 打印 issue/欢迎语并读用户名 | `main.c:357-366`（`IF`）、`:369`（`IM`）、`:398` + `:459-581`（`getname`，含 PPP 探测 `:505-521`、EOT 退出 `:523`） | |
| 25 | getty 交棒给 login | `main.c:435`（`makeenv`，环境组装见 `subr.c:615-638`）、`:441/444`（`execle(LO, "login", AL ? "-fp" : "-p", --, name, NULL, env)`） | 交棒即退出，不回收 |
| 26 | login 解析参数与定位终端 | `usr.bin/login/login.c:207`（`getopt "a:Ffh:ps"`）、`:266-276`（tty 名）、`:174-176`（超时） | |
| 27 | login 查口令库 | `login.c:348`（`getpwnam`）→ `lib/libc/gen/getpwent.c:617-642`（`_files_getpwnam`）、`:181`/`:194-199`（按 euid 选 `/etc/spwd.db` 或 `/etc/pwd.db`）；文本源 `_PATH_MASTERPASSWD` 见 `include/pwd.h:73`，由安装期 `pwd_mkdb` 编译（`etc/Makefile:304-313`） | |
| 28 | 口令校验与退避 | `login.c:368-374`（`-f` 预认证）、`:399`/`:423-424`（`crypt(getpass(...))`）、`:174-175`（超时）、`:182-195`、`:116-117` | 十次尝试与三秒退避 |
| 29 | 安全终端判定 | `login.c:441`（`rootterm`）→ `common.c:122-128`（`getttynam(ttyn) && ty_status & TTY_SECURE`）；终端类型 `stypeof` `common.c:370-372` | `/etc/ttys` 是第二处契约 |
| 30 | 账号可用性检查 | `login.c:483-487` + `:769-780`（`checknologin`）、`:535-556`（有效期） | |
| 31 | 记录会话与移交终端属主 | `login.c:558`（`update_db`，写 `utmp`/`utmpx`/`wtmp`，实现在 `common.c:166+`）、`:560-561`（`chown`）、`:563`（`ttyaction`） | `utmp` 的真源在此 |
| 32 | 布置凭据 | `login.c:498-519`（临时 `setegid/seteuid/initgroups` 进家目录）、`:525-527`（收回）、`:588`（`setgid`）、`:590`（`initgroups`）、`:598-601`（`setuid`） | Minix 上跳过 `setlogin`（`:592-595`） |
| 33 | 布置环境 | `login.c:573-574`（默认清空环境，`-p` 才继承）、`:616-617`（`HOME`/`SHELL`）、`:618-627`（`TERM` 取自 `stypeof`，缺省 `"su"`）、`:628-629`（`LOGNAME`/`USER`）、`:634`（`PATH` 取 `_PATH_DEFPATH`） | |
| 34 | exec shell | `login.c:604-605`（缺省 `/bin/sh`）、`:690-692`（argv[0] 前置 `-` 成为登录 shell）、`:723`（`execlp`）；口令过期时先 fork `/usr/bin/passwd`（`:704-715`） | 交付链终点 |
| 35 | sh 入口与启动文件 | `bin/sh/main.c:102-243`；`procargs` `bin/sh/options.c:88-136`（无操作数且无 `-c` 时 `sflag` `:98-99`、脚本文件操作数 `:113-118`、`-c` `:186-188`）；`main.c:198`（`argv[0][0]=='-'` → 读 `/etc/profile` `:200`）、`:203`（`.profile`）、`:207-213`（`ENV`）；`read_profile` `:314-342` | 登录 shell 的三层配置 |
| 36 | 两种进入方式 | `main.c:231-232`（`-c` 直接 `evalstring`）、`:234-237`（`cmdloop(1)`，实现 `:251-306`） | |
| 37 | 配置文件的真实内容 | `etc/profile:7-17`（`LD_LIBRARY_PATH`、`TZ`）、`etc/skel/dot.profile:41` / `etc/root/dot.profile:22`（`ENV` 指向 `$HOME/.shrc`）、`etc/skel/dot.shrc:3-4`（引入 `/etc/shrc`）、`etc/shrc:1-19`（`PS1` 与 emacs 模式） | |
| 38 | 命令的交棒入口（统一框架） | `lib/csu/arch/x86_64/crt0.S:39-44`（`_start` 搬参跳转）、`lib/csu/common/crt0-common.c:145`（`___start`）、`:154`（`environ = ps_strings->ps_envstr`）、`:158`（`__progname`）、`:191`（`exit(main(argc, argv, environ))`） | 每条命令都从这里开始 |
| 39 | 循环闭合 | `init.c:1469`（`find_session(pid)`）、`:1487`（重生 getty） | shell 退出 → getty 退出 → init 重生，链回到第 20 步 |

**线性段的边界说明。** 链在第 19 步（`read_ttys`/`multi_user`）第一次扇出成 N 路并发会话，此后会话之间没有次序关系；在第 11–12 步（`rcorder`）内部，启动脚本本身是一张依赖图而不是一条线，`NETWORKING`/`SERVERS`/`DAEMON`/`LOGIN` 四个脚本是纯屏障（`etc/rc.d/NETWORKING:6`、`SERVERS:6-7`、`DAEMON:6-7`、`LOGIN:6-7`，正文为空）；在第 34 步交给 shell 之后，链终止于任意命令树，本 stage 的线性叙事到此为止。

### 1.3 真序 B：一条命令的统一生命周期（汇聚点）

326 条命令共享同一条生命周期，这是整个集合型 stage 的汇聚点，也是全新篇章的原料。每一步都有唯一锚点：

| # | 动作 | C 函数与文件锚点 | 说明 |
|---|---|---|---|
| B1 | 装载与交棒 | `lib/csu/arch/x86_64/crt0.S:39-44`；`lib/csu/common/crt0-common.c:145-191` | 汇编 `_start` 把 `ps_strings` 指针搬进寄存器后跳 `___start` |
| B2 | 得到 `argc`/`argv`/`environ` | `crt0-common.c:154`（`environ`）、`:158`（`__progname`）、`:191`（`main` 三参数） | 参数与环境从内核装在栈上的 `ps_strings` 取，不经任何库 |
| B3 | 解析选项 | `lib/libc/stdlib/getopt.c`（单一实现，`include/getopt.h` 声明） | 全部命令共用同一套短选项解析；`--` 结束选项、可选参数、`opterr`/`optind` 语义一致 |
| B4 | 参数不合法 → 打印用法并退出 | 各命令内部的 `usage()` 模式（如 `minix/commands/svrctl/svrctl.c:107`、`minix/commands/at/at.c:26-80` 的用法行） | 用法行是命令的外部契约之一，与手册页 `SYNOPSIS` 对应 |
| B5 | 干活：文件、终端、网络、进程 | 各命令主体；跨进程面见 §1.4 | |
| B6 | 汇报：退出码与标准错误 | `crt0-common.c:191`（返回值成为 `exit` 参数）；错误面按各命令 `errno` 映射（如 `os/commands/bin/proctools/src/lib.rs` 的 `ProcError::NotFound` → 3） | 退出码是命令对调用者唯一的机器可读结论 |
| B7 | 安装期登记（命令"从哪来"） | `bin/Makefile.inc:7`、`sbin/Makefile.inc:7`、`usr.bin/Makefile.inc:5`、`usr.sbin/Makefile.inc:4`、`games/Makefile.inc:7,17`；`usr.bin/Makefile`（`SUBDIR=` 全量登记）、`minix/commands/Makefile` | 一层目录一个 `BINDIR`，一份 `SUBDIR` 清单就是命令的注册表 |
| B8 | 查找期发现（命令"怎么被找到"） | `include/paths.h:45`（`_PATH_DEFPATH`）、`include/paths.h:43`（`RESCUEDIR` 变体） | `execvp` 按 PATH 逐目录试；单用户救援路径在最前 |
| B9 | 自描述面（命令"怎么被读懂"） | 349 个手册页（命令目录内 `*.[1-9]`）、`etc/man.conf`、`usr.bin/man/man.c`、`libexec/makewhatis/makewhatis.c` | man 系统是命令面的索引层 |

### 1.4 真序 C：命令族的参考使用路径（扇出段）

集合型 stage 的"发现机制"不是函数调用，而是用户实际会走的使用路径。下面把 326 条命令能回答的真实场景列成路径，这也是命令族分组的依据。

| 路径 | 步骤序列 | 锚点 |
|---|---|---|
| P1 首次安装一台机器 | 分区（`fdisk`/`part`）→ 建卷（`newfs_*`）→ 挂载（`mount`）→ 建节点（`MAKEDEV`）→ 首次配置（`setup`） | `minix/commands/fdisk/`、`sbin/newfs_*`、`minix/commands/mount/mount.c`、`minix/commands/MAKEDEV/MAKEDEV.sh`、`minix/commands/setup/setup.sh` |
| P2 开机到能用 | 内核 → `init` → `/etc/rc` → `rcorder` → `rc.minix`（`fsck` + `mount -a`）→ 服务 → getty → `login` → shell | §1.2 全链 |
| P3 日常管理服务 | `service`/`minix-service` 查改 → `svrctl` 调内部参数 → `crontab`/`at` 排任务 | `usr.sbin/service/service`、`minix/commands/minix-service/`、`minix/commands/svrctl/`、`minix/commands/crontab/`、`minix/commands/at/` |
| P4 用户管理 | `passwd`/`chpass` 改口令 → `vipw` 或 `user` 直接编辑库 → `pwd_mkdb` 重建索引 | `usr.bin/passwd/`、`usr.bin/chpass/`、`usr.sbin/vipw/`、`usr.bin/user/`、`usr.sbin/pwd_mkdb/` |
| P5 命名到地址 | `getent` 查（`hosts`/`services`/`protocols`/`group`/`shells`） | `usr.bin/getent/getent.c:98-136` |
| P6 网络接通到验证 | `ifconfig` 配接口 → `route` 加路由 → `ping` 验证 → `traceroute` 定位 → `netstat` 看状态 | `sbin/ifconfig/ifconfig.c:1050/1059/1173`、`sbin/route/route.c:1196`、`sbin/ping/ping.c:897/1266`、`usr.sbin/traceroute/traceroute.c:472`、`usr.bin/netstat/main.c:866` |
| P7 挂载一块新盘 | `devsize` 看大小 → `partition`/`autopart` 排布 → `fdisk` 写表 → `newfs_*` 建卷 → `mount` 挂上 → `fsck` 定期体检 | `minix/commands/{devsize,partition,autopart,fdisk}`、`sbin/newfs_*`、`minix/commands/mount/`、`sbin/fsck/fsck.c:254` |
| P8 搬运整盘与镜像 | `dd` 复制 → `writeisofs` 制镜像 → `isoread`/`vol` 验镜像 → `vnconfig` 配虚拟盘 → `mount` | `bin/dd/args.c:105-121`、`minix/commands/writeisofs`、`minix/commands/isoread/isoread.c:41`、`usr.sbin/vnconfig` |
| P9 备份与恢复 | `backup` 做增量 → `remsync`/`synctree` 同步到别处 → `cleantmp` 清过期 → `mt` 管磁带 → `fix` 应用差异 | `minix/commands/backup/backup.c:3-11`、`minix/commands/remsync/remsync.c:101/177/285`、`minix/commands/synctree/synctree.c:54-55`、`minix/commands/cleantmp/cleantmp.c:31-32`、`minix/commands/mt/mt.c:42/78`、`minix/commands/fix/fix.c:10-14` |
| P10 进入系统看状态 | `uname`/`hostname`/`pagesize` → `version`/`readclock`/`printroot` → `ps`/`mtop`/`ministat` → `sysctl` 查改参数 → `ldd` 看依赖 → `srccrc`/`sprofdiff` | `usr.bin/uname/uname.c:111`、`bin/hostname/hostname.c:57`、`minix/commands/{version,readclock,printroot}`、`bin/ps/keyword.c:81-197`、`minix/usr.bin/{mtop,ministat}`、`sbin/sysctl/sysctl.c:252/520`、`usr.bin/ldd/ldd.c:94-96` |
| P11 交互会话管理 | `who`/`w`/`last`/`users` 看谁在 → `write`/`wall` 发消息 → `mesg` 开关接收 → `tty`/`logname` 查自己 | `lib/libc/compat/include/utmp.h:42-46`（36 字节记录）、`usr.bin/{who,w,last,users,write,wall,mesg,tty,logname}` |
| P12 shell 内做文本处理 | 取段（`head`/`tail`/`cut`/`paste`）→ 整形（`tr`/`fold`/`expand`）→ 排序去重（`sort`/`uniq`）→ 正则（`grep`/`sed`）→ 比较补丁（`cmp`/`diff`/`patch`） | `usr.bin/{head,tail,cut,paste,tr,fold,expand,sort,uniq,uniq.c:56}`、`minix/usr.bin/grep/util.c:205`、`usr.bin/sed/process.c:403-432`、`usr.bin/diff`、`usr.bin/patch` |
| P13 定位与查手册 | `whereis`/`whatis`/`apropos` 找页 → `man` 读页；`makewhatis` 离线建索引，`etc/man.conf` 定源 | `usr.bin/man/man.c`、`usr.bin/apropos/`、`usr.bin/whatis/`、`usr.bin/whereis/`、`libexec/makewhatis/makewhatis.c` |

**扇出段的分类框架（命令族）。** 上述路径归到四类，命令族分组按这四类落地：

1. **身份与注册类**（命令"是谁、在哪、谁在用"）：`version`、`printroot`、`readclock`、`uname`、`hostname`、`tty`、`logname`、`who`、`w`、`last`、`users`、`getent`。
2. **转换与计算类**（把输入变成另一种形态，不碰硬件）：文本行工具、正则、编辑器、排版、压缩、编码、哈希、数值换算。
3. **介质与状态类**（改变或读系统持久状态）：存储四组、网络配置、服务管理、用户与口令、设备节点。
4. **交互与娱乐类**（面向人的输入输出）：终端控制、终端游戏、stdio 游戏、文本游戏。

---

## 2. 知识点全集（存量池）

### 2.1 池总表

编号规则：`K-nnn` 为 stage 内唯一编号；**来源**列的"存量"指来自现有文档，"新增"指现有文档没有、由 §3 覆盖审计从 C 源码、非 C 制品或操作系统理论追加入池。**去向**列给新编号（见 §4 新目录）。

**K-001 … K-016｜来源：`00-commands-overview.md`（全文 21 行，自述 pending 的最小骨架）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-001 | 文档状态与范围声明（本篇是待改写骨架） | 工具与工程 | 存量 | 00 头部 | `00-commands-overview.md:3-7` | 知道本篇不可当权威 | 00（改写后作废此行） |
| K-002 | 源面与模块面清单（7 个 C 目录 + getty/login；Rust 侧 crate） | 工具与工程 | 存量 | 00 头部 | `00-commands-overview.md:5-7` | 知道覆盖哪些源与 crate | 00 |
| K-003 | 交付因果链总图（boot → init → rc → 服务 → 登录 → 设备库 → shell → 命令） | 导航 | 存量 | 00 核心点 1 | `plan.md §1.2` | 建立 24 篇的阅读地图 | 00（补篇名与锚点） |
| K-004 | 命令面规模统计（328 / 865 / 425,130） | 工具与工程 | 存量 | 00 核心点 2 | `plan.md §5.1`；本报告 §0.3 更正为 326 程序 | 知道命令面多大 | 00（按更正口径改写） |
| K-005 | 功能域矩阵（文件/文本/正则/…/游戏） | 导航 | 存量 | 00 核心点 3 | `plan.md §2` | 按域定位篇章 | 00 |
| K-006 | 文档导航与"编号=交付链阅读顺序、与实施顺序解耦" | 导航 | 存量 | 00 核心点 4 | `plan.md §6` | 区分阅读序与实现序 | 00 |
| K-007 | 设计原则：非 server 主线重定义 + 位置可回答性 | 架构演进 | 存量 | 00 核心点 5 | `plan.md §1.1/§1.3` | 理解本篇为何另立主线 | 00 |
| K-008 | 边界声明（前置 01-stage-kernel / 14-stage-runtime / 17-stage-net） | 导航 | 存量 | 00 边界 | `00-commands-overview.md:19-21` | 知道读本篇前要会什么 | 00 |
| K-009 | 命令面的一级分类框架（四类：身份与注册/转换与计算/介质与状态/交互与娱乐） | 概念 | 新增 | — | 本报告 §1.4；理论出处：POSIX 命令分类与 Minix3 目录分层 | 给 326 条命令一个可记忆的顶层分类 | 00 |
| K-010 | 阅读路径三线（主线：交付链→统一框架→命令族；支线：游戏/包管理；可跳读：i18n/音频/游戏） | 导航 | 新增 | — | 本报告 §4.3 | 让读者按需选路 | 00 |
| K-011 | 命令的"统一框架"三问（它从哪来、怎么被找到、怎么读懂） | 导航 | 新增 | — | `bin/Makefile.inc:7`；`include/paths.h:45`；349 个手册页 | 把三个散落的交付面串成一问 | 00 → 02 |
| K-012 | 本篇不覆盖索引（命令机制细节的归属） | 导航 | 存量 | 00 边界 | `00-commands-overview.md:20` | 知道细节去哪找 | 00 |
| K-013 | 命令层的分层契约摘要（命令只消费 minix-rt 与 minix-sys 顶层） | 约束与不变量 | 存量 | 00 头部"Rust 模块"隐含 | `99-global-concepts.md §1` | 知道依赖红线 | 03 |
| K-014 | getty/login 与 `etc/` 的归属声明 | 导航 | 存量 | 00 头部源码行 | `libexec/getty/`、`etc/` | 知道这两个面算不算命令 | 00 |
| K-015 | 统计口径的定义（什么算一条命令：一个目录一条命令） | 工具与工程 | 新增 | — | 实测：`ls -d <dir>/*/`；DESCRIBE 与 devmand 的两处例外 | 避免"328 个命令程序"式的口径混淆 | 00 + 99 |
| K-016 | 文档-实现的对应关系史（24 篇 ↔ 24 个域 crate 的既成事实） | 工具与工程 | 新增 | — | `todo.md §6.1 C-1` 的收敛目标段 | 理解为什么篇章与 crate 一一对应 | 02 + 99 |

**K-017 … K-032｜来源：`01-init-rc-scripts.md`（265 行，自述"已完成，等待评审收敛"）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-017 | 第一个用户进程：孤儿收养、运行级别、终端派生 | 概念 | 存量 | §1.1-1.2 | `sbin/init/init.c:229-367`（`main`） | 理解内核交权后谁带系统到可用 | 04 §1 |
| K-018 | 启动脚本三层架构（配置变量/函数库/脚本目录） | 机制 | 存量 | §1.3 | `etc/rc:17-22`、`etc/rc.subr`（1359 行）、`etc/rc.d/`（32 脚本） | 知道改开机行为该动哪层 | 04 §1、§2 |
| K-019 | 启动与关机互逆（`shutdown`/`reboot` 分工） | 概念 | 存量 | §1.4、§2.7 | `sbin/shutdown/shutdown.c`、`sbin/reboot/reboot.c`、`init.c:517-538` | 理解关机是通知/等待/卸载 | 04 §1、§2 |
| K-020 | 七状态状态机与初始转移 | 机制 | 存量 | §2.1 | `init.c:133-146`、`:195`、`:624` | 能读懂 init 主循环 | 04 §2.1 |
| K-021 | `runcom` 状态与 `FASTBOOT` | 机制 | 存量 | §2.2 | `init.c:975`、`:899`、`:866`、`:933` | 知道启动脚本由谁在何时执行 | 04 §2.2 |
| K-022 | 终端会话管理（`read_ttys`/`multi_user`/`clean_ttys` 与会话表） | 机制 | 存量 | §2.3 | `init.c:1222`、`:157-165`、`:1142-1180`、`etc/ttys` | 理解终端永远有人值守 | 04 §2.4 |
| K-023 | 信号面三条线（`SIGABRT`→重启、`SIGUSR1`→断电、灾难信号）与处理函数极简律 | 接口与协议 | 存量 | §2.4 | `init.c:315-320`、`:517-538`、`:504-511` | 知道关机/重启请求如何进入 init | 04 §2.3 |
| K-024 | `/etc/rc` 总体流程（配置检查、排序、日志分流、元数据标记） | 机制 | 存量 | §2.5 | `etc/rc:31-34`、`:50`、`:141-152`、`:327-331` | 理解开机每一步与 rc.log 分离 | 04 §2.5 |
| K-025 | rcorder 注释契约与拓扑排序（对照 systemd） | 接口与协议 | 存量 | §2.6、§3.5 | `sbin/rcorder/rcorder.c:78-91`、`:405-475`、`:587-626`；`etc/rc:147-152` | 能写合规启动脚本、解释排序 | 04 §2.6 |
| K-026 | 配置面与首次配置（`rc.conf`/`defaults`/`boot.cfg.default`/`setup`） | 工具与工程 | 存量 | §2.7-2.8 | `etc/rc.conf`、`etc/defaults/`、`etc/boot.cfg.default`、`minix/commands/setup/setup.sh` | 知道开机行为改哪里 | 04 §2.8 |
| K-027 | Rust 用枚举状态机替代函数指针状态机 | 架构演进 | 存量 | §3.1 | `os/commands/sbin/init/src/state_machine.rs` | 状态显式化换来编译期完备检查 | 04 §3 |
| K-028 | 纯决策函数下沉（`diff_line`/`request_for`/`shutdown_argv`/`decide_entry`） | 架构演进 | 存量 | §3.2、§4.3 | `os/commands/sbin/init/src/{clean_ttys,contracts,entry}.rs` | 明白 136 个测试为何能脱离进程跑通 | 04 §3 + 99 账本 |
| K-029 | 会话表：C 侵入式链表 → Rust 哈希表 | 数据结构 | 存量 | §3.3 | `os/commands/sbin/init/src/{session_db,session}.rs` | 所有权清晰消灭"忘记摘除"整类错误 | 04 §3 |
| K-030 | 架构演进候选 A-6：保留 shell 启动链 vs 编译期静态配置 | 架构演进 | 存量 | §3.4 | `minix/commands/setup/setup.sh`、`etc/rc*`、`os/commands/sbin/init/src/{runcom,sysctl,ttys}.rs` | 知道启动链对 shell 的依赖为何悬置 | 04 §3 + 99 ARCH 登记 |
| K-031 | Rust 模块职责映射与关键不变量 | 约束与不变量 | 存量 | §4.1-4.2 | `os/commands/sbin/init/src/`（20 个源文件） | 按职责索引 C→Rust 对应 | 99 账本 |
| K-032 | 测试要点（89 个）与两项留白 | 测试性质 | 存量 | §5 | `cargo test -p minix-init`（实测 136 个 `#[test]`） | 知道哪些层已覆盖 | 99 账本 |
| — | **缺口**：`etc/rc.minix`/`rc.cd`/`rc.capes`/`rc.shutdown` 的真实启动动作（`fsck -x /`、`mount -a`、`utmp` 清零、服务注册、`devmand` 启动） | 机制 | 新增 | — | `etc/rc.minix:144/145/155-156/159-179/200-203`；`etc/rc.d/minixrc:13` | 知道开机真正干活的是谁 | 04 §2.7（新增节） |
| — | **缺口**：`rc.d` 中 32 个脚本的依赖图与"多数是空壳"的事实 | 接口与协议 | 新增 | — | `etc/rc.d/{fsck,mountcritlocal,mountcritremote,root,ttys}` 全为空壳；`NETWORKING`/`SERVERS`/`DAEMON`/`LOGIN` 为纯屏障 | 不被"32 个脚本"的表象误导 | 04 §2.6 |
| — | **缺口**：`bootconf.sh` 的交互式引导配置（30 秒超时） | 机制 | 新增 | — | `etc/rc.d/bootconf.sh:16-84`、`:54-60` | 理解首次启动的选择分支 | 04 §2.8 |

**K-033 … K-046｜来源：`02-service-scheduler.md`（207 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-033 | 服务的概念：被照看的进程与重生服务（RS） | 概念 | 存量 | §1.1 | `minix/commands/minix-service/minix-service.c`、`03-stage-rs` | 能区分服务与普通进程 | 05 §1 |
| K-034 | 两个服务命令的血统与分工（`minix-service` vs `service`） | 概念 | 存量 | §1.2 | `minix-service.8`、`usr.sbin/service/service`、`etc/rs.single` | 不再混淆同名命令 | 05 §1 |
| K-035 | `svrctl` 低层专家接口与管理员门槛 | 接口与协议 | 存量 | §1.3、§2.3 | `minix/commands/svrctl/svrctl.c:107` | 知道调服务内部参数的门在哪 | 05 §1、§2 |
| K-036 | 定时调度三种"到点"（周期/一次/固定节拍） | 概念 | 存量 | §1.4 | `minix/commands/{cron,crontab,at,update}`；`update.c`（24 行） | 能为任务选对时间模型 | 05 §1、§2 |
| K-037 | crontab 时间字段的 Minix 方言（`*`/数字/区间/冒号步长/逗号/问号）与 `/etc/crontab` 实例 | 接口与协议 | 存量 | §1.5 | `minix/commands/cron/tab.c:285-360`、`:327`、`etc/crontab` | 能写出正确时间表行 | 05 §1、§2.4 |
| K-038 | `minix-service` 九种命令形状（启动/管理/关机三组） | 接口与协议 | 存量 | §2.1 | `minix-service.8` 提要段、`minix-service.c` | 能按形状正确注册/停止/刷新服务 | 05 §2.1 |
| K-039 | `service` 脚本三段式与 `-e`/`-l` 互斥 | 接口与协议 | 存量 | §2.2 | `usr.sbin/service/service` 用法段、`etc/rc.d/` | 能查/启用启动脚本 | 05 §2.2 |
| K-040 | cron 时间解析内部（`range_parse` 位图、日/周通配语义、坏行不拖垮守护） | 机制 | 存量 | §2.4 | `cron/tab.c:285-360`、`cron/cron.c:321`、`crontab.c:70-122` | 理解一行时间表如何变成触发判定 | 05 §2.4 |
| K-041 | `at`/`atnormalize`：一次任务两端与日期顺延/跨年回绕 | 机制 | 存量 | §2.5 | `minix/commands/at/at.c:26-80`、`atnormalize.c:25` | 知道"到点跑一次"如何落成队列文件 | 05 §2.5 |
| K-042 | `update`：24 行固定节拍与"简单到不可能出错" | 机制 | 存量 | §2.6 | `minix/commands/update/update.c:1-24` | 理解固定间隔触发模型 | 05 §2.6 |
| K-043 | Rust 两个服务解析函数独立、`check_name` 共享与目录逃逸拦截 | 架构演进 | 存量 | §3.1 | `os/commands/usr-sbin/svcsched/src/service.rs` | 理解名字校验防什么 | 05 §3 |
| K-044 | `ScheduleMatcher` 接口两实现与 `due_every` 触发语义 | 接口与协议 | 存量 | §3.2 | `os/commands/usr-sbin/svcsched/src/scheduler.rs` | 理解日历/间隔两种触发如何统一 | 05 §3 |
| K-045 | 问号仅分钟字段合法 + 冒号步长不兼容斜杠 | 约束与不变量 | 存量 | §3.3-3.4 | `svcsched/src/cron.rs`、`cron/tab.c:327` | 知道哪些写法必报错 | 05 §3 |
| K-046 | 模块结构、关键类型与测试要点（27 个） | 测试性质 | 存量 | §4、§5 | `os/commands/usr-sbin/svcsched/src/`（4 文件） | 知道类型边界与覆盖盲区 | 99 账本 |
| — | **缺口**：`shlock` 是 `cron`/`at`/`backup` 共用的锁原语，现有文档未把它与调度器联系起来 | 机制 | 新增 | — | `usr.bin/shlock/`（被 `cron`、`at`、`backup` 调用） | 理解调度器如何互斥 | 05 §2.7（新增节） |

**K-047 … K-062｜来源：`03-login-passwd.md`（209 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-047 | 登录链路三棒交接与"交棒即退出" | 概念 | 存量 | §1.1 | `libexec/getty/main.c:441/444`、`usr.bin/login/login.c:723` | 能画出登录链与崩溃恢复 | 06 §1 |
| K-048 | 口令数据库两张脸（`master.passwd` 十字段 / `passwd` 七字段 / `pwd_mkdb` 索引库） | 数据结构 | 存量 | §1.2 | `etc/master.passwd`、`etc/Makefile:304-313`、`include/pwd.h:73/80-81` | 知道用户数据在哪张脸、程序读哪张 | 06 §1、§2 |
| K-049 | 文本-库一致性工具链（`vipw` 加锁/校验/重建；`chpass`/`passwd`/`pwhash` 同模式） | 机制 | 存量 | §1.2、§2.5 | `usr.sbin/vipw/vipw.c`、`usr.bin/{passwd,chpass,pwhash}` | 理解"文本改了库没更新"的窗口如何消除 | 06 §2 |
| K-050 | 身份切换命令四件套（`su`/`newgrp`/`id`/`nologin`） | 工具与工程 | 存量 | §1.3 | `usr.bin/{su,newgrp,id}`、`sbin/nologin/` | 知道登录后身份如何变化 | 06 §2 |
| K-051 | 会话环境三来源与"模板复制而非引用" | 概念 | 存量 | §1.4 | `etc/profile`、`etc/skel/`、`etc/master.passwd` 的 shell 字段 | 知道家目录/环境/shell 从哪来 | 06 §1、§2 |
| K-052 | getty 认领终端五步（属主与权限收紧、`uu` 遗留、CPU 上限） | 机制 | 存量 | §2.1 | `getty/main.c:184-457`（`:261-262`）、`getty/init.c`、`getty/subr.c` | 理解终端如何被收拾成能对话的设备 | 06 §2.1 |
| K-053 | `/etc/ttys` 五列合同与被 `init`、`getty`、`login` 三方共读 | 接口与协议 | 存量 | §2.2 | `etc/ttys`；`init.c:1279-1281`；`login/common.c:122-128` | 能读改终端表、知道几方在消费 | 06 §2.2 |
| K-054 | `gettytab` 能力表（termcap 同源、竖杠别名机制） | 数据结构 | 存量 | §2.3 | `etc/gettytab`、`getty/subr.c:70-113`、`include/paths.h:73` | 能为新终端补能力 | 06 §2.3 |
| K-055 | login 验证核心（四标志、信号处置、查库、口令比对、十次/三秒退避） | 机制 | 存量 | §2.4 | `login.c:207`、`:174-176`、`:348`、`:423-424` | 理解登录如何防打断与防猜解 | 06 §2.4 |
| K-056 | login 收尾会话布设与 exec 替换（默认 shell、`SHELL`、强制改密） | 机制 | 存量 | §2.4 | `login.c:604-605`、`:616-634`、`:690-692`、`:723`、`:704-715` | 知道会话环境何时布设 | 06 §2.4 |
| K-057 | `master.passwd` 三种登录许可形态（空口令、星号、`nologin`） | 概念 | 存量 | §2.6 | `etc/master.passwd` 前三行；`login.c:769-780` | 能判断账号能否登录 | 06 §1 |
| K-058 | Rust 四种文件格式各建解析模块 | 架构演进 | 存量 | §3.1 | `os/commands/usr-bin/login/src/{passwd,ttys,gettytab,userdb}.rs` | 理解格式知识内聚 | 06 §3 |
| K-059 | 零拷贝借用解析器与生命周期安全 | 架构演进 | 存量 | §3.2 | `usr-bin/login/src/*.rs` 借用结构体 | 理解无堆分配的收益 | 06 §3 |
| K-060 | `UserDatabase` 接口两实现（空库/切片库）与"诚实留白" | 接口与协议 | 存量 | §3.3 | `login/src/userdb.rs`、`login.c:348` | 理解阶段状态下"谁也不认识"为何安全 | 06 §3 |
| K-061 | 登录判定规则与模块结构、关键类型不变量 | 约束与不变量 | 存量 | §3.4、§4 | `login/src/userdb.rs`、`login.c:290` | 能说出空名/查无的判定顺序 | 06 §3 + 99 账本 |
| K-062 | 测试要点（27 个）与三项留白 | 测试性质 | 存量 | §5 | `cargo test -p minix-login` | 知道覆盖盲区 | 99 账本 |
| — | **缺口**：`utmp`/`utmpx`/`wtmp` 三个记账文件的结构、写入者与读者（现有 03 篇只提"登录会计"，04 篇越界讲了一点） | 数据结构 | 新增 | — | `lib/libc/compat/include/utmp.h:42-46`；写入 `login/common.c:166+`；清零 `etc/rc.minix:155-156`；读者 `usr.bin/{who,w,last,users}` | 理解"谁在用系统"的唯一真相源 | 06 §2.5（新增节）+ 18 §2 |

**K-063 … K-077｜来源：`04-device-database.md`（174 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-063 | 设备即文件：`/dev` 条目 = 类型 + 主次号 + 权限的物化 | 概念 | 存量 | §1.1 | `minix/commands/MAKEDEV/MAKEDEV.sh:210` | 明白读写设备与普通文件是同一套动作 | 07 §1 |
| K-064 | 主/次设备号编码硬件拓扑（音频 13:0、总线号嵌入、`c0d0p0s0` 三级展开） | 概念 | 存量 | §1.1、§2.1 | `MAKEDEV.sh:210`、`:213-218` | 会读设备号就知道请求转给谁 | 07 §1、§2.1 |
| K-065 | `makedev` 七参数统一入口与参数校验 | 接口与协议 | 存量 | §2.1 | `MAKEDEV.sh:56-59`、`:96` | 看懂上百条设备清单行的传参规律 | 07 §2.1 |
| K-066 | 真建 / MTREE"只说"双输出模式（创建清单与校验标准同源） | 机制 | 存量 | §1.2、§2.1 | `MAKEDEV.sh:56`、`:96` | 理解"标准与实现不脱节" | 07 §2.1 |
| K-067 | `mknod` 单节点创建与 `pack_dev` 设备号打包 | 接口与协议 | 存量 | §2.2 | `sbin/mknod/mknod.c`、`sbin/mknod/pack_dev.c:84` | 理解脚本"知道建什么"与命令"动手建"的分工 | 07 §2.2 |
| K-068 | `dev_mkdb` 常量数据库（fts 物理遍历 + 逐条写入 + 只读成品） | 机制 | 存量 | §1.3、§2.3 | `usr.sbin/dev_mkdb/dev_mkdb.c:64`、`:88`、`:178`、`:214` | 理解"离线建索引、在线查索引" | 07 §2.3 |
| K-069 | `getent`：多个库名一个查询入口的表驱动分派 | 架构演进 | 存量 | §1.4、§2.4 | `usr.bin/getent/getent.c:98-136`、`:274`、`:809` | 排错不必记十余种查法 | 07 §2.4 |
| K-070 | 系统数据库文件分工（`group`/`services`/`protocols`/`shells`/`hosts`/`motd`/`nsswitch.conf`） | 概念 | 存量 | §1.4 | `etc/{group,services,protocols,shells,hosts,motd,nsswitch.conf}` | 知道每个名字去哪个文件查、谁消费 | 07 §1、§2.4 |
| K-071 | `mtree` 目录规范（安装时按规范建树、审计时按规范校验） | 机制 | 存量 | §2.5 | `usr.sbin/mtree/mtree.c`、`etc/mtree/NetBSD.dist.base` | 理解安装与审计共用一份层次标准 | 07 §2.5 |
| K-072 | Rust `LookupTable`：按"数据库"而非"行格式"切接口 | 架构演进 | 存量 | §3.1 | `os/commands/sbin/devdb/src/lookup.rs:16` | 新增数据库不动调用方 | 07 §3 |
| K-073 | 服务认别名而组只认主名（一个接口、两种语义） | 概念 | 存量 | §3.2 | `devdb/src/services.rs:48`、`etc/services:286` | 明白接口统一不等于语义统一 | 07 §3 |
| K-074 | 解析上限显式常量、超限报错不截断 | 约束与不变量 | 存量 | §3.3 | `devdb/src/group.rs:10` | "解析成功即完整" | 07 §3 |
| K-075 | 坏行跳过策略与解析复用 | 机制 | 存量 | §3.4 | `devdb/src/lookup.rs:72`、`dev_mkdb.c:64` | 一行坏格式不瘫痪登录链 | 07 §3 |
| K-076 | 错误码映射与模块划分 | 工具与工程 | 存量 | §4.1、§4.3 | `devdb/src/lib.rs:39`、`:50-51` | 把 Rust 行为对回 C 的 errno 契约 | 99 账本 |
| K-077 | 测试要点（23 个）与执行层留白 | 测试性质 | 存量 | §5 | `cargo test -p minix-devdb` | 知道哪些能力已验证 | 99 账本 |
| — | **缺口**：`devmand` 动态设备管理的用户命令面（`devmand` 本体归 11-stage，但命令侧怎么触发设备发现没有文档回答） | 机制 | 新增 | — | `minix/commands/devmand/`；`etc/rc.minix:200-203` | 知道静态 `MAKEDEV` 与动态发现的边界 | 07 §2.6（新增节，指向 11-stage-devman） |

**K-078 … K-094｜来源：`05-shell-family.md`（190 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-078 | shell 的双重身份（交互翻译官 + 完整编程语言） | 概念 | 存量 | §1.1 | `bin/sh/`（Almquist） | 理解"变量无类型、错误不中断脚本"的来源 | 08 §1 |
| K-079 | 三种方言（ash / ksh / csh）的谱系与选用规则 | 架构演进 | 存量 | §1.2 | `bin/{sh,ksh,csh}/` | 知道系统脚本/交互/个人脚本各用谁 | 08 §1（补 ksh/csh 的源码锚点） |
| K-080 | 启动文件决策四情形（登录/交互非登录/特权不一致/非交互） | 机制 | 存量 | §1.3、§2.1 | `bin/sh/main.c:198`、`:200`、`:203`、`:207-213` | 能解释"PATH 在脚本里不一样" | 08 §1、§2.1 |
| K-081 | `ENV` 机制与真实/有效标识一致性检查 | 机制 | 存量 | §1.3、§2.1 | `bin/sh/main.c:205-211` | 理解交互配置按需加载 | 08 §2.1 |
| K-082 | 内建命令判定、`builtins.def` 登记与双形态命令 | 概念 | 存量 | §1.4、§2.6 | `bin/sh/builtins.def:74`、`:88` | 明白 `cd`/`export`/`exit` 为何必须进程内执行 | 08 §1、§2 |
| K-083 | 引用感知分词（引号、转义、注释截断） | 机制 | 存量 | §2.2 | `bin/sh/parser.c`；`os/commands/bin/shell/src/lexer.rs:76` | 理解一行字如何变成单词 | 08 §2.2 |
| K-084 | 七种展开与"变量子集优先"的排序理由 | 机制 | 存量 | §2.3 | `bin/sh/expand.c:138`、`:355`；`shell/src/expand.rs:86` | 知道 `${}` 与其余六种展开的依赖关系 | 08 §2.3 |
| K-085 | 重定向两层（算子识别 + 压栈弹出 discipline） | 数据结构 | 存量 | §2.4 | `bin/sh/redir.c:79`、`:107`、`:130`；`shell/src/redir.rs:61` | 看懂 `>>`/`>&`/`<>` 与描述符保存恢复 | 08 §2.4 |
| K-086 | 求值器与作业控制的进程原语依赖链 | 机制 | 存量 | §2.5 | `bin/sh/eval.c`、`bin/sh/jobs.c:288`、`:349` | 理解执行层为何必须等进程原语 | 08 §2.5 |
| K-087 | "先文字层、后执行层"的阶段取舍 | 架构演进 | 存量 | §3.1 | `os/commands/bin/shell/Cargo.toml`、`shell/src/lib.rs` | 明白无 std 前三关为何能立即产出 | 08 §3 |
| K-088 | `Environ` 只读接口与"响亮拒绝赋值展开" | 接口与协议 | 存量 | §3.2、§4.2 | `shell/src/expand.rs:39`、`:47`、`:60` | 知道变量存储归执行器 | 08 §3 |
| K-089 | `Word` 片段串表示（零拷贝与引用级别） | 数据结构 | 存量 | §3.3、§4.2 | `shell/src/lexer.rs:47`、`:69` | 理解 `a"b c"d` 为何是三片段 | 08 §3 |
| K-090 | 显式留白（语法树、执行器、作业控制）与归属 | 约束与不变量 | 存量 | §3.4、§5 | `bin/sh/eval.c`、`bin/sh/jobs.c` | 知道接口切在"单词串进、单词串出" | 08 §3 |
| K-091 | 测试要点（36 个）与执行层未覆盖清单 | 测试性质 | 存量 | §5 | `cargo test -p minix-shell` | 能核对哪些行为已锁 | 99 账本 |
| K-092 | 环境命令六件套（`env`/`printenv`/`getopt`/`sysenv`/`hostname`/`uname`） | 工具与工程 | 存量 | §1.5 | `usr.bin/{env,getopt,uname}`、`minix/commands/sysenv`、`bin/hostname` | 会构造干净环境、解析脚本选项 | 08 §1.6（本篇合法化，不再算越界） |
| K-093 | `machine`/`pagesize`/`domainname` 三条同族命令（现有 05 篇漏掉） | 工具与工程 | 新增 | — | `usr.bin/{machine,pagesize}`、`bin/domainname` | 补全"环境与平台自述"家族 | 08 §1.6 |
| K-094 | `LD_LIBRARY_PATH`/`TZ`/`PS1` 在真实配置文件里的样子 | 工具与工程 | 存量 | §1.3（隐含） | `etc/profile:7-17`、`etc/shrc:1-19`、`etc/skel/dot.profile:41` | 知道登录后环境实际被设成什么 | 08 §2.1 |

**K-095 … K-116｜来源：`06-file-ops.md`（229 行，34 命令）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-095 | 文件命令四大家族分类法（改属性/建关系/搬运查看/判断求值） | 概念 | 存量 | §1.0-1.4 | §1.0 分组表 | 建立全篇总图 | 09 §1（判断求值组移出到 10） |
| K-096 | 十二权限位与 `chmod` 双写法（八进制/符号） | 概念 | 存量 | §1.1 | `bin/chmod/chmod.c:166`、`:216`；`fileops/src/mode.rs:32` | 理解权限模型 | 09 §1、§2 |
| K-097 | 建关系家族（硬/软链接与引用计数、`unlink`、`mkdir`/`rmdir` 非空约束） | 概念 | 存量 | §1.2 | `usr.sbin/link/`、`usr.sbin/unlink/`、`bin/{ln,mkdir,rmdir}` | 分清别名与悬空、目录删除的栏杆 | 09 §1 |
| K-098 | `cp` 三形态 × 四种属性策略（深讲） | 机制 | 存量 | §1.3、§2.3 | `bin/cp/cp.c:1-548` | 抓"选项组合即复杂度" | 09 §2（深讲 1） |
| K-099 | `ls`（715 行）：体量来自列组合而非算法（深讲） | 机制 | 存量 | §2.3 | `bin/ls/ls.c:1-715` | 学会按选项分组读大命令 | 09 §2（深讲 2） |
| K-100 | `find` 表达式树与 `xargs` 搭档（深讲） | 机制 | 存量 | §1.3、§2.3 | `usr.bin/find/find.c:1-306`、`usr.bin/xargs/` | 理解目录树查询语言与参数拼接 | 09 §2（深讲 3） |
| K-101 | 查看/定位/并发小工具三组（`df`/`du`/`stat`；`pwd`/`basename`/`dirname`/`pathchk`；`sync`/`flock`/`mkfifo`/`mktemp`） | 工具与工程 | 存量 | §1.3、§2.4 | `fileops/src/path.rs:18`、`:40` | 一次记全空间/字符串/锁三类小命令 | 09 §1（空间与并发组）+ 10 §2（路径组） |
| K-102 | 判断求值家族与"退出码即返回值"哲学 | 概念 | 存量 | §1.4 | §1.4；`bin/test/`、`bin/expr/` | 理解脚本分支与进程哲学的根基 | 10 §1 |
| K-103 | `test.c` 四层递归求值与三类算子表（深讲） | 机制 | 存量 | §2.1 | `bin/test/test.c:105-130`、`:160-167` | 拿到递归下降求值器的教科书样例 | 10 §2（深讲 1） |
| K-104 | `chmod.c` 外壳薄、库厚（`setmode`/`getmode`）与小命令纯函数形态 | 架构演进 | 存量 | §2.2、§2.4 | `bin/chmod/chmod.c:166`、`:216` | 理解 NetBSD 风格分层与决策下沉 | 09 §2 |
| K-105 | 命令契约总表（表头写 34 命令、实际 35 行） | 接口与协议 | 存量 | §2.5 | §2.5 表 | 一表掌握职责/选项/输入输出/退出码 | 09 §4（改到 99 账本，按命令族拆） |
| K-106 | 纯函数优先策略与求值器同名同序设计 | 架构演进 | 存量 | §3.1-3.2 | `fileops/src/testexpr.rs:187-220` | 理解"先钉语义、后接系统" | 09 §3 + 10 §3 |
| K-107 | `FileTester` 接口双实现与 `who_mask` 掩码移交 | 接口与协议 | 存量 | §3.3-3.4 | `testexpr.rs:25`、`:71`、`:112`；`mode.rs:60` | 学会用接口隔离系统 | 10 §3 |
| K-108 | 关键类型与不变量（`u16` 权限位、四层求值、`FileTester` 五问、路径切分） | 约束与不变量 | 存量 | §4.1-4.3 | `fileops/src/{mode,testexpr,path}.rs` | 把行为边界钉成可测规则 | 09/10 §3 + 99 账本 |
| K-109 | 命令契约与 `Requires` 分层表（含 `cat` 的 `open` 阻塞状态） | 接口与协议 | 存量 | §4.5 | §4.5；`os/libs/minix-sys/src/vfs.rs:627`（`open_existing_via` 已落地） | 知道哪些已接线、哪些等系统调用 | 99 账本（去陈旧状态） |
| K-110 | 测试格局（74 个库测试 + 8 个已接线薄壳）与执行层留白 | 测试性质 | 存量 | §5 | `cargo test -p minix-fileops`；`fileops/src/bin/`（8 个） | 知道覆盖了什么、显式缺什么 | 99 账本 |
| K-111 | `chroot` 的根切换语义（现被 06 篇收留，但它是进程状态而非文件操作） | 概念 | 存量 | §1.3（列名） | `usr.sbin/chroot/`、`14-stage-runtime` 的根目录语义 | 理解改写进程根的意义 | 08 §1.6（移到 shell/进程环境面） |
| K-112 | `cat` 的"拼接到标准输出"语义与 `-n` 行号（现无散文归属） | 机制 | 存量 | §2.5（仅表行） | `bin/cat/cat.c` | 最常用命令的行为契约 | 09 §1（补家族） |
| K-113 | `mv` 的跨设备回退与 `rm` 的递归护栏（现无散文归属） | 机制 | 存量 | §2.5（仅表行） | `bin/{mv,rm}/*.c` | 补上两个最常用命令的语义 | 09 §1（补家族） |
| K-114 | `echo` 的"只有首位 `-n` 是标志、禁止 getopt"怪癖 | 约束与不变量 | 存量 | §1.4（提及） | `bin/echo/echo.c:61`；`fileops/src/echo.rs`、`src/bin/echo.rs` | 知道 echo 为何不能用 getopt 解析 | 10 §2（深讲 2） |
| K-115 | `printf` 的格式引擎（格式复用、`\c` 截停、base 0 解析、`check_conversion` 双警告） | 机制 | 存量 | §4.5（状态说明） | `usr.bin/printf/printf.c`；`fileops/src/printf.rs` | 理解 printf 是运行时引擎而非编译期格式 | 10 §2（深讲 3） |
| K-116 | `expr` 的优先级梯与 `:` 的锚定基本正则 | 机制 | 存量 | §4.5（状态说明） | `bin/expr/expr.y:107`、`:112`（`regcomp(..., REG_BASIC)`）、`:121`（`rm[0].rm_so == 0`） | 理解 expr 与正则的接口 | **12 §2**（并入正则消费者，见 §3.2 重复主题表） |

**K-117 … K-135｜来源：`07-text-filter.md`（226 行，表内 35 行含 3 个伪命令）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-117 | 文本即行序列、五组分类与 07→08 阅读顺序的教学理由 | 概念 | 存量 | §1.0、§1.5 | §1.0 | 建立行变换总图 | 11 §1 |
| K-118 | 取段家族（`head`/`tail` 不对称、`split`/`csplit`/`cut`/`paste`/`join`/`column`/`lam`/`comm`） | 概念 | 存量 | §1.1 | §1.1；`textfilter/src/window.rs:11`、`:22`、`:65` | 理解"读一点就停"与环形缓冲 | 11 §1 |
| K-119 | 整形家族（折行/制表位语义/方向/分流） | 机制 | 存量 | §1.2 | §1.2 | 掌握终端宽度与 tab 到位语义 | 11 §1 |
| K-120 | 统计家族（`wc` 四计数与"词"定义、`cksum`/`crc` 校验、数据发生器） | 概念 | 存量 | §1.3 | §1.3 | 背下"最大非空运行"与发生器用途 | 11 §1 |
| K-121 | 比较家族（字节/行/最小编辑脚本三级与"生成—检视—应用"闭环） | 概念 | 存量 | §1.4 | §1.4 | 理解差异工具谱系 | 11 §1 |
| K-122 | 压舱石三件套与转储可视（`sort`/`uniq`/`tr`、`hexdump`/`vis`/`unvis`） | 概念 | 存量 | §1.6 | §1.6 | 掌握管道常用三块 | 11 §1（拆成"排序去重"与"字符转换/转储"两组） |
| K-123 | `uniq.c` 状态机（`cflag`/`dflag`/`uflag` 三元状态） | 机制 | 存量 | §2.1 | `usr.bin/uniq/uniq.c:56`、`:188`、`:190`；`textfilter/src/uniq.rs:43` | 全阶段最小的完整状态机范例 | 11 §2（深讲 1） |
| K-124 | `cut.c` 选项串即契约（`b:c:d:f:sn`）与区间去重 | 机制 | 存量 | §2.2 | `usr.bin/cut/cut.c:88`；`textfilter/src/cut.rs` | 读懂选项串即功能清单 | 11 §2 |
| K-125 | `wc`/`head` 计数器两副面孔与分块投喂一致性 | 机制 | 存量 | §2.3、§3.2 | `textfilter/src/count.rs` | 理解流式计数跨块保持状态 | 11 §2 |
| K-126 | `tr.c` 集合五写法展开为 256 元表，再做映射/删除/压缩 | 机制 | 存量 | §2.4 | `usr.bin/tr/tr.c:58-125`；`textfilter/src/tr.rs:23`、`:86` | 掌握字符集编译→流式处理架构 | 11 §2（深讲 2） |
| K-127 | `sort.c`：排序语义与执行策略之分（外部归并留白） | 架构演进 | 存量 | §2.5 | `usr.bin/sort/sort.c:1-418`；`textfilter/src/sort.rs` | 分清"比什么"与"放不下怎么办" | 11 §2（深讲 3） |
| K-128 | `LineWindow` 一个接口两种实现（取头/取尾） | 接口与协议 | 存量 | §3.1 | `textfilter/src/window.rs:11`、`:22`、`:65` | 用统一接口消弭头尾结构差异 | 11 §3 |
| K-129 | `CharClass` 三形态接口与 `uniq` 不缓冲全文 | 接口与协议 | 存量 | §3.3-3.4 | `textfilter/src/tr.rs:23`、`uniq.rs:43` | 接口按语义形状划分 | 11 §3 |
| K-130 | 命令契约与 `Requires` 表（含 `look`/`ifdef`/`crc` 三处证伪） | 接口与协议 | 存量 | §4.5 | §4.5 | 掌握接线状态与三处证伪结论 | 99 账本（证伪结论保留在 11 §1 注记） |
| K-131 | 关键类型与不变量（窗口/计数/字段表/字符集/去重器） | 约束与不变量 | 存量 | §4.1-4.3 | `textfilter/src/` 各模块 | 把行算法边界写成可测不变量 | 11 §3 + 99 账本 |
| K-132 | 测试格局（229 个）与执行/策略层留白 | 测试性质 | 存量 | §5 | `cargo test -p minix-textfilter`；`src/bin/`（33 个） | 知道覆盖了什么 | 99 账本 |
| K-133 | `diff` 引擎（稠密等价类、`stone` 候选搜索、`unravel` 与六种输出格式） | 机制 | 存量 | §4.5（状态行） | `minix3/usr.bin/diff/diffreg.c`；`textfilter/src/diff.rs` | 理解两文件差异引擎的全链 | 11 §2（深讲 4） |
| K-134 | `patch` 引擎（hunk 解析、`locate_hunk`、fuzz 上限 2、偏移交替搜索） | 机制 | 存量 | §4.5（状态行） | `usr.bin/patch/pch.c`、`patch.c`；`textfilter/src/patch.rs` | 理解差异如何被安全应用 | 11 §2（深讲 5） |
| K-135 | 孤儿与错位清单（`pr` 无家族、`tsort` 不在家族、`comm` 双收、`tee` 错归、`units` 错归、`look`/`ifdef`/`crc` 伪命令） | 工具与工程 | 存量 | §1.1-1.6、§4.5 | `usr.bin/pr/`（实属 14 篇）、`usr.bin/tsort/`（实属 15 篇） | 让每个命令有唯一家族 | §3.2 重复主题表 + §3.3 越界主题表 |

**K-136 … K-150｜来源：`08-grep-sed.md`（190 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-136 | 从枚举到描述：正则描述"形状"而非"内容" | 概念 | 存量 | §1.1 | §1.1 | 建立描述语言的可复用直觉 | 12 §1 |
| K-137 | BRE/ERE 两种拼写对照与"保守/大方"助记 | 概念 | 存量 | §1.2 | §1.2 | 看懂 C 工具正则拼写差异 | 12 §1 |
| K-138 | 最左起点、最长终点、贪婪与 `.*` 陷阱 | 机制 | 存量 | §1.3 | §1.3 | 掌握匹配语义与调试钥匙 | 12 §1 |
| K-139 | 回溯灾难与汤普森构造 + 派克虚拟机 | 机制 | 存量 | §1.4 | `os/commands/usr-bin/regex/src/pattern.rs` | 理解引擎为什么必须线性 | 12 §1、§3 |
| K-140 | `grep`/`sed` 分工、选项三分类、退出码三值 | 概念 | 存量 | §1.5 | §1.5 | 掌握两工具选项面与脚本契约 | 12 §1 |
| K-141 | `grep.c` 标志变量区即目录、选项处理三类翻译 | 机制 | 存量 | §2.1 | `minix/usr.bin/grep/grep.c:67-86`、`:112-122`、`:281-451`、`:505` | 学会按标志注释读选项语义 | 12 §2 |
| K-142 | `util.c` 单点匹配与 `-w` 词边界 | 机制 | 存量 | §2.2 | `minix/usr.bin/grep/util.c:205`、`:186` | 找到匹配语义的唯一入口 | 12 §2 |
| K-143 | `sed compile.c` 地址文法与替换编译 | 机制 | 存量 | §2.3 | `usr.bin/sed/compile.c:122`、`:189-196`、`:339`、`:480` | 理解地址首字符集与替换分派 | 12 §2（深讲 1） |
| K-144 | `sed process.c` 空匹配强制推进（防 `s/x*/-/g` 死循环） | 机制 | 存量 | §2.4 | `usr.bin/sed/process.c:403-432`、`:418-422` | 理解替换引擎的终止不变量 | 12 §2（深讲 2） |
| K-145 | 为什么用虚拟机而不是回溯器 | 架构演进 | 存量 | §3.1 | `regex/src/pattern.rs:172`、`:177` | 理解命令级基础设施的硬要求 | 12 §3 |
| K-146 | 为什么模式内后向引用被响亮拒绝 | 约束与不变量 | 存量 | §3.2 | `regex/src/pattern.rs` | 理解语言正则性与引擎代价的取舍 | 12 §3 |
| K-147 | `grep` 退出码写成纯函数与真值表 | 接口与协议 | 存量 | §3.3 | `regex/src/grep.rs:139`；`grep.c:505` | 掌握脚本契约的"笨"写法价值 | 12 §3 |
| K-148 | `parse_subst` 返回消耗长度 + 字节取向架构注记 | 机制 | 存量 | §3.4-3.5 | `regex/src/sed.rs:53` | 理解无分配解析与多字节统一留白 | 12 §3 |
| K-149 | 关键类型与不变量、命令契约与测试（34 个） | 测试性质 | 存量 | §4、§5 | `regex/src/{pattern,matcher,grep,sed}.rs`；`src/bin/{grep,sed}.rs` | 拿到类型边界与覆盖盲区 | 99 账本 |
| K-150 | `sed` 动词面（`d`/`p`/`a`/`i`/`c`/`y` 仅被列举、未展开） | 机制 | 存量 | §1.5（仅列举） | `usr.bin/sed/compile.c:339`（动词分派表） | 补上 sed 的完整脚本语言面 | 12 §2（新增深讲 3） |

**K-151 … K-164｜来源：`09-editors.md`（160 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-151 | 行编辑器会话模型（地址 + 命令 + 修饰三段式、命令即脚本） | 概念 | 存量 | §1.1 | §1.1 | 理解 ed 交互范式与 sed 同源 | 13 §1 |
| K-152 | 地址七种命名法与逗号/分号的关键差异 | 机制 | 存量 | §1.2 | `editor/src/addr.rs:31`、`:72` | 掌握行命名语言 | 13 §1、§3 |
| K-153 | 全屏编辑器与 ed/mined 双编辑器分工 | 概念 | 存量 | §1.3 | `minix/usr.bin/mined/mined1.c:1-1774` | 理解光标驱动的代价 | 13 §1 |
| K-154 | 单步撤销：反操作记录与取舍 | 机制 | 存量 | §1.4 | `bin/ed/ed.h:84-87`、`bin/ed/undo.c:1-158` | 理解 undo 的内存/复杂度权衡 | 13 §1 |
| K-155 | `vi` 缺位与三方选型代价（悬置） | 架构演进 | 存量 | §1.5 | §1.5；本报告 §9.3 OQ-2 | 记住未决重大决策与判断依据 | 13 §1 + 99 ARCH 登记 |
| K-156 | `ed main.c` 命令分派 `exec_command` 与地址范围守门 | 机制 | 存量 | §2.1 | `bin/ed/main.c:285`、`:314`、`:465`、`:481`、`:618`、`:898` | 掌握地址→分派的执行主干 | 13 §2（深讲） |
| K-157 | 行操作六组函数与 `TextStore` 最小公分母 | 机制 | 存量 | §2.2 | `bin/ed/main.c:1051`、`:1100`、`:1137`、`:1181`、`:1217`、`:1242` | 理解行链表如何抽象成存储接口 | 13 §2 |
| K-158 | 缓冲/IO/撤销三层与 Rust 三模块同构 | 架构演进 | 存量 | §2.3 | `bin/ed/{buf,io,undo}.c`；`editor/src/store.rs` | 看到分层边界是两边独立收敛的结论 | 13 §2、§3 |
| K-159 | `re`/`sub`/`cbc`/`glbl` 的引擎/地址/拼接三层复用 | 架构演进 | 存量 | §2.4 | `bin/ed/{re,sub}.c`；`ed.h:56` | 看到跨篇复用的具体形态 | 13 §2 |
| K-160 | `TextStore` 接口双后端（`GapStore`/`LineTable`） | 接口与协议 | 存量 | §3.1 | `editor/src/store.rs:35`、`:54`、`:213` | 理解 workload 分野与接口统一 | 13 §3 |
| K-161 | 读行为什么是拷贝而不是借用 | 约束与不变量 | 存量 | §3.2 | `editor/src/store.rs:35` | 理解"接口迁就物理现实" | 13 §3 |
| K-162 | `Address{基址,偏移}`：类型跟着文法走 | 数据结构 | 存量 | §3.3 | `editor/src/addr.rs:31`、`:58` | 拿到文法与类型同构的范例 | 13 §3 |
| K-163 | 搜索地址解析但不求值（解析先行、响亮拒绝） | 架构演进 | 存量 | §3.4 | `editor/src/addr.rs:72` | 理解"解析先行、执行随后"策略 | 13 §3 |
| K-164 | 关键类型、不变量与测试（22 个） | 测试性质 | 存量 | §4、§5 | `editor/src/cmd.rs:74`；`cargo test -p minix-editor` | 知道覆盖盲区 | 99 账本 |

**K-165 … K-185｜来源：`10-doc-man-tools.md`（205 行，表内 35 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-165 | 从文字到文档的三道工序框架 | 概念 | 存量 | §1.0 | §1.0 | 建立印制层总图 | 14 §1 |
| K-166 | 排版家族（分页编号/断行整理/Minix 轻量链 `cawf`/`spell`/`prep`） | 概念 | 存量 | §1.1 | §1.1；`minix/commands/{cawf,spell,prep}` | 理解纯文本到页面的流水线 | 14 §1 |
| K-167 | 手册系统：节、目录顺序与渲染规则解耦 | 接口与协议 | 存量 | §1.2 | `etc/man.conf`（`_subdir`/`_build` 实测存在） | 理解 man 查找与渲染的配置驱动 | 14 §1、§2 |
| K-168 | 开发辅助家族四组（索引/消息/拆分/宏） | 概念 | 存量 | §1.3 | §1.3 | 认识源码周边小工具谱系 | 15 §1 |
| K-169 | 日历：格里高利改革参数化（缺失日 + 双闰规则） | 机制 | 存量 | §1.4 | `usr.bin/cal/cal.c:65-66`、`:94-137` | 理解"历史进代码"的范例 | 14 §2（深讲 3） |
| K-170 | 国际化推迟（A-8）：UTF-8 一统与"字节即字符" | 架构演进 | 存量 | §1.5 | §1.5；`usr.bin/{locale,mklocale,mkesdb,mkcsmapper}` | 记住统一留白口径 | 15 §1、§3 + 99 ARCH 登记 |
| K-171 | `man.c`/`manconf.c`：四步找页与配置指令解析 | 机制 | 存量 | §2.1 | `usr.bin/man/man.c:1-1088`、`manconf.c:1-272` | 理解决策下沉/执行上浮分层 | 14 §2（深讲 1） |
| K-172 | `makewhatis` 索引器与查询三姐妹共享数据库格式 | 机制 | 存量 | §2.2 | `libexec/makewhatis/makewhatis.c:1-1174`、`usr.bin/apropos/` | 理解离线索引/在线查询 | 14 §2（深讲 2） |
| K-173 | `cal.c` 双闰规则（儒略 vs 格里高利与改革切换） | 机制 | 存量 | §2.3 | `usr.bin/cal/cal.c:65-66`、`:94-137`；`doctools/src/cal.rs:63`、`:94` | 拿到双历法计算与分叉测试选例 | 14 §2（深讲 3） |
| K-174 | 职责级覆盖与命令契约总表（35 行） | 工具与工程 | 存量 | §2.4-2.5 | §2.5 | 一表掌握大工具边界 | 99 账本（14/15 两篇各取本族行） |
| K-175 | 配置解析保留原文（`_build` 管道不碎） | 约束与不变量 | 存量 | §3.1 | `doctools/src/manconf.rs:45` | 理解"解析只切结构、不碰内容"铁律 | 14 §3 |
| K-176 | `ManDb` 接口配空/切片双实现 | 接口与协议 | 存量 | §3.2 | `doctools/src/whatis.rs:79` | 复用等值/包含两查询的统一接口 | 14 §3 |
| K-177 | 双计数器 vs 查表：闰规则显式化 | 数据结构 | 存量 | §3.3 | `doctools/src/cal.rs:63`、`:94`、`:121` | 理解可解释的日期算法设计 | 14 §3 |
| K-178 | 明确留白四项（排版引擎/词典数据/国际化/找页执行） | 架构演进 | 存量 | §3.4 | §3.4 | 知道哪些不做、各归哪篇 | 14 §3、15 §3 |
| K-179 | `tsort` 的图排序语义（现同时出现在 07 与 10 两篇的表里） | 机制 | 存量 | §2.5（表行） | `usr.bin/tsort/`；`textfilter/src/tsort.rs` | 理解拓扑排序工具 | 15 §2（唯一归属，见 §3.2） |
| K-180 | `m4` 宏处理器与词法/文法两段实现 | 机制 | 存量 | §1.3（一句） | `usr.bin/m4/{parser.y,tokenizer.l}` | 理解宏展开语言的实现形态 | 15 §2（新增深讲） |
| K-181 | `gencat`/`msgc`/`mkstr`/`xstr` 的消息目录与字符串抽取 | 工具与工程 | 存量 | §1.3 | `usr.bin/{gencat,msgc,mkstr,xstr}` | 知道消息目录生成链 | 15 §1、§2 |
| K-182 | `ctags`/`lorder`/`fpr`/`fsplit`/`asa`/`indent`/`checknr`/`deroff`/`colcrt`/`soelim`/`ul` 的职责 | 工具与工程 | 存量 | §1.1、§1.3 | 各命令目录；`ul` 仅在表内 | 认识源码与排版辅助面 | 14 §1（排版侧）+ 15 §1（源码侧） |
| K-183 | `calendar` 只在表内、散文无家 | 工具与工程 | 存量 | §2.5（表行） | `usr.bin/calendar/` | 补上日历提醒命令 | 14 §1（并入日历家族） |
| K-184 | 关键类型与不变量 | 约束与不变量 | 存量 | §4.1-4.3 | `doctools/src/{manconf,whatis,cal}.rs` | 把配置/数据库/日历边界钉死 | 14 §3 + 99 账本 |
| K-185 | 测试格局（18 个）与执行层留白 | 测试性质 | 存量 | §5 | `cargo test -p minix-doctools` | 知道覆盖了什么 | 99 账本 |

**K-186 … K-200｜来源：`11-compress-archive.md`（168 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-186 | 三种"变小"（压缩/归档/编码）正交与发行叠加链 | 概念 | 存量 | §1.0 | §1.0 | 分清三件常被混称的事 | 16 §1 |
| K-187 | LZW 字典压缩思想（边读边建表、清空码 256、结尾码 257、变宽编码） | 机制 | 存量 | §1.1 | `minix/commands/compress/compress.c:20-42`、`:974`；`compress/src/lzw.rs:48`、`:102` | 理解字典同步与自适应位宽 | 16 §1、§2（深讲） |
| K-188 | 传输编码（3 字节→4 个六位组、`uu` 与 base64 同思想） | 机制 | 存量 | §1.2 | `usr.bin/uuencode/uuencode.c:63-64`、`:109-113` | 掌握二进制过纯文本通道 | 16 §1、§2 |
| K-189 | 归档：`pax` 多格式互转与 `shar` 归档即脚本 | 概念 | 存量 | §1.3 | `bin/pax/`、`usr.bin/shar/shar.sh` | 理解复杂性在格式兼容而非算法 | 16 §1、§2 |
| K-190 | 完整性：CRC32（`0xCBF43926` 握手值）与 Adler32 | 机制 | 存量 | §1.4 | `compress/src/checksum.rs:13` | 理解静默损坏防护与标准向量 | 16 §1、§2 |
| K-191 | 命令契约总表（10 命令） | 接口与协议 | 存量 | §1.5 | §1.5 | 一表掌握压/包/编/密语义 | 99 账本 |
| K-192 | `compress.c` 头部注释说明书与"哈希字典 + 变宽 + 满表清空"三件套 | 机制 | 存量 | §2.1 | `compress.c:20-42`、`:974` | 拿到字典算法的完整形态 | 16 §2（深讲 1） |
| K-193 | `uuencode.c` 小而全（双编码器分派与长度字节） | 机制 | 存量 | §2.2 | `uuencode.c:47`、`:63-64`、`:109-113` | 理解传统式与现代式同源 | 16 §2（深讲 2） |
| K-194 | `shar.sh` 归档即脚本与 Rust 逆过程解析 | 机制 | 存量 | §2.3 | `usr.bin/shar/shar.sh`；`compress/src/shar.rs:50` | 理解生成端与解析端分测 | 16 §2（深讲 3） |
| K-195 | 大块头职责级覆盖（`gzip`/`unzip`/`bdes`/`pax` 的边界） | 工具与工程 | 存量 | §2.4 | `usr.bin/gzip/gzip.c:1-2114`、`unzip.c:1-1074`、`bdes.c:1-1076` | 知道算法家族边界与后续归属 | 16 §1、§2 |
| K-196 | LZW 选开放寻址哈希与 20 位键打包的工程账 | 数据结构 | 存量 | §3.1 | `compress/src/lzw.rs:48` | 理解精确查找该配什么结构 | 16 §3 |
| K-197 | 宽度切换点 2 的幂减一：解码器提前一级对冲滞后一码 | 约束与不变量 | 存量 | §3.2 | `compress/src/lzw.rs:102`；`compress.c:974` | 掌握全阶段最硬的一块正确性论证 | 16 §3（必须保留规格 + C 双锚） |
| K-198 | 校验按形状立接口、编解码不硬捏接口 | 接口与协议 | 存量 | §3.3 | `compress/src/checksum.rs:13` | 学会按输入输出形状划接口 | 16 §3 |
| K-199 | 明确留白四项（格式封装/形变/BWT 链/DES） | 架构演进 | 存量 | §3.4 | §3.4 | 知道四项移交与各自归篇 | 16 §3 |
| K-200 | 关键类型、不变量与测试（26 个，含 256 全值跨宽度切换往返） | 测试性质 | 存量 | §4、§5 | `compress/src/{uu,lzw,shar,checksum}.rs` | 知道覆盖了什么 | 99 账本 |

**K-201 … K-218｜来源：`12-process-tools.md`（185 行，表内 28 命令）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-201 | 看护运行三视角（看进程、看会话、发信号） | 概念 | 存量 | §1.0-1.4 | §1.0 | 建立本篇地图 | 17 §1 + 18 §1（按数据源切成两篇） |
| K-202 | `ps` 的本质是列选择（`keyword.c` 宏表登记列全集） | 机制 | 存量 | §1.1、§2.2、§3.4 | `bin/ps/keyword.c:81`、`:94`、`:96`、`:114-197` | 明白 ps 输出由列定义驱动 | 17 §1、§2（深讲 1） |
| K-203 | Minix 群体视角三件套（`ministat`/`mtop`/`toproto`） | 概念 | 存量 | §1.1 | `minix/usr.bin/{ministat,mtop,toproto}` | 知道 ps 之外的补充工具 | 17 §1 |
| K-204 | 32 个信号（编号 1–32）及语义序列 | 数据结构 | 存量 | §1.2、§2.1、§4.2 | `sys/sys/signal.h:52-84`；`proctools/src/signal.rs:12` | 建立信号名号对照底座 | 17 §1、§2（深讲 2） |
| K-205 | `kill` 名号双通道（`-l` 列表、名转号、默认 `TERM`、挂起/终止特判） | 机制 | 存量 | §1.2、§2.1 | `bin/kill/kill.c:83`、`:105`、`:119-127`、`:178`、`:188-195` | 学会按名或按号发信号 | 17 §2 |
| K-206 | 信号参数边界（数字 0 与 33 以上非法；0 的语义被库拒收） | 约束与不变量 | 存量 | §1.2、§3.1、§4.2 | `proctools/src/signal.rs` | 避免把 0 当普通信号 | 17 §3 |
| K-207 | `utmp` 36 字节定宽记录（终端行 8、用户名 8、主机 16、时间 4；空用户名即已退出） | 数据结构 | 存量 | §1.3、§2.3、§4.2 | `lib/libc/compat/include/utmp.h:42-46`；`proctools/src/utmp.rs:52` | 读懂会话真相的数据源 | 18 §2（深讲 1，同时回链 06 §2.5） |
| K-208 | 记录解析三动作与撕裂写入必须报错 | 约束与不变量 | 存量 | §1.3、§2.3、§4.2 | `proctools/src/utmp.rs:73` | 保证会话记录不静默丢失 | 18 §3 |
| K-209 | `who` 系十命令是同一 36 字节的十种读法 | 概念 | 存量 | §1.3 | `proctools/src/utmp.rs`；`usr.bin/{who,w,last,users,finger,logname,tty}` | 归类会话工具族 | 18 §1、§2 |
| K-210 | 外围家族（时间、优先级、锁、IPC、通知） | 概念 | 存量 | §1.4 | `bin/date`、`bin/sleep`、`usr.bin/{time,nice,renice,nohup,lock,ipcs,ipcrm,logger,leave,from}` | 知道每类工具各管什么 | 17 §1（优先级/进程）+ 18 §1（时间/锁/消息）+ 05 §2.7（`shlock`） |
| K-211 | 28 命令契约表 | 接口与协议 | 存量 | §1.5 | §1.5 | 快速查询命令面约定 | 99 账本 |
| K-212 | 时长打印四形态与阈值分支（60/3600/86400） | 机制 | 存量 | §3.3、§4.2 | `proctools/src/ptime.rs:16` | 会读 `time` 输出 | 17 §3 |
| K-213 | 错误码贴源不贴惯（`NotFound` → 3 而非 2） | 约束与不变量 | 存量 | §3.2 | `proctools/src/lib.rs` | 脚本能区分名字打错与进程已退 | 17 §3 + 03 §4 |
| K-214 | `ProcessRow` 最小三字段与 `ProcessTable` 接口 | 架构演进 | 存量 | §3.4、§4 | `proctools/src/stable.rs:28`、`:73` | 理解接口最小化这一取舍 | 17 §3 |
| K-215 | 模块结构与 C 源码位置对应表 | 工具与工程 | 存量 | §4.1、§4.3 | `proctools/src/`（5 文件） | 快速定位实现 | 99 账本 |
| K-216 | 测试性质（16 个；抄表代码逐字对、边界测试密度与格式分支数成正比） | 测试性质 | 存量 | §5 | `proctools/src/signal.rs:104`、`ptime.rs:85` | 学会评审"抄表类"代码 | 99 账本 + 17 §5 |
| K-217 | `ps` 数据源的执行面（`kvm`/内核表读取）被显式留白 | 约束与不变量 | 存量 | §5 | §5 | 知道 ps 为何只有列定义 | 17 §3 + 99 账本 |
| K-218 | 孤儿清单：`umount` 在 14 篇头部有源码行但正文无节；`updateboot`/`update_asr` 在 17 篇头部有源码行但正文无家；`uptime` 系（`w` 的上半屏）无专节 | 工具与工程 | 存量 | 各篇头部 | 见 §3.3 越界主题表 | 让每条被点名的命令都有落点 | §3.2 GAP-7（孤儿命令统筹） |

**K-219 … K-231｜来源：`13-terminal-termios.md`（174 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-219 | 终端三层规矩（行属性、能力数据库、键盘字体屏幕工具） | 概念 | 存量 | §1.0 | §1.0 | 建立终端命名层地图 | 19 §1 |
| K-220 | 波特率 19 档标准速度表与"速度 0 即挂断" | 概念 | 存量 | §1.1、§2.3、§3.1 | `bin/stty/stty.c:137`；`termctl/src/baud.rs` | 不再误解 `stty 0` | 19 §1、§2 |
| K-221 | 控制字符三属性与 21 项表（主表 18 + 别名 3） | 数据结构 | 存量 | §1.2、§2.1、§3.2 | `bin/stty/cchar.c:60-82`；`sys/sys/termios.h:50-79`；`sys/sys/ttydefaults.h:65-69` | 能查改任意控制字符 | 19 §2（深讲 1） |
| K-222 | Minix 退格默认是 `Control-H`（`CERASE CTRL('h')`） | 约束与不变量 | 存量 | §1.2 | `sys/sys/ttydefaults.h:65-67` | 解释"删不动字符"的根因 | 19 §1、§2 |
| K-223 | 控制字符命令行记法（`^C`、`^?`、`undef`） | 接口与协议 | 存量 | §1.2、§4.2 | `termctl/src/cchar.rs` | 会写 stty 的控制字符参数 | 19 §2 |
| K-224 | 四组标志与 `modeset` 查表顺序决定重名归属 | 机制 | 存量 | §1.3、§2.2 | `bin/stty/modes.c:65-175`、`:208` | 理解 stty 标志词分组与解析同序要求 | 19 §2（深讲 2） |
| K-225 | 16 个常用标志词子集边界与"子集不是全集" | 架构演进 | 存量 | §2.2、§3.3 | `bin/stty/modes.c`；`termctl/src/stty.rs` | 知道子集声明的诚实性 | 19 §3 |
| K-226 | termcap 条目结构（别名 + 标志/数字/字符串三类能力）与真实条目 | 数据结构 | 存量 | §1.4、§2.4、§4.2 | `etc/termcap.big:12541`；`termctl/src/caps.rs:168` | 会读能力数据库行 | 19 §2（深讲 3） |
| K-227 | 能力查询分三方法与"无不是零" | 接口与协议 | 存量 | §3.4 | `termctl/src/caps.rs` | 正确区分缺席与零值 | 19 §3 |
| K-228 | 两代终端数据库工具链（`term`/`tget` 老式；`tput`/`tic`/`infocmp` 新式） | 概念 | 存量 | §1.4 | `minix/commands/{term,tget}`；`usr.bin/{tput,tic,infocmp}` | 分清两代工具 | 19 §1、§2 |
| K-229 | A-2 决策悬置（移植 terminfo 数据 + 解析器 vs 只封装转义序列） | 架构演进 | 存量 | §1.4、§5 | §1.4；`23-terminal-games.md §3.5` | 知道能力库实现的未定边界 | 19 §3 + 99 ARCH 登记 |
| K-230 | 键盘/字体/屏幕三小工具与 `etc/fonts` | 概念 | 存量 | §1.5 | `minix/commands/{loadkeys,loadfont,screendump}`；`etc/fonts` | 知道控制台定制工具面 | 19 §1 |
| K-231 | 速度打印格式与 `SttyOp` 解析规则、10 命令契约、测试（19 个） | 测试性质 | 存量 | §1.6、§2.3、§4、§5 | `bin/stty/print.c:71-73`；`termctl/src/stty.rs` | 会读 `stty -a` 并知道解析次序 | 19 §2、§3 + 99 账本 |
| — | **缺口**：`termcap` 被当作命令列进契约表，但 `minix3/` 中不存在 `termcap` 命令目录（只有 `etc/termcap` 数据库与 `lib/libterminfo/termcap.c`） | 工具与工程 | 新增 | — | `ls minix3/etc/termcap*`、`lib/libterminfo/termcap.c`；无 `termcap/` 命令目录 | 纠正一个幽灵命令 | 19 §1 注记（从契约表撤下） |

**K-232 … K-245｜来源：`14-mount-fsck.md`（140 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-232 | 挂载即"把文件系统根嫁接到目录"；`umount` 忙则拒绝是保护 | 概念 | 存量 | §1.1 | `minix/commands/umount/umount.c` | 建立挂载语义与摘除纪律 | 20 §1 |
| K-233 | 挂载选项是安全/语义契约（只读、禁执行、忽略特殊位、同步写、不更新访问时间、无设备文件） | 约束与不变量 | 存量 | §1.1、§3.1、§4.2 | `os/commands/sbin/mountinfo/src/options.rs` | 知道选项拼错的后果 | 20 §1、§3 |
| K-234 | 未知挂载选项必须响亮失败（`noexce` 教训） | 约束与不变量 | 存量 | §3.1 | `mountinfo/src/options.rs` | 理解"响亮失败"硬规则 | 20 §3 |
| K-235 | `ro` 查存在语义（`ro` 与 `rw` 并存即只读） | 约束与不变量 | 存量 | §4.2 | `minix/commands/mount/mount.c:143-147` | 会判定实际挂载权限 | 20 §3 |
| K-236 | `fstab` 六字段静态婚约（设备、挂载点、类型、选项、备份频率、检查序号） | 数据结构 | 存量 | §1.2、§4.2 | `mountinfo/src/fstab.rs` | 会读会写挂载表 | 20 §2（深讲 1） |
| K-237 | 检查序号 discipline（根 1 先行、余下升序、同号保表序） | 机制 | 存量 | §1.3、§2.2、§4.2 | `sbin/fsck/fsck.c:254`；`mountinfo/src/order.rs:27` | 理解开机体检顺序规则 | 20 §2（深讲 2） |
| K-238 | 检查序号 0 永不检查（内存/网络/伪文件系统） | 约束与不变量 | 存量 | §2.2 | `sbin/fsck/fsck.c:254` | 会解释哪些条目被跳过 | 20 §2 |
| K-239 | `preen` 无人值守两档（"自己吃药"与"叫医生"） | 概念 | 存量 | §1.3 | `sbin/fsck/preen.c` | 理解开机默认检查模式 | 20 §2 |
| K-240 | fsck 族一文件系统一检查器（通用、mfs 专用、ext2 专用） | 架构演进 | 存量 | §2.4 | `sbin/fsck/`、`sbin/fsck_ext2fs/`、`minix/commands/fsck.mfs/` | 理解 Unix 可扩展性设计 | 20 §2 |
| K-241 | `mount.c` 命令行形状（`-t`/`-o` 各消费一词、非列表模式恰三参数） | 接口与协议 | 存量 | §2.1 | `minix/commands/mount/mount.c:41-60`、`:168` | 会正确书写挂载命令 | 20 §2 |
| K-242 | 检查计划用稳定排序（16 行以内插入排序天然稳定） | 机制 | 存量 | §3.2 | `mountinfo/src/order.rs:27` | 理解"输入顺序有意义"是需求 | 20 §3 |
| K-243 | `MountTable` 接口三实现与首中获胜 | 数据结构 | 存量 | §3.3、§4.2 | `mountinfo/src/order.rs:69`、`:101` | 复用只读小库接口范式 | 20 §3 |
| K-244 | `newfstab.sh` 三段式与"表由安装问答算出" | 工具与工程 | 存量 | §1.2、§2.3 | `etc/newfstab.sh` | 理解 fstab 的机器友好来源 | 20 §2 |
| K-245 | `mount` 族旁支 `fattr.c` 与模块结构、测试（12 个） | 测试性质 | 存量 | §2.4、§4、§5 | `sbin/mount/fattr.c`、`sbin/mount/mountprog.h`；`mountinfo/src/order.rs:135` | 知道族边界与覆盖面 | 20 §2、§3 + 99 账本 |
| — | **缺口**：命令到 FS 服务端/块设备的调用路径（`mount` 如何请求 VFS、`fsck` 如何读块）四篇存储文档全部未讲 | 接口与协议 | 新增 | — | `mountinfo/src/` 只定义 Rust 侧消费接口；`15-stage-fs` 持有服务端 | 补上命令与服务的接缝 | 20 §2（新增节）+ 21/22/23 各自一条 |

**K-246 … K-259｜来源：`15-partition-format.md`（150 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-246 | MBR 首扇区布局（表在偏移 446、四项各 16 字节、魔数 `0xAA55` 在 510） | 数据结构 | 存量 | §1.1、§2.1、§4.2 | `sys/sys/bootblock.h:204-208`、`:703` | 读懂磁盘分区表物理布局 | 21 §2（深讲 1） |
| K-247 | 分区项字段（激活标志、起始几何三字节、类型码、结束几何、小端 32 位起始扇区与扇区数） | 数据结构 | 存量 | §1.1、§2.1 | `sys/sys/bootblock.h:703-743` | 能解析单个分区项 | 21 §2 |
| K-248 | 几何三字节存而不算（BIOS 寻址遗产仅供显示） | 架构演进 | 存量 | §1.1、§3.1 | `sys/sys/bootblock.h:703-714` | 不把几何字段当现代寻址依据 | 21 §3 |
| K-249 | 魔数不对即"非表"（坏与无的区分责任在调用方） | 约束与不变量 | 存量 | §1.1、§4.2 | `os/commands/sbin/diskfmt/src/mbr.rs` | 避免把新盘与坏盘混为一谈 | 21 §3 |
| K-250 | 类型码身份声明（`0x80`/`0x81` Minix、`0x83` Linux、`0x82` 交换、`0xA5` 386BSD 等；只命名常见 17 种） | 数据结构 | 存量 | §1.2、§3.2 | `sys/sys/bootblock.h:299-300`、`:328`；`diskfmt/src/mbr.rs` | 不冒险猜类型码 | 21 §2 |
| K-251 | 激活标志只认 0 与 `0x80`；空槽/零长/溢出（`checked_add`）一律无末扇区 | 约束与不变量 | 存量 | §4.2 | `diskfmt/src/mbr.rs` | 安全计算分区范围 | 21 §3 |
| K-252 | 起止换算"起加长减一"与表序排序思想 | 机制 | 存量 | §2.2 | `minix/commands/part/part.c:691-702` | 会算分区首末扇区 | 21 §2 |
| K-253 | 分区工具链五操作面（`fdisk` 交互、`part` 脚本、`partition` 显示、`autopart` 自动排布、`repartition` 重排） | 概念 | 存量 | §1.3 | `minix/commands/{fdisk,part,partition,autopart,repartition}` | 选对分区工具 | 21 §1 |
| K-254 | 准备链两端（`format` 低级格式化与 `devsize` 报设备大小） | 概念 | 存量 | §1.3 | `minix/commands/{format,devsize}` | 知道介质初始化与容量查询入口 | 21 §1（`format`/`devsize` 与 22 篇的分工需在契约里写死） |
| K-255 | 格式化家族（`newfs_ext2fs`/`newfs_msdos`/`newfs_udf`/`newfs_v7fs` + `makefs` + `mkfs`） | 概念 | 存量 | §1.4、§2.4 | `sbin/newfs_*`、`usr.sbin/makefs` | 知道新卷工具分工 | 21 §1 |
| K-256 | 人类单位解析（十进制 + 可选取 K/M/G、`x` 连乘左结合、checked 全程） | 机制 | 存量 | §1.4、§3.3 | `diskfmt/src/size.rs` | 不把多一位零算成吞盘事故 | 21 §3 |
| K-257 | 扇区换算 `sectors_for`（尾数进一、零字节/零扇区边界） | 机制 | 存量 | §4.2、§4.3 | `diskfmt/src/size.rs` | 正确把卷大小换成扇区数 | 21 §3 |
| K-258 | `fdisk` 表指针定位与 Rust 切片索引等价 | 工具与工程 | 存量 | §2.3 | `minix/commands/fdisk/fdisk.c:380` | 对照指针算术与安全索引 | 21 §2 |
| K-259 | `bootblock.h` 是布局权威来源（注释区/宏区/类型码区/结构区四段）与测试（9 个） | 测试性质 | 存量 | §2.1、§5 | `sys/sys/bootblock.h:150-182`、`:204-208`、`:279-328`、`:703`；`diskfmt/src/size.rs:58` | 学会从系统头抄布局常量 | 21 §2 + 99 账本 |

**K-260 … K-273｜来源：`16-image-media.md`（137 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-260 | 盘级搬运三件事（块复制、光盘镜像、内存盘与虚拟盘） | 概念 | 存量 | §1.0 | §1.0 | 建立搬运层地图 | 22 §1 |
| K-261 | `dd` 操作数语言 17 项 | 接口与协议 | 存量 | §1.1、§2.1 | `bin/dd/args.c:105-121` | 会读会写任意 dd 调用 | 22 §2（深讲 1） |
| K-262 | `dd` 块长后缀与连乘（`K`/`M`/`G`、`c`/`w`、`1Mx2` 左结合） | 机制 | 存量 | §1.1、§4.2 | `diskimg/src/dd.rs` | 精确表达块大小 | 22 §2 |
| K-263 | `dd` 的 `if`/`of` 缺省标准输入输出与"终极危险" | 概念 | 存量 | §1.1 | `bin/dd/args.c` | 建立使用 dd 的风险意识 | 22 §1 |
| K-264 | `conv` 转换六标志位与五个"识别但透传"的转换 | 约束与不变量 | 存量 | §2.1、§4.2 | `bin/dd/args.c:107`；`diskimg/src/dd.rs` | 知道哪些转换尚未实现 | 22 §3 |
| K-265 | `CopyPlan` 全量结算为字节（边界换算一次、循环里全是字节） | 架构演进 | 存量 | §3.2 | `diskimg/src/dd.rs` | 理解边界换算、内部统一规则 | 22 §3 |
| K-266 | ISO9660 主卷描述符布局（2048 字节扇区、类型 1、标识 `CD001`、卷标 40–71、块大小双端序各存一遍） | 数据结构 | 存量 | §1.2、§2.2、§4.2 | `minix/commands/isoread/isoread.c:41`；`diskimg/src/iso.rs` | 能识别并读取光盘卷 | 22 §2（深讲 2） |
| K-267 | 端序互校挡撕裂扇区与"标识即本质" | 约束与不变量 | 存量 | §2.2、§3.3 | `isoread.c:41`；`diskimg/src/iso.rs` | 不在信任起点放过损坏扇区 | 22 §3 |
| K-268 | `BlockDevice` 接口与两实现（零块长错、越界错、短尾不编造、空设备全缺席） | 数据结构 | 存量 | §3.1、§4.2 | `diskimg/src/device.rs:12`、`:24`、`:60` | 统一内存像与驱动盘的块寻址契约 | 22 §3 |
| K-269 | `ramdisk`/`loadramdisk`（安装盘、无盘站启动术） | 概念 | 存量 | §1.3 | `minix/commands/{ramdisk,loadramdisk}` | 知道内存盘启动路径 | 22 §1 |
| K-270 | `vnconfig`（把常规文件配成虚拟节点盘，是 mount 的前置动作） | 概念 | 存量 | §1.3 | `usr.sbin/vnconfig` | 不刻盘即可挂载镜像 | 22 §1 |
| K-271 | `rawspeed`（裸读写测速，存储性能基准尺） | 概念 | 存量 | §1.3 | `minix/commands/rawspeed` | 会做存储性能对照 | 22 §1 |
| K-272 | `dosread`（FAT 文件跨系统摆渡车；FAT 解析移交后续阶段） | 概念 | 存量 | §1.3 | `minix/commands/dosread` | 知道跨系统取文件入口与边界 | 21 §1（移出 22，属文件系统族） |
| K-273 | 介质三件与生成端（`vol`/`eject`/`cdprobe`/`writeisofs`）、模块结构与测试（13 个） | 测试性质 | 存量 | §1.2、§2.3、§4、§5 | `minix/commands/{vol,eject,cdprobe,writeisofs}`；`diskimg/src/iso.rs:93`、`device.rs:84` | 知道光盘操作工具面 | 22 §1、§2 + 99 账本 |

**K-274 … K-289｜来源：`17-backup-maintenance.md`（205 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-274 | 存好数据后的三类担心（丢失、堆积、介质）与"只讲判断语言"的边界 | 概念 | 存量 | §1.0 | §1.0 | 建立维护层问题地图 | 23 §1 |
| K-275 | 备份增量决策像 make（目标缺失则复制、源更新则刷新；只比修改时间与名字） | 机制 | 存量 | §1.1、§2.1、§3.1 | `minix/commands/backup/backup.c:3-11`；`maint/src/backup.rs` | 理解廉价近似与昂贵精确的取舍 | 23 §2（深讲 1） |
| K-276 | `backup` 十个选项字母各答一个实际问题 | 接口与协议 | 存量 | §1.1、§2.1 | `backup.c:14-25` | 会用备份选项表达策略 | 23 §2 |
| K-277 | `backup.c` 常量与错误码分级（`COPY_SIZE`/`MAX_ENTRIES`/`NONFATAL`/`FATAL`/`NO_SAVINGS`/`OUT_OF_SPACE`） | 工具与工程 | 存量 | §2.1 | `backup.c:52-58` | 知道缓冲/目录上限与错误分级 | 23 §2 |
| K-278 | `remsync` 三张记录表（pathname/namelist/file entry） | 数据结构 | 存量 | §2.2 | `remsync/remsync.c:101`、`:177`、`:285` | 理解远端同步的数据模型 | 23 §2（深讲 2） |
| K-279 | 硬链接跟踪（相同设备号加索引号只存一次内容） | 机制 | 存量 | §1.2、§2.2 | `remsync.c:285-290` | 不在同步中把一份文件变多份 | 23 §2 |
| K-280 | `synctree` 固定块传输与三指针目录记录 | 数据结构 | 存量 | §2.2 | `synctree/synctree.c:54-55`、`:200` | 理解慢链路分块搬运与树记录法 | 23 §2 |
| K-281 | `cleantmp` 午夜对齐保留窗口（`SEC_DAY`、隐藏名 14 天、隐藏线钳位） | 机制 | 存量 | §1.3、§2.3、§3.2 | `cleantmp/cleantmp.c:31-32`、`:56` | 得到可预期、可心算的清理结果 | 23 §2（深讲 3） |
| K-282 | 进度条版式（`WIDTH 77`、等号/竖线/减号、总数为零画空条避免除零） | 机制 | 存量 | §1.4、§2.4、§3.3 | `progressbar/progressbar.c:11`、`:34` | 会实现健壮的进度显示 | 23 §2 |
| K-283 | `mt` 磁带动作语言（十六命令词与别名、设备状态三档与感觉键表） | 接口与协议 | 存量 | §1.4、§2.5 | `mt/mt.c:42`、`:78`；`maint/src/tape.rs` | 会用顺序介质动作语言 | 23 §2（深讲 4） |
| K-284 | 磁带计数三档规则（忽略/接受零和正数/严格为正）与别名共享规则 | 约束与不变量 | 存量 | §1.4、§2.5、§4.2 | `mt/mt.c:40`；`maint/src/tape.rs` | 不再给命令传无效计数 | 23 §3 |
| K-285 | 状态查询是唯一"解释执行"命令（标记是类型系统的一位布尔值） | 架构演进 | 存量 | §2.5、§3.4 | `maint/src/tape.rs:129` | 理解本地分支与驱动分支的划分 | 23 §3 |
| K-286 | 日志轮转顺序（删最老一代、中间各代上移、当前日志压缩成第一代、再截空） | 机制 | 存量 | §2.6 | `rotate/rotate.sh` | 会推算轮转后的文件布局 | 18 §2（移到会话/运行期小工具或 25 篇的日志家族，见 §3.3） |
| K-287 | `fix` 差异三块（`a`/`d`/`c`）与先逐行核对后写、冲突即报错 | 机制 | 存量 | §2.6 | `fix/fix.c:10-14`、`:38` | 安全应用补丁而不硬写 | 11 §2（并入差异家族） |
| K-288 | 轻量手写参数解析与静态链接取舍 | 架构演进 | 存量 | §3.5 | §3.5；`maint/src/` | 理解系统命令的稳定性取向 | 03 §5（跨域约定）+ 16 §3 |
| K-289 | 测试性质（50 个）与"存储四篇"闭合口径冲突 | 测试性质 | 存量 | §5、§1.0 | `maint/src/cleantmp.rs:100` | 学会按不变量条数配测试 | 99 账本 + §3.2 重复主题表 |

**K-290 … K-303｜来源：`18-network-config.md`（185 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-290 | 连通四问框架（接口/路由/邻居/名字库）+ 两件诊断 | 概念 | 存量 | §1.0 | §1.0 | 拿到网络命令面的整体地图 | 24 §1 |
| K-291 | 接口配置语义（名字 + 标志集 + MTU；读改写两调用顺序） | 概念 | 存量 | §1.1、§2.1 | `sbin/ifconfig/ifconfig.c:1050`、`:1059`、`:1173` | 会读改网卡开关与 MTU | 24 §1、§2 |
| K-292 | 路由与邻居共用路由套接字（目标/网关/出接口、六动词、删查无放行） | 概念 | 存量 | §1.2、§2.2 | `sbin/route/route.c:1196`；`usr.sbin/arp/arp.c:315`、`:350`、`:434`、`:690` | 理解路由/邻居消息面与容错语义 | 24 §2（深讲 1） |
| K-293 | 回显探测语义（类型号 8/0、标识符 + 序列号配对、丢包与时延统计） | 接口与协议 | 存量 | §1.3、§2.3 | `sbin/ping/ping.c:897`、`:909`、`:1029` | 理解 ICMP echo 请求应答配对 | 24 §2（深讲 2） |
| K-294 | 反码校验和算法（16 位累加、奇尾补高位、进位折回、取反） | 机制 | 存量 | §1.3、§2.3 | `sbin/ping/ping.c:1266`（`in_cksum`） | 掌握互联网校验和的完整算法 | 24 §2 |
| K-295 | 路径追踪机制（UDP 探测、TTL 逐跳抬高、RTT 三统计、星号超时） | 机制 | 存量 | §1.3、§2.4 | `usr.sbin/traceroute/traceroute.c:472`、`:721`、`:1350` | 理解"断在哪一段"的定位原理 | 24 §2（深讲 3） |
| K-296 | 名字数据库三文件与首次命中获胜、坏地址响亮报错 | 概念 | 存量 | §1.4 | `etc/{hosts,services,protocols}` | 理解名字到地址/端口/协议号翻译 | 24 §1（回链 07 §2.4） |
| K-297 | 诊断与配置命令面（`netstat` 用法行、软中断队列、`netconf.sh` 顺序配置） | 工具与工程 | 存量 | §2.4 | `usr.bin/netstat/main.c:866`；`minix/commands/netconf/netconf.sh` | 知道状态显示与安装问答脚本 | 24 §1、§2 |
| K-298 | 标志字解析与控制调用分离 | 架构演进 | 存量 | §3.1 | §3.1 | 学会隔离系统相关调用面 | 24 §3 |
| K-299 | 路由表用定容数组（16 条），满表报错当背压 | 架构演进 | 存量 | §3.2 | §3.2 | 理解无堆环境的容量保护 | 24 §3 |
| K-300 | 校验按字节切片而非 16 位指针强转 | 架构演进 | 存量 | §3.3 | §3.3 | 学会避免强转陷阱 | 24 §3 |
| K-301 | 首次命中获胜写进查找实现、空库做成独立类型 | 架构演进 | 存量 | §3.4 | §3.4 | 学会用类型钉死查询语义 | 24 §3 |
| K-302 | 分层对照（命令只做翻译，选路转发在内核）与模块结构 | 架构演进 | 存量 | §3.5、§4.1 | `os/commands/usr-sbin/netconfig/src/{lib,iface,route,arp,ping,hosts}.rs` | 明确命令层与协议栈边界 | 24 §3 + 99 账本 |
| K-303 | 测试基线（31 个）与执行层留白 | 测试性质 | 存量 | §5 | `cargo test -p minix-netconfig` | 知道哪些行为已锁 | 99 账本 |
| — | **缺口**：ioctl 面在 `minix-sys` 无 wrapper（`14-stage-runtime/todo.md:60` 登记），本篇只说"命令只消费套接字封装" | 接口与协议 | 新增 | — | `14-stage-runtime/todo.md:60`；`netconfig/src/` 的纯函数面 | 说清命令与 ioctl 的接缝在哪 | 24 §3 + 99 账本 Requires 表 |

**K-304 … K-318｜来源：`19-network-services.md`（171 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-304 | 连通后三件事框架（超级服务器 / 日志 / 跑腿客户端） | 概念 | 存量 | §1.0 | §1.0 | 拿到服务经营的全局地图 | 25 §1 |
| K-305 | `inetd` 语义（服务表七列、等待模式、内置服务、`OPEN_MAX 64` 与 `MAXARGV 20`） | 概念 | 存量 | §1.1、§2.1 | `usr.sbin/inetd/inetd.c:276`、`:306`、`:340`、`:344`、`:405`、`:409` | 理解按需起服务的机制与上限 | 25 §2（深讲 1） |
| K-306 | 日志优先级模型（facility + severity、`DEFUPRI`/`DEFSPRI`） | 概念 | 存量 | §1.2、§2.2 | `usr.sbin/syslogd/syslogd.c:60`、`:1471`、`:1488`、`:1528` | 看懂每条日志的归类与轻重 | 25 §2（深讲 2） |
| K-307 | 选择符与八种动作（类别.严重度/星号/`none`；八去向；重复消息抑制） | 接口与协议 | 存量 | §1.2、§2.2 | `syslogd.c:130-137` | 会读 `syslog.conf` 分拣规则 | 25 §2 |
| K-308 | `fetch` 定位符语法（四段、缺省端口按方案补、两种写法并存） | 接口与协议 | 存量 | §1.3、§2.3 | `minix/commands/fetch/fetch.c:858-859` | 会用取文件命令的两种写法 | 25 §2（深讲 3） |
| K-309 | 会话准入流程（欢迎横幅、验用户名口令、策略判定、`chroot` 受限、登录记账） | 机制 | 存量 | §1.3、§2.3 | `libexec/ftpd/{ftpd.c,cmds.c,conf.c,logutmp.c,logwtmp.c}` | 理解登录守护共同的准入语义 | 25 §2（深讲 4） |
| K-310 | 跑腿客户端家族职责（取文件/传文件/远 shell/查名字/超文本/调制解调传输） | 工具与工程 | 存量 | §1.3、§2.3 | §2.3 | 知道六个守护各自管什么 | 25 §1 |
| K-311 | 队列语义（邮件先入队后投递、打印先排队后假脱机） | 概念 | 存量 | §1.3 | §1.3 | 明确邮件/打印的覆盖边界 | 25 §1 |
| K-312 | 服务表用借用切片、只读装载、线性首命中 | 架构演进 | 存量 | §3.1 | §3.1 | 学会为只读配置选零拷贝结构 | 25 §3 |
| K-313 | 严重度用枚举派生排序而非数字比较 | 架构演进 | 存量 | §3.2 | §3.2 | 学会让顺序语义编译期可查 | 25 §3 |
| K-314 | 认证失败统一报查无（防用户名枚举）、策略拒绝单独报 | 架构演进 | 存量 | §3.3 | §3.3 | 理解认证错误面的安全取舍 | 25 §3 |
| K-315 | 日志池只按动作分八格计数、不存文本 | 架构演进 | 存量 | §3.4 | §3.4 | 理解判定层与存储层解耦 | 25 §3 |
| K-316 | 分层对照（守护只查表/分拣/分发）与模块结构 | 架构演进 | 存量 | §3.5、§4.1 | `os/commands/usr-sbin/netservices/src/{lib,inetd,syslog,fetch,session}.rs` | 明确守护层职责边界 | 25 §3 + 99 账本 |
| K-317 | 测试基线（25 个）与执行层留白 | 测试性质 | 存量 | §5 | `cargo test -p minix-netservices` | 知道哪些行为已锁 | 99 账本 |
| K-318 | 孤儿：`telnetd`/`zmodem`/`fingerd`/`httpd` 只在头部与 §2.3 一句带过；邮件/打印"概念有家、实现无家" | 工具与工程 | 存量 | §2.3、§1.3 | `libexec/{telnetd,fingerd,httpd}`、`minix/commands/{zmodem,lp,lpd,mail}` | 让每个守护有落点 | §3.2 GAP-7（孤儿命令统筹） |

**K-319 … K-332｜来源：`20-minix-system.md`（175 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-319 | 观察系统四种目光（身份 / 时间 / 参数 / 构成） | 概念 | 存量 | §1.0 | §1.0 | 拿到系统信息命令面地图 | 26 §1 |
| K-320 | `version` 命令（打开版本文件打印内容） | 工具与工程 | 存量 | §1.1、§2.1 | `minix/commands/version/version.sh` | 知道最简单的身份命令 | 26 §1 |
| K-321 | `printroot` 根设备发现（`stat` 根得设备号、扫 `/dev`、只认块设备、命中即停、`UNKNOWN_DEV`） | 机制 | 存量 | §1.1、§2.1 | `minix/commands/printroot/printroot.c:25`、`:27`、`:45`、`:55` | 理解根设备定位与失败回退 | 26 §2（深讲 1） |
| K-322 | `readclock` 双钟同步（缺省硬件→系统、`-w` 反转、`-n` 预览、`-q` 安静、大写寄存器隐含写、10 次 × 5 秒重试） | 机制 | 存量 | §1.1、§2.2 | `minix/commands/readclock/readclock.c:55`、`:59`、`:73`、`:122`、`:164` | 掌握硬件钟/系统钟同步选项面 | 26 §2（深讲 2） |
| K-323 | `intr` 限时执行（前后台两路、`alarm` 布防先于 `exec` 故可继承） | 机制 | 存量 | §1.2、§2.3 | `minix/commands/intr/intr.c:19`、`:51`、`:140` | 理解脚本中给命令套时限的手法 | 26 §2（深讲 3） |
| K-324 | `sysctl` 参数树（点分名为路径、查询/赋值、名字检查、根带 `CTLTYPE_NODE` 按类型分流） | 概念 | 存量 | §1.3、§2.4 | `sbin/sysctl/sysctl.c:252`、`:520` | 理解内核与服务参数怎么查改 | 26 §2（深讲 4） |
| K-325 | 参数内存表定容 16 行（满表再写即错、同名替换幂等、空表拒绝赋值） | 约束与不变量 | 存量 | §1.3、§3.3 | §1.3 | 掌握参数表的容量与幂等约束 | 26 §3 |
| K-326 | `ldd` 语义变化（静态链接无动态链接器，改列归档成员；ELF32/64 靠文件头识别；每成员一行） | 架构演进 | 存量 | §1.4、§2.4 | `usr.bin/ldd/ldd.c:94-96` | 理解静态链接对工具语义的改写 | 26 §1、§2 |
| K-327 | 其余系统工具职责（`profile`/`sprofalyze`/`zic`/`zdump`/`lspci`/`i2cscan`/`eepromread`/`trace`/`dhrystone`/`worldstone`/`srccrc`/`playwave`/`recwave`） | 工具与工程 | 存量 | §2.4 | `minix/commands/{profile,sprofalyze,dhrystone,worldstone,srccrc,playwave,recwave}`、`usr.sbin/{zic,zdump,i2cscan}`、`minix/usr.bin/eepromread` | 知道系统工具家族还有哪些成员 | 26 §1（各自给一行职责 + 散文节） |
| K-328 | 时钟方向由写标志决定、寄存器访问隐含写、预览不拦截 | 架构演进 | 存量 | §3.1 | §3.1 | 学会把危险操作改成显式开关 | 26 §3 |
| K-329 | 根发现返回"找到与否 + 回退名" | 架构演进 | 存量 | §3.2 | §3.2 | 学会区分正常分支与程序错误 | 26 §3 |
| K-330 | 参数表定容 + 归档每成员一行（机器可读优先，无堆可用） | 架构演进 | 存量 | §3.3、§3.4 | §3.3、§3.4 | 学会无堆环境的输出与容量设计 | 26 §3 |
| K-331 | 观察命令缺省只读、改变动作显式且可预览 | 架构演进 | 存量 | §3.5 | §3.5 | 建立"只读观察"的工具设计观 | 26 §3 |
| K-332 | 模块结构、关键类型与测试（27 个）；`version`/`profile` 等清单外溢项无 Rust 模块 | 测试性质 | 存量 | §4、§5 | `os/commands/bin/sysinfo/src/{lib,clock,intr,rootdev,sysctl,ldd}.rs` | 掌握边界与覆盖盲区 | 26 §3 + 99 账本 |

**K-333 … K-346｜来源：`21-package-tools.md`（181 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-333 | 供给四事框架（包管理 / 引导块写入 / 构建依赖 / 汇编符号抽取） | 概念 | 存量 | §1.0 | §1.0 | 拿到软件供给面地图 | 27 §1 |
| K-334 | 三组包与确认跳过、包名字符规则 | 概念 | 存量 | §1.1、§2.1 | `minix/commands/pkgin_sets/pkgin_sets.sh:3-5` | 理解经验分组的装机流程 | 27 §2 |
| K-335 | 光盘仓库（路径模板、`pkg_summary.bz2`、先根后挂载点查找顺序、命中即设 `PKG_REPOS`） | 接口与协议 | 存量 | §1.2、§2.2 | `pkgin_cd/pkgin_cd.sh:7-8` | 会解释离线装机为什么先查根 | 27 §2 |
| K-336 | `installboot`（按文件系统类型分发三种阶段文件、32 空闲扇区要求、不足即失败不截断） | 机制 | 存量 | §1.3、§2.3 | `usr.sbin/installboot/installboot.c:61`、`:246`、`:272-276` | 理解引导块安装的类型与空间约束 | 27 §2（深讲，回链 04 的引导面） |
| K-337 | `mkdep` 依赖行形状（目标:依赖、缺省写 `.depend`、后缀表、续行） | 接口与协议 | 存量 | §1.4、§2.5 | `usr.bin/mkdep/mkdep.c:80` | 看懂构建依赖文件格式 | 27 §2 |
| K-338 | `genassym` 符号抽取（`名字 数值` 行、`-c`/`-f` 互斥、余参组成编译器命令） | 接口与协议 | 存量 | §1.4、§2.5 | `usr.bin/genassym/genassym.sh:42` | 看懂汇编符号到链接器的接口 | 27 §2 |
| K-339 | `mk.conf` 五行（`?=` 缺省赋值、点开头条件行归构建器） | 接口与协议 | 存量 | §1.4、§2.6 | `etc/mk.conf`（5 行） | 看懂构建配置的最小语法 | 27 §2 |
| K-340 | 其余工具（`gcov-pull` 4 MB 静态缓冲、`nbperf` 三散列方法 `chm`/`chm3`/`bdz`） | 工具与工程 | 存量 | §2.4、§2.5 | `minix/commands/gcov-pull/gcov-pull.c:16`、`usr.bin/nbperf/nbperf.c:139-144` | 知道覆盖率拉取与完美散列的工程约定 | 27 §2 |
| K-341 | 集合用枚举而非字符串透传 | 架构演进 | 存量 | §3.1 | §3.1 | 学会把选择收敛到类型 | 27 §3 |
| K-342 | 路径拼装写调用方缓冲、字符白名单挡路径穿越 | 架构演进 | 存量 | §3.2 | §3.2 | 学会无堆环境的字符串安全 | 27 §3 |
| K-343 | 空间不足与参数无效用不同错误号 | 架构演进 | 存量 | §3.3 | §3.3 | 学会错误号即接口 | 27 §3 |
| K-344 | 续行分组只报告首行、拼接留给调用方 | 架构演进 | 存量 | §3.4 | §3.4 | 学会判定层不分配内存 | 27 §3 |
| K-345 | 分层对照（供给工具只做准备，下载/写入/编译归执行层） | 架构演进 | 存量 | §3.5 | §3.5 | 明确构建辅助职责边界 | 27 §3 |
| K-346 | 模块结构、关键类型与测试（28 个）；`pkgin_all`/`postinstall` 只在头部点名 | 测试性质 | 存量 | §4、§5 | `os/commands/usr-sbin/pkgtools/src/{lib,sets,cdrepo,bootinst,depfile,symgen,mkconf}.rs` | 掌握边界与覆盖盲区 | 27 §3 + 99 账本 |

**K-347 … K-360｜来源：`22-stdio-games.md`（217 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-347 | 验收层框架（只依赖启动、标准输入输出、退出，故最早可验收） | 概念 | 存量 | §1.0 | §1.0；`99-global-concepts.md §1` | 理解游戏篇为何是交付链验收 | 28 §1 + 03 §3 |
| K-348 | `factor` 试除输出形状与平方根收尾 | 机制 | 存量 | §1.1、§2.1 | `games/factor/factor.c:184`、`:197`、`:268` | 掌握试除分解与输出契约 | 28 §2（深讲 1） |
| K-349 | `primes` 区间生成、倒置区间空、百以内表与六步轮转、上界一百万 | 机制 | 存量 | §1.1、§2.2 | `games/primes/{primes.c,pattern.c,pr_tbl.c,spsp.c}` | 掌握区间筛的语义与加速手段 | 28 §2（深讲 2） |
| K-350 | `caesar` 轮转表构造与溢出保护 | 机制 | 存量 | §1.2、§2.3 | `games/caesar/caesar.c:82`、`:86`、`:125` | 掌握轮转密码与溢出保护 | 28 §2 |
| K-351 | `morse` 静态码表、整串匹配解码、未知字符报查无 | 接口与协议 | 存量 | §1.2、§2.4 | `games/morse/morse.c:97`、`:140`、`:219` | 掌握点划码表与查无语义 | 28 §2 |
| K-352 | `pig` 猪拉丁语三规则与三档大小写保持 | 机制 | 存量 | §1.3、§2.5 | `games/pig/pig.c:103`、`:133` | 掌握约定俗成的文字规则 | 28 §2 |
| K-353 | `number` 数词分段规则与上限 999999 | 机制 | 存量 | §1.3 | `games/number/number.c` | 掌握数字到英文单词的分段规则 | 28 §2 |
| K-354 | `banner` 整幅宽 132 与 `-w` 列映射四舍五入 | 机制 | 存量 | §1.4、§2.6 | `games/banner/banner.c:58`、`:1052`、`:1057`、`:1064-1066` | 掌握大字横幅的缩放算法 | 28 §2（深讲 3） |
| K-355 | `arithmetic` 范围缺省与算术规则（减法截零、除法只出整除、乘法饱和） | 机制 | 存量 | §1.4、§2.5 | `games/arithmetic/arithmetic.c:99`、`:100`、`:107`、`:131` | 掌握测验规则与算术语义 | 28 §2 |
| K-356 | 打孔显示数据模型（`bcd` 十二位孔位表与十四行卡框；`ppt` 十一列纸带与进纸孔锚定） | 数据结构 | 存量 | §4.1、§4.2 | `games/bcd/bcd.c:87`、`:154`；`games/ppt/ppt.c:52`、`:130`；`stdio-games/src/{bcd,ppt}.rs` | 掌握打孔卡/纸带的编码几何 | 28 §2 |
| K-357 | 试除先用小素数表再用轮转（空间换时间甜点） | 架构演进 | 存量 | §3.1 | §3.1 | 理解试除引擎的加速取舍 | 28 §3 |
| K-358 | 摩斯码表与空表做成接口的两个实现 | 架构演进 | 存量 | §3.2 | §3.2 | 学会用类型隔离测试替身 | 28 §3 |
| K-359 | 命令契约与 `Requires` 表（10 命令的最小 API 与接线状态） | 接口与协议 | 存量 | §4.4 | §4.4；`stdio-games/src/bin/`（9 个） | 知道每个命令差哪个 API | 99 账本 |
| K-360 | 测试基线（102 个）与六项留白 | 测试性质 | 存量 | §5 | `cargo test -p minix-stdio-games`；实测 54 个 `#[test]` | 知道哪些行为已锁 | 99 账本 |
| — | **缺口**：`bcd`/`ppt` 在 §1 概念叙事中无位置（只在 §4 与测试出现）；§1.0 称"十个游戏"却只列七种玩法 | 导航 | 新增 | — | `stdio-games/src/{bcd,ppt}.rs` | 让玩法清单与命令清单一致 | 28 §1（补玩法分组） |

**K-361 … K-373｜来源：`23-terminal-games.md`（173 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-361 | 格子三件事框架（知道屏幕多大 / 光标送到格 / 格中写字擦字） | 概念 | 存量 | §1.0 | §1.0 | 拿到终端游戏的公共底座 | 29 §1 |
| K-362 | 屏幕模型（格子 + 高亮、内存屏/空屏、越界报错、坐标从 1 起、定容 132×50、三类转义序列） | 数据结构 | 存量 | §1.1、§2.2 | `games/worm/worm.c:120`、`:203-204`；`term-games/src/screen.rs` | 掌握绘制与测试共用的屏幕抽象 | 29 §2（深讲 1） |
| K-363 | `tetris` 形状（七种各四格、首格转轴、邻居偏移表、旋转转置取反、转四次恒等） | 数据结构 | 存量 | §1.2、§2.1 | `games/tetris/shapes.c:46-53`、`:56` | 掌握方块几何表示 | 29 §2（深讲 2） |
| K-364 | 吻合/放置/消行/计分（40/100/300/1200） | 机制 | 存量 | §1.2、§2.1 | `tetris/shapes.c:82`、`:97`；`tetris/tetris.c:62`、`:109-118` | 掌握棋盘核心操作与计分 | 29 §2 |
| K-365 | 棋盘尺寸差异（C 12×23 含墙，Rust 取可玩区 10×20） | 约束与不变量 | 存量 | §2.1 | `games/tetris/tetris.h:54-56` | 避免对照实现时尺寸误判 | 29 §3 |
| K-366 | `snake`（朝向四向、反转拒绝、普通走头进尾松、吃食走尾留长一格、撞墙/撞己死） | 机制 | 存量 | §1.3 | `games/snake/`；`term-games/src/snake.rs` | 掌握蛇的步进与自撞语义 | 29 §2（深讲 3） |
| K-367 | `crawler`/`rain`（跟进搬移共用、雨每帧下一格到底回顶、延时 1–999 ms 转微秒） | 机制 | 存量 | §1.3、§2.2 | `games/rain/rain.c:72-88`、`:117-118`；`term-games/src/crawler.rs` | 掌握爬行类三种走法 | 29 §2 |
| K-368 | `dungeon`（每层最多 9 房间、房间至少 3×3 贴边留墙、重叠带一圈墙余量、走廊先横后竖只把岩石变通道） | 机制 | 存量 | §1.4、§2.3 | `games/rogue/rogue.h:54`、`:293`；`games/rogue/room.c` | 掌握地牢几何与走廊开凿规则 | 29 §2（深讲 4） |
| K-369 | 屏幕用接口隔离（内存屏/空屏），逻辑不碰转义序列 | 架构演进 | 存量 | §3.1 | §3.1 | 学会让测试不解析转义串 | 29 §3 |
| K-370 | 旋转用坐标公式而非查表 | 架构演进 | 存量 | §3.2 | §3.2 | 学会公式优于表格的场景 | 29 §3 |
| K-371 | 普通走尾巴格不算自撞、吃食走尾留照算 | 架构演进 | 存量 | §3.3 | §3.3 | 理解帧内同步移动的判定细节 | 29 §3 |
| K-372 | 随机用接口加两个确定源，测试可复现 | 架构演进 | 存量 | §3.4 | `term-games/src/random.rs` | 学会消除测试 flake | 29 §3 |
| K-373 | 不移植 termcap/terminfo，只发三类基础序列（ARCH 决策）；模块结构与测试（32 个）；`colorbars` 是纯孤儿 | 架构演进 | 存量 | §3.5、§4.1、§5 | §3.5；`term-games/src/{lib,screen,tetris,snake,crawler,dungeon,random}.rs` | 理解目标终端单一时的取舍 | 29 §3、§1 + 99 账本（`colorbars` 补节） |

**K-374 … K-387｜来源：`24-text-games.md`（174 行）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-374 | 故事三件事框架（听得懂人话 / 讲得起规则 / 藏得住惊喜） | 概念 | 存量 | §1.0 | §1.0 | 拿到文本游戏的公共主题 | 30 §1 |
| K-375 | `adventure` 词汇表（动作/物品/行为三类词带编号、`HTSIZE 512`、查词不分大小写） | 数据结构 | 存量 | §1.1、§2.1 | `games/adventure/hdr.h:78` | 掌握探险游戏的理解层 | 30 §2（深讲 1） |
| K-376 | `adventure` 物品两表（编号 100 分界，普通物品记房间号、固定物品记固定点） | 机制 | 存量 | §1.1、§2.1 | `adventure/hdr.h:101`、`:116`；`adventure/vocab.c:72` | 掌握物品放置的双表模型 | 30 §2 |
| K-377 | `monop` 钱与骰子（付钱扣负即破产报错、收钱饱和、开局 1500、40 格回绕、三次双数入狱） | 机制 | 存量 | §1.2 | `games/monop/{monop.c,cards.c,houses.c,jail.c}` | 掌握棋盘走子与资金规则 | 30 §2（深讲 2） |
| K-378 | `fortune` 抽签（索引表定位、取余抽签、空库查无、百分号行分隔） | 机制 | 存量 | §1.3、§2.3 | `games/fortune/fortune/fortune.c:267`、`:980-981` | 掌握格言库的磁盘形状与抽取 | 30 §2（深讲 3） |
| K-379 | `acronym`/`wtf`（词条:解释 行、两边去空、不分大小写首命中、命令行 `is` 词跳过） | 接口与协议 | 存量 | §1.3、§2.4 | `games/wtf/wtf` | 掌握缩写查询的输入约定 | 30 §2 |
| K-380 | `fish`（13 点数各 4 张、四张成书、13 本书满局、要牌策略、点数英数两形） | 机制 | 存量 | §1.4、§2.2 | `games/fish/fish.c:61`、`:64`、`:83`、`:160` | 掌握纸牌成书玩法与策略 | 30 §2（深讲 4） |
| K-381 | `random` 过滤器（每行按分母倒数概率打印、种子混时钟/微秒/进程号、分母零拒绝） | 机制 | 存量 | §1.4、§2.4 | `games/random/random.c:107`、`:126` | 掌握按概率过滤文本的实现 | 30 §2 |
| K-382 | 词汇查表不分大小写、空词单独拒绝 | 架构演进 | 存量 | §3.1 | §3.1 | 理解玩家输入宽容原则 | 30 §3 |
| K-383 | 付钱破产报错、收钱饱和（不对称处理各有理由） | 架构演进 | 存量 | §3.2 | §3.2 | 区分规则事件与算术事故 | 30 §3 |
| K-384 | 格言抽签用取余而非拒绝超界 | 架构演进 | 存量 | §3.3 | §3.3 | 学会划定责任边界 | 30 §3 |
| K-385 | 牌面点数额认英文与数字两形、解析期统一成零起编号 | 架构演进 | 存量 | §3.4 | §3.4 | 学会把用户友好留在解析层 | 30 §3 |
| K-386 | 大文本数据进程序资源不进代码表（ARCH 决策） | 架构演进 | 存量 | §3.5 | `games/adventure/glorkz`、`games/fortune/datfiles/` | 理解数据与代码分离的长期收益 | 30 §3 + 99 ARCH 登记 |
| K-387 | 模块结构与测试（22 个）；`wargames.sh` 只在头部点名 | 工具与工程 | 存量 | §4.1、§5 | `text-games/src/{lib,adventure,monop,fortune,fish,acronym,lottery}.rs`；`games/wargames/wargames.sh` | 知道覆盖盲区与孤儿 | 30 §2（`wargames` 补节）+ 99 账本 |

**K-388 … K-400｜来源：`99-global-concepts.md`（89 行，自述"依赖契约三节已写，其余待改写"）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 去向 |
|---|---|---|---|---|---|---|---|
| K-388 | 文档状态与范围（三节成稿的 stub） | 导航 | 存量 | 头部 | `99-global-concepts.md:3-7` | 知道本篇只写完三节 | 03（改写后此条作废） |
| K-389 | 分层契约：Minix3 链条 vs minix-rs 替代（core/alloc + `minix-sys` 顶层 + `minix-rt`） | 架构演进 | 存量 | §1 | `os/libs/minix-sys/src/lib.rs`、`os/libs/minix-rt/src/{crt0,handoff}.rs` | 理解一条命令从上到下的依赖链 | 03 §1 |
| K-390 | 依赖方向三层硬规则（命令禁直接构造 IPC、`minix-sys` 顶层禁假设服务器实现、`ipc/types` 禁业务语义） | 约束与不变量 | 存量 | §1 表 | `99 §1`；`tools/check-command-boundary.sh`（已机制化） | 知道跨层调用的红线 | 03 §1 |
| K-391 | `minix-sys` 现有顶层函数的准确含义（`send`/`receive`/`sendrec`/`notify`/`fork`/`exec`/`exit`/`waitpid`/`kill`/`open`/`close`/`read`/`write`/`mmap`） | 接口与协议 | 存量 | §1 末 | `os/libs/minix-sys/src/lib.rs:108-231`（实测行号已漂移，须重取） | 防止等待一个不会出现的 libc stdio | 03 §1 + 99 账本 Requires 表 |
| K-392 | "stdio" 一词在 plan 中的准确落点 | 接口与协议 | 存量 | §1 末 | `../14-stage-runtime/plan.md:157`、`:273`（行号已漂移） | 防止等待不存在的库 | 03 §1 |
| K-393 | 行为判定基准（POSIX 是接口契约、Minix3 C 实现是真值；C 偏离 POSIX 按 C 写并标注） | 概念 | 存量 | §2 | `99 §2` | 知道争议时以谁为准 | 03 §2 |
| K-394 | 命令契约表模板（八列 + `Requires`） | 接口与协议 | 存量 | §3 | `99 §3`；`plan.md §3.6` | 知道新契约表怎么填 | 03 §3 + 99 账本 |
| K-395 | `Requires` 示范四命令与缺口（`echo` 齐备、`cat` API 齐备、`ls` 缺 `getdents`/`stat`、`sh` 缺 `dup2`/`pipe`/信号面） | 接口与协议 | 存量 | §3 表 | `99 §3`；`../14-stage-runtime/todo.md:60` | 知道哪个命令现在能不能动手 | 99 账本 |
| K-396 | 参见网络（plan 的各表、todo 缺口登记、14/17 plan 的分层与归属） | 导航 | 存量 | 参见 | `plan.md:149/199/253/386` | 找到判定依据与承接阶段 | 99 账本 |
| K-397 | [ARCH] A-1 用户态静态链接与 `ldd` 语义变化 | 架构演进 | 存量 | 核心点 2 | `99 §核心点-2`；`20-minix-system.md §1.4` | 知道静态链接波及哪些工具 | 03 §5 + 99 ARCH 登记 |
| K-398 | [ARCH] A-4 命令参数框架（自研参数解析 + `--help`/usage 约定） | 工具与工程 | 存量 | 核心点 3 | `99 §核心点-3` | 知道参数解析的统一约定 | 03 §4 |
| K-399 | [ARCH] A-5 退出码与 errno 约定 | 约束与不变量 | 存量 | 核心点 4 | `99 §核心点-4` | 知道退出码的映射规则 | 03 §4 |
| K-400 | 命令安装面（`bin`/`sbin`/`usr.bin`/`usr.sbin` 分层 + PATH）、`/etc` 配置约定、`DESCRIBE` 构建面、curses/terminfo 决策汇总 | 导航 | 存量 | 核心点 1、7 | `99 §核心点-1/7` | 知道这些面在哪、待写状态 | 02 §1（安装面）+ 03 §5（配置约定）+ 99（DECRIBE/ARCH 汇总） |

### 2.2 统计摘要

| 维度 | 数值 |
|---|---|
| 知识点总条数 | 441 条（§2.1 存量 400 条 + §2.4 覆盖审计追加 41 条） |
| 按类型分布（存量 400 条） | 概念 63、机制 118、数据结构 27、接口与协议 71、约束与不变量 42、架构演进 44、工具与工程 25、测试性质 21、导航 9（同一行多类型的按首个类型计入） |
| 按来源篇分布 | 00:16、01:16、02:14、03:16、04:15、05:17、06:22、07:19、08:15、09:14、10:21、11:15、12:18、13:13、14:14、15:14、16:14、17:16、18:14、19:15、20:14、21:14、22:14、23:13、24:14、99:13 |
| 新增条数（现有文档未覆盖） | 41 条（K-401 至 K-441），集中在：统一生命周期 8 条、安装与发现机制 8 条、跨篇约定 5 条、`rc.minix` 真实启动动作 2 条、`utmp` 三方契约 1 条、命令到 FS/块设备的接缝 4 条、孤儿命令补节 11 条、其余 2 条 |
| 无去向条数 | 0（每条都已给新编号；删除项见 §6 与 §9.2） |

### 2.3 重复与主讲述点

下表列出同一知识点在多个现有文档重复展开的情形，给出新目录中的**唯一主讲述点**，其余位置改为引用。

| 主题 | 重复位置 | 新主讲述点 | 其余位置的处理 |
|---|---|---|---|
| `utmp` 36 字节记录与会话真相 | `04-device-database.md §1.4`（作为"系统数据库"之一）、`12-process-tools.md §1.3/§2.3/§4.2`（作为读法）、`03-login-passwd.md §2.4`（只提"登录会计"） | **18 §2**（读者视角：谁在用系统） | 06 §2.5 讲写入侧（`login` 写、`rc.minix` 清），07 §2.4 只列文件名并指向 18 |
| 口令数据库（`master.passwd` / `passwd` / `pwd_mkdb`） | `03 §1.2`（两脸）与 `03 §2.6`（实例导读） | **06 §1、§2** | 两处合并为一处：§1 讲两脸的形状，§2 讲工具链，实例作为 §3 锚点 |
| `gettytab` 能力表 | `03 §2.3`（登录链的能力表）与 `04 §1.4`（越界的一行） | **06 §2.3** | 07 删除该行，改指针 |
| 主机名数据库的消费策略 | `04 §1.4`（`nsswitch.conf`）与 `18 §1.4`（hosts/services/protocols） | **24 §1**（网络侧消费）+ **07 §2.4**（文件形状） | 04/07 讲文件形状与 `getent`，24 讲查询策略与首次命中获胜 |
| `tsort` 的拓扑排序 | `07 §4.5`（表行）与 `10 §2.5`（表行） | **15 §2** | 11 从表中撤下，改指针；`textfilter` crate 的 `tsort.rs` 归属在 99 账本中标明"实现留在 textfilter，语义归 15" |
| `pr` 的分页排版 | `07 §4.5`（表行）与 `10 §1.1`（排版家族） | **14 §1** | 11 从表中撤下，改指针；`textfilter` crate 的 `pr.rs` 同上标注 |
| `fold` 的折行语义 | `07 §1.2`（整形家族）与 `10 §1.1`（排版家族，已注明"引用 07"） | **11 §1** | 14 保留"引用 11"的写法，不重复展开 |
| `comm` 的归并列 | `07 §1.1`（取段家族）与 `07 §1.4`（比较家族） | **11 §1**（比较家族（差异与并列）一组） | 取段家族撤下 |
| "存储四篇"的闭合口径 | `16 §1.0` 说 13/14/15/16；`17 §1.0` 说 14/15/16/17 | **20–23 四篇的 §1 统一写法**：本组四篇 = 挂载检查、分区格式化、镜像介质、备份维护 | 两处口径统一为同一句 |
| A-2 终端数据库决策 | `13 §1.4/§5`、`23 §3.5`、`99 §核心点-7` | **99 ARCH 登记表**（唯一裁决记录）+ 19 §3（该决策在终端篇的落地形态） | 29 §3 改为指针 |
| A-8 国际化推迟 | `10 §1.5/§3.4`、`99 §核心点` | **99 ARCH 登记表** | 15 §3 讲落地形态并指向 99 |
| A-1 静态链接 | `20 §1.4`（`ldd` 语义）、`99 §核心点-2` | **02 §1**（交付形态）+ **99 ARCH 登记表** | 26 §1 只讲 `ldd` 的行为变化 |
| `Requires` 概念与模板 | `99 §3`（模板 + 四示例）与 `06 §4.5`、`07 §4.5`、`08 §4.5`、`22 §4.4`（四个实例表） | **03 §3**（模板与规则）+ **99 账本**（22 篇合并成的总表） | 各篇不再自建表，只在正文里引用账本小节号 |
| 各篇 §4.1"模块结构"表与 §5 测试统计 | 24 篇各一份，全部陈旧（实测：06 写 4 文件实为 17、07 写 6 文件实为 65、08 写 5 文件实为 8；01 写 15 文件实为 20） | **99 账本** | 24 篇撤下模块表与测试统计，各留一行"本篇对应 crate 与账本小节" |

### 2.4 新增知识点（覆盖审计追加）

来源类型全部为"新增"，锚点为 C 源码、非 C 制品或操作系统理论出处；不受去向规则约束，直接进入 §5 的契约。

| 编号 | 名称 | 类型 | 锚点（证据） | 为什么归这一篇 | 去向 |
|---|---|---|---|---|---|
| K-401 | `_start` 交棒：汇编入口如何把 `ps_strings` 交给 C 运行时 | 机制 | `lib/csu/arch/x86_64/crt0.S:39-44`；`lib/csu/common/crt0-common.c:145-191` | 命令的第一条指令 | 01 §1 |
| K-402 | `argv`/`environ` 的来源（`ps_strings` 静态量，不经库） | 数据结构 | `crt0-common.c:154`、`:158`、`:191` | 命令唯一的参数与环境来源 | 01 §1 |
| K-403 | `getopt` 作为全命令共用的选项解析契约（短选项、`--`、`optind`/`opterr`） | 接口与协议 | `lib/libc/stdlib/getopt.c`、`include/getopt.h` | 326 条命令共享同一套解析 | 01 §2 |
| K-404 | 用法行（`usage`）与手册页 `SYNOPSIS` 的双向对应 | 接口与协议 | 349 个手册页；`minix/commands/svrctl/svrctl.c:107` | 命令自描述面的两支 | 01 §3 + 02 §3 |
| K-405 | 退出码是唯一的机器可读结论（`exit(main(...))` 语义） | 约束与不变量 | `crt0-common.c:191`；`proctools/src/lib.rs` 的 `NotFound`→3 | 命令对调用者的契约 | 01 §3 |
| K-406 | 一条命令的三种"身份"：程序名（`__progname`）、路径（PATH 命中项）、手册页名 | 概念 | `crt0-common.c:158`；`include/paths.h:45` | 把散落的三个面串成一问 | 01 §1 |
| K-407 | 命令的输入输出三条通道（标准输入/输出/错误）与"无 stdio 库"的现实 | 接口与协议 | `99-global-concepts.md §1`；`os/commands/bin/fileops/src/bin/echo.rs` | 输出通道是 `write` 而非 `printf` | 01 §2 + 03 §3 |
| K-408 | 命令的"决定半/执行半"分层（纯逻辑下沉、薄壳在上） | 架构演进 | `os/commands/bin/fileops/src/lib.rs:8-25`；`todo.md §6.1 C-1` | 全 stage 统一实现的架构决策 | 01 §3 + 99 账本 |
| K-409 | `BINDIR` 分层决定权限与 PATH，不决定语义归属 | 工具与工程 | `bin/Makefile.inc:7`、`sbin/Makefile.inc:7`、`usr.bin/Makefile.inc:5`、`usr.sbin/Makefile.inc:4` | 安装面的第一原理 | 02 §1 |
| K-410 | `SUBDIR` 清单即命令注册表 | 工具与工程 | `usr.bin/Makefile`、`minix/commands/Makefile` | 集合型 stage 的"注册机制" | 02 §1 |
| K-411 | `/usr/games/hide` 双层安装与 setgid 游戏（`games/Makefile.inc:6-18`） | 工具与工程 | `games/Makefile.inc:7`、`:17`、`:15`（`SYMLINKS+= dm /usr/games/${PROG}`） | 唯一一个非扁平的安装面 | 02 §1 |
| K-412 | 默认查找路径与救援路径（`_PATH_DEFPATH`、`RESCUEDIR`） | 接口与协议 | `include/paths.h:43`、`:45` | 集合型 stage 的"发现机制" | 02 §2 |
| K-413 | 手册页索引面（`man.conf` 的 `_subdir`/`_build`、`makewhatis` 离线索引）与人读命令的路径 | 接口与协议 | `etc/man.conf`；`libexec/makewhatis/makewhatis.c` | 命令被发现与被读懂的完整链 | 02 §3 |
| K-414 | `DESCRIBE` 构建面（设备清单由 `ls`+`sed` 生成，属构建而非运行） | 工具与工程 | `minix/commands/DESCRIBE/DESCRIBE.sh` | 与设备的静态/动态面呼应 | 02 §1 |
| K-415 | `[ARCH] A-1` 静态链接对交付形态的影响（无 `ld.elf_so`、无 `.so`、`ldd` 语义改写） | 架构演进 | `14-stage-runtime` 的 A-1；`usr.bin/ldd/ldd.c:94-96` | 交付形态的架构前提 | 02 §1 + 99 ARCH 登记 |
| K-416 | 命令二进制面尚未装配（24 个 crate 中 19 个是纯库、5 个带 `src/bin/`；无安装层把库变可执行文件） | 工具与工程 | `os/commands/` 实测；`os/Cargo.toml`；`todo.md §6.1 C-1` | 交付形态的真实现状 | 02 §1 + 99 账本 §5 |
| K-417 | 分层依赖的三条硬规则与机械守卫 | 约束与不变量 | `tools/check-command-boundary.sh:26-36`；`99 §1` | 跨篇约定的第一条 | 03 §1 |
| K-418 | `Requires` 列的定义与跨篇统计方法（同一 API 的消费者数决定补齐顺序） | 接口与协议 | `todo.md §2`（P1-1 处置）；`99 §3` | 契约表的第九列 | 03 §3 |
| K-419 | 参数框架约定（`[ARCH] A-4`：轻量自研参数解析 + `--help`/usage 约定，不引入 clap） | 工具与工程 | `plan.md §4 A-4`；`os/commands/*/src/*.rs` 的手写解析 | 全命令统一 | 03 §4 |
| K-420 | 退出码与 `errno` 映射约定（`[ARCH] A-5`；`EXIT_SUCCESS`/`EXIT_FAILURE` 与具体 errno 的分工） | 约束与不变量 | `plan.md §4 A-5`；`proctools/src/lib.rs` 的错误码映射 | 全命令统一 | 03 §4 |
| K-421 | 命令层单线程模型与阻塞原语的归属 | 约束与不变量 | `lib/libc/thread-stub/thread-stub.c`；`minix/lib/libmthread/pthread_compat.c:7-8`；`todo.md §4`；`edge4 §6` | 跨篇约定 | 03 §5 |
| K-422 | `rc.minix` 的真实启动动作（`fsck -x /`、`mount -a`、`utmp` 清零、十个服务域注册、`devmand` 启动） | 机制 | `etc/rc.minix:144`、`:145`、`:155-156`、`:159-179`、`:200-203`；入口 `etc/rc.d/minixrc:13` | 开机真正干活的一段 | 04 §2.7 |
| K-423 | `rc.d` 依赖图与空壳清单（五个脚本是空壳、四个是纯屏障） | 接口与协议 | `etc/rc.d/{fsck,mountcritlocal,mountcritremote,root,ttys}`（空壳）；`etc/rc.d/{NETWORKING,SERVERS,DAEMON,LOGIN}`（屏障） | 防止"32 个脚本"的表象误导 | 04 §2.6 |
| K-424 | `utmp` 三方契约（`login` 写、`rc.minix` 清、`who` 系读） | 数据结构 | `login/common.c:166+`；`etc/rc.minix:155-156`；`usr.bin/{who,w,last,users}` | 会话真相的完整生命周期 | 18 §2（主）+ 06 §2.5 + 04 §2.7 |
| K-425 | `mount`/`umount` 命令侧的调用接缝（请求形状与错误处理，不含服务端实现） | 接口与协议 | `minix/commands/mount/mount.c:41-60`、`:143-147`；`os/commands/sbin/mountinfo/src/options.rs` | 补上被整体推给 15-stage-fs 的那一半 | 20 §2 |
| K-426 | `fsck` 读块的接缝与 `preen` 两种退出语义 | 接口与协议 | `sbin/fsck/fsck.c:254`；`sbin/fsck/preen.c` | 同上 | 20 §2 |
| K-427 | `mkfs`/`newfs_*`/`makefs` 打开设备与写卷的接缝 | 接口与协议 | `sbin/newfs_*`、`usr.sbin/makefs`；`diskfmt/src/size.rs` | 同上 | 21 §2 |
| K-428 | `dd`/`isoread`/`vnconfig` 的块读写接缝与 `BlockDevice` 抽象 | 接口与协议 | `bin/dd/args.c:105-121`；`minix/commands/isoread/isoread.c:41`；`diskimg/src/device.rs:12` | 同上 | 22 §3 |
| K-429 | `cat` 的拼接语义与 `-n`（旧文献只在表内） | 机制 | `bin/cat/cat.c` | 最常用命令必须有散文家族 | 09 §1 |
| K-430 | `mv` 的跨设备回退与 `rm` 的递归护栏 | 机制 | `bin/{mv,rm}/*.c` | 同上 | 09 §1 |
| K-431 | `umount` 的忙判定与强制选项（旧文献头部有源码行、正文无节） | 机制 | `minix/commands/umount/umount.c` | 让被点名的命令有落点 | 20 §1 |
| K-432 | `updateboot`/`update_asr`/`update_bootcfg` 的引导刷新流程 | 机制 | `minix/commands/{updateboot,update_asr,update_bootcfg}` | 同上 | 23 §2 |
| K-433 | `telnetd`/`zmodem`/`fingerd`/`httpd` 四个守护各自的职责与 C 源位置 | 工具与工程 | `libexec/{telnetd,fingerd,httpd}`、`minix/commands/zmodem` | 同上 | 25 §2 |
| K-434 | 邮件与打印的命令侧语义（`mail`/`lp`/`lpd` 的队列与假脱机） | 概念 | `minix/commands/{mail,lp,lpd}` | 同上 | 25 §1 |
| K-435 | `version`/`profile`/`sprofalyze`/`zic`/`zdump`/`i2cscan`/`lspci`/`eepromread`/`trace`/`dhrystone`/`worldstone`/`srccrc`/`playwave`/`recwave` 各自的职责与锚点 | 工具与工程 | 各命令目录 | 同上 | 26 §1、§2 |
| K-436 | `pkgin_all`/`postinstall`/`gcov-pull`/`nbperf` 的职责与锚点 | 工具与工程 | `minix/commands/{pkgin_all,postinstall,gcov-pull}`、`usr.bin/nbperf` | 同上 | 27 §2 |
| K-437 | `colorbars` 的颜色条生成（旧文献全篇无落点） | 机制 | `games/colorbars/` | 同上 | 29 §1 |
| K-438 | `wargames` 的剧本文件与交互面 | 概念 | `games/wargames/wargames.sh`、剧本数据 | 同上 | 30 §2 |
| K-439 | `shlock` 锁原语与它在 `cron`/`at`/`backup` 三处的使用 | 机制 | `usr.bin/shlock/`；`minix/commands/{cron,at,backup}` 的调用点 | 调度器互斥的公共设施 | 05 §2.7 |
| K-440 | 发行集合与镜像装配（`distrib/sets/`、`releasetools/release.sh`）的边界声明 | 工具与工程 | `minix3/distrib/`、`minix3/releasetools/release.sh` | 交付形态的最外层，本 stage 只登记指针 | 02 §1（标注范围外） |
| K-441 | C 侧命令测试基建（`minix3/tests/` 的 ATF 测试树）作为对照来源 | 测试性质 | `minix3/tests/` | 与 Rust 侧 990 个测试对照 | 99 账本 §5 |

---

## 3. 覆盖审计

### 3.1 主题全集与来源

主题全集由四路汇成，逐项与知识点池、现有文档对账。

**来源一：C 源码符号面。** 七个源目录的每个命令程序（326 个）都已进入 §2 的池或 §3.1 的缺口表；命令内部的函数、结构体、宏、常量与错误路径按族收束（例如 `bin/ps/keyword.c` 的列宏表、`usr.bin/uniq/uniq.c` 的三元状态标志、`sys/sys/bootblock.h` 的分区布局宏）。按目录逐项核对结果：

| 源目录 | 目录数 | 已分配 | 未分配 | 分配到的篇章 |
|---|---|---|---|---|
| `bin/` | 30 | 30 | 0 | 08（sh/ksh/csh/hostname/domainname）、09（cat/chmod/cp/df/echo/expr/ln/ls/mkdir/mv/pwd/rm/rmdir/sync/test）、10（true/false/printf 系）、13（ed）、16（pax）、17（date/kill/ps/sleep）、19（stty）、22（dd）、25（rcmd/rcp） |
| `sbin/` | 19 | 19 | 0 | 04（init/rcorder/reboot/shutdown）、06（nologin）、07（mknod）、09（chown）、20（fsck/fsck_ext2fs/mount）、21（newfs_*）、24（ifconfig/ping/ping6/route）、26（sysctl） |
| `usr.bin/` | 141 | 141 | 0 | 06（chpass/id/passwd/pwhash/su/newgrp/login）、07（getent）、08（env/getopt/machine/pagesize/printenv/uname）、09（basename/dirname/du/false/find/flock/mkfifo/mktemp/pathchk/printf/stat/touch/true/xargs/xinstall）、10（echo/pwd/test 系）、11（文本行工具 34 个）、12（sed/expr）、14（apropos/cal/calendar/checknr/colcrt/deroff/fmt/fpr/gencat/indent/lorder/m4/makewhatis 系/man/nl/pr/soelim/tsort/ul/what/whatis/whereis/xstr/asa/fsplit/mkstr/menuc/msgc）、15（ctags/locale/mkcsmapper/mkesdb/mklocale）、16（bdes/bzip2/bzip2recover/gzip/shar/unzip/uudecode/uuencode）、17（finger/nice/nohup/renice/time/ipcs/ipcrm/who/w/last/users/mesg/tty/logname/wall/write/logger/from/leave/lock/shlock）、18（会话记录与运行期小工具）、19（infocmp/tic/tput）、24（netstat）、25（ftp/mail/rsh/telnet/whois）、26（ldd）、27（genassym/make/mkdep/nbperf） |
| `usr.sbin/` | 25 | 25 | 0 | 05（service）、06（pwd_mkdb/user/vipw）、07（dev_mkdb/mtree/services_mkdb）、09（chroot/link/unlink）、21（makefs）、22（vnconfig）、24（arp/ndp/rdate/rtadvd/traceroute/traceroute6）、25（inetd/syslogd）、26（i2cscan/zdump/zic）、27（installboot/postinstall） |
| `minix/commands/` | 81 | 81 | 0 | 04（setup）、05（at/atnormalize/cron/crontab/minix-service/svrctl/update）、07（MAKEDEV）、08（sysenv）、09（truncate）、11（crc/ifdef/look 三处证伪注记）、14（cawf/prep/spell）、16（compress）、19（loadfont/loadkeys/screendump/term/termcap 注记/tget）、20（fsck.mfs/mount/umount）、21（autopart/devsize/fdisk/format/part/partition/repartition）、22（cdprobe/dosread/eject/isoread/loadramdisk/ramdisk/rawspeed/vol/writeisofs）、23（backup/cleantmp/fix/mt/progressbar/remsync/rotate/synctree/update_asr/update_bootcfg/updateboot）、24（netconf/slip/swifi）、25（fetch/lp/lpd/mail/zmodem）、26（dhrystone/intr/lspci/playwave/printroot/profile/readclock/recwave/sprofalyze/sprofdiff/srccrc/version/worldstone）、27（gcov-pull/pkgin_all/pkgin_cd/pkgin_sets）、99（DESCRIBE 构建面）；排除 `devmand`（→ 11-stage-devman） |
| `minix/usr.bin/` | 8 | 8 | 0 | 11（diff）、12（grep）、13（mined）、17（ministat/mtop/toproto）、26（eepromread/trace） |
| `games/` | 24 | 24 | 0 | 28（arithmetic/banner/bcd/caesar/factor/morse/number/pig/ppt/primes）、29（colorbars/rain/rogue/snake/tetris/worm/worms）、30（adventure/fish/fortune/monop/random/wargames/wtf） |

**来源二：机制对应的操作系统通用概念。** 进程生命周期与状态机、地址空间与根目录、权限模型（十二位与 setuid）、文件描述符与重定向、信号与进程组、终端行规程、块设备与文件系统挂载、命名服务（主机/服务/协议）、队列与假脱机、压缩与校验和、正则语言与自动机。这些概念在 §2 中对应 K-017 至 K-046（进程与服务）、K-078 至 K-094（shell 与描述符）、K-095 至 K-116（权限与文件对象）、K-219 至 K-231（终端）、K-232 至 K-259（挂载与块布局）、K-290 至 K-318（命名与守护）、K-136 至 K-150（正则语言）。

**来源三：非 C 制品承载的主题。** 见 §3.5 逐项回答；其中构建与工具链、命令安装面、手册页索引三个主题现有文档完全没有覆盖，已作为新增知识点入池（K-009 至 K-016、K-400）。

**来源四：阶段边界契约里属于本 stage 的主题。** `plan.md` §5.4 排除表八项（devman server、RS server 端、驱动实现、`ld.elf_so`、libc 实现、lwip/uds、宿主构建工具链、图形栈）、`edge_todo.md` 的 E-CMDSYSFACE 与 E-SYSCALL-SIGN 两条命令侧挂账、`edge2.md` L10 的命令层 wrapper 族（已销账）。这些都在 §0.1 的范围外表与 §9.3 的待裁决问题中登记。

### 3.2 覆盖缺口表

| 编号 | 缺口 | 为什么重要 | 建议 | 追加入池 |
|---|---|---|---|---|
| GAP-1 | **命令的统一生命周期**（`_start` 交棒 → `argc`/`argv`/`environ` → `getopt` → 工作 → 退出码）没有任何一篇讲 | 326 条命令共享这一条链。缺了它，每一篇都要各自重复一遍"命令怎么拿到参数、怎么汇报结果"，而且读者永远看不到集合型 stage 的汇聚点。这也是"统一框架篇"在本 stage 完全缺位的直接证据 | **新建篇章 01**（命令的一生） | K-401 至 K-408（新增） |
| GAP-2 | **命令的注册与发现机制**（`BINDIR` 分层、`SUBDIR` 清单、`PATH` 查找、`/usr/games/hide` 双层安装、静态链接对交付形态的影响、349 个手册页与 `man.conf` 索引） | 这是集合型 stage 的"注册机制"与"发现机制"。现有 99 篇只在核心点里点了"安装面"一词，正文没有小节 | **新建篇章 02**（命令的交付与安装面） | K-409 至 K-416（新增） |
| GAP-3 | **跨篇工程约定的单一出处**：三层依赖硬规则、行为判定基准、退出码/errno 约定、参数框架、单线程模型、边界守卫机制 | 现有文档把这套约定拆在 `99 §1/§2/§3` 与四篇的 `§4.5`，而 `99` 排在目录最后——中段各篇引用它时构成前向引用（`07 §4.5` 第一句就是"沿用 `99-global-concepts.md §2` 的规则"） | **新建篇章 03**，把 99 §1/§2/§3 前移并补齐其余四项 | K-388 至 K-400（存量前移）+ K-417 至 K-421（新增） |
| GAP-4 | **`rc.minix`/`rc.cd`/`rc.capes`/`rc.shutdown` 的真实启动动作**：`fsck -x /`、`mount -a`、`utmp` 清零、`minix-service edit` 十个服务域、`devmand` 启动 | 现有 04 篇（旧 01）对这四个文件的提及次数为 0，而开机真正干活的正是它们；`rc.d` 里 32 个脚本多数是空壳（`fsck`、`mountcritlocal`、`mountcritremote`、`root`、`ttys` 实测为空壳） | 并入 **04 §2.7**（新增节），并把 `rc.d` 依赖图与"空壳列表"写进 §2.6 | K-422、K-423（新增） |
| GAP-5 | **`utmp`/`utmpx`/`wtmp` 三方契约**（写入者在 `login`，清空者在 `rc.minix`，读者在 `who` 系） | 这是"谁在用系统"的唯一真相源，被三篇文档各讲一半（旧 03 提一句、旧 04 越界一行、旧 12 讲读法） | 主讲述点定在 **18 §2**，写入侧在 **06 §2.5**，清空侧在 **04 §2.7** | K-424（新增）+ K-207 改主 |
| GAP-6 | **命令到 FS 服务端/块驱动的调用路径**（`mount` 怎么请求 VFS、`fsck` 怎么读块、`mkfs`/`part` 怎么打开设备） | 存储四篇（20–23）一律只讲参数解析与表语义，把执行面整体推给 `15-stage-fs`，但对方拿到的只有一句移交声明，没有接口对照材料 | 四篇各加一节"命令侧的调用接缝"，只写命令发出的请求形状与错误处理，不写服务端实现 | K-425 至 K-428（新增） |
| GAP-7 | **孤儿命令**：旧 06 的 `cat`/`mv`/`rm` 无散文家族；旧 07 的 `pr`/`tsort` 无家族；旧 10 的 `ul`/`calendar` 只在表内；旧 13 的 `colorbars` 全篇无落点；旧 14 的 `umount` 无 C 节；旧 17 的 `updateboot`/`update_asr` 无节；旧 19 的 `telnetd`/`zmodem`/`fingerd`/`httpd` 与邮件打印无落点；旧 20 的 `version`/`profile`/`sprofalyze`/`zic`/`zdump` 无节；旧 21 的 `pkgin_all`/`postinstall`/`gcov-pull`/`nbperf` 无落点；旧 24 的 `wargames` 无节 | "表中有、散文无"让读者无法判断该命令是否被覆盖；也违反 `plan.md §3.6` 的"契约表必须覆盖组内全部命令" | 逐条在新篇章里补家族与小节，并在 99 账本的命令总表中标出状态 | K-429 至 K-438（新增） |
| GAP-8 | **`termcap` 是幽灵命令**：旧 13 的契约表把它列为命令，但 `minix3/` 中没有 `termcap` 命令目录 | 契约表出现不存在的命令，读者按表去找会扑空 | 从契约表撤下，改在 §1 注记"termcap 是数据库与库，不是命令" | K-231 注记（不新增编号） |
| GAP-9 | **`shlock` 的归属**：现有文档只在 `12 §1.4` 的杂项里列了名字 | 它是 `cron`/`at`/`backup` 共用的锁原语，应该与调度器一起讲 | 并入 **05 §2.7**（新增节） | K-439（新增） |
| GAP-10 | **`machine`/`pagesize`/`domainname` 三条命令无任何文档覆盖**（旧 05 的 §1.5 只列了 `env`/`printenv`/`getopt`/`sysenv`/`hostname`/`uname`） | 它们与 `uname`/`hostname` 同族，属"环境与平台自述" | 并入 **08 §1.6** | K-093（新增，已入池） |
| GAP-11 | **发行集合与镜像装配**（`releasetools/release.sh`、`distrib/sets/`：哪些命令进哪个集合） | 决定"命令面最终以什么形态出现在安装介质上"，是交付形态的最外层 | 本 stage 只登记指针：**02 §1** 说明"发行集合不在本 stage，见构建链"；作为范围外发现登记 | K-440（新增，落在 02，标注范围外） |
| GAP-12 | **`minix3/tests/` 的命令测试基建** | 与 99 账本的"测试与守卫"一节直接相关：现有的命令测试全在 Rust 侧（990 个 `#[test]`），C 侧的 ATF 测试树无人提及 | 在 **99 账本 §5** 登记"命令行为的 C 侧测试基建不在本 stage 交付面，仅作对照来源" | K-441（新增） |

### 3.3 重复主题表

| 主题 | 现有重复位置 | 新主讲述点 | 其余位置改为 |
|---|---|---|---|
| `utmp` 记录与会话真相 | 04 §1.4、12 §1.3/§2.3/§4.2、03 §2.4（一句） | 18 §2 | 06 §2.5（写入侧）、04 §2.7（清空侧）、07 §2.4（只列文件名） |
| 口令数据库两张脸 | 03 §1.2 与 03 §2.6 | 06 §1、§2 | 合并同一篇内的两处 |
| `gettytab` | 03 §2.3、04 §1.4 | 06 §2.3 | 07 删行改指针 |
| 主机名字典的消费策略 | 04 §1.4、18 §1.4 | 24 §1（策略）+ 07 §2.4（文件形状） | 两处分工写清 |
| `tsort` | 07 §4.5、10 §2.5 | 15 §2 | 11 撤行改指针 |
| `pr` | 07 §4.5、10 §1.1 | 14 §1 | 11 撤行改指针 |
| `fold` | 07 §1.2、10 §1.1 | 11 §1 | 14 保留"引用 11"的写法 |
| `comm` | 07 §1.1、07 §1.4 | 11 §1 | 取段家族撤下 |
| "存储四篇"闭合口径 | 16 §1.0（13/14/15/16）、17 §1.0（14/15/16/17） | 20–23 四篇统一写法 | 两处改成同一句 |
| A-2 / A-8 / A-1 三个 ARCH 决策 | 13 §1.4/§5、23 §3.5、99 核心点、10 §1.5/§3.4、20 §1.4 | 99 ARCH 登记表 | 各篇只讲落地形态并指向 99 |
| `Requires` 概念 | 99 §3 + 06/07/08/22 的 §4.4/§4.5 | 03 §3（模板）+ 99 账本（总表） | 各篇撤表改引用 |
| 模块结构表与测试统计 | 24 篇各一份 | 99 账本 | 各篇撤表，留一行指针 |
| `head`/`tail` 窗口机制 | 07 §2.3、§3.2 | 11 §1、§3 | 合并到一处讲，§3 只讲接口 |
| 越界主题表条目（`ipcs`/`ipcrm`/`logger`/`date` 等） | 12 §1.4 与 §1.5 的契约表 | 17/18 两篇按数据源切分 | 见 §3.4 |

### 3.4 越界主题表

| 现有位置 | 越界内容 | 声明边界（头块）怎么说的 | 正确归属 |
|---|---|---|---|
| 05 §1.5 全节 | `env`/`printenv`/`getopt`/`sysenv`/`hostname`/`uname` 六件套 | 头块写"不覆盖：命令工具本体（见 06 到 24 各篇）"，但 `plan.md §5.2` 又把它们分配给 05 | **08 §1.6**（本篇合法化：它们是命令的执行环境面，与 shell 同篇） |
| 04 §1.4 | `gettytab` 一行 | 头块写"不覆盖：终端能力表（见 03 篇）" | **06 §2.3** |
| 04 §1.4 | `utmp` 一行 | 头块写"不覆盖：主机网络数据库的消费方（见 18 篇）"，未提 `utmp` | **18 §2**（主）+ **06 §2.5**（写侧） |
| 04 §1.4 | `nsswitch.conf` 的"先文件还是先网络"策略 | 头块写"不覆盖：主机网络数据库的消费方（见 18 篇）" | **24 §1** |
| 04 §6 | 交付因果链第一环的全链复盘 | 声明范围是设备与数据库 | 删除（00 承担全链） |
| 06 §1.3、§2.5 | `chroot` 只被列名，实为进程根状态 | 头块写"调用方是 shell 的内建与外部程序之分" | **08 §1.6**（进程环境面） |
| 06 §1.4 与 §4.5 | `echo`/`printf`/`test`/`expr` 四命令被放进"判断求值家族" | 头块写"表达式求值之外的 `test` 内建包装见 05 篇" | `echo`/`printf`/`test` → **10**；`expr` → **12**（正则消费者） |
| 10 §1.1、§2.5 | `pr`、`tsort`、`ul`、`calendar`、`fold` 与排版家族混编 | 头块写"不覆盖：排版引擎实现" | `pr` → **14**；`tsort` → **15**；`ul` → **14**；`calendar` → **14**；`fold` → **11** |
| 12 §1.4 | `ipcs`/`ipcrm`（System V IPC）、`logger`（syslog 客户端）、`leave`/`from`/`write`/`wall`（消息）、`date`/`sleep`/`time`（时间）五个不相关家族收在一节 | 头块写"不覆盖：终端控制（13）、网络会话（19）、进程检验的内核表读取" | `ipcs`/`ipcrm`/`time`/`nice`/`renice`/`nohup` → **17**；`logger`/`leave`/`from`/`write`/`wall`/`mesg`/`date`/`sleep`/`lock` → **18**；`shlock` → **05 §2.7** |
| 13 §1.5 | `loadkeys`/`loadfont`/`screendump` 三件设备控制工具 | 头块写"不覆盖：终端驱动（见 16-stage-drivers）" | 保留在 **19**（只讲职责与设备接口形状，实现指向驱动） |
| 12 §3.5（跨域工程政策） | "手写参数解析与静态链接取舍" | 属跨域政策，不是进程工具主题 | **03 §5**（跨域约定） |
| 14 §2.3 | `newfstab.sh`（安装器脚本） | 头块写"不覆盖：文件系统服务端实现" | 保留在 **20 §2**（它是 fstab 的机器友好来源） |
| 14 §2.4 | `fattr.c` 文件属性工具 | 同上 | 改指针到 **09**（文件对象属性面） |
| 16 §1.3 | `dosread`（FAT 文件读取） | 头块写"不覆盖：FAT 解析（后续文件系统阶段）" | **21 §1**（分区与格式化篇的文件系统邻域） |
| 17 §2.6 | `rotate`（日志轮转，属日志管理）、`fix`（差异修复，属补丁工具） | 头块写"不覆盖：文件系统遍历执行与时钟读取、磁带驱动 ioctl、屏幕重绘" | `rotate` → **18 §2**；`fix` → **11 §2** |
| 21 §1.3 | `installboot`（引导块安装，属存储/引导） | 头块写"不覆盖：宿主工具链、包传输执行与磁盘写入" | 保留在 **27 §2**，加一条回链到 **04** 的引导面 |
| 19 §1.3 末 | 邮件与打印只有语义声明 | 头块写"不覆盖：邮件投递与打印假脱机的后端执行" | 保留在 **25 §1**，并补 C 源锚点（`minix/commands/{mail,lp,lpd}`） |

### 3.5 非 C 主题逐项回答

任务规定的固定清单，逐项给出"在哪里讲"或"为什么不在本 stage"，不允许留空。

1. **链接与加载**。在 **02 §1** 讲：`[ARCH] A-1` 决定不实现 `ld.elf_so`、全部命令静态链接；`ldd` 的语义因此从"列动态依赖"变成"列静态归档成员"（`usr.bin/ldd/ldd.c:94-96`）。运行时的装载过程（`exec` 如何把映像放进地址空间）归 `14-stage-runtime`。在 **01 §1** 讲交棒：`lib/csu/arch/x86_64/crt0.S:39-44` 与 `lib/csu/common/crt0-common.c:145-191` 是命令进程的第一条指令，静态链接使这段代码成为唯一入口。
2. **镜像与内存布局**。命令面自身不做镜像布局；**02 §1** 说明发行形态（`BINDIR` 分层 + `games/Makefile.inc:7` 的 `/usr/games/hide` 双层安装 + 发行集合 `distrib/sets/` 属范围外，只登记指针）。内存盘镜像归 **22 §1**（`ramdisk`/`loadramdisk`/`vnconfig` 的语义），盘上引导块的物理布局归 **21 §2**（`sys/sys/bootblock.h:204-208`、`:703-743`）。
3. **汇编入口与陷阱进入**。命令侧只有 `_start`：`01 §1` 讲 `lib/csu/arch/x86_64/crt0.S:39-44` 与 `crt0-common.c:145-191` 的三参数交棒。陷阱入口（trap frame、系统调用门）归 `01-stage-kernel` 与 `14-stage-runtime`，本 stage 只在 **01 §1** 声明这条边界。
4. **启动装配**。整条交付链在 **04**（`init` → `/etc/rc` → `rcorder` → `rc.minix` → 服务 → getty）；`etc/` 的装配数据在 **04 §2.8**（`rc.conf`、`defaults/`、`boot.cfg.default`）与 **06 §2.2-2.3**（`ttys`、`gettytab`）；首次配置在 **04 §2.8**（`setup` 与 `bootconf.sh`）。
5. **构建与工具链**。分两半：系统内的构建辅助工具（`mkdep`、`genassym`、`nbperf`、`gcov-pull`、`mk.conf`）在 **27**；宿主构建工具链（`gcc`/`binutils`/`make`、`tools/awk`、`gnu/dist`）不在本 stage，**27 §1** 给边界声明。命令的**登记机制**（`SUBDIR` 清单、`BINDIR` 分层）在 **02 §1**——这是本 stage 必须回答的"构建面"，因为它决定命令的可执行形态。
6. **跨模块接口与线格式**。命令与服务的消息格式（IPC 请求/回复的线格式）归各服务 stage，本阶段只在 **03 §1** 讲依赖分层红线（命令不得直接构造 IPC 消息，`tools/check-command-boundary.sh` 机械检查）。命令自有的数据格式在本 stage 讲：`fstab` 六字段（**20 §2**）、`master.passwd`（**06 §1**）、`ttys`（**06 §2.2**）、`gettytab`（**06 §2.3**）、`utmp` 36 字节（**18 §2**）、MBR 分区表（**21 §2**）、ISO9660 主卷描述符（**22 §2**）、`pax`/`shar`/`uuencode` 的归档与传输格式（**16 §2**）、`termcap` 条目（**19 §2**）、man 页与 `whatis` 数据库（**14 §2**）、syslog 选择符（**25 §2**）、`inetd.conf` 七列（**25 §2**）、`crontab` 时间字段（**05 §1**）、`rc.d` 注释契约（**04 §2.6**）。
7. **错误路径**。**03 §4** 统一讲：退出码与 `errno` 的映射约定（`[ARCH] A-5`）、用法行的位置与形状、诊断输出的通道（`crt0-common.c:191` 的返回值如何成为进程退出码）。各篇的族内错误面在契约表里逐命令给出，例如 **20 §3** 的"未知挂载选项必须响亮失败"、**22 §3** 的"`conv` 五转换识别但透传"、**25 §3** 的"认证失败统一报查无"。
8. **关闭与退出**。三个层次：单条命令的退出（**01 §3**：`exit(main(...))` 的返回值语义）；会话的退出与重生（**04 §2.4**：`collect_child` 发现 getty 退出即重生）；系统的关闭与重启（**04 §2.3**：`SIGABRT`→`shutdown -r now`、`SIGUSR1`→断电、`shutdown`/`reboot` 命令，`etc/rc.shutdown` 的卸载流程）。
9. **并发与同步**。命令层是单线程（`minix3/lib/libc/thread-stub/thread-stub.c` 的单线程桩；`minix3/minix/lib/libmthread/pthread_compat.c:7-8` 的用户级绿线程不是 pthread 实现）；`futex` 在 `minix3/` 全树（排除 `external/`）零命中。这条在 **03 §5** 讲，并登记线程模型与阻塞原语的归属（`todo.md §4` 与 `edge4 §6` 已裁决归 `14-stage-runtime`）。命令自身的并发点只有：`init` 的会话表、`cron` 的派生、`inetd` 的按需 fork、`xargs` 的参数分批——分别在 **04 §2.4**、**05 §2.4**、**25 §2**、**09 §2** 讲。
10. **测试基建**。命令行为的验证手段在 **99 账本 §5** 集中说明：24 个域 crate 的 990 个 `#[test]`、每 crate 的验收命令（`cargo test -p minix-*`）、边界守卫脚本 `tools/check-command-boundary.sh`、宿主与真机的接缝（`edge E-SYSCALL-SIGN` 的假成功问题）、以及"命令二进制面尚未打包装进镜像"这一现状（19 个 crate 是纯库、5 个带 `src/bin/`）。C 侧 `minix3/tests/` 的 ATF 测试树不在本 stage 交付面，只在账本里作对照来源登记。

---

## 4. 新目录

### 4.1 新篇章总表

26 篇 → **32 篇**：新增 3 篇框架篇（01–03）与 1 篇账本（99）；把旧 10、旧 12、旧 06 各拆成两篇（净增 3 篇）；其余 20 篇保持一篇一题，编号整体后移。旧 `99` 的三节内容前移到新 `03`。

| 新编号 | 标题 | 一句话定位 | 分组 | 旧来源 |
|---|---|---|---|---|
| 00 | 命令面总览：规模、交付链与阅读路径 | 让读者知道这 326 条命令是什么、按什么顺序读、每一类归哪一篇 | 框架 | 旧 00（改写扩充） |
| 01 | 一条命令的一生：从交棒到退出码 | 讲清 326 条命令共享的那一条生命周期，以及这条链上每一环的 C 锚点 | 框架 | **新建**（GAP-1） |
| 02 | 命令的交付与安装面：注册、查找与交付形态 | 讲清命令从源码到可执行文件、从磁盘到被找到、从名字到手册页的三段交付链 | 框架 | **新建**（GAP-2） |
| 03 | 跨篇约定：依赖分层、行为判定基准与命令契约表 | 给出全 stage 唯一的一套判定规则与表格模板，供后面每一篇引用 | 框架 | **新建**（GAP-3，吸收旧 99 §1/§2/§3） |
| 04 | 启动初始化进程与启动脚本链 | 内核交权之后，谁把系统带到多用户可用状态 | 交付链 | 旧 01（补 `rc.minix` 等真实执行面） |
| 05 | 服务管理与定时调度 | 服务被谁照看，任务到点如何被执行 | 交付链 | 旧 02（并入 `shlock`） |
| 06 | 登录链路与口令数据库 | 终端上的人如何通过验证并拿到自己的 shell | 交付链 | 旧 03（`utmp` 写侧下放，主讲述点移到 18） |
| 07 | 设备节点与名字数据库 | 系统中的"名字"从哪来：设备节点、用户组、服务名、主机名 | 交付链 | 旧 04（越界三处移出） |
| 08 | shell 家族与命令执行环境 | 用户与系统之间的翻译官，以及它交给命令的执行环境 | 交付链 | 旧 05（§1.5 合法化为执行环境面，补三条命令） |
| 09 | 文件对象操作命令 | 查看、搬动、改属性、建关系、试真值：文件系统上的日常动作 | 命令族 | 旧 06（前半，补 `cat`/`mv`/`rm`） |
| 10 | 路径、真值与输出工具 | 在脚本里做路径计算、条件判断与输出 | 命令族 | 旧 06（后半：`pwd`/`basename`/`dirname`/`pathchk`/`test`/`true`/`false`/`echo`/`printf`） |
| 11 | 文本行工具 | 把文本当行序列来取、整、比、排、转、转储 | 命令族 | 旧 07（家族重划，撤下 `pr`/`tsort`，并入 `fix`） |
| 12 | 正则语义与消费者 | 用一种描述代替无数行的枚举，grep、sed、expr 三个消费者共用一套契约 | 命令族 | 旧 08（并入 `expr`） |
| 13 | 编辑器 | 住在行里改（`ed`）与在屏幕上改（`mined`） | 命令族 | 旧 09（不变） |
| 14 | 排版与手册 | 从纯文本到能看的页面，以及命令自己的说明书系统 | 命令族 | 旧 10（前半：排版/手册/日历） |
| 15 | 开发辅助与国际化工具 | 围绕源码的小工具与显式推迟的国际化决策 | 命令族 | 旧 10（后半：`ctags`/`lorder`/`m4`/`gencat`/`msgc`/i18n 四件） |
| 16 | 压缩与归档 | 三种"变小"与两种"打包" | 命令族 | 旧 11（不变） |
| 17 | 进程与信号工具 | 看进程在干什么，以及如何让它停、让它换优先级 | 命令族 | 旧 12（前半：`ps`/`kill`/`nice`/`nohup`/`ipcs` 等） |
| 18 | 会话记录与运行期小工具 | 谁在用系统，以及会话之间如何互相打招呼 | 命令族 | 旧 12（后半：`who` 系 + 消息 + 时间 + `logger` + `rotate`） |
| 19 | 终端控制与能力数据库 | 终端的行属性、能力表，以及键盘字体屏幕三件工具 | 命令族 | 旧 13（撤下幽灵命令 `termcap`） |
| 20 | 挂载与检查 | 把存储嫁接到目录树，并按序号排队做体检 | 命令族 | 旧 14（补 `umount` 节与调用接缝） |
| 21 | 分区与格式化 | 切分、命名、格式化：卷从哪来 | 命令族 | 旧 15（并入 `dosread`） |
| 22 | 镜像与介质 | 搬运整个盘：块复制、光盘镜像、内存盘与虚拟盘 | 命令族 | 旧 16（不变） |
| 23 | 备份与维护 | 数据存好之后的三类担心：丢失、堆积、介质 | 命令族 | 旧 17（`rotate`/`fix` 移出，补引导刷新脚本） |
| 24 | 网络配置与诊断 | 连通之前要回答的四个问题与两件诊断工具 | 命令族 | 旧 18（不变） |
| 25 | 网络服务与守护 | 连通之后的三件事：超级服务器、日志、跑腿客户端 | 命令族 | 旧 19（补四个守护与邮件打印锚点） |
| 26 | Minix 特有工具与系统信息 | 观察系统自己：身份、时钟、参数、构成 | 命令族 | 旧 20（补齐十四个点名命令） |
| 27 | 软件供给与构建辅助 | 装好系统之后，软件从哪里来 | 命令族 | 旧 21（不变） |
| 28 | stdio 游戏 | 只用说话证明系统活着 | 命令族 | 旧 22（补 `bcd`/`ppt` 玩法位） |
| 29 | 终端游戏 | 在格子上动起来 | 命令族 | 旧 23（补 `colorbars`） |
| 30 | 文本游戏 | 留住人的三件事：听懂、讲规则、藏惊喜 | 命令族 | 旧 24（补 `wargames`） |
| 99 | 实现账本与全局索引 | 命令总表、篇章↔crate 映射、`Requires` 总表、ARCH 登记、测试与守卫、排除表 | 账本 | **新建**（吸收 `plan.md §5` 的覆盖契约与 `todo.md` 的状态登记） |

**编号位的分配理由（为什么是 +3 与 +4）。** 框架三篇必须在命令族之前（否则中段各篇引用约定构成前向引用，见 §3.2 GAP-3），交付链必须紧跟框架（它讲的是系统如何变成"可以敲命令"的状态），命令族必须在两者之后。旧 01 至 05 顺移为 04 至 08；旧 06 之后因为三次拆分（旧 06 拆成 09/10、旧 10 拆成 14/15、旧 12 拆成 17/18），位移逐段累加，到旧 24 落在 30。末尾 99 的编号位保留给"读完全部之后再来看"的账本，与旧 99 的位置习惯一致。

### 4.2 阅读路径

**主线（必读，按编号顺序，约 20 篇）**

`00` 总览 → `01` 一条命令的一生 → `02` 交付与安装面 → `03` 跨篇约定 → `04` init 与启动脚本链 → `05` 服务与调度 → `06` 登录链路 → `07` 设备与名字数据库 → `08` shell 与执行环境 → `09` 文件对象操作 → `10` 路径、真值与输出 → `11` 文本行工具 → `12` 正则语义 → `20` 挂载与检查 → `24` 网络配置 → `99` 账本。

这条线的判据是"一条命令从被找到、被执行、到把结果写出去所必须知道的全部前提"：前四篇给规则，第八篇给执行环境，第九至十二篇给最常用的三类命令，第二十与二十四篇给两类最常被问起的系统动作。

**支线 A（实现者，按 crate 与批次读）**

`01 §3`（决定半/执行半）→ `99 §1`（命令总表看接线状态）→ `99 §3`（`Requires` 查缺口）→ 目标篇章的族内契约表 → 对应 crate 的 `//!` 头注释。这条线对应 `plan.md §6` 的实施批次（stdio 批 → shell 批 → 文档归档进程批 → 终端批 → 存储批 → 网络批 → 系统批 → 交付链收尾）。

**支线 B（存储与网络管理员）**

`20` → `21` → `22` → `23`（存储四篇，按"先用后备"的顺序读，见 §4.3 序差表）与 `24` → `25`（网络两篇）。这两组都要求先读 `03`，其余可跳。

**支线 C（终端与游戏）**

`19` 终端控制 → `29` 终端游戏；`28` 与 `30` 只依赖 `10`（输入输出）与 `11`（文本），可以在读完第十篇之后立刻跳读。

**可跳读清单（不影响主线理解）**

`15` 开发辅助与国际化（i18n 是显式推迟项）、`26` 中 `playwave`/`recwave`/`i2cscan`/`eepromread`/`trace` 五条延续候选、`27` 的构建辅助面、`28`/`29`/`30` 三篇游戏。`99` 账本可按需查表，不必顺序读。

### 4.3 序差表（运行时序与讲述序的差异）

| 编号 | 运行时序事实（带锚点） | 讲述序选择 | 理由 | 回指补偿位置 |
|---|---|---|---|---|
| 序差-1 | 文本行工具（`head`/`tail`/`cut`/`tr`）在运行时不依赖正则引擎；正则引擎（`grep`/`sed`）独立 | 讲述序把 `11` 文本行工具排在 `12` 正则语义之前 | 教学上先具体后抽象：读者先熟悉"按行处理"的现象，再学描述行的语言。现有 07 篇 §1.5 已记录这一取舍 | `12 §1` 开头回指"第 11 篇的行与字节直觉"；`11 §1` 声明"正则见下一篇" |
| 序差-2 | `expr` 的 `:` 运算符调用 `regcomp(..., REG_BASIC)`（`bin/expr/expr.y:112`）并在 `rm[0].rm_so == 0`（`:121`）上判锚定，运行时不依赖其它命令 | 讲述序把 `expr` 放进 `12`（正则消费者）而不是 `10`（路径与真值工具） | 无前向引用的硬标准优先：读者若不懂 BRE，读不懂 `expr` 的 `:`。把 `expr` 与 `grep`/`sed` 并列为"正则的三个消费者"是唯一不产生前向引用的方案 | `12 §1` 声明"本篇同时是 `expr` 的模式语义出处"；`10 §1` 的"不讲什么"里说明 `expr` 归 `12` |
| 序差-3 | 开机时序是"先 `mount -a` 再进入日常"，而卷的创建（`fdisk`/`newfs_*`）在使用之前 | 讲述序保留 `20` 挂载与检查在 `21` 分区与格式化之前 | 本 stage 的主线是交付链，`mount` 在链上（`etc/rc.minix:145`）、`fdisk` 不在链上；且 `20` 的读者面（读懂 `fstab` 与开机体检）比 `21` 更宽 | `21 §1` 开头回指"第 20 篇讲了怎么挂，本篇讲卷从哪来"（现有 15 篇 §1.5 已有同义说明） |
| 序差-4 | `getty`（`06 §2.1`）在运行时早于 `login` 读到的 `ttys`（`06 §2.2`），而 `ttys` 的真正消费者是 `init`（`sbin/init/init.c:1279-1281`），即比本篇更早的 `04` | 讲述序把 `ttys` 放在 `06 §2.2` 与 `gettytab` 一起讲 | 三处消费的语义（`init` 派生、`getty` 认领、`login` 判安全终端）只有合在一起才讲得通；`04 §2.4` 只讲 `init` 怎么用它 | `04 §2.4` 声明"`ttys` 的格式与另两方消费者见第 6 篇"；`06 §2.2` 回指 `04` 的派生逻辑 |
| 序差-5 | `MULTI_USER` 一到就为每个 `TTY_ON` 终端派生一个 `getty`（`init.c:1546-1557`），登录会话从此并发，无先后 | 讲述序把并发会话的管理（`04 §2.4`）与登录链（`06`）分两篇 | 单篇单语义：一篇讲"终端上永远有人值守"的机制，一篇讲"值守者如何验证人" | `04 §2.4` 末声明"验证流程见第 6 篇"；`06 §1` 回指"谁派生了我" |
| 序差-6 | `rcorder` 的排序结果是一张依赖图，不是一条线（`etc/rc:147-152`） | 讲述序把它讲成"读注释、建图、拓扑输出"三步的线性流程 | 读者需要的是读懂一张脚本的依赖声明，不是复现全部调度细节 | `04 §2.6` 给出 `rc.d` 32 个脚本的依赖图与四个屏障脚本 |
| 序差-7 | 菜单/游戏与包管理在运行时不依赖文本工具，也不依赖存储 | 讲述序把它们排在最后（`27`–`30`） | 它们是交付链的验收展示（`22` 篇旧 §1.0 的原话）与"已声明不做"的集中区，属支线 | `28 §1` 回指 `03 §3` 的最小 API 面 |
| 序差-8 | `cron`/`inetd`/`syslogd` 是常驻的事件循环服务 | 讲述序把它们各自放进所属命令族（`05`、`25`），不单立服务阶段 | 本 stage 的组织轴是"命令"，服务只是命令的一个使用场景；服务的启动段见 `04`、循环段见 `05`/`25` | `05 §2.4`、`25 §2` 各自画出"启动段 + 循环段"两段 |

### 4.4 并行主题的分组与代表成员

**并行组一：命令族（`09`–`30`，326 条命令）。** 组织规则：每篇先给统一框架节（共同语义、共同数据格式、共同错误面），再按族分组，每族选一至三个代表成员精讲（讲透"是什么、为什么这样设计、C 里长什么样"），其余成员以**族内差异表**收束（每行一命令，列：职责、关键选项、输入输出、退出码、错误面、`Requires`、状态）。代表成员的选择标准是"该族的复杂度最高者"或"该族最常用者"，逐篇如下：

| 篇章 | 代表成员（精讲） | 差异表覆盖 |
|---|---|---|
| 09 文件对象操作 | `cp`（选项组合即复杂度）、`ls`（715 行的列组合）、`find`（表达式树 + `xargs`） | 其余 21 条 |
| 10 路径、真值与输出 | `test`（四层递归求值）、`echo`（禁 `getopt` 的怪癖）、`printf`（运行时格式引擎） | 其余 6 条 |
| 11 文本行工具 | `uniq`（三元状态机）、`tr`（字符集三形态）、`sort`（语义与策略之分）、`diff` 与 `patch`（差异引擎与安全应用） | 其余 33 条 |
| 12 正则语义与消费者 | `sed`（地址文法 + 空匹配推进 + 动词面）、`grep`（选项三分类）、`expr`（优先级梯 + `:`） | 无法再细分（三命令） |
| 13 编辑器 | `ed`（命令分派 + 行操作三层） | `mined` |
| 14 排版与手册 | `man`（找页与渲染分离）、`makewhatis`（离线索引）、`cal`（双历法） | 其余 17 条 |
| 15 开发辅助与国际化 | `m4`（宏处理器的词法与文法） | 其余 15 条 |
| 16 压缩与归档 | `compress`（LZW 完整形态）、`uuencode`（编码器分派）、`shar`（归档即脚本） | 其余 7 条 |
| 17 进程与信号 | `ps`（列定义驱动）、`kill`（名号双通道） | 其余 9 条 |
| 18 会话记录与运行期小工具 | `who` 系（同一 36 字节的多种读法） | 其余 12 条 |
| 19 终端控制 | `stty`（控制字符表 + 标志表 + 解析次序） | 其余 7 条（含三个设备小工具） |
| 20 挂载与检查 | `mount`（参数形状 + 选项安全契约）、`fsck`（序号 discipline） | 其余 5 条 |
| 21 分区与格式化 | `fdisk`/`part`（MBR 布局与读写两端） | 其余 12 条 |
| 22 镜像与介质 | `dd`（操作数语言 + 字节结算）、`isoread`（ISO9660 识别） | 其余 11 条 |
| 23 备份与维护 | `backup`（增量决策）、`remsync`（三张记录表）、`cleantmp`（午夜对齐）、`mt`（磁带动作语言） | 其余 9 条 |
| 24 网络配置与诊断 | `route`/`arp`（路由套接字）、`ping`（校验和与配对）、`traceroute`（TTL 逐跳） | `ifconfig`/`netstat`/`netconf`/`slip`/`swifi` 等 |
| 25 网络服务与守护 | `inetd`（服务表与等待模式）、`syslogd`（优先级与选择符）、`fetch`（定位符）、`ftpd`（会话准入） | 其余守护与客户端 |
| 26 Minix 特有与系统信息 | `printroot`（根设备发现）、`readclock`（双钟同步）、`intr`（限时执行）、`sysctl`（参数树） | 其余 20 条 |
| 27 软件供给与构建辅助 | `installboot`（引导块类型与空间约束） | 其余 9 条 |
| 28–30 游戏 | 每篇三至四个代表玩法 | 其余玩法 |

**并行组二：交付链上的并发会话（`04`/`06`）。** 组织规则按提示词的"汇聚点加触发时机"：汇聚点是 `read_ttys` 读到的 `/etc/ttys` 表，触发时机是 `MULTI_USER` 状态与子进程退出事件。`04 §2.4` 讲汇聚与触发，`06` 讲一次登录的完整生命周期。

**并行组三：`etc/` 的 49 项配置数据。** 不单立篇章，按"谁读它"分散到各篇：`rc` 体系归 `04`，口令与终端表归 `06`，名字数据库归 `07`，shell 启动文件归 `08`，`man.conf` 归 `14`，`fonts`/`termcap*` 归 `19`，`newfstab.sh` 归 `20`，网络配置归 `24`/`25`，`system.conf` 归 `26`，`mk.conf` 归 `27`；`devmand`/`xorg.conf`/`root` 三项排除（理由见 §3.5 与 `plan.md §5.3`）。

### 4.5 设计原则与拒绝的替代方案

**采用的原则。**

1. **框架先行、账本收尾。** 任何被多篇引用的规则必须排在引用者之前（这一条同时修掉了现有结构里"中段各篇前向引用 99"的硬伤）；任何会随实现变动的表格集中到末尾一篇，避免 24 处同步。
2. **一篇一个读者问题。** 用"这一篇回答读者的哪个问题"检验边界；答不出同一个问题的成员，移出去（这就是三次拆分的判据）。
3. **教学与账本分离。** 契约表、模块表、函数一览、测试统计、`Requires` 属于账本，不进入教学正文；教学正文保留概念、C 语义、设计决策、族内差异表。
4. **孤儿归位。** 每一条被命令数或归属表点名的命令，必须在新篇章里有唯一家族与至少一行契约，否则不许出现在归属表里。

**拒绝的替代方案（记录理由，供共识阶段复核）。**

| 方案 | 内容 | 为什么不采用 |
|---|---|---|
| A 保持 26 篇编号不变，把框架内容塞进旧 00 | 不触动任何编号，把生命周期与约定都写进总览篇 | 违反单篇单语义（总览会同时是导航文、生命周期教学、约定手册）；且旧 99 仍留在文末，中段各篇的前向引用照旧 |
| B 只重写内容、不重排编号 | 承认现有结构，逐篇把内容修好 | 修不掉三个结构性问题：框架缺位、约定前向引用、三个多语义篇。断链成本虽低（约 250 处），但收敛后仍要再动一次编号 |
| C 把三条命令族拆得更细（每篇 ≤ 10 条命令，约 40 篇） | 彻底按命令族拆分 | 命令族之间有大量共享语义（行/字节/路径/退出码），拆到过细会让每个共享概念重复 3 至 4 次，反而制造新的重复主题；且 40 篇的导航成本超过收益 |
| D 合并所有游戏为一篇 | 把 28/29/30 合成一篇 | 三篇的依赖面不同（stdio / termios / 文本数据），合并会让一篇同时讲转义序列与文本数据格式，违反单篇单语义 |
| E 把存储四篇的顺序调成"分区→格式化→镜像→挂载" | 按创建顺序讲 | 主线是交付链，`mount` 在链上而 `fdisk` 不在；调整会把 `20` 的读者面（读 `fstab`、理解开机体检）推到后面。改为在序差表里记录（序差-3） |

---

## 5. 每篇契约

**清单的写法约定。** 每篇的"知识点清单"按**同一锚点族合行**列出：一行可以覆盖 §2 里连续的多条知识点（例如"`K-118`…`K-122` 五个家族概念"共用同一组 C 目录作锚点）；每条的完整信息（类型、现有位置、读者收益）在 §2 的总表里可查。行内"来源"列写存量时给旧文档与小节，写新增时给 C 源码或非 C 制品锚点。

**全局前置。** `00`、`01`、`02`、`03` 四篇是全 stage 的公共前置，后面每一篇都隐含"已读这四篇"；各篇"前置"栏只列领域相关的前置。

### 00-命令面总览：规模、交付链与阅读路径

- **一句话定位**：让读者知道这 326 条命令是什么、按什么顺序读、每一类归哪一篇。
- **讲什么**：命令面的规模与统计口径；一级分类框架（身份与注册类、转换与计算类、介质与状态类、交互与娱乐类）；交付因果链总图（从内核交棒到交互 shell 的每一个进程边界，带 C 锚点）；功能域矩阵与 32 篇的对应；三条阅读路径与可跳读清单；文档导航表（编号、标题、一句话定位）；本篇不覆盖的索引。
- **不讲什么**：一切命令机制细节（交给 `01`–`30` 各篇，按功能域）；交付链的详细机制（交给 `04`–`08`）；约定与判定规则（交给 `03`）；实现状态与覆盖表（交给 `99`）。
- **前置**：`01-stage-kernel`（引导完成）、`14-stage-runtime`（`exec` 交棒与顶层调用封装）、`17-stage-net`（socket 封装）。
- **后置**：全部篇章（每篇的头部声明都回指本篇的导航位）。
- **事实底线**：
  - C：七个源目录的 `ls -d` 全量清点、`find -name '*.c' | wc -l` 计数（§0.3 实测表）；`sbin/init/init.c:229` 至 `login.c:723` 的交付链（§1.2 的 39 步）；`lib/csu/common/crt0-common.c:145-191`；`bin/Makefile.inc:7`、`include/paths.h:45`。
  - 非 C 制品：349 个手册页、`etc/` 51 项、`games/Makefile.inc` 的 `BINDIR` 双层安装。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-001…K-008 | 文档状态、源面清单、交付链总图、规模统计、功能域矩阵、导航原则、设计原则、边界声明 | 导航/工具与工程 | §0.3 实测表；§1.2 | 只有总览篇同时需要这八项 | 旧 00 全部（存量） |
| K-009 | 一级分类框架（四类） | 概念 | §1.4 的 P1–P13 路径 | 给 326 条命令一个顶层分类 | 新增（POSIX 分类 + Minix3 目录分层） |
| K-010 | 阅读路径三线与可跳读清单 | 导航 | §4.2 | 路径定义属总览 | 新增 |
| K-011、K-012、K-014 | 统一框架三问、不覆盖索引、getty/login 与 `etc/` 的归属 | 导航 | §1.3 的 B7–B9；`libexec/getty/` | 边界声明属总览 | 旧 00 边界（存量）+ 新增 |
| K-015 | 统计口径定义（一个目录一条命令；DESCRIBE 与 devmand 两处例外） | 工具与工程 | §0.3 口径更正段 | 口径必须在读者第一次看到数字的地方定义 | 新增 |
| K-016 | 篇章与 crate 的对应关系史 | 工具与工程 | `todo.md §6.1 C-1` 收敛目标段 | 解释为什么篇章与 crate 一一对应 | 新增 |

- **验收标准**：读者读完本篇能回答四个问题——(1) 本 stage 一共有多少条命令、数字怎么数出来的；(2) 一条命令从内核交棒到退出经过哪些环节、每一环归哪一篇；(3) 我要找的某条命令在哪一篇、依据是什么；(4) 我只想读存储/网络/游戏中的一类时，最短的阅读路径是什么。检查方式：本篇的导航表必须与 §4.1 的总表逐行一致；本篇出现的每一个数字必须能在 §0.3 的命令输出里找到出处。

### 01-一条命令的一生：从交棒到退出码

- **一句话定位**：讲清 326 条命令共享的那一条生命周期，以及这条链上每一环的 C 锚点。
- **讲什么**：`_start` 汇编入口如何把 `ps_strings` 交给 C 运行时；`argc`/`argv`/`environ` 的来源（不经任何库）；`__progname` 与命令的三种身份（程序名、PATH 命中路径、手册页名）；`getopt` 作为全命令共用的选项解析契约；用法行（`usage`）的位置与形状；标准输入输出错误三条通道在"没有 stdio 库"的现实下如何实现（`write` 而非 `printf`）；退出码是唯一的机器可读结论；"决定半/执行半"的实现分层。
- **不讲什么**：具体命令的选项面（交给 `09`–`30` 各篇的族内差异表）；依赖分层的硬规则与 `Requires` 的填法（交给 `03 §1`、`03 §3`）；命令如何被打包成可执行文件（交给 `02 §1`）；静态链接的架构决策（交给 `02 §1`）；`exec` 在服务端的实现（交给 `14-stage-runtime`）。
- **前置**：`00`。
- **后置**：`02 §1`（交付形态承接"程序名"一栏）、`03 §3`（契约表的九列在生命周期里各有出处）、`09`–`30` 各篇（每篇的族内差异表都按本篇定义的"选项、输入输出、退出码、错误面"四栏填写）。
- **事实底线**：
  - C：`lib/csu/arch/x86_64/crt0.S:39-44`（`_start` 搬参跳转）、`lib/csu/common/crt0-common.c:145`（`___start`）、`:154`（`environ = ps_strings->ps_envstr`）、`:158`（`__progname`）、`:191`（`exit(main(argc, argv, environ))`）；`lib/libc/stdlib/getopt.c`；`include/getopt.h`；用法行的实例（`minix/commands/svrctl/svrctl.c:107`、`minix/commands/at/at.c:26-80`）。
  - 非 C 制品：349 个手册页的 `SYNOPSIS` 段与命令 `usage` 行的对应关系。
  - Rust：`os/commands/bin/fileops/src/lib.rs:8-25`（决定半/执行半的自述）、`os/commands/bin/fileops/src/bin/echo.rs`（最小执行半样板）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-401、K-402 | `_start` 交棒；`argv`/`environ` 的来源 | 机制/数据结构 | `crt0.S:39-44`；`crt0-common.c:145-191` | 命令的第一条指令与唯一参数来源 | 新增 |
| K-403 | `getopt` 共用的选项解析契约 | 接口与协议 | `lib/libc/stdlib/getopt.c` | 326 条命令共享 | 新增 |
| K-404 | 用法行与手册页 `SYNOPSIS` 的双向对应 | 接口与协议 | 349 个手册页；`svrctl.c:107` | 命令自描述面的两支 | 新增 |
| K-405 | 退出码是唯一的机器可读结论 | 约束与不变量 | `crt0-common.c:191` | 命令对调用者的契约 | 新增 |
| K-406 | 命令的三种身份 | 概念 | `crt0-common.c:158`；`include/paths.h:45` | 把三个散落的面串成一问 | 新增 |
| K-407 | 三条通道与"无 stdio 库"的现实 | 接口与协议 | `99 §1`；`fileops/src/bin/echo.rs` | 输出通道的实现前提 | 新增 |
| K-408 | 决定半/执行半分层 | 架构演进 | `fileops/src/lib.rs:8-25`；`todo.md §6.1 C-1` | 全 stage 统一实现决策 | 新增 |

- **验收标准**：读者读完能回答——(1) `argv` 和 `environ` 是从哪里来的，为什么命令不需要调用任何函数就能拿到；(2) 一条命令解析选项的标准做法是什么，哪些写法是被同一份 `getopt` 决定的；(3) 命令失败时调用者能看到什么，退出码的取值范围与含义约定在哪一篇；(4) 为什么 minix-rs 的命令输出用 `write` 而不是 `printf`。检查方式：本篇出现的第一条指令、第一个参数来源、第一个退出码必须各有一个 `file:line` 锚点；文中给出的最小样板代码必须能对上 `os/commands/bin/fileops/src/bin/echo.rs` 的实际形态。

### 02-命令的交付与安装面：注册、查找与交付形态

- **一句话定位**：讲清命令从源码到可执行文件、从磁盘到被找到、从名字到手册页的三段交付链。
- **讲什么**：`BINDIR` 分层决定权限与 PATH、不决定语义归属；`SUBDIR` 清单即命令注册表；`/usr/games/hide` 双层安装与 setgid 游戏；`DESCRIBE` 构建面；默认查找路径与救援路径（`_PATH_DEFPATH`、`RESCUEDIR`）；`[ARCH] A-1` 静态链接对交付形态的影响；手册页索引面（`man.conf` 的 `_subdir`/`_build`、`makewhatis` 离线索引）与人读命令的路径；命令二进制面尚未装配的真实现状；发行集合与镜像装配的边界声明。
- **不讲什么**：`man` 命令本身的参数与渲染（交给 `14 §2`）；构建工具链与编译过程（交给 `27 §1` 与构建链，本篇只讲"命令怎么被登记"）；安装器 `setup` 的交互流程（交给 `04 §2.8`）；发行集合的具体内容（范围外，只在 §1 登记指针）。
- **前置**：`00`、`01`。
- **后置**：`03 §5`（约定篇引用本篇的交付形态结论）、`04 §2.8`（`setup` 与首次配置承接"装好之后怎么配"）、`14 §2`（手册系统承接本篇的索引面）、`27 §1`（构建辅助承接"宿主工具链不在本 stage"的边界）。
- **事实底线**：
  - 非 C 制品（本篇的主要证据面）：`bin/Makefile.inc:7`、`sbin/Makefile.inc:7`、`usr.bin/Makefile.inc:5`、`usr.sbin/Makefile.inc:4`、`games/Makefile.inc:7`（`/usr/games/hide`）、`games/Makefile.inc:15`（`SYMLINKS+= dm /usr/games/${PROG}`）、`games/Makefile.inc:17`（`/usr/games`）；`usr.bin/Makefile`（`SUBDIR=` 全量）、`minix/commands/Makefile`；`include/paths.h:43/45`；`etc/man.conf`；`minix/commands/DESCRIBE/DESCRIBE.sh`；`minix3/distrib/`、`minix3/releasetools/release.sh`（范围外指针）。
  - C：`usr.bin/man/man.c`、`libexec/makewhatis/makewhatis.c`、`usr.bin/ldd/ldd.c:94-96`。
  - Rust：`os/commands/` 的 24 个 crate 与 `os/Cargo.toml` 的成员表（19 个纯库、5 个带 `src/bin/`）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-409 | `BINDIR` 分层原理 | 工具与工程 | 五个 `Makefile.inc` | 安装面的第一原理 | 新增 |
| K-410 | `SUBDIR` 清单即注册表 | 工具与工程 | `usr.bin/Makefile`、`minix/commands/Makefile` | 集合型 stage 的注册机制 | 新增 |
| K-411 | `/usr/games/hide` 双层安装 | 工具与工程 | `games/Makefile.inc:7/15/17` | 唯一非扁平安装面 | 新增 |
| K-412 | 默认查找路径与救援路径 | 接口与协议 | `include/paths.h:43/45` | 集合型 stage 的发现机制 | 新增 |
| K-413 | 手册页索引面 | 接口与协议 | `etc/man.conf`、`makewhatis.c` | 命令被发现与被读懂的完整链 | 新增 |
| K-414 | `DESCRIBE` 构建面 | 工具与工程 | `DESCRIBE/DESCRIBE.sh` | 与设备静态面呼应 | 新增 |
| K-415 | `[ARCH] A-1` 对交付形态的影响 | 架构演进 | `14-stage-runtime` A-1；`ldd.c:94-96` | 交付形态的架构前提 | 新增 |
| K-416 | 命令二进制面尚未装配 | 工具与工程 | `os/commands/` 实测；`os/Cargo.toml` | 交付形态的真实现状 | 新增 |
| K-016 | 篇章与 crate 对应关系 | 工具与工程 | `todo.md §6.1` | 交付形态与文档的对应 | 新增 |
| K-400（安装面部分） | 命令安装面的待写状态 | 导航 | `99 §核心点-1` | 从旧 99 接手 | 存量（旧 99） |
| K-440 | 发行集合与镜像装配的边界 | 工具与工程 | `distrib/sets/`、`releasetools/release.sh` | 交付形态最外层 | 新增（范围外指针） |

- **验收标准**：读者读完能回答——(1) `ls` 为什么在 `/bin` 而 `mkdir` 在 `/usr/bin`，这个区别影响什么、不影响什么；(2) 新增一条命令要动哪几处登记；(3) 敲 `foo` 时系统按什么顺序找它、找不到时的救援路径是什么；(4) 本项目的命令最终以什么形态出现在镜像里，现状完成的哪一步、缺的是哪一步。检查方式：文中每一处安装路径都要有 `Makefile.inc` 的行号锚点；"现状"一节必须与 `os/commands/` 的实测计数一致（24 个 crate、19 个纯库、5 个带 `src/bin/`）。

### 03-跨篇约定：依赖分层、行为判定基准与命令契约表

- **一句话定位**：给出全 stage 唯一的一套判定规则与表格模板，供后面每一篇引用。
- **讲什么**：三层依赖硬规则（命令只依赖 `minix-rt` 与 `minix-sys` 顶层；命令不得直接构造 IPC；`minix-sys` 顶层不得假设服务器实现）与机械守卫脚本；`minix-sys` 现有顶层函数的准确含义（`send`/`receive`/`sendrec`/`notify`/`fork`/`exec`/`exit`/`waitpid`/`kill`/`open`/`close`/`read`/`write`/`mmap`）与行号（须重取）；"stdio" 一词在文档里的准确落点；行为判定基准（POSIX 为准绳、Minix3 C 实现为真值，C 偏离 POSIX 时按 C 写并标注）；命令契约表的九列定义与 `Requires` 列的填法；参数框架约定（`[ARCH] A-4`）；退出码与 `errno` 映射约定（`[ARCH] A-5`）；命令层单线程模型与阻塞原语的归属；命令"决定半/执行半"的测试哲学。
- **不讲什么**：具体命令需要哪些 API（交给 `99` 账本的 `Requires` 总表）；具体命令的退出码值（交给各篇的族内差异表）；`minix-sys` 的实现（交给 `14-stage-runtime`）；ARCH 决策的裁决记录（交给 `99` ARCH 登记表）。
- **前置**：`00`、`01`、`02`。
- **后置**：`04`–`30` 全部篇章（每篇的族内差异表按本篇的九列填写，判定争议按本篇的基准裁决）；`99` 账本（`Requires` 总表按本篇的填法汇总）。
- **事实底线**：
  - Rust：`os/libs/minix-sys/src/lib.rs`（顶层函数；2026-09-19 实测行号：`fork` `:163`、`open` `:229`、`write` `:255`，errno 常量再导出 `pub use minix_types::types::errno::*` `:55` 与 `pub use minix_types::Errno` `:362`——旧 99 引用的 `:148`/`:217`/`:191-231` 已随源码演进漂移，B 相必须当场重取）；`os/libs/minix-rt/src/{crt0,handoff}.rs`；`os/commands/bin/fileops/src/bin/echo.rs`（正面样本）；`os/commands/bin/proctools/src/lib.rs`（错误码映射样本）。
  - 非 C 制品：`tools/check-command-boundary.sh:26-36`（两条源码级硬规则与它的检查命令）。
  - C：`lib/libc/thread-stub/thread-stub.c`（单线程桩）、`minix/lib/libmthread/pthread_compat.c:7-8`（绿线程不是 pthread）；`lib/libc/stdlib/getopt.c`（选项解析实现，与 `[ARCH] A-4` 相关）。
  - 跨阶段：`14-stage-runtime/todo.md:60`（`stat`/`getdents`/`ioctl`/`fcntl` 的 wrapper 缺口）、`edge_todo.md` 的 `E-CMDSYSFACE`、`todo.md §2`（P1-1）、`todo.md §4` 与 `edge4 §6`（线程模型归属裁决）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-389、K-390、K-391、K-392 | 分层契约、三条硬规则、`minix-sys` 顶层函数含义、"stdio" 落点 | 架构演进/约束与不变量/接口与协议 | `minix-sys/src/lib.rs`；`tools/check-command-boundary.sh` | 全 stage 的依赖红线 | 存量（旧 99 §1） |
| K-393 | 行为判定基准 | 概念 | `99 §2` | 争议裁决规则 | 存量（旧 99 §2） |
| K-394、K-418 | 契约表模板与 `Requires` 列 | 接口与协议 | `99 §3`；`todo.md §2` | 各篇的表格基准 | 存量 + 新增 |
| K-417 | 三条硬规则的机械守卫 | 约束与不变量 | `check-command-boundary.sh:26-36` | 规则的落地机制 | 新增 |
| K-419 | 参数框架约定（A-4） | 工具与工程 | `plan.md §4 A-4` | 全命令统一 | 新增 |
| K-420 | 退出码与 `errno` 映射约定（A-5） | 约束与不变量 | `proctools/src/lib.rs` | 全命令统一 | 新增 |
| K-421 | 单线程模型与阻塞原语归属 | 约束与不变量 | `thread-stub.c`；`pthread_compat.c:7-8`；`edge4 §6` | 跨篇约定 | 新增 |
| K-288 | 轻量手写参数解析与静态链接取舍 | 架构演进 | 旧 17 §3.5 | 它是跨域政策而非进程工具主题 | 存量（旧 17 §3.5，越界移入） |
| K-397、K-398、K-399 | A-1/A-4/A-5 三条 ARCH 的约定面 | 架构演进 | 旧 99 核心点 | 决策在约定篇的落地形态 | 存量（旧 99 核心点） |
| K-213（约定部分） | 错误码贴源不贴惯 | 约束与不变量 | `proctools/src/lib.rs` | 退出码约定的实例 | 存量（旧 12 §3.2） |
| K-013 | 命令层的分层契约摘要（命令只消费 `minix-rt` 与 `minix-sys` 顶层） | 约束与不变量 | 旧 00 头部"Rust 模块"声明；`tools/check-command-boundary.sh:26-36` | 把旧文档头部的隐含约定总结成本篇的正式判定规则，是三条硬规则的来源 | 存量（旧 00 头部） |

- **验收标准**：读者读完能回答——(1) 写一条命令时允许 `use` 哪些 crate，越界的检查命令是什么；(2) 当 POSIX 与 Minix3 C 实现不一致时按哪个写、怎么标注；(3) 契约表九列分别填什么、`Requires` 一列从哪里取值、缺口往哪登记；(4) 命令要不要考虑多线程。检查方式：本篇给出的每一条硬规则都必须有一条可执行命令（`grep` 或守卫脚本）作为复核手段；本篇引用的 `minix-sys` 行号必须由 B 相当场 grep 重取，不得沿用旧 99 的漂移值。

### 04-启动初始化进程与启动脚本链

- **一句话定位**：内核交权之后，谁把系统带到多用户可用状态。
- **讲什么**：第一个用户进程的意义（孤儿收养、运行级别、终端派生）；七状态状态机与初始转移；`runcom` 状态与 `FASTBOOT`；三条信号线（重启、断电、灾难）与"处理函数只 fork"的极简律；终端会话管理（`read_ttys`、`multi_user`、`clean_ttys`、会话表）；`/etc/rc` 的总体流程（配置检查、`rcorder` 排序、日志分流、元数据标记）；`rcorder` 的注释契约与拓扑排序（对照 systemd）；`rc.d` 32 个脚本的依赖图与四个纯屏障脚本、五个空壳脚本；`rc.minix` 的**真实启动动作**（`fsck -x /`、`mount -a`、`utmp` 清零、十个服务域注册、`devmand` 启动）；单用户兜底与根口令校验；配置面与首次配置（`rc.conf`、`defaults`、`boot.cfg.default`、`setup`、`bootconf.sh` 的交互分支）；关机与重启（`shutdown`/`reboot`、`rc.shutdown`）；`[ARCH] A-6` 的保留 shell 链 vs 编译期静态配置。
- **不讲什么**：服务的注册与调度细节（交给 `05`）；`getty` 与 `login` 的验证流程（交给 `06`）；`mount`/`fsck` 命令的参数与检查序号（交给 `20`）；`MAKEDEV` 与设备节点（交给 `07`）；`ttys` 与 `gettytab` 的格式（交给 `06 §2.2`、`06 §2.3`）；`utmp` 的记录结构与读者（交给 `18 §2`）；Rust 侧 `minix-init` 的模块与测试（交给 `99` 账本）。
- **前置**：`01-stage-kernel`（引导完成）、`14-stage-runtime`（`exec` 与进程退出语义）、`03`（约定）。
- **后置**：`05`（承接"服务被拉起来之后"）、`06`（承接"终端上的人从哪里来"）、`07`（承接"设备节点从哪来"）、`20`（承接"挂载与检查的命令面"）。
- **事实底线**：
  - C：`sbin/init/init.c`（全部锚点见 §1.2 的 1–21 步与 39 步）、`etc/rc`、`etc/rc.subr`（`:73`、`:119-123`、`:136-189`、`:478-804`、`:849-907`、`:913-929`、`:1086-1111`、`:1118-1140`、`:1158-1167`、`:1234-1241`、`:1268-1282`、`:1343-1357`）、`etc/rc.conf`、`etc/rc.minix`（`:78-88`、`:100-103`、`:144`、`:145`、`:147-152`、`:155-156`、`:159-179`、`:182-187`、`:191-198`、`:200-203`、`:229-237`）、`etc/rc.cd:26-29`、`sbin/rcorder/rcorder.c`（`:78-91`、`:166-205`、`:271-385`、`:405-475`、`:477-555`、`:587-626`、`:628-655`、`:667-747`、`:749-771`）、`sbin/shutdown/shutdown.c`、`sbin/reboot/reboot.c`、`bin/sh/main.c:198-213`。
  - 非 C 制品：`etc/rc.d/` 全部 32 个脚本（逐个给 `PROVIDE`/`REQUIRE`/`BEFORE`/`KEYWORD` 行号，见 §1.2 的 rc.d 清单）、`etc/defaults/{minix.rc.conf,rc.conf}`、`etc/boot.cfg.default`、`minix/commands/setup/setup.sh`、`etc/rs.single`、`etc/rc.shutdown`、`etc/rc.capes/`。
  - Rust：`os/commands/sbin/init/src/`（20 个源文件，含 `main.rs`、`state_machine.rs`、`runcom.rs`、`session_db.rs`、`clean_ttys.rs`、`multi_user.rs`、`single_user.rs`、`ttys.rs`、`sysctl.rs`、`contracts.rs`、`entry.rs`、`driver.rs`、`host.rs`、`password.rs`、`wait.rs`、`signal_state.rs`、`shutdown.rs`、`log.rs`、`session.rs`、`utmp.rs`）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-017、K-018、K-019 | 第一个用户进程；启动脚本三层架构；启动与关机互逆 | 概念/机制 | `init.c:229-367`；`etc/rc:17-22` | 交付链的起点 | 存量（旧 01 §1.1-1.4） |
| K-020、K-021 | 七状态状态机；`runcom` 与 `FASTBOOT` | 机制 | `init.c:133-146`/`:195`/`:624`；`:975`/`:866` | init 的心脏 | 存量（旧 01 §2.1-2.2） |
| K-022 | 终端会话管理与会话表 | 机制 | `init.c:1222`、`:157-165`、`:1142-1180` | 交付链的扇出点 | 存量（旧 01 §2.3） |
| K-023 | 三条信号线 | 接口与协议 | `init.c:315-320`、`:517-538`、`:504-511` | 关机/重启入口 | 存量（旧 01 §2.4） |
| K-024、K-025、K-423 | `/etc/rc` 流程；`rcorder` 契约；`rc.d` 依赖图与空壳清单 | 机制/接口与协议 | `etc/rc:31-34`/`:141-152`；`rcorder.c:78-91`；`etc/rc.d/*` | 启动脚本的执行与排序 | 存量（旧 01 §2.5-2.6）+ 新增 K-423 |
| K-422 | `rc.minix` 的真实启动动作 | 机制 | `etc/rc.minix:144/145/155-156/159-179/200-203`；`rc.d/minixrc:13` | 开机真正干活的一段 | 新增 |
| K-026 | 配置面与首次配置 | 工具与工程 | `rc.conf`、`defaults/`、`boot.cfg.default`、`setup.sh` | 开机行为的可改面 | 存量（旧 01 §2.7-2.8） |
| K-027、K-028、K-029、K-030 | 枚举状态机；纯决策函数下沉；会话表改哈希表；A-6 决策 | 架构演进/数据结构 | `init/src/*.rs` | Rust 侧的设计决策 | 存量（旧 01 §3.1-3.4） |
| K-031、K-032 | 模块职责与不变量；测试与留白 | 约束与不变量/测试性质 | `init/src/*.rs`；`cargo test -p minix-init` | 实现账本，指向 99 | 存量（旧 01 §4-§5，改指向 99） |

- **验收标准**：读者读完能回答——(1) `init` 的七种状态各在什么条件下进入、哪种状态执行启动脚本；(2) 开机时真正执行 `fsck`/`mount`/服务注册的是哪个文件、它是怎么被调起来的；(3) `rc.d` 里一个脚本要怎么写才能被排到 `NETWORKING` 之前；(4) 单用户模式在什么情况下被触发、根口令校验发生在哪一步；(5) 关机时 `init` 与 `shutdown` 各做什么。检查方式：本篇必须给出 `rc.d` 全部 32 个脚本的 `PROVIDE`/`REQUIRE` 一览表，以及一条从 `init.c:975` 到 `etc/rc.minix:203` 的完整调用链；`rc.minix` 的五个动作各带行号。

### 05-服务管理与定时调度

- **一句话定位**：服务被谁照看，任务到点如何被执行。
- **讲什么**：服务的概念与重生服务（RS）；两个服务命令的血统与分工（`minix-service` 与 `service`）；`svrctl` 低层专家接口与管理员门槛；定时调度三种"到点"（周期、一次、固定节拍）；`crontab` 时间字段的 Minix 方言（含问号只在分钟字段合法、冒号步长不兼容斜杠）与 `/etc/crontab` 实例；`minix-service` 九种命令形状；`service` 脚本三段式；`cron` 的时间解析内部（`range_parse` 位图、坏行容错、守护主循环）；`at`/`atnormalize` 的一次任务两端与日期顺延；`update` 的固定节拍；`shlock` 锁原语与它在三处的使用；Rust 侧的双解析器、`ScheduleMatcher` 接口与目录逃逸拦截。
- **不讲什么**：RS 服务端实现（交给 `03-stage-rs`）；`cron`/`at` 跑的任务本身（散落各篇）；`syslogd` 的日志分拣（交给 `25 §2`）；服务启动脚本的排序机制（交给 `04 §2.6`）；`inetd` 的按需起服务（交给 `25 §2`，与 `service` 的分工在第 1 节声明）。
- **前置**：`04`（启动脚本调用服务的场合）、`03-stage-rs`（重生服务协议）。
- **后置**：`25`（网络守护的启动方式承接"服务被谁拉起来"）、`27`（软件供给承接"服务管理面之后怎么装软件"）。
- **事实底线**：
  - C：`minix/commands/minix-service/minix-service.c` 与 `minix-service.8`；`usr.sbin/service/service`；`minix/commands/svrctl/svrctl.c:107`；`minix/commands/cron/cron.c:321`（主循环）、`cron/tab.c:285-360`（`range_parse`）、`:327`（问号）；`minix/commands/crontab/crontab.c:70-122`；`minix/commands/at/at.c:26-80`、`:66`；`minix/commands/atnormalize/atnormalize.c:25`；`minix/commands/update/update.c:1-24`；`usr.bin/shlock/`。
  - 非 C 制品：`etc/crontab`、`etc/rs.lwip`、`etc/rs.single`、`etc/rc.d/`（服务的启动声明）。
  - Rust：`os/commands/usr-sbin/svcsched/src/{lib,service,cron,scheduler}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-033、K-034、K-035、K-036 | 服务概念；两个服务命令；`svrctl`；三种"到点" | 概念/接口与协议 | `minix-service.c`；`service`；`svrctl.c:107` | 本篇的概念地基 | 存量（旧 02 §1.1-1.4） |
| K-037、K-038、K-039 | `crontab` 方言；九种命令形状；`service` 三段式 | 接口与协议 | `cron/tab.c:285-360`；`minix-service.8` | 命令的外部契约 | 存量（旧 02 §1.5/§2.1-2.2） |
| K-040、K-041、K-042 | cron 解析内部；`at`/`atnormalize`；`update` | 机制 | `cron/tab.c`；`cron.c:321`；`at.c:26-80`；`update.c:1-24` | 三个命令的机制 | 存量（旧 02 §2.4-2.6） |
| K-439 | `shlock` 锁原语与三处使用 | 机制 | `usr.bin/shlock/`；`cron`/`at`/`backup` 调用点 | 调度器互斥的公共设施 | 新增 |
| K-043、K-044、K-045 | 双解析器；`ScheduleMatcher`；问号与步长的约束 | 架构演进/接口与协议/约束与不变量 | `svcsched/src/{service,scheduler,cron}.rs` | Rust 侧设计决策 | 存量（旧 02 §3） |
| K-046 | 模块、类型与测试 | 测试性质 | `svcsched/src/` | 实现账本，指向 99 | 存量（旧 02 §4-§5，改指向 99） |

- **验收标准**：读者读完能回答——(1) 我要把一个守护注册成受照看的服务，用哪个命令、写哪几处；(2) 写一行 `crontab` 时哪些写法是 Minix 方言、哪些会报错；(3) 三种"到点"各适合什么任务、过了时刻的任务怎么处理；(4) 两个调度守护如何避免同时跑同一个任务。检查方式：本篇必须给出 `crontab` 时间字段的完整文法与一个反例（问号出现在非分钟字段时报错）；`shlock` 一节必须列出三个调用点。

### 06-登录链路与口令数据库

- **一句话定位**：终端上的人如何通过验证并拿到自己的 shell。
- **讲什么**：三棒交接（`init` → `getty` → `login` → shell）与"交棒即退出"；口令数据库的两张脸（`master.passwd` 十字段、`passwd` 七字段、`pwd_mkdb` 编译出的索引库）与三种登录许可形态；文本与库的一致性工具链（`vipw` 加锁校验重建；`chpass`/`passwd`/`pwhash` 同模式）；身份切换四件套（`su`/`newgrp`/`id`/`nologin`）；会话环境三来源与"模板复制而非引用"；`getty` 认领终端的五步；`/etc/ttys` 的五列合同与三方共读；`gettytab` 能力表；`login` 的验证核心（四标志、信号处置、查库、口令比对、十次与三秒退避）；`login` 的收尾布设与 `exec` 替换；`utmp` 的**写入侧**（三方契约的写入端）。
- **不讲什么**：`utmp` 的记录结构与读者（交给 `18 §2`）；`ttys` 的另一个消费者 `init`（交给 `04 §2.4`）；口令哈希算法与认证后端（交给 `04-stage-pm`）；shell 启动文件的解释执行（交给 `08 §2.1`）；终端驱动（交给 `16-stage-drivers`）；终端行属性（交给 `19`）。
- **前置**：`04`（终端会话由谁派生）、`05`（服务视角的值守关系）、`03`（约定）。
- **后置**：`08`（承接"登录后拿到 shell"）、`18 §2`（承接 `utmp` 的读者）、`07`（承接"名字从哪来"）。
- **事实底线**：
  - C：`libexec/getty/main.c:184-457`（`:197`、`:199-203`、`:234-242`、`:261-262`、`:274-283`、`:291`、`:296`、`:302-315`、`:357-366`、`:369`、`:398`、`:435`、`:441/444`、`:459-581`）、`libexec/getty/init.c:58-118`、`libexec/getty/subr.c:70-113`、`:615-638`、`libexec/getty/pathnames.h:36`；`usr.bin/login/login.c`（`:135-725`、`:207`、`:266-276`、`:348`、`:368-374`、`:399`、`:423-424`、`:441`、`:483-487`、`:535-556`、`:558`、`:560-561`、`:563`、`:573-574`、`:588`、`:590`、`:592-595`、`:598-601`、`:604-605`、`:616-634`、`:690-692`、`:704-715`、`:723`、`:769-780`）、`usr.bin/login/common.c:122-128`、`:166+`、`:370-372`；`lib/libc/gen/getpwent.c:181`、`:194-199`、`:617-642`；`usr.sbin/pwd_mkdb/pwd_mkdb.c`；`usr.sbin/vipw/vipw.c`；`etc/Makefile:304-313`；`include/pwd.h:73`、`:80-81`；`include/paths.h:45`、`:73`、`:125`；`include/ttyent.h:40`。
  - 非 C 制品：`etc/ttys`（console/tty00/ttyp0 三个实例）、`etc/gettytab`（`default` 与 `std.9600|9600-baud`）、`etc/master.passwd`、`etc/passwd.conf`、`etc/skel/`、`etc/root/dot.profile`、`etc/profile`。
  - Rust：`os/commands/usr-bin/login/src/{lib,passwd,ttys,gettytab,userdb}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-047、K-050、K-051 | 三棒交接；身份切换四件套；会话环境三来源 | 概念/工具与工程 | `getty/main.c:441`；`login.c:723`；`etc/skel/` | 登录链的概念地基 | 存量（旧 03 §1） |
| K-048、K-049、K-057 | 口令数据库两张脸；一致性工具链；三种登录许可形态 | 数据结构/机制/概念 | `etc/master.passwd`；`pwd_mkdb.c`；`vipw.c`；`login.c:769-780` | 口令库的完整面 | 存量（旧 03 §1.2、§2.5-2.6，合并重复） |
| K-052、K-053、K-054 | `getty` 认领五步；`ttys` 五列合同；`gettytab` 能力表 | 机制/接口与协议/数据结构 | `getty/main.c:184-457`；`etc/ttys`；`getty/subr.c:70-113` | 第一棒与两个数据格式 | 存量（旧 03 §2.1-2.3，`gettytab` 从旧 04 收拢） |
| K-055、K-056 | `login` 验证核心；收尾布设与 `exec` | 机制 | `login.c:207`/`:174-176`/`:348`/`:423-424`；`login.c:604-723` | 第二棒的两段 | 存量（旧 03 §2.4） |
| K-424（写入侧） | `utmp` 写入端（`update_db`） | 数据结构 | `login/common.c:166+`；`login.c:558` | 三方契约的写入侧 | 新增（主讲述点在 18 §2） |
| K-058、K-059、K-060、K-061 | 四格式各建解析模块；零拷贝借用；`UserDatabase` 双实现；判定规则 | 架构演进/接口与协议/约束与不变量 | `login/src/{passwd,ttys,gettytab,userdb}.rs` | Rust 侧设计决策 | 存量（旧 03 §3-§4） |
| K-062 | 测试与留白 | 测试性质 | `login/src/` | 实现账本，指向 99 | 存量（旧 03 §5，改指向 99） |

- **验收标准**：读者读完能回答——(1) 从 `login:` 提示到拿到 shell，进程被替换了几次、每次替换的锚点在哪；(2) 改一个用户的口令要动哪几个文件、为什么不能只改文本；(3) 一个账号被锁住有哪几种表现；(4) 登录尝试连续失败时系统怎么防猜解；(5) 为什么 `TERM` 的值来自 `/etc/ttys` 而不是环境。检查方式：本篇必须画出三棒交接的进程边界图并给每处 `exec` 的行号；`master.passwd` 的字段数必须与 `include/pwd.h` 的宏一致。

### 07-设备节点与名字数据库

- **一句话定位**：系统中的"名字"从哪来：设备节点、用户组、服务名、主机名。
- **讲什么**：设备即文件（类型加主次号加权限的物化）与设备号编码硬件拓扑；`makedev` 的七参数统一入口与参数校验；真建与 MTREE"只说"双输出模式；`mknod` 与 `pack_dev` 的设备号打包；`dev_mkdb` 常量数据库的离线建与在线查；`getent` 的表驱动分派；系统数据库文件分工（`group`/`services`/`protocols`/`shells`/`hosts`/`motd`/`nsswitch.conf`）；`mtree` 目录规范；`devmand` 静态与动态节点管理的边界；`DESCRIBE` 与设备清单的关系；Rust 侧的 `LookupTable` 接口、服务认别名与组只认主名、解析上限与坏行策略。
- **不讲什么**：`devmand` server 本体（交给 `11-stage-devman`）；`gettytab`/`utmp`（交给 `06 §2.3`、`18 §2`）；主机名字典的查询策略与首次命中获胜（交给 `24 §1`）；`hosts`/`services`/`protocols` 三文件在网络侧的消费（交给 `24 §2`）；`mknod` 的文件系统语义（交给 `05-stage-vfs`）。
- **前置**：`04`（设备节点在启动链里被创建）、`06`（这些名字的消费者之一）、`11-stage-devman`（设备模型）。
- **后置**：`24 §1`（网络侧消费名字库）、`09`（文件对象操作消费设备节点）、`19`（终端设备节点）。
- **事实底线**：
  - C：`minix/commands/MAKEDEV/MAKEDEV.sh:56-59`（`makedev` 函数头与参数）、`:96`（`mknod` 调用）、`:210`（音频设备行）、`:213-218`（总线号展开）；`sbin/mknod/mknod.c`、`sbin/mknod/pack_dev.c:84`；`usr.sbin/dev_mkdb/dev_mkdb.c:54`（`FILE_PERMISSION`，注意旧 04 引的 `:56` 是错的）、`:64`、`:72`、`:88`、`:178`、`:214`；`usr.bin/getent/getent.c:98-136`、`:274`、`:809`；`usr.sbin/mtree/mtree.c`。
  - 非 C 制品：`etc/{group,services,protocols,shells,hosts,motd,nsswitch.conf,usr}`、`etc/mtree/NetBSD.dist.base`、`minix3/minix/commands/devmand/devmand.cfg`（范围外指针）、`minix/commands/DESCRIBE/DESCRIBE.sh`。
  - Rust：`os/commands/sbin/devdb/src/{lib,lookup,group,services,mtree}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-063、K-064 | 设备即文件；主次设备号编码硬件拓扑 | 概念 | `MAKEDEV.sh:210`、`:213-218` | 设备侧的第一原理 | 存量（旧 04 §1.1） |
| K-065、K-066、K-067 | `makedev` 七参数；双输出模式；`mknod` 与 `pack_dev` | 接口与协议/机制 | `MAKEDEV.sh:56-59`、`:96`；`pack_dev.c:84` | 设备节点的创建链 | 存量（旧 04 §2.1-2.2） |
| K-068、K-069、K-070 | `dev_mkdb`；`getent`；数据库分工 | 机制/架构演进/概念 | `dev_mkdb.c:64/88/178`；`getent.c:98-136`；`etc/*` | 名字库的建与查 | 存量（旧 04 §1.3-1.4、§2.3-2.4） |
| K-071 | `mtree` 目录规范 | 机制 | `usr.sbin/mtree/mtree.c`；`etc/mtree/NetBSD.dist.base` | 安装与审计共用标准 | 存量（旧 04 §2.5） |
| — | `devmand` 静态与动态节点管理的边界 | 机制 | `etc/rc.minix:200-203`；`minix/commands/devmand/` | 设备面的另一半 | 新增（指向 11-stage-devman） |
| K-072、K-073、K-074、K-075 | `LookupTable` 接口；别名与主名；解析上限；坏行策略 | 架构演进/概念/约束与不变量/机制 | `devdb/src/{lookup,services,group}.rs` | Rust 侧设计决策 | 存量（旧 04 §3） |
| K-076、K-077 | 错误码映射与模块；测试与留白 | 测试性质 | `devdb/src/` | 实现账本，指向 99 | 存量（旧 04 §4-§5，改指向 99） |

- **验收标准**：读者读完能回答——(1) `/dev/tty00` 是怎么被造出来的、它对应哪个驱动的第几个设备；(2) 加一个新的设备类要改哪几处、为什么要有"只说"模式；(3) 查一个服务名该用哪个命令、它去哪个文件找、找不到别名时行为如何；(4) 系统里一共有几张"名字表"，各自被谁消费。检查方式：本篇必须给出 `etc/` 名字表的逐文件清单（文件名、字段、消费者），并给 `MAKEDEV.sh` 至少三条设备行的解读示例。

### 08-shell 家族与命令执行环境

- **一句话定位**：用户与系统之间的翻译官，以及它交给命令的执行环境。
- **讲什么**：shell 的双重身份（交互翻译官与编程语言）；三种方言（ash、ksh、csh）的谱系与选用规则；启动文件四情形（登录、交互非登录、特权不一致、非交互）与 `ENV` 机制、真实标识与有效标识一致性检查；真实配置文件的内容（`/etc/profile`、`/etc/shrc`、`.profile`、`.shrc`）；内建命令的判定与 `builtins.def` 登记、双形态命令；引用感知分词；七种展开与"变量子集优先"；重定向两层（算子识别与压栈弹出）；求值器与作业控制的进程原语依赖链；执行环境面六件套与三条同族命令（`env`/`printenv`/`getopt`/`sysenv`/`hostname`/`uname` + `machine`/`pagesize`/`domainname`）；`chroot` 的根切换语义；Rust 侧"先文字层后执行层"的取舍与 `Environ` 只读接口、`Word` 片段串。
- **不讲什么**：`ksh`/`csh` 的语法细节（本 stage 只要声明方言差异与选用规则，各自的解析实现不在交付面，见 §9.3 的 OQ）；作业控制的进程组实现（交给 `14-stage-runtime` 的线程模型条目与 `04-stage-pm`）；命令工具本体（交给 `09`–`30`）；`echo`/`printf`/`test`/`expr` 四条双形态命令的外部程序契约（交给 `10` 与 `12`）；终端行规程（交给 `19`）；`PATH` 的查找机制（交给 `02 §2`）。
- **前置**：`06`（登录链交棒给 shell）、`03`（约定）。
- **后置**：`09`–`30` 全部命令族篇（shell 是它们的调用方）、`10 §1`（`cd`/`pwd` 的内建与外部双形态）、`19`（终端属性在 shell 侧的消费者）。
- **事实底线**：
  - C：`bin/sh/main.c:102-243`（`:122`、`:182-185`、`:198`、`:200`、`:203`、`:205-213`、`:231-232`、`:234-237`、`:251-306`、`:314-342`）、`bin/sh/options.c:88-136`（`:98-99`、`:113-118`、`:186-188`）；`bin/sh/parser.c`；`bin/sh/expand.c:138`、`:355`；`bin/sh/redir.c:79`（`struct redirtab`，旧 05 引的 `:99` 是错的）、`:98-104`（文件头注释，旧 05 引的 `99-119` 不实）、`:107`、`:130`；`bin/sh/eval.c`；`bin/sh/jobs.c:288`、`:349`；`bin/sh/builtins.def:74`、`:88`；`bin/ksh/`、`bin/csh/`；`usr.sbin/chroot/`；`usr.bin/{env,printenv,getopt,uname,machine,pagesize}`、`bin/{hostname,domainname}`、`minix/commands/sysenv/sysenv.c:59`、`bin/hostname/hostname.c:57`、`usr.bin/uname/uname.c:111`、`usr.bin/getopt/getopt.c:23`。
  - 非 C 制品：`etc/profile:7-17`、`etc/shrc:1-19`、`etc/skel/dot.profile:41`、`etc/skel/dot.shrc:3-4`、`etc/root/dot.profile:22`、`etc/{csh.cshrc,csh.login,csh.logout}`、`etc/hostname.file`、`include/paths.h:45`。
  - Rust：`os/commands/bin/shell/src/{lib,lexer,expand,redir,script}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-078、K-079 | 双重身份；三种方言 | 概念/架构演进 | `bin/{sh,ksh,csh}/` | shell 家族的地基 | 存量（旧 05 §1.1-1.2） |
| K-080、K-081、K-094 | 启动文件四情形；`ENV` 与标识一致性；真实配置文件内容 | 机制/工具与工程 | `bin/sh/main.c:198-213`；`etc/profile`、`etc/shrc` | shell 的执行环境从哪来 | 存量（旧 05 §1.3、§2.1）+ 新增 K-094 |
| K-082 | 内建判定与 `builtins.def`、双形态 | 概念 | `bin/sh/builtins.def:74/88` | 内建与外部的关系 | 存量（旧 05 §1.4、§2.6） |
| K-083、K-084、K-085、K-086 | 分词；七种展开；重定向两层；求值与作业控制依赖链 | 机制/数据结构 | `parser.c`；`expand.c:138/355`；`redir.c:79/107/130`；`eval.c`；`jobs.c:288` | shell 的四段机制 | 存量（旧 05 §2.2-2.5） |
| K-092、K-093 | 执行环境面六件套 + 三条同族命令 | 工具与工程 | `usr.bin/{env,printenv,getopt,uname,machine,pagesize}`、`bin/{hostname,domainname}`、`sysenv.c:59` | 它们是命令的执行环境面 | 存量（旧 05 §1.5，越界合法化）+ 新增 K-093 |
| K-111 | `chroot` 的根切换语义 | 概念 | `usr.sbin/chroot/` | 进程根状态而非文件操作 | 存量（旧 06 §1.3，越界移入） |
| K-087、K-088、K-089、K-090 | 先文字层后执行层；`Environ` 只读接口；`Word` 片段串；显式留白 | 架构演进/接口与协议/数据结构/约束与不变量 | `shell/src/{lib,expand,lexer}.rs` | Rust 侧设计决策 | 存量（旧 05 §3） |
| K-091 | 测试与未覆盖清单 | 测试性质 | `shell/src/` | 实现账本，指向 99 | 存量（旧 05 §5，改指向 99） |

- **验收标准**：读者读完能回答——(1) 打开一个终端敲 `echo $PATH` 时，这个 `PATH` 是哪几个文件按什么顺序设出来的；(2) 什么时候 shell 会去读 `.profile`，什么时候不读；(3) `cd` 与 `/bin/pwd` 的区别在什么地方，为什么必须有两个；(4) 环境变量到底存在哪、谁持有它；(5) `ksh` 与 `csh` 在这个项目里的交付面是什么。检查方式：本篇必须画出登录 shell 的三层配置加载顺序（`/etc/profile` → `.profile` → `$ENV`）并给每层行号；六件套 + 三同族命令必须各有 C 源锚点。

### 09-文件对象操作命令

- **一句话定位**：查看、搬动、改属性、建关系：文件系统上的日常动作。
- **讲什么**：四个家族的统一框架（改属性、建关系、搬运查看、空间与并行）；十二权限位与 `chmod` 双写法；链接与引用计数、`unlink`、目录删除的非空约束；`cp` 的三形态与四种属性策略（深讲）；`ls`（715 行）的列组合（深讲）；`find` 的表达式树与 `xargs` 搭档（深讲）；`cat` 的拼接语义；`mv` 的跨设备回退与 `rm` 的递归护栏；空间三件（`df`/`du`/`stat`）；建与删的小命令（`ln`/`mkdir`/`rmdir`/`touch`/`truncate`/`mkfifo`/`mktemp`/`xinstall`）；并发四件（`sync`/`flock` 与文件锁语义）；`chown`/`chroot` 的权限面（`chroot` 本体在 `08`）；族内差异表覆盖全部 24 条命令。
- **不讲什么**：路径字符串计算（`pwd`/`basename`/`dirname`/`pathchk`，交给 `10 §2`）；条件判断与输出（`test`/`expr`/`echo`/`printf`/`true`/`false`，交给 `10` 与 `12`）；文本处理（交给 `11`）；存储管理（交给 `20`–`23`）；设备节点（交给 `07`）；`find` 的谓词与 `xargs` 的参数分批属本篇，但正则匹配类谓词（`-regex`）不实现，理由在 §3 声明；Rust 侧模块与测试（交给 `99` 账本）。
- **前置**：`08`（shell 的调用面）、`14-stage-runtime`（文件系统调用）。
- **后置**：`10`（路径与真值工具建立在同一批文件系统调用上）、`11`（文本工具以文件为输入）、`20`–`23`（存储管理操作这些对象）、`23`（备份遍历依赖 `find` 的语义）。
- **事实底线**：
  - C：`bin/chmod/chmod.c:166`、`:216`；`bin/cp/cp.c:1-548`；`bin/ls/ls.c:1-715`；`usr.bin/find/find.c:1-306`；`usr.bin/xargs/`；`bin/{cat,mv,rm,ln,mkdir,rmdir,touch,ln}`、`minix/commands/truncate`、`usr.sbin/{link,unlink}`、`usr.bin/{xinstall,mkfifo,mktemp,stat,df,du,flock}`、`bin/{sync,chown}`、`usr.bin/{basename,dirname,pathchk}`（后三条归 `10`）。
  - Rust：`os/commands/bin/fileops/src/{lib,mode,testexpr,path,echo,expr,printf,pathchk,bin_support}.rs` 与 `src/bin/`（8 个）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-095 | 四个家族的分类法（判断求值组移出） | 概念 | 旧 06 §1.0 分组表 | 本篇的总图 | 存量（旧 06 §1.0） |
| K-096、K-097 | 十二权限位与 `chmod` 双写法；建关系家族 | 概念 | `chmod.c:166/216`；`usr.sbin/{link,unlink}` | 属性与关系两个面 | 存量（旧 06 §1.1-1.2） |
| K-098、K-099、K-100 | `cp`；`ls`；`find` 与 `xargs`（三个代表成员） | 机制 | `cp.c:1-548`；`ls.c:1-715`；`find.c:1-306` | 复杂度最高的三条 | 存量（旧 06 §2.3 等） |
| K-112、K-113 | `cat`；`mv` 与 `rm` | 机制 | `bin/{cat,mv,rm}` | 最常用命令必须有散文家族 | 新增（原只在表内） |
| K-101（空间与并发部分） | `df`/`du`/`stat`、`sync`/`flock`/`mkfifo`/`mktemp` | 工具与工程 | 各命令目录；`fileops/src/path.rs:18/40` | 小命令按用途归族 | 存量（旧 06 §1.3，路径组切给 10） |
| K-104 | `chmod.c` 外壳薄、库厚 | 架构演进 | `chmod.c:166/216` | 分层范例 | 存量（旧 06 §2.2） |
| K-106（文件侧） | 纯函数优先策略 | 架构演进 | `fileops/src/testexpr.rs:187-220` | Rust 侧决策 | 存量（旧 06 §3.1-3.2） |
| K-108（文件侧） | 关键类型与不变量 | 约束与不变量 | `fileops/src/{mode,path}.rs` | 边界钉成规则 | 存量（旧 06 §4.1-4.3） |
| K-109（文件侧） | 契约与 `Requires`（去陈旧状态） | 接口与协议 | `fileops/src/bin/`；`minix-sys/src/vfs.rs:627` | 账本条目 | 存量（旧 06 §4.5，改指向 99） |
| K-110 | 测试格局与留白 | 测试性质 | `cargo test -p minix-fileops` | 账本条目 | 存量（旧 06 §5，改指向 99） |
| K-429 | `cat` 的拼接语义与 `-n`（旧文献只在表内） | 机制 | `bin/cat/cat.c` | 最常用命令必须有散文家族（覆盖审计新增） | 新增（覆盖审计） |
| K-430 | `mv` 的跨设备回退与 `rm` 的递归护栏 | 机制 | `bin/{mv,rm}/*.c` | 同上 | 新增（覆盖审计） |

- **验收标准**：读者读完能回答——(1) `ln -s` 与 `ln` 的区别在 inode 层是什么；(2) 一次 `cp -R` 会对目标做哪几类属性决策、哪一类是 POSIX 规定的；(3) `ls -l` 的每一列从哪里来；(4) `find . -name '*.rs' -exec grep -l foo {} +` 的参数是怎么被分批传下去的；(5) `rm -rf` 遇到不可写目录时行为如何。检查方式：本篇必须给出一张覆盖 24 条命令的族内差异表（列：职责、关键选项、输入输出、退出码、错误面、`Requires`、状态），且每个命令至少属于一个家族。

### 10-路径、真值与输出工具

- **一句话定位**：在脚本里做路径计算、条件判断与输出。
- **讲什么**：路径四件（`pwd` 的当前目录来源、`basename`/`dirname` 的字符串切分、`pathchk` 的可移植性校验）与它们与文件系统调用无关的部分；条件判断 `test` 的四层递归求值与三类算子表（深讲）；`true`/`false` 的纯退出码语义；`echo` 的"只有首位 `-n` 是标志、禁止 `getopt`"怪癖（深讲）；`printf` 的运行时格式引擎（格式复用、`\c` 截停、base 0 数字解析、`check_conversion` 双警告，深讲）；族内差异表覆盖全部 9 条命令。
- **不讲什么**：`expr`（交给 `12 §1`，因为它的 `:` 需要 BRE）；文本行处理（交给 `11`）；shell 内建与外部程序的判定（交给 `08 §1`）；`test` 作为 shell 内建的包装（交给 `08 §2`）；浮点格式化的决策（见 §9.3 的 OQ-1）。
- **前置**：`09`（文件系统调用面）、`08`（脚本调用方）。
- **后置**：`11`（行工具重用这里的输出与退出码约定）、`28`/`30`（游戏篇以本篇的输入输出面为前置）、`12`（`expr` 的求值结构与 `test` 同族，交叉引用）。
- **事实底线**：
  - C：`bin/test/test.c:105-130`（三表）、`:160-167`（四层递归声明与 `filstat`/`t_lex`）；`bin/echo/echo.c:61`（禁止 `getopt` 的依据）；`usr.bin/printf/printf.c`；`bin/{true,false}`（`true.sh`/`false.sh` 脚本形态）；`bin/pwd`、`usr.bin/{basename,dirname,pathchk}`。
  - Rust：`os/commands/bin/fileops/src/{testexpr,echo,printf,pathchk,mode,path}.rs` 与 `src/bin/{echo,printf,expr,true,false,basename,dirname,pathchk}.rs`（`expr` 的语义归属见 `12`）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-102 | 判断求值家族与"退出码即返回值" | 概念 | 旧 06 §1.4 | 本篇的总图 | 存量（旧 06 §1.4） |
| K-103 | `test.c` 四层递归求值（代表成员 1） | 机制 | `test.c:105-130`、`:160-167` | 递归下降求值器的教科书样例 | 存量（旧 06 §2.1） |
| K-114 | `echo` 的 `-n` 怪癖（代表成员 2） | 约束与不变量 | `bin/echo/echo.c:61`；`fileops/src/echo.rs` | 最常用的输出命令 | 存量（旧 06 §1.4 提及，本篇深讲） |
| K-115 | `printf` 的运行时格式引擎（代表成员 3） | 机制 | `usr.bin/printf/printf.c`；`fileops/src/printf.rs` | 格式引擎与编译期格式的区别 | 存量（旧 06 §4.5 状态行） |
| K-101（路径部分） | `pwd`/`basename`/`dirname`/`pathchk` | 工具与工程 | `fileops/src/path.rs:18/40`、`pathchk.rs` | 路径字符串计算族 | 存量（旧 06 §1.3，从 09 切来） |
| K-107 | `FileTester` 接口双实现与 `who_mask` 掩码移交 | 接口与协议 | `testexpr.rs:25/71/112`；`mode.rs:60` | 用接口隔离系统 | 存量（旧 06 §3.3-3.4） |
| K-106（求值侧）、K-108（求值侧） | 纯函数优先；关键类型与不变量 | 架构演进/约束与不变量 | `fileops/src/testexpr.rs` | Rust 侧决策 | 存量（旧 06 §3.1-3.2、§4） |

- **验收标准**：读者读完能回答——(1) `test -n "$x"` 与 `[ -n "$x" ]` 是同一条命令吗，实现的层数有多深；(2) `echo -n foo` 与 `echo "-n" foo` 的输出为什么不同；(3) `printf '%d\n' 0x1f` 打出什么、依据是什么；(4) `pathchk -p` 校验的是什么、为什么它与文件系统无关。检查方式：本篇必须给出 `test` 的算子表与 `echo`/`printf` 的反例各至少两条，并各带 C 源行号。

### 11-文本行工具

- **一句话定位**：把文本当行序列来取、整、比、排、转、转储。
- **讲什么**：行序列的统一框架与六个家族（取段、整形、统计、比较与差异、排序与去重、字符转换与转储）；`uniq` 三元状态机（深讲）；`tr` 的字符集三形态编译与两阶段处理（深讲）；`sort` 的排序语义与执行策略之分（深讲）；`diff` 引擎（深讲）；`patch` 的安全应用（深讲）；`wc`/`head` 的分块投喂一致性；`cut` 的选项串即契约与区间去重；`fix` 的差异三块与"先核对后写"；`ifdef`/`unifdef` 的条件编译过滤；`cksum` 的 CRC 表生成；族内差异表覆盖全部 38 条命令（含 `look`/`ifdef`/`crc` 三处证伪注记）。
- **不讲什么**：`pr`（交给 `14 §1`）；`tsort`（交给 `15 §2`）；`fold` 的排版用法（在本篇讲语义，`14` 只引用）；正则匹配类工具（交给 `12`）；编辑器的行缓冲（交给 `13`）；多字节字符感知的统一升级（登记在 `12 §3` 与 `03 §5`，本篇只标注哪些命令受影响）。
- **前置**：`09`、`10`。
- **后置**：`12`（正则篇以本篇的行与字节直觉为前置）、`13`（编辑器复用行概念）、`14`（排版工具以本篇为前置）、`16`（压缩工具以本篇的字节与校验概念为前置）、`23`（`fix` 移到本篇后，备份篇只讲同步）。
- **事实底线**：
  - C：`usr.bin/uniq/uniq.c:56`、`:188`、`:190`；`usr.bin/cut/cut.c:88`、`:306`；`usr.bin/wc/wc.c`（354 行）、`usr.bin/head/head.c`（204 行）；`usr.bin/tr/tr.c:58-125`、`:283`；`usr.bin/sort/sort.c:1-418`、`init.c:239-242`；`minix3/usr.bin/diff/diffreg.c`；`usr.bin/patch/{pch.c,patch.c}`；`usr.bin/{tail,paste,join,comm,split,csplit,column,lam,rev,tee,fold,expand,unexpand,col,colrm,hexdump,vis,unvis,cksum,seq,jot,units,shuffle,uuidgen,yes,look,ifdef}`、`minix/commands/{crc,ifdef,fix}`、`lib/libc/gen/{vis.c:311-315,unvis.c:217-330}`。
  - Rust：`os/commands/usr-bin/textfilter/src/`（65 个源文件，含 `window.rs`、`count.rs`、`cut.rs`、`tr.rs`、`uniq.rs`、`sort.rs`、`diff.rs`、`patch.rs`、`pr.rs`（语义归 14）、`tsort.rs`（语义归 15））。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-117…K-122 | 六个家族概念与阅读顺序说明 | 概念 | 旧 07 §1.0-1.6 | 本篇的总图（重划六个家族） | 存量（旧 07 §1，`pr`/`tsort` 移出、`fix` 移入） |
| K-123 | `uniq` 三元状态机（代表 1） | 机制 | `uniq.c:56/188/190`；`textfilter/src/uniq.rs:43` | 最小的完整状态机范例 | 存量（旧 07 §2.1） |
| K-126 | `tr` 字符集三形态（代表 2） | 机制 | `tr.c:58-125`；`textfilter/src/tr.rs:23/86` | 字符集编译→流式处理架构 | 存量（旧 07 §2.4） |
| K-127 | `sort` 语义与策略之分（代表 3） | 架构演进 | `sort.c:1-418`；`textfilter/src/sort.rs` | 分清"比什么"与"放不下怎么办" | 存量（旧 07 §2.5） |
| K-133、K-134 | `diff` 引擎与 `patch` 应用（代表 4、5） | 机制 | `diffreg.c`；`patch/{pch,patch}.c` | 差异家族的两个深度成员 | 存量（旧 07 §4.5 状态行扩写） |
| K-124、K-125 | `cut` 选项串即契约；计数器两副面孔 | 机制 | `cut.c:88`；`textfilter/src/count.rs` | 取段与统计两族的机制 | 存量（旧 07 §2.2-2.3） |
| K-287 | `fix` 的差异三块与先核对后写 | 机制 | `fix/fix.c:10-14/38` | 差异家族的另一端 | 存量（旧 17 §2.6，越界移入） |
| K-128、K-129 | `LineWindow` 双实现；`CharClass` 三形态 | 接口与协议 | `textfilter/src/{window,tr,uniq}.rs` | Rust 侧接口设计 | 存量（旧 07 §3.1-3.4） |
| K-130、K-131、K-132 | 契约与 `Requires`（含三处证伪）；关键类型与不变量；测试与留白 | 测试性质 | `textfilter/src/`；`cargo test -p minix-textfilter` | 账本条目 | 存量（旧 07 §4-§5，改指向 99） |
| K-135 | 孤儿与错位清单的处置结果 | 工具与工程 | §3.3 重复主题表 | 让每个命令有唯一家族 | 存量（旧 07 的表内孤儿，本篇落实归位） |

- **验收标准**：读者读完能回答——(1) `head` 与 `tail` 的实现为什么不一样、内存开销差多少；(2) `tr -d 'a-c'` 的内部数据结构是什么；(3) `sort -k2,2n` 的键规格怎么解析、修饰符有哪些；(4) `diff` 的输出格式有几种、`patch` 如何在没有精确上下文时用 fuzz 救回来；(5) 哪些命令受多字节字符问题影响。检查方式：本篇的差异表必须覆盖 38 条命令且每条属于唯一家族；`pr`/`tsort` 必须只以指针形式出现。

### 12-正则语义与消费者

- **一句话定位**：用一种描述代替无数行的枚举，`grep`、`sed`、`expr` 三个消费者共用一套契约。
- **讲什么**：从枚举到描述（正则描述"形状"）；BRE/ERE 两种拼写的对照与助记；匹配语义（最左起点、最长终点、贪婪与 `.*` 陷阱）；回溯灾难与汤普森构造加派克虚拟机；`grep` 与 `sed` 的分工与退出码三值；`grep.c` 的标志变量区即目录；`util.c` 的单点匹配与 `-w` 词边界；`sed compile.c` 的地址文法与替换编译；`sed process.c` 的空匹配强制推进；`sed` 的动词面（`d`/`p`/`a`/`i`/`c`/`y`，新增深讲）；`expr` 的优先级梯与 `:` 的锚定基本正则（新增深讲）；Rust 侧"用虚拟机而不是回溯器"、"拒绝模式内后向引用"、"退出码写成纯函数"、"`parse_subst` 返回消耗长度"与字节取向架构注记；族内差异表覆盖 3 条命令。
- **不讲什么**：编辑器里的正则使用（交给 `13`）；`grep` 的递归遍历与上下文行、`sed` 的行号范围执行（执行层，见 §9.3 的 OQ-1）；`expr` 的算术与字符串求值结构（与本篇同族，本篇只讲 `:`，其余以指针指向 `10`）；多字节字符的统一升级（本篇登记为架构注记，见 §3.5）。
- **前置**：`11`（行与字节的直觉）。
- **后置**：`13`（编辑器共享同一套正则语法）、`14`（手册系统与排版工具的关键词搜索）、`10 §2`（`expr` 与本篇的关系在此声明）。
- **事实底线**：
  - C：`minix/usr.bin/grep/grep.c:67-86`、`:112-122`、`:281-451`、`:505`；`minix/usr.bin/grep/util.c:186`、`:205`；`usr.bin/sed/compile.c:122`、`:189-196`、`:339`、`:480`；`usr.bin/sed/process.c:403-432`、`:418-422`；`bin/expr/expr.y:107`、`:112`（`regcomp(..., REG_BASIC)`）、`:121`（`rm[0].rm_so == 0`）、`:44`（`#include <regex.h>`）。
  - Rust：`os/commands/usr-bin/regex/src/{lib,pattern,matcher,grep,sed,bin_support}.rs` 与 `src/bin/{grep,sed}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-136…K-140 | 描述语言、两种拼写、匹配语义、线性引擎、两工具分工 | 概念/机制 | 旧 08 §1 | 正则语言的地基 | 存量（旧 08 §1） |
| K-141、K-142 | `grep.c` 标志区；`util.c` 单点匹配 | 机制 | `grep.c:67-86`；`util.c:205` | `grep` 的机制 | 存量（旧 08 §2.1-2.2） |
| K-143、K-144 | `sed` 地址文法与替换编译；空匹配强制推进（代表 1、2） | 机制 | `compile.c:122/189-196/339`；`process.c:403-432` | `sed` 的机制 | 存量（旧 08 §2.3-2.4） |
| K-150 | `sed` 动词面（新增深讲 3） | 机制 | `compile.c:339` | 补齐 sed 的脚本语言面 | 存量（旧 08 §1.5 仅列举） |
| K-116 | `expr` 的优先级梯与 `:` 的锚定 BRE（新增深讲 4） | 机制 | `expr.y:112/121` | 正则的第三个消费者 | 存量（旧 06 §4.5，按序差-2 移入） |
| K-145、K-146、K-147、K-148 | 虚拟机而非回溯器；拒绝后向引用；退出码纯函数；消耗长度与字节取向 | 架构演进/约束与不变量/接口与协议/机制 | `regex/src/{pattern,grep,sed}.rs` | Rust 侧设计决策 | 存量（旧 08 §3） |
| K-149 | 关键类型与不变量、契约与测试 | 测试性质 | `regex/src/`；`cargo test -p minix-regex` | 账本条目 | 存量（旧 08 §4-§5，改指向 99） |

- **验收标准**：读者读完能回答——(1) `grep 'a*'` 与 `grep -E 'a*'` 在什么输入上结果不同、为什么；(2) 为什么这个项目的正则引擎不会因为一条输入爆炸；(3) `sed 's/x*/-/g'` 为什么不会死循环；(4) `expr "abc" : 'a\(.*\)'` 的结果是什么、它用的是哪种正则方言。检查方式：本篇必须给出 BRE 与 ERE 的完整对照表（含转义规则差异），并对三个消费者各给至少一条端到端示例与 C 源锚点。

### 13-编辑器

- **一句话定位**：住在行里改（`ed`）与在屏幕上改（`mined`）。
- **讲什么**：行编辑器的会话模型（地址、命令、修饰三段式，命令即脚本）；地址的七种命名法与逗号、分号的差异；全屏编辑器与 `ed`/`mined` 的分工；单步撤销的反操作记录与工程取舍；`vi` 缺位的三方选型代价（`[ARCH]` 悬置，见 §9.3 的 OQ-2）；`ed main.c` 的命令分派 `exec_command` 与地址范围守门（深讲）；行操作六组函数与 `TextStore` 最小公分母；缓冲、IO、撤销三层与 Rust 三模块同构；`re`/`sub`/`cbc`/`glbl` 的引擎、地址、拼接三层复用；Rust 侧 `TextStore` 双后端、读行为什么拷贝而非借用、`Address{基址,偏移}`、搜索地址解析但不求值；族内差异表覆盖 2 条命令。
- **不讲什么**：shell 的行编辑（交给 `08`）；终端控制与转义序列（交给 `19`）；屏幕绘制与按键解码的细节（终端的输入面，见 `03 §5` 的边界）；`vi` 补齐决策的实施（悬置项，见 OQ-2）；正则语法本身（交给 `12`）。
- **前置**：`12`（共享正则语法）、`08`（行编辑面在 shell 侧）。
- **后置**：`14`（排版工具以编辑器产出的行文本为输入）。
- **事实底线**：
  - C：`bin/ed/main.c:285`、`:314`、`:465`（命令分派）、`:481`、`:618`、`:898`、`:1051`、`:1100`、`:1137`、`:1181`、`:1217`、`:1242`、`:1271-1297`、`:1433`（文件总行数）；`bin/ed/ed.h:55`、`:56`、`:84-87`；`bin/ed/{buf.c,io.c,undo.c,re.c,sub.c,cbc.c,glbl.c}`；`minix/usr.bin/mined/mined1.c:1-1774`。
  - Rust：`os/commands/bin/editor/src/{lib,addr,cmd,store}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-151、K-153、K-154 | 会话模型；双编辑器分工；单步撤销 | 概念/机制 | 旧 09 §1.1/§1.3/§1.4 | 编辑器的概念地基 | 存量（旧 09 §1） |
| K-152 | 地址七种命名法与逗号/分号差异 | 机制 | `editor/src/addr.rs:31/72` | ed 的行命名语言 | 存量（旧 09 §1.2） |
| K-155 | `vi` 缺位与三方选型代价 | 架构演进 | 旧 09 §1.5 | 悬置决策的记录 | 存量（旧 09 §1.5） |
| K-156、K-157 | 命令分派与地址守门（代表成员 1）；行操作六组函数 | 机制 | `ed/main.c:465`；`:1051-1297` | ed 的执行主干 | 存量（旧 09 §2.1-2.2） |
| K-158、K-159 | 三层同构；引擎/地址/拼接三层复用 | 架构演进 | `bin/ed/{buf,io,undo}.c`；`{re,sub}.c` | 分层论证 | 存量（旧 09 §2.3-2.4） |
| K-160、K-161、K-162、K-163 | `TextStore` 双后端；读行为什么拷贝；`Address` 结构；解析先行 | 接口与协议/约束与不变量/数据结构/架构演进 | `editor/src/{store,addr}.rs` | Rust 侧设计决策 | 存量（旧 09 §3） |
| K-164 | 关键类型与测试 | 测试性质 | `editor/src/cmd.rs:74` | 账本条目 | 存量（旧 09 §4-§5，改指向 99） |

- **验收标准**：读者读完能回答——(1) `ed` 的地址有几种写法、`,` 与 `;` 的区别在哪一步生效；(2) 为什么 `ed` 的撤销只有一步、代价是什么；(3) `ed` 与 `sed` 共享哪一部分代码；(4) 这个项目要不要补一个全屏编辑器、卡在哪个决策上。检查方式：本篇必须给出一张地址文法表与一张命令分派表；`vi` 一节必须以"悬置"而非"结论"的措辞收尾，并指向 §9.3 的 OQ-2。

### 14-排版与手册

- **一句话定位**：从纯文本到能看的页面，以及命令自己的说明书系统。
- **讲什么**：三道工序框架（排版、手册、日历）；排版家族（分页编号 `pr`/`nl`、断行整理 `fmt`/`fold`、Minix 轻量链 `cawf`/`spell`/`prep`、列处理 `col`/`colcrt`/`deroff`/`checknr`/`soelim`/`ul`/`indent`）；手册系统的节与目录顺序、渲染规则解耦；`man` 与 `manconf` 的四步找页与配置指令解析（深讲）；`makewhatis` 索引器与查询三姐妹共享数据库格式（深讲）；`cal` 的格里高利改革参数化与双闰规则（深讲）；`calendar` 的日历提醒；族内差异表覆盖全部 22 条命令。
- **不讲什么**：排版引擎的实现（后续阶段）；拼写词典数据（数据面）；国际化实现（交给 `15 §3` 与 `99` 的 A-8 登记）；手册页内容本身（数据面）；`tsort`（交给 `15 §2`）；手册页的安装与索引机制（交给 `02 §3`）；压缩格式在 `man.conf` 的 `_build` 管道里的使用（交给 `16 §1`）。
- **前置**：`11`（行变换）、`12`（关键词搜索的理论）。
- **后置**：`16`（`man.conf` 的解压管道是压缩篇算法的消费者）、`15`（开发辅助与手册工具共享 `makewhatis` 的数据库形状）。
- **事实底线**：
  - C：`usr.bin/man/man.c:1-1088`、`usr.bin/man/manconf.c:1-272`；`libexec/makewhatis/makewhatis.c:1-1174`；`usr.bin/{apropos,whatis,whereis}`；`usr.bin/cal/cal.c:65-66`、`:94-137`（924 行）；`usr.bin/calendar/`；`usr.bin/{pr,nl,fmt,col,colcrt,deroff,checknr,soelim,ul,indent}`、`minix/commands/{cawf,spell,prep}`。
  - 非 C 制品：`etc/man.conf`（`_subdir`/`_build` 段）、349 个手册页。
  - Rust：`os/commands/usr-bin/doctools/src/{lib,manconf,whatis,cal}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-165、K-166、K-167 | 三道工序框架；排版家族；手册系统的解耦 | 概念/接口与协议 | 旧 10 §1.0-1.2；`etc/man.conf` | 本篇的总图 | 存量（旧 10 §1.0-1.2） |
| K-171、K-172 | `man`/`manconf` 找页（代表 1）；`makewhatis` 与查询三姐妹（代表 2） | 机制 | `man.c:1-1088`；`makewhatis.c:1-1174` | 手册系统的两个深度成员 | 存量（旧 10 §2.1-2.2） |
| K-169、K-173 | 日历的双闰规则与改革参数化（代表 3） | 机制 | `cal.c:65-66/94-137`；`doctools/src/cal.rs:63/94` | 双历法计算 | 存量（旧 10 §1.4、§2.3） |
| K-179 | `tsort` 是本文档的表内孤儿 | 机制 | 旧 10 §2.5 | 只在重复表里出现一次（移出） | 存量（旧 10 §2.5，移交 15） |
| K-183 | `calendar` 只在表内 | 工具与工程 | `usr.bin/calendar/` | 补上日历提醒命令 | 新增（原只在表内） |
| K-182（排版侧） | `ul`/`colcrt`/`deroff`/`checknr`/`soelim`/`indent` 的职责 | 工具与工程 | 各命令目录 | 排版辅助面 | 存量（旧 10 §1.1、§1.3） |
| K-175、K-176、K-177 | 解析保留原文；`ManDb` 双实现；双计数器 | 约束与不变量/接口与协议/数据结构 | `doctools/src/{manconf,whatis,cal}.rs` | Rust 侧设计决策 | 存量（旧 10 §3.1-3.3） |
| K-178（排版侧）、K-184、K-185 | 留白；关键类型与不变量；测试与留白 | 测试性质 | `doctools/src/` | 账本条目 | 存量（旧 10 §3.4、§4-§5，改指向 99） |

- **验收标准**：读者读完能回答——(1) 打一个 `pr -l 40 file` 出来的一页有哪些元素；(2) `man 3 printf` 里那个 `3` 是怎么被解析成目录的、配置在哪；(3) `apropos` 为什么比 `man -k` 快、索引是什么时候建的；(4) `cal 9 1752` 为什么少几天。检查方式：本篇必须给出 `man.conf` 的指令表与 `cal` 的改革切换测试选例（1752 年 9 月）。

### 15-开发辅助与国际化工具

- **一句话定位**：围绕源码的小工具与显式推迟的国际化决策。
- **讲什么**：开发辅助家族四组（索引 `ctags`/`lorder`、消息 `gencat`/`msgc`、拆分 `fsplit`/`mkstr`/`xstr`、宏 `m4`）；`m4` 的宏处理器词法与文法两段实现（深讲）；`tsort` 的拓扑排序（深讲，第三处出现且唯一归属）；`asa`/`fpr` 的输出格式处理；`indent` 的源码格式化；`gencat` 的消息目录格式与 `msgc` 的编译期消息抽取；国际化四件（`locale`/`mklocale`/`mkesdb`/`mkcsmapper`）与 `[ARCH] A-8` 的推迟口径（UTF-8 一统、字节即字符）；族内差异表覆盖全部 16 条命令。
- **不讲什么**：国际化实现（推迟项，`99` ARCH 登记表裁决，本篇只讲落地形态与推迟理由）；排版侧工具（`colcrt`/`deroff`/`checknr`/`soelim`/`ul`/`indent` 的排版用法归 `14`，本篇只覆盖它们的源码辅助面）；构建工具链的宿主面（交给 `27 §1`）；`lorder` 与链接顺序的关系（与 `02 §1` 的静态链接呼应）。
- **前置**：`11`、`12`。
- **后置**：`27`（构建辅助承接"源码周边工具"之后的一环）。
- **事实底线**：
  - C：`usr.bin/m4/{parser.y,tokenizer.l}`；`usr.bin/tsort/`；`usr.bin/{ctags,lorder,gencat,msgc,mkstr,xstr,fsplit,asa,fpr,indent}`；`usr.bin/{locale,mklocale,mkesdb,mkcsmapper}`（后四者含 `yacc.y`/`lex.l` 文法源）；`minix/usr.bin/` 的相关目录。
  - Rust：本期无专属 crate（`textfilter` crate 的 `tsort.rs` 是实现落点，语义归本篇，须在账本里标明这层错配）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-168 | 开发辅助家族四组 | 概念 | 旧 10 §1.3 | 本篇的总图 | 存量（旧 10 §1.3） |
| K-180 | `m4` 的宏处理器实现（代表成员 1） | 机制 | `usr.bin/m4/{parser.y,tokenizer.l}` | 宏展开语言的实现形态 | 新增（旧 10 §1.3 只有一句） |
| K-179 | `tsort` 的图排序语义（代表成员 2） | 机制 | `usr.bin/tsort/`；`textfilter/src/tsort.rs` | 唯一归属，消除双表重复 | 存量（旧 07 与旧 10 各一行，合并） |
| K-181 | `gencat`/`msgc`/`mkstr`/`xstr` 的消息目录与字符串抽取 | 工具与工程 | 各命令目录 | 消息面工具 | 存量（旧 10 §1.3） |
| K-170（落地形态） | 国际化四件与推迟口径 | 架构演进 | `usr.bin/{locale,mklocale,mkesdb,mkcsmapper}` | A-8 的落地形态 | 存量（旧 10 §1.5，裁决记录移 99） |
| K-178（开发辅助侧） | 留白与归属 | 架构演进 | 旧 10 §3.4 | 移交声明 | 存量（旧 10 §3.4） |

- **验收标准**：读者读完能回答——(1) `m4` 的输入要经过几段处理、宏定义存在哪；(2) `tsort` 为什么属于源码辅助而不是文本行工具；(3) 消息目录是怎么从源码里的标记生成出来的；(4) 这个项目对多字节字符的处理口径是什么、依据在哪一条决策。检查方式：`tsort` 一节必须给出它在 `07`/`10` 两处的旧位置与本篇的唯一归属说明；i18n 一节必须以"推迟"措辞收尾并指向 `99` 的 A-8 登记行。

### 16-压缩与归档

- **一句话定位**：三种"变小"与两种"打包"。
- **讲什么**：三种"变小"（压缩、归档、编码）的正交关系与发行叠加链；LZW 的字典压缩思想（清空码 256、结尾码 257、变宽编码）；传输编码（3 字节到 4 个六位组、`uu` 与 base64 同思想）；归档（`pax` 多格式互转、`shar` 归档即脚本）；完整性（CRC32 与 Adler32）；`compress.c` 的完整形态（深讲）；`uuencode.c` 的两编码器分派与长度字节（深讲）；`shar.sh` 的归档即脚本与逆过程解析（深讲）；宽度切换点 2 的幂减一与"解码器提前一级对冲滞后一码"（本篇最硬的正确性论证，必须保留规格与 C 源双锚）；Rust 侧的哈希字典选型、校验按形状立接口；族内差异表覆盖 10 条命令。
- **不讲什么**：压缩算法文件格式封装（后续阶段）；`bzip2` 变换链与 `gzip` 形变编码（后续算法阶段）；DES 分组密码实现（后续密码阶段，本篇只定位置）；`man.conf` 的 `_build` 管道（交给 `14 §2`）。
- **前置**：`09`（搬文件）、`14`（手册压缩格式的消费方）。
- **后置**：`23`（备份数据的压缩形态）、`25`（`zmodem` 的传输编码与 `uuencode` 同族，交叉引用）。
- **事实底线**：
  - C：`minix/commands/compress/compress.c:20-42`、`:974`（1618 行）；`usr.bin/uuencode/uuencode.c:47`、`:63-64`、`:109-113`（202 行）；`usr.bin/shar/shar.sh`；`usr.bin/gzip/gzip.c:1-2114`、`usr.bin/unzip/unzip.c:1-1074`、`usr.bin/bdes/bdes.c:1-1076`、`bin/pax/`。
  - Rust：`os/commands/usr-bin/compress/src/{lib,lzw,uu,shar,checksum}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-186 | 三种"变小"的正交关系 | 概念 | 旧 11 §1.0 | 本篇的总图 | 存量（旧 11 §1.0） |
| K-187、K-192 | LZW 字典压缩思想与完整形态（代表成员 1） | 机制 | `compress.c:20-42/974`；`compress/src/lzw.rs:48/102` | 字典算法的完整形态 | 存量（旧 11 §1.1、§2.1） |
| K-188、K-193 | 传输编码与 `uuencode.c`（代表成员 2） | 机制 | `uuencode.c:47/63-64/109-113` | 二进制过纯文本通道 | 存量（旧 11 §1.2、§2.2） |
| K-189、K-194 | 归档与 `shar.sh`（代表成员 3） | 概念/机制 | `bin/pax/`；`shar.sh`；`compress/src/shar.rs:50` | 归档即脚本 | 存量（旧 11 §1.3、§2.3） |
| K-190 | 完整性（CRC32 与 Adler32） | 机制 | `compress/src/checksum.rs:13` | 静默损坏防护 | 存量（旧 11 §1.4） |
| K-195 | 大块头职责级覆盖（`gzip`/`unzip`/`bdes`/`pax` 的边界） | 工具与工程 | `gzip.c:1-2114` 等 | 算法家族边界 | 存量（旧 11 §2.4） |
| K-196、K-197、K-198 | 哈希表选型；宽度切换点；接口按形状 | 数据结构/约束与不变量/接口与协议 | `compress/src/{lzw,checksum}.rs` | Rust 侧设计决策 | 存量（旧 11 §3.1-3.3） |
| K-199、K-200 | 留白；关键类型与测试 | 测试性质 | `compress/src/` | 账本条目 | 存量（旧 11 §3.4、§4-§5，改指向 99） |

- **验收标准**：读者读完能回答——(1) `.Z` 文件的码流在什么时刻改变位宽、解码器为什么要提前一级；(2) `uuencode` 与 `base64` 的关系是什么；(3) `shar` 生成的文件为什么本身就是一个可执行脚本；(4) `cksum` 与 `compress` 用的校验和是不是同一个。检查方式：本篇必须给出 LZW 的位宽切换表（9 至 16 位）与至少一条跨切换点的往返测试向量；`gzip`/`unzip`/`bdes` 三条必须以"本阶段不做"的口径出现并指明归属。

### 17-进程与信号工具

- **一句话定位**：看进程在干什么，以及如何让它停、让它换优先级。
- **讲什么**：进程视角的统一框架（内核进程表是唯一数据源）；`ps` 的列定义驱动（`keyword.c` 的宏表，深讲）；32 个信号的编号与语义序列；`kill` 的名号双通道与特判（深讲）；信号参数边界（0 与 33 以上非法）；优先级与批处理四件（`nice`/`renice`/`nohup`/`time`）；进程表的上层读者（`mtop`/`ministat`/`toproto`）；System V IPC 两件（`ipcs`/`ipcrm`）；`ProcessRow` 最小三字段与 `ProcessTable` 接口；错误码贴源不贴惯（`NotFound` 映射 3）；族内差异表覆盖 11 条命令。
- **不讲什么**：会话记录与 `utmp`（交给 `18`，两篇的分界是数据源：内核进程表 vs `utmp` 文件）；`logger` 与消息四件（交给 `18`）；时间工具（`date`/`sleep` 交给 `18`）；`ps` 的执行面（读取内核表的机制，见 §9.3 的 OQ-1 与 `03 §5` 的边界）；`loadkeys` 等设备控制（交给 `19`）。
- **前置**：`08`（shell 的调用面）、`03`（约定）。
- **后置**：`18`（同一批用户空间视角的另一半）、`26`（系统信息篇的 `sysctl` 与参数观察与本篇呼应）。
- **事实底线**：
  - C：`bin/ps/keyword.c:81`、`:94`、`:96`、`:114-197`；`bin/kill/kill.c:83`、`:105`、`:119-127`、`:178`、`:188-195`（226 行）；`sys/sys/signal.h:52-84`；`bin/{date,nice,sleep}`、`usr.bin/{renice,nohup,time,ipcs,ipcrm}`、`minix/usr.bin/{ministat,mtop,toproto}`。
  - Rust：`os/commands/bin/proctools/src/{lib,stable,signal,ptime,utmp}.rs`（`stable.rs` 的进程表在本篇，`utmp.rs` 的读者在 `18`）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-201（进程侧） | 看护进程的视角 | 概念 | 旧 12 §1.1 | 本篇的总图 | 存量（旧 12 §1.0-1.1） |
| K-202 | `ps` 列定义驱动（代表成员 1） | 机制 | `keyword.c:81/94/96/114-197` | ps 输出由列定义驱动 | 存量（旧 12 §2.2、§3.4） |
| K-203 | 群体视角三件套 | 概念 | `minix/usr.bin/{ministat,mtop,toproto}` | 进程表的上层读者 | 存量（旧 12 §1.1） |
| K-204、K-205、K-206 | 32 个信号；`kill` 名号双通道（代表成员 2）；参数边界 | 数据结构/机制/约束与不变量 | `signal.h:52-84`；`kill.c:83/105/178`；`proctools/src/signal.rs` | 信号族的完整面 | 存量（旧 12 §1.2、§2.1、§3.1） |
| K-210（进程侧） | 优先级与批处理四件、IPC 两件 | 概念 | `usr.bin/{renice,nohup,time,ipcs,ipcrm}` | 按数据源归族 | 存量（旧 12 §1.4，越界重分） |
| K-212、K-213、K-214 | 时长四形态；错误码贴源；`ProcessRow`/`ProcessTable` | 机制/约束与不变量/架构演进 | `proctools/src/{ptime,lib,stable}.rs` | Rust 侧设计决策 | 存量（旧 12 §3.2-3.4） |
| K-215、K-216、K-217 | 模块对应；测试性质；`ps` 数据源留白 | 测试性质 | `proctools/src/` | 账本条目 | 存量（旧 12 §4-§5，改指向 99） |

- **验收标准**：读者读完能回答——(1) `ps` 的默认输出列是哪几列、这一组列是怎么被选出来的；(2) `kill -HUP` 与 `kill -1` 是不是一回事、名字从哪张表来；(3) `kill -0` 为什么语义特殊、在这个项目里支持吗；(4) `nice` 与 `renice` 的区别在哪一层；(5) 为什么 `ps` 在本阶段只做了列定义。检查方式：本篇必须给出 32 个信号的完整名号对照表（与 `sys/sys/signal.h:52-84` 逐条对齐）与 `keyword.c` 的列宏一览。

### 18-会话记录与运行期小工具

- **一句话定位**：谁在用系统，以及会话之间如何互相打招呼。
- **讲什么**：`utmp` 36 字节记录契约（主讲述点）与三方消费者（写入侧 `login`、清空侧 `rc.minix`、读取侧 `who` 系）；记录解析三动作与撕裂写入必须报错；`who` 系十种读法（`who`/`w`/`last`/`users`/`finger`/`logname`/`tty`）；终端消息四件（`mesg`/`wall`/`write`/`from`）与提醒 `leave`、终端锁 `lock`；时间两件（`date`/`sleep`）与时间格式；日志客户端 `logger` 与 `syslogd` 的关系（指针到 `25 §2`）；日志轮转 `rotate` 的世代顺序；族内差异表覆盖 17 条命令。
- **不讲什么**：`utmp` 的写入与清空机制（交给 `06 §2.5` 与 `04 §2.7`，本篇只讲记录形状与读法）；`syslogd` 的分拣规则（交给 `25 §2`，本篇只讲客户端怎么发）；内核进程表（交给 `17`）；`shlock`（交给 `05 §2.7`）；邮件 `from` 的邮件系统面（交给 `25 §1`）。
- **前置**：`06`（登录链写入 `utmp`）、`08`（调用方）。
- **后置**：`25 §2`（日志服务端承接 `logger`）、`23`（`rotate` 与备份篇的维护面交叉引用）。
- **事实底线**：
  - C：`lib/libc/compat/include/utmp.h:42-46`（36 字节定宽）；`login/common.c:166+`（写入）；`etc/rc.minix:155-156`（清空）；`usr.bin/{who,w,last,users,finger,logname,tty,mesg,wall,write,from,leave,lock,logger,date,sleep}`；`minix/commands/rotate/rotate.sh`。
  - Rust：`os/commands/bin/proctools/src/utmp.rs`（实现落在 `proctools` crate，语义归本篇，账本里标明这层错配）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-201（会话侧）、K-209 | 会话视角；`who` 系十种读法 | 概念 | `proctools/src/utmp.rs` | 本篇的总图 | 存量（旧 12 §1.3） |
| K-207、K-424 | `utmp` 36 字节记录（**主讲述点**）与三方契约 | 数据结构 | `utmp.h:42-46`；`login/common.c:166+`；`etc/rc.minix:155-156`；`usr.bin/{who,w,last}` | 会话真相的唯一来源 | 存量（旧 12 §1.3、§2.3）+ 新增 K-424 |
| K-208 | 记录解析三动作与撕裂写入 | 约束与不变量 | `proctools/src/utmp.rs:73` | 不静默丢失会话记录 | 存量（旧 12 §1.3、§2.3） |
| K-210（消息侧） | 消息四件、提醒、终端锁、时间两件 | 工具与工程 | `usr.bin/{mesg,wall,write,from,leave,lock,date,sleep}` | 按数据源归族 | 存量（旧 12 §1.4，越界重分） |
| K-286 | `rotate` 的日志世代顺序 | 机制 | `minix/commands/rotate/rotate.sh` | 日志管理的维护动作 | 存量（旧 17 §2.6，越界移入） |
| K-215（会话侧）、K-216（会话侧） | 模块对应与测试 | 测试性质 | `proctools/src/utmp.rs` | 账本条目 | 存量（旧 12 §4-§5，改指向 99） |

- **验收标准**：读者读完能回答——(1) `who` 的输出有哪几列、每一列在 36 字节里的偏移是多少；(2) 一条会话记录从产生到消失经过哪些程序；(3) `mesg n` 之后别的用户还能不能给我发消息、判定在哪一步；(4) `logger` 发出的消息最终被谁分拣、规则在哪个文件。检查方式：本篇必须给出 `utmp` 记录的字段偏移表（与 `utmp.h:42-46` 逐字段对齐）与至少三条 `who` 系命令的读法对比。

### 19-终端控制与能力数据库

- **一句话定位**：终端的行属性、能力表，以及键盘字体屏幕三件工具。
- **讲什么**：终端的三层规矩（行属性、能力数据库、设备小工具）；波特率 19 档标准速度表与"速度 0 即挂断"；控制字符三属性与 21 项表（主表 18 加别名 3）；Minix 退格默认是 `Control-H` 的由来；控制字符的命令行记法；四组标志与 `modeset` 查表顺序决定重名归属（深讲）；16 个常用标志词的子集边界；termcap 条目结构（别名加三类能力，深讲）；能力查询三方法与"无不是零"；两代终端数据库工具链；**`termcap` 不是命令**的纠正；键盘、字体、屏幕三件小工具与 `etc/fonts`；`[ARCH] A-2` 决策的落地形态；族内差异表覆盖 10 条命令（含撤销 `termcap` 后的 9 条实命令）。
- **不讲什么**：终端驱动与行规程实现（交给 `16-stage-drivers` 与 `14-stage-runtime`）；terminfo 数据是否装船的裁决（交给 `99` 的 A-2 登记行）；终端游戏的转义序列用法（交给 `29`）；`getty` 对终端的认领（交给 `06 §2.1`）；`ldterm`/`ptem` 之类流模块（不在本 stage）。
- **前置**：`08`（shell 与终端的关系）、`03`（约定）。
- **后置**：`29`（终端游戏以能力序列为前置）、`18`（终端消息工具使用同样的终端属性面）。
- **事实底线**：
  - C：`bin/stty/stty.c:137`；`bin/stty/cchar.c:60-82`；`bin/stty/modes.c:65-175`、`:208`；`bin/stty/print.c:71-73`；`sys/sys/termios.h:50-79`；`sys/sys/ttydefaults.h:65-69`；`minix/commands/{term,tget,loadfont,loadkeys,screendump}`；`usr.bin/{tput,tic,infocmp}`；**不存在** `minix3/*/termcap/` 命令目录（`grep -l` 实测），只有 `etc/termcap`、`etc/termcap.big` 与 `lib/libterminfo/termcap.c`。
  - 非 C 制品：`etc/termcap`、`etc/termcap.big:12541`、`etc/fonts/`。
  - Rust：`os/commands/bin/termctl/src/{lib,baud,cchar,caps,stty}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-219 | 终端三层规矩 | 概念 | 旧 13 §1.0 | 本篇的总图 | 存量（旧 13 §1.0） |
| K-220、K-222 | 速度表与速度 0；退格默认值 | 概念/约束与不变量 | `stty.c:137`；`ttydefaults.h:65-67` | 行属性的两个易错点 | 存量（旧 13 §1.1-1.2） |
| K-221、K-223 | 控制字符三属性与 21 项表（代表成员 1）；命令行记法 | 数据结构/接口与协议 | `cchar.c:60-82`；`termios.h:50-79` | `stty` 的三个深度面之一 | 存量（旧 13 §1.2、§2.1、§3.2） |
| K-224、K-225 | 四组标志与查表顺序（代表成员 2）；子集边界 | 机制/架构演进 | `modes.c:65-175/208`；`termctl/src/stty.rs` | `stty` 的解析次序 | 存量（旧 13 §1.3、§2.2、§3.3） |
| K-226、K-227 | termcap 条目结构（代表成员 3）；查询三分法 | 数据结构/接口与协议 | `etc/termcap.big:12541`；`termctl/src/caps.rs:168` | 能力数据库 | 存量（旧 13 §1.4、§2.4、§3.4） |
| K-228、K-230 | 两代工具链；键盘字体屏幕三件 | 概念 | `minix/commands/{term,tget,loadfont,loadkeys,screendump}`；`usr.bin/{tput,tic,infocmp}` | 工具面 | 存量（旧 13 §1.4-1.5） |
| — | **`termcap` 不是命令**（撤销幽灵命令行） | 工具与工程 | 实测无该命令目录 | 纠正契约表 | 新增 |
| K-229 | A-2 决策的落地形态 | 架构演进 | 旧 13 §1.4；`99` A-2 登记行 | 决策在本篇的形态 | 存量（旧 13 §1.4，裁决记录移 99） |
| K-231 | 速度打印、`SttyOp` 解析、契约与测试 | 测试性质 | `print.c:71-73`；`termctl/src/` | 账本条目 | 存量（旧 13 §1.6、§4-§5，改指向 99） |

- **验收标准**：读者读完能回答——(1) 一个控制字符的"名字、槽位、默认值"分别从哪来；(2) `stty -a` 的输出里输入速度与输出速度什么时候合打；(3) `stty sane` 里的 `sane` 是从哪张表查出来的、如果两张表都有这个名字会怎样；(4) 为什么这个项目的 `stty` 只支持 16 个标志词；(5) `termcap` 到底是命令还是数据库。检查方式：本篇必须给出 21 项控制字符表与 16 个标志词表，并显式写出一句"`termcap` 在本项目里不是命令"。

### 20-挂载与检查

- **一句话定位**：把存储嫁接到目录树，并按序号排队做体检。
- **讲什么**：挂载的语义（根嫁接、摘除纪律、忙则拒绝）；挂载选项作为安全契约（只读、禁执行、忽略特殊位、同步写、不更新访问时间、无设备文件）与"未知选项必须响亮失败"；`fstab` 六字段（深讲）；检查序号的 discipline（根 1 先行、余下升序、同号保表序）与序号 0 永不检查（深讲）；`preen` 的两档无人值守；`fsck` 族一文件系统一检查器；`mount.c` 的命令行形状；稳定排序与"输入顺序有意义"；`MountTable` 的三实现；`newfstab.sh` 与表的机器友好来源；**`umount` 的完整一节**（新增）；**命令到 FS 服务端的调用接缝**（新增）；族内差异表覆盖 6 条命令。
- **不讲什么**：文件系统服务端实现（交给 `05-stage-vfs` 与 `15-stage-fs`）；块驱动（交给 `16-stage-drivers`）；检查遍的实现（交给 `15-stage-fs`）；分区与格式化（交给 `21`）；`fattr.c` 的文件属性面（交给 `09`）；`fsck` 的执行体（`fsck.mfs`/`fsck_ext2fs` 的内部结构归 `15-stage-fs`）。
- **前置**：`09`（文件对象）、`15-stage-fs`（挂载调用的另一端）、`03`（约定）。
- **后置**：`21`（承接"卷从哪来"）、`23`（备份以已挂载的卷为对象）。
- **事实底线**：
  - C：`minix/commands/mount/mount.c:41-60`、`:143-147`、`:168`；`minix/commands/umount/umount.c`（全文件，新增节）；`sbin/fsck/fsck.c:254`、`sbin/fsck/preen.c`；`sbin/fsck_ext2fs/`、`minix/commands/fsck.mfs/`；`sbin/mount/fattr.c`、`sbin/mount/mountprog.h`。
  - 非 C 制品：`etc/newfstab.sh`、`etc/fstab` 的形状（安装期生成）。
  - Rust：`os/commands/sbin/mountinfo/src/{lib,fstab,options,order}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-232、K-431 | 挂载语义与摘除纪律；`umount` 的忙判定与强制选项 | 概念/机制 | `umount.c` | 挂载的两端 | 存量（旧 14 §1.1）+ 新增 K-431 |
| K-233、K-234、K-235 | 选项作为安全契约；未知选项必失败；`ro` 查存在 | 约束与不变量 | `mount.c:143-147`；`mountinfo/src/options.rs` | 选项语义 | 存量（旧 14 §1.1、§3.1、§4.2） |
| K-236 | `fstab` 六字段（代表成员 1） | 数据结构 | `mountinfo/src/fstab.rs` | 挂载表格式 | 存量（旧 14 §1.2） |
| K-237、K-238、K-239 | 检查序号 discipline（代表成员 2）；序号 0；`preen` 两档 | 机制/约束与不变量/概念 | `fsck.c:254`；`preen.c` | 体检顺序规则 | 存量（旧 14 §1.3、§2.2） |
| K-240、K-241、K-244 | fsck 族；`mount.c` 命令行形状；`newfstab.sh` | 架构演进/接口与协议/工具与工程 | `sbin/fsck*/`；`mount.c:41-60`；`etc/newfstab.sh` | 命令面与来源 | 存量（旧 14 §2.1、§2.3-2.4） |
| K-425、K-426 | 命令到 FS 服务端的调用接缝（`mount` 请求形状；`fsck` 读块与 `preen` 退出语义） | 接口与协议 | `mount.c`；`fsck.c:254`；`mountinfo/src/` | 补上被整体推给 15-stage-fs 的那一半 | 新增 |
| K-242、K-243 | 稳定排序；`MountTable` 三实现 | 机制/数据结构 | `mountinfo/src/order.rs:27/69/101` | Rust 侧设计决策 | 存量（旧 14 §3.2-3.3） |
| K-245 | `fattr.c` 与模块结构、测试 | 测试性质 | `mountinfo/src/order.rs:135` | 账本条目（`fattr` 改指针到 09） | 存量（旧 14 §2.4、§4-§5，改指向 99） |

- **验收标准**：读者读完能回答——(1) `/etc/fstab` 里 `fsck` 那一列填 0、1、2 分别会发生什么；(2) 开机时哪些条目会被体检、按什么顺序；(3) `mount -o noexec,ro` 的实际效果与拒绝条件；(4) 一条未知挂载选项过来时命令的行为为什么是报错而不是忽略；(5) 命令发出挂载请求时经过什么接口。检查方式：本篇必须给出 `fstab` 字段表、检查序号排序的算例，以及一张"命令侧请求 → 服务端"的接缝图（后者只画请求形状，不写服务端实现）。

### 21-分区与格式化

- **一句话定位**：切分、命名、格式化：卷从哪来。
- **讲什么**：MBR 首扇区布局与分区项字段（深讲）；几何三字节"存而不算"；魔数不对即"非表"；类型码身份声明与"未知返回无名不猜"；激活标志与溢出保护；起止换算"起加长减一"；分区工具链五操作面（`fdisk`/`part`/`partition`/`autopart`/`repartition`）；准备链两端（`format`/`devsize`）；格式化家族（四个 `newfs_*` 加 `makefs` 加 `mkfs`）；人类单位解析与扇区换算的 checked 全程；`dosread` 的 FAT 摆渡（从 `22` 移入）；`bootblock.h` 作为布局权威来源；**命令打开设备与写卷的接缝**（新增）；族内差异表覆盖 13 条命令。
- **不讲什么**：文件系统内部结构（交给 `15-stage-fs`）；存储驱动（交给 `16-stage-drivers`）；镜像与内存盘（交给 `22`）；FAT 解析实现（后续文件系统阶段）；`installboot` 的引导块写入（交给 `27 §2`，本篇只讲分区表布局）。
- **前置**：`20`（阅读顺序先用后备，见序差-3）、`15-stage-fs`。
- **后置**：`22`（镜像与介质）、`23`（备份建立在已格式化卷上）。
- **事实底线**：
  - C：`sys/sys/bootblock.h:150-182`、`:204-208`、`:279-328`、`:299-300`、`:328`、`:703`、`:703-743`、`:703-714`；`minix/commands/part/part.c:691-702`；`minix/commands/fdisk/fdisk.c:380`；`minix/commands/{partition,autopart,repartition,format,devsize,dosread}`；`sbin/newfs_ext2fs`、`sbin/newfs_msdos`、`sbin/newfs_udf`、`sbin/newfs_v7fs`；`usr.sbin/makefs`。
  - Rust：`os/commands/sbin/diskfmt/src/{lib,mbr,size}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-246、K-247 | MBR 布局与分区项字段（代表成员 1） | 数据结构 | `bootblock.h:204-208/703-743` | 分区表的物理布局 | 存量（旧 15 §1.1、§2.1） |
| K-248 | 几何三字节存而不算 | 架构演进 | `bootblock.h:703-714` | 不误用遗产字段 | 存量（旧 15 §1.1、§3.1） |
| K-249、K-251 | 魔数不对即非表；激活标志与溢出 | 约束与不变量 | `diskfmt/src/mbr.rs` | 解析层的边界 | 存量（旧 15 §1.1、§4.2） |
| K-250 | 类型码身份声明 | 数据结构 | `bootblock.h:299-300/328`；`diskfmt/src/mbr.rs` | 不冒险猜类型码 | 存量（旧 15 §1.2、§3.2） |
| K-252、K-258 | 起止换算；`fdisk` 表指针定位 | 机制/工具与工程 | `part.c:691-702`；`fdisk.c:380` | 读写两端 | 存量（旧 15 §2.2-2.3） |
| K-253、K-254、K-255 | 五操作面；准备链两端；格式化家族 | 概念 | 各命令目录 | 工具面 | 存量（旧 15 §1.3-1.4） |
| K-256、K-257 | 人类单位解析；扇区换算 | 机制 | `diskfmt/src/size.rs` | 数值边界 | 存量（旧 15 §1.4、§3.3、§4.2-4.3） |
| K-272 | `dosread` 的 FAT 摆渡 | 概念 | `minix/commands/dosread` | 文件系统邻域 | 存量（旧 16 §1.3，越界移入） |
| K-427 | 命令打开设备与写卷的接缝 | 接口与协议 | `sbin/newfs_*`、`usr.sbin/makefs`、`diskfmt/src/size.rs` | 补上执行面的一半 | 新增 |
| K-259 | `bootblock.h` 权威来源与测试 | 测试性质 | `diskfmt/src/size.rs:58` | 账本条目 | 存量（旧 15 §2.1、§5，改指向 99） |

- **验收标准**：读者读完能回答——(1) 一个 16 字节分区项里每个字节是什么；(2) 扇区换算时 1M 的卷要多少扇区、为什么是"进一"而不是"截断"；(3) `fdisk`、`part`、`partition`、`autopart` 各适合什么场合；(4) 拿到一个魔数不对的扇区，命令应该报"坏盘"还是"没有分区表"。检查方式：本篇必须给出 MBR 首扇区的字节布局表（含偏移与长度）与一张 `bootblock.h` 的"注释区/宏区/类型码区/结构区"四段索引。

### 22-镜像与介质

- **一句话定位**：搬运整个盘：块复制、光盘镜像、内存盘与虚拟盘。
- **讲什么**：三件事框架（块复制、光盘镜像、内存盘与虚拟盘）；`dd` 的操作数语言 17 项（深讲）；块长后缀与连乘；`if`/`of` 缺省与"终极危险"；`conv` 六标志与五个"识别但透传"的转换；`CopyPlan` 的全量字节结算；ISO9660 主卷描述符布局（深讲）；端序互校挡撕裂扇区与"标识即本质"；`BlockDevice` 接口与两实现；`ramdisk`/`loadramdisk`/`vnconfig`/`rawspeed`；介质三件与生成端（`vol`/`eject`/`cdprobe`/`writeisofs`）；**块读写接缝与 `BlockDevice` 抽象的对应**（新增）；族内差异表覆盖 12 条命令。
- **不讲什么**：驱动实现（交给 `16-stage-drivers`）；介质控制的硬件面（弹出与探测的设备侧，交给驱动）；FAT 解析（交给 `21 §1`）；分区表（交给 `21`）；挂载（交给 `20`）。
- **前置**：`20`、`21`。
- **后置**：`23`（备份介质与本篇的介质面交叉引用）。
- **事实底线**：
  - C：`bin/dd/args.c:105-121`、`:107`（`conv` 表）；`minix/commands/isoread/isoread.c:41`；`minix/commands/{writeisofs,vol,eject,cdprobe,ramdisk,loadramdisk,rawspeed}`；`usr.sbin/vnconfig`。
  - Rust：`os/commands/bin/diskimg/src/{lib,dd,iso,device}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-260 | 三件事框架 | 概念 | 旧 16 §1.0 | 本篇的总图 | 存量（旧 16 §1.0） |
| K-261、K-262、K-263、K-264 | `dd` 操作数语言（代表成员 1）；块长后缀；缺省与危险；`conv` 六标志 | 接口与协议/机制/概念/约束与不变量 | `args.c:105-121/107`；`diskimg/src/dd.rs` | 搬运的通用语言 | 存量（旧 16 §1.1、§2.1） |
| K-265 | `CopyPlan` 全量字节结算 | 架构演进 | `diskimg/src/dd.rs` | Rust 侧设计决策 | 存量（旧 16 §3.2） |
| K-266、K-267 | ISO9660 主卷描述符（代表成员 2）；端序互校 | 数据结构/约束与不变量 | `isoread.c:41`；`diskimg/src/iso.rs` | 光盘识别 | 存量（旧 16 §1.2、§2.2、§3.3） |
| K-269、K-270、K-271 | `ramdisk`/`loadramdisk`；`vnconfig`；`rawspeed` | 概念 | 各命令目录 | 内存盘与虚拟盘 | 存量（旧 16 §1.3） |
| K-268、K-428 | `BlockDevice` 接口与两实现；块读写接缝 | 数据结构/接口与协议 | `diskimg/src/device.rs:12/24/60` | 统一块寻址契约 | 存量（旧 16 §3.1、§4.2）+ 新增 K-428 |
| K-273 | 介质三件与生成端；模块与测试 | 测试性质 | `diskimg/src/iso.rs:93`、`device.rs:84` | 账本条目 | 存量（旧 16 §1.2、§2.3、§4-§5，改指向 99） |

- **验收标准**：读者读完能回答——(1) `dd if=a of=b bs=1M count=2 skip=1` 一共读写了多少字节、跳过的是什么单位；(2) 怎么判断一个文件是不是光盘镜像、依据的字节在哪；(3) 不刻盘直接把镜像挂上要经过哪两步；(4) `conv=block` 在这个项目里支持吗。检查方式：本篇必须给出 ISO9660 主卷描述符的字段表（含偏移与长度）与 `dd` 操作数全表；`conv` 五个未实现的转换必须逐条标出。

### 23-备份与维护

- **一句话定位**：数据存好之后的三类担心：丢失、堆积、介质。
- **讲什么**：三类担心的问题地图与"只讲判断语言"的边界；备份的增量决策像 make（深讲）；`backup` 的十个选项字母；常量与错误分级；`remsync` 的三张记录表与硬链接跟踪（深讲）；`synctree` 的固定块传输与三指针目录记录；`cleantmp` 的午夜对齐保留窗口（深讲）；进度条的版式与"总数为零画空条"；`mt` 的磁带动作语言与计数三档（深讲）；状态查询作为唯一"解释执行"命令；`updateboot`/`update_asr`/`update_bootcfg` 的引导刷新流程（新增节）；族内差异表覆盖 13 条命令。
- **不讲什么**：文件系统遍历执行与时钟读取（执行层，见 §9.3 的 OQ-1）；磁带驱动的 ioctl（交给 `16-stage-drivers`）；屏幕重绘的转义序列（交给 `19`）；`rotate` 与 `fix`（分别交给 `18 §2` 与 `11 §2`）；`remsync` 的远端协议实现（交给 `17-stage-net` 或库层）。
- **前置**：`20`、`21`、`09`（遍历对象）。
- **后置**：`27`（引导刷新与 `installboot` 的交叉引用）。
- **事实底线**：
  - C：`minix/commands/backup/backup.c:3-11`、`:14-25`、`:52-58`；`remsync/remsync.c:101`、`:177`、`:285`、`:285-290`；`synctree/synctree.c:54-55`、`:200`；`cleantmp/cleantmp.c:31-32`、`:56`；`progressbar/progressbar.c:11`、`:34`；`mt/mt.c:40`、`:42`、`:78`；`minix/commands/{updateboot,update_asr,update_bootcfg}`。
  - Rust：`os/commands/sbin/maint/src/{lib,backup,cleantmp,progress,rotate,tape}.rs`（`rotate.rs` 的语义归 `18`，账本里标明）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-274 | 三类担心的问题地图 | 概念 | 旧 17 §1.0 | 本篇的总图 | 存量（旧 17 §1.0） |
| K-275、K-276、K-277 | 增量决策像 make（代表 1）；十个选项；常量与错误分级 | 机制/接口与协议/工具与工程 | `backup.c:3-11/14-25/52-58` | 备份的判定语言 | 存量（旧 17 §1.1、§2.1、§3.1） |
| K-278、K-279、K-280 | `remsync` 三张表（代表 2）；硬链接跟踪；`synctree` 分块 | 数据结构/机制 | `remsync.c:101/177/285`；`synctree.c:54-55/200` | 同步的数据模型 | 存量（旧 17 §2.2、§1.2） |
| K-281 | `cleantmp` 午夜对齐保留窗口（代表 3） | 机制 | `cleantmp.c:31-32/56` | 可心算的清理结果 | 存量（旧 17 §1.3、§2.3、§3.2） |
| K-282 | 进度条版式 | 机制 | `progressbar.c:11/34` | 健壮的进度显示 | 存量（旧 17 §1.4、§2.4、§3.3） |
| K-283、K-284、K-285 | `mt` 磁带动作语言（代表 4）；计数三档；状态查询的解释执行 | 接口与协议/约束与不变量/架构演进 | `mt.c:40/42/78`；`maint/src/tape.rs:129` | 顺序介质的语言 | 存量（旧 17 §1.4、§2.5、§3.4） |
| K-432 | `updateboot`/`update_asr`/`update_bootcfg` 的引导刷新 | 机制 | 各命令目录 | 被点名却无节的命令 | 新增 |
| K-289 | 测试与"存储四篇"口径统一 | 测试性质 | `maint/src/cleantmp.rs:100` | 账本条目 + 口径修正 | 存量（旧 17 §5、§1.0，改指向 99） |

- **验收标准**：读者读完能回答——(1) 第二次跑 `backup` 时哪些文件会被重新复制、依据是什么、这个依据有什么威胁模型上的假设；(2) `cleantmp` 在周三凌晨跑和周三中午跑，清理结果一样吗；(3) `mt` 的 `count` 参数哪些命令必须给、哪些必须不给；(4) 四个引导刷新脚本各改什么文件、什么时候用。检查方式：本篇必须给出 `backup` 的决策表（目标存在与否 × 源新旧）、`cleantmp` 的两条过期线与钳位关系、`mt` 的十六命令词表。

### 24-网络配置与诊断

- **一句话定位**：连通之前要回答的四个问题与两件诊断工具。
- **讲什么**：四问框架（接口、路由、邻居、名字库）与两件诊断；接口配置语义与读改写两调用顺序；路由与邻居共用路由套接字（深讲）；回显探测的配对语义（深讲）；反码校验和的完整算法；路径追踪的 TTL 逐跳机制（深讲）；名字数据库三文件与首次命中获胜、坏地址响亮报错；诊断与配置命令面（`netstat` 的用法行、`netconf.sh` 的顺序配置）；Rust 侧的四条设计决策（解析与调用分离、定容路由表、校验按字节切片、首次命中写进类型）；**ioctl 面缺 wrapper 的现状**（新增）；族内差异表覆盖 10 条命令。
- **不讲什么**：协议栈实现与套接字内部（交给 `17-stage-net`）；驱动收发（交给 `16-stage-drivers`）；无线与串行链路的硬件面（交给驱动）；`slip`/`swifi` 的硬件细节（只到命令词，实现归驱动）；名字库文件的形状（交给 `07 §2.4`）；`tcpdump`（不在本 stage 的命令面，若需要另立项）。
- **前置**：`19`（诊断输出的显示面）、`17-stage-net`（套接字接口）、`03`（约定）。
- **后置**：`25`（先连通，再经营服务）。
- **事实底线**：
  - C：`sbin/ifconfig/ifconfig.c:1050`、`:1059`、`:1173`；`sbin/route/route.c:1196`（`RTM_VERSION`；旧 18 头部把这一行错标到 `arp.c`）；`usr.sbin/arp/arp.c:315`、`:350`、`:434`、`:690`；`sbin/ping/ping.c:897`、`:909`、`:1029`、`:1266`（`in_cksum`）；`usr.sbin/traceroute/traceroute.c:472`、`:721`、`:1350`；`usr.bin/netstat/main.c:866`（旧 18 头部写的 `netstat.c` 不存在）；`minix/commands/netconf/netconf.sh`、`minix/commands/{slip,swifi}`；`usr.sbin/{ndp,rdate,rtadvd,traceroute6}`、`sbin/ping6`。
  - 非 C 制品：`etc/hosts`、`etc/services`、`etc/protocols`。
  - Rust：`os/commands/usr-sbin/netconfig/src/{lib,iface,route,arp,ping,hosts}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-290 | 四问框架与两件诊断 | 概念 | 旧 18 §1.0 | 本篇的总图 | 存量（旧 18 §1.0） |
| K-291 | 接口配置语义 | 概念 | `ifconfig.c:1050/1059/1173` | 接口面 | 存量（旧 18 §1.1、§2.1） |
| K-292 | 路由与邻居共用路由套接字（代表 1） | 概念 | `route.c:1196`；`arp.c:315/350/434/690` | 路由套接字 | 存量（旧 18 §1.2、§2.2） |
| K-293、K-294 | 回显探测配对（代表 2）；反码校验和 | 接口与协议/机制 | `ping.c:897/909/1029/1266` | 探测语义 | 存量（旧 18 §1.3、§2.3） |
| K-295 | 路径追踪的 TTL 逐跳（代表 3） | 机制 | `traceroute.c:472/721/1350` | 定位原理 | 存量（旧 18 §1.3、§2.4） |
| K-296 | 名字数据库三文件与首次命中获胜 | 概念 | `etc/{hosts,services,protocols}` | 名字到地址的翻译 | 存量（旧 18 §1.4，从旧 04 收拢） |
| K-297 | `netstat` 与 `netconf.sh` | 工具与工程 | `netstat/main.c:866`；`netconf.sh` | 诊断与装机脚本 | 存量（旧 18 §2.4） |
| K-298、K-299、K-300、K-301、K-302 | 五条 Rust 设计决策与模块 | 架构演进 | `netconfig/src/*.rs` | Rust 侧设计决策 | 存量（旧 18 §3、§4.1） |
| — | ioctl 面缺 wrapper 的现状与登记位置 | 接口与协议 | `14-stage-runtime/todo.md:60` | 命令与 ioctl 的接缝 | 新增 |
| K-303 | 测试与留白 | 测试性质 | `cargo test -p minix-netconfig` | 账本条目 | 存量（旧 18 §5，改指向 99） |

- **验收标准**：读者读完能回答——(1) 给网卡配地址要经过哪两个调用、顺序能不能反；(2) `ping` 怎么判断回来的是自己那一个请求的应答；(3) `traceroute` 为什么能定位到某一跳、星号代表什么；(4) 一个主机名解析失败时命令的行为是什么、坏地址与查无有什么区别；(5) 命令层与 ioctl 的接缝现在缺什么。检查方式：本篇必须给出 `in_cksum` 的逐步算法与一个手算例子；`netstat`/`netconf.sh` 必须以完整小节出现（不再是"只在 C 侧点名"）。

### 25-网络服务与守护

- **一句话定位**：连通之后的三件事：超级服务器、日志、跑腿客户端。
- **讲什么**：三件事框架；`inetd` 的服务表七列、等待模式、内置服务与两个上限（深讲）；`syslogd` 的优先级模型与八种动作（深讲）；`fetch` 的定位符语法（深讲）；`ftpd` 的会话准入流程（深讲）；跑腿客户端家族职责；**四个守护（`telnetd`/`zmodem`/`fingerd`/`httpd`）各自的职责与源位置**（新增节）；**邮件与打印的命令侧语义**（新增节）；Rust 侧四条设计决策（借用切片、枚举派生排序、认证失败报查无、日志池只计数）；族内差异表覆盖 20 条命令。
- **不讲什么**：协议状态机实现（交给 `17-stage-net`）；进程创建与权限切换（执行层，见 §9.3 的 OQ-1）；邮件投递与打印假脱机的后端执行（本篇只到队列语义）；`logger` 客户端（交给 `18 §1`）；`rdate`/`rtadvd`（归 `24`）。
- **前置**：`24`（先连通，再经营）、`18`（`logger` 与本篇的 `syslogd` 呼应）。
- **后置**：`27`（服务就绪之后的软件供给）。
- **事实底线**：
  - C：`usr.sbin/inetd/inetd.c:276`、`:306`、`:340`、`:344`、`:405`、`:409`；`usr.sbin/syslogd/syslogd.c:60`、`:130-137`、`:1471`、`:1488`、`:1528`；`minix/commands/fetch/fetch.c:858-859`；`libexec/ftpd/{ftpd.c,cmds.c,conf.c,logutmp.c,logwtmp.c}`；`libexec/{telnetd,fingerd,httpd,rshd}`；`minix/commands/{zmodem,lp,lpd,mail}`；`usr.bin/{telnet,ftp,rsh,whois,mail}`、`bin/{rcmd,rcp}`。
  - 非 C 制品：`etc/inetd.conf`、`etc/syslog.conf`、`etc/inet.conf`、`etc/named.conf`、`etc/namedb/`。
  - Rust：`os/commands/usr-sbin/netservices/src/{lib,inetd,syslog,fetch,session}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-304 | 三件事框架 | 概念 | 旧 19 §1.0 | 本篇的总图 | 存量（旧 19 §1.0） |
| K-305 | `inetd` 服务表与等待模式（代表 1） | 概念 | `inetd.c:276/306/340/344/405/409` | 按需起服务的机制 | 存量（旧 19 §1.1、§2.1） |
| K-306、K-307 | `syslogd` 优先级模型（代表 2）；选择符与八种动作 | 概念/接口与协议 | `syslogd.c:60/130-137/1471/1488/1528` | 日志的归类与分拣 | 存量（旧 19 §1.2、§2.2） |
| K-308 | `fetch` 定位符语法（代表 3） | 接口与协议 | `fetch.c:858-859` | 取文件命令的两种写法 | 存量（旧 19 §1.3、§2.3） |
| K-309 | `ftpd` 会话准入流程（代表 4） | 机制 | `libexec/ftpd/*.c` | 登录守护的准入语义 | 存量（旧 19 §1.3、§2.3） |
| K-310、K-433 | 跑腿客户端家族；四个守护的职责与源位置 | 工具与工程 | 各命令目录 | 服务面完整性 | 存量（旧 19 §2.3）+ 新增 K-433 |
| K-311、K-434 | 队列语义；邮件与打印的命令侧 | 概念 | `minix/commands/{mail,lp,lpd}` | 覆盖边界的补全 | 存量（旧 19 §1.3）+ 新增 K-434 |
| K-312、K-313、K-314、K-315、K-316 | 五条 Rust 设计决策与模块 | 架构演进 | `netservices/src/*.rs` | Rust 侧设计决策 | 存量（旧 19 §3、§4.1） |
| K-317 | 测试与留白 | 测试性质 | `cargo test -p minix-netservices` | 账本条目 | 存量（旧 19 §5，改指向 99） |

- **验收标准**：读者读完能回答——(1) 一条 `inetd` 记录有几列、`stream` 与 `dgram` 的处理方式差在哪；(2) 一条日志消息如何被决定写到哪里；(3) `fetch` 的 `host:port` 老写法与 URL 写法各在什么情况下用；(4) `ftpd` 认证失败为什么报"查无"而不是"口令错"；(5) 这个 stage 覆盖了哪些守护、各自覆盖到什么程度。检查方式：本篇必须给出 `inetd.conf` 七列字段表与 `syslog.conf` 选择符的文法；每个守护必须有一行"覆盖到哪一层"的状态说明。

### 26-Minix 特有工具与系统信息

- **一句话定位**：观察系统自己：身份、时钟、参数、构成。
- **讲什么**：四种目光框架（身份、时间、参数、构成）；`version` 与 `printroot` 的根设备发现（深讲）；`readclock` 的双钟同步与选项面（深讲）；`intr` 的限时执行（深讲）；`sysctl` 的参数树与名字检查、根按类型分流（深讲）；参数内存表的定容与幂等约束；`ldd` 的静态链接语义变化；**十四个被点名命令各自的职责与锚点**（`profile`/`sprofalyze`/`zic`/`zdump`/`i2cscan`/`lspci`/`eepromread`/`trace`/`dhrystone`/`worldstone`/`srccrc`/`playwave`/`recwave` 等，新增节）；族内差异表覆盖 20 条命令。
- **不讲什么**：驱动内部实现（交给 `16-stage-drivers`）；内核参数的生效逻辑（交给 `01-stage-kernel` 与 `10-stage-mib`）；音频设备的采集与播放（交给音频驱动，`[ARCH] A-13` 为 defer 候选）；时区数据库内容（数据面，`zic`/`zdump` 只讲职责）。
- **前置**：`00`、`03`（约定）。
- **后置**：`27`（软件供给篇与 `sysctl` 的参数面交叉引用）。
- **事实底线**：
  - C：`minix/commands/version/version.sh`；`printroot/printroot.c:25`、`:27`、`:45`、`:55`；`readclock/readclock.c:55`、`:59`、`:73`、`:122`、`:164`；`intr/intr.c:19`、`:51`、`:140`；`sbin/sysctl/sysctl.c:252`、`:520`；`usr.bin/ldd/ldd.c:94-96`；`minix/commands/{profile,sprofalyze,sprofdiff,dhrystone,worldstone,srccrc,lspci,playwave,recwave}`；`usr.sbin/{zic,zdump,i2cscan}`；`minix/usr.bin/{eepromread,trace}`。
  - 非 C 制品：`etc/system.conf`。
  - Rust：`os/commands/bin/sysinfo/src/{lib,clock,intr,rootdev,sysctl,ldd}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-319 | 四种目光框架 | 概念 | 旧 20 §1.0 | 本篇的总图 | 存量（旧 20 §1.0） |
| K-320、K-321 | `version`；`printroot` 根设备发现（代表 1） | 工具与工程/机制 | `version.sh`；`printroot.c:25/27/45/55` | 身份面 | 存量（旧 20 §1.1、§2.1） |
| K-322 | `readclock` 双钟同步（代表 2） | 机制 | `readclock.c:55/59/73/122/164` | 时间面 | 存量（旧 20 §1.1、§2.2） |
| K-323 | `intr` 限时执行（代表 3） | 机制 | `intr.c:19/51/140` | 控制面 | 存量（旧 20 §1.2、§2.3） |
| K-324、K-325 | `sysctl` 参数树（代表 4）；参数表定容与幂等 | 概念/约束与不变量 | `sysctl.c:252/520` | 参数面 | 存量（旧 20 §1.3、§2.4、§3.3） |
| K-326 | `ldd` 的静态链接语义变化 | 架构演进 | `ldd.c:94-96` | 构成面 | 存量（旧 20 §1.4、§2.4） |
| K-327、K-435 | 其余系统工具与十四条点名命令 | 工具与工程 | 各命令目录 | 覆盖完整性 | 存量（旧 20 §2.4）+ 新增 K-435 |
| K-328、K-329、K-330、K-331 | 四条 Rust 设计决策 | 架构演进 | 旧 20 §3.1-3.5 | Rust 侧设计决策 | 存量（旧 20 §3） |
| K-332 | 模块、类型与测试 | 测试性质 | `sysinfo/src/*.rs` | 账本条目 | 存量（旧 20 §4-§5，改指向 99） |

- **验收标准**：读者读完能回答——(1) 我怎么知道当前跑的内核是什么版本、根设备是哪个盘；(2) 把硬件钟写进系统钟和反过来，命令分别怎么写、为什么默认方向是安全的；(3) 给一条命令套 30 秒时限，超时后会发生什么；(4) `sysctl kern.foo` 的名字校验规则有哪些；(5) 静态链接之后 `ldd` 打出来的是什么。检查方式：本篇必须给出 `sysctl` 的点分名与 `CTLTYPE_NODE` 分流说明、`readclock` 的选项表，以及 `ldd` 静态语义的一反一正两个例子。

### 27-软件供给与构建辅助

- **一句话定位**：装好系统之后，软件从哪里来。
- **讲什么**：供给四事框架（包管理、引导块写入、构建依赖、汇编符号抽取）；三组包与确认跳过、包名字符规则；光盘仓库的路径模板与查找顺序；`installboot` 的三种阶段文件分发与空间约束（深讲，回链 `04` 的引导面）；`mkdep` 的依赖行形状；`genassym` 的符号抽取与两模式；`mk.conf` 的五行；`gcov-pull`、`nbperf` 的职责；**`pkgin_all`/`postinstall` 的补节**（新增）；四条 Rust 设计决策；族内差异表覆盖 10 条命令。
- **不讲什么**：宿主工具链（编译器与链接器的宿主面，构建链之外）；包传输执行与磁盘写入（执行层，见 §9.3 的 OQ-1）；完美散列算法实现（本篇只到方法选择）；引导块在盘上的物理布局（交给 `21 §2` 与 `04`）。
- **前置**：`05`（服务管理面先行）、`21`（分区与引导块布局）。
- **后置**：`99` 账本（软件供给的完成度进账本）。
- **事实底线**：
  - C：`minix/commands/pkgin_sets/pkgin_sets.sh:3-5`；`pkgin_cd/pkgin_cd.sh:7-8`；`usr.sbin/installboot/installboot.c:61`、`:246`、`:272-276`；`usr.bin/mkdep/mkdep.c:80`；`usr.bin/genassym/genassym.sh:42`；`minix/commands/gcov-pull/gcov-pull.c:16`；`usr.bin/nbperf/nbperf.c:139-144`；`minix/commands/pkgin_all/`、`usr.sbin/postinstall/`、`usr.bin/make/`。
  - 非 C 制品：`etc/mk.conf`（5 行）。
  - Rust：`os/commands/usr-sbin/pkgtools/src/{lib,sets,cdrepo,bootinst,depfile,symgen,mkconf}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-333 | 供给四事框架 | 概念 | 旧 21 §1.0 | 本篇的总图 | 存量（旧 21 §1.0） |
| K-334、K-335 | 三组包；光盘仓库 | 概念/接口与协议 | `pkgin_sets.sh:3-5`；`pkgin_cd.sh:7-8` | 包管理面 | 存量（旧 21 §1.1-1.2、§2.1-2.2） |
| K-336 | `installboot` 分发与空间约束（代表 1） | 机制 | `installboot.c:61/246/272-276` | 引导块安装 | 存量（旧 21 §1.3、§2.3） |
| K-337、K-338、K-339 | `mkdep`；`genassym`；`mk.conf` | 接口与协议 | `mkdep.c:80`；`genassym.sh:42`；`etc/mk.conf` | 构建辅助面 | 存量（旧 21 §1.4、§2.5-2.6） |
| K-340、K-436 | `gcov-pull`/`nbperf`；`pkgin_all`/`postinstall` | 工具与工程 | `gcov-pull.c:16`；`nbperf.c:139-144` | 点名命令的完整覆盖 | 存量 + 新增 K-436 |
| K-341、K-342、K-343、K-344、K-345 | 五条 Rust 设计决策 | 架构演进 | 旧 21 §3 | Rust 侧设计决策 | 存量（旧 21 §3） |
| K-346 | 模块、类型与测试 | 测试性质 | `pkgtools/src/*.rs` | 账本条目 | 存量（旧 21 §4-§5，改指向 99） |

- **验收标准**：读者读完能回答——(1) 在一块新盘上装引导块要满足什么空间条件、不满足时命令的行为是什么；(2) 离线装机时找包的顺序是怎样的、为什么先查根；(3) `.depend` 文件的一行长什么样、谁生成的；(4) 汇编文件里的符号值是怎么从编译器中抽出来的。检查方式：本篇必须给出 `installboot` 的三种文件系统类型分发表与 `mk.conf` 的五种行形态，并逐条说明 `postinstall`/`pkgin_all` 的职责。

### 28-stdio 游戏

- **一句话定位**：只用说话证明系统活着。
- **讲什么**：验收层框架（只依赖启动、标准输入输出、退出）；数论两件（`factor` 的试除与输出形状、`primes` 的区间筛与加速，深讲）；密码与电码（`caesar` 的轮转表与溢出保护、`morse` 的码表与整串匹配）；文字游戏（`pig` 的猪拉丁语三规则、`number` 的数词分段）；大字与测验（`banner` 的列映射缩放，深讲；`arithmetic` 的算术语义与计数）；打孔显示数据模型（`bcd` 的孔位表与卡框、`ppt` 的纸带几何，深讲）；四条 Rust 设计决策；族内差异表与最小 API 表覆盖全部 10 条命令。
- **不讲什么**：终端控制（交给 `29`）；键盘读取与逐行显示的执行（各游戏二进制的执行面）；大数概率分解（后续补齐）；字体位图数据（数据面）。
- **前置**：`10`（输入输出与真值工具）、`03 §3`（最小 API 面）。
- **后置**：`29`、`30`（另两类游戏以本篇的"验收层"口径与最小 API 表为参照）。
- **事实底线**：
  - C：`games/factor/factor.c:110`、`:184`、`:197`、`:268`；`games/primes/{primes.c,pattern.c,pr_tbl.c,spsp.c}`；`games/caesar/caesar.c:82`、`:86`、`:125`；`games/morse/morse.c:97`、`:140`、`:219`；`games/pig/pig.c:103`、`:133`；`games/number/number.c`；`games/banner/banner.c:58`、`:1052`、`:1057`、`:1064-1066`；`games/arithmetic/arithmetic.c:99`、`:100`、`:107`、`:131`；`games/bcd/bcd.c:87`、`:154`；`games/ppt/ppt.c:52`、`:130`。
  - Rust：`os/commands/games/stdio-games/src/{lib,bcd,ppt,caesar,factor,morse,primes,quiz,words,bin_support}.rs` 与 `src/bin/`（9 个）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-347 | 验收层框架 | 概念 | 旧 22 §1.0；`99 §1` | 本篇的总图 | 存量（旧 22 §1.0） |
| K-348、K-349 | `factor`（代表 1）；`primes`（代表 2） | 机制 | `factor.c:184/197/268`；`games/primes/*` | 数论两件 | 存量（旧 22 §1.1、§2.1-2.2） |
| K-350、K-351 | `caesar`；`morse` | 机制/接口与协议 | `caesar.c:82/86/125`；`morse.c:97/140/219` | 密码与电码 | 存量（旧 22 §1.2、§2.3-2.4） |
| K-352、K-353 | `pig`；`number` | 机制 | `pig.c:103/133`；`number.c` | 文字游戏 | 存量（旧 22 §1.3、§2.5） |
| K-354、K-355 | `banner`（代表 3）；`arithmetic` | 机制 | `banner.c:58/1052/1057/1064-1066`；`arithmetic.c:99/100/107/131` | 大字与测验 | 存量（旧 22 §1.4、§2.5-2.6） |
| K-356 | 打孔显示数据模型（代表 4） | 数据结构 | `bcd.c:87/154`；`ppt.c:52/130` | 打孔卡与纸带的几何 | 存量（旧 22 §4.1-4.2） |
| — | `bcd`/`ppt` 的玩法分组补位 | 导航 | 同上 | 概念叙事与命令清单一致 | 新增 |
| K-357、K-358 | 试除加速；码表接口 | 架构演进 | 旧 22 §3.1-3.2 | Rust 侧决策 | 存量（旧 22 §3.1-3.2） |
| K-359、K-360 | 契约与 `Requires`；测试与留白 | 测试性质 | `stdio-games/src/bin/`；`cargo test -p minix-stdio-games` | 账本条目 | 存量（旧 22 §4.4、§5，改指向 99） |

- **验收标准**：读者读完能回答——(1) 为什么游戏篇是最早可验收的一批、它们只需要哪几个 API；(2) `factor` 对一个 2^32 附近的数会走哪条路径；(3) `banner -w 40` 的字形是怎么被压窄的；(4) `bcd` 打印的卡框有几行几列、孔位怎么编码。检查方式：本篇必须给出 10 条命令的最小 API 表（列：命令、需要的 API、当前状态）与至少两条可复现的输出示例。

### 29-终端游戏

- **一句话定位**：在格子上动起来。
- **讲什么**：格子三件事框架（屏幕多大、光标送到格、格中写字擦字）；屏幕模型（内存屏与空屏、越界报错、坐标从 1 起、定容尺寸、三类转义序列，深讲）；`tetris` 的形状表示与旋转公式（深讲）；吻合、放置、消行与计分的规则；棋盘尺寸差异的说明；`snake` 的步进与自撞语义（深讲）；`crawler`/`rain` 的跟进搬移；`dungeon` 的房间与走廊生成规则（深讲）；`colorbars` 的颜色条（新增节）；随机源的两个确定实现；"不移植 termcap/terminfo"的架构决策与落地；族内差异表覆盖 7 条命令。
- **不讲什么**：终端能力数据库（交给 `19`）；键盘读取与真实绘制（各游戏二进制的执行面）；怪物、物品与计分文件（后续补齐）；最高分文件的持久化（后续补齐）。
- **前置**：`19`（终端控制与能力序列）、`10`（输入输出）。
- **后置**：`30`（文本游戏以本篇的屏幕抽象为参照）。
- **事实底线**：
  - C：`games/worm/worm.c:120`、`:203-204`；`games/tetris/shapes.c:46-53`、`:56`、`:82`、`:97`；`games/tetris/tetris.c:62`、`:109-118`；`games/tetris/tetris.h:54-56`；`games/snake/`（含 `snscore/`）；`games/rain/rain.c:72-88`、`:117-118`；`games/rogue/rogue.h:54`、`:293`；`games/rogue/room.c`；`games/colorbars/`。
  - Rust：`os/commands/games/term-games/src/{lib,screen,tetris,snake,crawler,dungeon,random}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-361 | 格子三件事框架 | 概念 | 旧 23 §1.0 | 本篇的总图 | 存量（旧 23 §1.0） |
| K-362 | 屏幕模型（代表 1） | 数据结构 | `worm.c:120/203-204`；`term-games/src/screen.rs` | 绘制与测试共用的抽象 | 存量（旧 23 §1.1、§2.2） |
| K-363、K-364、K-365 | `tetris` 形状（代表 2）；吻合与计分；尺寸差异 | 数据结构/机制/约束与不变量 | `shapes.c:46-53/56/82/97`；`tetris.h:54-56` | 方块几何与棋盘操作 | 存量（旧 23 §1.2、§2.1） |
| K-366、K-367 | `snake`（代表 3）；`crawler`/`rain` | 机制 | `games/snake/`；`rain.c:72-88/117-118` | 爬行类三种走法 | 存量（旧 23 §1.3、§2.2） |
| K-368 | `dungeon` 房间与走廊（代表 4） | 机制 | `rogue.h:54/293`；`rogue/room.c` | 地牢几何 | 存量（旧 23 §1.4、§2.3） |
| K-437 | `colorbars` 颜色条 | 机制 | `games/colorbars/` | 全篇无落点的命令 | 新增 |
| K-369、K-370、K-371、K-372、K-373 | 屏幕接口隔离；旋转公式；自撞判定；随机确定源；不移植能力数据库 | 架构演进 | `term-games/src/*.rs` | Rust 侧设计决策 | 存量（旧 23 §3） |

- **验收标准**：读者读完能回答——(1) 屏幕抽象为什么要把"写到格子"与"发转义序列"分开；(2) 俄罗斯方块旋转用公式而不是查表，换来什么、代价是什么；(3) 蛇走到自己尾巴所在的格子算不算撞；(4) 随机数怎么保证测试可复现。检查方式：本篇必须给出屏幕抽象的接口清单与旋转公式的推导（含"转四次等于恒等"的验证），并给出 `colorbars` 的一行职责说明。

### 30-文本游戏

- **一句话定位**：留住人的三件事：听懂、讲规则、藏惊喜。
- **讲什么**：故事三件事框架；`adventure` 的词汇表与物品两表（深讲）；`monop` 的钱与骰子规则（深讲）；`fortune` 的索引表定位与抽签（深讲）；`acronym`/`wtf` 的词条查找约定；`fish` 的成书规则与要牌策略（深讲）；`random` 的概率过滤与种子；**`wargames` 的剧本与交互面**（新增节）；`[ARCH] A-10` 的"大文本数据进程序资源"决策与落地；族内差异表覆盖 7 条命令。
- **不讲什么**：大文本数据文件格式的落地（`99` 的 A-10 登记行）；数据文件读取与玩家交互（各游戏二进制的执行面）；纸牌发牌与剧本数据（后续补齐）。
- **前置**：`10`（输入输出）、`14`（文本面）。
- **后置**：`99`（数据资源决策进 ARCH 登记表）。
- **事实底线**：
  - C：`games/adventure/hdr.h:78`、`:101`、`:116`；`games/adventure/vocab.c:72`；`games/adventure/glorkz`（剧本数据）；`games/monop/{monop.c,cards.c,houses.c,jail.c}`；`games/fortune/fortune/fortune.c:267`、`:980-981`；`games/fortune/datfiles/`；`games/wtf/wtf`（词条文件）；`games/fish/fish.c:61`、`:64`、`:83`、`:160`；`games/random/random.c:107`、`:126`；`games/wargames/wargames.sh`。
  - Rust：`os/commands/games/text-games/src/{lib,adventure,monop,fortune,fish,acronym,lottery}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-374 | 故事三件事框架 | 概念 | 旧 24 §1.0 | 本篇的总图 | 存量（旧 24 §1.0） |
| K-375、K-376 | `adventure` 词汇表与物品两表（代表 1） | 数据结构/机制 | `adventure/hdr.h:78/101/116`；`vocab.c:72` | 探险游戏的理解层 | 存量（旧 24 §1.1、§2.1） |
| K-377 | `monop` 钱与骰子规则（代表 2） | 机制 | `games/monop/*.c` | 棋盘走子与资金规则 | 存量（旧 24 §1.2） |
| K-378 | `fortune` 索引表定位与抽签（代表 3） | 机制 | `fortune.c:267/980-981` | 格言库的磁盘形状 | 存量（旧 24 §1.3、§2.3） |
| K-379 | `acronym`/`wtf` 词条查找 | 接口与协议 | `games/wtf/wtf` | 缩写查询约定 | 存量（旧 24 §1.3、§2.4） |
| K-380 | `fish` 成书规则与策略（代表 4） | 机制 | `fish.c:61/64/83/160` | 纸牌玩法 | 存量（旧 24 §1.4、§2.2） |
| K-381、K-438 | `random` 概率过滤；`wargames` 剧本与交互 | 机制/概念 | `random.c:107/126`；`wargames.sh` | 概率工具与剧本游戏 | 存量 + 新增 K-438 |
| K-382、K-383、K-384、K-385、K-386 | 五条 Rust 设计决策与数据资源决策 | 架构演进 | 旧 24 §3；`games/{adventure/glorkz,fortune/datfiles}` | Rust 侧决策与数据面 | 存量（旧 24 §3） |
| K-387 | 模块与测试 | 测试性质 | `text-games/src/*.rs` | 账本条目 | 存量（旧 24 §4-§5，改指向 99） |

- **验收标准**：读者读完能回答——(1) 探险游戏里"拿钥匙"这句话是怎么被理解的、词汇表有多大；(2) `monop` 里资金为负会发生什么、为什么这样定；(3) `fortune` 怎么从一个大文件里随机取一条、索引存了什么；(4) 游戏剧本这类大文本数据在这个项目里放在哪、为什么不进代码。检查方式：本篇必须给出 `adventure` 的词汇表三分类与物品两表的编号规则，以及 `fortune` 索引格式的字段说明。

### 99-实现账本与全局索引

- **一句话定位**：命令总表、篇章与 crate 的映射、`Requires` 总表、ARCH 登记、测试与守卫、排除表——读完全部之后来查的一篇。
- **讲什么**：命令总表（326 条命令的程序名、C 源目录与文件、归属篇章、Rust crate、接线状态、测试数），含 DESCRIBE 与排除项 `devmand` 的说明；篇章与 crate 与 C 源目录的三方映射表；`Requires` 总表（每命令的最小 API 面与缺口登记位置）；ARCH 登记表（A-1 至 A-14 加新增项，每条含三处一致标注的位置：本篇、设计文档、代码注释）；测试与守卫（每 crate 的测试数与验收命令、边界守卫脚本、宿主与真机接缝、二进制面装配现状、C 侧 ATF 测试的对照说明）；边界与排除表（八项范围外归属）；未决问题与跨阶段挂账（指向 `todo.md` 与 `edge*.md` 的条目）。
- **不讲什么**：任何机制与概念的讲解（全部交给 `00`–`30`）；设计决策的论证过程（交给 `plan.md`）；开放问题的处置过程（交给 `todo.md`）；跨阶段条目的裁决过程（交给 `edge*.md`）。
- **前置**：全部（`00`–`30`）。本篇允许引用任何更早的编号。
- **后置**：无（末篇）。
- **事实底线**：
  - 命令总表的事实来源：`minix3/` 七个源目录的 `ls -d` 全量清单（与 `plan.md §5.2` 逐目录 diff 的结果：0 遗漏、0 幽灵、0 跨篇重复）；`minix3/etc/` 的 49 项分配。
  - Rust 侧：`os/commands/` 24 个 crate 的包名、源文件数、行数、`#[test]` 数、`src/bin/` 内容；`os/Cargo.toml` 成员表；`tools/check-command-boundary.sh`。
  - ARCH 项：`plan.md §4` 的 A-1 至 A-14，逐项确认状态（已定/设计期/重大决策/defer 候选）与三处标注位置。
  - 开放的登记口：`todo.md`（P0-1、P1-1、P1-2、C-1 至 C-7、E-THREAD-MODEL）、`edge_todo.md`（E-CMDSYSFACE、E-SYSCALL-SIGN、E-FSCMDS）、`edge2.md` L10、`edge3.md` S35/S36/S40、`edge4.md §6`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 归本篇的理由 | 来源 |
|---|---|---|---|---|---|
| K-395、K-396、K-400（DESCRIBE 部分） | `Requires` 示范与缺口；参见网络；构建面汇总 | 接口与协议/导航 | `99 §3`；`plan.md` 各表 | 账本条目 | 存量（旧 99 §3、参见、核心点） |
| K-394（实例部分） | 契约表在各篇的实例位置索引 | 接口与协议 | §5 各篇的族内差异表 | 账本指向各篇 | 存量（旧 99 §3 模板的下游） |
| K-397、K-398、K-399 | A-1/A-4/A-5 的裁决记录 | 架构演进 | `plan.md §4` | ARCH 登记表 | 存量（旧 99 核心点） |
| K-415（登记部分）、K-229、K-170、K-386、K-155 | 全部 ARCH 项的三处标注位置与状态 | 架构演进 | `plan.md §4` 全表 | 唯一裁决记录 | 存量（散落各处收拢） |
| K-416、K-441、K-109、K-130、K-149、K-164、K-185、K-200、K-216、K-231、K-245、K-259、K-273、K-289、K-303、K-317、K-332、K-346、K-360、K-387、K-046、K-062、K-077、K-091、K-110、K-132（测试与契约部分） | 每 crate 的测试数、验收命令、契约表实例位置 | 测试性质 | `os/commands/*/src/`；`cargo test -p minix-*` | 全部账本条目集中一处 | 存量（24 篇的 §4-§5 合并） |
| K-015（口径部分） | 统计口径与两处例外 | 工具与工程 | §0.3 | 账本表的口径定义 | 新增 |
| K-105、K-174、K-191、K-211 | 命令契约总表的三个分族实例（35 行 / 28 命令 / 10 命令）与职责级覆盖 | 接口与协议 | §2.5 表；各篇族内差异表 | 表格本体留在各命令族篇，账本只做行数对账与总索引 | 存量（旧 06/07/10/12 的表） |

- **验收标准**：读者（尤其是补实现的执行者）读完能回答——(1) 某条命令的 C 源在哪、归哪一篇、对应的 crate 是什么、现在接线到哪一步、测试有多少；(2) 我要实现 `ls`，需要先补哪几个 API、缺口登记在哪一条；(3) 某个 ARCH 决策的现状与三处标注位置在哪；(4) 命令面还有哪些没做、为什么没做。检查方式：本篇的命令总表必须能通过一条机械核对——把表里的命令名集合与 `minix3/` 七个目录的 `ls -d` 输出做 `comm -3`，结果为空；测试数列必须能用 `cargo test -p <crate>` 逐条复现。

---

## 6. 变更表

### 6.1 重排（20 处）

整篇换位置，内容基本保留，边界按新契约收窄。

| 操作编号 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 |
|---|---|---|---|---|---|
| OP-R01 | 旧 00 | 新 00 | 从 21 行骨架扩写为完整总览 | K-001…K-016 | 全篇 |
| OP-R02 | 旧 01 | 新 04 | 前面插入三篇框架；本篇补 `rc.minix` 等真实执行面 | K-017…K-032、K-422、K-423 | 新 04 |
| OP-R03 | 旧 02 | 新 05 | 同上位移；并入 `shlock` | K-033…K-046、K-439 | 新 05 |
| OP-R04 | 旧 03 | 新 06 | 同上位移；`utmp` 主讲述点移出 | K-047…K-062 | 新 06（K-207/424 移 18） |
| OP-R05 | 旧 04 | 新 07 | 同上位移；三处越界内容移出 | K-063…K-077 | 新 07（K-053/054 归 06、K-424 归 18、K-296 归 24） |
| OP-R06 | 旧 05 | 新 08 | 同上位移；`§1.5` 从越界改为合法（执行环境面） | K-078…K-094、K-111 | 新 08 |
| OP-R07 | 旧 06（文件操作部分） | 新 09 | 拆出的前半，补 `cat`/`mv`/`rm` 家族 | K-095…K-101、K-104、K-106、K-108…K-110、K-112、K-113 | 新 09 |
| OP-R08 | 旧 07 | 新 11 | 位移 + 家族重划（撤 `pr`/`tsort`，并入 `fix`） | K-117…K-135 | 新 11（K-179 归 15、`pr` 归 14、K-287 入） |
| OP-R09 | 旧 08 | 新 12 | 位移 + 并入 `expr` | K-136…K-149、K-116、K-150 | 新 12 |
| OP-R10 | 旧 09 | 新 13 | 位移，内容不动 | K-151…K-164 | 新 13 |
| OP-R11 | 旧 10（排版手册部分） | 新 14 | 拆出的前半 | K-165…K-169、K-171…K-178、K-182…K-185 | 新 14 |
| OP-R12 | 旧 11 | 新 16 | 位移，内容不动 | K-186…K-200 | 新 16 |
| OP-R13 | 旧 12（进程信号部分） | 新 17 | 拆出的前半 | K-201…K-206、K-210…K-217 | 新 17 |
| OP-R14 | 旧 13 | 新 19 | 位移 + 撤销幽灵命令 `termcap` | K-219…K-231 | 新 19 |
| OP-R15 | 旧 14 | 新 20 | 位移 + 补 `umount` 节与调用接缝 | K-232…K-245、K-425、K-426、K-431 | 新 20 |
| OP-R16 | 旧 15 | 新 21 | 位移 + 并入 `dosread` | K-246…K-259、K-272、K-427 | 新 21 |
| OP-R17 | 旧 16 | 新 22 | 位移，`dosread` 移出 | K-260…K-271、K-273、K-428 | 新 22 |
| OP-R18 | 旧 17 | 新 23 | 位移 + `rotate`/`fix` 移出 + 补引导刷新 | K-274…K-285、K-289、K-432 | 新 23（K-286 归 18、K-287 归 11） |
| OP-R19 | 旧 18 | 新 24 | 位移，内容不动 | K-290…K-303 | 新 24 |
| OP-R20 | 旧 19 | 新 25 | 位移 + 补四个守护与邮件打印 | K-304…K-317、K-433、K-434 | 新 25 |
| OP-R21 | 旧 20 | 新 26 | 位移 + 补齐十四条规定点名命令 | K-319…K-332、K-435 | 新 26 |
| OP-R22 | 旧 21 | 新 27 | 位移 + 补 `pkgin_all`/`postinstall` | K-333…K-346、K-436 | 新 27 |
| OP-R23 | 旧 22 | 新 28 | 位移 + 补 `bcd`/`ppt` 玩法位 | K-347…K-360 | 新 28 |
| OP-R24 | 旧 23 | 新 29 | 位移 + 补 `colorbars` | K-361…K-373、K-437 | 新 29 |
| OP-R25 | 旧 24 | 新 30 | 位移 + 补 `wargames` | K-374…K-387、K-438 | 新 30 |

### 6.2 拆分（3 处）

拆分遵守"存量方向看去向"：每一处拆分的旧知识点逐条给出新位置，写不出位置的不许拆。

| 操作编号 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向（逐条） |
|---|---|---|---|---|---|
| OP-S01 | 旧 06 §1.4、§2.1、§2.5（判断求值）与 §1.3 的路径三条 | 新 10 | 两半回答不同问题：前半"如何操作文件对象"，后半"如何在脚本里算路径、判真假、出输出"；后者与 shell 同族属脚本工具箱 | K-101（路径部分）、K-102、K-103、K-106（求值侧）、K-107、K-108（求值侧）、K-114、K-115 | K-101 路径组→10 §2；K-102→10 §1；K-103→10 §2（深讲 1）；K-106 求值侧→10 §3；K-107→10 §3；K-108 求值侧→10 §3；K-114→10 §2（深讲 2）；K-115→10 §2（深讲 3）。保留在 09 的：K-095…K-101（空间与并发）、K-104、K-105、K-109、K-110、K-112、K-113。**K-116（`expr`）不留在 10，按序差-2 移 12** |
| OP-S02 | 旧 10 §1.1、§1.3、§2.4-2.5、§3.4 中的开发辅助与国际化部分 | 新 15 | "给人读的文档"与"给构建与工具链用的辅助"是两个问题；i18n 是显式推迟项，与排版无关 | K-168、K-170、K-178（开发辅助侧）、K-179、K-180、K-181 | K-168→15 §1；K-170→15 §1、§3；K-178 开发辅助侧→15 §3；K-179→15 §2（唯一归属）；K-180→15 §2；K-181→15 §1、§2。保留在 14 的：K-165…K-167、K-169、K-171…K-178（排版侧）、K-182（排版侧）、K-183、K-184、K-185 |
| OP-S03 | 旧 12 §1.3、§1.4 的消息与时间部分、§2.3、§4.2 | 新 18 | 两半的数据源不同：内核进程表 vs `utmp` 文件；读者问题也不同（"进程在干什么" vs "谁在用系统"） | K-201（会话侧）、K-207、K-208、K-209、K-210（消息侧）、K-216（会话侧）、K-286、K-424 | K-201 会话侧→18 §1；K-207→18 §2（主讲述点）；K-208→18 §3；K-209→18 §1、§2；K-210 消息侧→18 §1；K-216 会话侧→99 账本；K-286→18 §2；K-424→18 §2。保留在 17 的：K-201（进程侧）、K-202…K-206、K-210（进程侧）、K-212…K-215、K-217 |

### 6.3 合并（1 处）

| 操作编号 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 |
|---|---|---|---|---|---|
| OP-M01 | 旧 99 的 §1、§2、§3 三节（连同核心点里 A-1/A-4/A-5 的点名） | 新 03 | 这三节是全 stage 的判定规则与表格模板，被中段各篇引用；留在文末构成前向引用（硬标准一）。前移之后 99 的编号位改放账本 | K-388…K-400 | K-389…K-396→03 §1…§3；K-397…K-399→03 §4-§5 + 99 ARCH 登记行；K-400 拆为三份：安装面→02 §1、配置约定→03 §5、DESCRIBE/curses 汇总→99 |

### 6.4 新建（3 篇新篇章 + 1 篇账本）

| 操作编号 | 新位置 | 原料来源 | 理由 | 涉及知识点 | 来源 |
|---|---|---|---|---|---|
| OP-N01 | 新 01 一条命令的一生 | C：`lib/csu/*`、`lib/libc/stdlib/getopt.c`；Rust：`os/commands/*/src/bin/` 的薄壳；文档：旧 17 §3.5（参数解析政策）、旧 99 §1（依赖面） | 集合型 stage 的统一框架篇，现完全缺位（GAP-1） | K-401…K-408、K-406、K-407 | 新增 |
| OP-N02 | 新 02 命令的交付与安装面 | 非 C 制品：五个 `Makefile.inc`、两个 `Makefile` 的 `SUBDIR`、`include/paths.h`、`etc/man.conf`、`DESCRIBE.sh`、`distrib/`；C：`man.c`、`makewhatis.c`、`ldd.c:94-96`；Rust：`os/commands/` 与 `os/Cargo.toml` | 集合型 stage 的注册与发现机制，现只有核心点里的一行（GAP-2） | K-409…K-416、K-440 | 新增 |
| OP-N03 | 新 03 跨篇约定 | 旧 99 §1/§2/§3；`plan.md §4` 的 A-4/A-5；`tools/check-command-boundary.sh`；`todo.md §2`、§4；`edge4 §6` | 判定规则必须排在引用者之前；且需补 A-4/A-5、单线程模型、守卫机制四项（GAP-3） | K-388…K-400、K-417…K-421、K-288 | 存量前移 + 新增 |
| OP-N04 | 新 99 实现账本与全局索引 | `plan.md §5.1-§5.4`（覆盖契约与排除表）、`plan.md §2`（crate 映射）、各篇旧 §4.1/§4.5/§5（模块表、`Requires`、测试统计）、`plan.md §4`（ARCH 全表）、`todo.md` 与 `edge*.md` 的登记口 | 把 24 处会随实现变动的表格集中到一处，消除"改一处要同步 24 处"的维护面；也是 GAP-7 的登记处 | K-015、K-095 起各篇的账本条目、K-397…K-400、K-415、K-416、K-441 | 存量合并 + 新增 |

### 6.5 归档与删除

**归档（B 相执行，不删文件）。** 现有 26 篇编号文档全部退出正式目录，整体移入 `archive/`（或按项目惯例的归档位置），文件内的锚点全部按 §8 的迁移表重写后再归档一份索引。参考材料 `plan.md`、`todo.md`、`draft/README.md` 原地保留：`plan.md` 转为设计史记录（其 §5 的覆盖契约被新 99 取代，需在文首加一句指针）；`todo.md` 保持"实施规格缺口"职责不变（其 §6.1 的批次进度改为在 99 账本里反映状态，todo 只留处置过程）。

**明确删除的内容（各给理由）。**

| 删除项 | 位置 | 理由 | 是否影响其它知识点 |
|---|---|---|---|
| 交付链全链复盘 | 旧 04 §6 | 该内容属总览（新 00），放在设备篇是越界 | 不影响，K-003 保留在 00 |
| `termcap` 契约表行 | 旧 13 §1.6 | 该命令在 `minix3/` 中不存在（实测无命令目录），属虚构覆盖 | 不删除任何真实知识点；K-231 加一句注记 |
| `pr`/`tsort` 在 07 的表行 | 旧 07 §4.5 | 重复：同一命令在 07 与 10 两处出现；已定唯一归属（14/15） | K-179 保留（移 15），`pr` 的语义保留（移 14），只删重复行 |
| `fold` 在 10 的展开 | 旧 10 §1.1（已标"引用 07"） | 重复；已定唯一归属 11 | 不影响 |
| 各篇 §4.1 模块表、§4.3 函数一览、§5 测试统计 | 24 篇 | 陈旧（实测偏差如 06 写 4 文件实为 17、07 写 6 实为 65、01 写 15 实为 20）；且属账本而非教学内容 | 内容全部迁入新 99 账本，不丢失 |
| 各篇 §4.5/§4.4 的 `Requires` 表 | 06/07/08/22 四篇 | 合并为 99 的 `Requires` 总表，避免四份表格各自漂移 | K-109/K-130/K-149/K-359 保留，改指向 99 |
| 各篇"附：验证记录" | 24 篇 | 内容是评审过程的自我报告，不是读者需要的知识；其事实（测试数、文件数）迁入 99 账本 | 不丢失（投影到账本） |
| 旧 12 §3.5 跨域工程政策 | 旧 12 | 越界：属跨篇政策而非进程工具主题 | K-288 移 03 §5 |
| 旧 04 §1.4 的 `gettytab`/`utmp`/`nsswitch` 三行 | 旧 04 | 越界三处 | K-053/054→06、K-424→18、K-296→24 |
| 旧 17 §3.5、旧 21 §3.5 的"与同类系统对照" | 旧 17/21 | 与主题无关的横向比较，且无锚点 | 删除，不入池 |
| 旧 99 的"文档状态与范围"stub（K-388） | 旧 `99-global-concepts.md` 头部 | 它记录的是旧文档自身的成稿进度（"三节成稿、其余待改写"），属过程叙述；重建后旧 99 整体改写，该条自动作废 | 不影响其它知识点：三节内容分别归新 03（§1/§2/§3 的判定规则）与 99 账本 |

**不归档但需改写的参考材料（4 份）。**

| 文件 | 需要的改动 |
|---|---|
| `plan.md` | §2 的文档清单表与 §3.3 的边界表按新编号重写；§5 的覆盖契约加一句"已被 `99-实现账本` 取代"；§7.2 的 F10 行（"`login`/`truncate` 双目录存在"）按 §0.3 的实测更正；§7.3 的"`etc/` 45 项"改为 49 |
| `todo.md` | 条目本身保留；§5 的实证清单与 §6.1 的批次进度改为指向新 99 账本 |
| `00-master-plan/README.md` | 若其中引用旧编号，按 §8 的引用迁移表批量替换 |
| 其它 stage 的文档 | 对本 stage 的引用按 §8 批量替换（见引用迁移表） |

---

## 7. 缺漏新篇

按 §3.2 的缺口表逐项落实；不允许留空，不允许写"待定"。

| 缺口 | 主题是什么 | 为什么重要 | 原料在哪里 | 归哪一篇 | 验收标准 |
|---|---|---|---|---|---|
| GAP-1 | 命令的统一生命周期（交棒、参数、选项、退出码、三条通道） | 326 条命令的唯一汇聚点；缺了它每篇都要重复讲"命令怎么拿到参数" | `lib/csu/arch/x86_64/crt0.S:39-44`；`lib/csu/common/crt0-common.c:145-191`；`lib/libc/stdlib/getopt.c`；`include/getopt.h`；349 个手册页的 `SYNOPSIS`；`os/commands/bin/fileops/src/bin/echo.rs` | **新建新 01** | 新 01 的验收标准（见 §5）：`argv` 来源、选项解析、退出码、三条通道四问各有一个 `file:line` 锚点 |
| GAP-2 | 命令的注册与发现机制（`BINDIR`、`SUBDIR`、PATH、双层游戏安装、静态链接、手册页索引、`DESCRIBE`） | 集合型 stage 的"注册机制"与"发现机制"；也是"命令最终以什么形态交付"的唯一回答 | 五个 `Makefile.inc`；`usr.bin/Makefile`、`minix/commands/Makefile`；`include/paths.h:43/45`；`etc/man.conf`；`DESCRIBE.sh`；`os/Cargo.toml` 与 `os/commands/` 实测 | **新建新 02** | 新 02 的验收标准：每处安装路径带行号；"现状"与 `os/commands/` 实测一致 |
| GAP-3 | 跨篇约定（依赖分层、判定基准、契约表模板、参数框架、退出码、单线程、守卫） | 判定规则必须在引用者之前；现结构里中段各篇引用文末的 99，构成前向引用 | 旧 99 §1/§2/§3；`tools/check-command-boundary.sh:26-36`；`os/libs/minix-sys/src/lib.rs`；`plan.md §4` A-4/A-5；`todo.md §2/§4`；`edge4 §6` | **新建新 03** | 新 03 的验收标准：每条硬规则配一条可执行复核命令；`minix-sys` 行号当场重取 |
| GAP-4 | `rc.minix`/`rc.cd`/`rc.capes`/`rc.shutdown` 的真实启动动作与 `rc.d` 依赖图 | 开机真正干活的一段；现有 01 篇零次提及，会把读者引向"32 个脚本各自干活"的错误图景 | `etc/rc.minix:78-88/100-103/144/145/147-152/155-156/159-179/182-187/191-198/200-203/229-237`；`etc/rc.cd:26-29`；`etc/rc.d/minixrc:13`；`etc/rc.d/` 32 个脚本的头部关键字 | **新 04 §2.6、§2.7**（新增节） | 新 04 的验收标准：给出 32 个脚本的依赖一览表 + 从 `init.c:975` 到 `rc.minix:203` 的完整链 |
| GAP-5 | `utmp`/`utmpx`/`wtmp` 三方契约 | 会话真相的唯一来源；现被三篇各讲一半 | `lib/libc/compat/include/utmp.h:42-46`；`login/common.c:166+`；`etc/rc.minix:155-156`；`usr.bin/{who,w,last,users}` | **新 18 §2（主）** + 新 06 §2.5（写侧）+ 新 04 §2.7（清侧） | 新 18 的验收标准：字段偏移表 + 三方迁移链 |
| GAP-6 | 命令到 FS 服务端与块驱动的调用接缝 | 存储四篇整体推给 `15-stage-fs`，对方只拿到移交声明；接缝无对照材料 | `minix/commands/mount/mount.c`；`sbin/fsck/fsck.c:254`；`sbin/newfs_*`；`usr.sbin/makefs`；`bin/dd/args.c`；`isoread.c:41`；Rust 侧 `diskimg/src/device.rs:12`、`mountinfo/src/` | **新 20 §2、21 §2、22 §3、23 §2**（各一节） | 各篇给出"命令侧请求形状 → 服务端/驱动"的接缝图，只写请求形状与错误处理 |
| GAP-7 | 十一条孤儿命令的落点 | `plan.md §3.6` 要求契约表覆盖全部命令；孤儿让读者无法判断是否被覆盖 | 见 §3.4 与各篇契约的"新增"行（K-429…K-438） | 新 09 §1、新 18 §1、新 20 §1、新 23 §2、新 25 §1-§2、新 26 §1-§2、新 27 §2、新 29 §1、新 30 §2 | 每篇的族内差异表覆盖数与该篇归属表的命令数一致 |
| GAP-8 | `termcap` 幽灵命令 | 契约表里出现不存在的命令 | 实测无 `minix3/*/termcap/` 目录；`etc/termcap`、`etc/termcap.big`、`lib/libterminfo/termcap.c` 是数据库与库 | **新 19 §1 注记**（从表撤下） | 新 19 显式写出"`termcap` 不是命令" |
| GAP-9 | `shlock` 与调度器的关系 | 三个调度器共用的锁原语，现只在杂项里列名 | `usr.bin/shlock/`；`cron`/`at`/`backup` 调用点 | **新 05 §2.7**（新增节） | 列出三个调用点与锁文件语义 |
| GAP-10 | `machine`/`pagesize`/`domainname` 三条无覆盖命令 | 与 `uname`/`hostname` 同族，属执行环境自述 | `usr.bin/{machine,pagesize}`、`bin/domainname` | **新 08 §1.6** | 三条各有一行契约与 C 源锚点 |
| GAP-11 | 发行集合与镜像装配 | 交付形态的最外层 | `minix3/distrib/sets/`；`minix3/releasetools/release.sh` | **新 02 §1**（范围外指针） | 显式声明"不在本 stage"并给指针 |
| GAP-12 | C 侧命令测试基建 | 与 Rust 侧 990 个 `#[test]` 的对照来源 | `minix3/tests/`（ATF 测试树） | **新 99 §5**（对照说明） | 账本的测试一节给出两侧测试的口径对比 |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表

按旧篇逐节列出。迁移类型五档：原样搬移（内容与结论保留，仅换位置）、改写（本章节需按新契约重写）、合并（与别处合并）、拆分（本节内容分到两处）、删除（附理由）。

| 旧位置 | 旧内容（一句话） | 新位置 | 迁移类型 | 备注（断链风险） |
|---|---|---|---|---|
| 00 头部 | 状态与源码/模块面声明 | 00 头部 | 改写 | 无外部引用 |
| 00 核心点 1-5 | 交付链图、统计、功能域矩阵、导航、设计原则 | 00 §1–§4 | 改写 | 新 00 的导航表必须与 §4.1 逐行一致 |
| 00 边界 | 前置与不覆盖 | 00 §5 | 改写 | 新 00 前置保持三 stage 不变 |
| 01 §1 | 概念（第一个进程、运行级别、三层架构、开关机互逆） | 04 §1 | 原样搬移 | 无 |
| 01 §2.1-2.4 | 状态机、`runcom`、会话管理、信号面 | 04 §2.1-2.4 | 原样搬移 | 无 |
| 01 §2.5-2.6 | `/etc/rc` 流程、`rcorder` | 04 §2.5-2.6 | 改写（补 `rc.d` 依赖图与空壳清单） | 新增 K-423 |
| 01 §2.7-2.8 | shutdown/reboot/setup、配置面 | 04 §2.7-2.8 | 改写（补 `rc.minix` 动作与 `bootconf.sh`） | 新增 K-422 |
| 01 §3 | 四条 Rust 设计决策 | 04 §3 | 原样搬移 | 无 |
| 01 §4-§5 | 模块表、关键类型、测试 | 99 §1、§5 | 合并 | 必须重取实际文件数（20）与测试数（136） |
| 01 §6-§7 | 过渡、参见 | 04 §5-§6 | 改写（编号替换） | 引用迁移见 §8.2 |
| 02 §1-§2 | 服务概念、两个命令、`svrctl`、三种调度、时间方言、九形状、三段式、解析内部、`at`、`update` | 05 §1-§2 | 原样搬移 + 补 `shlock` 节 | 新增 K-439 |
| 02 §3-§5 | Rust 决策、模块、测试 | 05 §3 + 99 | 拆分（决策留本篇，账本进 99） | 无 |
| 03 §1-§2 | 三棒交接、口令库两脸、身份命令、会话环境、getty、ttys、gettytab、login 验证与收尾、实例导读 | 06 §1-§2 | 合并（§1.2 与 §2.6 合一） | `gettytab` 从 04 收拢；`utmp` 写侧留本篇 |
| 03 §3-§5 | Rust 四格式模块、零拷贝、双实现、判定规则、测试 | 06 §3 + 99 | 拆分 | 无 |
| 04 §1.1-1.3 | 设备即文件、双输出、设备数据库 | 07 §1 | 原样搬移 | 无 |
| 04 §1.4 | 系统数据库、`gettytab` 一行、`utmp` 一行、`nsswitch` | 07 §1（主体）、06 §2.3、18 §2、24 §1 | 拆分（三处越界移出） | 四处引用需同步改写 |
| 04 §2.1-2.5 | `makedev`、`mknod`、`dev_mkdb`、`getent`、`mtree` | 07 §2 | 原样搬移 + 补 `devmand` 边界节 | 修正 `dev_mkdb.c` 锚点（`FILE_PERMISSION` 在 `:54`） |
| 04 §3-§5 | Rust 决策、模块、测试 | 07 §3 + 99 | 拆分 | 无 |
| 04 §6 | 交付链全链复盘 | 删除（内容归 00） | 删除 | 无知识点丢失 |
| 05 §1.1-1.4 | shell 双重身份、方言、启动文件、内建 | 08 §1 | 原样搬移 | ksh/csh 需补锚点 |
| 05 §1.5 | 环境命令六件套 | 08 §1.6 | 改写（越界合法化 + 补三条） | 头块"不覆盖：命令工具本体"需同步改写 |
| 05 §2.1-2.6 | C 分析六节 | 08 §2 | 原样搬移 | 修正 `redir.c` 两个锚点（`:79`、注释 `98-104`） |
| 05 §3-§5 | Rust 决策、模块、测试 | 08 §3 + 99 | 拆分 | 无 |
| 06 §1.1-1.2、§1.3（空间与并发） | 改属性、建关系、搬运查看 | 09 §1 | 改写（补 `cat`/`mv`/`rm` 家族） | 新增 K-112/K-113 |
| 06 §1.3（路径组）、§1.4 | 路径三条、判断求值家族 | 10 §1 | 拆分（OP-S01） | `expr` 落到 12 |
| 06 §2.1-2.5 | `test`、`chmod`、三大量级、小命令、契约总表 | 09 §2（`cp`/`ls`/`find`）+ 10 §2（`test`）+ 99（契约表） | 拆分 | 契约表表头"34 命令"与 35 行不符，重建时按归属表核对 |
| 06 §3-§5 | Rust 决策、模块、契约、测试 | 09 §3 + 10 §3 + 99 | 拆分 | `§4.5` 的 ENOSYS 锚点已过期（`open_existing_via` 已落地），须重写状态列 |
| 07 §1.1-1.6 | 六家族与新阅读顺序 | 11 §1（重划为六族） | 改写（撤 `pr`/`tsort`，并入 `fix`） | K-135 的孤儿清单在新篇落实 |
| 07 §2.1-2.5 | `uniq`、`cut`、`wc`/`head`、`tr`、`sort` | 11 §2 | 原样搬移 + 补 `diff`/`patch` 深讲 | 旧 §4.5 的状态行扩为独立深讲（K-133/K-134） |
| 07 §3-§5 | Rust 决策、模块、契约、测试 | 11 §3 + 99 | 拆分 | 模块数（实际 65 文件）与测试数（229）须重取 |
| 07 §4.5 的 `pr`/`tsort` 行 | 两条重复行 | 14 §1 / 15 §2 | 删除重复行 | `textfilter` crate 的 `pr.rs`/`tsort.rs` 归属在 99 账本标明 |
| 08 §1-§2 | 正则语言、`grep`/`sed` 机制 | 12 §1-§2 | 原样搬移 + 补 `sed` 动词面与 `expr` | 新增 K-150、K-116 |
| 08 §3-§5 | Rust 决策、模块、契约、测试 | 12 §3 + 99 | 拆分 | 模块数（实际 8 文件）须重取 |
| 09 §1-§4 | 会话模型、地址、撤销、`vi` 悬置、`ed` 机制、三层复用、Rust 决策 | 13 §1-§3 | 原样搬移 | `vi` 的悬置状态须保留并指向 OQ-2 |
| 09 §5 | 测试 22 个 | 99 | 合并 | 无 |
| 10 §1.1-1.2、§1.4 | 排版家族、手册系统、日历 | 14 §1 | 原样搬移 + 补 `calendar`/`ul` | 新增 K-183 |
| 10 §1.3、§1.5 | 开发辅助四组、国际化 | 15 §1 | 拆分（OP-S02） | i18n 裁决记录移 99 |
| 10 §2.1-2.3 | `man`/`manconf`、`makewhatis`、`cal` | 14 §2 | 原样搬移（三条升为深讲） | 无 |
| 10 §2.4-2.5 | 职责级覆盖、契约总表 | 14 §1 + 15 §2 + 99 | 拆分 | 表内 `tsort` 移交 15，`tsort` 从 14 撤下 |
| 10 §3-§5 | Rust 决策、模块、测试 | 14 §3 + 15 §3 + 99 | 拆分 | 无 |
| 11 §1-§4 | 三变小、LZW、编码、归档、校验、四处深讲、Rust 决策 | 16 §1-§3 | 原样搬移 | §3.2 的宽度切换论证必须保留双锚 |
| 11 §5 | 测试 26 个 | 99 | 合并 | 无 |
| 12 §1.1-1.2 | 看进程、发信号 | 17 §1 | 原样搬移 | 无 |
| 12 §1.3 | 看会话（`utmp` 读法） | 18 §1-§2 | 拆分（OP-S03） | `utmp` 升为主讲述点 |
| 12 §1.4 | 时间、优先级、锁、IPC、通知 | 17 §1（优先级/IPC）+ 18 §1（消息/时间）+ 05 §2.7（`shlock`） | 拆分 | 五族按数据源重分 |
| 12 §1.5 | 28 命令契约表 | 99 | 合并 | 拆成 17/18 两篇的族内差异表 |
| 12 §2.1-2.3 | `kill`、`ps` 列定义、`utmp` 结构 | 17 §2（前两者）+ 18 §2（后者） | 拆分 | 无 |
| 12 §3-§5 | Rust 决策、模块、测试 | 17 §3 + 18 §3 + 03 §5（K-288 的误置） | 拆分 | K-288 移 03 |
| 13 §1.1-1.3 | 速度、控制字符、标志 | 19 §1 | 原样搬移 | 无 |
| 13 §1.4 | 能力数据库、两代工具链、A-2 | 19 §1-§2 | 原样搬移 | A-2 裁决移 99 |
| 13 §1.5 | 三件设备小工具 | 19 §1 | 原样搬移（只讲职责） | 无 |
| 13 §1.6 | 10 命令契约表（含 `termcap`） | 19 §1 注记 + 99 | 改写（撤幽灵行） | 覆盖数从 10 改 9 |
| 13 §2-§5 | C 分析、Rust 决策、模块、测试 | 19 §2-§3 + 99 | 拆分 | 无 |
| 14 §1.1-1.3 | 挂载语义、fstab、检查序号、preen | 20 §1-§2 | 原样搬移 | 补 `umount` 完整节 |
| 14 §2.1-2.4 | `mount.c`、`fsck.c`、`newfstab.sh`、族群 | 20 §2 | 原样搬移 + 补接缝节 | 新增 K-425/K-426 |
| 14 §2.4 的 `fattr.c` | 文件属性工具 | 09 §1 指针 | 拆分 | 无 |
| 14 §3-§5 | Rust 决策、模块、测试 | 20 §3 + 99 | 拆分 | 无 |
| 15 §1-§4 | MBR、类型码、工具链、格式化家族、单位解析、Rust 决策 | 21 §1-§3 | 原样搬移 + 并入 `dosread` | 新增 K-427 |
| 15 §5 | 测试 9 个 | 99 | 合并 | 无 |
| 16 §1.1-1.2 | `dd`、光盘卷 | 22 §1-§2 | 原样搬移 | 无 |
| 16 §1.3 | 内存盘与虚拟盘（含 `dosread`） | 22 §1 + 21 §1（`dosread`） | 拆分 | 无 |
| 16 §2-§3 | C 分析、Rust 决策 | 22 §2-§3 | 原样搬移 + 补接缝 | 新增 K-428 |
| 16 §5 | 测试 13 个 | 99 | 合并 | 无 |
| 16 §6 与 17 §6 的"存储四篇"口径 | 两处不一致的闭合说法 | 20–23 的 §1 统一句 | 改写 | §3.3 重复主题表已登记 |
| 17 §1-§2.5 | 三类担心、备份、`remsync`、`synctree`、`cleantmp`、进度条、`mt` | 23 §1-§2 | 原样搬移 + 补引导刷新节 | 新增 K-432 |
| 17 §2.6 | `rotate`、`fix` | 18 §2、11 §2 | 拆分（越界移出） | K-286、K-287 |
| 17 §3.1-3.4 | 四条 Rust 决策 | 23 §3 | 原样搬移 | 无 |
| 17 §3.5 | 轻量解析与静态链接政策 | 03 §5 | 拆分（越界移出） | K-288 |
| 17 §4-§5 | 模块、测试 50 个 | 99 | 合并 | 无 |
| 18 §1-§4 | 四问、接口、路由、探测、追踪、名字库、五决策、模块 | 24 §1-§3 | 原样搬移 + 补 ioctl 接缝 | 修正两处头部锚点（`RTM_VERSION` 在 `route.c:1196`；`netstat` 在 `main.c`） |
| 18 §2.4 的 `netstat`/`netconf.sh` | 只在 C 侧点名的两条命令 | 24 §1-§2 | 改写（补节） | 新增 |
| 18 §5 | 测试 31 个 | 99 | 合并 | 无 |
| 19 §1-§3 | 三件事、`inetd`、`syslogd`、`fetch`、准入、五决策、模块 | 25 §1-§3 | 原样搬移 + 补四守护与邮件打印 | 新增 K-433/K-434 |
| 19 §5 | 测试 25 个 | 99 | 合并 | 无 |
| 20 §1-§3 | 四种目光、五组命令、四决策、对照 | 26 §1-§3 | 原样搬移 + 补十四条规定点名命令 | 新增 K-435 |
| 20 §4-§5 | 模块、测试 27 个 | 99 | 合并 | 无 |
| 21 §1-§3 | 供给四事、六组 C 锚点、五决策 | 27 §1-§3 | 原样搬移 + 补 `pkgin_all`/`postinstall` | 新增 K-436 |
| 21 §3.5 | 与同类系统对照 | 删除 | 删除（无锚点、与主题无关） | 无知识点丢失 |
| 21 §4-§5 | 模块、测试 28 个 | 99 | 合并 | 无 |
| 22 §1-§4 | 验收层、十个游戏机制、四决策、模块/类型/契约 | 28 §1-§3 | 原样搬移 + 补 `bcd`/`ppt` 玩法位 | 无 |
| 22 §5 | 测试 102 个（附录写 35，自相矛盾） | 99 | 合并 | 按实测 54 个 `#[test]` 与手数合计两种口径分别说明 |
| 23 §1-§4 | 格子三件事、三组 C 锚点、五决策、模块 | 29 §1-§3 | 原样搬移 + 补 `colorbars` | 新增 K-437 |
| 23 §5 | 测试 32 个 | 99 | 合并 | 无 |
| 24 §1-§4 | 故事三件事、三组 C 锚点、五决策、模块 | 30 §1-§3 | 原样搬移 + 补 `wargames` | 新增 K-438 |
| 24 §5 | 测试 22 个 | 99 | 合并 | 无 |
| 99 §1-§3 | 分层契约、判定基准、契约表模板 | 03 §1-§3 | 原样搬移（唯一一次跨篇大移动） | 引用迁移见 §8.2；`minix-sys` 行号须重取 |
| 99 核心点 1-7 | 安装面、A-1、A-4、A-5、`/etc`、`DESCRIBE`、curses | 02 §1、03 §4-§5、99 | 拆分 | 无 |
| 99 边界、参见 | 前置与参见网络 | 03 边界、99 §6 | 改写 | 无 |

### 8.2 引用迁移表

分三类：文档之间的交叉引用、代码注释里的引用、非文档制品里的引用。

**第一类：文档之间的交叉引用（stage 内）**

| 引用形式 | 出现次数 | 新目标 | 验证方式 |
|---|---|---|---|
| `NN-name.md` 形式的篇间引用（stage 内） | 167 处（逐篇出站计数：01 篇 15；03、04 篇各 11；07 篇 9；09、12、17 篇各 8；06、10、16 篇各 7；02、05、08、11、20、21、24 篇各 6；13、22、23 篇各 5；14、15、18、19 篇各 4；99 篇 3；00 篇 0。合计 167） | 按 §4.1 的映射表逐个替换（旧 01→04、…、旧 24→30；旧 99 的引用按引用内容分别指向 03 或 99） | 重建完成后跑 `grep -oE '[0-9]{2}-[a-z0-9-]+\.md' 新目录/*.md \| sort -u`，逐个确认文件名存在 |
| 指向 `plan.md` / `todo.md` 的引用 | 约 30 处 | 不变（两份文件原地保留），但引用的小节号若因改写变动需同步 | `grep -n 'plan.md §\|todo.md §'` 逐个核对小节存在 |
| 指向其它 stage 的引用（`../NN-stage-*`） | 约 20 处 | 不变 | 无 |
| `99-global-concepts.md §1/§2` 的规则引用 | 06/07/08/22 四篇的 `§4.4/§4.5` 各一处起 | 改指 `03-…md §1/§2/§3` | `grep -rn '99-global-concepts.md §' 新目录/` 应为空 |

**第二类：代码注释里的引用**

| 引用形式 | 出现位置 | 新目标 | 验证方式 |
|---|---|---|---|
| `//! Covers .../18-stage-commands/NN-*.md` | 24 个 crate 的 `src/lib.rs:5` 各一行（`diskimg`、`editor`、`fileops`、`proctools`、`shell`、`sysinfo`、`termctl`、`stdio-games`、`term-games`、`text-games`、`devdb`、`diskfmt`、`init`、`maint`、`mountinfo`、`compress`、`doctools`、`login`、`regex`、`textfilter`、`netconfig`、`netservices`、`pkgtools`、`svcsched`） | 按 §4.1 映射替换文件名（例如 `bin/diskimg` 从 `16-image-media.md` 改为 `22-image-media.md`） | `rg -c '18-stage-commands/[0-9a-zA-Z._-]+\.md' os/commands/` 逐个确认新文件名存在；**注意 `pr.rs`/`tsort.rs`/`utmp.rs`/`rotate.rs` 四处跨篇错配要在注释里写明语义归属** |
| `99-global-concepts.md §1` 的政策引用 | `os/commands/bin/fileops/src/bin/echo.rs:8` | 改指新 03 的 §1 | `rg -n '99-global-concepts' os/` 应为空 |
| `99-global-concepts.md` 的裁决引用 | `os/libs/minix-sys/src/vfs.rs:968`（64 位布局裁决出自 99） | 改指新 03 §3（契约表与布局裁决）或新 99 的 ARCH 行 | 同上 |
| `18-stage-commands/README.md`（若存在） | 实测 `os/commands/README.md` 不存在 | 无需处理 | `ls os/commands/README.md` 确认不存在 |

**第三类：非文档制品里的引用**

| 引用形式 | 出现位置 | 新目标 | 验证方式 |
|---|---|---|---|
| `# 依据: 18-stage-commands/99-global-concepts.md §1` | `tools/check-command-boundary.sh:3` | 改指新 03 §1 | `bash tools/check-command-boundary.sh` 仍输出 OK |
| `18-stage-commands/99-global-concepts.md` 或 `06-file-ops.md` 的路径引用 | `os/Cargo.toml` 的注释（1 处） | 按映射替换 | `rg -n '18-stage-commands' os/Cargo.toml` |
| `18-stage-commands/todo.md` 引用 | `prompt/todo_plan.md` 3 处、`edge_todo.md` 3 处、`edge3.md` 3 处 | 不改（todo.md 原地保留）；但其中引用具体篇号的位置需替换 | `rg -n '18-stage-commands/[0-9]{2}-' prompt/ rewrite-notes/edge*.md` 逐个核对 |

### 8.3 断链成本摘要

| 维度 | 数量 | 说明 |
|---|---|---|
| 受影响的引用总数 | **约 250 处** | stage 内篇间引用 167 + `plan.md`/`todo.md` 内部引用 49 + 代码注释 26 + 非文档制品 2 + 其它 stage 文档 9 |
| 受影响文件数 | **约 60 个** | 26 篇文档 + 2 份参考材料 + 24 个 crate 的 `lib.rs` + 1 个守卫脚本 + 1 个 `os/Cargo.toml` + 约 6 份其它 stage 文档与提示词 |
| 热点 1 | 新 06（旧 03） | 旧 03 是被引最多的一篇（入站 17 处），改号后所有指向它的引用都要改 |
| 热点 2 | 新 09/10（旧 06）与 新 11（旧 07） | 旧 06 入站 12 处、旧 07 入站 8 处；旧 06 一拆为二，引用要按引用内容分流（指文件操作的进 09，指路径/真值的进 10） |
| 热点 3 | 新 03（旧 99 §1/§2/§3） | 四篇的 `§4.4/§4.5` 直接引用它；这是唯一一处"引用内容搬家"的迁移，不能机械替换，必须按引用的小节号分流 |
| 热点 4 | 24 个 crate 的 `//!` 头注释 | 一行一个文件名，24 处；机械替换但必须逐个确认目标存在 |
| 建议的批量修改方式 | 三步 | 第一步用一份"旧名 → 新名"的映射表（26 行）跑机械替换，覆盖所有 `NN-name.md` 形式的引用；第二步人工处理两类不可机械替换的引用：旧 06 的分流引用（指文件操作的归 09、指路径与真值的归 10，约 20 处）与旧 99 的分流引用（指分层契约与判定基准的归 03，指 ARCH 与账本的归 99，共 6 处）；第三步跑三条验证——(a) `grep -oE '[0-9]{2}-[a-z0-9-]+\.md' 新目录/*.md` 的输出逐个 `ls` 存在；(b) `rg -n '99-global-concepts|06-file-ops|07-text-filter|10-doc-man-tools|12-process-tools' 全仓` 只应命中 `archive/` 与历史记录；(c) `bash tools/check-command-boundary.sh` 输出 OK |

**成本与收益的对照。** 断链成本约 250 处机械修改，与 `14-stage-runtime` 那轮"互引约 100 处、入站 60 处"的重排相比数量更大（本 stage 有 24 个 crate 的头注释这一层独有成本）。收益是修掉三个结构性问题：框架缺位（新 01/02/03）、约定前向引用（旧 99 前移）、三个多语义篇（旧 06/10/12 拆分）。若不重排，这三条都修不掉，且账本仍在 24 处各自维护。裁决建议：**执行重建**，并在 B 相把 250 处引用迁移作为一次独立的机械提交，与内容改写分开，便于回滚。

---

## 9. 验证与自检门

### 9.1 四种机械检查结果

**检查一：前向引用扫描。** 按新目录顺序逐篇检查契约里的"前置"字段（§5 各篇的"前置"行），只允许指向更早的编号。结果：

| 篇 | 前置 | 是否全部更早 |
|---|---|---|
| 00 | 三个其它 stage | 不适用（stage 外） |
| 01 | 00 | 是 |
| 02 | 00、01 | 是 |
| 03 | 00、01、02 | 是 |
| 04 | 其它 stage、03 | 是 |
| 05 | 04、03-stage-rs | 是 |
| 06 | 04、05、03 | 是 |
| 07 | 04、06、11-stage-devman | 是 |
| 08 | 06、03 | 是 |
| 09 | 08、14-stage-runtime | 是 |
| 10 | 09、08 | 是 |
| 11 | 09、10 | 是 |
| 12 | 11 | 是 |
| 13 | 12、08 | 是 |
| 14 | 11、12 | 是 |
| 15 | 11、12 | 是 |
| 16 | 09、14 | 是 |
| 17 | 08、03 | 是 |
| 18 | 06、08 | 是 |
| 19 | 08、03 | 是 |
| 20 | 09、15-stage-fs、03 | 是 |
| 21 | 20、15-stage-fs | 是 |
| 22 | 20、21 | 是 |
| 23 | 20、21、09 | 是 |
| 24 | 19、17-stage-net、03 | 是 |
| 25 | 24、18 | 是 |
| 26 | 00、03 | 是 |
| 27 | 05、21 | 是 |
| 28 | 10、03 | 是 |
| 29 | 19、10 | 是 |
| 30 | 10、14 | 是 |
| 99 | 全部 | 不适用（末篇） |

结论：**0 处前向引用**。唯一需要注意的一处是 `expr`：它原本与 `test` 同族（新 10），因为 `:` 需要基本正则而移到新 12，这是为消除前向引用而做的调整（序差-2）。另外，"本篇不展开、由第 NN 篇承担"形式的**去向指针**不视为前向引用（提示词的契约格式本身就要求写去向），判定标准是"读者不读第 NN 篇能否读懂本篇"。

**检查二：依赖关系图无环。** 以 §5 的"前置"字段为边，图为：`00 → 01 → 02 → 03 → {04, 05, 06, 07, 08, 09, 10, 17, 19, 26}`，`{09,10} → 11 → 12 → {13, 14}`，`14 → {16}`，`{11,12} → {14,15}`，`{06,08} → 18`，`{08,03} → 19`，`{09,15-stage-fs} → 20 → {21 → 22 → 23}`，`{19,17-stage-net} → 24 → 25`，`{05,21} → 27`，`{10,03} → 28`，`{19,10} → 29`，`{10,14} → 30`，全部 `→ 99`。拓扑排序存在（按编号顺序即为一个合法拓扑序），**无环**。

**检查三：覆盖率百分之百。** §2 的知识点池共 441 条（存量 400 + 新增 41），逐条已给去向（§2.1、§2.4 的"去向"列与 §5 的清单）；新增 41 条全部有证据锚点（C 源码 `file:line`、非 C 制品路径或理论出处）。明确删除项 11 类，全部在 §6.5 列出并给了理由，逐项确认不带走真实知识点。**结论：全覆盖，无待定项。**

**检查四：断链成本统计。** 见 §8.3：约 250 处引用、约 60 个文件、4 处热点、3 步批量修改方案与 3 条验证命令。

### 9.2 自检门逐门结果

| 门 | 检查内容 | 结果 | 证据 |
|---|---|---|---|
| G1 | C 真序逐条可核对（随机抽十条核对锚点） | **通过** | 抽查十条：(1) `init.c:133-146` 状态常量——实读命中；(2) `init.c:899` `sh /etc/rc autoboot` 的 argv 组装——命中；(3) `etc/rc.minix:145` `mount -a`——命中；(4) `etc/rc.d/minixrc:13` `sh /etc/rc.minix start`——命中；(5) `lib/csu/common/crt0-common.c:191` `exit(main(...))`——命中；(6) `games/Makefile.inc:15` `SYMLINKS+= dm /usr/games/${PROG}`——命中；(7) `include/paths.h:45` `_PATH_DEFPATH`——命中；(8) `bin/expr/expr.y:112` `regcomp(..., REG_BASIC)`——命中；(9) `usr.bin/uniq/uniq.c:56` 标志位定义——命中；(10) `minix/commands/mtree` 的 `usr.sbin/mtree/mtree.c`——文件存在，函数级锚点由 B 相细核 |
| G2 | 知识点池完整：每个 C 文件、每个非 C 制品都有归属或"明确排除加理由" | **通过** | 七个源目录 326 条命令逐条进 §3.1 的分配表（0 遗漏）；49 项 `etc/` 配置与 349 个手册页在 §4.4 与各篇契约里有归属；排除项（`devmand`、`xorg.conf`、宿主工具链、图形栈）在 §0.1 与 §3.5 给了理由 |
| G3 | 新目录前向引用为零 | **通过** | §9.1 检查一：32 篇逐篇扫描"前置"，0 处违规 |
| G4 | 依赖关系图无环 | **通过** | §9.1 检查二 |
| G5 | 覆盖率百分之百；新增条目都有证据锚点；删除项单独列出 | **通过** | §9.1 检查三 + §6.5 的删除项表 |
| G6 | 每处拆分、合并写清存量去向；每处新建写清新增来源（抽查十处） | **通过** | 抽查：(1) OP-S01 的 9 条 K 逐条给去向；(2) OP-S02 的 6 条逐条给去向；(3) OP-S03 的 8 条逐条给去向；(4) OP-M01 的 13 条逐条给去向；(5) OP-N01 的 8 条新增各有 C 锚点；(6) OP-N02 的 9 条新增各有非 C 制品锚点；(7) OP-N03 的 5 条新增各有机制或文档锚点；(8) OP-N04 的来源是 `plan.md §5` 与各篇旧表，逐条可追；(9) K-116 的跨篇移动给了 C 锚点（`expr.y:112`）；(10) K-288 的越界移动给了原文位置（旧 17 §3.5） |
| G7 | 每篇契约七要素齐全 | **通过** | §5 的 32 篇契约逐篇含：一句话定位、讲什么、不讲什么（含去向）、前置、后置、事实底线、知识点清单加验收标准。知识清单按"同一锚点族合行"写（约定见 §5 开头） |
| G8 | 锚点迁移表覆盖所有变化文档的每一节；引用迁移表覆盖文档与代码注释 | **通过** | §8.1 覆盖 26 篇的全部小节（含"附：验证记录"的处置）；§8.2 三类引用齐备（stage 内 167 处、代码注释 26 处、非文档制品 2 处、其它 stage 9 处） |
| G9 | 事实断言都有锚点；推测项已标注 | **通过** | 抽查十处的锚点如前；本报告的三处不确定项已显式标注：`/etc/rc.ramdisk` 与 `/usr/etc/rc`、`/usr/local/etc/rc` 三个被引用的文件不在 C 树内（`etc/rc.minix:12`、`:229-230` 引用但文件缺失，已在 §1.2 标"不在本 C 树内"）；`usr.sbin/zdump`/`usr.bin/bzip2`/`bin/rcmd` 等命令的 `.c` 在其它树（Makefile `.PATH` 指向），§3.1 的分配按其目录归属处理；`plan.md §7.2` 的 F10 行有两处误述（`login`/`truncate` 并非双目录存在），按本次实测更正为 3 组同名（`mount`/`mail`/`banner`） |

### 9.3 结论与待用户裁决的问题

**结论。** 本蓝图完成 26 篇 → 32 篇的重建规格：新增 3 篇框架篇（统一生命周期、交付与安装面、跨篇约定）与 1 篇账本（99），把旧 `99` 的三节约定前移，把旧 06/10/12 三个多语义篇各拆为两篇，其余 20 篇按交付链与命令族顺序整体后移。知识点池 441 条全部有去向，明确删除 11 类且不带走真实知识；前向引用为 0；依赖图无环；断链成本约 250 处，给出了三步批量迁移方案与三条验证命令。**本报告结论为"完成"，可直接作为 B 相的施工图。**

**待用户裁决的问题（三项，全部给出倾向意见，不擅自代决）。**

| 编号 | 问题 | 事实与影响 | 倾向意见 |
|---|---|---|---|
| OQ-1 | 执行层留白的口径：命令的"执行面"（`open` 路径、`getdents`、`ioctl`、`fcntl`、`ps` 读内核表、时钟读取）本 stage 写到什么程度？现有多篇写着"待系统调用""显式留白"，但 `edge2 L10` 已销账（`stat` 族/`ioctl`/`fcntl`/`getdents` wrapper 与 `open` 既有路已落 `minix-types` 与 `minix-sys`） | 旧 06 §4.5 的 `ENOSYS` 状态已过期；若照旧口径写，读者会以为 `open` 还没通。影响 6 篇以上 | 建议：新 99 账本的 `Requires` 总表按**当前实测**重新登记（`open_existing_via` 已在 `os/libs/minix-sys/src/vfs.rs:627` 落地），各篇正文只写"命令侧请求形状 + 已实现/未实现的状态指向账本"，不再在正文里写"待系统调用"这类会过期的断言 |
| OQ-2 | `vi` 面（编辑器选型）是否在本轮补齐？旧 09 §1.5 把它标为悬置的重大决策，`plan.md` 无对应 ARCH 项 | 决定 `13` 的验收面（只有 `ed`/`mined` 还是含一个全屏编辑器）；影响 `13` 与 `29`（终端控制）的边界 | 建议：维持"悬置"，但把它登记为 `99` ARCH 登记表的一个显式条目（现状：Minix3 无 `vi`，`bin/ed` 与 `minix/usr.bin/mined` 是全部编辑器面；补齐需要终端全屏输入控件，属 `19` 的能力范围） |
| OQ-3 | `ksh`/`csh` 的交付面：旧 05 §1.2 把三者列为"shell 家族"，但只有 `sh` 有 C 与 Rust 双线分析，`ksh`/`csh` 只有一段方言史 | 决定 `08` 的验收面；若要求覆盖，需为两者各建解析面，工作量与 `sh` 同级 | 建议：维持"`sh` 主线 + `ksh`/`csh` 差异表"的口径，并在 `08 §1` 显式声明"本 stage 只交付 `sh` 的语义与实现面，`ksh`/`csh` 的解析实现不在本轮交付面"，把三者的差异写成可查的对照表 |

**（报告完）**
