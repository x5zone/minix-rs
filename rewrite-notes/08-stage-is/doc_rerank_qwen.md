# 08-stage-is 文档重建蓝图（qwen）

## 0. 元数据

```text
执行者   = qwen
日期     = 2026-09-19
目标目录 = rewrite-notes/08-stage-is/
仓库根   = /home/xzhao/github/minix-rs
当前提交 = 40dfbbeb9（2026-09-19，git log -1 实测）
任务     = R 相·重建蓝图：只产出本文件，不改任何正文。
```

用户的三条补充约束（原文转述，全文生效）：

1. 单篇行数上限是**软约束**：一个极复杂概念独占一篇、长达 3000 行可接受；但一般应控制长度以利阅读。
2. 旧文档数量**不是约束**：可增加或减少篇数。
3. 本任务的目标是**修正旧文档的问题、必要时重建**；旧文档可能本身含错误或未经审阅，**第一真相源是 `minix3/` C 源码 + `os/` Rust 代码**，不是旧文档。

### 0.1 范围声明

| 类别 | 内容 |
|------|------|
| 算文档（进入重建对象） | `00-is-overview.md` ~ `10-is-dump-vm.md`、`99-is-global-concepts.md` 共 12 篇 |
| 算参考材料（只取料不保形） | `plan.md`（340 行）、`todo.md`（271 行，V1 轮 + Fix #1~#8）、`draft/`（9 个素材文件） |
| 范围外（不读） | `doc_rerank_deepseek.md`、`doc_rerank_glm.md`（他人产物，按约束未读取）；`.design/`、`tmp_design_and_todo/`（项目规范禁止引用，本蓝图零引用） |

### 0.2 文档清单（编号 / 行数 / 头部声明要点）

| 篇 | 行数 | 头部声明（讲什么 / 前置 / 边界） |
|----|------|----------------------------------|
| 00-is-overview | 60 | 阶段概览：IS 定位、启动主线转述 plan、文档导航；正文自标 pending 最小骨架 |
| 01-is-init-main | 707 | main.c 全文件 + SEF 生命周期 + Rust 生命周期骨架；文末含"接线（S23 片 1/2）"节 |
| 02-is-fkey-contract | 572 | F-key 观察者协议：三层编号、三命令、libsys 客户端、TTY 侧状态机（只契约不实现） |
| 03-is-dump-dispatch | 360 | hooks 表 16 项、pressed 双检、do_fkey_pressed、key_name、mapping_dmp、分派决策 |
| 04-is-data-acquisition | 492 | 五条取数通道：sys_getinfo / diagctl / kerninfo 直读 / getsysinfo / vm_info |
| 05-is-dump-kernel | 323 | 内核域 8 转储 + 共享地基（三表/LINES/两宏）+ 三编码器 + PageCursor |
| 06-is-dump-pm | 191 | PM 域：mproc_dmp + sigaction_dmp + flags_str 11 位 + PmCursor |
| 07-is-dump-vfs | 177 | VFS 域：fproc_dmp + dtab_dmp + 双快照 + blocked_on 枚举 |
| 08-is-dump-rs | 153 | RS 域：rproc_dmp + s_flags_str 双源 + IN_USE 过滤 |
| 09-is-dump-ds | 174 | DS 域：data_store_dmp + 四类型行格式 + 游标三形状对照 |
| 10-is-dump-vm | 209 | VM 域：vm_dmp 双游标批机 + print_region 折叠状态机 + 三快照 |
| 99-is-global-concepts | 58 | 全局收口：常量权威 / 错误码 / 执行模型 / 排除项；正文自标 pending |

结构性观察（B 相改写时必须处理的通病，逐条有实测位置）：

- **补丁注记堆积**：05~10 各篇 §3 末嵌有"V1 执行轮更新（2026-09-15）"块（如 `10-is-dump-vm.md:137`），03 有 §4.4"双文档同步（02 修订 + 01 行文）"（`03-is-dump-dispatch.md:309`），叙事非线性——正文仍在讲"体延后（A-6）"，翻转事实另起补丁块。
- **重复小结**：06 `:32-36`、10 `:31-35` 各有一份"本章小结"逐字重复两遍（编辑事故）。
- **测试台账陈旧**：05~10 各篇 §5.3 声称"86 passed（截至 2026-09-04）"，todo.md 修复轮基线 106（2026-09-15），S23 接线后又增——`grep -c '#\[test\]' os/servers/is/src/*.rs` 实测 125（2026-09-19）。每篇各自记数必然漂移。
- **锚点被工具生成污染**：见 §3.4 勘误表（02 §2.6、03 §2.5 的标题锚指向错误符号）。
- **压缩黑话密集**（"三形同效""点例""缝""臂"级简写），与 style-bible 冲突；本蓝图不逐例列举，B 相按契约全文重写自然消除。

### 0.3 C 源码清单（路径修正 + 文件 ↔ 现有文档映射）

**路径勘误**：本 stage C 源码实际位于 `minix3/minix/servers/is/`（plan 与旧文档以 `minix3/` 相对根略写为 `servers/is/`；首次探测 `minix3/servers/is/` 不存在，`find` 实测确认）。keymap.h 实际位于 `minix3/minix/include/minix/keymap.h`（`F1 (0x10 + EXT)`，:93）。

| 文件 | 行数 | 内容 | 现有文档 |
|------|------|------|----------|
| main.c | 148 | 主循环 + SEF 回调 | 01 |
| dmp.c | 132 | hooks 表 / map_unmap_fkeys / do_fkey_pressed / key_name / mapping_dmp | 02/03 |
| dmp_kernel.c | 396 | 内核域 8 转储 + 共享地基 | 05 |
| dmp_pm.c | ~110 | mproc_dmp / sigaction_dmp / flags_str | 06 |
| dmp_fs.c | ~85 | fproc_dmp / dtab_dmp | 07 |
| dmp_rs.c | ~75 | rproc_dmp / s_flags_str | 08 |
| dmp_ds.c | ~52 | data_store_dmp | 09 |
| dmp_vm.c | 157 | print_region / vm_dmp | 10 |
| glo.h | ~20 | 死 extern（diag_buf/diag_next/diag_size/sys_panic/dont_reply）+ DIAG_BUF_SIZE（glo.h:6） | 99（排除项） |
| inc.h | ~33 | `_SYSTEM`（inc.h:7）+ 17 个头包含 | 99（排除项） |
| proto.h | ~30 | 函数原型 | —（无运行时语义，排除） |
| Makefile | 22 | PROG=is、SRCS 8 文件、USE_APIC CFLAGS 耦合（:18-20）、CPPFLAGS 私有 include（:11-16） | **无**（覆盖缺口，见 §3.1） |

合计 1233 行（8 .c + 3 .h + Makefile，`wc -l` 实测）。

### 0.4 Rust 制品清单（真相源核对，非审阅对象）

`os/servers/is/src/` 13 文件 6266 行（`wc -l` 实测 2026-09-19）：acquire.rs 1523、dispatch.rs 419、dump_ds.rs 277、dump_kernel.rs 963、dump_pm.rs 346、dump_rs.rs 202、dump_vfs.rs 375、dump_vm.rs 483、lib.rs 747、main.rs 35、sef.rs 368、state.rs 51、tty_fkey.rs 477。`Cargo.toml`：依赖 minix-types / minix-sys（E-ISKMESS 后按 V1-P2-1 约定加回，含 real-trap 门控注释）/ minix-sef。

代码已实现但正式文档未讲的机制（grep 核对结果，全部入知识点池"新增"）：`DumpState`（lib.rs:97，分域游标收拢——正式文档零命中，仅 todo.md:256 记录）、hosted -EIO 契约测试与 boot-image 构建门控（Cargo.toml 注释 + edge3.md:59）、测试台账现状（125 个 `#[test]`）。其余 S23 接线符号（SysSefTransport/SysFkeyCtl/SysAcquires/diag_out/fkey_ctl_via/warn_fkey_ctl）已在 01/04 有提及（grep 实测），但分散在补丁注记与"接线"节中，需要归位。

### 0.5 边界材料

- `00-master-plan/README.md:26`：08 = IS，"不在 boot_image，RS 运行时加载"；:74 本目录为新建 stage。
- 前一 stage `07-stage-ds/00-ds-overview.md`：DS 的 init/main 模式已在 07 讲过——**本 stage 不得重复展开 SEF 通用生命周期教学**（SEF 只在 IS 特有语境下讲：三 init 回调同名、信号面只认 SIGTERM）。
- `edge_todo.md`：E-ISWIRE（:560，✅ S23 片 1-3b 已落）、E-ISPROD（:583，PM 腿已对齐、RS/DS/VFS 腿归 S33/S12）、E-ISKMESS（:601，主体 832c20945 已落）、E-ISBOOT（:620，端到端未联调）。
- `edge3.md:59`：S23 接线进度详录（片 1/2/3a/3b-1/3b-2/3b-3），余件三处挂他 stage。
- `draft/README.md`：早期占位，其"1151 行"计数与实测 1233 不符（旧值），其 boot_image 结论与 table.c:44-64 一致。

### 0.6 使用的命令与关键输出（证据摘录）

```text
git log -1 --format='%h %ad'                      → 40dfbbeb9 2026-09-19
wc -l os/servers/is/src/*.rs                      → total 6266
grep -c '#\[test\]' os/servers/is/src/*.rs        → total 125
grep -rn "08-stage-is" notes（stage 外 .md）       → 10 处（edge3/edge_todo/00-master-plan/04-stage-pm/03-stage-rs）
grep -rn --include=*.rs <旧文件名集> os/           → 23 处代码注释引用（清单见 §8.2）
stage 内对编号文档的交叉引用（grep 计数）           → 01:19 03:20 04:17 02:15 05:6 06:7 07:7 08:5 09:6 10:7 00:6 99:6，合计 125
ls minix3/minix/servers/is/ + cat Makefile        → §0.3
grep -n "^#\{1,3\} " 0*.md 99*.md                 → 全部小节清单（§8.1 迁移表行来源）
```

---

## 1. C 真序（运行时重建）

### 1.1 阶段类型判定

**服务事件循环型**。依据：`main.c:44` `while (TRUE)` 无限循环 + `get_work()` 阻塞收消息（`main.c:121-130`）；进程由 SEF 框架启动（`main.c:40-41`）；不在 kernel boot_image（`minix3/minix/kernel/table.c:44-64` 无 is 条目，draft README 与 master-plan 一致），由 rc.minix 条件启动（`minix3/etc/rc.minix:117`）经 RS 运行时加载。故真序分**启动段 + 循环段**两段；16 个转储函数是循环段分派出去的**并行体**（§1.4）。

### 1.2 真序表——启动段（进程诞生到进入循环）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| S1 | 系统环境开启 debug fkeys 时，rc 脚本执行 `up -n is -period 5HZ` | `minix3/etc/rc.minix:117` | IS 是条件调试服务；`-period 5HZ` 是 SEF 时钟期参数 |
| S2 | RS 按 system.conf 加载 IS：`service is { vm INFO; uid 0; }` | `minix3/etc/system.conf:271-277` | uid 0（root 门控取数需要）、VM INFO 权限；不在 boot_image |
| S3 | `main()`：`env_setargs(argc, argv)` 保存环境参数 | `main.c:40` | 环境串供 SEF/参数解析 |
| S4 | `sef_local_startup()`：注册 3 个 init 回调（fresh/lu/restart 全部注册同一个 `sef_cb_init_fresh`）+ 1 个信号回调，然后 `sef_startup()` | `main.c:76-89` | 三回调同名 = Live Update/restart 都走同一初始化（对 STATELESS 语义关键） |
| S5 | SEF 完成握手后调用 `sef_cb_init_fresh`，函数体一行：`map_unmap_fkeys(TRUE)` | `main.c:94-102` | boot 锚点 |
| S6 | `map_unmap_fkeys`：遍历 hooks 表把 16 个键码汇编成 fkeys/sfkeys 两个位图（`bit_set(fkeys, key - F1 + 1)`，bit0 不用） | `dmp.c:45-65`（位号规则 :55/:57） | 键→位的双区映射 |
| S7 | `fkey_map(&fkeys, &sfkeys)` →（libsys）→ TTY `do_fkey_ctl` MAP 分支：写观察位图、记 owner=IS | `dmp.c:60`；`minix3/minix/servers/is/dmp.c:63-64` | 失败只打印告警 `IS: warning, fkey_ctl failed`，**不退出**——带病继续跑 |
| S8 | init 返回 OK，进入 `main` 主循环 | `main.c:44` | 启动段结束 |

### 1.3 真序表——循环段（每次按压的完整生命周期）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| L1 | `get_work()`：`sef_receive(ANY, &m_in)` 阻塞；失败 panic；成功写回 `who_e = m_in.m_source`、`callnr = m_in.m_type` | `main.c:121-130` | SEF 对 ping 透明应答（SEF 库内行为，主循环永远看不到 ping） |
| L2 | `is_notify(callnr)` 分类：notify → 按发送者分类；非 notify → `printf("IS: warning, got illegal request %d from %d")` + EDONTREPLY | `main.c:48/:59-63` | **IS 不服务任何普通请求**——它只有一个入口：TTY 的 notify |
| L3 | notify 且 `_ENDPOINT_P(who_e) == TTY_PROC_NR` → `do_fkey_pressed(&m_in)`；其他发送者 → 静默 EDONTREPLY（C 注释 `/* FIXME: error message. */`） | `main.c:49-57` | 两层分类：先 notify 位、再 TTY 槽 |
| L4 | `do_fkey_pressed` 第一步：`fkey_events(&fkeys, &sfkeys)` 向 TTY 拉取位图（通知本身不带"哪个键"）；失败打印告警但**继续用未初始化位图判定**（C 事实） | `dmp.c:79-85` | 拉取是**破坏性读**（TTY 侧清零，见 02） |
| L5 | 全表扫描：`for h in 0..NHOOKS`，`pressed(F1,F12,fkeys,key) || pressed(SF1,SF12,sfkeys,key)` 命中即 `hooks[h].function()`；**无 break**——一次按压多个位图命中就连续执行多个转储 | `dmp.c:88-94` | 表驱动 + 区间/位双检（`dmp.c:70-72` 宏） |
| L6 | 各 dump 函数（§1.4）自行取数并 printf 到控制台 | `dmp_*.c` 各文件 | 输出面 = 标准输出（进程 stdout → TTY） |
| L7 | 返回 `EDONTREPLY` → 回复门（L8）关闭——notify 无回复对象 | `dmp.c:97`；`main.c:66-68` | 若有 result ≠ EDONTREPLY 才 `reply(who_e, result)`（ipc_send 失败 panic，`main.c:143-145`） |
| L8 | 回到 L1 | `main.c:44` | — |
| X1 | 收到 SIGTERM：`sef_cb_signal_handler` → `map_unmap_fkeys(FALSE)` → `exit(0)`；其他信号直接 return | `main.c:107-116` | **先注销后退出**的顺序不变量：否则 TTY 留悬空观察者 |

