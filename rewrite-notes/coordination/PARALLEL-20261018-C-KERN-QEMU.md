# 并发任务板 C — 一线·内核/引导/真机（`os/kernel`、`os/arch`、`os/plat`、`os/boot-shim`、`os/qemu-tests`、`os/libs/minix-boot`）+ **真机窗持有者**

> **这份文件是什么**：`2026-10-18` 全仓库开放项按**领地互斥**切成的三份之一。切分依据：`coordination/CONCURRENT-WORK-PROTOCOL.md` §2。
> **配套件**：`PARALLEL-20261018-A-CMD.md`（三线 命令面）、`PARALLEL-20261018-B-LIBDRV.md`（二线 库与驱动）。
> **本板的两项特殊职责**：① `os/libs/minix-boot/` 归本板（协议 §2 明文确权，它不在二线的通用 `os/libs/` 清单里）；② **QEMU 由本板串行持有**——A、B 两板需要真机读数时走本板第 4 节的令牌窗，不自起 QEMU。
> **条目状态词**（五值）：`☐` / `🔄` / `⏸`（注明等谁）/ `✅`（带日期与提交号）/ `🚫`（写明为什么现在不做）。

---

## 0. 你（接手的 agent）开工前的五件事

1. **owner 名固定 `zcode-glm`**：`bash tools/claim.sh claim <条目ID> zcode-glm`。
2. **避让清单先看**：`bash tools/check-staged-inflight.sh --list` 与 `git status --porcelain`。**`rewrite-notes/01-stage-kernel/` 现在正被一条文档线程改写**（今天该目录有未提交在制品：`02-higher-half-kernel.md`、`09-vm-boot-protocol.md`、`todo.md`、`14-exception-interrupt.md`）。本板的文档类条目（C7）**动前必须在第 6 节的跨界登记行挂账，且只允许整件重写自己那一节，不允许顺手改别人的节**。
3. **先复核锚点再领**：行号是切板快照，逐条 `grep -n "<ID>" rewrite-notes/coordination/TODO-LEDGER-OPEN.md` 后再定。
4. **真机必须串行**（协议与本板核心约束）：同一时刻只允许一棵树跑 QEMU；跨板也照这把锁排队，见第 4 节。
5. **门读数必须 L1**：每条 `✅` 都要有「可重跑命令 + 读数 + 核实日期 + 当时快照提交号」四件，缺任一件视同未核实。

## 1. 任务表（按 P0→P1→P2 排）

