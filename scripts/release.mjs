// Cuts a release:
//
//   pnpm release 0.2.0
//
// 1. CHANGELOG.md's "Unreleased" heading becomes `## 0.2.0 — <today>`: a version is dated on the
//    day it's released.
// 2. The version goes into package.json, src-tauri/Cargo.toml, src-tauri/Cargo.lock and
//    src-tauri/tauri.conf.json.
// 3. A commit "Release 0.2.0" and the tag v0.2.0, pushed with main.
//
// The tag runs .github/workflows/release.yml: the checks, then a signed installer in a draft
// GitHub release. Publishing that draft is what ships it to installed copies.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

const fail = (why) => {
  console.error(why);
  process.exit(1);
};
const root = new URL("../", import.meta.url);
const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
const read = (path) => readFileSync(new URL(path, root), "utf8");
const write = (path, text) => writeFileSync(new URL(path, root), text);

const version = (process.argv[2] ?? "").replace(/^v/, "");
if (!/^\d+\.\d+\.\d+$/.test(version)) fail("usage: pnpm release <X.Y.Z>");

if (git("status", "--porcelain")) fail("Commit or stash your changes first.");
if (git("rev-parse", "--abbrev-ref", "HEAD") !== "main") fail("Releases are cut from main.");
if (git("tag", "--list", `v${version}`)) fail(`v${version} exists already.`);

// Never backwards. The same version is fine once: 0.1.0 is both the start and the first release.
const current = JSON.parse(read("package.json")).version;
const parts = (v) => v.split(".").map(Number);
const cmp = (a, b) => parts(a).reduce((d, x, i) => d || x - parts(b)[i], 0);
if (cmp(version, current) < 0) fail(`${version} is older than ${current}.`);

// The release day, in local time.
const now = new Date();
const today = [now.getFullYear(), now.getMonth() + 1, now.getDate()].map((n) => String(n).padStart(2, "0")).join("-");

/** Replaces the one match of `pattern` in `path`, or stops. */
function stamp(path, pattern, to) {
  const text = read(path);
  const found = text.match(pattern);
  if (!found) fail(`${path}: no ${pattern} to update.`);
  write(path, text.replace(pattern, to));
}

stamp("CHANGELOG.md", /^## (?:\S+ [—–-] )?Unreleased$/m, `## ${version} — ${today}`);
stamp("package.json", /"version": "[^"]+"/, `"version": "${version}"`);
stamp("src-tauri/tauri.conf.json", /"version": "[^"]+"/, `"version": "${version}"`);
stamp("src-tauri/Cargo.toml", /^version = "[^"]+"/m, `version = "${version}"`);
stamp("src-tauri/Cargo.lock", /(name = "hrtz"\r?\nversion = )"[^"]+"/, `$1"${version}"`);

git("add", "CHANGELOG.md", "package.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock");
git("commit", "-m", `Release ${version}`);
git("tag", `v${version}`);
git("push", "--atomic", "origin", "main", `v${version}`);
console.log(`Released ${version} (${today}). The Release workflow builds a draft; publish it on GitHub to ship.`);
