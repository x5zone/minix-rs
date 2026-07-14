# 02-todo: 03-stage-kernel 启动叙事的 TODO 列表

> **创建**: 2026-06-08
> **触发**: 对 01-boot-shim-bootstrap.md 和 boot-shim 加载 boot 模块机制的讨论
> **状态**: 讨论中，待决策

---

## 0. 讨论记录：Minix3 C 是怎么加载 boot 进程的？

### 0.1 Minix3 C 的完整链路

```
GRUB (boot.cfg)
  ├── load_mods /boot/minix_default/mod*    ← 把所有 boot 模块（VM/PM/VFS/RS...）作为纯数据加载到物理内存
  ├── multiboot /boot/minix_default/kernel  ← 加载内核 ELF 到物理内存
  └── 跳转到内核入口

内核 (pre_init.c)
  ├── get_parameters()
  │     ├── 遍历 multiboot_info_t.mi_mods → 拷贝到 kinfo.module_list[]
  │     └── cut_memmap() 切掉 module 占用的物理内存（防止后续分配器误用）
  └── pg_identity → pg_mapkernel → vm_enable_paging → return &kinfo

内核 (kmain → main.c)
  ├── for (i=0; i<NR_BOOT_PROCS; ++i)
  │     └── arch_boot_proc(ip, rp)
  │           ├── if VM: libexec_load_elf(&execi)  ← 内核自带 ELF 加载器，分配页，拷贝 ELF 到进程地址空间
  │           └── else: ip->start_addr = mod->mod_start  ← 仅记录物理地址
  │     ├── RTS_SET(rp, RTS_VMINHIBIT)   ← 非 VM 进程等 VM 创建页表
  │     └── RTS_SET(rp, RTS_BOOTINHIBIT) ← 非 VM 进程等 boot 完成
  │
  └── add_memmap(bootstrap)  ← 回收 module 物理内存（已拷贝到进程空间后归还）

VM 启动后
  └── 为 PM/VFS/RS 等创建页表，解除 VMINHIBIT
```

### 0.2 关键结论

1. **GRUB 只做纯数据加载**：GRUB 不解析 ELF，不搬迁，不重定位。它只是把文件内容原样放到物理内存。
2. **内核自己做 ELF 解析**：`arch_boot_proc` → `libexec_load_elf` 解析 VM 的 ELF，分配页，拷贝段。
3. **只有 VM 被内核主动加载**：其他进程（PM/VFS/RS）只是记录 `mod_start`，等 VM 启动后由 VM 创建页表。
4. **GRUB 的 `load_mods` 不是"搬迁"**：它只是把文件从磁盘读到内存，没有地址变换。

### 0.3 对 Rust 版本的含义

| 问题 | 回答 |
|------|------|
| UEFI/OpenSBI 会替我们加载 boot 模块吗？ | **不会。** UEFI 只加载 `.efi` 文件（即 boot-shim）。OpenSBI 只加载 kernel。它们都不认识 GRUB 的 `load_mods` 语义。 |
| boot-shim 必须有加载 boot 模块的代码吗？ | **是。** 这是必然的。要么编译时嵌入（`include_bytes!`），要么运行时从 ESP 分区读取。 |
| 01 文档是否需要修改？ | **是。** 01 文档 §1.7.1 说"模块加载: UEFI `ImageHandle` protocol 标准接口"——这是**不准确的**。UEFI 的 `ImageHandle` 只加载 `.efi` 自身，不加载额外的 boot 模块。boot 模块的加载必须在 boot-shim 中显式实现。 |

---

## 1. 四个 AI 方案的共识与分歧

已阅读 4 份独立分析：`02-answer-ds.md`, `02-answer-kimi.md`, `02-answer-glm.md`, `02-answer-minimax.md`。

### 1.1 共识（4/4 一致）

| 问题 | 结论 |
|------|------|
| 01→02 叙事断裂严重程度 | **P0**（不是"组织不好"，是"关键内容缺失+读者无法继续"） |
| head.S 的高地址跳转在 Rust 版是否必须 | **必须实现**（语义相同，形式不同——boot-shim trampoline） |
| 03-stage-kernel 目录覆盖缺口 | **P0**（arch_post_init/memory_init/system_init/add_memmap 无文档） |
| 独立 kernel ELF | **必须**（当前 rlib 链接方式无法实现 trampoline） |
| 手写 ELF 加载器 | **同意**（~50 行，不依赖第三方 crate） |
| 01 文档只需微调 | **同意**（加过渡节，不需要重写） |

### 1.2 分歧

| 维度 | DS | Kimi | GLM | Minimax | 建议 |
|------|-----|------|-----|---------|------|
| 新文档数 | 4 篇 | 10 篇 | 5 篇 | 10 篇 | **5 篇**（避免碎片化） |
| cstart 是否独立文档 | 否（合并到 kmain-boot） | 是（#05） | 否（合并到 kmain-boot） | 是（#04） | **否**（cstart 只是 4 个函数调用的容器） |
| AT() 是否保留 | 不保留 | 不保留 | 不保留 | 保留（给 boot-shim 算物理地址） | **不保留**（boot-shim 自行计算更清晰） |
| trampoline 实现位置 | kernel `_start` | `arch_boot_impl` 结尾 | 独立汇编文件 | kernel `_start`（独立汇编） | **独立汇编文件 + kernel `_start`**（Minimax 方案最准确） |
| kmain-entry 是否独立 | 否（合并） | 是（#04） | 否（合并） | 是（#03） | **否**（memcpy 几行不值得独立文档） |

### 1.3 决策

采用 **5 篇新文档** 方案（GLM 方案为主，吸收 Minimax 对 trampoline 的修正）：

