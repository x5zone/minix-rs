# 01-stage 01 R3 loop（V3.12，结构轮）

schema: V3.12 from R3
edition: V3.12
轮型: full（结构轮；第 3 个修改轮）
文档: rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
输入 commit: 9f1327187（`98b7cdaf…`；R2 终态 `cbc105c7…` 在 `a6a94b942`）
结构轮输入 sha256: 98b7cdaf4864d67d3da615bcab4fc9c18f9e7bb2f41f72d634cd47cd1a2333ea
结构轮输入行数: 3049
修复后 sha256: 67440cd885aad0e808cd5d76905323872b08b1c35849566b976c123d84dff6e8
修复后行数: 3085
单元: 48（6 章 + 42 节）；面板后 29 个单元因正文变化解冻并清空旧 review，R3 复验后重新冻结
run 目录: rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r3
范围: `SCOPE.md`；关联代码 `related-code.txt` 49 个文件
学习范围: `meta/review/scope/01-stage-kernel/01-boot-shim-bootstrap.yaml`（me-agent 代确认，待作者追认）
历史: R1/R2 均 `ROUND-CLOSED`；R3 是结构轮，输入是 R2 冷读四个硬阻断
L0.9: doc-publication 0 新增 / tutorial-rubric 0 缺项 / code-publication 0 新增
机械门: anchor 87/87、code-map 49/49、ref/style/pub/rubric/density/structure 全绿、citation failed=0

### §0 基线

- R3 输入：doc 3,049 行，sha `98b7cdaf…`，commit `9f1327187`。
- 结构改写：§1.1.1 现役字段速查、§3.2.3.1 wire 字节布局、§3.4.1/§3.2.1 历史标注、
  §4.1 端到端路标图、§4.2 代码节选读法、§1.0 主计数。
- 48 单元全部进入 R3 full survey；结构改写明细见 `STRUCTURAL-REWRITE.md`。
- 修复后 doc 3,085 行，sha `67440cd8…`；面板后 29 个单元因正文变化解冻，复验后重新冻结。

### §1 面板

- 6 角色 fresh，报告写 `panel/round3/` 并入库；提示词 `tmp/review-loop/01-stage/panel-prompts-r3/`。
- 事件：P-FACT `REV-20261123-9D52DEAE`、READER-A `2770EC55`、READER-B `730DC86A`、
  READER-C `3E5AAC6F`、DESIGN-D2 `EB0132C2`、P-REFUTE `EA2DEB65`，均 round 3，已 completed-pass。
- 面板合计 22 个 FINDING（P2 10 / P3 12）；另 P-REFUTE 1 个 P2 观察与 2 条非阻断观察；
  独立文档复验又发现 3 个本轮引入的问题（NEW-01..03，已修）；建议 37 条（READER-A 10、READER-B 16、READER-C 9、FACT 2）。

### §2 发现

| 来源 | FINDING | 严重度 | 主题 |
|---|---|---|---|
| READER-A | 8 | P2×4 / P3×4 | 框 1/框 2 首用、四类/五类、22 与字段链、TTBR1/Sv39/satp、store no-op、自指指针、“标题”指代、§5.1 计数 |
| READER-B | 5 | P3×5 | 框 1/框 2（重复）、IS/RS/PM、“四个字段”、VmBootHandoff 版本指针、空键 C 行为缺证据 |
| READER-C | 2 + 附带 2 | P2×2 / P3×2 | 删除扫描两支模型与中段误删冲突、“只能由发送侧解析 ELF”过宽、三架构直核面、I3 直核所有权 |
| P-FACT | 3 | P2×2 / P3×1 | §4.3 复算命令漂移、exec.c 行号锚点错一行、SourceRecord 字段清单不完整 |
| DESIGN-D2 | 4 | P2×2 / P3×2 | authority 双实参、kmain 参数与全局双句柄、Serializer 死计数器、五处过期注释 |
| P-REFUTE | 1 + 2 观察 | P2×1 | I4 总起句未限定接收侧；check-then-write 前提；BootReport 形状收敛不等于完整 |

### §3 归因

