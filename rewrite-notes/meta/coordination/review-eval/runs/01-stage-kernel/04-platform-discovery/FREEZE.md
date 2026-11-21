# D04 FREEZE —— 04-platform-discovery（CONVERGED）

- 冻结日期：2026-11-12
- 工作流 edition：V3.3
- 确认轮：R2（`轮型: confirmation`）
- 冻结提交：`f9582d655`
- 文档：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`
- 行数：2735（R2 时点）；R3 时点 2737
- 文档 sha256：`90179b7b4d7a4428fe2a1591502aaf64b3ec4cd98947617d97f126529ea3545d`（R3 复核后；R2 原值 `ff8aad7ea337acd52faedbfe9fda268814d2e632e794b11b4faea44f040b22f5`）
- 关联代码：`tools/doc-code-map.sh --check` 52/52 通过

## 收敛证据

- `eval-trace-check.py --round R2`：PASS（12 PASS / 2 WARN / 0 FAIL）。WARN 是 `D04-design-batch` 的已批准延期（D-054）。
- R2 台账：43 条（survey 40 + certify 3），处置 ① 18 / ② 6 / SUGG 19；`未闭环 = 0`；`certify 新准入 = 0`（3 条已修，同 agent grep 复核）。
- R1 台账：55 条，3 P0（`find_platform_sources`、`PlatformDescSource`、uefi 测试计数）全修；`--round R1` PASS。
- 文档门：anchor 77/77；doc-code-map 52/52；structure/ref/style 通过；citation 86 块全部跳过（`/* */`，MANUAL_FALLBACK）。
- 测试：`cargo test -p minix-platform` 18 passed；uefi_helpers 12 条；QEMU 未跑。

## 未解决事项（不阻断）

- `D04-design-batch`（D-054）：R1/R2 共 12 条代码/架构项（死接口、解析器默认值、`CpuInfo`/`ArchMiscDesc` 残留、GICD 双表示、`Frozen<T>` Sync bound、`DeviceTreeDesc` 缺 `psci_conduit`、`platform_sources` 无入口校验、`PlicDesc.context` 硬编码、x86 MMIO 全局原子、ARCH `DeviceWindow`）。
- 读者 P3 9 条与设计 P3 10 条进 suggestions。
- citation 工具对 `/* */` 的盲区留在 workflow 边界。

## 限制

- `CONVERGED` 是确认轮口径下的“无 open 项 + 无未销账债务”，不是零未知缺陷保证。
- 同 agent 复核；R2 读者项由修复代理按最小修复落地，主 agent 复核门与 diff。
- 设计代码项延期到 D04-design-batch；实施后需重新固定基线。

## 重开条件

- 出现新的 P0/P1 事实或行为缺陷证据；
- D04-design-batch 实施导致基线改变；
- 用户明确要求重开。

## 回归 review（2026-11-13）

- 回归审查判 PASS，附 REG-D04-01 P2：两处 `PlatformDescSource::new(DTB, …)` 应为 `with_optional_len`；另有 P3 `low 4 GiB` 与源码 `low 4 GB` 的措辞差。
- 两处已改为 `with_optional_len`，Safety 引文恢复 `low 4 GB` 并在块头标注旧前提；`anchor 77/77`、`doc-code-map 52/52`、`--round R2` 仍 PASS（12/2/0）。

## 回归 review（2026-11-17，恢复独立读者后重做）

用户裁决 D-055 后重做回归：

- **独立三角色**共报 17 条（READER-A 3、B 12、C 2，含 4 条 P1）；合并版当时 14 条里没有「恒等映射前提三处互斥」「§1.3 规则自我否定」「§1.1 与 §1.2 结论相反」这些结构矛盾。
- **独立冷读 certify 三轮**：7 条、6 条、2 条新准入。按 §4.0b 记 `BLOCKED` 并写 `STUCK-REPORT.md`。
- 结构性重写：§2.3 两层因果改为正面陈述「6 处 return -1、三处可达」；§1.3/§4.2 例外清单统一为（甲）（乙）；§3.1 承认 `SourceRecord` 跨 ABI；§2.10 列头改 `C i386`。
- 独立闭集冷读判 `§2.3 验证 = PASS`、`结构性重写验证 = PASS`（其余三点）。
- 新增真实缺口并登记：`R04-T13`（ACPI 表链 RSDT/XSDT/MADT 不在任何已注册恒等窗口内，QEMU 通过靠 2 MiB 片取整的布局偶然）。
- 最终文档 sha256 `ff8aad7e…`；anchor 77/77、doc-code-map 52/52、structure/ref/style 全绿。

**结论：回归通过，D04 维持 CONVERGED**，另附 `R04-T13` 为已登记的相邻缺口（D-054 延期）。


## r7 citation 门扩容补记（2026-11-18）

citation 门新增第四标注形态（`// path —— kind`）与 `.S/.lds/.inc` 路径支持后，
本文 checked 35→75（修 import 后该块仍实检，另有一块因标注可解析新增实检），新检出 1 条真实陈旧引文：§4.11 区块内 `use minix_platform::
{platform_desc, PlatformDesc}` 与现行源码（`os/kernel/src/lib.rs:1962`）的导出名
顺序相反——已按源码修正为 `{PlatformDesc, platform_desc}`。修正后 citation 门
`failed=0`。本文档 sha256 更新为 `ff8aad7ea337acd52faedbfe9fda268814d2e632e794b11b4faea44f040b22f5`；按 §4h（FREEZE append 语义）追加本节。

