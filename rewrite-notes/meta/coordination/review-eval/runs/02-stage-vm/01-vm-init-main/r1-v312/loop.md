# 02-stage 01 R1 loop（V3.12）

## R1 full（2026-11-22）

### §0 基线

- 文档：`rewrite-notes/02-stage-vm/01-vm-init-main.md`
- 行数：1145；sha256：`ea438ebcda4b112b2fa07c8c2bf74e3ef55cdab871036ab1a64ddfc5318808a7`
- edition：V3.12；round 1；full_rounds 1
- 单元：43（6 章 + 37 节）
- 内容范围：`SCOPE.md`（五份 rerank + plan §2 的共识）

### §1 面板

- READER-A / READER-B / READER-C：独立冷读，只读本篇。
- DESIGN-D2：对照 C 与 Rust，聚焦设计差异、ARCH 与实现边界。
- P-FACT：窄口径定点复核（原大范围 agent 超时；同 agent fact-check 作 MANUAL_FALLBACK，后补 P-FACT 4 条）。
- 五份报告在 `panel/`。

### §2 归因

83 条原始发现 → 34 个根因行；见 `defect-ledger.md`。重点根因：
1. `extra_pages()` 在 handoff 模块列表上沿用 C 的“排除末条”，丢掉真实模块与 VM 映像页（P0-code-bug）。
2. 内核交接页恒写 `is_first_time=1`，四态路由当前只可达 `ColdInit`，文档原称“复刻 C 门控”（P0-fact）。
3. 初始栈帧已实现，文档误写“未落地、stack/ps_str=0”（P0-fact）。
4. `register_page_alloc`、`mark_initialized`、`pt_init` 的完成时机与“与 C 顺序一致”冲突（P1）。
5. SEF 只实现 fresh，`init_type` 未消费；异步回复应为 `asynsend`（P1）。
6. `total_pages` 四口径混用、A2 reconcile 缺席、HeapArena/Direct Map 说反（P1）。

### §3 修复

- 代码：`boot.rs::extra_pages` 改为全部 handoff 模块 + VM 映像页；新增 VM 映像测试；`init_vm_slot` 生产缺 VM 条目时 panic。
- 文档：§1 问题入口/术语/阶段映射；§2 C 证据与行号；§3 设计边界/ARCH/SEF/总页数；§4 入口/init 时序/失败对照；§5 测试基线/真机门；§6 过渡。
- 单元：43/43 冻结，每单元独立 `UNIT` 证据；defect-ledger open=0。

### §4 复验

```text
doc-publication-check: 无新增违规
tutorial-rubric-check: 检查 5 章，0 章缺项
doc-style-lint: error级=0
doc-ref-check: 内部章节引用均可解析
doc-code-density-check: 承载门通过
cargo test -p minix-vm --lib: 549 passed / 0 failed
review-units check: units=43 frozen=43 chapters=6
review-units audit: 全部单元 open=0
```

### §5 轨迹

`trajectory-r1.md`：R1 PASS。面板覆盖、归因、修复复验、单元账均自洽。

### §6 指标

见 `metrics.md`：83 原始 / 34 根因 / P0 3 / open 0 / ROUND-CLOSED。

### §7 状态

`ROUND-CLOSED`。不宣告 `CONVERGED`：还缺第 2 个 full 轮、确认轮、作者确认的学习范围与 G18 冷读。

### §8 给下一轮的不确定点

1. `WarmRun`/`WarmNoState`/`DoubleInit` 是诊断占位；接 LU 时必须重定义 slot 状态与 `mark_initialized`。
2. 真机门尚无独立 VM 启动链断言；§5.4 只列 `e5d:` 路标与风险。
3. P-FACT 大范围 agent 超时；fact-check 是同一 agent 的 `MANUAL_FALLBACK`，P-FACT 窄复核已补 6 PASS / 4 FINDING。
