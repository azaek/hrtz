import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Lod = "0.7mm" | "1mm" | "2mm";
export type SensorMode = "lp" | "hp" | "corded";
export type MouseButton = "left" | "right" | "middle" | "back" | "forward";

export type ButtonAction =
  | { kind: "disabled" }
  | { kind: "mouse"; button: MouseButton }
  | { kind: "dpi"; action: "cycle" | "up" | "down" | "default" }
  | { kind: "scroll"; direction: "up" | "down" | "left" | "right" }
  | { kind: "fire"; intervalMs: number; times: number }
  | { kind: "pollingSwitch" }
  | { kind: "dpiLock"; dpi: number }
  | { kind: "media"; usage: number }
  | { kind: "keys"; modifiers: number; keys: number[] }
  | { kind: "macro"; slot: number; repeat: number }
  | { kind: "raw"; code: number; p1: number; p2: number }
  /** Switch the mouse's onboard profile (a Logitech's). */
  | { kind: "profile"; action: "next" | "previous" | "cycle" }
  /** Set in G HUB in a form hrtz doesn't edit yet (a macro, G-Shift, DPI Shift): its record, kept. */
  | { kind: "vendor"; bytes: number[] };

export interface DpiStage {
  dpi: number;
  color: string;
}

/** What the mouse holds now. A setting this mouse doesn't have is missing, and its card isn't shown.
 *  Sent back whole to change something; a missing setting stays as the mouse has it. */
export interface Settings {
  pollingHz?: number;
  /** How many stages the DPI button cycles through. */
  dpiStageCount?: number;
  /** 0-based index of the stage in use. */
  dpiStage?: number;
  /** Every stage slot the mouse has, used or not. */
  stages?: DpiStage[];
  lod?: Lod;
  sensorMode?: SensorMode;
  debounceMs?: number;
  motionSync?: boolean;
  angleSnapping?: boolean;
  rippleControl?: boolean;
  peakTimeoutEnabled?: boolean;
  peakTimeoutS?: number;
  /** Seconds without movement before the mouse sleeps (flash 0xBB). */
  sleepS?: number;
  /** What each button does, by the mouse's own button index. */
  buttons?: ButtonAction[];
}

/** What the mouse can do, with its real limits. A feature it lacks is missing (or false), so its
 *  card is left out, and app features that need it step aside for this mouse. */
export interface Capabilities {
  /** Connected by USB cable rather than a receiver. */
  wired: boolean;
  dpi?: {
    min: number;
    max: number;
    step: number;
    /** Stage slots; the DPI button cycles through the first `dpiStageCount`. */
    maxStages: number;
    /** Each stage has a colour for the mouse's light. */
    colors: boolean;
  };
  /** The rates this connection carries, lowest first. The mouse may hold a higher one, set for its
   *  other connection. */
  polling?: { rates: number[] };
  /** Lift-off distances offered, lowest first. */
  lod?: Lod[];
  sensorMode?: {
    modes: SensorMode[];
    /** The mouse runs Corded at this rate and above, whatever is set. */
    cordedFromHz?: number;
    /** On its cable, the mouse always runs Corded. */
    cordedWhenWired: boolean;
  };
  debounceMs?: { min: number; max: number };
  motionSync: boolean;
  angleSnapping: boolean;
  rippleControl: boolean;
  peakTimeoutsS?: number[];
  /** "Sleep after" choices: only where the sleep timer has been measured on the model. */
  sleepTimesS?: number[];
  /** The buttons that physically exist: the mouse's index for each, and its name. */
  buttons?: [number, string][];
  /** How many onboard profiles. */
  profiles?: number;
  battery: boolean;
  longRange: boolean;
  pairing: boolean;
  factoryReset: boolean;
  /** Its whole memory can be read and written back, for backups. */
  backups: boolean;
  /** It has a button that cycles the DPI stages out of the box (every Zaopin; not the Superlights). */
  dpiButton?: boolean;
  /** The maker's own figure for a charge: [Hz, hours of use], until hrtz has measured its own. */
  ratedHours?: [number, number][];
}

