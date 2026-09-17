# 09-stage-init Rust 实现架构级 Review TODO

> 来源：2026-09-17 第一轮（code-excellence 扫描：查漏补缺先行 + 分层设计审视；对照 Redox init 与 Rust 社区 nix 的 WaitStatus 设计）。
> 范围：一等对象 `os/commands/sbin/init/src/` 全部 Rust 代码（15 文件 2025 行，crate 名 `minix-init`，89 个单元测试）；对照 ground truth `minix3/sbin/init/init.c`（1902 行，42 函数 + main）；发现的所有跨 stage 依赖缺口按 edge 判定规则登记 `../edge_todo.md`，本文档只留双向指针。
> 定位：不复写 plan.md；跨 stage 条目唯一入口是 `../edge_todo.md`。
> 状态（2026-09-17 首轮）：**扫描轮，未修任何代码**。P0×10 全部开口，P1×8 全部开口，P2×7 全部开口。本 stage 的可修复项必须在 minix-sys 客户端面（edge E-INITSYS）落地后才能真正闭环，修复顺序建议：P0-9/P0-10 两个保真缺陷可立即修，其余跟随 E-INITSYS。

---

## 0. 结论速览

| 轮次 | 产出 | 结果 |
|------|------|------|
| 第一轮（2026-09-17） | P0×10（运行时缺失 + 保真缺陷）+ P1×8（架构分层）+ P2×7（打磨）+ edge E-INITSYS 登记 | 扫描轮未修，基线如下 |

验证基线（2026-09-17 实测；后续修复轮以此为对照）：

- `cargo test -p minix-init`：**89 passed / 0 failed**
- `cargo clippy -p minix-init`：本体 **82 条告警**（死代码族为主：7 个 trait、10 余个 struct/function "never used/constructed"；依赖 crate 另有 minix-types 1 / minix-sys 3 / minix-rt 6 条，归 edge）
- `tools/design-coverage-check.sh fork-syscall-rewrite --stage 09-stage-init`：14/16 PASS；`00-init-overview` 与 `99-init-global-concepts` 两篇缺 outline/outline-review/design 快照（H.1+H.6 FAIL，模式 69 预警）。两篇正文本身就是 pending 骨架，属已知状态，但按流程需在下次 review 前 Step 0.3 生成快照（P2-5 登记）
- Gate E：本文档全部锚点为 2026-09-17 逐条 grep 实证

**核心判断**：这个 crate 不是 translate 问题——纯逻辑部分质量良好（C 全局变量全部改成值传递、文档锚点纪律好、忠实排除了 `#if 0` 死代码）。真正的问题是**纯逻辑骨架先行，运行时一行都没有接**：`main()` 做完参数解析、宿主文件系统探测、入口决策之后直接 `std::thread::park()` 永久停泊（`main.rs:52-54`），全 crate 没有一处 fork/exec/waitpid/kill/信号的系统调用。82 条 clippy 死代码告警就是量化证据：全部 7 个 seam trait 在二进制里"never used"。因此本轮的主线不是继续细化分类器，而是 **L0 接线**：先把 edge E-INITSYS（minix-sys 信号/进程控制客户端面）补齐，再按 §1 P1-1/P1-3 的定型方案把状态机接成真的。

**对照参照（本轮联网核实）**：Redox 的 init（gitlab.redox-os.org/redox-os/init，src/main.rs）走的是另一极——极薄配置解释器（`/etc/init.rc` 逐行解释，内建 cd/export/run/run.d/stdio）加一个裸 `waitpid(0, &mut status, 0)` 回收循环，无信号处理、无监督重启、无 trait 接缝，直接调 libredox。它印证两点：其一，pid1 的运行模型本来就该是"阻塞 waitpid 循环"而不是事件分发器（本 stage P1-1 方案 A 的旁证）；其二，接缝 trait 不是必需品，接缝的价值在"能测"而非"抽象"（本 stage P1-3 的裁剪依据）。Minix3 的 init 是 4.4BSD/NetBSD 血统的完整 tty 会话监督者（pathnames.h 头部即 `$NetBSD` 标识），外部行为必须按 Rewrite 边界保持，所以 Redox 的"薄"不能照抄，但其"直呼 syscall、不预建抽象"的风格值得吸收。

