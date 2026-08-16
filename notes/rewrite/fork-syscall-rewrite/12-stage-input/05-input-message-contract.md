# 05-input-message-contract: 消息协议面

> **状态**: pending（最小骨架，待改写）
> **定位**: INPUT/TTY 消息常量、消息结构、全单向协议（阶段 2 协议面）
> **源码**: `minix3/minix/include/minix/com.h:877-893`、`ipc.h:232-259,990-1000,2434-2436,2517`、`dmap.h:78`、`MAKEDEV.sh:330-343`、`system.conf:400-403`
> **Rust 模块**: `minix-types`（`ipc/input.rs`）
> **draft 素材**: 无

## 核心点

- 消息常量：TTY_INPUT_UP/TTY_INPUT_EVENT（TTY_RQ_BASE+2/+3）、INPUT_CONF/INPUT_SETLEDS（INPUT_RQ_BASE 0x1500）、INPUT_EVENT（INPUT_RS_BASE 0x1580）（com.h:879-893）
- 四个消息结构：`mess_input_linputdriver_input_conf`（kbd/mouse/rsvd1/rsvd2 id）、`mess_input_linputdriver_setleds`（led_mask）、`mess_input_tty_event`、`mess_linputdriver_input_event`（id/page/code/value/flags）（ipc.h）
- **全单向协议**（com.h:886 明言无回复，A-9）；server 侧 asynsend、驱动侧阻塞 ipc_send
- `INPUT_MAJOR=64`（dmap.h:78）+ `/dev` 节点表：kbdmux(64,0)/kbd0-3(64,1-4)/mousemux(64,64)/mouse0-3(64,65-68)（MAKEDEV.sh:330-343）
- `system.conf:400-403`：service input 权限（ipc SYSTEM pm vfs rs ds tty vm; priority 1）
- `input_tab` 注册面（锚 01）+ `input_other` 消息面入口（A-10：DS notify/INPUT_EVENT/INPUT_SETLEDS 来源过滤）

## 边界

- **前置依赖**: 01 + 03（设备语义）+ 04（事件字段）
- **不覆盖（移交）**: handler 业务（06~10）、驱动生命周期（11）、TTY 消费（13）
