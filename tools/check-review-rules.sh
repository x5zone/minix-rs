#!/usr/bin/env bash
# check-review-rules.sh - validate the review source and its three adapters.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SKILLS=(
  review-code-skill
  review-core-semantics-skill
  review-coverage-skill
  review-doc-skill
  review-excellence-skill
  review-implementation-skill
  review-patterns-skill
  review-process-skill
  review-socratic-skill
)

errors=0

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  errors=$((errors + 1))
}

frontmatter_value() {
  local key="$1"
  local file="$2"
  sed -n "/^${key}:/ { s/^${key}:[[:space:]]*//; p; q; }" "$file"
}

strip_frontmatter() {
  awk '/^---$/{count++; next} count >= 2' "$1"
}

check_trae_skill() {
  local name="$1"
  local source="prompt/skill/${name}.md"
  local derived=".trae/skills/${name}/SKILL.md"
  local source_body derived_body

  [[ -f "$source" ]] || { fail "missing source: $source"; return; }
  [[ -f "$derived" ]] || { fail "missing Trae skill: $derived"; return; }

  source_body="$(mktemp)"
  derived_body="$(mktemp)"
  strip_frontmatter "$source" > "$source_body"
  strip_frontmatter "$derived" > "$derived_body"
  if ! diff -q "$source_body" "$derived_body" >/dev/null; then
    fail "Trae body drift: $source vs $derived"
  fi
  rm -f "$source_body" "$derived_body"
}