---

## 1. 开口项

### 1.1 P0 查漏补缺（对照 init.c 的运行时缺失与保真缺陷）

按"查漏补缺先行"的约定，先把缺口列全。标注说明：MISSING = C 有语义、Rust 无任何对应代码；TRAIT-ONLY = 只有 trait/fake、无 live 实现；BUG = 已有代码与 C 语义不符。

- **P0-1 ☐ main() 空转，入口 8 步只做了 3 步**（BUG-级缺失）。
  `main.rs:24-55` 实际只做参数解析（`parse_boot_args`）、设备探测（`FsDeviceProbe`）、入口决策（`decide_entry`），然后 `loop { park() }`。对照 `init.c:229-367` 的 8 步：身份校验（`check_identity` 存在于 `entry.rs:101` 但 main 从不调用）、setsid、信号注册（handle/delset，init.c:310-334）、close(0/1/2)（init.c:339-341）、securelevel 探测（has_securelevel，init.c:353）、`transition()`（init.c:358）全部缺失。`decision.runcom_mode` 计算后被丢弃（`main.rs:47-50` 两个 match 臂均为空），`-f` 目前没有任何运行时效果。`main.rs:29` 的 `_warnings` 也直接丢弃，C 侧 `warning()` 通道没有消费者。（修复方案：P1-1 运行模型定型后重写 main；在 minix-sys 缺口（E-INITSYS）落地前不可行动）

- **P0-2 ☐ 全部状态函数没有进程控制实体**（MISSING）。
  C 的核心动作——fork、exec、阻塞 waitpid、kill——在 crate 内零调用（minix-sys 的 `fork`/`exec`/`waitpid`/`kill` 封装在 `libs/minix-sys/src/lib.rs:148/157/167/175` 已存在，但 init 未使用）。具体缺口：`single_user` 的 shell spawn 与五分支 wait 循环（init.c:694-877，Rust 只有 `single_user.rs:60-83` 的分类器）；`runetcrc` 的 fork/exec + 双跑 chroot 重试（init.c:879-969，Rust 只有 `runcom.rs:27-47`）；`multi_user` 的 start_getty/start_window_system/setctty/collect_child 实体（init.c:1290-1370、669-689、1460-1497——**`setctty` 全库无任何 Rust 代码**，而 `multi_user.rs:3-5` 模块头声称覆盖它）；`clean_ttys` 的会话遍历 + kill（init.c:1569-1629，Rust 只有 `clean_ttys.rs:19-30` 四臂 diff）；`death` 的 kill(-1) + alarm + ECHILD 回收循环（init.c:1661-1698，Rust 只有 `shutdown.rs:22-29` 分类器）。（修复方案：随 P1-3 的 InitHost seam 逐状态函数落地，每落地一个状态函数同步其文档 §5 测试）

- **P0-3 ☐ 信号子系统只有映射表，没有真身**（MISSING/TRAIT-ONLY）。
  `state_machine.rs` 的 `signal_to_state`（:78-85）与 `AlarmFlag`（:156-168）是纯数据；`LiveSignalRegistry::register` 记录意图后返回 `Err(RegistryError::Deferred)`（:132-141），`block_all_except` 是空操作（:143-145）。C 的 5 个信号处理函数没有可运行的对应物：`transition_handler`（init.c:1502-1522）、`alrm_handler`（1649-1655）、`disaster`（505-511）、`minixreboot`（518-525）、`minixpowerdown`（531-538）。其中 minixreboot/minixpowerdown 需要 fork+exec `/sbin/shutdown`，`contracts.rs:24-35` 已能构造 argv 但无人执行。依赖 edge E-INITSYS ①。（修复方案：真 handler 落成一个只置原子标志 + `siglongjmp` 无关的最小函数族，与 P1-1 的 EINTR 主循环配套）

