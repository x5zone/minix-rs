# 09-stage-init 文档重建蓝图（glm）

```text
your_name(AI agent name) = glm
target_dir(关注的工作目录) = rewrite-notes/09-stage-init
repo_root(仓库根目录) = /home/xzhao/github/minix-rs
当前提交号 = d6ecd22cae78345e746c82ad7cbe3c1612c151c2

任务 = R 相·重建蓝图：输出 09-stage-init/doc_rerank_glm.md，不改任何正文。
约束 = 未引用 .design/ 与 tmp_design_and_todo/；未读取其它 AI 的 doc_rerank_* 产物；
       所有落盘产物仅本文件一个，带 _glm 后缀。
```

---

## 0. 元数据

- 执行者：glm；日期：2026-09-19；目标目录：`rewrite-notes/09-stage-init/`。
- **结论先行**：本 stage 的 15 篇文档（00~14 + 99）**全部为 2026-09-18 新写成稿**——code-excellence 首轮扫描 + 修复迭代轮（todo 23 条闭单 Fix #1~#20）+ 审计轮（P2-5 文档 drift 闭环、Gate E 全量对账"声称但代码缺失：无"、ALL DOCS COMPLETE）之后刚落定。因此本蓝图的操作集是三个已审 stage 中**最窄**的一档：**"保编号、修锚点、追基线"**：
  1. **保编号**——01 入口 → 02 状态机骨架 → 03 日志 → 04~06 启动三站 → 07/08 会话模型 → 09~11 稳态与关停 → 12~14 系统交互 → 99，编号即状态机推进序，满足四条硬标准（§9 G3/G4）；外部引用全为目录级（37 处）、篇内互引 ≈68 处、代码注释 0 处——重排零收益；
  2. **修锚点**——**全部 15 篇存在系统性锚点缺陷**："（LNN，工具生成）"形态的锚点**行号全部正确**（K&R 声明行约定，glm 逐条验证 22 个全 OK）但**符号名全部错误**（如 `init.c:make_utmpx（L229）` 实为 `main`、`init.c:setsecuritylevel（L624）` 实为 `transition`、`init.c:death（L1703）` 实为 `mfs_dev`）——这是本 stage 与 06/08 两个姊妹 stage 同源的工具产物缺陷，本 stage 因每篇定位行/参见行都带该形态而最密集；
  3. **追基线**——实测 `cargo test -p minix-init`：**135 passed / 1 FAILED**（136 个测试；todo 基线 143 已过时）。失败项 `host::tests::test_minix_host_honest_enosys_for_missing_wrappers`（host.rs:651）是 **2026-09-19 当天的跨 stage 活回归**：init 侧昨日在 E-INITSYS ① 轮钉下 `register_handlers == Ok(())` 的期望（commit 6a9614bc6），今天共享传输层的 E-SYSCALL-SIGN 诚实化改动（0cca4247d 等）把宿主回环行为改回诚实 `Err(EIO)`，init 侧 pinned 期望未跟。
- 审查范围：
  - **正文**：`[0-9][0-9]-*.md` 15 篇（2086 行）+ `README.md`（39 行，状态页）；
  - **参考材料**（不改）：`plan.md`（389 行）、`todo.md`（97 行）、`archive/todo-V1-archive-2026-09-18.md`（检索权威）、`draft/README.md`（占位素材）；
  - **范围外**：`.design/`、`tmp_design_and_todo/`、其它 AI 的 `doc_rerank_*`、`minix3/` 原树（只读）。

### 0.1 读取清单（步骤 0 四份清单）

**清单一：文档清单**（行数实测）

| 编号 | 文件 | 行数 | 状态 | 边界摘要 |
|---|---|---|---|---|
| 00 | 00-init-overview.md | 25 | 正文 v1（2026-09-18） | boot 链终点/非服务器执行模型/状态机主线/导航/Rust 实现形态 |
| 01 | 01-init-main-entry.md | 251 | 成文+审计轮 | main 八步/三重出生证明/BootArgs/EntryDecision |
| 02 | 02-init-state-machine.md | 196 | 成文+审计轮（有一处格式缺陷，G-4） | state_t 体操/七字符/handle/delset/transition/转换表 |
| 03 | 03-init-logging-failure.md | 150 | 成文+审计轮 | stall/warning/emergency/disaster + 排除项 |
| 04 | 04-init-single-user.md | 128 | 成文+审计轮 | 状态 's'：口令门/ALTSHELL/看护五结局 |
| 05 | 05-init-runcom.md | 103 | 成文+审计轮 | 状态 'r'：/etc/rc 双层实体/autoboot/chroot |
| 06 | 06-init-read-ttys.md | 88 | 成文+审计轮 | 状态 't'：自有 ttys 解析器（对齐 getttyent 字段语义） |
| 07 | 07-init-session-model.md | 88 | 成文+审计轮 | init_session 结构/SE_*/new/setupargv/分词 |
| 08 | 08-init-session-db.md | 75 | 成文+审计轮 | pid→session 反向索引；A-1 HashMap |
| 09 | 09-init-multi-user.md | 79 | 成文+审计轮 | 稳态 'm'：start_getty/collect_child/三常量/三处易漏语义 |
| 10 | 10-init-clean-ttys.md | 74 | 成文+审计轮 | 状态 'T'：存在性标记 diff |
| 11 | 11-init-shutdown.md | 72 | 成文+审计轮 | 'c' catatonia / 'd' death 三轮 kill |
| 12 | 12-init-sysctl-interaction.md | 72 | 成文+审计轮 | securelevel 三函数 + init.root/shouldchroot（A-4/A-5 defer） |
| 13 | 13-init-utmp.md | 77 | 成文+审计轮 | utmp/utmpx 台账（A-2 defer + 语义契约） |
| 14 | 14-init-external-contracts.md | 96 | 成文+审计轮 | 两个 Minix 挂钩 + 跨服务契约 13 项 |
| 99 | 99-init-global-concepts.md | 40 | 正文 v1（2026-09-18 补常量表） | 常量/路径/全局状态→Rust 归宿对照 |

**清单二：C 源码清单**

| C 文件 | 行数 | 承载文档 |
|---|---|---|
| `minix3/sbin/init/init.c` | 1902 | 01~14 全部（46 函数，plan §5.2 函数级映射经 2026-08-16 回归 review 逐项实证，本次抽验 22 个行号全对） |
| `minix3/sbin/init/pathnames.h` | 40 | 99（`_PATH_RUNCOM` 等；`_PATH_SLOGGER` 死常量已排除） |
| `minix3/sbin/init/init.8`、`NOTES`、`Makefile` | — | 00/02（状态机行为契约参考）/A-7 构建宏 |
| `minix3/include/paths.h`（:62-63,121,125） | 常量 | 99 |
| `minix3/include/{utmpx,utmp,ttyent,db}.h` | — | 13/06/08 |
| `minix3/lib/libc/gen/getttyent.c` | 对端语义 | 06（引号模式开关/精确 token/off 清除） |
| `minix3/lib/libcrypt/`（约 2000 行） | 口令后端 | 04（A-12 残余，建议独立 crate，todo P0-8） |
| 外部契约对端（`kernel/table.c:64`、`rs/table.c:28`、`pm/main.c:188-204`、`pm/forkexit.c:336,396`、`vm/main.c:346,498-514`、`keyboard.c:300`、`tps65217.c:226`、`procfs/service.c:195-207`） | 13 项 | 14（plan §5.3 全表，回归 review 已实证） |

**清单三：非 C 制品清单**

