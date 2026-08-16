# 09-input-event-processing: 事件处理与唤醒

> **状态**: pending（最小骨架，待改写）
> **定位**: INPUT_EVENT 消费链：input_event → input_process（阶段 4 事件机制核心）
> **源码**: `minix3/minix/servers/input/input.c:332-428`（input_process/input_event）
> **Rust 模块**: `event.rs`
> **draft 素材**: `../../../../tmp/input/tmp_input.c.md`（素材）

## 核心点

- `input_event`（:376-428）：id 边界（0..INPUT_DEV_MAX-1，越界静默丢弃，id==数组下标 A-3）；owner 校验（`m_source == devs[id].owner`，不匹配丢弃）；mux 选择（kbd minor → KBDMUX_DEV，否则 MOUSEMUX_DEV，:379-407）
- 分发优先级：设备 opened → `input_process(dev)`；否则 mux opened → `input_process(mux)`；否则转发 TTY（`TTY_INPUT_EVENT` + id/page/code/value/flags，阻塞 `ipc_send(TTY_PROC_NR)`，锚 13）
- `input_process`（:332-374）：溢出覆盖最旧（tail+1, count--，A-4）；enqueue（page/code/value/flags/devid=id/rsvd=0）；唤醒挂起 reader（拷贝 1 事件 + `chardriver_reply_task`，锚 07）或唤醒 selector（`chardriver_reply_select` CDEV_OP_RD，锚 08）
- ARCH A-3/A-4/A-9（单向协议）

## 边界

- **前置依赖**: 03 + 04 + 05
- **不覆盖（移交）**: LED 广播（10）、驱动连接（11）、TTY 消费（13）
