---
name: animation-guide
description: Check or write UI motion against 20 rules (easing, durations, press, popovers, lists, exits, gestures). Works alone with defaults, or with a project rulebook (profile) that overrides them.
---

# Animation guide (20 rules)

Project-neutral. It depends on no design system: the values below are defaults. A project can plug its own
rulebook in as a **profile** (see "Rulebook"); a profile only overrides, it never has to exist.

Source: the list from the "20 Ways to Make Beautiful AI Animations" video (screenshot plus
Christoffer's dictated read-through, which matches it). The 20 titles and their numbers (150-250ms,
30ms, 0.97, 12%, 0.95, 20%) are the video's; the other values, marked `~`, are my defaults. This is a prompt/reference file, not an installed Claude skill
(skills cannot be created from here). To use it: paste it, or point an agent at it, and say
"check <files> against the animation guide".

## Rulebook (optional profile)
A profile is a small markdown file with these headings; any heading may be left out:
`## Tokens` (curve and duration names/values that replace the defaults) · `## Exceptions` (what is allowed to break a rule, and why) ·
`## Never` (project bans) · `## Checks` (greps/commands for this codebase) · `## Platforms` (web, iOS, ... specifics).
How to use: "check <files> against the animation guide with profile <path>". Precedence: profile, then this guide's defaults.
No profile given: use the defaults and say so in the report. A report always names the profile it used.
Hub's own profile is `design/animation-profile-hub.md`; copy it as a starting point for another project.

## How to run a check
1. List every `transition`, `animation`, `@keyframes`, Svelte `transition:/in:/out:`, `withAnimation`, `.transition(` in the files.
2. For each, note: property, duration, curve, trigger, whether it has an exit.
3. Hold it against the rules below; report `file:line · now → proposed · rule #`.
4. Fix with the smallest change; prefer the profile's tokens over new literals.

## Default tokens (a profile may rename or replace)
`press 100ms` · `fast 120ms` (hover, chevrons) · `pop 160ms` (menus, popovers) · `move 220ms` (layout moves, panels) ·
`ease-out cubic-bezier(0.22, 1, 0.36, 1)` · `ease-move cubic-bezier(0.65, 0, 0.35, 1)` (ease-in-out, rule 9)

## The rules

**Entrances and exits**
1. **Ease-out for entrances.** Things arriving decelerate. Use the ease-out token. Never `ease-in` on enter, never `linear` except spinners and meters.
2. **Dropdowns 150-250ms.** Menus, selects, popovers: `~160ms` open. Under 100 reads as a flicker, over 250 as lag.
3. **Stagger list rows by 30ms.** Each row delayed `n x 30ms`, cap at about 6 rows (180ms) so the tail never waits.
10. **Start popovers at 0.95.** Open `opacity 0 + scale(0.95)` to `1`. Modals can start at 0.96-0.98. Don't start from 0.
11. **Enter from the trigger.** `transform-origin` is the trigger's edge or point (a dropdown grows from its button, a context menu from the cursor), not the centre.
12. **Exit to the trigger.** Closing runs back to the same origin; the thing returns to where it came from.
13. **No bounce on everyday UI.** No overshoot curves (`y > 1`) in menus, buttons, toggles, lists. Overshoot only in playful, rare spots a profile names.
15. **Exits 20% shorter than entrances.** Open `160ms`, close `~130ms`. Leaving should never block the next action.
16. **Overlap old and new.** Crossfade / start the new state before the old one finishes; no empty frame between them. Never fade out then fade in in sequence.

**Press and hold**
4. **Scale to 0.97 on press.** Buttons, chips, cards that act: `:active { transform: scale(0.97) }`, `~100ms`. Release returns on the ease-out token. Not on text-like rows.
14. **Don't tween while held.** While a thing follows a finger or cursor (drag, slider, resize), `transition: none` on the dragged property; tween only the release (snap, settle).
5. **Stretch moving knobs 12%.** A switch or slider thumb widens about 12% while pressed or moving (`scaleX(1.12)` or width +12%), settles on release.
19. **Rubber band past bounds.** Drag beyond an edge moves at a fraction (about 0.3-0.5) of the finger and springs back. On iOS use the system scroll bounce or a `.snappy` return; on the web resist, never hard-stop.

**Movement and state**
6. **Stagger as details settle.** After a container lands, its inner details (icon, text, meta) arrive 1-3 steps later, 30-60ms apart. Container first, content second.
7. **Arcs for object movement.** Something travelling between two places that is not a UI edge move (a chip flying to a tab, a card to the inbox) follows a curved path: x and y on different delays/curves. Straight lines are for edge slides.
8. **Pulse to confirm success.** Success = one short pulse (scale 1 to 1.06 to 1 or a brief green flash, `~200ms`), once, then still. Not a loop. In Hub: `@keyframes pulse-ok` and `.pulse-ok` in `app.css` (reduced motion and `.calm` turn it off); use it instead of a looping "sent" or "saved" state. Waiting states that should hold still (amber stamp, busy lamp, skeleton rows) are static colour, not a blink or shimmer. Symmetric loops for live things keep `ease-in-out`: that is the documented exception.
9. **Ease-in-out for moves.** Something that starts and stops on screen (a pill sliding between tabs, a panel resizing) uses the ease-move token; ease-out is for things arriving from off-screen or from nothing.

**Reveals and identity**
17. **Masks for reveals and colour.** Reveal with `clip-path` / `mask` (a wipe, an expanding circle) and change colour by a wipe or `color-mix` transition instead of cross-fading two layers of opacity. Heights: animate a wrapper with `overflow: hidden` and grid `0fr to 1fr`, not `height: auto` jumps.
18. **Crossfade icons + blur.** Swapping an icon: old out and new in together, `scale 0.8 to 1`, `opacity`, and `blur(2px to 0)`, `~160ms`. Hides the pop between two shapes.
20. **Reuse the same layoutId.** One element that exists in two places or states (selected pill, thread card to its page, image to lightbox) is one object that moves, not two that fade. Web: FLIP, View Transitions or the framework's crossfade. iOS: `matchedGeometryEffect` with one id.

## Cheap greps (replace `src/` with the code root; a profile's `## Checks` adds project ones)
- `grep -rnE "(transition|animation):[^;]*(ease-in|ease-out|ease-in-out|cubic-bezier|linear| ease[,; ])" src/` - curve not from a token
- `grep -rnE "cubic-bezier\([^)]*(1\.[1-9]|-0\.)" src/` - overshoot (13)
- `grep -rn ":active" src/ | grep -v "class:active"` - press feedback (4)
- `grep -rnE "scale\(0\.[0-9]+\)" src/` - popover/entrance starts (10)
- `grep -rn "transform-origin" src/` - origin set (11, 12)
- `grep -rnE "withAnimation|\.animation\(|\.transition\(" <ios dir>` - SwiftUI curves
- Every `transition: none` should be a held drag (14).

## Output format for a review
Group by rule number. One line each: `path:line · now · proposed`. End with a plan that splits fixes
into small independent PRs; keep UI-only PRs free of Rust so they take the fast lane.
