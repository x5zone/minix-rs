# D04 FREEZE —— 04-platform-discovery（CONVERGED）

- 冻结日期：2026-11-12
- 工作流 edition：V3.3
- 确认轮：R2（`轮型: confirmation`）
- 冻结提交：`bc232e627`
- 文档：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`
- 行数：2729
- 文档 sha256：`5ea4e0554403a63c1e002dcc12d5b4738ed11191d1d2e82b54d7b3915905c078`
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
- 文档 sha256 更新为 `5ea4e055…`；回归结论维持 **PASS**。
