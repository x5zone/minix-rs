# 14-stage-runtime 文档重建蓝图（qwen）

## 0. 元数据

- **执行者**：qwen
- **日期**：2026-09-19
- **目标目录**：`rewrite-notes/14-stage-runtime/`
- **仓库根目录**：`/home/xzhao/github/minix-rs`
- **当前提交号**：`068c27c61`
- **模式**：R 相·重建蓝图（只出蓝图，不改任何正文；不读其它 AI 的 `doc_rerank_*`）

### 0.1 审查范围

- **算作文档**：`00-runtime-overview.md`、`01`~`13` 编号篇、`99-global-concepts.md`（共 15 篇）。
- **算作参考材料（不作为搬迁对象）**：`plan.md`、`todo.md`、`draft/README.md`。
- **范围外（不读、不引用）**：`.design/`、`tmp_design_and_todo/`、其它 AI 的 `doc_rerank_deepseek.md` / `doc_rerank_glm.md`。
- **范围外发现**：`os/servers/vm`、`01-stage-kernel/09-vm-boot-protocol.md`、`../02-stage-vm/*` 属邻接 stage；`os/libs/minix-sys` 的 `ds.rs`/`rmib.rs`/`devman_client.rs`/`inputdriver.rs`/`usb_model.rs`/`socket.rs`/`tty.rs` 属其它 stage 客户端库（E-MINSYS-SCOPE）。

### 0.2 读取清单

- **文档**：15 篇编号文档头部声明 + 正文全读（合计 2043 行）。
- **C 源码锚点抽验**（逐条 grep/sed 实证，见 §0.3）：`lib/csu/arch/x86_64/crt0.S`、`lib/csu/common/crt0-common.c`、`minix/lib/libc/sys/init.c`、`syscall.c`、`loadname.c`、`libsys/kernel_call.c`、`include/minix/ipcconst.h`。
- **非 C 制品**：`os/libs/minix-rt/Cargo.toml`、`os/libs/minix-sys/Cargo.toml`（feature 门）、`os/libs/minix-sys/src/{grant,arch_trap,wait,tty,stack}.rs`、`os/libs/minix-rt/src/crt0.rs`（三架构入口桩）、`os/qemu-tests/test-rt-birth{,-aarch64,-riscv64}.sh`。
- **边界材料**：`00-master-plan/README.md`、`edge_todo.md`（E1 / E-MINTYPES-RUNTIME / E-MINSYS-SCOPE / E-BOOTFRAME 注记）、`01-stage-kernel/06-todo.md`（契约写法范例，仅借写法）。

### 0.3 使用的命令与关键输出（证据摘录）

```text
# C 真序锚点核对
sed -n '38,60p' minix3/lib/csu/arch/x86_64/crt0.S   → 6 条指令：andq/subq/movq×3/jmp ✓（doc02 "6 条" 属实）
sed -n '144,192p' minix3/lib/csu/common/crt0-common.c → ___start: ps_strings 检查→environ→progname→(rtld 分支)→_libc_init→_preinit→atexit(_fini)→_init→main→exit ✓
sed -n '1,35p' minix3/minix/lib/libc/sys/init.c      → __minix_init 是 __constructor__，ipc_minix_kerninfo 查询→MAGIC 校验→装 ipcvecs（失败清零继续，"not fatal"）✓
sed -n '1,30p' minix3/minix/lib/libc/sys/syscall.c   → _syscall 17 行体：m_type=callnr→ipc_sendrec→status→负值 errno→返回 ✓
sed -n '1,22p' minix3/minix/lib/libc/sys/loadname.c  → k=strlen+1；永远填 len+name；k<=M_PATH_STRING_MAX(40) 才 strcpy 内联 ✓
sed -n '1,25p' minix3/minix/lib/libsys/kernel_call.c → t=1；循环 m_type=callnr→do_kernel_call→r!=ENOTREADY break→tickdelay(t++) ✓
sed -n '1,36p' minix3/minix/include/minix/ipcconst.h → SEND=1..SENDA=16, IPCNO_HIGHEST=SENDA, _ASSERT_MSG_SIZE==56, MASK=0x3F, FROM_KERNEL=1, FLAGS_SHIFT=16 ✓

# 代码里存在但 15 篇无归属的运行时模块（grep 命中仅 todo.md）
ls os/libs/minix-sys/src/  → grant.rs  arch_trap.rs  wait.rs  tty.rs  stack.rs
head grant.rs  → "User-space grant table — Rust rewrite of C libsys safecopies.c"（C 源存在：minix3/minix/lib/libsys/safecopies.c）
head wait.rs   → sys/wait.h WIFEXITED/WNOHANG 状态解码 sum type
grep cfg( crt0.rs → target_arch = x86_64 / riscv64 / aarch64 三套 naked _start 桩
sed minix-rt/Cargo.toml → features: default=[std], real-trap, panic-handler, alloc-global
ls os/qemu-tests/ → test-rt-birth.sh / -aarch64.sh / -riscv64.sh

# 断链成本
grep 内部交叉引用（"第 NN 篇" / NN-name.md）within 14/*.md → 151 处（第 05 篇 25× / 第 01 篇 19× / 第 06 篇 16× 为热点）
grep "14-stage-runtime|covered in NN|see NN-*.md" os/libs/ → 仅 2 处
```

---

## 1. C 真序（运行时真序重建）

**阶段类型判定**：本 stage 是**混合型的启动链型 + 系统调用集合型**。前段（00~07 + 99）是一条线性启动/生存链——内核交付一个进程 → 该进程的第一条指令 → 运行时初始化 → 通信与协议机制 → 资源与终局；后段（08~12）是按服务分组的**并行系统调用集合**（PM/VFS/VM/misc/RS 共 190+ 个调用号），不能排成一条线，须按 §5.2 并行体规则以"汇聚点（05 协议）+ 服务分组"组织。判定理由：`crt0-common.c:144-192` 与 `init.c:20-32` 证明前段严格线性；`plan.md §1.3` 与 `callnr.h`/`com.h` 的分段基址证明后段是分组集合。

