# Changelog

Newest first. Each version lists what's **New**, **Improved** and **Fixed**. The app's "What's new" window reads this file, so write for the people using it.

Notes for the next version go under `## Unreleased`. Releasing it gives that heading the version and the day's date: `## 0.2.0 — 2026-11-02`.

## 0.5.10 — 2026-11-19

### New
- Logitech's G304 and G305, on their LIGHTSPEED receiver: their DPI stages, polling rate from 125 to 1,000 Hz, all six buttons, the onboard profile and the battery, with backups, restores and factory settings. Tested on a real G304, by @whyshhnuv in #26
- Logitech's PRO X SUPERLIGHT 2c, on its LIGHTSPEED receiver or its cable: DPI stages, polling rate, profiles and battery, as on the PRO X SUPERLIGHT 2. Not tested yet: its Not tested badge runs the check. Thanks to @bwyao for the report.

### Fixed
- Mouse button shortcuts are now recorded one key at a time, so Alt+Tab can be set without Windows switching away, by @whyshhnuv in #33

## 0.5.9 — 2026-11-18

### Improved
- The Zaopin Z2 is now tested. A check on a real one passed on its 4K receiver and its cable: its DPI stages, their colour and the DPI light, polling rate, lift-off distance, sensor mode, debounce, motion sync, angle snapping, ripple control, peak performance timeout and the onboard profiles, with each of its six buttons seen. Thanks to elist-exe for the check.

## 0.5.8 — 2026-11-18

### Improved
- The VXE R1 with a PAW3395 sensor is now tested. A check on a real one passed on its 1K receiver: its DPI stages, their colour and the DPI light, lift-off distance, sensor mode, debounce, motion sync, angle snapping, ripple control and the onboard profiles, with each of its six buttons seen. Thanks to krishiv1545 for the check.

### Fixed
- On a Zaopin's cable, the mouse check no longer leaves the polling rate changed. With 4,000 Hz set for its receiver, the check set 500 Hz over the cable and couldn't set 4,000 Hz back, since the cable goes up to 1,000 Hz, so every change after it failed too. The check now leaves the polling rate alone on a connection that can't hold the rate the mouse has set.
- On a Zaopin mouse, if anything is still changed at the end of the mouse check, hrtz now writes back its memory as it was before the check.

## 0.5.7 — 2026-11-17

### Improved
- A Zaopin Z2 that showed as "Zaopin mouse" is now known as the Z2, with its PAW3395's DPI up to 26,000, motion sync, lift-off and the sensor's other settings. Some Z2s answer with a newer model ID; this one was seen on a 4K receiver. Not tested yet: its Not tested badge runs the check. Thanks to derekjamesedu for the report.

## 0.5.6 — 2026-11-17

### New
- Your mouse's battery on the taskbar: a mouse and its level, like "93%", beside the tray icons, for the mouse the tray follows. Hover it for each mouse's battery, polling rate and DPI, or click it to open hrtz. It's on unless you turn it off under HUD & Tray. Windows 11.

### Improved
- hrtz's tray icon now shows on the taskbar instead of under the ^, unless you've already chosen where it goes.
- The VXE R1 PRO MAX with a PAW3395 sensor is now tested. A check on a real one passed on its 4K receiver: its DPI stages, their colour and the DPI light, the polling rate (4,000 Hz, and 500 reports a second counted at 500 Hz), sensor mode, debounce, motion sync, angle snapping, ripple control, the sleep timer and the onboard profiles, with each of its five buttons seen. Thanks to SAPNXTDOOR for the check.

## 0.5.5 — 2026-11-16

### New
- A blind test in Lab: can you feel 8,000 Hz, or motion sync? hrtz switches your mouse between the two without telling you which, you keep a crosshair on a moving dot full screen, then pick which run was which, and see after each round whether you were right. Ten rounds, about two minutes; your settings go back at the end. The result says how likely your score was by guessing, and whether you followed the dot more closely with either.
- Your week: a page with how far your mouse travelled, how long it moved, your fastest flick and your clicks, each day and in each game, with the full turns you made in Valorant, Apex or CS2. Counted as you use the mouse, totals only, kept on your PC; you can switch it off there.
- Sensitivity in your games, on the Sensor page: how far a full turn is in Valorant, Apex and CS2 at your DPI, and what to set in each so they all feel like the one you pick. hrtz reads each game's own settings.
- "New in hrtz" at the top of the Sensor page, a button away from each, until you hide it.

## 0.5.4 — 2026-11-15

### Fixed
- ATK's own app runs some of its newer mice a different way (the ATK F1 V3 ULTIMATE+ among them), and some of those share a model number with a mouse hrtz runs. hrtz now only takes an ATK mouse on a USB ID ATK's app lists for the mice hrtz runs, so it can't take one of those for another model. It leaves them alone and sends them nothing; Inspect a mouse still reports them.

## 0.5.3 — 2026-11-15

