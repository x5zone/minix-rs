# 01-stage 01 R1 loop（V3.12）

schema: V3.12 from R1
edition: V3.12
轮型: full
文档: rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
基线 commit: 64f76309fe3393f9884f797335e582412f18480f
文档 sha256: 82271968c81aae8630a70a886e80fb031546d79e80f84cf73df9a88c1842fe90
行数: 2983
单元: 48（6 章 + 42 节），0 冻结
run 目录: rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312
范围: 见 `v312/SCOPE.md`；关联代码 48 个文件
学习范围: `meta/review/scope/01-stage-kernel/01-boot-shim-bootstrap.yaml`（已由 me-agent 代确认，`confirmed_by=me-agent-deputy`，代裁待追认）
技能调用: `review-loop`、`full-review`、`style-bible`（Skill 工具实际调用）
VARIANCE: ZCode 运行时没有 `review-process-skill`/`review-doc-skill` 细粒度技能；用 `full-review` 伞形技能 + 规则源文本过渡，面板子 agent 只拿 `review-panel.md` 第四节 brief
遗留状态: `.review/zcode/01-stage-kernel/STATE.md` 预检 FAIL（引用不存在的 02-review.md/05-review.md，Open 列表与 scan.md 不一致）；该文件是旧 review 系统产物，V3.12 不使用，记录不修历史，避免污染本轮账户
L0.9: doc-publication 0 新增 / code-publication 0 新增 / todo-ledger open=0 / tutorial-rubric 6 章 0 缺项（TR-0 五章已补）
机械门: anchor 90/90；ref 可解析；density 通过；citation failed=0/warned=4；doc-code-map 48/48
对抗审查: `tmp/review-loop/01-stage/audit/plan-adversarial.md` → FAIL(10)；PA-01..PA-10 处置见
`tmp/review-loop/01-stage/PLAN.md` §15，从 y01 起生效

### §0 基线

- HEAD `64f76309fe3393f9884f797335e582412f18480f`，分支 `rewrite`。
- D01 2,983 行，sha256 `82271968c81aae8630a70a886e80fb031546d79e80f84cf73df9a88c1842fe90`。
- 48 个单元，`review-units.py baseline --evidence r1-v312/full-r1.md` 已记录 R1。
- 本级新增的 5 段 TR-0 问题入口：§1、§2、§3、§4、§5 章首；§6 原有问题入口不动。
- 旧 V3.11 证据保留在父目录；V3.12 全部写在本目录。

### §1 面板

本轮 R1 是 full survey，文档 2,983 行（≥500），启动链高风险，按 `review-panel.md` 表 A：
读者 A/B/C 各自独立读全文；P-REFUTE 强制启动；设计角色因 D-024 停跑 D1，只跑 D2，
采用「文档 Ch3/Ch4 主张 ↔ 源码事实」回退。

- `P-FACT`：文档全文 + `related-code.txt`（48 个 Rust 文件，约 54,933 行，按文档锚点选择性读）
  + D-024/D-047/D-062/D-116/D-117 一行裁决；不读历史面板/台账/结论。
- `READER-A`：文档全文 + 读者画像 + §4.0e 十条；不读源码、其他文档、学习范围表、历史评审。
- `READER-B`：同 A，角色为结构编辑。
- `READER-C`：同 A，角色为因果一致性；R1 不是 G18，不给学习范围表。
- `DESIGN-D2`：文档 Ch3/Ch4 + 48 个关联 Rust 文件（按锚点）+ `trait-audit.json`
  + AGENTS 约束 + D-047/D-062/D-110/D-111/D-112 一行裁决；D1 停跑，记
  `VARIANCE: D-024`；不读作者解释/历史评审/其他面板输出。
- `P-REFUTE`：文档全文 + 关联源码 + `v312/SCOPE.md` + 相关 DECISIONS 一行裁决；
  不读作者结论/历史评审/其他面板输出。

输入隔离：读者 A/B/C 不拿学习范围表（学习范围表只给确认轮 G18 与 beginner-me）；
面板子 agent 只写 `tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/v312-r1/<role>.md`，
不写仓库文件；面板前后各跑一次 `git status --porcelain`，有新改动即丢弃重跑。

事件：面板前由主 agent `event plan` 六个角色级事件（见
`tmp/review-loop/01-stage/evidence/y01-event-map-role.csv`）；面板报告完成后逐个
`event complete`。逐单元最终事件在修复/回读后另立，不复用面板 review_id。

- 事件 ID（2026-11-23）：P-FACT `REV-20261123-584030D8`；READER-A `REV-20261123-EAA8B5CB`；
  READER-B `REV-20261123-0D275715`；READER-C `REV-20261123-69AA1B93`；
  DESIGN-D2 `REV-20261123-19A348EB`；P-REFUTE `REV-20261123-8027B6CB`。
