#!/bin/sh
# Makes the files of a release (spec 038): the signed update file, the full-flash image for a first
# USB flash, SHA256SUMS, manifest.json and notes.md. It refuses a release that is not signed, not
# signed by the key of the public key, or whose version is not the tree's. Run by release.yml on a
# tag; the maintainer can run it by hand too.
#
#   sh scripts/release.sh <version> (--key-file <path> | --key-env <NAME>) [--prebuilt <folder>]
#                         [--public-key-file <path>] [--recipe-key-file <path>] [--out <folder>]
#                         [--notes <file>] [--min-version <x.y.z>] [--repo <owner/name>]
#   sh scripts/release.sh <version> --build-only [--public-key-file <path>]
#                         [--recipe-key-file <path>] [--out <folder>] [--notes <file>]
#
# The image carries two public keys (spec 042 R1.5): the update key's (release/update-key.pub) and
# the recipe key's (release/recipe-key.pub, whose private half signs the script recipes and is never
# here). The script refuses to build without the second: no release until the recipe key is made,
# since a release that cannot install a recipe has no use for none.
#
# A bridging release (spec 040, docs/release.md) is declared by release/update-key.bridge in the
# tree: the image carries the new public key, the file is signed with the old private key
# (--key-file / --key-env), and the new private key is given too (--next-key-file or
# --next-key-env), so that boxes never move to a key nobody holds. --bridge-file overrides the
# declaration's path (for the tests).
#
# Two steps in release.yml, in two jobs: --build-only builds the radio image (no key, none is
# allowed) and writes app.bin and flash.bin to --out; then --prebuilt <folder> signs those two
# files (a key is required) instead of building. By hand, one command with a key does both.
#
# The signing key is never an argument. With --key-env it is read from the environment with
# printenv (the environment's value, never one of the script's own variables) and the variable is
# unset, before the script assigns anything: no tool the build runs (build.rs, cmake, pip, npm)
# can inherit it. It reaches a file only while solite-host needs it (the probe, the signing) and
# while grep searches the output and the log for it, in a folder of its own (umask 077) that holds
# no file during the build. The next key of a bridge is read the same way.
#
# Seams for the tests: SOLITE_RELEASE_BUILDER, SOLITE_HOST_BIN, SOLITE_RELEASE_TREE,
# SOLITE_RELEASE_TIME (spec 038 R1.10).

set -u
umask 077

# ---- the command line (shell built-ins only: the key may be in the environment) ----------------

# The keys come out of the environment first, before the script assigns any variable of its own
# (a name it uses, VERSION or OUT, is then no trouble: the unset happens before). printenv sees the
# environment only. The name is checked here and again below. --key-env gives the signing key,
# --next-key-env the next key of a bridging release (the first of each is read).
_ke_val=
_ne_val=
_ke_done=false
_ne_done=false
_ke_prev=
for _ke_arg in "$@"; do
  case $_ke_prev in
    --key-env | --next-key-env)
      case $_ke_arg in
        [A-Za-z_]*)
          case $_ke_arg in
            *[!A-Za-z0-9_]*) ;;
            *)
              if [ "$_ke_prev" = --key-env ]; then
                if ! $_ke_done; then
                  _ke_done=true
                  _ke_val=$(printenv "$_ke_arg" 2>/dev/null || :)
                  [ -z "$_ke_val" ] || unset "$_ke_arg"
                fi
              elif ! $_ne_done; then
                _ne_done=true
                _ne_val=$(printenv "$_ke_arg" 2>/dev/null || :)
                [ -z "$_ne_val" ] || unset "$_ke_arg"
              fi
              ;;
          esac
          ;;
      esac
      ;;
  esac
  _ke_prev=$_ke_arg
done
KEY=$_ke_val
NEXT_KEY=$_ne_val
unset _ke_prev _ke_arg _ke_val _ne_val _ke_done _ne_done

VERSION=
KEY_FILE=
KEY_ENV=
NEXT_KEY_FILE=
NEXT_KEY_ENV=
BRIDGE_FILE=
PUB_FILE=
RECIPE_PUB_FILE=
PREBUILT=
BUILD_ONLY=false
OUT=
NOTES_FILE=
MIN_VERSION=
REPO=
BAD_USAGE=

while [ $# -gt 0 ]; do
  case "$1" in
    --build-only) BUILD_ONLY=true; shift ;;
    --key-file | --key-env | --next-key-file | --next-key-env | --bridge-file | --public-key-file | --recipe-key-file | --prebuilt | --out | --notes | --min-version | --repo)
      if [ $# -lt 2 ]; then BAD_USAGE="$1 needs a value"; break; fi
      case "$1" in
        --key-file) KEY_FILE=$2 ;;
        --key-env) KEY_ENV=$2 ;;
        --next-key-file) NEXT_KEY_FILE=$2 ;;
        --next-key-env) NEXT_KEY_ENV=$2 ;;
        --bridge-file) BRIDGE_FILE=$2 ;;
        --public-key-file) PUB_FILE=$2 ;;
        --recipe-key-file) RECIPE_PUB_FILE=$2 ;;
        --prebuilt) PREBUILT=$2 ;;
        --out) OUT=$2 ;;
        --notes) NOTES_FILE=$2 ;;
        --min-version) MIN_VERSION=$2 ;;
        --repo) REPO=$2 ;;
      esac
      shift 2
      ;;
    -*) BAD_USAGE="unknown option $1"; break ;;
    *)
      if [ -n "$VERSION" ]; then BAD_USAGE="one version only"; break; fi
      VERSION=$1
      shift
      ;;
  esac
