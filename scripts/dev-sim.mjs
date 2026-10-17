// A dev build with simulated mice beside the real ones, from any shell: `pnpm dev:sim` (a Z2 mini),
// or `pnpm dev:sim z2mini,z1promax,superlight,superlight2`. Their keys start `sim/`, so they never
// take over what a real mouse keeps (src-tauri/src/mice/compx/sim.rs, mice/hidpp/sim.rs).
import { spawnSync } from "node:child_process";

const mice = process.argv[2] ?? "z2mini";
const run = spawnSync("pnpm", ["tauri", "dev"], { stdio: "inherit", shell: true, env: { ...process.env, HRTZ_SIM: mice } });
process.exit(run.status ?? 1);