| 编号 | 标题 | 覆盖范围 | 行数 |
|------|------|---------|------|
| **02** | boot-bridge | AT()机制、head.S语义、独立ELF设计、ELF加载器、trampoline、boot模块加载 | ~600 |
| **03** | kmain-boot | kmain完整boot序列：memcpy→cstart→BKL→proc_init→arch_boot_proc→arch_post_init→memory_init→system_init→add_memmap→bsp_finish_booting | ~800 |
| **04** | vm-boot-protocol | VM启动后协商：VMCTL_SETADDRSPACE、VMCTL_KERN_PHYSMAP、VMCTL_KERN_MAP_REPLY、VMCTL_VMINHIBIT_CLEAR | ~500 |
| **05** | runtime-cross-space | createpde/lin_lin_copy/vm_memset/vm_lookup/VMSUSPEND（原 02-page-table-kernel 内容重新定位） | ~400 |
| — | 01 修改 | 结尾增加过渡节（~80行），说明 head.S 三行 + 预告 kmain | ~80 |

---

## 2. TODO 列表

### P0 — 阻塞性（文档误导读者 / 代码有隐藏 bug）

- [ ] **TODO: 01 文档修正 — UEFI 模块加载描述不准确**

  **Priority**: P0 | **Type**: 文档事实错误 | **File**: `01-boot-shim-bootstrap.md`

  **问题**: §1.7.1 表格中"模块加载: UEFI `ImageHandle` protocol 标准接口"——**不准确**。UEFI 的 `ImageHandle` 只加载 `.efi` 自身，不加载额外的 boot 模块。boot 模块（VM/PM/VFS/RS 等）的加载必须在 boot-shim 中显式实现（通过 `SimpleFileSystemProtocol` 读 ESP 分区，或编译时 `include_bytes!`）。

  **Fix**: 修改该表格行，说明 boot 模块加载是 boot-shim 的职责，UEFI 不替你做。同时补充 §1.7.1 中关于 boot 模块加载路径的说明。

  **Verify**: 对照 UEFI Spec §7.4（ImageHandle 只加载调用者指定的单个 image）和 Minix3 C 的 `boot.cfg`（GRUB `load_mods` 加载多个模块）。

- [ ] **TODO: 01 文档结尾增加过渡节**

  **Priority**: P0 | **Type**: 文档叙事断裂 | **File**: `01-boot-shim-bootstrap.md`

  **问题**: 01 结束于 `vm_enable_paging()`，读者不知道"接下来发生什么"。需要增加过渡节说明 head.S 三行代码（切栈+跳高地址）和 kmain 的概览。

  **Fix**: 在 01 末尾增加 §1.8（或 §6），内容：
  - head.S 三行代码的语义（切栈 + push kinfo + call kmain）
  - 这是 higher-half kernel 的标准做法
  - Rust 版的差异（boot-shim trampoline 替代）
  - 下一站指向 02-boot-bridge.md

  **Verify**: 对照 `minix3/minix/kernel/arch/i386/head.S:78-87` 和 `minix3/minix/kernel/main.c:96-324`。

- [ ] **TODO: 撰写 02-boot-bridge.md（核心新文档）**

  **Priority**: P0 | **Type**: 文档新建 | **File**: `02-boot-bridge.md`（新建）

  **问题**: 从 paging enable 到 kmain 入口之间，缺失 trampoline、ELF 加载、boot 模块加载的全部知识。这是代码 P0 和文档 P0 的交汇点。

  **Fix**: 撰写 ~600 行文档，覆盖：
  - Ch1: 概述 — 问题、三件事必须配合（trampoline + ELF加载 + 链接脚本）
  - Ch2: C 源码分析 — head.S 三行、kernel.lds AT()、为什么只有内核需要高地址
  - Ch3: Rust 设计决策 — 不保留 AT()、独立 ELF、trampoline 必须是独立汇编文件
  - Ch4: 实现详解 — 链接脚本、ELF 加载器、恒等+高映射、asm trampoline、**UEFI 读分区加载 boot 模块**
  - Ch5: 测试要点
  - Ch6: 参见

  **Verify**: 对照 `head.S`, `kernel.lds`, `main.c:96-324`, Redox bootloader 源码。

- [ ] **TODO: 实现 trampoline（代码 P0）**

  **Priority**: P0 | **Type**: 代码语义缺失 | **File**: `os/kernel/src/lib.rs`, `os/kernel/src/arch/x86_64/trampoline.S`（新建）

  **问题**: 当前 `arch_boot_impl` 在 `paging.enable()` 后直接返回，RIP 和 RSP 都在低地址。恒等映射一旦被 VM 移除（必然发生），内核立即崩溃。当前 `kmain()` 是 `loop {}` 所以暂时没出事，但这是定时炸弹。

  **Fix**:
  1. 创建独立汇编文件 `trampoline.S`（`.unpaged_text` section，链接到低地址）
  2. `arch_boot_impl` 结尾不返回，而是 `jmp` 到 trampoline
  3. trampoline 切 RSP 到高地址内核栈，`jmp` 到高地址 kmain
  4. 使用 `global_asm!` 引入，不用 `asm!` 宏（避免编译器插 prologue/epilogue）

  **Verify**: QEMU + GDB 验证 RIP 在高地址，移除恒等映射后内核仍能运行。

- [ ] **TODO: 独立 kernel ELF（代码 P0）**

  **Priority**: P0 | **Type**: 代码架构 | **File**: `os/kernel/Cargo.toml`, `os/kernel/src/link.ld`（新建）, `os/boot-shim/build.rs`

  **问题**: 当前 minix-kernel 是 rlib 链接进 boot-shim，所有符号在低地址。没有链接脚本，没有 VMA 高地址。trampoline 的前提不存在。

  **Fix**:
  1. 创建链接脚本 `link.ld`，定义 `KERN_VIRT_BASE` 和 `KERN_PHYS_BASE`
  2. kernel 编译为独立 ELF binary（`staticlib` 或独立 bin target）
  3. build.rs 编译 kernel ELF 后嵌入 boot-shim
  4. boot-shim 解析 ELF PT_LOAD 段，拷贝到物理页

  **Verify**: `readelf -l kernel.elf` 验证 VMA 是高地址，LMA 是低物理地址。