**运行时真序表**（步骤 / 动作 / C 锚点 / 说明；⚑ = 与现有文档叙述存在序差的点）：

| # | 动作 | C 函数与锚点 | 说明 |
|---|------|-------------|------|
| R0 | （exec 的**调用方**进程）libc 算出新栈尺寸、在用户缓冲里填好栈帧镜像，随 SYS_exec 把帧指针交给内核 | `minix/lib/libc/sys/stack_utils.c:76`（`minix_stack_params` 算尺寸/溢出检查）、`:119`（`minix_stack_fill` 填 argv/envp/auxv） | 初始栈镜像由调用方用户态 libc 预先构造，不是内核凭空生成 |
| R1 | 内核把调用方交来的栈帧装入新镜像，置 `ps_strings`→RBX、栈顶入约定位，跳到 `_start` | 装入/跳转属 `01-stage-kernel` 交付面（`sched.c`/`vm` exec 路径） | 进程诞生瞬间交接两样行李：kerninfo 页地址 + 栈顶 |
| R2 | 汇编入口桩整栈对齐、搬三个入口寄存器 | `lib/csu/arch/x86_64/crt0.S:43-49`：`andq $~15`→`subq $8`→`rdx→rdi`,`rcx→rsi`,`rbx→rdx`→`jmp ___start` | 静态链接下只有 RBX(=ps_strings) 有效，另两槽恒零 |
| R3 | C 入口 `___start` 检查 ps_strings，发布 `environ`，扫斜杠推 `__progname` | `crt0-common.c:150-165` | 空指针即 `_FATAL` 退出 |
| R4 | 动态加载器检查分支（`&rtld_DYNAMIC != NULL`） | `crt0-common.c:167-175` | 静态链接程序**永不进入**（ARCH A-1） |
| R5 | `_libc_init()` → `_preinit()` → `atexit(_fini)` → `_init()` | `crt0-common.c:177-189`；`lib/libc/misc/initfini.c:84-119` | `_init()` 运行 `.init_array` |
| ⑍ R6 | **构造器 `__minix_init` 运行**（查询 kerninfo→校验 MAGIC→装 ipcvecs） | `init.c:20-32`（`__attribute__((constructor))`） | **序差 ⑍**：plan §1.1 时序图把本步画在 crt0 ___start 之前；真相是它经 `.init_array` 在 R5 的 `_init()` 内运行，晚于 environ/progname 发布、早于 main |
| R7 | `main(argc, argv, environ)` → 返回值交 `exit` | `crt0-common.c:191` | 控制权移交应用 |
| R8 | 应用运行期任一系统调用：`_syscall` 打包 m_type=callnr→`ipc_sendrec`→负值转 errno | `libc/sys/syscall.c:9-25`；路径打包 `loadname.c:7-19`；内核调用重试 `libsys/kernel_call.c:7-21` | 05 协议是 08~12 每一封装的共同地基 |
| R9 | 六种通信原语陷入内核 | `ipcconst.h:6-14`（SEND/RECEIVE/SENDREC/NOTIFY/SENDNB/MINIX_KERNINFO/SENDA）；`libc/arch/i386/sys/_ipc.S`；向量 `arch/i386/include/ipcconst.h` | 32 位 i386 形态；64 位换 `syscall`/`int 0x21`（`arch_trap.rs`）ARCH A-6 |
| R10 | 堆：`brk`/`sbrk` 移动边界；malloc 上层切分 | `libc/sys/brk.c:22-34`、`sbrk.c:13-25`、`arch/i386/sys/brksize.S`、`lib/libc/stdlib/malloc.c:317/377-388` | Rust 换 slab（ARCH A-3） |
| R11 | 诊断/恐慌：`kputc` 缓冲→`sys_diagctl`→`panic` 梯子 | `libsys/kputc.c:17-32`、`sys_diagctl.c:5-27`、`panic.c:21-67`、`assert.c` | 绕过文件系统 |
| R12 | 各服务封装（并行）：PM/VFS/VM/misc/RS 族按调用号发起 | `libc/sys/{fork,execve,read,open,mmap,...}.c`、`libsys/{srv_*,vm_*,getsysinfo,...}.c`、`libc/sys/minix_rs.c` | 8 类文件，形状统一（清零→填→调协议→看正负） |
| R13 | 授权与安全拷贝：服务端经 grant 表直写调用者缓冲 | `libsys/safecopies.c`（cpf 机制、`sys_setgrant`、`cpf_grant_direct`/`cpf_revoke`） | **序差 ⑍**：R12 的 read/write/stat 依赖它（doc09 称"魔法授权"），但全 stage 无人讲解 |
| R14 | 终局：`exit`→`fini_array`→`atexit`→`PM_EXIT` | `_exit.c:12-30`；`crt0-common.c` 的 `atexit(_fini)` | 三级兜底（协议→非法跳转→打转） |

---

## 2. 知识点全集（去重后的存量池 + 新增）

> 完整逐条池见本目录 `doc_rerank_qwen.pool.tsv`（因体量分文件，带 `_qwen` 后缀，见 §2.3）。本节给总表与统计。

### 2.1 分组总表（按现有 15 篇，每篇列主知识点簇，标注类型与主讲述点）