- 面板派发前 `git status --porcelain`：仅 `events.csv` 由本步骤 `event plan` 修改；
  两个未跟踪文件保持不变。面板子 agent 只写 `tmp/.../panel/v312-r1/`。
- 面板提示词：`tmp/review-loop/01-stage/panel-prompts/{fact,reader-a,reader-b,reader-c,design-d2,refute}.md`。

### §2 发现

- 面板 6 角色：P-FACT 3、READER-A 14、READER-B 12、READER-C 13、DESIGN-D2 6、P-REFUTE 0 FINDING（6 条 CLAIM 全部反证失败，附 4 条论证边界 P2）。
- 合计 48 FINDING：P0 0 / P1 5 / P2 24 / P3 19；另有 36 SUGGESTION。
- P1：DESIGN-01（`KinfoStruct.user_sp` 读者事实）、READER-B-01（memmap 槽位表与 panic 冲突）、READER-B-02（riscv64 判据）、READER-C-01（I4 与 §4.4.1 冲突）、READER-C-02（I4 证据不证明顺序）。
- 原始报告在 `panel/round1/`；`INDEX.md` 由 `panel-index.py` 生成。

### §3 归因

- 根因合并：48 FINDING → 40 根因（`R1-01`..`R1-40`），重复合并 8。
- 台账：`defect-ledger.md`；P-REFUTE 附加 6 行；36 条建议在 `## 建议` 节。
- `R1-24`（DESIGN-02）按 `review-publication.md` §四.6 属设计批级，标 `⑥/BLOCKED`，案件卷见 `DESIGN-02-CASE.md`。

### §4 修复

- 文档批：`R1-01`..`R1-23`、`R1-25`..`R1-36`；代码批：`R1-37`..`R1-40`。
- 已提交：`7a356f781`（文档批）、`9228fd512`（代码批 + `R1-40`）。
- 测试：`cargo test -p minix-boot --lib` 188/0；`cargo test -p minix-kernel --features mock` 866/0/8（全部测试二进制 876/0/12）。
- 建议：36 条中约 15 条已随修复落地；其余待 R2（本轮 `BLOCKED`，不宣告收敛）。

### §5 复验

- 文档 verifier：34/35 PASS；R1-14 的 `LOADER_DATA` 指针在复检中修正后整体 PASS（见 `certify/round1-verify-doc.md` 顶部复检更新与 §5）。
- 代码 verifier：`R1-37`..`R1-40` 全部 PASS；`R1-41` 文档同步追加复核 PASS（`certify/round1-verify-code.md`）。
- beginner-me 冷读：`HARD-REJECTIONS: 0`、`VERDICT: READY`（`certify/cold-read-y01.md`；读的版本 `b1a9bd6…`，当前 `c9f2952…`，两处小修不改变可读性结论）。
- 文档门：rubric 0 缺项、publication 0 新增、anchor 90/90、ref OK、density OK、citation failed=0、style error=0；`review-units.py check` OK。
- 未做：最终单元事件/冻结/确认轮；`v312-closure-check` 仍 FAIL 于 5 条 stale 事件、`frozen=0/48`、t1/t2 pending、`R1-24` audit open=3。

### §6 指标

- `metrics.md`：发现总数 48、唯一根因数 40、重复合并 8、未闭环 1、轮结果 `BLOCKED`。

### §7 状态

- `INVALIDATED`（D-119）：旧 R1 被 Manuscript DESIGN-02 设计批按 `review-publication.md` §四.6 中止；旧 48 FINDING / 40 根因 / 单元证据保留为历史，不继承到新基线。
- 旧状态保留：`R1-24` 曾是唯一 open 根因；未写 `FREEZE.md`；未宣告 `ROUND-CLOSED` / `CONVERGED`；48 单元未冻结。
- 新 R1：`v312/r1-post-design02/loop.md`，基线 commit `cdd0d1c77`，doc sha `996ff161…`，2,946 行。

### §8 给 meta-review 的不确定点

- 零欠账与 §四.6 的冲突：设计批级 P2 在轮内不能实施，但 V3.7 关闭了 ⑥ 延期出口；本轮选择 `BLOCKED` + 案件卷，不代批 defer。
- 36 条建议只修了一部分；其余是否在 R2 继续、还是随 DESIGN-02 设计批一起处理，需要作者定。
- 冷读的五项可选小修（§3.2 字段账、§2.4.2.3 评审口吻、§5.1 I3/I4 单元格、§3.6 计数表、三处过程台账）与其余建议待 R2。
- 未冻结单元、未做最终单元事件、未跑确认轮；机械 `v312-closure-check` 仍应 FAIL。
- PIA/DIA 未运行；本轮先以 verifier + 冷读收口，PIA/DIA 留待 DESIGN-02 裁决后与 R2 一起做。
