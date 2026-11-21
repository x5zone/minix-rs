# D02 FREEZE —— 02-higher-half-kernel（CONVERGED）

- 冻结日期：2026-11-12
- 工作流 edition：V3.3
- 确认轮：R2（`轮型: confirmation`）
- 冻结提交：`809ac212b`（R2 修复与轨迹提交）
- 文档：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 文档行数：1280（R2 时点）；R3 时点 1283
- 文档 sha256：`4f096ea9997eae5aae99ad648967d2b8001256d22e7a63c017d6ea9de16804ce`（R3 复核后；R2 原值 `829b3a71d141d99110588e30d33c446939ed97c7b07de57b66bdef45054ae21b`）
- 关联代码：`tools/doc-code-map.sh --check` 28/28 通过

## 收敛证据

- `eval-trace-check.py --round R2`：PASS（11 PASS / 1 WARN / 0 FAIL）。WARN 是 `certify-fact.md` 无 FINDING 的豁免行已在 `loop.md §1` 登记。
- R2 台账：29 条（survey 21 + certify 8），处置 ① 22 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 7；`未闭环 = 0`；`certify 新准入 = 0`（certify #3 最终回读）。
- 100% 复验：每个 ① 行有可重放 grep / path:line / 冷读回执；无抽样。
- 文档六门：citation `32 块/1 检查/0 失败/31 跳过`；anchor `55/55 zero-def=0`；structure/ref 通过；style-lint 文件级 error=0；density 通过；doc-code-map 28/28。
- 宿主：本轮无代码行为改动；R1 已实测 `cargo test -p boot-shim --lib -- --list` 125、loader `#[test]` 22、minix-elf `#[test]` 23。QEMU 未跑（不触 arch/boot 行为门）。

## 轮次链

- V1 旧 run（69 条、NOT CONVERGED）保留为历史段，不追溯改账；其 42 条未批准 ② 未带入 V3.3。
- R1（V3.3 full）：33 条 survey + 16 条 certify；2 P0 + 8 P1 全修；ROUND-CLOSED。
- R2（V3.3 confirmation）：21 条 survey + 8 条 certify；5 P1（C 装载者模型、LMA 重叠报错、D-045 注释改写、前置知识漏 riscv64 载体、§2.3 其一/其二冲突）全修；certify #3 最终 0 → CONVERGED。

## 未解决事项（不阻断，已登记）

- 设计批 `D2-01..06/13`：SegmentIter 未检查加法、wire 派生标量、kmain 形参遮蔽、jump_to_kmain 冗余参数、LoaderError 分层、取证打印删除等；登记在 `follow-ups.md` 的 R1 设计 P3 表与 `DEFECT-LEDGER` 的 D2 行，下一 design batch 实施。
- ARCH 提案：`D-050`（载荷所有权类型化）、`D-051`（链接符号导出跨距）、`D-052`（BootLayout + ValidatedForJump token）待用户裁决；不实施、不阻断。
- 读者 SUGGESTION：R2 的 7 条设计 P3 与 11 条读者措辞建议未入台账，留待下一 edition 边界。

## 限制

- `CONVERGED` 的含义是“确认轮口径下没有 open 项 + 无未销账债务”，不是“零未知缺陷保证”。
- 同 agent 复核：面板与 certify 使用同一模型；已重放全部 grep / 门命令，cross-model 独立复核未做。
- C 侧装载者模型修正基于源码直读 + 等价链接脚本实验，未跑 Minix 实际 boot monitor / QEMU 原生引导链。
- 新证据、下一 edition 或设计批实施后基线改变，可以重开；重开不是本次失败。

## 重开条件

- 出现新的 P0/P1 事实或行为缺陷证据；
- 设计批（D2-01..06/13）或 ARCH 提案实施后基线改变（需重新固定基线并重跑受影响门）；
- 用户明确要求重开。

## 回归 review（2026-11-18，D-055 恢复独立读者后重做）

用户裁决 D-055 后重做回归，独立五角色（FACT、DESIGN、READER A/B/C）共报 30 条：
- FACT 3 条 P3（`use crate::build_kernel_info` 是测试引用、bootface 省略标注不对应、compute_kernel_layout 注释应为删节）；无 P0/P1/P2。
- DESIGN 9 条：零 P0/P1；1 条 P2（machine_type 全仓零读者）、8 条 P3；设计骨架与代码逐项一致。
- READER 独立 A/B/C 共 9 条 FINDING（A 2、B 6、C 1）+ 6 条编号 SUGG（C-S1..S6）+ 3 条聚合 SUGG 段；其中 P1 两条（前置块塞五腿细节、OpenSBI/shim 混淆）已修。原文「18 条（24 中的 7 + 17 中的 7）」计数笔误（7+7=14≠18），r6 补账时按面板实数更正（见 defect-ledger REG-2026-11-18 节）。

