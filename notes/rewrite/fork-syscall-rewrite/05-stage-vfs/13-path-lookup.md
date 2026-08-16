# 13-path-lookup: 路径解析

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 6 — 路径解析：所有名字类调用的前置
> **源码**: `path.c` 全文件、`path.h`、`utility.c:24-93`（copy_path/fetch_name）
> **Rust 模块**: （未实现）path 模块
> **draft 素材**: 无（新建）

## 核心点

- lookup/advance/eat_path/last_dir：路径名 → vnode 解析主链
- 挂载点穿越（find_vmnt/m_mounted_on）、符号链接循环（SYMLOOP=16）
- lookup_init/lookup 结构与 l_vnode_lock/l_vmnt_lock 锁请求
- get_name/canonical_path、fetch_name/copy_path（用户路径拷贝）
- DO_POSIX_PATHNAME_RES=0 历史行为（A-10）

## 边界

- open 的后续处理不覆盖（15）
- exec 脚本解释不覆盖（25）
