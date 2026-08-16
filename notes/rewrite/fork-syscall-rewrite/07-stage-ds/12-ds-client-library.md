# 12-ds-client-library: 客户端协议契约（libsys/ds.c）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 6 客户端契约（协议闭环）
> **源码**: `minix3/minix/lib/libsys/ds.c`（219 行）、`ds.h:40-68`
> **Rust 模块**: `minix-sys`（DS 客户端模块，A-8）
> **draft 素材**: 无（新建）

## 核心点

- `do_invoke_ds`：key grant 生命周期（CHECK/RETRIEVE_LABEL 写 grant 80B，其余读 grant strlen+1）+ `_taskcall` + `cpf_revoke`
- 全部客户端函数：publish_u32/str/mem/label、retrieve_*、delete_*、subscribe、check（消息字段组装 + 回复字段解析）
- 客户端 quirks：`ds_publish_str`/`ds_retrieve_str` 的 NUL 收尾
- `DS_DRIVER_UP` 事件语义（驱动库发布/订阅）
- A-7：死 map/snapshot API 排除契约
- 测试契约：`tests/ds/dstest.c` + `subs.c` 全量行为（与服务器 handler 篇对账）

## 边界

- **前置依赖**: 02 + 服务器 handler 篇（07~11）
- **不覆盖（移交）**: 服务器内部实现（03~11）