| 制品类 | 实际情况 | 归属 |
|---|---|---|
| 链接脚本/镜像布局 | 无（单文件用户程序） | 不在本 stage |
| 汇编入口与陷阱进入 | init 不做 trap 直接调用方；系统调用面走 minix-rt/minix-sys（E-INITSYS ①② 客户端面已落地 e8be0da5a+6a9614bc6） | 14-stage-runtime / edge |
| 引导链与引导协议 | boot_image 最后一项 + VM `exec_bootproc` 固定 argv `{"init",NULL}` | 00/01/14（证据面） |
| 构建脚本与工具链 | Makefile 七个编译宏（A-7） | 01/99（feature 取舍） |
| 跨模块接口与线格式 | utmp/utmpx 文件格式（A-2 defer，语义契约已钉）；shutdown 命令行方言 | 13/14 |
| 错误路径 | disaster 三层 + 五/六结局分类器（03/04/05） | 03~05/11 |
| 关闭与退出 | catatonia/death（11） | 11 |
| 并发与同步 | 单线程 + 信号 handler 置原子位（SignalState；自觉偏离已声明：fork 移出信号上下文） | 02 |
| 测试基建 | 136 个内联测试（剧本宿主 ScriptHost + 全图 boot-chain 测试）；真机归 E5 | 各篇 §5 |
| Rust 实现入口 | `os/commands/sbin/init/`（crate `minix-init`，**19 模块 5488 行**；依赖仅 minix-sys + minix-rt） | 各篇 §3/§4 |

**清单四：引用关系清单**

| 引用方 | 处数 | 形态 |
|---|---|---|
| 篇内互引实例 | ≈68 | 被引最多：09（10）、02（8）、05（7）、04（6） |
| `plan.md`/`todo.md` 自引 | 6+1 | 参考材料 |
| `edge_todo.md` | 4 | E-INITSYS/E-CMDSYSFACE/E-ISBOOT/E5 指针 |
| `04-stage-pm/doc_rerank_deepseek.md`（2）、`00-master-plan/README.md`（2）、`edge2/3.md`、`10/11/12-stage plan` 各 1 | 9 | 全部**目录级** |
| 代码注释（`os/` 下 .rs 引用本 stage 文档） | **0** | 无（与 sched/is 不同；init 的 README.md 承担了状态页职责） |

### 0.2 使用的命令与关键输出（证据摘录）

```bash
# 测试基线（1 个失败！todo 记 143 已过时）
cd os && cargo test -p minix-init
#   → test result: FAILED. 135 passed; 1 failed
#   失败：host::tests::test_minix_host_honest_enosys_for_missing_wrappers
#   host.rs:651  assert_eq!(host.register_handlers(&spec), Ok(()));
#   → left: Err(Errno(5))   right: Ok(())
#   根因时序：6a9614bc6（2026-09-18，init 钉 Ok 期望）
#   → 0cca4247d/df145274c 等（2026-09-19，E-SYSCALL-SIGN 传输失败符号短路/诚实化）
# 各模块测试数（合计 136）：ttys 16 / single_user 13 / runcom 13 / entry 12 /
#   utmp 9 / multi_user 9 / driver 8 / contracts 8 / session 6 / clean_ttys 6 /
#   signal_state 5 / shutdown 5 / session_db 5 / password 5 / sysctl 4 /
#   state_machine 4 / log 4 / host 4
# 锚点逐条验证（K&R 声明行约定：第 N 行是返回类型声明，函数名在 N+1 行）
sed -n '230p' minix3/sbin/init/init.c   # → main(int argc, char **argv)     （文档写 make_utmpx（L229））
sed -n '625p' minix3/sbin/init/init.c   # → transition(state_t s)           （文档写 setsecuritylevel（L624））
sed -n '441p' minix3/sbin/init/init.c   # → stall(const char *message, ...) （文档写 print_console（L440））
sed -n '695p' minix3/sbin/init/init.c   # → single_user(void)               （文档写 setctty（L694））
# 22 个锚点（main/transition/stall/single_user/runcom/read_ttys/start_session_db/
# multi_user/clean_ttys/catatonia/minixreboot/has_securelevel/session_utmpx/
# clear_session_logs/mfs_dev/handle/transition_handler/runetcrc/new_session/
# collect_child/death/setcty）全部行号 OK
# 常量核验
sed -n '92,96p;133,139p' minix3/sbin/init/init.c
#   → GETTY_SPACING 5/SLEEP 30/WINDOW_WAIT 3/STALL_TIMEOUT 30/DEATH_WATCH 10；七状态字符 ✓
# struct init_session（07 篇锚点）
sed -n '156,169p' minix3/sbin/init/init.c   # → typedef struct init_session { … } ✓
```

---

## 1. C 真序

### 1.0 阶段类型判定

**状态机驱动型**（单一新形态，区别于姊妹 stage 的"服务事件循环型"）。判定理由：init 不是 IPC 服务器（无 SEF/CALLMAP/主循环收消息），执行驱动只有 waitpid(-1) 阻塞循环 + 信号两个源，全部业务是 `transition()` 的状态推进（plan §1.2 判定成立，本次确认）。因此真序表以"boot 段 → 入口八步 → 状态机七状态推进序"为骨架，会话生命周期为次主线（plan §1.4 既定）。按 prompt 第九节的形态分类，init 最接近"启动链型"（以启动时序为骨架），但七状态是循环图而非线性链——主线取 's→'r→'t→'m 的首次推进序，'T/'c/'d 作支线。

### 1.1 真序表

**A boot 段（init 被加载与首次调度）**

| 步骤 | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| A1 | boot_image 最后一项登记 | `minix3/minix/kernel/table.c:64` | |
| A2 | RS 登记为 `USR_F` 普通用户进程 | `minix3/minix/servers/rs/table.c:28` | 非 IPC 服务 |
| A3 | VM `exec_bootproc` 为 boot 进程加载 ELF，argv 固定 `{"init",NULL}` | `minix3/minix/servers/vm/main.c:346,498-514` | `-s/-f` 在 boot 路径不生效 |
| A4 | PM 初始化：INIT 父进程=自身、INIT_PID=1、scheduler=KERNEL | `minix3/minix/servers/pm/main.c:188-204`；`pm/const.h:9` | |
| A5 | RTS 抑制位解除 → kernel 首次调度 init | `../01-stage-kernel/09-vm-boot-protocol.md` | |

**B 入口八步（`main`，init.c:229-367）**

| 步骤 | 动作 | C 锚点 | 失败策略 |
|---|---|---|---|
| B1 | 身份校验：uid!=0 → EPERM 退出；pid!=1 → "already running" 退出 | `init.c:242-249` | 退出 |
| B2 | `setsid()` 建初始会话 | `init.c:255-256` | 仅 warn |
| B3 | 非 Minix 分支 setlogin（Minix 跳过） | `init.c:262-265` | — |
| B4 | 设备探测：`MFS_DEV_IF_NO_CONSOLE` 下 `mfs_dev()` 失败 → 首状态改 single_user | `init.c:268-271`；`mfs_dev` :1703-1788（fork MAKEDEV） | 降级 |
| B5 | `openlog("init", LOG_CONS, LOG_AUTH)` | `init.c:278` | — |
| B6 | getopt "sf"：`-s` 置 single_user、`-f` 置 FASTBOOT | `init.c:287-303` | 警告并继续 |
| B7 | 信号注册八类 + 屏蔽 + 忽略 SIGTTIN/SIGTTOU | `init.c:310-334`（handle :369、delset :394） | — |
| B8 | close(0/1/2) → `createsysctlnode`（CHROOT）→ `has_securelevel()` → `transition(requested_transition)` | `init.c:339-358` | 跳转后不返回 |

**C 状态机推进序（`transition`，init.c:624-640，主循环）**

