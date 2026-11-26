#!/usr/bin/env bash
# Runs install-host.sh from nothing in fresh containers, the way Install hubd does on a new
# machine: a normal user whose sudo wants a password, no terminal, and none of git, a compiler,
# curl, Rust or Node. It checks every path a real machine takes:
#
#   1. --check lists everything missing at once
#   2. without root the install stops with one line naming the packages and the sudo it needs
#   3. the password, fed to sudo as Hub does, installs the packages and lingering
#   4. the install goes through and hubd answers
#   5. running it again carries on from what is there, and quickly
#
#   scripts/test-install-host.sh [IMAGE…]       default: debian:12 ubuntu:24.04
#
# With HUB_TEST_PREBUILT set to a hubd binary it takes the path of a ready-built hubd instead, as
# the PC sends one: no compiler is asked for, Rust is not installed and nothing is built. With
# HUB_TEST_PLATFORM (linux/arm64, linux/arm/v7) the containers are that machine.
#
# Containers have no systemd, so systemctl and loginctl are small stand-ins that start run-hubd
# directly; everything else is real. ARM images run under QEMU where binfmt is set up.
# HUB_TEST_DOCKER_ARGS adds docker arguments and HUB_TEST_PREP runs as root first, for a network
# that needs a proxy.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IMAGES=("$@")
[ "${#IMAGES[@]}" -gt 0 ] || IMAGES=(debian:12 ubuntu:24.04)

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"; git -C "$ROOT" update-ref -d refs/remotes/origin/install-test 2>/dev/null || true' EXIT
# What the PC sends: a bundle of its clone, here of HEAD.
git -C "$ROOT" update-ref refs/remotes/origin/install-test HEAD
git -C "$ROOT" bundle create --quiet "$WORK/hub.bundle" refs/remotes/origin/install-test
cp "$ROOT/scripts/install-host.sh" "$WORK/"
PREBUILT_VERSION=""
if [ -n "${HUB_TEST_PREBUILT:-}" ]; then
    cp "$HUB_TEST_PREBUILT" "$WORK/hubd.prebuilt"
    PREBUILT_VERSION="$(git -C "$ROOT" rev-parse --short=12 HEAD)"
fi

cat > "$WORK/inside.sh" <<'INSIDE'
set -euo pipefail
eval "${HUB_TEST_PREP:-}"
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq sudo procps > /dev/null
useradd -m -s /bin/bash pi
echo 'pi:raspberry' | chpasswd
usermod -aG sudo pi

cat > /usr/local/bin/systemctl <<'SH'
#!/bin/sh
# Stands in for `systemctl --user`: restart starts run-hubd in the background.
D="$HOME/.local/share/hub"
case "$*" in
    *restart*) pkill -f "$D/bin/" 2>/dev/null; sleep 0.5; nohup "$D/run-hubd" > "$D/hubd.out" 2>&1 & ;;
esac
exit 0
SH
cat > /usr/local/bin/loginctl <<'SH'
#!/bin/sh
case "$1" in
    show-user) echo "Linger=$(cat "/var/lib/linger-$2" 2>/dev/null || echo no)" ;;
    enable-linger) [ "$(id -u)" = 0 ] || { echo "Access denied" >&2; exit 1; }; echo yes > "/var/lib/linger-$2" ;;
esac
SH
chmod +x /usr/local/bin/systemctl /usr/local/bin/loginctl

as_pi() { su pi -c "cd ~ && $1" < /dev/null; }
D=/home/pi/.local/share/hub
mkdir -p "$D" && cp /t/install-host.sh /t/hub.bundle "$D/"
RUN="HUB_BUNDLE=$D/hub.bundle HUB_BRANCH=install-test"
# What a machine that builds hubd itself needs, and one that gets it from the PC does not.
BUILD_TOOLS="package git package build-essential"
if [ -n "$PREBUILT_VERSION" ]; then
    cp /t/hubd.prebuilt "$D/"
    RUN="$RUN HUB_PREBUILT=$D/hubd.prebuilt HUB_PREBUILT_VERSION=$PREBUILT_VERSION"
    BUILD_TOOLS=""
fi
chown -R pi "/home/pi/.local"
CHECK_RUN="$RUN bash $D/install-host.sh --check"
RUN="$RUN bash $D/install-host.sh"

