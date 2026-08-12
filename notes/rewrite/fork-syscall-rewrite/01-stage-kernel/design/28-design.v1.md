# 28-usermapped-data: 设计文档（Design v1）

> **文档**: `28-usermapped-data.md`
> **状态**: v1 快照（2026-08-12，基于 C 源码 + 当前 Rust 实现状态）
> **用途**: Gate H 依据（design↔code 一致性）+ Rust 实现审阅依据

---

## Ch1: 设计决策

### D1: 64-bit 重写不保留 `.usermapped` 段
- **C**: kernel.lds 定义 `.usermapped_glo` + `.usermapped` 段；`arch_phys_map()` 返回物理地址；VM 映射到用户空间
- **Rust 64-bit**: 不定义此段；不实现 `arch_phys_map()` usermapped 分支；数据通过 `sys_getinfo` 获取
- **ARCH 标记**: Architectural Evolution — 从"直接内存映射"到"系统调用获取"
- **理由**:
  1. 64-bit 使用 `syscall` 指令直接入内核，不需要用户态 IPC trampoline（`.usermapped_glo` 段废弃）
  2. 数据结构布局不泄漏到用户态 ABI（安全性提升）
  3. 简化地址空间管理（VM 不需要为每个进程映射 usermapped 段）
- **影响**: 用户态从直接指针读取改为系统调用获取，性能略降但安全性提升

### D2: KernelInfo (boot→kernel) 保留并增强
- **C**: `kinfo` 结构既用于 boot→kernel 传递，又通过 usermapped 暴露给用户态
- **Rust 64-bit**: `KernelInfo`（`os/libs/minix-boot/src/kernel_info.rs:13`）仅用于 boot-shim → kernel 传递
- **理由**: boot 阶段尚无系统调用机制，必须用共享内存；boot→kernel 是受控环境，安全性可保证
- **字段差异**: Rust KernelInfo 精简为 12 字段（C kinfo 有 ~50 字段），去除 usermapped 相关字段

### D3: `kclockinfo` 改为 `ClockState` 内部字段
- **C**: `struct kclockinfo kclockinfo __section(".usermapped")` 全局变量，用户态直接读取 `kclockinfo.uptime`
- **Rust 64-bit**: `ClockState` 结构体字段（`os/kernel/src/clock.rs:673`），通过 `get_monotonic()` / `get_realtime()` / `get_boottime()` 函数访问
- **理由**:
  1. 封装性——内部字段可添加验证逻辑
  2. 避免全局可变状态——ClockState 实例化后受所有权约束
  3. 不泄漏布局——用户态不依赖 kclockinfo 字段偏移
- **访问路径**: 用户态通过 `sys_getinfo` GET_HZ / GET_TIME 等子请求间接获取

### D4: `minix_kerninfo` 顶层结构不保留
- **C**: `struct minix_kerninfo minix_kerninfo __section(".usermapped")` 含 magic + flags + 7 个结构指针
- **Rust 64-bit**: 无统一"内核信息页"概念；各信息独立获取
- **理由**: 64-bit 无 usermapped 段，不需要顶层指针结构作为用户态入口
- **替代**: 用户态通过 `sys_getinfo` 各子请求独立获取（GET_KINFO / GET_MACHINE / GET_LOADINFO 等）

### D5: IPC 入口向量表不保留
- **C**: 3 套向量表（softint/sysenter/syscall）+ 21 个 trampoline 函数（`arch/i386/usermapped_glo_ipc.S`）
- **Rust 64-bit**: 统一 `syscall` 指令入内核
- **ARCH 标记**: Architectural Evolution — 从"多入口 + 用户态 trampoline"到"单入口 + 内核直接处理"
- **理由**:
  1. 64-bit 指令集统一使用 `syscall`（x86-64）/ `ecall`（riscv64）/ `hvc`（aarch64）
  2. 不需要兼容 32-bit 的 softint/sysenter 多入口机制
  3. 内核直接处理系统调用，不需要用户态 trampoline 跳板
- **影响**: 用户态 IPC 调用从"函数指针 → trampoline → int/sysenter/syscall"简化为"syscall 指令直接入内核"

### D6: `kuserinfo` userland ABI 不保留
- **C**: `struct kuserinfo kuserinfo __section(".usermapped")` 含 `kui_size` + `kui_user_sp`，是 userland ABI
- **Rust 64-bit**: 用户栈顶通过 `KernelInfo.user_sp`（boot→kernel）+ `sys_getinfo` GET_KINFO（用户态查询）获取
- **理由**: minix-rs 是完整重写，无 legacy binary 兼容需求；不需要 `kui_size` ABI 兼容性检查机制
- **影响**: 用户态获取 user_sp 从"直接读 kuserinfo.kui_user_sp"改为"`sys_getinfo` GET_KINFO 返回"

---

## Ch2: Minix3 对齐矩阵

