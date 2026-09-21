# 08-stage-is 文档重建蓝图（glm）

```text
your_name(AI agent name) = glm
target_dir(关注的工作目录) = notes/rewrite/fork-syscall-rewrite/08-stage-is
repo_root(仓库根目录) = /home/xzhao/github/minix-rs
当前提交号 = d6ecd22cae78345e746c82ad7cbe3c1612c151c2

任务 = R 相·重建蓝图：输出 08-stage-is/doc_rerank_glm.md，不改任何正文。
约束 = 未引用 .design/ 与 tmp_design_and_todo/；未读取其它 AI 的 doc_rerank_* 产物；
       所有落盘产物仅本文件一个，带 _glm 后缀。
```

---

## 0. 元数据

- 执行者：glm；日期：2026-09-19；目标目录：`notes/rewrite/fork-syscall-rewrite/08-stage-is/`。
- **结论先行**：本 stage 的 11 篇文档（00~10 + 99）**全部为成稿**——01~10 于 2026-09-04 按 plan §6 路线写成并全部 CONVERGED，00/99 于 2026-09-15 V1 执行轮成文（todo.md Fix #8），各篇已带"V1 执行轮更新"注记；代码侧 5689 行、115 个测试全绿。因此本蓝图的操作集是**"保编号、修漂移、补薄点"**：
  1. **保编号**——00~10+99 的编号顺序即"启动→协议→分派→数据面→六转储域→收口"，满足四条硬标准（§9 G3/G4），且篇间互引、外部引用、代码注释全部以现有文件名为锚，重排/重编号零收益；
  2. **修漂移**——最大的一处：**E-ISPROD kernel 半已于 2026-09-16 落地（commits 7d89ec114、987773bb3，快照权威上收 minix-types 方案 A），04/05 两篇文档未回写**（快照类型名 `*Snap` → minix-types `*Struct`、编码器入参 `i16` → `u32`、`KProcSnap` 等五个类型已删除），其次为测试基线漂移链（正文 ≤86 / todo 106 / 实测 **115**）与 04 篇一处 GET_MACHINE 内部措辞矛盾；
  3. **补薄点**——00 的启动主线图目前一行带过并指向 plan §1.2（与 06-stage-sched 00 骨架同型的"委托"缺陷，程度较轻），按契约内嵌全图；若干编辑缺陷（06/10 重复小结块、三处外文残留、06 一处乱句）。
- 审查范围：
  - **正文**：`[0-9][0-9]-*.md` 11 篇（3882 行）；
  - **参考材料**（不改，只作证据）：`plan.md`（340 行）、`todo.md`（271 行）、`draft/` 9 个文件（占位 README + 8 个 `tmp_*.md` 逐行素材）；
  - **范围外**：`.design/`、`tmp_design_and_todo/`、其它 AI 的 `doc_rerank_*`、`minix3/` 原树（只读）。

### 0.1 读取清单（步骤 0 四份清单）

**清单一：文档清单**（行数实测）

| 编号 | 文件 | 行数 | 状态 | 边界摘要 |
|---|---|---|---|---|
| 00 | 00-is-overview.md | 60 | 成文（V1 轮 Fix #8） | 聚合器定义/启动主线（简版，委托 plan §1.2）/无 boot_image/执行模型/导航 |
| 01 | 01-is-init-main.md | 701 | 成文+CONVERGED（2026-09-04）+V1 注记 | main.c 全部 + SEF 四注册 + ping 拦截 + 启动条件三证据 |
| 02 | 02-is-fkey-contract.md | 572 | 成文+CONVERGED +V1 注记 | FKEY 三命令/两消息/三层编号/客户端/TTY 侧契约 |
| 03 | 03-is-dump-dispatch.md | 360 | 成文+CONVERGED +V1 注记 | hooks 表/pressed/do_fkey_pressed/次主线路径图 |
| 04 | 04-is-data-acquisition.md | 457 | 成文+CONVERGED +V1 定型注记 | 五通道机制面 + Rust 五 trait 类型化出参 |
| 05 | 05-is-dump-kernel.md | 323 | 成文+CONVERGED +V1 注记 | dmp_kernel.c 全部 + 6 布局 ABI + 分页契约 |
| 06 | 06-is-dump-pm.md | 188 | 成文+CONVERGED +V1 注记 | PM 单表双转储 + 11 位编码 + 时钟面 + \r 分页 |
| 07 | 07-is-dump-vfs.md | 173 | 成文+CONVERGED +V1 注记 | VFS 双表双策略 + FP 位 + SDEV TODO 继承 |
| 08 | 08-is-dump-rs.md | 148 | 成文+CONVERGED +V1 注记 | RS 双表对齐 + 双源编码同值异义 |
| 09 | 09-is-dump-ds.md | 173 | 成文+CONVERGED +V1 注记 | DS 盘点 + 四类型 + 条件界游标 + 早返重放 |
| 10 | 10-is-dump-vm.md | 200 | 成文+CONVERGED +V1 注记 | VM 折叠状态机 + 双游标批机 |
| 99 | 99-is-global-concepts.md | 58 | 成文（V1 轮 Fix #8） | 常量权威 9 族对账/错误码/执行模型/排除项/跨服务引用 |

**清单二：C 源码清单**（逐文件确认）

| C 文件 | 行数 | 承载文档 |
|---|---|---|
| `minix3/minix/servers/is/main.c` | 148 | 01（全部）+ 02（init 调用点） |
| `minix3/minix/servers/is/dmp.c` | 132 | 02（map_unmap_fkeys）+ 03（hooks/分派/key_name/mapping_dmp） |
| `minix3/minix/servers/is/dmp_kernel.c` | 396 | 05 |
| `minix3/minix/servers/is/dmp_pm.c` | 109 | 06 |
| `minix3/minix/servers/is/dmp_fs.c` | 83 | 07 |
| `minix3/minix/servers/is/dmp_rs.c` | 74 | 08 |
| `minix3/minix/servers/is/dmp_ds.c` | 52 | 09 |
| `minix3/minix/servers/is/dmp_vm.c` | 157 | 10 |
| `minix3/minix/servers/is/{glo,inc,proto}.h` | 16+32+34 | 99（glo 死 extern 排除）/01（包含面）/各篇（签名） |
| `include/minix/com.h`（:49,64,90-93,205,236,252,316-345,412-415,476,507,729-734,872-877） | 常量 | 01/02/04/99 |
| `include/minix/keymap.h`（:14,16,93,135） | 常量 | 02/99 |
| `include/minix/ipc.h`（:1447-1454,1925-1931,2570,2623） | 结构 | 02 |
| `include/minix/sysinfo.h`（:11-17） | 常量 | 04/99 |
| `include/minix/callnr.h`（:60,120） | 常量 | 04 |
| `include/minix/type.h`（:214-232 minix_kerninfo） | 结构 | 04/05 |
| `sys/sys/errno.h`（:199 EDONTREPLY） | 常量 | 01/99 |
| `lib/libsys/fkey_ctl.c`（30 行） | 客户端 | 02 |
| `lib/libsys/getsysinfo.c`、`vm_info.c`、`sys_diagctl.c`、`sys_getinfo.c` | 客户端 | 04 |
| `drivers/tty/tty/arch/i386/keyboard.c`（:71-78,206,224,401-415,429-527,532-585） | 对端（fkey 面） | 02 |
| `servers/pm/misc.c`（:105-145）、`servers/vfs/misc.c`（:52-113） | 对端（getsysinfo 服务侧） | 04 |
| `etc/rc.minix`（:115-118）、`etc/system.conf`（:269-277） | 启动证据 | 00/01 |

**清单三：非 C 制品清单**

| 制品类 | 实际情况 | 归属 |
|---|---|---|
| 链接脚本/镜像布局 | 无自定义（目录仅 Makefile + 8 .c + 3 .h） | 不在本 stage |
| 汇编入口与陷阱进入 | 用户态 trap 归 minix-sys/rt（edge E1/E-ISWIRE 轨道）；本文档止步于 SefTransport/FkeyCtlTransport/Acquires 三缝 | 14-stage-runtime |
| 引导链与引导协议 | 无 boot_image 登记（`kernel/table.c` 17 项无 is）；rc 条件启动 + RS 动态加载 | 00/01（证据面）+ E-ISBOOT（实施面） |
| 构建脚本与工具链 | Makefile | WONTFIX（plan §5.4） |
| 跨模块接口与线格式 | fkey 两消息（02）+ GET/SI/VM_INFO 消息（04）+ 六域布局 ABI（05~10，kernel 半已上收 minix-types） | 02/04/05~10 |
| 错误路径 | 三层错误面（04 §2.6）+ EDONTREPLY（01/99） | 04/99 |
| 关闭与退出 | SIGTERM → unmap → exit(0)（01 §2.8） | 01 |
| 并发与同步 | 单线程事件循环、零分配、五游标实例隔离 | 00/99/各域 |
| 测试基建 | 115 个内联测试 + fake 接缝；真机联调归 E-ISBOOT/E5 | 各篇 §5 + 99 |
| Rust 实现入口 | `os/servers/is/src/` 13 文件 5689 行（dump_kernel 963、acquire 949、lib 747、dump_vm 478、tty_fkey 450、dispatch 419、dump_vfs 392、dump_pm 388、dump_ds 317、dump_rs 265、sef 237、state 51、main 33） | 各篇 §3/§4 |

**清单四：引用关系清单**

