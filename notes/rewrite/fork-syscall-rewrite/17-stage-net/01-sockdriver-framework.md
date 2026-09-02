# 01-sockdriver-framework — socket driver 框架 libsockdriver

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- libsockdriver 全量：`sockdriver_task`（sockdriver.c:1132）/`sockdriver_process`（:1061）/`sockdriver_terminate`（:1120）/`sockdriver_announce`（:47）
- `sdr_*` 回调表 19 项（sdr_socket → sdr_other，sockdriver.h struct sockdriver）
- SDEV 请求/回复消息布局：`SDEV_RQ_BASE 0x1900`（17 请求，com.h:1037-1061）+ `SDEV_RS_BASE 0x1980`（6 回复，com.h:1063-1068）+ `mess_vfs_lsockdriver_*` 6 布局（ipc.h）
- 拷贝辅助：`sockdriver_copyin/out`、`vcopyin/out`、`copyin_opt/copyout_opt`；悬挂调用标识 `sockdriver_call`（sc_endpt/sc_req/_sc_grant/_sc_len）、`sockdriver_select`、`sockdriver_packed_data`、`pack_data/unpack_data`
- 回复族：`sockdriver_reply_generic/accept/recv/select`
- Rust: `os/libs/minix-netdriver`（SDEV 类型 + 框架 trait，新建）；边界：与 `05-stage-vfs/22-sdev.md`（VFS 客户端侧）对称

## 边界

- **前置依赖**: 00、`../05-stage-vfs/22-sdev.md`（客户端协议面）
- **本篇不覆盖**: sock 对象/悬挂续作语义（02）；协议常量值（99）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
