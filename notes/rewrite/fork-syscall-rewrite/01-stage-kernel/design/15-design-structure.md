# 15-clock-timer Design Structure（设计骨架）

> **状态**: 骨架（待 outline-review 后填实为 15-design.md）
> **创建**: 2026-07-31
> **作者**: Trae (GLM-5.2)
> **前置**: 14-exception-interrupt.md, 05-clock-interrupt-init.md, 11-scheduling-primitives.md
> **C 源码**: `minix3/minix/kernel/clock.c`, `minix3/minix/kernel/system/do_vtimer.c`, `minix3/minix/kernel/system/do_setalarm.c`
> **Rust 实现**: `os/kernel/src/clock.rs`, `os/kernel/src/syscall_clock.rs`

---

## 1. 主题思想（一句话）

> 时钟中断是内核时间的唯一推进源：BSP 独占全局时间维护（uptime/realtime/闹钟队列），AP 只做本地进程记账；定时器到期与量子耗尽是两条独立的调度激活路径。

**删掉这一章，读者对系统的理解缺了什么**：
- 不知道内核"时间感"从何而来（为什么需要 100Hz 中断而非自由运行）
- 不知道 uptime（单调）与 realtime（可调）的语义差异及 adjtime 的渐进调整机制
- 不知道同步闹钟（系统进程用）与虚拟/性能定时器（用户进程用）是两套独立机制
- 不知道 BSP/AP 在时钟子系统的职责分离（SMP 时间一致性）

---

## 2. 目标读者

- 内核开发者（实现/维护时钟子系统）
- OS 学习者（理解时钟中断如何驱动调度、记账、定时器）
- 前置知识：中断与异常（14-doc）、进程结构（KProcess 字段）、调度原语（11-doc）

---

## 3. 章节大纲

| 章 | 标题 | 核心命题 | 预计行数 |
|----|------|---------|---------|
| 1 | 概念与动机 | 时钟中断为什么存在、做什么、不做什么 | 80 |
| 2 | C 源码分析 | timer_int_handler 的 7 步流程 + 数据结构 + 调用关系 | 120 |
| 3 | Rust 设计决策 | D1-D11 多方案对比（优中选优） | 150 |
| 4 | 实现要点 | ClockState / TimerQueue / tick / vtimer 代码片段 | 100 |
| 5 | 测试策略 | L1 对偶 / L2 契约 / 边界用例 | 60 |
| 6 | 与其他模块的关系 | 双向链路：05/10/11/14/16/21 | 40 |
| 7 | 本章不讲什么 | 预期管理：硬件定时器配置归 05/14，quantum 策略归 11 | 20 |

**总预计**: ~570 行（单章可接受，超 500 但主题统一不拆分）

---

## 4. 设计决策清单（D1-D11）

| # | 决策 | 当前实现 | 推荐方案 | 状态 |
|---|------|---------|---------|------|
| D1 | 全局时钟状态封装 | `ClockState` struct | **保留 ClockState** | 待详 |
| D2 | 定时器队列数据结构 | `BTreeMap<u64, TimerEntry>` | **BTreeSet<(u64, TimerId)> + HashMap<TimerId, TimerEntry>** | 待详 |
| D3 | 定时器 identity | exp_time 作 key | **TimerId newtype 作 identity** | 待详 |
| D4 | adjtime 机制 | 保留 | **保留** | 待详 |
| D5 | 负载平均 | 保留 | **保留** | 待详 |
| D6 | `cause_alarm` 回调 | `TimerAction` enum | **保留 enum，删除 KernelCallback 变体** | 待详 |
| D7 | `hz` 配置 | 编译时常量 + 运行时可覆盖 | **保留** | 待详 |
| D8 | BSP/AP 分支 | `PerCpuTick` const IS_BSP | **per-CPU ClockState 实例**（SMP 兼容） | 待详 |
| D9 | tick() quantum 职责 | tick 内 `quantum.consume(1)` | **移除 quantum，归 switch_to_user（10-doc）** | 待详 |
| D10 | billp 记账接口 | `_is_billable` 未使用 | **显式 `billp: Option<&mut KProcess>` 参数** | 待详 |
| D11 | vtimer_check 函数去留 | 独立函数 + tick 内重复 | **删除独立函数，tick 内 `tick_virt_timer`/`tick_prof_timer` 已处理** | 待详 |

---

## 5. 各决策方案对比骨架（待 design.md 详填）

### D1: 全局时钟状态封装

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. `static mut` 全局变量 | 直接对应 C `kclockinfo` | translate 直观 | 违反 Rust 安全模式；多字段散落 |
| **B. `ClockState` struct（当前）** | 封装为 struct，BKL 保护下可变 | 封装性；字段聚簇 | — |

