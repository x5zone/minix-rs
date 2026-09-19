# 09-stage-init Rust 实现架构级 Review TODO

> 来源：2026-09-17 第一轮扫描（code-excellence：查漏补缺先行 + 分层设计审视；对照 Redox init 与 nix WaitStatus）+ 2026-09-18 修复迭代轮。
> 范围：`os/commands/sbin/init/src/`（18 模块，crate `minix-init`）；对照 ground truth `minix3/sbin/init/init.c`（1902 行）与 `minix3/lib/libc/gen/getttyent.c`。
> 定位：不复写 plan.md；跨 stage 条目唯一入口是 `../edge_todo.md`（E-INITSYS）。
> 状态（2026-09-18 修复迭代轮收口）：**P0×10、P1×7、P2×6 共 23 条闭单（Fix #1~#20）**；开口项 4 条（P1-2/P2-5/P1-8 余件 + P2-6 残余面），全部挂共享基建等待态。已完成条目的完整修复记录以 `archive/todo-V1-archive-2026-09-18.md` 为检索权威，本主文件只留开口项。

---

## 0. 已完成轮次摘要（详情见 archive）

| 批次 | Fix | 内容 | 测试 |
|------|-----|------|------|
| Wave0 立修 | #1~#5 | P0-9 ttys 解析对齐 getttyent 字段语义（引号模式开关/精确 token/off 显式清除/window=）；P0-10 会话计数三条件过滤；P2-1 删恒等函数；P2-2 删 shim 与空洞测试；P1-5 Signal signum 权威收编（修真 bug：Minix3 SIGUSR1=30，10 是 SIGBUS） | 89→98 |
| Wave1 类型地基 | #6/#7 | P1-4 `wait.rs` WaitStatus 和类型（wait.h:53-70 位域锚定）+ 分类器签名改造；P1-6 ParsedCommand 分离 exec_path 与 argv[0] | 98→111 |
| Wave2 架构主脊 | #8 | P1-3 **[ARCH: init-host-seam]** 七 trait 收敛为单一 `InitHost`（plan.md §4 A-11 + docs 01/02/03/12 + host.rs 三处一致）；MinixSysHost 对缺失封装诚实 ENOSYS；ScriptHost 剧本宿主 | 111 保持 |
| Wave3 状态实体 | #9~#12 | P0-2a single_user 三幕实体；P0-2b runetcrc/runcom 双层实体；P0-2c multi_user 闭环；P0-2d clean_ttys/death/catatonia | 111→149 |
| 信号/台账 | #13/#14 | P0-3 SignalState（handler 只置原子位、spawn 移出信号上下文——自觉偏离已声明）；P0-6 utmp/wtmpx 台账语义闭环 | 149→157 |
| 主循环/接线 | #15~#17 | P1-7+P1-1 driver.rs（C 七行 transition 同构 + ChildCollector/Ledger 单所有者视图 + TransitionDriver 删除）；P0-1 main 八步接线（park() 移除）；接缝扩 read_file/append_file/chroot/read_line/set_env | 157→158 |
| 收尾 | #18~#20 | P0-8 password 分发+门控接线；P0-5 ensure_console（fork MAKEDEV）；P0-7 securelevel 收敛；P2-3 EEXIST 移除；P2-4 Signal::name；P2-6 死代码第一轮（删六个被实体取代的分类器） | 158→161 |

验证基线（2026-09-18 实测；后续轮对照）：

- `cargo test -p minix-init`：**143 passed / 0 failed**（首轮 89；P1-8 全图测试 +5 后为终态）
- `cargo clippy -p minix-init`：**32 条**——全部为 E-INITSYS 门控 API 面（见 §1 P2-6 残余表），非删除对象
- `tools/design-coverage-check.sh`：14/16 PASS（00/99 两篇骨架缺快照，归 P2-5）

---

## 1. 开口项（全部挂共享基建等待态）

### P2-5 ✅ 文档与快照 drift（2026-09-18 审计轮闭环）

