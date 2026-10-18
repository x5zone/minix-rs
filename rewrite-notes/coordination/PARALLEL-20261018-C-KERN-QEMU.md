# 并发任务板 C — 一线·内核/引导/真机（`os/kernel`、`os/arch`、`os/plat`、`os/boot-shim`、`os/kernel-image`、`os/qemu-tests`、`os/libs/minix-boot`）+ **真机窗持有者**

> **这份文件是什么**：`2026-10-18` 把全仓库开放项按**领地互斥**切成三份（A/B/C）中的一份，交给一个独立 agent 并发推进。切分依据：`coordination/CONCURRENT-WORK-PROTOCOL.md` §2。
> **本板的领地**：`os/kernel/`、`os/arch/`、`os/plat/`、`os/boot-shim/`、`os/kernel-image/`、`os/qemu-tests/`、`os/libs/minix-boot/`、`rewrite-notes/01-stage-kernel/`、`rewrite-notes/00-master-plan/`、本文件。协议 §2 的一线清单没有单列 `os/kernel-image/`，按职责（引导镜像与 DTB 解析）归本板；若编排线另有裁决，以裁决为准。
> **配套件**：`PARALLEL-20261018-A-CMD.md`（三线 服务器/文件系统/网络/命令面）、`PARALLEL-20261018-B-LIBDRV.md`（二线 库与驱动）。
> **本板的两项特殊职责**：① `os/libs/minix-boot/` 归本板（协议 §2 明文确权，它不在二线的通用 `os/libs/` 清单里）；② **QEMU 由本板串行持有**——A、B 两板需要真机读数时走本板第 4 节的令牌窗，不自起 QEMU。
> **条目状态词**（五值）：`☐` / `🔄`（已领取，别人别碰）/ `⏸`（注明等谁）/ `✅`（带日期与提交号）/ `🚫`（写明为什么现在不做）。

---

## 0. 你（接手的 agent）开工前的五件事

1. **owner 名固定 `zcode-glm`**：`bash tools/claim.sh claim <条目ID> zcode-glm`。**worktree 路径是分支名去掉 `claim/` 后整段小写**（`tools/claim.sh` 的 `wt_path`），以 claim.sh 打印的路径为准。
2. **避让清单先看**：`bash tools/check-staged-inflight.sh --list` 与 `git status --porcelain`。**`rewrite-notes/01-stage-kernel/` 现在正被一条文档线程改写**（该目录有未提交在制品：`02-higher-half-kernel.md`、`10-switch-to-user.md`、`todo.md`、`14-exception-interrupt.md`）。本板的文档类条目（C7/C7b）**动前必须在第 6 节的跨界登记行挂账，且只允许整件重写自己那一节，不允许顺手改别人的节**。
3. **先复核锚点再领**：行号是切板快照，逐条 `grep -n "<ID>" rewrite-notes/coordination/TODO-LEDGER-OPEN.md` 后再定。
4. **真机必须串行**（协议与本板核心约束）：同一时刻只允许一棵树跑 QEMU；跨板也照这把锁排队，见第 4 节。
5. **门读数必须 L1**：每条 `✅` 都要有「可重跑命令 + 读数 + 核实日期 + 当时快照提交号」四件，缺任一件视同未核实。

## 1. 内核主任务表（按 P0→P1→P2 排）