/** The highest polling rate this connection carries. */
export function topRate(caps: Capabilities | null | undefined): number | undefined {
  const rates = caps?.polling?.rates;
  return rates?.[rates.length - 1];
}

/** The DPI of the stage in use. */
export const currentDpi = (s: Settings | null | undefined): number | undefined =>
  s?.dpiStage === undefined ? undefined : s.stages?.[s.dpiStage]?.dpi;

export interface Battery {
  /** Smoothed estimate shown in the UI. */
  level: number;
  /** The mouse's own figure, which jumps while charging. */
  rawLevel: number;
  charging: boolean;
  millivolts: number;
}

/** Where the receiver (or the mouse, on its cable) is plugged in. */
export interface UsbPort {
  /** Windows' location path; the same port always reads the same. */
  path: string;
  /** Hubs between it and the PC's own port: 0 when it's plugged straight into the PC. */
  hubs: number;
}

export interface DeviceState {
  /** Goes up with every change; keep the copy with the highest. */
  rev: number;
  /** Who the mouse is, the same across connections and restarts. Set once it's met. */
  key?: string | null;
  connected: boolean;
  online: boolean;
  model: string | null;
  cid: number;
  mid: number;
  deviceType: number;
  /** Which of its model's looks it is, where the mouse says: a Logitech's colour, for its picture. */
  variant?: number | null;
  firmware: string | null;
  battery: Battery | null;
  profile: number | null;
  /** The names saved on the mouse, by profile, where it keeps them (a Logitech's). hrtz's names
   *  (`MousePrefs.profileNames`) take them, and go onto the mouse when renamed here. */
  profileNames?: (string | null)[] | null;
  longRange: boolean | null;
  /** Software drives the mouse live instead of its own memory: a Logitech in host mode, G HUB's
   *  doing with its On-board memory mode off. What's set here applies once it lets go. */
  hostMode?: boolean;
  settings: Settings | null;
  capabilities: Capabilities | null;
  port: UsbPort | null;
}

/** What a profile held the last time it was in use: the mouse only reports the one in use. */
export interface ProfileSnapshot {
  /** The model it was read from. */
  mid: number;
  /** DPI of the stage in use, and how many stages the DPI button cycles. */
  dpi: number;
  stages: number;
  pollingHz: number;
  /** What each button that exists does, by flash index. */
  buttons: ButtonAction[];
}

/** A game installed through a launcher, with the exe that runs it. */
export interface InstalledGame {
  name: string;
  exe: string;
  path: string;
  source: "Steam" | "Epic" | "Riot";
}

export interface AppRule {
  exe: string;
  profile: number;
  name?: string;
  path?: string;
}

/** A game the pad map records in. */
export interface PadGame {
  exe: string;
  name: string;
  path: string;
}

export interface Prefs {
  batteryAlerts: boolean;
  lowBatteryPercent: number;
  osd: boolean;
  trayStyle: "percent" | "battery";
  /** Track where the mouse moves on the pad, in the background too. */
  padTracking: boolean;
  /** Count double clicks over days, in the background too. */
  clickHistory: boolean;
  /** The games the pad map records in; nothing else is recorded. Known games join on their own. */
  padGames: PadGame[];
  /** Known games taken off the list, so they don't join again (exe names in lower case). */
  padGamesRemoved: string[];
  /** The first-run window has been through. */
  welcomed: boolean;
  /** Agreed to share anonymous data that tunes the algorithms. Opt-in; nothing is sent yet. */
  shareData: boolean;
  /** The in-game HUD. */
  hud: HudPrefs;
}

/** What hrtz keeps for one mouse: its profiles' names and what they held, the apps that switch
 *  them, and its battery saver. */
export interface MousePrefs {
  rulesEnabled: boolean;
  /** Profile to return to when no rule matches; null leaves the profile alone. */
  defaultProfile: number | null;
  rules: AppRule[];
  profileNames: string[];
  /** What each profile held when last in use. */
  profileSnapshots: (ProfileSnapshot | null)[];
  /** Lower the polling rate to `batterySaverHz` at `batterySaverBelow` percent, until charging. */
  batterySaver: boolean;
  batterySaverBelow: number;
  batterySaverHz: number;
  /** The rate to go back to, while the saver has it lowered. */
  batterySaverRestore: number | null;
}

