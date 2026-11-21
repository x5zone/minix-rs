# D01 FREEZE —— 01-boot-shim-bootstrap（CONVERGED）

- 冻结日期：2026-11-11
- 工作流 edition：V3.3
- 确认轮：R34（`轮型: confirmation`）
- 冻结提交：`7dfa2c21e`（R34 修复与轨迹提交）
- 文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- 文档行数：3579（R34 时点）；R35 时点 3582
- 文档 sha256：`1a71cb515d29db66735255a362f1804369c3d5854ce29185a615f57f3b4d68d7`（R35 复核后；R34 原值 `3284d47a7f21529626e7a7b8933aa91a37c0501d4abd56244e45b3d503145b97`）
- 关联代码：`tools/doc-code-map.sh --check` 54/54 通过

## 收敛证据

- `eval-trace-check.py --round R34`：PASS（12 PASS / 2 WARN / 0 FAIL）。两条 WARN 是 OPEN-DEBT 的已批准延期（`R34-design-batch`、`F-R34-01`，D-049 已落判决账）。
- R34 台账：28 条（survey 26 + certify #1 新准入 2），处置 ① 10 / ② 4 / ④ 6 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 8；`未闭环 = 0`；`certify 新准入 = 0`（certify #2 两路均 0）。
- 100% 复验：每个 ①/④ 行有可重放 grep / path:line / 冷读回执；无抽样。
- 文档六门：citation `blocks=69 checked=29 failed=0 skipped=39 warned=1`；anchor `105/105`；structure/ref 通过；style-lint（文件级）error=0；density 承载门通过；doc-code-map 54/54。
- 宿主：`cargo test -p minix-kernel --features mock` → 866 passed / 0 failed / 8 ignored。
- 代码改动仅 `os/kernel/src/vm_handoff.rs` 的注释同步，无行为变化；QEMU 未跑（不触 arch/boot 行为门）。

## 轮次链

- R29–R33（V3.1）：R33 收口 ROUND-CLOSED；本轮先补正 R32/R33 的 P3 分流与 OPEN-DEBT 销账，使 `--round R33` PASS。
- R34（V3.3 确认轮）：全量六角色 survey → 26 条；certify #1 抓到 2 条（1 P0 自引入 + 1 P3）→ 修完 → certify #2 两路均 0 → CONVERGED。

## 未解决事项（不阻断，已登记）

- `OPEN-DEBT.md` 的 `R34-design-batch`：四项设计 P2（U-Boot 安装型全局参数化、直核腿 `collect_owned`、wire 派生标量删除、`store_kernel_info` 同值早返回），D-049 批准延期至下一 design batch（D02 循环前）。
- `OPEN-DEBT.md` 的 `F-R34-01`：aarch64 直核面 `KERN_SIZE` 缺 L9 等价构建期断言，D-049 批准延期至下一 code batch。
- `suggestions.md` R34 表：设计 P3 7 条 + 读者 SUGGESTION 26 条（未入台账，不阻塞）。

## 限制

- `CONVERGED` 的含义是“确认轮口径下没有 open 项 + 无未销账债务”，不是“零未知缺陷保证”。
- 同 agent 复核：面板与 certify 使用同一模型；已重放全部 grep / 门命令，cross-model 独立复核未做。
- 新证据、下一 edition 或 OPEN-DEBT 批次可以重开；重开不是本次失败。

## 重开条件

- 出现新的 P0/P1 事实或行为缺陷证据；
- 下一 design/code batch 实施后基线改变（需重新固定基线并重跑受影响门）；
- 用户明确要求重开。

## R35 确认轮（V3.4；D-060 授权的重开确认，append-only 追加）