- ✅ doc 06 §3（幽灵 `TtysSource` trait、子串匹配描述）、`multi_user.rs` 模块头、`entry.rs` 过时注释、docs 01/02/03/04/05/07/09/10/11/12/13/14 随实体重写。
- ✅ **00/99 正文 v1 落稿**（00 补 Rust 实现形态段；99 补 13 行常量对照表——CONSTTY/INIT_PATH/RUNLVL_MSG/MAKEDEV 兜底四项含内）。
- ✅ **README.md** 状态行与关键文件清单更新（checklist 过期说法移除，指向 todo.md 与 archive）。
- ✅ **`.design/` 00/99 快照六件套**生成，`tools/design-coverage-check.sh` 收敛为 **ALL DOCS COMPLETE**（19 docs）。
- ✅ **Gate E 全量对账**：14 条失效行清理（doc 01/02/03/05/12 的已删测试行），复扫「声称但代码缺失：无」。
- ✅ 审计补漏（二次确认轮，2026-09-18）：①plan.md 两处「当前为 stub」声明刷新为实体接线现状；②12 篇 §5.1 累计统计统一刷为「143 个通过（全 crate 口径，截至 2026-09-18）」（原为 2026-09-04 起的六种旧计数）；③doc 02/04 的 rg 清单行补 host.rs/password.rs 两个新模块。

### P2-6 ✅（第一轮+审计轮完成，残余为门控 API 面）

clippy 32 条残余全部是等待 E-INITSYS 的接线目标，**不删除**：

| 残余项 | 解锁条件 |
|---|---|
| signal_state（take_* 待主循环 drain 接线；note_* 已由 trampoline 消费 ✅） | E-INITSYS ① PM 臂闭单后由 run_transition 接管 |
| log::disaster（fatal handler 安装） | E-INITSYS ①；信号名已经 Signal::name 权威生成（P2-4 ✓） |
| contracts（request_for/shutdown_argv/SHUTDOWN_PATH）+ SignalState::take_shutdown_request 的驱动侧 drain | E-INITSYS ①（handler 置位 + 主循环边界执行 spawn） |
| multi_user::setctty 包装 | E-INITSYS ②（setsid/login_tty） |
| utmp（session_utmpx/RecordType::Login/clear_session_logs） | E-INITSYS ②（open-existing 后的文件写） |
| session_db（FakeDb/DbError/open/is_open） | 会话表重启路径接线 |
| wait（exited/signal 辅助） | 后续状态扩展 |
| password（scheme_has_backend） | libcrypt 后端落地 |

附带：`cargo clippy --fix` 会移除仅测试配置使用的导入，后续跑 --fix 必须以 `--tests` 视角复查编译。

### P1-2 ◐ std/no_std 决策（①② 已落地，③ 决策已记录待 edge 认领）

- ✅ 已落：boot 路径的参数读取改走 `minix_rt::crt0::argv_count/argv_bytes`（E-CMDSYSFACE 方案 A ② 的 env API 已由共享 lane 落地），`std::env` 依赖归零；真机入口路径只准依赖 minix_rt/minix_sys 的约束已写入本条。
- ✅ **① HashMap→BTreeMap**（2026-09-20，卡 J）：`session_db.rs` 换 `alloc::collections::BTreeMap`，类型名随实现改 `SessionMapDb`。A-1 修订（08 篇）：`alloc` 不带哈希容器；`SessionDb` trait 不暴露遍历（C 的 db 表同样"无遍历接口"，init.c:1021-1096），键序不可观测——换容器不改行为。A-1 的行为承诺（接口与行为不变、去 libdb 依赖）不变。
- ✅ **② Arc/atomic 换 alloc/core**（2026-09-20，卡 J）：生产路径 `main.rs`/`driver.rs`/`host.rs` 的 `alloc::sync::Arc`，`signal_state.rs`/`shutdown.rs`/`host.rs` 的 `core::sync::atomic`；crate 根 `extern crate alloc;`。测试构建保持 std（`catch_unwind` 等测试面不动）。多用户态共用 `SignalState` 的 `Arc` 语义（handler 与 death 共享，99 篇）不因换源而变。
- ◐ **③ panic handler 形式——决策已记录，执行面待 edge 认领（C-21）**。裁决：init 采用 minix-rt 的**统一形式**——`minix_rt::diag::format_panic_report`（栈上缓冲、无分配器无系统调用）渲染 + init 特有的收尾（PID 1 不允许自旋挂死全机：发 SYS_DIAGCTL 控制台后 `exit` 非零，内核可见"init 死了"，与 C 的 panic 即死同果）。**执行障碍（为什么不能在本 stage 单方面落地）**：启用 minix-rt 的 `panic-handler` feature 需同时 `default-features = false`（attr 门控 `not(feature = "std")`），而 feature 统一会让 std 测试二进制里同时出现 minix-rt 的 `#[panic_handler]` 与 std 的 `panic_impl`——E0152；改为 init 自定义 `#[panic_handler]` 则要求 minix-rt 去掉 std feature，workspace 统一构建下会把 std 拖进 no_std init（同类冲突）。两条路都要动 workspace 级的 minix-rt 特性声明（os/Cargo.toml 共享文件 + edge2 领地），按 edge4 §1 规则登记 C-21 认领。
- ☐ 残余：③ 的执行面（workspace 级 minix-rt 特性声明 + `#![cfg_attr(...)] no_std` 翻转 + 全模块 alloc prelude 迁移——容器使用面约 168 处/15 文件已盘点）。

