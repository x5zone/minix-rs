# 05-memory-driver — 内存设备驱动 memory（boot image）

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

char+block 双面（memory.c:64,72 m_cdtab/m_bdtab）、设备表（/dev/mem、/dev/kmem、/dev/ram*、/dev/null、/dev/zero、/dev/boot、/dev/imgrd）、vm_map_phys 物理映射（memory.c:141）、RAM 盘后端（boot image 成员 kernel/table.c:58）。C: drivers/storage/memory/memory.c。Rust: os/drivers/storage/memory。

## 边界

- **前置依赖**: 01/02；02-stage-vm 物理映射接口
- **本篇不覆盖**: VM 物理映射机制（02-stage-vm）；root FS 挂载（15-stage-fs）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