| 引用方 | 处数 | 形态 |
|---|---|---|
| 篇内互引实例 | ≈53 | 被引最多：03（13）、04（10）、02（6）、01（5） |
| `edge_todo.md` | 9 | E-ISWIRE/E-ISPROD/E-ISKMESS/E-ISBOOT 四条目及交叉注记 |
| `03-stage-rs/doc_rerank_deepseek.md`（6）、`03-stage-rs/doc_rerank_glm.md`（5）、`03-stage-rs/todo.md`（3） | 14 | 目录级引用（SEF 先例、目录表） |
| `plan.md`/`todo.md` 自引 | 4+7 | 参考材料 |
| `00-master-plan/README.md`（2）、`edge3.md`（2）、其它 stage 的 doc_rerank/todo（5 文件各 1-2） | 9 | 目录级 |
| `04-stage-pm/04-ipc-dispatch.md` | 1 | 目录级 |
| 代码注释（`os/` 下 .rs） | 1 | `os/servers/is/src/main.rs:4` → `08-stage-is/01-is-init-main.md` |

### 0.2 使用的命令与关键输出（证据摘录）

```bash
# 测试基线（115 passed；漂移链：正文 ≤86 → todo 106 → 实测 115）
cd os && cargo test -p minix-is
#   → test result: ok. 115 passed; 0 failed
# 各模块测试数：dump_kernel 23 / dispatch 16 / acquire 15 / lib 12 / tty_fkey 11 /
#   dump_pm 9 / dump_vfs 8 / dump_vm 6 / dump_rs 5 / dump_ds 5 / sef 4 / state 1（合计 115）
# clippy 归属：minix-is 本体 4 条（全在测试代码：acquire.rs×3 + dump_vfs.rs×1）；
#   生产码 0 条；minix-types 1 + minix-sys 2（既有，归 E-MINSYS-HYGIENE）
# E-ISPROD 落地证据（快照权威上收，04/05 未回写的判定依据）
git log --oneline -1 -- os/libs/minix-types/src/types/proc_info.rs
#   → 7d89ec114 feat(types,is,vfs): D-15 E-ISPROD proc-tab 面——快照权威上收 minix-types
grep -n 'pub struct ProcInfoStruct' os/libs/minix-types/src/types/proc_info.rs   # :26
grep -rn 'KProcSnap' os/servers/is/src/    # 零命中（类型已删除改 import）
grep -n 'fn s_flags_str' os/servers/is/src/dump_kernel.rs
#   → :51  pub const fn s_flags_str(flags: u32) -> [u8; 8]   （文档 05 §4.2 仍写 i16）
# GET_MACHINE 锚点与 Rust 处置
grep -n 'define GET_MACHINE' minix3/minix/include/minix/com.h     # :327
sed -n '568,577p' os/servers/is/src/dump_kernel.rs
#   → 注释明写 "C fetches a machine struct too but never reads it"，render_kenv 不取 machine
# E-ISWIRE 剩余面（IS main.rs 仍 fail-closed 占位）
sed -n '19,33p' os/servers/is/src/main.rs   # Unimplemented* 三占位
```

---

## 1. C 真序

### 1.0 阶段类型判定

**服务事件循环型**（主形态，与 sched 相同的"出生→主循环"骨架，但事件源唯一）+ **转储域集合型**（辅形态：六个转储域共享同一分派框架与数据面，按"统一框架篇 + 分域展开"组织）。判定理由：`servers/is/main.c` 是标准的 SEF 出生 + `while(TRUE)` 收通知循环；但 IS 与 sched 的关键差异是**只有一个合法事件源**（TTY notify），全部业务是"按键→转储"，因此次主线（一次功能键按压的旅程）天然成为贯穿 02~10 的组织线——这正是 plan §1.3 既定且正文已落实的形态，保持。

### 1.1 真序表

**A 启动段**

| 步骤 | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| A1 | 存在条件：`sysenv debug_fkeys != 0` 时 rc 执行 `up -n is -period 5HZ` | `etc/rc.minix:115-118`（up 行 :117） | 条件性 debug 服务；`-period 5HZ` = RS 每 5 秒 ping |
| A2 | RS 运行时加载 ELF，动态分配 endpoint（全树无 `IS_PROC_NR`） | `rg IS_PROC_NR` 零命中（plan §5.4 实证，本次确认） | 权限面 `service is { vm INFO; uid 0; }`（`etc/system.conf:269-277`） |
| A3 | `main()`：`env_setargs` + `sef_local_startup` | `main.c:31-41` | |
| A4 | SEF 四注册：init_fresh/init_lu/init_restart 同一回调（STATELESS）+ signal handler | `main.c:75-89` | |
| A5 | `sef_cb_init_fresh`（boot 锚点）：`map_unmap_fkeys(TRUE)` 向 TTY 登记 F1~SF12 观察者 | `main.c:94-102` → `dmp.c:45-65` | 01 与 02 的铆接点 |
| A6 | 进入 `while (TRUE)` 主循环 | `main.c:44` | |

**B 循环段（每一轮）**

| 步骤 | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| B1 | `get_work`：`sef_receive(ANY)` 阻塞收，失败 panic；写回 `who_e`/`callnr` | `main.c:121-130` | SEF ping 在 `sef_receive` 内部被拦截应答（`libsys/sef.c` 拦截段 + `sef_ping.c`），主循环无感 |
| B2 | 分类：`is_notify(callnr)`（旧形式，`com.h:93`）→ 是则 `_ENDPOINT_P(who_e)` 槽号比较 | `main.c:48-49` | 非 notify → 告警 + EDONTREPLY（:59-63） |
| B3 | TTY 分支：`do_fkey_pressed(&m_in)`；非 TTY notify：静默 EDONTREPLY（FIXME 缺口保留） | `main.c:50-57` | `TTY_PROC_NR=5`（com.h:64），用槽号不用裸端点（generation 位） |
| B4 | 回复门：`result != EDONTREPLY` 才 `reply`；`reply` 的 `ipc_send` 失败 → panic | `main.c:66-68,135-146` | EDONTREPLY=203（`sys/sys/errno.h:199`，伪码） |

**C 请求处理（do_fkey_pressed 内部与转储）**

| 步骤 | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| C1 | `fkey_events(&fkeys,&sfkeys)` 拉位图（消费读）；`s < 0` 告警且继续（memset 零兜底） | `dmp.c:82-85` | 位图 `int`，bit0 空置 |
| C2 | hooks 表 16 项遍历：`pressed(F1,F12,…)` / `pressed(SF1,SF12,…)` 命中即调用，无 break | `dmp.c:88-94`（宏 :70-72；表 :14-35） | 表序即执行序 |
| C3 | 各 dump 体：经五通道取数 → 格式化 printf → 22/24 行分页 | `dmp_kernel.c`/`dmp_pm.c`/`dmp_fs.c`/`dmp_rs.c`/`dmp_ds.c`/`dmp_vm.c` | 五通道见 D 段 |
| C4 | 恒返回 EDONTREPLY（转储走打印通道非 IPC 回复） | `dmp.c:97` | |

**D 五条数据通道（取数面）**

| 步骤 | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| D1 | `sys_getinfo(GET_*)`：内核八臂取数到调用方（endpt=SELF） | `com.h:236,316-345`；`libsys/sys_getinfo.c`；IS 用点 `dmp_kernel.c:101,129,133,174,197,201,261,265/328/368` + `dmp_vm.c:83` | GET_KENV 从不使用；GET_MACHINE 取而不用（:201） |
| D2 | `sys_diagctl_stacktrace(ep)`：请内核打印栈（不回传） | `com.h:252,412-415`；`syslib.h:166-168`；`dmp_kernel.c:378` | minix-rs 内核侧暂 ENOSYS（32-stack-tracing forward ref） |
| D3 | kerninfo 直读 `kmessages`（.usermapped 映射页） | `type.h:214-232`；`libc/sys/init.c`；`dmp_kernel.c:71` | A-3：minix-rs 无 usermapped → GET_KMESSAGES 等价子请求（E-ISKMESS） |
| D4 | `getsysinfo(SI_*)`：四服务 who→callnr 映射 + size 精确匹配 + `sys_datacopy` + root 门 | `libsys/getsysinfo.c`；`callnr.h:60,120`；`com.h:476,507`；服务侧 `pm/misc.c:105-145`、`vfs/misc.c:61-113` | IS 用 7 次：PM×1 表、VFS×2、RS×2、DS×1 |
| D5 | `vm_info_stats/usage/region`：三子命令 + region 游标（count/next 写回） | `com.h:729-734`；`libsys/vm_info.c`；`dmp_vm.c:66,94,110,131` | 服务端归 02-stage-vm/26 |

**E 退出段**

| 步骤 | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| E1 | SIGTERM（=15）→ `map_unmap_fkeys(FALSE)` → `exit(0)`；非 TERM 忽略 | `main.c:107-116` | 先 unmap 后 exit 顺序不变量 |
| E2 | 无人订阅后 `func_key` 仍计数（脏计数）、不通知 | `keyboard.c:532-585` | IS 死后 TTY 侧行为 |

### 1.2 序差表

| # | 运行时事实 | 教学序 | 理由 | 回指补偿 |
|---|---|---|---|---|
| 序-1 | fkey 注册（A5）先于一切事件 | 02 在 01 之后 | 01 只记锚点调用，注册机制需先有消息面词汇 | 01§2.7 "机制归 02"声明 |
| 序-2 | 分派（C1-C2）发生在收到通知后 | 03 在 02 之后 | 分派消费 02 的 EVENTS 位图与键码常量 | 03 篇首回指 02 §2.7 |
| 序-3 | 五通道取数（D1-D5）是各 dump 体的第一步 | 04 在 03 之后、05~10 之前 | 机制集中一篇，六域只讲布局解释（汇聚点+触发时机） | 03 §6/04 §6 双向消费映射表 |
| 序-4 | 六转储域在运行时由同一循环分派，无先后 | 05~10 按 hooks 表顺序展开（kernel 8 席最多故先） | 表序即教学序；kernel 域同时是"九通道直连"的最复杂样本，先难后易收敛体例 | 05 §6/06 §6 的体例过渡段 |
| 序-5 | getsysinfo 服务侧（PM/VFS/RS/DS 的 do_getsysinfo）与 IS 请求同时发生 | 只写客户端面，服务侧归各主权 stage | 跨 stage 分工（04 §1.3 边界声明） | 04 篇对端锚点齐备 |
| 序-6 | `debug_fkeys` 在 TTY 侧与 sysenv 是两个开关（同名不同层） | 02 §2.8 辨析置于协议篇末 | 排障知识，非主线 | 00/99 一句话声明 |