- **P0-4 ☐ wait-status 解码缺位：预解码 bool 没有生产者**（MISSING/设计缺口）。
  `classify_wait`（`single_user.rs:60-66`）、`classify_rc_exit`（`runcom.rs:27-33`）、`classify_collect`（`multi_user.rs:34`）、`diff_line`（`clean_ttys.rs:19`）、`plan_read_ttys`（`ttys.rs:51`）全部吃"预先解码好的 bool 散参"（stopped/exited_normally/signaled/termsig_is_kill……）。C 用 `WIFEXITED`/`WEXITSTATUS`/`WUNTRACED` 宏族解码 waitpid 状态（init.c:825-873 等），而这组宏在全 `os/libs` 没有任何对应物——grep `WIFEXITED|WEXITSTATUS|WUNTRACED|WaitStatus` 在 pm/minix-sys/minix-types 均零命中。也就是说这些签名是 C 宏形状倒灌进 Rust 的产物，现实中没有任何调用方能造出这些 bool。（修复方案：P1-4 引入 `WaitStatus` 类型；宏族解码器的归属在 edge E-INITSYS ③ 落地）

- **P0-5 ☐ mfs_dev 的 MAKEDEV 分支缺失，live 探测用宿主文件系统**（MISSING/BUG-级）。
  `entry.rs:158-166` 注释自认"live implementation will use minix_sys once those syscalls land (currently stubbed)"；C 的 MAKEDEV fork/exec 兜底（init.c:1759-1787）与 `_exit(11/12)` 两条退路完全没有 Rust 代码。现存的 `FsDeviceProbe`（`entry.rs:202-214`）用 `std::fs::metadata` 问的是**宿主**文件系统，在真机镜像上这个 std 链接的二进制根本无法运行（P1-2）。`entry.rs:9` 头注释声称"Side effects (exiting, forking MAKEDEV, closing fds) live in main.rs"，而 main.rs 一个都没做——注释已过时。（修复方案：live 探测改走 minix-sys stat/access；MAKEDEV 分支随 P0-2 的 fork/exec 能力一起落地）

- **P0-6 ☐ utmp/utmpx 记账写路径整体缺失**（MISSING）。
  `utmp.rs` 只有记录结构与 runlevel 映射：`make_utmpx`/`session_utmpx`（init.c:1372-1409）、`utmpx_set_runlevel`（1429-1451）、`logwtmp`/`logwtmpx`、`clear_session_logs`（647-662）均无实现，模块头（`utmp.rs:1-6`）自述 ARCH A-2 defer 且连 sink trait 都未定义。C 在 runcom 末尾写 `~`/`reboot`（init.c:1008）、death 末尾写 `~`/`shutdown`（1674）这两个落账点没有任何 Rust 对应。依赖 VFS open 的存在路径（E-INITSYS ②）与 A-2 决策。（修复方案：先落 `UtmpSink` seam + 内存 fake，live 半等 VFS；两个落账点作为 P0-2 状态函数的组成部分实现）

- **P0-7 ☐ securelevel 只有不完整的 fake，createsysctlnode 缺失**（TRAIT-ONLY/MISSING）。
  `sysctl.rs:10-45` 的 `SecureLevel` trait 只有 `FakeSecureLevel` 一个实现（单行为 impl，另见 P1-3）；C 的 `createsysctlnode`（init.c:1811-1857，init.root 节点创建）没有任何 Rust 代码，`shouldchroot` 的 ENOENT 节点重建逻辑（1872-1885）也没有——`sysctl.rs:48-50` 的 `should_chroot` 只是字符串判断。依赖 A-4/A-5 与内核 mib 服务（10-stage）。（修复方案：trait 契约保持，live 半挂 edge E-INITSYS ② 的 sysctl 面；createsysctlnode 语义并入该条）