| # | 条目 ID | 登记处 | 状态 | 这一轮做到哪算完 | 判据与门 |
|---|---|---|---|---|---|
| C1 | `SD-10` / `P-RV-02` riscv64 非确定性内存污染 | `TODO-LEDGER-OPEN.md:21` | `☐` **P0 优先** | 写者落网。`PD-16` 已给出写者方向裁决（同 `:21` 行）；按方向加 instrumentation，**一轮内定位即可，不强求结案** | `os/qemu-tests/test-atf-riscv64.sh` 多轮跑；每次读数记下失败签名（地址/页表项形态），把「偶发」量成「可复现比例」 |
| C2 | `SD-37` 次级核放行邮箱无生产者（`AP_GO` 只有声明与等待位） | `TODO-LEDGER-OPEN.md:20`（已迁 `TODO-LEDGER-DONE.md` §1 同日行） | `✅ 2026-10-18（按台账 :20 原列乙案：删邮箱）` | **考古翻案后按乙案解决**：门基线（写者恒 0）`ap-arrived=1` 坐实次级核交付＝SBI/PSCI 单通道、邮箱零消费者零停车者——「使次级核真正被安放」半在本次考古前已由既有 SBI/PSCI 接线达成（门为证）；kernel-image 两架构落选臂改本地 wfi，两处静态与 park 装载序列删除；甲案（写生产者）因被投递路径在一切现链路不可达、无法给 L1 判据而弃。钳位未动；撤钳余前置见 `:17` 更新（C2b 唤醒臂 + C1 污染案） | aarch64 门前后对照逐位一致（arrived 1↔1 / timeout 0↔0 / panic 0↔0）；riscv 门 PASS（`sbi-hs ret=0`、`ap-arrived hart=1`）；`check-layout.sh all` 三架构 PASS；kernel mock 门（docker）832/0/8；`grep -rn "AP_GO" os --include=*.rs` 代码引用 0；落点＝`claim/SD-37-zcode-glm` 合入笔 |
| C2b | `SD-24` arm2：中断入口「唤醒空闲核」的生产者 | `TODO-LEDGER-OPEN.md:18`（arm1 清除半已落地） | `☐` | 在中断入口补上把空闲核标回忙的生产者；与 C2 同族但分开领，**不要合并成一笔**。`SD-41` 的 BKL 两步写建议随本项一起审 | `grep -rn "context_stop_idle" os --include=*.rs` 的现状读数；`run_all.sh` 前后对照 |
| C3 | `SD-41` riscv64 SWI/EI 臂、aarch64 生成中断臂未接入；两架构用户同步异常面 | `TODO-LEDGER-OPEN.md:25` | `☐` 可拆多轮 | 按架构拆：本轮只做 riscv64 的一条臂（SWI 或 EI 择一），做通并留下门，另一臂下一轮；aarch64 的生成中断臂再下一轮 | `run_qemu.sh <binary> <arch>` 单架构即可验；异常面用 E5 族对应测试 |
| C4 | `P-X86-01` x86_64 未进上游自动化装配白名单 | `TODO-LEDGER-OPEN.md:36` | `☐ 子项` | 缺口①已闭；本行只做剩下的②白名单/③门脚本/⑤上机：`os/xtask/src/image.rs:ATF_BOOT_LEG_READY` 加 x86_64、`tools/build-atf-test.sh` 补 x86_64 分支、新增 `os/qemu-tests/test-atf-x86_64.sh`。**`os/xtask/` 属 A 板、`tools/` 是共享件：先在第 6 节登记，等 A 板让路再动那两处**。工具链走 host `cc` + glibc（同 `:36` 的 `2026-10-13` 注），**不需要装 freestanding x86_64 工具链**。**终态判据必须上机** | `run_all.sh` 全量；新 ATF x86_64 门读 36/36；上机前先过第 4 节令牌 |
| C5 | `P-RV-01` riscv64 多核配置 | `TODO-LEDGER-OPEN.md:41` | `✅ 主口径已闭（`-smp 2` 双门真机 PASS，坐实于 `e6b4883fd`）；余两件 ⏸ 等 C2b（C2 已按乙案闭账：邮箱删除，撤钳前置清单见台账 `:17` 更新）` | **不要再把「从未跑过」当现状**：ATF 全量与 cmd-smoke 的 `-smp 2` 都已真机 PASS。余下的是 ① `-smp ≥3`（含 `-smp 4`）未验；② AP 并行调度未通（次级核 wfi 驻留，撤钳 = C2b 唤醒臂 + C1 污染案结）。本轮若要碰，只做 `-smp ≥3` 的复跑，不重开主口径 | `bash os/qemu-tests/test-atf-riscv64.sh`；cmd-smoke 同门；读数附串口 SMP 证据链 |
| C6 | `E5` 族真机腿（含 `E5-SMP`、`E5-ARCH`、新门 `test-e5-ds.sh`、x86_64 fault storm） | `TODO-LEDGER-OPEN.md:103`、`:148`、`:171`、`:172` | `☐` 持续（每轮至少一件） | 每轮把失败数往下压一件即可；`test-e5-ds.sh` 这枚新门的接入是本轮可选切片。**基线读数是账面记录，不是本轮现跑**：`run_all.sh` = 32 passed / 7 failed / 3 build-fail（`:148`，该行是已闭复测记录）；任何改进必须给同一命令的新读数 | 判据是串口出现 `### TEST_RESULT: PASS <name> ###`（`os/qemu-tests/README.md:55`） |
| C7 | `SD-20`/`SD-42` 的 `[ARCH]` 三处一致（代码已落、文档半待补） | `rewrite-notes/01-stage-kernel/todo.md:42` | `⏸ 等文档线程让路` | 补 `06-proc-init-boot-proc.md` §2.7 与 `04-platform-discovery.md` §1.114 的 `[ARCH]` 标注；**必须与代码注释在同一次提交里落**，并跑 `tools/check-rs-unwired.sh`（`TODO-LEDGER-OPEN.md:29` 的要求） | 文档改动前置 `style-bible` 技能；`bash tools/doc-style-lint.sh --diff` 零 error |
| C7b | 01 篇文档卫生 backlog：`I-22`/`I-23`/`I-24`/`I-20`/`I-17`/`I-18`/`I-19`（`I-21` 刻意后置）、`07-pagetable-struct.md` 断链、锚点漂移、正式文档计数口径、文档改名与代码注释同笔的前置 | `01-stage-kernel/todo.md:195`–`:202`；`TODO-LEDGER-OPEN.md:29`、`:157`、`:165`、`:167` | `⏸ 等文档线程让路` | 这是文档线程的排期 backlog，**不是本板代码 agent 的任务**；本板只负责在文档线程让路后按单篇领取，不整批扫。逐条读原行再定切片；`:29` 的改名前置必须与代码注释同一笔提交 | `python3 tools/notes-link-check.py rewrite-notes/`；`tools/anchor-resolve.sh --check`；`doc-style-lint` |
| C8 | `SD-21` / `PD-09` 的 16 字节栈对齐 | `TODO-LEDGER-OPEN.md:26` | `🚫 需人裁决` | 需要一次 `[ARCH]` 偏离裁决才能动。**维持登记但不排期**，本行不许被翻成 `☐` 后直接开工 | 裁决落地后由编排线补 `✅` 锚点 |
| C9 | **为本板持有真机窗服务的外来单**：`COMM-DIR-OPERAND`、`CMP-DIR-OPERAND`（open-on-dir 真值）、`JOT-PREC-RANDOM-AB-ON-MACHINE`、B 板的 `V1-P1-1` 真机步与 `at_wini` 尾巴 | A 板 `:2143`/`:2088`/`:1214`；B 板 `14-stage-runtime/todo.md:23`、`16-stage-drivers/todo.md:165` | `☐ 服务队列（不领）` | 这些条目的**码不在本板改**（归 A/B），本板只负责把它们要的真机读数跑出来并回传给对应板；在 A/B 板的状态列写「读数 + 日期 + 提交号」 | 单架构 `run_qemu.sh` 足矣的就不跑全量；回传时附串口证据 |

