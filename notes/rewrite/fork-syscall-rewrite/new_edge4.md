# new_edge4 — 编排 · 认领 · 验收阶梯（新一轮；前轮 edge4.md 于 2026-09-20 冻结）

> **定位**：本轮编排台。前轮 [edge4.md](edge4.md) 的 §1-§6 未决行全部携带至此并重编号；其 §7 阶梯 T0/T1 已达成部分在新 §7 标注。收敛归档规则见 §8。

---

## §1 并发规则（三线共同遵守，违反即 P0-process-violation）

1. **文件所有权**：各线只改自己头部所有权清单内路径；清单外触碰 = 跨界改点，先在 §2 认领板登记（线、条目、意图、触碰文件），登记后才动，动完销账。同一跨界改点同一时间只允许一条线持有。
2. **依赖等待**：前置属其他线时，开工前读对方 new_edgeX.md 状态列；非 ✅ 就先做别的，等待标 ⏸ 注明等谁。不允许代做对方所有权内的前置。
3. **共享文件串行化**：`os/Cargo.toml`、`os/qemu-tests/run_all.sh`、`edge_todo.md` 与 `00-master-plan/`、`tools/` ——任何线的修改先在 §2 登记一句话。
4. **进度记账**：完成即在状态列 ✅ + 日期 + commit；解锁他线条目同步勾 §3。**禁止直接回写 edge_todo.md / stage todo**——由本线（edge4 角色）在里程碑批量收敛（2026-09-20 已按此纪律完成一轮五路扫描收敛，见 edge_todo.md 同日节）。
5. **FIXLOG 增量纪律（新增，2026-09-20 事故后）**：三条线的 `.review/zcode/edgeN/FIXLOG.md` **只写增量，禁止全量复述**；edge3 的 FIXLOG 曾被全量重复追加 32 次膨胀到 68,552 行（95% 重复），已归档重组（263 行 + FIXLOG_archive.md + 原始底档 .bak）。追加前自检：`grep -c "^# edge3 线修复日志" FIXLOG.md` 必须为 1；edge3 编号从 **#122** 续（#117-119 历史撞号，引用带限定词）。
6. **并发隔离命名约定（新增）**：扫描/调研类中间产物一律带作者后缀（`new_todo_{name}.md`）；工作线文件按轮次更替（前轮 edge1-4 已冻结加横幅，本轮 new_edge1-4）；阶段收尾时由本线批量并回权威文件后冻结。
7. **领取锁（新增，2026-09-20）**：条目领取 = `tools/claim.sh claim <ID> <owner>`——`claim/<ID>-<owner>` 分支即排他锁（分支名全局唯一，双领第二个直接失败；worktree 间共享 refs 所以跨工作树有效）；`list` 按日期列全部领取（最旧者 = stale 候选，可回收）；完成后合入主线再 `release` 销账。重活并行用 `git worktree`（各线一树）；QEMU 真机验证保持全仓同一时刻只有一方在跑（串口判定不可并发）。
8. **构建优先级（新增，2026-09-20）**：cargo build/test/clippy **永远 docker `minix-ci:1.94` 第一优先**（`-m 2g -j 1`；docker 隔离一切 panic 崩 WSL，不只 OOM，无宿主直跑例外）；仅 docker 不可用才回退宿主 `ulimit -v 3145728`（KB）+ `-j 1`。标准命令与并发挂载见项目 memory《测试内存铁律》。

---

## §2 认领板（携带 + 新增）

### 携带的开放认领（状态照旧，详情见前轮 edge4.md §2 对应行）

| 编号 | 条目 | 线 | 前轮状态 | 本轮去向 |
|---|---|---|---|---|
| C-2 | E-SDEVOWN vfs 副本删除 | edge2 | 🔄 持锁 | **销账更正**：L8 已于 2026-09-18 闭环（907e65f79，vfs 923→233 行），前轮 §2 行未回勾 |
| C-6 | D-16 DumpCore wire 按值携带名字 | edge3 | ☐ | 携带；NS10 批内顺手 |
| C-17 | devman 客户端生产传输 SysClientTransport | edge3 | 🔄 登记即动 | 携带（非启动链，随 E-DMWIRE 通电波） |
| C-18 | 对话臂 magic grant（GrantTable::grant_magic） | edge3 | 🔄 登记即动 | 携带（VFS REQ_READ/STAT 臂消费，NS4/NS5 之后） |
| C-21 | fs-rt crate 成员 | edge3 | 🔄 卡D 开工 | 携带 |
| C-22 | MIB 取表 wire 权威扩展 | edge3 | 🔄 卡I 开工（kernel 半 C-25 已销） | 携带余段 |
| C-23 | termios wire + tty ioctl 面 | edge3 | 🔄 卡H 批次二十五 | 携带 |
| C-24 | smoltcp 外部依赖引入 | edge3 | 🔄 卡E 批三开工 | 携带 |
| C-26 | sffs 语义核心入库 | edge3 | 🔄 卡F3d 开工 | 携带 |
| C-27 | 全系统自举载体 | edge3 | 🔄 载体半程 | **并入 new_edge1 NK1 收口**（PASS 后接 run_all） |
| C-28 | srv_fork 子 PRIV_PROC 保留缺失【PM 类型设计裁决】 | edge3 | ☐ 待认领 | 携带（is_kernel_process 四判据；os/tests/srv_fork.rs 已钉偏差） |
| C-29 | boot loader per-process 地址空间/栈 | edge1 面 | ☐ 待认领 | **改编号 new_edge1 NK1**（设计裁决 OQ-N6 先行） |

