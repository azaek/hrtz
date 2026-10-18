<img width="1280" height="640" alt="og_banner" src="https://github.com/user-attachments/assets/dd6d2e28-adbe-43df-886b-d1e87f282abb" />

A free, open-source app for gaming mice on Windows, made for each mouse it supports: Zaopin's mice,
in place of their own "Mouse Drive Beta" app, and Logitech's PRO X SUPERLIGHT, without G HUB.

## Supported mice

<!-- supported:start (written from the app's model lists; `cargo test --lib readme` checks it) -->
| Mouse | Status |
|---|---|
| Zaopin Z1 PRO MAX | Tested |
| Zaopin ZI PRO | Not tested yet |
| Zaopin Z2 | Not tested yet |
| Zaopin Z2 MAX | Not tested yet |
| Zaopin Z2 mini | Not tested yet |
| Zaopin Z2 mini MAX | Not tested yet |
| Logitech PRO X SUPERLIGHT | Tested |
| Logitech PRO X SUPERLIGHT 2 | Not tested yet |
| Logitech PRO X SUPERLIGHT 2 DEX | Not tested yet |
<!-- supported:end -->

Zaopin mice connect by their receiver or cable, Logitech's by their LIGHTSPEED receiver or cable.
"Tested" means tried on the real mouse; the others use the same protocol as a tested one and should
work. More mice come one at a time, each tried with its owners before it's called tested.

## Help test it

hrtz is in beta. If you have one of the mice above, these help most:

- **A model that isn't tested yet:** its card shows an amber **Not tested** badge. Click it, or open
  Settings → Mouse → **Help test your …**: a 2-minute check on a full screen (move the mouse, press
  each button, and hrtz changes one DPI stage and puts it back) saves a report and opens a GitHub
  issue to send it in. A few of those and a model moves to Tested.
- **Something wrong:** Settings → **Report a problem** opens an issue with your hrtz version, mouse,
  firmware and Windows filled in.
- **Questions and ideas:** [Discussions](https://github.com/azaek/hrtz/discussions).

## What it does

Each mouse gets what it has; hrtz shows only the settings yours does.

- **Several mice:** every mouse hrtz knows is a click away, and the tray, the HUD and the pop-ups
  follow the one you're using.
- **Sensor:** DPI stages you can type or drag, and the polling rate (up to 8,000 Hz on a mouse that
  has it). On a Zaopin also a colour for each stage, lift-off distance, sensor mode, motion sync,
  angle snapping, ripple control, click debounce, and the peak performance timeout.
- **Buttons:** each button as a mouse button, DPI, scroll or tilt, a media key or a key combination.
  On a Zaopin also rapid fire, hold-for-DPI and the polling switch; on a Logitech, switching
  profiles.
- **Profiles:** the mouse's own profiles (four on a Zaopin, five on a Superlight), named, and
  switched by themselves when a chosen app or game comes into focus. A Logitech keeps the names, so
  G HUB shows them too.
- **Battery:** the level over time, how long a charge lasts at each polling rate, charge cycles and
  capacity, a battery saver that lowers the polling rate when it runs low, and (on a Z1 PRO MAX) how
  soon the mouse sleeps.
- **In-game HUD:** battery, polling rate, DPI and profile at the edge of the screen, in a window of
  its own. Nothing is injected into games.
- **Lab:** checks for the connection (the polling rate that runs best on your PC), tracking, lift-off,
  DPI accuracy, double clicks and the scroll wheel, a map of where the mouse travels on the pad, and a
  look at Windows' pointer settings. The timing checks run on a screen of their own, the way a game
  reads the mouse.
- **Backups:** full copies of the mouse's settings, restores that show what changes and can be undone,
  export and import, and factory settings.
- **Pairing** (Zaopin), a tray icon with the battery level, a low-battery alert, and a pop-up when the
  DPI or profile changes.

Settings are stored on the mouse, so they keep working on PCs without hrtz.

## Safety

- hrtz writes to the mouse's memory only the settings you change.
- It saves a backup of the mouse before a restore or factory settings, and when it first meets a
  model. Backups are in `%APPDATA%\com.azaek.hrtz\backups`.
- Lab checks that change a setting (the polling rate, the lift-off distance) put yours back when
  they finish, when you press Esc, or when you leave the check.
- Quit Zaopin's app while using hrtz with a Zaopin: both talk to the mouse on the same channel, and
  Zaopin's app turns the mouse's status messages off when it's minimised.
- G HUB can stay installed beside a Logitech, and hrtz says while it runs. With G HUB's On-board
  memory mode on, both change the same profiles on the mouse and the last change wins; with it off,
  G HUB takes the mouse over while it runs, and changes made in hrtz wait until it quits.

## Install

Download `hrtz_<version>_x64-setup.exe` from the [latest release](https://github.com/azaek/hrtz/releases/latest)
and run it. The installer isn't code-signed yet, so Windows SmartScreen may warn about it: choose
**More info**, then **Run anyway**. After that, hrtz updates itself: a new version downloads in the
background, and **Restart to update** in the sidebar or the tray menu installs it.

Quit Zaopin's own app first, and for a Logitech, G HUB or its On-board memory mode (see Safety).

### Build from source

1. Install [Rust](https://rustup.rs), [Node.js](https://nodejs.org) 20 or later, and
   [pnpm](https://pnpm.io). Windows 11 has the WebView2 runtime the window needs built in.
2. Then:

   ```
   pnpm install
   pnpm tauri dev
   ```

`pnpm tauri build` makes an installer (NSIS) in `src-tauri/target/release/bundle`.

## Develop

- `pnpm tauri dev` runs the app; the window reloads on save, and a Rust change restarts it.
- `cd src-tauri && cargo test --lib` runs the Rust tests; `npx tsc --noEmit -p .` checks the front
  end.
- [`CLAUDE.md`](CLAUDE.md) has the layout, the UI kit and the UI rules, for people and coding agents
  alike.
- [`docs/PROTOCOL.md`](docs/PROTOCOL.md) describes how hrtz talks to the mouse and its receiver.

| Path | |
|---|---|
| `src-tauri/src/mice/` | The mice: `engine.rs` runs a mouse's connection (sleep and wake, plugging in and out, the battery), `model.rs` is what the UI reads and writes |
| `src-tauri/src/mice/compx/` | Zaopin's protocol: the packets, the memory as typed settings written field by field, the models, and a simulated mouse for the tests |
| `src-tauri/src/mice/hidpp/` | Logitech's HID++ 2.0, ported from OpenMouse (see `PORTED.md`): finding the mouse on its receiver, its features, battery, and the onboard profiles it keeps its settings in, with a simulated Superlight for the tests |
| `src-tauri/src/meter.rs`, `diag.rs`, `capture.rs` | Raw input from the mouse, the connection and stroke analysis, and the Lab's game screen |
| `src-tauri/src/pad/`, `clicks.rs` | The mousepad map, and double clicks counted over days |
| `src-tauri/src/hud/` | The in-game HUD, drawn natively |
| `src-tauri/src/battery.rs`, `battery_log.rs` | The battery estimate and its history |
| `src-tauri/src/store.rs` | Preferences, per-app rules and backups |
| `src/pages/`, `src/cards/`, `src/kit/` | The pages, the cards with logic, and the shared controls |

## License

[AGPL-3.0-or-later](LICENSE), except the mouse pictures below. Support for more mice is to be ported
from OpenMouse's `mouse-protocol` under the same licence; [`PORTED.md`](PORTED.md) lists what has
been.

## Images and trademarks

hrtz isn't made by or affiliated with Zaopin, Logitech or any other mouse maker. "Zaopin", "Logitech",
"LIGHTSPEED" and the model names are their makers'. The mouse pictures in `src/assets/models` are the
makers' own product renders, taken from their apps and sites (Zaopin's driver app, Logitech G HUB
and logitechg.com). They aren't covered by this project's licence, and are here only to show which
mouse is connected (see [`NOTICE.md`](src/assets/models/NOTICE.md)). If a maker asks, theirs will be
removed.
