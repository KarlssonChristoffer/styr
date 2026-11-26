# Animation profile: Hub

Plugs into `design/ANIMATION.md`. Hub's own rulebook is `design/DESIGN.md` (Motion); this file only
carries the values that differ from the guide's defaults. Another project copies this file and edits it.

## Tokens
- Curve: `--ease` = `cubic-bezier(0.22, 1, 0.36, 1)` in `app/src/app.css`. Every transition uses it.
- Also in `app.css`: `--ease-move: cubic-bezier(0.65, 0, 0.35, 1)` (ease-in-out: on-screen moves and symmetric loops), `--dur-press: 100ms`, `--dur-fast: 120ms`, `--dur-pop: 160ms`, `--dur-move: 220ms`, and `--stagger: 30ms` (list rows, at most 6 steps; 0 under reduced motion).
- iOS: SwiftUI `.snappy` is the house spring.

## Exceptions
- The working cube (`Working.svelte`, `.dot.running` in `app.css`, `lib/workGrid.ts`) may overshoot and loop; it is the one working indicator.
- Live meters (`Composer.svelte` `.wave`) use `linear`.
- Symmetric pulses for live state use `var(--ease-move)`, never an `ease-in-out` literal.
- User-chosen theme effects in `UserMenu.svelte` (`.fx`) may loop slowly.

## Never
- Loops on a waiting or success state: amber "needs you" holds still; success pulses once.
- Shimmer on text or glow that breathes.
- The browser's own controls (use `Select.svelte`, Hub's tooltips).
- Ignoring `prefers-reduced-motion` (`app.css` zeroes animation).

## Checks
- `grep -rn "infinite" app/src` shows only live things. Audited 2026-11-06 (22 loops): live state (working cube, running tool, busy mic, CI/timeline running bar, tuning dot, writing mark, sending dot) and the `.fx` theme effects; no spinners (a web search shows the working grid) and no amber "needs you" loops. The one-shot restart icon spin in `UserMenu.svelte` (`.age.checking`) is the only turn left; it lasts one refresh.
- Hidden window: `boot.ts` sets `data-hidden` on `<html>` and `app.css` pauses every animation while it is set; reduced motion and Settings > Reduce motion already cut loops to one 0ms pass.
- Durations outside the scale: transitions use `--dur-press/-fast/-pop/-move`; longer literals (fold 0.3s, meters 0.6s, theme fade 420ms) are deliberate. Do not add `0.15s`, `0.2s` or `0.25s` literals.
- Held drags use `transition: none` (see `.slot.lifted` in `Board.svelte`).
- Things above the page (modals, Lightbox, FindBar, bottom panel, toast): `app/src/lib/pop.ts` (`modal`, `slide`, `toast`), 180ms in, 144ms out, ease-out, reduced motion and Settings > Reduce motion give 0ms. A new modal puts `in:modal|global out:modal|global={{ out: true }}` on its backdrop.
- Icon swaps: `grep -rnE '<Icon name=\{[^}]*\?' app/src` should only list icons that differ by item, not by state; state swaps use `IconSwap.svelte`.
- List motion: `app/src/lib/motion.ts` (FLIP); web shared objects go through View Transitions (`app.css`).

## Platforms
- Web: Svelte 5, CSS transitions, `svelte/transition`. iOS: SwiftUI in `ios/Hub/`.
- UI-only PRs stay free of Rust (fast lane).
