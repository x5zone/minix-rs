# 01-review-trajectory — boot-shim 正文篇深度 Review 轨迹

- **对象**：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（3,073 行版）
- **基线哈希**：sha256 `fda90e00…5b776`；基线 HEAD `51af6a608`
- **执行方**：zcode 深度 review（full-review + 卓越性 + 轨迹复盘），只审不修
- **判定**：NOT CONVERGED（P0=14 / P1=28 / P2=64；Layer 1 FAIL；Layer 2 PARTIAL）；报告见 [01-review.md](01-review.md)
- **本文件来源**：抢救自 gitignored 目录——轨迹时间线原在 `.review/zcode/01-stage-kernel/01-boot-shim-bootstrap/trace/00-timeline.md`、评审前线索原在 `.zcode/plans/plan-sess_bd028084-cb33-4967-acdc-48ba4bcbd8c8.md`，两者均不入库、面临丢失；`01-review.md` 交付时计划承诺的 `01-review-trajectory.md` 未产出（03/04/05 均有轨迹文件，唯独 01 缺）。2026-11-03 由 `prompt/WORKFLOW-OPEN-ITEMS.md` W-16（评审盲区元问题）登记触发抢救入库。原始长日志仍在 `.review/.../trace/logs/`（不入库，以时间线摘要为准）。

## 1. 评审前探查线索（抢救自 review 计划）

- **既有 review 全部过期**：`.review/qoder/01-stage-kernel/01-boot-shim-bootstrap/scan.md` 写于旧版仍称文档 294 行；`.review/claude/...` 针对旧路径与 1,965 行版本——本轮必须重做。
- **初步探查确认的 01-boot-shim-bootstrap.md 文档-代码漂移**（执行时逐条取证）：`main.rs:65` 实际调用 `prepare_and_enter_kernel_image(1024)`，而 §3.2/§4.2 仍把 `UefiBootShim::prepare_boot` 当 x86_64 生产路径；§4.1 表与同节正文对 x86_64 路径自相矛盾；§3.2 称 OpenSBI shim 无调用方，实际 `RiscvUbootBootShim::prepare_boot` 由 `uboot_main.rs:333` 调用；`build_kernel_info` 实际 10 参（含 `param_buf`），§4.4 仍写 9 参 + 固定 `&[]`；§6.1 多个里程碑字符串已不存在；§5.2.1 测试计数与实测不符。
- **正文残留**：L1536 `TODO 这个是TODO是交互式文档类TODO，我读不懂？？`。
- **Gate H 快照早于改版**：`01-design.v2.md` / `01-outline.v3.md` 均早于正文改版，按 Step 0.3 本轮重推。

## 2. R0 基线与预检

