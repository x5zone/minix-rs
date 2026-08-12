# 22-privilege-outline-review — outline 自审 + 文档正文评审依据

> **审阅对象**: `design/22-outline.md`
> **审阅日期**: 2026-08-01
> **审阅者**: Trae (GLM-5.2)
> **用途**: ①审 outline 4 维自审 ②review 阶段审文档正文（doc 是否遵循 outline）

---

## 一、outline 4 维自审

### 维度 1: 教学性（读者能否建立概念模型？）

| 检查项 | 判定 | 证据/理由 |
|--------|------|---------|
| Ch1 主语是概念（权限/特权）非函数名 | ✅ | "内核如何控制进程能做什么？" |
| 每节有"灵魂本质"一句话 | ✅ | §1.1-§1.5 均有 |
| WHY→WHAT→HOW 弧线 | ✅ | §1.1 为典型（为何需要/什么是分治/如何实现） |
| 新概念有定义+动机 | ✅ | priv/s_flags/掩码/静态动态区均有定义 |
| 无迭代叙事 | ✅ | 无"旧版/最初/后来/我们改成" |
| 无 tmp 文件引用 | ✅ | 来源仅为 C 源码 |

### 维度 2: 本质深度（触及理论本质？）

| 检查项 | 判定 | 证据/理由 |
|--------|------|---------|
| 解释"为什么这样设计" | ✅ | §1.1 空间效率 vs 权限隔离的权衡 |
| 区分机制 vs 策略 | ✅ | 位图是机制，预定义组合是策略 |
| redox 对照 | ✅ | §1.1 scheme/capability 模型对照 |
| 关键不对称说明 | ✅ | §1.2 CHECK_IPC 无标志位 vs CHECK_IO_PORT 有 |
| C bug/特殊性说明 | ⚠️ | 未发现明显 C bug；fork 降级语义已说明 |

### 维度 3: 概念覆盖（C 源码覆盖完整？）

| C 概念 | outline 覆盖? | 备注 |
|--------|--------------|------|
| struct priv 25 字段 | ✅ §1.1+§2.2 | 全部列出 |
| s_flags 11 位 | ✅ §1.2+§2.1 | 完整 |
| 预定义组合 8 个 | ✅ §1.2+§2.1 | IDL_F..IMM_F |
| s_ipc_to/s_k_call_mask/s_trap_mask | ✅ §1.3+§2.1/§2.2 | 三类掩码 |
| may_send_to/may_asynsend_to | ✅ §1.3+§2.2 | 宏 |
| static_priv_id/is_static_priv_id/USER_PRIV_ID | ✅ §1.4+§2.1 | ID 映射 |
| priv 表布局（静态/动态区） | ✅ §1.4 | 布局图 |
| s_io_tab/s_irq_tab/s_mem_tab | ✅ §1.5+§2.2 | 三类范围表 |
| get_priv/do_privctl | ✅ §2.3 | system.c 函数 |
| s_alarm_timer | ✅ §2.2+D8 | 闹钟 |
| s_ipcf | ✅ §2.2+D9 | IPC filter 指针 |
| s_grant_table | ✅ §2.2 | grant 表 |
| s_sig_mgr/s_bak_sig_mgr | ✅ §2.2 | 信号管理器 |
| s_notify_pending/s_asyn_pending/s_int_pending/s_sig_pending | ✅ §2.2 | 挂起位图 |
| s_stack_guard/s_diag_sig | ✅ §2.2 | 栈保护/诊断 |
| s_state_table | ✅ §2.2 | state 表 |
| s_asyntab/s_asynsize/s_asynendpoint | ✅ §2.2 | 异步表 |
| s_init_flags | ✅ §2.2 | 初始化标志 |

**覆盖率**: 18/18 C 概念全覆盖 ✅

### 维度 4: 组织合理性（章节结构合理？）

| 检查项 | 判定 | 证据/理由 |
|--------|------|---------|
| Ch1→Ch2→Ch3→Ch4→Ch5 叙事弧完整 | ✅ | 起(概念)→承(C源码)→转(设计)→合(实现/测试) |
| 章节无重复 | ✅ | 各节职责正交 |
| 概念依赖无倒置 | ✅ | priv→s_flags→掩码→ID分配→范围表，递进 |
| 覆盖矩阵无空行 | ✅ | A-G 七组均映射到 Ch1-Ch5 |
| 参见章节闭环 | ✅ | 引用 11/17/18/20/21/23 |

### 自审结论

- **P0**: 0
- **P1**: 0
- **P2**: 1（D9 s_ipcf/s_stack_guard 用 Option<usize> 是已知限制，标 P2 不阻塞）
- **判定**: ✅ 自动批准（P0=0）

---

## 二、doc ↔ outline 对齐检查（review 阶段使用）

> 当 review 22-privilege.md 时，用本表检查文档正文是否遵循 outline。

| outline 小节 | outline 规定的知识点 | 文档实际覆盖? | 偏离类型 | 严重度 |
|-------------|---------------------|--------------|---------|--------|
| §1.1 priv 模型 | 系统进程独立/用户共享/三类掩码 | ✅ §1.1+§1.4 | 一致 | — |
| §1.2 s_flags 11 位 | 含 CHECK_IPC 无标志位说明 | ✅ §1.2 注释 | 一致 | — |
| §1.3 三类掩码 | s_ipc_to/s_k_call_mask/s_trap_mask | ⚠️ §1.3 未单独成节，散落在 §1.1 | 遗漏 | P1 |
| §1.4 ID 分配 | static_priv_id/USER_PRIV_ID/静态动态区 | ✅ §1.4+§补充 | 一致 | — |
| §1.5 I/O/IRQ/MEM | 三类范围表+CHECK 标志 | ⚠️ 文档未单独成节 | 遗漏 | P2 |
| Ch2 C 源码 file:line | priv.h+system.c 锚定 | ⚠️ 行号为近似/缺失 | 不足 | P1 |
| Ch3 D1 6 子结构 | hypothesis-driven | ❌ 文档 Ch3 是平庸决策表 | 坏偏离 | P0 |
| Ch3 D3 CapabilityTemplate | 模板授予 | ❌ 文档未提 | 遗漏 | P0 |
| Ch3 D4 双系统 | ProcessCapability vs PrivFlagsBits | ❌ 文档未提 | 遗漏 | P0 |
| Ch3 D5 Newtype | IpcMask/KCallMask/TrapMask | ❌ 文档未提 | 遗漏 | P1 |
| Ch4 §4.3 6 子结构 | 真实代码 | ❌ 文档 §4.2 是扁平字段列表 | 坏偏离 | P0 |
| Ch4 §4.5 capability.rs | CapabilityTemplate+Newtype | ❌ 文档未提 | 遗漏 | P0 |
| Ch5 测试 | 37 个 grep 可验函数名 | ⚠️ 文档 §测试 是 bullet 非函数名 | 不足 | P1 |
| 补充 tmp 来源 | 禁止 tmp 引用 | ❌ 文档 §补充 引用 tmp-11-privilege.md | 坏偏离 | P0 |

**偏离统计**: 6 个 P0 + 3 个 P1 + 1 个 P2

**结论**: 文档与 outline 严重偏离（6 P0），**判定为 rewrite 级**——Ch3/Ch4/§补充 需彻底重写。
