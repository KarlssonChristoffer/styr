#!/usr/bin/env bash
# Installs hubd on a Linux machine (tincan, Plåtnicklas, a cloud VM) or a Mac so Hub on another
# machine can run threads here. Run it on that machine, once; after that hubd keeps itself up to date.
#
#   bash scripts/install-host.sh            from a checkout, or
#   curl -fsSL <raw url> | bash             when the repository is reachable, or
#   Settings > Machines > Install           on the PC, for a machine its SSH config reaches. That
#                                           sends this script and the PC's copy of the repository
#                                           (HUB_BUNDLE), and runs it with no terminal.
#
# hubd runs as a systemd user service (on a Mac a launchd agent) on 127.0.0.1:7878 only. With Tailscale here, Tailscale
# serves it over HTTPS to your tailnet and the script prints a pairing code for the PC; without,
# the PC reaches it through an SSH tunnel with your keys. Either way no port is opened.
# The updater fetches with your normal git credentials, so the repository may be private.
#
# It can be run again at any time: each step skips what is done, so after a failure it carries on
# where it stopped, and downloads try again before giving up. It first checks everything the
# machine lacks and says all of it at once. What needs root (apt packages, lingering) is done in
# one go: with sudo here when there is a terminal, or by Hub with the password you give it once.
#
#   install-host.sh --check             only says what is missing, one `package`, `linger`,
#                                       `sudo` or `problem` line each
#   install-host.sh --root USER PKG…    as root: installs PKG… and lets USER's services run
#                                       without a login
set -euo pipefail

REPO="${HUB_REPO:-https://github.com/KarlssonChristoffer/hub}"
BRANCH="${HUB_BRANCH:-main}"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}/hub"
SRC="$DATA/src"
NODE_DIR="$DATA/node"
# The engines' own command lines (claude, codex), for signing in and opening them there.
TOOLS="$DATA/tools"
STEPS=8
OS="$(uname -s)"

# Started over SSH there is no terminal, so sudo must not wait for a password nobody can type.
if [ "$(id -u)" = 0 ]; then SUDO=""; elif [ -t 0 ]; then SUDO=sudo; else SUDO="sudo -n"; fi
USER="${USER:-$(id -un)}"
# A shell started over SSH does not read the login profile, where rustup puts cargo and nvm
# puts node.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
[ -x "$NODE_DIR/bin/node" ] && PATH="$NODE_DIR/bin:$PATH"
[ -d "$TOOLS/bin" ] && PATH="$TOOLS/bin:$PATH"
# Over SSH a Mac's shell has no Homebrew in its PATH, and Tailscale's command line is inside its
# app (the name's case does not matter on a Mac's disk).
if [ "$OS" = Darwin ]; then
    for dir in /usr/local/bin /opt/homebrew/bin /Applications/Tailscale.app/Contents/MacOS; do
        [ -d "$dir" ] && case ":$PATH:" in *":$dir:"*) ;; *) PATH="$dir:$PATH" ;; esac
    done
fi
if ! command -v npx >/dev/null 2>&1 && [ -s "${NVM_DIR:-$HOME/.nvm}/nvm.sh" ]; then
    . "${NVM_DIR:-$HOME/.nvm}/nvm.sh" >/dev/null 2>&1 || true
fi

# Asks uname itself, so it also works in the root shell, which gets only some functions.
mac() {
    [ "$(uname -s)" = Darwin ]
}

# On a Mac hubd is a launchd daemon: it starts at boot, before anyone logs in, as this user.
LABEL=se.karlsson.hubd
DAEMON="/Library/LaunchDaemons/$LABEL.plist"

# sha256sum's arguments, with a Mac's shasum where there is no sha256sum.
sha256() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$@"; else shasum -a 256 "$@"; fi
}

# A Mac has no `timeout`; there the command just runs.
within() {
    if command -v timeout >/dev/null 2>&1; then timeout "$@"; else shift; "$@"; fi
}