/** A mouse hrtz has met, and what it keeps for it. */
export interface KnownMouse {
  /** Stays the same across connections and restarts (`DeviceState.key`). */
  key: string;
  model: string;
  /** When it was first and last connected, in seconds since 1970. */
  firstSeen: number;
  lastSeen: number;
  prefs: MousePrefs;
  /** What it held the last time it was connected: what its pages show while it isn't. */
  last?: DeviceState | null;
}

/** A mouse hrtz can run, as its family's table lists it. */
export interface Supported {
  family: string;
  brand: string;
  model: string;
  /** The key a connected one gets, for its picture (`compx/7-4`). */
  key: string;
  /** Tried on the real mouse. */
  tested: boolean;
}

export type HudShow = "off" | "change" | "always";
export type HudAnchor = "top-left" | "top" | "top-right" | "left" | "right" | "bottom-left" | "bottom" | "bottom-right";
/** How big the tabs are: 0.85, 1 or 1.2 times, on top of the display's scaling. */
export type HudSize = "small" | "medium" | "large";

/** A game the HUD can't draw over, found by the backend while it was in front. */
export interface HudBlocked {
  exe: string;
  name: string;
  reason: "exclusive";
}

/** The in-game HUD's settings. The backend decides what shows when; the UI only edits these. */
export interface HudPrefs {
  show: HudShow;
  /** Which tabs to show. Time left goes in the battery tab. hrtz's mark always leads them, with a
   *  yellow dot while the mouse is asleep or out of range. Without labels each value says what it
   *  is by itself: "82%", "8K Hz", "800 DPI". */
  fields: { battery: boolean; timeLeft: boolean; polling: boolean; dpi: boolean; profile: boolean; labels: boolean };
  anchor: HudAnchor;
  size: HudSize;
  /** How dark the tabs are, 0 to 100: black at plate × 0.69 / 100 alpha. Below 30 there's no tab
   *  and the text is outlined. */
  plate: number;
  /** Teal is Valorant's stat colour (#62e3e8). */
  valueColor: "teal" | "white";
  /** In "always": dim when nothing changes, and move a pixel or two now and then. */
  oledCare: boolean;
  /** Exe names in lower case that the HUD stays off in. */
  hiddenGames: string[];
  /** Written by the backend. */
  blocked: HudBlocked[];
}

/** One row of the battery log. */
export interface BatteryEntry {
  /** Unix seconds. */
  t: number;
  level: number;
  mv: number;
  charging: boolean;
  hz: number;
  /** False while the mouse sleeps. */
  awake: boolean;
}

export interface RateLife {
  hz: number;
  /** Hours of use from full to empty. */
  hours: number;
  /** Hours of use it was measured over. */
  measuredHours: number;
}

/** What the battery log says about battery life and health. */
export interface BatterySummary {
  since: number | null;
  life: RateLife[];
  /** Percent lost per day asleep. */
  standbyPerDay: number | null;
  /** Hours a day the mouse is awake, over the last two weeks. */
  awakePerDay: number | null;
  usualHz: number | null;
  daysPerCharge: number | null;
  /** Full discharges' worth of use since the log starts. */
  cycles: number;
  charges: number;
  /** Hours per charge lately as a share of the first two weeks', at the same rate. */
  capacity: number | null;
}

/** Why hrtz saved a backup by itself; backups you save have none. A restore's names the backup
 *  restored, as it was then. */
export type AutoReason = { kind: "first" } | { kind: "restore"; note: string; created: string } | { kind: "factory" };

export interface BackupMeta {
  id: string;
  created: string;
  /** The mouse it's from; none for a backup from before several mice (a Zaopin's). */
  key: string | null;
  model: string;
  firmware: string;
  profile: number | null;
  note: string;
  sha256: string;
  auto: AutoReason | null;
}

/** What a restore did: chunks written (0 when the mouse already matched it), and the backup of
 *  the mouse as it was just before, which Undo puts back. */