| Minix3 概念 | design 对应 | code 对应 | 一致性 |
|------------|------------|----------|--------|
| `.usermapped` section | D1 | （不实现） | ✅ WONTFIX — 64-bit 不保留 |
| `.usermapped_glo` section | D1/D5 | （不实现） | ✅ WONTFIX — 64-bit 不保留 |
| `minix_kerninfo` 顶层结构 | D4 | （不实现） | ✅ WONTFIX — 64-bit 不保留 |
| `kinfo` 结构 | D2 | `KernelInfo`（minix-boot） | ✅ 部分保留（boot→kernel 用途） |
| `machine` 结构 | D4 | （不实现） | ✅ WONTFIX — 通过 sys_getinfo GET_MACHINE |
| `kmessages` 结构 | D4 | （不实现） | ✅ WONTFIX — 64-bit 无诊断消息缓冲区 |
| `loadinfo` 结构 | D4 | `LoadInfoStruct`（misc.rs） | ✅ 通过 sys_getinfo GET_LOADINFO |
| `kuserinfo` 结构 | D6 | `KernelInfo.user_sp` | ✅ WONTFIX usermapped；保留 boot→kernel |
| `arm_frclock` 结构 | D4 | （不实现） | ✅ WONTFIX — ARM 32-bit 专用 |
| `kclockinfo` 结构 | D3 | `ClockState`（clock.rs:673） | ✅ 内部化 |
| `minix_ipcvecs_softint` | D5 | （不实现） | ✅ WONTFIX — 64-bit 不用 softint |
| `minix_ipcvecs_sysenter` | D5 | （不实现） | ✅ WONTFIX — 64-bit 不用 sysenter |
| `minix_ipcvecs_syscall` | D5 | （不实现） | ✅ WONTFIX — 64-bit 直接 syscall，不需 trampoline |
| `arch_phys_map()` usermapped | D1 | （不实现） | ✅ WONTFIX — 无 usermapped 段 |
| `get_minix_kerninfo()` | D4 | （不实现） | ✅ WONTFIX — 无 usermapped 段 |
| `minix_get_user_sp()` | D6 | `sys_getinfo` GET_KINFO | ✅ 替代方案 |

---

## Ch3: Rust 类型清单

| 类型 | 定义位置 | 用途 | 状态 |
|------|---------|------|------|
| `KernelInfo` | `os/libs/minix-boot/src/kernel_info.rs:13` | boot→kernel 信息传递 | ✅ 已实现（12 字段） |
| `ClockState` | `os/kernel/src/clock.rs:673` | kclockinfo 内部化 | ✅ 已实现 |
| `LoadInfoStruct` | `os/kernel/src/misc.rs` | GET_LOADINFO 子请求 | ✅ 已实现 |
| `MemoryRegion` | `os/libs/minix-boot/src/kernel_info.rs:117` | KernelInfo.memmap 元素 | ✅ 已实现 |
| `BootModule` | `os/libs/minix-boot/src/kernel_info.rs:123` | KernelInfo.boot_modules 元素 | ✅ 已实现 |
| `PlatformDescSource` | `os/libs/minix-boot/src/platform.rs` | KernelInfo.platform_sources 元素 | ✅ 已实现 |

**不实现的类型**（WONTFIX）:
- `MinixKernInfo`（顶层结构）
- `KinfoStruct`（完整 kinfo — KernelInfo 已精简）
- `MachineStruct`（通过 sys_getinfo 获取）
- `KmessagesStruct`（诊断消息缓冲区）
- `KuserinfoStruct`（userland ABI）
- `ArmFrclockStruct`（ARM 32-bit 专用）
- `MinixIpcvecs`（IPC 入口向量表）

---

## Ch4: 设计一致性检查

### 4.1 design↔code 一致性
- D1 (不保留 usermapped): ✅ Rust 链接脚本无 `.usermapped` 段
- D2 (KernelInfo 保留): ✅ `os/libs/minix-boot/src/kernel_info.rs` 存在
- D3 (ClockState 内部化): ✅ `os/kernel/src/clock.rs:673` 存在
- D4 (minix_kerninfo 不保留): ✅ Rust 无 `MinixKernInfo` 类型
- D5 (IPC vecs 不保留): ✅ Rust 无 `MinixIpcvecs` 类型
- D6 (kuserinfo 不保留): ✅ Rust 无 `KuserinfoStruct` 类型

### 4.2 design↔Minix3 对齐
- 所有 WONTFIX 项都有明确理由（64-bit 架构演进）
- 替代方案（sys_getinfo）已在 25-misc-unported.md 文档化
- 不引入 C 兼容/FFI 层（符合项目约束）

### 4.3 跨文档一致性
- 02-higher-half-kernel.md:116 "已废弃"声明 → 本文 D1 详述原因
- 07-cross-space-init.md `arch_phys_map()` → 本文 §2.4 引用
- 25-misc-unported.md GET_KINFO → 本文 D4 替代方案

---

## Ch5: 开放问题

1. **kmessages 替代方案**: 64-bit 是否需要内核诊断消息缓冲区？当前无实现，可能需要 future work
2. **machine.processors_count 获取**: 当前通过 `sys_getinfo` GET_CPUINFO（25-misc-unported.md DEFERRED），SMP 文档（16-smp.md）应覆盖运行时获取
3. **arm_frclock**: ARM 64-bit (aarch64) 是否有等价机制？当前 WONTFIX，未来 aarch64 实现可能需要重新评估