done

KEY_ENV_BAD=false
if [ -n "$KEY_ENV" ]; then
  case "$KEY_ENV" in
    [A-Za-z_]*) case "$KEY_ENV" in *[!A-Za-z0-9_]*) KEY_ENV_BAD=true ;; esac ;;
    *) KEY_ENV_BAD=true ;;
  esac
fi
NEXT_ENV_BAD=false
if [ -n "$NEXT_KEY_ENV" ]; then
  case "$NEXT_KEY_ENV" in
    [A-Za-z_]*) case "$NEXT_KEY_ENV" in *[!A-Za-z0-9_]*) NEXT_ENV_BAD=true ;; esac ;;
    *) NEXT_ENV_BAD=true ;;
  esac
fi

# ---- helpers ------------------------------------------------------------------------------------

KEYDIR=
WORK=
LOGDIR=
LOG=
KEEP_LOG=false
CHILD=

cleanup() {
  [ -z "$CHILD" ] || kill "$CHILD" 2>/dev/null
  CHILD=
  [ -n "$KEYDIR" ] && rm -rf "$KEYDIR"
  [ -n "$WORK" ] && rm -rf "$WORK"
  if [ -n "$LOGDIR" ] && ! $KEEP_LOG; then rm -rf "$LOGDIR"; fi
}
trap cleanup EXIT
# A process and all its descendants, deepest first, sent TERM ($1 the pid). Only the script's own
# descendants: never its process group, which holds whatever launched it.
kill_tree() {
  for _kid in $(pgrep -P "$1" 2>/dev/null); do kill_tree "$_kid"; done
  kill -TERM "$1" 2>/dev/null
}

# A signal: the script ignores further INT and TERM, sends TERM to the running command and its
# descendants (the builder's npm, cargo, cmake: background jobs of a non-interactive shell ignore
# SIGINT, so a Ctrl-C alone would leave them running), cleans up and exits with the shell's code
# for the signal.
on_signal() {
  trap '' INT TERM
  [ -z "$CHILD" ] || kill_tree "$CHILD"
  CHILD=
  cleanup
  exit "$1"
}
trap 'on_signal 130' INT
trap 'on_signal 143' TERM

# A refusal before the build: nothing was built, nothing is kept.
refuse() {
  echo "release: $1" >&2
  exit 2
}

