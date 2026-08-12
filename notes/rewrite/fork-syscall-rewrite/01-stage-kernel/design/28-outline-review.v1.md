# 28-usermapped-data: 大纲评审（Outline Review v1）

> **文档**: `28-usermapped-data.md`
> **状态**: v1 快照（2026-08-12）
> **评审维度**: 4 维（架构视角 / 教学深度 / 跨文档闭环 / Minix3 对齐）

---

## 评审矩阵

| # | 维度 | 检查项 | 判定 | 备注 |
|---|------|--------|------|------|
| 1 | 架构视角 | Ch1 从 CPU/OS perspective 提出核心问题 | ✅ | "内核如何安全暴露信息给用户态" |
| 2 | 架构视角 | Ch1 概念优先于实现 | ✅ | 先讲共享内存 vs 系统调用策略，再讲 C 实现 |
| 3 | 教学深度 | 概念首次出现有定义 + motivation | ✅ | `.usermapped` section 首次出现有定义 + "为什么需要" |
| 4 | 教学深度 | redox 对照提供多视角 | ✅ | §1.4 三方对照 |
| 5 | 跨文档闭环 | 前序引用完整 | ✅ | 02/06/07/09 全部引用 |
| 6 | 跨文档闭环 | 后续引用完整 | ✅ | 13/15/25 引用 |
| 7 | Minix3 对齐 | C 源码文件清单完整 | ✅ | 6 个文件全覆盖 |
| 8 | Minix3 对齐 | 8 个数据结构字段语义 | ✅ | §2.2 详述 |
| 9 | Minix3 对齐 | ARCH 演进标记 | ✅ | D1-D5 明确标记 64-bit 决策 |
| 10 | Minix3 对齐 | 不引入 C 兼容/FFI 内容 | ✅ | 纯 Rust 64-bit 重写视角 |
| 11 | 设计决策 | 每个决策有 C 行为 + Rust 对应 + 理由 | ✅ | D1-D5 三段式 |
| 12 | 设计决策 | WONTFIX 项明确标注 | ✅ | §4.2 列出 |

---

## 潜在问题

### P1: Ch2 §2.2 `kinfo` 结构字段过多（~50 字段）
- **问题**: C 的 `struct kinfo` 有约 50 个字段，全列出会过载
- **方案**: 只列关键字段（memmap/vir_base/proc_count/user_sp/freepde_start），其余引用 type.h
- **判定**: ✅ 可接受（教学导向，非 API 参考）

### P2: IPC trampoline 汇编细节深度
- **问题**: 21 个 trampoline 函数的栈布局/寄存器约定细节是否需要全列？
- **方案**: 只讲一套（syscall），其余两套（softint/sysenter）只列差异
- **判定**: ✅ 可接受（64-bit 只用 syscall）

### P3: `minix_kerninfo` 顶层结构的 ABI 兼容性
- **问题**: C 注释提到 `kinfo.user_sp` at offset 2440 被 legacy user binaries 依赖
- **方案**: 在 §3.1 D1 中说明"64-bit 不保留 legacy ABI 兼容"
- **判定**: ✅ 可接受（minix-rs 是完整重写，无 legacy binary 兼容需求）

---

## 结论

大纲 v1 通过评审，可进入 design v1 阶段。关键决策 D1-D5 明确，WONTFIX 项清晰，跨文档引用闭环。
