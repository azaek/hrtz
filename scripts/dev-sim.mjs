// A dev build with simulated mice beside the real ones, from any shell: `pnpm dev:sim` (a Z2 mini),
// or `pnpm dev:sim z2mini,z1promax,superlight,superlight2`. Their keys start `sim/`, so they never
// take over what a real mouse keeps (src-tauri/src/mice/compx/sim.rs, mice/hidpp/sim.rs).
//
// `pnpm dev:sim report:C:/path/report.json` is a tester's mouse, from the report Inspect a mouse
// saved of it. `pnpm dev:sim alone` has no simulated mouse: it leaves the real Zaopin and ATK mice
// alone instead, so Inspect a mouse reads them the way it reads a mouse hrtz doesn't run.
import { spawnSync } from "node:child_process";

const mice = process.argv[2] ?? "z2mini";
const alone = mice.split(",").includes("alone");
const env = { ...process.env, HRTZ_SIM: mice, ...(alone && { HRTZ_LEAVE_ALONE: "1" }) };
const run = spawnSync("pnpm", ["tauri", "dev"], { stdio: "inherit", shell: true, env });
process.exit(run.status ?? 1);