| 步骤 | 状态 | 动作 | C 锚点 | 下一站 |
|---|---|---|---|---|
| C1 | 's' single_user | fork shell（口令门 SECURE + ALTSHELL）→ 看护 waitpid 五结局 → 正常退出置 FASTBOOT | `init.c:694-877`（口令门 :747-763、看护 :825-873） | 'r' |
| C2 | 'r' runcom | `runetcrc(0)` 跑 /etc/rc（autoboot/fastboot）→ 失败回 's' → 成功且 `shouldchroot()` → `runetcrc(1)` | `init.c:974-1019`（runetcrc :879-969、chroot 决策 :990-998） | 't' |
| C3 | 't' read_ttys | 清旧会话链表 → `start_session_db`（失败：chroot 过进 'd' 否则回 's'）→ `do_setttyent` 逐行 `new_session` | `init.c:1222-1288`（DB 开 :1262-1271、new_session 过滤 :1147-1149） | 'm' |
| C4 | 'm' multi_user（稳态） | 安全级升 1（==0 才升）→ 全启动 getty → waitpid 主循环 `collect_child`（getty 退出重启 / SE_SHUTDOWN 摘链） | `init.c:1528-1564`（start_getty :1321-1370、collect_child :1460-1497、setctty :669-689） | 稳态驻留 |
| C5 | 'T' clean_ttys（SIGHUP） | 存在性标记 diff：命中更新/新行建会话/消失与熄火置 SHUTDOWN 发 SIGHUP | `init.c:1569-1632` | 'm' |
| C6 | 'c' catatonia（SIGTSTP） | 全置 SE_SHUTDOWN | `init.c:1634-1647` | 'm' |
| C7 | 'd' death（SIGTERM） | 全置 SHUTDOWN → 三轮 kill(-1, HUP→TERM→KILL) 各 alarm(10) → ECHILD 提前收工 → 出口恒 's' | `init.c:1661-1701`（DEATH_WATCH=10 :96） | 's' |

**D 会话生命周期次主线**：read_ttys 建会话（06）→ multi_user 启动 getty（09）→ collect_child 重启/摘链（09）→ clean_ttys diff（10）→ death/catatonia 全收（11）。

**E 信号面**：SIGHUP/SIGTERM/SIGTSTP → `transition_handler` 三映射（:1502-1522）；SIGALRM → `alrm_handler` 置 `clang`（:1649-1655）；SIGABRT → `minixreboot` fork `/sbin/shutdown -r`（:517-528）；SIGUSR1 → `minixpowerdown` fork `-p`（:530-541）；致命四信号 → `disaster` _exit(sig)（:504-515）。

### 1.2 序差表

| # | 运行时事实 | 教学序 | 理由 | 回指补偿 |
|---|---|---|---|---|
| 序-1 | 03 的日志设施在一切状态函数之前初始化（B5/B8） | 03 紧跟 02 | 基础设施先行，04 起的失败路径都要引用 stall | 03§6 过渡句 |
| 序-2 | 's→'r→'t 是运行时线性推进 | 04/05/06 按同序排列 | 教学序=状态推进序，无差 | 各篇 §6 过渡链 |
| 序-3 | read_ttys（'t'）运行时调用 `new_session`/`add_session`（07/08 的机制） | 07/08 排在 06 之后 | 结构与 DB 是汇聚机制，06 只讲解析与重建流程（plan I-7 调用点 vs 机制规则） | 06§1.1"机制见 07"声明 |
| 序-4 | sysctl/utmp 调用点散布在 B8/C1/C2/C4/02/06 | 12/13 后置为机制篇 | 调用点文档一行引用，机制归一篇，防散落 | 12/13 与调用点篇互引 |
| 序-5 | `minixreboot`/`minixpowerdown` 在 B7 注册、运行时随时触发 | 14 压轴 | 横切契约面收口 | 01§2.5 调用点表 |
| 序-6 | 'T'/'c'/'d' 在稳态后随时可入 | 10/11 在 09 之后 | 教学序=稳态先立、出口后讲 | 09§6 三出口预告 |

---

## 2. 知识点全集

### 2.1 知识点池总表

**域 A：身份与执行模型（00/01/14）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|---|---|---|---|---|---|---|
| K-001 | init 是 boot 终点与众生祖先（boot_image 最后一项、VM 加载、PM 特判、USR_F 非服务） | 概念 | 存量 | 00§核心点、01§1、14§1 | `table.c:64`；`rs/table.c:28`；`pm/main.c:188-204` | 00/14 共担 |
| K-002 | 非 IPC 服务执行模型（waitpid + 信号驱动状态机，无 SEF/CALLMAP） | 约束 | 存量 | 00§核心点、02§1 | `transition` :624-640 | 00/02 |
| K-003 | 三重出生证明（uid=0/pid=1 身份锚点、setsid 断终端、close 0/1/2 描述符卫生） | 概念 | 存量 | 01§1 | `init.c:242-256,339-341` | 01 主 |
| K-004 | 入口八步（S1-S8 表）与失败策略谱（退出/警告/降级） | 机制 | 存量 | 01§2.1 | `init.c:229-367` | 01 |
| K-005 | BootArgs 解析与 EntryDecision（-s 改首状态、-f 改 rc 模式；console 失败强制单用户；boot 路径 argv 固定故 -s/-f 是运维后门） | 机制 | 存量 | 01§2.3/§3.2-3.4 | `init.c:287-303`；`vm/main.c:346` | 01 |
| K-006 | mfs_dev 三层（快路径/`#if 0` 死段/fork MAKEDEV 慢路径；_exit(11/12) 语义） | 机制 | 存量 | 01§2.4/§3.5 | `init.c:1703-1788` | 01 |
| K-007 | init 是"众所周知的用户进程"（13 项跨服务契约：孤儿收养/Ctrl-Alt-Del/低电/procfs 非服务/调度继承/reboot 停 init） | 接口与协议 | 存量 | 14§2.2 | plan §5.3 全表 | 14 |
| K-008 | 两个 Minix 挂钩（SIGABRT→shutdown -r、SIGUSR1→shutdown -p；信号转进程） | 机制 | 存量 | 14§1/§2.1 | `init.c:517-541`；`keyboard.c:300`；`tps65217.c:226` | 14 |

**域 B：状态机骨架（02/03）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|---|---|---|---|---|---|---|
| K-009 | state_t 函数指针体操与 Rust 枚举替代 | 数据结构 | 存量 | 02§2.1/§3.2 | `init.c:130-131` | 02 |
| K-010 | 七状态字符（大小写敏感即协议：'t' 与 'T' 两状态） | 约束 | 存量 | 02§2.2 | `init.c:133-139` | 02 |
| K-011 | 请求桥模型（handler 只写 `requested_transition`，主循环认领；default 清零语义） | 约束 | 存量 | 02§1/§2.5 | `init.c:1502-1522` | 02 |
| K-012 | handle/delset（SA_NOCLDSTOP 仅 SIGCHLD；`/* XXX SA_RESTART? */` 诚实未决） | 机制 | 存量 | 02§2.3 | `init.c:369-409,384` | 02 |
| K-013 | transition 主循环（先记 runlevel 再推进；current_state 初始 death） | 机制 | 存量 | 02§2.4 | `init.c:624-640,201` | 02 |
| K-014 | SignalState 三位一体（clang/requested/minix 请求）+ 自觉偏离（fork 移出信号上下文，外部行为不变） | 架构演进 | 存量 | 02§3.5 | `signal_state.rs` | 02 |
| K-015 | Signal 枚举与 signum 权威（Minix3 SIGUSR1=30，10 是 SIGBUS——Linux 编号会认错人；P1-5 修真 bug） | 约束 | 存量 | 02§3.3 | `signal.h:52-82`；minix-types signal.rs | 02 |
| K-016 | InitHost 单接缝（A-11：七 trait 收敛；live 诚实 ENOSYS；ScriptHost 剧本宿主） | 架构演进 | 存量 | 01§3.5、02§3.6、03§3.3、99§归宿段 | `host.rs:55` | 01 主声明、各篇引用 |
| K-017 | 日志三级（stall 停 30 秒是给人读屏；warning 不停；emergency 最重；syslog A-3 defer） | 机制 | 存量 | 03§1/§2 | `init.c:95,440-488,278` | 03 |
| K-018 | disaster 遗言协议（_exit(sig) 退出码即信号号→PM 理解为重启；_exit 不冲刷） | 机制 | 存量 | 03§2.4 | `init.c:504-515` | 03 |
| K-019 | 排除项（print_console `#if 0`、badsys 非 minix、session logger NB 诚实缺口） | 约束 | 存量 | 03§2.5 | grep 实证 | 03/99 |

