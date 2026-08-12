# 21-syscall-clock-outline-review.v1.md — Outline 评审

> **评审对象**: `design/21-outline.v1.md`
> **评审方法**: 4 维自审（教学性 / 本质深度 / 概念覆盖 / 组织合理性）
> **判定标准**: P0=0 → 自动批准；P0>0 → 修订后重新评审
> **创建**: 2026-08-01

---

## 一、评审结论

| 维度 | 判定 | 说明 |
|------|------|------|
| 教学性 | ✅ PASS | Ch1 concept-driven，4 节均有"灵魂本质"+ WHY→WHAT→HOW 弧线；三时钟源语义表清晰 |
| 本质深度 | ✅ PASS | Ch3 hypothesis-driven，7 个决策均有"如果 X 会有 Y 问题所以用 Z"；D5 ClockState 参数传递为核心 anti-translate 推理 |
| 概念覆盖 | ✅ PASS | A-F 六组知识点全覆盖；断裂修复表完整（11 处） |
| 组织合理性 | ✅ PASS | 章节顺序符合认知弧线；Ch2 锚定 C 源码；Ch4 真实代码（非占位）；与 15-clock-timer 衔接清晰 |

**P0 计数**: 0
**P1 计数**: 2（非阻断，执行中注意）
**判定**: ✅ **自动批准**，可进入 design 生成阶段

---

## 二、详细评审

### 2.1 教学性（Teaching Quality）

**检查项**:
- [x] Ch1 主语是时间/定时器，非函数名/结构体名
- [x] Ch1 每节有"灵魂本质"一句话
- [x] Ch1 采用 WHY→WHAT→HOW 弧线（§1.1 TIMES、§1.4 VTIMER 为典型）
- [x] 新概念首次出现有定义（三时钟源/SELF/adjtime/VT_VIRTUAL vs VT_PROF）
- [x] 概念之间有因果链（TIMES 查询→SETALARM 闹钟→STIME/SETTIME 设置→VTIMER 虚拟定时器）

**优秀点**:
- §1.1 三时钟源语义表（monotonic/realtime/boottime）直观区分易混淆概念，标注"可设置?"列衔接 §1.3
- §1.2 SETALARM 的 time_left 三分支 + 绝对/相对时间语义，从 do_setalarm.c:40-46,56-61 提炼清晰
- §1.4 VT_VIRTUAL vs VT_PROF 对比表（计数范围/到期信号/字段/标志）一目了然
- §1.3 SETTIME 双模式表（adjtime vs set time）+ adjtime 保留理由，避免"为何不删"疑问

**P1 改进建议**（非阻断）:
1. §1.1 并发安全可补充一句"AtomicU64 load(Relaxed) 对应 C 单字段读原子性"——但已在 Ch3 D6 详述，Ch1 可不重复
2. §1.4 vtimer_check 的并发注释（do_vtimer.c:83-88 "clock handler 只递减不设标志"）可补充"Rust 用 AtomicU64 CAS 编译期保证，替代 C 注释约定"——已在 Ch3 D6 覆盖

### 2.2 本质深度（Conceptual Depth）

**检查项**:
- [x] Ch3 采用 hypothesis-driven（"如果 X 设计会有 Y 问题所以用 Z"）
- [x] 每个决策有 ≥2 个被否决的选项 + 否决理由
- [x] 无迭代叙事（"旧版/最初/后来/我们改成"）
- [x] 决策之间有逻辑关系（D1 timer 表达→D2 回调→D5 状态访问；D3 enum→D6 AtomicU64）

**优秀点**:
- D5 ClockState 参数传递是核心 anti-translate 推理：从"全局变量→不可测试"推出"参数传递"，逻辑严密。这正是原 21-syscall-clock.md §4.2 "D5 实现说明 (2026-06-15 更新)"迭代叙事应改写的方向——从"已实现"叙事改为"为何如此设计"的 hypothesis
- D6 virt_left/prof_left 的 AtomicU64 推理覆盖 Cell（非 Sync）/u64+锁（高频开销）/RefCell（非 Sync）三个否决选项，最后选 AtomicU64 对应 SMP 安全
- D7 vtimer_check 的 standalone vs tick-internal 推理体现跨文档职责分离（21 用户态接口 vs 15 内核 tick 机制）
- D3 VtimerType enum 推理明确值对齐 com.h:420-421（Virtual=1/Prof=2），修正原 §4.1 的 0/1 错误

**P1 改进建议**（非阻断）:
1. D1 BTreeMap dual-index 引用 15 D2，可在 design 阶段补充"dual-index 在 syscall_clock.rs 的具体使用：s_alarm_timer: Option<(TimerEntry, TimerId)> 存储 id 用于 reset"——已在 Ch4 §4.2 体现
2. D4 adjtime 保留推理可补充"NTP 守护进程依赖 adjtime 渐变调整"——但这是应用场景，非本质设计，可不补

### 2.3 概念覆盖（Concept Coverage）

**检查项**:
- [x] 知识点覆盖矩阵完整（A-F 六组 × Ch1-Ch5）
- [x] C 源码符号全部列出（do_times.c 46 + do_setalarm.c 78 + do_stime.c 19 + do_settime.c 58 + do_vtimer.c 103）
- [x] 断裂修复表完整（11 处断裂 + 修复方案）
- [x] 跨文档衔接完整（与 15-clock-timer 的 7 个衔接点）