## 1b. 内核余件承接清单（原板遗漏的结构债与半落地项）

这一节与第 1 节平级；一次只领一个条目 ID。账载状态可能早于现树，领取前先复核正文。

| # | 条目/族 | 登记处 | 状态 | 这一轮做到哪算完 | 判据与门 |
|---|---|---|---|---|---|
| C10 | `SD-15` 陷入帧与处理器上下文两套结构的撕裂校验 | `STRUCTURAL-DEBT-REGISTER-20261008.md:405` | `☐` 低优先 | 与 C1 的仪器重建同批做：加「逐次进入的陷入帧完整性校验和」，它是回答「恢复的寄存器值是否正确」的唯一仪器。**没有 C1 的案卷就不单独开工** | 校验和落点 + 宿主门；与 C1 的取证读数一并记 |
| C11 | `SD-12` 进程根的写执行互斥（`PD-22` 三步） | `STRUCTURAL-DEBT-REGISTER-20261008.md:362` | `☐` **非单轮量级** | 第一步：去掉内核文本映射的写权限；前置是静态核验没有运行期写者。第二/三步（低身份窗口退出、boot-root 拆分）另开轮 | 静态核验证据 + 三架构引导；`[ARCH]` 标注三处一致 |
| C12 | x86_64 浮点恢复腿的跨进程保存路径与文档定稿 | `TODO-LEDGER-OPEN.md:22` | `☐` | acting 臂已接、用户态首条 SSE 经真机恢复；余下是跨进程保存路径与文档定稿，先读 `:22` 的现状再切 | `cargo test -p minix-kernel --features mock`；真机 FPU 用例走 C 板窗 |
| C13 | 次级核本地定时器的真机断言（余量） | `TODO-LEDGER-OPEN.md:23`、`:140`、`:141` | `🚫 绑 C2` | **断言已进门**（`:140` 已闭：`test-smp-aps` 先释放 BKL，再硬断言「全部 AP 进调度循环 + 至少一个 AP 计到本地 tick」），本行不要重做。余量只有 `:141` 那件——cpu3 的 tick 五轮恒 0，已定谳为「空闲 AP 刻意停表 + `clamp_cpu_to_bsp` 钳位」的设计后果而非时钟缺陷，门控在 `SD-24` 撤钳（C2/C2b 的唤醒臂），**别再把它当独立定时器 bug 重推** | 撤钳落地后用 `test-smp-aps` 复跑，五核 tick 读数随 C2 的前后对照一并记 |
| C14 | `E-VMTLB` / `E-PREEMPTFLAG` 的内核半 | `TODO-LEDGER-OPEN.md:81`、`:82` | `☐` | 目标进程地址空间标识刷新与可抢占标志的生产可达性；服务侧半归 A22，本板只做内核侧 | 每项自己的门；与 A22 对账 |
| C15 | `SD-13` / `P-ALL-13` 的 delivery barrier 记账 | `STRUCTURAL-DEBT-REGISTER-20261008.md` 的 `SD-13`；`TODO-LEDGER-OPEN.md:49` | `🚫 绑 C2` | **今天没有独立代码可做**：delivery leg 已重定性为 completion/visibility 要求，绑在撤钳（C2）里程碑上；本行只负责在 C2 落地时把 P-ALL-13 的认领行补上，不许提前改 IPC 屏障 | C2 的撤钳审计一并命名 |
| C16 | `SD-38` 上游 `fdt 0.1.5` 的 `find_node` 缺陷 | `STRUCTURAL-DEBT-REGISTER-20261008.md:778`（状态翻 `partially fixed`，详见 `TODO-LEDGER-DONE.md` §1 同日行） | `✅ 2026-10-18（执行半闭）` | 钉版本（workspace `fdt = "=0.1.5"`，防静默换入）＋绕行两半统一：riscv `bootface.rs::fdt_table_pa` 的活 `find_node("/chosen")`（OpenSBI 链表址承重读）换 aarch64 同款 `all_nodes` 去后缀走查（现链零行为变化、兜底姊妹链已实证缺陷），两半 `fill_memmap` 陈旧注释勘误；fork/升级＝行定义内挂起裁决，等下一个 DTB 消费者再裁 | `check-layout.sh all` 三架构 PASS；真机 `test-smp-aps-riscv64.sh` PASS（12 模块表址正经改动后的 `/chosen` 读取得）；`grep -n "find_node" os/kernel-image/src` 仅存缺陷叙述注释；落点＝`claim/SD-38-zcode-glm` 合入笔 |
| C17 | 启动入口多形态的 `SD-3`/`SD-4` 余量 | `TODO-LEDGER-OPEN.md:48` | `☐` | `SD-1` 债①的等价断言层已落；余 `SD-3` 的 per-arch「最后一根线」（aarch64 UEFI descriptor 源、x86 config tables）与 `SD-4` 的 linker/load 半地址常量清扫。先读 `:48` 的现状与 `os/libs/minix-boot/src/boot_form.rs` | `cargo test -p minix-boot --lib`；三架构引导门；地址常量清扫按 `SD-4` 的判据 |
| C18 | `SD-22`/`PD-21` 防线边界的文档半 | `TODO-LEDGER-OPEN.md:28` | `⏸ 等文档线程让路` | 裁决（`PD-21`）已定：「软件层不直接解引用用户虚拟地址」承担同一防线、硬件屏蔽位随写下沉批次再谈，且要求把这条边界写进文档，否则会被后来者读成「忘了设位」。先对账 01 篇（含 `14-exception-interrupt.md`）是否已写：已写则标 `✅` 附行号；未写则补一段；落点若在别的阶段文档，先在 §6 登记再移交 | 只改文档不改码（码侧裁决已定）；`doc-style-lint` 零 error |
| C19 | `P-ALL-04`/`SD-25`/`T8` 取证探针滚除（结案批 `CC-INSTRUMENT`） | `TODO-LEDGER-OPEN.md:43`、`:159` | `🚫 绑 C1/C2` | `PD-10` 三档已定、逐站分类已完成：**真正可动的 0 枚**——余下探针全是 riscv64 污染案（`PD-16`）与主核钳位（`SD-24`）的在案取证。本行只在 C1 定案后随 `CC-INSTRUMENT` 结案批执行：先重跑一次逐站归类（含 `:159` 说的站外三式并扫），再滚 | 归类读数随结案批一并记；提前滚探针＝毁 C1 的取证现场 |
| C20 | `KernelInfo.bootstrap_*` 哨兵字段的去留 | `TODO-LEDGER-OPEN.md:30` | `🚫 维持登记` | 已临时裁决「保留 + guard 为现状」；删字段要动 `KernelInfo` 公共 API 与 40+ 个 qemu bootstrap 测试内核，按「等第二个真实使用者（独立 ELF 自引导 `TODO-02-3`）出现再裁」搁置。本行不许被翻成 `☐` 提前删 | 裁决换向时由编排线补锚点 |
| C21 | 地址常量清扫的低危残余 | `TODO-LEDGER-OPEN.md:138` | `🚫 低危不排期` | 高/中危已修并带断言（`:138` 已定 adjudicated）；余量是注释层卫生与 `#[cfg(test)]` 夹内字面量（如 `A12` 的 x86 心智模型注释），落点可能跨 A 板的 `os/servers/vm` 注释——动前先判归属。下次复扫（`python3 tools/address-constant-scan.py os`，按审计豁免清单过滤）时顺带判读 | 复扫输出与豁免清单的对照读数 |

