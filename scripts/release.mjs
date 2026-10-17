// Stamps a release into the files, for the Release workflow (.github/workflows/release.yml), which
// runs it when a tag vX.Y.Z is pushed:
//
//   node scripts/release.mjs 0.2.0 2026-11-02
//
// 1. CHANGELOG.md's "Unreleased" heading becomes `## 0.2.0 — 2026-11-02`: a version is dated on
//    the day it's released.
// 2. The version goes into package.json, src-tauri/Cargo.toml, src-tauri/Cargo.lock and
//    src-tauri/tauri.conf.json, so the build is that version.
//
// The workflow builds from the stamped files, then commits them to main as "Release 0.2.0".
import { readFileSync, writeFileSync } from "node:fs";

const fail = (why) => {
  console.error(why);
  process.exit(1);
};
const root = new URL("../", import.meta.url);
const read = (path) => readFileSync(new URL(path, root), "utf8");
const write = (path, text) => writeFileSync(new URL(path, root), text);

const version = (process.argv[2] ?? "").replace(/^v/, "");
const date = process.argv[3] ?? "";
if (!/^\d+\.\d+\.\d+$/.test(version) || !/^\d{4}-\d{2}-\d{2}$/.test(date)) fail("usage: release.mjs <X.Y.Z> <YYYY-MM-DD>");

// Never backwards. The same version is fine once: 0.1.0 is both the start and the first release.
const current = JSON.parse(read("package.json")).version;
const parts = (v) => v.split(".").map(Number);
const cmp = (a, b) => parts(a).reduce((d, x, i) => d || x - parts(b)[i], 0);
if (cmp(version, current) < 0) fail(`${version} is older than ${current}, the version in package.json.`);

/** Replaces the one match of `pattern` in `path`, or stops with `why`. */
function stamp(path, pattern, to, why) {
  const text = read(path);
  if (!pattern.test(text)) fail(why ?? `${path}: nothing matches ${pattern}.`);
  write(path, text.replace(pattern, to));
}

stamp(
  "CHANGELOG.md",
  /^## (?:\S+ [—–-] )?Unreleased$/m,
  `## ${version} — ${date}`,
  'CHANGELOG.md has no "Unreleased" heading: add the release\'s notes under `## Unreleased` before tagging.',
);
stamp("package.json", /"version": "[^"]+"/, `"version": "${version}"`);
stamp("src-tauri/tauri.conf.json", /"version": "[^"]+"/, `"version": "${version}"`);
stamp("src-tauri/Cargo.toml", /^version = "[^"]+"/m, `version = "${version}"`);
stamp("src-tauri/Cargo.lock", /(name = "hrtz"\r?\nversion = )"[^"]+"/, `$1"${version}"`);
console.log(`Stamped ${version} (${date}).`);