**覆盖验证**（对照 structure.md 知识点）:
- A.0-A.4 时间查询: ✅ §1.1 + §2.1 + D6 + §4.1 + test_dispatch_times_*
- B.0-B.6 同步闹钟: ✅ §1.2 + §2.2 + D1/D2 + §4.2 + test_ksc_setalarm_*
- C.0-C.5 时间设置: ✅ §1.3 + §2.3 + D4 + §4.3 + (§5.2 待补充)
- D.0-D.8 vtimer: ✅ §1.4 + §2.4 + D3/D6/D7 + §4.4 + test_vtimer_*/test_ksc_vtimer_*
- E.0-E.2 权限: ✅ §1.2/§1.4 + §2.2/§2.4 + D5 + §4.5 + test_ksc_*
- F.0-F.5 anti-translate: ✅ 贯穿 Ch3 D1-D7 + Ch4 全部

**无遗漏**: structure.md 列出的 8 处知识点遗漏全部在 outline 中有对应章节处理（实现或待补充测试）。

**关键修复验证**:
- virt_left/prof_left 占位（D.4）→ Ch4 §4.4 明确贴真实 `AtomicU64::load/store` 代码 ✅
- VtimerType 值错误（D.0）→ Ch3 D3 + Ch4 §4.4 修正为 1/2 ✅
- "D5 实现说明 (2026-06-15 更新)"迭代叙事 → Ch4 §4.2 删除，改事实陈述 ✅
- Ch3 平庸决策表 → Ch3 D1-D7 改为 hypothesis-driven ✅
- 测试不可 grep → Ch5 §5.1 列出 10 个实际 `fn test_*` 函数名 ✅

### 2.4 组织合理性（Organizational Soundness）

**检查项**:
- [x] 章节顺序符合认知弧线（概念→源码→决策→实现→测试）
- [x] Ch2 每个符号带 file:line
- [x] Ch4 贴真实代码，virt_left/prof_left 非 `/* */` 占位
- [x] Ch5 测试函数可 grep 验证（`fn test_*`）
- [x] 参见形成闭环（15/13/22/17/16）
- [x] 无迭代叙事（"D5 实现说明 (2026-XX-XX 更新)"等已删除）
- [x] 无 tmp 文件引用
- [x] 无内部 review ID（P0-XX/P1-XX）

**优秀点**:
- Ch2 §2.5 调用关系图用时序图展示 SETALARM 闘钟生命周期 + VTIMER 到期生命周期，清晰直观
- Ch4 §4.4 明确展示真实 AtomicU64 代码片段（`target.p_time.virt_left.load(Ordering::Relaxed)`），非占位
- Ch5 §5.1 现有 10 个测试 + §5.2 待补充 8 个测试，对应关系明确且可 grep
- Ch6 参见引用 15-clock-timer 明确标注"ClockState/TimerAction 在 15 定义，21 是用户态接口"

**无 P0 组织问题**。

---

## 三、执行注意事项（design 生成阶段关注）

1. **D5 ClockState 参数传递**: design 需明确所有 dispatch 函数的签名——`dispatch_setalarm(caller, msg, priv_table: &mut PrivTable, clock_state: &mut ClockState)` / `dispatch_vtimer(caller, msg, priv_table: &PrivTable, proc_table: &ProcessTable)`。重点展示"参数传入→可测试性"的因果链，参考 redox 的 `context::signal::requeue` + `time::monotonic` 设计（redox 用全局接口，minix-rs 显式传入更可测）。
2. **D6 AtomicU64 真实代码**: design 需贴 syscall_clock.rs:480,489,506,509,517,520 的真实 `virt_left.load(Relaxed)` / `store(Release)` 代码，**禁止 `/* virt_left */` 占位**。同时展示 proc.rs:588-589 的 `TimeStats.virt_left: AtomicU64` 定义。
3. **D7 vtimer_check 跨文档**: design 需标注 vtimer_check (do_vtimer.c:81-103) 在 15-clock-timer 实现（`tick_virt_timer`/`tick_prof_timer` + SIGVTALRM/SIGPROF），21 仅负责 set/query 接口。引用 15-design.md D11。
4. **VtimerType 值**: design 需明确 `Virtual = 1, Prof = 2`（对齐 com.h:420-421），与 syscall_clock.rs:48-53 一致。原 21-syscall-clock.md §4.1 的 0/1 是错误，需修正。
5. **无迭代叙事**: design 禁止"D5 实现说明 (2026-06-15 更新)"、"测试状态 (2026-XX-XX 更新)"、"previous Rust code used 0/1 which was a semantic drift (P0). Fixed 2026-06-15"等迭代叙事。改为事实陈述。
6. **redox 对比**: design 附录需对比 redox 时钟设计——redox 用 `context::signal::requeue` 重排队信号 + `time::monotonic` 全局接口；minix-rs 用 `TimerAction::NotifyAlarm` + 参数传入 ClockState（更可测）。

---

## 四、批准

✅ **批准**，进入 design 生成阶段（`design/21-design.md`）。

- P0: 0
- P1: 2（非阻断，design 阶段关注）
- 评审人: Trae (GLM-5.2)
- 评审日期: 2026-08-01
