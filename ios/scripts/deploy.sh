#!/bin/bash
# Builds the iPhone app and installs it on the phone over Wi-Fi (or the cable), no taps needed.
# The phone must have been connected by cable once, with Developer Mode on, and be on the same
# Wi-Fi as this Mac. A free Apple ID signs for 7 days, so run it at least once a week.
#
#   ios/scripts/deploy.sh            # the first paired iPhone that answers
#   HUB_PHONE=<name or id> ios/scripts/deploy.sh
set -euo pipefail
cd "$(dirname "$0")/.."
WORK=$(mktemp -d "${TMPDIR:-/tmp}/hub-ios-XXXXXX")
# ios/Hub/ReleaseNotes.json is a committed empty list that this script fills for the build; put it back after.
trap 'rm -rf "$WORK"; git checkout -q -- Hub/ReleaseNotes.json 2>/dev/null || true' EXIT

# The phone, as CoreDevice sees it: its identifier for devicectl.
xcrun devicectl list devices --json-output "$WORK/devices.json" >/dev/null
DEVICE=$(python3 - "$WORK/devices.json" "${HUB_PHONE:-}" <<'PY'
import json, sys
want = sys.argv[2].lower()
for d in json.load(open(sys.argv[1]))["result"]["devices"]:
    props, hw = d.get("deviceProperties", {}), d.get("hardwareProperties", {})
    name = props.get("name", "")
    if hw.get("deviceType") != "iPhone" or d.get("connectionProperties", {}).get("pairingState") != "paired":
        continue
    if want and want not in (name.lower(), d["identifier"].lower()):
        continue
    if d.get("connectionProperties", {}).get("tunnelState") == "unavailable":
        continue
    print(d["identifier"]); break
PY
)
if [ -z "$DEVICE" ]; then
  echo "No paired iPhone answers. Is it on the same Wi-Fi as this Mac, and unlocked?" >&2
  exit 1
fi

BUILD=$(date +%Y%m%d%H%M)

# What's new in this build: the pull requests merged under ios/ since the last build installed from this Mac.
# The history (newest first) lives in ~/.hub-ios/builds.json and goes into the app as ReleaseNotes.json,
# for the "What's new" sheet and Settings > Updates. It is written back only once the install has worked.
NOTES="${HUB_NOTES:-$HOME/.hub-ios/builds.json}"
python3 - "$NOTES" "$BUILD" "$WORK/notes.json" <<'PY'
import json, os, re, subprocess, sys
path, build, out = sys.argv[1:4]
git = lambda *a: subprocess.run(["git", *a], capture_output=True, text=True)
try:
    history = json.load(open(path))
except (OSError, ValueError):
    history = []
head = git("rev-parse", "HEAD").stdout.strip()
prev = history[0].get("commit") if history else None
if prev and git("cat-file", "-e", prev + "^{commit}").returncode != 0:
    prev = None
rng = [prev + "..HEAD"] if prev else ["-n", "12", "HEAD"]
log = git("log", "--first-parent", "--format=%s%x1f%b%x1e", *rng, "--", ":/ios").stdout
changes = []
for rec in filter(str.strip, log.split("\x1e")):
    subject, _, body = rec.strip().partition("\x1f")
    m = re.search(r"\(#(\d+)\)\s*$", subject)
    if m:
        title, pr = subject[:m.start()].strip(), int(m.group(1))
    elif (m := re.match(r"Merge pull request #(\d+)", subject)):
        pr = int(m.group(1))
        title = next((l.strip() for l in body.splitlines() if l.strip()), "")
    else:
        title, pr = subject.strip(), None
    title = re.sub(r"^iPhone app:\s*", "", title)
    title = title[:1].upper() + title[1:]
    if title and not title.startswith("Merge "):
        changes.append({"title": title, **({"pr": pr} if pr else {})})
history = [h for h in history if h.get("build") != build]
history.insert(0, {"build": build, "commit": head, "changes": changes})
json.dump(history[:40], open(out, "w"), indent=1, ensure_ascii=False)
print(f"{len(changes)} changes since the last build")
PY
cp "$WORK/notes.json" Hub/ReleaseNotes.json

echo "Building $BUILD"
xcodebuild -quiet -project Hub.xcodeproj -scheme Hub -configuration Release -destination 'generic/platform=iOS' \
  -derivedDataPath "$WORK/dd" -allowProvisioningUpdates CURRENT_PROJECT_VERSION="$BUILD" build
echo "Installing on the phone"
xcrun devicectl device install app --device "$DEVICE" "$WORK/dd/Build/Products/Release-iphoneos/Hub.app" >/dev/null
xcrun devicectl device process launch --device "$DEVICE" se.karlsson.hub >/dev/null 2>&1 || true
mkdir -p "$(dirname "$NOTES")" && cp "$WORK/notes.json" "$NOTES"
echo "Hub $BUILD is on the phone."