case $0 in */*) _d=${0%/*} ;; *) _d=. ;; esac
ROOT=$(cd "$_d/.." && pwd -P) || { echo "release: cannot find the repository" >&2; exit 2; }
TREE=${SOLITE_RELEASE_TREE:-$ROOT}

fw_fail() { echo "$1"; exit 1; }
. "$ROOT/scripts/lib/firmware.sh"

sha256() {
  if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | cut -d' ' -f1; else sha256sum "$1" | cut -d' ' -f1; fi
}

# The 64 hex characters of a text, with spaces and line ends trimmed; empty when it is not that.
trim_hex() {
  _t=$(printf '%s' "$1" | tr -d ' \t\r\n')
  case "$_t" in
    *[!0-9a-fA-F]*) return 0 ;;
  esac
  [ "${#_t}" -eq 64 ] && printf '%s' "$_t"
  return 0
}

# x.y.z of numbers up to 65535: returns 0 when it is.
valid_version() {
  case "$1" in *[!0-9.]* | '' | .* | *. | *..*) return 1 ;; esac
  _rest=$1
  _n=0
  while :; do
    _p=${_rest%%.*}
    [ -n "$_p" ] && [ "${#_p}" -le 5 ] && [ "$_p" -le 65535 ] || return 1
    _n=$((_n + 1))
    case "$_rest" in *.*) _rest=${_rest#*.} ;; *) break ;; esac
  done
  [ "$_n" -eq 3 ]
}

# $1 < $2, both valid versions.
version_lower() {
  _a1=${1%%.*}; _ar=${1#*.}; _a2=${_ar%%.*}; _a3=${_ar#*.}
  _b1=${2%%.*}; _br=${2#*.}; _b2=${_br%%.*}; _b3=${_br#*.}
  [ "$_a1" -lt "$_b1" ] && return 0
  [ "$_a1" -gt "$_b1" ] && return 1
  [ "$_a2" -lt "$_b2" ] && return 0
  [ "$_a2" -gt "$_b2" ] && return 1
  [ "$_a3" -lt "$_b3" ]
}

# The absolute, symlink-free path of a folder, or of a path whose parent exists.
abs_path() {
  if [ -d "$1" ]; then (cd "$1" && pwd -P); return; fi
  _dir=$(dirname "$1")
  _base=$(basename "$1")
  if [ -d "$_dir" ]; then printf '%s/%s\n' "$(cd "$_dir" && pwd -P)" "$_base"; return; fi
  case "$1" in /*) printf '%s\n' "$1" ;; *) printf '%s/%s\n' "$(pwd -P)" "$1" ;; esac
}

# True when the folder or one above it holds a .git.
in_working_tree() {
  _d=$1
  while :; do
    [ -e "$_d/.git" ] && return 0
    [ "$_d" = / ] || [ -z "$_d" ] && return 1
    _d=$(dirname "$_d")
  done
}

# Runs a command with its whole output in the log, in the background so that a signal reaches the
# script at once (and kills the command) instead of after it.
run_logged() {
  "$@" >>"$LOG" 2>&1 &
  CHILD=$!
  wait "$CHILD"
  _rc=$?
  CHILD=
  return "$_rc"
}

# Searches a path for the key's text (both keys of a bridge) (the pattern file lives only for the search): 0 when found
# or when the search itself failed (closed on doubt), 1 when the key is not there.
key_in() {
  printf '%s\n' "$KEY" >"$KEYDIR/pattern"
  [ -z "$NEXT_KEY" ] || printf '%s\n' "$NEXT_KEY" >>"$KEYDIR/pattern"
  grep -rqiF -f "$KEYDIR/pattern" "$1" 2>/dev/null
  _g=$?
  rm -f "$KEYDIR/pattern"
  [ "$_g" -ne 1 ]
}

# A failure after the checks: the reason, the log's path and its last lines, unless the log holds
# the key (then it is removed and nothing of it is printed).
fail1() {
  echo "release: $1" >&2
  if [ -n "$LOG" ] && [ -f "$LOG" ] && [ -n "$KEYDIR" ]; then
    if { [ -n "$KEY" ] || [ -n "$NEXT_KEY" ]; } && key_in "$LOG"; then
      rm -rf "$LOGDIR"
      echo "release: the log held the key's text: removed" >&2
    else
      KEEP_LOG=true
      echo "release: log kept in $LOGDIR" >&2
      tail -n 40 "$LOG" >&2
    fi
  fi
  exit 1
}

# ---- R1.1 to R1.5: every check, before the build -----------------------------------------------

[ -z "$BAD_USAGE" ] || {
  echo "release: $BAD_USAGE" >&2
  echo "usage: sh scripts/release.sh <version> (--key-file <path> | --key-env <NAME>) [--prebuilt <folder>] [--public-key-file <path>] [--recipe-key-file <path>] [--out <folder>] [--notes <file>] [--min-version <x.y.z>] [--repo <owner/name>]" >&2
  echo "                             [--next-key-file <path> | --next-key-env <NAME>] [--bridge-file <path>]   (a bridging release)" >&2
  echo "       sh scripts/release.sh <version> --build-only [--public-key-file <path>] [--recipe-key-file <path>] [--out <folder>] [--notes <file>]" >&2
  exit 2
}
[ -n "$VERSION" ] || refuse "usage: sh scripts/release.sh <version> (--key-file <path> | --key-env <NAME> | --build-only) ..."
VERSION=${VERSION#v}
valid_version "$VERSION" || refuse "version $VERSION is not x.y.z of numbers up to 65535 (a release has no suffix)"

# R1.1: the version of the three files.
ws_version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' "$TREE/Cargo.toml" 2>/dev/null | head -n 1)
fw_pkg_version=$(sed -n '/^\[package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' "$TREE/firmware/Cargo.toml" 2>/dev/null | head -n 1)
ui_version=$(sed -n 's/^  "version": *"\([^"]*\)".*/\1/p' "$TREE/ui/package.json" 2>/dev/null | head -n 1)
[ "$ws_version" = "$VERSION" ] || refuse "Cargo.toml says version ${ws_version:-none}, not $VERSION"
[ "$fw_pkg_version" = "$VERSION" ] || refuse "firmware/Cargo.toml says version ${fw_pkg_version:-none}, not $VERSION"
[ "$ui_version" = "$VERSION" ] || refuse "ui/package.json says version ${ui_version:-none}, not $VERSION"