修复 7 条 P1/P2 后（批次证据：`fe49cd804` 4 hunk + `afdd867b7` 2 hunk——提交信息称 6 条、diff 实为 4 主题 §1.2①/PC 行/例外清单/U-Boot 消歧，按 diff 记 4），独立冷读 certify 抓到 3 条 REOPEN（部分修复未落地）+ 2 条新 P2。补修后最终独立确认 3 PASS / 1 FAIL（FAIL 项为跨文档混淆——该文档不用 ①②③ 表标记，certify 误用了 D03 的格式），实际无缺陷。

最终文档 sha256 `829b3a71…`；anchor 55/55、doc-code-map 28/28、structure/ref/style 全绿。

**结论：回归通过，D02 维持 CONVERGED。** 合并读者漏掉的 2 条 P1 由独立 A/B/C 抓到——验证了 D-055 的必要性。

## R3 确认轮（V3.4；D-063 授权的重开确认，append-only 追加）

- 上一节记录的是 V3.3 口径下的 R2 确认；按 D-063（用户指令 `prompt/EXECUTE-D01-D06-CONVERGENCE.md`）在 V3.5 制品链下重开一次，轮次号 R3、edition V3.4、`schema: V3.4 from R3`。
- **R3 确认轮**：六角色全量 survey（fact 8、design-d2 16、reader-a 4、reader-b 9、reader-c 4 条 FINDING；SUGGESTION 48；refute 6 条 CLAIM）。无 P0，11 条 P1/P2 设计项 + 7 条读者 P1 FINDING（另有 1 条 P1 级 SUGGESTION）。
- **修复**：19 处文档修改点（§4.4 PC 行、§3.1 测试载体分类与复算口径、§3.2 外部装载器、§3.4 四条前置条件与对齐算式、§1.1 两套地址、§2.1 补基址常量定义、§5.1 M5、§6.2 `$rsp`/`$rip` 按架构、§4.1 基址反事实、§3.5 panic 路径、§4.3 源码注释口径、§3.3 措辞、§6.2 第四组语义等）。
- **certify**：`certify/round3-certify.md`（本轮新派发的冷启动子 agent）。首轮 19 条 14 CLOSED / 5 STILL-BROKEN；按回执证据改正后复核至 18 CLOSED / 1 STILL-BROKEN；再改正末条 `:1242` 的 aarch64 前缀后终判 PASS。**回执抓到主 agent 三处实质错误**：把 `0xffff7fffbfe00000`（L4 平移量）当 aarch64 栈地址、误称 riscv64 的 `map_huge` 不检查 1 GiB 降级、§3.4 对齐算式方向写反。
- **制品链**：`panel/round3/` + `INDEX.md`（6 文件：FINDING 41 / SUGGESTION 48 / CLAIM 6；refute 末尾由主 agent 追加了机器可读的 `## CLAIM 11..16` 索引行，正文未改动，记为 VARIANCE）。
- **机械门**：`--round R3` PASS；anchor 55/55；citation `checked=25 failed=0`；structure/ref/style(error=0)/density 通过；doc-code-map 30/30；`panel-index --check` 6 文件哈希一致。
- **未闭环 = 0**。`OPEN-DEBT` 的 `R3-design-batch`（11 条设计 P1/P2 + 5 条设计 P3）有 D-063 判决行与 2026-11-20 日期。
- **ARCH 提案 2 条**（RD3-A01/02）待用户亲裁，不阻断。
- 限制同前：`CONVERGED` = 确认轮口径下无 open 项 + 无未销账债务，不是零未知缺陷保证。

## R4 确认轮（V3.4；R3-design-batch 实施后的基线重开，append-only 追加）

- 冻结日期：2026-11-21
- 工作流 edition：V3.4（schema `V3.4 from R3`）
- 确认轮：R4（`轮型: confirmation`）；轮结果 `CONVERGED`
- 轨迹提交：`7ff9de6c1`（四份 certify 回执、loop §5–§8、metrics/ledger/suggestions/OPEN-DEBT、meta 报告与 BLIND-SPOTS B-69..B-75）；本 FREEZE 节提交 `06ffa1c60`；回归审计与台账表格修复见 `REGRESSION.md` 的 R4 节（后续提交）
- 文档：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 文档行数：1294
- 文档 sha256：`48630c90b2a9e1eb2b2814f93ad61d44de0c8a6c634d4835ca25692c602eb781`
- 基线提交：`79f910acd`（R4 预门同步后）；面板 6 个独立子 agent，最终修复提交 `ca5830fd5`、`d9ceba0cb`、`2ca9a9cd7`

## 收敛证据（R4）