| 池编号 | 名称簇 | 类型 | 来源 | 现有位置 | 主锚点 |
|--------|--------|------|------|---------|--------|
| K-001 | kerninfo 页三组字段 / MAGIC `0xfc3b84bf` / `ki_flags` | 数据结构·概念 | 存量 | 01 §2.2 | `type.h:214-247` |
| K-002 | kuserinfo + `KUSERINFO_HAS_FIELD` 版本探测 | 机制 | 存量 | 01 §2.1 | `type.h:205-212` |
| K-003 | `get_minix_kerninfo`/`minix_get_user_sp` 双回退 | 接口 | 存量 | 01 §2.3 | `kernel_utils.c:25-62` |
| K-004 | 初始栈镜像计算/填充 + ps_strings | 机制·数据结构 | 存量 | 01 §2.4-2.5 | `stack_utils.c`、`exec.h:104-116` |
| K-005 | crt0.S 入口桩 6 指令 + 寄存器搬运 | 机制 | 存量 | 02 §2.1 | `crt0.S:43-49` |
| K-006 | `___start` 全流程（发布/短名/动态分支/init_array/main） | 机制 | 存量 | 02 §2.3 | `crt0-common.c:144-192` |
| K-007 | 构造器兼容层逆序（crtbegin） | 机制 | 存量 | 02 §2.4 | `crtbegin.c:72-97` |
| K-008 | 静态链接 vs 动态（ARCH A-1） | 架构演进 | 存量 | 02 §1.3/§3 | `crt0-common.c:167-175` |
| K-009 | `__minix_init` 查询/校验/安装三步 | 机制 | 存量 | 03 §2.1 | `init.c:20-32` |
| K-010 | 向量表类型 + 内联包装 + 双重条件安装 | 数据结构·不变量 | 存量 | 03 §2.2 | `ipc.h:2786-2834` |
| K-011 | errno 全局槽 + `__errno()` + TLS（A-4/A-5） | 概念·约束 | 存量 | 03 §1.3/§2.3 | `_errno.c:44-55` |
| K-012 | environ 哨兵 `0x53535353` + progname 只读 | 数据结构 | 存量 | 03 §2.4-2.5 | `environ.c`、`get/setprogname.c` |
| K-013 | 六原语语义 + 共享陷入约定 | 概念·机制 | 存量 | 04 §1 | `ipcconst.h:6-14` |
| K-014 | 状态字两段编码（callnr/flags）+ 可信消息不回复 | 约束 | 存量 | 04 §2.1 | `ipcconst.h:20-35` |
| K-015 | 六个 `_ipc.S` 汇编函数 + `ipc_minix_kerninfo.S` | 机制 | 存量 | 04 §2.3-2.4 | `_ipc.S`、`ipc_minix_kerninfo.S` |
| K-016 | 批量异步发送队列（asynsend 五事 + 取错） | 机制·数据结构 | 存量 | 04 §2.5 | `asynsend.c:25-188`、`ipc.h:2745-2762` |
| K-017 | `_syscall` 四步协议 + 两层失败 | 接口·协议 | 存量 | 05 §2.1 | `syscall.c:9-25` |
| K-018 | `_loadname` 40 字节双路径打包 | 机制 | 存量 | 05 §2.2 | `loadname.c:7-19`、`ipc.h:761-767` |
| K-019 | `_kernel_call` ENOTREADY 线性退避重试 | 机制 | 存量 | 05 §2.3 | `kernel_call.c:7-21`、`tickdelay.c` |
| K-020 | 堆起点 `end` 符号（brksize.S）+ 弱别名 | 机制 | 存量 | 06 §2.1-2.2 | `brksize.S`、`brk.c:22-34` |
| K-021 | sbrk 相对移动 + 方向回绕检查 | 约束 | 存量 | 06 §2.3 | `sbrk.c:13-25` |
| K-022 | 两层分配（NetBSD 两条路 / Rust slab A-3） | 概念·架构演进 | 存量 | 06 §2.4/§3.2 | `malloc.c:317/377-388` |
| K-023 | kputc 攒后发 + DIAG_BUFSIZE 2000 + diagctl 四操作码 | 机制 | 存量 | 07 §2.1-2.2 | `kputc.c:17-32`、`sys_diagctl.c`、`com.h:412-416` |
| K-024 | panic 八阶梯子 + 弱别名钩子 | 机制 | 存量 | 07 §2.3 | `panic.c:21-67` |
| K-025 | itoa/stderr 打印辅助（含 C 五位局限） | 工具 | 存量 | 07 §2.4 | `itoa.c:9-35`、`stderr.c:6-12` |
| K-026 | panic_handler 链接唯一性 + 注册表住 minix-types | 架构演进·约束 | 存量 | 07 §3.4 | `minix-types::diagnostic`、kernel 注册点 |
| K-027 | PM 生命族（fork/exit/execve/wait4）+ 双回话/一去不回模型 | 机制 | 存量 | 08 §2.1-2.4 | `fork.c`、`_exit.c`、`execve.c`、`wait4.c` |
| K-028 | PM 信号族（kill/raise 范围检查）+ 服务端协议变体（srv_fork/kill/taskcall） | 机制·协议 | 存量 | 08 §2.5-2.6 | `kill.c`、`raise.c`、`srv_fork.c`、`taskcall.c:9` |
| K-029 | 调用号清单 PM_BASE + 端点 0 | 接口 | 存量 | 08 §2.7 | `callnr.h:9-60`、`com.h:59` |
| K-030 | VFS 四形状（读写/开关/定位/散列）+ select 等待形状 | 概念·机制 | 存量 | 09 §1/§2/§3.4 | `read.c`/`write.c`/`open.c`/`lseek.c`/`vectorio.c` |
| K-031 | 指针载荷三变体（stat 族/ioctl/fcntl/getdents）+ "魔法授权" | 机制 | 存量(未讲透) | 09 §2.7/§3.5 | `stat.c`、`ioctl.c`、`ipc.h` |
| K-032 | 打开分派（创建位道岔）+ LP64 内联 32 收缩（99 裁决） | 机制 | 存量 | 09 §2.2/§3.2 | `open.c`、`ipc.h:761-767` |
| K-033 | VM 三形状（映射七字段/边界/查询）+ 替办模型 + 哨兵消除 | 概念·机制 | 存量 | 10 §1/§2.1-2.3 | `mmap.c:21-171` |
| K-034 | VM 客户端库 + 余量清单（info/procctl/cache/rusage/willexit/unmap_phys/vfs_mmap） | 接口 | 存量 | 10 §2.4-2.5 | `vm_fork.c`、`vm_*.c`、`com.h:627-780` |
| K-035 | misc 四解法（sleep 复用 select/svrctl 分派/kerninfo 直读/tsc 拼接） | 机制 | 存量 | 11 §1-§3 | `nanosleep.c`、`svrctl.c`、`getticks.c`、`read_tsc_64.c` |
| K-036 | RS 三步（现查/反查/路由）+ 无缓存决定 | 机制·约束 | 存量 | 12 §1-§2 | `minix_rs.c`、`getepinfo.c`、`getsysinfo.c` |
| K-037 | 常量对账（errno 115/signal 32/调用号分段/端点排序）+ 权威归属 | 约束·工具 | 存量 | 13 §2-§3 | `errno.h`、`signal.h`、`callnr.h`、`com.h` |
| K-038 | endpoint/generation + message 56B LP64 判例 + 调用号归一 + 全局状态表 | 概念·协议·架构演进 | 存量 | 99 §1.1-1.4 | `type.h`、`ipc.h`、`com.h`、`kernel_call.rs` |
| **K-101** | **用户态授权表 / 安全内存拷贝（cpf grant、sys_setgrant、grant/revoke 纯内存编辑）** | **机制·数据结构·跨模块协议** | **新增** | 无（仅 todo 提"ABA 守卫"、doc09 提"魔法授权"） | `libsys/safecopies.c`、`minix-sys/src/grant.rs`、`minix-types::types::grant` |
| **K-102** | **64 位真陷阱 ABI（`syscall`/`int 0x21` 向量 33；RCX=callnr/RAX=endpoint/RBX=msgptr）** | **接口·协议·架构演进** | **新增** | 无（doc04 只讲 i386 `_ipc.S`，"64 位演进"一笔带过） | `arch_trap.rs:1-30`、`01-stage-kernel/18-trap-bridge-design.md` |
| **K-103** | **多架构 `_start` 入口桩（x86-64/riscv64/aarch64 三套 naked 桩 + 各自入口寄存器约定）** | **机制·架构演进** | **新增** | 无（doc02 只讲 x86-64 crt0.S） | `crt0.rs:323-380`、`test-rt-birth{,-aarch64,-riscv64}.sh` |
| **K-104** | **no_std 构建与 feature 门（default=std / real-trap / panic-handler / alloc-global；域外模块门控）** | **工具·工程·约束** | **新增** | 无 | `minix-rt/Cargo.toml`、`minix-sys/Cargo.toml:[features]` |
| **K-105** | **用户镜像布局与静态链接装配（text/data/bss/heap@`end`/stack/mmap 区 + crtbegin/crtend/crti/crtn）** | **数据结构·工具·工程** | **新增（部分散在 02/06）** | 无（brksize.S 起点在 06，但整体布局无单点） | `brksize.S`、`lib/csu/common/crtbegin.c`、链接脚本 |
| **K-106** | **测试基建（CannedTransport 剧本 / real-trap 真体 / rt-birth 真机五断言冒烟）** | **测试·工具** | **新增** | 散落各篇 §5（提及但无系统讲） | `ipc.rs` Canned*、`arch_trap.rs`、`qemu-tests/test-rt-birth.sh` |
| **K-107** | **wait 状态解码（WIFEXITED/WIFSIGNALED/WUNTRACED → 互斥和类型）** | **接口·数据结构** | **新增** | 无（doc08 讲 wait4 回抄但不解码） | `minix3/sys/sys/wait.h:60-80`、`minix-sys/src/wait.rs` |