echo "== 1. --check lists everything missing"
CHECK="$(as_pi "$CHECK_RUN")"
echo "$CHECK"
for want in ${BUILD_TOOLS:+"package git" "package build-essential"} "package curl" linger; do
    grep -qx "$want" <<< "$CHECK" || { echo "FAIL: --check did not say: $want"; exit 1; }
done
! grep -qx sudo <<< "$CHECK" || { echo "FAIL: sudo should want a password"; exit 1; }
if [ -z "$BUILD_TOOLS" ] && grep -qxE "package (git|build-essential)" <<< "$CHECK"; then echo "FAIL: asked for git or a compiler with a ready-built hubd"; exit 1; fi

echo "== 2. without root it stops with one line"
if as_pi "$RUN" > /tmp/out 2>&1; then echo "FAIL: installed without the packages"; exit 1; fi
LAST="$(grep -v '^[[:space:]]*$' /tmp/out | tail -n 1)"
echo "$LAST"
case "$LAST" in *"Needs "*"curl"*"sudo"*) ;; *) echo "FAIL: last line should name the packages and sudo"; exit 1 ;; esac

echo "== 3. the password, fed to sudo as Hub does"
PACKAGES="$(sed -n 's/^package //p' <<< "$CHECK" | tr '\n' ' ')"
su pi -c "echo raspberry | sudo -S -p '' -v" || { echo "FAIL: sudo -v with the password"; exit 1; }
su pi -c "echo raspberry | sudo -S -p '' bash $D/install-host.sh --root pi $PACKAGES" > /tmp/root.out 2>&1 ||
    { tail -n 20 /tmp/root.out; echo "FAIL: --root"; exit 1; }
[ "$(cat /var/lib/linger-pi)" = yes ] || { echo "FAIL: lingering not enabled"; exit 1; }
CHECK="$(as_pi "$CHECK_RUN")"
[ -z "$(grep -v '^sudo$' <<< "$CHECK" || true)" ] || { echo "$CHECK"; echo "FAIL: still missing after --root"; exit 1; }

echo "== 4. the install goes through"
START=$(date +%s)
as_pi "$RUN" > /tmp/install.out 2>&1 || { tail -n 30 /tmp/install.out; echo "FAIL: install"; exit 1; }
grep '^Step ' /tmp/install.out
echo "took $(( $(date +%s) - START )) s"
curl -fsS http://127.0.0.1:7878/api/health > /dev/null || { echo "FAIL: hubd does not answer"; exit 1; }
grep -q "is running on" /tmp/install.out || { echo "FAIL: no running line"; exit 1; }
if [ -n "$PREBUILT_VERSION" ]; then
    [ ! -e /home/pi/.cargo ] || { echo "FAIL: installed Rust with a ready-built hubd"; exit 1; }
    [ "$(cat "$D/current")" = "$PREBUILT_VERSION" ] || { echo "FAIL: current is not $PREBUILT_VERSION"; exit 1; }
    grep -q '"pushed": true' "$D/hub.json" || { echo "FAIL: updates do not come from the PC"; exit 1; }
fi

echo "== 5. again, carrying on from what is there"
START=$(date +%s)
as_pi "$RUN" > /tmp/again.out 2>&1 || { tail -n 30 /tmp/again.out; echo "FAIL: second run"; exit 1; }
TOOK=$(( $(date +%s) - START ))
echo "took $TOOK s"
! grep -q "Installing Rust\|Installing Node" /tmp/again.out || { echo "FAIL: installed Rust or Node again"; exit 1; }
curl -fsS http://127.0.0.1:7878/api/health > /dev/null || { echo "FAIL: hubd does not answer after the second run"; exit 1; }
echo "PASS"
INSIDE

FAILED=()
for image in "${IMAGES[@]}"; do
    echo "### $image"
    # shellcheck disable=SC2086 # extra arguments, one word each
    if docker run --rm ${HUB_TEST_DOCKER_ARGS:-} ${HUB_TEST_PLATFORM:+--platform "$HUB_TEST_PLATFORM"} \
        -e HUB_TEST_PREP -e PREBUILT_VERSION="$PREBUILT_VERSION" -v "$WORK:/t:ro" "$image" bash /t/inside.sh; then
        echo "### $image passed"
    else
        echo "### $image FAILED"
        FAILED+=("$image")
    fi
done
[ "${#FAILED[@]}" -eq 0 ] || { echo "Failed: ${FAILED[*]}"; exit 1; }
