import { useEffect, useRef } from "react";
import { api, on, type MeterSample } from "./api";

type Listener = (s: MeterSample) => void;

const listeners = new Set<Listener>();
let attached = false;
let rate = 1000;

/** Run the backend stream while any tool listens and the window is visible. */
function sync() {
  api.meter(listeners.size > 0 && !document.hidden, rate);
}

function attach() {
  if (attached) return;
  attached = true;
  on.meter((s) => listeners.forEach((l) => l(s)));
  document.addEventListener("visibilitychange", sync);
}

/**
 * Raw input ticks (every 100 ms) from the mouse while `active`. Several tools can listen at once
 * without turning each other off. `rateHz`, the polling rate stutters are measured against, comes
 * from the polling rate test; tools that don't count stutters leave it out.
 */
export function useRawInput(active: boolean, onSample: Listener, rateHz?: number) {
  const handler = useRef(onSample);
  handler.current = onSample;

  useEffect(() => {
    if (rateHz === undefined) return;
    rate = rateHz;
    if (listeners.size) sync();
  }, [rateHz]);

  useEffect(() => {
    if (!active) return;
    const l: Listener = (s) => handler.current(s);
    attach();
    listeners.add(l);
    sync();
    return () => {
      listeners.delete(l);
      sync();
    };
  }, [active]);
}