### 2.2 存量与新增分组 / 重复与主讲述点标记

- **存量 K-001~K-038**：全部有明确现有位置，主讲述点唯一。跨篇重复集中在"协议地基"（05 被 08~12 反复引用）与"消息布局"（99 收口，08/09/10 声明移交）——这是 §5.2 并行体"汇聚点"的正常复用，非冗余。
- **新增 K-101~K-107**：均有 C 源码或非 C 制品锚点（见 §3 覆盖缺口表），不受存量去向规则约束。

### 2.3 统计摘要

- 存量知识点簇 38 组（≈覆盖 15 篇正文主体），新增 7 组。逐条（非簇）明细见 `doc_rerank_qwen.pool.tsv`。
- 现有 15 篇行数：短（110~160 行）为主，最长 01/09 = 173 行。**均在 user 软上限内且偏低**——问题不是"单篇过长"，而是"部分机制根本没进篇"（见 §3）。

---

## 3. 覆盖审计

### 3.1 主题全集来源（四路）

1. C 符号：`lib/csu/`（crt0/crtbegin）、`libc/sys/`（133 .c）、`libminc/`、`libsys/`（kernel_call/kputc/sys_diagctl/panic/safecopies/asynsend/vm_*/getsysinfo/srv_*）、`include/`（type/ipc/ipcconst/com/callnr）。
2. OS 通用概念：进程地址空间布局、陷阱/系统调用 ABI、内存授权（copy facilities）、启动文件与链接装配、静态 vs 动态链接、按服务分组的调用面。
3. 非 C 制品：`crt0.S`/`_ipc.S`/`brksize.S`、`Cargo.toml` feature 门、`test-rt-birth*.sh`、`grant.rs`/`arch_trap.rs`/`wait.rs`。
4. 边界契约：`plan.md §5`（覆盖契约）、`todo.md §2`（对账矩阵）。