- [ ] **TODO: boot-shim 加载 boot 模块（VM/PM/VFS/RS）**

  **Priority**: P0 | **Type**: 代码语义缺失 | **File**: `os/boot-shim/src/`

  **问题**: 当前 `KernelInfo.boot_modules` 始终为空（`&[]`）。boot 模块（VM/PM/VFS/RS 等）完全没有被加载到内存。kmain 无法初始化任何 boot 进程。

  **Fix**:
  1. 在 `ExitBootServices` 之前，通过 UEFI `SimpleFileSystemProtocol` 读取 ESP 分区中的 `/boot/vm.bin`, `/boot/pm.bin` 等
  2. 分配物理页（`AllocatePages`），拷贝模块内容到物理内存
  3. 填充 `KernelInfo.boot_modules` 列表（物理地址 + 大小 + 命令行参数）
  4. 或者：编译时 `include_bytes!` 嵌入（适合小模块，但会增大 boot-shim 体积）

  **决策**: 采用方案 A（UEFI 读分区）——灵活，与 GRUB `load_mods` 语义一致。但 kernel 本身必须编译时嵌入（ExitBootServices 后文件系统不可用）。

  **Verify**: QEMU 启动后，GDB 检查 `KernelInfo.boot_modules` 非空，模块物理地址处有合法的 ELF header。

### P1 — 重要但不阻塞

- [ ] **TODO: 撰写 03-kmain-boot.md**

  **Priority**: P1 | **Type**: 文档新建 | **File**: `03-kmain-boot.md`（新建）

  **问题**: kmain 的完整 boot 序列（memcpy → cstart → BKL → proc_init → arch_boot_proc → arch_post_init → memory_init → system_init → add_memmap → bsp_finish_booting）没有一份文档以时间线叙事覆盖。

  **Fix**: 撰写 ~800 行文档，以 main.c 行号为时间线，引用 tmp-* 子系统的参考资料文档。

  **Verify**: 对照 `minix3/minix/kernel/main.c:96-324`。

- [ ] **TODO: 撰写 04-vm-boot-protocol.md**

  **Priority**: P1 | **Type**: 文档新建 | **File**: `04-vm-boot-protocol.md`（新建）

  **问题**: VM 启动后的内核-VM 协商协议（VMCTL_SETADDRSPACE, VMCTL_KERN_PHYSMAP, VMCTL_KERN_MAP_REPLY, VMCTL_VMINHIBIT_CLEAR）没有独立文档。

  **Fix**: 撰写 ~500 行文档，覆盖 VM 启动后与内核的完整协商流程。

  **Verify**: 对照 `minix3/minix/kernel/arch/i386/protect.c` 和 VM 侧源码。

- [ ] **TODO: 原 02-page-table-kernel.md 重新定位为 05-runtime-cross-space.md**

  **Priority**: P1 | **Type**: 文档重组 | **File**: `02-page-table-kernel.md` → 重命名为 `05-runtime-cross-space.md`

  **问题**: 当前 02 的内容（createpde/lin_lin_copy/vm_memset/vm_lookup）是运行时机制，在 VM running 之后才生效。放在 02 位置打破了线性叙事。

  **Fix**: 重命名为 05，在文档开头增加"前置条件"段（freepdes 来自 memory_init、ptproc 来自 arch_post_init），并引用 03-kmain-boot 和 04-vm-boot-protocol。

- [ ] **TODO: 填写 KernelInfo.boot_modules（代码侧）**

  **Priority**: P1 | **Type**: 代码实现 | **File**: `os/kernel/src/`, `minix-types/src/kernel_info.rs`

  **问题**: `BootModule` 结构体已定义（`minix-types/src/kernel_info.rs`）但从未被填充使用。不仅回收逻辑缺失，整个 boot module 生命周期（加载→ELF解析→复制→回收）都未实现。

  **Fix**: 在 kmain 中消费 `kernel_info.boot_modules`，实现 `arch_boot_proc` 的逻辑（VM 特殊处理：ELF 解析+拷贝；其他进程：记录物理地址+设置 VMINHIBIT）。

  **Verify**: 对照 `minix3/minix/kernel/main.c:168-277` 和 `protect.c:388-448`。

### P2 — 优化

- [ ] **TODO: 撰写 05-runtime-cross-space.md 的详细内容**

  **Priority**: P2 | **Type**: 文档内容 | **File**: `05-runtime-cross-space.md`（原 02）

  **问题**: 当前 02 的内容需要重新组织，增加前置条件说明，确保与 03 和 04 的引用关系正确。

- [ ] **TODO: 临时文档 tmp-* 清理**

  **Priority**: P2 | **Type**: 文档清理 | **File**: `tmp-*` 系列

  **问题**: tmp-* 文档加了前缀后变成"参考资料"，但旧编号系统（02→tmp-02, 03→tmp-03, ...）与新编号系统（02=boot-bridge, 03=kmain-boot, ...）冲突，容易混淆。

  **Fix**: 在新文档体系完成后，将 tmp-* 移入 `reference/` 子目录，或统一改为 `ref-*` 前缀。

---

## 3. 关于 boot 模块加载的决策记录

### 3.1 问题

用户发现：01 文档说 UEFI `ImageHandle` 加载模块，但 UEFI 实际上只加载 `.efi` 自身。boot-shim 中没有加载 VM/PM 等 boot 模块的代码。

### 3.2 Minix3 C 的真相

GRUB 通过 `boot.cfg` 中的 `load_mods` 命令把所有 boot 模块（VM/PM/VFS/RS...）作为**纯数据**加载到物理内存。GRUB 不解析 ELF，不搬迁，不重定位。内核的 `arch_boot_proc` + `libexec_load_elf` 负责解析 VM 的 ELF 并拷贝到进程地址空间。

### 3.3 Rust 版本的决策：boot 模块加载

**已确认**：boot-shim 必须在 `ExitBootServices` 之前，通过 UEFI `SimpleFileSystemProtocol` 从 ESP 分区读取 VM/PM/VFS/RS 等 boot 模块的 ELF 文件，作为**纯数据**加载到物理内存（分配物理页 + 拷贝）。与 GRUB `load_mods` 语义一致——bootloader 阶段只做纯数据搬运，不解析 ELF。

