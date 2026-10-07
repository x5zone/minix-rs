# ELF 加载与用户态执行

> **核心问题**: kernel 如何真正加载并运行 user ELF？

这是从"玩具内核"迈向"真正操作系统"的分水岭。

---

## 一、问题拆解

要做的其实是 4 件事：

```
1. 从 rootfs 读取 ELF 文件
2. 解析 ELF（program headers）
3. 建立用户地址空间（VM）
4. 切换到用户态执行
```

对应到结构：

```
kernel
 ├── fs        (读取 ELF)
 ├── loader    (解析 ELF)
 ├── vm        (映射内存)
 └── arch      (切换到 user mode)
```

---

## 二、最小可运行架构

### Crate 结构

```
crates/
  kernel/
  vm/
  ipc/
  elf/        👈 专门解析 ELF（强烈建议独立）
  arch-x86_64/
  syscalls/

user/
  init/
  sh/
```

### 核心原则

- ELF parsing → **纯库（no_std）**
- VM → 独立 crate
- arch → 只做硬件相关

---

## 三、ELF Loader

### ELF Crate 结构

最小只需要支持：
- ELF64
- PT_LOAD segment

```rust
pub struct Elf<'a> {
    data: &'a [u8],
}

pub struct ProgramHeader {
    pub vaddr: u64,
    pub memsz: u64,
    pub filesz: u64,
    pub offset: u64,
    pub flags: u32,
}
```

### Kernel 中的 Loader

```rust
pub fn load_elf(proc: &mut Process, elf_data: &[u8]) -> Result<()> {
    let elf = Elf::parse(elf_data)?;

    for ph in elf.program_headers() {
        if ph.is_load() {
            map_segment(proc, &ph, elf_data)?;
        }
    }

    proc.entry = elf.entry_point();
    Ok(())
}
```

### Map Segment

```rust
fn map_segment(proc: &mut Process, ph: &ProgramHeader, elf: &[u8]) {
    let start = align_down(ph.vaddr);
    let end   = align_up(ph.vaddr + ph.memsz);

    vm_map(
        proc,
        start,
        end,
        flags_to_perm(ph.flags),
    );

    // copy file part
    let src = &elf[ph.offset..ph.offset + ph.filesz];
    let dst = proc.translate(ph.vaddr);

    copy(dst, src);

    // zero bss
    zero(dst + ph.filesz, ph.memsz - ph.filesz);
}
```

> 这里已经是"正统 OS loader 逻辑"

---

## 四、地址空间设计

```
用户地址空间：
┌──────────────────────┐
│ user stack           │  👈 高地址
├──────────────────────┤
│ heap (brk)           │
├──────────────────────┤
│ mmap area            │
├──────────────────────┤
│ ELF segments         │
│  text / data / bss   │
├──────────────────────┤
│ guard page           │
└──────────────────────┘ 0x0
```

### 最小实现

```
固定布局：
0x400000  → ELF
0x800000  → stack
```

---

## 五、创建用户栈

### Linux 风格 Stack Layout

```
+------------------+
| argc             |
| argv[0]          |
| argv[1]          |
| NULL             |
| envp             |
| NULL             |
| auxv             |
| ...              |
| strings          |
+------------------+
```

### 最小版本

```rust
push(stack, "init\0");
push(stack, argv_ptr);
push(stack, argc);
```

---

## 六、切换到用户态

### 方式：iretq 或 sysret

最简单（推荐）：`iretq`

### 构造 Trap Frame

```rust
struct TrapFrame {
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
}
```

### 跳转代码

```rust
fn enter_user(entry: u64, stack: u64) -> ! {
    unsafe {
        asm!(
            "mov rsp, {stack}",
            "push {ss}",
            "push {rsp}",
            "push {rflags}",
            "push {cs}",
            "push {rip}",
            "iretq",
            stack = in(reg) stack,
            rip   = in(reg) entry,
            ...
        );
    }
}
```

---

## 七、完整流程

```
kernel_main
   ↓
init_process()
   ↓
read /bin/init  (从 rootfs)
   ↓
load_elf()
   ↓
setup_stack()
   ↓
switch_to_user()
   ↓
🎉 用户程序开始执行
```

---

## 八、rootfs + init

### rootfs 结构

```
rootfs/
  bin/
    init
```

### init 程序（Rust）

```rust
#![no_std]
#![no_main]

#[no_mangle]
fn _start() -> ! {
    loop {}
}
```

编译：

```bash
cargo build --target x86_64-unknown-none
```

---

## 九、QEMU 启动链

```
bootloader → kernel → load ELF → user init
```

---

## 十、常见坑

### 1. ELF 对齐问题

必须 page align

### 2. 用户栈没对齐

ABI 要求 16-byte

### 3. Page Fault

可能是：
- 没 map user memory
- 权限错（U/S bit）

### 4. 忘了设置

```
CR3
CR0.WP
CR4.SMEP / SMAP（先别开）
```

---

## 十一、系统本质模型

```
Linux = fork + exec
Minix = server + IPC
你   = ELF loader + per-process VM
```

**本质**：你正在实现一个"用户态程序执行引擎"

---

## 十二、演进路线

### Phase 1（当前）

- 单进程
- 加载 `/bin/init`
- 进入用户态

### Phase 2

- syscall（write / exit）
- simple scheduler

### Phase 3

- 多进程
- fork / exec

### Phase 4

- IPC / grant

---

## 十三、关键认知

OS 的本质不是调度，而是：

> **"安全地让不同地址空间的程序运行"**

而 **ELF loader + VM + user switch** 就是这件事的核心闭环。

---

## 附录：下一步

1. **最小 init ELF + kernel glue 代码**（能在 QEMU 跑起来）
2. **syscall ABI（Rust 风格）**

这两步会让 minix-rs 从"结构设计"跃迁到"能跑程序"。
