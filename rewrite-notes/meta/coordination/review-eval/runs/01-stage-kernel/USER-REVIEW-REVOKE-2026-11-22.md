# 用户人工审阅裁决：撤销 D01–D03 的 CONVERGED（2026-11-22）

## 用户原话

> 首先，将D01~D03全部改成不收敛=。=
> 这是我人工审阅D01之后的观点，我根本读不进去

## 裁决

- D01 `01-boot-shim-bootstrap`、D02 `02-higher-half-kernel`、D03 `03-kmain-cstart` 的 `CONVERGED` 全部撤销。
- 三篇最终状态一律改判 `NOT CONVERGED`（裁决号 D-116）。
- 撤销理由是读者可读性：用户人工审阅 D01 后无法读进去。机械门（2 轮 full、逐单元确认、trajectory、T3）通过不等于出版验收通过。

## 保留的历史事实

- 三篇的轮次、单元冻结、逐单元 `confirm-review`、trajectory PASS 记录不删除、不追溯改写。
- `review-units.py gate --fail` 已把三篇 T3 覆盖为 FAIL，证据为本文件。
- 已冻结单元暂不解冻；后续修复方案等用户进一步指令，不在本记录中预设。

## 可重放

```bash
python3 tools/review-units.py stats rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
python3 tools/review-units.py stats rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
python3 tools/review-units.py stats rewrite-notes/01-stage-kernel/03-kmain-cstart.md
```

依据：D-116；用户 2026-11-22 会话原话。
