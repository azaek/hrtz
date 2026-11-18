import { getVersion } from "@tauri-apps/api/app";
import { api, type DeviceState, type Inspected, type MouseRead, type Picture, type Reading } from "@/api";
import { english, msg, num, screen, t, type Say, type Speech } from "@/i18n";
import { LINK_NAME, linkName } from "@/links";
import { familyOf, modelKeyOf, SUPPORT_TEXT, type Support } from "@/models";
import { mouseIssueUrl } from "@/report";
import { pack, PACKED } from "./pack";
import type { Said, WriteCheck } from "./changes";
import type { Checks } from "./useParticipate";

/** Whole: a rate counted comes with decimals ("990.051 reports a second"). */
const fmt = (n: number) => Math.round(n).toLocaleString("en-US");
const hex = (n: number) => n.toString(16).toUpperCase().padStart(4, "0");

export type { Say };

/**
 * How what a check found is said: on screen (`SCREEN`) in the language shown, and in the report we
 * read, its file and its GitHub issue, in English (`ENGLISH`), whatever the language shown. `t`
 * says a line, `n` writes a number whole and grouped ("1,600"), `d` writes it as it is ("0.5"),
 * and `change` says a change the check tried.
 */
export interface Voice {
  t: Say;
  n: (x: number) => string;
  d: (x: number) => string;
  change: (w: WriteCheck) => Said;
  /** What a button was set to. */
  does: (b: { does: string; shown?: () => string }) => string;
  /** For the helpers that name a setting (`lodName`, `actionLabel`). */
  speech: Speech;
}
export const SCREEN: Voice = { t: (key, values) => t(key, values), n: (x) => num(Math.round(x)), d: (x) => num(x), change: (w) => w.shown?.() ?? w, does: (b) => b.shown?.() ?? b.does, speech: screen };
export const ENGLISH: Voice = { t: english.t, n: fmt, d: String, change: (w) => w, does: (b) => b.does, speech: english };

/** "3554:F527", as Windows and the GitHub forms write a device's IDs; a simulated mouse has none. */
export const idsOf = (m: Inspected) => (m.vendorId || m.productId ? `${hex(m.vendorId)}:${hex(m.productId)}` : "Simulated");

/** What to call a listed mouse: the model hrtz runs it as, else its name in Windows (a receiver's
 *  rarely says which mouse it's for). A simulated one keeps its own, which says it's simulated. */
export const nameOf = (m: Inspected) => (m.vendorId || m.productId ? (m.supported ?? m.name) : m.name);

/** A listed mouse, the same each time the list is read again (its key comes and goes as it wakes). */
export const idOf = (m: Inspected) => `${idsOf(m)}-${m.name}`;

/** How far hrtz is with a listed mouse, in a word or two: beside its IDs, in the list and on its
 *  check (only on screen). `support` is where its model stands, for one hrtz runs. */
export const statusOf = (m: Inspected, support: Support | undefined) =>
  m.reach === "asleep" ? t("Mouse not answering") : support && m.reach === "runs" ? t(SUPPORT_TEXT[support]) : t("Not supported yet");

/** One thing a check found: fine, not, or only noted. Its text is in its `Voice`'s language. */
export type Line = { ok: boolean | null; text: string };

/** What a check comes to, said first: all there, something to look at again, or only a little. */
export interface Verdict {
  tone: "good" | "warn" | "plain";
  title: string;
  detail: string;
}

/**
 * What its owner said of what hrtz read from the mouse's memory. Memory says where a setting is
 * kept, not what its bytes mean: a Z11's stages read as other DPIs until its owner's report showed
 * its sensor keeps them by a table. So the owner is asked, once, in the check.
 */
export interface Confirmed {
  /** Which reading of the DPI stages is theirs: a piece's name, "none" of them, or "unsure". Null
   *  where hrtz had no reading of them to ask about. */
  stages: string | null;
  /** What its own app shows, where none of hrtz's readings was right. */
  stagesAre: string;
  /** Whether the other settings are as its own app shows them. */
  rest: "right" | "wrong" | "unsure";
  /** What's different, in its owner's words. */
  note: string;
  /** What hrtz can't read off the mouse, asked where its maker's own list doesn't name the model. */
  topDpi: string;
  sensor: string;
}

