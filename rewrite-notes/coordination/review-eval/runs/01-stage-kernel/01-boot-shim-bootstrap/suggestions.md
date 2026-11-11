# D01 suggestions（R26）

> 本文件只收**清单外偏好/非缺陷建议**：不进 `defect-ledger.md`、不进 `metrics.md`、不阻塞收敛。
> 来源：`tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round26/*.md` 的 `## SUGGESTION` 段。
> 下一次 edition 边界可以把反复出现的模式晋升为规则或读者验收清单项。

## 已收（按角色）

- P-FACT-A：§4.6 引文循环头改写标注；§2.7 ARM32 省略指针收窄；§4.6 x86_64 空表主语补全。
- P-FACT-B：§4.2.2 `KNOWN` 的“16 位”措辞；§3.4.1 直核 startup 枚举措辞；§3.8 `pt_free` 回收链措辞。
- P-READER-A：§2.8 五条 bullet 化；§3.4.3 `repr(C)` 理由半句；§2.4 交叉引用“紧随其后”；§4.5 容量分组判据；文档/画像的 §1.3 导航句对齐。
- P-READER-B：见 `reader-b.md` 的 8 条 SUGGESTION（标题/缩写表/术语展开等）。
- P-READER-C：见 `reader-c.md` 的 10 条 SUGGESTION（表格摘要、全局计数口径、v9 注释措辞等）。
- P-DESIGN-D2：6 条“拒绝修改”的理由已写在 `design-d2.md` 的 VERDICT（避免下轮重复提）。

## 处置

- 本轮不修；不声称是缺陷。
- 下一 edition 边界按出现频次与读者影响排序，最多晋升 2 条为规则/清单项。
