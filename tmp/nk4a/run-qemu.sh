#!/bin/bash
# NK4-A 首亮诊断辅助：带 monitor socket 启动 QEMU（工作区路径，绕开 /tmp 沙箱限制）
OS=/home/xzhao/github/minix-rs/os
D=/home/xzhao/github/minix-rs/tmp/nk4a
Q="qemu-system-x86_6"  # 拼接避免 pgrep -f 自匹配
"${Q}4" -machine q35 -net none -smp 1 -m 512M \
  -drive "if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on" \
  -drive "if=pflash,format=raw,unit=1,file=$D/vars.fd" \
  -drive "file=$OS/target/image/x86_64/minix.img,format=raw,media=disk" \
  -serial "file:$D/serial_$1.log" \
  -monitor "unix:$D/mon_$1.sock,server,nowait" \
  -display none -no-reboot &
echo "qemu pid=$!"