/** A change tried: the polling rate set to another, measured, and put back. */
export interface Tried {
  what: string;
  from: number;
  to: number;
  /** The mouse held the new rate when read back. */
  kept: boolean;
  /** Reports a second counted while it was set; null if the mouse wasn't moved. */
  measured: number | null;
  restored: boolean;
  error?: string;
}

/** A polling rate measured: reports a second (null if the mouse wasn't moved), and stutters. */
export interface Measured {
  hz: number | null;
  stutters: number;
}

/**
 * One connection's part of a report: a mouse as it was checked on its cable or through its
 * receiver. `how` is how far hrtz got: its full check (it runs the mouse), a reading (it leaves
 * the mouse alone but could read it), or only what Windows knows. What it comes to (`verdictOf`)
 * and what it found (`linesOf`) are worked out from what's here, so they follow what its owner
 * confirms and a change tried after.
 */
export interface Part {
  /** The listed mouse it's of (`idOf`): checked again, it takes this one's place. */
  id: string;
  /** The model hrtz knows it as, else as Windows lists it. */
  name: string;
  ids: string;
  how: "check" | "read" | "windows";
  /** hrtz knows the model, so its check tried a change: the report is a test of it. */
  known: boolean;
  /** For its picture, and to run a change on one hrtz runs. */
  key: string | null;
  variant?: number | null;
  /** "USB cable", "8K receiver"; none where hrtz can't tell. */
  link: string | null;
  wired: boolean | null;
  /** A name for the mouse hrtz can offer: the model it answered as, where hrtz knows that one. */
  called: string | null;
  /** Its maker's own list names the model, so what it can do is known. */
  listed: boolean;
  /** hrtz knows the model but not how its sensor keeps its DPI, so doesn't offer it (an ATK with
   *  a PAW3311, PAW3315 or CORE26K): its owner is asked which reading of its stages is theirs. */
  dpiOpen: boolean;
  /** The connection hrtz read it on, to try a change there. */
  connection: string | null;
  /** hrtz's reading of it: one it leaves alone, or what its receiver said while it slept. */
  read: MouseRead | null;
  /** hrtz's check of it, for one it runs. */
  checks: Checks | null;
  /** Its memory, a piece at a time. */
  picture: Picture | null;
  /** Its polling rate measured, for one hrtz reads (one it runs has it in `checks`). */
  measured: Measured | null;
  /** hrtz could have read it, but its mouse never answered. */
  silent: boolean;
  confirmed: Confirmed | null;
  tried: Tried | null;
  /** What else is saved for it. */
  data: Record<string, unknown>;
}

const blank = {
  known: false,
  key: null,
  link: null,
  wired: null,
  called: null,
  listed: false,
  dpiOpen: false,
  connection: null,
  read: null,
  checks: null,
  picture: null,
  measured: null,
  silent: false,
  confirmed: null,
  tried: null,
} satisfies Partial<Part>;

/** The rates a mouse on this channel has. */
const RATES = [125, 250, 500, 1000, 2000, 4000, 8000];

/** The rate a count of reports a second is nearest: a radio that merges reports counts under its
 *  rate, and quick circles don't fill every poll. */
export const rateNear = (hz: number) => RATES.reduce((a, b) => (Math.abs(Math.log(b / hz)) < Math.abs(Math.log(a / hz)) ? b : a));

/** A setting's reading in a part's memory. */
export const readingOf = (p: Pick<Part, "picture">, id: string): Reading | null => p.picture?.settings.find((s) => s.id === id) ?? null;

/** The polling rate as memory has it, and as measured. */
export function pollingOf(p: Part): { set: number | null; measured: number | null } {
  if (p.checks) return { set: p.checks.polling?.set ?? null, measured: p.checks.polling?.measured ?? null };
  const set = readingOf(p, "polling")?.ways[0]?.value;
  return { set: typeof set === "number" ? set : null, measured: p.measured?.hz ?? null };
}

/** The measurement agrees with what memory says: hrtz reads this mouse's polling rate right. */
export function pollingAgrees(p: Part): boolean {
  const { set, measured } = pollingOf(p);
  return !!set && !!measured && rateNear(measured) === set;
}

/** All of its memory came. */
const whole = (p: Part) => (p.how === "check" ? !!p.checks?.read.ok : !!p.read && (p.read.memoryWanted ?? 0) > 0 && (p.read.memorySize ?? 0) >= (p.read.memoryWanted ?? 0));

