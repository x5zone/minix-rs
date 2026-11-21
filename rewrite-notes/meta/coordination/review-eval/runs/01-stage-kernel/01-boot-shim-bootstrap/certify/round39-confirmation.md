# D01 R39 确认轮记录（2026-11-22）

## 范围与抽样

- 确认轮由冷启动审阅者执行，不参与此前的修复。
- 抽样：`review-units.py sample <doc> --seed <HEAD>`，10 个冻结单元：
  4.8、3.5、3.7、6.0、2.5、2.1、1.0、3.6、6.2、5.3。
- 抽样单元覆盖 `##` 章导读、C 源对照、设计决策、实现与测试证据；未抽样单元仍受 T3 机械门与 R38 全篇 survey 覆盖。

## 第一轮结果

`UNIT` 判定：7 PASS / 3 FAIL（3.5、3.6、5.3）。

| ID | 节 | 严重度 | 问题 | 处置 |
|---|---|---|---|---|
| D1 | 3.5 | P2 | 把 bootface 门误写成 `run_all.sh` 注册；实际是手工门 | 已改为“U-Boot shim 门在 run_all.sh；bootface 门是手工门” |
| D2 | 3.5 | P3 | `place_modules` 条目数检查被写成“两条断言”；实际一条断言 + 一条 fail-closed 返回 | 已按源码改写 |
| D3 | 3.5 | P3 | `PlacementPool` 被写成三个 `u64`；实际五个字段 | 已改为五个字段并补内核跨度 |
| D4 | 3.5 | P3 | 非 ELF 输入被写成会“二次修剪”；实际保留整槽长度 | 已按 `elf_file_end` 的 `None` 路径改写 |
| D5 | 3.6 | P2 | 文档仍称 `PreparedJump`/prepare/jump 为 `pub`；实际仅 `enter_kernel_image_v1` 公开 | 已改文档两处与 `lib.rs` 注释 |
| D6 | 5.3 | P2 | “三层结论”指向已不存在的三层，且 §3.2.4/§4.5 反向循环引用 | 已改名“结论”并改两处交叉引用 |
| D7 | 5.3 | P2 | 称“没有归档的探针测试”；实际存在 `test-kernel-map` 的 N-5 print-only 探针 | 已改为“探针结果未归档、不参与 PASS 判据” |

修复登记：`RD-R39-001..007`；确认轮修复计数 1/2（`--confirm-fix` 3.5）。

## 第一轮增量复核

D1–D7 全部 `FIXED`。复核同时发现两处同源/旧有 P3：

| ID | 节 | 严重度 | 问题 | 处置 |
|---|---|---|---|---|
| N1 | 3.6 | P3 | `lib.rs:550-551` 仍称 split 是为了让 shim 自行打印路标，与新可见性说明矛盾 | 已改为“prepare 与绝对跳转是公开入口内部两步” |
| N2 | 3.1.1 | P3 | 同一 U-Boot shim 六路标门仍被标为“手工门”，与 §3.5 新句和 CI 事实冲突 | 已改为“`run_all.sh` / CI 覆盖门” |

修复登记：`RD-R39-008..009`；确认轮修复计数 2/2（`--confirm-fix` 3.1）。

## 第二轮增量复核

冷启动审阅者对 N1/N2 的只读复核：

- N1 `FIXED`：`lib.rs:550-551` 已改为“Split from prepare so `enter_kernel_image_v1` can keep preparation and the absolute jump as two internal steps”；旧理由句零命中。
- N2 `FIXED`：`doc:1122` 已改为“`run_all.sh` / CI 覆盖门”；`手工门` 只剩 §3.5 对 bootface 门的正确描述。

`DELTA: PASS`。两处修复均未引入新矛盾。

## 确认轮出口

- 10 个抽样单元：第一轮 7 PASS / 3 FAIL；修复后 D1–D7 全部 FIXED；N1/N2 两处 P3 同源矛盾修复后 DELTA PASS。
- 确认轮修复 2/2 批次（第一批 5 个单元、第二批 1 个单元），未开新修改轮。
- 单元状态：49/49 frozen、reopened=0、stale=0、T3 PASS；`audit` 全部单元 open=0。
- 机械门与真机门见 `round39-gates.md`；TODO 账本 `--fail-on-open --doc 01-boot-shim-bootstrap` 为 0。