| # | 条目 ID | 登记处 | 状态 | 这一轮做到哪算完 | 判据与门 |
|---|---|---|---|---|---|
| C1 | `SD-10` / `P-RV-02` riscv64 非确定性内存污染 | `TODO-LEDGER-OPEN.md:21` | `🔄 优先` | 写者落网。`PD-16` 已给出写者方向裁决（同 `:21` 行）；按方向加 instrumentation，**一轮内定位即可，不强求结案** | `os/qemu-tests/test-atf-riscv64.sh` 多轮跑；每次读数记下失败签名（地址/页表项形态），把「偶发」量成「可复现比例」 |
| C2 | `SD-37` 次级核放行邮箱无生产者（`AP_GO` 只有声明与等待位） | `TODO-LEDGER-OPEN.md:20` | `☐` | 内核侧写下 `AP_GO` 的生产者，使次级核真正被安放。钳位仍在生效，**别一次拆掉钳位**（解除钳位的三个前置条件在同 §1 行内） | 三架构 `run_all.sh`；x86_64 的钳位行为变化要有前后对照读数 |
| C3 | `SD-41` riscv64 SWI/EI 臂、aarch64 生成中断臂未接入；两架构用户同步异常面 | `TODO-LEDGER-OPEN.md:25` | `☐` 可拆多轮 | 按架构拆：本轮只做 riscv64 的一条臂（SWI 或 EI 择一），做通并留下门，另一臂下一轮 | `run_qemu.sh <binary> <arch>` 单架构即可验；异常面用 E5 族对应测试 |
| C4 | `P-X86-01` x86_64 未进上游自动化装配白名单 | `TODO-LEDGER-OPEN.md:36` | `☐` | 把 x86_64 放进装配白名单。**账面明确写着「终态判据必须上机」**，所以这一步没有真机读数就不算完 | `os/qemu-tests/run_all.sh` 全量；当前基线见第 3 节 |
| C5 | `P-RV-01` `-smp 2/4` 的多核真机面 | `TODO-LEDGER-OPEN.md:41` | `⏸ 等 C2` | 次级核放行没落地前，多核真机读数没有意义 | 同上 |
| C6 | `E5` 族真机腿（含 `E5-SMP`、`E5-ARCH`、新门 `test-e5-ds.sh`、x86_64 fault storm） | `TODO-LEDGER-OPEN.md:103`、`:148`、`:171`、`:172` | `🔄 持续` | 每轮把失败数往下压一件即可；`test-e5-ds.sh` 这枚新门的接入是本轮可选切片 | **现基线读数（`TODO-LEDGER-OPEN.md:148` 记录）：`run_all.sh` = 32 passed / 7 failed / 3 build-fail**；判据是串口出现 `### TEST_RESULT: PASS <name> ###`（`os/qemu-tests/README.md:55`） |
| C7 | 一线文档的 `[ARCH]` 三处一致（`SD-20`/`SD-42` 的代码已落、文档半待补） | `rewrite-notes/01-stage-kernel/todo.md:42`；开放文档项 `:195`–`:202`（`I-22`/`I-23`/`I-24`/`I-20`/`I-17`/`I-18`/`I-19`，`I-21` 刻意后置） | `⏸ 等文档线程让路` | 补 `06-proc-init-boot-proc.md` §2.7 与 `04-platform-discovery.md` §1.114 的 `[ARCH]` 标注；**必须与代码注释在同一次提交里落**，并跑 `tools/check-rs-unwired.sh`（`TODO-LEDGER-OPEN.md:29` 的要求） | 文档改动前置 `style-bible` 技能；`bash tools/doc-style-lint.sh --diff` 零 error |
| C8 | `SD-21` / `PD-09` 的 16 字节栈对齐 | `TODO-LEDGER-OPEN.md:26` | `🚫 需人裁决` | 需要一次 `[ARCH]` 偏离裁决才能动。**维持登记但不排期**，本行不许被翻成 `☐` 后直接开工 | 裁决落地后由编排线补 `✅` 锚点 |
| C9 | **为本板持有真机窗服务的外来单**：`COMM-DIR-OPERAND`、`CMP-DIR-OPERAND`（open-on-dir 真值）、`JOT-PREC-RANDOM-AB-ON-MACHINE`、B 板的 `V1-P1-1` 真机步与 `at_wini` 尾巴 | `A` 板 `:2143`/`:2088`/`:1214`；`B` 板 `14-stage-runtime/todo.md:23`、`16-stage-drivers/todo.md:165`（§7） | `☐` 排队 | 这些条目的**码不在本板改**（归 A/B），本板只负责把它们要的真机读数跑出来并回传给对应板 | 单架构 `run_qemu.sh` 足矣的就不跑全量；回传时在对方板的状态列写「读数 + 日期 + 提交号」 |

## 2. ⛔ 禁碰清单（本板默认只动一线路径）

| 不可写 | 属于谁 |
|---|---|
| `os/libs/` 除 `minix-boot/` 之外的全部 crate | B 板 |
| `os/drivers/`、`os/servers/`、`os/fs/`、`os/net/` | B 板 / 服务器腿 |
| `os/commands/`、`os/etc/`、`os/xtask/`、`rewrite-notes/18-stage-commands/` | A 板 |
| `rewrite-notes/01-stage-kernel/` 里文档线程正在改的那几件（第 0 节第 2 条列出的四件） | 文档线程（未提交在制品）——只读 |
| `rewrite-notes/coordination/TODO-LEDGER-OPEN.md`、`TODO-LEDGER-DONE.md`、`new_edge*.md`、`PARALLEL-*`（别人的板） | 编排线 / 各板自持 |

## 3. 门配方

- 三架构真机门的定义：`AGENTS.md:39`（「三架构真机门见 `os/qemu-tests/README.md`」）；全量入口 `os/qemu-tests/README.md:20` 的 `./run_all.sh`（CI 入口：`:28` 注 `run_all.sh # CI entry point`），单架构 `run_qemu.sh <binary> <arch>`（`:29`、`:66`），通过判据是串口 `### TEST_RESULT: PASS <name> ###`（`:55`；`run_qemu.sh` 捕获串口并查该标记：`:66`）。
- **当前基线**：`run_all.sh` = **32 passed / 7 failed / 3 build-fail**（`TODO-LEDGER-OPEN.md:148` 记录）。任何一轮声称改进，必须给同一命令的新读数与之对照。
- 内核宿主测：`cargo test -p minix-kernel --features mock`（必须带 `--features mock`；裸跑被 `form_contract` 绊线挡住是设计行为，不要放宽绊线）。CI 侧 `kernel-tests.yml` 的 `host-default-form` 腿即此命令。
- 权威容器编译门与在线/离线秒判：照 A 板第 3 节执行，**`-u "$(id -u):$(id -g)"` 不可省**（一线最容易留 root 属主残骸，本会话刚回收 3.5G）。
- 交叉编译面：`cargo check --workspace -j 1`（在 `os/` 根）+ 三架构 `--bins` 各 `Finished`。