check_frontmatter() {
  local dir="$1"
  local file name expected desc
  for file in "$dir"/*/SKILL.md; do
    [[ -f "$file" ]] || continue
    name="$(frontmatter_value name "$file")"
    expected="$(basename "$(dirname "$file")")"
    [[ "$expected" == review-agent-* ]] && continue
    [[ "$name" == "$expected" ]] || fail "name does not match directory: $file"
    if [[ "$dir" == ".codex/skills" ]]; then
      desc="$(frontmatter_value description "$file")"
      [[ "$desc" == '"'*'"' ]] || fail "Codex description is not quoted: $file"
      desc="${desc#\"}"
      desc="${desc%\"}"
      # Codex SKILL.md description limit is 1024 chars (developers.openai.com/codex/skills).
      # Count Unicode characters via python3 — bash ${#desc} is locale-dependent (counts bytes
      # under a non-UTF-8 locale, false-failing CJK text). python3 is locale-independent.
      desc_len=$(python3 -c 'import sys; print(len(sys.argv[1]))' "$desc" 2>/dev/null \
                 || printf '%s' "$desc" | wc -m)
      (( desc_len <= 1024 )) || fail "Codex description exceeds 1024 characters: $file (${desc_len})"
    fi
  done
}

for skill in "${SKILLS[@]}"; do
  check_trae_skill "$skill"
done

[[ -f ".trae/skills/review-agent-ide/SKILL.md" ]] || fail "missing Trae agent adapter"
[[ -f ".trae/skills/review-agent-trigger/SKILL.md" ]] || fail "missing Trae trigger adapter"
cmp -s prompt/skill/review-agent-ide.md .trae/skills/review-agent-ide/SKILL.md || fail "Trae agent drift"
cmp -s prompt/skill/review-agent-trigger.md .trae/skills/review-agent-trigger/SKILL.md || fail "Trae trigger drift"
# Trae IDE hard limits: Agent prompt ≤ 10000 chars (auto-truncated), trigger ≤ 5000 chars.
# Count characters via python3 (locale-independent); wc -m counts bytes under a non-UTF-8 locale.
agent_chars=$(python3 -c 'import sys; print(len(open(sys.argv[1],encoding="utf-8").read()))' prompt/skill/review-agent-ide.md)
(( agent_chars <= 10000 )) || fail "Trae agent prompt exceeds 10000-char hard limit (auto-truncated): ${agent_chars}"
trigger_chars=$(python3 -c 'import sys; print(len(open(sys.argv[1],encoding="utf-8").read()))' prompt/skill/review-agent-trigger.md)
(( trigger_chars <= 5000 )) || fail "Trae trigger description exceeds 5000-char hard limit: ${trigger_chars}"

check_frontmatter ".trae/skills"
check_frontmatter ".codex/skills"

for required in review-scan review-code-skill review-doc-skill review-patterns-skill review-process-skill; do
  [[ -f ".codex/skills/${required}/SKILL.md" ]] || fail "missing Codex skill: $required"
done

if rg -n '\.review/trae|tools/review-init\.sh trae' .codex/skills >/dev/null; then
  fail "Codex adapter contains a non-Codex runtime path (.review/trae or review-init.sh trae)"
fi
rg -q '\.review/codex' .codex/skills/review-scan/SKILL.md || fail "Codex orchestrator has no Codex output path"
rg -q '\.review/codex' .codex/skills/review-coverage-skill/SKILL.md || fail "Codex coverage skill has no Codex output path"
rg -q '\.review/codex' .codex/skills/review-process-skill/SKILL.md || fail "Codex process skill has no Codex output path"

# Codex body drift heuristic: count ### headings in source vs Codex.
# Codex has intentional path/layout adaptations, so exact diff won't work.
# But major content drift (missing sections) will show as a heading count gap.
# Threshold: 5 (allows minor intentional differences, catches missing sections like Step 1.0a-g)
for skill in "${SKILLS[@]}"; do
  src_file="prompt/skill/${skill}.md"
  cod_file=".codex/skills/${skill}/SKILL.md"
  [[ -f "$src_file" ]] || continue
  [[ -f "$cod_file" ]] || continue
  src_h3=$(strip_frontmatter "$src_file" | grep -c '^### ')
  cod_h3=$(strip_frontmatter "$cod_file" | grep -c '^### ')
  diff=$(( src_h3 > cod_h3 ? src_h3 - cod_h3 : cod_h3 - src_h3 ))
  if (( diff > 5 )); then
    fail "Codex body drift: $skill has ${cod_h3} H3 headings vs source ${src_h3} (diff=${diff}, threshold=5)"
  fi
done

rg -q 'Minix3 源码行为.*design 契约.*Rust 实现' prompt/review-rules/review.md \
  || fail "canonical Ground Truth chain is missing from source"
rg -q '所有 review 模式必检' prompt/skill/review-process-skill.md \
  || fail "source process skill does not require Gate H for all modes"
rg -q '9 个 grep 可验锚段' prompt/skill/review-process-skill.md \
  || fail "source process skill has stale Gate 0 anchor count"
rg -q 'notes/rewrite/\{module\}/\{stage\}/\.design/' prompt/review-rules/review-process.md \
  || fail "source process rules do not use .design"

# .claude layer validation (NEW 2026-08-15, meta-review F-B5-F14).
# The .claude/.codex review-scan layers are manually maintained — this blocks
# regressions like checks/code.md being overwritten with unrelated content.
for f in .claude/rules/review-core.md .claude/rules/review-process.md .claude/rules/fix-guard.md \
         .claude/skills/review-scan/SKILL.md .claude/skills/review-implementation-skill/SKILL.md; do
  [[ -f "$f" ]] || fail "missing .claude file: $f"
done
for check in doc code patterns process excellence; do
  [[ -f ".claude/skills/review-scan/checks/${check}.md" ]] || fail "missing .claude checks/${check}.md"
  [[ -f ".codex/skills/review-scan/checks/${check}.md" ]] || fail "missing .codex checks/${check}.md"
done
rg -q 'Check 01: Rewrite 质量' .claude/skills/review-scan/checks/code.md \
  || fail "checks/code.md is not a code checklist (regression of F-B3-F1)"
rg -q 'Check 01: Rewrite 质量' .codex/skills/review-scan/checks/code.md \
  || fail ".codex checks/code.md is not a code checklist"
rg -q 'spin_loop' .claude/skills/review-scan/checks/patterns.md \
  || fail ".claude patterns.md §0 item 4 missing spin_loop (C-P1-3)"
rg -q 'Step 5\.4 L3 grep|Step 5\.5 测试总数' .claude/rules/review-process.md .claude/skills/review-scan/checks/process.md \
  && fail "conflicting Step 5.4/5.5 numbering in .claude (should use §2.4i/§2.4j)"
[[ "$(grep -c '^### Step 1\.0b' .claude/rules/review-process.md)" -eq 1 ]] \
  || fail ".claude review-process.md has duplicate Step 1.0b heading"
[[ "$(grep -c 'Step 0\.5: 生成 structure' prompt/review-rules/review-process.md)" -eq 1 ]] \
  || fail "source review-process.md has duplicate Step 0.5 heading (F-A3-1 regression)"

if (( errors > 0 )); then
  printf '%d review-rule checks failed\n' "$errors" >&2
  exit 1
fi

printf 'Review source, Trae adapters, Claude runtime references, and Codex adapters are consistent.\n'