**`P-ALL-07` 的 55 个 `loop {}` 分工**：52 个在 `os/drivers/`（B 板），2 个在 `os/fs/`（A 板），1 个是 `os/boot-shim/` 的末态空转（不是服务入口，不排期）。三块板都不要把 boot-shim 那一个算成未竟的服务接线。

## 2. ⛔ 禁碰清单（本板默认只动一线路径）

| 不可写 | 属于谁 |
|---|---|
| `os/libs/` 除 `minix-boot/` 之外的全部 crate | B 板 |
| `os/drivers/` | B 板 |
| `os/servers/`、`os/fs/`、`os/net/` | A 板 |
| `os/commands/`、`os/etc/`、`os/xtask/`、`rewrite-notes/02`–`13`、`15`、`17`、`18` 阶段账 | A 板 |
| `rewrite-notes/14-stage-runtime/`、`16-stage-drivers/` | B 板 |
| `rewrite-notes/01-stage-kernel/` 里文档线程正在改的那几件（第 0 节第 2 条列出的四件） | 文档线程（未提交在制品）——只读 |
| `rewrite-notes/coordination/TODO-LEDGER-OPEN.md`、`TODO-LEDGER-DONE.md`、`new_edge*.md`、`PARALLEL-*`（别人的板）、`STRUCTURAL-DEBT-REGISTER-20261008.md`、`DOC-ITER-LEDGER.md`、`CONCURRENT-WORK-PROTOCOL.md`、`README.md` | 编排线 / 各板自持 |
| `os/Cargo.toml`、`os/Cargo.lock`、`.github/workflows/`、`tools/`、`AGENTS.md` | 共读共享件——动前在第 6 节登记；`Cargo.lock` 只由实际构建产生，不手改 |

