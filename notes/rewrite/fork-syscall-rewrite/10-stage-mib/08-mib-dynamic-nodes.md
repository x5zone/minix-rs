# 08-mib-dynamic-nodes: 动态节点生命周期

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 动态节点（create/destroy 元标识符语义）
> **源码**: `tree.c:242-360,426-481,483-917`
> **Rust 模块**: `tree/dynamic.rs`、`tree/version.rs`
> **draft 素材**: 无（新建）

## 核心点

- `mib_check_name`：C 符号风格（字母/_ 开头 + 数字），非空且 NUL 终止
- `mib_scan`：静态数组 + 动态链表双扫描（id/name 冲突 → EEXIST 并 copyout 现有节点）、自由 id 分配（≥ `CREATE_BASE=1024`，静态区外）
- `mib_create` 校验序列：auth → 父 RW → 子节点上限（INT_MAX）→ sysctlnode 拷入 → 版本校验 → flags 白名单（`SYSCTL_USERFLAGS|UNSIGNED`）→ OWNDATA/IMMEDIATE 互斥 → 类型/大小校验 → func/parent 字段拒绝 → 名称校验 → scan → 单块分配
- `mib_add`/`mib_remove`：链表插入/摘除 + 计数（mib_nodes/objects）+ `mib_upgrade` 版本递增（根→路径全链）
- `mib_destroy` 规则：PERMANENT→EPERM、REMOTE→EBUSY、函数节点→EPERM、非空子节点→ENOTEMPTY、版本/名称匹配校验
- 内存所有权：dynode 单块（name+data 内嵌）、OWNDESC `strdup`、分配失败返回 EINVAL（不返回 ENOMEM，A-3/A-11）

## 边界

- **前置依赖**: 03/05/06/07
- **不覆盖（移交）**: 数据读写（09）、枚举（11）
