---
name: "full-review"
description: "文档+代码的全面 review 与修复：修错误 bug、关注覆盖度。默认章节级范围。当用户说 review/scan/check 一个文档、章节或目录，或要求对照 Minix3 C 源码查遗漏时调用。"
---

# full-review

## 目标
修错误和 bug，关注覆盖度。文档和代码完成后按流程走一遍。

## scope（用户未指定时确认）
`chapter`（默认推荐）/ `section` / `doc` / `range` / `dir`。range/dir 逐 doc 推进，禁止一轮吞下。

## 执行
按 `prompt/review-rules/review-cmds.md` §二 进入 `prompt/review-rules/review-process.md` Step 0-7（按范围裁剪）；scope=range/dir 时 coverage 穷举（Gate A）强制。
Step 0 必须产出**关联代码清单**（`tools/doc-code-map.sh {doc}.md --check`，进 scan.md `§Step 0: 预检结果` 段）；**Step 3.6 必须执行**（代码维度表 + `gate-evidence-code` 块，含无发现声明）。目标为编号文档时语义 = 文档 + 关联代码完整 review（模式 C）。

## 强制门
锚点纪律门（模式 83）+ 测试名对账门（Gate E/Step 4.5a）+ Gate B/C/D/E/H（doc 及以上）+ Ground Truth 链（Minix3 C 源码优先）。

## 不做（发现即记录，不当场做）
卓越性重构、文风重写、自动新建文档——分别引导 code-excellence / style-fix / 用户决策。

## 产物
P0/P1/P2 清单（锚点 + 反查维度）+ **关联代码清单 + 代码维度结论（含无发现声明，gate-evidence-code）** + 修复标注 + scan/VERIFY-CHECK（doc 及以上）。文档类改动过 `tools/doc-style-lint.sh --diff` 零 error 文风增量门。