## 4. 真机窗的令牌锁（本板持有，跨板共用；机制现造，因为 `tools/` 里没有 QEMU 锁装置）

`tools/` 现有的并发装置只有 `claim.sh`、`check-staged-inflight.sh`、`todo-reconcile-scan.sh`、`todo-staleness-check.sh`——**没有真机锁**。所以本板用一把目录锁把「QEMU 必须串行」变成机制。取窗（任何板都用这段）：

```bash
QLOCK=/home/xzhao/github/minix-rs/tmp/qemu.lock
for i in $(seq 1 30); do
  if mkdir "$QLOCK" 2>/dev/null; then                 # mkdir 是原子操作：抢到就是持有
    echo "<owner>-$(date -Is)" > "$QLOCK/owner"; break
  fi
  echo "真机被占：$(cat "$QLOCK/owner" 2>/dev/null)，第 $i 次等待"; sleep 120
done
# ……在持有期内跑 run_all.sh / run_qemu.sh ……
rm -f "$QLOCK/owner"; rmdir "$QLOCK"                  # 用完立刻释放，这是硬要求
```

三条补充规则：
1. **抢占要登记**：若 `owner` 里的时间戳超过 40 分钟，且两查都表明该 owner 无在途提交（`git log --since` 查提交、`ls .wt` 查工作树），可判为僵持，抢占前先在自己板写一行「抢占 `<owner>` 于 `<时间>`，依据 `<读数>`」。
2. **持有期内不写别的账**：QEMU 串口读数是易腐品，边跑边改代码会污染。跑完先落读数，再做别的。
3. **排队等不到就换活**：拿不到窗不等于阻塞，按各自板的任务表挑一条不需要真机的先做（这就是用户说的「可以 sleep 等待尝试」的可执行版）。

## 5. 内核面红线（照 `AGENTS.md` 与协议原文，不重复论证）

- `#![no_std]`  everywhere（除 `#[cfg(test)]`）；错误类型必须映射 Minix3 errno，禁自造错误码。
- 内核是 **SMP + BKL**：共享数据用 `Arc`+`Mutex`/`Atomic`，**禁 `Rc`/`RefCell` 跨 CPU**；BKL 临界区内不得 sleep / schedule / IPC。
- 抽象语义不描述机制：启动契约不在射程内，arch trait 自身可谈硬件名，其余层看不见硬件差异。
- 三档术语（Rewrite / Refactor / Architectural Evolution）三处一致；`[ARCH: ...]` 要同时出现在文档、设计、代码。

## 6. 跨界登记行（本板兼当编排板的入口）

任何板想动**不在自己领地内**的文件，先在这里加一行（格式固定，便于并发时一眼看清）：

`<日期> <线> / <条目ID>：要改的文件清单前 3 个 + 一句话目的 —— 状态 ☐/🔄/✅`

当前登记：
- `2026-10-18` B / `E-DMWIRE`：可能要碰 `os/servers/` 与 `os/drivers/` 交界面，先登记 —— `☐`
- `2026-10-18` A / 第 5 节三条：要碰 `os/commands/`（自己领地）但**需要 C 板真机窗** —— `⏸ 等窗`

## 7. 每轮收尾（顺序固定）

claim 树内一笔提交 → 主树 `git merge --ff-only claim/<ID>-zcode-glm` → `bash tools/claim.sh release <ID> zcode-glm` → 五道收尾校验（命令见 A 板第 7 节）→ `bash tools/claim.sh sweep` 确认自己的树真的从磁盘消失。**主树内禁 `checkout`/`reset --force`**（C-35 判例）；要暂存自己的东西时用带唯一 tag 的方式，别用裸 `git stash`——工作树是所有人共用的，裸 stash 会连别人的在制品一起卷走。
