#!/usr/bin/env bash
# todo-reconcile-scan.sh — 待办台账判据执行器（只读，可重跑）
#
# 作用：把 `rewrite-notes/coordination/TODO-LEDGER-INDEX.md` §4 判据表里那些
#       「一条命令可定案」的条目重算一遍，输出 TSV（类别 / 读数 / 期望基线 / 是否漂移）。
#       台账里每条状态变更都要求附可重跑命令，本脚本就是这些命令的集中执行处——
#       下一轮不必再逐条手敲，也不必相信上一轮抄下的数字。
#
# 边界（硬性）：
#   - 只读：只用 grep / wc / sed，不跑 cargo、不跑 git 写命令、不起虚拟机。
#   - 不判定语义：需要读码追调用链（判据分级里的 L2）或需要真机（待验证）的条目
#     不在本脚本射程内，脚本只报「本脚本不定案」，绝不产出绿勾。
#   - 不写任何台账文件；输出到标准输出，需要留档时由调用方自行重定向到 tmp/。
#
# 用法：
#   tools/todo-reconcile-scan.sh              # 重算全部判据，漂移项标 DRIFT
#   tools/todo-reconcile-scan.sh --self-test  # 自检：判据函数可用、输出列数稳定
#   tools/todo-reconcile-scan.sh --help
#
# 退出码：0 = 全部与基线一致；1 = 有漂移（只报告，不阻断）；2 = 用法或环境错误
set -uo pipefail

usage() {
  cat <<'HELP'
用法：tools/todo-reconcile-scan.sh [--self-test|--help]

重算待办台账的可机器判据，输出四列 TSV：
  判据名 / 本次读数 / 台账基线 / 判定（OK|DRIFT|NA）
基线取自 rewrite-notes/coordination/TODO-LEDGER-INDEX.md §4，数字变化即 DRIFT——
这不代表台账错了，只代表写台账时该重跑并复核相应条目。
HELP
}

case "${1:-}" in
  -h|--help) usage; exit 0 ;;
esac

root=$(git rev-parse --show-toplevel 2>/dev/null) || { echo "不在 git 仓库内" >&2; exit 2; }
cd "$root"
[ -d os ] || { echo "缺 os/ 目录" >&2; exit 2; }

# rs <grep 参数...> → 统计 os/ 下 .rs 文件命中数（排除构建产物目录）
rs() { grep -rn "$@" os --include=*.rs 2>/dev/null | grep -v '^os/target' | wc -l | tr -d ' '; }

row() { # 判据名 读数 基线 判定
  printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4"
  [ "$4" = DRIFT ] && drift=$((drift + 1))
}
verdict() { [ "$1" = "$2" ] && echo OK || echo DRIFT; }
drift=0

printf '判据\t本次读数\t台账基线\t判定\n'

n=$(rs "用后即滚");                     row "探针待滚除（行）" "$n" 15 "$(verdict "$n" 15)"
n=$(rs -E "nk4[ac]:");                  row "探针命名族（行）" "$n" 15 "$(verdict "$n" 15)"
n=$(grep -rn "todo!()\|unimplemented!()" os/kernel/src os/arch/src 2>/dev/null | wc -l | tr -d ' ')
                                        row "内核与架构层静默占位" "$n" 0 "$(verdict "$n" 0)"
n=$(rs "UnimplementedTransport");        row "占位传输实现" "$n" 5 "$(verdict "$n" 5)"
n=$(grep -rln "loop {}" os --include=main.rs 2>/dev/null | grep -v '^os/target' | wc -l | tr -d ' ')
                                        row "服务入口空转占位（文件）" "$n" 55 "$(verdict "$n" 55)"
n=$(rs "clamp_cpu_to_bsp");             row "主核钳位（行）" "$n" 10 "$(verdict "$n" 10)"
n=$(rs "fn context_stop_idle");         row "空闲核唤醒臂本体" "$n" 0 "$(verdict "$n" 0)"
n=$(rs "AP_GO");                        row "次级核放行邮箱（行）" "$n" 11 "$(verdict "$n" 11)"
n=$(rs "fpu_owner");                    row "浮点归属相关行" "$n" 21 "$(verdict "$n" 21)"
n=$(rs -E "fence\.i|dc *cvau|ic *iallu|sync_icache");
                                        row "指令缓存维护（行）" "$n" 2 "$(verdict "$n" 2)"
n=$(grep -c "ATF_BOOT_LEG_READY: \&\[&str\] = \&\[\"aarch64\", \"riscv64\"\]" os/xtask/src/image.rs | tr -d ' ')
                                        row "测试套件白名单缺 x86（1=仍缺）" "$n" 1 "$(verdict "$n" 1)"
n=$(grep -n '^default' os/kernel/Cargo.toml 2>/dev/null | grep -c 'mock' | tr -d ' ')
                                        row "内核包默认带替身特性（1=是）" "$n" 1 "$(verdict "$n" 1)"
n=0; for d in os/arch/src/arm64 os/arch/src/riscv64 os/plat/src/arm64 os/plat/src/riscv64; do
     c=$(grep -rn "#\[test\]" "$d" 2>/dev/null | wc -l); n=$((n + c)); done
                                        row "宿主不可达的门内测试" "$n" 88 "$(verdict "$n" 88)"
printf '需读码或需真机的判据（调用链可达性、语义对 C 真源、门跑结果）不在本脚本射程\t-\t-\tNA\n'

if [ "$drift" -gt 0 ]; then
  echo "漂移项 $drift 个：读数与台账基线不符。基线随代码前进而失配是常态，"
  echo "处置方式是复核相应条目后更新台账（而不是改本脚本的基线去迁就现状）。" >&2
  [ "${1:-}" = "--self-test" ] || exit 1
fi

if [ "${1:-}" = "--self-test" ]; then
  echo "--- 自检：输出列数与判据条数 ---"
  out=$(bash "$0")
  cols=$(printf '%s\n' "$out" | awk -F'\t' 'NF>1 {print NF}' | sort -u | tr '\n' ' ')
  rows=$(printf '%s\n' "$out" | grep -c $'\t')
  echo "列数集合=[$cols]（应为 4）  行数=$rows（应 ≥ 12）"
  [ "$cols" = "4 " ] && [ "$rows" -ge 12 ] && { echo "自检通过"; exit 0; }
  echo "自检失败" >&2; exit 1
fi
