# 12-init-sysctl-interaction：安全级别与新根

> **定位**：`has_securelevel`（`minix3/sbin/init/init.c:544-563`）、`getsecuritylevel`（568-589）、`setsecuritylevel`（594-618）、`createsysctlnode`（1811-1857）、`shouldchroot`（1859-1900)。**[ARCH A-4]** securelevel 缺口 defer，**[ARCH A-5]** init.root 缺口 defer。
> **Rust**：`os/commands/sbin/init/src/sysctl.rs`。
> **前置依赖**：01（探测调用点）、04/05/09（调用方）。
> **本篇不覆盖（移交）**：内核 sysctl 实现（见 `../01-stage-kernel/` 对应文档）。

---

## 1. 概念：运行时可调的两个旋钮

init 有两个问内核要的运行时旋钮。安全级别是系统的写保护档位：单用户降到 0（随便修），多用户升到 1（关键文件锁死）。`has_securelevel` 先问内核支不支持，不支持后面一切免谈，这种能力探测避免了在老内核上硬调失败。`init.root` 是第二个根的位置：rc 脚本可能在新根里，读出来不是 `/` 就 chroot 进去跑第二遍。节点被子进程删了就重建，读出来不是字符串就拒绝，处处是防御性编程。

minix-rs 内核暂无这两个语义，本篇定义 trait 契约并用内存假实现测试，真实接线 defer。这是诚实的缺口，不是翻译。

### 1.1 小结

一档写保护，一个新根地址。下一章看会话日志。

---

## 2. C 源码分析

| 函数 | 行号 | 要点 |
|---|---|---|
| `has_securelevel` | 544-563 | sysctl 试探，ENOENT 返回 0，否则 1；无 KERN_SECURELVL 返回 0 |
| `getsecuritylevel` | 568-589 | 不支持返回 -1；失败记 emergency 返回 -1 |
| `setsecuritylevel` | 594-618 | 不支持或同值返回；失败记 emergency；SECURE 下记 warning |
| `createsysctlnode` | 1811-1857 | 建 init 节点再建 root 字符串节点，默认 `/`，失败 warning 返回 -1 |
| `shouldchroot` | 1859-1900 | 读失败 ENOENT 则重建后返回 0；非字符串返回 0；`/` 返回 0，否则 1 |

---

## 3. Rust 设计决策

`SecureLevel` trait 加内存假实现；`should_chroot(root)` 纯函数：`/` 或空为假，其余为真。Live sysctl 待内核服务。

---

## 4. 实现详解

模块 `sysctl.rs`；差异：sysctl 调用收敛为 trait，字符串校验保留（NUL 与长度双检语义简化为 Rust 字符串非空判断，单测锁定）。

---

## 5. 测试要点

| 测试 | C 对照 |
|---|---|
| `test_absent_returns_minus_one` | init.c:575-576 |
| `test_set_same_noop` | init.c:603-605 |
| `test_single_user_downgrades` | init.c:723-725 语义 |
| `test_should_chroot_matrix` | init.c:1896-1899 |
| `test_root_slash_no_chroot` | init.c:1896-1897 |

### 5.1 测试统计（截至 2026-09-04）

- `cargo test -p minix-init`：76 个通过（累计），0 失败。
- 清单：`rg "fn test_" os/commands/sbin/init/src/sysctl.rs`。

---

## 6. 过渡

交互契约已定。下一站 13 会话日志。

---

## 7. 参见

- `04-init-single-user.md`、`05-init-runcom.md`、`09-init-multi-user.md` — 调用方。
- C 源码：`minix3/sbin/init/init.c:544-618,1811-1900`。
