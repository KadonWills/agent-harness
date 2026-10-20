#!/usr/bin/env bash
# List translated pages whose English source changed since they were
# translated. Each translation's first line records its source and the git
# blob hash of the English text it was translated from:
#
#   <!-- translated-from: docs/guides/local-uis.md@1a2b3c4d5e6f -->
#
# A blob hash names file contents, so it survives squash merges (commit
# hashes from a PR branch do not). After updating a translation, set its
# marker to `git hash-object <source> | cut -c1-12`.
#
# Exit 0 when every translation is current, 1 when any is stale (the list
# says which, and the command that shows what changed). Not part of the CI
# gate: English is authoritative, and a stale translation is a to-do, not a
# broken build.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Blank lines and the language switcher in the English page are not content.
ignore='^[-+][[:space:]]*$|^[-+]\*\*English\*\* [|] \[Français\]'

stale=0
while IFS= read -r page; do
  marker=$(head -n 1 "$page")
  if [[ ! "$marker" =~ ^\<\!--\ translated-from:\ ([^@]+)@([0-9a-f]+)\ --\>$ ]]; then
    echo "MISSING $page: first line must be <!-- translated-from: <source>@<blob hash> -->"
    stale=1
    continue
  fi
  source="${BASH_REMATCH[1]}"
  recorded="${BASH_REMATCH[2]}"
  if [ ! -f "$source" ]; then
    echo "GONE    $page: source $source no longer exists"
    stale=1
  elif [[ "$(git hash-object "$source")" == "$recorded"* ]]; then
    continue
  elif ! git cat-file -e "$recorded^{blob}" 2>/dev/null; then
    echo "UNKNOWN $page: $recorded is not a version of $source in this repository"
    stale=1
  else
    # diff exits 1 when the files differ; only the filtered lines matter.
    changed=$(git cat-file -p "$recorded" | diff -u - "$source" | grep '^[-+]' |
      grep -v '^\(+++\|---\) ' | grep -Ev "$ignore" || true)
    if [ -n "$changed" ]; then
      echo "STALE   $page: $source changed since it was translated"
      echo "        see: git cat-file -p $recorded | diff -u - $source"
      stale=1
    fi
  fi
done < <(find README.*.md docs/fr -name '*.md' 2>/dev/null | sort)

if [ "$stale" -eq 0 ]; then
  echo "All translations are current."
fi
exit "$stale"