---

## 2. 知识点全集

> 存量 = 11 篇正文去重；新增 = 本次对照 C 源码、Rust 实现现状与 edge 边界材料发现。编号 stage 内唯一。

### 2.1 知识点池总表

**域 A：总览与执行模型（00/99）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|---|---|---|---|---|---|---|
| K-001 | IS = 调试转储聚合器（查看者与被查看状态不同地址空间的审计纪律） | 概念 | 存量 | 00§1、01§1.1 | `main.c:1-8` 文件头 | 00 主 |
| K-002 | 条件性 debug 服务（无 boot_image、rc 条件启动、system.conf 权限面、无 IS_PROC_NR/动态 endpoint） | 约束 | 存量 | 00§2-3、01§2.11 | `rc.minix:115-118`；`system.conf:269-277`；`rg IS_PROC_NR` 零命中 | 00/01 共担 |
| K-003 | 单线程事件循环 + 零分配（无 extern crate alloc）+ !Send 合理 | 约束 | 存量 | 00§4、99§3、01§4.1 | `lib.rs` 无 alloc；main.rs test 门 | 99 主 |
| K-004 | 文档导航与阅读路径 | 概念 | 存量 | 00§5 | — | 00 |
| K-005 | panic/warn 分界（transport 失败=事件循环死→panic；取数失败=本屏作废→告警续走） | 约束 | 存量 | 01§3 D5、04§2.6、99§3 | `main.c:126-127,144-145` vs `dmp_kernel.c:101-104` | 04 主、99 声明 |

**域 B：启动与主循环（01）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|---|---|---|---|---|---|---|
| K-006 | 四静态全局→IsServerState（call_nr 死状态已删，V1-P2-2） | 数据结构 | 存量 | 01§2.1/§3 D1 | `main.c:13-17`；`state.rs` | 01 |
| K-007 | main 三段式（取活→分类→回复；return 不可达→run 发散） | 机制 | 存量 | 01§2.2 | `main.c:31-71` | 01 |
| K-008 | SEF 四注册（三 init 同一回调=STATELESS；signal handler） | 机制 | 存量 | 01§2.6 | `main.c:75-89` | 01 |
| K-009 | boot 锚点 = map_unmap_fkeys(TRUE) 一行（UNUSED 参数即"出生无需外部信息"） | 机制 | 存量 | 01§2.7 | `main.c:94-102` | 01 |
| K-010 | SIGTERM 清理（只认 TERM=15；先 unmap 后 exit 顺序不变量） | 约束 | 存量 | 01§2.8 | `main.c:107-116` | 01 |
| K-011 | get_work 阻塞收 + panic | 机制 | 存量 | 01§2.9 | `main.c:121-130` | 01 |
| K-012 | SEF ping 透明拦截（sef_receive 内部应答，主循环无感） | 机制 | 存量 | 01§2.10 | `libsys/sef.c` 拦截段；`sef_ping.c`；`sef.h` | 01 |
| K-013 | is_notify 旧形式保留（FIXME 不跟进=行为变更拒绝）+ 槽号比较不用裸端点 | 约束 | 存量 | 01§2.3 | `com.h:90-93`；`main.c:48-49` | 01 |
| K-014 | 告警不对称（非法请求告警 vs 非 TTY 通知静默；保留 C FIXME 行为防"好心补日志"） | 约束 | 存量 | 01§2.3-2.4 | `main.c:53-56 vs 59-63` | 01 |
| K-015 | EDONTREPLY=203 回复抑制哨兵（伪码非错误码） | 约束 | 存量 | 01§2.5、99§2 | `sys/sys/errno.h:199` | 01 主、99 表 |
| K-016 | reply 的 ipc_send 失败 panic（回复路径不可恢复） | 机制 | 存量 | 01§2.5 | `main.c:135-146` | 01 |
| K-017 | Rust SEF trait 化（SefCallbacks 缺省三合一 + SefTransport 缝 + warn_* 三方法 + diag_out） | 架构演进 | 存量 | 01§3 D2/D4 | `sef.rs` | 01 |
| K-018 | 启动条件三证据链（无 boot_image / rc 条件 / 权限面窄） | 事实 | 存量 | 01§2.11 | 三处系统配置 | 01 主 |

**域 C：fkey 协议（02）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|---|---|---|---|---|---|---|
| K-019 | 三命令订阅模型（MAP/UNMAP/EVENTS；notify 无载荷→拉模式） | 概念 | 存量 | 02§1 | `com.h:874-877`；`ipc.h:2820` | 02 |
| K-020 | 两级编码 + 两消息布局（0x1301 外层/request 10-12 内层；56B 定长；无回显位；原地写回） | 接口与协议 | 存量 | 02§2.1-2.2 | `com.h:872-877`；`ipc.h:1447-1454,1925-1931,2570,2623` | 02 |
| K-021 | 三层编号（键码 F1=0x110/位号 1-12/下标 0-11；bit0 空置） | 约束 | 存量 | 02§1.3/§2.3 | `keymap.h:14,16,93,135`；`dmp.c:53-58` | 02 |
| K-022 | fkey_ctl 客户端（NULL 容忍；_taskcall 同步无 grant；状态+位图双通道；失败位保留；头注释过时已点破） | 机制 | 存量 | 02§2.4/§2.5 | `libsys/fkey_ctl.c` 全文；`sysutil.h:43-46` | 02 |
| K-023 | map_unmap_fkeys（hooks 驱动→注册集合=能力集合；失败告警不 panic；写回丢弃） | 机制 | 存量 | 02§2.6 | `dmp.c:45-65` | 02 |
| K-024 | TTY do_fkey_ctl 状态机（MAP 覆盖登记 DEAD_CODE→STATELESS 闭环；UNMAP owner EPERM 部分失败；EVENTS 消费读恒 OK） | 机制 | 存量 | 02§2.7 | `keyboard.c:429-527`（DEAD_CODE 段 :452-470 附近） | 02 |
| K-025 | func_key 通知链（events 先增后查；无载荷 ipc_notify；debug_fkeys 吞键门）+ 同名不同层辨析 | 机制 | 存量 | 02§2.8 | `keyboard.c:532-585,206,224,78,406` | 02 |
| K-026 | Rust FkeyId 类型化（bit0 不可编译）+ FkeyCtlTransport 缝 + fake TTY 镜像 + A-1 minix-types 类型 | 架构演进 | 存量 | 02§3、99§1 | `minix-types/src/ipc/tty.rs`；`tty_fkey.rs` | 02 |

**域 D：分派（03）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|---|---|---|---|---|---|---|
| K-027 | hooks 表 16 项（唯一键源；缺席键脏计数不可见；SF5 自指） | 数据结构 | 存量 | 03§2.1 | `dmp.c:14-40` | 03 |
| K-028 | pressed 宏（区间+位双检；+1 银行选择器） | 机制 | 存量 | 03§2.2 | `dmp.c:70-72` | 03 |
| K-029 | do_fkey_pressed（m 未用；s<0 告警续分派+memset 零兜底；循环无 break；恒 EDONTREPLY） | 机制 | 存量 | 03§2.3 | `dmp.c:73-98` | 03 |
| K-030 | key_name（三式；static 缓冲；Rust "?" 类型级消除） | 机制 | 存量 | 03§2.4 | `dmp.c:103-114` | 03 |
| K-031 | mapping_dmp 列宽格式（%10s；MAPPING_RULE 渲染已实现） | 机制 | 存量 | 03§2.5+V1 注记 | `dmp.c:120-132` | 03 |
| K-032 | 次主线路径图（按键→TTY→IS→EVENTS→分派→dump→EDONTREPLY） | 概念 | 存量 | 03§2.6 | 全链锚点 | 03 |
| K-033 | Rust DumpId 枚举 + dispatch_each 回调式零分配 + HOOKS 派生键表（V1-P3-1 后） | 架构演进 | 存量 | 03§3/§4 | `dispatch.rs`；`lib.rs` | 03 |

