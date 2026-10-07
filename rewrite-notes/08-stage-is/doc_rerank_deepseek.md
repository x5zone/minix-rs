# 08-stage-is 文档重建蓝图（deepseek）

## 0. 元数据

- 执行者：deepseek（AI agent 名称，产物后缀）
- 日期：2026-09-19
- 目标目录：`rewrite-notes/08-stage-is/`
- 仓库根目录：`/home/xzhao/github/minix-rs`
- 当前提交号：`6965e6885`；工作树有未提交修改，数据以工作树实测为准。
- 交付物：本文件。除本文件外不修改、不重命名、不移动、不删除任何文件；未提交任何变更。
- 本轮修订（2026-09-19，补做轮）：订正 §2.4 统计摘要的知识点总数与类型分布（161→146，按 §2.2 池表实测重算）、新增条数（21→20，并剔除未标注新增的 K-082）；订正 §9.1 检查三与 §9.3 结论的同类计数；订正 99 篇事实底线里的文件名（`dump_kernel.c` → `dmp_kernel.c`，仓库内真实文件为 `minix3/minix/servers/is/dmp_kernel.c`）；订正 §9.3 契约计数（13→12，与 §4/§5 的 12 篇一致）。

### 0.1 审查范围

**范围内正式文档（12 篇，全部进入知识点池与重建契约）**：`00-is-overview.md`、`01-is-init-main.md`、`02-is-fkey-contract.md`、`03-is-dump-dispatch.md`、`04-is-data-acquisition.md`、`05-is-dump-kernel.md`、`06-is-dump-pm.md`、`07-is-dump-vfs.md`、`08-is-dump-rs.md`、`09-is-dump-ds.md`、`10-is-dump-vm.md`、`99-is-global-concepts.md`。

**参考材料**：`plan.md`（340 行）、`todo.md`（271 行，含 E-ISPROD/E-ISWIRE/E-ISKMESS 等跨条目记录）、`draft/`（逐行素材）、`00-master-plan/README.md`、`edge_todo.md` 的 IS 相关条目。

**范围外**（只交叉引用）：内核侧 `GET_*` 生产者实现（01-stage-kernel）；PM/VFS/RS/DS/VM 的 `do_getsysinfo` 生产者（各自 stage）；TTY 键盘驱动的完整实现（16-stage-drivers，本 stage 只做观察者协议对端契约）；SEF 库实现（14-stage-runtime/01-stage-kernel）；同目录他人重建产物（不读、不引）。

### 0.2 读取清单

**文档**：上述 12 篇全部（由两个只读探查任务通读并逐条核对锚点；主会话复核承重结论）。另读 `plan.md` 与 `todo.md` 的关键结论行。

**C 源码（ground truth）**：`minix3/minix/servers/is/` 全部（`main.c` 148 行、`dmp.c` 132 行全文精读；`dmp_kernel.c` 396、`dmp_pm.c` 109、`dmp_fs.c` 83、`dmp_rs.c` 74、`dmp_ds.c` 52、`dmp_vm.c` 157 行按函数与锚点核对；`inc.h`/`proto.h`/`glo.h`/`Makefile` 全文）。对端与上游：`com.h`（GET_*/DIAGCTL/VM_INFO/FKEY 常量）、`sysinfo.h`、`callnr.h`、`ipc.h`（fkey 消息段）、`keymap.h`、`kernel/{proc.h,priv.h,type.h}`、`vm.h`、`pm/mproc.h`、`vfs/{fproc.h,dmap.h,const.h}`、`rs/{type.h,const.h}`、`ds/store.h`、`libsys/{getsysinfo,vm_info,sys_diagctl,fkey_ctl}.c`、`kernel/usermapped_data.c`、`etc/rc.minix`、`etc/system.conf`、`kernel/table.c`。

**Rust 实现**：`os/servers/is/src/` 13 个 `.rs`（5,689 行，实测 `#[test]` 115 个）；`minix-types` 的 wire 结构（`types/proc_info.rs`、`types/mproc.rs`、`types/fproc.rs`、`ipc/{sysinfo,tty,vm,kernel_call}.rs`）；生产者侧 `os/kernel/src`、`os/servers/{pm,vfs,rs,ds,vm}/src` 的 `do_getsysinfo`/`GET_*` 实现现状。

### 0.3 证据命令与关键输出

```text
$ wc -l minix3/minix/servers/is/*.c → 148+132+396+109+83+74+52+157 = 1151（8 .c）
$ grep -c '#\[test\]' os/servers/is/src/*.rs → 合计 115
  （state 1 / dispatch 16 / sef 4 / lib 12 / tty_fkey 11 / acquire 15 /
    dump_kernel 23 / dump_pm 9 / dump_vfs 8 / dump_rs 5 / dump_ds 5 / dump_vm 6）
$ grep -rn 'IS_PROC_NR' minix3/ → 0（endpoint 运行时注入，无硬编码）
$ grep -n 'up -n is' minix3/etc/rc.minix → :117（-period 5HZ）；system.conf:271-277
$ grep -n '{DS_PROC_NR\|{INIT_PROC_NR\|"is"' minix3/minix/kernel/table.c → boot_image 44-65 无 is
$ wc -l minix3/minix/servers/is/dmp_kernel.c → 396；dmp.c hooks[] 在 :14-35，NHOOKS :40
$ grep -n 'NR_IRQ_HOOKS\|NR_IRQ_VECTORS' os/kernel/src/syscall_device.rs os/plat/src/interrupt.rs
  → kernel 64 / plat 64；IS 侧按 16（`is/src/lib.rs` Irqtab 臂）→ 尺寸冲突
$ grep -n 'SI_PROC_TAB' os/servers/pm/src/misc.rs → :26-28 本地 0（C/minix-types 为 2）
$ grep -n 'SI_DMAP_TAB' os/servers/vfs/src/misc.rs → :161-165 返回 Inval（未实现）
$ grep -n 'GET_KMESSAGES' os/libs/minix-types/src/ipc/sysinfo.rs → :44 = 7（占 C 缺号 7）
$ git log --oneline -1 → 6965e6885
```

---

## 1. C 真序

### 1.1 阶段类型判定

判定为**服务事件循环型**，且带两个本阶段独有特征：① **条件启动**——IS 不在 boot_image（`kernel/table.c:44-65` 十七项无 `is`），由 `etc/rc.minix:117` 的 `up -n is -period 5HZ` 在调试配置下启动，endpoint 运行时注入（全树无 `IS_PROC_NR`）；② **拉模式转储**——主循环处理的唯一输入是 TTY 功能键通知（无参数），数据靠 IS 主动向五个主权方拉取。因此真序分三段：启动段、主循环段、转储执行段。

### 1.2 真序表

**启动段**：

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| S01 | 启动条件成立：rc.minix 以 `up -n is -period 5HZ` 拉起 | `rc.minix:117`；`system.conf:271-277` | 非 boot 因果链；debug 服务 |
| S02 | RS 加载并启动 IS；SEF 进入 `main(argc,argv)` | `main.c:31-41` | `env_setargs`（`main.c:40`） |
| S03 | SEF 本地启动：四注册 | `main.c:76-89` | init_fresh/init_lu/init_restart 同体 STATELESS；signal_handler |
| S04 | `sef_startup()` → `sef_cb_init_fresh` | `main.c:88,94-102` | 唯一启动动作：`map_unmap_fkeys(TRUE)`（`main.c:99`） |
| S05 | TTY 观察者注册：组装 F/SF 位图并 `fkey_map` | `dmp.c:45-65` | 16 个 hook 键位入位图（`dmp.c:14-35,53-58`） |
| S06 | 进入主循环 | `main.c:44` | IS 不设门控，注册完成即服务 |
| S07 | （生命周期）`-period 5HZ` 由 RS 周期 ping | `rc.minix:117`；`sef.c:sef_receive_status` 拦截（`sef.c:208-214`） | 用户态不可见：ping 被 SEF 库透明处理 |

**循环段**：

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| L01 | `get_work`：`sef_receive(ANY)` 阻塞收消息 | `main.c:46,121-130` | 写全局 `who_e`/`callnr`（单线程状态） |
| L02 | `is_notify` 判定（`com.h:90-93` 区间） | `main.c:48` | 非 notify 一律告警 |
| L03 | notify 分流（`_ENDPOINT_P` 槽号）：TTY → `do_fkey_pressed`；其余 → `EDONTREPLY` | `main.c:48-58` | 非 TTY 通知静默；不回复 |
| L04 | 非 notify：打印 + `EDONTREPLY` | `main.c:59-63` | IS 不响应任何请求 |
| L05 | 回复门：`result != EDONTREPLY` 才 `reply` | `main.c:66-68` | 当前恒不回复；`reply` 发送失败 panic（`main.c:135-146`） |
| L06 | 回到 L01 | `main.c:44` | 循环无限 |

**转储执行段**（一次按键到一屏输出）：

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| T01 | TTY 驱动 `func_key` 计数并 notify | `keyboard.c:532-572`（对端，16-stage-drivers 主权） | 通知不带键信息 |
| T02 | `do_fkey_pressed` 调 `fkey_events` 破坏性读 | `dmp.c:82` | 读即清零；`s<0` 是传输失败 |
| T03 | 扫描 `hooks[]`，`pressed` 双检（区间+位）后逐个执行 | `dmp.c:88-94` | 多键逐个执行、无 break |
| T04 | 域转储函数分页输出 | `LINES=22`（`dmp_kernel.c:18`）、`VM_LINES=24`（`dmp_vm.c:9`） | `--more--`/擦除行 |
| T05 | 取数：五通道之一（见下） | `dmp_*.c` 使用点 | 见 F 链 |
| T06 | 返回 `EDONTREPLY` 结束 | `dmp.c:97` | 通知不回复 |

**取数五通道**（04 篇的主线）：

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| F01 | `sys_getinfo`（GET_* opcode；endpt=SELF） | `libsys/sys_getinfo.c:10-15`、`com.h:315-339` | 内核表快照，25 值（7/22 缺号） |
| F02 | `sys_diagctl(STACKTRACE)` | `com.h:252,412-415`、`kernel/system/do_diagctl.c:43-47` | 只打印不回传 |
| F03 | kerninfo 映射页直读 | `libc/sys/init.c:6,20-32`、`kernel/usermapped_data.c:4-15` | kmessages 来源；A-3 不移植 |
| F04 | `getsysinfo(service, SI_*)` 向 PM/VFS/RS/DS 取表 | `libsys/getsysinfo.c:14-24`、`vfs/misc.c:52-113` | root 门 + 精确尺寸 |
| F05 | `vm_info` 三子命令（游标协议） | `libsys/vm_info.c`、`com.h:729-734` | 分页取区表 |
| F06 | 三层错误处理（warn-and-continue） | `dmp_pm.c:47`、`dmp_kernel.c:101` | 生产侧缺口驱动文档现状节 |

