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
#   tools/claim.sh release <ID> <owner>   释放：合入主线后删工作树+销锁
# 约定：领取后在本线 new_edgeX.md 状态列标 🔄；完成后合入主线再 release。
set -eu

usage() { sed -n '2,16p' "$0"; exit 2; }
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
      echo "OK: 已删除工作树 $wt"
    fi
    git branch -D "$branch"
    echo "OK: 已释放 ${id}（销账请同步本线状态列 ✅ + edge4 里程碑回写）"
    ;;
  *)
    echo "unknown command: $cmd"; usage;;
esac