N=0
# How long each step took, so a slow one is named and a new machine never looks hung. SECONDS is
# bash's own clock.
SECONDS=0
STEP_NAME="" STEP_AT=0 TIMINGS=""
lap() {
    [ -n "$STEP_NAME" ] || return 0
    local took=$((SECONDS - STEP_AT))
    TIMINGS="$TIMINGS  $STEP_NAME: ${took} s"$'\n'
    [ "$took" -lt 5 ] || echo "  $STEP_NAME took ${took} s"
    STEP_NAME=""
}
# Printed however the script ends, so a failure also says where the time went.
timings() {
    lap
    [ -z "$TIMINGS" ] || printf 'Time per step (%s s in all):\n%s' "$SECONDS" "$TIMINGS"
}
# Hub shows the latest line of this output next to the machine, so each step says where it is.
step() {
    lap
    N=$((N + 1))
    STEP_NAME="$*" STEP_AT=$SECONDS
    echo "Step $N/$STEPS: $*"
}

# Downloads try again with a growing pause, so a dropped connection does not end the install.
retry() {
    local tries=1
    until "$@"; do
        if [ "$tries" -ge 5 ]; then return 1; fi
        echo "  failed; trying again in $((2 ** tries)) s ($((tries + 1)) of 5)" >&2
        sleep $((2 ** tries))
        tries=$((tries + 1))
    done
}

# The engines start with npx, from Node.js 20 or newer.
node_ok() {
    command -v npx >/dev/null 2>&1 &&
        [ "$(node -p 'process.versions.node.split(".")[0]' 2>/dev/null || echo 0)" -ge 20 ]
}

# hubd comes ready-built: sent with this run, or installed that way before (its updates are too).
ready_built() {
    [ -n "${HUB_PREBUILT:-}" ] ||
        { [ -x "$DATA/bin/$(cat "$DATA/current" 2>/dev/null)/hubd" ] && grep -q '"pushed": true' "$DATA/hub.json" 2>/dev/null; }
}

node_arch() {
    case "$(uname -m)" in
        x86_64) echo x64 ;;
        aarch64 | arm64) echo arm64 ;;
        armv7l) echo armv7l ;;
        *) return 1 ;;
    esac
}

# What this machine lacks, one line each: `package NAME` for what apt can install, `linger` when
# services would stop at logout, `sudo` when root is at hand without a password, and
# `problem TEXT` for what a person has to fix.
check() {
    local apt="" free
    command -v apt-get >/dev/null 2>&1 && apt=1
    lack() {
        command -v "$1" >/dev/null 2>&1 && return 0
        if [ -n "$apt" ]; then echo "package $2"; else echo "problem $1 is missing ($3); install it with the package manager"; fi
    }
    if mac; then
        # Node.js 22 runs on macOS 11 and newer.
        local major
        major="$(sw_vers -productVersion 2>/dev/null | cut -d. -f1)"
        [ "${major:-0}" -ge 11 ] || echo "problem macOS $(sw_vers -productVersion 2>/dev/null) is too old for Node.js 22; update the Mac to macOS 11 or newer"
        # git and cc are there as stubs until the Command Line Tools are; softwareupdate gets them.
        xcode-select -p >/dev/null 2>&1 || echo "package xcode-clt"
    else
        ready_built || lack git git "for the source"
        # A ready-built hubd needs no compiler, nor the room to build.
        ready_built || lack cc build-essential "to build hubd"
    fi
    lack curl curl "for downloads"
    if [ -n "$apt" ] && [ ! -e /etc/ssl/certs/ca-certificates.crt ]; then echo "package ca-certificates"; fi
    if ! node_ok; then
        node_arch >/dev/null || echo "problem Node.js has no build for $(uname -m); install Node.js 20 or newer by hand"
        # A Mac gets Node.js as .tar.gz.
        mac || lack xz xz-utils "to unpack Node.js"
    fi
    # A Mac has launchd, which runs agents without lingering.
    mac || command -v systemctl >/dev/null 2>&1 || echo "problem systemd is missing; hubd runs as a systemd user service"
    free="$(df -Pk "$HOME" | awk 'NR == 2 { print int($4 / 1048576) }')"
    if ready_built; then
        [ "$free" -ge 1 ] || echo "problem less than 1 GB is free in $HOME"
    else
        [ "$free" -ge 3 ] || echo "problem only $free GB is free in $HOME; building hubd takes about 3 GB"
    fi
    if mac; then
        [ -f "$DAEMON" ] || echo linger
    elif ! loginctl show-user "$USER" -p Linger 2>/dev/null | grep -q yes &&
        ! loginctl enable-linger "$USER" >/dev/null 2>&1; then
        echo linger
    fi
    if [ "$(id -u)" = 0 ] || sudo -n true >/dev/null 2>&1; then echo sudo; fi
    return 0
}

