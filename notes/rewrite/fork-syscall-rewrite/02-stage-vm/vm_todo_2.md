# VM 文档 Review 修改记录 (15-16)

## 15-cow-mechanism.md

### P0 修改（概念错误修复）

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| 概述表 | 删除虚构`pt_makereadonly()`，替换为`map_writept()/map_ph_writept()/pr_writable()` | pt_makereadonly在Minix3源码中不存在 |
| CoW流程图 | `pt_makereadonly(parent/child_region)`→`map_writept()→map_ph_writept()→pr_writable()` | 同上 |
| §2.2 anon_pagefault | 删除虚构`free_mem(new_page_cl, 1)` | 源码中refcount<2或!write分支直接return OK，无free_mem |
| §2.3 map_writept | 单页函数签名→两层遍历函数；PTF_*标志→ARCH_VM_PTE_*宏；直接refcount>1→pr_writable() | 原签名和逻辑完全错误 |
| §2.4 map_proc_copy | 虚构代码+虚构`map_proc_set_readonly`→三层函数`map_proc_copy→map_proc_copy_range→map_copy_region` | map_proc_set_readonly不存在 |
| §2.4 cache/shared_pagefault | 完全重写：cache_pagefault实际是链接缓存页非CoW；shared_pagefault实际是从源进程获取phys_block | 原代码虚构 |
| PTE标志 | "PTE_P标志清除"→"PTF_WRITE标志清除"；CoW不清除Present位，清除Write位 | PTE标志描述错误 |
| §6.4 vm_bytecopies | 删除`vm_bytecopies+=PAGE_SIZE`和`memcpy()`→`sys_abscopy()`+VMSTATS说明 | mem_cow使用sys_abscopy而非memcpy，vm_bytecopies未在mem_cow中递增 |
| §2.5 do_fork | 极简版+签名错误→实际do_fork代码(fork.c:32) | 函数签名和实现错误 |

### P1 修改（结构/质量修复）

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| 多处 | 标注x86-32 vs x86-64差异：2级→4级页表、32→64位PTE、NX位 | 32/64位差异未标注 |
| §2.3 | mem_cow后页表更新：`map_writept(vmp,region,ph)`→`map_ph_writept(vmp,region,ph)` | 调用链描述错误 |
| §8 参见 | 4篇→8篇：添加12-vir-region/14-phys-region/06-pagetable-struct/07-pagetable-ops | 参见不完整 |

### P2 修改

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| 全文 | 25个ASCII框图→编号列表/Markdown表格/简洁文字 | ASCII图滥用 |
| 格式 | `---###`等格式错误修正 | Markdown格式错误 |
| §2.2 | CoW触发条件："释放预分配的页"→"源码中预分配的页未释放" | 与实际代码不一致 |
| 多处 | `PAGE_SIZE`→`VM_PAGE_SIZE` | Minix3使用VM_PAGE_SIZE |

---

## 16-pagefault.md

### P0 修改（概念错误修复）

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| §2.3 physblock_get | 链表遍历→数组索引O(1)查找(region.c:60) | 实现完全错误：源码用数组不用链表 |
| §2.3 region_search | 删除虚构AVL树遍历代码→CAVL宏展开说明 | 函数由CAVL宏包生成，签名完全不同 |
| §2.4 mem_cow | memcpy+refcount--→sys_abscopy+pb_unreferenced+pb_link+切换memtype | 实现完全错误 |
| §2.1 VR标志 | VR_NONE/VR_READABLE/VR_EXECUTABLE等→真实定义(VR_WRITABLE=0x001等) | 虚构标志定义 |
| §2.5 | 删除虚构函数check_memory_pressure()/log_oom_event()/alloc_mem_safe() | 函数不存在于源码 |
| §2.4 mappedfile_pagefault | 3行伪代码→完整实现(mem_file.c:84) | 简化过度 |
| §2.4 direct_pagefault | 函数名修正：direct_pagefault→phys_pagefault | 函数名错误 |
| §2.5 栈扩展 | VR_GROWSDOWN和vm_stack_low→说明Minix3不支持栈自动扩展 | 虚构功能 |
| §2.5 | 删除虚构is_instruction_fetch函数 | x86-32不支持执行权限位检测 |
| §2.6 map_ph_writept | 补全pr_writable()、PTF_PRESENT|PTF_USER基础标志、pt_flags回调、WMF_OVERWRITE | 代码不准确 |

### P1 修改（结构/质量修复）

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| 多处 | 标注x86-64额外错误码位(bit 2 reserved, bit 4 NX) | 32/64位差异未标注 |
| §2.2 | 补全do_pagefaults签名(pagefaults.c:240)和消息字段 | 签名缺失 |
| §2.2 | 补充hm_state的VALID宏(0xc0ff1) | 定义遗漏 |
| §7 参见 | 3篇→9篇 | 参见不完整 |
| §2.4 shared_pagefault | "映射共享页"→"从源区域获取物理块并链接" | 描述不准确 |
| 多处 | 补全所有C代码行号引用 | 行号缺失 |

### P2 修改

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| 全文 | 12个ASCII框图→编号列表/表格 | ASCII图滥用 |
| §6 | Rust测试代码→测试要点列表 | 测试章节应为要点 |
| §6.3 | 添加Minix3不支持栈自动扩展说明 | 场景错误 |
| PFERR表 | 移除"栈扩展"场景 | Minix3不支持 |