- `eval-trace-check.py --round R4`：PASS（11 PASS / 2 WARN / 0 FAIL）。两个 WARN 是 R3/R4 `OPEN-DEBT` 的「已批准延期，确认 DECISIONS 判决账行」提示，D-063/D-105/D-106 均存在。
- 台账：79 个 ID（RD-R4-001..079），处置 ① 19 / ② 2 / ④ 12 / FP 4 / SUGG 42；`未闭环 = 0`；`certify 新准入 = 0`。语义聚簇约 47 个缺陷根（含 4 条主张维持为 51 个主题），至少 28 个 ID 是跨角色或同根重复。
- 严重度（FINDING 36）：P0 4 / P1 5 / P2 20 / P3 7；5 条 P3 FINDING（DESIGN-D2-07/08/09、READER-B-05/B-06）按 G3 分流 SUGG。
- 独立 certify：事实/设计/代码面 `certify/round4-fact.md` 18/18 CLOSED（第一轮 `certify-r4-pass1-fact.md` 15/18）；读者面 `certify/round4-reader.md` 12/12 CLOSED（第一轮 `certify-r4-pass1-reader.md` 10/12，第二轮 11/12 未单独留档，属 B-73）。三轮残项同源于 §3.2 的一行旧题句与 §4.1 的 `.lrodata` LMA 说明。
- post-certify P3：§5.1 的「三类畸形」改为「四类畸形」并列出 `PhdrOutOfBounds`/`PhdrEntryTooSmall`/`SegmentOutOfBounds`/`FileszExceedsMemsz`；该行不在 18/12 个判定点内，改后文档 sha 为 `48630c90…`，已由冷启动单行复核 `certify/round4-postfix-line1120.md` 判 4/4 CLOSED（变体名与源码逐字一致、12 个测试名全部存在）。它使 `自引入 = 2`（两条均轮内修复）。
- 文档六门：anchor `54/54 zero-def=0 multi-def=0`；citation `blocks=32 checked=24 failed=0 skipped=4 warned=4`；structure/ref/style（error=0）/density 通过；doc-code-map 30/30；`--stale-lines HEAD --strict` PASS。
- 行为门：`check-layout.sh all` PASS（L0–L9；x86 L3a=4 个 PT_LOAD）；`test-sysboot.sh` PASS；`test-riscv64-uboot-shim.sh` 6/6 PASS；`minix-elf` 27/0、`boot-shim` 146/0、`minix-kernel --features mock` 866/0/8。
- 独立回归审计（§4h）：`REGRESSION.md` 的 R4 节，冷启动只读 agent 逐项 13/13 PASS；唯一格式问题（RD-R4-017 的未转义 `|`）已在同节提交修复，未影响任何门读数。
- 可引导镜像门：`cd os && cargo run -q -p xtask -- image --arch x86_64 -r` 重建后，从 `os/target/image/x86_64/minix.img` 解出 `::/EFI/minix/kernel.elf`，与 staging 同哈希 `fca5b85f202eb7ca03d38d2f8d26636400a768a02c41cd323cb9e507bca8efe2`；4 个 `LOAD`、0 个 `.lrodata`。meta 读到的旧镜像（5 LOAD / 986 `.lrodata`）已被替换。

## R4 未解决事项（不阻断，已登记）

- `R4-design-batch`（P2）：DESIGN-D2-04（`build_kernel_info` 改收 `BootReport`，删 12 处 `from_raw_parts`）与 DESIGN-D2-06（三份无生产读者的 arch `link.ld` 删除或补齐）。D-105/D-106 代理裁决，授权来源为用户 2026-11-21 review-loop D02 会话代裁指令；D2-06 与 `R3-design-batch` 的「arch link.ld 无读者」及 V1 `D02-F03` 同源，按迁移登记，不重复计根。
- 读者/设计 P3：37 条编号建议 + 5 条 fact 未编号建议（事后编号 `FACT-S1..S5`）登记在 `suggestions.md`；其中 5 条已由 FINDING 修复覆盖（`READER-A-S03`、`READER-A-S06`、`READER-B-S11`、`READER-B-S14`、`READER-C-S10(a)`），其余只登记或留下一设计批。
- meta 盲区 B-69..B-75 已回写 `BLIND-SPOTS.md`：语义去重、制品入库、镜像门、未编号建议、回执命名、检索材料面、严重度对账；除镜像门已手工补证外，其余仍是 OPEN 的流程缺口。

## 限制