### 1.4 并行体：16 个转储入口（hooks 表，`dmp.c:14-35`）

| 键 | 函数 | 描述（表内 name 字段） | 源文件 | 按取数通道分组 |
|----|------|----------------------|--------|----------------|
| F1 | proctab_dmp | Kernel process table | dmp_kernel.c | GET_PROCTAB（+GET_KINFO） |
| F3 | image_dmp | System image | dmp_kernel.c | GET_IMAGE |
| F4 | privileges_dmp | Process privileges | dmp_kernel.c | GET_PRIVTAB |
| F5 | monparams_dmp | Boot monitor parameters | dmp_kernel.c | GET_MONPARAMS |
| F6 | irqtab_dmp | IRQ hooks and policies | dmp_kernel.c | GET_IRQHOOKS + GET_IRQACTIDS |
| F7 | kmessages_dmp | Kernel messages | dmp_kernel.c | kerninfo 直读（.usermapped） |
| F8 | vm_dmp | VM status and process maps | dmp_vm.c | GET_PROCTAB + vm_info×3 |
| F10 | kenv_dmp | Kernel parameters | dmp_kernel.c | GET_KENV（取而不用，dmp_kernel.c:191-213） |
| SF1 | mproc_dmp | Process manager process table | dmp_pm.c | getsysinfo(SI_MPROC_TAB) |
| SF2 | sigaction_dmp | Signals | dmp_pm.c | getsysinfo(SI_SIGTAB_SET) |
| SF3 | fproc_dmp | Filesystem process table | dmp_fs.c | getsysinfo(SI_PROC_TAB) |
| SF4 | dtab_dmp | Device/Driver mapping | dmp_fs.c | getsysinfo(SI_DMAP_TAB) |
| SF5 | mapping_dmp | Print key mappings | dmp.c:120-132 | 无（自描述，纯本地表） |
| SF6 | rproc_dmp | Reincarnation server process table | dmp_rs.c | getsysinfo(SI_RS_PROC_TAB) |
| SF8 | data_store_dmp | Data store contents | dmp_ds.c | getsysinfo(SI_DS_TAB) |
| SF9 | procstack_dmp | Processes with stack traces | dmp_kernel.c | GET_PROCTAB + sys_diagctl(STACKTRACE) |

（SI_* 请求号与转储对应按 `dmp_pm/fs/rs/ds.c` 调用实测；键位缺 F2/F9/SF7/SF10-SF12——未被观察。）

### 1.5 死路径与非 C 制品路径（真序不含，文档必须显式处置）

- `click_to_round_k` 宏：定义于 dmp_kernel.c（05 §2.1"一死宏"），零调用。
- arm 空分支：`#if defined(__arm__)` 的空 `proctab_dmp`（dmp_kernel.c:348-353 区段）。
- glo.h 五个 extern（diag_buf/diag_next/diag_size/sys_panic/dont_reply）：全仓 `rg` 仅声明无使用（todo §1.1 已核）。
- Makefile `USE_APIC` CFLAGS（:18-20）：注释亲口"must match the kernel's, as it affects the IRQ hooks table size"——F6 irqtab 的表尺寸是**构建期耦合**，运行时序之外的真相。
- 环境双层开关：system env `debug_fkeys`（rc 条件启动）≠ TTY 驱动 env `debug_fkeys`（键是否升级为 fkey 事件），同名不同物（02/plan 已有点破，B 相保持点名）。

---

## 2. 知识点全集（存量池 + 新增池）

> 编号规则：K-0xx 源自旧 00/01，K-03x/05x/07x 源自旧 02/03/04，K-09x~K-15x 源自旧 05~10，K-17x 源自旧 99，K-2xx 为覆盖审计新增。类型：概念 / 机制 / 数据结构 / 接口协议 / 约束不变量 / 架构演进 / 工程 / 测试。锚点省略公共前缀：C 文件在 `minix3/minix/servers/is/`，Rust 文件在 `os/servers/is/src/`。