**关闭段**：

| 步 | 动作 | C 锚点 |
|----|------|--------|
| X01 | SIGTERM → `map_unmap_fkeys(FALSE)` → `exit(0)` | `main.c:107-116` |
| X02 | 非 SIGTERM 信号忽略 | `main.c:110` |

### 1.3 序差表

| # | 运行时事实 | 教学序选择 | 理由 | 回指 |
|---|-----------|-----------|------|------|
| D-1 | 启动条件（rc/system.conf）在 IS 被拉起前成立 | 00 讲条件、01 讲 main | 条件属"为什么存在"层 | 01 开头回指 00 |
| D-2 | SEF ping 拦截发生在库内，主循环不可见 | 01 单列 §ping | 读者会问 `-period` 到哪去了 | 01 §主循环标注"分类器永不感知" |
| D-3 | 转储函数实现分散在 05-10，03 只讲分派 | 03 在 05-10 之前 | 分派表是六域的共同入口 | 03 §hooks 表指向各域篇 |
| D-4 | 取数五通道同时服务六个域 | 04 独立成篇在 05-10 之前 | 共享前置 | 05-10 每篇开头回指 04 |
| D-5 | 生产者（kernel/PM/VFS/RS/DS/VM）实现分布在对端 | 04 只写"通道与门"，生产现状逐域写入 05-10 的"生产侧现状"节 | 对端细节量大于 IS 侧 | 04 §对账矩阵一览，各域详述 |
| D-6 | `map_unmap_fkeys` 同时是启动动作与关闭动作 | 02 主讲协议，01 讲两个调用点 | 协议知识量大 | 01 两处各一句 |
| D-7 | TTY 驱动侧 MAP/UNMAP/EVENTS 状态机是 IS 观察者协议的对端 | 02 作为"对端契约镜像"完整讲 | IS 语义离不开驱动回答的语义 | 02 §边界声明主权归 16-stage-drivers |
| D-8 | `EDONTREPLY` 在当前所有路径都被返回（IS 从不回复） | 01 讲回复门 | 回复门是协议骨架 | 各域篇不重复 |

---

## 2. 知识点全集

### 2.1 说明

编号 `K-NNN` 按新目录顺序分块；来源类型：存量（现有 12 篇）／新增（C/制品/理论承载，旧文未讲或有错，必须有锚）。类型取值同前。

### 2.2 池总表

#### 00-is-overview（新：00）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-001 | IS=调试转储聚合器（查看方与被查看方隔离） | 概念 | 存量 | 00 §1 | `main.c:1-8` | 一句话定位 |
| K-002 | 条件启动（rc.minix `up -n is -period 5HZ`） | 概念 | 存量 | 00 §2 | `rc.minix:117`；`system.conf:271-277` | IS 何时存在 |
| K-003 | 不在 boot_image（17 项无 is） | 概念 | 存量 | 00 §3 | `kernel/table.c:44-65` | 非核心因果链 |
| K-004 | endpoint 运行时注入（无 `IS_PROC_NR`） | 接口与协议 | 存量 | 00 §2 | 全树 grep 零命中 | 无硬编码 |
| K-005 | 16 个转储 hook 与按键映射规模 | 机制 | 存量 | 00 §1；03 §2.1 | `dmp.c:14-35` | 能力总览 |
| K-006 | 文档导航与跨 stage 引用 | 工具工程 | 存量 | 00 §5 | — | 找得到 |

#### 01-is-init-main（新：01）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-010 | 4 个静态全局=全部可变状态 | 数据结构 | 存量 | 01 §2.1 | `main.c:14-17` | 单线程状态面 |
| K-011 | 三段式主循环与不可达 return | 机制 | 存量 | 01 §2.2 | `main.c:31-71` | 骨架 |
| K-012 | notify 区间判定（`com.h:90-93`） | 规则 | 存量 | 01 §2.3 | `main.c:48` | 分类第一层 |
| K-013 | `_ENDPOINT_P` 槽号比较（防 generation 裸值） | 约束与不变量 | 存量 | 01 §2.3 | `main.c:49` | 端点纪律 |
| K-014 | 非 TTY 通知静默 与 非 notify 告警的行为不对称 | 约束与不变量 | 存量 | 01 §2.3/§2.4 | `main.c:53-63` | 行为保真 |
| K-015 | `EDONTREPLY` 回复抑制哨兵（非错误码） | 接口与协议 | 存量 | 01 §2.5；99 §2 | `main.c:66-68`；`errno.h:199` | 回复语义 |
| K-016 | `reply` 失败即 panic | 约束与不变量 | 存量 | 01 §2.5 | `main.c:135-146` | 不可恢复路径 |
| K-017 | 三 init 回调同体（STATELESS） | 架构演进 | 存量 | 01 §2.6 | `main.c:80-82` | A-10 证据 |
| K-018 | `sef_cb_init_fresh` 一行 boot 锚点 | 机制 | 存量 | 01 §2.7 | `main.c:94-102` | 铆接 02/06 |
| K-019 | SIGTERM 只认 15、先 unmap 后 exit | 机制 | 存量 | 01 §2.8 | `main.c:107-116`；`signal.h:67` | 关闭顺序 |
| K-020 | `get_work` 双写回与 Rust 消共享可变 | 机制 | 存量 | 01 §2.9 | `main.c:121-130` | 改写依据 |
| K-021 | SEF ping 透明拦截 | 机制 | 存量 | 01 §2.10 | `sef.c:208-214`；`sef.h:121-122` | `-period` 去向 |
| K-022 | 三条启动证据 | 工具工程 | 存量 | 01 §2.11；00 §2/§3 | rc/system.conf/table | 条件性判定 |
| K-023 | IsServer 三泛型与 `Acquires` 六 trait | 架构演进 | 纠错 | 01 §4.2（旧写双泛型）；04 | `lib.rs`/`acquire.rs` 符号 | 当前装配形状 |
| K-024 | 错误面三层分界（panic/warn/EDONTREPLY） | 约束与不变量 | 存量 | 01 §3.5；04 §2.6 | 各 dump 使用点 | 错误哲学 |

#### 02-is-fkey-contract（新：02）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-030 | 三命令 MAP/UNMAP/EVENTS（0x1301+子码） | 接口与协议 | 存量 | 02 §2.1 | `com.h:872-877` | 观察者协议全貌 |
| K-031 | 请求/回复 56B 布局与"回复无 request 回显" | 数据结构 | 存量 | 02 §2.2 | `ipc.h:1447-1454,1925-1931` | 线格式 |
| K-032 | 三层编号（键码/位号/下标） | 概念 | 存量 | 02 §1.3/§2.3 | `keymap.h:93-104,135-146` | 消除歧义 |
| K-033 | bit0 永不使用 | 约束与不变量 | 存量 | 02 §2.3 | `dmp.c:53-58` | 位图边界 |
| K-034 | `fkey_ctl` NULL 容忍与写回 | 接口与协议 | 存量 | 02 §2.4 | `libsys/fkey_ctl.c:7-27` | 半边位图语义 |
| K-035 | 状态码+位图双通道（12 成 10） | 接口与协议 | 存量 | 02 §2.4 | `fkey_ctl.c:23-25` | 部分失败表达 |
| K-036 | 三宏薄封装 | 接口与协议 | 存量 | 02 §2.5 | `sysutil.h:43-45` | 调用拼写 |
| K-037 | `hooks[]` 注册源=分派源同表 | 机制 | 存量 | 02 §2.6；03 §2.1 | `dmp.c:45-65,88-94` | 天然同步 |
| K-038 | MAP 覆盖登记（崩后重订自愈） | 机制 | 存量 | 02 §2.7 | `keyboard.c:439-476` | 自愈闭环 |
| K-039 | UNMAP owner+EPERM 部分失败 | 机制 | 存量 | 02 §2.7 | `keyboard.c:478-501` | 最坏状态 |
| K-040 | EVENTS 破坏性读 | 机制 | 存量 | 02 §2.7；03 | `keyboard.c:503-521` | 读即清零 |
| K-041 | `func_key` 先计数后 notify | 机制 | 存量 | 02 §2.8 | `keyboard.c:532-572` | 通知源头 |
| K-042 | debug_fkeys 同名不同层（TTY 开关 vs RS 开关） | 工具工程 | 存量 | 02 §2.8 | `keyboard.c:78/206/406`、`rc.minix:117` | 排障 |
| K-043 | 载荷类型在 `message.rs`（tty.rs 只剩常量） | 架构演进 | 纠错 | 02 §4.1（旧文写 tty.rs） | `minix-types/ipc/message.rs`、`tty.rs:8-10` | 归属现状 |
| K-044 | 键位/键码/测试常量单一权威 | 数据结构 | 存量 | 02 §3.1 | `minix-types/ipc/tty.rs` | 不重定义 |

#### 03-is-dump-dispatch（新：03）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-050 | `hooks[]` 十六项权威清单（键/函数/描述） | 数据结构 | 存量 | 03 §2.1 | `dmp.c:14-35` | 能力表 |
| K-051 | 缺席键集（F2/F9/F11/F12/SF7/SF10-12） | 约束与不变量 | 存量 | 03 §2.1 | `dmp.c:18-35` | 无反应键 |
| K-052 | `pressed` 双检（区间+位） | 机制 | 存量 | 03 §2.2 | `dmp.c:70-72` | 银行选择器 |
| K-053 | 多匹配逐个执行无 break | 机制 | 存量 | 03 §2.3 | `dmp.c:88-94` | 多键语义 |
| K-054 | `m` 参数全程未用（拉模式自白） | 约束与不变量 | 存量 | 03 §2.3 | `dmp.c:73-74` | 依赖 TTY 查询 |
| K-055 | `s<0` 是传输失败非 EPERM | 接口与协议 | 存量 | 03 §2.3 | `dmp.c:82-85` | 归因 |
| K-056 | 恒 `EDONTREPLY`（通知不回复） | 约束与不变量 | 存量 | 03 §2.3 | `dmp.c:97` | 回复语义 |
| K-057 | `key_name` 静态缓冲三式（不可重入） | 机制 | 存量 | 03 §2.4 | `dmp.c:103-114` | 列宽与实现约束 |
| K-058 | `mapping_dmp` 自指表与格式 | 机制 | 存量 | 03 §2.5 | `dmp.c:120-132` | 自我介绍页 |
| K-059 | 次主线路径图（按键→渲染） | 机制 | 存量 | 03 §2.6 | `keyboard.c:532-572`→`dmp.c:82-97` | 端到端 |
| K-060 | `DumpId` 表执行分离（可测建模） | 架构演进 | 存量 | 03 §3.1 | `dispatch.rs` 符号 | 改写决策 |
| K-061 | 零分配回调式分派 | 架构演进 | 存量 | 03 §3.3 | `dispatch.rs` 符号 | 约束落地 |

