# 14-stage-runtime 文档重建蓝图（glm）

## 0. 元数据

- **执行者**：glm
- **日期**：2026-09-19
- **目标目录**：`notes/rewrite/fork-syscall-rewrite/14-stage-runtime/`
- **仓库根目录**：`/home/xzhao/github/minix-rs`
- **当前提交号**：`a4afb7ffbf5d06658c167a21ea27625897e6625a`（2026-09-19）
- **任务**：R 相·重建蓝图。只产出本文件，不修改任何正文。多 AI bagging：未读取任何其它 AI 的 `doc_rerank_*` 产物（仅统计其对本目录的引用计数）；未读取 `.design/` 与 `tmp_design_and_todo/`。

### 0.1 审查范围

- **文档（重建对象）**：编号文档 15 篇——`00-runtime-overview.md`、`01-kernel-handoff.md`～`13-constants-abi.md`、`99-global-concepts.md`。状态：01~13 于 2026-09-05 收敛（plan §6.1），00/99 正文 v1 于 2026-09-17 落稿（V1-P1-5，commit 95f971f02）；09/02/07/01 各篇在 V1 修复轮（2026-09-16）与 E-MINTYPES-RUNTIME/E1 批（2026-09-17）多次增量改写。
- **参考材料（不重建，只作证据与边界）**：`plan.md`（373 行，2026-08-16 定稿，§7 含 21 项自审）、`todo.md`（375 行，2026-09-16 V1 首轮架构审查；**V1-P0-1/2/3、P1-1（两步全闭环，含 rt-birth 真机通电）、P1-2/3/4/5、P2-1/3、P3-1/2 全部 ✅；仅 V1-P2-2 的跨 crate 半维持挂 edge E-MINTYPES-RUNTIME**）、`draft/README.md`（占位素材）。
- **范围外**：`.design/`；其它 AI 的 `doc_rerank_*`；`os/libs/minix-sys` 中归属其它 stage 的六个客户端模块（ds/devman_client/inputdriver/rmib/socket/usb_model，约 3108 行——todo §0 范围声明同款口径，只做 crate 级一致性扫描）；`minix-sef`（占位 crate，挂 E-ISWIRE）。

### 0.2 读取清单

| 类别 | 内容 |
|------|------|
| 目标文档 | 15 篇编号文档全文、plan.md、todo.md |
| C 源码（行数与关键锚点核实） | `lib/csu/`：`arch/x86_64/crt0.S`（49 行，6 条指令 :44-49，`STRONG_ALIAS(_start,__start)` :40）、`common/crt0-common.c`（192 行，`___start` :144-192）、`common/crtbegin.c`（135 行）。`minix/lib/libc/sys/`：`init.c`（32）、`kernel_utils.c`（62）、`stack_utils.c`（172）、`syscall.c`（25）、`loadname.c`（19）、`brk.c`（35）、`sbrk.c`（26）、`environ.c`（16）、`fork.c`（18）、`_exit.c`（31）、`execve.c`（59）、`wait4.c`（26）、`kill.c`（22）、`read.c`（22）、`write.c`（19）、`open.c`（33）、`close.c`（18）、`lseek.c`（24）、`vectorio.c`（161）、`mmap.c`（172）、`nanosleep.c`（100）、`svrctl.c`（27）、`__sysctl.c`（40）、`minix_rs.c`（41）。`minix/lib/libsys/`：`kernel_call.c`（22）、`kputc.c`（32）、`sys_diagctl.c`（28）、`panic.c`（68）、`assert.c`（32）、`asynsend.c`（189）、`getticks.c`（13）、`getuptime.c`（24）、`clock_time.c`（44）、`getsysinfo.c`（32）、`getepinfo.c`（80）、`getprocnr.c`（19）、`srv_fork.c`/`srv_kill.c`（各 14）、`taskcall.c`（20）、`tickdelay.c`（48）、`safecopies.c`（381）、`vm_fork.c`（25）、`vm_exit.c`（36）、`vm_map_phys.c`（52）、`vm_info.c`（58）、`vm_procctl.c`（38）、`vm_cache.c`（88）、`vm_getrusage.c`（17）。头文件：`type.h`（KERNINFO_MAGIC 0xfc3b84bf :229）、`errno.h`、`signal.h`、`callnr.h`、`com.h`（端点段 :59-66，07-ds/MIB 轮已实测） |
| Rust 实现 | `os/libs/minix-rt/src/`（6 文件：lib/crt0/handoff/init/alloc/diag）；`os/libs/minix-sys/src/` 域内 10 文件（ipc/syscall/pm/vfs/vm/misc/rs/stack/grant/arch_trap）；`os/libs/minix-types`（errno/signal/kerninfo/diagnostic/ipc 各域）——关键符号锚点核验见 §0.3 |
| 阶段边界材料 | `edge_todo.md`（E1 诞生链通电注记 :74、E-MINTYPES-RUNTIME、E-MINSYS-SCOPE、E-DSWIRE/E2/E6/E9 批）、`00-master-plan/README.md`（前轮已读）、`18-stage-commands`（对本 stage 依赖最重的消费方，只读引用计数） |

### 0.3 使用的命令与关键输出（证据摘录）

```text
git log -1 → a4afb7ffbf5d06658c167a21ea27625897e6625a
(ulimit -v 3145728; cargo test -p minix-rt -p minix-sys --lib)
  → 53 passed / 224 passed, 0 failed；cargo test -p minix-types --lib → 268 passed
逐模块 #[test] 计数：
  minix-rt：diag 14 / alloc 11 / handoff 10 / init 7 / crt0 7 / lib 4 = 53
  minix-sys 域内：syscall 49 / vm 30 / pm 29 / vfs 29 / misc 16 / ipc 12 / rs 10
                / stack 5 / grant 4 / arch_trap 0 = 184（域内合计）
grep "fn test_" os/libs/minix-rt/src/handoff.rs → 10 个（01 篇 §5 表只列 9 行，
  缺 test_magic_mismatch_maps_to_executable_format_errno :444——Fix #5 所增未入表，见 E1）
grep "pub fn sys_" os/libs/minix-sys/src/syscall.rs → 34 个 SYS_* 内核调用 wrapper（grep 实测）
  （sys_kill/abort/times/sigsend/getksig/endksig/trace/runctl/resume/vircopy/clear/
    fork/exec/safecopyfrom/safecopyto/update/diagctl/diagctl_write/setalarm/get_machine…）
grep "grant" 14-stage-runtime/04-ipc-primitives.md → 0 命中；grep "SYS_\|sys_fork"
  05-syscall-mechanism.md → 0 命中（两族 wrapper 均无文档归属，见 G1/G2）
head grant.rs → 模块文档自述"the Rust rewrite of C libsys safecopies.c"（381 行）
引用统计：入站文档引用 20+ 文件 60+ 处（edge_todo 14、18-stage-commands 24 最热、
  其它 AI rerank 约 20 处内容未读）；stage 内互引约 100 处（05 篇被引 13 次为枢纽）；
  os/ 代码注释 3 文件 4 处；出站跨 stage 引用 1 个目标
  （../01-stage-kernel/09-vm-boot-protocol.md）实测存在
edge_todo.md:74 → rt-birth 真机通电注记（2026-09-17，五断言 PASS，两个内核级根因修复）
```

---

## 1. C 真序

### 1.1 阶段类型判定

**运行时库型**（R 相提示词 §九四类之外的特殊形态，本 stage 的主线是"进程运行时生命周期"而非服务启动序——plan §1.1 已裁定并自证：runtime 不是 server，无主循环无 IPC 分发）。生命周期六段：**交付（01）→ 入口（02）→ 初始化（03）→ 机制（04/05）→ 资源与终局骨架（06/07）→ 服务族（08~12）**，收口两篇横切（13 常量对账、99 全局概念）。

**附带两个次级形态**：① 客户端库面——08~12 五篇是"封装函数按服务分组"的集合型（每组内部"统一协议 + 代表成员 + 清单收录"）；② 常量审计面——13 篇是横切对账表（对账模型 + 归属模型）。

判定依据：`crt0-common.c:144-192` 的 `___start` 是生命周期起点、`_exit.c` 是终点；中间没有"服务主循环"——主循环属于每个 server stage。

### 1.2 生命周期真序表（交付到 main）

| 步骤 | 动作 | C 锚点 | 说明 |
|------|------|--------|------|
| S1 | 内核交付两样行李：kerninfo 页指针 + 初始栈顶（`kui_user_sp`，老程序后备读 `kinfo->user_sp` 偏移 2440） | `minix/include/minix/type.h`（kuserinfo :205-212、kerninfo :214-244、KERNINFO_MAGIC 0xfc3b84bf :229） | 版本探测不烧版本号：`KUSERINFO_HAS_FIELD` 长度比较宏（:211-212） |
| S2 | 初始栈镜像：从高到低 = 字符串区 / 对齐填充 / 环境指针数组+NULL / 参数指针数组+NULL / auxv / exec 名 / ps_strings | `minix/lib/libc/sys/stack_utils.c`（布局注释图 :28-60、`minix_stack_params` :76-114、`minix_stack_fill` :119-172） | 溢出检查靠"加完反而变小"捕捉回绕（:95-107 区段） |
| S3 | 汇编桩 6 条指令：`andq $~15,%rsp` → `subq $8` → 三次参数搬运（rdx→rdi、rcx→rsi、rbx→rdx）→ `jmp ___start`（跳转不调用：入口桩没有可返回之处） | `lib/csu/arch/x86_64/crt0.S:44-49`（`_start` 别名 :40） | 三入口值 = 清理回调 / 加载器描述 / ps_strings 指针，静态链接下前两个恒零 |
| S4 | C 入口 `___start`：描述结构空指针检查（`_FATAL` 且 Minix 上输出被禁用=直接退）→ 发布 `__ps_strings`/`environ` → 短名推导（逐字节扫斜杠）→ 动态链接分支（静态链接永不可达）→ `_libc_init`（单次守卫：auxv/栈保护/TLS/线程/env 红黑树）→ preinit/init_array 正序、`atexit(_fini)` 登记 fini_array → `main` → `exit` | `lib/csu/common/crt0-common.c:144-192`（列表定义 :106-117） | `environ` 的初始哨兵 0x53535353 在 `minix/lib/libc/sys/environ.c`（只比低两字节的注释）；老式构造器逆序运行在 `lib/csu/common/crtbegin.c:72-97` |
| S5 | 运行时初始化构造器 `__minix_init`（链接器保证先于入口运行）：查询 kerninfo（`ipc_minix_kerninfo` 陷阱，调用号 6）→ 魔数校验 → **失败清零继续运行**（默认直接陷阱向量表预装于 :10-18，永不失效）→ 双重条件（标志位+指针）满足才整表安装内核发布向量表 | `minix/lib/libc/sys/init.c`（全文 32 行） | 向量表七函数指针 `minix_ipcvecs`（`ipc.h:2786-2795`），整表换防新旧混用 |
| S6 | main 期间的每次内核对话：调用号进 m_type → `int 33`（IPC）/`int 32`（内核调用，i386）陷入 → 状态字回传（低 6 位调用号 + 高位标志，"内核可信消息不可回复"） | `minix/include/minix/ipcconst.h:6-35`、`arch/i386/include/ipcconst.h`（:5-6 向量号）、`libc/arch/i386/sys/_ipc.S`（六函数同模 :16-87） | 64 位演进：指令与向量号变、分工模型不变（plan A-6） |
| S7 | 服务 syscall 协议：`_syscall` 四步（写调用号 → sendrec → 往返失败把状态码写进 m_type → 负数取反进 errno 返回 -1）+ 双路径打包（≤40 字节内联含 NUL，指针永远填）+ `_kernel_call` ENOTREADY 线性退避（1,2,3…） | `libc/sys/syscall.c:9-25`、`loadname.c:7-19`（分界值 40 = `ipc.h:14` 内联缓冲）、`libsys/kernel_call.c:7-21`、`tickdelay.c` | |
| S8 | 堆与资源：`_brksize` = 程序结尾符号（链接期定死）→ brk 缓存比较变化才调 VM → sbrk 方向检查拦回绕、返回旧边界 → NetBSD malloc 两条路（大块 mmap / 小块 sbrk 增长） | `libc/arch/i386/sys/brksize.S`、`brk.c:22-34`（:27 相等跳过）、`sbrk.c:13-25`（:20-21 方向检查）、`lib/libc/stdlib/malloc.c` | |
| S9 | 终局：exit 三级兜底（协议 → 非法跳转自杀 → 原地打转）；panic 阶梯（身份 → 消息 → 栈标记 → 钩子 → exit → 信号自杀 → 非法跳转 → 打转）；诊断通道攒后发（DIAG_BUFSIZE=2000 一屏） | `libc/sys/_exit.c:25-30`、`libsys/panic.c:34-66`、`libsys/kputc.c:17-32`（:22 先判后存）、`sys_diagctl.c`（四操作码） | 两条路分开的理由：恐慌时文件系统可能正是坏的那个 |

