# Changelog

Newest first. Each version lists what's **New**, **Improved** and **Fixed**. The app's "What's new" window reads this file, so write for the people using it.

Notes for the next version go under `## Unreleased`. Releasing it gives that heading the version and the day's date: `## 0.2.0 — 2026-11-02`.

## 0.5.0 — 2026-11-14

### New
- The Zaopin Z11, not tested yet, with Zaopin's own picture: its DPI stages (up to 12,000), polling rate, sensor mode, debounce, angle snapping, ripple control, buttons and onboard profiles. Have one? Its Not tested badge opens a 2-minute check.
- 159 more ATK, VXE and MAD mice, not tested yet: every model in ATK's own app, among them VXE's R1 line, ATK's Dragonfly F1, X1, A9, U2, Y9 and F1, and the MAD R. Their polling rate (up to 8K on an 8K or NearLink receiver), debounce, motion sync, angle snapping, ripple control and the four onboard profiles; on most, the DPI stages and the DPI light; and lift-off where it's 1 or 2 mm (0.7 mm on a PAW3950). On models with a PAW3311, PAW3315 or CORE26K sensor, the DPI follows once an owner's check confirms how they keep it. Have one? Its Not tested badge opens a 2-minute check.
- VXE's R1 mice show ATK's own picture of the R1. On VXE's own receivers and cables (the R1, R1 PRO and R1 PRO MAX among them), and on the ATK X1 Pro Max and A9 Plus NK, hrtz also sets the sleep timer, from 30 seconds to 30 minutes.

### Fixed
- On a Zaopin model hrtz doesn't know yet, the DPI could show wrong (a Z11 set to 1,350 showed 1,600). Until the model is added, hrtz leaves its DPI, motion sync, angle snapping, ripple control and sensor mode as they are.
- When a receiver doesn't say which mouse it has, hrtz asks again once the mouse wakes.
- The mouse check no longer marks the DPI button as failed when only one DPI stage is on.
- A report for a mouse hrtz doesn't run now says which model its receiver named, instead of 0, 0.
- Switching to a mouse that doesn't have the page you're on (Buttons, on an ATK) no longer leaves the window blank: hrtz moves to Sensor.

## 0.4.1 — 2026-11-12

### New
- The ATK U2 Plus (PAW3395), not tested yet: its polling rate, DPI stages and DPI light, lift-off distance, debounce, motion sync, angle snapping, ripple control and its four onboard profiles, by its receiver or its cable. Have one? Help test it from its Not tested badge.

### Improved
- Inspect a mouse is now the one place to help with a mouse: pick it and run its check. hrtz gathers everything it safely can (what the mouse tells Windows, its settings, its polling rate and its buttons) into one report, then opens the right GitHub form. The Not tested badge and Settings open it at your mouse.
- A Zaopin model hrtz doesn't know yet shows as Not supported yet, with a badge that opens its check. Until it's added, hrtz keeps its DPI to 12,000 and leaves its sensor mode alone, since its sensor may differ.
- A mouse without a picture now shows the hrtz mark instead of a drawn mouse.

## 0.4.0 — 2026-11-09

### New
- SignalRGB can now set the DPI light on Zaopin mice, along with the rest of your lighting. Turn it on in Sensor → Light.
- More tray icons to choose from, under HUD & Tray.
- Inspect a mouse, in the title bar: request support for a mouse that isn't supported yet.

### Improved
- Hover the tray icon to see every connected mouse, with its DPI and profile.

## 0.3.1 — 2026-11-08

### New
- The DPI light on Zaopin mice, which shows the colour of the stage in use: turn it off, steady or breathing, and set its brightness or breathing speed, on the Sensor page.

### Improved
- Mousepad calibration: go back a step with Back or Backspace to mark it again. When the result doesn't add up, only the steps that need it are asked again, not the whole calibration.

### Fixed
- Mousepad calibration's progress marks fill in as you go.
- Undo on the Buttons page puts back what a button did before, after a change or Restore default buttons.
- Start with Windows starts the installed hrtz, even where another copy had set it up before.
- Start with Windows is on in the first-run guide, as intended. On a new PC it showed off.
- Another maker's mouse is no longer taken for a Zaopin. Some VGN, Lamzu, Pulsar and ATK mice share Zaopin's USB ID, and hrtz now leaves them alone.

## 0.3.0 — 2026-10-19

### New
- Several mice: the mouse card opens a list of every mouse hrtz knows, to switch between them, and Connect a new mouse.
- Setting up a mouse is a full-page guide, on first run and for each new mouse, with its first backup.
- Each mouse keeps its own profile names, app rules, battery saver and battery history.
- A mouse that isn't connected shows its last settings.
- Remove a mouse you no longer use, from its ⋯ in the mouse list or from Settings, while it's unplugged, with Undo.
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