**理由**（见上一轮讨论）：
- 这是所有主流 OS 的最佳实践（GRUB 加载 Linux `initramfs`、Redox bootloader 加载 kernel ELF、seL4 elfloader 加载 CPIO）
- `ExitBootServices` 后文件系统不可用，必须在 bootloader 阶段完成所有磁盘 I/O
- 内核不应该在 boot 阶段自带 FAT32 + NVMe 驱动

**kernel.bin 的加载方式**：待讨论（见 §5）。

### 3.4 对 01 文档的影响

01 文档 §1.7.1 的"模块加载"行需要修正为：

```
| 模块加载 | GRUB `load_mods` + `multiboot_info_t.mi_mods` | boot-shim `SimpleFileSystemProtocol` 读 ESP 分区，作为纯数据加载 |
```

---

## 4. 参考资料

- [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) — GRUB → pre_init → paging
- [02-design.md](02-design.md) — 架构设计方案（含链接脚本草案、trampoline 伪代码）
- [00-kernel-overview.md](00-kernel-overview.md) — "严格线性 boot 过程"的承诺
- [02-answer-kimi.md](02-answer-kimi.md) — 独立分析（含 build.rs 编译流程细节）
- [02-answer-minimax.md](02-answer-minimax.md) — 独立分析（含 trampoline 汇编文件的精确设计）
- Minix3 C 源码:
  - `minix3/minix/kernel/main.c:96-324` — kmain 完整实现
  - `minix3/minix/kernel/arch/i386/head.S:78-87` — trampoline 三行
  - `minix3/minix/kernel/arch/i386/kernel.lds` — AT() 链接脚本
  - `minix3/minix/kernel/arch/i386/protect.c:370-448` — arch_post_init + arch_boot_proc
  - `minix3/minix/kernel/arch/i386/memory.c:707-720` — memory_init
  - `minix3/etc/boot.cfg` — GRUB load_mods 配置