### 新增认领（2026-09-20 扫描收敛产生）

| 编号 | 条目 | 线 | 触碰文件 | 状态 |
|---|---|---|---|---|
| C-30 | E-BOOTMODS boot-shim 清单半 | new_edge1 | `os/boot-shim/src/loader.rs`（MODULE_NAMES 扩 12 项对 C 序 + 删虚假注释 + 缺件 fail-fast） | ☐（随 NK4/OQ-N2） |
| C-31 | E-CONSOLE 驱动半 | new_edge2 | `os/drivers/tty/tty/`（输出后端）、`os/drivers/storage/memory/`（进程壳） | ☐（NL1/NL6） |
| C-32 | E-IMGPKG xtask 半 | new_edge3 | `os/xtask/src/main.rs`（image/qemu/build 实装） | ☐（NS8） |

---

## §3 依赖状态板（跨线前置一览）

| 前置（供方） | 消费方 | 状态 |
|---|---|---|
| new_edge2 NL2（minix-sef SefEvent::Init） | new_edge3 NS1（六服务器应答臂）、NL6（memory 进程壳） | ☐ |
| new_edge2 NL6（memory 驱动进程） | new_edge3 NS4（挂根）/NS6（块源）/NS8（镜像） | ☐ |
| new_edge2 NL4（pipe2/whoami/文件族 wrapper） | new_edge3 NS12（sh 等）、NL6（tty 端点） | ☐ |
| new_edge2 NL3③（sigreturn trampoline） | new_edge3 NS11（init trampoline 填真） | ☐ |
| new_edge1 NK1（C-29 载体收口） | new_edge3 NS1/NS2（boot 链真机验收）、E5 族真机半 | ☐ |
| new_edge1 NK2（页故障转发） | 一切用户程序（内存故障安全网）；E5(d) 真机 | ✅（2026-09-20，5037491ff） |
| new_edge1 NK4/OQ-N2（12 模块契约） | new_edge3 NS8（装机清单） | ☐ |
| new_edge3 NS4/NS5（挂根+exec） | NS9/NS12、T3/T4 | ☐ |
| new_edge3 NS7 + NL1（console 全链） | T4 冒烟（可见输出） | ☐ |

---

## §4 在制避让（2026-09-20 快照）

- 工作树：`tools/coverage-extract/ds-semantic-map.json` 有**别的线**的在制修改（前轮登记延续，本轮各线不碰）。
- 未跟踪产物：`new_todo_{deepseek,glm,HY4,muse,qwen}.md`（五路扫描留存备查，不删）；`new_edge1-4.md`（本轮工作文件）；`AI-chats/`、`notes/redesign/` 等与本轮无关。
- 前轮 edge1-4.md 已冻结（头部横幅），只读参照；其未决行的携带映射见本文件 §2/§7。

---

## §5 E5 端到端联调包编排（条目主体在 edge_todo.md E5）

| 子项 | 内容 | 前置 | 状态 |
|---|---|---|---|
| E5(a) | PM↔VM fork 全链路 | — | 🔄 宿主半 ✅（430803d4f）；真机半挂 T2（NS1/NK1） |
| E5(b) | VM↔VFS fdclose 往返 | — | 🔄 宿主半 ✅（79d82ed91）；真机半挂 T2 |
| E5(c) | RS live-update 全链 | — | 🔄 前哨段宿主半 ✅（fbd33bcae）；主体挂 T2（NS1/NS2） |
| E5(d) | QEMU VM paging 冒烟（缺页完整回路+VM 写 PTE） | new_edge1 NK2（转发臂） | ☐（内核转发臂 ✅ 5037491ff + 载体真机 PASS；余 T2 boot 链真机半） |
| E5(e) | PM↔SCHED 调度链 | NS10 + K1/K2 | ☐ |
| E5(f) | DS 发布/订阅三链 | NS1 | 🔄 宿主半 ✅（65bd107a6）；真机半挂 T2 |
| E5(g) | MIB/sysctl 四链 + rmibtest | NS1 + E-RMIBWIRE 通电 | ☐ |
| E5(h) | devman 生命周期四链 | NS1 + E-DMWIRE | ☐ |
| E5-SMP | fork 后父子并发写 CoW 页（陈旧 TLB 用例，-smp 4） | NK3 + X-8 修复 | ☐ |
| E5-ARCH | 三架构全系统复跑 + 交叉构建矩阵 | T2~T4 + NL5 | ☐ |

