#!/bin/sh
# 真机读数件：`split -n <片数>` 在**管道**当标准输入时，C 的 `split3` 到底看见多少字节。
#
# 为什么这枚脚本要在真机跑而不是在宿主跑：本仓的 `split` 壳读尽输入后按 `input.len()`
# 分片，而 C 的 `split3` 在读任何字节之前就 `fstat(ifd)` 取 `sb.st_size`
# （`minix3/usr.bin/split/split.c:290-300`），两形在 stdin 是管道时分叉：
#   - glibc 宿主：管道 `fstat` 的 `st_size` 恒 0 ⇒ 参照件报 `can't split into more than 0 files` 退 1、不产文件；
#   - Minix：`do_fstat` 只是把请求转给该 vnode 所属文件系统（`minix3/minix/servers/vfs/stadir.c:185-187`
#     的 `req_stat(rfilp->filp_vno->v_fs_e, rfilp->filp_vno->v_inode_nr, ...)`），
#     管道归 **PFS**，而 **PFS 源码不在本仓 vendored 树里**（现查：`ls minix3/fs` 无输出、
#     `find minix3 -path "*pfs*" -name "*.c"` 只命中无关的 tcpdump 打印件），
#     所以「Minix 的管道 st_size 是 0 还是已缓冲字节数」在仓库内不可判——这一形只能靠真机读数。
# VFS 侧另有线索说明它未必是 0：关闭最后一个读者/写者时 VFS 会 `truncate_vnode(vp, vp->v_size)`
# 并注释「Tell PFS about latest pipe size」（`minix3/minix/servers/vfs/filedes.c:494-500`），
# 即 Minix 自己给管道记着尺寸。
#
# 跑法（真机上，`split_ref` 为该机上按原样编出的 C `split`）：
#   sh split_n_stdin_size.sh <split 二进制> [比对用的第二枚二进制]
# 输出的是每形的「退码 + stderr 全文 + 生成的片文件名与字节数」，可直接贴进台账当 L1 读数。
set -u

BIN=${1:-/bin/split}
# 绝对化：下面的每一形都在临时目录里跑，相对路径会指不到被测件
case "$BIN" in
	/*) : ;;
	*) BIN="$PWD/$BIN" ;;
esac
WORK=${TMPDIR:-/tmp}/split-n-stdin-size.$$
mkdir -p "$WORK" || exit 2

report() { # 形名 命令...
	label=$1; shift
	printf '\n=== %s ===\n命令行: %s\n' "$label" "$*"
	( cd "$WORK" && "$@" ) 2>"$WORK/.err"
	code=$?
	printf '退码=%s\nstderr=[%s]\n' "$code" "$(cat "$WORK/.err")"
	printf '产物: '
	( cd "$WORK" && for f in x*; do [ -f "$f" ] && printf '%s(%s字节) ' "$f" "$(wc -c <"$f")"; done; echo )
	rm -f "$WORK"/x* 2>/dev/null
}

echo "被测二进制: $BIN"
echo "== 前置：这台机的 wc -c 对管道给多少（旁证 st_size 是否与已缓冲字节一致）=="
printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n%.0s' 1 2 3 4 5 | wc -c

# 形 1：stdin 是管道，且写者已把 200 字节写完（读之前就应有数据在飞）
report "1 管道 stdin，200 字节已在飞，-n 4" \
	sh -c 'head -c 200 /dev/zero | tr "\0" A | '"$BIN"' -n 4'

# 形 2：同一内容换成**规则文件**操作数（这一形在宿主也已知，用来校验环境没歪）
head -c 200 /dev/zero | tr '\0' A > "$WORK/plain"
report "2 规则文件操作数，同内容，-n 4" "$BIN" -n 4 "$WORK/plain" "$WORK/f"

# 形 3：管道但写者慢（读之前可能没有数据在飞）——用来区分「恒 0」与「按已缓冲字节」
report "3 管道 stdin，写者睡 1 秒后再写，-n 4" \
	sh -c '(sleep 1; head -c 200 /dev/zero | tr "\0" A) | '"$BIN"' -n 4'

# 形 4：stdin 是终端类字符设备（`-n` 对字符设备的第三形，若这台机给得出）
report "4 /dev/zero 当 stdin（字符设备，-n 2，只验它肯不肯开工）" \
	sh -c 'head -c 40 /dev/zero | '"$BIN"' -n 2'

echo
echo "读法：形 1 与形 3 若都报 0 files ⇒ 这台机管道 st_size 恒 0，本壳应改成「管道不产文件、报 can't split into more than 0 files 退 1」；"
echo "形 1 出 4 片而形 3 报 0 ⇒ st_size 是「已缓冲字节数」，那是一条竞态语义，本壳无论怎么改都不可能在启动时对齐，得按竞态单独定策。"
rm -rf "$WORK"