- `CONVERGED` 的含义是「确认轮口径下没有 open 项 + 无未销账债务」，不是「零未知缺陷保证」。
- 同 agent 复核：6 个面板与三轮 certify 都是同一模型（opencode-go-chat/deepseek-v4.1-flash）的冷启动实例；无跨模型复核。
- `.lrodata` 归并改了三份生产链接脚本并重建工件；aarch64 的 `test-rt-birth-aarch64` 仍有已知 A2 user-chain 遗留（D01 轨道），不能作为本轮绿门。
- 可引导镜像一致性是本轮手工重建 + 哈希比对，不是 CI 常驻断言（B-71 OPEN）。
- `FP 4` 是工具桶名，语义为 `CLAIM-01..04` 的「反证失败、主张维持」，不是误报；本轮真正的误报 FINDING = 0。
- 测试计数 27/31/135 是默认特性下的 `--list` 读数；`--features test-all` 为 146。
- C-05 的「GNU ld 对孤儿段继承 LMA 平移」由 reader certify 在 `/tmp` 用 ld 2.42 最小复现，仓库只保留一个旧 staging ELF 样例，不是全量链接器矩阵。

## 重开条件

- 出现新的 P0/P1 事实或行为缺陷证据；
- `R4-design-batch`（D2-04/D2-06）或 ARCH 提案实施后基线改变（需重新固定基线并重跑受影响门）；
- 文档再次修改（含 P3 精度修复）后，受影响判定点需重新回读；
- 用户明确要求重开。

## 独立 meta 复核（2026-11-21，R4 轨迹轴）

- 报告：`meta/02-higher-half-kernel-r4.md`；总体 `FLAT`（检测/设计轴 IMPROVED，轨迹/去重轴 REGRESSED），运行质量 `NOT PASS`（抽样 41/79，含 SUGG/FP 桶一致性 32/41 = 78.0%）。
- 已收口项：四份 certify 回执与 loop §5–§8 入库、FREEZE 追加 R4、metrics 增语义根因数/重复合并/严重度、5 条过期 SUGG 标 `dup-of`、BLIND-SPOTS B-69..B-75 回写、生产镜像重建并验证 4 LOAD/0 `.lrodata`。
- 未收口项：`FP` 桶名未拆（工具契约限制，语义已在 metrics/loop 写清）、`minix.img` 一致性未做成 CI 门、第二轮 reader certify 回执未单独留档、5 条 fact 未编号建议仍不在 INDEX/台账。上述均记 OPEN 盲区，不阻断本轮文档收敛。

## D-107 清零批终态（2026-11-21）

- 代码批全部落码：D2-04 `309cd6e2c`、D2-06 `503ae169e`、F01 `83e8cbb2b`、F04 `f50159798`、F05 `1460d4147`、F06 `c376f5371`、F08 `6d0334dc6`、R02-T01 `fd456926a`、R02-T03 `94d5f2379`、R02-T04 `550d8c0b8`、D2-03 `7de857246`、D2-08 `ecfef0ae9`、D2-09 `74ea87c44`。
- 设计 P3 全部实现：`p_align`/`LoadSegment.align`/`HIGHER_HALF_CALLED`/`virtual_end` 删除；三处 `repr(C)` 删除；`source_records` 更名（`e27dc513b`）；`RamAuthority` 值类型（`aa0d5393c`）；`free_pt_page` 未注册 fail-closed（`88f3cb96a`）；`write_envelope` 收为测试/载体专用；`startup_record` 死分支删除（`7de857246`）。
- 台账：`OPEN-DEBT.md` 两行 `[CLOSED]`；`follow-ups.md` F01–F12 与 R02-T01..T04 全部 `[CLOSED]`/dup-of；`suggestions.md` 全部 implemented/already-fixed/dup-of。
- 验证：`cargo test -p minix-boot --lib` 187/0、`cargo test -p boot-shim --lib` 136/0、`cargo test -p minix-kernel --features mock --lib` 866/0/8、`cargo test -p minix-arch` 266/0、三架构 kernel/kernel-image check 0 error、`check-layout.sh all` PASS（含 L10）、riscv64 higher-half 载体 PASS、riscv64 bootface 门 PASS；`eval-trace-check --round R4` PASS 12/0/0。
- 已知非本批因果：aarch64 `test-smp-aps-aarch64` 在 boot/SMP 标记全绿后仍于 user-chain Data Abort 失败（`elr=0x23e9d4`，stash 对照在 F08 前同点复现）；`test-rt-birth-aarch64` 同属 D01 轨道已登记的 A2 遗留。


## 重写基线（2026-11-21）

- 旧 v3 原文归档到 `rewrite-notes/01-stage-kernel/draft/`；现行文件按“01=交接载荷、02=内核镜像”重写。
- 旧 R4 的 CONVERGED 作废；不跨重写比较轮次。新基线为下一轮（D01 R37 / D02 R5）的 L0。
- 行数：01 = 2651，02 = 1845；比例 1.44。
- 旧 OPEN-DEBT/follow-ups/suggestions 仍全 `[CLOSED]`；重写不改变旧债证据。
