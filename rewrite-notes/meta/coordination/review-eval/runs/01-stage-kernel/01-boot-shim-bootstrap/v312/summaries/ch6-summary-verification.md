DOC: 01-boot-shim-bootstrap.md
CHAPTER: /home/xzhao/github/minix-rs/tmp/review-loop/01-stage/chapters/01-boot-shim-bootstrap/ch6-slice-candidate.md (sha256 735a9c5d37c7ad393a79995bb2769f4458f3fbd41e8ffad2634d7d91bc0af250, 227 lines, verified)
VERDICT: FAITHFUL

- EBS 两通道：EBS 分界、U-Boot shim 与两个直核面仅裸串口、`raw_serial`/`raw_serial_line` 的 LSR 等待与 `\r` 差异、aarch64 共用节流发射器，均与 slice L18/L34 一致。
- 关键路标与三档停点：`step1+2` 归发送侧，三档读法与 CR3 后 shim 不可观测边界忠实（L14–16 对 slice L66–70）；`kernel: step4 DM coverage ok` 的误标已移除，aarch64 接收腿改列 `kernel-image: step4 DM coverage (additive on active root)`，与 slice L55 一致。
- panic 四约束：256 B 栈缓冲、超长丢弃尾部、只走 `raw_serial_line`、诊断钩子两侧注册与兜底语义完整（L20–24 对 slice L113–120）。
- 典型失败：slice §6.3 十四类全部有对应条目，4 GiB shim 行已补，v1 解码改为六分区对应行并删去无出处的 `byte_len`（L28–39 对 slice L128–143）。
- 可复制命令：三层结构、五个门脚本、`test-riscv64-uboot.sh`、aarch64 直核不在五门与 `test-smp-aps-aarch64.sh` 间接驱动、`target remote :1234` 均已补齐（L43–47 对 slice L151–188）。
- A2 删除符合 slice 无出处；`SKIP` 表示宿主 U-Boot 兼容缺口，与 slice L165 一致。
- 非阻断精度备注：`qemu ...` 仍为省略式，完整固件/磁盘参数需回看 §6.4；`-d mmu` 未提 `page` 仅 user-mode；关键路标列表为节选而非全表。不影响下游按摘要定位。
