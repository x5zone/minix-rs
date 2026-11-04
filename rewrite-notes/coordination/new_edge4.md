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
7. **领取锁 + 工作树隔离（2026-09-20 C-35 事故后硬化为机制）**：条目领取 = `tools/claim.sh claim <ID> <owner>`——`claim/<ID>-<owner>` 分支即排他锁（分支名全局唯一，双领第二个直接失败；worktree 间共享 refs 所以跨工作树有效）；claim **同时自动建 `.wt/<id>-<owner>/` 专属工作树并打印 `cd` 路径**，领取者只在本会话的专属树内改码/构建/测试；开工/换任务前跑 `tools/claim.sh verify` 自检位置（主树 → 警告；claim 分支 + 专属树 → OK）。**共享主树内禁 checkout/reset --force**——主树同时承载多线未提交在制品，换分支即销毁别人的工作（C-35 判例）。`list` 按日期列全部领取（最旧者 = stale 候选，可回收）；完成后合入主线再 `release` 销账（自动删工作树，树内有未提交改动则拒绝，防误弃）。**在制品守卫**：new_edgeX.md 与 `tools/claim.sh` 等纪律载体必须被 git 跟踪且改完即 commit——未跟踪/脏状态 = 一次误 reset 就没（C-35 同日 `tools/claim.sh`、`new_edge2.md` 曾处于未跟踪态）。QEMU 真机验证保持全仓同一时刻只有一方在跑（串口判定不可并发）。
8. **在制品即提交（新增，2026-09-20，随规则 7 机制化一并立规）**：new_edgeX.md 状态列/认领板每次更新后立即 commit（风格照旧 `docs(edgeN): …`），勿留未跟踪或长期脏状态——未跟踪文件是一次 `reset --hard` 的零成本牺牲品；`tools/claim.sh verify` 报出的未提交文件数即自查信号。
10. **共享文件触碰登记（run_all 接线，2026-09-21 zcode_glm_4）**：NK1 收口单在 `os/qemu-tests/run_all.sh` x86_64 特殊协议段尾接线 `test-sysboot.sh`（自建内核+六串行标记判定，test-rt-birth 同形），头注第 8 条测试族同步点名——共享文件触碰照规则 3 在此登记一句话，代码随 merge b5fe7e2fa。
9. **构建优先级（新增，2026-09-20）**：cargo build/test/clippy **永远 docker `minix-ci:1.94` 第一优先**（`-m 2g -j 1`；docker 隔离一切 panic 崩 WSL，不只 OOM，无宿主直跑例外）；仅 docker 不可用才回退宿主 `ulimit -v 3145728`（KB）+ `-j 1`。标准命令与并发挂载见项目 memory《测试内存铁律》。

---

## §2 认领板（携带 + 新增）

### 携带的开放认领（状态照旧，详情见前轮 edge4.md §2 对应行）

| 编号 | 条目 | 线 | 前轮状态 | 本轮去向 |
|---|---|---|---|---|
| C-2 | E-SDEVOWN vfs 副本删除 | edge2 | ✅ 2026-09-18 已闭环（907e65f79） | **销账回勾（2026-09-23 评审方）**：L8 已于 2026-09-18 闭环（907e65f79，vfs 923→233 行），前轮 §2 行未回勾——本轮回勾销账 |
| C-6 | D-16 DumpCore wire 按值携带名字 | edge3 | ✅ 2026-09-21（zcode_glm_3，57db7bbde，随 NS10 批内执行） | `VfsCall::DumpCore` 去指针槽改 `name_len(m7_i3)+name[16]（载荷尾 40..56，36..40 保留零）`；pm 生产点 identity.name 填充摘 [DEFERRED: D-16]；vfs 消费臂同步解构；types 306/pm 413+11/vfs 522 全绿 + 字节位 pin 测试；全账见 new_edge3 NS10 行与 FIXLOG edge3 #140 |
| C-17 | devman 客户端生产传输 SysClientTransport | edge3 | 🔄 登记即动 | 携带（非启动链，随 E-DMWIRE 通电波） |
| C-18 | 对话臂 magic grant（GrantTable::grant_magic） | edge3 | 🔄 登记即动 | 携带（VFS REQ_READ/STAT 臂消费，NS4/NS5 之后） |
| C-21 | fs-rt crate 成员 | edge3 | 🔄 卡D 开工 | 携带 |
| C-22 | MIB 取表 wire 权威扩展 | edge3 | 🔄 卡I 开工（kernel 半 C-25 已销） | 携带余段 |
| C-23 | termios wire + tty ioctl 面 | edge3 | 🔄 卡H 批次二十五 | 携带 |
| C-102 | `minix_sys` 再导出 `Strerror`，让三线 `find` 的走表告警能落 C 的 `strerror(号)` 句子 | qoder 三线（todo-fix 循环，`claim/PROBE-ERRNO-CONTRACT-qoder`） | 🔄 登记即动（本批交付见批次二百七十五那笔） | **已随批次二百七十五交付**：`os/libs/minix-sys/src/lib.rs` 加一行 `pub use minix_types::Strerror;`（与既有的 `pub use minix_types::Errno;` 同段同理：二线持有 errno 表与 C 完整形状的权威，三线经再导出通道取用）。刻意不选的方案：在三线另抄一份句子表，或在壳里 `match` 号到字符串——都会把 `compat_errlist.c` 的真源抄成第二份（本项目在批次二百三十三前后已为此立过规矩）。三线未新增对 `minix-types` 的直接依赖，命令层边界守卫零违规 |
| C-24 | smoltcp 外部依赖引入 | edge3 | 🔄 卡E 批三开工 | 携带 |
| C-26 | sffs 语义核心入库 | edge3 | 🔄 卡F3d 开工 | 携带 |
| C-27 | 全系统自举载体 | edge3 | ✅ 2026-09-21（zcode_glm_4，91d3fa024/merge b5fe7e2fa）| **并入 new_edge1 NK1 收口**——真机 test-sysboot PASS（六标记双 DONE）+ run_all 已接线（特殊协议脚本段） |
| C-28 | srv_fork 子 PRIV_PROC 保留缺失【PM 类型设计裁决】 | edge3 | ✅ 已修 2026-09-20（qorder_3，`.review/zcode/edge3/FIXLOG.md` Fix #123） | `Privilege::Kernel(Credentials)` 承载凭证，`is_kernel_process()==true`；偏差钉翻转正断言（os/tests/srv_fork.rs） |
| C-29 | boot loader per-process 地址空间/栈 | edge1 面 | ☐ 待认领 | **改编号 new_edge1 NK1**（设计裁决 OQ-N6 先行） |

### 新增认领（2026-09-20 扫描收敛产生）

| 编号 | 条目 | 线 | 触碰文件 | 状态 |
|---|---|---|---|---|
| C-30 | E-BOOTMODS boot-shim 清单半 | new_edge1 | `os/boot-shim/src/loader.rs`（MODULE_NAMES 扩 12 项对 C `table.c:44-64` image[] 序 = `ds,rs,pm,sched,vfs,memory,tty,mib,vm,pfs,mfs,init` + 删虚假"顺序无关"注释 + 缺件/OOM 带名 fail-fast + 编译期 `assert!(len==12)`） | ✅ 随 NK4（qorder_1，2026-09-20，boot-shim 14 单测+kernel 787 全绿，clippy 9→9） |
| C-31 | E-CONSOLE 驱动半 | new_edge2 | `os/drivers/tty/tty/`（输出后端）、`os/drivers/storage/memory/`（进程壳） | ✅ 2026-09-21（NL1 批B 收口，zcode_glm_2，代码 546aef11e / merge 1357689bc——OQ-N3 方案B 串口 backend + 写字节管道 + minix-sys sys_outb/sys_inb；NL6 memory 进程壳 ✅ c0a9b81aa 前闭；video-text backend 维持登记待 display 可见验收面）；VFS 半归 NS7 |
| C-32 | E-IMGPKG xtask 半 | new_edge3 | `os/xtask/src/main.rs`（image/qemu/build 实装） | ✅ 2026-09-21（zcode_glm_1，dfb910f37，NS8 收线——image()/qemu() 实装 + 装机清单三重锁 + /etc 最小集 + mkfs_mfs 装机消费真实播种 EXIT=0；登记 NS8-A〔kernel.elf 生产者缺口→edge1〕/NS8-B〔模块 guest 构建首阻塞点→NL5②/NS12〕见 new_edge3 新登记；NL5③ 的 xtask 拆 invocation 半随 build() 扩全量落了，CI 半与生产开关显式化核验收敛归 edge4/zcode_glm_3 对账） |
| C-33 | NK3 载体注册（共享文件半） | new_edge1 | `os/Cargo.toml`（workspace members 增 test-timer-irq-{riscv64,aarch64}）+ `os/qemu-tests/run_all.sh`（构建/run 段增两名）——NK3 主体在 edge1 所有权内（os/arch、os/kernel、os/qemu-tests） | ✅ 2026-09-20（zcode_glm_1，bd6a910cf——两名已入 members 与 run_all 构建/run 段，真机 PASS ×2；NK3 收线） |
| C-34 | RS `crate::boot` 路径 bug（宿主 `--workspace --bins` 首停点） | **new_edge3** | `os/servers/rs/src/main.rs:39`（`crate::boot::parse_rs_verbose` → `minix_rs::boot::…`；edge3 f5fcef73f 今日引入）——由 new_edge1 NK5 排查发现，非 edge1 所有权，仅登记待 edge3 认领 | ✅ 2026-09-20（zcode_glm_2，aec8ea361——一行改经 `minix_rs::boot::`；docker 宿主 `-p minix-rs --bins` Finished + test ok + clippy 零新增；NK5 的 C-34 前置清空） |
| C-35 | **共享工作树踩踏事故（2026-09-20 22:26）**：NK5 会话在单工作树下 `checkout`+`reset` 到 `claim/NK5-qorder_1`，摧毁两组未提交在制品——① C-28（qorder_3，`Privilege::Kernel(Credentials)` 类型重设计，review 已验语义贴 C/406+19 测试全绿，diff 仅存于审查会话）② NL6（qorder_2，memory 驱动进程壳半成品）。处置：C-28 重建前先 `claim.sh` 复核锁仍在；NL6 由 qorder_2 重做；§1 规则 7 补执行纪律——领取后开工前必须 `git worktree add`（各线一树），共享树内禁 checkout/reset。**处置已硬化为机制（2026-09-20）**：`tools/claim.sh` 升级——claim 自动建 `.wt/<id>-<owner>/` 专属树、新增 `verify` 开工自检、`release` 连树回收（脏树拒绝）；§1 规则 7 改写为机制版；new_edge1-3 头部领取规则同步；claim.sh/new_edge2.md 首次纳入 git 跟踪（此前未跟踪 = reset 一下就没了） | — | 全仓（流程） | ✅ 机制半落地（文档+脚本已 commit）；遗留：C-28/NK3/NL6 三个在制 claim 尚无专属树，各持有人下次开工前补 claim/建树 |
| C-36 | 规则 7/8 机制化改点登记（共享文件 `tools/`，照规则 3 补登） | new_edge4（编排） | `tools/claim.sh`（claim 自动建 .wt 专属树 + verify 自检 + release 连树回收）、`.gitignore`（补 `.wt/`——原只在 `.git/info/exclude`，不随 clone 走）、new_edge1-4.md（领取规则/§1 规则 7 改写 + 新增规则 8 在制品即提交 + C-35 处置更新） | ✅ 2026-09-20（实测 claim/verify/release 全周期通过后 commit） |
| C-38 | aarch64 ACPI 平台发现的 GICR 空洞 | **new_edge2** | `os/libs/minix-platform/src/acpi.rs`（MADT GICC offset 48 读 GICR Base——QEMU/AAVMF 恒填 0，v2 默认下同 offset 是 GICV 槽 0x0803_0000）：GICR=0 时应报错拒绝静默降级或加已知布局回退；真机证据（ESR 0x96000050/FAR 0x08030014 外部中止）见 new_edge1 新登记 NK3-A 与 .review/zcode/edge1/FIXLOG.md Fix #2 | ✅ 2026-09-20（zcode_glm_1，9b4b0c5b8——采「报错拒绝」：parse_madt walk 后校验 `check_gic_madt`，GICD version 字节 1/2 报 GicVersionUnsupported（对齐 DTB 路径只认 arm,gic-v3）、version≥3 而 GICR=0 报 GicrNotFound，Err 走 init_from_kinfo 下一 source/QemuVirt 回退；硬编码布局回退方案否决（掩盖固件缺陷、不可移植）。minix-platform 14 测试绿 + clippy 9→9 零新增 + aarch64-unknown-none check Finished。**遗留：真机 AAVMF 复核（NK3-A 验收标准）待 QEMU 空闲窗口**，NK3-A 行续踪） |
| C-37 | NK5 共享文件半（X-2 隔离的接线点，照规则 3 登记） | new_edge1（zcode_glm_2） | `os/Cargo.toml`（test-kernels 成员段注释 fw-* 特性词汇表）；`os/qemu-tests/run_all.sh`（三 arch 构建循环成对加 `--features fw-{x86-uefi,aarch64-uefi,riscv64-none}`）；`os/qemu-tests/test-{rt-birth,rt-birth-aarch64,rt-birth-riscv64,sysboot,riscv64-uboot}.sh`（自带 cargo 调用加 `--features`；rt-birth 三目标共用包用 `fw-user-none`）；`.github/workflows/qemu-tests.yml:84`（加 `--features fw-riscv64-none`）。实际交付另有 `os/boot-shim/Cargo.toml`（bin 门 fw-x86-uefi，本线所有权内） | ✅ 2026-09-20（zcode_glm_2，1e2abdced，与 NK5 同分支交付；核心门 GATE-EXIT=0 实证） |
| C-39 | rt-birth panic 注释一行同步（NL3② 的跨界触碰，照规则 1 登记） | new_edge1 面（登记人 new_edge2 zcode_glm_3） | `os/qemu-tests/test-kernels/user/rt-birth/src/main.rs` 强制 panic 处注释——"the process then spins, which is the handler's contract"：NL3② 已把 panic 尾改为经 `minix_sys::exit(1)` 终止（C panic.c:54 `_exit(1)` 对位），裸镜像无 PM 时退出发送失败在传输侧驻留，载体观测不变；仅同步注释的契约句 | ✅ 2026-09-20（zcode_glm_3，随 NL3 合入 72f1cf70f 销账——rt-birth 注释契约句已同步） |
| C-40 | `NR_BOOT_MODULES` 单一真源的跨界触碰（NK7，照规则 1 登记） | new_edge1（qorder_1） | `os/libs/minix-boot/src/kernel_info.rs`（新增 `pub const NR_BOOT_MODULES: usize = 12`，邻 `BootModule`）。**确权结论见状态列：minix-boot 归 new_edge1**——它是 boot-shim↔kernel 交接契约库（`KernelInfo`/`BootModule`/`BootShim`/`NR_BOOT_MODULES`），全部消费者（`boot-shim`/`kernel`/`arch`/`qemu-tests`）都在 edge1 所有权内，`os/libs/` 只是 Cargo 目录约定、非归属线索；edge2 的 `os/libs/` 认领已改为以括号枚举为准（minix-boot 不在枚举内），edge1/edge2 所有权头同步更新。另两侧代码改动均在 edge1 所有权内：`os/kernel/src/proc.rs`（自有字面量→`pub use minix_boot::NR_BOOT_MODULES`）、`os/boot-shim/src/loader.rs`（`const assert` →`==NR_BOOT_MODULES`）。无新增 Cargo 依赖（kernel/boot-shim 早已依赖 minix-boot），不碰 `os/Cargo.toml`。当前无并发 claim 触碰 minix-boot/proc.rs/loader.rs（已核验 `.wt/*` 脏态） | ✅ 代码半 2026-09-21（qorder_1，35fc2d03b；负例改 13→boot-shim `E0308` + kernel `expected size 13 found 12` 双编译失败已回退；boot-shim 14/minix-boot 17/kernel 787 全绿，clippy 零新增）。✅ **归属确权半 2026-09-21（qorder_1 依用户指示裁决）**：minix-boot 判归 **new_edge1**（boot 契约线），edge1 所有权头补入 `os/libs/minix-boot`、edge2 声明 `os/libs/` 以枚举为准且明示排除 minix-boot——跨界空档消除，C-40 销账 |
| C-41 | NS2 的跨界触碰：内核 grant/copy 路径端点哨兵权威化（照规则 1 登记） | new_edge1 面 `os/kernel/src/grant.rs` + `os/kernel/src/syscall_copy.rs`（登记人 new_edge3 zcode_glm_4） | NS2（E-RPROCTAB）核对门语义发现：两文件的 `SELF=-2/NONE=-1/ANY=-3` 本地哨兵与端点权威不符（minix-types endpoint.rs:48-49 / C endpoint.h:51-56+com.h:55 同源推导 SELF=31742/NONE=31743/ANY=31744）——who_to=ANY 的授权（rproctab 正是 `cpf_grant_direct(ANY,…)`，C main.c:185）会在 verify_grant 三处 grantee 门（grant.rs:324/:355/:388）被 EPERM 拒；umap_remote/datacopy 的哨兵比对（syscall_copy.rs:273/:879）同病。修法：删本地错值，改由 `minix_types::Endpoint::{SELF,NONE,ANY}` 派生（may_create_magic_grant 已有 Endpoint::VFS/MIB 单一权威先例）+ 宿主权威值测试。**clock 的同族残留（syscall_clock.rs:69 SELF=-2，实断：ipc-server/is 传 Endpoint::SELF.0 而内核比对 -2）不属 grant/copy 面，另见 new_edge1 新登记 NK8，不顺手修**。已核验无并发 claim 在 os/kernel（.wt 四树均为 edge2/edge3 面） | ✅ 2026-09-21（zcode_glm_4，随 NS2 代码 5917f2d51 / merge 2ed5278e6 销账：两文件哨兵权威化 + `test_grantee_gate_any_authority_value` 权威值 pin；kernel 788 全绿，clippy 120 与基线零新增） |
| C-42 | NS8-A 共享文件半：workspace 成员 + xtask 取件缺省名（照规则 3 登记） | new_edge1（zcode_glm_1） | `os/Cargo.toml`（members 增 `kernel-image` 生产内核镜像 bin 包，X-2 注释段词汇表内）；`os/xtask/src/image.rs`（`kernel_elf_path` 缺省文件名 `kernel.elf`→`kernel`——cargo 对 none 目标产无扩展名工件，ESP 侧安装名 `/EFI/minix/kernel.elf` 契约位不变；注释与测试同步） | ✅ 2026-09-21（zcode_glm_1，随 NS8-A 代码 d3725d1e8 / merge b564256ca 销账：xtask 10 测绿——含 15 步构建对账契约测试更新；NS8-A 全账见 new_edge3 本轮新登记行） |
| C-43 | NK8 的跨界触碰：minix-sys `sys_times` 过时注释更正（照规则 1 登记） | new_edge2 面 `os/libs/minix-sys/src/syscall.rs:314`（登记人/执行人 new_edge1 NK8 会话 zcode_glm_4） | NK8 注册范围内一处注释更正：`endpt == SELF`（**-1**）数值错且与同文件 :288 常量（-2）自相矛盾——改述为 endpoint.h:56 哨兵、权威推导见 `minix_types::Endpoint::SELF`，不落任何具体数字（:288 的 `-2` 值缺陷不在本次触碰内，另见 new_edge2 新登记 NL9）。仅注释行无行为变化；触碰前核验：NL4（qorder_3）在 minix-sys 的在制增量在顶层 API 区，与注释区不同 hunk | ✅ 2026-09-21（zcode_glm_4，随 NK8 代码 6f13f0936 销账；minix-sys 250 全绿） |
| C-44 | NL4 余量批：minix-sys 顶层 API 清单后半（NL4 收口转登记，2026-09-21 qorder_3） | new_edge2 | getcwd、isatty、utime/utimens、pathconf、getppid/times、getpriority、sync、brk/sbrk、whoami（NL6 tty 子项依赖在此）、mount/umount、reboot/shutdown、socket 族（17-stage 域）+ 既有 module-only（dup2_via/lseek_via/getpid_via/nanosleep_via 等）顶层化余件；每件配 Canned 回放测试。**逐件 server 就绪度未核**（侦察已知：mount/umount 臂 Nosys fail-closed、utimens 臂未见、isatty 走 ioctl 通道需查 C、getcwd 通道未查），认领时先核再排 | ✅ 2026-09-21 qorder_3 收口（de0f51712：新 via 5——sync/utimensat/futimens（VFS_UTIMENS 八 lane，C 客户端三面守卫复刻）+getppid（本仓凭证族 m1i2=parent 约定，divergence 注释钉死）/get+setpriority（PRIO_MIN 校正在 libc 层同 C）；顶层 19——getppid/getuid/geteuid/setuid/setsid/get+setpriority/sigpending/sigprocmask/raise/dup2/lseek/nanosleep/sync/utimensat/futimens/utimes/futimes/utimens/isatty；测试 +12；minix-sys 274 全绿、vfs410/pm506/vm522 回归绿、clippy 新代码位零警告、rebase 跨过 NL9 syscall.rs 零冲突。**侦察裁决**：isatty/utimens/pathconf 类为 C 用户态组合无新 wire；**余量转 C-46**） |
| C-45 | VFS server 侧两缺陷（NL4 高优先批侦察发现，照"不顺手修"纪律登记，待 edge3 认领） | **new_edge3** 面 `os/servers/vfs/src/syscalls.rs` | ① **fchmod mode lane 与 C 失配**：C `mess_lc_vfs_fchmod { int fd; mode_t mode }`（ipc.h:647-652，`__mode_t`=4B，ansi.h:41）LP64 落 fd@0、**mode@4**；我方 Fchmod 臂（syscalls.rs:1064 附近，注释自述 mode@8）按 **@8** 读——若未来 fchmod 客户端落地将静默读零。仓内闭环测不出（无客户端消费）。② **Readlink 臂窗口边界**：臂检查 `name_len <= OPEN_PATH_INLINE_MAX(32)` 但内联窗 `raw[32..]` 只有 24 字节（负载区 56），25..=32 会落入 `decode_name` 截断；我方客户端已按 READLINK_INLINE_MAX=24 严设（minix-sys vfs.rs），仓内不可达，服务端宜同设 24 或补注释。②的锚点：minix-sys `vfs.rs` READLINK_INLINE_MAX 注记互见 | ✅ 2026-09-21（zcode_glm_4，代码 1ccf8dd64 / merge a011f6325）：① fchmod mode lane `raw[8..12]`→`raw[4..8]`（C `mess_lc_vfs_fchmod` fd@0/mode@4，mode_t=4B；测试构造 @8 钉 0o777 干扰值，错位读取即断言失败）；② Readlink 窗门 `OPEN_PATH_INLINE_MAX(32)`→`READLINK_INLINE_MAX(24)`（56−32 内联窗，与客户端同门）+ 新窗门测试（namelen=25+窗内 NUL → 门上 ENAMETOOLONG，旧门会放行截断名；24 整窗仍走遍历）。**跨界触碰**：minix-sys `vfs.rs` readlink_via 文档注释单行同步（差异登记句移除——双侧同门；与 C-46 在制区无 hunk 交集）。范围核对：Chmod path 半 `mess_lc_vfs_path`（mode@20）与 C 一致无恙。docker：基点树 vfs 515 全绿（514+1）/ 合并 rewrite（含 NS5）复测 **vfs 521 全绿** + minix-sys 274 全绿；clippy lib 28=28 零新增（--all-targets 对改动区零命中）；rustfmt 新增区零命中（存量漂移未动）。FIXLOG edge3 #137 |
| C-46 | C-44 收口后余量（2026-09-21 qorder_3 逐件双侧侦察裁决） | new_edge2 | **server 阻塞**：sigaction/sigsuspend 顶层化（裸 sigreturn 桩地址依赖 edge3 S3 投递臂通电，via 已就位）；mount/umount（VFS server Nosys fail-closed，待 W1 transport）。**通道待建**：getcwd（C 是 NetBSD 用户态 stat/devfind 遍历，`minix3/lib/libc/gen/getcwd.c`，无专用 syscall，需整机制移植）、times（C=getrusage×2+gettimeofday 组合，`gen/times.c`；PM GetRUsage 臂已通但需 rusage 回写 struct 拷出设计+CLK_TCK 换算）、pathconf/fpathconf（纯用户态常数组合，`sys/fpathconf.c`，可做低值）。**归属非本线**：brk/sbrk（C 依赖 libc 全局 `_brksize`，与 minix-rt 堆管理冲突，属 rt 层）、whoami（非 syscall——C 无此调用，NL6 tty 子项实为驱动自端点查询，属 driver-rt/RS 域，C-44 清单表述纠正）、reboot（PM 臂就绪但 init 域低值，C `reboot.c` 四行可做可不做）、shutdown/socket 族（17-stage 域）。另记：`cargo test --workspace` 组合下 minix-kernel dev-profile 三错（E0308×2+MockDirectMap::default E0599）为**预存基线**（主树 58fa0f15e 无改动同错，feature 统一触发），逐 crate 跑全绿不受影响，归属 kernel 测试线。🔶 2026-09-21 zcode_glm_2：**getcwd 件收口**（代码 32ebaeff4，claim/C-46-zcode_glm_2）——真身勘误：机制在 `minix3/minix/lib/libc/sys/__getcwd.c`（stat/opendir/chdir 用户态攀升，无内核 syscall；原登记的 gen/getcwd.c 是 NetBSD wrapper+realpath，`__getcwd.S` 仅存 Makefile 残引无实现，模式 70 CTOS）。minix-sys 新增 `getcwd` 模块：`getcwd_via(transport, buf)`+顶层 `getcwd(buf)`，CwdSyscalls seam（生产 WireCwdSyscalls 走既有 via 面，测试 FakeFs 注入）；errno 面 EINVAL/ERANGE/ENOENT 与失败即 recover 复原逐项对齐；**继承缺口如实登记**：本线路径族内联窗 32 字节且服务端拒绝超长名（C 指针+长度恒发、服务端 fetch 未接），>31 字节父项跳过——服务端长名 fetch 臂属 VFS wire 面，未顺手修。验证：docker minix-sys 285 全绿（基线 274+11）、vfs 506 回归绿、clippy getcwd 归零（总 40=存量）、rustfmt 单文件合规、x86_64-unknown-none 宿主 check Finished。**余件保持**：sigaction/sigsuspend（server 阻塞）、mount/umount（W1 transport）、times（rusage 回写设计）、pathconf/fpathconf（低值待领）。✅ 2026-09-21 zcode_glm_1：**pathconf/fpathconf 件收口**（代码 be0354331，claim/C-46-zcode_glm_1，merge 进 rewrite）——minix-sys 新增 `pathconf` 模块：9 个 `_PC_*` 变量号钉 `sys/sys/unistd.h:198-206`、常数答案钉 `sys/sys/syslimits.h:54-68`（**__minix 分支 PIPE_BUF=32768**，非通用 512）+ `unistd.h:95/:119/:159`（_POSIX_VDISABLE=255）；`_PC_LINK_MAX` 走 fstat 既有 via 面（目录答 1/文件答 LINK_MAX，fpathconf.c:29-35；stat 失败 errno 传播），其余纯常数零新 wire（C-44 侦察裁决兑现）；`pathconf` 顶层保持 C 时序（open O_RDONLY → 查询 → close，close 返回值忽略，pathconf.c:37-45）；`FpathconfSyscalls` seam（getcwd.rs 判例）+ 顶层双函数与 9 常量再导出（NL7 termios 判例）。验证：docker minix-sys **304 全绿（基线 300+4 新测**：目录/文件分叉+失败传播、常数表、未知名 EINVAL、变量号 pin）/clippy 38=38 零新增/rustfmt 新增区清（lib.rs 10 hunk 全为预存 reorder_modules 漂移未动）。**余件保持**：mount/umount（W1 transport）。🔶✅ 2026-09-21 zcode_glm_1：**times 件收口**（代码 3d6279b86，claim/C-46-times-zcode_glm_1）——minix-sys 新增 `time` 模块：`times` = getrusage(SELF)+getrusage(CHILDREN)+gettimeofday 用户态组合（C gen/times.c:65-92 对位，NL4 判例"纯用户态组合无新 wire"兑现：GetRUsage=36 臂、kern.clockrate sysctl 通道、PM_GETTIMEOFDAY=28 均已在库）；`getrusage_via` 载荷 mess_lc_pm_rusage（who@0/addr@8，PM decode::rusage 对位）；clk_tck 走 sysctl {CTL_KERN,KERN_CLOCKRATE}（C sysconf.c:104-117 同路；函数级 static 缓存=纯优化，本面每调用读，偏差登记）；CONVTCK 整除序逐字对位；Rusage 128B 视图=PM RUSAGE_LEN（nvcsw/nivcsw 窗外省略）；hz<=0 拒 EINVAL（C 除零 UB 的诚实化）。**随单发现并修 C-54**：PM 时间应答 nsec 车道回归 C 形（@16→@8）。验证：docker minix-sys **313 全绿（基线 304+9）**/pm **416 全绿（415+1）**，clippy 38=38/33=33 零新增，rustfmt 新增行合规。🔶✅ 同日 zcode_glm_1：**sigaction/sigsuspend 件收口**（claim/C-46-sigact-zcode_glm_1）——登记阻塞前提过期核实（模式 70 CTOS）："裸 sigreturn 桩地址依赖 S3 投递臂通电"已被 S3 ✅（2026-09-19）+ NL3③ `__sigreturn` 真符号（72f1cf70f）+ NS11 init 消费先例（94c2e96a3）推翻。交付：①顶层面落 **minix-rt::signals**（方案裁决：C 的 libc 单一单元同时持有 wrapper 与 `__sigreturn`，minix-rs 分层 rt→sys 桩符号在 rt——`sigaction`/`sigaction_with` 填真桩地址、`sigsuspend`/`sigsuspend_with` 走直传；x86_64 门控=模块既有门，无桩架构无正确 ret 值不设面）；②minix-sys `sigsuspend_via` C 化删死参（PM `_ctx` 忽略、C sigsuspend.c:11-17 只发 .set；零消费点）；③两处过时文档句同步（rt 模块头"init 填 0"→NS11 已填真符号、pm.rs sigaction_via "仍传 0"→地址由调用方传入）。验证：docker rt **57 全绿（55+2）**/sys **313 保持**，clippy 10=10/38=38 零新增，rustfmt 新增区清。**C-46 全部余件仅剩 mount/umount（W1 transport 阻塞）** | 🔶 |
| C-48 | NS5 的跨界触碰：minix-types 增 `VfsCall::decode` + `VfsReply::encode`（照规则 1 登记） | new_edge2 面 `os/libs/minix-types/src/ipc/vfs.rs`（登记人/执行人 new_edge3 NS5 会话 zcode_glm_3） | VFS 侧 PM 控制面接线（Route::Pm 臂）需要 `VfsCall::decode`（encode 的互逆，service_pm 载荷半；VFS_PM_INIT 仍走 VfsPmInit::decode），PM 回复消费需要 `VfsReply::encode`（decode 的互逆，C `reply()` 的 memset 裸回复语义 + com.h:570-575 槽位）。各配往返测试（decode 族内/族外拒收 + Exec encode→decode 全字段）。触碰前核验：minix-types 无并发在制 claim | ✅ 2026-09-21（zcode_glm_3，随 NS5 主线 19be8bc92 销账；minix-types 304 全绿） |
| C-49 | NS5 的跨界触碰：minix-sys 增 `sys_memset` wrapper（照规则 1 登记） | new_edge2 面 `os/libs/minix-sys/src/syscall.rs`（登记人/执行人 new_edge3 NS5 会话 zcode_glm_3） | exec 的 clearmem 回调（exec_elf.c:294-309 经 `libexec_clear_sys_memset`，exec_general.c:66-71）需内核侧整块填充：C libsys `sys_memset`（sys_memset.c:3-16，SYS_MEMSET = com.h:221，kernel `Syscall::Memset=13` dispatch_memset 在位）的 Rust wrapper——`m_lsys_krn_sys_memset` 载荷臂（minix-types 已有）+ bytes==0 短路 OK。触碰前核验：qorder_3 的 NL4 在制同文件（claim/NL4-qorder_3，顶层 API 区），与 sys_datacopy 邻域的 wrapper 落点不同 hunk；NL4 收口批（C-44，de0f51712）后于本触碰合入，git 零冲突实证 | ✅ 2026-09-21（zcode_glm_3，随 NS5 主线 19be8bc92 销账；minix-sys 274 全绿） |
| C-47 | NS8-B 的跨界触碰：minix-rt crt0 Consumer contract 消费者注记 + std 形态 dead_code allow（照规则 1 登记；初登号 C-46 与 qorder_3 撞号让位） | new_edge2 面 `os/libs/minix-rt/src/crt0.rs`（登记人/执行人 new_edge3 NS8-B 会话 zcode_glm_2） | ①Consumer contract 节 Reference consumer 括注补：NS8-B 起 11 个 boot 装机模块同款 none 侧入口经 `servers/*/src/main.rs` 双形 main 落地、init 签名半随 NS11——纯文档注释；②模块级 `#![cfg_attr(feature = "std", allow(dead_code))]`：std 形态无 `_start` 出生链，crt0 私有部件（PsStringsRaw/read_process_strings/progname_of/birth_fail/argv_storage_of）自然闲置，pub 访问器（argv_count/argv_bytes/progname，init 宿主半消费）不受影响——每消费 crate 派生 5 条 dead_code 噪声的源头消除；触碰前核验 NL9（zcode_glm_4）在制于 minix-sys（同线异 crate，无 hunk 交集） | ✅ 2026-09-21（zcode_glm_2，随 NS8-B 交付同批 commit 销账） |
| C-50 | NS5-A 执行的跨界触碰：VM_MMAP 线束（MessMmap）64 位车道统一（照规则 1 登记） | new_edge2 面 `os/libs/minix-types/src/ipc/message.rs`（MessMmap 结构）+ `os/libs/minix-types/src/ipc/vm.rs`（VmMmapIn::decode_message / VmMmapOut 回程编码）+ `os/libs/minix-sys/src/vm.rs`（mmap_via/munmap_via）；new_edge3 面 `os/servers/vm/src/ipc/encode.rs`（Mmap 回程臂）+ `os/servers/vfs/src/exec_worker.rs`（vm_mmap 发送臂）（登记人/执行人：zcode_glm_2，认领 new_edge3 新登记 NS5-A） | 真机 wire 缺陷三合一：①exec 栈映射 `as u32` 截断（exec_worker.rs:887-888，栈顶缺省 0x7fff_ffff_f000 必中）；②minix-sys mmap_via 的 MapPayload（64 位车道）与 VM decode（u32 车道）双方言并存，mfs 块缓存/ipc-server sem 两活消费方真机必乱；③回程 VM 写 m1p1@16（minix-types vm.rs:1055）而 C 写同一 m_mmap overlay 的 retaddr（mmap.c:191/:276）、mmap_via 读 @40——三车道互斥。**登记行 LP64 前提勘误**：`_ASSERT_MSG_SIZE`（ipcconst.h:17-19）钉载荷恒 56 字节，LP64 布局 72 字节断言必炸——C 头只有 i386 形态（addr/len/retaddr 32 位），"加宽至 C 布局"不可能，64 位车道是仓内既有裁决（m_vm_pagefault / E-VMMCPWIRE / MapPayload 三先例）。修法：MessMmap 统一为 offset@0/addr@8/len@16/prot@24/flags@28/fd@32/forwhom@36/retaddr@40/pad@48（56 字节不变量保持，内核 IPC 不动），三端对齐。触碰前核验：主树无 NS5-A/NL 批在制 claim 触碰上述文件（`tools/claim.sh list` 仅 NK2-A 已收口/NS9-rs 面无交集） | ✅ 2026-09-21（zcode_glm_2，代码 142a006fc / merge d78833955 销账：五端 64 位车道对齐 + 测试 305/287/522/522 全绿 + clippy 基线对账零新增 + doc 20 同步 + FIXLOG edge3 #138；NS5-A 行同步标 ✅ 含 LP64 勘误） |
| C-53 | NS11-B 执行的跨界触碰：MIB_SYSCTL 交换面 64 位车道三向统一（照规则 1 登记；**初登号 C-52 与 NL10 会话撞号让位**——C-52 已先落主线归 NL10/zcode_glm_2，C-47 判例，本行改号；NS11-B 代码提交信息与 FIXLOG #141 内的 "C-52" 字样均指本行） | new_edge2 面 `os/libs/minix-types/src/ipc/message.rs`（MessLcMibSysctl overlay）+ `os/libs/minix-types/src/ipc/mib.rs`（SysctlRequest/SysctlName/DataSink/NewData 视图类型）+ `os/libs/minix-sys/src/sysctl.rs`（sysctl_via 客户端）+ `os/libs/minix-sys/src/misc.rs`（sysctl_name_fits_inline 门）；new_edge3 面 `os/servers/mib/src/server.rs`（sysctl_arm）+ `os/servers/mib/src/dispatch.rs`（classify_name 门）；文档 `rewrite-notes/10-stage-mib/02-mib-message-contract.md`（D1 决策行执行）（登记人/执行人：zcode_glm_1，认领 new_edge3 新登记 NS11-B——NS11-A 同人续作） | 修法＝登记候选①：56 字节内 oldp/oldlen/newp/newlen/namep 五 lane 全 64 位（LP64 自然宽度，NS5-A MessMmap 字段宽规则），内联名窗 name[8]→name[2] 换预算（恰 56 无补位）；窗常量唯一真源挂 overlay（`MessLcMibSysctl::INLINE_NAME_COMPONENTS`），minix-sys fits_inline 与 mib classify_name 双侧派生防漂移；REGISTER 面（`mess_lsys_mib_register` 的 `mib[8]`/rmib）零触碰——无地址车道，维持 C 32 位形与 `CTL_SHORTNAME=8`。触碰前核验：NL1（zcode_glm_2）在制 minix-sys/syscall.rs（devio wrapper）与本单 sysctl.rs/misc.rs 不同文件零 hunk 交集；message.rs 已随 NS5-A 排空（merge d78833955）；mib server 无在制 claim | **✅ 2026-09-21 销账（代码 16da674e5 / merge 7009d1d19；🔄 2026-09-21 zcode_glm_1，claim/NS11-B-zcode_glm_1 @ .wt/ns11-b-zcode_glm_1）：types 305/sys 298/mib 176（+2 全宽地址回归钉）/init 147 全绿；clippy 基线对账 2=2/5=5/5=5 零新增；guest check Finished（宿主 ulimit 腿，docker 无 target）；doc 同步 4 处 + doc-lint error=0；FIXLOG edge3 #141（含过程事故如实记录：clippy 对账 checkout 往返冲掉一次未提交在制，自会话记录重建并落实规则 8）——真机半挂 T2/E5(g)；应答 lane oldlen u32 维持（节点长度不可达 4 GiB，注记在 overlay doc）** |
| C-51 | NS11-A 执行的跨界触碰：minix-sys sysctl 客户端面新增 + init host.rs mib 三件接线（照规则 1 登记） | new_edge2 面 `os/libs/minix-sys/src/sysctl.rs`（新模块，getcwd.rs 判例：seam 拆分 + `_via` 面 + Canned 形状测试）+ `os/libs/minix-sys/src/lib.rs`（模块声明 + 顶层 sysctl/sysctlnametomib/sysctlbyname + minix-types sysctl 常量点名再导出，NL7 termios glob 判例）；new_edge3 面 `os/commands/sbin/init/src/host.rs`（mib 三件 ENOSYS → 真通道）+ `os/commands/sbin/init/src/sysctl.rs`（模块 doc 的 ENOSYS 前提句同步）（登记人/执行人：zcode_glm_1，认领 new_edge3 新登记 NS11-A——NS11 ◐ 持有人同线收口余件） | minix-types 本单零触碰（MessLcMibSysctl overlay 现状消费；其 u32 地址车道对 5 GiB guest 的截断缺陷另登记 new_edge3 NS11-B——修法候选①内联名窗 8→2 腾 64 位车道，触碰 minix-types/mib server/minix-sys 三向，message.rs 已随 NS5-A 排空、开工照 C-50 判例领板）。触碰前核验：C-44/NL9 已排空 ✅（NS11-A 登记行的等待条件满足）；`tools/claim.sh list` 在制 NS9（rs 面）/NS5-A（已收口）/NK1（kernel 面）与本单文件无交集 | 🔄 2026-09-21 zcode_glm_1（claim/NS11-A-zcode_glm_1）**✅ 2026-09-21 销账（代码 6c66a63af / merge 94c2e96a3；minix-sys 296/init 147 全绿，合并态 sys 298 保持；clippy stash 基线对账 37=37、59=59 零新增；FIXLOG edge3 #139）——minix-types 零触碰兑现，NS11-B 车道缺陷留板待领** |
| C-52 | NL10 执行的跨界触碰：VFS 路径臂长名 fetch 半 + doc 同步（照规则 1 登记） | new_edge3 面 `os/servers/vfs/src/path.rs`（新增 `copy_path` 共享 helper：C utility.c:17-43 双支对位——≤`OPEN_PATH_INLINE_MAX` 内联 decode、超长 `SysPathFetcher::fetch`）+ `os/servers/vfs/src/syscalls.rs`（八臂改线：open/mkdir/chdir\|chroot/unlink\|rmdir/chmod/access 补 fetch 支〔C copy_path 族：open.c:49/open.c:575/stadir.c do_chdir do_chroot/do_chmod/protect.c:221/link.c do_unlink〕；**creat 臂改恒 fetch**〔C do_creat open.c:72-74，wire 无 buf 字段——现臂从客户端恒零填充的 padding 区解内联路径 = 真 wire 缺陷〕；**readlink 臂改恒 fetch + 拆自造 24 字节内联窗**〔C do_rdlink link.c:494，wire 无 buf〕）+ `rewrite-notes/14-stage-runtime/09-vfs-syscalls.md`（:78/:103 "C loadname 超长静默跳过 strcpy 属未定义行为"系事实错误——loadname.c:15-17 恒写 len+name、utility.c:31-32 超 40 fetch，一并纠正）+ `99-global-concepts.md`（:66 内联裁决行同步）+ `os/libs/minix-sys/src/getcwd.rs`（模块头 Known platform gap 段——限 lifts 后过时）（登记人/执行人：zcode_glm_2，认领 new_edge2 NL10——client 半在本线所有权内） | 触碰前核验：`tools/claim.sh list` 在制 NK1（kernel 面）/NS10（pm 面）/NS11-B（minix-types message.rs + minix-sys **sysctl.rs** + mib server）与本单文件（minix-sys **vfs.rs** + vfs server 两文件）无 hunk 交集；NS11-B 同 crate 异文件照 C-47 判例可并行；vfs server 无在制 claim（NS4-A 已收口 3fa899ba6） | 🔄 2026-09-21 zcode_glm_2（claim/NL10-zcode_glm_2）**✅ 2026-09-21 销账（代码 19bccd8d7 / merge d58f74f81）**：server 八臂 copy_path/fetch 双支落地（侦察更正：fetch 恒族实为 do_creat/do_rdlink/stat/lstat/truncate/chown/link/symlink/rename/statvfs/mknod——creat/readlink 原臂解内联=真 wire 缺陷一并修）；doc 09 §3.2/§4 勘误落地（99 篇 :66 指针句核实免改）；minix-sys 300/vfs 523 全绿（主树合并态复测保持），clippy/doc-lint/rustfmt/guest check 全清（FIXLOG edge2 NL10 节）。**让位注记**：本编号被 NS11-B 初登撞用，依"先落主线"判例（C-47 判例）归本单，NS11-B 改号 C-53 销账 |
| C-54 | C-46 times 件执行中发现的跨界触碰：PM 时间应答车道回归 C 形（照规则 1 登记） | new_edge3 面 `os/servers/pm/src/ipc/calls.rs`（GetTimeOfDay\|ClockGetTime 臂 :722 与 ClockGetRes 臂 :743 的应答 nsec 车道 `raw[16..24]`→`raw[8..16]` + 臂注释勘误）；new_edge2 面（本线，随单交付）`os/libs/minix-sys/src/time.rs`（新模块 times/getrusage，消费该应答）（登记人/执行人：zcode_glm_1，认领 new_edge2 C-46 余件 times） | 仓内车道失配：C 权威应答 overlay 是 `mess_pm_lc_time`（`ipc.h:1769-1773`：sec@0/nsec@8，LP64 自然宽；C 服务端 `time.c:43-46` 写它、libc 客户端 `gettimeofday.c:18-20` 读它）——minix-sys 现有 `pm::gettimeofday_via`（pm.rs:660）按 nsec@8 解码 = C 对位；而 Rust PM 服务端臂把应答写进**请求 overlay** `m_lc_pm_time` 的 LP64 nsec 槽（@16，calls.rs:722/:743，注释自称"与 C do_gettime 的 mp_reply 同臂"系误认——C 的 mp_reply 车道是 m_pm_lc_time 非 m_lc_pm_time）。后果：真机上现客户端读到 nsec≡0（sec 车道一致故唯一消费点 init host.rs:440 只取 sec 未暴露）；C-46 times 件是首个 nsec 消费方，缺陷升 load-bearing。修法：两臂 nsec 车道改 @8（C 形）+ 注释勘误 + dispatch 车道 pin 测试（镜像 test_dispatch_getrusage_children_roundtrip 线束）；客户端（pm.rs 既有 @8 测试 + time.rs 新组合测试）互为对拍。触碰前核验：`tools/claim.sh list` 在制 NK1（kernel 面）/NS12（commands 面）；pm 面 NS10 已收口（75aaea831）；无并发 claim 触碰 os/servers/pm | **✅ 2026-09-21 销账（zcode_glm_1，代码 3d6279b86）：两臂 nsec 车道改 @8 + 臂注释勘误（mess_pm_lc_time 车道与读写双方锚点）+ ClockGetRes dispatch 车道 pin 测试（pm 416 全绿=基线 415+1，clippy 33=33 零新增）；GetTimeOfDay 臂的 SysClockSource 硬编码内核腿致宿主 dispatch 单测不可达（时钟源无注入缝）——缺口如实登记，车道正确性由同形 getres 臂 pin + minix-sys 客户端互斥测试对拍钉死 |
| C-55 | NL5-CI 的跨界触碰：kernel mock 伞生产开关注释显式化（照规则 1 登记） | new_edge1 面 `os/kernel/Cargo.toml`（`[features]` 段注释，零行为变化——登记 NL5③ 遗留"kernel `default=["mock"]` 伞的生产开关显式化"，C-43 判例注释级跨界） | 把既有语义写显：`default=["mock"]`＝宿主测试形态（`minix-arch/runtime-window` + `minix-plat/mock`）；生产形态＝`--no-default-features`（真 arch/plat，riscv64/aarch64 none target 判例 NK8/NL5②），唯一装机消费点＝`os/test-kernels/kernel-image/Cargo.toml`（`default-features = false`，NS8-A）；`qemu_test` 为载体侧追加位。触碰前核验：`tools/claim.sh list` 在制 NK1（os/kernel 代码面，Cargo.toml 不同 hunk 且其树当前净）/NS12（commands 面）；`os/xtask` 主体＝本线 edge3 所有权免登记 | **✅ 2026-09-21 销账（zcode_glm_3，代码 ac29a7153 / merge c383ed212）**：注释落地 + docker `cargo check -p minix-kernel` sanity EXIT 0；NL5-CI 全账见 new_edge3 本轮新登记行 |
| C-56 | NS12 命令真机 panic 面的跨界触碰：minix-rt panic handler 的 no-hook 分支从 SpinSink 挂死改 SYS_DIAGCTL + exit 阶梯（照规则 1 登记） | new_edge2 面 `os/libs/minix-rt/src/lib.rs`（`panic` handler 的 `run_panic_diagnostic_hook` false 分支，lib.rs:296-302 一带）+ 随单注释勘误 `os/commands/sbin/init/src/main.rs`（"minix-rt 自带 handler 的兜底是 spin"系 A-8 三阶段落地前的过时描述）（登记人/执行人：zcode_glm_2，NS12 命令消费方上会——edge3 所有权内 65 bin 全部取 rt `panic-handler` feature，真机 panic 面由本触碰定性） | 仓内实断：hook registry（minix-types diagnostic.rs `PANIC_DIAGNOSTIC_HOOK`）是**各地址空间私有 static**——kernel 在自己空间注册，用户进程永远查 None；现 no-hook 分支落 `SpinSink::emit`＝死循环（diag.rs:251-257），**stage-3 的 `minix_sys::exit(1)` 永不可达**——真机命令 panic＝无输出挂死整 CPU（65 bin 批二/三 FIXLOG #145/#146 遗留①的实底）。修法（init PID-1 判例 main.rs 底部同款）：no-hook 分支改 `sys_diagctl_write(&DirectKernelCallTransport, message)` 走内核诊断台（kernel 在位即真机可见；宿主/无核形态 transport 失败忽略），随后无条件落既有 `exit(1)`——无 PM 时 transport park 接管（C panic.c:66 挂起同形，契约注释保留）；SpinSink 类型保留（测试/阶段文档引用），handler 不再构造。触碰前核验：`tools/claim.sh list` 在制 NK1（kernel 面）/X-9（drivers 面）/C-46-sigact 已销账释放；minix-rt 无在制 claim（C-46-sigact 的 rt 面 55+2 已合线） | **✅ 2026-09-21 销账（zcode_glm_2，代码 acf044c64 @ claim/NS12-zcode_glm_2）**：no-hook 分支改 `sys_diagctl_write(DirectKernelCallTransport, message)` + 注释钉死每地址空间私有语义与 C panic.c:66 挂起同形；SpinSink 类型保留（staging/测试消费）；随单勘误 init main.rs:178-184 过时注释。验证：docker rt+init 宿主测试 EXIT=0（rt 57 保持）；guest 编译经真实消费面 minix-shell/minix-fileops（panic-handler 路径）x86_64-unknown-none=0 + rt 单包 none 门（alloc-global+panic-handler）=0 |
| C-57 | NS12 冒烟脚本的跨界触碰：`os/qemu-tests/test-cmd-smoke.sh` 新增（照规则 1 登记） | new_edge1 面 `os/qemu-tests/`（新脚本一份；**run_all.sh 本批不碰**——T4 判定面在 NK1 收口前必然 FAIL，接 run_all 与真机 PASS 同批，避免主干挂已知红）——其余触碰面（os/xtask 只调用不修改、os/commands 本线所有权）（登记人/执行人：zcode_glm_2，NS12 余件冒烟脚本） | 脚本职责＝T4 验收契约的载体化：①SKIP 门（qemu/OVMF/mtools 缺件=exit 2，house shape）②镜像装配经 `cargo run -p xtask -- image --arch x86_64 --release`（NS8 面）③ESP 断言（`/EFI/minix/kernel.elf`+`modules/`×12+`imgrd`，mtools 只读校验——无 QEMU 可验）④boot 至 `entering scheduler`（T1 既有 marker）⑤T4 命令输出 marker 窗口（env 可覆写，缺省等 OQ-3 rc 定稿）。串口纪律：QEMU 段本批不跑（NK1 在制占用窗口），本次只验 ①②③ 与 bash -n。核验：qemu-tests 在 edge1 清单内（C-37 判例同目录跨界先例） | 🔶 2026-09-21 zcode_glm_2 登记，随 NS12 批四执行 |

| C-59 | E5(g) 执行的跨界触碰：MIB_SYSCTL 调用号权威化 + sysctl_via 应答判定修正（照规则 1 登记）【⚠️ 撞号让位注记（2026-09-21 zcode_glm_1 二次顺延）：初登号 C-55 归 NL5-CI 的 kernel 伞注释单（341346d3e 先落主线）；注记原指改号 C-57 又被 NS12 冒烟脚本单（157d83ef7）先占——按先落主线判例顺延取 **C-59**。FIXLOG edge3 #145 与 merge 8d7c6ad79 内的 "C-55" 字样均指本行】 | new_edge2 面 `os/libs/minix-sys/src/misc.rs`（`MIB_CALL_SYSCTL` 0x600→`minix_types::MIB_SYSCTL` 派生）+ `os/libs/minix-sys/src/sysctl.rs`（应答判定：非零 m_type 即 errno）+ `os/tests/Cargo.toml`（minix-mib/minix-sef 依赖 + mib_sysctl 目标）（登记人/执行人：zcode_glm_1，认领 new_edge4 §5 E5(g) 宿主半） | 四链联调线束揭出两处仓内 wire 缺陷：①`MIB_CALL_SYSCTL=0x600` 与 minix-types 权威 `MIB_SYSCTL=MIB_BASE+0=0x1800`（C com.h:1022/:1026）相左——NS11-A 客户端的每次 sendrec 都落进服务端 Unknown 桶得 ENOSYS 应答，而双方 canned 测试各用各的常量恒绿；②`sysctl_via` 只把负 m_type 判失败，而 C MIB 服务端应答协议是正 errno（main.c:483 `m_out.m_type = r`）——服务端 ENOMEM/ENOSYS 一律被客户端当成功。修法：常量改权威派生（C-41/NK8 判例）+ 判定改 `0=Ok / 非 0=Err(errno)`（`__sysctl.c` 透传 r 的 C 形）。触碰前核验：在制 NK1（kernel）/NS12（commands）/NL5-CI（xtask/CI）与本单文件无 hunk 交集 | ✅ 宿主半已随 E5(g) 合入（评审方 2026-09-23 销账；真机半挂 T2 通电） |
| C-58 | NL6-A 的跨界触碰：minix-sys sys_whoami 客户端 + tty 出生自证消费（照规则 1 登记；用户指派 zcode_glm_3 认领 new_edge2 NL6 携带子项） | new_edge2 面 `os/libs/minix-sys/src/syscall.rs`（E9 GETINFO 家族新增 `WhoAmI`+`sys_whoami`，C sys_getinfo.c:29-55 对位 + 2 canned 回放测）+ `os/drivers/tty/tty/src/main.rs`（出生位 `Endpoint::NONE` 桩改 sys_whoami 真值，C sef.c:76-87 对位） | 三半对账：kernel `getinfo_whoami`（misc.rs:645）与 overlay（message.rs:1824）已真，缺客户端半+消费位；顺带纠正旧注"endpoint 由 RS 分配"失实（内核引导表 BOOT_MODULE_PROC_NRS 授予，引导期不经 RS，原"RS 端点分配接线"半随之销）。触碰前核验：`tools/claim.sh list` 在制 NK1/NS12/E5G 与本单两文件无交集；memory driver 与 input server 同缝未顺手修（登记随波次） | **✅ 2026-09-21 销账（zcode_glm_3，代码 d585fa7f2 / merge 55bb73931）**：sys 315（313+2）/tty 52 全绿，clippy 38=38 零新增、rustfmt 新增区清；全账见 FIXLOG edge2 NL6-A 节 |
| C-60 | NL6-B（同缝批）的跨界触碰：memory/pckbd 出生桩 whoami 化（照规则 1 登记；NL6-A 收口时登记的同缝余量，用户指派 zcode_glm_3 续清；初登跳号——C-59 已被 E5(g) 让位顺延占用，按先落主线判例取 60） | new_edge2 面 `os/drivers/storage/memory/src/main.rs` + `os/drivers/hid/pckbd/src/main.rs`（出生位 `Endpoint::NONE` 桩改 sys_whoami 真值 + 旧注失实纠正；input server `os/servers/input/src/main.rs` 为 edge3 本线面免登记，同批交付） | 全仓扫描确认出生桩类仅此三处余量（其余 `Endpoint::NONE` 命中均为合法哨兵用途非出生桩）。修法/tty NL6-A 完全同款（C sef.c:76-87 对位）。触碰前核验：`tools/claim.sh list` 在制 NK1/NS12 与本单三文件无交集 | **✅ 2026-09-21 销账（zcode_glm_3，代码 afe7c2a7a / merge baafaa80d）**：memory 34/pckbd 33/input 98 全绿，clippy 三包 EXIT 0 触碰文件零命中，rustfmt 三文件清；全账见 FIXLOG edge2 NL6-B 节 |
| C-60 | OQ-N5 落地的共享文件触碰：边界守卫豁免 #[cfg(test)] 模块（照规则 3 登记） | new_edge4（编排；裁决落地执行人 zcode_glm_1） | `tools/check-command-boundary.sh`（新增 `strip_test_modules` 预处理：两条 grep 前剥除 `#[cfg(test)]` 门控模块，输出保持 file:行号:内容 形，行号指原文件；OQ-N5 裁决 2026-09-21） | ✅ 2026-09-21（实测：4 处 sbin/init 生产违规照报不误；测试代码结构性豁免——NL7 后 commands 树内已无测试态直用，豁免为守卫语义补齐而非放水） |
| C-61 | 历史日志模式挖掘 → 回归检查（照规则 3 登记：tools/ 新增脚本） | `tools/pattern-gate.sh`（新增）+ `tools/pattern-gate-baseline.txt`（新增）+ `rewrite-notes/coordination/PATTERN-SCAN-REPORT-20260923.md`（新增）；**不修改任何既有脚本/`.rs`/os 构建面**（登记人/执行人：zcode-glm，claim/C-61-zcode-glm @ acac0ffb5；C-61 编号取自 §2 尾部 grep 实测空号） | 扫描 `.review/zcode/edge1-4` FIXLOG（含 archive/raw 底档）+ NK4A/NK4B WORKLOG + NK4A-REVIEW-REPORT/NK4-REGRESSION-REVIEW + new_edge1-4/edge_todo 账本教训，落 13 项机械检查（P1-P13：FIXLOG 完整性/防回归测试存在性清单/权威常量派生/os-tests [[test]] 对账/冒烟 marker↔emit 对账/CI 跨架构 build 门存在性/退出码吞没/CRLF/会话产物防线等；`--diff` 增量门 + baseline 存量豁免 + `--self-test` 判别矩阵）。本批不接 CI、不改 review-patterns 规则集（用户裁决）；run_all.sh 三处已知 `|| echo` 吞退出码进 baseline 待 P6 清理，不让新门出生即红。触碰前核验：`tools/claim.sh list` 无活跃 claim；三文件全为新增，与他线零 hunk 交集 | **✅ 2026-09-23 销账（zcode-glm，代码 3643c9331 / merge 1cfc59c4e）**：交付三件全为新增文件（pattern-gate.sh / baseline 21 keys / PATTERN-SCAN-REPORT 六族 34 模式目录）；验证=worktree 主树双态全绿（full PASS=10 FAIL=0、--diff PASS=12、selftest 18/18、真树变异 6 组注入全现形后复绿、doc-style-lint --diff 0 error、锚点逐条复位）；主树合并后终验 P1 首次真跑 4 份 FIXLOG 全健康。后续余量登记于 PATTERN-SCAN-REPORT §6（CI 接线、review-patterns 登记均用户裁决暂不做） |
| C-62 | 历史日志模式挖掘**增量轮**（2026-09-23→09-27）：NK4-C 274 笔 + edge1 FIXLOG 增量（照规则 3 登记：tools 既有脚本扩展） | `tools/pattern-gate.sh`（扩展：P12 启发式修正、P14 RTS 裸操作对账、P15 误提交防线、P2_TESTS +7）+ `tools/pattern-gate-baseline.txt`（重生成）+ `PATTERN-SCAN-REPORT-20260923.md`（追加 §7 增量节）+ `.gitignore` 与 `git rm -r --cached os/.dockercargo`（执行 R3 报告 §7.3 待办 1：d6f176451 误提交 158 文件 vendored 依赖源码，纯 git 卫生零代码改动）（登记人/执行人：zcode-glm，claim/C-62-zcode-glm @ a1b8b1663；C-62 取自 §2 尾部 grep 实测空号） | 增量语料=NK4C-WORKLOG 5237 行+R3 评审 214 笔+edge1 FIXLOG 迭代22-25+firmware-heap-corruption.md+RESUME-PROMPT 陷阱清单；新提炼模式归并既有六族（消息号=枚举序数第三发 §1.105、幽灵 CPU §1.118、`.is_ok()` 丢载荷 §1.117、stat 回复不 copy_out §1.106、取证通道假设 §1.118、探针 DoS 掩盖 §1.100、跨页表 static 失效 landing pad 等），可机械化者落 P14/P15/P2_TESTS；P12 六处 FAIL 经人工复核=2 处启发式误报（doc 注释、门超窗）修正启发式，其余 committed 取证基础设施进基线；handoff.rs CRLF 进基线（NK4-C 活跃文件不触碰，报告建议转 LF）。触碰前核验：`tools/claim.sh list` 无活跃 claim；.dockercargo 修复=R3 明列待办且修法已给 | **✅ 2026-09-27 销账（zcode-glm，代码 5aa2d74f0+378a52370 / merge 0859580eb）**：P14/P15 新检查 + P12 启发式修正 + P2_TESTS 27/27 + .dockercargo 解除跟踪（ls-tree HEAD 实证 0）；主树终验 PASS=12 FAIL=0（P1 真跑 4 FIXLOG）。过程事故两条如实记：①commit --only 的 pathspec 工作树语义丢弃 staged 删除，378a52370 裸提交补笔（index 预核对）；②shell 持久 cwd 停在 worktree 致一批诊断命令误跑在 worktree、一度误判「主树被切分支」——跨 worktree 操作必须显式 cd（教训入 PATTERN-SCAN-REPORT §7.4） |
| C-63 | 历史日志模式挖掘**补扫轮**（用户点名缺口：.zcode/plans + .review/{claude,codex,trae} 1689 md + NK4C 评审 R1/R2 + 各 OPENING-PROMPT/HANDOFF + NK4C-WORKLOG 正文深抽）（照规则 3 登记：tools 既有脚本小扩展） | `tools/pattern-gate.sh`（P2_TESTS +1：classification_matches_the_expected_dfsc_table）+ `tools/pattern-gate-baseline.txt`（不变，无需重生成）+ `PATTERN-SCAN-REPORT-20260923.md`（追加 §8 补扫节）（登记人/执行人：zcode-glm，claim/C-63-zcode-glm） | 补扫三路并行 agent：.review 三工具 52 条 + .zcode/plans 35 文件 60+ 条 + NK4C-WORKLOG 正文 53 条；四小文件补读（NK4C R1/R2 评审、NK4C-OPENING-PROMPT、NK4A-QWEN-OPENING-PROMPT、NK4A-HANDOFF-STATUS）。新模式归并既有族（mock 门不一致/修复动作制造漂移/多唤醒互覆单槽/异步槽栈局部/多页虚拟传输首帧线性写/容量修复布局副作用/观测扰动翻转/panic=0 假阴性/信号位偏移分裂/C 怪癖防好心修错等）；机械化判定=P2_TESTS +1、无新 P 检查（本轮素材语义级为主，判据见 §8.3）；~/.zcode/cli（2.1G 宿主全局 CLI 数据）划界不扫，理由 §8.1。触碰前核验：claim.sh list 无活跃 claim | **✅ 2026-09-27 销账（zcode-glm，代码 162b1cd07 / FF 入 rewrite）**：P2_TESTS 28/28 + 报告 §8 补扫节（三路 160+ 条提取 + ~/.zcode/cli 划界）；主树终验 PASS=12 FAIL=0；文风门 --diff 0 error |
| C-64 | 历史日志模式挖掘**增量轮二**（2026-09-27→09-30）：NK4-C 续-60~77c 107 笔 + NK4C-WORKLOG delta +3225 行 + riscv-reviewlog.md 1579 行**首次全扫**（C-61/62/63 语料清单均未含，用户原话「REVIEWLOG」即此文件）+ 09-27 后新文档九件（照规则 3 登记：tools 既有脚本小扩展） | `tools/pattern-gate.sh`（P2_TESTS 追加视增量实测）+ `tools/pattern-gate-baseline.txt`（如增量出现新存量则重生成）+ `PATTERN-SCAN-REPORT-20260923.md`（追加 §9 增量节）（登记人/执行人：zcode-glm，claim/C-64-zcode-glm） | 增量语料实测：git log a1b8b1663..HEAD=117 笔（续-60~77c）；NK4C-WORKLOG delta +3225/-3（§1.119~§1.120）；riscv-reviewlog.md 全文 1579 行（新增语料）；新文档=NK4C-REVIEW-REPORT-20260927-R3（C-63 只扫 R1/R2）、NK4C-R3-P1-AUDIT-TABLE、NK4C-REVIEWER-SESSION-RECORD-20260930、NK4C-MIGRATION-20260930、NK4C-BUG-AARCH64-VEC-CAP{,-GLM}、NK4C-AARCH64-EXEC-REBIND-LIVELOCK、trap-boundary-message-materialization、NK4C-RESUME-PROMPT、NK4C-GLM53-PROMPT、MISC-CONCEPTS-SESSION-HANDOFF-20260927、misc_concepts delta +659。FIXLOG 四份（mtime≤09-23）/​.zcode/plans（最新 09-27 17:04 早于 C-63 交付）/​.review STATE（≤09-18）实测零增量。五路并行深读→人工复核锚点→对照六族 34 模式判新→机械化判定（强形态进 gate/P2_TESTS，语义级进 §9）。触碰前核验：claim.sh list 仅本单 | **✅ 2026-09-30 销账（zcode-glm，代码 396f1f5db / rebase 后 FF 入 rewrite @ 5655cbd5c）**：P2_TESTS 43/43（+15：续-75×4/续-18/续-51×3/续-73/续-76b/§1.119续-3/§1.119续-7/§1.120续-5×3，grep 在位+出处提交钉准）+ P16 新检查（trap 腿消息物化防线，selftest 正反例 3 例）+ 报告 §9 增量节（45 条新形态归并既有族+吞错家族债⑫专节+riscv 结构债分类学；FIXLOG/plans/STATE 零增量实测）；主树终验 PASS=13 FAIL=0 SKIP=0；selftest 27 例全绿；真树变异 P16/P2 注入现形还原复绿；doc-style-lint --diff 0 error；增量门 triage 零新增存量（P7/P8/P12/P14 全在基线、代码面 P13 符号 20 个零虚构）。扫描截断面 a1b8b1663→c9e6ada3d（续-78a/b 在截断面后入账，留下一轮增量） |
| C-65 | 历史日志模式挖掘**增量轮三**（C-64 截断面 c9e6ada3d→现行 HEAD）：NK4-C 续-80~283 共 297 笔 + NK4C-WORKLOG delta +3624 行 + misc_concepts +672（114→148 条）+ R3.1 评审群（报告/对账表）+ TRANSIENT-PTE 档案 795 行 + riscv 取证方法论 + RETRO-AUDIT/STRUCTURAL-DEBT-REGISTER（untracked 首扫）+ 3 份接续 PROMPT + plans +2 + AI-chats delta（照规则 3 登记：tools 既有脚本小扩展） | `tools/pattern-gate.sh`（P2_TESTS 追加视增量实测）+ `tools/pattern-gate-baseline.txt`（如增量出现新存量则重生成）+ `PATTERN-SCAN-REPORT-20260923.md`（追加 §10 增量节）（登记人/执行人：zcode-glm，claim/C-65-zcode-glm） | FIXLOG 四份 mtime 实测仍停 09-20~09-23（用户提示「变动大」实为 WORKLOG 提交流）；.review STATE 零变动；方法=六路并行 agent 深读（WORKLOG 对半两路/misc_concepts/新文档群/prompt+plans 群/提交消息去重路）→锚点逐字抽验→对照六族 34+45 模式判新→机械化判定。增量门走有界 triage（C-64 教训：--diff 大区间挂起禁跑）。触碰前核验：claim.sh list 仅本单 | **✅ 2026-10-04 销账（zcode-glm，代码 7eca9cf4e，与登记同分支交付）**：P2_TESTS 64/64（+21：brk/split 族 11+PL011×4+stat 布局+懒 FPU+FPU 谓词+ATF×3）+ P12 第三发修正（函数名架构前缀=隐式门，主树实跑误报驱动，selftest+真树变异验证）+ 报告 §10 增量节（49 条新形态：取证仪器治理/翻案传播三层脱钩/门禁盲区三路独立发现/内核语义/契约测试折叠）；主树终验见 claim list 前最后一轮 full；selftest 28 例全绿；doc-style-lint --diff 0 error；增量 triage：P10=0/P13 21 符号零虚构/存量全在基线。扫描截断面 c9e6ada3d→484b5a12b（截断面后的续-284+ 留下一轮） |
| C-66 | 历史日志模式挖掘**补扫+增量合一轮**：①闭合上轮盘点 8 处历史缺口（new_laptop_migrate 5 件 42KB 首扫、.review 三工具 1689 md 扩面 60-80 文件密度优先、NK4B-WORKLOG 后半 M3.3 起、HANDOFF-NK4A×2、NK4-REGRESSION-REVIEW×2、NK4A/B-TODO 非定向正文、AI-chats daily.todo 中段、new_edge1/2/3 他线 09-23 后增行）；②追平 C-65 截断面 484b5a12b 之后新增量 86 笔（续-284~341 + T13 地址常量清扫）+ WORKLOG delta +732 + misc_concepts 148→160 + R32 评审群 + QEMU-ENVIRONMENTS + ADDRESS-CONSTANT-AUDIT/tools/address-constant-scan.py 新工具 + 三件档案转 tracked（照规则 3 登记：tools 既有脚本小扩展视增量实测） | `tools/pattern-gate.sh`（P2_TESTS 追加视增量实测；本轮增量实测零新增测试函数）+ `PATTERN-SCAN-REPORT-20260923.md`（追加 §11 补扫+增量节）（登记人/执行人：zcode-glm，claim/C-66-zcode-glm） | FIXLOG 四份/​.review STATE 实测仍零增量（mtime 09-20~23）；深读四路：增量路（WORKLOG delta/提交消息/misc delta/新文档群含新扫描工具审查）+ 补扫三路（new_laptop_migrate+AI-chats 中段 / .review 扩面 / 小件打包）；锚点抽验（内容 grep+行号重钉双步，C-65 教训）；对照 §2/§7-§10 目录判新。触碰前核验：claim.sh list 仅本单 | **✅ 2026-10-05 销账（zcode-glm，代码 ecd1778bf+基线笔）**：P17 新检查（report 式，145 工件计数上线）+ 报告 §11 补扫+增量节（45 条新形态；历史缺口 8 处闭合或定界；.review 扩面密度结论=六族 80-120 文件后续消化、中间产物 300 永久跳过）+ P12 基线收编（lib.rs:3842 sfence.vma 调用图可达性门=第四发误报形态；P8 随 NK4-C 转 LF 同步 6→5）；P2_TESTS +0（增量 86 笔零新增 #[test]，T13 审计交付零钉扎立为形态）；selftest 30 例全绿；文风门 0 error；锚点抽验 8 组（6 吻合 2 偏 1 重钉）。截断面后增量留下一轮 |
| C-67 | 历史日志模式挖掘**增量轮四**（C-66 截断面 5cf88a552→现行 HEAD）：NK4-C 续-342~384 共 53 笔（**目标③ riscv 36/36 正式达成 + 三终收尾结案横幅**）+ WORKLOG delta +711 + misc_concepts 160→169 + MEMORY-CORRUPTION 档案 542 行首扫 + R33 评审群 + T13 A2 修复落地（mmap 越界拒绝带测试）+ .review 高密度族欠账首批消化（照规则 3 登记：tools 既有脚本小扩展视增量实测） | `tools/pattern-gate.sh`（P2_TESTS 追加视增量实测）+ `PATTERN-SCAN-REPORT-20260923.md`（追加 §12）（登记人/执行人：zcode-glm，claim/C-67-zcode-glm） | FIXLOG 四份/​.review STATE 实测零增量（mtime 仍 09-20~23）；深读三路：增量路（WORKLOG delta/新档案/R33/接续PROMPT/misc delta）+ .review 高密度族路（STATE/fix-status/VERIFICATION/fix-log/test-audit 命名族首批 30-40 文件）+ 提交消息去重路；锚点抽验（grep+重钉双步）；release 前 pwd 必检主树（C-66 教训固化）。触碰前核验：claim.sh list 仅本单 | **✅ 2026-10-06 销账（zcode-glm，代码 c5e3e27cd）**：P2_TESTS 76/76（+12：T13 A1/A2×3+DumpCore×1+walker 判别族×8）+ P12 键去行号（教训⑦实现归位，3842→3848 行漂移活体判别）+ 报告 §12（收官四重折扣/绿证据失真簇/.review 欠账清零——非中间产物全集 39 个修正估计）；selftest 30 例全绿；P2 变异现形；文风门 0 error；基线三换三同 cksum。截断面后增量（NK4-C 滚除+档案终态入库收尾批次）留下一轮 |
| C-68 | 历史日志模式挖掘**增量轮五**（C-67 截断面 ad14ced56→现行 HEAD）：NK4-C 续-385~427 共 61 笔（三架构 SMP 对等战役启动 + 信号/notify wire 工作 + 续-413 评审回执轮九件事含 unsafe-audit.sh 假红修复）+ WORKLOG delta +579 + misc_concepts 169→179 + TODO-3ARCH-PARITY 新档 429 行 + R34 评审群 + 收尾批次对账（145 工件仍 tracked=移交项销账形态持续；档案终态在途）（照规则 3 登记：tools 既有脚本小扩展视增量实测） | `tools/pattern-gate.sh`（P2_TESTS 追加视增量实测）+ `PATTERN-SCAN-REPORT-20260923.md`（追加 §13）（登记人/执行人：zcode-glm，claim/C-68-zcode-glm） | FIXLOG/.review STATE 实测零增量；深读两路：增量路（WORKLOG delta/新档/R34/TRANSIENT-PTE delta/misc delta）+ 提交消息路（61 笔含续-413 九件事专挖+unsafe-audit.sh 改动审查）；锚点抽验双步；release 前 pwd 必检。触碰前核验：claim.sh list 仅本单 | **✅ 2026-10-07 销账（zcode-glm，代码 8b7aacffe）**：P2_TESTS 87/87（+11 minix-sef 信号臂家族）+ 基线三向收编消解（boot-shim 端口勘误新键/sfence 探针滚除消解/P7 存量清零）+ 报告 §13（25 条新形态：门工具残余缺陷实锤移交 NK4-C 含根治建议/总量 pin 免疫/挂名未注册残留/能力标记缺失 36-36 实为单核等）；selftest 全绿；P2 变异现形；文风门 0 error。SMP 战役推进与收尾批次落定留下一轮 |
| C-69 | 历史日志模式挖掘**增量轮六（迁移后首轮）**（C-68 销账 858a41085→现行 HEAD）：共 37 笔，其中 19 笔为 notes 三树迁移+工作流重构（e90e4173d 结构迁移/c399d33ae Phase 4 工具链适配/d29b0f191 Phase 0 全貌/7b156b407 review-scan 收编规范源/f41e7d12f L13 门/434d70359 settings 清理）+ NK4-C 线增量（SMP 战役续推进）+ misc_concepts 增量八 179→180；**迁移后路径全变**：报告/账本/交接/WORKLOG 迁至 rewrite-notes/coordination/、evidence 迁至 tmp/evidence/、历史重写（旧哈希失效） | `tools/pattern-gate.sh`（**迁移遗留缺陷修复：diff_added_lines pathspec 仍含死路径 notes/ 且缺 rewrite-notes/——--diff 增量门对三棵笔记树静默失明，与 doc-style-lint 修过的同形态**；P2_TESTS 追加视增量实测）+ `rewrite-notes/coordination/PATTERN-SCAN-REPORT-20260923.md`（追加 §14；**新路径**）（登记人/执行人：zcode-glm，claim/C-69-zcode-glm） | 深读两路：NK4-C 增量路 + 迁移/工作流重构路（含迁移自身的方法教训）；锚点抽验双步；release 前 pwd 必检。触碰前核验：claim.sh list 仅本单 | **✅ 2026-10-08 销账（zcode-glm，代码 7abc3cc43）**：轮六·迁移后首轮——本线工具三修（pathspec 三树失明修复：硬数字 4,839→536,131 新增行入 scope；P9 换代四条规则；P17 转强制=145 工件清零触发条件达成+变异实证）+ P2_TESTS 97/97（+10：逐成员 56B 断言+draining 门×5+process_init×4）+ 报告 §14（42 条：迁移自产教训+NK4-C 增量）；selftest 全绿；文风门 0 error；evidence 按 tmp 整域裁决留本地不入库。移交 NK4-C：unsafe-audit CTX 根治/test_nreqs 缺 #[test] 持续跟踪；移交 review-rules 线：SKILL.md 旧模板+$STAGE 未定义 |
| C-70 | 增量轮六移交项销账（上轮 §13/§14 登记的两项修复）（照规则 1/3 登记：跨界触碰 NK4-C 线文件 + 共享 tools/） | ① `tools/unsafe-audit.sh`（--diff CTX 快照来源修正：双参 RANGE 读右端修订/--cached 读索引/不可得打 WARN——旧实现一律读工作树，同 RANGE 两日读数矛盾在案，C-68 §13.2-1 登记项）；② `os/libs/minix-types/src/ipc/fs_driver.rs`（test_nreqs_and_is_fs_rq_gate 补 #[test]——「挂名未注册测试」形态，C-68 §13.2-3 登记项，NK4-C 线 b2d2c68b4 已自登同族残留）（登记人/执行人：zcode-glm，claim/C-70-zcode-glm） | 判别实验：①scratch 历史 RANGE 双跑同判 + WARN 路径实证；②docker minix-ci:1.94 跑该测试看首次真实执行结果（可能暴露断言过期=收获）。③ SKILL.md 旧模板项按上轮裁决缓行（评审线在飞） | **✅ 2026-10-08 销账（zcode-glm，代码 0f1588446）**：① unsafe-audit CTX 快照读取修正（判别实验双向实证：扰动后新脚本逐字节一致/旧脚本翻 13→1；3 处 bare 人工核快照为真；WARN 不静默降级）；② test_nreqs 补 #[test]（docker 首次执行 1 passed——断言仍有效纯执行位缺失）+ 同步入 P2_TESTS（98/98）。判别实验第一版 /tmp 副本 PROJECT_ROOT 伪影踩坑已记录修正。③ SKILL.md 项按裁决缓行待评审线窗口 |
| C-71 | PD-30 执行：持续集成加 kernel-tests 任务双腿（可信度地基组硬序 §九.4 的②；前置① SD-28 吞码已修——run_all.sh:25-30 现状核实在案；后接③ PD-29 摘 mock 默认）。描述权威 `TODO-LEDGER-OPEN.md` §7「跨架构常驻编译检查门未落地」，裁决权威 `PENDING-DECISIONS-3ARCH-PARITY.md` §七.4 采甲（照 vm-tests 先例：宿主默认形态测试 + 生产形态交叉编译） | 编排面（CI 基建，线外单，coordination 台账维护批） | 新增 `.github/workflows/kernel-tests.yml`（腿①宿主默认形态测试 `cargo test -p minix-kernel`＝库 829+集成 6；腿②x86_64 生产形态 `cargo build -p minix-kernel --target x86_64-unknown-none --no-default-features`——补齐 qemu-tests.yml cross-arch-kernel-build 门的三架构缺一）+ 台账 `TODO-LEDGER-OPEN.md` §7 翻状态、`TODO-LEDGER-DONE.md` §2 落执行行（登记人/执行人：glm，claim/PD-30-glm @ 1bad22152） | 🔄→**✅ 2026-10-08 销账（glm，代码 5511e7c3b）**：kernel-tests.yml 双腿交付——腿①宿主默认形态测试（docker minix-ci:1.94 `-m 2g -j 1` 实跑：库 829 通过/0 失败/8 忽略＋集成 3 通过/3 忽略）＋腿②x86_64 生产形态（原样命令实跑 exit 0）；YAML 解析 2 jobs；收尾五道校验全绿（reconcile 基线全 OK/受影响三文件零断链/staleness PASS/文风门 --diff 0 error/inflight 干净）；台账 OPEN §7 翻 closed、DONE §2 落 PD-30 执行行；硬序③ PD-29 解锁。触碰前核验：claim.sh list 仅本单 ✅ |
| C-72 | 回复通道 errno 符号族（F10 同族）的跨界触碰：折号单源上收 minix-types（照规则 1 登记） | new_edge2 面 `os/libs/minix-types/src/types/errno.rs`（新增 `reply_status` 单源 + 一枚单测）+ `os/libs/minix-types/src/types/mod.rs`（既有 `pub use errno::*` 承载，无改）；new_edge3 面 `os/servers/sched/src/server.rs`（settle 回复边界折号 + 13 处线序断言翻转）、`os/servers/is/src/lib.rs`（同）、`os/servers/ds/src/{dispatch,server}.rs`（DS 本地实现改调单源）、`os/servers/pm/src/sched.rs`（ELOCKED 重试判据取线序负值）、`os/tests/pm_sched.rs`（两处断言）（登记人/执行人：qoder，claim/F10-REPLYSIGN-qoder，台账 §4 传输族行的「逐台核」判据执行） | 判据来自 C 侧 `_SYSTEM` 定义分布（`grep -rn "_SYSTEM" minix3/minix/servers/*/*.h`：ds/ipc/is/rs/pm/sched/devman/vfs/vm 有、MIB 无）——有 `_SYSTEM` 的服务其 errno 常量本身带负号，`m_type = result` 写回的是负错码，用户侧 `_taskcall` 按 `m_type < 0` 判错；本树 errno 是正号常量，故每台 `_SYSTEM` 服务都要在回复边界折一次号（VFS 的 `queue_reply`、DS 的 `dispatch::reply_status` 已各自立过，本单把它们并成单源并补 SCHED/IS）。PM 侧 `ELOCKED` 原按正数 208 比对，是同一缺陷的镜像面（客户端跟随服务端的错误线序）。MIB 明确不套本函数（C-59 判例：其应答是正 errno）。触碰前核验：`tools/claim.sh list` 在制仅 DOCGEN2（文档面，os/ 零交集） | 🔄 本族累计：DS/SCHED/IS 三台已核已修（claim/F10-REPLYSIGN-qoder，merge e8c9187fb）；RS 一台同族续修（claim/F10-REPLYSIGN2-qoder，触碰面加 `os/servers/rs/src/lib.rs` 一处收口 + 形状钉测试），真机 `bash os/qemu-tests/test-e5-ds.sh` 全系统启动绿；余 devman/input/ipc-server/vm 四台待逐台核（台账 §4 同族行已登记判据、命令与「为何不能盲改」的配对缺口） |
| C-73 | 命令面分层边界的生产违例十四处（`PD-32` 执行半，`NL7`）：跨界触碰二线库面与三线命令面（照规则 1 登记；**本单为补登记——先动了手才登记，见下方教训**） | 二线面 `os/libs/minix-sys/src/lib.rs`（新增 `errno`/`MinixKerninfo`/`KuserInfo` 再导出 + `user_stack_top`/`gettimeofday`/`sigaction`/`setitimer` 四个顶层封装 + 一枚 `user_stack_top` 宿主回退钉）；三线面 `os/commands/bin/shell/src/bin/sh.rs`、`os/commands/sbin/init/src/{execve,host}.rs`（`minix_types::types::errno`/`DirectTrapTransport`/`IpcTransport` 全部改走 `minix-sys` 顶层，命令不再命名 ipc 传输层） | 判据：`bash tools/check-command-boundary.sh` 14→0。`minix_get_user_sp`（`kernel_utils.c:40-49`）此前在 `sh.rs`/`execve.rs` 各抄一份，收敛为单源；`host.rs` 的 `send_sigaction` 原带 `&impl IpcTransport` 测试替身缝，已无测试在用（C-7 判例：命令层不自建 transport 替身），去掉参数直接走顶层 `sigaction`。触碰前核验：`tools/check-staged-inflight.sh --list` 仅 `TODO-3ARCH-PARITY-20261006.md`（本单不碰） | 🔄→**✅ `2026-10-09` 销账（qoder，claim/CMD-BOUNDARY-qoder）**：三 crate 测试全绿（minix-init 149 / minix-shell 40 / minix-sys 316+1 新增钉）、`cargo build --workspace` 零错、build 告警 34=34、clippy 40=40、`cargo test --no-run` 告警 48=48 均无新增；台账 OPEN §5 翻 `closed`、DONE §1 落执行行。教训如实记：**登记顺序错了**——应先写本行再动 `os/libs/`+`os/commands/`，本次是先双查了避让（确认与他线零交集）就直接开工，收尾才补登记；避让双查不等于跨界登记，二者是两道独立的门 |
| C-74 | `SD-27` 锚点漂移批量纠错（一线/二线/三线全跨，本次先登记后动手，不再重踩 C-73 的教训） | 一线面：`os/kernel/src/misc.rs`（`kernel.h:528`→`profile.h:13`）、`os/kernel/src/syscall_copy.rs`（`safecopy.h:55-71`→`minix/safecopies.h:41-49`，两处）、`os/kernel/src/smp.rs`（`boot.c:731`——已核为 QEMU 侧引用，非 minix3 漂移，不改动只标注）。二线面：`os/libs/minix-types/src/ipc/input.rs`（`kbdio.h:23/24`→`19/20`）、`os/libs/minix-sys/src/syscall.rs`（`sys_getksig.c`/`sys_endksig.c`→实际文件名 `sys_getsig.c`/`sys_endsig.c`）。三线面：`os/fs/mfs/src/mkfs.rs`（`mfs.h:9`→`mkfs.mfs/v3/const.h:9`）、`os/commands/bin/termctl/src/stty.rs`（`stty.c:208`→`modes.c:208`）、`os/commands/bin/shell/src/exec_frame.rs` 与 `os/commands/sbin/init/src/execve.rs`（`stack_utils.c:174/176`→`168/170`）、`os/commands/usr-bin/textfilter/src/cmp.rs`（`regular.c:119`→`111`，三处）、`os/net/lwip/...`无关、`os/servers/vfs/src/syscalls.rs`（`ipc.c:2060`→`ipc.h:2261-2269`）、`os/servers/pm/src/{exit.rs,ipc/dispatcher.rs}`（`sys_getksig.c`/`sys_endksig.c`同二线面；`vm_willexit.c:11-21`→`vm_exit.c:11-21`，两处）、`os/drivers/tty/tty/src/char_face.rs`（`do_write.c:553-556`→`tty.c:553-556`） | 判据：本轮现场扫描 `os/**/*.rs` 注释里的 `file.c:NNN` 形态引用（复用 `tools/pattern-gate.sh` P11 的 basename-多候选消歧算法，非简单单命中），现算 17970 条中真正越界或找不到文件的共 23 处（非此前 naive 单候选误报的 4551）；逐条已与 `minix3/` 现状核对。触碰前核验：`tools/check-staged-inflight.sh --list` 仅 `TODO-3ARCH-PARITY-20261006.md`（本单不碰）、`tools/claim.sh list` 无并发在制 | ✅ 销账：批量纠正落在 claim/SD27-ANCHOR-BATCHFIX-qoder（修 21、豁免标注 2），工具化守卫（pattern-gate P18）落在其上的 claim/SD27-RS-MODE-qoder，并把前轮人工审计漏校的一类"区间 END 越界"再扫出 17 处一并修净（同单续修），P18 现零基线全绿；两分支待主树并发态落定后按序合回（`2026-10-09`） |
| C-75 | `SD-19` 旗舰 crate 收编：静默吞错机械化门的 maintain+comment 档落地（先登记后动手；与 C-73 同域：二线库面 os/libs/minix-sys + coordination 台账） | 二线面 `os/libs/minix-sys/src/` 九文件（ds 7/getcwd 5/lib 4/usb_model 5/vm 2/pm/syscall/vfs/devman_client 各 1）：27 处首轮警告逐站加 `//[SD19-gate] <理由>` + 语句级 `#[allow(clippy::let_underscore_must_use)]`，`lib.rs` 门注同步；零行为改动，纯注释+属性 | 判据（容器 minix-ci:1.94，touch 强制重编）：`cargo clippy -j 1 -p minix-sys --tests` → non-binding 27→0、总告警回基线 40 不多不少；`cargo test -p minix-sys` → 317/0 不变。理由全数对 C 锚点/不返回语义/测试夹具三类，逐条可在位读。触碰前核验：`check-staged-inflight.sh --list` 仅 TODO-3ARCH-PARITY-20261006.md（本单不碰），claim list 无并发在制 | ✅ 销账（`2026-10-09`，已合回 merge `ddff81824`） |
| C-76 | `SD-19` 处置表唯一的 signature rework 真修法：`fs-rt copy_out` 丢错从静默吞升级为传播（先登记后动手；跨二线库面 + 三线 FS 轨） | 二线面 `os/libs/minix-fs`（trait `FsTransport::copy_out` 签名改 `Result<(), Errno>` 对称 `copy_in`；dispatch 梯子 6 站捕获首个传输错并按 C 覆盖回复；测试夹具 ScriptedTransport + 新增失败注入测试）；三线面 `os/fs/fs-rt`（真传输 impl：invalid grant 维持 OK 与现行为字节等价，safecopyto 真失败按 copy_in 同构回 EIO）与 `os/tests/t_vfs_mfs_bridge.rs` 夹具 | 判据：C `libfsdriver/utility.c:52-53` fsdriver_copyout 直接 return sys_safecopyto 结果、`dentry.c:52-54` copyout 失败即 return r——旧 Rust 把 copy_to 错误丢弃、传输坏了仍回 OK。新回归测试：传输 copy_out 失败时 read 链回 EIO 非 status 0；`cargo test -p minix-fs`/`-p minix-fs-rt` 全绿。触碰前核验：check-staged-inflight 仅 TODO-3ARCH-PARITY-20261006.md，claim list 无并发在制 | ✅ 销账（`2026-10-09`，已合回 merge `7be334565`） |
| C-77 | `SD-19` 逐 crate 扩门的收尾批：workspace 扫出的最后 7 处真触发点接门（其中 6 处本单落地，1 处 textfilter cmp.rs 因该 crate 一处 base 即有 clippy 编译错暂搁，见右）（先登记后动手；跨二线库面 minix-rt/minix-sef + 三线 os/fs、os/commands） | 二线面 `os/libs/minix-rt/src/lib.rs:188`（补前轮漏：workspace 特性统一才暴露的 `sys_diagctl_write` OOM 报告臂，前轮 `-p minix-rt` 默认特性下该 cfg 未编译，per-crate 验证有盲区）、`os/libs/minix-sef/src/lib.rs`（落 warn 门 + `ipc.notify` pong 发后即忘 1 站）；三线面 `os/fs/fs-rt`（落 warn 门 + 4 站：birth 报告 send、错误回复 send、lookup 重写 copy_to 尽力而为、终回复 send）、`os/commands/usr-bin/textfilter`（原计划落 warn 门 + `write!` 入 String 1 站，**本单实测跳过**：该 crate 存在 base 即有的 `units.rs:120` clippy `unused_loop` deny 错，crate 编译在 clippy 阶段直接 abort，无法验证新加的 warn 门；这属另一条待修项，本单不顺手修，cmp.rs:76 待 units.rs 修好后并入下一次同程序扩门） | 判据：`cargo clippy -j 1 --workspace --tests -- -W clippy::let_underscore_must_use` 全 workspace non-binding 从 7→0（含 kernel；per-crate 有特性统一盲区，一律以 --workspace 为准）；逐站理由均为发后即忘 send/尽力而为 copy/仅分配可败 write!/最后报告通道，登记处置表 maintain+comment 档。触碰前核验：check-staged-inflight 仅 TODO-3ARCH-PARITY-20261006.md（本单不碰），claim list 无并发在制 | ✅ 销账（`2026-10-09`，已合回 merge `61e089b7a`） |
| C-78 | `SD-19` 第七家 crate textfilter 接门 + 前置修一处 `units.rs:120` `unused_loop` deny （先登记后动手；单域：os/commands/usr-bin/textfilter，同 crate 内不跨线） | 三线面 `os/commands/usr-bin/textfilter/`：`units.rs` 的 `loop { add_product(top); match bottom { Some => {add_product(bottom); break}, _=>break } }` 展开为顺序两步 `add_product(top, !flip)?; if let Some(b) = bottom { add_product(b, flip)?; }`（与 C `minix3/usr.bin/units/units.c:359-436` 的 do-while `while (doingtop >= 0)` 至多两轮语义等价——每轮尾固定 `doingtop--`、无斜杠额外一次 `--`，永远没有第三轮，`loop` 包装恒于首轮 break 就是 clippy::unused_loop 抓到的那个）；同轮 `lib.rs` 加 `#![warn(clippy::let_underscore_must_use)]` + `cmp.rs:76` `//[SD19-gate]` 理由 + 语句级 `#[allow]` | 判据（容器 minix-ci:1.94，`find . -name '*.rs' -exec touch {} +` 全域重扫）：`cargo clippy -j 1 --workspace --tests -- -W clippy::let_underscore_must_use` → textfilter/src/ 下 non-binding 命中 = 0；`cargo test -p minix-textfilter` → 229 passed / 0 failed。同轮如实报告本仓 SD-19 剩余真规模：420 处未接门 crate must_use 触发（分布 25+ crate），前几轮 `find libs fs commands kernel -name lib.rs | xargs touch` 漏了 servers/net/drivers 与其他 commands/* bin，其 clippy 缓存未刷新导致误判。触碰前核验：check-staged-inflight 仅 TODO-3ARCH-PARITY-20261006.md（本单不碰）、claim list 无并发在制 | ✅ 销账（`2026-10-09`，已合回 merge `69b613661`；本轮 `find os -type f -name '*.rs' -exec touch {} +` 全域重扫补记：textfilter `bin_support.rs:55` 前轮 `--workspace` 扫因 glob `bin/../bin_support.rs` 折回覆盖而漏，并入 C-79 补门） |
| C-79 | `SD-19` 第八家 crate `os/fs/ext2` 接门 + textfilter `bin_support.rs:55` 残骸补门（先登记后动手；跨三线 FS 轨 + 三线 commands 轨） | 三线面 `os/fs/ext2/src/`：`lib.rs` 落 `#![warn(clippy::let_underscore_must_use)]` + `server.rs` 逐站 8 处 `//[SD19-gate] 理由` + 语句级 `#[allow]`（`adjust_links` 5 处对应 C `minix3/minix/fs/ext2/link.c:92/255/434` 直接 `i_links_count++/--` 后 `put_inode` void 不回错，主操作 `add_entry/remove_entry` 已成功、链数尽力而为；`free_file_blocks` 2 处对应 C `link.c:260` `put_inode` 后不管错误，inode 归零后 block 释放是自动副作用路径；测试 3 处 `server.read` 紧邻 `assert_eq!` 让断言本身暴露错误，不 unwrap 是刻意形状）；同轮 `os/commands/usr-bin/textfilter/src/bin_support.rs:55` `let _ = std::io::stderr().write_all(bytes)` 上补 `//[SD19-gate]` + `#[allow]`（hosted warn 通道，`write_all` 可 EPIPE，与 C `fputs(msg, stderr)` 无检查同构；前轮 `--workspace` 全域扫里该文件由 glob `bin/../bin_support.rs` 折回覆盖而漏，本轮按文件直读补门） | 判据（容器 minix-ci:1.94，`find os -type f -name '*.rs' -exec touch {} +` 全域刷新）：`cargo clippy -j 1 --workspace --tests -- -W clippy::let_underscore_must_use` 现算 `fs/ext2/src/` non-binding 8→0、`textfilter/src/` non-binding 1→0；`cargo test -p minix-fs-ext2` 全绿、`-p minix-textfilter` 229/0 保持。触碰前核验：`check-staged-inflight.sh --list` 仅 `TODO-3ARCH-PARITY-20261006.md`（本单不碰）、`claim list` 无并发在制 | ✅ 销账（`2026-10-09`，已合回 merge `5329a06dd`） |
| C-80 | `SD-19` 第九家 crate `os/servers/vm` 接门（先登记后动手；单域：三线 servers 轨 VM server，不跨线） | 三线面 `os/servers/vm/src/`：`lib.rs` 落 `#![warn(clippy::let_underscore_must_use)]` + 逐站 8 处 `//[SD19-gate] 理由` + 语句级 `#[allow]`（vmproc_handle.rs:711 `unmap_range` 在 drop 路径，页面可能本就没映射、与 C `pt_writemap` MAP_NONE 同构，行内已自陈忽略理由；audit.rs:93 `diag_write` 头部契约“audit 丢失从不致命”，panic 会直接拿掉 VM 事件环；region/mod.rs:38 `pt.unmap(vaddr)` 区域撕页循环中同 711；ipc/dispatcher.rs:1239 `increment_region_remaps` remap 计数 bookkeeping，主操作已完成不回滚；mmap.rs:527 `vfs_queue.request` 入队失败后续重试路径会重发，区域已先建立；vm_server.rs:1895 deferred VFS callback 因 vfs_queue borrow 约朿拆开后才可回调，失败面同回调自身；memtype.rs:1920 `should_panic` 下回不到结果处属测试夹具；heap_arena.rs:274 测试 cleanup 中 unmap 失败可忽） | 判据（容器 minix-ci:1.94，`find os -type f -name '*.rs' -exec touch {} +` 全域刷新）：`cargo clippy -j 1 --workspace --tests -- -W clippy::let_underscore_must_use` 现算 `servers/vm/` non-binding 8→0；未接门 crate 真实余面从 462 降到 454。`cargo test -p minix-vm` 全绿。触碰前核验：`check-staged-inflight.sh --list` 仅 `TODO-3ARCH-PARITY-20261006.md`（本单不碰）、`claim list` 无并发在制 | 🔄 开工中（本分支，待合回） |
| C-81 | `SD-19` 第十家 crate `os/servers/ds` 接门（先登记后动手；单域：三线 servers 轨 DS server） | 三线面 `os/servers/ds/src/server.rs`：`lib.rs` 落 `#![warn(clippy::let_underscore_must_use)]` + 5 站 maintain+comment——birth sendrec(C ds.c:127)、普通 send(C 127-129)、3 处 notify(C ipc_notify 无回复/失败丢一次不持断循环)。判据（容器 minix-ci:1.94）：`cargo clippy -j 1 --workspace --tests` → servers/ds non-binding 5→0。`cargo test -p minix-ds` 全绿。触碰前核验：无并发在制 | 🔄 开工中（本分支，待合回） |
| C-82 | `test`/`[` 命令实现（先登记后动手；跨二线 minix-sys + 三线 commands/fileops） | 二线面 `os/libs/minix-sys/src/{pm.rs,lib.rs}`：新增 `getgid_via`/`getgid`/`getegid`（`PM_GETGID`，服务端 `GetResult::Gid{real,eff}`→`prefill(real,eff)` 同形，对位 `getuid_via`）+ 一枚 `getgid` 车道序回放测；三线面 `os/commands/bin/fileops/`：`testexpr.rs` 决定半扩到 `test.c` 全算子面（纯函数 `access_bits` 复刻 C `test_access`、`FileKind` 拆 `block`/`char`、`FileTester` 补 `-u/-g/-k/-O/-G/-N/-ef`）+ 新薄壳 `src/bin/test.rs`（真 `FileTester` 走 `stat`/`lstat`+`access_bits`，`[` 剥尾 `]`、退出码 0/1/2） | 判据：宿主 `cargo test -p minix-fileops`→78/0（+4）、`cargo test -p minix-sys --lib`→321/0（+1）；freestanding `cargo build -p minix-fileops --target x86_64-unknown-none --release`（docker `minix-ci:1.94`）绿；clippy 触及文件零告警。真机 per-command 断言与 imgrd 播种归 `C-57`/E5（命令面真机半）。触碰前核验：`check-staged-inflight.sh --list` 仅 `TODO-3ARCH-PARITY-20261006.md`（本单不碰）、`claim.sh list` 无并发在制 | ✅ 本分支落地（`claim/CMD-TEST-BIN-qoder`，一笔提交待合回 `rewrite`）|
| C-83 | `df` 命令实现（先登记后动手；跨二线 minix-sys/minix-types + 三线 commands/mountinfo） | 二线面 `os/libs/minix-sys/src/vfs.rs`+`lib.rs`：新增 `statvfs1_via`/`getvfsstat_via`（载荷 `mess_lc_vfs_statvfs1` fd/flags/len/name/buf、`mess_lc_vfs_getvfsstat` buf/len/flags，对位 C libc）+顶层 `statvfs`/`getvfsstat`+两枚 wire 回放测；`os/libs/minix-types/src/types/statvfs.rs` 补 `ST_WAIT`。三线面 `os/commands/sbin/mountinfo/`：新增 `df.rs` 决定半（`fsbtoblk`/`capacity_percent`/`format_p_row` 对 df.c:350-353,471-477，纯整数/字符串）+ `src/bin/df.rs`（getvfsstat 枚举/statvfs 逐路径、POSIX -P 视图） | 判据：`cargo test -p minix-mountinfo --lib df`→5/0、`-p minix-sys --lib`→323/0(+2)；freestanding `cargo build -p minix-mountinfo --bin df --target x86_64-unknown-none --release`（docker minix-ci:1.94）绿；clippy 触及文件零告警。真机 per-command 与 imgrd 播种归 C-57/E5。触碰前核验：`check-staged-inflight.sh --list` 仅 `TODO-3ARCH-PARITY-20261006.md`（本单不碰）、`claim.sh list` 无并发在制 | ✅ 本分支落地（`claim/CMD-DF-BIN-qoder`，一笔提交待合回 `rewrite`）|
| C-99 | 更正权威编译门的挂载点（先登记后动手；仓库共享件 `AGENTS.md` + coordination 两份配方文档 + 一处测试注释） | `AGENTS.md` Build & Test 段：把「docker `minix-ci:1.94` + `-m 2g -j 1`」补全为**在仓库根执行、`-v "$PWD:/work" -w /work/os`**，并写明以 `os/` 为根挂载会让读仓内文件的宿主测试假红；`rewrite-notes/coordination/NK4A-TODO.md` §5.1 与 §7 命令块（旧文正是「在 `os/` 目录下执行」，即错源）、`rewrite-notes/coordination/CONCURRENT-WORK-PROTOCOL.md` §6 内存铁律同口径；`os/libs/minix-types/src/types/stat.rs` 的 C 桥对账测注释写明「报 `NotFound` 是配方错不是布局漂移」 | 🔄 `2026-10-14`（qorder，`claim/GATE-MOUNT-RECIPE-qoder`）：同树同镜像 A/B 对照——旧配方 `minix-types` 330/1（`posix-stubs.c` NotFound）、新配方 331/0；批次二百三十三/二百三十七～二百三十九记录的「容器唯一 FAILED 是恒红测」据此改判为测量假象（16m 撤销，非代码债）|
| C-101 | `join` 的 `err(3)` 文件腿接线（先登记后动手；跨二线 `minix-types` 依赖边 + 三线 `os/commands/usr-bin/textfilter`） | 二线面**不改代码**，只新增依赖边：`os/commands/usr-bin/textfilter/Cargo.toml` 加 `minix-types = { workspace = true }`（用 `err_line`/`prog_name` 那一份权威）；三线面 `src/bin_support.rs`：`read_file` 的错误位由 `()` 改为 `minix_sys::Errno`（宿主取 `raw_os_error()`、真机传播 `read` 的号；行为不变，只多交一个号），并加 `prog_name`/`warn_line` 两个写行入口；本包十三个共用该缝的壳各改一处 `Err(()) → Err(_)`（编译器逐点指认）；`src/bin/join.rs` 接 `err(3)` 腿 + 两行 usage + 新落 C 的 `errx(1, "only one input file may be stdin")` 支；新建端到端门 `tests/join_face.rs`（6 枚）。批次二百四十九 ｜**批次二百五十五销边**：调用点改走 `minix_sys::` 再导出后，`os/commands/usr-bin/textfilter/Cargo.toml` 的 `minix-types` 直依赖已删除，本条依赖边不复存在（原登记的三线代码改动不动、行为不变）。
| C-100 | `what` 的报告行改回 `perror(3)` 那一族（先登记后动手；跨二线 `minix-types` 的一条依赖边 + 三线 `os/commands/usr-bin/doctools`） | 二线面**不改代码**，只新增依赖边：`os/commands/usr-bin/doctools/Cargo.toml` 加 `minix-types = { workspace = true }`（与 `fileops`/`shell` 同一先例，用其 `perror_line`/`err_line`/`prog_name` 那一份权威）；三线面 `src/bin_support.rs` 加写行入口 `perror_line(label, code)`、`src/bin/what.rs` 的 `read_file` 由 `Option<Vec<u8>>` 改 `Result<Vec<u8>, Errno>`（只有开档失手走 `perror`+退，读错按 C 的 `getchar()` 当文件尾继续）、新建端到端门 `tests/what_face.rs`（4 枚）。批次二百四十八 ｜**批次二百五十五销边**：同上，`os/commands/usr-bin/doctools/Cargo.toml` 的 `minix-types` 直依赖已删除，本条依赖边不复存在。另记：该 crate 与 `textfilter` 都产出同名 `what` 二进制（共享 `target/debug/what`），已登 `TODO-LEDGER-OPEN.md` §7。
| C-98 | `link`/`unlink` 的 `perror(3)` 腿接线，并把「`err(3)` 族缀程序名、`perror(3)` 不缀」写成一处权威（先登记后动手；跨二线 `os/libs/minix-types` + 三线 `os/commands/bin/fileops`） | 二线面 `os/libs/minix-types/src/types/errno.rs`：新增 `perror_line(label, code, out)`（逐字对位 `minix3/lib/libc/stdio/perror.c`：`<串>: <句子>`、空串时连 `": "` 都没有、**不打程序名**），与 `err_line`/`prog_name` 并排；三线面 `os/commands/bin/fileops/`：`src/bin_support.rs` 加写行入口 `perror_line`（渲染 + 换行、不退，退由调用者定），`src/bin/unlink.rs`、`src/bin/link.rs` 的失败臂交出那个号而非操作数（C 是 `perror("unlink")`/`perror("link")`，`unlink.c:56`、`link.c:57`）；新建端到端门 `tests/linkops_face.rs` 4 枚 | 🔄 `2026-10-14`（qorder，`claim/UNLINK-PERROR-qoder`）：minix-fileops 397 全绿（基线 393，+4 门）、minix-types 331、三架构零 error、五枚变异全咬（含「给 perror 加程序名前缀即 glibc 化」这一枚——它专门守住两族函数的之别）。余下各壳接线逐枚另轮 |
| C-97 | `mkfifo` 的 `warn(3)` 腿接线：一行里既给程序名也给那个号的句子（先登记后动手；跨二线 `os/libs/minix-types` + 三线 `os/commands/bin/fileops`、`os/commands/bin/shell`） | 二线面 `os/libs/minix-types/src/types/errno.rs`：新增 `prog_name(argv0)`（规则逐字取自 `minix3/lib/csu/common/crt0-common.c:156-165`，取最后一个 `/` 之后那段），与批次二百三十七的 `err_line` 同处一个权威；三线面 `os/commands/bin/shell/src/bin_support.rs`：本地那份同名实现改为转发，消除两处切法不一致的风险；三线面 `os/commands/bin/fileops/`：`Cargo.toml` 加 `minix-types` 依赖、`src/bin_support.rs` 新增 `prog_name` 与 `warn_line`（渲染 + 写 + 不退，对位 `minix3/lib/libc/gen/vwarn.c`）、`src/bin/mkfifo.rs` 的逐名失败腿改用它（`mkfifo.c:95`，且失败只记 `exitval` 不提前离开）；新建 fileops 第一扇端到端门 `tests/mkfifo_face.rs` 4 枚 | 🔄 `2026-10-14`（qorder，`claim/MKFIFO-ERRSUFFIX-qoder`）：minix-fileops 393 全绿（基线 389，+4 门）、minix-shell 81、minix-types 331、三架构零 error、四枚变异全咬（丢句子/留目录/提前退出/两字段对调）。其余 fileops 壳的同款接线逐枚另轮 |
| C-96 | `err(3)` 那一行的渲染下沉为 `minix-types` 的唯一权威，并按 C 补齐 `hostname`/`domainname` 的诊断行（先登记后动手；跨二线 `os/libs/minix-types` + 三线 `os/commands/bin/shell`、`os/commands/games/stdio-games`） | 二线面 `os/libs/minix-types/src/types/errno.rs`：新增 `err_line(prog, label, code, out)`（形状逐字取自 `minix3/lib/libc/gen/verrc.c`，句子出自本树的 `SYS_ERRLIST`/`Strerror`），并加 `err_line_renders_c_verrc_shape` 一枚；三线面 `os/commands/games/stdio-games/src/bin_support.rs`：本地那份渲染删掉，改 `pub use minix_types::err_line` 转发（七个共享面测不动）；三线面 `os/commands/bin/shell/`：`src/bin_support.rs` 新增 `prog_name`（`getprogname()` 按 `minix3/lib/csu/common/crt0-common.c:156-165` 取 `argv[0]` 末段）与 `err_exit`，并给 `warn` 补宿主双生面（宿主走 `std::io::stderr`，否则诊断进替身传输、宿主看不见）；`src/bin/hostname.rs`、`src/bin/domainname.rs` 的 `report_err`/`usage` 交程序名与句子；新增端到端门 `tests/hostname_face.rs` 4 枚 | 🔄 `2026-10-14`（qorder，`claim/ERR-LINE-SHARED-qoder`）：minix-shell 6 目标 81 全绿（基线 5/69）、minix-types 331、stdio-games 184 不动、三架构零 error、五枚变异全咬。其余命令的后缀接线转下轮（18-stage-commands todo.md 批次二百三十七「登记不执行」①） |
| C-95 | 全族 `strerror` 基础件：把 Minix3 自己的 `sys_errlist` 落成 `minix-types` 的 errno 句子表，并接第一个消费者 caesar（先登记后动手；跨二线 `os/libs/minix-types` + 三线 `os/commands/games`） | 二线面 `os/libs/minix-types/src/types/errno.rs`：新增 `SYS_NERR`、`SYS_ERRLIST`（80 项，真源 `minix3/lib/libc/compat/gen/compat_errlist.c:52-151`，全树唯一一份定义）、`Errno::message()`、`Strerror`（表外拼 C 的 `Unknown error: %u`，`minix3/lib/libc/string/strerror_r.c:66/:78`），并把 `impl Display for Errno` 的注释从「C strerror 的面」改准为标识符面（名与句两个面此前混为一谈）；三线面 `os/commands/games/stdio-games/`：`Cargo.toml` 加 `minix-types` 依赖、`src/caesar.rs` 的 `RotationFailure::errno()`（`caesar.c:133`/`:135`）、`src/bin/caesar.rs` 按 `minix3/lib/libc/gen/verrc.c` 缀句、`tests/caesar_face.rs` 两句金样随之更新（顺带纠正旧注释里照抄 glibc 的 `Numerical result out of range`） | 🔄 `2026-10-14`（qorder，`claim/STRERROR-TABLE-qoder`）：minix-types 330 全绿（+4）、stdio-games 172 全绿、三架构零 error、七枚变异全咬。同日 `claim/STRERROR-WIRE-qoder`（批次二百三十四）接上第二处消费者 factor 的溢出腿（`ERANGE` 与 `illegal numeric format` 分句），stdio-games 173 全绿、五枚变异全咬。同日 `claim/STRERROR-SEAM-qoder`（批次二百三十五）把 `stdin_chunk` 的失败面改成带 `Errno`、共享件加 `err_line`，接上 `factor: stdin` 与 `ppt: fgets` 两句 `err`，并用目录描述符当标准输入让这条腿第一次进程级可测；stdio-games 184 全绿、五枚变异全咬。同日 `claim/STRERROR-NOTES-qoder`（批次二百三十六）更正命令族 12 处「本树无 strerror 表」的假理由（基线 git grep 12 → 本树 0），并在核对中发现 `hostname`/`domainname` 的 `err(1,…)` 还缺 `getprogname()` 前缀，已按「不顺手修」登记另轮。其余消费者的后缀接线记为后续轮，见 18-stage-commands todo.md 批次二百三十三「登记不执行」① |
| C-94 | `find` 播种上机与三架构判据（先登记后动手；单域三线 commands + 同线 xtask/qemu-tests，但碰启动共享件，沿用 C-93 先例） | 三线面 `os/xtask/src/image.rs`（imgrd `/bin` 名单加 `find`）、`os/qemu-tests/test-cmd-smoke.sh` 与 `-aarch64`/`-riscv64` 变体（各加一行「恰为 `/etc/rc`」的形状判据）、启动共享件 `os/etc/rc`（加一行 `/bin/find /etc/rc`） | `find` 只读，文件操作数经隐式 `-print` 吐一行 `/etc/rc`，把整条 parse→walk_paths→lstat→eval→emit 腿收到串口；目录操作数因 imgrd 根盘 getdents 读腿未通（同 du/ls）会走 FTS_DNR 只报 warn，故本批只判文件操作数 | ✅ `2026-10-13`（qoder，随 FIND-SEED 合入销账：x86_64 `test-cmd-smoke.sh` 连续两轮 `RESULT: PASS (… the find command face)`，三门 `bash -n` 与 riscv64 python 判据负控验证均过；aarch64/riscv64 同源同判据待需时上机，同 du 批次一百二十一先例） |
| C-93 | `stat` 播种上机与三架构判据（先登记后动手；单域三线 commands + 同线 xtask/qemu-tests，但碰启动共享件，沿用 C-87 的先例） | 三线面 `os/xtask/src/image.rs`（imgrd `/bin` 名单加 `stat`）、`os/qemu-tests/test-cmd-smoke.sh` 与 `-aarch64`/`-riscv64` 变体（各加三种模板的形状判据）、启动共享件 `os/etc/rc`（加三行 `/bin/stat`、`/bin/stat -l`、`/bin/stat -s`） | `stat` 只读，不受只读 imgrd 根盘门控，是命令家族里第二个可在真机判读量的命令；判据按形状而非数字（盘上属主/大小/时间戳是镜像属性）。三门本地皆 `RESULT: PASS`，x86_64 串口读数见 `18-stage-commands/todo.md` 批次一百三十二 | ✅ `2026-10-12`（qoder，随 CMD-STAT-SEED 合入销账：x86_64/aarch64/riscv64 三条 `test-cmd-smoke*` 全绿，宿主 `cargo test -p minix-fileops --lib --bins` 294+2/0，权威编译门 0 error） |
| C-92 | 更正「内核 `--lib` 宿主测面不可作为门」这条台账行的口径，并把宿主测配方写进项目指令入口（先登记后动手；一线 `os/kernel` 注释 + 仓库共享件 `AGENTS.md`） | 一线面 `os/kernel/src/lib.rs` 的 `form_contract` 文档注释（绊线语义补精度：裸跑除绊线本身外还会在 `store_protection`/`store_trap_entry`/`CurrentDirectMap::default` 三处消费面同报同一形态错配）；仓库共享件 `AGENTS.md` 的「Build & Test」段（新增一行内核宿主测的显式形态配方，不裁剪既有条目） | 该行按不带形态的 `cargo test -p minix-kernel --lib` 的 5 件编译错写成「宿主测不可作为门、命令面各批只以 `cargo check --workspace` 与三架构 build 为准」，而 `PD-30` 裁决落地的 CI 作业 `kernel-tests.yml` 的 `host-default-form` 腿早就是显式 `--features mock` 的同一套测试；不更正就会长期自废一条门 | ✅ `2026-10-12`（qoder，随 KERNEL-LIB-HOSTTEST 合入销账：`cargo test -p minix-kernel --features mock` → 831 通过 / 0 失败 / 8 忽略（lib 腿）+ `boot_integration` 3 通过 / 3 忽略，与 CI `host-default-form` 腿逐字同形；不带形态的裸跑仍报同一 5 处（绊线按设计显形），已把配方写进 `AGENTS.md`、把该说明写进绊线注释本身） |
| C-91 | 滚除 NK4-C 1.5c 的 OOM 打印钩子（先登记后动手；跨二线 `os/libs/minix-rt` 与一线 `os/kernel-image`） | 二线面 `os/libs/minix-rt/src/lib.rs`（`alloc()` 的 32 件封顶 OOM 打印 + `nk4c_oom_tag` 本体）、一线面 `os/kernel-image/src/main.rs`（内核 bump arena 耗尽分支的 `nk4c_oom_hex` 打印）、三线面 `os/commands/sbin/init/src/main.rs`（1.10t「逐环仪器化」注释残骸，零行为） | 批次一百二十七给回的口径（三式并扫 servers/fs/drivers 之外的站）扫到这两处仍会写诊断通道的活体钩子，注释自陈 task1-close 裁决删除，而 1.5c 要锁定的「slab OOM 落在哪台服务器」随 task1 结案；`global_alloc_diag`/`AllocDiag`/`pool_diag` 一族按 `PD-10` 第二档留作正式诊断面（SD-11 T1 的 VM_BRK 安装后置条件测在用），本笔不动它们只重述文档 | ✅ `2026-10-12`（qoder，随 PROBE-SCAN-RT-KIMAGE 合入销账：minix-rt 的 OOM 打印钩子与 `nk4c_oom_tag` 本体、kernel-image 的 `nk4c_oom_hex` 本体与耗尽分支打印、init 的注释残骸皆滚除；diag 一族改判 `PD-10` 第二档留任，门证见 `18-stage-commands/todo.md` 批次一百二十八） |
| C-90 | 命令冒烟门接进持续集成（先登记后动手；仓库共享件 `.github/workflows/`） | 新增 `.github/workflows/cmd-smoke.yml`（不改既有的 `qemu-tests.yml`，只为让触发面各跟各的：那条跟引导载体，这条跟命令面与装配面） | `C-57` 命令冒烟门此前只可手跑；三架构门本地已跑绿，接入后由 PR 触发。作业按矩阵跑三条门，退出码 0/2/1 映射 PASS/SKIP/FAIL，SKIP 不红但显式打印 | ✅ `2026-10-12`（qoder，随 CMD-SMOKE-CI 合入销账：`python3 -c "import yaml; yaml.safe_load(open('.github/workflows/cmd-smoke.yml'))"` 通过；退出码映射 rc=0→PASS、1→FAIL(exit 1)、2→SKIP(exit 0) 逐项实测；三门本地皆 `RESULT: PASS` |
| C-110 | `random` 与 C 分叉六处：操作数个数、两句分母报文、`double` 取值域，以及**它本该逐行过滤输出多行**（纯台账笔先量分叉、定三片修法；单域三线 `os/commands/games/text-games`） | 片1 `src/bin/random.rs`（个数与两句报文）+ `src/bin_support.rs`（补 `warn_line`）；片2 过滤器主体与 `-e`/`-r` 两枚旗标；片3 `src/lottery.rs` 的 `parse_denominator` 改 `double` 语义 | C 零枚操作数取 `denom = 2` 继续干活、一枚走 `strtod` 加 `errno == ERANGE` 三段形或 `errx(1, "denominator is not valid.")`（**不带操作数**）、两枚以上 `usage()` 退 1（`minix3/games/random/random.c:86-105`、`:143-147`）；主体是 `selected = (int)(denom * random() / RANDOM_MAX) == 0` 的逐行过滤（`:114-137`）。本壳打 usage 退 1、句子自创且多带操作数、只收十进制、忽略多余操作数、且**只输出一行**（`os/commands/games/text-games/src/bin/random.rs:29-32`、`src/lottery.rs:13-32`） | 🔄 `2026-10-18`（qoder 开单，本笔未动码：参照件现量六形——`abc` 与 `0` → `denominator is not valid.` 退 1，`1e999` → 三段形退 1，`1.5` → 退 0，`2 3` → 一行 usage 退 1，无操作数 → 退 0 打出三行里的两行，`random 1` → 三行全打；`random_ref` 与读数在 `tmp/random-342/`，垫片需 `-DRANDOM_MAX`（Minix 专有常量，只影响抽样流不影响句子与退码）。三片修法与互不阻塞性、`wtf` 无 C 真源不可按 C 接、`fortune` 另有三处 `warn` 需单开一支，全写在 18 篇批次三百四十二那一条）|
| C-109 | `tr` 的等价式两句报文（`Misplaced equivalence equals sign` / `Misplaced equivalence right bracket`）未接，`[=ab=]` 这类形被悄悄当成合法式子收下（单域三线 `os/commands/usr-bin/textfilter`） | `src/tr.rs`（`SetGrammar` 两枚变体与等价式分支）、`tests/tr_face.rs`（面门一枚） | C 分两步：`bracket()` 只认「式内有没有 `=]`」（`minix3/usr.bin/tr/str.c:193-199`），认定后 `genequiv()` 逐枚核内部——成员一枚、然后 `=`、然后 `]`，两处各自 `errx(1, …)`（`:305-330`）；本壳只做第一步，两句一个都没接 | ✅ `2026-10-18`（qoder，闭合批次三百三十五登记的 `TR-EQUIV-SENTENCES`：`--lib tr::` 21/0（新立 3 枚）、`--test tr_face` 15/0（新立 1 枚）、宿主整包 1067/0（68 target）、**权威容器门 1067/0/0 退码 0**（`tmp/tr-341/container-gate.log`）、三架构各 Finished、workspace 0 error、触及两件 rustfmt 零漂移、clippy 退码 0 且触及件零命中；A/B 对 `tmp/tr-322/tr_ref` 十一形 × 两串位共 22 次 SAME=22 DIFF=0（`tmp/tr-341/ab341.log`）；负控六枚全咬 2-1/1-1/2-1/1-1/2-1/4-1（`tmp/tr-341/ctl341.log`）；门自己先写错一次期望（把 `[=c=]` 的成员当 `a`），靠参照件现量 `abx` 改正而不是改代码；核实日 `2026-10-18`、基线 `02d83cedf`。`TR-EQUIV-OOBCH-SET2` 不在本支射程，仍在册）|
| C-108 | `split` 的 `-n` 把字节数取自已读到的长度而不是取自内核（纯台账笔先量分叉、定改法；单域三线 `os/commands/usr-bin/textfilter`） | 下一轮动 `os/commands/usr-bin/textfilter/src/bin_support.rs`（`read_operand`、`read_stdin_error` 两条腿各交回大小）、`src/bin/split.rs`（`-n` 分支换掉 `input.len()`、新增 `stat` 失手臂）、`tests/split_face.rs`（两枚新门） | C 在读数据之前先 `fstat(ifd, &sb)`，失手 `err(1, "stat")`（`minix3/usr.bin/split/split.c:293-295`），并用 `sb.st_size` 判块数上限与算块宽（`:297-300`）；本壳用读全之后的 `input.len()` | ✅ `2026-10-18`（qoder，随取数那一步落地销账：`cargo test -p minix-textfilter --test split_face` → **23 通过 / 0 失败**（新立 3 枚：管道那形退 1 且零分片、命名档那形不退化、报文里的数是大小不是片数）；宿主整包 `cargo test -p minix-textfilter --offline -j 1` → **1063 / 0**、68 个 target；**权威容器门 1063 / 0 / 0、退码 0**（`tmp/split-340/container-gate.log`）；A/B 与参照件三形全 SAME，上一笔的管道 DIFF 消失（`tmp/split-340/ab340.log`）；负控六枚全咬 1/2/2/1/2/2、还原后 23/0（`tmp/split-340/ctl340.log`）；宿主三架构各 Finished、workspace 0 error、触及件 rustfmt 零漂移、clippy 触及件只剩基线里就有的「test 模块之后还有条目」一条；核实日 `2026-10-18`、基线 `dd77ce7c0`。两条如实登记：`stat` 失手那一臂只有代码形状、没有门——宿主开得出且读得动的档，问大小问的是同一份 inode 信息，构造不出失手，真机侧归 E5 既有口径；「先收全输入再切块」这件事本笔没做，`SPLIT-STREAMING-MEMORY` 名下的流式改造仍敞着）|
| C-107 | `split` 的输入拆成开档、读档、标准输入三形各归各句，并给读腿补上错误号（先登记后动手；单域三线 `os/commands/usr-bin/textfilter`） | 三线面 `os/commands/usr-bin/textfilter/src/bin_support.rs`（`InputFailure`、`read_operand` 两条 `cfg` 腿）、`src/bin/split.rs`（三形分派、`finish`）、`tests/split_face.rs` | C 两句话：`split.c:136-138` 开不出来报操作数原文、`:185-190`/`:252-257` 读不进去报 `read` 一词（`ifd` 初值是 0，标准输入与命名档同一条腿）；本壳把两臂折进一只 `Result`，标准输入那条腿 `read_stdin()` 没有错误通道 | ✅ `2026-10-17`（qoder，随 `SPLIT-READ-STAT-ERRLEG` 读腿一片合入销账：`cargo test -p minix-textfilter` → 1060/0、68 target；容器门 1060/0/0 退码 0（断三笔后首次跑成，配方见 18 篇 §7）；`--test split_face` → 20/0，新立 1 枚、补强 1 枚；宿主三架构各 Finished、workspace 0 error、触及件 rustfmt 零漂移；负控五枚全咬 2/2/2/1/1，其中两枚是先发现门不够（只比前缀不比整句、没有门跑过读不动的标准输入）才咬住的；`stat` 腿未接，随 `SPLIT-STREAMING-MEMORY` 下一片）|
| C-106 | `split` 分片文件的 `close` 腿接成第三臂（先登记后动手；单域三线 `os/commands/usr-bin/textfilter`） | 三线面 `os/commands/usr-bin/textfilter/src/bin_support.rs`（`SinkFailure`、`close_outcome`、两腿的 `finish`）、`src/bin/split.rs`（`ChunkWrite::Close`、`failure_label`、`sink_failure_of`） | C 一处一腿一句：`split.c:331-332` 开档报路径、`:319-320` 的 `close(ofd) != 0` 也报路径、`:204`/`:216`/`:225` 的写失败报 `write`；本壳目标腿把 `minix_sys::close` 的返回用 `let _ =` 吞掉，宿主腿由 `std::fs::File` 的 drop 关闭、不交回号 | ✅ `2026-10-18`（qoder，随 `SPLIT-CLOSE-ERRLEG` 合入销账：`cargo test -p minix-textfilter` → 1058/0；新立三枚表门与映射门；宿主三架构各 0 error、`cargo check --workspace` 0 error、触及件 rustfmt 零漂移、clippy 触及件零命中；负控六枚中五枚咬，第六枚改的是 `cfg(not(test))` 侧接线、宿主到不了，归真机门） |
| C-105 | `split` 的写腿交回平台号而不是被压平的 `EIO`，并给两臂的标签各立一枚门（先登记后动手；单域三线 `os/commands/usr-bin/textfilter`） | 三线面 `os/commands/usr-bin/textfilter/src/bin_support.rs`（`WriteSink::finish` 两条腿的返回类型与目标腿的续写循环）、`src/bin/split.rs`（`write_into`、`failure_label`）、`src/bin/uniq.rs`（签名跟随）、`tests/split_face.rs` | C 两臂各打一句：开不出来是 `err(1, "%s", fname)`（`minix3/usr.bin/split/split.c:332-333`），写不进去是 `err(1, "write")`（`:204`、`:216`、`:225`），第三段都是 `errno`；这一支的 `finish` 交回 `Result<(), ()>`，壳里再折成 `EIO`，且目标腿只 `write` 一次 | ✅ `2026-10-18`（qoder，随 `SPLIT-WRITE-ERRNO` 合入销账：`cargo test -p minix-textfilter` → 979/0；新立三门（写腿保号、两臂标签不互换、开不出分片报路径）；负控六枚全咬；宿主三架构与 `cargo check --workspace` 各 0 error；触及件 rustfmt 零漂移） |
| C-104 | `tr` 的集合语法四句报文接上标准错误，并把 `bracket()` 的三条成形判据照抄进本层（先登记后动手；单域三线 `os/commands/usr-bin/textfilter`） | 三线面 `os/commands/usr-bin/textfilter/src/tr.rs`（`SetSite`/`SetGrammar`/`SetError` 与 `parse_set` 的 `[` 分流）、`src/bin/tr.rs`（`expand` 打句并退 1）、`tests/tr_face.rs`（新立四枚门） | `parse_set` 把四类语法失败都折成 `TextError::InvalidArgument`，壳里静默退 1；C 在 `minix3/usr.bin/tr/str.c:253`、`:353-356`、`:362-366`、`:368-386` 四处各打一句，且 `bracket()`（`:176-210`）先用 `strpbrk(s->str + 2, "*]")` 判成形，没成形的 `[x]`、`[:alpha`、`[x` 在参照件里是字面量且退 0 | ✅ `2026-10-17`（qoder，随 `TR-SET-GRAMMAR-SENTENCES` 合入销账：`cargo test -p minix-textfilter --lib tr::` → 18/0；`cargo test -p minix-textfilter --test tr_face` → 14/0；与参照件 `tmp/tr-322/tr_ref` 的 21 形 A/B → SAME=20 / DIFF=1，唯一那条已拆登记） |
| C-103 | 把「销账」与「删目录」拆成两件事核验，并给共享磁盘加一条只读清点（照规则 3 登记：`tools/` 共享件） | `tools/claim.sh`（`release` 在 `git worktree remove` 之后加目录物理消失核验，未消失报 `WARN` 并打印占用；新增只读子命令 `sweep`；头注用法段同步，`usage()` 的 `sed` 区间随之挪一格）＋ `rewrite-notes/coordination/CONCURRENT-WORK-PROTOCOL.md` §4（要点扩到第五条，内容是销账之后要用物理视角确认工作树是否真的从磁盘上消失）与 §11 可粘贴版收尾段 | 病根：`git worktree remove` 先注销 Git 登记表再去删目录，第二步能单独失败而第一步已生效——`git worktree list` 看不见那棵树，磁盘上整棵检出原样留着。触发条件在权威编译门配方里（`AGENTS.md` 的 `docker run --rm -v "$PWD:/work" -w /work/os -m 2g minix-ci:1.94` 不带 `-u`，容器内 root 把 `os/target/debug/incremental/` 写成 root 属主，宿主用户删不动）。现场读数（清理前后实测）：`.wt` 共 50 G、逐棵 4.3–4.5 G，而 `git worktree list` 只报得出两棵；剩下 11 棵的 claim 分支全部不存在（`git branch --list 'claim/*'` 现算只剩本线一把），且树内文件百分之百落在 `os/target/` 里（逐棵 `find -type f \| grep -vc '/os/target/'` 读数 0）——即「目录还在、而 Git 登记表条目与 `claim/` 分支都不在」这种形态（至于它是 `release` 删不动留下的还是被手工弃掉的，效果一样：没有一条命令会再提示它占着磁盘）。修法三件：工具自检（`WARN` 取代未成立就打印的「已删除工作树」）、只读盘点（`sweep` 给 在册/可回收/需核对 三判定＋锁树成对检查＋各树占用）、删除判据按代价排序（只剩构建缓存才可删，其余先做 `git rev-list --objects --all` 的 blob 对账或救援副本；非本人属主条目打印 `sudo` 交人执行，纪律载体不代为提权）。刻意不选的方案：①把 `sweep` 挂成 `TODO-LEDGER-INDEX.md` §6 的第六道收尾校验——「五道校验」这个措辞在六份跟踪文件里被引用（`TODO-LEDGER-INDEX.md`、本文件、`AI-chats/todo-fix-prompt.md`、`18-stage-commands/todo.md`、`01-stage-kernel/02-higher-half-kernel.md` 与其 `.design` 快照），改成六道要动一批文件，而清理动作的 enforcement 点本来就长在 `release` 这一步，落在协议 §4 要点与 §11 收尾段即可，零涟漪；②给 docker 配方加 `-u $(id -u):$(id -g)` 从源头消除 root 属主——那是改权威编译门配方，影响所有线与 CI，属需要人拍板的平台裁决，本笔只登记事实不动配方 | ✅ `2026-10-17`（qoder，todo-fix 循环 `claim/REV-STDIN-ERRLEG-qoder` 线）：`sh -n tools/claim.sh` 通过；`unknown` 分支的 usage 打印含新增的 `sweep` 行且退码 2；五组负控全部成像——`sweep` 对纯构建缓存目录判「可回收」（读数 `0 个文件在 os/target 之外`）、掺进一个 `.wt/probe-sweep-qoder/rewrite-notes/a.md` 即改判「需核对」（防误删判据生效）、临时 `claim/SWEEPROBE-qoder` 有锁无树被判「失配 1 处」且撤锁后归零、探针树带一处未提交改动时 `release` 退码 1 并保留锁与树、改动 `checkout --` 复原后 `release` 退码 0 且 `ls -d` 报目录不存在。全程主树未提交项只含其它线程那 18 件与本笔的 `tools/claim.sh`，探针件零落地（`.wt` 无探针目录、无 `SWEEPROBE*` 分支、`git worktree list` 回到两棵）。**同一笔的第二轮：流程彩排把本笔的因果推断证伪了一半。**彩排（`claim/FLOWCANARY-qoder`，一处行为中立的 `fold.rs` 单测，轨迹 `tmp/flowcanary-20261017/trace.md` 共 30 节带命令原文）在 claim 树内真跑了容器门 `docker run --rm -v "$PWD:/work" -w /work/os -m 2g minix-ci:1.94 cargo test -j 1 -p minix-textfilter` → 通过=916 失败=0（68 个 target）、docker 退码 0，树内留下 6745 个 root 属主条目，于是第一次拿到了真实标本而不是走查。它当场揭出四处，其中第四处正是历史堆积的机制：①`release` 的 `WARN` 分支在真实失败场景里**根本走不到**——`git worktree remove` 此时返回非零，代码落到 `|| { echo FAIL; exit 1; }` 提前退出；②那句 FAIL 把原因咬死成「内仍有未提交改动」而吞掉了 git 原文，实测原因其实是 `Permission denied`，误诊会把人引去 `--force`；③`exit 1` 在 `git branch -D` 之前，所以锁没销，而登记表条目已经被注销——`sweep` 于是报出假信号「有锁无树，必须补树」，可那棵 1.6 G 的树就摆在磁盘上；④补第一处时又查出真正静默的那一支：条目早已不在了，`wt_of` 返回空，`release` 按「没有树要删」的分支直接打「已释放」，残骸一字不提——第一次 release 报错退出留下锁、第二次进来静默成功，`claim/` 分支消失而目录长留，就是那 11 棵的来历。四处已一并修掉（`wt_path()` 统一命名约定；`FAIL` 附 git 原文并按登记表分流，条目仍在册才退码 1 留锁；无条目而目录在则销锁前报目录与占用；`sweep` 把不成对分成 `残骸` 与 `失配`）。修后用活标本与受控标本复跑四支全成像：脏树 `退码=1` 且锁保留（`git branch --list 'claim/*'` 可见）、干净后 `退码=0` 且 `ls -d` 报不存在、条目注销而目录在报 `WARN` 且锁照销、无条目而目录在报 `WARN`＋`du` 读数后锁照销。彩排自身也留下一处勘误：轨迹里两处「退码=0」是脚手架把 `$?` 在外层 shell 提前展开造成的假读数，已另写勘误节并用单引号脚手架重测为 1 与 0。彩排的提交物归档在 `tmp/flowcanary-20261017/0001-canary-fold-rewrite.patch`（分支已随 release 销掉，不入 `rewrite`）；它留下的 `.wt/flowcanary-qoder` 1.6 G 残骸含 root 属主条目，需人执行 `sudo rm -rf` 回收——本笔按规矩不代为提权 |
| C-89 | riscv64 命令冒烟门与 boot-full 门的播种名单归单（先登记后动手；三线 os/qemu-tests 门脚本，与 init/内核线的启动面共用） | 三线面 `os/qemu-tests/test-cmd-smoke-riscv64.sh`、`os/qemu-tests/test-riscv64-boot-full.sh` | 两脚本各手抄一份 `for n in sh echo ls cat` 播种行，与 `os/xtask/src/image.rs:367-374` 的单源漂移，导致 x86_64/aarch64 已播 `du` 而 riscv64 未播；本笔改为从单源抽名单并在 `ls` 判据里区分「列得出」与「显式 read error」 | ✅ `2026-10-12`（qoder，随 RISCV64-IMGRD-SEEDS 合入销账：`bash os/qemu-tests/test-cmd-smoke-riscv64.sh`（`RS64_SMOKE_SKIP_BUILD=1`）→ `RESULT: PASS`，串口 `24\t/etc`、`12K\t/etc`、`du: /etc`、`ls: /bin: read error` 与另两架构逐字一致；`bash -n` 两门脚本通过 |
| C-88 | 修一线内核的 aarch64/riscv64 生产编译面（先登记后动手；三线 → 一线跨界改点） | 一线面 `os/kernel/src/syscall.rs`（给 `X86_64Syscall` 的定义与 impl 加 `#[cfg(target_arch = "x86_64")]`） | 命令面门接线的下游实锤：`cargo build -p minix-kernel --target aarch64-unknown-none --no-default-features` 与 riscv64 同款各起 3 件 `E0433: could not find CurrentPortIo in minix_plat`（`os/plat/src/lib.rs:148-151` 只在 x86_64 或 mock 下定义该别名；而 `CurrentArchSyscall` 本就只在 x86_64 选它，另两架构走 `DefaultSyscall`——即这段 x86 专属 leg 在另两架构根本不被引用，只是没被 cfg 挡住） | ✅ `2026-10-12`（qoder，随 A64-R64-KERNEL-COMPILE 合入销账：`cargo build -p minix-kernel --target aarch64-unknown-none --no-default-features` 与 riscv64 同款由 3 件 E0433 转 0 error；`bash os/qemu-tests/test-cmd-smoke-aarch64.sh` → `RESULT: PASS`；`docker run --rm -m 2g minix-ci:1.94 cargo check --workspace -j 1` → 0 error） |
| C-87 | `du` 播种上机与串口判据（先登记后动手；单域三线 commands + 同线 xtask，但碰启动共享件） | 三线面 `os/xtask/src/image.rs`（imgrd `/bin` 名单）、`os/commands/bin/fileops/**`（批次一百二十已合入）、`os/qemu-tests/test-cmd-smoke.sh` 与 `-aarch64`/`-riscv64` 变体、启动共享件 `os/etc/rc`（加一行 `/bin/du /etc`，并按盘上实测改正头注里「ls→getdents 端到端已通」的口径） | 命令面第一个只读命令的门：x86_64 已跑绿并读到 `24\t/etc` 与 `du: /etc` 两行；aarch64/riscv64 当下连镜像都装不出——内核编译面被 `minix_plat::CurrentPortIo` 缺两架构别名断住，已作为新行记入 `TODO-LEDGER-OPEN` 三架构齐平段交一线处理，本笔不代改一线码 | ✅ `2026-10-12`（qoder，随 DU-MACHINE 合入销账：`bash os/qemu-tests/test-cmd-smoke.sh` → `RESULT: PASS`；`cargo test -p xtask` → 17/0）；后续批次一百二十二（`HUMANIZE-DU-H-qoder`）沿用同一登记再动这两处共享件——`os/etc/rc` 多一行 `/bin/du -h /etc`、三门脚本各多一条 `-h` 判据，x86_64 已复跑绿 |
| C-86 | 行尾归一收尾（两件他人领地文件）与把两个仓库级门接进持续集成（照规则 1 登记） | 二线 `os/drivers/storage/at_wini/src/bus.rs`、一线 `os/libs/minix-boot/src/boot_form.rs`（均仅去 CRLF，零内容改动）、仓库共享件 `.github/workflows/rules-consistency.yml` | C-85 留下两件不属本线领地的 CRLF 文件，P8 因此仍红 2 项、丙子面（接闸）被它卡住。本笔把这两件归一（逐文件比对 `git show HEAD:<路径>` 去 `\r` 后与工作树逐字节相同，`git diff --ignore-cr-at-eol` 为空），并在 `rules-consistency.yml` 新增「仓库级门禁」一步跑全量 `bash tools/pattern-gate.sh` 与 `bash tools/check-command-boundary.sh`——两者已归零故可直接成为阻断门 | ✅ `2026-10-12`（qoder，随 P8-ZERO-AND-CI-GATES 合入销账：`bash tools/pattern-gate.sh` → `FAIL=0`，`[P8] PASS`） |
| C-85 | 行尾归一与入库前防线（碰共享件 `.gitattributes`，并点出两个跨线残留文件） | 共享件 `.gitattributes`（新增 `* text=auto`）；自有 `os/commands/**` 55 个 CRLF 文件；**不动**的两件属他人领地：`os/drivers/storage/at_wini/src/bus.rs`（二线）、`os/libs/minix-boot/src/boot_form.rs`（一线，C-40 已确权归 boot 线） | 台账 §7「门不在闸上」一行的乙子面：P8（跟踪文件 CRLF 入库）长期 57 项红着。本笔把本线 55 个文件归一（逐文件比对过 `git show HEAD:<路径>` 去 CR 后与工作树逐字节相同，`git diff --ignore-cr-at-eol` 为空，纯行尾零内容改动）并加 `.gitattributes` 把规矩变成提交时拦截。余下两件请各自线一次性归一（一行命令即可：`python3` 去 `\r\n`→`\n`，或 `dos2unix`） | ✅ 本线 55 件 + `.gitattributes` 已随 LINE-ENDINGS-GITATTRIBUTES 合入；P8 由 57 降到 2（两件待各自线处理） |
| C-84 | 边界守卫翻绿：`df` 的 statvfs 访问子改走 minix-sys 再导出（照规则 1 登记；跨二线 minix-sys + 三线 commands/mountinfo） | 二线面 `os/libs/minix-sys/src/lib.rs`（在既有 `Stat` 显式再导出旁补 `pub use minix_types::{StatvfsBuf, statvfs_off};`）；自有面 `os/commands/sbin/mountinfo/src/{bin/df.rs,df.rs}`（use 改指 minix_sys、模块文档引用同步） | `tools/check-command-boundary.sh` 自批次四十（`df` 接线）起红着、批次一百零五收尾校验时现算发现：`os/commands/sbin/mountinfo/src/df.rs:15` 与 `src/bin/df.rs:27` 直引 `minix_types`，违守卫自身提示「缺封装去 14 侧登记，不在命令里手搓」。修法采守卫同例（errno 常量再导出走 minix-sys）：`StatvfsBuf` 与其字节偏移模块 `statvfs_off` 是 `minix-sys::statvfs`/`getvfsstat` 已在签名里使用的同一 ABI 面，再导出即命令层唯一合法入口；不改 minix-types（权威所在），不动 VFS 服务端。台账读数登记于 `TODO-LEDGER-OPEN.md` §7 门不在闸上一行（甲子面）。 | ✅ `2026-10-12`（qoder，随 DF-BOUNDARY-REEXPORT 合入销账：守卫 `0 violation(s)` 退码 0；minix-mountinfo `--lib` 17/0、minix-sys `--lib` 323/0、freestanding `--bin df` 绿） |

---

## §3 依赖状态板（跨线前置一览）

| 前置（供方） | 消费方 | 状态 |
|---|---|---|
| new_edge2 NL2（minix-sef SefEvent::Init） | new_edge3 NS1（六服务器应答臂）、NL6（memory 进程壳） | 供方 ✅（e31a86ab5）/ **NS1 ✅（zcode_glm_4，74414dd1e——含 VM 应答半 + X-6）**；NL6 ✅（qorder_2，c0a9b81aa） |
| new_edge2 NL6（memory 驱动进程） | new_edge3 NS4（挂根）/NS6（块源）/NS8（镜像） | ✅ 供方 2026-09-20 qorder_2 c0a9b81aa（claim/NL6-qorder_2，双面消息泵+进程壳，待合入 rewrite 后下游方可消费）|
| new_edge2 NL4（pipe2/whoami/文件族 wrapper） | new_edge3 NS12（sh 等）、NL6（tty 端点） | ☐ |
| new_edge2 NL3③（sigreturn trampoline） | new_edge3 NS11（init trampoline 填真） | ☐ |
| new_edge1 NK1（C-29 载体收口） | new_edge3 NS1/NS2（boot 链真机验收）、E5 族真机半 | ✅（2026-09-21，91d3fa024/merge b5fe7e2fa——test-sysboot 真机 PASS 六标记双 DONE，run_all 接线；NS12 冒烟脚本前置同步解锁） |
| new_edge1 NK2（页故障转发） | 一切用户程序（内存故障安全网）；E5(d) 真机 | ✅（2026-09-20，5037491ff） |
| new_edge1 NK3（三架构生产 trap 腿 + timer-irq 载体） | E5-SMP（-smp 用例的中断交付前提）；E5-ARCH（三架构复跑）；一切 riscv64/aarch64 真机中断/用户往返 | ✅（2026-09-20，c1bb93c22 等 4 commit；真机 PASS ×2） |
| new_edge1 NK4/OQ-N2（12 模块契约）——✅ 契约已钉（2026-09-20，loader.rs MODULE_NAMES 12 项对 C image[] 序） | new_edge3 NS8（装机清单——须把 `/EFI/minix/modules/` 下 12 文件命名严格对齐 `ds,rs,pm,sched,vfs,memory,tty,mib,vm,pfs,mfs,init`，缺件 boot-shim 现 fail-fast panic） | 供方 ✅ / 消费方 ✅（NS8 2026-09-21 dfb910f37：装机清单三重锁对齐契约，xtask image() 按同序装机；NK4-A 真机端到端余量待 QEMU 窗口 + NS8-A kernel.elf 产出者） |
| new_edge3 NS4/NS5（挂根+exec） | NS9/NS12、T3/T4 | ☐ |
| new_edge3 NS7 + NL1（console 全链） | T4 冒烟（可见输出） | ◐ NL1 驱动半 ✅（2026-09-21，546aef11e——串口 backend + 写字节管道，真机半挂 T2）；**NS7 ✅（2026-09-21，zcode_glm_3，代码 af715dfaf / merge 50c533761——读/写臂字符路由 + CdevIo 完成面 + 回复 id 配对；登记 NS7-A 见 new_edge3）**；console 全链宿主半齐，真机半挂 T2 |

---

## §4 在制避让（2026-09-20 快照）

- 工作树：`tools/coverage-extract/ds-semantic-map.json` 有**别的线**的在制修改（前轮登记延续，本轮各线不碰）。
- 未跟踪产物：`new_todo_{deepseek,glm,HY4,muse,qwen}.md`（五路扫描留存备查，不删）；`new_edge1-4.md`（本轮工作文件）；`AI-chats/`、`redesign-notes/` 等与本轮无关。
- 前轮 edge1-4.md 已冻结（头部横幅），只读参照；其未决行的携带映射见本文件 §2/§7。

---

## §5 E5 端到端联调包编排（条目主体在 edge_todo.md E5）

| 子项 | 内容 | 前置 | 状态 |
|---|---|---|---|
| E5(a) | PM↔VM fork 全链路 | — | 🔄 宿主半 ✅（430803d4f）；真机半挂 T2（NS1/NK1） |
| E5(b) | VM↔VFS fdclose 往返 | — | 🔄 宿主半 ✅（79d82ed91）；真机半挂 T2 |
| E5(c) | RS live-update 全链 | — | 🔄 前哨段宿主半 ✅（fbd33bcae）；主体挂 T2（NS1/NS2） |
| E5(d) | QEMU VM paging 冒烟（缺页完整回路+VM 写 PTE） | new_edge1 NK2（转发臂） | ☐（内核转发臂 ✅ 5037491ff + 载体真机 PASS；余 T2 boot 链真机半） |
| E5(e) | PM↔SCHED 调度链 | NS10 + K1/K2 | ☐→前置全清（NS10 ✅ 75aaea831/merge d0ce564cd，2026-09-21；K1/K2 前轮 ✅）→ **宿主半 ✅ 2026-09-21（zcode_glm_1，claim/E5E-zcode_glm_1：代码 04f8c5417/09d3b9014/d5f53da9d）**——三链宿主联调四测试入库（START/INHERIT/NO_QUANTUM 回环/拒绝透传，`os/tests/pm_sched.rs`，E5(a) 判例：PM 真状态机+SCHED 真 run_once 经共享 bus 对跑，内核面以生产渲染线束对账）；顺带修 PM `MinixSchedCtl::taskcall` rv 吞回复 m_type 的真分歧（C taskcall.c:17-20 对位，SCHED 拒绝码此前被读成成功，+2 pin 测试）与 minix-tests E0046 断裂（NS8-B 夹具缺 sys_diagctl_stacktrace，该断裂曾令 clippy 多 crate 会话中途阻断）；pm 415(+2)/sched 98/minix-tests 12 全绿（合并态 sys 300/vfs 523 保持），clippy 净新增 0。**真机半挂 T2**（随 NK1 收口波次）；FIXLOG edge3 #142 |
| E5(f) | DS 发布/订阅三链 | NS1 | 🔄 宿主半 ✅（65bd107a6）；真机半 🔄（claim/E5-DS-ONLINE-qoder）：新门 `os/qemu-tests/test-e5-ds.sh` 已绿——传输链四腿（SYS_SETGRANT/int-33 taskcall/DS 服务环/safecopy 双向）+ 判据1（`E5f-LABELS 12/12`）；同批真修两处 P0（DS 回复 errno 符号、驱动 announce 误用 TYPE_LABEL）并闭 E-RSWIRE 的 DS shadow 半。发布/删除/订阅/notify 四族待「具名服务载体」（RS 拉起的服务），细则见 edge_todo.md E-DSWIRE |
| E5(g) | MIB/sysctl 四链 + rmibtest | NS1 + E-RMIBWIRE 通电 | 🔄 宿主半 ✅ 2026-09-21（zcode_glm_1，claim/E5G-zcode_glm_1）：`os/tests/mib_sysctl.rs` 九测四链——链1 MIB_SYSCTL 往返（clockrate 整读/ENOMEM 部分拷贝+全长回写/oldp=NULL 尺寸探测）+ sysctlbyname 真树逐级 CTL_QUERY 走树；链2 rmibtest 注册契约服务端半（SENDREC 拒/空路径静默）；链3 ERESTART 纯裁决面（check_reply 三分支/locate_slot Fresh·Reuse·ReapThenReuse·Full/dereg）；链4 minix.mib.* 统计对账（fresh objects=0=堆分配账、remotes=槽表一致、走查幂等）。执行中揭出并修 C-55 两处仓内 wire 缺陷（MIB_SYSCTL 调用号 0x600≠0x1800、sysctl_via 正 errno 判定）。**余量如实登记**：授权中继半（grant_magic 路径）与 libsys rmib_register 客户端本地失败面=E-RMIBWIRE 传输余量，真机半挂 T2 |
| E5(h) | devman 生命周期四链 | NS1 + E-DMWIRE | ☐ |
| E5-SMP | fork 后父子并发写 CoW 页（陈旧 TLB 用例，-smp 4） | ~~NK3~~ ✅（2026-09-20）+ X-8 修复 | ☐（余 X-8/NK6） |
| E5-ARCH | 三架构全系统复跑 + 交叉构建矩阵 | T2~T4 + NL5 | ☐ |

---

## §6 OQ 队列（等用户/联合裁决，任何线不得代决）

**携带**（前轮 §6 未决者照旧有效，编号不变）：OQ-3（**✅ 全闭 2026-09-21**：盘上文件面裁决 + /etc 最小内容集获用户认可为 T4 冒烟基线，详下）、OQ-4（已裁决归 14-stage）、其余未决项按前轮 §6。

**新增（2026-09-20 扫描收敛）**：

| 编号 | 议题 | 备选 | 建议 |
|---|---|---|---|
| OQ-N1 | 命令全集口径：328（plan.md:14 实测）vs 247（muse 口径） | 以 plan.md 为权威 / 重测 | 采 plan.md 328，登记分歧即可 **✅ 已裁决（用户，2026-09-21）：采 plan.md:14 实测 328 为全集权威口径**；muse 247 系子集口径差异，登记分歧、不重测 |
| OQ-N2 | E-BOOTMODS 修法：补齐 12 模块装机清单对 C 序 vs 放宽内核 assert/按名查找 | 前者（保 C 对位）/后者 | **✅ 已裁决（用户，2026-09-20）：采「补齐清单」**——boot-shim loader.rs MODULE_NAMES 补成对 C 序真 12 项、缺件 fail-fast、内核 assert 保持严格（放宽会掩盖装配错误，从 HY4 论证）。解锁 new_edge1 NK4 |
| OQ-N3 | console 输出通道选型 | 串口直程（需 port I/O 权限面）/ video-text+mem server / 系统任务中转 | QEMU 目标下串口最直接；需一并裁决 driver 进程的 I/O 特权模型 **✅ 已裁决（用户，2026-09-21，NL1 设计轮 zcode_glm_2 上会）：采方案 B「内核中转 + 串口先行」，两轴分裁**——①**通道**：/dev/console 输出走 minix-sys sys_outb/sys_sdevio wrapper（客户端半待建）→ 内核 SYS_DEVIO（已真，os/kernel/src/syscall_device.rs:432-520 含 CHECK_IO_PORT 门）→ COM1 0x3F8；/dev/console 落串口 = QEMU 载体适配（harness `-display none -serial file:`，os/xtask/src/qemu.rs:92-95，video-text 在验收面物理不可见），诚实标注不冒充 C 形态；video-text backend（console.c:963 `vm_map_phys` 对位）维持登记，待有 display 可见验收面再落。②**特权模型**：采 C 模型——驱动无 IOPL，port I/O 一律经 SYS_DEVIO + 端口白名单（C 对位：rs232.c `my_inb` 即 `sys_inb` 包装 :223-232、syslib.h:217、do_devio.c:38-48 CHECK_IO_PORT+s_io_tab；Rust 表机制在库 os/kernel/src/kpriv.rs:21/:292-293/:537），tty privilege 行加 0x3F8-0x3FF。**明确否决**：串口直程（驱动直持 I/O 特权，原案选项①"直程"半轴——C 驱动从不持 IOPL）；系统任务中转（原案选项③——C 无此通道，自造架构）。解锁 new_edge2 NL1 批B → new_edge3 NS7 |
| OQ-N4 | minix-rt 页供应商通道 | VM_BRK taskcall / mmap 通道 | 循 E-BOOTFRAME 判例设计轮定 **✅ 已裁决（用户，2026-09-21）：采 VM_BRK taskcall 通道**——对位 C libc brk() 直发 VM_PROC_NR（minix-types vm.rs:208 引 C 形），rt 侧 `request_new_break` 纯函数已在库；VM_MMAP 通道（NS5-A 64 车道）留作 exec/栈映射专用、不作堆形态；「维持静态池+上限」否决（命令运行后最先触顶）。解锁 new_edge2 NL3①（实施待领，推进另议） |
| OQ-N5 | 边界守卫是否豁免 `#[cfg(test)]` 代码 | 豁免 / 不豁免（测试改 re-export 面） | 豁免测试代码（守卫意图是生产行为分层）；stty 测试改用 minix_sys re-export **✅ 已裁决（用户，2026-09-21）：豁免并落地守卫脚本**——check-command-boundary.sh 于 grep 前剥除 #[cfg(test)] 门控模块（tools/ 触碰照规则 3 登记 §2 C-60）；现行 4 处 violations 全在 sbin/init **生产代码**（kerninfo 直读 ×2 + DirectTrapTransport ×2），系真实生产违规另行登记修复，不属本豁免范围 |
| OQ-N6 | C-29 修法：load_vm_elf 增 stack_high 参数 vs per-process 根（C 对位） | ①快但共享根段面互撞仍在 ②动 kernel/arch 设计先行 | **✅ 已裁决（用户，2026-09-21，NK1 设计轮 zcode_glm_4 上会）：采方案② per-process 根**——实施形态：`load_vm_elf` 语义不动（VM/bootstrap root 专用，A1 handoff 零波及 = C 内核只建 VM 那根，protect.c:388），新增 `load_process_elf` 每镜像新根原语（根帧出自 VmBootAllocator；`ProcessSegments.phys_root` 槽在位 proc.rs:799-824 = C p_cr3 对位），test-sysboot 载体 tx 槽接入；每镜像同址 `user_sp` 栈值回归 C `kinfo.user_sp` 本义（每地址空间一份布局）。方案①（stack_high 参数）否决：段面互撞与零隔离是结构性缺陷。解锁 NK1 |

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

- `2026-10-15` qoder / todo 循环：**批次二百八十六～二百八十八三笔已合入 `rewrite`**（`0c9681fb5`→`db4c688ba`→`34e0f416c`，均 `claim/*-qoder` 领取、合回后 release 销锁，锁板现为空）。内容：`cut` 列表语法按 C 定形（`, \t` 三分隔符 + 空项折叠、空列表不是错、`5-2` 只选不中、项数不封顶，并拓出「两个 `*p == '-'` 判据不许压平」第五形）；`sort` 键形五句 `errx` 分流（含推翻两处旧登记：`-k 2,1` 在 C 不报错、月名 `M` 在本仓是无源建模）；`sort -l` 按字段长度接线。包级门 `minix-textfilter` 620→621→624 全绿。**跨线告知**：① 批次二百八十六发现权威容器门以 root 执行，`chmod 0444` 类断言在容器内拿到的是相反读数，`sort_face` 那枚已改成按有效 uid 分支两支各钉（原正文保留、追加更正）；② 三笔容器门均在专属工作树/主树留下 root 属主构建产物（最多 44655 个，会反复触发 cargo 失效重编并拖垮 WSL），现已全部 chown 回 1000:1000（含 `os/.cargo-shared`，残留数归零），今后容器命令一律带 `-e CARGO_TARGET_DIR=/tmp/<name>`；③ 包级测试改在容器内跑并加 `--no-fail-fast`（宿主整包测试会把 WSL 打挂；缺 `--no-fail-fast` 时读数被首个失败目标截断）。新登记待办：`vis_face::fold_width_is_read_the_way_atoi_reads_it` 与 `tail_face` 跟随腿为负载敏感型，各在全包并跑时报过一次红、单跑与重跑均绿，需要加超时/隔离或改成不依赖环境的形。
- `2026-10-15` qoder / todo 循环第四笔：**批次二百八十九已合入 `rewrite`（`86e3c0af9`）**，只改 `tests/tail_face.rs`——把跟随门里唯一依赖调度快慢的「正向等待」从拍数判据改成耐心上限（`PATIENCE = ROUND × 20`），静默窗口 `QUIET` 保持 2.5 拍不动（缩短它就是削弱判据）。负控：`PATIENCE` 压成 1ms → 四枚跟随门同时报红（证明它们在等真读数）。容器权威门 `minix-textfilter` 624/0（`--no-fail-fast` + `--target-dir /tmp/…`，残留 root 文件 0）。**跨线两条**：① `vis_face::fold_width_is_read_the_way_atoi_reads_it` 在整包并跑时报过一次红，panic 文本因我自己的 grep 过滤而丢失、未定因，登记保持 open，**复现禁止用手造 CPU 压力**（已把这台机器打崩重启一次，脚本已删）；② 容器门配方统一为 `docker run --rm -v "$PWD:/work" -w /work/os -m 2g minix-ci:1.94 cargo test --target-dir /tmp/<名> --no-fail-fast -j 1 -p <pkg>`——`--target-dir` 走 CLI 比 `-e CARGO_TARGET_DIR` 更明确，`--no-fail-fast` 不可省（缺它时首个失败目标之后所有目标不跑，读数会偏小）。
- `2026-10-15` qoder / todo 循环第五笔：**批次二百九十已合入 `rewrite`（`3f5bd04cf`）**。改掉 `sort` 一处自创次序：并列键默认保持输入序，只有 `-S` 才按整条记录破并列（C 的 `posix_sort` 两面 `sort.c:194-212` + `fields.c:168`，`-u` 在选项环后归零该位 `:256-258`），并因此修好 `sort -u` 面对一叠同键行「全留下」的反面行为；全局 `-r` 下的稳定序倒装由壳里显式带输入序号实现。读数全部拿参照件 `sort_ref` 逐形量，推翻五处旧断言（含 `test_tie_falls_back_to_whole_line` 改名）。容器权威门 `minix-textfilter` **625/0**、workspace 0 error、三架构零错、三枚负控各咬一处、五道收尾全过、锁已清。**新登记（三处，归下一单）**：`sort -n` 把 `+7` 当非数字而 C 视 `+7`/`7`/`007` 同值（旧测的 Less 读数其实来自被删掉的整行回退）；本壳失序句 `sort: disorder:` 与 C 的 `found disorder:`（`msort.c:403`）差一词；本壳 `-c -u` 的 `sort: duplicate:` 在这份 C 里无对应 `errx`，疑为自创。**跨线提醒**：宿主不再跑整包测试（曾把这台机器打崩一次），配方为 `docker run --rm -v "$PWD:/work" -w /work/os -m 2g minix-ci:1.94 cargo test --target-dir /tmp/<名> --no-fail-fast -j 1 -p <pkg>`。
- `2026-10-15` qoder / todo 循环第六笔：**批次二百九十一已合入 `rewrite`（`c9ed3bf58`）**。`sort -n` 的数值键按 C 的 `number()` 归一：跳前导空白、认 `+` 号、吃整数前导零、一个数字也没有的形与真零同键（因此 `-0`＝`+0`），C 不认指数（`1e3` 键为 1）。跑负控时揪出并**删掉一处冗余支**（剥离小数尾零——删掉后 627 枚全绿，说明它不是判据；本树靠小数位逐位补零比较已经达到同值），同时补了一对真正逼出补零比较的形（`1.4`＝`1.40`、`1.4`≠`1.45`）。容器权威门 **627/0**、workspace 0 error、三架构零错、三枚负控全咬住、五道收尾全过、锁已清。**两处自纠进台账**：① 负控脚本第一版锚文本没命中（0 hits）却跑出「全绿」，那是假阴性——负控必须先确认注入成功再看颜色；② 批次二百九十那句 `sort_face` 基线枚数写偏一枚，已在原条目末尾追加更正（原正文不改）。**下一单登记**：`sort -c` 两路文案（`sort: disorder:` 对 C 的 `found disorder:`，`msort.c:403`；`sort: duplicate:` 疑为自创）与选项面余下 `D`/`R`/`T` 三字母。
- `2026-10-15` qoder / todo 循环第七笔：**批次二百九十二已合入 `rewrite`（`0cd153e22`）**。`sort -c` 两路文案按 C 逐字定形：失序 `sort: found disorder: <后一条>`（`msort.c:403`）、`-u` 下键相等 `sort: found non-uniqueness: <后一条>`（`:405-407`），都退 1、不接 usage、只打一行；程序名改走 `argv[0]`（原来写死 `sort`）。同时收严两枚只写 `contains("disorder")` 的旧门——它们正是让自创措辞一路绿灯的原因，并新增一枚含「stderr 只有一行」形状判据与反方向读数的独立门。容器权威门 **628/0**（合回后在容器里复跑同一读数）、三架构零错、三枚负控**确认注入后**全部咬住、五道收尾全过、锁已清。两处过程自纠入台账：负控生成脚本自身语法错导致三次「全绿」其实一次都没注入；`sort_face.rs` 有 5 处长行需断行（其中两处是上一批带进来的、当时那句「零漂移」读数不成立）。**下一单登记**：`sort` 选项面余下 `D`/`R`/`T`（`R` 记录分隔符是真功能）。
- `2026-10-15` qoder / todo 循环第八笔：**批次二百九十三已合入 `rewrite`**（`tools/build-atf-test.sh` 可执行位 + ATF 兼容层 XSI `strerror_r` 返回型）。两件事由同一条判据串起：交接写的那句直呼式 for 循环在 CI 镜像里 0/18（脚本索引模式 100644，同目录其余 shell 皆 100755，历史 18/18 是 `bash` 形式跑的）；补位后暴露出真正的账面不可复现点——`bsd-strerr.h` 把 `__xpg_strerror_r` 声明成 `char *`，`t_strerror.c:110` 的指针→int 在 gcc 13 只是 `-Wint-conversion` 警告、在 CI 镜像 gcc 14 是硬错误，所以「三架构 18/18」在权威镜像上其实只有 17/18。修法：返回型改回 `int`，且该头**先 `#include <string.h>` 再定义转发宏**（否则宏会把系统声明一并改名，与 `int` 冲突）。修后：容器内直呼 x86_64 **18/18**（诊断计数 0）、`atf-host-suite.sh` **36/36 达终态 0 failed/broken**、宿主两条 picolibc 腿 **18/18 + 18/18**（只拷 `.a` 时被 P1-2 防混用门正确挡下）。交接口中的 `bsd-popcount` 项**已被批次二百六十九/二百七十一关闭**（popcount 在 `bsd-strfun.{c,h}`，那对文件从未入库），判据不必再 diff 链接命令。台账 `P-X86-01` 行保持 `partially fixed` 并追加本批读数；②白名单/③门脚本/④⑤出生腿仍开放。
- `2026-10-15` qoder / todo 循环第九笔：**批次二百九十四已合入 `rewrite`**（`05bd39e73`，只动 `tests/join_face.rs`）。给 `join` 新链了一份 BSD 参照件（`tmp/family-diag-ref/join_ref` = `minix3/usr.bin/join/join.c` + `fgetln`/`getprogname` 垫片），把 19 个选项诊断形与我们二进制 A/B：**每一行差异只剩程序名段，句子与退码全同**——规划表 R13 的文案面经这样一次外部对照才敢算坐实。新增一枚表驱动门 `the_option_error_first_lines_match_the_bsd_reference`（8 行，取既有门未钉的 `-2`/`-j` 尾字符/`-o` 四形/`-t` 附值），门注释里写清它的**可用边界**：参照件的 `fgetln` 垫片在读档路径段错误、`usage()` 也段错误（`-1 0` 实测 exit 139），所以只能量「开档前就退出」的诊断首行，usage 那两行仍由既有门按 C 文本钉。容器门 **629/0**、workspace 0 error、一枚负控咬住、五道收尾全过、锁已清。**环境事实**：`minix-ci:1.94` 未装 freestanding Rust target（`E0463: can't find crate for 'core'`），三架构 `--bins` 只能在宿主跑（或容器内先 `rustup target add`）。**下一单登记**：`join.c:obsolete()`（`:547`）三段 legacy 重写（裸 `-a`→`\01`；`-j1`/`-j2`→`-1`/`-2`，`-j3`/`-j12` 落 `errx(1,"illegal option -- %s")`；`-o arg arg` 逐项改写）——是一次 pre-pass，在 `getopt` 前跑、遇 `--` 即停，判据与参照件读数都已备好。
- `2026-10-15` qoder / todo 循环第十笔：**批次二百九十五已合入 `rewrite`**。接上 `join` 的 `obsolete()` 三段 legacy 重写（裸 `-a`→`case '\01'`、`-j1`/`-j2`→`-1`/`-2`、畸形 `-j3`/`-j12` 在 `getopt` 前 `errx(1,"illegal option -- %s",整个 token)`、`-o arg arg` 逐项改写），并修掉两处真 bug：裸 `-a` 之前被当带值选项（会吞掉第一个文件名）、重复 `-o` 覆盖而非追加（C 是 `olistcnt++`，本壳丢前面的列）。容器门 **631/0**、workspace 0 error、四枚负控全咬住、五道收尾全过、锁已清。第三次踩「负控脚本经 heredoc 生成→语法错→四次全绿其实一次没注入」，控制器已改由编辑工具落盘。
- `2026-10-16` qoder / todo 循环第十一笔：**批次二百九十八已合入 `rewrite`（`14682db07`）**。把 `join` 输出面的分隔符槽位按 C 定形：C 的 `outfield()` 第一句是 `if (needsep++) printf("%c", *tabchar)`（`join.c:496-497`），**先占槽再决定内容**——字段号超出该行字段数时取 `-e` 替换串（`:499-501`），字段存在但是空串才 `return` 且槽位留着（`:503-504`）。本壳三处错全改：`push_tok` 不再先判空整个跳过、`twoline` 里缺席的连接字段占槽并按 `-e` 取值、`olist_token` 的 `Which::Join` 用 `fallback`。金样逐字节拿参照件 `join_ref` 现测。**推翻上一单记的一串金样**：`-t,` 对 `a,,c` 两侧参照件给 `a,,c,,c`（不是 `a,,c,a,,c`），顺此查出更深分歧——本壳 `split_fields` 把 `-t` 切出的空段跳掉而 C 把空段当字段，属字段切分层，已作为下一单判据写死，对应测标 `#[ignore]` 占位不删。读数：容器权威门 **632 passed / 0 failed / 1 ignored**（格式化后重跑一次、合回主树后再跑一次同数）、workspace 0 error、三架构 `--bins` 各 0、两枚负控确认注入后咬住、五道收尾全过。**自纠两处**：第一次把三形塞进同一枚测并整体 `#[ignore]`，导致两枚负控「注入成功却全绿」（一形一测的纪律再次生效）；`rustfmt` 首轮三处漂移里有两处（`src/join.rs:238`/`:412`）经 `git show HEAD:` 副本对照确认是批次二百九十五遗留，当时那句「零漂移」读数不成立，本笔一并格式化。**登记不执行**：`src/jot.rs:771` 的 `3.14159` 触发 deny 级 `clippy::approx_constant`，使 `cargo clippy -p minix-textfilter --all-targets` 在基线上就红一处。**环境事实重申**：合成 CPU 压力复现偶发红的手法永久禁用（已把这台机器打崩重启一次），包级测试一律 `docker run --rm -v "$PWD:/work" -w /work/os -m 2g minix-ci:1.94 cargo test --target-dir /tmp/<名> --no-fail-fast -j 1 -p <pkg>`。
- `2026-10-16` qoder / todo 循环第十二笔：**批次二百九十九合入**（`claim/JOIN-SPLIT-FIELDS-qoder` @ 基线 `4df3bfc97`，只动 `os/commands/usr-bin/textfilter/src/join.rs`、`tests/join_face.rs` 与 18 篇两件文档，单域三线 commands/textfilter，无跨界）。要点：`join` 的字段切分按 C 的两个全局量定形——`JoinOptions` 现在存 `tabchars`（分隔符集）与 `spans`（切出的空段算不算字段），`-t` 一次改两量，于是 `-t,` 保留空段、`-t ' '` 不把制表符当分隔符；不配对那行的空字段也按 C 的 `outfield` 占分隔符槽。读数：容器整包门 **644 passed / 0 failed / 0 ignored**（51 个 target，基线 633 个条目），workspace check 0 error，宿主三架构 `--bins` 各 0 error，三枚变异负控分别咬出 11 / 9 / 4 枚失败。**给后续轮与文档线的三条登记**：① `join` 仍有两形与 C 不同——`cmp`（`join.c:405-414`）把「字段号超出这一行的字段数」与「字段存在但是空串」当两种答案，本壳 `key` 两者都给空串；`fieldarg`（`:526-527`）从不接受 `-o 0`（参照件实测报 `malformed -o option field`），本壳把 `0` 当连接字段收下。判据与金样写在 18 篇台账批次二百九十九那条。② `minix-textfilter` 的 `cargo clippy --all-targets` 腿因存量 `jot.rs:771` 的 deny 级 `clippy::approx_constant` 在基线就红，修好之前这条腿不能当门（本轮改跑 `-- -A clippy::approx_constant` 取证：0 error、`join.rs`/`join_face.rs` 零命中）。③ 端到端门 `pr_face` 与 `vis_face` 有竞态：子进程提前退出时写它的标准输入拿到 `BrokenPipe`——`pr_face` 单目标三连跑 1 红 2 绿，`vis_face` 在同一容器里串跑「整包 + workspace check + 三架构 + clippy」那一趟独红，随后纯整包 644/0、两目标三连跑全绿。属测试自身的缺口，已登记未改。包测一律容器内跑，全程无合成 CPU 压力。
- `2026-10-16` qoder / todo 循环第十三笔：**批次三百合入**（`claim/JOT-APPROX-CONSTANT-qoder` @ 基线 `78dd478fb`，只动 `os/commands/usr-bin/textfilter/src/jot.rs` 测试模块里的一枚用例 + 18 篇台账一行，单域三线无跨界）。把 `jot.rs:771` 那个 π 的六位近似字面量换成两个二进制下精确的形（`9.5` 与 `7.9375`），`cargo clippy -p minix-textfilter --all-targets` 这条腿**首次**从基线红转绿（退码 0、0 error、86 条 warning 与改前逐编译单元同数，棘轮不动），后续轮可以直接拿它当门。两条实证纠正了交接里的改法：加下划线无效（clippy 比数值不比写法），`3.1416` 这种舍到四位的形同样被报（它的常量表含舍入短形）。另一条可复用的工具事实：`minix-ci:1.94` 镜像里 `lib/rustlib/` 只有 `x86_64-unknown-linux-gnu`，**没有三架构 none 目标的 core**，容器里跑 `--target x86_64-unknown-none` 会 E0463，所以三架构 `--bins` 只能在宿主现测——这与「权威编译门用容器」不矛盾，容器管宿主测试与 workspace。读数：整包 644/0/0（51 target，与上一笔同数）、`--lib` 452/0、workspace check 0 error、宿主三架构各 Finished。仍开放的本域登记：`jot.rs` 有 10 处**存量** rustfmt 漂移（本笔新增 0 处，未顺手格式化）、`jot` 的 `%5.2f` 不补宽支与负值支未钉、07 篇 jot 行没有测试名/计数声称可对账。负载条款：一条命令只跑一件事、容器 `-j 1 -m 2g -u`、宿主与容器不交叉用同一 target、无合成 CPU 压力。
- `2026-10-16` qoder / todo 循环第十四笔：**批次三百零一合入**（`claim/JOIN-CMP-OVERFIELD-qoder` @ 基线 `5daa464b4`，只动 `os/commands/usr-bin/textfilter/{src/join.rs,tests/join_face.rs}` + 18 篇两件文档，单域三线无跨界）。`join` 的比较层按 C 的 `cmp`（`join.c:405-414`）定形：它有**三种**答案——两侧都越界算相等、只有一侧越界则**越界那一侧算大**、两边都有字段才 `strcmp`。旧写法把「字段号越界」与「字段是空串」都折成空串，除了配对判等错，还让同一档内部的切组（`slurp` 按 `cmp != 0` 收尾，`:398`）把「没有这一列」的行并进「这一列是空串」的行，于是多拼出根本不存在的配对。读数：容器整包 **652 passed / 0 failed / 0 ignored**（51 target，基线 644，本笔 +8 与测数增量相符）、`--lib`/`join_face` 455/25、**`clippy --all-targets` 退码 0**（上一笔刚修好的这条腿第一次当门用，86 条 warning 与基线同数）、workspace check 0 error、宿主三架构各 Finished；两枚变异负控分别咬出 7 枚与 4 枚失败（第二枚只咬方向敏感的那几枚，「空输出」与「切组」两枚对定序没有分辨力——如实记在台账里）；改后与参照件逐形对账 SAME=7 DIFF=0。**join 与本树 C 真源的已知差异只剩一支**：`fieldarg`（`:526-527`）从不接受 `-o 0`（参照件报 `malformed -o option field`），本壳把 `0` 当连接字段收下，`Which::Join` 因此是无生产来源的支路 → 下一单 `JOIN-OZERO-ACCEPTANCE`。负载条款不变。
- `2026-10-16` qoder / todo 循环第十五笔：**批次三百零二合入**（`claim/JOIN-OZERO-ACCEPTANCE-qoder` @ 基线 `77ea92659`，只动 `os/commands/usr-bin/textfilter/{src/join.rs,tests/join_face.rs}` + 18 篇两件文档，单域三线无跨界）。`join` 的 `-o 0` 按 C 关掉：`fieldarg`（`join.c:526-527`）只认 token 首字符 `1`/`2`，「`0` 表示连接字段自己」是 BSD/GNU 的约定、本树真源没有它——参照件对 `-o 0`、`-o 0,1.1`、`-o 1.1,0`、`-o0` 四形实测都报 `malformed -o option field` 退 1，而改前的本壳把四形全部收下并输出。这属「保持外部行为」那一档，不留外来约定：`parse_olist` 去掉 `0` 支，解析器造不出 `Which::Join` 后连同该变体一起删（留着就是无生产来源的死路）。改测四处：拿 `0` 写的三枚库测与拿 `-o0,1.2` 写的那枚解析测是**给自创行为背书**，改成同义的 `1.1` 形、期望串一字未动；另加一枚库测（五形）与四枚门面（一形一枚）。读数：容器整包 **657 / 0 / 0**（51 target，基线 652）、`--lib`/`join_face` 456/29、`clippy --all-targets` 退码 0（86 warning 同基线，棘轮不动）、workspace check 0 error、宿主三架构各 Finished；改后与参照件对账 **SAME=6 DIFF=0**；两枚负控各咬 6 枚与 8 枚（第二枚多倒下的两枚说明「用哪句拒收」另有既有门把守）。至此 `join` 与 C 真源登记的差异只剩一处声明差 `-o 1.-1`（C 侧 `u_long` 回绕反而接受，本树拒之；参照件的读档段在宿主走不到那一步）。负载条款不变。
- `2026-10-16` qoder / todo 循环第十六笔：**批次三百零三合入**（`claim/FACE-GATE-STDIN-RACE-qoder` @ 基线 `66bcd264a`，只动 `os/commands/{usr-bin/textfilter,games/stdio-games}/tests/` 与 18 篇两件文档，**生产码零改动**，单域三线无跨界）。修掉端到端门自身的写标准输入竞态：C 的诊断支排在任何读之前，子进程先退出 → 管道读端关闭 → 父进程 `write_all` 拿到 `BrokenPipe`，而这**是否发生取决于父子谁先被调度**，所以 `pr_face` 三连跑 1 红 2 绿、`vis_face` 串跑多门那一趟独红——是门的问题不是被测代码的问题。改法：新增 `tests/face_support/mod.rs`（两包各一份）把「写标准输入并关闭」收成一件 `write_stdin`，**只放过 `BrokenPipe`**、其余写错仍当场失败（否则门会静默失去输入而假绿）；这个形状此前被复制 16 遍，正是批次二百八十记过的「同一事实多处承载」，一次改齐。另在 `pr_face` 末尾加一枚门自身契约测：`try_wait` 轮询到子进程退出再写，确定性地造出同一状态（不能用 `wait`——std 会先把 `child.stdin` 句柄丢掉）。读数：两包整包 **885 / 0 / 0**（71 target：textfilter 658、stdio-games 227 同基线）；三枚易目标**五连跑全绿**；两包 `clippy --all-targets` 退码 0、warning 86 与 23 **与基线逐编译单元同数**；workspace check 0 error；负控一枚：改回旧形状 → 契约测确定性变红且报的就是当年那句 `Os { code: 32, kind: BrokenPipe }`，其余 14+10+19 枚仍绿。登记：`primes_face.rs:207` 存量 rustfmt 漂移（经 HEAD 副本核对非本笔）、夹具两包各一份（Cargo 集成测无跨 crate 共享机制，单源需先立测试件 crate）、三架构 `--bins` 本轮未跑（改动全在 `tests/`，freestanding 不收集测目标）。过程自纠：负控第一次又用了 heredoc 嵌套引号 → 锚点没匹配上的假注入，已按批次二百九十五的教训改用编辑工具落盘。
- `2026-10-16` qoder / todo 循环第十七笔：**批次三百零四合入**（`claim/SORT-R-OPTION-qoder` @ 基线 `b46ac1baa`，只动 `os/commands/usr-bin/textfilter/{src/sort.rs,src/bin/sort.rs,tests/sort_face.rs}` + 18 篇两件文档，单域三线无跨界）。接上 `sort -R char`（记录分隔符）：C 的 `-R` 那一支判的是**值不是次数**（`sort.c:223-242`：当前 `REC_D` 已不等于 `'\n'` 才报 `multiple record delimiters`，所以 `-R '\12' -R ,` 合法）；`*optarg` 若就是换行则整支提前收尾；值长于一个字节才 `strtol`（反斜缀走八进制、否则 base 0），三条拒收判据（尾字符未吃尽 / 负 / `>= NBINS`）落 `invalid record delimiter %s` 且报文用跳过反斜缀后的串，而 `-R '\'` 是合法的零字节。执行侧按 `REC_D` 切记录（尾随分隔符不留空记录）、输出**以分隔符收尾**（`putrec`）。`-t` 与它不对称：只有 `-t` 查 `d_mask[c] & REC_D_F` 并落 `record/field delimiter clash`，所以 `-R , -t ,` 退 2 而 `-t , -R ,` 通过，且默认下 `-t '\n'` 也报。读数：容器整包 **668 / 0 / 0**（51 target，基线 658，本笔 +10）、`--lib`/`sort_face` 460/21、`clippy --all-targets` 0 error 且 86 warning 与基线同数、workspace 0 error、宿主三架构各 Finished；与参照件 12 形对账 9 全等 + 3 仅差参照件自身程序名段（实际形状差为零）；三枚负控分别咬 2 / 5 / 2 枚，其中「输出腿忽略 REC_D」那枚只咬门面、库测 460 全绿（决定半与执行半分工清楚）。自纠：一版期望按直觉写成 `a,b,\n` 被门挡下（真形 `a\n,b,`）；`ParseError` 因新变体带 `Vec<u8>` 去掉 `Copy`。登记：`d_mask['\n'] = d_mask[' ']` 那半未建模（能探到的形两边已同读数）、`-D`/`-T` 与外部归并引擎仍未做。
- `2026-10-16` qoder / todo 循环第十八笔：**批次三百零五合入**（`claim/FAMILY-GETOPT-LINE-qoder` @ 基线 `6a9daec0a`，只动 `os/commands/usr-bin/textfilter/{src/bin/vis.rs,tests/vis_face.rs}` + 18 篇两件文档，单域三线无跨界）。给 `vis` 的集外字母补上 libc `getopt` 那一行：C 的 vis.c:73 吃 libc getopt 且选项串不以 `:` 开头，所以 `getopt.c:101-104` 先印 `vis: unknown option -- <字母>`，`vis.c:135-140` 才接 usage —— 两行；带值选项缺值是 `:132-136` 的另一句。关键岔子：`-d` **不是**集外字母（C 写的是 `d:`，认它且要带值，只是 `case 'd'` 被 DEBUG 编译掉才落 default），所以本笔把 `-d` 单列一支，否则会把外推的「同族规律」写成真源。句子单源仍在 `GetoptReport`（已服务 head/tail/uniq/cut/join/sort 六支；`pr` 用的是同枚举里的 `IllegalOption`，因为它链 `egetopt`）。测面不增枚数，而是把两处 `contains("Usage: vis [")` 的弱断言换成逐字等式（四支 + `-F`/`-e` 缺值）。读数：整包 **668 / 0 / 0**（51 target，与基线同数——收紧断言不加测）、`vis_face` 19/0、clippy 0 error 且 `bin/vis.rs` 零命中、workspace 0 error、宿主三架构 `--bin vis` 各 Finished、rustfmt 零漂移；两枚负控各咬 1 枚。**同族判定表已交下一步**：待接十四支（nl/cksum/fold/rev/split/unexpand/col/colrm/column/comm/cmp/jot/unifdef/units/seq/paste，其中 nl 与 cksum 连句子本身都可疑），另判五支（pr 已对、expand/hexdump/colcrt/tr/lam 各用自己的解析器）——每支单独一轮，不许按同族规律外推。工具坑：门报 `NotFound` 而二进制明明在，先 `strings` 测试二进制看 `CARGO_BIN_EXE_*` 烘进去的路径（本笔被一棵旧树布局的烘路径坑了一次，`touch` 测试源重编即解）。
- `2026-10-16` qoder / todo 循环第十九笔：**批次三百零六合入**（`claim/NL-OPTION-FACE-qoder` @ 基线 `9b4fbd498`，只动 `os/commands/usr-bin/textfilter/{src/bin/nl.rs,tests/nl_face.rs}` + 18 篇台账一行，单域三线无跨界）。判定表第二支 `nl`：选项面按 C 重建——`nl.c:149` 吃 libc `getopt`（选项串不以 `:` 开头），所以集外字母与带值缺值都是「libc 一行 + 两行 usage」，而命令自己的 `errx` 只有一行不接 usage；旧写法按整 token 匹配字母（`-ba`、`-pb t` 会掉进集外字母）、把 getopt 的错当值非法报、句子多数自创。C 的句子逐句搬回：`illegal body/footer/header line numbering type -- <值>`（段名来自 `numbering_properties[]` 的次序 footer/body/header）、`illegal format -- <值>`、`invalid width value -- x` 与 `width argument must be > 0 -- 0` 两句、`-i/-l/-v` 各一句带值，且 C 的 `-i` **没有**非零检查（旧那句是自创）。数值面按 `strtol`/`strtoul` 的接受集复刻（跳空白、吃符号、末尾剩东西即拒、溢出即拒）。**新增第一扇 nl 端到端门（10 枚）立刻暴露两处真缺陷**：`nl` 自带只调裸 `read(0)` 的 `read_stdin()`，宿主读不到标准输入（改用 `bin_support::read_stdin()` 双生缝）；操作数读不到时打自创短语，而 C 是 `err(EXIT_FAILURE, "Cannot open `%s'")` 三段形（改走 `warn_line`）。读数：整包 **678 / 0 / 0**（52 target，基线 668/51）、`clippy --all-targets` 0 error 且 warning 86 与基线同数、workspace 0 error、**宿主三架构各 Finished——它抓到 no_std 下 `format!` 未 `use alloc::format` 的交叉编译错，宿主整包测与 clippy 全绿也发现不了**；三枚负控各咬一枚（集外字母行 / `-w` 两句 / `-i 0` 接受）；射程边界如实登记：`-b p<正则>` 需 `regcomp` 未接（拒收文案保持自描述，不冒充 C）、计数器是 `u64` 而 C 是 `int` → `NL-SIGNED-COUNTERS`。判定表还剩 cksum（句子也可疑）/fold/rev/split/unexpand/col/colrm/column/comm/cmp/jot/unifdef/units/seq/paste，另判 expand/hexdump/colcrt/tr/lam。
- `2026-10-16` qoder / todo 循环第二十笔：**批次三百零七合入**（`claim/CKSUM-GETOPT-LINE-qoder` @ 基线 `5e3c12907`，只动 `os/commands/usr-bin/textfilter/{src/cksum.rs,src/bin/cksum.rs,tests/cksum_face.rs}` + 18 篇两件文档，单域三线无跨界）。判定表第三支 `cksum`：它今天的句子 `cksum: illegal option -- Z` 是从 `pr` 的 `egetopt` 措辞抄来的，而 `cksum.c:193` 吃 libc `getopt` → 正确句子是 `unknown option`。一次改齐四类出口：getopt 的两支（集外字母 / 带值缺值）与命令自己的两支 `warnx`（`illegal argument to -a|-o option`、`requirehash` 的 `<旗标> flag requires \`-a algorithm'`）都接**同一块九行 usage**、退 1（旧写法 usage 只有一行、且一律退 2、句子多为自创）。扫描器改成 getopt 形状（聚簇 `-nq`、附值 `-o2`/`-acrc`、`--`、裸 `-` 是操作数）。测面：库测 +1、新增 `tests/cksum_face.rs`（5 枚，本包第 53 扇目标）。读数：整包 **684/0/0**（53 target，基线 678/52）、clippy 0 error 且 86 warning 同基线、workspace 0 error、宿主三架构各 Finished、rustfmt 零漂移；三枚负控各咬 1/1/2 枚。登记四条：文件腿的 strerror 形状（`CKSUM-FILE-LEGS`）、`-a`/`-o` 与已选 hash 的两句互斥判定（本树不可达，`CKSUM-HASH-STATE`）、多入口名靠 `argv[0]` 选算法未建模、07 篇第 155 行的陈旧测试计数 229 → 现值 684（归 V3-5/V3-6 的口径轮，不顺手改）。
- `2026-10-16` qoder / todo 循环第二十一笔：**批次三百零八合入**（`claim/FOLD-GETOPT-LINE-qoder` @ 基线 `c6c6d5cd6`，只动 `os/commands/usr-bin/textfilter/{src/fold.rs,src/bin/fold.rs,tests/fold_face.rs}` + 18 篇两件文档，单域三线无跨界）。判定表第四支 `fold`：C 的选项串 0123456789bsw: 里只有 w 带值、0-9 是合法字母、不以 `:` 开头故 opterr 生效 → 三类出口**行数不同**（getopt 两支 = libc 一行 + usage 一行；宽度非正 = 命令自己的 errx 一行）。旧写法压成一类（只打 usage、丢 libc 首行），usage 文本自创且丢了 `[file ...]`，聚簇里的数字被当集外字母；`FoldError::Usage` 拆成 `UnknownOption`/`MissingValue`。**新立的第一扇 fold 门当场暴露读缝缺陷**：`fold_stream` 直接拿裸 open/read 的 fd 自己读，宿主读不到任何输入（与批次三百零六给 nl 收的那处同形），已改走 bin_support 的 双生读缝；操作数读不到的报文也补回 err(3) 的三段形（此前缺 strerror 句子）。读数：整包 **690/0/0**（54 target，基线 684/53，+6 全是新门）、clippy 0 error 且 86 warning 同基线（新门里一处 useless_conversion 被门抓到、已修）、workspace 0 error、宿主三架构各 Finished、rustfmt 零漂移；三枚负控各咬 1/1/2 枚。登记：遗留 `-N` 与聚簇数字两形不照 C 复刻（C 取 argv[optind-1]，结果跟着 getopt 内部位置走；`-0` 在 C 里宽度 0 照常跑），要收得先链出带 Minix3 自带 getopt 的 fold_ref（`FOLD-LEGACY-WIDTH`）；真机 open 腿归 C-57/E5。
- `2026-10-16` qoder / todo 循环第二十二笔：**批次三百零九合入**（`claim/REV-GETOPT-LINE-qoder` @ 基线 `ce8536efb`，只动 `os/commands/usr-bin/textfilter/{src/rev.rs,src/bin/rev.rs,tests/rev_face.rs}` + 18 篇两件文档，单域三线无跨界）。判定表第五支 `rev`：C 的 optstring 是**空串**（`rev.c:70`），这一条不等于「没有选项环」——本壳当时把 `argv[1..]` 整个当操作数，`-Z` 因此变成要打不开的文件。三形逐形对参照件（`rev.c` 真源 + Minix 的 `getopt.c` + `fgetwln`/`setprogname` 垫片在宿主现链）：集外字母 = libc 一行 + `usage()` 一行退 1；`--` 结束选项环且自身不算操作数；裸 `-` 反而落回操作数（不推进 `optind`）；不重排。另两处与本批同形的收获：选项扫描写成 `while` 被 `clippy::never_loop` 挡住（那个圈确实定不了第二次，已改单次判定）；读缝用裸 `open`/`read` 在宿主读不到输入（与 `nl`/`fold` 同病），改走 `bin_support` 双生面并把操作数报文补回 err(3) 三段形。读数：整包 **699/0/0**（55 target，基线 690/54）、clippy 0 error 且 86 warning 同基线、workspace 0 error、宿主三架构 `--bins` 各 Finished、rustfmt 零漂移、A/B **SAME=16 DIFF=0**；三枚负控各咬 3/4/1 枚。登记：`read_stdin()` 无错误通道所以 stdin 读出错的 `warn("stdin")` 那一形不可观测（`REV-STDIN-ERRLEG`）、集外字母在 C 打一个字节而本壳按 `char` 取（`REV-NONASCII-OPTLETTER`）、真机 open 腿归 C-57/E5、07 篇第 155 行陈旧测试计数 229（现值 699，归 V3-5/V3-6 口径轮）。

- `2026-10-16` qoder / todo 循环第二十三笔：**批次三百一十合入**（`claim/COLRM-GETOPT-LINE-qoder` @ 基线 `58ff62978`，只动 `os/commands/usr-bin/textfilter/{src/lib.rs,src/rev.rs,src/colrm.rs,src/bin/colrm.rs,tests/colrm_face.rs}` + 18 篇两件文档，单域三线无跨界）。给 `colrm` 补回 C 的选项环并把列号段按 `strtol` 定形：`colrm.c:67` 的 optstring 是空串，与批次三百零九的 `rev.c:70` 逐字同形，于是把这一环从 `rev.rs` 提为共件 `minix_textfilter::empty_optstring_operands`（句子仍单源 `GetoptReport`），`rev` 侧改一行委托、行为不变。`colrm` 的操作数不是文件名而是列号，所以同一个裸 `-` 在两条命令里落到完全不同的句子。两处 C 性质由参照件现量而不靠同族规律：两支列号都坏时报**后写**的那支（C 的 `case 2:` 先算 `argv[1]` 再 FALLTHROUGH），以及负数经 `u_long` 回绕后**不被拒**（`colrm -- -5` 退 0 且整行透传）。第四处「薄壳裸 `read` 宿主读不到输入」的实例（`nl`/`fold`/`rev` 之后）：已改走 `bin_support::read_stdin()`，顺带修掉同一腿上的 UTF-8 校验把非文本字节整段丢光的失真。读数：整包 **713 / 0 / 0**（56 target，新立 `colrm_face` 一枚门测 12 枚）、clippy 0 error 且 86 warning 同基线、workspace 0 error、宿主三架构 `--bins` 各 Finished、rustfmt 触及件零漂移；四枚变异负控各咬 3 / 4 / 6 / 4 枚；A/B 与参照件 `tmp/colrm-310/colrm_ref` 逐形 30 形 **SAME=29 DIFF=1**，唯一之差是 `strerror(0)` 的句子（本机 glibc 印 `Success`，Minix 表 `minix3/lib/libc/compat/gen/compat_errlist.c:53` 印 `Undefined error: 0`，本壳跟 Minix 那张表）。**判定表已翻正一处分类错并交下一步**：`tr` 原列「另判」，现查 `minix3/usr.bin/tr/tr.c:68` 吃 libc `getopt("cds")`，改列「待接」；已对齐 12 支 + pr，待接 `split`/`unexpand`/`col`/`column`/`comm`/`cmp`/`jot`/`unifdef`/`units`/`seq`/`paste`/`tr`，另判 `expand`/`hexdump`/`colcrt`/`lam`。下一轮首动作 `tools/claim.sh claim SPLIT-GETOPT-LINE qoder`（`split.c:80` 的 optstring 含 `0-9` 六个合法字母与四个带值项，出口比 `colrm` 多一层）。新登记不执行：`COLRM-IO-ERRLEG`（C 的 `check()` 两形在本壳不可观测）、`COLRM-STREAMING-MEMORY`、`COLRM-NONASCII-OPTLETTER`、`COLRM-ULONG-WIDTH`。

- `2026-10-16` qoder / todo 循环第二十四笔：**批次三百一十一合入**（`claim/SPLIT-GETOPT-LINE-qoder` @ 基线 `589fc4ce0`，只动 `os/commands/usr-bin/textfilter/{src/split.rs,src/bin/split.rs,tests/split_face.rs}` + 18 篇两件文档，单域三线无跨界）。给 `split` 的诊断面按 C 分回四类出口：`split.c:80` 的 optstring 是 `"0123456789b:l:a:n:"`，`0-9` 六个是合法字母（NetBSD 自称 Undocumented kludge）加四个带值项，所以集外字母与缺值各由 libc 先印一行（`getopt.c:101-104`、`:132-136`）再由 `default: usage()` 接第二行；值非法是 `errx` 两支形；`-l 3 -l 4` 与多于两个操作数才是纯 usage。本壳改前五项失真：两类 libc 行都缺、`usage()` 句子自拟成 `usage: split [-l lines]` 且**退码写成 2**（C 是 `exit(1)` 与四选项式样、程序名跟着 `getprogname()`）、选项环**重排**（`split <file> -Z` 的 `-Z` 在 C 是输出前缀而本壳当坏选项）、遗留形只认全数字尾巴（`-12x` 在 C 报 `split: 12x: illegal line count.`）、后缀穷尽与 `-n` 超限两句自拟（C 是 `too many files.` 与 `can't split into more than %d files`，后者缀输入字节数）。**新门暴露的是写腿（同族第五例，方向相反）**：`split` 壳拿裸 `minix_sys::open`/`write`/`close` 建分片，宿主上一个文件都写不出去（门测首轮五枚红），已改走 `bin_support::create_sink` 双生面，并把开档失败与写失败按 C 分成两句（`err(1, "%s", fname)` 与 `err(1, "write")`）。读数：整包 **725 / 0 / 0**（57 target，新立 `split_face` 12 枚）、clippy 0 error 且 86 warning 同基线、workspace 0 error、宿主三架构各 Finished、rustfmt 触及件零漂移；A/B 与参照件 `tmp/split-311/split_ref` 比退码/报文/生成文件名三件，31 形 **SAME=31 DIFF=0**；四枚变异负控各咬 3 / 2 / 2 / 5 枚。test-audit 第 2 维当场逮住三处**测试自身**写错（三个操作数该走 usage、十二行按五行切是三片不是两片、`split -l 3 pp` 的 `pp` 是第一操作数即输入档），均按 C 改期望不改代码。判定表进度：已对齐 13 支 + pr，待接 `unexpand`/`col`/`column`/`comm`/`cmp`/`jot`/`unifdef`/`units`/`seq`/`paste`/`tr`，另判 `expand`/`hexdump`/`colcrt`/`lam`。下一轮首动作 `tools/claim.sh claim UNEXPAND-GETOPT-LINE qoder`（`unexpand.c:86` 的 optstring 是 `"at:"`，只有 `t` 带值）。新登记不执行：`SPLIT-READ-STAT-ERRLEG`、`SPLIT-CLOSE-ERRLEG`、`SPLIT-WRITE-ERRNO`、`SPLIT-NONUTF8-DROP`（输入过 UTF-8 校验、非文本字节整段丢光，与批次三百一十在 colrm 修掉同形，但需连行重建一并改，单独一轮）、`SPLIT-STREAMING-MEMORY`。

- `2026-10-16` qoder / todo 循环第二十五笔：**批次三百一十二合入**（`claim/UNEXPAND-GETOPT-LINE-qoder` @ 基线 `6302808e7`，只动 `os/commands/usr-bin/textfilter/{src/unexpand.rs,src/bin/unexpand.rs,tests/unexpand_face.rs}` + 18 篇两件文档，单域三线无跨界）。给 `unexpand` 的选项环与 `-t` 档位表按 C 定形：`unexpand.c:86` 的 optstring 是 `"at:"`（无值字母 `a` + 带值字母 `t`，两支互斥），四类出口各有自己的形制——集外字母与 `-t` 缺值各由 libc 先印一行（两句措辞不同）再由 `default: usage()` 接第二行；`-a`/`-t` 冲突只印 usage 一行；坏档位有 `Invalid tabstop`（尾巴字符/溢出）与 `Bad tabstop spec …, must be greater than the previous …`（不严格递增）两支不同句子。本壳改前十处失真：两类 libc 行都缺、usage 句子写成小写自拟短式且少了 `[file ...]`（C 那一行是 `Usage: <getprogname()> [-a] [-t tabstop] [file ...]`）、档位回显整段缺失、`strtoul` 的 base 0 不收（`010` 该是 8、`0x10` 该是 16）、三种分隔符与「空字段跳过」不收、两句 errx 被抹平成一句、选项环重排、`Cannot open` 少了 `strerror` 那一段、末行无换行时补一个换行。**量形量到的最有价值一条**：真源在选项环之后把档位表逐支打到 stderr（`unexpand.c:120-121` 的 `fprintf(stderr, "%lu %zu\n", …)`，显然是漏删的调试输出），但它就在外部行为里，本壳照搬；这类形制只能链真源量，看代码猜会得到「谁会把档位打到 stderr」。第六处同族读写缝缺陷（`nl`/`fold`/`rev`/`colrm` 的读腿、`split` 的写腿之后）：`unexpand` 壳拿裸 `open`/`read`，宿主读不到任何输入，已改走 `bin_support::read_stdin()`/`read_file()`，顺带说明裸 `-` 在 `unexpand` 这里不是 stdin 的别名（C 直接送去 `freopen`）。test-audit 第 2 维又逮住一处「期望写错而代码对」：`-ta` 不是两支冲突，C 的 getopt 一见 `t:` 就把同串剩下的 `a` 当 optarg，走 `Invalid tabstop \`a'`（参照件同句），改期望不改代码。读数：整包 **736 / 0 / 0**（58 target，新立 `unexpand_face` 11 枚）、clippy 0 error 且 **86 warning 同基线**（本笔一度推到 92，四处已改：可折叠 `if let`、两支 `assert_eq!` 比字面 bool、输出腿双重否定布尔式）、workspace 0 error、宿主三架构各 Finished、rustfmt 触及件零漂移；A/B 与参照件 `tmp/unexpand-312/unexpand_ref`（含自写的 `fgetln` 垫片——glibc 没有这一支）比退码/stdout/stderr 三件，29 形 **SAME=29 DIFF=0**；四枚变异负控各咬 3 / 3 / 2 / 2 枚。判定表进度：已对齐 14 支 + pr；待接 `col`/`column`/`comm`/`cmp`/`jot`/`unifdef`/`units`/`seq`/`paste`/`tr`；另判 `expand`/`hexdump`/`colcrt`/`lam`。下一轮首动作 `tools/claim.sh claim COL-GETOPT-LINE qoder`（`col.c:133` 的 optstring 是 `"bfhl:px"`，只有 `l` 带值）。新登记不执行：`UNEXPAND-SATURATING-DEC`（存量告警，C 的守卫式自减如实搬运）、`UNEXPAND-WRITE-ERRLEG`、`UNEXPAND-LOSSY-BYTES`、`UNEXPAND-ULONG-WRAP`。

- `2026-10-16` qoder / todo 循环第二十六笔：**批次三百一十三合入**（`claim/COL-GETOPT-LINE-qoder` @ 基线 `6802a77e7`，只动 `os/commands/usr-bin/textfilter/{src/col.rs,src/bin/col.rs,tests/col_face.rs}` + 18 篇两件文档，单域三线无跨界）。给 `col` 的选项环按 C 分回四类出口：`col.c:133` 的 optstring 是 `"bfhl:px"`（五个无值字母 + 一个带值的 `l`，且 `col` 一个操作数都不接受，`optind != argc` 直接 usage）。最有价值的一处纠正：`-l` 后面没有那一段，与 `atoi(optarg) <= 0`，在 C 是**两条完全不同的路径**——前者由 libc 印 `col: option requires an argument -- l` 再接 usage 两行，后者是裸 `fprintf(stderr, "col: bad -l argument %s.\n", optarg)` 加 `exit`（程序名写死、带操作数原文、带句点、不接 usage），而本壳把两支并成了一支且句子还丢了原文。`atoi` 的两处接受面照搬：值可附在本串（`-l3`）、尾巴有杂字符不管（`-l 3x` 是合法的 3）。第七处同族读写缝缺陷：`col` 壳拿裸 `minix_sys::read` 读标准输入，宿主读不到任何东西，那台行装配状态机（半行进位、退格叠印、ESC 半整行、字符集切换）从来没有被进程级测过，已改走 `bin_support::read_stdin()`。读数：整包 **746 / 0 / 0**（59 target，新立 `col_face` 9 枚 + 库测 1 枚）、clippy 0 error 且 **86 warning 同基线**（本笔一度到 87，一枚是测试里与 `false` 比相等，已改取反）、workspace 0 error、宿主三架构各 Finished、rustfmt 触及件零漂移；A/B 与参照件 `tmp/col-313/col_ref` 比退码/stdout/stderr 三件 29 形，再比五段输入的输出指纹，合计 **SAME=34 DIFF=0**；四枚变异负控各咬 3 / 2 / 2 / 4 枚。判定表进度：已对齐 15 支 + pr；待接 `column`/`comm`/`cmp`/`jot`/`unifdef`/`units`/`seq`/`paste`/`tr`；另判 `expand`/`hexdump`/`colcrt`/`lam`。下一轮首动作 `tools/claim.sh claim COLUMN-GETOPT-LINE qoder`（`column.c:93` 的 optstring 是 `"c:s:tx"`，`c`/`s` 带值）。新登记不执行：`COL-WARN-KIND`（C 的 `dowarn` 按 `line < 0` 分两句 `past first line` 与 `-- line already flushed`，本壳的 `warned` 是布尔只印后者）、`COL-WRITE-ERRLEG`（`wrerr` 的 `col: write error.` 不可观测）、`COL-STREAMING-MEMORY`、`COL-WARN-FACE-CASE`（本次输入没触发告警那条腿，仍只有库测）。

- `2026-10-16` qoder / todo 循环第二十七笔：**批次三百一十四合入**（`claim/COLUMN-GETOPT-LINE-qoder` @ 基线 `b8dafd773`，只动 `os/commands/usr-bin/textfilter/{src/lib.rs,src/col.rs,src/column.rs,src/bin/column.rs,tests/column_face.rs}` + 18 篇两件文档，单域三线无跨界）。给 `column` 的诊断面按 C 分回四类出口：`column.c:93` 的 optstring 是 `"c:s:tx"`（两个带值字母 + 两个旗标），而它的操数是**文件清单**（`:112-123`），所以四类出口各有形制——集外字母与 `-c`/`-s` 缺值各由 libc 先印一行（两句措辞不同）再由 `default: usage()` 接第二行；usage 那一行是**大写 `Usage`、程序名跟着 `getprogname()`、末尾带 `[file ...]`**（`:302-307`）；操数段是三段形 `warn(3)` 加置 `eval` 加**继续**。本壳改前六处失真：两类 libc 行都缺、usage 写成小写自拟短式并丢了 `[file ...]`、`Cannot open` 少了 `strerror` 那一段、选项环重排（`column 档 -t` 的 `-t` 在 C 是档名而本壳当旗标吃掉）、`-c` 的值按严格 `>0` 解析（C 是 `atoi` 无守卫：`0`、`abc`、`-5`、空串都退 0 且每条一行，这翻正了批次一百七十五登记的那处差异）、`warnx(3)` 的 `line too long` 整句缺失。**`atoi` 从 `col.rs` 提到 crate 根**由 `col`/`column` 共用（同 `GetoptReport` 的单源处置）。**新门第一次跑通布局，当场抓到两处 P0-code-bug**：① `c_columnate`/`r_columnate` 把条目、制表符、换行当三段各自返回，壳侧逐段补换行，于是两条布局从来没打出过一行像样的输出（五条短行会变成 `alpha\n\t\nbeta\n\t\n\n\n`）——改成与 `table_mode` 同形各交整行，并照 C 补两处次序（`if (!--entries) break` 与 `if ((base += numrows) >= entries) break` 都排在补齐之前，所以行尾不挂制表符）；② 条目是**整行**，C 存行首的 `buf` 而不是跳过前导空白后的 `p`（`:287`），`len = p - buf` 也从行首量（`:285`），旧写法与库文档都写着「剥掉前导空白」——参照件现量 `  lead` 后面跟着三个连续制表符（其中一个是条目自带的）把这条钉死；③ 自审抓到一枚**运行期 panic**：聚簇游标是**字节**下标，非 ASCII 字母时它落进多字节字符中间，`&arg[pos..]` 不在字符边界上当场 panic（`column -té` 即触发，参照件打的是一个裸字节 0xC3 后退 1），改成 `char::from(bytes[pos])` 按字节取并钉一枚库测——**同一枚 panic 住在批次三百一十三交回的 `parse_col_flags` 里**（`col -bé`），按一次一条的规矩登记为 `COL-NONASCII-PANIC` 交下一支顺手收，不在这一笔里改另一条命令的码。读数：整包 **762 / 0 / 0**（60 target，基线 746/59，+12 新门 +4 库测）、clippy 0 error 且 **85 warning**（基线 86，少的那一枚是旧 `parse_column_args` 里 `pos = arg.len()` 的死赋值，随重写消失，零新增）、workspace 0 error、宿主三架构各 Finished、rustfmt 触及的叶子件零漂移（`lib.rs` 余下三处 `pub mod` 顺序漂移是既有登记，本笔不动它）；A/B 与参照件 `tmp/column-314/column_ref`（`column.c` 真源 + Minix `getopt.c` + 自写 `fgetln`/`ecalloc`/`estrndup`/`erealloc`/`getprogname` 垫片）比退码/stdout/stderr 三件，42 形 **SAME=42 DIFF=0**；四枚变异负控各咬 **2 / 2 / 2 / 3** 枚。判定表进度：已对齐 16 支 + pr；待接 `comm`/`cmp`/`jot`/`unifdef`/`units`/`seq`/`paste`/`tr`；另判 `expand`/`hexdump`/`colcrt`/`lam`。新登记不执行：`COLUMN-ISSPACE-SET`（跳空白与判空行用 `isspace` 全六类，本层只认空格与制表符，故 `  \r` 那形两边不同）、`COLUMN-NUL-TRUNCATES-LINE`（C 的 `strchr` 在行内 NUL 处停下，带 NUL 的行按「长行」丢掉并警告）、`COLUMN-TERMINAL-WIDTH`（`ioctl(TIOCGWINSZ)` 与 `COLUMNS`，`:86-90`，属终端面）、`COLUMN-NONUTF8-DROP`（同族既有登记）、`COLUMN-NONASCII-OPTLETTER`（C 的 `%c` 打一个裸字节，本层按 `char` 渲染成两个 UTF-8 字节，句子其余部分与退码同）、`COL-NONASCII-PANIC`（上一笔交回的 `col` 同形游标，待下一支收）

- `2026-10-16` qoder / todo 循环第二十八笔：**批次三百一十五合入**（`claim/COMM-GETOPT-LINE-qoder` @ 基线 `ed4db08d6`，只动 `os/commands/usr-bin/textfilter/{src/comm.rs,src/bin/comm.rs,tests/comm_face.rs}` + 18 篇两件文档，单域三线无跨界）。给 `comm` 的诊断面按 C 分回两类出口：`comm.c:80` 的 optstring 是 `"123f"`，四个字母**都不带值**，所以 libc 那句 `option requires an argument` 在本支根本到不了——可达出口由 optstring 定，不按「同族都有缺值一支」的规律补；集外字母是 libc 一行（聚簇 `-1Z` 报 `Z`、`--x` 报 `-`）后接 usage 一行、退 1，而操作数不是两个**只有 usage 一行**（`comm.c:101`）。同一支命令里两行的程序名来历不同：libc 那行跟着 `getprogname()`，usage 那行把 `comm` 写死在串里（`:197`）。选项环**不重排**（`getopt.c:74-78`），所以 `comm 档 -1 档` 是三个操作数而不是两操作数加旗标；裸 `-` 是合法操作数即标准输入、`--` 结束环而自身不是操作数；打不开的档名是 `err(1, "%s", name)` 三段形且**致命**（第一个失败第二个连试都不试，与 `column`/`rev` 的 warn+继续相反）；末行没有换行符**照样是一条记录**（`getnextln` 只在 EOF 且一字符未读时返回 `NULL`），与批次三百一十四的 `column` 恰好相反。第八处同族读写缝缺陷：`comm` 壳拿裸 `open`/`read`/`close` 读操作数档，宿主一个档名都读不到，三列合并的进程级行为从没被测过，已改走 `bin_support` 双生面。**新门当场抓到列前缀发错的实缺陷**：C 把 `tabs[]` **依次**发给真正打出来的列（`:105-110` 的 `p = tabs; if (flag1) col1 = *p++; …`），旧写法固定按列号取 `tabs[column-1]`，于是 `comm -1` 把公共行顶成两个制表符、`-2` 把列三顶成两个。读数：整包 **772 / 0 / 0**（61 target，基线 762/60，+9 新门 +1 库测）、clippy 0 error 且 **85 warning 同基线**（本笔一度 86，一枚是新门里绑了没用的操作数路径，已改成正面那条断言）、workspace 0 error、宿主三架构各 Finished、rustfmt 触及件零漂移；A/B 与参照件 `tmp/comm-315/comm_ref`（`comm.c` 真源 + Minix `getopt.c` + `getprogname` 垫片）比退码/stdout/stderr 三件，32 形 **SAME=32 DIFF=0**；四枚变异负控各咬 **1 / 1 / 3 / 3** 枚。test-audit 第 2 维又逮到一处「期望写错而代码对」：`comm - 档` 剩下的行属于**第二边**、落列二（参照件 `\t\tapple`/`\t\tbanana`/`\tcherry`），按参照件改期望不改代码。判定表进度：已对齐 17 支 + pr；待接 `cmp`/`jot`/`unifdef`/`units`/`seq`/`paste`/`tr`；另判 `expand`/`hexdump`/`colcrt`/`lam`。下一轮首动作 `tools/claim.sh claim CMP-GETOPT-LINE qoder`（`cmp.c:74` 的 optstring 是 `"ls"`，都不带值）。新登记不执行：`COMM-SHARED-STDIN-CURSOR`（两个 `-` 共用一条流时 C 是交错读，本层「先收全再合并」没有游标可言）、`COMM-DIR-OPERAND`（C 的 `fopen` 对目录成功、`getc` 才失败，故退 0 而本层退 1）、`COMM-LINELEN-TRUNCATION`、`COMM-LOCALE-COLLATE`（`strcoll` 随 locale，本层按字节序）、`COMM-WRITE-ERRLEG`（`err(1, "stdout")` 不可观测）。

- `2026-10-16` qoder / todo 循环第二十九笔：**批次三百一十六合入**（`claim/CMP-GETOPT-LINE-qoder` @ 基线 `005e6a3a2`，只动 `os/commands/usr-bin/textfilter/{src/cmp.rs,src/bin/cmp.rs,tests/cmp_face.rs}` + 18 篇两件文档，单域三线无跨界）。给 `cmp` 的诊断面按 C 分回三类出口：`cmp.c:74` 的 optstring 是 `"ls"`（两个字母**都不带值**，所以 libc 的 `option requires an argument` 在本支到不了——可达出口由 optstring 定，不按同族规律补），三支**行数不同**：集外字母 = libc 一行 + usage 一行、`-l`/`-s` 同时给（含聚簇 `-ls`/`-sl`）= 一句 `errx` **不接 usage**、操作数不是 2..4 个 = 只有 usage 一行，三支都退 `ERR_EXIT` = 2（本支是 0/1/2 三档退出码）。前门按整个 token 比字母，聚簇形因此落进「未知选项」并打 usage：一行 `errx` 被换成两行、句子也没了。两处只能量出来推不出来的形制：开档失败那一句是 `if (!sflag) warn("%s", name)`——**`-s` 之下整句不打**只留退出码（旧写法无条件打且缺 `strerror` 一段）；环不重排之后 `cmp 档1 -l 档2` 是三个操作数、`-l` 是第二个档名（参照件 `cmp: -l: No such file or directory`）。选项环提入库侧 `parse_cmp_flags`（判定住库、句子交回壳），互斥与个数两判留在壳里按 C 的次序走。**既有的 `cmp_face` 门整扇重写**（10 枚 → 11 枚，target 数不变）：换掉的不是门而是**参照系**——旧门拿宿主 GNU `cmp` 只比退码与「这一路是否为空」，现在换成链真源的 Minix 参照件逐字比报文与行数，旧门那两条既有断言（`-` 侧报表写 `stdin`、开档失败排在坏 skip 之前）搬进新的第 11 枚。读数：整包 **775 / 0 / 0**（61 target）、clippy 0 error 且 **85 warning 同基线**、workspace 0 error、宿主三架构各 Finished、rustfmt 触及件零漂移；A/B 与参照件 `tmp/cmp-316/cmp_ref`（`cmp.c` + `misc.c` + `regular.c` + `special.c` 真源 + Minix `getopt.c` + 程序名垫片）比退码/stdout/stderr 三件，38 形 **SAME=38 DIFF=0**；四枚变异负控各咬 **1 / 1 / 3 / 1** 枚。判定表进度：已对齐 18 支 + pr；待接 `jot`/`unifdef`/`units`/`seq`/`paste`/`tr`；另判 `expand`/`hexdump`/`colcrt`/`lam`。下一轮首动作 `tools/claim.sh claim JOT-GETOPT-LINE qoder`（`jot.c:150` 的 optstring 是 `"b:cnp:rs:w:"`，`cn` 两个字母不带值，易误读成 `c:`+`n`）。新登记不执行：`CMP-SPECIAL-PATH`（C 的 `fstat` 两支与 `special`/`regular` 双路径在本层合成一条，`err(ERR_EXIT, "%s", file)` 那一形不可达）、`CMP-DIR-OPERAND`（目录在 C 是 `open` 成功后 `read` 才 `EISDIR`、本层报成开档失败，与上一笔的 `COMM-DIR-OPERAND` 同形）。

- `2026-10-16` qoder / todo 循环第三十笔：**批次三百一十七合入**（`claim/JOT-GETOPT-LINE-qoder` @ 基线 `4b2a81e4f`，只动 `os/commands/usr-bin/textfilter/{src/jot.rs,src/bin/jot.rs,tests/jot_face.rs}` + 18 篇两件文档，单域三线无跨界）。给 `jot` 的诊断面按 C 分回**四类出口**：`jot.c:150` 的 optstring 是 `"b:cnp:rs:w:"`，`c`/`n`/`r` 不带值而 `b`/`p`/`s`/`w` **四个都带值**（判定表里带值项最多的一支，`cn` 相连容易被误读成 `c:`+`n`），四类是 ① 集外字母 = libc 一行 + usage 一行 ② 带值字母缺值 = libc 的 `option requires an argument` + usage 一行 ③ `-p` 的值坏 = 一句 `errx` **不接 usage** ④ 环结束后：零操作数是只有 usage 且**没有** libc 行、五操作数以上是一句 `Too many arguments`。改前的形状是三处并一处：`-b`/`-s`/`-w` 缺值只打 usage、`-p` 缺值错走 `Bad precision value`、集外字母也少 libc 那一行，而**零操作数根本不报错**（按 C 的默认值跑起 100 行来）。两处只能量出来推不出来的形制：`-b` 与 `-w` 落进同一个 `format` 槽（`jot.c:172-176` 的 FALLTHROUGH），所以 `-bX -wY 3` 打三个 `Y` 而 `boring` 不落下；环不重排，`jot 3 -Z` 里 `-Z` 是 begin 那个操作数，`sscanf("%lf")` 不吃它时取末字符代码点当值（`jot.c:203-204`），于是打 90/91/92 而不报错。`-p` 的接受集由 `strtol(…, 0)` 定：base 0 认 `0x2` 与前导 `0` 的八进（`-p 010` 就是 8 位小数）、空串合法（`*ep` 仍是 `\0`）、`09`/`3x` 因尾巴剩字符被拒，且值先饱和到 `long` 再像 C 那样截成 `int`——`99999999999999999999`→`-1` 被拒、`4294967298`→`2` 被收（两支都经参照件现量）。选项环提入库侧 `parse_jot_args` 与 `parse_precision`（libc 两行交回 `GetoptReport` 单源，`errx` 句子留在壳里跟着 `getprogname()`）。**本支没有读写缝缺陷**：`jot` 既不读标准输入也不开任何档，值全由命令行推出来（那处缺陷的八次实例到上一笔 `comm` 为止）。读数：整包 **793 / 0 / 0**（62 target，基线 775/61）、clippy 0 error 且 **85 warning 同基线**、workspace 0 error、宿主三架构各 `Finished`、rustfmt 触及件零漂移（`src/jot.rs` 那 10 处存量漂移一并抹平）；A/B 与参照件 `tmp/jot-317/jot_ref`（`jot.c` 真源 + Minix 的 `getopt.c` + 程序名与 `strlcpy` 垫片）比 62 形，**前门 36 形 SAME=36 DIFF=0**，比标准输出的 26 形 **SAME=23 DIFF=3**——三形全在决定半（`3 1 100 0`/`3 100 1 0` 的 step 为零、`-r` 的随机流）；五枚变异负控各咬 **2 / 1 / 1 / 2 / 1** 枚；test-audit 第 2 维当场逮住门自己写错的两处（`-cb 3` 里 `3` 被 `-b` 取走因而不是缺值、`-p 010 3` 的期望抄错成三行同值），都按参照件改期望而不改代码。新登记不执行：`JOT-STEP-ZERO-UNBOUNDED`（C 在 `jot.c:281-284` 见 `step == 0.0` 就 `break` 留 reps，本壳推成无限输出，A/B 首跑被它吃到 5 GB 内存）、`JOT-PREC-MINUS-ONE-FORMAT`（`prec` 保持 `-1` 时 C 拼出的是坏说明符，宿主打回字面量而本壳打整数形，本机不可判定）、`JOT-RANDOM-NEGATIVE-SEED`、`JOT-REPS-BASE0-AND-NEGATIVE`、`JOT-FORMAT-BUFSIZ`、`JOT-DERIVATION-ERRX-FACE`（推导期三句 `errx` 进程级没门钉）。顺手翻正：上一笔在 18 篇台账留下的两条同文 `批次三百一十五` 标题合回一条。判定表进度：已对齐 19 支 + pr；待接 `unifdef`/`units`/`seq`/`paste`/`tr`；另判 `expand`/`hexdump`/`colcrt`/`lam`。下一轮首动作 `tools/claim.sh claim UNIFDEF-GETOPT-LINE qoder`（`unifdef.c:273` 的 optstring 是 `"i:D:U:I:o:cdeklst"`，五个带值项（`i`/`D`/`U`/`I`/`o`），且大小写字母混排）。新工具坑一条：**宿主 A/B 不能与容器的变异腿并发**——变异腿会往同一个 `os/target/debug/` 里写按变异源链出的二进制，本笔第一次重跑 A/B 因此报出一枚假 DIFF（`Too many arguments` 报最后一个操作数），已改名为 `tmp/jot-317/ab3.contaminated-by-mutation-rebuild.log` 留证；另一条：给会按操作数生成无限输出的命令做 A/B，每一形都要套 `timeout`，`$(...)` 会把无限输出全吃进内存。

- `2026-10-16` qoder / todo 循环第三十一笔：**批次三百一十八合入**（`claim/UNIFDEF-GETOPT-LINE-qoder` @ 基线 `caefad38c`，只动 `os/commands/usr-bin/textfilter/{src/unifdef.rs,src/bin/unifdef.rs,tests/unifdef_face.rs}` + 18 篇两件文档，单域三线无跨界）。给 `unifdef` 的诊断面按 C 分回**三类出口**：`unifdef.c:273` 的 optstring 是 `"i:D:U:I:o:cdeklst"`——五个带值（`D`/`U`/`i`/`I`/`o`）加七个不带值，且**大小写算不同字母**（`-uD` 的 `u`、`-tE` 的 `E` 都集外，可达出口第一次只能按字母集合本身定，不能按词族猜）。三类是：集外字母与缺值各由 libc 先印一行再接 usage；`-i` 的后随字符不是 `D`/`U` 或符号名不合 `addsym` 的形时**只打 usage 一行**（改前在这两处自拟了 `-i must be followed by D or U` 与 `bad symbol spec …`，各多一行）；环结束后「一个符号都没有且没给 `-s`」是 `warnx` 一句 + usage，且**排在操数判定之前**（所以 `unifdef -- -z` 报的是它而不是「只能一个档」）。本支四类出口**全退 2**，`usage()` 自己 `exit(2)`，而 usage 行的程序名写死在串里、libc 行跟着 `getprogname()`。四处只能量出来的形制：`-D ''`/`-U ''` 是**合法**空符号名（`skipsym` 原地返回，旧写法拒收）；`-i` 先吃一个字母，`-iDA` 的符号是 `A`（与 `-D A` 逐字节同效）、`-iD=1` 是「空名字挂个值」的合法形；环不重排，`unifdef -DA 档 -c` 的 `-c` 是第二个操数；开不了的档名是 `err(2, "can't open %s", …)` 三段形（旧写法缺 `strerror` 一段）。**第九处「薄壳裸 `read` 宿主读不到输入」在本支读腿**，现走 `bin_support` 的 `read_stdin`/`read_file`。读数：整包 **810 / 0 / 0**（63 target）、clippy 0 error 且**整包诊断 59 条同基线、触及件 4 → 3**（`if let Err(_)` 改 `.is_err()`，零新增；`grep -c '^warning'` 的 85/82 之差是各 crate 汇总行随缓存重放，不是告警增减，这条腿以后按带 `-->` 的诊断条数计）、workspace 0 error、三架构各 Finished、rustfmt 触及件零漂移；A/B 与参照件 `tmp/unifdef-318/unifdef`（`unifdef.c` + Minix `getopt.c` + 程序名与 `strlcpy` 垫片，另需空 `namespace.h` 与 `__printflike` 空宏）比 72 形四件（退码/stderr/stdout/`-o` 档内容），**前门 68 形 SAME=68 DIFF=0**，四枚 DIFF 全在决定半；五枚变异负控各咬 **3 / 2 / 3 / 2 / 2** 枚；test-audit 第 2 维当场逮住门自己写错一处（`-iU` 自己就是完整一支，符号名是剩下那段而非下一个 token，旧断言把 `-DFOO` 当成了它的值），按参照件改期望而不改代码。新登记不执行：`UNIFDEF-SYMLIST-OUTPUT`（07 篇那句「`-s` 符号清单」是超前陈述：C 由 `findsym` 查表时 `printf`，`unifdef -s` 打 `A`/`B` 两行而本壳打空，退码一致）、`UNIFDEF-D-DEBUG-OUTPUT`（`-d` 在 C 会往标准错误刷状态转移行，不是纯忽略）、`UNIFDEF-O-ERRNO-LEG`（`write_file` 返回 `Result<(), ()>` 无 errno 通道；C 的 `unchanged` 一句与临时档改名流程本层不可达）、`UNIFDEF-MAXSYMS-ERRX`（4096 个符号才触发，句子备好无门钉）。流程上一处改进：**A/B 排在变异腿之后跑并在跑前重链二进制**，避开三百一十七踩过的「变异源污染被测二进制」。判定表进度：已对齐 20 支 + pr；待接 `units`/`seq`/`paste`/`tr`；另判 `expand`/`hexdump`/`colcrt`/`lam`。下一轮首动作 `tools/claim.sh claim UNITS-GETOPT-LINE qoder`（`units.c:817` 的 optstring 是 `"lLvqf:"`，只有 `f` 带值）。

- `2026-10-16` qoder / todo 循环第三十二笔：**批次三百一十九合入**（`claim/UNITS-GETOPT-LINE-qoder` @ 基线 `d967ef638`，只动 `os/commands/usr-bin/textfilter/{src/units.rs,src/bin/units.rs,src/bin_support.rs,tests/units_face.rs}` + 18 篇两件文档，单域三线无跨界）。给 `units` 的前门按 C 分回**三类出口**：`units.c:817` 的 optstring 是 `"lLvqf:"`，只有 `f` 带值（所以缺值那一支本支只有一个字母可达），`l`/`L` 是两个语义成对的字母而不是大小写同义，`v` 在环内就退出。三类是：集外字母与缺值各由 libc 先印一行再接**整块九行** usage、`-v` 的两行版本打在标准错误上再接同一块、操作数不是 0/2/3 个或列表模式带操作数时**只有** usage 块，三支都退 3；usage 里的程序名写死在串里，libc 那两行跟着 `getprogname()`。**判据次序是可观测的**：C 把个数判排在 `readunits` 之前，所以 `units -f <坏档> m` 报 usage 而不是开档失败——旧写法把这形拖到读库之后。两支开档句子分属两形：`-f` 是 `err` 三段形（带 `strerror`），默认档找不到是 `errx` 两段形（不带），且默认档名按 `pathnames.h` 是 `/usr/share/misc/units.lib`，本壳此前写成 `/usr/lib/units`、07 篇第 212 行跟着错，本笔一并翻正。四处只能量出来的形制：`-f - m sec` 的档名就是 `-`（取下一个 token 不看脸色）；环不重排，`units -f 档 m sec -l` 的 `-l` 是第三个操作数（参照件 `Unknown unit '-l'` 且退 0，旧写法当旗标吃掉并进列表、还退 1）；裸 `-` 是操作数而 `--` 被吃掉；`optstring` 里的 `:` 只是记号，`units -:`/`-l:` 走集外字母那一支而 `-f:` 的冒号是值——**这最后一处是 full-review 的代码维度在补正轮抓出来的**，同轮改码加测。**第十处「薄壳裸 `read` 宿主读不到输入」在本支交互腿**：`units` 的交互提示机从没被进程级测过，新增 `bin_support::read_line_bounded` 双生缝（宿主 `std::io::stdin`、目标 `minix_sys::read`，按字节读，与 C 的 `fgets(buf, 80, …)` 同为一次至多 79 字节、截断时换行留在流里）。第二处交互面实缺陷：C 的 `do { 提示; fgets; } while (不认)` 是**重问同一格**（参照件对 `bogus\nm\nsec\n` 打 `You have: You have: You want: `），旧写法把两次读当成 have/want 一对。选项环提入库侧 `parse_units_args`，`mywarnx` 三句提入库侧 `units_error_line`。新门里 `-L` 那一枚当场抓到另一处实缺陷：`list_units` 的两个展开分支各多打一个换行（C 的 `listunits()` 是「先 `printf` 名字、再 `showunit()`」，结尾换行归 `showunit` 自带），本壳写成 `"{}{}\n"` 于是每条展开行后多一个空行——`-l` 的两支不受影响，所以这一形此前既没进门也没被库测覆盖。读数：整包 **839 / 0 / 0**（64 target）、clippy 0 error 且整包 65 条诊断（较上轮 59 多的六条全在 `src/units.rs` 决定半旧码，该文件上一轮走缓存未重打印，**触及件零新增**；落在我新码的那枚 `single_match` 已改 `if let`）、workspace 0 error、三架构各 Finished、rustfmt 触及件零漂移（顺带抹平 `src/units.rs` 那 14 处存量漂移）；A/B 与参照件 `tmp/units-319/units` 比 46 形（退码、stdout、stderr 三件逐形比，每形套 `timeout 5`）**SAME=40 DIFF=6** 形，前门 SAME=40 DIFF=6（差形：环不重排第三操作数是 -l、陌生名 have、陌生名两边、裸 - 作操作数、-L 展开真库全清单（决定半量级用）、非 ASCII 聚簇 -é）；六枚变异负控各咬 16 / 5 / 1 / 2 / 1 / 1 枚。test-audit 第 2 维当场逮住门自己写错两处（`-l <档>` 与 `-q <档>` 都漏了 `-f`，于是那两形落进「个数不对」而不是被测的那一支）与一处期望照抄失误（`bogus1 bogus2` 的名字段被 C 的尾数字重复规则折成 `bogus`），都按参照件改期望而不改代码；第 1 维补出 `-L` 展开列表那枚缺口。新登记不执行：`UNITS-PARTIAL-UNIT-ON-ERROR`（C 忽略返回值、拿半成品照样出答案块，本壳库交回 `Err` 就没有半成品）、`UNITS-LEADING-DASH-NAME`（参照件报 `'-l'`，本壳名字段少前导短横）、`UNITS-PATH-SEARCH`（默认档找不到时 C 先按 `PATH` 逐目录试）、`UNITS-ERRPREFIX-STDOUT`（列表模式 `errprefix = "/ "` 让句子改走标准输出带前缀，本壳 `list_units` 只数不印）、`UNITS-FGETS-80-DBLINE`、`UNITS-NONUTF8-DROP`。判定表进度：已对齐 21 支 + pr；待接 `seq`/`paste`/`tr`；另判 `expand`/`hexdump`/`colcrt`/`lam`。下一轮首动作 `tools/claim.sh claim SEQ-GETOPT-LINE qoder`（`seq.c:102` 的 optstring 是 `"f:hs:t:w"`，`f`/`t` 带值、`s`/`w` 不带，`w` 在串尾易误读成带值）。
- `2026-10-17` qoder / todo 循环第三十三笔：**批次三百二十合入**（`claim/SEQ-GETOPT-LINE-qoder` @ 基线 `65457036c`，只动 `os/commands/usr-bin/textfilter/{src/seq.rs,src/bin/seq.rs,tests/seq_face.rs}` + 18 篇两件文档，单域三线无跨界）。判定表第二十四支 `seq`：环写作 `seq.c:101-102` 的 `while ((optind < argc) && !numeric(argv[optind]) && (c = getopt(argc, argv, "f:hs:t:w")) != -1)`，句子是**三家两个退码**（`errflg` 与 `errx(1,…)` 退 1、`e_atof()` 一族退 2，且 C 的判序是先 `errno == ERANGE` 后尾巴杂字符）。三个真缺陷：环在第一个坏字母处就退出（参照件 `-ABC 3` 三行、`-5e310` 五行，少掉的整串 libc 行是本支最大的形状差）、`-w` 两级的宽度取自请求的上界而**不是实际被打印的末值**（`generate_format` 先用 `first ± incr * floor((first - last) / incr)` 折回来，故 `seq -w 1 5 10` 只打 `1`、`6` 宽度 1，而这里 C 的 `floor` 与 Rust 的截断除法在异号时差一格）、空格级 `% <宽>g` 让**非负数也占一个符号位**（`seq -w -w 8 10` 的 `10` 是三列的 `" 10"`，`seq -w -w -10 10` 里 `0` 得 `"  0"` 而 `-9` 得 `" -9"`）。另有两处随码修回：`unescape` 的十六进引导字母只有小写 `x`（C 的 `switch` 没有 `case 'X'`，`seq.c:353`）、`-s` 的分隔符打在**每一个数之后**含最后一个（`seq -s , 1 3` 的字节是 `1,2,3,`，`seq -s '' 1 3` 是 `123` 且无尾换行）。四处量出来才知道的形制：环前那道 `!numeric` 只挡以短横打头的 token（`seq 1e2` 一句不印就当操作数，因为 libc 见首字符不是 `-` 就收环且不推进 `optind`）；`numeric("")` 反倒为真且 `e_atof("")` 是合法的 0（`strtod` 把 endptr 停在起点那个 `\0`）⇒ `seq ""` 打 `1\n0\n`；解析次序 `last`→`first`→`incr` 加上环不重排 ⇒ `seq 3 -w` 点名的总是 `-w`；`valid_format` 只把 `A a E e F G g` 记转换且判 `conversions <= 1` ⇒ `%d` 与 `%d\n` **都非法**而 `abc`/`%%`/空串合法。**本支既无读缝也无开档腿**——判定表走到这里第一次碰上纯生成形的命令，「薄壳裸 `read` 宿主读不到输入」那一家族停在十处（到 `units` 为止）。选项环提入库侧 `parse_seq_args`，`e_atof`/`numeric`/`unescape`/`valid_format` 全归库侧单源，libc 两句交回 `lib.rs:GetoptReport`。新门 15 枚 + 库测 21 枚（基线 9 枚，本轮新增 12 枚）。读数：整包 **866 / 0 / 0**（65 target，基线 839/63）、`clippy --all-targets` 0 error、带 `-->` 的整包诊断 65 条而**指向本轮触及件的为 0 条**（一度多出的 `unused import: ToString` 已消）、workspace 0 error、宿主三架构各 Finished、`rustfmt --check` 零漂移；A/B 与参照件 `tmp/seq-320/seq` 比三件：**前门 64 形 SAME=64 DIFF=0**，两形（`1e310`、`1e-40000`）走 `err(2, "%s", num)` 的三段形，退码与句子形状全同而 `strerror(ERANGE)` 末段按 Minix 34 号表是 `Result too large or too small`、参照件链的 glibc 那张表写 `Numerical result out of range`（批次三百一十一的同源差），按 Minix 表归一后 SAME=2 DIFF=0；十形 DIFF 全在决定半。十三枚变异负控（`ctl320.py` 九枚 + `ctl320b.py` 四枚，全跑最终源码）分别咬 **5/跑不完/2/4/8/2/2/1/3** 与 **1/2/2/1** 枚——「跑不完」那一枚是死循环把测试进程撑到 OOM 被杀，形状本身就是证据。登记不执行五处：`SEQ-FORMAT-RENDER`（`-f` 合法之后的浮点渲染与非整数操作数，参照件照常打数）、`SEQ-NONFINITE-OPERAND`（`seq inf` 把参照件刷到 `timeout` 被杀、`seq nan` 打空退 0）、`SEQ-SUBNORMAL-ERANGE`（`1e-310` 在 glibc 置 ERANGE，而 Minix 的 gdtoa 那份实现不在本仓库，只有头文件 ⇒ 不推断）、`SEQ-NONASCII-OPTLETTER`（C 用 `%c` 打裸字节 `Ã`/`©`，本壳按 `char` 渲染，行数与退码同）、`SEQ-UNESCAPE-NONUTF8`。**工具坑新增**：宿主三架构那一腿的 riscv 目标名必须写全称 `riscv64gc-unknown-none-elf`；写成 `riscv64gc-unknown-none` 时 rustc 报「could not find specification for target」，看着像交叉编译失败其实是配方笔误。判定表剩 `paste`（`paste.c:70` 的 `"d:s"`：`d` 带值、`s` 是旗标）与 `tr`（`tr.c:68` 用 libc `getopt("cds")`，三个都是旗标——早前把 `tr` 记在「另判」是错的，本笔按现查翻正），另判 `expand`/`hexdump`/`colcrt`/`lam`。
- `2026-10-17` qoder / todo 循环第三十四笔：**批次三百二十合入后紧接的 `paste` 一笔（拆两片，同笔收口）**（`claim/PASTE-GETOPT-LINE-qoder` @ 基线 `e7e79fa5e`，只动 `os/commands/usr-bin/textfilter/{src/paste.rs,src/bin/paste.rs,tests/paste_face.rs}` + 18 篇两件文档，单域三线无跨界）。判定表第二十五支：`paste.c:70` 的环是 `getopt("d:s")`，而 `case '?': default: usage();`（`:80-82`）**坐在环里直接退出** ⇒ libc 报文不可能累加（与刚做完的 `seq` 相反，判定表里第一次出现这种形状）；错误先后就是扫描先后（`-d '' -Z` 报「没有分隔符」，`-Z -d ''` 报集外字母）；usage 只有一行且程序名写死在串里（`:231`）退 1；`-d ''` 是 `errx(1, "no delimiters specified.")`——句子自带句号且不带 `strerror`。开不了的档名**两种模式两条相反出口**：并行 `err(1, "%s", …)` 三段形致命退 1、顺序 `warn("%s", p)` 三段形跳过并继续且整体退 0，两支句子同形而退码与是否继续相反。四处量出来的形制：操作数**没有个数判**（`paste` 零操作数不读 stdin、不打东西、退 0）；环不重排（`paste a.txt -s` 的 `-s` 是第二个档名）；`-d` 的值可紧贴本 token（`-ds` 的 `s` 是值是值不是旗标）；`tr()`（`:199-226`）只认 `\n`/`\t`/`\0` 且**认不出的转义丢掉反斜杠**（与 `seq` 相反）、`\0` **占位而不打**。并行合并按参照件重建：游标**每行从表尾那个 NUL 起**（`dp = delim + delimcnt`），追平环按**列号**走 ⇒ 见底的流会让后面的列**多补一个分隔符**（`paste -d '+:' a b a` 第三行 `3+:3`），行尾再给剩下的每一列各补一个；且**一列 = 一个已打开的流**（`paste - -` 两列共用游标，`paste a.txt a.txt` 是两个游标），库侧签名因此改成 `(columns, sources, …)`。**第十一处「薄壳裸 `open`/`read` 宿主读不到输入」**：本支的读缝那一层是裸 `minix_sys::open`，宿主开不了任何档名 ⇒ 操作数形状与两种合并从未被进程级测过；本笔接到 `bin_support::read_file` 双生面。新门 `tests/paste_face.rs` 十一枚 + 库测 `paste.rs` 十一枚（全新增，本支把前门提入库侧 `parse_paste_args`/`tr_delims`）。读数：整包 **881 / 0 / 0**（66 target，基线 866/65）、`clippy --all-targets` 0 error 且**本轮触及件 0 条诊断**（中途四条 `collapsible_if`/`needless_lifetimes`/`items_after_test_module` 在末片清零）、workspace 0 error、宿主三架构各 Finished——**它抓到 `bin/paste.rs` 的 `vec!` 未导 `alloc::vec` 的交叉编译错（与批次三百一十七同坑第二犯）**、`rustfmt --check` 零漂移；A/B 与参照件 `tmp/paste-321/paste`（`paste.c` + Minix `getopt.c` + 程序名垫片 + **BSD `fgetln` 垫片**，glibc 无此函数，按 BSD 语义复刻：返回含换行的整行、以长度传长度、EOF 且一字未读才 `NULL`）**前门 49 形 SAME=49 DIFF=0**；十枚变异负控分别咬 **5/4/1/1/2/2/6/5/2/1** 枚。登记不执行三处：`PASTE-SERIAL-LINE-MAX`（`fgets` 的 `_POSIX2_LINE_MAX + 1` 上限与超长行的 `err(1, "%s: input line too long.")`，其末段是 `strerror(0)`，参照件打 `Success`）、`PASTE-NONASCII-OPTLETTER`、`PASTE-NONUTF8-DELIM`。判定表**只剩 `tr`**（`tr.c:68` 用 libc `getopt("cds")`，三个都是旗标），另判 `expand`/`hexdump`/`colcrt`/`lam`。**工具坑再加一条**：台账与文档里凡要写 `\n`、`\t`、`\0` 这类字面反斜杠串，python 侧必须用 raw string，否则真控制字符会把 markdown 表格行截断（本支写 `07-text-filter.md` 时踩到，随后按行并回并把控制字符换回字面转义，复核以 ` |` 结尾且全段无裸制表符/NUL 才算过）。
- `2026-10-17` qoder / todo 循环第三十五笔：**判定表走满的那一支 `tr`**（`claim/TR-GETOPT-LINE-qoder` @ 基线 `c39fd86dc`，只动 `os/commands/usr-bin/textfilter/{src/tr.rs,src/bin/tr.rs,tests/tr_face.rs}` + 18 篇两件文档，单域三线无跨界）。`minix3/usr.bin/tr/tr.c:68` 的环写作 `while ((ch = getopt(argc, argv, "cds")) != -1)`：三个字母都不带值，所以 libc 的「option requires an argument」在本支不可达；`case '?': default: usage();`（`:79-81`）坐在环里直接退出，于是集外字母**至多一行** libc 再跟整块四行 usage——与上一支 `seq` 的逐字母累加正好相反，判定表因此拿到第二个「同族不同形」的实证。四段判据按 C 的原次序各一句：`switch (argc)` 打发 0 与 3+（`:86-97`）、`-ds` 要两个（`:104-106`）、`-d` 不许两个（`:123-125`）、无旗标必须两个（`:156-157`），而 `-s` 单带一个在 `:139-140` 自成一局——**这条不对称就是 usage 四行的来历**，写成三个 `if` 而不是合并成一个布尔式，既对齐 C 也让 clippy 的 `nonminimal_bool` 无处可合。三处 libc 的隐蔽规则全靠现量才浮出：`getopt.c:79` 取短横后的第一个字节且 `:80-85` 只认恰好两横 ⇒ `tr --s a` 报的字母是 `-`；`getopt.c:86-93` 让光杆 `-` 落进操作数 ⇒ `tr - a` 把 `-` 换成 `a`、`tr - -` 原样；非选项收环不推进 ⇒ `tr a - b` 是三个操作数。另一条属数据通路但同样量出来才信：`-c` 与 `-s` 齐给时挤压集取 **string2**，所以 `tr -cs a b` 对 `aabbcc` 打 `aab`（连结尾换行都被挤掉）。门测 9 枚（本支第一扇端到端门）+ 库测 3 枚。读数：容器整包 **893 / 0 / 0**（67 target）、`clippy --all-targets` 退码 0 且指向触及件 0 条、workspace 0 error、宿主三架构各 `Finished`、`rustfmt --check` 零漂移（顺手把 `src/tr.rs` 自带的三处排版漂移归零）；A/B 与参照件 `tmp/tr-322/tr_ref` 比三件，**65 形 SAME=62 DIFF=3**，三形全在已登记的集合语法一族内（前门 49 形全等）。**八枚变异负控**（`tmp/tr-322/ctl322.py`，跑法带 `--no-fail-fast` 以保两个测试目标都执行）分别咬 3/6/3/编译挡住/5/3/5/1 枚，其中 7 枚被端到端门当场咬住。两处工具坑记档：① 三架构配方**必须带 `-p minix-textfilter`**，漏掉就成了整工作区建 bins，`minix-rt` 带默认 `std` feature 去编 none 目标，报「can't find crate for `std`」上千条而像是代码崩了；② A/B 与量读的日志**逐形一行**打印，三行一形的旧格式在一次 `grep -A` 里把两条形的行交叉了，据此差点把两条正确的期望值改错。登记不执行三处：`TR-SET-GRAMMAR-SENTENCES`（四形现量值已入台账：`[x*3]`/`[a*3]` 在 string1 参照件退 1 打 `Sequences only valid in string2` 而本层退 0 原样、`z-a` 参照件当三个字面成员退 0 打 `ybc` 而本层退 1 静默、`[:bogus:]` 参照件退 1 打 `unknown class bogus` 而本层退 1 静默）、`TR-NONASCII-OPTLETTER`（`%c` 裸字节对 `char` 渲染，同 `SEQ-`/`PASTE-`/`UNITS-` 一族）、以及一条**共享缝**的新登记 `BIN-SUPPORT-HOSTED-ARGV-NONUTF8`：宿主侧 `bin_support.rs:args` 走 `std::env::args()`，参数含非法 UTF-8 时当场 panic（`tr`/`seq`/`paste`/`units`/`rev`/`fold` 六支实测全退 SIGABRT），而目标侧同一函数走 `from_utf8_lossy`——两条双生面在非法字节上不等价，`TR-NONASCII-OPTLETTER` 那条也因此在本机量不到真值。**判定表走满**（head/tail/uniq/cut/join/sort/vis/nl/cksum/fold/rev/colrm/split/unexpand/col/column/comm/cmp/jot/unifdef/units/seq/paste/tr + pr），另判四支 `expand`/`hexdump`/`colcrt`/`lam`；下一支改按 `tmp/todo-loop-PLAN.md` 的 R 表现查未标 ✅ 的第一项（该表行号是 R1…R30，本循环几条台账里写过的「V3-11」是失效标签，本笔一并翻正）。
- `2026-10-17` qoder / todo 循环第三十六笔：**`tr` 的空 string1 折回恒等映射（P0-code-bug）**（`claim/TR-EMPTY-SET1-TRANSLATE-qoder` @ 基线 `872948d89`，只动 `os/commands/usr-bin/textfilter/{src/tr.rs,tests/tr_face.rs}` + 18 篇两件文档，单域三线无跨界）。缺口是上一笔复核翻出来的：库侧 `tr.rs:translate` 写着 `set1.is_empty() || set2.is_empty()` 就拒，而 C 的非取补路径**先**把映射表填成恒等（`tr.c:178-181`），空 string1 让改映射的循环第一次 `next(s1)` 就 break（`:214-215`），于是一张恒等表被原样用于过滤——参照件 `tr '' x` 逐字节照抄退 0，本壳退 1、不吐数据、也不打任何句子，退码/数据/诊断三件全错。修法取「删掉库侧多出的那半句判据」而不是「在壳里为空前门短路」：前者让 `translate` 的分支结构与 C 的三段一一对齐，后者等于在本树里再造一条 C 没有的分支并把同一语义复制到两处；`set2.is_empty()` 那半句保留，它是 `set2[set2.len() - 1]` 的越界防线，而 C 的等价物是循环之前那句 `errx(1, "empty string2")`（`:184-188`），本层已接在壳里，两条腿不是一个判据。门 +2 枚（库测 `test_empty_first_set_is_the_identity_map`、端到端 `an_empty_string1_is_the_identity_map`，后者同时钉住取补空集是全体字节故 `tr -c '' x` 得 `xxxxxxx`、空 string2 仍走那句 errx、三个操作数仍判个数）。读数：空集合一族 13 形现量 SAME=13（开工前 4 形 DIFF，`tmp/tr-323/measure_before.log` 与 `measure_after.log`），上一笔 65 形全量复跑 SAME=62 DIFF=3 不退化，四枚变异负控全咬且都咬到新门那枚。复核顺带发现的四条**存量**形状按名登记不执行：`TR-CLASS-OUT-UNUSED`（`CharClass::List` 一族只被测试构造、`parse_set` 的类数组被每个调用方丢弃）、`TR-TOOLONG-ARMS`（三处 `TooLong` 在现有壳里不可达）、坏集合面静默退 1（归 `TR-SET-GRAMMAR-SENTENCES`）、`%c` 裸字节与非法 UTF-8 argv（归 `TR-NONASCII-OPTLETTER` 与 `BIN-SUPPORT-HOSTED-ARGV-NONUTF8`）。
- `2026-10-17` qoder / todo 循环第三十七笔：**`jot` 的操作数派生 switch 按 C 的精确标签定形（P0：零步长无界）**（`claim/JOT-STEP-ZERO-UNBOUNDED-qoder` @ 基线 `9d117c1f6`，只动 `os/commands/usr-bin/textfilter/{src/jot.rs,tests/jot_face.rs}` + 18 篇两件文档，单域三线无跨界）。根因是一类**翻译形状错**而不是算法错：C 的 `switch (have)`（`jot.c:247-296`）拿 case 标签当**精确值**，本层却写成 `have & X != 0` 的子集判，于是五个操作数齐给时落进 `case ENDER | STEP` 那一支（`:249-259`，那里 `step == 0.0` 写 `reps = 0` 而打印循环是 `i <= reps || reps == 0`，`:129` ⇒ **永不终止**），而 C 真正命中的 `case REPS | BEGIN | ENDER | STEP`（`:281-284`）写的是 `break`（reps 保持 3 ⇒ 三行）。参照件现量三条：`jot 3 1 100 0` 与 `jot 3 1 1 0` 本壳无界、`jot 3 1 100 5` 本壳二十行而参照件三行。修法只有四条判据改成 `have == 值` 并按 C 的 case 次序排、末尾保留 `default`：不动壳、不动循环、不加特例。门 +4 枚（库测 3、端到端 1 内含六形），其中 `test_begin_ender_step_derives_reps` 是**变异负控咬出来的缺口**——把 `case ENDER | STEP | BEGIN` 这个标签删掉时原有 13 枚门全绿，说明 `jot - 1 100 5` 这一族从来没被钉过。读数：`tmp/jot-324/ab324.py` 二十形 SAME=16 DIFF=4，四处 DIFF 全是已登记的 `JOT-PREC-MINUS-ONE-FORMAT`；四枚负控（`tmp/jot-324/ctl324.py`）全咬且全部避开无界写法（变异体一旦把 reps 归零就会在门里真跑无限输出）。clippy 指向 `src/jot.rs` 的 4 条落在本笔未触碰的存量行，登记不执行。
- `2026-10-17` qoder / todo 循环第三十八笔：**`jot -r` 的种子判据按 C 收回**（`claim/JOT-RANDOM-NEGATIVE-SEED-qoder` @ 基线 `c40ad74cc`，只动 `os/commands/usr-bin/textfilter/{src/bin/jot.rs,tests/jot_face.rs}` + 18 篇台账两件，单域三线无跨界）。壳里那句 `Ok(v) if v >= 0.0 => v as u64` 是自创判据：`jot.c:118` 写的是 `srandom((unsigned long) step)`，先按 C 的截断取整再无符号回绕，**负数与小数都是合法种子**，参照件 `jot -r 3 1 10 -5` 退 0 打三个整数而本壳退 1 报 `Bad step value:  -5`——把能用的输入判成坏输入，属 P0-code-bug。修法只删判据、把转换写成 `(v as i64) as u64`，`parse_step` 的 `strtod` 全串检查不动，故 `5x` 两侧仍同句退 1。随机流的数值本就是另一档分歧（本层 LCG 非 libc `random()`，07 §4.4），所以门只钉退码、行数、全为数字，外加一条截断的可观察推论：`-0.5` 与 `0` 必须逐字节相同（参照件同读数 `9\n4\n8\n`）。门测 +1 枚内含七形；种子一族 10 形 SAME=9，唯一 DIFF 是 `jot -r 0 1 10 -5`——`reps == 0` 在 C 就是无界，两侧都跑到截停。clippy 触及件 0 条、workspace 0 error、三架构各 Finished、rustfmt 零漂移、库测 514 与门测 14 全绿、容器整包 900/0/0。三枚负控**只有一枚证明门的灵敏度**（放宽 `parse_step` 全串检查那枚被咬住）：加回自创判据那枚在编译期就挡住（模式不再穷尽），而把截断改成 +1 偏移那枚**漏网**——本层 LCG 与 libc `random()` 的数值流本是另一档分歧，「`-0.5` 与 `0` 同串」这条推论对种子的同幅偏移不敏感；要钉住转换本身得先把 `(v as i64) as u64` 提成库侧可单测的函数，已登记 `JOT-SEED-CAST-UNIT-TEST`（改的是可测性结构不是行为，本笔不顺手做）。本笔闭合三百一十七登记的第③条 `JOT-RANDOM-NEGATIVE-SEED`。
- `2026-10-17` qoder / todo 循环第三十九笔：**`col` 的集外字母改回按字节取，并注销一条不成立的旧登记**（`claim/COL-NONASCII-OPTLETTER-qoder` @ 基线 `c2abf1ea7`，只动 `os/commands/usr-bin/textfilter/{src/col.rs,tests/col_face.rs}` + 18 篇台账两件，单域三线无跨界）。批次三百一十四把这条登记成 `COL-NONASCII-PANIC`（写的是 `col -bé` 即 panic），本笔开工前现量六形一形都不 panic：`parse_col_flags` 只在字节上步进、一碰集外字母就 return，`arg[pos..]` 的 `pos` 必然在字符起点上——**登记的名字把症状猜错了，成立的是取字符不取字节**（参照件打裸字节 `0xC3`，本壳打整个 `é` 即 `c3 a9`）。按 `src/column.rs:parse_column_args` 三百一十四已定的同法改成 `char::from(bytes[pos])`，libc 那句 `%c`（`getopt.c:103`）拿的本就是 `optopt` 这个字节。门 +2 枚（库测钉 `é` 落在 U+00C3 且聚簇 `-xé` 同形；端到端钉 lossy 后的码位并复钉 ASCII 的 `-Z`）。A/B 六形 SAME=3 DIFF=3，三形 DIFF 是本族共有的**编码末段**：C 一个裸字节 `c3`，本树经 `char` 的 `Display` 成 `c3 83`；那一半要改 `src/lib.rs:GetoptReport::line` 的返回类型与八处调用方的拼句，属共享缝，登记 `GETOPT-REPORT-BYTE-LETTER`（与 `REV-`/`COLRM-`/`COLUMN-`/`UNITS-`/`SEQ-`/`PASTE-`/`TR-NONASCII-OPTLETTER` 同源，本笔起 `COL-` 也并入这族，`COL-NONASCII-PANIC` 这个名字注销）。三件套：容器 902/0/0、clippy 指向触及件的两条都在未触碰的存量行（`:314` never_loop、`:450` needless_range_loop）、workspace 0 error、三架构各 Finished、rustfmt 零漂移、库测 515 与门测 10 全绿；两枚负控都咬住（改回按字符取、取末字节不取首字节），**但本轮负控只有两枚，不到六枚门槛，属记账不是达标**。
- `2026-10-17` qoder / todo 循环第四十笔：**`jot` 的种子转换提到库侧并单测**（`claim/JOT-SEED-CAST-UNIT-TEST-qoder` @ 基线 `06dfabb6e`，只动 `os/commands/usr-bin/textfilter/{src/jot.rs,src/bin/jot.rs}` + 18 篇台账两件，单域三线无跨界）。起点是上一笔自己报出的一个漏网负控：给种子加一整幅偏移，端到端门照旧全绿——不是期望值写错，而是**可测性缺口**：`(v as i64) as u64` 写在壳里没法单测，而种子只在随机流里显形、本层 `Lcg` 与 libc `random()` 的流本就登记为另一档分歧（07 §4.4），所以任何门对「同幅偏移」都不敏感。做法是提为 `src/jot.rs:seed_from_step` 并在 doc 注释里写清 C 的两步（`jot.c:118` 的 `(unsigned long) step` 先向零截断再无符号回绕）、以及本层 64 位种子与 Minix ILP32 32 位 `unsigned long` 的位宽差。库测 +1 枚钉住 `0/-0.5/0.9/-0.9` 都截成 0、`-5` 与 `-5.9` 同种子且为 `u64::MAX-4`、正数不回绕。两枚负控：P1（加偏移，即上一笔那枚漏网形状）被新库测咬住；P2（换 `round_ties_even`）因该方法 unstable 而编译期就没过，对门的灵敏度既不证明也不否证，照此记录不冒充咬住。本轮负控仍只有两枚、不到六枚门槛，与上一笔同为记账；补齐随 `GETOPT-REPORT-BYTE-LETTER` 那轮做——本笔顺手把它的射程量清：调用点 26 处、覆盖 20 余个薄壳，`GetoptReport::line` 需从 `String` 改字节串，且多个既有门钉的是 `char` 渲染形状要同步改期望。三件套：容器 903/0/0、clippy 指向 `jot.rs` 的四条都是既有存量行（行号仅位移）、workspace 0 error、三架构各 Finished、rustfmt 零漂移、库测 516 与门测 14 全绿。
- `2026-10-17` qoder / todo 循环第四十一笔：**给 `column` 的残行告警计数补门，生产码一字未动**（`claim/COLUMN-TAIL-WARN-COUNT-qoder` @ 基线 `4643cab05`，只动 `os/commands/usr-bin/textfilter/tests/column_face.rs` + 18 篇台账两件，单域三线无跨界）。销的是批次三百一十四测试自审第 1 维留下的 `COLUMN-TAIL-WARN-COUNT`：`minix3/usr.bin/column/column.c:114-123` 逐操作数开流、逐流判残行，所以 `column.c:277-283` 那句两段形 `warnx` `column: line too long` 是**按来源计数**的，既有门只钉了单来源那一形。开工先把四形量清（参照件 `tmp/column-314/column_ref` 与新层逐形对读，全同）：两档各一条残行 ⇒ 两句退 1、同一档名写两遍 ⇒ 两句、好档 + 残行档 ⇒ 一句、`- tail1.txt` ⇒ 先 `column: Cannot open \`-': No such file or directory` 再补一句退 1。**结论是本层已对**——`src/bin/column.rs:86` 的 `for _ in 0..pool.dropped_tail` 本就按来源印，于是本笔只补门（`tests/column_face.rs:each_source_with_a_long_tail_line_warns_once`，第 13 枚，四段各钉一形，句数/首句/退码三项全取参照件读数），不改码；这一区分如实写进台账，免得门测的绿被读成修过什么。**六枚负控实得 4/5**：G2 句子改三段形、G3 整句不打、G4 不看残行每来源都打、G6 句子前缀漂移 ⇒ 都被既有门与新门一起咬住；G5「残行不再置退码位」编译期就没过，对门的灵敏度既不证明也不否证；G1「整轮只报一句」的替换锚点在源码里没命中，未施加、不计入咬住数——**仍不到六枚门槛，与上一笔同为记账**，欠账随 `GETOPT-REPORT-BYTE-LETTER` 那轮一起做。三件套：容器 `cargo test -p minix-textfilter -j 1` → 904/0/0（67 个 target，`column_face` 那格 `running 13 tests`）、clippy 退码 0 且 `column_face` 零命中（本笔新增行零诊断）、workspace 0 error、三架构各 Finished、rustfmt 零漂移、宿主直跑库测 516 与门测 13 全绿。
- `2026-10-17` qoder / todo 循环第四十二笔：**`bin_support::args` 的宿主腿改成 `args_os()` + lossy，两条腿同规则**（`claim/BIN-SUPPORT-HOSTED-ARGV-NONUTF8-qoder` @ 基线 `4e02c33c1`，只动 `os/commands/usr-bin/textfilter/{src/bin_support.rs,tests/bin_support_face.rs 新增}` + 18 篇台账两件，单域三线无跨界）。销的是批次三百二十三登记的那条**共享缝**：`src/bin_support.rs:args` 是一对 `cfg` 双生面，目标侧把 `minix_rt::crt0::args()` 的原始字节过 `String::from_utf8_lossy`，宿主侧那一腿写的是 `std::env::args()`——它对每个参数做 UTF-8 解码并在失败处 `unwrap`，于是同一个参数一条腿 panic、一条腿继续。开工现量（`tmp/bin-support-329/probe.py`→`measure.log`）：argv 含裸 `0xFF` 时 `tr`/`seq`/`paste`/`fold`/`rev`/`units` **六支全退 `rc=-6`**，唯一不退信号的是纯 ASCII 的 `seq 1 3`；参照件同形给的是 `tr_ref: unknown option -- \xff` + usage 退 1、`seq_ref` 两行报文退 1、`tr a\xffb c` **退 0 打 `cccccc`**、`paste -d\xff` 退 0 空输出。改法是宿主腿走 `std::env::args_os()` 并用 `OsStrExt::as_bytes` 取原始字节后过**同一个** lossy，从此两条腿共用一条渲染规则。**这是双生面对齐而不是与 C 对齐**：lossy 先把 `0xFF` 变成 U+FFFD，其后按字节取的支显成 `c3 af`、按 `char` 取的 `rev` 显成 `ef bf bd`、累加型环的 `seq` 把替换字符的三个字节当三个字母连报三行，而 C 只有一行原字节 `ff`——所以只撤 `BIN-SUPPORT-HOSTED-ARGV-NONUTF8`，`GETOPT-REPORT-BYTE-LETTER` 不撤，且它的射程反而量得更清：根因有**两处**，句子渲染侧（`src/lib.rs:110 UnknownOption(char)`、`:129 line() -> String`，26 个调用点）与 **argv 在缝上就被 lossy 吃掉**，后者要 `args` 两条腿都交字节、薄壳的选项环与操作数跟着按字节走。门：新立 `tests/bin_support_face.rs` 四枚（自己起子进程、argv 用 `OsStr::from_bytes` 传原始字节；公共夹具只收 `&str` 所以不借道），把每一形的退码、句子与**精确字节**都钉住——精确字节断言是故意的，字节化那笔落地时它们会当场变红，逼改的人有意识翻正而不是让门静默跟着漂。六枚负控实得 **3/6**：C1 退回 `env::args()` 咬住全部四枚、C2 整段丢词咬住三枚、C4 交回空串咬住四枚；C3 剥高字节、C5 丢 `argv[0]`、C6 strict `from_utf8` 都被编译期挡住，不证明灵敏度。不到六枚门槛，欠账照旧在台账。三件套：容器 908/0/0（68 个 target，新格 `running 4 tests`）、clippy 退码 0 且 `bin_support` 零命中、workspace 0 error、三架构各 Finished、rustfmt 零漂移、宿主直跑库测 516 与新门 4 枚全绿。
- `2026-10-17` qoder / todo 循环第四十三笔：**`rev` 的标准输入读错腿接上 C 的 `ferror` 形状，闭合 `REV-STDIN-ERRLEG`**（`claim/REV-STDIN-ERRLEG-qoder` @ 基线 `dafbdde28`，只动 `os/commands/usr-bin/textfilter/{src/bin_support.rs,src/bin/rev.rs,tests/rev_face.rs}` + 18 篇两件文档，单域三线 commands/textfilter，无跨界）：读缝以前把 `Err` 当 EOF（宿主 `let _ =`、真机 `Err(_) => break`），标准输入读出错时一句话不说还退 0；C 在行环之后试 `ferror` 并按**流名**报一行（`minix3/usr.bin/rev/rev.c:80-81` 的初值 `"stdin"`、`:101-104` 的 `warn` + `rval = 1`）。新增 `bin_support::read_stdin_error()`（两条腿同签名，交回已收到的字节与号），`read_stdin()` 退成它的第一半所以其余各壳语义不变，`rev` 先出已读到的行再报一行置退码。门 2 枚（宿主，承载是「stdin 是一个目录的 fd」→ `EISDIR`）；变异负控六枚全咬（1/1/1/1/4/1，`tmp/rev-331/ctl331.py`，baseline 与还原后同为 15/0）；A/B 九形 **SAME=9 DIFF=0**（`tmp/rev-331/ab331.py`，比对前把参照件自己的程名归一）。收尾自审抓到本笔自身两处并修：`bin_support.rs` 两处破碎中英混排的注释改为单语成句（C 锚点留在句内），`rev_face.rs` 的 `scratch()` 只按进程号命名而五枚门各要一个目录、并行跑时会互相 `remove_dir_all`，改成每枚门各拿互不相交的目录（负控六枚在此改动之后复跑仍 6/6 咬住）。读数最终取自**宿主回退**：WSL 重启后容器网络断了（`failed to get 'bitflags'`、交叉目标 `curl` 28 超时、容器内 DNS 解析到假 AAAA 而 HTTPS 返回 000、镜像 `/usr/local/cargo` 下无 `registry` 目录），同一条容器命令在本笔早期取过 917/0/0，故登记为环境断网而非代码变化；宿主 `ulimit -v 3145728` 下整包 **917/0/0**（68 target）、`cargo check --workspace -j 1` 0 error、三架构各 `Finished`、clippy 触及三件零命中、rustfmt 触及三件零漂移。新登记不执行 `REV-FILE-PARTIAL-ON-ERROR`（操作数中途读错时 `std::fs::read` 丢掉已读部分，C 先打印再报）。下一轮首动作 `tools/claim.sh claim COLRM-IO-ERRLEG qoder`。
- `2026-10-17` qoder / todo 循环第四十四笔：**`colrm` 的读写两根腿都接上 C 的 `check()` 形状，闭合 `COLRM-IO-ERRLEG`**（`claim/COLRM-IO-ERRLEG-qoder` @ 基线 `6975cab44`，只动 `os/commands/usr-bin/textfilter/{src/bin_support.rs,src/bin/colrm.rs,tests/colrm_face.rs}` + 18 篇两件文档，单域三线 commands/textfilter，无跨界）：C 把流上的错收成同一个 `check()`（`colrm.c:123-130`），先看 `feof` 再看 `ferror`，`getchar` 返 EOF 查 `stdin`、`putchar` 返 EOF 查 `stdout`，两句都是 `err(1, "%s", 流名)` 三段形加退 1。本壳读缝上一笔已有错误通道，写缝 `emit()` 却是静默 `terminate(1)`——退码对、句子没有。新增 `emit_error()`：宿主腿交回 `errno_of` 的号，真机腿按 `write` 返回值推余下字节（单次写可以只吃一部分，旧写法看不见短写）。**量出一个必须写进文档的事实**：参照件的写腿有观察阈值，实测在 (4096, 4505] 字节之间，小于一块 stdio 缓冲的输出要等 `exit()` 冲刷而 `exit()` 不报 `fflush` 失败——那是所链 libc 的机制而不是命令语义，所以否决「抄阈值」采纳「写不进就报」，分叉由门钉住，A/B 九形 SAME=8 DIFF=1 唯一一形就是它。门 4 枚、负控 6 枚各咬 2 枚（咬合数都是 2 恰好证明两根腿彼此独立）、A/B 用 `tmp/colrm-332/ab332.py`。登记不执行：先后规则在宿主不可证（`COLRM-ERRLEG-ORDER-UNOBSERVABLE`）、短写那一圈宿主测不到、本 crate 还有 34 支壳用静默 `emit()` 各走一轮。读数全部取自宿主 `ulimit -v 3145728` 加 `--offline`（WSL 恢复后容器与宿主网络同断，`index.crates.io` 返 `HTTP=000`）：整包 **921/0/0**（68 target）、`cargo check --offline --workspace -j 1` 0 error、三架构各 Finished、clippy 触及三件零命中、rustfmt 仅报 `src/bin/colrm.rs:23` 的 `use` 序**存量**漂移（主树同一份文件同样报，批次三百一十 `589fc4ce0` 留下，未顺手改）。下一轮 `claim CKSUM-HASH-STATE qoder`。
- `2026-10-17` qoder / todo 循环第四十五笔：**`cksum` 的 hash 选中状态建模，接上由它驱动的两条诊断，并修掉一枚吞操作数的下标缺陷（`CKSUM-HASH-STATE` 第一片）**（`claim/CKSUM-HASH-STATE-qoder` @ 基线 `b6f2f62ea`，只动 `os/commands/usr-bin/textfilter/{src/cksum.rs,src/bin/cksum.rs,tests/cksum_face.rs}` + 18 篇两件文档，单域三线 commands/textfilter，无跨界）：C 的 `const struct hash *hash`（`cksum.c:162`）驱动四条分支——`-a` 二次出现报 `illegal use of -a option`（`:195-199`）、`-o` 在选中后报 `%s mutually exclusive with sum` 且用表里大写 `hashname`（`:228-234`）、`requirehash` 在选中后放行（`:521-525`）、名字表八项且无 `ripemd160` 拼法（`:120-144`）。本壳不建状态，这三条一行都到不了。**新门第一次跑就照出一枚既有缺陷**：取值闭包与循环尾各推一次 `index`，带值选项之后紧跟的 token 被吞——`cksum -a crc file` 于是丢掉 `file` 转读标准输入，附值形同病；该闭包形全仓仅 `cksum.rs` 一处（`grep -rl parser_index os/commands` → 1）。`is_crypto_name` 另有一处事实错：只列六项且多认表里没有的 `ripemd160`，改成 `crypto_hashname()` 照表返回大写那一份并补齐 sha256/384/512。摘要仍不接：占位句从解析期移到解析之后，句子与退码 2 不变，整批接八项登记为 `CKSUM-HASH-DIGEST`（本仓 `os/libs/minix-crypt` 已有 md5/sha1，「等密码原语」这个前提对本仓已部分过期，但只接两项会新开一个分叉）。门：库测 5 条断言扩到 22 条，面测新增 3 枚；负控 6/6 各咬 1~3 枚（**下标退回旧算法咬 3 枚**，证明吞档断言有牙），baseline 与还原后同为 23/0。过程被 clippy 抓到我自己新门里一个未用变量（`s_s`），补断言后触及件零命中。读数（宿主 `ulimit -v 3145728` 加 `--offline`，容器与宿主网络同断）：整包 924/0/0（68 target，上笔 921）、`--lib cksum::` 15、`--test cksum_face` 8、workspace 0 error、三架构各 Finished、rustfmt 触及两件零漂移（`src/bin/cksum.rs:19` import 序为存量，主树同报）。登记不执行：`CKSUM-HASH-DIGEST`、多入口名 `argv[0]` 选算法、`CKSUM-FILE-LEGS`。下一轮 `claim JOT-PREC-MINUS-ONE-FORMAT qoder`。
- `2026-10-17` qoder / todo 循环第四十六笔：**定下 `jot` 精度未写那一族的真值判据，并把默认转换从整数支改到浮点支，闭合 `JOT-PREC-MINUS-ONE-FORMAT`**（`claim/JOT-PREC-MINUS-ONE-FORMAT-qoder` @ 基线 `679db151f`，只动 `os/commands/usr-bin/textfilter/{src/jot.rs,tests/jot_face.rs}` + 18 篇两件文档，单域三线 commands/textfilter，无跨界）：登记当年写的是「本机不可判定」，这一笔判据定了，结论是**宿主参照件在这一形不能当真值承载**。两条独立证据：Minix 的 libc 是 NetBSD 血脉，`vsnprintf_ss.c:213-226` 把 `.` 后的非数字收成精度 0、再按旗标循环把 `-` 当左调、`1` 当宽度、`f` 当转换，故 `%.-1f` ≡ `%-1.0f`（四舍五入）；本机探针实测 glibc 对同一串打回字面量 `%.0-1f`（`串=[%.-1f] 长度=5`、`出=[%.0-1f]`）。壳侧病根是 `options.prec.unwrap_or(0)` 把「从未写过」（C 的 -1）与「显式 0」合并，落进整数支 `%ld` 的 `floor`——`0.7` 该是 `1` 而壳打 `0`。改法：条件改成 `== Some(0)`，未写精度构造 `%-1.0f`；不再抄通用 printf 解释器（`-w` 的说明符走既有 `parse_conv_spec`）。门 2 枚（库测钉左调/宽度/精度 0/转换四要素与 0.7→1、1.5→2、2.5→2 的凑偶，另钉 `-c` 配未写精度仍走整数支；面门钉登记那四形退 0 且输出无小数点无 `%`）。负控**第一轮 4/6**：`%.0f`（丢左调与宽度）与「整数支忽略 chardata」两枚咬不住，实为断言不够而非代码无事——补旗标/宽度断言与 `-c` 配未写精度后 **7/7**，baseline 与还原后 38/0、逐枚核 md5。读数（宿主 `ulimit -v 3145728` 加 `--offline`，网络同断）：整包 **926/0/0**（68 target，上笔 924）、`--lib jot::` 23、`--test jot_face` 15、workspace 0 error、三架构各退码 0、rustfmt 触及两件零漂移（面门 `for` 数组由 rustfmt 要求换行后归零）、clippy 触及件零命中。登记不执行：`-r` 的种子默认 `time*getpid` 使宿主不可重复，上机对账归 `C-57`/E5；`-w` 用户说明符的 libc 分歧面同待真机；新增一条判例——以宿主 glibc 输出当 C 真值的对账须先查该形是否经 libc 解析。下一轮 `claim TR-SET-GRAMMAR-SENTENCES qoder`。
- `2026-10-17` qoder / todo 循环第四十七笔：**把 `tr` 集合语法的四句报文接到标准错误，并被参照件证伪了「`[` 一律是重复式」这条判据，改写 `bracket()` 的三条成形判据，闭合 `TR-SET-GRAMMAR-SENTENCES`**（`claim/TR-SET-GRAMMAR-SENTENCES-qoder` @ 基线 `5a6fbda21`，只动 `os/commands/usr-bin/textfilter/{src/tr.rs,src/bin/tr.rs,tests/tr_face.rs}` 与 18 篇两份文档；门读数：lib 18/0、face 14/0、全套件零失败、宿主三架构各 Finished、workspace 0 error、触及件 rustfmt 零漂移、clippy 触及件零命中、与参照件 21 形 A/B 得 SAME=20 / DIFF=1；负控六枚有效、第七枚因变异写法不编译而作废并如实登记；`TR-EQUIV-SENTENCES` 与 `TR-EQUIV-OOBCH-SET2` 两条拆出登记不执行；容器门因宿主与容器两侧网络不通未跑，`--offline` 回退口径已写在 18 篇条目里）
- `2026-10-18` qoder / todo 循环第四十八笔：**让 `split` 的写腿把平台那个号一路带到报文里，并给两臂的标签各立一枚门，闭合 `SPLIT-WRITE-ERRNO`**（`claim/SPLIT-WRITE-ERRNO-qoder` @ 基线 `0021e749f`，动 `os/commands/usr-bin/textfilter/{src/bin_support.rs,src/bin/split.rs,src/bin/uniq.rs,tests/split_face.rs}` 与 18 篇两份文档；门读数：全套 979/0、宿主三架构各 0 error、workspace 0 error、触及件 rustfmt 零漂移；负控六枚全咬，其中「开档腿丢号」那枚是先发现门只钉住路径没钉住句子、补强尾句断言后才咬住；目标腿的续写循环在 `cfg` 的另一侧，宿主门够不着，归真机门；`close` 失败与 `uniq` 写腿报文两条拆出登记）
- `2026-10-18` qoder / todo 循环第四十九笔：**把 `split` 分片文件的 `close` 腿接成第三臂，并为此拆出两枚纯函数好让「臂没接错」有门可钉，闭合 `SPLIT-CLOSE-ERRLEG`**（`claim/SPLIT-CLOSE-ERRLEG-qoder` @ 基线 `d52b9f383`，动 `os/commands/usr-bin/textfilter/{src/bin_support.rs,src/bin/split.rs}` 与 18 篇两份文档；门读数：全套 1058/0、宿主三架构各 0 error、workspace 0 error、触及件 rustfmt 零漂移、clippy 触及件零命中；负控六枚五枚咬，第六枚属 `cfg(not(test))` 侧接线、宿主不可达，记为本轮验证边界；中途三架构编译抓到一次 `E0308` 而宿主全绿，教训是双生面一侧的编译必须同轮跑；`SPLIT-CLOSE-HOST-VISIBILITY` 与 `UNIQ-WRITE-ERRLEG` 两条登记）

- `2026-10-17` qoder / todo 循环第五十笔：**把 `split` 的输入拆成开档、读档、标准输入三形各归各句，并顺手把断了三笔的权威容器门跑回来**（`claim/SPLIT-READ-STAT-ERRLEG-qoder` @ 基线 `f780c6614`，动 `os/commands/usr-bin/textfilter/{src/bin_support.rs,src/bin/split.rs,tests/split_face.rs}` 与 18 篇三份文档；C 的两句是 `err(1, "%s", *argv)` 与 `err(1, "read")`，标准输入与命名档共用 `read` 那一臂）：门读数 全套 1060/0、`split_face` 20/0、**容器 1060/0/0 退码 0**、宿主三架构各 Finished、workspace 0 error、触及件 rustfmt 零漂移、clippy 退码 0 且只剩基线里就有的 `items after a test module` 一条；负控五枚全咬（2/2/2/1/1），两枚不咬的根因都是门不够——前缀断言换成整句逐字节、以及新立一枚「目录当标准输入递进去」的门；自审抓到负控脚本用反向替换还原、而同形锚点有两处，源文件被写坏一次，改为整篇存原文还原并逐字节断言；容器门的病根、四条探针读数与那条以当前用户跑、把缓存与产物都挂到仓库外的完整配方写进 18 篇 §7，那里同时登记「三百三十五~三百三十七缺权威读数、待补 `--workspace` 一条」；`stat` 腿拆入下一轮 `SPLIT-STREAMING-MEMORY`）

- `2026-10-17` qoder / todo 循环第五十一笔：**纯台账笔：把 `split -n` 喂管道那一形量成分叉，并给取数那一步定下改法**（`claim/SPLIT-STREAMING-MEMORY-qoder` @ 基线 `102919d89`，只动 18 篇 `todo.md` 与本板；生产码、门一字未动）。现量三形：命名片 SAME、**管道 DIFF**（参照件 `split: can't split into more than 0 files`、退 1、零分片；本壳退 0、三枚分片）、重定向 SAME，脚本 `tmp/split-339/ab339.py`。根因是字节数取自 `input.len()` 而不是取自内核（C 在读之前 `fstat(ifd)`，`split.c:293-300`）。三条路走完判据并选定：命名档与真机各自在已打开的句柄上取数，宿主侧标准输入退到 `/dev/stdin` 这条路径（本机三形与 `fstat(0)` 同数：10/0/0），std 里没有不带 `unsafe` 的通道（`File: From<Stdin>` 未实现，探针 `/tmp/probe_stdin.rs`）。本笔销账后把认领与树一起释放，下一轮首动作 `claim SPLIT-STREAMING-MEMORY qoder`，两处落点与两枚新门已写在该条里。

- `2026-10-18` qoder / todo 循环第五十二笔：**把 `split -n` 要切的字节数改成向内核询问，管道那一形从此报错而不是切得出（闭合 C-108，`SPLIT-READ-STAT-ERRLEG` 的 `stat` 腿一片）**（`claim/SPLIT-STREAMING-MEMORY-qoder` @ 基线 `dd77ce7c0`，动 `os/commands/usr-bin/textfilter/{src/bin_support.rs,src/bin/split.rs,tests/split_face.rs}` + 18 篇 `todo.md`/`07-text-filter.md` 与本板，单域三线 commands/textfilter，无跨界）：`src/bin_support.rs` 新立 `input_size(path: Option<&str>) -> Result<u64, minix_sys::Errno>` 双生面（宿主命名档 `std::fs::metadata(path).len()`、标准输入退到 `/dev/stdin` 这条路径；真机 `fstat(STDIN)` 与 `stat(name)` 各按 `Option` 分派、`st_size` 夹住负数），理由与「承载差异不是语义差异」的论证沿用上一笔走完的判据（std 里没有不带 `unsafe` 的标准输入→可问大小对象的通道）。`src/bin/split.rs` 的 `-n` 分支第一件事改为问大小：问不到即 `split: stat: <strerror>` 退 1（对位 `split.c:293-295`），随后 `(n as u64) > size` 报 `can't split into more than <size> files` 退 1（对位 `:297-300`，句子打的是大小不是片数），`chunks_n()` 交回 `None` 那一臂打同一句话同一个数。门三枚新立（`tests/split_face.rs:556`、`:569`、`:613`），第三枚「报文引的是大小」是写码时才想到的一形——M2 那枚负控靠它才咬得住。读数：`--test split_face` **23/0**、宿主整包 **1063/0**（68 target）、**容器 1063/0/0 退码 0**、三架构各 Finished、workspace 0 error、触及三件 rustfmt 零漂移、clippy 退码 0 且触及件只剩基线里就有的那一条；A/B 三形全 SAME（上一笔的管道 DIFF 消失）；负控六枚全咬 1/2/2/1/2/2，还原用整篇存原文并逐字节断言。两条如实登记：`stat` 失手臂宿主构造不出失手的输入（问大小与开档读的是同一份 inode 信息），只有代码形状、没有门，真机侧归 E5；**边读边写本笔没做**，`SPLIT-STREAMING-MEMORY` 仍敞着，改动面是 `read_operand`、三种切法的驱动与写腿的释放时机，超一轮预算，按本文件惯例独立成轮。

- `2026-10-18` qoder / todo 循环第五十三笔：**把 `tr` 等价式的两句报文接上，`[=ab=]` 这一类形不再被悄悄收成单枚成员（闭合 `TR-EQUIV-SENTENCES`，本板新增 C-109）**（`claim/TR-EQUIV-SENTENCES-qoder` @ 基线 `02d83cedf`，动 `os/commands/usr-bin/textfilter/{src/tr.rs,tests/tr_face.rs}` + 18 篇 `todo.md` 与本板，单域三线 commands/textfilter，无跨界）：C 分两步判 `[=…=]`——`bracket()` 只认式内有没有一处 `=]`（`minix3/usr.bin/tr/str.c:193-199`），认过之后才由 `genequiv()` 逐枚核内部：成员一枚（遇反斜杠一次走过两枚字节）、然后必须 `=`、然后必须 `]`，两处各自 `errx(1, …)`（`:305-330`）。本壳此前只做第一步，两句一个没接，于是 `[=ab=]`、`[=abc=]`、`[=a b=]`、`[==]`、`[=a=b=]` 五形在本壳退 0 交出翻译结果、在参照件退 1 并报句子——P0-code-bug。对照 GNU 与 NetBSD 的 `tr`：两家都不核内部（前者只报 `invalid character class syntax`），这两句是 Minix3 自己的血脉，真值只认 `minix3/` 的 C 源。门：库测三枚（四形两串位钉等号句、`[=a=b=]` 钉右括号句、转义腿钉 `[=\=]` 报等号而 `[=\n=]`/`[=\t=]` 正常收成那枚字符），面门一枚（五形两串位共十次跑进程，钉退码 1、stdout 空、stderr 逐字节等于 `tr: <句子>`，并钉成形的 `[=c=]` 不退化）。**门第一次跑红红在我自己的期望**：我把 `[=c=]` 的成员当 `a`，参照件现量给 `abx`——改期望不改代码，与批次三百三十八「门红先问 C」同源。读数：`--lib tr::` 21/0、`--test tr_face` 15/0、宿主整包 1067/0（68 target）、**容器门 1067/0/0 退码 0**、三架构各 Finished、workspace 0 error、触及两件 rustfmt 零漂移、clippy 退码 0 且触及件零命中、A/B 22 次 SAME=22 DIFF=0、负控六枚全咬（两枚锚点第一次没命中：rustfmt 把长句折了行、而「取成员」那一形在等价腿与序列腿各有一处，改按真实排写取锚并用上一行 `let mut cursor = pos + 2;` 区分）。登记不执行：`TR-EQUIV-OOBCH-SET2` 仍在册（`string2` 用尽后参照件填 `\0`、本壳重复末字节，属 `tr.c` 的 OOBCH 腿，不是 `bracket()`/`genequiv()` 的判据）。**另两笔流程收获**：① 排期表 R 表的状态列与若干「下一支取 …」指针系统性滞后——本轮先取的 `SEQ-GETOPT-LINE` 早在批次三百二十闭合，`R10`/`R11`/`R12`/`R13` 复核后也都已闭合，真敞口是 `R16`（`fortune`、`random` 两支的 C `warn`/`err` 腿，`minix3/games/fortune/fortune/fortune.c:525`、`:705`、`:1056` 与 `minix3/games/random/random.c:97`、`:99`；`wtf` 那支是 sh 脚本、无 C 真源）与 `R17`（`truncate` 的 `relative_target` 用 `saturating_add` 而 C 是 `off_t` 自然回绕后钳零，`minix3/minix/commands/truncate/truncate.c:121-131`）；取项规矩加一条：**按 R 表取项之前先对代码现查**。② §7 那笔「待补的容器 `--workspace`」按字面写法跑不成，三条读数（`--workspace` 退 101、`--workspace --exclude minix-kernel` 同退 101、`-p minix-kernel --features mock` 退 0 且 835/0/12）已写进 §7：撞的是 `SD-30` 的形态绊线（设计行为），所以权威读数的形状是两条腿而不是一句全仓命令。

- `2026-10-18` qoder / todo 循环第五十四笔：**纯台账笔：把 `random` 一整支量清——五处操作数与句子的分叉，另加一处主体语义的分叉，并定出三片修法**（`claim/RANDOM-DENOM-LEGS-qoder` @ 基线 `b07c82555`，只动 18 篇 `todo.md` 与本板；生产码、门一字未动）。登记时的标题是「text-games 三面的 `err`/`errx` 腿」（排期行 R16），照 C 真源量完发现主洞更深：C 的 `random` 是**逐行过滤器**（`minix3/games/random/random.c:114-137`，`selected = (int)(denom * random() / RANDOM_MAX) == 0`，选中即 `putchar`），本壳读全输入、只保留最后一次命中的那一行、一行都没命中就印自创的 `random: nothing selected` 退 1。参照件现量六形（`tmp/random-342/random_ref`，垫片 `-D__COPYRIGHT(s)=`、`-D__FBSDID(s)=`、`-D__dead=`、`-DRANDOM_MAX=2147483647L`）：`abc` 与 `0` → `denominator is not valid.` 退 1；`1e999` → 三段形 `1e999: Numerical result out of range` 退 1；`1.5` → 退 0；`2 3` → 一行 usage 退 1；无操作数 → 退 0 且三行打出两行（`denom` 默认 2）；`random 1` → 三行全打。本壳对照的六处分叉、三片互不阻塞的修法（片1 个数与两句报文并给 `bin_support` 补 `warn_line`；片2 过滤器主体与 `-e`/`-r`；片3 分母改 `double` 语义含 `*ep != '\0'` 的整串判据）、以及「`wtf` 是 sh 脚本无 C 真源、不可按 C 接」与「`fortune` 另有三处 `warn` 需单支」全写在 18 篇该条。本笔开单 C-110 标 🔄，未动码，测试读数沿用上一笔（宿主与容器同为 1067/0、68 target）。

- `2026-10-18` qoder / todo 循环第五十五笔：**`RANDOM-DENOM-LEGS` 片1 落地——`random` 操作数那一步接到 C 的形状**（`claim/RANDOM-DENOM-LEGS-qoder` @ 基线 `417fd35e6`，动 `os/commands/games/text-games/{src/lottery.rs,src/bin_support.rs,src/bin/random.rs,tests/random_face.rs}` + 18 篇 `todo.md` 与本板）。判断住决定半（新增 `OperandLeg` 与 `judge_operands`，取 `&[&str]` 所以库侧不引 `alloc`），两句文本 `USAGE`（`usage: random [-er] [denominator]`，程序名写死在串里，`random.c:143-147`）与 `NOT_VALID_SENTENCE`（`denominator is not valid.`，句子里没有操作数，`:98-99`）也住决定半；三段形那支给 `bin_support` 补了 `prog_name`/`warn_line`（同 `textfilter/src/bin_support.rs:329-353`，号走 `TextGameError::OutOfRange.as_errno()` = 34）。顺带把读标准输入那一步从裸 `minix_sys::read` 换成 `read_stdin_error` 双生面——不换的话宿主上根本走不到过滤器（实测 `random: read error` 退 1），而「失手那句 `random: read error` 退 1」仍是自创、C 是 `err(2, "stdin")` 退 2，留在片2。门读数：宿主库测 25/0、端到端新门 `--test random_face` 6/0（本 crate 第一枚 `tests/` 门，改名调用那一枚钉住「`usage` 不缀程序名而 `errx`/`err` 缀」这一对差异）、**容器门退码 0**、三架构各 Finished、clippy 触及件零命中、rustfmt 触及四件零漂移；A/B 归一化两处承载差异后 SAME=8/DIFF=7，七形全是 `strtod` 取值域（归片3）；负控七枚全咬。**两次红都在自己的期望上**：`random 1` 断言了片2 才该管的「全行回声」；退码断言压在六行 × 分母 2 的「全不中」概率上（2 的 -6 次方）会随机咬空，改成 200 行钉退码并连跑八次零红。条目状态 **partially fixed**：C-110 的片1 销账，片2（过滤体、`-e`/`-r`、`err(2,…)` 两句与退 2）与片3（分母取值域）仍敞，本板下一笔可继续 `claim RANDOM-DENOM-LEGS`。

- `2026-10-18` qoder / todo 循环第五十六笔：**`TRUNCATE-RELATIVE-WRAP` 闭合（排期行 R17）——`truncate` 的相对尺寸加法从饱和改回 C 的带符号回绕**（`claim/TRUNCATE-RELATIVE-WRAP-qoder` @ 基线 `93eba32e3`，动 `os/commands/bin/fileops/src/truncate.rs` + 18 篇 `todo.md`、`06-file-ops.md` 与本板）。C 在 `minix3/minix/commands/truncate/truncate.c:122` 把 `sb.st_size + rsize` 直接写进带符号的 `off_t`，溢出即回绕到负侧，再由 `:131-132` 的钳位收成 **0**——所以 `truncate -s +9223372036854775807` 在 C 那里是把文件清空；决定半用的是 `clamp(current.saturating_add(delta))`，饱和停在 `i64::MAX`，于是同一句命令在本壳去截一个荒谬的大长度。`:123` 那句 `if (oflow < (sb.st_size + rsize))` 拿刚赋完值的变量与同一个来源式再比，恒假，不抄；可达的回绕只有正方向，因为 `st_size` 非负。门：宿主 `--lib` 392/0，新立 `a_positive_overflow_wraps_then_clamps_to_zero` 一枚，并**翻正一枚既有期望**（`relative_sizes_land_on_the_clamped_sum` 把 `relative_target(i64::MAX, 5)` 断成 `i64::MAX` 并注释「饱和加保住了钳位的答案」，那是登记在案的待定形进了门），负控五枚全咬（回绕改回饱和 → 2 红等，还原后 0 红，`tmp/truncate-344/`）；三架构各 Finished、clippy 触及件零命中、rustfmt 零漂移、容器门读数入 `tmp/truncate-344/container-gate.log`。落盘后果宿主测不到：`tests/truncate_face.rs` 篇头已写明宿主 `open`/`stat` 经传输缝只回 `EIO`，够不到真 `ftruncate`，端到端那一腿归可写根镜像上的 `C-57`/`E5`。

- `2026-10-18` qoder / todo 循环第五十七笔：**纯台账笔：`RANDOM-DENOM-LEGS` 片2 量成 2a/2b 两半并钉死四组门期望，其间判明「参照件在选项环措辞上不是真值承载」**（`claim/RANDOM-DENOM-LEGS-qoder` @ 基线 `f8d7bdd05`，只动 18 篇 `todo.md` 与本板；生产码一字未动，认领已释放、现场已清）。参照件十二形选项环读数在 `tmp/random-345/ab345-ref.log`：`-e`/`-r` 被接受、`--` 之后算操作数、裸 `-` 走 `denominator is not valid.`、`-e 2 3` 走 usage、集外字母打**两行**（先 `invalid option -- 'Z'` 缀 `argv[0]`，再 usage）退 1。关键在于那句 `invalid option` 来自宿主 glibc：Minix 自己的 `minix3/lib/libc/stdlib/getopt.c:103` 打的是 `unknown option -- %c`（缺值那一句在 `:134` 是 `option requires an argument -- %c`），都不带引号也不同字——与 `tr`/`jot` 那几支由 `GetoptReport` 单源交回的口径同源，所以 2b 的字符串要照 `getopt.c` 抄，参照件只用来确认行数、顺序与退码。2a 的改动范围与四组门期望（`random 1` 升到逐字节全等、200 行验保序子序列、分母 40 亿验空输出退 0、目录当标准输入验 `random: stdin: Is a directory` 退 2 且句子取 `minix-types` 的 `errno.rs:413`）都写在 18 篇该条；负控四枚的变异点也一并定死。本笔也记下两个流程坑：改测试文件前必须先按盘上（`rustfmt` 之后）文本取锚点，否则上一笔的格式化会让改动落空；`String::from_utf8_lossy(..).as_str()` 在本工具链撞 `str_as_str` 未稳定，要 `&*` 或先 `into_owned()`。**已写的 2a 代码与三门草稿在门跑绿之前被还原**（`git status --porcelain` 归零后才 `release`），因此条目状态保持 `partially fixed`，不是降级也不是完成。

- `2026-10-18` qoder / todo 循环第五十八笔：**`RANDOM-DENOM-LEGS` 片2a 落地——`random` 的过滤体接成 C 的一趟过流，两处自创报文删除**（`claim/RANDOM-DENOM-LEGS-qoder` @ 基线 `0ab605642`，动 `os/commands/games/text-games/{src/bin/random.rs,tests/random_face.rs}` + 18 篇 `todo.md`、`24-text-games.md` 与本板）。薄壳原来读全输入、只留最后一次掷中的那一行、一行都没掷中就打自创的 `random: nothing selected` 退 1、读失手打自创的 `random: read error` 退 1；C 是逐行独立掷、中者整行写出、环走完 `exit(0)`，失手那一路是 `err(2, "stdin")` 的三段形退 2（`minix3/games/random/random.c:126-137`）。决定半一字未动（`is_selected`、`mix_seed` 沿用）。门：宿主库测 25/0、端到端 9/0（补 3 枚：200 行验保序子序列、分母 40 亿验空输出退 0、目录当标准输入验 `random: stdin: Is a directory` 退 2；并把 `random 1` 从「非空且子集」升到逐字节全等）；**容器门退码 0、合计 34/0**；三架构各 Finished；clippy 触及件 0 命中；rustfmt 零漂移；负控四枚全咬（还原后 0 红，`tmp/random-345/ctl346.py.log`）。**本笔把一次自伤挡在合入之前**：为消一条 `mut` 告警我按同名文本一次删了两处 `mut`，其中测试帮助件里那处是必须的（`child.stdin.as_mut()`），宿主编不过、容器门退码 101，诊断后才修回——删 `mut`、删参数这类清扫必须按调用点逐处分清。敞着的仍是 2b（选项环，且那句集外字母的字样要照 Minix 自己的 `lib/libc/stdlib/getopt.c:103` 而非参照件的 glibc 字样）与片3（`strtod` 取值域），外加 `emit` 那一条共用写缝的退码（C 是 `err(2,"stdout")`），条目状态保持 `partially fixed`。

- `2026-10-18` qoder / todo 循环第五十九笔：**`RANDOM-DENOM-LEGS` 片2b 落地——`random` 的选项环按 Minix 自己的 `getopt.c` 接上**（`claim/RANDOM-DENOM-LEGS-qoder` @ 基线 `31f90d3a5`，动 `os/commands/games/text-games/{src/lottery.rs,src/bin/random.rs,tests/random_face.rs}` + 18 篇 `todo.md`、`24-text-games.md` 与本板）。环住决定半（新增 `Ring`/`scan_ring`/`UNKNOWN_OPTION`/`RANDOM_MAX`/`exit_status_value`），薄壳只渲染：集外字母先打 `random: unknown option -- Z`（`minix3/lib/libc/stdlib/getopt.c:101-104`，字母不带引号）再打不缀程序名的 usage，退 1；`--` 是结束标记、裸 `-` 反而留在操作数里（`getopt.c:81-93`）；`-e` 在读标准输入之前就把 `(denom * random()) / RANDOM_MAX` 当退码返回（`random.c:109-111`），`-r` 只关 stdio 缓冲。上一笔判明的「参照件在选项环措辞上不是真值承载」在这一笔兑现：字样照 libc，不照参照件那句 glibc 的 `invalid option -- 'Z'`。门：宿主库测 28/0、端到端 12/0（新立三枚）、容器退码 0 合计 40/0、三架构 Finished、clippy 触及件 0、rustfmt 零漂移、负控五枚全咬（`tmp/random-345/ctl347.log`）。两处红都红在我自己算错（`operands_at` 的下标口径、`3/4` 折算给 2 不是 3），规矩补进 18 篇：带整除的断言落笔前手算三个数。敞着的是片3 取值域与共用的 `emit` 写缝退码，条目状态 `partially fixed`。

- `2026-10-18` qoder / todo 循环第六十笔：**`RANDOM-DENOM-LEGS` 片3 落地——分母从 `u32` 换成 C 的那个 `double`**（`claim/RANDOM-DENOM-LEGS-qoder` @ 基线 `ba57665eb`，动 `os/commands/games/text-games/{src/lottery.rs,src/bin/random.rs,tests/random_face.rs}` + 18 篇 `todo.md`、`24-text-games.md` 与本板）。决定半新增 `Strtod`/`scan_strtod`/`match_fold`/`pow10`/`line_selected`，`OperandLeg::Accepted` 与 `NO_OPERAND_DENOMINATOR` 改 `f64`（只到 `PartialEq`：浮点没有全等关系），`exit_status_value` 按 C 的表达式在 `f64` 上算再向零截。真值取自本仓库自己的 libc 配置而不是照抄参照件：`gdtoa/gdtoaimp.h:191` 与 `:643-645` 开着 `INFNAN_CHECK`、`NO_HEX_FP` 全树未定义、`gdtoa/smisc.c:123-140` 的 `match` 折小写，故 `inf`/`Infinity`/`NaN` 都认、`infi` 只吃到 `inf`。门：宿主库测 28/0、端到端 18/0（新立 6 枚）、容器退码 0 合计 46/0（6 target）、对参照件 A/B 38 形 **36 同 2 异**（`tmp/random-348/ab348.log`）、clippy 触及件 0、rustfmt 零漂移、文风门对我件 error=0、断链 124 中我这两篇 0。**这一笔又抓到一枚「门钉实现不钉参照件」**：`an_overflowed_operand_uses_the_three_part_err_shape` 把 `4294967296`（参照件退 0）钉成三段形溢出、`a_rejected_operand_says_the_errx_sentence_without_naming_itself` 把 `1e3` 钉成「不是有效分母」，两处随本片翻正——第三百四十三、三百四十四、三百四十六之后第四次同形失误。敞着的：十六进制浮点 `0x…p…` 与 `nan(0x…)` 载荷、非有限值转整数那一格的 UB 不追平、超过 17 位数字串的末位单位残差、子正规是否立 `ERANGE`、共用的 `emit` 写缝退码；条目状态 `partially fixed`。

- `2026-10-18` qoder / todo 循环第六十一笔：**`RANDOM-DENOM-LEGS` 片3 收尾——十六进制浮点照 `gethex` 接上**（`claim/RANDOM-DENOM-LEGS-qoder` @ 基线 `6023d7cc9`，动 `os/commands/games/text-games/{src/lottery.rs,tests/random_face.rs}` + 18 篇 `todo.md`、`24-text-games.md` 与本板）。三条不同都取自源、不照参照件：①`gethex.c:110-112` 的 `switch (*s)` 没有 `default` 拒绝分支，故 `0x10` 认；②`strtod.c` 的十六进制 `switch` 只分 `NoNumber`/`Zero`、其余 `ULtod` 后直接 `goto ret`，**整支绕过 `range_err`**，故 `0x1p99999999` 是认下的 `inf`（退 0、一行不掷中）而不是三段形，`0x1p-99999999` 是 0 → 由 `denom == 0` 报「不是有效分母」；③`gethex.c:122-126` 指数位收不满就 `s = s1` 退回 `p` 之前。glibc 对②那两形立 `ERANGE`，A/B 的两处 DIFF 就是这个，属承载差异。实现新增 `scan_hex`/`hex_value`/`pow2`，13 位十六进制以内精确。门：宿主库测 29/0（新增 11 形读数表）、端到端 22/0（新立 4 枚）、A/B 19 形 17 同 2 异（`tmp/random-349/ab349.log`）、clippy 触及件 0、rustfmt 零漂移；容器门首跑退码 1 的原因查明是脚手架：`CARGO_TARGET_DIR` 指的宿主目录不存在时 docker 以 root 建它，容器内 `-u $(id -u)` 的 cargo 写不进（`Permission denied (os error 13)`），改用已存在的可写 target（`tmp/ci-target-348`）重跑，退码 0、合计 **51 通过 / 0 失败**（6 个 target，逐套读数在 `tmp/random-349/container-gate.log`）。**教训**：容器门跑前必须先确认 target 目录存在且属主是自己。敞着的：`nan(0x…)` 载荷、非有限值转整数的 UB 格、超 13 位十六进制的末位残差、子正规是否立号、共用的 `emit` 写缝退码；条目状态 `partially fixed`。

- `2026-10-18` qoder / todo 循环第六十二笔：**`RANDOM-DENOM-LEGS` 的 `nan(0x…)` 载荷落地——本支只复写「吃掉多少」**（`claim/RANDOM-DENOM-LEGS-qoder` @ 基线 `8f9faa58f`，动 `os/commands/games/text-games/{src/lottery.rs,tests/random_face.rs}` + 18 篇 `todo.md`、`24-text-games.md` 与本板）。`strtod.c:314` 的 `#ifndef No_Hex_NaN` 与 `hexnan.c:112-118` 那条向前找 `)` 的 pedantic 路在本仓库都是活的（两个宏只出现在 `gdtoaimp.h` 的注释和 `gdtoa/README` 里，从没 `#define`）。判「位样不可见」是这一笔的核心论证：`random` 只用 `denom == 0` 与那两处算式读分母，所有 NaN 都给同一答案，所以唯一可见的差别是 `ep`——`hexnan.c:105-109`/`:112-118` 命中 `)` 就吃到串尾，`:146-147` 落空就把游标留在 `(`。门：宿主库测 30/0、端到端 24/0、A/B 12 形 11 同 1 异（`tmp/random-350/ab350.log`：`nan( 0x1f )` 参照件退 1、本支退 0，C 在 `hexnan.c:77-78`/`:97-98` 允许载荷里的空白而 glibc 不允许，又一处「参照件不是真值承载」）、clippy 触及件 0、rustfmt 零漂移、容器读数见本节。至此取值域三片加载荷全闭，条目只剩四条登记在册的不追平与残差（非有限值转整数的 UB、超位数的末单位残差、子正规是否立号、共用的 `emit` 写缝退码），状态保持 `partially fixed`。

- `2026-10-18` qoder / todo 循环第六十三笔（**纯测量笔，生产码一字未动**）：**把 `SPLIT-STREAMING-MEMORY` 量成三片，并先指出流式化会改掉一处可见行为**（`claim/SPLIT-STREAMING-MEMORY-qoder` @ 基线 `c64db13f7`，只动 18 篇 `todo.md` 与本板，单域三线 commands/textfilter，无跨界）。现状锚点：`os/commands/usr-bin/textfilter/src/bin/split.rs:60-70` 把输入整个收进 `Vec<u8>`，`:76-90` 行模式、`:91-104` `-b`、`:105-` `-n` 全在**全量**上切；C 侧 `minix3/usr.bin/split/split.c:181-215` 的 `split1` 是 `char bfr[MAXBSIZE]` 加 `for (bcnt = 0;;) switch (len = read(ifd, bfr, MAXBSIZE))`——块满即 `bigwrite`，跨缓冲的半个块靠 `bcnt` 续上、**不需要拼接缓存**，`split2`（`:245-`）同构但切点是换行。三片：片A `-b`（唯一不需要留存面，先做）、片B 行模式（`split2` 的跨缓冲留存策略本笔没读，不写结论）、片C `-n`（大小已由 `fstat` 问来，流式化等于复用片A）。**开工前必须先查的门**：流式化会把「`read` 失手」那一形从「一块都不落盘再报错」变成「已落盘若干块再报错」，`tests/split_face.rs` 若有依赖前者的断言就又是「门钉实现不钉参照件」同族失误（三百四十八、三百四十九刚各抓过一枚）。条目状态保持 `partially fixed`；下一轮首动作是读 `split.c:245-320` 与 `MAXBSIZE` 定义处、现查那枚断言，再落片A。

- `2026-10-18` qoder / todo 循环第六十四笔（**负控轮，生产码一字未动**）：**给三百四十八至三百五十一的门补跑九枚变异控制，并当场修掉控制机自己的两处错**（`claim/RANDOM-DENOM-LEGS-qoder` @ 基线 `f40907343`，只动 18 篇 `todo.md` 与本板；脚本 `tmp/random-352/ctl352.py`、日志 `tmp/random-352/ctl352.log`，盘面 md5 `3c44b46623df469f0aff6ab3b5748900`，跑完 `git status --porcelain` 行数 0）。第一次读数 `咬住数 = 0 / 7` 是**裁判错了**：脚本要求六套 `test result:` 摘要全到达才算咬，而库测一红 cargo 即停、只剩 1 套——把「没看到红」当成了「没有红」，与三百四十四那次同族；判据改成「失败数大于零 + 无编译错 + 期望的那一枚门在变红名单里」之后同一批变异得 `8 / 9`。第二处错是把「四舍五入」写成 `.round()`：本 crate `no_std` 下它属 std，报编译错——编译错不是门有效而是变异不合法，换成加 `0.5` 再截断随即变红。唯一没按预期红的那枚（改掉 `NOT_VALID_SENTENCE` 句尾的点）先红了决定半那枚逐字节比对门 `the_two_hardwired_lines_keep_c_spelling`，端到端那枚没轮到跑：两条门都在、紧的那条在前。本笔覆盖 348—351 全部新门；**三百二十五~三百二十九那六枚旧负控欠账仍在台上**。`RANDOM-DENOM-LEGS` 状态不变（四条登记在册的不追平与残差 + 共用的 `emit` 写缝退码）；下一轮首动作 `tools/claim.sh claim SPLIT-STREAMING-MEMORY qoder` 落片A，预算不足则先清旧六枚。

- `2026-10-18` qoder / todo 循环第六十五笔（**负控轮，生产码一字未动，本轮零枚跑成**）：**试跑三百二十五~三百二十九那六枚旧负控失败，失败原因收成下一轮能直接用的锚点清单**（`claim/NEGATIVE-CONTROL-DEBT-qoder` @ 基线 `7954ba17c`，只动 18 篇 `todo.md` 与本板）。四处变异点按台账里引过的源码形状去定位，全不命中：`os/commands/usr-bin/textfilter/src/jot.rs` 搜不到 `(v as i64) as u64`；`bin_support.rs` 的 `args_os` 有两处（一处在文档注释）；`col.rs` 命中的是提到裸字节的**注释**、不是取字节的表达式；`column.rs` 的 `line too long` 不在我猜的模式上。**「没看到锚点」不等于「没有锚点」**：这与三百五十二那枚控制机「把没看到红当成没有红」是同一面，这次翻在我自己手上，所以不下「门没咬」的结论、也不凭猜改码。批次三百五十三逐处附了现查原文与行号，并写了重跑配方（反面替换 + `cargo test -p minix-textfilter` 在 ulimit 内 + 每枚期望变红的门名 + 跑前跑后各核一次 `git status --porcelain` 与 md5）。另记一笔流程实话：本批用的 claim 号 `NEGATIVE-CONTROL-DEBT` 是我自起的跨件负控占位号、不对应任何单一条目，只为拿排他树，用完即销。六枚旧欠账仍在台上；348—351 那九枚已按批次三百五十二清掉（8 枚咬住）。

- `2026-10-18` qoder / todo 循环第六十六笔（负控轮补记，生产码一字未动）：**三百二十五/三百二十七的两枚负控跑成并咬住**（`claim/NEGATIVE-CONTROL-DEBT-qoder` @ 基线 `6c45d9a1e`，动 18 篇 `todo.md` 与本板；日志 `tmp/negctl-353/controls.log`，跑完盘面 md5 未变、`git status --porcelain` 0 行）。锚点是 `os/commands/usr-bin/textfilter/src/jot.rs:362` 的 `(step as i64) as u64`（盘面唯一一处）：删掉「先按 i64 截断再按 u64 回绕」与把种子整体加一，两次都让 `seed_from_step` 那组单测变红——后者正是三百二十六登记为漏网的那一枚，三百二十七把转换式提到库侧就是为了让它能红。仍敞四枚：`bin_support` 宿主 args 腿（代码位点 `src/bin_support.rs:54`，48 行是文档注释，锚点要按行取）、`col` 取字节、`column` 残行计数、`split -b`；现查行号已在台账列出。下一轮首动作仍是先清这四枚，再动 `SPLIT-STREAMING-MEMORY` 片A。
- `2026-10-18` qoder / todo 循环第六十七笔（负控轮，生产码一字未动）：**三百二十五至三百二十九欠账里的四枚旧负面控制全部跑成并咬住**（`claim/NEGATIVE-CONTROL-DEBT-qoder` @ 基线 `e9e12fc7f`，动 18 篇 `todo.md` 批次三百五十四与本板；日志 `tmp/negctl-354/controls.log`，逐枚原始输出留 `tmp/negctl-354/raw-1.txt`~`raw-4.txt`，基线 `raw-0-baseline.txt`）。四枚各自咬在哪：`src/bin_support.rs:54` 退回 `std::env::args()` ⇒ `tests/bin_support_face.rs` 四枚全红；`src/col.rs:149` 取字节退回取字符 ⇒ `col::tests::unknown_nonascii_option_letter_reports_the_byte` 红；`src/bin/column.rs:85-88` 残行告警改成整趟只打第一句 ⇒ `each_source_with_a_long_tail_line_warns_once` 红（正是三百二十八那扇端到端门）；`src/split.rs:72` 每片多切一字节 ⇒ `split::tests::chunk_bytes_splits_fixed_and_last_short` 红。四枚跑完各自整文件还原、md5 与跑前逐一相符，最后 `git status --porcelain` 0 行。**本轮自己的错（同类第三面，已修对并机制化）**：第一次跑时四枚明明都红了，日志却写「没有咬」——我这次重写计数正则而没沿用三百五十二修好的那一套，把摘要行的匹配写成紧跟 `FAILED.` 之后，而 cargo 的实际形状是 `test result: FAILED. 24 passed; 1 failed;`，失败数取不到就恒为零。修法两条已落进 `tmp/negctl-354/ctl354b.py`：判据以变红测试名清单非空为主信号；并给裁判自己加一枚负面控制（同一套判据回一份已知变红的转录与一份全绿基线，本轮读数 `known-red red=4 / baseline red=0 / JUDGE OK`）。**两枚残留如实记着**：① cargo 默认在第一个变红套件处止步，所以 `col` 与 `split` 这两枚里台账点名的端到端门（`tests/col_face.rs:199`、`tests/split_face.rs:436`）本轮没被跑到，覆盖度既没证实也没证伪，下一轮用 `--test col_face` / `--test split_face` 单套件复跑；② `split` 没有任何门覆盖「先读成功若干次再读失手」这一形。顺带把 `SPLIT-STREAMING-MEMORY` 片甲的开工前置查完并写进批次三百五十四：`MAXBSIZE` 真源在 `minix3/tools/compat/compat_defs.h:1151`（`64 * 1024`），`split.c:175-236` 跨两次读只留字节计数器不留数据，`split.c:195-200` 的首个输出文件只在读到非零字节之后才打开——因此现有两枚读失手门（`tests/split_face.rs:490`、`:513`）断言的「一枚分片都不生成」在流式化之后仍然成立，三百五十一担心的「门钉实现不钉参照件」在这一形不成立。六枚欠账至此清了四枚（余二枚在三百五十三已跑成）；本批 claim 号仍是自起的跨件占位号，用完即销。
- `2026-10-18` qoder / todo 循环第六十八笔（负控轮补记，生产码一字未动）：**批次三百五十四残留①关掉，两扇端到端门单独跑各自那一套集成测试也都咬**（`claim/NEGATIVE-CONTROL-DEBT-qoder` @ 基线 `f40771202`，动 18 篇 `todo.md` 与本板；日志 `tmp/negctl-354/controls-suite.log`）。改坏 `src/col.rs:149` 后跑 `--test col_face` ⇒ `nonascii_option_letter_is_reported_as_one_byte` 红；改坏 `src/split.rs:72` 后跑 `--test split_face` ⇒ `byte_mode_is_untouched_by_the_line_rewrite` 红。两枚跑完整文件还原并核 md5 相符、`git status --porcelain` 0 行。同一套件里的 `the_split_reaches_the_chunk_files_end_to_end` 没红是应当的（它走行模式默认千行一档，不受定长切点影响）。至此三百二十五至三百二十九那六枚负面控制欠账全部清完；下一轮首动作 `claim SPLIT-STREAMING-MEMORY` 落片甲。
- `2026-10-18` qoder / todo 循环第六十九笔（`SPLIT-STREAMING-MEMORY` 片甲第一步）：**抓到并修掉一枚 P0 代码缺陷：`split -b` 把读腿收回来的失手号整块丢掉，退 0 而 C 退 1**（`claim/SPLIT-STREAMING-MEMORY-qoder` @ 基线 `04653c09f`，动 `os/commands/usr-bin/textfilter/` 的 `src/bin/split.rs` + `tests/split_face.rs` 与本板 + 18 篇 `todo.md` 批次三百五十五）。C 的 `split1` 在 `minix3/usr.bin/split/split.c:188-190` 是 `case -1: err(1, "read")`，而行模式与 `-n` 两支早已走本壳同一个 `finish(prog, read_failure)`（`src/bin/split.rs:146`），只有 `-b` 一支收尾写成 `support::terminate(0)`。补两枚门把 `-b` 的两形钉住：`byte_mode_blames_an_unreadable_operand_and_exits_one`、`byte_mode_blames_an_unreadable_stdin_and_exits_one`；套件从 23 枚到 25 枚全绿，变异控制（把收尾改回退 0）让这两枚双双变红。**本轮自己的坑**：还原变异用了 `git checkout --`，可修复本身还没提交，HEAD 那份是旧码，于是「还原」把修复一起抹掉、md5 报 MISMATCH，靠锚点重新落回才复绿——工作态未提交时，负控的还原源只能是改坏前留在内存里的原文，不能是 git。片甲的流式本体（分块读缝 + 只带一个已写计数器的切块函数）留在下一轮；「先读成功再失手」那一形仍无门可造，继续登记为覆盖空洞。
- `2026-10-18` qoder / todo 循环第七十笔（批次三百五十五收尾，容器门读数落地）：**`split -b` 那枚 P0 修复的容器权威门转绿**（`claim/SPLIT-STREAMING-MEMORY-qoder` @ 树内提交 `ed8660621`）。`docker run --rm -m 2g -u $(id -u):$(id -g) minix-ci:1.94 cargo test -j 1 -p minix-textfilter` 的`test result:` 摘要共 68 套，合计 `1069 passed / 0 failed`，退码 0，日志 `tmp/split-355/container-gate-2.log`。第一次跑退 101 与代码无关：`CARGO_HOME` 挂到了认领树的 `tmp/ci-cargo`，而 `tmp/` 不入库、该目录下没有这个缓存，Docker 建成 root 属主后写不进去——与批次三百五十记过的同一面，第二次改挂主树可写缓存即过。片甲的流式本体与「先读成功再失手」那一形的门仍敞着，见批次三百五十五。
- `2026-10-18` qoder / todo 循环第七十一笔（`SPLIT-STREAMING-MEMORY` 片甲第二步，落决定半不落半截缝）：**流式切块语义独立坐实，五枚新门 + 两枚变异控制都有牙**（`claim/SPLIT-STREAMING-MEMORY-qoder` @ 基线 `83b07ba58`，只动 `os/commands/usr-bin/textfilter/src/split.rs` 与本板 + 18 篇 `todo.md` 批次三百五十六）。`ByteCutter{size,filled,open}` 与 `CutPiece{bytes,opens_new_file}` 逐条对位 `minix3/usr.bin/split/split.c:196-236`（跨读只留 `bcnt` 计数器）、`:228-230`（正好切满即关档，下一次读属新的一枚）、`:221-230` 的 `if (len)`（一次跨多片不留零长度尾巴），`size == 0` 与 `chunk_bytes` 同口径。核心那枚等价门用 6 种片长乘 8 种读边界证明「按 `opens_new_file` 归档」与 `chunk_bytes` 一次切完逐字节相同——这就是下一轮接读缝时唯一要守住的式子。宿主 `--lib` 534 通过 / 0 失败（起点 529，+5）；rustfmt 零漂移；两枚变异控制分别让 4 枚与 2 枚变红，复原按批次三百五十五立的新规矩用**内存里的原文**而不是 `git checkout`，md5 `4a27f441…` 前后一致。本轮不动读缝与输出缝：两面都要改，还要把 `SPLIT-CLOSE-ERRLEG` 一并接上，留作下一轮首动作。
- `2026-10-18` qoder / todo 循环第七十二笔（`SPLIT-STREAMING-MEMORY` 片甲收口）：**两条缝接上，`split -b` 真的边读边写**（`claim/SPLIT-STREAMING-MEMORY-qoder` @ 基线 `abd308083`，动 `os/commands/usr-bin/textfilter/` 的 `src/bin_support.rs` + `src/bin/split.rs` + 18 篇 `todo.md` + 本板）。`bin_support` 新增 `stream_input`（双生面：宿主 `std::io::Read` / 真机 `minix_sys::read`，开档臂与读臂分开交回，已交出去的块不收回）与 `WriteSink::append`/`seal`；壳里 `split_bytes` 用批次三百五十六那枚 `ByteCutter` 驱动，**先关上一枚再开下一枚**并带上一枚的名字（对位 `split.c:309-335` 的 `close(ofd)` 与 `err(1,"%s",fname)`，也就是 `SPLIT-CLOSE-ERRLEG` 那一形），块大小取 C 的 `MAXBSIZE`（`compat_defs.h:1151`，`64 * 1024`），`-b` 的分岔挪到整档读之前。可见行为未改：`split_face` 25 枚门一字未改全绿、宿主 `--lib` 534 全绿；负面控制（`if part.opens_new_file` 改成永不起）让 `byte_mode_is_untouched_by_the_line_rewrite` 变红，复原用内存原文、md5 `d2be80d9…` 相符。**还欠一手**：「先读成功再失手」那一形现在语义对了但仍无门可钉（宿主造不出中途读错），登记为 `SPLIT-MIDSTREAM-READERR-GATE`，下一轮把读源抽成可注入闭合再补宿主单测。
- `2026-10-18` qoder / todo 循环第七十三笔（`SPLIT-MIDSTREAM-READERR-GATE` 闭）：**「中途读失手不许丢掉已读那块」现在有了门**（`claim/SPLIT-MIDSTREAM-READERR-GATE-qoder` @ 基线 `ce761a45e`，动 `os/commands/usr-bin/textfilter/` 的 `src/bin_support.rs` + `src/bin/split.rs` + 18 篇 `todo.md` + 本板）。办法是挪注入点而不是放弃断言：宿主读循环抽成对任意 `Read` 通用的 `drive_reads`，`stream_input` 宿主腿只负责选读源与折错误号，真机腿不动；门用脚本读源喂它（给两块、第三次报错），断言失手时缓冲里那 4 字节已交出且错误照报（C 的 `split.c:184-204` 边读边写在前、`err(1,"read")` 在后）。**本轮最该记的一条**：第一次我把读源设成每次正好给满一片，负控（删掉失手前的那次交出）没能让它变红——那条分支根本没走到；改成给不满一片后同一枚变异立刻让它变红，复原即复绿。又一次坐实「门跑绿不等于门有牙，每枚新门都得配一枚变异」。读数：`--bin split` 11 通过，整包 68 套 `1075 passed / 0 failed`，容器门同读数退码 0（`tmp/split-358/container-gate-2.log`），rustfmt 零漂移。
- `2026-10-18` qoder / todo 循环第七十四笔（`RANDOM-STDOUT-EMIT-RC` 闭）：**`random` 写标准输出失手现在按 C 报名字退 2**（`claim/RANDOM-STDOUT-EMIT-RC-qoder` @ 基线 `5114fdd47`，动 `os/commands/games/text-games/` 的 `src/bin_support.rs` + `src/bin/random.rs` + `tests/random_face.rs` + 18 篇 `todo.md` 批次三百五十九 + 本板）。C 的判据在 `minix3/games/random/random.c:131-133`（每行末查 `ferror(stdout)`，失手 `err(2, "stdout")`），改前写腿用的 `support::emit` 两面都是「失手即退 1、一字不报」，退码与句子两重分叉。修法选「添一条只交回号的缝」而不是「改共用缝」：`emit` 还被同 crate 的 `wtf.rs:90`、`fortune.rs:55` 用着，那两支的 C 写腿判据本批未核，改缝等于把 `random` 的口径套给别人；新缝 `emit_error` 的两面（`bin_support.rs:99`/`:116`）与同文件读腿 `read_stdin_error`、与 `textfilter` 那侧同名件同一分工。新门用 `/dev/full` 注入写失手、分母取 1 保证第一行必掷中：宿主 55 通过 / 0 失败（`random_face` 29→30），容器门退码 0 同读数 55/0，rustfmt 零漂移、clippy 对本包零命中。两枚变异（退回 `emit`、位词 `stdout`→`write`）各让新增那一枚独红，第一枚的失败读数 `(Some(1), "")` 就是那两处分叉本身（退 1、一字不报）；还原用改前的内存备份、md5 `58fa9432…` 前后一致。**留下的账**：`TEXTGAMES-EMIT-ERRLEG`——`wtf` 与 `fortune` 两支的写腿判据待去 C 源核，下一轮首动作。
- `2026-10-18` qoder / todo 循环第七十五笔（`TEXTGAMES-EMIT-ERRLEG` 现查，纯台账笔、生产码一字未动）：**两支的判据查到底，一支改标有前置、一支改判不适用 C 口径**（`claim/TEXTGAMES-EMIT-ERRLEG-qoder` @ 基线 `bff9b4101`，动 18 篇 `todo.md` 批次三百六十 + 本板）。`fortune` 的真源是 `minix3/games/fortune/fortune/fortune.c`，输出腿只有 `:272` `fputs(line, stdout)` 与 `:274` `(void) fflush(stdout)`，全文件没有 `ferror(stdout)`（那句在 `strfile/strfile.c:255`，属另一枚工具）⇒ 该对的是「写失手不可见也不改退码」，我们的 `emit` 多出一个退 1，分叉坐实、修法已定（换 `support::emit_error` 不接号）。**但本项不能当轮落码**：`src/bin/fortune.rs:20` 的 `use minix_sys::{open, read, Fd};` 没有宿主双生面，实测 `./target/debug/fortune /tmp/fdb.txt`（档存在且可读）报 `fortune: /tmp/fdb.txt: cannot open` 退 1，两枚试写的端到端门都红在开档臂、走不到写臂——新立前置 `FORTUNE-HOSTED-FILE-SEAM`。`wtf` 改判：真源 `minix3/games/wtf/wtf` 经 `file` 判为 POSIX shell script（`$NetBSD: wtf,v 1.21`），真值优先序的「C 源码行为」一格对它不适用，故不动码也不立项。试写的码改动与试验门全部撤回，树内 `git status --porcelain` 空行；试验读数留在 `tmp/textgames-360/host-test.log`。下一轮首动作 `claim FORTUNE-HOSTED-FILE-SEAM`。
- `2026-10-18` qoder / todo 循环第七十六笔（`FORTUNE-HOSTED-FILE-SEAM` 闭，`fortune` 写腿随门同批落地）：**`fortune` 第一次能在宿主跑起来，四枚端到端门立住，C 不查写失手那一形也钉死了**（`claim/FORTUNE-HOSTED-FILE-SEAM-qoder` @ 基线 `a19bf0dd0`，动 `os/commands/games/text-games/` 的 `src/bin_support.rs` + `src/bin/fortune.rs` + 新建 `tests/fortune_face.rs` + 18 篇 `todo.md` 批次三百六十一 + 本板）。前置的因是壳里把开档读档直接写成 `minix_sys::{open, read, close}`、没有双生面，宿主对真实存在的档报 `cannot open` 退 1；办法是在 `bin_support.rs` 加 `slurp_operand` + `OperandFailure{Open, Read}`（宿主腿 `std::fs`、真机腿逐块 `read` 且两条出路都 `close`），与同文件 `emit`/`emit_error`/`read_stdin_error` 的「缝交回哪条腿失手、句子归壳里定」同一分工，弃案是壳里内联 `#[cfg]` 分岔（要在 OS 语义层养两条 syscall 序列）。写腿按批次三百六十定好的修法同批换掉 `support::emit` 为 `let _ = support::emit_error(...)`：C 是 `fputs(line, stdout)`（`fortune.c:272`）+ `(void) fflush(stdout)`（`:274`），全文件没有 `ferror(stdout)`，退码只由 `:202`/`:377` 两条 `exit` 决定 ⇒ 写失手不新增一句话也不改退码。读数：宿主 `cargo test -j 1 --offline -p minix-text-games` **59 passed / 0 failed**（lib 30、`random_face` 25、`fortune_face` 4，`tmp/fortune-361/host-test.log`），容器门读数 `tmp/fortune-361/container-gate.log`，rustfmt 三件零漂移、clippy 对本包零命中；变异（写腿换回 `emit`）让 `/dev/full` 那一枚独红、读数 `("", 1)` 正是 C 不给的那个退 1，还原用改前内存备份、md5 `b2fc1b44…` 前后一致。**本轮自己的坑**：第二枚 Edit 想把一枚测试辅助函数删掉，锚点跨到下一枚门的函数签名，把整套测试吞坏——改结构一律整件重写，别用跨构件边界的局部替换。**留下的账**：`FORTUNE-OPEN-ERRLEG`（本壳那两句自创句形对 C 的三段形尚未齐平，下一轮首动作）、`TEXTGAMES-EMIT-ERRLEG` 的 `wtf` 一半可按「判据不适用」收尾（判据是 `file` 判 `minix3/games/wtf/wtf` 为 POSIX shell script）。
- `2026-10-18` qoder / todo 循环第七十七笔（`FORTUNE-OPEN-ERRLEG` 闭）：**`fortune` 开不了档那一行改成 C 的 `err(1, ...)` 整形状**（`claim/FORTUNE-OPEN-ERRLEG-qoder` @ 基线 `dc3a7d215`，动 `os/commands/games/text-games/` 的 `src/bin_support.rs` + `src/bin/fortune.rs` + `tests/fortune_face.rs` + 18 篇 `todo.md` 批次三百六十二 + 本板）。真判据 `minix3/games/fortune/fortune/fortune.c:1056` 的 `err(1, "Cannot open `%s'", fp->path)`：中间那位是句子代入用反引号与单引号包住的档名、尾巴是号对应的句子、退 1；上一批留下的 `fortune: <操作数>: cannot open` 三处都不对。改法是让缝把号交回来——`OperandFailure::Open` 从空变体改成带 `minix_sys::Errno`（宿主面取 `raw_os_error`、取不到按 `EIO`；真机面直接交回 `minix_sys::open` 的错误），壳里走 `warn_line(prog, "Cannot open \`<路径>'", code)`，前缀由 `prog_name(&argv)` 取。**读臂那句自创的 `read error` 本批不动**：C 的读腿走 stdio，`fortune.c` 里没有对应的 `err` 句形（`err(1, NULL)` 是 `:845` 的 do_malloc，`warn("Cannot open ...")` 在 `:525`/`:705` 是目录腿且不退场），硬造就是把猜测写成口径——登记 `FORTUNE-READ-ERRLEG`。读数：宿主 `cargo test -j 1 --offline -p minix-text-games` `59 passed / 0 failed`，容器门退码 0 同读数 59/0（`tmp/fortune-362/container-gate.log`），rustfmt 零漂移、clippy 对触及三件零命中；变异（中间那位换回 `cannot open`）让改名那枚独红、其余三枚不动，还原 md5 `aa4bad2e…` 相符。下一轮首动作 `claim SPLIT-NONUTF8-DROP`（P0 级，批次三百二十九收尾那条已备好改动清单与门的期望值）。
- `2026-10-18` qoder / todo 循环第七十八笔（`SPLIT-NONUTF8-DROP` 复测翻状态 closed）：**两半分叉经现量坐实都已在码里闭着，末行那一半补了一枚端到端门**（`claim/SPLIT-NONUTF8-DROP-qoder` @ 基线 `f24fe6c69`，动 `os/commands/usr-bin/textfilter/tests/split_face.rs` + 18 篇 `todo.md` 批次三百六十三 + 本板）。决定半是不碰解码的 `lines_bytes`（`src/split.rs:30`），C 的对应是 `split2` 的 `if (Cs < Ce) bigwrite(ofd, Cs, Ce - Cs)`——补不出一个字节。现量两条：`-l 2` 吃 `aa\n\xff\xfe\nbb\n` ⇒ 两行原样分片退 0；`-l 2` 吃 `aa\nbb` ⇒ 一片五个字节、末尾无换行。新门 `line_mode_does_not_invent_a_final_line_terminator` 钉后者那一形；非法字节那一半早有 `line_mode_keeps_undecodable_bytes`（`tests/split_face.rs:368`）、`-b` 那枚（`:437`）与决定半那枚（`src/split.rs:479`）三处覆盖，不重复造。读数：`--test split_face` **26 passed / 0 failed**、整包 **1076 / 0**、容器门退码 0、rustfmt 零漂移。**没跑成的负控如实记账**：末行那枚变异写成 `Box::leak` 直接编译不过（返回类型 `Vec<&[u8]>` 生命周期无解），所以本轮这枚门只算跑绿不算有牙，登记 `SPLIT-TAIL-NOEOL-NEGCTL`，下一轮首动作先补它。状态词：`SPLIT-NONUTF8-DROP` open → closed（分叉已闭 + 门在套件里跑绿，负控欠账单列不遮蔽）。
- `2026-10-18` qoder / todo 循环第七十九笔（`SPLIT-TAIL-NOEOL-NEGCTL` 闭）：**上一轮只跑绿的那枚末行门现在有牙了**（`claim/SPLIT-TAIL-NOEOL-NEGCTL-qoder` @ 基线 `05e9be678`，动 18 篇 `todo.md` 批次三百六十四 + 本板，生产码一字未动）。能编译的注入点选在写侧：`src/bin/split.rs:150` 的 `create_write(&path, &render_chunk(chunk_lines))` 改成结果无条件 `push(b'\n')`（等价于每个分片末尾补换行，正是 C 的 `split2` 尾段 `if (Cs < Ce) bigwrite` 不做的）。变异读数：`--test split_face` ⇒ **8 红 / 18 绿**，红名单含 `line_mode_does_not_invent_a_final_line_terminator`、`line_mode_does_not_invent_a_final_newline`、`line_mode_keeps_each_line_terminator_inside_a_multi_line_chunk`、`line_mode_keeps_undecodable_bytes`、`line_mode_makes_a_blank_line_its_own_chunk` 等（日志 `tmp/split-364/control1.log`）——这一族门确实在数终止符。还原用改前内存备份、md5 `5453999e…` 相符，复绿 26/0、整包 1076/0；同码的容器权威读数沿用批次三百六十三那一笔（退码 0、1076/0）。上一轮那句「只算跑绿不算有牙」的保留语随之销掉，`SPLIT-TAIL-NOEOL-NEGCTL` open → closed。下一轮首动作 `claim FORTUNE-READ-ERRLEG`。
- `2026-10-18` qoder / todo 循环第八十笔（`FORTUNE-READ-ERRLEG` 闭）：**`fortune` 的读腿不再自成一形——读断了就当读到尽头**（`claim/FORTUNE-READ-ERRLEG-qoder` @ 基线 `f51410091`，动 `os/commands/games/text-games/` 的 `src/bin_support.rs` + `src/bin/fortune.rs` + `tests/fortune_face.rs` + 18 篇 `todo.md` 批次三百六十五 + 本板）。判据是批次三百六十四读清的：`fortune.c:268`/`:293` 用 `fgets(...) != NULL` 装载，整个文件没有一次 `ferror`（`grep` 只命中 `fgets` 两处、输出腿 `:1312` 与三处取长度的裸 `read`），所以 C 把读失手与 EOF 同看：停止装载、不报名字、不改退码；我们以前那句 `read error` 加退 1 两处都是自造。改法是取消那一臂而非换措辞：`OperandFailure` 只剩 `Open(Errno)`，`slurp_operand` 两面都把读断当 EOF，壳里的 Read 分支删除。新门 `an_operand_that_opens_but_cannot_be_read_is_an_empty_database` 用目录作操作数（宿主开得了、`read` 回「是一个目录」），只钉「与空库同形」而不猜 C 的绝对句子。读数：宿主 60 passed / 0 failed（`fortune_face` 4→5），容器门 `tmp/fortune-365/container-gate.log`，rustfmt 零漂移；变异把读断改回致命（复用 `Open` 变体，避开上一轮 `Box::leak` 编不过那个坑）⇒ 五枚里独这枚红，还原 md5 `8331389e…` 相符、复绿 60/0。**留下的账**：`FORTUNE-EMPTY-ERRLEG`（空库那句仍自创，需先编参照件量 `fortune <空档>` / `fortune <目录>` 的三样读数），下一轮首动作。
- `2026-10-18` qoder / todo 循环第八十一笔（`FORTUNE-EMPTY-ERRLEG` 现查，生产码一字未动、状态仍 `open`）：**这一支卡住的不是措辞，是两条形而不同的前置**（`claim/FORTUNE-EMPTY-ERRLEG-qoder` @ 基线 `9cee84dbb`，动 18 篇 `todo.md` 批次三百六十六 + 本板）。第一件，参照件造不出来且因由可重跑：`gcc -I minix3/include` 撞 `__COPYRIGHT`/`__RCSID` 未定义（`fortune.c:37`/`:45`）与 `minix3/include/stddef.h:38` 要的 `sys/featuretest.h`；补上批次二百六十四那套带尾等号的 `-D'__COPYRIGHT(x)='` 再 `-include minix3/include/sys/cdefs.h` 仍 `No such file`——本仓 vendor 的 `minix3/include/` 只有半套头（`ls minix3/include/sys/cdefs.h` 与 `sys/featuretest.h` 双双 No such file）。第二件更要紧：C 压根不接受一枚纯文本档当格言库，读档前置是配对的 `.dat`（`open_dat` 的 `err(1, "Cannot open \`%s'", fp->datfile)`，`:1064-1068`）与定界串 `STR_ENDSTRING(line, fp->tbl)`（`:268-269`），而本支 Rust 壳是按行切格言、取模抽取的**重新设计过的简易过滤器**（没有 `.dat`、没有 `%` 定界、没有概率表）。⇒ 「空库那句齐平成 C」不是措辞问题：要么承认外部形状不同（那是 `Refactor`/`[ARCH]` 级裁决，文档+设计+代码三处要一致），要么先把 `.dat` 那套数据格式实现出来。据此把前置拆成两条可各自立项的账：`FORTUNE-DAT-FORMAT`（需设计裁决）、`BSD-HEADER-COMPAT-REF`（按批次二百六十七/二百六十九的补位面手法做）。不做的事也写明：不换状态词、不把那句随手改成我以为像 C 的句子、不拿宿主壳输出冒充参照件读数。下一轮首动作 `claim REV-STDIN-ERRLEG`。
- `2026-10-18` qoder / todo 循环第八十二笔（`REV-STDIN-ERRLEG` 的账面复测，生产码一字未动、不翻任何状态词，`claim/REV-STDIN-ERRLEG-QODEL-CHECK-qoder` @ 基线 `c61bbd57b`）：上一轮写的「下一轮首动作」指向一条**早已闭合**的账——批次三百三十一（`2026-10-17`，18 篇 `todo.md:1291`）就把 `rev` 的标准输入读错腿接上了 C 的 `ferror` 形状，总账 `TODO-LEDGER-OPEN.md` 也查无此条（`grep -n "REV-STDIN-ERRLEG" rewrite-notes/coordination/TODO-LEDGER-OPEN.md` 读数空）。复测三件（L1）：代码腿 `os/commands/usr-bin/textfilter/src/bin/rev.rs:51-56` 走 `read_stdin_error()`，先出已读到的行、再报流名 `stdin` 并置退码 1（对位 `minix3/usr.bin/rev/rev.c:101-104`，流名初值 `:80-81`、被操作数顶掉 `:91`）；门腿 `tests/rev_face.rs:180` 与 `:216` 各一枚，把「报流名」与「报操作数名」分成两条车道；本轮现跑 `cd os && cargo test --offline -j 1 -p minix-textfilter --test rev_face` → **10 passed / 0 failed** 且退码 0（专属树与主树各一次，`tmp/rev-367/rev_face.log`、`tmp/rev-367/rev_face_maintree.log`）。另登记一处滞后而不处理：18 篇 `:3` 的状态摘要仍是 `2026-10-08` 快照 `9f752834c` 的「未闭合 `open` 2 + 占位 5：命令面尚未接真实系统调用返回」，`TODO-LEDGER-OPEN.md` §6 的「18 命令」行照抄它，需一次逐壳清点才翻得动——单轮改摘要等于拿本轮口径覆盖别轮的账。据此立规矩：**「下一轮首动作」必须现查后再写，写前 `grep` 总账与阶段账**。下一轮首动作 `tools/claim.sh claim BSD-HEADER-COMPAT-REF qoder`（批次三百六十六拆出的两条前置里不依赖设计裁决的那条，补位面手法照批次二百六十七/二百六十九；`FORTUNE-DAT-FORMAT` 需人裁决，按 `open` 挂着不降级）
- `2026-10-18` qoder / todo 循环第八十三笔（容器门在线复跑，生产码一字未动，`claim/CI-GATE-ONLINE-RESTORE-qoder` @ 基线 `a72782be5`）：把「宿主与容器 DNS 两侧不通，只能 `--offline` 回退」这句环境账改成十秒能判的门面。分层读数：宿主 `https://index.crates.io/config.json` → 200、`https://static.crates.io/` → 403（桶根目录禁列举的正常码，最容易被误读成断网）；容器内同两条同码，`nameserver` 与宿主同为 `10.255.255.254`（`wslinfo --networking-mode` = `nat`）；决定性一层是容器 `getent ahosts` 把 `static.crates.io` 解到 **198.18.1.89**、`index.crates.io` 解到 **198.18.1.88**，`198.18.0.0/15` 即透明代理的假 IP 段 ⇒ 容器与宿主共用同一枚代理面，代理一停（WSL 重启后没跟上）两侧同时哑火，批次三百三十一的 `curl` 28 与「HTTPS 000」就是这一形。秒判配方：`docker run --rm minix-ci:1.94 sh -lc 'curl -sS -o /dev/null -w "%{http_code}\n" --max-time 10 https://index.crates.io/config.json'`，200 走在线门、000 或退码 28 去修宿主代理，**不改门配方、不拿 `--offline` 冒充在线门**。两枚在线读数（容器内全新 `CARGO_HOME=/tmp/ch`、`-u $(id -u):$(id -g)`）：`-p minix-text-games` 退码 0、**60 passed / 0 failed**（7 段）；`-p minix-textfilter` 退码 0、**1076 passed / 0 failed**（68 段）；日志里 `Downloaded bitflags v2.11.0` 这行是在线腿真跑的铁证（该 crate 不在任何挂载缓存）。另纠正交接件一处不实：`.wt/rev-stdin-errleg-qoder`(3.5G) 与 `.wt/docgen2-01-doc01iter2`(144K) 的 `stat -c %U` 读数皆 `xzhao` 而非 root，`claim.sh sweep` 判**需核对**（`os/target` 之外 1528 与 3 个文件），故不提权不删除，逐文件对账留到删除那一轮；仍需提权的只剩 `tmp/ci-target-349/340/365` 三枚 4.0K 空目录。下一轮首动作 `claim BSD-HEADER-COMPAT-REF`
- `2026-10-18` qoder / todo 循环第八十四笔（负控轮，生产码一字未动，`claim/NEGATIVE-CONTROL-DEBT-qoder` @ 基线 `7c372deff`）：批次三百五十三留下的四枚敞账全部跑成并咬住——A `os/commands/usr-bin/textfilter/src/bin_support.rs:56` 丢 lossy（`--test bin_support_face` 退码 101、5 枚红、命中 `no_command_aborts_on_a_nonutf8_option_letter`）、B `src/col.rs:149` 改按解码 char（`col_face` 2 枚红、命中 `nonascii_option_letter_is_reported_as_one_byte`）、C `src/column.rs:188` 残行不计数（`--lib` 2 枚红、命中 `column::tests::test_unterminated_tail_line_is_dropped_and_counted`）、D `src/split.rs:73` 片长偏一（`--lib` 3 枚红、命中 `split::tests::chunk_bytes_splits_fixed_and_last_short`）。每枚注入前取 md5、跑完整文件还原复验、再复跑同一目标确认基线归零；还原后该树 `git status --porcelain` 零输出、宿主整包 **1076 passed / 0 failed**（68 段，`tmp/negctl-369/host-after-restore.log`）。**控制机自身错两处**：① 按裸名比对而 cargo 报带模块前缀的全名，一度把 C、D 误判成「未咬住」（同一份日志里明明写着 `FAILED`）；② 把 `---- 名字 stdout ----` 详情行算进变红名单，读数虚高成 30。另 B 的第一版注入 `String::from_utf8_lossy` 在 `no_std` 件里编不过（`error[E0433]`，`src/col.rs:150`），控制机判「未成像（注入编不过）」而不是「门没咬」——判据第一条一次挡假阳性、一次挡假阴性。六枚旧欠账（三百二十五~三百二十九）到此全部清空，跨件占位号 `NEGATIVE-CONTROL-DEBT` 可销。下一轮首动作 `claim BSD-HEADER-COMPAT-REF`
- `2026-10-18` qoder / todo 循环第八十五笔（`.wt` 残骸回收，生产码一字未动，`claim/WT-DEBRIS-RECLAIM-qoder`）：`claim.sh sweep` 长期判「需核对」的两棵树做了逐文件三方对账（与 `git ls-tree -r HEAD` 的 blob、与主树同路径工作副本的 blob 两个基准比）。`rev-stdin-errleg-qoder` 的 1528 个非缓存文件：与 HEAD 同文 1502、与主树现版同文 1507、比主树旧且不同 20，只剩 `os/Cargo.lock` 两头不中——而 `.gitignore:78` 正是 `.wt/`，构建生成物本不进历史 ⇒ 无在途工作。`docgen2-01-doc01iter2` 三件全部比主树现版旧（`01-boot-shim-bootstrap.md` 树内 77177 字节 / `10-09`，主树 265300 字节 / 今天 `14:22`，`diff` 4119 行），主树那份是活的。24 个「两头都不中」的文件先抄进 `tmp/wt-rescue/reclaim-20261018/`（560K）再 `rm -rf`：`docgen2` 整棵消失，`rev` 树非 target 文件清零，但 `os/target/debug/incremental/` 的 `.o` 与 `work-products.bin` 报 `Permission denied`——**目录属主不等于条目属主**，正是 C-103 那句「不带 `-u` 的容器把缓存写成 root 属主」的现场遗留，剩余需人执行 `sudo rm -rf .wt/rev-stdin-errleg-qoder`。**对账路上自错三次**：`rev-list --objects --all --use-parent-stage` 的旗标凭空（命令失败名单为空→假报 26 枚独有）、`hash-object --stdin-paths` 批量输出与文件表错位（假报 docgen2 三件全独有）、只看 `stat -c %U` 就断言不需 sudo（方向反了）。立规矩：回收判据=逐文件三方对账 + 按条目查 root 属主。另批次三百六十八「皆 xzhao 无需 sudo」那条纠正只有一半对，本笔纠回。下一轮首动作 `claim BSD-HEADER-COMPAT-REF`
- `2026-10-18` qoder / todo 循环第八十六笔（`SPLIT-STREAMING-MEMORY` 片乙收口，行模式真的边读边写）：**读完 C 的 `split2` 才定下方案：跨读只要一个行计数器和一枚「当前分片还开着没有」的布尔位，不需要任何拼接缓存**（`claim/SPLIT-STREAMING-MEMORY-qoder` @ 基线 `f64b46af4`，动 `os/commands/usr-bin/textfilter/` 的 `src/split.rs` + `src/bin/split.rs` + `src/bin_support.rs` + `tests/split_face.rs`、18 篇 `07-text-filter.md` 与本板与 A 板 A3 行）。判据出处是 `minix3/usr.bin/split/split.c:242-282`：缓冲尾部那半行按 `if (len) newfile(); else file_open = 0;` 直接写进还没关的分片，所以留在两次读之间的只有行数与开档位。实现分两半：决定半是 `LineCutter{numlines,lines,open}` 与它的 `take`（与片甲的 `ByteCutter` 同一形，切点与片段末重合时不另起一枚），做工半把 `split_lines` 从「先收完全量再切」换成按 `MAXBSIZE`（`compat_defs.h:1151`，`64 * 1024`）成块读、逐块交回片段直接写文件。等价门两枚用形状集合乘片长，证明「按 `opens_new_file` 归档」与整档一次切完 `chunks_bytes(&lines_bytes(data), numlines)` 逐字节相同；可见行为的新形「读失手时前面的分片已经在盘上」由时序门钉住。**读数**：宿主 `cargo test --offline -j 1 -p minix-textfilter` 68 套 `1085 passed / 0 failed`（起点 1076，净增 9：库 8 枚加壳侧 1 枚，进程级 2 枚新增、2 枚随旧渲染件退场），三架构裸机 `cargo build --bins`（`x86_64-unknown-none`、`aarch64-unknown-none`、`riscv64gc-unknown-none-elf`）全绿，`cargo clippy` 触及件告警数改前改后都是 154 未增。**前置另立一笔**：批次三百五十八把宿主读源与读循环写成 `std` 而无 `cfg` 门，实测挡住全部 30 余枚命令裸机构建（九处报错都落在这两件），修复在 `9d892b46c`，与本片本体分开提交以便单独回退。**四枚变异控制各自咬住**（注入编译通过、失败数大于 0、指定门出现在变红名单）：删切点后清开档位 3 枚红、删缓冲尾部那段 14 枚红、删切点行数清零 8 枚红、切点恒报「另起一枚」4 枚红，还原后复跑失败数 0。**两处控制机自己的坑值得单独记**：① 变异写入与还原若落在同一秒内，cargo 的改动指纹认为源码没变而复用变异体产物，于是「还原后仍报两枚红」——判据是日志里必须有 `Compiling minix-textfilter`，且每次落盘后等满两秒再跑；② `cargo test` 默认在第一个失败的测试目标段停下，注入库内代码时只有库段的门会变红，壳侧与进程级根本没跑，看似的「只咬两三枚」其实是没覆盖，必须加 `--no-fail-fast`。**还敞着的**：片丙（`-n` 按 `fstat` 规模切）尚未做，它可以直接复用片甲与片乙已经落好的这块机器；详细论证与逐条锚点在 18 篇 `todo.md` 的批次三百七十二（`rewrite-notes/18-stage-commands/todo.md:122`）。
- `2026-10-18` qoder / todo 循环第八十七笔（`BSD-HEADER-COMPAT-REF` 闭，A 板 A1 行；**本轮只动工具面，`os/` 生产码一字未改**）：批次三百六十六两次想编 `minix3/games/fortune/fortune/fortune.c` 都失败（一次缺 `__COPYRIGHT`/`__RCSID` 的展开形，一次被 `minix3/include/stddef.h:38` 要的 `sys/featuretest.h` 卡死），本轮把补位面按符号拆成四处并收进一条构建配方（`claim/BSD-HEADER-COMPAT-REF-qoder` @ 基线 `2a6ebb4be`，提交 `bc4598315`，动 `tools/atf-c-compat/host-ref/` 的 `sys/cdefs.h` + `sys/endian.h` + `bsd-compat-proto.h` + `bsd-progname.c`、新增 `tools/build-bsd-ref.sh`、18 篇 `todo.md` 批次三百七十三、A 板 A1 行与本板）。三条设计判据各挡一次坑：① **绝不 `-I minix3/include`**，那半套 vendored 头与宿主头互相指错，所以 `sys/featuretest.h` 根本不必造；② 补面用 `#include_next` 接住 glibc 的 `<sys/cdefs.h>`，另写一份等于替 glibc 决定 `__THROW`/`__nonnull` 这些内部拼写；③ `getprogname` 带 `__GLIBC_PREREQ(2, 40)` 判据，宿主 `Ubuntu GLIBC 2.39` 没这枚函数（隐式声明把指针截成 `int`）、容器 `Debian GLIBC 2.41` 有（无条件定义撞 duplicate symbol）。BSD 的 `struct dirent` 有 `d_namlen` 而 glibc 没有，**但没改 C 源也没包装 `readdir`**：`fortune.c:186` 自带 `#ifndef NAMLEN`，配方用 `-D'NAMLEN(d)=((size_t)strlen((d)->d_name))'` 走源码自己的守卫进去，与批次二百六十四的 `-D'__COPYRIGHT(x)='` 同类。**读数**：两支参照件 `-Wall` 零警告链出，九形绝对读数全取到（`tmp/bsd-ref-373/readings.log`，`bash tmp/bsd-ref-373/shapes.sh` 可重跑）。**最要紧的一条新事实**：空库那一形（`strfile` 报 `There were 0 strings`）在 C 里既不报 `empty database` 也不报 `Cannot open`，而是 `random() % str_numstr` 除零当场 `Floating point exception(core dumped)`、退 **136**、两路输出全空（锚点 `fortune.c:956`、`:1007`）——本壳 `fortune.rs:60` 打的正是那句 `empty database`，于是 `FORTUNE-EMPTY-ERRLEG` 那个「要不要齐平成 C 的 `Cannot open`」的问题**前提被这组读数否证**；改判属 `Refactor`/`[ARCH]` 级，本轮只摆事实不动状态词。**本轮自己的两个坑**：① `strfile` 单参数把 `.dat` 追加在整个输入文件名之后（`poke.txt` → `poke.txt.dat`），第一轮造库全落空还当是 `strfile` 失败；② 驱动脚本把 stderr 捕获文件写成绝对路径 `/tmp/...` 而目录在仓库 `tmp/` 下，重定向失败让九形齐刷刷读成「退 1 / 全空」——整齐的失败是控制机坏了，不是被测件行为。**下一轮首动作**：`bash tools/claim.sh claim CMD-LEDGER-RECOUNT qoder`（A10 台账卫生，清点读数本轮已备好：113 枚 bin、0 占位壳）。
- `2026-10-18` qoder / todo 循环第八十八笔（`CMD-LEDGER-RECOUNT` 闭，A 板 A10 行；纯台账笔，`os/` 一字未改）：**逐壳清点把 18 篇第一行那句状态摘要重立成现数**（`claim/CMD-LEDGER-RECOUNT-qoder` @ 基线 `f28847e10`，提交 `349066507`，动 `rewrite-notes/18-stage-commands/todo.md` 的 `:3` 摘要行与新立批次三百七十四、A 板 A10 行与本板）。旧摘要「未闭合 `open` 2 + 占位 5：命令面尚未接真实系统调用返回」双重滞后：占位那半在总账 `TODO-LEDGER-OPEN.md:149` 那轮实测（68 枚 bin、0 占位壳）就被否证过一次，本轮第二次否证且 bin 数已涨到 **113**；「尚未接真实系统调用返回」那半被 `bin_support` 的双生读缝（宿主 `std::io::Read` / 真机 `minix_sys::read`）与各壳的 `err(3)` 三段形否证。**新读数三条**：`find os/commands -path '*/src/bin/*.rs' | wc -l` → 113；小于 1 KB 的壳 0、`unimplemented!`/`todo!` 各 0；真实缺口是 15 枚 bin 里 **23 处选项级 `not wired`**，摊在 6 个 crate（`bin/fileops` 6、`bin/proctools` 3、`usr-bin/textfilter` 2、`usr-bin/doctools` 2、`usr-bin/compress` 1、`sbin/mountinfo` 1）。**两条判断写法上的规矩**：① 摘要改完不留「未闭合 `open` N」这种自造整数——逐行状态归 A 板 §1/§1b、跨阶段项归总账，本账再写整数就是越权；② 「按行数判占位」这条旧判据会误伤 `os/commands/bin/fileops/src/bin/true.rs` 与 `false.rs`（各 30 行，而 C 的 `true`/`false` 本来就是一句话的程序），**短不等于空壳**。**本轮自己的坑**：`grep --include` 的模式是对**文件名**求值、不是对路径求值，第一次写 `--include='*/src/bin/*.rs'` 数 `not wired` 得 0，差点被当成「命令面已无未接线报文」的假结论；换成 `find … -exec grep -l … {} +` 才是 15 枚 / 23 处——凡靠 `--include` 收窄的计数，先单独验它匹配到了东西，否则「零」这个读数毫无意义。**不代改的东西**：总账 §6 那行照抄旧摘要这件事按并发协议 §2（那本账编排线自持，多写者必覆盖）留给编排线，本笔只把行号 `:133`/`:149` 与失配事实留在账里。**下一轮首动作**：`bash tools/claim.sh claim COLRM-ERRLEG-ORDER-UNOBSERVABLE qoder`（A8 收尾型，按「判据不适用」收尾并保留证据，与 A9 的 `wtf` 半同路子）。
- `2026-10-18` qoder / todo 循环第八十九笔（A8 `COLRM-ERRLEG-ORDER-UNOBSERVABLE` 现查：**A 板给的收尾口径被否证，本行改挂人裁决**，生产码一字未动，`claim/COLRM-ERRLEG-ORDER-UNOBSERVABLE-qoder` @ 基线 `4040e610c`，本笔提交 `a307714b5`；动 `tools/atf-c-compat/probes/colrm_leg_order.py`（新增）、`os/commands/usr-bin/textfilter/tests/colrm_face.rs` 的一枚门注释、18 篇 `todo.md` 批次三百七十五、A 板 A8 行与 §6、本板）。A8 原写「判据在宿主不可观测」，判据出处是批次三百三十二留的门注释：「宿主造不出『有字节在飞、同时读也出错』的承载」。本轮去造这个承载：pty 的 master 端在 slave 关闭后**先交已缓冲的字节、下一口才回 `EIO`**，正是一根宿主给的真读源，写洞取 `/dev/full`——载体造出来了，那句断言因此不成立。四形矩阵读数（C 参照件 `minix3/usr.bin/colrm/colrm.c` 用上一轮的 `tools/build-bsd-ref.sh` 现编，对本壳）：①小输出＋读坏 → C `stdin`/退 1，本壳 `stdout`/退 1；②小输出＋读净＋写坏 → C **退 0 且一字不报**，本壳 `stdout`/退 1（**连退码都不一致**）；③④大输出两形两侧同形。**当年为什么会判成不可证**：他们只试了「stdin 是目录」那一形，那里读进零字节、输出也是零字节，而空写永不失败，反转顺序读数一模一样——分辨不出是真没有分叉，还是载体选错了。**为什么仍然不当场修**（这是本轮真正的结论）：甲案「把读腿提到写前」能对齐形①却**破坏形③**（那一形 C 报 stdout，它的写错是环内刷缓冲时撞上的，先于读失手）；乙案「改成边读边写」在小输出形②仍会当场撞见写错并退 1，而 C 那里是静默退 0——C 的沉默来自 libc 只在缓冲攒满时才真写。要四形全对只能在命令层复刻 stdio 的缓冲阈值与策略：Minix 侧 `BUFSIZ`＝1024（`minix3/include/stdio.h:182`）、非 tty 按 `BUFSIZ` 建缓冲、且**字符设备走行缓冲**（`minix3/lib/libc/stdio/makebuf.c:168-178` 的 `*couldbetty = S_ISCHR(...)`，`/dev/full` 恰是字符设备），这与本项目第一原则「抽象语义，不描述机制」正面冲突；不抄则两形外部行为含退码与真源不符。影响面不止 colrm——同 crate 另有 34 支壳用静默的 `support::emit()`（批次三百三十二现查读数），故按族定规才有意义。**落地清单**：① 探针入册 `tools/atf-c-compat/probes/colrm_leg_order.py`，它同时是裁决后的验收矩阵；② 那枚门注释里被否证的半句就地纠正并写明维持现状的理由，断言本身未动；③ A8 行改标 `⏸ 等人裁决`、§6 新增 `PD-COLRM-STDIO-FLUSH-ORDER` 摆出甲乙两案与后果（不降级、不硬造门）。**读数**：`cd os && ulimit -v 3145728; cargo test --offline -j 1 -p minix-textfilter --test colrm_face` → **16 passed / 0 failed**；矩阵全文在 `tmp/colrm-374/matrix_readings.log`。**本轮自己的坑**：第一版先往 slave 灌满载荷再起子进程，pty 缓冲很小，写方直接堵死——必须先起子进程再喂。**下一轮首动作**：`bash tools/claim.sh claim TEXTGAMES-EMIT-ERRLEG qoder`（A9 的 `wtf` 半，收尾型：真源 `minix3/games/wtf/wtf` 经 `file` 判为 POSIX shell script，「C 源码行为」这格对它不适用）。
- `2026-10-18` qoder / todo 循环第九十笔（A9 `TEXTGAMES-EMIT-ERRLEG` 的 `wtf` 半闭，**顺路抓到两枚让 `wtf` 对真库完全失效的 P0**，`claim/TEXTGAMES-EMIT-ERRLEG-qoder` @ 基线 `fbf09c577`，本笔提交 `3eefc6c51`，动 `os/commands/games/text-games/` 的 `src/acronym.rs` + `src/bin/wtf.rs` + 新建 `tests/wtf_face.rs`、18 篇 `todo.md` 批次三百七十六、A 板 A9 行与新增 A9b 行、本板）。**A9 的答案换了个说法**：不是「判据不适用」而是「判据不归这个程序」——脚本自己没有任何写失手处理逻辑，报文与退码都由解释它的 shell 给；宿主实测（`/bin/sh -> dash`）`ACRONYMDB=<库> sh minix3/games/wtf/wtf nasa >/dev/full` 回 `wtf: 79: echo: echo: I/O error` 而**退码 0**（句子是 dash 内部形，不是 `err(3)` 三段形；`rv` 只由「查无此词」置），真机上解释它的是 Minix 的 shell，又是另一套句子 ⇒ 不建门、不改本壳口径。**但把 `wtf` 真跑起来就露出两枚 P0，而它以前没有端到端门，所以一直没被发现**：甲＝`wtf.rs:22` 写死 `use minix_sys::{open, read, Fd};`，宿主上**真实存在**的库档也报 `wtf: <路径>: cannot open` 退 1（与批次三百六十一给 `fortune` 治的同一枚，那笔只治了一支），接现成的 `support::slurp_operand` + `OperandFailure{Open}`，并去掉本壳自创的 `{db}: read error` 句——脚本那侧是 `fgrep … 2>/dev/null`，库读断退化成「查无此词」而非另一句报文（`wtf:76-77`），这条缝的既有语义本就是「读断当读到尽头」；乙＝`parse_acronym_line` 用 `split_once(':')`，而真库 `minix3/share/misc/acronyms` 是**制表符**分界（读数：`10Q\tthank you`，557 行里含冒号只有 **1** 行，还是 `$NetBSD:` 标识行），脚本取释义用 `sed -ne "^$target[[:space:]]*||p"` 同样按空白 ⇒ 按冒号断词等于**对真库一条都解不开**（实测改前 `wtf -f <真库> NASA` 回 `nothing appropriate` 退 1），改 `split_once(char::is_whitespace)`，库内三枚单元测的夹具随之从 `LOL: …` 改 `LOL\t…`（夹具在复述错的口径，第二次误导人），且空白断词后 `term.contains(whitespace)` 成不可达死码一并删。**补的门**：`tests/wtf_face.rs` 5 枚（命中形一枚同时是甲乙两枚缺陷的守卫）+ 库内一枚直接读真库读数的 `the_shipped_acronym_database_is_readable_and_resolves`。**读数**：宿主 `cargo test --offline -j 1 -p minix-text-games` → **66 passed / 0 failed**（lib 30→31、`random_face` 25、`fortune_face` 5、`wtf_face` 新增 5）；容器权威门同读数 **66/0**（8 个 target 段、退码 0；在线腿 `curl …/config.json` 容器内回 **200**，所以是权威门不是 `--offline` 降级）；三架构 `cargo build --bins` 全退 0（本轮删的正是真机腿的直接 syscall 用法，这一面必须验）；`cargo clippy --all-targets -p minix-text-games` 改前改后都 **13** 条告警未增；rustfmt 零漂移——**注意本包吃 workspace edition**，板上配方写的 `--edition 2021` 会把 `acronym.rs:46` 那枚 let-chain 报成 `error: let chains are only allowed in Rust 2024 or later`，别照抄。**两枚变异控制各自咬住**（`tmp/wtf-376/negctl.py`，三条硬判据齐）：N1 把分隔符退回冒号 → 5 枚红（预期库内三枚全中，另带进程级两枚）；N2 让宿主开档腿必然失败 → 6 枚红（预期三枚 `wtf_face` 全中，**并把 `fortune_face` 的三枚一起带红，恰好证明两支壳共用这条缝**，也说明 N2 是缝级变异）；还原用内存原文、md5 一致、复跑失败数 0。**新立的账 A9b `WTF-SCRIPT-PARITY`**：真源是脚本时它**自有的句子**仍是判据，四处不同形逐条列在板上（大写化的命中形、`cannot open acronym database file \`%s'`、`I don't know what \`%s' means!`、以及本壳把未知选项当操作数咽掉——实测 `-o nasa` 报的是默认库开不了而非坏选项）。**本轮自己的坑**：`cargo fmt -p` 会把同包内**没碰的四件**（`adventure.rs`、`monop.rs`、`bin/fortune.rs`、`bin/random.rs`）一并格式化造出无谓 churn，已撤回；**格式化只点文件，不要 `-p`**。**下一轮首动作**：`bash tools/claim.sh claim RANDOM-DENOM-LEGS qoder`（A12 板上写 `🚫` 而台账最新一笔写 `partially fixed`，先对账这一格再定它该是什么，不默默降级）。
- `2026-10-18` qoder / todo 循环第九十一笔（A12 `RANDOM-DENOM-LEGS` 对账闭合，**板上 `🚫` 与账上 `partially fixed` 两条都不准**；纯台账笔，生产码一字未动，`claim/RANDOM-DENOM-LEGS-qoder` @ 基线 `e2aefd803`，动 18 篇 `todo.md` 批次三百七十七、A 板 A12 行、本板）。登记的四条敞口对出两处滞后：④（共用 `bin_support::emit` 写缝退码）**早已闭合**——批次三百五十九把 `os/commands/games/text-games/src/bin/random.rs:113-114` 换成 `support::emit_error` 并由这一支自己定句子与退码，判据是 C 的 `if (ferror(stdout)) err(2, "stdout")`（`minix3/games/random/random.c:131-133` 本轮实读复核），门在 `tests/random_face.rs:456` 的 `a_stdout_that_stops_taking_bytes_is_blamed_by_name_and_exits_two`；剩下 ①—③ **本来就不是待办**，三条各带可复核出处：① C 那一格是未定义行为，x86-64 的 `cvttsd2si` 给 `0x80000000` 而 aarch64 的 `fcvtzs` 饱和，没有单一真值可追；② 末位单位残差来自不做 gdtoa 的大数精化、只按「有效数字 × 10^指数」乘出，只可能挪动「恰好压在阈值那一格」的选中判定；③ 子正规立号与否是 gdtoa `Avoid_Underflow` 的配置差异。⇒ 取值域三片与 `nan` 载荷早已落地、④ 闭合、①—③ 以三行「登记不追」继续留名（留名是为了不让后来者当缺口重做），条目按六值词表记 **`closed`**。这**不是「改成 DEFERRED 充数」**：闭合依据是每一条都有出处，而不是本轮新造理由；另现查 `grep -n "RANDOM-DENOM-LEGS" rewrite-notes/coordination/TODO-LEDGER-OPEN.md` 读数空，本条不在总账，状态权威就在阶段账，不存在越权改总账。**A12 行内已把「三条理由的出处 + ④ 由谁闭合」写全**，后来人不必再翻两处行号。**下一轮首动作**：`bash tools/claim.sh claim SPLIT-N-STDIN-FSTAT-SIZE qoder`（A6 行第一枚，按板上写定顺序逐个领）。
- `2026-10-18` qoder / todo 循环第九十二笔（A6 拆行后第一枚 `SPLIT-N-STDIN-FSTAT-SIZE` 追到底，**结论是这一格在仓库内没有真值来源**；生产码一字未动，`claim/SPLIT-N-STDIN-FSTAT-SIZE-qoder` @ 基线 `e55d58135`，动 `tools/atf-c-compat/probes/split_n_stdin_size.sh`（新增读数件）、`tools/atf-c-compat/host-ref/sys/param.h` + `sys/stat.h`（新增补面头）+ `bsd-compat-proto.h`（补一行 include）、18 篇 `todo.md` 批次三百七十八、A 板 A6 拆成 A6a/A6b/A6c、本板）。批次三百三十登记这条时只说「真值取决于 Minix 自己管道 `fstat` 的读数」就停在“登记保留”，本轮把这句话追到可证的边界：C 的 `split3` 在读任何字节之前就 `fstat(ifd)` 并拿 `sb.st_size` 判能不能切成这么多片（`minix3/usr.bin/split/split.c:290-300`），而 Minix 的 `do_fstat` **自己不算任何字段**，只 `req_stat(rfilp->filp_vno->v_fs_e, rfilp->filp_vno->v_inode_nr, ...)` 转发给该 vnode 所属的文件系统（`minix3/minix/servers/vfs/stadir.c:185-187`）；管道的 vnode 归 **PFS**，而 **PFS 不在本仓 vendor 的树里**（现查 `ls minix3/fs` 无输出，`find minix3 -path "*pfs*" -name "*.c"` 只命中无关的 tcpdump 打印件，VFS 全仓 `grep -rn "st_size" minix3/minix/servers/vfs/*.c` **零命中**）。⇒ 「管道 stdin 的 `st_size` 是 0 还是已缓冲字节数」在仓库内无从判定。**反向线索一条**：VFS 关掉最后一个读者/写者时会 `truncate_vnode(vp, vp->v_size)` 并注释「Tell PFS about latest pipe size」（`minix3/minix/servers/vfs/filedes.c:494-500`），说明 Minix 自己给管道记着尺寸 ⇒ 那一格很可能是「已缓冲字节数」，那它就是**竞态语义**，本壳无论怎么改都不可能在启动时对齐；这条读法已写进读数件结尾的判读规则，免得下一轮重新推。**交出的仪器**：`tools/atf-c-compat/probes/split_n_stdin_size.sh` 四形（管道已灌 200 字节 / 管道慢写者 / 规则文件操作数 / 字符设备），宿主侧已验通——glibc 形 1 与形 3 都报 `split_ref: can't split into more than 0 files` 退 **1** 且不产物，形 2 退 0，与批次三百三十记的分叉一致（全文 `tmp/split-378/host_readings.log`）。**不硬造门**的理由写进台账：这一形的判据要等真机读数，现在钉只能钉住我猜的那一种语义，钉错就把错的语义固化成回归基线——比不钉更坏。**顺路把 A1 的补位面补到 `split` 编得过**：缺的两枚 BSD 常量按「谁提供就补在谁身上」归位——`MAXBSIZE`（`split.c:179`/`:249` 定读缓冲）进新头 `host-ref/sys/param.h`，值取本仓 vendor 的真源 `minix3/tools/compat/compat_defs.h:1150-1151` 的 `(64 * 1024)`（与片甲/片乙落 Rust 侧同一个数）；`DEFFILEMODE`（`:332` 建档模式）进新头 `host-ref/sys/stat.h`；而 `split.c` 只含到 `<sys/param.h>`/`<unistd.h>` 就直接调 `fstat`（NetBSD 的包含链替它做掉这层），所以在 `bsd-compat-proto.h` 补一行 `#include <sys/stat.h>`。回归两支参照件（`split`、`fortune`）都 `-Wall` 零告警链出。**这处补面一落，A4/A5/A5b 那条「报文形逐字节对参照件」的判据第一次真的可执行**。**A 板处理**：按 §0 的领取办法把三枚 ID 挤一行的 A6 拆成 A6a（`⏸ 等 C 板真机窗`，等的是读数不是裁决）/A6b/A6c（各 `☐`）。**下一轮首动作**：`bash tools/claim.sh claim SPLIT-CLOSE-HOST-VISIBILITY qoder`。
- `2026-10-18` qoder / todo 循环第九十三笔（A6b `SPLIT-CLOSE-HOST-VISIBILITY` 现查闭合；纯现查笔，生产码一字未动，`claim/SPLIT-CLOSE-HOST-VISIBILITY-qoder` @ 基线 `28250fe6f`，动 18 篇 `todo.md` 批次三百七十九、A 板 A6b 行与新立 A6d 行、本板）。批次三百三十那句「宿主腿看不见 `close` 失败未修」把两件事混成一件，分开之后各有结论。**一、响应逻辑宿主测得到**：`support::SinkFailure::Close(code)` → `ChunkWrite::Close(code)`（`os/commands/usr-bin/textfilter/src/bin/split.rs:185`）、退码取号 `:202`、**报的档名是刚写完那一枚分片**（`:213` 的 `ChunkWrite::Open(_) | ChunkWrite::Close(_) => path`），对得上 C 的 `else if (close(ofd) != 0) err(1, "%s", fname);`（`minix3/usr.bin/split/split.c:321-322`）；钉它的两枚门 `the_sink_arms_land_on_their_own_chunk_arms`（`:406`，防 `Close` 被并成 `Write`）与 `the_two_write_arms_do_not_swap_their_labels`（`:420`，`Write` 报流名、`Open`/`Close` 报档名，两形不许互换标签）都在宿主跑，现跑 `cd os && cargo test --offline -j 1 -p minix-textfilter --bin split` → **10 passed / 0 failed** 且两枚点名 `ok`。所以「未修」若指响应逻辑，这句不成立。**二、载体那一半确实造不出，原因具体**：宿主腿走 `std::fs::File`（`bin_support.rs:409-413`），它由 drop 关闭且**不交回任何号**，std 没有「关闭并取错」的公开接口（`sync_all` 交回的是冲刷错，与 `close` 错不等价，ENOSPC 虽多挂在下刷上但 close 失败另有来源）。要造得出只有两条路且都越界：引一个能调 `close(2)` 的外部依赖（现查本 workspace **无 `libc`**：`grep -rn libc os/*/*/Cargo.toml` 零命中，这棵树里也没有 `os/Cargo.lock`；动依赖面与锁按 §2 属共读共享件），或在 `os/libs/` 造宿主 `close` 缝（B 板领地）。两条都不划算，因为**出货那侧本来就把号交回并报了**（真机腿 `bin_support.rs:467-471`/`:477` 的 `minix_sys::close(self.fd).map_err(SinkFailure::Close)`，`SPLIT-CLOSE-ERRLEG` 已闭）。按本行原写口径「宿主看不见的那几形按判据写明、不硬造门」结。**为什么宁可不补门**：补一枚就得为造错搭一个可注入的假 closer，那钉的是我自己的注入件而不是 `split` 的行为——与批次三百七十三「门跑绿不等于门有牙」同一类教训。**顺带查出一件该单独一笔的事并立为 A6d `SPLIT-HOST-FLUSH-AT-SEAL`**：宿主 `seal()` 现在是 `Ok(())`（`bin_support.rs:439-441`），即宿主跑出的 `split` **最后一枚分片的 close 错被无声吞掉**；真机不会这样，但宿主 A/B 对账会因此得到「宿主多产一片而参照件报错」的假分叉，`tools/atf-c-compat/probes/split_n_stdin_size.sh`（上一笔立的读数件）将来在宿主跑同形就会撞上。改法是给宿主腿加一次显式冲刷并把冲刷错按 `Close` 报，牵连 `append`/`seal`/`finish` 三处与既有门，故另立一行，本笔不动。**下一轮首动作**：`bash tools/claim.sh claim UNIQ-WRITE-ERRLEG qoder`（A6c）。
- `2026-10-19` qoder / todo 循环第九十四笔（A6c `UNIQ-WRITE-ERRLEG` 闭，**`uniq` 的输出腿从不查写失手，而本壳两形都报**，本笔落码修掉；`claim/UNIQ-WRITE-ERRLEG-qoder` @ 基线 `9f1b3cae7`，本笔提交 `a10f32a10`，动 `os/commands/usr-bin/textfilter/` 的 `src/bin/uniq.rs` + `tests/uniq_face.rs`、`tools/atf-c-compat/host-ref/bsd-fgetln.c`（新增）+ `bsd-compat-proto.h`、`tools/build-bsd-ref.sh`、18 篇 `todo.md` 批次三百八十、A 板 A6c 行与新立 A6e 行、本板）。登记语「写腿只跟了签名、报文形状未动」说轻了：C 的 `uniq` 只在 `file()` 开档失败时报 `err(1, "%s", name)`（`minix3/usr.bin/uniq/uniq.c:216-221`），`fprintf` 返回值一律不看，全文件 `ferror`/`fflush` **零命中** ⇒ 写失手既不多一句话也不改退码；参照件实测 `uniq_ref < in.txt >/dev/full` 退 **0**、stderr 空。本壳改前两形都错：① 无输出操作数、stdout 挂 `/dev/full` → 静默但**退 1**（走 `support::emit`，它的语义就是替调用方退场）；② 输出操作数写不进 → 印一句 C 世界里不存在的**两段形** `uniq: /dev/full` 并退 1。修法 `bin/uniq.rs:107-126` 两条出路都改成「失手咽下、末尾 `terminate(0)`」，关键是**用 `emit_error` 不用 `emit`**，与批次三百六十一给 `fortune` 定的同型判据（`Rewrite` 档）；开不了那一形本来就对（`:86` 的 `warn_line` 三段形 + 退 1，实测 `uniq: <只读目录>/out: Permission denied`），未动。**门与负控**：`uniq_face.rs` 新增两枚（13→**15 全绿**）；M1 把 stdout 那形改回失手即退 1、M2 把 sink 那形改回失手即退 1，两枚各咬一处（失败数各 1、指定门命中、日志含 `Compiling minix-textfilter`、还原用内存原文 md5 一致、复跑 0）。**读数**：整包 `cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` → **1087/0**（起点 1085，净增即这两枚）；三架构 `cargo build --bins` 全退 0；`clippy --all-targets -p minix-textfilter` **154** 与基线同数未增；rustfmt 触及两件零漂移（`bin_support.rs:228` 那处既存漂移不属本支，未顺手改）。**参照件为本支而扩**：`uniq.c` 的行读入口是 BSD 的 `fgetln(3)`，glibc 从未有这个名字（`undefined reference to 'fgetln'`），新增 `host-ref/bsd-fgetln.c` 照 BSD 三条语义补位（返回串**不** NUL 结尾、长度**含**换行、缓冲归流所有调用者不许 free——用一枚复用的静态缓冲即等价，因为 uniq 拿到指针立刻比较/写出不跨调用缓存），带 `#if defined(__GLIBC__)` 判据故在别的 libc 上整件退化成空翻译单元；`build-bsd-ref.sh` 从写死一枚改成**链上 `host-ref/bsd-*.c` 全部**，以后添函数级补面不必再改脚本；`uniq`/`fortune`/`split` 三支参照件回归 `-Wall` 零告警。**新立 A6e `BSD-GETOPT-DASH-OPTSTRING`（一处载体限制，卡的不是本支）**：`uniq.c:77` 的 optstring 是 `"-cdf:s:u"`，BSD `getopt` 用 `-` 表示「非选项原样交回」（对应 `:79-81` 的 `case '-': --optind; goto done;`），而 **glibc 的 `-` 语义是「非选项按字符码 1 交回」** ⇒ `case '-'` 永不命中、落到 `case '?'` → `usage()`；实测 `uniq_ref 输入`、`uniq_ref 输入 输出`、`uniq_ref -c 输入` 全部 usage 退 1，只有 stdin 形能跑。所以**宿主参照件带任何文件操作数都跑不动**，A2/A11 的 `getopt` 族与本板多条「报文形逐字节对参照件」判据的文件操作数那一半都卡在它上面；本轮输出操作数那形的真值因此取自源码缺省行为（`ferror` 零命中）而非参照件读数，这点在台账里写明。**下一轮首动作**：`bash tools/claim.sh claim BSD-GETOPT-DASH-OPTSTRING qoder`。
- `2026-10-19` qoder / todo 循环第九十五笔（A6e `BSD-GETOPT-DASH-OPTSTRING` 闭，**顺手把上一轮自己的归因纠正**；工具面一笔，`os/` 生产码一字未动，`claim/BSD-GETOPT-DASH-OPTSTRING-qoder` @ 基线 `ce4793e48`，动 `tools/atf-c-compat/host-ref/bsd-getopt.c`（新增）+ `bsd-compat-proto.h`、18 篇 `todo.md` 批次三百八十一、A 板 A6e 行与 A6c 行的归因句、本板）。上一轮立 A6e 时写的归因是「BSD getopt 用 `-` 表示非选项原样交回，glibc 却按字符码 1 交回 ⇒ `case '-'` 永不命中」。方向对，**但真值不用去手册里找，本仓就有**：`minix3/lib/libc/stdlib/getopt.c:63-143` 是 Minix 自己 libc 的实现，本轮逐行读完，语义三条——① **非选项就停**（`if (optind >= nargc || *place++ != '-') { place = EMSG; return (-1); }`，既不重排 argv 也不把非选项当选项交回）；② optstring 里那个 `-` 只管**孤零零一枚 `-`**（`:85-96` 的 `if (strchr(ostr, '-') == NULL) return -1; optopt = '-';`，`uniq.c:79-81` 的 `case '-': --optind; goto done;` 正是为这一形写的，不是为「非选项交回」写的）；③ 其余与经典 BSD 版一致。⇒ glibc 的偏离其实有两处：交回字符码 1（撞 `case '?'`）**加上它会重排 argv**。补面因此按 Minix 那份照抄而不是按手册抄：新件 `host-ref/bsd-getopt.c` 带 `#if defined(__GLIBC__)` 判据（别的 libc 上整件退化成空翻译单元），复用 glibc 的 `optind`/`optarg`/`optopt`/`opterr` 四个全局、只换扫描算法，改名走 `#define getopt bsd_getopt` 以避免与系统符号纠缠；`build-bsd-ref.sh` 上一轮已改成「链上 `host-ref/bsd-*.c` 全部」，这枚补面零改动即生效。**六形现跑读数**：两操作数退 0 出 `a|b|c|`；`-c` 带文件操作数出 `   2 a|   1 b|   2 c|`（宽度 4，与本壳 `%4d` 同形）；输出档开不了 `uniq_ref: <路径>: Permission denied` 退 1；**输出档写不进（`/dev/full`）退 0 一字不报**；stdout 写不进退 0；坏选项 `uniq_ref: unknown option -- Z` 加 usage 退 1。**两处收获值得单记**：① 上一轮 A6c 只能从源码缺省行为推断的那一格（`ferror`/`fflush` 全文件零命中 ⇒ 写不进不改退码），这一轮**直接量出来了**，推断与实测一致，`a10f32a10` 的修法得证——这也是补这面墙的价值：以前「宿主参照件跑不动文件操作数」，凡是报文形落在操作数上的判断都只能靠读源码；② 坏选项那句从 glibc 的 `invalid option -- 'Z'` 换成了 Minix 自己的 `<prog>: unknown option -- %c`（`getopt.c:101-104`），A2/A11 那族 `getopt` 报文形的判据**从此才算可对**，此前对的是宿主 libc 的句子，那种比对无效。回归：`uniq`/`split`/`fortune`/`colrm` 四支参照件全部 `-Wall` 零告警；`split` 两形抽查与批次三百七十八的读数不冲突（那次报 `0 files` 的是管道 `st_size` 那一形，不是 getopt 形）。**下一轮首动作**：`bash tools/claim.sh claim CKSUM-HASH-DIGEST qoder`（A4；拦路件已清，逐字节对参照件这条路第一次真能走）。
- `2026-10-19` qoder / todo 循环第九十六笔（A6d `SPLIT-HOST-FLUSH-AT-SEAL` **由提出者本人现查撤销**，生产码一字未动，`claim/SPLIT-HOST-FLUSH-AT-SEAL-qoder` @ 基线 `dd5e5d764`，动 18 篇 `todo.md` 批次三百八十二、A 板 A6d 行、本板）。批次三百七十九立新行时我写的修法是「给宿主腿加一次显式冲刷并把冲刷错按 `Close` 报，ENOSPC 类错正是在冲刷时浮现」——动手前先验，**验否了**：一枚三十行 `rustc` 小探针对 `/dev/full` 跑出 `write_all -> Err(Os { code: 28, kind: StorageFull })`，紧随 `flush -> Ok(())`；写错在写那一刻就已交回，`flush` 什么都不知道。根因是宿主落点为 `bin_support.rs:394` 的裸 `std::fs::File`（Rust 里**不缓冲**，`flush` 是 std 默认空实现，全文件 `BufWriter`/`LineWriter` 零命中）——「冲刷时才浮现」是 **C 的 stdio** 那一侧的模型，我把两边混了。⇒ 宿主真正不可见的仍只有 `close(2)` 自身（延迟分配 ENOSPC、NFS 提交失败那一类），而 std 全程不给拿它的号；这**与 A6b 是同一条载体限制的两个名字**，两条越界出路（引能调 `close(2)` 的外部依赖——现查本 workspace 无 `libc`、树内也没有 `os/Cargo.lock`；或在 `os/libs/` 造宿主 close 缝——B 板领地）已在 A6b 写明并结掉。**处置**：A6d 翻 `🚫 维持登记但不排期`（不是 `☐`，也不删条目——删了会让人以为已经解决），理由与读数写进行内，并指明批次三百七十九那句修法以本笔为准（旧段文字按规矩不回头改，过程痕迹留在新条目）。**副产物**：探针顺带证实 `uniq: /dev/full` 那句是**写**失败被误报成 `Close` 语义，与批次三百八十一从参照件量到的「写不进退 0」不矛盾；真机腿确实在交回 `close` 的号（`bin_support.rs:467-471`，走 `minix_sys::close` 而非 std），那句「真机那条腿按 `close_outcome` 报」不受本笔影响。**教训值得抄进以后每一轮**：提修法之前先花三分钟把「我以为的模型」拿去跑一遍，尤其是跨语言/跨 libc 的边界（Rust 无缓冲 File vs C stdio 有缓冲）。**下一轮首动作**：`bash tools/claim.sh claim CKSUM-HASH-DIGEST qoder`（A4）。
- `2026-10-19` qoder / todo 循环第九十七笔（**补写**：批次三百八十三那轮收尾时提交信息写了「认领板补第九十七笔」而本板实际未落行，现按 18 篇 `todo.md` 批次三百八十三补齐这条，不复述过程——细节以那一条为准）。A9b `WTF-SCRIPT-PARITY` 闭，`os/commands/games/text-games/src/bin/wtf.rs` 与它的真源 `minix3/games/wtf/wtf`（POSIX shell 脚本）逐形对齐五形：命中打印跟大写化走、查无此词印操作数原样那句 ``wtf: I don't know what `zzz' means!``、库开不了印 `wtf: cannot open acronym database file `<路径>'`、usage 走**标准输出**再退 1 且文本补回 `[-o]`、未知选项与裸 `-f` 走 usage 而不是自创句子。**方法上的收获**：真源是脚本且能直接跑，所以每一形都是量出来的而不是读出来的；脚本对 `-Z` 多印的那句 `Illegal option -Z` 是 **shell 的 getopts 印的**，不是 wtf 的，故不复创。超出行定义的四条另立 A9c（`ACRONYMDB` 环境变量、`-o` 的通配展开、三级退化查找 `whatis`→`pkg_info`→make help、`is` 词只移开头那一枚）。读数：宿主与容器各 **66 / 0**、三架构 `--bins` 全退 0、rustfmt 零漂移、**触及路径 clippy 告警 0 条**；由此定下一条方法——`cargo clippy` 的告警总数会随「这次重编了哪些 crate」变，拿总数当棘轮会自己骗自己，**要按告警位置是否落在本次触及件判**（本轮就是这么差点把库里既存告警算成自己添的）。基线快照 `f39e57f58`，提交 `bf9194d72`（码）+ `5b45c116b`（账）+ `6960a1a56`（提交号回写补笔）。
- `2026-10-19` qoder / todo 循环第九十八笔（A4 `CKSUM-HASH-DIGEST` **第一片**，八项里的三族；`claim/CKSUM-HASH-DIGEST-qoder` @ 基线 `6960a1a56`，动 `os/commands/usr-bin/textfilter/{Cargo.toml,src/digest.rs,src/cksum.rs,src/lib.rs,src/bin/cksum.rs,tests/cksum_face.rs}` + 18 篇 `todo.md` 批次三百八十四、07 篇 cksum 行、A 板 A4 行、本板）。**动手前先查树里有没有**：`os/libs/minix-crypt` 已经有 `md5.rs`（`Md5::new`/`update`/`finalize` 对位 C 的三个入口）与 `sha1.rs`，所以本笔**不在命令面重译 MD5/SHA1**——同一 workspace 放两份 MD5 只会让人去问「哪一份是真的」，而这个问题本来不该存在；SHA-2 那一族在 `minix-crypt` 里整个缺席，于是 SHA256 按 `minix3/common/lib/libc/hash/sha2/sha2.c` 的 `SHA256_Transform` 新译，`K256[64]` 与初值表用脚本从源文件**机械摘出**、不按素数小数公式重算（重算会把「抄错」与「算法本就如此」混成一团，源文件本身就是判据）。四种报文形逐字对 `cksum.c:506-524` 与 `:474-491`：缺省 `NAME (档) = 摘要`、`-n` 摘要在前、`-q` 只剩摘要且 **`-q` 赢 `-n`**（C 是 `if QUIET … else if NORMAL … else …` 一串），无操作数走 `filterfunc(pflag)`——`-p` 先原样转抄再印摘要、**中间不插换行**，那一腿不吃 `-n`/`-q`。为此把 C 的 `pflag`（`:250`）与 `nohashstdin`（`:258`/`:264`/`:273`）补成状态位：`-p` 放行后标准输入那一腿照旧要跑，`-s`/`-t`/`-x` 放行后自己印完并压掉那一腿。**参照件路线按板上判据走通并且先自证**：`tmp/cksum-384/build-ref.sh` 用本仓三份 C 原件 + `tools/atf-c-compat/host-ref/` 补面编出参照件，它必须先复现 `md5test-out`/`sha1test-out`/`sha1test2-out` 与 `t_hashes.c:97-98` 的九串仓库内期望值才被采信（缺 `be32dec`/`be32enc` 原型时 gcc 按 `int` 隐式声明截断，摘要会**静默算错**——这类「编译过但读数错」的坑值得每条参照件腿都防一遍）。**本轮最大收获是一枚 P0 现查**：`cksum` 的宿主读腿**整条是断的**——`src/bin/cksum.rs` 直写 `use minix_sys::{close, open, read}`，而 `minix_sys::open`/`read` 走的 `ipc::DirectTrapTransport` 自陈真实 trap 序列尚未接（`os/libs/minix-sys/src/ipc.rs:530-534`），现跑 `printf 'abc' | cksum` → `4294967295 0` 即**零字节输入的 CRC**、读一份真实存在的档 → `cksum: <档名>` 退 1。它藏到今天的原因是本包既有五枚端到端门全在选项诊断面上、一次也没读过字节；**没有读字节能力的端到端门比没有门更坏**。摘要两条腿因此改走 `bin_support` 的 cfg 双生面（否则新门只能钉住空输入的摘要），CRC 与 `-o 1`/`-o 2` 三条腿本轮不动，另立 `CKSUM-HOSTED-READ-SEAM`（修法即批次三百七十六给 `wtf` 用过的那一步）。另三行登记不执行：`CKSUM-PROGNAME-SELECT`（C 在 getopt **之前**按 `getprogname()` 查表，`:177-186`）、`CKSUM-SHA-FILECHUNK-FSTAT`（SHA 族 `File` 走 `FileChunk`+`fstat` 把 `st_size` 当上限，管道/字符设备交回**空输入的摘要**，而 MD 族读到尽头——两族在 C 里本就分叉，`hashhl.c:102-106` 对 `mdXhl.c:71-96`；与 A6a 那条同一族但**真值在仓库内读得到**，不需等真机窗）、向量门挪进 `os/libs/minix-crypt` 本身（`md5.rs` 现无一枚测试，`grep -c "#\[test\]"` → 0；那是 B 板领地）。一处与旧账的判断冲突按板走并写明：批次三百三十三要求「八项整批接、不逐项开口子」，本行允许拆 2–3 轮，以切板时的行定义为准；口子这样收——未接的五枚走同一句占位同一退码 2，`Algo::from_hashname` 认不出即交回 `None`，薄壳不会印一枚空摘要冒充结果。**抄向量这件事自己会错，两处留给以后每一轮**：`md5test-in` 最后一行是八十个字符而我第一次按眼数抄成七十个，门当场报红；`jot -s '' -b 'a' -n 1000000` 灌进去的那一枚被摘要是**整整一百万个 `a`**（`h_hash.c` 的 `CHOMP` 只剥行尾那一个换行），读错就钉错一整枚门。读数：宿主与容器权威门各 **1098 / 0**（68 段，起点 1087 = +7 库门 +4 面门）、在线腿预检 200；三架构 `--bins` 全退 0（新依赖 `minix-crypt` 在三个 none 目标都建得过）；clippy 154 持平、**按告警位置判**触及件零命中（唯一一次 `needless_range_loop` 落在我件上，用 `iter().enumerate()` 消掉而不是加 `#[allow]`）；rustfmt 零漂移；六枚负控各咬住（1/3/1/2/12/2 枚变红）、还原 md5 一致、复跑 19/0。**A4 保持 🔄，下一片首动作**：`bash tools/claim.sh claim CKSUM-HASH-DIGEST qoder`（第二片接 `sha2.c` 的 64 位那一半 `SHA384`/`SHA512` 与 `md4c.c` 的 `MD4`；`minix3/tests/lib/libc/hash/t_sha2.c` 已备 `test384[]`/`test512[]` 各二十七枚现成期望值）。
- `2026-10-19` qoder / todo 循环第九十九笔（A4 `CKSUM-HASH-DIGEST` **第二片**，八枚里到六枚；`claim/CKSUM-HASH-DIGEST-qoder` @ 基线 `45936302d`，动 `os/commands/usr-bin/textfilter/src/digest.rs` 与三处「剩下五枚未接」的说明 + 18 篇 `todo.md` 批次三百八十五、07 篇 cksum 行、A 板 A4 行、本板）。新译 MD4（`minix3/common/lib/libc/md/md4c.c` 的 `MD4Transform`）与 SHA384/SHA512（共用一颗按 `sha2.c` 的 `SHA512_Transform` 译出的芯，SHA384 只换初值并取 64 字节结果的前 48——`SHA384_Final` 本身就只写六个状态字）。**两处只有读源文件才写得对**：MD4 第三组常数是 `0x6ed9eba1`，不是记忆里那个 `0x6d70756d`；MD4 的 `FF` 宏没有 MD5 那句 `(a) += (b)`。两格各设一枚负控（N1/N2）钉住，换成「记忆里」的数就恰好咬住四枚 MD4 门——**记错常数比抄错常数更危险，因为它看起来像回事**。**填充判据从一条写死的常数升级成一条式子**：`rem + counter.len() + 1 > block_len` 才补一整块纯零；SHA-512 家族块长 128 而末尾比特数占 **16** 字节，阈值因此是 `rem <= 111`，与 64 字节族的 `rem <= 55` 同源不同值，于是同一条式子被两种块长从两边夹住（N5 把它退成 `>=`，两张边界表同时变红）。参照件从三族扩到六族（多链一份 `md4c.c`），**采信前照例先自证**：`md4test.c:75-94` 的 RSA 七条、`t_sha2.c` 的 `test384[]`/`test512[]`（现查各 **26** 条——A 板那一行当时按行数估成「各二十七枚」，本轮按脚本解析值改定）逐条与参照件相同，才由它出 23 个边界长度。**这一片把「抄向量」这件事从流程里删掉了**：Rust 测试里的期望值由脚本从 C 源解析后直接写入（`tmp/cksum-384/patch_tests.py`），不再经人眼；上一片手抄 MD5 第七行错过一位（那行是八十个字符），门当场报红，这一片不给自己第二次机会。顺带一条 C 源解析陷阱：`md4test.c` 第七条字符串用反斜杠续行，机械摘取时必须先抹掉「反斜杠 + 换行」，否则会把带裸换行的输入当摘要对象。门：库侧 +4（`digest::` 7 → **11**）、面侧 +1（`cksum_face` 12 → **13**），并把上一片那枚 `the_three_ported_families_print_c_shapes_for_a_file_operand` 改名 `md5_sha1_sha256_print_c_shapes_for_a_file_operand`，免得「三族」在六族面前变含糊。读数：宿主与容器权威门各 **1103 / 0**（68 段，起点 1098）、在线腿 200；三架构 `--bins` 全退 0；clippy 154 持平、按告警位置判触及件零命中；rustfmt 零漂移；六枚负控 N1—N6 各咬住（4/4/4/4/2/4 枚变红）、还原复跑 **24 / 0**。**流程陷阱一枚**：`rustfmt src/bin/cksum.rs` 会跟 `#[path = "../bin_support.rs"]` **递归格式化 `bin_support.rs`**，本轮因此动了 A6c 明确写着「不顺手改」的 `bin_support.rs:228`（if-else 收成一行 + 测试模块 `use` 重排）；撤法是对非本笔文件 `git checkout --`（撤的是本会话刚造的排版改动，不是别人的在制工作），撤后复跑整包门确认只是排版。**A4 仍差第三片**：`md2` 与 `rmd160`（真源 `minix3/lib/libc/hash/md2/md2.c`、`minix3/common/lib/libc/hash/rmd160/rmd160.c`；`md2` 那颗 256 项 S 表同样机械摘取）。**下一轮首动作**：`bash tools/claim.sh claim CKSUM-HASH-DIGEST qoder`（第三片收齐八枚，A4 才翻 ✅；此后才轮到 A5 的 `-s`/`-t`/`-x` 三条腿与 A5b 的文件腿报文形）。
- `2026-10-19` qoder / todo 循环第一百笔（A4 `CKSUM-HASH-DIGEST` **结案**：第三片补上 MD2 与 RMD160，八枚摘要本体全有；`claim/CKSUM-HASH-DIGEST-qoder` @ 基线 `e40931b03`，动 `os/commands/usr-bin/textfilter/{src/digest.rs,src/cksum.rs,src/bin/cksum.rs,tests/cksum_face.rs}` + 18 篇 `todo.md` 批次三百八十六、07 篇 cksum 行、A 板 A4 行翻 ✅、本板）。MD2 按 `minix3/lib/libc/hash/md2/md2.c`：块长 32 而非 64、**没有**末尾比特数那段填充（补的是「缺几个字节就填几个值为该数的字节」）、补完把**当前**校验和整个喂进去，且 `MD2Transform` 末行把写指针送回 **16 而不是 0**——前 16 格住着链接值，清 0 就是把链接值当输入区覆盖（负控 T3）。RMD160 按 `minix3/common/lib/libc/hash/rmd160/rmd160.c` 的 160 条 `R(...)` 行：**右线函数组倒着走而常数组正着走**（第 0..15 轮 `F4` 配 `KK0`，最后 16 轮 `F0` 配 `KK4`），末尾合并式 `t = state[1] + cc + d;` 的两个加数**来自不同的线**（`cc` 是左线存进 `aa`…`ee` 的、`d` 是右线当前的）——**本笔真写反过**，第一次跑就红在 `rmdtest.c` 第一条（空输入）上；这类「两条对称线交叉取值」从对称性推不出来，只能照抄那六行，对调它现在单列为负控 T1。**结构性的一处收拢**：`-a` 的名字表原本有两份（`src/cksum.rs` 的八行 `crypto_hashname` 与 `src/digest.rs` 的 `Algo::from_hashname`），本笔并成一份并照 C 的 `strcasecmp` 改成大小写不敏感、加 `Algo::hashname()` 交回诊断与报文头那一个大写名，`CksumRequest::hash` 随之从 `Option<&str>` 改成 `Option<Algo>`；收益不是少十几行，而是**「认得名字但算不出」这个中间态在类型上不再存在**，薄壳那条分支被删掉而不是留成死码（负控 T6 退回大小写敏感，一次咬住 11 枚——三族面门 + cksum 库门 + 占位门全在红名里）。占位句现在只剩两条腿：`-s`/`-t`/`-x` 与 hash 在里的 `-c`（A5 的射程因此比切板时窄了一格，八枚本体不再是它的前置）。读数：宿主与容器权威门各 **1107 / 0**（68 段，起点 1103 = +3 库门 +1 面门）、三架构 `--bins` 全退 0、clippy 154 持平且触及件零命中、rustfmt 零漂移；六枚负控 T1—T6 各咬住（4/4/3/3/4/11 枚变红）、还原复跑 **570 / 0**；参照件已链到六份 C 原件（`md2.c`/`md4c.c`/`md5c.c`/`rmd160.c`/`sha1.c`/`sha2.c`）交回八串摘要，且**先逐条复现 `rmdtest.c` 那八条仓库内期望值**（八条全合）才用来出 31 个长度的读数；MD2 在仓库内没有期望值文件（现查 `md2.3` 无任何摘要串），它的真值就是 `md2.c` 本身，而参照件链的正是那份。**两处本笔补回的缺陷都在账上留名**：① 批次三百八十五收尾那次「起止标记」式替换把 07 篇那一行的「四种输出形状逐字对」半句吞掉了（结束标记被消费却没回写）——用起止标记做替换时，结束标记要么原样保留、要么在替换文本里重写一遍，只能选一个；② `A 板 A4 行`原写 `test384[]`/`test512[]`「各二十七枚」是按行数估的，脚本解析实为**各 26 条**。**A4 结案，下一轮首动作**：`bash tools/claim.sh claim CKSUM-FILE-LEGS qoder`（A5b：文件腿的 `Cannot read %s` 一族与 `warn` 的第三段 `: <strerror>`；本支「坏档不出行」那枚门已经把位置钉出来）。

- `2026-10-19` qoder / todo 循环第一百零一笔（**补记**，批次三百八十七 A5 第一片）：`cksum` 的 `-s`/`-t`/`-x` 三条自己要把一整套印完的腿接成真读数（`claim/CKSUM-HASH-STATE-qoder`），整包 **1107 → 1114**（+3 库门 +4 面门），容器权威门同读数退 0。本轮的另一件仪器事更重要：第一次把**整支 `cksum` 参照件**编出来（`tmp/cksum-hash-state/build-cksum-ref.sh`，`minix3/usr.bin/cksum/` 十三件 + libc 侧六份摘要本体 + 八份 `*hl.c`，自检步是 `abc` 的 MD5/SHA1/SHA256 三串与仓库内期望值逐字相同），于是 `-c` 与文件腿从此都有绝对读数可依；新立的 `shapes.sh`/`ab.py`（202 形）同轮建起。新门当场咬出一枚旧缺陷：`'s'` 分支取完值没有 `break`，`-sabc` 把 `bc` 也当选项继续吃。详情与逐形读数见 18 篇批次三百八十七。
- `2026-10-19` qoder / todo 循环第一百零二笔（**补记**，批次三百八十八 A5 收尾片）：`hash` 在里的 `-c` 校验腿落地，本壳那句 `-a crypto algorithms are not modelled yet` 占位报文整条退场（`cksum` 从此没有占位句）；`-c` 的两处读（校验档自身、逐行档名的档）改走 `bin_support` 的 cfg 双生面——此前宿主跑 `-c` 时**每一行都判红**。整包 **1114 → 1121**。**A5 四条腿全落地、板上翻 ✅**（代码笔 `c7e28d08b` + `3efbff4ba`）。两处手抄期望值被门当场抓住（`hello\n` 的 SHA256、`abc\n` 的 CRC），据此立一条规矩：期望值一律现取，数字也不例外。新登记两条：`CKSUM-FILEFUNC-NULL-CRASH`（C 那条 `strcmp(cksum, NULL)` 竞态面不复现，按登记不追）、`CKSUM-HOSTED-CHECK-DIR-CARRIER`（校验档名是目录时两边退码不同，差别在承载，真机读数等 C 板窗）。详情见 18 篇批次三百八十八。
- `2026-10-19` qoder / todo 循环第一百零三笔（**补记**，批次三百八十九 A5b `CKSUM-FILE-LEGS`）：文件腿的报文形四处缺第三段补齐——`warn(3)` 会缀 `: <strerror>` 而 `warnx(3)` 不会，摘要腿、crc 腿、校验档打不开、`-w` 下的档名各是一处（`cksum.c:477-483`、`:491-495`、`:301-303`、`:451-456`）；顺带把 crc 那条腿的宿主读缝也接到双生面上，**`CKSUM-HOSTED-READ-SEAM` 随之整条闭合**。A/B 第一次做到**零分叉**（202 形）。整包 **1121 → 1124**；代码笔 `aae06166e`。一枚负控（`Speed` 除数）跑成无分辨力——本机时间恒 0，如实记「门存在但量不出来」而不假造时钟。详情见 18 篇批次三百八十九。
- `2026-10-19` qoder / todo 循环第一百零四笔（**补记**，批次三百九十 A7 `TR-EQUIV-OOBCH-SET2`）：`tr abc '[=c=]'` 的 string2 耗尽之后交回的不是末字节而是**当时的 `lastch`**，而集合类走到数组末尾撞上的是哨兵 `OOBCH = UCHAR_MAX + 1`（`extern.h:39`），经 `putchar` 截断成 `0`——所以参照件是 `c\0\0` 而本壳是 `ccc`。判据是「**最后一项**是不是类」而不是「式子里有没有出现过类」。`-s` 与映射按 C 合并成同一趟（`tr.c:222-224`、`:239-246`，`lastch` 初值就是那枚哨兵），三格后果全落进门里。本轮还撞出一枚**未登记的 P0**：本层自造的八格上限让九枚字面量的集合静默退 1（`tr 'abcdefghi' 'x'` 空输出退 1，参照件 `xxx` 退 0），那张 `classes` 表交出后无人消费，整块删。A/B 26 形 0 分叉；整包 **1124 → 1131**；负控六枚 P1—P6 各咬住（P4 空咬一次后补形才咬）；代码笔 `39b2218d3`。**板上 A7 翻 ✅**。详情见 18 篇批次三百九十。
- `2026-10-19` qoder / todo 循环第一百零五笔（批次三百九十一 A11 `REV-NONASCII-OPTLETTER`）：`rev` 的集外字母那一行报文，C 打的是**一个裸字节**而本壳按 `char` 取。`claim/REV-NONASCII-OPTLETTER-qoder` @ 基线 `30d800b58`，动 `os/commands/usr-bin/textfilter/{src/lib.rs,src/rev.rs,src/bin/rev.rs,tests/rev_face.rs,tests/bin_support_face.rs}` + 18 篇批次三百九十一、07 篇 rev/colrm 两行、A 板 A11 行翻 ✅ 且 A2 行射程两分（代码笔 `21cef354b`）。**取法与渲染两半一次做完**：共件 `empty_optstring_operands` 取短横后的第一个字节（`getopt.c:79` 的 `optopt = *place++` 步进在 `char *` 上，与 `src/col.rs` 批次三百二十六同一法），`GetoptReport` 新增 `line_bytes` 交字节串（三句措辞经私有 `wording()` 与 `line` 单源，措辞只有一份——负控 N7 把 `unknown` 改成 glibc 的 `invalid` 时 8 枚门一起红，证的就是这件事），`rev` 薄壳改调它。**读 C 源纠正一处形状**：`rev.c:70-75` 拿到 `'?'` 就 `usage()` 退出，所以 `-é`（`c3 a9`）只有**一行**报文、打 `c3`，后半那个字节根本不进报文——登记里容易被读成「每个字节一行」。为什么加第二个入口而不是原地改签名：`line` 的调用点本轮现量 **29 处 / 24 个源文件**，签名一改就是 24 个文件一起编译失败，而板上 A2 明写「每轮只换一个消费者」——本笔就是 **A2 渲染半的切片 1**；`line_bytes` 对码位 > `0xFF` 退回 UTF-8 与 `line` 同串，于是「换渲染」与「换取法」两轮互不干扰。**共件连带**：`empty_optstring_operands` 由 `rev` 与 `colrm` 共用，故 `COLRM-NONASCII-OPTLETTER` 的取法半随之落地，它的渲染半并入 A2 下一轮（现量 `colrm -é` 本壳 `c3 83` 而参照件 `c3`）。门：库 +3、`rev_face` +1（新增 `run_bytes` 夹具逐字节比 stderr——lossy 的 `String` 把 `c3` 与 `c3 83` 都渲染成替换字符，文本夹具区分不了这两种错法），并把 `tests/bin_support_face.rs` 里 `rev` 在 `-\xff` 上的期望从 `ef bf bd` **有意识翻正**为 `ef`（批次三百二十九故意留在门上的那批断言，等的就是这一笔）。读数：A/B **11 形不一致 0**（`bash tmp/rev-391/ab.sh`，参照件复用 `tmp/rev-309/rev_ref`），另三形（`-\xff`/`-\x80`/`-\xc3` 这些非法 UTF-8）仍差一个字节且已如实记账——根因在 `bin_support::args` 的 `from_utf8_lossy` 把原字节在参数缝上就换掉了，属 A2 射程②；宿主与容器权威门各 **1135 / 0 / 68**（起点 1131）、三架构全退 0、clippy 154 同数触及件零命中、rustfmt 新增长零漂移。负控**七枚 N1—N7 各咬住**（4/2/1/7/6/3/8 枚变红，还原按内存原文比 md5、复跑 0 失败）。**两处本轮自己的错，都写进账**：① 第一遍七枚全报「未达标」，红名与失败数都对、唯独 md5 不符——判据把「还原后读回」写在还原之前，读到的是注入态；与批次三百五十四那枚计数正则错同族，**判据的假阴同样是判据的错**；② 三架构命令漏了 `-p minix-textfilter` 就变成整 workspace 的 none 构建，`minix-rt` 在那儿要 `std`，第一发撞出 170 条 `^error`（与本片无关，但命令得写对）。另记一条**配方勘误**：本 crate edition 是 2024（`os/Cargo.toml:217`），`rustfmt --edition 2021` 连 `src/lib.rs` 都解析不了（`src/pr.rs:284` 的 let 链要 2024），此前若干笔写的「`--edition 2021` 触及件零漂移」只对能独立解析的叶子文件成立。**流程债一并认**：批次三百八十七至三百九十这四笔当时没做 edge4 里程碑回写（`tools/claim.sh release` 每次都提示了这一条），上面四条是补记，事实全部取自 18 篇对应批次。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（A2 渲染半切片 2，先 `colrm`：取法已是字节，只差把 `line` 换成 `line_bytes`）。

- `2026-10-19` qoder / todo 循环第一百零六笔（批次三百九十二，A2 `GETOPT-REPORT-BYTE-LETTER` 渲染半**切片 2**）：第二支壳 `colrm` 换到字节形（`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `82acce8f6`，动 `os/commands/usr-bin/textfilter/{src/bin/colrm.rs,tests/colrm_face.rs}` + 18 篇批次三百九十二、07 篇 colrm 行、A 板 A2 行记到切片 2；代码笔 `90bb21b70`）。共件一字未动。**为什么是 `colrm`**：它的取法半已随上一笔的共件 `empty_optstring_operands` 落地（`rev` 与 `colrm` 共用同一支扫描件），只差壳侧那一行——换完 `colrm -é` 与参照件 `tmp/colrm-310/colrm` **逐字节相同**（都是 `c3` 一个字节），这一支壳上的 `c3 83` 错法（按字节取但仍按 `char` 渲染）消失。**门** `colrm_face` 16→**17**：`the_option_line_of_a_nonascii_letter_carries_one_raw_byte` 三形逐字节（`-é` 钉 `c3`+usage、`-xé` 钉聚簇第一个字节且**只有一行** libc 报文、`-Z` 钉 ASCII 那一形两个入口一字不差），夹具新增 `run_bytes`；**枚名与 `rev_face` 那枚刻意不同**——两枚同名时负控的红名清单分不出是谁红，判据就退化成「有东西红」。**A/B 十三形 SAME=12 / DIFF=1**：唯一 DIFF 是 `colrm 5 2`（起止倒置）缀的 `strerror(0)` 句子——本机 glibc 印 `Success`、Minix 自己的表（`minix3/lib/libc/compat/gen/compat_errlist.c:53`）印 `Undefined error: 0`，本壳跟 Minix 那份，差在所链 libc 的表而不是句子形状，批次三百三十二已记；`-\xff`/`-\x80` 两形仍差一个字节，根因在 `bin_support::args` 的 `from_utf8_lossy`（射程②，本笔不闭）。**负控七枚各咬住**（1/5/1/4/10/12/4 枚变红，逐枚先还原、再读回比 md5、复跑 0 失败），其中 C7 是本笔新加的注入形状：**只撤掉 usage 那一行、报文文本一字不改**，证这一枚门钉的是「libc 行 + usage」的配对而不只是那个字节。读数：宿主与容器权威门各 **1136 / 0 / 68**（起点 1135）、三架构带 `-p` 全退 0、clippy 154 同数触及两件零命中、`rustfmt --edition 2024` 新增长零漂移（`tests/colrm_face.rs` 基线本来就是零漂移，所以直接对它跑一次格式化只会动本笔新行——比手抄 rustfmt 建议更稳）。**闭合**：`COLRM-NONASCII-OPTLETTER` 两半全闭；渲染半进度 2 支壳，余 **28 处调用点 / 23 个源文件**。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 3 建议 `col` 或 `column`：两支取法半分别在批次三百二十六与三百一十四已定，换渲染时 `tests/col_face.rs` 的 `nonascii_option_letter_is_reported_as_one_byte` 钉的 `c3 83` 期望要**有意识翻正**为 `c3`）。

- `2026-10-19` qoder / todo 循环第一百零七笔（批次三百九十三，A9c `WTF-SCRIPT-FEATURE-PARITY` **第④条：`is` 词的位置**）：`claim/WTF-SCRIPT-FEATURE-PARITY-qoder` @ 基线 `192c7a5be`，动 `os/commands/games/text-games/{src/bin/wtf.rs,src/acronym.rs,tests/wtf_face.rs}` + 18 篇批次三百九十三、24 篇六处、A 板 A9c 行（代码笔 `38b83bfd1`）。**真值只有一句**：`minix3/games/wtf/wtf:32-34` 的 `if [ "$1" = "is" ]; then shift; fi`——位置在**开头**、次数**一次**、比较是**字面量**；所以 `LOL is` 的尾部 `is`、`is is LOL` 的第二枚、`IS LOL` 的大写枚，全是**查询词**（各补一句 `I don\'t know what …` 并把退码置 1，四形原文存 `tmp/wtf-393/script-readings.log`）。**本壳旧写法错两处且同源**：① 选项环之后另起一圈 `if args[at] != "is"` 把任意位置的 `is` 丢掉；② 查询循环再用库件 `is_skipped_word`（大小写不敏感）跳一次。根因是**承载位置选错**：这条规则发生在参数收法里（脚本在查询之前已经 `shift` 完），却被做成 `acronym.rs` 的一张「跳过词」表，于是「谁跳、跳几枚、跳多大写」三件事都脱离了真值那一行。修法按 C 归位——一个圈走完选项环（`-f` 吃值、`-o` 不吃值、`--` 结束、第一个非选项 token 就 `break` 且不重排，对位脚本 `:30` 的 `shift "$OPTIND-1"`），圈后只在首枚等于 `is` 时移走一次，查询循环不再跳任何东西；**`SKIPPED_WORDS` 与 `is_skipped_word` 整块删除**（唯一消费者就是那两处错法）。门：面 +3（三枚都同时比 stdout、stderr 与退码——`LOL is` 那形的退码差比少一行报文更容易漏）、库 −1（随删件），整包 **66 → 68**（8 段）。**参照件就是真源本身**：`wtf` 是 POSIX shell 脚本（判据见批次三百七十六），A/B 直接 `sh minix3/games/wtf/wtf -f minix3/share/misc/acronyms <形>` 现跑，**十一形不一致 0**（`bash tmp/wtf-393/ab.sh`）。**本轮撞出的新登记 `WTF-MULTI-F-ACCUMULATE`（A9c 第⑤条，登记不执行）**：脚本的 `-f` 是累加（`acronyms="$OPTARG $acronyms"`，逐档查）——现量两枚各含一条目的档 `-f db1 -f db2 aaa` 命中 db1，本壳因「后写覆盖」报查无；它与第②条同段（`:42-56`）但是两件事（②问「哪些档进列表」，⑤问「同一枚档进过多次」），另起一笔。**24 篇六处同错一并改定**（P0-fact 的文档半）：文中「命令行里的 `is` 词跳过」「`acronym.rs` 管跳过词」这类过度一般化陈述正是缺陷的来源，现全改为「开头那一枚、一次、字面量，且属参数收法归壳」；`grep -n "跳过词" 24-text-games.md` 现读为空。读数：宿主与容器权威门各 **68 / 0 / 8**、三架构带 `-p` 全退 0、clippy 13 同数触及三件零命中、`rustfmt --edition 2024` 新增长零漂移。负控**七枚各咬住**（W1 移走全部 1 枚 / W2 大小写不敏感 1 枚 / W3 一枚都不移 2 枚 / W4 改移末尾 1 枚 / W5 把查询循环的跳过装回去 2 枚 / W6 `-f` 不吃自己的值 7 枚 / W7 `--` 不推进 1 枚；逐枚先还原、再读回比 md5，复跑 0 失败）。板上 A9c 记④闭，余 ①②③ 与新⑤。下一轮首动作 `bash tools/claim.sh claim WTF-SCRIPT-FEATURE-PARITY qoder`（第②条：`-o` 的真语义是通配展开 `/usr/share/misc/acronyms*` 且 `*-o` 那批只在 `-o` 时纳入，脚本 `:42-53`；本机没有该档时脚本自己就把这一形露出来了——打的是 `cannot open acronym database file \`/usr/share/misc/acronyms*\'`，即**通配没展开就把字面量当档名**）。

- `2026-10-19` qoder / todo 循环第一百零八笔（批次三百九十四，A9c **第⑥条：`wtf` 的词条侧大小写**——本轮新量出的一条规则）：`claim/WTF-SCRIPT-FEATURE-PARITY-qoder` @ 基线 `2a23e21d1`，动 `os/commands/games/text-games/{src/acronym.rs,tests/wtf_face.rs}` + 18 篇批次三百九十四、24 篇两处、A 板 A9c 行（代码笔 `5d812e44a`）。**规则是单向的**：`minix3/games/wtf/wtf:75-77` 先把操作数整个大写（`tr '[a-z]' '[A-Z]'`），再用 `fgrep -h "$target"` 与 `sed -ne "\|^$target[[:space:]]*||"` **逐字**比库档行首，所以查询侧被抹平、词条侧不抹平，**库里行首带小写字母的条目在 C 里永远取不到**。本壳按 `eq_ignore_ascii_case` 两侧都抹平：现量库行 `abc<TAB>lower term entry` 时 C 查 `abc`、查 `ABC` 都是 ``I don't know what `…' means!`` 退 1，本壳两形都错报命中（而 `DEF<TAB>upper` 查 `def` 两边都命中 `DEF: upper`）。修法在库件的比较上叠一条「`name` 自身不含小写 ASCII 字母」——与「先大写查询词再逐字比」等价（两条件合起来恰好要求 `name == 大写(term)`），且不留临时 `String`；非 ASCII 字节两侧都不动，与 `tr` 只管 ASCII 那一段一致。壳侧一行未动（命中打大写形、查无打操作数原形，上一笔已对过参照件）。门：库 +1 `a_database_term_written_in_lowercase_is_unreachable`（四条形）、面 +1 `a_lowercase_database_term_is_not_reachable_end_to_end`（三形分通道比 stdout/stderr/退码），整包 **68 → 70**（8 段）；**既有门一枚都不必翻正**——`the_lookup_ignores_case` 与 `test_lookup_finds_first` 的夹具词条本来就是大写形，钉的是查询侧的不敏感，那条 C 也有。A/B 十形 **0 分叉**（`bash tmp/wtf-394/ab.sh`，参照件仍是脚本本身）。负控六枚各咬住（V1 撤新条件 2 枚 / V2 两侧都逐字 8 枚 / V3 判据反向 8 枚 / V4 判据挪到操作数 8 枚 / V5 撤空查询守卫 1 枚 / V6 壳侧打印原形 5 枚），逐枚先还原再读回比 md5、复跑 0 失败。**V4 第一次跑成未达标，错在注入自己**：原设计把判据挪到释义那一侧并要求「释义全小写」，而夹具的释义里都含空格——空格不是小写字母，于是那枚注入的真实效果是「凡含空格的释义都取不到」，新门两条形恰好都取不到，看起来像门没牙。这与此前三次空咬同族：**注入空咬先怀疑注入的形状是不是它自称的那个形状**，别急着给门加断言。**①⑤ 的读数一并量清**（同一段脚本 `:42-56`，即「库档列表」那件事）：`-f` 是**前插**（`acronyms="$OPTARG $acronyms"`，故 `-f w1 -f w2` 先搜 w2）、多档命中时**只给第一行加 `TARGET: ` 前缀**（现量 `-f w1 -f w2 lol` → `LOL: second file` 换行 `first file`）、任一档不存在就在**那一档**报 `cannot open acronym database file` 并当场退 1（不看后面的档）、`ACRONYMDB` 整串**按空白断词**成列表且 `-f` 一旦给过就被完全忽略。⇒ 下一轮把 ① 与 ⑤ 合并做（只补 `ACRONYMDB` 不做列表，等于把一个错换成另一个错）。**② 仍是阻塞**：缺省那支要对 `/usr/share/misc/acronyms*` 做通配展开，目录枚举在本 crate 拿不到（`dents` 住在 `minix-fileops`，跨命令 crate 依赖或把它提到 `os/libs/` 都不在本板领地），连「没展开时把字面量当档名」那一形也只能等那支缝定下来。**③** 的三级退化（`whatis` → `pkg_info -qc` → `make help topic=`，`:82-104`）要先定「脚本调外部程序时本壳该调、该退化、还是该省」的口径，且本机没有 `whatis` 才落到查无句——真机装了就会多一条分路，归编排线定夺。文档纠正 24 篇两处（§2「查询不分大小写」与 §4 不变量——把单向规则写成双向，与上一笔「`is` 词跳过」同种毛病；剩下三处「不分大小写」都在探险游戏词汇表一族，未动）。读数：宿主与容器权威门各 **70 / 0 / 8**、三架构带 `-p` 全退 0、clippy 13 同数触及件零命中、`rustfmt --edition 2024` 零漂移。板上 A9c 记⑥闭，余 ①⑤ 合并一轮 + ②③ 各按阻塞与口径。

- `2026-10-19` qoder / todo 循环第一百零九笔（批次三百九十五，A9c **①+⑤：`wtf` 的库档是个列表**）：`claim/WTF-SCRIPT-FEATURE-PARITY-qoder` @ 基线 `c268c4bba`，动 `os/commands/games/text-games/{src/lib.rs,src/acronym.rs,src/bin/wtf.rs,src/bin_support.rs,tests/wtf_face.rs}` + 18 篇批次三百九十五、24 篇两处、A 板 A9c 行（代码笔 `a1810dc89`）。**为什么两条并作一轮**：它们在 C 里是同一个变量——脚本 `:42-56` 造 `acronyms` 那串、`:64-69` 逐档检查、`:76` 逐档搜；只做 ①（`ACRONYMDB`）不做 ⑤（多档累加）会把 `"alpha  beta "` 这种整串当一个档名，等于把一个错换成另一个错。**四条现量规则**（参照件仍是脚本本身）：`-f` 每给一枚**插到最前面**（`:21` `acronyms="$OPTARG $acronyms"`，故 `-f alpha -f beta` 先搜 beta）；一枚 `-f` 都没给时 `ACRONYMDB` 整串**按空白断词**顶上，而 `-f` 给过后环境**整支不看**（现量 `ACRONYMDB=<含 aaa 的档> … -f <不含的档> aaa` → 查无）；列表里任何一档开不了就在**那一档**报 `cannot open acronym database file` 并**当场退 1**，后面的好档不查也不印命中行（正序反序同形）；一个词命中**多行**时 `:80` 的 `echo "$target: $ans"` 只给第一行加 `TARGET: ` 前缀，其余行裸出（同档两行 `LOL` → `LOL: first match` 换行 `second match`）。**码形**：壳侧列表全程借用 `Vec<&str>`，库件 `lookup_acronym`（交回第一个）换成 `matching_expansions`（交回全部，顺序 = 列表序 + 档内行序），`src/lib.rs` 补 `extern crate alloc;`（本 crate 第一次在库里要 `Vec`），环境缝新增 `bin_support::envs()` 双生面（照 `os/commands/bin/fileops/src/bin_support.rs:envs` 的形状，宿主腿把 `vars()` 重新拼回 `KEY=VALUE`）。**三架构门本轮第一次挡住本笔自己的码**：初版用 `db_files.iter().map(|p| p.to_string())`，宿主 `cargo test` 全绿，而 `cargo build -p minix-text-games --bins --target x86_64-unknown-none` 三条 `E0599`——`no_std` 形态下 `ToString` 不在 prelude；改借用后退 0。门：面 +4（含 `run_env` 夹具，每形都显式写 `ACRONYMDB` 不让父进程的值漏进来）、库 +1，另三枚旧门改成新 API 形状而断言一字未增删，整包 **71 → 75**（8 段）。A/B 十二形 SAME=11 / DIFF=1，唯一那形就是 ② 的阻塞残差：环境空串时 C 落到**未展开的通配字面量**，打 `cannot open acronym database file \`/usr/share/misc/acronyms*'`（带星号），本壳打不带星号的单档名——差一个字符，根因是本 crate 拿不到目录枚举。负控七枚各咬住（1/1/1/3/2/2/1 枚变红），**全部重跑过一次**：上一版脚本的 L2/L3/L7 锚点写的是 `to_string` 那一版的代码，借用版落地后锚点变了——锚点必须跟着最终形态走，否则「未施加」会被误读成「门没牙」。新立 **⑦ `WTF-DB-REGULAR-FILE-TEST`（登记不执行）**：C 的前置检查是 `[ ! -f $f ]`（普通文件），本壳用「读失手」代替，现量 `-f /tmp` 与 `-f /dev/null` 两句报文不同（C `cannot open acronym database file \`/tmp'`，本壳「I don't know what `lol'」）而退码同为 1；补它要 `S_ISREG`，同一条判据在本仓已有两份（`fileops/src/cp.rs:59`、`doctools/src/bin/which.rs:52`），第三份该等共件的家定下来——`env_value` 的跨 crate 重复也挂在同一个决定上。读数：宿主与容器权威门各 **75 / 0 / 8**、三架构带 `-p` 全退 0、clippy 13 同数触及件零命中、`rustfmt --edition 2024` 新增行零漂移。板上 A9c 四条里此刻只剩 ②③，都不是「本壳自己的事」（② 缺目录枚举、③ 缺「脚本调外部程序时本壳该调、该退化、还是该省」的口径裁决）。

- `2026-10-19` qoder / todo 循环第一百一十笔（批次三百九十六，**A26 与 A19 两行的对账笔**，纯文档、不改码）：`claim/K2-S-6.3-DOC-ACCOUNTING-qoder` @ 基线 `5c9dc576c`，动 `rewrite-notes/coordination/PARALLEL-20261018-A-CMD.md` 两行 + `rewrite-notes/09-stage-init/todo.md` 摘要句 + 18 篇批次三百九十六（文档笔 `2c0513f0e`）。**A26（`K2`/`S-6.3` 每核运行队列的文档记账半）改判 `⏸ 等一线`**，两条读数：① 那条「记账取代实现」的理由在文档里确实还没有（`06-stage-sched` 四搜为空；`01-stage-kernel/11-scheduling-primitives.md` 只有 BKL 的取得点两处 `:834`/`:1612`，指向 08 篇 §3.2 的锁约定），所以不能按「已写明则标 ✅ 附行号」收；② **按 ledger 那句话字面写会与现码矛盾**——每核队列在码里已做实：`ProcessTable::sched` 是 `[Scheduler; MAX_CPUS]`，`os/kernel/src/lib.rs:3226` 的 `pick_proc` 走 `table.sched_for_cpu(cpu)`，`os/kernel/src/proc_table.rs:719-742` 的 `# Design (per-CPU run queues)` 段逐项对位 C 的 `run_q_head[]`（`cpulocals.h:58-59`）与 `rp->p_cpu`（`proc.c:1614/1688/1739/1801`）；还能写的只剩「队列操作由大内核锁串行化」这半句，而它属内核侧叙事，落在 `rewrite-notes/01-stage-kernel/`——本板禁碰清单第一节。另记一条行号漂移：板上与索引写 `TODO-LEDGER-OPEN.md:24`，现查该 K2 行在 **`:23`**。**A19 两半分判**：① C-21 **仍挡本服务**，实证两处（`09-stage-init/todo.md:53` 的 `login_tty` dup2/TIOCSCTTY 封装待扩面、`:66` 的 panic 执行半待 edge 认领）⇒ `⏸ 等编排线`；②「加密后端余量」**查无此项**，只活在 `:3` 摘要句里，正文无条目承接（`grep -rn "加密后端|crypto" 09-stage-init/todo.md` 只命中 `:3`），代码侧无依赖（`os/commands/sbin/init/src/` 搜 `arc4random|crypto|rand_bytes` 为空，C 的 `minix3/servers/init/`+`minix3/cmd/` 同空），本仓唯一 `arc4random` 使用面是 `os/commands/bin/fileops/src/rm.rs` 的 `rm -x` 那一族（另在本板命令面账上）⇒ 按「账面滞后」注销并就地改正摘要句，写明注销依据。**两行都没有标 ✅**（各还有真实等待对象，只是不属本板），也都没有「把 TODO 改 DEFERRED 充数」——每个判据都带 grep 与文件行号。**一处流程偏差如实记着**：A26 与 A19 两个条目 ID 本轮并在一把锁里做，协议 §2 的组合行办法是一次只领一个 ID；本轮两笔都只是文档对账、无并发写面，下一轮起分开领。读数：`minix-init` 门 **149 / 0**（改前改后各一次，本轮读数是「未回归」不是「已修」）、文风增量门 error 级 0、断链总数 222 且本轮改动行不新增断链。板上 §1 命令面此刻只剩 **A2 一条 `☐` 实现行**（27 处 / 23 个源文件，一轮一支）；A9c 的 ②③⑦ 与 A26/A19 均已写明去处；§1b 服务器行需「一轮一条腿」推进。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（A2 渲染半切片 3：`col`）。

- `2026-10-19` qoder / todo 循环第一百一十一笔（批次三百九十七，A2 `GETOPT-REPORT-BYTE-LETTER` **渲染半切片 3：`col`**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `79c460292`，动 `os/commands/usr-bin/textfilter/{src/bin/col.rs,tests/col_face.rs}` + 18 篇批次三百九十七、A 板 A2 行（代码笔 `93675a335`）。共件与 `src/col.rs` 一字未动：`flag_error` 的两处出口（集外字母、`-l` 缺值）改调 `GetoptReport::line_bytes`。**门的重点是一次翻正**：既有的 `nonascii_option_letter_is_reported_as_one_byte`（批次三百二十六立）断的是 lossy 之后的码位 U+00C3，而那恰好只能证到「取法半」——`c3 83`（按字节取、按 char 渲染）与 `c3 a9`（按字符取）在文本夹具里都渲染成替换字符，分辨不了；本轮把它改成逐字节比 stderr（新增 `run_bytes`），形集扩到 `-bé`/`-é`/`-xé` 三形（`col.c:133` 的 optstring 是 `"bfhl:px"`，`b`/`x` 都是合法字母，聚簇会继续吃到 `é` 的第一个字节，参照件 `tmp/col-313/col` 三形现量都是 `unknown option -- ` + `c3` + usage、退 1）。另新立 `-l` 缺值一枚。**本轮量出一件「不可观察」并如实记账**：`MissingValue` 的字母只能来自 optstring（ASCII），所以那一支换不换字节形输出一个字节都不变；负控 M2 就照这条设计成「只把 `MissingValue` 退回 `line()`」的注入，整包 **0 红**，判据表记 **「无分辨力」**而不是给门补一个假期望（新立那枚门本身仍有牙——M7 撤掉换行时它红）。其余六枚：M1 两处都退 `line()` 咬 1、M3 字节位仍按 UTF-8 写咬 4（`col`/`rev`/`colrm` 三枚字节门加库门）、M4 无条件取低字节咬 1、M5 拿到集外字母后不接 usage 咬 4、M6 把 `col` 自己的取法退回按字符取咬 5（证两半确实成对）、M7 丢换行咬 15；逐枚先还原再读回比 md5、复跑 0 失败。A/B 十二形 **SAME=12 / DIFF=0**（含 `-lx`、`-l3x`、`-bfpx`、`--`、裸 `-`、文件操作数六形回归），两形非法 UTF-8 仍差一字节属射程②。读数：宿主与容器权威门各 **1137 / 0 / 68**、三架构带 `-p` 全退 0、clippy 154 同数触及两件零命中、`rustfmt --edition 2024` 新增长零漂移。**闭合与进度**：`COL-NONASCII-OPTLETTER` 一族在 `col` 这一支两半全闭；渲染半 **3 支壳落地、余 26 处调用点 / 22 个源文件**（`col` 一处占两个调用点，本笔前是 28——三百九十六收尾写的「余 27 处」把 `col` 记成了一处，按现量改清）。**另记一处账面滞后**：A2 状态列在三百九十二结案时没从「切片 2 在制」翻走（当时只追加判据格），本轮起结案时两格同批改。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 4：`column`，取法半在批次三百一十四已按字节定）。

- `2026-10-19` qoder / todo 循环第一百一十二笔（批次三百九十八，A2 **渲染半切片 4：`column`**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `9006b1170`，动 `os/commands/usr-bin/textfilter/{src/bin/column.rs,tests/column_face.rs}` + 18 篇批次三百九十八、A 板 A2 行（代码笔 `32f9d65e4`）。`src/bin/column.rs` 的 `Err(report)` 出口改调 `line_bytes`；`column_face` 13→**14**（三形逐字节 + `-c` 缺值一句；`-té` 那形有意选它——`column.c:93` 的 optstring 是 `"c:s:tx"`，`t` 合法所以聚簇继续吃到 `é` 的第一字节，参照件 `tmp/column-314/column` 现量两形都是 `c3`+usage 退 1，本轮并为该参照件补了 `column → column_ref` 的软链以便逐字节比程序名）；A/B 十二形 SAME=12 / DIFF=0；负控七枚各咬住（1/5/1/18/3/16/2）。**本轮最重要的一条是流程教训**：容器权威门第一次跑出 **EXIT=101、六个 target 全红**，红状是「字母位打成 `0xFD`」——`0xFD` 恰为 U+FFFD 的低字节，也就是负控 N3（无条件取低字节）的注入形状。根因**不在码**：我把容器门放后台跑，而同一棵领取树里 `negctl.py` 正在逐枚改写源码，容器把那棵**注入态**的树编了出来。把容器门单跑一次即 **1138 / 0 / 68**、退码 0。⇒ 立纪律：**负控注入器运行期间，同一棵树不得并发起任何 cargo/容器任务**，跑完必须核 `git status --porcelain` 只剩本轮真改动。这一条与批次三百五十四「裁判自己也要有对照」、三百九十四「还原读回写在还原之前」同族：**门与判据的读数要先证明被测对象是干净状态，才有资格谈红绿**。进度：渲染半 4 支壳落地，余 **25 处调用点 / 21 个源文件**；`COLUMN-NONASCII-OPTLETTER` 两半全闭。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 5：`units`，`bin_support_face.rs` 里那枚 `c3 af` 期望随之有意识翻正）。

- `2026-10-19` qoder / todo 循环第一百一十三笔（批次三百九十九，A2 **渲染半切片 5：`units`**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `75557148a`，动 `os/commands/usr-bin/textfilter/{src/bin/units.rs,tests/units_face.rs,tests/bin_support_face.rs}` + 18 篇批次三百九十九、A 板 A2 行（代码笔 `32d0c6306`）。`UnitsArgError::Getopt` 那一处出口改调 `line_bytes`，`-é`/`-qé` 两形与参照件 `tmp/units-319/units` 逐字节相同（`c3` + 那份九行 usage，退 3）；A/B 十形 SAME=10 / DIFF=0；负控七枚各咬住（2/6/1/22/6/2/3，其中 U2「字节位仍按 UTF-8 写」一次咬红**五支壳**的门加库门，是共件真单源的横向证据）；宿主与容器各 **1138 / 0 / 68**、三架构全退 0、clippy 154 同数触及三件零命中。**本轮的一条判据纪律**：本想照 `col`/`colrm` 的样子新立一枚逐字节的门，跑起来才发现 `units_face.rs` 里早有 `off_letter_outside_the_charset_is_reported_by_its_first_byte`——它当年就是为这处失真立的，注释写着「报文里把它渲染成 Latin-1 码点而非原始字节——这一处失真登记为 `UNITS-NONASCII-OPTLETTER`」，断的正是 lossy 后的 U+00C3。于是**撤掉新加的那枚、改翻正既有的**：一条事实一枚门，双胞胎门在未来任一处被抹平时既不改变行为、又会让红名清单含糊（门数因此不涨，仍是 20 枚）。`bin_support_face.rs` 的 `units` 期望随之从 `c3 af` 有意识翻正为 `ef`。**上一笔立的并发纪律本轮照办**：负控跑完并核过 `git status --porcelain` 只剩本轮三件之后才单跑容器权威门，一次过（三百九十八那次并跑，容器把注入态的树编出来报了六个假红靶）。闭合：`UNITS-NONASCII-OPTLETTER` 两半全闭；进度 **5 支壳、余 24 处调用点 / 20 个源文件**；`SEQ-NONASCII-OPTLETTER` 那支只能闭一半——`seq` 是累加型环，lossy 之后把替换字符的三个字节当三个字母连报三行，渲染换了仍剩那一半，另一半在射程②。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 6：`paste`/`tr`/`fold` 里挑一支，`seq` 留到射程②一起做）。

- `2026-10-19` qoder / todo 循环第一百一十四笔（批次四百，A2 **渲染半切片 6：`tr`**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `a05115101`，动 `os/commands/usr-bin/textfilter/{src/bin/tr.rs,tests/tr_face.rs,tests/bin_support_face.rs}` + 18 篇批次四百、A 板 A2 行（代码笔 `7cb6f41bf`）。`src/bin/tr.rs:fail` 里那个**报告向量**的循环改调 `line_bytes`——`tr` 与 `seq` 同构（`TrError::Usage { reports }` 是一个向量），所以渲染半对每一行 libc 报文都生效；`tr` 的 usage 又是**四行硬编码块**（`tr.c:276-282`，后三行七个空格、程序名写死），所以逐字节断言连一个多余换行都看得见。参照件 `tmp/tr-322/tr` 现量 `-é`、`-cé` 都是 `tr: unknown option -- ` + `c3` + 换行 + 四行块、退 1。门 `tr_face` 18→**19**（新立 `an_off_letter_of_a_multibyte_flag_prints_one_raw_byte`：两形逐字节 + ASCII 的 `-Z` 同钉；这一支**没有**现成同类门可翻正，与上一笔 `units` 的「找到既有门就不新增」正好是两种处置，各自判据都写在账上）；`bin_support_face` 的 `tr` 期望从 `c3 af` 有意识翻正为 `ef`。A/B 十一形 SAME=11 / DIFF=0（形集带 `-d a`/`-s a`/`a-z X`/单操作数/`--` 的数据面回归）；负控七枚各咬住（T1 退 `line()` 2 枚、T2 仍按 UTF-8 写 **7 枚=六支壳的门一次全红**、T3 无条件取低字节 1 枚、T4 丢换行 25 枚、T5 不接 usage 块 6 枚、T6 措辞漂移 24 枚、T7 字母先小写化 4 枚）。**两枚第一遍未达标，都归因到判据自己**：T6 的期望里列了 `flag_cluster_is_per_letter_and_order_free`，而那枚门比的是字母集合的语义不是句子文本，措辞改它不红（红了 24 枚不含它完全可解释）→ 换成 `off_letter_gets_one_libc_line_then_the_block`；T7 第一版把 `String::from_utf8_lossy` 直接塞进 `Vec` 初始化处，**编译就没过**（对门的灵敏度既不证明也不否证）→ 换成编得过的「字母 `to_ascii_lowercase`」形状。与批次三百九十四 V4、三百九十五锚点漂移同族：**未达标的负控先分清是注入的问题还是门的问题**。上一笔立的并发纪律照办：容器权威门单跑前先核 `git status --porcelain` 只剩本轮四件真改动。闭合 `TR-NONASCII-OPTLETTER`（该登记的失真本来就只指渲染，`src/tr.rs` 的取法自始是 `char::from(letter)`）；进度 **6 支壳、余 23 处调用点 / 19 个源文件**。读数：宿主与容器各 **1139 / 0 / 68**、三架构全退 0、clippy 154 同数触及三件零命中、rustfmt 新增长零漂移。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 7：`paste`，`src/paste.rs:88` 的取法已按字节；`seq` 留到与射程②同做，只换渲染闭不掉那三行连报）。

- `2026-10-19` qoder / todo 循环第一百一十五笔（批次四百零一，A2 **渲染半切片 7：`paste`**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `89e52f151`，动 `os/commands/usr-bin/textfilter/{src/bin/paste.rs,tests/paste_face.rs,tests/bin_support_face.rs}` + 18 篇批次四百零一与 07 篇三处、A 板 A2 行（代码笔 `0beb24086`）。`src/bin/paste.rs:fail` 那一处出口改调 `line_bytes`：`paste` 的取法自批次三百二十一就按字节（`token[k]` 是一枚 `u8` 再 `char::from`），登记里那条失真只指渲染，所以这一支不存在「取法半」要做。它的环与刚做完的 `tr` 相反——`case '?': default: usage();` 坐在环里（`paste.c:80-82`）⇒ 报文不累加，libc 一行加一行写死程序名的 usage、退 1。参照件 `tmp/paste-321/paste_ref` 现量三形各有讲头：`-é` 报 `c3`、`-sé` 也报 `c3`（`s` 是 `"d:s"` 里的合法字母，聚簇继续吃到 `é` 的第一字节）、`-Zé` **停在 `Z`**（第二个字母轮不到）；另有一形不是错——`-dé` 里 `é` 整个被当 `-d` 的值收走，A/B 带这形就是盯住「换渲染」不许碰取值那半。门 `paste_face` 11→**12**（新立 `the_libc_line_of_a_multibyte_flag_ends_at_its_first_byte`，两形逐字节 + `-Zé` 一形，`run_bytes` 夹具同 `rev`/`tr` 那几支：lossy 的 `String` 分不清裸 `c3` 与 `c3 83`），`bin_support_face` 的 `paste` 期望从 `c3 af` 有意识翻正为 `ef`。**本轮两件记账收获**：① **一枚「只换渲染」的门对取法也有牙**——Q7 把取法改回「按字符取整簇的首字符」（`args[i].chars().nth(k - 1)`），`paste` 的逐字节门与跨支期望同时红（`line_bytes` 对 U+00E9 打 `e9`、对 U+FFFD 退 UTF-8），所以设计注入时不能默认「本轮只动渲染 ⇒ 只有渲染能咬」，这与 `tr` 那笔 T7 钉字母**值**（先小写化）不同型，这枚钉的是字母**宽度的来源**；② **写完先自查冗余再删**：初版门还带 ASCII 的 `-Z` 与缺值的 `-d` 两形，断言与既有的 `off_letter_gets_one_libc_line_and_the_hardcoded_usage`、`missing_value_for_d_is_a_libc_line_then_usage` 逐字相同（纯 ASCII 下文本形与字节形给不出不同读数）⇒ 删；删前删后七枚负控读数一字不变（2/9/1/33/5/30/2），这本身就是「被删的断言没有灵敏度」的证据，与三百九十九 `units` 那笔「既有门已钉同一件事就不新加」同族、方向相反。**两处自撞的工具坑也记**：容器权威门第一次跑成 `EXIT=127`「cargo: executable file not found」——把 `tmp/ci-cargo` 挂到 `/usr/local/cargo` 会把镜像自带的 cargo 挡掉，正解是 §7.3 那条三挂载形（`-u` + `CARGO_HOME=/cargo` + `CARGO_TARGET_DIR=/tgt`）；另一次在领取树里用 `git checkout <commit>` 回退取基线读数，把自己弄成 detached HEAD，回法必须是 `git checkout <branch>`。**回填一处账面滞后**：`07-text-filter.md` 篇首「库测试 564 个」停在批次三百九十，三百九十一那笔已把 `--lib` 记成 **567** 却没回填篇首，本轮按现量改齐（现跑 `--lib` 567/0）。读数：宿主与容器各 **1140 / 0 / 68**（起点 1139）、三架构全退 0、clippy 154 同数且 85 处唯一告警位置里触及三件零命中、`rustfmt --edition 2024` 触及件零漂移（`src/bin_support.rs` 那四块是存量）、A/B 二十二形 SAME=22 / DIFF=0（另两形非法 UTF-8 是射程②在册残差：`ff`/`80` 对 `ef`，退码两侧同为 1）、负控七枚全咬两遍读数相同、五道收尾门除已知探针族漂移外全过（断链 131 与改动前同数）。闭合 `PASTE-NONASCII-OPTLETTER`；`PASTE-NONUTF8-DELIM` 属射程②仍在册。进度 **7 支壳、余 22 处调用点 / 18 个源文件**。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 8：`jot`——`src/jot.rs:252`/`:226` 已按字节取且是单枚报告，改 `bin/jot.rs:46` 一处即可；`vis` 次之，要先拆 `bin/vis.rs:147` 那句 `format!("{}{}", report.line(prog), USAGE)`；`seq` 仍与射程②同轮）。

- `2026-10-19` qoder / todo 循环第一百一十六笔（批次四百零二，A2 **渲染半切片 8：`jot`**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `7f3283ac3`，动 `os/commands/usr-bin/textfilter/{src/bin/jot.rs,tests/jot_face.rs,src/paste.rs,src/bin/paste.rs,tests/paste_face.rs,tests/tr_face.rs}` + 18 篇批次四百零二、07 篇两行、A 板 A2 行（代码笔 `ceedacb50`）。`src/bin/jot.rs` 里 `Err(JotArgError::Getopt(report))` 那一处出口改调 `line_bytes`（取法在 `src/jot.rs:215` 就按字节），参照件 `tmp/jot-317/jot_ref` 现量 `-é` 与 `-cé` 都是 `jot: unknown option -- ` + `c3` + 换行 + usage、退 1。门 `jot_face` 15→**16**（`the_libc_line_of_a_multibyte_flag_is_a_single_raw_byte`：两形逐字节 + `-Zé` 钉环停在第一个坏字母 + `-bé 3` 钉取值那半没被碰；ASCII 两形本文件已有主人，按上一笔的冗余判据不重复承载）。**本轮真正的收获是纠正一处形制归类错**：`src/paste.rs` 模块头、`src/bin/paste.rs` 的形制清单、`tests/paste_face.rs` 与 `tests/tr_face.rs` 的头部注释、`07` 篇的 `paste` 行——四处都把 `jot` 与 `seq` 并列成「BADCH 之后环继续扫、报文累加」那一族，而 `jot.c:150-152` 写的是 `for (;;) { switch (getopt(…)) { default: usage();` ⇒ 报文只可能一行，参照件 `-ZQ` 与 `-Z -Q` 都只报 `Z`；真正累加的只有 `seq`（`errflg++` 后继续扫，`seq_ref -ZQ` 连报两行）。四处同批改判，`jot` 归到与 `paste`/`tr` 同一列。**判据**：机制只能逐家实读，形制归类不许从邻居继承——上一支做完 `seq`，下一支 `jot` 就被顺推了过去，而两家的 `switch` 默认支不同。**两处自错也记全**：① `-bé 3` 的期望我按一行写，宿主门当场红红在三行——`-b` 把那枚词按 `reps` 重复，参照件同三行；根因是**读数时用 `head -1` 截断了**，期望就成了猜的（与批次四百 T6、三百九十四 V4 同族：**截断的输出不能当期望的来源**）；② 注入 J7 第一版 `map_or(char::from(bytes[pos]), |c| c as u8)` 交出 `u8` 而外层要 `char`，**没编过**，换成 `arg.chars().nth(pos).unwrap_or_else(…)` 这个真·按字符形状后才咬住（与批次四百 T7 同坑）。负控八枚各咬住（J1 退 `line()` 1 枚、J2 仍按 UTF-8 写 **8 枚=七支已迁壳的门加共件门一次全红**、J3 无条件取低字节 1 枚、J4 丢换行 38 枚、J5 不接 usage 5 枚、J6 措辞漂移 34 枚、J7 按字符取 1 枚、J8 取错字节 1 枚）；**J8 是新添的一型**——它把字母位取成 `a9` 而不是 `c3`，证明那枚门钉的是「哪一个字节」而非「只有一个字节」。A/B 二十五形 SAME=22 / 非预期 DIFF=0 / **已知分叉 3 形**（裸 `-`、`jot 3`、`-s , 3` 全落在 `JOT-PREC-MINUS-ONE-FORMAT`：C 拼出 `%.-1f`，Minix libc 收成 `%-1.0f` 而宿主 glibc 打回字面量，参照件在该族不作真值——`07` 篇早已写明，脚本据此单独分类不算回归）+ 两形射程②残差（`ff`/`80` 对 `ef`）。读数：宿主与容器各 **1141 / 0 / 68**（容器单跑前核 `git status` 只剩本轮七件）、三架构全退 0、clippy 154 同数且触及六件零命中、rustfmt 新增长零漂移、五道收尾门除已知探针族漂移外全过（断链 131 与改动前同数）。进度 **8 支壳、余 21 处调用点 / 17 个源文件**。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 9：`unifdef`，`src/unifdef.rs:215` 按字节、`bin/unifdef.rs:48` 与 `jot` 同形；`vis` 次之，要先拆 `bin/vis.rs:147` 那句把 libc 行与 usage 拼成一个 `String` 的 `libc_line`；`seq` 仍与射程②同轮；其余十余支取法仍按字符，得两半一起做）。

- `2026-10-19` qoder / todo 循环第一百一十七笔（批次四百零三，A2 **渲染半切片 9：`unifdef`**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `0fe88b3cd`，动 `os/commands/usr-bin/textfilter/{src/bin/unifdef.rs,tests/unifdef_face.rs}` + 18 篇批次四百零三、07 篇 `unifdef` 行、A 板 A2 行（代码笔 `ba67dcfcd`）。`src/bin/unifdef.rs` 里 `Err(UnifdefArgError::Getopt(report))` 那一处出口改调 `line_bytes`（取法 `src/unifdef.rs:215` 自始按字节），参照件 `tmp/unifdef-318/unifdef_ref` 现量 `-é`、`-cé` 都是 `unifdef: unknown option -- ` + `c3` + 换行 + usage、退 **2**（`unifdef.c:378` 的 usage 退 2 不是 1，本支是唯一一支）。门 `unifdef_face` 13→**14**。**本轮抓住的是自己的命名违规**：新门初版照抄 `jot` 那笔的枚名 `the_libc_line_of_a_multibyte_flag_is_a_single_raw_byte`，两枚同名门落在两个集成测试二进制里各自都绿，写负控期望清单时才发现**红名清单无法区分是谁红**（V1 只动 unifdef，期望里却出现 jot 那枚的同名字串），改名 `a_multibyte_flag_letter_reaches_the_libc_line_as_one_byte` 后才对得上账——「枚名跨壳唯一」是批次三百九十二立的老规矩，**靠名字做对账的机制必须在新增那一步执行命名纪律，不能等归因时补**。本支的少见处也进了归一化：`unifdef.c:375-377` 的 usage 那行**程序名写死在串里**（没有 `getprogname()`），而 libc 行与 `err`/`warnx` 的前缀跟着 `getprogname()` ⇒ 参照件里两行来历不同，脚本只换 `unifdef_ref: ` 前缀。负控八枚全咬（V1 退 `line()` 1 枚、V2 仍按 UTF-8 写 **9 枚=八支已迁壳的门加共件门一次全红**、V3 无条件取低字节 1 枚、V4 丢换行 42 枚、V5 不接 usage 4 枚、V6 措辞漂移 37 枚、V7 按字符取 1 枚、V8 取错字节 3 枚），一遍过零判据自错；**V8 是上一笔新添那一型的复用**——把字母位取成 `a9` 而非 `c3`，证门钉的是「哪一个字节」。A/B 二十六形 SAME=23 / 非预期 DIFF=0 / **已知分叉 1 形**（`unifdef -s` 在一枚 `-D`/`-U` 都没给时，参照件由 `findsym` 在查表同时 `printf` 出档里的条件符号名两行、本壳打空而退码一致——`UNIFDEF-SYMLIST-OUTPUT` 第二次现量，读数与批次三百一十八那次相同，本轮只换渲染不碰）+ 两形射程②残差（`ff`/`80` 对 `ef`，退码两侧同为 2）。读数：宿主与容器各 **1142 / 0 / 68**（起点 1141，容器单跑前核 `git status` 只剩本轮两件）、三架构全退 0、clippy 154 同数且触及两件零命中、`rustfmt --edition 2024` 抓到本轮**新写的一处超宽行**（`[a, b].concat()` 单行形）就地修掉后归零、五道收尾门除已知探针族漂移外全过（断链 131 同数）。进度 **9 支壳、余 20 处调用点 / 16 个源文件**。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 10：`vis`——取法已按字节，但 `bin/vis.rs:146` 的 `libc_line()` 把 libc 行与 usage 拼成**一个** `String` 并在六处当错误值返回，要先把那条通道换成字节向量；`cksum` 那两处的 `flag` 来自按字符的取法，得两半一起做；`seq` 仍与射程②同轮）。

- `2026-10-19` qoder / todo 循环第一百一十八笔（批次四百零四，A2 **渲染半切片 10：`vis`**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `a70c2247f`，动 `os/commands/usr-bin/textfilter/{src/bin/vis.rs,tests/vis_face.rs}` + 18 篇批次四百零四、07 篇 `vis` 行、A 板 A2 行（代码笔 `010b9af97`）。本支形状与前九支不同：libc 行与 usage 不是两次 `warn`，而是先拼成**一个错误值**再由唯一出口写出（`bin/vis.rs:146` 的 `libc_line()` 返回 `String`，`parse_vis_args` 的错误通道也是 `String`），所以落码是**整条通道换字节**——`libc_line` 交回 `Vec<u8>`、签名随之外改、三处自创句子的 `String::from(…)` 改 `as_bytes().to_vec()`、调用侧 `support::warn(&message)`；改动仍全在这一支壳内（一处签名 + 六处出口），符合板上「每轮只换一个消费者」。取法本就按字节（`UnknownOption(bytes[pos] as char)`）。门 `vis_face` 19→**20**（`the_libc_line_of_a_multibyte_flag_is_the_first_byte_of_the_flag`：`-é`/`-cé` 逐字节 + `-Zé` 停在 `Z`），并补 `run_streams` 夹具——既有 `run` 把标准错误交回 lossy `String`，够不着字节。**本支没有差分腿**：宿主没有 `vis(3)`，`vis_face` 篇首自始写着「byte-exact expectations are what the C source says」，也就没有 `tmp/vis-*/vis_ref` 可比；本轮把期望的两处来源写进门注释——机制在 `getopt.c:79`（`optopt = *place++`）与 `:103`（`%c`），读数借同族已有参照件的十支壳（`rev`/`paste`/`jot`/`unifdef` 各 ref 对 `-é` 一致交回单个 `c3`）。这不是拿邻居读数替本支作证：`line_bytes` 是同一支共件、机制逐字相同，且本支自己过了八枚负控。**顺带量出一处新的程序名承载差并登记不执行**：C 那行 usage 是 `fprintf(stderr, "Usage: %s …", getprogname())`（`vis.c:136-139`）跟着 `argv[0]`，本层的 `USAGE` 常量把 `vis` 写死 ⇒ 现量把本壳软链成 `vv` 跑 `vv -Z`，第一行 `vv: unknown option -- Z` 跟着 `argv[0]`、第二行仍是 `Usage: vis …`——**同一支命令的两行里只有一行跟 `argv[0]`**。登记 `VIS-USAGE-PROGNAME-HARDCODED`；同族 `paste`/`tr`/`unifdef` 的 usage 在 C 里本来就写死（`paste.c:231`、`tr.c:276-282`、`unifdef.c:375-377`），不算差异——**判据是「C 用没用 `getprogname()`」，不是「本层写没写死」**。**工具行为新记一条**：对 `src/bin/vis.rs` 就地跑 `rustfmt --edition 2024` 会顺着 `#[path = "../bin_support.rs"] mod support;` **把 `bin_support.rs` 一起格式化**（那是别轮留下的存量漂移），`git diff --stat` 现形后 `git checkout --` 还原——**格式化一个壳不等于只改一个壳，收尾必须核 `git status`**。负控八枚全咬（W1 只撤渲染不动拼接 1 枚、W2 仍按 UTF-8 写 **10 枚=九支已迁壳的门加共件门一次全红**、W3 无条件取低字节 1 枚、W4 丢换行 45 枚、W5 只撤拼接不动渲染 3 枚、W6 措辞漂移 39 枚、W7 按字符取 1 枚、W8 取错字节 1 枚），一遍过零判据自错；W1/W5 这对是本支形状特有的分辨——渲染与拼接分开注入。读数：宿主与容器各 **1143 / 0 / 68**（起点 1142）、三架构全退 0、clippy 154 同数且触及两件零命中、rustfmt 触及件零漂移（还原被顺手格式化的文件之后）、五道收尾门除已知探针族漂移外全过（断链 131 同数）。进度 **10 支壳、余 19 处 / 15 个源文件**。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 11：`fold`——现查 `src/fold.rs` 是 `UnknownOption(bytes[pos] as char)` 已按字节，只差渲染，`bin/fold.rs:33`/`:37` 两处出口同换；按字节取而只差渲染的还有 `cksum`/`cut`/`join`/`split`/`unexpand`/`sort`，取法按字符的 `nl`/`uniq`/`head`/`tail`/`comm`/`cmp`/`pr` 要两半一起做；`seq` 仍与射程②同轮。**取法状态一律现查再写**：本轮初稿把 `fold` 写成「按字符、需两半一起做」，凭的是上一轮的印象，查出后改判）。

- `2026-10-19` qoder / todo 循环第一百一十九笔（批次四百零五，A2 **渲染半切片 11：`fold`**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `115cbfbf8`→（换基线后）`c63b08058`，动 `os/commands/usr-bin/textfilter/{src/bin/fold.rs,tests/fold_face.rs,tests/bin_support_face.rs}` + 18 篇批次四百零五、07 篇 `fold` 行、A 板 A2 行（代码笔 `58957782d`、文档笔 `cd86bcc46`）。`bin/fold.rs` 的集外字母与 `-w` 缺值两处出口同改 `line_bytes`（同一支壳的两处，仍算板上说的「一个消费者」；取法 `src/fold.rs:213` 是 `bytes[pos] as char`，本就按字节）。`fold_face` 6→**7**（`the_libc_line_of_a_multibyte_flag_folds_to_one_byte`：`-é`/`-bé` 两形逐字节 + `-Zé` 钉环停在第一个坏字母），`bin_support_face` 的 `fold` 期望从 `c3 af` 有意识翻正为 `ef`。**本轮新配一支参照件**：`tmp/fold-405/`（`build.sh` 把 `minix3/usr.bin/fold/fold.c` 与 Minix 自带 `lib/libc/stdlib/getopt.c` 链在一起，垫片供 `setprogname`/`getprogname`（按 `crt0-common.c:156-165` 的 basename 语义）与 `strlcpy`），并**经 `tmp/fold-405/fold` 软链起**——垫片的 `getprogname()` 从 `/proc/self/cmdline` 现算出的名字就是 `fold`，与本壳二进制同名 ⇒ 报文逐字节直比、不需归一化。写脚本时同时记下一条限制：宿主链接时 `wcwidth` 是**隐式声明**（垫片没给原型），多字节输入的列宽结果不可信 ⇒ A/B 的数据面形集全用 ASCII，本轮判据也全在选项环。**配参照件当场兑现收益：量出一处此前未登记的 C 怪规则**（登记 `FOLD-DIGIT-NOT-FIRST-IN-CLUSTER`，本轮只换渲染不碰）：`fold.c:88-96` 的数字支在「数字不是短横后第一位」时读 `atoi(argv[optind] + 1)`，即**下一个 token 的第二个字符**，且那个 token 随后被 `optind` 越过当成操作数。三形现量：`-b9` ⇒ `argv[optind]` 是 `NULL`，参照件当场段错误（退出码 -11）而本壳按「聚簇里继续累加宽度」交 9；`-b80` ⇒ `atoi("b80")` 得 0，参照件按宽度 0 一行一个字而本壳按 80 打一行；`-b9 80` ⇒ 参照件把 `80` 当档名报 `fold: 80: No such file or directory` 退 1，本壳**同形**（开档失败先撞上，宽度那条规则没机会显形）。**两处过程事件都按不遮盖的写法入账**：① 领用的树是复用了一棵 `08:27` 被中断会话留下的同名树——index **0 字节**、`git status` 报 `index file smaller than expected`，重建 index 后 `git diff HEAD --numstat` 显出 **649 个文件相对 HEAD 只有删除**（工作树被清零），真内容只有未完成的 `bin/fold.rs`（先 `cp` 存证到 `tmp/fold-405/incoming-fold.rs`）；`claim.sh release` 按设计拒绝，照它自己给的办法 `git worktree remove --force` + `prune` + 销锁 + 重新 claim，新树核过与 HEAD 一致才开工。判据：领用树第一件事 `git diff HEAD --numstat`，「成片只有删除」= 树被清零而不是自己有改动；0 字节 index 是 git 进程被中断的指纹，先存证再弃树。② **我自己第二犯老纪律**：主树 `git merge --ff-only` 已经失败（另一条线刚把 `c63b08058` 落进 `rewrite`，分支分叉），我没确认合成就去 `release`，两笔提交一度只剩游离对象。回水四步：`git cat-file -t` 确认对象仍在 ⇒ `git branch recover-batch-405` 先钉住 ⇒ 重新 claim 一棵新树（新基线）先 `numstat` 核干净、`git cherry-pick` 接上两笔、整包与架构门在新基线上重跑（**即便别人那笔动的是 `os/arch/` 与内核、与本包不相交，换基线就得重跑**）⇒ `merge --ff-only` 成功后**逐笔 `git merge-base --is-ancestor` 验在 `rewrite` 上**才 `release`，最后才删临时存证分支。判据：合入失败等于没合入，`release` 只在祖先校验通过之后。读数：宿主与容器各 **1144 / 0 / 68**（起点 1143；本轮第一次整包跑是 1143/1，红在 `no_command_aborts_on_a_nonutf8_option_letter`——我漏翻了跨支表里 `fold` 那行期望，那是那张表的本职，翻正后复跑 0 失败）、三架构全退 0（rebase 后 x86/riscv 各复跑一次退 0）、clippy 154 同数触及三件零命中、`rustfmt --edition 2024` 触及件零漂移、五道收尾门除已知探针族漂移外全过（断链 131 同数、staleness PASS、staged-inflight 干净）、A/B 二十五形 SAME=23 / 非预期 DIFF=0 / 已知分叉 2 形（上面那族）+ 两形射程②残差（`ff`/`80` 对 `ef`，退码两侧同 1）、负控八枚各咬住 2/12/1/45/2/39/2/2（X2 一次咬红**十一支已迁壳的门 + 共件门**）一遍过。进度 **11 支壳、余 17 处调用点 / 16 个源文件**。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 12：`cut`——现查 `src/cut.rs:356` 是 `bytes[pos] as char`，只差 `bin/cut.rs:116` 一处渲染；其后 `join`（`src/join.rs:463` 的 `b[1] as char`）。**上一笔清单里两处按现查改判**：`unexpand`（`arg[pos..].chars().next()`）与 `split`（`tail.chars().next()`）其实按字符，得两半一起做；`sort`/`cksum` 的 `flag` 来源与 `nl`/`uniq`/`head`/`tail`/`comm`/`cmp`/`pr` 一样逐个现查再写；`seq` 仍与射程②同轮）。

- `2026-10-19` qoder / todo 循环第一百二十笔（批次四百零六，A2 **渲染半切片 12：`cut`**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `747b53751`→（换基线后）`2c3db4353`，动 `os/commands/usr-bin/textfilter/{src/bin/cut.rs,tests/cut_face.rs}` + 18 篇批次四百零六、07 篇 `cut` 行、A 板 A2 行（代码笔 `08a839815`、文档笔 `a4aac52b6`）。`src/bin/cut.rs` 里 `CutError::Getopt(report)` 那一处出口改调 `line_bytes`（取法 `src/cut.rs:356` 是 `bytes[pos] as char`，本就按字节）；**跨支表 `bin_support_face` 六支里没有 `cut`**，所以本轮没有期望要翻正——这也说明那张表覆盖面有限，逐支的门仍得各自钉。门 `cut_face` 14→**15**（`the_libc_line_of_a_multibyte_flag_is_one_byte_before_the_three_line_usage`：`-é`/`-sé`/`-né` 三形逐字节 + `-Zé` 停在第一个坏字母 + `-cé` 反面形钉取值那半没被碰）。**又配一支参照件，比 `fold` 多补三样垫片**：`tmp/cut-406/`（宿主没有 `<util.h>`，本地造只给 `strlcpy`/`strlcat` 原型；glibc 只有 `__unused__` 而 NetBSD 写 `__unused`，抄成 `__attribute__((__unused__))`；`cut.c` 把 `x_cut.c` 编两份，字节那份要 BSD `fgetln`、字段那份要 Minix 的 `emalloc`/`ecalloc`/`erealloc`——两份实现沿用 `tmp/paste-321/paste_shim.c` 与 `tmp/family-diag-ref/sort_shim.c` 里已验过的）。经 `tmp/cut-406/cut` 软链起 ⇒ `getprogname()` 算出的就是 `cut`，**报文逐字节直比不需归一化**；C 的 usage 三行本来写死 `cut`（`cut.c:290-296` 没有 `%s`），两侧同文。**顺带盘到一笔库存**：`tmp/family-diag-ref/` 里已有 `head_ref`/`join_ref`/`uniq_ref`/`sort_ref` 四支配好的参照件（批次三百前后留下），后面那几支先复用别重配。**本轮两处判据自错同源，都归到「期望来自印象而不是来自读数」**：① 反面形我最初写 `-cé` 并期望 libc 那一行报 `c3`，门红——参照件现量答的是 `cut: [-bcf] list: illegal list value` 且**不接 usage**，因为 `cut` 的 optstring 是 `"b:c:d:f:sn"`，`b`/`c`/`d`/`f` **四个都带值**、只有 `s`/`n` 不带，而我按刚做完的 `fold`/`jot` 里 `c` 是旗标代入了；改成 `-sé`/`-né` 后绿，`-cé` 留作反面形。**判据：`optstring` 谁带值是逐支的事实，写形之前先在参照件上打一遍那一形**（与批次四百零五 `-bé 3` 那处「`head -1` 截断读数」同族）。② A/B 脚本第一版用 `capture_output` 无上限收，而 `cut -b ''` 在 C 里对每一行**无限打换行**，三秒吐出约 1 GB，Python 当场 `MemoryError`；改成 `timeout=3` 并把「两边都不结束」当作可比读数才跑通 ⇒ **与参照件对打的脚本必须给时间与输出上限**（它会挂死、会刷屏、会段错误——四百零五的 `-b9` 就是 -11）。**两处真分叉现量到手，登记不执行**（都不属 A2 射程，`07` 篇 `cut` 行同步记名）：`CUT-SUPPRESS-STILL-PRINTS-NEWLINE`——`-s` 遇到没有分隔符的行时 C **整行跳过、连换行都不打**（判据 `cut.c:248` 的 `if (!isdelim && !sflag)`、`:66` 的 `static int sflag`），本壳多打一个换行，现量 `printf 'a\tb\tc\nx y\n' | cut -f3 -s` 参照件 `c\n`、本壳 `c\n\n`，不带 `-s` 时两侧同形 ⇒ 缺陷只在 `-s` 那一支；`CUT-EMPTY-BYTE-LIST-RUNAWAY`——`-b ''`/`-c ''` 在 C 里无限输出而本壳每行一个空行后退 0（`-f ''` 两侧同形六字节），这处要与既有那枚 `an_empty_list_is_not_an_error_and_prints_a_blank_line_per_input` 对齐口径，**属一次裁决，不在渲染轮里做**。负控八枚全咬（Y1 退 `line()` 1 枚、Y2 仍按 UTF-8 写 **12 枚=十一支已迁壳的门加共件门一次全红**、Y3 无条件取低字节 1 枚、Y4 丢换行 48 枚、Y5 不接三行 usage 4 枚、Y6 措辞漂移 40 枚、Y7 按字符取 1 枚、Y8 取错字节 1 枚）一遍过——Y7 的注入形状最初写成 `arg[pos..]` 按字节下标切片（落进多字节字符中间会当场 panic），动手前先换成 `arg.chars().nth(pos)`，所以不占未达标名额。读数：宿主与容器各 **1145 / 0 / 68**（起点 1144）、三架构全退 0、clippy 154 同数（落在我方文件那条 `cut_face.rs:170` 的 `iter().copied().collect()` 在改动前的同一句上就有，只是行号偏移，非新增）、rustfmt 触及两件零漂移、五道收尾门除已知探针族漂移外全过（断链 131 同数）、A/B 二十九形 SAME=25 / 非预期 DIFF=0 / 已知分叉 2 形 + 两形射程②残差。**上一笔立的「先核 merge 落地再销锁」这次执行到位**：主树 ff 两次都被别的线推进挡住（`c63b08058`、`2c3db4353`），都在领取树里 `git rebase rewrite` 接上新基线、整包与架构门在新基线重跑（1145/0/68、x86 全退 0）、`merge --ff-only` 成功后逐笔 `git merge-base --is-ancestor` 验过才 `release`。进度 **12 支壳、余 16 处调用点 / 13 个源文件**。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 13：`join`——`src/join.rs:463` 是 `b[1] as char` 已按字节，只差 `bin/join.rs:90` 一处渲染，且 `tmp/family-diag-ref/join_ref` 现成可复用；领用树开工照旧先 `git diff HEAD --numstat` 核树。其后 `cksum`/`sort` 的 `flag` 来源现查再定，`unexpand`/`split` 已确认按字符要两半一起做，`nl`/`uniq`/`head`/`tail`/`comm`/`cmp`/`pr` 逐个现查；`seq` 仍与射程②同轮）。

- `2026-10-19` qoder / todo 循环第一百二十一笔（批次四百零七，A2 **渲染半切片 13：`join`**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `3ed94fd93`，动 `os/commands/usr-bin/textfilter/{src/bin/join.rs,tests/join_face.rs}` + 18 篇批次四百零七（含上一笔的改判）、07 篇 `join` 行、A 板 A2 行（代码笔 `7050d570f`、文档笔 `39ee6a8f7`）。`src/bin/join.rs` 里 `JoinError::Getopt(report)` 那一处出口改调 `line_bytes`（取法 `src/join.rs:463` 是 `b[1] as char`，本就按字节）；`join_face` 29→**30**（`the_unknown_letter_of_a_multibyte_flag_is_written_as_one_byte`：`-é` 逐字节、`-Zé` 停在第一个坏字母、`-aé` 反面形钉取值那半没被碰）；跨支表 `bin_support_face` 没有 `join` 行，所以本轮无期望要翻正。**上一笔的库存盘点被本轮现量推翻，两处文字都留着并在原句就地改判**：`tmp/family-diag-ref/join_ref` 打 `-Z` 交回 `invalid option -- 'Z'`——带引号、措辞是 glibc 的，说明那四支（`head_ref`/`join_ref`/`uniq_ref`/`sort_ref`）链的是**宿主 glibc 的 `getopt`**，而 Minix 的 `getopt.c:101-104` 写的是 `unknown option -- Z`；这一族的判据全在那一行的字母位与措辞上，拿它当差分腿会把**措辞差**读成**字母差** ⇒ 本轮重配 `tmp/join-407/join_ref`（`join.c` + Minix `getopt.c` + 上一轮那套垫片 `util.h`/`__unused`/程序名/`strlcpy`/`emalloc` 族/`fgetln`），经 `tmp/join-407/join` 软链起 ⇒ 程序名同为 `join`，报文逐字节直比。**判据：复用任何既有参照件之前，先打一枚 `-Z` 验它用的是谁的 `getopt`**。**第三处同族判据自错**：反面形 `-aé` 我按邻居 `-1`/`-2` 那句 `option field number…` 写期望，门红——C 在 `-a`/`-v` 两支写的是 `-a option **file** number not 1 or 2`（`join.c:171`、`:217`），只有 `-1`/`-2`/`-j` 写 **field**（`:141`、`:152`、`:186`）：**同一支命令里同一类错有两种措辞，门按各自原句钉**。连续三笔同一个根因（四百零五 `head -1` 截断读数、四百零六 `optstring` 从邻居类推、本笔消息措辞从邻居类推）⇒ **期望必须逐形从读数来**。两处注入端错也按「先归因到注入还是门」处理：Z5 期望里列的 `usage_is_two_lines_with_the_name_only_in_the_first` 走 `JoinError::Usage` 那条腿、根本不经过 `Getopt` 支 ⇒ 归因到期望清单并去掉该项（Z5 仍咬 2 枚）；Z7 第一版没编过（`no_std` 的 `src/join.rs` 没有 `String` 名字空间），换成 `core::str::from_utf8(&b[1..]).map_or(…)` 这个不越字符界的形状后咬住（与批次四百 T7、四百零二 J7 同坑第三次）。**`rustfmt` 的一条新形状**：本轮只往 match 臂里插了四行注释，`--check` 却要求把**同文件另一条臂**（`IllegalOptionToken`）的块形收成单行；把 HEAD 的同一份文件单独拿出来跑是零漂移，说明该决定是整个 match 的排版一起算的 ⇒ **「只加注释」不等于「只有注释变」**，处置是触及件就地格式化、还原被顺带改的 `src/bin_support.rs`、复跑门确认行为未变，收尾以 `git status` 为准。负控八枚全咬（Z1 退 `line()` 1、Z2 仍按 UTF-8 写 **13 枚=十二支已迁壳的门加共件门一次全红**、Z3 无条件取低字节 1、Z4 丢换行 50、Z5 不接两行 usage 2、Z6 措辞漂移 42、Z7 按字符取 1、Z8 取错字节 1）；A/B 二十九形 **SAME=27 / DIFF=0 / 已知分叉 0**——这一族里第一支连数据面都零分叉的壳（选项环十七形 + 数据面十形，脚本一开始就带 `timeout=5` 与输出上限，那是上一笔立的规矩），另两形非法 UTF-8 仍是射程②在册残差（`ff`/`80` 对 `ef`，退码两侧同为 1）。读数：宿主与容器各 **1146 / 0 / 68**（起点 1145，容器单跑前核 `git status` 只剩本轮两件）、三架构全退 0、clippy 154 同数且触及两件零新增、`rustfmt --edition 2024` 触及件零漂移、五道收尾门除已知探针族漂移外全过（断链 131 同数）。合入这一步这次按规矩走：`merge --ff-only` 成功后逐笔 `git merge-base --is-ancestor` 验过才 `release`。进度 **13 支壳、余 15 处调用点 / 12 个源文件**。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 14：`uniq`——现查 `bin/uniq.rs:272` 是 `bytes[j] as char` 已按字节，只差 `bin/uniq.rs:44` 一处渲染；`sort`/`unexpand`/`split`/`cksum`/`comm`/`cmp` 现查结论是按字符要两半一起做，`nl` 的 `letter` 来源再查，`pr` 走自带 `egetopt`（措辞 `illegal option`）单列一支，`seq` 仍与射程②同轮）。

- `2026-10-19` qoder / todo 循环第一百二十二笔（批次四百零八，A2 **渲染半切片 14：`uniq`**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `8f9441c68`，动 `os/commands/usr-bin/textfilter/{src/bin/uniq.rs,tests/uniq_face.rs}` + 18 篇批次四百零八、07 篇 `uniq` 行、A 板 A2 行（代码笔 `5dc9b3d48`、文档笔 `0d6b4e388`）。`src/bin/uniq.rs` 里 `Err(UniqArgError::Getopt(report))` 那一处出口改调 `line_bytes`——**本支的选项环整个住在薄壳里**（取法 `bin/uniq.rs:272` 是 `bytes[j] as char`，本就按字节），所以注入点也全在 `bin/`，与「注入必须打在真正读字母的那一层」一致。门 `uniq_face` 15→**16**（`a_multibyte_flag_letter_is_emitted_as_a_single_byte_by_uniq`：`-é`/`-cé`/`-ué` 三形逐字节 + `-Zé` 停在第一个坏字母；`c`/`d`/`u` 是 C 选项串 `"-cdf:s:u"` 里仅有的三个不带值字母，所以聚簇会继续吃到 `é` 的第一字节——这一条是照着参照件读数写进注释的，不是推的）。**上一笔那条规矩第二次兑现**：`tmp/family-diag-ref/uniq_ref` 打 `-Z` 交回 `invalid option -- 'Z'`（glibc 血脉）⇒ 重配 `tmp/uniq-408/uniq_ref`（`uniq.c` + Minix `lib/libc/stdlib/getopt.c` + 同一套垫片，软链名同为 `uniq` ⇒ 报文逐字节直比）。**本轮是三笔以来第一次零判据自错**：期望全部先在参照件上打过再写进门、门注释里抄 C 原句对照（直接是上一笔 `-a`/`-v` 写 file 而 `-1`/`-2`/`-j` 写 field 那一课的后果），负控八枚的锚点与类型也一次对上——**同一族错误连续三笔之后，写法改对了就不再复现**，这是「判据自错」可以被流程吸收的证据。负控八枚全咬（Z1 退 `line()` 1、Z2 仍按 UTF-8 写 **13 枚=十三支已迁壳的门加共件门一次全红**、Z3 无条件取低字节 1、Z4 丢换行 50、Z5 不接 usage 2、Z6 措辞漂移 42、Z7 按字符取 1、Z8 取错字节 1），复跑 0 失败。A/B 二十三形 **SAME=23 / DIFF=0 / 已知分叉 0**——第二支连数据面都零分叉的壳（选项环十四形含三个不带值字母的聚簇与 `-s`/`-f` 缺值与两个坏值、数据面九形含 `-c`/`-d`/`-u`/`-s 1`/`-f 1`/单档/两档出档/开不了的档名）；出档那一形比的是**写进档的内容**（两边各跑一次、跑完立刻读回，不比进程标准输出）；脚本一开始就带 `timeout=5`（批次四百零六立的规矩）。读数：宿主与容器各 **1147 / 0 / 68**（起点 1146，容器单跑前核 `git status` 只剩本轮两件）、三架构全退 0、clippy 154 同数且触及两件零新增、`rustfmt --edition 2024` 触及两件零漂移（上一笔那条「插注释会改变同文件其它臂排版」的现象本轮没出现）、五道收尾门除已知探针族漂移外全过（断链 131 同数）、合入按规矩先验祖先再销锁。进度 **14 支壳、余 14 处调用点 / 11 个源文件**。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 15：`comm` 或 `cmp`——两支的字母都从 `chars()` 来（`src/comm.rs:151`、`src/cmp.rs:158`），**要两半一起做**：先让环按字节取（与 A11 那笔 `rev` 同型）再换 `line_bytes`；`sort`/`unexpand`/`split`/`cksum` 同型；`nl`/`head`/`tail`/`pr` 的字母来源逐个现查（`pr` 走自带 `egetopt`、措辞 `illegal option`，单列一支）；`seq` 是累加环、仍与射程②同轮。参照件是否现成，先按那条验法打一枚 `-Z` 再说）。

- `2026-10-19` qoder / todo 循环第一百二十三笔（批次四百零九，A2 **切片 15：`comm`，取法半 + 渲染半同轮**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `d97905fe9`，动 `os/commands/usr-bin/textfilter/{src/comm.rs,src/bin/comm.rs,tests/comm_face.rs}` + 18 篇批次四百零九、07 篇 `comm` 行、A 板 A2 行（代码笔 `b671fdb3d`、文档笔 `9c73415f2`）。**A2 从这一支起改节奏**：前十四支的取法都已按字节，只差渲染；`comm` 的环写的是 `for ch in arg[1..].chars()`（`src/comm.rs`），把 `é` 的一个字符当成一枚字母交回，只换渲染会留下「字母是 U+00E9 ⇒ 打 `e9`」这种半对半错。落码三件：环改 `arg[1..].bytes()` 并在报错处 `char::from(other)`（ASCII 四个字母 `1`/`2`/`3`/`f` 行为一字不变）、`src/bin/comm.rs` 那处出口改 `line_bytes`、`tests/comm_face.rs` 加面门与 `run_bytes`。**门两侧各一枚**：库侧 `the_unknown_letter_is_the_first_byte_not_the_first_character` 钉 `parse_comm_args` 交回 U+00C3，面侧 `the_libc_line_of_a_multibyte_flag_is_one_byte_after_the_cluster` 钉整条缝；分成两枚不是繁琐，而是**只改一半也必须红**——负控 X1 只把 `line_bytes` 换回 `line()` 只红面门 1 枚，X7 只把环换回 `chars()` 则面门与库门同红（U+00E9 与 U+00C3 两种读数都过不了面门）。这与批次四百零五 Q7「渲染轮的门对取法也有牙」是同一条规律的另一侧：**牙长在哪儿，注入就要打在哪儿**。参照件两件事：本轮配了 `tmp/comm-409/comm_ref`，同时按上一笔那条规矩验了现成的 `tmp/comm-315/comm_ref`（打 `-Z` 交回 `unknown option -- Z`，Minix 血脉，可用）——**这条规矩第一次同时给出两种结论：排除 glibc 货（`family-diag-ref/*_ref`），确认现成件可用**。A/B 二十一形 **SAME=21 / DIFF=0**，是这一族里第三支连数据面都零分叉的壳（选项环九形含 `-é`/`-1é`/`-Zé`/`--x`/`--`、三列合并与抑制六形、操作数判形与开档两支），另两形非法 UTF-8 仍是射程②在册残差。**本笔两处流程自错，都不遮盖**：① 往库里插新门时多留了一枚 `#[test]`（`#[test] /// 文档 … #[test] fn`），编译器不报错、测试框架却把同一函数**注册两遍**，整包读数从 1147 直接跳到 1149 而新增只有两枚——发现靠的是老规矩「结案前对门数」（库里 `comm::` 前缀跑出 12 条而本轮只加 1 条）。**判据：重复的 `#[test]` 不违反任何编译规则，所以门数对账不能省；插入式改动收尾要看一眼 `git diff` 里的属性行**。② 换壳复用上一支的负控脚本时，**锚点、变量名、标签三样没逐条对**，四轮才对齐：`SITE` 还是 uniq 的那句（在 comm 里根本不存在 ⇒ 两枚注入「未施加」）、标签 `Z5` 忘了改名（红名清单分不清是谁）、两处替换串把 `u8` 塞进 `Option<char>` 且留着 uniq 的 `report` 变量名 ⇒ **没编过**。这已是同族第四、五次（批次四百 T7、四百零二 J7、四百零五 Z7），**一枚没编过的注入对门的灵敏度既不证明也不否证，脚本复用必须逐条重对**。负控终局八枚全咬（X1 只撤渲染 1、X2 共件按 UTF-8 写 **13=十四支已迁壳的门加共件门一次全红**、X3 无条件取低字节 1、X4 丢换行 50、X5 不接 usage 2、X6 措辞漂移 42、X7 只撤取法 3、X8 取错字节 3），复跑 0 失败。读数：宿主与容器各 **1149 / 0 / 68**（起点 1147）、三架构全退 0、clippy 154 同数且触及三件零新增、`rustfmt --edition 2024` 触及件零漂移、五道收尾门除已知探针族漂移外全过（断链 131 同数）、合入按规矩先验祖先再销锁。`comm` 的字母位失真**两半全闭**（此前无独立登记号）。进度 **15 支壳、余 13 处调用点 / 10 个源文件**。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 16：`cmp`——现查 `src/cmp.rs:158` 的 `other` 来自 `chars()`，与 `comm` 同型，两半一起做、两侧各立一枚门；`sort`/`unexpand`/`split`/`cksum` 同型在后，`nl`/`head`/`tail` 的字母来源逐个现查，`pr` 走自带 `egetopt`（措辞 `illegal option`）单列一支，`seq` 仍与射程②同轮）。

- `2026-10-19` qoder / todo 循环第一百二十四笔（批次四百一十，A2 **切片 16：`cmp`，两半同轮第二例**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `fdad2cd10`→（别的线推进后换基线）`cccdf4210`，动 `os/commands/usr-bin/textfilter/{src/cmp.rs,src/bin/cmp.rs,tests/cmp_face.rs}` + 18 篇批次四百一十、07 篇 `cmp` 行、A 板 A2 行（代码笔 `5ffabb536`、文档笔 `386bfc414`）。环从 `arg[1..].chars()` 改成 `arg[1..].bytes()` 再 `char::from(other)`（`cmp.c:74` 的选项串是 `"ls"`，两个字母都不带值，ASCII 行为一字不变），出口改 `line_bytes`；参照件 `tmp/cmp-410/cmp_ref` 本轮配（真源四件 `cmp.c`/`misc.c`/`regular.c`/`special.c` 一起链 + Minix `getopt.c` + 垫片，软链名同为 `cmp` ⇒ 报文逐字节直比）现量 `-é`、`-lé` 都是 `cmp: unknown option -- ` + 单个 `c3` + 换行 + usage、退 **2**（`ERR_EXIT`，本支的退码不是 1），`-Zé` 停在 `Z`。门两侧各一枚：库侧 `the_unknown_letter_is_the_first_byte_not_the_first_character`、面侧 `the_libc_line_of_a_multibyte_flag_is_one_byte_for_cmp`，整包 **1149 → 1151**，**门数对平**（上一笔那枚重复 `#[test]` 造成同一函数被注册两遍的噪声，本轮靠"新增两枚就该 +2"当场对账，没复现）。**上一笔的教训直接改了做法并被读数验证**：那轮换壳复用负控脚本，锚点/变量名/标签三样没逐条对，四轮才对齐；本笔**整份 negctl 手写**（脚本头就写着这件事，锚点全部从本支源码抄、变量名只用本支真有的 `letter`/`other`/`arg`），结果**八枚一遍过、零判据自错**（K1 只撤渲染 1、K2 只撤取法 4、K3 共件按 UTF-8 写 **15=十四支已迁壳的门加共件门一次全红**、K4 无条件取低字节 1、K5 丢换行 54、K6 不接 usage 2、K7 措辞漂移 46、K8 取错字节 4）——**换壳复用脚本省下的时间小于它制造的错判时间**，这条现在有数了。K1/K2 这对也是批次四百零九那条「牙长在哪儿注入打在哪儿」的复现：渲染与取法分开注入，两半各自有牙。**顺带记一枚门的命名问题并登记待整理**：`an_unknown_letter_prints_the_libc_line_then_the_usage` 这枚 ASCII 门的名字在 `cmp_face`/`uniq_face`/`nl_face`/`fold_face` 四个文件里逐字相同（比批次三百九十二「枚名跨壳唯一」那条规矩还早），共享名字会让红名清单失去分辨力——本轮新立的门都不同名所以归因清楚，脚本改用「本支新门 + 失败总数」为准、同名门只当旁证（K6 那行旁证输出）；改名整理登记为 **`CMP/UNIQ/NL/FOLD-ascii-gate-rename`**，属门的命名整理、不属渲染射程，未动。A/B 二十一形 **SAME=21 / DIFF=0**：第四支连数据面都零分叉的壳（选项环六形含 `-é`/`-lé`/`-Zé`/`--x`/`--`、操作数判形三形、数据面十形含 `-l` 全列、`-s` 静默、两者同给的 `errx`、短档 EOF、一个与两个 `skip`、坏 `skip` 值、开不了的档名、`-` 当标准输入），另两形非法 UTF-8 仍是射程②在册残差（`ff`/`80` 对 `ef`，退码两侧同为 2）。读数：宿主与容器各 **1151 / 0 / 68**、三架构全退 0、clippy 154——**基线是把本轮改动 `git stash` 后同题重跑量出来的，也是 154**，所以落在 `src/cmp.rs:78`（`write!` 格式串以换行结尾）与 `src/bin/cmp.rs:261`（test 模块之后还有条目）那两条都在既有构造上、只是行号偏移，非新增；`rustfmt --edition 2024` 只在 `src/bin_support.rs` 报存量、触及三件零漂移；五道收尾门除已知探针族漂移外全过（断链 131 同数）。合入这一步 again 按规矩走：主树 ff 被别的线推进挡住（`cccdf4210`，C 板 C1 行读数更新）⇒ 在领取树 `git rebase rewrite`、整包与架构门在新基线重跑（1151/0/68、x86 退 0）⇒ `merge --ff-only` 成功后逐笔 `git merge-base --is-ancestor` 验过才 `release`。`cmp` 的字母位失真**两半全闭**。进度 **16 支壳、余 12 处调用点 / 9 个源文件**。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 17：`sort`——现查 `src/sort.rs:336` 是 `while let Some(flag) = chars.next()`，两半同轮第三例，环改按字节 + 出口改 `line_bytes`、两侧各立一枚门，负控照本笔手写不复用脚本；`unexpand`/`split`/`cksum` 同型在后，`nl`/`head`/`tail` 逐个现查，`pr` 单列（自带 `egetopt`、措辞 `illegal option`），`seq` 仍与射程②同轮）。

- `2026-10-19` qoder / todo 循环第一百二十五笔（批次四百一十一，A2 **切片 17：`sort`，两半同轮第三例**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `a4641d01b`→（别的线推进后换基线）`a23648e4b`，动 `os/commands/usr-bin/textfilter/{src/sort.rs,src/bin/sort.rs,tests/sort_face.rs}` + 18 篇批次四百一十一、07 篇 `sort` 行与篇首库测数、A 板 A2 行（代码笔 `ba8d15d91`、文档笔 `028081f47`）。环从 `let mut chars = arg[1..].chars().peekable()` 改成按字节下标（`letters[at]` 一次进一个、`match` 臂全换 `b'r'` 这类字节模式、集外与缺值两支交 `char::from(flag)`），`src/bin/sort.rs:263` 出口改 `line_bytes`；门两侧各一枚（库 `sort::tests::the_unknown_letter_is_the_first_byte_not_the_first_character`、面 `the_libc_line_of_a_multibyte_flag_is_one_byte_for_sort`），整包 **1151 → 1153**，`git diff` 新增 `#[test]` 属性行数 = 2 与门数增量对平（批次四百零九那条「插入式改动收尾要看属性行」按现量执行）。**本轮立的新判据：环按字节化之后，「在同一 token 里往后切剩余当值」的那三处（`o`/`k`/`t`/`R`）必须单独论证字符边界**——字节下标的 `tail[at..]` 只有在切点落在边界上才等价于按字符收剩余；这里成立是因为走到取值臂时当前字节必是 ASCII 那四个之一，而任何非法字节先被 `_ =>` 那一臂当场带走。按字符收剩余时这条边界是顺带保证的，所以它以前不是一条要求，现在它是。**`sort` 也是这一族里第一支参照件不可用的壳**：`tmp/family-diag-ref/sort_ref` 按批次四百零七那条验法测出是宿主 glibc 血脉（`invalid option -- 'Z'`），且**选项错的那几形当场段错误、退 -11**，退码与行数都无法背书 ⇒ 差分拆两腿：数据面 21 形逐字节比（程序名段 `sort_ref:` 归一为 `sort:` 后 **SAME=21 / DIFF=0**，覆盖 `-f`/`-i`/`-k 2`/`-k 1,1`/`-t:`/`-s`/`-S`/`-c`/`-m`、坏键句、键越界、`-t` 与记录符相撞、两档名、开不了的档名），选项环四形只比抽出的那一枚字母字节（`Z`/`c3`/`c3`/`Z` 全同），字母位的**绝对**读数交给机制层探针 `tmp/sort-411/probe.c`（编 `minix3/lib/libc/stdlib/getopt.c`、按 `sort.c:148` 的选项串 `"bcdD:fHik:lmno:rR:sSt:T:ux"` 跑，`-é` 交回 `optopt=0303`）。探针把环跑满因而把 `é` 的两个字节**各报一次**（`0303` 紧跟 `0251`），而真实 `sort.c:250-252` 的 `case '?': default: usage(NULL);` 在第一个坏字母就退出，所以报文是一行——**这差别是探针的形状而不是 C 的形状**，已写进门注释免得后来人拿探针读数当报文行数。**第二处新判据来自负控本身**：脚本第一遍跑完发现 CMD 只列了 15 支面试目标，`join_face`/`column_face`/`bin_support_face` 不在内 ⇒「共件坏 ⇒ 各壳面门全红」这句话当时只覆盖了 15 支。补齐重跑后 M3 红名 16→**17**（失败数 19，因为 `colrm_face` 与 `column_face` 那两枚门逐字同名 `the_option_line_of_a_nonascii_letter_carries_one_raw_byte`，红名清单合并成一项——上一笔登记的**同名门失去分辨力**第二次现量），M5 失败 **63**；顺带量出 `bin_support_face` 里当时数出来的三枚门对渲染半不红——**这句在切片 20 被现量改正**：同一文件里的 `no_command_aborts_on_a_nonutf8_option_letter` 逐字节钉 `-\xff` 那族读数，共件一坏它就红，所以跨支表对渲染半是有牙的；漏数源于用一条 grep 数门而不是按跑出来的清单数（**判据：门清单由执行结果列，不由印象列**）。**判据：结案时核对「已迁壳清单 ⊆ 负控运行范围」**，否则「全红」是打折的。八枚全咬住（M1 只撤渲染 1、M2 只撤取法 2、M3 共件按 UTF-8 写 17、M4 无条件取低字节 1、M5 丢换行 63、M6 不接 usage 3、M7 措辞漂移 54、M8 取错字节 2）、一遍过、复跑 0 失败。读数：宿主与容器各 **1153 / 0 / 68**（容器日志 `tmp/sort-411/container.log`）、三架构 `--bins` 全退 0、clippy 整包 **154** 与基线同数（`sort` 关键字只匹到 `tsort.rs:97` 那枚既有告警，与本支无关）、`rustfmt --edition 2024` **本轮是 `src/sort.rs` 自己有排版差**（`MissingValue` 三处折行形状变了）故就地格式化，而 `--check src/bin/sort.rs` 报的仍是 `#[path]` 顺进去的 `src/bin_support.rs` 存量（批次四百零四那条规矩第二次撞上，处置不变：不动它、收尾以 `git status` 为准；格式化后宿主整包复跑仍 **1153 / 0**）；五道收尾门除已知探针族漂移（`探针命名族` 26 对基线 18，**在主树 `rewrite` 上同读数**，故属已知存量漂移非本轮引入）外全过，断链 131 同数。**又一处账面滞后被按现量补齐**：07 篇首那句「库测试 567 个」停在 rev 那一轮，此后 `comm` +1、`cmp` +1 都没回填，本轮数出等式 **567 + 1 + 1 + 1 = 570** 与 `--lib` 现跑读数逐字对上后改成 **570**（「结案时正文与账面同批改」第三次执行）。合入按规矩：主树 ff 被别的线推进挡住（`a23648e4b`，C17/A6 对账笔，只动 C 板一个文件、与本轮六件零重叠）⇒ 领取树 `git rebase rewrite` ⇒ 整包门在新基线重跑 **1153 / 0 / 68** ⇒ `merge --ff-only` 成功后逐笔 `git merge-base --is-ancestor` 验过才 `release`。`sort` 的字母位失真**两半全闭**。进度 **17 支壳、余 11 处调用点 / 8 个源文件**（现量：`cksum` 2、`head` 1、`nl` 1、`pr` 1、`unexpand` 2、`tail` 1、`seq` 1、`split` 2）。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 18：`unexpand`——本轮已顺手现查：它的环**本身按字节走**（`pos` 是字节下标），失真只在交回报文那一句 `src/unexpand.rs:283` 的 `arg[pos..].chars().next().unwrap_or('-')`，取法半是一行改动（`char::from(arg.as_bytes()[pos])`），渲染半是 `src/bin/unexpand.rs:91`/`:95` 两处；带值字母只有 `t` 且 `MissingValue('t')` 是写死的字母，不受影响。负控继续手写，且先按「已迁壳 17 支 ⊆ CMD 列表」核对运行范围。`split`/`cksum` 在后，`nl`/`head`/`tail` 逐个现查，`pr` 单列（自带 `egetopt`、措辞 `illegal option`），`seq` 仍与射程②同轮）。

- `2026-10-19` qoder / todo 循环第一百二十六笔（批次四百一十二，A2 **切片 18：`unexpand`**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `dbc02b80e`，动 `os/commands/usr-bin/textfilter/{src/unexpand.rs,src/bin/unexpand.rs,tests/unexpand_face.rs}` + 18 篇批次四百一十二、07 篇 `unexpand` 行与篇首库测数、A 板 A2 行（代码笔 `06a8ab9b1`、文档笔 `b368a53b2`；本笔里程碑是 `add852cc6`。领取基线 `dbc02b80e` 之后 C 板推进了一笔 `83bb1603d`（只动 C 板与 `TODO-LEDGER-OPEN.md`，与本轮七件零重叠）⇒ 领取树 `git rebase rewrite` 后三笔换基为上述哈希，故此处记的是**落地哈希**而非注入时的哈希）。取法半是一行：`src/unexpand.rs:283` 的 `arg[pos..].chars().next().unwrap_or('-')` 改成 `char::from(bytes[pos])`（本支的环本就按字节下标走，失真只在把字母交回的那一刻）；渲染半是 `src/bin/unexpand.rs:91`/`:95` 两处 `line_bytes`。两半各一枚门（库 `unexpand::tests::the_unknown_letter_is_the_first_byte_not_the_first_character`、面 `the_libc_line_of_a_multibyte_flag_is_one_byte_for_unexpand`），整包 **1153 → 1155**（`--lib` 571、`unexpand_face` 12）。**本轮两条新立的规矩**：① 既有库测的题注写着「集外字母只报第一个**字符**」——改动让一句题注变成反面时必须同笔改题注，否则文档把缺陷写成契约（改成「第一个字节」）；② `-t` 缺值那一支在本壳**没有可观察的字母**（选项串 `"at:"` 里带值的只有 `t`、句子字母位写死 ASCII），所以那半是形态统一而非行为修复，**不为它造一枚只会绿的门**，并把这句话写进台账与 07 篇免得后来人去找不存在的牙。**负控第一次整轮没跑起来却被读成「八枚全不咬」，这是本轮最重要的一处自错**：脚本 CMD 把 `-j 1` 写成了一个 argv（`"-j 1"`），cargo 报 `could not parse ` 1``（退 101）且**不产出任何 `test result:` 行**，而脚本的「编得过」判据只看 `error[E`/`could not compile` ⇒ `失败数=0` 看起来像基线正常、八枚全票未达标、还原复跑也「0 失败」。**判据：每轮注入前后都必须先证「这套门真的被执行过」**（`test result:` 出现且执行门数 > 0）——没跑起来的注入既不能证明也不能否证任何门的灵敏度，它是批次四百零九「没编过的注入不算证据」那条的更隐蔽形态（那时尚且能看见「未施加」，这次连红都没红）。补了这道前置后又错一次：门数只数 `test result: ok.` 那一形，于是「跑起来且大面积红」（U3/U5/U7 各 20/68/58 枚失败，每个分区都至少一枚红）被算成 0 枚执行 ⇒ 三枚真咬住的注入被判未达标；**计数要按实际执行，`ok` 与 `FAILED` 两形都算——红的时候恰恰是跑得最对的时候**。两处都改完八枚一次全咬（U1 只撤渲染 1、U2 只撤取法 2、U3 共件按 UTF-8 写 **失败 20/红名 18**、U4 无条件取低字节 1、U5 丢换行 68、U6 不接 usage 4、U7 措辞漂移 58、U8 取 token 末字节 5），并按上一笔新立的规矩**先把运行范围核对成「已迁壳 18 支全列」**，于是「共件坏 ⇒ 全红」这句话第一次真的覆盖 18 支（红名 18 = 17 支各一枚 + 共件门；`colrm_face` 与 `column_face` 那两枚同名门仍合成一行，是 `CMP/UNIQ/NL/FOLD-ascii-gate-rename` 的第三条证据）。**A/B 用现成件**：按批次四百零七那条验法先打 `-Z` ⇒ `tmp/unexpand-312/unexpand_ref` 交回 `unknown option -- Z`（Minix `getopt.c` 血脉），确认件可用、不必重配（「既排除 glibc 货也确认现成件」第二次复现）。数据面与开档腿 20 形 **SAME=19 / DIFF=1**，唯一那形**不是**本轮射程，而是 A/B 撞出的真缺陷：**`-t 0` 参照件回显 `0 0` 后原样输出并退 0，本壳在 `src/unexpand.rs:105` 的 `stops.last() - 1` 上溢出减 panic（进程退 -6）** ⇒ 新登记 **`UNEXPAND-ZERO-STOP-PANIC`**（P0-code-bug；上限折算成减法这一步 C 根本不存在，所以修法要先量 C 在停位含 0 时 `tabify` 的逐形行为，换成 `saturating_sub` 会未经读数就给出「0 也是合法停位」这个答案）；按「一次只修一条」与「发现新 bug 退回正确性线」，它不进本轮、排在 A2 剩余壳之后作独立一轮。选项环七形**字母位全同**（`-é` 与 `-aé` 都是单个 `c3`、`-Zé` 停在 `Z`、反面 `-té` 两侧同句 `Invalid tabstop `<c3 a9>'` 且不接 usage），退码与句子也逐字节同框；两形非法 UTF-8 仍是射程②在册残差（`ff` 对 `ef`，退码同为 1）。读数：宿主与容器各 **1155 / 0 / 68**、三架构 `--bins` 全退 0、clippy 整包 **154** 与基线同数（落 `src/unexpand.rs:163`/`:166` 那两条是既有 `implicitly performing saturating subtraction`，即 `UNEXPAND-SATURATING-DEC` 的在册存量，行号未因本轮改动漂移）、`rustfmt --edition 2024` 触及三件零漂移；五道收尾门除已知探针族漂移（26 对基线 18，主树同读数）外全过，断链 131 同数。`unexpand` 的字母位失真**两半全闭**。进度 **18 支壳、余 9 处调用点 / 7 个源文件**（现量：`cksum` 2、`split` 2、`head`/`nl`/`pr`/`tail`/`seq` 各 1）。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 19：`split`——现查 `src/split.rs:328` 是 `tail.chars().next().unwrap_or('-')`，两半同轮第四例，它的环也带四个「切剩余当值」的字母（`l`/`a`/`b`/`n`，`MissingValue` 那几枚字母是写死的），按批次四百一十一那条边界判据逐个查切点；`cksum` 随后（两处渲染 + `RequiresHash`/`IllegalOptionValue` 两句里 `{flag}` 也是字母位，一并现查取的是字节还是字符），`nl`/`head`/`tail` 逐个现查，`pr` 单列（自带 `egetopt`、措辞 `illegal option`），`seq` 仍与射程②同轮）。

- `2026-10-19` qoder / todo 循环第一百二十七笔（批次四百一十三，A2 **切片 19：`split`，两半同轮第四例**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `827733337`，动 `os/commands/usr-bin/textfilter/{src/split.rs,src/bin/split.rs,tests/split_face.rs}` + 18 篇批次四百一十三、07 篇 `split` 行与篇首库测数、A 板 A2 行（代码笔 `915340fd6`、文档笔 `7bac9dbd7`；本笔里程碑在主树 ff 落地**之后**才写，所以记的是落地哈希——批次四百一十二那次先写后 rebase 导致哈希漂移、补了一笔更正，这次的次序把它避免了）。取法半是一行：`src/split.rs:330` 的 `tail.chars().next().unwrap_or('-')` ⇒ `char::from(bytes[1])`（环只看短横后那一枚字节）；渲染半是 `src/bin/split.rs:564`/`:568` 两处 `line_bytes`。两半各一枚门，整包 **1155 → 1157**（`--lib` 572、`split_face` 29，属性行增量对平），题注两处「只报第一个字符」同笔改「字节」（批次四百一十二那条规矩第二次执行）。「切剩余当值」的边界按切片 17 的判据查过：走到取值臂时 `bytes[1]` 必是 ASCII，下标 2 必在边界上；`MissingValue('l'|'a'|'b'|'n')` 四句的字母写死 ASCII ⇒ 那里的 `line_bytes` 是形态统一而非行为修复，不设只会绿的门。**A/B 的夹具判据自错一处，先归因到夹具**：第一版每形都跑在空的 case 目录（`in.txt` 只放在上一级），于是「操作数一档/两档/三档」量的其实是「档名不存在」，而 `split` 的输出是**往当前目录写的分片文件**——修正为每形先在 case 目录落一份 `in.txt`、并比「退码 + stderr 原文 + 目录里的文件名 + 每个文件的 md5」之后数据面 21 形 **SAME=21 / DIFF=0**，选项环六形整条 stderr 逐字节同串。**判据：差分夹具的目录内容本身就是被测行为的一部分，造形之前先问「C 在这一步读的是哪个路径」。**纠正过程撞出一处真分叉并**登记不执行** `SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`：C 选项环之后先 `open` 第一支操作数（`split.c:135-138` 的 `err(1, "%s", *argv)`）、到第三支才 `usage()`（`:148-149`），所以「第一支开不了 + 三支操作数」C 报档名而本壳报 usage（退码两侧同为 1）；反证同形量过——第一支可开时两侧同报 usage，分叉只落在这一形。负控八枚全咬住（S1 只撤渲染 1、S2 只撤取法 2、S3 共件按 UTF-8 写 **失败 21/红名 19**、S4 无条件取低字节 1、S5 丢换行 73、S6 不接 usage 4、S7 措辞漂到 glibc 62、S8 取 token 末字节 5），运行范围按批次四百一十二两条新规矩核对：已迁壳 19 支的面试目标全列进 CMD（含跨支表 `bin_support_face`），基线打印「执行门数 883」以证明确实跑了，`executed_count` 把 `ok` 与 `FAILED` 两形都算；逐枚编得过且跑起来、还原 md5 相符、复跑 **0 失败**。参照件是现成的 `tmp/split-311/split_ref`，先按批次四百零七那条验法打 `-Z`（交回 `unknown option -- Z`，Minix `getopt.c` 血脉）再经同名软链 `tmp/split-413/split` 起 ⇒ 程序名段一致、直比（「验归属既排除 glibc 货也确认现成件」第三次复现）。读数：宿主与容器各 **1157 / 0 / 68**、三架构 `--bins` 全退 0、clippy 整包 **154** 与基线同数（落 `src/bin/split.rs:361`/`:400` 那两条在 HEAD 同款构造上、位置未漂移）、`rustfmt --edition 2024` 只在 `src/split.rs` 与 `tests/split_face.rs` 就地格式化（`src/bin/split.rs:26` 的 import 排版差是 **HEAD 存量**，`git show HEAD` 同段逐字相同，未动它以免顺带改到 `#[path]` 包进来的 `bin_support.rs`），格式化后宿主整包复跑仍 1157/0；篇首「库测试」571 → **572** 与 `--lib` 现跑读数同批改（第四次执行）；五道收尾门除已知探针族漂移（26 对基线 18，主树同读数）外全过，断链 131 同数。**另记一处工具行为（`tools/` 不在本线射程，只登记）**：文风门 SL-4 剥行内码是**从左成对**，而 A2 那一整行的反引号数在改动前后都是奇数（既有存量），于是「同一句里带反引号的日期」是否被判裸日期取决于它之前反引号的总数——同一种写法上一笔过门、这一笔不过。处置是把切片记录里的日期去掉（批次号与里程碑本就承载时间），判据：**往反引号不成对的长行追加内容时，行内码里的日期不可靠，时间信息放在成对的短字段里**。`split` 的字母位失真**两半全闭**。进度 **19 支壳、余 7 处调用点 / 6 个源文件**（现量：`cksum` 2、`head`/`nl`/`pr`/`seq`/`tail` 各 1）。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 20：`cksum`——`src/bin/cksum.rs:71`/`:76` 两处渲染，且它的 `RequiresHash`、`illegal argument to -X option` 这几句自印报文里也带字母位，要一并现查每一句的字母取自字节还是字符；环在 `src/cksum.rs`，先查它是 `bytes()` 还是 `chars()`。`head`/`nl`/`tail`/`seq` 各一处渲染而 `src/bin/` 里没有 `UnknownOption(` 构造 ⇒ 字母来自库侧的 parse 返回值，取法半要去库里找；`pr` 单列（自带 `egetopt`、措辞 `illegal option`），`seq` 仍与射程②同轮。负控继续手写，基线必须打印执行门数。）

- `2026-10-19` qoder / todo 循环第一百二十八笔（批次四百一十四，A2 **切片 20：`cksum`，只换渲染的一支**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `b62813e63`，动 `os/commands/usr-bin/textfilter/{src/cksum.rs,src/bin/cksum.rs,tests/cksum_face.rs}` + 18 篇批次四百一十四、07 篇 `cksum` 行与篇首库测数、A 板 A2 行（代码笔 `af9137377`、文档笔 `ddbcc9942`；里程碑仍在 ff 落地之后写，记的是落地哈希）。现查与上一笔的预判不同：`cksum` 的环**本来就按字节**（`src/cksum.rs:334` 是 `let flag = bytes[pos] as char;`，与 `getopt.c:79` 的 `optopt = *place++` 同形），所以取法半没有码要改，改的只有 `src/bin/cksum.rs` 的两处出口（`UnknownOption`/`MissingValue` → `line_bytes`）。门两枚：面侧 `the_libc_line_of_a_multibyte_flag_is_one_byte_for_cksum`（`cksum_face` 27→28）钉整条缝，库侧 `cksum::tests::the_unknown_letter_is_the_first_byte_not_the_first_character` **钉的是「已经是那样」的取法**——这一枚是新做法：不为改动立门，而为「别把这一半改坏」立门；它的价值在负控里被量出来（C7 把 `bytes[pos] as char` 换成 `arg[pos..].chars().next().unwrap()` 后面门与库门同红 2 枚，若只有面门红就说不清坏的是哪一半）。整包 **1157 → 1159**（`--lib` 573、`cksum_face` 28，属性行增量对平）。**负控第一次放等价变异**：`-a`/`-o`/`-s` 缺值那几句的字母是写死的 ASCII，把那一处出口换回 `line()` 是没有可观察差的改动，C2 现量「编得过、跑起来、失败数 0」⇒ 切片 18 那句「那里的 `line_bytes` 是形态统一而非行为修复」现在有测量背书而不是断言（与「不造只会绿的门」配成一对：不假造门，但可以用**预期不红**的注入去证明某处确实没有牙）。**上一笔的一句话被本轮否证，三处产物同批改**：批次四百一十一那句「`bin_support_face` 的三枚门对渲染半一枚都不红 ⇒ 跨支表挡不住渲染回归」不成立——同一文件 `tests/bin_support_face.rs:65` 的 `no_command_aborts_on_a_nonutf8_option_letter` 逐字节钉 `-\xff` 那族跨壳读数，本轮 C3（共件按 UTF-8 写）与 C5（丢行尾换行）都让它红。漏数的原因写清楚：当时是用一条 `fn [a-z_]*(` 的 grep 去数那个文件里有几枚门，模式没覆盖到它。**判据：门清单要由跑一遍的结果来列，不能由一条 grep 的印象来列**（改正落在 18 篇批次四百一十一那条、A 板 A2 的切片 17 记录、里程碑第一百二十五笔三处）。又一处「期望来自印象」的自错，被门当场挡住：新库门第一版把 ASCII 反例写成 `-q` 报集外字母，而 `q` 在 `cksum.c:193` 的 `"a:cno:pqs:twx"` 里是**合法字母**（读数 `left: Ok(…print_quiet: true…)`），改成选项串里真没有的 `z`——同族第六次（四百零五、四百零六、四百零七、四百零八、四百一十一各一次，本轮一次），根因同一句：**期望必须逐形从读数来**，「顺手挑一个 ASCII 字母」也算期望。A/B 是本族第一支连选项环整条 stderr 都逐字节同串的壳：参照件用现成的 `tmp/cksum-hash-state/cksum_ref`（先打 `-Z` 验归属：`unknown option -- Z`，Minix `getopt.c`；再经同名软链 `tmp/cksum-414/cksum` 起 ⇒ 程序名段一致），数据面与开档腿 24 形 **SAME=24 / DIFF=0**（标准输入与一档名、`-n`/`-p`/`-q`/`-t`/`-x`、`-a md5` 与 `-amd5`、`-a nosuch`、`-a md5 -p`、`-a md5 -s x`、requires-hash 三形、`-o 1`/`-o 2`/`-o 3`、`-c` 合档与不合档、`-c -w`、开不了的档名、`--`、裸 `-`），选项环十形**字母位同且整串同**（`-é`/`-né` 都是单个 `c3`、`-Zé` 停在 `Z`、`-aé`/`-sé` 走各自自印句、`-a`/`-s`/`-o` 三形缺值），九行 usage 块（`cksum.c:535-549`）逐字节对上；非法 UTF-8 一形仍是射程②在册残差（`ff` 对 `ef`，退码同为 1）。负控八枚 = 七枚咬住 + 一枚等价：C1 只撤渲染 1、C2 等价 0、C3 共件按 UTF-8 写失败 22/期望 20 名全中、C4 取低字节 1、C5 丢换行失败 77/20 名、C6 措辞漂到 glibc 失败 65/3 名、C7 取法改回按字符 2、C8 取 token 末字节 2（清单外另红都是钉整条 stderr 的既有门，只报告不判负）；基线打印「执行门数 912」以证明确实跑了，逐枚还原 md5 相符、复跑 **0 失败**，范围按切片 18 的规矩列满已迁壳 20 支 + 共件 + 跨支表。读数：宿主与容器各 **1159 / 0 / 68**、三架构 `--bins` 全退 0、clippy 整包 **154** 与基线同数且**本轮触及件零告警**（`grep '^  --> ' | grep cksum` 无命中）、`rustfmt --edition 2024` 只就地格式化 `src/cksum.rs` 与 `tests/cksum_face.rs`（`src/bin/cksum.rs:20` 那条 import 排版差是 **HEAD 存量**，`git show HEAD` 同段逐字相同，不动它以免顺带改到 `#[path]` 包进来的 `bin_support.rs`）；篇首「库测试」572 → **573** 与 `--lib` 现跑同批改（第五次执行）；五道收尾门除已知探针族漂移（26 对基线 18，主树同读数）外全过，断链 131 同数，文风增量门 error 级 0。`cksum` 的字母位失真**渲染半闭合、取法半经现量确认本已按字节并被门钉住**；既有 `CKSUM-*` 与 `UNEXPAND-ZERO-STOP-PANIC`、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE` 三条登记本轮未动。进度 **20 支壳、余 5 处调用点 / 5 个源文件**（现量 `grep -rn "\.line(" src/bin/ | grep -v line_bytes`：`head`/`nl`/`pr`/`seq`/`tail` 各 1）。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 21：`head`——`src/bin/head.rs:45` 一处渲染，而 `src/bin/` 里没有 `UnknownOption(` 构造 ⇒ 字母来自库侧的 parse 返回值，要去 `src/` 里现查它是 `bytes()` 还是 `chars()` 交回；若已按字节就照本支做法补一枚「钉现状」的库门。`nl`/`tail`/`seq` 同法，`pr` 单列（自带 `egetopt`、措辞 `illegal option`），`seq` 仍与射程②同轮；负控继续手写、必带等价变异一枚、基线必须打印执行门数。）

- `2026-10-19` qoder / todo 循环第一百二十九笔（批次四百一十五，A2 **切片 21：`head`，只换渲染的一支**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `2b7f88c96`，动 `os/commands/usr-bin/textfilter/{src/window.rs,src/bin/head.rs,tests/head_face.rs}` + 18 篇批次四百一十五、07 篇 `head` 行与篇首库测数、A 板 A2 行（代码笔 `70e74fa8c`、文档笔 `c6ddede1c`）。取法半经现查**已按字节**（`parse_head_args` 的环交回 `bytes[pos] as char`，同文件的 `parse_tail_args` 也一样，顺带把切片 22 的现查做完了），渲染半只有一处——`head` 的两类报文共用同一个臂 `Err(HeadArgError::Getopt(report))`，所以 `src/bin/head.rs:45` 改 `line_bytes` 一句就同时覆盖集外字母与缺值。门两枚：面侧 `the_libc_line_of_a_multibyte_flag_is_one_byte_for_head`（`head_face` 8→9）钉整条缝，库侧 `window::tests::the_unknown_letter_is_the_first_byte_not_the_first_character` 钉「已经是那样」的取法（负控 H2 把它换成 `token[pos..].chars().next()` 后两枚同红，说明这枚门不是装饰）；整包 **1159 → 1161**（`--lib` 574、`head_face` 9，属性行增量对平）。**等价变异这一笔不放，并写明形状为什么不允许**：上一笔那枚之所以成立，是因为 `cksum` 有两处出口而其中一处（缺值）的字母写死 ASCII；`head` 两类报文共用同一处出口，没有「换回去也不该红」的第二处可放——规矩随之从「必带等价变异一枚」改成「**能放就放，放不了就说明形状，不为凑数硬塞**」（上一笔的措辞在本笔被形状否证，这本身就是一次校正）。参照件重配 `tmp/head-415/head_ref`（`minix3/usr.bin/head/head.c` + Minix `lib/libc/stdlib/getopt.c` + 垫片 `__dead`/`__COPYRIGHT` 与 `getprogname()` 的 `/proc/self/cmdline` basename 语义，经同名软链 `head` 起 ⇒ 报文整条直比）：现成的 `tmp/family-diag-ref/head_ref` 打 `-Z` 交回 glibc 那句带引号的 `invalid option -- 'Z'`、且它 usage 里的程序名是 `?` ⇒ **双重不可用**，这是先验归属规矩的第三次兑现（它此前已两次排除 glibc 货、两次确认现成件可用，这次是第一种情形）。**本笔最重的一处是文档-代码-测试同侧的错判被参照件当场推翻，新登记 `HEAD-NON-PERMUTE-OPERAND-TAIL`（P0-fact + P0-code-bug）**：07 篇 `head` 行与批次一百六十六的台账都写着「`head f -5` 里的 `-5` 因 `obsolete()` 只折前导那一串而走成字母 `5` 那一句」，而 `tests/head_face.rs` 的 `unknown_option_prints_the_libc_line_then_the_usage_line` 与 `window.rs` 里 `parse_head_args(&["f", "-5"])` 那条断言**正好钉着这个行为**。参照件现量相反：`head a.txt -5` 打 `==> a.txt <==` 横幅 + 十行内容 + `head: -5: No such file or directory`、退 1。机制在 `getopt.c:74-78`——Minix 的 `getopt` **不重排**，第一个非选项 token 一到就叫环结束、`optind` 停在那里，其后 token 全是操作数；`obsolete()` 只重写前导那一串是另一件事，不能把后面的 token 变成选项。**登记不执行**（按「一次只修一条」）：修它要同时改 `parse_head_args` 的扫描、翻那两枚门、回填 07 篇与批次一百六十六，四处在同一件事上；07 篇那句本轮已改成正确表述并挂上登记号，同一处还改掉「两边对无尾换行的末行处理不同」——换上 Minix 参照件后 `head noeol.txt` 的补换行是**与 Minix 的分叉**，落在既有在册项（C 的 `head()` 逐字符 `putc` 原样输出）而不是 GNU 差异上。A/B（`python3 tmp/head-415/ab.py`，日志 `ab.log`）：数据面与开档腿 18 形 **SAME=16 / DIFF=2**，两枚 DIFF 逐一是上述新登记、一是既有在册项，没有来历不明的分叉；选项环八形字母位同且**整条 stderr 逐字节同**（`-Z`、`--x`、`-é`→单个 `c3`、`-qé`→`c3`、`-Zé` 停在 `Z`、`-cé` 走 `illegal byte count` 不接 usage、`-n`/`-c` 缺值）；非法 UTF-8 一形仍是射程②在册残差（`ff` 对 `ef`，退码同为 1）。负控八枚全咬住（H1 只撤渲染 1、H2 取法改回按字符 2、H3 共件按 UTF-8 写失败 23/期望 22 名、H4 无条件取低字节 1、H5 丢换行失败 79/22 名、H6 不接 usage 失败 2/1 名、H7 措辞漂到 glibc 失败 67/3 名、H8 取 token 末字节 2）：期望清单这次把跨支表 `no_command_aborts_on_a_nonutf8_option_letter` 一并列入（切片 20 改正过的那条事实），基线打印「执行门数 922」以证明确实跑了，逐枚还原 md5 相符、复跑 **0 失败**。读数：宿主与容器各 **1161 / 0 / 68**、三架构 `--bins` 全退 0、clippy 整包 **154** 与基线同数（`src/window.rs:123`/`:835`/`:836` 三条位置都在本轮改动之前，属存量）、`rustfmt --edition 2024` 触及三件零自差、篇首「库测试」573 → **574** 与 `--lib` 现跑同批改（第六次执行）；五道收尾门除已知探针族漂移（26 对基线 18，主树同读数）外全过，断链 131 同数，文风增量门 error 级 0。`head` 的字母位失真**渲染半闭合、取法半经现量确认本已按字节并被门钉住**。进度 **21 支壳、余 4 处调用点 / 4 个源文件**（现量 `grep -rn "\.line(" src/bin/ | grep -v line_bytes`：`nl`/`pr`/`seq`/`tail` 各 1）。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 22：`tail`——`src/bin/tail.rs:638` 一处渲染；取法半本笔已顺带现查为 `bytes[pos] as char`（`src/window.rs:625`）⇒ 照 `head` 的做法补一枚「钉现状」的库门，`tail` 的选项面另有 `-b`/`-c`/`-f`/`-n`/`-r`/`-q`/`-v`/`-x` 多枚字母与 legacy `+` 形，聚簇面比 `head` 宽，A/B 造形要按 `tail.c` 的 ARG 宏逐形来。`nl`/`seq` 随后，`pr` 单列（自带 `egetopt`、措辞 `illegal option`），`seq` 仍与射程②同轮；另有两条 P0 独立轮在册：`HEAD-NON-PERMUTE-OPERAND-TAIL`、`UNEXPAND-ZERO-STOP-PANIC`。）

- `2026-10-19` qoder / todo 循环第一百三十笔（批次四百一十六，A2 **切片 22：`tail`，只换渲染的一支**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `9ac01c1fd`，动 `os/commands/usr-bin/textfilter/{src/window.rs,src/bin/tail.rs,tests/tail_face.rs}` + 18 篇批次四百一十六、07 篇 `tail` 行与篇首库测数、A 板 A2 行（代码笔 `c1b8c47a7`、文档笔 `131a75501`）。取法半本已按字节（`src/window.rs:625` 的 `parse_tail_args` 交回 `bytes[pos] as char`，切片 21 做 `head` 时顺带现查到），渲染半只有一处——集外字母与缺值共用同一个臂 `TailError::Getopt(report)`，所以 `src/bin/tail.rs:638` 一句 `line_bytes` 覆盖两支；门两枚：面侧 `the_libc_line_of_a_multibyte_flag_is_one_byte_for_tail`（`tail_face` 20→21）钉整条缝，库侧 `window::tests::the_tail_unknown_letter_is_the_first_byte_not_the_first_character` 钉「已经是那样」的取法（含反面的 `-né`/`-bé` 值路）；整包 **1161 → 1163**（`--lib` 575）。等价变异本笔也不放，理由与 `head` 同形：两类报文共用一处出口，没有「换回去也不该红」的第二处。参照件配了 `tmp/tail-416/tail_ref`：五件真源 + Minix `lib/libc/stdlib/getopt.c` + 垫片，编译障碍两处都记进配方——真源的 kqueue 腿本来用 `#if !defined(__minix)` 划界，加 `-D__minix` 关掉并把 `sys/event.h` 那层做成空占位头；`misc.c` 的 `xerrx`/`xwarnx` 转调 glibc 没有的 `vwarnx`，由垫片按 BSD 语义给出（顺带补 `SIZE_T_MAX` 与 `stdint.h`）。现成的 `tmp/tail-follow-ref/tail_ref` 打 `-Z` 交回 glibc 那句带引号的 `invalid option -- 'Z'` ⇒ 判不可用，**验归属规矩第四次兑现**；据此把 `tail_face.rs` 里那句「这一句的真值只能来自 C 源本身」改成「现在有逐字节的差分腿可依」——它当年正是因为没有 Minix 血脉的参照件才写成那样。**两处判据自错都被读数当场抓住**：① 本笔负控由上一笔脚本派生，替换 `HeadArgError::Getopt(...)` 时只改了**第一处**，`TAKE_BY_CHAR`/`TAKE_LAST_BYTE` 两个常量里还留着它 ⇒ T2/T8 **没编过**（新加的「执行门数」一栏直接显示 `编得过=False 执行门数=0`，一眼见底）——同族第五次（批次四百 T7、四百零二 J7、四百零五、四百零九），这次的形状是「替换的广度没核对」，**凡同一段文本在文件里出现多次必须数遍**；② `tail.c:108` 的选项串 `"Fb:c:fn:r"` 里 `n` 后面那个冒号我读错一次，一度以为 `-né` 该走集外字母那支、于是怀疑本壳与 C 分叉，参照件读数（`tail: illegal offset -- é`）加重数一遍才定下 **`b`/`c`/`n` 三个都带值**——optstring 要逐字符数，不能凭印象归类（第二次，第一次是批次四百零六给 `cut` 配参照件）。A/B（`ab.py`，日志 `ab.log`）：数据面与开档腿 28 形 **SAME=27 / DIFF=1**，唯一那形 `tail noeol.txt` 参照件原样输出 `tail-no-newline`、本壳多打一个换行 ⇒ 新登记 **`TAIL-FINAL-LINE-ADDS-NEWLINE`**（与 `head` 那条同族；07 篇此前把它写成「与 GNU 比对时的豁免项」，换上 Minix 血脉参照件后升格为与真源的分叉，措辞已改）；选项环十二形字母位同且**整条 stderr 逐字节同**（`-Z`、`--x`、`-é`、`-ré` 都是单个 `c3`、`-Zé` 停在 `Z`、`-né`/`-bé` 走 `illegal offset` 不接 usage、`-b`/`-n`/`-c` 三形缺值、两次 `-n`、`-r` 与坏值并给）；非法 UTF-8 一形仍是射程②在册残差（`ff` 对 `ef`，退码同为 1）；`-f`/`-F` 不入差分——跟随形两边都靠超时切断，比出来的「同」只是都被杀掉（真机跟随腿归 C-57/E5，既有登记）。负控八枚全咬住（T1 只撤渲染 1、T2 取法改回按字符 2、T3 共件按 UTF-8 写失败 24/期望 23 名全中、T4 无条件取低字节 1、T5 丢换行失败 81/23 名、T6 不接 usage 失败 2/1 名、T7 措辞漂到 glibc 失败 69/3 名、T8 取 token 末字节失败 3/2 名），基线打印执行门数 944、逐枚还原 md5 相符、复跑 **0 失败**；运行范围这次把 `head_face` 补回列表（派生时被改名替换吃掉，切片 21 那条「先核对已迁壳 ⊆ 运行范围」的规矩正好拦下）。读数：宿主与容器各 **1163 / 0 / 68**、三架构 `--bins` 全退 0、clippy 整包 **154** 与基线同数、`rustfmt --edition 2024` 触及三件零自差（`src/window.rs` 与 `tests/tail_face.rs` 就地格式化后复跑仍 1163/0）、篇首「库测试」574 → **575** 同批改（第七次执行）；五道收尾门除已知探针族漂移外全过，断链 131 回基线（本笔新写的 `sys/event.h` 尖括号写法被笔记断链检查当 autolink，已改成不带尖括号的措辞并把这条观察记进台账），文风增量门 error 级 0。`tail` 的字母位失真**渲染半闭合、取法半经现量确认本已按字节并被门钉住**。进度 **22 支壳、余 3 处调用点 / 3 个源文件**（现量 `grep -rn "\.line(" src/bin/ | grep -v line_bytes`：`nl`/`pr`/`seq` 各 1）。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 23：`nl`——`src/bin/nl.rs:54` 一处渲染，字母来源本笔没核过：先现查环住在 `src/bin/nl.rs` 还是库里、交回的是字节还是字符，已按字节就照 `head`/`tail` 补一枚钉现状的门，按字符就两半一起做。`seq` 与射程②同轮（累加环，`seq.c:101-102` 的 `"f:hs:t:w"`），`pr` 单列（自带 `egetopt`、措辞 `illegal option`）；三条 P0 独立轮在册：`HEAD-NON-PERMUTE-OPERAND-TAIL`、`UNEXPAND-ZERO-STOP-PANIC`、`TAIL-FINAL-LINE-ADDS-NEWLINE`。派生负控脚本时替换文本出现多次必须数遍。）

- `2026-10-19` qoder / todo 循环第一百三十一笔（批次四百一十七，A2 **切片 23：`nl`，只换渲染的一支、本支没有库门**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `e9cadfb70`→（别的线推进后换基）`8967c54a8`，动 `os/commands/usr-bin/textfilter/{src/bin/nl.rs,tests/nl_face.rs}` + 18 篇批次四百一十七 + A 板 A2 行（代码笔 `8c66111da`、文档笔 `123300178`）。取法半本已按字节（`src/bin/nl.rs:136` 的 `let letter = bytes[pos] as char;`），渲染半只有一处 `getopt_leg`（集外字母与缺值两句共用同一个臂），改 `line_bytes` 一处即覆盖两支；门只一枚面侧 `the_libc_line_of_a_multibyte_flag_is_one_byte_for_nl`（`nl_face` 10→11），整包 **1163 → 1164**，`--lib` 仍 575 —— **本轮库里没加门，篇首那个数就不动**（与批次四百一十二「无可观察差不造门」是同一条规律的另一侧：不造门，也不虚报门数）。没有库门的理由写清：`nl` 的选项环整个住在薄壳里（`:136` 取字母、`:249` 交出），库里没有可单测的 parse 函数，硬造要先改结构，超出 A2 射程；与批次四百零八的 `uniq` 同一类。**新配参照件 `tmp/nl-417/nl_ref` 当场逮出两处分叉，其中一处是 P0**——本族此前没有 Minix 血脉的 nl 参照件（`tmp/nl-306/` 只留下负控脚本与条目，批次三百零六那批读数直接回 C 源推），也就是说 **`nl` 的数据面从来没有被逐字节对过**：① **`NL-OPERAND-CONTENT-DROPPED`**：`nl t.txt` 参照件正常编号、本壳**什么都不输出且退 0**，根因在 `src/bin/nl.rs:275` 的 `if let Err(code) = support::read_file(f) { … }`——只处理失败侧，**`Ok` 的字节被丢弃**；标准输入那一腿用的是 `input.extend(support::read_stdin())`，两腿形状不一致正是线索。既有门覆盖不到它：`nl_face` 里与操作数有关的两枚只钉「开不了报 `Cannot open …`」与「多于一个操作数是 usage」，**没有任何一枚断言可读档名的编号输出**。按「一次只修一条」不进本射程，排下一轮，并且它是三条在册 P0 里最重的（静默丢内容重于报文少一个字节）。② **`NL-DELIM-LENGTH-RULE`（真源缺陷形，待裁决不自动照抄）**：`-d` 给不足两个字符的值（`:`、`x`、空串）时参照件报 `nl: invalid delim argument -- <值>` 退 1、本壳接受；机制在 `nl.c:157-167`——判「超过两个字符」写的是 `if (optarg[2] != '\0')`，值为 0 或 1 字符时那个下标**越过串尾读下一个字节**（argv 块里字符串紧挨着放，通常非零），效果就变成「长度不足 2 也拒」；`-d ::` 与 `-d ab` 两侧都接受。与 `FOLD-DIGIT-NOT-FIRST-IN-CLUSTER`、`CUT-EMPTY-BYTE-LIST-RUNAWAY` 同族：C 的缺陷形是否照抄要一次裁决。③ 另两处 DIFF（`-b pt`/`-b pn`/`-h pn -f pn`）不是新账：本壳那几句是 `nl: number_regex (-b/-h/-f p…) not wired` 的占位报文，正是台账在案的「选项级未接线」条目，A/B 只记录其可见形状。**两处派生脚本自错都被本轮判据当场抓住**：锚点缩进照抄上一笔（`tail` 的渲染点在 match 臂里是 12 空格，`nl` 的 `getopt_leg` 是函数体只有 4 空格）⇒ N1/N6 报「锚点没命中 ⇒ 未施加」，一眼见底、不混进咬住数；把 `tail_face` 换成 `nl_face` 时**漏留了 `tail_face`** ⇒ N3/N5 的期望里 tail 那枚没红，被「未命中」清单直说。两处合起来是同一条：**派生脚本后逐锚点数出现次数，并把运行范围与已迁壳清单做一次集合核对**（上一笔立的「数遍次数」挡住了没编过那类，但没挡住缩进；范围核对这条挡住了漏列）。A/B 另加一条本笔新写的自查：**任一侧退码 127/255 就标 `NOT-RUN` 而不是 `DIFF`**——写脚本前手工比对时我把「壳侧二进制还没建、bash 报 127」读成过「输出为空」，这类假读数从此由脚本自己挡。负控八枚全咬住（N1 只撤渲染 1、N2 取法改回按字符 1（**只有面门红，正是「面门承载两半」的现量形状**）、N3 共件按 UTF-8 写失败 25/期望 24 名全中、N4 取低字节 1、N5 丢换行失败 84/24 名、N6 不接 usage 失败 3/1 名、N7 措辞漂到 glibc 失败 71/3 名、N8 取末字节失败 2/1 名），基线打印执行门数 954、逐枚还原 md5 相符、复跑 **0 失败**。A/B 数据面 26 形 SAME=19 / DIFF=7（七形逐条归因到上述 ①②③，没有来历不明的分叉），选项环九形字母位同且整条 stderr 逐字节同；非法 UTF-8 一形仍是射程②在册残差（`ff` 对 `ef`，退码同为 1）。读数：宿主与容器各 **1164 / 0 / 68**、三架构 `--bins` 全退 0、clippy 整包 **154** 与基线同数、`rustfmt --edition 2024` 只在 `tests/nl_face.rs` 就地格式化（`src/bin/nl.rs` 零自差）；五道收尾门除已知探针族漂移外全过，断链 131 同基线，文风增量门 error 级 0。**合入这一步按规矩走并另记一次自己的操作错误**：主树 ff 被别的线推进挡住（`8967c54a8`，docs(01) boot-shim 口径，与本轮两件零重叠）⇒ 领取树 `git rebase rewrite`、整包与 x86 架构门在新基线重跑（1164/0/68、退 0）⇒ `merge --ff-only` 成功后逐笔验祖先才 `release`；而这条命令链里我用 `;` 混在 `&&` 后面，导致链中断后门脚本在**主树**跑了一次（报出的是别的线在制的 `01-stage-kernel/todo.md` 等文件，与本轮无关，主树内容一字未改、`git diff` 空）——教训是**收尾链不要用 `;` 混 `&&`**，且跨目录的门必须在**本线树**里跑，否则读数会归错人。`nl` 的字母位失真**渲染半闭合**。进度 **23 支壳、余 2 处调用点 / 2 个源文件**（`pr`、`seq`）。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（按 P0 优先：**先修 `NL-OPERAND-CONTENT-DROPPED`**——`src/bin/nl.rs:275` 把 `read_file` 的 `Ok` 字节接回 `input`，并新立一枚「档名内容进输出」的面门，它同时补上操作数腿一直没有门覆盖的那块；负控要能咬住「只丢内容」与「只丢 stderr 号」两种形状。之后再回 A2 收尾 `pr`（自带 `egetopt`、措辞 `illegal option`）与 `seq`（累加环 + 射程②同轮）。`NL-DELIM-LENGTH-RULE` 需要一次裁决，不自动照抄 C 的越界读。）

- `2026-10-19` qoder / todo 循环第一百三十二笔（批次四百一十八，P0 独立轮：**`NL-OPERAND-CONTENT-DROPPED` 闭**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `f802ba8dc`，动 `os/commands/usr-bin/textfilter/{src/bin/nl.rs,tests/nl_face.rs}` + 18 篇批次四百一十八 + A 板 A2 行（代码笔 `047f7fef9`、文档笔 `bc4b4c164`）。缺陷是上一轮新配的参照件当场逮出来的：`src/bin/nl.rs` 的操作数腿写的是 `if let Err(code) = support::read_file(f) { 报案; terminate(1) }`——**只处理失败侧，成功侧的 `Vec<u8>` 被当场丢弃**，于是 `nl file` 什么都不输出还退 0；同函数里标准输入那一腿用的是 `input.extend(support::read_stdin())`，两腿形状不一致正是线索。改成 `match support::read_file(f) { Ok(bytes) => input.extend(bytes), Err(code) => {…} }`，与 C 的形状对齐：`nl.c:241-252` 是 `if (!(fp = fopen(...))) err(...)`，之后整条编号流水线都读同一个 `fp`，档名与标准输入没有两条腿。门两枚新立：`a_readable_operand_is_numbered_into_stdout`（标准输入给空、输出必须是档名的编号内容——不给空就分不清内容从哪来）与 `an_operand_without_a_trailing_newline_still_reaches_the_output`（末行不带换行也要进输出；**这枚刻意只断 `starts_with("     1\\tx")`**，因为参照件打 `     1\\tx` 而本壳补一个换行，那是另一个在册分叉，不能借这枚门把缺陷钉成契约）。整包 **1164 → 1166**（`--lib` 仍 575，两枚都在 `nl_face`：11 → 13）。**这块空洞为什么存在至今**：`nl_face` 里与操作数有关的两枚只钉失败侧（`Cannot open \`…'` 的三段形、多于一个操作数是 usage），成功侧从来没有「内容真的到输出」的门——**失败侧有门、成功侧没有门，就是缺陷能静默通过的形状**；这与本线反复遇到的是同一课（批次四百零五给 `fold` 接读缝、批次三百九十二给 `rev` 换字节形时都撞过「接线类改动只钉报文不钉数据」）。A/B 复跑（同一份 `tmp/nl-417/ab.py`，日志改存 `tmp/nl-418/ab.log`）：数据面 **SAME 19 → 21 / DIFF 7 → 5**，「一档名 / `--` 之后当档名 / 两个操作数 / 前缀为空串」这几形的内容为空分叉全部消失；剩余五形逐条落账——`-b pn`/`-b pt`/`-h pn -f pn` 是 `nl: number_regex … not wired` 的占位报文（在册的「选项级未接线」条目本身）、`-d :` 是 `NL-DELIM-LENGTH-RULE`（C 用 `optarg[2]` 越界读判长度，要不要照抄需一次裁决）、`末行缺换行` 是新登记的 **`NL-FINAL-LINE-ADDS-NEWLINE`**（`printf x > f; nl f` 参照件 `     1\\tx`、本壳 `     1\\tx\\n`，机制在写出循环无条件 `push(b'\\n')`）。**建议把 `HEAD`/`TAIL`/`NL` 三条末行项并成一轮修**：同一件「末行换行状态」不该在三处各改一遍，并且并轮能把「行腿是否保留输入末行的换行状态」一次判清。负控五枚全咬住（`python3 tmp/nl-418/negctl.py`，整份手写）：F1 整段换回只处理 `Err` 的形状 ⇒ 咬 2（两枚新门同红）；F2 结构在但接进输出的是空字节 ⇒ 咬 2；F3 接内容而失败那腿静默 ⇒ 咬 1（`an_unreadable_operand…`）；F4 把档名读成标准输入 ⇒ 失败 3、期望 2 名全中；F5 回归上一笔的共件注入 ⇒ 失败 25，抽查共件门 + 跨支门 + `nl` 面门三枚全红，证明本笔改动没把上一轮的牙钝化。**本笔没有等价变异可放**，理由与上一笔同形：只有一个可观察面（档名内容进不进标准输出）。锚点唯一性在注入前先数一遍（`FIXED` 1 次，且同一趟打印出注入目标形状在源码里 0 次）——这是批次四百一十七那条「派生脚本先数次数」的延续执行。读数：宿主与容器各 **1166 / 0 / 68**（68 段）、三架构 `--bins` 全退 0、clippy 整包 **154** 与基线同数且 `src/bin/nl.rs` 零命中、`rustfmt --edition 2024` 触及两件零自差；五道收尾门除已知探针族漂移（26 对基线 18）外全过，断链 131 同基线，文风增量门 error 级 0（一处「修复前」的措辞被 SL-5 挡住，改写成不叙修复史的说法后过门——这条规则连台账也不豁免，记下来）。**在册 P0 现余四条**：`HEAD-NON-PERMUTE-OPERAND-TAIL`、`UNEXPAND-ZERO-STOP-PANIC`、`TAIL-FINAL-LINE-ADDS-NEWLINE` + 本轮新增 `NL-FINAL-LINE-ADDS-NEWLINE`（后两条与 `HEAD` 那条同族，建议并轮）、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`，另有 `NL-DELIM-LENGTH-RULE` 待裁决。A2 余两支：**切片 24 = `pr`**（`src/bin/pr.rs:186` 一处渲染；`pr` 走它自带的 `egetopt`（`minix3/usr.bin/pr/egetopt.c`），措辞是 `illegal option -- <字母>`，**字母位的取法必须在 `egetopt.c` 里现查**，不能套 libc 那一份 `getopt.c:79` 的结论），**切片 25 = `seq`**（累加环，与射程② argv 字节化同轮）。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 24：`pr`——先读 `egetopt.c` 的 `optopt` 取法与那两句报文形状，再决定是只换渲染还是两半同轮；参照件要连 `egetopt.c` 一起链，别把 libc 的 `getopt` 链进去当差分腿。）

- `2026-10-19` qoder / todo 循环第一百三十三笔（批次四百一十九，A2 **切片 24：`pr`，走自带 `egetopt` 的一支**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `38a07860a`，动 `os/commands/usr-bin/textfilter/{src/pr.rs,src/bin/pr.rs,tests/pr_face.rs}` + 18 篇批次四百一十九、07 篇 `pr` 行与篇首库测数、A 板 A2 行（代码笔 `a6d564b75`、文档笔 `24d2986b9`）。取法本已按字节（`src/pr.rs:523` 的 `bytes[pos] as char`），只换渲染一处——`src/bin/pr.rs` 的 `PrError::Getopt(report)` 一个臂同时接集外字母与缺值。门两枚：面侧 `the_libc_line_of_a_multibyte_flag_is_one_byte_for_pr`（`pr_face` 15→16）、库侧 `pr::tests::the_illegal_letter_is_the_first_byte_not_the_first_character`（`--lib` 575→576），整包 **1166 → 1168**。**这一支把「机制层证据要跟着代码走」摆到台面上**：`pr` 不链 libc 的 `getopt`，它自带 `minix3/usr.bin/pr/egetopt.c`，字母位是 `:110` 的 `eoptopt = (int)*place++` 一次进一个字节、`:148` 的 `%c` 原样写出，措辞也是它自己的 `illegal option -- <字母>`——前 23 支的锚点是 `getopt.c:79`/`:103`，套过来就是拿别人的代码给自己背书。负控因此多了一枚 pr 专用方向的注入 **P7**：把 `GetoptReport::IllegalOption` 的措辞漂到 libc 那句 `unknown option` ⇒ 三枚门同红（共件措辞单源门 + 字节形门 + 本支面门），钉住「两支措辞不能互相套」；它与批次四百一十四那枚「漂到 glibc」是同一条测试的两侧。参照件从零配出 `tmp/pr-419/pr_ref`（`pr.c` + `egetopt.c` + 垫片），唯一链接障碍是 `raise_default_signal`：**原型 `int raise_default_signal(int)`**（`minix3/include/util.h:127`，调用点 `pr.c:1576-1581` 带着信号号），垫片按 NetBSD 那份的四步语义实现（挡所有信号 → 设成 `SIG_DFL` → `raise` → 恢复掩码）；第一版凭印象写成 `void (void)` 并塞了一张含 glibc 没有的 `SIGEMT`/`SIGCPU`/`SIGSIZE` 的表，编译当场拒绝——**判据：补位真源也要逐签名从真源来**，与「期望必须逐形从读数来」同族，只是这次的对象是垫片。**A/B 先量清可比性再写结论**：带表头的形里参照件打本机时间串（`Oct 19 16:52 2026  Page 1`）而本壳那一栏按设计留给壳注入（打 ` Page 1`），这一栏不可能逐字节相同；于是分两趟跑——**`-t` 关表头下 16 形逐字节全同**（缺省、`-l 66`、`-h T`、`-2`、`-2 -a`、`-m`、`-w 40`、`-o 3`、`-e`、`-i`、`-F`、`+2`、一档名、两档名、`-s:`、`-l 10 -2`），带表头那 14 形只在这一栏分叉 ⇒ 记为**验证边界**，不登记成分叉；`-r`/`-p`/`-T` 三形走 `pr: option form not modelled yet` 的占位报文（在案的未接线条目）。选项环十六形**整条 stderr 逐字节同**（`-é`/`-dé` 单个 `c3`、`-Zé` 停在 `Z`、`--x` 报 `-`、`-wé` 那句以空格结尾的自印句、`-h`/`-w`/`-s` 缺值与四个坏值句）。负控八枚全咬住（P1 1、P2 2、P3 失败 26/期望 25 名全中、P4 1、P5 失败 86/25 名、P6 失败 2/1 名、P7 失败 4/3 名、P8 2/2 名），基线打印执行门数 974、逐枚还原 md5 相符、复跑 **0 失败**；**派生脚本的老坑这一笔连踩两次，都被本轮自查拦住**：① 改名替换把 `--test nl_face` 从运行范围里抹掉 ⇒ P3/P5 的期望少命中 1 名，「未命中」清单直接点名（同一类错误的连续第二次，说明「运行范围与已迁壳清单做集合核对」这条正在产生价值）；② 标签里嵌了一段英文双引号 ⇒ `SyntaxError`，编译器当场拒绝、不进门。读数：宿主与容器各 **1168 / 0 / 68**（68 段）、三架构 `--bins` 全退 0、clippy 整包 **154** 与基线同数且触及件零告警、`rustfmt --edition 2024` 两件就地格式化后零残留、篇首「库测试」575 → **576**（第八次同批执行）；五道收尾门除已知探针族漂移（26 对基线 18）外全过，断链 131 同基线，文风增量门 error 级 0。`pr` 的字母位失真**渲染半闭合、取法半经现量确认本已按字节并被门钉住**。**A2 只剩最后一支**：切片 25 = `seq`（`src/bin/seq.rs:60` 一处渲染；它的环是**累加形**——`seq.c:101-102` 的 `"f:hs:t:w"` 配 `errflg` 累加，一个 token 里多个坏字母各报一行，这与其余各壳「停在第一个」相反），且 `-\xff` 那一形压着**射程②**（`bin_support::args` 两条腿都先 `from_utf8_lossy`，原字节在到环之前就没了；判据现量是 `ff` 对 `ef`），按原计划把射程②与 `seq` 同轮做掉；做完重数剩余调用点应为 **0 处**。P0 独立轮在册：`HEAD-NON-PERMUTE-OPERAND-TAIL`、`UNEXPAND-ZERO-STOP-PANIC`、`HEAD/TAIL/NL` 三条末行项（建议并成一轮）、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`；`NL-DELIM-LENGTH-RULE` 待裁决。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 25 + 射程②：先把 `bin_support::args` 改成按字节交回 argv（保留一个「要文本时才 lossy」的下游口），再改 `seq` 那一处渲染，两侧各立门并逐形量 `-\xff`/`-é`/多坏字母累加三族；改完重数 `.line(` 调用点，若归零就把 A2 行整行结项。）

- `2026-10-19` qoder / todo 循环第一百三十四笔（批次四百二十，A2 **切片 25：`seq`，环是累加形的那一支；textfilter 的渲染半到此收口**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `4c2455c43`，动 `os/commands/usr-bin/textfilter/{src/seq.rs,src/bin/seq.rs,tests/seq_face.rs,tests/bin_support_face.rs}` + 18 篇批次四百二十、07 篇 `seq` 行/§门清单/篇首库测数、A 板 A2 行（代码笔 `2b1ca9fe1`、文档笔 `45820c975`）。渲染点一处（`src/bin/seq.rs:63` 的 `for report in reports` 圈，集外字母、缺值、非法格式三类报文共用这一处出口），取法半现查本就逐字节（`src/seq.rs:262` 的 `token = args[i].as_bytes()`、`:275` 的 `let letter = token[k];`、`:279` 交出 `char::from(letter)`），所以取法侧只加一枚**钉现状**的库门——它断言的正是「已经是那样」的取法，日后被改回去时能单独归因。门两枚：面侧 `the_libc_lines_of_a_multibyte_flag_are_one_byte_each_for_seq`（`seq_face` 15→**16**，`-é`/`-Zé`/`-aé` 三形逐字节）、库侧 `seq::tests::the_unknown_letters_are_the_first_bytes_not_the_first_characters`（`--lib` 576→**577**），另**翻正两枚既有门**：`every_byte_of_a_nonascii_cluster_is_reported` 从 char 形（`c3 83`）翻成 `c3`/`a9` 两个裸字节各一行、`bin_support_face::no_command_aborts_on_a_nonutf8_option_letter` 的 `seq` 行从 `\xc3\xaf` 翻成 `\xef`（lossy 后那一串的第一个字节）；`SEQ-NONASCII-OPTLETTER` 随本轮闭。整包 **1168 → 1170**。**这一支特有的形状是「累加」**：`seq.c:101-113` 把坏字母累加进 `errflg`、出环才统一接一行 usage，与其余各壳「停在第一个」相反，所以渲染错的后果不止是字母那一个字节，而是**行数**——负控为此单加一枚 **S6**：把 `for report in reports` 改成只取第一枚 ⇒ 三枚门同红（含 `the_ring_keeps_going_and_accumulates_the_libc_lines`），证面门对行数也有牙；另 **S8 是一枚声明等价的注入**（把缺值那点的取法改成按字符取，零枚红是正确读数——能走到缺值必先在该 `OPTSTRING` 里命中字母，而 optstring 全 ASCII，那一点没有分岔可言），留它在清单里是为了让「这一点没有牙」有读数而不是有说法。其余六枚 S1—S5、S7 各咬住（3/4/27/1/27+清单外 41/4 名），逐枚还原 md5 相符、复跑 **0 失败**、基线打印执行门数 1001。**A/B 十一形 SAME=5 / DIFF=6，六形同因**（参照件 `tmp/seq-320/seq_ref` 经名为 `seq` 的软链执行；读数 `tmp/seq-420/ab2.log`，命令 `python3 tmp/seq-420/ab.py`）：SAME 的五形是渲染半的正证（`-é` 两行 `303`/`251`、`-Zé` 三行、`-é -Z` 四行且次序按扫描走）；DIFF 的六形全落在**参数缝半**（`-\xff` 参照件一行 `ff`、本壳三行 `ef`/`bf`/`bd`；`-a\xfe`、`-v\x80` 同形），并且本轮新量出它**不只脏报文也脏数据面**——`seq -s<0xfe> 3` 参照件的分隔符是原字节 `fe`、本壳打的是 U+FFFD 的三个字节 ⇒ 新登记 `ARGV-LOSSY-VALUE-PLANE`。**本笔最大的一条自错是夹具自己造的假绿**：A/B 第一版把 `-é` 写成 Python 的 `str` 再 `.encode("utf-8")`，`c3 a9` 被**二次编码**成 `c3 83 c2 a9`、`-\xff` 变成 `c3 bf`（一个合法 UTF-8 字符），于是十一形全 SAME，看着像「连参数缝半都好了」；改成表里直接写字面字节后六形 DIFF 立刻现形。判据回写：**测字节语义的夹具，其输入必须是字节字面量**——这与批次四百一十五那枚「文本夹具分不清 `c3` 与 `c3 83`」是同一条规律的两侧（那次在断言侧，这次在输入侧）。**范围对账也修了一处**：第一版把「运行范围」与「期望门名」两条对账写成了一条——拿门名去比文件名，印出一整列假的「不在范围」；现在拆成两条（范围由 `tests/*_face.rs` 文件清单动态列 27 支，门名逐条回源码查存在，读数 `scope-check.log` 为「查不到的：[]」），这是派生脚本名单类问题的连续第八次。**两件事按现量改判**：① textfilter 的**渲染半 25 支壳全收**——`grep -rn "\.line(" os/commands --include=*.rs | grep -v "/tests/"` 只剩 `textfilter/src/lib.rs` 里 char 形 API 自己的单测与 `editor`/`stdio-games` 的 `reader.line()`（同名不同物），`.line(prog` 形式在 `os/commands` 非测试码里是 **0 处**；② 原计划「把射程②与 `seq` 同轮做掉」**不同轮**——参数缝半是全 crate 的入口（`bin_support::args` 两条腿都过 `from_utf8_lossy`），与换渲染同轮等于让 25 支壳的期望一起翻正、`--lib` 与跨支表同期重写，且两半的可观察形状不同（报文半差在字母字节、数据面半差在选项值内容），合并会让负控无法归因。**度量口径按本轮放宽后新抓到一族**：原配方只数「调 `.line(prog`」的壳，于是看不见**自印 libc 句子**的消费者——`os/commands/games/text-games` 的 `src/lottery.rs:93` 交回 `Ring::Unknown(other as char)`（取法按字节）、`src/bin/random.rs:48` 用 `format!("{prog}: {UNKNOWN_OPTION}{letter}\n")` 渲染（射程①同错）⇒ **切片 26 = `text-games` 的 `random`**；同口径复核 `os/commands/bin/shell/src/bin/getopt.rs:58`/`:73` 是 `err.push(letter)`（`letter` 是 `u8`）⇒ 那支本就字节形，不进射程。**07 篇顺带补了一处文档-代码对账缺口**：`seq.rs` 现量 22 枚库门，文中写 21 个而枚列举只有 20 枚——漏列的是 `test_equalize_width_follows_the_last_printed_value`（`-w` 的宽度取自**实际打印的**首尾两值而非算出的 `last`，`seq -w 1 5 10` 只打 1 与 6），补齐后文中名数与源码门数逐一对上（`comm` 双向空）。读数：宿主与容器各 **1170 / 0 / 68**（68 段、容器 `--lib` 现量 577 tests）、三架构 `-p minix-textfilter --bins` 全退 0、clippy 整包 **154** 与基线同数且触及四件零新告警、`rustfmt --edition 2024` 零自漂移；五道收尾门除已知探针族漂移（26 对基线 18）外全过，断链 131 同基线，文风增量门 error 级 0（板上新写的一句带 `2026-10-19` 被 SL-4 挡住，改成只留批次号与代码笔后过门——台账与协调板都不豁免）。在册 P0 余四条：`HEAD-NON-PERMUTE-OPERAND-TAIL`、`UNEXPAND-ZERO-STOP-PANIC`、`HEAD`/`TAIL`/`NL` 三条末行项（建议并成一轮）、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`；`NL-DELIM-LENGTH-RULE` 待裁决。**A2 余下两族**：切片 26（`text-games` 的 `random` 渲染半，一支壳）、参数缝半（射程②，全族一次改，非单轮量级）。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（切片 26：先给 `text-games` 配参照件——`minix3/games/random/random.c` 链 Minix 的 `getopt.c`，经名为 `random` 的软链起，先跑 `-Z` 确认那句是 Minix 形而不是 glibc 的 `invalid option -- 'Z'`；再定字节形落在哪一层：该 crate 有自己的 `bin_support`，要么就地写一个字节出口，要么把 `GetoptReport` 提到能给 games 用的位置——**这一步是跨 crate 的依赖决定，先量清再动**。）

- `2026-10-19` qoder / todo 循环第一百三十五笔（批次四百二十一，A2 **切片 26：text-games 的 `random`，自印 libc 句子的第一支**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `0155e9c4c`，动 `os/commands/games/text-games/{src/lottery.rs,src/bin/random.rs,tests/random_face.rs}` + 18 篇批次四百二十一、24 篇 `random` 三处行同步、A 板 A2 行（代码笔 `70cbecc22`、文档笔 `cd7e7207c`）。这一支不走 `GetoptReport`，是切片 25 放宽度量口径（从「调 `.line(prog`」放宽到「印 libc 那三句的字面」）后才看得见的第一族：`src/lottery.rs:93` 的环交回 `Ring::Unknown(other as char)`、`src/bin/random.rs:48` 用 `format!` 渲染——取法半本就按字节，渲染半与前 25 支同错。改法两条：**`Ring::Unknown(char)` 换成 `Ring::Unknown(u8)`**（类型本身把「这是一枚字节」说出来），句子收进字节出口 **`lottery::unknown_option_line(prog, letter) -> Vec<u8>`**，薄壳只接线。门 库 +2（`the_off_letter_is_a_byte_and_not_a_character` 钉现状、`the_libc_line_writes_the_letter_as_the_byte_it_was_given`）、面 +1（`the_libc_line_of_a_multibyte_flag_is_one_byte_for_random`，`-é`/`-eé` 逐字节 + U+0080 判据位 + 两枚 ASCII 反面），`--lib` 32→**34**、`random_face` 24→**25**、整包 **75→78 / 0**（宿主与容器同读数，8 段）。**参照件重链了一份，老的那份当场被 `-Z` 判了死刑**：`tmp/random-342/random_ref` 现量 `-Z` 打的是 glibc 的 **`invalid option -- 'Z'`**（它链的是宿主 getopt），承不了「字母位是一枚字节」的真值——这条规则第五次在开工前拦住假参照件；`tmp/random-421/` 按 seq/pr 配方重链（`random.c` + Minix `getopt.c` + basename 垫片 + 同名软链），链接障碍只有 `RANDOM_MAX`（真源 `minix3/include/stdlib.h:188` 的 `0x7fffffff`，宿主头没有，按 A1 行判据从垫片补位）；新参照件现量 `-é`/`-eé` 都是 `303` 一枚字节 + usage、退 1——这一支是「停在第一个坏字母」族（`random.c:80-83` 的 `usage()` 坐在环里），与 `seq` 的累加形相反。**A/B 九形 SAME=7 / DIFF=2，两形 DIFF 挖出一枚新 P0**（`tmp/random-421/ab.log`；argv 全写字面字节，沿用批次四百二十的夹具判据）：`-\xff` 与 `-e\xff` 参照件打 `unknown option -- \xff`，本壳**直接 panic 退 134**——根因在 `src/bin_support.rs:43` 宿主腿的 `std::env::args()`（非 UTF-8 参数 unwrap 即炸），而 textfilter 那份 `bin_support.rs:46-57` 早已是 `args_os()` + lossy 并写明「两条腿得同形」；真机腿不受影响，故这是宿主/真机两腿分叉 + 宿主门测不到该形的空洞，按「一次只修一条」登记 **`TEXTGAMES-ARGV-HOSTLEG-PANIC`**（P0 独立轮：照抄 textfilter 修法 + 给 wtf/fortune/random 三支立跨壳表，textfilter 的对应门是 `no_command_aborts_on_a_nonutf8_option_letter`），本轮不动它。**夹克的一枚时序假红顺手治了**：整包门第一轮 3 枚红全是 `random_face.rs:46` 的 `write_all(...).expect("feed stdin")` 撞 `BrokenPipe`，主树基线两跑一绿一红（`a_rejected_operand…` 同点）——`random` 有若干形在碰到输入之前就先退（`-Z` 落 usage、`-e` 在 `random.c:109-111` 读 stdin 前返回），父子调度先后决定拿不拿得到 EPIPE，是夹克的时序不是被测代码的缺陷；textfilter 的 `tests/face_support/mod.rs::write_stdin` 早已定过「只容忍 BrokenPipe」的口径，本 crate 无共件测试模块可引，按同一判据落本地 `feed_stdin`（三处喂入点全换），连跑三遍 **78/0** 稳定——修复只动测试夹克，与被测行为无关，故并入本轮并记明出处。**负控八枚：七枚各咬住 + 一枚声明等价**（`tmp/random-421/negctl.py`，最终读数 `negctl-final.log`；运行范围整包 8 段 78 枚，锚点先数次数——`support::warn(USAGE.as_bytes());` 在 `src/bin/random.rs` 有 **2** 处，故 usage 配对那条用两行对偶锚点）：R1 薄壳出口退回按字符渲染（1 名）、R2 库出口按 UTF-8 再编码（2 名）、R3 丢换行（3 名）、R4 措辞漂到 `illegal option`（3 名）、R5 取末字节（4 名）、R6 不接 usage（2 名）、R8 取法按字符取（4 名），R7 等价注入（库出口构造改写成同字节串，78 全绿是正确读数）；**注入形状错了两次都被自查拦住**——第一版 R1 漏 `\\n` 把 ASCII 门带红（多咬一口先怀疑注入形状，与批次三百九十 P4、三百八十九 K4 同族）、第一版 R5 写 `body.last()` 编不过（`body` 是 `&str`，取末字节要经 `as_bytes()`）。**clippy 折了一次被 `--all-targets` 当场抓住**：消 `needless_borrow` 时把 `&prog` 改成 `prog.as_str()`，撞上 1.94 里仍未稳定的 `str_as_str`，`E0658` 退 101；按 clippy 自己的建议传 `prog` 归零。判据照旧按告警位置：整包 13 条、触及三件 **0** 条。rustfmt 零自漂移；三架构 `--bins` 按最终形态复跑全退 0。五道收尾门除已知探针族漂移（26 对基线 18）外全过，断链 131 同基线，文风增量门 error 级 0。在册 P0 新增一条（`TEXTGAMES-ARGV-HOSTLEG-PANIC`），原有四条不变：`HEAD-NON-PERMUTE-OPERAND-TAIL`、`UNEXPAND-ZERO-STOP-PANIC`、`HEAD`/`TAIL`/`NL` 三条末行项（建议并成一轮）、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`；`NL-DELIM-LENGTH-RULE` 待裁决。**A2 现余一族**：参数缝半（射程②，全族一次改，非单轮量级）。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`（P0 独立轮 `TEXTGAMES-ARGV-HOSTLEG-PANIC`：把 text-games `src/bin_support.rs:42-44` 的宿主腿换成 textfilter `bin_support.rs:51-57` 的 `args_os()` + lossy 形状，给 wtf/fortune/random 立一枚跨壳表门 `no_command_aborts_on_a_nonutf8_option_letter` 的姊妹门（名字要不同，否则负控红名分不出是谁），A/B 用 `tmp/random-421/ab.py` 的 `-\xff` 两形复跑应从 panic 转 SAME=8/DIFF=1（唯一残差是 lossy 后的 `ef` 对原字节 `ff`，属参数缝半），负控至少三枚：宿主腿退回 `env::args()`、真机腿退回非 lossy、跨壳表撤一行。）

- `2026-10-19` qoder / todo 循环第一百三十六笔（批次四百二十二，P0 独立轮：**`TEXTGAMES-ARGV-HOSTLEG-PANIC` 闭**）：`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ 基线 `2a416c773`，动 `os/commands/games/text-games/{src/bin_support.rs,tests/argv_seam_face.rs(新)}` + 18 篇批次四百二十二、24 篇 argv 缝两处同步、A 板 A2 行（代码笔 `3fa89ec6f`、文档笔 `c58712553`）。缺陷是上一轮 A/B 现量逮住的：宿主腿 `std::env::args()` 对非 UTF-8 参数 unwrap 即 panic 退 134（参照件打 `unknown option -- \xff`），真机腿 `minix_rt::crt0::args()` + lossy 从来不炸——两腿分叉，且宿主门测不到非 UTF-8 参数。修法照 textfilter `bin_support.rs:51-57` 已定过口径的同一条缝：宿主腿换 `args_os()` 保原字节、两条腿各自 lossy（C 把 argv 当字节传，从不校验）。**缝的契约由一枚新门承载，报文归各壳自己的面门**：新 target `tests/argv_seam_face.rs` 一枚 `a_raw_nonutf8_argument_behaves_like_its_lossy_equivalent`——wtf/fortune/random 各喂一枚裸 `\xff`（`OsStr::from_bytes`）与它的 lossy 等价，三样（退码/stdout/stderr）逐字节相同且不得以信号退出；三支壳吃这枚 token 的位置各不相同（random/wtf 走选项环、fortune 当档名），缝契约对三种位置都得成立。现量（修后）：random `unknown option -- \xef` + usage 退 1、wtf usage 走 stdout 退 1、fortune `Cannot open \`-\xef\xbf\xbd'` 三段形退 1。整包 **78 → 79 / 0**（宿主与容器同读数，9 段）。**A/B 复跑 panic 清零**（`tmp/argv-422/ab.log`），SAME 7 / DIFF 2 维持但性质变了：从「崩 vs 报文」收窄为参数缝半的字母位差一位（参照件 `\xff`、本壳 `\xef`）——**交接 prompt 里预估的「SAME=8/DIFF=1」按现量修正为「DIFF 2 形维持、性质收窄」**，预估与现量不一致时以现量为准。负控三枚：N1 宿主腿退回 `env::args()` 咬住、N2 撤 lossy（`from_utf8(...).unwrap_or_default()`，真机腿的对应变异 `cfg(target_os="none")` 编译不进宿主套件，此为宿主代形）咬住、N3 等价注入 `to_string_lossy()` 全绿——**只有三枚是有意的**：本枚门的可观察面只有这条缝，各壳报文/环形状的注入批次四百二十一已在三份面门做过，改环咬不到缝、硬塞就是制造未达标。注入形状错一次被自查拦住：替换块把 `use OsStrExt` 一并换掉后 `into_vec` 找不到（它住在 `OsStringExt`），变异体自带 use 后过门。三架构 `--bins` 全退 0、clippy 13 条且触及件零告警、rustfmt 零自漂移；五道收尾门除已知探针族漂移（26 对基线 18）外全过（文风门拦下一句带日期的写法，去日期留批次号后 error 级 0），断链 131 同基线。在册 P0 余四条：`HEAD-NON-PERMUTE-OPERAND-TAIL`、`UNEXPAND-ZERO-STOP-PANIC`、`HEAD`/`TAIL`/`NL` 三条末行项（建议并成一轮）、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`；`NL-DELIM-LENGTH-RULE` 待裁决。**A2 现余一族**：参数缝半（射程②）——lossy 让非法字节变 U+FFFD，字母位差一位、fortune 档名报文里是 `ef bf bd` 而非原字节；要做就得把 `args()` 改成交回字节或提供字节访问口，牵动 textfilter 与 text-games 两个 crate 的全部壳，非单轮量级。下一轮首动作 `bash tools/claim.sh claim GETOPT-REPORT-BYTE-LETTER qoder`，二选一（按板上顺序与体量自裁）：① P0 并轮修三条末行项（`HEAD`/`TAIL`/`NL-FINAL-LINE-ADDS-NEWLINE`：同一件「末行换行状态」不该三处各改一遍，并轮能把「行腿是否保留输入末行的换行状态」一次判清；参照件 `tmp/nl-417`、`tail`/`head` 的参照件在各自 tmp 目录）；② 参数缝半的**预研轮**（不落码：先量清 `bin_support::args` 改成按字节交回 argv 的两条路——`Vec<Vec<u8>>` 全量迁移 vs 提供字节访问口而保留 `String` 腿——各要动多少处、跨支表门要翻多少枚、textfilter 与 text-games 两个 crate 怎么分轮，把结论写成板上 A2 行的施工顺序，供下一轮直接动工）。

- `2026-10-19` qoder / todo 循环第一百三十七笔（批次四百二十三，P0 并轮：**末行换行状态一族四支壳一次判清**）：`claim/TAIL-FINAL-LINE-ADDS-NEWLINE-qoder` @ 基线 `8180778e8`。板上原记三条（`HEAD`/`TAIL`/`NL` 的末行项）并建议并轮，本轮先量族边界再修，量出**这一族在 C 里并不一致**：`head`/`tail`/`nl`/`uniq` 四支原样交回输入末行（`nl.c:288`+`:340` 的 `fgets` 连着串尾换行、`head.c:169-172` 逐字符 `getc`/`putchar`、`reverse.c:133-144` 记的行区间含换行、`uniq.c` 打印含换行的原行），而 `cut`/`rev` 两支**确实**每行补一枚换行（`tmp/cut-406/cut_ref` 对 `alpha\nbeta\ngamma` 现量 `alp\nbet\ngam\n`，`tmp/rev-309/rev_ref` 同形）。所以修法不是「从共件里删掉补换行」，而是把终止状态做成行上的数据：库侧 `window::Line { text, terminated }` 与单源判据 `final_line_terminated`，`lines_of` 自 `bin_support.rs` 搬进库侧并交回 `Vec<Line>`，`HeadWindow`/`TailWindow` 的存储与 `uniq` 的 `Run.line` 都存 `Line`——`tail -r` 会把缺换行的末行翻到输出的**最前面**、`uniq` 印的是一个连续段的**第一行**，这两处按行号问「是不是末行」都会问错。写出侧 `head`/`tail`/`uniq` 取 `line.eol()`，`nl` 的编号腿按字节自己切行（要容忍非 UTF-8）故复用同一条判据，`cut` 显式保留它那枚换行并在注释里写明它为何是例外。门 1170→**1178**（`--lib` 577→**579**：行腿标记与 `eol` 两枚；面侧六枚含把 `nl_face` 那枚只断 `starts_with` 的过渡门翻成逐字节断言、以及 `cut_face` 那枚钉住归一化的门），宿主与容器权威门同读数 **1178 / 0**（68 段）、三架构 `--bins` 全退 0、`rustfmt` 触及件零残留、参照件矩阵 **SAME=31 / DIFF=0**（改前 17 形红）。负控九枚：`eol` 恒交回换行咬 5、`lines_of` 不标末行咬 5、判据恒真咬 8、`head`/`tail`/`uniq`/`nl` 写出腿退回无条件补换行各咬 1/2/1/1、`cut` 撤归一化咬 8，另**等价变异一枚零枚红**（`eol` 换用 `terminated()` 访问器写同一判断）。本轮也栽在注入自己形状上一次：三枚注入第一次跑成「没编过」，根因是 `re.sub` 把替换串里的 `\n` 解释成真换行，换成不动转义的替换函数后才拿到读数——「注入多咬/空咬先怀疑注入」这条判据第六次兑现，新形状是**替换端的转义被正则库吃掉**。闭 `TAIL-FINAL-LINE-ADDS-NEWLINE`、`NL-FINAL-LINE-ADDS-NEWLINE`、`head` 那条无编号的在册项（本笔起名为 `HEAD-FINAL-LINE-ADDS-NEWLINE` 再当场闭）与新逮的 `UNIQ-FINAL-LINE-ADDS-NEWLINE`；新登记不执行 `LINES-OF-NONUTF8-DROPS-ALL`（`lines_of` 解码失败交回零行，属参数缝半那一次量）。码笔 `abbd2b129`+`1db42e1fe`，本笔为文档与里程碑。A2 余量不变：参数缝半（射程②）；板上在册 P0 还剩 `HEAD-NON-PERMUTE-OPERAND-TAIL`、`UNEXPAND-ZERO-STOP-PANIC`、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`、`LINES-OF-NONUTF8-DROPS-ALL`。
- `2026-10-20` qoder / todo 循环第一百三十八笔（批次四百二十四，P0 独立轮：**`HEAD-NON-PERMUTE-OPERAND-TAIL` 闭**）：`claim/HEAD-NON-PERMUTE-OPERAND-TAIL-qoder` @ 基线 `a58831de2`（领用树内 rebase 一次，C 板在此期间落了 C3 信号执行腿与 C12 释放登记两族），门按新基线复跑。**这一轮判的是「操作数一到，选项环该不该收口」**：库里 `os/commands/usr-bin/textfilter/src/window.rs` 的 `parse_head_args`/`parse_tail_args` 两处 else 分支在这一轮之前收一个 token 后回环继续扫，于是 `head f -5` 走成「字母 `5` 集外」那句、`tail f -r` 会反序；真值在 `minix3/lib/libc/stdlib/getopt.c:71-74`（撞上非选项 token 交回 `-1` 且**不推进** `optind`）与 `getopt.c:96-104`（裸 `-` 也算非选项），配上 `head.c:118-119`、`tail.c:110` 的 `argv += optind`，后果是**第一个操作数及其后的全部 token 都是档名**——`-q` 不置旗标、`-x` 不走集外字母那句、`-n 3` 不改计数、`--` 只是一个叫 `--` 的档名。改成一次收下 `scanned[index..]` 并 `break`。
  **门**：面侧各新增一枚 `an_operand_ends_option_scanning_like_c`（`head_face` 钉 `f -5`/`f -q`/`-n 1 f -5`/`f -- -5`/`f -` 五形、`tail_face` 钉 `f -r`/`-r f -n 3` 两形，逐字节对参照件读数），`head_face` 的 `unknown_option_prints_the_libc_line_then_the_usage_line` 去掉「`[file,"-5"]` 报字母 `5`」那枚子形（它钉的是错的一侧，同一句报文由 `-x` 那形继续钉）；库侧翻正 `test_parse_head_args_errors`、`test_parse_tail_args_faithful_getopt`、`test_parse_tail_args_obsolete_stops_at_first_non_match` 的形集——`--lib` 仍 **579**（翻形不加门），整包 **1178→1180**。
  **读数（新基线 + 最终形态）**：宿主 `cd os && ulimit -v 3145728; cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` 与容器权威门同 **1180 passed / 0 failed**（68 段、退码 0，日志 `tmp/head-424/reb-host.log`、`reb-container.log`），三架构 `--bins` 全退 0，clippy 整包 **154** 同基线（触及件三处命中全是存量：`src/window.rs:204` 的 `needless_range_loop` 与批次四百二十三日志同行同句，`:944`/`:945` 两条 `unused import` 是同一批告警位移 23 行），`rustfmt` 本轮自引入三处漂移（新增库测里的多行 `vec![...]`）已按 `--edition 2024` 就地归正——归正前整包 102 处 / 触及件 3 处，归正后 **99**（与主树同数）/ **0**，五道收尾门全过（`doc-style-lint --diff a58831de2` error级=0，台账两处被 SL-5 判为修复史叙事的措辞在形修笔里改成现状陈述；`notes-link-check` 断链 **131** 同基线；`todo-reconcile-scan` 仅存「探针命名族」一项漂移，那是 C 板在途的 kernel 探针族，不属本轮射程）。
  **负控五枚**（`tmp/head-424/negctl-*.log`）：注入前先数锚点（两条 `extend_from_slice(&scanned[index..]);` 各 1 次；不带 `result.` 前缀的公共子串会数成 2，故锚串连缩进一起比）。N1 `head` 退回逐个 push 并继续扫咬 2、N2 `tail` 同形咬 3、N3 `head` 收全尾但不 `break` 咬 4、N5 `tail` 同形咬 5，另**等价变异**一枚（`extend_from_slice` 换成逐条 `push` 的显式 `for` 再 `break`）614 passed / 0 failed 全绿。**这枚等价第一次跑成「没编过」**：注入写 `String::from(*rest)` 撞上 `E0507 cannot move out of *rest`（`rest` 是 `&String`），而脚本把 `passed=0 failed=0` 读成了绿——「注入空咬先怀疑注入自己的形状」这条判据第七次兑现，这次连**判读法**一起修：以后等价注入必须核 `^error` 计数为零才算绿。
  **同轮量出这条判据是族级的**（`python3 tmp/head-424/family.py`，读数 `family.log`/`cut-tail.log`/`tail-extra.log`）：`fold a.txt -s`、`unexpand a.txt -a`、`pr a.txt -t`、`cut -f 1 a.txt -s` 的参照件都把后随旗标当档名，所以登记八支待逐支现查：`CUT`/`FOLD`/`PR`/`NL`/`EXPAND`/`UNEXPAND`/`SORT`/`CKSUM-NON-PERMUTE-OPERAND-TAIL`（`nl` 的环在 `bin/nl.rs:126-129`，形状与本轮改掉的两处相同；`sort` 的现成参照件被 `-Z` 现量判为 glibc 血脉——第六次拦住假参照件，`cksum` 在 `tmp/cksum-414/` 下没有参照件 binary，这两支要先重链）。**按「一次一条」不在本轮顺手全改**：七处同族缺陷挤一轮会让负控无法归因，也会让某一支的现查未完成就先落码。码笔 `918df6543`、文档笔 `2c9ea4965`、形修笔 `3ab1d2c23`。A2 余量：参数缝半（射程②）；板上在册 P0 还剩 `UNEXPAND-ZERO-STOP-PANIC`、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`、`CUT-SUPPRESS-STILL-PRINTS-NEWLINE`、`CUT-EMPTY-BYTE-LIST-RUNAWAY`、`LINES-OF-NONUTF8-DROPS-ALL` 与上述八支，`NL-DELIM-LENGTH-RULE` 等人裁决。

- `2026-10-20` qoder / todo 循环第一百三十九笔（批次四百二十五，P0 独立轮：**`UNEXPAND-ZERO-STOP-PANIC` 闭**）：`claim/UNEXPAND-ZERO-STOP-PANIC-qoder` @ 基线 `06d27b8a4`，动 `os/commands/usr-bin/textfilter/{src/unexpand.rs,tests/unexpand_face.rs}` + 18 篇批次四百二十五一节 + 07 篇 `unexpand` 行 + 板 A2 行记闭。
  **判的是「档位 0 该不该让减法停下」**：`minix3/usr.bin/unexpand/unexpand.c:146` 写 `limit = nstops == 0 ? UINT_MAX : tabstops[nstops - 1] - 1;`，制表符那两支又各有 `tabstops[n] - 1`（`:157`、`:160`），三处减的都是 `size_t`——档位为 0 时回绕成一个极大值而不是报错，循环照旧走到头，所以 0 在 C 里是一支**合法但谁也够不着**的档位；本壳那三处是 `usize` 的裸减法，在带溢出检查的构建里先触发 trap，`unexpand -t 0` 就把命令崩掉。动手前先把 C 含 0 档位的逐形行为量清（`tmp/unexpand-425/ref-matrix.log`，参照件 `tmp/unexpand-312/unexpand_ref` 先按批次四百零七那条规矩打 `-Z` 验归属）：`-t 0` 六形全部退 0 且整行原样交回、档位表回显 `0 0`，`-t 0,8` 再多回显 `1 8`；`-t 4,0` 由单调性检查挡下（`unexpand.c:108-112`）⇒ 0 作首档合法、作后档非法；`-a` 与 `-t` 互斥，谁后到谁 `usage()`。
  **落码**：`src/unexpand.rs:110`/`:131`/`:134` 三处换成 `wrapping_sub(1)`（码笔 `62cd45b01`），制表符那一支的注释同时点明它自己还有一处已登记的偏离，免得下一个读码的人把两件事当成一件。
  **门**：库侧 `a_zero_tabstop_wraps_its_limit_the_way_c_does`（`parse_stops` 收 0 的两形 + 四形逐字节输出 + 一形只要求不崩）、面侧 `a_zero_tabstop_reaches_the_line_unchanged`（`-t 0` 三形逐字节，含一枚无尾换行的裸字节输入），`--lib` 579→**580**、`unexpand_face` 12→**13**、整包 1180→**1182**。
  **读数（最终形态）**：宿主与容器权威门同 **1182 passed / 0 failed**（68 段、退码 0，`tmp/unexpand-425/f-host.log`、`f-container.log`），三架构 `--bins` 全退 0，clippy 154 与基线同数（触及件两处命中是存量那两枚 `implicit_saturating_sub` 的位移，对读 `tmp/head-424/final-clippy.log` 的同一句），`rustfmt` 触及三件零漂移且整包 99 与主树同数；两侧逐字节 A/B 复跑 **35 形 SAME=29 / DIFF=6**（`tmp/unexpand-425/ab.log`）。五道收尾门全过，仅存已知漂移（探针命名族 31 对 18，属 C 板在制的内核探针族，本线禁碰）。
  **负控四枚**（`tmp/unexpand-425/negctl-run2.log`）：注入前数锚点，三处 `wrapping_sub(1)` 各 1 次。N1（`limit` 退回裸减法）咬 2 枚＝库门＋面门；N2（找档的 `while`）、N3（判断可用的 `if`）各咬 1 枚＝只有库门，因为面门那三形都不进制表符支——没有 `-a` 时首个非空白字节就收尾，只有库侧 `  \tx` 那一形走得到；N4 等价改写（`checked_sub(1).unwrap_or(usize::MAX)`）593 passed / 0 failed 全绿。
  **本轮门腿自己有两处形状问题，都在拿到读数之前被抓住**：逐件 rustfmt 那一圈写成 `RFMT` 而非 `"$RFMT"`，shell 的「命令不存在」被 `grep -c` 吞成零漂移，是 `cargo fmt` 报的 100 处（基线 99）按文件名列出来才露出本轮新写那处；负控脚本把 cargo 在测试正常失败时印的 `error: test failed` 当编译器错误计数，一度把三枚真咬住的读数判成「不算读数」。判据「注入或夹具空咬/多咬先怀疑它自己的形状」在本线第八、第九次兑现，形状分别是**变量名当命令名写**与**失败信息里带 error 字样**。
  **本轮登记不执行两枚**：`UNEXPAND-LAST-STOP-TAB-DROPPED`（档位表最后一档已越过当前列时，C 落到 `unexpand.c:195` 那句 `putchar(*p)` 把输入里的制表符原样交回，本壳在这一支吞掉；A/B 那六形即此，且与档位是否含 0 无关，`-t 8` 也吞）；`UNEXPAND-C-COMMENT-ANCHOR-DRIFT`（本轮触及的两件里既有的 C 行号锚点四处与真源不符，差 2 到 9 行，逐条对账 `tmp/unexpand-425/anchor-audit.log`；本轮只把自己新写的四处按现量改对）。**在册未动**：`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`、`CUT-SUPPRESS-STILL-PRINTS-NEWLINE`、`CUT-EMPTY-BYTE-LIST-RUNAWAY`、`LINES-OF-NONUTF8-DROPS-ALL`、八支 `*-NON-PERMUTE-OPERAND-TAIL` 族、`NL-DELIM-LENGTH-RULE`（待裁决）、参数缝半（射程②）。

- `2026-10-20` qoder / todo 循环第一百四十笔（批次四百二十六，P0 独立轮：**`UNEXPAND-LAST-STOP-TAB-DROPPED` 闭**）：`claim/UNEXPAND-LAST-STOP-TAB-DROPPED-qoder` @ 基线 `98ba64cc2`，动 `os/commands/usr-bin/textfilter/{src/unexpand.rs,tests/unexpand_face.rs}` + 18 篇批次四百二十六一段 + 07 篇 `unexpand` 行 + 板 A2 行记闭。
  **判的是「够不着下一档的制表符算不算已经消费掉的空白」**：`minix3/usr.bin/unexpand/unexpand.c:152-165` 里制表符那一支只在 `n < nstops - 1 && tabstops[n] - 1 < limit` 成立时才把列推到那一档并 `continue`（`:161-162`），不成立就顺着控制流落到 `/* Output our tabs */`（`:167`）、`/* Output remaining spaces */`（`:187`），最后由 `/* Output our char */` 的 `putchar(*p)`（`:195`）把**制表符本身**交出去，`:202-204` 再让两列各进一格。本壳把 `cursor += 1; continue;` 写在 `if` 外，于是这一支的制表符整枚丢掉——现量在负控 N1 的读数里：`left: "a"` 对 `right: "\ta"`。`src/unexpand.rs:133-140` 把 `continue` 收回 `if` 体内，落点与 C 同一腿（码笔 `deb2654b9`）。
  **门**：库侧 `a_tab_beyond_the_last_stop_is_handed_back_like_c_does`（八形：`-t 8` 四形含「八空格先把列推到第 8 档、随后制表符够不着下一档」这枚两枚制表符的输出、`-t 4,8` 两形含反例——够得着第 1 档那一形被折进档里交 `\tx`、`-t 0` 两形）、`a_blank_run_is_collapsed_only_by_a_non_blank_byte`（间隙只由非空白冲出，末行只剩空白交出零字节，不足一档的间隙照原样留着）；面侧 `a_tab_past_the_last_stop_and_a_blank_final_line_reach_stdout`（逐字节钉 `-t 8`/`-t 4,8` 三形＋两枚无尾换行的裸字节输入）。`--lib` 580→**582**、`unexpand_face` 13→**14**、整包 1182→**1185**（文档笔 `7c7eeb8cc`）。
  **读数（最终形态）**：宿主与容器权威门同 **1185 passed / 0 failed**（68 段、退码 0，`tmp/unexpand-426/final-host2.log`、`final-gates-run2.log`），三架构 `--bins` 全退 0，clippy 整包 **154** 与基线同数（触及件两处命中仍是批次四百二十五那一对存量 `implicit_saturating_sub`，因本轮增删位移到 `:172`/`:175`），`rustfmt` 触及件零漂移且整包 99 与主树同数。回归面：上一轮那 35 形逐字节矩阵复跑 **SAME=35 / DIFF=0**，本轮 17 形 SAME=13 / DIFF=4、末行 12 形 SAME=10 / DIFF=2（`tmp/unexpand-426/ab-425-matrix-final.log`、`ab-final.log`、`eol-final.log`），那 6 形全属下面登记的行末冲洗一家。五道收尾门全过，仅存已知漂移（探针命名族 31 对 18，属 C 板在制的内核探针族，本线禁碰）。
  **负控四枚**（`tmp/unexpand-426/negctl-b1.log`…`negctl-b4.log`，注入前先数锚点，四处各 1 次）：N1 制表符那一支退成「够不够得着都消费掉」咬 3（库内两枚＋面侧那一枚）、N2 够得着档时不推进 `dcol` 咬 1、N4 冲洗那一腿的档位判据由「够到档」收紧成「越过档」咬 6（连档位表回显与 base 0 解析两枚都被带走，因为它们的输入行里有制表符）；N3 是**等价变异**（`n + 1 < len` 写成 `n < len - 1`，这一支的档位表非空所以无符号减法安全）→ 596 passed / 0 failed 全绿。第一轮另有两枚**没跑成注入**：锚点串的前导四空格是循环内那次调用八空格缩进的子串，计数为 2 而非 1，脚本按「锚点先数次数」当场判不成立并跳过——本线第十次由注入或夹具自己的形状拦下一次错读数；同轮还把负控脚本的腿标号正则从 `\S+` 改成 `.+?`（`--test unexpand_face` 中间有空格，`legs` 读数一度为空）。
  **同轮量清并登记不执行 `UNEXPAND-BLANK-LINE-END-FLUSH-NEEDS-TERMINATOR`**：本轮一度把「行末也冲一次档」当成第二件缺陷落码（`unexpand_line` 末尾加一次冲洗），12 形现量把它否掉了——C 的 `tabify` 走到缓冲区末尾直接 `return`（`unexpand.c:214-215`），行末**不冲任何档**；参照件对 `    \n` 交出 `\t\n` 靠的是 `fgetln` 交回的换行符自己走「非空白」那一腿，而本壳的 `unexpand_line` 拿到的是剥掉换行的行体，「这一行是否以换行结束」在这层没有答案：无条件冲把「末行没有换行」那一家从同形变成反向分叉（`        ` 无换行 → 参照件零字节、冲的那版 `\t`），不冲则维持那一家同形、把带换行那半分叉（六形在案）。于是收回，并按不冲登记。两条施工路一并写进板 A2 行：①给 `unexpand_line` 加一枚「本行以换行结束」的参数（库内 31 处调用点跟着改）；②把换行留在行体里，让这一层与 C 的 `tabify` 同形（要改 `unexpand()` 的返回值契约与壳侧写口，且与板上「末行换行状态」那一族是同一个决定）。**同一轮里为共享两处调用点抽出的 `flush_gap` 辅助函数也一并收回**——行末那次冲洗撤掉后它只剩一个调用者，留着就是为假想的下一步养抽象。07 篇 `unexpand` 行里那句「一处 panic 在册待修」是批次四百二十五留下的过期陈述，本轮按 Doc-Code Sync 撤掉。**在册未动**：`UNEXPAND-C-COMMENT-ANCHOR-DRIFT`、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`、`CUT-SUPPRESS-STILL-PRINTS-NEWLINE`、`CUT-EMPTY-BYTE-LIST-RUNAWAY`、`LINES-OF-NONUTF8-DROPS-ALL`、八支 `*-NON-PERMUTE-OPERAND-TAIL` 族、`NL-DELIM-LENGTH-RULE`（待裁决）、参数缝半（射程②）。


- `2026-10-20` qoder / todo 循环第一百四十一笔（批次四百二十七，P0 独立轮：**`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE` 闭**）：`claim/SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE-qoder` @ 基线 `c1bf529d2`，动 `os/commands/usr-bin/textfilter/{src/split.rs,src/bin/split.rs,tests/split_face.rs}` + 18 篇批次四百二十七一段 + 07 篇 `split` 行 + 板 A2 行记闭（码笔 `978fa3890`、文档笔 `e6826477b`、文风门复跑笔 `4f2bedff2`）。
  **判的是「判据写在哪一层」而不是「判据对不对」**：`minix3/usr.bin/split/split.c` 把三件事排成先后——`argv += optind`（`:132`）之后先开第一支操作数（`:135-139`，`-` 在 `:136` 走特例不开档，开不成当场 `err(1, "%s", *argv)`），再取输出前缀（`:143`），最后才 `if (*argv != NULL) usage()`（`:148-149`）。库里 `split.rs:fn parse_split_args` 的契约是取数，它看不见文件系统；把超数那一判写在这一层，等于让取数替开档决定谁先报错，于是 `split 开不了的档名 前缀 第三支` 交出 usage，而参照件交出的是那个档名（`tmp/split-427/ref-shapes.log` 四形：默认腿与 `-l`/`-b`/`-n` 各一，全是 `split_ref: nope.txt: No such file or directory`；开得开的 `in.txt pfx third` 才是 `usage: split …`；裸 `-` 不开档，那一形直接落进 usage）。反过来把开档挪进取数层也不通：一个纯计算函数从此带 I/O 副作用，且与壳侧真正开档那一腿重复。落码是库里那一判撤掉（`src/split.rs:342-349` 现在写的是「为什么它不在这里判」），壳侧 `src/bin/split.rs:fn run` 取到前缀之后按第一支探一次开档（`-` 跳过），开不成报档名退 1，否则落 usage；探测只在这条错误腿里发生。
  **门**：面侧 `an_unopenable_input_outranks_the_third_operand`（四形逐字节钉 `split {档名}: No such file or directory`、退 1、且**不落下任何分片**，另钉裸 `-` 那一形仍落 usage）；库侧 `parse_error_paths` 里两支断言随判据挪位翻转（`["in","-l","3"]` 与 `["a","b","c"]` 现在断言 `unwrap().operands` 三支全在），并撤掉同一函数里一枚与之重复的旧断言。`split_face` 29→**30**、整包 1185→**1186**。
  **读数（最终形态）**：宿主与容器权威门同 **1186 passed / 0 failed**（68 段、退码 0，`tmp/split-427/sf-host.log`、`sf-container.log`），三架构 `--bins` 全退 0，clippy 整包 **154** 与基线同数（触及件两处命中是同一文件测试段那一对存量项 `items after a test module` 与 `io::Error::new(ErrorKind::Other, …)`，被本笔净增的 14 行推到 `:414`/`:375`；本轮新增代码零告警。带 `-A clippy::all` 的过滤腿整包读数 96，同一形状下的另一套数，故整包数只作旁证、判据一律按告警位置），`rustfmt` 触及件零漂移且整包 98 与主树同数。回归面：本轮 10 形逐字节 A/B **SAME=10 / DIFF=0**（`tmp/split-427/ab-final.log`）。五道收尾门全过，仅存已知漂移（探针命名族 31 对 18，属 C 板在制的内核探针族，本线禁碰）。
  **负控四枚**（`tmp/split-427/negctl-N1.log`…`negctl-N4.log`，注入前壳侧两锚与库侧注释块各数得 1 次）：N1 把「多于两支即错」放宽成「多于三支才错」（`operands.len() > 2` → `> 3`）咬 2（新面门 + 存量那枚 `three_operands_print_only_the_usage`）；N2 探测开档却不看结果（`warn_line`/`terminate` 换成 `let _ = …`，等于回到「先报 usage」）咬 1；N4 把超数那一判搬回库里的取数腿咬 2（`split::tests::parse_error_paths` + 新面门）；N3 是**等价变异**（`> 2` 写成 `>= 3`）→ 612 passed / 0 failed 全绿。**N1 第一次跑成等价**：注入脚本那条腿做的是「把锚点串里的 `> 2` 换成 `>= 3`」，而锚点串当时已经是 `>= 3`，替换是空操作，于是 N1 与 N3 注入出同一形状、报绿——先怀疑注入自己的形状，改成真正放宽的 `> 3` 后按预期转红。这是本线第十一次由夹具或注入脚本自己的形状拦下一次错读数。
  **同轮的 A/B 归一化教训第二次现量**：第一轮跑成 0 SAME / 11 DIFF，差的全是报文里的**程序名**（参照件那条腿打 `split_ref:` 与 `usage: split_ref`，本壳打 `split`），归一化只覆盖二进制的路径、没覆盖打印出来的程序名；按行首把程序名也归一之后到 SAME=10 / DIFF=0。判据与批次四百二十四同源并升级一次：**归一化必须两侧同覆盖，且要用现量证明它覆盖到报文正文**，不能假定「换了路径就够了」。**在册未动**：`UNEXPAND-C-COMMENT-ANCHOR-DRIFT`、`UNEXPAND-BLANK-LINE-END-FLUSH-NEEDS-TERMINATOR`（两条施工路已在批次四百二十六写明）、`CUT-SUPPRESS-STILL-PRINTS-NEWLINE`、`CUT-EMPTY-BYTE-LIST-RUNAWAY`、`LINES-OF-NONUTF8-DROPS-ALL`、`NL-DELIM-LENGTH-RULE`（待裁决）、八支 `*-NON-PERMUTE-OPERAND-TAIL` 族余量（`sort`/`cksum` 的参照件需重链）、参数缝半（射程②）。

- `2026-10-20` qoder / todo 循环第一百四十二笔（批次四百二十八，P0 独立轮：**`CUT-SUPPRESS-STILL-PRINTS-NEWLINE` 闭**）：`claim/CUT-SUPPRESS-STILL-PRINTS-NEWLINE-qoder` @ 基线 `108ab8abb`，动 `os/commands/usr-bin/textfilter/{src/cut.rs,src/bin/cut.rs,tests/cut_face.rs}` + 18 篇批次四百二十八一段 + 07 篇 `cut` 行 + 板 A2 行记闭（码笔 `a4ea99e74`、文档笔 `374c0631e`、文风门复跑笔 `c16ee44e5`）。
  **判的是「`-s` 抹掉的是那一行还是那一行的内容」**：`minix3/usr.bin/cut/cut.c:246-254` 里 `f_cut` 先扫一遍判定 `isdelim`，走到行末时 `if (!isdelim && !sflag)` 才把整行交给 `fwrite`（`:248-249`，那一行自带终止符），紧接着 `:253-254` 的 `if (!isdelim) continue;` 让这一行**跳过**字段抽取与 `:279` 那句 `putchar('\n')`。所以「这一行有没有分隔符」决定的是它**存不存在**：`-s` 之下它一个字节都不该留。与它最像却又必须相反的是「有分隔符但选不出字段」——`-f 9 -s` 对 `a\tb\tc` 走的是另一条腿，照常写一行只是没内容，参照件交回一枚换行（逐形读数 `tmp/cut-428/ref-s.log`）。库里若用一个 `Ok(0)` 同时表示这两形，壳侧就无从分辨该不该补那一枚换行，`-f3 -s` 于是交出 `c\n\n`。修法不是补一个 `if`，而是把返回值改成能分辨的形状：`cut.rs:select_fields` 交回 `Option<usize>`（`None` 即整行不存在），壳侧 `bin/cut.rs:fn run` 只在 `Some` 时写正文与换行；字节腿经 `.map(Some)` 并进同一个 `match`——`-s` 只能配 `-f`（`cut.c:124` 的 `dflag || sflag` 那道 post-check），那条腿永远交出「要写的一行」，统一形状而不掺语义。
  **门**：库侧新增 `a_suppressed_line_is_absent_rather_than_empty`（三形对照：有分隔符选空 → `Some(0)`、无分隔符带 `-s` → `None`、无分隔符不带 `-s` → `Some(5)`），存量 `test_missing_delimiter_policy` 第二条断言随返回类型由空串翻成 `None`；面侧新增 `a_suppressed_line_leaves_nothing_behind`（四形逐字节）。`--lib` 582→**583**、`cut_face` 16→**17**、整包 1186→**1188**。
  **读数（最终形态）**：宿主与容器权威门同 **1188 passed / 0 failed**（68 段、退码 0，`tmp/cut-428/cf-host.log`、`cf-container.log`），三架构 `--bins` 全退 0，clippy 整包 **154** 与基线同数（触及件那一处命中是存量的 `cut_face.rs` 里 `iter().copied().collect()`，基线 `:170` 被本轮新门顶到 `:197`；本轮新增代码零告警），`rustfmt` 本轮新写的行零漂移、整包 **98** 与主树同数（`src/cut.rs` 剩的两条漂移是存量的 `select_bytes` 签名与 `in_ranges` 链式调用，基线同文件也是这两处，本轮没把它们卷进来重排）。回归面：本轮 17 形逐字节 A/B **SAME=17 / DIFF=0**（`tmp/cut-428/ab.log`）。五道收尾门全过，仅存已知漂移（探针命名族 31 对 18，属 C 板在制的内核探针族，本线禁碰）。参照件归属现量核验：软链 `tmp/cut-428/cut` 起、`-Z` 交回 `unknown option -- Z` 的 BSD 写法而非 glibc 的带引号句子。
  **负控四枚**（`tmp/cut-428/negctl-N1.log`…`negctl-N4.log`，三个锚点注入前各数得 1 次）：N1 壳侧把「整行跳过」写成「打一枚空行」咬 1（新面门）；N2 库里把被抑制的行交成 `Some(0)` 咬 3（两枚库门 + 面门）；N4 把抑制判据反过来（`if !suppress`）咬 3；N3 是**等价变异**（`Ok(None) => {}` 写成 `Ok(None) => continue`）→ 600 passed / 0 failed 全绿。
  **本轮 17 形没覆盖到的一形，另测之后登记不执行 `CUT-NEWLINE-DELIMITER-NEEDS-TERMINATOR`**：`-d $'\n'` 时参照件把换行当普通分隔符看（C 在扫描里先比 `ch == sep` 再比 `ch == '\n'`，`cut.c:244-251` 那句注释 `this should work if newline is delimiter` 就写在那一判上），而本壳的行腿先按换行切行并剥掉终止符，那一枚分隔符在行体里已不存在。五形现量（`tmp/cut-428/newline-delim.log`）：不带 `-s` 两侧同形（`a\nb c\n`），带 `-s` 分叉（参照件 `a\nb c\n`、本壳零字节；`-f 2 -s` 参照件 `\n\n`），换成 `-d ';'` 两侧又同形（`a;b\nc\n` → `a\n`）。这与 `UNEXPAND-BLANK-LINE-END-FLUSH-NEEDS-TERMINATOR` 和板上「末行换行状态」那一族是同一道题（**行腿到底保不保留输入的终止符**），裁完那道题再动这两处；本轮修复对这一形既没变好也没变坏（`-s` 之下改前多打换行、改后整行不打）。**在册未动**：`CUT-EMPTY-BYTE-LIST-RUNAWAY`（需一次裁决）、`CUT-NEWLINE-DELIMITER-NEEDS-TERMINATOR`、`UNEXPAND-C-COMMENT-ANCHOR-DRIFT`、`UNEXPAND-BLANK-LINE-END-FLUSH-NEEDS-TERMINATOR`、`LINES-OF-NONUTF8-DROPS-ALL`、`NL-DELIM-LENGTH-RULE`（待裁决）、八支 `*-NON-PERMUTE-OPERAND-TAIL` 族余量、参数缝半（射程②）。
- `2026-10-20` qoder / todo 循环第一百四十三笔（批次四百二十九，P0 独立轮：**`UNEXPAND-C-COMMENT-ANCHOR-DRIFT` 闭**）：`claim/UNEXPAND-C-COMMENT-ANCHOR-DRIFT-qoder` @ 基线 `3c8e66bba`，动 `os/commands/usr-bin/textfilter/{src/unexpand.rs,src/bin/unexpand.rs,tests/unexpand_face.rs}` 三件注释 + 18 篇批次四百二十九一段 + 07 篇 `unexpand` 行一处 + 板 A2 行记闭。
  **判的是「注释里那些 C 行号还能不能把人带到那一句」**：真源 `minix3/usr.bin/unexpand/unexpand.c` 只有一个 vendored 提交，所以号数与真源之间的每一格差都是锚点自己漂出去的。锚点漂了不是无害的过期而是反向误导——按 `:120-121` 去读落到的是 `}` 与 `argc -= optind;`，而句子讲的是档位表回显（真位置 `:124-125`）；读到那里的人会把回显误挂到选项环的收尾上。更难的一类是**区间罩住了但罩歪**：`:91-106` 里确实有 `strsep`（`:96`），可起点 `:91` 是 `-a` 那一臂的 `all++`，而句子明说的「必须严格大于前一支」在 `:108`，落在区间之外。
  **量法**：不靠肉眼逐条数。机械门 `tmp/unexpand-429/anchor-check.py` 每枚引用断三样——区间之内必须有那句、区间首行是那句的起点、区间末行是终点（首末两栏专治「罩住了但罩歪」），并先核对该 Rust 行确实写着表里那个行号（行表自己也会漂，不核对就是拿过期的表盖章）。51 条引用里**修正前 20 条不成立、修正后 51/51 全绿**；修正前的读数跑在领用树基线件形快照 `tmp/unexpand-429/before-tree/` 上，可复跑。漂移分三族：同族一起漂（档位回显 `:124-125` 被五处抄成 `:120-121`）、边界差一位（`Invalid tabstop` 的 `errx` 跨 `:102-103` 而引用报到 `:104`；`case '?':`、`default:`、`usage();` 是 `:116-118`，写成 `:115-117` 两头各偏一位）、罩歪（`-t` 臂 `:93-115`、`strtoul` 在 `:100` 不是 `:98`、`tabify` 那段还讲行末那一腿故区间取到 `:214`）。跨源一枚同型错：`-ta` 的「同串余部当 optarg 交回」在 `getopt.c:117-118`，引用 `:120-122` 罩的是 GNU 可选值注释块。07 篇 `unexpand` 行里同族那一枚一并改掉；该行 `:160-165`、`:146`、`:214-215`、`getopt.c:79` 四枚经同一套断言核过未漂（码笔 `3320040f3`，文档笔 `7fc347f06`）。
  **门打在门自己身上**（本轮零行为改动，测试计数不变）：负控四枚注入漂移全咬住（`tmp/unexpand-429/negctl.log`），并挖出门自己的两个洞——① 区间核对第一版只认区间的**首位**数字，于是 `101-103` 收窄成 `101-102` 照样过闸，门当场从「核对区间」退化成「核对首位」，改成整对数字必须出现才咬得住；② 判「咬住」若只看退码，注入脚本自己崩溃时那份**空 stdout** 会冒充成一次红（N1 第一轮的读数就是这么错的，注入留在了件里，直到复跑核对才撤回）。于是两条并立：红行必须点到被注入的那一件那一行，注入一律在 `finally` 里复原。文档侧顺带扫 18-stage-commands 全篇 **113** 条区间引用（`tmp/unexpand-429/doc-anchor-sweep.py`）：越界 1 条、跨空行 10 条（整函数或整块的题注，属合法不动），其余 102 条落在连续语句段内；扫描的第一版按 `getopt.c` 这个子串匹配，把另一支援 `usr.bin/pr/egetopt.c` 的 `egetopt.c:144-151` 也切成 libc 的号数（libc 只有 142 行），四条「越界」全是这么来的，补整词后消失——**「注入或扫描多咬先怀疑自己的形状」这条判据在文档侧也兑现一次**。
  **读数（最终形态）**：宿主与容器权威门同 **1188 passed / 0 failed**（68 段、退码 0，`tmp/unexpand-429/final-host.log`、`final-gates.log`，与基线同数因为没动行为）；三架构 `--bins` 全退 0、`^error` 计数 0；clippy 整包 **154** 与基线同数（按告警位置判），触及件两处命中是批次四百二十五在册那一对存量 `implicitly performing saturating subtraction`（`src/unexpand.rs:172`、`:175`，本轮不增删行故位置未动）；`rustfmt` 领用树与主树同法（`cargo fmt --offline -p minix-textfilter -- --check`）各 **0** 处 `Diff in`；`git diff -U0` 逐行确认 19 增 19 删**全部落在注释行**（非注释行命中数 0）。五道收尾门全过（仅存已知漂移：探针命名族 31 对 18，属 C 板在制的内核探针族，本线禁碰；`notes-link-check` 断链 131 与基线同数）。
  **登记与闭合**：闭 `UNEXPAND-C-COMMENT-ANCHOR-DRIFT`（板 A2 行同步记闭）。**新登记不执行**：`TEXTFILTER-COMMENT-ANCHOR-AUDIT-REST`（同一道机械门尚未用在 textfilter 其余件：`cut.rs` 引 `cut.c`、`seq.rs` 引 `seq.c` 与 `getopt.c`、`split.rs` 引 `split.c`，各成一族）、`LEDGER-ANCHOR-OFFBYONE`（`todo.md:842` 的 `getopt.c:63-143` 越界一行，以及「台账里的锚点要不要与正式文档吃同一套门」需一次口径裁决）。**在册未动**：`CUT-EMPTY-BYTE-LIST-RUNAWAY`（需裁决）、`CUT-NEWLINE-DELIMITER-NEEDS-TERMINATOR` 与 `UNEXPAND-BLANK-LINE-END-FLUSH-NEEDS-TERMINATOR`（与板上「末行换行状态」那一族同一道题）、`LINES-OF-NONUTF8-DROPS-ALL`、`NL-DELIM-LENGTH-RULE`（待裁决）、八支 `*-NON-PERMUTE-OPERAND-TAIL` 族余量（`sort`/`cksum` 的参照件需重链）、参数缝半（射程②）。

- `2026-10-20` qoder / todo 循环第一百四十四笔（批次四百三十，P0 独立轮：**`LINES-OF-NONUTF8-DROPS-ALL` 闭**）：`claim/LINES-OF-NONUTF8-DROPS-ALL-qoder` @ 基线 `28395f393` → 码笔 `0d2c6b68f`、文档笔 `f40dce7be`、文风门修订一笔 `780541952`
  **判的是「一枚读不懂的字节会不会吃掉整份输入」**：`lines_of` 改前先把整份输入 `core::str::from_utf8(...).unwrap_or("")` 解一次再按换行切，解失败即交回空串，于是一片也切不出。夹具 `tmp/lines-430/in.bin`（`a\xffb\nc\n\xff\nplain\nlast`）在 `head -n 2` 下本壳打零字节，参照件 `tmp/head-415/head_ref` 连那枚 `\xff` 一起把两行打出来。C 那侧从来不需要"解码成功"才有行：`minix3/usr.bin/nl/nl.c:288` 的 `fgets` 连着换行读进缓冲、`minix3/usr.bin/head/head.c:169-172` 逐字符 `getc`/`putchar` 搬运、`minix3/usr.bin/unexpand/unexpand.c:133` 的 `fgetln` 直接给长度与指针。
  **改法**：一行就是「一段字节 + 输入自己有没有那枚换行」——`Line` 的内容字段换成 `&[u8]`（`text()` 撤、`bytes()` 立），`lines_of` 按 `b'\n'` 直接切字节；取法侧三处入口同步收字节：`cut.rs:fn select_bytes`、`cut.rs:fn select_fields`、`uniq.rs:fn comparison_key`。**不逐行解码再换成替换字符**的理由是那一改等于替输入改写内容：`cut` 的位置与 `uniq` 的比较都定义在字节上，把 `\xff` 换成 U+FFFD 会让一格的字节变成三格、也让本来不等的两行变得相等。`From<&str>` 腿保留给测试与只造内容的调用方，代价是想拿 `&str` 的下游腿必须自己声明拿的是内容还是字节（`tail.rs`、`uniq.rs` 的测试助手现在显式转换并 expect，就是这个声明）。
  **门与读数（最终形态）**：门 1188→**1191**（库 +3：`an_undecodable_byte_is_still_a_line`、`a_byte_that_is_not_utf8_occupies_one_position`、`a_byte_that_is_not_utf8_counts_as_one_skip_step`）。宿主 `cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` **1191 passed / 0 failed**（68 段，`tmp/lines-430/host-final.log`）；容器 `minix-ci:1.94` 同配方 **1191 / 0**、退码 0（`container-gate2.log`）；三架构 `--bins` 全退 0；`clippy --all-targets` 与主树基线按「告警文本 + 文件」逐条对读**新增 0 条**（`clip-claim.txt` 与 `clip-main.txt` 的 `comm` 空集）；`rustfmt --check` 与基线比**新增漂移 0 件**（本轮自己那条链式调用已被 `rustfmt --edition 2024` 收回一行）。五道收尾门全过，仅存 C 板那枚已知探针命名族漂移（台账 31 对现量 18）。
  **门打在门自己身上**：负控 **5 枚**（`tmp/lines-430/negctl.py`，每枚注入后立即跑 `--lib`、红名必须点名本轮三枚新门之一，`finally` 无条件还原）——N1 `lines_of` 退回「先整份解码再切」（缺陷原形复活）咬住；N2 把解不出 UTF-8 的那一行整行丢掉咬住；N3 `select_bytes` 解码后再数位置咬住；N4 `comparison_key` 把高位字节当成一个字符的第二格咬住；N5 等价注入（`Line::new` 展开成同样的字段构造）全绿，证明门不是见什么都咬。
  **容器门的时序教训（新增判据）**：**容器权威门不得与任何 cargo 构建或测试并发**。并发跑时容器报 83 红——两者共用同一个 `os/target`，前一份构建的产物在后一份执行途中被替换，红的是测量装置不是被测对象；串行复跑即 1191/0。这与批次四百二十三那条「参照件开工先验归属」同类：先把装置本身排除，再谈代码。
  **A/B 现量与唯一残差**：`tmp/lines-430/ab.py` 对五支参照件（head/tail/nl/uniq/cut）跑 19 形，**18 形逐字节同、1 形不同**：`cut -c 2` 本壳交回 `ff 0a 0a 0a 6c 0a 61 0a`、参照件交回空（退码同为 0）。差异在 C 的 `-c` 腿本身：`minix3/usr.bin/cut/x_cut.c` 把同一份代码编两份，`b_cut` 的取放宏是 `getc`/`putchar`（`:45-50`），`c_cut` 的是 `getwc`/`putwchar`（`:52-57`）——`-c` 数的是**字符位置**，撞上非法字节后整条腿交回空；本壳 `-c` 目前与 `-b` 同走字节。
  **登记与闭合**：闭 `LINES-OF-NONUTF8-DROPS-ALL`（板 A2 行同步记闭）。**新登记不执行**：`CUT-C-LEG-CHARACTER-ORIENTED`（`-c` 应按字符位置数，`x_cut.c:52-57`；改它要先定 `no_std` 下"字符位置"按什么算）、`TEXTFILTER-NONUTF8-DECODE-SITE-CENSUS`（现量普查：整份 `unwrap_or("")` 还剩 8 处、直接 `.unwrap()` 的 2 处即 `nl.rs:240` 与 `join.rs:723`（非 UTF-8 输入当场 panic）、`from_utf8_lossy` 22 处散在 14 件；三族各要不同裁决，不是一次机械替换）。07 篇表头库测试数按现量对账 577→**586**，并在「取头取尾」那条补上行为什么按字节承载。

- `2026-10-20` qoder / todo 循环第一百四十五笔（批次四百三十一，P0 独立轮：**`UNEXPAND-BLANK-LINE-END-FLUSH-NEEDS-TERMINATOR` 闭**）：`claim/UNEXPAND-BLANK-LINE-END-FLUSH-NEEDS-TERMINATOR-qoder` @ 基线 `234e91a91` → 码笔 `cca07652b`、文档笔 `dbd514d1b`
  **判的是「行末的空白串由谁冲出来」**：C 的 `tabify` 只在非空白字节上冲档（`minix3/usr.bin/unexpand/unexpand.c:167-205`），走到缓冲末尾直接 `return`（`:214-215`），一个字节也不冲。而 `unexpand.c:133` 的 `fgetln` 把换行连同内容一起交回，所以**行末那枚换行本身就是那个非空白字节**：一串结尾空白后面有换行就冲出来，输入的最后一行没有换行就永远等不到触发者。本壳的行腿拿到的是剥掉换行的行体，这一形在这层原本没有答案——批次四百二十六把这枚差异量清并按「不冲」登记，本轮把它按机制补全。
  **改法**（板上登记两条施工路的第①条）：`unexpand_line(line, options, terminated: bool)`；冲档的两腿抽成 `flush_gap(out, ocol, dcol, stops, limit)`，**循环内与行末共用同一份实现**（两处各写一遍就会各错一遍），行末只在 `terminated` 为真时调用；`unexpand()` 用 `window::final_line_terminated(input.as_bytes())` 给末行定终止状态，其余每一行都带着换行——判断落在输入的字节上而不是行号上，所以换行结尾时末行仍属「带换行」那一形。第②条（把换行留在行体里）要动 `unexpand()` 的返回值契约与壳侧写口，本轮不走。
  **门 1191→1193**：新增 `unexpand::tests::a_blank_line_ending_in_a_newline_flushes_its_gap`（`-t 4,8` 的 `    ` 带换行冲成 `\t`；`   ` 带换行够不着 8 这一档，就照原样落三枚空格不折成制表符）与 `the_last_line_flushes_only_when_the_input_terminated_it`（同选项下 `"    \n"`、`"    "`、`"    \n    "` 三形的分岔完全由终止符决定）。另把批次四百二十六那枚门里四行「纯空白行交回空」的断言改成 `terminated = false`——它们建模的正是没有换行那一形，两枚门合起来才是同一条机制的两半。
  **读数（L1，日志落 `tmp/unexpand-431/`）**：宿主 **1193 passed / 0 failed**（68 段，`host.log`）；容器 **1193 / 0**、退码 0（`container.log`，本轮单独跑，按批次四百三十定的判据不与任何 cargo 任务并发）；三架构 `--bins` 全退 0；`clippy --all-targets` 与主树基线逐条对读新增 0 条；`rustfmt --check` 与基线比新增漂移 0 件；五道收尾门全过（仅存 C 板那枚已知探针命名族漂移）。
  **负控 5 枚**：N1 `if terminated` 改 `if true`（无条件冲）咬住；N2 改 `if false`（永不冲）咬住；N3 plumbing 丢终止符（`unexpand()` 每行传 `true`）咬住；N4 一枚按判据当场作废——等价注入的锚点 `flush_gap(...)` 那一行在文件里出现两次（循环内与行末各一次），脚本按「锚点先数次数」拒绝注入并报出计数 2，改用带注释行的长锚点后成为 N5；N5 等价注入（`&options.stops` 写作 `options.stops.as_slice()`）全绿。
  **A/B 现量**：对 `tmp/unexpand-312/unexpand_ref` 跑 15 形（批次四百二十六那张矩阵 12 形加三形，把带换行与不带换行两半都覆盖到 `-a` 与 `-t 4,8` 上），**改前 11 同 4 异、改后 15 形全同**（`eol.log`）。差异方向与批次四百二十六那次相反：那轮钉的是「无条件冲」那一版的过冲，本轮修的是「一律不冲」的欠冲——两端都被现量量过，中间那条判据（冲与不冲由输入的终止符决定，不由行号决定）才立得住。
  **登记与闭合**：闭 `UNEXPAND-BLANK-LINE-END-FLUSH-NEEDS-TERMINATOR`（板 A2 行同步记闭）。同族的 `CUT-NEWLINE-DELIMITER-NEEDS-TERMINATOR` 维持在册并改注施工位置：它不吃这一层的终止符位，C 的 `b_cut` 逐字节读、把 `'\n'` 也当普通字段内容比（`minix3/usr.bin/cut/cut.c:244-251`），要让 `-d $'\n'` 同形得让 `select_fields` 看得见那枚字节，属独立一轮。07 篇表头库测试数按现量对账 586→**588**。

- `2026-10-20` qoder / todo 循环第一百四十六笔（批次四百三十二，P0 独立轮：**`CUT-NEWLINE-DELIMITER-NEEDS-TERMINATOR` 闭**）：`claim/CUT-NEWLINE-DELIMITER-NEEDS-TERMINATOR-qoder` @ 基线 `af2bf5881` → 码笔 `92d7ad770`、文档笔 `5f1cbd854`
  **判的是「把换行当分隔符时，行末那一格算不算分隔符」**：`select_fields` 用「行体里找不找得到那枚字节」判这一行带不带分隔符，而它拿到的行体是**切行之后的产物**，里面永远没有换行。于是 `-d $'\n'` 的每一行都被判成不带分隔符——`-s` 抹掉整行，不加 `-s` 整行原样搬运（夹具 `a\nb c\n` 的 `-f 2` 交回 `a\nb c\n`，加 `-s` 交回零字节）；参照件两种都交回 `\n\n`（每行一枚换行、字段为空）。
  **真源为什么必然命中**：C 扫一行的循环是「等于 `sep` 就记下 `isdelim`，等于 `\n` 就收尾」（`minix3/usr.bin/cut/cut.c:242-251`，注释 `this should work if newline is delimiter` 正写在那一判上），而**交给这个循环的那一串总是以换行结尾**——`fgetln` 交回的行长含换行，末行没有换行时 C 当场 `malloc(len + 1)` 补一枚（`cut.c:235-241`）。所以 `sep` 是换行时两个判据在同一格同时成立：「整行原样搬运」（`!isdelim && !sflag`）与「`-s` 抹掉整行」（`if (!isdelim) continue;`）两腿都不走，剩下的是通常字段扫描——第一格是行体、其后没有内容、行末照旧一枚换行。
  **改法与为什么这样改**：把那一格补进判据（分隔符是换行即算带着分隔符，否则回到行体里找），**不把换行塞回行体**。行体与终止符分开承载是批次四百二十三定的口径（`Line::terminated`），而 `cut.c` 这一段要求把「行末有换行」当成内容的一部分来扫；这是切分模型与 C 的扫描模型之间的一处真实差异，落在一条判据上最省事，也最不会误导下一位来核对的人。
  **门 +2**：库 `cut::tests::a_newline_delimiter_makes_every_line_a_fielded_line`（`-f 2` 交回 `Some(0)`——走字段腿、第二格为空，不是整行搬运的 `Some(len)`；加 `-s` 同样 `Some(0)`，与批次四百二十八那枚 `None` 语义相反；换成 `;` 时同一行仍是 `None`）、壳 `cut_face::a_newline_delimiter_keeps_every_line_in_the_field_leg`（`-f 2` 与 `-f 2 -s` 都交回 `\n\n`，`-f 1` 交回 `a\nb c\n`）。
  **读数（L1，日志落 `tmp/cut-432/`）**：宿主 **1195 passed / 0 failed**（68 段，`host2.log`）；容器 `minix-ci:1.94` 同配方单独跑 **1195 / 0**、退码 0（`container.log`）；三架构 `--bins` 全退 0；`clippy --all-targets` 与主树基线逐条对读新增 0 条；`rustfmt --check` 与基线比新增漂移 0 件（自己那条长 `assert_eq!` 与 `tests/cut_face.rs` 一并收回格式）。负控 3 枚：N1 撤判据咬住本轮新门；N2 一律算带分隔符（把面分隔符那一腿也抹掉）咬住批次四百二十八那枚 `None` 语义门；N3 两判据换序的等价注入全绿。五道收尾门全过（仅存 C 板那枚已知探针命名族漂移）。
  **A/B 现量**：14 形（换行与面分隔符各 7 形：`-f 1`、`-f 1-2`、`-f 2`、`-f 2-3`、`-f 2 -s`、`-f 2 -s -n`、`-b 2`）对 `tmp/cut-406/cut_ref` **逐字节全同**；改前三形红（`-f 2`、`-f 2 -s`、`-f 2 -s -n` 在换行分隔符下），读数 `ab.log`。
  **登记与闭合**：闭 `CUT-NEWLINE-DELIMITER-NEEDS-TERMINATOR`（板 A2 行同步记闭）。同族另一枚 `UNEXPAND-BLANK-LINE-END-FLUSH-NEEDS-TERMINATOR` 已由批次四百三十一闭；`CUT-C-LEG-CHARACTER-ORIENTED` 维持在册——那一枚是 `-c` 该按字符位置数（`x_cut.c:52-57` 的 `getwc`/`putwchar`），与终止符无关。07 篇表头库测试数按现量对账 588→**589**。

- `2026-10-20` qoder / todo 循环第一百四十七笔（批次四百三十三，**预研轮：`TEXTFILTER-NONUTF8-DECODE-SITE-CENSUS` 量清并更正批次四百三十的登记**）：`claim/TEXTFILTER-NONUTF8-DECODE-SITE-CENSUS-qoder` @ 基线 `8cfda22fe` → 文档笔 `242f43a50`
  **为什么先普查再动手**。批次四百三十把行腿改成按字节之后，同一族还剩一批「先解一次 UTF-8 再干活」的地方。它们看着一样，处置分三类：必须按字节保真的、C 自己就是宽字符向（遇非法字节**报错退场**，照字节改反而错）、压根不是生产代码。不分类就动手会把第二类改坏。本轮不落码，只做量清与施工顺序，并更正上一轮的普查句。
  **量法**：夹具 `tmp/census-433/in` 写字面字节 `a\xffb\nc\n`（一枚裸 `\xff`，不是它的 UTF-8 转义——这条是批次四百二十定的判据），本壳现量与参照件现量并排落在 `tmp/census-433/ref-readout.log`。
  **甲组 4 处（生产、修法既定）**：`bin/pr.rs:fn split_lines`、`bin/column.rs:fn collect`、`bin/comm.rs:fn split_lines`、`bin/paste.rs:fn split_lines`——形状完全相同：收 `&[u8]`、第一句 `core::str::from_utf8(...).unwrap_or("")`、再按换行切。现量：`pr` 与 `column` 本壳交回**空**、参照件分别交回 `a 377 b \n c \n` 与 `a 377 b \t c \n`；`comm` 本壳只剩 `\t x \n`（把整个非 UTF-8 文件当空文件）、参照件 `a 377 b \n c \n \t x \n`；`paste` 本壳 `\t x \n`（非 UTF-8 那份的行全丢）、参照件 `a 377 b \t x \n c \t \n`。这就是批次四百三十在 `lines_of` 上修掉的缺陷换了函数名，修法沿用：切分交给字节，行内容一路按 `&[u8]` 传到写出侧。
  **乙组 1 处（会改坏的那类，待裁决）**：`rev`。本壳交回空且无报文；参照件 `tmp/rev-309/rev_ref` 交回**空 stdout 加一句 stderr** `stdin: Invalid or incomplete multibyte or wide character`。C 的 `rev.c` 走宽字符腿，非法序列是报错而不是忽略——与 `x_cut.c` 的 `c_cut`（`getwc`/`putwchar`，`:52-57`）同族，即板上在册的 `CUT-C-LEG-CHARACTER-ORIENTED` 的姊妹形。新登记 `REV-MULTIBYTE-ERROR-LEG`：要么建模「遇非法序列即报错退场」（含报文与非零退码），要么明确决定不建模并记成已知差异，这条要人裁决。
  **丙组 3 处（本笔更正上一轮）**：批次四百三十写着「直接 `.unwrap()` 的 2 处（`nl.rs:240`、`join.rs:723`，非 UTF-8 输入即 panic）」——**不成立**：两行都在各自 `mod tests` 之内（`src/nl.rs:mod tests` 起于 `:224`、`src/join.rs:mod tests` 起于 `:708`），是测试助手把输出字节转回 `String` 比对，panic 条件是**夹具写错**而不是输入里的字节，生产腿不受影响。另 `src/unexpand.rs:fn unexpand` 里那句 `from_utf8(input.as_bytes()).unwrap_or("")`（`:208-209`）入参本来就是 `&str`，再解一次**永不失败**，属可删的死防御；`unexpand` 真可能整份丢字节的位置在壳侧读入路径，不在这一句。
  **规模与顺序**：甲组 4 处（生产、按既定口径）、乙组 1 处（待裁决）、丙组 3 处（2 处测试腿不修、1 处死防御可随手删）；`units` 与 `tsort` 本轮未测（夹具形状不同：`units` 要单位表达式，现量是 `units: Can't find units file`；`tsort` 要成对的边表），`from_utf8_lossy` 那 22 处单列——其中既有选项字母的兜底（参数缝半口径，保留）也有内容被改写（要换成字节），不能一把替换。施工顺序定成三支：`pr` 一支、`column` 一支、`comm` 与 `paste` 并一支（两处 `split_lines` 同形，一次改完让两支的门同批翻）。本轮零代码改动，`git status` 仅 `rewrite-notes/` 三件，编译与测试门按基线不变。

- `2026-10-20` qoder / todo 循环第一百四十八笔（批次四百三十四，**甲组第一支：`paste` 与 `comm` 的行腿、分隔符腿、折叠腿一起改回字节**）：`claim/TEXTFILTER-NONUTF8-DECODE-SITE-CENSUS-qoder` @ 基线 `676607a11` → 码笔 `fa10961d2`（＋注释锚点更正 `ee517c93f`）、文档笔 `e4c7d5927`
  **为什么两支并一支**。批次四百三十三把「先把整份输入解一次 UTF-8、再按换行切」这个形状量成三组，甲组四处是同一件缺陷的四个现场，施工顺序定成三支：`pr` 一支、`column` 一支、`comm` 与 `paste` 并一支。并一支的理由是两处的切分函数**逐字同形**（同名同体），一次改完让两支的门在同一批里翻；拆开改就是把同一个形状写两遍判据。
  **C 那一侧三条腿都是「按长度搬字节」**。`paste` 并行腿：`fgetln` 交回指针加长度（`minix3/usr.bin/paste/paste.c:121`），末尾换行是减掉的切分符（`:138-139`），行内容按 `%.*s` 的精度以量出来的长度发出（`:140`）；分隔符腿是 `ch = *dp++; if (ch) putchar(ch);`（`:131-133`、`:148-150`），`if (ch)` 只压掉值为 0 的那一位。`paste` 顺序腿同理：`fgets` 拿行、`strchr` 找到换行就改写成串尾、`printf("%s", buf)` 发出（`:175-191`）。`comm` 的行腿更直白：`while ((c = getc(fp)) != '\n' && c != EOF) buf[i++] = c;`（`minix3/usr.bin/comm/comm.c:200-205`）。
  **改前三处失真（现量 `tmp/paste-comm-434/ab.log`，两支参照件都先过了 `-Z` 归属验字样）**。其一，一枚非 UTF-8 字节抹掉**整个档**的行：夹具 `raw.txt` = `61 ff 62 0a 63 0a`，`paste raw.txt raw.txt` 参照件打 `61 ff 62 09 61 ff 62 0a 63 09 63 0a`、本壳交回零字节，`comm` 的三列同理归零；混合形（好档在前）露出机制——好档的行照打、坏档那一格空，因为解一次 UTF-8 得空串、`"".split('\n')` 交回一个空 piece、末位换行再把它弹掉，于是零条记录。其二，分隔符位经 `char` 域：`slots[dp] as char` 把一枚裸 `ff` 升成 U+00FF，落进 `String` 是 `c3 bf` **两字节**，而 `putchar` 写的是那**一**字节。其三，`comm -f` 的折叠用了字符域小写：`c3 89`（É）与 `c3 a9`（é）被折成同一个码点、判成公共行落第三列，而 `strcasecmp` 两侧各按 `unsigned char` 查 ctype 表（`minix3/common/lib/libc/string/strcasecmp.c:58-67`），C locale 下那张表只折 `A`–`Z`，两行各落自己那一列（参照件打四行、本壳改前打两行）。
  **改后形状**。两个壳的 `split_lines` 收 `&[u8]` 交回 `Vec<Vec<u8>>`（`os/commands/usr-bin/textfilter/src/bin/paste.rs:fn split_lines`、`os/commands/usr-bin/textfilter/src/bin/comm.rs:fn split_lines`）；库里 `join_serial`/`join_parallel` 收 `&[&[u8]]` 交回 `Vec<u8>`，`delimiter_char` 换 `delimiter_byte`（`os/commands/usr-bin/textfilter/src/paste.rs:fn_delimiter_byte`）；`comm` 的行类型 `CommLine.text` 改成 `&[u8]`，`-f` 折叠换成逐字节 `to_ascii_lowercase`（`os/commands/usr-bin/textfilter/src/comm.rs:fn_line_cmp`）。不折叠那一腿本来就是字节序：`strcoll` 无条件转 `strcmp`，源文件自己写着「LC_COLLATE 未实现，因此永远是 C」（`minix3/lib/libc/string/strcoll.c:56-72`）。
  **门 1195→1201**（库 +4、面 +2，夹具一律改写字面字节）。库侧 `paste::tests::a_delimiter_slot_is_one_byte_not_a_codepoint`、`paste::tests::a_byte_that_is_not_utf8_is_still_part_of_the_line`、`comm::tests::a_byte_that_is_not_utf8_is_still_a_comparable_line`、`comm::tests::the_fold_is_a_per_byte_ctype_lookup_not_a_unicode_lowercase`；面侧 `an_operand_line_that_is_not_utf8_keeps_every_byte`（四形：同档两列、好档带坏档、`-s`、`-` 走标准输入）、`a_nonutf8_line_is_a_record_and_the_fold_is_per_byte`（三形：非 UTF-8 共有行、不折叠、`-f`）。新折叠门内**同时**留了 `Apple`/`apple` 判等这一半——只测「É 与 é 不等」的门，被「把折叠整个撤掉」这种改法也能骗过去，加上 ASCII 那一半才两头堵。
  **门读数（全 L1）**：宿主 **1201/0**（68 段）、容器 **1201/0**（68 段，单独跑——批次四百三十定的判据「容器权威门不得与任何 cargo 并发」）、三架构 `x86_64`/`aarch64`/`riscv64gc` 退 0、触及六件 clippy 告警 **0** 条（主树同法读数 0 条）、`rustfmt` 新增漂移 **0** 件（本树与主树同为 21 处 `Diff in`，全在未触及件）、负控六枚（`tmp/paste-comm-434/negctl.log`，5 咬 + 1 等价）。**A/B 终读数 15 形 14 同 1 异**（改前 2 同 12 异）。
  **唯一残差不是本轮的**。`paste -d` 的分隔符里塞一枚裸 `ff`：参照件打 `apple\xffbanana`，本壳打 `apple\xefbanana`——参数进到本层之前经 `from_utf8_lossy`，一字节变成 `ef bf bd` 三字节，于是分隔符表成了**三位**、按位循环取到第一位。这是已在 07 篇登记的 `PASTE-NONUTF8-DELIM`，属板 A2 的参数缝半（射程②）。本轮把库里的发出腿改成字节，正是为了让参数缝修好之后那一腿不用再动。
  **新登记一条不执行（本轮现量挖出）**：`PASTE-COMM-NUL-TRUNCATES-PRINT-LEG`。C 这两条发出腿都以 NUL 结尾（`paste.c:180` 把换行改写成串尾、`comm.c:210` 写串尾），`%s`/`%.*s` 都停在串尾，于是**行体内含值为 0 的字节时 C 在那里截断**。现量：`tmp/paste-321/paste_ref -` 吃 `61 00 62 0a 63 0a` 打 `61 0a 63 0a`；`tmp/comm-409/comm_ref /dev/stdin nul.txt` 打 `09 09 61 0a 63 0a 09 7a 0a`（共有行只剩一个 `61`）。本壳按字节发出整行、改前发零字节——两边都不是 C 的形状，所以这不是本轮改坏的，而是本轮把行腿理顺之后**新暴露**出来的一条；修法在发出腿（按 C 的串尾截断），不在切分腿（改回文本等于把本轮改的东西撤掉）。
  **本轮固化的一条判据**：**注释里的 C 行号也要逐条回数，不许按记忆写**。码笔落地后逐条核对，发现本轮新写的五处题注有 2–4 行漂移，另外两处把机制说反了——`parallel` 发的是 `printf("%.*s", (int)line_len, line)`（`paste.c:138-140`）不是 `fwrite`，`sequential` 发的是 `printf("%s", buf)`（`:181`）不是 `putchar`，`comm` 的行腿是 `:200-205` 那个 `getc` 环。这类错误不会红任何测试，只能靠「写注释时也在场」的人回源文件数一遍，故单独一笔 `ee517c93f` 更正，并顺带记下：`src/paste.rs` 模块头与 `join_parallel` 里**既有**的区间题注（`paste.c:111-140`、`:127-133`、`:144-150`）实测落在 `:114-141`、`:129-136`、`:147-152`，属 `TEXTFILTER-COMMENT-ANCHOR-AUDIT-REST` 的 paste 族证据，本轮不扩大改动面。
  **五道收尾门**：`todo-reconcile-scan.sh` 唯一 DRIFT 仍是 C 板的「探针命名族 31 对 18」（与本线无关，前若干笔同读数）；`notes-link-check.py rewrite-notes/` 断链 **131**（基线不变）；`todo-staleness-check.sh` PASS 0 项；`doc-style-lint.sh --diff 676607a11..HEAD`（3 文件 / 121 新增行）**error 级 0**；`check-staged-inflight.sh` 无在制文件。**余量**：甲组还剩 `pr`（`Page`/`render_page`/`merge_body_rows` 三处是 `String`，是一次库面重构）与 `column`（`bin/column.rs:fn collect` 单点）两支；`units`/`tsort` 要各自夹具形状；`from_utf8_lossy` 那 22 处分两类口径；乙组 `REV-MULTIBYTE-ERROR-LEG` 与 `CUT-C-LEG-CHARACTER-ORIENTED` 待人裁决。

- `2026-10-20` zcode-glm / C 板无人值守循环（PARALLEL-C：**run_all 32/7/3→39/0/0 七红连压 + C3 信号腿 + C12 验收收官 + P0 修复落地**）：`claim/E5-zcode-glm`（三轮）、`claim/SD-41-zcode-glm`、`claim/C12-zcode-glm`（保开→全合并）、`claim/SD-10-zcode-glm` 等先后领做，全部合入 rewrite。**C6 七红**（32/7/3→39/0/0，账面读数两次全量复核）：①rt-birth/timer-irq-riscv64 载体内嵌 ELF 起址未页对齐（`#[repr(C, align(4096))]` 包裹，契约侧修复）②smp-topo-aarch64+rt-birth-aarch64 的脚本缺 `gic-version=3`（QEMU 默认 GICv2 令 AAVMF ACPI 解析红）③protection-aarch64 载体探针表与 minix-arch 生产 K9 表 `duplicate symbol`（符号改名 `_probe` 分家）④rt-birth-x86（同①第三姊妹；`guest never reached` 为超时假签、真红在其后串口）。**C3 信号腿**：riscv64+aarch64 用户同步异常→信号 acting 臂（C exception.c:275 用户源 cause_sig parity；映射纯函数宿主 2 枚；mock 834/0/8、ATF 36/36 回归）。**C12 验收三层**：①FpuTrap 臂 BKL 丢失释放死锁（BklSection 不随 drop 释放、iretq 直返用户缺显式 unlock——gdb 活体解剖三抓定案，已修 `89f4920f1`）②**P0 新债：x86 用户 FP 无内核 excursion 保护**（x86_64-unknown-uefi 默认开 SSE=内核代码自由用 xmm、陷阱帧仅 GPR；当日采选项 2 修复=uefi 目标 `-C target-feature=-sse,-sse2,-mmx`——C i386 基线无 SSE+Linux `-mno-sse` 同款机制，台账行当日闭）③FP 交替探针真机双绿（RX 42.0/TX 7.5 全链保活，sysboot 腿 FP 判据常驻 run_all）。**C17 D1 shim 半**：ESP `/dumpdtb.dtb` 出界闸+AAVMF 两腿 PASS。**C11 换形裁决申请**已提交编排线（附录一，甲乙两案+三裁点）。工具沉淀：BklSection「不随 drop 释放」须显式 unlock、gdb 活体解剖（-s + x/16i $pc-24）、qemu monitor 无 i386 反汇编改 x/20bx 手工解码、lld 找不到 .ld 先查指纹绝对路径（build.rs 烙 CARGO_MANIFEST_DIR 的孤儿指纹）、超时假签掩盖真红（读全串口）。剩余行全数外部门控（C2b 撤钳原子批、C4/C17-xtask 等 A 板、C7 族等文档线、C11/C1 案结等编排线、C3 首源等 B 板、C8/C13/C15/C19/C20/C21 人裁绑定），门控地图与解除条件见 PARALLEL-C 各行。

- `2026-10-20` qoder / todo 循环第一百四十九笔（批次四百三十五，**甲组第二支：`column` 的条目池、分隔符集、布局宽度一起按字节**）：`claim/TEXTFILTER-NONUTF8-DECODE-SITE-CENSUS-qoder` @ 基线 `5ef29f10f`（轮内 rewrite 被别板推进 6 笔，领用树内 rebase 后门按新基线复跑）→ 码笔 `641172ced`（＋注释锚点回数更正 `734aec68a`）、文档笔 `f2b9a6c7c`
  **这一支与第一支的差别**。批次四百三十三定的三支施工里，`comm`+`paste` 那一支只管「行按字节搬」；`column` 还**拿行的长度当布局的输入**：`maxlength` 决定要不要塌成每条一行（`minix3/usr.bin/column/column.c:131` 那一判），`-t` 的 `lens[col]` 决定每列补几个空格（`:249-251`）。所以字节口径在这一支同时钉条目、分隔符表、量长与补齐三处——「只改切分腿」在这里改不完。
  **C 那一侧一次解码都没有**。`input` 用 `fgetln` 一字节一字节读进 `buf`、`len = p - buf` 是两枚指针相减（`:272-296`）；`maketbl` 用 `strtok(p, separator)` 按字节表切字段（`:224-225`）、用 `strlen` 量字段长（`:243`）；发出腿非末列走 `printf("%s%*s", …, lens[coloff] - t->len[coloff] + 2, " ")`（`:249-251`），**最后一列单独一句 `printf("%s\n", t->list[coloff])`（`:252`）**——不在补齐的环里，也就不补空格、不裁行尾。
  **改前 7 红（现量 `tmp/column-435/ab-prefix.log`，参照件 `tmp/column-314/column` 经同名软链起、开工先过 `-Z` 归属验字样）**。六红是同一族：夹具 `61 ff 62 0a 7a 0a` 一进池，本壳解一次 UTF-8 得空串、于是**整个档一条都不交**（纵向、横向、`-c 20`、`-t`、`-t -s '|'` 全打零字节），而多操数是共用一个条目池（`:114-123`），好档带坏档那形打的是「好档的行 + 坏档全无」，退码还是 0——这种错最难发现。第七红与解码无关：`-t` 末字段自带回车时参照件把 `0d` 原样留在行尾，本壳「补齐所有列再整行 `trim_end`」把它裁掉了。`column` 比 `paste`/`comm` 还多一层放大：条目池空时壳按 `eval` 直接返回（`:125-126`），连布局都不进。
  **改后形状**。`src/bin/column.rs:fn collect` 收字节段、发出腿 `support::emit` 交字节；`src/column.rs` 的 `EntryPool::entries`、`scan_entries`、`columnate`、`table_mode`、`c_columnate`、`r_columnate`、`maxlen` 全线 `String`→`Vec<u8>`，`split_fields` 的分隔符判换成按字节 `separator.contains(byte)`，表模式改为「只补非末字段、末字段原样发出」。前导空白那一判在 `&[u8]` 上没有现成的 `trim_start_matches`，改手写游标，集合维持空格与制表两枚不变。
  **门 1201→1205**（库 +3、面 +1，夹具一律字面字节）。库侧 `column::tests::a_byte_that_is_not_utf8_is_still_an_entry_and_its_layout_is_bytes`、`column::tests::the_separator_is_a_set_of_bytes_not_a_set_of_characters`（`-s é` 的值是 `c3 a9`，条目里一枚单独的 `a9` 在 C 就是分隔符）、`column::tests::a_trailing_carriage_return_survives_because_the_last_column_is_printed_raw`；面侧 `tests/column_face.rs:an_entry_that_is_not_utf8_keeps_its_bytes_through_every_layout`（四形：纵向、横向、好档带坏档、`-t` 补齐）。
  **门读数（全 L1）**：宿主 **1205/0**（68 段）、容器 **1205/0**（68 段，单独跑）、三架构退 0、触及件 clippy 告警 **0** 条、`rustfmt` 新增漂移 **0** 件、负控五枚（`tmp/column-435/negctl.log`，4 咬 + 1 等价全绿）、**A/B 终读数 12 形 12 同 0 异**（改前 5 同 7 异）。rebase 后按新基线复跑：宿主与容器各 1205/0、三架构退 0、A/B 仍 12/12，`git show` 逐字节对账新旧两笔的 patch **完全相同**。
  **本轮固化的三条判据**。① **面门成片红先看产物形状**：宿主门第一轮 6 个 target 全报 `Exec format error`，取证是 `os/target/debug/{cut,join,rev,units,vis}` 五枚产物首 16 字节全零（`od` 现量）——上次构建被中断留下的半成品，`cargo clean -p minix-textfilter` 复跑即全绿，把「靶子坏了」读成「代码坏了」就会白改一轮。② **clippy 按告警位置判，不拿总数当棘轮**：`String::push(' ')` 变 `Vec<u8>::push(b' ')` 之后才触发 `same_item_push`（该 lint 只管 `Vec`），改用本 crate 既有写法 `extend(core::iter::repeat_n(b' ', …))`（同 `os/commands/usr-bin/textfilter/src/pr.rs:fn expand_tabs`）归零。③ **负控的红名对账要同类相比**：`--lib` 交回带模块前缀的名字（`column::tests::…`）、面文件交回裸名，脚本按全名比就把咬住的门判成「漏咬」——先怀疑对账自己的形状，再怀疑注入。
  **新登记一条不执行**：`COLUMN-BLANK-SET-NARROWER-THAN-ISSPACE`。C 的空行判 `for (p = buf; *p && isspace((unsigned char)*p); ++p); if (!*p) continue;`（`:274-278`）里 `isspace` 含 `' ' \t \n \v \f \r` 六枚，本壳只跳空格与制表两枚（该集合本轮之前就如此）。差别落在「整条空白又没有换行符」的末行：参照件静默丢弃退 0，本壳当残行打 `column: line too long` 并退 1。现量 `tmp/column-435/blankset.log`（复跑 `python3 tmp/column-435/blankset.py`）：`0b`、`0b 0b`、`0c 0d`、`0d` 四形参照件 rc=0 零输出、本壳 rc=1 加警告；`0d 0a` 参照件连换行一起当空白丢、本壳交回一行；对照形 `20 09` 两边同形，证明差别确实只在 `\v \f \r` 三枚。修法是把跳过集合换成 C 的 ctype 判，属另一条口径，与「字节 vs 文本」无关。
  **本轮顺带归位的锚点**：写注释时一并回数，订正五处——`strtok` 在 `:224-225`（旧注 `214-231` 落在声明与 realloc 段）、`input` 循环 `:272-296`（旧注 `266-299`/`270-297` 越到 `usage` 头上）、补齐腿 `:249-251`（旧注 `252-256` 把末列那句算了进去）、`maketbl` 字段环 `:223-247`、`maxlength >= termwidth` 判在 `:131`（旧注 `:130` 是 `maketbl();`）。单独一笔 `734aec68a`，`git diff -U0` 逐行确认非注释行改动 0 处。
  **五道收尾门**：`todo-reconcile-scan.sh` 唯一 DRIFT 仍是 C 板「探针命名族 31 对 18」（与本线无关）；`notes-link-check.py rewrite-notes/` 断链 **131**（基线不变）；`todo-staleness-check.sh` PASS 0 项；`doc-style-lint.sh --diff 5ef29f10f..HEAD`（3 文件 / 168 新增行）**error 级 0**；`check-staged-inflight.sh` 无在制文件。**余量**：甲组只剩 `pr` 一支（`Page`/`render_page`/`merge_body_rows` 三处是 `String`，是一次库面重构）；`units`/`tsort` 要各自夹具形状；`from_utf8_lossy` 那 22 处分两类口径；`CUT-C-LEG-CHARACTER-ORIENTED` 与 `REV-MULTIBYTE-ERROR-LEG` 待人裁决；板 A2 的参数缝半（射程②）仍在册。

- **里程碑第一百五十笔（批次四百三十六，A2 板，qoder）**：普查甲组的最后一支 `pr` 落地，
  `os/commands/usr-bin/textfilter` 的行腿至此全部按字节。`Page` 的三个行池、`paginate` 家族、
  三条列腿与并排腿 `merge_body_rows` 携带 `Vec<u8>`，壳侧 `split_lines` 按 `b'\n'` 切而不解码；
  `clip_to_width` 改照 `minix3/usr.bin/pr/pr.c:inln` 的字节预算硬截（若把剪点回退到字符边界，
  同一份输入会比 C 少交几个字节、页宽随之算歪）；`-n` 编号腿改为直接拼字节，因为一枚制表符经
  `as char` 进 `String` 会被重新编成两字节。门：库新增
  `a_cell_is_cut_at_bytes_even_mid_code_point`、面新增
  `a_nonutf8_line_keeps_its_bytes_through_the_line_legs`，宿主与容器各 68 段 1207/0，三架构退 0，
  `rustfmt` 零漂移，`clippy` 按告警位置与主树对账同为 34 枚存量、零新增，负控四枚咬住加一枚等价全绿
  （`tmp/pr-436/negctl.log`）。A/B 对同名软链起的参照件共 13 形，12 形逐字节同参照件
  （`tmp/pr-436/ab-postfix.log`）；基线读数 6 同 7 异存 `tmp/pr-436/ab.log`，两读之差即本支射程。
  **本轮登记（在册未执行）**：唯一残差是缺省页眉少一段时间字段——`prhead` 打印的 `buf` 前段由同文件
  `:1412` 的 `strftime` 按缺省宏 `TIMEFMT`（`:105`，该宏定义未见于仓内 `minix3/` 快照 [待验证]）造好，
  本壳 `PrOptions` 的 `time_prefix` 缺省为空串；登记 `PR-HEADER-TIME-FIELD-DEFAULT-EMPTY`，
  射程要拿操作数的修改时间与本地时区，非单轮量级，与字节口径无关。
  **判据补一条**：宿主门全绿的另一面是产物形状——上一轮已记下「先验二进制再信红」，本轮负控读数同样
  走了两次才准：解析器取 cargo 摘要块取不到红名时，改取 `test <名> ... FAILED` 行才是无歧义的读法
  （`tmp/pr-436/negctl.log` 第一版四条「构建失败」实为解析空咬）。
  提交：`cb3edcee4`（码笔）、`f0ceb0d34`（文档笔）、`c9d7dfff4` 与 `dedd9442b`（文档笔续，文风门 SL-5）。
  甲组三支至此全部落地；板 A 的 A2 行仍余参数缝半（射程②），其余在册项未动。

- **里程碑第一百五十一笔（批次四百三十七，A2 板，qoder）**：板 A 在册项 `COLUMN-BLANK-SET-NARROWER-THAN-ISSPACE`
  落地（批次四百三十五收尾时登记的普查甲组残留）。`column` 判空行的那一跳只认空格与制表符，而
  `minix3/usr.bin/column/column.c:input` 扫前导空白用的是 `isspace`，C locale 表里有六枚：空格、制表、
  换行、垂直制表 `0b`、换页 `0c`、回车 `0d`。差异的可观察面不止「少丢一条空行」：一条整是空白的末行
  若没被认定为空行，就会走「末行无换行符」那一支，多打一句 `column: line too long` 并把退码置 1，
  而参照件交的是白页、退 0。判序本身（判空在判残行之前）本壳已按 C 摆好，本轮只补字节表。
  门：库新增 `a_blank_line_is_any_run_of_isspace_bytes_not_just_space_and_tab`（夹具写字面字节）。
  现量：空白集六形经同名软链起的参照件逐字节同形（`tmp/column-437/blankset.log`，`-Z` 先验归属）；
  `column` 全谱 A/B 十二形仍全同（`tmp/column-437/ab.log`）；宿主与容器各 68 段 1208/0；三架构退 0；
  `rustfmt` 零漂移；`clippy` 在 `column.rs` 上零告警；负控四枚咬住加一枚等价全绿
  （`tmp/column-437/negctl.log`）。**负控读法补一条**：一枚「把空白整条改判成残行」的注入咬在既有门
  `test_unterminated_tail_line_is_dropped_and_counted` 上而非新门上——这既说明判序已有主人看着，
  也说明「按预期门名判咬合」会把真咬误记成未咬，读数要按红门清单记、不按预设名记。
  提交：`3a4adeec4`（码笔）、`cf67bccd3`（文档笔）、`b60835a9f`（文档笔续，补板 A2 段落那一处漏落）。
  板 A 余量：普查乙组（`from_utf8_lossy` 22 处）与丙组（`units`、`tsort` 自带夹具）、
  `PR-HEADER-TIME-FIELD-DEFAULT-EMPTY`、八支 `*-NON-PERMUTE-OPERAND-TAIL` 族余量、参数缝半（射程②）。

- **里程碑第一百五十二笔（批次四百三十八，A2 板，qoder）**：A2 行「参数缝半（射程②）」预研轮——只量体、不落码。
  量出两条改法的施工面（可复现命令见 18 篇批次四百三十八的表）：`support::args()` 的壳侧入口 42 处、
  解析层 `&[&str]` 29 处、`as_str()` 桥 11 处、操作数当路径打开 14 处、操作数进报文 39 个壳文件、
  面级非 UTF-8 夹具 42 行；失真点在 `String::from_utf8_lossy` 那一折——一枚 `ff` 变成三字节 `efbfbd`，
  凡从此回流到开档或报文的操作数都不是原字节，批次四百二十一与四百二十二 A/B 剩下的那条集外字母残差
  （`ï` 对 `ÿ`）正是这条链的末端。**裁决走甲路**（`args()` 全量改交 `Vec<Vec<u8>>`），因为乙路
  （只加字节访问口、保留 `String` 腿）的漏改是静默的、没有编译期挡；分三小轮：缝本体加四支试点 →
  `textfilter` 余 35 支与 29 个解析入口同步换 `&[&[u8]]` → `text-games` 三支并把跨壳表门
  `tests/argv_seam_face.rs` 的期望升级为逐字节等于原字节，该轮闭 A2 行。本轮生产码零改动
  （`git diff --stat` 只含 `rewrite-notes/`），故宿主/容器/三架构门无需复跑，五道收尾门照跑。
  提交：本笔文档笔一笔（无码笔）。

- **里程碑第一百五十三笔（批次四百三十九，A2 板，qoder）**：A2 行「参数缝半（射程②）」的落地前置轮，
  生产码零改动（`git diff --stat` 只含 `rewrite-notes/`）。两件现量各定一件事。**可观察形**：一枚带裸
  `ff` 的档名交给 `os/commands` 下 28 支壳，19 支把该字节折成三字节 `efbfbd` 打进报文（`head`、
  `column`、`wc`、`colrm` 四形逐字节入 18 篇表），9 支的用法腿在碰到操作数前退出故不可观察；C 侧同一
  形打原字节，依据 `minix3/usr.bin/head/head.c:main` 的 `warn("%s", *argv)`、
  `minix3/usr.bin/wc/wc.c:cnt` 的 `warn("%s", file)`、`minix3/usr.bin/column/column.c:main` 的
  `warn("Cannot open \`%s'", *argv)`。**最小可编译单位**：只把缝本体
  `os/commands/usr-bin/textfilter/src/bin_support.rs:fn args` 两条腿改成 `Vec<Vec<u8>>`、其余不动，
  `cargo build -p minix-textfilter --keep-going -j 2` 报 39 枚 E0308、39 支壳各红一处、lib 目标 0 错
  ——每支壳都以 `#[path = "../bin_support.rs"] mod support;` 内联同一份缝源码，改签名对全族同时生效，
  所以参数缝半的施工粒度是 crate 不是支；批次四百三十八设想的「缝本体加四支试点」不成立（要留住旧
  签名就回到乙路），A2 行的分轮据此收为两笔：`textfilter` 一笔带走缝 + 39 支壳 + 29 个解析入口换
  `&[&[u8]]` + 面门夹具字面字节，`text-games` 一笔迁三支并把
  `tests/argv_seam_face.rs:fn a_raw_nonutf8_argument_behaves_like_its_lossy_equivalent` 的期望升级为
  逐字节等于原字节，该笔闭 A2 行。另登记两条在册未执行：`ARGV-OPERAND-BYTES-NEEDS-PARSE-LAYER-AT-ONCE`
  （整族一次改）、`ARGV-BYTE-PATH-IN-LIBS`（操作数字节的开档腿与报文腿分别卡在
  `os/libs/minix-sys/src/lib.rs:299` 的 `open(path: &str, …)` 与
  `os/libs/minix-types/src/types/errno.rs:556` 的 `err_line(…, label: &str, …)`，二者在 A 板 §2 禁碰
  清单内，需库侧先行或另线裁决）。五道收尾门读数：`todo-reconcile-scan` 仅存已知探针命名族 31 对 18
  漂移、`todo-staleness-check` PASS(0)、`notes-link-check rewrite-notes` 760 篇 131 断链与基线同数且
  本笔新增 markdown 链接 0 条、`doc-style-lint --diff` error 级 0、`check-staged-inflight` 暂存区无
  在制文件。新固化一条实验纪律：翻转签名取编译面读数必须备份-还原-`git status --short` 空输出收尾，
  预研轮不在领用树留实验残留。提交：本笔里程碑笔一笔（文档笔在前）。

- **里程碑第一百五十四笔（批次四百四十，A2 板，qoder）**：A2 行「参数缝半（射程②）」第一次落到一支壳
  ——`rev` 的命令行参数全程按字节（切片 27）。**施工粒度重新定**：批次四百三十九那条「整族一次改」的
  读数是「原地改缝的签名」取到的（39/39 支壳各红一处），本轮换「新增双生口 + 逐对迁移」再取一次——
  `os/commands/usr-bin/textfilter/src/bin_support.rs:fn args_bytes`（真机腿直取 `minix_rt::crt0::args()`
  原始字节、宿主腿取 `args_os()` 的 `as_bytes()`）与 `fn prog_name_bytes`（basename 那一跳在壳侧复现，
  因为 `minix_sys::prog_name` 收 `&str` 且在 A 板 §2 禁碰清单内）、共件 `src/lib.rs:fn
  empty_optstring_operands_bytes`、`src/rev.rs:fn parse_rev_args` 换取字节形、`src/bin/rev.rs` 用字节缝；
  其余 38 支仍走旧口，同一次构建编得过。门读数：宿主与容器各 68 段 / 1211 通过 / 0 失败；三架构退 0（x86_64-unknown-none、aarch64-unknown-none、riscv64gc-unknown-none-elf 走宿主交叉编译——这三枚 bare-metal target 装在宿主工具链里，容器镜像 minix-ci:1.94 未装，同一条命令在容器里报 E0463，是配方差异不是代码缺陷）；clippy 与 rustfmt 在触及件的新代码位零告警零漂移（`bin_support.rs:fn drive_reads` 的 unused import、`rev.rs` 头行的 doc-list 告警、`col.rs`/`jot.rs` 的 value-assigned 与 `bin/cksum.rs` 的 fmt 漂移都按 `git show HEAD` 逐处核对为既存）。 跨壳表
  `tests/bin_support_face.rs:fn no_command_aborts_on_a_nonutf8_option_letter` 的 `rev` 行由 `ef` **翻正**
  为 `ff`（字母位与参照件同形，`rev: unknown option -- \xff`），其余五支仍钉 `ef`，这张表逐行翻正即本族
  进度条；门 库 +2（`lib.rs` 一枚把折叠腿与字节形的差别钉成对照断言、`rev.rs` 一枚钉原字节字母）
  面 +1（`rev_face.rs` 用 `OsString::from_vec` 传裸 `ff`，`&str` 夹具传不出）。A/B SAME=8 DIFF=1（九形，日志 `tmp/argv-440/ab.log`）：唯一残差是
  **操作数**那一腿（打不开 `no\xffpe.txt` 时回显 `efbfbd` 而参照件回显 `ff`），卡在
  `os/libs/minix-sys/src/lib.rs:299` 的 `open` 与 `os/libs/minix-types/src/types/errno.rs:556` 的
  `err_line` 都收 `&str`，即 `ARGV-BYTE-PATH-IN-LIBS` 在册的边界，不是本轮漏改。负控四枚 → 退回折叠腿咬住跨壳表 1 枚、字母判据取小写咬住 2 枚、短横判据改双横咬住 4 枚、等价改写全绿 rc=0：
  另捉两处自身形状——`prog_name_bytes` 首版交回整个 `argv[0]`，程序名就成了全路径，跨壳表面门当场钉红；
  一枚注入自身编译不过（`Cow<str>` 无 `into_bytes`）被误读成「空咬」，判据落定为**注入必须先过编译，
  红名清单要连构建退出码与编译错计数一起看**。分轮改为「壳与它的解析入口一对一笔」，末笔删 `args()`、
  `prog_name`、`empty_optstring_operands` 的 `&str` 形并迁 `text-games` 三支、把
  `tests/argv_seam_face.rs:fn a_raw_nonutf8_argument_behaves_like_its_lossy_equivalent` 的期望升级为
  逐字节等于原字节，该笔闭 A2 行。提交：码笔 + 本笔里程碑笔（文档笔在两者之间）。
- **里程碑第一百五十五笔（批次四百四十一，A2 板，qoder）**：A2 行「参数缝半（射程②）」第二对落到
  `paste`（切片 28）——按批次四百四十定的「一支壳 + 它自己的解析入口」粒度推进。这一对除了字母位，
  多修掉一处**输出内容**的真缺陷：`-d` 的值旧形进本层之前已被 `String::from_utf8_lossy` 折过，
  一枚裸 `ff` 变成 `ef bf bd`，而 `minix3/usr.bin/paste/paste.c:199-226` 的 `tr()` 是按字节数位的，
  于是分隔符表从**一位**被建成**三位**——`paste -d$'\xff' a.txt b.txt` 交出 `1\xefa`（`ef` 用在第一行，
  `bf`、`bd` 顺延到后两行），参照件 `tmp/paste-321/paste_ref` 现量是每行一枚 `ff`。改前读数留在
  `tmp/argv-441/before-probe.log`，改后与参照件同形（`tmp/argv-441/measure-ref.log`）。
  改动面：`os/commands/usr-bin/textfilter/src/paste.rs` 的 `parse_paste_args` 取 `&[&[u8]]`、
  `PasteArgs.operands` 交回 `Vec<&[u8]>`、`tr_delims` 与 `parse_args` 同批换字节形（入口内的步进
  逻辑一字未动，它本就按 `token[k]` 取字节）；`src/bin/paste.rs` 用 `support::args_bytes()` 与
  `support::prog_name_bytes()`，裸 `-` 的比较换成 `b"-"`，lossy 只留在 `read_file` 与 `warn_line`
  那两句边界（`ARGV-BYTE-PATH-IN-LIBS` 在册）。门 库 +2（一枚钉字母位是那枚裸字节、一枚钉 `-d`
  值的表位数）、面 +2（`paste_face.rs` 两枚经 `OsString::from_vec` 传参——`&str` 夹具传不出裸 `ff`）；
  跨壳表 `bin_support_face.rs` 的 `paste` 行由 `ef` 翻正为 `ff`，余四支 `tr`/`seq`/`fold`/`units`
  仍钉 `ef`。A/B 十一形 SAME=10 DIFF=1，唯一残差是**档名**那一腿：`"\xff.txt"` 参照件打得开，本壳在
  `minix_sys` 的 `open(2)` 口上折了一次，报 `paste: \xef\xbf\xbd.txt: No such file or directory`。
  宿主与容器门见 18 篇批次四百四十一段；三架构退 0（宿主跑、独立 `--target-dir`，容器镜像未装
  bare-metal target 这条配方差异沿用上轮记录）。负控四枚：退回折叠腿咬住 3 枚、`-d` 值先折一次咬住
  2 枚、字母位退回文本渲染咬住 3 枚、等价改写全绿。新判据三条——① 取红名清单要带 `--no-fail-fast`
  （`cargo test` 在第一个红目标处收摊，同一注入的读数从 1 枚变 3 枚，多腿改动会系统性低估门的覆盖面）；
  ② 字节形解析入口的寿命写在元素上（`&[&'a [u8]]`），外层借用不绑同一寿命参数，否则 `let parsed =
  args(&[...])` 那种就地数组夹具全编不过（本轮十处当场报错，`&str` 形不受此约束，因为元素是指向
  常量字符串的胖指针）；③ 包级 `cargo fmt -p` 会连带重排全包既有漂移件（本轮一度波及 23 支不相干
  文件，已全部还原并复跑门），触及件的格式门只跑 `rustfmt --check <那几件>`。
  提交：码笔 + 本笔里程碑笔（文档笔在两者之间）。
- **里程碑第一百五十六笔（批次四百四十二，A2 板，qoder）**：A2 行「参数缝半（射程②）」第三、四对
  落到 `yes` 与 `tee`（切片 29—30）。`yes` 这一对修的是**输出内容**：`minix3/usr.bin/yes/yes.c` 的
  `main` 把 `argv[1]` 存成 `const char *`，`while (puts(yes) >= 0)` 每轮把那串字节原样发出再补一个
  换行——参数既不是档名也不是旗标，它就是被无限重复打出的那些字节，所以旧形状在参数缝上先
  `String::from_utf8_lossy` 一次，`yes $'\xff'` 的每一行就成了 `ef bf bd` 三字节（改前现量
  `tmp/argv-442/before-yes.log`），改后是一枚 `ff`（`tmp/argv-442/after-yes.log`）。这一支不需要宿主
  参照件：字节语义由 C 的那一句 `puts` 直接决定，证据是改前后同形对照加 C 源那一处，
  不写 A/B 读数。面门 +3，新文件 `os/commands/usr-bin/textfilter/tests/yes_face.rs`
  （`the_repeated_word_is_the_raw_bytes_of_the_argument`、
  `a_nonutf8_byte_inside_the_word_does_not_disturb_its_neighbours`、
  `no_argument_repeats_a_lone_y`），argv 一律经 `OsString::from_vec` 传——`&str` 夹具传不出裸 `ff`；
  无限循环的夹具只取前若干字节再 `kill`，而取多少要按**整行**数：`a\xffb` 一行是四字节
  （`a`、`ff`、`b`、换行），期望第一次按可见字符数成七字节，被门当场钉红。
  `tee` 这一对**没有可观察差**：`tee.c:80` 的 `getopt("ai")` 只有两枚旗标且字母取法本就按字节
  （`src/bin/tee.rs:parse_tee_args` 里 `for &letter in &bytes[1..]`），档名那条腿折叠在
  `minix_sys::open` 的 `&str` 口上（`ARGV-BYTE-PATH-IN-LIBS`），所以 `tee -\xff` 与
  `tee $'\xff'` 两形逐字节与改前相同；它只把 `parse_tee_args` 与壳的 argv 挪到字节形，
  让族末笔删共件 `&str` 出口时少一个障碍，**不新增门也不冒充修了一处**。同轮两类必须分别记账。
  门：宿主与容器各 69 段 / 1218 通过 / 0 失败（新增一个面段），三架构退 0（宿主跑、独立
  `--target-dir`），触及件 `rustfmt --check` 零漂移；clippy 只有一条属本轮（`tee.rs` 的
  `ToString` 随 `.to_string()` 作废，已收），`items_after_test_module` 那条改前同一结构上就有。
  负控四枚：`yes` 缝退回折叠形咬住 2、去掉 `puts` 那个换行咬住 3、`tee` 聚簇只看第一个字母咬住 1、
  等价改写全绿（清单带 `--no-fail-fast`，沿用上轮判据）。余量：`textfilter` 三十九支已迁四支、
  余三十五支，共件 `empty_optstring_operands` 的 `&str` 形只剩 `colrm` 一个消费者。
  提交：码笔 + 本笔里程碑笔（文档笔在两者之间）。

### 第一百五十七笔（并发任务 A 板 · A2 切片 31 · seq 的 argv 全程按字节，参数缝折叠第一次被证实会改输出内容与报文行数）

**里程碑**。板 A 行 A2（GETOPT-REPORT-BYTE-LETTER）参数缝半第五对 `seq` 落地。这是本族
第一处「折叠不在报文面上而在数据面上」的缺陷被闭：`seq` 的 `-s`/`-t` 值直接成为打出去的
字节，`-f` 与操作数直接成为报文里的字节，而它又是唯一按 `errflg` **累加**的选项环，所以
同一枚裸 `ff` 在文本缝上会同时坏**字母位、数据位、行数**三层（参照件 `tmp/seq-320/seq_ref`
对 `seq -s<ff> 1 3` 打 `1\xff2\xff3\xff`，折叠形打 `1\xef\xbf\xbd2…`；对 `seq -<ff> 3` 打一行，
折叠形打三行）。A/B 十二形由 SAME=6/DIFF=6 变 **SAME=11/DIFF=1**（唯一残差 `tr` 的字母位，
下一对）。

**提交与账**。码笔 25a8312c9（前门 `parse_seq_args`/`parse` 取 `&[&[u8]]`，
`separator`/`terminator`/`fmt`/`operands` 全字节，`e_atof` 只在数字文法那一步折一次并交回
原字节，壳侧新增 `sentence` 拼「程序名+句子+原字节+换行」）；文档笔 58e57afb3（18 篇立批次
四百四十三段、07 篇三处口径：库测试 604、`seq.rs` 决定半 24 枚、`seq_face.rs` 十八枚、
`ARGV-LOSSY-VALUE-PLANE` 闭）；文档笔补 aa3231459（文风门 SL-5 咬住两处「原先」的修复史
叙事，改写为直接表述，`doc-style-lint --diff rewrite` 的 error级由 2 归 0）。

**门证据（L1，命令+读数）**。宿主 `cargo test -p minix-textfilter --no-fail-fast` →
**69 段 / 1222 通过 / 0 失败**；容器权威门 `docker run --rm -v $PWD:/work -w /work/os -m 2g
minix-ci:1.94 cargo test -j 1 -p minix-textfilter` → 同一读数、编译错 0、`rc=0`
（`tmp/argv-443/container.log`）；三架构 `cargo build --target {x86_64,aarch64,riscv64gc}…`
各退 0、错误 0；触及件 `rustfmt --check` 零漂移；`cargo clippy` 触及件零告警；
负控四枚 **N1 咬 4 / N2 咬 3 / N3 咬 1 / N4 等价 0 咬**；五道收尾门：门1 仅存已知
「探针命名族 31-vs-18」漂移，门2 断链 131（=基线），门3 PASS，门4 error级=0，门5 无在制。
门的新增与翻正：库 +2、面 +2、跨壳表 `seq` 行由 `ef` 翻正为 `ff`，原钉「三行」的门改名为
`the_accumulating_ring_names_the_raw_byte_once` 并断言一行一个原字节。

**新判据（三条 + 一条手法）**。① 同一处折叠在一支里可能坏三层（字母位/数据位/行数），
体量必须按现量分类后再定，不能按「字母位差一字节」统一预估；② 钉住错误形状的门不能留作
历史证据，要改名承接正确形状并在注释里写明旧形状是缝的产物；③ 数字文法与字节回显可以分居
两侧——「非 ASCII 字节必然不是数字的一部分」这条判据成立时，折在 `parse_float_prefix` 那一步
不影响判定，只影响表示，而错对象必须携带原字节；④ 手法：`rustfmt` 会经
`#[path = "../bin_support.rs"]` 连带重排被包含的模块件，触及件之外的格式差必须逐件还原，
否则「本次触及件」的定义被工具行为悄悄放大。

**本轮自己踩的坑（记法不记史）**：注入类负控脚本若写成
`open(path,"w").write(open(path).read().replace(new,old))`，外层 `open(...,"w")` 会先清空文件，
内层读到空串——两件源码当场归零。判据：**先读进内存再动文件**；且代码改动必须有可重放的
补丁链（本轮 `patch2.py` → `fix_tests.py` → `libgates.py` 三步行得通，现场才能复原）。

**A 板余量**。A2 参数缝半：`textfilter` 三十九支壳已迁五支（`rev`、`paste`、`yes`、`tee`、
`seq`）、余三十四支；跨壳表余 `tr`/`fold`/`units` 三行待翻正；`text-games` 三支未动；
族末笔要删 `bin_support::args`/`prog_name`/`empty_optstring_operands` 的 `&str` 共件并把
`os/commands/games/text-games/tests/argv_seam_face.rs:fn a_raw_nonutf8_argument_behaves_like_its_lossy_equivalent`
升级为逐字节。在册未执行：`ARGV-BYTE-PATH-IN-LIBS`（本支新增 `strerror` 那条腿）、
`UNEXPAND-*`、`CUT-*`、`NL-*`、`PASTE-COMM-NUL-*`、`PR-HEADER-TIME-FIELD-DEFAULT-EMPTY`、
八条 `*-NON-PERMUTE-OPERAND-TAIL`、`SEQ-FORMAT-RENDER`、普查乙/丙两组。下一对建议 `tr`
（唯一 A/B 残差所在，且它的两个集合已是字节形）。


### 第一百五十八笔（并发任务 A 板 · A2 切片 32 · `tr` 的 argv 全程按字节，参数缝折叠在本族第一次做到与参照件零残差）

**里程碑**。板 A 行 A2（GETOPT-REPORT-BYTE-LETTER）参数缝半第六对 `tr` 落地。`tr` 把「折叠坏在哪
一层」这件事摊全了：`seq` 那一对证明折叠会同时改报文与输出内容，`tr` 这一对里三张脸一起坏——

① **数据位**：`-d`/`-c`/`-s` 三个操作都以集合成员判据为准，成员判据拿的是式子里的字节。折叠把
`a\xffb` 变成 `a<ef bf bd>b`，于是 `tr -d 'a\xffb'` 删不掉输入里那枚 `ff`、`tr -c 'a\xff' y` 的
补集算错、`tr -s '\xff\xff'` 挤不动；`tr a '\xff'` 更直白——映射值本身就是写往 stdout 的字节。
② **回显位**：`minix3/usr.bin/tr/str.c:253` 用 `%.*s` 带长度把未知类名原样打回（`'[:\xfffoo:]'`
→ `tr: unknown class \xfffoo`），句子只要借文本口过一遍，回显就变成替换字符。
③ **字母位**：环 `tr.c:68-82` 不重排、三字母都不带值，`getopt.c:79` 一次进一个字节，所以集外
字母那一行的字母位必须是一枚裸 `ff`。

改法与 `seq` 同一口径：`parse_tr_args` 收 `&[&'a [u8]]`（片段生命周期与外层借用解绑，测试适配器
才能交本地 `Vec`），`TrArgs.operands` 存原字节片，`parse_set` 与壳侧 `expand` 的集合式入参收
`&[u8]`，命名类按字节字面量匹配，未知类名载荷是 `Vec<u8>` 且句子出口是字节口
`SetGrammar::sentence_bytes()`，壳侧新增 `grammar_line` 按字节拼「程序名 + 冒号 + 句子」那一行，
libc 那一行继续走 `GetoptReport::line_bytes`（`os/commands/usr-bin/textfilter/src/lib.rs:fn line_bytes`）。

**读数（L1）**：`tr` 的 A/B 八形逐字节对参照件 `tmp/tr-322/tr_ref` = **SAME=8 / DIFF=0**——本族
第一支做到零残差（`seq` 那轮是 SAME=11/DIFF=1，唯一残差正是 `tr` 的字母位，本轮把它收了）。
门 库 +3（`a_raw_ff_set_member_is_one_byte_of_the_set`、`an_unknown_class_name_is_echoed_as_its_own_bytes`、
`parse_tr_names_a_raw_ff_option_letter_by_that_byte`）、面 +2（`tests/tr_face.rs` 十九枚→二十一枚，
两枚以 `OsString::from_vec` 交原始字节）、跨壳表 `tr` 行的字母位读数是一枚裸 `ff`。宿主单包现量
`minix-textfilter` 69 段 / 1227 / 0、`minix-text-games` 9 段 / 79 / 0（两包合跑 78 段 / 1306 / 0）；
容器 `minix-ci:1.94`（`-m 2g`、`-j 1`、挂领用树根、工作目录 `os/`）同读数 78 段 / 1306 / 0；三架构
`--bin tr` 裸编各退 0；clippy 触及件零告警；rustfmt 四件零漂移；负控四枚三咬 + 一枚等价全绿
（`tmp/argv-444/negctl444.log`）。五道收尾门全过：门 1 仅存已知探针命名族漂移（31 对 18）、门 2
断链 131 等于基线、门 3 PASS、门 4 `--diff rewrite` error级=0、门 5 无在制文件。

**取证手法三条入档**：负控脚本判「编译失败」不能用裸 `^error`（cargo 在测试失败段也打
`error: test failed`，会把真咬住误标成 BUILD-ERR）；容器门的挂载根必须写领用树的绝对路径（靠脚本
自身位置 `cd ../..` 的写法在归档副本里会指到主树）；分账读数别拿「两包合跑再按 `Running` 行的包名
切」来算——包名只出现在 lib 段路径里，集成测那几段切不出来（本轮首次落笔因此把 textfilter 记成
76 段，已由文档笔补订正为单包现量）。

**提交与账**（领用树内 rebase 到新基线 `1502f2e9e` 后的哈希）：`d936e7ead`（码）、`4dc666f88`（文档）、`b6362e3d8`（文档笔补 · 读数订正）。rebase 前的三笔哈希不再有效。
板 A2 行余量：`textfilter` 三十九支已迁六支（`rev`、`paste`、`yes`、`tee`、`seq`、`tr`）、余三十三支；
跨壳表余 `fold`/`units` 两行；共件 `empty_optstring_operands` 的 `&str` 形只剩 `colrm`；`tr`/`seq`
的 `prog` 那处程序名 lossy 属全族共件，留给参数缝半收尾那一笔。登记未执行项维持：
`ARGV-BYTE-PATH-IN-LIBS`（本支未新增腿）、`UNEXPAND-*`、`CUT-*`、`NL-*`、`PASTE-COMM-NUL-*`、
`SEQ-FLOAT-RENDER`、`TR-SET-GRAMMAR-SENTENCES`、八条 `*-NON-PERMUTE-OPERAND-TAIL` 与
`PR-HEADER-TIME-FIELD-DEFAULT-EMPTY`。

### 第一百五十九笔（并发任务 A 板 · A2 切片 33 · `colrm` 的 argv 全程按字节，共件 `empty_optstring_operands` 的 `&str` 形自此无人使用）

**里程碑**。板 A 行 A2（GETOPT-REPORT-BYTE-LETTER）参数缝半第七对 `colrm` 落地。这一支的缺陷面比
`tr` 窄、比 `seq` 干净：坏在两张嘴——

① **回显位**：`minix3/usr.bin/colrm/colrm.c:80-86` 的 `errx(1, "illegal column -- %s", argv[..])`
   打的是**操作数原字节**。参照件 `tmp/colrm-310/colrm_ref` 现量 `colrm 1<ff>3` →
   `colrm: illegal column -- 1<ff>3`；参数缝先折一次的读数是 `1<ef bf bd>3`，句子结构完全正确，
   坏的只是那三枚替换字符。
② **字母位**：optstring 是空串（`colrm.c:67`），`-<ff>` 那一行由 libc 印（`getopt.c:79`、`:103`），
   字母位是短横后的一枚裸字节；折叠形取到的是 `ef`（lossy 后替换字符的第一个字节）。

改法与前两对同口径：`parse_columns` 收 `&[&'a [u8]]`、`ColrmOperand::IllegalColumn` 的载荷是
`&'a [u8]`，`column_value` 直接吃字节（它的空白跳、符号、十进制扫描本来就按字节做，迁移只是把
`as_bytes()` 那一步从函数里挪到了调用方看不见的地方），壳用共件的
`empty_optstring_operands_bytes`，非法列号那一行走新的字节出口 `illegal_column`。

**结构性收获**：`colrm` 是 `empty_optstring_operands` 的 `&str` 形的最后一个使用者，本支之后那一形
**无人使用**——参数缝族收尾那一笔可以直接删除它，连同 `bin_support::args()` 与 `prog_name`。

**读数（L1）**：A/B 八形逐字节对参照件 = SAME=7 / DIFF=1（`tmp/argv-445/ab.log`），唯一残差是
`colrm 4 2` 那句 `err(1, ...)` 尾巴上的 `strerror` 表内容（本壳 `Undefined error: 0` 对参照件
`Success`）——`errno` 为 0 时 strerror 表随 libc 而不同，属已登记的 `COLRM-STRERROR-ZERO-TEXT`，
本支不动。门 库 +2、面 +2（`tests/colrm_face.rs` 十七枚→十九枚，两枚以 `OsString::from_vec`
交原始字节）；宿主单包 `cargo test -p minix-textfilter --no-fail-fast` = 69 段 / 1231 / 0，容器
`minix-ci:1.94`（`-m 2g`、`-j 1`、挂领用树绝对路径、工作目录 `os/`）两包合跑 = 78 段 / 1310 / 0；
三架构 `--bin colrm` 裸编各退 0；clippy 触及件零告警、rustfmt 三件零漂移；负控四枚三咬 + 一枚
等价全绿、还原后 632 / 0。五道收尾门全过（门 1 仅存已知探针命名族漂移 31 对 18、门 2 断链 131
等于基线、门 3 PASS、门 4 `--diff rewrite` error级 0、门 5 无在制文件）。

**手法两条入档**：`empty_optstring_operands_bytes` 交回 `Vec<&[u8]>` 而不是切片，壳里要传 `&operands`；
原始字节参的夹具用 `OsString::from_vec` 必须先 `use std::os::unix::ffi::OsStringExt;`——这个构造子
挂在 trait 上、不在预导出集里，漏掉就是 `E0599 no associated function`。另订正一处陈旧读数：
07 篇库测试口径那行的总数早已不成立，且带着正文日期（SL-4 咬住），现改为「按模块拆分只是代表性
节选、不求加总等于总数」。

**提交与账**：`8dd45cebe`（码）、`f922164bb`（文档）、`031cd5920`（文档笔补 · 文风门与陈旧读数）。
板 A2 行余量：`textfilter` 三十九支已迁七支、余三十二支；跨壳表仍余 `fold`/`units` 两行的字母位是
折叠形 `ef`；`tests/bin_support_face.rs` 三处注释把 `tr`/`seq` 仍列为未换字节形，登记为待订正；
`tr`/`seq`/`colrm` 的 `prog` 那处程序名 lossy 属全族共件，留给收尾那一笔。

### 第一百六十笔（并发任务 A 板 · A2 切片 34 · `fold` 的 argv 全程按字节，A/B 顺手量出拆行腿两臂写反）

**落点**：`os/commands/usr-bin/textfilter/src/fold.rs`、`src/bin/fold.rs`、`tests/fold_face.rs`、
`tests/bin_support_face.rs`（另把批次四百四十五挂错位置的适配器注释在 `src/colrm.rs` 里归位）。
领用 `GETOPT-REPORT-BYTE-LETTER`，基线是批次四百四十五合入后的 `rewrite` 头。

**这一支只坏字母位那一张脸，但那张脸有两条腿**：跨壳表里 `-<ff>` 那一形走单字母腿，
而 `b` 是选项串 `"0123456789bsw:"`（`minix3/usr.bin/fold/fold.c:76`）里合法且不带值的字母，
环吃完 `b` 继续吃下一位，所以 `-b<ff>` 的字母来自聚簇扫描——`tests/fold_face.rs` 因此新立一枚专钉聚簇腿的门，
与跨壳表那行不重复承载。批次四百零五那次只换了渲染半，合法序列 `c3 a9`（`é`）一直是对的，
**不属于任何合法序列的字节**才把这一位露出来：旧的 `bin_support::args` 先过一遍 `from_utf8_lossy`，
一枚 `ff` 在到达选项环之前就成了三字节的替换字符，环取到的第一字节是 `ef`，参照件交回的是 `ff`。
数据位这一支无处可坏：操作数只有宽度值与文件路径两个去处，宽度那句 `illegal width value` 不带操作数，
路径腿要在 `os/libs/minix-sys/src/lib.rs:fn open` 与 `os/libs/minix-types/src/types/errno.rs:fn err_line`
两个 `&str` 口上折一次，属已登记的 `ARGV-BYTE-PATH-IN-LIBS`，本支不新增腿。

**门**：库 +2（`parse_fold_args_keeps_a_raw_ff_operand_as_its_own_bytes`、
`parse_fold_args_names_a_raw_ff_option_letter_as_one_byte`）、面 +1（`fold_face` 七枚→八枚，新枚名与
`colrm_face`/`rev_face` 的同族门刻意不同名，否则负控红名清单分不出是谁）、跨壳表 `fold` 行由 `ef` 翻正为 `ff`。
跨壳表的表头注释按现量重写：六支里五支已走 `bin_support::args_bytes`，只余 `units` 一支仍走文本缝；
批次四百四十五登记的那三处过时注释（把 `tr`/`seq` 列为未换字节形）随本笔订正完毕。

**A/B 十三形**（`python3 tmp/argv-446/ab.py > tmp/argv-446/ab.log`，参照件 `tmp/fold-405/fold_ref`
开工先跑 `-Z` 验字样：那句是 `fold_ref: unknown option -- Z`，Minix 措辞，承真值）：
**SAME=11 / DIFF=2**，两形都在册——① `fold a<ff>.txt` 的报文回显差是路径腿的 lossy 折叠；
② `-w 4` 遇空格行的拆行形状差是**本轮新量出的真码缺陷**。

**新登记不执行 `FOLD-SPLIT-BRANCH-SWAPPED`（P0-code-bug + 文档失真一处）**：`minix3/usr.bin/fold/fold.c:152-176`
的分支条件是 `if (split_words && last_space != -1)`——**只有开了 `-s` 才回找缓冲里最后一个空格**，
不开 `-s` 时列位一过宽度就把整块缓冲吐出、空格留在它落到的那一行。
`os/commands/usr-bin/textfilter/src/fold.rs:fn fold_line` 把两臂写反了：`split_words` 为真时整块吐出，
为假时去 `rposition` 找最后一个空格，于是空格位于缓冲首位那一形吐出**空行**。现量两形：
`-w 4` 对 `abcdefghij klmnop` 参照件 `abcd`/`efgh`/`ij k`/`lmno`/`p`，本壳 `abcd`/`efgh`/`ij`/空/空/空/` klmnop`；
`-w 4` 对 `one two three four` 参照件 `one `/`two `/`thre`/`e fo`/`ur`，本壳 `one`/空/` two`/空/空/空/` three`/空。
同一条失真也写在 07 篇的 `fold` 行（「缺省最后空格断行且空格留在续行头」）与本件模块头注释里，
文档随本笔订正，代码留给专轮：改一臂要连库测三形的期望一起翻正，不属参数缝半这一轮。

**门读数**：宿主两包各跑 78 段 / 1313 通过 / 0 失败（`tmp/argv-446/host-final.log`，其中 `--lib` 611、
`fold_face` 8、`colrm_face` 19）、容器 78 段 / 1313 / 0（`tmp/argv-446/container.log`）、三架构退 0
（`tmp/argv-446/arch.log`）、clippy 触及件零告警、rustfmt 触及五件零漂移（`tmp/argv-446/rustfmt.log`）、
负控五枚四咬 + 一枚等价全绿（`tmp/argv-446/negctl446.log`，四枚红名分别落在新立的聚簇门、跨壳表门、
库测 `test_parse_fold_cluster_and_errors` 与翻正前的 `ef` 期望）、五道收尾门全过（门 1 仅存已知探针命名族
漂移 31 对 18、门 2 断链 131 等于基线、门 3 PASS、门 4 `--diff rewrite` error级 0、门 5 无在制文件）。

**手法一条入档**：`rustfmt -p <包>` 会连同**未触及**的件一起重排——本笔 `src/bin_support.rs` 里一处
`if/else` 被折成多行却与本轮无关，所以格式化只能逐件点名（`rustfmt --edition 2021 <文件…>`），
混进工作区的别件格式差要 `git checkout --` 单独还原，别把它算进本轮的触及件。

**提交与账**：`22ac29d4d`（码）、`04e6454ea`（文档）。板 A2 行余量：`textfilter` 三十九支已迁八支、
余三十一支；跨壳表只余 `units` 一行的字母位是折叠形 `ef`；`FOLD-SPLIT-BRANCH-SWAPPED` 与
`FOLD-DIGIT-NOT-FIRST-IN-CLUSTER`、`FOLD-NON-PERMUTE-OPERAND-TAIL`、`COLRM-STRERROR-ZERO-TEXT`、
`ARGV-BYTE-PATH-IN-LIBS` 各条维持已登记不执行；`fold` 支的 `prog` 那处程序名 lossy 属全族共件，
留给族收尾那一笔（同笔还要删 `bin_support::args` 与 `prog_name` 的文本形）。

### 第一百六十一笔（并发任务 A 板 · P0 专轮 · `fold` 拆行两臂按 C 翻正，`FOLD-SPLIT-BRANCH-SWAPPED` 闭）

**落点**：`os/commands/usr-bin/textfilter/src/fold.rs` 一枚文件。领用 `GETOPT-REPORT-BYTE-LETTER`，
基线是批次四百四十六合入后的 `rewrite` 头 `614c1260f`；轮内 `rewrite` 被别板推进两笔
（`f5de222d6` 内核测试与抽象收口的合并笔、`f81ba5e76` 01 篇三结构体答疑轮），在领用树内 rebase 后
按新基线复跑全部门，台账与本页哈希指向 rebase 后的 `a9bb4e814` / `82adb1049` / `bd6482d48`。

**这一枚 P0 是上一轮 A/B 顺手量出来的**（批次四百四十六登记，本轮执行）。真源一句话定形：
`minix3/usr.bin/fold/fold.c:152-176` 的条件是 `if (split_words && last_space != -1)`——
**只有 `-s` 打开才回找缓冲里最后一个空格**，且找到之后 `last_space++` 才 `wmemmove`，那个空格本身被丢掉；
不开 `-s` 时列位一过宽度就把整块缓冲吐出，空格留在它填到的那一行末尾。
`os/commands/usr-bin/textfilter/src/fold.rs:fn fold_line` 此前把两臂写反（`split_words` 为真时整块吐出、
为假时去 `rposition` 找空格），并且找到空格后按 `buffer[space..]` 带走续行，把本该丢掉的空格留在行首。
后果是用户可直接看到的：不开 `-s` 的普通文本被按空格拆开，空格恰在缓冲首位时还多吐空行。

**改法**：空格搜索收进 `split_words` 条件里；`Some(space)` 一支吐 `buffer[..space]` 后
`drain(..=space)`，续行列位由剩下的字符重算；`None` 一支整块吐出并把列位置零；
两支合流后再为当前字符推进一列（对位 C 那行 `col = new_column_position (col, ch)` 的
「calculate the column position for the next line」），随后才把字符存进缓冲。
模块头注释那句「breaking after the last space unless `-s`」是与参照件相反的失真，同笔按 C 改写。

**门**：库 +2——`without_split_words_the_break_is_a_hard_one_and_spaces_stay`、
`split_words_drops_the_space_and_carries_the_rest_as_the_next_line`；`test_break_after_the_last_space`
从「不开 `-s` 也能在空格后断」的错误期望改按 `-s` 形。`--lib` 611→613，`fold_face` 八枚不退。
**三形期望全部取自参照件 `tmp/fold-405/fold_ref` 的现量，不取自推演**——本轮第一次就按推演写了一枚
「中间该有一行空行」的期望，被参照件当场否掉（真形没有那一行），这条判据因此再补一条证据。

**A/B 二十五形**（`python3 tmp/argv-447/ab.py > tmp/argv-447/ab.log`，在原十三形上新增十二形拆行：
空格落在宽内、空格在行首、制表位、退格回拉、回车重置、连续空格、一行变多行、`-b` 逐字节，
各分 `-s` 与不开）逐字节对参照件：**SAME=24 / DIFF=1**。批次四百四十六那一形
（`ij` 之后三行空行再带前导空格）归零；唯一残差是 `fold a<ff>.txt` 的报文回显，属路径腿的
`from_utf8_lossy`（`ARGV-BYTE-PATH-IN-LIBS`，在册不动）。

**门读数（L1 证据）**：rebase 前宿主两包各跑 78 段 / 1315 通过 / 0 失败，rebase 后按新基线复跑同读数
（`tmp/argv-447/host.log`、`tmp/argv-447/host-rebase.log`）；容器 78 段 / 1315 / 0 退 0
（`tmp/argv-447/container.log`）；三架构 `cargo build -p minix-textfilter --bin fold --target
{x86_64-unknown-none, aarch64-unknown-none, riscv64gc-unknown-none-elf}` 两轮各退 0；
`rustfmt --edition 2021 --check` 触及件零漂移、clippy 触及件零告警；
负控四枚三咬 + 一枚等价全绿（`tmp/argv-447/negctl447.log`）：两臂再写反咬住三形、
续行少丢一位空格咬住两形、撤掉 `-s` 条件咬住不开 `-s` 那一形、闭包形等价改写不动色，还原后复跑全绿。

**门 4 的归因记一笔**：`doc-style-lint.sh --diff rewrite` 报了 error级 3，三处全在
`rewrite-notes/01-stage-kernel/todo.md`（别板新增的 `[SL-3] P9-1` 与两处 `[SL-4]` 正文日期），
`git diff --name-only rewrite...HEAD` 证明本轮只触及 `fold.rs` 与 18 篇、07 篇、板三个文档件，
本线触及件内零命中——按板的禁碰清单不动别板文件，只把归因写清。

**提交与账**：`a9bb4e814`（码）、`82adb1049`（文档笔）、`bd6482d48`（文档笔补：前一笔只落了 18 篇，
07 篇与板没跟上，本笔补齐并修掉批次四百四十六改写 07 篇 `fold` 行时留下的一个游离反引号）。
板 A 在册余量：`FOLD-SPLIT-BRANCH-SWAPPED` 闭合；余 `FOLD-DIGIT-NOT-FIRST-IN-CLUSTER`、
`FOLD-NON-PERMUTE-OPERAND-TAIL`、`ARGV-BYTE-PATH-IN-LIBS` 与各条登记不执行项；
参数缝半余 `textfilter` 三十一支，跨壳表只余 `units` 一行（其裁决要点见批次四百四十六段那条预研读数）。

### 第一百六十二笔（并发任务 A 板 · A2 参数缝半第九对 · `col` 的字母位与被它拒收的 `-l` 值都按字节交回）

**这一支换的是缝，量出来的却是一条契约事实**：`col` 是本族第一支 **A/B 零残差**的壳。
不是因为路径腿修好了——`ARGV-BYTE-PATH-IN-LIBS` 仍在册——而是因为 `col` 一个操作数都不收
（`minix3/usr.bin/col/col.c:157-158` 的 `if (optind != argc) usage()`），那条腿在这一支
根本没有可观察面。于是「残差有几形」从来不是缝的质量计，而是各支契约的形状计：一支壳能把
操作数用在哪儿（打开文件、当单位名、当列表），它的残差就长在哪儿。

**回显位第一次落在选项值上**：此前各支的 `%s` 回显的都是命令行**操作数**（`colrm` 的非法列、
`cut` 的非法列表），`col` 那句 `col.c:145-150` 的 `col: bad -l argument %s.` 打回的是
`optarg`——`-l` 那一段的**选项值**。同一串字节流经不同的型，改动面因此比既往任何一支都深一处：
`ColFlagError::BadLineCount` 的载荷从 `&str` 换成 `&[u8]`，`src/lib.rs:fn atoi` 也从 `&str` 改吃
`&[u8]`。后者不是顺手改：C 的 `atoi` 逐字节扫描（可选空白、可选符号、十进制前缀），把它留在
`&str` 上等于让「这个值能不能被解析」先过一遍字符解码——那是把决定半挂到缝的裁决之后。

**门**：库 +1 枚 `col::tests::a_raw_nonutf8_line_count_is_named_by_its_own_bytes`（`--lib` 613→614）、
面 +1 枚 `tests/col_face.rs::a_raw_nonutf8_letter_and_line_count_keep_their_own_bytes`
（十一枚→十二枚，枚名与 `colrm_face`、`fold_face` 的同族门不同，负控红名才分得清是谁），
跨壳表 `tests/bin_support_face.rs:fn no_command_aborts_on_a_nonutf8_option_letter` **补上 `col` 一行**
（六支→七支，该行期望 `ff` 有现量：本壳与参照件 `tmp/col-313/col` 对 `col -<ff>` 的 stderr 逐字节
`cmp` 相同）。

**A/B 二十三形**（`python3 tmp/argv-448/ab.py`，逐字节对 `tmp/col-313/col`；开工先跑 `-Z` 验字样，
那句是 `col: unknown option -- Z`，Minix 措辞，承真值）：**SAME=23 / DIFF=0**。

**门读数（L1 证据）**：宿主两包各跑 78 段 / 1317 通过 / 0 失败（`tmp/argv-448/final-gates.log`，
跨壳表补行后复跑 `tmp/argv-448/host-table.log` 同读数）；容器 78 段 / 1317 / 0 退 0
（`tmp/argv-448/container.log`）；三架构 `cargo build -p minix-textfilter --bin col --target
{x86_64-unknown-none, aarch64-unknown-none, riscv64gc-unknown-none-elf}` 各退 0；
`cargo fmt -p minix-textfilter -- --check` 对触及六件零漂移；负控五枚（四咬 + 一枚等价）见
`tmp/argv-448/negctl448.log` 与 `tmp/argv-448/negctl448-table.log`，还原后复跑全绿。

**两条手法入档**：① **注入脚本必须在异常路径也还原**。本轮 `suite()` 的仓库路径拼错抛异常，
N1 的注入留在工作区没被还原，源码带着一条假缝跑完了后面的门，靠 `git status` + `git diff` 现量
才逮到——脚本改成先备份、`try/finally` 还原，并加一枚「注入前基线」读数，基线不绿就别谈注入结果。
② **已迁的壳必须在跨壳表里有一行**。`col` 改完才发现表里没有它：「表里几支」与「迁了几支」两个
名单对不上账，属覆盖率漏项而非代码缺陷。

**clippy 的归因**：触及件 `src/col.rs` 报两枚告警（`fn filter` 里一处赋值后读不到、`fn flush_line`
里循环变量只用来索引），主树同位置同文案现量在案——**基线既有**，属存量 lint 债，本轮不扩范围去动
那台行装配状态机，新登记 `COL-CLIPPY-STOCK-LINT`（只登记未执行）。

**门 4 的一次真红**：`doc-style-lint.sh --diff rewrite` 首跑报 error级 1，命中的是板 A2 行一处括号里的
「取证」二字（SL-11 把过程标记列进射程）——事实没错，措辞越界，改笔复跑 error级 0。这是文风门咬住
本轮自己的例子，不是别板存量。

**提交与账**：`3ad9308cc`（码笔：缝与句子）、`61ca27450`（码笔二：跨壳表补行）、`ff8076c8b`（文档笔：
18 篇新段 + 07 篇三处口径 + 板 A2 行）、里程碑笔与门 4 措辞订正笔。板 A2 参数缝半已落九支
（`rev`、`paste`、`yes`、`tee`、`seq`、`tr`、`colrm`、`fold`、`col` = 切片 27—35），
余 `textfilter` 三十支 + `text-games` 三支壳 + 族终笔（删文本缝三口、把 `argv_seam_face` 那枚门
升级成逐字节相等）。下一支按板上顺序仍是 `units`：它要先裁决「表键换字节」还是「查询边界折一次
lossy」，不是既有的字母位/数据位/回显位三条腿能直接套的形。

### 第一百六十三笔（并发任务 A 板 · `units` 裁决轮 · 定「表键换字节」，拆切片 36 与 37，不落码）

`units` 是跨壳表里最后一行仍走文本缝的支，也是本族第一支**参数缝与决定半共用同一串字节**的壳。
不裁决就动码，会把「缝」这一层的改动扩成整张表的类型迁移，选错方向要整片回退——所以本轮只出裁决。

**三条 C 证据把路定死**。表项比对是 `strcmp`（`minix3/usr.bin/units/units.c:503`、`:511`、`:523`、
`:532`）、前缀是 `strncmp`（`:542`）：`strcmp` 的语义就是逐字节相等，字符解码从不参与。前缀那一支的
返回值是**用输入余部拼出来的串**（`strlcpy(buffer, prefixval)`、`strlcat(buffer, " ")`、
`strlcat(buffer, unit)`，`:546-549`），而这个串随后作为单位名被打印（`printf(" %s", *ptr)`，`:299`、
`:318`）——命令行递进来的字节能走到 stdout，这是本族此前没遇到过的形状：操作数不只是查询键，它还是
被回显的内容。`-f` 的选项值另被两句错误按 `%s` 回显（`:146`、`:179`），与批次四百四十八 `col` 的 `-l`
同型。

**乙路（缝按字节、在查询边界折一次 lossy 再查文本表）被同三条否证**：表内容本身也来自字节——
`-f` 指向的用户自定义表文件里可以有任意字节，把表读成 `String` 就已经折叠过一次，而两处折叠不在同一
位置发生（一处折 argv、一处折表行），连「同一串字节两边相等」这条最低要求都不保；② 的回显位仍要
字节才与参照件同形。甲路的代价如实记账：动的是决定半（表、查询、乘积名字、两条输出腿），
`os/commands/usr-bin/textfilter/src/units.rs` 里 `&str`/`String` 共 30 处，比既往任何一支都宽。

**拆两片**（写进板 A2 行当施工顺序）。切片 36 = 表与读表腿换字节键：`src/units.rs:fn read_units` 收
字节切片、`UnitsTable` 的键值改 `Vec<u8>`、`fn lookup_unit` 收 `&[u8]` 回字节串、`UnitType` 的分子
分母与普通输出、`listunits` 两条腿改成字节缓冲；**这一片不碰 argv**，既有 ASCII 夹具应逐字同形，
库内那 10 枚 `units` 决定半门随迁（期望不改，改的是类型）——一次可独立收敛的类型迁移。
切片 37 = argv 与 `-f` 回显：`fn parse_units_args` 收 `&[&'a [u8]]`、`UnitsArgs.userfile` 改
`Option<&'a [u8]>`，两句错误按字节拼，壳走 `bin_support::args_bytes`，跨壳表 `units` 行由 `ef`
翻正为 `ff`——翻正后跨壳表七支全是原字节，「表里几支」与「迁了几支」两个名单第一次对齐。

**顺带量清可观察面的边界**：非 ASCII 的单位名在 C 里同样查不到（`lookupunit` 返回空指针），
② 那条「输入余部被回显」的路只在**前缀 + 表内已知余部**时才走得到，而已知余部是表里的 ASCII 文本，
纯 `ff` 字节进不去。所以切片 37 的门只钉两处——集外字母位与 `-f` 的选项值回显——不钉单位名回显。
这条判断是裁决轮真正的产出：省下一轮去钉一门钉不住的形状。

**提交与账**：`602a942d8`（文档笔：18 篇新段 + 板 A2 行）、本页为里程碑笔；本轮零代码笔，
宿主与容器门未跑（无源码改动），只跑五道收尾门。板 A 余量：`textfilter` 三十支（含 `units`
那两片）、`text-games` 三支壳、族终笔（删 `bin_support::args`/`prog_name`/`empty_optstring_operands`
三个文本口，把 `os/commands/games/text-games/tests/argv_seam_face.rs:fn
a_raw_nonutf8_argument_behaves_like_its_lossy_equivalent` 升级成逐字节相等）。

### 第一百六十四笔（并发任务 A 板 · 名单对账轮 · 「末行换行状态」四条在册 P0 核销为闭）

台账比代码慢了一族：板上与 18 篇的余量段仍把 `NL-FINAL-LINE-ADDS-NEWLINE`、
`TAIL-FINAL-LINE-ADDS-NEWLINE` 写成在册待修，而裁决早已落地并且四支壳各有一枚面门钉着。
判据是「同类相比」——登记项要对门名与现量，不能拿旧读数当现状；这一轮做的就是这笔对账。

**裁决本身一句话说清**：行腿保留输入末行的换行状态——末行没有那枚换行，输出就不许补。
承载点在 `os/commands/usr-bin/textfilter/src/window.rs:fn lines_of` 与同模块的 `final_line_*`：
终止状态随行走完这条腿，所以「补不补」不由各支壳的写出循环各自决定。当初之所以裂成四条登记项，
正是因为四支壳各写各的无条件 `push(b'\n')`；把决定收进行腿，四条同轮归零，也就不用「三处各改一遍」。

**现量证据（L1）**：`python3` 逐字节比 本壳 × 参照件 × 三种选项 × 七形末行状态 = **98 形
SAME=98 / DIFF=0**（`tmp/finalnl-450/ab.log`）。七形是 `a\nb`（末行无换行）、`a\nb\n`、单行无换行、
空档、中间空行、纯空格无换行、三行带换行。三个参照件开工先跑 `-Z` 验归属，都交回 Minix 那句
`unknown option -- Z`（`tmp/nl-417/nl_ref`、`tmp/tail-416/tail_ref`、`tmp/head-415/head_ref`），
承真值——这条规矩又拦了一次，只是这次拦的是「别把不是 Minix 血脉的读数当裁决依据」。

**四枚钉子**：`tests/head_face.rs:fn an_unterminated_final_line_reaches_stdout_without_a_newline`、
`tests/tail_face.rs:fn tail_leaves_an_unterminated_final_line_unterminated`（同文件另有
`reversed_output_carries_the_unterminated_line_to_the_front` 钉 `-r`）、
`tests/uniq_face.rs:fn uniq_does_not_supply_a_newline_the_input_lacked`、
`tests/nl_face.rs:fn an_unterminated_final_line_is_numbered_without_a_newline`。
本笔**没有**再造一枚跨壳门：同一裁决的第五份重复断言不增加覆盖，只会让负控红名更难归因
（批次四百四十八那句「门名同类相比」的反面用法）。

**两处明确不扩**：`HEAD-NON-PERMUTE-OPERAND-TAIL` 是多操作数要不要置换的旧约定问题，与末行终止符
无关；`UNEXPAND-BLANK-LINE-END-FLUSH-NEEDS-TERMINATOR` 问的是只剩空白的末行要不要冲制表档，
比「补不补换行」深一层（批次四百二十六已把两条路各动什么写进板 A2 行）。两条都维持在册，
并把「末行」两字相同、层次不同的这两件事的区别写进 18 篇，免得下一个读台账的人并成一件。

**提交与账**：`eb77c0201`（文档笔：18 篇新段 + 板 A2 行核销）、本页为里程碑笔。本轮零源码改动，
宿主与容器门不重跑；五道收尾门全过（门 1 仅存已知探针族漂移、门 2 断链 131 与基线同、门 3 PASS、
门 4 error级 0、门 5 干净）。板 A 余量：参数缝半 `textfilter` 三十支（含 `units` 两片）+
`text-games` 三支壳 + 族终笔；在册 P0 只剩 `HEAD-NON-PERMUTE-OPERAND-TAIL` 一条——
交接 prompt 里列的四条 P0 至此核销完三条（另两条 `UNEXPAND-ZERO-STOP-PANIC`、
`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE` 分别随批次四百二十五、四百二十七闭）。

### 第一百六十五笔（并发任务 A 板 · 施工名单轮 · 余量从整数变成一张带顺序的名单）

「余三十支」作为一个整数对下一个接手的人没有用：他还得自己再数一遍才知道是谁、哪些支换了缝之后跨壳表
看得见。这一轮把三档名单一次点清（`grep` 现量，逐名进 `tmp/uniq-census-451/census.log`）：
`os/commands/usr-bin/textfilter/src/bin/` 共 39 枚 bin，已走 `args_bytes` 的 9 枚
（`col colrm fold paste rev seq tee tr yes`），仍走 `support::args()` 的 30 枚；按可观察面再分一档——
诊断层构造 `UnknownOption`（集外字母那一行由 libc 印）的 16 枚，换完缝字母位从 `ef` 回到 `ff`；
剩下 14 枚的 optstring 为空或根本没有选项环（`head`/`tail`/`wc`/`unvis`/`diff`/`patch`/`pr` 那一型），
换缝只影响路径与操作数腿。**「跨壳表能不能钉」不是缝的属性，是各支契约的属性**——这与批次四百四十八
那条「零残差因 `col` 不收操作数」是同一件事的两面。

**下一支点名为 `uniq`**，三条理由都来自现量：它是本族唯一一支有两个**选项值回显位**的壳（`-f` 与 `-s`
的坏值各由 C 自己印一句带 `%s` 的 `errx`，`minix3/usr.bin/uniq/uniq.c:90-98`；参照件
`tmp/uniq-408/uniq_ref` 本轮按 `-Z` 验过归属，交回 Minix 那句 `uniq_ref: unknown option -- Z`）；
它把 `obsolete()` 的旧式改写（`-2`→`-f2`、`+3`→`-s3`）也带在参数层里，那一腿非 ASCII 的尾巴不能被折；
而这三样都长在同一个文件 `os/commands/usr-bin/textfilter/src/bin/uniq.rs`
（`fn obsolete_args` 与 `fn parse_uniq_args`，连文件内 `mod tests` 的夹具也在同一件），
改一个文件即可收口——低风险、高面值。

**顺序定案已写进板 A2 行**：`units` 两片（裁决见批次四百四十九）→ `uniq` → 字母位可观察的其余各支按字母序 →
无字母位的一批放最后。族终笔的前置一并写明：三十支走完才删 `bin_support` 的三个文本口
（`args`、`prog_name`、`empty_optstring_operands`），才把 `os/commands/games/text-games/tests/argv_seam_face.rs:fn
a_raw_nonutf8_argument_behaves_like_its_lossy_equivalent` 从「raw 与 lossy 等价」升级成「逐字节等于原字节」。

**提交与账**：`3d8e082cc`（文档笔：18 篇新段 + 板 A2 行）、本页为里程碑笔。本轮零源码改动，
宿主与容器门不重跑；五道收尾门全过（门 1 仅存已知探针族漂移、门 2 断链 131 与基线同、门 3 PASS、
门 4 error级 0、门 5 干净）。板 A 余量：参数缝半三十支 + `text-games` 三支壳 + 族终笔 + 登记不执行清单。
### 第一百六十六笔（批次四百五十二 · A2 参数缝半切片 36：`uniq` 的 argv 交回原字节）

**这一支为什么在名单上排得靠前**：`uniq` 是本族**唯一一支有两个选项值回显位**的壳。`-f` 与 `-s` 的坏值各由
C 自己印一句带 `%s` 的 `errx`（`minix3/usr.bin/uniq/uniq.c:90-98`），而这两句里的值取的是 `optarg`——也就
是 argv 的原字节；同一支里还叠着 `obsolete()` 那段旧式改写（`-2`→`-f2`、`+3`→`-s3`），那一腿非 ASCII 的
尾巴不能被折掉。字母位、两个回显位、折叠尾巴共用同一串字节，所以它按「一对」一次做完：改动全在
`os/commands/usr-bin/textfilter/src/bin/uniq.rs` 一个文件里（连文件内 `mod tests` 的夹具也在同一件）。

**落地的形状**：argv 从 `support::args()`（两条腿都先把字节过一遍 `String::from_utf8_lossy`，非法字节在走到
选项环之前就已经没了）换成 `support::args_bytes()` 交回 `Vec<Vec<u8>>`；`obsolete_args`、`parse_uniq_args`、
`skip_count` 随之收字节形，`UniqArgError::IllegalField` 与 `IllegalChar` 的载荷从 `String` 改成 `Vec<u8>`；
坏值那句由新的 `fn illegal_value(prog, kind, value) -> Vec<u8>` 逐段拼。程序名与源/汇两条路径腿仍按既定口径
在臂内折一次 lossy——路径口径的字节化是另一条在册项（`ARGV-BYTE-PATH-IN-LIBS`），本轮不动它。

**门**：文件内 +1 枚 `a_raw_byte_is_kept_by_the_ring_and_by_the_obsolete_fold`（`--bin uniq` 12→13），
跨壳表 `tests/bin_support_face.rs` 补 `uniq` 一行（七行→八行；表仍是 4 枚门，行数不是门数）。库测试总数
不变：新枚长在二进制自己的测试模块里，不在 `--lib` 那一侧（现量 614）。

**读数（L1）**：宿主与容器权威门同读数 **78 段 / 1318 passed / 0 failed**；三架构 `--bin uniq` 各退 0；
触及件 clippy 严格名过滤为空。逐字节对真源 11 形 **SAME=11 / DIFF=0**（`tmp/uniq-452/ab.log`，参照件
`tmp/uniq-408/uniq_ref` 已按 `-Z` 验过归属）。负控六枚：五枚各咬住（N4 一次咬住六枚红名，连库侧
`tests::parse_obsolete_skip_forms` 一起红，证折叠那条腿被两侧同时钉着），一枚等价注入全绿；注入前基线与
复原复跑均全绿（`tmp/uniq-452/negctl452.log`）。

**两条新判据**（都来自本轮的现量，不是先验）。其一：**程序名那一腿跟着 argv[0]**，本壳叫 `uniq`、参照件那份
二进制叫 `uniq_ref`，逐字节比对前必须把名字 token **全部**替换成同一个占位。上一版脚本只替换 stderr 里第一次
出现的 `uniq:`，usage 行里那个名字漏了，于是三形报成假红——「按第一次出现归一」在有多处程序名的报文里必然漏。
其二：`rustfmt --check` 与 `clippy --tests` 都会**顺带**报告未触及件——`src/bin/uniq.rs` 经 `#[path]` 带出
`bin_support.rs` 的三处既有排版漂移，`--bin uniq --tests` 报的两枚 `collapsible_if` 位置在未触及的库件
`src/uniq.rs:239`、`:245`（与本轮改的二进制同名不同件）。归属办法是按**路径全名**过滤，并拿主树同一条命令
复现；两条 stock 项登记为 `BIN_SUPPORT-FMT-DRIFT-STOCK` 与 `UNIQ-CLIPPY-COLLAPSIBLE-IF-STOCK`，
本轮不顺手格式化，因为 `cargo fmt -p` 会把二十四个未触及件一起重排。

**顺序与编号按现量翻正**：`uniq` 先于 `units` 落地，切片号取 36；批次四百四十九裁决的 `units` 甲路两片
维持，编号顺延为 37（表与读表腿换字节键）与 38（argv 与 `-f` 回显）。参数缝半至此**已落十支**
（`rev`、`paste`、`yes`、`tee`、`seq`、`tr`、`colrm`、`fold`、`col`、`uniq`），余 `textfilter` 二十九支 +
`text-games` 三支壳 + 族终笔。

**提交与账**：`6d5cdcf77`（代码笔）、文档笔随本页同笔（18 篇新段 + 07 篇 `uniq` 行 + 板 A2 行三处）。
五道收尾门读数见 `tmp/uniq-452/closeout.log`。板 A 余量：参数缝半二十九支 + `text-games` 三支壳 +
族终笔 + 登记不执行清单（`HEAD-NON-PERMUTE-OPERAND-TAIL`、
`UNEXPAND-BLANK-LINE-END-FLUSH-NEEDS-TERMINATOR` 等各条维持）。
### 第一百六十七笔（批次四百五十三 · 体量核实轮：`units` 表键字节化的真实射程）

**为什么要先量这一刀**：批次四百四十九裁决 `units` 走甲路（表键换字节）并拆两片，第一片写着「表与读表腿
换字节键、不碰 argv、既有 ASCII 夹具应逐字同形」。按这句话直接动工，会在一半的时候发现收不住——所以先
把 `os/commands/usr-bin/textfilter/src/units.rs` 数一遍，再定切片边界。

**结论：那片拆不开，原因在对象的同一性而不是工作量**。`units` 里被比较的串只有一批，就是单位名，而它
同时扮演三个角色：表键（`UnitsTable.units` 与 `UnitsTable.prefixes`，`src/units.rs:26-27`）、表达式节点
（`UnitType.numerator` 与 `UnitType.denominator`，`:45-46`）、报错回显（`UnitsError::UnknownUnit(String)`，
`:34`；构造点 `:285`，渲染点 `:731`）。真源里这三处本来就是同一种东西——C 用 `strcmp`/`strncmp` 逐字节比
（`minix3/usr.bin/units/units.c:503`、`:511`、`:542`）。只换表键而表达式节点留 `String`，则每次比较边界都要
折一次 lossy，那就是批次四百四十九已经否证的乙路（否证理由：表文件内容本身也是字节，两处折叠不在同一位置）。

**现量（L1，`tmp/units-453/census.log`）**：`String`/`&str` 在该件出现 **30** 处；比较/切分/前后缀判定
**44** 处；`read_units`、`lookup_unit`、`add_unit`、`list_units`、`complete_reduce` 五个口的定义与调用点
跨 `src/` 与 `tests/` 共 **37** 处；库内 units 门 **20** 枚、面侧 `tests/units_face.rs` **23** 枚函数。
跨壳表里 `units` 那一行（`tests/bin_support_face.rs:87`）仍是折叠形 `ef`，是该表唯一未换的一行。

**改后的两片**：切片 37 = 单位名全链路字节化（`fn read_units` 收 `&[&[u8]]`、`UnitsTable` 与 `UnitType`
四处字段改 `Vec<Vec<u8>>`、`fn lookup_unit` 收 `&[u8]` 并交回字节定义、`fn add_unit` 收 `&[u8]`、
`UnknownUnit` 载荷与 `fn units_error_line` 那句逐字节、`fn list_units` 输出改字节串；ASCII 夹具期望逐字不变
即这片的验收面）。切片 38 = argv 与 `-f` 回显（`src/bin/units.rs:51-53` 换 `args_bytes`、
`parse_units_args` 收 `&[&[u8]]`、`userfile` 改字节，跨壳表那行 `ef`→`ff` 翻正）。

**提交与账**：文档笔为 `todo.md` 新段 + 板 A2 行体量核实句；本轮零源码改动，故不跑宿主与容器门，
五道收尾门读数见 `tmp/units-453/closeout.log`。板 A 余量：参数缝半二十九支（`units` 按改后两片切法）+
`text-games` 三支壳 + 族终笔 + 登记不执行清单。
### 第一百六十八笔（批次四百五十四 · 名单对账轮：`HEAD-NON-PERMUTE-OPERAND-TAIL` 核销为闭，族名数翻正为六个具名 ID）

**这一轮没有改一行码，做的是把两条台账读数拉回现状**，判据沿用「同类相比」：门名对门名、登记项对登记项，
不拿旧读数当现状。

**第一条订正**：`HEAD-NON-PERMUTE-OPERAND-TAIL` 早已落地。承载门是
`os/commands/usr-bin/textfilter/tests/head_face.rs:an_operand_ends_option_scanning_like_c`（现跑 11 passed /
0 failed），钉的正是参照件那一形——`head <档> -5` 打横幅加整份内容、再 `head: -5: No such file or directory`
退 1，`unknown option` 那一句根本不出现；同枚门里还钉了 `-q` 在此是个叫 `-q` 的档名（横幅照打、静默不生效），
以及选项写在前头照常生效、但环撞上操作数就再也回不来。真源判据 `minix3/lib/libc/stdlib/getopt.c:71-74`
的非选项即停，加 `minix3/usr.bin/head/head.c:118-119` 的 `argv += optind`。板上与批次四百五十那两处
「维持在册」因此是台账滞后，本笔翻正为闭。

**第二条订正**：这一族的「八枚」是个复现不了的数。按 ID 数两个台账的实际写法，具名项六个：
`HEAD`/`FOLD`/`CUT`/`CKSUM`/`NL`/`PR` 各配 `-NON-PERMUTE-OPERAND-TAIL`；余下一处通配写法
`*-NON-PERMUTE-OPERAND-TAIL` 被反复引用（18 篇十处、板一处），「八枚」是把通配与重复引用一并算进去得到的。
复现命令 `grep -o "[A-Za-z*]*-NON-PERMUTE-OPERAND-TAIL" ... | sort | uniq -c`（读数
`tmp/nonpermute-454/census.log`）。翻正后的现状：`head`、`tail` 各有一枚同名终结门（11/0、24/0），
故六枚里**两枚闭**；`fold`、`cut`、`cksum`、`nl`、`pr` 五支没有对应门，维持在册，逐支做时照 `head` 那枚的
形状——先用 `-Z` 验过归属的参照件量出「操作数在前」那一形，再落一枚面门，不另造跨壳门。

**读数**：本轮零源码改动，故不跑宿主与容器门；五道收尾门全过（门 1 仅存已知探针族漂移、门 2 断链 131 同基线、
门 3 PASS、门 4 error级 0、门 5 干净，`tmp/nonpermute-454/closeout.log`）。A2 参数缝半余量复核：
`grep -rl "support::args()" os/commands/usr-bin/textfilter/src/bin/` 现量 **29**，与批次四百五十二记的数相符。

**提交与账**：`6c950fbe2`（文档笔：18 篇新段 + 板 A2 行两处订正）、本页为里程碑笔。板 A 余量：A2 参数缝半
二十九支 + `text-games` 三支壳 + 族终笔 + 登记不执行清单（`UNEXPAND-BLANK-LINE-END-FLUSH-NEEDS-TERMINATOR`、
余五支终结门、`UNIQ-CLIPPY-COLLAPSIBLE-IF-STOCK` 等）。
### 第一百六十九笔（批次四百五十五 · A2 参数缝半切片 37：`units` 的单位名全链路按字节）

**这一轮按批次四百五十三定的切法做第一片**：单位名所在的每一条腿换成字节形，argv 留给切片 38。库里
`fn read_units` 收 `&[&[u8]]`，`UnitsTable` 的 `units`/`prefixes` 与 `UnitType` 的 `numerator`/`denominator`
四处字段、`UnitsError::UnknownUnit` 的载荷都是字节，`fn lookup_unit`/`fn add_unit` 收 `&[u8]`，
`fn units_error_line`/`fn show_unit`/`fn render_answer`/`fn list_units` 交回字节串；壳侧两条喂字节的腿同步
——词典 `fn split_file_lines` 按换行切字节（一枚非法字节不再能让整份词典消失），交互那一行不再折
`String::from_utf8_lossy`（名字里的 `é` 交回 `c3 a9` 两个字节，不再是折出来的 `c3 83 a9` 三字节）。
共件 `src/floatfmt.rs` 新增 `fn parse_float_prefix_bytes`，`&str` 那一个委托它，所以 `jot`/`diff`/`seq`
三支的调用形一字未动。**在册项 `UNITS-NONUTF8-DROP` 的两条腿据此核销为闭**。

**一条判据从论证升级成读数**：批次四百四十九否证「在查询边界折一次 lossy」时给的是理由，本轮负控 U3 给出
现量——词典里 `weird` 加裸 `ff` 与 `weird` 加字面替换字符 `ef bf bd` 是两枚不同的键，按字节查前者命中
「定义 `2 m`」那一枚，折一次 lossy 则两边同形、命中排在前面的错那一枚。逐字节比是唯一与真源同形的做法。

**门与读数**：库 +2 / 面 +2（面侧另加逐字节夹具 `fn run_bytes`，理由与 `col_face` 那笔同形——文本形分不清
裸 `ff` 与 `ef bf bd`）。宿主与容器权威门各 **69 段 / 1243 passed / 0 failed** 退 0（容器带
`-u $(id -u):$(id -g)` 与两个缓存挂载点，未与任何 cargo 并发）；三架构 `--target x86_64-unknown-none`、
`aarch64-unknown-none`、`riscv64gc-unknown-none-elf` 各退 0；clippy 整包 150 条、按路径全名判触及件
**零新增**（落 `units.rs:148`/`:197`/`floatfmt.rs:66` 那三条都在本轮改动之前的既有构造上，只是行号位移）；
`rustfmt --edition 2024` 触及四件零漂移；负控六枚 = 五枚各咬住一枚新门 + 一枚**声明等价**（U6 分母那串
由扫描保证全是 ASCII，零枚红是正确读数），基线执行门数 1243、注入期 1242、复跑 0 失败
（`tmp/units-455/negctl-final.log`）。A/B 对 `tmp/units-319/units`（先打 `-Z` 验归属：交回
`unknown option -- Z`，Minix 的 `getopt.c` 那一句）十八形 **SAME=14 / DIFF=4**，射程内六形裸字节腿全 SAME，
四形分别归 `UNITS-PARTIAL-UNIT-ON-ERROR`（三形）与 `UNITS-LEADING-DASH-NAME`（一形）。

**门 1 的漂移项本轮复核过，不在本线**：`tools/todo-reconcile-scan.sh` 报「探针命名族（行）读数 31 / 台账基线 18」，
而本轮 diff 里 `nk4a:`/`nk4c:` 出现 **0** 次（`git diff 1b4175e99..HEAD | grep -cE 'nk4[ac]:'` = 0），
`rewrite-notes/` 同一读数在基线快照与现状都是 345 行 ⇒ 这条漂移由别的线推进，处置归该线，本笔不改它的基线。
门 2 断链 **131** 同基线（本轮三件文档命中数前后都是 6，只有行号随新段位移）；门 3 PASS；门 4 增量零 error；
门 5 干净（`tmp/units-455/` 下 `ab.log`、`negctl-final.log`、`container.log`、`links-before.txt`、
`links-after.txt` 是本笔取证）。

**新登记一条待独立轮的 P0，并记下它的形状**：`UNITS-DASH-REWRITE-AT-FIRST-CHAR`——C 的短横改写循环
（`minix3/usr.bin/units/units.c:344-348`）游标从 `scratch + 1` 起，行首那枚短横不动；本壳
`fn add_into` 的条件含 `i == 0` 这一支，把首字符也改写了，于是 `units -f <档> - m` 的操作数 `-` 被折成空格、
名字段变空（参照件报 `Unknown unit '-'` 并出 `\t1 -`，本壳不打报文、出 `\t1`）。这正是既有登记
`UNITS-LEADING-DASH-NAME` 的根因：**登记语的措辞不等于根因**，那一行从参照件读数出发写的是「名字段少前导短横」，
而真正坏的是改写循环的起点。按「一次只修一条」不进本射程。两处夹具与期望的自错也一并入账：
把「一个元素就是一行」当成两个字段传入、把约分后的因子按推演写成展开值（`reduceproduct` 在定义含
`PRIMITIVECHAR` 那一格就 `break`，`units.c:583-585`），都改到读数而不是推演。

**提交与账**：码笔 `1646448c8`、文档笔 `0592b26f2`（18 篇新段 + 07 篇 units 行与篇首库测 614→616 +
板 A2 行）、本页为里程碑笔。A2 余量：按 argv 口径已迁十支 / 余二十九支（`units` 仍在那二十九支里，
跨壳表那一行仍钉 `ef`，切片 38 翻正）、`text-games` 三支壳、族终笔；独立轮在册 `UNITS-DASH-REWRITE-AT-FIRST-CHAR`、
`UNITS-PARTIAL-UNIT-ON-ERROR`、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`，`NL-DELIM-LENGTH-RULE` 待裁决。


### 第一百七十笔（批次四百五十六 · A2 参数缝半切片 38：`units` 的 argv 交回原字节）

**这一轮按批次四百五十三定的切法做第二片**：动参数缝，单位名链路上一片已经按字节。薄壳
`os/commands/usr-bin/textfilter/src/bin/units.rs:run` 改走 `support::args_bytes()` 与
`support::prog_name_bytes()`，库里 `UnitsArgs<'a>` 的 `userfile`/`operands` 改成 `&'a [u8]`、
`fn parse_units_args` 收 `&'a [&'a [u8]]`、`fn convert_operands`/`fn load_units_file` 的签名随之外改。
真源依据是 C 的 `crt0` 交给进程的就是字节数组，`minix3/usr.bin/units/units.c:855-875` 把 `argv`
原样交给 `addunit`、`-f` 的值原样交给 `open`，全程没有一步问编码。

**两条仍要 `&str` 的腿登记 `ARGV-BYTE-PATH-IN-LIBS` 并在臂内各折一次 `from_utf8_lossy`**：`-f` 的档名
要交给 `minix_sys::open` 与 `minix_types::errno::err_line`，程序名要交给 `warn_line` 与
`fn units_error_line` 的名字段。为什么不就地换字节：那两个口在 `os/libs/`，按
`coordination/CONCURRENT-WORK-PROTOCOL.md` §2 是二线的领地，本板不许替别人实现他的前置。

**门数按现跑对账**：面侧 `units_face` 22 → **23**，新立
`an_operand_with_a_raw_byte_is_warned_about_in_raw_bytes`，配 argv 字节形夹具 `fn run_argv`
（参数经 `OsStr::from_bytes` 交下去——一枚裸 `ff` 写不成 `&str`，这是本族第二处「文本形夹具承载不了
本轮要比的字节」，判据同批次四百四十）；库内 units 门 **21 → 21**，本轮只把夹具写法从 `"…"` 改成
`b"…"` 并随之重排多行数组，期望串逐字未动，所以门数不涨（与批次四百五十五「既有门翻正不新增」同处置）；
`--lib` 段 616 → 616；整包 1243 → **1244**。跨壳表 `tests/bin_support_face.rs` 的 `units` 行
`ef` → `ff`，本表八行至此全是原字节。

**A/B 的读数本身就是这一轮的验收**：对 `tmp/units-319/units` 跑十一形 **SAME=9 / DIFF=2**
（`tmp/units-456/ab.log`）。参数缝落地前，「选项字母位吃裸 `ff`」与「操作数含 `weird<ff>`」两形是 DIFF
（本壳交回 `ef`、参照件交回 `ff`），本轮两形都 SAME；两处残留 DIFF 归既有在册项
`UNITS-PARTIAL-UNIT-ON-ERROR`，且 `Unknown unit 'weird\xff'` 那一行现在逐字节相同，差的只是
C 拿半成品照样吐出的那个答案块。

**负控五枚（`tmp/units-456/negctl.py`，读数 `negctl-final.log`）**：W1（argv 退回文本缝）咬住 2 枚
——新面门与跨壳表那枚跨支期望 `no_command_aborts_on_a_nonutf8_option_letter` 同时红，是共件单源的横向证据；
W2（have 侧操作数交给链路前折一次 lossy）咬住 1；W3（字母位退回按字符取整簇）红 3 枚、命中新门 1；
W4、W5 **零红＝等价声明**。本轮立一条判据：**声明等价的负控必须写清这一形为什么不允许存在**——
W4 成立的条件是门里的程序名全是 ASCII 且名字段本来收 `&str`，W5 是同一个 `0x20` 换了写法；
不写清的话「零红」读起来像门没牙。基线执行门数 1244、注入期 1241/1242/1243、复跑基线 1244 / 0，
每枚注入前后按整文件 md5 复核还原。两处注入端自错照旧归因到注入而不是门：W2 第一版把 lossy 折在
want 侧的 `"m"`（ASCII，折与不折同果）、W3 第一版锚在薄壳而字母位在库件的选项环里。

**一条编译期教训重演**：字节形的 `fn parse_units_args` 把生命周期绑在**元素**上而不是外层数组，
`parse_units_args(&[b"-f".as_slice(), b"db".as_slice()])` 这种临时数组写法报 E0716；改法是每处先
`let argv: &[&[u8]] = &[…]` 绑定再交出去——与批次四百四十一那条同一课。

**五道收尾门读数**（`tmp/units-456/gate1.log`、`gate2.log`、`gate345.log`）：门 1 只有一个漂移项，
「探针命名族（行）读数 31 / 台账基线 18」，本轮两笔提交的 diff 里 `nk4a:`/`nk4c:` 出现 **0**
（`git diff HEAD~2..HEAD | grep -cE 'nk4[ac]:'` = 0），判定不在本线、不改它的基线；门 2 断链 **131**
与上一笔同数（把行号剥掉做集合差，只差汇总那一行的空白），本轮新增断链 **0**；门 3 PASS（0 项）；
门 4 `--diff HEAD~2` 覆盖 3 个文件 / 73 个新增行，**error 级 0**；门 5 暂存区干净。
全包门宿主与容器各 **69 段 / 1244 passed / 0 failed** 退 0（容器按板 §7.3 三挂载形带
`-u $(id -u):$(id -g)` 与两个缓存挂载点，跑时未与任何 cargo 并发，`tmp/units-456/container.log`）；
三架构 `cargo check --target x86_64-unknown-none / aarch64-unknown-none / riscv64gc-unknown-none-elf
-p minix-textfilter --bins` 各退 0；`rustfmt --edition 2024 --check` 触及四件各退 0 零漂移；
**clippy 在改动前后各跑一次同一条命令 `cargo clippy --offline -j 1 --all-targets -p minix-textfilter`，
各 151 条、零新增**——把两份日志的 `warning:` 行去掉数字后排序做集合差，唯一两处文字差是 `colrm` 与
`sort` 两行的「(1 duplicate)」后缀位置，即同一枚警告在哪个 target 先报的差别，不是新增警告。

**由此立一条判据并订正上一笔的账面**：**clippy/rustfmt 的计数必须与命令同记**。批次四百五十五记
「整包 150 条」却没留命令，本轮实测同一条命令在该笔的落码点是 151 条，两个数不可比——「同数」若不附命令
就是一个无法核对的说法。本轮起按「改动前后各跑同一条命令」取证。

**进度与两处账面滞后一并翻正**：参数缝半按 argv 口径**已落十一支 / `textfilter` 余二十八支**
（`grep -rl "support::args()" os/commands/usr-bin/textfilter/src/bin/ | wc -l` = 28），按整个命令面口径
还有 **97** 支在文本缝上；渲染半此前那些「余 N 处调用点 / N 个源文件」是滞后数，现量
`grep -rn "\.line(prog\|\.line(&prog" os/commands --include=*.rs | grep -v tests/` = **0 行**，
即 `GetoptReport::line_bytes` 之外不再有按 `String` 渲染 libc 那一行的消费者（41 处调用点 / 29 个源文件
全在 `usr-bin/textfilter`，`text-games` 那三支走 `lottery.rs` 自己的字节口）。

**提交三笔**：码笔 `27eff20f3`（四件源码与门）、文档笔 `fce4ecfe6`（18 篇批次四百五十六段、07 篇 units 行、
板 A2 行）、本页为里程碑笔。A2 余量：`textfilter` 二十八支的参数缝 + `text-games` 三支壳 + 族终笔
（删 `bin_support` 三个文本口、把 `tests/argv_seam_face.rs` 那枚门升级成逐字节等于原字节）；
独立轮在册 `UNITS-DASH-REWRITE-AT-FIRST-CHAR`、`UNITS-PARTIAL-UNIT-ON-ERROR`、`UNITS-PATH-SEARCH`、
`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`、五枚 `*-NON-PERMUTE-OPERAND-TAIL`，
`REV-MULTIBYTE-ERROR-LEG` 与 `NL-DELIM-LENGTH-RULE` 待裁决。

### 第一百七十一笔（批次四百五十七 · A2 参数缝半切片 39：`cksum` 的 argv 与取值链路交回原字节）

**这一笔做的是施工名单（批次四百五十一板 A2 行第③项）的字母序第一支**。落码三层：薄壳
`os/commands/usr-bin/textfilter/src/bin/cksum.rs:run` 走 `bin_support::args_bytes`/`prog_name_bytes`；
参数层 `src/cksum.rs` 的 `CksumRequest::operands` 与 `CksumAction::String` 的载荷改成 `&'a [u8]`、
`fn parse_cksum_args_into` 收 `&[&'a [u8]]`、取值那一步交回 `&bytes[pos + 1..]`；
算法名层 `src/digest.rs:fn from_hashname_bytes` 按字节做 ASCII 大小写不敏感比对。真源依据是
`minix3/usr.bin/cksum/cksum.c:202` 的 `strcasecmp(hashes[i].hashname, optarg)`、`:259` 把 `optarg`
原样交给 `stringfunc`、`md5.c:60-67` 与 `print.c:54` 的 `%s` 逐字回显——C 全程不问编码。
另一条**不对称照搬下来**：`-a` 不敏感而 `-o` 在 `cksum.c:236`、`:239` 是 `strcmp(optarg, "1")`/`"2"`，
两套规则若共用一份匹配函数，`-o` 就会认下 C 不认的取值。

**A/B 六形 SAME=3 / DIFF=3**（`tmp/cksum-457/ab.log`，参照件经同名软链起，程序名与本体同源不需归一）：
SAME 是本轮射程——字母位 `-\xff`、多字节字母 `-\xc3\xa9` 只报第一枚 `c3`、`-a md5 -s ab<ff>cd` 那行
摘要逐字节相同；DIFF 三形同一根因且在册 `ARGV-BYTE-PATH-IN-LIBS`：档名进 `os/libs` 那个 `&str` 的
`open` 口就非折一次不可，按原字节名落的档在宿主上根本打不开，回显位所在的腿走不到。
**判据一条：DIFF 的归因必须写到腿**。同样三形写成「回显位没改成字节」就会把修法指向本层，
而真实的修法在 `minix_sys::open` 与 `minix_types::errno::err_line` 收 `&[u8]`——那是二线领地，本族不许替别人实现前置。

**参照件那侧的承载教训**：`tmp/cksum-457/cksum_ref` 此前每次运行都段错误（`gdb` 栈停在
`__strcmp_avx2 ← main`），根因不在 C 也不在门：构建脚本漏了
`-include tools/atf-c-compat/host-ref/bsd-compat-proto.h`，宿主 glibc 2.39 没有 `getprogname()`，
`cksum.c:175` 那句退化成隐式声明、返回的指针被截成 `int`，第一次 `strcmp` 就踩空。补上之后 `cc` 退 0、
`-Z` 交回 Minix `getopt.c:101-104` 那句 `unknown option -- Z`。**判据：参照件段错误先查承载再查 C**，
否则会把一次编译选项缺失记成一处行为差异。

**门账**：`cksum_face` 28 → **30**（两枚逐字节门 + `fn run_argv`/`fn scratch_bytes` 字节形夹具；
其中档名那一枚把临时档按**折叠名**落盘，为的是绕开开档腿那层在册的折损、只钉回显腿，注释写明
`ARGV-BYTE-PATH-IN-LIBS` 落地后要改回原字节名而断言不动）；跨壳表 `tests/bin_support_face.rs` 补
`cksum` 一行，**本表九行至此全是原字节**；整包 69 段 / 1244 → **1246** passed / 0 failed，
`--lib` 616 → 616（本轮把 `src/cksum.rs` 测试里 130 处字面量改写成字节形，期望串逐字未动，
所以门数不涨——与「找到既有门就翻正、不新增双胞胎」同一处置）。
**负控两枚各咬中一枚、零误伤**（`tmp/cksum-457/negctl.log`）：把 `fn emit_line` 的名字位改回折一次
lossy → 档名那一枚红、其余 29 绿；把 `fn string_line` 的串位折一次 → 串那一枚红、其余 29 绿；
两次注入之间整文件比对确认还原。

**五道收尾门读数**：① `tools/todo-reconcile-scan.sh` 退 1，**1 项漂移**＝「探针命名族（行）」读数 31
对基线 18，测的是内核里 `nk4[ac]:` 那一族日志名；本轮 `git diff HEAD~2 HEAD | grep -c 'nk4'` = **0**，
该条目属一线在制（`rewrite-notes/01-stage-kernel/todo.md` 正在别人的避让清单里），本族不代改台账也不改脚本基线。
② `python3 tools/notes-link-check.py rewrite-notes/` 断链数 131；与批次四百五十六那份
`tmp/units-456/links-after.txt` 按（文件, 链接文本）做集合差——94 个键两侧全等，**新增零**。
按 `文件:行号` 那口径会有 5 增 5 删的假差异，那是本篇新增 94 行造成的位移，判据：**断链集合差要按
（文件, 链接文本）为键**，行号会随内容漂移。③ `tools/todo-staleness-check.sh …TODO-LEDGER-OPEN.md` 退 0、PASS。
④ `tools/doc-style-lint.sh --diff` 首跑用默认 `RANGE=HEAD` 报「0 个文件 / 0 个新增行」——那是**空转读数**
（本笔文档已提交，`HEAD..HEAD` 自然为空）；改用显式 `--diff HEAD~2..HEAD` 得 3 个文件 / 106 个新增行，
抓到本行一处 `[SL-4] 正文日期`（我在 A2 状态列写了个日期），删掉日期后复跑 **error级=0**。
**判据：`--diff` 必须在提交之后带显式 range 复跑**，否则这道门会在刚提交过的改动上空转。
⑤ `tools/check-staged-inflight.sh` 退 0（避让清单 1 条，本轮未触碰）。

**门读数补全**：宿主与容器权威门各 69 段 / 1246 / 0 退 0（容器 `docker run --rm -u $(id -u):$(id -g)
-v $PWD:/work -v …/tmp/ci-cargo:/cargo -v …/tmp/ci-target:/tgt -e CARGO_HOME=/cargo
-e CARGO_TARGET_DIR=/tgt -w /work/os -m 2g minix-ci:1.94 cargo test -j 1 -p minix-textfilter`，
`tmp/cksum-457/container-gate.log`；跑前 `pgrep -a cargo` 无并发 cargo）；三架构
`cargo build --offline -j 1 --target {x86_64-unknown-none, aarch64-unknown-none,
riscv64gc-unknown-none-elf} -p minix-textfilter --bins` 各退 0 零 error 行。
clippy 同一条命令 `cargo clippy --offline -j 1 -p minix-textfilter --all-targets` 现跑 **151** 与上一笔同数，
且**归属本轮两件源件的 0 条**——中途确实自增过两枚 `manual_ignore_case_cmp`（本轮新写的逐字节大小写不敏感比对），
改用 `eq_ignore_ascii_case` 后归零，所以这个 151 是清掉自增之后的读数而不是巧合。
`cargo fmt --check -p minix-textfilter` 残留 21 枚文件有 diff，全部不在本轮改过的文件里——本包既有排版债，登记不执行。

**顺手修了一处会让整张表画坏的结构缺陷**：板 §1 的 `A2` 行有 5 枚**未转义**管道符（三枚是历轮追加留下的
` |。`，两枚在码段 `-o 1|2|3` 里），未转义分隔符 11 枚而表头是 7 枚——GFM 里码段中的裸管道符同样切格，
所以这一行一直在被切碎或整行退出表格。本轮补转义并补回收尾的 ` |`，复现命令
`python3 -c` 以负向后视 `(?<!\\)\|` 计数：表头 7、`A1` 7、`A2` 7、`A3` 7。
**判据：往多列表格里追加长文本，落笔后必须数一遍未转义管道符。** 同一行还有一段历轮切分留下的散行
（中间那行只剩一个分页符 `\f`，尾巴在「甲组余 `pr`、`column` 两支」处收口）——本轮只补结构符号，
没有重排文字次序，登记进独立轮名单。

**提交四笔**：码笔 `ce6065339`（三件源件 + 两件门）、文档笔 `4ef4ad6c9`（18 篇批次四百五十七段、
07 篇 cksum 行、板 A2 行）、本页为里程碑笔（含 A2 状态列那处正文日期的删除）。
**进度与余量（全部现量）**：参数缝半已落**十二支**（`rev`、`paste`、`yes`、`tee`、`seq`、`tr`、`colrm`、
`fold`、`col`、`uniq`、`units`、`cksum`）；`textfilter` 余 **27** 支
（`grep -rl "support::args()" os/commands/usr-bin/textfilter/src/bin/ | wc -l`），按整个命令面口径 **96** 支；
族终笔前置不变（删 `bin_support` 三个文本口 + 把 `text-games/tests/argv_seam_face.rs` 那枚门升级成
逐字节等于原字节）。下一支按字母序是 `cmp`。
独立轮在册：`ARGV-BYTE-PATH-IN-LIBS`、`CKSUM-CHECK-LEG-TEXTSEAM`、`UNITS-DASH-REWRITE-AT-FIRST-CHAR`、
`UNITS-PARTIAL-UNIT-ON-ERROR`、`UNITS-PATH-SEARCH`、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`、
五枚 `*-NON-PERMUTE-OPERAND-TAIL`、板 `A2` 行的分页符散行；`REV-MULTIBYTE-ERROR-LEG`、
`NL-DELIM-LENGTH-RULE` 待裁决。

---

### 第一百七十二笔（批次四百五十八 · A2 参数缝半切片 40：`cmp` 的操作数与两处报文名字位交回原字节）

**这一笔落的是字母序续支**：库侧 `os/commands/usr-bin/textfilter/src/cmp.rs` 的
`CmpFlags.operands` 改 `Vec<&'a [u8]>`、`fn parse_cmp_flags` 收 `&'a [&'a [u8]]`，两处报文构造点
`fn report_first_diff` 与 `fn report_eof` 从「交 `&str` 再 `as_bytes()`」改成交回 `Vec<u8>` 并按字节
`extend` 档名；壳侧 `src/bin/cmp.rs:run` 走 `bin_support::args_bytes`/`prog_name_bytes`，
`fn parse_skip` 按字节读 C 的 `strtoq(…, 0)` 那个形。真源依据是 `minix3/usr.bin/cmp/misc.c:81` 的
`printf("%s %s differ: char %lld, line %lld\n", file1, file2, byte, line)`、`misc.c:64`/`:67`/`:70`
的三条 `warnx("EOF on %s", …)`、`cmp.c:125`/`:130` 把 `argv[2]`/`argv[3]` 原样交给 `strtoq`——
C 全程不问编码。**`cmp` 的形状与 `cksum` 不同处在这一条**：档名同时是回显位与开档位，所以一枚
裸字节在此会同时错两处，而 `cksum` 的回显位与摘要计算是分开的两条腿。

**A/B 十四形 SAME=14 / DIFF=0**（`tmp/cmp-458/ab.log`；参照件 `tmp/cmp-410/cmp` 先按批次四百零七
那条验法打 `-Z`，交回 `cmp: unknown option -- Z` 加 usage、退 2 ⇒ Minix `getopt.c` 血脉）。
这是本族**头一支 A/B 全等**，判据要写准以免被读成「这条缝比上条干净」：全等的原因是 `cmp` 没有
第二条消耗参数的腿，而 `cksum` 那三形 DIFF 归在算法侧与 `-c` 校验腿两条独立轮上。**裸字节档名那一形
在盘上同时放原字节名与它的 `from_utf8_lossy` 折叠名**（内容相同），为的是把在册的
`ARGV-BYTE-PATH-IN-LIBS` 从读数里排除出去；这个排除动作写进了 `tmp/cmp-458/ab.py` 的模块注释——
**判据：A/B 为了量一条腿而屏蔽另一条腿时，屏蔽方式必须留在取证件里**，否则「全等」会被下一个读者
当成两腿都修完了。

**门账**：`cmp_face` 12 → **14**（differ 行两支档名一枚、`EOF on` 行档名一枚，配
`fn run_argv` 与 `fn write_raw_named` 两件字节形夹具）；库内 `cmp` 模块门 **10 → 10**（只把夹具
字面量改成字节形、期望串逐字未动，「既有门翻正就不新增双胞胎」第四次执行）；跨壳表
`tests/bin_support_face.rs` 补 `cmp` 一行（退 **2**，与 `cksum` 那行的 1 不同，取自 `cmp.c` 的
`ERR_EXIT`），**本表十行至此全是原字节**；整包 69 段 / 1246 → **1248** passed / 0 failed，`--lib` 616 不变。

**负控四枚的读数形状值得单独记**：W1 只把 `fn report_first_diff` 的第一支档名折一次 lossy → 红
**1** 枚（differ 那枚）；W2 只折 `fn report_eof` 的档名 → 红 **1** 枚（EOF 那枚）。
**两条报文腿各只咬自己那一条**，这正是「一形一枚、分开钉」想要的形状——若两处共用一个折叠点，
一次注入会同时红两枚，那就说不清门钉的是哪条腿。W3（argv 退回文本缝）咬 3 枚（两枚新面门 +
跨壳表），W4（字母位退回按字符取整簇）咬 3 枚（库门 + 面门 + 跨壳表）。负控只跑射程内三段
（库内 616 + 跨壳表 4 + 面侧 14），不是整包：注入只动 `cmp` 一支，跑全包只是等。

**本轮最该记的是自己造的一起混写事故**：做 clippy 前后比对时，我把改动四件按**文件名**收进
`tmp/cmp-458/mine/`，而 `src/cmp.rs` 与 `src/bin/cmp.rs` 同名，后者把前者盖掉；随后那个目录被当成
还原源回写，库件就被壳件覆盖了。混写之前的宿主整包是绿的，红的是紧接着的容器权威门：
`error: couldn't read commands/usr-bin/textfilter/src/../bin_support.rs: No such file or directory`，
指向 `src/cmp.rs:31` 那句 `mod support;`。三条判据入册：**①跨同名文件的备份与还原必须按完整相对路径
建目录树，不许用文件名拍平；②「临时把树换成别的版本再换回来」的循环一结束，第一件事是重跑一道会
编译全部 target 的门，不能直接进下一道门；③依赖所在目录的声明（`#[path = "../bin_support.rs"]`）
是免费的身份探针——同一个声明在库件与壳件里解出不同路径，文件被认错身份时它第一个炸**。
恢复源是 `tmp/cmp-458/cmp.rs.orig`，恢复后补一次 `rustfmt`，复跑整包 1248/0 与 A/B 十四形仍全等。

**另一处该记的是我手里的数而不是账**：交接摘要带着「上一笔整包 1244」开工，而本轮新增两枚门却读到
1248，减法差两枚。当场把上一笔那棵树复跑：`git checkout HEAD~1 -- <四件>` 后
`cargo test -p minix-textfilter` → **1246**，`git checkout HEAD -- <四件>` 还原 → **1248**。
台账本来就写 1244 → 1246，那个 1244 是**批次四百五十六**的读数，隔了一笔被我当成上一笔。
**判据：开工前把基线当场复跑，别拿转述来的数字当基线**——减法不平时的排查次序是「被减数 → 减数 → 台账」。

**五道收尾门读数**：① `tools/todo-reconcile-scan.sh` 仍有 **1 项漂移**＝「探针命名族（行）」读数 31 对
基线 18，测的是 `os/kernel/src`、`os/arch/src` 里 `nk4[ac]:` 那一族日志名；本轮
`git diff --name-only HEAD~2..HEAD` 七件全在 `os/commands` 与 `rewrite-notes`，与该判据无交集，
属一线在制，本族不代改台账也不改脚本基线（与第一百七十一笔同一项，未新增）。
② `python3 tools/notes-link-check.py rewrite-notes/` 全树断链 **131**，与上一笔那份同数；本轮另按
（文件, 链接目标）对**触碰的四篇**做集合差：基线键 0、现键 0 ⇒ 新增零。
③ `tools/todo-staleness-check.sh …TODO-LEDGER-OPEN.md` 退 0、PASS（0 项）。
④ `tools/doc-style-lint.sh --diff HEAD~2..HEAD` → 3 个文件 / 114 个新增行、**error级=0**
（按上一笔那条判据带显式 range 跑，不用默认 `HEAD` 空转）。
⑤ `tools/check-staged-inflight.sh` 退 0（避让清单 1 条：`rewrite-notes/coordination/TODO-3ARCH-PARITY-20261006.md`，
本轮未触碰）。

**门读数补全**：宿主与容器权威门各 69 段 / 1248 / 0 退 0（容器按板 §7.3 那条三挂载配方带
`-u $(id -u):$(id -g)`，跑时未与其他 cargo 并发，`tmp/cmp-458/container-gate.log`）；三架构
`cargo check --offline -j 1 --target {x86_64-unknown-none, aarch64-unknown-none,
riscv64gc-unknown-none-elf} -p minix-textfilter --bins` 各退 0；`rustfmt --edition 2024 --check`
触及四件首跑有 3 件报漂移（23/25/13 行），就地格式化后四件全退 0。clippy 同一条命令
`cargo clippy --offline -j 1 --all-targets -p minix-textfilter` 改动前后各 **151** 条零新增
（`tmp/cmp-458/clippy-before.log` 与 `tmp/cmp-458/clippy-final.log`；前后各跑一次时把 HEAD 那四件
临时放回原位再还原，正是那起混写事故的发生现场——所以这条流程从现在起必须自带「还原后逐件 md5
成对复核」，只看聚合哈希数会把两个不同文件误判成一致）。触及本轮两件源件的 2 条与 HEAD 同一对：
`src/cmp.rs:78` 的 `fn format_list_entry` 那句 `write!` 尾换行、`src/bin/cmp.rs` 的「test 模块之后
还有条目」（HEAD 报在第 261 行、本轮位移到第 298 行）——位移不是新增。

**进度与在册（全部现量）**：参数缝半已落**十三支**，`textfilter` 余 **26** 支、整个命令面余 **95** 支；
渲染半那条 grep 仍 **0** 行，`line_bytes` 调用点按可复跑命令是 **40 处 / 29 个源文件**（板 A2 行此前那句
「41 处」拿不到命令，本笔订正）。独立轮在册不碰：`ARGV-BYTE-PATH-IN-LIBS`（`cmp` 两条开档腿与程序名腿
都挂在它上面）、`CKSUM-CHECK-LEG-TEXTSEAM`、`UNITS-DASH-REWRITE-AT-FIRST-CHAR`、
`UNITS-PARTIAL-UNIT-ON-ERROR`、`UNITS-PATH-SEARCH`、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`、
五枚 `*-NON-PERMUTE-OPERAND-TAIL`；`REV-MULTIBYTE-ERROR-LEG`、`NL-DELIM-LENGTH-RULE` 待裁决。

### 第一百七十三笔（批次四百五十九 · A2 参数缝半切片 41：`column` 的分隔符值交回原字节——这条参数缝直接改到数据面）

**这一笔的形状与前面十四支不同处在「参数值走到哪里去」**：`asa`、`cksum`、`cmp` 那些支里，命令行
参数最终只被回显或拿去开档；`column` 的 `-s` 值既不回显也不开档，它被 C 一句 `separator = optarg;`
（`minix3/usr.bin/column/column.c:99`）原样接住，再整个交给 `strtok` 当分隔符表去切**要输出的内容**
（`column.c:225` 的 `(cols[coloff] = strtok(p, separator))`）。所以文本缝在此折一次 `from_utf8_lossy`
会同时错两件事：一枚裸 `ff` 变成 `ef bf bd` 三枚之后，真 `ff` 切不开，而输入里那三字节 U+FFFD
反倒被当成分隔符切开。缺省分隔符 `"\t "`（`column.c:74`）两枚本来就是 ASCII，所以缺省路一个字节
都不差，只有 `-s` 给了非 ASCII 值才显形。落码在库侧 `os/commands/usr-bin/textfilter/src/column.rs`：
`fn parse_column_args` 收 `&'a [&'a [u8]]`、`ColumnArgs.operands` 改 `Vec<&'a [u8]>`、
`ColumnOptions.separator` 从 `String` 改 `Vec<u8>`；`fn split_fields` 的比对口径本来就是按字节集
（`strspn` 一枚一字节比），本轮只把入参换成 `&[u8]`。壳侧 `src/bin/column.rs` 走
`bin_support::args_bytes`/`prog_name_bytes`，libc 那一行走字节形 `GetoptReport::line_bytes`；
程序名与开档两条腿仍要 `&str`（在册 `ARGV-BYTE-PATH-IN-LIBS`，壳内现量两处折叠）。

**两起自造的错，各自换出一条判据**（这是本笔最该被后续读走的东西）。第一起：给库内新门落夹具用的是
脚本化替换，插入段自带一个 `#[test]`，而门注释块前面我又手写了一个 `#[test]`——同一枚门被注册两遍；
更糟的是插入位置把既有门 `parse_operand_discrimination_without_reordering` 的 `#[test]` 顶掉了，那个
函数从此永不执行。一减一增正好相抵：整包 **1250**、`--lib` **617**，减法与「本轮新增两枚门」看着
完全对得上，`cargo clippy` 也一声不响——**丢掉 `#[test]` 的函数不报错，也不被当死代码报**，因为它在
`#[cfg(test)]` 模块里是个合法的普通函数。暴露它的是负控的原始日志：同一枚门的名字在里面连打两行
`FAILED`。判据三条：**①门数对账要比测试名集合，不能只比总数**——总数只在一增一减时失灵，而那正是
脚本化插入最容易造出的形状，可复跑的命令是对四件各跑一遍
`diff <(git show HEAD:<件> | grep -A2 "#\[test\]" | grep -oE "fn [a-z0-9_]+" | sort) <(grep -A2 "#\[test\]" <件> | grep -oE "fn [a-z0-9_]+" | sort)`，
本轮读数应是「库件 +1、面件 +1、两件壳与跨壳表零增减」；**②重复 `#[test]` 不违反编译规则**（批次
四百零九那条的第二次现量，那次虚增一枚、从门数就能看见，这次被另一枚的丢失抵消、门数看不见，所以
判据必须从「对总数」升级为「对名字」）；**③负控原始日志除了当红名单还是门清单的权威来源**（批次
四百一十七「门清单由执行结果列，不由印象列」的另一侧兑现：执行结果还会告诉你哪一枚出现了两次）。

第二起：A/B 第一遍报 17 SAME / 1 DIFF，红的那一形正是数据面「`-s<ff>` 切不开」，而门是绿的。根因是
`os/target/debug/column` 那份入口件是负控期间从注入态源码编出来的——负控只跑
`--lib --test column_face --test bin_support_face` 三段，cargo 只为测试重新链接 `deps/` 里那份，
入口件不保证被刷新，而门用的 `CARGO_BIN_EXE_column` 指向 `deps/` 那份新件，于是「门绿」与「入口件
是旧的」同时成立。`cargo build --offline -j 1 -p minix-textfilter --bins` 之后复跑，SAME=18 /
DIFF=0。判据两条：**④A/B 之前先显式重建被测 bin——门全绿不等于 `target/debug` 里那个件是新的**，
尤其在同一棵树刚跑过负控注入之后；**⑤中断或崩溃过的树，除重编之外还要 `file` 一下那个件**——本笔
第二次撞上的是文件报 `data` 而 cargo 报「Finished（新鲜）」，删掉 `target/debug/column` 与
`target/debug/deps/column-*` 再 build 才恢复。

**门账（全部现跑）**：库内 `column` 模块 **616 → 617**，新立
`the_separator_value_reaches_the_byte_set_one_byte_per_separator`，一枚门里钉三件事（分隔符集里就是
那一枚字节、真 `ff` 切得开、U+FFFD 三字节不被误切）；`column_face` **15 → 16**，新立
`an_option_value_of_column_splits_on_the_raw_byte_it_carries`，配一件字节形夹具 `fn run_argv`
（参数经 `OsStr::from_bytes` 交下去，否则一枚 `ff` 写不进 `&str`）；跨壳表 `bin_support_face`
门数 **4 → 4** 不变、表体从十行变成十一行（`column` 行钉 `column: unknown option -- \xff` 与退 **1**，
`column.c:107-109` 的 `default: usage();` 走 `exit(1)`，与 `cmp` 那行的 2 不同）；整包 69 段
**1248 → 1250 passed / 0 failed**。

**负控四枚与腿分离**（`tmp/column-459/negctl.py`，逐枚读数 `negctl-C1..C4.log`，汇总
`negctl-final.log`；基线库内 617、跨壳表 4、`column_face` 16 三段全绿）：C1（`-s` 取值处折一次
lossy）咬 **2** 枚（库内新门 + 面门）；C2（argv 退回 `support::args()`）咬 **2** 枚（面门 + 跨壳表
`no_command_aborts_on_a_nonutf8_option_letter`），**库内 617 全绿**——库件入参本来就是字节，缝在壳上，
所以牙只长在面侧与跨壳表，这条本族的通则第一次用数钉住；C3（字母位退回按字符取整簇）咬 **4** 枚
（库内既有 `a_non_ascii_cluster_letter_reports_a_byte_without_panicking`、跨壳表那行、面侧既有
`the_option_line_of_a_nonascii_letter_carries_one_raw_byte`、面侧新门第三形）；C4（分隔符集退回按
字符判）咬 **3** 枚（库内既有 `the_separator_is_a_set_of_bytes_not_a_set_of_characters`、库内新门、
面门）。C1 与 C4 是两条不同的腿（收值口径与比表口径），各自多咬一枚库内既有门，所以「按字节收值」
与「按字节比表」各自都有牙。另记一条流程自错：第一遍负控命令没带 `--no-fail-fast`，cargo 在第一个
红靶就停住，于是 C1 只拿到库段、C2 只拿到跨壳表段——**判据⑥：注入只跑射程内多段时必须带
`--no-fail-fast`，否则那份红名单只是「第一个红靶的名单」，而它看起来与完整名单一模一样**。每枚
注入前后按整文件 md5 与 `tmp/column-459/orig/` 下的**完整相对路径**备份成对复核（上一笔那条判据
照办），跑完核 `git status --porcelain` 只剩本轮真改动四件。

**A/B 十八形 SAME=18 / DIFF=0**（`tmp/column-459/ab2.log`；参照件 `tmp/column-314/column` 是指向
`column_ref` 的软链，让 `getprogname()` 算出的程序名与本壳一致，报文才能逐字节直比；先按批次
四百零七那条验法打 `-Z`，交回 `column: unknown option -- Z` 加 usage、退 1 ⇒ Minix `getopt.c` 血脉）。
十八形覆盖字母位四形（ASCII `Z`、裸 `ff`、多字节 `c3 a9`、聚簇 `-t\xff`）、缺值 `-c`、数据面两形
（`-s<ff>` 切得开、U+FFFD 不被切）、ASCII 分隔两形（含值附形 `-s,`）、宽度两形（`-c 40` 与
`-c 0x2a` 的 `atoi` 形）、无旗标读标准输入、`-x` 横向、两枚操作数、开不了的操数、裸 `ff` 档名那一形
（盘上同时放原字节名与其折叠孪生件，把在册的开档腿从读数里排除，排除方式写在 `ab.py` 的模块注释里）、
`--` 终结符、裸 `-` 当档名。**这一支没有任何已知 C 侧分叉**，但全等的边界只到参数缝与由它决定的
数据面，不含开档腿——这条边界上一笔已立，本轮照用。

**五道收尾门逐条读数**（`2026-10-21`，本笔前快照 `7e0b05c2d` 码笔、`582978505` 文档笔）：
① `tools/todo-reconcile-scan.sh` → 13 项判据里 **1 项 DRIFT**（`探针命名族（行）` 现量 31 vs 台账基线 18），
`git diff --name-only HEAD~3..HEAD` 显示本轮自己的改动全在 `os/commands/usr-bin/textfilter/` 与
`rewrite-notes/18-stage-commands/`、`rewrite-notes/coordination/`，与该项的取数域 `os/kernel/src`、
`os/arch/src` 无交集 ⇒ 非本轮引入，是一线在制的账，本笔不改脚本基线也不代改台账；
② `python3 tools/notes-link-check.py rewrite-notes/` → 扫 762 个 md、**断链 131**，与上一笔同数；
本轮两笔文档新增行里的 markdown 链接数为 **0**（`git diff HEAD~2..HEAD -- rewrite-notes/ | grep '^+' |
grep -F '](' | wc -l`）⇒ 新增断链键为 0，这条按（文件, 链接文本/目标）为键核；
③ `tools/todo-staleness-check.sh …TODO-LEDGER-OPEN.md` → **PASS（0 项）**；
④ `tools/doc-style-lint.sh --diff HEAD~2..HEAD` → 3 个文件 / 112 个新增行，**error 级 0**
（按上一笔那条判据带显式 range 跑，默认 `HEAD` 会空转）；
⑤ `tools/check-staged-inflight.sh` → 退 0，避让清单 1 条未触碰。

**门读数补全**：宿主整包 `cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` →
**69 段 / 1250 / 0**（`tmp/column-459/pkg-final4.log`）；容器权威门按板 §7.3 那条三挂载配方
（`-u $(id -u):$(id -g)` + `ci-cargo`/`ci-target` 两个缓存挂载点，跑时未与其他 cargo 并发）→ 退 0、
**69 段 / 1250 / 0**，与宿主同读数（`tmp/column-459/container-gate.log`）；三架构
`cargo check --offline -j 1 --target {x86_64-unknown-none, aarch64-unknown-none,
riscv64gc-unknown-none-elf} -p minix-textfilter --bins` 各退 0；
`rustfmt --edition 2024 --check` 触及四件全零漂移（库件首跑报 2 处本轮新写长行，就地格式化后归零，
`bin_support.rs` 未被牵连——`#[path = "../bin_support.rs"]` 那句只在壳件里才成立，格式化壳件时
顺带改库件那条只在**对壳件**跑 rustfmt 时才会发生）；`cargo clippy --offline -j 1 --all-targets
-p minix-textfilter` 在主树（即本笔基线）与本轮改动树各跑一次，各 **151** 条 `^warning`、`-->` 各 82 处、
**零新增**，且触及本轮四件的命中数两棵树同为 **0**（`tmp/column-459/clippy-main-tree.log` 与
`clippy-final.log`）。基线数不当转述用：本轮的 151 与 82 都是当场两条命令各跑一遍取到的。

**进度与在册（全部现量）**：参数缝半已落 **十四支**（`rev`、`paste`、`yes`、`tee`、`seq`、`tr`、
`colrm`、`fold`、`col`、`uniq`、`units`、`cksum`、`cmp`、`column`）；
`grep -rl "support::args()" os/commands/usr-bin/textfilter/src/bin/ | wc -l` → **25**；
整个命令面口径（同一条 grep 去掉 `tests/` 与 `bin_support.rs`）→ **94**；渲染半那条 grep 仍 **0** 行；
`line_bytes` 调用点 **40 处 / 29 个源文件**（本支未新增调用点，用的还是切片 4 那一处）。
余下 25 支里带字母位诊断面的有 13 支（`comm`、`cut`、`head`、`join`、`jot`、`nl`、`pr`、`sort`、
`split`、`tail`、`unexpand`、`unifdef`、`vis`，按各支 `*_face.rs` 里 `line_bytes`/`unknown option`
命中数点出），**切片 42 建议 `comm`**：字母序第一支，两支档名既是回显位又是开档位（与 `cmp` 同形），
参照件 `tmp/comm-409/comm_ref` 现成且已按 `-Z` 验过是 Minix 血脉。独立轮在册不碰：
`ARGV-BYTE-PATH-IN-LIBS`、`CKSUM-CHECK-LEG-TEXTSEAM`、`UNITS-DASH-REWRITE-AT-FIRST-CHAR`、
`UNITS-PARTIAL-UNIT-ON-ERROR`、`UNITS-PATH-SEARCH`、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`、
五枚 `*-NON-PERMUTE-OPERAND-TAIL`；`REV-MULTIBYTE-ERROR-LEG`、`NL-DELIM-LENGTH-RULE` 待裁决；
`todo-reconcile-scan` 的 `探针命名族` 漂移归一线。**新立一条流程待办**：领取树 `release` 之前必须先把
本轮取证的 `tmp/<批号>` 整目录复制回主树 `tmp/`，否则文档里引的取证路径会随树一起消失（批次四百五十七
的 `tmp/cksum-457` 已因此查无对应目录，本笔起按此办理）。

### 第一百七十四笔（批次四百六十 · A2 参数缝半切片 42：`comm` 的 argv 与两条报文名字位交回原字节）

**这一支的射程由 optstring 决定，而不是由「同族规律」决定**：`minix3/usr.bin/comm/comm.c:80` 的
`"123f"` 四个字母**一个都不带值**，所以 `comm` 既没有 `column` 那条「参数值就是输出内容」的腿，也
没有 `cmp` 那条「skip 偏移也是参数」的腿（`comm.c` 里 libc 的缺值句到不了，这一点在批次三百一十六
就登记过）。argv 在本支只被用在两处：集外字母那一行的字母位，以及两支档名；而档名只走去 `file()`
（`comm.c:177-187`）与 `err(1, "%s", name)` 的句子（同一支函数），两条都要 `&str`（在册
`ARGV-BYTE-PATH-IN-LIBS`）。于是本支缝的落点既不在数据面也不在回显写法，而在**库里能不能表达原
字节的操数**——旧签名 `parse_comm_args(&[&str])` 让那一形在库里根本写不出来。落码：库侧
`os/commands/usr-bin/textfilter/src/comm.rs` 的 `fn parse_comm_args` 改收 `&'a [&'a [u8]]`、交回
`[&'a [u8]; 2]`，环里 `arg == b"--"`、`arg.len() < 2 || arg[0] != b'-'`、`for &byte in &arg[1..]`
三处判据按字节（字母位的取法批次四百零九已按字节，本笔未动其语义）；壳侧 `src/bin/comm.rs` 走
`support::args_bytes()` 与 `support::prog_name_bytes()`，libc 那一行仍走 `GetoptReport::line_bytes`，
`fn operand_lines` 入参改 `&[u8]`、`-` 的判据按字节（`name == b"-"`），开档与 `err` 句子那一臂折一次
`from_utf8_lossy`（壳内现量 2 处）。

**门账（全部现跑，名字集合先对再对总数）**：库内 `comm` 模块 **13 → 14**，新立
`an_operand_that_is_not_utf8_is_returned_as_the_argv_bytes`；`comm_face` **11 → 12**，新立
`comm_reports_the_raw_option_byte_before_the_hardcoded_usage_line`（配两件字节形夹具 `fn run_argv`
与 `fn write_raw`）；`bin_support_face` 门数 **4 → 4** 不变、表体十一行 → **十二行**（`comm` 行钉
`comm: unknown option -- \xff` 与退 **1**，取自 `comm.c:195-199` 的 `usage()` 走 `exit(1)`，与
`cmp` 那行的 `ERR_EXIT` = 2 不同）；既有门 `the_unknown_letter_is_the_first_byte_not_the_first_character`
的期望**逐字未动**，只把夹具从 `"-\u{e9}"` 文本形翻正成 `b"-\xc3\xa9"` 字节形（「找到既有门就翻正、
不新增双胞胎」第四次执行）；整包 69 段 **1250 → 1252 passed / 0 failed**。上一笔立的那条
**「门数对账要比测试名集合」**本笔当开工即办：四件各跑一遍
`diff <(git show HEAD:<件> | grep -A2 "#\[test\]" | grep -oE "fn [a-z0-9_]+" | sort) <(grep -A2 "#\[test\]" <件> | grep -oE "fn [a-z0-9_]+" | sort)`，
读数是「库件 +1、面件 +1、壳件与跨壳表零增减」，另核 `#[test]` 属性数与属性后 `fn` 行数成对
（14 与 14、12 与 12），确认没有第二起「同名执行两遍」或「顶掉既有属性」。

**负控四枚换出两条判据**（`tmp/comm-460/negctl.py`，逐枚读数 `negctl-D1..D4.log`，汇总
`negctl-final.log`；基线库内 618、跨壳表 4、`comm_face` 12 三段全绿，命令带 `--no-fail-fast`）。
D1（壳侧 argv 退回 `support::args()`）咬 **2** 枚（面侧新门 + 跨壳表那一行），**库内 618 全绿**：
「库件的入参本来就是字节，缝在壳上，所以牙只长在面侧与跨壳表」这条通则至此第二次量化。
D2（字母位退回按字符取、只交每字符低字节）咬 **4** 枚（库内既有字母门、跨壳表、面侧既有聚簇门、
面侧新门），而本笔新立的**库门不红**——操数与字母位是两条腿，这条分工正是那枚库门存在的理由。
D3（`--` 终结符退回按前缀判 `arg.starts_with(b"--")`）只咬 **1** 枚（面侧
`an_unknown_letter_prints_the_libc_line_then_the_usage`，因为只有它带 `--x` 那一形）——记下来：
**一个判据的牙长在哪些门，等于「哪些门带了会因它改变的那一形」**。D4 是本笔最该记的东西：面侧新门
的第三形（两枚操作数都带裸 `ff`、盘上同时放原字节名与其折叠孪生件）我初版写下的是「这一形对参数缝
**没有分辨力**，因为折叠是幂等——argv 先折一次与按字节走到开档臂再折一次，得到的是同一个名字」，
推理没错，D1 也确实让这一形两侧都不红；但把开档臂的 `from_utf8_lossy` 换成
`from_utf8(name).unwrap_or("")` 时（D4），**这一形当场红**。所以准确表述是分层的：对 **argv 那条缝**
无分辨力，对**折叠写法**有牙。**判据：「某条断言无分辨力」这类结论必须由注入证，不能由推理证**——
推理只能提出候选，能证否的只有把代码改坏一次；门注释已按 D1 与 D4 的读数改写。

**clippy 本轮有过三条新警，修完才进容器门**：`cargo clippy --offline -j 1 --all-targets
-p minix-textfilter` 主树 **151** / 本树 **154**。总数只说明「涨了」，说不出该修几条，于是用**位置
集合差**定位：两份日志各跑 `grep -oE '\-\-> [^ ]+' | sort`，再 `comm -13` 交出本树独有的两个位置，
都在 `tests/comm_face.rs`——一处是夹具 `fn write_raw(dir: &PathBuf, …)` 的 `ptr_arg`（改 `&Path`），
一处是门注释里那行以 `+` 开头的散文被 markdown 当成列表项、触发 `doc_lazy_continuation`（改写行文，
不让续行顶格）。修完复跑：两树各 **151**、位置各 **82**、独有位置零条。两条判据：**①聚合数与位置数
可以不一致**（cargo 那句「(bin X test) generated 1 warning (1 duplicate)」的归属随 target 数漂），
所以**归属用位置集合、结论用总数**；**②新警先修完再进容器门**，别让「门绿」把「门净」顶掉。
（上一笔那条「`--diff` 提交后须带显式 range 复跑」本轮照办，见下面门④。）

**A/B 对 `tmp/comm-409/comm` 十八形 SAME=18 / DIFF=0**（`tmp/comm-460/ab2.log`）。参照件先按批次
四百零七那条验法打 `-Z`：交回 `comm: unknown option -- Z` 再跟那行**写死的** usage、退 1，说明它链的
是 Minix 的 `getopt.c`；该路径是指向 `comm_ref` 的软链，`getprogname()` 算出的程序名与本壳一致，
报文才敢逐字节直比。十八形覆盖字母位六形（`-Z`、裸 `ff`、`-é`、聚簇 `-1\xff`、聚簇 `-1é`、`-Zé`
停在 `Z`）、`--x`、操作数个数三形（零、一、三）、裸 `-` 读标准输入、`--` 终结符、全压三列 `-1 -2 -3`、
聚簇 `-123f`、数据面两档含裸 `ff` 行、`-f` 折叠形（`c3 89` 对 `c3 a9`、`Apple` 对 `apple`）、裸 `ff`
档名、开不了的操数；程序名 token 两侧**全部**归一。**上一笔立的「A/B 前先显式重建 bin」本轮开工即办**：
`cargo build --offline -j 1 -p minix-textfilter --bins` 之后 `file os/target/debug/comm` 确认是 ELF
才打第一遍，所以本笔没再撞上旧入口件。**本支没有新登记任何 C 侧分叉**，全等边界与上一笔同一条：
量的是参数缝与由它到达的地方，不含开档腿（盘上的折叠孪生件就是为排除它而放，排除动作写在 `ab.py`
的模块注释里）。

**五道收尾门逐条读数**（`2026-10-21`，码笔 `570cf7dd2`、文档笔 `970333d94`）：①
`tools/todo-reconcile-scan.sh` → 13 项判据 **1 项 DRIFT**（`探针命名族（行）` 现量 31 vs 台账基线 18），
`git diff --name-only HEAD~2..HEAD` 显示本轮七件全在 `os/commands/usr-bin/textfilter/` 与
`rewrite-notes/`，与该项取数域 `os/kernel/src`、`os/arch/src` 无交集 ⇒ 非本轮引入，归一线在制，
本笔不改脚本基线也不代改台账；② `python3 tools/notes-link-check.py rewrite-notes/` → 764 个 md、
**断链 131**，与上一笔同数，且本轮文档新增行里的 markdown 链接数为 **0**
（`git diff HEAD~2..HEAD -- rewrite-notes/ | grep '^+' | grep -F '](' | wc -l`）⇒ 新增断链键为 0；
③ `tools/todo-staleness-check.sh …TODO-LEDGER-OPEN.md` → **PASS（0 项）**；
④ `tools/doc-style-lint.sh --diff HEAD~2..HEAD` → 3 个文件 / 93 个新增行、**error 级 0**（带显式
range 跑，默认 `HEAD` 提交后空转）；⑤ `tools/check-staged-inflight.sh` → 退 0，避让清单 1 条未触碰。

**门读数补全**：宿主整包 → **69 段 / 1252 / 0**（`tmp/comm-460/pkg-final3.log`）；容器权威门按板
§7.3 那条三挂载配方（`-u $(id -u):$(id -g)` + `ci-cargo`/`ci-target`，跑时未与其他 cargo 并发）→
退 0、**69 段 / 1252 / 0**，与宿主同读数（`tmp/comm-460/container-gate.log`）；三架构
`cargo check --offline -j 1 --target {x86_64-unknown-none, aarch64-unknown-none,
riscv64gc-unknown-none-elf} -p minix-textfilter --bins` 各退 0；`rustfmt --edition 2024 --check`
触及四件零漂移（库件与门注释首跑各有一处本轮新写长行，就地格式化后归零，`bin_support.rs` 未被牵连）。
**基线不当转述用**：本笔开工基线 1250 是上一笔在主树合回之后当场复跑取到的
（`tmp/column-459/main-tree-recheck.log`，其快照即本笔基线 `1f5104e0e`）。

**进度与在册（全部现量）**：参数缝半已落 **十五支**（`rev`、`paste`、`yes`、`tee`、`seq`、`tr`、
`colrm`、`fold`、`col`、`uniq`、`units`、`cksum`、`cmp`、`column`、`comm`）；
`grep -rl "support::args()" os/commands/usr-bin/textfilter/src/bin/ | wc -l` → **24**；命令面口径
（同一条 grep 去掉 `tests/` 与 `bin_support.rs`）→ **93**；渲染半那条 grep 仍 **0** 行；`line_bytes`
**40 处 / 29 件**（本支未新增调用点）。余下 24 支里带字母位诊断面的有 12 支（`cut`、`head`、`join`、
`jot`、`nl`、`pr`、`sort`、`split`、`tail`、`unexpand`、`unifdef`、`vis`），**切片 43 建议 `cut`**：
字母序第一支，与 `comm` 的差别在 `-b`/`-c`/`-d`/`-f` **四个字母都带值**（`cut.c` 的 optstring
`"b:c:d:f:sn"`），参数值会进列表解析、`-d` 还进分隔符，落码面更像 `column` 那一笔；参照件
`tmp/cut-406/cut_ref` 现成（本笔顺手打过 `-Z`：`cut_ref: unknown option -- Z`，Minix 血脉），
用之前仍按批次四百零七那条验法当场复验。独立轮在册不碰：`ARGV-BYTE-PATH-IN-LIBS`、
`CKSUM-CHECK-LEG-TEXTSEAM`、`UNITS-DASH-REWRITE-AT-FIRST-CHAR`、`UNITS-PARTIAL-UNIT-ON-ERROR`、
`UNITS-PATH-SEARCH`、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`、五枚 `*-NON-PERMUTE-OPERAND-TAIL`；
`REV-MULTIBYTE-ERROR-LEG`、`NL-DELIM-LENGTH-RULE` 待裁决；`todo-reconcile-scan` 的 `探针命名族`
漂移归一线。**取证回抄照办**：本笔 release 前把 `tmp/comm-460` 整目录复制回主树 `tmp/`（上一笔立的
那条流程，`tmp/cksum-457` 失踪事故之后第一次执行）。

### 第一百七十五笔（批次四百六十一 · A2 参数缝半切片 43：`cut` 的 argv、`-d` 的分隔符与列表值交回原字节；负控的一段全绿补出一枚库门）

**这一笔先说形状**：`cut` 是本族第二个「参数值直接进输出内容」的壳（第一个是上一笔的 `column`），但它的两条值腿形状相反。`-d` 的值被
`minix3/usr.bin/cut/cut.c:101-102` 的一句 `dchar = *optarg` 取走**首字节**当分隔符，那枚字节拿去切每一行——`column` 的 `-s` 是一族字节、这里是
一枚字节，所以切分判据从「集里有没有这枚」变成「等不等这枚」。而 `-b`/`-c`/`-f` 的值只喂 `get_list`，报错用的又是那两句**不把出错那段拼回
句子**的固定 errx（`cut.c:196` 残留、`:198` 含零），所以列表腿在字节口径上没有任何可回显的东西，它能钉的只有「不许多报也不许少报」。
落码四层：库侧 `os/commands/usr-bin/textfilter/src/cut.rs` 的 `fn parse_list`、`fn parse_token`、`fn scan_number` 三个入口从 `&str` 改
`&[u8]`（切分集「空格、制表符、逗号」按字节判，数字腿本来就走 `is_ascii_digit`，语义一字未动），`CutArgs` 的两份列表与 `operands` 改字节形，
`fn parse_cut_args` 收 `&'a [&'a [u8]]`，环里 `arg == b"--"`、`arg[0] != b'-'`、`char::from(bytes[pos])` 三处按字节，`-d` 的取值改成
`*value.first().unwrap_or(&0)`；壳侧 `src/bin/cut.rs` 走 `support::args_bytes()`/`prog_name_bytes()`、libc 行走 `GetoptReport::line_bytes`、
`-` 的判据 `*operand == b"-"` 按字节，开档与 `warn("%s", *argv)` 两条 `&str` 口之前折一次（在册 `ARGV-BYTE-PATH-IN-LIBS`，壳内现量 2 处）。

**本轮最该被后续读走的，是一枚靠「一段全绿」补出来的门**：负控 E3（把字母位的取法退回按字符取整簇）第一遍红的是面侧新门、跨壳表那一行与面侧
既有的聚簇门，而**库内 620 枚全绿**。这不是注入打偏——同族的 `comm`（批次四百六十 D2）与 `cmp`（四百五十八 W4）在同一枚注入下都先红了库里那枚
`the_unknown_letter_is_the_first_byte_not_the_first_character`，`cut` 独缺，说明**这一支的字母腿在库里从来没钉住过**：环的取法早在批次四百一十之前就
是按字节，可库里所有夹具的字母都是 ASCII，没有任何一枚带过 `c3 a9`。处置是补一枚库门 `the_unknown_letter_is_taken_bytewise_in_the_ring`
（`-é` 与聚簇 `-sé` 都交回 `c3`，ASCII `-Z` 不受波及），补完重跑 E3：红名单从 3 枚变成 **4** 枚，取法与渲染两半这才分开钉住。判据一条：
**每一枚注入都应当至少红一处「本轮所改的那一层」；一整段全绿时先怀疑覆盖面有洞，而不是宣布注入无效**——这是批次四百一十七「门清单由执行结果列，
不由印象列」的另一侧兑现：执行结果不只列出门，还列出**缺了哪些门**。

**门账（先对测试名集合再对总数，批次四百五十九那条判据本轮开工即办）**：库内 `cut` 模块 **11 → 13**，`cut_face` **18 → 19**（新立
`a_raw_byte_delimiter_splits_the_field_and_a_raw_byte_letter_is_named`，配字节形夹具 `fn run_argv`，四段是 `-d<ff> -f1`、`-d <ff> -f2`、
`-d <ff>` 对只含 U+FFFD 的行、字母位 `-\xff`），跨壳表门数 **4 → 4** 不变而表体十二行 → **十三行**（`cut` 行钉 `cut: unknown option -- \xff`
与退 **1**），整包 69 段 **1252 → 1255 passed / 0 failed**。名字集合逐件对账：`cut.rs` 差集恰好是两枚新名、`cut_face.rs` 一枚、壳件与跨壳表零增减，
`#[test]` 属性数与属性后 `fn` 行数成对，没有「同名执行两遍」也没有顶掉邻居属性。既有夹具的期望串逐字未动，只把字面量从文本形改成字节形
（`parse_list("1,,2")` → `parse_list(b"1,,2")`），「找到既有门就翻正、不新增双胞胎」第五次执行。

**一轮 WSL 崩溃插在同一批的负控之前，恢复按「先核现场再动手」走完**：领取树与锁都在（`claim/GETOPT-REPORT-BYTE-LETTER-qoder` @ `a1b4fe638`），
`git status --porcelain` 只有本轮四件，`git diff --numstat` 读数是 `16/10`、`145/83`、`9/5`、`64/0` 全是真改动——没有批次四百零五那种「成片只有删除」
的清零指纹，`tmp/cut-461/` 下的 `patch.py`、`negctl.py` 与 `orig/`（完整相对路径备份）完好，四个可执行件 `file` 出来都是合法 ELF（批次四百五十九那条
「崩过的树除重编外还要 `file` 一下」在这里第二次用上，本轮没撞上截断产物）。复跑宿主整包得 69 段 / 1254 / 0 才续做负控。**同一处还留了一条自己的
取数错**：崩前那一次整包是跑在 `cargo test … | tee 日志 | grep … | head` 这条管道里的，`head` 取够行数就关流，SIGPIPE 把 cargo 打断，日志只剩 23 段、
聚合数被读成 698——**取聚合读数时不要让下游提前关流**：先把输出整份重定向到日志，再对日志算，且跑完必核 `EXIT` 与「段数 = 靶数」这一条对账。

**A/B 对 `tmp/cut-406/cut` 跑二十四形，SAME=24 / DIFF=0**（`tmp/cut-461/ab.log`）。归属验法照旧但要点在**软链名**：`cut_ref` 直接打 `-Z` 交回的是
`cut_ref: unknown option -- Z`，那是程序名差不是措辞差；经 `cut` 这个软链名起跑才交回 `cut: unknown option -- Z` 加三行制表符对齐的 usage、退 1，
与 `minix3/lib/libc/stdlib/getopt.c:101-104` 同形。二十四形里分隔符腿占六形（裸 `ff` 切得开、第二列拿到 `ff` 之后那一段、U+FFFD 三枚不被当分隔符、
`-dé` 取 `c3`、`-dab` 只取 `a`、`-d ''` 取 NUL），列表腿占五形（含裸 `ff` 的 illegal 那句、含零那句、倒序区间不选东西、空项折叠、空列表不是错），
字母位四形、缺值一形、操数与通道六形（裸 `ff` 档名那一形在盘上同时放原字节名与其折叠孪生件，把在册的开档腿从读数里排除，排除方式写在 `ab.py` 的
模块注释里），外加 `--` 终结符、裸 `-` 读标准输入、`-b 2-4`、`-f 2 -s -d,`。**本支没有新登记任何 C 侧分叉**。上一笔立的「A/B 前先显式
`cargo build -p … --bins` 再 `file`」本轮第三次照办。负控四枚（`tmp/cut-461/negctl.py`，逐枚读数 `negctl-E1..E4.log`，汇总 `negctl-final.log`；
基线库内 620、跨壳表 4、`cut_face` 19 三段全绿，命令带 `--no-fail-fast`）各咬 **2/2/4/1**：E1（`-d` 取值退回按字符取首字符低字节）咬库门 + 面门而
跨壳表不红；E2（argv 退回文本缝）咬面门 + 跨壳表而**库内全绿**——「库件的入参本来就是字节，缝在壳上，牙只长在面侧与跨壳表」这条通则第三次量化；
E3 见上；E4（开档那句丢掉操作数名）只咬面侧既有那枚 `an_unopenable_operand_prints_program_operand_and_errno_sentence`，即回显腿的牙长在面侧。
每枚注入前后按整文件 md5 与备份成对复核，跑完核 `git status --porcelain` 只剩本轮四件。

**排版与归属两笔账，还有一处自己的表格事故**：`rustfmt --edition 2024 --check` 在格式化前对 `src/cut.rs` 报 8 处漂移，而 HEAD 那一份同文件本来就报
**3** 处（`fn select_bytes` 的签名折行、`fn in_ranges` 的链式调用、第 456 行那处），所以本轮 diff 里含**三处存量排版归零**——账上写明，免得下一个读者
去追「这几行谁改的」；`cut_face.rs` 那 1 处是本轮新写断言的折行。clippy 用同一条命令 `cargo clippy --offline -j 1 --all-targets -p minix-textfilter`
在主树（= 本笔基线 `a1b4fe638`）与本树各跑一次：各 **151** 条 `^warning`、位置各 **82** 处；触及本轮四件的各 **1** 条，是同一条
`iter().copied().collect()` 在 `cut_face.rs` 里从第 197 行**位移**到第 261 行（被本轮插入的夹具顶下去），位移不是新增（批次四百五十八那条判据第二次兑现）。
自己的事故一条：把「管道」这条取数错写进板 A2 那个表格单元时，命令串里的三枚竖线没转义，行内未转义竖线从 7 涨到 10 —— 表格列数当场被改变；
按批内既有的 `\|` 规矩补转义后回到 7/13 与 HEAD 同构。**判据：把含管道的命令写进台账表格单元，竖线一律 `\|`，改完必核「竖线总数 − 转义数」与 HEAD 一致**。

**五道收尾门逐条读数**（`2026-10-29`，码笔 `2c68ae118`、文档笔 `e41df2570` + SL-5 补笔）：① `tools/todo-reconcile-scan.sh` → 13 项判据里
**1 项 DRIFT**（`探针命名族（行）` 现量 31 vs 台账基线 18），本轮七件全在 `os/commands/usr-bin/textfilter/` 与 `rewrite-notes/`，与该项取数域
`os/kernel/src`、`os/arch/src` 无交集 ⇒ 非本轮引入，归一线在制，本笔不改脚本基线也不代改台账；② `python3 tools/notes-link-check.py rewrite-notes/`
→ 764 个 md、**断链 131**（与上一笔同数），本轮文档新增行的 markdown 链接数为 **0** ⇒ 新增断链键 0；③ `tools/todo-staleness-check.sh …TODO-LEDGER-OPEN.md`
→ **PASS（0 项）**；④ `tools/doc-style-lint.sh --diff HEAD~3..HEAD` → 3 个文件 / 99 个新增行、**error 级 0**——**这里先红了一次**：首跑报
`07-text-filter.md:185 [SL-5] 修复史叙事 :: 原先`，那一处是我把负控的时间顺序写进了正文，改写机制叙述后另落一笔补笔，复跑带显式 range 归零
（默认 `HEAD` 在提交后空转，批次四百五十九那条）；⑤ `tools/check-staged-inflight.sh` → 退 0，避让清单 1 条未触碰。**另记一次解释器用错**：
`doc-style-lint.sh` 是 bash 脚本内嵌 python，`python3 tools/doc-style-lint.sh …` 会报脚本自己的语法错——那是调用错不是门红。

**门读数补全**：宿主整包 → **69 段 / 1255 / 0**（`tmp/cut-461/pkg-host2.log`）；容器权威门按板 §7.3 三挂载配方（`-u $(id -u):$(id -g)` +
`ci-cargo`/`ci-target`，跑时未与其他 cargo 并发）→ 退 0、**69 段 / 1255 / 0**，与宿主同读数（`tmp/cut-461/container-gate.log`）；三架构
`cargo check --offline -j 1 --target {x86_64-unknown-none, aarch64-unknown-none, riscv64gc-unknown-none-elf} -p minix-textfilter --bins` 各退 0；
`rustfmt --edition 2024 --check` 触及四件终态零漂移。开工基线不当转述用：本笔基线 1252 是上一笔在主树合回后当场复跑取到的
（`tmp/comm-460/main-tree-recheck.log`，快照即本笔基线 `a1b4fe638`）。

**进度与在册（全部现量）**：参数缝半已落 **十六支**（`rev`、`paste`、`yes`、`tee`、`seq`、`tr`、`colrm`、`fold`、`col`、`uniq`、`units`、`cksum`、
`cmp`、`column`、`comm`、`cut`）；`grep -rl "support::args()" os/commands/usr-bin/textfilter/src/bin/ | wc -l` → **23**；命令面口径 → **92**；
渲染半那条 grep 仍 **0** 行；`line_bytes` **40 处 / 29 件**（本支未新增调用点）。余下 23 支里带字母位诊断面的有 11 支（`head`、`join`、`jot`、`nl`、
`pr`、`sort`、`split`、`tail`、`unexpand`、`unifdef`、`vis`），**切片 44 建议 `head`**（字母序第一支，`-c`/`-n` 带值且值可前置减号，形状近 `cut`
的列表腿）。参照件本笔当场验过归属：`tmp/family-diag-ref/` 那四支（`head_ref`、`uniq_ref`、`join_ref`、`sort_ref`）打 `-Z` 全部交回
`invalid option -- 'Z'` ⇒ 宿主 glibc 的 `getopt`，**不能**当这一族的差分腿（批次四百零七、四百零八那两次现量的第三次兑现）；可用的是
`tmp/head-415/head`（`head_ref` 的软链名，Minix `getopt.c` 编进去，打 `-Z` 交回 `head: unknown option -- Z`），是批次四百一十五为 `head` 诊断面配的。
独立轮在册不碰：`ARGV-BYTE-PATH-IN-LIBS`、`CUT-SUPPRESS-STILL-PRINTS-NEWLINE`、`CUT-EMPTY-BYTE-LIST-RUNAWAY`、`CKSUM-CHECK-LEG-TEXTSEAM`、
`UNITS-DASH-REWRITE-AT-FIRST-CHAR`、`UNITS-PARTIAL-UNIT-ON-ERROR`、`UNITS-PATH-SEARCH`、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`、
五枚 `*-NON-PERMUTE-OPERAND-TAIL`；`REV-MULTIBYTE-ERROR-LEG`、`NL-DELIM-LENGTH-RULE` 待裁决；`todo-reconcile-scan` 的 `探针命名族` 漂移归一线。
**取证回抄照办**：release 前把 `tmp/cut-461` 整目录复制回主树 `tmp/`。

### 第一百七十六笔（批次四百六十二 · A2 参数缝半切片 44：`head` 的 argv、计数值的回显与横幅档名交回原字节；参照件现量把台账一句「负数都拒」推翻）

**这一笔先说形状**：`head` 是本族第一支「同一段 argv 既进解析又进报文」的壳。前面四支各占一半：`cut` 的 `-d` 值进输出但不回显，
`cut` 的列表值回显不了（`cut.c:196`、`:198` 那两句固定 errx 不带 `%s`）；`column` 的 `-s` 是一族字节进分隔符表、不回显；`comm` 的
optstring `"123f"` 四个字母都不带值，argv 只剩两处用处。而 `minix3/usr.bin/head/head.c:96-97`、`:102-103` 写的是
`errx(1, "illegal byte count -- %s", optarg)`——**值本身被打进句子**。于是一枚裸 `ff` 在这一支同时决定两件事：这个计数转不转得出数，
以及报出来的那句长几个字节；两件事由同一段字节驱动，却能被两条腿分别钉住。第二处独有是**横幅**：`head.c:130-131` 的
`printf("%s==> %s <==\n", …, *argv)` 由命令自己打印，不挤 `minix-types` 里 `err_line` 那条 `&str` 口，所以档名可以逐字节回显，
与开档腿各自独立——这是「档名腿」在本族第一次被拆成两个可分别证伪的对象。

**落码**：库侧 `os/commands/usr-bin/textfilter/src/window.rs` 的 `fn parse_head_args` 收 `&[&[u8]]`，`HeadArgs.operands` 改
`Vec<Vec<u8>>`，`HeadArgError::IllegalCount.value` 从 `String` 改 `Vec<u8>`；环里 `arg == b"--"`、首字节判 `b'-'`、
`char::from(bytes[pos])` 三处按字节；obsolete 那一段重写成「`-n` 两枚字节 + 原串去掉短横后的全部字节」，因为 `head.c:188-192` 拼的是
字符数组，尾巴里的非 UTF-8 字节必须跟着走——它随后被 `strtoimax` 拒掉，并且**被回显**。数值腿仍交给 `parse::<i64>`，但先在字节串上走
`core::str::from_utf8(...).ok()` 再解析：**能转成 i64 的文本必是 ASCII，所以「解码失败」与「这不是一个数」是同一件事**（这条边界论证与
批次四百一十一给 `sort` 的那条同型，此处第三次用）。壳侧 `src/bin/head.rs` 走 `support::args_bytes()`/`prog_name_bytes()`，
`illegal … count` 那句与横幅都按字节拼（`Vec<u8>` 依次接程序名、`": illegal "`、`byte` 或 `line`、`" count -- "`、原值、换行），
只有开档与 `warn("%s", *argv)` 两条 `&str` 口之前折一次（在册 `ARGV-BYTE-PATH-IN-LIBS`，壳内现量
`grep -c from_utf8_lossy os/commands/usr-bin/textfilter/src/bin/head.rs` = **2** 处）。

**最该被后续读走的一条：台账里那句从 C 源码「读出来」的断言被参照件推翻了**。07 篇 head 行写着「零计数与非零负数都拒」，理由是
`head.c:92-103` 的 `strtoimax` 之后紧跟 `*ep || count <= 0`——这句话在源码里确实存在，但只看它是错的：`main` 在 `head.c:73-74` 把
`linecnt`/`bytecnt` 声明成 **`uintmax_t`**，`strtoimax` 交回的 `-1` 一进无符号量就成了天文数字，`count <= 0` 对任何负数都不成立；
随后 `head(fp, linecnt, bytecnt)` 的形参是 `intmax_t`，那个大数又转回 `-1`，于是 `-n -1` 走「一直打到 EOF」那一圈、`-c -1` 在 `fread`
的长度算式里溢出成一枚空窗。现量（参照件对三行档）：`-n -1` 打三行退 0，`-c -1` 什么都不打退 0，而本壳两句都报 `illegal … count -- -1`
退 1。零仍两侧同拒（`0 <= 0` 成立）。同一轮还量出第二处：`strtoimax` 跳前置空白，`-c ' 3'` 在参照件是合法的三字节输出而本壳拒——
但 `-c 5x` 两侧同拒（C 的 `*ep` 那道判据与本壳的「解析不完」等价），`-c +3` 两侧同受，所以这条只窄到「前置空白」一项。两处登记
`HEAD-NEGATIVE-COUNT-UNSIGNED-GUARD`、`HEAD-COUNT-LEADING-BLANK-ACCEPTED`，**登记不执行**：本轮射程是参数缝，一次只修一条，且真修要连带
裁决数值腿到底照 C 的宽松 `strtoimax` 还是照 POSIX 的严格式，那是 `head` 计数面的一整轮。判据：**「从 C 源码读出来的判据」若没有一枚对照
二进制跑过的现量，就还是推断而不是事实**——前例两次都在同一处摔倒（批次四百零六那句「`-k 2,1` 在 C 是 `fields out of order`」、
批次二百八十七的 `-M` 月名排序），这次是第三次，而且这次摔在**已经写进台账好几百轮**的一句上。

**门账（先对测试名集合再对总数）**：库内 `window` 模块 **26 → 27**（新立 `an_illegal_count_value_and_the_operands_carry_the_argv_bytes`，
四段各钉一枚裸 `ff`：`-c<ff>` 的值、`-c 5<ff>` 的整串、obsolete `-5<ff>` 重写后的尾巴、操数原字节），`head_face` **11 → 12**（新立
`an_illegal_count_value_is_echoed_as_the_argv_bytes_and_so_is_the_banner`，配字节形夹具 `fn run_argv` 与按原字节名落盘的
`fn write_raw(dir: &Path, …)`），跨壳表门数 4 → 4 不变而表体十三行 → **十四行**（`head` 行钉 `head: unknown option -- \xff` 与退 **1**；
这一支的 usage 只**一行且带程序名**，与 `cut`/`comm` 那两行把命令名写死在串里不同），整包 69 段 **1255 → 1257 passed / 0 failed**。
既有门的期望串逐字未动，只把夹具字面量从文本形改成字节形。**本轮新记一条判据**：夹具里的 `fn operands` 是 `head` 与 `tail` 共用的助手，
批量改写正则把它一并翻了，编译器当场拦下（`TailArgs.operands` 是 `Vec<String>`，收不到 `Vec<Vec<u8>>`）⇒ **共用助手要另立名字，
不改共用助手的签名**——head 侧新立 `fn operands_bytes`，共用那枚原样留着。同一轮还删掉两条真无用的存量 `unused import`（见下面 clippy 那条）。

**负控四枚各咬 2/1/4/2**（`tmp/head-462/negctl.py`，逐枚读数 `negctl-F1..F4.log`、汇总 `negctl-all.log`；基线库内 621、跨壳表 4、
`head_face` 12 三段全绿，命令带 `--no-fail-fast`）。F1（壳侧 argv 退回文本缝）咬面门 + 跨壳表两枚，**库内 621 全绿**——「库门对 argv 缝没有牙」
这条通则第四次量化（`comm` D1、`cut` E2、本轮 F1，加上批次四百五十九 `column` 那一次）。F2（横幅退回折一次，拿折叠名而不是原字节）只咬
**1** 枚：面门的横幅那一段，而开档腿的期望纹丝不动——这就是「横幅与开档两条腿第一次能被分开钉」的实际兑现，`cut`/`comm` 那两轮做不到这条
是因为它们的档名回显全走 `err_line`，一折就两条腿一起折。F3（字母位退回按字符取整簇）咬 **4** 枚：库内既有字母门、面侧既有
`the_libc_line_of_a_multibyte_flag_is_one_byte_for_head`、跨壳表、面侧新门。F4（obsolete 重写那一段折一次 lossy）咬 **2** 枚：库内新门 + 面门，
钉的是「重写尾巴必须带着原字节走」这条只有 `head` 用得上的性质。每枚前后按整文件 md5 与 `tmp/head-462/orig/`（完整相对路径备份）成对复核，
跑完核 `git status --porcelain` 只剩本轮四件。

**A/B 对 `tmp/head-415/head` 跑，非预期二十四形 SAME=24 / DIFF=0**（`tmp/head-462/ab.log`；另有三形落在上面两条新登记的分叉上，脚本按类
分开计数、不计入回归）。二十四形覆盖字母位五形（`-Z`、裸 `ff`、`-é`、聚簇 `-q\xff`、`-Zé` 停在 `Z`）、缺值一形、**值回显四形**
（`-c<ff>`、`-c 5<ff>`、`-n<ff>`、obsolete `-5<ff>`——这四形是本轮真正的新覆盖面）、计数与错误五形、操数与横幅五形（含裸 `ff` 档名，
盘上同时放折叠孪生件以排除在册的开档腿）、`--` 终结符、裸 `-` 当档名（`head` 不像 `cut` 特判标准输入）、字节模式无尾换行。程序名 token
两侧全部归一。**参照件归属探针本轮多看了一个字段**：`tmp/family-diag-ref/head_ref` 打 `-Z` 交回 `invalid option -- 'Z'`，那是宿主 glibc 的
`getopt`；而它的 usage 行里程序名位是 `?`——**同一枚探针要同时看措辞与程序名位**，只挑一句看就会把「措辞差 + 程序名差」读成字母差。
可用的是批次四百一十五为 `head` 配的 `tmp/head-415/head`（软链名、Minix `getopt.c` 编进去，打 `-Z` 交回 `head: unknown option -- Z`，
本轮当场复跑过）。

**排版与归属**：`rustfmt --edition 2024 --check` 四件终态零漂移，且**本轮没有存量牵连**——格式化前 `src/window.rs` 与 `tests/head_face.rs`
各报 3 处，而 HEAD 那两份都报 **0** 处，所以那三处全是本轮改动自造、与批次四百六十一那种「diff 里含三处存量排版归零」不是一回事。
`cargo clippy --offline -j 1 --all-targets -p minix-textfilter` 主树 **151** 条 / 本树 **149** 条、位置数 82 → 80：少掉的正是夹具改字节形之后
真无用的两条存量 `unused import`，触及本轮四件的其余那一条（`window.rs:213` 的 `needless_range_loop`）在两树同一行号。**判据：警数下降也要写清
是哪几条、为什么少**，否则「151 → 149」会被下一个读者当成统计噪声，甚至以为是自己那一笔的功劳。

**基线这条要坦白**：本轮没有改前当场复跑整包（四件改动先落码），delta 是靠门名集合对账闭合的——`window` +1、`head_face` +1、两件壳件与
跨壳表零增减，于是 1255（上一笔在主树合回后当场复跑的那次读数）+ 2 = 1257 与本轮三次宿主、一次容器读数一致。**当改动只加门、不动既有门时，
名字集合对账与基线复跑等价；一旦既有门的期望串也被改（本轮夹具形就是这种），这个等价就不成立，下轮仍须当场复跑基线。**

**五道收尾门逐条读数**（`2026-10-29`，码笔 `280f45170`、文档笔 `c1943dcd5`）：① `bash tools/todo-reconcile-scan.sh` → 13 项判据里
**1 项 DRIFT**（`探针命名族（行）` 现量 31 vs 台账基线 18），本轮七件（四件代码 + 三件文档）全在 `os/commands/usr-bin/textfilter/` 与 `rewrite-notes/`，与该项取数域
`os/kernel/src`、`os/arch/src` 无交集 ⇒ 非本轮引入，归一线在制，本笔不改脚本基线也不代改台账；② `python3 tools/notes-link-check.py rewrite-notes/`
→ 764 个 md、**断链 131**（与上一笔同数），本轮新增行落在 `todo.md:122..227`、`07-text-filter.md:6/192`、板 `:27`，逐行核对该清单在这三处
**零命中**（板那一条在 `:35`，非本轮）⇒ 新增断链键 0；③ `bash tools/todo-staleness-check.sh …TODO-LEDGER-OPEN.md` → **PASS（0 项）**；
④ `bash tools/doc-style-lint.sh --diff HEAD~2..HEAD` → 3 个文件 / 108 个新增行、**error 级 0**（本轮一次过；新行里凡涉及时间顺序的说法一律写成机制叙述，
这是批次四百六十一那枚补笔留下的口径——本笔随后自己撞了一次，见下面那笔补笔）；⑤ `bash tools/check-staged-inflight.sh` → 退 0，避让清单 1 条
（`TODO-3ARCH-PARITY-20261006.md`）未触碰。

**门读数补全**：宿主整包 `cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` → **69 段 / 1257 / 0**（负控还原后复跑，
`tmp/head-462/pkg-3.log`；`--lib` 单靶 621、`head_face` 12、`bin_support_face` 4 三段各自复核过）；容器权威门按板 §7.3 三挂载配方
（`-u $(id -u):$(id -g)` + `ci-cargo`/`ci-target`，跑时未与其他 cargo 并发）→ 退 0、**69 段 / 1257 / 0**，与宿主同读数
（`tmp/head-462/container-gate.log`）；三架构 `cargo check --offline -j 1 --target {x86_64-unknown-none, aarch64-unknown-none,
riscv64gc-unknown-none-elf} -p minix-textfilter --bins` 各退 0。A/B 用的产物按批次四百五十九那条判据复核到位：`os/target/debug/head` 是合法 ELF、mtime `19:30:26` 新于四件源码里最晚的
`19:29:42`，而 `ab.log` 写在 `19:30:55` ⇒ 差分跑的确实是改后件。

**进度与在册（全部现量）**：参数缝半已落 **十七支**（`rev`、`paste`、`yes`、`tee`、`seq`、`tr`、`colrm`、`fold`、`col`、`uniq`、`units`、
`cksum`、`cmp`、`column`、`comm`、`cut`、`head`）；`grep -rl "support::args()" os/commands/usr-bin/textfilter/src/bin/ | wc -l` → **22**；
命令面口径 → **91**；渲染半那条 grep 仍 **0** 行；`line_bytes` **40 处 / 29 件**（本支未新增调用点，用的是批次二百九十四起就存在的那一处）。
余下 22 支里带字母位诊断面的有 **10** 支（`join`、`jot`、`nl`、`pr`、`sort`、`split`、`tail`、`unexpand`、`unifdef`、`vis`），**切片 45 建议
`join`**：字母序第一支，`join.c:133` 的 optstring 是 `"\01a:e:j:1:2:o:t:v:"`，八个字母**全带值**，其中 `-o` 的格式串既进解析又进输出
（它决定每一行怎么拼），所以这一支的形状是 `cut` 的列表腿与 `column` 的值腿合体，还要加上 `head` 这种「值进报文」的第三条腿；开头那枚 `\01`
的待遇要对着 Minix 的 `getopt.c` 现查，别按同族规律补。参照件 `tmp/join-407/join_ref` 现成（批次四百零七为本族重配的那一份），用前仍先打
`-Z`。独立轮在册不碰：`ARGV-BYTE-PATH-IN-LIBS`、`CKSUM-CHECK-LEG-TEXTSEAM`、`UNITS-DASH-REWRITE-AT-FIRST-CHAR`、
`UNITS-PARTIAL-UNIT-ON-ERROR`、`UNITS-PATH-SEARCH`、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`、五枚 `*-NON-PERMUTE-OPERAND-TAIL`、
`CUT-SUPPRESS-STILL-PRINTS-NEWLINE`、`CUT-EMPTY-BYTE-LIST-RUNAWAY`、`COL-CLIPPY-STOCK-LINT`、`BIN_SUPPORT-FMT-DRIFT-STOCK`、
`REV-MULTIBYTE-ERROR-LEG`、`NL-DELIM-LENGTH-RULE`，**本轮新立两枚** `HEAD-NEGATIVE-COUNT-UNSIGNED-GUARD`、
`HEAD-COUNT-LEADING-BLANK-ACCEPTED`；`todo-reconcile-scan` 的 `探针命名族` 漂移归一线。**取证回抄照办**：release 前把 `tmp/head-462` 整目录
复制回主树 `tmp/`。

**合回读数（第一百七十六笔补记，批次四百六十二 补笔）**：本支五笔（码笔 `280f45170`、文档笔 `c1943dcd5`、里程碑笔 `b77aa3f1b`、SL-5
补笔 `3990a194b`、合入笔 `419314c98`）在合回前已在领取树上把 `rewrite` 那五笔 C3 收官笔合进来，冲突只有一处——`new_edge4.md` 的同一处尾追，
按两侧各留全解（本笔的 `###` 段在前、C 板那条流水行在后）。**逐笔验祖先**：五个哈希对 `rewrite` 全部 `merge-base --is-ancestor` 为真，
主树 `rewrite` 从 `f425e8c0f` 前进到 `419314c98` 是 fast-forward，主树脏文件与入栈八件零交集（`git status --porcelain` 与
`git diff --name-only rewrite...claim/*` 取集合差）。主树合回后当场复跑 `cargo test --offline -j 1 --no-fail-fast -p minix-textfilter`
→ 退 0、**69 段 / 1257 / 0**（`tmp/head-462-main-recheck.log`），与领取树合回前后两次同读数；取证目录 `tmp/head-462`（25 件）已回抄主树。
**跨线告知一条**：门④ `doc-style-lint --diff` 的范围若含合入笔，会在 `PARALLEL-20261018-C-KERN-QEMU.md` 第 26 那一行报出
`[SL-11]`，命中的是 C 板那条流水行自己的尾缀编号——那一行在 `rewrite` 的文件里已存在（`git show rewrite:… | sed -n '26p'` 现量命中），
属 C 板合回内容而非本支引入；本支四笔的增量范围
`HEAD~4..HEAD~1` 复跑仍是 4 个文件 / 207 个新增行、**error 级 0**。**判据：合回之后再跑增量门，范围里混进他人笔就要按「谁的行」分开归因，
不能替他人认账，也不能因为门红就把自己那笔回退。**

- `2026-10-20` zcode-glm / C 板收尾（**C3 首源读数到手——B 板代领完成，C 板可做件终局清零**）：`claim/C3-zcode-glm`（release 先于 merge 险情以提交哈希 `ef23cbbf0` 直接合并化解）→ 合并笔 `f425e8c0f`。交付=UART RX（PLIC IRQ10）回环驱动（`kernel/uart_irq.rs`：register_hook 首挂自动 unmask + `set_irq_priority` 三层 API + `sie.SEIE` + IER.ERBFI + **脉冲式 MCR.LOOP** 使能序）+ 载体阶段（test-timer-irq PASS 门=uptime ∧ 首源 matched>0）。诊断三层定案：①上段「stride 0x2000 修复」实为**误修**（QEMU `sifive_plic.c` CONTEXT_STRIDE=0x1000、规范 CONTEXT1@0x201000、DTB `interrupts-extended <0x0b,0x09>` 证 hart0-S=context1——回滚并留注）②真病根=脉冲式 LOOP 序（LOOP 悬置=同 UART 控制台内环失聪+控制台字节反成 RX 垃圾源）③修后 claim→钩子→match 三环真机全通：`uart-rx first-source ok`+`TEST_RESULT: PASS`，mock 834/0/8、aarch64 check 0 error、全量 **39/0/0 维持**。PLIC 设备驱动两件必修常识入板行：**priority 复位 0=永不投递必写**、**MCR.LOOP 悬置=控制台失聪**。C 板 21 行终态=全闭或外部门控（C2b/C4/C7 族/C11 案结/C1 案结/C3 首源✅本笔解/C8-C21 人裁绑定），门控地图与解除钥匙见 PARALLEL-C。

### 第一百七十七笔（批次四百六十三 · A2 参数缝半切片 45：`join` 的 argv、`-t` 的判据、`-e` 的输出与三支值回显交回原字节；参数缝第一次改到「接受还是拒绝」那一层）

**这一笔先说形状**：前面十七支都在「同一个判据给不给得出、报出来长几个字节」这一侧打转，`join` 是第一支
**参数缝改到判据本身**的壳。`minix3/usr.bin/join/join.c:201` 写的是 `if (strlen(tabchar = optarg) != 1)`——
量的是字节数。文本缝之下 `bin_support::args()` 先过 `from_utf8_lossy`，一枚裸 `ff` 变成 `ef bf bd` 三枚，于是
`join -t<ff>` 在 C 里是「接受，并拿这枚字节去切每一行」，在本壳里是「拒绝」。三侧读数（
`tmp/join-463/before-after-smoke.log`，改前那件取自合回前的主树产物）：改前 `join: illegal tab character
specification` 退 1，改后与参照件 `tmp/join-463/join` 都退 0 且逐字节交回 `a<ff>1<ff>x`。同一枚字节还走第二条腿：
`join.c:495` 打的是 `printf("%c", *tabchar)`，分隔符集的首字节同时是**展出的每一行拿什么连起来**。`-e` 更直接——
`:500-501` 的 `printf("%s", empty)` 把替代串打进空缺字段，折过的形态让输出里多出两枚字节
（`before-after-smoke-2.log`：改前 `ef bf bd x`，改后与参照件同为 `ff x`）。加上三支把 `optarg`/整个 token 拼进句子的
回显（`:145`/`:156`/`:190`、`:176`/`:222`、`:592`），**判据、输出、报文三条腿在这一支齐了**。落码四层：库侧
`os/commands/usr-bin/textfilter/src/join.rs` 的 `fn parse_join_args` 收 `&'a [&'a [u8]]`、`JoinArgs.files` 改
`Vec<&'a [u8]>`、`JoinError` 三支回显值改 `&'a [u8]`，`fn parse_olist`/`fn strtol_decimal`/`fn is_field_spec`
三个入口收 `&[u8]`（切分集「逗号、空格、制表符」按字节判，数字腿本来按 `is_ascii_digit`，语义一字未动），环里
`a == b"--"`、首字节判 `b'-'`、`char::from(b[1])` 三处按字节；壳侧 `src/bin/join.rs` 走 `args_bytes()`/
`prog_name_bytes()`，`diagnose`/`fail`/`warn_then_usage` 三兄弟的短语参数改 `&[u8]`（新立 `fn quoted`），
开档与 `err(1, "%s", name)` 收进同一个 `read_operand` 闭包折一次（在册 `ARGV-BYTE-PATH-IN-LIBS`，壳内现量 2 处）。

**顺带查清了一处照抄了很久却没问过为什么的字面量**：`join.c:133` 递的 optstring 是 `"\01a:e:j:1:2:o:t:v:"`，
开头那枚 `\01` 不是装饰。`obsolete()` 在 `:568` 把裸 `-a` 的第二个字符就地写成 `'\01'`，而 BSD `getopt` 用
`getopt.c:98` 的 `strchr(ostr, optopt)` 去认这个字母——**那枚字节必须在选项串里**才落得进 `join.c:135` 的
`case '\01'`；它的下一位是 `a`，于是 `getopt.c:109` 的 `oli[1] != ':'` 把这枚合成字母当**无值**字母处理，
这正是 legacy 裸 `-a` 不吞掉第一个文件名的机制来历。它占串首位也安全：同一份 `getopt.c` 只对 `*ostr == ':'`
（`:101`、`:130`）与 `strchr(ostr, '-')`（`:90`）看首字符。判据一条：**照抄字面量时，「它为什么在这个位置」是
也要查清的另一半**——只抄形状不抄理由，等于把一段机制知识留在原地没人接手。

**门账（先对测试名集合再对总数）**：库内 `join` 模块 **26 → 28**（`a_raw_byte_is_the_delimiter_the_replacement_and_the_three_echoes`
六段钉判据/输出/三支回显/操数；`the_ring_names_the_first_byte_of_a_multibyte_letter` 四段钉字母位取法），`join_face`
**30 → 32**（`a_raw_byte_delimiter_is_accepted_and_a_raw_byte_replacement_is_printed` 四段、
`the_three_value_echoes_carry_the_argv_bytes` 三段），跨壳表门数 4 → 4 不变而表体十四行 → **十五行**，整包 69 段
**1257 → 1261 passed / 0 failed**（基线是本轮开工时在该树当场复跑的 69 段 / 1257 / 0，快照 `a204bc6b6`）。
既有 28 处 `parse_join_args(&[…])`、11 处 `parse_olist("…")`、7 处 `strtol_decimal("…")` 全改成字节形，期望串逐字未动。
**夹具这一层学到新的一条**：`JoinArgs` 借的是词表里那些字节，所以 `let a = parse_join_args(&argv(&[…]))` 这种写法
会被编译器当场拒——临时 `Vec` 撑不过一条语句；凡结果要留着断言的 11 形都拆成 `let w_x = argv(&[…]);` +
`let x = parse_join_args(&w_x);` 两行，只在一条语句里用完的形才可以内联。测试模块里的 `fn argv` 助手因此必须
交回 `Vec<&'static [u8]>` 而不是在调用点造临时量。

**负控五枚各咬 3/1/1/3/2**（`tmp/join-463/negctl.py`，逐枚读数 `negctl-J1..J5.log`、汇总 `negctl-all.log`；
基线三档全绿：库内 622、跨壳表 4、`join_face` 32，命令带 `--no-fail-fast`）。J1（壳侧 argv 退回文本缝）咬面门两枚 +
跨壳表，**库内 622 全绿** ⇒「库门对 argv 缝没有牙」这条通则第五次量化（`column`、`comm`、`cut`、`head`、本轮）。
J2（三支回显折一次）只红报文那一枚，J5（`-e` 的值折一次）只红输出那一枚——**「值进报文」与「值进输出」两条腿在
同一支命令里第一次被分开证伪**，这是前三支做不到的（它们的值只走一条腿）。J3（`-t` 的长度判据退回按字符数）咬的正是
`-t<é>`（两个字节、合法 UTF-8）那枚**控制形**：三侧同拒，钉住「缝改成字节不等于什么都放宽」。J4（字母位取法退回按
字符取整簇）第一遍只咬 2 枚（跨壳表 + 面侧既有 `the_unknown_letter_of_a_multibyte_flag_is_written_as_one_byte`）而
**库内全绿**——`join` 的字母腿在库里从来没钉住过（库里所有夹具的字母都是 ASCII），于是补出
`the_ring_names_the_first_byte_of_a_multibyte_letter` 并**重跑 J4 变 3 枚**；这是批次四百六十一给 `cut` 补门那条
判据的第二次兑现：**一整段全绿时先怀疑覆盖面有洞，而不是宣布注入无效**。

**A/B 给了两次教训，一次假全等、一次真差异**：第一遍三十六形报 `SAME=31 / DIFF=0`，看退码列才发现数据面那批形全是
`1/1`——脚本里 `~N` 占位映射到 `/tmp/join-463-ab/1`，而落档用的键是 `t1`/`s1`，**两套编号不一致**让两侧都撞上
「档打不开」这条同形出口：全等是真的，覆盖面是空的。判据一条：**差分全等之前先数退 0 的形有几枚**，成功形一枚都没
成功的全等读数等于没测。占位对齐后立刻逮到 `join - 档` 这一形：参照件打 `a 1 1`，本壳交回空。拿改前的件（主树产物）
同形复跑也是空 ⇒ **存量、非本轮引入**；根因是 `src/bin/join.rs:read_stdin` 自造一条直取 `minix_sys::read` 的缝，
而那条缝在宿主没有双生面（同族 `cksum`/`cmp`/`col`/`column`/`comm`/`cut`/`fold`/`head` 八支都走
`bin_support.rs:read_stdin`）。登记 `JOIN-STDIN-LEG-BYPASSES-DUAL-SURFACE` **登记不执行**：本轮射程是参数缝，
一次只修一条，而这一修的收口形（stdin 真带内容流过来）属读缝那条腿。顺带一条覆盖账：`join_face` 三十二枚里没有一枚
让 stdin 带过内容（只有 `two_stdin_operands_are_refused_before_anything_is_read` 碰过那支错误出口），所以它在门账上
是 P0-test-missing 而不是 P0-code-bug 的副产品。最终读数：**非预期三十形 SAME=30 / DIFF=0**（`tmp/join-463/ab.log`；
已知分叉另计两形——在册开档腿的名字位折叠、以及上面那条 stdin 缺口），程序名 token 两侧全部归一。参照件归属探针照例
先打 `-Z`：`tmp/join-407/join_ref` 交回 `join_ref: unknown option -- Z`（Minix 血脉），同批 `tmp/family-diag-ref/join_ref`
交回 glibc 那句带引号的 `invalid option -- 'Z'`，不能用——第三次兑现同一条判据。

**取数三条自错，全是「读数看着绿、其实没测」那一类**：① `cargo fmt --offline -p … -- --check` 报
`error: unexpected argument '--offline' found` 并退 2，而下游 `grep -c '^Diff in'` 交回 0——**门计数必须连 EXIT
一起核**，`grep` 的 0 可以是「根本没跑」；改用逐文件 `rustfmt --edition 2024 --check` 并对 HEAD 同件各测一次，
四件现量 0、HEAD 也 0（本轮无存量牵连）。② clippy 的位置集合因两次 `sed` 归一化写法不同（`s/-->> *//` 与
`s/^--+> +//`）得出「80 条全在本树独有」的假差异 ⇒ **位置对账的归一化命令要固定成一条**，否则量到的是脚本不是
代码；改成同一条后主树 149 / 本树 149、位置 80 vs 80、双向差集为空、本轮四件零命中。③ rustfmt 一次读数给出
4/1/3、复跑给出 0，复跑时同时核 `md5` 与 `tmp/join-463/post/` 快照一致 ⇒ **同一份文件上的计数复现不了，先取指纹
再定案**。

**一起自造事故：表格行被换行拆成 42 个物理行**。给板 A2 那一行追加大段时，脚本里那段 python 字符串**带着换行**，
于是 markdown 的一行表格被写成 42 行，未转义竖线从 7 变成 6（表格少了一个列边界，渲染会塌）。修法是把这些物理行
按原顺序并回一行并做三重核验：**未转义竖线数回到 7**、转义数 16 → 18（本笔新增的两枚 `\|` 在预期内）、以及把
HEAD 那行做同样两处状态列改写后与新行**逐字符对到前缀末尾**（67313 字符全等，证明并回来的过程中没有改动别人的
内容）。判据：**表格行是物理单行，往里追加长段之前先确认写入路径不会带换行；改完必核「未转义竖线 = 列数 + 1」，
再用前缀对账证明存量段未受牵连**。

**五道收尾门逐条读数**（`2026-10-29`，码笔 `907bfad76`、文档笔 `ff3ce588d`）：① `bash tools/todo-reconcile-scan.sh`
→ 13 项判据里 **1 项 DRIFT**（`探针命名族（行）` 现量 32 vs 台账基线 18）。本轮四件在该项取数域里**零命中**
（`grep -rnE "nk4[ac]:" os --include=*.rs` 的 32 处全在 `os/kernel`、`os/arch`、`os/libs`、`os/servers`、`os/fs`），
31 → 32 那一枚来自合进来的 C3 收官笔（`os/kernel/src/uart_irq.rs` 在本笔基线是 1 处、在上一笔的树上是 0 处，
当场 `git show` 两边各数过）⇒ 非本轮引入，归一线在制，本笔不改脚本基线也不代改台账；② `python3 tools/notes-link-check.py rewrite-notes/`
→ 764 个 md、**断链 131**（与上一笔同数），本轮新增行落在 `todo.md:122..`、`07-text-filter.md:6/219`、板 `:27`，
逐行核对该清单在这三处零命中 ⇒ 新增断链键 0；③ `bash tools/todo-staleness-check.sh …TODO-LEDGER-OPEN.md` →
**PASS（0 项）**；④ `bash tools/doc-style-lint.sh --diff HEAD~2..HEAD` → 3 个文件 / 107 个新增行、**error 级 0**
（一次过；写板行与 07 篇时把「时间顺序那一类说法」一律写成机制叙述，上一笔那两枚自撞触发词的教训在前）；
⑤ `bash tools/check-staged-inflight.sh` → 退 0，避让清单 1 条（`TODO-3ARCH-PARITY-20261006.md`）未触碰。

**门读数补全**：宿主整包 `cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` → **69 段 / 1261 / 0**
（补 J4 那枚库门之前一次 1260，两遍都落日志 `tmp/join-463/pkg-1.log`、`pkg-2.log`；负控还原后复跑同读数）；
容器权威门按 §7.3 三挂载配方（`-u $(id -u):$(id -g)` + `ci-cargo`/`ci-target`，跑时未与其他 cargo 并发）→ 退 0、
**69 段 / 1261 / 0**，与宿主同读数（`tmp/join-463/container-gate.log`）；三架构 `cargo check --offline -j 1
--target {x86_64-unknown-none, aarch64-unknown-none, riscv64gc-unknown-none-elf} -p minix-textfilter --bins`
各退 0、错误 0；A/B 用的 `os/target/debug/join` 是显式 `cargo build -p minix-textfilter --bins` 之后 `file` 过的
合法 ELF（批次四百五十九那条判据第五次照办）。

**进度与在册（全部现量）**：参数缝半已落 **十八支**（`rev`、`paste`、`yes`、`tee`、`seq`、`tr`、`colrm`、`fold`、
`col`、`uniq`、`units`、`cksum`、`cmp`、`column`、`comm`、`cut`、`head`、`join`）；
`grep -rl "support::args()" os/commands/usr-bin/textfilter/src/bin/ | wc -l` → **21**；命令面口径 → **90**；
渲染半那条 grep 仍 **0** 行；`line_bytes` **40 处 / 29 件**（本支未新增调用点，用的是批次四百零七起就存在的那一处）。
余下 21 支里带字母位诊断面的有 **9** 支（`jot`、`nl`、`pr`、`sort`、`split`、`tail`、`unexpand`、`unifdef`、`vis`），
**切片 46 建议 `jot`**：字母序第一支，`jot.c` 的 `-w`/`-s` 两个值都直接进输出行（形状近 `seq` 那条「值即内容」的腿），
参照件用前照例先打一枚 `-Z`。**另建议插一小轮收 `JOIN-STDIN-LEG-BYPASSES-DUAL-SURFACE`**——改动面是把
`src/bin/join.rs:read_stdin` 换成 `support::read_stdin()`、补一枚「stdin 带内容的连接」端到端门、再跑一次 A/B
把那形从已知分叉挪进全等，一条腿、一枚门、一次差分，正好一轮的量。独立轮在册不碰：`ARGV-BYTE-PATH-IN-LIBS`、
`CKSUM-CHECK-LEG-TEXTSEAM`、`UNITS-DASH-REWRITE-AT-FIRST-CHAR`、`UNITS-PARTIAL-UNIT-ON-ERROR`、`UNITS-PATH-SEARCH`、
`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`、五枚 `*-NON-PERMUTE-OPERAND-TAIL`、`CUT-SUPPRESS-STILL-PRINTS-NEWLINE`、
`CUT-EMPTY-BYTE-LIST-RUNAWAY`、`COL-CLIPPY-STOCK-LINT`、`BIN_SUPPORT-FMT-DRIFT-STOCK`、`REV-MULTIBYTE-ERROR-LEG`、
`NL-DELIM-LENGTH-RULE`、`HEAD-NEGATIVE-COUNT-UNSIGNED-GUARD`、`HEAD-COUNT-LEADING-BLANK-ACCEPTED`，**本轮新立**
`JOIN-STDIN-LEG-BYPASSES-DUAL-SURFACE`；`todo-reconcile-scan` 的 `探针命名族` 漂移归一线。**取证回抄照办**：
release 前把 `tmp/join-463` 整目录复制回主树 `tmp/`。

**合回读数（第一百七十七笔补记，批次四百六十三）**：本支四笔（码笔 `907bfad76`、文档笔 `ff3ce588d`、里程碑笔 `e27d5bbbc`、
合入笔 `5a3066cc5`）在合回前把 `rewrite` 那十二笔 01TODO-B1 收进来，冲突只有一处——`new_edge4.md` 的同一处尾追，按两侧
各留全解（本笔的 `###` 段在前、那条流水行在后）。**逐笔验祖先**：四个哈希对 `rewrite` 全部
`merge-base --is-ancestor` 为真，主树 `rewrite` 从 `e24a7aa1f` 前进到 `5a3066cc5` 是 fast-forward，主树脏文件与入栈八件
零交集（`git status --porcelain` 的词表与 `git diff --name-only rewrite...claim/*` 取集合差）。主树合回后当场复跑
`cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` → 退 0、**69 段 / 1261 / 0**（`tmp/join-463-main-recheck.log`），
与领取树合回前后两次同读数；取证目录 `tmp/join-463`（39 项，含 `orig/` 与 `post/` 两份快照）已回抄主树。
门④按两种范围各跑一次都是 error 0：本支三笔的 `HEAD~4..HEAD~1`（4 个文件 / 221 个新增行）与含合入笔的
`HEAD~2..HEAD`（3 个文件 / 148 个新增行）——这一回合进来的那十二笔没有触发任何规则，与上一笔撞上他线
`[SL-11]` 的情形不同，所以「范围含合入笔就会红」不是规律，只是上一次恰好有。**门②的跨树教训再记一遍**：
同一条 `notes-link-check` 在领取树是 764 个 md / 断链 131，在主树是 2010 个 md / 222——主树多出的那一千多个
文件是别的线未入库的工作笔记，两个数**不同树不可比**，做集合差要在同一棵树里取基线。

- `2026-10-29` zcode-glm / 01 篇 todo 首批 10 条（`claim/01TODO-B1-zcode-glm` → 合并笔 `5a133a44b`）：D-71（`BootForm::Aarch64UefiCrossImage` 落点改 `os/libs/minix-boot/src/handoff.rs`，形态测试补五落点存在性断言）、I-26（opensbi_helpers 孤儿 doc 与 `hardcoded` 双归零）、N-3（删 `uefi_helpers` 两个零消费者 pub fn）、N-1（`BootAlloc::resume`/`region` 区间自洽断言 + 五枚宿主测试）、N-2（vm_handoff 扣除记录与交接清单两处分述）、TODO-02（`pt_alloc` 写一次改结构性拒绝：debug panic / release 不覆盖，集成测试 2 条 debug 与 release 双形态实跑）、TODO-03（三枚落地区超容量 fail-fast 负控）、I-27+L1424（OQ-N6 已裁口径 + §3.1/§3.3 现状指针 + main.rs 头注）、§5.2.1（opensbi 12 / `15+12+1=28` / paging 15 / boot_alloc 10 / store 系列 4 / minix-boot 32 全量复算）、§4.6（`IDENTITY_MAP_END` 按别名实况改写）。**门读数**：docker `minix-ci:1.94` 四包——minix-boot 32/0、boot-shim `--features test-all` 28/0、minix-arch `--features runtime-window` lib 270/0 + 集成各腿 0 failed、minix-kernel `--features mock` 842/0/8；`doc-style-lint --diff` error 0。**在册**：N-4 待 D-68、N-5 取证项、TODO-01/04/06/07 与 §2 批次未动；01-todo.md / todo.md 状态列已在主树工作区刷新（脏版不代提交）。

- `2026-10-29` zcode-glm / 01 篇 todo 第二批（`claim/01TODO-B2-zcode-glm` → 合并笔 `fa7c0fbe5`）：I-22 回勾（bootface 头注去掉对已删 `build_memmap` 的指涉；文档连带 ①–⑦ 逐项核过，现算 `硬编码的 QEMU virt DRAM 布局|单条 DRAM` → 0）、TODO-07（`assert_bootstrap_outside_memmap` 三案：相离/贴边通过、包含/跨界 panic；首跑以「贴尾被当贴边」的红纠正了用例本身）、TODO-06（`test-cmd-smoke.sh` Stage 3b 断言 shim 三路标，实跑三行全在）、TODO-01（四入口源码序钉 `store_before_alloc_pin.rs` + `arch_boot_impl` 入口 `assert!(kernel_info().is_some())`（`not(mock)`，含 QEMU release 件））。**N-5 转独立 QEMU 探针批次**：当前产线三块落地区已把可观测症状消掉，历史读数无法直接复现，判别设计留在 01-todo §3。**门读数**：boot-shim `--features test-all` 31/0（默认 uefi 19/0）、kernel mock 842/0/8 + 序钉 1/0、三架构生产形态 check 各 0 error、`doc-style-lint --diff HEAD~4..HEAD` error 0。**诚实边界**：workspace 的 x86 smoke 脚本在本批实跑中 M14 段通过、随后在 du 读侧总计腿失败；主树 HEAD 基线复跑同一脚本同样失败（rc=1、同一条 FAIL），属既有问题（该脚本不在 run_all 39 段内），未计入本批回归。


### 第一百七十八笔（批次四百六十四 · A2 参数缝半切片 46：`jot` 的 argv、词槽、分隔符与操作数尾字节交回原字节；三条「只有按字节才成立」的判据一次钉死）

**这一笔先说形状**：`jot` 的 argv 有五个去处。三个直接是**输出内容**——`-b` 与 `-w` 落进同一个
`format` 槽（`minix3/usr.bin/jot/jot.c:173-176` 的 `boring = 1; /* FALLTHROUGH */ case 'w':`），由
`putdata`（`:300-311`）打出去，`-s` 存进 `sepstring`（`:169-170`）再由 `:310` 的 `fputs` 打在条目之间；
第四个是**报文**（`:189`、`:215`、`:225`、`:385`、`:396` 那五句里的 `%s`）；第五个最特别——操作数的
**数值本身**：`:196`、`:204` 的 `argv[i][strlen(argv[i]) - 1]` 取的是最后一个字节。前面十九支里
「值进输出」与「值进报文」最多各占一条腿，`jot` 一次占齐，还多出一条谁都没遇到过的：**取到的字节要
存进 `char`**。

**三条判据级读数（这才是本轮的正菜，都不是渲染问题）**：① 那个赋值目标是 `char`，而 x86 的 `char`
带符号，所以一枚 `ff` 是 **-1** 而不是 255。三形现量（参照件 `tmp/jot-464/jot`）：`jot 3 <ff> 5` →
`-1 2 5`（begin = -1，步长 (5-(-1))/2 = 3）、`jot 3 1 <ff>` → `1 0 -1`、`jot -c 3 1 <ff>` → 字节
`01 00 ff`；按无符号读中间那格从 `00` 变成 `80`，而文本缝之下更糟：`ff` 先被折成三枚，取到的最后一枚
是 `bd` = 189，改前的件同形打的是 `189 97 5`。② `%c` 走 `putchar`，交回**一枚**字节：参照件对
`-p 0 -w '%c' 1 200 1` 现量 `c8`，本层那句 `format!("{}", v as u8 as char)` 经 UTF-8 编成 `c3 88`
两枚——字节形之后 `char` 这条通道整个退出。③ `%%` 的展开发生在**整条格式串**上（C 把整个 `format`
交给 `printf`，`:306`、`:308`），转换前后两段都要展开；只展开后缀的那一版把 `-p 2 -w '%%x%d' 3 1 5`
打成 `%%x1`，参照件打 `%x1`。③这一形是 A/B 逮出来的，而**第一版负控在三层门里全绿**：门里当时没有它。
判据仍是批次四百六十一那条——**一整段全绿先怀疑覆盖面有洞**——本轮是它第三次兑现，且这次洞在
「门与参照件之间」而不是「注入与门之间」：A/B 有料，门没接住。

**两处判据与一处报文缺口也在同一射程里**：④ 空操作数在 C 是 `argv[i][-1]` 的**越界读**（值取决于那块
内存前面是什么），本层不复制未定义行为：`fn operand_to_number` 改交回 `Option<f64>`，空串那一格由
调用方留缺省——按宿主参照件的现量定形（`jot 3 '' 5` → `1 3 5`）。本层在这一行原本是 `s.len() - 1`
的减法下溢、**当场 panic**（实测退 134 并留 core dumped），所以这一处同时收掉一类崩溃；登记
`JOT-EMPTY-OPERAND-READS-OOB`，把「不复制 UB、按现量留缺省」这个选择本身记进账。⑤ 空 reps 在 C 是
合法的 0：`strtoul` 一个数字也没转时把 `ep` 留在起点，于是 `*ep == '\0'` 成立，那句 Bad reps 不触发，
`jot '' 1 2` 落 `:272-275` 的 `Infinite sequences cannot be bounded`；本层此前按「digits 为空即坏值」
判，报的是空壳一句 `Bad reps value:  `。这与 `parse_precision` 里那条「只有整串本来就空才算合法零」
同理，本轮把 reps 对齐过去。⑥ `unknown or invalid format \`%s'`（`:385`、`:396`）在 C 带的是整个
format 词，本壳那句根本没有值位，所以 `JotError` 拆成 `InvalidFormat(Vec<u8>)`（带词）与
`FormatWantsString`（`:376-377` 那句 C 本来不带值）——登记 `JOT-INVALID-FORMAT-ECHO-MISSING`，本轮按
C 落定。附带一条窄判据：`isspace` 只认六枚字节，`c2 a0` 前置的 reps 在 C 不是空白，参照件报
`Bad reps value:   3`，而 `str::trim_start` 会把它跳掉并打出 `2 3 4`（改前的件实测如此）——**缝改成
字节在这里是收窄，不是放宽**。

**门账（先对测试名集合再对总数）**：库内 `jot` 模块 **23 → 24**（新立
`the_word_the_separator_and_the_operand_bytes_all_reach_the_output`，十一段：词槽、分隔符、前缀字面段、
`%c` 一枚、`%%` 前缀展开、说明符校验失败的整词回显、尾字节带符号、空操作数、空 reps、非 ASCII 空白、
缺值字母），`jot_face` **16 → 18**（`a_raw_byte_in_the_word_or_the_separator_is_printed_as_that_byte`
五段、`the_operand_and_the_word_echoed_into_a_sentence_are_the_argv_bytes` 六段），跨壳表十五行 →
**十六行**（`jot` 行钉 `jot: unknown option -- \xff` 与退 **1**；这一支的 usage 只有**一行**且程序名跟着
`getprogname()`，`jot.c:314-319`），整包 69 段 **1261 → 1264 passed / 0 failed**。夹具侧两条新形状：
`fn argv` 交回 `Vec<&'static [u8]>`（结果要留着断言的形必须先把词表 `let` 出来，本轮拆 6 形），
以及新立的 `fn words`——**长度不齐的数组字面量在 `vec![b"1", b"18"]` 里会混型**，渲染半交回的条目
期望只能走助手。

**负控六枚各咬 3/2/3/1/2/2**（`tmp/jot-464/negctl.py`，逐枚读数 `negctl-K1..K6.log`；基线三档全绿：
库内 624、跨壳表 4、`jot_face` 18，命令带 `--no-fail-fast`）。K1（壳侧 argv 退回文本缝）咬面门两枚 +
跨壳表而**库内 624 全绿**——「库门对 argv 缝没有牙」第六次量化；K2（`-s` 的值折一次）咬库门 + 输出面门；
K3（`-b`/`-w` 词槽折一次）咬库门 + 输出面门 + 报文面门；K4（三支回显折一次）**只**咬报文那一枚——
K3 与 K4 各红各的，就是「值进输出」与「值进报文」两条腿分开的证据；K5（撤掉 signed char）咬库门与面门的
高字节段；K6（撤掉前缀 `%%` 展开）第一遍三层全绿，补完两段门之后咬 2 枚。每枚前后按整文件 md5 与
`tmp/jot-464/post/`（完整相对路径）成对复核，跑完核 `git status --porcelain` 只剩本轮四件。

**A/B 对 `tmp/jot-464/jot` 五十一形 SAME=49 / DIFF=0**（`tmp/jot-464/ab.log`；字母位五形、缺值两形、
回显四形、词槽与分隔符与说明符十余形、操作数八形含高字节与空串、legacy 与出口形状若干）。两形按类分开
计数：`-w'%%<ff>'` 与 `jot 3 - 5` 都落在 `prec` 保持 `-1` 那一族（C 在 `:354` 拼出 `%.-1f`，宿主 glibc
打回字面量而 Minix libc 读成 `%-1.0f`，本机不可判定，在册 `JOT-PREC-MINUS-ONE-FORMAT`）——**同两形各补
了一枚带 `-p` 的可对照版本**（`-p 2 -w'%%x%d'`、`-p 0 3 - 5`），要钉的性质照旧钉住，不可比的不再混进
读数。上一笔那条判据这次是先做的：**五十一形里退 0/0 的有 34 形**，先数过再谈全等，免得「两侧都报错」
的同形空跑冒充覆盖面。

**三条工具自错，同属「没跑被当成读数」这一族**：① 一轮 `rustfmt` 因为 `PATH` 里没有 cargo 的 bin 目录
根本没执行，而下游 `grep -c '^Diff in'` 交出 0——上一笔刚记过这条，本笔又踩；② 注入脚本在 `run_tests`
里抛异常（还是 cargo 不在 PATH）时**没走到还原**，把注入态留在工作树，下一次跑读到的锚点自然是 0 次，
看起来像「注入没落地」而实际是「上一枚没还原」——改成 `try/finally` 之后才拿到 K3/K6 的真读数；判据：
**注入类工具必须无论成败都还原**；③ 台账里我先写下了一句「参照件对超长 `-w` 报 `-w word too long`」，
回头实测两侧**完全同形**（`-p 0 -w <1100 枚 a> 2`：每行 1101 字节、总 2204 字节），一条没量过的断言被
自己的命令证伪——登记号改成立在真机上待量的 `JOT-WORD-TOOLONG-ON-MACHINE`；判据：**台账里每句
「参照件现量」都要对应一条跑过的命令与它的输出**，只从 C 源码读出来的只能写成「按源码应如此、未量」。

**五道收尾门逐条读数**（`2026-10-29`，码笔 `66864d1f1`、文档笔 `fa22969db`）：① `bash tools/todo-reconcile-scan.sh`
→ 13 项判据里 **1 项 DRIFT**（`探针命名族（行）` 现量 32 vs 台账基线 18），本轮四件全在
`os/commands/usr-bin/textfilter/`，与该项取数域（`grep -rnE "nk4[ac]:" os --include=*.rs` 的 32 处全在
`os/kernel`、`os/arch`、`os/libs`、`os/servers`、`os/fs`）无交集，且 32 这个数与上一笔相同 ⇒ 非本轮引入，
归一线在制；② `python3 tools/notes-link-check.py rewrite-notes/` → 764 个 md、**断链 131**（与上一笔同数），
本轮新增行落在 `todo.md:122..259`、`07-text-filter.md:227`、板 `:27`，逐行核对该清单在这三处**零命中**
（板那一条在 `:35`，非本轮）⇒ 新增断链键 0；③ `bash tools/todo-staleness-check.sh …TODO-LEDGER-OPEN.md` →
**PASS（0 项）**；④ `bash tools/doc-style-lint.sh --diff HEAD~2..HEAD` → 3 个文件 / 139 个新增行、
**error 级 0**；⑤ `bash tools/check-staged-inflight.sh` → 退 0，避让清单 1 条
（`TODO-3ARCH-PARITY-20261006.md`）未触碰。

**门读数补全**：宿主整包 `cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` →
**69 段 / 1264 / 0**（`tmp/jot-464/pkg-2.log`~`pkg-4.log` 三次同读数，最后一次在负控还原之后）；容器权威门
按 §7.3 三挂载配方（`-u $(id -u):$(id -g)` + `ci-cargo`/`ci-target`，跑时未与其他 cargo 并发）→ 退 0、
**69 段 / 1264 / 0**（`tmp/jot-464/container-gate-final.log`；`%%` 修复之前那一次也是 1264/0，两次门数
相同——本轮加的两段都在既有门内）；三架构 `cargo check --offline -j 1 --target {x86_64-unknown-none,
aarch64-unknown-none, riscv64gc-unknown-none-elf} -p minix-textfilter --bins` 各退 0、错误 0；
`rustfmt --edition 2024 --check` 四件终态零漂移（格式化前 `src/jot.rs` 9 处、`jot_face.rs` 4 处，
HEAD 同件都报 0 ⇒ 全是本轮自造）；`cargo clippy --offline -j 1 --all-targets -p minix-textfilter` 主树与本树
各 **149** 条、位置各 80，集合差是同四条警在 `jot.rs` 的 441/630/642/702 → 487/704/716/776 **位移**
（`width` 未读、`dox`/`exhausted` 死字段、`match` 可简化、`next` 与迭代器同名），零新警；A/B 用的
`os/target/debug/jot` 是显式 `cargo build --bins` 之后 `file` 过的合法 ELF。

**进度与在册（全部现量）**：参数缝半已落 **十九支**（`rev`、`paste`、`yes`、`tee`、`seq`、`tr`、`colrm`、
`fold`、`col`、`uniq`、`units`、`cksum`、`cmp`、`column`、`comm`、`cut`、`head`、`join`、`jot`）；
`grep -rl "support::args()" os/commands/usr-bin/textfilter/src/bin/ | wc -l` → **20**；命令面口径 → **89**；
渲染半那条 grep 仍 **0** 行；`line_bytes` **40 处 / 29 件**（本支未新增调用点，用的是批次三百一十七起就
存在的那一处）。余下 20 支里带字母位诊断面的有 **8** 支（`nl`、`pr`、`sort`、`split`、`tail`、`unexpand`、
`unifdef`、`vis`），**切片 47 建议 `nl`**：字母序第一支，`nl.c` 的 `-b`/`-f`/`-h`/`-p`/`-v` 都带值，而
分隔符与行号格式两支都直接进输出行——形状上是 `jot` 与 `cut` 的合体；参照件用前照例先打一枚 `-Z`。
**两笔插轮建议**：①收 `JOIN-STDIN-LEG-BYPASSES-DUAL-SURFACE`（上一笔登记的读缝，一条腿一枚门一次差分）；
②把 `JOT-EMPTY-OPERAND-READS-OOB`、`JOT-PREC-MINUS-ONE-FORMAT`、`JOT-WORD-TOOLONG-ON-MACHINE` 一起拿到
真机上量一次（`JOT-PREC-RANDOM-AB-ON-MACHINE` 已在册）。独立轮在册不碰：`ARGV-BYTE-PATH-IN-LIBS`、
`CKSUM-CHECK-LEG-TEXTSEAM`、`UNITS-DASH-REWRITE-AT-FIRST-CHAR`、`UNITS-PARTIAL-UNIT-ON-ERROR`、
`UNITS-PATH-SEARCH`、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`、五枚 `*-NON-PERMUTE-OPERAND-TAIL`、
`CUT-SUPPRESS-STILL-PRINTS-NEWLINE`、`CUT-EMPTY-BYTE-LIST-RUNAWAY`、`COL-CLIPPY-STOCK-LINT`、
`BIN_SUPPORT-FMT-DRIFT-STOCK`、`REV-MULTIBYTE-ERROR-LEG`、`NL-DELIM-LENGTH-RULE`、
`HEAD-NEGATIVE-COUNT-UNSIGNED-GUARD`、`HEAD-COUNT-LEADING-BLANK-ACCEPTED`、
`JOIN-STDIN-LEG-BYPASSES-DUAL-SURFACE`，**本轮新立** `JOT-EMPTY-OPERAND-READS-OOB` 与
`JOT-WORD-TOOLONG-ON-MACHINE`（`JOT-INVALID-FORMAT-ECHO-MISSING` 本轮已闭合，不留开放号）；
`todo-reconcile-scan` 的 `探针命名族` 漂移归一线。**取证回抄照办**：release 前把 `tmp/jot-464` 整目录
复制回主树 `tmp/`。

**合回读数（第一百七十八笔补记，批次四百六十四）**：本支四笔（码笔 `66864d1f1`、文档笔 `fa22969db`、里程碑笔
`502b724f1`、合入笔 `449af843a`）在合回前把 `rewrite` 那六笔 01TODO-B3/B4 收进来，冲突只有一处——`new_edge4.md`
的同一处尾追，按两侧各留全解（本笔的 `###` 段在前、那两条流水行在后）。**逐笔验祖先**：四个哈希对 `rewrite` 全部
`merge-base --is-ancestor` 为真；主树 `rewrite` 从 `95944784c` 前进到 `449af843a` 是 fast-forward，主树脏文件与入栈
八件零交集（`git status --porcelain` 的词表与 `git diff --name-only rewrite...claim/*` 取集合差）。主树合回后当场复跑
`cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` → 退 0、**69 段 / 1264 / 0**
（`tmp/jot-464-main-recheck.log`），与领取树合回前后两次同读数；取证目录 `tmp/jot-464`（37 项，含 `head/`、`post/`
两份快照与六枚负控日志）已回抄主树。门④本支三笔的 `HEAD~3..HEAD` 复跑是 4 个文件 / 257 个新增行、**error 级 0**。
门②那条「不同树不可比」的口径本笔再核一次：同一条命令在领取树是 764 个 md / 断链 131，在主树是 2010 个 md / 222，
差的那一千多个文件是别的线未入库的工作笔记，**做集合差必须在同一棵树里取基线**。**码笔信息的一处错字与为什么不走 `--amend`（同一处末尾追加，原正文不改）**：`66864d1f1` 的提交信息把参照件写成
`tmp/jot-464/join`，正确是 `tmp/jot-464/jot`（差分日志 `tmp/jot-464/ab.log` 与脚本 `ab.py` 里的 `REF` 都是后者，读数不受影响）。
不做历史改写的理由有两条，都可在当场量到：① 那笔之后已有 5 笔，其中 `80a081de6`、`d92713b59` 是 01TODO-B5 那条线的合并笔与
里程碑回写，改一个信息就要连带重写**他线**的提交；② 台账已把 `66864d1f1` 当锚点引用 3 次（板 A2 行一处、本文件两处），换哈希
等于让这 3 处指向不存在的东西——**代价远大于收益**。判据：**已进共享分支且被他线合在下游的提交，错字用追加更正收口，不用改写
历史**；`--amend` 只在这笔就是树顶、且没有任何地方引用它的哈希时才成立（上一笔那条补正正是这种情形：它是 HEAD 且只改信息）。

- `2026-10-29` zcode-glm / 01 篇 todo 第三批（`claim/01TODO-B3-zcode-glm` → 合并笔 `23b3e3267`）：T4/D-68 闭合——`bootface_a64` 删 `fill_memmap`/`clip_firmware`/`MEMMAP_STORAGE`/`MEMMAP_MAX`，改调 `minix_boot::DtbMemoryMap::new(&fdt, 0).collect()`（第四链并入 I-22 单源；`fw_end=0` 形态前提成文）；内核 Phase A.2 新增 Step 4：遍历 `platform_sources` 的 DTB 源，`dtb_blob_len` 从 8 字节头取真实 totalsize 后 `cut_memmap` 永久扣除（riscv 已削界、UEFI reserved 区均 no-op）。宿主测试用真实 QEMU blob（7699B @ 0x4000_0000，/memory=512MiB）断言切除后自由段 `[0x4000_2000, +512MiB-0x2000)`。**门读数**：`MEMMAP_STORAGE|clip_firmware|fill_memmap` → 0；aarch64 kernel-image（SD-23 RUSTFLAGS）0 error、x86/riscv kernel-image 各 0 error；docker kernel mock 843/0/8（memmap 16→17）；`test-smp-aps-aarch64.sh` 直核全链 PASS（ap-arrived=1，终版代码重跑）；doc-lint 0。**在册**：T1/T5/T3/T2 与 N-4/N-5 未动。

- `2026-10-29` zcode-glm / 01 篇 todo 第四批（`claim/01TODO-B4-zcode-glm` → 合并笔 `39fd630ea`）：N-4 重复面收敛（D-68 解锁后项）——`NAME_POOL`/`NAME_SLOT`、名字池与 `MODULE_NAMES` 的编译期对账、`be_cell`/`fdt_table_pa`/`in_memmap`/`in_memmap_range`/`table_magic_ok` 下沉新模块 `kernel-image/src/bootface_common.rs`；`say`/`say_hex`、`place_modules`、布局常量组留两份并互指（架构真差异/本地胶水）。**偏离**：工单原写「能下沉 boot-shim 库就下沉」，实际落在 kernel-image 包内共享模块（这批 helper 是零堆/物址视图专用，进 boot-shim 公共面会为单消费者扩边界）。**门读数**：riscv64/aarch64 kernel-image check（SD-23 RUSTFLAGS）各 0 error、无 bootface 告警；`test-smp-aps-aarch64`（ap-arrived=1）与 `test-kernel-image-riscv64-bootface` 双 PASS。**在册**：N-5 独立探针批次；T1/T5/T3/T2/D-59/D-72/D-73 与 §5 文档批未动。

- `2026-10-29` zcode-glm / 01 篇 todo 第五批（`claim/01TODO-B5-zcode-glm` → 合并笔 `80a081de6`）：**回归修复**——B2 的 TODO-01 在 `arch_boot_impl` 入口加过 `cfg(not(mock))` 的 M4 运行期断言；B4 后复跑 run_all 暴露 **25 项红**（hello-boot / test-protection / test-shutdown-aarch64 等），根因是 QEMU 测试内核**直接调用 `arch_boot_impl`** 分阶段验证、不经 `store_kernel_info`，生产形态（非 mock）下断言必触发。修法：撤该断言，M4 自动证据回到四入口源码序钉（`store_before_alloc_pin.rs`），钉注释与 01 篇 §5.1/§5.3 写明「运行期钩无法区分生产入口与分阶段测试内核」的判据。**门读数**：kernel mock 843/0/8、pin 1/0；领取树复跑 run_all → **39/0/0**（`tmp/qemu-runall-b5.log`），主树合并后即此读数。**教训**：新增「每次启动都执行」的运行期断言，必须同时跑三架构真机门——宿主 mock 与单架构冒烟都看不到测试内核的分阶段调用面。

### 第一百七十九笔（批次四百六十五 · A2 参数缝半切片 47：`nl` 的分页分隔符、行号分隔串与七句值回显交回原字节；参照件在 `-d` 空值上读的是内存里排在后面的那个参数）

**这一笔先说形状**：`minix3/usr.bin/nl/nl.c:149` 的 optstring 是 `"pb:d:f:h:i:l:n:s:v:w:"`——**十一个
字母、十个带值、只有 `p` 不带值**。板上两处预估（「九字母全带值」与「`-b`/`-f`/`-h`/`-p`/`-v` 带值」）
按现量一并订正。这一支的 argv 字节有六个去处，密度是本族最高的：① `-d` 的两枚字节既是**判据**
（`nl.c:158-166`：第一枚、第二枚各一个 `if`，第三枚才落 `invalid delim argument`）又是**匹配物**
（`:289-304` 把它们依次比到行的第 0、2、4 个字节上，对应 `footer`/`body`/`header` 三段；整行正好等于
分隔符时 `:294` 成立、`:299` 的 `goto nextline` 把那一行**吃掉**——分页符行不输出，而 `section` 落进
`footer`，它的缺省类型是 `number_none`（`:73`），所以随后那行交回「六枚空格 + 制表符 + 正文」）；
② `-s` 的值由 `:340` 的 `printf("%s%s", sep, buffer)` 打在**每一行**的行号与正文之间，且整个替换掉
缺省的那个制表符；③ `-b`/`-f`/`-h` 的类型串只看首字符（`parse_numbering` 在 `:357`，
`switch (argstr[0])` 在 `:362`），非法那句带整串与各自段名（`:393`，名字在 `:73-75`）；④ `-n` 的
三个格式名是三支 `strcmp`（`:195-199`），比的就是字节串；⑤ `-i`/`-l`/`-v`/`-w` 四个数走
`strtol`/`strtoul` 的 `*ep != '\0'`（末尾剩任何一枚字节都算错），四句回显在
`:182`/`:191`/`:214`/`:223`，`-w` 还多一句把转出来的数打回去（`:225-228`）；⑥ 操作数至多一枚，
`-` 走标准输入（`:243-244` 那个 `strcmp` 短路掉了 `freopen`）。**本轮库侧零改动**——
`src/nl.rs` 早就是 `NlOptions.delim: [u8; 2]`、`sep: Vec<u8>`、`nl_lines(&[&[u8]], …)`，六个去处
全在壳里，所以「库内 `nl` 的 14 枚未动」是结构而不是漏测。壳内 `from_utf8_lossy` 现量 **2 处**
（程序名与档名），受 `ARGV-BYTE-PATH-IN-LIBS` 那两条 `&str` 口牵制。

**本轮最值钱的读数不是门，是参照件自己**：`-d` 给空串时，C 那三条 `if` 读的是 `optarg[1]` 与
`optarg[2]`，而空串只有一枚终止符——两处都在终止符之外。取证办法是把「内存里排在后面的那段」直接当
夹具用：`nl_ref -d "" f`（档名 `f` 的内容是一行 `\f` 加一行 `q`）与 `nl_ref -d '\f' f` **逐字节全等**，
而与显式默认的 `nl_ref -d '\:' f` 不同形，也就是**第二枚分隔符来自下一个参数的首字母**；换成两字符
以上的档名（`body.txt`、`zone.txt`）`optarg[2]` 非零 ⇒ 当场 `invalid delim argument` 退 1；后面没有
任何参数时（读标准输入）照拒，`env -i` 把环境清空也照拒。同一条命令行只换档名首字母就翻面，这不可
复现也不该照抄，所以本层按那三条 `if` 的**意图**实现：空值两个条件都不成立 ⇒ 不动默认，长度判据只看
值本身的字节数。`NL-DELIM-LENGTH-RULE` 现覆盖两形（`-d\xff` 与 `-d ""`）。这条同时给「真值优先序：
Minix3 C 源码行为 > 设计契约 > Rust 实现」划出一道边界——**参照件的读数并非都来自源码的意图**，
越界读产出的那一形要写成在册待裁而不是照抄；三形对照与命令都在
`tmp/nl-465/kd2-empty-value-oob.log` 里，判读段落款在册号。

**A/B 又逮出两条此前没登记的分叉，均登记不执行**：① `-f`/`-h` **缺值**时本壳报的字母是 `b`——那三支
共用一个 `value!('b')` 宏实例、字母写死了，而 libc 报的是各自的字母（`getopt.c:134`）；两侧同退 1、
只差那一枚字母，正是「两侧都报错」里最容易滑过去的一形，登记 `NL-MISSING-VALUE-LETTER-HARD-CODED`
（一处单词级的修法）。② 附值形（`-s x`、`-b a` 这种值紧跟字母的写法）本壳跳掉前导空白，libc **不跳**：
`getopt.c:118` 是 `optarg = __UNCONST(place)`，整段剩余直接交回；现量 `nl -s' x'` 本壳打 `     1xa`、
参照件打 `     1 xa`，`nl -b' a'` 本壳接受、参照件报 `illegal body line numbering type --  a`。登记
`NL-ATTACHED-VALUE-LEADING-BLANK-STRIPPED`。②要先裁一件事才算可修：照抄 libc 的「不跳空白」还是保留
现形——那是判据裁决，不属本轮射程，所以本轮只把那处 `str::trim_start_matches` 换成等价的字节循环，
保持既有行为。**一次只修一条在本笔是可查证的**：这两条都没顺手改，只落登记。

**负控五枚（`tmp/nl-465/negctl.py`，逐枚读数 `negctl-L1..L5.log`）各咬 3/1/1/2/2**：L1（壳侧 argv 退回
`args()` 的 lossy 取法）咬面门两枚 + 跨壳表；L2（`-s` 的值折一次）只咬输出那枚；L3（七句回显折一次）
只咬报文那枚——**「值进输出」与「值进报文」两条腿各红各的，`jot` 那轮的形状在此复现**；L4（撤掉
「第三个字节才报错」）咬既有门 `delim_takes_two_characters_and_refuses_a_third` 加本轮新门；L5（字母位
退回按字符取）咬跨壳表加 `the_libc_line_of_a_multibyte_flag_is_one_byte_for_nl`。**五枚都出现「库内 624
全绿」，而这一轮它不是覆盖面有洞**：`nl` 的选项环整个住在薄壳里，库侧只吃拆好的 `NlOptions`，那一层
根本没有这条腿。所以「一整段全绿先怀疑覆盖面有洞」这条判据本笔补一句：**先问这条腿住在哪一层，
再决定补门还是接受结构**。前面六次量到的「库门对 argv 缝没有牙」是同一件事的另一面。

**两起工具自错，同属「计数不是读数」这一族的新形态**：① 为了强制重编跑了一句
`touch …/src/bin/cat.rs`，而这个包里**没有** `cat` 这枚 bin——`touch` 把它创建成一枚 0 字节的散落文件，
于是整包 `cargo test` 编译失败退 101、`grep -c '^test result:'` 交出 **0 段**。判据：拿 `touch` 当重编
探针之前先确认目标存在；任何「段数为 0」先 `git status` 看有没有本轮自造的散落文件（这枚是未跟踪的
`??`，一眼可辨，删掉即恢复 69 段 / 1266 / 0）。② 想量 HEAD 同件的 `rustfmt --check` 漂移时，把 HEAD 的
blob 拷到 `/tmp` 单独跑，rustfmt 解析不到 `#[path]` 带进来的兄弟模块（`../bin_support.rs`、
`./face_support.rs`），交回的是**解析报错而不是 diff**，`wc -l` 把它数成了「HEAD 的 `src/bin/nl.rs`
漂移 7 行」这条根本不存在的断言。改成原地注入/还原（`tmp/nl-465/fmt-baseline.py`：退出码不属于 0/1
即判「没跑成」直接抛异常，`finally` 还原后逐件核 md5）之后，HEAD 与现件两侧都是 **0**。判据：
**格式门必须在原地量**（模块解析依赖目录布局），并按 rustfmt 的退出码分档——0 无漂移、1 有漂移、
其余没跑成。

**八处错锚点按现量订正**，其中两处在 HEAD 里就错着、正落在本轮改的那几行上：`parse_numbering` 此前写成
`:340-…`，而 `:340` 其实是 `printf("%s%s", sep, buffer)` 那行（函数 `:357`、`switch` `:362`）；操作数那段
把 C 写成 `fopen` 加一个「后续都读这个 `fp`」，实际是 `freopen(argv[0], "r", stdin)`（`:243-245`）而整条
流水线只有 `fgets(..., stdin)`（`:288`）一个读缝——这也正是「档名内容与标准输入走同一条编号腿」的机制
出处；七句值回显此前写成六个范围，实际各在 `:165`/`:182`/`:191`/`:203`/`:214`/`:223`/`:393`；`-n` 的三支
`strcmp` 写成 `:193-200`、实际 `:195-199`；`-w` 那句写成 `:224-228`、实际判据 `:225` 句子 `:226-228`；
附值形那处挂到 `getopt.c:117-118`，该句是 `:118`。

**五道收尾门逐条读数**（`2026-10-29`，码笔 `52cbcb3c5`、文档笔 `05727e9ce`）：①
`bash tools/todo-reconcile-scan.sh` → 13 项判据里 **1 项 DRIFT**（`探针命名族（行）` 现量 32 vs 台账基线
18），该项取数域是 `grep -rnE "nk4[ac]:" os --include=*.rs`，32 处全在 `os/fs/pfs`、`os/fs/mfs`、
`os/kernel` 等，本轮改动三件各 **0** 命中，且同一条命令在 HEAD 上也是 32 ⇒ 非本轮引入，归一线在制；
② `python3 tools/notes-link-check.py rewrite-notes/` → 764 个 md、**断链 131**（与上一笔同数），本轮新增行
落在 `todo.md:122..265`、`07-text-filter.md:6`、板 `:27`，清单在这三处**零命中**（`todo.md` 那五条在
`:3585`、`:3641`、`:3644`、`:3645` 等旧行，板那一条在 `:35`）⇒ 新增断链 0；
③ `bash tools/todo-staleness-check.sh rewrite-notes/coordination/TODO-LEDGER-OPEN.md` → **PASS（0 项）**；
④ `bash tools/doc-style-lint.sh --diff` → 3 个文件 / 146 个新增行、**error 级 0**——首跑抓到 **1 条真错**
板那行有一句把「取证」包进全角括号，撞 SL-11「过程标记入正文」那条禁令，改成行文里的逗号写法后复跑归零——
门是有效的；
⑤ `bash tools/check-staged-inflight.sh` → 退 0，避让清单 1 条（`TODO-3ARCH-PARITY-20261006.md`）未触碰。

**门读数补全**：宿主整包 `cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` →
**69 段 / 1266 / 0**，同一读数在本轮三个时点各跑一次（负控与差分之后 `tmp/nl-465/pkg-post.log`、我自己
格式化之后 `pkg-post-rustfmt.log`、锚点订正后的最终态 `pkg-final.log`）；容器权威门按三挂载配方
（`-u $(id -u):$(id -g)` + `ci-cargo`/`ci-target`，跑时未与其他 cargo 并发）两次，最终态
`container-final.log` 退 0、**69 段 / 1266 / 0**，与宿主同读数；三架构 `cargo check --offline -j 1
--target {x86_64-unknown-none, aarch64-unknown-none, riscv64gc-unknown-none-elf} -p minix-textfilter
--bins` 各退 0、错误 0（最终态三件 `arch-final-*.log`）；`rustfmt --edition 2024 --check` 三件原地零漂移
（中途我自己写出过 33 行与 44 行的漂移，格式化后归零；HEAD 同件原地量也是 0 ⇒ 无存量牵连）；
`cargo clippy --offline -j 1 --all-targets -p minix-textfilter` 退 0、共 **149** 条，**改动三件上 0 条**，
唯一挂到 `bin "nl"` 的那条源在共享层 `src/bin/../bin_support.rs:688` 的 `unused import: std::io::Read`，
本轮没改那一层、未改过的 `head` 同样出 ⇒ 存量；库测数按文档那句的**同一条命令**现量
`cargo test --offline -j 1 -p minix-textfilter --lib` → 624 / 0；A/B 用的 `os/target/debug/nl` 由负控还原
之后重编，与 `tmp/nl-465/post/` 三件快照逐件核过 md5。
**A/B 对 `tmp/nl-465/nl`（软链 `tmp/nl-417/nl_ref`，用前照例打 `-Z`：交回
`nl: unknown option -- Z` 加那两行 `Usage:`）跑，矩阵四十三形，SAME=36（其中退 0 的 13 形先数过）/
非预期 DIFF=0 / 已知分叉另计 7 形**（`tmp/nl-465/ab.log`）。「先数退 0 形数」这条判据本笔又用上一次，
并暴露初版矩阵的一处空洞：那条写着「裸 `-` 是档名」的形其实测的是字母位（`-` 紧跟一枚裸 `ff` 被当成
选项簇），与第二形**完全重复**——换成三条真的走操作数那条腿的形：单个 `-` 走标准输入、档名内容与标准
输入不同时谁进流水线、开不了的 ASCII 档名那一句。中间那形正是 `NL-OPERAND-CONTENT-DROPPED` 那一坑的
守门形（标准输入放 `not-from-stdin`、档名放 `a\nb\n`，输出必须是档名内容，两侧全等）。四类已知分叉
各计：`ARGV-BYTE-PATH-IN-LIBS` 1 形、`NL-DELIM-LENGTH-RULE` 2 形、
`NL-MISSING-VALUE-LETTER-HARD-CODED` 2 形、`NL-ATTACHED-VALUE-LEADING-BLANK-STRIPPED` 2 形。
**门账**：`nl_face` **14 → 16**（`a_raw_byte_delimiter_matches_the_line_and_a_raw_byte_separator_is_printed`
五段、`the_seven_value_echoes_carry_the_argv_bytes` 七段 + 段名两支 + 反面形），跨壳表 **十六行 → 十七行**
（`nl` 行钉 `nl: unknown option -- \xff` 与退 1；这一支的 usage 是两行、第一行行尾 `\n\t`，`nl.c:400-405`），
库内 `nl` 的 14 枚未动，整包 69 段 **1264 → 1266**。两枚新门的期望值全部按实测写：第一版四处是推理，
实测否证三处（那一格是六枚空格不是八枚、`-s` 的值紧跟行号而无多余空格、`-b a<ff>` 是接受的），
第四处是我把夹具写错——`nl` 唯一的操作数是档名。

**进度与在册（全部现量）**：参数缝半已落 **二十支**（`rev`、`paste`、`yes`、`tee`、`seq`、`tr`、`colrm`、
`fold`、`col`、`uniq`、`units`、`cksum`、`cmp`、`column`、`comm`、`cut`、`head`、`join`、`jot`、`nl`）；
`grep -rl "support::args()" os/commands/usr-bin/textfilter/src/bin/ | wc -l` → **19**；命令面口径 →
**88**；渲染半那条 grep 仍 **0** 行；`line_bytes` **40 处 / 29 件**（`nl` 本来就在其中，本支未新增调用点）。
余下 19 支里带字母位诊断面的有 **7** 支（`pr`、`sort`、`split`、`tail`、`unexpand`、`unifdef`、`vis`；
判据是本壳或它调用的库模块交回 `GetoptReport`——`split`/`tail`/`unexpand`/`vis` 在壳里、
`pr`/`sort`/`unifdef` 在库模块里），**切片 48 建议 `pr`**：字母序第一支，带值字母多且值既进报文也进排版
宽度；参照件用前照例先打一枚 `-Z`。**三笔插轮建议**：①收 `JOIN-STDIN-LEG-BYPASSES-DUAL-SURFACE`；
②把 `JOT-EMPTY-OPERAND-READS-OOB`、`JOT-PREC-MINUS-ONE-FORMAT`、`JOT-WORD-TOOLONG-ON-MACHINE` 一起拿到
真机上量一次；③`NL-MISSING-VALUE-LETTER-HARD-CODED` 与 `NL-ATTACHED-VALUE-LEADING-BLANK-STRIPPED` 都是
单词级修法，但后者要先裁判据。独立轮在册不碰：`ARGV-BYTE-PATH-IN-LIBS`、`CKSUM-CHECK-LEG-TEXTSEAM`、
`UNITS-DASH-REWRITE-AT-FIRST-CHAR`、`UNITS-PARTIAL-UNIT-ON-ERROR`、`UNITS-PATH-SEARCH`、
`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`、五枚 `*-NON-PERMUTE-OPERAND-TAIL`、
`CUT-SUPPRESS-STILL-PRINTS-NEWLINE`、`CUT-EMPTY-BYTE-LIST-RUNAWAY`、`COL-CLIPPY-STOCK-LINT`、
`BIN_SUPPORT-FMT-DRIFT-STOCK`、`REV-MULTIBYTE-ERROR-LEG`、`NL-DELIM-LENGTH-RULE`（含本笔补的空值那一形）、
`NL-NON-PERMUTE-OPERAND-TAIL`、`NL-SIGNED-COUNTERS`、`HEAD-NEGATIVE-COUNT-UNSIGNED-GUARD`、
`HEAD-COUNT-LEADING-BLANK-ACCEPTED`、`JOIN-STDIN-LEG-BYPASSES-DUAL-SURFACE`、
`JOT-EMPTY-OPERAND-READS-OOB`、`JOT-WORD-TOOLONG-ON-MACHINE`，**本笔新立**
`NL-MISSING-VALUE-LETTER-HARD-CODED` 与 `NL-ATTACHED-VALUE-LEADING-BLANK-STRIPPED`；
`todo-reconcile-scan` 的 `探针命名族` 漂移归一线。**取证回抄照办**：release 前把 `tmp/nl-465` 整目录
复制回主树 `tmp/`。

**合回读数（第一百七十九笔补记，批次四百六十五）**：本支三笔（码笔 `52cbcb3c5`、文档笔 `05727e9ce`、
里程碑笔 `ccfc129b2`）合回时 **`rewrite` 自基线 `cf306a62e` 起没有新提交**——`git log --oneline
cf306a62e..rewrite` 交回空、`git merge-base rewrite HEAD` 就是 `cf306a62e`，于是这次是**纯快进、零冲突**：
上一笔那一处 `new_edge4.md` 尾追撞车（两侧各留全解）本笔没有复现，`release` 之前也不需要解任何东西。
判据照旧不许省：**快进之前先量两边是否真的分叉**，`cf306a62e..rewrite` 为空这句就是「没分叉」的读数，
而不是「我以为没人动」。快进前另核一次避让：主树 `git status --porcelain` 对本支要改的 7 个文件
（三件 `os/commands/usr-bin/textfilter/` 与四件 `rewrite-notes/`）交回**空**，其它线的在制内容全在
`prompt/`、`AI-chats/`、`rewrite-notes/01-stage-kernel/` 那一侧 ⇒ 快进不会覆盖他人在制文件。
逐笔验祖先（`git merge-base --is-ancestor <笔> rewrite`）三笔**全 ✓**，快进后 `git rev-parse rewrite`
与领取树顶同为 `ccfc129b2`。主树当场复跑 `cargo test --offline -j 1 --no-fail-fast -p minix-textfilter`
→ **69 段 / 1266 / 0**，其中 `nl_face` 16 枚、`bin_support_face` 4 枚（`tmp/nl-465-maintree.log`），
与领取树的最终态读数逐数相同。门②那条「不同树不可比」的口径本笔再核一次：同一条 `notes-link-check`
在领取树是 764 个 md / 断链 131，在主树（快进后）是 2010 个 md / 断链 **222**——主树多出的那一千多个
md 是 `minix3/`、`redesign-notes/` 等未纳入领取树视野的目录，所以两树的绝对数**不可比**，可比的是
**同树跨轮**：主树上一笔也是 2010 / 222 ⇒ 本笔在主树同样零新增断链。取证回抄已办：`tmp/nl-465`
整目录（32 件、900K）复制进主树 `tmp/`，关键四件 `ab.log`、`pkg-final.log`、`container-final.log`、
`kd2-empty-value-oob.log` 与领取树逐字节核过 md5 相同。

### 第一百八十笔（批次四百六十六 · A2 参数缝半切片 48：`pr` 的列分隔符、行号后缀、页眉标题与四句值回显交回原字节；同一枚字节在 `-e` 与 `-i` 两侧的符号性并不相同）

**这一笔先说形状**：`minix3/usr.bin/pr/pr.c:1655` 的 optstring 是 `"#adFfmrte?h:i?l:n?o:ps?T:w:"`
——除起始的 `#`，带值字母九个，`h`/`o`/`T`/`w` 必填（`:`）、`e`/`i`/`l`/`n`/`s` 可选（`?`）。
可选与必填的差别**只有在字节口径下才看得清**：`egetopt.c:206-215` 让可选值只取**附着在本词里**
的那一段，另起一词不会被取值而是掉进操作数——本轮当场踩过（`-e x` 之后那个词被当成档名，报的是
`pr: Cannot open x`），已写成测试注释。argv 的原字节有六个去处：① `-s` 的字节由 `vertcol`/
`horzcol` 打在列间（`pr.c:487`、`:745`），且 `-s` 存在时列间**不补空白**；② `-n` 的字节是
`nmchar`（`:95`），`addnum` 把它跟在行号后（`:215`）；③ `-h` 的值与**档名本身**由 `prhead` 的
`printf(HDFMT, buf+offst, fname, pagcnt)`（`pr.h:59`、`pr.c:1511`）打在页眉行，`%s` 不解读内容；
④ `-e`/`-i` 的字节是**比较物**（下一条）；⑤ 四句 `invalid value for -X %s` 回显 `eoptarg` 从游标
起的原字节（`:1726-1729`、`:1757-1760`、`:1785-1788`）；⑥ 档名还进 `Cannot open %s` 那一句
（`:1364-1370`）。字母位那一枚也不来自 libc：`pr` 链的是自带的 `minix3/usr.bin/pr/egetopt.c`，
`:110` 的 `eoptopt = (int)*place++` 一次进一个字节，而 `:148` 打的句子是
**`illegal option -- %c`**，与 `getopt.c:103` 的 `unknown option` 不同词——跨壳表因此第一次出现
一条与本族其它行不同句的读数（`pr: illegal option -- \xff`），拿「同族规律」抹平会把措辞写错。

**本轮的正菜是一条比较判据：同一枚字节，`-e` 与 `-i` 两侧符号性不一样**。`inln` 比的是
`int ch = getc(inf)`（字节的 0..=255）与 `int chk = (int)inchar`（`pr.c:1038`、`:1049`），而
`inchar` 声明为**带符号 `char`**（`:87`）——高位字节被提升成负数，于是**永远配不上任何输入字节**：
`pr -e<ff>4` 对输入 `x<ff>y` 在 C 那里是空操作，参照件现量 `78 ff 79`。`otln` 比的是
`*buf == ochar`（`:1165`），两边都是 `char`，同一枚高位字节在**输出侧照常命中**：
`pr -t -i<ff>4` 对 `a<ff><四空格>b` 现量 `61 ff ff 62`。本壳此前两支都按 `u8` 比，等于在 `-e`
那一侧接受了 C 永不接受的匹配；现在 `expand_tabs` 走 `i32::from(ch as i8)` 的探针，
`contract_tabs` 保持字节比并在注释里写明两侧为何不同。缺省那枚制表符（`0x09`）不受影响，
`-e4` 对真制表符仍展开成三枚空格——这一形同时是「缝改成字节既不放宽判据也不动默认」的控制形。
**`jot` 那轮量到「尾字节经 `char` 落地所以为负」，本支量到「同一个字节进比较时符号性随两侧类型
而分叉」，是同族判据的第二种形态**；而这条不对称不是推理：P5（抹掉 `-e` 的带符号落地）与
P6（把 `-i` 也改成带符号形）两枚注入**各红两层**，各自咬住一条腿。

**档名在本支出现了一种本族第一次的形状**：`pr` 的 `Cannot open` 走 `errf` 的
`fprintf("pr: Cannot open %s, %s\n", …)`（`pr.c:1364-1370`），不是 `err(3)`/`err_line` 那个冒号形
——所以本壳可以自己按字节拼那一句，名字位与参照件**逐字节同形**（A/B 里它是回归项而不是分叉）。
只有开档那一步仍折一次（`minix_sys::open` 的 `&str` 口，在册 `ARGV-BYTE-PATH-IN-LIBS`），后果是
「真实存在的裸字节档名本壳打不开」（参照件打得开，因为 `fname` 就是它 `open` 的那个指针），
这一形按 KD1 分开计数。而档名除了进报文还经 `prhead` 进**页眉标题**，所以操作数的原字节本来是
输出内容——这一条把「参数缝只影响诊断」的旧直觉彻底否掉。

**页眉行不可逐字节比，本笔因此立了一条新比法**：C 的 `HDFMT` 第一个 `%s` 装的是
`strftime("%b %e %H:%M %Y")` 的当下时刻（`pr.h:62`），本壳不渲染时间场（`-T` 未接、时钟未接），
所以带页眉的每一页第一行必然差那一段。差分脚本对这三形改用**后缀全等**：参照件那一行必须以
本壳那一行结尾（两侧共用 `{标题} Page {N}` 那一段）、行数与退码相同——**标题位的原字节仍被钉住，
摘出去的只有时间场那一族**，这不是放宽。口径写在 `tmp/pr-466/ab.py` 的文件注释里，并说明它适用
哪几形。

**A/B 又量出一条在册之外的分叉族，登记不执行**：`egetopt` 的 optstring 含 `#`，所以 `-`/`+` 后
紧跟数字时走 `egetopt.c:118-140`——它把数字串**就地用 NUL 截断**（`savec = *p; *place = '\0'`，
`:135-136`）交回 `delim`，下一次调用先恢复那个字节（`:81-83`）再从它继续扫**同一个词的尾巴**；
而 `:152-168` 那句 `illegal '+' delimiter with option` 只对 `+` 生效（文件头 `:60-61` 明写
`The - may be combined with other options, but the + cannot`）。本壳把带数字的词当整词处理，
六形现量两侧不同：`-2t` 参照件接受（两列 + 无页眉，退 0）而本壳报 `illegal option -- 2`；
`-2x` 与 `-2<ff>` 两侧都拒但**报的字母不同**（本壳报 `2`、参照件报尾巴那一枚）；`+3t`、`+2a`、
`+3<ff>` 本壳接受而参照件报错。这里面没有一枚字节涉及 UTF-8 折叠——**与参数缝是两条判据**，
登记 `PR-EGETOPT-NUMBER-TOKEN-CONTINUATION`，按「一次只修一条」本轮不动（它是选项环建模，
一个批次量级，且六形都已在矩阵里按 KD4 分开计数，不会被读成全等）。

**负控九枚（`tmp/pr-466/negctl.py`，逐枚读数 `negctl-P1..P9.log`）各咬 3/2/2/2/2/2/2/3/1，
并把上一笔那条判据反过来用了一次**：P1（壳侧 argv 退回文本缝）出现「库内全绿」，而其余八枚
都红到库门——因为 `pr` 的选项环整个住在 `fn parse_pr_args` 里，与上一支 `nl` 的「环住在壳里、
五枚全绿是结构」形状**相反**。P2（`-s` 的值折一次）、P3（`-e`/`-i` 回显折一次）、P4（`-s` 回显
折一次）、P7（标题位折一次）第一遍**只咬面门**：那四条决策住在库里却没有一条库门钉它们的字节位
——按「一整段全绿先怀疑覆盖面有洞」补出
`the_option_bytes_and_the_operand_bytes_stay_bytes_through_the_library`（六段）之后复跑，
四枚各红两层。P8（字母位退回按字符取）咬跨壳表 + `the_libc_line_of_a_multibyte_flag_is_one_byte_for_pr`
+ 库门三处；P9（报文名字位折一次）咬 `the_four_invalid_value_echoes_…` 的最后一形。
每枚前后按整文件 md5 与 `tmp/pr-466/post/` 成对复核，跑完 `git status` 只剩四件。

**三起当场自错，同属「读数要看清自己在测什么」**：① 第一版把 `od -An -c` 的**列距**当成字节，
给 `-s` 那形写出 `a\xff<空格>c` 的期望——实测那一行是 `a`、`ff`、`c` 三枚（`-s` 存在时列间不补
空白），断言改写成 `[b"a\xff", b"c\nb\xff", b"d\n"].concat()`；② 页眉那三行 usage 我在测试里用
**续行字面量**重写了一遍，正好吃掉后两行各自的十个前导空格，而 `pr_face.rs` 里 `USAGE` 常量的
注释早就写明这个陷阱——同一个文件里已经警告过的坑还会再踩，判据是**先查有无现成常量与它的警告**
再动手写；③ 在主树用相对路径重定向 `cargo` 输出，落到一个并不存在的目录而白跑一遍
（`cd os` 之后 `../tmp/…` 指向主树 `tmp/`，与领取树的 `tmp/pr-466` 不是同一个地方）。
另把一条**存量**登记入库：07 篇命令契约表里 `pr` 行在「`pgpause |= FIRSTPAGE`」那段代码文字里
带一枚未转义竖线（该行未转义 5、表头与分隔行 4），同篇 `uniq`/`head`/`tail`/`cmp` 四行同此——
逐行核过 HEAD 与现件同数（本轮插入未新增），登记 `TEXTFILTER-07-TABLE-UNESCAPED-PIPES`，属另一条射程。

**五道收尾门逐条读数**（`2026-10-29`，码笔 `003be029a`、文档笔 `878a9e10b`）：①
`bash tools/todo-reconcile-scan.sh` → 13 项判据里 **1 项 DRIFT**（`探针命名族（行）` 现量 32 vs
台账基线 18），该项取数域是 `grep -rnE "nk4[ac]:" os --include=*.rs`，本轮改动四件各 **0** 命中，
且同一条命令在 HEAD 也是 32 ⇒ 非本轮引入，归一线在制（与上一笔同一条件数）；
② `python3 tools/notes-link-check.py rewrite-notes/` → 764 个 md、**断链 131**（与上一笔同数），
本轮新增行落在 `todo.md:122..230`、`07-text-filter.md:6`、`07-text-filter.md:198`、板 `:27`，
逐行核对清单在这四处**零命中** ⇒ 新增断链 0；③ `bash tools/todo-staleness-check.sh
…TODO-LEDGER-OPEN.md` → **PASS（0 项）**；④ `bash tools/doc-style-lint.sh --diff` →
3 个文件 / 95 个新增行、**error 级 0**（本笔首跑即归零，上一笔那条 SL-11 教训已落到写法里）；
⑤ `bash tools/check-staged-inflight.sh` → 退 0，避让清单 1 条
（`TODO-3ARCH-PARITY-20261006.md`）未触碰。

**门读数补全**：宿主整包 `cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` →
**69 段 / 1270 / 0**（`tmp/pr-466/pkg-post.log`、格式化后 `pkg-post-rustfmt.log` 两次同数）；
容器权威门按三挂载配方 → 退 0、**69 段 / 1270 / 0**，与宿主同读数
（`tmp/pr-466/container-final.log`）；三架构 `cargo check --offline -j 1 --target
{x86_64-unknown-none, aarch64-unknown-none, riscv64gc-unknown-none-elf} -p minix-textfilter
--bins` 各退 0、错误 0（`arch-*.log`）；`rustfmt --edition 2024 --check` 四件**原地** HEAD 0 /
现件 0（`tmp/pr-466/fmt-baseline.py`：退出码不属于 0/1 即判「没跑成」并抛异常，`finally` 还原后
逐件核 md5；中途我自己写出过 9 处与 2 处漂移，格式化后归零）；
`cargo clippy --offline -j 1 --all-targets -p minix-textfilter` 退 0、共 **149** 条，改动四件上
**34 条**——同一条命令在主树（HEAD 态）也是 149 条、同四件 34 条，按（文件, 消息）做集合差
**为空**（全在 `src/pr.rs`、全部存量；壳与两件测试各 0 条）⇒ 本轮零新警。
**A/B 对 `tmp/pr-466/pr`（软链到 `tmp/pr-419/pr_ref`，取名 `pr` 是为了让 `egetopt` 自己算出的
名字段与本壳一致；用前照例打 `-Z`：交回 `pr: illegal option -- Z` 加那三行 usage）跑，矩阵四十八形，SAME=35（其中退 0 的 9 形先数过）
/ 非预期 DIFF=0 / 页眉后缀比通过 3 形 / 已知分叉另计 10 形**（`tmp/pr-466/ab.log`），四类分别是
`ARGV-BYTE-PATH-IN-LIBS` 1 形、`PR-UNMODELLED-LEGACY`（`-T`/`-p`/`-r`）3 形、
`PR-EGETOPT-NUMBER-TOKEN-CONTINUATION` 6 形，加上时间场那一族按后缀比的 3 形。
**门账**：库内 `pr` **48 → 50**、`pr_face` **17 → 19**、跨壳表十七行 → **十八行**、
整包 69 段 **1266 → 1270**；夹具侧 55 处 `parse_pr_args` 改字节形（绑定形 27 拆两行、内联形 28
就地包），测试模块新立 `fn argv`，既有门的期望串逐字未动。

**进度与在册（全部现量）**：参数缝半已落 **二十一支**（`rev`、`paste`、`yes`、`tee`、`seq`、`tr`、
`colrm`、`fold`、`col`、`uniq`、`units`、`cksum`、`cmp`、`column`、`comm`、`cut`、`head`、`join`、
`jot`、`nl`、`pr`）；`grep -rl "support::args()" os/commands/usr-bin/textfilter/src/bin/ | wc -l`
→ **18**；命令面口径 → **87**；渲染半那条 grep 仍 **0** 行；`line_bytes` **40 处 / 29 件**
（`pr` 本来就在其中，本支未新增调用点）；`bin/pr.rs` 内 `from_utf8_lossy` 现量 **2 处**。
余下 18 支里带字母位诊断面的有 **6** 支（`sort`、`split`、`tail`、`unexpand`、`unifdef`、`vis`；
判据是本壳或它调用的库模块交回 `GetoptReport`），**切片 49 建议 `sort`**：字母序第一支，
`-t` 的判据与 `-k` 的字段说明符既进比较又进报文，形状与 `join` 同族而更密；参照件用前照例
先打一枚 `-Z`。**在册未办的五条建议**：①插一小轮收 `JOIN-STDIN-LEG-BYPASSES-DUAL-SURFACE`；
②把 `JOT-EMPTY-OPERAND-READS-OOB`、`JOT-PREC-MINUS-ONE-FORMAT`、`JOT-WORD-TOOLONG-ON-MACHINE`
一起拿到真机上量一次；③裁 `NL-ATTACHED-VALUE-LEADING-BLANK-STRIPPED` 那族判据；
④`PR-EGETOPT-NUMBER-TOKEN-CONTINUATION`（六形现量，一个批次量级）；⑤本轮新立的
`TEXTFILTER-07-TABLE-UNESCAPED-PIPES`（五处未转义竖线，纯文档面）。独立轮在册不碰：
`ARGV-BYTE-PATH-IN-LIBS`、`CKSUM-CHECK-LEG-TEXTSEAM`、`UNITS-DASH-REWRITE-AT-FIRST-CHAR`、
`UNITS-PARTIAL-UNIT-ON-ERROR`、`UNITS-PATH-SEARCH`、`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`、
五枚 `*-NON-PERMUTE-OPERAND-TAIL`、`CUT-SUPPRESS-STILL-PRINTS-NEWLINE`、
`CUT-EMPTY-BYTE-LIST-RUNAWAY`、`COL-CLIPPY-STOCK-LINT`、`BIN_SUPPORT-FMT-DRIFT-STOCK`、
`REV-MULTIBYTE-ERROR-LEG`、`NL-DELIM-LENGTH-RULE`、`NL-NON-PERMUTE-OPERAND-TAIL`、
`NL-SIGNED-COUNTERS`、`HEAD-NEGATIVE-COUNT-UNSIGNED-GUARD`、`HEAD-COUNT-LEADING-BLANK-ACCEPTED`、
`JOIN-STDIN-LEG-BYPASSES-DUAL-SURFACE`、`JOT-EMPTY-OPERAND-READS-OOB`、
`JOT-WORD-TOOLONG-ON-MACHINE`、`PR-TIMEFIELD-UNRENDERED`（本笔把这条从「注释里的一句话」
升成立册号，后缀比法就是为它立的）、`PR-UNMODELLED-LEGACY`，**本笔新立**
`PR-EGETOPT-NUMBER-TOKEN-CONTINUATION` 与 `TEXTFILTER-07-TABLE-UNESCAPED-PIPES`；
`todo-reconcile-scan` 的 `探针命名族` 漂移归一线。**取证回抄照办**：release 前把 `tmp/pr-466`
整目录复制回主树 `tmp/`。
- `2026-10-29` zcode-glm / 01 篇 todo 第六批（文档批，`claim/01TODO-DOC-zcode-glm` → 合并笔 `d30818db9`）：I-17（§2.6 内部拆五节，最大节 176 行；顶层号不动以保 prompt/ 与 02 篇的历史锚点，工单括注的 `alloc_lowest` 段现居 §2.4.1 不随移）、I-18（§4/§6 补章头导语；≥300 行巨节仅 §2.6 且已拆，§2.4 287 行未达阈值记录不改）、I-19（§3–§6 讲解条 ~90 条续扫、修 4 条纯事实条目）、I-23（§3.7 FW_PAYLOAD 落位段与三架构矩阵；`[ARCH]` 同字标记落 doc/design/code，设计侧因主树脏版留在工作区）、I-24 + L1378/L1408（否决表拆两行、TODO 块改状态叙述、I-22 终态句）、I-25 射程样例（FREE_MEMMAP 陈旧锚点改实况、D-64② 补可达路径、F8/d1a 按口径 b 内联）。**门读数**：`doc-style-lint --diff` 各笔 error 0；kernel mock / boot-shim check 无新告警。**在册**：I-25 全仓批（低优）、I-21 延后、L1488/L1532 用户侧；N-5 探针与 T1/T5/T3/T2/D-59/D-72/D-73 未动。

- `2026-10-29` zcode-glm / 01 篇 todo 第七批（`claim/01TODO-N5-zcode-glm` → 合并笔 `16904d718`）：N-5 池页零化归因探针——`test-kernel-map` 加一枚对照页（快照后 `AllocatePages(MaxAddress(4GiB-4KiB), LOADER_DATA)` 分配、位置标记填满），EBS 前/后/开分页后三次整页校验和 + 快照 conventional 归属判定。读数：`pa=0x5e9b000`、`pre=post_ebs=post_enable=0x4a00_0000_0001_ff00`、`verdict=replayed`、`in_snapshot_free=0`。判别：①「页表不重放 1:1」对该形态不成立（自举根重放映射、字节未变）；②「被当空闲复用清零」的结构前提未出现；历史 `res=127 zeroed=127` 的剩余解释是深拷之前的入口引用链（NK4-A），与深拷落地后症状消失一致。01 篇 §3.4 两处已改写，探针保留为可复跑证据。**门读数**：test-kernel-map build + 单测腿 PASS；doc-lint 0。**在册**：T1（大件）、TODO-04（随 T1）、T5，其余 T3/T2/D-59/D-72/D-73 与 §5 余项未动。

- `2026-10-29` zcode-glm / T1 子步 1a（`claim/01TODO-T1A-zcode-glm` → 合并笔 `5fd9ee756`）：`minix-boot/src/handoff_v1.rs` 新增跨镜像 wire 协议 v1 类型层——`BootHandoffV1` 信封（168B：协议头 magic/version/size/arena_len + 内核标量 + 运行时字段 + 五块 `byte_len` 描述符）、`WireMemoryRegion`(16B)/`WireBootModule`(32B, 16B 定长名槽)/`WirePlatformDescSource`(16B)/`WirePhysAddr`/`WireVirtAddr`、`FREE_UPPER_IDX_NONE=u64::MAX`；载荷不带地址（分区 = arena 基址 + 固定偏移，v8 形态）。arena 常量：信封 256B → wire 7616B → 语义 6144B，总 14016B ≤ 4 页；size/align/关键 offset 编译期门禁随三架构构建执行。**门读数**：minix-boot 35/0（+3）；三目标 check 各 0 error；doc-lint 0。**余下**：1b 序列化与语义分区、2 raw EBS 配对循环（四硬性契约）、3 受控复制五步、4 九面失败测试族、5 D-67 存留件 + D-69 builder 统一；TODO-04 与 T5 随其后。

- `2026-10-29` zcode-glm / T1 子步 1b–2b（`claim/01TODO-T1B..T1D` → 合并笔 `464ced711` / `624667b6a` / `5bf55c528`）：1b 生产者序列化器 `handoff_v1_write.rs`（信封/保留区/模块定长名槽/平台源/param/内存图双写/运行期补丁，容量与名长 fail-fast，LE 字节序；minix-boot 45/0）；2a `ebs_map.rs` 原始描述符走查分类（版本/步长/8 对齐/整除/checked/PA 限/两趟 RAM 顶；12 条测试）；2b `ebs_pair.rs` 注入式配对循环（重试只认 INVALID_PARAMETER、EBS 总尝试 2、EBS 前可扩容/失败后只复用、旧分类不跨 MapKey、map_key 配对断言；8 条测试）。**门读数**：minix-boot 45/0、boot-shim test-all 51/0（默认 uefi 39/0）、三架构 + aarch64-uefi check 0 error、clippy 无新面、doc-lint 0。**诚实边界**：本段全部为宿主可测层；raw `boot_services` 字段取值、arena/map buffer 分配与 `prepare_boot` 接线（2c）、接收端受控复制（3）、剩余失败面（4）、D-67/D-69（5）未动，生产启动路径零改动。

- `2026-10-29` zcode-glm / T1 子步 3a–5b（`claim/01TODO-T1E..T1G` → 合并笔 `b367e56c5` / `b65bca3fe` / `11ed113ba`）：3a `handoff_v1_read.rs` 接收端解码器（信封校验/哨兵/分区再验证/元素级拷贝，9 条测试）；**4 九面失败测试 ✅**（逐面对应 ebs_map/ebs_pair/read 的测试名已写入 01-todo §2.2）；5a D-69 单一 `build_kernel_info`（boot-shim 库根，9 参含 reserved_regions；删两份漂移副本；opensbi/直核腿改传 `DtbMemoryMap` 的 reserved，riscv 直核腿此前丢失的固件区回到 `KernelInfo`）+ D-67「唯一跨边界数据」旧口径改写；5b `param_buf()` getter。**门读数**：minix-boot 54/0、boot-shim test-all 51/0；三架构 + aarch64-uefi check 各 0 error；两个生产镜像冒烟 PASS（riscv bootface、aarch64 smp-aps）；doc-lint 0。**余下**：2c/3b 联合切根（aarch64 跨镜像腿 arena 分配/序列化/配对循环/`x0=arena` 跳转 + 内核 resume 解码与受控复制进落地区），与 T5 第五块落地区同批接线；T1 共享层与可测层至此完成。

**合回读数（第一百八十笔补记，批次四百六十六）**：本支六笔（码笔 `003be029a`、文档笔 `878a9e10b`、
里程碑笔 `0b4e4e7a0`、合并笔 `da9ca7547`、二次合并笔 `813211226`、三次合并笔即分支顶 `ea2b5b8ae`）
合回时并发线**正在连续落盘**：领取时的基线是 `e250214e4`，做完三笔后 `rewrite` 已到 `b367e56c5`
（01TODO-T1A..T1E/N5/DOC 二十七笔），第一次合并只撞一处——`new_edge4.md` 的尾追（本线要插
第一百八十笔，那侧要加 zcode-glm 的第六批、第七批、T1 1a、T1 1b–2b 四条子弹），按两侧各留全解
（编号里程碑在前、并发流水子弹在后）；解完之后主树又前进四笔（T1 5a/5b），因与本批八件
**零交集**，第二次合并只核一次即净落；再往前一笔（T1 3a–5b 的里程碑回写，只碰 `new_edge4.md`），
第三次合并由 `ort` 自动并上（`+2` 行、无标记）。判据在这一笔是可重复的：**快进资格必须每次
当场重验**——`git merge-base --is-ancestor rewrite <分支顶>` 要在**合并之后**再跑一次，
不能拿两分钟前那次「可快进」的读数当真（本笔就是靠这条重验发现了主树又动了两次）。
预判冲突的口径也留在这里：只有**按提交区间**列文件才算数
（`git diff --name-only <上一个已合入点>..rewrite`），按树比（`da9ca7547..rewrite`）会把本线自己
改过的文件也列进交集，看起来像八件全撞——本笔第一遍就差点被这个假交集带偏。
逐笔验祖先：六笔 `git merge-base --is-ancestor <笔> rewrite` **全 ✓**，快进后主树 `rewrite` 与
领取树顶同为 `ea2b5b8ae`（`8 files changed, 706 insertions(+), 161 deletions(-)`）。
主树当场复跑 `cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` →
**69 段 / 1270 / 0**，其中库内 626、`pr_face` 19、`bin_support_face` 4
（`tmp/pr-466-maintree.log`），与领取树最终态逐数相同。门②那条「不同树不可比」的口径本笔第三次
核：同一条 `notes-link-check` 在领取树是 764 个 md / 断链 131（与上一笔同数、本轮新增内容零命中），
在主树（快进后）另测；两树绝对数不可比，可比的是同树跨轮。取证回抄已办：`tmp/pr-466` 整目录
（32 件、952K）复制进主树 `tmp/`，`ab.log`、`pkg-post.log`、`container-final.log`、`negctl-P5.log`、
`clippy-post.log` 与领取树逐字节核 md5 相同。主树门④的存量 error 仍全落在别线在制的
01-stage 两个文件（本线四件 0 条），未动。

- `2026-10-29` zcode-glm / T5 + TODO-04（`claim/01TODO-T5-zcode-glm` / `01TODO-T4A-zcode-glm` → 合并笔 `6277a1d6a` / `014c929ff`）：T5/D-70 落地区扩到五块——`PLATFORM_SOURCE_STORE`（4 条，补 `BklProtected` 审计条目）与 `PARAM_BUF_STORE`（1024，先清零再复制），`store_kernel_info` 五块全部重指向 `.bss`；落地区测试扩平台源/param（含幂等再存指针稳定），超容量负控 5 枚（kernel mock 845/0/8）；01 篇 §4.5 三块→五块、M8/TODO-03、§5.2 计数 4→6。TODO-04 文本级先行：`os/boot-shim/tests/ebs_boundary_audit.rs` 切 `prepare_boot` 体、去注释、扫 EBS 后语句（零 `uefi::`/`boot::`/allocate/free/drop），带注入负控与注释豁免自检（3/0）；T1 配对循环接线后升级检查范围。**门读数**：三架构生产 check 各 0 error、doc-lint 0。**在册**：T1 的 2c/3b 联合切根（与 T5 已备齐的落地区接线）、T3/T2/D-59/D-72/D-73/TODO-05/I-25。

### 第一百八十一笔（批次四百六十七 · A2 参数缝半切片 49：`sort` 的字段分隔符、记录分隔符、操作数与四处回显交回原字节；同一枚 `ff` 走 `-R` 的两条路径有两种下场）

**这一支先说形状上的两件事**。第一件：`sort` 的 optstring 是 `minix3/usr.bin/sort/sort.c:148` 的
`"#adFfmrte?h:i?l:n?o:rR:sSt:T:ux"`——二十个字母，带值的六个是 `D`/`k`/`o`/`R`/`t`/`T`，
其余十四个不带值。第二件更影响做法：**它的选项环整个住在库里**
（`os/commands/usr-bin/textfilter/src/sort.rs:fn parse_options`），壳只交下 argv、再按交回的
`ParseError` 拼句子，所以这一轮的库门有牙——与上一支 `pr` 同类、与「环住在壳里」的 `nl` 相反。
值的六个去处逐条带锚点：`-t` 的首字节写进 `d_mask`（`:219`）决定字段怎么切，因此**进比较**；
`-R` 的首字节决定记录边界（`files.c:113-124` 的 `getrec`）并被 `putrec` 补在每条记录之后
（`files.c:194`、`:215`）；`-k` 的串交给 `setfield`/`setcolumn`（`init.c:159-217`），那五句 `errx`
**都不带** `%s`；`-o` 与操作数是路径；`invalid record delimiter %s` 回显 `optarg` 从游标起的原字节
（`sort.c:233-238`）；字母位来自 libc（`getopt.c:79` 一次进一个字节、`:103` 的 `%c` 原样写出），
措辞是 `unknown option`——与上一笔 `pr` 自带 `egetopt.c:148` 的 `illegal option` 在**同一张跨壳表**里
并存，这是「拿同族规律抹平会把句子写错」最省事的实物对照。

**本轮的正菜是判据的第三种形态：同一个变量的符号性随赋值路径分叉**。`REC_D` 在 C 里是 `int`
（`sort.c:96` 声明、`:226` 赋值），而它有两条来路：**字符路径** `REC_D = *optarg` 经过带符号的
`char`，所以 `-R<ff>` 拿到的是 **-1**；**数值路径**（`-R\377`、`-R 255` 之类经 `strtol`，
`:228-238` 还带 `REC_D < 0 || REC_D >= NBINS` 两道拒收，`NBINS` 是 `sort.h:77` 的 256）拿到的是
**+255**。两个可观察后果：① `-t` 那一支查 `d_mask[(u_char)*optarg] & REC_D_F`（`:219-220`），
字符路径把那位写到表外（`d_mask[-1]` 的越界写；本层不复制越界，只取它唯一可观察的后果
「不算冲突」），数值路径正落在 `d_mask[255]` 上——所以 `-R<ff> -t<ff>` **不**冲突而
`-R\377 -t<ff>` **是**冲突；② `getrec` 比的是 `int ch = getc()`（0..=255）与 `REC_D`，所以字符路径
那一枚永不命中，整份输入是一条记录。本层此前把 `record_delim` 存成 `u8`，两路混为一谈，
于是 `-R<ff> -t<ff>` 报了一句 C 根本不报的 `record/field delimiter clash`——**这一形是 A/B 逮出来的**
（参照件交回 `x\n\xff` 退 0，本壳退 2 且无输出）。现在字段类型改 `i32` 照 C 建模，写出去那枚仍是
低字节（`putchar` 的 `int`→`unsigned char`），五形现量两侧全等。前两轮各给过一条：`jot` 是
「取到的尾字节经 `char` 落地所以为负」，`pr` 是「同一枚字节进比较时符号性随**两侧类型**而分叉」，
本轮是「随**赋值路径**分叉」——同族判据至此有三种形态，且都是 A/B 而非门先发现的。

**参数缝本身的落点**：`parse_options` 收 `&'a [&'a [u8]]`，`PrArgs`/操作数改 `&'a [u8]`，
`c_atoi`/`scan_long`/`scan_column`/`parse_key` 一律按字节（`-k` 的 `rest` 从 `&str` 改 `&[u8]`；
它的分支判据本来只认 ASCII 数字与修饰字母），`-R` 数值形失败那句的回显位真的按 `Vec<u8>` 拼出；
壳侧走 `args_bytes()`/`prog_name_bytes()`，操作数那一句是 `err(2, "%s", argv[i])`（`sort.c:285`）
的三段形，本壳自己拼，**名字位与参照件逐字节同形**（以前经 `support::warn_line` →
`minix_sys::err_line(&str)` 会把裸字节折成 U+FFFD）。壳内 `from_utf8_lossy` 现量 **5 处**
（程序名、`-o` 两处、记录内容、操作数开档），**每一处都挂名在册项**——`sort` 因此是本族折点最多的一支，
而这恰好是它的结构决定的：它同时有开档、开写、路径试探三条腿。

**三条边界登记不执行**（都是现量之后判定「不在本轮射程」的）：① 记录内容本身是 `String`
（`bin/sort.rs:fn split_records` 过 `from_utf8_lossy`），所以「输入里带裸字节」的形两侧不同——
本壳交回 `a\xef\xbf\xbdb\n\xff`、参照件交回 `a\xffb\n\xff`；登记
`SORT-RECORD-CONTENT-TEXTSEAM`。这条同时给出了**参数缝改到了哪一步**的证据：argv 那头的字节现在对了，
内容那头的折叠才单独暴露出来（改之前两处一起错，读数上反而看不出来）。② `-o` 整条输出腿经
`bin_support` 的 `&str` 路径 API（`probe_path`/`create_sink`/`temp_candidate`），属「路径缝」另一条射程。
③ `fixit()`（`sort.c:151`）把 `+field` 换成 `-k field`，本树未建模，于是 `+2` 在这里被当档名
（参照件接受并按第二字段排）；登记 `SORT-PLUS-FIELD-LEGACY-UNMODELLED`。另有 `D`/`T`/`x` 三个字母
落进「集外字母」那一支，登记 `SORT-D-T-X-LETTERS-UNMODELLED`。

**参照件自身有一条缺陷，必须写在差分口径前面**：`tmp/family-diag-ref/sort_ref` 是 minix3 的 sort
源码链宿主 glibc（`strings` 里能看到 `record/field delimiter clash` 与那条 `usage: %s … [-R char]`，
所以判据可用），但它**一走到自己的 `usage()` 就 SIGSEGV**（rc 139；`-Z` 与 `-t, -t;` 两形当场都是）。
于是凡以 usage 收尾的形只能比「那一行报的是哪枚字节」，不能比退码与全文；字母位的绝对读数改由
Minix `getopt.c` 探针承载（主树 `tmp/sort-411/probe`，本轮现跑交回 `optopt=0303` 即 `c3`、
`0377` 即 `ff`，与本壳逐枚相同）。这套两段口径是批次四百一十一立下的，本笔只是继续用它，
并新记一条：**重建那枚探针要 `sys/featuretest.h` 一整套 Minix 头树，直接 gcc 会失败**，
所以用现成件、别再试重编。

**负控六枚（`tmp/sort-467/negctl.py` Q1..Q6）各咬 3/3/2/1/1/0**：Q1（壳侧 argv 退回文本缝）咬面门
两枚 + 跨壳表而**库内全绿**（壳的缝够不到库门，与 `pr` 的 P1 同形）；Q2（`-t` 的取值折一次）咬库门
两枚 + 面门；Q3（`-R` 的取值折一次）咬库门 + 面门；Q4（回显位折一次）与 Q5（操作数名字位折一次）
各咬面门一枚——这两条决策本来就住在壳里。**Q6 是有意放进去的「无分辨力探针」**：把 `-k` 的 spec
折一次，三档门全绿、cargo 退 0。原因是那五句 `errx` 都不带值，而折叠串与裸字节停在同一个位置
（既不是数字也不是修饰字母），所以折叠前后走同一支、交回同一句。它留在脚本注释与本文里，
是为了让下一次看到「注入不红」时能立刻分清是**漏测**还是**这条腿本来没有可观察差**——
「一整段全绿先怀疑覆盖面有洞」这条判据要有这一半才算完整。

**两起当场自错**：① 操作数那枚门第一版把期望写成「两枚开不了的档名各报一句」，实测两侧都是
**只出第一句**——C 的 `err(2, …)` 自己终止进程，不是「报完再退」；期望按实测改写并在注释里写明理由。
② 我把 C 的八进制 `"\377"` 直译成 Rust 的 `b"-R\377"`，编译期就报 `unknown byte escape`——
**Rust 的字节串没有八进制转义**，要的是「一个反斜杠 + 377」这五个字符，得写 `b"-R\\377"`；
同类直译风险在 `"%b %e %H:%M %Y"` 那种 strftime 格式串上也在。

**五道收尾门逐条读数**（`2026-10-30`，码笔 `7f6ed9413`、文档笔 `d2c47b2dd`）：①
`bash tools/todo-reconcile-scan.sh` → 13 项里 **1 项 DRIFT**（`探针命名族（行）` 32 vs 基线 18），
同一条命令在 HEAD 也是 32，且本轮改动四件各 **0** 命中 ⇒ 非本轮引入，归一线在制（连续第四笔同一条件数）；
② `python3 tools/notes-link-check.py rewrite-notes/` → 764 个 md、**断链 131**（连续三笔同数），
本轮新增行落在 `todo.md:122..228`、`07-text-filter.md:6`、`:195`、板 `:27`，逐行核对这四处**零命中**；
③ `bash tools/todo-staleness-check.sh …TODO-LEDGER-OPEN.md` → **PASS（0 项）**；
④ `bash tools/doc-style-lint.sh --diff` → 3 个文件 / 111 个新增行、**error 级 0**；
⑤ `bash tools/check-staged-inflight.sh` → 退 0，避让清单 1 条（`TODO-3ARCH-PARITY-20261006.md`）未触碰。

**门读数补全**：宿主整包 `cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` →
**69 段 / 1273 / 0**（`tmp/sort-467/pkg-post.log`；负控还原并重编之后的 `pkg-final.log` 同数）；
容器权威门退 0、**69 段 / 1273 / 0** 与宿主同读数（`container-final.log`）；三架构
`cargo check --offline -j 1 --target {x86_64-unknown-none, aarch64-unknown-none,
riscv64gc-unknown-none-elf} -p minix-textfilter --bins` 各退 0、错误 0（`arch-*.log`——
中途一次三架构**同红**是 `cargo` 跑在了仓库根而不是 `os/`，同一条判据第四次兑现：
「计数异常先问跑没跑对地方」）；`rustfmt --edition 2024 --check` 四件**原地** HEAD 0 / 现件 0
（`fmt-baseline.py`；中途自造的 2 处已格式化归零）；`cargo clippy --offline -j 1 --all-targets
-p minix-textfilter` 退 0、共 **149** 条、**改动四件上 0 条**，与主树（HEAD 态）同四件做
（文件, 消息）集合差为**空**——中途那次 151 条是本轮自造的两条（`&bytes[..]` 冗余切片、
`parse_key(&spec[0])` 多取一层引用），已改。**A/B 对 `tmp/sort-467/sort` 跑，矩阵四十五形，
SAME=29（其中退 0 的 17 形先数过）/ 字母位三方核通过 6 形 / 非预期 DIFF=0 / 已知分叉另计 10 形**
（`tmp/sort-467/ab.log`），五类计数按实数为：`SORT-RECORD-CONTENT-TEXTSEAM` **4** 形、
`SORT-D-T-X-LETTERS-UNMODELLED` 3 形、`ARGV-BYTE-PATH-IN-LIBS` 1 形（真实存在的裸字节档名
本壳打不开）、`-o` 路径缝 0 形（那一形实测两侧同形，已归回归项）、参照件自身走 `usage()`
段错误 1 形、`SORT-PLUS-FIELD-LEGACY-UNMODELLED` 1 形——合计 10。**门账**：库内 `sort` **28 → 29**、`sort_face` **22 → 24**、
跨壳表十八行 → **十九行**、整包 69 段 **1270 → 1273**。

**进度与在册（全部现量）**：参数缝半已落 **二十二支**；`textfilter` 余 **17** 支
（`grep -rl "support::args()" os/commands/usr-bin/textfilter/src/bin/ | wc -l`）、命令面 **86** 支；
渲染半那条 grep 仍 **0** 行；`line_bytes` **40 处 / 29 件**（`sort` 本来就在其中）。余下 17 支里
带字母位诊断面的有 **5** 支（`split`、`tail`、`unexpand`、`unifdef`、`vis`），**切片 50 建议
`split`**：字母序第一支，它的 `-a`/自动后缀长度与操作数尾巴都在字节链上，且
`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE` 已在册、可与本轮同批裁。**在册未办的六条建议**：
①插一小轮收 `JOIN-STDIN-LEG-BYPASSES-DUAL-SURFACE`；②`jot` 那三条拿真机量一次；
③裁 `NL-ATTACHED-VALUE-LEADING-BLANK-STRIPPED` 那族判据；④`PR-EGETOPT-NUMBER-TOKEN-CONTINUATION`
（六形现量，一个批次量级）；⑤本轮新立的 `SORT-RECORD-CONTENT-TEXTSEAM`（**内容半**，
比参数缝大一档，但它现在是 `sort` 剩余读数的唯一大头）；⑥`TEXTFILTER-07-TABLE-UNESCAPED-PIPES`。
独立轮在册不碰：`ARGV-BYTE-PATH-IN-LIBS`、`CKSUM-CHECK-LEG-TEXTSEAM`、`UNITS-*` 三枚、
`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`、五枚 `*-NON-PERMUTE-OPERAND-TAIL`、
`CUT-SUPPRESS-STILL-PRINTS-NEWLINE`、`CUT-EMPTY-BYTE-LIST-RUNAWAY`、`COL-CLIPPY-STOCK-LINT`、
`BIN_SUPPORT-FMT-DRIFT-STOCK`、`REV-MULTIBYTE-ERROR-LEG`、`NL-*` 四枚、`HEAD-*` 两枚、
`JOIN-STDIN-LEG-BYPASSES-DUAL-SURFACE`、`JOT-*` 两枚、`PR-TIMEFIELD-UNRENDERED`、
`PR-UNMODELLED-LEGACY`、`PR-EGETOPT-NUMBER-TOKEN-CONTINUATION`，**本笔新立**
`SORT-RECORD-CONTENT-TEXTSEAM`、`SORT-PLUS-FIELD-LEGACY-UNMODELLED`、
`SORT-D-T-X-LETTERS-UNMODELLED`；`todo-reconcile-scan` 的 `探针命名族` 漂移归一线。
**取证回抄照办**：release 前把 `tmp/sort-467` 整目录复制回主树 `tmp/`。

**合回读数（第一百八十一笔补记，批次四百六十七）**：本支三笔（码笔 `7f6ed9413`、文档笔 `d2c47b2dd`、
里程碑笔 `76c1319d6`）合回时 `rewrite` 自基线 `f03aa8ffc` 起**没有新提交**——
`git log --oneline f03aa8ffc..rewrite` 交回空、merge-base 就是基线 ⇒ 纯快进、零冲突。
但这条「没分叉」依旧是**当场量出来的**，而且量了两遍：快进前一遍（`git merge-base
--is-ancestor rewrite <顶>` 与主树八件的 `git status --porcelain` 各交回空），快进动作本身一遍；
上一笔就是靠这个二次重验发现主树在两次命令之间被别线推进了两回，本笔没有复现那一幕不等于
不必再验。逐笔验祖先三笔**全 ✓**，快进后主树 `rewrite` 与领取树顶同为 `76c1319d6`
（`8 files changed, 618 insertions(+), 125 deletions(-)`）。主树当场复跑
`cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` → **69 段 / 1273 / 0**，其中
`sort_face` 24 枚、`bin_support_face` 4 枚（`/home/xzhao/github/minix-rs/tmp/sort-467-maintree.log`），
与领取树最终态逐数相同。门②那条「不同树不可比」第四次核：同一条 `notes-link-check` 在领取树是
764 个 md / 断链 131（连续三笔同数），在主树另测；主树门④的 error 全落在别线在制的 01-stage
文件上，本线四件 0 条，未动。取证回抄已办：`tmp/sort-467` 整目录（28 件、848K）复制进主树
`tmp/`，`ab.log`、`pkg-final.log`、`container-final.log`、`negctl-Q6.log`、
`clippy-post.log` 与领取树逐字节核过 md5 相同——**其中 `negctl-Q6.log` 是有意全绿的那枚
无分辨力探针，回抄它正是为了让「注入不红」这条读数有据可查**。

### 第一百八十二笔（批次四百六十八 · A2 参数缝半切片 50：`split` 的操作数、输出前缀与四处值回显交回原字节；这一支没有判据分岔，量出的是「宿主全绿、真机编译红」）

**这一支的形状**：`minix3/usr.bin/split/split.c:80` 的 optstring 是 `"0123456789b:l:a:n:"`——
十个数字字母（遗留的 `-<数字>` 形，`:82-98` 从 `argv[optind] + 1` 起取数）加四个带值字母。
值和操作数一共通向五条腿，逐条带锚点：① 字母位由 libc 打印（`minix3/lib/libc/stdlib/getopt.c:79`
的 `optopt = *place++` 一次进一枚字节、`:103` 的 `%c` 原样写出，措辞是 `unknown option`，
与上一支 `pr` 自带 `egetopt.c:148` 的 `illegal option` 又成一对）；② 四句
`errx(1, "%s: illegal … count.", optarg)`（`:99-108` 字节数、`:109-116` 行数、`:117-121` 后缀长度、
`:123-127` 分片数）把**值本身**打进句子；③ 第一枚操作数是输入档名，开不成即
`err(1, "%s", *argv)`（`:135-139`）；④ 第二枚操作数是输出前缀（`:143` 的 `base = *argv++`，
缺省 `"x"`），它加上 26 进制两位后缀就是每一枚分片的名字，这名字既进 `creat`、失败时又进
`err(1, "%s", fname)`（`:332-333`）；⑤ `split` 自己那三句 usage 之外的出口都排在开档之后
（`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE` 已在册，本轮未动）。取数与切块住在库里
（`os/commands/usr-bin/textfilter/src/split.rs:fn parse_split_args`），报文与拼名住在壳里
（`src/bin/split.rs:fn err3`、`fn out_path`），所以本轮的库门与壳门都有牙。

**本轮第一条「不是渲染、是读数」的发现：这一支没有判据分岔**。前三支各翻出过判断被字节改写
的活例——`join` 是 `-t` 按字节数判长度（`join.c:201` 的 `strlen(tabchar = optarg) != 1`），
`jot` 是尾字节经带符号 `char` 落地，`sort` 是同一枚 `ff` 走 `-R` 的两条赋值路径符号性不同。
`split` 把三侧读数摆齐之后一条都没有：它的取值判据只认 ASCII 数字与小写 `k`/`m`
（`split.c:100-105` 的 `strtoull` 加 `*p != '\0'` 判尾），一枚非法字节**折叠前后都不是数字**，
所以接受与拒绝完全同形，差的只是报出去与拼出来的那串字节。现量在
`tmp/split-468/measure.log`（十一形 × 三侧，件同名放置）：改前的壳只有正常切分那一形与参照件
逐字节相同，改后的壳十一形里十形相同；剩下那一形是**盘上的分片名**（见下面那条在册项）。
「同一条腿的两侧符号性」这类判据在 `split` 上不存在，是因为它的值从不做比较——它只做两件事：
当数字用（切多少）和当名字用（写到哪）。

**第二条是编译器给的，也是本轮最该留下的一条判据**：三架构门当场红，报
`no method named 'to_string' found for struct 'Strerror'`（`x86_64-unknown-none`、
`aarch64-unknown-none`、`riscv64gc-unknown-none-elf` 三个 target 同红、各 2 条错误）。原因是本轮
新写的 `fn err3` 用 `minix_sys::Strerror(code).to_string()` 取那句 `strerror` 文本，而 `no_std`
形态下 `to_string()` 来自 `alloc::string::ToString`，本文件只 import 了 `String`。宿主侧 std
的 prelude 带着 `ToString`，所以整包 69 段 / 1276 全绿也看不见这件事。这与批次四百六十七在
`bin/sort.rs` 上栽的是同一条：**只要动了 `src/bin/*.rs` 里的字符串拼装，三架构门就得当场跑，
不能留到收尾**——本笔把它从「记忆」升成写在台账里的动作。补 import 后三架构各退 0、错误 0。

**参数缝本身的落点**：库侧 `SplitArgs.operands` 改 `Vec<&'a [u8]>`、四枚 `SplitError::Illegal*`
的值位改 `&'a [u8]`、`line_count`/`byte_count` 收 `&[u8]` 并新立 `fn decimal`（逐位
`checked_mul(10)` + `checked_add`，装不进 `usize` 的全数字串交回 `None`）；壳侧走
`args_bytes()`/`prog_name_bytes()`，`out_path` 交回 `Vec<u8>`、`ChunkWriter.prefix` 是 `&[u8]`、
`fn err3` 自己拼三段形（程序名、标签、`strerror` 句子），标签位按字节。程序名那一处仍折一次
（`GetoptReport::line_bytes` 与 `minix_sys::err_line` 两条 `&str` 口，在册
`ARGV-BYTE-PATH-IN-LIBS`），另有三条开档口（`create_sink`/`stream_input`/`input_size`）折的是
路径，所以**盘上的档名**还是折过的：参照件对 `split -l 1 - q<ff>` 落 `q` + `377` + `aa`，
本壳落 `q` + `357 277 275` + `aa`（`tmp/split-468/ab.log` 末行按类分开计数）。同一条腿的
**报文**那一半本轮翻正了：`-l 1 - p<ff>/` 现量 `split: p\xff/aa: No such file or directory`，
与参照件逐字节相同。

**顺手量出一处取值判据的分岔，本轮不动、只登记**：`split.c:110-112` 对「一串全是数字、
但装不进 `unsigned long long`」的 `-l` 取值是**饱和之后接受**（那一句只查 `*p != '\0'` 与
是否为零，不查 `errno`、也没有上界可查）。参照件三形退 **0** 且各切出一枚 `xaa`
（`-l 99999999999999999999`、`-l 18446744073709551616` 即 `ULLONG_MAX + 1`、
`-l 9999999999999999999999999999999`），同位置的 `-l 4294967296`（装得进）两侧都退 0；
本层 `fn decimal` 装不进就交回 `None`，于是那三形报 `illegal line count.` 退 1。**这条与参数缝
无关**：改前的件同判，两份读数一并记在同一份 `tmp/split-468/overflow.log` 里。登记
`SPLIT-OVERFLOW-COUNT-ACCEPTED`——它要的是「本层是否复制 `strtoull` 的饱和语义」这个口径裁决，
不在参数缝的射程里，也不该由本片顺手改。

**门账（先对测试名集合再对总数）**：库内 `split` 模块 **29 → 30**（新立
`the_value_and_the_operand_legs_hand_back_the_argv_bytes`，六段：四句值位、字母位 `ff` 交回
U+00FF、前缀原字节跟着操作数一起交回；名字集合用
`cargo test --offline -j 1 -p minix-textfilter --lib split::tests -- --list | grep -c ': test'`
现量 = 30），壳内单测 **10 → 11**（新立 `a_raw_byte_in_the_prefix_survives_into_the_chunk_name`，
钉拼名这一腿），`split_face` **30 → 31**（新立
`the_diagnostic_and_the_chunk_name_carry_the_argv_bytes`，八段，其中最后一段按当前形状钉住
盘上那个折过的档名），跨壳表十九行 → **二十行**（`("split", SPLIT, 1, b"split: unknown option -- \xff\n")`，
usage 一行、退 1，`split.c:355-361`），整包 69 段 **1273 → 1276 passed / 0 failed**。夹具侧 50 处
`parse_split_args` 的字面量数组包进新立的 `fn argv`，其中 7 处结果要留用的拆成两行，10 处操作数
期望改字节、10 处错误值位改字节面量；`é` 的三形写成字面字节 `c3 a9`，因为那两枚字节正是要断言的东西。

**负控五枚（`tmp/split-468/negctl.py`，逐枚读数 `negctl-K1.log`..`negctl-K5.log` 与
`negctl-summary.txt`）各咬 2/4/1/2/1 档**：K1（壳侧 argv 退回文本缝）咬跨壳表与 `split_face`
各一枚而**库内 628 全绿**——「库门对 argv 缝没有牙」第七次量化；K2（字母位改按**字符**取，
即 `String::from_utf8_lossy(&bytes[1..]).chars().next()`）是唯一穿透到库里的一枚，咬库内两枚 +
跨壳表 + 面门两枚；K3（`-l` 那句的值位折一次）只咬面门，因为报文住在壳里；K4（`out_path` 里
折前缀一次）咬壳内新门 + 面门；K5（`err3` 的标签位折一次）只咬面门。**K3 与 K4/K5 各红各的**，
「值进报文」「名进报文」是两条腿，与 `jot` 那轮的 K3/K4 分工同形。每枚前后按整文件 md5 与
`tmp/split-468/post/`（完整相对路径）成对复核，跑完 `git status` 只剩改动的那四件。

**A/B 对参照件 `tmp/split-311/split`（同名软链起，程序名段自然对齐）跑矩阵五十三形，
SAME=53 / DIFF=0**（`tmp/split-468/ab.log`），另计 1 形在册分叉（盘上分片名）。比的是退码、
标准输出、标准错误、目录里的档名（按字节）与各档内容五件事；非空跑核验照上一笔那条先做——
五十三形里两侧同退 0 的有 17 形。参照件归属当场验过：`-Z` 交回 `split: unknown option -- Z`
加一行 usage，是 Minix 的 `getopt.c` 那一支，不是 glibc 带引号的 `invalid option`。

**四起工具自错，全是「没跑或被截断的东西被当成读数」这一族**：① **夹具转换脚本把
`src/split.rs` 整个截成 0 字节**（`git diff --numstat` 交回 `0 1186`，库里一次 `#[test]`
都不剩）：脚本把链式正则替换的结果直接写回，没有先断言「新内容非空、尺寸在合理带内」。
本轮起所有写回类脚本一律带两道断言（非空 + 尺寸带），`tmp/split-468/fixture.py` 与
`fixture2.py` 即带断言的样板。② **负控驱动第一次跑出来的五枚全是「没编译过」**：cargo 的
`Running …` 行走 stderr、`test result:` 行走 stdout，脚本分开两股流只解析 stdout，于是
「目标名 → 结果」永远配不上对；改成 `stderr=STDOUT` 合流再解析，五枚读数才落地。这条与
「0 命中先问跑没跑成」是同一族，但形态新：**流分两股时，读数会全空而不是全错**。③ **第一次
三侧对照跑的是没重编的改前件**：造「改前件」时从 HEAD 覆写那两个源文件去编译，还原源码之后
没有重编，于是 `target/debug/split` 还是改前那枚——表现为 AFTER 与 BEFORE 逐形相同、只有程序名
不同，一眼可疑才查出来。判据：任何多侧对照之前先重编，并把件复制出来**同名放置**（本轮是
`bin-after/split` 与 `bin-before/split`），再核对复制件的 md5 与构建目录那枚一致。④ 新立的
壳内门第一版把 `out_path` 的后缀写成 index 27 = `ab`，实测是 `bb`（27 = 1×26+1），又一次
「期望值必须先量后写」。

**五道收尾门逐条读数**（`2026-10-30`，码笔 `339b4932f`、文档笔 `4636310f7`）：①
`bash tools/todo-reconcile-scan.sh` → 13 项里 **2 项 DRIFT**（`探针待滚除（行）` 0 vs 基线 13、
`探针命名族（行）` 32 vs 基线 18），**同一条命令在主树跑出完全相同的两项漂移**
（`/home/xzhao/github/minix-rs/tmp/split-468/gate1-reconcile.log` 与主树同跑）⇒ 非本线引入，
归 01-stage-kernel 那侧的探针台账；② `python3 tools/notes-link-check.py rewrite-notes/` →
764 个 md、**断链 131**（与上一笔同数），集合差按 `(文件:行, 目标)` 逐对核：只有
`todo.md` 里五条既有断链的行号因本轮插入 **131 行**而位移（3785→3916 等），**新增为零**；
③ `bash tools/todo-staleness-check.sh rewrite-notes/coordination/TODO-LEDGER-OPEN.md` →
**PASS（0 项）**；④ `bash tools/doc-style-lint.sh --diff` → 3 个文件 / 149 个新增行、
**error 级 0**（中途一次 1 条：`todo.md` 里有一句拿「过去怎么写的」当主语叙述修复史，SL-5 命中，已改成
按 C 的行为直接表述）；⑤ `bash tools/check-staged-inflight.sh` → 退 0，避让清单 1 条
（`TODO-3ARCH-PARITY-20261006.md`）未触碰。

**门读数补全**：宿主整包 `cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` →
**69 段 / 1276 / 0**（`tmp/split-468/pkg-1.log`；负控还原后 `pkg-2.log`、格式化后 `pkg-3.log`
三次同读数、退出码各 0）；容器权威门按 §7.3 三挂载配方 → 退 0、**69 段 / 1276 / 0** 与宿主
同读数（`container-gate.log`）；三架构各退 0、错误 0（`arch-*.log`，中途那次三架构同红是
真错而不是跑错目录，见上面那条）；`rustfmt --edition 2024 --check` **原地**量：HEAD 那三份
各 0 处 `Diff in`，本轮改出的三份格式化前是 6 / 4 / 3 处（`fmt-head-*.log` 对照 `fmt-*.log`），
格式化之后四件全 0 处；`cargo clippy --offline -j 1 --all-targets -p minix-textfilter` 退 0，
与主树（HEAD 态）同参数对照两树都是 149 行 / 去重 61 条 / 位置 44 处，集合差只有
`src/bin/split.rs` 那两条既有警告（`items_after_test_module`、`io::Error::other`）在
414/375 → 442/403 的**位移**，本轮零新警。

**进度与在册（全部现量）**：参数缝半已落 **二十三支**；`textfilter` 余 **16** 支
（`grep -rl "support::args()" os/commands/usr-bin/textfilter/src/bin/ | wc -l`）、命令面 **86** 支；
渲染半那条 grep 仍 **0** 行。`line_bytes` 本笔把口径固定成「整包含测试」：
`grep -rn line_bytes os/commands/usr-bin/textfilter --include=*.rs` → **82 处 / 34 件**
（`split` 未新增调用点）；上一笔的「40 处 / 29 件」是另一口径且未附命令，两个数不可直接比较，
此后一律按上面这条命令复现。余下 16 支里带字母位诊断面的有 **4** 支（`tail`、`unexpand`、
`unifdef`、`vis`），**切片 51 建议 `tail`**：字母序第一支，optstring 是 `"Fb:c:fn:r"`
（`tail.c:108`，注意 `n` **不带冒号**），除字母位与 `tail.c:90` 的 `illegal offset -- %s`
值回显之外，`obsolete()`（`tail.c:217-`）在 getopt 之前**就地重写 argv**、判据按字节比 token
的尾字符（`:246` 的 `*t == 'f' || *t == 'r'`、`:251-260` 的 `case 'b'/'c'/'l'`），是这一族里少见的
可能翻出**判据**分岔的腿。**在册未办的七条建议**：①插一小轮收
`JOIN-STDIN-LEG-BYPASSES-DUAL-SURFACE`；②`jot` 那三条（`JOT-EMPTY-OPERAND-READS-OOB`、
`JOT-PREC-MINUS-ONE-FORMAT`、`JOT-WORD-TOOLONG-ON-MACHINE`）拿真机量一次；③裁
`NL-ATTACHED-VALUE-LEADING-BLANK-STRIPPED` 那族判据；④`PR-EGETOPT-NUMBER-TOKEN-CONTINUATION`；
⑤`SORT-RECORD-CONTENT-TEXTSEAM`（**内容半**，比参数缝大一档）；
⑥`TEXTFILTER-07-TABLE-UNESCAPED-PIPES`；⑦本笔新立的 `SPLIT-OVERFLOW-COUNT-ACCEPTED`
（饱和语义的口径裁决）。独立轮在册不碰：`ARGV-BYTE-PATH-IN-LIBS`（本笔为它添一条腿的读数：
分片名落盘那一条）、`CKSUM-CHECK-LEG-TEXTSEAM`、`UNITS-*` 三枚、
`SPLIT-OPERAND-ORDER-OPEN-BEFORE-USAGE`、五枚 `*-NON-PERMUTE-OPERAND-TAIL`、`CUT-*` 两枚、
`COL-CLIPPY-STOCK-LINT`、`BIN_SUPPORT-FMT-DRIFT-STOCK`、`REV-MULTIBYTE-ERROR-LEG`、`NL-*` 四枚、
`HEAD-*` 两枚、`JOIN-STDIN-LEG-BYPASSES-DUAL-SURFACE`、`JOT-*` 三枚、`PR-*` 三枚、`SORT-*` 三枚；
**本笔新立** `SPLIT-OVERFLOW-COUNT-ACCEPTED`。另有一处账面更正：码笔 `339b4932f` 的提交信息里
写「库内 split 28→29」，现量应为 **29 → 30**（命令见上面门账），提交信息不改写，以本笔为准。
**取证回抄照办**：release 前把 `tmp/split-468` 整目录复制回主树 `tmp/`。

**合回条件（本笔当场量）**：`rewrite` 自基线 `934bf04df` 起没有新提交——
`git log --oneline 934bf04df..rewrite` 交回空、`git merge-base --is-ancestor 934bf04df rewrite`
退 0 ⇒ 纯快进、零冲突。主树合并路径（`rewrite-notes/18-stage-commands/`、
`rewrite-notes/coordination/new_edge4.md`、`rewrite-notes/coordination/PARALLEL-20261018-A-CMD.md`、
`os/commands/usr-bin/textfilter/`）的 `git status --porcelain` 四处交回空。快进之后的主树复跑
读数另记一笔合回补记（沿用批次四百六十七那两笔的做法）。

**合回读数（第一百八十二笔补记，批次四百六十八）**：快进前又核一遍——`git log --oneline
934bf04df..rewrite` 仍交回空、`git merge-base --is-ancestor 934bf04df rewrite` 退 0 ⇒ `git merge
--ff-only claim/GETOPT-REPORT-BYTE-LETTER-qoder` 纯快进成功（`8 files changed, 768 insertions(+),
171 deletions(-)`，主树 `rewrite` 顶到 `ae3fc1c6e`），主树合回前后各跑一次 `git status --porcelain`：两侧都是 **29** 条别线在制项
（`prompt/`、`AI-chats/`、`rewrite-notes/01-stage-kernel/` 三族），本线四个路径
（`rewrite-notes/18-stage-commands/`、`rewrite-notes/coordination/new_edge4.md`、
`rewrite-notes/coordination/PARALLEL-20261018-A-CMD.md`、`os/commands/usr-bin/textfilter/`）
一条都没被牵进暂存区。逐笔验祖先三笔全 ✓（码笔 `339b4932f`、
文档笔 `4636310f7`、里程碑笔 `ae3fc1c6e`）。主树当场复跑
`cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` → **69 段 / 1276 / 0**，其中库内 628、
`split_face` 31、`bin_support_face` 4，与领取树最终态逐数相同
（`/home/xzhao/github/minix-rs/tmp/split-468-maintree.log`）。**这一笔自己也踩了那条老坑**：第一次
复跑是在仓库根发起的，cargo 当场报 `could not find 'Cargo.toml' in /home/xzhao/github/minix-rs`，
退出码 101 而「test result」行数 0——正是「0 命中先问跑没跑成」的第五次实例；把命令挪进 `os/`
重跑才是上面那串数。取证 `tmp/split-468/`（三侧现量、A/B、负控五枚日志、门日志、改前与改后两枚件）
与主树门日志都在主树 `tmp/` 下，无需回抄。

### 第一百八十三笔（批次四百六十九 · A2 参数缝半切片 51：`tail` 的取值回显、遗留形整段回声、横幅与档名交回原字节；横幅那条腿端到端观察不到，只能钉在纯函数上）

**这一支的形状与本族前几支不同**。`minix3/usr.bin/tail/tail.c:108` 的 optstring 是 `"Fb:c:fn:r"`：
七个字母，带值的只有 `b`/`c`/`n`。但 `tail` 在 `getopt` **之前**还有一趟 `obsolete()`（`:106` 调用、
`:217-296` 实现），它**直接改写 argv**：按字节判 token 的形状（`:225-228` 的 `ap[0]` 是 `-`/`+`
与 `ap[1] == '-'` 即 `--` 两个终止条件、`:231` 起的外层 `switch (*++ap)`、`:248` 的
`*t == 'f' || *t == 'r'`、`:252-261` 的 `case 'b'/'c'/'l'` 与数字那一串），命中就 `malloc` 一枚新串
（`-` + 尾字母 + 单位字母 + 原 token 首字符 + 数字，`:271-272` 的 `*p++ = *argv[0]` 与
`strcpy(p, ap)`），再把指针换掉。于是这一支的字节腿比前几支多两条：

- **回声里的值可以是程序自己造的**。`-1<ff>2` 被改写成 `-n-1<ff>2`，`ARG` 的 `*p`（`:89`）停在
  那枚 `ff` 上，于是 `xerrx(1, "illegal offset -- %s", optarg)`（`:90`）报的是**改写之后**的值
  `-1<ff>2`——`optarg` 在这里不再是 argv 的原词，而是本程序自己 `malloc` 出来的串。前五支
  （`join`/`jot`/`nl`/`pr`/`sort`/`split`）的回声值都直接来自 argv，这是第一条。
- **多操作数时那行 `==> %s <==` 横幅走标准输出**（`:173-174`），不是诊断腿，名字位同样是 argv
  的那枚字节。

六条腿逐一带锚点：字母位（libc `getopt.c:79` 一次进一枚字节、`:103` 的 `%c` 原样写出，措辞
`unknown option`）；三句 `illegal offset -- %s` 的值位（附值 `-n<ff>`、分词 `-n <ff>`、`-c<ff>`）；
`obsolete()` 自己那句 `xerrx(1, "illegal option -- %s", *argv)`（`:269`）带的是**整段 token**；
改写后的值位；诊断行里的档名（`ierr()` 念全局 `fname`，`:169` 与 `forward.c:196-198`）；横幅的名字位。
usage 一行、大写 `Usage:`、带程序名、退 1（`:299-305`）。

**判据这一面量到的结论是「折叠改不了判决」，而它依赖编码构造**。`obsolete()` 的判据全是单字节
ASCII 比较，而 `from_utf8_lossy` 只会把非法字节换成 `ef bf bd` 三枚——UTF-8 的延续字节恒在
`80..=BF` 之内，**永远不是 ASCII**，所以「折叠前后命中同一个 case」对这一支是构造级的恒成立，
不是这批读数的运气。会变的只有长度：C 的 `t = *argv + len - 1` 取的是最后一枚字节，折过一次
token 变长、`t` 落到别处，但 `-1<ff>2` 那一形换上去的 `bd` 与原来的 `ff` 都不是 ASCII 字母，
判决仍然一致。三侧现量印证（`tmp/tail-469/measure.log` 二十二形）：**改前壳 10/22 与参照件逐字节
同形，改后 19/22**；翻正的九形正是上面那六条腿（`obsolete-digit-ff-last` 与
`obsolete-illegal-whole-token` 改前改后走同一支，只是回声里的字节不同）。

**横幅那条腿端到端观察不到，所以只能在纯函数上钉**。参照件对 `-n 1 a<ff>.txt a.txt` 开得了那枚
档名带裸字节的**真档**并打出 `==> a\xff.txt <==`（`tmp/tail-469/ab.log` 末段），而本壳开不了——
档名递给 `open` 之前必须过 `os/libs` 那条 `&str` 口（在册 `ARGV-BYTE-PATH-IN-LIBS`），于是只能报
`tail: a\xff.txt: No such file or directory`（**报文**那一半本轮已按字节，所以这一句与参照件逐字节
同形；不同的只是开档成败）。为了让「名字按字节进横幅」这件事有门可钉，壳侧把拼字抽成纯函数
`fn banner(first, name) -> Vec<u8>` 并立一枚单测。负控 K5（在 `banner` 里折一次）**只咬这枚壳内门、
面门全绿**——这一枚把「这条腿本来没有可观察差」与「这条腿现在观察不到」明确分开：后者是被那条
`&str` 口挡住，等它换成字节形，端到端那一形会自己红。与 `sort` 那轮的 Q6（有意放的无分辨力探针）
相对，这是同一判据的另一种形态。

**另立一形与字节缝无关的取值判据分岔，四形两方向**：C 的 `ARG` 写的是
`off = strtoll(optarg, &p, 10) * (units)`（`:88`），而 `:89` 只判 `if (*p)`、不查 `errno`，于是
「一串全是数字但装不进 `long long`」的取值是**饱和之后继续跑**。读数在
`tmp/tail-469/saturation.log`：`-n +99999999999999999999 a.txt` 参照件退 **0** 且不输出任何东西
（跳过 `LLONG_MAX` 行直达档尾），本壳报 `illegal offset` 退 1——**可观察的接受/拒绝分岔**；
`-n <20 位>` 与 `-c <23 位>` 两侧都退 1 但句子不同（参照件在环形缓冲那步打
`tail: malloc: Cannot allocate memory`）；反方向 `-b 99999999999999999 a.txt` 里 C 那句 `* 512`
溢出成一个巨大负偏移，`fseek` 失败，参照件退 1 报 `tail: a.txt: Invalid argument`，而本壳
`saturating_mul` 得 `usize::MAX`、退 0 把整档打出来。登记 `TAIL-OFFSET-SATURATION-OUTCOME`，并与
上一笔的 `SPLIT-OVERFLOW-COUNT-ACCEPTED` 并成同一件事：**「本层是否复制 `strtoll`/`strtoull` 的饱和
语义」需要一次口径裁决**，而且其中乘法溢出那一半在 C 本来就是未定义行为、不该复制。现在这件
裁决有两支的读数（`split` 三形 + `tail` 四形），本轮**不动**，只把四形按类分开记在 A/B 之外。

**落码层**：库侧 `src/window.rs` 的 `parse_tail_args` 收 `&[&[u8]]`、`TailArgs.operands` 改
`Vec<Vec<u8>>`、`obsolete_step` 收 `&[u8]` 且 `ObsoleteStep::Rewritten` 交回 `Vec<u8>`、两枚
`TailError` 的值位改 `Vec<u8>`、字母位统一 `char::from(bytes[pos])`；数字收法新立
`fn digits_to_usize`（逐位 `checked_mul(10)` + `checked_add`），它给出的接受集与
`str::parse::<usize>()` **逐条相同**——这是刻意的，本轮只搬字节、不动取值判据。壳侧走
`args_bytes()`/`prog_name_bytes()`，新立 `fn banner` 与 `fn warn_bytes`（三段形自己拼），
`FollowTarget.path`/`label()` 交回 `&[u8]`；开档、按名字查身份、`freopen` 三条腿仍走 `&str` 口。
**`warn_bytes` 是本族第三份自拼的三段形**（`split` 的 `err3`、`sort` 的自拼、`tail` 的
`warn_bytes`），而形状的权威在 `os/libs/minix-types` 的 `err_line`——它到现在只有 `&str` 口
（现量 `grep -rn "fn err_line" os/libs | wc -l` → 1，`grep -rl "minix_sys::err_line" os/commands | wc -l`
→ 3）。按「一次只修一条」本轮不动它，登记 `TEXTFILTER-WARN-BYTES-DUPLICATES-ERR-LINE`。

**门账**：库内 `window` **628 → 629**（新立
`the_tail_value_and_operand_legs_hand_back_the_argv_bytes`，六段），壳内单测 **17 → 18**（新立
`the_banner_names_the_operand_bytes_and_only_the_second_one_gets_a_blank_line`），`tail_face`
**24 → 25**（新立 `the_offset_echo_the_token_echo_and_the_file_name_carry_the_argv_bytes`，七段，
末段按当前形状钉住盘外那条 `&str` 口），跨壳表二十行 → **二十一行**
（`("tail", TAIL, 1, b"tail: unknown option -- \xff\n")`），整包 69 段 **1276 → 1279 passed / 0 failed**。
夹具侧 76 处 `parse_tail_args` 的字面量包进既有 `fn argv`、22 处期望位改 `b"…".to_vec()`、
文本形助手 `fn operands` 删除改用既有 `operands_bytes`；`tail_face` 新写 `fn run_raw`
（`OsStr::from_bytes` 交原始 argv，并在临时目录里造一枚档名带裸字节的真档）。

**负控六枚（`tmp/tail-469/negctl.py`，读数 `negctl-K1.log`..`negctl-K6.log`、汇总
`negctl-summary-merged.txt`）各咬 2/4/2/2/1/1 档**：K1（壳侧 argv 退回文本缝）咬跨壳表 + 面门而
**库内 629 全绿**（「库门对 argv 缝没有牙」第八次量化）；K2（字母位按字符取）是唯一穿透到库里的一枚；
K3（值位折一次）与 K4（**改写位**折一次）各咬库内 + 面门——K4 是本轮新出现的层间分布：
程序自己造的串也要按字节，注入它同样只在库门露牙；K5 只咬壳内那枚纯函数门（上面已说）；
K6（诊断行档名折一次）只咬面门。每枚前后按整文件 md5 与 `tmp/tail-469/post/`（完整相对路径）成对
复核，跑完 `git status` 只剩改动的那四件。

**A/B 对参照件 `tmp/tail-416/tail` 跑矩阵五十五形，SAME=55 / DIFF=0**（`tmp/tail-469/ab.log`），
比退码、标准输出、标准错误三件事；另按类分开计数：饱和族四形、档名带裸字节的真档一形。非空跑
核验先做：五十五形里两侧同退 0 的有 28 形。矩阵**不含 `-f`/`-F`**：C 的 `forward()` 在跟随时不
返回（`forward.c:191`），两侧都只能被超时杀掉，比出来的不是判据；跟随的形状由
`tests/tail_face.rs` 用真临时档自己钉。参照件归属当场验过：`-Z` 交回
`tail: unknown option -- Z` 加一行 usage；`tmp/tail-follow-ref/tail_ref` 打的是 glibc 那句带引号的
`invalid option -- 'Z'`，判为不可用（沿用批次四百零七那条规矩）。

**六起工具自错与一次「没跑就写进台账」**（这一条最重，因为它差点变成账面事实）：① 三侧现量第一次
跑的是 `rc=127`——脚本在临时目录里用**相对路径**起件，`cd` 之后路径失效，二十二形全部「command
not found」，而两侧对比的汇总会把「两边都 127」算成一致；判据：对照脚本里的可执行文件一律绝对
路径，且第一次跑完先看退码分布再看得分。② A/B 的裸名档那一形**差点假绿**：`snapshot()` 内部先
`prepare()` 重建目录，把摆好的 `a\xff.txt` 删了，于是两侧都报「档不存在」而读数为「一致」；把
「摆档」与「跑一次」拆开才看到参照件真的开得了它——**夹具若在被测那一步里重建目录，它就管不了
目录里的东西**。③ 一处删改式 `Edit` 的替换文本没带上被锚定的函数头两行，把既有那枚
`unknown_letters_and_bad_counts_are_refused_not_leaked` 吞掉，靠当场 `grep -n 函数名` 复验才发现。
④ 把 `\xff` 写进 Rust 的**文本**串（断言消息里的档名），编译期报 `out of range hex escape`——
字节转义只在 `b"…"` 里合法；上一笔记的是 C 八进制直译进 Rust 字节串，同一族的另一种形态。
⑤ 负控 K2/K3/K4 第一版全是 `BUILD-FAIL`：注入串用了裸 `String`，而 `src/window.rs` 顶层那行
`use alloc::string::String` 正是本轮当未用 import 删掉的——**注入串必须自带它要依赖的每一项限定**，
不能假设宿主文件里有 import。⑥ 台账草稿里我写下「`tail -n +3.5 a.txt` 参照件接受而从第三行之后
打，本壳拒绝 ⇒ 登记一形分岔」，理由只是 C 的 `ARG` 判 `if (*p)`（`:89`）于是推测那一份 `strtoll`
把 `.5` 也吞了进去。**这条推断没有任何命令与输出撑着**，把它放进 A/B 现跑之后直接被否证：三侧
完全同形（都报 `tail: illegal offset -- +3.5`、退 **1**）。于是那一节改成「自我更正」并把这一形
留在矩阵里当反向对照。判据仍是批次四百六十四立的那条：**台账里每一句「参照件现量」都必须对应
一条跑过的命令与它的输出**；只从 C 源码推出来的，只能写成「按源码应如此、未量」。

**五道收尾门逐条读数**（`2026-10-30`，码笔 `77f7c8f56`、文档笔 `b9fdf9706`）：①
`bash tools/todo-reconcile-scan.sh` → 13 项里 2 项 DRIFT（`探针待滚除（行）` 0 vs 基线 13、
`探针命名族（行）` 32 vs 18），**同一条命令在主树跑出完全相同的两项**（上一笔已核，本笔复跑同数）
⇒ 非本线引入，归 01-stage-kernel 那侧的探针台账；②
`python3 tools/notes-link-check.py rewrite-notes/` → 764 个 md、**断链 131**，与上一笔同数，
集合差按 `(文件, 目标)` 逐对核（忽略行号，因为本轮在 `todo.md` 里插入了一百多行）：**新增为空**；
③ `bash tools/todo-staleness-check.sh …TODO-LEDGER-OPEN.md` → **PASS（0 项）**；
④ `bash tools/doc-style-lint.sh --diff` → 3 个文件 / 157 个新增行、**error 级 0**（中途 2 条：
一句拿「过去怎么写的」当主语叙述修复史命中 SL-5、一句把临时目录写成过程用语命中 SL-11，都改成按行为直接表述）；
⑤ `bash tools/check-staged-inflight.sh` → 退 0，避让清单 1 条（`TODO-3ARCH-PARITY-20261006.md`）
未触碰。

**门读数补全**：宿主整包 `cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` →
**69 段 / 1279 / 0**（`tmp/tail-469/pkg-2.log`；负控还原后 `pkg-3.log`、格式化后 `pkg-4.log` 三次
同读数、退出码各 0）；容器权威门按 §7.3 三挂载配方 → 退 0、**69 段 / 1279 / 0** 与宿主同读数
（`container-gate.log`）；三架构 `cargo check --offline -j 1 --target {x86_64-unknown-none,
aarch64-unknown-none, riscv64gc-unknown-none-elf} -p minix-textfilter --bins` 各退 0、错误 0
（`arch-*.log`）——**一次通过**：上一笔立的那条「动了壳侧字符串拼装就当场跑三架构」本轮前置执行，
`ToString` 随 `warn_bytes` 一起 import，没有重演批次四百六十八那一红。`rustfmt --edition 2024
--check` **原地**量：HEAD 那三份各 0 处 `Diff in`（`fmt-head-*.log`），本轮改出的三份格式化前是
7 / 1 / 3 处，格式化后四件全 0；`cargo clippy --offline -j 1 --all-targets -p minix-textfilter`
退 0，与主树（合并态）同参数对照两树都是 149 行 / 去重 61 条 / 位置 44 处，集合差只有
`src/window.rs` 一条既有警告在 213 → 211 的**位移**（本轮删掉文件头两行未用 import），零新警。

**进度与在册（全部现量）**：参数缝半已落 **二十四支**；`textfilter` 余 **15** 支
（`grep -rl "support::args()" os/commands/usr-bin/textfilter/src/bin/ | wc -l`），其中带字母位
诊断面 **3** 支（`unexpand`、`unifdef`、`vis`），**切片 52 建议 `unexpand`**（字母序第一支）。
命令面口径本笔起固定为现量命令 `grep -rl "support::args()" os/commands --include=*.rs | wc -l`
→ **88**（按 crate 分布见 18 篇台账那一节）；台账前几笔写的 86/87 与更早的「40 处 / 29 件」
一样**未附命令、现量复现不出**，本笔起一律带命令入账。同口径 `line_bytes` → **83 处 / 34 件**，
`args_bytes()` 调用点 → **25** 处；渲染半那条 grep 仍 **0** 行。**本笔新立两枚**：
`TAIL-OFFSET-SATURATION-OUTCOME`（四形两方向，与 `SPLIT-OVERFLOW-COUNT-ACCEPTED` 同属一件
饱和语义裁决）与 `TEXTFILTER-WARN-BYTES-DUPLICATES-ERR-LINE`（三份自拼的三段形等 `err_line`
换字节形后并回去）。**在册建议更新**：饱和语义那一条现在有 `split` 三形 + `tail` 四形的读数，
建议**先裁口径再动码**；其余六条不变（`JOIN-STDIN-LEG-BYPASSES-DUAL-SURFACE`、`jot` 那三条上真机、
`NL-ATTACHED-VALUE-LEADING-BLANK-STRIPPED`、`PR-EGETOPT-NUMBER-TOKEN-CONTINUATION`、
`SORT-RECORD-CONTENT-TEXTSEAM`、`TEXTFILTER-07-TABLE-UNESCAPED-PIPES`）。独立轮在册不碰的还有
`ARGV-BYTE-PATH-IN-LIBS`（本笔再添一条腿的读数：档名带裸字节的真档开不了）。
**取证回抄照办**：`tmp/tail-469/` 全目录就在主树 `tmp/` 下，无需另抄。

**合回条件（本笔当场量）**：`rewrite` 自基线 `6ea1eae7e` 起没有新提交——
`git log --oneline 6ea1eae7e..rewrite` 交回空、`git merge-base --is-ancestor 6ea1eae7e rewrite`
退 0 ⇒ 纯快进、零冲突；主树合并路径的 `git status --porcelain` 四处交回空。快进之后的主树复跑
读数另记一笔合回补记。

**合回读数（第一百八十三笔补记，批次四百六十九）**：快进动作之前把上面那两条命令复跑一遍
——`git log --oneline 6ea1eae7e..rewrite` 仍交回空、`git merge-base --is-ancestor 6ea1eae7e rewrite`
仍退 0 ⇒ `git merge --ff-only claim/GETOPT-REPORT-BYTE-LETTER-qoder` 纯快进成功
（`8 files changed, 814 insertions(+), 193 deletions(-)`，主树 `rewrite` 顶到里程碑笔 `991e8638f`），
主树 29 条别线在制项一件没被牵进暂存区（本线四路径的 `git status --porcelain` 合并前后都交回空）。
逐笔验祖先四笔全 ✓（码笔 `77f7c8f56`、文档笔 `b9fdf9706`、锚点更正 `a37988e4c`、里程碑笔
`991e8638f`）。主树当场复跑 `cargo test --offline -j 1 --no-fail-fast -p minix-textfilter`
→ **69 段 / 1279 / 0**、退出码 0（`/home/xzhao/github/minix-rs/tmp/tail-469-maintree.log`），
逐目标与领取树同数：库内 629、`bin/tail` 18、`tail_face` 25、`bin_support_face` 4。
取证目录 `tmp/tail-469/`（三侧现量、A/B、负控六枚日志与汇总、门日志、`post/` 四件源码快照、
改前与改后两枚件）本来就在主树 `tmp/` 下，无需回抄。

### 第一百八十四笔（批次四百七十 · A2 参数缝半切片 52：`unexpand` 的六形 tabstop 回声与操作数档名交回原字节；同一件「饱和怎么办」在 C 自己手里有三种答案）

**这一支的形状**：`minix3/usr.bin/unexpand/unexpand.c:86` 的 optstring 是 `"at:"`——七个字母里
只有 `-a` 与带值的 `-t`。`-a` 与 `-t` 互斥（`:88-95` 的 `if (nstops) usage()` 与 `if (all) usage()`），
所以 `-at4` 是 usage；`usage()` 一行、大写 `Usage:`、带 `getprogname()`、退 1（`:69-73`）。
`-t` 的值不整体使用，而是由 `strsep(&optarg, ", \t")`（`:96`）**就地切开**成一支一支，空字段跳过，
每支交给 `strtoul(tab, &ep, 0)`（`:100`），两句坏值报文是
`errx(EXIT_FAILURE, "Invalid tabstop \`%s'", tab)`（`:101-103`）与
`errx(... "Bad tabstop spec \`%s', must be greater than the previous \`%zu'", tab, tabstops[nstops-1])`
（`:108-112`）——**两处的 `%s` 打的都是切出来的那一支，不是整张值**。选项环之后还有一行
`fprintf(stderr, "%lu %zu\n", i, tabstops[i])`（`:124-125`）的调试输出，它在外部行为里，本壳照搬。
操作数走 `freopen(argv[0], "r", stdin)`，失败即
`err(EXIT_FAILURE, "Cannot open \`%s'", argv[0])`（`:129-130`）三段形。

**三侧现量**（`tmp/unexpand-470/measure.log`，十八形 × 三侧）：改前的件 **9/18** 与参照件
`tmp/unexpand-312/unexpand_ref` 逐字节同形，改后的件 **17/18**。翻正的八形是字母位（libc 那句
`unknown option -- %c`）、六形 tabstop 回声（附值 `-t<ff>`、分词 `-t <ff>`、数字尾巴 `-t4<ff>`、
混合表 `-t8,4<ff>`、空格分隔 `-t8 <ff>`、逗号分隔 `-t,<ff>`）与操作数那句 `Cannot open` 的档名位。
剩下那一形是**档名本身带裸字节的真档**：参照件退 0 并打出 `\tx`，本壳只能报
`unexpand: Cannot open \`a\xff.txt': No such file or directory` 退 1——报文里的名字现在对了，
`freopen` 那一步还不对（`os/libs` 那条 `&str` 口，在册 `ARGV-BYTE-PATH-IN-LIBS`）。

**「折叠改不了判决」这一条在 `unexpand` 上比前两支更硬**。分隔集是 `", \t"` 三枚 ASCII 字节，
数字判据只认 ASCII 数字与 `0x`/八进制前缀，而 `from_utf8_lossy` 换进去的三枚字节恒在
`80..=BF`——**这是 UTF-8 的构造区间，不是这批读数的巧合**，所以切点位置与命中判据都不可能被
折叠改变。与 `split`、`tail` 两笔合起来看，参数缝半里已经有三支持有这条结论，而它们各自的理由
都指向同一处编码性质；这条理由留在这里，是为了将来换解析器时知道要去重验什么。
**另有一条「按构造没有可观察差」**：`Bad tabstop spec` 那一臂要等到某一支已经解析成数之后才会
走到，所以它的 `%s` 位**不可能**带非法字节。这一条写进新立库门的注释，与批次四百六十七那枚
无分辨力探针同族——让下一次看到「这一枚注入没红」时能立刻分清是覆盖面有洞还是本形无差。

**本轮最值钱的一条是饱和问题的第三种答案，而且是 C 自己给的**：`-t 99999999999999999999999`
两侧**同判**（都报 `Invalid tabstop \`…'` 退 1，读数 `tmp/unexpand-470/overflow.log`），因为
`unexpand.c:101` 写的是「`*ep` 非零，或 `errno` 已是 `ERANGE` 且交回 `ULONG_MAX`」——**这一支
查了 `errno`**。而上一笔的 `split`（`strtoull`，不查 `errno`）与本笔前一笔的 `tail`
（`strtoll`，不查 `errno`）都是饱和之后继续跑，于是接受一长串本层判非法的数字。同一个问题在
三份 C 里有三种写法，所以前面登记的那件裁决（`SPLIT-OVERFLOW-COUNT-ACCEPTED` +
`TAIL-OFFSET-SATURATION-OUTCOME`）**做不成一条全局口径，只能逐命令照各自那一句改**——
`unexpand` 已经与 C 同，不需要动。

**顺手量清内容半的一处双重编码，登记 `UNEXPAND-LINE-CONTENT-TEXTSEAM`**：`unexpand -a` 对
标准输入 `   x<ff>y` 的读数，参照件原样穿过，本壳交回 `   x` + `c3 af c2 bf c2 bd` + `y`。
两步叠加：`src/bin/unexpand.rs:fn emit_tabified` 先 `String::from_utf8_lossy`（`ff` → `ef bf bd`），
`src/unexpand.rs:fn unexpand_line` 又以 `out.push(byte as char)`（`:175`、`:193`）把每枚字节当码位
再编一次。这比 `SORT-RECORD-CONTENT-TEXTSEAM`（单纯折一次）更靠里，也说明内容半那一档的修法
必须同时改掉这两处，只改一处仍然对不上。本轮不动（它比参数缝大一档）。

**落码层**：库侧 `parse_unexpand_args` 收 `&'a [&'a [u8]]`、`UnexpandArgs.operands` 改
`Vec<&'a [u8]>`、`parse_stops` 收 `&[u8]` 且分隔判据写成
`matches!(byte, b',' | b' ' | b'\t')`、`base_zero` 按字节走前缀、两枚 `StopRejection` 的值位改
`&'a [u8]`；壳侧走 `args_bytes()`/`prog_name_bytes()`，两句 tabstop 报文与 `Cannot open` 那句
按字节拼——后者是本族**第四份**自拼的三段形（`split` 的 `err3`、`sort` 的操作数句、`tail` 的
`warn_bytes`、`unexpand` 的这一份），`TEXTFILTER-WARN-BYTES-DUPLICATES-ERR-LINE` 的计数随本笔更新。

**门账**：库内 `unexpand` 模块 **+1**（新立
`the_stop_spec_and_the_operand_carry_the_argv_bytes`，六段，含「回声带切出来的一支而不是整张值」
与那条按构造说明），`unexpand_face` **14 → 15**（新立
`the_tabstop_echoes_and_the_operand_name_carry_the_argv_bytes`，九段，末段按当前形状钉住裸名档
那条 `&str` 口），跨壳表二十一行 → **二十二行**
（`("unexpand", UNEXPAND, 1, b"unexpand: unknown option -- \xff\n")`），整包 69 段
**1279 → 1281 passed / 0 failed**；`bin/unexpand` 的 3 枚不变。夹具侧 27 处调用包进新立的
`fn argv`、26 处值位与期望位改字节面量、`é` 那一形写成字面字节 `c3 a9`。

**负控五枚（`tmp/unexpand-470/negctl.py`，逐枚 `negctl-K1.log`..`negctl-K5.log`、汇总
`negctl-summary.txt`）按红门数各咬 2/5/2/1/1**：K1（壳侧 argv 退回文本缝）咬跨壳表 + 面门而
**库内全绿**——「库门对 argv 缝没有牙」第九次量化；K2（字母位按字符取）是唯一穿透到库里的一枚，
咬库内两枚 + 跨壳表 + 面门两枚；K3 把回声位从「切出来的一支」换成「整张 `-t` 值」——
**一个看上去完全合理的实现选择，被库门与参照件双双否证**（C 打的是 `strsep` 之后的 `tab`）；
K4（壳侧值位折一次）与 K5（档名折一次）各只咬面门。

**A/B 对参照件跑矩阵三十九形 SAME=39 / DIFF=0**（`tmp/unexpand-470/ab.log`；件经
`tmp/unexpand-470/bin-ref/unexpand` 同名软链起，程序名段可直比），非空跑核验先做：
三十九形里两侧同退 0 的有 16 形；另按类分开计数两族（裸名真档 2 形、内容半双重编码 2 形）。
归属验过：`-Z` 交回 `unexpand: unknown option -- Z` 加一行大写 `Usage:`。

**五起工具自错，三起属「读数被工具自己的形状骗了」**：① rustfmt 的日志名直接拼了带斜杠的
文件路径，重定向失败，那批「exit=1、diffs=空」是失败的 `>` 的退出码而不是 rustfmt 的——
判据：**日志名里不许出现斜杠**（`tr / -`），且退出码要和「这条命令真的跑了吗」一起读。
② **在负控驱动跑动期间改了同源文件**：为量 HEAD 的 rustfmt 基线做的那次「覆写 + 从快照还原」
把快照之后新加的一枚库门断言静默还原掉，驱动随后按它自己读到的内容继续跑，K3 那一步的库内
计数因此是 629 而不是 630。判据：**负控在跑时同源文件不许再动，快照必须重打于最后一次编辑之后**；
廉价的防线是每写一段断言就 `grep -c` 一次自己的锚点。③ 容器门「67 段 vs 宿主 69 段」是
`grep -cE "^\s+Running"` 被一行交错输出破坏的假读数（容器里那条 `Running unittests src/bin/col.rs`
与上一行测试黏在同一行）；按 `test result` 行数与 passed 总和两树都是 69 / 1281。判据：
**跨树比段数要比 `test result` 行与总和，不比 `Running` 行**。④ 新库门第一版把 `-t8,\xff` 的
期望写成 `NotIncreasing{spec: b"\xff"}`，当场红——那枚字节根本解析不成数，走的是 `Invalid`，
又一次「期望值必须先量后写」。⑤ 改前/改后两枚件的 md5 与源码 md5 一度串行混淆，最后靠**行为**
（同一形一个打 `ef bf bd`、一个打 `ff`）确认配对；判据：md5 只复核同一对象前后一致，
不用来区分两个对象。另有一处账面不实由本笔更正：码笔 `40b3974bb` 的提交信息把负控写成六个数
（实为五枚 2/5/2/1/1）、把参照件写成 `tmp/unexpand-311`（实为 `tmp/unexpand-312`）。

**五道收尾门逐条读数**（`2026-10-30`，码笔 `40b3974bb`、文档笔 `7596f0d29`）：①
`bash tools/todo-reconcile-scan.sh` → 13 项里 2 项 DRIFT（探针两项），与主树同读数、非本线引入；
② `python3 tools/notes-link-check.py rewrite-notes/` → 764 个 md、**断链 131**（连续四笔同数）；
③ `bash tools/todo-staleness-check.sh …TODO-LEDGER-OPEN.md` → **PASS（0 项）**；
④ `bash tools/doc-style-lint.sh --diff` → 3 个文件 / 120 个新增行、**error 级 0**；
⑤ `bash tools/check-staged-inflight.sh` → 退 0，避让清单 1 条未触碰。

**门读数补全**：宿主整包 → **69 段 / 1281 / 0**（`pkg-1.log`、`pkg-2.log`、`pkg-3.log` 三次同读数、
退出码各 0）；容器权威门按 §7.3 三挂载配方 → 退 0、**69 段 / 1281 / 0**（`container-gate.log`）；
三架构各退 0、错误 0（`arch-*.log`，动壳侧字符串拼装后当场跑，一次通过）；
`rustfmt --edition 2024 --check` 原地量：HEAD 三份各 0 处 `Diff in`，本轮三份格式化前 6 / 0 / 3
与 1 处，格式化后四件全 0；`cargo clippy` 退 0，两树同为 149 行 / 去重 61 条 / 位置 44 处，
`src/unexpand.rs` 那两条 saturating-subtraction 存量**两树同位**（177/180），零新警。

**进度与在册（全部现量）**：参数缝半已落 **二十五支**；`textfilter` 余 **14** 支、命令面 **87** 支
（`grep -rl "support::args()" os/commands --include=*.rs | wc -l`），`line_bytes` 同口径
**84 处 / 34 件**，`args_bytes()` 调用点 **26** 处，渲染半那条 grep 仍 **0** 行。带字母位诊断面
只剩 **2** 支（`unifdef`、`vis`），**切片 53 建议 `unifdef`**（`-i`/`-o`/`-I`/`-d` 都带值，
`-o` 是输出档名、`-I` 是 include 档名，报文与开档两条腿都在字节链上）。本笔新立
`UNEXPAND-LINE-CONTENT-TEXTSEAM`；在册建议里那条饱和裁决按上面所说**改成逐命令照各自那一句**，
不再是单一口径。**取证回抄照办**：`tmp/unexpand-470/` 全目录已在主树 `tmp/` 下。

**合回条件（本笔当场量）**：`rewrite` 自基线 `5365ee593` 起除本线四笔之外没有新提交——
`git log --oneline 5365ee593..rewrite` 交回空、`git merge-base --is-ancestor 5365ee593 rewrite`
退 0 ⇒ 纯快进、零冲突；主树合并路径的 `git status --porcelain` 交回空。快进之后的主树复跑
读数另记一笔合回补记。

**合回读数（第一百八十四笔补记，批次四百七十）**：快进前复跑上面两条命令，仍分别是空与退 0
⇒ `git merge --ff-only claim/GETOPT-REPORT-BYTE-LETTER-qoder` 纯快进成功
（`8 files changed, 562 insertions(+), 92 deletions(-)`，主树 `rewrite` 顶到里程碑笔 `2f7c4134a`）。
逐笔验祖先三笔全 ✓（码笔 `40b3974bb`、文档笔 `7596f0d29`、里程碑笔 `2f7c4134a`）。主树当场复跑
`cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` → 退出码 0、**69 段 / 1281 / 0**，
与领取树最终态逐数相同（库内 630、`unexpand_face` 15，
`/home/xzhao/github/minix-rs/tmp/unexpand-470-maintree.log`）。本笔的另一处教训也留在读数里：
主树第一次复跑时我用 `awk -F'[ ;]' '{s+=$5}'` 求总通过数，读出 `passed=0`——分隔符切出来的字段
位置不对，退出码 0 与 `test result` 行数 69 才是可信的那两项，改回按空白分列取第 4 字段才得到 1281。
判据：**汇总口径的读数要与它的字段定义一起复核，一次切错就把整段总和读成 0**。
取证目录 `tmp/unexpand-470/`（三侧现量、A/B、负控五枚、门日志、`post/` 快照、改前与改后两枚件、
`overflow.log`）已在主树 `tmp/` 下，无需另抄。

### 第一百八十五笔（批次四百七十一 · A2 参数缝半切片 53：`unifdef` 的操数名、符号名与 `-o` 名交回原字节；一处「读码读出来的因果」被实测否掉，一处内容半的分叉按现量立住）

**这一支的形状**：`minix3/usr.bin/unifdef/unifdef.c:273` 的 optstring 是
`"i:D:U:I:o:cdeklst"`——五个带值字母（`i`、`D`、`U`、`I`、`o`）加七个不带的，大小写混排。
三处 `%s` 回声都在字节上：`err(2, "can't open %s", filename)`（`:337`，输入操数）、
`err(2, "can't open %s", ofilename)`（`:365`，`-o` 的值，它同时也是 `fopen` 的参数〔`:364`〕）、
`errx(2, "%s unchanged", ofilename)`（`:509`、`:514`、`:1034`，本壳未建模那条支路）。
符号名那一族的判据也按字节：`skipsym`（`:928`，判据是 `:262` 的 `endsym`，只放过字母、数字与 `_`）
吃出一段名字，`addsym` 以它的落点当尾巴起点（`:977`）再看尾巴是 `'\0'` 还是 `'='`，
查表比的是 `strlcmp`（`:951`，定义 `:998`）；`-i` 那一支先吃**一个字节**选边（`:281`）。
`usage()` 那一行的程序名**写死在串里**（`:375-377` 没有 `getprogname()`），而 libc 那两行与
`err`/`warnx` 的前缀跟着 `getprogname()`，且 `exit(2)`（`:378`）——**这一支的 usage 退 2**，
跨壳表那一行钉的是 2 而不是同族常见的 1。

**参照件归属本轮现核了两遍**（`tmp/unifdef-471/attribution.log`）：同一个件经**同名软链**
`tmp/unifdef-318/unifdef` 起时 `-Z` 交回 `unifdef: unknown option -- Z` + usage 行、退 2；
经本轮那个改名软链 `bin-ref-unifdef` 起时**第一行跟着 argv[0] 变成 `bin-ref-unifdef`、
第二行的 `unifdef` 一个字都不动**。上面那句「两行来历不同」以前是读码得到的，这次是量到的。

**三侧现量**（`tmp/unifdef-471/measure.log`，十三形 × 三侧）：改前的件 **9/13** 与参照件
`tmp/unifdef-318/unifdef_ref` 逐字节同形，改后的件 **11/13**。翻正的是字母位与输入操数那句
`can't open`；`-o` 的名字位现在也按字节（句子那一段还差一截，见下面第一条在册分叉）。
三形判据核实为同判：`-D\xffA` 与 `-i\xff` 两侧都只打 usage 一行、`-DA=\xff` 两侧都接受。
**这一支也没有「折叠改了判决」的形**，理由与上一笔同源且更硬：名字的合法字符集是 ASCII 三类，
裸字节是**截断点**而不是名字的一部分，所以折叠既不改接受也不改拒绝，只改报出去的那串字节。

**本轮最值钱的是一处自纠**：台账初稿只读了 `src/unifdef.rs:fn find_sym` 里那句
`core::str::from_utf8(&b[cp..end]).ok()?`，就写成「档名里带裸字节的符号在 `-s` 那一形里整个消失」，
并据此新立一枚在册项——一次都没量。补跑之后两头都不成立：本壳 `-s` **一行都不打**（谈不上
「某个符号消失」），而名字位按两边的同一取法恒为 ASCII（参照件现量：`-s -DA=\xff` 与档里的
`#ifdef A\xffB` 都只打 `A`）。册项删除、码内注释同步改掉，判据入台账：**读码得到的「应该如此」
不是观察，立册之前必须有一条跑过的命令兜着**；给不出观察的分支只能写成「按源码应如此、未量」。

**`-s` 那一族因此被量准了**（`tmp/unifdef-471/symcheck.log`，二十次调用、两侧对照）：C 在 `-s` 下
唯一的打印点就是 `findsym` 里那句 `printf`（`:948-949`），于是**每次 `findsym` 调用出一行**，
而调用点有两族——`addsym` 在建表之初先 `findsym(sym)`（`:969`）⇒ 命令行每个 `-D`/`-U`/`-iD`/`-iU`
的名字各回声一行、且发生在 `=` 拆值之前（现量 `-s -DFOO=BAR` 打 `FOO`、`-s -UFOO` 打 `FOO`）；
`evalstr` 扫档时对每个标识符调它（`:760`、`:771`）⇒ 同一符号出现几次打几次、与表里有没有它无关
（现量 `-s -DA` 扫 `#ifdef A`/`#ifdef B` 那份档打 `A`/`A`/`B` 三行，本壳打空，两侧同退 0）。
在册项 `UNIFDEF-SYMLIST-OUTPUT` 第三次现量，且比批次三百一十八那次多出「命令行名字也回声」这一条。

**真正立住的内容半分叉在档里的行**（`tmp/unifdef-471/content-seam.log`）：同一份档
（`#ifdef A / x / #else / one \xff tail / #endif / plain \xff line`）四形对照，参照件把保留下来的
那行原样交回单个 `377`，本壳交回 `357 277 275`（U+FFFD 三字节），退码两侧同 1——`-DA`、`-iDA`、
`-l -DA` 三形都是这个差，`-c -DB` 那形两侧都空。成因是缝在决定半的入口：壳读档先
`String::from_utf8_lossy`，而 `Unifdef::run` 收 `&str`，折发生在读档那一刻、比任何选项都早。
登记 `UNIFDEF-LINE-CONTENT-TEXTSEAM`（与 `UNEXPAND-LINE-CONTENT-TEXTSEAM`、
`SORT-RECORD-CONTENT-TEXTSEAM` 同族），本轮不碰。

**落码层**：库侧 `parse_unifdef_args` 收 `&'a [&'a [u8]]`、`UnifdefArgs.operands` 改
`Vec<&'a [u8]>`、`Sym.name`/`Sym.value`/`Options.output` 改 `Vec<u8>`、`symbol_run` 与
`add_symbol` 的 spec 收 `&[u8]`、`-i` 臂按一枚字节选；壳侧走 `args_bytes()`/`prog_name_bytes()`，
开不了的操数那一句由新立的 `fn err_can_open` 自己拼三段形（本族第三份自拼助手，`sort` 另有内联
一处，`TEXTFILTER-WARN-BYTES-DUPLICATES-ERR-LINE` 的计数随本笔更新）。

**门账**：库内 `unifdef` 模块 **21 → 22**（现量 `grep -c '#\[test\]' src/unifdef.rs`；新立
`the_symbol_and_path_legs_hand_back_the_argv_bytes`，七段：名字位、值位、`-o` 名、操数、字母位、
`-i` 臂、环不重排），`unifdef_face` **14 → 15**（新立
`the_operand_and_output_names_carry_the_argv_bytes`，六段，末段按当前形状钉住 `-o` 落盘时那个折过
的档名），跨壳表二十二行 → **二十三条**（`("unifdef", UNIFDEF, 2, b"unifdef: unknown option -- \xff\n")`），
整包 69 段 **1281 → 1283 passed / 0 failed**。篇首那句「库测试 630 个」随本笔回填 **631**
（现量 `cargo test --offline -j 1 -p minix-textfilter --lib`）。夹具侧 27 处 `parse_unifdef_args`
包进新立的 `fn argv`、13 处 `add_symbol` 的 spec 与 6 处期望位改字节面量。

**负控三枚（`tmp/unifdef-471/negctl.py`，逐枚 `negctl-K1.log`..`negctl-K3.log`）按红门数各咬 2/4/1**：
K1（壳侧 argv 退回文本缝）咬跨壳表 + 面门而**库内 631 全绿**——「库门对壳侧 argv 缝没有牙」第十次量化；
K2（字母位改按字符取）**第一遍库内全绿**，按判据先怀疑覆盖面有洞而不是相信这条腿没差，查出新立的库门
只覆盖了 `usage()` 那两支与值位/档名位、没覆盖字母臂的取法，补一段 `parse_unifdef_args(&[b"-\xff"])`
的断言后重跑同一枚注入，**当场咬库内一枚**；K3（`can't open` 的档名位折一次）只咬面门。
改完注释与台账之后三枚重跑一遍（`negctl-driver2.log`），读数一字不差仍是 2/4/1，注入还原后四件 md5
与跑前清单相符（`post2/md5-before-negctl.txt`）。

**A/B 对参照件跑矩阵四十五形 SAME=45 / DIFF=0**（`tmp/unifdef-471/ab.log`；两侧同退 0 的 5 形先数过），
撞见的两形在册分叉都有主人：① `UNIFDEF-D-DEBUG-OUTPUT`——`-d` 在 C 不是纯忽略，它把 `findsym` 与
状态机每一步用 `debug()` 刷到标准错误（参照件对那枚三行的小档刷了 **13** 行，`findsym` 一句加六对
`parser`/`process`，`tmp/unifdef-471/debug-ref.err`），本壳只当合法空字母；
② `UNIFDEF-O-ERRNO-LEG`——`-o` 开不了时 C 那句 `err` 的第三段是 `strerror`，本层 `write_file` 交回
`Result<(), ()>` 没有 errno 通道，于是本壳打 `Undefined error: 0`（恰是 C 的 `err` 在 `errno == 0`
时的句子形状）而参照件打 `No such file or directory`，名字位两边都是原字节。

**六起工具自错**（逐条见 18 篇台账本笔）：① bash 里 `$FFx` 被当成变量名，那枚裸字节根本没进 argv，
第一轮「三侧同形」是假的；② 同一枚混淆把改前件/改后件与源码的 md5 串着看；③ 负控驱动的目标标签
还写着上一笔的 `bin_unexpand`（读数没错、标签会误导）；④ `parse_unifdef_args(&[b"-D\xff"])` 的期望
先写成「接受」，当场红；⑤ `post/` 快照打早了，负控跑完拿它复验时报 FAILED；⑥ **A/B 脚本末尾那几行
在册形的打印语句里压根没有 `out=` 栏**，标准输出的差异落不进日志，而我在它旁边写了一句把它解释掉
（「早先两形的标准输入是空的」——那两形带的是档名操数、`in.txt` 非空，连前提都不对）。补栏重跑，
原日志存档 `ab-stdout-blind.log`。判据：**少了的那一栏会让读日志的人以为看见了全部**，而含糊的解释
又是自己写的，就更看不见。另有一处账面用字要更正：码笔 `3d0497ead` 的提交信息把跨壳表写成
「二十三轮」，正字是「二十三条」。

**门读数（全部 L1，`2026-10-30`，基线快照 `ae4aadcfb`）**：宿主整包 **69 段 / 1283 / 0** 四次同读数
（`pkg-2.log`、`pkg-3.log`、`pkg-4.log`、`pkg-5.log`，退出码各 0）；容器权威门按 §7.3 三挂载配方
→ 退 0、**69 段 / 1283 / 0**（`container-gate3.log`，前两次同读数）；三架构各退 0、错误 0
（`arch3-*.log`，动壳侧字符串拼装后当场跑，一次通过）；`rustfmt --edition 2024 --check` 原地量：
HEAD 三份各 0 处 `Diff in`，本轮三份格式化前 7 / 2 / 1 处、格式化后四件复量各 0 处（`fmt3-*.log`）；
`cargo clippy --offline -j 1 --all-targets -p minix-textfilter` 退 0、警告 **149** 行与主树同数，
去路径后的唯一句子两树都是 **37** 条、`diff` 为空，位置清单两树都是 **80** 处、差集只有 `unifdef.rs`
的四处行号漂移（本轮改注释所致），零新警。

**五道收尾门逐条读数**：① `bash tools/todo-reconcile-scan.sh` → 十三项里 2 项 DRIFT（探针两项），
与主树同读数、非本线引入；② `python3 tools/notes-link-check.py rewrite-notes/` → 764 个 md、
**断链 131**（连续五笔同数），本轮三件文档另按窄口径做过改前/改后集合对照，两侧都是 **6** 条、
目标串集相同 ⇒ **新增 0**；③ `bash tools/todo-staleness-check.sh …TODO-LEDGER-OPEN.md` →
**PASS（0 项）**；④ `bash tools/doc-style-lint.sh --diff` → 3 个文件 / 132 个新增行、**error 级 0**；
⑤ `bash tools/check-staged-inflight.sh` → 退 0，避让清单 1 条未触碰。

**进度与在册（全部现量）**：参数缝半已落 **二十六支**；`textfilter` 余 **13** 支
（`grep -rl "support::args()" os/commands/usr-bin/textfilter/src/bin/ | wc -l`）、命令面 **86** 支
（`grep -rl "support::args()" os/commands --include=*.rs | wc -l`）；`line_bytes` **86 处 / 34 件**
（本笔给 `unifdef` 的壳添了一处调用点，上一笔写的 85 是改前的数）。**带字母位诊断面的只剩 `vis` 一支**
——逐支数 `grep -c Getopt`：`vis.rs` 七处，其余十三支（含同件的 `unvis`）全零。**切片 54 = `vis`**
（批次四百七十二）是本族参数缝半的最后一支，做完接着落那笔族终笔：删三个文本口
（`bin_support::args()`、`bin_support::prog_name()` 与 `minix_textfilter::empty_optstring_operands`
的 `&str` 形——现量本树里那形只剩 `src/lib.rs` 自己的七处门断言还在用），并把
`os/commands/games/text-games/tests/argv_seam_face.rs:fn a_raw_nonutf8_argument_behaves_like_its_lossy_equivalent`
的期望从「raw 与 lossy 同形」升到逐字节等于原字节；`text-games` 三支壳（`random`、`wtf`、`fortune`）
与 `textfilter` 余下十三支同批迁字节缝。在册不变者：
`ARGV-BYTE-PATH-IN-LIBS`（本笔再添 `-o` 落盘的档名一条腿）、`UNIFDEF-O-ERRNO-LEG`、
`UNIFDEF-D-DEBUG-OUTPUT`、`UNIFDEF-SYMLIST-OUTPUT`、本笔新立 `UNIFDEF-LINE-CONTENT-TEXTSEAM`、
`TEXTFILTER-WARN-BYTES-DUPLICATES-ERR-LINE`、`SPLIT-*` 两枚与 `TAIL-OFFSET-SATURATION-OUTCOME`
（饱和裁决按上一笔的结论**逐命令照各自那一句**）。

**合回条件（本笔当场量）**：`git log --oneline ae4aadcfb..rewrite` 交回空、
`git merge-base --is-ancestor ae4aadcfb rewrite` 退 0 ⇒ 纯快进、零冲突；主树 `git status --porcelain`
里本笔涉及的 `rewrite-notes/18-stage-commands/`、`rewrite-notes/coordination/PARALLEL-20261018-A-CMD.md`
与 `os/commands/usr-bin/textfilter/` 三处都没有在制改动。快进之后的主树复跑读数另记一笔合回补记。

**合回读数（第一百八十五笔补记，批次四百七十一）**：快进前复跑上面两条命令，
`git log --oneline ae4aadcfb..rewrite` 仍交回空、`git merge-base --is-ancestor ae4aadcfb rewrite`
退 0 ⇒ `git merge --ff-only claim/GETOPT-REPORT-BYTE-LETTER-qoder` 纯快进成功
（`8 files changed, 620 insertions(+), 91 deletions(-)`，主树 `rewrite` 顶到里程碑笔 `df7701d47`）。
逐笔验祖先四笔全 ✓（码笔 `3d0497ead`、文档笔 `1f6dc50b2`、台账补正笔 `01a1aaa84`、里程碑笔
`df7701d47`）。主树当场复跑 `cargo test --offline -j 1 --no-fail-fast -p minix-textfilter`
→ 退出码 0、**69 段 / 1283 / 0**，与领取树最终态逐数相同（库内 631、`unifdef_face` 15，
`/home/xzhao/github/minix-rs/tmp/unifdef-471-maintree.log`）。取证目录 `tmp/unifdef-471/`
（三侧现量、A/B 补栏前后两份、负控三枚、`symcheck.log`、`content-seam.log`、`attribution.log`、
门日志、改前与改后两枚件、`post2/` 快照）已在主树 `tmp/` 下，无需另抄。
本笔留在读数里的两处教训：其一，`-s` 那一形的初版判断是**读码读出来的因果**（`find_sym` 里那句
`from_utf8(...).ok()?` 会吃掉带裸字节的符号），一次都没量就写成了在册项——补跑之后本壳那侧
根本没有输出，册项删除，判据是「读码得到的『应该如此』不是观察，立册之前必须有一条跑过的命令兜着」；
其二，A/B 脚本末尾那几行在册形的打印语句里**没有 `out=` 栏**，标准输出的差异落不进日志，
而我还在旁边写了一句把它解释掉、连前提都不对——补栏重跑才看见参照件打 `A`/`B` 两行而本壳打空。
少了的那一栏会让读日志的人以为看见了全部，而含糊的解释又是自己写的，就更看不见。

### 第一百八十六笔（批次四百七十二 · A2 参数缝半切片 54：`vis` 的 `-e` 值、字母位与操数名交回原字节；本族第一枚「折叠改判决」的腿；支级切片至此走完，下一笔是族终笔）

**这一笔先把差分腿从无到有配了出来**。批次四百零四写的是「宿主没有 `vis(3)`，本支没有差分腿」——
那句话只对了一半：宿主确实没有现成的 `vis(3)`，但引擎本身就在本仓库里
（`minix3/lib/libc/gen/vis.c` 的 `strsenvisx` 一族）。本轮把它连同
`minix3/usr.bin/vis/{vis.c,foldit.c}` 与 Minix 的 `getopt.c` 一起链成宿主参照件
`tmp/vis-472/vis_ref`（垫片 `visshim.h` 供 `__RENAME`、`CTASSERT`、`_DIAGASSERT`、程序名与
`strlcpy`，空的 `namespace.h`，配方 `tmp/vis-472/build.sh`）。归属验过：经同名软链起 `-Z`
交回 `vis: unknown option -- Z` 加 usage、退 1；**两侧现量都钉 `LC_ALL=C`**——参照件的
`process()` 走 `getwc`/`wctomb`，只有在 C locale 下才是按字节的答案，而本壳的引擎自始按字节。

**这一支的形状与它独有的判决腿**：`vis.c:73` 的 optstring 是 `"bcde:F:fhlMmNnoSstw"`，
带值的只有 `e` 与 `F`。三处 `%s` 都在字节上：libc 句的字母位（`getopt.c:79`、`:103`）、
开不了的操数（`warn("%s", *argv)`〔`:157`〕，三段形 `vis: 档名: strerror`，打完继续跑下一个
并把退码定成 1〔`:158`〕）、以及 `-e` 的值——`extra = optarg`（`:87`）进 `makeextralist`
（`lib/libc/gen/vis.c:351-384`），C-locale 路径把 `-e` 串按 `(wchar_t)(u_char)src[i]`
**逐字节**上榜。于是 **`-e` 的值位是本族第一枚「折叠改判决」的腿**：extra 表是一张字节集合，
`-e $'\377'` 扫 `x\377y` 参照件交回 `x\377y`（ff 上榜、走三位八进制），折过的本壳交回
`x\M^?y`（名单变成 `ef bf bd` 三枚而 ff 不在）——折叠换掉的是**编码的答案本身**。
至此这族两分：`split`/`tail`/`unexpand`/`unifdef` 四支的判据是 ASCII 谓词、折叠只改报文，
`join`/`pr`/`sort`/`vis` 四支的判据是按字节比较的集合或串、折叠连答案一起改；
`-F` 的值位按构造无差（`atoi` 只认 ASCII 数字、那句报文不带 `%s`），写进了门注释。

**参照件现量翻正了两处旧账面**（改前的件 **16/26** 与参照件逐字节同形，改后的件 **26/26**）：
① `-d` 是**不带值**的字母——07 篇与本壳注释长期写「C 的选项串里是 `d:`（带值）」，对照
`vis.c:73` 的字面，冒号挂在 `e` 与 `F` 上；非 `DEBUG` 构建里 `case 'd'` 被编译掉，getopt 认
`d`（不报缺值行）于是落到 `default:`。参照件四形现量：`-d`、`-d x`、`-cd`、`-dZ` 都只有
usage 一行、退 1。本壳此前把 `d` 当带值字母，`-d` 排在末尾时多打了 libc 的缺值行（两行）；
面门期望**有意识翻正**。② 开不了的操数是 `warn("%s", *argv)` 的三段形，本壳此前只打两段、
strerror 那一段整个缺失；落码 `fn warn_can_open` 自己拼（本族第四份自拼的三段形助手，
`TEXTFILTER-WARN-BYTES-DUPLICATES-ERR-LINE` 的计数随本笔更新），档名位按原字节。

**落码层**：壳侧 `args_bytes()`/`prog_name_bytes()`，`parse_vis_args` 收 `&'a [&'a [u8]]`、
`Plan.operands` 改 `Vec<&'a [u8]>`、`-e` 的两个取值臂（附值与下一词）都交原字节；开档口仍走
`&str`（在册 `ARGV-BYTE-PATH-IN-LIBS`），折只发生在递给它的那一刻。**库侧零改动**：
`VisOptions.extra` 自始就是 `Vec<u8>`、`extra_set` 按字节建表，且 vis 的选项环**没有提库**
（`parse_vis_args` 住在 `src/bin/vis.rs`）——这是与 unifdef/unexpand/split 三支的结构性差别，
负控 K1 因此注定「库内全绿」。

**门账**：面侧 `vis_face` **20 → 21**（新立
`the_extra_list_the_letter_and_the_operand_name_carry_the_argv_bytes`，六段：`-e` 分词形、
`-e` 附值形、反面形、字母位、操数名、`-F` 按构造无差），跨壳表二十三行 → **二十四行**
（`("vis", VIS, 1, b"vis: unknown option -- \xff\n")`；本支此前不在表上，因为渲染半那笔
落地时参数缝还没落），整包 69 段 **1283 → 1284 passed / 0 failed**。既有的 `-d` 期望与操数
报文期望按参照件翻正。

**负控六枚（`tmp/vis-472/negctl.py`，逐枚 `negctl-K1.log`..`negctl-K6.log`）按红掉的测试
枚数各咬 2/3/1/1/1/2**：K1（壳侧 argv 退回文本缝）咬跨壳表 + 面门而**库内 631 全绿**——
「库门对壳侧 argv 缝没有牙」第十一次量化，且这一支是结构性的：环没提库，unifdef 那种
「K2 穿透库内」的形状在这里造不出来；K2（字母位按字符取整簇）咬三枚；K3/K4（`-e` 的两个
取值臂各折一次）各咬一枚——**两枚分开注入是有意的**，只折一臂时另一臂的形还绿着，红名
清单会漏；K5（`warn` 档名位折一次）咬一枚；K6（撤掉三段形的 strerror 段）咬两枚。
`ToString` import 补上之后六枚重跑一遍（`negctl-driver2.log`），读数一字不差，注入还原后
三件 md5 与跑前清单相符（`post/md5-final.txt`）。

**三起工具自错**：① **no_std 的 `ToString` 第三次**——`warn_can_open` 里
`Strerror(code).to_string()` 需要 `alloc::string::ToString` 在作用域里，宿主测试看不见它，
三架构第一遍各报 2 个 `E0599`（`arch-*.log`）；批次四百六十八的判据第三次兑现，而且本轮把
三架构排在了负控之后才跑、白等一轮——**判据加严：动壳侧字符串拼装后三架构当场跑，排在
负控之前**。② 量具第一版有一枚形状写坏：想造 `-é` 却拼出 `-` 加一枚孤字节 `c3` 的两个
token，改前读数 15/25 建立在错形上；改成字面 `b"-\xc3\xa9"` 重跑才得 16/26——**形状的
注释与它构造的字节要一次核对**。③ bash 探针里 `cmd | head` 之后的 `$?` 是 `head` 的退出码，
`-d` 那次探针显示 `rc=0` 是假的；读退码要么直接跑、要么 `${PIPESTATUS[0]}`。

**门读数（全部 L1，`2026-10-30`，基线快照 `1366871e6`）**：宿主整包 **69 段 / 1284 / 0**
（`pkg-1.log`、`pkg-2.log` 同读数、退出码 0）；容器权威门按 §7.3 三挂载配方 → 退 0、
**69 段 / 1284 / 0**（`container-gate2.log`，第一次同读数）；三架构各退 0、错误 0
（`arch2-*.log`）；`rustfmt --edition 2024 --check` 触及三件各 0 处（`fmt2-*.log`、
`fmt3-*.log`；`bin_support.rs` 的四块存量不在本笔触碰范围）；`cargo clippy --offline -j 1
--all-targets -p minix-textfilter` 退 0、警告 **149** 行与主树同数，去路径后的唯一句两树
都是 **37** 条且 `diff` 为空（`clippy2.log` 对 `clippy-main.log`），零新警。

**五道收尾门逐条读数**：① `bash tools/todo-reconcile-scan.sh` → 十三项里 2 项 DRIFT
（探针两项），与主树同读数、非本线引入；② `python3 tools/notes-link-check.py rewrite-notes/`
→ 764 个 md、断链 131（连续六笔同数），本轮三件文档按窄口径做改前/改后集合对照，两侧都是
**6** 条、目标串集相同 ⇒ 新增 0；③ `bash tools/todo-staleness-check.sh …TODO-LEDGER-OPEN.md`
→ **PASS（0 项）**；④ `bash tools/doc-style-lint.sh --diff` → 3 个文件 / 102 个新增行、
**error 级 0**；⑤ `bash tools/check-staged-inflight.sh` → 退 0，避让清单 1 条未触碰。

**进度与在册（全部现量）**：**参数缝半已落二十七支，`textfilter` 的字母位诊断面至此清零**
（逐支数 `grep -c Getopt`：余下十二支全零）；`textfilter` 余 **12** 支、命令面 **85** 支；
`line_bytes` **87 处 / 34 件**；`bin_support::prog_name()` 的文本形全命令面余 **16** 个文件
（本包清零）；三段形自拼助手 **4** 份。在册不变者：`ARGV-BYTE-PATH-IN-LIBS`（本笔再添 vis
的开档口一条腿）、`VIS-USAGE-PROGNAME-HARDCODED`、`TEXTFILTER-WARN-BYTES-DUPLICATES-ERR-LINE`、
`SPLIT-*` 两枚、`TAIL-OFFSET-SATURATION-OUTCOME`、`UNIFDEF-*` 四枚。
**下一笔是族终笔**（支级切片已尽）：删三个文本口（`bin_support::args()`、
`bin_support::prog_name()`、`minix_textfilter::empty_optstring_operands` 的 `&str` 形——
现量那形只剩 `src/lib.rs` 自己七处门断言在用）、迁 `text-games` 三支壳（`random`、`wtf`、
`fortune`）、把 `os/commands/games/text-games/tests/argv_seam_face.rs:fn
a_raw_nonutf8_argument_behaves_like_its_lossy_equivalent` 的期望升到逐字节等于原字节，
该笔闭本行。

**合回条件（本笔当场量）**：`git log --oneline 1366871e6..rewrite` 交回空、
`git merge-base --is-ancestor 1366871e6 rewrite` 退 0 ⇒ 纯快进、零冲突；主树
`git status --porcelain` 里本笔涉及的 `os/commands/usr-bin/textfilter/`、
`rewrite-notes/18-stage-commands/` 与 `rewrite-notes/coordination/` 三处都没有在制改动。
快进之后的主树复跑读数另记一笔合回补记。

**合回读数（第一百八十六笔补记，批次四百七十二）**：快进前复跑上面两条命令，
`git log --oneline 1366871e6..rewrite` 仍交回空、`git merge-base --is-ancestor 1366871e6 rewrite`
退 0 ⇒ `git merge --ff-only claim/GETOPT-REPORT-BYTE-LETTER-qoder` 纯快进成功
（`7 files changed, 399 insertions(+), 56 deletions(-)`，主树 `rewrite` 顶到里程碑笔 `990e535ad`）。
逐笔验祖先三笔全 ✓（码笔 `f726ac054`、文档笔 `a6f8364f1`、里程碑笔 `990e535ad`）。主树当场复跑
`cargo test --offline -j 1 --no-fail-fast -p minix-textfilter` → 退出码 0、**69 段 / 1284 / 0**，
与领取树最终态逐数相同（`vis_face` 21，`/home/xzhao/github/minix-rs/tmp/vis-472-maintree.log`）。
取证目录 `tmp/vis-472/`（参照件与其配方、`measure.py` 与改前/改后两份读数、负控六枚两遍、
门日志、`post/md5-final.txt`）已在主树 `tmp/` 下，无需另抄。本笔留在读数里的教训：`-e` 值位
那枚判决腿是本族八支里第一次让「折叠改判决」落在一个**编码答案**而不是报文上——量它靠的是
把差分腿从无到有配出来，而配出来之后顺手翻正的两处旧账面（`-d` 带值、操数报文两段）都是
「只有读码、没有参照件」时代留下的口径；差分腿缺位时写下的期望要标注判据来源，配齐之后
第一批事就是把旧期望全部重测一遍。

### 第一百八十七笔（批次四百七十三 · A2 族终笔：十五支壳迁字节缝、三个文本口删除、`argv_seam_face` 的门升到逐字节；本行闭合）

**删口的前提是消费者清零**，所以这一笔先迁后删：十二支没有字母面的 textfilter 壳
（`wc`、`hexdump`、`fmt`、`unvis`、`look`、`lam`、`tsort`、`asa`、`expand`、`diff`、
`colcrt`、`patch`）与 text-games 三支（`random`、`wtf`、`fortune`）全部换到
`args_bytes()`，然后删掉三个文本口——`bin_support::args()` 两条腿、
`bin_support::prog_name()`、`minix_textfilter::empty_optstring_operands` 的 `&str` 形
（那张三形判据的钉子全部迁到 `_bytes` 形上）。四个库入口同时改收字节：
`hexdump::parse_skip`、`expand::{parse_expand_args,parse_stops}`、`tsort::parse_args`、
`lam::parse_width`。**报文形状全部照旧**：这些壳的开档失手句与 C 三段形的差是既有维度，
本笔只把名字位换成原字节、不改句子——「一次只修一条」在族末笔的收口处同样成立。

**门的两处升级**：① `argv_seam_face` 那枚门从「裸字节 == lossy 等价」（批次四百二十二
立的契约）升到「裸字节 == C 的答案」（参照件 `tmp/random-421/random` 的读数）：random 的
环字母位、wtf 的查询词位、fortune 的档名位三形逐字节。两处期望是参照件读出来的——
wtf 与 fortune 的报文里**连短横一起**原样出现（token 整个是查询词/档名），写期望时凭空
丢掉短横就当场红。② 新立跨壳门
`the_plain_shells_operand_echo_carries_the_argv_bytes`（十行）：没有字母面的十支壳各喂
一枚带裸 `ff` 的操数，钉住报文里的名字位是原字节；colcrt 与 fmt 的失手句不带档名位、
没有可观察的字节腿，不入列（理由写进门注释）。

**门读数（全部 L1，`2026-10-30`，基线快照 `d6140babc`）**：宿主 textfilter **69 段 /
1285 / 0**、text-games **9 段 / 79 / 0**；容器权威门两包同读数退 0（第一遍失败是
`tmp/ci-target` 被删后 docker 以 root 重建、挂载写不进去——目录属主问题不是代码问题，
删掉重建成用户属主一次通过）；三架构两包各退 0；`rustfmt --edition 2024 --check` 触及件
各 0 处；clippy 两包退 0、唯一句集合与主树相同（textfilter 149 行、text-games 14 行）。
负控三枚各咬 1（`tmp/family-473/negctl-summary.txt`）：K1（wc 的壳侧 argv 内联一条有损
折叠——`args()` 已删，注入只能复刻那条腿）咬十行门的 wc 行，K2/K3 各咬 seam 门一形；
random 的 OutOfRange 臂只有 ASCII 数字能到，折叠无可观察差，**不设注入**并写明原因。

**四起工具自错**：① **no_std 的 `ToString` 第四次**（`unvis`、`random`、`fortune` 三件
当场补 import），且本笔把三架构又排在了负控之后——上一笔刚加严的判据自己没执行，
连续两笔犯同一类顺序错；② **rustfmt 顺着 `src/lib.rs` 的模块树带出九件纯格式漂移**
（join/nl/look/diff/fmt/colcrt/patch/rev/seq 的库件被就地格式化），按批四十零四那条规矩
`git checkout --` 还原、不带走——**对 lib 件跑 rustfmt 等于对全族跑**，这条判据第二次
兑现；③ clippy 抓到五处换缝后死掉的 `format`/`ToString` import 与一枚可省略的生命周期，
就地清掉才与主树同数——**删口之后 import 的死活要靠 clippy 复核**；④ 负控驱动两起：
K1 的注入串引用了已删除的 `support::args()`（负控也是代码，删口后要跟着改），K3 的锚点
在 fortune 里命中两处、收窄到带句子的上下文才咬对形。

**进度终值与本行闭合**：渲染半十支（`GetoptReport::line_bytes`）+ 参数缝半二十七支
= **三十七支壳全部按字节**；`line_bytes` **88 处 / 35 件**；三段形自拼助手 **5** 份；
`args()` 文本口在全命令面只剩其它族自己的 bin_support 各一份（**13** 份，现量
`grep -rl "pub fn args()" os/commands --include=bin_support.rs | wc -l`，不在 A2 射程）。
自批次三百九十一起立账的字母位失真与参数缝失真两条残差**全部归零**，协调板 A2 行翻 ✅。
在册不变者：`ARGV-BYTE-PATH-IN-LIBS`（本笔再添十五处开档口一条腿）、
`TEXTFILTER-WARN-BYTES-DUPLICATES-ERR-LINE`、`SPLIT-*` 两枚、
`TAIL-OFFSET-SATURATION-OUTCOME`、`UNIFDEF-*` 四枚、`VIS-USAGE-PROGNAME-HARDCODED`；
wtf 的行池内容半按源码应如此、未量（无参照件，不成册项）。

**五道收尾门逐条读数**：① `bash tools/todo-reconcile-scan.sh` → 十三项里 2 项 DRIFT
（探针两项），与主树同读数、非本线引入；② `python3 tools/notes-link-check.py rewrite-notes/`
→ 764 个 md、断链 131（连续七笔同数），本轮两件文档窄口径改前/改后对照，两侧都是 **6** 条、
目标串集相同 ⇒ 新增 0；③ `bash tools/todo-staleness-check.sh …TODO-LEDGER-OPEN.md` →
**PASS（0 项）**；④ `bash tools/doc-style-lint.sh --diff` → 2 个文件 / 78 个新增行、
**error 级 0**；⑤ `bash tools/check-staged-inflight.sh` → 退 0，避让清单 1 条未触碰。

**合回条件（本笔当场量）**：`git log --oneline d6140babc..rewrite` 交回空、
`git merge-base --is-ancestor d6140babc rewrite` 退 0 ⇒ 纯快进、零冲突；主树
`git status --porcelain` 里本笔涉及的三处（`os/commands/usr-bin/`、`os/commands/games/`、
`rewrite-notes/`）都没有在制改动。快进之后的主树复跑读数另记一笔合回补记。

**合回读数（第一百八十七笔补记，批次四百七十三）**：快进前复跑上面两条命令，
`git log --oneline d6140babc..rewrite` 仍交回空、`git merge-base --is-ancestor d6140babc
rewrite` 退 0 ⇒ `git merge --ff-only claim/GETOPT-REPORT-BYTE-LETTER-qoder` 纯快进成功
（`29 files changed, 1089 insertions(+), 574 deletions(-)`，主树 `rewrite` 顶到里程碑笔
`6859b6075`）。逐笔验祖先三笔全 ✓（码笔 `077c9dfc1`、文档笔 `2eacd268d`、里程碑笔
`6859b6075`）。主树当场复跑两包：textfilter 退出 0、**69 段 / 1285 / 0**，text-games
退出 0、**9 段 / 79 / 0**（`tmp/family-473-maintree-{tf,tg}.log`），与领取树最终态逐数
相同。取证目录 `tmp/family-473/`（negctl 三枚三遍、门日志、`post/md5-final.txt`）与
`tmp/family-473-maintree-{tf,tg}.log` 都已在主树 `tmp/` 下。**A2 本行自此闭合**：
`GETOPT-REPORT-BYTE-LETTER` 领用链从批次三百九十一起，跨渲染半与参数缝半两个子射程、
三十七支壳、九个手工配置的参照件（rev/paste/jot/unifdef/colrm/head/split/tail/unexpand/
unexpand-vis 族等复用与新建合计）、六起「参照件翻正旧账面」的现量，收官于本笔的三个
文本口删除——`bin_support::args` 的有损折叠自此在这两族不再存在。

### 第一百八十八笔（A 板 A25 · `SD-19` 的服务侧面：十一 crate 逐家核门与处置表；sched 是唯一缺门的）

**这一笔的载荷**：`os/servers/` 下十一家服务 crate（devman、ds、input、ipc-server、is、mib、
pm、rs、sched、vfs、vm）逐家核 `#![warn(clippy::let_underscore_must_use)]` 门并写处置表。
现量：**十家此前已带**这枚门（与本行上方 SD-19 的 libs 面、B8 的十二家库 crate 同形），
**sched 是唯一没接的**——本片给它接上，`cargo clippy -p minix-sched --all-targets` 当场报出
三处 `#[must_use]` 站，逐站按 C 行处置：

- `server.rs` 的 `let _ = self.do_noquantum(&message, kernel)`——C 70-77 的封印支：失败只印一行
  （C 73-74）、照样不回话；诊断句没有 Rust 归宿，不回话是可观察的那一半，吞返回值与 C 同义；
- `server.rs` 的 `let _ = ipc.sendnb(sender, &reply)`——C 101-106 把失败的 reply 记一行就丢掉、
  循环继续；
- `server.rs` 的 `let _ = self.fanout_local(index, kernel)`——C 362 原文就标 `unchecked`。

每处 `//[SD19-gate] <理由>` 加语句级 `#[allow]`，与 `minix-sys` 的旗舰样本同形。

**处置表**（写进 `TODO-LEDGER-OPEN.md` 的 SD-19 行）：十一行、合计 **440 个 `let _ =` 站点 /
344 处带 `[SD19-gate]` 标记 / 真触发 0**。未带标记的 96 个站点不是漏网——它们的 RHS 不是
`#[must_use]`，门在而 clippy 静默，这就是逐站可证的「不适用」。**vm 的整目标读数受限**：
`cargo clippy -p minix-vm --all-targets` 撞存量 deny 错（`servers/vm/src/phys_mem/bitmap_alloc.rs:548`
的 `% 1`，本行上方 workspace 扫尾段已与 `minix-textfilter` 的 `approx_constant` 并列登记），
lib 目标单独跑退 0、真触发 0；本片不碰那两枚存量错。

**读数**：`minix-sched` 宿主 **88/0**、容器权威门 **3 段 / 88 / 0** 退 0、三架构 `--bins` 各退 0；
十一家 `cargo clippy -p minix-<crate> --all-targets` 除 vm 外各退 0、真触发全 0。
**射程边界照板上路由注**：libs 侧的同类门归 B 板（B8 已落），本片只动服务侧。
**本笔的一处工具自错**：第一次三架构用了 `--all-targets`，测试目标在 `-none` 上没有 `std`
（12 个 `E0463`），按前几笔的配方改成 `--bins` 才是指标；另一处：workspace 级 clippy 不能裸跑
（kernel 的 form 契约在无特性时编译期报错，AGENTS.md 已写明），改用「逐 crate + 两家存量错
exclude」的分域读数。

**合回读数（第一百八十八笔补记，A25）**：快进前复跑 `git log --oneline 11689b345..rewrite`
空、`git merge-base --is-ancestor 11689b345 rewrite` 退 0 ⇒ 纯快进成功（`5 files changed,
77 insertions(+), 1 deletion(-)`，主树顶到 `b1ede829f`）。逐笔验祖先两笔全 ✓（码笔
`e5169e66c`、台账笔 `b1ede829f`）。主树复跑 `cargo test -p minix-sched` → 3 段 / 88 / 0
（`tmp/sd19-473/maintree-sched2.log`；第一次用共享 `os/target` 撞上并发写冲突的
`Permission denied`——同目录另一条线正在构建，按并发纪律改用 `CARGO_TARGET_DIR` 独立目标
目录复跑，读数一致）。处置表与读数随台账笔入 SD-19 行与 A25 行。

### 第一百八十九笔（A 板 A16 · 04 进程管理：三条「open」逐项现查全部消解，本行 by-audit 闭合）

**载荷**：`04-stage-pm/todo.md` 的摘要行写着「未闭合 open 3：核心转储路径契约、调度服务缺席
导致的假端点、调用号双址」。逐项现查的结果是三条都已被后来的批次消解，本行按 by-audit 闭合：

- **核心转储（D-16）**：不再是「路径名指针为 0」——`os/servers/pm/src/exit.rs` 的
  `VfsCall::DumpCore` 改走**按值名字**协议（`identity.name` 直接进载荷尾字节 40..56，
  `name_len` 取首个 NUL 之前），理由与 `term_sig` 的来源修正都写在注释里（EXITING 之前
  回读恒 0，而 VFS 的 DUMPCORE 臂拒 `term_sig=0`，所以用 `sig_status` 实参）。该处仍挂在
  E7 的 wire 系统化上，但**没有 stage 内待办**。
- **假端点（D-17）**：不成立。`init.rs::sched_start` 是真实
  `sendrec(Endpoint::SCHED, …)` + ELOCKED(208) 重试（与 C `libsys/sched_start.c` 的
  `_taskcall` 同形），`sched.rs` 的 `SchedCtl`/`sched_start_user` 与 `exit.rs` 的
  `SCHEDULING_STOP`（C forkexit.c:425-441 的打印后继续）都已接线。**`init.rs` 里两处
  「DEFERRED: the minix_sched client (16-scheduling.md)」是传输接上之后没跟着改的旧注释**
  ——注释与代码相反正是 SD-19 那一族，本笔按实现改写。
- **调用号双址（PROC_EVENT_REPLY）**：已随 Fix #58 改 re-export `minix-types` 常量；余下的
  `PmRequest`/`PmResponse` 死代码本体在 E7 共享面，属跨板登记。

**读数**：`cargo test -p minix-pm --lib` → **427 / 0**（宿主与容器权威门同读数退 0），
三架构 `--bins` 各退 0。**本笔只动两处注释与台账**（一行摘要、一行板行、本里程碑）——
「不是修复」的自我设限在这里成立：三条的修复早已在别的批次落地，本笔的职责是把账面追上代码，
并把「注释与代码相反」的那两处就地清掉。

### 第一百九十笔（A 板 A14 第一腿 · 02 虚存服务器：装载链对账已落，热更腿立 `V20-P1-1` 挂设计决定）

**第一腿（装载链）对账即已实现**：`os/servers/vm/src/vm_server.rs::exec_bootproc`（`:785`）走
`minix_elf::entry_point` + `segment_iter` 的 PT_LOAD 遍历，逐段页对齐落页（注释里留着 RS 入口段
`vaddr=0x2246c0` 非页对齐那个旧缺陷的修法），按 `memsz` 留堆/BSS 尾，随后建初始栈帧——
「启动进程的可执行文件装载链」不再是待办。

**第二腿（热更/重启状态机）立成可执行条目**：本阶段 todo 新增 §20 `V20-P1-1`。C 语义是
`main.c:79-108` 的 `is_first_time()` 靠 BSS 全局在 `RTS_BOOTINHIBIT` 清后存活（进程重启、数据
活着），而 Rust 的状态在 `VmServer` 实例里、随进程消失。补齐它要一个 `[ARCH]` 级决定——跨重启
状态放在哪（静态区/共享页/从根服务重放）加三架构的登记与清零时序；在那之前 `main.rs:73` 的
**响亮拒绝**是诚实行为（静默以空状态继续会让既有进程的页表失去宿主）。顺带修掉 panic 文案里
悬空的 `G-V12-10` 锚点（本文件并无该条目，改指 `V20-P1-1`）——悬空锚点与 SD-19 的注释族同源。

**读数**：`cargo test -p minix-vm --lib` → **543 / 0**（宿主与容器权威门同读数退 0），三架构
`--bins` 各退 0。A14 行记 🔄（一条腿落、一条腿挂设计决定），不冒认 ✅。

### 第一百九十一笔（A 板 A15 · 03 根服务：二十四符号接线面与自升级两条现查都在边界侧，本行 by-audit 闭合）

**二十四符号接线面**：`os/servers/rs/src/trap_api.rs` 的 `KernelGateway`/`IpcLeg` 两条腿逐条落地
（`get_machine`、`get_hz`、`get_ticks`、`privctl`、`getpriv`、`setalarm`、`sys_kill`、`sys_update`、
`diagctl_stacktrace`、`diag_write`、`sched_init_proc`、`sched_stop`、`getnuid`、`getnpid`、
`getprocnr`、`cpf_reload`、`srv_fork`、`srv_execve`、`srv_kill`、`waitpid`、`setuid`、`vm_memctl`、
`vm_set_priv`、`receive`、`reply`）；`grep -rn "unimplemented!()\|todo!()" os/servers/rs/src`
生产区零命中。**自升级**：决策面与 `update_service` 闭包缝在 `live_update.rs:1239+`、
`self_lifecycle.rs` 的纯切片就绪；剩余是 E-1 点名的「19 的进程/IPC 缝上驱动 update+init+YIELD
链」——`exec.rs`/`sched.rs` 的 `awaiting-wiring` 注记指的都是**消费侧（19 / E-11）**，本阶段
没有缺件。板上路由注「自升级先读 19 阶段接线面，不代做 19 的前置」照旧成立。

**读数**：`cargo test -p minix-rs --lib` → **357 / 0**。本笔只动一行摘要、一行板行与本里程碑。

### 第一百九十二笔（A 板 A17 · `E-DMWIRE` 四缺逐条读原文：服务侧三缺已落，client 面归 E5(h)/B 板）

- **server transport**：`os/servers/devman/src/main.rs:37-42`——`MinixTransport::new(SysKernel)` +
  `server.run(&mut transport)` 的生产环；环结束即响亮 panic（传输坏掉没有恢复面）。
- **请求分类器**：`src/ipc/dispatch.rs:38-43`——`DEVMAN_ADD_DEV`/`DEL_DEV`/`BIND`/`UNBIND` 四消息
  五路 + `Ignored` 缺省；注释写明与 C 的 `switch` 同形（未匹配的 case 什么都不做、fail-closed），
  `null`/`REPLY`/垃圾走忽略臂。
- **装配半**：`src/server.rs:314` 的 `build_forward` 与 server 在自有状态上的装配。
- **client/RS 侧**：`minix-sys::devman_client` 的三条腿已实现（`C-17`，`devman_client.rs:105-162`，
  宿主测在），生产驱动的消费者与端到端生命周期（mount → 注册 → 事件 → bind → 删除）挂在
  **E5(h) 真机联调**；client 面按板上路由注归 B 板对账。

**读数**：`cargo test -p minix-devman --lib` → **90 / 0**。本笔只动一行 §6 表与一行板行。

### 第一百九十三笔（A 板 A18 · 07 数据存储：四族测试载体与 `E-DSWIRE` 客户端面现查都已落，本行 by-audit 闭合）

- **四族的测试载体**：`os/servers/ds/src` 里已有具名载体——`test_publish_check_roundtrip_through_transport`
  （subscribe → publish → check 三信一封的跨族全链）、`test_delete_wakes_through_transport`、
  `test_first_subscribe_takes_seat_zero` / `test_second_subscribe_without_overwrite_refuses`、
  `notify.rs::test_publish_wakes_matching_subscriber`、
  `test_label_delete_wakes_matching_subscriber_before_clear`；全链跑在 `server.rs` 的 `wired()`
  夹具上逐封 `run_once`。
- **`E-DSWIRE` 客户端面**：`minix-sys/src/ds.rs` 的 `publish_label`/`publish_u32`/`publish_mem`/
  `publish_str` 与 `retrieve_u32`/`retrieve_label_name`/`retrieve_label_endpt`（`:300`、`:349`）
  齐备——`ds_retrieve_label_endpt` 不再是缺口。

**读数**：`cargo test -p minix-ds` → **115 / 0**（3 段）。四族的**端到端联调**归 C 板真机窗
（E5(f) 首份终态在册），本板不代做。本笔只动一行摘要、一行板行与本里程碑。

### 第一百九十四笔（A 板 A20 首轮 · 15 文件系统：第一家 vbfs 的静默桩改为响亮拒绝，fsdriver 环照 pfs 模板待接）

**四家按「有真盘才接」的次序执行，本轮把第一家的现状量清并做了一处求实**：
`os/fs/vbfs/src` 的 lib 只有 `default_params()`（sffs 参数集）与空 `init()`；`main.rs` 原本是
`init(); loop {}`——对 RS 和操作者都像「服务在跑」的**静默挂起**，正是 SD-19「假装」那一族的
原型。现改为带计划锚的 panic：RS 今天不启 vbfs，panic 在引导路径上是惰性的，而把一个假活进程
换成一个明确「还没造好」的拒绝。

**下一轮的第一切片已量准**：vbfs 的 fsdriver 环照 `os/fs/pfs/src/{lib,main}.rs`（836 行，
`#![no_std]`，`cargo check --target x86_64-unknown-none -p minix-fs-pfs --bins` 退 0 已验证是
可行模板）整条移植；ext2/isofs 等 `E-FSBDEV` 真镜像（本仓 ext2 现为只读语义层），hgfs 与 vbfs
同量级。判据：`cargo test -p minix-fs-vbfs` → **2 / 0**；vbfs 的**三架构门当前不适用**
（其 binary 是宿主态桩），接环时随 pfs 模板一并转 `no_std`。

### 第一百九十五笔（A 板 A21 · 17 网络：`N1-P1-4` 的「主循环占位」已过期，循环半在；余协议栈本体与 E-NETSTART）

板上落码注要求「先复核 `N1-P1-2`/`N1-P1-4` 的落地行是否覆盖生产主循环」。答案：**覆盖了**。
`os/net/lwip/src/main.rs` 与 `os/net/uds/src/main.rs` 都以 `ProductionHandler` +
`minix_netdriver::socktable::SockTable` + `server::run(…)` 起真环：lwip 侧带 7 段映射 C 十三步的
`Startup` 链（`lwip.c:203-263`）、`-dev` 参数收集与 `wait_for_init` 的 RS 生命期门，handler 里
stack/mcast/routes/pending/wake_replies 各面齐备；`grep -rn "unimplemented!()|todo!()" os/net`
生产区零命中。

**剩下的两条**：① **协议栈本体**——`ProductionHandler.stack: Option<Box<dyn Stack>>` 在启动链
第 2 步建，smoltcp 适配的落地随 `25-smoltcp-shim.md` 的设计（本轮不代做）；② **SF/RS 启动链**
挂 **E-NETSTART**（跨板）。判据：`cargo test -p minix-net-lwip -p minix-net-uds` → **148 / 0**（6 段）。

### 第一百九十六笔（A 板 A22 成员 `E-ISWIRE`：devman 的 SefHooks 生产实现现查已落，本成员闭）

A22 是跨阶段传输族的**家庭行**（板上自注「逐项领，一次一个」），本笔领的是成员 `E-ISWIRE`
（devman SefHooks 是 `minix-sef` 的第三消费方）。现查：`os/servers/devman/src/hooks.rs:108` 的
`SefHooks` trait 已有生产实现 `DevmanSef`（`:134` 的 `init_server` = 重建 `Server::new(&config)`，
对位 C `vtreefs.c:52-62` 的 fresh 回调；restart 注册为 `SEF_CB_INIT_RESTART_STATEFUL`、正文空，
因为状态随 RS 镜像恢复存活；信号注册落在 `ipc::minix::MinixTransport::terminate` 的停止闩上），
`test_devman_sef_production_hooks` 钉住；`main.rs` 以等价的 inline 形（`Server::new` + `run`，
带 `main.c:77-89` 锚、初始化失败响亮 panic）起服务。

判据：`cargo test -p minix-devman --lib` → **90 / 0**（hooks 过滤下 5/0）。A22 其余成员照旧逐项领，
E1/E6/E7/E8/E9 的内核半边与真机腿归 C 板。

### 第一百九十七笔（A 板 A22 成员 `E-INWIRE`：输入服务器的传输与 serve 循环现查已落，本成员闭）

`os/servers/input/src/serve.rs` 带 `Transport` 缝（`:40`）与真循环 `serve()`（`:388` 的 `loop`，
按到达逐个分类/分发/执行）；`main.rs` 的注记自陈「E-INWIRE: the transport is landed（minix-sys
direct verbs）；the loop classifies, dispatches, and performs through serve.rs」，SEF 切换点
（`receive → sef_receive_status`）写在 `KernelTransport::receive`；生产区 `unimplemented!`/`todo!`
零命中。判据：`cargo test -p minix-input` → **99 / 0**（3 段）。

A22 家庭行至此已有两个成员闭（`E-ISWIRE`、`E-INWIRE`）；其余成员（E1/E6/E7/E8/E9 的内核半边与
真机腿、E-ISPROD/E-ISBOOT/E-RMIBWIRE/E-MIBPROD/E-NETSTART/E-INITSYS/E-IPCWIRE/E-VMTLB/
E-PREEMPTFLAG）照旧逐项领。

### 第一百九十八笔（A 板 A22 成员 `E-IPCWIRE`：八缺逐条已有落点，ipc-server 四步装配是真入口，本成员闭）

E-IPCWIRE 登记的是 ipc-server 生产面的八缺（trap 桥/SEF 层/`sys_datacopy`/`proceventmask`/
`VM_SHM_UNMAP`/clock/`getepinfo` 窄 helper 七件 minix-sys 面 + minix-types 的 `semid_ds`/`shmid_ds`
布局面）。现查：七件 minix-sys 面全部有落点（`syscall.rs`/`pm.rs`/`vm.rs`/`rs.rs`），两份布局在
`minix-types/ipc/sysvipc.rs`；`os/servers/ipc-server/src/main.rs` 的四步装配
（`SysBoundary` + `SysEventLoopTransport` + `IpcServer::new` + `init`/`run`）是真入口，与 C
`main.c:216-284` 同序。

判据：`cargo test -p minix-ipc-server` → **114 / 0**（4 段）。

### 第一百九十九笔（A 板 A22 成员 `E-MIBPROD`：布局对账落成「单一权威 + 消费」，本成员闭）

E-MIBPROD 登记的是「MIB 快照 vs kernel/PM/VFS producer 布局对账」。现查：对账的形状已经是
**单一权威 + 消费**——表布局归生产者（kernel/PM/VFS），`os/servers/mib/src/proc/tables.rs:298-300`
的 `KERN_ROW`/`PM_ROW` 直接取 `core::mem::size_of::<minix_types::ProcInfoStruct>()` 与
`<MProcSnap>()`；kernel 四臂（GET_PRIVTAB/GET_IRQHOOKS/GET_IMAGE/GET_KINFO）的布局见证
（`priv_info.rs` 56B、`irq_hook.rs` 48B）在 minix-types 且带锁测（edge_todo 里 `987773bb3` 的进度）。
MIB 侧没有自己的布局副本可漂。

判据：`cargo test -p minix-mib` → **178 / 0**（3 段）。

### 第二百笔（A 板 A22 成员 `E-RMIBWIRE`：minix-sys 的 rmib 协议半已落，本成员闭）

登记时是「minix-sys rmib 客户端协议半整缺」。现查：`os/libs/minix-sys/src/rmib.rs` 已带稀疏表
（`sparse_find`/`validate_sparse`/槽位 `claim`/`release`/`live_slots`）与 `rmib_copyout`、
`rmib_copyout_node`（父规则与 inrange/oldlen 都在，纯函数测试 13 枚）。

判据：`cargo test -p minix-sys --lib` → **341 / 0**。

### 第二百零一笔（A 板 A22 成员 `E-ISPROD`：IS 消费侧的六组快照已全部改吃 minix-types 单一权威，本成员闭）

E-ISPROD 的裁决（方案 A：快照权威上收 `minix-types`）在 2026-09-16 已落，kernel 四臂随后闭环；
本笔现查的是**消费侧**：`os/servers/is/src/dump_kernel.rs:23` 与 `dump_rs.rs:14` 都以 `pub use
minix_types::{…}` 消费权威类型（PM/VFS/DS/VM 的 dump 模块同形），`KProcSnap`/`KPrivSnap` 只在
注释里出现——IS 侧再没有第二份 `#[repr(C)]` 可漂。

判据：`cargo test -p minix-is` → **129 / 0**（3 段）。本条 register 至此 `closed`。

### 第二百零二笔（A 板 A22 成员 `E-INITSYS`：init 真实运行所需的三件 minix-sys 面全在，本成员闭）

- **信号族**：`minix-sys/src/lib.rs:572` 的 `sigprocmask`、`:1082` 的 `sigaction`（另带
  `pm.rs:336` 的 `sigaction_via`）。
- **进程控制族与 open 存在路径**：`lib.rs:214/223/267/274` 的 `fork`/`exec`/`waitpid`/`kill`，
  配套 `pm.rs:170/212/286` 的 `*_via`。
- **`WaitStatus` 解码与 `WUNTRACED`**：`minix-sys/src/wait.rs:13-14` 起（含 `signaled_by`/
  `signaled` 判读），init 的 `driver.rs` 按变体消费。

判据：`cargo test -p minix-init` → **149 / 0**。

### 第二百零三笔（A 板 A22 成员 `E-FSRUNTIME`：hgfs 的空转桩改成响亮拒绝，两个空转桩至此全清，本成员记「桩半清、框架待」）

E-FSRUNTIME 的 8 个 fs server bin 里，2026-09-21 的复核已把 6 个接出 `loop {}` 占位
（mfs/pfs/procfs/ptyfs/ext2/isofs），余 hgfs 与 vbfs 两个空转桩。vbfs 随 A20 首轮改成
带计划锚的 panic；本笔把 **hgfs 同形处理**——`os/fs/hgfs/src/main.rs` 的
`init(); loop {}` 改为 `panic!("hgfs: the fsdriver loop is not implemented …")`，条目指
E-FSRUNTIME 与 pfs 模板。静默挂起看起来像「服务在跑」，响亮拒绝让「挂载为什么卡住」变成
「这个 server 还没造好」（SD-19 的「假装」族）。

**剩下的**是 SEF 三段逐 bin 核与 RS 启动握手——与 E-NETSTART 同根，等 03-rs 侧的服务运行时
startup 面，本板不代做。判据：`cargo test -p minix-fs-hgfs` → **2 / 0**。

### 第二百零四笔（A20 账本成员 `E-FSCMDS`：fsck/mkfs 的轨道认领已兑现，两条真工具在命令轨道上，本成员闭）

E-FSCMDS 登记的是「`os/commands/sbin/{fsck,mkfs}` 各 14/15 行纯参数解析占位，等盘上结构层稳定
后由命令轨道认领」。现查：占位 crate 早已不在，两条真工具已在命令轨道——`os/commands/sbin/mountinfo/
src/bin/fsck_mfs.rs`（168 行；判定半在 `minix_fs_mfs::fsck`，超级块校验、遍历、位图比对与计数对账
都在库里，壳只做旗标解析/读镜像/打印/退码）与 `os/commands/sbin/diskfmt/src/bin/mkfs_mfs.rs`
（182 行）。

判据：`cargo test -p minix-mountinfo -p minix-diskfmt` → **26 / 0**（7 段）。本条 `closed`。

### 第二百零五笔（A20 账本成员 `E-FSVMCACHE`：FS 侧二级缓存已落成零拷贝页移交，本成员 FS 侧闭）

E-FSVMCACHE 的 FS 侧已经落地：`os/fs/mfs/src/second_level.rs` 的 `MfsSecondLevel` 实现
`minix_fs::cache::SecondLevelCache`，消费 `minix_sys::vm` 的 `map_cacheblock_via`/
`forget_cacheblock_via`/`clear_cache_via`（零拷贝页移交面），块大小按页向上取整、`VMSF_ONCE`
走 setflags 车道（`second_level.rs:113`）；boot 配置按 `use_vmcache` 选面（`startup.rs:28-65`）。
`minix-types/types/vm_cache.rs` 带 `VMMC_EVICTED`/`VMSF_ONCE` 的 wire 常量与注释。

判据：`cargo test -p minix-fs-mfs -p minix-fs` → **317 / 0**（5 段，`second_level` 4 枚）。
`REQ_PEEK` 的 HAS_PEEK 能力与 VM 侧逐出回报的联调归 02 轨道的真机面，本条 register 的 FS 侧
无待办。

### 第二百零六笔（A 板 A21 栈体半：`SmoltcpStack` 与两个帧设备桥已在，A21 两半皆闭）

A21 剩余的两条里，「协议栈本体」也已经是实现而非待办：`os/net/lwip/src/stack.rs` 的
`SmoltcpStack<D: Device>`（1338 行）带 `impl Stack`（`:210`）、`TrunkDevice`/`LoopDevice` 两个
帧设备桥与 `add_address_v4`；服务侧只认识 `lwip_port::Stack` trait，具体栈不外泄（`[ARCH:
protocol stack behind the Stack trait]`）。

判据：`cargo test -p minix-net-lwip --lib` → **127 / 0**（`stack` 过滤 11/0）。A21 两半（主循环 +
协议栈本体）至此都闭，A21 行转 ✅；余者只有 **E-NETSTART** 的 RS 加载握手，跨板单列。

### 第二百零七笔（A 板 A14 第二腿：热更/重启状态机的设计裁决草案——推荐「静态承载」，本草案不改码）

`V20-P1-1` 缺的不是一段代码，而是「跨重启状态放在哪」的 `[ARCH]` 决定。本笔把三个选项写进
`02-stage-vm/todo.md` §20：

- **(A) 静态承载（推荐）**：把 `VmServer` 的状态搬进一个地址稳定的 `static`（`static mut` 加
  单线程守卫，或 `UnsafeCell` 包一层），`main` 用一个「已初始化」标志区分冷暖——冷启动走 `init()`、
  暖启动直接进 `run()`。对位 C 的 BSS 语义最直，代价是构造/借用面改成 `&'static mut` 或内部
  可变性，需要一次全量借用检查复核。
- **(B) 从对端重放（否决）**：页表/缓存映射这类内核侧对象无法从 VFS/PM/RS 重建，重建状态与真实
  硬件状态会分叉。
- **(C) 维持响亮拒绝（现状，非目标态）**：安全但没有热更语义。

验收两步走：先把「初始化恰好一次 + 暖启动不重入 `init`」的判据在宿主立成测试（把 `main` 两条路
抽成库函数），再在 QEMU 的 e5-ds/vm 重启用例上验真重启；两处齐了才把 `V20-P1-1` 转 ✅。
**本草案不改码**：静待裁决后才动承载面，且实施时按 `[ARCH]` 三处一致标注（本篇、设计快照、
`main.rs`/`vm_server.rs` 注解）。

### 第二百零八笔（C 板·内核优先 · SD-22 H5：riscv vmdm 的 `PteRead/Write/Zero` 从「任意物理地址」收到「调用者==VM + 物理 RAM」，H5 闭）

**这笔修的是内核侧一个无界物理读写面**：`PteRead`/`PteWrite`/`PteZero` 是本重写自造的 KDM 旁路
（C 没有这三号调用），此前把 `pa` 原样加进 `KDM` 基址就直接读写——VM 传什么就碰什么，设备 MMIO
也不例外，正是登记里说的 `direct_map(wrong_pa)` 洞的具象形（SD-22 H5 / SD-36）。

**修法**（`os/kernel/src/syscall.rs`）：三个 riscv 手臂各加双重闸——
1. **调用者必须 == `VM_PROC_NR`**（这条旁路按契约就是 VM 专用；C 侧「只有 VM 会调」是隐式的，
   这里落成显式判据），否则 `EPERM`；
2. **`phys_ram_covers(pa, len, align)`**：`pa..pa+len` 必须整段落进 boot 上报的物理 RAM——
   空闲 memmap ∪ 内核镜像 `[kern_phys_base, +kern_size)` ∪ 固件非约定快照 `reserved_regions`
   （模块 blob、UEFI 堆对象都在后者），**MMIO 窗按构造排除**（VM 的页表流量从不指向设备寄存器）；
   同时校验该手臂的对齐与宽度：u64 手臂 8 字节、整页零填 4 KiB；越界/错位/长度为零答 `EINVAL`。

谓词做成不吃全局的自由函数（`phys_ram_covers`，手臂侧用 `phys_ram_view()` 喂 boot 状态），
**宿主可测**——这正是「生产手臂是 riscv-only、宿主跑不到」时的正确缝。

**读数**：kernel mock（`--features mock`）**5 段 / 860 / 0**，新增 4 枚
`syscall::phys_ram_gate_tests`（空闲区/u64 槽、reserved 与内核镜像区、缝隙/MMIO 地址/跨区尾/
溢出地址、错位与零长度）；三架构 `cargo check -p minix-kernel` 各 0 error（riscv64 编过新闸）；
`rustfmt --check syscall.rs` 与 HEAD 基线同为 111 处（本笔新增 0 处）。登记侧：SD-22 的 H5 残量
与 SD-36 的「until then」句都改成已落并指向读数；台账对应行同步。

### 第二百零九笔（C 板·内核优先 · SD-20：装载路径的另一半——VM 自己的 `exec_bootproc` 也同步指令缓存，闸用 H5 同一套规则）

**台账与寄存器在本笔开头互相矛盾**：台账的 SD-20 行早已 `closed / L1`（契约、三架构实现、内核装载器尾调用
与 mock 记录全在），寄存器那份却写着 `open (untouched)`——读码后发现两边都没说全：内核的
`load_elf_into` 尾巴确实同步了，可**生产启动链的装载器是 VM 服务器**——
`os/servers/vm/src/vm_server.rs::exec_bootproc` 把每个启动进程的镜像经 VM Direct Map 逐页写进新根，
那些页从未经过任何同步。aarch64 的 I-cache 与这些 D 侧写入不粘，riscv64 首取前需要 `FENCE.I`；
QEMU 不建模该故障类，所以「全门绿」从来没有证明过它——寄存器说的「最高确定性真机定时炸弹」正好落在这条
腿上。

**修法（内核 + VM 两侧）**：
1. **内核**：`VmCtlParam::PteSyncIcache`（wire 23，`pa` 在 m1p1、长在 m1p2）。手臂按 **H5 同一套规则**
   设闸——调用者必须 `VM_PROC_NR`（否则 EPERM）、页必须落在 boot 上报的物理 RAM（`phys_ram_covers`，
   否则 EINVAL）——然后把该页经**内核自己的** Direct Map 视图（同一批物理行）交给
   `CurrentCacheSyncArch::sync_icache_range`。
2. **VM**：新模块 `os/servers/vm/src/icache_bridge.rs`；`exec_bootproc` 在写完每个**可执行段**页
   （文件字节 + .bss 头尾清零都落定之后）调一次。x86_64 与宿主测试编译掉整个调用（缓存相干/无内核可问）。

**读数**：kernel mock（`--features mock`）**5 段 / 861 / 0**（新增闸测试：非 VM → EPERM、VM + 越界页 →
EINVAL；H5 谓词族 4 枚仍在）；`minix-vm --lib` **546 / 0**；三架构 `cargo check -p minix-kernel -p minix-vm`
各 **0 error**（aarch64/riscv64 编过新桥）；`rustfmt --check` 触及件的自漂移与 HEAD 基线一致（新增区 0 处，
新文件 0 处）。**诚实边界**：门的绿仍不能证明真机行为，真机窗是权威；本笔记的是**结构半**——树内每一条
装载路径（内核两个装载器 + VM 装载器）的尾同步至此齐了。

### 第二百一十笔（C 板·内核优先 · SD-42：riscv 的 PLIC 与 UART 页从 Normal 缓存改为 Device，恒等映射分片改成架构通用）

**SD-42 的 aarch64 腿早已落地**（`PageFlags::DEVICE` → AttrIndx=1 → MAIR Attr1=0x00
Device-nGnRnE，恒等与高半两条别名一致，`arm64_mmio_device_attr_pin` 钉住），但**恒等映射的
设备分片在 Step-1 里只对 aarch64 写**——riscv 的 PLIC 与 **NS16550A UART 页**就那样落在
Normal 缓存里：`kernel/src/uart_irq.rs` 以 VA==PA 直取 UART（首源中断自检在用），QEMU 不报
缓存性差异，真机上 MMIO 的强序语义没有保障。这正是 SD-42 那条「真机会收账」的账。

**修法**：把分片做成**架构通用**——
1. `identity_device_windows()` 三臂给窗口：aarch64 = 平台 MMIO 窗；**riscv64 = 
   `minix_arch::kernel_identity_mmio_regions()`，并把 UART 页（`0x1000_0000`，4 KiB）加进这张表**
   （用户根回放同一份表，`uart_irq` 在任何根下都够得着）；x86_64 = 空（其 LAPIC/IO-APIC 靠固件
   MTRR 定 UC，页表没有逐 PTE 设备位）。
2. 两个**纯判定**（宿主可测）：`identity_block_touches_device`（块是否碰窗 → 决定按 2 MiB 分片）
   与 `identity_piece_is_device`（片是否交窗 → 发 DEVICE）。4 KiB 的 UART 窗会把它所在的 2 MiB
   片整体提为 Device——那一块是 QEMU virt 的设备区，不是 RAM。

**读数**：kernel mock（`--features mock`）**5 段 / 863 / 0**（+2 枚分片判定测试：riscv 形与
aarch64 形各自的块/片判定）；`minix-arch --features runtime-window` **22 段 / 373 / 0**（含按新
形状改写并**加强**的 `arm64_mmio_device_attr_pin`：除原三断言外再钉两个判定函数在场，防「判定被
内联回循环」）；三架构 `cargo check -p minix-kernel` 各 **0 error**；`rustfmt` 新增区 0 处。
**诚实边界**：QEMU 不区分 Normal/Device，pin 证明的是「属性到达描述符、映射点仍走该判定」，
真机窗仍是权威。

### 第二百一十一笔（C 板 · SD-34：防回长计数门升到 CI 级并跨 crate——宿主编译不到的 `#[test]` 不许再静默增减）

**这笔先把账对清**：SD-34 的**清扫早已闭**（台账 `closed / L1`，`os/arch/tests/arch_gated_audit.rs`
冻结 86 文件清单、漂移即红），寄存器的 `open (31/86)` 是陈旧读数；但寄存器推荐的那条
「count 每个 crate 的 `cfg(target_arch)` 门测试、防静默回长」当时只落在 arch 目录的测试清单里，
**没有 CI 级、也不跨 crate**。

**本轮补的正是那半边**：
- `tools/arch-gated-test-count.py`：统计**全 `os/`** 里「宿主测试形态下根本不编译」的 `#[test]`——
  两种形态都算：`target_arch` 属性栈直叠在 `#[test]` 上（含 `#[cfg(all(...))]`），以及
  `#[cfg(target_arch = …)]` 门住整块 `mod`、块内测试全计入；
- 逐文件计数冻结进 `tools/arch-gated-test-count-baseline.txt`（现算 **5 枚 / 2 文件**：
  `arch/src/arch/timer_irq_gate.rs` 3 枚编译存在性检查 + `libs/minix-platform/src/device_tree.rs`
  2 枚形状测试）；基线外**新增或缺失**都让门变红，并逐文件打印「基线 → 现在」；
- `--self-test` 用夹具覆盖「直接 / 模块级 / `cfg(all(...))` 都计入、宿主可跑的不计、新增即红」
  四种判别，随 `rules-consistency.yml` 常驻；**工作流路径加 `os/**`**——否则 os 里新增的门测试
  不会触发这道门，等于没接。

**读数**：`python3 tools/arch-gated-test-count.py --self-test` → 自检通过；
`python3 tools/arch-gated-test-count.py` → **5 枚 / 2 文件与基线一致**；工作流 YAML 解析通过
（7 个步骤）。**边界**：这道门管的是「测试被静默跳过」这一形态本身，不评价那 5 枚是否该继续
存在（timer 门三枚是每架构编译存在性检查，按设计就该在 arch 下；要迁成形状 pin 是另一笔的取舍）。

**合回读数（第二百一十一笔补记，SD34-GATE）**：主树在领取期间被 I21HOIST 一笔推进
（`a4394a7b0`，也改同一份工作流），本笔的提交父就是它 ⇒ 纯快进成功（`rewrite` 顶到
`052d977c8`）。主树复验：工作流 YAML 解析 **7 个步骤**、`os/**` 在 push/pull_request 两处路径里、
新步骤在场；`python3 tools/arch-gated-test-count.py --self-test` → 自检通过，
`python3 tools/arch-gated-test-count.py` → **5 枚 / 2 文件与基线一致**（退 0）。
**一处工具自错的账要记**：本笔的提交 diff 里那份工作流显示「整篇 95 行重写」——原因是父版
**blob 带 CRLF**（95 行 `\r`），而 `.gitattributes` 的 `* text=auto` 在入库时把触碰过的文件
归一为 LF（文件头自己写着这条规矩，批次一百一十三的同款先例）；本笔写回的 Python 只是如实
触发了归一，不是内容改写。**按策略保留归一结果**；`pattern-gate.sh` 的 P8 只扫
`*.ld/*.sh/*.rs`，yml 不在其列，不影响门。

**T1CUT 里程碑（2026-10-30，01 篇 T1 2c/3b 联合切根）**：码笔 `aab77e1c2`、合并笔
`9be4654da`。aarch64 UEFI 链从「旧快照 + 一页 `BootHandoff` blob」切到「四页 arena +
`GetMemoryMap`/分类/raw `ExitBootServices` 配对循环 + `x0=arena`」；接收端
`arch_boot_resume_v1` 把五块载荷解码进 T5 落地区后走原 additive 高半序列。旧
`bootstrap_to_kernel_image` 与 `handoff.rs` 删除，`BootPrepareResult` 保留给 x86/riscv
进程内路径。偏离一处：`uefi` 0.33 的 `memory_map_size` 是 crate 私有，map buffer 改用
raw 空缓冲探测加 `BUFFER_TOO_SMALL` 扩容；`uefi-raw 0.9` 成为 boot-shim 直接依赖，
`Cargo.lock` 未变。门读数：minix-boot **57/0**、boot-shim test-all **53/0** + 边界审计
**5/0**（新扩到第二条入口）、kernel mock **845/0/8**、三架构 kernel/kernel-image check
**0 error**、`test-shim-bootmarks-aarch64` **PASS**、合并后 `run_all` **39/0/0**
（`tmp/01-cboard-baseline-20261030/run-all-t1.log`）。`test-cmd-smoke-aarch64` 到达
`/etc/rc` 且串口有 `resuming high-half boot (handoff v1 arena)`；`du`/`ls` 目录读腿
在基线 `f03aa8ffc` 的干净树同样失败（`tmp/01-cboard-baseline-20261030/a64-smoke-baseline.log`），
属既有 A 板目录读问题，不计本批回归。基线取证同时确认：kernel mock 845/0/8、boot-shim
52/0 + 3/0、minix-boot 54/0、minix-arch runtime-window 270/0、run_all 39/0/0
（`tmp/01-cboard-baseline-20261030/`）。

**T3PARAM 里程碑（2026-10-30，01 篇 D-66 param_buf）**：码笔 `b860dd2cc`、合并笔
`3c6f81513`。`minix-boot` 新增 `boot_params.rs`（`normalize_param_buf`：NUL/空白分词、
可打印 ASCII、内部键拒绝、重复键首条胜出、整条记录截断）；`build_kernel_info` 增
`param_buf` 形参；UEFI 腿读 `EFI_LOADED_IMAGE_PROTOCOL.LoadOptions`，OpenSBI/直核腿读
DTB `/chosen/bootargs`（`all_nodes` 走查，避开 fdt 0.1.5 `find_node` 的 SD-38 缺陷——
`chosen()` helper 会在 QEMU 重建树上 panic）。`KernelInfo` 与内核零改动。门读数：
minix-boot **65/0**、boot-shim test-all **53/0** + 边界审计 **5/0**、kernel mock
**845/0/8**、三架构 kernel/kernel-image check **0 error**、合并后 `run_all` **39/0/0**
（`tmp/01-cboard-baseline-20261030/run-all-t3.log`）。真机读数：aarch64 直核注入
`/chosen/bootargs "ARCH=probe BOARD=qemu FOO=bar"` 后串口
`param_buf bytes=0x15 first=ARCH=aarch64`（ARCH 13 B + `FOO=bar` 8 B；BOARD 无来源
省略、外部同名键被拒），`test-smp-aps-aarch64` PASS；UEFI aarch64 腿
`param_buf bytes=13 rejected=0 dropped=0`（AAVMF 启动管理器未传 LoadOptions）。
riscv 直核面复用同一代码，其现有夹具模块表为空、走不到 builder，端到端读数留 D-73。

**T2X86 里程碑（2026-10-30，01 篇 NK1/C-29 x86 独立镜像）**：码笔 `bb32bbb0a`、合并笔
`9577f6eed`。x86-64 从「内核链进 boot-shim、进程内调 `arch_boot`」切到「shim 建根开分页 +
v1 arena + `rbx` 交棒 + 独立 `kernel.elf` 高半入口」；`reserved_regions` 的 kern_ident
输入从 `vm_handoff::build_identity_windows` 删除（只留架构 MMIO 恒等窗口），`dm_coverage`
源保留；模块 blob 改 `MaxAddress` 上界并验区间末端；`load_elf_into` 的恒等可读前提成文。
**同批修一处 ABI 真 bug**：`extern "C"` 在 `x86_64-unknown-uefi` 上是 Win64 约定
（rcx/rdx），与 SysV stub 不符，曾把 `KernelInfo` 指针当入口跳进栈而报 #UD；改
`extern "sysv64"` 后稳定。新增 `test-shim-bootmarks-x86.sh`（三 shim 行 + 接收端行 +
`entering scheduler`，连跑三次 PASS），`test-cmd-smoke.sh` 的 M14 行改为
`[raw] entering kernel image v1`。门读数：kernel mock **845/0/8**、minix-boot **65/0**、
boot-shim **53/0** + 边界审计 **5/0**、三架构 kernel/kernel-image check **0 error**、
aarch64 冒烟门 **PASS**、`test-cmd-smoke.sh` 到达 `/etc/rc`（仅 `du` 既有 A 板红）、
合并后 `run_all` **39/0/0**（`tmp/01-cboard-baseline-20261030/run-all-t2.log`）。

**D72VM 里程碑（2026-10-30，01 篇 D-72 kerninfo 映射责任移交 VM）**：码笔 `167197e44`、
合并笔 `c8b9ba08d`，哨兵修复 `72d83477c`/`cf99a24d0`、合并 `c1354d0b5`/`3ce21b9a3`。
`VmBootHandoff` 升 v7 加 `kerninfo_pa`；`minix-types` 增 `KERNINFO_USER_VA` 单一权威；
VM `init_page_table` 在建根时把该页只读映射到固定 VA（fork/exec 共用），内核
`vmctl_set_addr_space` 的每根注入块删除，源码序钉 `set_addr_space_no_boot_alloc_pin.rs`
2/0。**首轮全量 32/7**：新加的 `expect` 在 27 个未开 `qemu_test` 的 bootstrap 测试内核上
触发（它们跳过 kmain Phase B.5）；改用「未初始化即 0、VM 跳过映射」的显式哨兵后修红，
生产正确性由 rt-birth/user-trap 的 CPL3 kerninfo 读取门兜底。门读数：minix-types
**331/0**、minix-vm **543/0**、kernel mock **845/0/8**、三架构 kernel/kernel-image check
**0 error**、最终 `run_all` **39/0/0**（`tmp/01-cboard-baseline-20261030/run-all-d72c.log`）。
跨线登记：`os/servers/vm` 属 A 板，C 板 §6 已追加一行（无在途 A 领）。

**D73RV 入口契约实测（2026-10-30，未接线，阻断在宿主 U-Boot）**：宿主 U-Boot
2025.10-0ubuntu0.24.04.4（`/usr/lib/u-boot/qemu-riscv64_smode/uboot.elf`）实测：
① `test-riscv64-uboot.sh` 的 distro-boot 链在本机基线就停在 `Hit any key to stop
autoboot` 后不再执行 `boot.scr`（与 D-73 改动无关）；② 手工控制台链
`fatload virtio 0:1` 成功，但 `bootelf <addr>` / `bootelf -p` / `bootelf -s`
均立即返回提示符、不执行探针（ELF e_entry=0x84000000、`_start` 符号一致）；
③ QEMU 对 `-append` 要求 `-kernel`，`dumpdtb -append` 通路不可用。结论：U-Boot
`bootelf` 的 argc/argv 与 `-d` 参数通道在本机无法用现有宿主件实证，D-73 的入口
契约闭合门未通过，shim 腿不接线、不落半成品代码；下一步需在 CI 的 T-10 窗口
（`qemu-tests.yml` 的 riscv64-uboot 作业）用可复现的 U-Boot 件重测，或换
`go` + 固定内存槽契约并重新裁决。探针源码与日志在 `tmp/uboot-probe/`、
`tmp/uboot-manual.log`（会话产物）。

**C2BWAKE 试形取证（2026-10-30，未合并，已回滚）**：按 C `proc.c:1641-1650` 在
`sched_enqueue_with` 补「入队到不同且 idle 的目标核 → 发调度 IPI」、删
`clamp_cpu_to_bsp`、中断入口清 `cpu_is_idle` 时重启本地定时器。三架构
`test-smp-aps`（`-smp 2` 与 `-smp 4`，aarch64/riscv 用改参脚本）全 PASS；但
`run_all` **38/1**：`test-e5-ds` 用户态腿出现 `vector 0x0e err 0x10 rip=0`
取指故障——旧注释警告的「AP 跑用户进程后 VM 跨 CPU 服务腐蚀」再现。结论：
SD-24 arm2 不能先于 C1（SD-10 污染案 / PD-16）结案落地；本线已 `git checkout -- .`
回滚，claim 已释放，C 板 C2b 行改 `⏸ 绑 C1`。取证
`tmp/01-cboard-baseline-20261030/run-all-c2b.log`。

**C7ARCH 里程碑（2026-10-30，C 板 C7/C18 文档半）**：码笔与合并笔见 `git log --grep C7ARCH`。
06 篇 §2.7 装载器尾缓存同步段补 `[ARCH: cache-sync]` 标识；04 篇 aarch64 MMIO
段补 `[ARCH: mmio-device]`（恒等/高半两别名同 Device 属性、2 MiB 分片与对齐
断言；该篇没有 §1.114，落点按实际段落）；14 篇 §3.3 补 PD-21 用户地址防线
边界（软件层翻译/页表检查承担防线，SMAP/SMEP 位随写下沉批次再谈）。
`doc-style-lint --diff` error 0。C 板 C7/C18 行改 ✅。C7b 余 I-25 与文档门。

**D59PAGE 里程碑（2026-10-30，D-59 阶段一/五）**：码笔与合并笔见 `git log --grep D59PAGE`。
`boot_validate_and_prepare` 提取 `choose_kernel_page_size`：先查
`P::supports_huge_page` 再比布局对齐；「1 GiB 不支持但布局按 1 GiB 对齐」的机器
改选 2 MiB 回退，不再选一个不支持的尺寸到 `map_huge` 才失败。四条纯函数测试
覆盖支持/不支持/仅回退/都不可用；kernel mock **849/0/8**（845+4）、x86 QEMU
`hello-boot` PASS、HEAD 干净树全量 `run_all` **39/0/0**（`tmp/01-cboard-baseline-20261030/run-all-final.log`）。D-59 余四阶段（`PageSize`/能力 API、事实/规划与策略拆分、
`map_huge` 防御与 SMP 预检、机械重命名）与覆盖损失门仍开放。

**D59SIZE 里程碑（D-59 阶段二）**：码笔 `916d7346f`、合并笔 `4dc6a797f`。
`PageSize` 新类型（≥4 KiB 的 2 的幂，`try_from_bytes`/`try_from_usize`/`try_as_usize`
可失败）；`HugePages` 的尺寸常量与 `map_huge` 全部改用它；`supports_huge_page(PageSize)`
去掉默认实现，x86-64 用 CPUID.80000001H:EDX.GBPAGES 回答 1 GiB；删除
`HugePages::supports_1gb_page` 与 `BootDmInstaller::supports_1gb_page` 两个入口，
DM 规划以 `one_gb: bool` 接收能力事实。门读数：minix-arch runtime-window **273/0**
（270+3）、kernel mock **849/0/8**、三架构生产 check 0 error、kernel-image
`check-layout all` PASS；x86 冒烟到 scheduler/T4（du 读侧为既有 A 板红，基线同报）。

**D59POLICY 里程碑（D-59 阶段三 A/B）**：码笔 `007bcabdb`（arch）+
`d5d18b3a7`（kernel）、合并笔 `8d99bb774`。`Paging` 增加 `phys_addr_limit()`（x86
先查 CPUID 最大扩展叶再读 80000008H；ARM64 以 `ID_AA64MMFR0_EL1.PARange >= 0b101`
为前提返回 1<<48；RISC-V 平台声明 1<<48）与逐端点 canonical 的
`validate_virtual_range()`；`BootArch`/`CurrentBootArch` 绑定 Paging/DirectMap/
DmInstaller；`DirectMapArch` 增加 `KERNEL_DIRECT_MAP_SIZE`/`KERNEL_DM_VA_END_LIMIT`
与三架构编译期断言；`DirectMapLayout`/`PageSupport`/`BootPageSupport`/
`detect_boot_page_support` 落码；`DmPagePolicy`/`DmWindow`/`plan_dm_policy` 与
`plan_range_units` 从策略取尺寸；`preflight_boot_dm_range` 在一次写表前预检全部
候选，`install_prevalidated_boot_dm_range` 只重放已验证序列；内核新增
`boot_policy.rs`（`KernelLayout`/`KernelMapPolicy`/`BootPagePolicy`/
`plan_kernel_mapping`/`plan_boot_pages`），两条启动入口一次规划、分别取
`policy.kernel()`/`policy.dm()`。首跑 `run_all` **30/9** 暴露三处真实问题并当批修正：
①boot module 候选用裸字节长度（页尾不齐，x86 实测 `[0x1db79000,0x1db9a500)`）——
按页向上取整；②aarch64 能力门要求 48 位 PA，而测试 CPU `cortex-a57/a72` 只有 44 位
——QEMU 改 `-cpu max`（run_qemu 与四个专用脚本/xtask 同步）；③riscv 测试内核
`_start` 在 `.text.init`，QEMU 从镜像基址进入，泛型单态化代码可排在 `_start` 之前
（rt-birth 基址变成 `establish_boot_dm` 后静默挂死）——三个使用 `.text.init` 的
link.ld 显式前置。修正后三架构收口读数见 D59DEFEND。

**D59DEFEND 里程碑（D-59 阶段四）**：码笔 `6b8c9ff10`、合并笔
`f9bab95c2`。三架构与 mock 的 `map_huge` 在写表前检查能力（`NotSupported`）、
尺寸对齐、区间 `checked_add`、canonical 范围与 `phys_addr_limit`（`InvalidAddress`）；
mock 增两条「错误不落表」负控。aarch64 测试平台改用 `-cpu max`；riscv 三个
`.text.init` 内核链接脚本前置入口段。门读数：minix-arch **286/0**、kernel mock
**850/0/8**、xtask 17/0、三架构生产 check 0 error、kernel-image `check-layout all`
PASS、x86/aarch64 shim 冒烟 PASS、aarch64/riscv rt-birth PASS；HEAD 全量 `run_all`
**39/0/0**（`tmp/d59size-phase2/run-all-defend.log`）。

**D59RENAME 里程碑（D-59 阶段五）**：码笔 `30e14c6ce`、合并笔
`5479b8083`。`DmCoverageArch`→`BootDmInstaller`、`dm_install_leaf`→`install_leaf`、
`CurrentDmCoverage`→`CurrentBootDmInstaller`，四个实现与聚合 `Arch::DmInstaller`
关联类型随改，模块 `arch/dm_coverage.rs`→`arch/boot_dm.rs`；内核自己的候选组装层
`kernel/src/dm_coverage.rs` 不改名。01 篇 §3.6/§3.7/§4.5 与 07 篇 §4.3 的逐字引文
同步到阶段一至四后的真实 API。验证：`rg 'DmCoverage|dm_install_leaf|arch::dm_coverage|
arch/dm_coverage|establish_dm_range' os` 零命中、arch **286/0**、kernel mock
**850/0/8**、三架构 check 0 error、check-layout PASS、两架构冒烟 PASS、
`doc-style-lint --diff` error 0；HEAD 全量 `run_all` **39/0/0**
（`tmp/d59size-phase2/run-all-rename.log`）。D-59 五阶段至此全部落地；覆盖损失门
仍按设计 §10 独立立项，不在本轮闭合。

**C1VERIFY 里程碑（C1 内容级扩片）**：码笔 `ddf977298`、合并笔
`5092e5501`。`nk4a_copy_verify` 在分片挂起前先核已拷贝前缀 `[0, done)`，恢复后的
重试再走全量路径——两段合起来覆盖本次拷贝实际搬过的每个字节，挂起点（未映射页）
不读。真机门：`test-atf-riscv64.sh` **RESULT: PASS（36/36：34 passed + 2 skipped +
0 failed/broken）**；串口 `nk4c: csc-verify armed` 在、csc 内容不一致 **0** 条；
`nk4c: mset-verify armed` 在、零 HIT；ptwb HIT 3 条均为既有已知良性形状。串口存
`tmp/c1-atf/rs64/serial.log`。C1 按 PD-16 仍保持开放（仪器覆盖已补到内容级）。

**I25 里程碑（C 板领地注释英文化与锚点内联）**：码笔 `0d235964a`、合并笔
`e754ea77a`。范围是 C 板领地（`os/kernel`、`os/arch`、`os/plat`、
`os/libs/minix-platform`、`os/libs/minix-boot`、`os/boot-shim`、
`os/kernel-image`、`os/qemu-tests`）的 **155 个 `.rs` 文件**：全部中文注释
译为技术英文，删除不可达的协作/工单锚点（NK4-*、D-*、SD-*、§续/续-*、
C-/I-/L####、TODO-*、P-XXX-*、S-/K/M/B/R/FIX-/V 系列、edgeN、甲/乙/丙案）
并把技术背景写回注释本身；`[ARCH: ...]` 与活约定 `[SD19-gate]` 保留，
`rewrite-notes/` 章节引用与 C 真值源行号保留。安全门为非注释 token 与
HEAD 逐文件对比：**0 个 `CODE-CHANGED`**。读数：
`check_comments.py` → `files=155 cjk_comment_lines=0 anchor_kinds=0 bad=0`；
`check_broad.py` → `BROAD-ANCHOR-LINES=0`；三架构生产 check 各 0 error、
kernel mock 850/0/8、minix-arch runtime-window 286/0、kernel-image
check-layout all PASS、`run_all` **39/0/0**（`tmp/i25-batches/run-all.log`）。
A/B 领地未越界编辑：578 文件 / 24,816 条注释 CJK 行的移交清单、按目录批次
与归属建议落在 `tmp/i25-cross-territory-handoff.md`；I-25 在全仓口径下只剩
这份 A/B 移交面，C 板领地已清零。

**I25AB / I25POST 里程碑（全仓注释清零）**：在 C 板领地 155 文件之后，
A/B 领地 576 文件完成同类整改（合并笔 `9c9c24bbe`），合并时主树新增的
15 个 textfilter/text-games 文件再补一笔 29 文件翻译（合并笔
`f933b53ab`）。最终全仓 **1,549 个 Rust 文件：注释 CJK 0 行、广义锚点
0 行**；654 个纯注释文件与基线的非注释 token 零差异，另
`os/servers/vm/src/pagetable/sim.rs` 的 `SimPaging` 按 D-59 补齐
`phys_addr_limit`/`validate_virtual_range`（独立提交 `9d073dfdb`）。
安全门 `check_comments.py` / `check_broad.py` 全绿；workspace 8,284
passed / 0 failed、minix-vm 四形态 543/562/543/561、kernel mock
850/0/8、arch 286/0、boot-shim test-all 53/0、三架构 check 0 error、
kernel-image check-layout PASS、`run_all` 39/0/0。审计落
`tmp/i25-final-audit.md`；I-25 至此全仓闭合。

**C5SMP 里程碑（riscv ATF -smp 4 复跑）**：码笔 `a97dde870`、合并笔
`d0371b561`。`test-atf-riscv64.sh` 增加 `RS_ATF_SMP`（默认 2），拓扑 dump
与点火共用同一 CPU 数；`RS_ATF_SMP=4` 全量三轮均 **36/36（34 passed + 2
skipped + 0 failed）**，csc 内容不一致 0、mset 零 HIT，ptwb 命中均为已知
良性形状。C 板 C5 行更新为 `-smp ≥3` 已验三轮，余量只剩 AP 并行调度
（绑 C2b/PD-16）。PD-16 的止损线要求 N 在首次执行前登记，这三轮只作
观察读数，不构成结案；C1 仍按裁决保持开放，C2b 维持回滚。

**C4STUB 里程碑（x86_64 ATF 出生腿）**：码笔 `04326fb44`、合并笔
`04ba75d4f`。`tools/atf-c-compat/startup-minix.c` 新增 x86_64 入口：
内核出生契约的 RBX（ps_strings）转 SysV 第一参数 RDI，16 字节对齐后调
`startup_c_main`；新增 `tools/atf-c-compat/tests/` 的静态 ET_EXEC 子进程
与 ptrace spawn 夹具（exec 后写入合成 ps_strings 并设 RBX），实测
`FIXTURE ps=0x... argc=1 argv0=probe-argv` PASS；x86_64/aarch64/riscv64
三架构 `startup-minix.c` 均可编译。C4 仍门控于 glibc 启动/系统调用桥与
x86_64 ATF 36/36，`ATF_BOOT_LEG_READY` 暂不放行。

**C4GLIBC 取证（未合入）**：x86_64 ATF 的 glibc 启动桥在临时 claim 里做到
「libc 静态归档定义改写 + 隐藏 `__write` 拦截 + freestanding p1 `P1-OK` 真机
出现」，但 glibc TLS 需要每进程 FS base：Minix-RS 的 x86_64 上下文没有
FS base 字段/MSR 切换，`arch_prctl(ARCH_SET_FS)` 无对应内核调用，用户 `fs`
基址保持 0，`init_cpu_features`/TLS 访问 fault（串口到 SHIM/PREJUMP 为止）。
结论：C4 客态运行面要打通需先加内核每进程 FS base 或改用非 TLS 的
x86_64 libc；半成品已弃树，未合入，`ATF_BOOT_LEG_READY` 仍不放行。

**PD16-N3 里程碑（机械止损线达成）**：按 PD-16 第十轮定稿，先登记固定
N=3（C 板 C1 行落笔于 `e7c99cb57` 之后），再以 `RS_ATF_SMP=4` 连跑
3 轮 `test-atf-riscv64.sh`：每轮 **36/36（34 passed + 2 skipped +
0 failed/broken）**，`csc-verify armed` 在、内容不一致 0，`mset-verify`
armed 在、零 HIT，ptwb 命中均为既有已知良性形状。覆盖完成度（tf-guard /
csc 含挂起前缀 / mset / ptwb）+ N=3 零命中同时成立，PD-16 止损线达成，
C1 由 `🔄` 转 `✅（转长期观察）`，C2b 前置解除。串口：
`tmp/pd16-n3/round{1,2,3}/serial.log`。

**C2B / C15 里程碑（AP 唤醒与撤钳）**：码笔 `3626b52d5`（C2B）、合并
`8b4c51514`；C15 完成边 `C15BARRIER`。`sched_enqueue_with` 在目标 CPU
`cpu_is_idle` 时发 `send_sched_ipi`，并在 IPI 前 `fence(Release)` 发布
运行队列写（PD-19 完成边）；删除 `clamp_cpu_to_bsp` 及其调用/测试，cpu
亲和可落到 AP。门读数：kernel mock **849/0/8**、HEAD `run_all`
**39/0/0**（含 `-smp 4` 的 test-smp-aps 与 e5-ds；此前 38/1 的 e5-ds
取指故障未再现）。C 板 C2b/C5/C13/C15 置 ✅；C19 由绑 C1/C2 改为可执行
CC-INSTRUMENT 结案批。

**C2C 里程碑（撤钳止损）**：合并 `72208b155`。C2B 段所述「cpu 亲和可
落到 AP」由本条撤回：撤钳后 SCHED 的 `pick` 会把非系统进程放到非 BSP
（`os/kernel/src/misc.rs` 的 `processors_count = smp.ncpus()`，
`ChangeMask::ALL` 下发 cpu），而接收侧未齐——aarch64 的 SGI 0 从未在
GIC 使能（`os/plat/src/arm64/interrupt.rs` 的 `init_redistributor` 全
屏蔽，注释所写 IPI bring-up 不存在）；riscv64 `riscv64_swi_arm` 只清
`sip.SSIP`、丢弃调度 flags；三架构都没有唤醒后清 `cpu_is_idle` + 重启
AP 本地定时器的臂（`restart_local_timer()` 为 no-op，`stop_local_timer`
已 mask）；riscv64 远端 `fence.i` 未落（`os/arch/src/riscv64/cache_sync.rs`
明文前置）。恢复 `clamp_cpu_to_bsp` 作止损，唤醒生产者保留但惰性；11 篇
§3.3/§3.4/§5.3/§5.4 回写为「生产者已在、接收侧未齐、钳制兜底」。门读数：
kernel mock **850/0/8**（+1 clamp 单测）、minix-arch **286/0**（含
riscv64_cache_sync_pin 3/0）、三架构 `minix-kernel` check 0 error。C 板
C2b/C5/C13 转 ⏸，新增 C2C 行；接收侧四件同批落地并经三架构真机门后才可
再撤钳。

**C7B-08 里程碑（08 篇锚点漂移）**：合并 `5b60bf6de`。
`os/kernel/src/kerninfo.rs` 已改为 `pub use minix_types::KERNINFO_USER_VA`，
08 篇原锚点 `os/kernel/src/kerninfo.rs:const KERNINFO_USER_VA` 被
`anchor-resolve.sh` 报 ZERO-DEF；改指权威定义
`os/libs/minix-types/src/types/boot.rs:const KERNINFO_USER_VA` 并注明内核侧
是再导出。08 篇锚点 **134/134**、断链 **0**、`doc-style-lint --diff`
error 0。C7b 余项收窄为正式文档计数口径与改名前置。

**C2D-A64SGI 里程碑（aarch64 SGI 片）**：码笔 `4e6a73f13`、合并笔
`426212ff6`。先定位到一个真实
接线缺陷：`os/arch/src/arm64/smp.rs` 的 `set_gicd_base` 全仓无调用者，
`GICD_BASE` 恒 0；基线全镜像的晚段 panic 正是首次 IPI 在 `GICD_SGIR`
偏移 `0xF00` 处取指/数据中止（ESR `0x96000046`、FAR `0xf00`）。本片：
① `arch_init` 从 `Gicv3Desc` 把 GICD 物理基址经 `set_gicd_base` 接入
（`gicd_base()` 在未初始化时显式 panic，不再写 0 地址）；② 控制器保存
`gicr_stride`，新增 `enable_sgi` 按 `hw_id` 写该 CPU 重分发器的
`GICR_WAKER`/`GICR_IGROUPR0`/`GICR_ISENABLER0` 并打开 CPU 接口；
③ `send_sched_ipi` 按 `GICD_CTLR.ARE_NS` 选 `ICC_SGI1R_EL1` 或
GICD_SGIR；④ aarch64 IRQ 分派特判 SGI 0，走
`sched_handler_full` + `preempt_current_on_ipi` + `complete_hardware_irq`
（不再让通用路径 mask 后返回 Spurious）；⑤ `sched_enqueue_with` 补 C 的
`else` 语义，只对**别的** CPU 发唤醒 IPI。真机全镜像
`test-smp-aps-aarch64.sh`：`ap-arrived=1`、`panic=0`、PASS 并走到 rc
marker。**新 blocker**：aarch64/riscv 的生产 AP 在 `ap_early_entry` 里
`wfi` 停车，从未进入 `scheduler_loop`（只有 x86 有 `smp_ap_tail`），
故 AP 侧 SGI 使能与端到端调度仍不能验；C2D 余下三件（riscv flag 应用、
唤醒后重启 AP 定时器、riscv 远端 `fence.i`）与两架构 AP 进环同批后才能
撤钳。门读数：kernel mock 850/0/8、minix-arch pin 4/0、aarch64 check
0 error、`run_all` 39/0/0（`tmp/c2d-runall.log`）。

**C2D-APENTRY 里程碑（aarch64 AP 进调度环）**：码笔 `3c448bb51`、合并
`1a5e389db`。`ap_early_entry` 加 `AP_TAIL` 注册门：收敛点先快照
record（logical/hw_id/stack_top），置 AP_ARRIVED，有 tail 则转 kernel
continuation，否则 wfi 停车。kernel 侧新增 aarch64 `smp_ap_tail`：
MPIDR→logical 身份表（原 `ap_cpu_id_readback` 恒 0，AP 进环会拿 BSP 的
per-CPU 状态）、boot_ack、per-CPU stack/VBAR、ptproc、SGI+timer PPI
使能、本地定时器、`ap_finish_booting`、`scheduler_loop`；`smp_init`
seed BSP 身份、注册 tail、每核清 AP_ARRIVED、补 `wait_for_aps`，其后
打印 `nk4c: ap-irq-ready cpu=<logical> sgi=1 timer=1`。
`test-smp-aps-aarch64.sh` 的 PASS 判据改为 ap-arrived + ap-irq-ready +
panic=0 + ap-timeout=0。真机全镜像读数：`ap-arrived=1 /
ap-irq-ready=1 / ap-timeout=0 / panic=0`，走到 rc marker；`run_all`
39/0/0。riscv AP 仍在 `ap_early_entry` 里 wfi 停车，是 C2D 余项里最大的
一件；钳制继续兜底。

**C2D-RVAPENTRY 里程碑（riscv AP 进调度环 + 零根 walker 守卫）**：码笔
`28077ea70`、合并 `2e17f365e`。`ap_early_entry` 加 `AP_TAIL` 注册门并
快照 record；kernel riscv `smp_ap_tail` 做 boot_ack、per-CPU
stack/sscratch、stvec、ptproc、`sie.SSIE+STIE` 使能、本地定时器、
`ap_finish_booting`、`scheduler_loop`；`smp_init` 注册 tail、每核清
AP_ARRIVED、补 `wait_for_aps`，其后打印 `nk4c: ap-irq-ready
cpu=<logical> sgi=1 timer=1`。身份锚改为内核栈区间（riscv 无 S 模式
hartid 可读、tp 会被 trap return 恢复、sscratch 载用户 sp），BSP 回退
`seed_bsp_masks` 记录的槽位。真机暴露出零根 PTE walk 类缺陷：AP 首次
copy 用 root=0 走表，walker 把 `DM+0x7f8` 当 PTE 读而 fault；三架构
walker 统一加零根守卫返回 None。读数：riscv 全镜像 `ap-arrived=1 /
ap-irq-ready=1 / ap-timeout=0 / panic=0`（BSP rc marker 未达，脚本按
AP 判据 PASS）、`run_all` 39/0/0。SSI 跨核投递仍受固件未委托限制，
脚本只证门状态与进环；唤醒后定时器重启、riscv flag 应用与远端
`fence.i` 仍是 C2D 余项。

**C2D-WAKETIMER 里程碑（唤醒后重启 AP 定时器）**：码笔 `49a373a21`、
合并 `3bbe4132d`。`ClockArch::restart_local_timer(cpu_id, icr)` 三架构
落地（x86 LVT 解屏蔽+ICR、arm64 `rearm_next_tick`+CNTP enable、riscv
STIE+SBI 重装），kernel `restart_local_timer` 加 BSP-PIT 守卫并读校准
ICR；`context_stop_idle_current` 在 x86 SCHED IPI/0xF1/PIT、aarch64
SGI/timer、riscv SWI/timer 入口执行 clear+restart。读数：x86
`test-smp-aps` PASS（AP local LAPIC ticks observed）、aarch64/riscv 全镜像
`ap-irq-ready=1 / panic=0`、`run_all` 39/0/0。C2D 余 riscv flag 应用、
riscv 远端 `fence.i`、READY 有界 ack；钳制继续兜底。

**C2D-RVFLAGS 里程碑（riscv SWI 臂应用 flags）**：码笔 `dac9c2237`、
合并 `308af6a6f`。`riscv64_swi_arm` 在 BKL 内由“只 drain flags”改为
`sched_handler_full`（应用并清除 STOP/SAVE_CTX/VM_INHIBIT）+
`preempt_current_on_ipi`，删除 `nk4c: swi-ack` 诊断并补上缺失的
`bkl_unlock`。读数：`test-smp-aps-riscv64` PASS（ap-irq-ready=1 /
panic=0）、`test-smp-ipi-riscv64`（aclint=on）PASS、`run_all` 39/0/0。
C2D 余 riscv 远端 `fence.i`、READY 有界 ack；钳制继续兜底。

**C2D-RVFENCE 里程碑（riscv 远端 fence.i）**：码笔 `57098d93d`、合并
`f32fa8f85`。`cache_sync` 在本地 `fence.i` 后调 SBI RFENCE
`remote_fence_i`（EID `0x52464E43`/FID 0，`hart_mask_base=-1` 覆盖全部
可用 hart），返回码非零 fail-fast；共享契约与 riscv 模块文档改为已
实现，`riscv64_cache_sync_pin` 翻转为要求广播形状；11/sched/lib/smp
注记回写。读数：riscv 全镜像 PASS（ap-irq-ready=1/panic=0；初始 ELF
装载的实际 RFENCE ecall 返回 0）、`test-smp-ipi-riscv64`（aclint=on）
PASS、`run_all` 39/0/0。C2D 只剩 READY 有界 ack；钳制继续兜底。

**C2D-READYACK 里程碑（有界 ack/READY）**：码笔 `b9013b981`、合并
`156e6ea0d`。x86 的内联 ack/READY 等待抽成 `wait_ap_boot_ack`（双时钟
500 tick / 20e9 TSC，轮询 `observe_boot_ack` 后置 READY）+
`warn_ap_boot_timeout`；aarch64/riscv 的 `smp_init` 在 AP_ARRIVED 后
同样调用（两次独立发布，单次采样会输竞态）。aarch64/riscv 全镜像
PASS，`WARNING: not all CPUs booted` 计数 0，`run_all` 39/0/0。C2D
前置至此全绿；撤钳实施另列 C2E（riscv 默认固件 SSI 投递限制仍在，
按实测决定窄钳/固件门）。

**C2E 里程碑（撤钳：x86/aarch64 撤除、riscv 窄钳）**：码笔
`dcf99d197`、合并 `558352fa0`。`sched_proc` 删除全局
`clamp_cpu_to_bsp` 调用/函数/测试；x86_64/aarch64 的 cpu 经 Step 3.5
迁移与 Step 7 affinity 落到 AP（保留范围/就绪校验）。riscv 默认固件
不投递跨 hart SSIP（`test-smp-ipi-riscv64` 仅 `aclint=on` PASS），
保留 `cfg(riscv64)` 窄钳：校验后重写回 BSP，防 AP-homed 进程饿死。
读数：`run_all` 39/0/0（x86 `test-smp-aps`/`test-smp-ipi` PASS、
e5-ds `chain ok`）、aarch64/riscv 全镜像 PASS（ap-irq-ready=1 /
panic=0）、kernel mock 849/0/8、三架构 check 0 error、riscv pin 6/0。
C2b/C2E 至此收口；riscv SSI 固件门保留为平台前置。

**C7b 里程碑（计数口径与改名前置）**：码笔 `4b0d4e560`。README 定义
“正式文档数”=顶层 `NN-*.md`（排除 `NN-todo.md`）的机械计数，附复算
命令；01 阶段旧表 38 是迁移前陈旧读数，按口径改为 23，其余 stage
读数一致。新增“文档改名与拆分的前置”：改名/拆分必须与全部代码注释
同笔，提交前重跑 `tools/check-rs-unwired.sh`（PASS）；附引用分布
复算命令与当前头部读数。README style error 0、断链 0。C7b 余项清零。


**C4B 里程碑（x86_64 ATF 客态腿：非 TLS picolibc）**：码笔 `249a5c37b`、
合并 `5f6d2c4f8`。用户对 A/B 两案未返回裁决，按 B 案可逆默认落地，未加
内核每进程 FS base。`tools/vendor-picolibc-x86_64.sh` 本地构建 picolibc
1.8.6（显式 `-Dthread-local-storage=false`）；`x86_64` 腿改链 picolibc +
compat 接管面 + `startup-minix.c` 的 x86 `_start`；`sys-bridge.c` 补 x86
`syscall`（RDI=&Message）与 `int 0x21`（RAX=端点/RBX=消息/RCX=3/R10 状态）
桥。真机点火逮到一处平台级陷阱：x86 的 `libsemihost` 只有 `e9_io.o`，其
putc 是 ring 3 特权指令 `out %al,$0xE9`，首个 `'P'` 即 #GP；`stdio-minix.c`
改为在 x86 直接定义 stdin/stdout/stderr 三个 FILE 流，e9_io.o 不再被拉入。
xtask 的 `ATF_BOOT_LEG_READY` 放行 x86_64，新增
`os/qemu-tests/test-atf-x86_64.sh`（18 案 + p1/p2/p3、OVMF/q35、按
`atf-plan.txt` 计 36 条终态）。宿主执行腿另立 `x86_64-host`（glibc 原语义），
`tools/atf-host-suite.sh` 默认值随改并首次自建库腿。读数：x86 ATF
**36/36**（34 passed + 2 skipped，0 failed/broken；证据件
`suite_sha256=866e091d66d1…`、`progs=18`）、宿主腿 36/36、`run_all`
39/0/0、kernel mock 849/0/8、三架构 kernel/kernel-image check 0 error；
aarch64/riscv 单例交叉编译仍过 ehdr 校验。已知非本笔：`test-cmd-smoke`
的 du 目录读腿仍红（A 板 C-57）。C4 行与 `P-X86-01` 台账同批回写。


**C5BSP 里程碑（riscv 选后 BSP 身份 + ATF skip 面）**：码笔 `27e4fe980`、
合并 `54818cabd`。`init_smp_state` 不再直接信任 DTB 的 `bsp_id`，改从
`BOOT_HART_ELECTED` 解析真实 BSP（OpenSBI 可能把控制交给任意 hart；
QEMU `-smp 4` 实测进过 hart 1/2），`seed_bsp_masks` 随身份移动清旧槽、
置新槽并迁移 VM ptproc；启动集的 `p_cpu` 在调度器启动前按
RTS_SET(NO_QUANTUM)→store→RTS_UNSET 迁到 BSP（`bsp=0` 时零变化）。
`test-atf-riscv64.sh` 的 skip 路径改由 `atf-plan.txt` 派生 prog 清单、
不重跑 atf-face（未派生清单的跳过分支会留下空清单并把 `rc.imgrd` 写到比 `imgrd` 新）。
读数：riscv ATF `-smp 1` **36/36**（34 passed + 2 skipped）、x86
`test-smp-aps` PASS（cpu1=2/cpu2=2/cpu3=0）、`test-smp-aps-aarch64` PASS、
`test-smp-aps-riscv64` PASS、kernel mock 850/0/8、三架构 check 0 error。
C13 的 cpu3=0 同笔定为空闲停表设计。

**C5SMP2 里程碑（riscv 用户返回按 CPU 重锚 sscratch）**：码笔
`88eadff27`、合并 `d03fbb17b`。根因：`KERNEL_TRAP_STACK_BASE` 是全局
标量，AP 的 `init_ap` 把自身栈顶写入；BSP 下一次 `restore_to_user` 从
该标量重锚 sscratch，用户陷阱落到 AP 栈。riscv 的 `current_cpu_id()`
按栈区间识别 CPU，于是 VM 的 kernel call 被记到 AP 的 idle 进程
（`KC cpu=1 nr=-4`），`dispatch_vmctl` 前的权限检查失败，VmDm 的
PteWrite 代理不执行，`VM_HEAP_BASE` 未映射，VM 在 `memset` 处 store
page fault（`scause=0xf`）。修法：调度器在每次 `restore_to_user` 前按
当前 CPU 重写该标量与 sscratch（BSP 用 `kernel_info.kern_stack_top`，
AP 用 `ap_kernel_stack_top`）。读数：riscv ATF `-smp 1`/`-smp 2`/
`-smp 4` 均 **36/36**（34 passed + 2 skipped，0 failed/broken；
`tmp/c5smp2-smp1f.log`、`-final2.log`、`-final4.log`）、kernel mock
850/0/8、三架构 kernel check 0 error。C5/P-RV-01 全闭。


**D-73 入口契约实测（U-Boot bootelf 支持面）**：本机 U-Boot 2025.10 的
`help bootelf` 输出 `Usage: bootelf [-p|-s] [address]`——没有 `-d`，也没有
image 之后的 `argc/argv` 通道；设计注 §五引用的 `bootelf [-d ...] [args]`
在这个版本上不可用。手工串口驱动 U-Boot 已让 `boot.scr` 真正执行，
`fatload virtio 0 $loadaddr boot.elf` 成功，但 hello-boot 的 ELF LMA
`0x80200000` 与 U-Boot 自身装载地址重叠，`bootelf $loadaddr` 当场
`Load access fault`（EPC `0x8f73d74e`、RA `0x8f73d780`）并复位。⇒
`-d` + 固定槽回退在现装 U-Boot 上不可实现；固定槽路线也必须先做 LMA
在 U-Boot 之上的独立 `kernel-shim.elf`/`kernel.elf` 产物，并解决
U-Boot 预载模块、BootFileTable、DTB 固定槽与池基去 QEMU 化。D-73 保持
⏳，门控钥匙为：T-10 CI 窗口换支持 `-d`/argv 的 U-Boot 重测；若仍不支持，
按 `go`/`bootelf` 固定地址 + 独立 LMA 产物重裁入口契约。


**C5SMP3 里程碑（riscv 入口栈 per-hart sscratch）**：码笔 `7d7330b7b`、
合并 `5ebf01950`。上一笔在每次 `restore_to_user` 前刷新共享标量
`KERNEL_TRAP_STACK_BASE`，只是把共享格改成正确值；本笔按 trait 已有的
per-CPU 语义收尾：删除共享标量，riscv 的 `init`/`init_ap`/
`set_kernel_stack` 只写当前 hart 的 `sscratch`；`TrapReturnArch` 增
`prepare_user_return(cpu_id, stack_top)` 默认 no-op 钩子，调度器经 trait
调用，riscv 实现只写本 hart CSR，返回腿汇编不再读共享符号。trait/impl
文档写死“每次用户返回前保证当前 CPU 入口栈最新、实现不得使用共享状态”。
测试：`MockTrapReturn` 记录 `(cpu_id, stack_top)`；新增
`riscv64_percpu_trap_stack_pin` 三条源码钉（无共享符号、无 `csrw
sscratch`、契约句），`riscv64_return_leg_pin` 改为“返回腿不得自行写
sscratch”。读数：riscv ATF `-smp 1/2/4` 均 **36/36**（34 passed +
2 skipped，0 failed/broken）、minix-arch runtime-window 全绿、kernel mock
850/0/8、三架构 check 0 error、合并后 `run_all` 39/0/0。


**D-73 核定里程碑（GPT 裁决后的探针取证，更正早前结论）**：最小 RISC-V
探针（`_start(argc, argv, a2)`，SBI console）在本机 U-Boot 2025.10 上
实测：早前「bootelf 无 argv」**证伪**——`bootelf -p <img> [args…]` 把
image 地址之后的参数以 `(extra_argc, extra_argv)` 交入口：无参→argc=0；
`87654321`→argc=1/argv[0]="87654321"；`aaaa bbbb`→argc=2。两个此前
没看见的门：`autostart` 默认未定义时 bootelf 只装载不执行（必须
`setenv autostart yes`）；不带 `-p` 的 extra-arg 形状在本 build 未通过
（`## No elf image at address 0x80200000`），契约钉在 `-p`。a2 是
U-Boot 堆内指针、不是 FDT。hello-boot 的 `Load access fault` 真因修正
为 **staging==LMA 自重叠**（默认 `loadaddr=0x80200000` 既是 fatload
落点又是其 PT_LOAD 目标）；探针 LMA=0x84000000、staging 留在
0x80200000 后两次全链干净执行。布局证据（256M）：lmb reserved =
OpenSBI `0x80000000-0x8005ffff`、`0x8deaa000-0x8deacfff`、U-Boot 区
`0x8dead8d0-0x8fffffff`（relocaddr `0x8f6cf000`、malloc
`0x8eeaf000-0x8f6cf000`、fdt_blob `0x8eead8e0`）；可用带
`0x80060000-0x8dea9fff`；staging：`loadaddr=0x80200000`、boot.scr
执行拷贝 `0x8c100000`。去向（裁决首选路线）：物理地址链接的
`riscv64-uboot-shim.elf`（VMA=LMA=物理入口、自带 bootstrap 栈），入口
ABI=`bootelf -p <img> <MNXBOOT1 表地址>` 的 `(extra_argc, extra_argv)`，
表承载 DTB/kernel/12 模块；**不需要固定 handoff 槽**（仅作板级备用）；
LMA 由 D-73 专属布局配置从 bdinfo/lmb 推导，不硬编码 `0x90000000`；
`BootHandoffV1` 冻结不动；`OpenSbiBootShim` 更名
`UbootBootShim`/`RiscvUbootBootShim` 属下一批实施范围。探针证据：
`/tmp/d73/`（probe.elf、argv-base2/argv-plus/argv-matrix 串口全文）；
01-todo §2.8 已同步更正（保护文件，未代提交）。

**D-73 实施里程碑（U-Boot → boot-shim → kernel-uboot 全链 PASS）**：交付
四个面。① shim 产物：`os/boot-shim/src/uboot_main.rs` + `riscv64-uboot.ld`
（物理链接 VMA=LMA=0x84500000；入口自清 SIE、自清 .bss 含 `.lbss`、自立
64KB bootstrap 栈）+ `build.rs` 特性门挂脚本；MNXBOOT1 表由 shim 在自身
.bss 用 `uboot_layout` 常量自建（kernel 0x86000000/2MB、模块
0x87000000+1MB×12、DTB 0x8c000000），boot.scr 只 fatload 文件——U-Boot
把表与文件分两次落盘时，表条目的 `phys_addr` 与文件实际落点不保证一致
（实测 `phys_addr` 读垃圾而 `len` 正确），表改由消费方自建即消解。
② 接收产物：`kernel-image` 增 `fw-riscv64-uboot` 特性，
`rust_image_main(arena_pa)` → `arch_boot_resume_v1`，与 x86_64 臂同构。
③ 三处实施期修正（各有 QEMU 读数）：`jump_to_kmain` 加 `auipc` 执行位置
探针——接收腿已被跳到高半链接地址、PC 相对 `la` 已解析真值，链接↔装载
偏移不得再加（故障闭合：EPC 0xffffff7f7fe0477a = kmain 链接址
0xffffffc00000477a + delta 0xffffffbf7ffe0000）；shim memmap 的
`firmware_reserve_end` 从 0 改 `KERN_PHYS_BASE=0x80200000`（bootface 同源
语义）——此前 OpenSBI 足迹 `[0x80000000,0x80200000)` 漏进 VM 空闲表、被
选作 PMM 元数据供体，DM(0x80000000) 写入页 fault（stval
0x0000001080000000 = DM 基 64GiB + PA 0x80000000），内核 DM 计划拒绝映射
固件区间属正确行为，错在 memmap 上游；门脚本 Stage 1 `set +x` 曾把构建
失败掩盖成旧二进制续跑，改 `set -e -x`。④ 用户腿取证诊断扩展：
riscv64 用户 trap 面板增 `ura/usp/ua0/ua2`。门读数：`test-riscv64-uboot-shim.sh`
PASS（6/6 路标至 entering scheduler；空闲表修正后 n=2 无固件区间）；
直核腿 `test-riscv64-boot-full.sh` PASS（rc marker，含新 jump 代码）；
docker kernel mock 850/0/8；boot-shim test-all 58/0；三架构 kernel check
0 error；01 篇 §3.7/§4.8 三形态叙事更新（四条链收敛句、riscv64 两行形态
表）；文风门 0 error、承载门通过。01-todo §1 D-73 行改 ✅（保护文件，未
代提交）。后续窗口：xtask 产线接线（`fw-riscv64-uboot` 进 image 装配）、
真板 U-Boot 契约（`FW_PAYLOAD` 直核形态保持可用，另计）。

**C8ALIGN 里程碑（C8/SD-21 入口 sp 十六字节对齐闭合）**：修复覆盖四个交付
点——三帧构造器（`minix_sys::stack`/shell `exec_frame`/init `execve`）总帧
圆整 8→16（指针槽保持 8、帧内偏移与 frame_size 无关、内容逐字节不变），
加探针真机抓获的第四点：`arch/src/arch/boot.rs::load_elf_into` 内核出生帧
`sp=stack_high-56`（8 mod 16，全架构；历史注释自证先前只修到八字节）。
根因=`STACK_MIN_SZ=1400 ≡ 8 (mod 16)`，修复前全部十二个出生进程入口 sp
错位。探针=minix-rt 三 naked 臂首指令 stash + `entry_sp()` + rt-birth 载体
DIAGCTL 行，三 `test-rt-birth*` 门内嵌 16 对齐断言。`[ARCH: exec-sp-16]`
依据=C `stack_utils.c` 八字节圆整的不可观察偏离（参数经 ps_strings 寄存器），
裁决=c-board-rulings-brief.md 两轮外部评审。门读数：minix-sys 342/0
（反例钉 `test_frame_size_sixteen_byte_aligned`）、shell 61/0、init 149/0、
arch runtime-window 287/0（两布局钉随新契约 −64/−40）、docker mock
850/0/8、三架构 rt-birth PASS、riscv ATF 36/36。台账 SD-21（ledger:26、
register:494）改 closed，C 板 C8 行改 ✅。

**C11WX 里程碑（C11/SD-12 第一步 PD-22① 乙案′落地——进程根内核文本 RX）**：
三段实施：① 三份 kernel-image/*.ld `.text` 节末 `PROVIDE(__text_end = .;)`
（readelf 坐实符号 = R E 段末 0x3561a，可执行区 = 镜像起点连续前缀，单符
号充分）；② `vm_handoff::kern_text_pages` 以符号算真值（feature
`kernel-text-rx` 门控 + kernel-image fw-none-image 转发；非 kernel-image
链接的消费者——测试载体/boot-shim——回退"整段当文本"占位，行为逐位不
变；**契约零变更**，v5 字段既有）；③ `map_kernel` 文本段 RX（按交接
data_pages 运行时自判占位/真值）+ 引导根高半分割（`harden_kernel_text`，
调用点 kmain 内 `smp_init` 前：`from_active_root` KernelDm 句柄 +
split_huge/update_flags，2MiB 步进 + 逐 4K RX，窗口条件=AP 释放前从未装
载内核根）。**交付中发现并修复**：首版误用 `adopt_active_root`
（VmDm 通道）——内核态经 VM 窗读根 PTE 即 fault（stval
0x1082000800 = VM_DM + root 0x82000000 + 0x800），改 `from_active_root`
（KernelDm）后全绿；两构造器并存语义自此有判例。承诺条款（精确）：进程
根高半文本 RX、DM 别名 RW/NX、进程根恒等窗仅 MMIO（`map_kernel` 第四段
陈旧注释随批更正为 T2 后口径）、引导根低 4GiB 恒等 RWX 维持现状、不承
诺物理页级 W^X。甲案（建根期 4K 分片）挂起 = 独立技术债 register 新行
`SD-12a`。门读数：riscv ATF 36/36（RX 文本首次上全系统）、test-smp-aps
PASS、x86/aarch64 生产 boot PASS、U-Boot 链 6/6、docker mock 850/0/8、
arch runtime-window 287/0（mock map_kernel 钉随 RX 契约更新）、三架构
check 0 error。台账 SD-12 行与 register:362 标 step① landed，C 板 C11
行改 ✅（第一步）。

**C17A6 里程碑（C17-A6 静默 mock 布局回退消除）**：三处变更——①
`BootParams::simple` 显式携带 mock `KernelLayout`（夹具语义：用简单构造
器 = 明确要 mock 布局；x86 mock 常量逐值保留）；② `VmServer::init` 的
`kernel_layout` 缺失处理从 unwrap_or_else 回退改为**三架构统一拒绝**
（expect，riscv 既有 panic 泛化；旧形状=riscv panic、x86/aarch64 静默
采用 x86 mock 常量）；③ 负控 `test_init_rejects_missing_kernel_layout`
（真实 v<5 形状 None 输入被拒）。**cfg(test) 边界核查**（评审质疑点）：
simple 全调用点在 cfg(test) 门控内（vm_server new/new_for_test + boot.rs
单元测试），零集成测试调用点——cfg(test) 当前充分，但仍按评审偏好收窄
为显式夹具语义。走 init 的既有测试（boot_params 助手）随新契约注入夹
具；纯字段测试的 None 保留（合法被测值）。生产路径不变（真实交接 ≥ v5
恒带真值）。门读数：minix-vm 544/0（+负控）、riscv ATF 36/36。A 板复
核请求已随 C 板 C17 行留痕。A9（来源断言）为余片。

**C17A9 里程碑（C17-A9 user_sp 来源契约断言）**：两条缝各一枚断言——①
夹具缝：`BootParams::simple().user_sp == USER_STACK_TOP`（架构权威常量，
按宿主 target 取值、零字面量）；② 直通缝：构造器把 params.user_sp 原样
落进 `VmContext.user_sp`（来源链不允许转换或字面量替换）。语义按评审裁
决钉死：`ctx.user_sp` = 帧构造前栈顶输入（非入口 sp）；入口 sp 对齐断
言属 C8（test-rt-birth* 门内嵌），两项分立。内核侧源头（build_kernel_
info 的 user_sp）已由 boot-shim 既有测试钉住——来源链闭合：kinfo→交
接页→params→ctx。门读数：minix-vm 546/0（+2）。C 板 C17 行 A6/A9 族
双闭，余=xtask 构建侧（等 A 板）。

**C2ESSWI 里程碑（C2E-riscv 窄钳钥匙消费——运行时条件撤钳）**：`sched_
proc` 的 riscv64 亲和钳制从编译期 cfg 改为**运行时条件**——`platform_
desc().sswi_setip_base().is_none()` 时维持钳制（默认 QEMU 无 SSWI 节
点，行为不变）；平台暴露 ACLINT SSWI 设备（DTB `riscv,aclint-sswi`，如
QEMU `aclint=on`）即撤钳——生产 `send_sched_ipi` Path 1 直写 SETIP 寄存
器递跨核 SSIP（`test-smp-ipi-riscv64` 真机往返、run_all 常驻）。钥匙消
费依据：SSWI 往返已真机证实 + 生产路径就位（本轮核查确认，无需新探
针）。test-smp-aps-riscv64.sh 增 `ACLINT=1` 门控参数（两条 -machine 参
数追加 `,aclint=on`），双形态门：默认（钳制）PASS + ACLINT=1（撤钳）
PASS——全系统载体的撤钳路径首次真机执行。评审限制如实记录：aclint 属
TCG 路径，探通结果不推广到 KVM/真板；SSWI 为可选平台设备。门读数：默
认 PASS、ACLINT=1 PASS、test-smp-ipi PASS、riscv ATF 36/36、docker mock
850/0/8、riscv check 0 error。C 板 C2E 行钥匙消费注记。

**C11WX2 里程碑（C11WX 两缺陷修正——分割启用 + x86 文本边界孤儿节）**：
C11WX 合并笔带入两处缺陷，本批修正并全量重验。① **bisect 注释态泄漏**：
C11WX 的 `harden_kernel_text()` 调用在合并时仍处二分禁用态——riscv ATF
"绿"为分割关闭态，进程根 RX（真值）与引导根分割（`from_active_root`
KernelDm）两条运行时路径当时均未真机验证；本批重启用。② **x86 文本边
界孤儿节**：x86_64.ld 的 `.text` 输入匹配漏掉 `.ap_early_entry` 与
`.ltext.*`（rustc 逐函数可执行节）——孤儿放置把它们插进 R E 段内、
PROVIDE 之后，`__text_end`=0x2c64 vs 真实 R E 段末 0x403ae；
`kern_text_pages=3` → 进程根 [0x203000,0x2403ae) 的代码映为 RW-NX，
VM 首个内核腿取指 `x86_trap_dispatch` → #PF(e=0x11，P|I NX 取指) 无限
复烫 → #DF → #GP（QEMU `-d int` 抓获 RIP=CR2 现场定界）。修复=显式收
纳孤儿节；修复后三架构不变量 **`__text_end == R E 段末`** 全部成立
（x86 0x403ae、aarch64 0x38914、riscv 0x3561a，readelf 对账）——建议
后续进 check-layout。二分读数：占位态 e5-ds PASS（Round1 sp 无罪）、
真值+legacy 根 FAIL、legacy 根+分割 FAIL → 元凶=x86 RX（孤儿子集），
分割无罪。最终态全绿：riscv ATF 36/36（RX 根+分割双开）、smp-aps、
e5-ds 12/12、cmd-smoke 核心 marker（du 腿=TODO-06 既载既有问题）、
U-Boot 链 6/6、mock 850/0/8、arch 287/0。hardened boot 诊断行
（text harden enter/adopted/done）常驻。

**C17XTASK 里程碑（C17-xtask 构建侧闭合——aarch64 镜像随产 dumpdtb）**：
xtask image 装配清单增 `Action::ToolBestEffort`（失败/缺件属契约正常态的
外部工具动作——qemu dumpdtb 不自退故 timeout 15 包裹，宿主无
qemu-system-aarch64 则静默跳过，ESP 无文件即 shim 的缺席路径）+
aarch64 分支动作：`-machine virt,gic-version=3,dumpdtb=staging/dumpdtb
.dtb -cpu max -m 512M`（与门生成参数逐参一致）→ mcopy 入 ESP 根
`/dumpdtb.dtb`（shim loader::DUMPDTB_PATH 契约位）。门
test-shim-bootmarks-aarch64 的 MARK3 改为**装配后按 ESP 实况解析**
（mdir 检测；D1_DUMPDTB=1 保留为手工注入覆盖），修掉首轮"默认等 legacy
行而镜像已打 gated 行"的错配。门读数：默认（无环境变量）
test-shim-bootmarks-aarch64 PASS，串口
`memmap source: uefi-getmemorymap gated by dtb /memory (conv=14
dtb_regions=1)`；xtask check/clean。A 板窗口=用户本轮指令授权，A 板复
核留痕于 C17 行。C17 行余片=0。

### 第二百一十二笔（A 板 · `vis` 的四处程序名位改跟 `argv[0]`，在册 `VIS-USAGE-PROGNAME-HARDCODED` 闭合）

内核板与二线板本轮无在册可领项（C 板末两行分别是真机窗服务队列与 `🚫` 维持登记，B 板各项全部
`✅` 或 `⏸`），按 C→B→A 的次序落到 A 板在册项。`vis` 有四条报文带程序名：usage 行
（`minix3/usr.bin/vis/vis.c:136-139` 的 `"Usage: %s …", getprogname()`）、`-F` 宽度不足的
`errx`（`:91`）与 `-m` 和 `-h` 并用的 `errx`（`:144`）；本壳把 `vis` 写死在三处字符串里，
以别的名字起时只有 libc 的 `unknown option` 那一行跟 `argv[0]`。修法把名字收成一个
`fn usage_bytes(prog: &str)`，`libc_line` 与两个 `errx` 出口共用同一个 `prog`。

参照件 `tmp/vis-472/vis_ref` 经 `vv` 软链现量：四形（`-Z`、`-d`、`-F 4`、`-m -h`）全部打 `vv`。
A/B 十二形（`vv`/`vis` 各四形，加 `-F 40`、`-e x`、默认与 `-n`）退码、标准输出、标准错误
逐字节比较 **SAME=12 / DIFF=0**（`tmp/vis-474/ab-final2.log`）。面门新立
`every_program_name_slot_follows_argv0`（用 `CommandExt::arg0` 直接把 `argv[0]` 置成 `vv`，
并在同一枚里以 `vis` 起一次确认默认名不变），面侧 21 → 22 枚；整包 69 段
**1286 / 0**、容器权威门同读数退 0、三架构 `--bins` 各退 0、clippy 与主树唯一句集合各 85 行
无差；负控三枚（usage、`-F`、`-m/-h` 各写回 `vis`）各咬住 1。判据、读数与两起工具自错
（no_std 的 `format!` 导入第五次；负控还原源码后未重建二进制导致一次假 DIFF）全文见
18 篇批次四百七十四。

**PD22S2 交接（运行中——NOT DONE，run_all 26/13 回归未解）**：五批已合并
（1a 描述层 / 1b map_kernel 设备窗段 / 2a PLIC+per-hart context /
2b UART+SSWI / 2c x86 LAPIC+逐 CPU 校验 / 3 identity 退场+强形态扫描）。
单门全绿：riscv ATF 36/36、smp-aps、test-smp-ipi、x86 e5-ds/cmd-smoke、
mock 857/0/8、arch 290/0、三架构 check 0 error。**但收官 run_all
26 passed / 13 failed**：① aarch64 全家族（hello-boot 实测 panic=
`platform::init_from_kinfo: no platform source parsed successfully`
@ platform global.rs:283——shim 产 platform_sources 解析失败/为空，复
现命令=直接 qemu aarch64 跑 minix.img）；② x86 smp-aps/smp-ipi
（"CPU 2/3 didn't boot"+mask mismatch，boot_ack=1——AP 未到齐，嫌疑
=LAPIC_VA 注入后 ICR 路径在测试内核上下文；或与 LAPIC init 序相关）；
③ e5-ds 超时（单独跑 PASS、run_all 内超时——资源/时序嫌疑）。
**下一步**：先修 aarch64 platform source panic（dumpdtb 程序集是
PD22S2C 加入 xtask 的，嫌疑最大——回看 run_all 前后 minix.img 内容差
异），再查 x86 ICR 注入路径，最后重跑 run_all 到 39/0/0 后才可宣布
PD-22② 完成。简报/裁决/验收表=rewrite-notes/01-stage-kernel/
pd22-step2-brief.md（§15）。中途实证：实测 DTB 纠正 SSWI/UART 地址、
PLIC 2MiB 反证、4K map 引导根卡死（map_huge 路径正常）等已在 edge4
前文与简报留档。

**PD22S2H 进行中（claim 在途，未合并）**：PD22S2G 后 run_all=36/3
（e5-ds 超时、timer-irq-riscv64、+1）。timer 载体实测定位：UART RX
自检在引导根阶段武装（stageC csrs sie 后写 UART IER/MCR/THR 走设备
窗 slot1 VA）→ UART 槽在引导根未映射 → load fault。已落
UART bootstrap=true（物理 2MiB 对齐可装）；**复测新发现**：stageC 后
静默挂起（SEI 触发后无 trap 诊断输出）——SEI 路径上的窗口访问链
（PLIC claim→uart hook 读→complete→console）仍有未映射点或 BKL/序
问题，需在 SEI kernel leg 加逐点窗口访问诊断。claim PD22S2H 在途
（含已提交 UART fix），下会话从该 claim 续。

**PD22S2H-8 二分定界（claim 在途，b633338d3）**：MCR.LOOP=1 写挂死
vCPU（QEMU 8.2.2 16550 设备模型行为；PA/设备窗 VA 均挂）。去 LOOP →
不挂、ticks 正常、carrier 跑到窗口结束。且 IER.ERBFI=1 写后 SEI 触发
→ kernel leg claim 走 0xffffff8040201004（基座 0xffffff8040000000 非
任何已知 arch 窗——非设备窗/DM/identity）→ load fault。另发现 PLIC
enable context stride 代码=0x80、规格应为 0x100（QEMU plic.c context
间隔）——潜在 context 寻址错误。下会话首刀：核对 PLIC 全部 context
stride（enable=0x100、threshold/claim=0x2000）+ hart↔context 映射 +
MCR.LOOP 替代回环方案 → timer 载体 PASS。

**PD22S2H-9 会话终态（预算尽，claim 在途）**：git 历史考古关键发现——
`ef23cbbf0 SD-41/C3 收官` 曾将 stride 误修反转回滚 0x1000（QEMU
CONTEXT_STRIDE 规范值；0x2000 为上段误修），且"脉冲式 LOOP 使能序为
真病根解法"——C11WX2 时代 timer 载体以 stride 0x1000 + 脉冲式 LOOP
PASS。当前回归的根因候选：① 设备窗迁移改变了 UART 访问路径的时序/
上下文，使脉冲式 LOOP 使能序行为变化；② MCR.LOOP=1 写经设备窗 VA 挂
死 vCPU（PA 直访也挂——设备模型层面问题，非映射问题）。下会话续作
方案：A) 回退 UART bootstrap=true（恢复 G 时代 bootstrap=false，
timer 载体回退到引导根 PA 直访=identity 覆盖 ✓——C11WX2 时代行为），
同时保留设备窗 process root 映射（map_kernel seg5）；B) PLIC stride
0x1000/0x80/0x2000 全部按 ef23cbbf0 确认值维持；C) timer 载体
PASS 后合并 PD22S2H → run_all 39/0/0 → 收官记账。claim 树完整
（9 笔提交含全套诊断/二分/边界校验），门读数 arch 290/0、三架构
check 0 error。

**PD22S2H-12 取证闭环（claim 在途，最新 diag 提交）**：去失聪修复
（panic handler + diag_panic 双双先清 MCR.LOOP）后 panic 文本全量可
见：scause 0xd stval=0xffffff8040201004，seictx=0x1（per-hart 派生正
确——2a 实现无误），ua0=0xffffff8040201000（fault 基座）、ua2=0x10。
disasm 形态=f120<<12 + f110*2 + 0x200004（claim_offset 形状但基座非
注入的 0xffffffc020000000）。下会话首刀：沿 ua0 基座来源定位——载体
自建 KernelInfo 的 DM 窗基座（D59POLICY 字段）嫌疑最大；grep
0xffffff8040000000 的产生点（0xffffff8020000000+0x20000000 或
base-0x40000000 形态）。次候选：claim_hardware_irq 内联的
s_mode_context 路径与 plat claim_context 的 plic_va 基座不一致。
修好 → timer 载体 PASS → 合并 PD22S2H → run_all 39/0/0 → 记账。

**PD22S2H-14 收官冲刺读数（已合并主树 3bb370884）**：PD22S2H 全链
合并（SEI 首源链在设备窗下全通，test-timer-irq-riscv64 PASS debug+
release 双验证）。x86 ap-alive 修复落地（X86_WINDOW_LIVE 门：自建根
载体回退 PA）→ test-smp-ap-alive PASS。**run_all = 37/2 → 修复后预
期 38/1**。唯一余红 = test-e5-ds：standalone 现场取证（tmp/evidence/
e5-ds-serial-*.log）= scheduler hand-off 后内核态写页异常 vector 14
err=2（supervisor write），rip=0xffff800000230258——嫌疑=step③ 内核
文本 RX 与某内核写路径交互、或 E 批 identity 退场在 e5-ds 链的后续；
ripage 需 debug 符号归因。下会话首刀 = e5-ds（独立工作面）→
39/0/0 → 收官记账（C 板/register/SD-12/edge4/简报终态）。

**e5-ds 归因（下一会话工作面已明确）**：evidence serial（tmp/evidence/
e5-ds-serial-*.log）+ run_all 实测：scheduler hand-off 后内核态写页异
常（vector 14 err=2 supervisor write），rip=0xffff800000230258 落
x86_trap_dispatch_body 内 `movl %ecx, 0x380(%rax)` = **CpuLocal.
fpu_owner 记账写**（trap_dispatch.rs ~2392 行 FPU trap 路径：`local.
fpu_owner = Some(cur_fpu)`）。写 fault = 目标页不可写——嫌疑排序：
① SMP_STATE（CpuLocal 数组宿主）被链接进只读段（x86_64.ld orphan
段收编——与 ap-alive-era x86 orphan 段历史同族，核验 nm SMP_STATE
地址 vs 段表 R E / R / RW 边界）；② PD22S2E identity 退场后 e5-ds 链
的某根缺内核数据段映射。修复 → run_all 39/0/0 → 收官记账。

**PD-22②③ 收官（2026-11-02，全绿）**：收官 run_all **39 passed / 0
failed / 0 build failures**（三架构，/tmp/runall-pd22h16-final.log）。
PD22S2H-15/16 两笔修复闭合 e5-ds（LAPIC 四条时钟路径 + IO-APIC 运行
期 re-mask 全量窗口化；cr2 取证 0xFEE00380/0xFEC00000 逐一定位）。
SD-12 台账行已闭（CLOSED 2026-11-02 + 完整实施记录）；C 板 C11 行状
态列与裁决列已更新。简报终态=p22-step2-brief.md（§15 验收表全项满
足，含强形态扫描）。PD-22 全部完成。

### 第二百一十三笔（C 板·内核优先 · `VMSTYPE-MAP-FACT`：更正「VMSTYPE_MAP/SYS_NONE 在 Minix3 里没用」这条失实陈述）

接管三板后的第一轮。C 板任务表 32 行里 29 行 `✅`，`C17` 正文已写「余片=0」而状态列未翻，
`C9` 是自标「不领」的真机窗服务队列，`C20` 是 `🚫` 维持登记；内核领地（`os/kernel`、`os/arch`、
`os/plat`、`os/kernel-image`）现量的 11 处 `not wired`/`not implemented` 全部是 WONTFIX 设计决定
或对固件/硬件限制的说明（NMI 采样、earm 的 SBI 语义、aarch64 `init` 与首次 `disable()` 等价），
唯一一条**事实失真**在 `VmSuspendType` 的篇注里。

**现量否证**：C 四个码全有定义（`proc.h:98-101`），`VMSTYPE_MAP` 还有一条消费臂
（`do_vmctl.c:102-104`，只做 `assert(RTS_ISSET(p, RTS_VMREQUEST))`），`default:` 才是
`panic("strange request type")`（`:105-106`）；生产侧只有两值——`VMSTYPE_KERNELCALL`
（`arch/i386/memory.c:444,567,661`）与 `VMSTYPE_DELIVERMSG`（`proc.c:282`）。所以旧注释
「not implemented — they are unused in Minix3」把「无生产者」误写成「无用/不存在」，
并且引用的是 `draft/24-cross-space-runtime.md`（非交付物）。同一句错误在 01 篇 todo 的 D-24
备忘里（「C 本身无此变体」）。

**本轮改的两处**：`os/kernel/src/vm.rs` 的枚举篇注按现量重写（四码 + 两生产者 + MAP 臂 +
default panic 的行号全写死），并说明为什么本树**不需要**写 `Map` 变体与 panic 臂：类型不走线
（`VMCTL_MEMREQ_REPLY` 只读回结果），它是内核自己存的值，不可生产的类型在枚举里不可表达，
而 C 的 `SYS_NONE`=0 这一「无挂起」态由 `p_vm_suspend` 的 `None` 承载；
`rewrite-notes/01-stage-kernel/todo.md` 的 D-24 理由随之改正（结论不变，理由从「无此变体」
改成「有定义、有消费臂、无生产者」）。

**门**：宿主内核 `cargo test -p minix-kernel --features mock` → **857 通过 / 0 失败 / 8 忽略**
（`tmp/vmstype-fact/mock.log`）；三架构 `cargo check --target … -p minix-kernel --bins` 各退 0
（`arch.log`）；本轮 diff 全是注释行（`git diff | grep -vE '^\+\s*//'` 零命中），格式漂移按
「改前/改后 `Diff in` 计数相等」量：`vm.rs` 基线 29 处、改后 29 处，**零新增**——全文的存量
漂移属他线，按批次四百零四那条规矩不带走；`doc-style-lint --diff` error 级 0；在制自检干净。
`VMSTYPE` 相关的行为门本就存在（`memreq_reply_delivermsg_requires_mf_delivermsg` 对位 C 的
`assert`），本轮无新增门、无代码路径变化。

### 第二百一十四笔（A 板 · `NL-SIGNED-COUNTERS`：`nl` 的 `-i`/`-v` 交回 C 的 `int` 域，负号计数器与三种格式的符号归位）

C 的两个计数器都是 `static int`（`minix3/usr.bin/nl/nl.c:125`、`:176-183`、`:208-216`），赋值走
`(int)val` 且**无符号检查**；本层此前存成 `u64`，壳侧对负值回一句自创的
`signed numbering counters (-i/-v) not wired`。改的三层：`src/nl.rs` 的 `NlOptions.startnum`/`incr`
改 `i32`、`write_number` 按 `%-*d`/`%*d`/`%0*d` 三张串各就符号归位（`%0*d` 把零填在符号之后，
`-7` 在宽度 5 下是 `-0007`）、取数走 `i64` 以容纳 `i32::MIN`，`counter += incr` 改
`wrapping_add`（C 在该处是未定义行为，参照件实测回绕）；`src/bin/nl.rs` 两臂的判据换成
「`strtol` 语义 + `int` 域」，越界与尾串取同一句报文。

**一处判据来源的更正值得留档**：参照件 `tmp/nl-417/nl_ref` 链的是宿主 glibc，`long` 是 64 位，
`-i 2147483648` 在它那儿被接受并截成 `-2147483648`——这一形的权威是 C 源加 Minix i386 的 32 位
`long`，不是参照件，A/B 里那两形 DIFF 因此是**预期分形**而非回归。其余十三形 SAME
（`tmp/nl-475/ab.log`）。另量出一形登记不执行：`NL-WIDTH-TAIL-TRUNCATION`——C 打的是
`intbuffer + max(0, consumed - width)`（`nl.c:331-335`），数字宽过字段时从左截尾，参照件现量
`-w 1 -v 100` → `0/1/2`、`-w 2 -v -100` → `00/99/98`，本壳无这层截尾。

**门**：库内 +2 枚、面门 +2 枚；宿主整包 69 段 **1286 → 1290 / 0**，**容器权威门同读数退 0**
（`tmp/nl-475/container-gate.log`），三架构 `--bins` 各退 0，`clippy` 与主树唯一句集合各 85 行、
位置集合无差，`rustfmt` 触及三件零新增漂移（`src/nl.rs` 基线 5 处存量、改后仍 5）。负控四枚
各咬住（K1 面门 3、K2 库 2 + 面 1、K3 库 1、K4 面 1），还原后复跑面门 18/0。两处自错入 18 篇
批次四百七十五：负控驱动第一版把失败名去 `... FAILED` 行上找（实际在 `failures:` 块里）⇒ 四枚
全报 `red=0`；`-n ln` 那例我先把期望写成 `incr: -1` 的第二行，靠参照件读数改正期望而不是改代码。

### 第二百一十五笔（A 板 · `NL-WIDTH-TAIL-TRUNCATION`：`nl` 的数字宽过字段时按 C 从左截尾，三张格式串同律）

上一笔登记不执行的那一形，本轮闭合。C 交出的是 `intbuffer + max(0, consumed - width)`
（`nl.c:331-335`），缓冲按 `max(INT_STRLEN_MAXIMUM, width) + 1` 分配（`:261`），`snprintf` 从不
截断，故削掉的是左边的格子；参照件现量 `-w 1 -v 100` → `0/1`、`-w 1 -v -100` → `0/9`（符号是
第一格）、`-n ln -w 3 -v 12345` 与 `-n rz -w 3 -v 12345` → `345/346`、`-w 4 -v -100000` →
`0000/9999`。改法在 `src/nl.rs:fn write_number` 里把字段先建进局部串，末尾按 `width` 交回尾部
若干格。**左截不是右对齐的产物**这一条由 K3 负控（只对右对齐生效即红两枚）钉住。

门：库内 +1 枚、面门 +1 枚；整包 69 段 **1290 → 1292 / 0**、容器权威门同读数退 0、三架构各退 0、
clippy 与主树同集合（85 句）、rustfmt 两件零新增。A/B 十七形 **SAME=16 / DIFF=1**，唯一那形是
上一笔已写明「参照件管不了」的 `-i 2147483648`（宿主 glibc 64 位 `long`）；上一笔的两形截尾
DIFF 本轮转 SAME。负控三枚各咬 2（库 + 面），还原后库 634/0、面门 19/0。

### 第二百一十六笔（A 板 · `TR-BACKSLASH-OCTAL`：`tr` 的 `\` 交回完整的 `backslash()`——三位八进制折叠 + 命名转义表 + 游标）

C 的 `backslash()`（`minix3/usr.bin/tr/str.c:398-452`）一个函数三个出口：至多三位数字按
`val*8+digit` 折叠（判据是 `isdigit`，故 `8`/`9` 照收：`\9`=9、`\18`=16、`\999`=657），没有数字时
走命名转义表（`a b e f n r t v`，其余原样），`\` 落在串尾交出反斜杠；值最终进 `unsigned char`
表，故按模 256 保低字节（`\999`→145、`\777`→255）。本壳此前只认 `\n\t\r`，把数字段当普通成员，
`\102` 交回 `1` 并留下 `0`、`2`。新增 `read_escape(bytes, at) -> (u8, usize)` 承担整条腿，
`escape_literal` 退化成「无数字」那一支的表并补全八枚，三个调用点（`[=x=]` 成员腿、`[x*n]` 成员腿、
裸成员腿）换用它并**接回消费到的位置**——第四位数字回到普通文本正是靠这一步。

参照件现量关键几形：`tr 'abc' '[X*2]y'` → `XXy`（计数真产出两枚成员）、`tr '[=\102=]' x` →
`AxC`（等价类成员是 B）、`\0666` → 两个成员（三位数字 + 第四位 `6` 的字节值恰也是 54）。
A/B 两遍共二十九形全 SAME；第一版把参照件按 `tr_ref` 直接起，未知类名那形的句子差在程序名段，
属夹具错。门：库内 +1、面门 +1，整包 69 段 **1294/0**、容器权威门同读数退 0（第一遍容器目录被
docker 以 root 建、写不进去，重建为用户属主后过）、三架构各退 0、clippy 同集合、rustfmt 零新增。
负控四枚各咬 2（上限 2 位 / 只认八进制数字 / 饱和代替取模 / 表退回三枚），四枚指向四条不同判据。
两处期望写错（`\0666` 单成员、`[=\102=]` 交 `xBC`）都按参照件读数改正，未动实现。

### 第二百一十七笔（A 板 · `JOIN-STDIN-LEG-BYPASSES-DUAL-SURFACE`：登记语按 C 真源改正，顺带修掉 `join` 在宿主读不到标准输入）

批次四百六十三条登记这条读缝时写着「绕开双生面」，隐含应当报读失手。本轮先读 C 真源：
`join.c` 的 `ferror` 只查标准输出（`:457,488,498,508`），输入侧零检查 ⇒ 正确形态是走共件那条缝
但**刻意弃号**，注释按此重写，不再借 `rev`/`head` 一族「读失手要报句子」的口径。

真实缺陷是量出来的：绕开共件的那圈 `minix_sys::read` 在宿主构建里读不到标准输入，
`join - file` 交出空。同名副本对跑二十形：六形真差全在带 `-` 的读腿上，其余十四形逐字节同
（`tmp/join-478/matrix2.log`；第一版矩阵 26 形「差」全是我用 `join-before`/`join-after`
两个不同文件名起造成的程序名段差——**夹具的名字也要同形**）。

门：面门 +1 枚 `a_dash_operand_reads_the_piped_standard_input`（文件-文件、`- 文件`、`文件 -`
三形同答）；负控用改前真码——退回基线提交后该门红在「标准输入作第一个操作数」（空 vs
`b 2 20`/`c 3 30`），换回补丁 join_face 33/0。整包 69 段 **1295/0**、容器权威门同读数退 0、
三架构各退 0、clippy 同集合、rustfmt 两件零新增、文风 `--diff` 零 error。

### 第二百一十八笔（A 板 · `TOUCH-R-REFERENCE`：`touch -r` 接上，登记语「缺基础设施」被本文件自己的 `stat` 用法否证）

C 的 `stime_file()`（`minix3/usr.bin/touch/touch.c:318-327`）只做一次 `stat` 再交出
`st_atimespec`/`st_mtimespec`；本壳头注把 `-r` 与 `-d`/`-h`/长选项一并记成「需要本树没有的基础设施」，
而 `stat` 恰是它给每个操作数用的调用 ⇒ 本轮接上这一条并改写头注。三处对齐 C：参照档在选项环里就
`stat`（失手排在任何操作数被创建之前，走 `err(1, "%s", fname)` 三段形 + 退 1，程序名取 `argv[0]`）、
值可附着可后随、**后写者胜**（`-t` 与 `-r` 写同一枚 `ts[2]`，故 `-t … -r ref x` 用参照时间、
`-r ref -t … x` 用解析时间）。

后写者胜负抽成纯函数 `minix_fileops::touch::chosen_times`，库内测钉六形（含「声称 `-r` 后写却无参照值」
退回当下时间而不 panic）。**没有面门**是实测出来的边界：本 crate 的文件腿在宿主全是替身
（`touch -r ref target` 交回 `touch: ref: Input/output error`——句子形状对、号来自替身），时间写入要可写根盘，
与该壳头注既有的「写类命令整体挂 `E5`」同一条线，不是本轮新造的推迟。

门：`cargo test -p minix-fileops` → 41 段 **456/0**；三架构 `--bins` 各退 0；`rustfmt` 对
`src/touch.rs` 基线 3 处、改后 2 处且 `git diff` 零他人行（纯增量，rustfmt 顺手改的 3 行别人注释已还原）；
文风 `--diff` 零 error；在制自检干净。

### 第二百一十九笔（A 板 · `PR-NODIAG-LEG`：`pr -r` 接上——C 的 `nodiag` 只压那句开档诊断，`errcnt` 与退码照旧）

`pr` 的未建模字母那一支把 `-p`/`-r`/`-T` 一并回 `pr: option form not modelled yet`。C 的
`case 'r'` 只是 `++nodiag`（`minix3/usr.bin/pr/pr.c:1775-1777`），而 `nodiag` 全树一处使用
（`:1364-1369`）：开档失败先 `++errcnt`，再 `if (nodiag) continue;`，才轮到
`pr: Cannot open %s, %s`——压的是句子，不是失败。落码：`PrOptions.nodiag` + 解析臂 + 壳侧把
`support::warn` 包进 `if !options.nodiag`，`had_error = true` 留在外面，未建模一支缩到 `-p`/`-T`。

门两枚：库内 `dash_r_is_a_flag_and_the_unmodelled_arm_stays_narrow`（`-r`、`-tr`、`-rt`、`-r -t`
四种给法、只给 `-t` 时不亮、`-p`/`-T` 照旧 `Unmodelled`——钉住搬走 `r` 没放宽那一支）；
面门 `dash_r_silences_the_open_diagnostic_but_not_the_failure`（`-r 缺档` 空 stdout/空 stderr/退 1、
`-t -r 缺档 -` 仍打 `a\n` 且退 1、两枚缺档仍静默仍退 1、去掉 `-r` 逗号形报文一字不差回来）。
负控三枚各咬（吞掉失败 1 红 / 照印 3 红 / 塞回未建模 2 红）。

读数：宿主整包 69 段 **1297/0**、容器权威门同读数退 0、三架构各退 0；本轮新写代码起初有 3 处
自有格式漂移，就地格式化后归零（基线 0，未带走存量）。07 篇 `pr` 行「三形未接线」改成两形并按 C
写明 `nodiag` 的分量。

合回这一笔走了一段弯路，记下来当作板面的规矩：两笔提交落完后，`git merge --ff-only` 因为另一条线
已把 `rewrite` 往前推了一格而退 128，同一命令行里紧跟的 `claim.sh release` 照例删工作树、销锁分支，
于是这两笔当场变成没有引用的游离提交。找回的路子是 `git fsck --lost-found` 列出的悬空提交里按
提交信息认出那一枚，`git log --oneline <那枚>` 顺着父链把前一笔也带出来（两笔都还在，对象没被回收），
再 `git merge --no-ff <尾笔>` 合回——本板并发推进是常态，快退能过的假设只在该线尚未动过时成立，
下一笔起把合回写成「先试快退，退不动就 `--no-ff`，两形都在合回前把状态核对做完」。

### 第二百二十笔（A 板 · `SED-READ-SEAM`：`sed`/`grep` 的输入腿补上第二个载体——宿主此前读到空）

`os/commands/usr-bin/regex/src/bin_support.rs` 的 `read_stdin` 与两支壳各自自带的 `read_file`
只有系统调用这一个载体，而 `minix_sys::{open, read}` 在宿主是替身。动手前先量：
`printf 'abc\ndef\n' | sed 's/a/A/'` 交出**空输出**且退 0（宿主自带 sed 同命令交 `Abc`、`def`），
命名操作数与 `grep a <真实档>` 同样读不到——与批次四百七十八的 `join`、批次四百七十前的 `nl`/`fold`/
`rev`/`colrm`/`split`/`wtf` 同一族缺陷的第七次落地。

落码是载体不是语义：两条腿各配一对 `cfg` 双生面（宿主 `std::io::stdin` / `std::fs::File::open`，
内核仍走 `minix_sys::{open, read, close}`），两份逐字相同的本地 `read_file` 合并进 bin_support，
两支壳只改调用点并删掉随之失效的 `use minix_sys::{close, open, read}`。失败形态一笔未动：仍只有
「打不开」回 `Err`，读中断仍交回已收到的字节。

本 crate 没有进程级门面，这一笔新建两件共十枚：`tests/sed_face.rs` 六枚（标准输入走进模式空间、
命名操作数与管道逐字节同形、多操作数按序、`-n` 之后只剩 `s///p` 那句 `OUT()`、开不了的档 warn +
继续 + 退 1、一枚名叫 `-` 的文件不是标准输入别名），`tests/grep_face.rs` 四枚（只测同一条缝：
命名操作数、两形同形、`grep.c:493-494` 的名头规则、退码 2 只体现失败不吞输出）。C 侧锚点逐条回查过：
`main.c:214` 与 `:361-363`（无操作数即那枚 `NULL`，按标准输入读）、`process.c:270`（自动打印）、
`process.c:466-467`（`s` 的 `p` 标志自己印）、`main.c:449-452`（warn/`rval = 1`/继续）、
`grep.c:505`（退码式）。

读数：宿主整包 6 段 **65/0**，容器权威门同读数退 0（`tmp/sed-read-seam/container.log`），三架构
`--bins` 各退 0。负控两枚各咬：K1（宿主 `read_stdin` 不再读真标准输入）红 4，K2（宿主 `read_file`
一律打不开）红 7，还原复跑 65/0。**本轮两次自错**：①首轮负控没带 `--no-fail-fast`，两件门面同跑时
第一件红掉就收场，读数被误当成「只咬住一枚」；②`rustfmt` 顺手改了两支壳的 import 排序与一处折行，
逐条还原后残留漂移与基线同段（`sed.rs` 基线 3 现存 1、`grep.rs` 基线 4 现存 2，本轮新写三件零漂移）。

**两条登记不执行**：`SED-READ-FAIL-FATAL`（读中断 C 是 `err(1, "%s", fname)` 当场致命
`main.c:463-465`，本壳当读到尽头）、`OPEN-WARN-SHAPE`（`warn` 经 `minix3/lib/libc/gen/vwarnc.c`
打三段 `<程序名>: <文本>: <strerror>`，两支壳打两段且程序名写死未取 `argv[0]`，与批次四百七十二
的 `vis` 那一笔同族）。08 篇 `grep`/`sed` 两行按实况补上载体与门面归属。


### 第二百二十一笔（A 板 · `SED-SCRIPT-CHAIN`：`sed` 的执行半接上——脚本是一条条编译出来的，不是在 `;` 上切开的一堆片段）

库半的 `parse_script` 早就是按分隔符拆段的，**却没有一个生产者**：壳自己写的是「不以 `s` 开头就拒、
`s` 之后还有尾随字节就回 `chained commands are not modelled`」。这既是欠接线也是形状错——C 的
`compile_stream` 一次编译一条命令，命令按自己的定界符数到哪里结束（`compile.c:480-490`），所以
`s/a;b/x/` 里那个 `;` 是图案文本。解析环照 `compile.c:177-184` 重写：先吃空白，再吞多余分隔符，
`#` 丢掉**本行**其余（那个 `continue` 是回读下一行），命令读完吃空格与制表符后必须撞上分隔符、行尾
或脚本尾。

打印规则四条形体各回 C 的一处臂，抽成纯函数 `step` 与 `auto_print`：`d` 立 `pd` 并跳出命令环
（`process.c:142-144`）、裸 `p` 印模式空间（`:191-194`）、`=` 印行号且**既不查 `-n` 也不查 `pd`**
（`:264-265`）、`s` 的 `p` 标志只在真替换过才印且不看 `-n`（`:466-467`），周期尾才是那句
`new: if (!nflag && !pd) OUT()`（`:269-270`）。抽纯的必要性跟上：宿主那侧读通道是替身，把条件留在
壳里就只能起进程才看得见。模式空间随之改成 `&[u8]` 承载（`apply` 的输入换了类型）——链上要把上一条
的输出接着喂给下一条，用 `&str` 就得在两条命令之间重解码，一次截在半字符上的替换会把整行丢成空串。

门：库内 +7 枚、`tests/sed_face.rs` 由 6 枚扩到 14 枚。差分腿十三形与宿主自带 `sed` 逐字节同形且
退码相同（链、`s///p` 双印、`-n` 形、`d`、`p`、`=`、`s/a/A/;d`、`p;d`、`-n p`、分号在图案内、
`p;#x;d`、`#n` 换行、`p` 换行 `d`）；`sed` 参照件本树没建，C 侧判据全回 `compile.c`/`process.c`/
`main.c` 行号。读数：宿主整包 6 段 **80/0**、容器权威门同读数退 0、三架构各退 0；负控三枚各咬
（忘 `!pd` 红 3 / `p` 标志不看替换 红 4 / 回到先切 `;` 红 2），还原复跑 80/0。

**两处自错按真源改正**：① 以为 `"p\n#x;d"` 是两条命令——C 的 `continue` 回读下一行，那个 `d` 早跟着
注释丢了；② 照搬旧代码把两条拒绝句打到标准输出，而 C 的 `errx` 经 `verrx.c:57` 写标准错误，本轮搬过去
改后的读数是 `sed 1d` 把报文交回标准错误、标准输出零字节、退 1。**三笔登记不执行**：
`SED-FLAG-SPACE-EXTENSION`（C 的 `compile_flags` 在循环顶就 `EATSPACE()`，`compile.c:576` 自标
EXTENSION，故 `s/a/b/ p` 在那侧设上 `p` 标志）、`SED-USAGE-STREAM-AND-TEXT`（缺脚本那句仍在 stdout，
C 的 `usage()` 写 stderr 且是 `main.c:223-229` 那两行、程序名取 `getprogname()`）、`SED-SCRIPT-CAP-16`
（一支脚本上限 `MAX_SCRIPT_COMMANDS` 条形体，C 边编译边挂链表无上限）。08 篇 sed 行与「尚未覆盖」那句
按实况改口径。

### 第二百二十二笔（A 板 · `SED-EMPTY-RE-REUSE`：`s//x/` 是「用上一支表达式」，本壳却当「一支空表达式」拒了）

C 把跑过的表达式记在 `defpreg`，图案为空那条 `s` 不编译任何东西、直接取记下的那支；一支都没记过时
`errx(1, "first RE may not be empty")` 当场停（`minix3/usr.bin/sed/process.c:668-671`）。本壳两形都不对：
`s/a/A/;s//x/` 回 `sed: bad s command` 退 1（宿主自带 sed 同脚本交 `Ax`），`s//B/` 也回同一句而不是
C 那句。

修法不是放开空图案：本引擎对空图案本就显式拒绝（`pattern.rs:fn compile` 见空即 `InvalidPattern`），
照 `sed` 的需求放开会把 `s//x/` 变成「每处都匹配」的改写，而 `grep ''` 那侧的空图案口径是另一件事。
于是把规则做成选型——库内 `pattern_choice` 交回 `Own`/`Reuse(第几支)`/`NoPrevious` 三臂；壳侧把表达式
放进一张 `patterns` 表、命令只带表里的位号，两条命令因此能指向同一支而不复制表达式。记时时机本壳在
编译期、C 在执行期，本子集里两者挑到同一支（无地址无分支，列表每条每行都跑到），有地址时会分叉，
而本壳那形本就直接拒绝——这一段写在 `pattern_choice` 的注里，免得下人来改时看不出为什么可以不管。

门：库内 1 枚（同一枚空图案在三种现场分别给 `Reuse(0)`/`Reuse(1)`/`NoPrevious`，且自带非空图案永远
`Own`），面门 +4 枚共 18 枚——`aa` 交 `Ax`；`s/a/A/;s/b/B/;s//C/` 在 `ab b` 上交 `AB C`（误取第一支会
留在 `AB b`）；`s/\(a\)/X\1/;s//Y\1/` 在 `a` 上交 `XYa`（回填的 `\1` 来自新一次匹配）；`s//B/` 空 stdout、
退 1、stderr 逐字节 `sed: first RE may not be empty\n`（C 那句不带行号与档名，是本壳能整句对齐的一句）。
读数：宿主整包 6 段 **85/0**、容器权威门同读数退 0、三架构各退 0；负控三枚各咬（恒选自带 红 5 /
把复用臂改回硬停 红 3 / 不再记录 红 3），还原复跑 85/0。自错一处：面门把 `Vec<u8>` 与 `&str` 直接
比较，编译期挡住——本 crate 的面门 helper 交回字节，文本过滤那族交回字符串。

**两笔登记不执行**：`SED-BACKREF-NOT-DEFINED`（替换串里 `\N` 超出所用表达式的组数，C 编译期查自带图案
`compile.c:524-529`、执行期查复用的那支 `process.c:386-390`，本壳两形都展开成空——`sed 's/a/X\1/'` 在
`a` 上交 `X` 退 0，C 退 1；`Pattern::group_count` 已公开，缺的是解析期记下替换串里最大的 `\N`）、
`BRE-PAREN-LITERAL`（C 的基本式里未转义 `(` 是普通字符，本引擎把 `(a)` 判非法：`sed 's/(a)/X/'` 与
`grep -G '(a)'` 都非零退，宿主 sed 原样交 `a`；这条住在 `pattern.rs`，改它要连 grep 那一族的口径一起过）。
08 篇 sed 行与「尚未覆盖」那句按实况补。

### 第二百二十三笔（A 板 · `SED-ERRX-SHAPE`：`sed` 的失手报文交回 C 的那一句三段形）

C 的 `compile.c` 六处 `errx` 都以 `"%lu: %s: "` 起头——脚本行号与 `fname`——`errx` 自己再加
`<程序名>: `（`minix3/lib/libc/gen/verrx.c:57`）。本壳把这些形态不同的失手并成一句自造的
`sed: bad s command`，读者分不出是没闭合的定界符、认不得的标志、还是编号溢出。

库半的 `ScriptError` 随之按 C 的臂拆：`InvalidCode(字节)`（`:320`，`%c` 是原字节）、
`ExtraChars(动词)`（`:264-265`，C 打的是 `cmd->code`，不是尾巴字节）、`Subst(SubstError)` 四臂
（`BadDelimiter :325`／`Unterminated :331`／`BadFlag :636`／`NumberOverflow :601-607`），
另留两臂没有 C 句可抄（命令数上限、C 有而本壳未带的动词与地址）。`parse_script` 交回
`ParseFailure { error, line }`，`line` 跟着 `cu_fgets` 的 `linenum` 数（`main.c:274`、`:292`、
`:306`、`:315`）。名槽按 C 的算术切：`string_ident[30]`（`main.c:242`），塞不进 29 字节就
`strcpy(+24, " ...\"")`（`main.c:261-266`）→ 前 24 字节接 ` ..."`；切点落在多字节中间时本壳退回
字符边界，这是唯一一处取舍。

顺带闭掉在册的 `SED-FLAG-SPACE-EXTENSION`：`compile_flags` 的 `EATSPACE()` 在循环**顶**
（`compile.c:576` 源码自标 EXTENSION），所以 `s/a/b/ g`、`s/a/b/ p` 在那侧是设上标志；把空白当作标志串的终点就会把尾巴字母判成拒绝，所以标志串在空白处
继续走。改后宿主自带 sed 与本壳交回同一结果（旁证）。

门：库内改写两枚 + 新立三枚（臂的归属、行号、标志空白、`0` 落 BadFlag、`2147483648` 落溢出而
`2147483647` 仍是计数、反斜杠定界符、没闭合的定界符），面门 18 → 22 枚逐字节钉 C 的整行。
读数：宿主整包 6 段 **90/0**、容器权威门同读数退 0、三架构各退 0；负控四枚各咬红 2/3/4/3。

**两处自错 + 一次假红**：① 面门把 `s/a/b/ p` 写成期望 `bbb`，实测两侧都交 `bab`——那条脚本没有
`g`，只换第一个匹配，写期望时把上一枚门的「全局」搬了过来；② `script_ident` 用了
`str::to_string()`，宿主测全绿而三架构三目标各报 `no method named to_string`（这一族第六次，只有
freestanding 看得见 `alloc` 缺什么），改 `String::from` 后归零；③ 容器门第一次是 `rc=101
sections=0`，起因是取证的 `ci-target` 被上一笔 docker 以 root 建出来、宿主 `mkdir -p` 对着已存在
的 root 目录什么都不做 ⇒ 判据是 `failed to create directory /tgt/debug`，属假红而非门失败，
`rmdir` 后以本用户重建即可。

仍在册：`SED-BACKREF-NOT-DEFINED`（要先有解析期的「替换串里最大 `\N`」）、`compile_re` 失手那句
C 打 `RE error: <strerror>`（`compile.c:467`）需要正则库的 regerror 文本、以及本壳两句自造拒绝
（地址/未带动词、命令数上限）。08 篇 sed 行的失手报文口径按实况改写。

### 第二百二十四笔（A 板 · `SED-BACKREF-NOT-DEFINED`：`\N` 越界的两臂两时机，顺带把「复用哪一支」搬回执行期）

替换串里 `\N` 要得比表达式能给的更多时，C 说 `\N not defined in the RE`，且分两处：**自带**表达式时
在读替换串的过程中逐个数字比过去，报那一个数字（`compile.c:523-528` 的 `%c`，`s/\(a\)/\2\3/` 报 `\2`
而不是 `\3`）；**图案为空**时 `s->re` 是 NULL，编译遍无从可查，C 等到执行时拿记录的那支比，报
`s->maxbref`（`process.c:384-391` 的 `%u`）。本壳两臂都没有：`\1` 落空就展开成空串，
`sed 's/a/X\1/'` 在 `a` 上交回 `X` 退 0。两臂的数字都落在 `1`-`9`（C 的数字集是
`strchr("123456789", *p)`），所以只差来源，不是两种格式。

扫描规则照 C 的步法：反斜杠紧跟 `1`-`9` 才算一次引用，且这一对一起吞掉，于是 `\\1` 是「反斜杠加字面
`1`」不算引用；`\&`、`\\` 各吞两字节但不计数；`&` 是整段匹配、从来不是引用。

**上一笔埋的时机错在本笔搬正**：上轮把「空图案落到哪一支」放进编译遍，理由是「本子集里每条命令每行都
跑到，编译序与执行序挑到同一支」——那话对*挑到哪一支*成立，对*错误会不会发生*不成立：走不到的命令
根本不检查。`d;s//x/` 在 C 安静退 0（`d` 结束周期，那条 `s` 一次都没跑），按编译遍定时会当场报
`first RE may not be empty`。现在编译遍只备齐各命令自带的表达式，执行遍每遇到一条 `s` 才用
`pattern_choice(own, recorded)` 决定用哪一支并顺手记下用过的（C 记的时机也是执行时，且在看匹配结果
之前，`process.c:671-672`）⇒ `defpreg` 跨行、跨操作存活，第一档一行没匹配上也算"用过一次"。

门：库内新立 `test_backref_scan_follows_the_c_escape_rules`，`pattern_choice` 那枚按新签名改写
（`own` 有值恒走 `Own`）；面门 22 → 26 枚，四枚新门的报文逐字节钉住 C 的三段形。**差分腿的边界写清**：
`zz`/`ab` 跨操作数那一形与自带越界那一形宿主 sed 同形；**复用臂的组数检查 GNU 根本没有**（它把
认不出的 `\2` 静默展开成空），所以那枚门的判据只有 C 源，门里也这样注明——不把没有差当成一致。

读数：宿主整包 6 段 **95/0**、容器权威门同读数退 0、三架构各退 0、rustfmt 三件零漂移；负控四枚各咬
（拿掉编译期那臂 红 2 / 拿掉执行期那臂 红 2 / 不再记录用过的表达式 红 6 / 报文里写死数字 红 2）。

**闭合**：`SED-BACKREF-NOT-DEFINED`。**新登记不执行**：`SED-NEWLINE-IN-SUBST`（C 对替换串里未转义的
换行报 `unescaped newline inside substitute pattern`，`compile.c:542-544`；本壳把换行当普通文本读进
图案或替换）。仍在册：`RE error: <strerror>`（`compile.c:467`，需正则库 regerror 文本）、
`SED-SCRIPT-CAP-16`、`OPEN-WARN-SHAPE`、`SED-READ-FAIL-FATAL`、`BRE-PAREN-LITERAL`。

### 第二百二十五笔（A 板 · `SED-NEWLINE-IN-SUBST`：一行没写完就是没写完——未转义换行有它自己那一句）

C 每次只交给编译器一行脚本（`cu_fgets` 交回带行尾换行的缓冲区，`main.c:303-315`），所以字段还在读
就撞上换行，说明这条命令没在自己那一行闭合，报 `unescaped newline inside substitute pattern`
（`compile.c:542-544`）。本壳把整份脚本当一条连续文本扫，字段会一路读过换行去找定界符：
`s/a<换行>b/x` 要么被当成一条图案里带换行的命令收下，要么报成 `unterminated substitute pattern`。

**优先次序是这一笔的内容**：同一个现场 C 先撞上换行再谈别的，故这句**盖过**
`unterminated substitute pattern`——`s/a/b<换行>c/` 里闭合的 `/` 出现在下一行，C 读到换行就报了，
不会等读完缓冲区。落码只是 `parse_subst` 定界扫描里加一问（跳过转义对之后、认定界符之前），
新臂 `SubstError::UnescapedNewline` 配上 C 的原句；名槽里的换行原样打出，与 C 把 `fname` 指向整份
脚本串一致。

门：库内 `test_subst_error_arms` 扩到九形（两形报换行、文本读完的仍报未闭合、跨行的两条正常命令仍读成
两条、反斜杠续行那一形仍收下），面门 26 → 27 枚逐字节比对两句报文并带一枚正形对照。差分腿注明：
宿主自带 sed 对这两形报自己的 `unterminated 's' command`（同退 1、同不动输出，句子不同），判据取 C 源。

读数：宿主整包 6 段 **96/0**、容器权威门同读数退 0、三架构各退 0、rustfmt 三件零漂移；负控两枚各咬红 3
（拿掉检查 / 只换句子不换条件——后者专钉优先次序），还原复跑 96/0。上一笔容器门那道假红本轮一开始就把
取证目录以本用户建出来，绕开了。

**闭合** `SED-NEWLINE-IN-SUBST`；**新登记不执行** `SED-ESCAPED-NEWLINE-CONTINUATION`：反斜杠紧跟换行时
C 丢掉那个换行、把下一段文本接进同一字段继续读（`compile.c:507-519` 的 `sawesc` 配 `more` 重入），
本壳把 `\` 与换行两字节一起留在字段文本里——两边都认"命令跨过行尾继续"，差在字段是否带着那个换行。

### 第二百二十六笔（A 板 · `BRE-PAREN-LITERAL`：基本式的括号是文本，扩展式落单的 `)` 也是——因为 Minix 编译正则库带着 `-DPOSIX_MISTAKE`）

现场：`grep -G '(a)'` 与 `sed 's/(a)/X/'` 都回 `bad pattern`／`bad s command` 退非零。C 那边基本式的
原子表（`minix3/lib/libc/regex/regcomp.c:628-690`）根本没有 `)` 这一 `case`，全落
`default: ordinary(...)`；分组体也不在裸 `)` 上结束——`p_bre` 的终止条件是**成对**的 `\` `)`
（`:570-597` 的 `SEETWO(end1, end2)`，调用点 `:656` 传 `'\\'` 与 `')'`），故 `\(a)b\)` 的体是 `a)b`。

扩展式那一侧还有一层构建口径：`:428-438` 本给落单 `)` 设了 `SETERROR(REG_EPAREN)`，但那一段包在
`#ifndef POSIX_MISTAKE` 里，而 `minix3/lib/libc/regex/Makefile.inc:7` 写着
`CPPFLAGS+=-DPOSIX_MISTAKE` ⇒ 被编译掉，组外的 `)` 也是文本。**只读 `regcomp.c` 不看 `Makefile.inc`
会得出相反结论**——正则的口径由源码与编译开关共同定义，这一条进 08 篇正文。

落码是一行判停：`)` 只在「扩展式且正在组体内」才结束原子读取；`|` 仍在扩展式两深度都断（`p_ere` 的
串联循环在任何深度都在 `|` 断开，`:361`）。基本式的组收尾本就走 `parse_escape` 的 `\)` 路径。

**随笔修掉一族跨件断**：`minix-editor` 在本树里编不过——`exec.rs:2004` 还按批次四百八十二之前的形状
把 `&str` 交给 `sed::apply`，而那一笔把模式空间换成字节承载（`&[u8]`），当时只跑了 `minix-regex` 的门。
现场 `cargo test -p minix-editor -p minix-fileops` 报 `expected &[u8], found &str`；调用点交回
`line_text.as_bytes()`。**规则补一条（本线自约束）**：动任一 crate 的公开签名，收尾门必须带上依赖件
（`grep -rln minix-regex os --include=Cargo.toml` ⇒ `minix-editor`、`minix-fileops` 两处在消费），
只跑本包门就把断线留在树里——批次四百八十二那道门当时是绿的，说明"本包全绿"不构成跨件证据。

门：引擎侧新立两枚（基本式括号成文本并把 `|`、`+`、`{}` 三族一起钉住，每形两条断言；扩展式组与
POSIX_MISTAKE 那形），面门 `grep_face.rs` +2、`sed_face.rs` +1；差分腿六形与宿主 GNU 逐字节同
（GNU 正则族与 BSD 同源，作旁证，判据取 `regcomp.c`）。读数：`minix-regex` 6 段 **101/0**，依赖件
`minix-editor` + `minix-fileops` 44 段 **522/0**，容器权威门三包合跑 **50 段 623/0** 退 0，三架构各
退 0；`rustfmt` 对 `pattern.rs` 基线 9 处、改后仍 9 处且位置同一（我的插入只顶了后面的行号），两件
面门本轮新代码格式化归零。负控三枚各咬：P1（裸 `)` 两式都判停）红 6、P2（基本式 `(` 开组）红 4、
P3（基本式 `|` 判停）红 2。

**闭合** `BRE-PAREN-LITERAL`；**新登记不执行** `RE-INTERVAL-UNIMPLEMENTED`：`grep -G 'r\{2\}'` 与
`grep -E 'r{2}'` 都该匹配 `rr`（宿主 GNU 交回 `rr`），本引擎把两者当字面交出 `r{2}`——要动引擎的
重复算子层，另立一笔。

### 第二百二十七笔（A 板 · `OPEN-WARN-SHAPE`：开档警告补齐 C 的三段，程序名交回 `argv[0]`；区间量词勘查后让位）

C 两处开档失败都是 `warn("%s", name)`（`minix3/usr.bin/sed/main.c:450`、
`minix3/minix/usr.bin/grep/util.c:111`），而 `warn` 走 `vwarnc`，形制
`<getprogname()>: <文本>: <strerror 句子>\n`（`minix3/lib/libc/gen/vwarnc.c:3-8`）。本壳只印两段，
且名字是源码里的字面量——换名或走符号链接调用时 C 的行首跟着 `argv[0]` 变，本壳不变。

落码三件：`bin_support::prog_name`（`getprogname()` 即 `argv[0]` 最后一段）、`read_file` 的失败侧从
`()` 改成携带 `minix_sys::Errno`（第三段是**号**，把失败并成一个单位类型就没号可印；宿主侧从
`raw_os_error()` 取，取不到号的按 `EIO`，对应 C 里 `warn` 读的是刚失败那次调用留下的 `errno`）、
`warn_name(prog, name, code)` 拼三段。两支壳的**所有**诊断都改走这个名字（`grep` 三处、`sed` 六处，
含批次四百八十四起的 `errx` 三段形一族——`verrx.c:59` 打的第一段同样是 `getprogname()`，不能一半
随 `argv[0]`、一半写死）。

门：四枚新立（两支壳各两枚：三段形逐字节 + `arg0` 改名后行首跟着改，`sed` 那枚还顺带把编译诊断的
名头一起验），两枚旧门把"只断操作数被点名"的注释指回新门。`arg0` 按 `vis_face.rs:373` 的先例把
`CommandExt` 收在函数内。差分腿：宿主 `grep` 与改后逐字节同；`sed` 那侧 GNU 用自己的句子
（`can't read ...`），判据取 C 源。读数：`minix-regex` **105/0**、依赖件 `minix-editor` +
`minix-fileops` 44 段 **522/0**（上一笔立的规矩当场执行：本轮动的是 crate 内消费的 `read_file` 失败
类型，仍照规矩跑一遍依赖件）、容器权威门同读数退 0、三架构各退 0、`rustfmt` 三件基线 0、改后 0。
负控三枚各咬：只印前两段 红 2、名字槽写回硬编码 红 8（一次把 `sed` 全族 C 形报文都咬出来，反证这些
句子同源）、宿主侧把号并成 `EIO` 红 2。

**闭合** `OPEN-WARN-SHAPE`；同源未闭的一条写进台账：这一族报文的字节载体仍是 lossy `&str`
（`bin_support.rs:fn args` 在目标侧 `from_utf8_lossy`），属 `ARGV-BYTE-PATH-IN-LIBS` 那一族，本笔不
顺手改。

**另一件事：区间量词勘查后让位**（本轮曾领 `RE-INTERVAL-UNIMPLEMENTED`，勘查后释放，未产生提交）。
原因是它不是一处判停能了事的：`{m,n}` 在 C 里由 `p_simp_re`/`p_ere_exp` 在原子之后处理
（`regcomp.c:473`、`:486`、`:512-536`、`:705-725`，发 `OBRACE`/`ORANGE` 让引擎计数循环），本引擎
`Inst` 只有 `Byte/Class/Any/Split/Jump/Save/AssertStart/AssertEnd/Match`，没有计数态。可行的替代是
**展开**：`x{m,n}` → m 份必选 + (n−m) 份嵌套可选（`quest` 的 Split 先取体后跳过，贪心次序与 C 一致）。
单字节/字符类/`.` 的原子直接可展开；但 `(ab){2}` 这类**组上的区间**要先有子图复制（C 的 `dupl`，
`regcomp.c` 里给 `\(...\)` 与 `+` 用），而且复制时各组必须**复用同一批 Save 槽位**才能保持 POSIX 的
"捕获留最后一次迭代"——这要先动 `Compiler` 的槽位分配（现在是 `pattern.group_count` 单调递增）。
另有一处次序细节：C 里 `a{2}*` 是 `REG_BADRPT`（区间之后不再有可重复的原子），本引擎会把 `*` 当
前导运算符取字面。勘查结论：拆成两笔才诚实——①单原子区间展开 + `{` 后非数字仍字面（`regcomp.c:473`）
+ 越界计数报错；②组上的区间（需要槽位可控的复制设施）。已在册不动。

### 第二百二十八笔（A 板 · `RE-SINGLE-ATOM-INTERVAL`：区间量词用展开接上，组上的那一半留在册）

C 把 `{m,n}` 交给 `repeat()` 由引擎计数（`minix3/lib/libc/regex/regcomp.c:512-529` 扩展式、
`:705-721` 基本式，反界 `REQUIRE(count <= count2, REG_BADBR)`，界数由 `p_count` 读）。本引擎指令里
没有计数态，两式都把 `{` 当普通字节：现场是 `grep -E 'a{2}b'` 与 `grep -G 'a\{2\}b'` 都找不到 `aab`，
宿主 grep 交回 `aab`。

形状照 C 抄、实现换成展开：`m` 份必选，其后每多一份配一个 `quest` 包起来的可选份，开上界末尾挂
`star`，`{0}` 留一个空跳转（与空分支同形）。贪心次序不变，`quest`/`star` 都先取体后跳过。`{` 后不跟
数字不开区间（`:473`），基本式收尾是 `\}`，双重量词按 `:531-537` 的 `REG_BADRPT` 一并拒——做法是任何
一次量词用掉后就清空"可重复位"。**组上的区间显式拒绝而非退化**：`Compiler` 的捕获槽位单调编号，
重复子表达式要复制代码段并让各份共用同批槽位（C 的 `dupl` + `repeat`，POSIX 还要"捕获留最后一次
迭代"），没有这套设施时把 `(ab){2}` 读成字面 `ab{2}` 是静默错配，比当场拒坏得多。

门：引擎侧三枚新立（四形原子 × 四种界数的两式拼写；反界/缺收尾/`{x}`/`{`/`{2}x` 的形状错；组区间与
三条双重量词），面门 `grep_face.rs` +2、`sed_face.rs` +1，七形跨两式逐字节与宿主同。差分腿的边界照旧
写清：反界两边都拒但句子不同（GNU 是 `Invalid content of {}`），组上的区间是唯一 GNU 支持而本壳拒的
形状。另钉一处现行通道差异并按现状登记 `GREP-DIAGNOSTIC-STREAM`（`bad pattern or option` 今天打在
标准输出，C 的每条诊断都走 `errx`/`fprintf(stderr, …)`，`grep.c:114`/`:484`），不在本笔顺手搬。

读数：`minix-regex` **111/0**、依赖件 `minix-editor` + `minix-fileops` 44 段 **522/0**（改引擎公开
行为就全跑消费方）、容器权威门同读数退 0、三架构各退 0；负控四枚各咬红 6/3/2/5。
**过程记一笔**：本轮先让 `rustfmt` 全文跑过，它顺手把 `pattern.rs` 基线里 9 处别人行的既有漂移一起改了
——以 HEAD 为底反向还原那 9 块（`git apply -R --unidiff-zero`）才把本轮改动收回到 244 行纯新增、
零删除；第一次还原用 `--recount` 不认零上下文块，改 `--unidiff-zero` 才过。教训：改完好意格式化之前先
数一遍基线漂移，还原文本身要用 `git diff --stat` 的删除数为零来验，不能只信 `rustfmt --check` 归零
（那一度是 0，因为还原出来的文件当时根本不过解析——编译门当场戳穿）。

**在册变化**：`RE-INTERVAL-UNIMPLEMENTED` 拆两半——单指令原子那半闭合，组上的那半以
`RE-GROUP-INTERVAL-DUP` 继续在册；新立 `GREP-DIAGNOSTIC-STREAM`。08 篇的 `grep`/`sed` 两行与
「尚未覆盖」那句按此改写。

### 第二百二十九笔（A 板 · `GREP-DIAGNOSTIC-STREAM`：诊断一律走标准错误，标准输出只放数据）

`grep`/`sed` 两支壳还有三句诊断打在标准输出上。C 侧一句都没有：`grep` 的 `usage()` 是
`fprintf(stderr, …)`（`grep.c:111-118`）、图案不合法是 `errx(2, "%s", re_error)`（`:484`）；`sed` 的
`usage()` 同形（`main.c:223-226`）；`verrx.c:57`/`vwarnc.c:3` 的前缀段也都在标准错误。现场
`grep -E 'a{2,1}b' 文件` 把 `grep: bad pattern or option` 吐在结果流里——下游管道分不清一句诊断与一条
命中行，而 `grep` 的标准输出本就是它的答案。

落码：`bin/grep.rs` 三处、`bin/sed.rs` 两处改走 `support::warn`，顺带把 sed usage 的名字位从字面量
换成 `argv[0]`（C 那两行按 `getprogname()` 填 `%s`）。**usage 的文本仍只写本壳接线的 `-n`**：C 那串
`[-aElnru] … [-e command] [-f command_file] [-I[extension]] [-i[extension]]` 里多数旗标本壳没有，
照抄等于让一句拒绝提示去兜售不存在的能力——这一条是设计取舍，不是待修。

门：面门两枚新立（`every_diagnostic_reaches_standard_error_and_leaves_standard_output_clean` 三形各钉
stderr 原文 + stdout 零字节 + 退 2；`the_usage_and_option_refusals_reach_standard_error` 用
`arg0("sd")` 钉 `usage: sd [-n] script [file ...]`），并把批次四百八十九两枚按"现行通道"钉在标准
输出上的期望同时翻正。读数：`minix-regex` **113/0**、依赖件 44 段 **522/0**、容器权威门退 0、
三架构各退 0、rustfmt 四件基线 0 改后 0；负控 D1/D2 各咬一枚（各红一处）。

**闭合** `GREP-DIAGNOSTIC-STREAM`；`SED-USAGE-STREAM-AND-TEXT` 按实际拆两半——通道那半随本笔闭，
文本那半转为设计取舍（不再列作待修）。仍在册的旧账：`bad pattern`/`bad s command` 是自造句，C 在那
两处打正则库的 `re_error` 文本（`grep.c:484`、`compile.c:467`），接它先得有 regerror 那套文本。

### 第二百三十笔（A 板 · `SED-READ-FAIL-FATAL`：读失败按发生的时机分两形，代价一路从 warn 到退场）

本轮照 C→B→A 的顺序先勘两板：C 板（内核）`01-stage-kernel/01-todo.md` 已无开放标记，`D-59`/`D-60`
的代码与文档两处措辞分别落在 `006f4f556` 与 `2026-10-29` 的 01TODO-B1，coordination 总账里仍 `open`
的两行——中断臂（`riscv64` 软/外中断、`aarch64` 生成中断、用户同步异常到信号的执行腿）要真机窗与
S-8，文档重生成的文件名引用那行归文档线——都不是单轮可闭；B 板（库与驱动框架）任务表里 `☐` 行为零。
遂回 A 板命令面领 `SED-READ-FAIL-FATAL`。

C 在 `mf_fgets` 里给两件事定了两种代价：`fopen` 失败是 `warn` + `rval = 1` + `continue`
（`main.c:449-452`），已经打开之后的 `getline` 失败是 `err(1, "%s", fname)`（`:463-465`）——印完就退，
后面的操作数一个也不再打开；标准输入走同一条读环，名字位由 C 设成 `stdin`（`:348`）。本壳的输入缝只
有一形（`read_file` 交 `Result<Vec<u8>, Errno>`：打开失败交 `Err`、读失败吞掉当读到尽头），于是本该
终结进程的那一句被降级成警告。现场 `sed 's/a/A/' <目录> <正常文件>`：本壳跨过目录、印出第二个操作数
的 `Abc`、退 1；C 印 `sed: <目录>: Is a directory` 就停。

落码：`bin_support.rs` 的 `Operand` 三形（`Data`/`OpenFailed`/`ReadFailed`）+ `read_stdin` 交回
`(bytes, Option<Errno>)` + 新增 `err_name`——它与 `warn_name` 共用同一个三段格式器再退 1，因为 C 的
`err` 走的正是 `verr` → `verrc` → `vwarnc`（`minix3/lib/libc/gen/verr.c`），末尾多一个 `exit(eval)`。
`sed` 两腿按 C 各自决定停不停，读失败前已到的字节先走完周期再报（C 的行循环在那之前逐行跑过）。
**`grep` 一臂不动**：`grep_fgetln` 对读失败与读到尽头一律给 `NULL`，`procfile` 只跳出本次扫描
（`minix3/minix/usr.bin/grep/util.c:132`）——两支壳共用一对腿而代价不同，这件事从今写在缝上。

门：`sed_face.rs` 新立 `a_failed_read_stops_the_run_rather_than_warning_and_going_on`（stdout 零字节、
stderr 逐字节三段、退 1，钉"后面的操作数不许被打开"）与 `a_failed_standard_input_read_names_stdin`
（名字位必须是 `stdin`）；`grep_face.rs` 新立 `a_failed_read_ends_the_scan_without_a_complaint` 守另一
臂，免得日后有人把 `sed` 的致命性顺手搬过去。读数：`minix-regex` **116/0**（基线 113/0，+3 枚）、
依赖件 44 段 **522/0**、容器权威门 50 段 **638/0** 退 0、三架构 `--bins` 各 0 错（新添的 `read_stream`
只有这条门真编译）、rustfmt 五件 0 漂移、clippy 与 HEAD 基线同三条零新增。负控 N1/N2 各咬一枚，还原
后 `sed_face` + `grep_face` 46/0。差分腿：宿主 GNU `sed` 在同一形上也**停**（退 4、句子是它自己的
`read error on <名字>: …`，第二个操作数同样不印），stdin 也报 `stdin`——停不停两家一致，退 1 与
三段形按 Minix C。

**自错**：三架构门第一次用了 `x86_64-unknown-none-elf`/`aarch64-unknown-none-elf` 两个不存在的三元组，
`rustc` 报 "could not find specification for target"，而命令尾接了 `head`，管道退出码仍是 0——一次
**假绿**，靠本文台账里的现算配方改回 `…-none` / `riscv64gc-unknown-none-elf` 才算真跑；三架构腿的判定
要看 `^error` 计数，不能看管道尾的 `$?`。另：目录载体第一次用 `create_dir(...).expect(...)`，同测试
二跑必炸，改 `create_dir_all`。

**闭合** `SED-READ-FAIL-FATAL`。核实所得：`grep` 的读失败臂按 C 就是"读到尽头"，不是缺口，本笔起它有
了自己的面门。新登记 `SED-READFAIL-PARTIAL-CARRIER`（P2，测试载体而非代码缺陷）：宿主造不出"先有字节、
后失败"的载体（目录第一次读即 `EISDIR`），"失败前的行仍先印出来"这一半只有实现与注释承载，端到端门只
钉住"停"。08 篇的 `sed` 行改正读中断的口径并补上两臂分工。

### 第二百三十一笔（A 板 · `SED-SCRIPT-CAP-16`：命令链随脚本生长，长度不设上限）

C 的 `compile_stream` 一边读一边把编译好的命令挂上链（`cmd->next` 走法，
`minix3/usr.bin/sed/compile.c`），从头到尾不问会有多少条。本壳的 `parse_script` 却往调用者给的十六格
固定数组里写（`MAX_SCRIPT_COMMANDS`），第十七条只能回一个自造的 `NotModelled`，壳再把它译成
`sed: address or verb not modelled`——一句讲地址与动词的话，指的却是长度。现场：二十条链式替换在宿主
`sed` 上逐字节改出 `ABCDEFGHIJKLMNOPQRST`、退 0，本壳退 1、不吐数据、报的还是地址那句。

落码：`parse_script` 交回 `Vec<Scripted>`（库新增 `extern crate alloc`，只此一处按调用者文本生长），
`MAX_SCRIPT_COMMANDS` 删除，`NotModelled` 的文档收回到它真正的两义（地址前缀、C 有而本壳没接的
动词）；壳侧删掉十六格数组与切片，直接走返回的列表；库头设计句改写为"引擎自身仍是定长数组，`alloc`
只出现在随调用者文本生长的结果上"。

门：库测 `test_a_long_script_is_not_capped`（五十条 `p`，逐条核类型与行号）、面门
`a_script_longer_than_sixteen_commands_runs`（二十条替换链逐字节对 `A…T`）。读数：宿主 50 段 **640/0**
（基线 638/0）、容器权威门同读数退 0、三架构 `--bins` 各 0 错（库新增 `alloc` 后 `no_std` 侧要真编）、
`rustfmt` 的 diff 块数与 HEAD 基线相同（13/2，均系他处既有），本笔引入的一处超宽 `use` 已按 rustfmt
拆回；`clippy` 零新增。负控 N1 把十六格上限放回 `push` 之前，两枚新门同时红，还原后库 55/0、
面门 35/0。差分腿：二十命令脚本宿主 `sed` 同输出同退码，四十条 `p` 两边都是四十一行。

**自错**：批量改测试时按 `grep -A` 的截断窗口估了六处旧 sink，实际九处——一次删净后靠编译错误逐处补
回绑定，教训是这类计数以编译器的报错为准，别以窗口截断的 grep 为准。

**闭合** `SED-SCRIPT-CAP-16`。新登记 `SED-SUBST-TEXT-CAP`（P2）：`MAX_SUBST_TEXT = 256` 是另一处自造
上限——C 的 `compile_subst` 从 `2 * _POSIX2_LINE_MAX + 1` 起、之后按 `xrealloc` 倍增
（`compile.c:540-552`），不设二百五十六字节的界；本笔不顺手改，留作单独一笔。

### 第二百三十二笔（A 板 · `SED-ESCAPED-NEWLINE-CONTINUATION`：`s` 的两半按 C 的两支读者读）

本轮照 C→B→A 复勘：C 板 `01-stage-kernel/01-todo.md` 的开放标记仍为 0，B 板任务表 `☐` 行仍为 0，遂回
A 板收 `sed` 家族最后一条在册的行为差异。

C 把 `s` 的两半交给两支函数读，句子与续行规则都不同。图案半 `compile_delimited` 一次只读一行、不能
续行（`minix3/usr.bin/sed/compile.c:305-317`），行尾反斜杠与原始换行都让它带着 NULL 退场，调用方印
`unterminated substitute pattern`（`:330-331`）。替换半 `compile_subst`：原始换行是它独有的
`unescaped newline inside substitute pattern`（`:542-544`）；反斜杠收尾则**续行**——反斜杠与换行一起
从字段里消失，下一行头一个字符顶替它的转义位置（`:511-521` 的 `sawesc`）；没闭合时是第三句
`unterminated substitute in regular expression`（`:555-556`）。本壳把两半当同一种读者：图案里的反斜杠
换行被接受、替换里的被原样留在字段里，两种「没闭合」共用图案那句。

落码：`parse_subst` 按两半分开走；图案半三种形状一律 `Unterminated`；替换半在反斜杠收尾时把已读部分
搬进 `String` 续拼，转义位上的字符按拼接器的读法落笔（`&`、`\`、`1`-`9` 前补反斜杠，其余原样——正是
C 写进缓冲的那份文本，`:519-530`），之后每个字符继续追加。字段类型从 `&str` 变为 `Cow<'a, str>`
（无续行仍是切片），`Subst`/`Command`/`Scripted` 去 `Copy` 留 `Clone`；`bin/sed.rs` 编译遍改为按引用
匹配再 clone，`editor/src/exec.rs` 用 `Cow::Borrowed`。新增 `SubstError::UnterminatedReplacement`；
`parse_subst` 收 `start_line`、交回带行号的 `SubstFailure`（续行过的命令让行号跟着走），`parse_script`
每条命令后按消耗字节补数换行。

门：库测 `test_replacement_continuation_stitches_like_c`（`bc`/`\&`/`\1`/`.`/空续行五形）与
`test_a_continued_command_moves_the_line_counter`；面门 `a_continued_replacement_joins_the_next_line`、
`a_pattern_continued_across_lines_is_unterminated`、`an_unterminated_replacement_has_its_own_sentence`；
三枚旧面门的期望按 C 改正。读数：宿主 50 段 **645/0**（基线 640/0）、容器权威门同读数退 0、三架构
`--bins` 各 0 错、`rustfmt` 块数与 HEAD 基线相同（`sed.rs` 2、`exec.rs` 68）、`clippy` 零新增（本笔自造
的两条已改掉）。负控 N1（不拼接）、N2（图案接受续行）各咬库测与面门各一枚，还原后 57/0 与 38/0。

**差分与真值**：宿主 GNU `sed` 在这两形上与 C 相反——它把反斜杠换行当字面换行（替换形输出 `b` 换行
`cbc`，图案形不匹配）。真值优先序里 C 在 GNU 之前，按 C 的拼接语义落地；分歧记在
`tmp/sed-escaped-newline/differential.log`，免得后续拿 GNU 当基准翻回去。

**自错**：续行第一版只追加转义位一个字符，行内后续字节没继续追加（空续行得到 `\n` 而非 `\nc`，库测
抓住）；重写标志环时漏掉 `p` 臂（三条既有测试一起红，按 HEAD 原文补回）；续行面门期望写成 `bbc`，
实测是 `bcbc`。

**闭合** `SED-ESCAPED-NEWLINE-CONTINUATION`。新登记 `SED-NONASCII-DELIMITER`（P2，现行崩溃）：
非 ASCII 定界符使字段切片落在字符中间，`sed 'séaébé'` 实测 panic 退 134，GNU 用自己的句子拒绝、C 按
字节照跑，修法需先裁决字段文本是否收字节。另收紧批次四百九十二 `SED-SUBST-TEXT-CAP` 的修法：壳的
`apply` 输出缓冲定长、写满静默截断，去 `MAX_SUBST_TEXT` 之前得先让输出跟着长。

### 第二百三十三笔（01 阶段 · I-29 续：DTB 保留来源落码 + 双轴回归评审）

先落码、再评审、再按事实回写文档。两笔合入 `1e11a39e2`（代码 `38668f843`）：

- 采集器：`/reserved-memory` 静态 `reg`（含 `no-map`，单元数取父节点）与文件头保留映射块接入；声明按页向外取整、按 `/memory` 全景裁剪，越出 RAM 的部分不产生主张；固件前缀段不重复登记。容量、回绕、声明重叠（含 `no-map`×普通混合）、不可读声明形状、不可达保留节点各自具名拒绝。
- 缺陷两条：8 格静默截断改为错误；引导区重叠断言改 `checked_add` 并补两条 `should_panic`。
- 撤销一条缺陷登记：核对 `uefi 0.33` 源码后，`exit_boot_services` 失败时 reset、不返回，路径 A 封装没有可丢弃的状态；原定的「24 个调用点补失败分支」批次取消。这是第二次「未读源码先登记」，处理方式写进档案错误登记。
- 门：`cargo test -p minix-boot --lib` 75/0；`cargo test -p boot-shim --lib` 66/0；x86_64/aarch64（`fw-none-image`）/ riscv64 交叉 check 通过；`os/qemu-tests/test-kernel-image-riscv64-bootface.sh` PASS（串口 memmap 行不变）。`doc-style-lint --diff` 零命中；主简报 1745 行（上限 1750）。
- 文档：简报 §4.1/§4.3/R4b-c1 按落码修正（R4b-c1 撤回「声明必须整段在 /memory 内」，改为按交集生效）；缺陷表第 1/4 条标已修、第 5 条撤销；档案补第二十六轮之后落码块与第二十一至二十六轮缺块补记；材料册新增落码证据表。
- 边界：QEMU/OpenSBI 的运行时 DTB 未在前缀之上暴露保留声明，该次 QEMU 只证「接入不改变既有引导」，扣除行为由单元测试覆盖，真实板卡样本待取。

### 第二百三十四笔（01 阶段 · I-29 第二十七轮合同收尾）

文档提交 `cdf6ba20a`（基线 `e96613504`），不扩集合、只闭合边界；主文档 1745 → 1739 行。

- §0.3 三输入域与表头对齐（原分隔行只有两列），`DeviceMappingRequirements` 降为映射义务、只引用 `AuthorityId`。
- AArch64 锁定 4 KiB granule：代码锚 `TCR_EL1` TG0/TG1=0、`PhysFrame::SIZE=0x1000`；16K/64K 不在支持集合。
- `NormalizeByType` → `NormalizationRole` 六行表；`RamLikeSourceRanges` → `MemoryTypeSourceRanges`；补 R2 的包含前提。
- R13-A 主键改 `AccessUseId`，`(tree,phase,path)`/`(caller,phase)` 降为属性；调用点完备性 = 受控边界 + CI 静态审计 + 负控（实现先决）。代码读数：raw 端口 asm 仅 arch/plat/boot-shim 五个文件，kernel/servers 零命中。
- 普通主存语义准入覆盖所有 `NormalMainMemory` 映射范围（含 `ReservedMappedRanges`）。
- PERSISTENT 归 `DiagnosticMemberRanges`（代码：type 14 落 side list）；诊断范围表（完整、超限拒绝启动）与操作者样本（可截断）拆成两个合同。
- 计数重算 `4+5+2+11+1+2+7+7=39`，24 组 `G01–G24` 对照；三处「请评审人回答」收敛进 §12.4。
- 门：`doc-style-lint --diff` error 0；表格/围栏自检 0 新增（主文档从 1 处既有列错位降到 0）；档案与材料册补本轮块与代码读数。

### 第二百三十五笔（01 阶段 · I-29 第二十八轮：集合边界、验收目标、事实状态）

文档提交 `154aed914`（基线 `cdf6ba20a`）；主文档 1745 → 1669 行（本轮上限 1700）。

- 非 UEFI `SourceDomain` 只由 `/memory` 构造；`DeviceTreeReservedRanges` = 声明向外 ∩ `SourceDomain`；越出部分只进禁止域。
- `ReservedMapped` 两步派生 + `DenyOnly`；同一页既要求映射又落在 `DenyOnly` 时构造期拒绝。
- R21 目标补 Startup/Device 两类义务（S3/S4 各自最终根）；审计点在 T2 之前，S4 只确认同一份根。
- R18 补 `RequiredPhysicalRanges` 定义（启动 ∪ 设备；Port I/O 不参与）。
- R13-A 白名单到函数级 + 拒绝白名单文件内新增裸操作 + 调用参数与 `AccessUseId` 一致。
- T1 启用前审计完成点；S1 逐项访问清单 + per-hart 转换状态。
- 输入表：`ReservedPrefixEnd` 为表外政策标量；保留声明来源身份独立。
- 类型 12 改按项目支持边界定性（注记不使类型成为架构专属）；样本证明仍是门槛。
- §4.4.1b 的「全仓零命中」作废；补四行事实表与解析/转换/接入三层状态。
- 关系组：评审人指出的 G22/G23 重复计入 R20 经核对**不成立**（原文为 `G22 R18+R19a+R19b+R19c`、`G23 R20+R21`），仍补唯一性说明。
- 门：`doc-style-lint --diff` error 0；表格/围栏自检 0 新增；档案与材料册同步。

### 第二百三十六笔（01 阶段 · I-29 第二十九轮：主文档改 as-built，旧文冻结）

文档提交 `a15fcccac`（主树 `17c26eb06`）。主文档 1669 → 137 行。

- 新结构：五条不变量 + as-built（采集器/分类器/单根与附加 DM/交接/三条腿/尚不存在的机制）+ 已裁定未实现合同 + O-01～O-13 + 证据索引 + 签字状态。
- 旧文冻结为 `04-design-backlog-round28.md`（只增不改）；主文档重写的输入是代码+测试+ADR，不是旧文。
- 第二十九轮九项意见三分：全部成立，但都是未设计集合的公式/时序，不是代码缺陷；统一登记为 O 表。无「不成立」项。
- 代码事实：今天只有一棵项目根；Step 4 的 DM 是追加（`establish_boot_dm`），不是第二棵树——两棵树相关合同没有代码消费者。
- 过程事故（已修）：ff-only 合并时主树已被 `17c26eb06` 推进，认领释放把分支删掉，提交成为孤立对象；重新认领后 cherry-pick `0ce9eeb64` → `a15fcccac` 合入。教训：合并前先取主树 HEAD，合并失败不得先释放认领。
- 门：`doc-style-lint --diff` error 0；主文档表格/围栏自检 0；档案与材料册补本轮块与 as-built 读数。

### 第二百三十七笔（01 阶段 · I-29 主文档第二次改写：自包含简报）

评审人指出 137 行的 as-built 备忘对看不到仓库的读者不可用。同一纪律下改写为主文档 364 行：

- §1 自包含背景（三条腿、四种来源、四个正交维度、两棵树的设计意图与今天的单根事实、三架构差异）；
- §2 as-built（每条带测试名，新增「不保证什么」）；§3 五条不变量；§4 已裁定未实现；
- §5 十三条未决问题，每条含失败形态、候选 A/B（标注未裁定）、判据、第一个消费者；
- §6 证据；§7 签字状态。

纪律不变：候选不是合同；第二层不签的理由仍是 O-01～O-12 没有代码。137 行备忘被替换，未另存。
门：`doc-style-lint --diff` error 0；表格/围栏自检 0。

### 第二百三十八笔（01 阶段 · I-29 第三十轮：语义修订，不写代码）

主文档 364 → 384 行，按评审人要求只做定点语义修订：

- O-01 改生命周期（ACPI_RECLAIM/BootServices/Loader 都不是永久保留；v1 永不回收是项目策略）。
- O-02 拆 O-02a 映射授权完整性 / O-02b 普通保留不产生映射禁令；O-03 改禁止项来源分级。
- O-05 采用候选 A：审计时点表 + 冻结字段表（含 A/D 硬件更新的处置）。
- O-06 采用候选 A：逐类型三问；O-07 补访问完整性来源；O-11 改首次访问授权的循环依赖。
- O-08/O-09/O-10 已裁定（A），保留编号待实现；O-12 拆成 12a～12e。
- 启动入口改四条（补 aarch64 直启）；新增 §2.7 代码位置表；§6 补测试≠输入完整性。
- 门：`doc-style-lint --diff` error 0；表格/围栏 0；主文档 384 行。

### 第二百三十九笔（01 阶段 · I-29 第三十一轮：dtb 形状校验 + 语义修订）

代码 `2209fb3e9`、文档 `77b0e3c7b`。

- 代码：`/reserved-memory` 父节点必须声明空 `ranges`（缺失/非空 → `ReservedMemoryRangesNotEmpty`）；子节点 `no-map` 与 `reusable` 互斥（`ConflictingReservationAttributes`）。夹具补 `ranges`/`reusable`，+2 测试。
- 门：minix-boot 77 通过、boot-shim 66 通过；三目标交叉 check；riscv bootface QEMU PASS（OpenSBI DTB 的 `ranges` 为空）。
- 文档：启动入口矩阵四行；aarch64 UEFI 磁盘 DTB 改为「只作自检闸，不是内存政策输入」；O-01 三层叠加、O-02a 页粒度与授权字段、O-02b 只产生 PMM 排除、O-03/O-04 优先级与五种禁止语义、O-05 先审后用、O-06 类型表补全、O-07 限定证明范围、O-10 第二枚负控、O-11 两步授权与一致性策略、O-12f `RamSourceDomain`；§1.5 补 x86 PAT×MTRR 与 RISC-V PMA 来源。
- 主文档 384 → 401 行；`doc-style-lint --diff` error 0；表格/围栏 0。

### 第二百四十笔（01 阶段 · I-29 第三十二轮：解析器校验、自检定性、来源域分层）

代码 `431bf8461`、文档 `46cbef986`。

- 解析器：`no-map`/`reusable` 必须为空属性；`/reserved-memory` cells 必须与根一致。
- 回归修复：aarch64-unknown-uefi 两处 `collect()` 未处理 `Result`（上一轮漏检该目标）。
- 行为：UEFI 磁盘 DTB 的 `/memory` 覆盖对照由 panic 降为诊断，不再参与启动决策；QEMU MARK3 同步。
- 文档：`RamSourceDomain` 与 `RamBacking` 分层（`EfiReservedMemoryType` 可属来源域非 backing）；O-01 逐类型保留期限与 `reusable` 所有者；O-02a x86 有效类型与别名；O-06 backing 不证明硬件可访问、`LOADER_CODE` 可执行；O-11 header 边界 + MRB 交叉验证；O-12g 采集器重构登记。
- 门：minix-boot 79、boot-shim 66、三 UEFI/riscv 目标编译、aarch64 UEFI QEMU PASS；lint 0；主文档 437 行。

### 第二百四十一笔（01 阶段 · I-29 第三十三轮：MRB 完整性 + O 表收敛）

代码 `413ff9265`；主文档 437 → 471 行。

- MRB：条目互不重叠、扫描上限 16 条（缺终止项也拒绝）；+2 测试（81）。
- 文档：MRB 三项项目限制（status/hotpluggable/reusable）；O-11 四条 DTB 入口 + 两阶段 + 重叠类型裁决；O-06 加 RamSourceDomain 列与属性位处置；O-12c 设备窗口来源矩阵；O-12g 原始范围保留；O-12h 结构校验与分类分开；§6 证据基线。
- 门：minix-boot 81、boot-shim 66、三目标编译、riscv bootface QEMU PASS；lint 0。

### 第二百四十二笔（01 阶段 · I-29 第三十四轮：节点状态准入 + O-12 重构）

代码 `f0c518179`；主文档 471 → 490 行。

- `/memory` 的 `status`（非 okay 拒绝）与 `hotpluggable`（拒绝）成为启动准入条件；+2 测试（83）。
- O-06：`RESERVED` 域外（B），`UNUSABLE` 在来源域但进 PMM 排除；补 `EFI_MEMORY_HOT_PLUGGABLE`（依赖库无该位 → fail-closed）。
- O-12：改为子合同列表，O-12c 拆三概念并列静态表清点与冲突规则；O-12h 加内存图元数据层与 VirtualStart；O-12g 补每级 ranges 记录；§7 改五项阻塞。
- 门：minix-boot 83、boot-shim 66、UEFI 目标编译、riscv bootface QEMU PASS；lint 0。

### 第二百四十三笔（01 阶段 · I-29 第三十五轮：来源域统一 + 量化合同 + 静态窗口）

代码 `53755840b`；主文档 490 → 519 行。

- MRB 零长度非终止项拒绝；UEFI 热插拔属性位（0x400）负控测试。
- §3 与 O-06 统一：DTB 保留身份 + UEFI 物理处置都不能单独推出 RamSourceDomain 成员资格。
- O-06 属性位改为 capability 元数据（WP ≠ 页表写权限），区分 GetMemoryMap 属性与 Memory Attributes Table。
- 新增 O-12i（六投影页粒度 + 非页对齐设备窗口负控）、O-12j（MRB 偏移对齐/边界缺口）。
- O-12h 补描述符重叠与 2^64 顶端；O-12c 补实际静态窗口清单（5 条）与冲突规则倾向。
- 门：minix-boot 84、boot-shim 67、UEFI 双目标编译、riscv bootface QEMU PASS；lint 0。

### 第二百四十四笔（01 阶段 · I-29 第三十六轮：PLIC 窗口与页域公式）

代码 `f98d4edff`；主文档 519 → 548 行。**哈希更正**：第二百四十三笔的代码/文档落点恢复后为 `5a519b3f2` / `7d8a7c36a`（原记 `53755840b` / `b96cc6e06` 因合并误释放，已 cherry-pick）。

- riscv64 PLIC 窗口 2 MiB → `0x24_0000`（覆盖 32 hart 的 M/S context）；QEMU DTB reg 为 `0xc000000+0x600000`。
- `EFI_MEMORY_HOT_PLUGGABLE` 修正为 `0x0010_0000`（EDK2 头），负控用精确位值。
- O-12c：PLIC/aarch64 窗口解释与版本绑定；O-12i：`RamSourcePageDomain`/`PMMExcluded`/`PMMAvailable` 公式与 MRB 双粒度；O-03：六行冲突规则；O-12j 仍标未闭合。
- 门：minix-boot 84、boot-shim 67、UEFI 双目标编译、riscv bootface QEMU PASS；lint 0。

### 第二百四十五笔（01 阶段 · I-29 第三十七轮：PMM 分区公式与设备映射包络）

代码 `9b2b09f1c`；主文档 548 → 573 行。

- PLIC：资源 `0x24_0000`，授权按 2 MiB 叶子包络 `0x40_0000`；编译期断言对齐与 aperture 上限。
- O-12c：`DeviceResourceRange` / `DeviceMappingRange` 两栏，授权覆盖包络。
- O-12i：`PMMAvailable(p)` 补类型资格、显式保留、存活对象、平台拒绝，`PMMExcluded` 取差集。
- O-03：`no-map` 三分（禁止标准映射、禁止非受控推测、受控驱动使用）与 MRB 部分页规则。
- O-12j 标不签并给出 `fdt 0.1.5` 行为；O-12g 补排除原因多重归属；§6 基线四条命令。
- 门：minix-boot 84、boot-shim 67、UEFI 双目标编译、riscv bootface QEMU PASS；lint 0。

### 第二百四十六笔（01 阶段 · I-29 第三十八轮：DTB 首访边界与范围四层化）

代码 `a52cfee54`；主文档 573 → 607 行。

- DTB 指针先验 8 字节对齐、totalsize 40 B–64 MiB 上限与 checked；三处 boot 面接入。
- O-12c 四层范围；IO-APIC/LAPIC 独立包络；PLIC 绑定 QEMU/accel/CPU/-smp/S-mode 配置。
- O-12i 页集合类型；O-12g 补 `/memory` 重叠与保留不输出区间描述符；O-06 Runtime 边界；O-05 硬件同步；§1.5 PMA/MTRR。
- O-12h 精度；O-12j `fdt 0.1.5` 核对结果；§6 基线统一 86/67；§7 六组。
- 过程更正：第三十七轮正文补丁在 reset 恢复时丢失，本轮补回；教训「reset 后重放全部补丁」。
- 门：minix-boot 86、boot-shim 67、riscv/aarch64 编译、riscv bootface QEMU PASS；lint 0。

### 第二百四十七笔（01 阶段 · I-29 第三十九轮：FDT 容器校验与范围五列化）

代码 `1a9e2a14f`；主文档 607 → 620 行。

- `fdt_layout::validate` 检查整容器（magic/totalsize/对齐/块边界与重叠/MRB 终止项），五处 DTB 入口接入；O-12j 关闭。
- O-12c 五列（Aperture/Required/PolicyAuthorized/Coverage），UART 实测 256 B，PLIC profile 收紧到单 socket+AIA 关闭+TCG。
- O-06 拆 Loader/BootServices 的 CODE/DATA 四行、默认 NX；§4 权限改为对象义务制；Reserved 域外但来源身份有效。
- O-12i `FullPages`/`TypeAllocatablePages`/`no-map` 部分页；O-12g 同义重复 vs 异义冲突；O-12h 版本策略与 2^64 独立错误码。
- §6 统一 96/67；§7 O-01 归入来源组，改名「第一轮实现复审的起点」。
- 门：minix-boot 96、boot-shim 67、四目标编译、riscv 与 aarch64 UEFI QEMU 双 PASS；lint 0。

### 第二百四十八笔（01 阶段 · I-29 第四十轮：容器版本/块顺序与 O-12j 三分）

代码 `3333d428c`；主文档 620 → 637 行。

- `fdt_layout::validate` 增加 FDT 版本（17/≤17）与项目块顺序（MRB 先于 structure）检查；+3 测试（99）。
- O-12j 拆 a（容器，已实现）/b（结构块解析，未闭合）/c（节点语义，未闭合）；§2.1 归属改正。
- O-11 改名两层授权并补 PA/VA 与整段可达性；O-12c 区分预期/观测覆盖、APIC profile、PLIC use set；O-12i 先裁决再取整页。
- 门：minix-boot 99、boot-shim 67、双目标编译、两个 QEMU 腿 PASS；lint 0。

### 第二百四十九笔（01 阶段 · I-29 第四十一轮：FDT 版本子集与平台窗口授权）

代码 `23feb9bc0`；主文档 637 → 646 行。

- FDT 版本固定 v17/16（项目子集）；结构块 4 字节对齐、FDT_END 结尾、MRB→structure→strings 顺序；+3 测试（102）。
- UART 2 MiB 包络改 `RiscvVirtPlatformMmioWindow`；PLIC context 推导链；`PlatformMemoryProfile`；`DeviceAccessUseSet` 八字段。
- 五个校验调用点表（含「是否参与内存政策」）；空表头删除；no-map/MRB 措辞；FullPages 页编号；MRB 中文名。
- 门：minix-boot 102、boot-shim 67、双目标编译、两个 QEMU 腿 PASS；lint 0。

### 第二百五十笔（01 阶段 · I-29 第四十二轮：结构预扫受阻与容器收口）

代码 `51b784cb3`；主文档 658 → 666 行。

- 容器校验：MRB 16 条上限进扫描函数；strings 非对齐正向测试；104 测试。
- O-12j-b 预扫尝试：真实 DTB 上 `fault_fetch`（cause=1，`epc` 在 `validate_structure` 的 `jr`）；`match`/比较链都被 LLVM 变成 `.rodata` 跳转表，早期映射不覆盖该绝对地址；已回退并登记方向（移到分页后 / 无跳转表编译形态）。
- 文档：O-12c 平台窗口资源清单与政策字段；O-06 属性三分（WP≠RO、SP 不产生禁令）与 CONVENTIONAL 的 WB 条件；O-12i `PageBytes` 修正；§2.7 复核口径；§6 104/67。
- 门：minix-boot 104、boot-shim 67、riscv 与 aarch64 UEFI QEMU 双 PASS；lint 0。

### 第二百五十一笔（01 阶段 · I-29 第四十三轮：预扫落地，早期执行禁用跳转表）

代码 `0464a1e5c`；主文档 666 → 677 行。

- 结构块预扫落地（27 条测试）；`validate_structure` 反汇编无表驱动 jr。
- 关键修正：本工具链选项是 `-C jump-tables=no`（非 `no-jump-tables`）；RUSTFLAGS 覆盖 config，故 xtask 注入 + config 双写；aarch64 直启目标同配。
- 新合同 EarlyExecutionClosure（指令 ∪ 只读数据依赖 ∪ 可写状态）。
- O-12i 半开区间、§2.1 调用链、PLIC 段落合并、§6 113/67。
- 门：minix-boot 113、boot-shim 67、两 QEMU 腿 PASS；lint 0。

### 第二百五十二笔（01 阶段 · I-29 第四十四轮：预扫与 fdt 0.1.5 消费能力对齐）

代码 `d615d0190`；主文档 677 → 698 行。

- 结构预扫拒绝「子节点后属性」（`PropertyAfterChild`）；深度上限 64 → 63（`all_nodes()` 父栈 64 槽、根占下标 1）。
- 撤回上一轮判断：「QEMU/OpenSBI 会产出属性后置」不成立——QEMU 8.2.2 `dumpdtb` 全树 0 处违规，且两条 QEMU 腿在「拒绝」规则下 PASS；上一轮观测来自有缺陷的临时扫描器。
- 新测试 `the_parser_consumes_the_deepest_tree_this_gate_accepts`；fdt_layout 共 28 条。
- 文档：O-11 补自有副本第三段；O-12g 原始记录层列为禁止集合编译的前置；UEFI 属性位证据改按规范源文件 + EDK2；`DeviceRequiredAccessEnvelope` 统一命名；§6 清单按代码重生成。
- 门：minix-boot 114、boot-shim 67、riscv bootface 与 aarch64 UEFI QEMU 双 PASS、x86_64-unknown-uefi check 通过。

### 第二百五十三笔（01 阶段 · I-29 第四十五轮：来源记录层落地）

代码 `28e277d5b`；主文档 699 → 751 行。

- `SourceRecord` 五类 + `collect_with_sources()`；`raw_*` 不裁剪、`page_*` 供投影；容量 40，`collect()` 语义不变。
- 新测试四条：no-map 记录留存、MRB 原始范围、RAM 外声明留存、记录覆盖每张投影表条目。
- 文档：O-11 provenance/reachability 分离 + 复制算法 + 一致性合同；O-12h `2^64` 下移平台准入（代码待改）；O-06 兼容矩阵/第一版处置；`OrdinaryRamEligibility`；O-12c VA 观测单元；O-12i 三范围；O-05 `FrozenBootMappingSet`。
- 门：minix-boot 118、boot-shim 67、两条 QEMU 腿 PASS、x86_64-unknown-uefi check 通过。

### 第二百五十四笔（01 阶段 · I-29 第四十六轮：记录只留事实，容量按输入上限）

代码 `9a4d0eb51`；主文档 752 → 765 行。

- `SourceRecord { id, kind, origin, raw_start, raw_end }`：去掉预置页范围（`/memory` 内向、保留外向、MRB 精确，不能共用一个）；`kind` 增 `ReservedReusable`；MRB 更名 `MrbEntry`。
- 容量 40 改由输入上限推出；负控 9 条 RAM 外 `no-map`（不进任何表）仍被拒绝。
- 新测试 `dtb_collect_and_collect_with_sources_agree`、`dtb_no_map_declarations_count_against_the_declaration_cap`；minix-boot 120、boot-shim 67；riscv QEMU PASS。
- 登记：四条 DTB 调用点仍用 `collect()`，记录未进启动路径（§2.5 三层表）。
- **外部红**：`2d292a06b`（sef E-NETSTART）给 `SefIpc` 加 `sendrec`，`os/servers/rs/src/lib.rs:203` 未实现 → aarch64-unknown-none 编译失败，aarch64 QEMU 腿本轮无法运行。属他人切片，未改。

### 第二百五十五笔（01 阶段 · I-29 第四十七轮：输入声明与派生前缀分层）

代码 `b49afa97d`；主文档 765 → 775 行。

- `class()`：`FirmwarePrefix` 是派生保留，其余五类是输入声明；origin 带 `{node, reg}` 与 `memory_id` 派生链。
- 三条新测试：共用声明上限（普通/reusable/no-map 合计 9 被拒）、40 条容量恰好可达、前缀指回其 `/memory` 记录。
- 文档：修正六类/五类与 `reusable` 矛盾；`OrdinaryRamEligibility` 收窄；`uefi-raw 0.9` 常量展开；O-12i 端点统一待改；§6 证据分层。
- aarch64 仍被 `servers/rs` 的 `E0046`（`sendrec`）阻断，启动行为未验证。
- 门：minix-boot 123、boot-shim 67、riscv QEMU PASS。

### 第二百五十六笔（01 阶段 · I-29 第四十八轮：重复声明与派生参数）

代码 `09e56a729`；主文档 776 行。

- `SourceRecord.range_*` 中性命名；派生前缀 `origin = FirmwarePrefix { memory_id, cut }`。
- 精确重复：投影去重、来源记录各留一条；部分重叠拒绝；同范围异语义 → `ConflictingReservationAttributes`。
- 新测试 4 条（重复 2、冲突 1、反向完整性 1）；minix-boot 127、boot-shim 67、riscv QEMU PASS。
- aarch64 仍被 `servers/rs` 的 `E0046` 阻断。

### 第二百五十七笔（01 阶段 · I-29 第四十九轮：MRB 重复改判与投影分层）

代码 `b3c93b83f`；主文档约 800 行。

- 撤回上一轮把重复去重套到 MRB 的做法：DTSpec 5.3.2 要求保留区互不重叠，重复即重叠，改为拒绝；`/reserved-memory` 去重明确为项目策略。
- 不变量 1 补四层（来源事实/冲突解析/纯集合/归因）与各投影量化顺序；O-11 增全范围可达、源目标不相交、内容一致三条；DeviceAccessUse 补 VA；RESERVED 与 UNUSABLE 语义显式分开；WP 归物理保护能力。
- 门：minix-boot 127、boot-shim 67、riscv QEMU PASS；aarch64 仍被 `E0046` 阻断。

### 第二百五十八笔（01 阶段 · I-29 第五十轮：三语义重复矩阵）

代码 `59aa94cf3`；主文档约 820 行。

- `ReservationIntent` 三分；普通+reusable、reusable+no-map 同范围改判冲突；`/memory` 部分重叠新错误 `OverlappingMemoryRanges`。
- 旧夹具 `dtb_multi_region_and_unit_address_variant` 本是重叠 RAM，已修正。
- 文档：UEFI↔DTB 覆盖合同与三种必测形态（未实现）；`EfiReservedMemoryType` 两层语义；O-11 MRB 授权措辞；`LiveInputReservation = Outward(...)`；UEFI 描述符记录最小字段。
- 门：minix-boot 131、boot-shim 67、riscv QEMU PASS；aarch64 仍被 `E0046` 阻断。
