# Design brief: Home screen and navigation (round 1)

Paste everything below the line into Claude Design.

---

Design the home screen and navigation for a desktop app: a personal command center for one developer who runs many AI agents, servers and projects at once.

## What the app is
One place to talk to a central AI and see everything that is going on: AI threads (conversations with agents doing work), background workers, servers and services, keys, and projects. The central AI chat is the heart of the app. Everything else supports it.

The user's own tools appear by name:
- **Ask about everything**: a thread on Home that sees all of Hub (no named persona).
- **Knegarn**: background worker that runs jobs unattended.
- **Forman**: the foreman that decides what runs next.
- **Aftermath**: the log of what happened.
Hosts: **PC**, **Plåtnicklas** (always-on home server), **tincan**, **Server**.
App name: use the placeholder "Hub" for now.

## Platform
Desktop app first (around 1440×900, should also work at 1280 wide). A phone layout comes later, so keep the structure simple enough to collapse to one column.

## Look and feel
Dark mode. Muted, calm and simple, in the spirit of the Claude Code and Codex desktop apps: near-black warm-grey background, one soft accent colour, generous line height, monospace only for code, paths, ports and IDs. Few borders, mostly spacing and subtle surface tints. No gradients, no glassmorphism, no dashboard clutter. Status colours (green, amber, red) used sparingly and only for state.

## Layout and navigation
Model it on the Claude Code / Codex desktop layout:
- **Left sidebar** (collapsible):
  - Top: "New thread" button and search.
  - Section "Views": Home, Threads, Workers, Services, Keys. (Arsenal, Tech stack and Projects exist but can be shown as collapsed or "coming soon".)
  - Section "Projects": a list of projects (e.g. Atlas, Aftermath, a new game "Untitled game"), each expandable to show its threads.
  - Section "Recent threads": thread titles with a small status dot (running, waiting on you, done) and host label.
  - Bottom: settings, current AI engine/model, connection status of hosts.
- **Center**: the chat about all of Hub. A large composer at the bottom with: text input, @-mentions for threads/projects/hosts (e.g. "@atlas", "@plåtnicklas"), a model/engine picker (Claude, GPT, DeepSeek, local), and an attach button.
- **Right panel** (collapsible, context-aware): shows details for whatever the chat is about, defaulting to the home summary below.

## Home screen content
When the user opens the app in the morning, the center shows a greeting with a short brief, and the composer is focused. Around or above the composer, show compact cards:

1. **Since yesterday** (from Aftermath): 3–5 lines like "Knegarn finished 'fix capture job' on Plåtnicklas", "Atlas deploy succeeded", each with time and a link.
2. **Needs you**: questions and approvals waiting from threads and workers, each with a one-line ask and quick reply buttons. Show a count badge in the sidebar too.
3. **Running now**: active workers and threads, with name, host, engine, elapsed time and a small live indicator. Example: "Knegarn · refactor auth · Plåtnicklas · 42 min".
4. **Warnings**: quiet list, not pop-ups. Examples: "3 orphaned dev servers on PC (ports 3000, 5173, 5174)", "OpenAI API key not checked in 90 days", "tincan disk 91%". Only critical ones get red.
5. **Hosts at a glance**: one small row per host (PC, Plåtnicklas, tincan, Server) with online state, CPU/RAM bar and number of services/open ports.

Keep cards dense but calm; the chat must stay visually dominant.

## States to show
- Default morning state with data as above.
- A state where the user has typed "what's Forman doing?" and Hub answers inline with a small embedded card of Forman's current jobs.
- Sidebar collapsed.

## Deliverables
Home screen in the default state, the chat-answer state, and the collapsed-sidebar state, plus the sidebar on its own showing hover and selected styles.