- **P0-8 ☐ 单用户口令门没有密码验证**（MISSING）。
  `classify_attempt(input_empty, matches)`（`single_user.rs:29-37`）吃一个 `matches: bool`，而 C 用 getpwnam + crypt 比较口令（init.c:754-788）——比较本身没有任何 Rust 实现，`root_has_password` 的来源（读 /etc/passwd 或等价物）同样缺失。真机上这道门等于不存在。依赖 passwd/group 读路径（与 15-stage-fs 相关，但口令比较属本 stage 语义）。（修复方案：`PasswordVerifier` seam + crypt 语义的 Rust 实现，live 数据源另议）

- **P0-9 ✅ 2026-09-18（Fix #1）ttys 解析对齐 getttyent 字段语义**。
  原问题：`ttys.rs:37-38` 用 `status.contains("on")` 和 `contains("secure")` 判断 TTY_ON/TTY_SECURE，子串误匹配（`ondemand` 会开行、`insecure` 会开 root）；且真实样本 `minix3/etc/ttys` 首行的引号 getty 字段 `"/usr/libexec/getty default"` 会被空白分词错位。
  **修复记录**：对照 `minix3/lib/libc/gen/getttyent.c` 后发现比预想深一层——引号是模式开关（`skip()` 的 `q ^= QUOTED`，getttyent.c:183-185，引号可在字段任意位置出现并被剥除）、`off` 是显式清除（先 `on` 后 `off` 后者赢）、`window=` 是带值选项、无状态 token 的行交出 status 0 的条目（过滤归 07）。重写为 `strip_inline_comment`（引号感知行内注释截断）+ `next_field`（skip 状态机：引号剥除、`\"` 转义）+ `TtyStatus` 位组 token 循环；golden 测试取真实 ttys 行。方案对比：A 仅 token 化（引号字段仍错位，否决）/ B getttyent 语义对齐（采纳）/ C 正则（no_std 零依赖不可用）。doc 06 §3/§4/§5 重写同步。测试 89 → 97；clippy 82 → 85（新增纯函数接线前传递性死代码，终态门在 P2-6）。

- **P0-10 ✅ 2026-09-18（Fix #2）plan 会话计数按 new_session 三条件过滤**。
  原问题：`plan_read_ttys` 把解析出的全部行计入（`lines.len()`），C 只对 TTY_ON 且名字与 getty 非空的行建会话（`new_session` init.c:1147-1149）。
  **修复记录**：方案对比——A plan 内部过滤（采纳：C 的过滤就发生在 read_ttys 的建会话循环 init.c:1279-1282 内，属本层语义）/ B 推给调用方（read_ttys 层语义外泄，否决）/ C 直接返回会话列表（Wave 3 实体化时自然发生，本轮不做）。过滤条件取 init.c:1147-1149 的三条件（on + 非空名 + 非空 getty）；新增 `test_plan_counts_exclude_off_and_gettyless_lines`。doc 06 §5 同步。测试 97 → 98。

### 1.2 P1 架构分层审视（每项 ≥2 方案对比；整体 → 模块 → trait → 函数）

- **P1-1 ☐ [L0 整体] 运行模型定型：阻塞 waitpid 主循环**。
  方案 A：单线程阻塞 `waitpid(-1, &status, WUNTRACED)` 主循环，信号 handler 只置原子标志（`AlarmFlag` 已为此预备，`state_machine.rs:152-168`），循环每轮检查标志位，waitpid 返回 EINTR 时继续——与 C 结构同构（init.c:1559-1561 的 while(1) waitpid），与 Redox init 的裸 waitpid 循环同构（其 src/main.rs 收尾三行就是一个 loop-waitpid）。方案 B：self-pipe 事件循环（Linux 最佳实践，Kerrisk TLPI 的 async-signal-safe 标准解）——需要 pipe(2)，minix-sys 无此封装，为 init 单独引管道收益存疑。方案 C：现状 park()——不是方案，是缺口本身。**推荐 A**；前提是 E-INITSYS ① 的 sigaction/sigprocmask 与 EINTR 语义（plan.md A-9）落地。附带决策：EINTR 后重试与"先查 flag 再 wait"的竞态窗口处理要写进文档 02（C 靠 sigsetmask 窗口，init.c:335-367）。

