# 06-stage-sched Rust 实现架构级 Review TODO

> 来源：2026-09-06 第一轮（archive/todo-round1-archive-2026-09-06.md）+ 2026-09-09 第二轮 V2 轮与执行轮（archive/todo-V2-archive-2026-09-14.md，**Fix #5~#14 修复记录、§1 覆盖矩阵全文、§4 五层深审全文、§9/§10 执行轮总记以该存档为检索权威**；本主文件 2026-09-14 精简，已完成条目不再正文保留）。
> 范围：一等对象 `os/servers/sched/src/` 全部 Rust 代码（19 文件，crate 名 `minix-sched`）；内核契约面与客户端面为辅（发现按 edge 判定规则登记 `../edge_todo.md`）。
> 定位：不复写 plan.md；跨 stage 条目唯一入口是 `../edge_todo.md`，本文档只留双向指针。
> 状态（2026-09-14 复核）：**stage 内全部可行动 TODO 保持闭环**（两轮共 14 条修复，P0 恒为零；基线 81 passed / clippy 本体 0 告警未回退）。唯一开口项 P2-3（SUSPEND 常量）挂 edge E-MINTYPES-SYS，属共享基建收敛非本 stage 代码。下一轮触发条件：E1/E8 通电后或 01-stage SMP 落地后的验证轮，非时间驱动例行轮。

---

## 0. 两轮结论速览（详情见 archive）

| 轮次 | 产出 | 结果 |
|------|------|------|
| 第一轮（2026-09-06） | P1×3（主循环 / SchedServer 单一所有者 / 双 trait 接缝）+ P2×1 + P3×5 + edge E8/E-SCHEDSMP 前身 | ✅ Fix #1~#4，基线 59 → 79 passed |
| 第二轮 V2 + 执行轮（2026-09-09） | stage 内 1 P2 升格 + 2 P3 新条目 + 4 条存量复核；**行为级发现全部在内核对端**（niced 断环 / PREEMPTIBLE 近似 / SMP 三件套 / SYS_* 常量缺位） | ✅ Fix #5~#14，基线 79 → **81 passed**，Gate H.1/H.6 缺失归零，Rule Discovery 3 条候选（archive §7） |

验证基线（2026-09-09 实测，2026-09-14 复核未回退；后续修复轮以此为对照）：

- `cargo test -p minix-sched`：**81 passed / 0 failed**
- `cargo clippy -p minix-sched --all-targets`：本体 **0 条告警**（全链告警来自依赖 crate，归 edge E-MINSYS-HYGIENE）
- `tools/design-coverage-check.sh fork-syscall-rewrite --stage 06-stage-sched`：**ALL DOCS COMPLETE**（Fix #14）
- Gate E：02 篇 §5 server.rs 锚点 3/3 + minix-types 侧两处行锚已修正（Fix #12），全部对齐

---

## 1. 开口项（唯一）

### P2-3 SUSPEND 常量本地定义 —— ⏸ 挂 edge E-MINTYPES-SYS（依赖未解除，非 DEFERRED 充数）

dispatch.rs:21 `pub const SUSPEND: i32 = -998;` 与 transport.rs:41/:45 的 SYS_SCHEDULE/SYS_SCHEDCTL 本地镜像同批收敛：minix-types 一次补两族常量（SUSPEND + SYS_* 调用号），本 crate 改消费。**见 `../edge_todo.md` E-MINTYPES-SYS**（08-stage-is V1 轮复核该条目时无新增观察）。

---

## 2. 边界条目双向指针（唯一入口：../edge_todo.md）

| edge 条目 | 来源 | 一句话 | 06 侧关联 |
|---|---|---|---|
| E-SCHEDNICED | V2 §1.3 | kernel 丢弃 SYS_SCHEDULE 的 niced 字段，注释引用不存在的 SYS_NICE | 12 篇契约 niced 半 |
| E-PREEMPTFLAG | V2 §1.3 | PREEMPTIBLE 用 priority!=0 近似，队列 0 进程永不通知调度者 | 08/12 篇 NO_QUANTUM 链 |
| E-SCHEDSMP | 第一轮 §7 升级 | cpu 下发链三环断（每核队列/EBADCPU/迁移） | 06 篇重试环、10 篇 pick |
| E-MINTYPES-SYS | V2 §4.4 | SYS_* 调用号常量三处各自表达 | 本文 §1（P2-3 合并修） |
| E5 增补 (e) | V2 §4.5 | PM↔SCHED 联调验收面（START/INHERIT/NO_QUANTUM 回环） | E8 的联调出口 |
| E8 | 第一轮 P1-3 抽取 | SCHED SYS_* 内核调用真实通电 | 传输接缝的生产半 |

---

## 3. 剩余工作与领取条件

1. **edge E-PREEMPTFLAG / E-SCHEDNICED**（行为级两件，价值最高；随 edge_todo 单线程队列领取，前者优先）。
2. **P2-3 + E-MINTYPES-SYS**（minix-types 常量收敛轮）。
3. **E-SCHEDSMP / E5(e) / E8**（挂 01-stage SMP 与 E1 工作窗）。
4. **00/99 正文改写**（plan.md §6.1 排期；快照六件套已就位）。
5. **下一轮架构审查触发条件**：E1/E8 通电后，或 01-stage SMP 落地后（验证轮）。

每次修复遵循 fix-guard（修前读目标行 ±5、grep 确认现状、一次一条、修后 grep 验证并记录），修完跑 `cargo test -p minix-sched` 对照基线 **81 passed**。

## 4. 存档

- 第一轮全文（含 Fix #1~#4）：`archive/todo-round1-archive-2026-09-06.md`
- 第二轮 V2 轮 + 执行轮全文（含 Fix #5~#14、覆盖矩阵 V2、五层深审 V2、Rule Discovery 3 条候选、git 竞态事故注记）：`archive/todo-V2-archive-2026-09-14.md`