export interface Restored {
  written: number;
  saved: string | null;
}

export interface AppStatus {
  foregroundExe: string | null;
  /** The mouse in your hand (its key): the tray, the HUD and the pop-ups follow it. */
  inUse?: string | null;
  /** The families whose maker's app is running: `compx` (Zaopin's app), `hidpp` (G HUB). */
  vendorApps: string[];
  /** The HUD window is visible now. */
  hudShowing: boolean;
}

export interface ButtonEvent {
  /** 0 left, 1 right, 2 middle, 3 back, 4 forward. */
  button: number;
  down: boolean;
  /** Microseconds since the listener started. */
  tUs: number;
  /** On release: sensor counts moved while the button was held. */
  dx: number;
  dy: number;
}

/** One wheel report: 120 a notch, positive away from you (scrolling up) or to the right. */
export interface WheelEvent {
  delta: number;
  horizontal: boolean;
  /** Microseconds since the listener started. */
  tUs: number;
}

/** Windows' own pointer settings. */
export interface PointerSettings {
  /** "Enhance pointer precision". */
  acceleration: boolean;
  /** 1 to 20; 10 is 6 of 11 on the Settings slider. */
  speed: number;
  /** Raw Accel's driver is in the mouse stack. */
  rawAccel: boolean;
}

/** How the gaps in a connection run fall: too few to say, at a steady rhythm (the mouse's own
 *  timing), in bursts (interference on the radio), or at random. */
export type GapPattern = "few" | "regular" | "bursts" | "scattered";

/** A connection test run so far. Only movement fast enough to fill every poll counts. */
export interface ConnStats {
  /** Milliseconds of movement that counted. */
  ms: number;
  /** Reports that came in that time, and polls that came without one. */
  reports: number;
  missed: number;
  /** Gaps by polls missed: 1, 2, 3, 4–7, 8–15, 16 or more. */
  gaps: [number, number, number, number, number, number];
  /** Gaps of 2 ms or more. */
  stutters: number;
  /** The longest gap in microseconds, or 0 without one. */
  longestUs: number;
  pattern: GapPattern;
  /** For a steady rhythm: how often a gap comes, in milliseconds. */
  everyMs: number | null;
  /** The same for gaps of 4 polls or more, which the many short gaps of a radio merging reports
   *  would otherwise hide. */
  longPattern: GapPattern;
  longEveryMs: number | null;
}

/** One stroke, from a stop or a turn to the next. Distances are in sensor counts. */
export interface Stroke {
  /** Microseconds since the listener started. */
  startUs: number;
  endUs: number;
  dx: number;
  dy: number;
  /** The farthest it got from where it started. */
  reach: number;
  /** All its movement, whatever the direction. */
  path: number;
  /** Movement against the stroke's own direction: the hand settling, or the sensor tracking for a
   *  moment as the mouse rises or lands. */
  back: number;
  /** The fastest it went, in counts a second. */
  peak: number;
  /** Reports pointing back while going over 1 m/s: the sensor losing track. */
  flips: number;
  /** Gaps of 2 ms or more while moving fast enough to fill every poll. */
  dropouts: number;
  longestDropoutUs: number;
  /** Stops mid-stroke after which it carried on the same way at much the same speed: the sensor
   *  losing the surface for a moment. */
  stalls: number;
  /** It stopped (or left the surface) for 150 ms or more, turned back, or went on too long. */
  end: "pause" | "turn" | "cut";
}

/** What game mode's screen shows, sent by the page. */
export interface CaptureScreen {
  /** What to do, big, and a line under it. */
  title: string;
  detail: string;
  /** The figure in the middle ("7,946") and what it is ("reports a second"). */
  figure: string;
  figureLabel: string;
  /** How far along, 0 to 1, and a line under the bar. */
  progress: number;
  progressLabel: string;
}

/** Why game mode's screen closed: the page closed it, Esc, or another window came in front. */
export type CaptureClosed = "done" | "esc" | "away";