# R1.2: the key's source.
$KEY_ENV_BAD && refuse "--key-env $KEY_ENV: not a variable name (letters, digits and _, not starting with a digit)"
$NEXT_ENV_BAD && refuse "--next-key-env $NEXT_KEY_ENV: not a variable name (letters, digits and _, not starting with a digit)"
if [ -n "$NEXT_KEY_FILE" ] && [ -n "$NEXT_KEY_ENV" ]; then refuse "give --next-key-file or --next-key-env, not both"; fi
if [ -n "$KEY_FILE" ] && [ -n "$KEY_ENV" ]; then refuse "give --key-file or --key-env, not both"; fi
if $BUILD_ONLY; then
  [ -z "$KEY_FILE" ] && [ -z "$KEY_ENV" ] && [ -z "$NEXT_KEY_FILE" ] && [ -z "$NEXT_KEY_ENV" ] \
    || refuse "--build-only takes no key: the signing is the next step (--prebuilt)"
  [ -z "$PREBUILT" ] || refuse "--build-only and --prebuilt are two steps: give one"
elif [ -z "$KEY_FILE" ] && [ -z "$KEY_ENV" ]; then
  refuse "a release is always signed: give --key-file <path> or --key-env <NAME>"
fi
OUT=${OUT:-$ROOT/target/solite-release/$VERSION}
OUT_ABS=$(abs_path "$OUT")
if $BUILD_ONLY; then
  :
elif [ -n "$KEY_ENV" ]; then
  [ -n "$KEY" ] || refuse "the signing key is missing: $KEY_ENV is empty"
