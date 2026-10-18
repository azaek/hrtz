# Changelog

Newest first. Each version lists what's **New**, **Improved** and **Fixed**. The app's "What's new" window reads this file, so write for the people using it.

Notes for the next version go under `## Unreleased

### New
- Several mice: the mouse card opens a list of every mouse hrtz knows, to switch between them, and Connect a new mouse.
- Setting up a mouse is a full-page guide, on first run and for each new mouse, with its first backup.
- Each mouse keeps its own profile names, app rules, battery saver and battery history.
- A mouse that isn't connected shows its last settings.
- Remove a mouse you no longer use: Settings shows Remove while it's unplugged, with Undo.
- The tray, the HUD and the pop-ups follow the mouse you're using.
- Logitech's PRO X SUPERLIGHT and PRO X SUPERLIGHT 2, on their LIGHTSPEED receivers or cables, each shown in its own colour: DPI stages, polling rate, profiles and battery. Settings are saved in the mouse's own profiles, so they stay without hrtz running. The SUPERLIGHT 2 hasn't been tried on a real one yet.
- Buttons for Logitech mice: set each button to a mouse button, DPI up, down, cycle or default stage, tilt left or right, a key with modifiers, a media key, the next, previous or cycling profile, or nothing. What G HUB set that hrtz can't edit yet (macros, G-Shift, DPI Shift) is kept as it is.
- Logitech profile names are saved on the mouse, so G HUB shows the names given in hrtz, and hrtz shows those given in G HUB.
- Backups for Logitech mice: every onboard profile, saved when the mouse first connects and whenever you like, and restored with Undo. Each mouse lists only its own model's backups.
- Restore factory settings for Logitech mice: the mouse's own factory profile goes back over each profile, after a backup.
- The Battery page and the HUD show Logitech's own battery-life figure for the first PRO X SUPERLIGHT until hrtz has measured yours.
- Help test a mouse hrtz hasn't been tried on: a "Not tested" badge on its card, and in Settings a 2-minute check on a full screen (movement, each button, one setting changed and put back) that saves a report to send in a GitHub issue.
- A warning while G HUB is running, and when G HUB is controlling a Logitech mouse (with its On-board memory mode off), changes made in hrtz wait until it lets go.

### Improved
- Backups, restores and factory settings show their progress where you started them.
- The Lab lists only the tests your mouse can run, and offers a debounce fix only on a mouse that has the setting.

## 0.2.2 — 2026-10-17

### Fixed
- Switching profiles by app no longer lags or piles up when you alt-tab quickly, and the pop-up shows once per switch. A profile switches once an app has been in front for a moment, and only once.

## 0.2.1 — 2026-10-17

### New
- Settings has a Help section: report a problem with your hrtz version, mouse and Windows filled in, or send a short report on how an untested mouse works.

## 0.2.0 — 2026-10-17

### New
- Sensor settings: DPI stages, polling rate up to 8,000 Hz, lift-off distance, motion sync, angle snapping, ripple control, click debounce and more.
- Buttons: set any button to a click, DPI, scroll, a media key, a key combination, rapid fire or the polling switch.
- Profiles: rename the four profiles and switch them automatically per app.
- Battery: level history, time left, life at each polling rate, and health.
- Battery saver lowers the polling rate on low battery until you charge.
- Sleep after: how long the mouse stays awake when idle (Z1 PRO MAX).
- In-game HUD: battery, polling rate, DPI and profile on screen, without touching the game.
- Lab: full-screen checks for connection, tracking and lift-off, plus DPI accuracy, double clicks, the scroll wheel and a mousepad map.
- Backups in Settings, saved automatically before changes, with undoable restores, export and import.
- Pair a mouse with its receiver from Settings.
- Tray icon with the battery level, low battery alerts and profile switching.
- A pop-up when DPI or the profile changes.
- Updates download in the background and install when you choose Restart to update, in the sidebar or the tray menu.