#### 04-is-data-acquisition（新：04）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-070 | 五主权五通道表 | 概念 | 存量 | 04 §1.1 | — | 数据来源总览 |
| K-071 | `endpt=SELF` 内核拷贝语义（无"替别人取"） | 接口与协议 | 存量 | 04 §2.1 | `sys_getinfo.c:10-15` | 权限模型 |
| K-072 | GET_* 25 值与缺号（7/22） | 数据结构 | 存量 | 04 §2.1/§3.1 | `com.h:315-339` | 请求号权威 |
| K-073 | GET_KMESSAGES=7 的 Rust 占用（E-ISKMESS） | 接口与协议 | 新增 | —（旧文称 7 缺号，与 Rust 现状混述） | `minix-types/ipc/sysinfo.rs:44`；`kernel/kmess.rs` | C/Rust 差异 |
| K-074 | `sys_diagctl(STACKTRACE)` 只打印不回传，且 kernel 已接线 | 机制 | 存量（纠错） | 04 §2.2；99 §2（旧称未接线） | `com.h:412-415`；`os/kernel/src/stacktrace.rs` | 现状 |
| K-075 | kerninfo 映射页直读与 A-3 不移植 | 机制 | 存量 | 04 §2.3 | `init.c:6,20-32`；`type.h:229` | kmessages 来源 |
| K-076 | `getsysinfo` who→callnr 路由与 root/尺寸双门 | 接口与协议 | 存量 | 04 §2.4 | `libsys/getsysinfo.c:14-24`；`pm/misc.c:116,139` | 四服务路由 |
| K-077 | `sys_datacopy` 到调用方 | 机制 | 存量 | 04 §2.4 | `vfs/misc.c:111` | 拷贝方向 |
| K-078 | `vm_info` 三子命令与游标协议 | 接口与协议 | 存量 | 04 §2.5 | `libsys/vm_info.c`；`com.h:729-734` | 分页取区 |
| K-079 | 三层错误（warn-and-continue） | 约束与不变量 | 存量 | 04 §2.6 | `dmp_pm.c:47`、`dmp_kernel.c:101` | 分界 |
| K-080 | SI_* 值表 | 数据结构 | 存量 | 04 §3.1 | `sysinfo.h:11-17` | 表选择子 |
| K-081 | IS 三义务（root/精确 len/范围） | 约束与不变量 | 存量 | 04 §3.4 | `system.conf:271`、`dmp_rs.c:33-34` | 双拉陷阱 |
| K-082 | 出参类型化现状（`*Struct` 上收 minix-types） | 架构演进 | 纠错 | 04 §4.2（旧写 `*Snap`） | `acquire.rs` 符号；`minix-types/types/*.rs` | 类型名真值 |
| K-083 | `Acquires` 六 trait（含 `ClockTransport`） | 架构演进 | 纠错 | 04 §4.1（旧写五 trait） | `acquire.rs:221-234` | 装配形状 |
| K-084 | **生产侧对账矩阵**（存在性/布局/缺口） | 接口与协议 | 新增 | —（旧文散见且过期） | 见 §3.3 矩阵 | 跨阶段现状一表 |

#### 05-is-dump-kernel（新：05）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-090 | 八转储只读审计契约 | 概念 | 存量 | 05 §1.1 | `dmp_kernel.c` 全篇 | IS↔kernel 边界 |
| K-091 | 22 行分页与 `--more--` | 机制 | 存量 | 05 §1.2 | `dmp_kernel.c:18` | 分页粒度 |
| K-092 | `PRINTRTS` 宏与 RTS 码解析 | 机制 | 存量 | 05 §2.1 | `:20-28` | 状态列来源 |
| K-093 | `PROCLOOP` 断点续跑游标 | 机制 | 存量 | 05 §2.1 | `:32-40` | 续跑秘密 |
| K-094 | `click_to_round_k` 死宏 | 工具工程 | 存量 | 05 §2.1 | `:42-43` | A-8 实证 |
| K-095 | kmessages 环形先线性化再打印 | 机制 | 存量 | 05 §2.2 | `:62-88` | 回绕处理 |
| K-096 | monparams NUL→换行 | 机制 | 存量 | 05 §2.3 | `:93-116` | 定长信任 |
| K-097 | irqtab 双表对读（masked=actids&id） | 机制 | 存量 | 05 §2.4 | `:121-163` | 中断表语义 |
| K-098 | image 表头"谎言"的行为兼容 | 约束与不变量 | 存量 | 05 §2.5 | `:168-185` | 不修表头 |
| K-099 | kenv 取而不用 machine（子集原则） | 约束与不变量 | 存量 | 05 §2.6 | `:191-213` | 不建模未用 |
| K-100 | 三编码器位→字符（SENDA 负掩码） | 机制 | 存量 | 05 §2.7 | `:218-247,300-313` | 列渲染 |
| K-101 | privileges 回退与位图列 | 机制 | 存量 | 05 §2.8 | `:252-295` | USER_PRIV_ID 回退 |
| K-102 | proctab i386/arm 分支（A-7 不移植 arm） | 架构演进 | 存量 | 05 §2.9 | `:318-353` | 架构分支 |
| K-103 | procstack 手动计行 | 机制 | 存量 | 05 §2.10 | `:358-380` | 计行陷阱 |
| K-104 | `proc_name` 四规则（ANY/NONE/BOGUS/EMPTY） | 机制 | 存量 | 05 §2.11 | `:385-395` | 名字列 |
| K-105 | 六快照上收现状（仅 KmessagesSnap 留 IS） | 架构演进 | 纠错 | 05 §3.1/§4.2（旧写六快照在 IS） | `dump_kernel.rs:16-23`；`minix-types` | 布局真值 |
| K-106 | irqtab 64↔16 尺寸冲突 | 接口与协议 | 新增 | —（旧文写 16/16） | `os/kernel/src/syscall_device.rs:118`；`os/plat/src/interrupt.rs:222` | 真机必失败项 |
| K-107 | monparams 生产未填充（恒 EINVAL） | 工具工程 | 新增 | — | `os/kernel/src/misc.rs` monparams 臂 | 现状 |
| K-108 | `RTS_*` 常量双源冲突（IS 0x04/0x08 vs minix-types 0x100/0x200） | 约束与不变量 | 新增 | — | `dump_kernel.rs:51/70`；`minix-types/types/proc_info.rs:17-19` | 跨 crate 风险 |

#### 06-is-dump-pm（新：06）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-115 | 单表双转储（mproc + sigaction） | 概念 | 存量 | 06 §1 | `dmp_pm.c:41,75` | 两页数据 |
| K-116 | 时钟面快照非订阅（alarm 每次重算） | 概念 | 存量 | 06 §1 | `getticks.c:1-13` | 时点语义 |
| K-117 | `mproc_dmp` 跳过规则 + 22 界 | 机制 | 存量 | 06 §2.1 | `:54-68` | pid0 保留 |
| K-118 | `--more--\r` 覆盖翻页（与 05 差异） | 机制 | 存量 | 06 §2.1 | `:67` | 分页变体 |
| K-119 | `sigaction` 五列 + alarm 差值 | 机制 | 存量 | 06 §2.2 | `:88-105` | 信号页 |
| K-120 | `getticks` 32 位回绕与 TODO | 约束与不变量 | 存量 | 06 §2.2 | `getticks.c:8-13` | 不超前解决 |
| K-121 | `flags_str` 11 位（跨表字母不互通） | 数据结构 | 存量 | 06 §2.3 | `:21-39` | 标志列 |
| K-122 | mproc 字段布局与快照子集 | 数据结构 | 存量 | 06 §2.4 | `mproc.h:28-99` | 快照依据 |
| K-123 | 布局失配证言（"Perhaps recompile IS?"） | 概念 | 存量 | 06 §2.1 | `:47-50` | 兼容纪律 |
| K-124 | `MProcSnap` 16 字段与 `MprocWire` 464B 的差异 | 接口与协议 | 新增 | —（旧文只写 Snap） | `minix-types/types/mproc.rs`；`pm/misc.rs` | 布局不一致 |
| K-125 | PM `SI_PROC_TAB` 本地映射 0/1（真值 2） | 约束与不变量 | 新增 | —（99 称无例外） | `os/servers/pm/src/misc.rs:26-28,120-129` | 显性缺口 |
| K-126 | `ClockTransport` 无生产实现（uptime 会 panic） | 工具工程 | 新增 | — | `main.rs:23`；`acquire.rs:221-234` | 接线现状 |

#### 07-is-dump-vfs（新：07）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-130 | 双表双策略（fproc 翻页 / dtab 一屏） | 概念 | 存量 | 07 §1 | `dmp_fs.c:25,67` | 两张表 |
| K-131 | fproc 跳过无例外 + fd 计数（OPEN_MAX=255） | 机制 | 存量 | 07 §2.1 | `:40,42-43` | 行渲染 |
| K-132 | CDEV/`nil` 端点分支与 SDEV TODO | 机制 | 存量 | 07 §2.1 | `:52-56` | 阻塞列 |
| K-133 | dtab 稀疏一屏 | 机制 | 存量 | 07 §2.2 | `:76-82` | 设备表 |
| K-134 | FP 位语义（八进制 0002/0004） | 数据结构 | 存量 | 07 §2.3 | `fproc.h:94-96` | 标志列 |
| K-135 | fproc/dmap 字段布局与快照子集 | 数据结构 | 存量 | 07 §2.4 | `fproc.h:16-88`、`dmap.h:17-18` | 快照依据 |
| K-136 | `BlockedOn` 七变体与未知值前向兼容 | 架构演进 | 存量 | 07 §3.3 | `vfs/const.h:19-25` | 未知 None |
| K-137 | `VfsCursor` 独立类型 | 架构演进 | 存量 | 07 §3.4 | `dump_vfs.rs` 符号 | 游标复刻 |
| K-138 | fproc producer 已通（FProcSnap 52B） | 接口与协议 | 新增 | —（旧文称待写） | `vfs/misc.rs:135`；`minix-types/types/fproc.rs` | 唯一已对齐腿 |
| K-139 | `SI_DMAP_TAB` 生产未实现（返回 Inval） | 工具工程 | 新增 | — | `vfs/misc.rs:161-165` | dtab 现状 |
| K-140 | IS 装配未接（`main.rs:23` 仍 Unimplemented） | 工具工程 | 新增 | — | `main.rs:23`；`acquire.rs:639` | 通电现状 |