**域 C：启动三站（04/05/06）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|---|---|---|---|---|---|---|
| K-020 | 单用户口令门（条件信任布尔式；空输入=放弃抢修进多用户；错一次重试） | 机制 | 存量 | 04§1/§2.3/§3.1 | `init.c:747-763` | 04 |
| K-021 | ALTSHELL 与双次 exec 兜底（altshell→INIT_BSHELL；失败 emergency+睡 30+_exit(3)） | 机制 | 存量 | 04§2.3/§4 | `init.c:768-783,803-808` | 04 |
| K-022 | 看护循环五结局（WUNTRACED 可见暂停；SIGKILL 静默 sigsuspend；零退出即 FASTBOOT 不看码值） | 机制 | 存量 | 04§2.4/§3.3 | `init.c:825-873` | 04 |
| K-023 | 单用户安全级降级（>0 才降 0） | 机制 | 存量 | 04§2.2 | `init.c:723-725` | 04（机制归 12） |
| K-024 | runcom 黑盒契约（退出码是唯一协议；非零/被杀→回 's'；零→'t'） | 约束 | 存量 | 05§1/§2.3 | `init.c:959-968` | 05 |
| K-025 | autoboot/fastboot 参数（argv[2] 有无；rc 出口重置 AUTOBOOT） | 机制 | 存量 | 05§2.1/§2.4 | `init.c:897-900,1005` | 05 |
| K-026 | chroot 第二遍启动（runetcrc(0)→shouldchroot→runetcrc(1)；两次独立判；did_multiuser_chroot） | 机制 | 存量 | 05§1/§2.4 | `init.c:990-998` | 05（机制归 12） |
| K-027 | rc fork 失败与 single_user fork 失败的差异化（前者睡 30 秒后者不睡——易混淆点已点名） | 事实 | 存量 | 05§4 | `init.c:911-922` | 05 |
| K-028 | 静默重启双条件（catatonia 请求 + SIGTERM 同时成立；文档自称"最易读漏"） | 约束 | 存量 | 05§3 | `init.c:949-957` | 05 |
| K-029 | /etc/ttys 解析三细节（引号是模式开关；状态 token 精确匹配；off 显式清除按序生效；A-6 自有解析器对齐 libc 字段语义） | 机制 | 存量 | 06§3 | `getttyent.c:skip`；`minix3/etc/ttys` 实行 | 06 |
| K-030 | read_ttys 重建策略（先清空再重建；DB 失败双归宿：chroot 过进 'd' 否则回 's'；do_setttyent chroot 感知路径） | 机制 | 存量 | 06§2 | `init.c:1222-1288,1792-1806` | 06 |

**域 D：会话模型与稳态（07/08/09/10）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|---|---|---|---|---|---|---|
| K-031 | init_session 结构九字段与 SE_* 两标志 | 数据结构 | 存量 | 07§2.1 | `init.c:156-169` | 07 |
| K-032 | new_session 四关过滤（OFF/空名/空 getty→NULL；malloc/device/setupargv 失败链） | 机制 | 存量 | 07§2.2 | `init.c:1142-1183` | 07 |
| K-033 | construct_argv 极简分词 + ParsedCommand 双字段（exec_path 与 argv[0] 分离的两种形状） | 数据结构 | 存量 | 07§1/§3 | `init.c:1101-1121` | 07 |
| K-034 | 会话 DB 反向索引（dbopen(NULL)=内存表无持久化；A-1 HashMap 演进；add 静默宽容/find 空语义） | 数据结构 | 存量 | 08§1/§2 | `init.c:1021-1099` | 08 |
| K-035 | 稳态主循环（==0 才升安全级的 -1 语义；waitpid 不带 WUNTRACED 暂停不可见——与救援态相反；防抖睡在子进程） | 机制 | 存量 | 09§3（三处易漏钉） | `init.c:1541-1543,1559,1347-1349` | 09 |
| K-036 | start_getty/chroot/防抖/窗口次序（GETTY_SPACING 5/SLEEP 30/WINDOW_WAIT 3；exec 失败 _exit(8)） | 机制 | 存量 | 09§2 | `init.c:1321-1370,92-94` | 09 |
| K-037 | collect_child 四动作（未知 pid 忽略/清台账/SHUTDOWN 摘链/重启重挂 DB） | 机制 | 存量 | 09§2/§4 | `init.c:1460-1497` | 09 |
| K-038 | setctty 成人礼（setsid/DTR 等待/open/login_tty；失败退出交父回收） | 机制 | 存量 | 09§1/§2 | `init.c:669-689` | 09 |
| K-039 | clean_ttys 存在性标记 diff（PRESENT 清零-置位-收尾关停；序号变化只 warning；n² 自嘲） | 机制 | 存量 | 10§1/§2 | `init.c:1569-1632` | 10 |
| K-040 | 读不到文件=空表=全体退役（live ENOSYS 降级与 C getttyent 无行同效） | 事实 | 存量 | 10§4 | — | 10 |

**域 E：关停与系统交互（11/12/13）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|---|---|---|---|---|---|---|
| K-041 | catatonia 装死语义（只标记不杀，自然衰竭） | 机制 | 存量 | 11§1/§2 | `init.c:1634-1647` | 11 |
| K-042 | death 三轮升级（HUP→TERM→KILL 各 alarm(DEATH_WATCH)；kill(-1) ESRCH 直回；ECHILD 提前收工；杀不完 ps axl 警告；出口恒 's'） | 机制 | 存量 | 11§1/§2/§4 | `init.c:1661-1701,1667` | 11 |
| K-043 | securelevel 能力探测与查询/设置（has 的 ENOENT=不支持；get 失败 -1；set 同值 no-op；A-4 defer） | 机制 | 存量 | 12§2 | `init.c:544-621` | 12 |
| K-044 | init.root 动态 sysctl 节点与 shouldchroot 决策矩阵（A-5 defer） | 机制 | 存量 | 12§2 | `init.c:1811-1900` | 12 |
| K-045 | utmp/utmpx 台账语义（LOGIN/DEAD/RUN_LVL/BOOT 四记录点；get_runlevel 七映射未知回 death；空会话短路=等不到可写 /var；A-2 defer + 文本编码临时态声明） | 接口与协议 | 存量 | 13§1/§3 | `init.c:1372-1458,647-666,1008,1674`；`utmpx.h:57-64` | 13 |
| K-046 | clear_session_logs 两步写（DEAD 进 utmpx、成功才进 wtmpx） | 机制 | 存量 | 13§4 | `init.c:652-658` | 13 |

**域 F：全局（99）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|---|---|---|---|---|---|---|
| K-047 | 常量总表（INIT_PID/INIT_BSHELL/七字符/五超时/dtrtime/RUNLVL_MSG + Rust 落点对照 13 行） | 约束 | 存量 | 99§常量表 | `pathnames.h`/`paths.h`/`init.c:92-96` | 99 |
| K-048 | 十个 C 全局 → DriverState/SignalState 归宿对照（A-11 收敛账） | 架构演进 | 存量 | 99§归宿段、00§实现形态 | `init.c:151-217`；`driver.rs:39` | 99 |
| K-049 | 构建变体编译宏 → feature 取舍（A-7：LETS_GET_SMALL 不实现，余默认全开） | 约束 | 存量 | 99§核心点、plan A-7 | `sbin/init/Makefile` | 99 |
| K-050 | 测试基线与分布（**136**：ttys 16/single_user 13/runcom 13/entry 12/utmp 9/multi_user 9/driver 8/contracts 8/session 6/clean_ttys 6/signal_state 5/shutdown 5/session_db 5/password 5/sysctl 4/state_machine 4/log 4/host 4；**1 failed 见 G-1**） | 测试性质 | **新增** | 各篇 §5.1 写 143（过时） | `cargo test -p minix-init` 实测 | 各篇 §5.1 修正 + 99 |
| K-051 | E-INITSYS 四件套状态（①②③④客户端面已落地 e8be0da5a+6a9614bc6；剩 PM dispatch 臂/sigreturn 桩/E5 通电；P2-6 残余 clippy 32 条全是门控 API 面不删除） | 工具与工程 | **新增**（状态性） | 正文各篇"live 诚实 ENOSYS"散见但无汇总指针 | edge_todo:1071 起；todo §2 | 01/99 状态指针 |
| K-052 | libcrypt 后端缺口（P0-8 残余：DES/MD5/SHA1/bcrypt 四后端缺失时一律拒绝——不会假通过；建议独立 minix-crypt crate 登记 edge） | 事实 | **新增**（todo 有、正文无） | 04§5.1 仅一句带过 | `minix3/lib/libcrypt/` | 04 状态指针 |