# As root: the packages, and lingering so hubd runs from boot without a login. Tailscale lets
# WHO change what it serves, for `hubd pair --tailscale`.
root_setup() {
    local who="$1" pkg
    shift
    for pkg in "$@"; do
        [[ "$pkg" =~ ^[a-z0-9][a-z0-9.+-]*$ ]] || { echo "not a package name: $pkg" >&2; return 1; }
    done
    if mac; then
        mac_root "$who" "$@"
        return
    fi
    if [ "$#" -gt 0 ]; then
        echo "Installing $* with apt…"
        export DEBIAN_FRONTEND=noninteractive
        # A fresh machine is often busy with its own updates; wait for apt instead of failing.
        retry apt-get -o DPkg::Lock::Timeout=600 update -qq < /dev/null
        retry apt-get -o DPkg::Lock::Timeout=600 install -y -qq --no-install-recommends "$@" < /dev/null
    fi
    loginctl enable-linger "$who" || echo "Could not enable lingering; hubd stops when $who logs out." >&2
    if command -v tailscale >/dev/null 2>&1; then
        tailscale set --operator="$who" >/dev/null 2>&1 || true
    fi
}

# The Command Line Tools, without the dialog `xcode-select --install` opens on the Mac's screen:
# this flag file makes softwareupdate offer them, as Homebrew's installer does.
install_clt() {
    xcode-select -p >/dev/null 2>&1 && return 0
    echo "Installing the Xcode Command Line Tools (git and a compiler) with softwareupdate; this takes a while…"
    local flag=/tmp/.com.apple.dt.CommandLineTools.installondemand.in-progress label
    touch "$flag"
    # Newer macOS says "* Label: Command Line Tools for Xcode-16.0", older "* Command Line Tools …";
    # the last one is the newest.
    label="$(softwareupdate -l 2>&1 | sed -n 's/^[[:space:]]*\* \(Label: \)\{0,1\}\(Command Line Tools.*\)$/\2/p' | tail -n 1)"
    if [ -z "$label" ]; then
        rm -f "$flag"
        echo "softwareupdate offers no Command Line Tools; on the Mac run 'xcode-select --install' and press Install" >&2
        return 1
    fi
    retry softwareupdate -i "$label" < /dev/null
    rm -f "$flag"
    xcode-select -p >/dev/null 2>&1 || { echo "The Command Line Tools did not install; on the Mac run 'xcode-select --install'" >&2; return 1; }
}

# As root on a Mac: the Command Line Tools, hubd as a launchd daemon that runs as WHO from boot
# without a login, and no sleep while on the charger, so the machine stays reachable.
mac_root() {
    local who="$1" home data pkg mem jobs
    shift
    for pkg in "$@"; do
        case "$pkg" in
            xcode-clt) install_clt ;;
            *) echo "no way to install $pkg on a Mac" >&2; return 1 ;;
        esac
    done
    home="$(dscl . -read "/Users/$who" NFSHomeDirectory 2>/dev/null | awk '{ print $2 }')"
    [ -d "$home" ] || { echo "Could not find $who's home folder." >&2; return 1; }
    data="$home/.local/share/hub"
    install -d -o "$who" "$home/.local" "$home/.local/share" "$data"
    touch "$data/hubd.log" && chown "$who" "$data/hubd.log"
    mem=$(($(sysctl -n hw.memsize) / 1048576 / 1200))
    jobs="$(sysctl -n hw.ncpu)"
    [ "$mem" -ge 1 ] || mem=1
    [ "$mem" -le "$jobs" ] && jobs="$mem"
    # It waits for the installer's first version, then starts whatever version `current` names;
    # an update hands over and exits, and launchd starts it again.
    cat > "$DAEMON.new" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$LABEL</string>
  <key>UserName</key><string>$who</string>
  <key>ProgramArguments</key>
  <array>
    <string>/bin/sh</string>
    <string>-c</string>
    <string>while [ ! -s "$data/current" ]; do sleep 2; done; exec "$data/bin/\$(cat "$data/current")/hubd"</string>
  </array>
  <key>WorkingDirectory</key><string>$home</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>HOME</key><string>$home</string>
    <key>USER</key><string>$who</string>
    <key>LANG</key><string>en_US.UTF-8</string>
    <key>PATH</key><string>$data/tools/bin:$data/node/bin:$home/.cargo/bin:/usr/local/bin:/opt/homebrew/bin:/Applications/Tailscale.app/Contents/MacOS:/usr/bin:/bin:/usr/sbin:/sbin</string>
    <key>CARGO_BUILD_JOBS</key><string>$jobs</string>
  </dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ThrottleInterval</key><integer>5</integer>
  <key>StandardOutPath</key><string>$data/hubd.log</string>
  <key>StandardErrorPath</key><string>$data/hubd.log</string>