/** One 100 ms tick of raw input from the mouse. */
export interface MeterSample {
  /** Reports per second while moving, or null when the mouse was still. */
  hz: number | null;
  /** |dx| + |dy| sensor counts per second over the same time. */
  countsPerS: number | null;
  /** Signed sensor counts moved during the tick. */
  dx: number;
  dy: number;
  /** Movement reports during the tick, and the counts they carried (|dx| + |dy|). */
  reports: number;
  travel: number;
  /** Gaps of 2 ms or more in movement fast enough to fill every poll, found during the tick. */
  stutters: number;
  /** The connection run so far, since the stream started or `meterReset`. */
  conn: ConnStats;
  /** Strokes that ended during the tick. */
  strokes: Stroke[];
  buttons: ButtonEvent[];
  wheel: WheelEvent[];
  /** hrtz's window is in front. Behind others, Windows hands it reports about 125 times a second,
   *  merged, so the rate, the connection run and strokes wait; clicks, the wheel and distances
   *  still count. */
  foreground: boolean;
  /** The pointer is held still, as a game holds it, while a timing test counts fast movement. */
  held: boolean;
}

export interface PadCalibration {
  /** Rotation from the sensor's axes to the pad's, radians. */
  angle: number;
  /** Usable cm from the origin to each edge. */
  left: number;
  right: number;
  top: number;
  bottom: number;
  /** 2: the origin is the user's own resting spot. Older calibrations measured from the centre. */
  version: number;
}

/** An app the mouse was used in, kept with its own map. */
export interface PadApp {
  /** Exe name in lower case. */
  exe: string;
  name: string;
  path: string;
  movingMs: number;
}

/** A game's mouse sensitivity, read from its own settings file. */
export interface GameSensitivity {
  game: string;
  sens: number;
  /** Degrees turned per count at sensitivity 1. */
  yaw: number;
  /** Acceleration is on in the game, so a turn's length depends on speed. */
  acceleration: boolean;
  file: string;
}

/** Mousepad record. Positions are cm from the resting spot, x right and y down. */
/** One button's day in the click log. */
export interface ClickButtonDay {
  presses: number;
  /** Presses that came too soon after a release: the switch bouncing. */
  doubles: number;
  /** The longest gap of a double (ms): what click debounce would have to cover. */
  worstMs: number;
}

/** A day in the click log: left, right, middle, back, forward. */
export interface ClickDay {
  /** Local date, "2026-10-17". */
  date: string;
  buttons: ClickButtonDay[];
}

export interface PadView {
  calibration: PadCalibration | null;
  /** The app shown (exe in lower case), or null for every app together. */
  app: string | null;
  /** Apps kept with their own map, most used first. */
  apps: PadApp[];
  /** Learned put-down spot, and its spread as covariance xx, xy, yy (cm²). */
  home: [number, number];
  homeSpread: [number, number, number];
  cols: number;
  rows: number;
  /** cm of movement per 1 cm cell, row by row from the top-left corner. */
  cells: number[];
  /** Expected edge hits per 1 cm along each edge. */
  edgeTop: number[];
  edgeBottom: number[];
  edgeLeft: number[];
  edgeRight: number[];
  /** Expected number of lifts, including pauses still being decided. */
  lifts: number;
  /** Swipes by width and height, 1 cm bins. */
  reachX: number[];
  reachY: number[];
  movingMs: number;
  distanceCm: number;
  /** Live estimate, and how unsure it is (cm). */
  pos: [number, number];
  spread: number;
  /** Typical correction the edges have made (cm), once there are a few. */
  accuracy: number | null;
  /** Grip angle in use: calibration plus what the edges have taught (degrees). */
  gripDeg: number;
  /** Pauses the lift model has learned from. */
  learned: number;
  tracking: boolean;
}

export interface RunningApp {
  exe: string;
  /** Product name from the exe's version info, or the window title. */
  name: string;
  title: string;
  path: string;
  /** PNG data URL of the app icon. */
  icon: string | null;
}

/** A number that only grows, across reloads too, for switches the page can flip in a burst: React
 *  mounts a page twice in development (on, off, on in a blink), and calls can arrive out of order.
 *  The backend drops one older than the last it applied, so the newest sent wins. */