/** The settings its owner is shown to confirm: the ones any maker's app on this channel has. The
 *  DPI stages are asked about by themselves. None for a model hrtz knows: it shows those itself. */
const SHOWN = ["polling", "lod", "debounce", "motionSync", "angleSnapping", "rippleControl", "sleep"];
export const shownOf = (p: Part): Reading[] => (p.known ? [] : (p.picture?.settings ?? []).filter((s) => SHOWN.includes(s.id) && s.ways.length > 0));

/** Whether its owner is asked about what hrtz read: a mouse whose model hrtz doesn't know, or
 *  whose DPI it doesn't offer yet, read whole, with something read to ask about. */
export const asks = (p: Part) =>
  (!p.known || p.dpiOpen) && p.how !== "windows" && whole(p) && ((readingOf(p, "stages")?.ways.length ?? 0) > 0 || shownOf(p).length > 0);

/** Whether a change can be tried on it: a model hrtz doesn't know (one it knows has its own),
 *  read whole, whose polling rate hrtz reads as measured, so what's written is known to be it. */
export const canTry = (p: Part) => !p.known && p.how !== "windows" && whole(p) && pollingAgrees(p) && (p.how === "check" ? !!p.key : !!p.connection);

/** The change tried held: kept, measured at the new rate, and put back. */
const triedHeld = (t: Tried) => !t.error && t.kept && t.restored && !!t.measured && rateNear(t.measured) === t.to;

/** The stages' reading its owner picked. */
const pickedStages = (p: Part) => readingOf(p, "stages")?.ways.find((w) => w.piece === p.confirmed?.stages) ?? null;

/** What the check of a mouse hrtz runs found, a line each. */
export function summaryLines(c: Checks, voice: Voice): Line[] {
  const { t, n, d, change, does } = voice;
  const lines: Line[] = [];
  lines.push(
    !c.read.ok
      ? { ok: false, text: t("Memory not read: {{error}}", { error: c.read.error ?? t("no answer") }) }
      : c.read.settingsOnly
        ? { ok: null, text: t("Settings read; hrtz doesn't read this model's memory whole yet") }
        : { ok: true, text: t("Settings and memory read ({{bytes}} bytes)", { bytes: n(c.read.bytes ?? 0) }) },
  );
  if (c.polling) {
    const { set, measured, stutters } = c.polling;
    const steady = stutters <= 2;
    lines.push({
      ok: measured !== null && steady,
      text:
        measured === null
          ? t("Polling not measured")
          : set
            ? steady
              ? t("{{rate}} reports a second at {{hz}} Hz, steady", { rate: n(measured), hz: n(set) })
              : t("{{rate}} reports a second at {{hz}} Hz, {{count}} stutters", { rate: n(measured), hz: n(set), count: stutters })
            : steady
              ? t("{{rate}} reports a second, steady", { rate: n(measured) })
              : t("{{rate}} reports a second, {{count}} stutters", { rate: n(measured), count: stutters }),
    });
  }
  const seeable = c.buttons.filter((b) => b.seen !== null);
  if (seeable.length) {
    const seen = seeable.filter((b) => b.seen);
    lines.push({ ok: seen.length === seeable.length, text: t("Buttons: {{seen}} of {{total}} seen", { seen: d(seen.length), total: d(seeable.length) }) });
    for (const b of seeable.filter((x) => !x.seen)) lines.push({ ok: false, text: t("{{button}}: no press seen (set to {{action}})", { button: b.name, action: does(b) }) });
  }
  for (const b of c.buttons.filter((x) => x.seen === null)) lines.push({ ok: null, text: b.why ? t("{{button}}: {{why}}", { button: b.name, why: t(b.why) }) : t("{{button}} is set to {{action}}, which hrtz can't see", { button: b.name, action: does(b) }) });
  // Every change tried; a report from before each setting was tried has the one.
  const writes = c.writes ?? (c.write ? [c.write] : []);
  const held = writes.filter(writeHeld);
  if (held.length === 1) {
    const { what, from, to } = change(held[0]);
    lines.push({ ok: true, text: t("{{what}} {{from}} → {{to}}: kept, put back", { what, from, to }) });
  } else if (held.length) lines.push({ ok: true, text: t("{{count}} settings changed, kept and put back: {{settings}}", { count: held.length, settings: held.map((w) => change(w).what).join(", ") }) });
  for (const w of held) {
    if (w.hz && w.measured) lines.push({ ok: true, text: t("Set to {{hz}} Hz, it counted {{rate}} reports a second", { hz: n(w.hz), rate: n(w.measured) }) });
    else if (w.hz) lines.push({ ok: null, text: t("Set to {{hz}} Hz and kept; not counted, the mouse wasn't moved", { hz: n(w.hz) }) });
  }
  for (const w of writes.filter((x) => !writeHeld(x))) lines.push({ ok: false, text: writeFailed(w, voice) });
  if (c.left) {
    const at = c.left.slice(0, 6).join(", ");
    lines.push(
      c.left.length
        ? { ok: false, text: c.left.length > 6 ? t("After the check, its memory isn't as it was at {{at}} and more", { at }) : t("After the check, its memory isn't as it was at {{at}}", { at }) }
        : { ok: true, text: t("After the check, its memory is as it was") },
    );
  }
  if (c.wroteBack) {
    lines.push(c.wroteBack.ok ? { ok: true, text: t("hrtz then wrote its memory back as it was before the check") } : { ok: false, text: t("Writing its memory back failed: {{error}}", { error: c.wroteBack.error }) });
  }
  return lines;
}

