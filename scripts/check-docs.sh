#!/bin/sh
# Keeps the docs current and free of the sentences that drift. Installed by /start-software-project
# as scripts/check-docs.sh.
#
#   sh scripts/check-docs.sh                 run by the validate command:
#                                            - every specs/NNN-* folder has its three files
#                                              (spec.md, architecture.md, plan.md) and its row in
#                                              docs/specs-index.md;
#                                            - the docs that describe the tool as it is (living
#                                              spec, architecture, design system, README) hold no
#                                              words of the future ("not yet", "planned", "for
#                                              now"...) and no status line: what comes next, and
#                                              how far the work has got, live in docs/roadmap.md;
#                                            - no doc, CLAUDE.md, skill or agent names one
#                                              computer's paths (a drive letter, /mnt/<drive>/,
#                                              /Users/<name>/, Git Bash's /<drive>/)
#   sh scripts/check-docs.sh commit-msg FILE run by the commit-msg git hook: a feat or fix commit
#                                            that changes code also changes docs/living-spec/,
#                                            or says why not with a line
#                                            "Living-Spec: none — <reason>"
#
# It finds words, not truth: a sentence in the present that a change made false is found by the
# search "what this change makes false" of the feature and fix skills, and by the code review.
# "Code" is any file outside the docs: docs/, specs/, inputs/, .claude/, .githooks/ and *.md.
# Text in "quotes" or `backticks` is not read for the future: it quotes the tool (a message shown,
# a command). A paragraph that describes the present but trips the rule holds
# <!-- check-docs: allow -->.

# This project's own marks of the future, as an extended regular expression matched with its case,
# for example its roadmap's increment ids: '[A-C][0-9]+' for milestones A to C ("available in B3").
# Only the milestones' letters: decisions are D1, D2..., and a steel grade or a screw size must pass.
# Empty: the common words only.
PROJECT_FUTURE='[ABCEFGHIJ][0-9]+'

if [ "$1" = "commit-msg" ]; then
  msg="$2"
  head -n 1 "$msg" | grep -qE '^(feat|fix)(\([^)]*\))?!?: ' || exit 0

  files=$(git diff --cached --name-only)
  code=$(printf '%s\n' "$files" | grep -vE '^(docs|specs|inputs|\.claude|\.githooks)/|\.md$' | grep -v '^$')
  [ -z "$code" ] && exit 0
  printf '%s\n' "$files" | grep -q '^docs/living-spec/' && exit 0
  grep -qE '^Living-Spec: none *(—|-) *[^ ]' "$msg" && exit 0

  echo "check-docs: this $(head -n 1 "$msg" | cut -d: -f1) commit changes code but not docs/living-spec/." >&2
  echo "Update the living spec in the same commit, or, if the tool's behaviour does not change," >&2
  echo "add a line to the message:  Living-Spec: none — <the reason, required>" >&2
  exit 1
fi

status=0
for dir in specs/[0-9][0-9][0-9]-*/; do
  [ -d "$dir" ] || continue
  num=$(basename "$dir" | cut -c1-3)
  for f in spec.md architecture.md plan.md; do
    if [ ! -f "$dir$f" ]; then
      echo "check-docs: $dir has no $f (a spec has spec.md, architecture.md and plan.md)." >&2
      status=1
    fi
  done
  if ! grep -qE "^\| *$num *\|" docs/specs-index.md 2>/dev/null; then
    echo "check-docs: $dir has no row in docs/specs-index.md." >&2
    status=1
  fi
done

# The present only. A sentence about the future becomes false the day that future comes, and the
# change that brings it does not know the sentence exists. Words that also describe the present
# ("not yet started", "planned maintenance") are narrowed to their future use.
future='(not|no [a-z ]{0,40}) yet *([.:;)]|$)|not yet (built|available|implemented|supported|possible|done|in place)'
future="$future|yet to|\(planned|(is|are|was|were) planned|\| *planned *\||for now|coming soon"
future="$future|in a (later|future|next) [a-z]+|later (increment|milestone|spec|version|step)s?"
future="$future|next (increment|milestone)s?|for later|later:|; later|in the future|once it exists"
future="$future|to be (built|added|done|decided|defined|measured)|will come"
marks='TODO|TBD|FIXME'  # matched with their case: "the todo list" passes
[ -n "$PROJECT_FUTURE" ] && marks="$marks|$PROJECT_FUTURE"
tab=$(printf '\t')

