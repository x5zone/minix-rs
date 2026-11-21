# D02 R4 post-certify 单行复核（§5.1 line 1120）

- 结论：**PASS（4/4 CLOSED）**，一致性 100%。
- 角色：冷启动只读复核者；未参与 R4 面板、修复、三轮 certify 与回归审计。
- 目标：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1120`（迭代器行为测试族的“证明什么”列）。
- 基线：文档 sha256 `48630c90b2a9e1eb2b2814f93ad61d44de0c8a6c634d4835ca25692c602eb781`，1294 行；该行修改已入库（`git diff -U0` 无输出）。
- 范围：只复核这一行，不重审 18/12 个判定点。

## 判定

1. **CLOSED** — `grep -c '三类畸形'` = 0；`grep -c '四类畸形'` = 1，命中 `:1120`。
2. **CLOSED** — `SegmentIter::next()`（`os/libs/minix-elf/src/lib.rs:393-446`）的 `Err` 值域恰好四类：`PhdrOutOfBounds`（`:399/:403`，及 `parse_phdr:272` 透传）、`PhdrEntryTooSmall`（`parse_phdr:269` 透传）、`SegmentOutOfBounds`（`:421/:424`）、`FileszExceedsMemsz`（`:432`）。`NoLoadSegments`、`InvalidSegment`、`TargetBufferTooSmall` 分别只在 `loader.rs:203`、`:196/:208/:353`、`:290` 由装载器返回，`next()` 不返回。
3. **CLOSED** — 四个变体名与枚举定义（`lib.rs:157-198`）逐字一致，无近似拼写。
4. **CLOSED** — 该族列出的 12 个 `test_segment_iter_*` 测试名在 `lib.rs` 中全部存在（`grep -n` 命中 12 个），无缺失。

## 结论

**PASS（4/4 CLOSED）**。§5.1 的“四类畸形以 `Err` 返回”与源码逐字吻合，无遗漏、无多余；该行修改不改变两轮 certify 的 18/12 个判定点结论。

## 限制

- 同模型冷读，无跨模型复核。
- 只判定该行的措辞与四个变体/12 个测试名的对应关系，不重跑测试套件；测试名存在性用 `grep` 核验，未执行 `cargo test`。