</dict>
</plist>
PLIST
    plutil -lint "$DAEMON.new" >/dev/null || { echo "The launchd file came out wrong: $DAEMON.new" >&2; return 1; }
    # Installing again leaves a running hubd alone when nothing changed: restarting it here would
    # have its updater at the source while this install fetches it.
    if cmp -s "$DAEMON.new" "$DAEMON" && launchctl print "system/$LABEL" >/dev/null 2>&1; then
        rm -f "$DAEMON.new"
    else
        mv "$DAEMON.new" "$DAEMON"
        chown root:wheel "$DAEMON"
        chmod 644 "$DAEMON"
        launchctl bootout "system/$LABEL" >/dev/null 2>&1 || true
        launchctl bootstrap system "$DAEMON"
    fi
    echo "hubd starts with the Mac from now on, as $who, also before anyone logs in."
    pmset -c sleep 0 && echo "The Mac no longer sleeps while on the charger (closing the lid still does)."
}

case "${1:-}" in
    --check)
        check
        exit 0
        ;;
    --root)
        shift
        root_setup "$@"
        exit 0
        ;;
esac

trap timings EXIT
step "Checking $(hostname)"
FOUND="$(check)"
PACKAGES="$(printf '%s\n' "$FOUND" | sed -n 's/^package //p' | tr '\n' ' ')"
PROBLEMS="$(printf '%s\n' "$FOUND" | sed -n 's/^problem //p')"
if [ -n "$PROBLEMS" ]; then
    echo "Cannot install yet: $(printf '%s\n' "$PROBLEMS" | paste -sd ';' - | sed 's/;/; /g')" >&2
    exit 1
fi
if [ -n "$PACKAGES" ] || printf '%s\n' "$FOUND" | grep -qx linger; then
    # shellcheck disable=SC2086 # one word per package
    if printf '%s\n' "$FOUND" | grep -qx sudo || [ -t 0 ]; then
        $SUDO bash -c "set -euo pipefail; $(declare -f retry root_setup mac install_clt mac_root); LABEL=$LABEL DAEMON=$DAEMON; root_setup \"\$@\"" root_setup "$USER" $PACKAGES
    elif [ -n "$PACKAGES" ]; then
        echo "Needs ${PACKAGES% }, which takes sudo: press Install hubd in Hub and give the password, or run this script here in a terminal" >&2
        exit 1
    elif mac; then
        echo "hubd runs only while you are logged in: press Install hubd in Hub and give the password once, so it starts with the Mac."
    else
        echo "Could not enable lingering; hubd stops when you log out. Run: sudo loginctl enable-linger $USER" >&2
    fi
fi

# Built for this machine by the PC (Install hubd), for the version the source is at.
PREBUILT=""
if [ -n "${HUB_PREBUILT:-}" ] && [ -s "$HUB_PREBUILT" ]; then PREBUILT="$HUB_PREBUILT"; fi

step "Rust"
if ready_built; then
    echo "Not needed: the PC builds hubd for this machine, and its updates."