else
  [ -f "$KEY_FILE" ] && [ -r "$KEY_FILE" ] || refuse "the key file $KEY_FILE does not exist or cannot be read"
  key_dir=$(cd "$(dirname "$KEY_FILE")" && pwd -P)
  in_working_tree "$key_dir" && refuse "the key file $KEY_FILE is inside a git working tree: keep the signing key outside the repository"
  case "$key_dir/" in "$OUT_ABS"/*) refuse "the key file $KEY_FILE is inside the output folder $OUT" ;; esac
  KEY=$(cat "$KEY_FILE")
fi
if ! $BUILD_ONLY; then
  KEY=$(trim_hex "$KEY")
  [ -n "$KEY" ] || refuse "the signing key is not 64 hexadecimal characters (make one with solite-host --update-keygen)"
fi

# R1.3: the public key.
PUB_FILE=${PUB_FILE:-$ROOT/release/update-key.pub}
[ -f "$PUB_FILE" ] || refuse "the public key file $PUB_FILE is missing: make the key and commit its public key (docs/release.md)"
PUB=$(trim_hex "$(cat "$PUB_FILE")")
[ -n "$PUB" ] || refuse "the public key file $PUB_FILE is not 64 hexadecimal characters (docs/release.md)"
PUB8=$(printf '%s' "$PUB" | cut -c1-8)

# Spec 042 R1.5: the recipe key's public half. No release without it.
RECIPE_PUB_FILE=${RECIPE_PUB_FILE:-$ROOT/release/recipe-key.pub}
[ -f "$RECIPE_PUB_FILE" ] || refuse "the recipe key's public file $RECIPE_PUB_FILE is missing: no release until the recipe key is made and its public key committed (docs/release.md)"
RECIPE_PUB=$(trim_hex "$(cat "$RECIPE_PUB_FILE")")
[ -n "$RECIPE_PUB" ] || refuse "the recipe key's public file $RECIPE_PUB_FILE is not 64 hexadecimal characters (docs/release.md)"
# Public, so exported: the builder (the test seam's too) reads it where the real one reads the file.
export SOLITE_RECIPE_PUBLIC_KEY=$RECIPE_PUB

# Spec 040 R1.1 to R1.3: a bridging release is declared by a file in the tree; every check of it
# is here, before the build (the keys' probes follow, once the host program is there).
BRIDGE_FILE=${BRIDGE_FILE:-$TREE/release/update-key.bridge}
BRIDGE=false
PREVIOUS=
PREV8=
if [ -f "$BRIDGE_FILE" ]; then
  BRIDGE=true
  _bv=
  _bp=
  while IFS= read -r _bl || [ -n "$_bl" ]; do
    _bl=$(printf '%s' "$_bl" | tr -d '\r')
    _bk=${_bl#"${_bl%%[![:space:]]*}"}
    case $_bk in '' | '#'*) continue ;; esac
    _bname=${_bk%%[[:space:]]*}
    _bval=${_bk#"$_bname"}
    _bval=${_bval#"${_bval%%[![:space:]]*}"}
    _bval=${_bval%"${_bval##*[![:space:]]}"}
    case $_bname in
      version)
        [ -z "$_bv" ] || refuse "release/update-key.bridge: version given twice"
        case $_bval in '' | *[[:space:]]*) refuse "release/update-key.bridge: version needs one value" ;; esac
        _bv=$_bval
        ;;
      previous)
        [ -z "$_bp" ] || refuse "release/update-key.bridge: previous given twice"
        case $_bval in '' | *[[:space:]]*) refuse "release/update-key.bridge: previous needs one value" ;; esac
        _bp=$_bval
        ;;
      *) refuse "release/update-key.bridge: unknown line: $_bk" ;;
    esac
  done <"$BRIDGE_FILE"
  [ -n "$_bv" ] || refuse "release/update-key.bridge: version is missing"
  [ -n "$_bp" ] || refuse "release/update-key.bridge: previous is missing"
  [ "$_bv" = "$VERSION" ] \
    || refuse "release/update-key.bridge is for $_bv, not $VERSION: a bridge is one release; remove the file after it (docs/release.md)"
  PREVIOUS=$(trim_hex "$_bp")
  [ -n "$PREVIOUS" ] || refuse "release/update-key.bridge: previous is not 64 hex characters"
  _pl=$(printf '%s' "$PREVIOUS" | tr 'A-F' 'a-f')
  _nl=$(printf '%s' "$PUB" | tr 'A-F' 'a-f')
  [ "$_pl" != "$_nl" ] || refuse "release/update-key.bridge: previous is the current public key: nothing to rotate"
  PREVIOUS=$_pl
  PREV8=$(printf '%s' "$PREVIOUS" | cut -c1-8)
  unset _bv _bp _bl _bk _bname _bval _pl _nl
elif [ -n "$NEXT_KEY_FILE" ] || [ -n "$NEXT_KEY_ENV" ]; then
  refuse "--next-key-* is for a bridging release only (release/update-key.bridge)"
fi

# The next key of a bridge: required in a signing run, with the same rules as the signing key.
if $BRIDGE && ! $BUILD_ONLY; then
  if [ -n "$NEXT_KEY_ENV" ]; then
    [ -n "$NEXT_KEY" ] || refuse "the next key is missing: $NEXT_KEY_ENV is empty"
  elif [ -n "$NEXT_KEY_FILE" ]; then
    [ -f "$NEXT_KEY_FILE" ] && [ -r "$NEXT_KEY_FILE" ] || refuse "the next key file $NEXT_KEY_FILE does not exist or cannot be read"
    _nk_dir=$(cd "$(dirname "$NEXT_KEY_FILE")" && pwd -P)
    in_working_tree "$_nk_dir" && refuse "the next key file $NEXT_KEY_FILE is inside a git working tree: keep the keys outside the repository"
    case "$_nk_dir/" in "$OUT_ABS"/*) refuse "the next key file $NEXT_KEY_FILE is inside the output folder $OUT" ;; esac
    NEXT_KEY=$(cat "$NEXT_KEY_FILE")
  else
    refuse "a bridging release needs the next key (--next-key-file or --next-key-env): boxes must not move to a key nobody holds"
  fi
  NEXT_KEY=$(trim_hex "$NEXT_KEY")
  [ -n "$NEXT_KEY" ] || refuse "the next key is not 64 hexadecimal characters (make one with solite-host --update-keygen)"
  [ "$NEXT_KEY" != "$KEY" ] || refuse "the next key and the signing key are the same: a bridge is signed with the old key"
fi

# R1.4: the notes.
WORK=$(mktemp -d "${TMPDIR:-/tmp}/solite-release-work.XXXXXX") || refuse "cannot make a temporary folder"
NOTES=$WORK/notes.md
if [ -n "$NOTES_FILE" ]; then
  [ -f "$NOTES_FILE" ] && [ -s "$NOTES_FILE" ] || refuse "the notes file $NOTES_FILE is missing or empty"
  cp "$NOTES_FILE" "$NOTES"
else
  awk -v v="$VERSION" '
    BEGIN { h = "## " v }
    /^## / { if (on) exit; on = ($0 == h || index($0, h " ") == 1); next }
    on { lines[++n] = $0 }
    END {
      first = 1; last = n
      while (first <= last && lines[first] ~ /^[ \t\r]*$/) first++
      while (last >= first && lines[last] ~ /^[ \t\r]*$/) last--
      for (i = first; i <= last; i++) print lines[i]
    }' "$TREE/docs/release-notes.md" >"$NOTES" 2>/dev/null
  [ -s "$NOTES" ] || refuse "no release notes for $VERSION in docs/release-notes.md"
fi

# R1.5: the other options.
if [ -e "$OUT" ]; then
  [ -d "$OUT" ] || refuse "the output $OUT exists and is not a folder"
  [ -z "$(ls -A "$OUT")" ] || refuse "the output folder $OUT is not empty: a release is never mixed with a previous run"
fi
if [ -n "$MIN_VERSION" ]; then
  valid_version "$MIN_VERSION" || refuse "--min-version $MIN_VERSION is not x.y.z"
  version_lower "$MIN_VERSION" "$VERSION" || refuse "--min-version $MIN_VERSION must be lower than $VERSION"
fi
REPO=${REPO:-${GITHUB_REPOSITORY:-mchacher/solite}}
if [ -n "$PREBUILT" ]; then
  [ -d "$PREBUILT" ] || refuse "the prebuilt folder $PREBUILT does not exist"
  [ -s "$PREBUILT/app.bin" ] || refuse "the prebuilt folder $PREBUILT has no app.bin"
  [ -s "$PREBUILT/flash.bin" ] || refuse "the prebuilt folder $PREBUILT has no flash.bin"
fi

# R1.10: the seams.
BUILDER=${SOLITE_RELEASE_BUILDER:-}
if [ -n "$BUILDER" ]; then
  [ "${GITHUB_ACTIONS:-}" != true ] || refuse "SOLITE_RELEASE_BUILDER is a test seam: refused in GitHub Actions"
  [ -f "$BUILDER" ] || refuse "the builder $BUILDER does not exist"
fi

KEYDIR=$(mktemp -d "${TMPDIR:-/tmp}/solite-release-key.XXXXXX") || refuse "cannot make the key folder"
LOGDIR=$(mktemp -d "${TMPDIR:-/tmp}/solite-release-log.XXXXXX") || refuse "cannot make the log folder"
LOG=$LOGDIR/release.log
: >"$LOG"

echo "release: version $VERSION checked"
! $BRIDGE || echo "release: bridging release (previous key $PREV8, the image carries $PUB8)"
[ -z "$BUILDER" ] || [ -n "$PREBUILT" ] || echo "release: builder $BUILDER (a test seam)"

# A prebuilt image must carry the committed public key: the check that stands for the build's.
if [ -n "$PREBUILT" ]; then
  grep -aqF "$PUB" "$PREBUILT/app.bin" || {
    echo "release: the prebuilt image does not carry $PUB_FILE's key" >&2
    exit 1
  }
  grep -aqF "$RECIPE_PUB" "$PREBUILT/app.bin" || {
    echo "release: the prebuilt image does not carry $RECIPE_PUB_FILE's key" >&2
    exit 1
  }
  echo "release: prebuilt image from $PREBUILT"
fi

# The host program: the signing and the checking are its (a build-only run does neither).
if ! $BUILD_ONLY; then
  if [ -n "${SOLITE_HOST_BIN:-}" ]; then
    HOSTBIN=$SOLITE_HOST_BIN
    echo "release: host $HOSTBIN (a test seam)"
  else
    (cd "$ROOT" && run_logged cargo build -p solite-host --release) || fail1 "cargo build -p solite-host --release failed"
    HOSTBIN=$ROOT/target/release/solite-host
  fi
  [ -x "$HOSTBIN" ] || fail1 "the host program $HOSTBIN is not there"
fi

# Runs solite-host with the key in its own file for the time of the command only.
with_key_value() {
  _kv=$1
  shift
  printf '%s\n' "$_kv" >"$KEYDIR/key"
  "$@" --key "$KEYDIR/key" >>"$LOG" 2>&1
  _kr=$?
  rm -f "$KEYDIR/key"
  return "$_kr"
}
with_key() {
  with_key_value "$KEY" "$@"
}

# ---- R1.7: the probe ----------------------------------------------------------------------------

if ! $BUILD_ONLY; then
  printf 'solite-probe-key' >"$WORK/probe.bin"
  with_key "$HOSTBIN" --update-sign "$WORK/probe.bin" --version "$VERSION" --out "$WORK/probe.sup" \
    || fail1 "the probe could not be signed"
  if $BRIDGE; then
    # A bridge is signed with the old key: the probe is verified against `previous`.
    "$HOSTBIN" --update-verify "$WORK/probe.sup" --public-key "$PREVIOUS" >>"$LOG" 2>&1 \
      || fail1 "the signing key does not match the bridge's previous key: nothing is published"
    echo "release: key matches the bridge's previous key $PREV8"
    # The next key signs nothing that is published: only this probe, against the committed key.
    with_key_value "$NEXT_KEY" "$HOSTBIN" --update-sign "$WORK/probe.bin" --version "$VERSION" --out "$WORK/probe-next.sup" \
      || fail1 "the probe could not be signed with the next key"
    "$HOSTBIN" --update-verify "$WORK/probe-next.sup" --public-key "$PUB" >>"$LOG" 2>&1 \
      || fail1 "the next key does not match $PUB_FILE: nothing is published"
    echo "release: next key matches the public key $PUB8"
  else
    "$HOSTBIN" --update-verify "$WORK/probe.sup" --public-key "$PUB" >>"$LOG" 2>&1 \
      || fail1 "the signing key does not match $PUB_FILE: nothing is published"
    echo "release: key matches the public key $PUB8"
  fi
fi

# ---- R1.6: the build ----------------------------------------------------------------------------

# ota_0's size, from the partition table's text (no ESP-IDF tool).
SLOT_RAW=$(awk -F, '{ gsub(/[ \t]/, "") } $1 == "ota_0" { print $5 }' "$ROOT/firmware/partitions.csv")
[ -n "$SLOT_RAW" ] || fail1 "no ota_0 slot in firmware/partitions.csv"
SLOT_BYTES=$(fw_bytes "$SLOT_RAW")

# The script's own builder: $1 the version, $2 the public key in hex, $3 the folder for app.bin and
# flash.bin. Its output is the log's.
own_builder() {
  _v=$1 _k=$2 _dir=$3
  # Its own folder on a computer, so the release's key never lands in target/radio, which
  # box.sh flash takes (D63); the release workflow's runner shares target/radio's cache (D84).
  RELEASE_TARGET=${SOLITE_RELEASE_TARGET_DIR:-target/release-radio}
  [ -d "$ROOT/ui/node_modules" ] || { echo "ui/node_modules is missing: cd ui && npm ci"; return 1; }
  [ -f "$HOME/export-esp.sh" ] || { echo "the ESP32 toolchain is not set up (espup install, see README.md)"; return 1; }
  (cd "$ROOT/ui" && npm run build) || return 1
  (
    . "$HOME/export-esp.sh"
    cd "$ROOT/firmware" || exit 1
    # ESP-IDF and its Xtensa tools first, alone: on a machine without firmware/.embuild (the
    # release workflow's runner), QuickJS's bindings (vendor/rquickjs-sys) are made in parallel
    # with ESP-IDF's download and need its headers (the build of v0.2.0 failed so). Nothing to do
    # when they are there.
    SOLITE_UPDATE_PUBLIC_KEY=$_k SOLITE_RECIPE_PUBLIC_KEY=$RECIPE_PUB CARGO_TARGET_DIR=$RELEASE_TARGET \
      ESP_IDF_SDKCONFIG_DEFAULTS="sdkconfig.defaults;sdkconfig.radio.defaults" \
      cargo build --release -p esp-idf-sys || exit 1
    SOLITE_UPDATE_PUBLIC_KEY=$_k SOLITE_RECIPE_PUBLIC_KEY=$RECIPE_PUB CARGO_TARGET_DIR=$RELEASE_TARGET \
      ESP_IDF_SDKCONFIG_DEFAULTS="sdkconfig.defaults;sdkconfig.radio.defaults" \
      cargo build --release --features radio || exit 1
    _image_dir=$ROOT/firmware/$RELEASE_TARGET/xtensa-esp32-espidf/release
    _elf=$_image_dir/solite-firmware
    [ -f "$_elf" ] || fw_fail "$_elf not found after the build."
    grep -aqF 'radio: uart ' "$_elf" || fw_fail "the build is not the radio image (feature radio)."
    grep -aqF 'solite-ui ' "$_elf" || fw_fail "the radio build has no embedded interface (no solite-ui marker)."
    grep -aqF "$_k" "$_elf" || fw_fail "the prebuilt image does not carry $PUB_FILE's key"
    grep -aqF "$RECIPE_PUB" "$_elf" || fw_fail "the prebuilt image does not carry $RECIPE_PUB_FILE's key"
    grep -aqs '^CONFIG_UART_ISR_IN_IRAM=y' "$_image_dir"/build/esp-idf-sys-*/out/sdkconfig \
      || fw_fail "the radio build was not made with sdkconfig.radio.defaults (CONFIG_UART_ISR_IN_IRAM=y is not in its sdkconfig)."
    fw_find_tools
    fw_build_table "$ROOT/firmware/partitions.csv" "$WORK/table.bin"
    fw_find_bootloader "$_image_dir"
    fw_elf2image "$_elf" "$_dir/app.bin"
    $ESPTOOL --chip esp32 merge_bin -o "$_dir/flash.bin" --flash_mode keep --flash_freq keep --flash_size keep \
      0x1000 "$BOOTLOADER" 0x8000 "$WORK/table.bin" "$APP_OFFSET" "$_dir/app.bin" || fw_fail "esptool merge_bin failed."
  )
}

