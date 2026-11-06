# traps —— review-loop 对抗样本

> 这些文件是评测样本，不是内核代码，也不是正式文档。不要把它们复制进 `os/`、`minix3/` 或 `rewrite-notes/` 的正文。

## 使用方式

1. 只在版本边界运行，不每篇运行。
2. 把样本路径交给对应面板角色；**不要**把 `expected/` 目录交给任何审查者。
3. 审查者只读样本文件，报告写到 `tmp/review-eval/boundary-V<n>/traps/`。
4. 主 agent 或 meta-reviewer 拿报告与 `expected/T0x.md` 对照，记三项：命中、部分、漏报；阴性样本另记误报。
5. 结果写入 `meta/boundary-V<n>.md` 的 recall / false-positive / scope 表。

## 样本清单

| 样本 | 类型 | 期望 |
|---|---|---|
| `T01-translation-residue.rs` | 阳性 | 翻译残留、单实现 trait、非法状态 |
| `T02-dead-abstraction.rs` | 阳性 | 死抽象，但必须问测试接缝 |
| `T03-arch-leak.rs` | 阳性 | 硬件机制泄漏到共享层 |
| `T04-duplicate-state.rs` | 阳性 | 重复状态、非法组合可表示 |
| `T05-sentinel-error.rs` | 阳性 | 哨兵返回值 |
| `T06-doc-incomprehensible.md` | 阳性 | 读者理解障碍与证据缺失 |
| `T07-frozen-decision.md` | 阴性 | 不许重提冻结决策 |
| `T08-adjacent-scope.md` | 阴性 | 相邻问题只登记，不阻塞 |

## 评分口径

- 阳性样本：命中期望条目至少一半算 PASS；少于一半算 PARTIAL；零命中算 FAIL。
- 阴性样本：零误报算 PASS；出现一次范围升级或重提冻结决策算 FAIL。
- 误报：报告了期望之外、且经复核不成立的问题，按条计；不作为“更严格”的加分项。
