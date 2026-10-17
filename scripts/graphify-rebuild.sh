#!/bin/sh
# Rebuild this repo's graphify graphs after git changes.
# Official `graphify hook install` only sees one repo-root graphify-out/.
# Usage: graphify-rebuild.sh commit|checkout|merge|rewrite [hook args...]
# Always exits 0. Skip one run with GRAPHIFY_SKIP_HOOK=1.

event="${1:-commit}"
if [ "$#" -gt 0 ]; then
  shift
fi

if [ "${GRAPHIFY_SKIP_HOOK:-0}" = "1" ]; then
  exit 0
fi

root=$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
cd "$root" || exit 0

if ! command -v graphify >/dev/null 2>&1; then
  for c in "$HOME/.local/bin/graphify" /usr/local/bin/graphify; do
    if [ -x "$c" ]; then
      PATH="$(dirname "$c"):$PATH"
      export PATH
      break
    fi
  done
fi
if ! command -v graphify >/dev/null 2>&1; then
  echo "[graphify] not on PATH; skip rebuild" >&2
  exit 0
fi

gitdir=$(cd "$(git rev-parse --git-dir 2>/dev/null)" 2>/dev/null && pwd)
commondir=$(cd "$(git rev-parse --git-common-dir 2>/dev/null)" 2>/dev/null && pwd)
if [ -n "$commondir" ] && [ "$gitdir" != "$commondir" ]; then
  exit 0
fi

changed=""
case "$event" in
  commit)
    [ -d "$gitdir/rebase-merge" ] && exit 0
    [ -d "$gitdir/rebase-apply" ] && exit 0
    [ -f "$gitdir/MERGE_HEAD" ] && exit 0
    changed=$(git -c core.quotePath=false diff --name-only HEAD~1 HEAD 2>/dev/null || true)
    ;;
  checkout)
    prev="${1:-}"
    new="${2:-}"
    flag="${3:-}"
    [ "$flag" = "1" ] || exit 0
    [ -n "$prev" ] && [ "$prev" = "$new" ] && exit 0
    [ -d "$gitdir/rebase-merge" ] && exit 0
    [ -d "$gitdir/rebase-apply" ] && exit 0
    [ -f "$gitdir/MERGE_HEAD" ] && exit 0
    [ -f "$gitdir/CHERRY_PICK_HEAD" ] && exit 0
    if [ -n "$prev" ] && [ -n "$new" ]; then
      changed=$(git -c core.quotePath=false diff --name-only "$prev" "$new" 2>/dev/null || true)
    else
      changed="."
    fi
    ;;
  merge)
    [ -d "$gitdir/rebase-merge" ] && exit 0
    [ -d "$gitdir/rebase-apply" ] && exit 0
    [ -f "$gitdir/CHERRY_PICK_HEAD" ] && exit 0
    changed=$(git -c core.quotePath=false diff --name-only ORIG_HEAD HEAD 2>/dev/null || true)
    ;;
  rewrite)
    [ "${1:-}" = "rebase" ] || exit 0
    changed="."
    ;;
  *)
    exit 0
    ;;
esac

[ -n "$changed" ] || exit 0

pkgs="$root/.githooks/packages"
[ -f "$pkgs" ] || exit 0

log="${HOME}/.cache/graphify-rebuild.log"
mkdir -p "$(dirname "$log")" || exit 0

(
  match_dir() {
    file="$1"
    case "$file" in
      */graphify-out/*|graphify-out/*) return 1 ;;
    esac
    best="."
    best_len=0
    while IFS=$(printf '\t') read -r prefix dir; do
      [ -n "$prefix" ] || continue
      case "$prefix" in \#*) continue ;; esac
      if [ "$prefix" = "." ]; then
        continue
      fi
      case "$file" in
        "$prefix"|"$prefix"/*)
          n=$(printf '%s' "$prefix" | wc -c)
          if [ "$n" -gt "$best_len" ]; then
            best="$dir"
            best_len=$n
          fi
          ;;
      esac
    done < "$pkgs"
    if [ "$best_len" -eq 0 ]; then
      while IFS=$(printf '\t') read -r prefix dir; do
        if [ "$prefix" = "." ]; then
          best="$dir"
          best_len=1
          break
        fi
      done < "$pkgs"
    fi
    [ "$best_len" -gt 0 ] || return 1
    printf '%s\n' "$best"
  }

  dirs=""
  if [ "$changed" = "." ]; then
    while IFS=$(printf '\t') read -r prefix dir; do
      [ -n "$dir" ] || continue
      case "$prefix" in \#*) continue ;; esac
      dirs="$dirs $dir"
    done < "$pkgs"
  else
    while IFS= read -r f; do
      [ -n "$f" ] || continue
      d=$(match_dir "$f") || continue
      case " $dirs " in
        *" $d "*) ;;
        *) dirs="$dirs $d" ;;
      esac
    done <<EOF
$changed
EOF
  fi

  [ -n "$dirs" ] || exit 0
  echo "[graphify] $event $(date -Is) dirs=$dirs"
  for dir in $dirs; do
    if [ ! -f "$dir/graphify-out/graph.json" ]; then
      echo "[graphify] skip $dir (no graph yet)"
      continue
    fi
    echo "[graphify] updating $dir"
    if ! (cd "$dir" && graphify update .); then
      echo "[graphify] update failed for $dir"
    fi
  done
) >> "$log" 2>&1 &

echo "[graphify] launching background rebuild (log: $log)"
exit 0