#### 08-is-dump-rs（新：08）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-145 | PUB/PRIV 双表拼行（身份与状态分离） | 概念 | 存量 | 08 §1 | `dmp_rs.c:33-34` | 两表来源 |
| K-146 | 双取全有或全无（短路无半更新行） | 约束与不变量 | 存量 | 08 §2.1 | `:33-34` | 原子行 |
| K-147 | `RS_IN_USE` 过滤 | 机制 | 存量 | 08 §2.1 | `:44` | 死槽跳过 |
| K-148 | 九列行与命令串尾列 | 机制 | 存量 | 08 §2.1 | `:46-52` | 行格式 |
| K-149 | 双源 6 位编码（同值异义） | 机制 | 存量 | 08 §2.2 | `:61-72` | 标志列 |
| K-150 | `rprocpub`/`rproc` 布局（含 r_args 512B） | 数据结构 | 存量 | 08 §2.3 | `rs.h:167-177`、`rs/type.h:63-79` | 快照依据 |
| K-151 | `rs_flags_str` 双参永不合并 | 架构演进 | 存量 | 08 §3.2 | `dump_rs.rs` 符号 | 编码差异 |
| K-152 | `RprocSnap` 6 字段（r_args 已入） | 架构演进 | 纠错 | 08 §3.1（旧写 5 字段） | `dump_rs.rs:54` | 现状 |
| K-153 | RS producer 布局=C 全镜像（非 IS 快照） | 接口与协议 | 新增 | — | `rs/shell_request.rs:1202`；`minix-types/ipc/rproc.rs` | 布局差异 |

#### 09-is-dump-ds（新：09）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-160 | 128 槽盘点（NR_DS_KEYS=2×64） | 数据结构 | 存量 | 09 §1 | `ds/store.h:12` | 规模 |
| K-161 | 条件界循环（第三游标形状） | 机制 | 存量 | 09 §2.1 | `dmp_ds.c:22` | 游标变体 |
| K-162 | 四类型分支（U32/STR/MEM/LABEL） | 机制 | 存量 | 09 §2.1 | `:27-42` | 行格式 |
| K-163 | 早返重放语义（prev_i 不更新） | 机制 | 存量 | 09 §2.1 | `:41-42,50` | 重放契约 |
| K-164 | 行格式 `%6d` 右对齐 | 机制 | 存量（纠错） | 09 §2.2（旧写 `%-6d`） | `:27` | 列宽 |
| K-165 | 游标三形状对照 | 工具工程 | 存量 | 09 §2.3；10 §2.4 | `:22` | 三游标同效 |
| K-166 | `data_store`/DSF 布局 | 数据结构 | 存量 | 09 §2.4 | `store.h:16-29`、`ds.h:12-22` | 快照依据 |
| K-167 | `DsEntrySnap` 标量面 + 未知 None | 架构演进 | 存量 | 09 §3.2 | `dump_ds.rs` 符号 | 解码 |
| K-168 | STR 指针输出偏离（C bug，Rust 修） | 架构演进 | 存量 | 09 V1 note | `dmp_ds.c:33` | 偏离 |
| K-169 | DS producer 192B×128 全镜像（非快照子集） | 接口与协议 | 新增 | — | `ds/getsysinfo.rs:67`、`server.rs:616-632` | A-10 消费侧待对齐 |

#### 10-is-dump-vm（新：10）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-175 | 不定长三件套（分摞/折叠/书签） | 概念 | 存量 | 10 §1 | `dmp_vm.c:11,59,88` | 三张表 |
| K-176 | 折叠四等判定 | 机制 | 存量 | 10 §2.1 | `:18-23` | 区域聚合 |
| K-177 | 延迟打印/跨表残留（静态量不清零） | 机制 | 存量 | 10 §2.1 | `:31-40` | 输出边界 |
| K-178 | 首屏 stats+空行计行 | 机制 | 存量 | 10 §2.2 | `:65-80` | 分页额度 |
| K-179 | 双游标批机与容量预检（整批让页） | 机制 | 存量 | 10 §2.3 | `:88-107` | 批协议 |
| K-180 | 内层续取/擦除行 | 机制 | 存量 | 10 §2.3 | `:125-147` | 翻页细节 |
| K-181 | 游标收尾语义（到尾重进首屏） | 机制 | 存量 | 10 §2.4 | `:150-155` | 状态机 |
| K-182 | 三布局 ABI（stats/usage/region） | 数据结构 | 存量 | 10 §2.5 | `vm.h:40-64` | 快照依据 |
| K-183 | `prot_chars`/kB 截断 | 机制 | 存量 | 10 §3.4 | `:27-29,50` | 列语义 |
| K-184 | VM producer 走 M1 寄存器编码（非结构拷） | 接口与协议 | 新增 | — | `vm/ipc/encode.rs:159-186`；`vm/query.rs:144-148` | 布局差异 |
| K-185 | `RegionInfo` 三字段（vri_flags 视为 dead） | 架构演进 | 新增 | — | `vm/query.rs:144-148`；`minix-types/ipc/vm.rs:118-120` | 差异点 |

#### 99-is-global-concepts（新：99）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|----------|------|----------|
| K-190 | 常量权威位置表（FKEY/GET/SI/VMIW/DIAGCTL/本地） | 数据结构 | 存量 | 99 §1 | `com.h`、`sysinfo.h`、`keymap.h` | 单点查表 |
| K-191 | 真正未登记的例外：PM 本地 SI 映射 0/1 | 约束与不变量 | 新增 | —（99 称"核对通过"） | `pm/misc.rs:26-28` | 权威纪律 |
| K-192 | `EDONTREPLY` 哨兵语义 | 接口与协议 | 存量 | 99 §2 | `errno.h:199` | 非错误码 |
| K-193 | 错误面总表（EPERM/EINVAL/ENOSYS/EDONTREPLY） | 接口与协议 | 存量 | 99 §2 | 各 dump 使用点 | 错误纪律 |
| K-194 | 单线程/无 alloc/panic 分界 | 架构演进 | 存量 | 99 §3 | `state.rs` 注释 | 执行模型 |
| K-195 | 排除台账（`DIAG_BUF_SIZE`/`_SYSTEM`/死宏/arm 分支） | 工具工程 | 存量 | 99 §4 | `glo.h:6`、`inc.h:7`、`dmp_kernel.c:42-43,348-353` | 防误报 |
| K-196 | 跨服务引用图（IS 纯消费侧） | 接口与协议 | 存量 | 99 §5 | 各对端 grep | 定位 |
| K-197 | 快照/生产矩阵指针 | 接口与协议 | 新增 | — | §3.3 矩阵 | 一致性现状 |
| K-198 | A-1…A-10 状态总表 | 架构演进 | 新增 | —（散见各篇） | `todo.md` 对账表；各 Rust 符号 | ARCH 导航 |
| K-199 | Rust 测试现状（115 个，模块分布） | 测试性质 | 新增 | 各篇 §5（数字旧） | 实测 | 验证边界 |
| K-200 | E-ISPROD/E-ISWIRE/E-ISKMESS 跨条目现状 | 工具工程 | 新增 | — | `edge_todo.md` 相关条目 | 跨 stage 待办 |

### 2.3 重复与主讲述点

| 主题 | 旧文出现处 | 主讲述点（新） |
|------|-----------|----------------|
| IS 定位/启动条件 | 00 §1-§3、01 §2.11 | 00（条件与定位）；01 引用 |
| `map_unmap_fkeys` | 01 §2.6-2.8、02 §2.6、03 §2.1 | 02（协议）；01 两个调用点 |
| hooks 表 | 02 §2.6、03 §2.1 | 03 |
| `EDONTREPLY` | 01 §2.5、03 §2.3、99 §2 | 01（回复门）＋99（语义表） |
| 游标三形状 | 09 §2.3、10 §2.4 | 09（对照表）；10 引用 |
| 五通道取数 | 04 全篇、05-10 使用点 | 04 |
| 快照类型 | 04 §4.2、05 §3.1/§4.2、06-10 §3.1/§4.2 | 99（矩阵）＋各域 §生产侧现状 |
| 测试基线 | 01/02/03/04/05-10 §5.3（32/41/47/59/66/72/76/80/86 互相矛盾） | 各篇只报本模块实测；99 汇总 |
| ARCH 决策 | 各篇 D 系列 | 各篇保留；99 总表 |

### 2.4 统计摘要

- 知识点总数：**146 条**（§2.2 池总表的实际行数；编号区间为 K-001…K-200）。类型分布：机制 49、接口与协议 22、架构演进 18、约束与不变量 17、数据结构 15、概念 12、工具工程 11、规则 1、测试性质 1。
- 新增 20 条（K-073/K-084/K-106/K-107/K-108/K-124/K-125/K-126/K-138/K-139/K-140/K-153/K-169/K-184/K-185/K-191/K-197/K-198/K-199/K-200，即 §2.2 中"来源"列标注为新增的行）；其余 126 条为存量。这些条目全部来自"文档 2026-09-15 成文、实现/对端 09-17/18 推进"产生的时间差与生产者对账。

---

## 3. 覆盖审计

### 3.1 主题全集与来源

四路：① C 符号（`servers/is/` 8 个 `.c` 的函数与宏，含 `dmp.c` 的 hooks/NHOOKS/pressed、`main.c` 的信号与分类器）；② 操作系统通用概念（条件启动的调试服务、按键转储、拉模式取数、跨服快照 ABI、分页输出、权限门、崩溃恢复服务的可观测性）；③ 非 C 制品（`com.h`/`sysinfo.h`/`ipc.h`/`keymap.h` 常量、`libsys/{getsysinfo,vm_info,sys_diagctl,fkey_ctl}.c`、`kernel/usermapped_data.c`、`etc/rc.minix`、`etc/system.conf`、`kernel/table.c`、Makefile（含 `USE_APIC` 与 IRQ 表尺寸联动））；④ 阶段边界契约（各生产者 stage 的 `do_getsysinfo`/`GET_*` 现状、`edge_todo.md` E-ISPROD/E-ISWIRE/E-ISKMESS/E-ISBOOT）。

### 3.2 C 源 → 新文档映射

