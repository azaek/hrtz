import { useEffect } from "react";

/** A test's latest result in a few words, for the Lab page's list: "1,619 DPI" and "1.2% above
 *  the set 1,600". The tone says whether it found something worth fixing. */
export type TestResult = { value: string; detail: string; tone: "good" | "warn" | "neutral" };

/** A test runs while it's shown and tells the page its latest result, or null before a run has
 *  one (it just opened, or its runs were cleared). */
export type TestProps = { onResult?: (r: TestResult | null) => void };

/** Tells `onResult` about `r` when its words change. */
export function useReport(onResult: TestProps["onResult"], r: TestResult | null) {
  const key = r ? `${r.value}|${r.detail}|${r.tone}` : "";
  useEffect(() => onResult?.(r), [key]);
}
