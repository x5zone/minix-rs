# 24-cross-space-runtime Outline Review v1

> 本文件是 24-outline.v1.md 的自审 + 24-cross-space-runtime.md 正文的审稿依据。
> 双重用途：①审 outline ②审文档正文覆盖度。

---

## §A. Outline 自审（4 维）

### A.1 教学性
- ✅ Ch1 从"为什么需要"出发（VMREQUEST 的 WHY）
- ✅ 核心概念按依赖顺序引入（拷贝→VMREQUEST→VMSUSPEND→链表→挂起类型→Direct Map→CrossSpaceResult）
- ✅ 双向闭环表覆盖 VMREQUEST 进入/恢复
- ✅ "本章不讲什么"明确边界
- ⚠️ Ch1 §1 "三阶段抽象"应明确"阶段 1 失败"如何进入 VMREQUEST——重写时需在 Ch1 末尾加过渡段

### A.2 本质深度
- ✅ 抓到核心矛盾：内核代行时缺页 → 无 handler → 需 VM 协助
- ✅ 区分 VMSUSPEND（信号）vs EFAULT（错误）——C 混为 int，Rust 用 enum 分离
- ✅ 区分 VMREQUEST（内核代行）vs PAGEFAULT（用户态执行）
- ⚠️ 未深入讨论"为什么内核不能自己处理缺页"——Ch1 应加 1 段：内核无缺页 handler 的设计理由（避免递归 fault）

### A.3 概念覆盖
- ✅ 覆盖 C 源码 9 个核心元素（见矩阵）
- ✅ 覆盖 Rust 实际实现（cross_space_copy / AddressRef / CrossSpaceResult / VmCopyContext）
- ✅ 包含 redox 对照空间（D1 Direct Map vs C 临时 PDE；D2 enum vs C int）
- ⚠️ 未覆盖 `virtual_copy_f`（memory.c:592，C 中 actual 实现）——Ch2 应补一段

### A.4 组织合理性
- ✅ Ch1→Ch2→Ch3→Ch4→Ch5→Ch6 顺序合理
- ✅ Ch3 设计决策可追溯 Ch1&2
- ✅ Ch4 实现对应 Ch3 决策
- ⚠️ Ch4 §7 "未实现部分" 应改为 "已知缺口"（避免开发记录风格）

### 自审判定：P0 = 0，自动批准 ✅

---

## §B. 文档正文覆盖度（post-rewrite 验证用）

### B.1 Ch1 概念章
| outline 知识点 | 文档覆盖? | 偏离类型 | 严重度 |
|--------------|---------|---------|--------|
| 三阶段抽象 | 待验证 | — | — |
| VMREQUEST 机制 | 待验证 | — | — |
| VMSUSPEND 语义 | 待验证 | — | — |
| vmrequest 链表 | 待验证 | — | — |
| 三种挂起类型 | 待验证 | — | — |
| 本章不讲什么 | 待验证 | — | — |

### B.2 Ch2 C 源码分析
| outline 知识点 | 文档覆盖? | 偏离类型 | 严重度 |
|--------------|---------|---------|--------|
| syslib.h 宏 | 待验证 | — | — |
| com.h 常量 | 待验证 | — | — |
| vm.h VMSUSPEND | 待验证 | — | — |
| do_copy.c | 待验证 | — | — |
| memory.c data_copy_vmcheck | 待验证 | — | — |
| memory.c virtual_copy_vmcheck | 待验证 | — | — |
| proc.h p_vmrequest | 待验证 | — | — |
| proc.c vm_suspend | 待验证 | — | — |

### B.3 Ch3 设计决策
| 决策 # | 文档覆盖? | 偏离类型 | 严重度 |
|--------|---------|---------|--------|
| D1 Direct Map | 待验证 | — | — |
| D2 CrossSpaceResult | 待验证 | — | — |
| D3 AddressRef | 待验证 | — | — |
| D4 caller 显式 | 待验证 | — | — |
| D5 VmCopyContext | 待验证 | — | — |
| D6 VmRequestQueue | 待验证 | — | — |
| D7 删除 dispatch_datacopy | 待验证 | — | — |
| D8 统一 CopyResult | 待验证 | — | — |
| D9 trait 静态分派 | 待验证 | — | — |

### B.4 Ch4 实现 + Ch5 测试 + Ch6 参见
（post-rewrite 验证）

---

## §C. 反查一致项

| 反查维度 | 一致性 | 说明 |
|---------|--------|------|
| outline ↔ design | 待验证 | — |
| design ↔ C 源码 | 待验证 | — |
| design ↔ Rust 代码 | 待验证 | — |
| outline ↔ 文档正文 | 待验证 | — |

---

## §D. Open Questions

| OQ ID | 反查维度 | 两侧方案 | AI 倾向 |
|-------|---------|---------|---------|
| OQ-1 | D7 删除 vs 保留 dispatch_datacopy | 删除 vs 保留作教学示例 | 倾向删除（C 无 SYS_DATACOPY） |
| OQ-2 | D8 CopyResult 统一方向 | 删除 CopyResult 用 CrossSpaceResult vs 反向 | 倾向用 CrossSpaceResult（已含 VmFaultType） |
