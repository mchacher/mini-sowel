// The box serves the interface from its flash: the first load (index.html and every script and
// style it references) must stay under the budget, gzipped. Run after `npm run build`.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { gzipSync } from "node:zlib";

const BUDGET_BYTES = 170 * 1024;
const dist = new URL("../dist/", import.meta.url).pathname;

const html = readFileSync(join(dist, "index.html"), "utf8");
const assets = [...html.matchAll(/(?:src|href)="\/([^"]+\.(?:js|css))"/g)].map((m) => m[1]);
const files = ["index.html", ...new Set(assets)];

let total = 0;
for (const file of files) {
  const size = gzipSync(readFileSync(join(dist, file)), { level: 9 }).length;
  total += size;
  console.log(`${(size / 1024).toFixed(1).padStart(7)} KB  ${file}`);
}
const kb = (total / 1024).toFixed(1);
if (total > BUDGET_BYTES) {
  console.error(`First load ${kb} KB gzip: over the budget of ${BUDGET_BYTES / 1024} KB.`);
  process.exit(1);
}
console.log(`First load ${kb} KB gzip, budget ${BUDGET_BYTES / 1024} KB: OK.`);