### New
- Buttons on ATK, VXE and MAD mice: set each to a mouse button, a DPI change, a wheel scroll or tilt, a key combination, a media key, rapid fire, the polling rate switch, or nothing, in each of the four profiles. Not tried on a real mouse yet: your mouse's check now asks for each button.
- Lift-off distance on mice with a PAW3950 Ultra or PAW3955 sensor, from 0.7 to 1.7 mm in steps of 0.1, as ATK's app has it. Not tried on a real mouse yet.
- Sensor mode on ATK, VXE and MAD mice: Low power or High performance, and Maximum on the models and firmware ATK's app offers it on (its "competitive MAX"), which runs the sensor at its limit and uses the most battery. Not tried on a real mouse yet.
- Sensor angle on ATK, VXE and MAD mice whose firmware has it: turn the sensor's direction up to 30° either way to match how you hold the mouse, or switch it off. Not tried on a real mouse yet.
- Long-range mode on ATK, VXE and MAD mice that have it (ATK's "Ultra Long Range Mode"), in Settings, as on a Zaopin. Not tried on a real mouse yet.
- How long a charge lasts on 53 ATK, VXE and MAD models, from ATK's own figures for the sensor mode in use, at 1K and 8K Hz, until hrtz has measured your mouse's own. Where ATK gives a range, the Battery page shows its low end.
- A sensor angle test in Lab, for any mouse hrtz runs: swipe side to side the way that feels level, and hrtz measures how far your grip tilts your swipes. On a mouse with a sensor angle, one click sets the angle that levels them; on any other, it says the tilt, which hrtz can't correct there.

### Improved
- The VXE R1 PRO with a PAW3950 Ultra sensor is now tested, the first of ATK's mice to be. Its owner's check passed on its 1K receiver and its cable: its DPI stages, their colours and the DPI light, polling rate, debounce, motion sync, angle snapping, ripple control, sleep timer, the DPI button and the onboard profiles.

### Fixed
- The ATK PHANTOM EXTREME, listed until now as the X EXTREME, is read as ATK's newest app reads it: its sensor is a PAW3955, which keeps its DPI stages elsewhere. The ATK X1 MAG no longer shows a DPI light, which it doesn't have.
- The sleep timer on ATK's and VXE's mice is now set in both places ATK's own app sets it. hrtz set one, which may not have changed when the mouse really sleeps.

## 0.5.2 — 2026-11-15

### Improved
- One check now says everything a model keeps. The mouse check used to change one DPI stage; it now changes each setting hrtz offers on your mouse, one at a time, and puts each back before the next: the DPI stages, their colours and light, the polling rate, debounce, motion sync and the rest, then a switch to another profile and back. While the polling rate is changed, it counts your mouse's reports again, to see that the rate really changed.
- The check ends by comparing your mouse's memory with how it read at the start, and the report says whether it's as it was.
- If your mouse doesn't keep a change, the report now holds what hrtz sent and what the mouse answered, so we can work it out without asking you to run it again.
- On an ATK, VXE or MAD mouse, the check asks you to press the DPI button, to see that hrtz follows it.

### Fixed
- On an ATK, VXE or MAD mouse, changing a setting could fail with "unsupported peak performance timeout", and the mouse check's change failed the same way. hrtz was holding these mice to a Zaopin setting they don't have. Found by the first check of a real VXE R1 PRO; nothing was written to the mouse.
- A change to an ATK, VXE or MAD mouse is read back a few times before hrtz says the mouse didn't keep it, in case the mouse takes a moment.
- The mouse check says what it does on your mouse: it no longer says it asks for each button on a mouse whose buttons hrtz doesn't set yet.
- A report's polling line shows whole reports a second (it read "990.051").

## 0.5.1 — 2026-11-14

### Improved
- Inspect a mouse learns more from a mouse hrtz doesn't run yet. One that speaks the same language as Zaopin's mice (some from ATK, VXE, VGN, Lamzu and Pulsar do) is now read when you start its check: which mouse it is, its firmware and its memory, the way its own app reads them. hrtz then measures its polling rate and shows what it read, so you can say whether that's what its own app shows.
- One check can be enough to add a mouse. On a mouse hrtz doesn't know, Inspect a mouse can try one change, if you let it: it sets the polling rate to another rate, counts the mouse's reports, and puts it back.
- An ATK, VXE or MAD mouse whose DPI hrtz doesn't set yet (one with a PAW3311, PAW3315 or CORE26K sensor) now ends its check with one question about your DPI stages. Your answer is what lets us turn its DPI on.
- A sleeping mouse no longer gives an empty report. Inspect a mouse says to move it, and its check starts once it answers.
- One report for one mouse. After a check, Inspect a mouse offers the mouse's other connection (its receiver, or its cable) and adds that check to the same report.
- A report now opens with what it comes to: everything we need, something to check again, or only what Windows knows.
- Sending a report is one click. The GitHub issue opens with the report already inside it, so there's no file to attach. You can still save the report as a file and send it any other way.

## 0.5.0 — 2026-11-14

### New
- The Zaopin Z11, not tested yet, with Zaopin's own picture: its DPI stages (up to 12,000), polling rate, sensor mode, debounce, angle snapping, ripple control, buttons and onboard profiles. Have one? Its Not tested badge opens a 2-minute check. Thanks to MayankJairaj for the reports.
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