elif ! command -v cargo >/dev/null 2>&1; then
    # Rust goes in the home folder and needs no sudo.
    case "$OS $(uname -m)" in
        "Darwin x86_64") TRIPLE=x86_64-apple-darwin ;;
        "Darwin arm64") TRIPLE=aarch64-apple-darwin ;;
        "Linux x86_64") TRIPLE=x86_64-unknown-linux-gnu ;;
        "Linux aarch64" | "Linux arm64") TRIPLE=aarch64-unknown-linux-gnu ;;
        "Linux armv7l") TRIPLE=armv7-unknown-linux-gnueabihf ;;
        *) echo "No Rust installer here for $(uname -m); install Rust from https://rustup.rs" >&2; exit 1 ;;
    esac
    echo "Installing Rust (rustup, in ~/.cargo)…"
    # rustup-init itself, as sh.rustup.rs would fetch it, checked against its published checksum.
    INIT="https://static.rust-lang.org/rustup/dist/$TRIPLE/rustup-init"
    TMP="$(mktemp -d)"
    retry curl -fsSL -o "$TMP/rustup-init" "$INIT"
    retry curl -fsSL -o "$TMP/rustup-init.sha256" "$INIT.sha256"
    [ "$(sha256 "$TMP/rustup-init" | cut -d' ' -f1)" = "$(cut -d' ' -f1 "$TMP/rustup-init.sha256")" ] ||
        { echo "The Rust installer did not match its checksum." >&2; rm -rf "$TMP"; exit 1; }
    chmod +x "$TMP/rustup-init"
    retry "$TMP/rustup-init" -y --profile minimal --no-modify-path >/dev/null
    rm -rf "$TMP"
    . "$HOME/.cargo/env"
fi

step "Node.js"
if ! node_ok; then
    # Node's own build goes in Hub's folder and needs no sudo, like Rust.
    ARCH="$(node_arch)"
    if mac; then PLATFORM="darwin-$ARCH" EXT='tar\.gz' UNPACK=-xzf; else PLATFORM="linux-$ARCH" EXT='tar\.xz' UNPACK=-xJf; fi
    DIST=https://nodejs.org/dist/latest-v22.x
    echo "Installing Node.js 22 (in $NODE_DIR)…"
    TMP="$(mktemp -d)"
    retry curl -fsSL -o "$TMP/SHASUMS256.txt" "$DIST/SHASUMS256.txt"
    LINE="$(grep -E " node-v[0-9.]+-$PLATFORM\.$EXT$" "$TMP/SHASUMS256.txt" | head -n1)"
    [ -n "$LINE" ] || { echo "Could not find Node.js for $PLATFORM at $DIST" >&2; exit 1; }
    TARBALL="${LINE##* }"
    retry curl -fsSL -o "$TMP/$TARBALL" "$DIST/$TARBALL"
    (cd "$TMP" && printf '%s\n' "$LINE" | sha256 -c --status -) ||
        { echo "The Node.js download did not match its checksum." >&2; rm -rf "$TMP"; exit 1; }
    rm -rf "$NODE_DIR" && mkdir -p "$NODE_DIR"
    tar "$UNPACK" "$TMP/$TARBALL" -C "$NODE_DIR" --strip-components=1
    rm -rf "$TMP"
    PATH="$NODE_DIR/bin:$PATH"
fi
node_ok || { echo "npx from Node.js 20 or newer is missing; install it from https://nodejs.org or nvm" >&2; exit 1; }

# The engines run through npx, but signing in there (`hubd sign-in`) and opening an engine in a
# terminal need their command lines. They go in Hub's folder, so no sudo; a failure here only
# means signing in waits for a later install.
step "Engine command lines"
for tool in claude:@anthropic-ai/claude-code codex:@openai/codex; do
    name="${tool%%:*}" package="${tool#*:}"
    if command -v "$name" >/dev/null 2>&1; then
        echo "$name is there."
    elif echo "Installing $name (npm; a minute or two on a slow connection)…" &&
        # --loglevel=http prints each download as it happens: with no terminal npm is otherwise
        # silent until it is done, which looks like a hang.
        retry npm install --global --prefix "$TOOLS" --no-fund --no-audit --no-update-notifier --loglevel=http "$package" < /dev/null; then
        echo "Installed $name."
    else
        echo "Could not install $package; Hub's Ready for work will offer it again." >&2
    fi