- 根因 20 个（R3-01..R3-20）：术语首用无定义、计数口径混用、store 语义冲突、指针落空、
  概化断言、C 删除模型不完整、I3 直核所有权缺失、I4 发送侧范围、视图完整性、§5.1 密度、
  复算命令漂移、手写行号锚点、字段清单不完整、authority 双实参、kmain 双句柄、死计数器、
  过期注释。
- 无 ⑤（设计裁决）项；DESIGN-01..04 全部是可立即落地的可信链/删除/注释修复，不改变外部行为。
- 无新增 defer；无 ARCH 提案。

### §4 修复

- 文档 18 组修复：见 `defect-ledger.md` 主表（22 FINDING + 3 观察全部 ①）。
- 代码 4 组修复：
  - `arch_boot_finish` 在 `store_boot_report` 前加 authority↔source_records 绑定断言；
    `minix_boot::authority_has_source_record` + 单元负控。
  - 生产 `kmain` 用 `ptr::eq` 绑定参数与 `KERNEL_INFO`；删除临时 Evidence capture 块与
    “can be deleted”注释；字段读取继续走 `.bss` 全局。
  - `Serializer` 删除 `modules_n`/`platform_n`/`param_n` 三个死计数器。
  - 五处过期注释改为当前 `BootReport`/六分区/`validate_requiring_sources` 形状。
- 顺带采纳的高价值建议：§1.1.1 类型速查、§1.0 第一次读路线、§3.6 三行调用图、
  §4.4.1 时间序表、§5.1 I3 行拆分、§2.3 空键推演、§7.5 DTB 路径、§3.3 审计关系。

### §5 复验

- 宿主：`minix-boot` 190/0（新增 1 条 authority 负控）、`boot-shim` 136/0、
  `minix-kernel --features mock` 866/0/8、xtask 顺序锁 1/1；`cargo check -p minix-kernel` 通过。
- QEMU：x86_64 UEFI、aarch64 UEFI、riscv64 U-Boot shim、riscv64 `bootface` 四门 PASS
  （日志 `tmp/review-loop/01-stage/logs/r3-*.log` 的判据行；命令退出码另行记录为 0）。
- 文档门：anchor 87/87、code-map 49/49、ref/style/pub/rubric/citation/density/structure 全绿。
- 独立文档复验与代码复验：`certify/round3-doc-verify.md` 与 `certify/round3-code-verify.md`
  均 `VERDICT: PASS` + `RECHECK: PASS`；汇总见 `fix-verify.md`（tmp 原件同内容）。
- 关账：48/48 冻结、`t1_pending=t2_pending=reopened=stale=0`、`event verify --fresh` OK、
  `v312-closure-check --round 3` PASS。

### §6 指标

见 `metrics.md` 的 `## R3` 块：30 个发现 ID（22 主 FINDING + 2 附带 + 3 观察 + 3 复验新发现）合并为 28 行台账、全部 ①；FP 3（P-REFUTE CLAIM 01/02/06），
未闭环 0，轮结果 `ROUND-CLOSED`。

### §7 状态

`ROUND-CLOSED`。第 3 个修改轮（结构轮）已用尽；下一步是确认轮（`begin-confirmation`），
不写 `CONVERGED`。

### §8 给 meta-review 的不确定点

- READER-B 的 16 条 SUGGESTION 在 `panel-index` 里计为 0（它用单个 `## SUGGESTION` 分组，
  子项是加粗 ID）；人工计数为 16。建议 `panel-index.py` 增加对 `**SUGGESTION <ID>**` 形态的识别。
- R3 修复后 29 个单元解冻；`review_rounds` 按规则清空，确认轮的 `--confirm-review` 会同时写入
  `review_rounds` 与 `post_freeze_rounds`，因此每单元可达到 ≥2 次 review。
- DESIGN-D2 提出的 `kmain` 参数可信性：本轮选择“保留参数 + `ptr::eq` 绑定 + 全局读”，
  没有改 `HigherHalf` 的 ABI 合同；若后续证明参数在目标栈上仍不可信，需要作者裁决入口合同。
