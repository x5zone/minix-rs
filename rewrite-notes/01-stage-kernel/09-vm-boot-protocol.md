# 09 · VM 启动协商：一页交接与一条 SYS_VMCTL 通道

> **C 真值源**：`minix3/minix/include/minix/com.h`（SYS_VMCTL 调用号与 VMCTL_* 子命令常量、SVMCTL_* 消息字段）、`minix3/minix/kernel/system/do_vmctl.c:do_vmctl`、`minix3/minix/kernel/arch/i386/arch_do_vmctl.c`（`setcr3`/`arch_do_vmctl`）、`minix3/minix/servers/vm/main.c`（`exec_bootproc`/`init_vm`/`is_first_time`）、`minix3/minix/servers/vm/pagetable.c`（`pt_bind`/`pt_mapkernel`/`pt_allocate_kernel_mapped_pagetables`）、`minix3/minix/servers/vm/glo.h`（BSS 全局 `kernel_boot_info`）、`minix3/minix/lib/libsys/sys_vmctl.c:sys_vmctl_set_addrspace`、`minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc`、`minix3/minix/kernel/arch/i386/pre_init.c`（模块表装配）、`minix3/minix/kernel/arch/i386/memory.c`（`createpde`/`mem_clear_mapcache`/`arch_phys_map`）。
> **Rust 实现**：`os/libs/minix-types/src/types/boot.rs`（`VmBootHandoff` 与交接页常量）、`os/kernel/src/syscall.rs`（`dispatch_vmctl` 与 `vmctl_*` 子函数）、`os/kernel/src/vm.rs:enum VmCtlParam`、`os/kernel/src/vm_handoff.rs`（`classify`/`build_vm_handoff`/`build_identity_windows`）、`os/servers/vm/src/boot.rs`（`read_boot_params`/`reconcile`/`BootParams`）、`os/servers/vm/src/pagetable/vm_self_map.rs:fn init_vm_self_pt`、`os/servers/vm/src/vm_server.rs`（`init`/`init_global_state`/`exec_bootproc`/`init_boot_procs`）、`os/servers/vm/src/main.rs`（启动链）。
> **目标读者**：熟悉操作系统基本概念（进程、页表、IPC、内核/用户态边界）与 C 语言的开发者；Rust 一侧读到 repr(C) 结构体、enum 与 `unsafe` 契约大意即可。
> **前置知识**：08 篇的启动收尾、07 篇的 DM 双窗口与 bootstrap 根、06 篇的进程表与 RTS 位族（VMINHIBIT/BOOTINHIBIT 两道闸的置位）。
> **教学目标**：①协商问题与 VM 的出生家当（§1.1）；②五步协商与两条放行路径（§1.2）；③交接页的内容与版本门（§1.3、§3.1）；④setcr3/SETADDRSPACE 支点与其余子命令族（§2）；⑤Rust 的一页契约与分派（§3、§4）；⑥测试与诊断（§5、§6）；C 侧覆盖边界见 §1.5。
> **三档术语**：本篇的 Rust 实现相对 Minix3 C 属 Rewrite（保持外部行为）与 Refactor（不改语义的重构）；`[ARCH: riscv-vmddm]`（PteRead/PteWrite/PteZero 三臂）是 Architectural Evolution（riscv64 专属，文档/注释/代码三处一致标注）。
> **编号说明**：正文与引文里出现的 `NK4-C`、`fix2x`、`gh*`、`T8`、`D-40`、`X-7` 等编号是本仓的开发工作条目号与评审记录编号——原文保留，溯源按编号在本仓检索。
> **路径约定**：本篇所有 `minix3/...` 与 `os/...` 路径都相对仓库根目录。引用位置由符号名定位（`path:func`、`path:fn NAME`、`path:static NAME`），不使用行号。
> **本篇主题**：VM 出生时只拿着内核建的 bootstrap 根，却要接管全系统的内存管理——中间的协商怎么完成。两件交接物（一页世界状态快照 + 一条 SYS_VMCTL 通道）与两道闸的两条放行路径（SETADDRSPACE 清 VMINHIBIT 不入队、BOOTINHIBIT_CLEAR 才入队）。
> **前置**：[08-system-init-boot-finish.md](08-system-init-boot-finish.md)（三开关与 bsp 收尾；其 §3.3 预告的 set_vm_running 补写兑现于本篇 SETADDRSPACE 臂，§3.5 预告的凭证随页交付兑现于本篇 reconcile，§5.1 移交的 identity_windows 与 handoff 协议面归本篇）、[07-cross-space-init.md](07-cross-space-init.md)（DM 双窗口与 bootstrap 根的来历）、[06-proc-init-boot-proc.md](06-proc-init-boot-proc.md)（VMINHIBIT/BOOTINHIBIT 的置位——本篇讲它们的解除者）。
> **后续**：调度循环本体（`10-switch-to-user`，切用户与调度循环篇，尚未重生成）、分派入口与 kernel_call_finish（`13-syscall-dispatch`，系统调用分派篇，尚未重生成）、VMSUSPEND 挂起-恢复运行时（`24-cross-space-runtime`，跨空间运行时篇，尚未重生成）、VM 服务器内部本体（`rewrite-notes/02-stage-vm/`，init_page_table/map_kernel/PMM）。