- Redox OS: [bootloader](https://gitlab.redox-os.org/redox-os/bootloader) — 独立 ELF + trampoline 参考实现

---

## 5. 讨论项：kernel.bin 是独立 ELF 还是与 boot-shim 打包？

### 5.1 四个 AI 的统一结论

4/4 一致：kernel 必须是**独立 ELF**（不是 rlib），链接到高 VMA 地址。

但它们在"如何送入内存"上有分歧：
- **Kimi/GLM/DS**：编译时 `include_bytes!` 嵌入 boot-shim，boot-shim 解析 ELF 后自行拷贝到物理页
- **Minimax**：kernel 的 `unpaged_text` section 使用 AT() 指定 LMA，boot-shim 链接脚本保证 kernel 字节已在正确物理地址

### 5.2 核心问题：kernel 应该从哪儿来？

用户提出的关键问题：

> 如果是独立的，那么它就应该像 vm，pm 一样，从磁盘 load 进来……
> 如果和 boot-shim 打包的话，那 uefi 会自动的一块加载进来，但是这似乎需要 AT() 这个技巧了？？

这其实不是技术问题，而是**架构哲学问题**。

### 5.3 三条路径分析

| 路径 | 描述 | 加载方式 |
|------|------|---------|
| **A: 独立 ELF，ESP 分区加载** | kernel 编译为独立 `kernel.elf`，放在 ESP 分区，boot-shim 通过 `SimpleFileSystemProtocol` 读取后解析 ELF、分配物理页、拷贝段 | 与 VM/PM 完全相同的 ELF 加载流程 |
| **B: 独立 ELF，编译时嵌入** | kernel 编译为独立 ELF，build.rs 通过 `include_bytes!` 嵌入 boot-shim，boot-shim 从内存中解析 ELF、分配物理页、拷贝段 | ELF 字节已在 boot-shim 数据段中，但仍需拷贝到目标物理页 |
| **C: rlib 链接进 boot-shim（当前状态）** | kernel 是 rlib，所有符号在低地址，无 trampoline | UEFI 自动加载，但 kernel 运行在低地址 |

**路径 C 已被排除**（无法实现 higher-half kernel）。

### 5.4 路径 A vs B 的对比

| 维度 | A: 独立 ELF + ESP 加载 | B: 编译时嵌入 |
|------|------------------------|---------------|
| kernel 更新 | 替换 ESP 上的 `kernel.elf` 即可 | 必须重编译 boot-shim.efi |
| boot-shim 代码 | 统一 ELF 加载器（kernel/VM/PM 同代码路径） | kernel 特殊路径（从内存中的 bytes 解析） |
| ESP 文件数 | +1（kernel.elf） | 无额外文件 |
| AT() 需求 | 不需要（boot-shim 自行计算物理地址） | 不需要（boot-shim 自行拷贝） |
| disk I/O | kernel 需要一次文件读取 | kernel 省去一次磁盘 I/O |
| **boot-shim 仍需做的事** | 解析 ELF → 分配物理页 → memcpy → 建页表 → trampoline | **完全相同**（解析 ELF → 分配物理页 → memcpy → 建页表 → trampoline） |

**关键发现**：无论路径 A 还是 B，boot-shim 在 "拿到 kernel ELF 字节后" 做的事情是**完全一样的**。区别只在 "字节来源"。

### 5.5 Minix3 C 的做法

GRUB 加载 kernel ELF 和所有 boot 模块，**全部从磁盘**。没有 `include_bytes!` 等效物。Minix3 的 `kernel.lds` 使用 AT() 是因为 GRUB 的 multiboot 协议要求 ELF 的 p_paddr 字段告诉 GRUB 把 segment 放在哪儿——GRUB 只做 memcpy，不做 ELF 解析。

### 5.6 初步判断：路径 A 更优（统一 ESP 加载，kernel 不特殊）

**理由**：
1. **代码统一**：boot-shim 只需要一个 `load_elf_from_disk(path)` 函数，kernel/VM/PM/VFS/RS 全走同一路径。路径 B 需要两套加载逻辑。
2. **运维友好**：更新 kernel 只需要替换 ESP 上的文件，不需要重编译 boot-shim。这对于开发和调试尤其重要（`make kernel` → `cp kernel.elf /mnt/esp/` → reboot）。
3. **Minix3 一致性**：Minix3 C 的 GRUB 就是从磁盘加载内核。Rust 版不应该引入额外约束。
4. **AT() 不需要**：UEFI 不执行 multiboot 协议，不存在"让 UEFI 把 segment 放到指定物理地址"的机制。boot-shim 必须自己解析 ELF、自己分配物理页、自己拷贝。AT() 在这种情况下毫无意义。
5. **ExitBootServices 不是问题**：kernel 和 VM/PM 都是在 ExitBootServices 之前加载的。文件系统此时可用。不存在"kernel 必须在 ExitBootServices 前就位而 VM 不需要"的区别——它们都需要。

### 5.7 待确认

- [ ] **讨论项 #1**: kernel.bin 是否采用路径 A（独立 ELF + ESP 分区加载，与 VM/PM 统一）？
- [ ] **讨论项 #2**: 如果采用路径 A，ESP 分区布局：`/EFI/BOOT/BOOTX64.EFI` (boot-shim), `/boot/kernel.elf`, `/boot/vm.bin`, `/boot/pm.bin`, ...？
- [ ] **讨论项 #3**: TODO（独立 kernel ELF）和 TODO（boot-shim 加载 boot 模块）的代码实现是否可以合并成一个统一的 ELF 加载器？

### 5.8 已确认的决策

- [x] **01 文档已修正**：§1.7.1 表格"模块加载"行已改为"boot-shim 通过 `SimpleFileSystemProtocol` 从 ESP 分区读取"
- [x] **01 文档已补充 ExitBootServices 约束**：§1.7.1 后增加了关键约束说明——`ExitBootServices()` 后文件系统不可用，boot-shim 必须在此之前完成所有磁盘 I/O
- [x] **01 文档已补充 hello-boot rlib 限制**：§5.2 测试内核部分增加了当前架构限制说明——rlib 链接无法实现 higher-half kernel

### 5.9 ELF 解析器是否应该独立为共享 lib？

boot-shim 和 kernel 都需要 ELF 解析能力，但用途不同：

| 消费者 | 用途 | 需要解析到什么程度 |
|--------|------|-------------------|
| **boot-shim** | 加载 kernel ELF 到物理内存 | 解析 PT_LOAD 段 → 拷贝到物理页 → 记录 entry point |
| **kernel (arch_boot_proc)** | 加载 VM 的 ELF 到进程地址空间 | 解析 PT_LOAD 段 → 分配进程页 → 拷贝 → 设置 pc/sp |

两者都需要"解析 ELF PT_LOAD 段"这个核心能力。区别在于"拿到段信息后做什么"——boot-shim 拷贝到物理页，kernel 拷贝到进程地址空间。

**方案**：在 `os/libs/` 下新建 `minix-elf` crate，提供纯数据结构的 ELF 解析（不涉及内存分配和拷贝）：

```rust
// os/libs/minix-elf/src/lib.rs
pub struct ElfFile<'a> { data: &'a [u8] }
pub struct ProgramHeader { p_type: u32, p_flags: u32, p_offset: u64, p_vaddr: u64, p_paddr: u64, p_filesz: u64, p_memsz: u64, ... }

impl<'a> ElfFile<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, ElfError> { ... }
    pub fn program_headers(&self) -> impl Iterator<Item = ProgramHeader> { ... }
    pub fn entry_point(&self) -> u64 { ... }
    pub fn loadable_segments(&self) -> impl Iterator<Item = ProgramHeader> { ... }  // PT_LOAD only
}
```

boot-shim 和 kernel 各自实现"拿到 ProgramHeader 后做什么"的逻辑。`minix-elf` 只负责解析，不负责加载——这保持了 crate 的单一职责和 `no_std` 兼容性。

**待确认**：
- [ ] **讨论项 #4**: ELF 解析器是否独立为 `os/libs/minix-elf` crate？还是分别实现在 boot-shim 和 kernel 中？

---

## 6. 02 文档（boot-bridge）第三章必须包含的设计决策

无论 02 文档最终叫什么名字，其 Ch3（Rust 设计决策）必须包含以下讨论：

| 决策编号 | 讨论项 | 已有结论？ |
|----------|--------|-----------|
| D1 | kernel 是否与 boot-shim 打包（include_bytes! vs ESP 分区加载） | 倾向路径 A（ESP 统一加载），待用户确认 |
| D2 | kernel ELF 的链接方式（rlib vs 独立 ELF + link.ld） | 已确认：必须是独立 ELF |
| D3 | AT() 是否保留 | 已确认：不保留（boot-shim 自行计算物理地址） |
| D4 | trampoline 实现方式（asm! vs global_asm! vs 独立 .S 文件） | 已确认：独立 .S 文件 + global_asm! |
| D5 | boot 模块加载方式（UEFI SimpleFileSystemProtocol vs include_bytes!） | 已确认：UEFI 读 ESP 分区 |
| D6 | ELF 解析器是否独立 crate | 待确认 |

---

## 7. 文档规划：02-10 线性时间线方案

> **创建**: 2026-06-08
> **触发**: 对前版 5 篇方案的三个约束修正
> **约束**: (1) 不引用 tmp_*，只允许前向引用 (2) 单文档 ≤600 行 (3) 严格时间线，绝无遗漏

### 7.1 设计原则

1. **严格时间线**：每篇文档的起始点 = 上一篇的结束点，绝无间隙
2. **只前向引用**：文档 N 只能引用 01~N-1，绝不引用 N+1 或 tmp_*
3. **单文档 ≤600 行**：通过细粒度拆分避免膨胀

### 7.2 完整时间线

```
T0  vm_enable_paging() 返回，pre_init() return &kinfo     ← 01 结束点

T1  head.S: 切栈到高地址 + call kmain                      ← 02 起始
T2  kmain() 被调用，RIP 在高地址                             ← 02 结束

T3  kmain: memcpy(&kinfo) + BSS检查 + kernel_may_alloc      ← 03 起始
T4  cstart(): prot_init() → GDT/IDT/TSS                     ← 03 结束

T5  cstart(): init_clock()                                   ← 04 起始
T6  cstart(): intr_init() + arch_init()                      ← 04 结束

T7  BKL_LOCK() + proc_init()                                 ← 05 起始
T8  arch_boot_proc() 循环：加载 VM ELF，记录其他模块地址       ← 05 结束

T9  arch_post_init(): ptproc=VM, pg_info()                   ← 06 起始
T10 memory_init(): freepdes 分配                              ← 06 结束

T11 system_init() + add_memmap(bootstrap)                     ← 07 起始
T12 bsp_finish_booting() → switch_to_user()                  ← 07 结束

T13 VM 第一个运行，初始化自身页表                              ← 08 起始
T14 VM-VMCTL_SETADDRSPACE → 切换 CR3                         ← 08 结束

T15 VM-VMCTL_KERN_PHYSMAP + KERN_MAP_REPLY                   ← 09 起始
T16 VM-VMCTL_VMINHIBIT_CLEAR → vm_running=1                  ← 09 结束

T17 createpde / lin_lin_copy / vm_memset / vm_lookup          ← 10 起始
T18 VMSUSPEND 机制                                            ← 10 结束
```

### 7.3 文档列表

| 编号 | 文件名 | 标题 | 时间线 | 核心问题 | 预估行数 |
|------|--------|------|--------|---------|---------|
| **02** | `02-higher-half-kernel.md` | 内核如何到达高地址 | T0→T2 | pre_init返回后，RIP还在低地址——怎么跳到高地址的kmain？ | ~500 |
| **03** | `03-kmain-cstart.md` | kmain入口与保护模式初始化 | T3→T4 | kmain拿到kinfo后第一件事是cstart()，prot_init()建立GDT/IDT/TSS | ~550 |
| **04** | `04-clock-interrupt-init.md` | 时钟与中断初始化 | T5→T6 | cstart()的后半段：init_clock + intr_init + arch_init | ~400 |
| **05** | `05-proc-init-boot-proc.md` | 进程表初始化与boot进程加载 | T7→T8 | proc_init清空进程表，arch_boot_proc加载VM的ELF | ~550 |
| **06** | `06-post-init-memory.md` | arch_post_init与memory_init | T9→T10 | ptproc=VM让VM接管页表，freepdes分配是运行时机制的前置条件 | ~400 |
| **07** | `07-kmain-finish-switch.md` | kmain收尾与切换到用户态 | T11→T12 | system_init + 回收bootstrap + switch_to_user | ~400 |
| **08** | `08-vm-boot-setaddr.md` | VM启动与地址空间设置 | T13→T14 | VM第一个运行，通过VMCTL_SETADDRSPACE切换CR3 | ~450 |
| **09** | `09-vm-boot-physmap.md` | VM物理映射协商与VMINHIBIT解除 | T15→T16 | 内核声明物理映射需求，VM回复映射，解除其他进程的VMINHIBIT | ~450 |
| **10** | `10-runtime-cross-space.md` | 运行时跨地址空间访问机制 | T17→T18 | VM running后内核如何访问用户空间 | ~400 |

**总计**：9 篇新文档，~4100 行，平均 ~455 行/篇。

### 7.4 每篇文档的详细规划

#### 02-higher-half-kernel.md（T0→T2）

**核心问题**：pre_init 返回后 RIP/RSP 在低地址，恒等映射移除后内核崩溃。Minix3 C 用 head.S 三行汇编解决，Rust 版怎么做？

| 章节 | 内容 | 行数 |
|------|------|------|
| Ch1 概述 | 为什么内核必须在高地址运行；higher-half kernel 概念；三件事必须配合（独立ELF + 链接脚本 + trampoline） | ~80 |
| §1.1 | 问题定义：01结束时系统处于"分页已开但RIP低地址"的中间态 | |
| §1.2 | 为什么只有内核需要高地址（其他进程由VM管理） | |
| §1.3 | 物理代码从未移动——只是CPU走不同的映射窗口 | |
| Ch2 C源码 | kernel.lds AT()机制 + head.S三行 + GRUB如何加载 | ~150 |
| §2.1 | kernel.lds：VMA=高地址、LMA=低物理、AT()让GRUB按p_paddr放 | |
| §2.2 | head.S:78-87 逐行：call pre_init → mov $k_initial_stktop,%esp → push %eax → call kmain | |
| §2.3 | 切栈和跳高地址必须成对：只跳不切=栈在低地址，恒等映射移除后崩溃 | |
| Ch3 Rust设计 | 不保留AT()、独立ELF、trampoline | ~120 |
| §3.1 | 决策：不保留AT()——boot-shim全盘控制加载路径 | |
| §3.2 | 决策：独立kernel ELF——rlib无法实现higher-half | |
| §3.3 | 决策：trampoline用独立汇编文件，不用inline asm | |
| Ch4 实现 | 链接脚本 + trampoline.S + boot-shim跳转 | ~120 |
| §4.1 | link.ld：KERN_VIRT_BASE + KERN_PHYS_BASE | |
| §4.2 | trampoline.S：mov rsp + jmp entry | |
| §4.3 | boot-shim arch_boot_impl结尾跳trampoline | |
| Ch5 测试 | QEMU+GDB验证RIP在高地址 | ~20 |
| Ch6 参见 | 引用01 | ~10 |

#### 03-kmain-cstart.md（T3→T4）

**核心问题**：kmain 拿到 kinfo 后，cstart() 的 prot_init() 建立 GDT/IDT/TSS——这是后续所有中断和进程切换的基础。

| 章节 | 内容 | 行数 |
|------|------|------|
| Ch1 概述 | kmain六阶段概览；cstart()的角色；prot_init()建立保护模式基础设施 | ~80 |
| §1.1 | kmain六阶段总览（A:入口 B:cstart C:进程表 D:post-init E:system F:finish） | |
| §1.2 | 为什么cstart必须先于proc_init——GDT/IDT是进程切换的前提 | |
| Ch2 C源码 | main.c:96-147 + protect.c | ~180 |
| §2.1 | kmain入口：memcpy + BSS检查 + kernel_may_alloc | |
| §2.2 | cstart()调用序列：prot_init → init_clock → intr_init → arch_init | |
| §2.3 | prot_init()详解：GDT初始化（NULL+KERNEL+USER CS/DS+TSS）、IDT初始化（256个门描述符）、TSS设置 | |
| Ch3 Rust设计 | kmain签名、cstart对应、GDT/IDT抽象 | ~120 |
| §3.1 | kmain(info: &KernelInfo)——无BSS检查（Rust保证零初始化） | |
| §3.2 | Protection trait：init_gdt/init_idt/init_tss | |
| §3.3 | GDT/IDT描述符用bitflags而非裸整数 | |
| Ch4 实现 | kmain骨架 + Protection trait实现 | ~140 |
| Ch5 测试 | | ~20 |
| Ch6 参见 | 引用01、02 | ~10 |

#### 04-clock-interrupt-init.md（T5→T6）

**核心问题**：cstart() 的后半段——时钟和中断初始化，让内核能响应硬件事件。

| 章节 | 内容 | 行数 |
|------|------|------|
| Ch1 概述 | init_clock和intr_init的角色；为什么必须在proc_init之前 | ~60 |
| Ch2 C源码 | clock.c + hw_intr.c | ~120 |
| §2.1 | init_clock()：时钟源初始化、tick频率设置 | |
| §2.2 | intr_init()：8259A/APIC初始化、IRQ掩码 | |
| §2.3 | arch_init()：架构特定初始化 | |
| Ch3 Rust设计 | Clock trait + InterruptController trait | ~100 |
| Ch4 实现 | | ~90 |
| Ch5 测试 | | ~20 |
| Ch6 参见 | 引用01-03 | ~10 |

#### 05-proc-init-boot-proc.md（T7→T8）

**核心问题**：进程表从空到有——proc_init 清空进程表，arch_boot_proc 加载 VM 的 ELF 并设置其他进程的启动参数。

| 章节 | 内容 | 行数 |
|------|------|------|
| Ch1 概述 | 进程表是内核最核心的数据结构；boot进程是系统启动的第一批进程；VM特殊处理 | ~80 |
| §1.1 | 为什么VM必须先于其他进程启动——VM管理所有页表 | |
| §1.2 | VMINHIBIT/BOOTINHIBIT的语义——"等VM创建页表"/"等boot完成" | |
| Ch2 C源码 | main.c:143-277 + protect.c:388-448 | ~180 |
| §2.1 | BKL_LOCK()——首次获取大内核锁 | |
| §2.2 | proc_init()——清空proc[NR_PROCS] | |
| §2.3 | arch_boot_proc()循环：VM用libexec_load_elf加载ELF，其他仅记录mod_start | |
| §2.4 | RTS_SET(RTS_VMINHIBIT/RTS_BOOTINHIBIT) | |
| Ch3 Rust设计 | ProcTable、BootProcLoader、进程状态enum | ~120 |
| §3.1 | ProcTable：从static mut [Proc; NR_PROCS]到安全抽象 | |
| §3.2 | BootProcLoader：VM用minix-elf解析，其他记录BootModule | |
| §3.3 | ProcState enum替代RTS位标志 | |
| Ch4 实现 | | ~140 |
| Ch5 测试 | | ~20 |
| Ch6 参见 | 引用01-04 | ~10 |

#### 06-post-init-memory.md（T9→T10）

**核心问题**：arch_post_init 让 VM 接管页表（ptproc=VM），memory_init 分配 freepdes——这是运行时跨地址空间机制的前置条件。

| 章节 | 内容 | 行数 |
|------|------|------|
| Ch1 概述 | ptproc=VM的含义；freepdes的用途；为什么这两步必须在boot进程加载之后 | ~60 |
| Ch2 C源码 | protect.c:370-387 + memory.c:707-720 | ~120 |
| §2.1 | arch_post_init()：ptproc=proc_addr(VM_PROC_NR)，pg_info() | |
| §2.2 | memory_init()：freepdes[0]=kinfo.freepde_start++，分配2个free PDE | |
| Ch3 Rust设计 | ptproc表达、freepdes管理 | ~100 |
| Ch4 实现 | | ~90 |
| Ch5 测试 | | ~20 |
| Ch6 参见 | 引用01-05 | ~10 |

#### 07-kmain-finish-switch.md（T11→T12）

**核心问题**：kmain 的收尾工作——system_init 初始化特权表，add_memmap 回收 bootstrap 内存，bsp_finish_booting 切换到用户态开始调度。

| 章节 | 内容 | 行数 |
|------|------|------|
| Ch1 概述 | kmain最后四步；switch_to_user的含义——内核"退场"，让进程上场 | ~60 |
| Ch2 C源码 | main.c:285-324 | ~120 |
| §2.1 | system_init()：特权表初始化 | |
| §2.2 | add_memmap(bootstrap)：回收bootstrap区物理内存 | |
| §2.3 | bsp_finish_booting()：announce → 清RTS_PROC_STOP → switch_to_user | |
| Ch3 Rust设计 | | ~100 |
| Ch4 实现 | | ~90 |
| Ch5 测试 | | ~20 |
| Ch6 参见 | 引用01-06 | ~10 |

#### 08-vm-boot-setaddr.md（T13→T14）

**核心问题**：VM 是第一个运行的用户态进程。它初始化自身页表后，通过 VMCTL_SETADDRSPACE 让内核切换 CR3——从此内核运行在 VM 的页表上。

| 章节 | 内容 | 行数 |
|------|------|------|
| Ch1 概述 | VM第一个运行的必然性；内核-VM协议的起点 | ~60 |
| Ch2 C源码 | protect.c (arch_do_vmctl) + VM侧源码 | ~150 |
| §2.1 | VM初始化自身页表 | |
| §2.2 | SYS_VMCTL(VMCTL_SETADDRSPACE)：arch_do_vmctl → setcr3 → write_cr3 → switch_address_space | |
| §2.3 | 地址空间切换的后果：video_mem从物理地址切到虚拟地址 | |
| Ch3 Rust设计 | VmctlRequest enum、地址空间切换的trait抽象 | ~100 |
| Ch4 实现 | | ~100 |
| Ch5 测试 | | ~20 |
| Ch6 参见 | 引用01-07 | ~10 |

#### 09-vm-boot-physmap.md（T15→T16）

**核心问题**：内核声明需要映射的物理区域（video_mem、APIC），VM 回复映射结果，然后解除其他进程的 VMINHIBIT。

| 章节 | 内容 | 行数 |
|------|------|------|
| Ch1 概述 | 为什么内核需要VM映射物理区域；VMINHIBIT解除的时序 | ~60 |
| Ch2 C源码 | arch_phys_map + arch_phys_map_reply + VMINHIBIT_CLEAR | ~150 |
| §2.1 | VMCTL_KERN_PHYSMAP：arch_phys_map()声明物理区域 | |
| §2.2 | VMCTL_KERN_MAP_REPLY：arch_phys_map_reply()获得虚拟地址 | |
| §2.3 | VMCTL_VMINHIBIT_CLEAR：VM为其他进程创建页表后解除阻塞 | |
| §2.4 | vm_running=1：系统完全就绪 | |
| Ch3 Rust设计 | PhysMapRequest/PhysMapReply结构化、协议状态机 | ~100 |
| Ch4 实现 | | ~100 |
| Ch5 测试 | | ~20 |
| Ch6 参见 | 引用01-08 | ~10 |

#### 10-runtime-cross-space.md（T17→T18）

**核心问题**：VM running 后，内核需要访问用户空间（拷贝数据、查页表）。createpde/lin_lin_copy/vm_memset/vm_lookup 是运行时跨地址空间的核心机制。

| 章节 | 内容 | 行数 |
|------|------|------|
| Ch1 概述 | 前置条件（freepdes来自06的memory_init、ptproc来自06的arch_post_init、VM running来自09）；为什么内核不能直接访问用户空间 | ~80 |
| Ch2 C源码 | memory.c (createpde) + system.c (cross_space_copy/memset/lookup) | ~150 |
| §2.1 | createpde()：用freepdes临时映射目标进程的页目录项 | |
| §2.2 | lin_lin_copy()：跨地址空间内存拷贝 | |
| §2.3 | vm_memset()/vm_lookup()：跨地址空间清零/查询 | |
| §2.4 | VMSUSPEND：进程等待VM完成页表操作 | |
| Ch3 Rust设计 | CrossSpaceCopy trait、VMSUSPEND状态表达 | ~100 |
| Ch4 实现 | | ~100 |
| Ch5 测试 | | ~20 |
| Ch6 参见 | 引用01-09 | ~10 |

### 7.5 01 文档修改

在 §6 参见之前增加过渡节（~30行）：

```markdown
## 5.5 从 pre_init 返回后

pre_init() 返回 &kinfo 后，系统处于"分页已开启但 RIP/RSP 在低地址"的中间态。
Minix3 C 的 head.S 用三行汇编完成切栈+跳高地址，Rust 版通过 trampoline 实现。
→ 详见 02-higher-half-kernel.md
```

同时修正 §1.7.1 表格中"模块加载"行（见 §2 TODO）。

### 7.6 时间线完整性验证

| 时间点 | 事件 | 文档 | 无遗漏? |
|--------|------|------|---------|
| T0 | vm_enable_paging() 返回 | 01 | ✅ |
| T1 | head.S 切栈+跳高地址 | 02 | ✅ |
| T2 | kmain() 被调用 | 02 | ✅ |
| T3 | memcpy + BSS + kernel_may_alloc | 03 | ✅ |
| T4 | cstart() → prot_init | 03 | ✅ |
| T5 | init_clock | 04 | ✅ |
| T6 | intr_init + arch_init | 04 | ✅ |
| T7 | BKL + proc_init | 05 | ✅ |
| T8 | arch_boot_proc 循环 | 05 | ✅ |
| T9 | arch_post_init | 06 | ✅ |
| T10 | memory_init | 06 | ✅ |
| T11 | system_init + add_memmap | 07 | ✅ |
| T12 | bsp_finish_booting → switch_to_user | 07 | ✅ |
| T13 | VM 初始化自身 | 08 | ✅ |
| T14 | VMCTL_SETADDRSPACE | 08 | ✅ |
| T15 | VMCTL_KERN_PHYSMAP + MAP_REPLY | 09 | ✅ |
| T16 | VMCTL_VMINHIBIT_CLEAR → vm_running=1 | 09 | ✅ |
| T17 | createpde/lin_lin_copy/... | 10 | ✅ |
| T18 | VMSUSPEND | 10 | ✅ |

### 7.7 引用关系验证（只前向，无后向，无 tmp_*）

| 文档 | 引用 |
|------|------|
| 02 | 01 |
| 03 | 01, 02 |
| 04 | 01, 02, 03 |
| 05 | 01-04 |
| 06 | 01-05 |
| 07 | 01-06 |
| 08 | 01-07 |
| 09 | 01-08 |
| 10 | 01-09 |

### 7.8 与前版方案（5篇）的对比

| 维度 | 前版（5篇） | 本版（9篇） |
|------|-----------|-----------|
| 文档数 | 5 | 9 |
| 平均行数 | ~560 | ~455 |
| 最大文档 | 03-kmain-boot ~800行 | 03/05 ~550行 |
| cstart覆盖 | 合并在kmain-boot中 | 拆为03(prot_init)+04(clock/intr) |
| VM协商 | 合并在04-vm-boot-protocol | 拆为08(SETADDRSPACE)+09(PHYSMAP/VMINHIBIT) |
| post_init+memory | 合并在kmain-boot中 | 独立为06 |
| kmain-finish | 合并在kmain-boot中 | 独立为07 |
| 时间线保证 | 依赖文档内叙事 | 每篇文档=一个连续时间段 |