BUILD=$WORK/build
mkdir -p "$BUILD"
if [ -n "$PREBUILT" ]; then
  cp "$PREBUILT/app.bin" "$BUILD/app.bin" && cp "$PREBUILT/flash.bin" "$BUILD/flash.bin" \
    || fail1 "cannot read the prebuilt files"
elif [ -n "$BUILDER" ]; then
  run_logged sh "$BUILDER" "$VERSION" "$PUB" "$BUILD" || fail1 "the builder failed"
else
  run_logged own_builder "$VERSION" "$PUB" "$BUILD" || fail1 "the build failed"
fi
[ -s "$BUILD/app.bin" ] || fail1 "the build wrote no app.bin"
[ -s "$BUILD/flash.bin" ] || fail1 "the build wrote no flash.bin"
APP_BYTES=$(wc -c <"$BUILD/app.bin" | tr -d ' ')
[ "$APP_BYTES" -le "$SLOT_BYTES" ] || fail1 "app.bin ($APP_BYTES bytes) is larger than the ota_0 slot ($SLOT_BYTES bytes)"
echo "release: radio image built ($((APP_BYTES / 1024)) KB, $((APP_BYTES * 100 / SLOT_BYTES)) % of the $((SLOT_BYTES / 1024)) KB ota_0 slot)"