### 1.3 服务族真序表（五组封装的形状）

| 组 | C 形状 | 代表锚点 |
|----|--------|---------|
| PM（08） | fork 7 行（清零/协议/回复即孩子号）；exit 三级兜底；execve 全组最长（算栈→sbrk 腾地→五字段→协议→失败还地）；wait4 回抄双回；kill/raise 范围先查；srv_fork/kill 走**服务端协议变体**（错误直接负，不绕全局变量，`taskcall.c:9` 注释第一句） | `fork.c`、`_exit.c`、`execve.c:14-59`、`wait4.c:24-25`、`kill.c`、`raise.c`、`srv_fork.c`、`taskcall.c`；调用号 `callnr.h:9` 起（PM_BASE=0，EXIT 1/FORK 2/WAIT 3/GETPID 4/KILL 11/EXEC 14/SRV_FORK 41/SRV_KILL 42） |
| VFS（09） | 四形状：读写形（四字段去一数字回，载荷**地址在前长度在后** ipc.h:795-803）；开关形（创建位是道岔：创建布局 vs 路径布局）；定位形（载荷**偏移第一** ipc.h:726-734，回复体捎回新位置）；散列形（三关校验：段数≤1000/总长回绕/空针，零总长直接成功） | `read.c`/`write.c`、`open.c`（创建位 0x200，fcntl.h:99；调用号 0x103/0x104）、`lseek.c`、`vectorio.c`、`stat.c` 三函数一布局（ipc.h:874-880）、`ioctl.c`、`fcntl.c`、`getdents.c`（复用读写载荷）、`dup.c`（F_DUPFD 组合） |
| VM（10） | 映射七字段 + **替办模型**（受益人≠自己 → 加第三方标志 0x800000，mman.h:124）；三个哨兵（MAP_FAILED/0/全一）与合法值撞衫；边界缓存短路 | `mmap.c:21-47`（:36-38 替办）、`:143-171` 查物理/查引用、`vm_fork.c`、`vm_cache.c` 四件（对齐恐慌 + NO_DEV 断言 :17-31） |
| misc（11） | 睡觉 = select 空集+超时（参数三查/微秒向上取整/余量借位规整）；svrctl 字符分派（右移八位取低八位，ioccom.h:68）；kerninfo 直读（三数全算术零跑路；频率零 C 除零崩溃）；tsc 低前高后拼 64 位 | `nanosleep.c:22-95`、`svrctl.c`、`getticks.c`、`getuptime.c`、`clock_time.c:20-28`（两步防溢出）、`read_tsc_64.c` |
| RS（12） | 查找每次现查不缓存（重启立即可见）；反查端点进身份出（四身份全回）；getsysinfo 门牌路由（0/1/2/6 四家，陌生 ENOSYS） | `minix_rs.c:23-40`（RS_REQ_BASE 0x700+8）、`getepinfo.c`、`getprocnr.c`、`getsysinfo.c:8-32` |

### 1.4 客户端基建真序（post-plan 现实，见 §3.2 G1/G2）

| 步骤 | 动作 | C 锚点 | 说明 |
|------|------|--------|------|
| X1 | **内核调用 wrapper 族**（服务专用内核调用：sys_fork/exec/safecopy/sigsend/trace/diagctl/…约 50 个 C 封装）——Rust 侧已落地 34 个 `sys_*` wrapper 于 `minix-sys/src/syscall.rs`（E2/E6/E9 批），是 syscall.rs 49 个测试的主体 | `libsys/sys_*.c` 全族（plan A-10 排除项）；Rust：`syscall.rs:246-777+` | **plan A-10"不进入 minix-sys"已被既成事实取代**——蓝图处置见 §3.2 G1 |
| X2 | **用户态 grant 表**（授权表的槽位+空闲链+序号在授权者自己地址空间管理，cpf_grant_direct/revoke 纯内存表编辑不陷阱；槽布局是跨空间共享契约） | `libsys/safecopies.c`（381 行）；Rust：`minix-sys/src/grant.rs`（292 行，模块文档自述"the Rust rewrite of C libsys safecopies.c"，槽布局权威在 `minix-types::types::grant`） | **无文档归属**（04 篇 grep 零命中）——见 G2 |
| X3 | **栈镜像构建**（生产实现）：`STACK_MIN_SZ=1400`（LP64 裁决）、填充期边界检查（FrameTooSmall/SizeMismatch） | Rust：`minix-sys/src/stack.rs`（372 行，5 测试）；消费方 `os/servers/vm/src/vm_server.rs:786,791` | 01 篇 §3.4 已如实承载（V1-P0-3 修复后）✓ |

---

## 2. 知识点全集

### 2.1 知识点池总表

说明：**类型**取 概念/机制/接口协议/约束不变量/架构演进/工具工程/测试性质 八类；**来源**取 存量/新增。**主** = 唯一主讲述点。相关条目按"共生死"并 row。

#### 来自 00（3 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-001 | runtime 定位：一切 userland 共享的运行时库层，不是 server（无主循环/分发）；minix-types（契约）+ minix-sys（机制）+ minix-rt（出生与生存脚手架）三 crate 分工 | 概念 | 存量 | 00 Ch1 | `os/libs/*` 三 crate；plan §1.1 | 00 |
| K-002 | 生命周期主线：交付→crt0→初始化→服务→终局；**rt-birth 真机里程碑**（2026-09-17 五断言 PASS） | 概念 | 存量 | 00 Ch2 | `edge_todo.md:74`；`os/qemu-tests/test-rt-birth.sh` | 00 |
| K-003 | 设计原则四条 + 导航（6 阶段 15 篇） | 工具工程 | 存量 | 00 Ch3-4 | plan §3 | 00 |

#### 来自 01（10 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-010 | kerninfo 页：魔数 0xfc3b84bf 防伪 + 特性标志 + ki_flags 有效性位 + 用户接口双指针（ipcvecs/kuserinfo）+ 服务专用指针组（普通程序当不存在） | 接口协议 | 存量 | 01 §2.2 | `type.h:214-244`（:229,:236-243,:246-247） | 01 |
| K-011 | kuserinfo 两字段 + 追加-only 演进契约 + `KUSERINFO_HAS_FIELD` 长度探测（不烧版本号）→ Rust `user_info_has_field` 函数化 | 接口协议 | 存量 | 01 §2.1、§3.2 | `type.h:205-212`；`handoff.rs` | 01 |
| K-012 | 栈顶选择三态：新位优先/标志未设回退老位（偏移 2440）/页面缺失显式错误 → Rust `select_initial_stack_pointer` 纯函数（机制策略分离） | 机制 | 存量 | 01 §2.3、§3.3 | `kernel_utils.c:39-62`（:25-32 断言）；`handoff.rs` | 01 |
| K-013 | 栈镜像布局与填充：字符串置顶指针置底的理由、回绕捕捉、`*vsp+(fp-frame)` 一次加法换算 | 机制 | 存量 | 01 §2.4 | `stack_utils.c:28-60,76-172` | 01 |
| K-014 | **栈镜像构建的生产家在 minix-sys/stack.rs**（STACK_MIN_SZ=1400 LP64、填充期 FrameTooSmall/SizeMismatch 拦截 C 溢出标志家族；handoff 只选栈顶不重算尺寸——V1-P0-3 删平行实现后的单一真相） | 架构演进 | 存量 | 01 §3.4 | `os/libs/minix-sys/src/stack.rs`（5 测试）；`vm_server.rs:786,791` | 01 |
| K-015 | `ValidatedKernInfo` 构造即校验（非法状态不可表达）；`MagicMismatch` 独立错误型（Fix #5 收窄，NullPage 分支类型级消除） | 架构演进 | 存量 | 01 §3.1、§4 | `handoff.rs` | 01 |
| K-016 | `ProcessStrings::from_raw` 负数个数防御（C 隐式信任的显式化，DELIBERATE DIVERGENCE 声明） | 架构演进 | 存量 | 01 §3.5 | `handoff.rs`；`exec.h:104-116` | 01 |
| K-017 | 错误映射：MagicMismatch/NullPage→ENOEXEC、负个数→EINVAL | 约束不变量 | 存量 | 01 §4 | errno 语义 | 01 |
| K-018 | 测试面：handoff.rs 10 个测试（**§5 表只列 9 行，缺 :444 的 magic_mismatch 映射测试——Fix #5 所增未入表，E1**） | 测试性质 | 存量（勘误载体） | 01 §5 | `handoff.rs:325-444` 实测 10 | 01 |
| K-019 | kerninfo 页的 Rust 布局权威在 `minix-types::types::kerninfo`（E-KERNINFO/E-MIBPROD 镜像，99 篇 §1.4 记账）——01 篇未点名该权威落点（轻） | 架构演进 | **新增** | （01 未点名） | `minix-types/types/kerninfo.rs`；99 §1.4 | 99/01 |

#### 来自 02（8 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-030 | 入口三元组模型（清理回调/加载器描述/ps_strings；静态链接前两恒零）+ Minix 与 Linux/Redox 的栈传参差异（NetBSD ps_strings 源流） | 概念 | 存量 | 02 §1.1 | `crt0.S:44-49`；`exec.h:104-116` | 02 |
| K-031 | 汇编桩 6 条指令逐条语义（16 字节对齐/影子空间/三次搬运/跳转不调用）+ `_start` 别名 | 机制 | 存量 | 02 §2.1 | `crt0.S:40-49` | 02 |
| K-032 | `___start` 全流程：空指针 `_FATAL`（Minix 输出被禁=直接退）→ 发布 environ/ps_strings/progname（逐字节扫斜杠）→ 动态分支（静态永不可达，Rust 不建模）→ `_libc_init` 单次（initfini.c:84-119 七步）→ 列表正序 + atexit 登记 → main→exit | 机制 | 存量 | 02 §2.2-§2.3 | `crt0-common.c:77-81,144-192`；`libc/misc/initfini.c:84-119` | 02 |
| K-033 | 启动函数列表三族 + 老式构造器逆序（目标文件逆序存放的历史遗留） | 机制 | 存量 | 02 §1.2、§2.4 | `crt0-common.c:106-117`；`crtbegin.c:72-97` | 02 |
| K-034 | environ 哨兵 0x53535353（只比低两字节的理由；**C 无检测点**——start.rs 的"哨兵检测"叙述已随 V1-P1-1 修正为文档教训） | 约束不变量 | 存量 | 02 §2.2、§3 | `environ.c` | 02 |
| K-035 | Rust 诞生链现状：裸汇编 `_start`（`and rsp,-16`→`mov rdi,rbx`→call）+ `rt_birth` 六命名阶段 + 访问器族（progname/argv/env 借用字节切片零分配）——**rt-birth 真机验证** | 架构演进 | 存量 | 02 §3 | `crt0.rs`（7 测试）；`edge_todo.md:74` | 02 |
| K-036 | start.rs 删除史与五条接线准则（顺序要有名字/哨兵比较要有名字/字节扫描/单运行器/单次可拥有）——投机性设计教训的文档化 | 工具工程 | 存量 | 02 §3 | todo.md V1-P1-1 Fix #4 | 02 |
| K-037 | fini_array/init_array 维持登记（无消费方）；入口失败两态映射（ENOEXEC/EINVAL）与 birth_fail 路径 | 约束不变量 | 存量 | 02 §4-§5 | `crt0.rs`；todo V1-P1-1 | 02 |