let lastSeq = 0;
const seq = () => (lastSeq = Math.max(lastSeq + 1, Date.now() * 1000));

/** The mouse the window shows, by key: commands about a mouse go to it. Set by the store; null
 *  leaves the choice to the app (its primary mouse). */
let mouse: string | null = null;
export const showMouse = (key: string | null) => {
  mouse = key;
};

export const api = {
  state: () => invoke<DeviceState>("get_state"),
  /** Every connection's state. */
  states: () => invoke<DeviceState[]>("get_states"),
  applySettings: (settings: Settings) => invoke<Settings>("apply_settings", { mouse, settings }),
  setProfile: (index: number) => invoke<void>("set_profile", { mouse, index }),
  setLongRange: (on: boolean) => invoke<void>("set_long_range", { mouse, on }),
  factoryReset: () => invoke<void>("factory_reset", { mouse }),
  prefs: () => invoke<Prefs>("get_prefs"),
  setPrefs: (prefs: Prefs) => invoke<void>("set_prefs", { prefs }),
  mice: () => invoke<KnownMouse[]>("get_mice"),
  supportedMice: () => invoke<Supported[]>("supported_mice"),
  setMousePrefs: (key: string, prefs: MousePrefs) => invoke<void>("set_mouse_prefs", { key, prefs }),
  /** Remove a mouse that isn't connected: what's kept for it goes. Returns that, for Undo. */
  forgetMouse: (key: string) => invoke<KnownMouse>("forget_mouse", { key }),
  /** Put back a mouse just removed. */
  restoreMouse: (mouse: KnownMouse) => invoke<void>("restore_mouse", { mouse }),
  listBackups: () => invoke<BackupMeta[]>("list_backups"),
  createBackup: (note: string) => invoke<BackupMeta>("create_backup", { mouse, note }),
  /** Saves the mouse as it is first (an automatic backup), unless it already matches. */
  restoreBackup: (id: string) => invoke<Restored>("restore_backup", { mouse, id }),
  /** Puts back what a restore replaced, from the backup it saved, and drops that backup. */
  undoRestore: (id: string) => invoke<number>("undo_restore", { mouse, id }),
  /** A named automatic backup becomes yours. */
  renameBackup: (id: string, note: string) => invoke<BackupMeta>("rename_backup", { id, note }),
  deleteBackup: (id: string) => invoke<void>("delete_backup", { id }),
  exportBackup: (id: string, path: string) => invoke<void>("export_backup", { id, path }),
  importBackup: (path: string) => invoke<BackupMeta>("import_backup", { path }),
  backupSettings: (id: string) => invoke<Settings>("backup_settings", { id }),
  openBackupsFolder: () => invoke<void>("open_backups_folder"),
  /** Windows' name and version, for a problem report: "Windows 11 24H2 (26100)". */
  windowsVersion: () => invoke<string>("windows_version"),
  /** A downloaded update: a notification, and "Restart to update" in the tray menu. */
  updateReady: (version: string) => invoke<void>("update_ready", { version }),
  /** Saves what Quit would, before the updater exits hrtz. */
  prepareUpdate: () => invoke<void>("prepare_update"),
  runningApps: () => invoke<RunningApp[]>("running_apps"),
  appIcon: (path: string) => invoke<string | null>("app_icon", { path }),
  /** Games installed through Steam, Epic and Riot. */
  installedGames: () => invoke<InstalledGame[]>("installed_games"),
  /** Name and icon of an exe picked from disk; null if it isn't an exe. */
  appInfo: (path: string) => invoke<RunningApp | null>("app_info", { path }),
  trayPreviews: () => invoke<{ percent?: string; battery?: string; taskbar: "light" | "dark" }>("tray_previews"),
  appStatus: () => invoke<AppStatus>("app_status"),
  /** Raw input for the Lab page; `rate` is the polling rate stutters are measured against. */
  meter: (on: boolean, rate?: number) => invoke<void>("meter", { on, rate, seq: seq() }),
  /** Start a new connection test run, and forget the stroke in progress. */
  meterReset: () => invoke<void>("meter_reset"),
  /** A timing test is open: while it is, its timing counts only in game mode, with the pointer
   *  held. Send `false` when it closes. */
  meterHold: (on: boolean) => invoke<void>("meter_hold", { on, seq: seq() }),
  /** Game mode: a screen of hrtz's own takes the pointer as a game does, showing `screen`. Over
   *  hrtz's page Windows hands hrtz about 150 reports a second; here it gets every one. */
  captureOpen: (screen: CaptureScreen) => invoke<void>("capture_open", { screen }),
  captureUpdate: (screen: CaptureScreen) => invoke<void>("capture_update", { screen }),
  captureClose: () => invoke<void>("capture_close"),
  /** The pad record for `app` (exe in lower case), or for every app together. */
  padState: (app?: string | null) => invoke<PadView | null>("pad_state", { app: app ?? null }),
  /** Put the receiver into pairing for up to 30 s (`on.pairing` counts down). True when a mouse
   *  paired; it then connects again by itself. */
  pairReceiver: () => invoke<boolean>("pair_receiver", { mouse }),
  /** Double clicks counted over the last 30 days, oldest first. */
  clickHistory: () => invoke<ClickDay[]>("click_history"),
  gameSensitivity: (exe: string) => invoke<GameSensitivity | null>("game_sensitivity", { exe }),
  batterySummary: () => invoke<BatterySummary>("battery_summary", { mouse }),
  pointerSettings: () => invoke<PointerSettings | null>("pointer_settings"),
  /** Show one of the app's pop-ups as it really appears, for checking how it looks. */
  testPopup: (kind: "dpi" | "polling" | "profile" | "low-battery" | "battery-saver") => invoke<void>("test_popup", { kind }),
  /** Turn off Windows' pointer acceleration and/or set the pointer speed to 6 of 11. */
  fixPointer: (acceleration: boolean, speed: boolean) => invoke<PointerSettings | null>("fix_pointer", { acceleration, speed }),
  batteryHistory: (days: number) => invoke<BatteryEntry[]>("battery_history", { mouse, days }),
  padCalibrate: (calibration: PadCalibration) => invoke<void>("pad_calibrate", { calibration }),
  padClear: () => invoke<void>("pad_clear"),
  /** The HUD page's preview: the real HUD on screen, every chosen tab showing (sample values with
   *  no receiver). Send `true` again at least every 5 minutes to keep it up (it ends by itself
   *  otherwise), and `false` to end it. It does nothing while Show is off or the main window is
   *  hidden or minimised, and gives way to a game in front. */
  hudTest: (on: boolean) => invoke<void>("hud_test", { on }),
};