# A build-only run stops here: app.bin and flash.bin, nothing signed, nothing secret.
if $BUILD_ONLY; then
  chmod 755 "$BUILD" && chmod 644 "$BUILD"/* || fail1 "cannot set the modes of the built files"
  if [ -d "$OUT" ]; then rmdir "$OUT" || fail1 "cannot replace the empty folder $OUT"; fi
  mkdir -p "$(dirname "$OUT")" || fail1 "cannot make the folder of $OUT"
  mv "$BUILD" "$OUT" || fail1 "cannot move the built files to $OUT"
  echo "release: built, not signed (--build-only)"
  echo "release: $OUT"
  exit 0
fi

# ---- R1.7: sign, then verify --------------------------------------------------------------------

STAGE=$WORK/stage
mkdir -p "$STAGE"
SUP=solite-$VERSION.sup
FLASH=solite-$VERSION-flash.bin
mv "$BUILD/flash.bin" "$STAGE/$FLASH" || fail1 "cannot stage the full-flash image"
with_key "$HOSTBIN" --update-sign "$BUILD/app.bin" --version "$VERSION" --out "$STAGE/$SUP" \
  || fail1 "the signing failed"
if [ -z "$BUILDER" ] && grep -qF 'signed: no application descriptor' "$LOG"; then
  # The probe's own line is expected: only the signing after the build is checked.
  [ "$(grep -cF 'signed: no application descriptor' "$LOG")" -le 1 ] \
    || fail1 "the image has no application descriptor: its version was not checked"
fi
if $BRIDGE; then
  "$HOSTBIN" --update-verify "$STAGE/$SUP" --public-key "$PREVIOUS" >>"$LOG" 2>&1 \
    || fail1 "the signing key does not match the bridge's previous key: nothing is published"
  if "$HOSTBIN" --update-verify "$STAGE/$SUP" --public-key "$PUB" >>"$LOG" 2>&1; then
    fail1 "the bridge's file verifies with the new key: nothing is published"
  fi
  echo "release: key bridge: signed with $PREV8, the image carries $PUB8"
else
  "$HOSTBIN" --update-verify "$STAGE/$SUP" --public-key "$PUB" >>"$LOG" 2>&1 \
    || fail1 "the signing key does not match $PUB_FILE: nothing is published"
  echo "release: signed and verified (public key $PUB8)"
fi

# ---- R1.8: manifest, checksums, notes -----------------------------------------------------------

RELEASED_AT=${SOLITE_RELEASE_TIME:-$(date -u +%Y-%m-%dT%H:%M:%SZ)}
python3 - "$STAGE/manifest.json" "$VERSION" "$RELEASED_AT" "$NOTES" "$MIN_VERSION" "$SUP" \
  "$(wc -c <"$STAGE/$SUP" | tr -d ' ')" "$(sha256 "$STAGE/$SUP")" "$REPO" <<'PY' >>"$LOG" 2>&1 || fail1 "cannot write manifest.json"
import json, sys

out, version, released_at, notes_file, min_version, name, size, digest, repo = sys.argv[1:]
with open(notes_file, encoding="utf-8") as f:
    notes = f.read().rstrip("\n")
manifest = {
    "format": 1,
    "version": version,
    "releasedAt": released_at,
    "notes": notes,
    "minVersion": min_version or None,
    "image": {
        "name": name,
        "size": int(size),
        "sha256": digest,
        "url": f"https://github.com/{repo}/releases/download/v{version}/{name}",
    },
}
with open(out, "w", encoding="utf-8") as f:
    json.dump(manifest, f, indent=2)
    f.write("\n")
PY
if [ -n "$NOTES_FILE" ]; then cp "$NOTES_FILE" "$STAGE/notes.md"; else cp "$NOTES" "$STAGE/notes.md"; fi
(
  cd "$STAGE" || exit 1
  for n in $(printf '%s\n' manifest.json "$FLASH" "$SUP" | LC_ALL=C sort); do
    printf '%s  %s\n' "$(sha256 "$n")" "$n"
  done >SHA256SUMS
) || fail1 "cannot write SHA256SUMS"

# The key's text must be in none of what is published, nor in the log (it cannot be by
# construction); fail1 removes a log that holds it.
if key_in "$STAGE"; then
  rm -rf "$STAGE"
  fail1 "the signing key's text was found in the output: nothing is kept"
fi
if key_in "$LOG"; then
  rm -rf "$STAGE"
  fail1 "the signing key's text was found in the log: nothing is kept"
fi

# ---- the move: all or nothing -------------------------------------------------------------------

# The umask kept the key's files private; the release's files are for everyone.
chmod 755 "$STAGE" && chmod 644 "$STAGE"/* || fail1 "cannot set the modes of the release's files"

if [ -d "$OUT" ]; then rmdir "$OUT" || fail1 "cannot replace the empty folder $OUT"; fi
mkdir -p "$(dirname "$OUT")" || fail1 "cannot make the folder of $OUT"
mv "$STAGE" "$OUT" || fail1 "cannot move the release to $OUT"
echo "release: $OUT"