#### 来自 03（8 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-050 | 构造器三步：查询→魔数→安装；**失败清零继续运行**（kerninfo 是增强能力非生存前提；默认直接陷阱表预装永不失效）；整表换防新旧混用 | 机制 | 存量 | 03 §1、§2.1 | `init.c`（全文 32 行，:10-18 预装） | 03 |
| K-051 | 向量表七函数指针 + 内联包装（`ipc.h:2786-2834`）；`ipc_minix_kerninfo` 陷阱（调用号 6，`ipc_minix_kerninfo.S` 13 行）是 03 与 04 的衔接点 | 接口协议 | 存量 | 03 §2.2 | `ipc.h:2786-2834` | 03（查询陷阱在 04） |
| K-052 | Rust `KerninfoSource` trait（真实陷阱 vs 测试预设，两实现不同行为——Gate D 双实现规则的正面引用） | 架构演进 | 存量 | 03 §3.1 | `init.rs` | 03 |
| K-053 | `RuntimeState` 可拥有状态 + `KerninfoAvailability` 三态枚举（QueryFailed/BadMagic/Available——C 的 NULL 压平三态，Rust 买回可诊断性；Fix #5 对齐 C 容错裁决） | 架构演进 | 存量 | 03 §3.2、§4 | `init.rs`；todo V1-P1-4 Fix #5 | 03 |
| K-054 | errno 全局模型 → Result 类型（`errno_to_negative`/`negative_to_errno` 边界转换唯一两处）；`ThreadLocalModel` 枚举说出 32/64 位两形态（A-4/A-5） | 架构演进 | 存量 | 03 §1.3、§2.3、§3.4-3.5 | `_errno.c:44-55`；`init.rs` | 03 |
| K-055 | progname 只读语义（setprogname 被 NetBSD 条件编译关闭）；三全局（environ/ps_strings/progname）哨兵→真值的发布时序 | 约束不变量 | 存量 | 03 §2.4-§2.5 | `getprogname.c:50-57`、`setprogname.c:53-65` | 02（发布）/03（只读） |
| K-056 | ipcvecs 维持不保留的裁决（64 位 syscall 直入内核；99 篇 §1.4 记账）——03 篇 C 侧保留教学、Rust 侧不装表 | 架构演进 | 存量 | 03 §2.2；99 §1.4 | 28 篇 D5（kernel stage） | 99 |
| K-057 | 测试面：init.rs 7 个测试（四分支 + 降级保后备表 + 宿主陷阱显式失败 + errno 往返） | 测试性质 | 存量 | 03 §5 | `init.rs` | 03 |

#### 来自 04（8 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-070 | 六原语语义与"共享同一套陷入约定"的核心观察（向量号粗分/调用号细分；33=IPC、32=内核调用，i386 特有） | 机制 | 存量 | 04 §1、§2.2 | `ipcconst.h`（两份）、`_ipc.S:16-87` | 04 |
| K-071 | 调用号七值（SEND 1…SENDA 16，6-16 空号预留）+ 56 字节编译期断言 + 状态字两段编码（低 6 位调用号/高位标志；**内核可信消息不可回复**） | 接口协议 | 存量 | 04 §2.1 | `ipcconst.h:6-35`（:28-31 注释） | 04 |
| K-072 | asynsend 生产消费模型：槽位四字段 + 六标志（八进制换算）+ 入队五步（**先写目标消息最后写标志**）/表满整理/取错确认一次 | 机制 | 存量 | 04 §1.3、§2.5 | `ipc.h:2745-2762`；`asynsend.c:17-188` | 04 |
| K-073 | Rust：命名常量+向量枚举/状态字类型（移位掩码私有化）/标志类型（十进制注明来源）/自有队列（`&mut self` 从构造上杜绝重入，表满还决定权给调用者） | 架构演进 | 存量 | 04 §3 | `ipc.rs`（12 测试） | 04 |
| K-074 | 传输 trait（真实实现在宿主测试显式失败不伪造；`real-trap` feature 门控真体 `arch_trap.rs`——**arch_trap.rs 0 测试，真机路径归 E1**） | 架构演进 | 存量 | 04 §3.5 | `ipc.rs`；`arch_trap.rs:61-107` | 04 |
| K-075 | **新增**：用户态 grant 表无文档归属——`grant.rs`（292 行，4 测试）是 C `safecopies.c`（381 行）的 Rust 重写：槽位+空闲链+序号在授权者地址空间自管、direct/revoke 纯内存编辑不陷阱、`sys_setgrant` 只向内核注册表地址、槽布局权威在 `minix-types::types::grant`（E-DSWIRE 产物，DS/devman/RS 客户端的公共传输半）——**04 篇 grep "grant" 零命中（G2）** | 机制 | **新增** | （缺） | `grant.rs` 模块文档；`safecopies.c`；`minix-types/types/grant` | 04 |
| K-076 | **新增**：arch_trap.rs 的归属声明（real-trap feature 真体；宿主 -EIO 诚实门控；通电挂 E1 切片 5）——04 篇 §3.5 提 trait 但未点名该文件（轻） | 架构演进 | **新增** | （04 未点名文件） | `arch_trap.rs` | 04 |

#### 来自 05（8 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-090 | `_syscall` 四步协议与两层失败模型（m_type 是唯一讲台：非负=成功/负数=错误号/往返故障先写状态码再解读） | 机制 | 存量 | 05 §1.1、§2.1 | `syscall.c:9-25`（:6 弱别名防重定义） | 05 |
| K-091 | 双路径打包（≤40 内联含 NUL；指针永远填；40 = `ipc.h:14` 内联缓冲的同数两面）→ Rust 打包结果类型（39/40/41 边界三测）——**LP64 判例后内联收缩为 32、超容量 ENAMETOOLONG 加固**（99 篇裁决，09 篇 §3.2 消费） | 接口协议 | 存量 | 05 §1.2、§2.2；09 §3.2 | `loadname.c:7-19`；`ipc.h:14,761-767` | 05（协议）/09（VFS 消费）/99（裁决） |
| K-092 | `_kernel_call` ENOTREADY(201) 线性退避（1,2,3…无界——改上限=改外部行为，否决案已记录）；tickdelay 零负直返 | 机制 | 存量 | 05 §1.3、§2.3、§3.3 | `kernel_call.c:7-21`；`errno.h:197`；`tickdelay.c` | 05 |
| K-093 | Rust 协议三纯函数（结果类型收编全局 errno/打包结构体不变量类型化/延迟回调参数化——无界循环保持 C 语义） | 架构演进 | 存量 | 05 §3 | `syscall.rs` 协议半 | 05 |
| K-094 | **新增**：SYS_* 内核调用 wrapper 家族无文档归属——`syscall.rs` 现有 34 个 `pub fn sys_*`（sys_fork/exec/safecopyfrom/safecopyto/update/diagctl/sigsend/getksig/endksig/trace/runctl/kill/times/vircopy/clear/setalarm/get_machine/abort/resume/diagctl_write…，49 个测试的主体），是 C `libsys/sys_*.c` 全族（plan A-10 排除项）的 Rust 落地（E2/E6/E9 批，消费方：VM gateway/init/server 们）——**05 篇 grep "SYS_\|sys_fork" 零命中（G1）**；C 侧对应概念 `_kernel_call` 已属 05，wrapper 是其执行家族，归属 05 增补一节最自然 | 机制 | **新增** | （缺） | `syscall.rs:246-777+`；`libsys/sys_*.c`；edge E2/E6/E9 批记录 | 05 |
| K-095 | 测试面：syscall.rs 现 49 个测试（协议半 12 个已入 05 §5 表；wrapper 半 37 个无表）——**05 篇 §5 表是旧快照且无 wrapper 半（随 G1 增补刷新）** | 测试性质 | **新增** | （部分缺） | `syscall.rs` 逐个实测 | 05 |
| K-096 | 弱别名手法（公开名=内部名，防用户重定义踩坏系统调用）——syscall.c/brk.c/_exit.c 三处同款 | 工具工程 | 存量 | 05 §2.1；08 §2.2 | `syscall.c:6`、`brk.c:22 注释`、`_exit.c` | 05 |
| K-097 | grant 表与 SYS_* wrapper 的边界声明（99 篇增补，见 G3）：minix-sys 现状三类共享基建（SYS_* wrapper→05、grant 表→04、stack 布局→01），plan A-10"不进入 minix-sys"按既成事实修订 | 架构演进 | **新增** | （缺） | plan:165,:276；`syscall.rs`/`grant.rs`/`stack.rs` | 99 |

#### 来自 06（7 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-110 | 堆两层模型（下层管边界少调：地址没变就不调；上层管切分：小块分档大块整页）；`_brksize`=程序结尾符号链接期定死 | 机制 | 存量 | 06 §1、§2.1 | `brksize.S`；`brk.c:27` 相等跳过 | 06 |
| K-111 | brk/sbrk 规则：缓存只在成功路径更新；sbrk 方向检查拦双向回绕、**返回旧边界**；弱别名手法 | 机制 | 存量 | 06 §2.2-§2.3 | `brk.c:22-34`、`sbrk.c:13-25`（:20-21） | 06 |
| K-112 | NetBSD malloc 两条路（页描述符+魔数竞技场；:317 大块路、:377-388 sbrk 增长路——**V1-P2-3 #8 修正：317-388 是 sbrk 增长非"大对象直走 mmap"，MMAP 用于页目录 :448/:559**） | 机制 | 存量 | 06 §2.4 | `malloc.c:162,193,317,377-388,448,559` | 06 |
| K-113 | Rust 平板分配器：九档 8→2048（8 字节对齐是副产品）、空位链表内嵌空槽前两字节零管理内存、元数据六项在档记录 | 架构演进 | 存量 | 06 §3.2 | `alloc.rs` | 06 |
| K-114 | 页供应接口（要一页/要连续多页/还页；测试两实现：永远拒绝+数组切；生产第二供给=VM 通道接通后补）——供给换代不改切分 | 架构演进 | 存量 | 06 §1.3、§3.3 | `alloc.rs` | 06 |
| K-115 | 分配器的显式失败（零尺寸/耗尽/记录用尽→空指针；陌生指针与错位指针直接终止——静默损坏当场停）+ 错误映射（ENOMEM/EINVAL） | 约束不变量 | 存量 | 06 §3.4、§4 | `alloc.rs` | 06 |
| K-116 | 测试面：alloc.rs 11 + lib.rs 全局 3 = 14（页对齐测试池 `#[repr(align(4096))]`——V1-P3-1 Fix #13 的"改夹具不硬凑断言"教训） | 测试性质 | 存量 | 06 §5 | `alloc.rs`、`lib.rs` | 06 |

#### 来自 07（8 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-130 | 诊断通道定位（服务/驱动专用、不走文件系统、内核环形日志）；staging 攒后发（DIAG_BUFSIZE=2000 一屏、零字节句号满缓冲换页、先判后存） | 机制 | 存量 | 07 §1.1、§2.1 | `kputc.c`；`com.h:416` | 07 |
| K-131 | sys_diagctl 四操作码（打印/栈/注册/注销；未知码 C 直接恐慌→Rust 构造期拒绝还决定权） | 接口协议 | 存量 | 07 §2.2、§4 | `sys_diagctl.c:5-27`；`com.h:412-415`；`syslib.h:164-171` | 07 |
| K-132 | panic 阶梯八级（身份→消息→栈标→钩子→exit→信号→非法跳转→打转）+ assert 前置 + itoa 五位局限（C 真实局限，Rust 修正性改进如实标注） | 机制 | 存量 | 07 §1.2、§2.3-§2.4 | `panic.c:34-66`、`assert.c:12-22`、`itoa.c:9-35`、`stderr.c:6-12` | 07 |
| K-133 | Rust 单一格式化之家：`format_panic_report`（位置前缀+消息+换行；**静默截断返回实写字节数恒≤缓冲**——测试抓出的契约修正）+ handler 三步（格式化→发送（钩子或 SpinSink）→打转）；PanicStage 枚举删除留文档（YAGNI） | 架构演进 | 存量 | 07 §3.3 | `diag.rs`；todo V1-P1-2 Fix #2 | 07 |
| K-134 | 恐慌 handler 唯一性与注册表归属：`#[panic_handler]` 全镜像一份 → 住 minix-rt `panic-handler` feature 门后；钩子注册表住 `minix-types::types::diagnostic`（依赖箭头唯一解：kernel→minix-types←minix-rt；V1-P0-1 修复） | 架构演进 | 存量 | 07 §3.4 | `minix-types/types/diagnostic.rs:35`；todo V1-P0-1 Fix #1 | 07 |
| K-135 | 输出端 trait（旋转/捕获两实现）分离格式化与发送；数字写调用者内存（截断报全长对 C 静态缓冲的修正）；写刷尾便利方法（C 转场掉尾条的补齐） | 架构演进 | 存量 | 07 §3.1-§3.2 | `diag.rs` | 07 |
| K-136 | A-8 演进四阶段（打转→格式化→接通道→接退出）与两处已登记分歧（DiagCode::from_number 返 None；write_and_flush 附加行为） | 架构演进 | 存量 | 07 §1.3、§4、todo §3.3 | todo V1-P1-2 边界段 | 07 |
| K-137 | 测试面：diag.rs 14 个（报告形状/静默截断两行是 Fix #2 后新契约） | 测试性质 | 存量 | 07 §5 | `diag.rs` | 07 |

