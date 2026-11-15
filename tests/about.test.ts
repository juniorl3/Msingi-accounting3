import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { ABOUT, copyrightLine } from "../src/aboutInfo.ts";

assert.equal(
  copyrightLine(),
  "© 2026 GTN Global Network. All rights reserved."
);

const tauriFile = readFileSync("src-tauri/tauri.conf.json", "utf8");
const tauri = JSON.parse(tauriFile).bundle;

assert.equal(tauri.copyright, copyrightLine());
assert.equal(tauri.publisher, ABOUT.owner);

const pkg = JSON.parse(readFileSync("package.json", "utf8"));
assert.deepEqual(pkg.author, { name: ABOUT.owner });

const cargo = readFileSync("src-tauri/Cargo.toml", "utf8");
assert.match(
  cargo,
  /^authors\s*=\s*\["GTN Global Network"\]\s*$/m
);

const notice = readFileSync("COPYRIGHT.txt", "utf8");

assert.ok(notice.includes(ABOUT.owner));
assert.ok(notice.includes(ABOUT.address));

assert.equal(ABOUT.address, "Arusha, Tanzania");
assert.ok(!JSON.stringify(ABOUT).includes("Eloi"));
assert.ok(tauriFile.includes("Arusha, Tanzania"));

// Check actual public ownership information, not unrelated
// configuration entries such as icons/128x128@2x.png.

const publicInfo = [
  JSON.stringify(ABOUT),
  JSON.stringify(pkg.author),
  notice,
  tauri.copyright,
  tauri.publisher,
  ABOUT.address,
  'GTN Global Network'
].join("\n");

assert.doesNotMatch(
  publicInfo,
  /[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/i
);

assert.doesNotMatch(
  publicInfo,
  /\+\d[\d\s()-]{7,}\d/
);

assert.ok(!("email" in ABOUT));
assert.ok(!("phone" in ABOUT));

console.log("about tests passed");
