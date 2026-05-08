# 03-todo: Review 修复记录

## 审查文件
`03-acl.md`

## 审查结果

### P1 修复

#### 1. 失效交叉引用：acl-todo.md

**问题**: §6 "参见"中引用了 `[acl-todo.md](acl-todo.md) - ACL 重构设计文档`，但该文件在仓库中不存在。这是上一轮 review 遗留的失效引用。

**修复**: 删除该引用。保留的两个引用（`01-vmproc-struct.md` 和 `17-vm-fork.md`）均有效。

### P2 修复

#### 2. VM_PAGEFAULT 描述不精确

**问题**: §3.3 中"VM_PAGEFAULT 不在 AclMask 中：其偏移量 0xFF = 255 超出 `u64` 范围"——"超出 u64 范围"容易被误解为"255 超出 u64 的值域"（u64 当然能表示 255），实际含义是"偏移量 255 超出 u64 的位宽"。

**修复**: 改为"其偏移量 0xFF = 255 超出 u64 的 64 位宽度（`1 << 255` 无法用 u64 表示）"。

### 源码行号验证

| 引用 | 文档标注 | 实际位置 | 一致? |
|------|----------|----------|-------|
| `acl.c:21` acl_init | L21 | L21=函数签名 | ✅ |
| `acl.c:37` acl_check | L37 | L37=函数签名 | ✅ |
| `acl.c:70` acl_set | L70 | L70=函数签名 | ✅ |
| `acl.c:110` acl_fork | L110 | L110=函数签名 | ✅ |
| `acl.c:120-128` acl_clear | L120-128 | L120=函数签名, L128=闭合花括号 | ✅ |
| `com.h:627` VM_RQ_BASE | L627 | L627=`#define VM_RQ_BASE 0xC00` | ✅ |

### Rust 代码审查

Rust 代码与文档设计一致，无需修改：

- `AclState` enum（Uninitialized/Default/System）与 §3.2 一致
- `AclMask` bitflags 30 个常量与 §3.3 一致
- `AclMask::DEFAULT` 定义与 §3.3 一致
- `acl_check()` 行为与 §3.5 一致（含 VM 自身检查、Uninitialized 放行、Default/System 位图检查）
- `acl_set()` 纯函数设计与 §3.6 一致
- `acl_fork()` 继承规则与 §3.7 一致
- `acl_clear()` 简化设计（无需释放槽位）与 §3.8 一致
- `mask()` 方法与 §3.3 一致
- 测试覆盖 §5 中所有测试要点，且额外包含 `test_acl_set_user/system` 和 `test_acl_clear`

### 上一轮 review 修复验证

上一轮 03-todo.md 记录了 3 个修复：
1. ✅ P0: §2.1 已移除 Rust 代码块，替换为交叉引用
2. ✅ P1: §3.3 AclMask 已列出全部 30 个常量
3. ✅ P1: §3.3 DEFAULT 常量已使用 `Self::VM_EXIT.bits() | ...` 语法

### 未修复项（P2，记录备查）

1. 文档 §2.3.2 的"ACL 检查的唯一位置"代码块是伪代码（简化了 main.c 的消息循环），这是合理的简化
2. 文档 §2.4.1 的 `do_fork` 代码块也是伪代码（简化了 fork.c 的实际流程），这是合理的简化
3. Rust 代码中 `acl_check` 的 TODO 注释与文档略有不同（代码更详细地说明了 no_std 环境限制），行为一致，无需修改