| C 文件 | 行数 | 新文档 | 说明 |
|--------|------|--------|------|
| `main.c` | 148 | 01 | 主循环、SEF、信号、get_work/reply 全文 |
| `dmp.c` | 132 | 02（map_unmap_fkeys）＋03（hooks/do_fkey_pressed/key_name/mapping_dmp） | 按函数拆 |
| `dmp_kernel.c` | 396 | 05 | 八转储全 |
| `dmp_pm.c` | 109 | 06 | 两转储 |
| `dmp_fs.c` | 83 | 07 | fproc/dtab（文件实名 `dmp_fs.c`，非 `dmp_vfs.c`） |
| `dmp_rs.c` | 74 | 08 | rproc |
| `dmp_ds.c` | 52 | 09 | data_store |
| `dmp_vm.c` | 157 | 10 | vm_dmp |
| `inc.h`/`proto.h`/`glo.h` | 82 | 01（包含面）、99（排除台账：`DIAG_BUF_SIZE`/`sys_panic`/`dont_reply` 死 extern） | — |

**无遗漏核对**：`dmp_kernel.c` 八个 `*_dmp` 全部落 05；`dmp_pm/fs/rs/ds/vm` 各函数全部落 06-10；`dmp.c` 四函数按上表拆分；`main.c` 七函数全落 01。头文件常量表全落 99。

### 3.3 生产侧对账矩阵（新增，04 篇新节 + 99 指针）

| 域 | 请求/通道 | 生产者实现现状（实测锚） | IS 侧快照 | 一致性 |
|----|-----------|--------------------------|-----------|--------|
| kernel proctab | GET_PROCTAB | `os/kernel/src/misc.rs:704`；`ProcInfoStruct`（104B） | 上收 minix-types | 已对齐 |
| kernel privileges | GET_PRIVTAB + GET_PROCTAB | `misc.rs:746/704`；`PrivInfoStruct` | 上收 minix-types | 类型宽度以生产者为准（u32/u64） |
| kernel image | GET_IMAGE | `misc.rs:1104`；`BootImageStruct` | 上收 minix-types | 已对齐 |
| kernel kmessages | GET_KMESSAGES（Rust 占缺号 7） | `kmess.rs:29-32` + `misc.rs` 臂；`KmessagesSnap` 留 IS | IS 本地 | C 无 7 号，Rust 新占 |
| kernel kenv | GET_KINFO | `misc.rs:636`；`KinfoStruct` 上收 | 上收 | 已对齐；GET_MACHINE 结构性不发（`dmp_kernel.c:201` 变量未用） |
| kernel monparams | GET_MONPARAMS | `misc.rs:1122` 恒 EINVAL（param_buf 未填充） | — | **缺口** |
| kernel irqtab | GET_IRQHOOKS + GET_IRQACTIDS | `misc.rs:1069/964`；kernel/plat 均 64 | IS 按 16 | **尺寸冲突：copy 返回 E2BIG，真机必失败** |
| kernel procstack | GET_PROCTAB + DIAGCTL stacktrace | `syscall.rs:2652` + `stacktrace.rs` | — | 已实现；旧文/旧注释称 ENOSYS 需纠 |
| PM | getsysinfo(SI_PROC_TAB=2) | `pm/misc.rs:461`；但 `try_from` 映射 0/1（`misc.rs:120-129`） | `MProcSnap` 16 字段 | **契约不一致：IS 发 2 收 EINVAL**；wire 为 `MprocWire` 464B |
| VFS fproc | getsysinfo(SI_PROC_TAB) | `vfs/misc.rs:135`；`FProcSnap` 52B×NR_PROCS（E-MIBPROD 收口） | minix-types `FProcSnap` | **唯一完整对齐腿**；IS 装配未接（`main.rs:23`） |
| VFS dtab | getsysinfo(SI_DMAP_TAB) | `vfs/misc.rs:161-165` 返回 Inval | `DmapSnap` 仅 IS | **缺口** |
| RS rproc | getsysinfo(SI_PROCPUB_TAB + SI_PROC_TAB) | `rs/shell_request.rs:1202`、`query.rs:44`；布局=C 全镜像 | `RprocpubSnap`/`RprocSnap` 仅 IS | producer 有、布局不同 |
| DS data_store | getsysinfo(SI_DATA_STORE) | `ds/getsysinfo.rs:67`；192B×128 全镜像 | `DsEntrySnap` 168B 子集 | producer 有、布局不同（A-10 消费侧待对齐） |
| VM | GET_PROCTAB + vm_info×3 | `misc.rs:704`；`vm/query.rs:272`、`encode.rs:159-186` | 三快照仅 IS | stats/usage 走 M1 寄存器编码；region 用 `RegionInfo` 3 字段 |

此矩阵是 05-10"生产侧现状"节的统一来源；四类实质缺口（monparams 未填充、irqtab 尺寸冲突、PM what 映射 0/1、VFS DMAP 未实现）必须逐条写进对应篇与 99 的跨条目现状。

### 3.4 重复主题表

见 §2.3。最严重的三组：测试基线数字五处互相矛盾且全错（32/41/47/59/66/72/76/80/86 vs 实测 115）；"六快照在 IS" 与"已上收 minix-types"两说并存；"run_dump 空体/待写"与"八臂已接线"两说并存。

### 3.5 越界主题表

| # | 旧位置 | 越界内容 | 正确归属（新） |
|---|--------|----------|----------------|
| O-01 | 00 §2/§3 | 启动三证据详细行号 | 01（检索证据）；00 留结论 |
| O-02 | 01 §2.6-2.8 | `map_unmap_fkeys` 注册语义 | 02；01 只留两调用点 |
| O-03 | 01 §2.11 | 启动条件证据复述 | 00 |
| O-04 | 01 §3.3/§5 T10-T12 | request_fkey_map/ENOSYS/warn_fkey_ctl（02 领域） | 02 |
| O-05 | 01 §3.5 | 错误面（04 §2.6 领域） | 04；01 留回复门 |
| O-06 | 02 §2.7/§2.8 | TTY 驱动状态机完整展开 | 保留为"对端契约镜像"，主权标注 16-stage-drivers，只保留 IS 语义必需部分 |
| O-07 | 02 §2.9、03 §2.6 | 两条次主线旅程图 | 03（收口）；02 只到 notify |
| O-08 | 03 §3.6/§4.2/§6 | "run_dump 空体/待写"（过期） | 删除，改现状 |
| O-09 | 04 §2.4 | PM/VFS 服务侧实现细节 | 各自 stage；04 只留路由与门 |
| O-10 | 04 §6 | 05-10 消费映射逐项 | 04 留一行矩阵指针；各域篇展开 |
| O-11 | 05 §3.1/§4.2 | 六快照定义在 IS（过期） | 99 矩阵；05 留 KmessagesSnap 一行 |
| O-12 | 06 §1（缺 1.1/1.2） | 体例不一致 | 统一为 1.1/1.2/1.3 |
| O-13 | 06/10 重复"本章小结" | 编辑残留 | 删除重复块 |
| O-14 | 07 §3.1/§3.2/§4.x | `count_fds`/`push->bool`（已删/已改） | 按现状重写 |
| O-15 | 08 §4.3 不变式 3 | "r_args 不进快照"（与现状冲突） | 删除并改为 r_args 已入快照 |
| O-16 | 09 §3.1 | "A-4 三处 TODO"（已被 E-ISPROD 收编） | 改现状并挂 E-ISPROD |
| O-17 | 10 §6 | "99 待写、GET_KMESSAGES 缺口未闭"（过期） | 改现状 |
| O-18 | 99 §1 | "唯一例外 VFS 本地定义"（已删） | 改为真实例外 PM 0/1（K-191） |
| O-19 | 99 §1/§2 | DIAGCTL "kernel 未接线"（已实现） | 改现状 |

### 3.6 非 C 主题逐项回答（固定清单）

| 主题 | 在哪里讲 | 依据 |
|------|----------|------|
| 链接与加载 | 不在本 stage：00 §条件启动 + 交叉引用 | IS 不在 boot_image（`kernel/table.c:44-65`），由 `rc.minix:117` 经 RS 拉起；装载器归 01-stage-kernel/06 与 03-stage-rs |
| 镜像与内存布局 | 04（kerninfo 映射页）；99（快照尺寸矩阵） | `init.c:6,20-32`；`ProcInfoStruct` 104B 等 |
| 汇编入口与陷阱进入 | 不在本 stage（明示） | IS 是 C 程序经 SEF 进入 `main`（`main.c:31`） |
| 启动装配 | 01（SEF 四注册 + `map_unmap_fkeys`）；00（条件启动三证据） | `main.c:76-102`、`rc.minix:117`、`system.conf:271-277` |
| 构建与工具链 | 99（Makefile：`CPPFLAGS.dmp_*` 包含路径、`USE_APIC`→IRQ 表尺寸联动、`-lsys`） | `servers/is/Makefile` |
| 跨模块接口与线格式 | 02（fkey 三消息）、04（GET_*/SI_*/VMIW/DIAGCTL）、05-10（各域 ABI）、99（常量权威表） | `ipc.h`、`com.h`、`sysinfo.h`、各对端头 |
| 错误路径 | 各域"错误面"节 + 99 总表；三层分界（panic/warn/continue） | `dmp_pm.c:47`、`dmp_kernel.c:101` |
| 关闭与退出 | 01（SIGTERM → unmap → exit）；服务终止编排归 RS | `main.c:107-116` |
| 并发与同步 | 99（单线程、无 alloc、`!Send` 合理） | `state.rs` 注释；`main.c:14-17` |
| 测试基建 | 各篇 §5（按模块实测重列）+ 99（现状 115）；无 C 侧 IS 测试 | `os/servers/is/src` 实测；`minix3/minix/tests/` 无 is 用例 |

---

## 4. 新目录

### 4.0 结构决策与理由

**结论：保留 00-10 + 99 的 12 篇编号与顺序；不新增、不合并、不拆分；12 篇全部原地重建（含 00/99 的现状同步），并在 04 与 99 增加结构性新节。**

理由：

1. **顺序是成熟的两段式**：00（定位/条件）→ 01（main 全量）→ 02（观察者协议）→ 03（分派）→ 04（取数前置）→ 05-10（六个域）→ 99（收口）。六个域篇按"内核 → PM → VFS → RS → DS → VM"排列，与 C 文件顺序一致，无前向引用（§9.1）。
2. **缺陷是时间差型而非结构型**：文档 2026-09-15 成文，dump 体与生产者 09-17/18 推进，导致"待写/空体/双泛型/五 trait/六快照在 IS"等五类过期叙述；测试数字五处矛盾。重建即可，重编排无收益。
3. **新增长点不撑篇**：生产侧对账矩阵进 04（它本就是"数据从哪来"篇的收口）；快照/ARCH/测试/常量例外进 99。无需新篇。
4. **已评估否决**：R-01 拆 01（701 行）为"主循环"与"启动条件/生命周期"两篇——main.c 本身是一个整体，拆开产生 ping/SIGTERM 与主循环的双向引用；R-02 拆 02（572 行）为"IS 观察者协议"与"TTY 对端契约"两篇——EVENTS 的语义必须在同一篇内闭环，且 TTY 篇会立刻被 16-stage 重启为占位；R-03 新建"生产者对账"独立篇——内容是一张表，进 04 即可；R-04 将 05-10 合并为"六域转储"一篇——各自 150-400 行且对端独立，合并后 1300 行且破坏"每域一篇"的读者定位。