## R3 确认轮（V3.4；D-070 授权的重开确认，append-only 追加）

- 上一节记录的是 V3.3 口径下的 R2 确认（含 2026-11-17 D-055 重做回归与 2026-11-18 citation 扩容补记）；按 D-070（用户指令 `prompt/EXECUTE-D01-D06-CONVERGENCE.md`）在 V3.5 制品链下重开一次，轮次号 R3、edition V3.4、`schema: V3.4 from R3`。
- **R3 确认轮**：六角色全量 survey（fact 6、design-d2 9、reader-a 1、reader-b 7、reader-c 4 条 FINDING；SUGGESTION 37；refute 6 条 CLAIM）。无 P0，1 条 P1（READER-C-01：§1.4 的 Rust 失败点位与 §4.1/§6.1 矛盾），11 条 P2。
- **修复**：13 处文档修改点（§1.4 失败点位改 `init_from_kinfo`/`init_protection`、C 侧约束放大口径、§1.3 DTB 单形态限定、§3.7 双措辞区分、§4.2 authority 过滤机制句、§2.9 板号两步失败停点、§5.4 M1 口径、§5.2 riscv64 topo 构造器、§4.2 cross_validate/push_dtb_sources 锚点组、三处引文标注、§3.2 enum 理由原话/推论区分、两处裸锚点补路径、三变体构造器差异说明）。
- **certify**：`certify/round3-certify.md`（本轮新派发的冷启动子 agent）——12/12 CLOSED，**四篇中唯一首轮即 PASS**。
- **面板后处理**：无（本篇面板标题层级规范）；派发按平台并发 3+3 两波（L1.1 降档协议）。
- **机械门**：`--round R3` PASS（11/2/0）；`--round R1/R2` 仍 PASS；anchor 77/77；citation `checked=68 failed=0 warned=7`；structure/ref/style(error=0)/density 通过；doc-code-map 54/54；`panel-index --check` 6 文件哈希一致。
- **未闭环 = 0**。`OPEN-DEBT` 的 `R3-design-batch`（3 条设计 P2 + 6 条设计 P3 + ARCH 提案 RD04-A1）有 D-070 判决行与 2026-11-20 日期。
- 限制同前：`CONVERGED` = 确认轮口径下无 open 项 + 无未销账债务，不是零未知缺陷保证。

## 冻结后同步（2026 年 11 月 20 日，D-064 / I-32 代码改动的文档侧回写）

代码侧把 `Frozen::freeze` 的二次冻结守卫从 `debug_assert!` 换成 `assert!`（`os/libs/minix-types/src/types/cell.rs`，验收见 `rewrite-notes/01-stage-kernel/01-todo.md` 的 I-32）；本篇有三处正文与一条建议行的口径随之失真，同批回写：

| 位置 | 原口径 | 现口径 |
|---|---|---|
| §3.7 引文块（`cell.rs:struct Frozen` 节选） | `freeze` must be called at most once (**debug builds assert**) | `freeze` must be called at most once (**every build asserts**)——源码注释的逐字节引用，随代码同步 |
| §5 前 §3.7 否决段 | `freeze` 里只有 `debug_assert!`，发布构建重复 `freeze` 静默覆盖 | `freeze` 开头是 `assert!`，两种构建形态下都当场 panic |
| §1 概述（C「发现跑两次」对照段） | 开发构建报错、发布构建静默覆盖 | 两种构建形态下都 panic |
| §5.4 M9 覆盖表 | 断言在类型内（`debug_assert!`）；R04-T07 未销 | 断言在类型内（`assert!`）；R04-T07 已由 `types::cell::frozen_tests::test_freeze_twice_panics_and_keeps_first_value` 销账 |
| §5.5 R04-T07 建议行 | 「`Frozen` 二次 `freeze` 的 debug 断言测试」 | 注记已销与销账测试名 |

不动的东西：`Frozen` 的 `Sync` bound（`T: Send` 而缺 `T: Sync`）仍是 `D04-design-batch` 的 DESIGN-01 项，与本条并账无关。

- 行数：2737（与 R3 时点一致，改动全为等行数替换）
- 文档 sha256：`c66408797925f85d299700b883dd39db84a1d645c5088e4d92606daa643e1ae8`（本段追加前的 R3 值 `90179b7b…` 已被这次同步替换）
- 门：`--round R3` 仍 PASS（11 PASS / 2 WARN / 0 FAIL）；anchor 77/77；structure/ref/style(error=0) 通过；doc-code-map 54/54

## R39 确认轮（V3.10，append-only；2026-11-22）

- 轮结果：`CONVERGED`。
- 文档：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`，sha256 `4d7f41b51cf7dad9ce3689895a5defc114ce8d71258c8fb2b74f29457f2b8f39`，2979 行。
- 单元账本：54/54 frozen、reopened=0、stale=0、T3 PASS；`audit` 全部单元 open=0。
- 确认轮：11 个冻结单元抽样；15 条发现（6 P1 + 6 P2 + 3 P3）全部修复，`DELTA: PASS`；修复 2/2 批次，未开新修改轮。
- 文档七门：publication/tutorial 0 新增；structure/ref/style/density 通过；citation `68 checked/0 failed`；anchor 70/70；doc-code-map 53/53。
- 代码门：53 个 related-code code-publication 0 命中、CJK 0；minix-arch 266/0、minix-platform 19/0、minix-kernel mock 866/0/8。
- 真机：test-smp-topo x86_64/aarch64/riscv64 三腿 PASS。
- 账本：D04 TODO 匹配 0 行，无未闭环项。