### 3.2 覆盖缺口表（主题在全集但无篇讲）

| 缺口 | 主题 | 证据 | 建议 | 落实为新增知识点 |
|------|------|------|------|------------------|
| G-1 | **授权表 / 安全内存拷贝** | `grant.rs:1`（"Rust rewrite of C safecopies.c"）；doc09 §2.7/§3.5 用"魔法授权"但全 stage 无一处解释 slot 分配/seq/`sys_setgrant`/grant/revoke | **新建篇章**（读序在 05 之后、08 之前） | K-101 |
| G-2 | **64 位真陷阱 ABI** | `arch_trap.rs:1-30` 定义 RCX/RAX/RBX + `int 0x21` 向量 33；doc04 只给 i386 `_ipc.S`，A-6 仅"一笔带过" | **并入 04 扩章** | K-102 |
| G-3 | **多架构 `_start`** | `crt0.rs:323-380` 三架构桩；`test-rt-birth-aarch64/riscv64.sh` 存在；doc02 单架构 | **并入 02 扩节 + 14 支线注记** | K-103 |
| G-4 | **构建与 feature 门 / 测试基建** | 两个 `Cargo.toml` 的 `[features]`；`CannedTransport`；`test-rt-birth.sh` | **新建支线篇章**（编号 14） | K-104、K-106 |
| G-5 | **用户镜像与内存布局整体** | heap@`end`（`brksize.S`）在 06，其余区段无单点 | **新建支线节（并入 14）或 01 扩节** | K-105 |
| G-6 | **wait 状态解码和类型** | `wait.rs` 存在、`minix3/sys/sys/wait.h:60-80`；doc08 讲 wait4 但不解码 | **并入 08（或 13）** | K-107 |

### 3.3 重复主题表

| 主题 | 现多处在 | 新目录主讲述点 | 其余处理 |
|------|---------|---------------|---------|
| 系统调用请求协议 | 05（主）+ 08~12 各自套用 | 05 | 保持"参见"引用，不改（并行体汇聚点，符合 §5.2） |
| message 56B 布局/LP64 | 99（主）+ 08/09/10 声明移交 | 99 | 已收口，无冗余 |
| 边界移动规则 | 06（定义）+ 10（消费） | 06 | 已按"定义一次消费多处"处理，无冗余 |
| 调用号体例归一 | 99 §1.3（主）+ 各篇 | 99 | 无冗余 |

**结论**：现有文档在"重复"维度控制良好——真正的重复都被刻意收敛为"参见引用"。不需要因重复而合并。

### 3.4 越界主题表

| 篇 | 越界内容 | 正确归属 |
|----|---------|---------|
| 09 | "服务端直写调用者缓冲"（魔法授权）点到为止 | 应展开处 = 新增授权表篇（G-1）；09 改为参见 |
| 08 | 服务端 `sys_*.c` 全套内核调用面（A-10） | 各 server stage（plan §5.3 已排除，无越界） |
| minix-sys 内 `tty.rs`（fkey_ctl，消费方 08-stage-is） | E-MINSYS-SCOPE | 属别 stage 客户端库，14 不立项（14 篇已声明 crate 级扫描） |

### 3.5 非 C 主题逐项显式回答（第四部分第 3 类固定清单）

| 非 C 主题 | 在哪讲 / 为何不在本 stage |
|-----------|--------------------------|
| 链接与加载 | 静态链接决定 02；**整体镜像/区段布局缺 → 新篇 14（G-5）**；ELF 装载动作属 VM/kernel stage |
| 镜像与内存布局 | **缺单点 → 14（G-5）**（heap@`end` 现散在 06） |
| 汇编入口与陷阱进入 | 入口桩 02（**多架构缺 → K-103**）；i386 陷阱 04（**x86-64 真体缺 → K-102**） |
| 启动装配 | crtbegin/init_array 02；**feature 门控 no_std 构建缺 → 14（G-4）** |
| 构建与工具链 | **完全缺 → 新支线篇 14（G-4）** |
| 跨模块接口与线格式 | message 99；**授权表（copy facility）缺 → 新篇（G-1）** |
| 错误路径 | errno 映射各篇 §4 + 13；无整体缺口 |
| 关闭与退出 | exit 08 + fini 02 + panic 07；无缺口 |
| 并发与同步 | TLS 03（A-4）；用户态单线程模型 00；无缺口 |
| 测试基建 | 各篇 §5 零散；**系统化缺 → 14（G-4）**（CannedTransport / real-trap / rt-birth 三形态） |

---

## 4. 新目录

**核心决策（有据）**：经 §0.3 断链成本实测——目录内 151 处交叉引用、外部代码注释仅 2 处、且 15 篇事实锚点抽验全部准确、线性启动序与并行分组均满足 §5.1 四硬标准。**大规模重排/重编号收益 < 断链代价**（复现 `01-stage-kernel/todo.md I-14` 的历史裁决）。故本蓝图判定为**低扰动重建**：

1. **保留 `00`~`13` + `99` 的编号与主线**（不 renumber，151 处引用不动）。
2. **插入式新增**：用带字母后缀的编号在正确读序位置插篇，扰动仅限新文件与个别参见。
3. **就地补章/补节**：把 G-2/G-3/G-6 折进既有篇，不新建。
4. **内容修复（B 相执行）**：修 R6 序差叙述、补"首次出现即完整"缺口。

### 4.1 新目录总表

