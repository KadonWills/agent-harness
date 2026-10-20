#!/usr/bin/env bash
# List translated pages whose English source changed since they were
# translated. Each translation's first line records its source and commit:
#
#   <!-- translated-from: docs/guides/local-uis.md@6cf97db -->
#
# Exit 0 when every translation is current, 1 when any is stale (the list
# says which, and the diff command to see what changed). Not part of the CI
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
    echo "MISSING $page: first line must be <!-- translated-from: <source>@<commit> -->"
    stale=1
    continue
  fi
  source="${BASH_REMATCH[1]}"
  commit="${BASH_REMATCH[2]}"
  if [ ! -f "$source" ]; then
    echo "GONE    $page: source $source no longer exists"
    stale=1
  elif git diff "$commit" HEAD -- "$source" | grep '^[-+]' | grep -v '^\(+++\|---\) ' |
    grep -Evq "$ignore"; then
    echo "STALE   $page: $source changed since $commit"
    echo "        see: git diff $commit HEAD -- $source"
    stale=1
  fi
done < <(find README.*.md docs/fr -name '*.md' 2>/dev/null | sort)

if [ "$stale" -eq 0 ]; then
  echo "All translations are current."
fi
exit "$stale"
