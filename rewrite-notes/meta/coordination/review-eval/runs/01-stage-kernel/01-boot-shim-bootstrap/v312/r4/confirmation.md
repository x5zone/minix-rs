DOC 01-boot-shim-bootstrap.md

# R4 确认轮报告（G18 学生冷读）

- 确认轮：R4
- 冷读报告：`G18-COLD-READ.md`（同目录）
- 文档：3,085 行，sha256 `67440cd885aad0e808cd5d76905323872b08b1c35849566b976c123d84dff6e8`
- 冷读结论：`HARD-REJECTIONS: 0`、`VERDICT: READY`
- 48 个单元（6 章 + 42 节）全部判“可跟读”；S1–S6 六项学习任务全部“能完成”，逐项证据见冷读报告第三节。
- 本次确认只读正文与学习范围表，未读源码、其他文档、评审轨迹或历史版本。

## 逐单元确认

冷读报告的逐单元表覆盖全部 48 个单元（`G18-COLD-READ.md` 第二节，1–48 行表），每行给出单元 ID、行范围、判定与备注；无“有卡点”单元。逐单元 `--confirm-review` 的证据文件在 `unit-confirm/<id>.md`，每条只点名一个单元并指向冷读报告的对应行。

## 重放

- `rg -n '^\\| [0-9]' G18-COLD-READ.md` → 48 行逐单元记录。
- `rg -n 'HARD-REJECTIONS: 0|VERDICT: READY' G18-COLD-READ.md` → 2 行。
- `sed -n '72,81p' G18-COLD-READ.md` → S1–S6 六项全“能完成”。