#### 来自 08（8 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-150 | PM 三族心智模型（生命：创建两份回话/执行一去不回/退出不回话/等孩子连吃带拿；信号：范围先查再递；服务启停：服务端协议变体错误直接负） | 概念 | 存量 | 08 §1 | `fork.c`/`_exit.c`/`execve.c`/`wait4.c`/`taskcall.c:9` | 08 |
| K-151 | execve 会师点（01 算栈 + 06 腾地 + 05 协议三篇会师；成功没有失败返回路） | 机制 | 存量 | 08 §2.3 | `execve.c:14-59` | 08 |
| K-152 | 调用号清单（PM_BASE=0：EXIT 1/FORK 2/WAIT 3/GETPID 4/KILL 11/EXEC 14/SRV_FORK 41/SRV_KILL 42；PM 端点 0）；子集策略声明（同形清单收录不逐个铺开） | 接口协议 | 存量 | 08 §2.7、§1 | `callnr.h:9` 起；`com.h:59` | 08 |
| K-153 | Rust：传输泛型第三次套用/双形态函数（用户 Result vs 服务端原始负回复）/预执行请求拆分（签名变化如实记录：旧签名无调用者）/本地 56 字节布局 + 字节序断言 | 架构演进 | 存量 | 08 §3 | `pm.rs` | 08 |
| K-154 | 信号族 wire 半已由 edge E7 落入 minix-types（MessLcPmSig 等），wrapper 半归 04-stage-pm 批次表（todo §2.1 对账）；A-9 信号运行时语义面仍缺 [待验证] | 架构演进 | 存量 | 08 §5 外（todo §2.1） | `minix-types ipc/pm`；`04-stage-pm/todo.md §11.1.1` | 08（登记） |
| K-155 | 测试面：pm.rs 现 29 个测试（**08 §5 表 13 行为 2026-09-05 快照——E7/E9 批后未刷新，随 B 相按实有重列**） | 测试性质 | 存量（快照漂移） | 08 §5 | `pm.rs` 实测 29 | 08 |
| K-156 | raise 与 NetBSD 线程版同名的澄清（计划清单组合面指用户态组合逻辑） | 工具工程 | 存量 | 08 §2.5 | `raise.c` | 08 |
| K-157 | 服务端变体的存在理由（服务间错误就地处理；注释第一句声明是有意接口差异） | 约束不变量 | 存量 | 08 §1.3、§2.6 | `taskcall.c:9` | 08 |

#### 来自 09（9 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-170 | 四形状总纲（读写形/开关形/定位形/散列形）+"四字段去一数字回" | 概念 | 存量 | 09 §1 | `read.c`/`write.c`/`open.c`/`lseek.c`/`vectorio.c` | 09 |
| K-171 | 布局反直觉两处（读写载荷**地址在前长度在后** ipc.h:795-803 vs 赋值顺序；定位载荷**偏移第一** ipc.h:726-734）——抄字段必须看定义，Rust 布局结构体+逐字节测试锁死 | 接口协议 | 存量 | 09 §2.1、§2.4、§3.1 | `ipc.h:726-734,795-803` | 09 |
| K-172 | 打开道岔（创建位 0x200 分创建布局 vs 路径布局；调用号 0x103/0x104）；**LP64 落地下文**（内联 40→32 含 NUL、超容量 ENAMETOOLONG 加固偏差——C loadname 超长静默跳过 strcpy 属 UB；99 篇裁决 2026-09-17） | 接口协议 | 存量 | 09 §2.2、§3.2 | `open.c`；`fcntl.h:99`；`callnr.h:68,75-76` | 09 |
| K-173 | 散列三关（段数≤1000 参数错误/总长回绕参数错误/空针坏地址 EFAULT——三关错误号不同可分清错因）+ 零总长直接成功 | 约束不变量 | 存量 | 09 §1.3、§2.5、§3.3 | `vectorio.c` | 09 |
| K-174 | 指针载荷三变体（stat 族一布局三调用号、ioctl 请求号定形状方向、fcntl 整数指针并排一次填一）+ getdents 复用读写载荷（非零预留计数服务端报参数错误 vfs/read.c:282）+ `minix_types::types::stat::Stat` 152 字节布局见证 | 接口协议 | 存量 | 09 §2.7、§3.5 | `stat.c`、`ipc.h:874-880,699-705,655-662`；`sys/stat.h:59-97` | 09 |
| K-175 | 等待形状（空集+超时；通用 select 暂缓待 fd_set 消费方——V1-P1-3 Fix #10 落地 select_empty_via）+ dup/fstatfs 等组合面老路新用 | 机制 | 存量 | 09 §2.3、§3.4 | `close.c`、`dup.c`、`fcntl.h:178` | 09 |
| K-176 | 调用号清单（VFS_BASE 0x100：READ+0…SELECT+30；套接字族排除归 17-stage-net） | 接口协议 | 存量 | 09 §2.6 | `callnr.h:68-135` | 09 |
| K-177 | Rust：分派枚举/布局结构体家族/纯校验函数/TimeVal LP64/select_empty_via | 架构演进 | 存量 | 09 §3 | `vfs.rs` | 09 |
| K-178 | 测试面：vfs.rs 29 个测试（**09 §5 表 23 行为 2026-09-18 快照，此后 E7 批又有增量——B 相按实有重列**） | 测试性质 | 存量（快照漂移） | 09 §5 | `vfs.rs` 实测 29 | 09 |

#### 来自 10（9 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-190 | 替办模型（受益人≠自己 → 第三方标志 0x800000；记账正确的全部保障就三行）；哨兵模型（MAP_FAILED/0/全一与合法值撞衫 → Rust 单层 Result；V1-P2-1 Fix #6 收窄双层签名弯路如实记载） | 约束不变量 | 存量 | 10 §1、§3.2 | `mmap.c:21-47`（:36-38,:44）；`mman.h:124,130`；todo Fix #6 | 10 |
| K-191 | 边界缓存短路（消费 06 规则不重复定义——跨文档复用走函数不走复制） | 机制 | 存量 | 10 §1.3、§3.3 | `brk.c:27` 消费面 | 10 |
| K-192 | 解除/重映射/共享解除/查物理/查引用形状（载荷栏位各表；失败哨兵消除） | 机制 | 存量 | 10 §2.2-§2.3 | `mmap.c:76-171` | 10 |
| K-193 | 客户端库余量清单（§2.5 七行表：vm_info 三函数/vm_procctl 两公开/vm_cache 四件/vm_getrusage/vm_willexit/vm_unmap_phys/minix_vfs_mmap——V1-P0-2 Fix #7 的契约对账产物）+ 三层现状（wire 半 info/procctl/rusage/cache 四族备好；封装半缓存四件+预告退出+物理解除+procctl 两件已落地；**vm_info 三函数暂缓已论证**——指针写回语义待定稿且零消费方，防 start.rs 式投机） | 机制 | 存量 | 10 §2.5 | `vm_info.c:10,24,39`、`vm_procctl.c:10,28,33`、`vm_cache.c:15,47,59,68,77`、`vm_getrusage.c:7`、`vm_exit.c:25`、`vm_map_phys.c:33`、`mmap.c:49` | 10 |
| K-194 | cache 族细节（m_vmmcp 64 位加宽布局镜像权威=VM 服务器解码侧；对齐恐慌与 NO_DEV 断言是调用方缺陷→Rust 同形 panic 不发明错误通道；flags_ptr 栏服务器暂不回写如实注释） | 机制 | 存量 | 10 §2.5（Fix #9 段） | `vm_cache.c:17-31`；`minix-types VmCacheIn` | 10 |
| K-195 | 排除三项维持（vm_set_priv/update/memctl/prepare 归集成 stage；SHM_UNMAP 归 E-IPCWIRE；REMAP_RO 待 E-MINTYPES-RUNTIME） | 约束不变量 | 存量 | 10 §2.5 末 | plan §5.3 | 10 |
| K-196 | Rust：请求类型收拢七字段（第三方标志成方法）/单层 Result 消哨兵/缓存短路消费 06 规则 | 架构演进 | 存量 | 10 §3 | `vm.rs` | 10 |
| K-197 | 测试面：vm.rs 现 30 个测试（**10 §5 表 14 行为 Fix #7 与 #8 之间快照——Fix #8/#9 的 11 个测试未入表，B 相按实有重列**） | 测试性质 | 存量（快照漂移） | 10 §5 | `vm.rs` 实测 30 | 10 |
| K-198 | 调用号清单（VM_RQ_BASE 0xC00：EXIT+0…PROCCTL+45/VFS_MMAP+46/GETRUSAGE+47；特权/热更新/内存控制三件列编号不封装） | 接口协议 | 存量 | 10 §2.4 | `com.h` VM 段 | 10 |

#### 来自 11（7 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-210 | 四需求四解法总纲（等待复用/字符分派/直读/拼接） | 概念 | 存量 | 11 §1 | 四文件 | 11 |
| K-211 | 睡觉三段（参数三查含纳秒越界/微秒向上取整非零不归零/余量借位规整超支清零——**V1-P1-3 修正：余量不依赖 select 回写，C 用 gettimeofday 挂钟差**） | 机制 | 存量 | 11 §2.1、§3.1 | `nanosleep.c:22-95`；todo Fix #10 | 11 |
| K-212 | svrctl 字符分派（右移八位取低八位；两 PM 字符+一 VFS 字符；陌生 EINVAL）→ Rust 枚举+svrctl_via 双路由（PM 38/VFS 0x12B） | 机制 | 存量 | 11 §2.2、§3.2 | `svrctl.c`；`ioccom.h:68`；`callnr.h:51,115` | 11 |
| K-213 | kerninfo 直读四函数（tick/三元组/挂钟两步防溢出 40000×25000=10^9/tsc 低前高后）+ 频率零加固（C 除零崩溃→Rust 返回启动秒+零小数）；32 位字段原子读假设与 64 位待办 | 机制 | 存量 | 11 §2.3、§3.3 | `getticks.c:7-13`、`getuptime.c`、`clock_time.c:20-28`、`read_tsc_64.c` | 11 |
| K-214 | `__sysctl` 归属澄清（MIB_PROC_NR，10-stage-mib 的客户端；短名 ≤8 内联规则；失败也回写旧长度的 NetBSD 习惯）——发送半挂 E-RMIBWIRE 轨道 | 接口协议 | 存量 | 11 §2.2；plan R-19 | `__sysctl.c`；`com.h:66,1026` | 11（客户端面）/10-stage-mib（服务端） |
| K-215 | Rust：纯函数四件套 + 快照结构体（四个字段收进 struct，读函数是快照方法——分层同 10 消费 06）+ nanosleep_via 组合 | 架构演进 | 存量 | 11 §3 | `misc.rs` | 11 |
| K-216 | 测试面：misc.rs 16 个测试（Fix #10 后刷新 ✓ 与实测一致） | 测试性质 | 存量 | 11 §5 | `misc.rs` 实测 16 | 11 |

#### 来自 12（6 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-230 | 查找不缓存（重启立即可见；缓存的失效策略三问比一次往返贵；测试以调用计数锁死） | 约束不变量 | 存量 | 12 §1.1、§3.1 | `minix_rs.c:23-40` | 12 |
| K-231 | 反查元组化（C 空指针跳过 → Rust 全回四元组；窄 helper 在调用点自然形成不需三函数） | 架构演进 | 存量 | 12 §1.2、§3.2 | `getepinfo.c`、`getprocnr.c` | 12 |
| K-232 | 门牌路由（0/1/2/6 四家各有 getsysinfo 调用号；陌生 ENOSYS 零次往返） | 机制 | 存量 | 12 §1.3、§2.3、§3.3 | `getsysinfo.c:8-32`；`callnr.h:60`、`callnr.h:120`、`rs.h`、`com.h:1022+7` | 12 |
| K-233 | 载荷细节（RS_REQ_BASE 0x700+8；回复端点栏在偏移 8；getepinfo 回复四身份+组数偏移 0/4/8/12） | 接口协议 | 存量 | 12 §2.1-§2.2 | `ipc.h:1887-1896` | 12 |
| K-234 | Rust：无缓存查找/元组/路由纯函数 | 架构演进 | 存量 | 12 §3 | `rs.rs` | 12 |
| K-235 | 测试面：rs.rs 现 10 个测试（**12 §5 表 9 行为 09-05 快照，E 批后 +1 未刷新**） | 测试性质 | 存量（快照漂移） | 12 §5 | `rs.rs` 实测 10 | 12 |

