#!/usr/bin/env bash
# NK4-C 缺陷 (A) 抓写者 · 形1↔形2【自动交替】单后台长跑
#
# 派生自 riscv_watch_9dc377f8.sh（plan 步骤1）。对 §续-239 收敛的唯一物理 L2
# 中间项槽 0x9DC377F8（子根 0x9dc37000 + i2(255)*8）设【硬件写 watchpoint】，
# -S halt 从头设点、continue 到首次命中即拓出把数据字写进该 L2 槽的那条 store
# PC + 回栈 + 写入值。
#
# 与单形脚本的区别：本脚本在【一次后台调用】内顺序跑两种 QEMU riscv system-mode
# gdbstub 的 hw-watchpoint 地址语义，免去人工反复点火轮次（§续-240 封锁点①）：
#   形2（先试）：VmDm 虚拟 0x19DC377F8（=1<<36 DM 基 + 物理；satp 开启后经 VM DM
#              访问该物理帧——exec 装填写者多半走此虚拟车道）
#   形1（兜底）：裸物理 0x9DC377F8 + maintenance packet Qqemu.PhyMemMode:1
#              （boot 早期 satp=Bare 虚=物；PhyMemMode 让 gdbstub 按物理解释点地址）
#
# 零 guest 文本探针（不扰动 (A) 时序，除 halt 本身——§续-232 铁证 (A) 对 halt 敏感，
# 但 watchpoint 抓的是【写入瞬间】非崩点 PC，崩点漂移不影响命中，§续-239/plan）。
#
# 用法: riscv_watch_auto.sh [tag]   （默认 tag=gh92；日志落 tmp/nk4a/{tag}-*.{serial,gdb.txt}）
# 收尾: 每形点火前验端口空闲 + 端口锚定精确杀（绝不裸杀会误伤并行会话，§续-240/plan 风险表）。
set -u
TAG="${1:-gh92}"
PORT=1241                       # fresh 端口，避开任何其它会话的 1240
ROOT=/home/xzhao/github/minix-rs/os
REL="$ROOT/target/riscv64gc-unknown-none-elf/release"
IMG="$ROOT/target/image/riscv64"
LOG=/home/xzhao/github/minix-rs/tmp/nk4a
PHY=0x9DC377F8                  # 唯一收敛靶（L2 中间项槽，物理）
DMV=0x19DC377F8                 # = 1<<36 + PHY（VM 调试底图虚拟形态）
BOOT_TIMEOUT="${BOOT_TIMEOUT:-340}"   # 覆盖 §续-236 boot 150~250s + 余量