## 3. 门配方

- 三架构真机门的定义：`AGENTS.md:39`（「三架构真机门见 `os/qemu-tests/README.md`」）；全量入口 `os/qemu-tests/README.md:20` 的 `./run_all.sh`（CI 入口：`:28` 注 `run_all.sh # CI entry point`），单架构 `run_qemu.sh <binary> <arch>`（`:29`、`:66`），通过判据是串口 `### TEST_RESULT: PASS <name> ###`（`:55`；`run_qemu.sh` 捕获串口并查该标记：`:66`）。
- **当前基线**：`run_all.sh` = **32 passed / 7 failed / 3 build-fail**（`TODO-LEDGER-OPEN.md:148` 的记录，该行本身是已闭的复测记录）。任何一轮声称改进，必须给同一命令的新读数与之对照；不要把历史记录当本轮现跑。
- 内核宿主测：`cargo test -p minix-kernel --features mock`（必须带 `--features mock`；裸跑被 `form_contract` 绊线挡住是设计行为，不要放宽绊线）。CI 侧 `kernel-tests.yml` 的 `host-default-form` 腿即此命令。
- 权威容器编译门与在线/离线秒判：照 A 板第 3 节执行，**`-u "$(id -u):$(id -g)"` 不可省**（一线最容易留 root 属主残骸，本会话刚回收 3.5G）。
- 交叉编译面：`cargo check --workspace -j 1`（在 `os/` 根）+ 三架构 `--bins` 各 `Finished`。
- `os/kernel-image` 的包名先 `cargo metadata --no-deps --format-version 1` 取；不要按目录名猜。

