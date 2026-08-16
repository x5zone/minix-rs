# 02-is-fkey-contract: 功能键协议（TTY ↔ IS）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 2 协议面（init 锚点 map_unmap_fkeys 的数据流前提）
> **源码**: `com.h:874-877`、`ipc.h:1453,1930`、`keymap.h:93,104,135,146`、`sysutil.h:43-46`、`libsys/fkey_ctl.c`、`drivers/tty/tty/arch/i386/keyboard.c:401-585`
> **Rust 模块**: `tty_fkey.rs`、`minix-types`（**缺 TtyFkeyCtlReq/Reply，A-1**）
> **draft 素材**: `draft/tmp_dmp.c.md`（map_unmap_fkeys 素材）

## 核心点

- `TTY_FKEY_CONTROL`（com.h:874）+ `FKEY_MAP/UNMAP/EVENTS`（com.h:875-877）三命令语义
- 消息格式：`mess_lsys_tty_fkey_ctl`（request/fkeys/sfkeys，ipc.h:1453）、`mess_tty_lsys_fkey_ctl`（回复位图写回，ipc.h:1930）
- F1~F12/SF1~SF12 常量（keymap.h:93,104,135,146）与 hooks 位图编码
- `map_unmap_fkeys`（dmp.c:46）：IS 侧注册/注销逻辑（bit_set 组装 → fkey_map/fkey_unmap）
- 客户端 `fkey_ctl`（libsys/fkey_ctl.c）：`_taskcall(TTY_PROC_NR, TTY_FKEY_CONTROL)` + 位图写回
- TTY 侧契约：`do_fkey_ctl`（keyboard.c:429-527，覆盖登记无 EBUSY（DEAD_CODE 段）/UNMAP owner 检查 EPERM/EVENTS 消费位图）、`func_key`（532-585，events++ → `ipc_notify`）、观察者数组（401-415）、`debug_fkeys` 开关（78,206,406）
- 通知语义：拉模式（FKEY_EVENTS 消费位图），notify 无 payload（A-2）

## 边界

- **前置依赖**: 01 + TTY `drivers/tty` + kernel `13-syscall-dispatch`
- **不覆盖（移交）**: IS 侧分派（03）、dump 内容（05~10）