# One record per paragraph (a list item, a table row, a heading, a run of prose lines, a diagram):
# its first line number, a tab, its lines joined, without what is in quotes or backticks.
paragraphs() {
  awk '
    function flush() { if (text != "") print start "\t" text; text = "" }
    /^[ \t]*$/ { flush(); next }
    /^[ \t]*([-*|#>]|[0-9]+\.|```)/ { flush() }
    { if (text == "") { start = NR; text = $0 } else text = text " " $0 }
    END { flush() }
  ' "$1" | sed -e 's/"[^"]*"//g' -e 's/`[^`]*`//g'
}

for f in $(find docs/living-spec -name '*.md' 2>/dev/null | sort) docs/architecture.md \
  docs/design-system.md README.md; do
  [ -f "$f" ] || continue
  found=$(paragraphs "$f" | while IFS="$tab" read -r n text; do
    case "$text" in (*"check-docs: allow"*) continue ;; esac
    hit=$(printf '%s\n' "$text" | grep -oiE "(^|[^[:alnum:]])($future)" | head -n 1)
    [ -z "$hit" ] && hit=$(printf '%s\n' "$text" | grep -oE "(^|[^[:alnum:]])($marks)([^[:alnum:]]|$)" | head -n 1)
    [ -n "$hit" ] && printf '%s: "%s" in: %s\n' "$n" "$(echo $hit)" "$(sed -n "${n}p" "$f" | cut -c1-90)"
  done)
  case "$f" in
    README.md | docs/architecture.md)  # how far the work has got: the roadmap's job
      progress=$(grep -niE '^[> ]*\**status\**:|^#+ *status *$|\| *status *\|[[:space:]]*$' "$f" \
        | grep -v 'check-docs: allow')
      found=$(printf '%s\n%s\n' "$found" "$progress" | grep -v '^$') ;;
  esac
  if [ -n "$found" ]; then
    echo "check-docs: $f speaks of the future or of how far the work has got:" >&2
    printf '%s\n' "$found" | sed 's/^/  /' >&2
    echo "  What comes next goes to docs/roadmap.md, and the sentence says what the tool does today." >&2
    echo "  If the paragraph does describe the present, rephrase it, or add <!-- check-docs: allow -->." >&2
    status=1
  fi
done

# Paths of one computer: the project moves between computers and people. Only the files git keeps
# or could keep are read, as text: no binary, no ignored file (a worktree, settings.local.json).
drive='(^|[^[:alnum:]])[A-Za-z]:[\\/][A-Za-z_]|/mnt/[a-z]/|/Users/[A-Za-z]|(^|[[:space:]`(])/[a-z]/[A-Za-z_]'
paths=$(git grep -n -I --untracked -E "$drive" -- docs README.md CLAUDE.md .claude inputs/README.md \
  ':(exclude).claude/settings.local.json' 2>/dev/null | grep -v 'check-docs: allow')
if [ -n "$paths" ]; then
  echo "check-docs: these lines name one computer's paths; write them from the project's folder" >&2
  echo "(scripts/..., ../other-repository) so they work on every computer:" >&2
  printf '%s\n' "$paths" | sed 's/^/  /' >&2
  status=1
fi
# docs/roadmap.html, the roadmap for its users: dated no earlier than the last spec of the index.
if [ -f docs/specs-index.md ]; then
  last=$(grep -oE '^\| *[0-9]{3} *\|.*\| *(20[0-9]{2}-[0-9]{2}-[0-9]{2}) *\|' docs/specs-index.md \
    | grep -oE '20[0-9]{2}-[0-9]{2}-[0-9]{2}' | sort | tail -n 1)
  shown=$(grep -oE 'Updated 20[0-9]{2}-[0-9]{2}-[0-9]{2}' docs/roadmap.html 2>/dev/null | grep -oE '20[0-9-]+')
  if [ -z "$shown" ] || [ "$shown" \< "$last" ]; then
    echo "check-docs: docs/roadmap.html is dated ${shown:-nowhere}, before the last spec ($last)." >&2
    echo "  Update its rows to what is built and planned, and its 'Updated <date>' line." >&2
    status=1
  fi
  # Every spec of the index is in a row's "Specs" cell (004–009 counts for 004 to 009).
  covered=$(grep -oE '<td>[0-9][0-9, –-]*</td></tr>' docs/roadmap.html 2>/dev/null \
    | sed -e 's/<[^>]*>//g' -e 's/–/-/g' | tr ',' '\n' | awk -F- '
      { gsub(/ /, ""); if ($0 == "") next; a = $1 + 0; b = ($2 == "" ? a : $2 + 0);
        for (k = a; k <= b; k++) printf "%03d\n", k }')
  for num in $(grep -oE '^\| *[0-9]{3} *\|' docs/specs-index.md | grep -oE '[0-9]{3}'); do
    printf '%s\n' "$covered" | grep -qx "$num" && continue
    echo "check-docs: spec $num is in no row of docs/roadmap.html (its Specs cell)." >&2
    status=1
  done
fi

exit $status
