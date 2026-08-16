# 10-ds-subscribe-check: 订阅/通知机制（次主线核心）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 数据面 handler（次主线路径图所在篇）
> **源码**: `minix3/minix/servers/ds/store.c:456-581,186-227`
> **Rust 模块**: `subscribe.rs`、`check.rs`、`notify.rs`
> **draft 素材**: `draft/tmp_store.c.md`（逐行素材）

## 核心点

- 次主线路径图：publish → `update_subscribers(dsp,1)` → notify → `do_check` → retrieve → delete
- `do_subscribe`：owner → 已有订阅 EEXIST / OVERWRITE 释放 → `^...$` 锚定 + `REG_EXTENDED` → type_set 掩码 → `DSF_INITIAL` 即时扫描 + notify
- `do_check`：首置位扫描（ENOENT）→ key 拷贝 → 回复 type/owner → `UNSET_BIT`
- `update_subscribers`/`check_sub_match`：类型匹配 + auth + `regexec` + SET/UNSET_BIT + `ipc_notify`
- A-2：正则引擎决策（no_std 无 POSIX regex）
- 测试契约：`tests/ds/subs.c`（订阅者 notify 循环 + ds_check/retrieve）

## 边界

- **前置依赖**: 02/03/04/05
- **不覆盖（移交）**: 客户端 `ds_subscribe`/`ds_check`（12）
