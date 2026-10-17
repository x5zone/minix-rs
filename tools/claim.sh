#!/bin/sh
# 领取锁（2026-09-20）：条目领取的排他机制 + 并发隔离机制。
# 分支 claim/<ID>-<owner> 即锁——分支名全局唯一，第二个领取者建同名/同 ID
# 分支直接失败，天然防双领；worktree 即隔离壳——claim 自动建
# .wt/<ID>-<owner>/ 专属工作树，本会话只在该树内开工。
# 背景（C-35 事故，2026-09-20）：NK5 会话在共享主树 checkout+reset 换分支，
# 连带销毁其它 agent 的未提交在制品。教训：文件头规则是提示，
# 分支才是锁，工作树才是壳——共享主树内禁 checkout/reset/--force。
# 用法：
#   tools/claim.sh claim   <ID> <owner>   领取：建锁分支 + 自动建专属工作树
#   tools/claim.sh verify                  开工/换任务前自检：位置、分支、在制数
#   tools/claim.sh list                    列出全部领取（按日期，最旧在上=stale 候选）
#   tools/claim.sh sweep                   盘点 .wt/ 现场（只读：谁在册、谁只剩构建缓存、各占多少磁盘）
#   tools/claim.sh release <ID> <owner>   释放：合入主线后删工作树+销锁
# 约定：领取后在本线 new_edgeX.md 状态列标 🔄；完成后合入主线再 release。
set -eu

usage() { sed -n '2,17p' "$0"; exit 2; }
cmd="${1:-}"; [ -n "$cmd" ] || usage

# 主工作树根（.wt/ 一律挂在这里；在链接工作树内调用也能解析回主仓）
main_root=$(git rev-parse --path-format=absolute --git-common-dir)
main_root=${main_root%/.git}

# 取分支所在工作树路径（porcelain 条目内 worktree 行在 branch 行之前，按条目配对）
wt_of() {
  git worktree list --porcelain | awk -v b="$1" '
    /^worktree /{w=substr($0,10)}
    /^branch /{if($0=="branch "b){print w;exit}}'
}