#### 来自 13（6 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-250 | 对账模型（机器数一遍人工逐项认；名字值行号三列齐全）+ 归属模型（一处权威到处引用：错误号/信号住 minix-types，调用号/端点住各调用组） | 工具工程 | 存量 | 13 §1、§1.1-§1.2 | `errno.rs` | 13 |
| K-251 | errno 全表 115=115（脚本 diff 22 缺口补全；96 号双名 C 有意别名；两层门牌：权威 types/errno.rs + minix-sys 顶层再导出供命令面 `use minix_sys::EEXIST`） | 接口协议 | 存量 | 13 §2.1、§3.1 | `sys/sys/errno.h`；`types/errno.rs`（对账测试 :551） | 13 |
| K-252 | signal 新模块（32 常量+范围规则；跨组概念住共享库；调用组删副本改调共享函数） | 接口协议 | 存量 | 13 §2.2、§3.2 | `sys/sys/signal.h:45,52-84`；`types/signal.rs` | 13 |
| K-253 | 调用号分段与端点排序（PM_BASE 0 用 1-47/VFS 0x100 用 +0-63 套接字族排除/端点按启动顺序 0-10）+ 六个 pin 测试家族 | 接口协议 | 存量 | 13 §2.3-§2.4 | `callnr.h`、`com.h:59-66` | 13 |
| K-254 | 大族登记制（termios 304 行 116 定义等按需移植三理由；本轮三件先例：创建标志/复制命令/段数上界） | 工具工程 | 存量 | 13 §2.5、§3.3 | `sys/sys/termios.h` | 13 |
| K-255 | 测试面：minix-types 268（13 篇记录 167 为 09-05 快照；errno/signal 模块测试在册） | 测试性质 | 存量（快照） | 13 §5 | `cargo test -p minix-types` 实测 268 | 13 |

#### 来自 99 + 纯新增（6 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-260 | endpoint/generation 语义（槽位+代数防旧消息打新进程；Endpoint(pub i32)+get()；常量表与 ANY/NONE 哨兵） | 接口协议 | 存量 | 99 §1.1 | `type.h/endpoint.h`；`types/endpoint.rs` | 99 |
| K-261 | 56 字节负载契约与 LP64 判例（指针域 4→8、padding 等比收缩、总长保持 56；新增 wire 结构必须带 size_of/offset_of 见证——E-ISPROD 事故防线） | 约束不变量 | 存量 | 99 §1.2 | `ipc.h _ASSERT_MSG_SIZE`；MprocWire 464B 等 | 99 |
| K-262 | 调用号体例归一裁决（libc/服务侧发绝对值 0x600+偏移；内核入口归一一次减基直通，C mpx.S 同型；宿主测试可喂相对值——T2 真机发现修复） | 约束不变量 | 存量 | 99 §1.3 | `com.h:205`；`kernel_call_dispatch_inner` | 99 |
| K-263 | 全局状态表记账（kerninfo 页已实装 types/kerninfo.rs+kerninfo::init；ipcvecs 不保留） | 架构演进 | 存量 | 99 §1.4 | `types/kerninfo.rs` | 99 |
| K-264 | **新增**：minix-sys 现状边界声明（G3）——三类共享基建的归属表：SYS_* 内核调用 wrapper（syscall.rs 上半）→05、用户态 grant 表（grant.rs）→04、栈镜像构建（stack.rs）→01；plan A-10"服务端 syscall 面不进入 minix-sys"按 E2/E6/E9 既成事实修订为"进入 minix-sys 但按语义分篇承载，server 专用的高层封装仍归各 server stage"——99 篇与 plan 双处回写 | 架构演进 | **新增** | （缺） | plan:165,:276；`syscall.rs`/`grant.rs`/`stack.rs` | 99 |
| K-265 | **新增**：测试资产总表（B 相落 00 篇导航或 13 篇）——rt 53（diag 14/alloc 11/handoff 10/init 7/crt0 7/lib 4）+ sys 域内 184（syscall 49/vm 30/pm 29/vfs 29/misc 16/ipc 12/rs 10/stack 5/grant 4）+ types 268；各篇 §5 快照与实有的对账方法（Gate E 口径：中文描述标签逐一对代码 fn） | 测试性质 | **新增** | （散见各篇） | §0.3 计数输出 | 13（对账文化延伸） |

### 2.2 统计摘要

- **总条数**：约 128 条（存量约 117 + 新增约 11；新增中 3 条同时是勘误/缺口载体 K-018/E1、K-094/G1、K-075/G2）。
- **按类型分布**：概念 7、机制 51、接口协议 26、约束不变量 19、架构演进 20、工具工程 4、测试性质 1+（并row 后约数）。
- **按现有文档分布**：00:3、01:10、02:8、03:8、04:8、05:8、06:7、07:8、08:8、09:9、10:9、11:7、12:6、13:6、99:5；纯新增 3 条（K-094/K-097/K-265 部分重合）。
- **重复标记**：K-014、K-019、K-035、K-051、K-055、K-091、K-096、K-151、K-214、K-255 存在跨篇讲述，主讲述点已标。

---

## 3. 覆盖审计

### 3.1 主题全集与来源

四路来源：① C 符号面——`lib/csu/` 三文件 + `libc/sys/` 133 .c（plan §5.1 全映射，本轮行数抽核 24 文件全对）+ `libc/gen` 组合面 + `libsys` 共享子集（kernel_call/kputc/sys_diagctl/panic/assert/asynsend/getticks/getuptime/clock_time/getsysinfo/getepinfo/getprocnr/srv_fork/srv_kill/taskcall/tickdelay/**safecopies**/vm_* 11 文件）+ 头文件 14 份；② 操作系统通用概念——运行时启动链、构造器初始化、分配器分层、诊断通道、服务协议；③ 非 C 制品——crt0.S 汇编、链接期符号（`_brksize`/`_start` 别名/弱别名族）、56 字节消息断言、rt-birth 冒烟内核与 `test-rt-birth.sh`；④ 阶段边界契约——plan A-1~A-10、edge E-MINTYPES-RUNTIME/E-MINSYS-SCOPE/E1。

**三向对账结论**（承接 todo §2 矩阵，本轮独立复核其 V1 修复后的状态）：V1 轮 11 条中 10 条已闭环（含三项 P0），代码与文档在 2026-09-16/17 两轮大批量同步；**本轮新发现的缺口集中在"V1 之后 E2/E6/E9 批带来的新现实"**——SYS_* wrapper 族与 grant 表在 minix-sys 落地后没有文档归属（G1/G2），这是 plan A-10 边界决策被既成事实超越的产物，不是 V1 轮的遗漏（V1 扫描时 syscall.rs 尚无 wrapper 半）。

### 3.2 覆盖缺口表

| # | 缺口/失真主题 | 证据 | 建议归属 | 处置 |
|---|--------------|------|---------|------|
| G1 | **SYS_* 内核调用 wrapper 族无文档归属**：`syscall.rs` 上半（sys_fork/exec/safecopyfrom/safecopyto/update/diagctl/sigsend/getksig/endksig/trace/runctl/kill/times/vircopy/clear/setalarm/get_machine/abort/resume/diagctl_write 等 34 个 wrapper，37 个测试）是 C `libsys/sys_*.c` 全族的 Rust 落地（E2/E6/E9 批），05 篇 grep "SYS_\|sys_fork" 零命中 | `syscall.rs:246-777+`；05 §5 表只有协议半 12 测试 | **05 篇增补"内核调用 wrapper 家族"一节**（C `_kernel_call` 已属 05，wrapper 是其执行家族；A-10 联动见 G3） | 增补（K-094/K-095） |
| G2 | **用户态 grant 表无文档归属**：`grant.rs`（292 行，4 测试）模块文档自述是 C `safecopies.c`（381 行）的 Rust 重写（槽位+空闲链+序号自管、纯内存表编辑、槽布局权威在 minix-types），04 篇 grep "grant" 零命中 | `grant.rs` 模块文档；`safecopies.c`；04 篇 | **04 篇增补"grant 表"一节**（grant 是 IPC 机制的配套：陷入搬运消息，grant 授权缓冲——同一"怎么和外界换数据"语义单元） | 增补（K-075） |
| G3 | **plan A-10 与 §5.1 :276 的"不进入 minix-sys"已过时**：E2/E6/E9 批把内核调用 wrapper、grant 表、stack 布局三类共享基建落进 minix-sys（既有提交链可溯） | plan:165,:276 vs `syscall.rs`/`grant.rs`/`stack.rs` 实测 | plan 回写 + **99 篇增补边界声明**（K-097/K-264）：server 专用高层封装仍归各 server stage，共享传输基建归 minix-sys 分篇承载 | 回写 + 增补 |
| G4 | **测试清单快照漂移四处**：01 §5 表 9 行 vs 自称 10 个（**E1：Fix #5 所增 `test_magic_mismatch_maps_to_executable_format_errno` :444 未入表**）；05 §5 表 12 行 vs syscall.rs 实有 49；08 §5 表 13 行 vs pm.rs 实有 29；10 §5 表 14 行 vs vm.rs 实有 30（Fix #8/#9 的 11 个未入表）；12 §5 表 9 行 vs rs.rs 实有 10。总数快照已声明日期不算错，但表格作为"本模块测试清单"已不完整 | 逐模块 `#[test]` 计数（§0.3）；`handoff.rs:444` | 各篇 §5（B 相按实有重列 + 表头加"基线日期+全 crate 计数"）；00 或 13 加一张全 crate 测试资产总表（K-265） | 勘误 E1 + 刷新 |
| G5 | 01 篇未点名 kerninfo 布局权威落点（`minix-types::types::kerninfo`，99 §1.4 已记账）——01 讲 C 布局与 handoff.rs 解码，但 Rust 侧 wire 权威在哪一句话没说（轻） | `types/kerninfo.rs`；99 §1.4 | 01 §3 或 §7 一句 | 增补（K-019，轻） |
| G6 | 04 篇未点名 arch_trap.rs（real-trap feature 真体；宿主 -EIO 门控；0 测试——真机路径归 E1） | `arch_trap.rs:61-107`；todo Gate D | 04 §3.5 一句 | 增补（K-076，轻） |
| G7 | 杂项：`grant.rs:3` 模块文档 "C'scpf mechanism" 缺空格（代码注释 style，随触碰顺带） | `grant.rs:3` | （代码侧，B 相顺带） | style |
| G8 | 悬置项维持：V1-P2-2 wire 打包双体系的跨 crate 半挂 E-MINTYPES-RUNTIME；08 信号运行时语义面 [待验证]；termios 大族登记制 | todo §3.2/§2.1；13 §2.5 | edge/04-stage-pm 轨道（不属本蓝图） | 维持登记 |

### 3.3 重复主题表

| # | 主题 | 出现处 | 主讲述点 | 其余处置 |
|---|------|--------|---------|---------|
| R1 | ENOMEM/EINVAL 错误映射表 | 01/06/08/09/10/11/12 各篇 §4 | 各篇只映射**自己的失败族**（无跨篇重复——表式分治已是最优形态） | 13 篇管常量值，各篇管用法 |
| R2 | 弱别名手法 | 05 §2.1、08 §2.2、06 §2.2 | 05（首次出现即完整） | 06/08 引用 |
| R3 | kerninfo 页 | 01 §2.2（结构主）、03 §2.1（查询与校验）、11 §2.3（直读消费）、99 §1.4（记账） | 01 | 03/11/99 引用 |
| R4 | 双路径打包 | 05 §1.2（协议主）、09 §2.2/§3.2（VFS 消费+LP64 落地） | 05（规则）/09（落地下文） | 99 裁决 |
| R5 | 传输 trait/剧本传输 | 04 §3.5（建立）、05/08/09/10/11/12（套用） | 04 | 各篇"第 N 次套用"一句 |
| R6 | 服务端协议变体 | 05 §2.3（kernel_call 机制）、08 §2.6/§3.2（srv_* 消费） | 05（机制）/08（消费面） | 维持 |
| R7 | ps_strings/kuserinfo | 01 §2.1-§2.2（ABI 主）、02 §1.1（入口消费） | 01 | 02 引用 |
| R8 | 边界缓存规则 | 06 §1.1（定义主）、10 §1.3/§3.3（消费） | 06 | 10 引用（跨文档复用走函数） |
| R9 | progname/environ 发布 | 02 §2.2-§2.3（发布主）、03 §2.4-§2.5（只读语义） | 02 | 03 引用 |
| R10 | 各篇 §5 测试表 | 01~13 各篇 | 各篇保留自家清单（B 相刷新） | G4 的总表归 00/13 |

