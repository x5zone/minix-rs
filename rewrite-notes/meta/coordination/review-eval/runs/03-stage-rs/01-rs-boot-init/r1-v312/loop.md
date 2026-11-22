# 03-stage 01 R1 loop（V3.12）

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

### §2 归因

84 条原始发现 → 24 个根因行。P0：
1. Step 1 被写成“不向内核提交”，C 实际对非 RS/VM 服务调 `SYS_PRIV_SET_SYS` + `sys_getpriv`。
2. 生产接线状态过期：仍写 `placeholder`/`UnimplementedKernelApi`，实际 `acquire_from` + `TrapKernelApi`。
3. 测试基线 208/29 与 359/41 互斥，且模块计数与实测不符。
4. RS 的 SEF 入口路径写错：RS 走 `do_sef_rs_init()` 本地合成，不接收自己的 RS_INIT。

### §3 修复

- Step1/Step2/Step3 的范围与依赖重写；INIT（USR_F）不参加 RS_INIT 握手。
- SEF 入口改为 `do_sef_rs_init`；其他服务才接收 RS_INIT。
- 生产状态统一：`BootTables::acquire_from` + `RsServer::new` 的 `TrapKernelApi`。
- BootInit 字段改 `RProcTable`；run 返回 `Result`；grant 类型 `Option<u32>`。
- 测试基线 359/0、三模块 42 项、361 个 `#[test]` 含 2 个 feature-gated。
- 删除导航骨架、dispatch 细节、重复示例与 process ID；章节连续编号。
- 39/39 单元冻结，defect-ledger open=0。

### §4 复验

```text
doc-publication-check: 无新增违规
tutorial-rubric-check: 5 章 0 缺项
doc-style-lint: error=0
doc-ref-check: 内部引用均可解析
density: 承载门通过
cargo test -p minix-rs --lib: 359 passed / 0 failed
review-units check: units=39 frozen=39
review-units audit: 全部单元 open=0
```

### §5 轨迹

`trajectory-r1.md`：R1 PASS。

### §6 指标

见 `metrics.md`：84 原始 / 24 根因 / P0 4 / open 0 / ROUND-CLOSED。

### §7 状态

`ROUND-CLOSED`。不宣告 CONVERGED：还缺第 2 个 full 轮、确认轮、作者确认的学习范围与 G18 冷读。

### §8 不确定点

1. DESIGN-D2 大范围 agent 超时；窄复核只覆盖 BootTables/BootInit/KernelApi/feature 四项。
2. `self_update` 当前无生产调用点；默认 Rust boot 比 C 默认 yes 少自升级尾步，接 18 时补。
3. Step 3 的生产 `IpcApi` 未接线时是 ENOSYS 占位，不是 C 的阻塞等待；12 落地后恢复。