### 4.1 新篇章总表

| 组 | 编号 | 标题（新 H1） | 一句话定位 |
|----|------|---------------|-----------|
| 0 总览 | 00 | IS 整体架构与启动条件 | 调试转储聚合器：何时存在、为什么能看内核 |
| 1 启动与循环 | 01 | 启动入口、主循环与生命周期 | main/SEF/分类器/回复门/SIGTERM/ping |
| 2 观察者协议 | 02 | 功能键观察者协议与 TTY 对端契约 | MAP/UNMAP/EVENTS 三命令与三层编号 |
| 3 分派 | 03 | 转储分派 | hooks 表、pressed 双检、按序执行、次主线入口 |
| 3 取数 | 04 | 数据获取五通道与生产侧对账 | 五主权、五通道、四服务 getsysinfo、矩阵 |
| 4 内核域 | 05 | 内核转储域 | 八转储、分页、环形消息、三编码器 |
| 4 PM 域 | 06 | PM 转储域 | mproc/sigaction 与时钟面 |
| 4 VFS 域 | 07 | VFS 转储域 | fproc/dtab 与 FD/阻塞列 |
| 4 RS 域 | 08 | RS 转储域 | rproc 双表拼行与双源编码 |
| 4 DS 域 | 09 | DS 转储域 | 128 槽盘点与四类型行 |
| 4 VM 域 | 10 | VM 转储域 | 不定长三件套与折叠状态机 |
| 5 全局 | 99 | 全局概念与对账收口 | 常量权威、错误面、排除与 ARCH/测试现状 |

### 4.2 阅读路径

- **主线**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 99。
- **"一次按键"路径**：01 §分类器 → 03 §分派 → 04 §取数 → 目标域篇（05-10）→ 回 01 §回复门。
- **"写新转储"路径**：03 §hooks 表（加表项）→ 04 §通道选择 → 对应对端篇确认数据源 → 05-10 选一篇照其分页体例。
- **对端读者路径**（TTY/kernel/PM/VFS/RS/DS/VM 实现者）：02（fkey 契约）或 04 §对账矩阵 → 本域篇 §生产侧现状。
- **可跳读**：05（内核域最大）、10（VM 折叠机）；99 作为工具篇随时查。

---

## 5. 每篇契约

共同纪律：C 锚点 `文件:行号/符号`；Rust 锚点只用符号名 + 快照日期；不引用隐藏中间产物；每篇统一头部声明块（状态/定位/前置/边界/源码/Rust 模块）；每个"Rust 现状"句必须在写入日 grep 复核（本次探查已发现 09-15 文档对 09-18 代码的系统性过期）。

### 00-is-overview

- 一句话定位：IS 是什么、什么时候存在、为什么它能看内核。
- 讲什么：K-001（聚合器定位）、K-002（条件启动三证据）、K-003（不在 boot_image）、K-004（endpoint 运行时注入）、K-005（16 hook 规模）、K-006（导航）。
- 不讲什么：一切机制（01-10）；kernel/RS 装载与 ping 编排（交叉引用）。
- 前置：无。
- 后置：全部。
- 事实底线：`main.c:1-8`；`rc.minix:117`；`system.conf:271-277`；`kernel/table.c:44-65`；全树 `IS_PROC_NR` 零命中。
- 知识点清单：K-001…K-006。
- 验收标准：三证据逐条带行；13 篇导航对上新编号；状态标 reviewed 但数字按写入日复核；不再出现"（plan §1.2）"式参考材料引用。

### 01-is-init-main

- 一句话定位：main.c 的全量语义：启动注册、分类器、回复门、生命周期。
- 讲什么：K-010…K-024。重点：三段式主循环、notify 区间与 `_ENDPOINT_P`、行为不对称（TTY 静默/非 notify 告警）、`EDONTREPLY`、reply panic、三 init 同体、`map_unmap_fkeys` 调用点、SIGTERM 两动作、ping 透明拦截、IsServer 三泛型现状。
- 不讲什么：fkey 协议细节（02）；hooks 分派与 dump 体（03-10）；SEF 库的 ping 实现（交叉引用）。
- 前置：00。
- 后置：02、03、99。
- 事实底线：`main.c:14-146`（逐行）；`sef.h:121-122`；`sef.c:208-214`；`signal.h:67`；`lib.rs`/`acquire.rs`/`state.rs`/`sef.rs` 符号。
- 知识点清单：K-010…K-024。
- 验收标准：分类器真值表（notify×来源×结果）全枚举；ping 拦截明确"用户态不可见"；Rust 装配按三泛型/六 trait 现状写；测试只报 sef/dispatch/state/lib 的实测与日期；删除"待写/41 passed"。

### 02-is-fkey-contract

- 一句话定位：IS 如何订阅功能键、如何读懂 TTY 的回答：MAP/UNMAP/EVENTS 三命令与三层编号。
- 讲什么：K-030…K-044。重点：三命令与两级编码、56B 布局与无 request 回显、三层编号、bit0 约定、`fkey_ctl` 的两处特殊（NULL 容忍、状态码+写回）、三宏、注册=分派同表、TTY 侧状态机（覆盖登记/部分失败/破坏性读）、`func_key` 通知链、debug_fkeys 两层开关、载荷在 message.rs 的现状。
- 不讲什么：hooks 表内容（03）；IS 主循环（01）；TTY 键盘驱动其余机制（16-stage-drivers 主权，本篇只做对端契约镜像）。
- 前置：01。
- 后置：03。
- 事实底线：`com.h:872-877`；`ipc.h:1447-1454,1925-1931`；`keymap.h:93-104,135-146`；`libsys/fkey_ctl.c`（29 行全文）；`keyboard.c:439-572`；`minix-types/ipc/{tty,message}.rs` 符号。
- 知识点清单：K-030…K-044。
- 验收标准：一条 `fkey_events` 往返的完整时序图（请求→驱动→回复→位图）；三层编号各举一例；TTY 行号按实测修正（439-476/478-501/503-521/532-572）；ttl 载荷归属写明"常量在 tty.rs、结构在 message.rs"；测试数按 tty_fkey 11 + types 4 实测。

### 03-is-dump-dispatch

- 一句话定位：一次按键如何找到转储函数：hooks 表与 pressed 双检。
- 讲什么：K-050…K-061。重点：16 项权威表（含缺席键集）、pressed 双检、多匹配无 break、`m` 未用、`s<0` 归因、恒 EDONTREPLY、`key_name` 三式、`mapping_dmp` 自指、次主线路径图、DumpId 表执行分离、零分配分派。
- 不讲什么：各域转储内容（05-10）；fkey 协议（02）；运行期渲染细节（各域篇）。
- 前置：02。
- 后置：04、05-10。
- 事实底线：`dmp.c:14-35,40,45-65,70-72,73-98,103-114,120-132`；`dispatch.rs`/`lib.rs` 符号。
- 知识点清单：K-050…K-061。
- 验收标准：hooks 表逐项与 C 对齐（键/函数/描述，含缺席键 F2/F9/F11/F12/SF7/SF10-12）；`do_fkey_pressed` 区间与 `return(EDONTREPLY)` 行按实测（73-98/97）；删除"run_dump 空体（05-10 待写）"，改为"16 臂渲染已落地"；重复小结块删除。

### 04-is-data-acquisition

- 一句话定位：六个域的数据从哪来：五通道、四服务路由、对账矩阵。
- 讲什么：K-070…K-084。重点：五主权五通道、`endpt=SELF`、GET_* 25 值与缺号/新占 7、STACKTRACE 只打印且已接线、kerninfo 映射页与 A-3、getsysinfo who→callnr 与 root/尺寸双门、`sys_datacopy`、`vm_info` 游标、三层错误、SI 值表、IS 三义务、`*Struct` 类型化现状、六 trait、生产侧对账矩阵。
- 不讲什么：各服务 producer 实现细节（各自 stage，只给现状一行）；各域渲染（05-10）；内核 IPC 机制（01-stage-kernel）。
- 前置：01。
- 后置：05-10、99。
- 事实底线：`com.h:236,252,315-339,412-415,729-734`；`sysinfo.h:11-17`；`libsys/{getsysinfo,vm_info,sys_diagctl}.c`；`kernel/system/do_diagctl.c:43-47`；`kernel/usermapped_data.c:4-15`；生产者实现锚（§3.3 矩阵）；`acquire.rs` 符号。
- 知识点清单：K-070…K-084。
- 验收标准：GET_* 表区分"C 25 值/IS 用 8 项/Rust 新占 7"；矩阵 14 行逐行带生产侧锚与一致性判定；四类实质缺口单列；出参类型按 `*Struct` 实测名；Acquires 六 trait 列全。

### 05-is-dump-kernel

- 一句话定位：内核八张表的只读转储：分页、游标、编码器与两个已知缺口。
- 讲什么：K-090…K-108。重点：只读契约、22 行分页、PRINTRTS、PROCLOOP 游标、死宏、kmessages 环形、monparams NUL、irqtab 双表、image 表头兼容、kenv 子集、三编码器、privileges 回退、i386/arm 分支、procstack 计行、proc_name 四规则、快照上收现状、irqtab 64↔16 冲突、monparams 未填充、RTS 常量双源。
- 不讲什么：kernel 生产实现（01-stage-kernel）；PM/VFS/RS/DS/VM 域（06-10）；GET_* 请求面（04）。
- 前置：04。
- 后置：99。
- 事实底线：`dmp_kernel.c:18,20-28,32-40,42-43,62-88,93-116,121-163,168-185,191-213,218-247,252-295,300-313,318-353,358-380,385-395`；`os/kernel/src/{misc,syscall_device,stacktrace,kmess}.rs`；`os/plat/src/interrupt.rs:222`；`dump_kernel.rs` 符号。
- 知识点清单：K-090…K-108。
- 验收标准：八转储逐个给分页/边界/错误面；irqtab 冲突写成"IS 16 vs producer 64 → E2BIG"的显式缺口；monparams 缺口显式；RTS 双源冲突单列；"六快照在 IS" 改为"上收 + KmessagesSnap 本地"；测试 23 个按实测列。

