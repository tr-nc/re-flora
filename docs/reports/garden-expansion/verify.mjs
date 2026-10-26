// Offline report integrity check. Uses only Node built-ins; no original worktree required.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const base = fileURLToPath(new URL('.', import.meta.url));
const html = fs.readFileSync(path.join(base, 'index.html'), 'utf8');
const manifest = JSON.parse(fs.readFileSync(path.join(base, 'assets/manifest.json'), 'utf8'));
const ids = new Set([...html.matchAll(/\bid="([^"]+)"/g)].map(m => m[1]));
assert.equal(manifest.length, 25);
assert(!/<script\b|@import|url\(/i.test(html), 'report must not need scripts or external CSS assets');
const imageSources = new Set([...html.matchAll(/<img[^>]*\bsrc="([^"]+)"/g)].map(m => m[1]));
for (const [, attribute, link] of html.matchAll(/\b(src|href)="([^"]+)"/g)) {
  if (/^https?:/.test(link)) {
    assert.equal(attribute, 'href', 'remote content must be an optional navigation only');
    continue;
  }
  const [file, fragment] = link.split('#');
  if (!file) {
    assert(ids.has(fragment), `missing anchor ${link}`);
    continue;
  }
  const resolved = path.resolve(base, decodeURIComponent(file));
  assert(resolved.startsWith(base), `nonportable dependency ${link}`);
  assert(fs.statSync(resolved).isFile(), `missing local resource ${link}`);
}
for (const entry of manifest) {
  const b = fs.readFileSync(path.join(base, 'assets', entry.file));
  assert.equal(b.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
  assert.equal(b.readUInt32BE(16), entry.width);
  assert.equal(b.readUInt32BE(20), entry.height);
  assert.equal(b[24], entry.png_bit_depth);
  assert.equal(b[25], entry.png_color_type);
  assert.equal(b.length, entry.bytes);
  assert.equal(createHash('sha256').update(b).digest('hex'), entry.sha256);
  assert(imageSources.delete(`assets/${entry.file}`), `unused manifest image ${entry.file}`);
}
assert.equal(imageSources.size, 0, 'untracked screenshot');
const perf = JSON.parse(fs.readFileSync(path.join(base, 'evidence/performance.json'), 'utf8'));
for (const group of [perf.flowers, perf.bees]) {
  for (const m of Object.values(group)) {
    const values = m.samples.map(v => v.ms).sort((a, b) => a - b);
    assert.equal(values.length, m.sample_count);
    assert.equal(new Set(m.samples.map(v => v.frame)).size, values.length);
    assert.equal((values[Math.floor((values.length - 1) / 2)] + values[Math.ceil((values.length - 1) / 2)]) / 2, m.median_ms);
    assert.equal(values[Math.ceil(values.length * .95) - 1], m.p95_nearest_rank_ms);
    assert.equal(values.at(-1), m.max_ms);
  }
}
console.log(`Verified ${manifest.length} original PNGs, all local HTML references and four performance sample sets. No worktree, network or temporary files required.`);