## 4. 真机窗的令牌锁（本板持有，跨板共用；机制现造，因为 `tools/` 里没有 QEMU 锁装置）

`tools/` 现有的并发装置只有 `claim.sh`、`check-staged-inflight.sh`、`todo-reconcile-scan.sh`、`todo-staleness-check.sh`——**没有真机锁**。所以本板用一把目录锁把「QEMU 必须串行」变成机制。取窗函数如下（任何板都用这段；复制时把 `<owner>` 换成自己的 owner 名）：

```bash
run_with_qemu_lock() (
  set -eu
  QLOCK=/home/xzhao/github/minix-rs/tmp/qemu.lock   # 必须是主树的绝对路径，不能是链接工作树的路径
  TOKEN="<owner>-$(date -Is)-$$"
  acquired=0
  for i in $(seq 1 30); do
    if mkdir "$QLOCK" 2>/dev/null; then                 # mkdir 是原子操作：抢到就是持有
      printf '%s\n' "$TOKEN" > "$QLOCK/owner"
      acquired=1
      break
    fi
    # 僵持只报告，不自动抢占：长测跑过 40 分钟是正常形态，自动改名会把活锁当成死锁。
    if [ -f "$QLOCK/owner" ] && [ $(( $(date +%s) - $(stat -c %Y "$QLOCK/owner" 2>/dev/null || echo 0) )) -gt 2400 ]; then
      echo "锁看起来僵持：$(cat "$QLOCK/owner" 2>/dev/null || true)" >&2
      echo "先做两查（git log --since='40 minutes ago' 与 ls .wt），确认无在途、在自己板登记抢占后，" >&2
      echo "用 mv \"$QLOCK\" \"$QLOCK.stale.$(date +%s)\" 原子改名，再重跑本命令。" >&2
      exit 1
    fi
    echo "真机被占：$(cat "$QLOCK/owner" 2>/dev/null || true)，第 $i 次等待" >&2
    sleep 120
  done
  if [ "$acquired" -ne 1 ]; then
    echo "FAIL: 等满 60 分钟仍未拿到真机窗；改做不需要真机的条目。" >&2
    exit 1
  fi
  trap 'if [ "$(cat "$QLOCK/owner" 2>/dev/null || true)" = "$TOKEN" ]; then rm -rf "$QLOCK"; fi' EXIT INT TERM
  "$@"
)

# 用法：
# run_with_qemu_lock bash os/qemu-tests/run_all.sh
# run_with_qemu_lock bash os/qemu-tests/run_qemu.sh <binary> <arch>
```

三条补充规则：

