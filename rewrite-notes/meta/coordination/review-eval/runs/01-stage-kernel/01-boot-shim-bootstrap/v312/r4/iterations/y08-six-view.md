## y08 — 六视角守门：D01 5/6 通过，编辑视角要求体裁分离

- 作者新增六条读者体验规则；workflow 已落：`tutorial-rubric-check.py` 检查 N.0≤2、小结/术语位置、
  单段>500 字 WARN；§4.0e 加第 11 条；READER-B 报告要求“重建结构 + 与节序对比表”；规则六记录。
- D01 规则一/二追溯：六章 N.0 只留问题入口+前置概念，小结/术语移章末，最小示例/失败对照归位；
  八门全绿；补 `FREEZE.md`。
- D01 规则三/四/五回归：五组复述压到权威段、15 处证据标签化、21 个长段拆分；
  `REGRESSION.md` 记录；正文净减约 1.1k 实义字符。
- 六视角（当前 doc `9d6e18a5…`，3,142 行）：
  - beginner-me `HARD 0 / READY`；G18 `HARD 0 / READY`；导航者 `READABLE`；
    效率读者 `READABLE`；教学设计师 `READABLE`；
  - 编辑 `NOT-READABLE`：主线内部仍夹着 §3.2 的 155 行账本，要求物理拆成主文+参考。
- me-agent 裁决：不在一轮内把 3,142 行正式文档拆成主文+附录（会改单元/锚点/review 账户）；
  方案 A 是新增 600–800 行 `01-boot-shim-bootstrap-mainline.md` 伴生篇，细节用指针回完整篇，
  完成后重跑六视角。D01 状态写作 `CONVERGED（评审门）；六视角 5/6，编辑待主线篇`。
- D02：N.0 已按规则一重构；R1 面板 53 FINDING（P0 5 已清，48 open）；规则三/四/五未做，
  R1 保持 `ROUND-OPEN`；当前循环按作者要求停在 D02。