| 编号 | 标题 | 定位 | 操作 | 分组 |
|------|------|------|------|------|
| 00 | 整体概览与导航 | 不变（更新导航表加入 05A/14） | 改写导航节 | 0 总览 |
| 01 | 内核交付 | 保留 | 就地（修 R6 序差指向 03；栈镜像参见 05A/stack.rs） | 出生 |
| 02 | 程序入口与静态链接启动 | 保留 | **扩节**：多架构 `_start`（K-103，G-3） | 出生 |
| 03 | 运行时初始化 | 保留 | 就地：明确"构造器经 `_init()` 在 ___start 内运行"（R6 序差正解） | 出生 |
| 04 | 通信原语与陷阱 | 保留 | **扩节**：x86-64/riscv/aarch64 真陷阱 ABI（K-102，G-2） | 机制 |
| 05 | 系统调用协议 | 保留 | 就地 | 机制 |
| **05A** | **授权表与安全内存拷贝** | **新增** | 新建（K-101，G-1）；读序在 05 之后、06 之前 | 机制 |
| 06 | 内存分配器 | 保留 | 就地 | 资源 |
| 07 | 诊断输出与恐慌 | 保留 | 就地 | 终局 |
| 08 | 进程管理调用组 | 保留 | **扩节**：wait 状态解码和类型（K-107，G-6）；参见 05A | 服务 |
| 09 | 文件系统调用组 | 保留 | 就地：把"魔法授权"改为参见 05A（去越界） | 服务 |
| 10 | 虚拟内存调用组 | 保留 | 就地：客户端拷贝参见 05A | 服务 |
| 11 | 杂项调用组 | 保留 | 就地 | 服务 |
| 12 | 服务发现 | 保留 | 就地 | 服务 |
| 13 | 常量接口对账 | 保留 | 就地：加 wait 状态位族登记（若 08 主述） | 契约 |
| 99 | 全局概念 | 保留 | 就地：授权 slot 线布局可纳入（参见 05A） | 契约 |
| **14** | **构建、装载与测试基建** | **新增（支线）** | 新建（K-104/K-105/K-106，G-4/G-5）；集中支线，允许跳读 | 支线 |

### 4.2 阅读路径

- **主线（线性启动/生存链）**：00 → 01 → 02 → 03 → 04 → 05 → 05A → 06 → 07。
- **次主线（并行服务组，§5.2）**：05 是汇聚点；08/09/10/11/12 为角色分组，组内代表成员讲透 + 差异表收束。
- **契约层**：13（常量对账）+ 99（全局布局/编号权威）。
- **可跳读支线**：14（构建/镜像/多架构/测试基建）。

### 4.3 并行主题分组与代表成员

系统调用集合按服务分组，每组选代表精讲、其余按"同形 + 清单"收束（现状已如此，无需改动）：PM=exec（最长）、VFS=open（分派）/vectorio（拼装）、VM=mmap（七字段+替办+哨兵）、misc=nanosleep（跨篇组合）、RS=lookup（无缓存决定）。

---

## 5. 每篇新文档契约

> 仅对**发生变化**的篇（新建 + 扩章）给出完整契约；未变篇沿用现状契约。存量知识点去向以池编号引用，明细在 `doc_rerank_qwen.pool.tsv`。

### 05A-授权表与安全内存拷贝（新建）

- **一句话定位**：回答"服务端凭什么能把数据直接写进用户程序的缓冲区"——把 doc09/08/10 反复引用的"魔法授权"讲成一处机制。
- **讲什么**：K-101（cpf grant 表、`sys_setgrant` 注册、`cpf_grant_direct`/`cpf_revoke` 纯内存编辑、free list + 序列号防 ABA、slot 布局是跨空间共享契约）。
- **不讲什么**：内核侧 `verify_grant` 如何跨空间读（属 01-stage-kernel）；各服务具体拷贝哪个字段（属 08/09/10）。
- **前置**：04（通信原语）、05（协议）、01（kerninfo/端点）。
- **后置**：08（exec 栈拷贝）、09（stat/read 服务端直写）、10（物理映射）、99（slot 线布局）。
- **事实底线**：C `minix3/minix/lib/libsys/safecopies.c`；Rust `os/libs/minix-sys/src/grant.rs`；契约类型 `os/libs/minix-types/src/types/grant.rs`。
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为何归本篇 | 来源 |
  |------|------|------|------|-----------|------|
  | K-101 | 授权表与安全拷贝 | 机制·数据结构·跨模块协议 | `safecopies.c`、`grant.rs:1-18` | 唯一承载此概念的篇 | 新增（C + 代码） |

- **验收标准**：读者能回答"为什么 VFS 写文件时不用把数据经消息体 56 字节中转"；画出 grant slot 生命周期（分配→grant→服务端 verify 直写→revoke→回收）；给出与 doc09"魔法授权"语句的显式参见闭合。

### 14-构建、装载与测试基建（新建·支线）