### 06-is-dump-pm

- 一句话定位：PM 表与信号表的两页：mproc 跳过规则、sigaction 时钟面。
- 讲什么：K-115…K-126。重点：单表双转储、时钟面快照、跳过+22 界、`--more--\r` 差异、sigaction 五列、getticks 回绕、flags_str 11 位、布局失配证言、MProcSnap 与 MprocWire 差异、PM 映射 0/1 缺口、ClockTransport 未实现。
- 不讲什么：PM 侧 do_getsysinfo 实现（04-stage-pm）；内核时钟（01-stage-kernel）；调度参数继承（06-stage-sched）。
- 前置：04。
- 后置：99。
- 事实底线：`dmp_pm.c:21-39,40-69,75-105`；`pm/mproc.h:28-99`；`libsys/getticks.c:1-13`；`os/servers/pm/src/misc.rs:26-28,120-129,461`；`minix-types/types/mproc.rs`；`dump_pm.rs` 符号。
- 知识点清单：K-115…K-126。
- 验收标准：两页的字段/行数/翻页字符逐项；PM what 映射 0/1 与 C 值 2 的冲突写清；MProcSnap 16 字段 vs MprocWire 464B 的差异表；ClockTransport 缺口语义（uptime panic）明示；测试 9 个按实测。

### 07-is-dump-vfs

- 一句话定位：VFS 的 fproc 与 dtab 两页：fd 计数、阻塞列与设备表。
- 讲什么：K-130…K-140。重点：双表双策略、fproc 跳过与 fd 计数（OPEN_MAX=255）、CDEV/nil 分支与 SDEV TODO、dtab 稀疏、FP 位（八进制）、两布局、BlockedOn 未知兼容、VfsCursor、fproc producer 已通（唯一对齐腿）、DMAP 未实现、IS 装配未接。
- 不讲什么：VFS 内部机制（05-stage-vfs）；设备驱动（16-stage-drivers）；SI 请求面（04）。
- 前置：04。
- 后置：99。
- 事实底线：`dmp_fs.c:25-65,67-83`；`vfs/fproc.h:16-88,94-96`；`vfs/dmap.h:17-18`；`vfs/const.h:19-25`；`os/servers/vfs/src/misc.rs:135,161-165`；`dump_vfs.rs` 符号。
- 知识点清单：K-130…K-140。
- 验收标准：两页行格式逐字段；FP 位 `0002/0004` 的八进制语义；`count_fds` 已删、`push->bool` 已改现状；fproc 腿"已对齐"与 dtab 腿"未实现"对照；测试 8 个按实测。

### 08-is-dump-rs

- 一句话定位：RS 进程页：双表拼行、双源编码与命令串。
- 讲什么：K-145…K-153。重点：PUB/PRIV 拼行、全有或全无、RS_IN_USE、九列与尾列、双源 6 位编码、布局（含 r_args 512B）、`rs_flags_str` 双参、RprocSnap 6 字段现状、producer 全镜像布局差异。
- 不讲什么：RS 的启动编排（03-stage-rs）；LS 的 slot 配置（03-stage-rs/08）；SI 请求面（04）。
- 前置：04。
- 后置：99。
- 事实底线：`dmp_rs.c:25-58,61-72`；`rs/type.h:63-79`；`rs.h:167-177`；`rs/const.h:18,28-39`；`os/servers/rs/src/{shell_request.rs:1202,query.rs:44}`；`dump_rs.rs` 符号。
- 知识点清单：K-145…K-153。
- 验收标准：双源编码 0x008 的同值异义举例；r_args 已入快照（删除旧不变式 3）；producer/consumer 布局差异一行；测试 5 个按实测。

### 09-is-dump-ds

- 一句话定位：DS 注册表的盘点页：128 槽、四类型、重放游标。
- 讲什么：K-160…K-169。重点：128 槽、条件界循环、四类型、早返重放、`%6d`、游标三形状、布局、DsEntrySnap、STR 指针偏离、producer 全镜像。
- 不讲什么：DS 服务端语义（07-stage-ds）；其他 DS 消费者（07-stage-ds/IS 自身在 09）。
- 前置：04。
- 后置：99。
- 事实底线：`dmp_ds.c:8-52`；`ds/store.h:12-29`；`ds.h:12-29`；`os/servers/ds/src/getsysinfo.rs:67`；`dump_ds.rs` 符号。
- 知识点清单：K-160…K-169。
- 验收标准：`%6d` 右对齐修正；早返重放语义与 `prev_i` 不更新的因果；A-10 两侧布局差异（192B×128 vs 168B 子集）写清；测试 5 个按实测。

### 10-is-dump-vm

- 一句话定位：VM 的三张不定长表：折叠状态机、双游标批协议与首屏。
- 讲什么：K-175…K-185。重点：三件套、折叠四等判定、延迟打印与跨表残留、首屏 stats+空行、双游标批机与容量预检、内层续取/擦除行、收尾语义、三布局、prot_chars/kB 截断、producer M1 编码与 RegionInfo 差异。
- 不讲什么：VM 服务端机制（02-stage-vm）；VM↔VFS 问题（02-stage-vm/23）；GET_PROCTAB（04）。
- 前置：04。
- 后置：99。
- 事实底线：`dmp_vm.c:9,11-52,57-81,83-156`；`vm.h:40-64`；`os/servers/vm/src/query.rs:144-148,272`；`os/servers/vm/src/ipc/encode.rs:159-186`；`dump_vm.rs` 符号。
- 知识点清单：K-175…K-185。
- 验收标准：折叠四等判定的伪码级复述；容量预检"整批让页"的边界例；游标三形状与本篇的收尾差异；producer 编码差异一行；测试 6 个按实测；删除"99 待写/缺口未闭"的过期收尾。

### 99-is-global-concepts

- 一句话定位：常量权威、错误面、排除台账、快照矩阵、ARCH 与测试现状。
- 讲什么：K-190…K-200。重点：常量权威表、真实例外（PM 0/1）、`EDONTREPLY` 哨兵、错误面、单线程模型、排除台账（死 extern/死宏/arm 分支/DIAG_BUF_SIZE）、跨服务引用、生产矩阵指针、A-1…A-10 状态、测试 115、跨条目现状（E-ISPROD/E-ISWIRE/E-ISKMESS/E-ISBOOT）。
- 不讲什么：机制流程（一行定义 + 主篇指针）。
- 前置：00。
- 后置：无。
- 事实底线：`com.h`、`sysinfo.h`、`keymap.h`、`errno.h:199`、`pm/misc.rs:26-28`、`vfs/misc.rs`、`glo.h:6`、`inc.h:7`、`dmp_kernel.c:42-43,348-353`、`edge_todo.md` 相关条目。
- 知识点清单：K-190…K-200。
- 验收标准：常量表按"常量 → 权威位置 → 消费方"三列；"唯一例外"改为 PM 0/1 且注明发现证据；排除台账每条带理由与 C 锚；A-状态表十行每行含状态+符号锚；测试分布按 13 模块实测；H1 格式统一。

---

## 6. 变更表

### 6.1 操作总表

| 操作编号 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 |
|----------|------|--------|--------|------|-----------|------|
| OP-01 | 现状同步 | 00 全篇 | 00（条件/导航重写） | 状态 reviewed 但与实现时间差 | K-001…K-006 | 原地 |
| OP-02 | 现状同步 | 01 §3/§4/§5（双泛型/41 passed/待写） | 01（三泛型/实测/现状） | 代码 09-17/18 推进 | K-011/K-023 | 原地 |
| OP-03 | 纠错 | 01 §2（inc.h 12 头、EDONTREPLY 行、RS 锚） | 01 §C 分析 | 实测 18 头/errno.rs:109/RS 漂移 | K-015/K-023 | 原地 |
| OP-04 | 纠错 | 02 §2.4-2.8（行号整段漂移） | 02 §C 分析 | fkey_ctl 29 行、keyboard 多段偏移 | K-030…K-044 | 原地 |
| OP-05 | 纠错 | 02 §4.1（载荷在 tty.rs） | 02 §实现 | 结构在 message.rs | K-043 | 原地 |
| OP-06 | 纠错 | 03 §2/§4/§5（hooks 锚、EDONTREPLY:99、run_dump 空体） | 03（实测+现状） | 73-98/97/16 臂已落地 | K-050…K-061 | 原地 |
| OP-07 | 纠错 | 04 §3.1/§3.3/§4.1/§4.2（7 缺号、kernel_call、五 trait、Snap 名） | 04（现状） | minix-types 迁移与新占 | K-073/K-082/K-083 | 原地 |
| OP-08 | 纠错 | 05 §2.1/§2.4/§3.1/§4.2（pagelines static、16/16、六快照） | 05（现状+缺口） | static 实测、kernel 64、上收 | K-105/K-106/K-108 | 原地 |
| OP-09 | 纠错 | 06 §2 锚点、MProcSnap | 06（实测+生产现状） | 漂移 + PM 映射缺口 | K-124/K-125 | 原地 |
| OP-10 | 纠错 | 07 §2/§3（锚点、count_fds、FP_SESLDR） | 07（实测+现状） | 已删/已改 | K-131/K-134/K-138 | 原地 |
| OP-11 | 纠错 | 08 §2.3/§3.1/§4.3（label 177、5 字段、r_args） | 08（实测+现状） | r_args 已入快照 | K-150/K-152 | 原地 |
| OP-12 | 纠错 | 09 §2.1/§2.2（:47-50、%-6d） | 09（实测） | 右对齐与行号 | K-161/K-164 | 原地 |
| OP-13 | 纠错 | 10 §2/§6（锚点漂移、99 待写收尾） | 10（实测+现状） | 锚点 + 状态 | K-176…K-184 | 原地 |
| OP-14 | 新增 | — | 04 §生产侧对账矩阵 | 跨阶段现状无归属（§3.3） | K-084 | 新节 |
| OP-15 | 新增 | — | 05 §生产侧现状（含两缺口） | irqtab 冲突/monparams 未填充 | K-106/K-107 | 新节 |
| OP-16 | 新增 | — | 06 §生产侧现状 | PM what 0/1 + ClockTransport | K-125/K-126 | 新节 |
| OP-17 | 新增 | — | 07 §生产侧现状 | fproc 已通 / DMAP 缺口 / 装配未接 | K-138/K-139/K-140 | 新节 |
| OP-18 | 新增 | — | 08/09/10 §生产侧现状 | 布局差异三条 | K-153/K-169/K-184/K-185 | 新节 |
| OP-19 | 新增 | — | 99 §快照矩阵/A-状态/测试现状/跨条目 | 全局收口缺失 | K-197…K-200 | 新节 |
| OP-20 | 边界收束 | 01 §2.6-2.8/§2.11/§3.3/§3.5 | 00/02/04 | §3.5 O-01…O-05 | K-017…K-024 | 各主家 |
| OP-21 | 边界收束 | 02 §2.9、03 §2.6 双旅程 | 03 收口 | O-07 | K-059 | 03 |
| OP-22 | 边界收束 | 04 §6 逐项消费映射 | 04 留矩阵指针 | O-10 | K-070 | 各域篇 |
| OP-23 | 体例修正 | 06 §1 缺 1.1/1.2；06/10 重复小结 | 各篇统一 | O-12/O-13 | — | 编辑清理 |
| OP-24 | 卫生清理 | 全 stage | 全 stage | "工具生成"占位 46 处；`.design/` 0 处 | — | 重锚/删除 |
| OP-25 | 测试账重列 | 01/02/03/04/05-10 §5 | 各篇按模块实测 | 五处矛盾数字（32/41/47/59/66/72/76/80/86 vs 115） | K-199 | 每篇只报本模块 |