| 轮次 | 步骤 | 命令/动作 | 输出摘要 | 等级 | 判定 | 关联 |
|---|---|---|---|---|---|---|
| R0 | 基线 | `sha256sum rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `fda90e00…5b776` | L1 | 记录 | — |
| R0 | 基线 | `wc -l` | 3073 | L1 | 记录 | — |
| R0 | 工作树 | `git status --porcelain` | `M AI-chats/daily.todo.md`（既存） | L1 | 记录 | — |
| R0 | 最新提交 | `git log -1` | `51af6a608 …` | L1 | 记录 | — |
| R0 | 初始化 | `tools/review-init.sh zcode … --size-adaptive` | 生成 `.review/zcode/01-stage-kernel/STATE.md`；推荐 4 rounds | L1 | PASS | — |
| R0 | design 预检 | `tools/design-coverage-check.sh 01-stage-kernel` | 01 篇 outline/outline-review/design 均 ✅ | L1 | PASS | Gate H.1/H.6 |
| R0 | 4 条 ls | `ls .design/01-{outline,outline-review,design}*.v*.md` | outline v1–v3；outline-review v1–v3；design v1–v2；无 design-final | L1 | 旧快照存在，需重推 | Gate H |
| R0 | 关联代码 | `tools/doc-code-map.sh … --check` | 41 路径，40 存在，`handoff.rs` 缺失 | L1 | **1 缺失** | P0-fact-09 |

## 3. Gate H 快照重推

| 轮次 | 步骤 | 动作 | 产出 | 等级 | 判定 |
|---|---|---|---|---|---|
| R1.1 | design-structure | 从 C 源码 + OS 理论 + 当前 Rust 独立推导 | `.review/.../design-structure.md` | L2 | 完成 |
| R1.1 | outline v4 | 按规范源 7 章骨架 + 每节知识点/证据/反向推演 | `.design/01-outline.v4.md` | L2 | 完成 |
| R1.1 | outline-review v4 | 4 维自审 + doc↔outline 偏离 D1–D9 | `.design/01-outline-review.v4.md` | L2 | P0（outline 自身）=0；正文偏离 9 处 |
| R1.1 | design v3 | Ch1–Ch4 + v3 vs v2 差异矩阵 | `.design/01-design.v3.md` | L2 | 完成 |

## 4. 覆盖率与机械门

| 轮次 | 步骤 | 命令 | 输出摘要 | 等级 | 判定 | 关联 |
|---|---|---|---|---|---|---|
| R1.2 | 覆盖率 | `coverage-extract.py kernel … --doc-file 01-boot-shim-bootstrap.md` | 192 语义域；doc 61/192；Rust 15/192；46 有文档无Rust | L1 | PASS | Gate A |
| R1.2 | 语义列 | 61 行逐条填写 + `--check-filled` | exit 0；0 残留 | L1 | PASS | Gate A |
| R1.2 | 锚点 | `anchor-resolve.sh --check` | anchors=98 resolved=98 zero-def=0 multi-def=0 | L1 | PASS | Sync-5 |
| R1.2 | 路径 | `doc-code-map --check` | 1/41 缺失 | L1 | FAIL | P0-fact-09 |
| R1.2 | 文风 | `doc-style-lint.sh <doc>` | error=0 | L1 | PASS | Sync-1 |
| R1.2 | 章节引用 | `AWK=gawk doc-ref-check.sh --check` | 1 处「第 8 号条目」（自然语言误伤） | L1 | 卫生项 | P2-docStyle-01 |
| R1.2 | 承载门 | `doc-code-density-check.sh --check` | 4 个可判章均有码/命令；阻断项通过 | L1 | PASS | 通用门 #9 |

## 5. 章节块审查（6 个只读代理 + 主会话复验）

| 轮次 | 代理 | 范围 | 主要产出 | 主会话复验 |
|---|---|---|---|---|
| R1.3 | Block A | 01-boot-shim-bootstrap.md §1–§2（L1–1250） | 28 条事实核验、24 块代码 diff、16 条复算；P0：§2.4.2 模块回收 | `vm/main.c:482-520` 重放 |
| R1.3 | Block B | §3（L1251–1962） | 设计断言核验、17 字段处置、trait 三问；P0：OpenSbi 名/调用方、reserved 腿、cfg、pt_alloc release | `opensbi_helpers.rs:211`、`uboot_main.rs:333`、`lib.rs:48`、`pt_alloc.rs:96` 重放 |
| R1.3 | Block C | §4（L1963–2678） | 实现断言核验、代码块 diff、调用链；P0：生产路径、Step 2c、256 常量、build 签名 | `main.rs:65`、`lib.rs:936-1085`、`globals.rs:313`、`lib.rs:48` 重放 |
| R1.3 | Block D | §5–§7（L2679–3073） | 测试计数、QEMU 标记、诊断字符串、路径；P0：28/32、伪里程碑、handoff.rs、M6 | `cargo test` 53/65、`grep` 0 命中、`ls handoff*` 重放 |
| R1.3 | 代码维度 | boot 5 crate | 错误处理/unsafe/trait/死代码/非法态；P1：EBS 重试分配、U-Boot 表长度、from_utf8_unchecked | `uefi_helpers.rs:837`、`uboot_main.rs:274`、`lib.rs:1176` 重放 |
| R1.3 | 测试审计 | 测试与 QEMU | 22 文件计数、五维、脚本静态审计；P0：M6 假覆盖、M4 不完整 | `paging.rs` 16 条、`store_before_alloc_pin.rs` 重放 |

## 6. 代码与测试实跑

| 轮次 | 命令 | 输出摘要 | 等级 | 判定 |
|---|---|---|---|---|
| R1.4 | `cargo check -p boot-shim --features test-all` | exit 0；6 warnings | L1 | 记录 |
| R1.4 | `cargo clippy -p boot-shim --lib --no-deps -- -D warnings` | exit 101；存量 7 条（非本轮引入） | L1 | 记录 |
| R1.4 | `cargo test -p boot-shim --features test-all --lib` | 53 passed / 0 failed | L1 | PASS |
| R1.4 | `cargo test -p minix-boot -p minix-elf` | 65 passed / 23 passed | L1 | PASS |
| R1.4 | `cargo test -p minix-kernel --features mock` | 857 passed / 0 failed / 8 ignored；集成 3+2+1 | L1 | PASS |
| R1.4 | `cargo test -p minix-arch --features runtime-window --test …` | 2+8+7 passed | L1 | PASS |
| R1.4 | `tools/unsafe-audit.sh --diff` | total=0 with_safety=0 bare=0 | L1 | PASS |
| R1.5 | `SKIP_BUILD=1 check-layout.sh all` | CHECK-LAYOUT=PASS（all） | L1 | PASS |
| R1.5 | `grep -rn 'pt_alloc registered, entering kernel arch_boot\|handing control to kernel arch_boot' os` | 0 命中 | L1 | FAIL |
| R1.5 | `grep -n 'MARK3' test-shim-bootmarks-aarch64.sh` | 50/157/165；脚本判 3 条 | L1 | FAIL |
| R1.5 | `sed -n '141,145p' test-cmd-smoke.sh` | 第三条 `entering kernel image v1` | L1 | FAIL |

## 7. 跨文档与 Doc Code Sync

| 轮次 | 检查 | 命令/动作 | 结果 | 等级 |
|---|---|---|---|---|
| R1.6 | Sync-1 | `rg "static mut" <doc>` | 0 | L1 |
| R1.6 | Sync-2 | `doc-code-map --check` | `handoff.rs` 缺失 | L1 |
| R1.6 | Sync-3 | 抽查 5 个常量 | 全部命中 C/Rust | L1 |
| R1.6 | Sync-4 | `grep TODO` | L1536 残留 | L1 |
| R1.6 | Sync-5 | `anchor-resolve --check` | 98/98 | L1 |
| R1.6 | Sync-6 | `grep 'os/kernel/src/'` | 48 处；无裸前缀 | L1 |
| R1.6 | Sync-7 | 文档 28/32 vs 实测 53/65 | 偏差 >50% | L1 |
| R1.6 | 跨文档 | 02–05 各篇回指与 01-boot-shim-bootstrap.md §7 路径 | 可解析；其 §7.3 漏文件 | L1 |

## 8. 卓越性与走查

| 轮次 | 动作 | 产出 | 判定 |
|---|---|---|---|
| R2 | 文档 12 节 | `structure.md` | 结构层 PASS |
| R2 | 教学构件/反向推演/死代码/trait/非法态/跨架构 | `excellence.md` | Layer 2 PARTIAL |
| R2 | 学生型走查（只读子代理，禁外部知识） | 8 题（6 题可只凭本文答对）；32 节三态：①27/②5/③0；红旗：11 处过程编号、20+ 元话语、7 处黑话、6 组代码块未解码、4 处清单未兑现 | 补入 P1-docStyle-02/03/04、P1-fact-20、P2-fact-38、P2-docStyle-02 |

## 9. Gate G 与报告

| 轮次 | 动作 | 输出 | 判定 |
|---|---|---|---|
| R3 | `verify-check.py --sample-ratio 0.2 --seed 20261102` | Gate 0 PASS；8 证据块齐；抽样 21；一致 20；部分一致 1；95.2% | CONCERN（同 agent） |
| R4 | 写 `01-review.md` | 14 P0 / 28 P1 / 64 P2；Layer 1 FAIL；Layer 2 PARTIAL；NOT CONVERGED | 完成 |
| R5 | 轨迹复盘 | 覆盖遗漏 1（bootface_common 补审）；假阴性探针 3 抽 2 中；结论缺陷 1（P2-test-06 降级） | 完成（详见 01-review.md §13） |

## 10. 复盘补审

| 项 | 动作 | 结果 | 判定 |
|---|---|---|---|
| 覆盖遗漏 | 补审 `bootface_common.rs` 的 `be_cell/in_memmap/table_magic_ok/fdt_table_pa/fdt_bootargs/param_landing` | 与 bootface/bootface_a64 调用一致；新增 3 个 P2（P2-code-14/15/16）；未发现新 P0/P1 | 补审完成 |
| 假阴性探针 1 | 01-boot-shim-bootstrap.md §2.6.4 重读 `pagetable.c`/`protect.c` | 发现 `prot_init` 整表重建未覆盖 → P2-fact-03 | 命中 |
| 假阴性探针 2 | 01-boot-shim-bootstrap.md §4.5 重读 `store_kernel_info`/`globals.rs` | reserved 正向深拷/精确容量未测 → P2-test 组 | 命中 |
| 假阴性探针 3 | 01-boot-shim-bootstrap.md §6.2 重读 panic 路径 | 无新发现 | 通过 |

## 11. 结论

- 本轮 review 的主结论：文档概念骨架成立，但存在 14 个 P0 事实/测试缺陷；NOT CONVERGED。
- 轨迹自身的最弱环节：QEMU 未实际点火、同 agent Gate G、`bootface_common.rs` 首轮未逐行审。三项均已在报告 §13/§14 披露，并做了可做的补审。
- 本文件为入库抢救副本；`.review/` 与 `.zcode/` 原件随 gitignore 不入库，若原件被清理，以本文件 + [01-review.md](01-review.md) §13 为准。
