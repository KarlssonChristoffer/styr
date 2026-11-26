#!/bin/bash
# iPhone app tests, on a Mac with Xcode. The two older ones are plain Swift; ModelTests runs in
# the iOS simulator over every source file of the app but HubApp.swift (it has its own @main).
#   ios/scripts/test.sh
set -euo pipefail
cd "$(dirname "$0")/.."
out="$(mktemp -d "${TMPDIR:-/tmp}/hub-ios-tests.XXXXXX")"
trap 'rm -rf "$out"' EXIT

swiftc Hub/ReleaseNotes.swift tests/ReleaseNotesTests.swift -o "$out/notes" && "$out/notes"
swiftc Hub/Updates.swift tests/UpdatesTests.swift -o "$out/updates" && "$out/updates"

sdk=$(xcrun --sdk iphonesimulator --show-sdk-path)
arch=$(uname -m); [ "$arch" = arm64 ] || arch=x86_64
sources=$(ls Hub/*.swift | grep -v '^Hub/HubApp.swift$')
# shellcheck disable=SC2086
xcrun --sdk iphonesimulator swiftc -sdk "$sdk" -target "$arch-apple-ios18.0-simulator" -parse-as-library \
  $sources tests/ModelTests.swift -o "$out/model"
# A simulator process runs without launching a simulator window when one is already booted;
# boot one when none is.
if ! xcrun simctl list devices booted | grep -q Booted; then
  dev=$(xcrun simctl list devices available | grep -E 'iPhone' | head -1 | sed -E 's/.*\(([0-9A-F-]{36})\).*/\1/')
  xcrun simctl boot "$dev"
  booted_here=$dev
fi
status=0
xcrun simctl spawn booted "$out/model" || status=$?
[ -z "${booted_here:-}" ] || xcrun simctl shutdown "$booted_here"
exit $status