### 2.1 池总表

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 主讲述点→新篇 |
|------|------|------|------|----------|------|----------|----------------|
| K-001 | IS 定位：用户态调试转储聚合器（"不再直接摸内核内存"的设计宣言） | 概念 | 存量 | 00§1、01§1.1-1.2 | main.c:1-8 头注释；dmp.c:1-9 头注释 | 回答"IS 是干什么的、为什么调试功能值得一个独立服务" | 新 00 |
| K-002 | 本 stage 文档地图与阅读路径 | 工程 | 存量 | 00§5 | — | 导航 | 新 00 |
| K-003 | IS 不在 boot_image、由 RS 运行时加载 | 机制 | 存量 | 00§3、draft README | kernel/table.c:44-64（无 is 条目） | 回答"IS 从哪来" | 新 01（主）、00（一句） |
| K-010 | 启动装配全链：rc.minix 条件启动（debug_fkeys≠0）+ `-period 5HZ` + system.conf 权限声明 | 机制 | 存量(部分)+新增 | 01§2.11（部分） | rc.minix:117；system.conf:271-277 | 回答"什么条件下 IS 存在、以什么权限存在" | 新 01 |
| K-011 | main() 三段式骨架（env_setargs→sef_local_startup→while） | 机制 | 存量 | 01§2.2 | main.c:31-71 | 服务主循环通用形态在 IS 的最小实例 | 新 01（启动部分）+新 03（循环部分） |
| K-012 | 四个静态全局 m_in/m_out/who_e/callnr = 服务器全部可变状态 | 数据结构 | 存量 | 01§2.1 | main.c:14-17 | 单线程事件循环的状态规模感 | 新 01 |
| K-013 | get_work：sef_receive(ANY) 阻塞 + 双写回 + 失败 panic | 机制 | 存量 | 01§2.9 | main.c:121-130 | 消息如何进入循环 | 新 03 |
| K-014 | 两层分类：is_notify → _ENDPOINT_P(who_e)==TTY 槽；default 分支 C 留 FIXME 静默 | 机制 | 存量 | 01§2.3 | main.c:48-57 | 回答"IS 认谁的消息、不认的怎么处理" | 新 03 |
| K-015 | 非 notify 一律"告警 + 不回复"：IS 不服务普通请求 | 约束不变量 | 存量 | 01§2.4 | main.c:59-63 | 与普通服务器的本质区别 | 新 03 |
| K-016 | 回复门 reply(who_e, result)（EDONTREPLY 抑制；ipc_send 失败 panic） | 机制 | 存量 | 01§2.5 | main.c:66-68,135-146 | 回复路径全貌 | 新 03 |
| K-017 | sef_local_startup：3 个 init 回调（fresh/lu/restart 同名）+ 信号回调注册 | 机制 | 存量 | 01§2.6 | main.c:76-89 | SEF 生命周期钩子怎么挂 | 新 01 |
| K-018 | init_fresh 一行 = map_unmap_fkeys(TRUE)：boot 锚点 | 机制 | 存量 | 01§2.7 | main.c:94-102 | 注册的触发时机 | 新 01 |
| K-019 | 只认 SIGTERM：unmap → exit(0)，其他信号忽略；"先注销后退出"顺序不变量 | 约束不变量 | 存量 | 01§2.8 | main.c:107-116 | 关闭语义与悬空观察者防御 | 新 01 |
| K-020 | SEF ping 透明拦截：主循环永远看不到 ping | 机制 | 存量 | 01§2.10 | SEF 库行为（01§2.10 论证） | 循环段输入面为何只剩 notify/请求两类 | 新 03 |
| K-021 | Live Update 三回调同名的语义（LU 不特化；Rust 侧 LU 登记不实装） | 约束不变量 | 存量(半)+新增 | 01§2.6 一句 | main.c:80-82；edge3.md:59 | 回答"LU/restart 时观察者状态怎么办"（配合 K-038） | 新 01（事实）+新 99（WONTFIX 对账） |
| K-022 | Rust IsServer<T,F,A> 三泛型传输缝设计 | 架构演进 | 存量 | 01§3.2/§4.2 | lib.rs:127 | 用户态服务器可测性模式 | 新 01 |
| K-023 | startup/step/run 三段拆分（step 单轮可测、run `-> !`） | 架构演进 | 存量 | 01§3.5/§4.2 | lib.rs:151/161/200 | 事件循环的单步测试策略 | 新 01 |
| K-024 | 分类器纯函数化（classify 与副作用分离） | 架构演进 | 存量 | 01§3.3 | dispatch.rs | 真值表可穷举测试的基础 | 新 03 |
| K-025 | 注册失败必须告警、不 panic、不退出（C printf；Rust warn_fkey_ctl 通道） | 约束不变量 | 存量 | 01/02 §V1 注记 | dmp.c:63-64；lib.rs request_fkey_map Err 臂 | 半开状态的可观测性 | 新 01（不变量）、新 10（通道实现） |
| K-026 | 生产 transport 三件套接线（SysSefTransport/SysFkeyCtl/main 装配） | 工程 | 存量 | 01"接线（S23 片 1/2）"节 | sef.rs/tty_fkey.rs/main.rs；edge3.md:59 | 从 fake 到 real 的路径 | 新 10 |
| K-030 | 三层编号：键码（keymap.h F1=0x10+EXT）/ 位号（1-12，bit0 不用）/ 数组下标 | 接口协议 | 存量 | 02§1.3/§2.3 | minix3/minix/include/minix/keymap.h:93；dmp.c:55/:57 | 读任何 fkey 代码不被三套数字绕晕 | 新 02 |
| K-031 | 三命令控制码 FKEY_MAP/UNMAP/EVENTS（10/11/12） | 接口协议 | 存量 | 02§2.1 | minix3/include/minix/com.h:874-877 | 协议的请求面 | 新 02 |
| K-032 | 两消息布局（请求/回复的 M1 槽字段表） | 接口协议 | 存量 | 02§2.2 | minix3/include/minix/ipc.h:1447-1454 | 线格式 | 新 02 |
| K-033 | libsys 客户端 fkey_ctl：30 行薄封装（三命令同构） | 机制 | 存量 | 02§2.4 | libsys 源（02§2.4 引全码） | 调用方视角的协议 | 新 02 |
| K-034 | fkey_map/fkey_unmap/fkey_events 三宏 | 数据结构 | 存量 | 02§2.5 | minix3/include/minix/sysutil.h:43-46 | C 侧 API 糖 | 新 02 |
| K-035 | map_unmap_fkeys 位图汇编循环（一表两区、键→位） | 机制 | 存量 | 02§2.6、01§3.3 | dmp.c:45-65 | 注册的实际内容是什么 | 新 01（注册动作）、新 02（编号规则深挖） |
| K-036 | TTY 侧 do_fkey_ctl 三命令状态机：MAP 覆盖注册、UNMAP owner 校验 EPERM 部分失败、EVENTS 破坏性读 | 机制 | 存量 | 02§2.7 | TTY 驱动源（02§2.7 引）；`#if DEAD_CODE` EBUSY 跳过 | 协议对手方行为，回答"重复注册会怎样/别人能注销我的键吗/读事件后事件还在吗" | 新 02（契约面）；TTY 内部实现 → 12-stage-input |
| K-037 | 通知来源链：kb 中断 → func_key → notify IS | 机制 | 存量 | 02§2.8 | TTY 驱动源 | 按键如何变成一条消息 | 新 02（一行版）；完整版 → 12-stage-input |
| K-038 | STATELESS 重启 ↔ 覆盖注册闭环（重启即重注册，无需恢复状态） | 约束不变量 | 存量 | 02§2.9 | main.c:80-82 + do_fkey_ctl MAP 行为 | IS 崩了/被 RS 重启后系统如何自愈 | 新 02 |
| K-039 | A-1：fkey 协议类型/常量上移 minix-types::ipc::tty（单一权威） | 架构演进 | 存量 | 02§3.1 | os/libs/minix-types/src/ipc/tty*（实测存在） | 跨 crate 常量纪律 | 新 02（决策）、新 99（对账行） |
| K-040 | FkeyId 中转层：三层编号在 Rust 类型系统中的显式化 | 架构演进 | 存量 | 02§3.2 | tty_fkey.rs:17 注释 | 防"三个 int 混用"错误 | 新 02 |
| K-041 | 客户端 trait 缝（FkeyCtlTransport）与占位→真装的演进 | 架构演进 | 存量 | 02§3.3 | tty_fkey.rs | 何时允许 panic 占位 | 新 02（设计）、新 10（真装） |
| K-042 | 协议面双通道回复：状态码 + 位图写回（out 参数经消息回传） | 接口协议 | 存量 | 02§2.2/§2.7 | ipc.h 布局 | "reply 里带数据"的微内核表达 | 新 02 |
| K-050 | struct hook_entry + hooks[] 16 项（键/函数/描述三元组） | 数据结构 | 存量 | 03§2.1 | dmp.c:14-35 | 一表三用：注册集合、分派表、自描述清单 | 新 03 |
| K-051 | NHOOKS 表驱动 + "注册集合 = 转储能力集合"不变量 | 机制 | 存量 | 03§2.1/§3.1 | dmp.c:40 | 改表即改能力的闭环设计 | 新 03 |
| K-052 | pressed 宏：区间 + 位双检 | 数据结构 | 存量 | 03§2.2 | dmp.c:70-72 | 位图→键的判定原语 | 新 03 |
| K-053 | do_fkey_pressed：EVENTS 整包拉取 → 全表扫描无 break → EDONTREPLY；events 失败告警后带病继续（C 事实） | 机制 | 存量 | 03§2.3 | dmp.c:73-98 | 一次通知的完整处理；多键并发按压行为 | 新 03 |
| K-054 | key_name 静态缓冲三式 → Rust `?` 分支类型级消除 | 架构演进 | 存量 | 03§2.4/§3.5 | dmp.c:103-114 | C 静态缓冲陷阱的类型系统解法 | 新 03 |
| K-055 | mapping_dmp：SF5 自查键位清单（枚举 hooks 表打印） | 机制 | 存量 | 03§2.5 | dmp.c:120-132 | 服务自描述能力 | 新 03 |
| K-056 | "一次 F1 按压的旅程"路径图（notify→拉位图→扫表→转储→不回复） | 概念 | 存量 | 03§2.6 | 综合 §1.3 真序 | 全链心智模型 | 新 03 |
| K-057 | dispatch_each 回调式分派（零分配）+ m 参数消失（C 宏隐式→Rust 显式） | 架构演进 | 存量 | 03§3.3/§3.4 | dispatch.rs | 表驱动在 Rust 的惯用形 | 新 03 |
| K-058 | 16 长度隐式耦合 → HOOKS.map + 编译期断言（Fix #4） | 约束不变量 | 存量 | 03 §V1 注记 | lib.rs:173-176 区段；todo §2 V1-P3-1 | 表变更防漂移 | 新 03 |
| K-059 | run_dump：16 臂取数→渲染分发；恒抑制回复 + 失败告警通道（warn_fkey_events） | 架构演进 | 存量 | 03§3.6 | lib.rs run_dump | 分派与执行的接缝 | 新 03（形态）、新 10（16 臂实装） |
| K-070 | 为什么五条通道而不是一条：数据主权分散 | 概念 | 存量 | 04§1.1 | 综合 dmp_*.c 调用点 | 取数面的总体动机 | 新 04 |
| K-071 | sys_getinfo 通道：SYS_GETINFO 调用号、GET_* 全 24 值中 IS 用 8 个、endpt=SELF"内核永远存到调用方" | 接口协议 | 存量 | 04§2.1 | com.h:236,315-345 | kernel 表的读取协议 | 新 04 |
| K-072 | sys_diagctl(DIAGCTL_CODE_STACKTRACE)：procstack 专用回溯通道 | 接口协议 | 存量 | 04§2.2 | com.h（DIAGCTL 族） | 为什么栈回溯不走 getinfo | 新 04 |
| K-073 | kerninfo->kmessages 直读（.usermapped 段）+ A-3 移除 + E-ISKMESS 等价新子请求设计 | 架构演进 | 存量 | 04§2.3 | C get_minix_kerninfo 调用点；edge_todo.md:601-612 | 内核内存直读为何被禁、替代是什么 | 新 04 |
| K-074 | getsysinfo(SI_*) 通道：size 精确匹配契约 + root uid 门 + 服务侧 sys_datacopy(SELF→who_e) | 接口协议 | 存量 | 04§2.4 | sysinfo.h:11-17；各服务 getsysinfo 处理点 | 跨服务取数的三段式 | 新 04 |
| K-075 | vm_info 通道：STATS/USAGE/REGION 三查询 + 值通道（回复 M1 槽，不写调用方内存）+ 游标分批 | 接口协议 | 存量 | 04§2.5 | com.h:729-734；vm.h:40-71 | 与 K-074 相反的拷贝方向 | 新 04 |
| K-076 | 五通道 trait + Acquires 捆绑（细缝设计） | 数据结构 | 存量 | 04§3/§4.2 | acquire.rs | Rust 侧通道抽象 | 新 04 |
| K-077 | 类型化快照出参（数据出口设计：out 参数形态，Fix #5） | 架构演进 | 存量 | 04 §V1 注记 | acquire.rs 各 trait 签名（实测） | trait 如何表达"把表给我" | 新 04 |
| K-078 | ClockTransport::uptime 第六通道（getticks 语义，sigaction 告警列数据源） | 机制 | 存量 | todo §8（正式文档零） | acquire.rs ClockTransport | 时钟查询也在取数面上 | 新 04 |
| K-079 | 生产取数客户端全景：SysAcquires 七 what 直调 + diagctl code 2 + uptime real_ticks + GET_KMESSAGES 10008B 拆包 + SysGetsysinfo 五腿 + VM M1 解码/REGION -ENOTSUP | 工程 | 存量(散) | 01/04 §V1 注记 + edge3.md:59 | acquire.rs Sys* 实现 | 每腿的生产形态与已知缺口 | 新 10 |
| K-080 | 未接通腿的诚实出口：VFS 两腿本地 ENOSYS（发送侧缺回复会挂死 IS） | 约束不变量 | 存量 | 04 §V1 注记 | edge3.md:59 | fail-closed 纪律 | 新 10 |
| K-081 | E-ISPROD producer 布局对齐（KProcSnap 双源冲突；PM 腿已对齐 464B→76B + SI 值 0/1→2/9 真 bug 修复） | 工程 | 存量 | 04/todo | edge_todo.md:583-594；edge3.md:59 | 快照解释错位是行为级事故 | 新 04（契约）、新 10（进度）、新 99（对账） |
| K-090 | 内核域 8 转储清单与"最大数据主权者的只读审计"动机 | 概念 | 存量 | 05§1/§2 | dmp_kernel.c 全文件 | 为什么 8/16 席给 kernel | 新 05 |
| K-091 | 共享地基：每次按压全表拷贝 proc[]/priv[]/image[]（一次取数多表渲染的 C 母本） | 机制 | 存量 | 05§2.1 | dmp_kernel.c:55-57 | 转储的一致性问题背景 | 新 05 |
| K-092 | LINES=22 分页纪律 + MORE_MARKER/--more-- 交互 | 机制 | 存量 | 05§2.1 | dmp_kernel.c（LINES 定义） | 控制台逐屏输出协议 | 新 05（主）、各域差异行 |
| K-093 | PROCLOOP/PRINTRTS 宏：分页循环骨架 | 机制 | 存量 | 05§2.1 | dmp_kernel.c | C 侧游标的母本形态 | 新 05 |
| K-094 | PageCursor：oldrp 行指针、`>=` 先比较、三实例隔离（游标家族第一形） | 机制 | 存量 | 05§3.3 | dump_kernel.rs PageCursor | 分页游标基准形 | 新 05（主讲述点） |
| K-095 | kmessages 环形展开（kmess_start：静态末位回绕起点） | 机制 | 存量 | 05§2.2/§3.4 | dmp_kernel.c:62-88 | 环形缓冲的转储语义 | 新 05 |
| K-096 | monparams 换行展开 expand_newlines（含尾换行 bug 修复，Fix #6/V1-P1-4） | 机制 | 存量 | 05§2.3/§3.5 | dmp_kernel.c:93-116；dump_kernel.rs expand_newlines | 单串→多行 + 截断边界 | 新 05 |
| K-097 | irqtab 双表对读（GET_IRQHOOKS+GET_IRQACTIDS）+ USE_APIC 构建期表尺寸耦合 | 机制 | 存量(半)+新增 | 05§2.4 | dmp_kernel.c:121-163；Makefile:18-20 | 为什么 IRQ 表要和 kernel 同配置编译 | 新 05 |
| K-098 | image_dmp 表头名实不符 / kenv_dmp 取而不用（C 怪癖照录纪律的代表案例） | 约束不变量 | 存量 | 05§2.5/§2.6 | dmp_kernel.c:168-185,191-213 | "语义照录不修复"的判例 | 新 05 |
| K-099 | 三编码器：位→字符 const fn 契约（A-12：procs/rts/priv 位图打印） | 机制 | 存量 | 05§2.7/§3.2 | dump_kernel.rs 编码器 ×3 | 位图的可读化统一模式 | 新 05 |
| K-100 | privileges_dmp 回退与位图列（priv 表 + s_ipc_to 位图字扩展） | 机制 | 存量 | 05§2.8 | dmp_kernel.c:252-295；KPrivSnap | 权限表渲染 | 新 05 |
| K-101 | proctab 双架构分支：x86 实体 + arm 空函数（死分支处置） | 工程 | 存量 | 05§2.9 | dmp_kernel.c:348-353 | 架构分支在 Rewrite 中的去留判据 | 新 05 |
| K-102 | procstack"多一行"的谜 + diagctl 循环内取数 | 机制 | 存量 | 05§2.10 | dmp_kernel.c:358-380 | 渲染中取数形态的第一例 | 新 05 |
| K-103 | proc_name 四规则 → NameClass 分类枚举 | 机制 | 存量 | 05§2.11/§3.6 | dmp_kernel.c:385-395 | 名字派生规则 | 新 05 |
| K-104 | Wire 快照子集策略（A-4：只取打印所需字段 + 兼容方向） | 架构演进 | 存量 | 05§3.1 | dump_kernel.rs 快照 | 布局 ABI 子集怎么选 | 新 05（模式主讲述点，各域引用） |
| K-105 | PCStr：字节串 Display 手工实现宽度/精度/对齐（%-8.8s→write! 映射层） | 机制 | 存量 | 05 §（V1 轮吸收） | lib.rs:63 | C printf 格式规格在 Rust 的等价物 | 新 05 |
| K-106 | 诊断输出通道定型（A-6 裁决：core::fmt::Write sink + SYS_DIAGCTL 出口）与 render_kernel 8 体 | 架构演进 | 存量 | 05 §V1 注记 + todo §8 | dump_kernel.rs render_* | 输出面从"延后"到"实装" | 新 05（渲染层）、新 10（通道实装） |
| K-107 | click_to_round_k 死宏 + 05§2.12 明确排除清单 | 工程 | 存量 | 05§2.1/§2.12 | dmp_kernel.c（宏定义处） | 死代码识别与登记 | 新 05（就地）、新 99（汇总） |
| K-110 | mproc_dmp + sigaction_dmp 两转储（PM 域清单） | 机制 | 存量 | 06§2.1-2.2 | dmp_pm.c:41-73,75-109 | PM 表与信号动作表渲染 | 新 06 |
| K-111 | flags_str 11 位编码（mproc 状态位→字符列） | 机制 | 存量 | 06§2.3/§3.2 | dmp_pm.c:21-39 | 位图解码 PM 实例 | 新 06 |
| K-112 | MProcSnap 子集 + 464B→76B producer 对齐（上移 minix-types 单一权威） | 数据结构 | 存量 | 06§2.4/§3.1 | edge3.md:59 片 3b-2 | 快照即 wire 契约的实例 | 新 06 |
| K-113 | PmCursor：跳过 + 22 界 + 回绕（`>` 候选断，游标第二形） | 机制 | 存量 | 06§3.3 | dump_pm.rs PmCursor | 与 K-094 的差异 | 新 06（差异行）、新 05（对照表） |
| K-114 | sigaction 告警列：回绕减法（不超前）+ uptime 数据源 | 约束不变量 | 存量 | 06§3.4 | dmp_pm.c alarm 计算 | 时钟回绕算术 | 新 06 |
| K-120 | fproc_dmp + dtab_dmp 两转储（VFS 域清单） | 机制 | 存量 | 07§2.1-2.2 | dmp_fs.c:25-64,67-83 | VFS 表渲染 | 新 07 |
| K-121 | FP 位语义（fproc 标志位解码；blocked_on 枚举 + SDEV 缺口继承） | 数据结构 | 存量 | 07§2.3/§3.3 | dmp_fs.c；dump_vfs.rs | 阻塞原因列的表达 | 新 07 |
| K-122 | 双快照 DtabSnap/FProcSnap + 执行面扩展（nfds/fp_cdev_endpt 进快照） | 数据结构 | 存量 | 07§3.1 | dump_vfs.rs；todo §8 | 快照契约可演进（A-4 实例） | 新 07 |
| K-123 | fd 计数切片纯函数（每进程变长 fd 段的渲染前置） | 机制 | 存量 | 07§3.2 | dump_vfs.rs | 变长段处理 | 新 07 |
| K-124 | VfsAction 三态游标（跳过 vs 满页可区分，接线时从 bool 修正）+ 跳过差异点名 | 机制 | 存量 | 07§3.4 | dump_vfs.rs；todo §8 | 游标形状演化的动因案例 | 新 07 |
| K-130 | rproc_dmp（RS 域转储）+ IN_USE 过滤谓词 | 机制 | 存量 | 08§2.1/§3.3 | dmp_rs.c:26-58 | RS 花名册渲染 | 新 08 |
| K-131 | s_flags_str 双源 6 位（状态位两表合成一列） | 机制 | 存量 | 08§2.2/§3.2 | dmp_rs.c:61-73 | 位图解码的双源变体 | 新 08 |
| K-132 | RprocSnap + r_args 决策翻转（"不进快照"→512B 尾列 %s 数据源） | 数据结构 | 存量 | 08§3.1；todo §8 | dump_rs.rs | 契约扩展的判例（原决策与执行面冲突时怎么办） | 新 08 |
| K-133 | RsCursor：IN_USE 跳过形态（游标第四形） | 机制 | 存量 | 08§3.3 | dump_rs.rs RsCursor | 游标家族补全 | 新 08 |
| K-134 | R31 跨 stage 契约：RS 错误串面（srv_to_string_gen/print_services_status）"归 08 dump 面"的归属裁决 | 约束不变量 | 存量 | 03-stage-rs/todo.md:120/203 | **待验证**：C dmp_rs.c 全函数清单（rproc_dmp/s_flags_str，见 §1.4）无打印 RS 服务状态串的对应物 | 回答"这笔跨 stage 欠账到底归谁" | 新 08 契约含验证任务；若 C 无对应物 → 判定归属 03-stage-rs 并回写 edge |
| K-140 | data_store_dmp 全读（DS 域唯一转储）+ 四类型行格式（U32/STR/MEM/LABEL） | 机制 | 存量 | 09§2.1-2.2 | dmp_ds.c:9-51 | DS 仓库盘点 | 新 08 |
| K-141 | STR 行的 C bug（指针在 IS 地址空间无意义）+ Rust 偏离打标量字 | 约束不变量 | 存量 | 09 §V1 注记 | dmp_ds.c STR 分支；os/libs/minix-types/src/types/ds_store.rs:8 注释 | "照录 vs 修复"的判例（修复需代码注释双向存证） | 新 08 |
| K-142 | DsCursor：`&&` 条件界（第三形：早返保持 + 到尾归零 + SKIP 不计数） | 机制 | 存量 | 09§2.3/§3.3 | dump_ds.rs DsCursor | 游标第三形 | 新 08（差异行）、新 05（对照表） |
| K-143 | DsEntrySnap（A-4 + A-10 消费：DS wire 类型来自 07-stage-ds） | 数据结构 | 存量 | 09§3.1 | dump_ds.rs | 跨 stage 类型复用 | 新 08 |
| K-144 | 类型枚举 + 缺省中止（未知 flags 行为） | 约束不变量 | 存量 | 09§3.2 | dump_ds.rs | 防御行为照录 | 新 08 |
| K-150 | vm_dmp 三快照（VmStats/VmUsage/VmRegionSnap，vm.h:40-71 子集）+ PROT 1/2/4 | 数据结构 | 存量 | 10§2.5/§3.1 | minix3/include/minix/vm.h:40-71；sys/sys/mman.h:62-65 | VM 面 ABI 子集 | 新 09 |
| K-151 | FoldState 折叠状态机：相邻四等判定、吸收计数、延迟打印（"多重复几次"不含首个）、NULL 双用、跨表残留照录、FlushRepeat 重喂契约 | 机制 | 存量 | 10§2.1/§3.2 | dmp_vm.c:11-50 | 连续相同区间的压缩表达 | 新 09 |
| K-152 | BatchCursor 双游标批机：prev_i/prev_base、每轮迭代尾清基、首屏容量预检（header 都不打整批让位）、`n > LINES` 防御内错、擦除行 8 空格 | 机制 | 存量 | 10§2.2-2.4/§3.3 | dmp_vm.c:59-155 | 最复杂游标（第五形） | 新 09 |
| K-153 | VM_LINES=24 ≠ LINES=22（分页额度域间差异）+ 首屏两空行各计一行 | 约束不变量 | 存量 | 10§2.2/§4.2 | dmp_vm.c:14（LINES 重定义）,76-79 | 常量的域局部性 | 新 09 |
| K-154 | M1 槽值通道解码（[ARCH: 26-D1]；REGION 条目数组 ≤2 条/包 + 游标停滞守卫，D7 编码缺口的 IS 侧现状） | 接口协议 | 存量 | 10§2.5/§3.1 | dump_vm.rs；edge3.md:59 片 3b-3 | 消费侧与生产侧的编码耦合 | 新 09（消费）、新 04（通道）、新 10（REGION 缺口） |
| K-155 | prot_chars(rwx) const fn + kB 下取整注记 | 机制 | 存量 | 10§3.4 | dump_vm.rs | 保护位渲染 | 新 09 |
| K-156 | render_vm"渲染中取数"唯一形态（region 批在循环内拉取；其余域先取后渲） | 机制 | 存量 | 10 §V1 注记；todo §8 | dump_vm.rs render_vm | 三章结构中的例外点名 | 新 09 |
| K-170 | 常量单一权威对账表（9 族：FKEY 族/GET_*/SI_*/VMIW_*/NOTIFY/SIGTERM/EDONTREPLY/格式常量族/SI_PROC_TAB 重复定义案例） | 约束不变量 | 存量 | 99§1；todo §1.2 | 对账表本体在 todo.md，99 正文 pending 未吸收 | 谁能改这些值、值在哪 | 新 99 |
| K-171 | 错误码面（EDONTREPLY 权威 + errno 映射纪律） | 接口协议 | 存量 | 99§2 | errno.rs:101 | 错误传播底线 | 新 99 |
| K-172 | 执行模型纪律：用户态单线程事件循环，!Send/!Sync/Rc/RefCell 合理、零分配（无 alloc 依赖，全定长栈数组） | 约束不变量 | 存量 | 99§3；todo V1-P3-4 | lib.rs:1（无 extern crate alloc，Fix #7 后文实一致） | 并发审查的免除依据 | 新 99 |
| K-173 | 排除项清单：glo.h 死 extern ×5 + DIAG_BUF_SIZE(glo.h:6) + _SYSTEM(inc.h:7) + proto.h + 死宏/死分支汇总 | 工程 | 存量 | 99§4；plan §5.4 | 各锚点 | "为什么不写"的账本 | 新 99 |
| K-174 | 跨服务引用登记（谁在引用本 stage） | 工程 | 存量 | 99§5 | §8.2 引用清单 | 断链治理入口 | 新 99 |
| K-175 | ARCH 演进编号体系 A-1..A-12（plan §4 定义、各篇消费） | 架构演进 | 存量 | plan §4；各篇散引 | plan.md §4 | 决策索引 | 新 99（权威表），各篇就地复述禁用 |
| K-200 | DumpState：C 各 static prev_i/oldrp 的 Rust 收拢（IsServer 分域状态字段） | 机制 | **新增** | 正式文档零覆盖（grep 实测）；todo.md:256 | lib.rs:97/133/144 | 游标实例归谁持有 | 新 05（模式）、新 03（run_dump 接缝） |
| K-201 | 测试基建与台账：fake 三件套（FakeTransport/FakeFkey/FakeAcquires）+ hosted -EIO 契约测试 + real-trap 构建门控；当前 `#[test]` 计数 125（grep 实测 2026-09-19） | 测试 | **新增** | 各篇 §5.3 旧计数（41~86，截至 09-04）漂移；edge3.md:59 | os/servers/is/src 各 test 模块；Cargo.toml 注释 | 数谁的测试、以何为准 | 新 10（台账唯一化）；各篇 §5 只留场景表 |
| K-202 | 渲染架构三案裁决（各域自由渲染函数 + sink 入参；否决体持 Acquires、否决堆缓冲） | 架构演进 | **新增** | todo §8（正式文档零） | todo.md:256；lib.rs run_dump 各臂 | 执行面的设计依据 | 新 05（总模式）、新 10 |
| K-203 | 端到端启动链状态（E-ISBOOT：rc 条件启动 + RS 动态加载 + TTY 观察者联调未做） | 工程 | **新增** | 仅 edge_todo/01 接线节散记 | edge_todo.md:620-… | IS 何时真正跑起来 | 新 01（C 事实面）、新 10（Rust 状态面） |