### 3.4 越界主题表

| # | 越界/可疑内容 | 判定 | 处置 |
|---|--------------|------|------|
| Y1 | 02 §3/§7、03 §3.2、06 §3.4、07 §3.3 的 Redox relibc 对照 | 教学性对照且经 V1-P2-3 #4 修正过虚构引用（"linker crate"已改 relibc） | 保留 |
| Y2 | 01 §3.3、05 §3.2、10 §3.2 的 Redox/OS 理论对照（机制策略分离、打包与陷入分离、哨兵消除） | 概念章职责内 | 保留 |
| Y3 | 07 §3.4 的依赖方向论证（kernel→minix-types←minix-rt） | 本 stage 的关键架构声明，有 V1-P0-1 修复背书 | 保留 |
| Y4 | 04 §2.2 的 i386 汇编逐行讲解（本仓 x86-64 目标） | C ground truth 本身是 i386 特有（plan A-6 已声明 64 位演进），教学必要 | 保留 |
| Y5 | 08 §2.7 提到 `vfs/read.c:282`（服务端行为）作 getdents 的佐证 | 跨 stage 单行佐证，有锚点 | 保留 |
| Y6 | 各篇 §5 测试名的中文描述标签（非 Rust 标识符） | Gate E 已对账到代码（todo §1），风格属"白话重写"批次的刻意选择 | 保留；B 相刷新表格时补 Rust 标识符列（轻） |

### 3.5 非 C 主题逐项回答（固定清单）

| 主题 | 在哪讲 / 为什么不在本 stage |
|------|---------------------------|
| 链接与加载 | **csu 三文件 + 链接期符号（`_start` 别名、`_brksize`=结尾符号、弱别名族、init_array/fini_array 段）是本 stage 核心链接主题**——02/06 承载；ELF 装载器（load_vm_elf/libexec）归 kernel/02-stage-vm 轨道（rt-birth 的 load_vm_elf 根因修复在 edge E1 注记） |
| 镜像与内存布局 | 初始栈镜像布局（01/stack.rs）、kerninfo/kuserinfo 页（01）、56 字节消息与 LP64 判例（99）、MprocWire 等镜像（99 §1.2）——本 stage 的布局主题已全覆盖 |
| 汇编入口与陷阱进入 | **crt0.S 6 条指令（02）与 `_ipc.S`/`ipc_minix_kerninfo.S` 陷阱函数（04）是本 stage 的汇编主题**；内核侧陷阱处理归 01-stage-kernel；arch_trap.rs 真体归 E1 通电 |
| 启动装配 | 生命周期主线即启动装配（00/01/02/03）；服务器级启动（SEF）归各 server stage 与 minix-sef |
| 构建与工具链 | libminc/Makefile 组成 = A-2 排除基线（00/plan §5.1 已核对）；`real-trap`/`panic-handler`/`alloc-global` feature 门（04/07/02 各自声明）；MISSING_SYSCALLS WONTFIX（plan §5.3） |
| 跨模块接口与线格式 | 六载荷族与 LP64 判例（99）、各服务载荷（08~12 + minix-types）、grant 槽布局（04 增补/G2）——wire 权威在 minix-types（E-MINTYPES-RUNTIME 收敛中） |
| 错误路径 | 各篇 §4 错误表（表式分治）+ 13（常量值）+ 07（恐慌路径）；errno 全表 115=115（13） |
| 关闭与退出 | exit 三级兜底与 PM_EXIT（08 §2.2）；panic 阶梯（07）；`minix_sys::exit` 委托链（02 §3、V1-P2-3 #2 修正后表述） |
| 并发与同步 | 单线程用户态前提（00 隐含 + 03 全局状态论述）；TLS 两形态枚举（03，A-4）；多线程构建的 C 条件编译（02 `_libc_init`）——Rust 侧无线程，A-4 登记为接线时裁决 |
| 测试基建 | rt-birth 冒烟内核 + test-rt-birth.sh（00/02/edge E1）；宿主测试三层形态（minix-rt 90% 纯函数/minix-types 布局 pin/minix-sys Canned 回放——todo §4 Gate D）；各篇 §5 清单（G4 刷新）；Gate E 对账方法（todo §1） |

---

## 4. 新目录

### 4.1 总判决与理由

**编号 00-13 与 99 全部保持不变；不新建任何篇章；15 篇按本蓝图契约做"保号重建"（两处结构性增补 + 一处勘误 + 测试表刷新 + 轻量点名）。** plan.md A-10/:276 两行回写随 B 相附带。

理由（三条，均带证据）：

1. **这是四个 rerank 目标中"文档-代码同步质量最高"的 stage**：V1 架构审查 11 条中 10 条闭环且修复记录带方案对比与验证命令（todo §3），00/99 双双落稿（2026-09-17），诞生链真机通电（rt-birth 五断言 PASS），三 crate 545 测试全绿（53+224+268）。15 篇的生命周期主线（plan §1.1 裁定）与 R 相提示词的判断标准逐条对得上：01→13 即运行序，无前向引用（plan §7.1 R-2/R-3 两项顺序修复的遗产）。
2. **断链成本与收益不成比例**：入站 60+ 处（18-stage-commands 一家 24 处、edge_todo 14 处），互引约 100 处（05 篇枢纽被引 13 次），01~13 的收敛记录在 `.review/codex/runtime/STATE.md`（plan §6.1）。重排无任何待解的结构矛盾——V1 轮已把顺序问题修完。
3. **缺口是"增补"形而非"结构"形**：G1（SYS_* wrapper 归 05）/G2（grant 表归 04）都是往**语义上已经正确的篇**里加一节——C 侧对应概念（`_kernel_call`、grant 授权机制）本来就属于那两篇的管辖边界，只是 Rust 落地晚于文档收敛。无需新篇；13 篇的审计文化与 00 篇的导航职责足以承载测试资产总表（K-265）。

### 4.2 新篇章总表（15 篇，编号不变）

| 编号 | 标题 | 一句话定位 | 分组 |
|------|------|-----------|------|
| 00 | runtime 整体概览 | runtime 是什么、生命周期主线（含 rt-birth 里程碑）、导航 | 总览 |
| 01 | 内核交付 | kerninfo/kuserinfo ABI、栈顶选择、栈镜像（生产家在 stack.rs） | 出生 |
| 02 | 程序入口 | 6 条汇编桩 → `___start` → main；rt_birth 六阶段 | 出生 |
| 03 | 运行时初始化 | 查询-校验-安装三步、可拥有状态、errno/TLS 模型 | 出生 |
| 04 | 通信原语 | 六原语+状态字+SENDA 队列+传输 trait；**增补 grant 表** | 机制 |
| 05 | 系统调用机制 | `_syscall`/`_kernel_call`/双路径/退避；**增补 SYS_* wrapper 家族** | 机制 |
| 06 | 内存分配器 | 堆两层、九档平板、页供应接口 | 资源 |
| 07 | 诊断输出 | 攒后发通道、单一格式化之家、恐慌 handler 唯一性 | 终局骨架 |
| 08 | PM 调用组 | 生命/信号/服务启停三族 | 服务族 |
| 09 | VFS 调用组 | 四形状 + LP64 打开落地 + 指针载荷变体 | 服务族 |
| 10 | VM 调用组 | 替办/哨兵消除/边界短路 + 客户端库余量清单 | 服务族 |
| 11 | 杂项调用组 | 睡觉/分派/直读/拼接 | 服务族 |
| 12 | RS 查询 | 查找/反查/路由 | 服务族 |
| 13 | 常量接口 | 对账与归属（errno 115/signal/调用号/端点/大族登记）+ 测试资产总表 | 契约 |
| 99 | 全局概念 | endpoint/56B 判例/调用号归一/全局状态记账 + **minix-sys 现状边界声明** | 收口 |

### 4.3 阅读路径与序差表

**主线**（编号即生命周期序）：`00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 99`。

**支线（可跳读）**：① 命令作者：`00 → 05 → 08/09 → 13`（errno 再导出在 13 §3.1）；② 服务器作者：`00 → 03 → 04 → 05 → 07 → 各服务组`；③ 只查常量：13 + 99 直达。

**序差表**：

| # | 生命周期事实（锚点） | 教学序位置 | 偏差理由 | 回指补偿 |
|---|---------------------|-----------|---------|---------|
| D1 | panic/exit 可发生在进程一生任意时刻 | 07（终局骨架）在服务族（08~12）之前 | 07 的依赖只有 03/05；服务族是 05 的套用——机制先于套用（plan R-2/R-4 修复的遗产） | 07 §6 过渡已声明"回到日常"的视角切换 |
| D2 | 13（常量）与 99（全局概念）无生命周期时点 | 殿后两篇 | 横切对账篇，前置声明"前置依赖：无" | 各篇已写明（13 无前置/99 无前置） |
| D3 | 堆增长（06）运行时要发 VM 消息（10 的语义） | 06 在 10 之前 | plan R-3 修复：06 只讲分配策略与用户态视图，消息封装移交 10 | 06 §1.2 边界声明 + 10 §3.3 消费面回指 |
| D4 | 04 的 `_ipc.S` 是 i386 汇编而本仓目标是 x86-64 | 原样讲 C 真值 | A-6 声明的 64 位演进：指令会换、分工模型不变；教学必须对着存在的源码 | 04 §2.2 已声明"64 位上指令和向量号都会变化" |

**并行体的组织**：08~12 五篇共用"清零-填字段-调协议-看正负"形状（08 讲透形状、09 讲四形状细化、10/11/12 各带差异），代表成员精讲 + 清单收录；13 是对账表；无强行线性化。

---

## 5. 每篇契约

> 格式：定位 / 讲什么（K 编号）/ 不讲什么（去向）/ 前置 / 后置 / 事实底线 / 验收标准。知识点清单以 K 编号指涉 §2.1。

### 00-runtime-overview

- **定位**：总览导航——runtime 三 crate 分工、生命周期主线（含 rt-birth 真机里程碑）、15 篇导航与设计原则。
- **讲什么**：K-001~003；B 相增补导航表备注（05/04 两篇的增补节）。
- **不讲什么**：一切机制（01~13）；服务器主循环（各 server stage）。
- **前置**：无。**后置**：全部。
- **事实底线**：`os/libs/*` 三 crate；`edge_todo.md:74`；plan §1.1。
- **验收**：三问可答（runtime 与 server 的区别/主线五段/如何选起点读）；导航表与实有篇一致。

### 01-kernel-handoff

- **定位**：内核交付——两样行李的 ABI、栈顶选择、栈镜像（生产家在 minix-sys/stack.rs）。
- **讲什么**：K-010~K-019。
- **不讲什么**：入口汇编（02）；构造器（03）；陷馅机制（04）。
- **前置**：00（+kernel 09 声明）。**后置**：02/03/08（execve 算栈）/11（直读）。
- **事实底线**：`type.h:205-247`；`kernel_utils.c`（62 行）；`stack_utils.c`（172 行）；`handoff.rs`；`stack.rs`；`minix-types/types/kerninfo.rs`（增补点名）。
- **验收**：KUSERINFO_HAS_FIELD 逐行讲解保留；三态栈顶选择有纯函数与测试锚；§3.4 的 stack.rs 单点真相表述保留；**勘误 E1**——§5 表补第 10 行（`test_magic_mismatch_maps_to_executable_format_errno`，Fix #5 所增）；**增补 K-019**——§3 或 §7 一句点名 kerninfo wire 权威落点。

### 02-crt0-start

- **定位**：程序入口——6 条汇编桩到 main；rt_birth 六命名阶段；start.rs 教训的文档化。
- **讲什么**：K-030~K-037。
- **不讲什么**：kerninfo 查询细节（01）；构造器（03）；分配器（06）。
- **前置**：01。**后置**：03；init 命令（panic-handler feature 消费面）。
- **事实底线**：`crt0.S:40-49`；`crt0-common.c:77-81,106-117,144-192`；`crtbegin.c:72-97`；`initfini.c:84-119`；`environ.c`；`crt0.rs`。
- **验收**：6 条指令逐条语义（不是"7 行"）；动态分支"静态永不可达"的论证保留；start.rs 删除史与五准则保留；rt_birth 六阶段与 rt-birth 真机验证表述保留。

