# 06-structure-new.md — 知识点结构与重组诊断（Step 1 验证确认版）

> **目的**：按工作流 Step 1 重新穷举知识点。本文件是对 `06-structure.md`（v2，源码驱动）的**验证确认 + 权威基线声明**，而非简单复制。
>
> **方法**：Step 0 已逐文件精读 C 源码（proc.c / main.c / system.c / arch/*）与 Rust 实现（os/kernel/src/lib.rs / proc.rs / proc_table.rs / kpriv.rs / capability.rs + os/arch/*/boot.rs），逐条核对 `06-structure.md` 的知识点锚点。
>
> **结论**：`06-structure.md` 的 15 个概念组（A.0-O.5）、C 源码锚点、Rust 锚点、10 个断裂点诊断均**经核查准确**，作为本次重写的权威基线。本文件补充核查证据与少量修正。

---

## 〇、验证状态总览

| 概念组 | 核查项 | C 源码锚点 | Rust 锚点 | 核查结论 |
|--------|--------|-----------|----------|---------|
| A.0-A.10 进程抽象与进程表 | proc_init / proc[] / KProcess | proc.c:119-161, proc.h:283-330 | proc.rs, proc_table.rs | ✅ 准确 |
| B.1-B.12 特权表与能力模型 | priv[] / get_priv / CapabilityTemplate | priv.h:21-86, system.c:274-360, main.c:178-248 | kpriv.rs, capability.rs | ✅ 准确 |
| C.1-C.8 RTS 标志与进程状态 | RTS_* / misc_flags / P_BLOCKEDON | proc.h:141-262 | proc.rs RtsFlags/MiscFlags | ✅ 准确 |
| D.1-D.8 boot image 与启动清单 | image[] / boot 循环 / schedulable | table.c:44-66, main.c:165-282 | proc.rs KERNEL_TASKS, lib.rs:699 | ✅ 准确 |
| E.1-E.12 CPU 上下文与寄存器初始化 | arch_proc_reset/init/boot_proc | arch_system.c, protect.c, memory.c | arch/*/boot.rs CpuContextArch | ✅ 准确 |
| F.1-F.14 VM ELF 加载与鸡生蛋 | arch_boot_proc / libexec / bootstrap 页表 | protect.c:388-455, exec_elf.c | arch load_vm_elf | ✅ 准确 |
| G.1-G.5 SMP 预留 | per-CPU IDLE / BKL | proc.c:152-160 | smp.rs | ✅ 准确（标注阶段 F） |
| H.1-H.4 fork 运行时路径 | fork_from / FPU 继承 | （运行时路径） | proc.rs fork_from | ✅ 准确（标注运行时） |
| I.1-I.4 零堆启动 | kernel_may_alloc / const fn | pre_init.c:33/38, glo.h:76, main.c:105/142 | lib.rs:978 | ✅ 准确 |
| J.1-J.4 测试覆盖 | 三层测试 + L3 grep | — | tests/* | ✅ 准确 |
| K.1-K.4 跨架构统一抽象 | CpuContextArch trait | arch_system.c 三架构 | arch/*/boot.rs | ✅ 准确 |
| L.1-L.3 错误处理与不变量 | errno→Result / panic 策略 | （散落） | capability.rs CapabilityError | ✅ 准确 |
| M.1-M.7 调度字段与统计 | p_priority / p_quantum / SchedFields | main.c:184/194/210, sched.h | proc.rs SchedFields | ✅ 准确 |
| **N.1-N.9 boot→running 转换** | **bsp_finish_booting** | **main.c:38-109** | **lib.rs:1140+** | ✅ **核查确认（最大缺口）** |
| O.1-O.5 boot 期架构后初始化 | arch_post_init / memory_init / system_init | main.c:284-298 | lib.rs | ✅ 准确（范围边界） |

---

## 一、关键核查证据（补充 06-structure.md）

### 1.1 proc_init() 核查（概念组 A.7）