### 2.2 统计摘要

- 总条数 **52**：概念 4、机制 30、数据结构 4、接口与协议 2、约束 8、架构演进 2、事实 3、工具与工程 1（跨类按主类型计）。
- 来源：存量 49、新增 3（K-050/K-051/K-052，均为状态/基线性知识点）。
- 主讲述点重复：见 §3.3。

---

## 3. 覆盖审计

### 3.1 主题全集与来源

四路：① C 符号全集（init.c 46 函数——plan §5.2 表 2026-08-16 逐项回归实证，本次抽验 22 个行号全对；44 生效 + print_console `#if 0` + badsys 非 minix 两排除）；② OS 通用概念（PID 1 语义、会话/控制终端、单用户-多用户 runlevel、守护进程卫生）；③ 非 C 制品（清单三）；④ 边界契约（plan §5.3 十三项 + edge E-INITSYS/E-CMDSYSFACE/E-ISBOOT/E5）。

### 3.2 覆盖缺口表

| # | 缺口 | 证据 | 建议 | 处理 |
|---|---|---|---|---|
| G-1 | **实测 1 个测试失败 + 基线漂移**：`cargo test -p minix-init` = 135 passed / **1 FAILED**（host.rs:651 `register_handlers` 期望 `Ok(())` 得 `Err(EIO)`）；todo 基线 143（2026-09-18）→ 当前 136。根因：init 侧 6a9614bc6（09-18）钉的"E-INITSYS ① 真实 register_handlers 宿主回环返 Ok"期望，被 09-19 当天的 E-SYSCALL-SIGN/edge2 传输诚实化 lane（0cca4247d"transport 失败符号短路,宿主 *_via 诚实化"、df145274c）改写——**跨 stage 活回归**，非 init 自身缺陷 | §0.2 命令输出；`git log` 时序 | 归共享 lane 裁决：init 侧测试期望按 E-SYSCALL-SIGN 新契约重钉（宿主回环现在诚实 EIO 是**更诚实**的行为，测试应跟随而非回退传输层）；G-3 的基线刷新在失败清零后执行 | **采纳（本轮最高优先）** |
| G-2 | **系统性锚点符号错位**：15 篇的定位行/参见行"（LNN，工具生成）"锚点**行号全对、符号名全错**——每个都指向同一文件里的另一个函数（`make_utmpx（L229）`=main、`setsecuritylevel（L624）`=transition、`print_console（L440）`=stall、`setctty（L694）`=single_user、`runetcrc（L974）`=runcom、`setupargv（L1222）`=read_ttys、`runcom（L1021）`=start_session_db、`transition_handler（L1528）`=multi_user、`multi_user（L1569）`=clean_ttys、`clean_ttys（L1634）`=catatonia、`disaster（L517）`=minixreboot、`minixpowerdown（L544）`=has_securelevel、`start_getty（L1372）`=session_utmpx、`transition（L647）`=clear_session_logs、`death（L1703）`=mfs_dev、`INIT_PATH（L130）`=typedef 区）。成因与 06/08 两 stage 的锚点工具产物同源 | §0.2 抽验（22 个行号 OK） | B 相统一改为**纯行号锚**（`init.c:229-367`）或"正确符号（N+1 行名）"；本 stage 缺陷密度最高（每篇 2-6 处） | **采纳（工作量最大项）** |
| G-3 | **各篇 §5.1 基线行过时**：除 01 写"13 个通过"（篇内口径，仍对）外，02~14 全部写"143 个通过（全 crate 口径，截至 2026-09-18）"——实测 136 且 1 failed | §0.2 | G-1 清零后统一刷新为当期实测数（建议保留"截至日期"格式） | 采纳 |
| G-4 | **02 篇格式缺陷**：:131 行 `### 3.5 决策四：SignalState——异步写状态的三位一体` 标题粘连在 §3.4 段落末尾（缺空行换行），markdown 渲染后 §3.5 标题消失 | grep 定位 :131 | 段末与标题间补空行 | 采纳 |
| G-5 | **plan.md 状态过时**（记录不改）：§6.1 十五篇全标 pending（实际全部成文且经审计轮）；§3.5 "0 passed (stub)"（实际 136）；§4 A-11 行已更新但 §6.1 未跟 | plan §6.1/§3.5 对照现状 | plan 属参考材料；B 相交付说明记录对账 | 记录 |
| G-6 | **K-052（libcrypt 缺口）正文承载不足**：04 篇仅在 §5.1 清单行尾注"crypt 后端见 12/ARCH A-12"，12 篇无此内容（A-12 是 plan 的 ARCH 编号，12 篇正文只写 A-4/A-5）；"后端缺失一律拒绝——不会假通过"这一安全语义只活在 todo | 04§5.1、12 篇全文对照 | 04 §3.1 补一句状态指针（后端缺失语义 + 建议独立 crate 挂 edge），不展开实现 | 采纳（轻） |
| G-7 | **00 篇状态图为文字链**：状态机主线是行内文字（`'s'→'r'→'t'→'m' ↔ 'T'/'c'/'d'`），无 ASCII 图；plan §1.3 有完整图但 00 未内嵌（与 08-stage-is 00 同型的轻症） | 00:12 对照 plan §1.3:31-53 | 00 契约：内嵌 ASCII 状态图（含七状态与转移触发源），保留文字版作导语 | 采纳（soft） |
| G-8 | **代码注释零文档引用**：`os/commands/sbin/init/src/` 无一处引用 09-stage-init 篇名（姊妹 stage 的 main.rs 均引 01 篇）。现有模块头注释引 plan/design 行号 | `rg '09-stage-init' os/commands/sbin/init/src/` 零命中 | 非缺陷（README.md 承担状态页职责且模块注释有 design 锚）；可选补一条 main.rs → 00/01 引用对齐姊妹惯例 | 记录（可选） |
| G-9 | **edge 状态指针分散**：E-INITSYS 四件套进度（①②③④ 已落地、余 PM 臂/sigreturn/E5）只活在 todo §2 表；正文各篇"live 诚实 ENOSYS"散见但读者无法一眼看到"哪半已活" | todo §2 对照 01/02/12/13 正文 | 01 或 99 加一行汇总状态指针（K-051） | 采纳（轻） |

### 3.3 重复主题表（主讲述点裁决）

| 主题 | 出现位置 | 主讲述点 | 其余处理 |
|---|---|---|---|
| 非 IPC 执行模型 | 00、02§1、14§1 | 00 | 02/14 一段引用 |
| 请求桥模型（handler 只写请求） | 02§1、14§1（挂钩同原则） | 02 | 14 引用 |
| securelevel 机制 | 12（主）、04§2.2/05 调用点 | 12 | 调用点一篇一行 |
| chroot 机制 | 12（主）、05§1/§2.4、06§1 路径感知 | 12 | 调用点引用 |
| utmp 台账 | 13（主）、02§2.4/05§2.4/08§2 挂钩点 | 13 | 挂钩点一行 |
| collect_child 回收语义 | 09（主）、10§1（摘链归属声明） | 09 | 10 引用 |
| FASTBOOT 语义 | 05（主）、04 出口 | 05 | 04 引用 |
| InitHost 接缝 | 01§3.5（主声明）、02§3.6/03§3.3/12§3 收敛注记 | 01 | 各篇一句 |
| 状态字符表 | 02§2.2（主）、13 get_runlevel、99 常量表 | 02 | 13/99 引用 |

### 3.4 越界主题表

| 越界描述 | 所在 | 裁决 |
|---|---|---|
| 06 篇深读 libc getttyent 实现（skip 状态机/scmp/vcmp） | 06§3 | 合规边界：A-6 自有解析器的对齐目标是 libc 字段语义而非臆测格式，属契约必需；libc 实现主权在 libc 移植层；保持 |
| 14 篇汇总 13 项跨服务契约（PM/VM/kernel/tty/power/procfs） | 14§2.2 | 合规：契约表即本篇主体，对端实现归各 stage；保持 |
| 09 篇引 PM waitpid 语义面 | 09§3 | 合规：A-9 依赖声明；保持 |
| 04 篇 catch_unwind 测发散的手法说明 | 04§3.3 | 合规：测试策略自述；保持 |