### 2.2 统计摘要

- 总条数：**110**（存量 106 + 新增 4：K-200~K-203；另 K-010/K-021/K-097 三条为"存量+新增"复合条，已按存量计入）。
- 按类型分布（按表内主类型统计）：机制 45、约束不变量 15、架构演进 14、数据结构 11、接口协议 10、概念 4、工程 10、测试 1。
- 按旧文档分布：00:3、01:17、02:13、03:10、04:12、05:18、06:5、07:5、08:5、09:5、10:7、99:6、无旧文档归属(新增):4。
- 重复标记（同一知识点多位置出现，主讲述点唯一）：K-035（01/02）、K-094/K-113/K-124/K-133/K-142/K-152（游标家族，对照表主讲述点新 05）、K-025（01/02 注记）、K-104（05 定义、06~09 引用）、K-175（plan/各篇散引）。

---

## 3. 覆盖审计

### 3.1 主题全集与覆盖缺口表

主题全集四路来源：① C 符号（33 函数 + 宏 + 结构 + 常量 + 状态机 + 架构分支，`minix3/minix/servers/is/` 全量已读）；② 通用 OS 概念（服务生命周期、观察者模式、分页游标、位图解码、环形缓冲、快照/ABI、单线程事件循环）；③ 非 C 制品（§3.5 十项清单）；④ 边界契约（E-ISWIRE/ISPROD/ISKMESS/ISBOOT、A-1..A-12）。

| 缺口主题 | 现状 | 处置 | 新增知识点 |
|----------|------|------|------------|
| 启动装配全链（rc.minix 条件 + system.conf + RS 加载 + `-period 5HZ`） | 01§2.11 只讲"何时存在"一句，权限声明与 SEF 时钟参数无正式讲述 | 并入新 01 | K-010 |
| Makefile USE_APIC 构建期耦合（IRQ 表尺寸）与 CPPFLAGS 私有 include | 正式文档零覆盖（本次 `cat Makefile` 实测发现） | 并入新 05（irqtab 小节） | K-097 新增半 |
| DumpState 分域游标收拢（C static 游标的 Rust 持有者） | 仅 todo.md:256；正式文档零（grep 实测） | 并入新 05（模式）+ 新 03（接缝） | K-200 |
| Live Update 三回调同名语义 + Rust 侧 LU 不实装的对照 | 01§2.6 一句带过，无裁决记录 | 新 01 讲事实，新 99 记 WONTFIX 对账 | K-021 |
| 生产接线全貌（transport 三件套 / 五腿取数客户端 / hosted -EIO 测试 / 构建门控） | 散在 01"接线"节 + 各篇 V1 补丁注记（非正式叙事） | **新建新 10** 集中收口 | K-026/K-079/K-080/K-201 |
| 测试台账（唯一计数出口） | 每篇 §5.3 各记一份且全部陈旧（实测 125 vs 文中 86） | 新 10 唯一台账；各篇只留场景表 | K-201 |
| E-ISBOOT 端到端联调状态 | edge 材料有、文档无 | 新 01（C 事实）+ 新 10（Rust 状态） | K-203 |
| R31 跨 stage 欠账（RS 错误串面"归 08 dump 面"） | 03-stage-rs/todo.md 单方面登记；本 stage 无文档认领；C `dmp_rs.c` 无对应物 | 新 08 契约含**验证任务**：若确认 C IS 无此功能，判定归还 03-stage-rs 并回写 edge_todo（禁止无中生有写进 dump 文档） | K-134 |
| IS 二进制的镜像安放 / boot image 构建装配 | 无 | **判定不做**：属 19-stage-integration 镜像组装主题（master-plan 定位），新 10 留一行指针 | — |
| `reply` 失败 panic / `sef_receive` 失败 panic 的错误路径 | 01§2.5/§2.9 已讲 | 无缺口（池内 K-013/K-016） | — |

### 3.2 重复主题表

| 主题 | 重复位置 | 新目录主讲述点 | 其余处理 |
|------|----------|----------------|----------|
| map_unmap_fkeys 位图汇编 | 01§3.3、02§2.6 | 新 01（注册动作场景） | 新 02 只回指 + 深挖编号三层 |
| 三层编号/位号规则 | 02§1.3、02§2.3 | 新 02 | 新 01 首次用到位号时完整定义"键→位"（满足首次即完整），键码/下标层声明留给新 02 |
| hooks 表结构 | 02§2.6、03§2.1 | 新 03 | 新 01 只说"遍历注册表汇编位图"，表细节回指 |
| EDONTREPLY 语义 | 01§2.5、03§2.3、05~10 各引 | 新 03（循环段） | 其余回指 |
| 分页游标家族（五形） | 05§3.3、06§3.3、07§3.4、08§3.3、09§2.3/§3.3、10§2.4 | 新 05（基准形 + 对照总表） | 各域篇只留"本域差异一行 + 回指" |
| A-4 快照子集模式 | 05~10 每篇 D1 | 新 05（模式定义） | 各篇只列本域快照字段表 |
| "体延后（A-6）"决策 + 翻转注记 | 05~10 每篇 D 末 + V1 注记 | 新 05（A-6 通道定型一次）+ 新 10（执行面总账） | 各篇正文直接写实装后的事实（B 相重写消灭补丁叙事） |
| getsysinfo 机制（size 匹配/root 门/datacopy） | 04§2.4、06/07/08/09 各篇 §2 复述 | 新 04 | 各域篇回指 + 只留本域请求号与布局 |
| 常量权威/排除项/ARCH 编号 | plan §4/§5.4、99、todo §1.2、各篇散引 | 新 99 | 各篇禁用复述，一律回指 |
| "先 unmap 后 exit" 不变量 | 01§2.8、02§2.9 | 新 01 | 新 02 从协议闭环视角回指 |
| 章内小结重复两遍（06:32-36、10:31-35） | 编辑事故 | — | 重建时删除其一（不视为知识点） |

### 3.3 越界主题表

| 越界内容 | 位置 | 正确归属 |
|----------|------|----------|
| "接线（S23 片 1/2）"开发进度叙事混入正式文档 | 01 文末节 | 新 10（生产形态篇）；进度流水账归 edge3/todo，正式文档只写状态事实 |
| "V1 执行轮更新 / 03 修订 / 01 更新 / Fix #n 注记" 补丁块 | 03§4.4、05~10 §3 末 | 事实融入正文叙述（重写后"体已实装"是正文默认态，不是注记）；决策理由归新 10/新 99 |
| 每篇 §5.3 测试计数台账 | 03~10 | 新 10 唯一台账 |
| "10 篇全 reviewed / 阶段完成"交付状态叙事 | 10§6 | 删除（归 todo/STATE 面），过渡节只留教学过渡 |
| TTY 侧 do_fkey_ctl 实现细节的展开风险 | 02§2.7 | 压缩为"观察者契约"（对手方行为承诺表）；TTY 内部机制 → 12-stage-input |
| VM 内部语义（vm_info 服务端实现） | 10 各处引用 | 保持引用指针（02-stage-vm），不展开（旧篇边界声明已如此，维持） |
| 00§2 "启动主线（plan §1.2）"转述 plan 结论 | 00 | 新 00 用自有 C 证据重写（§1 真序表即证据），不转述 plan |

### 3.4 锚点勘误表（B 相按此修，禁止原样搬迁）

| # | 位置 | 现锚点 | 实测应为 | 状态 |
|---|------|--------|----------|------|
| E1 | 02§2.6 标题 | `dmp.c:NHOOKS（L44，工具生成）` | `dmp.c:map_unmap_fkeys`（:45-65；L40 是 NHOOKS 定义，L42-44 是注释块） | ✅ 实测确认（本次重读 dmp.c） |
| E2 | 03§2.5 标题 | `dmp.c:key_name（L118，工具生成）` | `dmp.c:mapping_dmp`（:120-132；L118 是其注释块行） | ✅ 实测确认 |
| E3 | 03§2.1 | `hooks 表 16 项（dmp.c:11-35）` | `dmp.c:14-35`（:11 为 #include） | ✅ 实测确认（微偏） |
| E4 | 07§2.3 区段 | `FP_SESLDR 0004（…fproc.h:FP_PENDING）` 锚点与符号名不匹配 | 待验证（B 相 grep fproc.h 双向对账后修正） | ⚠️ 待验证 |
| E5 | 代码引用 | `os/libs/minix-types/src/types/ds_store.rs:8` 引 `09 §3.4` | 该节确有 STR bug 注记（在 V1 补丁块内，09:108 实测）——引用现状成立，但重建后事实融入新 08 正文，引用须迁移（§8.2 R-14） | ✅ 实测确认 |

### 3.5 非 C 主题逐项回答（固定十项清单）

| 项 | 在哪讲 / 为什么不在本 stage |
|----|------------------------------|
| 链接与加载 | C 侧：Makefile（PROG=is、-lsys、CPPFLAGS）→ 新 01 §2 + 新 05（USE_APIC）；Rust 侧：Cargo.toml 依赖与构建门控 → 新 10。ELF 装载机制本身 → 03-stage-rs（前 stage 已讲，不重复） |
| 镜像与内存布局 | kerninfo `.usermapped` 段直读（kmessages）→ 新 04 §2.3；各 wire 布局 ABI 子集 → 各域篇；IS 自身镜像布局无特殊内容（标准用户态服务）→ 不做，99 记一句 |
| 汇编入口与陷阱进入 | 用户态服务无汇编入口；`_start`/陷阱属内核与 libc 启动链 → 判定不在本 stage（01-stage-kernel / 19 库链），新 01 一行指针 |
| 启动装配 | rc.minix 条件启动 + system.conf + RS 动态加载 + `-period 5HZ` → 新 01 §2（K-010/K-003）；端到端联调状态 → 新 10（K-203） |
| 构建与工具链 | C Makefile USE_APIC 耦合 → 新 05；Rust workspace/feature 门控（real-trap）→ 新 10；xtask 镜像构建 → 19-stage-integration（不做，99 记指针） |
| 跨模块接口与线格式 | fkey 协议（com.h/ipc.h/sysutil.h）→ 新 02；GET_*/SI_*/VMIW_* 与拷贝方向 → 新 04；A-4 快照 repr(C) 契约 → 新 05 模式 + 各域字段表；M1 值通道 → 新 04/新 09 |
| 错误路径 | 注册失败告警（不 panic 不退出）→ 新 01；events 失败带病继续（C 事实）→ 新 03；illegal request 告警 → 新 03；panic 分支（receive/reply 失败）→ 新 03；ENOSYS/-ENOTSUP 诚实出口 → 新 10；errno 映射纪律 → 新 99 |
| 关闭与退出 | SIGTERM→unmap→exit 顺序不变量 + STATELESS 闭环 → 新 01/新 02 |
| 并发与同步 | IS 无锁无共享（单线程事件循环）；纪律条目（!Send/!Sync/Rc/RefCell 合理、零分配）→ 新 99（K-172）；跨 CPU 语义 → 内核 stage（不重复） |
| 测试基建 | fake 三件套模式 → 新 01（生命周期测试用 fake transport 首次引入处讲透）；hosted -EIO 契约 + 构建门控 + 唯一台账 → 新 10（K-201） |

---

## 4. 新目录

### 4.1 总表（12 篇 → 12 篇：合并 1、新建 1、移位 1、其余原位重建）