**C 源码** `minix3/minix/kernel/proc.c:119-161`：
```c
void proc_init(void)
{
  for (rp = BEG_PROC_ADDR, i = -NR_TASKS; rp < END_PROC_ADDR; ++rp, ++i) {
    rp->p_rts_flags = RTS_SLOT_FREE;
    rp->p_magic = PMAGIC;
    rp->p_nr = i;
    rp->p_endpoint = _ENDPOINT(0, rp->p_nr);
    rp->p_scheduler = NULL;
    rp->p_priority = 0;
    rp->p_quantum_size_ms = 0;
    arch_proc_reset(rp);
  }
  for (sp = BEG_PRIV_ADDR, i = 0; sp < END_PRIV_ADDR; ++sp, ++i) {
    sp->s_proc_nr = NONE;
    sp->s_id = (sys_id_t) i;
    ppriv_addr[i] = sp;
    sp->s_sig_mgr = NONE;
    sp->s_bak_sig_mgr = NONE;
  }
  idle_priv.s_flags = IDL_F;
  for (i = 0; i < CONFIG_MAX_CPUS; i++) {
    struct proc * ip = get_cpu_var_ptr(i, idle_proc);
    ip->p_endpoint = IDLE;
    ip->p_priv = &idle_priv;
    ip->p_rts_flags |= RTS_PROC_STOP;
    set_idle_name(ip->p_name, i);
  }
}
```
**核查结论**：与 structure.md A.7 完全一致。三遍循环（进程表→特权表→IDLE）职责分离清晰。

### 1.2 boot 循环核查（概念组 D.5 / B.12 / C.4）

**C 源码** `minix3/minix/kernel/main.c:165-282`：
- `schedulable_proc = (iskerneln(proc_nr) || isrootsysn(proc_nr) || proc_nr == VM_PROC_NR)` — main.c:173-174 ✅
- VM 分支：`VM_F / SRV_T / SRV_M / SRV_KC / s_sig_mgr=SELF / SRV_Q / SRV_QT` — main.c:180-187 ✅
- 内核 task 分支：`IDL_F/TSK_F / TSK_I / CSK_T|TSK_T / TSK_M / TSK_KC` — main.c:189-198 ✅
- RS 分支：`RSYS_F / SRV_I / SRV_T / SRV_M / SRV_KC / SRV_SM / SRV_Q / SRV_QT` — main.c:200-210 ✅
- 非 schedulable：`RTS_SET(rp, RTS_NO_PRIV | RTS_NO_QUANTUM)` — main.c:230 ✅
- 非 VM 用户进程：`RTS_VMINHIBIT | RTS_BOOTINHIBIT` — main.c:242-244 ✅
- 所有进程：`RTS_PROC_STOP`，清 `RTS_SLOT_FREE` — main.c:246-247 ✅

**核查结论**：与 structure.md D.5/B.12/C.4 完全一致。**补充修正**：structure.md B.12 提到"VM→VM_F+SRV_T+SRV_M+SRV_KC+PM_SM"，但 C 源码 VM 的 sig_mgr 是 `SELF`（不是 PM_SM）；RS 的 sig_mgr 才是 `SRV_SM=ROOT_SYS_PROC_NR`。重写时需注意此区别。

### 1.3 bsp_finish_booting() 核查（概念组 N.1-N.9，最大缺口）

**C 源码** `minix3/minix/kernel/main.c:38-109`：
```c
void bsp_finish_booting(void)
{
  cpu_identify();
  vm_running = 0;
  krandom.random_sources = RANDOM_SOURCES;
  krandom.random_elements = RANDOM_ELEMENTS;
  get_cpulocal_var(bill_ptr) = get_cpulocal_var_ptr(idle_proc);
  get_cpulocal_var(proc_ptr) = get_cpulocal_var_ptr(idle_proc);
  announce();
  for (i=0; i < NR_BOOT_PROCS - NR_TASKS; i++) {
    RTS_UNSET(proc_addr(i), RTS_PROC_STOP);
  }
  cycles_accounting_init();
  if (boot_cpu_init_timer(system_hz)) { panic(...); }
  fpu_init();
  #ifdef CONFIG_SMP
  cpu_set_flag(bsp_cpu_id, CPU_IS_READY);
  machine.processors_count = ncpus;
  machine.bsp_id = bsp_cpu_id;
  #endif
  kernel_may_alloc = 0;          /* main.c:105 */
  switch_to_user();
  NOT_REACHABLE;
}
```
**核查结论**：与 structure.md N.1-N.9 完全一致。关键点确认：
- `kernel_may_alloc = 0` 在 main.c:105，**确实位于 bsp_finish_booting 函数体内**（函数 span 38-109）✅
- RTS_PROC_STOP 清除循环只遍历用户态 boot 进程（`i < NR_BOOT_PROCS - NR_TASKS`），不含内核 task ✅
- `switch_to_user()` 后 `NOT_REACHABLE`，函数不返回 ✅