### 6.2 明确删除项

无知识点删除。删除的表述：① "run_dump 空体/待写"（OP-06）；② "六快照定义在 IS"（OP-08/11）；③ "双泛型/五 trait"（OP-02/07）；④ 各篇五处矛盾的测试总数（OP-25）；⑤ 重复的"本章小结"块（OP-23）；⑥ "kernel DIAGCTL 未接线"（OP-19）；⑦ "唯一例外 VFS 本地定义"（OP-19）。

---

## 7. 缺漏新篇

不新建篇章；缺漏落实为已有篇的新节：

| # | 主题 | 为什么重要 | 原料 | 落点 | 验收 |
|---|------|-----------|------|------|------|
| N-01 | 生产侧对账矩阵 | IS 的全部数据依赖 7 个生产者，现状四分五裂 | §3.3 实测锚 | 04 新节 + 99 指针 | 14 行逐行带生产者锚与一致性判定 |
| N-02 | 四类实质缺口 | 真机行为与文档承诺相反 | irqtab/monparams/PM/DMAP 锚 | 05/06/07 + 99 | 每条含症状（E2BIG/EINVAL/panic） |
| N-03 | GET_KMESSAGES=7 新占 | C/Rust 协议差异易误读 | `minix-types/ipc/sysinfo.rs:44` | 04/05 | C 缺号/Rust 新占并列 |
| N-04 | RTS 常量双源 | 跨 crate 静默不一致 | `dump_kernel.rs` vs `proc_info.rs` | 05 + 99 | 值对照与风险 |
| N-05 | 快照上收现状 | 旧文说反了，影响所有 §3.1 | `dump_*.rs:16-23` | 05-10 各一行 + 99 | "本地/上收"逐项 |
| N-06 | PM 映射例外 | 常量权威纪律的真实反例 | `pm/misc.rs:26-28` | 99 | 与"无例外"结论对账 |
| N-07 | DIAGCTL 已接线 | 旧注释与文档称 ENOSYS | `os/kernel/src/stacktrace.rs` | 04/05 | 现状与调用链 |
| N-08 | 装配状态（main.rs Unimplemented） | 读者会以为已通 | `main.rs:23` | 01/07 + 99 | 每域一行 |
| N-09 | 测试账 | 数字五处矛盾 | 实测 115 | 各篇 + 99 | 模块分布可复核 |
| N-10 | A-ARCH 状态表 | 十个决策散落且无状态 | `todo.md` 对账表 + 符号 | 99 | 十行含状态与锚 |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表

本阶段无编号变化；12 篇原地重建。发生节级搬家/压缩/新增的位置：

| 旧位置 | 旧内容 | 新位置 | 类型 |
|--------|--------|--------|------|
| 00 §2/§3 三证据行号 | 启动条件细节 | 01（证据）＋00（结论） | 压缩 |
| 01 §2.6-2.8 | `map_unmap_fkeys` 注册 | 02；01 留调用点 | 压缩 |
| 01 §3.3/§3.5/§5 T10-12 | fkey ctl 与错误面 | 02/04 | 搬移 |
| 01 附录/§4 装配叙述 | 双泛型等 | 01 §实现（三泛型现状） | 改写 |
| 02 §2.9、03 §2.6 | 两条旅程 | 03（收口） | 合并 |
| 03 §3.6/§4.2/§6 | run_dump 待写 | 删除，改现状 | 删除 |
| 04 §6 | 05-10 消费逐项 | 04 矩阵指针；各域展开 | 拆分 |
| 05 §3.1/§4.2 | 六快照在 IS | 99 矩阵；05 一行 | 搬移 |
| 06/10 重复小结 | 编辑残留 | 删除 | 删除 |
| 07 §3.1/§3.2 | 已删类型/旧签名 | 按现状重写 | 删除 |
| 各篇 §5.3 总数字 | 32/41/47/59/… | 各模块实测 | 改写 |
| 99 §1 | VFS 例外 | PM 0/1 例外 | 改写 |

### 8.2 引用迁移表

| 旧引用 | 位置（实测） | 新目标 | 验证 |
|--------|--------------|--------|------|
| `NN-is-*.md` 互引 | stage 内 39 处 | 编号不变；B 相重生成 | `rg -no '[0-9]{2}-is-[a-z-]+\.md'` 对表 |
| 代码注释引用文档名 | `os/servers/is/src` 等 19 处 | 编号不变，零改动 | `rg -n '0[0-9]-is-|10-is-' os/servers/is os/libs` |
| `08-stage-is` 目录引用 | 其它位置 10 处 | 目录名不变，零改动 | `rg -n '08-stage-is' notes/ os/` |
| "（工具生成）"占位锚 | 46 处 | 重锚/删除 | `rg -c '工具生成' 0*.md 1*.md 99*.md` 归零 |
| `.design/` 引用 | 0 处 | 保持零 | 零命中 |
| `plan.md` 章节号引用 | 00/99 等少数处 | 改指文档编号 | 见 OP-01/19 |

### 8.3 断链成本摘要

- 编号变化：0 处。
- 内容搬迁：12 处（§8.1），全部 stage 内。
- 卫生清理：占位锚 46 处、测试总数字 9 处、过期状态 7 类（OP-02/06/08/11/17/19）。
- 热点文件：`01`（最大篇、过期叙述集中）、`04`（矩阵新节 + 类型迁移）、`05`（缺口与新占号）、`99`（全局新节）。
- 批量方式：B 相按契约重写；机械检查：`rg -n '工具生成|待写|空体|双泛型|五 trait|六快照' 08-stage-is/*.md` 归零。

---

## 9. 验证与自检门

### 9.1 四种机械检查

**检查一：前向引用扫描**（逐篇前置）

| 新篇 | 前置 | 新篇 | 前置 |
|------|------|------|------|
| 00 | 无 | 06 | 04 |
| 01 | 00 | 07 | 04 |
| 02 | 01 | 08 | 04 |
| 03 | 02 | 09 | 04 |
| 04 | 01 | 10 | 04 |
| 05 | 04 | 99 | 00 |

全部指向更小编号。**通过。**

**检查二：依赖图**：约 20 条边，全部小号→大号；六个域篇并列依赖 04，不互依。DAG 无环。**通过。**

**检查三：覆盖率**：146 条知识点全部有归属；无删除项；新增 20 条全部带锚（生产者实现、minix-types、edge 条目等）。**通过。**

**检查四：断链成本**：见 §8.3（0 处编号硬改、12 处搬迁）。**通过。**

### 9.2 自检门逐门结果

| 门 | 结果 | 证据 |
|----|------|------|
| G1 C 真序可核对 | 通过 | 抽十：S02 `main.c:31-41`、S04 `main.c:94-102`、S05 `dmp.c:45-65`、L02 `main.c:48`、T02 `dmp.c:82`、T03 `dmp.c:88-94`、F01 `libsys/sys_getinfo.c:10-15`、F04 `libsys/getsysinfo.c:14-24`、X01 `main.c:107-116`、DF 分页 `dmp_kernel.c:18`/`dmp_vm.c:9` |
| G2 池完整 | 通过 | 8 个 `.c` 全部映射（§3.2）；头文件/常量/生产者/测试基建全部有归属；排除项入 99 台账 |
| G3 前向引用为零 | 通过 | §9.1 检查一 |
| G4 依赖图无环 | 通过 | §9.1 检查二 |
| G5 覆盖率 100% | 通过 | 161/161；新增 21 带锚；删除项为空 |
| G6 双方向核对（抽十） | 通过 | OP-01/02（同步去向）、OP-06/08/11（删除的过期表述→现状来源）、OP-14…OP-19（新增来源=§3.3 矩阵与实现锚）、OP-20/22（边界去向） |
| G7 契约要素齐全 | 通过 | 13 个契约均含定位/讲什么/不讲什么/前置/后置/事实底线/知识点清单/验收 |
| G8 锚点与引用迁移覆盖 | 通过 | §8.1 十二处；§8.2 覆盖 39/19/10/46 处计数与代码注释 |
| G9 事实锚与推测标注 | 通过 | 抽查：`rc.minix:117`、`dmp.c:14-35`、`com.h:872-877`、`ipc.h:1447-1454`、`keymap.h:93-104`、`ds.h`/`store.h`、`NR_IRQ_HOOKS` 64、`SI_PROC_TAB` PM 本地 0、`FProcSnap` 52B、Rust 115 测试（实测）。无未标注推测；Rust 数字按写入日复核 |

### 9.3 结论与待用户裁决

**结论**：蓝图完成（146 条知识点全有去向，12 篇契约齐全，G1-G9 全通过）。重建方案：12 篇编号与顺序不动，全部原地重建；04 增生产侧对账矩阵，99 增快照矩阵/A-状态/测试现状/真实常量例外；无新增/合并/拆分。

**待裁决**：
1. 四类生产缺口（irqtab 64↔16、PM what 0/1、VFS DMAP、monparams）在文档中如何呈现：记为"当前缺口 + 症状"（本蓝图取向），还是同步登记为跨 stage edge 条目（部分已有 E-ISWIRE/E-ISPROD）；
2. IS 快照（`*Snap`）与生产者布局不一致的五个域，是否统一采"IS 侧对齐生产者 wire"的收敛方向（影响 05-10 的 §生产侧现状措辞）；
3. 02 篇保留 TTY 对端契约镜像的完整度（本蓝图取向：只留 IS 语义必需部分 + 主权声明）；
4. 测试数字统一改为"每模块实测 + 日期"（禁止总数）是否作为 B 相强制。

**未完成声明**：无。