| 新编号 | 新文件名 | 一句话定位 | 分组 | 与旧关系 |
|--------|----------|------------|------|----------|
| 00 | 00-is-overview.md | IS 是什么、为什么存在、怎么读这十篇 | 框架 | 00 重建 |
| 01 | 01-is-startup.md | 启动段：条件诞生 → SEF 生命周期 → 注册观察者 → SIGTERM 退出 | 框架·主线 | 01 重建（拆出循环段） |
| 02 | 02-is-fkey-contract.md | F-key 观察者协议：三层编号、三命令、TTY 对手方行为、自愈闭环 | 框架·主线 | 02 重建 |
| 03 | 03-is-mainloop-dispatch.md | 循环段：收消息、两层分类、拉位图、查表分派、不回复 | 框架·主线 | 03 重建 + 吸收 01 循环段 |
| 04 | 04-is-data-acquisition.md | 取数通道总览：六条通道、两种拷贝方向、trait 出口设计 | 框架·主线 | 04 重建 |
| 05 | 05-is-dump-kernel.md | 内核域（并行体代表篇）：共享地基、分页游标基准形、快照/编码器/格式常量三件套、渲染管线 | 转储域 | 05 重建 + 升格为代表作 |
| 06 | 06-is-dump-pm.md | PM 域：mproc 点名册与 sigaction 闹钟簿 | 转储域 | 06 重建 |
| 07 | 07-is-dump-vfs.md | VFS 域：fproc 借书登记与 dtab 书架分布 | 转储域 | 07 重建 |
| 08 | 08-is-dump-rs-ds.md | 注册服务小域合篇：RS 花名册 + DS 仓库盘点 | 转储域 | **08+09 合并** |
| 09 | 09-is-dump-vm.md | VM 域（并行体最复杂成员）：折叠状态机 + 双游标批机 | 转储域 | **旧 10 重编号为 09** |
| 10 | 10-is-production-wiring.md | 生产形态：transport 三件套、取数五腿真装、测试基建与唯一台账、遗留缺口一览 | 支线 | **新建** |
| 99 | 99-is-global-concepts.md | 全局对账：常量权威表、错误码、执行模型纪律、排除项与死代码账本、ARCH 编号索引 | 收口 | 99 重建 |

### 4.2 结构决策与理由（对四条硬标准的独立回应）

1. **保持 01→04→05 主线骨架**：§1 真序表独立确认了旧目录的骨架顺序就是运行时序（诞生→协议→循环→取数→转储），重建不推翻已被真相源支持的结构——顺序保留、**内容全部按契约重写**（消灭补丁叙事/黑话/锚点污染）。
2. **01 拆分**（旧 01 一篇两用）：旧 01 同时承载启动段与循环段（§2.3-2.5/§2.9-2.10 全是循环段），违反单篇单语义；拆为 新 01（启动/关闭）+ 新 03（循环/分派），循环段与旧 03 的 hooks 分派本是同一段真序（L1-L7）的两半，合篇消除"分类器在 01、处理在 03"的断裂。
3. **08+09 合并**：RS 域（2 个 C 函数，dmp_rs.c ~75 行）与 DS 域（1 个 C 函数，dmp_ds.c ~52 行）各不足以独立成篇——两篇旧文分别只有 153/174 行且互引密集（游标对照）；合并后仍 <450 行，同组（同为 getsysinfo 通道的注册用户服务状态表）语义一致。DS 的 STR-bug 判例（K-141）值得独立保留教学位？不：它是"照录 vs 修复"纪律的案例，与 RS 的 r_args 契约翻转（K-132）同属"快照契约演进"主题，放一起对比更强。
4. **旧 10 → 新 09**：腾出的 10 号位给新建的"生产形态"支线篇，转储域编号连续（05~09）。
5. **新建 10**：生产接线、测试台账、遗留缺口是当前散落在补丁注记与 edge3 里的最大一块"代码已实现、文档没讲全"的面（§0.4 grep 实测）；也是唯一能容纳"开发过程事实"的位置——教学篇（01~09）保持线性叙事，状态/进度类事实全部下沉到此篇与 99。
6. **并行体组织**（规则 5.2）：六个转储域是并行体。统一框架 = 04（通道）+ 05 前半（渲染三件套模式 + 游标基准形 + 对照总表）；代表成员 = kernel 域（8 席最全、游标第一形、唯一涉及三种 kernel 通道）讲透；06~09 按"与 05 基准的差异"组织（各自契约中"与 05 的差异表"为强制小节）；主线 00→05，支线 06→09 任意序可跳读，10/99 完全独立。
7. **文档数控制**：篇数不变（12）但内容再分配；01/02 旧文 707/572 行的膨胀主因是补丁注记 + 复述，重写目标各 ≤500 行；05 因升格为代表篇，可放宽至 ~450 行（用户软约束：以概念复杂度为准）。

### 4.3 阅读路径

- **主线**（必读，顺序固定）：00 → 01 → 02 → 03 → 04 → 05。
- **转储域支线**（读完 05 后任意序）：06 / 07 / 08 / 09；推荐 06→07→08→09（游标复杂度递增，09 收官）。
- **可跳读支线**：10（只关心生产状态的人）、99（对账/审查的人）——均不依赖支线转储域篇（10 依赖 04，99 只依赖 00）。

### 4.4 序差表（教学序 ≠ 运行时序的条目，全部显式登记）

| # | 运行时序事实（锚点） | 教学序选择 | 理由 | 回指补偿位置 |
|---|---------------------|-----------|------|--------------|
| P-1 | 取数调用只发生在转储体内（按压触发后；dmp_*.c 各函数体内 sys_getinfo 等调用点） | 04（通道框架）先于所有转储域 | 规则 5.2：并行体先给统一框架；否则每域篇都要现场解释通道 | 05~09 开头"本篇用到 04 的哪几条通道"行 |
| P-2 | 位图注册在启动段（S5-S7），EVENTS/UNMAP 在循环段/关闭段 | 02 把三命令**作为一个协议整体**讲（含尚未执行的 UNMAP/EVENTS 对手方行为） | 协议拆两半讲必然产生前向引用；01 只讲"注册发生在这里 + 位号规则"的完整最小集 | 01 末"协议的完整对手方行为见 02"；03 L4 回指 02 §2 |
| P-3 | 游标五形各自只在自己域的转储里运行 | 05 讲基准形 + 对照总表，各域只讲差异 | 代表成员模式（规则 5.2） | 各域差异表行 |
| P-4 | 生产 transport 在启动/循环/取数的每一环都参与 | 10 集中讲（末篇） | 教学主线用 fake trait 即可成立；生产形态是"实现交接面"不是概念 | 01/03/04 各留一行"生产形态见 10"指针 |
| P-5 | 渲染实装（render_*）与"取数→渲染"管线是代码最新事实，旧文档叙事停留在"体延后" | 各域正文直接写实装后状态（B 相重写消灭补丁块） | 真相源优先（用户约束 3） | 无需要 |

---

## 5. 每篇契约

> 契约 = B 相写正文的任务书。知识点清单表的"来源"列：存量条目填旧文档位置（取料处），新增条目填证据锚点。锚点公共前缀同 §2 注。所有篇目通用禁令（写进每篇头部声明）：不引用 `.design/`；不复述 99 的常量对账表；不出现"V1/修复轮/Fix #n/S23"等开发过程词汇（状态事实的合法位置是新 10）；测试只写场景不写计数。

### 00-is-overview.md

- **一句话定位**：用 15 分钟回答"IS 是什么、为什么一个调试功能值得一个服务、本 stage 十篇各讲什么"。
- **讲什么**：K-001（定位与设计宣言）、K-002（文档地图与三条阅读路径）、K-003（一句：IS 由 RS 运行时加载，细节交 01）。
- **不讲什么**：任何协议/通道/游标机制（→01~05）；启动因果链全展（→01）；生产状态（→10）。
- **前置**：07-stage-ds 完成（会 SEF 服务通用形态）；IPC 基础（01/05-stage）。
- **后置**：全部。
- **事实底线**：main.c:1-8 头注释（"这些转储不再直接摸内核内存"）；dmp.c:14-35（16 席 hooks 表存在性的概览证据）；rc.minix:117；table.c:44-64。旧 00§2 转述 plan 的写法废除，导航表用本蓝图 §4.1 重制。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-001 | 调试转储聚合器定位 | 概念 | main.c:1-8; dmp.c:1-9 | 全 stage 动机句 | 旧 00§1、旧 01§1.1-1.2 收编为概述级 |
  | K-002 | 文档地图/阅读路径 | 工程 | 本蓝图 §4.1/§4.3 | 导航唯一处 | 旧 00§5 重制 |
  | K-003 | RS 加载（一句） | 机制 | table.c:44-64 | 概览需要 | 旧 00§3 |
- **验收标准**：读者不看任何其他篇能回答：①IS 与 DS/RS 在服务形态上的最大差异（普通请求一律拒绝）；②IS 何时存在（debug_fkeys 环境）；③主线/支线/跳读三条路径分别是什么。行数 ≤120。

### 01-is-startup.md

- **一句话定位**：讲透启动段与关闭段真序（S1-S8、X1）：IS 如何诞生、以什么权限诞生、boot 时做的唯一一件事（注册观察者）、以及怎样正确死去。
- **讲什么**：K-010（启动装配全链）、K-003（RS 加载细节）、K-011（main 启动部分：env_setargs/sef_local_startup）、K-012（四静态全局）、K-017（SEF 回调注册，三 init 同名）、K-021（LU 同名回调语义）、K-018（init_fresh 一行 + 注册触发时机）、K-035（位图汇编注册动作：位号规则此处完整定义到"够用"）、K-025（注册失败告警不变量）、K-019（SIGTERM→unmap→exit 顺序不变量）、K-022/K-023（Rust IsServer 三泛型 + startup/step/run）、K-203（C 事实面：条件启动链；Rust 状态面指针→10）。
- **不讲什么**：消息分类与循环体（→03）；协议对手方行为全貌（→02，P-2 序差）；生产 transport 实装（→10）；SEF 通用生命周期教学（07-stage-ds 已讲，只讲 IS 特有面）。
- **前置**：00。
- **后置**：02（注册协议深挖）、03（进入循环）、10（生产形态）、99（LU WONTFIX 对账）。
- **事实底线**：main.c:14-17/31-44/76-89/94-102/107-116；dmp.c:45-65；rc.minix:117；system.conf:271-277；table.c:44-64；lib.rs:63/127/151/161/200。
- **知识点清单**：上列 K-010/003/011/012/017/018/019/021/022/023/025/035/203 十三条（锚点见池表）。新增列两条：K-010 新增半（system.conf 权限声明，旧文无）、K-021（main.c:80-82 + edge3.md:59）。
- **验收标准**：①读者能默画启动段 S1-S8 时序（每步带函数名）；②能回答"为什么注册失败不阻止 IS 启动"（C printf 事实 + 半开状态可观测性）与"为什么必须先 unmap 再 exit"（悬空观察者）；③能回答"IS 被 RS 重启后需要恢复什么状态"（→ 引出 02 的闭环，一句话指向）；④文中不出现前向未定义概念（键码值、数组下标层、TTY 内部均只到"够用"深度并显式指向 02）。行数 ≤500。

### 02-is-fkey-contract.md

- **一句话定位**：把 F-key 观察者协议作为"IS↔TTY 合同"讲透：为什么一个按键通知需要三个命令、三层编号、TTY 对手方对每个命令的行为承诺、以及这套合同如何支撑 IS 的无状态自愈。
- **讲什么**：K-030（三层编号完整定义：键码 keymap.h:93/位号/数组下标）、K-031（三命令控制码）、K-032（两消息布局）、K-042（双通道回复：状态码+位图写回）、K-033（libsys 薄封装）、K-034（三宏）、K-035（回指 01 后深挖位号与键码换算）、K-036（TTY 侧 do_fkey_ctl 三命令行为契约表：MAP 覆盖、UNMAP owner/EPERM 部分失败、EVENTS 破坏性读，含 DEAD_CODE EBUSY 分支事实）、K-037（通知来源链一行版）、K-038（STATELESS↔覆盖注册闭环）、K-039（A-1 类型上移决策）、K-040（FkeyId 三形同效的类型学处理——用完整句解释，禁简写）、K-041（trait 缝设计，真装指针→10）。
- **不讲什么**：TTY 驱动内部实现（中断/键盘缓冲/func_key 细节→12-stage-input，本篇只给行为承诺表）；IS 主循环（→03）；注册调用发生在何时（→01 已讲，回指）。
- **前置**：01（注册动作与位号最小集）。
- **后置**：03（EVENTS 的消费端）、10（真装）、12-stage-input（对手方实现）。
- **事实底线**：com.h:874-877；ipc.h:1447-1454；sysutil.h:43-46；minix3/minix/include/minix/keymap.h:93（路径按本次实测）；dmp.c:45-65/70-72/82；TTY 侧 do_fkey_ctl 源（drivers/tty 内，B 相引精确路径）；tty_fkey.rs 全文；minix-types ipc::tty。
- **知识点清单**：K-030~K-042 十三条（其中 K-035 与 01 共享：01 讲"做什么"、02 讲"为什么这样编号"；表内两行都保留、锚点同一）。
- **验收标准**：①"一次 MAP 的旅程"图完整（IS 汇编位图→fkey_map→_taskcall→do_fkey_ctl→写观察位图/记 owner→回复）；②三命令对手方行为表每格有 C 源锚点；③读者能回答："IS 崩溃未注销，别人按 F1 会怎样？"（悬空观察者→notify 发给回收的槽？按 TTY 侧 owner 机制如实回答，标注 C 事实与推理边界）、"重复 MAP 报 EBUSY 吗？"（DEAD_CODE 分支事实）、"EVENTS 读两次得到什么？"（破坏性读）；④E1 勘误修复。行数 ≤520。

### 03-is-mainloop-dispatch.md

- **一句话定位**：讲透循环段真序（L1-L8）：一条消息如何被分类、一次 notify 如何变成零到多个转储、以及 hooks 表为什么是"一表三用"的分派心脏。
- **讲什么**：K-013（get_work）、K-014（两层分类 + FIXME 静默）、K-015（非 notify 告警不服务）、K-016（回复门）、K-020（ping 透明拦截）、K-024（分类器纯函数）、K-050（hook_entry 16 项全表）、K-051（表驱动 + 注册集合=能力集合）、K-052（pressed 双检宏）、K-053（do_fkey_pressed 全流程，含 events 失败带病继续的 C 事实）、K-054（key_name→静态 str）、K-055（mapping_dmp 自描述）、K-056（F1 旅程图）、K-057（dispatch_each 零分配）、K-058（16 长度耦合→编译期断言）、K-059（run_dump 16 臂形态）、K-200（DumpState 接缝部分：游标实例挂在 IsServer 上，模式详解回指 05）。
- **不讲什么**：注册时机与关闭（→01）；协议线格式（→02 回指）；各转储函数内部（→05~09）；生产 transport（→10）。
- **前置**：01、02。
- **后置**：04（转储体要取数）、05~09（分派终点）、10。
- **事实底线**：main.c:44-70/121-130/135-146；dmp.c:14-40/70-98/103-114/120-132；dispatch.rs 全文；lib.rs:146-158 区段（handle_fkey_pressed/matched）。
- **知识点清单**：上列 22 条。
- **验收标准**：①L1-L8 每步读者可复述；②"同时按 F1+F3 会发生什么"能从代码推出（无 break 全表扫描→两个转储顺序执行→一次 EDONTREPLY）；③分类器真值表（notify×{TTY,其他}∪请求）穷举与 dispatch.rs 测试对得上；④E2/E3 勘误修复；⑤"一表三用"（注册/分派/自描述）作为收束概念出现且只出现一次。行数 ≤520。

