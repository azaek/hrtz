// The portable copy's update manifest, latest-portable.json, put on each release beside the
// installer's latest.json. The portable copy reads it from the newest release
// (src-tauri/src/portable.rs): the version, its notes, and where its exe is with the exe's minisign
// signature, which the copy checks against the pubkey in tauri.conf.json before it uses the exe.
//
//   node scripts/portable-manifest.mjs 0.6.1 hrtz-portable.exe.sig > latest-portable.json
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const fail = (why) => {
  console.error(why);
  process.exit(1);
};

const [version = "", sigPath = ""] = process.argv.slice(2);
if (!/^\d+\.\d+\.\d+$/.test(version) || !sigPath) fail("usage: portable-manifest.mjs <X.Y.Z> <exe.sig>");

const repo = process.env.GITHUB_REPOSITORY || "azaek/hrtz";
const notes = execFileSync(process.execPath, [fileURLToPath(new URL("release-notes.mjs", import.meta.url)), version], {
  encoding: "utf8",
}).trim();

const manifest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    // TARGET in portable.rs: a name the installer's latest.json never has.
    "windows-x86_64-portable": {
      signature: readFileSync(sigPath, "utf8").trim(),
      url: `https://github.com/${repo}/releases/download/v${version}/hrtz-portable.exe`,
    },
  },
};
process.stdout.write(JSON.stringify(manifest, null, 2) + "\n");