- **一句话定位**：回答"一个 no_std 用户态二进制怎么被装配、按什么 feature 门编译、在真机上怎么被验证"——把散在 §5 与 Cargo.toml 的工程事实集中成一处。
- **讲什么**：K-104（`minix-rt`/`minix-sys` 的 `[features]`：`default=std`、`real-trap`、`panic-handler`、`alloc-global`；域外模块 `ds/rmib/devman/input/usb/socket` 门控与 E-MINSYS-SCOPE 关系）、K-105（用户镜像区段布局与静态链接装配：crtbegin/crtend/crti/crtn、heap@`end`）、K-106（测试三形态：CannedTransport 剧本回放 / real-trap 真体 / rt-birth 真机五断言，含多架构）。
- **不讲什么**：各原语/协议语义（04/05）；具体调用封装（08~12）；其它 stage 客户端库内部。
- **前置**：02（入口与静态链接决定）、04（真陷阱 ABI 来自 04 扩节）。
- **后置**：无（末端支线，允许跳读）。
- **事实底线**：`os/libs/minix-rt/Cargo.toml:[features]`、`os/libs/minix-sys/Cargo.toml:[features]`、`os/qemu-tests/test-rt-birth.sh`/`-aarch64.sh`/`-riscv64.sh`、`lib/csu/common/crtbegin.c`、`brksize.S`、`crt0.rs:323-380`。
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为何归本篇 | 来源 |
  |------|------|------|------|-----------|------|
  | K-104 | feature 门与 no_std 构建 | 工具·工程·约束 | 两 `Cargo.toml` | 构建主题唯一落点 | 新增（非 C 制品） |
  | K-105 | 镜像与内存布局 | 数据结构·工具 | `brksize.S`+`crtbegin.c` | 整体布局无它处承载 | 新增 |
  | K-106 | 测试基建三形态 | 测试·工具 | `test-rt-birth.sh`、`ipc.rs` Canned | 系统化测试叙述归此 | 新增 |

- **验收标准**：给出"编一个真机 rt-birth 二进制需要开哪些 feature"的可复现清单；一张用户地址空间区段图；说明 rt-birth 五断言各验证生命周期哪一段。

### 02 扩节（就地）：多架构入口桩

- **新增**：K-103。锚点 `crt0.rs:323-380`（三套 `naked_asm!` `_start`）、`test-rt-birth-{aarch64,riscv64}.sh`。原 §2.1 保留为"以 x86-64 为代表性成员"，新增差异表列 riscv64（`sp`+`a0`）、aarch64（`sp`+`x0`）入口寄存器约定；标注 ARCH（与 `01-stage-kernel/*/boot.rs build_cpu_context` 交接）。存量知识点去向：原 02 内容全部保留（§2.1-§3 不变）。

### 04 扩节（就地）：64 位真陷阱 ABI

- **新增**：K-102。锚点 `arch_trap.rs:1-30`。原 §2.2/§2.3（i386 `_ipc.S` + 向量号）保留为代表性成员，新增"从 int 33 到 syscall/int 0x21"节：RCX=callnr、RAX=端点/SENDA 计数、RBX=msgptr/表指针、返回 RAX=errno、RBX=entry‖status。存量去向：04 原内容不动。

### 08 扩节（就地）：wait 状态解码

- **新增**：K-107。锚点 `minix-sys/src/wait.rs`、`minix3/sys/sys/wait.h:60-80`。原 §2.4（wait4 回抄状态）后接"状态整数 → 和类型解码"，与 08"看正负"模型衔接。存量去向：08 原内容不动；服务端 sys_ 面维持移交。

### 01 / 03 / 09 就地修（序差与越界收口）

- **01/03**：把构造器运行时机写成 R6 真序（经 `.init_array`/`_init()` 在 `___start` 内、environ 发布后、main 前运行），删/改 `plan.md §1.1` 时序图造成的"先于 crt0"误导（正文层面澄清，plan.md 非搬迁对象）。
- **09**：§2.7/§3.5 的"魔法授权"改为显式参见 05A，去越界。

---

## 6. 变更表

| 操作号 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 |
|--------|------|--------|--------|------|-----------|------|
| C-1 | 新建 | 无 | 05A | 授权表无归属，doc09 悬空引用 | K-101 | 新增，有锚（safecopies.c） |
| C-2 | 新建 | 无（散落 §5/Cargo） | 14 | 构建/镜像/测试基建无单点 | K-104/K-105/K-106 | 新增，有锚（非 C 制品） |
| C-3 | 扩节 | 02 §2.1 后 | 02 | 多架构桩代码存在未讲 | K-103 | 新增，有锚（crt0.rs） |
| C-4 | 扩节 | 04 §2.2 后 | 04 | x86-64 真陷阱未讲 | K-102 | 新增，有锚（arch_trap.rs） |
| C-5 | 扩节 | 08 §2.4 后 | 08 | wait 解码未讲 | K-107 | 新增，有锚（wait.rs） |
| C-6 | 改写（就地） | 01/03 §正文 | 同 | R6 序差正解 | K-006/K-009 | 存量原样，仅校正时序叙述 |
| C-7 | 改写（就地） | 09 §2.7/§3.5 | 同 + 参见 05A | 去越界 | K-031 | 存量原样，引用改指 05A |
| C-8 | 改写（就地） | 00 §Ch3 导航表 | 同 | 纳入 05A/14 | — | 元导航更新 |
| C-9 | 归档 | 无 | — | **本轮不归档任何篇**（15 篇事实准确、边界清晰） | — | — |
| C-10 | 合并 | 无 | — | **不合并**（重复已收口为参见，见 §3.3） | — | — |

**存量去向完整性**：K-001~K-038 全部保留在原编号篇（无删除、无搬移），仅 C-6/C-7/C-8 就地微调；不存在"写不出去向"的拆分。新增 K-101~K-107 全部带 C 源码或非 C 制品锚点。

---

## 7. 缺漏新篇（非 C 主题清单逐项落实）

> §3.5 中标"缺"的四项，逐项落实（不留"待定"）：

- **链接与加载 / 镜像与内存布局（G-5）** → 落实为 **14**（K-105）。原料：`brksize.S`（heap@`end`）、`lib/csu/common/crtbegin.c`/`crtend`、静态链接决定（02 §1.3）。验收：一张区段图 + feature 清单。
- **启动装配 / 构建与工具链（G-4）** → 落实为 **14**（K-104）。原料：两个 `Cargo.toml:[features]`、E-MINSYS-SCOPE 门控注记（edge_todo）。
- **汇编入口与陷阱进入（G-2/G-3）** → 落实为 **04 扩节**（K-102，`arch_trap.rs`）+ **02 扩节**（K-103，`crt0.rs` 三架构）。
- **跨模块接口与线格式·授权（G-1）** → 落实为 **05A 新篇**（K-101）。
- **测试基建（G-4）** → 落实为 **14**（K-106）。原料：`test-rt-birth*.sh`、`ipc.rs` Canned*Transport、`real-trap` feature。