done
PATH="$TOOLS/bin:$PATH"

mkdir -p "$DATA"

# The updater's own clone. Never work in it; it is reset to the branch on every update.
step "Getting the source"
if ready_built; then
    echo "Not needed: the PC builds hubd for this machine."
else
    # The PC's copy comes as a bundle, so a private repository needs no login here. It goes into a
    # bare copy next to the clone, which becomes the clone's origin, and the PC sends each new
    # version there later (`push_updates`), so updates need no login either. The repository itself
    # is the fallback, with this machine's git credentials.
    if [ ! -d "$SRC/.git" ]; then
        rm -rf "$SRC"
        git init --quiet "$SRC"
        git -C "$SRC" remote add origin "$REPO"
    fi
    MIRROR="$DATA/source.git"
    if [ -n "${HUB_BUNDLE:-}" ] && [ -f "$HUB_BUNDLE" ]; then
        if { [ -d "$MIRROR" ] || git init --quiet --bare "$MIRROR"; } &&
            git -C "$MIRROR" fetch --quiet "$HUB_BUNDLE" "+refs/remotes/origin/$BRANCH:refs/heads/$BRANCH"; then
            git -C "$SRC" remote set-url origin "$MIRROR"
        else
            echo "Could not read the PC's copy of the source; trying $REPO." >&2
            git -C "$SRC" remote set-url origin "$REPO"
        fi
    fi
    retry git -C "$SRC" fetch --quiet origin "$BRANCH" ||
        { echo "Could not get $REPO; if it is private, log in to GitHub here first (gh auth login)." >&2; exit 1; }
    git -C "$SRC" checkout --quiet --force -B "$BRANCH" "origin/$BRANCH"
fi

# hubd here follows the PC. With a ready-built hubd the PC builds each new version for this
# machine and sends it (`pushed`), so this machine needs no Rust and no GitHub login. Otherwise
# only the core is tested and built here: there is no window to show, and nobody to click
# "install", so a version that passes is switched to as soon as no thread is working. A Raspberry
# Pi's card is small, so building asks for 3 GB free instead of the PC's 8.
pushed_config() {
    cat <<'JSON'
{
  "update": {
    "enabled": true,
    "ask": false,
    "pushed": true
  }
}
JSON
}
built_config() {
    cat <<'JSON'
{
  "update": {
    "enabled": true,
    "ask": false,
    "min_free_gb": 3,
    "max_build_gb": 3,
    "steps": [
      { "dir": "", "command": ["cargo", "build", "--release", "-p", "hubd"] }
    ],
    "artifacts": ["target/release/hubd"]
  }
}
JSON
}
# Before 2026-10-31 a machine that builds also ran the tests first; the PC already ran them.
tested_config() {
    cat <<'JSON'
{
  "update": {
    "enabled": true,
    "ask": false,
    "min_free_gb": 3,
    "max_build_gb": 3,
    "steps": [
      { "dir": "", "command": ["cargo", "test", "-p", "hub-core"] },
      { "dir": "", "command": ["cargo", "build", "--release", "-p", "hubd"] }
    ],
    "artifacts": ["target/release/hubd"]
  }
}
JSON
}
if [ -f "$DATA/hub.json" ] && tested_config | cmp -s - "$DATA/hub.json"; then
    built_config > "$DATA/hub.json"
    echo "Builds here no longer run the tests first."
fi
if [ ! -f "$DATA/hub.json" ] || { [ -n "$PREBUILT" ] && built_config | cmp -s - "$DATA/hub.json"; }; then
    if [ -n "$PREBUILT" ]; then pushed_config > "$DATA/hub.json"; else built_config > "$DATA/hub.json"; fi
    echo "Wrote $DATA/hub.json"
elif ! grep -q '"update"' "$DATA/hub.json"; then
    echo "Note: $DATA/hub.json has no \"update\" section; updates would try to build the desktop app too." >&2
fi

