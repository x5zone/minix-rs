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
# - `*.c|*.h` paths that carry directories (`minix3/minix/servers/rs/const.h:29`)
#   are honored verbatim — the doc already disambiguated.
# - bare `*.c|*.h` names (`proc.c:1595` — the C ground-truth shorthand) are
#   resolved against the frozen `minix3/` tree by filename index. A bare name
#   is ambiguous by construction (20× `const.h`, 27× `proto.h`): if more than
#   one candidate can host the line, the tool reports AMBIG and FAILS instead
#   of silently picking the top-ranked file — a silent pick is how a VFS doc's
#   `const.h:9` (vfs/const.h:9 = `NR_WTHREADS 9`) came back "OK" from
#
#       minix3/minix/kernel/const.h:9   (an empty line)
#
#   Resolution: qualify the reference in the doc.
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

# Resolves a bare filename to the single candidate that can host `line`.
# Echoes the path on success. Zero candidates → `*-MISS`; more than one →
# `AMBIG` (the reference is not mechanically verifiable — qualify it).
# Both failure shapes report on **stderr** (the caller captures stdout for
# the path) and exit non-zero; the caller sets `status=1` and returns.
resolve_unique() {
    local doc="$1" name="$2" line="$3" kind="$4"
    local cands="" n=0 c
    while IFS= read -r c; do
        [ -z "$c" ] && continue
        if [ "$line" -le "$(wc -l < "$c")" ]; then
            cands="${cands:+$cands }$c"
            n=$((n + 1))
        fi
    done < <(resolve_candidates "$name")
    if [ "$n" -eq 0 ]; then
        echo "$kind-MISS $name:$line (no candidate fits) (from $doc)" >&2
        return 1
    fi
    if [ "$n" -gt 1 ]; then
        echo "AMBIG $name:$line ($n candidates: $cands) — qualify the path (from $doc)" >&2
        return 1
    fi
    printf '%s' "$cands"
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
                        if ! resolved=$(resolve_unique "$doc" "$file" "$line" rs); then
                            status=1
                            return
                        fi
                        file="$resolved" ;;
                esac
            fi ;;
        *.c|*.h)
            # A path-ful reference is already disambiguated by the doc —
            # honor it verbatim; only bare names go through the index.
            case "$file" in
                */*)
                    if [ ! -f "$file" ]; then
                        echo "NOFILE $file (from $doc)"
                        status=1
                        return
                    fi ;;
                *)
                    local resolved
                    if ! resolved=$(resolve_unique "$doc" "$file" "$line" C); then
                        status=1
                        return
                    fi
                    file="$resolved" ;;
            esac ;;
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