**选定 B**（保留）。理由：封装性 + BKL 保护下无需 interior mutability。

### D2: 定时器队列数据结构

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. `BTreeMap<u64, TimerEntry>`（当前） | exp_time 作 key | O(log N) | 同 exp_time 多 timer 覆盖 |
| B. `BTreeMap<u64, Vec<TimerEntry>>` | exp_time → Vec | 支持同 exp_time | 删除需扫 Vec；碎片 |
| **C. `BTreeSet<(u64, TimerId)>` + `HashMap<TimerId, TimerEntry>`** | 二级索引 | stable identity + 排序 + 唯一 | 双索引维护 |
| D. `BTreeMap<TimerId, TimerEntry>` | TimerId 作 key | stable identity | 查到期需扫全表 |
| E. `SlotMap<TimerId, TimerEntry>` | SlotMap | O(1) + stable id | no_std 生态弱 |

**选定 C**。理由：BTreeSet 按 (exp_time, id) 排序支持 O(k log N) 到期扫描 + TimerId 唯一支持 reset_timer(id)。

### D3: 定时器 identity

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. exp_time 作 key（当前） | C `minix_timer_t *tp` 的 translate 误用 | 简单 | 同 exp_time 无法区分；reset 误用 |
| **B. `TimerId(u64)` newtype** | stable identity | 类型安全；支持 reset(id) | 需生成 id |

**选定 B**。理由：C 用 timer 指针作 stable identity，Rust 用 newtype 替代指针语义。

### D4: adjtime 机制

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| **A. 保留（当前）** | `adjtime_delta` + `uptime & 0x1` 隔 tick | NTP 兼容；与 C 对齐 | — |
| B. 删除 | — | 简化 | 丢失 NTP 支持 |

**选定 A**（保留）。

### D5: 负载平均

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| **A. 保留（当前）** | `LoadInfo` + circular buffer | 与 C 对齐 | — |
| B. 简化 | 仅就绪队列计数 | 简化 | 丢失历史 |

**选定 A**（保留）。

### D6: `cause_alarm` 回调

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. 函数指针 `tmr_func_t` | C 原始 | — | unsafe；类型不安全 |
| **B. `TimerAction` enum（当前）** | `NotifyAlarm { endpoint }` | 类型安全 | — |
| C. trait object `Box<dyn TimerHandler>` | 动态分发 | 扩展性 | 堆分配；no_std 不友好 |

**选定 B**（保留），但**删除 `KernelCallback { id }` 变体**（死代码，无调用方）。

### D7: `hz` 配置

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. 运行时环境变量 | C 原始 | 灵活 | 运行时解析 |
| **B. 编译时常量 + 运行时可覆盖（当前）** | `with_hz()` | 编译期优化 + 灵活 | — |

**选定 B**（保留）。

### D8: BSP/AP 分支

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. 运行时 `cpu_is_bsp()` 分支 | C 原始 | SMP 兼容 | 每次中断有分支 |
| **B. `PerCpuTick` const IS_BSP（当前）** | 编译期单态化 | 零分支 | **SMP 单镜像无效**（BSP/AP 共用 binary） |
| C. per-CPU `ClockState` 实例 | BSP 实例有 timers，AP 无 | SMP 兼容 + 数据分离 | 内存开销 |
| D. Typestate + 运行时 downcast | 类型安全 + SMP | 复杂 | — |

**选定 C**。理由：SMP 单镜像模型下，BSP/AP 在运行时确定，编译期 const 无法区分。per-CPU 实例天然支持。

### D9: tick() quantum 职责

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. tick 内 `quantum.consume(1)`（当前） | — | — | **违反 C 语义**（clock.c:70-173 无 quantum） |
| **B. 移除 quantum，归 switch_to_user** | 与 C 对齐 | 正确性 | 需 10-doc 配合 |
| C. tick 返回 `enum TickAction` | 调用方决定 | 灵活 | 仍含 quantum 语义 |

**选定 B**。理由：14-doc §1.2 已 verify "timer_int_handler 不递减 quantum"；quantum 递减归 `switch_to_user`（10-doc）与调度原语（11-doc）。

### D10: billp 记账接口

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. `_is_billable: bool` 未使用（当前） | — | — | **缺失 C 关键行为** |
| **B. 显式 `billp: Option<&mut KProcess>` 参数** | 调用方传入 billp | 显式；对齐 C | 需调用方查找 billp |
| C. 传 `&mut ProcessTable` | 内部查找 billp | 接口简单 | 过宽；隐藏依赖 |
| D. 封装 `AccountingCtx` | billp 引用包装 | 可扩展 | 过度封装 |