**域 E：数据面（04）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|---|---|---|---|---|---|---|
| K-034 | 五主权五通道总览 | 概念 | 存量 | 04§1.1 | — | 04 |
| K-035 | sys_getinfo（GET_* 全表含缺号 7/22；endpt=SELF；速记宏；IS 用 8 请求；GET_KENV 从不发） | 机制 | 存量 | 04§2.1 | `com.h:205,236,316-345`；`syslib.h:175-187` | 04 |
| K-036 | sys_diagctl_stacktrace（码复用 arg2；打印不回传；minix-rs 内核侧 ENOSYS 现状） | 机制 | 存量 | 04§2.2 | `com.h:252,412-415`；`sys_diagctl.c`；`dmp_kernel.c:378` | 04 |
| K-037 | kerninfo 直读 kmessages + A-3 演进（usermapped 移除→GET_KMESSAGES 等价子请求；E-ISKMESS） | 架构演进 | 存量 | 04§2.3/§3 D3 | `type.h:214-232`；`libc/sys/init.c`；`dmp_kernel.c:71` | 04 |
| K-038 | getsysinfo（who→callnr 四映射；SI_* 7 值；size 精确匹配 EINVAL；sys_datacopy；root 门 EPERM） | 机制 | 存量 | 04§2.4 | `getsysinfo.c`；`callnr.h:60,120`；`com.h:476,507`；`pm/misc.c:105-145`；`vfs/misc.c:61-113` | 04 |
| K-039 | vm_info 三子命令（region 游标协议 count/next 写回） | 机制 | 存量 | 04§2.5 | `com.h:729-734`；`vm_info.c` | 04 |
| K-040 | Rust 五通道类型化出参 + Acquires 超特质 + GetRequest 删除（V1-P1-2 定型；三义务：root 恒过/len 精确/RS 双拉陷阱命名） | 架构演进 | 存量 | 04§3 D2/§4.2 | `acquire.rs:104-118` | 04 |
| K-041 | **E-ISPROD kernel 半落地（快照权威上收 minix-types 方案 A）**：ProcInfoStruct/PrivInfoStruct/BootImageStruct/KinfoStruct/IrqHookStruct 上收单一权威；IS 删 KProcSnap 等五类型改 import；编码器入参 i16→u32；VFS 本地 SI_* 常量收敛 re-export | 架构演进 | **新增**（2026-09-16 两笔提交后文档未回写） | 04§4.2/05§2.7/§4.2 仍是旧签名 | `minix-types/src/types/{proc_info,priv_info,boot_image,kinfo,irq_hook}.rs`；commits 7d89ec114、987773bb3；edge_todo E-ISPROD 两条进度注记；`rg KProcSnap os/servers/is` 零命中 | **04/05 修正** |
| K-042 | GET_MACHINE 处置分歧（C 的 kenv_dmp 取 machine 不读 ：201；Rust 刻意不取不发——外部行为不变但属 deliberate deviation；04 §4.3.1 "IS 永不发 GET_MACHINE" 与 §2.1 "8 请求含 GET_MACHINE" 措辞矛盾） | 事实 | **新增**（矛盾修正） | 04§2.1 vs §4.3.1、05§2.6/§3 D1 | `com.h:327`；`dmp_kernel.c:201`；`dump_kernel.rs:568-577` 注释 | **04/05 修正** |

**域 F：转储域（05~10）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|---|---|---|---|---|---|---|
| K-043 | 三表共享地基 + PROCLOOP/PRINTRTS 宏 + LINES=22 + pagelines 真全局 + click_to_round_k 死宏（A-8） | 机制 | 存量 | 05§2.1 | `dmp_kernel.c:18-57` | 05 |
| K-044 | kmessages_dmp 环形展开（起算式；线性缓冲防撕裂） | 机制 | 存量 | 05§2.2 | `dmp_kernel.c:62-88` | 05 |
| K-045 | monparams 换行展开 + expand_newlines 截断硬化 + C 忠实修复（V1-P1-4：do/while 每个终止 NUL 都改写） | 机制 | 存量 | 05§2.3；todo Fix #6 | `dmp_kernel.c:93-116` | 05 |
| K-046 | irqtab 双表对读（#if 0 排除；IRQ_REENABLE 词列；masked 判定；NR_IRQ_HOOKS=16/VECTORS=16） | 机制 | 存量 | 05§2.4 | `dmp_kernel.c:121-163` | 05 |
| K-047 | image_dmp 表头谎言（四列头两列行；行为兼容保留不修） | 事实 | 存量 | 05§2.5 | `dmp_kernel.c:168-185` | 05 |
| K-048 | kenv 取而不用（machine 历史冗余；Rust 不建模——与 K-042 联动） | 事实 | 存量 | 05§2.6/§3 D1 | `dmp_kernel.c:191-213` | 05 |
| K-049 | 三编码器位→字符（s_flags "PBDSIQM"/s_traps "SARBN"/p_rts "sSRIPTp"；A-12；Rust 宽度已 u32 化） | 机制 | 存量+修正 | 05§2.7/§3 D2 | `dmp_kernel.c:218-247,300-313`；`dump_kernel.rs:51,70,88` | 05 修正 |
| K-050 | privileges 匹配失败回退 USER_PRIV_ID + 位图列 %08x 分块 | 机制 | 存量 | 05§2.8 | `dmp_kernel.c:252-295` | 05 |
| K-051 | proctab 双架构分支（i386 真体/arm 空体；A-7 排除） | 机制 | 存量 | 05§2.9 | `dmp_kernel.c:318-353` | 05 |
| K-052 | procstack 多一行之谜（printf 后 pagelines++；Emit 即计数同构） | 机制 | 存量 | 05§2.10 | `dmp_kernel.c:358-380` | 05 |
| K-053 | proc_name 四规则（ANY/NONE/BOGUS/EMPTY/名） | 机制 | 存量 | 05§2.11 | `dmp_kernel.c:385-395` | 05 |
| K-054 | PM 域（单表双转储；flags_str "WZAETUFspxd" 11 位；`++n > 22` 候选断；`--more--\r` 回车分页；alarm 回绕减 + getticks 32 位 TODO 继承） | 机制 | 存量 | 06§2 | `dmp_pm.c` 全文；`getticks.c` | 06 |
| K-055 | VFS 域（双表双策略；fproc `pid<=0` 无例外跳过 + fd 计数 + FP_SESLDR/REVIVED + SDEV 端点 TODO 继承；dmap 稀疏一屏零游标） | 机制 | 存量 | 07§2 | `dmp_fs.c` 全文 | 07 |
| K-056 | RS 域（双表对齐拼行；短路或全有或全无；s_flags_str 双源同值异义 0x008=AUNCR 中的 N 与 C） | 机制 | 存量 | 08§2 | `dmp_rs.c` 全文 | 08 |
| K-057 | DS 域（128 槽盘点；四类型行；条件界游标；早返重放语义；STR 指针行 C bug——Rust 打标量字已偏离标注） | 机制 | 存量 | 09§2+V1 注记 | `dmp_ds.c` 全文 | 09 |
| K-058 | VM 域（三快照；print_region 折叠四等+跨表残留照录；双游标批机：首屏/容量预检/擦除行/内错防御；LINES=24） | 机制 | 存量 | 10§2 | `dmp_vm.c` 全文 | 10 |
| K-059 | 游标形状对照（05 先比较 / 06-07 候选断 / 09 条件界+早返 / 10 双游标）+ 五游标类型并存维持决策（V1-P3-5） | 概念 | 存量 | 09§2.3、10§2.4、todo V1-P3-5 | 各 dmp_*.c | 09/10 收口 |

**域 G：全局（99）**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|---|---|---|---|---|---|---|
| K-060 | 常量权威位置 9 族对账（VFS SI_PROC_TAB 本地副本已收敛 re-export） | 约束 | 存量 | 99§1、todo§1.2 | `minix-types::ipc::{tty,sysinfo,vm}` | 99 |
| K-061 | 排除项全集（glo.h 死 extern×5、DIAG_BUF_SIZE、_SYSTEM、click_to_round_k、arm 分支、Makefile、IS_PROC_NR 不存在） | 约束 | 存量 | 99§4、plan§5.4 | 各排除项 grep 证据 | 99 |
| K-062 | 跨服务引用与 IS 纯消费姿态 | 概念 | 存量 | 99§5 | 各对端 | 99 |
| K-063 | 测试基线与分布（115 = dump_kernel 23/dispatch 16/acquire 15/lib 12/tty_fkey 11/dump_pm 9/dump_vfs 8/dump_vm 6/dump_rs 5/dump_ds 5/sef 4/state 1；另有 minix-types tty 4 + sysinfo 3 归属本 stage 契约） | 测试性质 | **新增** | 各篇 §5.3 基线行过时（≤86；todo 记 106） | `cargo test -p minix-is` 实测 | 各篇 §5.3 修正 + 99 总账 |
| K-064 | edge 四条目状态（E-ISWIRE：minix-sef 已实装、IS main.rs 生产替换仍挂 A-6 裁决；E-ISPROD：kernel 半 ✅、PM/VFS/RS/DS/VM 五 producer 余项；E-ISKMESS open；E-ISBOOT open） | 工具与工程 | **新增** | 各篇无状态指针 | `edge_todo.md:560-634`；`main.rs:19-33` | 01/04/05 状态指针 + todo 双向指针 |
| K-065 | A-6 诊断通道定型（`SefTransport::diag_out() -> &mut dyn fmt::Write`；dump 体 write! 直写） | 架构演进 | 存量 | 01§3 D4 V1 注记、04 V1 注记 | `sef.rs`；各 render_* | 01/04（已有，保持） |
| K-066 | A-4 双侧对齐状态账（kernel 半闭环 vs 五 producer 余项；IS 侧 6 个 TODO(P1) 注释状态） | 架构演进 | **新增**（进度性事实） | 04~10 各篇"A-4 提案+对齐待办"表述已部分过时 | `dump_{pm,vfs,rs,ds,vm}.rs:12-13` TODO 现状；edge E-ISPROD 进度注记 | 04~10 修正注记 |

### 2.2 统计摘要

- 总条数 **66**：概念 6、机制 34、数据结构 2、接口与协议 3、约束 12、架构演进 6、事实 4、工具与工程 1、测试性质 1（跨类条目按主类型计）。
- 来源：存量 59、新增 7（K-041、K-042、K-063、K-064、K-066，另 K-049/K-050 含修正半）。
- 主讲述点重复：见 §3.3。

---

## 3. 覆盖审计

### 3.1 主题全集与来源

