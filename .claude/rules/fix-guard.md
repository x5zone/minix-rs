# Fix Guard — rules for SAFE code/doc fixes

## ⛔ Before EVERY fix, you MUST do these 4 things:

1. **Read the TARGET line ±5 lines** — do NOT fix from memory or from a report.
2. **Grep-confirm** the current state — `rg "PATTERN" FILE -n`
3. **Apply ONE fix** — then read the result to confirm.
4. **Write fix-status** — append to the fix list at bottom of the report.

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
