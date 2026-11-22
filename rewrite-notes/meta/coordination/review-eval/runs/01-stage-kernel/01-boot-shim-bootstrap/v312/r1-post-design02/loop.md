# 01-stage 01 R1 loop（V3.12，post-DESIGN-02）

schema: V3.12 from R1
edition: V3.12
轮型: full
文档: rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
基线 commit: cdd0d1c777042b048bd9fdfac3e5102b95dac869
文档 sha256: 996ff161d02ef867151e780adb7243e49e7dc249b5b161d6d0c5ed229b5f22ec
行数: 2946
单元: 48（6 章 + 42 节），0 冻结
run 目录: rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r1-post-design02
范围: `SCOPE.md`；关联代码 `related-code.txt` 49 个文件
学习范围: `meta/review/scope/01-stage-kernel/01-boot-shim-bootstrap.yaml`（me-agent 代确认，待作者追认）
历史: 旧 R1（`v312/` 根下的 panel/defect-ledger/loop/metrics）被 D-119 Manuscript 设计批按 §四.6 中止；旧 48 FINDING / 40 根因 / 单元证据只作历史，不继承
L0.9: doc-publication 0 新增 / code-publication 0 新增（49 个 .rs）/ todo-ledger open=0 / tutorial-rubric 0 缺项
机械门: anchor 87/87；ref OK；density OK；citation failed=0；style error=0
面板: 6 角色 fresh；读者 A/B/C 只拿正文+画像+§4.0e；D2 走 D-024 D1 停跑回退；P-REFUTE 强制

### §0 基线

- 本轮是 DESIGN-02（b）落地后的新 R1；旧 R1 的基线（doc 2,979 行、sha `75c04e8…`）已作废。
- 新基线：doc 2,946 行，sha `996ff161…`，commit `cdd0d1c77`；48 单元全部未冻结。
- 修复后文档：2,951 行，sha `f707a4e5a38b5a4544592231ea724215a06bc1179fd4f25ee41db47afc1f5e3d`，
  commit `bb3b6e56a`（本轮修复批）加最终 P3 文档批（§3.7 v1 链、§4.0 落地区措辞、删除 §1.4 过程编号段）。
- `baseline --evidence r1-v312/full-r1.md` 已记录 R1；scope 保持作者/代确认内容。
- 关联代码清单由 `doc-code-map.sh --format=plain` 重新生成，49 个文件（旧清单漏了 `ap_early_entry.rs`、`proc_table.rs`、`trap_api.rs`）。

### §1 面板

本轮 R1 是 DESIGN-02（b）落地后的 full survey，文档 2,946 行，启动链高风险，按
`review-panel.md` 表 A 启 6 角色；每个角色 fresh、互不可见。

- `P-FACT`：全文 + `related-code.txt`（49 个 Rust 文件）+ D-024/D-047/D-062/D-116/D-117/D-119
  一行裁决；不读历史面板/台账/结论。
- `READER-A`：全文 + 读者画像 + §4.0e 十条；不读源码、其他文档、学习范围表、历史评审。
- `READER-B`：同 A，角色为结构编辑。
- `READER-C`：同 A，角色为因果一致性；R1 不是 G18，不给学习范围表。
- `DESIGN-D2`：Ch3/Ch4 + 49 个关联 Rust 文件（按锚点）+ `trait-audit.json`
  + AGENTS 约束 + D-047/D-062/D-110/D-111/D-112/D-119；D1 停跑，记
  `VARIANCE: D-024`；不读作者解释/历史评审/其他面板输出。
- `P-REFUTE`：全文 + 关联源码 + `SCOPE.md` + 相关 DECISIONS 一行裁决；
  不读作者结论/历史评审/其他面板输出。

输入隔离：读者 A/B/C 不拿学习范围表；面板子 agent 只写
`tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/v312-r1-post-design02/<role>.md`，
不写仓库文件；面板前后各跑一次 `git status --porcelain`，有新改动即丢弃重跑。

事件（`event plan` 于 2026-11-23，doc sha `996ff161…`，target commit `df3441a57…`）：

- P-FACT `REV-20261123-36BA35EA`；READER-A `REV-20261123-B792E52E`；
  READER-B `REV-20261123-8899DFE0`；READER-C `REV-20261123-D9196E35`；
  DESIGN-D2 `REV-20261123-80E27F98`；P-REFUTE `REV-20261123-92A2BD16`。
- 映射表：`tmp/review-loop/01-stage/evidence/y01b-event-map-role.csv`；
  面板完成后由 `tmp/review-loop/01-stage/bin/complete-panel-events-post.sh` 逐个
  `event complete`，报告副本加调度绑定 `DOC` 行后写入 `panel/round1/`。
- 面板派发前 `git status --porcelain`：仅 `events.csv` 由本步骤 `event plan` 修改；
  两个未跟踪文件保持不变（`tmp/review-loop/01-stage/evidence/y01-post-design02-pre-panel.txt`）。