### 3.5 非 C 主题逐项回答

| 主题 | 在哪讲 / 为什么不在本 stage |
|---|---|
| 链接与加载 | VM exec_bootproc 加载语义归 01-stage-kernel/09；本 stage 持证据面（00/01/14） |
| 镜像与内存布局 | 不在本 stage（普通用户程序无自定义段） |
| 汇编入口与陷阱进入 | 系统调用面走 minix-rt/minix-sys（E-INITSYS 客户端面已落地）；trap 层归 14-stage-runtime |
| 引导链与引导协议 | boot_image 位置与 argv 契约在本 stage（00/01/14），协议本体归 kernel stage |
| 启动装配 | B 八步即装配（01）；Rust main 接线（01§4） |
| 构建与工具链 | A-7 编译宏 → feature 取舍（99/plan A-7）；Makefile 本体 WONTFIX |
| 跨模块接口与线格式 | utmp/utmpx 记录契约（13，A-2 defer 已钉语义）+ shutdown 命令方言（14） |
| 错误路径 | 三级日志 + disaster（03）+ 各状态结局分类器（04/05/09/11） |
| 关闭与退出 | catatonia/death（11） |
| 并发与同步 | 单线程 + SignalState 原子位（02；自觉偏离已声明） |
| 测试基建 | 136 内联测试 + ScriptHost 剧本宿主 + 全图 boot-chain 测试（各篇 §5）；真机归 E5 |

---

## 4. 新目录

### 4.1 新篇章总表（编号不变，15 篇）

> **裁决**：编号即状态机推进序，四条硬标准满足（§9 G3/G4）。重建工作 = 2 处高优先修正（G-1 回归跟踪、G-2 锚点）+ 基线刷新 + 轻量补注。**不重排、不重编号、不拆分、不合并、不新建篇章。**