### 04-is-data-acquisition.md

- **一句话定位**：转储域的统一框架：IS 拿数据的六条通道、两种拷贝方向（写进调用方 vs 回复槽携带）、每条通道的门控与失败语义，以及 Rust trait 如何表达"把表给我"。
- **讲什么**：K-070（为什么多通道）、K-071（sys_getinfo：GET_* 24 值用 8、endpt=SELF 语义）、K-072（diagctl STACKTRACE）、K-073（kerninfo 直读史 + A-3 + E-ISKMESS 替代设计）、K-074（getsysinfo 三段式：size 精确匹配/root 门/sys_datacopy）、K-075（vm_info 值通道 + 游标分批）、K-078（clock uptime 第六通道）、K-076/K-077（trait 形态与类型化出参设计）、K-081（producer 布局对齐的契约面：快照即 wire 提案）、K-154 通道半（M1 值通道的机制定义处）。
- **不讲什么**：各请求号驱动哪个转储的渲染细节（→05~09）；生产客户端实装状态（→10，本篇只声明"生产实现存在，逐腿状态见 10"）；GET_* 消费字段表（→05 等）；kernel 侧 do_getinfo 实现（→01-stage-kernel/对应篇，指针）。
- **前置**：01、02、03（知道取数发生在 run_dump 臂内）。
- **后置**：05~09 全部；10；99。
- **事实底线**：com.h:236/315-345/729-734/874-877 邻域；sysinfo.h:11-17；dmp_kernel.c/dmp_vm.c 各调用点；acquire.rs（trait 定义区 :100-180 量级，B 相以现码为准）；edge_todo.md:583-612（E-ISPROD/E-ISKMESS 契约文本）。
- **知识点清单**：K-070~K-081 池内 12 条 + K-154 的通道半跨引。
- **验收标准**：①通道总表（六通道 × 拷贝方向 × 门控 × 失败语义 × 消费者）每格带锚点；②读者能回答"为什么 kmessages 不能像 C 那样直接读"（A-3 用户态映射移除）与"为什么 vm_info 不回 -ENOTSUP 于 STATS"（现状：STATS/USAGE 真通、REGION 缺编码——状态句指向 10）；③"size 精确匹配"契约有正反例（错误尺寸→生产者拒答路径，getsysinfo 消费端行为）；④测试场景表含 fake 数据注入用例（无计数）。行数 ≤560（旧文 492 + 吸收注记中的设计事实）。

### 05-is-dump-kernel.md

- **一句话定位**：并行体的代表篇：以一个域（8/16 席）讲透转储的完整流水线——共享地基取数、快照/编码器/格式常量三件套、分页游标基准形、PCStr 格式化、诊断 sink——后续四篇只讲与本 Baseline 的差异。
- **讲什么**：K-090（8 转储清单+动机）、K-091（三表全局拷贝）、K-092（LINES=22/--more-- 协议主讲述点）、K-093（PROCLOOP/PRINTRTS）、K-094（PageCursor 基准形 + **游标五形对照总表**主讲述点）、K-200（DumpState 主讲述点：C static→Rust 分域字段）、K-095（kmess_start 环形）、K-096（expand_newlines 含尾换行判例）、K-097（irqtab 双表 + **USE_APIC 构建耦合**新增半）、K-098（照录纪律判例：表头名实不符/kenv 取而不用）、K-099（三编码器 A-12）、K-100（privileges 位图列）、K-101（arm 死分支处置判例）、K-102（procstack 循环内取数第一例）、K-103（proc_name→NameClass）、K-104（A-4 快照子集模式主讲述点）、K-105（PCStr）、K-106（A-6 sink 定型主讲述点 + render_kernel）、K-107（死宏与排除就地登记）、K-202（渲染三案裁决）。
- **不讲什么**：fkey/分派（→01~03）；通道机制本身（→04，本篇只标每转储用哪条）；PM/VFS/RS/DS/VM 域布局（→06~09）；生产接线细节（→10）；struct proc 内核侧完整字段（→01-stage-kernel/06 进程表篇，指针）。
- **前置**：04（+主线全部）。
- **后置**：06~09 的"与 05 差异表"全部以本篇为基准；99（排除项汇总）。
- **事实底线**：dmp_kernel.c 全文件（396 行，逐函数：:42 死宏、:55-57 三表、:62-88/:93-116/:121-163/:168-185/:191-213/:252-295/:318-353/:358-380/:385-395）；Makefile:18-20（USE_APIC）；dump_kernel.rs（963 行）；lib.rs:63（PCStr）/:97（DumpState）。
- **知识点清单**：上列 22 条。
- **验收标准**：①"取数→快照→游标→编码→格式化→sink"管线图完整且每环节在本篇有实例；②游标五形对照总表（05 行指针形/06 候选断/07 三态/08 过滤/09 双游标）读者可用它预测任一域篇内容；③三个"照录判例"（K-098/K-101/环形起点）与一个"修复判例"（K-141 STR，在 08 篇讲，此处引用）齐备；④任一内核转储（选 proctab）能从 C 逐段走到 Rust render_kernel 对应段；⑤测试场景表含 PageCursor 回绕/满页/续跑三态（无计数）。行数 ≤700（代表篇，用户软约束允许）。

### 06-is-dump-pm.md

- **一句话定位**：PM 域差异篇：两张 getsysinfo 表（mproc/sigaction）相对 05 基准的四点差异——双请求号、11 位解码、回绕算术、第二形游标。
- **讲什么**：K-110（两转储）、K-111（flags_str 11 位）、K-112（MProcSnap 与 464B→76B 对齐史实）、K-113（PmCursor 差异行）、K-114（alarm 回绕减法 + uptime 消费端）。
- **不讲什么**：游标通用模式（→05 回指）；PM 内部语义（→04-stage-pm）；通道机制（→04）；生产真装状态（→10 一句）。
- **前置**：05。
- **后置**：99（SI_* 常量对账行）。
- **事实底线**：dmp_pm.c:21-39/41-73/75-109；dump_pm.rs（346 行）；sysinfo.h SI_MPROC_TAB/SI_SIGTAB_SET 邻域；minix-types MProcSnap 上移后权威位置（edge3.md:59 片 3b-2 实测）。
- **知识点清单**：K-110~K-114 五条。
- **验收标准**：①"与 05 差异表"四行齐（请求号×2、游标形、时钟列、快照权威位置在 minix-types 而非 IS 本地）；②回绕减法"不超前"语义有算例；③06:32-36 重复小结事故消除；④测试场景表（无计数）。行数 ≤280。

### 07-is-dump-vfs.md

- **一句话定位**：VFS 域差异篇：变长 fd 段（每进程 nfds 不同）如何打破 05 定长行游标假设，催生三态游标与快照契约扩展。
- **讲什么**：K-120（两转储）、K-121（FP 位/blocked_on 枚举 + SDEV 缺口继承）、K-122（双快照 + nfds/fp_cdev_endpt 扩展判例）、K-123（fd 计数切片）、K-124（VfsAction 三态差异行 + "接线时 bool 不够用"的演化动因叙事——以事实句写，不写成开发日志）。
- **不讲什么**：VFS 内部语义（→05-stage-vfs）；通用游标（→05）；两腿生产应答缺口（→10）。
- **前置**：05。
- **后置**：99。
- **事实底线**：dmp_fs.c:25-64/67-83；dump_vfs.rs（375 行）；fproc.h 位定义（E4 勘误任务在此篇执行）；edge3.md:59 ②（VFS 腿挂他处事实）。
- **知识点清单**：K-120~K-124 五条 + E4 验证任务。
- **验收标准**：①差异表含"唯一含变长段的域"点题且回答"页边界落在 fd 段中间怎么办"；②E4 锚点勘误修复留痕（fproc.h 符号双向对账输出）；③blocked_on 枚举与 C 打印行为对拍表；④测试场景表（无计数）。行数 ≤280。

### 08-is-dump-rs-ds.md（新合并篇）

- **一句话定位**：两个注册类服务的小域合篇：RS 花名册（状态位双源解码、IN_USE 过滤游标）与 DS 仓库盘点（四类型行格式、条件界游标），以及"快照契约遇到执行面"的两个人类学样本（r_args 翻转、STR bug 偏离）。
- **讲什么**：K-130（rproc_dmp+IN_USE）、K-131（s_flags_str 双源 6 位）、K-132（RprocSnap r_args 决策翻转判例）、K-133（RsCursor 第四形）、K-134（**验证任务**：R31 归属裁决）、K-140（data_store_dmp+四类型）、K-141（STR C bug 修复判例，双向存证纪律）、K-142（DsCursor 第三形差异行）、K-143（DsEntrySnap/A-10 消费）、K-144（缺省中止）。
- **不讲什么**：DS 服务端发布/订阅语义（→07-stage-ds）；RS 重启策略（→03-stage-rs）；游标通用（→05）。
- **前置**：05（+04）。
- **后置**：99（SI 常量对账、K-134 裁决结果登记）。
- **事实底线**：dmp_rs.c:26-58/61-73；dmp_ds.c:9-51；dump_rs.rs（202 行）/dump_ds.rs（277 行）；ds_store.rs:8 注释（K-141 双向存证一侧）；03-stage-rs/todo.md:120/203（R31 登记面）。
- **知识点清单**：上列 10 条。
- **验收标准**：①两域各成一半、各自"与 05 差异表"完整；②两个判例（K-132/K-141）能作为"什么时候允许偏离 C"的对比教学（一个改快照、一个改输出且注释存证）；③R31 裁决有明确结论写进篇末边界节（C 无对应物→归还 03-stage-rs 并回写 edge_todo，或发现对应物→说明在哪）；④测试场景表（无计数）。行数 ≤380。

### 09-is-dump-vm.md（旧 10 重编号）

- **一句话定位**：并行体最复杂成员收官篇：不定长数据的三件套对策（分摞取/折叠/双游标续抄），FoldState 与 BatchCursor 两台纯状态机是全 stage 状态机设计的顶点。
- **讲什么**：K-150（三快照+PROT）、K-151（FoldState 全语义：四等/吸收/延迟打印不含首个/NULL 双用/跨表残留照录/重喂契约）、K-152（BatchCursor：双游标/迭代尾清基/容量预检让位/防御内错/擦除行）、K-153（VM_LINES=24 差异 + 两空行计数）、K-154 消费半（M1 槽字段读取表）、K-155（prot_chars）、K-156（渲染中取数唯一形态）。
- **不讲什么**：VM 内部（→02-stage-vm）；REGION 生产侧 D7 编码缺口的补齐（→10/26-vm-queries）；通道机制（→04）。
- **前置**：05（游标对照表）、04（region 三元组）。
- **后置**：10、99。
- **事实底线**：dmp_vm.c 全文件（157 行；:11-50/:59-80/:83-155/:150-155）；vm.h:40-71；mman.h:62-65；dump_vm.rs（483 行）。
- **知识点清单**：上列 7 条。
- **验收标准**：①折叠机状态转移图（Buffered/FlushRepeat/FlushRegion/FlushEnd）+ 每转移的 C 行号；②"首屏容量预检失败时 header 都不打"的可观测后果讲清；③与 05 差异表（LINES、取数时机、双游标键、折叠独有）四行；④旧 10 的重复小结/交付状态叙事/补丁注记全部消除；⑤测试场景表（无计数）。行数 ≤320。

### 10-is-production-wiring.md（新建，支线）

- **一句话定位**：实现交接面：教学主线（01~09）用 fake trait 成立，本篇记录生产形态——三个 transport 真装、六腿取数客户端、诊断出口、测试基建与唯一台账、以及四张 edge 欠账的当前状态。
- **讲什么**：K-026（transport 三件套 + main 装配）、K-079（六腿客户端全景：七 what/diagctl/uptime/kmess 10008B/SysGetsysinfo/VM M1）、K-080（ENOSYS/-ENOTSUP 诚实出口纪律）、K-041/K-025 实装半（fkey_ctl_via、warn_fkey_ctl 通道）、K-106 通道实装半（diag_out→SYS_DIAGCTL）、K-154 REGION 缺口半、K-201（测试基建 + **全 stage 唯一测试台账**）、K-202 实装对照、K-203（E-ISBOOT 状态面）、遗留缺口一览（RS/DS/VFS 腿端到端归 S33/S12；REGION 编码归 02-stage-26；E-ISKMESS kernel 侧余件）。
- **不讲什么**：任何机制的第一次定义（全部回指 01~05/04）；C 侧行为（C 无生产接线对应物）；未来计划（归 todo/edge 材料，本篇只写"已实装/未实装/归他处"三态事实）。
- **前置**：01、03、04（其余可跳读）。
- **后置**：99（对账行消费）。
- **事实底线**：main.rs:1-35；sef.rs SysSefTransport 区段；tty_fkey.rs SysFkeyCtl；acquire.rs Sys* 实现区段（1523 行内）；Cargo.toml 依赖与门控注释；edge3.md:59；edge_todo.md:560-620。
- **知识点清单**：上列 10 条目（含四张 edge 状态行）。
- **验收标准**：①每腿一行的"生产状态表"（通道/实装/缺口/归属）全部带 grep 级锚点；②测试台账唯一（`grep -c '#\[test\]'` 输出快照 + 采集日期，声明其余篇禁计数）；③读者能回答"现在的 minix-is 在真实系统里跑到哪一步"（E-ISBOOT 三态）；④无开发流水账文风（状态表+事实句，不是日志）。行数 ≤450。

### 99-is-global-concepts.md

