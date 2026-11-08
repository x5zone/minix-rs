# Fix Guard — rules for SAFE code/doc fixes

> 归属：本文件是**规范源**。`.claude/rules/fix-guard.md` 是它的运行时副本（Claude Code 自动加载），内容同源；
> 改规则只改本文件，再同步副本。此前这份守则只存在于 `.claude/rules/`，而入口文档一直把「开工前必读
> fix-guard」列为硬性要求——属于运行时端比源端多内容的漂移，工作流优化批次已补正。

## ⛔ Before EVERY fix, you MUST do these 5 things:

1. **Read the TARGET line ±5 lines** — do NOT fix from memory or from a report.
2. **Grep-confirm** the current state — `rg "PATTERN" FILE -n`
3. **Apply ONE fix** — then read the result to confirm.
4. **Write fix-status** — append to the fix list at bottom of the report.
5. **Doc fixes: no process traces, pass the diff lint** — 文档类修复不得把过程痕迹（review 编号/日期/修复史/元注释/工具术语）写进正文；过程信息写进 report 的 fix-status；提交前跑 `tools/doc-style-lint.sh --diff`，零 error 命中才算完成（2026-09-18 A1.5/A3.1）。

## Fix Status Format (append after each fix)
```
### ✅ Fix #N: P0 — <description>
- **File**: `path/to/file`
- **Before**: `old line content`
- **After**: `new line content`
- **Verified**: `rg "PATTERN" FILE` → 1 match, correct
```

## ⛔ PROHIBITED
- Batch fixing without confirmation
- Fixing a line you haven't read first
- Fixing without verifying the result with grep
- Deleting content when you meant to replace
- Guessing the replacement text

## Safe Patterns
- Cross-reference fix: old filename → current filename (grep-ls confirm)
- SAFETY comment: `// SAFETY: <reason>`
- Truncation comment: `// value bounded by <limit>, safe for u32`
- Emoji replacement: ✅→"-", ❌→"×", ⚠️→"?"
- Missing `.md` extension: append it

## Code Refactoring Discipline (⑥ DESIGN items)

> P-DESIGN 发现的可实施代码改进标 ⑥，在本轮循环的 L3.5 实施。
> 重构不是"转给下一个人"的欠账——如果它值得报，就值得改。

### Before EVERY refactor:
1. **Write the design trade-off first** — 改哪些文件、为什么这样改、替代方案、拒绝理由。一行就够，但必须写在 fix-status 里。
2. **Confirm the current API surface** — `rg "pub fn|pub struct|pub trait" FILE` 确认调用方。
3. **Run the tests BEFORE changing** — `cargo test -p <crate>` 记录基线。

### During refactor:
4. **One crate at a time** — 改完一个 crate 就跑它的测试，绿了再碰下一个。
5. **同步文档** — 代码改完后，找到文档 §3/§4/§5 中描述被改结构的段落，立即更新。这是"文档与代码同步"门，不是可选步骤。

### After refactor:
6. **全量绿** — `cargo test -p <crate>` + `cargo clippy -p <crate>` 全通过才提交。
7. **文档门** — 五门全绿。

### ⛔ PROHIBITED (refactoring)
- 标 ② 然后写"修于下一轮"——要么本轮 ⑥ 实施，要么给出跨 crate 协调的**具体理由**并立独立项目（有负责人与截止轮次）。
- 改了代码但没改文档——"文档与代码同步"是硬门。
- 改了文档但没跑五门——门绿才证明没坏。
