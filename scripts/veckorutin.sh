#!/usr/bin/env bash
# Hub weekly routine (Mondays, on tincan only: idle and build pins must come from the same machine every week).
# Runs garden.py --write, monday.py and mutanter.py on a fresh clone of main and pushes the reports to the branch
# `halsorutin` (docs/GARDEN.md + docs/halsorutin/*). Report only: nothing here edits main or opens a PR.
# Env: HUB_REPO_URL (default origin of the checkout this file lives in), HUB_HUBD (hubd binary for the idle pin; default: a release build of this clone, kept in the same target dir every week).
set -uo pipefail
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/bin:$PATH"   # a timer or a plain ssh shell has no cargo on PATH
command -v cargo >/dev/null || { echo "cargo not found: install rustup or set PATH"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
URL="${HUB_REPO_URL:-$(git -C "$HERE" remote get-url origin)}"
W="$(mktemp -d "${TMPDIR:-/tmp}/hub-veckorutin-XXXXXX")"
trap 'rm -rf "$W"' EXIT
export CARGO_TARGET_DIR="${HUB_ROUTINE_TARGET:-$HOME/.cache/hub-veckorutin/target}"   # same cache every week, or build_s compares cache states
export CARGO_INCREMENTAL=0
git clone --quiet --depth 50 "$URL" "$W/repo" || exit 1
cd "$W/repo"
OUT="$W/halsorutin"; mkdir -p "$OUT"
python3 -I scripts/garden.py . --write   # docs/GARDEN.md in this clone
python3 -I scripts/mutanter.py . --out-dir "$OUT"
python3 -I scripts/adapters.py . --out-dir "$OUT" || echo "adapters.py: an engine adapter pin is behind npm (see adapters-*.md)"
# monday.py compares with the newest earlier pins-*.json in --out-dir: seed it from the report branch
git fetch --quiet origin halsorutin 2>/dev/null && git archive FETCH_HEAD docs/halsorutin 2>/dev/null | tar -x -C "$W" && cp -n "$W"/docs/halsorutin/* "$OUT"/ 2>/dev/null
if [ -z "${HUB_HUBD:-}" ] && cargo build --release -p hubd --quiet; then HUB_HUBD="$CARGO_TARGET_DIR/release/hubd"; fi
python3 -I scripts/monday.py . --out-dir "$OUT" ${HUB_HUBD:+--hubd "$HUB_HUBD"} --repo "$(echo "$URL" | sed -E 's#.*github.com[:/]##; s#\.git$##')" || echo "monday.py: a pin is red or could not be measured (see pins-*.json)"
# the report branch is main plus this week's reports (old reports are carried along in $OUT)
mkdir -p docs/halsorutin && cp "$OUT"/* docs/halsorutin/
git checkout --quiet -B halsorutin
git add docs/GARDEN.md docs/halsorutin
git -c user.name="Hub veckorutin" -c user.email="hub@localhost" commit --quiet -m "Veckorutin $(date +%F)" && git push --quiet --force origin halsorutin
