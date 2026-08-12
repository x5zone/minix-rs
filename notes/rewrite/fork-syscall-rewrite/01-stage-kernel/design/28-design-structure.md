# 28-usermapped-data: 设计结构（Design Structure）

> **文档**: `28-usermapped-data.md`
> **状态**: v1 快照（2026-08-12，基于 C 源码 + 当前 Rust 实现状态）
> **用途**: 知识点全集 + Gate H 一致性依据

---

## 知识点全集

### 1. `.usermapped` section 机制
- **链接器层面**: kernel.lds 定义 `.usermapped_glo` (可执行) + `.usermapped` (数据) 两个段
- **物理布局**: 段位于内核镜像起始（unpaged 段之后，.text 之前），4KB 对齐
- **映射机制**: `arch_phys_map()` 返回段物理地址，VM 将其映射到每个进程的用户地址空间
- **访问控制**: `VMMF_USER` 标志让用户态可读；`VMMF_GLO` 标志表示全局可执行（IPC trampolines）

### 2. 8 个用户可见内核数据结构
| 结构 | C 类型 | 用途 | userland ABI |
|------|--------|------|-------------|
| `minix_kerninfo` | `struct minix_kerninfo` | 顶层信息结构，含其他结构指针 | 部分（kuserinfo/ipcvecs） |
| `kinfo` | `struct kinfo` | 内核信息（memmap/vir_base/proc_count 等） | ❌ NOT userland ABI（legacy user_sp 例外） |
| `machine` | `struct machine` | 机器信息（CPU 数/BSP ID/APIC/RSDP/board_id） | ❌ NOT userland ABI |
| `kmessages` | `struct kmessages` | 内核诊断消息环形缓冲区 | ❌ NOT userland ABI |
| `loadinfo` | `struct loadinfo` | 系统负载平均值 | ❌ NOT userland ABI |
| `kuserinfo` | `struct kuserinfo` | 用户态 ABI（kui_size + kui_user_sp） | ✅ userland ABI |
| `arm_frclock` | `struct arm_frclock` | ARM 自由运行时钟（hz + tcrr 地址） | ❌ NOT userland ABI |
| `kclockinfo` | `struct kclockinfo` | 时钟信息（boottime/uptime/realtime/hz） | ❌ NOT userland ABI（volatile） |

### 3. 3 个 IPC 入口向量表
| 向量表 | 入口机制 | C 位置 | 用途 |
|--------|---------|--------|------|
| `minix_ipcvecs_softint` | `int $VEC` 软中断 | usermapped_data_arch.c:4 | 兼容所有 x86（慢路径） |
| `minix_ipcvecs_sysenter` | `sysenter` 指令 | usermapped_data_arch.c:14 | Intel 快速系统调用 |
| `minix_ipcvecs_syscall` | `syscall` 指令 | usermapped_data_arch.c:24 | AMD 快速系统调用 |

每表 7 个函数指针: send / receive / sendrec / sendnb / notify / do_kernel_call / senda

### 4. IPC trampoline 汇编
- **文件**: `arch/i386/usermapped_glo_ipc.S`
- **段**: `.usermapped_glo`（映射到用户空间可执行）
- **三套入口**: softint / sysenter / syscall
- **7 个 IPC 函数**: send / receive / sendrec / sendnb / notify / do_kernel_call / senda
- **栈布局**: 依赖 proc_stacktrace() 需找到 %ebp

### 5. 用户态访问 API
- `get_minix_kerninfo()`: 返回 `struct minix_kerninfo *` 指针
- `minix_get_user_sp()`: 从 kuserinfo 获取用户栈顶（fallback 到 kinfo.user_sp）
- `KUSERINFO_HAS_FIELD(kui, f)`: ABI 兼容性检查宏
- `kerninfo_magic`: 0xfc3b84bf 魔数验证
- `ki_flags`: `MINIX_KIF_IPCVECS` / `MINIX_KIF_USERINFO` 标志位

### 6. 64-bit 重写决策
- **02-higher-half-kernel.md:116 声明**: `.usermapped` 段在 64-bit 重写中"已废弃"
- **实际状态**: 部分废弃
  - IPC trampolines (softint/sysenter/syscall): 64-bit 使用 `syscall` 指令直接入内核，不需要用户态 trampoline
  - 数据结构 (kinfo/machine 等): 64-bit 通过 `sys_getinfo` 系统调用获取，不再直接映射
  - KernelInfo (boot-loader→kernel): 保留并增强（见 `os/libs/minix-boot/src/kernel_info.rs`）
- **ARCH 标记**: 这是 Architectural Evolution — 从"直接内存映射"到"系统调用获取"

### 7. redox 对照
- **redox**: 无 usermapped 段——scheme 模型，用户态通过 scheme 请求获取内核信息
- **Minix3**: usermapped 段直接映射——性能优化，避免系统调用开销
- **minix-rs**: 64-bit 采用 sys_getinfo 模型（类似 redox 的 scheme 请求，但保留 Minix3 集中式调用）

### 8. 与前序文档的关系
- **02-higher-half-kernel.md**: 链接脚本中 `.usermapped` 段定义（L24-28）
- **06-proc-init-boot-proc.md**: `kinfo` 结构在 boot 阶段初始化
- **07-cross-space-init.md**: `arch_phys_map()` 机制（usermapped 段映射入口）
- **09-vm-boot-protocol.md**: VM 启动时建立 usermapped 映射
- **13-syscall-dispatch.md**: `sys_getinfo` 系统调用（64-bit 替代方案）
- **15-clock-timer.md**: `kclockinfo` 结构（已在 ClockState 中实现）
- **25-misc-unported.md**: `GET_KINFO` / `GET_MACHINE` 等子请求

---

## 设计决策预览

| ID | 决策 | 理由 |
|----|------|------|
| D1 | 64-bit 重写不保留 `.usermapped` 段 | 64-bit 使用 syscall 指令直接入内核，不需要用户态 IPC trampoline |
| D2 | 数据结构通过 `sys_getinfo` 获取 | 简化地址空间管理；避免内核数据布局泄漏到用户态 |
| D3 | KernelInfo (boot→kernel) 保留并增强 | boot-shim → kernel 信息传递仍需要共享内存 |
| D4 | `kclockinfo` 在 Rust 中改为 `ClockState` 内部字段 | 不再全局可见；通过 `get_monotonic()` / `get_realtime()` 函数访问 |
| D5 | `minix_kerninfo` 顶层结构不保留 | 64-bit 无统一"内核信息页"概念；各信息独立获取 |
| D6 | IPC 入口向量表不保留 | 64-bit 使用统一 `syscall` 指令；不需要 softint/sysenter/syscall 三套 |

---

## 待回答问题

1. `kuserinfo.kui_user_sp` 在 64-bit 中如何获取？（当前 KernelInfo.user_sp 已存在）
2. `machine.processors_count` / `bsp_id` 在 64-bit 中如何获取？（SMP 文档 16-smp.md 应覆盖）
3. `kmessages` 诊断消息缓冲区在 64-bit 中是否有替代？（当前无）
4. `loadinfo` 负载信息在 64-bit 中通过 `GET_LOADINFO` 子请求获取（见 25-misc-unported.md）
