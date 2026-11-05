// Gzip every file in a build for the firmware to embed (firmware-rs/build.rs serves
// each .gz at its path without the .gz), and drop the originals.
import { readdirSync, readFileSync, statSync, unlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { gzipSync } from 'node:zlib';

const dir = process.argv[2];
let total = 0;
function walk(d) {
  for (const name of readdirSync(d)) {
    const p = join(d, name);
    if (statSync(p).isDirectory()) walk(p);
    else if (!p.endsWith('.gz')) {
      const gz = gzipSync(readFileSync(p), { level: 9 });
      writeFileSync(`${p}.gz`, gz);
      unlinkSync(p);
      total += gz.length;
    }
  }
}
walk(dir);
console.log(`${dir}: ${(total / 1024).toFixed(1)} KB gzipped`);