- **P1-2 ☐ [L0 整体] std 链接 vs no_std 决策**。
  AGENTS.md 规定 `#![no_std]` everywhere except `#[cfg(test)]`；本 crate 通过 minix-rt 的 std feature 链接了完整 std（`Cargo.toml:14`），使用 `std::env::args`/`std::fs`/`std::thread::park`（`main.rs:28/34-41/53`），不是 freestanding Minix 进程。方案 A：`cfg(feature = "hosted")` 双模式——真机路径只准用 minix_rt/minix_sys，hosted 路径供宿主机跑集成测试（`FsDeviceProbe` 降级为 hosted-only）。方案 B：立即 no_std 化，宿主测试全部改走 fake seam。这是全部 os/commands 的共性问题（不止 init），建议在 edge E-CMDSYSFACE 下统一决策；**本 stage 侧先落约束**：新代码不得再新增 std 依赖面，真机入口函数（main 的 boot 路径）只准依赖 minix_rt/minix_sys。（关联：edge_todo.md E-CMDSYSFACE）

- **P1-3 ☐ [L2 trait 层] seam 收敛：7 个 trait → 单一 InitHost 面（推荐）**。
  现状 7 个 seam：`DeviceProbe`/`SignalRegistry`/`TransitionDriver`/`LogSink`/`Clock`/`SessionDb`/`SecureLevel`。其中三个是单行为 impl（`Clock` 只有 FakeClock，`log.rs:50-64`；`SecureLevel` 只有 Fake，`sysctl.rs:10-45`；`TransitionDriver` 唯一实现是测试脚本，`state_machine.rs:199-207`）；`ConsoleLogSink` 是为凑"两个 impl"而存在的静默 no-op（`log.rs:42-47`，违反模式 80 的精神——预建 mock 抽象）；`FakeDb` 只是 HashMapDb 加一个 fail_open 标志（`session_db.rs:65-93`）。方案 A：保留 7 trait、逐个补 live impl——impl 面爆炸（每个 syscall 族一个 trait），且多数 live 半被 E-INITSYS 阻塞，是模式 80 的持续温床。方案 B：**收敛为一个 `InitHost` trait**（fork/exec/waitpid/kill/alarm/sigmask/sleep/now/console_write/open_stat 一个面，live 实现 = minix-sys + minix-rt，hosted 实现 = std，fake 实现 = 测试脚本），纯决策逻辑保持自由函数——embedded-hal 的单 trait 资源面风格，消灭全部单 impl trait 与凑数 impl。方案 C：无 trait、直接调 minix-sys（Redox 风格），测试靠纯函数抽取——最薄，但宿主机无法集成测试主循环。**推荐 B**（决策逻辑自由函数 + 副作用收敛 InitHost 的混合体）；落地时逐模块迁移，迁移完成的模块删除旧 seam。架构级改动需按规范在 doc + design + code 三处一致标注 `[ARCH: init-host-seam]`。

- **P1-4 ☐ [L3 类型层] 引入 WaitStatus，重造 5 个分类器签名**。
  参照 nix 0.31 的 `WaitStatus`（docs.rs/nix：`from_raw(pid, status) -> Result<WaitStatus>`，variant `Exited(Pid, i32)`/`Signaled(Pid, Signal, bool)`/`Stopped(Pid, Signal)`/`Continued(Pid)`/`StillAlive`，EINVAL 显式报错不 panic），在 minix-sys（或 init 内先行、成熟后上移）定义 no_std 版本 `enum WaitStatus { Exited(i32), Signaled(Signal, bool), Stopped(Signal), Continued }` + `from_raw`。`classify_wait`/`classify_rc_exit`/`classify_collect` 改为吃 `&WaitStatus`——这同时让 C 的 WUNTRACED 观察点（init.c:827-841 的 stopped 分支）在类型上可表达，而不是靠一个无人生产的 `stopped: bool`。解码器本体归 edge E-INITSYS ③；**本 stage 的签名改造是自己的事**。