### 03-runtime-init

- **定位**：运行时初始化——查询-校验-安装三步、失败清零继续的 C 契约、可拥有状态、errno/TLS 模型。
- **讲什么**：K-050~K-057。
- **不讲什么**：陷阱指令本体（04）；分配器（06）；恐慌（07）。
- **前置**：01/02。**后置**：04/06/07。
- **事实底线**：`init.c`（32 行全文）；`ipc.h:2786-2834`；`_errno.c:44-55`；`getprogname.c`/`setprogname.c`；`init.rs`。
- **验收**：三步顺序与双重条件一个不省；"没有致命失败只有可诊断状态"的契约推理保留（Fix #5 裁决）；KerninfoSource 双实现论证保留。

### 04-ipc-primitives

- **定位**：通信原语——六操作共享陷入约定、状态字编码、SENDA 队列、传输 trait；**增补：用户态 grant 表**。
- **讲什么**：K-070~K-076。
- **不讲什么**：消息协议（05）；各服务载荷（99/08~12）；内核侧 verify_grant（kernel stage）。
- **前置**：03。**后置**：05（协议建立其上）；07-ds/11-devman（grant 消费方声明）。
- **事实底线**：`ipcconst.h` 两份；`_ipc.S:16-87`；`ipc_minix_kerninfo.S`；`ipc.h:2745-2762`；`asynsend.c`；`ipc.rs`；`arch_trap.rs`（增补点名）；**`grant.rs` + `safecopies.c`（增补节事实底线）**。
- **验收**：状态字两段编码与"内核可信消息不可回复"保留；SENDA 先写标志最后的生产消费论证保留；**增补 K-075（G2）**——"用户态 grant 表"一节：C cpf 机制（表在授权者地址空间、direct/revoke 不陷阱、sys_setgrant 注册表地址）→ grant.rs（槽布局权威 minix-types::types::grant、E-DSWIRE 公共传输半定位）；**增补 K-076（G6）**——§3.5 点名 arch_trap.rs 与 real-trap 门控、宿主 -EIO 诚实语义、通电挂 E1。

### 05-syscall-mechanism

- **定位**：系统调用机制——`_syscall` 协议、双路径打包、`_kernel_call` 退避；**增补：SYS_* 内核调用 wrapper 家族**。
- **讲什么**：K-090~K-097。
- **不讲什么**：六原语本体（04）；各服务消息体（08~12）；errno 值表（13）。
- **前置**：04。**后置**：06~12（全部封装的地基）；23（无此篇——测试表自持）。
- **事实底线**：`syscall.c:9-25`；`loadname.c:7-19`；`kernel_call.c:7-21`；`tickdelay.c`；`ipc.h:14`；`syscall.rs`（协议半 + wrapper 半）。
- **验收**：两层失败模型与线性退避论证保留；无界循环不改语义的否决案保留；**增补 K-094（G1）**——"内核调用 wrapper 家族"一节：C `sys_*.c` 全族（A-10 历史排除项）经 E2/E6/E9 批落地为 syscall.rs 上半的 20+ `sys_*` wrapper（消费方 VM gateway/init/server），wrapper 遵循的"三条款"约定（m_type 由传输写/载荷按 union 臂填/负 errno 不吞——edge E6 记录）；**增补 K-095**——§5 表分两段（协议半 12 + wrapper 半按实有重列），表头加基线日期。

### 06-allocator

- **定位**：内存分配器——堆两层、九档平板、页供应接口。
- **讲什么**：K-110~K-116。
- **不讲什么**：VM 消息封装（10）；恐慌（07）。
- **前置**：03（+10 接口约定声明）。**后置**：10（消费）/08（execve 腾地）。
- **事实底线**：`brksize.S`；`brk.c`/`sbrk.c`；`malloc.c:162,193,317,377-388,448,559`；`alloc.rs`、`lib.rs`。
- **验收**：九档推导与 8 字节对齐论证保留；页对齐测试池教训（Fix #13）保留；malloc.c 归因修正（V1-P2-3 #8）后的表述为准。

### 07-panic-output

- **定位**：诊断输出——攒后发通道、单一格式化之家、恐慌 handler 唯一性与注册表归属。
- **讲什么**：K-130~K-137。
- **不讲什么**：exit 消息语义（08）；printf 全量（A-2 排除）。
- **前置**：03/05。**后置**：08（exit）；init/服务器（panic-handler feature 消费面）。
- **事实底线**：`kputc.c`；`sys_diagctl.c`；`panic.c:34-66`；`assert.c`；`itoa.c`；`stderr.c`；`diag.rs`；`minix-types/types/diagnostic.rs:35`。
- **验收**：恐慌梯子八级（C 侧）与 handler 三步（Rust 侧）分层清楚；静默截断契约保留；注册表归属论证（V1-P0-1）保留；两处已登记分歧不消失。

### 08-pm-syscalls

- **定位**：PM 调用组——生命/信号/服务启停三族。
- **讲什么**：K-150~K-157。
- **不讲什么**：消息布局归档（99）；常量值（13）；信号运行时语义面（登记待验证）。
- **前置**：05（+01 衔接声明）。**后置**：18-stage-commands（命令消费面）。
- **事实底线**：`fork.c`/`_exit.c`/`execve.c`/`wait4.c`/`kill.c`/`raise.c`/`srv_fork.c`/`srv_kill.c`/`taskcall.c`；`callnr.h:9`；`pm.rs`。
- **验收**：三族心智模型保留；execve 会师点保留；**§5 表按实有 29 个测试重列（G4）**；信号族 wire 半归属（E7/04-stage-pm 批次表）声明保留。

### 09-vfs-syscalls

- **定位**：VFS 调用组——四形状、LP64 打开落地、指针载荷变体。
- **讲什么**：K-170~K-178。
- **不讲什么**：套接字族（17-stage-net）；布局归档（99）。
- **前置**：05/08。**后置**：15-stage-fs；nanosleep（11 消费 select）。
- **事实底线**：`read.c`/`write.c`/`open.c`/`lseek.c`/`vectorio.c`/`stat.c`/`ioctl.c`/`fcntl.c`/`getdents.c`/`dup.c`；`ipc.h:726-734,795-803,874-880`；`sys/stat.h:59-97`；`vfs.rs`；`minix-types/types/stat.rs`。
- **验收**：两处反直觉布局顺序的测试锁死表述保留；ENAMETOOLONG 加固偏差（99 裁决）保留；**§5 表按实有 29 个重列（G4）**。

### 10-vm-syscalls

- **定位**：VM 调用组——替办/哨兵消除/边界短路 + 客户端库余量清单。
- **讲什么**：K-190~K-198。
- **不讲什么**：VM 服务端（02-stage-vm）；特权/热更新（集成 stage）。
- **前置**：05/06。**后置**：02-stage-vm；RS/命令消费面。
- **事实底线**：`mmap.c`；`vm_*.c` 九文件（行锚见 K-193）；`mman.h:124,130`；`vm.rs`；`minix-types ipc/vm.rs`。
- **验收**：§2.5 余量清单七行表与三层现状保留（vm_info 暂缓论证不删）；双层签名弯路（Fix #6）如实记载保留；**§5 表按实有 30 个重列（G4，Fix #8/#9 的 11 个测试补入）**。

### 11-misc-syscalls

- **定位**：杂项调用组——睡觉/分派/直读/拼接。
- **讲什么**：K-210~K-216。
- **不讲什么**：select 通用形状（09 已承载空集形）；MIB 服务端（10-stage-mib）。
- **前置**：05/01（直读数据源）。**后置**：10-stage-mib（sysctl 客户端对端）。
- **事实底线**：`nanosleep.c`；`svrctl.c`；`getticks.c`/`getuptime.c`/`clock_time.c`；`read_tsc_64.c`；`misc.rs`。
- **验收**：四解法总纲保留；频率零加固标注保留；余量不依赖 select 回写的修正（Fix #10）保留；§5 表 16 个与实测一致（已刷新 ✓ 维持）。

### 12-rs-query

- **定位**：RS 查询——查找/反查/路由。
- **讲什么**：K-230~K-235。
- **不讲什么**：RS 服务端（03-stage-rs）；注册协议。
- **前置**：05。**后置**：03-stage-rs。
- **事实底线**：`minix_rs.c:23-40`；`getepinfo.c`/`getprocnr.c`/`getsysinfo.c:8-32`；`rs.rs`。
- **验收**：不缓存论证与调用计数测试锚保留；元组化论证保留；**§5 表按实有 10 个重列（G4）**。

### 13-constants-abi

- **定位**：常量对账与权威归属 + 测试资产总表（K-265 增补落点）。
- **讲什么**：K-250~K-255、K-265。
- **不讲什么**：常量用法（各文档）。
- **前置**：无。**后置**：全部（对账基线）。
- **事实底线**：`sys/sys/errno.h`、`signal.h`、`termios.h`；`callnr.h`、`com.h:59-66`；`types/errno.rs`（:551 对账测试）、`types/signal.rs`。
- **验收**：115=115 与脚本 diff 方法保留；登记制三理由保留；**增补 K-265**——全 crate 测试资产总表（rt 53/sys 域内 184/types 268 + 逐模块计数 + Gate E 对账方法一段）。

### 99-global-concepts

- **定位**：全局概念——endpoint/56B 判例/调用号归一/全局状态记账 + minix-sys 现状边界声明。
- **讲什么**：K-260~K-264。
- **不讲什么**：一切机制（00~13）。
- **前置**：无（导航声明）。**后置**：无。
- **事实底线**：`type.h`/`ipc.h`/`com.h:205`/`endpoint.h`；`types/endpoint.rs`/`kerninfo.rs`；plan:165,:276（回写对账）。
- **验收**：LP64 判例与调用号归一裁决保留；**增补 K-264（G3）**——§1 后加"minix-sys 现状边界"小节：三类共享基建归属表（SYS_* wrapper→05、grant 表→04、stack 布局→01）+ A-10 修订声明（server 专用高层封装仍归各 server stage）。

---

## 6. 变更表

| 操作编号 | 操作类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向/来源 |
|---------|---------|--------|--------|------|-----------|----------|
| C1 | 重建·保号 | 00 | 00 | 导航对齐（05/04 增补节备注） | K-001~003 | 存量保留 |
| C2 | 重建·保号 | 01 | 01 | **E1 表补第 10 行** + K-019 点名 kerninfo 权威 | K-010~019 | 存量保留 + 勘误增补 |
| C3 | 重建·保号 | 02 | 02 | 无事实改动（V1 后已多轮同步） | K-030~037 | 存量保留 |
| C4 | 重建·保号 | 03 | 03 | 无事实改动 | K-050~057 | 存量保留 |
| C5 | 重建·保号 | 04 | 04 | **G2 增补 grant 表一节（K-075）+ G6 点名 arch_trap（K-076）** | K-070~076 | 存量保留 + 增补 |
| C6 | 重建·保号 | 05 | 05 | **G1 增补 SYS_* wrapper 家族一节（K-094/095）** | K-090~097 | 存量保留 + 增补 |
| C7 | 重建·保号 | 06 | 06 | 无事实改动 | K-110~116 | 存量保留 |
| C8 | 重建·保号 | 07 | 07 | 无事实改动 | K-130~137 | 存量保留 |
| C9 | 重建·保号 | 08 | 08 | §5 表刷新（29） | K-150~157 | 存量保留 |
| C10 | 重建·保号 | 09 | 09 | §5 表刷新（29，E7 增量补入） | K-170~178 | 存量保留 |
| C11 | 重建·保号 | 10 | 10 | §5 表刷新（30，Fix #8/#9 的 11 个补入） | K-190~198 | 存量保留 |
| C12 | 重建·保号 | 11 | 11 | 无事实改动（已刷新 ✓） | K-210~216 | 存量保留 |
| C13 | 重建·保号 | 12 | 12 | §5 表刷新（10） | K-230~235 | 存量保留 |
| C14 | 重建·保号 | 13 | 13 | **增补测试资产总表（K-265）** | K-250~255、265 | 存量保留 + 增补 |
| C15 | 重建·保号 | 99 | 99 | **G3 增补 minix-sys 现状边界声明（K-264）** | K-260~264 | 存量保留 + 增补 |
| C16 | 回写（B 相附带） | plan.md:165（A-10）+ :276（§5.1 行） | plan.md | G3：排除表述修订为"共享传输基建已入 minix-sys（SYS_* wrapper/grant/stack），分篇承载" | — | doc-plan 同步 |

