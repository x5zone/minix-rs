# 03-stage-kernel 全局 TODO

> 本文件汇总 03-stage-kernel 中各文档尚未完成、待后续实现的 Rust 开发任务。
> 每个条目标注来源文档。

---

## 1. Boot module 内存回收（Rust 实现）

> 来源：`01-multiboot-bootstrap.md` / `01-todo.md` C 项（C 源码讲解已完成）

**背景**：Minix3 的 boot module 物理内存生命周期是：`cut_memmap()` 临时切掉 → `protect.c` 解析 ELF 复制到进程空间 → `add_memmap()` 回收。C 源码已在 01 文档 §2.5 讲解完毕。

**Rust 实现待办**：

| 环节 | Minix3 C | Rust minix-rs | 状态 |
|------|----------|---------------|------|
| boot module 加载 | GRUB 传入 `module_list[]` | `boot_modules: &[]` 始终为空 | ❌ 缺失 |
| 从 memmap 切掉 module 内存 | `cut_memmap()` (pre_init.c:211) | 无等价实现 | ❌ 缺失 |
| ELF 解析 + 复制到进程空间 | `protect.c` 解析 ELF | `kmain()` 是 `loop {}` | ❌ 缺失 |
| 回收 module 物理内存 | `add_memmap()` (protect.c:450) | 无等价实现 | ❌ 缺失 |
| 回收 bootstrap 代码内存 | `add_memmap()` (main.c:301) | 无等价实现 | ❌ 缺失 |

`BootModule` 结构体已定义（`minix-types/src/kernel_info.rs`）但从未被填充使用。不仅在回收逻辑缺失，整个 boot module 生命周期（加载→ELF解析→复制→回收）都尚未实现。

---

## 2. qemu-tests 后续扩展

> 来源：`01-todo.md` D 项（目录结构重组已完成）

**待办**：

- [ ] 按文档语义添加后续测试目录：

```
test-kernels/
├── kernel/
│   ├── bootstrap/      # 01-multiboot-bootstrap.md ✅ 已创建
│   ├── pagetable/      # 02-page-table-kernel.md
│   ├── exception/      # 05-exception-interrupt.md
│   └── ipc/            # 09-sync-ipc.md
└── vm/                 # 02-stage-vm/
    └── ...
```

- [ ] 代码去重：test-kernel 中的 UEFI 入口/串口/memmap 代码应复用 `boot-shim` + `kernel` 的正式代码，而非各自复制
- [ ] 全系统 QEMU 启动规划：每个阶段用同一内核 + 不同 boot modules

---

## 3. 页表页分配器：VM 阶段接入

> 来源：`01-todo.md` E 项（boot 阶段已完成）

- [ ] 设计 memmap 中切除 bump 范围的机制（确保 VM 不踩踏）
- [ ] VM 接入时实现 `vm_pt_alloc` 并注册