# One compile job per 1.2 GB of memory and swap, so a Raspberry Pi builds slowly instead of
# running out. Updates later build with the same number.
if mac; then
    # A Mac swaps to its disk as needed, so all of its memory counts.
    MEM_MB=$(($(sysctl -n hw.memsize) / 1048576))
    CPUS="$(sysctl -n hw.ncpu)"
else
    MEM_MB="$(awk '/^(MemAvailable|SwapFree):/ { kb += $2 } END { print int(kb / 1024) }' /proc/meminfo)"
    CPUS="$(nproc)"
fi
JOBS=$((MEM_MB / 1200))
[ "$JOBS" -ge 1 ] || JOBS=1
[ "$JOBS" -le "$CPUS" ] || JOBS="$CPUS"
export CARGO_BUILD_JOBS="$JOBS"

if [ -n "$PREBUILT" ]; then step "Installing hubd"; else step "Building hubd ($JOBS at a time; a few minutes, up to an hour on a Raspberry Pi)"; fi
if ready_built; then
    VERSION="${HUB_PREBUILT_VERSION:-$(cat "$DATA/current" 2>/dev/null || true)}"
else
    VERSION="$(git -C "$SRC" rev-parse --short=12 HEAD)"
fi
if [ -x "$DATA/bin/$VERSION/hubd" ] && [ "$(cat "$DATA/current" 2>/dev/null)" = "$VERSION" ]; then
    echo "hubd $VERSION is already built."
elif [ -n "$PREBUILT" ] && [ -n "$VERSION" ]; then
    echo "Installing the ready-built hubd $VERSION."
    mkdir -p "$DATA/bin/$VERSION.partial"
    cp "$PREBUILT" "$DATA/bin/$VERSION.partial/hubd"
    chmod 755 "$DATA/bin/$VERSION.partial/hubd"
    echo "$VERSION" > "$DATA/bin/$VERSION.partial/VERSION"
    rm -rf "${DATA:?}/bin/$VERSION"
    mv "$DATA/bin/$VERSION.partial" "$DATA/bin/$VERSION"
    printf '%s' "$VERSION" > "$DATA/current.tmp" && mv "$DATA/current.tmp" "$DATA/current"
    rm -f "$PREBUILT"
elif ready_built; then
    echo "The PC's hubd did not arrive; press Install hubd again on the PC." >&2
    exit 1
else
    (cd "$SRC" && retry cargo fetch --quiet)
    # Built in its own target folder, so it never locks the binaries it installs. An earlier
    # attempt's work there is kept, so building again carries on.
    (cd "$SRC" && cargo run --quiet --release --target-dir "$DATA/bootstrap" -p hubd -- install)
    # Updates build in their own folder; this one's gigabytes are better free on a small card.
    rm -rf "$DATA/bootstrap"
fi

step "Starting the service"
# The service starts whatever version `current` names. An update hands over to the new version
# and exits; systemd then restarts the service, which starts that version.
cat > "$DATA/run-hubd" <<SH
#!/bin/sh
exec "$DATA/bin/\$(cat "$DATA/current")/hubd"
SH
chmod +x "$DATA/run-hubd"

LOG_HINT="journalctl --user -u hubd -n 50"
if mac && [ -f "$DAEMON" ]; then
    # The daemon from the root step waits for `current`; a running older hubd is stopped, and
    # launchd starts the new one.
    LOG_HINT="tail -n 50 $DATA/hubd.log"
    # An agent from an install without sudo would start a second hubd at login.
    if [ -f "$HOME/Library/LaunchAgents/$LABEL.plist" ]; then
        launchctl bootout "gui/$(id -u)/$LABEL" >/dev/null 2>&1 || true
        rm -f "$HOME/Library/LaunchAgents/$LABEL.plist"
    fi
    # Only an older version: the new one may have started already.
    for pid in $(pgrep -u "$(id -u)" -f "^$DATA/bin/.*/hubd" || true); do
        case "$(ps -o command= -p "$pid")" in "$DATA/bin/$(cat "$DATA/current")/hubd"*) ;; *) kill "$pid" 2>/dev/null || true ;; esac
    done
    echo "hubd runs as the launchd daemon $LABEL."
