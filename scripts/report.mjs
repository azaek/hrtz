// Reads a mouse report back out of a GitHub issue's text, where Inspect a mouse packs it
// (src/cards/participate/pack.ts), and writes it as the file hrtz would have saved:
//
//   pnpm report issue.txt [report.json]
//
// `issue.txt` is the issue's text, or just its packed line. Then `pnpm dev:sim report:report.json`
// runs a simulated copy of the tester's mouse, to work on what hrtz makes of it without going back
// to them. A packed report leaves out what hrtz works out again from the rest (its memory's
// reading a piece at a time), so the file has a little less than one saved in the app.
import { readFileSync, writeFileSync } from "node:fs";
import { inflateRawSync } from "node:zlib";

const [from, to = "report.json"] = process.argv.slice(2);
if (!from) {
  console.error("usage: pnpm report <file with the issue's text> [report.json]");
  process.exit(1);
}
const packed = readFileSync(from, "utf8").match(/hrtz-report\/2:([A-Za-z0-9_-]+)/)?.[1];
if (!packed) {
  console.error("No packed report in that text: look for a line starting hrtz-report/2:");
  process.exit(1);
}
const report = JSON.parse(inflateRawSync(Buffer.from(packed, "base64url")).toString("utf8"));
writeFileSync(to, `${JSON.stringify(report, null, 2)}\n`);
const parts = (report.connections ?? []).map((c) => `${c.link ?? c.how} ${c.ids}`).join(", ");
console.log(`${to}: ${report.mouse ?? "a mouse"} (${parts}), from hrtz ${report.hrtz}`);
