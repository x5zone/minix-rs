1. """
这是一个 P1 结构性问题 （不是 P0，因为内容本身没有事实错误，但叙事逻辑与系列文档的定位矛盾）：

- 00-kernel-overview.md L100-199 明确说"内核的主线叙事是从 GRUB 到第一个用户进程的严格线性过程"
- 01 遵循了这个叙事（GRUB → pre_init → paging）
- 02 打破了这个叙事（直接跳到运行时机制）
- 违反了文档结构规范中 Ch1"解释为什么需要它"和 Ch2"解释函数在整体流程中的角色"的要求
是否需要立即重构？ 这取决于你的判断——内容本身是完整的，只是组织方式不对。如果你想重构，我可以帮你按上述方向重新组织。如果暂时不改，至少应该在文档开头加一段说明，明确本文的叙事定位与 01 的衔接关系。
""" 我判断，这其实是一个P0级问题，对读者不友好，导致读者直接放弃。。。。必须得到妥善的解决。

```
pre_init()                           ← 01-multiboot-bootstrap.md 结束于此
  pg_identity() → pg_mapkernel() → pg_load() → vm_enable_paging()
  return &kinfo

head.S                               ← 返回到汇编
  mov $k_initial_stktop, %esp        ← 切换到高地址栈
  push %eax                          ← kinfo 指针
  call kmain                         ← 进入 C 主函数

kmain()                              ← main.c:96
  memcpy(&kinfo, local_cbi, ...)     ← 保存 boot info
  cstart()                           ← main.c:139: prot_init(), init_clock(), intr_init(), arch_init()
  BKL_LOCK()                         ← main.c:143: 首次获取 BKL
  proc_init()                        ← 清空进程表
  for(i=0; i<NR_BOOT_PROCS; ++i)     ← 初始化 boot image 进程
    arch_boot_proc(ip, rp)           ← 为每个进程设置 arch 状态
      if(rp->p_nr == VM_PROC_NR)     ← ★ 关键：将 VM ELF 加载到 bootstrap 页表
        libexec_load_elf(&execi)     ← 分配 VM 的代码/数据/栈页
        arch_proc_init(rp, ...)      ← 设置 VM 的 pc/sp
    RTS_SET(rp, RTS_VMINHIBIT)       ← 非 VM 进程标记为"等 VM 设置页表"
    RTS_SET(rp, RTS_BOOTINHIBIT)     ← 非 VM 进程标记为"boot 未完成"

  arch_post_init()                   ← ★ protect.c:370
    ptproc = proc_addr(VM_PROC_NR)   ← 设置 ptproc = VM
    pg_info(&vm->p_seg.p_cr3, ...)   ← 告诉内核 VM 的页表在哪

  memory_init()                      ← ★ memory.c:707
    freepdes[0] = kinfo.freepde_start++   ← 分配 2 个 free PDE
    freepdes[1] = kinfo.freepde_start++   ← 用于 createpde() 临时映射

  system_init()                      ← 初始化 system task
  add_memmap(&kinfo, bootstrap_start, bootstrap_len)  ← 回收 bootstrap 内存
  bsp_finish_booting()               ← main.c:36
    vm_running = 0                   ← ★ VM 还没跑！
    switch_to_user()                 ← 切换到用户态，开始调度

  → 内核开始调度进程
  → VM 是第一个运行的（无 VMINHIBIT）
  → VM 初始化自身，创建自己的页表
  → VM 通过 SYS_VMCTL(VMCTL_SETADDRSPACE) 设置自己的 CR3
    → arch_do_vmctl → setcr3()
      → write_cr3(vm->p_seg.p_cr3)  ← ★ 切换到 VM 的真实页表
      → arch_enable_paging(p)        ← ★ 地址空间切换！
        → switch_address_space(caller)
        → video_mem = video_mem_vaddr ← 切换到虚拟地址
        → APIC 地址切换
  → VM 通过 SYS_VMCTL(VMCTL_KERN_PHYSMAP) 协商物理映射
    → arch_phys_map()               ← 内核声明需要映射的物理区域
  → VM 通过 SYS_VMCTL(VMCTL_KERN_MAP_REPLY) 回复映射结果
    → arch_phys_map_reply()         ← 内核获得虚拟地址
  → VM 为其他进程创建页表
  → VM 通过 SYS_VMCTL(VMCTL_VMINHIBIT_CLEAR) 解除进程的 VMINHIBIT
  → vm_running = 1（在 VM 进程中设置）

  → 之后：内核开始处理来自用户进程的 system call
    → createpde() / lin_lin_copy() / vm_memset() / vm_lookup()
    → 这些是运行时跨地址空间访问机制
    → VMSUSPEND 机制在 VM running 后才生效
```
2. """
head.S                               ← 返回到汇编
  mov $k_initial_stktop, %esp        ← 切换到高地址栈
  push %eax                          ← kinfo 指针
  call kmain                         ← 进入 C 主函数 
""" 这个rust版本有吗？我完全没印象，这个是否必须？redox是怎么做的？ 这好像是01文档结束时需要做的事情？也许应该修复的是01文档？？

3. """
  memcpy(&kinfo, local_cbi, ...)     ← 保存 boot info
  cstart()                           ← main.c:139: prot_init(), init_clock(), intr_init(), arch_init()
  BKL_LOCK()                         ← main.c:143: 首次获取 BKL
  proc_init()                        ← 清空进程表
  """ 这些内容，全部都没有？？？ 02文档所讲述的内容，全部在这些内容之后吧？这是跳过了太多内容了？甚至我阅读了整个03-stage-kernel的目录，根本没有发现任何init_clock的地方？？？所以并不仅仅是 "这是一个 P1 结构性问题 （不是 P0，因为内容本身没有事实错误，但叙事逻辑与系列文档的定位矛盾）："，而是03-stage-kernel目录层级的P0问题了，它跳过了大量内容？？？