四路：① C 符号全集（8 .c 的 33 个函数定义 + 3 .h + 协议头 8 个 + libsys 客户端 4 个 + TTY 对端 fkey 面 + 服务侧 getsysinfo 两处）——plan §7.1/§7.2 已做全量 grep 对账，本次逐文件复核成立；② OS 通用概念（调试面外置、订阅-查收、分页转储、wire 契约）；③ 非 C 制品（清单三）；④ 边界契约（edge 四条目 + 6 个主权 stage 分工句）。

### 3.2 覆盖缺口表

| # | 缺口 | 证据 | 建议 | 处理 |
|---|---|---|---|---|
| G-1 | **04/05 篇未回写 E-ISPROD kernel 半**：04 §4.2 签名块仍写 `KinfoSnap/KProcSnap/BootImageSnap/KPrivSnap/IrqHookSnap`，实际已全部替换为 minix-types 的 `KinfoStruct/ProcInfoStruct/BootImageStruct/PrivInfoStruct/IrqHookStruct`（`acquire.rs:104-118`）；05 §4.2 仍写 `KProcSnap(10 字段)/KPrivSnap(s_flags i16)`，实际 `KProcSnap` 已删除（rg 零命中）、编码器入参 i16→u32（`dump_kernel.rs:51,70,88`）、PrivInfoStruct 宽度以 kernel 生产者为准（u32 flags/u64 ipc_to） | §0.2 命令输出；edge_todo E-ISPROD 两条 2026-09-16 进度注记 | 04/05 各加"E-ISPROD 收编轮"更新注记 + 签名块重写（对齐 minix-types 权威） | **采纳（本轮最大修正项）** |
| G-2 | **04 §4.3.1 与 §2.1 的 GET_MACHINE 矛盾**：§2.1 列 GET_MACHINE 为 C 侧 8 请求之一（正确，`dmp_kernel.c:201`），§4.3.1 却写"IS 永不发的请求（GET_KENV/GET_MACHINE）无方法可调"——把"C 发但 Rust 刻意不建模"误写成"C 也不发"。Rust 侧不取 machine 是外部行为不变的 deliberate deviation（`dump_kernel.rs:568` 注释自认），但按 rewrite 纪律应在 04/05 显式标注偏离，而非用错误措辞掩盖 | 两节对照；`com.h:327`；`dump_kernel.rs:568-577` | 04 §4.3.1 措辞修正（GET_KENV 才是两侧都不发；GET_MACHINE 是 C 发/Rust 不发的显式偏离）；05 §2.6 补偏离标注 | 采纳 |
| G-3 | **测试基线漂移链**：各篇 §5.3 基线为 2026-09-04 快照（32~86 递增计数），todo 记 106（2026-09-16），实测 **115**；且各模块归属计数需按 K-063 分布对账（如 07 篇 V1 注记 "+3" 后 §5.3 未更新） | §0.2 | 全篇 §5.3 基线行统一修至 115 + 模块分布表 | 采纳 |
| G-4 | **clippy 声称漂移**：todo §0 "clippy 本体 0 告警"已过时——实测 minix-is lib test 4 条（acquire.rs×3 测试代码 + dump_vfs.rs×1 未用变量），生产码 0 | §0.2 | todo 是台账（B 相修复轮顺带更新）；文档无此声称，无需改正文；4 条测试码告警登记为代码侧小卫生项 | 记录 |
| G-5 | **编辑缺陷四处**：① 06 §1 末"本章小结"块整段重复两次（:32-36）；② 10 §1 末同病（:31-35）；③ 02 :147 外文残留"evidence 见 §2.8"；④ 03 :152 "ничего没按"、:205 "formation 以下"外文残留；⑤ 06 §2.3 :74-76 乱句"'S'（ SIG？无——本表无 S"（编辑残迹） | 逐行核对 | 五处原位修删（文风门：禁外文插语；重复块删一处；乱句改写为"大写 S 本表空缺，05 的 RTS 表才有"） | 采纳 |
| G-6 | **00 主线图委托**：00 §2 标题"启动主线（plan §1.2）"，正文只有一行压缩链，完整 ASCII 时序图仍在 plan——正式总览应自带全图（与 06-stage-sched 00 骨架同型的"委托"缺陷，本篇程度轻：压缩链带锚点） | 00:17-21 对照 plan §1.2 | 00 契约：内嵌 plan §1.2 全图（带锚点），保留压缩叙述作导语 | 采纳（soft） |
| G-7 | **锚点卫生**：少量"（LNN，工具生成）"锚点符号名错误——如 02/03 篇 `dmp.c:NHOOKS（L44）`（NHOOKS 在 ：40，:44 起是 map_unmap_fkeys）、03 §2.5 `dmp.c:key_name（L118）`（:118 起是 mapping_dmp）；多数为"函数名+函数内行号"形态（可接受但宜规范化） | 逐条 sed 核对 | B 相按"文件:起-止行 + 真实符号"修正错符号者 | 采纳 |
| G-8 | **plan.md 状态过时**（记录不改）：§6.1 表 00=skipped/99=pending，实际两篇已于 2026-09-15 成文；§3.5 "stub 0 测试"，实际 115；§2 表 02 行 Rust 侧"缺 TtyFkeyCtlReq/Reply（A-1）"已兑现 | plan §6.1/§3.5/§2 对照现状 | plan 属参考材料；B 相交付说明记录对账结论 | 记录 |
| G-9 | **edge 状态指针缺失**：01/04/05 篇描述的接线/E-ISPROD 状态已变（minix-sef 实装、kernel 半闭环、IS main.rs 仍占位），正文无指针，读者无法区分"未做/做了/做了一半" | edge_todo:560-634 对照正文 | 01/04/05 各加一行状态指针（K-064），06~10 的 TODO(P1) 注释状态随 K-066 注记说明 | 采纳 |
| G-10 | 99 篇 §1 常量表的 IS 本地常量行（NOTIFY_MESSAGE/SIGTERM 行号）与代码对账：`dispatch.rs:20/24`、`sef.rs:16`——实测行号有漂移（代码经多轮修复后行号移动），99 表宜改为"模块级归属"不带行号，或按现树复核 | `grep -n NOTIFY_MESSAGE os/servers/is/src/dispatch.rs` 等 | 99 §1 行号列复核或改模块级 | 采纳（轻） |

### 3.3 重复主题表（主讲述点裁决）

| 主题 | 出现位置 | 主讲述点 | 其余处理 |
|---|---|---|---|
| 双层执行模型/单线程声明 | 00§4、01、99§3 | 99 | 00 一段，01 只在模块文档句 |
| panic/warn 分界 | 01§3 D5、04§2.6 | 04 | 01 留分界一句 |
| 注册集合=能力集合（hooks 唯一键源） | 02§2.6、03§2.1 | 03（表的本体在 03） | 02 引用 |
| 拉模式/notify 无载荷 | 01§1.2、02§1.1、03§2.3① | 02 | 01/03 引用 |
| 分页 22 行惯例 | 05§2.1、06/07/09/10 各域 | 05（PROCLOOP 本体） | 各域讲差异（\r、候选断、条件界、24 行） |
| 三层编号/bit0 | 02§1.3/§2.3、03§2.2 | 02 | 03 引用 |
| A-4 快照契约 | 04§3 D1、05~10 各篇 | 04（总纲） | 各篇只讲本域布局与对齐待办现状 |
| EDONTREPLY | 01§2.5、03§2.3④ | 01 | 03 引用 |
| 游标形状对照 | 09§2.3、10§2.4 | 10（收官表含全部四形） | 09 保留三形表，10 收口 |

### 3.4 越界主题表

| 越界描述 | 所在 | 裁决 |
|---|---|---|
| 02 篇讲 TTY 侧 do_fkey_ctl/func_key 实现（keyboard.c 约 150 行语义） | 02§2.7-2.8 | 合规边界：fkey 协议对方是 IS 行为的决定性契约（plan D-4 裁决），且"TTY 实现细节归 TTY crate"边界已声明；保持 |
| 04 篇引 PM/VFS getsysinfo 服务侧行号 | 04§2.4 | 合规：契约面（size 精确匹配/root 门）必须有对方证据；服务实现归各 stage；保持 |
| 06~10 各篇引对方服务布局头文件字段行号 | 各域 §2.4 | 合规：A-4 wire 契约提案的本体；对齐裁决归 edge E-ISPROD；保持并按 G-9 更新状态 |
| 06 篇 getticks 的"不超前解决 64 位 TODO" | 06§2.2/§3 D4 | 合规：显式继承 C TODO，越俎代庖警告在位；保持 |

### 3.5 非 C 主题逐项回答

| 主题 | 在哪讲 / 为什么不在本 stage |
|---|---|
| 链接与加载 | RS 动态加载语义归 03-stage-rs/01-stage-kernel；本 stage 只持存在条件证据（00§2-3、01§2.11） |
| 镜像与内存布局 | .usermapped 移除 = A-3 已文档化（04§2.3/05§2.2 + E-ISKMESS）；IS 自身无自定义段 |
| 汇编入口与陷阱进入 | trap 层归 14-stage-runtime（edge E1/E-ISWIRE）；本文档止步三缝（SefTransport/FkeyCtlTransport/Acquires） |
| 引导链与引导协议 | 无 boot_image = 本 stage 的核心语义之一（00§3/01§2.11），协议本体归 01-stage-kernel/09 |
| 启动装配 | SEF 四注册（01）；Rust main.rs 装配 + test 门（01§4） |
| 构建与工具链 | Makefile WONTFIX（plan §5.4） |
| 跨模块接口与线格式 | 本 stage 主场：fkey 两消息（02）、GET/SI/VM_INFO 消息（04）、六域布局 ABI（05~10，kernel 半已上收 minix-types——G-1 回写） |
| 错误路径 | 三层错误面（04§2.6）+ EDONTREPLY（01/99）+ 排除项（99§4） |
| 关闭与退出 | SIGTERM→unmap→exit（01§2.8） |
| 并发与同步 | 单线程事件循环 + 零分配 + 五游标实例隔离（00§4/99§3） |
| 测试基建 | 115 内联测试 + fake 接缝（各篇 §5 + 99 总账）；真机联调归 E-ISBOOT/E5 |