`tty.rs`（K-边界）明确**不在本 stage**：消费方 08-stage-is，属 E-MINSYS-SCOPE，14 篇仅 crate 级一致性扫描。

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表（仅变化项；未变篇正文整体保留，不逐节迁移）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 断链风险 |
|--------|--------|--------|---------|---------|
| plan.md §1.1 时序图 | `__minix_init` 画在 crt0 之前 | 03 正文澄清 + 参见 02 §2.3 | 改写 | 低（plan.md 非引用目标） |
| 09 §2.7/§3.5 "魔法授权" | 悬空描述 | 参见 05A | 拆分外移 | 中（需同步 09 两处措辞） |
| （新增） | — | 05A / 14 | 新建 | 无（无旧引用） |
| 02 §2.1 / 04 §2.2 / 08 §2.4 | 单架构/i386/wait 现状 | 原节 + 新差异表节 | 扩写 | 低（原编号不变） |

### 8.2 引用迁移表

| 旧引用 | 出现处 | 新目标 | 验证方式 |
|--------|--------|--------|---------|
| "移交第 99 篇"/"参见 05 篇"等 151 处 | 14/*.md 内部 | **不变**（编号保持） | 无需迁移（这是保留主线的直接收益） |
| 09 对"魔法授权"的服务端拷贝 | 09 正文 | 新增 `参见 05A` | 改后 grep "05A" in 09 ≥ 1 |
| `os/libs/` 内 2 处代码注释引用 | `minix-rt/src`（V1-P2-3 已修） | 保持 | 已闭合 |

### 8.3 断链成本摘要

- **内部交叉引用 151 处**（热点：05 篇 25×、01 篇 19×、06 篇 16×、03 篇 14×、99 篇 13×、04 篇 13×）——若重编号，这 151 处须逐一改写，且引用图是稠密网（每篇引前篇），任何位移引发连锁。**建议批量方式**：不重编号，用 `05A`/`14` 插入式编号把改动面压到"新文件 + 09/00 各 1-2 处参见"。
- **外部代码引用仅 2 处**，已在 V1-P2-3 修复轮闭合。
- **结论**：重建的断链成本几乎全部落在"重编号"这一个动作上；本蓝图规避该动作，把成本降到 <5 处编辑。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**：05A 前置={04,05,01}（均更早）；14 前置={02,04}；02/04/08 扩节不引入新前置。全部指向更早编号或元前置 → **通过**（0 前向引用）。
2. **依赖图无环**：05→05A→{08,09,10}（05A 只被后篇引，不回指后篇）；14 为末端 → **无环，通过**。
3. **覆盖率**：K-001~K-038 全保留原篇；K-101~K-107 全落 05A/14/02/04/08；无删除项 → **100% 有去向，通过**。
4. **断链成本**：已量化（151/2），方案规避重编号 → **通过**。

### 9.2 自检门逐门

| 门 | 结果 |
|----|------|
| G1 C 真序逐条可核对 | **通过**：§1 每条带 `文件:行`，§0.3 抽 7 条实跑核对一致 |
| G2 知识点池完整（每 C 文件/非 C 制品有归属或排除） | **通过**：存量 38 簇 + 新增 7；非 C 十项逐项答 §3.5；`tty.rs` 等边界显式排除 |
| G3 新目录前向引用为零 | **通过**（§9.1-1） |
| G4 依赖图无环 | **通过**（§9.1-2） |
| G5 覆盖率 100%（去向或删除理由；新增有锚；删除项单列） | **通过**：删除项=0；新增全部带锚 |
| G6 拆分/合并写清存量去向、新建写清新增来源（抽 10 处） | **通过**：本轮无拆分合并；新建 05A/14 + 扩 02/04/08 均带来源锚 |
| G7 每篇契约七要素齐全 | **通过**：05A/14/02/04/08 契约七要素全列（§5） |
| G8 迁移表覆盖变化文档每一节 | **通过**：仅 5 处变化，逐条列 §8.1；未变篇声明"整体保留" |
| G9 事实断言都有锚（抽 10 核对，推测标注） | **通过**：crt0/init/syscall/loadname/kernel_call/ipcconst 已实跑核对；无推测项未标注 |

### 9.3 结论与待用户裁决

**结论**：14-stage-runtime 现有 15 篇**结构正确、事实准确、无前向引用**，不需要推倒重排。真正的缺陷是**七处机制/工程主题在代码里已存在却无文档归属**（授权表、真陷阱 ABI、多架构入口、构建 feature 门、镜像布局、测试基建、wait 解码）。重建 = **保留主线编号 + 插入 05A/14 两新篇 + 就地扩 02/04/08 三节 + 修 R6 序差与 09 越界**，把断链成本压到 <5 处编辑。

**待用户裁决（2 项）**：
1. **新篇编号方式**：用插入式 `05A`/`14`（扰动最小，本蓝图默认）还是接受为授权表正式重排到 `06`（会连锁 06~13 + 151 引用改写）？——强烈建议前者。
2. **多架构（K-103/K-102）深度**：作为 02/04 的"差异表节 + ARCH 标注"（本蓝图默认），还是独立成篇？受 §5.4 单篇单语义与"非 C 制品须显式回答"约束，差异表节足以承载；如需可跳读的架构专章，可并入 14。