/** A change the check tried held: kept, put back, and where it was a polling rate that was
 *  counted, counted at that rate. */
const writeHeld = (w: WriteCheck) => !w.error && w.kept && w.restored && (!w.hz || !w.measured || rateNear(w.measured) === w.hz);

function writeFailed(w: WriteCheck, { t, n, change }: Voice): string {
  const { what, from, to } = change(w);
  const changed = w.from || w.to ? t("{{what}} {{from}} → {{to}}", { what, from, to }) : t("Change");
  if (w.error) return t("{{change}} failed: {{error}}", { change: changed, error: w.error });
  if (!w.kept) return t("{{change}}: not kept", { change: changed });
  if (!w.restored) return t("{{change}}: kept, not put back", { change: changed });
  return t("{{change}}: kept, but it still counted {{rate}} reports a second", { change: changed, rate: n(w.measured ?? 0) });
}

/** What its owner said of the readings, a line each; none for a part they weren't asked about. */
function confirmedLines(p: Part, { t }: Voice): Line[] {
  if (!asks(p)) return [];
  const lines: Line[] = [];
  const stages = readingOf(p, "stages");
  const picked = pickedStages(p);
  if (picked) lines.push({ ok: true, text: t("DPI stages {{stages}}, as its owner says", { stages: picked.text }) });
  else if (p.confirmed?.stages === "none") lines.push({ ok: null, text: p.confirmed.stagesAre ? t("DPI stages: none of hrtz's readings (its own app shows {{stages}})", { stages: p.confirmed.stagesAre }) : t("DPI stages: none of hrtz's readings") });
  else if (stages?.ways.length) lines.push({ ok: null, text: t("DPI stages read as {{stages}}, not confirmed", { stages: stages.ways[0].text }) });
  if (shownOf(p).length) {
    const c = p.confirmed;
    lines.push(
      c?.rest === "right"
        ? { ok: true, text: t("Its other settings read as its own app shows them") }
        : c?.rest === "wrong"
          ? { ok: null, text: c.note ? t("Its other settings don't all read right: {{note}}", { note: c.note }) : t("Its other settings don't all read right") }
          : { ok: null, text: t("Its other settings read, not confirmed") },
    );
  }
  const more = [p.confirmed?.topDpi && t("up to {{dpi}} DPI", { dpi: p.confirmed.topDpi }), p.confirmed?.sensor && t("sensor {{sensor}}", { sensor: p.confirmed.sensor })].filter(Boolean);
  if (more.length) lines.push({ ok: null, text: t("Its owner says: {{facts}}", { facts: more.join(", ") }) });
  return lines;
}