- **P1-5 ☐ [L3 类型层] Signal 枚举补 signum 映射，收编全部裸数字**。
  `Signal` 枚举（`state_machine.rs:54-62`）没有与 POSIX signum 的映射，导致 `contracts.rs:15-21` 只能裸写 `6`、`10 | 30`，`shutdown.rs:11` 裸写 `[1, 15, 9]`。方案 A：给 Signal 加 `as_signum/from_signum`（目标平台常量，x86-64 与 Minix 原始值差异按 99 篇常量表钉住）。方案 B：独立 `mod sig` 常量族（SIGABRT: i32 = 6 等），Signal 枚举只管 transition 相关三信号。**推荐 A**（单一权威，contracts/shutdown/log-disaster 全部改引）。顺带：`Signal::Sigstp` 命名偏离 POSIX（SIGTSTP，终端停止），更名为 `Sigtstp` 或至少补注释说明。（修复成本小，可与 P1-4 同轮）

- **P1-6 ☐ [L3 类型层] exec 路径与 argv[0] 的区分**。
  `ParsedCommand { program, argv }`（`session.rs:41-44`）中 `program` 既是 exec 路径又是 argv[0]。C 语义里两者分三种情形：runetcrc 组 argv[0]="sh"（init.c:899-900）后 `execv(INIT_BSHELL, argv)`（init.c:913）——路径来自 _PATH_BSHELL，argv[0] 只是 "sh"；minixreboot/minixpowerdown 硬编码 `execl("/sbin/shutdown", "shutdown", ...)`（init.c:521-522、534-535）；而 getty/window 反而是 argv[0] 即路径（`execv(sp->se_getty_argv[0], ...)`，init.c:1365）。现 Rust 数据把 rc 组装成 `vec!["sh", "/etc/rc"]`（`runcom.rs:9-15`）、shutdown 组装成 `vec!["shutdown", ...]`（`contracts.rs:24-35`）——rc/shutdown 真机 exec 按头元素找路径必然失败，getty 恰好可行。方案 A：`ParsedCommand` 改为 `{ exec_path: String, argv: Vec<String> }`——getty/window 两者相等（沿 C 惯例），rc 与 shutdown 显式分离。方案 B：保持现结构、exec 调用方按情形硬编码路径——把路径知识散落各处。**推荐 A**，随 P0-2 落地。（pathnames 常量：`minix3/sbin/init/pathnames.h` 定义 _PATH_SLOGGER/_PATH_RUNCOM，INIT_BSHELL= _PATH_BSHELL 见 init.c:105——99 篇常量表应补全）

- **P1-7 ☐ [L2 trait 层] TransitionDriver 重审：任意图驱动器测不了真图**。
  `TransitionDriver` 的 `step()` 抽象（`state_machine.rs:175-193`）让唯一实现（测试的 ScriptDriver）去演一张**任意的**状态图——它验证的是驱动器循环本身，而不是 init 的真实转移规则。C 的 `transition()` 是固定 switch（init.c:624-640：每态调对应函数、取返回的下一态）。方案 A：删 trait，落一个纯函数 `fn next_state(state: StateKind, outcome: StateOutcome) -> Option<StateKind>` 按真 switch 编码转移规则 + 状态函数在 InitHost 上的执行——测试直接喂 StateOutcome 序列验证**真图**。方案 B：保留 trait 但让它包装真实状态函数表——多一层间接，测试还是测不到真逻辑。**推荐 A**（与 P1-3 的 InitHost 合并落地）。

