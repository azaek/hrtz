# hrtz

A Windows app for gaming mice: Tauri 2 (Rust) with a React 19 front end. Fluent UI v9 is the base design system, and shadcn/ui (on Base UI) fills in where Fluent lacks a control. It supports one mouse family at a time (Zaopin first), each with its own limits and copy.

## Commands

- `pnpm tauri dev`: run the app (Vite on port 1420; the window reloads on save).
- `pnpm dev:sim`: the same, with a simulated Z2 mini beside the real mice (`pnpm dev:sim z2mini,z1promax` for more), to try several mice with one.
- `npx tsc --noEmit -p .`: type-check the front end.
- `npx vite build`: production front-end build.
- `cd src-tauri && cargo test --lib`: Rust tests.

## Releases

- `CHANGELOG.md` feeds the app's "What's new" and each GitHub release's text. Add a user-facing change under `## Unreleased`, at the top.
- **A release is cut by pushing its tag**, from the tip of `main`: `git tag v0.2.0 && git push origin v0.2.0`. There's no version to bump by hand.
- **Only the maintainer decides to release.** A coding agent never pushes a release tag on its own: it finishes the change, runs the checks, and asks first. Fixes are tried in a dev build before they're tagged.
- The tag runs `.github/workflows/release.yml`: the type-check, `cargo fmt --check`, clippy with warnings as errors, and the Rust tests. Then `scripts/release.mjs` stamps the release (the "Unreleased" heading becomes `## 0.2.0 — 2026-11-02`, the day in the maintainer's time zone, and the version goes into `package.json`, `Cargo.toml`, `Cargo.lock` and `tauri.conf.json`), and a signed NSIS installer goes into a **draft** GitHub release with those notes. After a good build, the stamped files go to `main` as "Release 0.2.0" (by github-actions), so pull before the next change.
- Publish the draft to ship: installed copies read `latest.json` from the latest release and update themselves.
- The updater's signing key is `~/.tauri/hrtz-updater.key` (no password), set on the repo as `TAURI_SIGNING_PRIVATE_KEY`. Its public half is the `pubkey` in `tauri.conf.json`. Only the release build signs (`tauri.release.conf.json` turns on `createUpdaterArtifacts`), so `pnpm tauri build` works without the key. Lose the key and installed copies can't take updates, so keep a backup.
- Run the checks before pushing, as the release does: `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` in `src-tauri`, with `rustfmt.toml`'s width of 120.
- The release builds with the newest stable Rust, whose clippy can have lints an older one lacks (1.99 added `chunks_exact_to_as_chunks`). Keep the local toolchain current with `rustup update stable`, or run the workflow by hand to check main.

## Layout

- `src-tauri/src/`: the tray, the OSD, the HUD, raw input and the stores.
  - `mice/`: what talks to the mice. `engine.rs` runs a mouse's connection on its own thread;
    `model.rs` is what the UI reads and writes. Each protocol family is a folder (`compx/` for
    Zaopin) with its driver behind the `Driver` trait in `mice/mod.rs`, and a simulated mouse the
    tests run against (`compx/sim.rs`). The plan for several mice is the notes' `PLAN-mice.md`.
- `src/pages/`: one file per sidebar page.
- `src/components.tsx`: the Fluent building blocks (`SettingCard`, `Group`, `Choice`, `NavRow`, …).
- `src/kit/`: the shared controls below. **Use these before writing new ones.**
- `src/cards/`: cards with real logic (for example `dpi-stages/`), built from the kit.
- `src/components/ui/`: shadcn primitives (CLI-added, style `base-nova`). Use them through the kit, or inside a `.tw` part.
- `src/shadcn.css`: Tailwind and shadcn tokens, scoped to `.tw` parts. `base.css` and `states.css` hold the Fluent overrides and the soft states.
- `hrtz-notes/` (git-ignored): the maintainer's private notes repo, cloned here: the design record (`DECISIONS.md`), plans, handoff and the full protocol notes. Code comments that cite `DECISIONS.md` mean this copy.
- `lab-*.html` + `src/lab/`: dev-only design labs, not part of the build. `lab-app.html?page=lab` runs the real app in a browser on a stand-in backend with this PC's data, to look at any page as built. A lab whose direction has been picked is deleted once it's built.

## Building UI

**Every card is a Fluent `SettingCard`** inside a `Group`: the icon on the left (a `*20Regular` icon), the title, a one-line description, and the control on the right. For a list of items, pass the card a `CardList` as `below`. For a choice between a few values, use `Choice`, whose options are all the same width. For on/off, use Fluent's `Switch`. The exception is master and detail (the Buttons and Profiles pages): two `Pane`s side by side.

### The kit (`src/kit`)

| Need | Use |
|---|---|
| A number with − / + (and typing, ↑/↓, the wheel) | `NumberStepper`. It shows each step at once and calls `onChange` once the value settles, so holding a button doesn't write per step. |
| A shadcn input group that matches Fluent fields | `fieldLook` on the `InputGroup` (the Fluent button's fill and edge, plus the `tw` scope) |
| An icon-only button | `IconButton` (32px; a bare Fluent icon button is 28px here) |
| A row's ⋯ actions | `MoreMenu`: Fluent's menu, with `*16Regular` icons and `"divider"` entries. An item that opens a field (Rename) wraps its handler in `afterMenu`, or the menu takes the focus back. |
| Picking a colour | `ColorMenu` with a `palette`: a Fluent button showing the colour and a menu of swatches |
| Items inside a card | `CardList` (edge to edge, hairline separators, optional `footer`) and `CardListRow` (hover wash only) |
| The line under a list | `CardNote` (a note, amber when it's a warning, or a hint) or `UndoNote` (after a removal) |
| A control that appears on hover in a row | `HoverSwap`: the quiet text normally, the control on hover or while in use |
| "This is the current one" | `AccentBadge`, the only place the accent colour shows |
| A card surface a SettingCard doesn't fit (master and detail) | `Pane` with `PaneHeader` and `PaneFooter`: the card's fill, edge and corners. A master list shorter than the window takes `sticky`, so it stays in view while a long detail scrolls. |
| One of several kinds, each with an icon | `TileChoice`: icon tiles in a grid; the chosen one takes the tile tint |
| An edge or corner of the screen | `AnchorPicker`: a 3 × 3 grid of 32px buttons, centre empty; the chosen one takes the tile tint |
| A few headline figures | `StatTile` in a grid: a label, the number large, a line saying what it means. `StatTile.Unknown` is the dash for a figure not known yet. |
| One step of a guided check | `StepCard`: where it is ("Step 1", "Run 2 · 8,000 Hz"), what to do or what was found said big, a line more, something drawn on the right, then progress, a status line with links, and anything below |
| A run counting or a countdown | `ProgressLine`: a thin neutral line that fills (never the accent) |

Something two cards need belongs in the kit, not inside one card. Add it to `src/kit/index.ts` and to the table above.

### Rules we've settled

- **Fluent first.** Menus, dropdowns, buttons, switches and spin buttons are Fluent unless the kit has a version. Pop-up menus are always Fluent `Menu`.
- **Sizes:**
  - Controls are 32px tall. Buttons are normal size, never compact or small; a light action is a Fluent `Link`.
  - Everything in a row is the same 32px: an icon-only button is always `IconButton`, never a bare Fluent `Button` with only an icon (28px).
  - Button and menu icons are 16px (`*16Regular`); card icons are 20px.
- **States are soft.**
  - Hover and press are faint neutral washes. Nothing turns accent-coloured on hover (`base.css` covers Fluent menus and options).
  - Rows only get a hover wash; clicking into a row doesn't mark it.
  - Selection and focus follow `states.css` and `states.json`. Don't add rings or hard outlines.
- **One accent** (`--brand`, from `states.json`), and only for the current item, such as the stage in use.
- **Surfaces sit on Mica.** Fills are translucent, and hairlines use `var(--border)`.
- **Radii:** controls and menu items 4px, flyouts 8px. Fluent cards keep their own.
- **Flyouts that can grow tall** (a list or search) use Fluent positioning with `autoSize: "height"` and `overflowBoundaryPadding: FLYOUT_PADDING` (from the kit), so they shrink to fit below the title bar. Only their list scrolls.
- **Numbers:**
  - Numbers use `tabular-nums`, and units follow the value ("8 ms", "1,600 DPI").
  - `NumberStepper` rounds a typed value to its step quietly (the user is fine with that, 2026-10-15), and holds it to its range quietly too.
  - Other fields round or clamp out loud: the DPI field says "Rounded to 1,250", and a new field doesn't change a typed value silently.
- **Writes to the mouse settle first.** Drags and held buttons show live and write once, when they end.
- **Removing:** removing something happens at once and offers Undo (`UndoNote`). Confirm dialogs are only for device-wide resets.
- **Copy:** sentence case, plain words, and no claims the hardware doesn't back up. Check what a mouse really does before describing it.
  - A card's description is one short sentence saying what the setting does, plus a short second one only for a trade-off or a limit.
  - Neutral and professional, like Windows Settings: "restores yours", "automatically", "when idle", not "puts yours back after", "by itself", "once you let go".
  - Don't say the same thing twice on one screen (a step and its footer, a verdict and its hint).

### shadcn notes

- Tailwind has no global preflight here. Its reset applies only inside `.tw`. Kit parts carry `tw`, and so do the shadcn pop-up positioners in `components/ui`.
- Tailwind only generates classes it finds in `src/kit`, `src/cards`, `src/components/ui` and `src/lab` (the `@source` lines in `shadcn.css`). Pages don't count: put a page's Tailwind layout in a card component under `src/cards`, and keep the page file thin.
- After every `pnpm dlx shadcn@latest add`:
  - Check that the new files import `cn` from `@/lib/utils`. The CLI once rewrote this to a stray npm package called `cn`.
  - Add `tw` to any new positioner.
  - Round pop-ups with `rounded-xl` (8px).
- shadcn components style by `data-horizontal` / `data-vertical`, and `shadcn.css` maps these to Base UI's `data-orientation`.
- Inside Fluent markup, Tailwind utilities lose to Fluent's own classes (theirs are unlayered). Restyle Fluent parts with `style` or `makeStyles`, not utilities.

### Exploring a new design

Don't guess at a card: build the directions in a lab, a `lab-*.html` page that shows them inside the real app (below), with a picker and a panel of numbers to tune. Let them be compared live, tune the numbers on the panel, then record the choice, with the rejected options, in the design record (`DECISIONS.md`).

A lab can show its directions inside the real app: `src/lab/Frame.tsx` renders `App` at the window's size with one page swapped (`App`'s `pages` and `start` props), the picker and a panel. It runs on `src/lab/fake/backend.ts`, which answers the app's commands and sends its events from `src/lab/data/` (this PC's settings, backups and games, written by the ignored `dump_lab_*` Rust tests; git-ignored, since it holds personal paths). Keep the stand-in's commands in step with `src/api.ts`.

## Commits

Never add a `Co-Authored-By` line, or any other credit to an AI, in a commit or PR.
