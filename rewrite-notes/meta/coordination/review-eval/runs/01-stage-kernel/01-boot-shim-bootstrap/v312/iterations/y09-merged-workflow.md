# y09 — 合并章流程：切片提取器、每章摘要、输入限定

- 任务书：`tmp/review-loop/01-stage/PROMPT-merged-chapter-workflow.md`。
- 增量：
  1. `tools/extract-chapter-slice.py`（`--chapter`/`--list`/`--stdout`/`--self-test`；围栏外切章；
     默认 `tmp/review-loop/01-stage/chapters/<doc>/chN.md`）；CI 章契约门加入 self-test 与 D01 Ch1 活切片。
  2. 每个冻结章写 `runs/<stage>/<doc>/v312/summaries/chN-summary.md`（50–80 行、带锚点）；
     六视角只拿“切片 + 契约 + 上一章摘要 + scope”，正确性 review 定向读源码但不读全文；
     最终整篇六视角是唯一全文读。
  3. D01 Ch1 六视角可继承，不因 workflow 修改重跑（除非正文变化）；D01 Ch1–Ch6 都要章级六视角 + 冻结 + 摘要。
- 已落：extractor + self-test；CI 接入；`review-chapter-contract.md` §三改写；回归证据增补。
- 独立 verifier 首轮 FAIL：brief 输入限定不一致（导航者缺契约/scope、效率读者缺 scope、
  Reader-B 缺 outline；brief 门不拦）。已补三份 brief 与 `chapter-brief-check.py` 的四件输入/
  outline 断言及反向自测；复验 `WORKFLOW-REGRESSION-MERGED-RECHECK.md` 运行中。
- 事故：验证期间 `minix3/minix/include/arch/i386/include/vm.h` 被并发进程截为 0 字节；
  已 `git checkout --` 恢复为 103 行，`git status minix3/` 干净。未提交。
- D01 Ch1：G18 在 `1e5c3827` 上仍判 HARD 1（I4 两项检查不可操作、I3 时序无定义，示例“四问全过”不可复核）。
  已改为四项可演示检查（长度/布局/版本/来源），I3 时序定义为“记录只写一次、交给内核后不再修改”，
  示例逐项复算；新切片 `ch1-final-44ea30fd.md`（133 行，extractor 产出），
  六视角 + Reader-B + theory 按该哈希重跑；摘要 `summaries/ch1-summary.md`（67 行）已写。
- D01 Ch2：fact 门在 `ch2-final-08f0715e.md`（910 行）重跑；首轮 F1–F6/N1 已修。
- 下一步：Ch1 44ea 全过后写 `CHAPTER-FREEZE.md` 的 Ch1 节；Ch2 fact PASS → 章切片六视角 → 冻结 → 摘要；
  Ch3–Ch6 同流程；D01 整篇六视角 → 第一次 meta-review KEEP → D02。
