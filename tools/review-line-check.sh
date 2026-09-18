#!/usr/bin/env bash
# review-line-check.sh — batch reverse-check of line-number anchors in
# markdown docs (Proposal #7/#13, edge1 K16).
#
# What it does
# ------------
# Docs accumulate claims of the shape `path/to/file.rs:123` (bare) or
# `file:///abs/path.rs#L123`. Code drifts; the line number silently rots
# while the symbol it once named moved elsewhere. This tool re-opens every
# referenced file at the referenced line and echoes what is THERE now, so
# a reviewer can spot the drift at reading speed instead of by opening
# each file.
#
# Resolution rules
# ----------------
# - `*.rs|*.sh|*.toml` paths: repo-root-relative, as written. Run the
#   tool from the repository root.
# - `*.c|*.h` bare names (`proc.c:1595` — the C ground-truth shorthand):
#   resolved against the frozen `minix3/` tree by filename index; the C
#   source never moves, so a hit is authoritative.
#
# What it deliberately does NOT do
# --------------------------------
# It does not decide whether the line still names the symbol the doc
# meant — that is semantic judgment (anchor discipline, Pattern 83) and
# stays with the reviewer. It also does not chase moved symbols; use
# rg for that once a hit looks suspicious.
#
# Usage
# -----
#   tools/review-line-check.sh DOC.md [more.md ...]   # check whole files
#   git diff -U0 | tools/review-line-check.sh --stdin # check added lines
#
# Output: one line per anchor —
#   OK    path:line | current content
#   MISS  path:line | file shorter than N lines
#   NOFILE path    | repo-side file does not exist (fails the run)
#   C-MISS name:line | C shorthand not found in minix3/ (fails the run)
# Exit: 0 = all OK, 1 = at least one MISS/NOFILE/C-MISS.

set -uo pipefail

status=0
declare -A CAND_CACHE

# Rank candidates: kernel-domain first, then headers, then servers, then
# the rest (a bare "config.h" must not resolve to some library's build
# template; a bare "proc.c" must not resolve to usr.bin/trace's).
rank() {
    awk '
        /\/minix\/kernel\//           {print "0 " $0; next}
        /\/minix\/include\/minix\//   {print "1 " $0; next}
        /\/minix\/servers\//          {print "2 " $0; next}
        /\/os\/kernel\/src\//         {print "0 " $0; next}
        /\/os\/arch\/src\//           {print "1 " $0; next}
        /\/os\/plat\/src\//           {print "2 " $0; next}
        {print "3 " $0}'
}

# Resolve a bare filename to candidate files (cached per filename).
resolve_candidates() {
    local name="$1"
    if [ -z "${CAND_CACHE[$name]+x}" ]; then
        CAND_CACHE[$name]=$( { find minix3 -name "$name" -not -path "*/hdd/*" 2>/dev/null;
                               find os -name "$name" 2>/dev/null; } | rank | sort -n | cut -d' ' -f2-)
    fi
    echo "${CAND_CACHE[$name]}"
}

# Pick the first candidate whose line actually exists (shorthand names are
# ambiguous — "schedule.c" lives in several servers — so the line range
# itself disambiguates). Echoes nothing if no candidate fits.
pick_fitting() {
    local line="$1"
    local c
    while IFS= read -r c; do
        [ -z "$c" ] && continue
        local total
        total=$(wc -l < "$c")
        if [ "$line" -le "$total" ]; then
            echo "$c"
            return 0
        fi
    done
    return 1
}

check_anchor() {
    local doc="$1" file="$2" line="$3"
    case "$file" in
        *.rs|*.sh|*.toml)
            if [ ! -f "$file" ]; then
                # Bare shorthand (e.g. "sched.rs:355") — resolve by
                # candidates; a path with directories that misses is a
                # genuine NOFILE.
                case "$file" in
                    */*)
                        echo "NOFILE $file (from $doc)"
                        status=1
                        return ;;
                    *)
                        local resolved
                        resolved=$(resolve_candidates "$file" | pick_fitting "$line")
                        if [ -z "$resolved" ]; then
                            echo "MISS  $file:$line (no candidate fits) (from $doc)"
                            status=1
                            return
                        fi
                        file="$resolved" ;;
                esac
            fi ;;
        *.c|*.h)
            local resolved
            resolved=$(resolve_candidates "$(basename "$file")" | pick_fitting "$line")
            if [ -z "$resolved" ]; then
                echo "C-MISS $file:$line (no minix3/ candidate fits) (from $doc)"
                status=1
                return
            fi
            file="$resolved" ;;
        *)
            echo "SKIP  $file (non-source reference, from $doc)"
            return ;;
    esac
    local total content
    total=$(wc -l < "$file")
    if [ "$line" -le 0 ] || [ "$line" -gt "$total" ]; then
        echo "MISS  $file:$line (file has $total lines) (from $doc)"
        status=1
        return
    fi
    content=$(sed -n "${line}p" "$file" | sed 's/^[[:space:]]*//')
    echo "OK    $file:$line | $content"
}

scan_line() {
    local doc="$1" text="$2"
    text=${text//file:\/\//}
    local re='([A-Za-z0-9_./-]+\.(rs|sh|c|h|toml))(:|\#L)([0-9]+)'
    local match file line rest
    rest="$text"
    while [[ "$rest" =~ $re ]]; do
        match="${BASH_REMATCH[0]}"
        file="${BASH_REMATCH[1]}"
        line="${BASH_REMATCH[4]}"
        check_anchor "$doc" "$file" "$line"
        rest="${rest#*"$match"}"
    done
}

if [ "${1:-}" = "--stdin" ]; then
    local_doc="stdin(diff)"
    while IFS= read -r l; do
        case "$l" in "+"*) scan_line "$local_doc" "${l#+}" ;; esac
    done
else
    if [ $# -eq 0 ]; then
        echo "usage: review-line-check.sh DOC.md [...] | --stdin < git diff -U0" >&2
        exit 2
    fi
    for doc in "$@"; do
        while IFS= read -r l; do
            scan_line "$doc" "$l"
        done < "$doc"
    done
fi

exit "$status"
