<img width="1280" height="640" alt="og_banner" src="https://github.com/user-attachments/assets/dd6d2e28-adbe-43df-886b-d1e87f282abb" />

A free, open-source app for gaming mice on Windows, made for each mouse it supports. It starts with
Zaopin's mice, in place of their own "Mouse Drive Beta" app.

## Supported mice

| Mouse | Receiver | Status |
|---|---|---|
| Zaopin Z1 PRO MAX | 8K receiver | Tested |
| Zaopin Z1 PRO, Z2, Z2 mini, Z2 MAX, Z2 mini MAX | | Recognised, not tested yet |

More mice come one family at a time, each tried before the next.

## What it does

- **Sensor:** DPI stages from 50 to 30,000 with a colour each, polling rate up to 8,000 Hz, lift-off
  distance, sensor mode, motion sync, angle snapping, ripple control, click debounce, and the peak
  performance timeout.
- **Buttons:** any of the six buttons as a mouse button, DPI, scroll, a media key, a key
  combination, rapid fire, hold-for-DPI or the polling switch.
- **Profiles:** the mouse's four profiles, named, and switched by themselves when a chosen app or
  game comes into focus.
- **Battery:** the level over time, how long a charge lasts at each polling rate, charge cycles and
  capacity, a battery saver that lowers the polling rate when it runs low, and how soon the mouse
  sleeps.
- **In-game HUD:** battery, polling rate, DPI and profile at the edge of the screen, in a window of
  its own. Nothing is injected into games.
- **Lab:** checks for the connection (the polling rate that runs best on your PC), tracking, lift-off,
  DPI accuracy, double clicks and the scroll wheel, a map of where the mouse travels on the pad, and a
  look at Windows' pointer settings. The timing checks run on a screen of their own, the way a game
  reads the mouse.
- **Backups:** full copies of the mouse's settings, restores that show what changes and can be undone,
  export and import.
- **Pairing**, a tray icon with the battery level, a low-battery alert, and a pop-up when the DPI or
  profile changes.

Settings are stored on the mouse, so they keep working on PCs without hrtz.

## Safety

- hrtz writes to the mouse's memory only the settings you change.
- It saves a backup of the mouse before a restore or factory settings, and when it first meets a
  model. Backups are in `%APPDATA%\com.azaek.hrtz\backups`.
- Lab checks that change a setting (the polling rate, the lift-off distance) put yours back when
  they finish, when you press Esc, or when you leave the check.
- Quit the vendor app while using hrtz: both talk to the mouse on the same channel, and the vendor
  app turns the mouse's status messages off when it's minimised.

## Install

There's no release yet. Build it from source:

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
| `src-tauri/src/protocol.rs`, `settings.rs` | The mouse's packets, and its memory as typed settings, written field by field |
| `src-tauri/src/device.rs` | The device thread: requests, status messages, sleep and wake, plugging in and out, pairing |
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

hrtz isn't made by or affiliated with Zaopin or any other mouse maker. "Zaopin" and the model names
are Zaopin's. The mouse pictures in `src/assets/models` are Zaopin's own product renders, taken from
their driver app. They aren't covered by this project's licence, and are here only to show which
mouse is connected (see [`NOTICE.md`](src/assets/models/NOTICE.md)). If Zaopin asks, they will be
removed.