function triedLine(tried: Tried, { t, n }: Voice): Line {
  if (tried.error) return { ok: false, text: t("A change wasn't tried: {{error}}", { error: tried.error }) };
  const counted =
    tried.measured === null ? t("not measured") : rateNear(tried.measured) === tried.to ? t("measured {{rate}}", { rate: n(tried.measured) }) : t("but still {{rate}} reports a second", { rate: n(tried.measured) });
  return {
    ok: triedHeld(tried) ? true : tried.kept && tried.restored && tried.measured === null ? null : false,
    text: t("{{what}} {{from}} → {{to}} Hz: {{result}}", {
      what: t(tried.what),
      from: n(tried.from),
      to: n(tried.to),
      result: [tried.kept ? t("kept") : t("not kept"), counted, tried.restored ? t("put back") : t("not put back")].join(", "),
    }),
  };
}

/** What a part's check found, a line each. */
export function linesOf(p: Part, voice: Voice): Line[] {
  const { t, n, d } = voice;
  const tried = p.tried ? [triedLine(p.tried, voice)] : [];
  if (p.how === "check" && p.checks) return [...summaryLines(p.checks, voice), ...confirmedLines(p, voice), ...tried];
  if (p.how === "windows") {
    const interfaces = (p.data.inspection as { interfaces?: unknown[] } | null)?.interfaces?.length ?? 0;
    const holds = p.read?.answeredAs;
    // Noted, not ticked: none of it says the mouse can be added.
    return [
      { ok: null, text: t("IDs {{ids}}", { ids: p.ids }) },
      { ok: null, text: interfaces ? t("{{count}} interfaces, with what each reports", { count: interfaces, defaultValue_one: "{{count}} interface, with what each reports" }) : t("No interfaces read") },
      ...(holds ? [{ ok: null, text: t("Its receiver answers for maker {{maker}}, model {{model}}", { maker: holds.cid, model: holds.mid }) }] : []),
    ];
  }
  const read = p.read;
  if (!read) return [];
  const who = read.model ?? read.answeredAs;
  const [got, wanted] = [read.memorySize ?? 0, read.memoryWanted ?? 0];
  const lines: Line[] = [
    who
      ? { ok: true, text: read.knownAs != null ? t("Answers as {{model}}", { model: read.knownAs }) : t("Answers as maker {{maker}}, model {{model}}", { maker: who.cid, model: who.mid }) }
      : { ok: false, text: t("It didn't say which mouse it is") },
    read.firmware ? { ok: true, text: t("Firmware {{firmware}}", { firmware: read.firmware }) } : { ok: null, text: t("Firmware not given") },
    whole(p)
      ? { ok: true, text: read.listening ? t("Memory read ({{bytes}} bytes), once told an app was listening", { bytes: n(got) }) : t("Memory read ({{bytes}} bytes)", { bytes: n(got) }) }
      : { ok: false, text: t("Memory read in part ({{bytes}} of {{total}} bytes)", { bytes: n(got), total: n(wanted) }) },
  ];
  const pic = p.picture;
  if (pic) {
    lines.push(
      pic.kept * 3 >= pic.of * 2
        ? { ok: true, text: t("{{kept}} of {{total}} settings kept where hrtz knows them", { kept: d(pic.kept), total: d(pic.of) }) }
        : { ok: null, text: t("Its settings are kept another way: {{kept}} of {{total}} where hrtz looks", { kept: d(pic.kept), total: d(pic.of) }) },
    );
  }
  const { set, measured } = pollingOf(p);
  if (set && measured)
    lines.push(
      pollingAgrees(p)
        ? { ok: true, text: t("Polling rate {{hz}} Hz, measured {{rate}}", { hz: n(set), rate: n(measured) }) }
        : { ok: null, text: t("Polling rate reads {{hz}} Hz, but {{rate}} reports a second measured", { hz: n(set), rate: n(measured) }) },
    );
  else if (set) lines.push({ ok: null, text: t("Polling rate reads {{hz}} Hz, not measured", { hz: n(set) }) });
  else if (measured) lines.push({ ok: null, text: t("{{rate}} reports a second measured; its rate isn't where hrtz looks", { rate: n(measured) }) });
  return [...lines, ...confirmedLines(p, voice), ...tried];
}