- 上一节记录的是 V3.3 口径下的 R34 确认；按 D-060（用户指令 `prompt/EXECUTE-D01-D06-CONVERGENCE.md`）在 V3.5 制品链下重开一次，轮次号 R35、edition V3.4、`schema: V3.4 from R35`。
- **R31 补清点先行**：R31 断链（D-057 授权的预期红）按「R31 补清点」解除——53 条找到 24 个根因行，独立冷读 `certify/round31-backfill-coldread.md` 判 CLOSED 33 / STILL-BROKEN 20；20 条中 12 条本轮修文档、8 条按 §4.0c 门槛登记 SUGG。`--round R31` 与 `--round R34` 均恢复 PASS。
- **R35 确认轮**：六角色全量 survey（fact-a 4、fact-b 9、design-d2 13、reader-a 4、reader-b 2、reader-c 3 条 FINDING；SUGGESTION 37； CLAIM 6）。无 P0，2 条 P1。面板制品入库 `panel/round35/` + `INDEX.md`（7 文件，FINDING 35 / SUGGESTION 37 / CLAIM 6）。
- **certify**：`certify/round35-fact.md` 21/21 CLOSED、`certify/round35-reader.md` 12/12 CLOSED，均为本轮新派发的冷启动子 agent（D-055 墙 2）。`certify 新准入 = 0`。
- **机械门**：`--round R35` PASS（13 PASS / 3 WARN / 0 FAIL）；anchor 108/108；citation `blocks=69 checked=29 failed=0 skipped=35 warned=5`；structure/ref/style（error=0）/density 通过；doc-code-map 54/54；`panel-index.py --check` 7 文件哈希一致。
- **未闭环 = 0**。`OPEN-DEBT.md` 的 `R35-design-batch`（6 条设计 P2 + 7 条设计 P3 + 1 条 ARCH 提案）有 D-060 判决行与 2026-11-19 日期，按 §4.0f 出口 2 销账。
- **自引入 2 条**（§4.1 的「四条架构」与 §4.6 的 mock 分叉段，均为 R31 补清点批写下的）已在本轮修掉，并在 metrics 记 `自引入: 2`。
- **归档审计**（不带 `--round`）：PASS 24 / WARN 5 / FAIL 100，全部落在 R17–R30 的历史段（V3 之前格式）；按 §3.4 单独记账、不追溯、不阻断本轮。
- FREEZE 的限制照旧：`CONVERGED` 的含义是「确认轮口径下没有 open 项 + 无未销账债务」，不是零未知缺陷保证。新增两条已知限制：① 设计项在确认轮被系统性降级为 `R35-design-batch` 延期（§3.6 的代价）；② 面板裸编号与历史轮重名，本轮靠 `RD35-` 前缀在台账里区分（§4.0c 身份键的已知缺口）。
- 重开条件不变：出现新的 P0/P1 事实或行为缺陷证据；`R35-design-batch` 实施导致基线改变；用户明确要求重开。

## R36 确认轮（V3.4；基线变更重开，append-only 追加）

