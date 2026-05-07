# VM 文档 Review 修改记录 (10-12)

## 10-phys-block.md

### P0 修改（概念错误修复）

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| §1.2 | 删除 Rust 代码块 | 第1章不得包含Rust内容 |
| §1.3 | 删除"Rust"列，6行3列→6行2列表 | 第1章不得包含Rust内容 |
| §2.3 | 删除3处"Rust实现建议"代码块 | 第2章不得包含Rust内容 |
| §2.4 pb_free | 虚构的引用计数版替换为真实pb_free+pb_unreferenced | 原pb_free是虚构的，Minix3的pb_free不做引用计数 |
| MAP_NONE值 | 0→0xFFFFFFFE | 与vm.h源码一致 |
| SLABALLOC宏 | 虚构的`allocate(&pb_slab,...)`→真实`slaballoc(sizeof(*var))` | 原宏定义不存在 |
| ABS2CLICK宏 | 修正宏名 | 与源码不一致 |
| USE宏 | 补充遗漏 | 源码中存在 |
| PbFlags::DIRTY | 删除虚构标志 | 源码中不存在 |
| pb_slab | 删除虚构声明 | 源码中不存在 |

### P1 修改（结构/质量修复）

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| 全文 | 约40个ASCII框图替换为文字/表格/列表 | ASCII图滥用 |
| 第1章 | 清除Rust内容 | 文档结构违规 |
| 多处 | 标注32/64位差异 | 架构差异未说明 |
| 多处 | 虚构代码标注 | 概念准确性 |
| §6 | 测试要点化 | 原为测试代码 |
| §7 | 参见补全 | 交叉引用不完整 |
| 格式 | `---###` 格式修复 | Markdown格式错误 |

### P2 修改

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| 线程安全 | 讨论简化 | 过度展开 |

---

## 11-memtype.md

### P0 修改（概念错误修复）

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| §1.2 | 删除Rust代码块(MemType trait等) | 第1章不得包含Rust内容 |
| §1.3 | 删除"Rust"列 | 第1章不得包含Rust内容 |
| §2.3 | 删除3处Rust代码块(约100行) | 第2章不得包含Rust内容 |
| §2.4 pb_free | 虚构引用计数版→真实pb_free+pb_unreferenced | pb_free(pb.c:54-59)不做引用计数，引用计数在pb_unreferenced(pb.c:96-134) |
| §2.2.4 cache_writable | "总是可写"→"物理页已分配则可写" | 源码cache_writable(mem_cache.c:76-80)返回`pr->ph->phys != MAP_NONE` |
| §4.2 DirectPhysical.is_writable | `fn is_writable(&self, _pr) -> bool { true }`→`pr.get_phys_addr().is_some()` | C源码phys_writable返回`phys != MAP_NONE` |

### P1 修改（结构/质量修复）

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| §1.4 | 删除约20行ASCII架构图→3点文字 | ASCII图滥用 |
| §2.2.1-2.2.6 | 约15个ASCII流程图→编号列表 | ASCII图滥用 |
| §2.1.1后 | 新增32位vs64位差异说明 | 架构差异未说明 |
| §2.2.5 | mappedfile简化伪代码→带行号完整源码引用 | 行为简化误导 |
| §2.2.5 | 删除重复cow_block展示 | 重复展示 |
| §2.2.1 | `remaps`注释修正：mremap→共享内存映射 | 概念混淆 |

### P2 修改

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| §7 参见 | 3条→7条 | 交叉引用不完整 |

---

## 12-vir-region.md

### P0 修改（概念错误修复）

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| §2.1 param联合体 | `VR_MAPPEDFILE`→`def_memtype==mem_type_mappedfile` | VR_MAPPEDFILE标志不存在 |
| §2.4.1 phys_region | 添加`memtype`字段和SANITYCHECKS字段 | 关键字段遗漏 |
| §2.4.1 phys_block | 添加`seencount`字段(SANITYCHECKS) | 条件编译遗漏 |
| §2.3.1 map_page_region | 补充MF_PREALLOC处理、VR_UNINITIALIZED清除 | 行为简化误导 |
| §2.3.1 region_new | 补充static id、remaps=0等初始化 | 字段初始化遗漏 |
| §2.3.2 map_free/map_subfree | 补充调试输出和SANITYCHECKS验证 | 代码不完整 |
| §2.3.3 map_lookup | 补充SANITYCHECKS panic和offset断言 | 代码不完整 |
| §2.4.2 pb_link | 使用USE()宏包裹 | 代码不准确 |
| §2.4.3 pb_unreferenced | 补充assert、链表遍历assert、ev_unreference返回值检查 | 行为简化误导 |
| §4.4 split_region | 完整重写：添加assert、goto bail、r2迁移循环 | 行为简化误导 |

### P1 修改（结构/质量修复）

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| §5 fork相关 | 整章精简为概述+参见引用 | 与17-vm-fork.md/15-cow-mechanism.md重复 |
| §3.1后 | 新增32位vs64位差异表 | 架构差异未说明 |
| §2.3/§2.4 | 补全行号引用 | 行号缺失 |
| §5.2 | map_ph_writept行号修正 | 行号偏移 |
| §7 参见 | 扩展至8篇引用 | 参见不完整 |

### P2 修改

| 位置 | 修改内容 | 原因 |
|------|---------|------|
| §1.2 | 地址空间布局关联Minix3常量 | 概念准确性 |
| §5 CoW流程图 | 随§5精简删除 | ASCII图滥用 |
