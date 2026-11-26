# Install, update, develop
<!-- för: Christoffer -->

The operational detail the README leaves out: installing, how updates work, a second PC, config,
development and tests. The app is called Hub throughout.

## Install (Windows)

Requirements: Git, Rust (stable), Node 22+, and Claude Code installed and logged in.

```powershell
git clone https://github.com/KarlssonChristoffer/styr.git
cd styr
powershell -ExecutionPolicy Bypass -File scripts\install.ps1
```

That clones Hub into `%LOCALAPPDATA%\hub\src`, builds it, installs it and adds **Hub** to the
Start menu.

### Updates

From then on Hub updates itself: the core checks `main` every few minutes, builds new commits in
the background and switches to them once no thread is working. Threads resume afterwards. The
version, or the update in progress, is shown at the bottom of the sidebar. A build that fails is
skipped and shown there, with the log in `%LOCALAPPDATA%\hub\update.log`.

Never edit `%LOCALAPPDATA%\hub\src`. It is the updater's own clone and is reset on every update.

### A second PC

A second PC does not need to compile Hub. On a PC that already has it, make a pairing code
(Settings > Machines, or `hubd pair`). Then, on the new PC (only git is needed, for updates):

```powershell
powershell -ExecutionPolicy Bypass -File scripts\install.ps1 -From http://<that-pc>:7878 -Code ABCD-EFGH
```

It takes that PC's installed Hub. If that fails it builds from source as above.

## Develop

```powershell
cd app; npm install; cd ..
cargo build -p hubd
cd app; npm run tauri dev     # starts hubd if it is not running
```

Stop the installed Hub first (it holds port 7878), or run the dev core on another port. A dev
build never updates itself.

The first message in a thread downloads the Claude Code ACP adapter with `npx`, so it takes a few
seconds longer than the rest.

## Config

`hubd` reads `%LOCALAPPDATA%\hub\hub.json` (or the file in `HUB_CONFIG`). Everything is optional.
The defaults are:

```json
{
  "bind": "127.0.0.1:7878",
  "default_engine": "claude",
  "engines": {
    "claude": { "command": "npx.cmd -y @agentclientprotocol/claude-agent-acp@0.107" },
    "codex": { "command": "npx.cmd -y @agentclientprotocol/codex-acp@2.7" }
  }
}
```

Any program that speaks ACP on stdio can be added as an engine.

`"tidy": { "engine": "claude", "model": "haiku" }` sets which engine and model turn a long dump
into a prompt (the defaults are the default engine and its Haiku model).

## Tests

```sh
cargo test --workspace       # includes end-to-end thread tests against crates/mock-agent
cd app && npm run check      # Svelte and TypeScript
cd app && npm test           # vitest
```

`cargo test -p hub-core` also regenerates the TypeScript types in `app/src/lib/bindings` from the
Rust model, so the app and the core cannot drift apart.

To try the UI without spending tokens, point an engine at the mock agent:
`"engines": { "claude": { "command": "<repo>/target/debug/mock-agent" } }`.