- **P1-8 ☐ [测试架构] 测试三层重构：分类器单测 → golden → 主循环集成**。
  现状 89 个测试全部是纯分类器单测，其中 3 个是无效测试（见 P2-1/P2-2）。重构方向：第一层保留分类器单测但改吃 `WaitStatus`/token 化输入（P1-4/P0-9 落地后）；第二层给 ttys 解析加 `minix3/etc/ttys` 真实样本 golden 测试（含 `off`、注释、window 列、secure 组合）；第三层在 InitHost fake 上做状态机集成测试——用脚本化 host 驱动 `transition` 全图（'s'→'r'→'t'→'m'→'T'/'c'/'d' 的每条边），断言 fork/exec/waitpid/kill 调用序列。这是"接线后死代码重审"（P2-6）的验收面：集成测试就位后，82 条 clippy 死代码告警应收敛到个位数。

### 1.3 P2 打磨

- **P2-1 ✅ 2026-09-18（Fix #3）删除恒等函数 `catatonia_marks` 及无效测试**。
  原问题：`shutdown.rs:33-35` 的 `catatonia_marks(session_count) -> usize { session_count }` 是恒等函数，其测试断言 5==5。C 的 catatonia 语义是"全部会话置 SE_SHUTDOWN 后回 multi_user"（init.c:1634-1643），数量不是语义。
  **修复记录**：方案对比——A 删除，语义由 Wave3 的 catatonia 实体承接（采纳）/ B 改造为标记动作枚举（为保留而保留，YAGNI，否决）。doc 11 §5 表同步删除该行。测试 98 → 97（净减一个无效测试，属预期减项）。
- **P2-2 ☐ 假调用 shim 与空洞测试：`vec_from_slice` / utmp 空断言**。
  `entry.rs:359-365` 的 `alloc_or_std_vec::vec_from_slice` 唯一使用点是一个测试里的 `let _ = vec_from_slice(&[1u8]);`（:232）——调用只为压制 dead_code 告警，断言零内容。`utmp.rs:95-100` 的 `test_runlevel_skipped_when_no_sessions` 断言局部常量 `let sessions_empty = true` 为真——C 语义（init.c:1439-1440 的 sessions==NULL 短路）完全没有被测试到。两条都是模式"测试族-无效"实例：删除 shim；utmp 测试改为对真实函数的空会话输入断言（依赖 P0-6 的 sink 落地，或先删除占位测试）。
- **P2-3 ☐ `AlreadyRunning → EEXIST` 的自造 errno 重审**。
  `entry.rs:88-91` 把 C 的 `errx(1, "already running")`（init.c:248-249，无 errno）映射为 EEXIST，commit 70d66c605 有辩护记录。问题：AGENTS.md 规定错误类型必须对齐 Minix3 errno、不自造；errx 语义是"打印+退出码 1"，不是 errno。方案 A：`EntryError::AlreadyRunning` 不再提供 to_errno，改为 console 报错 + `exit(1)`（忠实 C）。方案 B：保留映射但文档标注"仅 hosted 测试用"。**推荐 A**，真机路径不经过 errno。
- **P2-4 ☐ `disaster` 的信号名参数**。
  `log.rs:92-101` 的 `disaster(sink, clock, sig, sig_name: &str)` 让调用方自带信号名字符串；C 用 `sys_siglist[sig]`（init.c:507）。P1-5 的 Signal 映射落地后，信号名应从同一权威生成，删掉 &str 参数。
- **P2-5 ☐ 文档与快照 drift 一批**。
  ① `00-init-overview.md:6` 与 `99-init-global-concepts.md` 仍是 pending 骨架，且缺 `.design/` 快照（design-coverage-check CRITICAL，见 §0）；② `README.md:32` 仍说"checklist.md 实现期创建"，早于 2026-09-04 的 01-14 重写，已过时；③ `multi_user.rs:3-5` 模块头声称覆盖 setctty/start_window_system/start_getty/collect_child，实际文件里没有这些函数（P0-2 落地时改写）；④ `entry.rs:9` "side effects live in main.rs" 已不成立（P0-5 落地时改写）。五处均为文档-代码同步门范围，随对应修复轮处理。