1. **拿不到就退出，不许裸跑**。函数在等满 30 轮后 `exit 1`；把循环写成「等满后继续往下跑」等于没锁。拿到锁后所有 QEMU 命令都放在 `run_with_qemu_lock` 的参数里，trap 保证正常/异常退出都释放。
2. **释放前核 owner token**。trap 只在自己仍是主人时删除锁；如果锁在持有期内被僵持抢占过，token 不匹配，绝不删别人的锁。
3. **排队等不到就换活**：拿不到窗不等于阻塞，按各自板的任务表挑一条不需要真机的先做（这就是用户说的「可以 sleep 等待尝试」的可执行版）。

## 5. 内核面红线（照 `AGENTS.md` 与协议原文，不重复论证）

- `#![no_std]` everywhere（除 `#[cfg(test)]`）；错误类型必须映射 Minix3 errno，禁自造错误码。
- 内核是 **SMP + BKL**：共享数据用 `Arc`+`Mutex`/`Atomic`，**禁 `Rc`/`RefCell` 跨 CPU**；BKL 临界区内不得 sleep / schedule / IPC。
- 抽象语义不描述机制：启动契约不在射程内，arch trait 自身可谈硬件名，其余层看不见硬件差异。
- 三档术语（Rewrite / Refactor / Architectural Evolution）三处一致；`[ARCH: ...]` 要同时出现在文档、设计、代码。

## 6. 跨界登记行（本板兼当编排板的入口）

任何板想动**不在自己领地内**的文件，先在这里加一行（格式固定，便于并发时一眼看清）：

`<日期> <线> / <条目ID>：要改的文件清单前 3 个 + 一句话目的 —— 状态 ☐/🔄/✅`

当前登记：
- `2026-10-18` C / `P-X86-01`：要改 `os/xtask/src/image.rs`（A 板领地）与 `tools/build-atf-test.sh`（共享件），目的为 x86_64 装配白名单与 ATF 门；A 板让路后才动 —— `☐`
- `2026-10-18` C / `SD-20`+`SD-42` 文档同步：要改 `rewrite-notes/01-stage-kernel/06-proc-init-boot-proc.md`、`04-platform-discovery.md`（本板领地，但文档线程正在改该目录）—— `⏸ 等文档线程`
- `2026-10-18` B / `E-DMWIRE`：主线改由 A 板持有；B 只做 `minix-sys` 的 client 半与 `minix-types` 解码 —— `☐`
- `2026-10-18` B / `P-ALL-08`：`minix-sef` 框架侧由 B 改，服务器消费方归 A —— `☐`
- `2026-10-18` B / `E-DSWIRE`：`minix-sys ds.rs` 客户端半由 B 改，服务端归 A —— `☐`
- `2026-10-18` B / `E-SDEVOWN`：`minix-netdriver` 的 sdev/sockevent 与 A 板 vfs 副本要确权，B 不擅自删 —— `☐`
- `2026-10-18` A / 第 1b 节服务器线与第 5 节真机项：要碰自己的 `os/servers/`、`os/fs/`、`os/net/`、`os/commands/`，但**需要 C 板真机窗** —— `⏸ 等窗`
- `2026-10-18` A / `A4` 常量的消费者侧：`os/servers/vfs/src/cdev.rs`、`bdev.rs` 的重述归 A，B 只做定义侧 —— `☐`
- `2026-10-18` B / `B1` 的 `os/fs` 两个 `loop {}`：归 A 板（A20），B 只做 `os/drivers` 的 52 个 —— `☐`

## 7. 每轮收尾（顺序固定）

claim 树内一笔提交 → 主树 `git merge --ff-only claim/<ID>-zcode-glm` → `bash tools/claim.sh release <ID> zcode-glm` → 五道收尾校验（命令见 A 板第 7 节）→ `bash tools/claim.sh sweep` 确认自己的树真的从磁盘消失。**主树内禁 `checkout`/`reset --force`**（C-35 判例）；要暂存自己的东西时用带唯一 tag 的方式，别用裸 `git stash`——工作树是所有人共用的，裸 stash 会连别人的在制品一起卷走。
**`doc-style-lint --diff` 若报 `rewrite-notes/01-stage-kernel/todo.md` 的命中**，那是文档线程的在制品，不是你的改动；只处理自己路径的命中。