| 编号 | 标题 | 一句话定位 | 分组 | 操作 |
|---|---|---|---|---|
| 00 | INIT 整体概览 | boot 终点/非服务器/状态机主线/导航 | 阶段 0 总览 | 保持+补图（G-7） |
| 01 | 入口与进程身份 | 八步出生流程与三重证明 | 阶段 1 入口 | 保持+修正（G-2/G-9） |
| 02 | 状态机骨架与信号转换 | 七状态/请求桥/注册表 | 阶段 2 骨架 | 保持+修正（G-2/**G-4**） |
| 03 | 日志三件套与致命信号 | 停 30 秒给人读屏 + disaster | 阶段 2 | 保持+修正（G-2） |
| 04 | 单用户抢修态 | 口令门+ALTSHELL+看护五结局 | 阶段 3 启动序列 | 保持+修正（G-2/G-6） |
| 05 | 运行启动脚本 | /etc/rc 黑盒契约+chroot 两遍 | 阶段 3 | 保持+修正（G-2） |
| 06 | 读终端表与会话重建 | 自有解析器对齐 libc 语义 | 阶段 3 | 保持+修正（G-2） |
| 07 | 会话结构与生命周期 | init_session 九字段+分词 | 阶段 4 会话模型 | 保持+修正（G-2） |
| 08 | 会话数据库 | pid 反向索引（A-1） | 阶段 4 | 保持+修正（G-2） |
| 09 | 多用户稳态 | 全启动+回收循环+三易漏点 | 阶段 5 运行态 | 保持+修正（G-2） |
| 10 | 重读终端表 | 存在性标记 diff | 阶段 5 | 保持+修正（G-2） |
| 11 | 假死与关机 | catatonia/death 三轮 | 阶段 6 关停 | 保持+修正（G-2） |
| 12 | 安全级别与新根 | securelevel/init.root（defer 契约） | 阶段 7 系统交互 | 保持+修正（G-2） |
| 13 | 会话日志账本 | utmp/utmpx 四记录点（defer 契约） | 阶段 7 | 保持+修正（G-2） |
| 14 | 对外契约 | 两个挂钩+13 项跨服务契约 | 阶段 7 | 保持+修正（G-2） |
| 99 | 全局概念总表 | 常量/路径/全局归宿/状态指针 | 全局查询 | 保持+修正（G-3/G-9） |

### 4.2 阅读路径

- **主线（状态机首次推进序，全读）**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 99。
- **支线/可跳读**：只关心稳态运行的读者 00 → 02 → 09 直达；查常量/全局归宿随时跳 99；12/13 是 defer 契约篇，实现期读者可后置。
- **并行体声明**：无严格并行体——七状态是循环图，教学序取首次推进链，'T/'c/'d 支线在 09§6 预告后于 10/11 展开，符合"主线与支线分离"。

---

## 5. 每篇契约

> B 相执行说明：15 篇全部【保持+修正】型——**不重写正文**，执行各自修正清单（fix-guard 逐条）。全局性修正（G-1 回归、G-3 基线、G-2 锚点）在各篇清单中只写差异部分，共同做法在此声明一次：
> - **锚点修法（G-2）**：定位行/参见行的 `init.c:<错符号>（LNN，工具生成）` 一律改为 `init.c:NNN-NNN`（纯行号）或 `init.c:<正确符号>,NNN`（N+1 行函数名）；跨文件锚点（pm/main.c 等）逐条复核。
> - **基线修法（G-3）**：G-1 失败清零后，各篇 §5.1 的全 crate 口径行统一刷为当期实测（格式保留"截至日期"）。

### 00-init-overview【保持+修正】

- 讲什么：K-001/K-002/K-048 概览面。前置：无。后置：全部。
- 修正清单：
  1. **G-7**：§核心点状态机行下内嵌 ASCII 状态图（七状态节点 + 触发源标注：'s →fork shell、'r →/etc/rc、't →/etc/ttys、'm 稳态 waitpid、'T←SIGHUP、'c←SIGTSTP、'd←SIGTERM；'d→'s、'T/'c→'m、's→'r→'t→'m），以 plan §1.3:31-53 为底稿；
  2. **G-9**：补一行 edge 状态指针（E-INITSYS ①②③④ 客户端面已落地；PM dispatch 臂与真机通电余）；
  3. 定位行的两处工具生成锚点改纯行号（G-2）。
- 验收标准：ASCII 图含全部七状态与转移触发；正文无委托 plan 的图引用。

### 01-init-main-entry【保持+修正】

- 讲什么：K-003~K-006/K-016。前置：00。后置：02/05/12/14。
- 修正清单：
  1. **G-2**：定位行 `init.c:make_utmpx（L229）`→`init.c:229-367（main）`；§7 `init.c:death（L1703）`→`init.c:1703-1788（mfs_dev）`；`pm/main.c:sef_cb_init_fresh（L188）` 复核（该行段在 PM 初始化区，按现文件复核符号）；
  2. **G-9**：§3.5 的"live 宿主诚实返回 ENOSYS"处补半句现状：信号安装面已随 E-INITSYS ① 接真实封装（宿主回环行为以 host.rs 测试契约为准）。
- 验收标准：八步表锚点复核；G-1 相关的 host 语义表述与修后契约一致。

### 02-init-state-machine【保持+修正】

- 讲什么：K-009~K-016。前置：01。后置：04~11/13。
- 修正清单：
  1. **G-4**：:131 段末与 `### 3.5` 标题间补空行（修复标题渲染）；
  2. **G-2**：定位行 `init.c:setsecuritylevel（L624）`→`init.c:624-640（transition）`；§7 `init.c:INIT_PATH（L130）`→`init.c:130-139（state_t/七状态字符）`；
  3. §4.1 模块树补 `driver.rs`/`signal_state.rs` 现状（§3.4 已述 driver，模块树行未列——对齐 todo Wave3 后实况）。
- 验收标准：`### 3.5` 独立成行渲染为标题；七字符/signum 权威锚点不变。

### 03-init-logging-failure【保持+修正】

- 修正清单：**G-2**（定位行 `print_console（L440）`→`init.c:440-455（stall）`；§7 参见同行修正）；**G-3** 基线行刷新。
- 验收标准：三级日志/排除项锚点不变。

### 04-init-single-user【保持+修正】

- 修正清单：
  1. **G-2**：定位行 `setctty（L694）`→`init.c:694-877（single_user）`；§7 参见同行修正；
  2. **G-6**：§3.1 口令门段补一句后端现状指针："crypt 后端（DES/MD5/SHA1/bcrypt）尚未落地，缺失即一律拒绝——管理员代价是 ^D 直接进多用户，不会得到假通过；独立 crate 候选挂 edge（todo P0-8）"；
  3. **G-3** 基线刷新。
- 验收标准：五结局次序锚点不变；口令门布尔式与 `password.rs` 一致。

### 05-init-runcom【保持+修正】

- 修正清单：**G-2**（定位行 `runetcrc（L974）`→`init.c:974-1019（runcom）`；§7 `single_user（L879）`→`init.c:879-969（runetcrc）`）；**G-3**。
- 验收标准：五归宿表、双条件静默重启、fork 失败差异点锚点不变。

### 06-init-read-ttys【保持+修正】

- 修正清单：**G-2**（定位行 `setupargv（L1222）`→`init.c:1222-1288（read_ttys）`；§5 两处 `getttyent.c:skip（L184/L188）` 复核行号；§7 同行修正）；**G-3**。
- 验收标准：三解析细节锚点不变；golden 测试引用（minix3/etc/ttys 实行）有效。

### 07-init-session-model【保持+修正】

- 修正清单：**G-2**（定位行 `session_t（init.c:init_session）` 补行号 `init.c:156-169`——struct init_session typedef 行已验证；`new_session` 等行号复核）；**G-3**。
- 验收标准：九字段表与 SE_* 位值（SHUTDOWN 0x1/PRESENT 0x2）不变。

### 08-init-session-db【保持+修正】

- 修正清单：**G-2**（定位行 `runcom（L1021）`→`init.c:1021-1036（start_session_db）`；§7 参见同行修正）；**G-3**。
- 验收标准：A-1 论证与 SessionDb 接缝外地位声明不变。

### 09-init-multi-user【保持+修正】

- 修正清单：**G-2**（定位行 `transition_handler（L1528）`→`init.c:1528-1567（multi_user）`；§7 `clear_session_logs（L669）`→`init.c:669-692（setctty）`）；**G-3**。
- 验收标准：三处易漏点（无 WUNTRACED/==0 才升/子进程内防抖）锚点不变。

### 10-init-clean-ttys【保持+修正】

- 修正清单：**G-2**（定位行与 §7 `multi_user（L1569）`→`init.c:1569-1632（clean_ttys）`）；**G-3**。
- 验收标准：diff 四动作与空文件语义不变。

### 11-init-shutdown【保持+修正】

- 修正清单：**G-2**（定位行 `clean_ttys（L1634）`→`init.c:1634-1647（catatonia）`；§7 参见同行修正）；**G-3**。
- 验收标准：三轮序列/ESRCH/ECHILD 锚点不变。

### 12-init-sysctl-interaction【保持+修正】

- 修正清单：**G-2**（定位行 `minixpowerdown（L544）`→`init.c:544-566（has_securelevel）`；§7 参见同行修正）；**G-3**。
- 验收标准：A-4/A-5 defer 契约与能力探测语义不变。

### 13-init-utmp【保持+修正】

- 修正清单：**G-2**（定位行 `start_getty（L1372）`→`init.c:1372-1381（session_utmpx）`；§7 `transition（L647）`→`init.c:647-666（clear_session_logs）`）；**G-3**。
- 验收标准：四记录点/空会话短路/文本编码临时态声明不变。

### 14-init-external-contracts【保持+修正】

- 修正清单：**G-2**（定位行 `disaster（L517）`→`init.c:517-528（minixreboot）`；§2.2 表两处工具生成锚点规范化）；**G-3**。
- 验收标准：13 项契约表锚点复核；SIGBUS 非挂钩测试语义不变。

### 99-init-global-concepts【保持+修正】

- 修正清单：
  1. **G-3**：补测试总账（G-1 清零后当期实测 + 模块分布，K-050 分布表可整体收录）；
  2. **G-9**：§归宿段后补"运行时现状"三行（E-INITSYS 进度、clippy 门控 API 面残余不删除、libcrypt 后端缺口）——K-051/K-052 的汇总落点；
  3. **G-2**：头两处工具生成锚点（`boot_image（L64）`/`boot_image_priv（L28）`）按真符号复核（`table.c:64` 的数组项与 `rs/table.c:28` 的登记行符号名复核）。
- 验收标准：常量表 13 行与 Rust 落点对账一致；归宿段与 DriverState/SignalState 实况一致。

---

## 6. 变更表

| 操作 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向/来源 |
|---|---|---|---|---|---|---|
| N-0 | **跨 stage 回归跟踪** | `os/commands/sbin/init/src/host.rs:651` 测试 | 原位（代码侧） | G-1：E-SYSCALL-SIGN 诚实化后 pinned 期望过期 | K-050 | 归共享 lane 裁决（建议测试随新契约重钉）；文档侧不先动 |
| N-1 | 修正 | 00§核心点、定位行 | 原位 | G-7/G-9/G-2 | K-002 | plan §1.3 图为底稿 |
| N-2 | 修正 | 01 定位/§7/§3.5 | 原位 | G-2/G-9 | K-004~K-006 | — |
| N-3 | 修正 | 02:131、定位/§4.1/§7 | 原位 | **G-4**/G-2 | K-011~K-016 | — |
| N-4 | 修正 | 03 定位/§7/§5.1 | 原位 | G-2/G-3 | K-017~K-019 | — |
| N-5 | 修正 | 04 定位/§3.1/§5.1 | 原位 | G-2/**G-6**/G-3 | K-020~K-023/K-052 | todo P0-8 语义 |
| N-6 | 修正 | 05 定位/§7/§5.1 | 原位 | G-2/G-3 | K-024~K-028 | — |
| N-7 | 修正 | 06 定位/§5/§7/§5.1 | 原位 | G-2/G-3 | K-029/K-030 | — |
| N-8 | 修正 | 07 定位/§5.1 | 原位 | G-2/G-3 | K-031~K-033 | — |
| N-9 | 修正 | 08 定位/§7/§5.1 | 原位 | G-2/G-3 | K-034 | — |
| N-10 | 修正 | 09 定位/§7/§5.1 | 原位 | G-2/G-3 | K-035~K-038 | — |
| N-11 | 修正 | 10 定位/§5.1 | 原位 | G-2/G-3 | K-039/K-040 | — |
| N-12 | 修正 | 11 定位/§7/§5.1 | 原位 | G-2/G-3 | K-041/K-042 | — |
| N-13 | 修正 | 12 定位/§7/§5.1 | 原位 | G-2/G-3 | K-043/K-044 | — |
| N-14 | 修正 | 13 定位/§7/§5.1 | 原位 | G-2/G-3 | K-045/K-046 | — |
| N-15 | 修正 | 14 定位/§2.2/§5.1 | 原位 | G-2/G-3 | K-007/K-008 | — |
| N-16 | 修正 | 99 常量表/归宿段 | 原位 | G-3/G-9/G-2 | K-047~K-051 | — |
| N-17 | 对账（不改） | plan.md §6.1/§3.5 | — | G-5：状态表/基线过时；plan 属参考材料 | — | B 相交付说明记录 |

**未执行的操作类型**：重排/拆分/合并/新建/归档均 0 处。draft/archive/README 维持现状。

---

## 7. 缺漏新篇（非 C 主题逐项落实）

> §3.5 已逐项回答；本 stage **无需新建篇章**。

