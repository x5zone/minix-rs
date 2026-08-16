# 10-libdevman-client: 客户端库契约

> **状态**: pending（最小骨架，待改写）
> **定位**: 设备驱动的注册客户端（阶段 5 客户端契约）
> **源码**: `minix3/minix/lib/libdevman/generic.c`（275 行）+ `local.h`
> **Rust 模块**: `minix-sys/devman_client.rs`
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- `serialize_dev`/`save_string`（:36/:24）：offset 布局序列化（A-4）
- `devman_add_device`（:102）：grant（cpf_grant_direct CPF_READ）→ ADD_DEV sendrec → dev_id 回写 → dev_list
- `devman_del_device`（:154）：DEL_DEV sendrec → dev_list 移除
- `devman_init`（:188）：ds_retrieve_label_endpt("devman") + dev_list 初始化
- `devman_handle_msg`（:258）：m_source == devman_ep 校验 → do_bind/do_unbind（:207/:233）→ bind_cb/unbind_cb（DEVMAN_ENDPOINT 入参）→ reply
- panic 语义：失败即 panic（A-7 讨论）

## 边界

- **前置依赖**: 05（消息面）+ 99
- **不覆盖（移交）**: USB 高层建模（11）、驱动侧调用方（usbd）