**选定 B**。理由：显式传 billp 与 C `get_cpulocal_var(bill_ptr)` 语义对齐；调用方（trap 入口）已知 billp。

### D11: vtimer_check 函数去留

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| A. 保留独立 `vtimer_check` 函数（当前） | — | — | 与 tick 内 `tick_virt_timer`/`tick_prof_timer` 重复 |
| **B. 删除独立函数** | tick 内已递减+检查 | 无重复 | 进程退出清理需新函数 |
| C. `vtimer_check` 改为 `vtimer_cleanup_on_exit` | 退出时用 | 语义清晰 | — |

**选定 B + C 组合**：删除当前 `vtimer_check`（与 tick 重复），如需进程退出清理新增 `vtimer_cleanup_on_exit`。

---

## 6. 与相邻文档的关系

| 文档 | 关系 | 双向链路 |
|------|------|---------|
| 05-clock-interrupt-init | 05 讲 `init_clock()` 初始化，15 讲运行时 tick | 15 §7 加 05；05 §7 加 15 |
| 10-switch-to-user | 10 讲 quantum 递减 + RTS_NO_QUANTUM 设置 | 15 §1 引用 10 |
| 11-scheduling-primitives | 11 讲调度原语，15 讲时钟如何触发调度 | 15 §7 加 11；11 §7 加 15 |
| 14-exception-interrupt | 14 把时钟中断定位为"三条激活路径之一"，15 承接进入+返回 | 14 §1.2 → 15；15 §1 ← 14 |
| 16-smp | 16 讲 AP 定时器初始化，15 讲 BSP/AP 职责分离 | 15 §7 加 16；16 §7 加 15 ✅ |
| 21-syscall-clock | 21 讲时钟系统调用（用户态接口），15 讲内核实现 | 15 §7 加 21；21 §1 加 15 ✅ |

---

## 7. 本章不讲什么（预期管理）

- **硬件定时器配置**（8254 PIT / LAPIC Timer / ARM Generic Timer 初始化）→ 归 05-clock-interrupt-init.md + 14-exception-interrupt.md
- **quantum 递减策略与 RTS_NO_QUANTUM 设置** → 归 10-switch-to-user.md + 11-scheduling-primitives.md
- **时钟系统调用的消息解析**（SYS_SETALARM/SYS_VTIMER/SYS_STIME/SYS_SETTIME/SYS_TIMES）→ 归 21-syscall-clock.md
- **AP 启动时的定时器注册** → 归 16-smp.md
- **TSC 校准细节** → 归 04-platform-discovery.md（platform_desc.timer()）

---

## 8. 待 outline-review 确认的问题

1. **D8 per-CPU 实例**：是否需要 `ClockState` 按 CPU 实例化？还是保留 `PerCpuTick` 但改为运行时分支（方案 A）作为过渡？
2. **D9 quantum 移除**：10-switch-to-user.md 是否已实现 quantum.consume？需协调。
3. **D2/D3 TimerId 生成**：全局计数器还是 per-ClockState 计数器？
4. **D10 billp 传入**：trap 入口是否方便获取 billp？需确认 10/14 的调用链。
5. **章节拆分**：~570 行单章是否可接受？还是拆为 15a（概念+C 分析）+ 15b（设计+实现）？

---

## 9. 重写对齐清单（来自 review scan）

| P0/P1 | 修复项 | 对应决策 |
|--------|--------|---------|
| P0-1 | §1.4 行为规则 3 quantum 归因错误 | D9 |
| P0-2 | tick() quantum.consume 加料 | D9 |
| P0-3 | billp->p_sys_time 缺失 | D10 |
| P0-4 | billp vtimer 缺失 | D10 |
| P0-5 | p_time_stats vs p_time | 实现对齐 |
| P0-6 | vtimer_check 行号 68-89 → 81-103 | 文档修正 |
| P0-7 | §6 引用 tmp-16-timer.md | 结构整合 |
| P0-8 | 无 design.md | 本骨架 → design.md |
| P1-1 | clock.rs 注释 14→15 | 实现修正 |
| P1-2 | cause_alarm 行号 73 → 69-76 | 文档修正 |
| P1-3 | §7 缺 05 | 双向链路 |
| P1-4 | BTreeMap 同 exp_time 覆盖 | D2 |
| P1-5 | set/reset_timer 用 exp_time | D3 |
| P1-6 | KernelCallback 死代码 | D6 |
| P1-7 | PerCpuTick SMP 不匹配 | D8 |
| P1-8 | vtimer_check 冗余 | D11 |
| P1-9 | test 无 assert | 测试补充 |
| P1-10 | D1-D8 无方案对比 | 本骨架 → design.md |