- **一句话定位**：全 stage 对账与纪律索引：常量权威表、错误码面、执行模型纪律、排除项与死代码总账、ARCH 编号（A-1..A-12）权威表。
- **讲什么**：K-170（9 族常量对账表，直接吸收 todo §1.2 并更新 SI_PROC_TAB 重复定义现状——片 3b-2 后 MProcSnap 已上移，vfs misc.rs:66 案例复查更新）、K-171（EDONTREPLY/errno 纪律）、K-172（单线程事件循环 + 零分配纪律）、K-173（排除项总账：glo.h 死 extern×5、DIAG_BUF_SIZE、_SYSTEM、proto.h、死宏、arm 分支、`#if 0` 块）、K-174（被引用登记，指向 §8.2 类清单的现码实况）、K-175（A-1..A-12 表：编号/决策/兑现状态/主讲述篇）、K-021 对账半（LU WONTFIX 行）。
- **不讲什么**：任何机制教学（全部指针到主讲述篇）；plan.md 的历史规划（plan 保留原位，99 只收"仍有效的决策账"）。
- **前置**：00（跳读友好：术语在各主讲述篇有定义，99 按表索引用）。
- **后置**：无（收口篇）。
- **事实底线**：todo.md §1.2（对账表源数据，需逐行按当前代码复核后吸收）；glo.h/inc.h（本次实测）；errno.rs:101；lib.rs:1；plan.md §4/§5.4（A 编号与排除表原料）。
- **知识点清单**：K-170~K-175 六条 + K-021 对账行。
- **验收标准**：①每张表逐行可机械复核（附验证命令列）；②A-1..A-12 每条有"兑现于哪篇哪节"回链；③Gate A 复跑口径写明（tools/coverage-extract 命令原文）；④行数 ≤350。

---

## 6. 变更表

> 操作类型：重排 / 拆分 / 合并 / 新建 / 归档 / 就地重写。所有 12 篇旧文档最终整体归档（B 相不删除文件，移出正式目录）；下表"新位置"即知识点新家。去向规则：存量写去向、新增写来源，双向均已满足（无"写不出去向的拆"）。

| # | 操作 | 旧位置 | 新位置 | 理由 | 涉及知识点 |
|---|------|--------|--------|------|------------|
| O-01 | 就地重写 | 00 | 新 00 | 结构成立、正文 pending；导航表按 §4.1 重制，§2 转述 plan 改为 C 自证 | K-001/002/003 |
| O-02 | 拆分 | 01 §2.1-2.2/2.6-2.8/2.11、§3.1-3.2/3.4-3.5 | 新 01 | 启动/关闭段独立成语义单元 | K-010~012/017~026(部分)/035/203 |
| O-03 | 拆分+合并 | 01 §2.3-2.5/§2.9-2.10/§3.3/§5.1 + 旧 03 全篇 | 新 03 | 循环段真序（L1-L8）本是一体：分类器与 hooks 分派合篇 | K-013~016/020/024 + K-050~059/200(接缝半) |
| O-04 | 节移新建 | 01 文末"接线（S23 片 1/2）" | 新 10 | 开发进度/实现交接事实移出教学篇 | K-026 |
| O-05 | 就地重写 | 02 | 新 02 | 骨架成立；修 E1、位图汇编主讲述点移交 01、§2.7 压缩为契约表、补丁注记融入 | K-030~042 |
| O-06 | 就地重写 | 04 | 新 04 | 骨架成立；增第六通道（uptime）、§2.4g 常量权威移交 99、V1 数据出口设计融入正文 | K-070~081/154(通道半) |
| O-07 | 就地重写+升格 | 05 | 新 05 | 升为并行体代表篇：吸收游标对照总表（原散在 09/10）、DumpState、渲染三案、A-6/PCStr 集中处 | K-090~107/200(主)/202 |
| O-08 | 就地重写 | 06 | 新 06 | 修重复小结（:32-36）、改"与 05 差异表"组织 | K-110~114 |
| O-09 | 就地重写 | 07 | 新 07 | 同上 + E4 勘误任务 | K-120~124 |
| O-10 | **合并** | 旧 08 + 旧 09 | 新 08（rs-ds 合篇） | 两域各 2/1 个 C 函数、独立成篇信息量不足；互引密集（游标对照）；合并理由详见 §4.2.3 | K-130~134 + K-140~144 |
| O-11 | **重编号** | 旧 10 | 新 09 | 腾 10 号给新建支线篇；转储域编号连续 | K-150~156 |
| O-12 | **新建** | —（原料：旧 01 接线节、旧 03~10 V1 注记、todo §8、edge3.md:59、edge_todo E-IS* 四条） | 新 10 | §4.2.5 | K-026/079/080/201/203 + K-025/041/106/154 实装半 |
| O-13 | 就地重写 | 99 | 新 99 | pending 骨架转正式收口：吸收 todo §1.2 对账表、旧 00§4、各篇排除小节、plan A 编号 | K-170~175/021(对账半) |
| O-14 | 删除（非知识点） | 03 §4.4 补丁块、10 §6 交付状态段、06:32-36 与 10:31-35 重复小结、各篇 §5.3 计数行 | — | 过程注记/事故/易漂移台账不属正式文档；事实部分已分别融入新 01/03/05~09 正文与新 10 台账 | — |
| O-15 | 归档 | 全部 12 篇旧文件 | archive/（B 相执行） | 重建方法论（内容按契约重写，不搬段落） | 全部 |

**存量覆盖核对**：旧 12 篇的每个章节均出现在 O-01~O-15 或 §8.1 迁移表；池 110 条中 109 条有确定新家，1 条（K-134）新家在"新 08 验证任务"（裁决后二选一，两分支均有归属——不构成无去向）。

---

## 7. 缺漏新篇（固定十项逐项落实；详表见 §3.5，此处落到篇章与验收）

| 项 | 落实 | 原料在哪里 | 归哪篇 | 验收标准 |
|----|------|------------|--------|----------|
| 链接与加载 | 讲 | Makefile（本次实测全文）；os/servers/is/Cargo.toml | C 侧→新 01 §2 尾 + 新 05（USE_APIC）；Rust 侧→新 10 | 每处提及带文件行号锚点；不复制构建教程 |
| 镜像与内存布局 | 讲（IS 消费面）| get_minix_kerninfo 调用点；vm.h/com.h 结构 | 新 04 §2.3 + 各域布局子集节 | 六布局子集表齐全且与 99 对账行一致 |
| 汇编入口与陷阱进入 | **不做** | — | — | 理由：用户态服务无汇编入口；`_start`/陷阱归 01-stage-kernel 与库启动链。新 01 边界节写一行指针即验收 |
| 启动装配 | 讲 | rc.minix:117、system.conf:271-277、table.c:44-64、edge_todo E-ISBOOT | 新 01 §2（C 事实）+ 新 10（Rust 三态） | S1-S2 可复述；Rust 侧"现在能不能真启动"有明确答案行 |
| 构建与工具链 | 讲（耦合面）| Makefile:11-20；Cargo.toml 门控注释 | 新 05（USE_APIC）+ 新 10（real-trap 门控）；xtask 镜像→**不做**（19-stage-integration，99 记指针） | USE_APIC 一句因果（IRQ 表尺寸）带锚点 |
| 跨模块接口与线格式 | 讲 | com.h/ipc.h/sysutil.h/keymap.h；A-4 快照；M1 槽 | 新 02（fkey 线格式）+ 新 04（拷贝方向与请求号）+ 新 05（快照模式） | 三条线：消息内联值 / 调用方缓冲 / 回复槽携带，各有正反例 |
| 错误路径 | 讲 | main.c:59-63；dmp.c:63-64/83-85；panic 分支；ENOSYS/-ENOTSUP 现状 | 新 01（注册告警不变量）、新 03（分类/带病继续/panic）、新 10（诚实出口纪律） | 每条失败路径回答"谁看到、看到什么" |
| 关闭与退出 | 讲 | main.c:107-116；do_fkey_ctl UNMAP | 新 01（顺序不变量）+ 新 02（协议闭环） | "不注销就退出会发生什么"有推理+标注边界 |
| 并发与同步 | 讲（纪律级） | lib.rs:1 零分配；AGENTS.md 执行模型 | 新 99（纪律条目）| 明确"本 stage 为何无锁无 Arc"三行论证 |
| 测试基建 | 讲 | 各 .rs test 模块（grep 实测 125）、FakeTransport/FakeFkey/FakeAcquires、hosted EIO（edge3.md:59） | fake 模式首次引入处→新 01；台账与门控→新 10 | 台账唯一；各篇只留场景表；`#[test]` 计数快照带采集日期 |

另落实三项非清单缺口：**DumpState**（新 05/03，K-200）、**Live Update WONTFIX 对账**（新 01 事实 + 新 99 账目，K-021）、**R31 归属裁决**（新 08 验证任务，K-134——此项 B 相执行前必须落结论，不许带"待验证"进正式目录）。

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表（逐节；小节清单来自 `grep -n "^# "` 实测输出）

> 迁移类型：搬改 = 按新契约重写正文后落位；移交 = 主讲述点换篇；删除 = 见 O-14 理由。旧文件整体归档后，表内"新位置"即所有引用的重定向目标。

**旧 00**（7 节）：

| 旧位置 | 旧内容一句话 | 新位置 | 类型 | 断链备注 |
|--------|--------------|--------|------|----------|
| 00§1 | IS 是调试转储聚合器 | 新 00§1 | 搬改 | — |
| 00§2 | 启动主线（转述 plan） | 新 00§2 | 搬改（换 C 自证，§1 真序表） | 禁止再引 plan 结论充当证据 |
| 00§3 | 无 boot_image 登记 | 新 01§2 | 移交 | — |
| 00§4 | 执行模型 | 新 99§3 | 移交 | — |
| 00§5 | 文档导航 | 新 00§4 | 搬改（按 §4.1 重制） | — |
| 00§6/§7 | 过渡/参见 | 新 00 末 | 搬改 | — |

**旧 01**（40 节含小节）：

| 旧位置 | 旧内容一句话 | 新位置 | 类型 | 断链备注 |
|--------|--------------|--------|------|----------|
| 01 头部声明块 | 源码/Rust/draft/位置四行 | 新 01 头部 | 搬改（draft 引用按新禁令处置） | 各篇头部都引 `draft/tmp_*.md`，新目录是否保留该行→裁决项 Q-5 |
| 01§1/1.1/1.2 | 为什么调试是独立服务+火灾报警器类比 | 新 00§1（概述级）+ 新 01§1（动机级） | 拆分 | 类比只留一处 |
| 01§1.3 | 边界声明 | 新 01 头部 | 搬改 | — |
| 01§2.1 | 四静态全局 | 新 01§2 | 搬改 | state.rs:1 引用重定向 |
| 01§2.2 | main 三段式 | 新 01§2（启动）+ 新 03§2（循环） | 拆分 | — |
| 01§2.3 | notify 两层分类+FIXME | 新 03§2 | 移交 | dispatch.rs:40/68 引用重定向 |
| 01§2.4 | 非 notify 告警 | 新 03§2 | 移交 | — |
| 01§2.5 | 回复门与 reply | 新 03§2 | 移交 | — |
| 01§2.6 | sef_local_startup 四注册 | 新 01§2 | 搬改（扩 LU 语义 K-021） | — |
| 01§2.7 | init_fresh 一行 | 新 01§2 | 搬改 | — |
| 01§2.8 | signal_handler 只认 SIGTERM | 新 01§2 | 搬改 | — |
| 01§2.9 | get_work | 新 03§2 | 移交 | — |
| 01§2.10 | ping 透明拦截 | 新 03§2 | 移交 | — |
| 01§2.11 | 启动条件 | 新 01§2 | 搬改（扩 K-010 全链） | — |
| 01§3.1 | D1 全局→IsServerState | 新 01§3 | 搬改 | — |
| 01§3.2 | D2 trait 缝 | 新 01§3 | 搬改 | sef.rs:1/254-262 引用重定向 |
| 01§3.3 | D3 分类器纯函数+02/03 留桩 | 新 03§3 | 移交（桩史删除） | 含 E1 关联的 NHOOKS 错锚，修复 |
| 01§3.4 | D4 告警 log 抽象 | 新 01§3（不变量）+ 新 10（A-6 选型） | 拆分 | — |
| 01§3.5 | D5 生命周期与错误面 | 新 01§3 | 搬改 | — |
| 01§4.1/4.2/4.3 | 模块树/签名/不变量 | 新 01§4 与新 03§4 | 拆分 | 11 处代码文件头注释的主要重定向源 |
| 01§5.1 | 分类器真值表 | 新 03§5 | 移交 | — |
| 01§5.2 | 生命周期测试 | 新 01§5 | 搬改 | — |
| 01§5.3 | 测试计数 | 新 10 台账 | 移交+删计数 | — |
| 01§6/§7 | 过渡/参见 | 新 01 末 | 搬改 | — |
| 01 末"接线（S23 片 1/2）" | 生产 transport 落地记 | 新 10§2 | 移交 | 与 edge3.md:59 去重：文档留状态表、流水账归 edge |

**旧 02**（22 节）：§1(1.1-1.4)→新 02§1（1.3 三层编号保持主讲述点）；§2.1-2.5→新 02§2 同名序；§2.6→**拆分**：位图汇编动作移交新 01§2，编号换算留新 02§2（修 E1 错锚）；§2.7→新 02§2（压缩为行为承诺表）；§2.8→新 02§2（一行版，完整版指针 12-stage-input）；§2.9→新 02§2（闭环收束）；§3.1-3.5→新 02§3（D1 加 99 对账行、D3 加 10 指针）；§4.1-4.3→新 02§4（lib.rs:117、sef.rs:124 引用点）；§5.x→新 02§5 + 新 10 台账。断链备注：tty.rs:4（minix-sys）文件名不变仅验节号；02 是本 stage 被代码引用第二多的篇。

**旧 03**（24 节）：§1(1.1-1.3)→新 03§1（吸收"分类器也在 03"后的完整边界句）；§2.1→新 03§2（修 E3）；§2.2-2.5→新 03§2（§2.5 修 E2）；§2.6→新 03§2（与 §1.3 真序表对拍后重绘）；§3.1-3.6→新 03§3（V1 事实融入：D6"空体+恒抑制"改为实装后叙述）；§4.1-4.3→新 03§4；**§4.4 双文档同步→删除**（O-14）；§5/5.3→新 03§5 + 新 10。断链备注：dispatch.rs:3/7/53、lib.rs 内多处分派注释重定向（含文件名变更）。

**旧 04**（含 §2.1-2.5、§2.4g、§4.2b 等；约 25 节）：§1→新 04§1；§2.1-2.5→新 04§2（各加"生产形态见 10"一行；增 §2.6 clock uptime 第六通道 K-078——原缺）；**§2.4g 常量权威位置→移交新 99§1**（集中化，消除与各篇 §4.2 尾注的重复）；§3 D 系→新 04§3（D2 细缝、数据出口设计融入正文，删 V1 补丁块形态）；§4.1/§4.2/§4.2b/§4.3→新 04§4（acquire.rs:1 注释重定向）；§5/5.3→新 04§5+新 10；§6（通道×消费者总表）→新 04§6（更新为六通道，K-154/K-079 状态列移交新 10）。断链备注：05~10 与 edge_todo 大量引"04 §2.x"——新 04 §2 保持通道顺序则节号近稳，B 相按新目录重排后逐条复核（§8.2 R 类）。

**旧 05**（25 节）：§1→新 05§1；§2.1-2.11→新 05§2（§2.4 增 USE_APIC 锚 K-097；§2.12 排除保留就地+汇入 99）；§3.1→新 05§3（A-4 全 stage 模式定义处）；§3.2-3.7→新 05§3（D7"体延后"翻转为正文；增游标对照总表、DumpState K-200、渲染三案 K-202——三条**新增**自 todo §8/edge3）；§4.1/4.2/4.3→新 05§4；§5/5.3→新 05§5+新 10；§6/7→新 05 末。断链备注：dump_kernel.rs:1 文件名不变。

