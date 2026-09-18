# Fix Guard — rules for SAFE code/doc fixes

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
