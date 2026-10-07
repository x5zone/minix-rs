# 09-stage-init todo.md V1 归档（扫描轮 + 修复迭代轮，2026-09-17/18）

> **历史快照，不作现状来源**（标记于 `2026-10-08`）：本文件记录的是归档当轮的判定与读数，本轮尚未逐条复核；现状请以 ../../../coordination/TODO-LEDGER-OPEN.md（未完成）与 ../../../coordination/TODO-LEDGER-DONE.md（已完成与已定案）为准。

> 本文件是 2026-09-17 扫描轮全部原始条目与 2026-09-18 修复迭代轮 Fix #1~#20 的检索权威。主 todo.md 只留开口项。

## 扫描轮原始条目（2026-09-17）

P0×10：P0-1 main 空转 / P0-2 状态函数无实体 / P0-3 信号子系统只有映射表 / P0-4 wait-status 解码缺位 / P0-5 MAKEDEV 缺失+宿主 fs 探测 / P0-6 utmp 写路径缺失 / P0-7 securelevel fake+createsysctlnode 缺失 / P0-8 口令门无验证 / P0-9 ttys contains("on") 子串误匹配 / P0-10 plan 把 off 行计入会话数。
P1×8：P1-1 运行模型 / P1-2 std-no_std / P1-3 seam 收敛 / P1-4 WaitStatus / P1-5 Signal signum / P1-6 exec_path 分离 / P1-7 TransitionDriver / P1-8 测试三层。
P2×7：P2-1 catatonia_marks / P2-2 shim+空洞测试 / P2-3 EEXIST / P2-4 sig_name / P2-5 文档 drift / P2-6 死代码 / P2-7 SessionFlags。

扫描轮核心判断：crate 不是 translate 问题——纯逻辑质量良好；问题是纯逻辑骨架先行、运行时一行未接（main park() 永久停泊，clippy 82 条死代码告警为量化证据）。联网对比：Redox init 为极薄配置解释器 + 裸 waitpid 循环（无信号/无监督/无 trait），印证 pid1 运行模型与「接缝价值在能测不在抽象」；nix WaitStatus::from_raw 为解码类型参照。

## 修复迭代轮记录（每 Fix 一个提交）