case "$cmd" in
  claim)
    id="${2:?用法: tools/claim.sh claim <ID> <owner>}"
    owner="${3:?用法: tools/claim.sh claim <ID> <owner>}"
    branch="claim/${id}-${owner}"
    if git show-ref --verify --quiet "refs/heads/$branch"; then
      echo "FAIL: $branch 已存在（你此前已领取？list 查看）"; exit 1
    fi
    others=$(git for-each-ref --format='%(refname:short) (%(committerdate:short))' \
             "refs/heads/claim/${id}-*")
    if [ -n "$others" ]; then
      echo "FAIL: ${id} 已被领取："; echo "$others" | sed 's/^/  /'; exit 1
    fi
    git branch "$branch"
    wt="$main_root/.wt/$(echo "${branch#claim/}" | tr 'A-Z' 'a-z')"
    if git worktree add "$wt" "$branch" 2>/dev/null; then
      echo "OK: 已领取 ${id}（锁分支 $branch @ $(git rev-parse --short HEAD)）"
      echo "    专属工作树已建：$wt"
      echo "    cd \"$wt\" ——本会话只在这棵树内开工；共享主树内禁 checkout/reset。"
    else
      git branch -D "$branch" >/dev/null
      echo "FAIL: worktree 建树失败（$wt 已存在？残留过 git worktree prune 后重试）；锁已回滚"
      exit 1
    fi
    echo "    请同步在本线 new_edgeX.md 状态列标 🔄（附日期）。"
    ;;
  verify)
    here=$(git rev-parse --show-toplevel)
    branch=$(git rev-parse --abbrev-ref HEAD)
    dirty=$(git status --porcelain | wc -l | tr -d ' ')
    echo "位置: $here"
    echo "分支: $branch（未提交改动: $dirty 个文件）"
    if [ "$here" = "$main_root" ]; then
      echo "警告: 你在共享主树——只可做编排/记账；开工必须 cd 进 .wt/ 专属树。"
      echo "纪律: 主树内禁 checkout/reset --force——会销毁其它线未提交在制品（C-35 事故判例）。"
    elif [ "${branch#claim/}" = "$branch" ]; then
      echo "警告: 链接工作树但不在 claim 分支上——位置与锁不匹配，先核实归属。"
    else
      echo "OK: 专属工作树 + claim 锁分支，位置正确。"
    fi
    echo "--- 当前全部领取（分支 @ 工作树）---"
    git for-each-ref --sort=committerdate --format='%(refname:short)' refs/heads/claim/ |
    while read -r b; do
      wt=$(wt_of "refs/heads/$b"); [ -n "$wt" ] || wt="（无树——历史领取或已 release 残留，开工前先 claim/补树）"
      echo "  $b -> $wt"
    done
    ;;
  list)
    git for-each-ref --sort=committerdate \
      --format='%(committerdate:short)  %(refname:short)' refs/heads/claim/
    ;;
  release)
    id="${2:?用法: tools/claim.sh release <ID> <owner>}"
    owner="${3:?用法: tools/claim.sh release <ID> <owner>}"
    branch="claim/${id}-${owner}"
    wt=$(wt_of "refs/heads/$branch")
    if [ -n "$wt" ]; then
      git worktree remove "$wt" || {
        echo "FAIL: $wt 内仍有未提交改动——先提交并合入主线，或确认弃树后 git worktree remove --force $wt"; exit 1; }
      # 注销条目不等于目录消失。权威编译门在容器里以另一属主写 os/target 的增量缓存，
      # git 删不动那些目录，于是留下一整棵 `git worktree list` 里已经查不到的树。
      if [ -e "$wt" ]; then
        echo "WARN: $wt 已从 git 注销，但目录仍在磁盘上（树内有当前用户删不掉的条目）"
        du -sh "$wt" 2>/dev/null || true
        echo "      这类残骸不报错、不在册，只有 tools/claim.sh sweep 与 df 看得见它。"
      else
        echo "OK: 已删除工作树 $wt"
      fi
    fi
    git branch -D "$branch"
    echo "OK: 已释放 ${id}（销账请同步本线状态列 ✅ + edge4 里程碑回写；跑 tools/claim.sh sweep 确认现场已清）"
    ;;
  sweep)
    # 只读盘点 .wt/：磁盘是各线共用的资源，树删没删要能用一条命令查出来，不能靠记忆。
    root="$main_root/.wt"
    me=$(id -un)
    if [ ! -d "$root" ]; then
      echo "OK: 没有 $root 目录，现场干净"
      exit 0
    fi
    reg=$(git worktree list --porcelain | awk '/^worktree /{print substr($0,10)}')
    n_reg=0; n_free=0; n_check=0
    for d in "$root"/*/; do
      [ -d "$d" ] || continue
      d=${d%/}
      size=$(du -sh "$d" 2>/dev/null | cut -f1)
      outside=$(find "$d" -type f 2>/dev/null | grep -vc '/os/target/' || true)
      if echo "$reg" | grep -qx "$d"; then
        n_reg=$((n_reg+1))
        echo "在册    $d  $size  ——工作树仍被 claim 分支持有，属在途，勿删"
      elif [ "$outside" = "0" ]; then
        n_free=$((n_free+1))
        foreign=$(find "$d" ! -user "$me" 2>/dev/null | wc -l | tr -d ' ')
        echo "可回收  $d  $size  ——$outside 个文件在 os/target 之外（内容只剩构建缓存）"
        if [ "$foreign" != "0" ]; then
          echo "        含 $foreign 个非 $me 属主的条目，必须由人执行 sudo 才能删净"
        fi
      else
        n_check=$((n_check+1))
        echo "需核对  $d  $size  ——os/target 之外还有 $outside 个文件，先做内容对账或救援副本再谈删除"
      fi
    done
    echo "--- 锁与树是否成对 ---"
    miss=0
    for b in $(git for-each-ref --format='%(refname:short)' refs/heads/claim/); do
      wt=$(wt_of "refs/heads/$b")
      if [ -z "$wt" ] || [ ! -d "$wt" ]; then
        echo "失配    锁 $b 在册，但工作树 ${wt:-（无）} 不在磁盘上——在这条线上开工前必须补树"
        miss=$((miss+1))
      fi
    done
    [ "$miss" = "0" ] && echo "OK: 每个 claim 锁都有对应的工作树"
    echo "--- 判定：在册 $n_reg 棵（在途）/ 可回收 $n_free 棵 / 需核对 $n_check 棵；锁树失配 $miss 处 ---"
    echo "本命令只读，不执行任何删除。"
    ;;
  *)
    echo "unknown command: $cmd"; usage;;
esac
