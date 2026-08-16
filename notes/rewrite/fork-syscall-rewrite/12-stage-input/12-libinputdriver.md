# 12-libinputdriver: 输入驱动客户端库

> **状态**: pending（最小骨架，待改写）
> **定位**: libinputdriver 全部 7 函数（阶段 6 客户端库）
> **源码**: `minix3/minix/lib/libinputdriver/inputdriver.c`（206 行，全部）+ `minix3/minix/include/minix/inputdriver.h`
> **Rust 模块**: `minix-sys/inputdriver.rs`
> **draft 素材**: `../../../../tmp/input/tmp_input.c.md`（素材）

## 核心点

- `inputdriver_announce`（:20-39）：`ds_retrieve_label_name` 取自身 label → `ds_publish_u32("drv.inp."+label, typemask, DSF_OVERWRITE)`（A-2）
- `inputdriver_send_event`（:42-79）：`input_endpt==NONE`/id==INVALID → 丢弃；阻塞 `ipc_send(INPUT_EVENT)`，失败 → 重置 `input_endpt=NONE`（崩溃检测 + 防堆积，A-9）
- `do_conf`（:82-117）：`ds_retrieve_label_endpt("input")` 校验发送者；保存 input_endpt/kbd_id/mouse_id；双 INVALID → "driver disabled" 日志
- `do_setleds`（:119-138）：source 校验 + `idr_leds(mask)` 回调
- `inputdriver_process`（:141-174）：notify 分发（HARDWARE→idr_intr、CLOCK→idr_alarm、其他→idr_other）+ 消息分发（INPUT_CONF/INPUT_SETLEDS/default→idr_other）
- `inputdriver_task`（:188-206）/`inputdriver_terminate`（:177-186）：`sef_receive_status(ANY)` 主循环 + sef_cancel 退出
- `struct inputdriver` 回调表（inputdriver.h：idr_leds/idr_intr/idr_alarm/idr_other）

## 边界

- **前置依赖**: 05（消息面）+ 99
- **不覆盖（移交）**: server 内部（01~11）、pckbd 硬件细节（14）