# 复用 gh 已建产物（无生产码变更，binary 即当前 HEAD 构建）；仅重打包各模块 bin 供 loader
LA=$(ROOT="$ROOT" IMG="$IMG" REL="$REL" python3 -c '
import os
rel=os.environ["REL"];img=os.environ["IMG"];base=0x86000000
mods=[("ds","minix-ds"),("rs","minix-rs"),("pm","minix-pm"),("sched","minix-sched"),("vfs","minix-vfs"),("memory","minix-driver-memory"),("tty","minix-driver-tty"),("mib","minix-mib"),("vm","minix-vm"),("pfs","minix-fs-pfs"),("mfs","minix-fs-mfs"),("init","minix-init")]
pa=base;o=[]
for n,b in mods:
    d=open(os.path.join(rel,b),"rb").read(); mf=os.path.join(img,"mod_%s.bin"%n); open(mf,"wb").write(d)
    o+=["-device","loader,addr=0x%x,file=%s,force-raw=on"%(pa,mf)]; pa+=(len(d)+0xfff)&~0xfff
print(" ".join(o))')

cleanup() { pkill -f "[q]emu-system-riscv64.*$PORT" 2>/dev/null || true; sleep 1; }

# run_form <form_tag> <watch_addr> <phys_mode:0|1> <desc>
run_form() {
  local FT="$1" WA="$2" PM="$3" DESC="$4"
  echo ""
  echo "===AUTO-BEGIN desc=$DESC tag=$FT addr=$WA physmode=$PM port=$PORT==="
  if ss -ltn 2>/dev/null | grep -q ":$PORT "; then
    echo "WARN: port $PORT busy before start; anchored-cleanup"
    cleanup
  fi
  cleanup
  setsid qemu-system-riscv64 -machine virt -smp 1 -m 512M -bios default \
    -kernel "$REL/kernel" -dtb "$IMG/qemu.dtb" \
    -device "loader,addr=0x85000000,file=$IMG/table.bin,force-raw=on" $LA \
    -serial file:$LOG/$FT.serial -display none -no-reboot \
    -S -gdb tcp::$PORT </dev/null >/dev/null 2>&1 &
  local QPID=$!
  sleep 3
  local PMEX=()
  if [ "$PM" = "1" ]; then PMEX=( -ex "maintenance packet Qqemu.PhyMemMode:1" ); fi
  # 先设点→打设点确认行→continue 到首次命中拓现场；命中判据=gdb 输出含
  # "Hardware watchpoint $FT ... Old value/New value" 且 $pc 落在 minix-vm U 态。
  timeout "$BOOT_TIMEOUT" gdb-multiarch -batch -nx \
    -ex "set pagination off" \
    -ex "set confirm off" \
    -ex "set can-use-hw-watchpoints 1" \
    -ex "file $REL/minix-vm" \
    -ex "target extended-remote localhost:$PORT" \
    "${PMEX[@]}" \
    -ex "printf \"===SET-WATCH begin===\\n\"\n" \
    -ex "awatch *(unsigned long*)$WA" \
    -ex "info breakpoints" \
    -ex "printf \"===SET-WATCH end (expect a 'Hardware watchpoint N' line above)===\\n\"\n" \
    -ex "continue" \
    -ex "printf \"\\n===HIT PC/INSN===\\n\"\n" \
    -ex "x/1i \$pc" \
    -ex "printf \"===REGS===\\n\"\n" \
    -ex "info registers a0 a1 a2 a3 a4 a5 a6 a7 t0 s0 sp ra pc" \
    -ex "printf \"===VALUE just written into slot===\\n\"\n" \
    -ex "x/1xg $WA" \
    -ex "printf \"===decoded paddr=(val>>10)<<12 ; leaf R/W/X low bits===\\n\"\n" \
    -ex "p/x ((*(unsigned long*)$WA) >> 10) << 12" \
    -ex "printf \"===BACKTRACE===\\n\"\n" \
    -ex "bt" \
    2>&1 | tee "$LOG/$FT-gdb.txt"
  local RC=${PIPESTATUS[0]}
  kill "$QPID" 2>/dev/null; pkill -P "$QPID" 2>/dev/null || true
  cleanup
  # 命中判定：gdb 日志出现 watchpoint 触发（Stopped: Hardware watchpoint）且 info registers 段
  if grep -qE "Hardware watchpoint [0-9]+:.*(Old value|New value)" "$LOG/$FT-gdb.txt" 2>/dev/null \
     && grep -qE "^===HIT PC/INSN===" "$LOG/$FT-gdb.txt" 2>/dev/null; then
    echo "===AUTO-RESULT tag=$FT desc=$DESC HIT=yes rc=$RC==="
  elif grep -qE "Hardware watchpoint [0-9]+:" "$LOG/$FT-gdb.txt" 2>/dev/null; then
    echo "===AUTO-RESULT tag=$FT desc=$DESC HIT=no (watch SET but not triggered; boot-timeout/crash) rc=$RC==="
  else
    echo "===AUTO-RESULT tag=$FT desc=$DESC HIT=no (watch NOT set — syntax/PhyMemMode/gdbstub) rc=$RC==="
  fi
}

# ---- 顺序跑两形（形2 DM 虚先，形1 裸物理+PhyMemMode 兜底）----
run_form "${TAG}f2" "$DMV" 0 "form2-DM-virtual"
# 若形2 命中则短路，不再跑形1（省一次 150~250s boot）
if grep -qE "Hardware watchpoint [0-9]+:.*(Old value|New value)" "$LOG/${TAG}f2-gdb.txt" 2>/dev/null; then
  echo "===AUTO-SHORTCIRCUIT form2 HIT — skip form1==="
else
  run_form "${TAG}f1" "$PHY" 1 "form1-bare-phys-PhyMemMode"
fi

echo ""
echo "===AUTO-DONE logs: $LOG/${TAG}f2-gdb.txt $LOG/${TAG}f1-gdb.txt (if ran) $LOG/${TAG}*.serial==="