/** What a part comes to. */
export function verdictOf(p: Part, voice: Voice): Verdict {
  const { t } = voice;
  if (p.how === "windows") {
    return p.silent
      ? { tone: "warn", title: t("The mouse didn't answer"), detail: t("This holds only what Windows knows. Wake the mouse and check again for a full report.") }
      : { tone: "plain", title: t("Only what Windows knows"), detail: t("hrtz can't read this mouse yet. Its IDs and how it connects still tell us someone has one, and where to start.") };
  }
  if (p.known) {
    return linesOf(p, voice).some((l) => l.ok === false)
      ? { tone: "warn", title: t("Some things didn't work"), detail: t("The report shows which, and that's what we need to fix them.") }
      : {
          tone: "good",
          title: t("Everything checked works"),
          detail:
            (p.checks?.writes?.length ?? 0) > 1
              ? t("hrtz read the mouse, measured it and changed each setting it offers, and all of it held.")
              : t("hrtz read the mouse, measured it and changed a setting, and all of it held."),
        };
  }
  if (!whole(p)) {
    return p.read?.fellAsleep
      ? { tone: "warn", title: t("The mouse fell asleep partway"), detail: t("Keep it moving while it's read, and check again.") }
      : p.how === "check"
        ? { tone: "warn", title: t("Its memory couldn't be read"), detail: t("Check again with the mouse awake and near its receiver.") }
        : { tone: "warn", title: t("Only part of its memory was read"), detail: t("Check again with the mouse awake and near its receiver.") };
  }
  if (p.tried && !triedHeld(p.tried) && !(p.tried.kept && p.tried.restored && p.tried.measured === null)) {
    return { tone: "warn", title: t("The change tried didn't hold"), detail: t("The report shows what happened, and that's what we need to look into it.") };
  }
  if (p.picture && p.picture.kept * 2 < p.picture.of) {
    return { tone: "plain", title: t("Its settings are kept another way"), detail: t("hrtz read all of its memory, but little of it is where hrtz looks. The report shows us where to start.") };
  }
  // What hrtz doesn't read right yet, and what's still open, said plainly: each is something
  // we'd otherwise have to ask about later.
  const c = p.confirmed;
  const stagesAsked = asks(p) && (readingOf(p, "stages")?.ways.length ?? 0) > 0;
  const restAsked = asks(p) && shownOf(p).length > 0;
  const { set, measured } = pollingOf(p);
  const counted = !!set && !!measured;
  const named = (xs: (string | false)[]) => xs.filter((x): x is string => !!x);
  /** "its DPI stages, its other settings or its polling rate". */
  const either = (xs: string[]) => (xs.length > 1 ? t("{{list}} or {{last}}", { list: xs.slice(0, -1).join(", "), last: xs[xs.length - 1] }) : xs[0]);
  const wrong = named([stagesAsked && c?.stages === "none" && t("its DPI stages"), restAsked && c?.rest === "wrong" && t("its other settings"), counted && !pollingAgrees(p) && t("its polling rate")]);
  const open = named([
    stagesAsked && !pickedStages(p) && c?.stages !== "none" && t("its DPI stages"),
    restAsked && (c?.rest ?? "unsure") === "unsure" && t("its other settings"),
    !counted && t("its polling rate"),
  ]);
  const changed = !p.tried || p.tried.error ? t("Nothing was changed.") : triedHeld(p.tried) ? t("A change held, and was put back.") : t("A change was tried and put back, but not measured.");
  if (!wrong.length && !open.length) return { tone: "good", title: t("This has what we need to add it"), detail: [t("hrtz read the mouse, and what it read is confirmed."), changed].join(" ") };
  const detail = [
    wrong.length ? t("hrtz read the mouse, but doesn't read {{settings}} right yet. The report shows us how.", { settings: either(wrong) }) : t("hrtz read the mouse."),
    open.length > 0 && t("Not confirmed yet: {{settings}}.", { settings: open.join(", ") }),
    changed,
  ]
    .filter(Boolean)
    .join(" ");
  return { tone: "good", title: t("This has what we need to start adding it"), detail };
}

/**
 * A mouse hrtz runs, checked: what hrtz knows of it (its model, firmware, capabilities and
 * settings), what the check found, and its memory and its family's facts. Nothing that tells this
 * mouse or this PC apart: the key is the model's (a Logitech's unit ID goes), and the port isn't
 * kept. `known` is whether hrtz knows the model, so the check tried a change.
 */