---

## 4. 新目录

### 4.1 新篇章总表（编号不变，11 篇）

> **裁决**：现有目录已经是"总览→启动→协议→分派→数据面→六转储域→收口"的成熟结构，与四条硬标准完全吻合（§9 G3/G4）。重建工作 = 定向修正（G-1~G-10）+ 00 主线图内嵌。**不重排、不重编号、不拆分、不合并、不新建篇章。**

| 编号 | 标题 | 一句话定位 | 分组 | 操作 |
|---|---|---|---|---|
| 00 | IS 整体架构概览 | 是什么/何时存在/怎么读 | 阶段 0 总览 | 保持+补主线图（G-6） |
| 01 | 启动入口与主循环骨架 | 出生、等活、分类、回复门 | 阶段 1 启动入口 | 保持+修正（G-3/G-7/G-9） |
| 02 | 功能键观察者协议 | 三命令+两消息+三层编号+TTY 契约 | 阶段 2 协议面 | 保持+修正（G-5/G-7） |
| 03 | 转储分派 | hooks 表+pressed+次主线路径图 | 阶段 3 分派 | 保持+修正（G-5/G-7） |
| 04 | 数据获取五通道 | 五主权五通道+类型化出参缝 | 阶段 4 数据面 | 保持+修正（**G-1/G-2**/G-3/G-9） |
| 05 | 内核转储域 | 8 席+6 布局 ABI+分页契约 | 阶段 5 转储域 | 保持+修正（**G-1**/G-2/G-3/G-9） |
| 06 | PM 转储域 | 单表双转储+时钟面+\r 分页 | 阶段 5 | 保持+修正（G-3/G-5） |
| 07 | VFS 转储域 | 双表双策略+FP 位 | 阶段 5 | 保持+修正（G-3/G-9） |
| 08 | RS 转储域 | 双表对齐+双源编码 | 阶段 5 | 保持+修正（G-3/G-9） |
| 09 | DS 转储域 | 盘点+四类型+条件界游标 | 阶段 5 | 保持+修正（G-3/G-9） |
| 10 | VM 转储域 | 折叠状态机+双游标批机 | 阶段 5 | 保持+修正（G-3/G-5/G-9） |
| 99 | 全局概念收口 | 常量权威/错误码/执行模型/排除项 | 全局查询 | 保持+修正（G-3/G-10） |

### 4.2 阅读路径

- **主线（启动+次主线双线合一，全读）**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 99。一次 F-key 按压的旅程（03 §2.6 路径图）是次主线，读完 05~10 后回头看 03 §2.6 即可闭环。
- **可跳读**：只关心某一转储域的读者可从 00 → 04（该域通道行）→ 该域篇直达；99 随时跳入查询。
- **并行体声明**：六个转储域是并行体，统一框架已由 03（分派）+04（数据面）给出，组内 05 为代表成员（九通道直连、最复杂），06~10 按"单通道+对方布局"收敛体例——现状已是此形态，保持。

---

## 5. 每篇契约

> B 相执行说明：本 stage 无"全文新建"篇；11 篇全部为【保持+修正】型契约——**不重写正文**，只执行各自修正清单（每条修完跑 fix-guard 验证）。事实底线列出修正时必须复核的锚点集。

### 00-is-overview【保持+修正】

- 定位（不变）：是什么/何时存在/怎么读。
- 讲什么：K-001~K-004。
- 不讲什么（不变）：一切机制（01~10）。
- 前置：无。后置：全部。
- 事实底线：`main.c:1-8`；`rc.minix:115-118`；`system.conf:269-277`；`kernel/table.c` boot_image 17 项。
- 修正清单：
  1. **G-6**：§2 内嵌完整启动时序图（以 plan §1.2 的 ASCII 图为底，逐锚点自带：rc.minix:117 → RS 加载 → main.c:31 → sef_local_startup :75-89 → sef_cb_init_fresh :94-102 → map_unmap_fkeys → 主循环 :44-69），保留现有压缩叙述作导语；删除对 plan §1.2 的委托性指引（plan 保留在 §7 参见）。
  2. §7 参见补一行 edge 状态指针：E-ISBOOT（启动链实施面 open）。
- 验收标准：主线图六步全部带锚点且 `sed` 可复核；正文中不再存在"机制图见 plan"式委托句。

### 01-is-init-main【保持+修正】

- 讲什么：K-005~K-018。前置：00。后置：02、03。
- 事实底线：`main.c` 全文；`sef.h`；`com.h:64,90-93`；`sys/sys/errno.h:199`；`sef.rs`/`state.rs`/`dispatch.rs`/`lib.rs`。
- 修正清单：
  1. **G-3**：§5.3 基线行更新——"41 passed（2026-09-04）"后追加当前基线："**115 passed（2026-09-19 实测）**；本篇直接相关：state 1 + dispatch 16（01 上半分类器约 8 个）+ sef 4 + lib 12 + tty_fkey 11 中的编排断言（具体归属以 `rg "#\[test\]"` + 测试名对账为准）"；
  2. **G-9**：§3 D2"传输侧属 minix-sef/minix-sys，当前均为 stub"一句追加现状指针——minix-sef 已实装（sef_receive_status 含 ping 拦截，E-ISWIRE 进度 2026-09-15）；IS main.rs 生产替换仍 fail-closed 占位（`main.rs:19-33`），挂 A-6 裁决（E-ISWIRE 剩余面）；
  3. **G-7**：§头部与正文的"（LNN，工具生成）"锚点符号名复核（`libsys/sef.c:sef_receive_status（L208）` 等按现文件行号复核）。
- 验收标准：三条修正落位；T1~T14 测试表与 sef.rs/dispatch.rs 现签名一致。

### 02-is-fkey-contract【保持+修正】

- 讲什么：K-019~K-026。前置：01。后置：03。
- 事实底线：`com.h:872-877`；`ipc.h:1447-1454,1925-1931`；`keymap.h`；`fkey_ctl.c`；`keyboard.c:429-585`；`minix-types/src/ipc/tty.rs`；`tty_fkey.rs`。
- 修正清单：
  1. **G-5**：§1.3 表后正文 ":147 evidence 见 §2.8" → "证明见 §2.8"（外文残留，文风门）；
  2. **G-7**：§2.3/§2.6 的 `dmp.c:map_unmap_fkeys（L50/L55/L44，工具生成）` 系列锚点规范为 `dmp.c:45-65`（函数体）+ `dmp.c:53-58`（组位循环）——NHOOKS 在 :40，现锚点符号名错；
  3. **G-3**：§5.3 基线更新（32 → 当前；minix-types tty.rs 4 个布局测试归属声明保持）。
- 验收标准：三命令/两消息/三层编号锚点不变；外文残留清零；`rg -n "evidence" 02-is-fkey-contract.md` 零命中。

### 03-is-dump-dispatch【保持+修正】

- 讲什么：K-027~K-033。前置：01、02。后置：04。
- 事实底线：`dmp.c:14-40,70-132`；`dispatch.rs`；`lib.rs`。
- 修正清单：
  1. **G-5**：§2.3② "ничего没按" → "什么都没按"；§2.6 "formation 以下" → 删该残词（改"以下是下篇"）；
  2. **G-7**：§2.5 标题锚 `dmp.c:key_name（L118，工具生成）` → `dmp.c:118-132（mapping_dmp）`（符号名错：key_name 在 :103-114）；§4.3 的 `dmp.c:hook_entry（L18）` → `dmp.c:14-35（hooks[] 表）`；
  3. **G-3**：§5.3 基线更新（41 → 当前基线 + dispatch.rs 16 个测试归属声明）。
- 验收标准：次主线路径图锚点不变；外文残留清零（`rg -n "ничего|formation" 03-is-dump-dispatch.md` 零命中）。

### 04-is-data-acquisition【保持+修正】（修正量并列最大）

- 讲什么：K-034~K-042。前置：01 + 各主权 stage。后置：05~10。
- 事实底线：`com.h:205,236,252,316-345,412-415,476,507,729-734`；`sysinfo.h:11-17`；`callnr.h:60,120`；`libsys/{getsysinfo,vm_info,sys_diagctl,sys_getinfo}.c`；`acquire.rs`；`minix-types/src/types/{proc_info,priv_info,kinfo,boot_image,irq_hook}.rs`。
- 修正清单：
  1. **G-1（核心）**：§4.2 签名块重写——五个取数方法的出参类型从 `*Snap` 改为 minix-types 权威类型（`get_kinfo(&mut KinfoStruct)`、`get_image(&mut [BootImageStruct])`、`get_proctab(&mut [ProcInfoStruct])`、`get_irqhooks(&mut [IrqHookStruct])`、`get_privtab(&mut [PrivInfoStruct])`，对齐 `acquire.rs:104-118` 实况）；新增"E-ISPROD 收编轮更新（2026-09-16）"注记：快照权威上收方案 A（commits 7d89ec114/987773bb3）、IS 侧五个 Snap 删除改 import、编码器宽度 i16→u32、VFS 本地 SI_* 常量收敛 re-export；PM/VFS/RS/DS/VM 五 producer 仍为余项（E-ISPROD open）；
  2. **G-2**：§4.3 不变量 1 措辞修正——"IS 永不发的请求（GET_KENV/GET_MACHINE）无方法可调"改为："GET_KENV 两侧均从不发（C 的 kenv_dmp 读 kinfo+machine，从不调 sys_getkenv）；GET_MACHINE 是 **C 发、Rust 不发**的显式偏离：C 的 kenv_dmp 取 machine 但全程不读（`dmp_kernel.c:201`，05§2.6 历史冗余），Rust 不建模故无 `get_machine` 方法（`dump_kernel.rs:568` 注释）——外部行为不变（该调用无副作用），偏离 deliberate，标注于此"；
  3. **G-9**：§2.2 两侧现状段补状态指针：minix-rs 内核 DIAGCTL 仍 ENOSYS（32-stack-tracing forward ref 不变）；kerninfo 通道仍 fail-closed（E-ISKMESS open）；
  4. **G-3**：§5.3 基线更新（47 → 当前；acquire.rs 15 个测试归属）。