- **P2-6 ☐ 死代码消除（接线后重审）**。
  当前 82 条 clippy 死代码告警（7 个 trait、Session/TtysLine/SessionRecord/ParsedCommand 等 never constructed）的根因是 L0 未接线，**现在不是删除时机**——删除会让接线时全部重写。正确顺序：P0-2/P1-3 接线完成 → 重跑 clippy → 按"为何死 + 消除影响"逐条消除真死码。本条领取条件：E-INITSYS ①② 闭单之后。
- **P2-7 ☐ SessionFlags 手搓位标志与 SE_SHUTDOWN 无置位方**。
  `session.rs:14-37` 手搓了 SessionFlags(u8) + 常量；`SE_SHUTDOWN`（:9）在全 crate 无任何逻辑置位（C 由 catatonia/clean_ttys/death 置位，init.c:1639-1640 等）。方案 A：迁 bitflags 风格宏（no_std 兼容）；方案 B：改为枚举集 `EnumSet` 风格。**推荐 B**（只有两个 flag，不值得引宏）。SE_SHUTDOWN 的置位随 P0-2 的 collect_child/catatonia 实体落地。

---

## 2. 边界条目双向指针（唯一入口：../edge_todo.md）

| edge 条目 | 来源 | 一句话 | 09 侧关联 |
|---|---|---|---|
| E-INITSYS | 本文 P0-1/2/3/4/5/6/7 | minix-sys 客户端面三件：①信号族封装（PM 端点已在，sigaction/sigprocmask/sigsuspend 客户端缺）②进程控制族（setsid/getuid/reboot 客户端缺；open 存在路径 ENOSYS）③WaitStatus 解码 + WUNTRACED | 本文 P0-1~P0-7、P1-1、P1-4 |
| E-CMDSYSFACE（既有） | edge_todo.md:1028 | 命令层 libc face 缺口总条目；std 链接问题是 commands 系共性 | 本文 P1-2（no_std 决策挂此条） |
| E-ISBOOT（既有） | edge_todo.md:617 | rc/system.conf 等价物不存在 | 本文 P0-2 的 /etc/rc 消费面；os/etc/ 仍是占位（os/etc/README.md） |
| E5（既有） | edge_todo.md:185 | 端到端联调包（含 init 的 START 冒烟链） | 本文 P0-2 完成后的验证出口 |

---

## 3. 剩余工作与领取条件

1. **可立即修（不等 edge）**：P0-9（ttys token 化 + golden 测试）、P0-10（plan 会话计数）、P2-1（恒等函数）、P2-2（shim 与空洞测试）、P1-5（Signal signum 收编）。每条遵循 fix-guard（修前读目标行 ±5、grep 确认、一次一条、修后 grep 验证并记录），修完跑 `cargo test -p minix-init` 对照基线 **89 passed**。
2. **P1-4 + P0-4（WaitStatus 签名改造）**：解码器 mini 版可先在 init 内落地（纯解码不需要 syscall），成熟后按 E-INITSYS ③ 上移 minix-sys。
3. **主体接线（P0-1/2/3 + P1-1/3/7）**：领取条件 = edge E-INITSYS ①② 至少①闭单（信号客户端面），否则状态机只能测到 fake 半边。
4. **P0-5/6/7/8**：分别挂 E-INITSYS ②、A-2 utmp 决策、10-stage mib、passwd 读路径，逐项领取。
5. **P2-3/4/5/7**：随上述对应修复轮顺带处理；P2-6（死代码消除）在接线完成后执行，验收 = clippy 死代码告警收敛到个位数 + 集成测试三层就位（P1-8）。
6. **下一轮架构审查触发条件**：E-INITSYS ①② 闭单后的验证轮（届时 P0-1~P0-3 的缺口表全部失效，需重扫）。

---

## 4. 存档

（首轮，暂无存档。后续完成的条目按 06-stage-sched 惯例移入 `archive/todo-V*-archive-*.md`，主文件只留开口项。）