export function checkedPart(mouse: Inspected, device: DeviceState, known: boolean, checks: Checks, dump: unknown): Part {
  return {
    ...blank,
    id: idOf(mouse),
    name: device.model ?? mouse.name,
    ids: idsOf(mouse),
    how: "check",
    known,
    key: device.key ?? null,
    variant: device.variant,
    link: linkName(device),
    wired: device.capabilities?.wired ?? null,
    called: known ? (device.model ?? null) : null,
    listed: known,
    dpiOpen: known && familyOf(device.key) === "compx" && !device.capabilities?.dpi,
    checks,
    // Its family's reading of its memory, a piece at a time.
    picture: (dump as { family?: { picture?: Picture | null } | null } | null)?.family?.picture ?? null,
    data: {
      mouse: {
        model: device.model,
        modelKey: device.key ? modelKeyOf(device.key) : null,
        family: familyOf(device.key) ?? null,
        firmware: device.firmware,
        variant: device.variant ?? null,
        link: linkName(device),
        cid: device.cid,
        mid: device.mid,
        deviceType: device.deviceType,
      },
      capabilities: device.capabilities,
      settings: device.settings,
      battery: device.battery,
      hostMode: device.hostMode ?? false,
      checks,
      device: dump,
      inspection: mouse.device,
    },
  };
}

/** A mouse hrtz leaves alone, read: who it answers as, its firmware, its memory as far as it gave
 *  it, and its polling rate as measured. */
export function readPart(mouse: Inspected, read: MouseRead, measured: Measured | null): Part {
  return {
    ...blank,
    id: idOf(mouse),
    name: mouse.name,
    ids: idsOf(mouse),
    how: "read",
    link: read.answeredAs ? (LINK_NAME[read.answeredAs.deviceType] ?? null) : read.wired ? msg("USB cable") : msg("receiver"),
    wired: read.wired,
    called: read.knownAs ?? null,
    listed: !!read.knownAs,
    connection: mouse.connection,
    read,
    picture: read.picture ?? null,
    measured,
    data: { read, inspection: mouse.device },
  };
}

/** A mouse hrtz can't read: what it tells Windows. `silent`: hrtz could have read it, but the
 *  mouse never answered; `receiver` is then what its receiver said meanwhile, where it did. */
export function windowsPart(mouse: Inspected, silent = false, receiver: MouseRead | null = null): Part {
  return {
    ...blank,
    id: idOf(mouse),
    name: mouse.name,
    ids: idsOf(mouse),
    how: "windows",
    read: receiver,
    silent,
    data: { ...(receiver && { read: receiver }), inspection: mouse.device },
  };
}

/** What to call a part's connection: "USB cable · 3554:F511", in its `Voice`'s language. */
export const partLabel = (p: Part, { t }: Voice) => [p.link && t(p.link), p.ids].filter(Boolean).join(" · ");

/** The report sent as a test of a model hrtz knows, rather than a request to add one. */
export const isTest = (parts: Part[]) => parts.some((p) => p.known);

/** The mouse's name for the report: its owner's, else one hrtz can offer. */
export const calledOf = (parts: Part[], typed: string) => typed.trim() || parts.find((p) => p.called)?.called || "";

async function versions() {
  const [hrtz, windows] = await Promise.all([getVersion().catch(() => ""), api.windowsVersion().catch(() => "")]);
  return { hrtz, windows };
}

/**
 * The file a tester sends: one report for the mouse, with a part for each connection it was
 * checked on, so a mouse checked on its cable and through its receiver is still one file and one
 * issue. `pnpm dev:sim report:<file>` makes a simulated mouse of it.
 */
export async function buildReport(parts: Part[], called: string) {
  return {
    format: "hrtz-mouse-report/2",
    created: new Date().toISOString(),
    ...(await versions()),
    // As its owner calls it (or the model hrtz knows): a receiver's own name rarely says.
    mouse: called || null,
    connections: parts.map((p) => ({
      name: p.name,
      ids: p.ids,
      how: p.how,
      link: p.link,
      verdict: verdictOf(p, ENGLISH).title,
      summary: linesOf(p, ENGLISH),
      ...(p.measured && { measured: p.measured }),
      ...(p.confirmed && { confirmed: p.confirmed }),
      ...(p.tried && { tried: p.tried }),
      ...p.data,
    })),
  };
}