- 验收标准：§4.2 每个 trait 方法签名与 `acquire.rs` 逐行一致；G-2 修正句两处 C/Rust 锚点齐备；`rg -n "KProcSnap|KinfoSnap|BootImageSnap|KPrivSnap|IrqHookSnap" 04-is-data-acquisition.md` 修后零命中（或仅存于"演进史"注记内并注明已删）。

### 05-is-dump-kernel【保持+修正】（修正量并列最大）

- 讲什么：K-043~K-053。前置：03、04。后置：06。
- 事实底线：`dmp_kernel.c` 全文；`kernel/proc.h`/`priv.h`；`minix-types/src/types/*` 五结构；`dump_kernel.rs`。
- 修正清单：
  1. **G-1（核心）**：§2.1 三表段、§2.7 三编码器段、§3 D1 快照段、§4.2 签名块按收编后实况回写——快照名改 `ProcInfoStruct`/`PrivInfoStruct`/`BootImageStruct`/`KinfoStruct`/`IrqHookStruct`（minix-types 权威，含布局见证测试），仅 `KmessagesSnap` 仍为本 crate 定义；编码器签名改 `s_flags_str(u32) -> [u8;8]`/`s_traps_str(u32) -> [u8;6]`/`p_rts_flags_str(u32) -> [u8;8]`（`dump_kernel.rs:51,70,88`）；§2.7 的"SENDA 负掩码边角"表述补一句宽度演进说明（C 侧仍是 short 整型提升边角；Rust 侧入参已 u32 化，宽度以 kernel 生产者为准——edge E-ISPROD 进度注记原文）；
  2. **G-2**：§2.6 kenv 段补偏离标注："Rust 不取 machine（`dump_kernel.rs:568` 注释；04 §4.3 偏离声明）——C 发 GET_MACHINE 而不读，Rust 连取数一起省略，外部行为不变"；
  3. **G-3**：§5.3 基线更新（59 → 当前；dump_kernel.rs 23 个测试归属声明——本 stage 最大测试模块）；
  4. **G-9**：§1.3/§3 D1 的 "A-4 对齐待办" 表述收窄为 "kernel 半已闭环（E-ISPROD 2026-09-16）；本域无余项"。
- 验收标准：§4.2 无任何已删除类型名（`rg -n "KProcSnap|KPrivSnap" 05-is-dump-kernel.md` 修后仅存于演进史注记）；三编码器签名与代码逐字一致；G-2 标注在位。

### 06-is-dump-pm【保持+修正】

- 讲什么：K-054。前置：04、05。后置：07。
- 事实底线：`dmp_pm.c` 全文；`mproc.h`；`getticks.c`；`dump_pm.rs`。
- 修正清单：
  1. **G-5**：§1 末重复的"本章小结"块删除一处（:32-36 两段相同）；§2.3 乱句 "'S'（ SIG？无——本表无 S" 改写为 "大写 S 本表空缺（05 的 RTS 表才有 'S'）"；
  2. **G-3**：§5.3 基线更新（66 → 当前；dump_pm.rs 9 个测试归属）；
  3. **G-9**：§2.4/§3 D1 的 A-4 待办补状态：PM producer 对账仍 open（E-ISPROD 余项），`dump_pm.rs:13` 的 TODO(P1) 注释仍在位。
- 验收标准：重复块与乱句清零；MProcSnap 签名与 `dump_pm.rs:22` 一致（本域 Snap 未上收，保持）。

### 07-is-dump-vfs【保持+修正】

- 讲什么：K-055。前置：04、06。后置：08。
- 修正清单：
  1. **G-3**：§5.3 基线更新（72 → 当前；dump_vfs.rs 8 个测试归属）；
  2. **G-9**：A-4 待办补状态（VFS producer open；VFS misc.rs 的 SI_* 本地副本已收敛 re-export——E-ISPROD 2026-09-16 进度注记，07 篇如引用该事实需同步）。
- 验收标准：FProcSnap/DmapSnap 与 `dump_vfs.rs:184` 等一致；VfsAction 三态契约表述与 V1 注记一致。

### 08-is-dump-rs【保持+修正】

- 讲什么：K-056。前置：04、07。后置：09。
- 修正清单：
  1. **G-3**：§5.3 基线更新（76 → 当前；dump_rs.rs 5 个测试归属）；
  2. **G-9**：A-4 待办补状态（RS producer open；`dump_rs.rs:13` TODO 在位）。
- 验收标准：双源编码/短路语义锚点不变。

### 09-is-dump-ds【保持+修正】

- 讲什么：K-057、K-059（三形表）。前置：04、07、08。后置：10。
- 修正清单：
  1. **G-3**：§5.3 基线更新（80 → 当前；dump_ds.rs 5 个测试归属）；
  2. **G-9**：A-4 待办补状态（DS 生产侧已有模块 `os/servers/ds/src/lib.rs:70`，对账归 E-ISPROD；`dump_ds.rs:13` TODO 现状复核）。
- 验收标准：早返重放与 STR 偏离标注不变。

### 10-is-dump-vm【保持+修正】

- 讲什么：K-058、K-059（收官表）。前置：04、09。后置：99。
- 修正清单：
  1. **G-5**：§1 末重复的"本章小结"块删除一处（:31-35）；
  2. **G-3**：§5.3 基线更新（86 → 当前 115；本篇 §6 "阶段完成"段的遗留缺口清单补一句 E-ISPROD kernel 半已闭环）；
  3. **G-9**：A-4 待办补状态（VM producer open；`dump_vm.rs:12` TODO 在位）。
- 验收标准：折叠/双游标锚点不变；收官段状态与现实一致。

### 99-is-global-concepts【保持+修正】

- 讲什么：K-003/K-005/K-015/K-060~K-063（含新增总账）。
- 修正清单：
  1. **G-3**：补测试总账行："minix-is 115 passed（2026-09-19 实测；分布见各篇 §5.3），另有 minix-types tty.rs 4 + sysinfo.rs 3 个布局/码值测试归本 stage 契约"；
  2. **G-10**：§1 常量表的 IS 本地常量行去掉行号或按现树复核（`dispatch.rs`/`sef.rs` 行号经多轮修复已漂移），改"模块级归属"表述；
  3. **G-9**：§5 跨服务引用表补一行 edge 状态汇总（E-ISWIRE 剩余 IS main.rs 生产替换；E-ISPROD kernel 半 ✅ 五 producer 余项；E-ISKMESS/E-ISBOOT open）。
- 验收标准：常量表与 `minix-types` 现树对账；总账行与实测一致。

---

## 6. 变更表

| 操作 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向/来源 |
|---|---|---|---|---|---|---|
| N-1 | 修正 | 00§2 | 原位内嵌全图 | G-6 去委托 | K-002 | plan §1.2 图为底稿（参考材料） |
| N-2 | 修正 | 01§3 D2、§5.3 | 原位 | G-3/G-9 | K-063/K-064 | minix-sef 实装事实 + 115 基线 |
| N-3 | 修正 | 02§1.3、锚点、§5.3 | 原位 | G-5/G-7/G-3 | K-021 | — |
| N-4 | 修正 | 03§2.3/§2.6/锚点、§5.3 | 原位 | G-5/G-7/G-3 | K-029/K-063 | — |
| N-5 | 修正 | 04§4.2/§4.3、新增注记、§5.3 | 原位 | **G-1/G-2**/G-3/G-9 | **K-041/K-042**/K-063 | minix-types 五 Struct + edge E-ISPROD 进度注记 |
| N-6 | 修正 | 05§2.1/§2.6/§2.7/§3 D1/§4.2/§5.3 | 原位 | **G-1/G-2**/G-3/G-9 | **K-041/K-042**/K-049/K-063 | 同上 + `dump_kernel.rs:51,70,88,568` |
| N-7 | 修正 | 06§1/§2.3/§5.3 | 原位 | G-5/G-3/G-9 | K-054/K-063 | — |
| N-8 | 修正 | 07§5.3+状态 | 原位 | G-3/G-9 | K-063/K-066 | — |
| N-9 | 修正 | 08§5.3+状态 | 原位 | G-3/G-9 | K-063/K-066 | — |
| N-10 | 修正 | 09§5.3+状态 | 原位 | G-3/G-9 | K-063/K-066 | — |
| N-11 | 修正 | 10§1/§5.3/§6 | 原位 | G-5/G-3/G-9 | K-058/K-063 | — |
| N-12 | 修正 | 99§1/§5 | 原位 | G-3/G-9/G-10 | K-063/K-064 | — |
| N-13 | 对账（不改） | plan.md §6.1/§3.5/§2 | — | G-8：状态表/基线/A-1 状态过时；plan 属参考材料 | — | B 相交付说明记录 |
| N-14 | 代码侧登记（非本文档） | os/servers/is 测试码 | — | G-4：4 条测试码 clippy（acquire×3 + dump_vfs×1） | — | 建议随下一轮代码修复批清理；不属文档重建 |

