// The GitHub release's text for a version, from CHANGELOG.md: the same notes the app's What's new
// shows. Fails when the version has no entry, or its heading isn't dated yet ("Unreleased"):
// `pnpm release` dates it on the release day, before the tag exists.
//
//   node scripts/release-notes.mjs 0.2.0
import { readFileSync } from "node:fs";

const fail = (why) => {
  console.error(why);
  process.exit(1);
};

const version = (process.argv[2] ?? "").replace(/^v/, "");
if (!/^\d+\.\d+\.\d+$/.test(version)) fail("usage: release-notes.mjs <X.Y.Z>");

// Read as the app reads it (src/changelog.ts): `## version — date`, `### Section`, `- item`, and an
// indented line continuing the item above.
const lines = readFileSync(new URL("../CHANGELOG.md", import.meta.url), "utf8").split(/\r?\n/);
const heading = (line) => line.match(/^##\s+(.+?)\s+[—–-]\s+(.+)$/);
const at = lines.findIndex((l) => heading(l)?.[1] === version);
if (at < 0) fail(`CHANGELOG.md has no entry for ${version}.`);
const date = heading(lines[at])[2].trim();
if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) fail(`CHANGELOG.md's ${version} reads "${date}". Cut releases with: pnpm release ${version}`);

const out = [];
for (const line of lines.slice(at + 1)) {
  if (/^##\s/.test(line)) break;
  const section = line.match(/^###\s+(.+)$/);
  if (section) out.push("", `**${section[1].trim()}**`);
  else if (/^[-*]\s+/.test(line)) out.push(line.replace(/^\*\s+/, "- "));
  else if (/^\s{2,}\S/.test(line) && out.length) out[out.length - 1] += ` ${line.trim()}`;
}
const body = out.join("\n").trim();
process.stdout.write((body || `hrtz ${version}.`) + "\n");