elif mac; then
    # A launchd agent, which starts when this user logs in and again whenever it stops. Over SSH
    # it goes into the logged-in desktop session when there is one, so it outlives this session.
    PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
    LOG_HINT="tail -n 50 $DATA/hubd.log"
    mkdir -p "$HOME/Library/LaunchAgents"
    cat > "$PLIST" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$LABEL</string>
  <key>ProgramArguments</key><array><string>$DATA/run-hubd</string></array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key><string>$PATH</string>
    <key>CARGO_BUILD_JOBS</key><string>$JOBS</string>
  </dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ThrottleInterval</key><integer>3</integer>
  <key>StandardOutPath</key><string>$DATA/hubd.log</string>
  <key>StandardErrorPath</key><string>$DATA/hubd.log</string>
</dict>
</plist>
PLIST
    DOMAIN="gui/$(id -u)"
    launchctl print "$DOMAIN" >/dev/null 2>&1 || {
        DOMAIN="user/$(id -u)"
        echo "Nobody is logged in on the Mac's screen, so hubd runs only until it restarts. Turn on automatic login (System Settings > Users & Groups) so it starts by itself."
    }
    launchctl bootout "$DOMAIN/$LABEL" >/dev/null 2>&1 || true
    launchctl bootstrap "$DOMAIN" "$PLIST"
    launchctl kickstart -k "$DOMAIN/$LABEL"
else
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
mkdir -p "$UNIT_DIR"
cat > "$UNIT_DIR/hubd.service" <<UNIT
[Unit]
Description=Hub core (hubd)
After=network-online.target

[Service]
ExecStart=$DATA/run-hubd
# cargo for updates, npx and git for the engines, as in the shell this was installed from.
Environment=PATH=$PATH
Environment=CARGO_BUILD_JOBS=$JOBS
Restart=always
RestartSec=3

[Install]
WantedBy=default.target
UNIT

systemctl --user daemon-reload
systemctl --user enable --now hubd.service
systemctl --user restart hubd.service
fi

step "Waiting for hubd and letting the PC in"
for _ in $(seq 1 60); do
    if curl -fsS -o /dev/null http://127.0.0.1:7878/api/health 2>/dev/null; then
        echo
        echo "hubd $(cat "$DATA/current") is running on $(hostname)."
        HUBD="$DATA/bin/$(cat "$DATA/current")/hubd"
        # Installed from the PC (with its bundle or its build), the PC has this machine already.
        ADD=""
        [ -n "${HUB_BUNDLE:-}${HUB_PREBUILT:-}" ] || ADD="On the PC: Settings > Machines > Add, next to this machine's SSH alias."
        if mac; then
            # Tailscale on a Mac is its app, whose command line cannot reach it from SSH (error
            # -600), so the PC reaches this hubd through its SSH tunnel instead.
            [ -z "$ADD" ] || echo "$ADD"
        elif command -v tailscale >/dev/null 2>&1; then
            # Tailscale serves hubd over HTTPS to the tailnet, and a pairing code lets the PC in.
            # Changing what Tailscale serves needs root unless this user is its operator.
            "$HUBD" pair --tailscale 2>/dev/null ||
                { $SUDO tailscale set --operator="$USER" && "$HUBD" pair --tailscale; } ||
                { [ -z "$ADD" ] || echo "$ADD"; }
        else
            [ -z "$ADD" ] || echo "$ADD"
            echo "With Tailscale here, 'hubd pair --tailscale' lets the PC (and later the phone) in without SSH."
        fi
        # Built updates fetch from the repository with this machine's own login; the install did not.
        if ! ready_built && ! GIT_TERMINAL_PROMPT=0 within 30 git -C "$SRC" ls-remote --quiet origin "$BRANCH" >/dev/null 2>&1; then
            echo "Updates need a GitHub login here: run 'gh auth login' once (or set up git credentials for $REPO)."
        fi
        if ! command -v claude >/dev/null 2>&1 && [ -z "${ANTHROPIC_API_KEY:-}" ]; then
            echo "Threads here use Claude Code: log in once with 'npx @anthropic-ai/claude-code' and /login."
        fi
        exit 0
    fi
    sleep 0.5
done
echo "hubd did not answer; see: $LOG_HINT" >&2
exit 1
