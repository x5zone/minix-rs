# study-notes — 早期 Minix3 学习笔记

> **创建**: 2026-10-07（目录迁移当天）。本区自建档起**冻结**，不再新增内容。

这里是在动手重写之前，对着 `minix3/` 里的 C 源码做的一批学习笔记：按主题分子目录
（`arch/`、`boot/`、`interrupt/`、`ipc/`、`pm/`、`process/`、`services/`、`syscall/`、`vfs/`、`vm/`），
另有学习路线与进度类文件放在本层（`learning-path.md`、`roadmap.md`、`progress.md`、
`minix_structure.md`、`daily.md`），以及若干 `archive/` 子目录里的逐文件笔记。

## 使用规则

1. **不能当事实依据引用。** 这批笔记由 AI 生成，未逐篇与 C 源码核对过。要查 Minix3 的真实行为，
   直接读 `minix3/` 下的 C 源，或读 `rewrite-notes/` 里带锚点的重写文档。
2. **正式文档不引用本区。** 重写文档与设计文档需要自包含，引用一律指向 C 源码或代码符号。
3. **可以当线索读。** 它们记录了当时对 Minix3 的整体印象与阅读路径，找方向时有参考价值；
   每一条具体结论都要自己验证。

## 后续处理

本区共 184 个文件（含本 README）。清理已在做的部分：`progress.md.backup_20260329_233603`
已删除——它是正本更早的分叉快照（缺「核心理念」等段落，不是重复拷贝），需要回看时用 `git show` 该路径即可取回。剩下的精简（去掉 `.bak` 备份、合并重复主题、删除已被 `rewrite-notes/` 取代的篇目）仍是独立任务，**不在本次目录迁移里做**：迁移的验收标准只有「内容零丢失、引用零断裂」，
掺进删除动作就会让验收同时需要证明「删的都是该删的」。
需要找回本区迁移前的状态：annotated tag `notes/pre-migrate-20261007`，
逐文件清单与 sha256 见 `tmp/migrate_notes_plan/pre-migrate-20261007/manifest.notes.disk.tsv`。
