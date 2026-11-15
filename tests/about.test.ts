import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { ABOUT, copyrightLine } from "../src/aboutInfo.ts";

assert.equal(copyrightLine(), "© 2026 GTN Global Network. All rights reserved.");
const tauri = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")).bundle;
assert.equal(tauri.copyright, copyrightLine());
assert.equal(tauri.publisher, ABOUT.owner);
const pkg = JSON.parse(readFileSync("package.json", "utf8"));
assert.deepEqual(pkg.author, { name: ABOUT.owner, email: ABOUT.email });
assert.ok(readFileSync("src-tauri/Cargo.toml", "utf8").includes(`${ABOUT.owner} <${ABOUT.email}>`));
const notice = readFileSync("COPYRIGHT.txt", "utf8");
for (const v of [ABOUT.owner, ABOUT.address, ABOUT.email, ABOUT.phone]) assert.ok(notice.includes(v), v);
assert.equal(ABOUT.address, "Arusha, Tanzania");
assert.ok(!JSON.stringify(ABOUT).includes("Eloi"));
assert.ok(readFileSync("src-tauri/tauri.conf.json", "utf8").includes("Arusha, Tanzania"));
assert.match(ABOUT.email, /^[^@\s]+@[^@\s]+\.[^@\s]+$/);
assert.match(ABOUT.phone, /^\+255 \d{3} \d{3} \d{3}$/);
console.log("about tests passed");
