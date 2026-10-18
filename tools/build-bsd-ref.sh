#!/usr/bin/env bash
# 用宿主 glibc + BSD 补位面，把 minix3/ 里的一支 C 程序编成「参照件」。
#
# 参照件是本项目 A/B 对账的真值载体：判据「Minix3 C 源码行为」要落成可比的绝对读数，
# 光读源码不够，得真跑。此前每支参照件都靠一条手敲的 `gcc` 行，行长且容易漏掉必需的
# `-I`/`-D`，漏了就把承载差异读成 C 的行为（判例见 `rewrite-notes/18-stage-commands/todo.md`
# 批次三百六十六：两次尝试分别撞在 `__COPYRIGHT` 与 `sys/featuretest.h` 上）。本脚本把这些
# 固定项收在一处，只把「程序自己的额外参数」留给调用方。
#
# 用法：tools/build-bsd-ref.sh -o OUT SRC [SRC...] [额外 gcc 参数...]
# 额外参数原样传给 gcc，因此每支程序的配方仍是一条能贴进台账、在任何检出里重跑的命令。
#
# 补位面为什么不会污染别的门：`host-ref/` 只由本脚本的 `-I` 选中，交叉编译与内核构建都
# 不经过它；`tools/atf-c-compat/sys/` 那几枚给 picolibc 的头一字未动。
set -euo pipefail

if [ $# -lt 3 ]; then
	echo "用法: $0 -o OUT SRC [SRC...] [额外 gcc 参数...]" >&2
	exit 2
fi

root=$(git rev-parse --show-toplevel)
compat="$root/tools/atf-c-compat/host-ref"
[ -d "$compat" ] || { echo "$0: 找不到补位面 $compat" >&2; exit 1; }

out=""
sources=()
extra=()
while [ $# -gt 0 ]; do
	case "$1" in
	-o)
		shift
		out=$1
		;;
	-*)
		if [ ${#sources[@]} -eq 0 ]; then
			echo "$0: 第一个参数得是 -o OUT" >&2
			exit 2
		fi
		extra+=("$1")
		;;
	*)
		if [ ${#sources[@]} -eq 0 ]; then
			sources+=("$1")
		else
			extra+=("$1")
		fi
		;;
	esac
	shift
done

[ -n "$out" ] || { echo "$0: 缺 -o OUT" >&2; exit 2; }
case "$out" in
"$root/minix3/"*)
	echo "$0: 参照件不许落进 minix3/（那是只读真源）" >&2
	exit 2
	;;
esac
mkdir -p "$(dirname "$out")"

# 补面里的函数级单元全部一起链上（一枚 `.c` 一个 BSD 家姓的函数族，都带自己的 libc 判据，
# 在有的 libc 上整件退化成空 translation unit，不会与系统实现撞符号）。
compat_units=()
for unit in "$compat"/bsd-*.c; do
	[ -e "$unit" ] && compat_units+=("$unit")
done

cmd=(gcc -O0 -Wall
	-I"$compat"
	-include "$compat/bsd-compat-proto.h"
	"${compat_units[@]}"
	"${sources[@]}"
	"${extra[@]}"
	-o "$out")

echo "+ ${cmd[*]}"
"${cmd[@]}"
echo "参照件：$out"