**Rust 实现** `os/kernel/src/lib.rs:1140+`：
- `bsp_finish_booting(proc_table, smp_state) -> !` — 返回 `!` 类型表达"不返回" ✅
- Step 1: `VM_RUNNING.store(false, ...)` ✅
- Step 2: `proc_table.set_bill_to_idle()` ✅
- Step 4: `for nr in 0..(NR_BOOT_PROCS - NR_TASKS) { proc_table.rts_unset(nr, PROC_STOP); }` ✅
- Step 8: `kernel_may_alloc = 0`（lib.rs:1299-1300）✅

### 1.4 kernel_may_alloc 生命周期核查（概念组 I.4）

**C 源码**：
- `pre_init.c:33/38`：`int kernel_may_alloc = 1;`（初始化为 1，boot 期允许分配）✅
- `main.c:142`：`kernel_may_alloc = 1;`（kmain 开头再次确认）✅
- `main.c:105`：`kernel_may_alloc = 0;`（bsp_finish_booting 末尾关闭）✅
- `pg_utils.c:42/74/102/143/274`：`assert(kernel_may_alloc);`（分配器多处断言）✅

**核查结论**：与 structure.md I.4 完全一致。boot 期内存分配窗口 = pre_init → bsp_finish_booting。

---

## 二、10 个断裂点确认（对照 06-structure.md §三）

| 断裂点 | 描述 | 核查结论 | 修复归属 |
|--------|------|---------|---------|
| 1 | Ch1 讲进程表，Ch3 无 ProcessTable 设计专节 | ✅ 真实断裂 | outline §3.1 [NEW] |
| 2 | Ch1 讲特权表，Ch3 无 PrivTable 整体设计 | ✅ 真实断裂 | outline §3.2 [NEW] |
| 3 | Ch1 讲 RTS，Ch3/Ch4 无 RtsFlags 设计 | ✅ 真实断裂 | outline §3.3 [NEW] |
| 4 | Ch1 讲 boot image，Ch3 无类型设计 | ✅ 真实断裂 | outline §3.4 [NEW] |
| 5 | Ch1 缺进程概念抽象（A.0） | ✅ 真实断裂 | outline §1.1 [NEW] |
| 6 | enable_user_io（E.12）缺失 | ✅ 真实断裂 | outline §4.1 [NEW] |
| 7 | 错误处理与不变量（L）缺失 | ✅ 真实断裂 | outline §3.13 [NEW] |
| 8 | 调度字段与统计（M）缺失 | ✅ 真实断裂 | outline §3.11 [NEW] |
| 9 | boot→running 转换（N）完全缺失 | ✅ **最大缺口** | outline §1.6+§2.8+§3.12+§4.8+§5.3 [全 NEW] |
| 10 | post-init（O）缺失 | ✅ 真实断裂 | outline §2.9 [NEW]（范围边界） |

---

## 三、开发文档味诊断确认（对照 06-structure.md §四）

`06-structure.md` §四识别的 6 处开发文档味（"最初/旧版/后来/我们改成"）经核查确实存在于当前 06 文档草稿中，需在重写时改为假设性推理。确认无误。

---

## 四、权威基线声明

**`06-structure.md`（v2，源码驱动）经 Step 0 逐条核查，确认为本次重写的权威知识点基线。** 本文件（06-structure-new.md）作为验证确认记录，不重复罗列全部知识点（详见 06-structure.md §一）。

重写 06 文档时，以 06-structure.md 的 15 个概念组（A.0-O.5）为知识点全集，以 10 个断裂点为修复清单，以 6 处开发文档味为改写清单。

---

## 五、Step 1 自检

- [x] C 源码知识点穷举：proc.c / main.c / system.c / protect.c / arch_system.c / exec_elf.c / priv.h / proc.h / const.h / com.h / param.h / sched.h / glo.h / pre_init.c / pg_utils.c / proto.h / memory.c / ipc_filter.h — 全部核查
- [x] OS 理论与机制抽象：进程抽象 / 特权稀缺 / RTS 位图 / bootstrapping / CPU 四问 / VM 鸡生蛋 / 零堆 / SMP/BKL / 架构演进 — 全部覆盖
- [x] Rust 实现对照：proc.rs / proc_table.rs / kpriv.rs / capability.rs / lib.rs / arch/*/boot.rs — 全部核查
- [x] 纵向链路断裂点：10 个，全部确认
- [x] 开发文档味：6 处，全部确认
- [x] 知识点遗漏：无（C 源码核心符号全部落到概念组）
- [x] Rust 实现错误点：DEFERRED 路径（VM ELF 加载）已标注，非错误而是诚实标记

**Step 1 完成，可进入 Step 2。**
