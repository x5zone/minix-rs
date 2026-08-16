# 11-input-driver-connect: 驱动注册与生命周期

> **状态**: pending（最小骨架，待改写）
> **定位**: DS 订阅 → input_check → connect/alloc/disconnect（阶段 5 驱动生命周期）
> **源码**: `minix3/minix/servers/input/input.c:430-644`（input_alloc_id/input_connect/input_disconnect/input_check/input_other）+ `minix3/minix/include/minix/ds.h` 使用面
> **Rust 模块**: `connect.rs`
> **draft 素材**: `../../../../tmp/input/tmp_input.c.md`（素材）

## 核心点

- `input_check`（:558-606）：`ds_check` 轮询新增项（`drv.inp.` 前缀过滤 + `ds_retrieve_u32` 取 typemask）；逐设备 `ds_retrieve_label_endpt` 检测移除（ESRCH → `input_disconnect`，OK → 更新 owner）
- `input_connect`（:475-531）：`ds_retrieve_label_name` label 校验（不匹配忽略）；`input_alloc_id`（kbd/mouse）；CONF 回复（asynsend，分配失败仍发 INVALID 值 → 驱动静默禁用）；kbd 注册后恢复 LED（锚 10）
- `input_alloc_id`（:430-473）：槽位范围（FIRST..LAST）、同 label 复用并更新 owner、**不占用已断开但打开的槽**（`!opened` 才可分配）、无槽 → INVALID_INPUT_ID
- `input_disconnect`（:533-556）：挂起读 → `chardriver_reply_task` EIO（锚 07）；selector → `chardriver_reply_select` CDEV_OP_RD（锚 08）；`owner=NONE`
- DS 订阅契约：`ds_subscribe("drv\\.inp\\..*", DSF_INITIAL)`（:665）+ libinputdriver 发布面（锚 12）；`input_other` 的 DS notify 分支
- ARCH A-2（DS 使用面契约，DS server 内部属 07-stage-ds）/A-6（LED 恢复）

## 边界

- **前置依赖**: 03 + 05 + 07/08（唤醒面）+ 12（发布面）
- **不覆盖（移交）**: DS server 内部（07-stage-ds）、驱动侧实现（12/14）