- 确认轮：R36（`轮型: confirmation`），edition V3.4，schema `V3.4 from R35`；重开依据 = 本节上一条「R35-design-batch 实施后基线改变」+ 用户 2026-11-21 指令（review loop D01 直至收敛，代裁授权）。
- 冻结提交：`961615359`（R36 轨迹与收敛提交；代码修复链 `d8ce8b29c`、`49ed0c54a`、`624fcccf1`，文档修复链 `e4374af7e`、`773e4bbc5`）。
- 文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`；行数 3600；sha256 `1c0d9de478c09760a77e4068ac8190c82c0d88c0c36fe372e791d9e524c11660`。
- 关联代码：`tools/doc-code-map.sh --check` 54/54。

### 收敛证据

- `python3 tools/eval-trace-check.py <run> --round R36`：PASS（12 PASS / 24 WARN / 0 FAIL）。WARN 为历史 OPEN-DEBT 的已批准延期与 V3.1 edition 试验窗口提示。
- R36 台账：93 个 ID 承载根因（FINDING 62 + 编号 SUGGESTION 25 + CLAIM 6），处置 ① 44 / ② 0 / ④ 16 / ⑤ 0 / ⑥ 0 / FP 3 / SUGG 30；`未闭环 = 0`；`certify 新准入 = 0`（三轮 certify 的 STILL-BROKEN 均为同根修复不完整，不是新根因；最终 44/44 + 16/16 CLOSED）。
- 100% 复验：`certify/round36-fact.md` 44/44 CLOSED、`certify/round36-reader.md` 16/16 CLOSED；pass1/pass2 回执保留为历史证据。
- 文档六门：anchor 110/110 zero-def=0 multi-def=0；citation `blocks=69 checked=29 failed=0 skipped=35 warned=5`；structure/ref/style（error=0）/density 通过；doc-code-map 54/54；`--stale-lines a0261fdd1 --strict` PASS。
- 宿主：minix-kernel --features mock 866/0/8；minix-boot 185/0；boot-shim 146/0；minix-elf 27/0；aarch64 kernel-image `cargo check` 完成。
- QEMU：`test-sysboot.sh` PASS（x86 wire v10）；`test-riscv64-uboot-shim.sh` 6/6 PASS（`BootFileTable` 类型化）；`test-rt-birth-aarch64.sh` 在同一处 user-chain Data Abort 失败，`git stash` 对照改动前状态复现同一地址/ESR，确认是既有 A2 遗留（`01-todo.md` 的 rt-birth 深跑附注），不是本轮因果；本轮 aarch64 bump 由“到达 entering scheduler”覆盖。
- 独立 meta：`meta/01-boot-shim-bootstrap-r36.md`，总体 IMPROVED / 运行质量 NOT PASS（抽样一致性 93%）；meta FAIL 记为流程缺陷（同根残项射程、裸 ID 冲突、D2-05 re-litigation 的初判错误），不阻断文档关闭（§5.4 / §6A）。

### 未解决事项（不阻断，已登记）

- `suggestions.md` R36 段：33 条面板建议 + 3 条降级结构项 + 2 条设计 P3；含 `DESIGN-D2-03`（startup 死构造器）、`DESIGN-D2-09`（静态 scratch）与读者结构项。
- `DESIGN-D2-05` 判 re-litigation（D-064 路线 B），维持安装型全局；不立 `R36-design-batch`。
- `V3.6 遗留声明`：本轮按 V3.4 口径关闭，V3.6 的减法门与“修复点=制品类别射程”门未作为轮门执行；`BLIND-SPOTS.md` B-68 已登记。
- 范围外已知缺陷：aarch64 rt-birth 用户链 A2 遗留（`01-todo.md`），不在本轮 doc-code-map 修复射程。

### 限制（照旧 + 本轮新增）

- `CONVERGED` 的含义是“确认轮口径下没有 open 项 + 无未销账债务”，不是零未知缺陷保证。
- 面板与三次 certify 均使用同一模型；无跨模型独立复核。
- 工具已知缺口：面板裸 finding ID 与历史轮重名，R36 在 DESIGN-D2 行 action 补“用户批准 2026-11-21（R36 代裁授权）”以通过历史轮 ② 检查；这是轨迹口径 workaround，下一版应把 ledger 键改为 (round, id)。
- 8 条 fact 角色 SUGGESTION 在 suggestions.md 补了 `R36-FACT-*-S*` 编号，但不进台账分档。

### 重开条件

- 出现新的 P0/P1 事实或行为缺陷证据；
- `R36-design-batch` 不存在（D2-05 已判 re-litigation）；若后续独立设计批改变基线，需重新固定基线并重跑受影响门；
- 用户明确要求重开。


## 重写基线（2026-11-21）

- 旧 v3 原文归档到 `rewrite-notes/01-stage-kernel/draft/`；现行文件按“01=交接载荷、02=内核镜像”重写。
- 旧 R36 的 CONVERGED 作废；不跨重写比较轮次。新基线为下一轮（D01 R37 / D02 R5）的 L0。
- 行数：01 = 2673，02 = 1840；比例 1.45。
- 旧 OPEN-DEBT/follow-ups/suggestions 仍全 `[CLOSED]`；重写不改变旧债证据。

## 重写后 review 状态（2026-11-21）

- 轮次：D01 R37 / D02 R5，轮型 `full`，轮结果 `ROUND-CLOSED`。full 轮无 CONVERGED 声明权，故不写 CONVERGED。
- 本轮发现全部处置：未闭环 0；certify 新准入 0（四份独立回读）。
- 未达 CONVERGED 的原因不在本轮发现：G11 code-publication 的存量注释、G13 tutorial-rubric 的 6/6 缺项、以及 `D01/D02-EXTERNAL-D107` 外部 todo 哨兵；三项均不由本轮文档重写引入，也不在本任务允许的修改面内。
- 本轮 `os/`、`minix3/` 零改动；QEMU 未重跑。下一代码批次需重新固定基线。
