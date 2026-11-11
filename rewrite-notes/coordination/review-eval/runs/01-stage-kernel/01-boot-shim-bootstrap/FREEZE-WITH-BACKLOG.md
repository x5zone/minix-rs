# D01 FREEZE（V3.1，2026-11-10；R30 前的历史记录）

> **已被 R30 取代**：R30 完整 survey 的 57 条已全部关闭，最终结论见同目录 `CONVERGED.md`。本文件只保留 R29 时点的冻结记录，不再是最新状态。

> 这不是“零缺陷”证明。它是“按 V3.1 的关闭条件把 55 条已知项全部关闭后，停止同一
> edition 的继续迭代”的记录；剩余的是证据缺口（QEMU 宿主兼容、独立冷读），不是
> 未处置的缺陷。文档仍可在 V3.2 或新证据下重开；重开不是本次失败的反悔，而是新
> 尺度的认证。

## 一、冻结对象

- 文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- 关联代码：`os/` 下本篇引用的 kernel / kernel-image / boot-shim / minix-boot / minix-arch
- 工作流版本：V3.1（`prompt/review-rules/review-evals.md`、`prompt/skill/review-loop.md`）
- 轨迹：`rewrite-notes/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/`
- 基线：工作树（HEAD `3f0a466ed`；本轮改动未提交）

## 二、关闭证据

- R28（V3.1 第 1 轮，结构轮）：R27 backlog 的 37 个发现 ID / 32 个根因全部关闭；
  `eval-trace-check.py --round R28` PASS；六道文档门 + 三目标 `cargo check` 通过。
- R29（V3.1 第 2 轮，完整 survey）：6 个角色 55 条发现，全部进入
  `defect-ledger.md` 的 `## R29 完整 survey`；`--round R29` PASS（PASS 11 / FAIL 0）；
  未闭环 0、certify 新准入 0。
- 机械门（最终工作树）：citation 69 块 / 29 检查 / 0 失败；anchor 106/106；
  ref/structure/style/density 全绿。
- 宿主：`cargo test -p minix-kernel --features mock` 866 passed / 8 ignored；
  `cargo check -p kernel-image` 的 riscv64/aarch64 直核目标通过。
- QEMU：R29 未改启动行为的部分沿用 R27 的 `logs/round27-*.log`；Step 1 错误路径与
  `BootStorageCell` 改动没有重跑 QEMU，这是本冻结的已知证据缺口（见第四节）。

## 三、55 条发现的去向

| 处置类 | 数量 | 去向 |
|---|---|---|
| ① | 10 | 本轮已修：FACT-A 3、FACT-B 7；逐条证据在台账 |
| ④ | 35 | 本轮已修：READER-A 10、READER-B 15、READER-C 10；逐条证据在台账 |
| ⑥ | 9 | 本轮已实施：D2-20..28 全部九项 |
| FP | 1 | FACT-B-08：编译期断言指控不成立（`handoff_v1.rs:422-467`） |
| ② | 0 | 无转出项；D2-20/21/23 已在 R29 内实施 |


## 四、已知证据缺口（不掩盖）

1. **R27 台账已回填**：`defect-ledger.md` 的 `## R27 面板轮` 现有 70 行（64 个
   FINDING + 6 个 CLAIM），R28/R29 覆盖项用 `dup-of: RD-R28-xx` 指向结构轮根因。
   回填不改变 R27 当时的结论，只补可追溯性。
2. **QEMU 已重跑 6 条：5 PASS + 1 SKIP**：riscv64 bootface、aarch64 SMP AP、
   riscv64 U-Boot shim、x86 UEFI bootmarks、aarch64 UEFI bootmarks 全 PASS。
   第 6 条 riscv64 U-Boot 直载测试链在宿主 U-Boot 2025.10 上于
   `## Executing script` 之后触发 `Unhandled exception: Load access fault` 并复位
   （裸机与 smode 两个 blob 都复现），内核从未执行；这是 U-Boot 自身的装载故障，
   不是内核启动失败，按脚本契约记 `SKIP`（exit 2），不写成 PASS。同一内核经
   OpenSBI 直载能走到 `entering scheduler`（`test-riscv64-boot-full.sh` 的首跑证据）。
3. **独立冷读未做**：R29 的 55 条由主 agent 修复并同 agent 复读，存在自证偏差；
   没有第二读者交叉验证。
4. **不声称零缺陷**：本冻结只表示“V3.1 的 2 轮交易下，55 条已知项全部关闭；
   证据缺口见上”。

## 五、为什么不在同一 edition 跑 R30

- 总体 5 轮预算（R25 收口 + R26–R29 四轮 review）已经用完；V3.1 的 3 轮窗口用了 2 轮。
- R27 backlog 清掉后，R29 仍找到 55 条（含 3 条 P1）。再跑一轮结构轮，边际收益
  主要是“补 QEMU 宿主兼容 + 独立冷读”，而不是“发现新的大类问题”；这更像 V3.2 的
  代码批，不像同一 edition 的第 3 轮结构轮。
- 按 V3.1 §7.5，第 3 轮若仍是线级补丁或仍 `open>0`，应判 `NOT CONVERGING`；本冻结
  直接承认“没有在 V3.1 内收敛”，不把 R30 当成自动收敛的保证。

## 六、重开规则

出现以下任一情况才重开，不因“某次评审又提了建议”重开：

1. 新的真实盲区（P0/P1），有源码、测试或真机证据；
2. V3.2 代码批落地后，需要同步文档的接口/行为变化；
3. QEMU 重跑发现与文档冲突的启动行为；
4. 用户明确要求。

重开时从 V3.2 的完整 survey 开始，不继承本冻结的“已关闭”状态；本文件只作为
历史输入，不是合同。

## 七、目标 2『干净』口径（代理裁决，2026-11-10）

用户未在会话中回答口径问题（AskUserQuestion 无回答）。按目标里“待裁决项可以根据你的
建议，由你代为裁决”的授权，采用口径 A：

- **干净 = 所有已登记问题都有处置且 `open=0`**，不是“证明零未知缺陷”。
- 当前满足：R29 的 55 条全部关闭（① 10 / ④ 35 / ⑥ 9 / FP 1 / ② 0 / ⑤ 0），
  R27 的 70 行台账已回填，文档六门全绿。
- QEMU 第 6 条（riscv64 U-Boot 直载链）按脚本契约记 **环境 SKIP**（U-Boot 2025.10
  在 `## Executing script` 后触发自身 Load access fault，内核未执行），不写成 PASS。
- 因此目标 2 按口径 A 判“干净”；若用户要求口径 B（QEMU 6/6）或 C（零未知缺陷 +
  独立冷读），本冻结不满足，需在 V3.2 继续。
