# Hub design book

<!-- covers: app/src/app.css, app/src/lib/components/WhereZoom.svelte -->

A reference for how Hub looks, not a law. Christoffer's own word in a thread always wins; if a
change departs from this book, say so in one line and do it. The book grows one rule per pull
request: each section is the decision, why, and how to see where code strays. Defaults, not
requirements.

Source of the direction: the T3 Code design pass (Theo's council run, pull requests
pingdotgg/t3code #24776, #24837, #24865, #25251), kept where it fits Hub.

## Principles (already decided, from CLAUDE.md and the home design)

- Dark, calm and dense. A light theme (Cappuccino) keeps the same shapes.
- Colour only for state; a grey glyph says what a thing is (see "Visual language" in
  ARCHITECTURE.md).
- Hub's own controls, never the browser's (`Select.svelte`, not `<select>`).
- Tokens live in `app/src/app.css`; `design/home-v1.html` is the reference page.

## Corner radius

Four steps, as tokens in `app/src/app.css`:

| Token | px | Used for |
| --- | --- | --- |
| `--r-xs` | 4 | chips, small bars, keycaps, tiny marks |
| `--r-sm` | 6 | small buttons and fields inside a row |
| `--r-md` | 8 | controls, rows, inputs, menu items (the default) |
| `--r-lg` | 12 | containers: cards, the composer, dialogs, menus, panels |

- 1, 2 and 3 px stay as they are (hairlines, scrollbars, minimap ticks).
- A pill or a circle is `999px` or `50%`, never a number that happens to be near half the
  height. A switch, a counter chip and a segmented handle are pills.
- A box inside a rounded box with padding `p` takes the outer radius minus `p`, rounded to the
  nearest step (a 12 px composer with 8 px padding holds 6 px buttons, with 4 px padding 8 px).
- `--radius` (8) and `--radius-sm` stay defined for page frames and older code; new code uses
  the four steps.
- Brand imitations (the OpenWeather card in API connections) keep the brand's own shapes.

**Why:** on the screens measured (Home, thread, new thread, two Settings pages, Machines) 6, 7, 8
and 9 px were used side by side for the same kind of control, plus 5, 10, 12 and 14. Four steps
with a rule per kind of surface keep the look even without anyone having to eyeball it.

**Where code strays:** `grep -rn "border-radius: [0-9]" app/src` should show only 1 to 3 px and
`999px`. Anything else is off the scale.

## Icon line weight

One weight for line icons: `stroke-width: 1.75` on a 24-unit viewBox, round caps and joins,
`stroke: currentColor` (or `var(--muted)` for a quiet glyph). `Icon.svelte`, `.glyph` and every
inline icon follow it, whatever size they are drawn at.

- Charts, sparklines, progress rings, wires on the board and brand marks (`ProviderMark`) draw
  their own weight: they are data or logos, not icons.
- Tiny state marks drawn on a 14-unit box (`StateSign`) keep 1.4 to 1.5, so they read the same at
  their size.

**Why:** the app had line icons at 1.4, 1.5, 1.6, 1.7, 1.8, 1.9 and 2, so neighbours in one row
looked as if they came from different sets. T3 Code settled on 1.75 for the same reason.

**Where code strays:** `grep -rn "stroke-width" app/src` and look for 1.4 to 2 on an icon.

## Contrast

Small text needs 4.5 : 1 against the surface it sits on, large text and icons 3 : 1. Dark passes
today (muted text is 5.5 : 1 on the base). Cappuccino did not: muted text was about 3.0 : 1 on the
paper and 2.6 : 1 on a card, and the green, amber and red state colours were 3.4 to 4.2 : 1.

- Cappuccino `--muted` is `#6a5c52` (5.4 : 1 on the base, 4.7 : 1 on a card, still a step lighter
  than `--text-3`).
- Its state colours are darker: accent `#484c9a`, green `#2c633f`, amber `#7d4b0d`, red `#8e3527`,
  chosen so that the colour on its own tinted fill (a pill, a primary button) is 4.5 : 1 on the
  base and close to it on a card. The tints are 9 to 10 % so they do not eat the contrast.
- Fills (`--green-bg` and the like) follow their colour at 9 to 10 %.

**Why:** T3 Code's light mode had the same fault and fixed it the same way (pingdotgg/t3code
#24776, #25251): quiet text has to stay readable on warm paper.

**Where code strays:** compute the ratio of any new text colour against `--bg` and `--surface`
(WCAG relative luminance) before using it for text.

## Text levels in Settings

Three levels, so a page reads top to bottom without squinting:

| Level | Size / weight | Colour |
| --- | --- | --- |
| Page title (`h1`) | 20 / 600 | `--text` |
| Section title (`h2`) | 16 / 600 | `--text` |
| Intro sentence under a title (`.lead`) | 14 / 400 | `--text-3` |
| Row name (`.name`) | 14 / 500 | `--text` |
| Row description (`.hint`) | 12.5 / 400 | `--muted` |

A name is the only thing in a row that is medium weight; descriptions never get a second colour.

## List rows

Routines, Recipes, Workers and Problems share three tokens in `app.css`: `--row-min` (48 px, the
least height of a row), `--row-pad` (8 px above and below) and `--row-gap` (2 px between a row's
two lines). A new list uses them instead of its own numbers. Hover is `--surface`; actions that
appear on hover also show on keyboard focus.
Geist is bundled with real 500 and 600 weights, so the medium step always shows.

**Why:** T3 Code's settings had three flat levels and were reworked to a 16/600 title, 14/500 row
and 12 muted description (pingdotgg/t3code #24776). Hub's rows were all one weight, so a name and
its hint differed only in colour.

## Lines and dividers

A line is for separating things that space alone cannot: a list's rows, a table, a section head.
Not between controls that already sit in one row.

- The composer's control row has no vertical separators; groups are told apart by a gap.
- Inline code is a soft fill (`--surface-2`) with no border.
- A new control starts without a border; add one only when it must read as a field.

**Why:** T3 Code removed the separators in its composer, the border around inline code and the
split-button hairlines (pingdotgg/t3code #24837): every extra hairline competes with the text.

**Where code strays:** look for `border-left: 1px` and one-pixel-wide spans between buttons.

## Copy

- Sentence case everywhere: "New thread", "Pull requests", "Source control". Capitals only for
  proper names (Hub, Claude, GitHub, Cappuccino, Customs).
- A button says what it does ("Open Settings", "Tell Guard"), not "OK" or "Submit".
- An empty state is one quiet line that says what is missing and, if it helps, what to do
  ("Nothing changed yet."); a row with nothing to show (`+0 −0`, an empty Push) is left out.
- Say it once: the same thing has the same words on every page and on the phone.
- Reading, empty and "needs a newer Hub" are drawn by `Quiet.svelte` with fixed words: "Reading…",
  "None" (or "No keys", what is missing, in two or three words, no how-to when the button is in
  view) and "Needs a newer Hub". A new list uses it, never its own paragraph. A slow read (Overview cards, Machines) draws its
  last answer first (`lastSeen.ts`) and keeps it if the new read fails.
- Cut text at a word, and keep the end that tells things apart (a branch's last part, not its
  first), so "…machine" never loses to "t3/clone-re".

**Why:** T3 Code's design pass fixed one placeholder everywhere, sentence-cased its headings and
hid empty rows (pingdotgg/t3code #24837, #25251). A sweep of Hub's labels found it already uses
sentence case, so this section records the rule rather than changing text.

**Measured 2026-11-04:** the design audit (`app/scripts/design-audit.cjs`) found no text under 4.5 : 1 in dark, and 75 text styles under it in Cappuccino (muted text on cards, tinted pills, the primary button), which this step fixes.

## Spacing

Padding, margin and gap use even steps: 2, 4, 6, 8, 10, 12, 14, 16, 20, 24 and up. No 5, 7, 9, 13 or
15 (unslop polish, 2026-11-06; about 440 values rounded to the nearest step, never more than 1 px).
Dots, icon boxes and hairlines keep their own sizes.

**Where code strays:** `grep -rnE "(padding|margin|gap)[a-z-]*: *[^;]*\b(5|7|9|13|15)px" app/src`
should print only brand looks.

## Type sizes

Whole pixels only: **11** (tags, counts, micro labels), **12** (secondary text, hints, meta),
**13** (body in rows and cards), **14** (names, titles of rows, primary text), **15** (lead text),
**16** (section titles), **20 and up** (page titles and big numbers). Half pixels do not exist.

- The text-size setting (Settings > Appearance) scales these through `vite.config.ts`; a size
  written here is the size at the default setting.
- Text drawn inside an SVG's viewBox (`--keep-size: 1`) sizes with its drawing and is exempt.
- Line height is written with the size where it matters (`font: 13px/1.55 ...`).

**Why:** the stylesheets used 10, 10.5, 11, 11.5, 12, 12.5, 13, 13.5, 14, 14.5 and 15 px
side by side, about a third of all text at a half pixel (a hint at 12.5 beside a name at 13 and
a tag at 11.5). Neighbouring rows read as slightly different sizes. T3 Code's pass was also
about hierarchy first (pingdotgg/t3code #24776). Half sizes round up (11.5 to 12, 12.5 to 13), so
text is up to half a pixel larger, never smaller; `check-layout.cjs` finds no overlap or cut-off
text at default and large text size.

**Where code strays:** `grep -rhoE "font-size: *[0-9]+\.[0-9]px" app/src` should print nothing.

**Eyebrow labels.** A small label above a card or group ("Memory in use", "New threads start on")
is sentence case like everything else: 12 px, muted, normal tracking, no capitals (unslop polish,
2026-11-06; it used to be 11 px uppercase with 0.06em). A label on a value (Memory free, Disk
free) is the same.

**Weights.** 400 and 500 for text, 600 for titles and names that must lead; no 450, 550, 650 or
700. Big numbers share one look: 500, tabular figures, `letter-spacing: -0.02em`.

## Motion

One easing curve for every transition, `--ease` (`cubic-bezier(0.22, 1, 0.36, 1)` in `app.css`).
Durations stay with each piece. A loop is only for something that is happening now (a thread
working, a loading row, a recording mic); a state that waits, like amber "needs you", holds still.
No shimmer on text and no glow that breathes.

**Why:** the unslop run (2026-11-06) found five easing curves for the same kind of hover and amber
dots that pulsed while nothing was happening.

### Animation rules

<!-- covers: design/ANIMATION.md, design/animation-profile-hub.md, app/src/app.css, app/src/lib/motion.ts, app/src/lib/popover.ts, app/src/lib/components/IconSwap.svelte, app/src/lib/components/Select.svelte, app/src/lib/components/ContextMenu.svelte, app/src/lib/components/Switch.svelte, ios/Hub/ -->
Twenty rules from the "20 Ways to Make Beautiful AI Animations" video. Project-neutral guide with how to check: `design/ANIMATION.md`; Hub's values for it: `design/animation-profile-hub.md`. Hub defaults where the video gives no number are marked `~`.

| Case | Rule |
| --- | --- |
| Entrances (1) | ease-out (`--ease`); `linear` only for spinners and meters |
| Menus, selects, popovers (2, 10) | open in 150-250 ms (`~160`), from `scale(0.95)` and `opacity: 0` |
| Origin (11, 12) | grow from the trigger (`transform-origin` at its edge or the cursor) and return to it |
| Exits (15, 16) | 20% shorter than the entrance (`~130 ms`); new state starts while the old one leaves |
| List rows (3, 6) | stagger `--stagger` (30 ms) per row, max 6; inner details follow the container |
| Press (4, 5) | one global rule in `app.css` gives every button `scale: 0.97` on `:active` (100 ms, desktop pointer, off for reduced motion; opt out with `.no-press`, rows, tabs, menu items); the `Switch` knob stretches 12% while pressed |
| Held or dragged (14, 19) | `transition: none` while held; tween only the release; past bounds it rubber-bands |
| Moves (7, 9) | on-screen moves and loops use `--ease-move` (`cubic-bezier(0.65, 0, 0.35, 1)`); travel between places follows an arc |
| Confirm (8) | one pulse (`~1 to 1.06 to 1`, 200 ms), then still; never a loop |
| Reveals (17, 18) | `clip-path` or mask for reveals and colour; icons crossfade with scale 0.8 to 1 and `blur(2px)` to 0 |
| Icon swap (18) | an icon that changes shape (copy to check, play to pause) goes through `IconSwap.svelte`, never `<Icon name={a ? x : y}>`; it overlaps old and new and honours reduced motion |
| Same object (20) | one `layoutId` (iOS `matchedGeometryEffect`, web FLIP in `lib/motion.ts`), not two things fading |
| Everyday UI (13) | no bounce or overshoot curves; only the cube and onboarding may |

**Floating layers use `lib/popover.ts`** (modals and the toast use `lib/pop.ts`). Every menu, select list, picker and sheet gets
`transition:pop|global` on its own element (add `import { pop } from "../pop"`). It scales from 0.95
and fades in over 160 ms with `--ease`, leaves in 130 ms, and grows from the spot the user pressed
(or the focused control), so it returns there too. A layer with a fixed corner passes
`{ origin: "50% 0" }` (a string or a function of the node). It animates the `scale` property, so a layer that
positions itself with `transform` keeps working, and reduced motion turns it off. A new floating layer
without it is a miss. Dialogs with a backdrop use `modal`.

**Where code strays:** the guide ends with the greps; start with `grep -rn ":active" app/src` (no press scale
yet) and menus that appear without an entrance.

**Where code strays:** `grep -rnE "transition:[^;]*(ease-in|ease-out|cubic-bezier| ease[,;])" app/src`
should print nothing, and `grep -rn "infinite" app/src` only live things.

## Marks and floating cards (Inspect mode)

<!-- covers: app/src/lib/components/InspectOverlay.svelte, app/src/lib/components/InspectCard.svelte, app/src/lib/components/Inspector.svelte -->

Anything drawn over the page by a tool (frames, numbered marks, a note box, a card that floats
above the window) follows these, so tools added later look like the same family:

- A mark is the accent: a 1.5 px accent frame with `--accent-bg` fill, `--r-sm`; a hover frame is
  the same dashed. Numbers and letters sit in a pill (`999px`).
- Text on an accent fill is `var(--bg)`, never a written colour: dark on the light accent, light
  on Cappuccino's dark one.
- A floating card is `--r-lg` on `--surface`, a 1 px inset `--line-strong` and one soft shadow; its
  fields are `--r-md` on `--bg`. No hairline between its controls, only space.
- A magnifier lens is a floating card too: `--r-lg`, the same inset line and shadow, no label on it.
- Tool pieces carry `data-inspect-ui` so they are never picked or photographed, and sit above the
  page (z-index 9000 and up) but under the live-mode frame.

**Why:** Inspect mode was the first thing drawn over every page, so it set the look once; the
first draft's hard-coded dark text on the accent was unreadable in Cappuccino.

**Where code strays:** `grep -rn "#1a1918" app/src` should find no text colour.

## Opening a file or page

A card in the chat opens four ways: **unfolded in the chat** (default for a click), **expanded in
place**, **in the right panel**, **in a modal**. A click follows Settings > Chat > Files open; the
caption carries a button for the panel and one that expands, ✕ folds an unfolded card back, and
⋮ holds "Open in a window" (the modal) beside the kind's own entries.

- The unfolded body is the same surface as the card (`--surface`, radius 12), scrolls inside itself
  at most 70vh, and takes no frame of its own.
- **Expanded in place** (the caption's expand button): the card leaves the chat column for the room
  the thread's scroller has. Width is the scroller's less 20 px each side, at most 1280 px and
  centred (a big screen gets air at the sides, a small window gets all it has); height is what the content needs
  (bar plus content at the zoom it is at), at most the scroller's less 40 px and never under 160, so
  a short page leaves no empty box. It animates the height from the card's own in 260 ms
  (`--ease`, none for reduced motion) while the thread scrolls the card to the
  top, so the chat above is out of sight, not gone: ✕ or Esc folds it back and the scroll returns
  to where it was. The composer stays. Its bar is 38 px: name and kind, zoom −/%/+ (click the % to
  go back to 100), panel, ⋮, ✕. Ctrl + wheel zooms (25 to 400 %) around the pointer, wheel scrolls,
  and holding Ctrl or Space lays a catcher over the content so dragging pans it even over a live
  page. The card is `--surface` with a `--line-strong` border and radius 12; the content sits on
  `--bg`.
- Caption buttons are the 24 × 22 quiet icon buttons, radius 8, icons at 13 px and 1.75 stroke.
- Unfolded, panel and modal never rearrange the page around them. Expanded does not either: the
  card sits in the flow of the chat's own scroller, only wider than the column.

**Why:** #704 moved the chat into a narrow column beside the file and the chat could not be
moved back. A choice that does not reflow the window is calmer; expanding in place keeps the
chat one scroll away and has an Esc.

**Where code strays:** a file or page open state that changes `main`'s width or order in `App.svelte`.

## Heat squares

<!-- covers: app/src/lib/components/UseCard.svelte -->

Where a card shows how much each thing is used (the Usage map), it draws one small square per
thing in `--r-xs`, 24 px high, with its name at 12 px and the count at 11 px muted.

- Four steps of brightness, the accent at 10, 22, 38 and 58 % over the card, on a log scale so a
  few heavy things do not wash out the rest. No other colour: heat is not a state.
- Nothing used is a dashed `--line-strong` frame with muted text, not an empty hole.
- Hovering says the count, the last use and the usual time in a tooltip; the card carries no
  legend or heading of its own (the board's frame does).

**Why:** the Usage map is read at a glance for what is never touched, so unused must look different
from "a little used", and a gradient of one colour keeps it calm.

**Where code strays:** a heat grid with half-pixel text or a second hue.

## Color and theme

<!-- covers: app/src/app.css, app/src/lib/themes.ts, app/src/lib/prefs.svelte.ts, app/src/lib/components/ProviderMark.svelte, app/src/lib/components/WhereItGoes.svelte, app/src/lib/components/MachineView.svelte, app/src/lib/components/DaylightCard.svelte, app/src/lib/components/TimeCard.svelte, app/src/lib/components/PluginPage.svelte -->

Mapped 2026-11-04 as a survey, nothing changed in the app. Proposals are at the end and wait for
Christoffer's go; a swatch of them is in `design/color-proposals.html`.

### Themes

All colours are CSS variables on `:root` in `app/src/app.css`. A theme is a block that overrides the
same names, so a component never names a theme.

| Theme | Selector | Set by | Notes |
| --- | --- | --- | --- |
| Dark (default) | `:root` | `prefs.theme = "dark"` | The reference. `color-scheme: dark`. |
| Cappuccino | `:root[data-theme="cappuccino"]` | `prefs.theme = "cappuccino"` | Warm light paper. Ink for text, colour only for state. |
| Chat world | `:root[data-world="chat"]:not([data-theme="cappuccino"])` | `prefs.world = "chat"` (the world button on the rail) | Dark in warm brown, serif titles. Changes surfaces, text and **accent** only; status colours stay Dev's. In Cappuccino the world changes nothing. |

Since 2026-11-05 there are ten themes (eleven since 2026-11-06, see Neutral below) (`app/src/lib/themes.ts`, picked in Settings > Appearance):
Dark (default) and Cappuccino as above, and eight named ones generated from 12-step OKLCH ramps: dark
Coal (warm, orange instead of yellow amber), Graphite (cool grey, blue accent), Ink (blue-black,
muted lilac), Forest (green-black, sand accent), Aubergine (plum, orchid), Smoke (almost no colour),
and light Paper (cool) and Sage (green-grey). Each is a `:root[data-theme="<id>"]` block; green, amber
and red keep the same hues in all of them so status means the same everywhere; muted text, accent and
status all clear 4.5 : 1 on the hover row. The Chat world only recolours the default Dark. A new
theme is: add an id to `themes.ts`, a token block to `app.css`, and check every pair.

**Neutral** (2026-11-06, from the unslop run): dark, a grey with no tint (`#141415` page, `#19191b`
card) and a clear blue accent `#7fb2f0`, so the colour lives in the content and not in the chrome. It
is a theme to pick, not the default: Dark keeps its warmth.

The world is not a theme: it is a second dark palette selected by where you are. A new theme should
override the full token set below, and `app/src/App.svelte` and `ViewerWindow.svelte` clear
`data-theme` for dark, so a new name needs the same treatment there.

### Tokens

| Token | Dark | Cappuccino | Chat world | Use |
| --- | --- | --- | --- | --- |
| `--bg` | `#1a1918` | `#f1e9df` | `#201b16` | The page. 116 uses |
| `--bg-sidebar` | `#151413` | `#efe6db` | `#1a1612` | Sidebar and rail. 34 |
| `--surface` | `#211f1e` | `#e5dace` | `#2b251f` | Cards, panels. 197 |
| `--surface-2` | `#242220` | `#e9dfd3` | `#30291f` | Inset inside a card. 89 |
| `--surface-hover` | `#2a2826` | `#dacdbf` | `#352e27` | Hover and selected row. 320 |
| `--line` | `#2a2826` | `#dfd3c5` | `#352e27` | Dividers, 1 px. 285 |
| `--line-strong` | `#3a3734` | `#c3b3a1` | `#473e34` | Input and card borders. 343 |
| `--text` | `#ece8e2` | `#2a211b` | `#f1e9de` | Titles, body. 621 |
| `--text-2` | `#d3cfc8` | `#463a31` | `#cbbfae` | Secondary text. 392 |
| `--text-3` | `#bdb8b0` | `#54473d` | `#b8ac9b` | Tertiary text. 413 |
| `--muted` | `#958f88` | `#625549` | `#a09484` | Quiet text, hints. 1073 |
| `--accent` / `-bg` | `#a5abee` / 15 % | `#484c9a` / 9 % | `#e2b883` / 16 % | Selection, links, primary buttons. 359 / 82 |
| `--green` / `-bg` | `#84c29a` / 10 % | `#2c633f` / 9 % | as dark | Working, ok. 226 / 25 |
| `--amber` / `-bg` | `#ddaa5e` / 16 % | `#7d4b0d` / 10 % | as dark | Needs you, warning. 234 / 36 |
| `--red` / `-bg` | `#e5806f` / 10 % | `#8e3527` / 9 % | as dark | Failed, destructive. 260 / 43 |
| `--p-*` (10) | pastel, 7.2 to 10.1 : 1 | darker, about 5.3 : 1 | as dark | A project's own colour (violet, blue, sky, teal, lime, orange, rose, pink, sand, slate). Never a state. 8 direct uses, the rest through `looks.svelte.ts` |
| `--daylight-gold` | none (card falls back to `rgb(241,207,142)`) | `#6f5208` | none | Daylight card only |

Rules that hold today: text is always a `--text*` level or `--muted`; state is green, amber, red
(and accent for "selected"); lines are `--line` or `--line-strong`; fills are the `-bg` tints; text
on an accent fill is `var(--bg)`.

### Contrast (WCAG, measured on the token values)

Normal text needs 4.5 : 1. Everything below passes except where marked.

| Colour on | Dark | Cappuccino | Chat world |
| --- | --- | --- | --- |
| `--text` on base / card | 14.4 / 13.4 | 13.1 / 11.5 | 14.2 / 12.6 |
| `--text-3` on base / card | 8.9 / 8.3 | 7.4 / 6.5 | 7.7 / 6.8 |
| `--muted` on base / card / hover | 5.5 / 5.1 / 4.6 | 6.0 / 5.2 / 4.6 | 5.7 / 5.1 / 4.5 |
| accent on base | 8.1 | 6.3 | 9.3 |
| green, amber, red on base | 8.5, 8.3, 6.4 | 5.9, 6.1, 6.5 | as dark |
| green on hover (lowest) | 7.1 | **4.5** | 6.5 |
| `--line` / `--line-strong` against base | 1.2 / 1.5 | 1.2 / 1.7 | 1.3 / 1.6 |

`--muted` on a hovered row is right at the limit in all three themes (4.5 to 4.6). Lines are meant to
be quiet, but `--line` at 1.2 : 1 is close to invisible on a poor screen. Project colours: 7.2 to 10.1
on dark and 5.3 to 5.7 on Cappuccino, all fine.

### Where colours leave the tokens

Counted in `app/src` outside `app.css`. About 20 files hold a literal colour; most are on purpose.

- **Exempt by rule:** brand marks (`ProviderMark.svelte`: Claude `#d97757`, Codex `#5b7bff`, DeepSeek
  `#6b9cf5`), the OpenWeather look and brand tints in `ApiConnections.svelte` (about 49), the terminal
  fallbacks in `TerminalPane.svelte` (they read the tokens first), `#fff` and `#000` for overlay
  scrims and image backdrops (`Lightbox`, `ImageCarousel`, `FilePreview`, `BrowserFrame`), and
  about 60 `rgba(0,0,0,x)` shadows.
- **Engine colours repeated by hand:** `WhereItGoes.svelte` draws Claude `#d97757`, Codex `#9b8cff`,
  DeepSeek `#4d6bfe`, and `ProviderMark` draws Codex `#5b7bff` and DeepSeek `#6b9cf5`. Same engines,
  two sets of blues and violets; they are not the same colour in a bar and in its logo.
- **Pink `#e650bb` for the Jev brand** in `JevBar` and `WhereItGoes`, outside `--p-pink`.
- **Fixed colours that ignore the theme and may stray:** `MachineView.svelte` passes `#78c4e6` (sky) and
  `#e0a96d` (a third amber, not `--amber`) as group tones, which are pastel-for-dark values that are
  light on Cappuccino; `Transcript.svelte` has `#1a1918` for the site favicon ink; `WindowControls`
  uses Windows red `#c42b1c` for close (intended); `Wave.svelte` has `#7fa6ee` (= `--p-blue`);
  `inspectSend.ts` falls back to the dark accent (it reads the tokens first, fine).
- **`DaylightCard`, `TimeCard`** carry their own sky and moon colours (`#c9cbea`, `#dfe3ff`, cream
  `rgb(252,234,190)`); the card is a picture of the sky, so this is by design, but the gold falls back
  to a pastel in dark and has no token there.
- **`PluginPage.svelte`** has a fixed `#1d1c22` hero background that stays dark in Cappuccino.
- **Names used but never defined:** `--surface-raised` (checked: no use), and `--brand`, `--tint`,
  `--card-alpha` are set inline by the component that uses them, which is correct.

### Inconsistencies

1. **The book said Cappuccino red `#9a3d2e`; the code has `#8e3527`** (a later sweep darkened it). The
   Contrast section now says `#8e3527`; nothing else differs between book and code.
2. **Chat world's accent `#e2b883` is amber's twin** (1.1 : 1 against `#ddaa5e`). In that world,
   "selected" and "needs you" are the same yellow, which breaks "colour only for state".
3. **Yellow does a lot.** Amber is "needs you", warning, and in Chat also the accent, plus the
   Daylight gold, `#e0a96d` in Machines and the sand project colour. Christoffer finds it too
   yellow; the pastel `#ddaa5e` is the most saturated warm tone on a dark page.
4. **Two sets of engine colours** (see above), and Claude's orange `#d97757` sits close to `--red`
   `#e5806f` and `--p-orange`.
5. **Chat world only re-colours some tokens.** `--green`, `--amber`, `--red` and `--p-*` are Dev's, so
   on warm brown the greens are cooler than their surroundings. Acceptable, but untested by eye.
6. **Cappuccino's `--text-3` and `--muted` are only a step apart** (7.4 and 6.0 on base) while dark's
   are 8.9 and 5.5. Quiet levels read flatter on paper.
7. **Daylight gold** only exists in Cappuccino, so dark relies on a literal fallback.
8. **Row hover is `--surface-hover` = `--line` in dark** (`#2a2826` both): a hovered row and a divider
   are the same colour, so a divider next to a hovered row disappears.

### Proposals (none applied)

Each is one token change, and `design/color-proposals.html` shows them next to today's values.

1. **Cooler, calmer amber** `#ddaa5e` to `#d9a15f` (7.7 : 1) or `#e0975a` (7.3 : 1, more orange, further
   from "yellow"). Keeps it readable and still apart from red `#e5806f`. Cappuccino stays.
2. **Chat accent apart from amber.** Either use Dev's lavender `#a5abee` in Chat too (one accent
   everywhere, simplest), or a clay `#dc8f72` (6.7 : 1) which belongs to the warm brown but is nearer
   the red. Recommendation: lavender, as colour for state should be the same in every world.
3. **One engine palette**: take the three engine colours from one file (a `--engine-claude`,
   `--engine-codex`, `--engine-deepseek` set) used by both `ProviderMark` and `WhereItGoes`.
4. **Give the sky tone in Machines a token** (`--p-sky`) and drop `#e0a96d` for `--amber`/`--p-sand`.
5. **`--line` a touch stronger** (dark `#2e2c2a`, 1.3 : 1) and **`--surface-hover` a notch off the line**
   (`#2d2b29`), so dividers survive a hovered row.
6. **Give dark a `--daylight-gold`** (`#f1cf8e`) so the card has no hidden fallback.
7. **Before adding a theme:** list every token above, check each colour against its base and card
   (4.5 : 1), and add a `data-theme` value to `prefs.theme`, `App.svelte` and `ViewerWindow.svelte`.

**Where code strays:** `grep -rnE "#[0-9a-fA-F]{3,8}\b" app/src --include=*.svelte` outside the exempt
files above, and any text colour that is not a `--text*`, `--muted`, state or `--p-*` token.

## Charts

Bars in Hub are **Edge bars** (Christoffer's pick, 2026-11-04), made flat in the unslop polish
(2026-11-06): a bar tinted in the colour of what it counts, with a solid top line and no glow, on
the card's own grey. Where it goes
(`WhereItGoes.svelte`, `WhereZoom.svelte`) is the reference.

- The fill is a flat `rgba(a, .3)` with a 1 px inner line at `rgba(a, .35)`; the top edge is a
  solid 2 px line (3 px in a wide chart) in `a`, with no glow and no gradient. The bar for now, or
  under the pointer, fills to `.5`. (`b` is no longer drawn.)
- Colours are the companies' own, as on the limits card: Claude clay `217, 119, 87` to sand
  `204, 160, 112`, Codex sky `110, 140, 255` to iris `150, 120, 255`, Claude in the cloud clay to
  blue `120, 160, 220`. A stacked bar is one flat piece per part, each with its own top line;
  parts that are not companies take the project colours (`--p-*`) in order of size.
- These colours are data, like brand marks: they are exempt from "colour only for state".
- A bar stays a bar: one bar is at most 72 px wide (36 px in a small card), never a block across
  the chart. An empty bucket is a 2 px dim line, so time still reads.
- Labels are 12 px. From the newest back, a label is drawn only where it clears the one after it
  (measured from its length, so a longer language thins out more), and the first and last stay
  inside the chart so nothing reaches the column beside it. A label names the
  turn: the month on the 1st, the weekday at midnight. Hours are `14:00`, never `02:00 PM`.
- An **estimate** (Claude's Elsewhere, from the plan meter) is a part like any other but striped
  (`repeating-linear-gradient(135deg, …)`) and never lit, and it never sets the scale: a chart is
  scaled to the most it counted plus a quarter, and a bar an estimate lifts above that is drawn
  full and cut by a slanted gap near its top, with its sum (≈) above the tallest one.
- A card that opens grows to twice the width of the others; the rest stay side by side.
- A page that can count in more than one unit says so with a small button group at the right end
  of the range row (`$` / `Tokens`), the same buttons as the ranges; the choice is remembered, and
  a unit the hubd cannot count yet is greyed with "Comes with the next version of Hub". Tokens are
  written `106.6 M`, `350 k`, never with decimals of a token.
- A chart over time zooms: the scroll wheel (and + / −) around the pointer, drag or a sideways
  scroll to move, a click to open a bar, hover for its parts in a small card. The bars change what
  they stand for (hours, days, weeks, months) as the window widens, so there are never more than
  about one per 9 px.

- A card for a company takes a **flat tint** of its colour (`color-mix` of the colour into
  `--surface`, 11 to 13 %): Claude clay, Codex blue, Hub plain. No glow clouds.
- A split into parts that is not over time (who asked, which engine, what Hub sent) is **one slim
  strip** (6 px, parts separated by 2 px gaps, `--r-xs`) with the parts named beside it as rows or
  chips carrying one number each. No percent column and no bar per row; the percent is in the
  tooltip. A page has no heading of its own under the tab or row that names it.

**Why:** grey and white bars said nothing about whose use they were, the 30-day labels ran into
each other and the all-time ones into the facts column; plain colours "undersold" it. One guessed
week of Elsewhere ($57 607) flattened every counted bar to a line, so a guess may not set the scale.

**Where code strays:** a bar drawn in a fixed white or grey (`rgba(236, 232, 226, …)` as a fill),
or a chart label that is not thinned by width.

## Switches in a card

<!-- covers: app/src/lib/components/JevBar.svelte -->

A card that lets you turn something off (the Jev card's "Agents can ask Jev") uses the same row as
its other switches: label on the left with a muted one-word hint, `Switch` on the right. The
all-agents switch lives in the gear view; a per-chat switch is one row on the "This thread" tab and
is disabled with the hint "off for all agents" while the global one is off. The Today tab keeps an empty
row of the same height, so the card never changes height when you switch tabs. A switch whose call
the running Hub does not have yet shows "comes with the next version" in place of the switch
(`hubdHas`), never a dead switch. Turning one off is never a stop: it only changes what the agent is
told.

**Why:** Christoffer asked to turn Jev off smoothly for all agents and per chat (2026-11-06).

## Focus on the phone

<!-- covers: ios/Hub/FocusMode.swift, ios/Hub/Frame.swift -->

Focus (one working project) looks the same on the phone as a state, not a theme: the project's own
colour (`Hub.projectColor`) and a lock.

- The project button in the pill takes the project's colour, a 1.5 pt ring and a small accent lock; nothing
  else in the pill changes.
- The focused project is one hero card on the Projects page (tint at 22 % fading to the surface, 16 pt
  corners, numbers in dark boxes); the others are rows, not cards: a folder in a tinted square, one meta
  line, the to-test pill on the right, a ring for the stage.
- A project that is not the focused one carries an accent capsule, "Peeking", with one button back.
- A project's own home is a grid of parts, three across, each with an icon, a name and one live fact.

**Why:** the sketches "Projektsidan" (2026-11-04) chose list C and project view B; focus has to be seen
from every page without a banner on all of them.

## Inbox density and the scope line
<!-- covers: app/src/lib/components/ActivityList.svelte, app/src/lib/components/ScopeBar.svelte -->

The Inbox has three densities (`prefs.cards`, Settings > Sidebar > Thread cards), all on the same
tokens: **comfortable** three lines, **compact** two (11 / 600 project in its `--p-*` colour, 12 title,
11 muted state line), **dense** one (12 title, a 6 px project dot, 11 time). State is always the sign
(amber `?`, green turning ring), never a fill, at every density; "Asks you" stays as a word down to
compact.

What the list is narrowed to is a divider line, like "Pinned", not a chip and a dropdown: project
icon, name in its colour and a count, a `--line` rule, machine glyphs in an inset `--surface` group
(`--r-md`, glyphs 13 px at stroke 1.75, the chosen one on `--surface-hover`), then a cross. The
glyphs adapt: none with one machine, all up to four, else three plus a "+N" menu. A row inside a
list that is already one project does not repeat the project name.

**Why:** sketches "Trådkort" and "Filter" (2026-11-05): many parallel threads must fit, and the rail
already shows the project, so the old chip and "All machines" line only cost two lines.

## Permission mode under the input
<!-- covers: app/src/lib/components/Composer.svelte, app/src/lib/components/Home.svelte -->

The permission mode is always a pill under the input, in the composer of a thread and in the one for
a new thread. A new thread shows the mode the core starts it in, **Auto** (Hermes: Don't ask), not
the engine's "Default"; picking another one there is sent, an unpicked one is not, so the core's
default stands. Only the danger mode gets the danger colour.

**Why:** Christoffer wants agents to decide and nothing to start in manual (2026-11-06); the pill
used to say Default before the thread existed, which read as manual.

## Questions that do not wait
<!-- covers: app/src/lib/components/AskLine.svelte, app/src/lib/components/ComposerDock.svelte -->

A question the agent asks without waiting (`ask_user`) is quiet: a help glyph, the question in
`--text-3` and what the agent assumed in `--muted` 12 px, no status dot and no red or amber, since
the thread is not waiting. Docked over the box it has the same numbered options as a permission
question, minus the dot. In the chat the options show as small outlined buttons (`--r-sm`) until
it is answered, then fold behind the line, which says "you said Z" and can be clicked to change it.

**Why:** Christoffer's rule (2026-11-06): an agent's question never blocks the thread or says
"Needs you"; his answer arrives later as a normal message.

## First steps note

<!-- covers: app/src/lib/components/ProjectView.svelte -->

A short note on a page that says what to do next, shown once (the project page right after New
project): a `--surface` strip with `--r-md`, 13 px `--text-2` lines of one sentence each (two or three
lines at most), a link inside a line is `--text` and underlined, and a 14 px `x` ghost button on the
right hides it. No accent colour and no icon on the left: it is a hint, not a state. Link in the header
of a thread (the quick thread's "Make project") is a `crumb`: text in the header's own grey that
underlines on hover.

## Guide page (Welcome)
<!-- covers: app/src/lib/components/Welcome.svelte -->

A page that explains rather than shows data: a 640 px column, a title, one grey lead line, small
muted headings ("After you create a project"), then cards (`--r-lg`, `--surface`, one hairline).
A step is a numbered 20 px circle (`--surface-2`, muted number), a 14 px title, a 13 px `--text-3`
line, and at most one accent link. No icons in the cards, no colour for anything but the link.

**Why:** T3 Code's welcome is a few lines and a button; the same weight keeps a guide from
becoming a manual nobody reads.

## Page frame

<!-- covers: app/src/lib/components/Page.svelte, app/src/lib/components/RoutinesView.svelte, app/src/lib/components/RecipesView.svelte, app/src/lib/components/WorkersView.svelte, app/src/lib/components/MachinesView.svelte, app/src/lib/components/PluginsView.svelte, app/src/lib/components/LibraryView.svelte -->

A view that is a page uses `Page.svelte`, not its own `.page` and `header`. It owns the scroll area,
the gutter and `--page-top`, the bottom margin (60 px), the title and the width.

- **Title:** `h1`, 20 / 600, one slot (`head`) after it for a count, a host or an action. An action that
  belongs at the right gets `margin-left: auto`.
- **Widths:** `reading` 760 for text and forms; `list`, `wide` and `full` fill the window (Christoffer wants the
  whole screen used) and differ only in intent, so a cap can be set per
  kind later in one place.
- **`fill`:** the page does not scroll, its body does (Library, a document).
- Not for the thread, the project page or Settings, which have their own chrome. `PageFrame.svelte`
  is the agent page in an iframe, a different thing.

Where code strays: `grep -rn "padding: var(--page-top)" app/src` outside `Page.svelte`.

## Keyboard and focus

<!-- covers: app/src/app.css, app/src/lib/inputMode.ts -->

Anything you can click can be reached and used with the keyboard. A clickable thing is a `button`
(or `a`); where a row must be a `div`/`span`, it carries `role`, `tabindex="0"` and an Enter/Space
handler. The focus ring is one: 1.5 px `--accent`, offset 2 px, shown only after Tab or an arrow key
(`inputMode.ts`), for buttons and for `role="button|link|tab|menuitem|option|switch"`, `a` and
`summary`. A dialog is `role="dialog"` with `aria-modal="true"` and a name, and closes on Esc.
An action that shows only on hover (`.x`, `.del`, row actions) also shows on `:focus-within`, so a
keyboard user can reach it. A small control nested in a row stays out of the tab order
(`tabindex="-1"`); the row itself opens the same thing.

## Coverage: what has been swept

Run `node app/scripts/design-audit.cjs` (dev server and core up) to see where the app stays on
this book. It does two things: a **source scan** of every `.svelte` and `.css` file (radii off the
scale, half-pixel font sizes), which covers every surface whether or not a view shows it, and a
**render audit** that opens 63 views, menus and dialogs in dark and Cappuccino and measures what is on screen
(radii, icon line weights, text contrast, font sizes). Status as of 2026-11-04 (after sweeps 1 to 5):

| Surface | Radius | Icon weight | Type sizes | Contrast (both themes) | Lines | Copy |
| --- | --- | --- | --- | --- | --- | --- |
| Home and its six tabs | source ✓ | source ✓ | source ✓ | rendered ✓ | ✓ | ✓ |
| New thread, thread, inbox list, sidebar | source ✓ | source ✓ | source ✓ | rendered ✓ | ✓ composer | ✓ |
| Composer and its menus (model, +) | source ✓ | source ✓ | source ✓ | rendered ✓ | ✓ | ✓ |
| Right-click menu on a thread, command palette | source ✓ | source ✓ | source ✓ | rendered ✓ | not reviewed | ✓ |
| User menu, idea inbox | source ✓ | source ✓ | source ✓ | rendered ✓ | not reviewed | ✓ |
| Overview, Workers, Machines, Services, Routines, Recipes, Labs, Problems, Library, Plugins | source ✓ | source ✓ | source ✓ | rendered ✓ | by eye ✓ | ✓ |
| Project page (overview, library, GitHub, docs, files, services tabs) | source ✓ | source ✓ | source ✓ | rendered ✓ | not reviewed | ✓ |
| All 22 Settings pages | source ✓ | source ✓ | source ✓ | rendered ✓ | not reviewed | ✓ |
| Cards: Home blocks, machine, plugin, pulse, daylight, receipt, ... | source ✓ | source ✓ | source ✓ | rendered where on a view | not reviewed | ✓ |
| Dialogs and windows that need a state to open (folder picker, share sheet, handoff dialog, doc switcher, lightbox, viewer windows, install window, update centre) | source ✓ | source ✓ | source ✓ | **not rendered**; read through for fixed colours (lightbox, share sheet fixed) | not reviewed | not reviewed |
| Permission, error, sign-in, tour, design and review cards in a thread | source ✓ | source ✓ | source ✓ | **not rendered** (needs a real agent turn); no fixed colours | not reviewed | not reviewed |
| The thread's "..." menu, right-click on a message, find bar, dropdowns (Select) open, new-project dialog | source ✓ | source ✓ | source ✓ | rendered ✓ | by eye ✓ (menus share one look: 13 px, 4 px pad, radius 8) | ✓ |
| Settings > Where it goes, its cards and the zoom chart | source ✓ | source ✓ | source ✓ | dark, by eye (mocked data); Cappuccino not rendered | ✓ | ✓ |
| First steps note on the project page, "Make project" crumb in a thread header | source ✓ | source ✓ | source ✓ | not rendered | ✓ | ✓ |
| iPhone app (SwiftUI) | out of scope here: separate tokens in `Theme.swift` | | | | | |

- **source ✓** means the scan finds nothing off the rule in any file; with it a new stray shows up
  the next time the audit runs.
- **rendered ✓** means the render audit measured it in both themes. Dark has none under 4.5 : 1;
  Cappuccino has one (an offline machine's line in New project, dimmed on purpose). Sweeps 3 and 4
  darkened Cappuccino's muted and third-level text, red and the project colours (`--p-*`), and gave
  the daylight card its own gold (`--daylight-gold`).
- **Dark surfaces inside a light theme.** A surface that is dark in every theme (the lightbox veil)
  pins the dark theme's ink on what sits on it, since Cappuccino's own text would vanish. Colours for
  a state or a kind come from tokens (`--p-*`, `--green`, `--amber`), never fixed pastels: the share
  sheet's icons and the plugin logo had pastels made for dark.
- Brand marks (`ProviderMark`, the OpenWeather look) and `--keep-size` SVG text are exempt.
- "Not reviewed" for lines and copy means nobody has gone through that surface by eye yet; the
  rules in this book are what to review against.

## Tabs

<!-- covers: app/src/lib/components/Tabs.svelte, app/src/lib/components/Settings.svelte, app/src/lib/components/ProjectView.svelte, app/src/lib/components/MachineView.svelte, app/src/lib/components/Customs.svelte, app/src/lib/components/ProblemDetail.svelte -->

Every tab strip is `Tabs.svelte`, never a hand-made row of buttons: 13 px, `8px 12px`, `--muted`, the
active tab `--text`, a 2 px `--accent` underline on a `--line` rule that slides to the picked tab
(off for reduced motion). `items` are `{ id, label, count?, amber?, tip?, end? }`: `count` is the
small number after the label, `amber` is for a count that needs you, `end` pushes that tab and the
rest to the right edge. It scrolls sideways when narrow instead of wrapping and opts out of the
press squash. Spacing under it is the caller's: `style="margin: ..."`. Used by Settings, the project
page, the machine page, Customs and a problem. A filter (ProblemsView's pills) is not a tab strip.

Settings has ten rows in the left list and no group headings, so the list never scrolls. A row whose
parts used to be rows of their own shows them as tabs over the page. Each tab is still one
section id (`SettingsSection`), so every `openSettings("handoffs")` and Ctrl K entry lands on the
right tab. A page promises nothing it does not do: no "Soon" rows; ideas go under Parked in FOCUS.md.

The first tab of a row is named like its row (Account, Safety), so the left label and the visible
tab never differ. A dense page is split by `h2` into blocks of about six rows or fewer: Appearance
is Look (theme, fonts), Size (zoom, text size, per screen) and More; Chat is Message box,
Transcript and Thread list. The animation preferences "Opening a group" and "Switching
subscriptions" are parked (the default stays; ANIMATION.md says one motion). The "restart the core"
banner is a sticky bar at the bottom of the page, so it never pushes the content down.

Account puts the name field on the title line, the two subscriptions side by side as bordered cards with their logos (one column when narrow) (login details behind a
"Details" link, no plan-name row), then GitHub with its logo and a Connect button, then the limit
rules, where Warn at and Switch to a cheaper model take a typed percent. In a thread list that shows
a title twice (two "Guard" threads), the repeats get their folder name after the title.