**未执行的操作类型**：重排/拆分/合并/新建/归档均 0 处。draft/ 与 plan/todo 维持现状（G-8 记录不改）。

---

## 7. 缺漏新篇（非 C 主题逐项落实）

> §3.5 已逐项回答；收口为裁决表，无"待定"项。本 stage **无需新建篇章**——所有非 C 主题已有归属。

| 主题 | 裁决 | 承载位置 | 验收 |
|---|---|---|---|
| 链接与加载/镜像布局 | 不在本 stage（RS 加载语义 + A-3 已文档化） | 00§2-3、04§2.3 | 已有 |
| 汇编入口与陷阱进入 | 不在本 stage（三缝为止） | 01§3 D2、04§3 D2 | 已有 |
| 引导链与引导协议 | 存在条件证据在本 stage，协议本体不在 | 00§3、01§2.11 | 已有 |
| 启动装配 | 在本 stage | 01 | 已有 |
| 构建与工具链 | WONTFIX | plan §5.4 | 不新增 |
| 跨模块接口与线格式 | 在本 stage（主场） | 02/04/05~10 | 已有 + G-1 回写 |
| 错误路径 | 在本 stage | 04§2.6、01、99 | 已有 |
| 关闭与退出 | 在本 stage | 01§2.8 | 已有 |
| 并发与同步 | 在本 stage（单线程+零分配+游标隔离） | 00§4、99§3 | 已有 |
| 测试基建 | 在本 stage（内联+fake）；真机归 E-ISBOOT/E5 | 各篇 §5 + 99 | 99 补总账（G-3） |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表

> 编号不变 ⇒ 篇间锚点零迁移。变化全部为原位修正：

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 断链风险 |
|---|---|---|---|---|
| 04§4.2、05§2.1/§2.7/§3 D1/§4.2 | `*Snap` 快照名与 i16 宽度签名 | 原位改写为 minix-types `*Struct` + u32 签名 + 演进注记 | 改写（G-1） | 无节级外部引用（外部引用均为目录级/篇级）；正文内 05~10 交叉引用为篇级 |
| 04§4.3.1 | "IS 永不发 GET_MACHINE" 错误措辞 | 原位改写为 C 发/Rust 不发的偏离声明 | 改写（G-2） | 无 |
| 00§2 | 压缩主线链+委托句 | 原位扩为全图 | 改写（G-6） | 无 |
| 06§1、10§1 | 重复小结块 | 删一处 | 删除 | 无 |
| 02:147、03:152/:205、06§2.3 | 外文残留/乱句 | 原位改写 | 改写（G-5） | 无 |
| 各篇 §5.3 基线行 | 32~86 旧快照 | 115+分布 | 原样替换（G-3） | 无 |
| 少量"（LNN，工具生成）"错符号锚点 | NHOOKS→:44 等 | 真实符号+行段 | 原样替换（G-7） | 无（本就解析失败） |

### 8.2 引用迁移表

| 引用方 | 旧引用 | 新目标 | 验证方式 |
|---|---|---|---|
| `edge_todo.md`（9 处） | `08-stage-is/todo.md §…`、篇名 | 不变 | `rg -c '08-stage-is' edge_todo.md` 前后一致 |
| `03-stage-rs/` 三个文件（14 处） | 目录级 | 不变 | 目录名不动 |
| `00-master-plan/README.md`、`edge3.md`、`04-stage-pm/04-ipc-dispatch.md` 等 | 目录级 | 不变 | 同上 |
| `os/servers/is/src/main.rs:4` | `08-stage-is/01-is-init-main.md` | 不变 | 文件名不动 |
| 篇内互引 ≈53 处 | `NN-*.md` | 不变 | 重建后 `rg -c` 复跑计数不减 |
| 本蓝图（`06-stage-sched/doc_rerank_glm.md` 曾 2 处引用本目录） | 目录级 | 不变 | 同上 |

### 8.3 断链成本摘要

- **本蓝图方案（保编号）**：受影响外部引用 **0**；代码注释 **0**；篇间互引 **0**。
- **反事实（重编号/重排）**：篇内互引 ≈53 + 外部 16 文件 ≈37 处 + 代码注释 1 处 ≈ **91 处**；热点 03（13 次被引）、04（10）、02（6）。规模虽小于 sched，但收益同样为零（现结构已满足硬标准）——维持不动的裁决不变。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**：00 无前置；01→00；02→01；03→01,02；04→01+各主权 stage；05→03,04；06→04,05；07→04,06；08→04,07；09→04,07,08；10→04,09；99→全部。全部指向更早编号或跨 stage 主权文档，**通过**。
2. **依赖图检查**：链式偏序 00→01→02→03→04→{05…10 按序}→99，无环，**通过**。
3. **覆盖率检查**：§2 池 66 条全部有去向（59 条原篇保留/修正、7 条新增注入 04/05/99 与状态指针）；删除项 0；新增 7 条全部带 C/Rust/edge 锚点，**通过**。
4. **断链成本**：§8.3——方案内 0 断链，反事实 ≈91 处已列热点，**通过**。

### 9.2 自检门逐门结果

| 门 | 结果 | 证据 |
|---|---|---|
| G1 C 真序逐条可核对 | **通过** | 抽 10 条：A1（rc.minix:115-118 sed）、A2（system.conf:269-277）、B2（com.h:90-93 grep）、B3（main.c:48-57 通读）、B4（errno.h:199 grep）、C1-C2（dmp.c:73-98 通读）、D1 的 GET_MACHINE :327/:201（grep+sed）、D4 的 callnr.h:60,120（grep）、C3 的 LINES 22/24（dmp_kernel.c:18、dmp_vm.c:9）、E1（main.c:107-116）——全部与源码一致 |
| G2 知识点池完整 | **通过** | C 侧 33 函数 + 3 头文件 + 协议头 8 个 + libsys 4 客户端 + TTY 对端 + 服务侧 2 处全部入池或显式排除（99§4 排除表 7 项，grep 证据在 plan §5.4 本次抽验成立）；非 C 制品 10 类逐项有归属 |
| G3 前向引用为零 | **通过** | §9.1 第 1 条 |
| G4 依赖图无环 | **通过** | §9.1 第 2 条 |
| G5 覆盖率 100% | **通过** | §9.1 第 3 条；明确删除项 0 |
| G6 拆合去向/新建来源 | **通过** | 拆分/合并/新建均 0；13 条修正操作全部写明事实源（minix-types 五结构文件、edge 进度注记、实测基线） |
| G7 契约七要素齐全 | **通过** | 11 份契约均为保持型，含定位/讲什么/不讲什么/前置/后置/事实底线/修正清单+验收标准 |
| G8 迁移表覆盖 | **通过** | §8.1 覆盖全部变化节；§8.2 覆盖文档间与代码注释引用（全部 0 迁移） |
| G9 事实断言有锚点 | **通过** | 抽 10 条：115 基线（cargo 实跑）、E-ISPROD 落地（git log 7d89ec114 + rg KProcSnap 零命中 + acquire.rs:104-118 通读）、编码器 u32（dump_kernel.rs:51,70,88 grep）、GET_MACHINE 处置（com.h:327 + dmp_kernel.c:201 + dump_kernel.rs:568-577 sed）、EDONTREPLY 203（errno.h:199 grep）、TTY_PROC_NR=5（com.h:64 grep）、is_notify 区间（com.h:93 grep）、SI_* 7 值（sysinfo.h:11-17 grep）、minix-sef 实装与 IS 占位并存（main.rs:19-33 sed + edge_todo:560-582）、重复小结块（06:32-36/10:31-35 通读）。推测项：无；待复核项（11§1.6 外链类）本 stage 无 |

### 9.3 结论与待用户裁决的问题

**结论**：本蓝图已完成，可交付 B 相：11 篇全部保持+定向修正（合计 28 条修正项，全部带锚点与验收标准），0 篇重写，0 处结构变动。核心工作量集中在 04/05 两篇的 E-ISPROD 收编回写（G-1）与一处矛盾修正（G-2）。

**待用户裁决**：

1. **"保编号、修漂移、补薄点"方案是否采纳**（glm 建议采纳）：本 stage 是"文档先行、代码追赶"的成功样本（todo §0 原话），11 篇全部新鲜成稿且经 V1 轮审查；唯一实质漂移（G-1/G-2）是代码侧 2026-09-16 两笔提交跑在文档前面，属定向回写而非重建问题。若汇总轮仍裁定全量重写，本蓝图知识点池与契约可直接作任务书。
2. **G-2 的偏离登记级别**：GET_MACHINE 的"Rust 不取不发"目前只有代码注释（dump_kernel.rs:568），glm 建议按 P3-design-deviation 在 04/05 显式标注（外部行为不变、无对端影响，但按 rewrite 纪律偏离必须三处可见——目前只有代码一处）。
3. **G-4 的 4 条测试码 clippy**：不属文档重建范围，glm 建议登记为 stage 内小卫生项（随下一轮代码修复批清理），不计入 B 相文档工作。
4. **plan.md 是否需要一次状态刷新**（G-8）：glm 建议维持"参考材料不改"的既有惯例（与 06-stage-sched G-11 同一裁决），由 B 相交付说明记录对账。

---

*（蓝图完。执行者 glm，2026-09-19，基线 commit d6ecd22ca。本文件是 08-stage-is 目录内唯一的 `_glm` 产物。）*