**旧 06**（17 节）：§1（含 :32-36 重复小结）→新 06§1（删重复）；§2.1-2.5→新 06§2；§3.1-3.5→新 06§3（D5 翻转正文；游标只留差异行）；§4/§5/5.3→新 06§4/§5+新 10；§6/7→新 06。同旧 07（结构同构，另加 E4 任务）。

**旧 08**（18 节）与**旧 09**（18 节）→ 合并映射到新 08：旧 08 §1→新 08§1（合篇定位句重写）；旧 08 §2.1-2.3→新 08§2（RS 半）；旧 08 §3.1-3.3→新 08§3；旧 09 §1→新 08§1（DS 半）；旧 09 §2.1-2.2/2.4→新 08§2（DS 半）；**旧 09 §2.3 游标三形状对照→新 05§3 对照总表**（移交+合并，主讲述点变更）；旧 09 §3.1-3.4（含 §3.4 内 STR V1 注记）→新 08§3（注记融入正文，K-141）；两篇 §4/§5/5.3/§6/§7→新 08 对应节+新 10。断链备注：**最高危**——`dump_rs.rs:1`、`dump_ds.rs:1`、`ds_store.rs:8` 三处代码注释指向旧 08/09 文件名，必须迁移（§8.2 R-12/R-22/R-25）；其余 stage 引"09 篇三游标"的文字（旧 10 §7）随旧 10 重写自动更新。

**旧 10**（24 节）→ 新 09：§1（含 :31-35 重复小结）→新 09§1（删重复）；§2.1-2.5→新 09§2；§3.1-3.5→新 09§3（D5 翻转；§3.1 的 [ARCH: 26-D1/D7] 引注保留但改为跨 stage 路径引用）；§4→新 09§4；§5/5.3→新 09§5+新 10；**§6"阶段完成/10 篇全 reviewed"→删除**（O-14）；§7→新 09 末（"09 §2.3"改指新 05）。断链备注：dump_vm.rs:1 文件名从 10- 改 09-（R-18）。

**旧 99**（6 节）：§1→新 99§1（吸收 todo §1.2 + 旧 04§2.4g）；§2→新 99§2；§3→新 99§3（吸收旧 00§4）；§4→新 99§4（吸收各篇排除小节成总账）；§5→新 99§5（按 §8.2 现码实况更新）；§6→新 99 头部。

### 8.2 引用迁移表（stage 外入链；本次 grep 实测全集）

| # | 引用方 | 现引用 | 新目标 | 动作 | 验证方式 |
|---|--------|--------|--------|------|----------|
| R-01 | os/servers/is/src/state.rs:1 | 01-is-init-main.md §4.1 | 01-is-startup.md §4 | **迁移**（文件名变） | rg "is-init-main" os/ 归零 |
| R-02 | tty_fkey.rs:1 | 02-is-fkey-contract.md §4.1 | 同文件 §4 | 验节号 | 人工比对 |
| R-03 | tty_fkey.rs:17 | 02 §1.3 | 同文件对应节 | 验节号 | 同上 |
| R-04 | tty_fkey.rs:168 | 01-is-init-main.md §3 D2 | 10-is-production-wiring.md（接线事实）或 01 §3 | **迁移** | rg 归零 |
| R-05 | tty_fkey.rs:196 | 02 §3 D3（wiring pending panic 注释） | 注释本身疑陈旧（片 1 已真装，本次 grep 仍命中——B 相核对现码后改注释+引用） | **修注释** | cargo test + 人工 |
| R-06 | acquire.rs:1 | 04 §4.1 | 同文件 §4 | 验节号 | 人工 |
| R-07 | dispatch.rs:3 | 01 §4.1（classifier） | 03-is-mainloop-dispatch.md | **迁移**（两处文件名均变） | rg 归零 |
| R-08 | dispatch.rs:7 | 03-is-dump-dispatch.md §4.1 | 03-is-mainloop-dispatch.md §4 | **迁移**（文件名变） | rg 归零 |
| R-09 | dispatch.rs:40/68 | 01 §2.3 | 03 新篇对应节 | **迁移** | rg 归零 |
| R-10 | dispatch.rs:53 | 03-is-dump-dispatch.md | 新 03 文件名 | **迁移** | rg 归零 |
| R-11 | dump_rs.rs:1 | 08-is-dump-rs.md §4.1 | 08-is-dump-rs-ds.md | **迁移** | rg 归零 |
| R-12 | dump_ds.rs:1 | 09-is-dump-ds.md §4.1 | 08-is-dump-rs-ds.md（DS 半） | **迁移** | rg 归零 |
| R-13 | os/libs/minix-types/src/types/ds_store.rs:8 | 09-is-dump-ds.md §3.4 | 新 08 §(STR 判例节，B 相定位) | **迁移** | rg 归零 |
| R-14 | main.rs:4、lib.rs:7-8 | 01-is-init-main.md | 01-is-startup.md | **迁移** | rg 归零 |
| R-15 | sef.rs:1 | 01 §4.1 | 01-is-startup.md §4 | **迁移** | rg 归零 |
| R-16 | sef.rs:124 | 02 §4.3 | 01-is-startup.md（caller-warns 不变量新家）| **迁移** | rg 核对 |
| R-17 | sef.rs:254/258/262 | 01 §3 D2（wiring pending） | 新 10（疑陈旧，同 R-05 处置） | **修注释+迁移** | 人工 |
| R-18 | dump_vm.rs:1 | 10-is-dump-vm.md §4.1 | 09-is-dump-vm.md | **迁移**（编号移位最高危例） | rg 归零 |
| R-19 | dump_pm.rs:1 / dump_vfs.rs:1 / dump_kernel.rs:1 | 06/07/05 各 §4.1 | 同号新篇 | 验节号 | 人工 |
| R-20 | lib.rs:117 | 02 | 02 | 无 | — |
| R-21 | lib.rs:123 | "five data-acquisition channels" 注释 | 新 04 六通道口径 → 注释同步改 | **修注释** | 人工 |
| R-22 | os/libs/minix-sys/src/tty.rs:4 | 02-is-fkey-contract.md | 同文件 | 无 | — |
| R-23 | os/servers/rs/src/shell_request.rs:1048 | "IS dump face is 08-stage-is" | 目录级 | 无 | — |
| R-24 | notes/…/04-stage-pm/04-ipc-dispatch.md:101 | `08-stage-is`（目录级） | 无 | — |
| R-25 | 00-master-plan/README.md:26/74 | 目录级 | 无 | — |
| R-26 | edge_todo.md:609 | "04 篇 §2.1"、"05 篇 §4.1" | 新 04/新 05（节号或已漂移） | **验节号**（edge 材料，低优先） | B 相收尾抽查 |
| R-27 | edge3.md:59 | "01/04 篇"级提及 | 验节号 | 低优先 | 同上 |
| R-28 | 03-stage-rs/todo.md:120/203/577 | "归 08-stage-is dump 面" | 随 K-134 裁决更新归属表述 | **待裁决联动** | 裁决记录 |
| R-29 | 本 stage 内 125 处交叉引用 | "0X §Y" 简写遍布 | 不逐条迁移——B 相全部重写自然再生产；仅"09→08、10→09"两个编号位移在重写时最高危（漏改即错指），验收含全 stage `rg "09 篇|10 篇|09-|10-"` 人工过一遍 | 策略 | §9 检查 1/2 |

### 8.3 断链成本摘要

- **硬迁移（文件名/编号变化导致必改）**：代码注释 12 处（R-01/04/07/08/09/10/11/12/13/14/15/16/17/18 中文件名变更项，热点文件 dispatch.rs ×4、tty_fkey.rs ×2、sef.rs ×4、lib.rs ×2）+ stage 外 md 2 处（R-13 属代码、R-26 低优先）≈ **15 处**。
- **软验证（文件名不变、节号需核）**：代码注释 8 处 + 外部 edge 材料 3 处 ≈ **11 处**。
- **注释陈旧连带修复**：R-05/R-17/R-21 三处（wiring pending 字样疑被 S23 接线淘汰，本次 grep 仍命中文本、实际语义待核代码）。
- **stage 内 ~125 处**：随全篇重写再生产，成本已含在 B 相写作中，不重复计。
- **建议批量方式**：①先落一张 `旧文件名 → 新文件名` sed 映射（01/03/08/09/10 五项变更）跑 `rg -l` 圈定文件；②代码注释改动走 fix-guard（单条、读上下文、cargo test 对照）；③收尾用 `tools/anchor-resolve.sh` / `check-rs-unwired.sh` 类既有工具复跑（本次实测确认 `tools/` 存在该族脚本；B 相按实际用法执行）；④R-13/R-28 两处跨 crate 引用改后 grep 双向验证。
- **成本评估结论**：外部硬断链 15 处、集中于 IS 自家 crate 注释（同批人可修），**断链成本可控，支持重建**（对比 01-stage-kernel I-14 当年"三十节点不敢动"的处境：本 stage 仅 12 篇、入链 33 处，且 2/3 在自己家里）。

---

## 9. 验证与自检门

### 9.1 四种机械检查结果

| 检查 | 结果 | 依据 |
|------|------|------|
| 前向引用扫描 | **通过**。全部契约"前置"字段只指向更早编号（00←∅；01←00；02←01；03←01,02；04←01~03；05←04；06~09←05；10←01,03,04；99←00）。两处易误判项已在契约内消化：01 使用位号（首次出现处完整定义到够用，P-2）；02 讲 UNMAP/EVENTS（协议整体，序差表 P-2 登记，回指补偿 01 末指针） | §5 逐篇前置字段 |
| 依赖图无环 | **通过**。前置关系构成全序链 00<01<02<03<04<05<{06,07,08,09}，10/99 挂更早节点，无回边 | §4.3 |
| 覆盖率 | **通过（1 项带裁决条件）**。池 110 条：109 条有"新篇章+小节级"去向；K-134 去向为"新 08 验证任务"（两分支均有归属），列入待裁决 Q-3。明确删除项单列：O-14（四类，均非知识点） | §6 存量覆盖核对 |
| 断链成本统计 | **完成**。硬迁移 15 / 软验证 11 / 陈旧修复 3 / 内部再生 125；热点与批量方案见 §8.3 | §8 |

### 9.2 自检门逐门

| 门 | 结果 | 说明 |
|----|------|------|
| G1 C 真序逐条可核对 | ✅ | 本次会话重读 main.c/dmp.c 全文并修正行号（旧文摘要中 `sef_local_startup :38-41` 实为 :76-89 声明、hooks 表 :14-35 含 struct 等）；S/L/X 表全部行号来自实测。抽查例：S7 告警不退出（dmp.c:63-64）、L5 无 break（dmp.c:88-94）、X1 unmap→exit（main.c:113-115） |
| G2 知识点池完整性 | ✅ | 8 .c 全读 + 3 .h + Makefile 逐个有归属或排除理由（proto.h/`_SYSTEM`/glo.h externs → K-173）；非 C 制品十项逐答（§3.5）；keymap.h/com.h/ipc.h/sysutil.h/vm.h/mman.h/rc.minix/system.conf/table.c/Cargo.toml 均入锚 |
| G3 前向引用为零 | ✅ | §9.1 检查 1 |
| G4 依赖图无环 | ✅ | §9.1 检查 2 |
| G5 覆盖率 100% / 新增有锚 | ✅（条件项 1） | 新增 4 条全部带代码/edge 锚（K-200 lib.rs:97、K-201 grep 125、K-202 todo.md:256、K-203 edge_todo.md:620）；K-134 标注待验证并给出两分支归属 |
| G6 拆分/合并去向、新建来源 | ✅ | 抽验 6 处：O-02/O-03（01 拆分逐节 §8.1）、O-10（08+09 合并，旧 09§2.3 移交 05 有明确新家）、O-11（旧 10→09 逐节）、O-12（新 10 五路原料清单）、K-097 新增半（Makefile:18-20 实测）、K-010 新增半（system.conf:271-277 实测） |
| G7 契约七要素齐全 | ✅ | 12 篇逐篇：定位/讲/不讲/前置/后置/事实底线/知识点清单+验收，无缺件 |
| G8 迁移表覆盖 | ✅ | §8.1 覆盖 12 篇全部小节（小节清单来自标题 grep 实测输出）；§8.2 覆盖代码注释 + 外部 md 全量（本次 rg 命中集） |
| G9 事实断言带锚 | ✅（推测项已标注） | 全部行号断言本次实测或有旧文读档；未实测项标"待验证"仅 3 处：E4 锚点错配复核、K-134、"IS 崩溃未注销"行为推演（契约里明写标注边界）；R-05/R-17 注释陈旧判断标"疑"待 B 相核码 |

### 9.3 结论

**蓝图完成，可执行**。核心判断：本 stage 旧目录的骨架顺序经 C 真序独立验证成立，重建的重点不是排序，而是**内容再组织**——把 12 篇从"补丁注记 + 台账 + 进度混写"的沉积形态，重写为"启动/协议/循环/通道/代表篇/差异篇/交接篇/收口篇"的线性教学形态；结构性操作仅三处（01 拆循环段、08+09 合并、旧 10 重编号），换来单篇单语义与并行体规则 fully 落地。断链硬成本 15 处且 2/3 位于 IS 自家 crate，低于历史教训（01-stage-kernel I-14）所防范的量级。

### 9.4 待用户裁决的问题

| # | 问题 | 两难 | 本蓝图默认 |
|---|------|------|------------|
| Q-1 | 08+09 是否合并为新 08（rs-ds 合篇） | 合并→语义更强、迁移成本最高（3 处代码注释+编号位移链）；不合并→旧 08/09 各 150 行上下的薄篇保留 | **合并**（§4.2.3 理由） |
| Q-2 | 旧 10 → 新 09 重编号是否接受 | 接受→转储域编号连续、10 号给交接篇；不接受→新建篇改编号 11（阅读路径不变，仅号位审美/11 号位本就空） | **重编号** |
| Q-3 | K-134（R31 欠账）：B 相验证后若 C 确无对应物，是"归还 03-stage-rs"还是"在 08 立一节讲 RS 状态串打印的缺失" | 归还→真相源一致；立节→消化邻 stage 登记 | **归还 + edge 回写**（C dmp_rs.c 无对应物即不虚构） |
| Q-4 | 新 01/新 03 文件名变更（is-init-main→is-startup、is-dump-dispatch→is-mainloop-dispatch）是否随重建生效 | 改名→名实相符但触发 12 处代码注释迁移；保留旧名→零迁移但名不副实（01 已不含 main loop） | **改名**（迁移表已备好） |
| Q-5 | 各篇头部 `draft 素材` 行是否保留 | 保留=draft 继续充当底料入口；删除=正式目录与中间产物彻底切割（符合隐藏文件夹纪律的延伸） | **删除**（B 相取料用 §8.1 表定位旧文档，draft 仅存档） |
