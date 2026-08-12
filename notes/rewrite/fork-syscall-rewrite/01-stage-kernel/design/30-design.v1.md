# 30-kernel-profile: 设计文档（Design v1）

> **文档**: `30-kernel-profile.md`
> **状态**: v1 快照（2026-08-12）
> **用途**: Gate H 依据 + DEFERRED 文档化

---

## Ch1: 设计决策

### D1: profile 时钟接口保留（trait 抽象）
- **C**: `init_profile_clock(freq)` + `stop_profile_clock()` + `arch_init_profile_clock` / `arch_stop_profile_clock`
- **Rust 64-bit**: `ClockArch::init_profile_clock` trait 方法 + `X86_64ClockArch` impl
- **理由**: 接口轻量，trait 抽象符合 HW 抽象原则；保留接口为未来实现预留

### D2: 样本收集不实现（DEFERRED）
- **C**: `sprof_save_sample` / `sprof_save_proc` / `profile_sample` / `profile_clock_handler`
- **Rust 64-bit**: 不实现
- **理由**: SPROFILE 当前未启用；样本收集依赖 `sprof_sample_buffer` + `sprof_info` 全局状态；优先级低于核心子系统

### D3: NMI profiling 不实现（WONTFIX）
- **C**: `nmi_sprofile_handler` — NMI 触发的 profiling
- **Rust 64-bit**: 不实现（见 26-watchdog.md）
- **理由**: 64-bit 无 NMI 子系统

### D4: `sprof_sample_buffer` 不保留（DEFERRED）
- **C**: `char sprof_sample_buffer[SAMPLE_BUFFER_SIZE]` 全局数组
- **Rust 64-bit**: 不保留
- **理由**: 随 D2 样本收集一起 DEFERRED；未来实现时用 `Vec<SprofSample>` 或 `Box<[SprofSample]>` 替代全局数组

---

## Ch2: Minix3 对齐矩阵

| Minix3 概念 | design 对应 | code 对应 | 一致性 |
|------------|------------|----------|--------|
| `init_profile_clock` | D1 | `ClockArch::init_profile_clock` | ✅ 已实现（trait） |
| `stop_profile_clock` | D1 | `X86_64ClockArch::stop_profile_clock` | ✅ 已实现 |
| `sprof_save_sample` | D2 | （不实现） | ✅ DEFERRED |
| `sprof_save_proc` | D2 | （不实现） | ✅ DEFERRED |
| `profile_sample` | D2 | （不实现） | ✅ DEFERRED |
| `profile_clock_handler` | D2 | （不实现） | ✅ DEFERRED |
| `nmi_sprofile_handler` | D3 | （不实现） | ✅ WONTFIX（见 26） |
| `sprof_sample_buffer` | D4 | （不保留） | ✅ DEFERRED |
| `sprof_info` 全局状态 | D2 | （不保留） | ✅ DEFERRED |

---

## Ch3: 设计一致性检查

- D1: trait 接口已实现 ✅
- D2: 样本收集 DEFERRED，与 25-misc-unported.md `do_sprofile` DEFERRED 一致 ✅
- D3: NMI WONTFIX，与 26-watchdog.md 一致 ✅
- D4: buffer 随 D2 一起 DEFERRED ✅