### P1-8 ✅ 测试三层终检（2026-09-18 审计轮闭环）

三层就位：纯函数单测 ✓；ttys golden + 密码/台账单测 ✓；**全图逐边界断言** ✓（step() 提 pub(crate)，五条 boot-chain 测试走 's'→'r'→'t'→'m' 主线、rc 失败回退、'T'/'c'/'d' 边界与 ESRCH 早退）。

### P0-8 残余 ☐ libcrypt 哈希后端

password.rs 的解析/分发/门控接线已闭环；DES/MD5/SHA1/bcrypt 四后端（`minix3/lib/libcrypt/*.c` 约两千行）建议独立 `minix-crypt` crate，作为共享基建候选登记 edge（全体命令的认证消费方）。当前语义：后端缺失的方案一律拒绝——管理员代价是 ^D 走多用户，不会得到假通过。

---

## 2. 边界条目双向指针（唯一入口：../edge_todo.md）

| edge 条目 | 状态 | 一句话 | 09 侧关联 |
|---|---|---|---|
| E-INITSYS | 🔄 ④③①②客户端面已落地（e8be0da5a+6a9614bc6）；剩 PM dispatch 臂（04）+ minix-rt sigreturn 桩（14）+ E5 通电 | ①信号族封装 ②进程控制族 ③WaitStatus 上移 ④signal 再导出 | P0-1~P0-7 live 半、P1-1、P2-6 残余 |
| E-CMDSYSFACE（既有） | 开口 | 命令层 libc face；minix-rt env API + no_std 决策 | P1-2 |
| E-ISBOOT（既有） | 开口 | rc/system.conf 等价物不存在 | P0-2b 的 /etc/rc 消费面 |
| E5（既有） | 开口 | 端到端联调包（init START 冒烟） | 本轮实体的真机验证出口 |

---

## 3. 剩余工作与领取条件

1. **E-INITSYS ①②③**（edge 队列单线程领取）：闭单后 init 侧跟进 = 信号 handler trampoline + setsid/ctty 接线 + WaitStatus 上移 + 台账真文件写，预计一个小批次；同时 §1 P2-6 残余表逐条消解。
2. **P0-8 残余**：libcrypt 后端独立 crate（edge 登记）。
3. **P2-5 余件**：README/00/99 正文 + 快照 + 99 常量表（style-fix 级）。
4. **P1-2 余件**：cfg feature hosted 拆分（等 minix-rt env API）。
5. 每次修复遵循 fix-guard（修前读 ±5、一次一条、修后 grep 验证），修完跑 `cargo test -p minix-init` 对照基线 **138 passed**，Gate E 对照各篇 §5。

---

## 4. 存档

- 扫描轮原始条目 + Fix #1~#20 全记录 + Rule Discovery 三候选：`archive/todo-V1-archive-2026-09-18.md`
- 提交序列：53da21aeb（Fix #1）起每 Fix 一个提交，至 P2-6 轮（a135dea5b 及后续）
