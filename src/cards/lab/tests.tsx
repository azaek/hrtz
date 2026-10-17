import { useEffect, useState, type ReactElement } from "react";
import { Switch } from "@fluentui/react-components";
import { ArrowBidirectionalUpDown20Regular, ArrowStepOver20Regular, CursorClick20Regular, DataLine20Regular, Map20Regular, Ruler20Regular, TopSpeed20Regular } from "@fluentui/react-icons";
import { api, currentDpi, type PadCalibration } from "@/api";
import { savePrefs, useStore } from "@/store";
import { ClickCheck } from "./ClickCheck";
import { ConnectionCheck } from "./ConnectionCheck";
import { DpiCheck } from "./DpiCheck";
import { LiftOffCheck } from "./LiftOffCheck";
import { PadUsage, padResult } from "./PadUsage";
import type { TestResult } from "./result";
import { ScrollCheck } from "./ScrollCheck";
import { TrackingCheck } from "./TrackingCheck";

// The Lab page's tests, in list order, and the last result of each, remembered on this PC so the
// list can say how things stood when last checked.

export type TestId = "connection" | "dpi" | "tracking" | "liftoff" | "clicks" | "wheel" | "pad";

export type Test = {
  id: TestId;
  /** In the list. */
  name: string;
  /** Over the open test, where there's room for more. */
  title: string;
  icon: ReactElement;
  /** One line under the title. */
  description: string;
  /** Said under the open test. */
  hint: string;
};

export const TESTS: Test[] = [
  {
    id: "connection",
    name: "Connection",
    title: "Connection",
    icon: <DataLine20Regular />,
    description: "Checks that every report reaches your PC and finds the best polling rate for it.",
    hint: "Runs full screen. May change the polling rate, then restores yours.",
  },
  {
    id: "dpi",
    name: "DPI accuracy",
    title: "DPI accuracy",
    icon: <Ruler20Regular />,
    description: "Measures the DPI your sensor really delivers. You'll need a ruler.",
    hint: "Runs are counted while this test is open.",
  },
  {
    id: "tracking",
    name: "Tracking",
    title: "Tracking on your pad",
    icon: <TopSpeed20Regular />,
    description: "Checks that the sensor holds still and keeps count at speed on your pad.",
    hint: "Runs full screen and moves through each part automatically.",
  },
  {
    id: "liftoff",
    name: "Lift-off",
    title: "Lift-off on your pad",
    icon: <ArrowStepOver20Regular />,
    description: "Finds the lowest lift-off distance that tracks reliably on your pad.",
    hint: "Runs full screen. Changes lift-off, then restores yours.",
  },
  {
    id: "clicks",
    name: "Double clicks",
    title: "Double clicks",
    icon: <CursorClick20Regular />,
    description: "Catches clicks that register twice.",
    hint: "Use single clicks only.",
  },
  {
    id: "wheel",
    name: "Scroll wheel",
    title: "Scroll wheel",
    icon: <ArrowBidirectionalUpDown20Regular />,
    description: "Catches notches that scroll the wrong way or twice, signs of a dirty encoder.",
    hint: "Notches are counted while this test is open.",
  },
  {
    id: "pad",
    name: "Mousepad",
    title: "Mousepad use",
    icon: <Map20Regular />,
    description: "Maps where your mouse travels on the pad while you play.",
    hint: "With the switch on, it records in the background while you play.",
  },
];

export const testOf = (id: TestId) => TESTS.find((t) => t.id === id) ?? TESTS[0];

export type Kept = { result: TestResult; at: number };
const KEY = "hrtz.testResults";
const read = (): Partial<Record<TestId, Kept>> => {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "{}");
  } catch {
    return {};
  }
};

/** The last result of each test, and a way to keep a new one. A test that reports nothing yet
 *  (it just opened, or its runs were cleared) leaves the last result as it was. */
export function useResults() {
  const [kept, setKept] = useState(read);
  const keep = (id: TestId) => (r: TestResult | null) => {
    if (!r) return;
    const next = { ...read(), [id]: { result: r, at: Date.now() } };
    localStorage.setItem(KEY, JSON.stringify(next));
    setKept(next);
  };
  return { kept, keep };
}

/** "today", "yesterday", "14 Oct". */
export function whenText(at: number) {
  const day = (t: number) => new Date(new Date(t).toDateString()).getTime();
  const days = Math.round((day(Date.now()) - day(at)) / 86_400_000);
  if (days === 0) return "today";
  if (days === 1) return "yesterday";
  return new Date(at).toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

/** The mousepad as it stands (the area it maps, and whether it's recording), read every 2 s. It
 *  records in the background, so this is live rather than remembered. */
export function usePadStatus(): TestResult | null {
  const prefs = useStore((x) => x.prefs);
  const [cal, setCal] = useState<PadCalibration | null>(null);
  useEffect(() => {
    const read = () => api.padState(null).then((v) => setCal(v?.calibration ?? null), () => {});
    read();
    const t = setInterval(() => !document.hidden && read(), 2000);
    return () => clearInterval(t);
  }, []);
  return padResult(cal, prefs);
}

/** The double-click test's one setting: counting while you use the PC, on unless turned off. */
export function ClickSwitch() {
  const prefs = useStore((x) => x.prefs);
  return (
    <Switch
      checked={prefs?.clickHistory ?? true}
      disabled={!prefs}
      label="Count in the background"
      labelPosition="before"
      onChange={(_, d) => prefs && savePrefs({ ...prefs, clickHistory: d.checked })}
    />
  );
}

/** The mousepad's one setting: recording while you play. It needs a calibration first. */
export function PadSwitch({ calibrated }: { calibrated: boolean }) {
  const prefs = useStore((x) => x.prefs);
  return (
    <Switch
      checked={!!prefs?.padTracking}
      disabled={!prefs || !calibrated}
      aria-label="Record while you play"
      onChange={(_, d) => prefs && savePrefs({ ...prefs, padTracking: d.checked })}
    />
  );
}

/** A test's own body, running while it's shown. */
export function TestBody({ id, onResult }: { id: TestId; onResult: (r: TestResult | null) => void }) {
  const device = useStore((x) => x.device!);
  const dpi = currentDpi(device.settings) ?? 1600;
  switch (id) {
    case "connection":
      return <ConnectionCheck onResult={onResult} />;
    case "dpi":
      return <DpiCheck dpi={dpi} onResult={onResult} />;
    case "tracking":
      return <TrackingCheck onResult={onResult} />;
    case "liftoff":
      return <LiftOffCheck onResult={onResult} />;
    case "clicks":
      return <ClickCheck onResult={onResult} />;
    case "wheel":
      return <ScrollCheck onResult={onResult} />;
    case "pad":
      return <PadUsage dpi={dpi} onResult={onResult} />;
  }
}
