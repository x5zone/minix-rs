# 03-stage 01 R1 loop（V3.12）

schema: V3.12 from R1

## R1 full

### §0 基线

- 文档：`rewrite-notes/03-stage-rs/01-rs-boot-init.md`
- 行数：1116；sha256：`4b4a8bcae96e394671b74cc7057376f09a3bdeec35dbc3fae5c126236424da3b`
- edition：V3.12；round 1；full_rounds 1
- 单元：39（6 章 + 33 节）
- 范围：`SCOPE.md`（plan + qwen 收缩，拒绝 HY4/glm 的替代 01）

### §1 面板

- READER-A / READER-B / READER-C：独立冷读，只读本篇。
- P-FACT：定点 7 项（4 PASS / 3 FAIL），13 条 FINDING。
- DESIGN-D2：首个 agent 超时；窄复核 3 项（2 PASS / 1 FAIL），补 2 条。
- 报告在 `panel/`。

### §2 发现

- 面板原始发现：84 条（五份报告）。
- 归因前按根因合并，原始 ID 与根因组对应见 defect-ledger。

### §3 归因

84 条原始发现 → 24 个根因行。P0：
1. Step 1 被写成“不向内核提交”，C 实际对非 RS/VM 服务调 `SYS_PRIV_SET_SYS` + `sys_getpriv`。
2. 生产接线状态过期：仍写 `placeholder`/`UnimplementedKernelApi`，实际 `acquire_from` + `TrapKernelApi`。
3. 测试基线 208/29 与 359/41 互斥，且模块计数与实测不符。
4. RS 的 SEF 入口路径写错：RS 走 `do_sef_rs_init()` 本地合成，不接收自己的 RS_INIT。

### §4 修复

- Step1/Step2/Step3 的范围与依赖重写；INIT（USR_F）不参加 RS_INIT 握手。
- SEF 入口改为 `do_sef_rs_init`；其他服务才接收 RS_INIT。
- 生产状态统一：`BootTables::acquire_from` + `RsServer::new` 的 `TrapKernelApi`。
- BootInit 字段改 `RProcTable`；run 返回 `Result`；grant 类型 `Option<u32>`。
- 测试基线 359/0、三模块 42 项、361 个 `#[test]` 含 2 个 feature-gated。
- 删除导航骨架、dispatch 细节、重复示例与 process ID；章节连续编号。
- 39/39 单元冻结，defect-ledger open=0。

### §5 复验

```text
doc-publication-check: 无新增违规
tutorial-rubric-check: 5 章 0 缺项
doc-style-lint: error=0
doc-ref-check: 内部引用均可解析
density: 承载门通过
cargo test -p minix-rs --lib: 359 passed / 0 failed
review-units check: units=39 frozen=39
review-units audit: unit 1 open=35（根因行当前都归在单元 1，R2 需按发现位置重映射）
```

### §5 复验（轨迹复核）

`trajectory-r1.md`：R1 PASS；`event verify --fresh` 39/39 OK。

### §6 指标

见 `metrics.md`：87 原始 / 48 根因 / P0 4 / open 35 根因 / `ROUND-OPEN`；P-REFUTE 的 4 条反证另见台账 `## P-REFUTE 附加`。

### §7 状态

`ROUND-OPEN`。R1 台账对账后发现 87 条面板发现中仍有 43 条缺失项未修、既有行另有过期 ID；`未闭环 = 35` 根因行。不得宣告 `ROUND-CLOSED` 或 `CONVERGED`。下一步是 R2 修复这些行，而不是重记状态。

### §8 给 meta-review 的不确定点

1. DESIGN-D2 大范围 agent 超时；窄复核只覆盖 BootTables/BootInit/KernelApi/feature 四项。
2. `self_update` 尾步已在 `init_fresh` 的 `live-update` feature 下接线并有 `test_init_fresh_runs_self_update_tail`；默认 feature 仍关闭，默认构建与 C 默认的差异保留。
3. Step 3 的 `IpcApi::receive` 未复刻 `sef_receive_status` 的信号/控制消息拦截（D2-02，`R1-48` 仍为 ⑥）；12 落地时补。
4. REFUTE-01..04 的代码修复已落地，但正文仍有 43 条面板发现未修；台账 `未闭环` 与 `ROUND-OPEN` 是当前唯一口径。

### L6

本轮未开独立 meta；R1 面板与缺口已在 §1/§2 记录。

### L6.5

三问记录：① 本轮发现是否收敛？未收敛：台账对账后 open=35 根因（43 条缺失 FINDING 未修）。② 是否有结构性新根因？有，P-REFUTE 的 REFUTE-01..04 已另表登记并修代码；面板根因仍待 R2。③ 是否进入下一轮？进入 R2；当前状态 `ROUND-OPEN`。