[08-system-init-boot-finish.md](08-system-init-boot-finish.md) 的结尾，`bsp_finish_booting` 拨完三个开关，一句 `switch_to_user()` 把 CPU 交了出去。此刻 boot image 里能跑的用户进程只有 VM——其余每个 boot 进程都被 VMINHIBIT 和 BOOTINHIBIT 两个位按住（[06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §1.4 的置位），等着有人给它们建页表、装镜像、放行。干这件事的人就是 VM 自己，而 VM 拿到的全部家当是一张内核替它建好的 bootstrap 页表根：它不知道自己能用的内存在哪、内核映射长什么样、其他进程的表该建给谁。**本协议是 switch_to_user 之后的第一个用户态动作。**CPU 此刻怎么落到 VM 的入口，由调度循环的第一次切用户完成（`10-switch-to-user`，切用户与调度循环篇，尚未重生成）；VM 发出的内核调用怎么穿过陷入与分派入口抵达本篇的 handler，由分派入口与 kernel_call_finish 管（`13-syscall-dispatch`，系统调用分派篇，尚未重生成）。本篇从 handler 收到消息、从 VM 读到第一个字节开始：内核把"世界状态"交出去（一页），VM 把"我为别人做的事"申报回来（一条通道），双方按两道闸放行进程。

---

## 1. 概述

### 1.1 协商问题：被别人建好地址空间的进程要接管内存管理

先把困难本身摆清楚。VM 出生时是全系统唯一可调度的用户进程，但它的地址空间是内核造的 bootstrap 根：这张表里只有内核自己的映射和一段恒等映射，没有任何一条"用户内存归谁"的记录。VM 要接管的职责却需要三类它此刻没有的信息——自己能用的内存在哪（不然分配器无从建起）、内核的映射长什么样（不然给每个进程复刻内核映射时只能猜）、其他进程的镜像在哪（不然无从装载）。协商就是补齐这三类信息的过程，它由三件交换物构成：

1. **一页世界状态快照**。内核在调度 VM 之前，把 VM 需要的全部启动参数写进一页内存，映射进 VM 的地址空间让 VM 自己读。C 的对应物是一组 BSS 全局：`kernel_boot_info`（声明在 `minix3/minix/servers/vm/glo.h`），`minix3/minix/servers/vm/main.c:init_vm` 里一次 `sys_getkinfo` 把内核的 kinfo 整块拷进来。
2. **一条控制通道**。SYS_VMCTL 内核调用（`minix3/minix/include/minix/com.h` 里 `SYS_VMCTL = KERNEL_CALL + 43`），带 15 个子命令常量，覆盖装载协商、故障处置、抑制控制、TLB 维护与 32 位遗留五族（§2.3 的分类）。VM 之后做的每一件事，几乎都要经这条通道在内核侧落下对应的状态。
3. **两道闸**。每个非 VM 的 boot 进程出生时就带着 VMINHIBIT 和 BOOTINHIBIT 两个位（[06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §1.4）；协商的收尾动作就是把这两个位按正确的次序、由正确的子命令清掉。

三件交换物的方向与次序画在一起是这样的：

```
          内核（bootstrap 结束，switch_to_user 已执行）
              │
              │ ① 一页交接：写一次，映射给 VM 读
              v
   +--------------------------------------------------+
   |  一页世界状态快照（写者：内核；读者：VM；只此一轮）  |
   |  身份：VM 的地址空间根物理地址                     |
   |  资源：可用内存区间 + 内核扣减记录                 |
   |  内核形状：镜像布局 / DM 窗口 / 恒等窗口           |
   |  社会关系：boot 进程表 + 保留模块表                |
   +--------------------------------------------------+
              │  VM 读一次；之后内核状态再变，
              │  也不影响 VM 已读到的副本
              v
          VM 服务器（唯一在跑的用户进程）
              │
              │ ② 一条通道：SYS_VMCTL（15 个子命令）
              │    对每个 boot 进程：建表 → 绑定 → 装载 → 申报
              v
   +--------------------------------------------------+
   |  两道闸（每个位管一件独立的事）                     |
   |  VMINHIBIT    = "页表还没就绪"  ← SETADDRSPACE 清  |
   |  BOOTINHIBIT  = "VM 还没放行"   ← BOOTINHIBIT_CLEAR 清|
   +--------------------------------------------------+
              │  两道闸都清掉的那一刻，rts 逻辑把进程放进就绪队列
              v
          boot 进程逐台可被调度（10 方向接管）
```

这张图里先记两件事，第 2 章逐一对到代码。第一，交接是**单向一次性**的：VM 读到的是自己地址空间里的一页快照，之后内核状态再变也不影响已读副本——C 侧的对应行为是 `sys_getkinfo` 的一次整块拷贝（`sys_getkinfo` 是 `minix3/minix/include/minix/syslib.h` 里转发 `sys_getinfo(GET_KINFO, …)` 的宏，内核侧拷出点在 `minix3/minix/kernel/system/do_getinfo.c:do_getinfo` 的 GET_KINFO 臂，消费在 `minix3/minix/servers/vm/main.c:init_vm`），拷完之后内核与这份副本再无联系。第二，两条清腿分属两个不同的子命令：SETADDRSPACE 清 VMINHIBIT 但进程仍不可调度（BOOTINHIBIT 还在），BOOTINHIBIT_CLEAR 才是入队时刻。

把"单向一次性"反过来推演，坏点落在状态一致性上。设想协商是双向多轮的：内核随时更新那页交接数据，VM 分多次读。bootstrap 期没有锁，也没有"读到一半内核改了"的重读协议——VM 的每一步决策都依赖"我读到的是哪一版"，而它无从得知版本，两边对同一字段各持一个值，状态撕裂无从裁定。一次性快照让核对成为可能：内核把扣减记录连同空闲区间一起交付，VM 在消费端对账（§3.2 的四检查），账对不上就在入口停机，而不是带着一份没人能验证的账本往前跑。

### 1.2 五步协商（C 视角时序）

一个 boot 进程从"被装载"到"可调度"隔着哪几道门、每道门由哪个子命令打开？C 的答案是一条五步的直线，每步都有明确的 C 锚点。两条闸各自的清除动作本文称作一条**清腿**——腿一清 `VMINHIBIT`、腿二清 `BOOTINHIBIT`（"腿"是闸的比喻，不是源码术语）：

```
 switch_to_user（08 篇收尾，CPU 落到 VM）
      │
      v
 ① VM 上场：boot image 里唯一被放行的进程
      │                        [08-system-init-boot-finish.md](08-system-init-boot-finish.md) §1.3
      v
 ② 读世界状态：一次 sys_getkinfo 装入 kernel_boot_info
      │                        minix3/minix/servers/vm/main.c:init_vm
      v
 ③ 对每个 boot 进程（VM 自己除外）：
      pt_new 建表 → pt_bind 绑定
      pt_bind 尾部发 SETADDRSPACE ────► 内核 setcr3：
          记物理根 + 别名地址；目标是 VM 则开分页；
          一律 RTS_UNSET(VMINHIBIT)（清腿一，不入队）
      │                        minix3/minix/servers/vm/pagetable.c:pt_bind
      │                        minix3/minix/kernel/arch/i386/arch_do_vmctl.c:setcr3
      v
 ④ 装载 ELF + 摆栈帧 + sys_exec（PC/SP 写进进程框）
      │                        minix3/minix/servers/vm/main.c:exec_bootproc
      v
 ⑤ VMCTL_BOOTINHIBIT_CLEAR（"make it runnable"）
      │                        清 BOOTINHIBIT（清腿二，此刻入队）
      v
 进程进入就绪队列，可被调度（10 方向接管）
```

五步里藏着两条清腿的分工。SETADDRSPACE 的内核侧落点是 `minix3/minix/kernel/arch/i386/arch_do_vmctl.c:setcr3`，它的最后一行一律执行 `RTS_UNSET(p, RTS_VMINHIBIT)`——但此刻 BOOTINHIBIT 还在，进程仍不可调度。BOOTINHIBIT_CLEAR 由 VM 在装载全部完成后发出（`minix3/minix/servers/vm/main.c:exec_bootproc` 的收尾一行），这才是进程真正入队的时刻。两道闸各管一件事：VMINHIBIT 的语义是"页表没就绪"，BOOTINHIBIT 的语义是"VM 还没放行"；分两条腿使"表就绪"与"VM 放行"两个条件解耦。

两个条件若合并成一道闸，坏点都能落到具体对象上。若一道闸一次清两个位：ELF 装载失败时进程已经在队列里，调度器会选中一个只有空页表、PC/SP 还是旧框内容的进程，第一条指令执行的就是垃圾现场——装载失败路径的停放语义依赖两个条件分开成立。若 BOOTINHIBIT_CLEAR 由内核在 SETADDRSPACE 时顺手做："VM 放行"的决策权从 VM 滑回内核，VM 失去了"装载完成才放行"这个控制点——装载次序、装载失败的重试，都成了内核猜的事。

### 1.3 交接页里有什么：世界状态的清单

VM 在做任何事之前需要知道世界的哪些事实？按"没有它哪一步做不下去"分，一共四类：

| 信息类 | 一页快照承载的内容 | C 的取数点 |
|---|---|---|
| 身份 | VM 自己的地址空间根物理地址——VM 的初始页表就是内核建的 bootstrap 根这件事，靠这个值接上 | 内核建表后记录在案；VM 侧经 getkinfo 的 kinfo 一并可见（`minix3/minix/kernel/arch/i386/protect.c:arch_post_init` 在内核侧登记根与别名） |
| 资源 | 可用物理内存区间表 + 内核扣减记录（内核镜像、根页、bootstrap 池、保留模块各占哪些区间） | `minix3/minix/servers/vm/main.c:init_vm` 的 memmap 与 `get_mem_chunks`；模块尺寸在 `:init_vm` 的计费循环读 `module_list` |
| 内核形状 | 内核镜像虚拟/物理基址与页数、DM 窗口基址与页数、恒等窗口清单、初始用户栈顶 | kinfo 的 `kernel_allocated_bytes(_dynamic)`、`user_sp` 等（`minix3/minix/kernel/arch/i386/pre_init.c` 装填）；C 的 DM 窗口另由 `freepde_start` 一类参数点拨（`minix3/minix/include/minix/param.h`） |
| 社会关系 | boot 进程表（每台的 proc 号、名字、端点、镜像物理地址与长度）+ 保留模块表 | `minix3/minix/servers/vm/main.c:init_vm` 的 boot_procs 循环；`kernel_boot_info.module_list` |

这张表暴露了一个和直觉不同的形状：**C 的交接物不是一个结构体，而是一次 getkinfo 整块拷入加一组从它解析、散落在 BSS 里的全局变量**。`kernel_boot_info`（`minix3/minix/servers/vm/glo.h`）是其中最大的一个，但模块表、内核足迹、DM 窗口参数各有各的读取入口，VM 的全部函数隐式读这些全局——没有任何一处把这些输入声明成"VM 的启动契约"。"一页契约"是重写发明的形状，它承载的信息清单与 C 相同，差别在显式与隐式——第 3 章按这份清单逐项对位。

清单里少一类信息会坏到哪里，每类不同。漏掉内核形状：VM 给每个进程复刻内核映射时只能用猜的值，映射与真实内核错位，进程一进内核态就在错误的翻译上取指——重写早期用哨兵常量顶过这一段，后来在真机上栽了（§3.1 的版本史里有那次事故的完整链）。漏掉扣减记录：VM 的空闲区间表和内核的实际占用对不上账，分配器会把内核正踩着的页发出去。漏掉身份：VM 不知道自己的根就是 bootstrap 根，会去另建一张，内核替它建的映射全部失联。

### 1.4 本篇范围与可以安全跳过的内容

| 主题 | 本篇怎么办 | 去处 |
|---|---|---|
| VM 服务器内部本体：init_page_table / map_kernel / PMM / region 管理 | 只讲到"清单交付、凭证核验"为止 | `rewrite-notes/02-stage-vm/07-pagetable-struct.md`、`rewrite-notes/02-stage-vm/05-physical-memory.md` |
| 调度循环与 VM 如何被首次调度 | 一句完成式带过 | `10-switch-to-user`（切用户与调度循环篇，尚未重生成） |
| 分派入口与 kernel_call_finish | 一句完成式带过 | `13-syscall-dispatch`（系统调用分派篇，尚未重生成） |
| VMSUSPEND 挂起-恢复运行时 | §2.3 只讲 GET/REPLY 两臂形状与 VMREQUEST 概念首现 | `24-cross-space-runtime`（跨空间运行时篇，尚未重生成） |
| kerninfo 页本体 | 只讲"kerninfo 页被内核代映射进每个新根"这一个动作 | [08-system-init-boot-finish.md](08-system-init-boot-finish.md) §4.3 |
| direct_map 建立 | 引用不重复 | [07-cross-space-init.md](07-cross-space-init.md) §4.4 |

认知减负声明：本章不必深究 MEMREQ 的运行时挂起-恢复协议——GET/REPLY 两臂在本篇只讲形状（谁停车、谁取走、谁放回），进程停车期间内核怎么继续跑别的、恢复时现场怎么接回去，属 `24-cross-space-runtime`（跨空间运行时篇，尚未重生成）的辖区。也不必深究 map_kernel 如何把内核映射复刻进每张进程表——那套机制的实现细节在 `rewrite-notes/02-stage-vm/07-pagetable-struct.md`，本篇只关心"清单从哪里来"。

把第 1 章的图按同一框架收一次：内核写一页（单向、一次性），VM 读一页、走一条通道、按两条腿放行——腿一在 SETADDRSPACE 里清 VMINHIBIT，腿二在 BOOTINHIBIT_CLEAR 里清 BOOTINHIBIT。第 2 章按这张图走 C 代码：先看通道的分派结构，再看支点（setcr3 与 pt_bind）、全序（exec_bootproc）、其余子命令族、VM 侧的取数与计费。

### 1.5 本篇的 C 侧覆盖边界

本篇的 C 侧对照物是 Minix3 的两个内核架构实现（i386 与 earm 分支）。Rust 侧多数动作有 C 逐条可对照（§2 走查、§3.6 处置表逐行对账），少数形态在 C 树里没有对应物：

| Rust 侧分支 / 机制 | C 对照物 | 判定 |
|---|---|---|
| x86-64 主链（setcr3、exec_bootproc、MEMREQ/inhibit/TLB 三族） | i386：`do_vmctl.c` / `arch_do_vmctl.c` / `vm/main.c` | **有 C 可对照**（§2 逐处对账） |
| earm 分支（引文里的 arm 侧） | earm 同名文件 | 有 C 可对照（引文按原样） |
| `VmBootHandoff` 的 repr(C) 独立页（§3.1） | C 的 `kernel_boot_info` 结构 + 多次 `sys_getkinfo` 取数 | 改型对照（投影语义同、承载形态新：一页一次拷贝 vs 十几次取数） |
| `identity_windows` 一次性清单（§3.5） | C 的 per-space 恒等映射协商（`arch_phys_map` / `VMCTL_*` 族） | 改型对照（协议面有 C 形态，清单形态为 Rust 重组） |
| **PteRead/PteWrite/PteZero 三臂（`[ARCH: riscv-vmddm]`，§3.3/§4.2）** | **无**——C 内核没有 riscv64 版本，也没有页表直读写的核内旁路 | **未覆盖（无 C 源）** |

"未覆盖"的含义：Pte 三臂的正确性依据不是 C 对照，而是设计契约（§3.3）与三处一致的 `[ARCH]` 标注。本篇对它一律写作"无对应物"，不写作"与 C 一致"——纪律适用于后续修订中的新增段落。

---

## 2. C 源码分析

### 2.0 本章地图：一条通道的两段分派

SYS_VMCTL 的内核侧 handler 是 `minix3/minix/kernel/system/do_vmctl.c:do_vmctl`，173 行。它的结构是两段分派：先在一个 switch 里处理 9 个通用子命令，switch 没接住的落到 `minix3/minix/kernel/arch/i386/arch_do_vmctl.c:arch_do_vmctl` 再试 4 个架构子命令，两段都没接住则 printf + 返回 EINVAL。文件头的协议注释把消息三字段说在明处：

```c
int do_vmctl(struct proc * caller, message * m_ptr)
{
  int proc_nr;
  endpoint_t ep = m_ptr->SVMCTL_WHO;
  struct proc *p, *rp, **rpp, *target;

  if(ep == SELF) { ep = caller->p_endpoint; }

  if(!isokendpt(ep, &proc_nr)) {
	printf("do_vmctl: unexpected endpoint %d from VM\n", ep);
	return EINVAL;
  }

  p = proc_addr(proc_nr);

  switch(m_ptr->SVMCTL_PARAM) {
```

（`minix3/minix/kernel/system/do_vmctl.c:do_vmctl` 入口段：SELF 解析、端点校验、switch 开始。省略号处为 9 个通用臂的 case 体，§2.3 分族逐段贴出。）

```c
  /* Try architecture-specific vmctls. */
  return arch_do_vmctl(m_ptr, p);
}
```

（`minix3/minix/kernel/system/do_vmctl.c:do_vmctl` 收尾：switch 没接住就交给架构段。）

15 个子命令在两段之间的归属与职责，全景如下表（常量与取值都出自 `minix3/minix/include/minix/com.h`；复算命令：`grep -c 'define VMCTL_' minix3/minix/include/minix/com.h`，实跑得 15）：

| 常量 | 值 | 臂归属 | 职责一句话 |
|---|---|---|---|
| VMCTL_CLEAR_PAGEFAULT | 12 | 通用 | 清目标的 RTS_PAGEFAULT |
| VMCTL_GET_PDBR | 13 | 架构 | 读目标页目录根物理地址 |
| VMCTL_MEMREQ_GET | 14 | 通用 | 取一条挂起的内存请求 |
| VMCTL_MEMREQ_REPLY | 15 | 通用 | 回复内存请求结果并恢复进程 |
| VMCTL_NOPAGEZERO | 18 | 无臂 | 32 位遗留——两段都没接，落 EINVAL 尾 |
| VMCTL_I386_KERNELLIMIT | 19 | 无臂 | 32 位遗留——同上 |
| VMCTL_I386_INVLPG | 25 | 架构 | 单页 TLB 失效 |
| VMCTL_FLUSHTLB | 26 | 架构 | 重载 CR3（全刷非全局 TLB 项） |
| VMCTL_KERN_PHYSMAP | 27 | 通用 | 内核向 VM 逐区申报待恒等映射的物理区 |
| VMCTL_KERN_MAP_REPLY | 28 | 通用 | VM 逐区回报映射到的虚拟地址 |
| VMCTL_SETADDRSPACE | 29 | 架构 | 记新页表根、目标是 VM 则开分页、清 VMINHIBIT |
| VMCTL_VMINHIBIT_SET | 30 | 通用 | 置 VMINHIBIT（SMP 跨 CPU 走 IPI） |
| VMCTL_VMINHIBIT_CLEAR | 31 | 通用 | 清 VMINHIBIT（附 stale_tlb 全填副作用） |
| VMCTL_CLEARMAPCACHE | 32 | 通用 | 清内核的借窗口缓存（07 篇的清理腿） |
| VMCTL_BOOTINHIBIT_CLEAR | 33 | 通用 | 清 BOOTINHIBIT——boot 进程的入队时刻 |

有两个值（18、19）在 `com.h` 里有定义、在两段 handler 里都没有 case——`grep -rn 'VMCTL_NOPAGEZERO\|VMCTL_I386_KERNELLIMIT' minix3/minix/ --include=*.c` 在 handler 文件里零命中（实跑验证）。它们是 32 位时代的遗留：调用号活着，语义已经没有承接者，落进 EINVAL 尾。

还有一个通道级的形状要先点出来：do_vmctl 本体不问 caller 是谁。它校验的是**目标**端点（`isokendpt`），不是发起者的权限；"只有 VM 调它"这件事由 system 进程的特权结构保证——谁能发内核调用、能发哪几号，由 [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) 讲过的调用掩码与陷入口径把守，VM 的特权模板里开着 SYS_VMCTL 这一位。通道本身对身份是盲的，这个形状在第 3 章会成为一处对照（Rust 侧补了一道显式权限门）。

### 2.1 setcr3 与 SETADDRSPACE：协商的支点

要讲清一次 SETADDRSPACE 在内核里改变了哪些状态，得先把 32 位 C 的一个前置机制讲明白：**VM 凭什么能写所有进程的页表**。这一段的机制是"页目录的页目录"，它是本篇 C 侧叙述的主走查对象，后面几章的对照都压在它上面。

i386 是两级页表：页目录 1024 项、每项管 4 MB。每个进程的页目录本身也是一页普通的物理内存。VM 的本职是给每个进程建页表、改页表——页表页（包括页目录）对 VM 必须是可读写的普通数据。C 的解法：拨出最多 5 个 PDE 槽（`minix3/minix/servers/vm/pagetable.c` 的 `MAX_PAGEDIR_PDES`），每个槽挂一张专门的页表页，这张页表的页表项依次指向各进程的页目录物理页——**一张"列出所有页目录"的页表**，也就是页目录的页目录。槽号从哪来？内核在 kinfo 里报告 `freepde_start`（`minix3/minix/include/minix/param.h` 的 `freepde_start` 字段，注释原文是 "lowest pde unused kernel pde"），VM 从这里起顺次取槽（`minix3/minix/servers/vm/pagetable.c:pt_allocate_kernel_mapped_pagetables`）。

这个机制能跨地址空间工作的关键是：**每张新页表都把这几个槽复制进去**。`minix3/minix/servers/vm/pagetable.c:pt_mapkernel` 给每张表映射内核大页之后，紧跟一段 "Kernel also wants to know about all page directories"——把 5 个 pdm 的槽值原样写进 `pt->pt_dir[pdm->pdeno]`。于是同一张物理页表页（里面列着所有进程的页目录）出现在每个地址空间的同一批虚拟地址上；VM 写它自己视图里的这份表，就是在写所有地址空间共享的那份表。内核侧的消费现场在 `minix3/minix/kernel/arch/i386/memory.c:createpde`：它解引用目标的 `p_cr3_v`（目标页目录的别名虚拟地址）读旧 PDE 值、再写进当前表的别名槽——这套借窗口动作 [07-cross-space-init.md](07-cross-space-init.md) §2.3 已完整走过，此处只指出依赖关系：createpde 能从任何上下文读写任意进程的页目录，靠的正是这套别名在每张表里同址存在。

有了这个前置，`pt_bind` 就是一段能逐行读懂的代码。它做的三件事：算出目标进程的页目录挂在共享表里的哪个槽、把物理根写进那个槽、把别名虚拟地址报给内核：

```c
int pt_bind(pt_t *pt, struct vmproc *who)
{
	int procslot, pdeslot;
	u32_t phys;
	void *pdes;
	int pagedir_pde;
	int slots_per_pde;
	int pages_per_pagedir = ARCH_PAGEDIR_SIZE/VM_PAGE_SIZE;
	struct pdm *pdm;

	slots_per_pde = ARCH_VM_PT_ENTRIES / pages_per_pagedir;

	/* Basic sanity checks. */
	assert(who);
	assert(who->vm_flags & VMF_INUSE);
	assert(pt);

	procslot = who->vm_slot;
	pdm = &pagedir_mappings[procslot/slots_per_pde];
	pdeslot = procslot%slots_per_pde;
	pagedir_pde = pdm->pdeno;
	assert(pdeslot >= 0);
	assert(procslot < ELEMENTS(vmproc));
	assert(pdeslot < ARCH_VM_PT_ENTRIES / pages_per_pagedir);
	assert(pagedir_pde >= 0);

#if defined(__i386__)
	phys = pt->pt_dir_phys & ARCH_VM_ADDR_MASK;
#elif defined(__arm__)
	phys = pt->pt_dir_phys & ARM_VM_PTE_MASK;
#endif
	assert(pt->pt_dir_phys == phys);
	assert(!(pt->pt_dir_phys % ARCH_PAGEDIR_SIZE));

	/* Update "page directory pagetable." */
#if defined(__i386__)
	pdm->page_directories[pdeslot] =
		phys | ARCH_VM_PDE_PRESENT|ARCH_VM_PTE_RW;
#elif defined(__arm__)
{
	int i;
	for (i = 0; i < pages_per_pagedir; i++) {
		pdm->page_directories[pdeslot*pages_per_pagedir+i] =
			(phys+i*VM_PAGE_SIZE)
			| ARCH_VM_PTE_PRESENT
			| ARCH_VM_PTE_RW
			| ARM_VM_PTE_CACHED
			| ARCH_VM_PTE_USER; //LSC FIXME
	}
}
#endif

	/* This is where the PDE's will be visible to the kernel
	 * in its address space.
	 */
	pdes = (void *) (pagedir_pde*ARCH_BIG_PAGE_SIZE + 
#if defined(__i386__)
			pdeslot * VM_PAGE_SIZE);
#elif defined(__arm__)
			pdeslot * ARCH_PAGEDIR_SIZE);
#endif

	/* Tell kernel about new page table root. */
	return sys_vmctl_set_addrspace(who->vm_endpoint, pt->pt_dir_phys , pdes);
}
```

（`minix3/minix/servers/vm/pagetable.c:pt_bind`，逐字引用。`pages_per_pagedir` 在 i386 上为 1（一页目录恰好一页），`slots_per_pde` 为 1024；`pdm` 按 `procslot/1024` 选共享表、`pdeslot` 按 `procslot%1024` 选表内项；`pdes` 的算式 `pagedir_pde*ARCH_BIG_PAGE_SIZE + pdeslot*VM_PAGE_SIZE` 就是"目标页目录被当作数据页映射出来的那个虚拟地址"。）

三件事各有指向。写 `pdm->page_directories[pdeslot]` 是把"进程 P 的页目录在物理地址 X"发布进共享表——从此 P 的所有页表页都进了 VM 的可寻址范围。算 `pdes` 是得到"P 的页目录在（每张）地址空间里的别名虚拟地址"。发 `sys_vmctl_set_addrspace(who->vm_endpoint, pt->pt_dir_phys, pdes)` 把两个地址一起递给内核——`minix3/minix/lib/libsys/sys_vmctl.c:sys_vmctl_set_addrspace` 就是把它装进 SYS_VMCTL 消息（SVMCTL_PTROOT 带物理根、SVMCTL_PTROOT_V 带别名）的 wrapper。在整台 VM 服务器里，SETADDRSPACE 只有这一个发出点：`grep -rn 'set_addrspace\|SETADDRSPACE' minix3/minix/servers/vm/ minix3/minix/lib/libsys/sys_vmctl.c` 的命中只有 pt_bind 尾这一处调用加 wrapper 本体（实跑验证）——建表与换表的所有路径最终都收敛到 pt_bind。

消息进内核后落在 `minix3/minix/kernel/arch/i386/arch_do_vmctl.c:setcr3`，全函数 15 行，三分支：

```c
static void setcr3(struct proc *p, u32_t cr3, u32_t *v)
{
	/* Set process CR3. */
	p->p_seg.p_cr3 = cr3;
	assert(p->p_seg.p_cr3);
	p->p_seg.p_cr3_v = v; 
	if(p == get_cpulocal_var(ptproc)) {
		write_cr3(p->p_seg.p_cr3);
	}
	if(p->p_nr == VM_PROC_NR) {
		if (arch_enable_paging(p) != OK)
			panic("arch_enable_paging failed");
	}
	RTS_UNSET(p, RTS_VMINHIBIT);
}
```

（`minix3/minix/kernel/arch/i386/arch_do_vmctl.c:setcr3`，逐字引用。）

三分支各自的防点列成表：

| 分支 | 条件 | 动作 | 防什么 |
|---|---|---|---|
| 双记录 | 无条件 | `p_cr3 = cr3`（物理根，给硬件）；`p_cr3_v = v`（别名，给内核代写 PTE 用——07 篇 §2.1 已立这两个记录的分工） | 两个消费方各用各的地址形态：调度器装 CR3 用前者，内核代写页表用后者 |
| 在位特判 | `p == get_cpulocal_var(ptproc)` | `write_cr3(p->p_seg.p_cr3)` | 目标正是当前装着的那张表时，硬件寄存器必须跟上，否则记录与翻译脱节 |
| VM 特判 | `p->p_nr == VM_PROC_NR` | `arch_enable_paging(p)`，失败即 panic | VM 第一次绑定自己新表的时刻，真正把分页切到 VM 的根上 |

第三个分支的时点要先看清楚：它在 `pt_init` 里触发（`minix3/minix/servers/vm/pagetable.c` 的 pt_init 尾段对 VM 自己调 pt_bind）。bootstrap 期内核替 VM 跑在恒等映射上；VM 建好自己的第一张表并绑定，`arch_enable_paging` 在这一刻把 CPU 切到 VM 的根——这是 32 位"VM 上电分页"的时刻。此后 VM 一辈子不再换自己的根：它给别人建表、换表，全是写共享 PDE 槽，自己的 CR3 纹丝不动。setcr3 也没有失败路径：它是 `static void`，唯一可能出错的 `arch_enable_paging` 用 panic 表达——协商的支点不允许带错往前走。

两个分支漏掉的后果各落到一个具体对象上。若 VM 分支漏掉 `arch_enable_paging`：VM 继续跑在内核替它建的恒等映射上，它随后写的所有"进程页表"都进不了自己的地址空间——它以为自己填好的是将来要用的表，硬件翻译走的还是那张只有恒等映射的旧表，协商在第一步就静默失败，直到第一次高位地址访问才炸出来。若 ptproc 特判删掉：正在 CPU 上跑的进程换了表而 CR3 不跟，记录说"新根"，硬件翻译走"旧根"，下一条取指就在错位的翻译上找指令。

SETADDRSPACE 的最后一行 `RTS_UNSET(p, RTS_VMINHIBIT)` 是五步时序里的清腿一。此刻 BOOTINHIBIT 还在，`rts` 的入队逻辑不会触发——表就绪了，进程仍然不可调度，等的是腿二。

### 2.2 exec_bootproc：装载与放行的全序

表绑定好之后，VM 还要替内核做一遍原本由 C 启动代码做的事：读 ELF 头、搬段、摆初始栈、把入口交给内核。`minix3/minix/servers/vm/main.c:exec_bootproc` 是这段全序的载体，函数体分两段看。前半段：建表绑定、读头、装载：

```c
static void exec_bootproc(struct vmproc *vmp, struct boot_image *ip)
{
	struct vm_exec_info vmexeci;
	struct exec_info *execi = &vmexeci.execi;
	/* libexec need proper alignment for casting to structures */
	char hdr[VM_PAGE_SIZE] __aligned(8);

	size_t frame_size = 0;	/* Size of the new initial stack. */
	int argc = 0;		/* Argument count. */
	int envc = 0;		/* Environment count */
	char overflow = 0;	/* No overflow yet. */
	struct ps_strings *psp;

	int vsp = 0;	/* (virtual) Stack pointer in new address space. */
	char *argv[] = { ip->proc_name, NULL };
	char *envp[] = { NULL };
	char *path = ip->proc_name;
	char frame[VM_PAGE_SIZE] __aligned(sizeof(void *));

	memset(&vmexeci, 0, sizeof(vmexeci));

	if(pt_new(&vmp->vm_pt) != OK)
		panic("VM: no new pagetable");

	if(pt_bind(&vmp->vm_pt, vmp) != OK)
		panic("VM: pt_bind failed");

	if(sys_physcopy(NONE, ip->start_addr, SELF,
		(vir_bytes) hdr, sizeof(hdr), 0) != OK)
		panic("can't look at boot proc header");

	execi->stack_high = kernel_boot_info.user_sp;
	execi->stack_size = DEFAULT_STACK_LIMIT;
	execi->proc_e = vmp->vm_endpoint;
	execi->hdr = hdr;
	execi->hdr_len = sizeof(hdr);
	strlcpy(execi->progname, ip->proc_name, sizeof(execi->progname));
	execi->frame_len = 0;
	execi->opaque = &vmexeci;
	execi->filesize = ip->len;

	vmexeci.ip = ip;
	vmexeci.vmp = vmp;

	/* callback functions and data */
	execi->copymem = libexec_copy_physcopy;
	execi->clearproc = NULL;
	execi->clearmem = libexec_clear_sys_memset;
	execi->allocmem_prealloc_junk = libexec_alloc_vm_prealloc;
	execi->allocmem_prealloc_cleared = libexec_alloc_vm_prealloc;
	execi->allocmem_ondemand = libexec_alloc_vm_ondemand;

	if (libexec_load_elf(execi) != OK)
		panic("vm: boot process load of process %s (ep=%d) failed\n", 
			execi->progname, vmp->vm_endpoint);
```

（`minix3/minix/servers/vm/main.c:exec_bootproc` 前半，逐字引用。`hdr` 是一页对齐缓冲，`sys_physcopy` 把镜像头从物理地址读进来——libexec 需要按结构体对齐访问它。）

后半段：栈帧、入口、放行：

```c
	/* Setup a minimal stack. */
	minix_stack_params(path, argv, envp, &frame_size, &overflow, &argc,
		&envc);

	/* The party is off if there is an overflow, or it is too big for our
	 * pre-allocated space. */
	if(overflow || frame_size > sizeof(frame))
		panic("vm: could not alloc stack for boot process %s (ep=%d)\n",
			execi->progname, vmp->vm_endpoint);

	minix_stack_fill(path, argc, argv, envc, envp, frame_size, frame, &vsp,
		&psp);

	if(handle_memory_once(vmp, vsp, frame_size, 1) != OK)
		panic("vm: could not map stack for boot process %s (ep=%d)\n",
			execi->progname, vmp->vm_endpoint);

	if(sys_datacopy(SELF, (vir_bytes)frame, vmp->vm_endpoint, vsp, frame_size) != OK)
		panic("vm: could not copy stack for boot process %s (ep=%d)\n",
			execi->progname, vmp->vm_endpoint);

	if(sys_exec(vmp->vm_endpoint, (vir_bytes)vsp,
		   (vir_bytes)execi->progname, execi->pc,
		   vsp + ((int)psp - (int)frame)) != OK)
		panic("vm: boot process exec of process %s (ep=%d) failed\n",
			execi->progname,vmp->vm_endpoint);

	/* make it runnable */
	if(sys_vmctl(vmp->vm_endpoint, VMCTL_BOOTINHIBIT_CLEAR, 0) != OK)
		panic("VMCTL_BOOTINHIBIT_CLEAR failed");
}
```

（`minix3/minix/servers/vm/main.c:exec_bootproc` 后半，逐字引用。收尾一行就是五步时序的第五步，注释原文 "make it runnable"。）

按执行序拆成步序表，每步的 C 锚点与状态后果：

| 步 | 动作 | C 锚 | 状态后果 |
|---|---|---|---|
| 1 | `pt_new` 建新表 | `exec_bootproc` 首个调用 | 目标进程获得一张空白表（内核映射由 pt_mapkernel 复刻） |
| 2 | `pt_bind` 绑定 | 紧随其后 | SETADDRSPACE 发出；内核清 VMINHIBIT（腿一） |
| 3 | `sys_physcopy` 读 ELF 头 | `hdr` 装填 | 一页头进 VM 可见缓冲 |
| 4 | `libexec_load_elf` 装载 | 回调注入后调用 | 段字节经回调搬进目标地址空间 |
| 5 | `minix_stack_params`/`minix_stack_fill` 算栈帧 | 栈段 | argc/argv/envp/ps_strings 摆进一页帧缓冲 |
| 6 | `handle_memory_once` + `sys_datacopy` | 栈帧落地 | 栈区间映射确认、帧字节拷进目标空间 |
| 7 | `sys_exec` | 收尾前 | PC/SP/ps_str 写进内核的进程框 |
| 8 | `VMCTL_BOOTINHIBIT_CLEAR` | 最后一行 | BOOTINHIBIT 清除；进程入队（腿二） |

第 4 步的回调注入要先说清形状：libexec 是一个与内存后端无关的装载库，它不直接碰内核——`execi->copymem`/`clearmem`/`allocmem_*` 三族函数指针由 VM 填入自己的实现（physcopy 搬字节、sys_memset 清零、boot_alloc 分配），libexec 只负责按 ELF 语义驱动这些回调。装载的"怎么搬"是 VM 的知识，"搬什么"是 libexec 的知识。

步 7 有一个和直觉不同的形状：**`sys_exec` 本身不装载**。它只把 PC、SP、ps_str 三样写进内核的进程框（内核侧的落点是 do_exec 的 arch_proc_init 路径），段字节早在第 4 步就由 VM 搬完了。装载在服务器、切换在内核——这个分工让内核不需要理解 ELF，也让"装载失败"停留在 VM 侧，不会把半装好的现场留给内核。

步序的次序是硬的。把步 8 挪到步 7 之前：进程在 PC/SP 就位前入队，首个调度切片执行的是旧框里的垃圾现场——进程框在 exec 之前装的是 fork 语义留下的旧值，不是任何合法入口。把步 2 删掉直接装载：SETADDRSPACE 没发，VMINHIBIT 不清，后面全白做——进程停在"表没就绪"那道闸后面。

### 2.3 其余子命令族：MEMREQ、inhibit、TLB 三族

装载协商之外，通道平时还办四类事。按"装载协商 / 故障处置 / 抑制控制 / TLB 维护 / 32 位遗留"五族把 15 个子命令归位（SETADDRSPACE 与 BOOTINHIBIT_CLEAR 已在 §2.1/§2.2 讲过，本节补齐其余）。

**故障处置族**：CLEAR_PAGEFAULT 与 MEMREQ 两臂。前者一行：

```c
	case VMCTL_CLEAR_PAGEFAULT:
		assert(RTS_ISSET(p,RTS_PAGEFAULT));
		RTS_UNSET(p, RTS_PAGEFAULT);
		return OK;
```

（`minix3/minix/kernel/system/do_vmctl.c:do_vmctl` 的 CLEAR_PAGEFAULT 臂，逐字引用。）

后者是本节的主块。要读懂它，先补一个概念的首现：**VMREQUEST**。一个进程在用户态碰到了 VM 必须裁决的内存事件（越界访问、缺页之类），内核把它的请求挂进全局的 vmrequest 链、给它置 RTS_VMREQUEST——进程停车，等 VM 裁决（`vmresult` 记为 `VMSUSPEND` 的动作在 MEMREQ_GET 臂里，见下文引用）。GET 臂就是 VM 来取单：

```c
	case VMCTL_MEMREQ_GET:
		/* Send VM the information about the memory request. We can
		 * not simply send the first request on the list, because IPC
		 * filters may forbid VM from getting requests for particular
		 * sources. However, IPC filters are used only in rare cases.
		 */
		for (rpp = &vmrequest; *rpp != NULL;
		    rpp = &(*rpp)->p_vmrequest.nextrequestor) {
			rp = *rpp;

			assert(RTS_ISSET(rp, RTS_VMREQUEST));

			okendpt(rp->p_vmrequest.target, &proc_nr);
			target = proc_addr(proc_nr);

			/* Check against IPC filters. */
			if (!allow_ipc_filtered_memreq(rp, target))
				continue;

			/* Reply with request fields. */
			if (rp->p_vmrequest.req_type != VMPTYPE_CHECK)
				panic("VMREQUEST wrong type");

			m_ptr->SVMCTL_MRG_TARGET	=
				rp->p_vmrequest.target;
			m_ptr->SVMCTL_MRG_ADDR		=
				rp->p_vmrequest.params.check.start;
			m_ptr->SVMCTL_MRG_LENGTH	=
				rp->p_vmrequest.params.check.length;
			m_ptr->SVMCTL_MRG_FLAG		=
				rp->p_vmrequest.params.check.writeflag;
			m_ptr->SVMCTL_MRG_REQUESTOR	=
				(void *) rp->p_endpoint;

			rp->p_vmrequest.vmresult = VMSUSPEND;

			/* Remove from request chain. */
			*rpp = rp->p_vmrequest.nextrequestor;

			return rp->p_vmrequest.req_type;
		}

		return ENOENT;
```

（`minix3/minix/kernel/system/do_vmctl.c:do_vmctl` 的 MEMREQ_GET 臂，逐字引用。回填的五个 SVMCTL_MRG_* 字段（目标、地址、长度、读写旗标、请求者）就是 VM 裁决所需的全部现场。）

REPLY 臂是反向：VM 裁决完把结果写回、把进程放回运行的轨道：

```c
	case VMCTL_MEMREQ_REPLY:
		assert(RTS_ISSET(p, RTS_VMREQUEST));
		assert(p->p_vmrequest.vmresult == VMSUSPEND);
  		okendpt(p->p_vmrequest.target, &proc_nr);
		target = proc_addr(proc_nr);
		p->p_vmrequest.vmresult = m_ptr->SVMCTL_VALUE;
		assert(p->p_vmrequest.vmresult != VMSUSPEND);

		switch(p->p_vmrequest.type) {
		case VMSTYPE_KERNELCALL:
			/*
			 * we will have to resume execution of the kernel call
			 * as soon the scheduler picks up this process again
			 */
			p->p_misc_flags |= MF_KCALL_RESUME;
			break;
		case VMSTYPE_DELIVERMSG:
			assert(p->p_misc_flags & MF_DELIVERMSG);
			assert(p == target);
			assert(RTS_ISSET(p, RTS_VMREQUEST));
			break;
		case VMSTYPE_MAP:
			assert(RTS_ISSET(p, RTS_VMREQUEST));
			break;
		default:
			panic("strange request type: %d",p->p_vmrequest.type);
		}

		RTS_UNSET(p, RTS_VMREQUEST);
		return OK;
```

（`minix3/minix/kernel/system/do_vmctl.c:do_vmctl` 的 MEMREQ_REPLY 臂，逐字引用。）

GET 臂开头那段注释是 IPC 过滤器的存在证明：请求链不能无脑取第一条，因为 IPC filters 可能禁止 VM 得知某些源进程的请求——过滤器罕用，但罕用不等于可跳。若 GET 臂不做过滤检查：被过滤源进程的请求照常被 VM 取走处理，过滤策略在唯一有执行权的路径上旁路。REPLY 臂按请求类型三分支：KERNELCALL 型置 MF_KCALL_RESUME（内核调用的恢复点在调度器再选中它时），DELIVERMSG 与 MAP 型用断言确认现场齐备，类型不对直接 panic——vmresult 写错类型是内核不变量的破坏，不是可恢复错误。进程停车之后现场怎么接回去，归 `24-cross-space-runtime`（跨空间运行时篇，尚未重生成）；本篇只认得"GET 摘链停车、REPLY 写回放行"这个形状。

**抑制控制族**：VMINHIBIT_SET/CLEAR。SET 臂处理 SMP 下目标在别的 CPU 的情况，CLEAR 臂带一段全文件最长的注释——stale_tlb 全填的理由：

```c
	case VMCTL_VMINHIBIT_SET:
		/* check if we must stop a process on a different CPU */
#if CONFIG_SMP
		if (p->p_cpu != cpuid) {
			smp_schedule_vminhibit(p);
		} else
#endif
			RTS_SET(p, RTS_VMINHIBIT);
#if CONFIG_SMP
		p->p_misc_flags |= MF_FLUSH_TLB;
#endif
		return OK;
	case VMCTL_VMINHIBIT_CLEAR:
		assert(RTS_ISSET(p, RTS_VMINHIBIT));
		/*
		 * the processes is certainly not runnable, no need to tell its
		 * cpu
		 */
		RTS_UNSET(p, RTS_VMINHIBIT);
#ifdef CONFIG_SMP
		if (p->p_misc_flags & MF_SENDA_VM_MISS) {
			struct priv *privp;
			p->p_misc_flags &= ~MF_SENDA_VM_MISS;
			privp = priv(p);
			try_deliver_senda(p, (asynmsg_t *) privp->s_asyntab,
							privp->s_asynsize);
		}
		/*
		 * We don't know whether kernel has the changed mapping
		 * installed to access userspace memory. And if so, on what CPU.
		 * More over we don't know what mapping has changed and how and
		 * therefore we must invalidate all mappings we have anywhere.
		 * Next time we map memory, we map it fresh.
		 */
		bits_fill(p->p_stale_tlb, CONFIG_MAX_CPUS);
#endif
		return OK;
```

（`minix3/minix/kernel/system/do_vmctl.c:do_vmctl` 的 VMINHIBIT_SET/CLEAR 两臂，逐字引用。）

CLEAR 臂的两个副作用各有来历。MF_SENDA_VM_MISS 重试：VM 抑制目标期间，别处可能有一次 senda 因目标不可投递而落了 miss 标记，解抑制时补投。stale_tlb 全填是注释给的那段理由：内核不知道自己装的临时映射落在哪台 CPU 上、也不知道哪些映射变过，于是把目标在所有 CPU 上的 TLB 项全部作废，下次映射重新来。这是 32 位别名窗口的代价——借窗口机制让"内核装过哪个映射"成为跨 CPU 的未知数（[07-cross-space-init.md](07-cross-space-init.md) §2.3 已立窗口机制），64 位的常驻 DM 下翻译恒定，这类全填没有存在理由。

**TLB 维护族**：GET_PDBR、FLUSHTLB、INVLPG 三个架构臂，§2.1 已见 setcr3 所在文件，三个臂各两三行——GET_PDBR 读 `p->p_seg.p_cr3` 回填、FLUSHTLB 重载 CR3、INVLPG 对单个地址失效。FLUSHTLB 的 VM 侧消费密度是个可数的形状：`grep -rn 'VMCTL_FLUSHTLB' minix3/minix/servers/vm/` 实跑得 7 处调用表达式——pagetable.c 六处（其中两处集中在 pt_init 收尾的动态重绑段，§2.1 的共享表页在初始化后期被换过一批物理页，换完要全刷）加 utility.c 一处。TLB 失效的密度跟着别名窗口的写密度走，这也是第 3 章一处对照的背景。

**32 位遗留族**：KERN_PHYSMAP/KERN_MAP_REPLY 是一对逐区协商协议，第 3 章要拿它们当对照物，这里把机制讲全。内核侧有个 `.usermapped` 段——内核地址空间里划给用户进程可见的内核数据（minix_kerninfo 一类）；32 位下每个进程页表都要把它映射进去，而"它有哪些物理区、VM 该映射到哪"内核和 VM 各知一半，于是有了两臂对话：

```c
	case VMCTL_KERN_PHYSMAP:
	{
		int i = m_ptr->SVMCTL_VALUE;
		return arch_phys_map(i,
			(phys_bytes *) &m_ptr->SVMCTL_MAP_PHYS_ADDR,
			(phys_bytes *) &m_ptr->SVMCTL_MAP_PHYS_LEN,
			&m_ptr->SVMCTL_MAP_FLAGS);
	}
	case VMCTL_KERN_MAP_REPLY:
	{
		return arch_phys_map_reply(m_ptr->SVMCTL_VALUE,
			(vir_bytes) m_ptr->SVMCTL_MAP_VIR_ADDR);
	}
```

（`minix3/minix/kernel/system/do_vmctl.c:do_vmctl` 的 KERN_PHYSMAP/KERN_MAP_REPLY 两臂，逐字引用。`arch_phys_map` 的实现在 `minix3/minix/kernel/arch/i386/memory.c:arch_phys_map`：按索引依次交出 video 内存、`.usermapped` 全局段、local APIC 页等物理区；`arch_phys_map_reply` 接住 VM 回报的虚拟地址记账。）

对话的形状是：VM 先连发 GET（"下一区是什么"）直到取尽，把每区恒等映射进自己地址空间，再逐区 REPLY（"我映射到了这个虚拟地址"）——内核据此把 `.usermapped` 的地址通报给用户进程。CLEARMAPCACHE 臂也在本节收尾：

```c
	case VMCTL_CLEARMAPCACHE:
		/* VM says: forget about old mappings we have cached. */
		mem_clear_mapcache();
		return OK;
	case VMCTL_BOOTINHIBIT_CLEAR:
		RTS_UNSET(p, RTS_BOOTINHIBIT);
		return OK;
```

（`minix3/minix/kernel/system/do_vmctl.c:do_vmctl` 的 CLEARMAPCACHE 与 BOOTINHIBIT_CLEAR 两臂，逐字引用。前者是 07 篇 §2.5 清理腿的通道落点；后者一行，是 §2.2 的腿二。）

### 2.4 VM 侧消费：init_vm 的取数与计费

世界状态到了 VM 手里，先做哪三件账？`minix3/minix/servers/vm/main.c:init_vm` 的开头给出次序：断言、空闲链、补计费。断言与取数：

```c
	/* Retrieve various crucial boot parameters */
	if(OK != (s=sys_getkinfo(&kernel_boot_info))) {
		panic("couldn't get bootinfo: %d", s);
	}

	/* Turn file mmap on? */
	enable_filemap=1;	/* yes by default */
	env_parse("filemap", "d", 0, &enable_filemap, 0, 1);

	/* Sanity check */
	assert(kernel_boot_info.mmap_size > 0);
	assert(kernel_boot_info.mods_with_kernel > 0);
```

（`minix3/minix/servers/vm/main.c:init_vm` 开头段，逐字引用。`kernel_boot_info` 是 `minix3/minix/servers/vm/glo.h` 声明的 BSS 全局，`sys_getkinfo` 一次拷入。）

补计费与 boot 循环：

```c
	/* The kernel's freelist does not include boot-time modules; let
	 * the allocator know that the total memory is bigger.
	 */
	for (mod = &kernel_boot_info.module_list[0];
		mod < &kernel_boot_info.module_list[kernel_boot_info.mods_with_kernel-1]; mod++) {
		phys_bytes len = mod->mod_end-mod->mod_start+1;
		len = roundup(len, VM_PAGE_SIZE);
		mem_add_total_pages(len/VM_PAGE_SIZE);
	}

	kern_dyn = kernel_boot_info.kernel_allocated_bytes_dynamic;
	kern_static = kernel_boot_info.kernel_allocated_bytes;
	kern_static = roundup(kern_static, VM_PAGE_SIZE);
	mem_add_total_pages((kern_dyn + kern_static)/VM_PAGE_SIZE);

	/* Give these processes their own page table. */
	for (ip = &kernel_boot_info.boot_procs[0];
		ip < &kernel_boot_info.boot_procs[NR_BOOT_PROCS]; ip++) {
		struct vmproc *vmp;

		if(ip->proc_nr < 0) continue;

		assert(ip->start_addr);

		/* VM has already been set up by the kernel and pt_init().
		 * Any other boot process is already in memory and is set up
		 * here.
		 */
		if(ip->proc_nr == VM_PROC_NR) continue;

		vmp = init_proc(ip->proc_nr);

		exec_bootproc(vmp, ip);

		/* Free the file blob */
		assert(!(ip->start_addr % VM_PAGE_SIZE));
		ip->len = roundup(ip->len, VM_PAGE_SIZE);
		free_mem(ABS2CLICK(ip->start_addr), ABS2CLICK(ip->len));
	}
```

（`minix3/minix/servers/vm/main.c:init_vm` 计费段与 boot 进程循环，逐字引用。）

三件账的次序与内容：第一件，两条断言把世界状态的底线钉住——内存图非空、模块数非零，坏了就 panic，VM 不带着残缺的账开工。第二件，`get_mem_chunks` 与 `mem_init` 从内存图建空闲链。第三件，补计费：内核的空闲链不含 boot 期占用的内存，VM 要把总页数补成真实物理内存的规模——模块 blob 一份、内核自身足迹一份。

模块计费那个循环的上界值得对着源码解码（此处解码不引入任何新断言，只读代码）：循环变量从 `module_list[0]` 走到 `module_list[mods_with_kernel-1]` **之前**——最后一项被跳过。跳过的是谁？看 `minix3/minix/kernel/arch/i386/pre_init.c` 的模块表装配段：内核把自己的镜像**追加在模块表末尾**（注释原文 "Pretend the kernel is an extra module"，`kern_mod` 取的就是最后那个下标、`mods_with_kernel` 比真实模块数多一）。所以 -1 上界跳过的是**内核自己的条目**——内核内存不走这条循环，它由紧随其后的 `kernel_allocated_bytes(_dynamic)` 那两行单独补上；若把内核条目也计进循环，同一份内核镜像会被记两次，空闲链总页数系统性多出一份内核大小。

VM 自己的条目在这张表里是另一个形状。内核在启动期装载 VM 的 ELF 时就把 blob 回收了——`minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc` 的回收段（注释原文 "Free VM blob that was just copied into existence"）把 VM 模块条目的 mod_start/mod_end 清零、内存还回内存图。所以 boot 进程装载循环里 `if(ip->proc_nr == VM_PROC_NR) continue;`——VM 不走 exec_bootproc，它的地址空间在 pt_init 里已由自己重建（§2.1 的 arch_enable_paging 分支）。但注意计费循环对这条被清零的条目**并不跳过**：`mod_end-mod_start+1` 对清零条目算出 1，取整后仍是一页——C 的总页数里有一页"幽灵计费"（把一个已不存在的模块算作占一页）。这是 C 现码的真实行为；重写侧的对应物没有这笔幽灵——`modules` 表在内核侧构建时就排除了 VM 自己，`extra_pages` 的循环碰不到它。

最后一个反向推演：若 VM 给自己计费——总页数会多算一份已消耗的 blob，空闲链与真实可用内存系统性偏差；若把内核条目也计费——偏差一份内核镜像。两份都是"把已占用的页再卖给分配器"的形状，只是份量不同。

### 2.5 本章归纳：C 协商总表

把 §2.1-§2.4 的动作按执行序串成一张总表，发起方、锚点、状态后果逐行对齐：

| 序 | 动作 | 发起方 | C 锚 | 状态后果 |
|---|---|---|---|---|
| 1 | 一次 getkinfo 装入世界状态 | VM | `main.c:init_vm` | BSS 全局就位；两条断言过 |
| 2 | pt_init：建共享页目录表、绑自己的根 | VM | `pagetable.c:pt_bind`（对 VM） | setcr3 走 VM 分支：开分页；VM 根自此恒定 |
| 3 | 每 boot 进程：pt_new + pt_bind | VM | `pagetable.c:pt_bind` | SETADDRSPACE：双记录 + 在位特判；清 VMINHIBIT（腿一，不入队） |
| 4 | 每 boot 进程：读头、装载段、摆栈 | VM | `main.c:exec_bootproc` | 段字节进目标空间；帧字节进栈页 |
| 5 | 每 boot 进程：sys_exec | VM | `main.c:exec_bootproc` | PC/SP/ps_str 进内核进程框 |
| 6 | 每 boot 进程：BOOTINHIBIT_CLEAR | VM | `main.c:exec_bootproc` 收尾 | 清 BOOTINHIBIT；rts 入队（腿二） |
| — | 运行期：MEMREQ GET/REPLY、inhibit、TLB 维护 | VM | `do_vmctl.c` 各臂 | 故障裁决、抑制控制、翻译维护 |

表里能读出的硬次序约束只有两条。其一，SETADDRSPACE 先于 BOOTINHIBIT_CLEAR：两道闸的次序，反向推演（装载失败即入队、决策权滑动）在 §1.2 已经落到具体对象。其二，取数与断言先于任何分配：先知世界再动手，断言失败停在开工前——若次序反掉，分配器会基于一份没验过的账发页，错误从入口断言退化成运行期腐坏。

带着这张表进第 3 章：Rust 把哪几拍重排了、哪几件东西换了形状、哪几件消失了。

---

## 3. Rust 设计决策

### 3.1 一页契约取代十几次取数：VmBootHandoff（→§2.4）

§2.4 留在 C 里的负担有两个：世界状态摊在一次整块拷入与一组 BSS 隐藏全局里，VM 的每个函数都在隐式依赖没人声明的输入；且这些输入没有任何入口校验——账目对不对，VM 无从对起。Rust 的决定是把"世界状态"压成一张显式的、可校验的一页契约：`os/libs/minix-types/src/types/boot.rs:VmBootHandoff`，`#[repr(C)]`、编译期断言不超过一页，内核写一次、以用户只读映射固定在 `os/libs/minix-types/src/types/boot.rs:VM_BOOT_HANDOFF_VA`（`0x1_0000_0000`），VM 启动时读一次。类型库的模块注释把这个协议的指向写成了对本篇的引用：

```rust
//! # Why an explicit struct (not a hidden global)
//!
//! C keeps `kernel_boot_info` in BSS and every VM function reads it
//! implicitly. Rust models the kernel→VM boot hand-off as an explicit
//! input to [`VmServer::new_with_boot_params`](crate::VmServer):
//! the boot protocol (see `01-stage-kernel/09-vm-boot-protocol.md`) is
//! one-way and one-shot, so making the dependency visible at the
//! constructor keeps the startup chain auditable and testable.
```

（`os/servers/vm/src/boot.rs` 模块注释，逐字引用。注释里指向 `01-stage-kernel/09-vm-boot-protocol.md` 的那一句，正是源代码反向引用本篇的地方——boot 协议的文档落点在本篇。）

页的头部文档把单向一次性、身份交接与资源交接（A1/A2）三件事说全：

```rust
/// Kernel → VM boot handoff page (one-way, one-shot).
///
/// The kernel fills this struct in a dedicated frame before scheduling VM
/// and maps the frame read-only, user-accessible at
/// [`VM_BOOT_HANDOFF_VA`]. VM reads it once during startup; later kernel
/// state changes do not affect the already-read copy.
///
/// `root_paddr` is the address-space identity hand-off (A1): VM's initial
/// page table IS the bootstrap root the kernel built and enabled, so VM
/// constructs its self page-table handle from this physical address
/// instead of creating a fresh root. `root_paddr == 0` is invalid — the
/// bootstrap root is always a real, page-aligned physical page.
///
/// `free_regions` + `deducted` are the resource hand-off (A2,
/// post-bootstrap classification): the kernel finalizes VM's free list by
/// cutting `LiveBootstrap(t_classify)` out of the full memmap and hands
/// the deduction record itself over, so VM can verify
/// `LiveBootstrap ∩ VM-free = ∅` at the consumer boundary instead of
/// trusting the kernel's bookkeeping.
///
/// The struct must fit one 4 KiB page (`size_of::<VmBootHandoff>() <= 4096`).
```

（`os/libs/minix-types/src/types/boot.rs:VmBootHandoff` 头部文档，逐字引用。）

信息清单与 §1.3 的四类一一对应。字段骨架（签名示意——注释为本篇注解，容量常量取值随后列表）：

```rust
#[repr(C)]
pub struct VmBootHandoff {
    pub magic: u32,                      // VM_BOOT_HANDOFF_MAGIC（"VMBH"）
    pub version: u32,                    // == VM_BOOT_HANDOFF_VERSION（= 6）
    pub root_paddr: u64,                 // A1：bootstrap 根物理地址（非零、页对齐）
    pub vm_allocated_bytes: u64,         // C: kinfo.vm_allocated_bytes
    pub kernel_allocated_static: u64,    // C: kinfo.kernel_allocated_bytes
    pub kernel_allocated_dynamic: u64,   // C: kinfo.kernel_allocated_bytes_dynamic
    pub kern_virt_base: u64,             // 内核文本虚拟基址
    pub kern_phys_base: u64,             // 内核文本物理基址
    pub kern_text_pages: u32,            // 内核镜像页数
    pub kern_data_pages: u32,            // 内核数据页数（0 = 整段并 text）
    pub kern_dm_vbase: u64,              // DM 窗口虚拟基址
    pub kern_dm_pages: u64,              // DM 窗口覆盖页数
    pub user_sp: u64,                    // 初始用户栈顶
    pub is_first_time: u32,              // 1 = 冷启动
    pub free_region_count: u32,          // free_regions 有效项数
    pub deducted_count: u32,             // deducted 有效项数
    pub module_count: u32,               // modules 有效项数
    pub free_regions: [HandoffMemRegion; VM_BOOT_HANDOFF_MAX_REGIONS],
    pub deducted: [HandoffMemRegion; VM_BOOT_HANDOFF_MAX_DEDUCTED],
    pub modules: [HandoffModule; VM_BOOT_HANDOFF_MAX_MODULES],
    pub kern_ident_count: u32,           // kern_ident 有效项数
    pub kern_ident: [HandoffMemRegion; VM_BOOT_HANDOFF_MAX_IDENT],
    pub boot_procs: [BootImage; NR_BOOT_PROCS],
}
```

（`os/libs/minix-types/src/types/boot.rs:VmBootHandoff` 字段骨架，签名示意。容量常量实值：`VM_BOOT_HANDOFF_MAX_REGIONS = 64`、`VM_BOOT_HANDOFF_MAX_DEDUCTED = 32`、`VM_BOOT_HANDOFF_MAX_MODULES = 12`、`VM_BOOT_HANDOFF_MAX_IDENT = 64`、`NR_BOOT_PROCS = 17`；magic 常量 `VM_BOOT_HANDOFF_MAGIC = 0x564D_4248`（"VMBH"）。复算命令：`grep -n 'VM_BOOT_HANDOFF_MAX_\|NR_BOOT_PROCS: usize\|VM_BOOT_HANDOFF_MAGIC' os/libs/minix-types/src/types/boot.rs`。）

一页要塞下全部四类信息，就有上限问题。上限的处理原则在容量常量的文档里写着：超出就 fail-fast（内核侧断言区间数装得下），绝不静默截断——截掉一个区间等于无声地缩小 VM 可分配的内存。编译期的一页断言是同一原则的另一半：

```rust
const _: () = assert!(
    core::mem::size_of::<VmBootHandoff>() <= 4096,
    "VmBootHandoff must fit one 4 KiB handoff page"
);
```

（`os/libs/minix-types/src/types/boot.rs` 的一页断言，逐字引用。）

版本史是这个契约最有信息量的部分——每个版本字段对应一次真实事故，逐段读下来就是一部"一页里还缺什么"的发现史：

```rust
/// `VmBootHandoff::version` — handoff layout version.
///
/// Version history:
/// - 1: `root_paddr` only (A1 identity hand-off).
/// - 2: full boot contract — free regions (A2 classification), the
///   LiveBootstrap deduction record, reserved modules, the boot image
///   table and the kernel footprint.
/// - 3: kernel text/data span for per-process kernel mappings (V11/E3).
/// - 4: `user_sp` — the initial user stack top, C `kinfo.user_sp`
///   (pre_init.c:156). VM builds every boot process's initial stack
///   frame downward from it (E-BOOTFRAME; C exec_bootproc reads the
///   same value out of `kernel_boot_info`, main.c:372).
/// - 5: `kern_dm_vbase` / `kern_dm_pages` — the kernel Direct Map
///   window's real VA base and PA coverage (fix26). Version ≤ 4
///   handoffs carried hardcoded sentinels here (wrong base + 4-page
///   window): a process page table built from them left nearly the
///   whole Direct Map not-present, so kernel physical-memory access
///   after a CR3 switch to such a table was broken (the sentinel-era
///   tables are unusable either way — see v6 for what actually killed
///   the first switch). The kernel now computes both from the same
///   candidate union `establish_boot_dm` maps
///   (`dm_coverage::kernel_dm_pa_end`).
/// - 6: `kern_ident` / `kern_ident_count` — the kernel runtime
///   identity windows (fix27). NK4-A forensics (int_fix26f.log)
///   established the real first-switch death cause: the kernel is
///   statically linked into the UEFI boot-shim PE image and EXECUTES
///   from the firmware-relocated low addresses (text ~0x1dac_xxxx,
///   GDT/IDT/TSS ~0x1db9_xxxx after `mov cr3` succeeded, the very
///   next instruction fetch at IP == CR2 hit a not-present page →
///   #PF → #DF → triple fault). The higher-half standalone
///   kernel.elf copy is loaded but never executed, so the v3+
///   kern_* image span alone leaves the live code and tables
///   unmapped in every VM-built page table. v6 ships the occupied
///   (non-conventional) physical regions snapshotted from the UEFI
///   memory map so `map_kernel` replays them identity (VA = PA,
///   supervisor-only) into every process page table.
pub const VM_BOOT_HANDOFF_VERSION: u32 = 6;
```

（`os/libs/minix-types/src/types/boot.rs:VM_BOOT_HANDOFF_VERSION` 的版本史注释与常量定义，逐字引用。内含仓库内部编号，按原文引用。）

版本演进按"事故 → 新字段"整理成表（表中事故描述取自上面的版本史注释原文）：

| 版本 | 新增内容 | 逼出它的事故 |
|---|---|---|
| 1 | root_paddr | A1 身份交接：VM 要认下 bootstrap 根 |
| 2 | 空闲区间、扣减记录、保留模块、boot 表、内核足迹 | A2 资源交接：只有根没法分配 |
| 3 | 内核 text/data 跨度 | VM 内嵌的 mock 布局在真机不成立，每进程内核映射需要真值 |
| 4 | user_sp | 初始栈顶要由内核统一权威给出，boot 进程栈帧才能向下搭 |
| 5 | DM 窗口真实基址与页数 | 版本 ≤4 在这两个字段上带的是硬编码哨兵（错一个 PML4 槽 + 4 页覆盖），切 CR3 后内核物理内存访问全断 |
| 6 | 恒等窗口清单 | 真机取证确认首次切换的死因：内核静态链入 UEFI 镜像、从固件重定位的低地址执行，高半 kernel.elf 装而未执行——只带 kern_* 镜像跨度的表让取指落在 not-present 页，#PF→#DF→三重故障 |

validate 是契约的执行点：`os/libs/minix-types/src/types/boot.rs:fn validate` 对 magic、**版本的精确匹配**、根页非零且页对齐、四个容量计数逐项断言。版本用硬断言而不用"≥ 才有新字段"的软兼容，理由要分两半说。其一，`repr(C)` 的布局没有版本分叉：软兼容下，旧消费者拿新页会按旧偏移读出新字段位置上的垃圾——读偏是静默的，错误以"字段值很怪"的形态 downstream 扩散。其二，`os/libs/minix-types/src/types/boot.rs` 的 `kernel_layout()` 版本门注释写的是现码的实际策略：validate 硬断言精确版本后，"版本不足"在生产不可达，版本不足时的哨兵值只在宿主测试手工构造的交接页上出现——也就是说，硬断言把一切错配推到 validate 一处爆，boot 第一时间停机，而不是靠各字段的回退分支各自接住。硬匹配的代价（任何一侧单独升级即停机，两侧必须同步发版）登记在下面的取舍表里。

取舍表：

| 选什么 | 否掉什么 | 代价 |
|---|---|---|
| 显式 `#[repr(C)]` 一页契约 | BSS 隐藏全局 + 散装取数 | 4096 字节硬上限，加字段要看容量表 |
| 版本精确匹配的硬断言 | 软版本兼容（≥ 判定） | 任何一侧单独升级即停机——两侧必须同步发版 |
| 用户只读映射固定 VA | 消息通道分次传参 | 消费方必须信任映射先行（内核在调度 VM 前装好，`os/kernel/src/lib.rs:init_proc_and_boot` 的映射点） |

还有一个登记要留在字段走查里：`modules` 字段的文档注释写 "every module except VM itself — VM's blob was reclaimed"——"表本身排除 VM"与现码一致，但理由句是旧叙事：现码的扣减记录**包含** VM blob（`os/kernel/src/vm_handoff.rs:classify` 对每个 boot 模块无差别 push，注释原文写明 INCLUDING VM's own），"回收所以不在记录里"讲的是另一张表（§3.6 汇总登记）。注释与现码并存时以现码为准。

### 3.2 A1 采纳与 A2 凭证核验（→§2.1、08 §3.5）

§2.1 的共享页目录给出的是一种"VM 不换根"的机制——CR3 恒定，靠别名页目录够到所有进程的表；§2.4 的取数面给出的是一种"信内核记账"的姿态——账目拿来就用。Rust 对这两件事各给了一个显式形状：采纳（adopt）与核验（reconcile）。

A1 采纳：`os/servers/vm/src/pagetable/vm_self_map.rs:init_vm_self_pt` 把交接页里的 `root_paddr` 直接包成 VM 自己的页表句柄——VM 不建新根，bootstrap 根"本来就是你建的"，采纳即认领。与 C 殊途同归但机制不同：C 靠共享 PDE 别名让 VM 永不换根，Rust 靠"根是内核替你建的、采纳它"达到同一结果；C 的别名机制在 64 位 DM 下没有存在理由（翻译恒定，VM 够别的进程内存走 DM 或内核代拷），随 32 位一起消失。采纳函数的完整性约束写在注释里（其中一条是"只能调一次"）：

```rust
/// Initialize VM's own page table by adopting the bootstrap root.
///
/// `root_paddr` is the address-space identity handed over by the kernel
/// via the boot handoff page (`BootParams::root_paddr`, read from
/// `minix_types::VM_BOOT_HANDOFF_VA` in `main`): VM's initial page table
/// IS the bootstrap root the kernel built and enabled, so VM wraps it
/// instead of creating a fresh one. Must be called once during VmServer
/// init, before any heap allocation that triggers HeapArena::grow().
///
/// # Panics
///
/// Panics if called more than once — re-adoption would create a second
/// authority for the same address space.
pub(crate) fn init_vm_self_pt(root_paddr: PhysBytes) {
    // SAFETY: VM is single-threaded. No concurrent access to VM_SELF_PT_STORAGE.
    let opt = unsafe { &mut *VM_SELF_PT_STORAGE.get() };
    if opt.is_some() {
        panic!("init_vm_self_pt: called more than once");
    }
    *opt = Some(VmSelfPageTable::adopt(root_paddr));
}
```

（`os/servers/vm/src/pagetable/vm_self_map.rs:fn init_vm_self_pt`，逐字引用。）

A2 凭证核验：`os/servers/vm/src/boot.rs:reconcile` 在 PMM 开工前，把内核随页交付的扣减记录当成一份凭证来对账。这是 [08-system-init-boot-finish.md](08-system-init-boot-finish.md) §3.5 预告的"凭证随页交付"的兑现点。四检查各自拦一种内核侧回归：

```rust
fn reconcile(params: &BootParams, deducted: &[HandoffMemRegion]) {
    assert!(
        !deducted.is_empty(),
        "reconcile: empty LiveBootstrap record — kernel did not hand over the deduction list"
    );
    let covered = |addr: u64, len: u64| {
        deducted
            .iter()
            .any(|r| addr >= r.base && addr + len <= r.base + r.size)
    };

    // (2) root page — A1 identity hand-off must be part of the record.
    assert!(
        covered(params.root_paddr.0, PAGE),
        "reconcile: root page 0x{:x} not covered by the deduction record",
        params.root_paddr.0
    );

    // (3) reserved module blobs. Kernel tasks carry no blob (len == 0);
    // VM's own blob was reclaimed after the ELF copy
    // (C: protect.c:450-451) and therefore stays out of the record.
    for ip in params.boot_procs {
        if ip.len == 0 || ip.proc_nr == VM_PROC_NR {
            continue;
        }
        assert!(
            covered(ip.start_addr, ip.len),
            "reconcile: boot module [{:#x},{:#x}) (proc {}) not covered by the deduction record",
            ip.start_addr,
            ip.start_addr + ip.len,
            ip.proc_nr
        );
    }

    // (4) LiveBootstrap ∩ VM-free = ∅ (Proof 2 at the consumption point).
    for fr in params.free_regions {
        let fb = fr.base as u64;
        let fe = fb + fr.size as u64;
        for dr in deducted {
            assert!(
                fe <= dr.base || dr.base + dr.size <= fb,
                "reconcile: free [{fb:#x},{fe:#x}) overlaps deducted [{:#x},{:#x})",
                dr.base,
                dr.base + dr.size
            );
        }
    }
}
```

（`os/servers/vm/src/boot.rs:reconcile`，逐字引用。内含仓库内部编号，按原文引用。）

| 检查 | 拦什么形状的内核 bug | 反向：删掉它故障走到哪 |
|---|---|---|
| 1 记录非空 | 内核侧分类没跑、记录没装页 | 空记录让后面三检查全空转，腐坏直达分配器 |
| 2 根页在记录内 | A1 交付的根页没被扣减——分配器会把 VM 正踩着的根发出去 | VM 自身翻译间歇性腐坏，症状是随机 panic |
| 3 保留模块 blob 在记录内 | 某个服务器镜像的帧没扣——它会被当空闲页发给别的进程，镜像被覆写 | 被覆写服务的取指错乱，且难复现 |
| 4 free ∩ deducted = ∅ | 扣减切割漏切或多切 | "扣多了"或"扣少了"直达分配器第一笔分配 |

检查 3 有一处注释与现码的偏离要在正文里登记：注释写 "VM's own blob was reclaimed … and therefore stays out of the record"——现码的扣减记录**包含** VM blob（`os/kernel/src/vm_handoff.rs:classify` 的模块循环对全部 boot 模块 push，含 VM 自己），代码用 `ip.proc_nr == VM_PROC_NR` 跳过的是**检查本身**而非记录内容。两句话讲清两张表：`modules` 表排除 VM（交接给 VM 的"还有谁占着内存"清单里没有自己——自己那块早被回收），`deducted` 记录包含 VM（bootstrap 期 VM 镜像帧确实被占过，账上要留着）。注释的"reclaimed 所以不在记录里"是把两张表的事揉成了一句（同族登记见 §3.6）。

消费次序也是设计的一部分，而它有一个容易读错的形状：`os/servers/vm/src/main.rs` 的启动链先调 `read_boot_params`（reconcile 在其内）再建 VmServer（采纳 init_vm_self_pt 在 `new_with_boot_params` 内），最后 init 启用分配器——所以**实际执行序是 reconcile → adopt → PMM enabled**（核验先于认领）。内核侧源注释（`os/servers/vm/src/boot.rs` reconcile 的文档注释）写的"adopt → reconcile → PMM enabled"描述的是逻辑依赖——认领的根是核验的对象之一——而非调用次序；无论哪个方向，承重结论不变：分配器的第一笔分配不可能早于核验。若省掉 reconcile，内核侧扣减的任何回归（比如某条 push 漏了）会直达 VM 分配器的第一笔分配，故障从"入口断言"变成"运行期腐坏"，两类故障的可诊断性差一个量级。

### 3.3 VmCtlParam：两段分派并一张 match（→§2.0）

§2.0 的两段分派是 C 里的真实形状：通用臂一个 switch、架构臂一个 switch、无匹配落 printf+EINVAL，15 个子命令的知识分散在两个文件里。Rust 把两段并成一张 match：`os/kernel/src/vm.rs:enum VmCtlParam` 一个枚举收拢全部子命令（含 wire 值映射），`TryFrom` 解析，`os/kernel/src/syscall.rs:dispatch_vmctl` 一张 match 分派——15 个子命令的归属在一张表里看得见。

解析的核心是"未知子命令回答什么"。C 的回答链是：通用 switch 没接住 → 架构 switch 没接住 → `minix3/minix/kernel/arch/i386/arch_do_vmctl.c:arch_do_vmctl` 收尾 `return EINVAL`。Rust 的 `try_from` 对未知值返回 Err，`dispatch_vmctl` 把 Err 映射成同一个 EINVAL——与 C 对齐：

```rust
impl TryFrom<i32> for VmCtlParam {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            12 => Ok(VmCtlParam::ClearPageFault),
            13 => Ok(VmCtlParam::GetPdbr),
            14 => Ok(VmCtlParam::MemReqGet),
            15 => Ok(VmCtlParam::MemReqReply),
            27 => Ok(VmCtlParam::KernPhysMap),
            28 => Ok(VmCtlParam::KernMapReply),
            29 => Ok(VmCtlParam::SetAddrSpace),
            30 => Ok(VmCtlParam::VmInhibitSet),
            31 => Ok(VmCtlParam::VmInhibitClear),
            32 => Ok(VmCtlParam::ClearMapCache),
            33 => Ok(VmCtlParam::BootInhibitClear),
            26 => Ok(VmCtlParam::FlushTlb),
            25 => Ok(VmCtlParam::InvlPg),
            20 => Ok(VmCtlParam::PteRead),
            21 => Ok(VmCtlParam::PteWrite),
            22 => Ok(VmCtlParam::PteZero),
            // VMCTL_NOPAGEZERO (18) and VMCTL_I386_KERNELLIMIT (19) are
            // 32-bit only and unused on 64-bit — return ENOSYS.
            _ => Err(()),
        }
    }
}
```

（`os/kernel/src/vm.rs` 的 `TryFrom<i32> for VmCtlParam`，逐字引用。）

这里有一处注释与行为的偏离要登记：块尾注释写 18/19 "return ENOSYS"，实际 `try_from` 返回 Err，`dispatch_vmctl` 的 Err 臂返回 EINVAL——分派侧的注释（§4.2 引用）把这层演进写明了：上游 C 对"未识别"与"合法未实现"一律 EINVAL，按 EINVAL 对齐。以现码行为为准：18/19 在 Rust 的答案是 EINVAL，与 C 一致。

32 位遗留命令的去向同样是对齐问题而不是裁剪问题。NOPAGEZERO/I386_KERNELLIMIT（18/19）落 Err→EINVAL；KERN_PHYSMAP/KERN_MAP_REPLY（27/28）保留枚举值、臂返回 ENOSYS——保留的理由是语义：C 里这两个调用号是"活着但 64 位无实现"，答案应是 ENOSYS（合法但本实现不提供）而非 EINVAL（参数不认识）——这一区分是 Rust 侧的决定（C 的 vmctl 线对未识别参数一律 EINVAL，没有 ENOSYS 分支）；若为对齐 C 的"精简"而 `#[cfg]` 裁掉这两个变体，调用号 27/28 的答案就从"ENOSYS 拒绝"变成"非法（EINVAL）"（[08-system-init-boot-finish.md](08-system-init-boot-finish.md) §3.1 对同族论证的复用）。

新增的一族没有 C 对位：PteRead/PteWrite/PteZero（wire 20/21/22）。它是 riscv64 上 VM 的页表直读直写旁路——内核经 KDM（内核直映窗口）代读/代写指定物理地址处的 PTE 并刷 TLB。[ARCH: riscv-vmddm] 标注（文档、注释、代码三处一致）。

权限面上有一个与 C 的对照（§2.0 结尾埋的那一处）：C 的通道对 caller 盲，靠特权结构把守；Rust 在 `dispatch_vmctl` 入口补了一道显式权限门 `caller_has_sys_proc_with_table`——形状对照在 §4.2 走查时展开。

### 3.4 SETADDRSPACE 五步的重写（→§2.1）

§2.1 的 setcr3 一次调用要做五件事：双记录、在位特判、VM 特判开分页、清 VMINHIBIT。Rust 的对应函数 `os/kernel/src/syscall.rs:vmctl_set_addr_space` 把这五件事重排成五步，并补了两件 C 没有的。函数文档注释先给出了 C 五步的对照：

```rust
/// SetAddrSpace — switch the target's page table root.
///
/// C: arch_do_vmctl.c:48-50 → setcr3(p, SVMCTL_PTROOT, SVMCTL_PTROOT_V)
///
/// C setcr3 (arch_do_vmctl.c:19-33) does:
///   1. p->p_seg.p_cr3 = cr3
///   2. p->p_seg.p_cr3_v = v
///   3. if (p == ptproc) write_cr3(p->p_seg.p_cr3)
///   4. if (p->p_nr == VM_PROC_NR) arch_enable_paging(p)
///   5. RTS_UNSET(p, RTS_VMINHIBIT)
///
/// Rust implements all 5 steps:
///   - Steps 1-2: data layer (p_seg.phys_root / virt_root).
///   - Step 3: `TlbArch::set_active_root` when target is current ptproc
///     (tracked by the per-CPU `CpuLocal.ptproc` slot (D-40), initialized in
///     `init_post_and_memory`). The arch impls write CR3/TTBR0/satp.
///   - Step 4: no-op on 64-bit (paging enabled at boot via
///     `Paging::enable`).
///   - Step 5: clear RTS_VMINHIBIT.
///
/// # C bug correction
///
/// Minix3 C never sets `vm_running = 1` (only `main.c:47` sets it to 0).
/// Rust corrects this: when the target is `VM_PROC_NR`, set
/// `vm_running = true` so readers (`do_umap_remote`, `acpi`, `oxpcie`)
/// see VM as active. See `09-vm-boot-protocol.md §3 decision4` and
/// `lib.rs::set_vm_running` doc comment.
fn vmctl_set_addr_space(
```

（`os/kernel/src/syscall.rs:fn vmctl_set_addr_space` 文档注释，逐字引用。内含仓库内部编号，按原文引用。）

五步的逐步对照与两处补写：

| C 的行为（§2.1 锚点） | Rust 步 | 差异说明 |
|---|---|---|
| `p_cr3 = cr3; p_cr3_v = v` 双记录 | 步 1：`phys_root`/`virt_root` 双字段 | `as u32 as u64` 位型保真（见下） |
| （C 无此步） | 步 2：kerninfo 页内核代映射 | 对位并取代 §2.3 的 KERN_PHYSMAP 协议 |
| `if (p == ptproc) write_cr3` | 步 3：ptproc 判等 + `set_active_root_tracked` | proc-nr 判等而非指针同一性 |
| `if (p->p_nr == VM_PROC_NR) arch_enable_paging` | 步 4：无对应（64 位分页自 arch_boot 常开） | 消失项 |
| （C 缺口） | 步 4'：`set_vm_running(true)` 当 target==VM | C 从不写 vm_running=1，Rust 补写 |
| `RTS_UNSET(p, RTS_VMINHIBIT)` | 步 5：`rts_unset(VMINHIBIT)` | boot 路唯一清腿；BOOTINHIBIT 仍在故不入队 |

步 1 的位型保真要单独说。消息字段是 `i32`，C 把它赋给 `u32_t` 形参（模运算截断）；Rust 若直接 `as u64` 会做符号扩展——物理根 ≥0x8000_0000 时会变成 0xFFFFFFFF_8xxxxxxx，翻译必错。`as u32 as u64` 先截断再零扩展，保住 C 的位型；这不是多余的转换，是踩过的坑（步 1 注释原文与 §4.3 引用块）。

步 2 是 C 没有的动作：内核在唯一提交点把 kerninfo 页以用户只读映射装进每个新根。它的对位物是 §2.3 的 KERN_PHYSMAP 协议——C 里这段"用户可见内核数据进每个进程空间"由 VM 逐区协商完成；重写把协议收进内核单点。反向推演：若 kerninfo 映射交给 VM 自己做，每个新根都要一次额外的通道往返，且"kerninfo 对所有进程可见"这一契约的执行点从一个函数散开到 VM 的每条建表路径——C 的 KERN_PHYSMAP 协议正是这个散开形状的代价样本。

步 3 的判等用 proc-nr 而非指针同一性：进程槽在 ProcessTable 里一一对应，proc-nr 判等与指针判等等价，而跨数据结构的指针判等在 Rust 的借用模型下反而做不出 C 的形状。`set_active_root_tracked` 是 C 的 `write_cr3` 加一层 CR3 镜像维护——调度器的 `switch_address_space` 现码用活寄存器做比较（`CurrentTlbArch::get_active_root()`），镜像失步不会造成"同一根重装一遍"；镜像只服务诊断读数。

步 4' 的 `set_vm_running(true)` 是对 C 缺口的补写：C 源码里 `vm_running` 只在启动时被写成 0（`minix3/minix/kernel/main.c:bsp_finish_booting`），从不写 1；读它的一方（`minix3/minix/kernel/system/do_umap_remote.c` 一类远程 umap 路径）拿到的永远是"VM 未起"。Rust 在 target==VM 的 SETADDRSPACE 里置位——这正是 [08-system-init-boot-finish.md](08-system-init-boot-finish.md) §3.3 预告的补写点，兑现于此。

步 5 与 C 同形，但 boot 路语境下的意义要再钉一次：`rts_unset` 带入队检查，此刻 BOOTINHIBIT 还在，入队不触发——清腿一清完进程仍不可调度，腿二在 §4.4 的 BOOTINHIBIT_CLEAR。

还有一处有文档的偏离：pdes 别名在 64 位 DM 下没有意义，VM 侧所有 SETADDRSPACE 调用的别名实参都是 0，内核按 0→`virt_root = None` 处理（`os/servers/vm/src/vm_server.rs:exec_bootproc` 的注释与 `os/servers/vm/src/fork.rs`、`os/servers/vm/src/exit.rs` 的同型调用）。VM 生产侧的 SETADDRSPACE 调用方共三处——`grep -rn 'sys_vmctl_set_addrspace' os/servers/vm/src/ | grep -v kernel_gateway | grep -v 'fn sys'` 实跑得 exec_bootproc、fork、exit 各一（测试 stub 另计）——三处全部传 0。

### 3.5 恒等窗口的移交：从 per-space 协议到一次性清单（→§2.3）

§2.3 的 KERN_PHYSMAP/KERN_MAP_REPLY 是一条逐区协商协议：内核逐区申报、VM 逐区映射回报，`.usermapped` 进每个进程空间靠这轮对话。v6 用一张一次性清单取代了整轮协议：内核在构建交接页时算好"哪些物理区间必须以恒等映射（VA=PA）进每张进程表"，连同 DM 窗口参数一起交付；VM 的 map_kernel（`rewrite-notes/02-stage-vm/07-pagetable-struct.md` 的辖区）照单逐区安装。清单的生成器是 `os/kernel/src/vm_handoff.rs:build_identity_windows`，函数文档把两源与变换写全：

```rust
/// Build the kernel runtime identity windows for the handoff (fix27,
/// version 6) — the physical regions `map_kernel` must replay identity
/// (VA = PA, supervisor-only) into every VM-built process page table.
///
/// Sources:
/// - `KernelInfo::reserved_regions()` — the boot-shim's non-conventional
///   memory-map snapshot (running PE image incl. the linked-in kernel,
///   UEFI heap objects, ACPI tables, LOADER_DATA allocations). Empty on
///   platforms without link-in execution (OpenSBI, host tests).
/// - [`minix_arch::kernel_identity_mmio_regions()`] — fixed low-VA MMIO
///   the kernel dereferences at runtime (x86-64: local APIC EOI page).
///
/// Transform: page-align out, drop everything below
/// `USER_IDENTITY_FLOOR`, sort by base, merge overlapping/adjacent
/// regions. Fail-fast on candidate, merged-entry, and total-page caps.
///
/// `USER_IDENTITY_FLOOR` (4 MiB): user-space boot images link their
/// text/data at 2-4 MiB, and NO live kernel object sits below 4 MiB in
/// the UEFI model (the standalone kernel.elf copy at 2-4 MiB is loaded
/// but never executed, and its pages stay marked conventional so they
/// never enter `reserved_regions` anyway). The floor is the defensive
/// half of the identity-window / user-VA non-overlap discipline
/// ([ARCH]: future VM mmap ranges must stay clear of the windows).
```

（`os/kernel/src/vm_handoff.rs:fn build_identity_windows` 文档注释，逐字引用。内含仓库内部编号，按原文引用。）

为什么逐区协商能被一张清单取代？两件事变了。其一，协议要回答的问题只有一个——"哪些物理区间在真实执行中被内核踩着"——这答案内核在 bootstrap 收尾时就能一次性算出（UEFI 内存图的非常规区快照 + 固定低地址 MMIO 两源），不需要 VM 参与发现。其二，64 位下每个进程表的安装动作是同一份（map_kernel 照单恒等映射），没有 32 位借窗口下"每个空间各自映射、各自回报"的多样性。协议的双向性只服务于发现，发现可以前置，清单就是前置的产物。

清单怎么防"把不该恒等映射的东西也装进去"？四道工序（§4.5 逐行走查）：页对齐出界、地板裁剪（`USER_IDENTITY_FLOOR = 0x40_0000`——用户态 boot 镜像链在 2-4 MiB 处，地板之下的条目整条丢、跨地板的裁上半段）、排序合并、双上限（条目数与总页数）。总页数上限 128Ki 页（512 MiB）的意义在常量注释里：occupied 快照吞了巨大东西（固件图碎片化或 MMIO 窗口回归）时 fail-fast，而不是把每个进程表撑爆。容量 256 的候选数组来自一次真机读数——OVMF 的 4 KiB 碎片图比"几十条"的预估忙得多，实测 128 条保留描述符后上限从 128 提到 256（§4.5 引用块里有原文："128 reserved descriptors after the above-RAM filter"）。

清单漏一区与多一区各坏在哪。漏一区：链入执行的代码或表在首次 CR3 切换后落在 not-present 页——v6 版本史记录的三重故障就是清单缺失的实弹。多一区：每个进程表平白多映射一段，512 MiB 上限是爆炸前的闸——闸触发说明快照本身坏了，停下修快照比带着一张巨大的表继续跑正确。

### 3.6 处置表：C 协商面的每件东西去了哪（→Ch2 各节）

第 2 章清点的每件东西都要有去向。15 个子命令逐个对、交接物逐项对、三个机制形状逐个对：

| C 的东西 | C 锚 | 去向 | Rust 形态与锚 |
|---|---|---|---|
| CLEAR_PAGEFAULT (12) | `do_vmctl.c:do_vmctl` | 保留同位 | `VmCtlParam::ClearPageFault`；assert 转 EINVAL；rts_unset 入队半（§4.2） |
| GET_PDBR (13) | `minix3/minix/kernel/arch/i386/arch_do_vmctl.c:arch_do_vmctl` | 保留同位 | `GetPdbr`：字段读（§4.2） |
| MEMREQ_GET (14) | `do_vmctl.c:do_vmctl` | 保留同位 | `MemReqGet`：形状保留，运行时归 24 方向（§2.3） |
| MEMREQ_REPLY (15) | `do_vmctl.c:do_vmctl` | 保留同位 | `MemReqReply`（§4.2） |
| NOPAGEZERO (18) | `com.h` 有定义、无臂 | 保留调用号 | 落 Err→EINVAL，与 C 的 EINVAL 尾一致（§3.3） |
| I386_KERNELLIMIT (19) | 同上 | 保留调用号 | 同上 |
| I386_INVLPG (25) | `minix3/minix/kernel/arch/i386/arch_do_vmctl.c:arch_do_vmctl` | 保留，零调用者 | `InvlPg` 经 TlbArch；VM 侧无调用者是有意设计（§4.2） |
| FLUSHTLB (26) | 同上 | 保留，零调用者 | `FlushTlb` 同上；C 的 7 处消费是别名窗口产物 |
| KERN_PHYSMAP (27) | `do_vmctl.c:do_vmctl` | ENOSYS | 被 kerninfo 代映射取代（§3.4 步 2） |
| KERN_MAP_REPLY (28) | 同上 | ENOSYS | 被 v6 恒等窗口清单取代（§3.5） |
| SETADDRSPACE (29) | `arch_do_vmctl.c:setcr3` | 改型保留 | `vmctl_set_addr_space` 五步（§3.4） |
| VMINHIBIT_SET (30) | `do_vmctl.c:do_vmctl` | 保留同位 | `VmInhibitSet`：SMP IPI 臂 + 无条件 FLUSH_TLB（§4.2） |
| VMINHIBIT_CLEAR (31) | 同上 | 保留（收窄） | `VmInhibitClear`；stale_tlb 全填未实现（DM 下无此问题） |
| CLEARMAPCACHE (32) | 同上 | no-op 返回 OK | 64 位 DM 无缓存表（§4.2） |
| BOOTINHIBIT_CLEAR (33) | 同上 | 保留同位 | `BootInhibitClear`：rts_unset 即入队时刻（§4.4） |
| kernel_boot_info BSS 全局组 | `minix3/minix/servers/vm/glo.h`（`kernel_boot_info`） | 改型 | `VmBootHandoff` 一页契约（§3.1） |
| KERN_PHYSMAP 逐区协议 | `minix3/minix/kernel/arch/i386/memory.c:arch_phys_map` | 淘汰 | v6 `kern_ident` 一次性清单（§3.5） |
| 共享页目录 / pdes 别名 | `pagetable.c:pt_bind` | 淘汰 | DM 下翻译恒定；别名实参传 0→`virt_root=None`（§3.4） |
| p_cr3/p_cr3_v 双记录 | `arch_do_vmctl.c:setcr3` | 改型保留 | `phys_root`/`virt_root` 双字段（§3.4 步 1） |
| arch_enable_paging VM 特判 | 同上 | 淘汰 | 64 位分页自 arch_boot 常开（§3.4 步 4） |
| 模块计费循环（上界 -1） | `main.c:init_vm` | 改型 | `BootParams::extra_pages`；两侧被排除的对象不同（见下） |

表末行要单独展开，因为它和 §2.4 的解码连着：C 的 -1 跳过的是**追加在模块表末尾的内核条目**（内核内存另由 `kernel_allocated_bytes` 补计费），Rust 的 `handoff.modules` 表里本来就没有内核条目（内核足迹走 `kernel_allocated` 字段），于是 -1 上界落在**表内最后一个保留模块**上——C 的 -1 跳内核、Rust 的 -1 跳最后一个模块，两个被排除对象不同。这是移植时循环形状对齐、指称对象错位的如实登记：现码行为以 `os/servers/vm/src/boot.rs:fn extra_pages` 与其测试为准，差异的处置（改上界或在表头补一个占位条目）留给后续轮次。

还有一处注释族登记（§3.1/§3.2 两处偏离的汇总）：扣减记录与模块表"两张表"的旧叙事在注释里出现了多处——`os/servers/vm/src/boot.rs:reconcile` 检查 3 的注释（§3.2 已引原文）、`VmBootHandoff::modules` 字段注释（§3.1 已引）、以及 `os/kernel/src/lib.rs:init_proc_and_boot` 交接页构建段的同族句子。三处讲的都是"VM 的 blob 被回收所以不在账上"，而现码的 `classify` 对 VM 模块无差别计入扣减记录（注释原文 INCLUDING VM's own）。处理原则全篇一致：**表本身的行为以现码为准（modules 表排除 VM、deducted 记录包含 VM），理由句的旧叙事登记不采用**。与注释坐标相关的还有两笔：其一，`os/kernel/src/syscall.rs:fn vmctl_set_addr_space` 的文档注释把 vm_running 补写指向"09-vm-boot-protocol.md §3 decision4"——本篇没有设 decision4 这个编号锚（补写的内容在 §3.4 步 4'），断锚如实登记；其二，被引注释里的几处 C 行号已漂移——`kernel/src/lib.rs` 的 VMINHIBIT|BOOTINHIBIT 置位注释写 :1666（现码在 :2085）、`main.c:372`（user_sp 读取实为 :362）、`main.c:101-108`（实为 :100-103）、`main.c:522-572`（CALLMAP 实到 :575）——引文按原文保留，行号以现码为准。同族未列的坐标另有五处：`arch_do_vmctl.c:56`/:38-40/:52-54、`do_vmctl.c:105-118`、`main.c:428-584`——同按原文保留、以现码为准。恢复实验留一个：若要恢复 C 的逐区协商形状，需要重建 per-space 协议通道与内核侧物理映射登记器——两件东西在现码里都没有存活形态。

---

## 4. 实现详解

### 4.1 交接页的消费序列（→§3.1、§3.2）

页进 VM 之后经几道手才变成可用状态？`os/servers/vm/src/boot.rs:read_boot_params` 是唯一的消费入口，序列是：validate → 二次切割 → 区域/模块转换（Box::leak 一次性）→ BootParams 组装 → validate → reconcile。前半段：

```rust
pub fn read_boot_params() -> BootParams<'static> {
    // SAFETY: the kernel guarantees the handoff page is mapped at
    // VM_BOOT_HANDOFF_VA in VM's initial address space before VM is
    // scheduled; the mapping is user read-only and its contents are fixed
    // for VM's lifetime (one-way, one-shot boot contract). VM runs
    // single-threaded and reads it before any other boot-contract use.
    let handoff =
        unsafe { &*(minix_types::VM_BOOT_HANDOFF_VA as *const minix_types::VmBootHandoff) };
    handoff.validate();

    // A2 free list: kernel-cut survivors, already clipped to VM's DM
    // window (VM PMM eligible = conventional ∩ DM-representable −
    // LiveBootstrap).
    //
    // … 成修：A2 切割漏了 VM 正在运行的**地址空间根页**（以及
    // bootstrap 直接挂在其下的 L1 表簇）——gh46 sas-clear 实证 kernel
    // 持根=0x82000000（handoff root_paddr=boot bump 池基），而该页在
    // free list 内（gh32/45 分配轨迹穿越池区间）；池耗尽降序发放到池
    // 底时根页被当普通帧分配→VM 自身翻译（BSS/堆/DM 窗）间歇腐坏→
    // walk 走设备洞/btree panic/region range 坏全家族。此处按 root 页
    // 起扣 64KB（覆盖 gh35 实测 bootstrap L1 簇 0x82001000..0x8200f000），
    // 泄漏表同步收缩。
    // … 扩界：kernel bump 池整体（gh49 vm pc=0x3a006 取指 fault=
    // VM 自身 text 帧在池内被发放覆写；池=bootface root/pt/名字池/模块
    // 落地/全部 boot 服务器镜像帧的所在，32MiB 全扣）。
    let root_deduct_base = 0x8200_0000;
    let root_deduct_end = 0x8400_0000;
    // … 成修：模块镜像帧同样在 free list 内被回收——VM 自身 text
    // 被其他服务器 ELF 覆写→取指 fault（gh53 vm 0x8 pc=0x39f4a SIGSEGV-
    // for-itself）与全部"腐坏"家族同根。扣减所有 boot 模块区 + root 块。
    let mut deduct: alloc::vec::Vec<(u64, u64)> = alloc::vec![(root_deduct_base, root_deduct_end)];
    for m in handoff.modules[..handoff.module_count as usize].iter() {
        let m_base = m.start_addr & !(0x1000u64 - 1);
        let m_end = (m.start_addr + m.len + 0xfff) & !(0x1000u64 - 1);
        deduct.push((m_base, m_end));
    }
```

（`os/servers/vm/src/boot.rs:fn read_boot_params` 前半，节选——注释中三个 "…" 各代一个仓库内部节号记号，内含其余仓库内部编号（gh 族），按原文引用。）

这一段要先按 §3.1/§3.2 的决策定位：handoff 引用 + validate 是一页契约的消费形态（§3.1）；随后的二次切割与内核侧 A2 扣减的关系要如实说——内核侧分类已把 bootstrap 占用整段切掉（§3.2 的凭证），消费侧这段再切一次 root 块与模块镜像，是对**事故期**内核侧扣减不足的防御：注释里三段取证记录了根页、bump 池、模块镜像先后从空闲表里漏掉的形状。内核侧扣减补齐后，这段切割在现码下是幂等空转（切掉的区间已在扣减记录里、free list 里已没有它们）——它是防御纵深，删它省不了什么、留着要注明现状；处置（删或注）留后续轮次。这就是 §3.6 登记的同一族事实在消费侧的形状。

中段的转换与泄漏：

```rust
    let free_regions: &'static [BootMemRegion] = {
        let v: alloc::vec::Vec<BootMemRegion> = handoff.free_regions
            [..handoff.free_region_count as usize]
            .iter()
            .map(|r| BootMemRegion {
                base: r.base as usize,
                size: r.size as usize,
            })
            .flat_map(move |r| {
                let r_base = r.base as u64;
                let r_end = r_base + r.size as u64;
                let mut parts: alloc::vec::Vec<BootMemRegion> = alloc::vec::Vec::with_capacity(2);
                let mut cur = r_base;
                for (db, de) in deduct.iter() {
                    let (db, de) = (*db, *de);
                    if de <= cur || db >= r_end {
                        continue;
                    }
                    if db > cur {
                        parts.push(BootMemRegion { base: cur as usize, size: (db - cur) as usize });
                    }
                    cur = cur.max(de);
                }
                if cur < r_end {
                    parts.push(BootMemRegion { base: cur as usize, size: (r_end - cur) as usize });
                }
                parts
            })
            .collect();
        alloc::boxed::Box::leak(v.into_boxed_slice())
    };
    let modules: &'static [BootModule] = {
        let v: alloc::vec::Vec<BootModule> =
            handoff.modules[..handoff.module_count as usize]
                .iter()
                .map(|m| BootModule {
                    start_addr: m.start_addr,
                    len: m.len,
                })
                .collect();
        alloc::boxed::Box::leak(v.into_boxed_slice())
    };
```

（`os/servers/vm/src/boot.rs:fn read_boot_params` 中段，逐字引用。）

Box::leak 是一次性的正确形状而非偷懒：boot 数据的生命周期即进程生命周期（C 的 BSS 全局同寿），BootParams 的切片借用标注 `'static`，若换成栈上切片，借用随栈帧消亡，后续所有持有 BootParams 的代码悬垂。

组装与二次 validate：

```rust
    let params = BootParams {
        root_paddr: PhysBytes::new(handoff.root_paddr),
        // Bitmap capacity: highest physical page index across all free
        // regions. On architectures with non-zero RAM base (aarch64 at PA
        // 0x40000000) this exceeds the sum of free pages; the bitmap must
        // span absolute page indices for alloc/free to work correctly.
        total_pages: free_regions
            .iter()
            .map(|r| (r.base + r.size) / CLICK_SIZE)
            .max()
            .unwrap_or(0),
        free_regions,
        boot_procs: &handoff.boot_procs[..],
        modules,
        kernel_allocated: KernelAllocated {
            static_bytes: handoff.kernel_allocated_static,
            dynamic_bytes: handoff.kernel_allocated_dynamic,
        },
        vm_allocated_bytes: handoff.vm_allocated_bytes,
        is_first_time: handoff.is_first_time != 0,
        // V11/E3: version ≥ 3 handoffs carry the kernel layout; version ≤ 2
        // yields None (consumer falls back to the mock constants + warn).
        kernel_layout: handoff.kernel_layout(),
        // E-BOOTFRAME: handoff v4 carries the initial user stack top
        // (C exec_bootproc reads `kernel_boot_info.user_sp`, main.c:372).
        user_sp: VirBytes(handoff.user_sp),
    };
    params.validate();
    reconcile(&params, &handoff.deducted[..handoff.deducted_count as usize]);
    params
}

```

（`os/servers/vm/src/boot.rs:fn read_boot_params` 尾段，逐字引用。内含仓库内部编号，按原文引用。）

BootParams 的两个字段语义要单独钉住。`total_pages` 是**位图容量**不是可用页数——它等于最高物理页索引加一，aarch64 的 RAM 基址非零时位图必须跨过地址空洞，容量口径大于可用页数；口径混用会虚报总量。validate 里的等式检查把这个口径钉死：

```rust
        // Bitmap capacity must equal the highest free-region page index:
        // physical pages are addressed by absolute PA / CLICK_SIZE, and
        // architectures with non-zero RAM base (aarch64 PA 0x40000000+)
        // need the bitmap to span that address space. C achieves this via
        // a fixed-size bitmap covering the entire 32-bit PA space
        // (NUMBER_PHYSICAL_PAGES = 4 GiB / page_size, alloc.c:33).
        let max_page_index: usize = self
            .free_regions
            .iter()
            .map(|r| (r.base + r.size) / CLICK_SIZE)
            .max()
            .unwrap_or(0);
        assert_eq!(
            self.total_pages, max_page_index,
            "BootParams: total_pages ({}) must equal the highest free-region page index ({})",
            self.total_pages, max_page_index
        );
```

（`os/servers/vm/src/boot.rs:fn validate` 的容量段，逐字引用。）

`extra_pages` 是 §2.4 计费循环的对应物：

```rust
    pub fn extra_pages(&self) -> usize {
        let mut pages: u64 = 0;

        // C: main.c:485-489 — the loop bound is `mods_with_kernel - 1`,
        // i.e. the last module entry is deliberately excluded.
        let charged_modules = self.modules.len().saturating_sub(1);
        for m in &self.modules[..charged_modules] {
            let len = round_up_page(m.len);
            pages += len / CLICK_SIZE as u64;
        }

        // C: main.c:492-495 — kernel_allocated_bytes_dynamic is already
        // page-rounded by the kernel; static is rounded up here.
        let kern_static = round_up_page(self.kernel_allocated.static_bytes);
        pages += (self.kernel_allocated.dynamic_bytes + kern_static) / CLICK_SIZE as u64;

        pages as usize
    }
```

（`os/servers/vm/src/boot.rs:fn extra_pages`，逐字引用。）

saturating_sub(1) 就是那条 -1 上界（§3.6 表末行的偏离登记在此处同样生效）。消费入口在 `os/servers/vm/src/main.rs` 的启动链：read_boot_params → new_with_boot_params → init → run。温启动在这里被显式拒绝：

```rust
        // C: main.c:101-108 — if(is_first_time()) { init_vm(); __vm_init_fresh=1; }
        //
        // G-V12-10: a warm start (`is_first_time == false`, C hot-restart
        // semantics after RTS_BOOTINHIBIT clears) cannot be served here — C
        // survives it because its state lives in BSS globals that outlive the
        // restart, while minix-rs state lives inside the VmServer instance.
        // Refuse loudly instead of silently panicking at `run()`'s first
        // `assert!(self.initialized)` (02-stage-vm/todo.md G-V12-10).
        if params.is_first_time {
            server.init();
        } else {
            panic!(
                "VM warm restart (is_first_time = false) is not supported: \
                 minix-rs keeps no cross-restart BSS state (C main.c:101-108 \
                 semantics); see 02-stage-vm/todo.md G-V12-10"
            );
        }

```

（`os/servers/vm/src/main.rs` 启动链的温启动拒绝段，逐字引用。内含仓库内部编号，按原文引用。）

拒绝的理由是状态住在哪：C 的状态在 BSS 全局里，重启天然幸存；minix-rs 的状态住在 VmServer 实例里，热重启没有可复用的载体——响亮拒绝好过在 run 的第一个断言上静默爆掉。VM 服务器 init() 的整体步序与 C init_vm 的逐行对照在 `os/servers/vm/src/vm_server.rs:fn init` 的注释里（§4.4 引用与展开）。

init 的第一段实质动作是 init_global_state，它先钉口径：

```rust
    fn init_global_state(&mut self) {
        // SAFETY: init() must be called exactly once during VM startup.
        // Seed the global total with C's accounting figure (Σ usable pages,
        // `boot_usable_pages`), NOT the bitmap capacity
        // (`page_alloc.total_pages()`). The latter spans address space to
        // cover absolute page indices and would overstate `vsi_total` by
        // every reserved hole below the top free page. `account_boot_memory`
        // then adds the boot-module extras (C `mem_add_total_pages`).
        unsafe {
            crate::global::init(self.boot_usable_pages);
        }
```

（`os/servers/vm/src/vm_server.rs:fn init_global_state` 开头段，逐字引用。）

Σ 可用页与位图容量两个口径在这里分家（§4.1 开头钉过的 total_pages 语义在此消费），总量先种可用页口径，模块补计费再由 account_boot_memory 加上——与 §2.4 的三件账次序同构。

### 4.2 dispatch_vmctl 走查（→§3.3）

§3.3 的单 match 决策落到 `os/kernel/src/syscall.rs:dispatch_vmctl`。走查按执行序：权限门 → SELF 解析 → 端点解析 → try_from → 各臂。权限门是 C 没有的显式检查（§2.0 结尾的对照点），注释里留着一个潜伏 bug 的取证：

```rust
    // Permission check: only system processes may call VMCTL.
    // C: implicit — only VM calls this, and VM always has SYS_PROC.
    //
    // FIX-25 (2026-08-14): use `caller_has_sys_proc_with_table` — the legacy
    // `caller_has_sys_proc` builds a fresh empty `PrivTable::new()` internally
    // and always returns false, rejecting every privileged caller (same
    // latent bug as `dispatch_privctl`/`dispatch_schedule`, now fixed).
    if !crate::syscall_clock::caller_has_sys_proc_with_table(
        proc_table.get(caller_nr).expect("dispatch_vmctl: caller slot must exist"),
        priv_table,
    ) {
        return KcallResult::Ok(EPERM);
    }
```

（`os/kernel/src/syscall.rs:fn dispatch_vmctl` 权限门段，逐字引用。内含仓库内部编号，按原文引用。）

解析段（SELF 解析、endpoint→nr、try_from）在 §3.3 的 try_from 块之后衔接，Err 臂的 EINVAL 对齐注释也在其中（X-7 对齐——C 的未知参数落架构尾 EINVAL，此处逐字引用它的推演）：

```rust
    // Resolve target endpoint. C: do_vmctl.c:22-28
    // SELF means the caller's own endpoint.
    let target_ep = if who_ep == minix_types::Endpoint::SELF.0 {
        proc_table
            .get(caller_nr)
            .map(|c| c.p_endpoint)
            .expect("dispatch_vmctl: caller slot must exist")
            .0
    } else {
        who_ep
    };

    let target_ep = Endpoint(target_ep);
    let target_nr = match proc_table.endpoint_to_nr(target_ep) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // Parse sub-command. C: switch(m_ptr->SVMCTL_PARAM)
    let param = match VmCtlParam::try_from(param_raw) {
        Ok(p) => p,
        Err(()) => {
            // X-7 对齐（P-ALL-05，2026-10-06 定谳）：C 的未知 VMCTL param
            // 落到 switch 收尾的 arch_do_vmctl()，其无匹配臂返回 EINVAL
            // （minix3/minix/kernel/arch/i386/arch_do_vmctl.c:56 收尾
            // `return EINVAL`；本仓 i386 专属 VMCTL 已在 32-bit legacy
            // 臂返回 ENOSYS——那是有真源形状的臂，与本臂不同）。此前
            // 「ENOSYS 区分未识别 vs 合法未实现」的论证不成立：上游对
            // 两者一律 EINVAL。
            return KcallResult::Ok(EINVAL);
        }
    };
```

（`os/kernel/src/syscall.rs:fn dispatch_vmctl` 的参数解析与 Err 臂，逐字引用。内含仓库内部编号，按原文引用。）

各臂的对照按 §2.3 的五族走。故障处置族的 ClearPageFault 是子函数：

```rust
/// ClearPageFault — clear RTS_PAGEFAULT on the target.
///
/// C: do_vmctl.c:32-35 — assert(RTS_ISSET(p,RTS_PAGEFAULT));
/// RTS_UNSET(p, RTS_PAGEFAULT). The C assert converts to EINVAL.
///
/// The clear goes through the scheduler-aware `rts_unset`, which is the
/// RTS_UNSET macro's enqueue half: the process became non-runnable at
/// fault time (RTS_PAGEFAULT, dequeued) and must re-enter its run queue
/// now that VM resolved the fault. The primitive flag clear alone left
/// the process dequeued forever — the fault loop never closed.
fn vmctl_clear_page_fault(
    proc_table: &mut crate::proc_table::ProcessTable,
    target_nr: ProcNr,
) -> KcallResult {
    let was_set = proc_table
        .get(target_nr)
        .is_some_and(|p| p.p_rts_flags.is_set(crate::proc::RtsFlagsBits::PAGEFAULT));
    if !was_set {
        // C: assert(RTS_ISSET(p, RTS_PAGEFAULT)) — convert to error return
        return KcallResult::Ok(EINVAL);
    }
    // TLB 失效半的时点说明（2026-09-22）：INVLPG 只击落当前 CR3 标签的
    // 非 G 项——clear 时点执行在 VM 的 CR3 上下文里，杀不到故障进程自己
    // 的陈旧项。真正的 invlpg 在 finish_and_restore（被恢复进程的 CR3 已
    // 激活、iretq 之前）以其 p_fault_addr 执行；本臂只负责保留
    // p_fault_addr（不再清 None）供恢复点使用。
    proc_table.rts_unset(target_nr, crate::proc::RtsFlagsBits::PAGEFAULT);
    KcallResult::Ok(0)
}
```

（`os/kernel/src/syscall.rs:fn vmctl_clear_page_fault`，逐字引用。）

两个非显然点。其一，C 的 assert 转 EINVAL：C 里对"位本来就没置"的调用直接断言死机，Rust 转成错误返回——契约违规的两种表达，停机与报错，选了不拖累调用方的那个。其二，注释里那段 TLB 失效时点说明：INVLPG 只击落当前 CR3 标签的非全局项，clear 的时点执行在 VM 的 CR3 上下文里，杀不到故障进程自己的陈旧项；真正的单页失效在恢复路径（被恢复进程的 CR3 已激活、返回用户态之前）以它自己的故障地址执行。反向推演：若把 InvlPg 放在本臂（VM 的 CR3 上下文）执行——失效动作打在 VM 的翻译集上，故障进程的陈旧项原封不动，恢复后第一条访问重新页错误，故障环闭合不上。环的另一半（恢复路径的失效点）归 `13-syscall-dispatch`（系统调用分派篇，尚未重生成）。

抑制控制族的两臂与 §2.3 的 C 形状对照：SET 臂的跨 CPU 判定用目标记录的 CPU 字段比较当前 CPU，不同则走 IPI、相同则本地置位，两臂之后**无条件**置 FLUSH_TLB 标记（C 的 set 臂在 if/else 之外置这个标记，注释对位了这一点）；CLEAR 臂的 MF_SENDA_VM_MISS 补投与 stale_tlb 全填未实现——stale_tlb 全填的缺失是 §3.6 表里的"收窄"：64 位 DM 下翻译恒定，全填没有需要服务的对象。

TLB 维护族的三个臂，注释是本节的主块——它把"零调用者是有意设计"讲全了：

```rust
        // ── Arch-specific commands: GetPdbr, FlushTlb, InvlPg ──
        // C: handled by arch_do_vmctl() in arch_do_vmctl.c:38-65
        //
        // FIX-24 (Phase 5): Implemented via `TlbArch` trait (flush_all /
        // flush_addr) + direct field read (GetPdbr). Three architectures
        // covered: x86_64 (CR3/INVLPG), aarch64 (TLBI ALLE1IS/VAAE1IS),
        // riscv64 (SFENCE.VMA).
        //
        // V13-P2-1 (02-stage-vm): the VM server has ZERO callers of
        // FlushTlb/InvlPg — that is by design, not a missing wire. C's VM
        // flushed itself at 4 sites (pagetable.c:119/255/319/430) because it
        // aliased process memory into its own address space; minix-rs's
        // Direct Map keeps translations constant, and `Paging`'s
        // `write_pte_dm` binds an invlpg to every PTE write (08-pagetable-ops
        // …). These commands stay as the kernel-side escape hatch for a
        // world where VM ever needs explicit TLB maintenance again — do not
        // read the absence of consumers as an unwired edge.
        VmCtlParam::GetPdbr => {
            // C: arch_do_vmctl.c:38-40 — rv = p->p_seg.p_cr3
            // Return the target process's page table root physical address.
            // This is a simple field read — no arch operation needed.
            match proc_table.get(target_nr) {
                Some(p) => KcallResult::Data(p.p_seg.phys_root.0 as i32), // data: physical address, not an error code
                None => KcallResult::Ok(EINVAL),
            }
        }
```

（`os/kernel/src/syscall.rs:fn dispatch_vmctl` 的架构命令段（注释与 GetPdbr 臂），节选——注释中一处 "…" 代一个仓库内部节号记号，内含其余仓库内部编号，按原文引用。）

"VM 服务器零调用者"可复核：`grep -rn 'FlushTlb\|flush_tlb\|VMCTL_FLUSHTLB' os/servers/vm/src/` 在生产路径上无调用（实跑验证；C 侧同口径是 §2.3 数过的 7 处）。注释里"C 的 4 处"指页表写路径上的那四处，加上 pt_init 重绑段两处与 utility.c 一处合计七处——注释按它当时的口径列的，总数以 §2.3 的实测为准。这三个命令保留作内核侧逃生口，注释末句把"别把零调用者读成没接线"写成了显式告示。

InvlPg 臂单独看一眼，它有一处注释字段名的偏离要登记：

```rust
        VmCtlParam::InvlPg => {
            // C: arch_do_vmctl.c:52-54 — invlpg(m_ptr->SVMCTL_WHERE)
            // Invalidate the TLB entry for a single virtual address.
            // The virtual address comes from the SVMCTL_WHERE message field
            // (m1_i3, same field as SVMCTL_VALUE for SetAddrSpace).
            let vaddr = VirBytes(value_raw as u64);
            // SAFETY: Called from syscall dispatch context with paging
            // enabled. The virtual address is provided by the caller
            // (VM server) and is expected to be a valid user-space address.
            unsafe { minix_arch::CurrentTlbArch::flush_addr(vaddr); }
            KcallResult::Ok(0)
        }
```

（`os/kernel/src/syscall.rs:fn dispatch_vmctl` 的 InvlPg 臂，逐字引用。）

注释里的 "SVMCTL_WHERE" 在 C 头文件里不存在——`grep -n 'SVMCTL_' minix3/minix/include/minix/com.h` 实跑列出全集：SVMCTL_WHO（m1_i1）、SVMCTL_PARAM（m1_i2）、SVMCTL_VALUE（m1_i3）、SVMCTL_MAP_*（m2 系）、SVMCTL_PTROOT（m1_i3）、SVMCTL_PTROOT_V（m1_p1），没有 WHERE。实际承载该地址的就是 SVMCTL_VALUE（注释自己也写了 "same field as SVMCTL_VALUE"）——字段名是笔误，字段选择是对的，按原文引用、以现码为准。

32 位遗留臂与 Pte 三臂收尾：

```rust
        // ── 32-bit legacy: KernPhysMap, KernMapReply ──
        // C: do_vmctl.c:105-118 — arch_phys_map/arch_phys_map_reply
        // These are 32-bit-only (x86 PAE) and unused on 64-bit.
        VmCtlParam::KernPhysMap | VmCtlParam::KernMapReply => {
            KcallResult::Ok(ENOSYS)
        }
```

（`os/kernel/src/syscall.rs:fn dispatch_vmctl` 的 32 位遗留臂，逐字引用。）

```rust
        // … 内核代读旁路（[ARCH: riscv-vmddm]）：KDM 直读 m1p1 处
        // u64 回填 m1p1。仅 riscv64 生产形态使用（VM 的 VmDm 读钩子）。
        // … 内核代写/零填旁路（[ARCH: riscv-vmddm] 同族）：KDM 直写
        // m1p1 处 u64（值=m1p2）/整页零填。仅 riscv64 生产形态使用。
```

（`os/kernel/src/syscall.rs:fn dispatch_vmctl` 的 Pte 三臂 [ARCH] 注释段，节选——注释中两处 "…" 各代一个仓库内部编号，其余按原文引用；[ARCH: riscv-vmddm] 标注与 §3.3 一致。）

ClearMapCache 的 no-op 也要留个原文（§3.6 的"no-op 返回 OK"行的依据）：

```rust
        // ── ClearMapCache: clear cached mappings ──
        // C: do_vmctl.c:161-164 — mem_clear_mapcache()
        // WONTFIX (FIX-24): mem_clear_mapcache() is a 32-bit-only optimization
        // that flushes the kernel's cached physical-to-virtual mapping table
        // used by the 32-bit Direct Map implementation. On 64-bit minix-rs,
        // the Direct Map covers all physical memory with a static offset
        // (no cache table), so there is nothing to clear. Return OK (0)
        // to indicate success — matching the C behavior on architectures
        // where mem_clear_mapcache() is a no-op.
        VmCtlParam::ClearMapCache => KcallResult::Ok(0),
```

（`os/kernel/src/syscall.rs:fn dispatch_vmctl` 的 ClearMapCache 臂，逐字引用。内含仓库内部编号，按原文引用。）

### 4.3 vmctl_set_addr_space 逐步（→§3.4）

§3.4 的五步落到函数体。步 1 与入口守卫——位型保真、槽缺失转 EINVAL、双记录：

```rust
    // SVMCTL_PTROOT = m1_i3 (same field as SVMCTL_VALUE)
    // SVMCTL_PTROOT_V = m1_p1 (virtual address of page table root)
    //
    // `as u32 as u64` preserves the C bit pattern: C assigns the `int`
    // field to a `u32_t` cr3 parameter (modular conversion, NK4-A
    // fix25 note — the previous plain `as u64` sign-extended physical
    // roots ≥ 0x8000_0000 into 0xFFFFFFFF_8xxxxxxx). Wire width keeps
    // the C i386 constraint: physical roots above 4 GB cannot travel
    // this field (boot-allocated page tables are low; same as C).
    let ptroot_phys = value_raw as u32 as u64; // m1_i3 (i32) → bit-preserving u64
    let ptroot_virt = unsafe { msg.m_u.m_m1.m1p1 }; // m1_p1

    // C: setcr3() is `static void` with no failure path; the slot-missing
    // case (impossible in C — the caller resolved the slot first) maps to
    // EINVAL here, same as before.
    if proc_table.get(target_nr).is_none() {
        return KcallResult::Ok(EINVAL);
    }
    {
        let p = proc_table
            .get_mut(target_nr)
            .expect("slot existence checked above");

        // Steps 1-2: Set page table roots.
        // C: p->p_seg.p_cr3 = cr3; p->p_seg.p_cr3_v = v;
        p.p_seg.phys_root = minix_types::PhysBytes(ptroot_phys);
        p.p_seg.virt_root = if ptroot_virt != 0 {
            Some(minix_types::VirBytes(ptroot_virt))
        } else {
            None
        };
```

（`os/kernel/src/syscall.rs:fn vmctl_set_addr_space` 步 1 段，逐字引用。内含仓库内部编号，按原文引用。）

步 2——kerninfo 页内核代映射，查后装、幂等：

```rust
        // ── NK4-C 1.54（B26 fix B）：把 kerninfo 页的用户只读映射复制进
        // 每个新根。C 把 `.usermapped` 段经 VMCTL_KERN_PHYSMAP 协议映射
        // 进每个进程地址空间（i386 memory.c arch_phys_map + VM per-space
        // map）；本重写由内核在本唯一提交点代做（kerninfo.rs 模块 doc
        // 登记的 mapping-responsibility 转移尚未发生，不新增外部契约）。
        // 先查后装（重绑同一根幂等；子根 virtual_copy 继承过则跳过）。
        // 中间表页经 boot_pt_alloc（其区域已从 VM free list 扣减，
        // … boot_alloc_used_bytes 记账）。
        if let Some(ki_pa) = crate::kerninfo::kerninfo_page_phys() {
            use minix_arch::paging::Paging as _;
            let mut target_paging = minix_arch::CurrentPaging::from_active_root(
                minix_types::PhysBytes(ptroot_phys),
            );
            let needs_map = match target_paging.query(minix_types::VirBytes(
                crate::kerninfo::KERNINFO_USER_VA,
            )) {
                Some((cur, _flags)) => cur.0 != ki_pa,
                None => true,
            };
            if needs_map {
                target_paging
                    .map(
                        minix_types::VirBytes(crate::kerninfo::KERNINFO_USER_VA),
                        minix_types::PhysBytes(ki_pa),
                        minix_arch::paging::PageFlags::read_only(),
                    )
                    .expect("vmctl_set_addr_space: failed to map the kerninfo page into the new root");
            }
        }
```

（`os/kernel/src/syscall.rs:fn vmctl_set_addr_space` 步 2 段，节选——注释中一处 "…" 代一个仓库内部节号记号，内含其余仓库内部编号，按原文引用。）

"查后装"的语义：先 query 目标根上 kerninfo 固定虚拟地址的现映射，已指向同一物理页则跳过——重绑同一根幂等，子根从父根复制过映射的也跳过；只有真缺才 map。中间层页表页经 boot 分配器取（那段注释里写明其区域已在扣减账内）。失败路径只有 map 失败一种，expect 停机——契约提交点不允许带错返回。反向推演：若去掉查询无条件重映射，对已装的根重复 map 要么触发重复映射 panic、要么翻页表重写一遍——写放大换不来任何语义。

步 3 与步 4'——在位特判与 VM 置位：

```rust
        // Step 3: If target is the current ptproc, reload the
        // hardware root register (CR3/TTBR0/satp) so the new
        // page table takes effect immediately.
        // C: if (p == get_cpulocal_var(ptproc)) write_cr3(p->p_seg.p_cr3);
        //
        // The ptproc comparison uses proc-nr (i32) rather than
        // pointer identity. This is equivalent because proc-nrs
        // uniquely identify process slots in the ProcessTable
        // (one-to-one mapping, no aliasing).
        //
        // `set_active_root_tracked` = C's write_cr3 + the Rust
        // CR3-mirror update: the scheduler's
        // `switch_address_space` compares against the mirror
        // (C reads the live CR3), so the mirror must reflect
        // every root change or the first dispatch after this
        // would needlessly reload the same root.
        if crate::current_ptproc_nr() == Some(p.p_nr) {
            crate::set_active_root_tracked(
                minix_types::PhysBytes(ptroot_phys),
            );
        }

        // Step 4: arch_enable_paging — no-op on 64-bit
        // (paging enabled in `arch_boot_impl` via `Paging::enable`).

        // C bug correction: set vm_running = true when target is VM.
        // C source omits this (never writes vm_running=1). Rust
        // corrects the omission so VM is marked as running after
        // it has switched to its own page table.
        if p.p_nr == crate::proc::proc_nr::VM_PROC_NR {
            crate::set_vm_running(true);
        }
```

（`os/kernel/src/syscall.rs:fn vmctl_set_addr_space` 步 3/4/4' 段，逐字引用。）

步 5——清腿一：

```rust

    // Step 5: Clear VMINHIBIT — this is THE boot-path clear leg.
    // C: RTS_UNSET(p, RTS_VMINHIBIT) (arch_do_vmctl.c:32), fired for
    // every pt_bind → VMCTL_SETADDRSPACE (exec_bootproc main.c:355,
    // do_clear exit.c:137, …). The `rts_unset` wrapper carries the
    // ready-queue check (NK4-A fix24/25: a bare clear left boot
    // processes VMINHIBIT-parked even after BOOTINHIBIT was lifted —
    // this arm was the third instance of that shape). For boot
    // processes the enqueue does not fire yet here (BOOTINHIBIT still
    // set); it fires on the later BOOTINHIBIT_CLEAR, in C order.
    proc_table.rts_unset(target_nr, crate::proc::RtsFlagsBits::VMINHIBIT);

    KcallResult::Ok(0)
}
```

（`os/kernel/src/syscall.rs:fn vmctl_set_addr_space` 步 5 段，逐字引用。内含仓库内部编号，按原文引用。）

### 4.4 VM 侧 exec_bootproc 与 init()（→§3.4、§3.2）

VM 侧把协商走完的最后一公里分两段：init() 的启动步序与 exec_bootproc 的单台装载。init 的 C 步序对照表（十一行，C 的 `init_vm` 步序逐行映射到 Rust 的调用点）：

```rust
        // C: init_vm() — main.c:428-584. Step order mirrors C exactly:
        //
        //   main.c:442      sys_getkinfo; main.c:451-452 asserts → BootParams::validate() (new_with_boot_params)
        //   main.c:455      get_mem_chunks; main.c:471 mem_init → VmServer::new (allocator construction)
        //   main.c:457-462  memset(vmproc) + vm_slot    → compile-time vacant slots + get_empty()
        //   main.c:465      acl_init()                  → AclState::Uninitialized (compile-time default)
        //   main.c:468      map_region_init()           → RegionMap::new() lazily per process
        //   main.c:474-475  init_proc(VM_PROC_NR)+pt_init → init_vm_slot() + init_vm_self_pt() (new)
        //   main.c:480      __minix_init()              → mark_initialized() (V10-P0-2; real trap wiring → E1)
        //   main.c:485-495  mem_add_total_pages()       → account_boot_memory()
        //   main.c:497-520  boot procs (exec_bootproc)  → init_boot_procs() + exec_bootproc (V11/T14; stack frame → E-BOOTFRAME)
        //   main.c:522-572  CALLMAP                     → compile-time match (MessageDispatcher)
        //   main.c:577-579  VM instance mark            → mark_vm_instance()
        //
```

（`os/servers/vm/src/vm_server.rs:fn init` 的 C 步序对照注释，逐字引用。内含仓库内部编号，按原文引用。）

对照表之外，init 里还有几个注释故事要如实转述（编号留在注释里，正文只讲事实）：页分配器全局指针按**终址**注册——server 局部变量此后不再移动，注册早了会指向死栈帧（init 注释原文讲了 dangle 的形状）；PageFrames 必须先于 boot 进程循环建立——装载路径消费它，晚建会在第一台装载时 panic，init 的注释记了这个次序；温启动拒绝在 main.rs（§4.1 已引块）；内核布局来自 handoff 的版本门——版本不足三分支在 riscv64 上硬失败（`os/servers/vm/src/vm_server.rs:fn init_global_state` 的回退分支：mock 常量是 x86 形状，Sv39 上非规范，静默套错形状不如停机）。

exec_bootproc 的 SETADDRSPACE 发送段，§3.4 的三调用方之一在此：

```rust
        // C: pt_bind(&vmp->vm_pt, vmp) — main.c:355, whose substance is
        // `sys_vmctl_set_addrspace(endpoint, pt_dir_phys, pdes)`
        // (pagetable.c:1421). The kernel stores the root and clears
        // RTS_VMINHIBIT (C setcr3, arch_do_vmctl.c:19-33) — this is the
        // ONLY boot-path VMINHIBIT clear leg (the kernel sets
        // VMINHIBIT|BOOTINHIBIT on every non-VM boot proc,
        // kernel/src/lib.rs:1666); without it the exec'd processes stay
        // parked even after BOOTINHIBIT lifts (NK4-A fix25 forensics).
        // `pdes` (the kernel-visible PDE alias) has no meaning under the
        // Direct Map — 0 travels as `virt_root = None` (documented
        // deviation, exit.rs `handle_procctl_clear` Step-4 note).
        let ptroot_phys = <crate::pagetable::PageTable as crate::pagetable::Paging>::root_paddr(
            proc.page_table_mut(),
        )
        .0;
        self.ctx
            .gateway
            .borrow_mut()
            .sys_vmctl_set_addrspace(ip.endpoint, ptroot_phys, 0)
            .map_err(|_| "VMCTL_SETADDRSPACE failed")?;
```

（`os/servers/vm/src/vm_server.rs:fn exec_bootproc` 的 SETADDRSPACE 段，逐字引用。内含仓库内部编号，按原文引用。）

pdes=0 的偏离、唯一 boot 路清腿的定位、"不发送则进程永停"的取证形状，注释都写了。镜像读取与 C 的差异也在前几行：C 用 sys_physcopy 把头拷进 VM 缓冲，Rust 经 DM 窗口直读物理镜像——零拷贝，这一条是 DM 红利（[07-cross-space-init.md](07-cross-space-init.md) §1.3 的方向性区别在此兑现）。

段装载的两处关键对位：

```rust
            // NK4-A C-3 迭代5（2026-09-22）：段基页对齐向下取整。RS 的入口
            // 段 vaddr=0x2246c0 非页对齐——旧实现以未对齐段基为 region 零点
            // 铺页，硬件 PTE 却按 4KiB 页界安装，页内内容整体错位
            // (seg.vaddr & 0xfff)——RS 首指令在错位字节上 #GP(0)。
            // C libexec 对位：p_offset ≡ p_vaddr (mod page) 的 ELF 约定下，
            // 对齐页的文件源 = seg.offset - (seg.vaddr & 0xfff)。
            let va_base = seg.vaddr & !(PS as u64 - 1);
            let va_end = seg.vaddr + seg_len;
            let pages = (va_end - va_base).div_ceil(PS as u64) as usize;
            // NK4-C … boot-OOM 根因修复：region 覆盖整段 memsz（含
            // heap/BSS 预留尾，读 elf 末段 FileSiz=64K / MemSiz=4M），但只有
            // 文件页需要 eager 实体化；memsz 与 filesz 之间的页对齐尾部（未
            // 初始化 .bss + 堆）留给按需缺页（对位 C libexec：exec_image 只对
            // 文件页 physcopy，其余 demand-fault）。旧实现对 0..pages 全量
            // alloc_pfn + map_page，12 台 boot server 各 eager 铺 ~4MB → 12×
            // 1041 页撑爆 14319 帧池于第八台（mib）报 "boot segment page
            // allocation failed"（exec 只到 7）。首/末页页内 .bss 空洞由下方
            // write_bytes 显式清零（对位 C clearmem 头/尾）；页对齐整页尾的零
            // 填依赖 ANON demand-fault 给零页——boot 期无页缓存可回收、
            // alloc_pfn_reclaiming 退化为全新帧，故行为不变；把 demand-fault
            // 显式接 PAF_CLEAR（对位 C VR_UNINITIALIZED）列为后续加固项。
            let file_pages = if seg.filesz == 0 {
                0
            } else {
                ((seg.vaddr + seg.filesz - va_base) as usize)
                    .div_ceil(PS)
                    // SHOULD（CodeReview）：minix-elf SegmentIter 不校验
                    // filesz<=memsz；畸形末段会让 file_pages 超出 region
                    // 长度 pages，map_page 裸下标 index-OOB panic（VM 起不来
                    // = 全系统停摆）。钳到 pages 保持旧「静默丢多余文件字节」
                    // 的容错语义。
                    .min(pages)
            };
```

（`os/servers/vm/src/vm_server.rs:fn exec_bootproc` 的段装载关键段，节选——注释中两处替换（一处 "…" 代仓库内部节号记号、一处 "第八" 原文作数字形式），内含其余仓库内部编号，按原文引用。）

第一处：段基页对齐向下取整。ELF 入口段虚拟地址不必页对齐，装页却必须按页界装——以未对齐段基为区域零点铺页，页内内容整体错位段地址的低 12 位，服务首指令在错位字节上取指——注释里记着那次 #GP。对齐页的文件源偏移按 ELF 约式回推：文件偏移与虚拟地址模页同余。第二处：只 eager 实体化文件页。区域覆盖整段 memsz（含 BSS 与堆预留尾），文件页才分配物理帧，页对齐尾留给按需缺页；首末页的页内 .bss 空洞显式清零。eager 全量的旧形状有实测数字：12 台 boot 服务器各 eager 铺约 4 MB，帧池在第八台耗尽（注释原文的 12×1041 页与 14319 帧池两个读数）。可写位与可执行位从 ELF 段旗标承接（PF_W/PF_X），注释里两侧的真机取证（不可写数据段的首写页错误、present-but-NX 的取指活锁）把"旗标不能默认"钉死。

sys_exec 与腿二收尾：

```rust
        // C: main.c:414-416 — "make it runnable": the boot process was
        // forked with VMINHIBIT|BOOTINHIBIT (kernel/src/lib.rs:1666) and
        // stays parked after exec unless VM clears the inhibit. Without
        // this leg every `exec X ok` booted a permanently stopped
        // process (NK4-A fix23b forensics 2026-09-21: 11 boot procs
        // exec'd, none ever reached `_start`).
        gateway
            .sys_vmctl_boot_inhibit_clear(endpoint)
            .map_err(|_| "VMCTL_BOOTINHIBIT_CLEAR failed")?;
```

（`os/servers/vm/src/vm_server.rs:fn install_boot_stack` 尾段（sys_exec 与 BOOTINHIBIT_CLEAR），逐字引用。内含仓库内部编号，按原文引用；注释里 `kernel/src/lib.rs:1666` 的行号已漂移——VMINHIBIT|BOOTINHIBIT 的置位语句现码在 lib.rs 的 :2085 附近，以现码为准。）

BOOTINHIBIT_CLEAR 是入队时刻——§1.2 的腿二在 VM 侧的发出点。此前的 sys_exec 已携带完整现场：初始栈帧由 `os/servers/vm/src/vm_server.rs:fn install_boot_stack` 构造（`minix_sys::stack::stack_fill` 在一页帧内摆好 argc/argv/envp 与 ps_strings，得出虚拟栈顶），sys_exec 以真实 vsp 与 ps_str 调用（E-BOOTFRAME 边已落地——函数文档注释写明"then sys_exec with the **real** stack/ps_str values"）。清腿之后 rts 逻辑才把进程放进就绪队列；没有这一腿的取证形状注释里写着：11 台 exec 全部报成，无一到达用户入口。

### 4.5 identity_windows 与 kern_ident 消费（→§3.5）

§3.5 的清单生成器全走查。生成段（两源 push 闭包、页对齐、地板裁剪、计数器）：

```rust
pub(crate) fn build_identity_windows(
    kernel_info: &KernelInfo,
) -> ([HandoffMemRegion; VM_BOOT_HANDOFF_MAX_IDENT], usize) {
    const USER_IDENTITY_FLOOR: u64 = 0x40_0000;
    /// Candidate slots: UEFI non-conventional descriptors are a few
    /// dozen on QEMU/OVMF; overflow is a firmware-shape surprise, not
    /// something to truncate silently. fix27d: real machine measured
    /// 128 reserved descriptors after the above-RAM filter (the 4 KiB
    /// fragmented OVMF map is far busier than the "few dozen" the
    /// first cut assumed) — 256 keeps the fail-fast meaningful instead
    /// of firing on the normal shape.
    /// F6：单一常量双引用——与 globals::RESERVED_REGION_STORE_LEN 同源，
    /// 编译期消除两处 256 手工对齐的漂移面。
    const MAX_CANDIDATES: usize = crate::globals::RESERVED_REGION_STORE_LEN;
    /// Sanity cap on 4 KiB identity leaves: 128 Ki pages = 512 MiB.
    /// Blowing this means the "occupied" snapshot swallowed something
    /// enormous (fragmented firmware map or an MMIO-window regression) —
    /// mapping it per-process would exhaust the boot bump pool.
    const MAX_TOTAL_PAGES: u64 = 128 * 1024;

    let mut cand = [(0u64, 0u64); MAX_CANDIDATES];
    let mut n = 0usize;
    // fix27 forensic (2026-09-21): real machine showed res=127 arriving
    // with cand=2 — every reserved entry rejected. Count WHY to split
    // "payload zeroed (dangling slice)" from "all below the floor".
    let mut zeroed = 0usize;
    let mut below_floor = 0usize;
    let mut max_end = 0u64;
    let mut push = |base: u64, size: u64, n: &mut usize| {
        assert!(*n < MAX_CANDIDATES, "vm_handoff: identity candidate overflow");
        if base + size > max_end {
            max_end = base + size;
        }
        if size == 0 {
            zeroed += 1;
            return;
        }
        // align out: floor base, ceil end.
        let b = base & !(PAGE_SIZE - 1);
        let e = (base + size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        // clip below the user-image floor.
        if e <= USER_IDENTITY_FLOOR {
            below_floor += 1;
            return;
        }
        let b = b.max(USER_IDENTITY_FLOOR);
        cand[*n] = (b, e - b);
        *n += 1;
    };
    for r in kernel_info.reserved_regions() {
        push(r.base.0, r.len as u64, &mut n);
    }
    for &(base, size) in minix_arch::kernel_identity_mmio_regions() {
        push(base, size, &mut n);
    }
    // 结束闭包对计数器的可变借用，之后才能再读它们。
    drop(push);
    let res_rejected = zeroed + below_floor;
    let _ = res_rejected;
```

（`os/kernel/src/vm_handoff.rs:fn build_identity_windows` 生成段，逐字引用。内含仓库内部编号，按原文引用。）

排序合并段：

```rust
    // insertion sort by base — n ≤ MAX_CANDIDATES, boot-path sizes only.
    for i in 1..n {
        let mut j = i;
        while j > 0 && cand[j - 1].0 > cand[j].0 {
            cand.swap(j - 1, j);
            j -= 1;
        }
    }

    // merge overlapping/adjacent runs; fail fast on a produced-entry or
    // page-total cap breach (never truncate — a cut window would put
    // the #PF→#DF→triple-fault landmine back).
    let mut ident = [HandoffMemRegion::ZERO; VM_BOOT_HANDOFF_MAX_IDENT];
    let mut count = 0usize;
    let mut total_pages = 0u64;
    let mut i = 0usize;
    while i < n {
        let (b0, s0) = cand[i];
        let mut end = b0 + s0;
        let mut j = i + 1;
        while j < n && cand[j].0 <= end {
            end = end.max(cand[j].0 + cand[j].1);
            j += 1;
        }
        assert!(count < VM_BOOT_HANDOFF_MAX_IDENT, "vm_handoff: identity window overflow");
        total_pages += (end - b0) / PAGE_SIZE;
        assert!(
            total_pages <= MAX_TOTAL_PAGES,
            "vm_handoff: identity windows exceed the 4KiB-leaf sanity cap"
        );
        ident[count] = HandoffMemRegion {
            base: b0,
            size: end - b0,
        };
        count += 1;
        i = j;
    }
```

（`os/kernel/src/vm_handoff.rs:fn build_identity_windows` 排序合并段，逐字引用。）

生成管线五步手排一个样例（用 §3.5 的两源形状）：设保留区快照给 `{[0x1da0_0000, 0x1da3_0000), [0x1da2_8000, 0x1da4_0000), [0x1000, 0x2000)}`，MMIO 源给 `{(0xFEE0_0000, 4096)}`。第一步页对齐：三段本已页对齐（若不是，基址向下取整、终点向上取整）。第二步地板裁剪：`[0x1000,0x2000)` 整段在 0x40_0000 之下，整条丢；其余保留。第三步排序：`0x1da0_0000`、`0x1da2_8000`、`0xFEE0_0000` 升序。第四步合并：前两段相邻重叠（第二段基址 0x1da2_8000 ≤ 第一段终点 0x1da3_0000），并为 `[0x1da0_0000, 0x1da4_0000)`；MMIO 段不相邻，独立成条——两条进清单。第五步上限：合计 64 页 + 1 页，远小于 128Ki 页闸。这份清单随后随交接页交付，VM 侧 map_kernel 照单把每区以 VA=PA、内核可执行可写（`kernel_executable_writable`：PRESENT|WRITABLE|EXECUTABLE|GLOBAL）装进每张进程表（消费在 `rewrite-notes/02-stage-vm/07-pagetable-struct.md`）。

计数器（zeroed/below_floor）是分诊**意图**的载体：真机读到 128 条保留描述符进来、候选只剩 2 条时，两个计数器本应回答"是载荷被清零（悬垂切片）还是全在地板下"——但现码在求和后即丢弃（`let _ = res_rejected;`，`vm_handoff.rs` 里两处同形），**分诊目前没有消费者**；失败模式只能靠候选清单读数与调用方的错误路径观察（与 §5.3 的诚实披露同族，源注释的"分诊"措辞待回修）。若清单漏区或乱序安装：漏区的后果是 §3.5 的 not-present 三重故障；乱序直接装叶会让重复安装撞 AlreadyMapped 拒启（[07-cross-space-init.md](07-cross-space-init.md) §4.4 的安装次序与验证同族）。

---

## 5. 测试要点

### 5.1 清单与读数（→Ch4）

测试面的读数分口径报：grep 定义数与 cargo 收集数分开。VM 服务器 boot.rs 的 12 条（复算命令：`grep -c 'fn test_' os/servers/vm/src/boot.rs`，实跑得 12）分三组：

| 组 | 条数 | 测试名 | 钉住的点 |
|---|---|---|---|
| validate | 5 | `test_boot_params_simple_validates` / `test_boot_params_validate_empty_regions` / `test_boot_params_validate_total_pages_mismatch` / `test_boot_params_validate_misaligned_root` / `test_boot_params_validate_zero_root` | 合法样例过；空区间、容量口径错位、根页未对齐、根页为零各自拒绝（§4.1 的口径与 A1 契约） |
| extra_pages | 2 | `test_boot_params_extra_pages_modules` / `test_boot_params_extra_pages_kernel` | -1 上界（最后模块不计）、静态取整动态直加（§2.4 对位） |
| reconcile | 5 | `test_reconcile_accepts_consistent_record` / `test_reconcile_rejects_empty_record` / `test_reconcile_rejects_root_not_in_record` / `test_reconcile_rejects_free_deducted_overlap` / `test_reconcile_rejects_module_not_in_record` | §3.2 四检查各一条拒绝样例加一条接受样例 |

minix-types 侧 11 条（复算命令：`grep -c '#\[test\]' os/libs/minix-types/src/types/boot.rs`，实跑得 11）：BootImage 四条（empty/from_wire/name/name_truncated）、KernelLayout 基础三条（new/eq/copy）、`kernel_layout_tests` 四条（v3 报跨度、v5 报 DM 窗口、v6 报恒等窗口、v2 返回 None）——最后四条钉住 §3.1 版本门的分支。内核侧 vmctl 相关：`os/kernel/src/vm.rs` 里 VmCtlParam 两条（`test_vmctl_param_from_u32`/`test_vmctl_result_variants`），`os/kernel/src/vm_handoff.rs` 七条（含 `test_identity_windows_clip_sort_merge`/`test_identity_windows_empty_reserved_still_has_mmio`，复算命令：`grep -c '#\[test\]' os/kernel/src/vm_handoff.rs`，实跑得 7），`os/kernel/src/syscall.rs` 里 vmctl 三条（`test_vmctl_vminhibit_local_arm_sets_both_flags`/`test_vmctl_vminhibit_park_helper_does_not_touch_flush_tlb`/`test_vmctl_clear_page_fault_requeues_target`，复算命令：`grep -c 'fn test_vmctl' os/kernel/src/syscall.rs`，实跑得 3）。

### 5.2 关键测试细读：reconcile 的四条拒绝测试（→§3.2）

四条拒绝测试逐条对着 §3.2 的四检查，一块读完：

```rust
    #[test]
    #[should_panic(expected = "empty LiveBootstrap record")]
    fn test_reconcile_rejects_empty_record() {
        let regions = [region(0x100000, 4)];
        let params = BootParams::simple(4, &regions);
        reconcile(&params, &[]);
    }

    #[test]
    #[should_panic(expected = "root page")]
    fn test_reconcile_rejects_root_not_in_record() {
        let regions = [region(0x100000, 4)];
        let params = BootParams::simple(4, &regions);
        // Record covers some other page, not the root.
        let record = vec![HandoffMemRegion {
            base: 0x500000,
            size: 0x1000,
        }];
        reconcile(&params, &record);
    }

    #[test]
    #[should_panic(expected = "overlaps deducted")]
    fn test_reconcile_rejects_free_deducted_overlap() {
        let regions = [region(0x100000, 4)];
        let params = BootParams::simple(4, &regions);
        // Record covers the root AND claims part of the free region.
        let record = vec![
            HandoffMemRegion {
                base: params.root_paddr.0,
                size: 0x1000,
            },
            HandoffMemRegion {
                base: 0x100000,
                size: 0x2000,
            },
        ];
        reconcile(&params, &record);
    }

    #[test]
    #[should_panic(expected = "not covered by the deduction record")]
    fn test_reconcile_rejects_module_not_in_record() {
        // A boot image with a blob (RS) whose physical range the record
        // does not cover.
        let regions = [region(0x100000, 4)];
        let params = BootParams::simple(4, &regions);
        let mut record = root_record(params.root_paddr);
        record.push(HandoffMemRegion {
            base: 0x300000,
            size: 0x1000,
        });
        // params.boot_procs is empty in simple(); exercise the module
        // check through a boot-image slice on a locally built params.
        let boot_procs = [BootImage {
            proc_nr: 2, // RS_PROC_NR — not VM
            proc_name: [0; 16],
            endpoint: Endpoint(2),
            start_addr: 0x700000,
            len: 0x1000,
        }];
        let params = BootParams {
            boot_procs: &boot_procs,
            ..params
        };
        reconcile(&params, &record);
    }
```

（`os/servers/vm/src/boot.rs` 的 reconcile 拒绝测试组，逐字引用。）

逐条看改错哪行会红。`test_reconcile_rejects_empty_record` 传空记录，命中检查 1 的断言串 "empty LiveBootstrap record"；把检查 1 删掉，这条红。`test_reconcile_rejects_root_not_in_record` 让记录盖住别的页而不是根页，命中 "root page ... not covered"；删检查 2 它红。`test_reconcile_rejects_free_deducted_overlap` 让记录同时盖住根页和空闲区间的一段，命中 "overlaps deducted"；删检查 4 它红——这就是"扣多了"回归的拦截点，删掉后此类回归直达分配器。`test_reconcile_rejects_module_not_in_record` 用 proc_nr=2（RS）构造一条带 blob 的 boot 镜像、记录不盖它，命中 "not covered by the deduction record"；删检查 3 它红。这条测试还有一层含义：构造里 VM 被显式跳过——**跳过本身有测试含义**（检查 3 对 VM 的跳过是设计而非遗漏），若有人把跳过条件改掉，VM 自身条目会进来被核，与记录的现状（含 VM）反而一致、测试不红——所以条件改动还要靠 §5.3 的边界披露盯着。

### 5.3 测试边界与诚实披露（→Ch3/Ch4）

没有测试的面，逐项给反证命令：SETADDRSPACE 五步的端到端（`grep -n 'fn test_.*addr_space' os/kernel/src/syscall.rs` 无命中，实跑验证）——宿主测试跑不了真 CR3 语义，五步的正确性靠 §4.3 的逐行对位与真机门；exec_bootproc 全序（`grep -rn 'fn test.*exec_boot' os/servers/vm/src/` 无命中）——真装载走真机；温启动拒绝路径——拒绝分支在 `os/servers/vm/src/main.rs` 的 cfg(not(test)) 块里宿主不可达（`grep -n 'is_first_time' os/servers/vm/src/boot.rs` 的命中只是字段初始化与测试构造，没有温启动路径测试）；kerninfo 代映射的幂等分支——依赖真页表（`grep -n 'kerninfo' os/kernel/src/syscall.rs` 命中的三条测试都是 IPC 入口臂的，不覆盖"查后装"分支）。真机/宿主分界：上述四项全在真机门覆盖（`os/qemu-tests/README.md` 的三架构回归），宿主单测只保住纯逻辑面（validate/reconcile/extra_pages/try_from/identity_windows 变换）。

### 5.4 机制 → 需要证明什么（反转核对表）

（同系列先例：从**机制**出发列"要证明它成立需要什么断言"；"测试覆盖状态"与 §1.5 的"C 侧边界"是两个维度，读时按列名区分。）

| # | 机制（正文位置） | 需要证明什么（可观测断言） | 测试覆盖状态 | 现有证据 / TODO |
|---|---|---|---|---|
| M1 | BootParams validate + reconcile 四检查（§3.2、§4.1） | 合法过；四类拒绝各一条 | 已覆盖 | `boot.rs` 12 条（validate 5 + extra_pages 2 + reconcile 5） |
| M2 | 版本门 v2/v3/v5/v6（§3.1） | 四版本分支各一断言 | 已覆盖 | minix-types `kernel_layout_tests` 四条 |
| M3 | identity_windows 变换（§3.5） | 裁剪/排序/合并；空保留区仍有 MMIO | 已覆盖 | `vm_handoff.rs` 七条（含 clip_sort_merge、empty_reserved_still_has_mmio） |
| M4 | VmCtlParam 解析与结果变体（§3.3） | wire 值往返；结果变体互异 | 已覆盖 | `vm.rs` 两条 |
| M5 | VMINHIBIT 置位与 park-helper（§4.2） | 本地臂置两旗标；park-helper 不触 FLUSH_TLB；ClearPageFault 重入队 | 已覆盖 | `syscall.rs` 三条 |
| M6 | SETADDRSPACE 五步端到端（§4.3） | 五步在真 CR3 下的时序 | 未覆盖（宿主） | §5.3 已披露（真机门承担）→ **R09-T01** |
| M7 | exec_bootproc 全序与温启动拒绝（§4.4） | 装载→放行全序；重启路径拒绝 | 未覆盖（宿主） | §5.3 已披露 → **R09-T02** |
| M8 | kerninfo 代映射幂等分支（§4.2） | "查后装"分支 | 未覆盖（宿主） | §5.3 已披露 → **R09-T03** |
| M9 | 交接页自身的 validate（§3.1、§3.2） | 页内校验器自身的负向样例 | 未覆盖（未披露） | 本轮新增披露 → **R09-T04** |
| M10 | boot-OOM 帧池读数（§4.4） | 12×1041 与 14319 的闭合口径 | ⑤证据不足（读数口径未闭合） | 登记 → **R09-T05** |
| M11 | res_rejected 分诊（§4.5） | 计数器有消费者 | 未覆盖（现码丢弃，`let _ = res_rejected;`） | 文档已按现码改写；源侧 → **SRC-39** |
| M12 | deduct 单遍未排序减法（§4.5） | 防御场景下不漏减模块区 | 未覆盖（低置信） | → **SRC-38**（源码侧核实） |

（§5.3 的披露项分别对到 M6/M7/M8；M9/M11/M12 为本轮新增披露与转出；两类表互为反转面。）

**新 TODO（对源码/测试 workflow 的需求输入）**：

| ID | 内容 | 分流 |
|---|---|---|
| R09-T01 | SETADDRSPACE 五步的端到端载体（真 CR3 语义） | **真机/测试侧** |
| R09-T02 | exec_bootproc 全序与温启动拒绝的真机载体 | **真机/测试侧** |
| R09-T03 | kerninfo 代映射"查后装"分支的测试 | **测试侧** |
| R09-T04 | 交接页自身 validate 的负向样例 | **测试侧** |
| R09-T05 | boot-OOM 帧池读数口径（12×1041 vs 14319）登记 | **登记型**（待真机日志口径） |

---

## 6. 诊断

### 6.1 症状 → 怀疑对象 → 确认命令

| 症状 | 怀疑对象 | 确认命令与预期关键行 |
|---|---|---|
| VM 起不来，串口停在交接页读取前后 | 交接页 validate：版本错配、magic 错、根页为零/未对齐 | 看串口最后的 boot_stage 路标（`grep -n 'boot_stage!' os/kernel/src/lib.rs` 列出全部路标，如 "kernel: kmain A.1 validate ok"）定位内核走到的步；VM 侧 panic 消息带字段名（"VmBootHandoff: unsupported version {}" / "bad magic" / "root_paddr"），对照 §3.1 的 validate 逐项 |
| VM 起来了但 reconcile panic | 四检查各对应一种内核侧回归：空记录=分类没交付；根页不在=扣减漏根；模块不在=漏扣镜像；重叠=切割错 | panic 串前缀 "reconcile:" 加检查名（§3.2 引用块的四条断言串即关键字）；回查 `os/kernel/src/vm_handoff.rs:classify` 的对应 push |
| boot 进程 exec 完不跑 | 两道闸的腿：SETADDRSPACE 没发（VMINHIBIT 按着）或 BOOTINHIBIT_CLEAR 没发（BOOTINHIBIT 按着） | 串口搜各 boot 服务器名与 `boot_stage` 路标（VM 的每台 exec 路标已在源中滚除）；内核侧调度计数确认无一入队即腿二缺失（§4.4 引用块里的记录：exec 全部报成、无一到入口） |
| 某服务首指令取指错位/#GP | 段基未按页对齐取整（§4.4 第一处对位）或可执行位没从 PF_X 承接 | 核对入口段 vaddr 低 12 位与装载区域基址；对照 `os/servers/vm/src/vm_server.rs:fn exec_bootproc` 的对齐与旗标两段 |

症状一的判别特征：串口最后出现的 boot_stage 路标给出内核侧死点，VM 侧 panic 消息里的字段名给出契约校验的失败项——两边一对照，错配方向（内核没写/VM 读错位）即出。

### 6.2 手算与常量核对命令

15 个子命令与调用号（§2.0 全景表的对账）：

```bash
grep -n 'SYS_VMCTL' minix3/minix/include/minix/com.h     # KERNEL_CALL + 43
grep -c 'define VMCTL_' minix3/minix/include/minix/com.h # 15
grep -rn 'VMCTL_NOPAGEZERO\|VMCTL_I386_KERNELLIMIT' minix3/minix/ --include=*.c
# 预期：两个常量在 handler 侧零命中（落 EINVAL 尾的遗留值）
```

交接页版本与大小：

```bash
grep -n 'VM_BOOT_HANDOFF_VERSION: u32\|VM_BOOT_HANDOFF_VA: u64\|size_of::<VmBootHandoff>() <= 4096' \
  os/libs/minix-types/src/types/boot.rs
# 预期：version = 6；VA = 0x1_0000_0000；一页断言在场
```

SETADDRSPACE 调用方清单（§3.4 的三处与 C 的唯一处）：

```bash
grep -rn 'set_addrspace\|SETADDRSPACE' minix3/minix/servers/vm/ minix3/minix/lib/libsys/sys_vmctl.c
# 预期：仅 pagetable.c 的 pt_bind 尾一处调用 + libsys wrapper
grep -rn 'sys_vmctl_set_addrspace' os/servers/vm/src/ | grep -v kernel_gateway | grep -v 'fn sys'
# 预期：vm_server.rs（exec_bootproc）、fork.rs、exit.rs 各一处
```

五步锚点定位（§1.2 时序图的代码落点）：

```bash
grep -n 'sys_vmctl_set_addrspace' minix3/minix/servers/vm/pagetable.c
grep -n 'VMCTL_BOOTINHIBIT_CLEAR' minix3/minix/servers/vm/main.c
grep -n 'arch_enable_paging\|RTS_VMINHIBIT' minix3/minix/kernel/arch/i386/arch_do_vmctl.c
# 预期：三条分别落在 pt_bind 尾、exec_bootproc 收尾、setcr3 三分支
```

---

## 7. 参见

- [08-system-init-boot-finish.md](08-system-init-boot-finish.md)（三开关与收尾；§3.3/§3.5 预告的兑现处）、[07-cross-space-init.md](07-cross-space-init.md) §2.1/§2.2/§3.2（bootstrap 根的记录、memory_init 拨槽与 DM 双窗口）、[06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) §1.4/§2.6（inhibit 置位与 boot 循环）、[01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md)（保留区与链入执行的来历）
- `10-switch-to-user`（VM 如何被首次调度，尚未重生成）、`13-syscall-dispatch`（分派入口与 finish/resume，尚未重生成）、`24-cross-space-runtime`（VMSUSPEND 运行时，尚未重生成）、`rewrite-notes/02-stage-vm/`（init_page_table/map_kernel/PMM 本体）
- C 源码：`minix3/minix/kernel/system/do_vmctl.c:do_vmctl`、`minix3/minix/kernel/arch/i386/arch_do_vmctl.c:setcr3`、`minix3/minix/servers/vm/main.c:exec_bootproc`、`minix3/minix/servers/vm/main.c:init_vm`、`minix3/minix/servers/vm/pagetable.c:pt_bind`
- Rust：`os/libs/minix-types/src/types/boot.rs`（`VmBootHandoff`）、`os/kernel/src/syscall.rs`（`dispatch_vmctl`/`vmctl_set_addr_space`）、`os/kernel/src/vm_handoff.rs`（`build_identity_windows`）、`os/servers/vm/src/boot.rs`、`os/servers/vm/src/vm_server.rs`、`os/servers/vm/src/pagetable/vm_self_map.rs`