- 面板提示词：`tmp/review-loop/01-stage/panel-prompts-post/{fact,reader-a,reader-b,reader-c,design-d2,refute}.md`。
- 独立设计批复核：`tmp/review-loop/01-stage/panel-prompts-post/design02-verifier.md`，
  报告写 `tmp/review-loop/01-stage/certify/design02-verify.md`，与面板并行、不共享结论。
- 输入修正（05:10）：派发时 `related-code.txt` 仍是 Phase 0 的 48 条旧表，缺
  `os/servers/rs/src/trap_api.rs`（文档 §1.4 引用其 `:223-230`、`:430`）。已用
  `doc-code-map.sh --format=plain` 重新生成为 49 条，并请 P-FACT 补查该文件；
  P-FACT 的补查若改变结论，记入本轮台账。其余角色按派发时的 48 条输入完成，R2 用 49 条全套。

### §2 发现

- 面板 6 角色：P-FACT 1、READER-A 16、READER-B 4、READER-C 6、DESIGN-D2 9；
  P-REFUTE 0 FINDING，6 条 CLAIM（2 条阻断、4 条非阻断/反证失败）。
- 合计 36 FINDING：P0 0 / P1 3 / P2 18 / P3 15；另有 26 条 SUGGESTION。
- P1：READER-C-01（v1 接收侧 store 入口与 §4.4 概括矛盾）、READER-C-02（v1 解码落盘与
  `store_kernel_info` 包装调用两个事件未分开）、DESIGN-01（同一处“统一接收侧”的三处不实表述）。
- 原始报告在 `panel/round1/`；`INDEX.md` 由 `panel-index.py` 生成。
- 事件：6 个角色事件已 `completed-pass`（doc sha `996ff161…`）；逐单元最终事件在修复/回读后另立。

### §3 归因

- 根因合并：36 panel FINDING → 32 个面板根因；P-REFUTE 观察与 DESIGN-02 verifier 的
  QEMU 载体回归各占独立根因，台账共 36 个 `R1-*` 根因。
- 台账：`defect-ledger.md`；面板 36 个 ID 全覆盖，P-REFUTE 6 条在 `## P-REFUTE 附加`。
- 关键合并：`READER-C-01`/`FACT-01`/`DESIGN-01` 合并为 R1-01；`READER-B-02`/`READER-A-05`
  合并为 R1-12；`READER-A-01`/`READER-A-02` 合并为 R1-15。
- 唯一 certify 新准入：`CERTIFY-01`（三个 `test-higher-half*` 载体 E0061，DESIGN-02(b)
  批次引入的测试固定装置回归），根因 R1-36。

### §4 修复

- 文档批（`bb3b6e56a`）：§1/§2/§3/§4/§5/§7 的术语、计数、指针、门链、存储位置与重复段；
  重点是把“接收侧统一走 `store_boot_report`”拆成 `land_and_store` 下的两个入口，并把 v1
  “解码落盘”与 `store_kernel_info` 包装调用分成两个事件。
- 代码批（同 commit）：`validate_view` 改收 `Option<RamAuthority>`；删除
  `KernelInfo::validate_before_tables`、`BootReport::validate`、`param_ptr`、两份
  `test_kernel_info` 脚手架；三个 `test-higher-half*` 改为 `BootReport` + 三参数
  `arch_boot`；同步 `boot-shim`/`kernel`/`kernel-image` 注释与 `README`/`Cargo.toml`。
- 测试：minix-boot 188/0；boot-shim 136/0；minix-kernel mock lib 866/0/8（全部二进制 0 failed）；
  `cargo check -p minix-kernel`、`kernel-image` riscv64/aarch64 构建通过。
- QEMU：三个 `test-higher-half*` 载体 release 构建后实跑，三支 PASS；日志
  `tmp/review-loop/01-stage/evidence/y01b-qemu-*.log`（日志头记录了运行时的 HEAD，
  代码修复在随后的 `bb3b6e56a`；两处仅注释差异，见 code verifier）。
- 建议：26 条 SUGGESTION 全部在台账 `## 建议` 处置（修 / FP / 部分修）；未改项的理由已写。

### §5 复验

- 设计批独立 verifier（2026-11-23 05:12）：**FAIL**。`certify/design02-verify.md`。
  两项阻断：(a) §4.4 旧符号 `validate_impl_inner` 与“v1 腿统一走 `store_boot_report`”的
  错误概括，代码/Cargo 注释同步债；(b) 三个 `test-higher-half*` QEMU 载体 E0061。
  两项均在 D-119 授权内，待面板报告到齐后与面板发现一起修复、再复核。
- 面板与修复的单元级复验待本轮后续填写。

### §6 指标

- `metrics.md`：发现总数 36、唯一根因数 36、重复合并 4、certify 新准入 1、自引入 1、
  未闭环 0；轮结果待独立复验与关账后写。

### §7 状态

待本轮填写。修改轮最多写 `ROUND-CLOSED` / `CONVERGENCE-CANDIDATE`，不写 `CONVERGED`。

### §8 给 meta-review 的不确定点

待本轮填写。