---

## §6 OQ 队列（等用户/联合裁决，任何线不得代决）

**携带**（前轮 §6 未决者照旧有效，编号不变）：OQ-3（已裁决盘上文件面——/etc 最小内容集仍需用户过目）、OQ-4（已裁决归 14-stage）、其余未决项按前轮 §6。

**新增（2026-09-20 扫描收敛）**：

| 编号 | 议题 | 备选 | 建议 |
|---|---|---|---|
| OQ-N1 | 命令全集口径：328（plan.md:14 实测）vs 247（muse 口径） | 以 plan.md 为权威 / 重测 | 采 plan.md 328，登记分歧即可 |
| OQ-N2 | E-BOOTMODS 修法：补齐 12 模块装机清单对 C 序 vs 放宽内核 assert/按名查找 | 前者（保 C 对位）/后者 | 补齐清单（HY4 论证：放宽 assert 掩盖装配错误） |
| OQ-N3 | console 输出通道选型 | 串口直程（需 port I/O 权限面）/ video-text+mem server / 系统任务中转 | QEMU 目标下串口最直接；需一并裁决 driver 进程的 I/O 特权模型 |
| OQ-N4 | minix-rt 页供应商通道 | VM_BRK taskcall / mmap 通道 | 循 E-BOOTFRAME 判例设计轮定 |
| OQ-N5 | 边界守卫是否豁免 `#[cfg(test)]` 代码 | 豁免 / 不豁免（测试改 re-export 面） | 豁免测试代码（守卫意图是生产行为分层）；stty 测试改用 minix_sys re-export |
| OQ-N6 | C-29 修法：load_vm_elf 增 stack_high 参数 vs per-process 根（C 对位） | ①快但共享根段面互撞仍在 ②动 kernel/arch 设计先行 | 方案②（C 对位，一劳永逸）；先出设计轮 |

---

## §7 最终验收阶梯（目标：三架构 QEMU 跑起来并执行 18-stage cmds）

| 阶梯 | 内容 | 主责 | 判定 |
|---|---|---|---|
| T0（已达成） | x86_64：boot+SMP 四核+user-trap+rt-birth；aarch64/riscv64：boot 冒烟 | — | 前轮已验 |
| T1（基本达成，余 trap-lane） | 三架构用户态门槛 | new_edge1 | NK2/NK3 落地 + run_all 三架构全绿 |
| T2 | x86_64 服务器通电链：C-29 载体（NK1）→ RS 出生面（NL2+NS1+NS2）→ PM/VFS/DS/MIB → VFS+mfs 挂根（NS4+NS6+NL6）→ console（NS7+NL1） | 三线协同 | 各服务器 main 去停车，E5(a)(b)(c)(f) 真机半 PASS |
| T3 | init 真实运行：NS3（exec 门 bug）→ NS5（exec worker）→ NS8 /etc → NS10/NS11 | new_edge3 | init 进 multi-user 雏形，waitpid/信号端到端 |
| T4 | 18-stage 命令执行：NS8 装机 → NS12 命令批（echo/ls/cat/sh）→ 冒烟脚本 | new_edge3 + new_edge2 | 18-stage 命令冒烟 PASS（有可见输出） |
| T5 | 三架构复跑 T2~T4 + SMP 正确性（NK6/E5-SMP/E5-ARCH） | new_edge4 编排 | 三架构 × 全梯次 PASS → §8 收尾 |

---

## §8 收尾与归档规则

1. 每达成一个阶梯，本线勾账并**批量**把涉及条目回写 edge_todo.md 与对应 stage todo（2026-09-20 已示范一轮）。
2. 本轮四文件（new_edge1-4.md）与前轮 edge1-4.md 同规则：T5 全绿后冻结、全量对账、并回 edge_todo.md 归档段。前轮 edge1-4.md 已于 2026-09-20 加冻结横幅（其未决行的携带映射见本文件 §2/§7）。
3. 五份 new_todo_*.md 为 2026-09-20 扫描的证据留存，合并完成后不删（备 later 对账）；下轮扫描产物继续按 `new_todo_{name}.md` 命名隔离。
4. 过程中新发现条目先进所属线 new_edgeX.md（§1 规则 7 前轮条款延续），收尾随批次并回权威文件。