| Fix | 条目 | 提交 | 摘要 |
|-----|------|------|------|
| #1 | P0-9 | 53da21aeb | ttys 解析对齐 getttyent 字段语义：引号模式开关（q^=QUOTED，getttyent.c:183-185）、off 显式清除、精确 token、window= 带值；strip_inline_comment + next_field 状态机；golden 取 minix3/etc/ttys 真实行。doc 06 重写 |
| #2 | P0-10 | 88dad1c15 | plan_read_ttys 按 new_session 三条件过滤（on+非空名+非空 getty，init.c:1147-1149） |
| #3 | P2-1 | f4a36d924 | 删恒等函数 catatonia_marks + 5==5 无效测试 |
| #4 | P2-2 | e8d47c637 | 删 vec_from_slice shim + utmp 空洞测试（runlevel 短路真测试随 P0-6 补） |
| #5 | P1-5 | 3cb54f375 | Signal signum 权威收编；修真 bug：Minix3 SIGUSR1=30（signal.h:82），10 是 SIGBUS——原 contracts 10\|30 臂会把总线错误当关机；Sigstp→Sigtstp；sig 常量模块锚定 minix-types 权威（minix-sys 再导出被并行 lane 阻塞，登记跟进） |
| #6 | P1-4 | e3d3900b3 (+6d965eb6b) | 新建 wait.rs：WaitStatus 和类型（wait.h:53-70 位域锚定；无 Continued 变体——Minix3 无 WIFCONTINUED；无 EINVAL 失败路径——位域全组合恰落一变体）；WNOHANG/WUNTRACED 常量；分类器签名改造（范围修正：需改造的分类器实为 2 个） |
| #7 | P1-6 | 6a3c81fb8 | ParsedCommand 分离 exec_path 与 argv[0]（getty/window 路径即 argv[0]（init.c:1365）、rc=INIT_BSHELL+argv[0]"sh"（init.c:899-900/913）、shutdown=字面 /sbin/shutdown（init.c:521-522））；rc_argv/shutdown_argv 升级 |
| #8 | P1-3 | fe709c410 | [ARCH: init-host-seam] 七 trait 收敛为单一 InitHost（plan.md §4 A-11 + docs 01/02/03/12 + host.rs 三处一致）；MinixSysHost 对缺失封装诚实 ENOSYS（沿 minix-sys open-existing 先例）；ScriptHost 剧本宿主替代五个 Fake；SessionDb 留接缝外 |
| #9 | P0-2a | 61ca8bfb9 (+docs) | single_user 三幕实体：安全级降级（gate 用降级前值）、SIG_IGN 窗口（ignore_spec/restore_spec）、fork 失败重试、等待循环每子进程喂 collect、SIGKILL→AwaitReboot；子分支 constty/console 选路+口令门（^D=_exit(0)）+ALTSHELL+PATH+双 exec 兜底（重试即使同路径，init.c:803-808）；接缝补 read_line/set_env/Ignore/SIGNAL_CONTINUE |
| #10 | P0-2b | ec49dcf99 (+a9ae80f63) | runetcrc/runcom 双层实体：子分支 SIG_IGN+占控制台+解屏蔽+可选 chroot（_exit 4）+exec（stall+_exit 5）；父循环 SIGCONT 续等；SIGTERM+catatonia 双条件静默；fork 失败睡 30s；Booted{did_multiuser_chroot}+ledger.reboot() |
| #11 | P0-2c | dd9afa5f5 | multi_user 闭环：==0 升安全级 1、扫表补 getty、waitpid 不带 WUNTRACED（options 0）、start_getty（chroot 跟随/_exit 7、子进程内防抖、窗口先于 getty/WINDOW_WAIT、_exit 8）、start_window_system（_exit 6）、collect_child 四动作；Session 增 started_secs；时钟 ENOSYS 降级 |
| #12 | P0-2d | bdcd159a1 | clean_ttys PRESENT 标记-清扫 diff（索引迁移警告、三路 SHUTDOWN+SIGHUP、空文件=全体退役）；death 三轮 kill(-1)+alarm+clang（ECHILD 早退、ps-axl 警告）；catatonia；ScriptHost 补 kill_errors/wait_errors/alarm 触发 clang |
| #13 | P0-3 | 42c99e436 | SignalState 三位一体（clang/requested_transition/minix 挂钩请求）；handler 只置原子位、spawn 移出信号上下文（自觉偏离声明，外部行为不变）；AlarmFlag 收敛单点 |
| #14 | P0-6 | 082643157 | utmp/wtmpx 台账闭环：UtmpxRecord 契约+RecordType 数值；runlevel 短路（sessions==NULL）真测试；~reboot/~shutdown 条目；DEAD 两步写；on-disk 编码为自觉临时文本形态（ABI 收敛记 A-2 残余）；接缝补 append_file |
| #15 | P1-7+P1-1 | eba640891 | driver.rs：run_transition 七行循环同构（台账→分派→执行）；DriverState 单所有者 + ChildCollector/Ledger 切片视图；TransitionDriver 任意图 trait 删除；接缝补 read_file；single_user 补 EINTR 续等 |
| #16 | P0-1 | a3a794451 | main 八步接线：身份门（getuid ENOSYS 告警降级、getpid 非 1 退出）、setsid/信号注册/stdio 清理逐项尝试+ENOSYS 告警、ensure_console→决策→DriverState→run_transition；park() 移除 |
| #17 | P0-8 | ff9dfaaed | password.rs：pw_passwd 前缀分发（Empty/Locked/DES/MD5/SHA1/bcrypt 对齐 __crypt 分派）+ /etc/passwd root 行提取 + build_verifier（后端缺失拒绝不伪装）；main 接线 root_verify（read_file ENOSYS → 无门，同 getpwnam 失败的 C 形） |
| #18 | P0-5 | ff9dfaaed | ensure_console：console 缺失时 fork sh /dev/MAKEDEV -MM init + 重检；helper _exit(10/11/12) 折叠为 bool + 同款告警 |
| #19 | P0-7 | ff9dfaaed | securelevel 语义经 InitHost 完整（读/降级/升 1）；live 半维持 A-4/A-5 kernel 缺口标注 |
| #20 | P2-6 等 | a135dea5b (+后续) | 删六个被实体内联取代的分类器（classify_wait/wait_outcome、classify_rc_exit/RcOutcome、classify_collect、diff_line/LineAction、classify_round/DeathRoundOutcome、classify_attempt/PasswordAttempt）；P2-3 EntryError/EEXIST 移除（main 已是 console 报错+exit(1) 忠实路径）；P2-4 Signal::name；机械 lint（wildcard-or/冗余 guard/&mut Vec/must_use） |

## 基线终态（2026-09-18）

- `cargo test -p minix-init`：**138 passed / 0 failed**（首轮 89；中途峰值 165，P2-6 删 23 个被实体取代的分类器测试后为终态）
- `cargo clippy -p minix-init`：**32 条**——全部为 E-INITSYS 门控 API 面（signal_state handler 入口、disaster、setctty 包装、session_utmpx、contracts 构造器、wait 辅助方法、FakeDb 等），逐条登记于主 todo.md §1 P2-6 残余表；E-INITSYS 各子项闭单后即消解
- Rule Discovery 三候选：①「预解码 bool 分类器」模式（C 宏族缺位时签名形状暴露 translate，上游无解码器判 P0-设计缺口）；②模块头声明覆盖范围与文件实际内容的一致性应进 Gate E 类检查；③clippy --fix 会移除仅测试配置使用的导入，--fix 后必须以 --tests 视角复查编译
