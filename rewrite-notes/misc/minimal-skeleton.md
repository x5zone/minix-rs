# 最小可运行骨架设计

> **目标**: 能编译 → 能打包 rootfs → 能用 QEMU 启动 → 能跑一个 user 程序

---

## 一、整体目标

```
QEMU
 └── kernel (Rust)
      ├── basic scheduler（甚至可以没有）
      ├── simple syscall（write）
      └── init process
             └── /bin/hello (用户程序)
```

**第一阶段简化**：
- 不做 VFS
- 不做 driver
- 用 **initramfs / 内存打包 rootfs**

---

## 二、项目结构

```
minix-rs/
├── Cargo.toml              # workspace
├── xtask/                  # 构建系统（核心）
│   └── src/main.rs
│
├── crates/
│   ├── arch/
│   ├── ipc/
│   └── syscalls/
│
├── kernel/
│   ├── Cargo.toml
│   └── src/main.rs
│
├── user/
│   └── hello/
│       ├── Cargo.toml
│       └── src/main.rs
│
├── build/
│   ├── rootfs/
│   └── image/
│
├── scripts/
│   └── linker.ld
│
└── Makefile（可选）
```

---

## 三、Cargo Workspace

```toml
[workspace]
members = [
    "kernel",
    "user/hello",
    "xtask",
]

resolver = "2"
```

---

## 四、Kernel 最小版本

### kernel/Cargo.toml

```toml
[package]
name = "kernel"
version = "0.1.0"
edition = "2021"

[dependencies]

[profile.dev]
panic = "abort"

[profile.release]
panic = "abort"
```

### kernel/src/main.rs

```rust
#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
```

> 先跑起来再说（后面再加 syscall / IPC）

---

## 五、User 程序

### user/hello/Cargo.toml

```toml
[package]
name = "hello"
version = "0.1.0"
edition = "2021"

[dependencies]

[profile.dev]
panic = "abort"

[profile.release]
panic = "abort"
```

### user/hello/src/main.rs

```rust
fn main() {
    println!("Hello from userland!");
}
```

> 第一阶段可以直接用 std（跑在宿主机测试）
> 后面再切 no_std + syscall

---

## 六、xtask（构建系统核心）

### xtask/Cargo.toml

```toml
[package]
name = "xtask"
version = "0.1.0"
edition = "2021"
```

### xtask/src/main.rs

```rust
use std::process::Command;

fn main() {
    let cmd = std::env::args().nth(1);

    match cmd.as_deref() {
        Some("build") => build(),
        Some("rootfs") => rootfs(),
        Some("run") => run(),
        _ => {
            println!("usage: xtask [build|rootfs|run]");
        }
    }
}

fn run_cmd(cmd: &str, args: &[&str]) {
    println!("> {} {:?}", cmd, args);
    let status = Command::new(cmd)
        .args(args)
        .status()
        .expect("failed to run command");

    if !status.success() {
        panic!("command failed");
    }
}

fn build() {
    run_cmd("cargo", &["build", "-p", "kernel"]);
    run_cmd("cargo", &["build", "-p", "hello"]);
}

fn rootfs() {
    std::fs::create_dir_all("build/rootfs/bin").unwrap();

    // copy user program
    run_cmd(
        "cp",
        &[
            "target/debug/hello",
            "build/rootfs/bin/hello",
        ],
    );
}

fn run() {
    // 假设 kernel 是裸 bin
    run_cmd(
        "qemu-system-x86_64",
        &[
            "-kernel",
            "target/debug/kernel",
            "-serial",
            "stdio",
        ],
    );
}
```

---

## 七、运行流程

```bash
# 1. 编译
cargo run -p xtask build

# 2. 准备 rootfs
cargo run -p xtask rootfs

# 3. 启动
cargo run -p xtask run
```

---

## 八、演进路线图

### Step 1: 让 kernel 有输出

```rust
// 写到 0x3F8（串口）
```

### Step 2: 真正执行 user 程序

```
kernel:
  load ELF
  创建用户态
  跳转执行
```

### Step 3: 加 syscall

```
user → kernel:
  write()
```

### Step 4: 加 IPC

```
send / receive
```

### Step 5: 再考虑 grant

> 到这一步再回来看 grant，会非常清晰

---

## 九、正确节奏

### ✔ 正确

```
能跑 → 能打印 → 能 syscall → 能 IPC → 再谈设计
```

### ❌ 错误

```
一开始就：
调度 + IPC + grant + driver + SMP
```

> 这样一定卡死

---

## 十、骨架的意义

### 1. 解决工程问题

- workspace ✓
- xtask ✓
- rootfs ✓

### 2. 系统闭环

```
build → image → run
```

### 3. 可扩展性

之后可以自然加：
- drivers/
- servers/
- game/
- shell/

---

## 十一、关键洞察

你现在做的不是"写一个 OS"，而是：

> **构建一个可以持续演进的 OS 实验平台**

这个骨架就是你的"地基"。

---

## 附录：下一步升级

**让 kernel 真正加载并运行 user ELF（最关键的一步）**

这个一旦打通，整个系统就"活了"。
