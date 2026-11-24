DOC: rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
CHAPTER: 3. Rust 设计决策
HARD-REJECTIONS: 0
VERDICT: PASS

核对：切片 586 行，sha256 `b809bd9dfe0a69fd5c145715eed9fb35c5624e23cf75fc3f29006e2263e5525a`，与题面一致（mtime 05:47:39）。
定点复核（唯一项）：L515 现为“直载测试链不计入生产腿与‘四条有门链的腿’，但在‘三类链 6’与‘真机门链 5’两类计数里出现”。逐项对账：L522 的“三类链 6”含直载测试链并注明“不是生产腿”；L526 的“四条有门链的腿”= 五条真机门链去掉直载测试链；L528 明写“真机门链五条含直载测试链”。三者与 L515 一致。
旧句“只在‘三类链’里出现”已零命中；上一轮唯一 HARD 闭合。
结论：该句 PASS。本次只核这一句，未重读全章。