export const on = {
  state: (f: (s: DeviceState) => void): Promise<UnlistenFn> => listen<DeviceState>("device-state", (e) => f(e.payload)),
  progress: (f: (p: [number, number]) => void) => listen<[number, number]>("device-progress", (e) => f(e.payload)),
  /** Seconds the receiver keeps looking for a mouse to pair with. */
  pairing: (f: (secondsLeft: number) => void) => listen<number>("pairing", (e) => f(e.payload)),
  prefs: (f: (p: Prefs) => void) => listen<Prefs>("prefs", (e) => f(e.payload)),
  mice: (f: (m: KnownMouse[]) => void) => listen<KnownMouse[]>("mice", (e) => f(e.payload)),
  /** A mouse hrtz never met connected (its key). */
  mouseNew: (f: (key: string) => void) => listen<string>("mouse-new", (e) => f(e.payload)),
  status: (f: (s: AppStatus) => void) => listen<AppStatus>("app-status", (e) => f(e.payload)),
  meter: (f: (s: MeterSample) => void) => listen<MeterSample>("meter", (e) => f(e.payload)),
  /** hrtz saved a backup by itself (meeting a mouse, factory settings). */
  backups: (f: () => void) => listen("backups", () => f()),
  /** The tray's "Restart to update". */
  updateInstall: (f: () => void) => listen("update-install", () => f()),
  /** Game mode's screen closed. */
  captureClosed: (f: (why: CaptureClosed) => void) => listen<CaptureClosed>("capture-closed", (e) => f(e.payload)),
};

export function message(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}