type Json = Record<string, unknown>;
const without = (o: unknown, ...keys: string[]): Json => Object.fromEntries(Object.entries((o ?? {}) as Json).filter(([k]) => !keys.includes(k)));
const each = (report: Json, f: (c: Json) => Json): Json => ({ ...report, connections: (report.connections as Json[]).map(f) });

/**
 * A report made smaller for an issue's text, a step at a time until it fits the link:
 * 1. what hrtz can work out again from the rest (the lines, each memory's reading a piece at a
 *    time, a known model's capabilities and decoded settings);
 * 2. then the report descriptors Windows gave;
 * 3. then each question as sent and answered, and the rest of what Windows knows.
 * The memory, who the mouse is, what was measured, tried and confirmed are kept to the last.
 */
const SMALLER: ((report: Json) => Json)[] = [
  (r) =>
    each(r, (c) => ({
      ...without(c, "summary", "capabilities", "settings"),
      ...(c.read ? { read: without(c.read, "picture") } : {}),
      ...(c.device ? { device: { ...(c.device as Json), family: without((c.device as Json).family, "picture") } } : {}),
    })),
  (r) =>
    each(r, (c) => {
      const inspection = c.inspection as Json | null | undefined;
      if (!inspection) return c;
      return { ...c, inspection: { ...inspection, interfaces: ((inspection.interfaces as Json[] | undefined) ?? []).map((i) => without(i, "reportDescriptor")) } };
    }),
  (r) => each(r, (c) => ({ ...without(c, "inspection"), ...(c.read ? { read: without(c.read, "asked") } : {}) })),
];

/** A link GitHub still opens. Tried signed out, 2026-11-14: it took links of 6,575 characters and
 *  failed at 7,075 (a server error; "address too long" from 8,375), and one of 4,514 came through
 *  its sign-in page whole. A report with both connections makes a link of about 3,500. */
const LINK_MAX = 5000;
/** The most the report's few lines take of it: what its owner typed is in them. */
const SUMMARY_MAX = 2000;

/** The report in a few lines, for the GitHub issue. */
async function summaryText(parts: Part[]) {
  const { hrtz, windows } = await versions();
  const mark = (ok: boolean | null) => (ok === null ? "-" : ok ? "OK" : "FAILED");
  const blocks = parts.map((p) => [`${p.name} (${partLabel(p, ENGLISH)}): ${verdictOf(p, ENGLISH).title}`, ...linesOf(p, ENGLISH).map((l) => `${mark(l.ok)}  ${l.text}`)].join("\n"));
  const text = [`hrtz ${hrtz} · ${windows}`, ...blocks].join("\n\n");
  return text.length > SUMMARY_MAX ? `${text.slice(0, SUMMARY_MAX)}…` : text;
}

/**
 * The GitHub issue for a report: a test report for a model hrtz knows, a request to add one it
 * doesn't. Its text is the report in a few lines, then the report itself, packed (`pack`), so the
 * issue holds what the file does and nothing has to be dragged in: both real reports before this
 * lost their file on the way (sent on Discord; the wrong file in the box). `whole` is false where
 * the report had to lose something to fit, or didn't fit at all: the file is then worth adding.
 */
export async function issueOf(parts: Part[], called: string): Promise<{ url: string; whole: boolean }> {
  const form = isTest(parts) ? "mouse_report" : "mouse_request";
  const summary = await summaryText(parts);
  let report = (await buildReport(parts, called)) as Json;
  for (const [i, smaller] of SMALLER.entries()) {
    report = smaller(report);
    const text = `${summary}\n\nThe report, for hrtz to read. Leave this line as it is:\n\`${PACKED}${await pack(report)}\``;
    const url = mouseIssueUrl(form, called, text);
    if (url.length <= LINK_MAX) return { url, whole: i === 0 };
  }
  // Not even the least of it fits: the lines alone, cut to what does.
  const lines = (cut: number) => mouseIssueUrl(form, called, `${summary.slice(0, summary.length - cut)}\n\nDrag the report file here:`);
  const over = lines(0).length - LINK_MAX;
  return { url: lines(Math.max(0, over)), whole: false };
}

/** The file's name: the mouse and the day. */
export const reportName = (called: string) =>
  `hrtz-report-${called.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "mouse"}-${new Date().toISOString().slice(0, 10)}.json`;