**没有的操作**：重排 0、拆分 0、合并 0、归档 0、新建 0（§4.1 理由 1-3）。

---

## 7. 缺漏新篇（非 C 主题清单逐项落实）

| 主题 | 落实 | 验收标准 |
|------|------|---------|
| 链接与加载 | 已有归属：02（crt0/链接期符号/弱别名）+ 06（`_brksize`）；load_vm_elf 归 kernel 轨道（edge E1 注记） | 02 契约已含 |
| 镜像与内存布局 | 已有归属：01（栈镜像+kerninfo）+ 99（56B 判例）+ stack.rs（01 §3.4 单点真相） | K-013/014/261 |
| 汇编入口与陷阱进入 | 已有归属：02（crt0.S）+ 04（`_ipc.S`/`ipc_minix_kerninfo.S`/arch_trap 增补点名） | K-031/070/076 |
| 启动装配 | 已有归属：00/01/02/03 生命周期主线；SEF 归 minix-sef（E-ISWIRE） | K-002 |
| 构建与工具链 | 一句话制：libminc 基线（00/plan）、feature 门三处（02/04/07）、MISSING_SYSCALLS WONTFIX（plan §5.3） | plan 已核对 |
| 跨模块接口与线格式 | 已有归属：99（判例与权威）+ 08~12（载荷）+ 04（grant 槽布局增补） | K-261/075 |
| 错误路径 | 已有归属：各篇 §4 表式分治 + 13（值）+ 07（恐慌） | R1 维持 |
| 关闭与退出 | 已有归属：08（exit 三级兜底）+ 07（panic 阶梯） | K-132/150 |
| 并发与同步 | 已有归属：03（TLS 两形态 + errno 模型）；A-4 登记接线时裁决 | K-054 |
| 测试基建 | 已有归属：rt-birth（00/02/edge E1）+ 各篇 §5（G4 刷新）+ **13 增补总表（K-265）**——不新建篇章 | §5 契约 13 验收 |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表

编号与文件名全部不变；B 相变化集中在增补节与表格刷新：

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 断链风险 |
|--------|--------|--------|---------|---------|
| 01 §5 表 | 9 行测试表 | 同位（10 行，补 :444 行） | 勘误 E1 | 低 |
| 01 §3/§7 | （无 kerninfo 权威句） | 同位一句 | 增补 K-019 | 低 |
| 04（新节） | （无 grant 内容） | 04 §2 后插"用户态 grant 表"一节 + §3.5 点名 arch_trap | 增补 G2/G6 | 低（纯新增；07-ds/11-devman 的 grant 消费声明同步一句） |
| 05（新节） | （无 SYS_* 内容） | 05 §2 后插"内核调用 wrapper 家族"一节 + §5 表分段 | 增补 G1 | 中：05 篇是互引枢纽（被引 13 次），增补节须放在"机制"语义内不扩边界——契约已钉 |
| 08/09/10/12 §5 | 旧快照表 | 同位按实有重列 + 表头基线日期 | 刷新 G4 | 低 |
| 13（新节） | （无测试总表） | 13 末尾加"测试资产总表"一节 | 增补 K-265 | 低 |
| 99（新节） | （无边界声明） | 99 §1 后加"minix-sys 现状边界"小节 | 增补 G3 | 低 |
| plan.md:165,:276 | A-10 排除表述 | 同位修订 | 回写 G3 | 中：A-10 被其它 stage plan 引用为边界先例（05/17/18 plan 均有"归 runtime/minix-sys"类表述），回写时 grep 引用面一并核对 |

### 8.2 引用迁移表

**入站引用**（编号不变 ⇒ 文件名级零断链）：

| 引用方 | 处数 | 引用对象 | 核对要点 |
|--------|------|---------|---------|
| `edge_todo.md` | 14 | E1 诞生链/E-MINTYPES-RUNTIME/E-MINSYS-SCOPE 等 | 事实性引用；B 相不回改 edge |
| `18-stage-commands/`（plan 12 + todo 9 + 05 篇 3 + 01 篇 2 等） | 26 | **最大消费方**：errno 再导出（13 §3.1）、write 通道（plan §5.3）、exit 语义（08） | 13/08 的语义面保持 |
| `15-stage-fs/plan.md` | 5 | fslib 排除、minix-vtreefs 边界 | 维持 |
| `16-stage-drivers/plan.md`、`17-stage-net/plan.md` | 4/3 | driver-rt/net 边界 | 维持 |
| 其它 AI 的 doc_rerank_*（07/03/02/05/09/01/08/10/11 六 stage 约 20 处） | ~20 | 目录级 | bagging 约束未读；目录引用安全 |
| `06-stage-sched/doc_rerank_glm.md`（本执行者前轮产物） | 4 | 目录级 | 自家产物 |

**出站引用**：`../01-stage-kernel/09-vm-boot-protocol.md`（01 篇）实测存在，零断链。

**代码注释引用**（3 文件 4 处）：`os/libs/minix-sys/src/lib.rs`（2）、`os/qemu-tests/.../test-rt-birth/src/main.rs`（1）、`os/qemu-tests/.../user/rt-birth/src/main.rs`（1）——编号不变零迁移。

### 8.3 断链成本摘要

- **文件名级断链：0 处**（保号方案直接收益）。
- **节级引用**：互引约 100 处，全部是"同编号文档 §N/章节名"形态；需人工核对约 10 处（E1 表行、G1/G2 增补节的邻近引用、§5 刷新处的计数引用）。批量方式：不适用 sed；B 相每篇完成时 `rg "NN-[a-z-]+\.md" notes/ os/` 抽验。
- **热点文件**：`05`（被引 13 次）、`01`（9 次）、`10`（10 次）——B 相排期上 05 宜先做（G1 增补落点 + 枢纽地位）。
- **外部成本**：60+ 处入站引用零回改；约束是 05（协议）/13（errno 再导出）/08（exit 语义）的语义面不得在重建中变调——契约"事实底线"已钉死。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**：逐篇契约前置——00∅ → 01{00(+kernel 声明)} → 02{01} → 03{01,02} → 04{03} → 05{04} → 06{03(+10 声明)} → 07{03,05} → 08{05(+01 衔接)} → 09{05,08} → 10{05,06} → 11{05,01} → 12{05} → 13∅（审计篇） → 99∅（导航声明）。**全部指向更早编号或声明为横切，无前向**。（口径同前两轮：边界指针允许指后篇；06→10 的消息封装依赖已由边界声明+消费面回指解决——plan R-3 遗产。）
2. **依赖关系图无环**：上述前置边构成 DAG（主线链 00→01→…→12 无回边；13/99 为无前置横切篇）。**通过。**
3. **覆盖率检查**：知识点池约 128 条逐一对照 §5 契约——00:K-001~003；01:K-010~019；02:K-030~037；03:K-050~057；04:K-070~076；05:K-090~097；06:K-110~116；07:K-130~137；08:K-150~157；09:K-170~178；10:K-190~198；11:K-210~216；12:K-230~235；13:K-250~255、265；99:K-260~264。**全部有去向，删除项 0**；新增约 11 条全部带锚点（K-075/K-094 的"零命中"证据本轮 grep 实测）。**通过。**
4. **断链成本统计**：见 §8.3——文件名级 0、节级约 10 处、外部 60+ 处零回改。**已算清。**

### 9.2 自检门逐门结果

| 门 | 检查与结果 |
|----|-----------|
| G1 | C 真序逐条可核对：随机抽十条——S1 魔数（type.h:229 ✓ 本轮 grep 实测）、S3 六条指令（crt0.S:44-49 ✓ sed 实测）、S4 列表定义（:106-117 ✓ 读源）、S5 三步（init.c 32 行全文 ✓）、S7 分界 40（loadname.c:19 行文件 + ipc.h:14 ✓）、S8 方向检查（sbrk.c:20-21 ✓ 读源）、S9 梯子（panic.c:34-66 ✓ 读源）、PM 调用号（callnr.h:9 起 ✓ plan/todo 双证）、VFS 载荷顺序（ipc.h:795-803 ✓ 09 篇引文+读源计划）、grant 自管语义（grant.rs 模块文档 + safecopies.c 381 行 ✓ 本轮实测）。**通过** |
| G2 | 池完整性：C 侧——csu 3 文件→02；libc/sys 133 .c 按 plan §5.1 映射（本轮抽核 24 文件行数全对）；libsys 共享子集 23 文件→各篇（**safecopies.c 本轮补入 04**）；头文件 14 份→各篇；非 C 制品——rt-birth 冒烟内核与脚本→00/02/测试基建、feature 门三处→02/04/07、链接期符号→02/06；minix-sef/sys_*.c 高层封装等排除项沿 plan §5.3 维持。**通过** |
| G3 | 前向引用为零：见 §9.1 检查 1。**通过** |
| G4 | 依赖图无环：见 §9.1 检查 2。**通过** |
| G5 | 覆盖率 100%：见 §9.1 检查 3，删除项 0、新增锚点齐全。**通过** |
| G6 | 增补来源抽查十处：C5 的 G2 来源=grant.rs 模块文档+safecopies.c 行数 ✓ 实测；C6 的 G1 来源=syscall.rs `pub fn sys_` 20+ ✓ 实测；C2 的 E1 来源=handoff.rs:444 实测+01 §5 原文 ✓；C15 的 G3 来源=plan:165,:276 原文+三文件实有 ✓；C14 的 K-265 来源=逐模块计数 §0.3 ✓ 实测；C11 的 G4 来源=vm.rs 30 vs 表 14 ✓ 实测；C9/C10/C13 的 G4 同法 ✓；K-019 来源=99 §1.4+types/kerninfo 存在（99 篇引文+todo E-KERNINFO）✓；K-076 来源=arch_trap.rs:61-107（todo Gate D 引文）✓；K-095 来源=逐模块计数 ✓。10/10 有来源且全部实测。**通过** |
| G7 | 契约七要素：15 篇契约逐篇含定位/讲什么/不讲什么/前置/后置/事实底线/验收标准（知识点清单以 K 编号指涉 §2.1）。**通过** |
| G8 | 迁移表覆盖：§8.1 八行覆盖全部变化点；§8.2 覆盖文档引用（入站分类）与代码注释（3 文件 4 处逐条）。**通过** |
| G9 | 事实断言锚点抽查十条：KERNINFO_MAGIC=0xfc3b84bf（type.h:229 ✓ 实测）、crt0.S 49 行 6 指令（wc+sed ✓）、init.c 32 行（wc ✓）、syscall.c 25 行（wc ✓）、safecopies.c 381 行（wc ✓）、grant.rs 292 行（wc ✓）、rt 53/sys 224/types 268（cargo test 实测 ✓）、handoff 10 测试（grep ✓）、syscall.rs 49 测试（grep ✓）、rt-birth 五断言（edge_todo:74 ✓ 实测引文）。推测项：G1 的"wrapper 属 05 最自然"是归属判断（基于 C `_kernel_call` 已属 05 的语义论证，非事实断言），已按裁决项呈报。**通过** |

### 9.3 结论与待用户裁决的问题

**结论**：蓝图完成。新目录 = 保号 15 篇、零新建；知识点池约 128 条全覆盖；结构性缺口 3 项（G1 SYS_* wrapper 归 05、G2 grant 表归 04、G3 A-10 边界回写+99 声明）+ 勘误 1 项（E1 测试表缺行）+ 快照刷新 4 处 + 轻量点名 3 处；断链成本 = 文件名级零、节级约 10 处、外部 60+ 处零回改。四项机械检查与 G1-G9 全部通过。

**待裁决**：

1. **不新建篇章的判定**（本蓝图判定不新建：生命周期主线完整、13 篇审计文化可承载测试总表、G1/G2 是"往语义正确的篇里加一节"；若共识蓝图倾向新建"14-runtime-infra"承载 syscall.rs wrapper 半与 grant，K-075/076/094/095/097 五条可平移，编号不影响既有篇）。
2. **G1 的归属方向**（05 篇"内核调用 wrapper 家族"节 vs 独立篇）：本蓝图判定归 05——C 侧 `_kernel_call` 已是 05 的管辖概念，wrapper 是其执行家族；若裁定独立篇，K-094/095 迁出即可，05 的其余契约不变。
3. **G3 的 plan 回写时机**（随 B 相首篇顺带 vs 单独 doc-plan 同步批）：本蓝图判定随 B 相顺带（同 MIB 蓝图裁决 3）；回写时 grep 其它 stage plan 对 A-10 的引用面一并核对。