| 主题 | 裁决 | 承载位置 | 验收 |
|---|---|---|---|
| 链接与加载/镜像布局 | 不在本 stage | 00/01 证据面 | 已有 |
| 汇编入口与陷阱进入 | 不在本 stage（minix-rt/minix-sys 面） | 01/99 状态指针 | G-9 补注 |
| 引导链与引导协议 | 证据在本 stage，本体不在 | 00/01/14 | 已有 |
| 启动装配 | 在本 stage | 01 | 已有 |
| 构建与工具链 | A-7 feature 取舍 | 99/plan A-7 | 已有 |
| 跨模块接口与线格式 | utmp 契约 + shutdown 方言 | 13/14 | 已有 |
| 错误路径 | 在本 stage | 03/04/05/09/11 | 已有 |
| 关闭与退出 | 在本 stage | 11 | 已有 |
| 并发与同步 | 在本 stage（单线程+原子位，自觉偏离已声明） | 02 | 已有 |
| 测试基建 | 在本 stage（剧本宿主+全图测试）；真机归 E5 | 各篇 §5 + 99 | 99 补总账（G-3） |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表

> 编号不变 ⇒ 篇间锚点零迁移。变化全部为原位修正：

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 断链风险 |
|---|---|---|---|---|
| 全部 15 篇定位行/参见行 | `init.c:<错符号>（LNN，工具生成）`（行号对、符号错） | 纯行号或正确符号锚 | 原样替换（G-2，约 35+ 处） | 无（这些符号本就解析失败；行号保留） |
| 02:131 | `### 3.5` 标题粘连 | 独立标题行 | 原样替换（G-4） | 无 |
| 各篇 §5.1 | "143 个通过" | G-1 清零后当期实测 | 原样替换（G-3） | 无 |
| 00§核心点 | 文字版状态链 | 内嵌 ASCII 图 | 扩写（G-7） | 无 |
| 04§3.1 | crypt 后端一句话 | 补状态指针 | 扩写（G-6） | 无 |
| 01§3.5/99 | live ENOSYS 散述 | 补汇总指针 | 扩写（G-9） | 无 |

### 8.2 引用迁移表

| 引用方 | 旧引用 | 新目标 | 验证方式 |
|---|---|---|---|
| `edge_todo.md`（4 处） | `09-stage-init` 目录/`09-stage-init 首轮架构扫描` | 不变 | `rg -c '09-stage-init' edge_todo.md` 前后一致 |
| `00-master-plan/README.md`（2 处） | 目录名 | 不变 | 目录名不动 |
| `04-stage-pm/doc_rerank_deepseek.md`（2）、`edge2/3.md`、`10/11/12-stage plan` 各 1 | 目录级 | 不变 | 同上 |
| 篇内互引 ≈68 处 | `NN-*.md` | 不变 | 重建后 `rg -c` 复跑计数不减 |
| 代码注释 | 无（0 处，G-8 记录） | — | — |

### 8.3 断链成本摘要

- **本蓝图方案（保编号）**：受影响外部引用 **0**；代码注释 **0**；篇间互引 **0**。
- **反事实（重编号/重排）**：篇内互引 ≈68 + 外部 8 文件 ≈18 处 ≈ **86 处**；热点 09（10 次被引）、02（8）、05（7）。收益为零——现结构即状态机推进序。维持不动。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**：00 无前置；01→00；02→01；03→02；04→02,03；05→04,02,03；06→02,05；07→06；08→07；09→06/07/08；10→07,09；11→02,09；12→01,04/05/09（调用点反向引用不构成阅读前置）；13→02,07；14→01,02,09/11；99→全部。指向更早编号或跨 stage 主权文档，**通过**。（12/13 被调用点篇前置引用属 plan I-7"调用点 vs 机制"规则的机制侧归属，调用点文档自带一行移交声明，不产生阅读依赖。）
2. **依赖图检查**：链式偏序 00→01→02→03→{04→05→06→07→08→09→10→11}→{12,13,14}→99，无环，**通过**。
3. **覆盖率检查**：§2 池 52 条全部有去向（49 条原篇保留/修正、3 条新增落 04/99/状态指针）；删除项 0（print_console/badsys/_PATH_SLOGGER/INIT_MOUNT_MFS/LETS_GET_SMALL 五排除项已由 03/99/plan §5.4 以"排除"身份承载）；新增 3 条全部带实测/edge 锚点，**通过**。
4. **断链成本**：§8.3——方案内 0 断链；反事实 ≈86 处已列热点，**通过**。

### 9.2 自检门逐门结果

| 门 | 结果 | 证据 |
|---|---|---|
| G1 C 真序逐条可核对 | **通过** | 抽 10 条：229=main、624=transition、440=stall、694=single_user、1021=start_session_db、1528=multi_user、1634=catatonia、1703=mfs_dev（全部 sed 到声明行+1 验证函数名）；92-96 五常量、133-139 七字符、156-169 struct init_session（sed 原文）；plan §5.3 的 `table.c:64`/`rs/table.c:28`（2026-08-16 回归 review 实证 + 本轮目录级复核） |
| G2 知识点池完整 | **通过** | init.c 46 函数全映射（plan §5.2 回归实证，本次 22 锚点抽验一致）；pathnames.h/paths.h/四个 libc 头/getttyent/libcrypt 全部入池或显式排除；非 C 制品 10 类逐项有归属 |
| G3 前向引用为零 | **通过** | §9.1 第 1 条 |
| G4 依赖图无环 | **通过** | §9.1 第 2 条 |
| G5 覆盖率 100% | **通过** | §9.1 第 3 条；明确删除项 0 |
| G6 拆合去向/新建来源 | **通过** | 拆分/合并/新建均 0；17 条操作全部写明事实源（实测基线、git 时序、todo 状态、plan 底稿） |
| G7 契约七要素齐全 | **通过** | 15 份契约均为保持型，含定位/讲什么/不讲什么（沿正文既有边界声明）/前置/后置/事实底线/修正清单+验收标准；共同修法（锚点/基线）声明一次避免 15 份重复 |
| G8 迁移表覆盖 | **通过** | §8.1 覆盖全部变化节；§8.2 覆盖外部与篇间引用（全部 0 迁移） |
| G9 事实断言有锚点 | **通过** | 抽 10 条：1 failed 实测（cargo 输出）、回归时序（git log 三提交日期）、136 分布（rg -c 计数）、22 锚点行号（sed 验证）、错符号实样（230/625/441/695 行原文）、常量五行（sed 92-96）、struct init_session（sed 156-169）、02 格式缺陷（grep :131 原文）、E-INITSYS 状态（todo §2/edge:1071）、外部引用目录级（rg -c 扫描）。推测项：无；待复核项：pm/main.c:188 区符号名（写入 01 契约修正清单） |

### 9.3 结论与待用户裁决的问题

**结论**：本蓝图已完成，可交付 B 相：15 篇全部保持+定向修正（合计 22 条修正项，全部带锚点与验收标准），0 篇重写，0 处结构变动。工作量排序：G-2 锚点（约 35+ 处机械替换，最大宗但零风险）> G-1 回归跟踪（跨 stage，建议共享 lane 裁决后文档随动）> G-3 基线 > 其余轻项。

**待用户裁决**：

1. **"保编号、修锚点、追基线"方案是否采纳**（glm 建议采纳）：本 stage 是三个已审 stage 中最新的成稿（昨夜审计轮刚收口），唯一实质缺陷是锚点工具符号错位（历史工具产物，与 06/08 同源）与当天发生的跨 stage 测试回归——都不构成重建理由。若汇总轮裁定全量重写，本蓝图池与契约可直接作任务书。
2. **G-1 失败的修复归属**：`host.rs:651` 的 pinned 期望与今天 E-SYSCALL-SIGN 诚实化冲突。glm 建议：测试随新契约重钉（诚实 EIO 是更好的行为），由 edge2/E-SYSCALL-SIGN lane 的下一轮顺手处置并在本 stage todo 记一行；**不建议**为保 143 基线回退传输层行为。
3. **G-8（代码注释零文档引用）**：是否按姊妹 stage 惯例补一条 main.rs → 00/01 引用。glm 建议补（一行成本，惯例对齐），但不强制。
4. **plan.md 状态刷新**（G-5）：glm 建议维持"参考材料不改"惯例（与 06/08 两轮同一裁决），由 B 相交付说明记录。

---

*（蓝图完。执行者 glm，2026-09-19，基线 commit d6ecd22ca。本文件是 09-stage-init 目录内唯一的 `_glm` 产物。）*
